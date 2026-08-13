#!/usr/bin/env sh
set -eu

exclude_args="
:!apps/observatory/node_modules/**
:!apps/**/pnpm-lock.yaml
:!target/**
"

hits_file="${TMPDIR:-/tmp}/mdx-secret-history-hits.txt"
rm -f "$hits_file"

if [ "$(git rev-parse --is-shallow-repository 2>/dev/null || echo false)" = "true" ]; then
  echo "secret_history_check: repository is shallow; fetch full history before scanning" >&2
  exit 1
fi

# The full tree-at-every-revision scan is exact but too slow for CI-scale
# history. Git pickaxe scans the patch stream for the same secret patterns,
# including the root commit, and finishes in bounded time on hosted runners.
: >"$hits_file"
while IFS= read -r secret_pattern; do
  [ -n "$secret_pattern" ] || continue
  # shellcheck disable=SC2086
  git log --all --extended-regexp -G "$secret_pattern" \
    --date=iso-strict \
    --pretty='format:%H %ad %s' \
    -- $exclude_args >>"$hits_file"
done <<'PATTERNS'
AKIA[0-9A-Z]{16}
gh[pousr]_[A-Za-z0-9_]{20,}
sk-[A-Za-z0-9]{20,}
xox[baprs]-[A-Za-z0-9-]{20,}
-----BEGIN [A-Z ]*PRIVATE KEY-----
PATTERNS

if [ -s "$hits_file" ]; then
  echo "secret_history_check: possible historical secret pattern found:" >&2
  cat "$hits_file" >&2
  exit 1
fi

echo "secret_history_check: OK revisions=$(git rev-list --all --count) tracked_secret_patterns=clean"
