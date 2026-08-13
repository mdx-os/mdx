#!/usr/bin/env sh
set -eu

ensure_contains() {
  file="$1"
  needle="$2"
  if ! grep -F "$needle" "$file" >/dev/null 2>&1; then
    echo "package_posture_check: $file missing $needle" >&2
    exit 1
  fi
}

ensure_contains Cargo.toml 'license = "Apache-2.0"'
ensure_contains Cargo.toml 'repository = "https://github.com/mdx-os/mdx"'

manifest_count=0
find crates -mindepth 2 -maxdepth 2 -name Cargo.toml -print | sort |
while IFS= read -r manifest; do
  ensure_contains "$manifest" "description = "
  ensure_contains "$manifest" "publish = false"
  ensure_contains "$manifest" "license.workspace = true"
  ensure_contains "$manifest" "repository.workspace = true"
done

manifest_count="$(find crates -mindepth 2 -maxdepth 2 -name Cargo.toml | wc -l | tr -d ' ')"
echo "package_posture_check: OK publish=false workspace_packages=$manifest_count"
