#!/bin/sh
set -eu

root="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
template="$root/Support/ExportOptions-TestFlight.plist"
output="${MDX_IOS_EXPORT_OPTIONS_OUTPUT:-}"
team="${MDX_APPLE_DEVELOPMENT_TEAM:-}"

fail() {
  echo "testflight_export_options: ERROR $*" >&2
  exit 1
}

[ -n "$output" ] || fail "MDX_IOS_EXPORT_OPTIONS_OUTPUT is required"
[ -n "$team" ] || fail "MDX_APPLE_DEVELOPMENT_TEAM is required"
: "${MDX_IOS_APP_PROFILE_SPECIFIER:?MDX_IOS_APP_PROFILE_SPECIFIER is required}"
: "${MDX_IOS_ACTIVITY_PROFILE_SPECIFIER:?MDX_IOS_ACTIVITY_PROFILE_SPECIFIER is required}"
: "${MDX_IOS_NOTIFICATIONS_PROFILE_SPECIFIER:?MDX_IOS_NOTIFICATIONS_PROFILE_SPECIFIER is required}"

for profile_specifier in \
  "$MDX_IOS_APP_PROFILE_SPECIFIER" \
  "$MDX_IOS_ACTIVITY_PROFILE_SPECIFIER" \
  "$MDX_IOS_NOTIFICATIONS_PROFILE_SPECIFIER"
do
  case "$profile_specifier" in
    *[!A-Za-z0-9-]*|'') fail "profile specifier contains unsupported characters" ;;
  esac
done
case "$team" in
  *[!A-Za-z0-9]*|'') fail "development team contains unsupported characters" ;;
esac

cp "$template" "$output"
/usr/libexec/PlistBuddy -c "Delete :teamID" "$output" >/dev/null 2>&1 || true
/usr/libexec/PlistBuddy -c "Delete :provisioningProfiles" "$output" >/dev/null 2>&1 || true
/usr/libexec/PlistBuddy \
  -c "Add :teamID string $team" \
  -c "Add :provisioningProfiles dict" \
  -c "Add :provisioningProfiles:com.mdx.anywhere string $MDX_IOS_APP_PROFILE_SPECIFIER" \
  -c "Add :provisioningProfiles:com.mdx.anywhere.activity string $MDX_IOS_ACTIVITY_PROFILE_SPECIFIER" \
  -c "Add :provisioningProfiles:com.mdx.anywhere.notifications string $MDX_IOS_NOTIFICATIONS_PROFILE_SPECIFIER" \
  "$output"

plutil -lint "$output" >/dev/null
[ "$(plutil -extract signingStyle raw "$output")" = "manual" ] || fail "signingStyle must be manual"
[ "$(plutil -extract signingCertificate raw "$output")" = "Apple Distribution" ] || fail "signingCertificate must be Apple Distribution"
if plutil -extract testFlightInternalTestingOnly raw "$output" >/dev/null 2>&1; then
  fail "external cohort builds must not be marked TestFlight internal only"
fi

echo "testflight_export_options: OK profiles=3 strategy=manual external_testflight_eligible=true"
