#!/usr/bin/env bash
# Visual-QA capture rail for the MDx macOS operator app.
#
# Replaces the throwaway winid + screencapture scripts each campaign arc rebuilt
# with one committed, reusable tool. It:
#   1. Boots its OWN isolated worn kernel on a spare port (never the founder's
#      18890) from a copy of a snapshot, so captures show real data and nothing
#      is ever written back to a live company. Or attaches to a kernel you name.
#   2. Builds and stages the release app bundle (via stage_bundle.sh).
#   3. Iterates the seeded state gallery (script/gallery-states.tsv) across a
#      light / dark / accessibility matrix, launching the app once per
#      (state, variant) with the matching MDX_SHOT_* env, and does a
#      WINDOW-SCOPED capture of the "MDx" window. Never a full-screen grab.
#   4. Writes a manifest of what was captured, including states the seeded kernel
#      could not supply (it records "no window" / an honest note rather than
#      faking the state).
#
# The staged capture app runs as MDxGallery with its own bundle id and process
# name. The rail never terminates or captures the founder's installed MDx canary.
#
# Every override is env-configurable so this is not pinned to one machine:
#   MDX_GALLERY_KERNEL_URL  attach to this running kernel; skip the boot entirely
#   MDX_GALLERY_PORT        spare port for the isolated kernel (default 18896)
#   MDX_GALLERY_SNAPSHOT    worn snapshot to restore (default repo .mdx-local one)
#   MDX_GALLERY_SERVER_BIN  mdx-server binary (default repo target/debug, built if absent)
#   MDX_GALLERY_OUT         output dir (default apps/mdx-operator-macos/.gallery/<stamp>)
#   MDX_GALLERY_VARIANTS    space list of variants (default "light dark dark-large_text")
#   MDX_GALLERY_STATES      states TSV (default script/gallery-states.tsv)
#   MDX_GALLERY_ONLY        optional state name for a focused rerun
#   MDX_GALLERY_SETTLE      seconds to let a launched window settle (default 5)
#
# Accessibility matrix note: a still-frame rail can only capture settings that
# change a static frame. Larger Dynamic Type (large_text) IS captured. Reduce
# Motion changes only transitions (nothing in a still) and its environment key is
# read-only; Increase Contrast has no supported per-process override. The rail
# documents both instead of faking identical frames. See Support/ShotOverrides.swift.
set -euo pipefail

APP_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REPO_ROOT="$(cd "$APP_ROOT" && git rev-parse --show-toplevel)"

PORT="${MDX_GALLERY_PORT:-18896}"
if [[ "$PORT" == "18890" ]]; then
  echo "capture_gallery: refusing to use port 18890 (the founder's live kernel)" >&2
  exit 2
fi
SNAPSHOT="${MDX_GALLERY_SNAPSHOT:-$REPO_ROOT/.mdx-local/kernel-ledger-snapshot.json}"
SERVER_BIN="${MDX_GALLERY_SERVER_BIN:-$REPO_ROOT/target/debug/mdx-server}"
VARIANTS="${MDX_GALLERY_VARIANTS:-light dark dark-large_text}"
STATES_FILE="${MDX_GALLERY_STATES:-$APP_ROOT/script/gallery-states.tsv}"
ONLY="${MDX_GALLERY_ONLY:-}"
SETTLE="${MDX_GALLERY_SETTLE:-5}"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT="${MDX_GALLERY_OUT:-$APP_ROOT/.gallery/$STAMP}"
GALLERY_PROCESS="MDxGallery"
GALLERY_OWNER="MDx Gallery"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

MANIFEST="$OUT/manifest.tsv"
: >"$MANIFEST"
printf 'state\tvariant\tfile\tstatus\n' >>"$MANIFEST"

SCRATCH="$(mktemp -d)"
KERNEL_PID=""
cleanup() {
  [[ -n "$KERNEL_PID" ]] && kill "$KERNEL_PID" 2>/dev/null || true
  pkill -x "$GALLERY_PROCESS" 2>/dev/null || true
  rm -rf "$SCRATCH" 2>/dev/null || true
}
trap cleanup EXIT

# --- Kernel: attach or boot an isolated, worn instance ---------------------
if [[ -n "${MDX_GALLERY_KERNEL_URL:-}" ]]; then
  KERNEL_URL="$MDX_GALLERY_KERNEL_URL"
  echo "capture_gallery: attaching to $KERNEL_URL (no kernel booted)"
else
  KERNEL_URL="http://127.0.0.1:$PORT"
  if [[ ! -x "$SERVER_BIN" ]]; then
    echo "capture_gallery: building mdx-server ..."
    ( cd "$REPO_ROOT" && cargo build -q -p mdx-server )
    SERVER_BIN="$REPO_ROOT/target/debug/mdx-server"
  fi
  mkdir -p "$SCRATCH/.mdx-local"
  if [[ -f "$SNAPSHOT" ]]; then
    cp "$SNAPSHOT" "$SCRATCH/.mdx-local/kernel-ledger-snapshot.json"
    echo "capture_gallery: restoring worn snapshot ($(du -h "$SNAPSHOT" | cut -f1)) into an isolated cwd"
  else
    echo "capture_gallery: no snapshot at $SNAPSHOT; booting an empty kernel (data-dependent states will be sparse)"
  fi
  ( cd "$SCRATCH" && MDX_KERNEL_SNAPSHOT=1 "$SERVER_BIN" serve "127.0.0.1:$PORT" >"$OUT/kernel.log" 2>&1 ) &
  KERNEL_PID=$!
  printf 'capture_gallery: waiting for kernel at %s ' "$KERNEL_URL"
  for _ in $(seq 1 40); do
    if curl -fsS "$KERNEL_URL/health" >/dev/null 2>&1; then break; fi
    printf '.'; sleep 1
  done
  echo
  if ! curl -fsS "$KERNEL_URL/health" >/dev/null 2>&1; then
    echo "capture_gallery: kernel did not come up; see $OUT/kernel.log" >&2
    exit 1
  fi
  echo "capture_gallery: kernel up at $KERNEL_URL (isolated, snapshots only to scratch)"
fi

# --- Build + stage the app -------------------------------------------------
echo "capture_gallery: building release app ..."
( cd "$APP_ROOT" && swift build -c release >/dev/null )
APP_BIN="$APP_ROOT/.build/release/MDxWorkbench"
bash "$APP_ROOT/script/stage_bundle.sh" "$APP_BIN" >/dev/null
STAGED_APP="$APP_ROOT/dist/MDx.app"
APP="$SCRATCH/$GALLERY_OWNER.app"
cp -R "$STAGED_APP" "$APP"
mv "$APP/Contents/MacOS/MDxWorkbench" "$APP/Contents/MacOS/$GALLERY_PROCESS"
plutil -replace CFBundleExecutable -string "$GALLERY_PROCESS" "$APP/Contents/Info.plist"
plutil -replace CFBundleIdentifier -string "com.mdx.app.gallery" "$APP/Contents/Info.plist"
plutil -replace CFBundleDisplayName -string "$GALLERY_OWNER" "$APP/Contents/Info.plist"
plutil -replace CFBundleName -string "$GALLERY_OWNER" "$APP/Contents/Info.plist"
codesign --force --deep --sign - "$APP" >/dev/null

# --- Window-id helper (compiled once) --------------------------------------
WINID="$SCRATCH/winid"
swiftc -O -o "$WINID" "$APP_ROOT/script/winid.swift"

# Stop only the gallery process and WAIT until it is actually gone. A bare
# `pkill; sleep` races: if a prior gallery instance is slow to exit, its window
# lingers and the window-id helper can capture the wrong stale surface. The
# founder's installed MDxWorkbench process remains untouched throughout.
quit_gallery_instance() {
  pkill -x "$GALLERY_PROCESS" 2>/dev/null || true
  for _ in $(seq 1 25); do
    pgrep -x "$GALLERY_PROCESS" >/dev/null 2>&1 || return 0
    sleep 0.2
  done
  pkill -9 -x "$GALLERY_PROCESS" 2>/dev/null || true
  sleep 0.4
}

# --- Capture loop ----------------------------------------------------------
# variant "dark-large_text" -> MDX_SHOT_APPEARANCE=dark MDX_SHOT_A11Y=large_text
launch_and_capture() { # name variant env-tokens...
  local name="$1" variant="$2"; shift 2
  local appearance="${variant%%-*}"
  local a11y=""
  [[ "$variant" == *-* ]] && a11y="${variant#*-}"

  local -a envargs=(--env "MDX_API_URL=$KERNEL_URL" --env "MDX_SHOT_APPEARANCE=$appearance")
  [[ -n "$a11y" ]] && envargs+=(--env "MDX_SHOT_A11Y=$a11y")
  # Keep the welcome cover out of every non-onboarding state.
  [[ "$name" != onboarding* ]] && envargs+=(--env "MDX_SHOT_SKIP_WELCOME=1")
  local tok
  for tok in "$@"; do envargs+=(--env "$tok"); done

  quit_gallery_instance
  open -n -F "${envargs[@]}" "$APP"
  sleep "$SETTLE"

  local id="" tries launch_attempt
  for launch_attempt in 1 2; do
    for tries in $(seq 1 12); do
      if id="$("$WINID" "$GALLERY_OWNER" 2>/dev/null)"; then break 2; fi
      sleep 1
    done
    # LaunchServices can occasionally discard one rapid relaunch while the
    # prior app instance is still being reaped. Retry once instead of recording
    # a false no-window result. This still targets only the isolated gallery.
    quit_gallery_instance
    open -n -F "${envargs[@]}" "$APP"
    sleep "$SETTLE"
  done

  local file="$OUT/${name}__${variant}.png" status
  if [[ -n "$id" ]]; then
    if screencapture -x -o -l"$id" "$file" 2>/dev/null && [[ -s "$file" ]]; then
      status="ok"
    else
      status="capture-failed"
    fi
  else
    status="no-window"
    file="-"
  fi
  printf '%s\t%s\t%s\t%s\n' "$name" "$variant" "$(basename "$file")" "$status" >>"$MANIFEST"
  echo "capture_gallery: $name / $variant -> $status"
  quit_gallery_instance
}

while read -r name rest || [[ -n "$name" ]]; do
  [[ -z "${name// }" ]] && continue
  [[ "$name" == \#* ]] && continue
  [[ -n "$ONLY" && "$name" != "$ONLY" ]] && continue
  # shellcheck disable=SC2086
  for variant in $VARIANTS; do
    # shellcheck disable=SC2086
    launch_and_capture "$name" "$variant" $rest
  done
done <"$STATES_FILE"

# Accessibility diff report. For every variant that layers an a11y setting onto
# an appearance (e.g. "dark-large_text" over "dark"), compare the frame to its
# base-appearance frame. "identical" means the setting changed nothing on this
# platform (macOS has no Dynamic Type user setting and largely ignores the
# dynamicTypeSize environment for AppKit-hosted SwiftUI, so this is expected);
# "differs" is where a reviewer should look for a real layout regression. This is
# how the rail SURFACES the a11y result instead of leaving 30 look-alike frames.
A11Y_REPORT="$OUT/a11y-report.tsv"
printf 'state\ta11y_variant\tbase\tresult\n' >"$A11Y_REPORT"
while read -r name rest || [[ -n "$name" ]]; do
  [[ -z "${name// }" ]] && continue
  [[ "$name" == \#* ]] && continue
  [[ -n "$ONLY" && "$name" != "$ONLY" ]] && continue
  for variant in $VARIANTS; do
    [[ "$variant" != *-* ]] && continue
    base="${variant%%-*}"
    vf="$OUT/${name}__${variant}.png"; bf="$OUT/${name}__${base}.png"
    [[ -s "$vf" && -s "$bf" ]] || continue
    if [[ "$(md5 -q "$vf")" == "$(md5 -q "$bf")" ]]; then res="identical (setting had no visible effect)"; else res="differs (inspect for regression)"; fi
    printf '%s\t%s\t%s\t%s\n' "$name" "$variant" "$base" "$res" >>"$A11Y_REPORT"
  done
done <"$STATES_FILE"

echo
echo "capture_gallery: done. $(grep -c $'\tok$' "$MANIFEST" || true) frame(s) captured under:"
echo "  $OUT"
echo "capture_gallery: manifest    -> $MANIFEST"
echo "capture_gallery: a11y report -> $A11Y_REPORT"
