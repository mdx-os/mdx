#!/bin/sh
set -eu

root="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
workspace_root="$(CDPATH= cd -- "$root/../.." && pwd)"
output_root_input="${MDX_IOS_DISTRIBUTION_DIR:-$workspace_root/.mdx-local/ios-distribution}"
team="${MDX_APPLE_DEVELOPMENT_TEAM:-}"
profile_env_file="${MDX_IOS_PROFILE_ENV_FILE:-}"
profile_install_dir="${MDX_IOS_PROFILE_INSTALL_DIR:-$HOME/Library/MobileDevice/Provisioning Profiles}"
profile_cleanup_file="${MDX_IOS_PROFILE_CLEANUP_FILE:-$output_root_input/installed-profile-paths.txt}"
profile_input_format="${MDX_IOS_PROFILE_INPUT_FORMAT:-cms}"

umask 077

fail() {
  echo "ios_distribution_profiles: ERROR $*" >&2
  exit 1
}

plist_value() {
  pv_plist="$1"
  pv_key="$2"
  pv_label="$3"
  pv_value="$(/usr/libexec/PlistBuddy -c "Print :$pv_key" "$pv_plist" 2>/dev/null)" ||
    fail "$pv_label is missing key=$pv_key"
  printf '%s' "$pv_value"
}

require_equal() {
  re_actual="$1"
  re_expected="$2"
  re_label="$3"
  [ "$re_actual" = "$re_expected" ] ||
    fail "$re_label mismatch expected=$re_expected actual=$re_actual"
}

require_absent() {
  ra_plist="$1"
  ra_key="$2"
  ra_label="$3"
  if /usr/libexec/PlistBuddy -c "Print :$ra_key" "$ra_plist" >/dev/null 2>&1; then
    fail "$ra_label must not contain $ra_key"
  fi
}

require_value_or_array_contains() {
  rv_plist="$1"
  rv_key="$2"
  rv_expected="$3"
  rv_label="$4"
  rv_value="$(plist_value "$rv_plist" "$rv_key" "$rv_label")"
  if [ "$rv_value" = "$rv_expected" ] ||
    printf '%s\n' "$rv_value" | grep -Eq "^[[:space:]]*$rv_expected[[:space:]]*$"; then
    return
  fi
  fail "$rv_label must include $rv_expected"
}

case "$profile_input_format" in
  cms|plist) ;;
  *) fail "MDX_IOS_PROFILE_INPUT_FORMAT must be cms or plist" ;;
esac

[ -n "$team" ] || fail "MDX_APPLE_DEVELOPMENT_TEAM is required"
[ -n "$profile_env_file" ] || fail "MDX_IOS_PROFILE_ENV_FILE is required"
: "${MDX_IOS_APP_PROFILE_BASE64:?MDX_IOS_APP_PROFILE_BASE64 is required}"
: "${MDX_IOS_ACTIVITY_PROFILE_BASE64:?MDX_IOS_ACTIVITY_PROFILE_BASE64 is required}"
: "${MDX_IOS_NOTIFICATIONS_PROFILE_BASE64:?MDX_IOS_NOTIFICATIONS_PROFILE_BASE64 is required}"

mkdir -p "$output_root_input" "$profile_install_dir"
output_root="$(CDPATH= cd -- "$output_root_input" && pwd)"
profile_staging_dir="$(mktemp -d "${TMPDIR:-/tmp}/mdx-ios-profiles.XXXXXX")"
trap 'rm -rf "$profile_staging_dir"' EXIT HUP INT TERM
: > "$profile_cleanup_file"
touch "$profile_env_file"

evidence_path="$output_root/profile-evidence.plist"
rm -f "$evidence_path" "$output_root/profile-evidence.json"
plutil -create xml1 "$evidence_path"
/usr/libexec/PlistBuddy -c "Add :strategy string manual" "$evidence_path"
/usr/libexec/PlistBuddy -c "Add :profiles array" "$evidence_path"

install_profile() {
  ip_index="$1"
  ip_key="$2"
  ip_expected_bundle_id="$3"
  ip_encoded_profile="$4"
  ip_validate_app_entitlements="$5"
  ip_encoded_path="$profile_staging_dir/$ip_key.mobileprovision"
  ip_decoded_path="$profile_staging_dir/$ip_key.plist"

  printf '%s' "$ip_encoded_profile" | base64 -D > "$ip_encoded_path" ||
    fail "$ip_key profile is not valid base64"
  [ -s "$ip_encoded_path" ] || fail "$ip_key profile decoded to an empty file"

  if [ "$profile_input_format" = "plist" ]; then
    cp "$ip_encoded_path" "$ip_decoded_path"
  else
    security cms -D -i "$ip_encoded_path" > "$ip_decoded_path" ||
      fail "$ip_key profile is not a valid signed provisioning profile"
  fi
  plutil -lint "$ip_decoded_path" >/dev/null || fail "$ip_key decoded profile is not a valid plist"

  ip_uuid="$(plist_value "$ip_decoded_path" UUID "$ip_key profile UUID")"
  ip_team="$(plist_value "$ip_decoded_path" 'TeamIdentifier:0' "$ip_key profile team")"
  ip_application_id="$(plist_value "$ip_decoded_path" 'Entitlements:application-identifier' "$ip_key application identifier")"
  ip_get_task_allow="$(plist_value "$ip_decoded_path" 'Entitlements:get-task-allow' "$ip_key get-task-allow")"

  case "$ip_uuid" in
    *[!A-Za-z0-9-]*|'') fail "$ip_key profile UUID contains unsupported characters" ;;
  esac
  require_equal "$ip_team" "$team" "$ip_key profile team"
  require_equal "$ip_application_id" "$team.$ip_expected_bundle_id" "$ip_key bundle id"
  require_equal "$ip_get_task_allow" "false" "$ip_key distribution entitlement"
  require_absent "$ip_decoded_path" ProvisionedDevices "$ip_key App Store profile"
  require_absent "$ip_decoded_path" ProvisionsAllDevices "$ip_key App Store profile"

  if [ "$ip_validate_app_entitlements" = "true" ]; then
    require_equal "$(plist_value "$ip_decoded_path" 'Entitlements:aps-environment' "$ip_key push entitlement")" "production" "$ip_key push entitlement"
    require_equal "$(plist_value "$ip_decoded_path" 'Entitlements:com.apple.developer.applesignin:0' "$ip_key Apple sign-in entitlement")" "Default" "$ip_key Apple sign-in entitlement"
    require_value_or_array_contains "$ip_decoded_path" 'Entitlements:com.apple.developer.devicecheck.appattest-environment' "production" "$ip_key App Attest entitlement"
  fi

  ip_installed_path="$profile_install_dir/$ip_uuid.mobileprovision"
  cp "$ip_encoded_path" "$ip_installed_path"
  printf '%s\n' "$ip_installed_path" >> "$profile_cleanup_file"
  printf 'MDX_IOS_%s_PROFILE_SPECIFIER=%s\n' "$ip_key" "$ip_uuid" >> "$profile_env_file"

  /usr/libexec/PlistBuddy \
    -c "Add :profiles:$ip_index dict" \
    -c "Add :profiles:$ip_index:bundle_id string $ip_expected_bundle_id" \
    -c "Add :profiles:$ip_index:profile_uuid string $ip_uuid" \
    "$evidence_path"
}

install_profile 0 APP "com.mdx.anywhere" "$MDX_IOS_APP_PROFILE_BASE64" true
install_profile 1 ACTIVITY "com.mdx.anywhere.activity" "$MDX_IOS_ACTIVITY_PROFILE_BASE64" false
install_profile 2 NOTIFICATIONS "com.mdx.anywhere.notifications" "$MDX_IOS_NOTIFICATIONS_PROFILE_BASE64" false

plutil -convert json -o "$output_root/profile-evidence.json" "$evidence_path"
rm -f "$evidence_path"

echo "ios_distribution_profiles: OK profiles=3 strategy=manual"
