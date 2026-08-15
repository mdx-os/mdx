#!/usr/bin/env bash
# Developer ID signing + notarization for real distribution. Gated on
# credentials so it never fakes readiness:
#
#   MDX_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)"
#   MDX_NOTARY_PROFILE="mdx-notary"   # local Keychain profile, or CI inputs:
#   MDX_NOTARY_KEY_PATH=/path/to/AuthKey_ABC123.p8
#   MDX_NOTARY_KEY_ID=ABC123
#   MDX_NOTARY_ISSUER_ID=00000000-0000-0000-0000-000000000000
#
# One-time setup:
#   1. Apple Developer account -> create a Developer ID Application certificate.
#   2. xcrun notarytool store-credentials mdx-notary \
#        --apple-id you@example.com --team-id TEAMID --password <app-specific>
#
# Then: script/package_release.sh && script/notarize.sh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="$ROOT_DIR/dist/MDx.app"
ZIP_PATH="$ROOT_DIR/dist/MDx.zip"
DMG_PATH="$ROOT_DIR/dist/MDx.dmg"
SUBMISSION_ZIP="$ROOT_DIR/dist/MDx-notary-submission.zip"
MANIFEST_PATH="$ROOT_DIR/dist/manifest.json"
NOTARY_RESULT_PATH="$ROOT_DIR/dist/notary-submission.json"
NOTARY_LOG_PATH="$ROOT_DIR/dist/notary-log.json"
DMG_NOTARY_RESULT_PATH="$ROOT_DIR/dist/dmg-notary-submission.json"
DMG_NOTARY_LOG_PATH="$ROOT_DIR/dist/dmg-notary-log.json"
SIGNED_ENTITLEMENTS_PATH="$ROOT_DIR/dist/signed-entitlements.plist"
SPARKLE_FRAMEWORK="$APP_BUNDLE/Contents/Frameworks/Sparkle.framework"
SPARKLE_SIGN_UPDATE="$ROOT_DIR/.build/artifacts/sparkle/Sparkle/bin/sign_update"
PACKAGE_DMG="$ROOT_DIR/script/package_dmg.sh"

if [[ -z "${MDX_SIGN_IDENTITY:-}" ]]; then
  echo "notarize: SKIPPED (set MDX_SIGN_IDENTITY; see header of this script)" >&2
  exit 2
fi
notary_args=()
if [[ -n "${MDX_NOTARY_PROFILE:-}" ]]; then
  notary_args=(--keychain-profile "$MDX_NOTARY_PROFILE")
elif [[ -n "${MDX_NOTARY_KEY_PATH:-}" && -n "${MDX_NOTARY_KEY_ID:-}" && -n "${MDX_NOTARY_ISSUER_ID:-}" ]]; then
  test -r "$MDX_NOTARY_KEY_PATH" || { echo "notarize: MDX_NOTARY_KEY_PATH is not readable" >&2; exit 2; }
  notary_args=(--key "$MDX_NOTARY_KEY_PATH" --key-id "$MDX_NOTARY_KEY_ID" --issuer "$MDX_NOTARY_ISSUER_ID")
else
  echo "notarize: SKIPPED (set MDX_NOTARY_PROFILE or all three App Store Connect API key inputs)" >&2
  exit 2
fi

test -d "$APP_BUNDLE" || { echo "notarize: run script/package_release.sh first" >&2; exit 1; }
test -d "$SPARKLE_FRAMEWORK" || { echo "notarize: Sparkle.framework is missing from the staged app" >&2; exit 1; }
test -x "$SPARKLE_SIGN_UPDATE" || { echo "notarize: Sparkle sign_update tool is missing" >&2; exit 1; }
test -x "$PACKAGE_DMG" || { echo "notarize: package_dmg.sh is missing or not executable" >&2; exit 1; }
test -r "${MDX_SPARKLE_PRIVATE_KEY_PATH:-}" || { echo "notarize: MDX_SPARKLE_PRIVATE_KEY_PATH is required and must be readable" >&2; exit 2; }
# Browser-based Apple OAuth is used for direct Developer ID distribution.
# Remove any stale profile from a previously staged bundle before signing.
rm -f "$APP_BUNDLE/Contents/embedded.provisionprofile"

# Hardened runtime + Developer ID. Never --deep: Sparkle's nested executables
# must be signed from the inside out before the host app.
codesign --force --options runtime --timestamp \
  --sign "$MDX_SIGN_IDENTITY" "$SPARKLE_FRAMEWORK/Versions/Current/Autoupdate"
codesign --force --options runtime --timestamp \
  --sign "$MDX_SIGN_IDENTITY" "$SPARKLE_FRAMEWORK/Versions/Current/Updater.app"
codesign --force --options runtime --timestamp \
  --sign "$MDX_SIGN_IDENTITY" "$SPARKLE_FRAMEWORK"
codesign --force --options runtime --timestamp \
  --sign "$MDX_SIGN_IDENTITY" "$APP_BUNDLE"

codesign --verify --strict --verbose=2 "$APP_BUNDLE"
codesign -d --entitlements :- "$APP_BUNDLE" > "$SIGNED_ENTITLEMENTS_PATH" 2>/dev/null || true
if [[ ! -s "$SIGNED_ENTITLEMENTS_PATH" ]]; then
  plutil -create xml1 "$SIGNED_ENTITLEMENTS_PATH"
fi
if /usr/libexec/PlistBuddy -c 'Print :com.apple.developer.applesignin' "$SIGNED_ENTITLEMENTS_PATH" >/dev/null 2>&1; then
  echo "notarize: native Sign in with Apple entitlement is not supported for this Developer ID release" >&2
  exit 1
fi

rm -f "$SUBMISSION_ZIP" "$ZIP_PATH" "$DMG_PATH" "$MANIFEST_PATH" \
  "$NOTARY_RESULT_PATH" "$NOTARY_LOG_PATH" "$DMG_NOTARY_RESULT_PATH" "$DMG_NOTARY_LOG_PATH"
/usr/bin/ditto -c -k --keepParent "$APP_BUNDLE" "$SUBMISSION_ZIP"
xcrun notarytool submit "$SUBMISSION_ZIP" "${notary_args[@]}" --wait --output-format json >"$NOTARY_RESULT_PATH"
test "$(plutil -extract status raw "$NOTARY_RESULT_PATH")" = "Accepted" || {
  echo "notarize: Apple did not accept the submission" >&2
  cat "$NOTARY_RESULT_PATH" >&2
  exit 1
}
submission_id="$(plutil -extract id raw "$NOTARY_RESULT_PATH")"
xcrun notarytool log "$submission_id" "$NOTARY_LOG_PATH" "${notary_args[@]}"
xcrun stapler staple "$APP_BUNDLE"
xcrun stapler validate "$APP_BUNDLE"
codesign --verify --strict --verbose=2 "$APP_BUNDLE"
spctl --assess --type execute --verbose=2 "$APP_BUNDLE"

# Rebuild the distributable after stapling. The pre-submission archive does not
# contain the ticket and must never be published as the canary download.
/usr/bin/ditto -c -k --keepParent "$APP_BUNDLE" "$ZIP_PATH"
rm -f "$SUBMISSION_ZIP"
sha256="$(shasum -a 256 "$ZIP_PATH" | awk '{print $1}')"
size_bytes="$(stat -f '%z' "$ZIP_PATH")"

# The website offers a native drag-to-Applications disk image while Sparkle
# keeps the ZIP as its EdDSA-verified update payload. The app inside the DMG
# already carries its own stapled ticket; the signed container is submitted
# and stapled separately so Gatekeeper can verify both layers offline.
"$PACKAGE_DMG" "$APP_BUNDLE" "$DMG_PATH"
codesign --force --timestamp --sign "$MDX_SIGN_IDENTITY" "$DMG_PATH"
codesign --verify --verbose=2 "$DMG_PATH"
xcrun notarytool submit "$DMG_PATH" "${notary_args[@]}" --wait --output-format json >"$DMG_NOTARY_RESULT_PATH"
test "$(plutil -extract status raw "$DMG_NOTARY_RESULT_PATH")" = "Accepted" || {
  echo "notarize: Apple did not accept the DMG submission" >&2
  cat "$DMG_NOTARY_RESULT_PATH" >&2
  exit 1
}
dmg_submission_id="$(plutil -extract id raw "$DMG_NOTARY_RESULT_PATH")"
xcrun notarytool log "$dmg_submission_id" "$DMG_NOTARY_LOG_PATH" "${notary_args[@]}"
xcrun stapler staple "$DMG_PATH"
xcrun stapler validate "$DMG_PATH"
codesign --verify --verbose=2 "$DMG_PATH"
spctl --assess --type open --context context:primary-signature --verbose=2 "$DMG_PATH"
dmg_sha256="$(shasum -a 256 "$DMG_PATH" | awk '{print $1}')"
dmg_size_bytes="$(stat -f '%z' "$DMG_PATH")"
dmg_notarized_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

version="$(plutil -extract CFBundleShortVersionString raw "$APP_BUNDLE/Contents/Info.plist")"
build="$(plutil -extract CFBundleVersion raw "$APP_BUNDLE/Contents/Info.plist")"
sparkle_signature="$("$SPARKLE_SIGN_UPDATE" --ed-key-file "$MDX_SPARKLE_PRIVATE_KEY_PATH" "$ZIP_PATH" | awk -F'"' '/sparkle:edSignature=/{print $2; exit}')"
[[ "$sparkle_signature" =~ ^[A-Za-z0-9+/]{86}==$ ]] || {
  echo "notarize: Sparkle did not return a valid EdDSA archive signature" >&2
  exit 1
}
plutil -create xml1 "$MANIFEST_PATH"
plutil -insert version -string "$version" "$MANIFEST_PATH"
plutil -insert build -string "$build" "$MANIFEST_PATH"
plutil -insert sha256 -string "$sha256" "$MANIFEST_PATH"
plutil -insert size_bytes -integer "$size_bytes" "$MANIFEST_PATH"
plutil -insert sparkle_ed_signature -string "$sparkle_signature" "$MANIFEST_PATH"
plutil -insert notarized_at -string "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$MANIFEST_PATH"
plutil -insert dmg_sha256 -string "$dmg_sha256" "$MANIFEST_PATH"
plutil -insert dmg_size_bytes -integer "$dmg_size_bytes" "$MANIFEST_PATH"
plutil -insert dmg_notarized_at -string "$dmg_notarized_at" "$MANIFEST_PATH"
plutil -insert bundle_id -string "$(plutil -extract CFBundleIdentifier raw "$APP_BUNDLE/Contents/Info.plist")" "$MANIFEST_PATH"
plutil -insert apple_sign_in -bool true "$MANIFEST_PATH"
plutil -insert apple_sign_in_transport -string "browser_oauth" "$MANIFEST_PATH"
plutil -convert json "$MANIFEST_PATH"

echo "notarize: OK app=$APP_BUNDLE archive=$ZIP_PATH disk_image=$DMG_PATH manifest=$MANIFEST_PATH sha256=$sha256 dmg_sha256=$dmg_sha256"
