#!/usr/bin/env sh
# The narrated demo company. A first run used to open on empty rooms or
# whatever test junk the last session left behind; this writes a small,
# honest story through the kernel's own governed routes instead, so the
# first ten minutes show an autonomous company mid-flight:
#   - the guide welcomes the team in Message and says where to look
#   - Forge opens connected to a real demo repo, ready for its first build
#   - the library opens with three published pages
#   - Strategy keeps its one open call: the first decision belongs to the
#     person, so the story never ratifies anything for them
# Everything lands as real records on the chain, attributed to the demo
# guide agent - nothing is painted onto a surface. Local-demo only.
set -eu

api="${MDX_LOCAL_API_URL:-http://127.0.0.1:18890}"
case "$api" in
  http://127.0.0.1:*|http://localhost:*) ;;
  *)
    echo "local_demo_story: refusing to seed a non-local kernel ($api)" >&2
    exit 1
    ;;
esac

if ! curl -fsS "$api/health" >/dev/null 2>&1; then
  echo "local_demo_story: kernel is not answering at $api" >&2
  exit 1
fi

# Already told: posting the story twice would duplicate it. Channels
# live where the UI reads them - the thread-messages projection.
if curl -fsS "$api/messages/thread-messages/projection.json?channel_id=general&limit=1" \
  | jq -e '[.messages[]? | select(.actor_id == "agent:mdx_guide")] | length > 0' >/dev/null 2>&1; then
  echo "local_demo_story: OK already-seeded channel=general"
  exit 0
fi

post() {
  path="$1"
  payload="$2"
  curl -fsS -X POST "$api$path" -H 'content-type: application/json' -d "$payload"
}

say() {
  channel="$1"
  message_id="$2"
  body="$3"
  post /messages/thread-messages.json "$(jq -n \
    --arg channel "$channel" --arg id "$message_id" --arg body "$body" \
    '{channel_id: $channel, message_id: $id, body: $body, actor_id: "agent:mdx_guide"}')" >/dev/null
}

# Forge opens connected to a real repo: a tiny local git repo seeded on
# disk, then connected through the governed route. The guide's words in
# #the-build below are true because this record exists. No run is
# started here - a run executes real work, and the first build belongs
# to the person.
demo_repo_root="${MDX_DEMO_REPO_ROOT:-.mdx-local/demo/weekly-digest}"
if [ ! -d "$demo_repo_root/.git" ]; then
  mkdir -p "$demo_repo_root"
  printf '%s\n' "# Weekly digest" \
    "" \
    "The home of the weekly digest build: one Monday morning page that" \
    "gathers last week's decisions, finished work, and open questions." \
    > "$demo_repo_root/README.md"
  git -C "$demo_repo_root" init -q
  git -C "$demo_repo_root" add .
  git -C "$demo_repo_root" -c user.name="MDx Demo" -c user.email="demo@mdx.local" \
    commit -q -m "Seed the weekly digest demo repo"
fi
post /forge/repos.json "$(jq -n --arg root "$(cd "$demo_repo_root" && pwd)" '{
  repo_id: "demo_weekly_digest",
  label: "Weekly digest (demo)",
  root: $root,
  kind: "local"
}')" >/dev/null

# Three pages through the real lifecycle: draft, review, approve, publish.
publish_page() {
  slug="$1"
  title="$2"
  body_text="$3"
  draft_receipt="$(post /pages/edit-drafts.json "$(jq -n \
    --arg draft "pages_draft_demo_$slug" --arg doc "page_demo_$slug" \
    --arg title "$title" --arg body "$body_text" \
    '{draft_id: $draft, document_id: $doc, title: $title, body_text: $body}')" \
    | jq -r '.edit_draft_receipt_id // .receipt_id // empty')"
  approval_request_receipt="$(post /pages/approval-requests.json "$(jq -n \
    --arg req "pages_approval_demo_$slug" --arg doc "page_demo_$slug" \
    --arg draft "pages_draft_demo_$slug" --arg source "$draft_receipt" \
    '{approval_request_id: $req, document_id: $doc, draft_id: $draft, source_edit_draft_receipt_id: $source, requested_visibility: "internal"}')" \
    | jq -r '.approval_request_receipt_id // empty')"
  approval_decision_receipt="$(post /pages/approval-decisions/approve.json "$(jq -n \
    --arg decision "pages_approval_decision_demo_$slug" --arg request "$approval_request_receipt" \
    '{approval_decision_id: $decision, approval_request_receipt_id: $request, decision_note: "Reads true on a cold pass; publishing."}')" \
    | jq -r '.approval_decision_receipt_id // empty')"
  post /pages/publications.json "$(jq -n \
    --arg doc "page_demo_$slug" --arg decision "$approval_decision_receipt" \
    '{document_id: $doc, approval_decision_receipt_id: $decision, page_type: "knowledge"}')" >/dev/null
}

publish_page "welcome" "Welcome to your company" \
"This is your company's shared brain. The team talks in Message. What the company knows lives here in Pages. Work gets built in Forge, and every claim this product makes can show where it came from.

Start with the open call in Strategy: one direction is waiting for a person to decide it, and nothing moves until someone does. That is by design - the system proposes, people decide."

publish_page "decisions" "How decisions work here" \
"Every consequential move follows the same path: something proposes it, evidence gathers behind it, a person decides it on the record, and only then do money, hiring, builds, or releases open behind that decision.

You will see this shape everywhere - Strategy holds the direction calls, Product holds the bets, Forge holds the build sign-offs. If a door is closed, the page tells you which decision opens it."

publish_page "digest-spec" "The weekly digest - what we are building" \
"The first build waiting in Forge: a weekly digest page that lands every Monday with three sections - decisions made last week, work that finished, and the questions still open.

Its repo is already connected in Forge. Describe the build there when you are ready - nothing runs until a person asks - and when it ships, the digest will be written into Pages where everyone can read it."

# The floor wakes up: each loop runner does one real round of its job,
# so Talent opens on a working floor with every runner's receipts behind
# it - presence as record truth, never decoration.
for loop in aegis_scanner_agent charter_attestation_agent evals_runner_agent forge_orchestrator_agent product_shaping_agent talent_autonomy_agent; do
  curl -fsS -X POST "$api/run-loop/$loop" >/dev/null
done

# One bet on the Product radar, shaped from the digest spec the library
# already holds - so the pipeline opens mid-flight, with the next move
# obvious.
post /product/bet-drafts.json '{
  "bet_id": "product_bet_demo_digest",
  "bet": "Build the weekly digest that lands every Monday.",
  "for_whom": "The whole team",
  "signal_ref": "page_demo_digest-spec",
  "slice": "One digest, hand-curated sections, posted to #general"
}' >/dev/null

# The bet opens into its work: two pieces in flight, so the drawer and
# the rollup read real the first time anyone looks.
post /product/work-items.json '{
  "work_item_id": "work_item_demo_layout",
  "title": "Draft the digest layout",
  "description": "Three sections, hand-curated, one screen tall.",
  "bet_id": "product_bet_demo_digest",
  "owner_id": "local_user",
  "page_ref": "page_demo_digest-spec"
}' >/dev/null
post /product/work-item-moves.json '{
  "work_item_id": "work_item_demo_layout",
  "status": "in_motion",
  "note": "Started on the layout."
}' >/dev/null
post /product/work-items.json '{
  "work_item_id": "work_item_demo_sections",
  "title": "Write the three section openers",
  "bet_id": "product_bet_demo_digest"
}' >/dev/null

# One piece of signal waits in the incoming stream, so triage opens
# with a real call to make.
post /product/triage-entries.json '{
  "text": "Could the digest link each section to the page it came from?",
  "source": "manual",
  "source_ref": "msg_demo_welcome_001"
}' >/dev/null

# The guide speaks last, so the words describe records that now exist.
say general msg_demo_welcome_001 \
"Welcome in. This is where your company talks - people and agents share these channels, and anything worth keeping can become a page or a decision without leaving the conversation."
say general msg_demo_welcome_002 \
"Three things are already moving: Forge is connected to the weekly digest repo and ready to build, the library opens with its first three pages, and Strategy has one call waiting for a person. Start there - the company is waiting on you, not the other way around."
say the-build msg_demo_build_001 \
"The weekly digest repo is connected in Forge. Describe the build there when you are ready - nothing runs until you ask, and every step lands on the record."

echo "local_demo_story: OK channels=general,the-build pages=3 repo=demo_weekly_digest bet=product_bet_demo_digest work=2-items triage=1-open floor=6-loops-run"
