#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="$ROOT_DIR/dist/MDx.app"

"$ROOT_DIR/script/package_release.sh"
codesign --verify --strict --verbose=2 "$APP"
plutil -lint "$APP/Contents/Info.plist" >/dev/null
test "$(plutil -extract LSMinimumSystemVersion raw "$APP/Contents/Info.plist")" = "14.0"

if [[ -z "${MDX_SIGN_IDENTITY:-}" ]]; then
  echo "distribution_readiness: LOCAL PACKAGE READY"
  echo "distribution_readiness: BLOCKED Developer ID signing needs MDX_SIGN_IDENTITY"
  [[ "${1:-}" == "--require-credential-inputs" ]] && exit 2
  exit 0
fi
if [[ -z "${MDX_NOTARY_PROFILE:-}" ]] &&
   { [[ -z "${MDX_NOTARY_KEY_PATH:-}" ]] || [[ -z "${MDX_NOTARY_KEY_ID:-}" ]] || [[ -z "${MDX_NOTARY_ISSUER_ID:-}" ]]; }; then
  echo "distribution_readiness: BLOCKED notarization needs MDX_NOTARY_PROFILE or complete App Store Connect API key inputs"
  [[ "${1:-}" == "--require-credential-inputs" ]] && exit 2
  exit 0
fi

echo "distribution_readiness: CREDENTIALS PRESENT run script/notarize.sh to submit and staple"
