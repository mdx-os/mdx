#!/usr/bin/env bash
set -euo pipefail

PRODUCT_NAME="MDxWorkbench"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

swift build -c release
BUILD_BINARY="$(swift build -c release --show-bin-path)/$PRODUCT_NAME"

"$ROOT_DIR/script/stage_bundle.sh" "$BUILD_BINARY"

echo "package_release: OK app=$ROOT_DIR/dist/MDx.app binary=$PRODUCT_NAME"
