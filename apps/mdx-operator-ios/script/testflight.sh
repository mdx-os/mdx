#!/bin/sh
set -eu

root="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
workspace_root="$(CDPATH= cd -- "$root/../.." && pwd)"
output_root_input="${MDX_IOS_DISTRIBUTION_DIR:-$workspace_root/.mdx-local/ios-distribution}"
mkdir -p "$output_root_input"
output_root="$(CDPATH= cd -- "$output_root_input" && pwd)"
archive_path="$output_root/MDxAnywhere.xcarchive"
export_path="$output_root/export"
inspection_path="$output_root/exported-ipa"
build_number="${MDX_IOS_BUILD_NUMBER:-$(git -C "$workspace_root" rev-list --count HEAD)}"
marketing_version="${MDX_IOS_MARKETING_VERSION:-0.9.2}"
team="${MDX_APPLE_DEVELOPMENT_TEAM:-}"

: "${MDX_SUPABASE_URL:?MDX_SUPABASE_URL is required}"
: "${MDX_SUPABASE_PUBLISHABLE_KEY:?MDX_SUPABASE_PUBLISHABLE_KEY is required}"
: "${MDX_PUBLIC_HOST_URL:?MDX_PUBLIC_HOST_URL is required}"

if [ -z "$team" ]; then
  echo "testflight: MDX_APPLE_DEVELOPMENT_TEAM is required" >&2
  exit 1
fi
if ! printf '%s\n' "$marketing_version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
  echo "testflight: MDX_IOS_MARKETING_VERSION must be a semantic version" >&2
  exit 1
fi
case "$build_number" in
  *[!0-9]*|'') echo "testflight: MDX_IOS_BUILD_NUMBER must be numeric" >&2; exit 1 ;;
esac
if ! xcodebuild -version >/dev/null 2>&1; then
  echo "testflight: full Xcode is required" >&2
  exit 1
fi

sh "$root/script/generate_project.sh"

fail() {
  echo "testflight: ERROR $*" >&2
  exit 1
}

require_directory() {
  require_directory_path="$1"
  require_directory_label="$2"
  [ -d "$require_directory_path" ] || fail "$require_directory_label directory is missing path=$require_directory_path"
}

require_file() {
  require_file_path="$1"
  require_file_label="$2"
  [ -f "$require_file_path" ] || fail "$require_file_label file is missing path=$require_file_path"
}

plist_value() {
  plist_value_path="$1"
  plist_value_key="$2"
  plist_value_label="$3"
  plist_value_result="$(/usr/libexec/PlistBuddy -c "Print :$plist_value_key" "$plist_value_path" 2>/dev/null)" ||
    fail "$plist_value_label is missing plist=$plist_value_path key=$plist_value_key"
  printf '%s' "$plist_value_result"
}

require_equal() {
  require_equal_actual="$1"
  require_equal_expected="$2"
  require_equal_label="$3"
  [ "$require_equal_actual" = "$require_equal_expected" ] ||
    fail "$require_equal_label mismatch expected=$require_equal_expected actual=$require_equal_actual"
}

reset_artifact_path() {
  reset_path="$1"
  case "$reset_path" in
    "$output_root"/*) ;;
    *) fail "refusing to reset artifact outside output root path=$reset_path" ;;
  esac
  rm -rf "$reset_path"
}

verify_bundle() {
  verify_bundle_path="$1"
  verify_bundle_id="$2"
  verify_bundle_label="$3"

  require_directory "$verify_bundle_path" "$verify_bundle_label"
  require_file "$verify_bundle_path/Info.plist" "$verify_bundle_label Info.plist"
  require_equal "$(plist_value "$verify_bundle_path/Info.plist" CFBundleIdentifier "$verify_bundle_label bundle id")" "$verify_bundle_id" "$verify_bundle_label bundle id"
  require_equal "$(plist_value "$verify_bundle_path/Info.plist" CFBundleShortVersionString "$verify_bundle_label version")" "$marketing_version" "$verify_bundle_label version"
  require_equal "$(plist_value "$verify_bundle_path/Info.plist" CFBundleVersion "$verify_bundle_label build")" "$build_number" "$verify_bundle_label build"
  if ! codesign --verify --strict "$verify_bundle_path"; then
    fail "$verify_bundle_label code signature verification failed path=$verify_bundle_path"
  fi
  echo "testflight: verified $verify_bundle_label bundle_id=$verify_bundle_id version=$marketing_version build=$build_number"
}

verify_shipping_bundles() {
  shipping_app_path="$1"
  shipping_stage="$2"

  verify_bundle "$shipping_app_path" "com.mdx.anywhere" "$shipping_stage app"
  require_directory "$shipping_app_path/PlugIns" "$shipping_stage PlugIns"
  shipping_extension_count="$(find "$shipping_app_path/PlugIns" -mindepth 1 -maxdepth 1 -type d -name '*.appex' | wc -l | tr -d '[:space:]')"
  require_equal "$shipping_extension_count" "2" "$shipping_stage extension count"

  shipping_verified_extension_count=0
  for extension_contract in \
    "MDxAnywhereNotifications.appex:com.mdx.anywhere.notifications" \
    "MDxAnywhereActivity.appex:com.mdx.anywhere.activity"
  do
    extension_name="${extension_contract%%:*}"
    extension_bundle_id="${extension_contract#*:}"
    verify_bundle "$shipping_app_path/PlugIns/$extension_name" "$extension_bundle_id" "$shipping_stage $extension_name"
    shipping_verified_extension_count=$((shipping_verified_extension_count + 1))
  done
  require_equal "$shipping_verified_extension_count" "2" "$shipping_stage verified extension count"
}

extract_entitlements() {
  entitlements_app_path="$1"
  entitlements_destination="$2"
  entitlements_label="$3"
  if ! codesign -d --entitlements :- "$entitlements_app_path" > "$entitlements_destination" 2>/dev/null; then
    fail "$entitlements_label entitlement extraction failed path=$entitlements_app_path"
  fi
  require_file "$entitlements_destination" "$entitlements_label entitlements"
}

reset_artifact_path "$archive_path"
reset_artifact_path "$export_path"
reset_artifact_path "$inspection_path"
rm -f \
  "$output_root/archive-evidence.plist" \
  "$output_root/archive-evidence.json" \
  "$output_root/archive-entitlements.plist" \
  "$output_root/ExportOptions-TestFlight.resolved.plist" \
  "$output_root/signed-entitlements.plist" \
  "$output_root/upload-result.json"

signing_preflight() {
  preflight_path="$output_root/signing-preflight.txt"
  : > "$preflight_path"

  for target_contract in \
    "MDxAnywhere:com.mdx.anywhere" \
    "MDxAnywhereNotifications:com.mdx.anywhere.notifications" \
    "MDxAnywhereActivity:com.mdx.anywhere.activity"
  do
    target="${target_contract%%:*}"
    expected_bundle_id="${target_contract#*:}"
    settings_path="$output_root/signing-$target.txt"

    xcodebuild \
      -project "$root/MDxAnywhere.xcodeproj" \
      -target "$target" \
      -configuration Release \
      -sdk iphoneos \
      DEVELOPMENT_TEAM="$team" \
      CODE_SIGN_STYLE=Automatic \
      -showBuildSettings > "$settings_path"

    signing_style="$(awk -F ' = ' '/^[[:space:]]*CODE_SIGN_STYLE = / { print $2; exit }' "$settings_path")"
    bundle_id="$(awk -F ' = ' '/^[[:space:]]*PRODUCT_BUNDLE_IDENTIFIER = / { print $2; exit }' "$settings_path")"
    if [ "$signing_style" != "Automatic" ]; then
      echo "testflight: $target must resolve automatic signing" >&2
      exit 1
    fi
    if [ "$bundle_id" != "$expected_bundle_id" ]; then
      echo "testflight: $target resolved unexpected bundle id $bundle_id" >&2
      exit 1
    fi
    if grep -Fq 'CODE_SIGN_IDENTITY = Apple Distribution' "$settings_path"; then
      echo "testflight: $target mixes automatic signing with a manual distribution identity" >&2
      exit 1
    fi

    printf 'target=%s bundle_id=%s signing_style=%s manual_distribution_identity=false\n' \
      "$target" "$bundle_id" "$signing_style" >> "$preflight_path"
    rm -f "$settings_path"
  done

  echo "testflight: signing preflight passed targets=3 strategy=automatic"
}

signing_preflight
if [ "${MDX_TESTFLIGHT_PREFLIGHT_ONLY:-0}" = "1" ]; then
  echo "testflight: preflight-only mode complete"
  exit 0
fi

archive() {
  xcodebuild \
    -project "$root/MDxAnywhere.xcodeproj" \
    -scheme MDxAnywhere \
    -configuration Release \
    -destination "generic/platform=iOS" \
    -archivePath "$archive_path" \
    DEVELOPMENT_TEAM="$team" \
    CODE_SIGN_STYLE=Automatic \
    MARKETING_VERSION="$marketing_version" \
    CURRENT_PROJECT_VERSION="$build_number" \
    MDX_SUPABASE_URL="$MDX_SUPABASE_URL" \
    MDX_SUPABASE_PUBLISHABLE_KEY="$MDX_SUPABASE_PUBLISHABLE_KEY" \
    MDX_PUBLIC_HOST_URL="$MDX_PUBLIC_HOST_URL" \
    -allowProvisioningUpdates \
    "$@" \
    clean archive
}

if [ "${MDX_TESTFLIGHT_UPLOAD:-0}" = "1" ]; then
  : "${MDX_APP_STORE_CONNECT_KEY_ID:?MDX_APP_STORE_CONNECT_KEY_ID is required for signing}"
  : "${MDX_APP_STORE_CONNECT_ISSUER_ID:?MDX_APP_STORE_CONNECT_ISSUER_ID is required for signing}"
  : "${MDX_APP_STORE_CONNECT_KEY_PATH:?MDX_APP_STORE_CONNECT_KEY_PATH is required for signing}"
  : "${MDX_IOS_APP_PROFILE_SPECIFIER:?MDX_IOS_APP_PROFILE_SPECIFIER is required for manual distribution export}"
  : "${MDX_IOS_ACTIVITY_PROFILE_SPECIFIER:?MDX_IOS_ACTIVITY_PROFILE_SPECIFIER is required for manual distribution export}"
  : "${MDX_IOS_NOTIFICATIONS_PROFILE_SPECIFIER:?MDX_IOS_NOTIFICATIONS_PROFILE_SPECIFIER is required for manual distribution export}"
  [ -r "$MDX_APP_STORE_CONNECT_KEY_PATH" ] || fail "App Store Connect key is not readable path=$MDX_APP_STORE_CONNECT_KEY_PATH"
  archive \
    -authenticationKeyPath "$MDX_APP_STORE_CONNECT_KEY_PATH" \
    -authenticationKeyID "$MDX_APP_STORE_CONNECT_KEY_ID" \
    -authenticationKeyIssuerID "$MDX_APP_STORE_CONNECT_ISSUER_ID"
else
  archive
fi

archive_app="$archive_path/Products/Applications/MDxAnywhere.app"
verify_shipping_bundles "$archive_app" "archive"
extract_entitlements "$archive_app" "$output_root/archive-entitlements.plist" "archive app"
require_equal "$(plist_value "$output_root/archive-entitlements.plist" 'com.apple.developer.applesignin:0' 'archive Apple sign-in entitlement')" "Default" "archive Apple sign-in entitlement"
require_equal "$(plist_value "$output_root/archive-entitlements.plist" 'com.apple.developer.devicecheck.appattest-environment' 'archive App Attest entitlement')" "production" "archive App Attest entitlement"
archive_push_environment="$(plist_value "$output_root/archive-entitlements.plist" aps-environment 'archive push entitlement')"
case "$archive_push_environment" in
  development|production) ;;
  *) fail "archive push entitlement is unexpected actual=$archive_push_environment" ;;
esac
echo "testflight: archive proof passed push_environment=$archive_push_environment distribution_check=deferred_until_export"

export_options="$root/Support/ExportOptions-AppStore.plist"
if [ "${MDX_TESTFLIGHT_UPLOAD:-0}" = "1" ]; then
  export_options="$output_root/ExportOptions-TestFlight.resolved.plist"
  MDX_IOS_EXPORT_OPTIONS_OUTPUT="$export_options" \
    sh "$root/script/prepare_testflight_export_options.sh"
  xcodebuild \
    -exportArchive \
    -archivePath "$archive_path" \
    -exportPath "$export_path" \
    -exportOptionsPlist "$export_options"
else
  xcodebuild \
    -exportArchive \
    -archivePath "$archive_path" \
    -exportPath "$export_path" \
    -exportOptionsPlist "$export_options" \
    -allowProvisioningUpdates
fi

ipa_path=""
ipa_count=0
for candidate in "$export_path"/*.ipa; do
  if [ -f "$candidate" ]; then
    ipa_path="$candidate"
    ipa_count=$((ipa_count + 1))
  fi
done
require_equal "$ipa_count" "1" "exported IPA count"
require_file "$ipa_path" "exported IPA"
mkdir -p "$inspection_path"
if ! ditto -x -k "$ipa_path" "$inspection_path"; then
  fail "exported IPA extraction failed path=$ipa_path"
fi

exported_app="$inspection_path/Payload/MDxAnywhere.app"
verify_shipping_bundles "$exported_app" "exported"
extract_entitlements "$exported_app" "$output_root/signed-entitlements.plist" "exported app"
require_equal "$(plist_value "$output_root/signed-entitlements.plist" 'com.apple.developer.applesignin:0' 'exported Apple sign-in entitlement')" "Default" "exported Apple sign-in entitlement"
require_equal "$(plist_value "$output_root/signed-entitlements.plist" aps-environment 'exported push entitlement')" "production" "exported push entitlement"
require_equal "$(plist_value "$output_root/signed-entitlements.plist" 'com.apple.developer.devicecheck.appattest-environment' 'exported App Attest entitlement')" "production" "exported App Attest entitlement"
if exported_get_task_allow="$(/usr/libexec/PlistBuddy -c 'Print :get-task-allow' "$output_root/signed-entitlements.plist" 2>/dev/null)"; then
  require_equal "$exported_get_task_allow" "false" "exported get-task-allow entitlement"
fi
echo "testflight: distribution export proof passed push_environment=production get_task_allow=${exported_get_task_allow:-absent}"

plutil -create xml1 "$output_root/archive-evidence.plist"
plutil -insert version -string "$marketing_version" "$output_root/archive-evidence.plist"
plutil -insert build -string "$build_number" "$output_root/archive-evidence.plist"
plutil -insert team_id -string "$team" "$output_root/archive-evidence.plist"
plutil -insert bundle_id -string "com.mdx.anywhere" "$output_root/archive-evidence.plist"
plutil -insert archive_push_environment -string "$archive_push_environment" "$output_root/archive-evidence.plist"
plutil -insert exported_app_attest_environment -string "production" "$output_root/archive-evidence.plist"
plutil -insert exported_push_environment -string "production" "$output_root/archive-evidence.plist"
plutil -insert exported_apple_sign_in -bool true "$output_root/archive-evidence.plist"
plutil -insert exported_get_task_allow -string "${exported_get_task_allow:-absent}" "$output_root/archive-evidence.plist"
plutil -insert signed_extension_count -integer "2" "$output_root/archive-evidence.plist"
plutil -insert signed_app_sha256 -string "$(shasum -a 256 "$exported_app/MDxAnywhere" | awk '{print $1}')" "$output_root/archive-evidence.plist"
plutil -insert exported_ipa_sha256 -string "$(shasum -a 256 "$ipa_path" | awk '{print $1}')" "$output_root/archive-evidence.plist"
plutil -convert json -o "$output_root/archive-evidence.json" "$output_root/archive-evidence.plist"
rm -f "$output_root/archive-evidence.plist"

if [ "${MDX_TESTFLIGHT_UPLOAD:-0}" = "1" ]; then
  if ! xcrun altool \
    --upload-package "$ipa_path" \
    --api-key "$MDX_APP_STORE_CONNECT_KEY_ID" \
    --api-issuer "$MDX_APP_STORE_CONNECT_ISSUER_ID" \
    --p8-file-path "$MDX_APP_STORE_CONNECT_KEY_PATH" \
    --output-format json > "$output_root/upload-result.json" 2>&1; then
    cat "$output_root/upload-result.json" >&2
    fail "App Store Connect upload failed version=$marketing_version build=$build_number"
  fi
  cat "$output_root/upload-result.json"
  echo "testflight: upload submitted version=$marketing_version build=$build_number"
else
  echo "testflight: export ready version=$marketing_version build=$build_number path=$ipa_path"
fi
