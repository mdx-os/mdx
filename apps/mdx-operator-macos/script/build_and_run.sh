#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-run}"
PRODUCT_NAME="MDxWorkbench"
DISPLAY_NAME="MDx"
BUNDLE_ID="com.mdx.app"
MIN_SYSTEM_VERSION="14.0"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$ROOT_DIR/dist"
APP_BUNDLE="$DIST_DIR/$DISPLAY_NAME.app"
APP_CONTENTS="$APP_BUNDLE/Contents"
APP_MACOS="$APP_CONTENTS/MacOS"
APP_BINARY="$APP_MACOS/$PRODUCT_NAME"
INFO_PLIST="$APP_CONTENTS/Info.plist"

cd "$ROOT_DIR"

# The development launcher deliberately replaces the running app. Record that
# intent before sending SIGTERM so the next launch does not present a crash
# recovery banner for a normal rebuild loop.
defaults write "$BUNDLE_ID" MDxCleanExit -bool true
pkill -x "$PRODUCT_NAME" >/dev/null 2>&1 || true
pkill -x "MDxOperator" >/dev/null 2>&1 || true

swift build
BUILD_BINARY="$(swift build --show-bin-path)/$PRODUCT_NAME"

"$ROOT_DIR/script/stage_bundle.sh" "$BUILD_BINARY"

open_app() {
  local api_url="${MDX_API_URL:-${MDX_OPERATOR_API_URL:-}}"
  local open_args=(-n)
  if [[ -n "$api_url" ]]; then
    open_args+=(--env "MDX_API_URL=$api_url")
  fi
  if [[ -n "${MDX_SHOT_ROUTE:-}" ]]; then
    open_args+=(--env "MDX_SHOT_ROUTE=$MDX_SHOT_ROUTE")
  fi
  if [[ -n "${MDX_SHOT_MEMORY_RAIL:-}" ]]; then
    open_args+=(--env "MDX_SHOT_MEMORY_RAIL=$MDX_SHOT_MEMORY_RAIL")
  fi
  if [[ -n "${MDX_SHOT_RUN:-}" ]]; then
    open_args+=(--env "MDX_SHOT_RUN=$MDX_SHOT_RUN")
  fi
  if [[ -n "${MDX_SHOT_TAB:-}" ]]; then
    open_args+=(--env "MDX_SHOT_TAB=$MDX_SHOT_TAB")
  fi
  if [[ -n "${MDX_SHOT_BUILD_INTENT:-}" ]]; then
    open_args+=(--env "MDX_SHOT_BUILD_INTENT=$MDX_SHOT_BUILD_INTENT")
  fi
  if [[ -n "${MDX_SHOT_AUTOSTART_BUILD:-}" ]]; then
    open_args+=(--env "MDX_SHOT_AUTOSTART_BUILD=$MDX_SHOT_AUTOSTART_BUILD")
  fi
  if [[ -n "${MDX_SHOT_AUTOSTART_DELAY_MS:-}" ]]; then
    open_args+=(--env "MDX_SHOT_AUTOSTART_DELAY_MS=$MDX_SHOT_AUTOSTART_DELAY_MS")
  fi
  if [[ -n "${MDX_SHOT_FLEET_WIDTH:-}" ]]; then
    open_args+=(--env "MDX_SHOT_FLEET_WIDTH=$MDX_SHOT_FLEET_WIDTH")
  fi
  if [[ -n "${MDX_SHOT_PROOF_COMMANDS:-}" ]]; then
    open_args+=(--env "MDX_SHOT_PROOF_COMMANDS=$MDX_SHOT_PROOF_COMMANDS")
  fi
  if [[ -n "${MDX_SHOT_REPO_ID:-}" ]]; then
    open_args+=(--env "MDX_SHOT_REPO_ID=$MDX_SHOT_REPO_ID")
  fi
  /usr/bin/open "${open_args[@]}" "$APP_BUNDLE"
}

case "$MODE" in
  run)
    open_app
    ;;
  --debug|debug)
    lldb -- "$APP_BINARY"
    ;;
  --logs|logs)
    open_app
    /usr/bin/log stream --info --style compact --predicate "process == \"$PRODUCT_NAME\""
    ;;
  --telemetry|telemetry)
    open_app
    /usr/bin/log stream --info --style compact --predicate "subsystem == \"$BUNDLE_ID\""
    ;;
  --verify|verify)
    open_app
    sleep 1
    pgrep -x "$PRODUCT_NAME" >/dev/null
    ;;
  *)
    echo "usage: $0 [run|--debug|--logs|--telemetry|--verify]" >&2
    exit 2
    ;;
esac
