#!/usr/bin/env sh
# MDx setup - the one cold start.
#
# This is the single canonical way to stand MDx up locally (snapshot-backed, no
# Docker). It DETECTS prerequisites and prints exact install steps for anything
# missing - it never installs the toolchains, so it is safe to run on a
# locked-down machine. With prerequisites present it installs the workspace
# packages, boots the local stack, and opens the browser to the first-run
# welcome screen. Everything after that happens in the browser, not here.
#
# The Docker + Postgres "production-shaped" path is the opt-in; see deploy/README.md.
# The canonical path is declared in generated/setup/mdx-setup-path.json and held
# honest by `make setup-path-check`.
set -eu

repo_root="$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

ui_port="${MDX_DOGFOOD_UI_PORT:-5200}"
ui_url="http://127.0.0.1:${ui_port}/"
shell_log=".mdx-local/dogfood/shell.log"

# The pinned pnpm version is read from package.json so this script can never
# drift from what the workspace actually requires.
pinned_pnpm="$(sed -n 's/.*"packageManager": *"pnpm@\([0-9][0-9.]*\)".*/\1/p' package.json | head -1)"
[ -n "$pinned_pnpm" ] || pinned_pnpm="9.15.9"

printf '\n  MDx setup\n  Checking what this machine has, then starting MDx locally.\n\n'

# --- Detect prerequisites (never install the toolchains) ------------------
missing=0

need() {
  # need <command> <name> <why> <how>
  if command -v "$1" >/dev/null 2>&1; then
    printf '  [ok]      %s\n' "$2"
  else
    printf '  [missing] %s - %s\n            install: %s\n' "$2" "$3" "$4"
    missing=1
  fi
}

need cargo "Rust (cargo)" "builds the MDx kernel" "https://rustup.rs"
need node "Node.js" "runs the MDx product shell" "Node.js LTS from https://nodejs.org"

# pnpm must be present AND the pinned version - any pnpm is not enough.
pnpm_hint="comes with Node via Corepack: run 'corepack enable', then 'corepack prepare pnpm@${pinned_pnpm} --activate'"
if ! command -v pnpm >/dev/null 2>&1; then
  printf '  [missing] pnpm %s - the product shell is a pnpm workspace\n            install: %s\n' "$pinned_pnpm" "$pnpm_hint"
  missing=1
else
  have_pnpm="$(pnpm --version 2>/dev/null || echo unknown)"
  if [ "$have_pnpm" != "$pinned_pnpm" ]; then
    printf '  [wrong]   pnpm %s required, found %s\n            fix:     %s\n' "$pinned_pnpm" "$have_pnpm" "$pnpm_hint"
    missing=1
  else
    printf '  [ok]      pnpm %s\n' "$pinned_pnpm"
  fi
fi

if [ "$missing" -ne 0 ]; then
  printf '\n  Some prerequisites are missing or the wrong version. Fix the ones flagged\n  above, then run this again:\n\n      make setup\n\n  Nothing was installed or changed on your machine.\n\n'
  exit 1
fi

# --- Install the workspace packages the shell needs -----------------------
# Explicit and up front, with a frozen lockfile, so a fresh clone never falls
# back to lazy or implicit dependency work while booting.
printf '\n  Installing workspace packages (pnpm install)...\n\n'
if ! pnpm install --frozen-lockfile; then
  printf '\n  pnpm install failed (see the error above). Fix it, then run: make setup\n\n'
  exit 1
fi

# --- Boot the canonical local stack ---------------------------------------
# dogfood-stack is idempotent and fails loudly if a service does not come up;
# the snapshot path - no Docker. The first kernel build can take a few minutes.
printf '\n  Starting MDx (the first kernel build can take a few minutes)...\n\n'
sh ./scripts/dogfood-stack.sh

# --- Verify, then open the browser ----------------------------------------
# Never print success on a dead shell. Confirm the front door actually answers.
if ! curl -fsS "$ui_url" >/dev/null 2>&1; then
  printf '\n  MDx did not finish starting - the app is not answering at %s.\n  See the log: %s\n\n' "$ui_url" "$shell_log"
  exit 1
fi

if command -v open >/dev/null 2>&1; then
  open "$ui_url" >/dev/null 2>&1 || true
elif command -v xdg-open >/dev/null 2>&1; then
  xdg-open "$ui_url" >/dev/null 2>&1 || true
fi

printf '\n  MDx is running.\n\n      Open %s\n\n  If your browser did not open, paste that into it. Next steps - claiming\n  this install and connecting a model - are on that screen.\n\n' "$ui_url"
