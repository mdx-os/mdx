#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BUNDLE="${1:-$ROOT_DIR/dist/MDx.app}"
DMG_PATH="${2:-$ROOT_DIR/dist/MDx.dmg}"

test -d "$APP_BUNDLE" || { echo "package_dmg: app bundle not found: $APP_BUNDLE" >&2; exit 1; }
mkdir -p "$(dirname "$DMG_PATH")" "$ROOT_DIR/dist"
stage_dir="$(mktemp -d "$ROOT_DIR/dist/.dmg-stage.XXXXXX")"
cleanup() {
  if [[ -n "${stage_dir:-}" && "$stage_dir" == "$ROOT_DIR/dist/.dmg-stage."* ]]; then
    rm -rf "$stage_dir"
  fi
}
trap cleanup EXIT

/usr/bin/ditto "$APP_BUNDLE" "$stage_dir/MDx.app"
ln -s /Applications "$stage_dir/Applications"
rm -f "$DMG_PATH"
hdiutil create \
  -volname "MDx" \
  -srcfolder "$stage_dir" \
  -ov \
  -format UDZO \
  -imagekey zlib-level=9 \
  "$DMG_PATH" >/dev/null
hdiutil imageinfo "$DMG_PATH" >/dev/null

echo "package_dmg: OK app=$APP_BUNDLE disk_image=$DMG_PATH"
