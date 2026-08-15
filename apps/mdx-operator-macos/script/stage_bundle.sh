#!/usr/bin/env bash
# Shared bundle recipe: stage dist/MDx.app from a built binary, write the
# Info.plist once (both callers had their own copy and they had already
# drifted), and sign with a STABLE identity when one exists.
#
# Signing: pure ad-hoc ("-") produces a new code identity every rebuild, so
# macOS resets notification authorization and TCC grants each time. If a
# self-signed "MDx Dev" codesigning certificate exists in the login keychain,
# it is used instead, which keeps the identity stable across rebuilds. Create
# one once with Keychain Access > Certificate Assistant > Create a
# Certificate (name: "MDx Dev", type: Code Signing). Falls back to ad-hoc.
#
# Usage: stage_bundle.sh <built-binary-path>
set -euo pipefail

PRODUCT_NAME="MDxWorkbench"
DISPLAY_NAME="MDx"
BUNDLE_ID="com.mdx.app"
MIN_SYSTEM_VERSION="14.0"
# Marketing + build version. Bump MARKETING on notable milestones; BUILD is a
# date stamp so Notification Center and crash logs identify the exact build.
MARKETING_VERSION="${MDX_MARKETING_VERSION:-0.9.2}"
BUILD_VERSION="${MDX_BUILD_VERSION:-$(date +%Y%m%d%H%M)}"
SIGN_IDENTITY_NAME="MDx Dev"

BUILD_BINARY="${1:?usage: stage_bundle.sh <built-binary-path>}"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
APP_BUNDLE="$DIST_DIR/$DISPLAY_NAME.app"
APP_CONTENTS="$APP_BUNDLE/Contents"
APP_MACOS="$APP_CONTENTS/MacOS"
APP_BINARY="$APP_MACOS/$PRODUCT_NAME"
INFO_PLIST="$APP_CONTENTS/Info.plist"
SPARKLE_FRAMEWORK_SOURCE="$ROOT_DIR/.build/artifacts/sparkle/Sparkle/Sparkle.xcframework/macos-arm64_x86_64/Sparkle.framework"
SPARKLE_FRAMEWORK="$APP_CONTENTS/Frameworks/Sparkle.framework"

rm -rf "$APP_BUNDLE"
mkdir -p "$APP_MACOS"
mkdir -p "$APP_CONTENTS/Resources"
mkdir -p "$APP_CONTENTS/Frameworks"
cp "$BUILD_BINARY" "$APP_BINARY"
chmod +x "$APP_BINARY"
test -d "$SPARKLE_FRAMEWORK_SOURCE" || {
  echo "stage_bundle: Sparkle.framework artifact is missing; run swift package resolve and build first" >&2
  exit 1
}
/usr/bin/ditto "$SPARKLE_FRAMEWORK_SOURCE" "$SPARKLE_FRAMEWORK"
# MDx is not sandboxed. Sparkle's Downloader and Installer XPC services are
# only required for sandboxed applications and broaden the signing surface.
rm -rf "$SPARKLE_FRAMEWORK/Versions/Current/XPCServices"

# App icon (regenerate with: swift script/make_icon.swift + iconutil).
if [[ -f "$ROOT_DIR/Resources/AppIcon.icns" ]]; then
  cp "$ROOT_DIR/Resources/AppIcon.icns" "$APP_CONTENTS/Resources/AppIcon.icns"
fi

cat >"$INFO_PLIST" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>$PRODUCT_NAME</string>
  <key>CFBundleIdentifier</key>
  <string>$BUNDLE_ID</string>
  <key>CFBundleDisplayName</key>
  <string>$DISPLAY_NAME</string>
  <key>CFBundleName</key>
  <string>$DISPLAY_NAME</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleIconFile</key>
  <string>AppIcon</string>
  <key>CFBundleShortVersionString</key>
  <string>$MARKETING_VERSION</string>
  <key>CFBundleVersion</key>
  <string>$BUILD_VERSION</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.developer-tools</string>
  <key>LSMinimumSystemVersion</key>
  <string>$MIN_SYSTEM_VERSION</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
  <key>CFBundleURLTypes</key>
  <array>
    <dict>
      <key>CFBundleURLName</key>
      <string>com.mdx.app.auth</string>
      <key>CFBundleURLSchemes</key>
      <array>
        <string>mdx-workbench</string>
      </array>
    </dict>
  </array>
</dict>
</plist>
PLIST

cloud_values=(
  "${MDX_SUPABASE_URL:-}"
  "${MDX_SUPABASE_PUBLISHABLE_KEY:-}"
  "${MDX_PUBLIC_HOST_URL:-}"
)
cloud_value_count=0
for value in "${cloud_values[@]}"; do
  [[ -n "$value" ]] && cloud_value_count=$((cloud_value_count + 1))
done
if [[ "$cloud_value_count" -ne 0 && "$cloud_value_count" -ne 3 ]]; then
  echo "stage_bundle: cloud builds require MDX_SUPABASE_URL, MDX_SUPABASE_PUBLISHABLE_KEY, and MDX_PUBLIC_HOST_URL together" >&2
  exit 1
fi
if [[ "$cloud_value_count" -eq 3 ]]; then
  case "$MDX_SUPABASE_URL" in https://*) ;; *) echo "stage_bundle: MDX_SUPABASE_URL must use https" >&2; exit 1 ;; esac
  case "$MDX_PUBLIC_HOST_URL" in https://*) ;; *) echo "stage_bundle: MDX_PUBLIC_HOST_URL must use https" >&2; exit 1 ;; esac
  plutil -insert MDXSupabaseURL -string "$MDX_SUPABASE_URL" "$INFO_PLIST"
  plutil -insert MDXSupabasePublishableKey -string "$MDX_SUPABASE_PUBLISHABLE_KEY" "$INFO_PLIST"
  plutil -insert MDXPublicHostURL -string "$MDX_PUBLIC_HOST_URL" "$INFO_PLIST"
  plutil -insert MDXDistributionChannel -string "${MDX_DISTRIBUTION_CHANNEL:-canary}" "$INFO_PLIST"
  if [[ ! "${MDX_SPARKLE_PUBLIC_KEY:-}" =~ ^[A-Za-z0-9+/]{43}=$ ]]; then
    echo "stage_bundle: cloud canaries require a valid MDX_SPARKLE_PUBLIC_KEY" >&2
    exit 1
  fi
  plutil -insert SUFeedURL -string "${MDX_PUBLIC_HOST_URL%/}/download/macos/appcast.xml" "$INFO_PLIST"
  plutil -insert SUPublicEDKey -string "$MDX_SPARKLE_PUBLIC_KEY" "$INFO_PLIST"
  # MDx owns the schedule so it can refresh the private beta bearer token
  # immediately before each quiet check.
  plutil -insert SUEnableAutomaticChecks -bool false "$INFO_PLIST"
  plutil -insert SUSendsSystemProfile -bool false "$INFO_PLIST"
fi

plutil -lint "$INFO_PLIST" >/dev/null

if command -v codesign >/dev/null 2>&1; then
  identity="-"
  # Match by name WITHOUT -v (valid only): a self-signed "MDx Dev" cert is
  # untrusted by default (CSSMERR_TP_NOT_TRUSTED), which -v filters out, but
  # codesign still signs with it and it gives a STABLE local identity across
  # rebuilds - the point here (trust only matters for Gatekeeper verification,
  # not for keeping TCC/notification grants). Ad-hoc stays the fallback.
  if security find-identity -p codesigning 2>/dev/null | grep -q "$SIGN_IDENTITY_NAME"; then
    identity="$SIGN_IDENTITY_NAME"
  fi
  codesign --force --sign "$identity" "$SPARKLE_FRAMEWORK/Versions/Current/Autoupdate" >/dev/null
  codesign --force --sign "$identity" "$SPARKLE_FRAMEWORK/Versions/Current/Updater.app" >/dev/null
  codesign --force --sign "$identity" "$SPARKLE_FRAMEWORK" >/dev/null
  codesign --force --sign "$identity" "$APP_BUNDLE" >/dev/null
  if [[ "$identity" == "-" ]]; then
    echo "stage_bundle: signed ad-hoc (create an 'MDx Dev' certificate for stable notifications/TCC)"
  else
    echo "stage_bundle: signed with '$identity'"
  fi
fi

echo "stage_bundle: OK app=$APP_BUNDLE version=$MARKETING_VERSION ($BUILD_VERSION)"
