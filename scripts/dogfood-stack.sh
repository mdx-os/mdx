#!/usr/bin/env sh
# The daily-driver stack for real usage. One command boots the company
# you actually live in:
#
#   - the kernel on its dogfood address with DURABLE receipts
#     (MDX_KERNEL_SNAPSHOT=1: snapshot after every governed write,
#     fail-closed restore at boot - a restart no longer erases your
#     week)
#   - your provider keys, read from .mdx-local/provider.env (gitignored;
#     you write that file yourself, it never touches the repo or any
#     transcript). Presence is reported, values never are.
#   - the product shell on its dogfood port, proxying same-origin
#   - the demo story seeded once (idempotent - a restored snapshot
#     already has it and is left alone)
#
# Default posture is ATTACH: a kernel already answering is left running
# and reported, because killing a live company mid-thought is rude.
# MDX_DOGFOOD_RESTART=1 restarts the kernel deliberately (the snapshot
# means no records are lost). Local only.
set -eu

kernel_addr="${MDX_DOGFOOD_KERNEL_ADDR:-127.0.0.1:18890}"
ui_port="${MDX_DOGFOOD_UI_PORT:-5200}"
restart="${MDX_DOGFOOD_RESTART:-0}"
kernel_url="http://$kernel_addr"
env_file=".mdx-local/provider.env"
out_dir=".mdx-local/dogfood"
mkdir -p "$out_dir"

case "$kernel_addr" in
  127.0.0.1:*|localhost:*) ;;
  *)
    echo "dogfood_stack: refusing a non-local kernel address ($kernel_addr)" >&2
    exit 1
    ;;
esac

# Your keys, your file. Sourced into the KERNEL's process environment -
# the gateway reads them there. Never echoed.
provider_loaded="absent"
if [ -f "$env_file" ]; then
  set -a
  # shellcheck disable=SC1090
  . "./$env_file"
  set +a
  provider_loaded="loaded"
fi
provider_name="${MDX_DEFAULT_MODEL_PROVIDER:-none}"
xai_key_present="absent"
[ -n "${XAI_API_KEY:-}" ] && xai_key_present="present"

if [ ! -d node_modules ] || [ ! -d apps/mdx-host/node_modules ]; then
  cat >&2 <<'EOF'
dogfood_stack: JavaScript dependencies are not installed yet.

Run one setup step first:
  pnpm install --frozen-lockfile

Then start the dogfood stack again:
  make dogfood-stack
EOF
  exit 1
fi

kernel_running=0
if curl -fsS "$kernel_url/health" >/dev/null 2>&1; then
  kernel_running=1
fi

if [ "$kernel_running" = "1" ] && [ "$restart" = "1" ]; then
  echo "dogfood_stack: restarting the kernel (snapshot keeps the records)"
  pkill -f "mdx-server serve $kernel_addr" || true
  sleep 2
  kernel_running=0
fi

if [ "$kernel_running" = "0" ]; then
  cargo build -q -p mdx-server
  # MDX_FORGE_HOST_CHECKS=1: Forge runs its allowlisted checks in the
  # throwaway worktree on this machine, reusing the warm build cache -
  # fast, the way a local coding harness runs. The worktree is the
  # isolation; the live repo is never the cwd.
  # MDX_FORGE_REASONING_EFFORT=high: run the coder at full reasoning effort
  # (the big quality lever). Set MDX_FORGE_REASONING_EFFORT= (empty) to fall
  # back to provider default if a run feels too heavy. Auto-escalate is a
  # sandbox concern and a no-op under host checks, but set for completeness.
  MDX_KERNEL_SNAPSHOT=1 MDX_FORGE_HOST_CHECKS=1 \
  MDX_FORGE_REASONING_EFFORT="${MDX_FORGE_REASONING_EFFORT:-high}" \
  MDX_FORGE_AUTO_ESCALATE="${MDX_FORGE_AUTO_ESCALATE:-1}" \
    nohup ./target/debug/mdx-server serve "$kernel_addr" \
    >"$out_dir/kernel.log" 2>&1 &
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    curl -fsS "$kernel_url/health" >/dev/null 2>&1 && break
    sleep 1
  done
  curl -fsS "$kernel_url/health" >/dev/null
  echo "dogfood_stack: kernel up at $kernel_url (snapshot=on)"
else
  echo "dogfood_stack: kernel already answering at $kernel_url - attached."
  echo "dogfood_stack: NOTE env changes (keys, snapshot) need MDX_DOGFOOD_RESTART=1"
fi

# The story seeds once; a restored snapshot already carries it.
MDX_LOCAL_API_URL="$kernel_url" sh ./scripts/local-demo-story.sh | tail -1

# The shell. Attach if it already answers.
ui_url="http://127.0.0.1:$ui_port"
if curl -fsS "$ui_url" >/dev/null 2>&1; then
  echo "dogfood_stack: shell already answering at $ui_url - attached."
else
  (cd apps/mdx-host && MDX_LOCAL_API_URL="$kernel_url" nohup npx vite dev \
    --host 127.0.0.1 --port "$ui_port" >"../../$out_dir/shell.log" 2>&1 &)
  for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15; do
    curl -fsS "$ui_url" >/dev/null 2>&1 && break
    sleep 1
  done
  # Verify, never assume: a loop that times out must not print success.
  if ! curl -fsS "$ui_url" >/dev/null 2>&1; then
    echo "dogfood_stack: shell did NOT come up at $ui_url - see $out_dir/shell.log" >&2
    exit 1
  fi
  echo "dogfood_stack: shell up at $ui_url"
fi

snapshot_state="absent"
[ -f ".mdx-local/kernel-ledger-snapshot.json" ] && snapshot_state="present"

echo ""
echo "dogfood_stack: OK"
echo "  company:   $ui_url"
echo "  kernel:    $kernel_url (durable: snapshot $snapshot_state)"
echo "  provider:  $provider_name (env file $provider_loaded, xai key $xai_key_present)"
if [ "$provider_loaded" = "absent" ]; then
  cat <<'TEMPLATE'
  to go live, create .mdx-local/provider.env yourself (it is gitignored):
    XAI_API_KEY=...
    MDX_XAI_MODEL=grok-4
    MDX_DEFAULT_MODEL_PROVIDER=xai
  then: MDX_DOGFOOD_RESTART=1 make dogfood-stack
TEMPLATE
fi
