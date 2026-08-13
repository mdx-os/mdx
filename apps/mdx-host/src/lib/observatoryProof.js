// Observatory translation. The chain is the company's memory of what
// actually happened; these helpers turn the kernel's counts and receipt
// ids into human sentences without inventing anything. Receipt ids carry
// a global anchor counter in their numeric suffix, so recency across
// surfaces falls out of the ids themselves - no wall-clock claims.
import { receiptOrder } from "./homeToday.js";

const LATEST_SHOWN = 6;

// loop_id tokens are machine words; say what landed in plain language.
// Unknown tokens degrade to a tidied phrase, never a raw ENUM.
const LOOP_PHRASES = {
  message_thread_message: "a message was sent to the team",
  thread_message: "a message was sent to the team",
  twin_session_draft: "Twin answered a question",
  session_draft: "Twin answered a question",
  pages_publish: "a page joined the library",
  pages_page: "a page joined the library",
  forge_build_request: "a build was requested in Forge",
  build_request: "a build was requested in Forge",
  charter_record: "a charter was recorded",
  memory_record: "the company remembered something",
  policy_decision: "a decision passed review",
  eval_verdict: "a check returned its verdict"
};

export function humanLoop(loopId) {
  const token = String(loopId ?? "").trim();
  if (!token) {
    return "something happened";
  }
  if (LOOP_PHRASES[token]) {
    return LOOP_PHRASES[token];
  }
  const tidy = token.replace(/[._-]+/g, " ").trim();
  return tidy.length > 0 ? tidy : "something happened";
}

function plural(count, one, many) {
  return count === 1 ? one : many;
}

// The hero sentence: how much the company's memory holds, in one breath.
// Null when nothing is on the chain yet, so the page can invite instead.
export function chainHeadline(receiptCount) {
  const count = Number(receiptCount ?? 0);
  if (count <= 0) {
    return null;
  }
  return `${count.toLocaleString()} ${plural(count, "thing", "things")} the company did, each with the proof one tap away.`;
}

// The chapters of the record: counts that are worth their own line, in
// human words. Empty when nothing has accrued, so we never recite zeros.
export function chainChapters(observatory) {
  if (!observatory) {
    return [];
  }
  const chapters = [];
  const policy = Number(observatory.policy_decision_count ?? 0);
  const evals = Number(observatory.eval_verdict_count ?? 0);
  const charter = Number(observatory.charter_record_count ?? 0);
  const memory = Number(observatory.memory_record_count ?? 0);

  if (policy > 0) {
    chapters.push({
      label: "Decisions reviewed",
      line: `${policy.toLocaleString()} ${plural(policy, "action passed", "actions passed")} review before it ran.`,
      token: "policy_decision_count"
    });
  }
  if (evals > 0) {
    chapters.push({
      label: "Work checked",
      line: `${evals.toLocaleString()} ${plural(evals, "check", "checks")} returned a verdict on the work.`,
      token: "eval_verdict_count"
    });
  }
  if (charter > 0) {
    chapters.push({
      label: "Promises kept",
      line: `${charter.toLocaleString()} ${plural(charter, "charter", "charters")} the company holds itself to.`,
      token: "charter_record_count"
    });
  }
  if (memory > 0) {
    chapters.push({
      label: "Remembered",
      line: `${memory.toLocaleString()} ${plural(memory, "thing", "things")} the company kept to learn from.`,
      token: "memory_record_count"
    });
  }
  return chapters;
}

// The latest handful of receipts as quiet, real ids - newest first by the
// anchor order in their suffix. Ids are the content of this surface, so we
// keep them, but only a handful and with an honest "and N more" tail.
// A receipt id carries its kind in the prefix; the chain reads as what
// happened, in words, with the raw links one tap away.
const RECEIPT_LINES = [
  ["twin_session", "A companion conversation was saved"],
  ["message_thread", "A message was kept"],
  ["message", "Message work went on the record"],
  ["strategy_ratification", "Strategy work went on the record"],
  ["product_ratification", "A call on a bet went on the record"],
  ["forge_build", "Build work moved forward"],
  ["forge", "Forge work went on the record"],
  ["pages", "A page moved"],
  ["publication", "A page was published"],
  ["talent", "Talent work went on the record"],
  ["policy_decision", "An action passed review"],
  ["harness", "A loop run finished"],
  ["charter", "The company held itself to its word"]
];

function lineFor(id) {
  const value = String(id);
  for (const [prefix, line] of RECEIPT_LINES) {
    if (value.startsWith(prefix)) {
      return line;
    }
  }
  return "Work went on the record";
}

export function latestReceipts(observatory) {
  const ids = Array.isArray(observatory?.receipt_ids) ? observatory.receipt_ids : [];
  if (ids.length === 0) {
    return { items: [], more: 0 };
  }
  const sorted = [...ids].sort((a, b) => receiptOrder(b) - receiptOrder(a));
  const items = sorted
    .slice(0, LATEST_SHOWN)
    .map((id) => ({ id: String(id), line: lineFor(id) }));
  return { items, more: Math.max(0, ids.length - items.length) };
}

// Recent human moments, reusing the Home feed shape: real things someone
// said, newest first by receipt anchor. This is the warm texture beneath
// the counts - the chain in people's own words.
export function recentActivity({ twinDrafts = [], messages = [] } = {}, limit = 5) {
  const moments = [];
  for (const draft of twinDrafts) {
    if (!draft?.draft_text) {
      continue;
    }
    moments.push({
      text: draft.draft_text,
      meta: "Twin answered a question",
      receiptId: draft.answer_receipt_id ?? draft.draft_receipt_id ?? "",
      order: receiptOrder(draft.answer_receipt_id ?? draft.draft_receipt_id)
    });
  }
  for (const message of messages) {
    if (!message?.body) {
      continue;
    }
    const rawActor = String(message.actor_display_name ?? message.actor_id ?? "someone").replace(/^[a-z]+:/, "");
    const actor = rawActor === "local_user" || rawActor === "local user" ? "you" : rawActor;
    moments.push({
      text: message.body,
      meta: `${actor} wrote to the team`,
      receiptId: message.message_receipt_id ?? "",
      order: receiptOrder(message.message_receipt_id)
    });
  }
  moments.sort((a, b) => b.order - a.order);
  return moments.slice(0, limit);
}

// One human sentence for the latest run, or null. Loop tokens become
// readable; status stays honest but plain.
export function latestRunLine(latestRun) {
  if (!latestRun || (!latestRun.loop_id && !latestRun.status)) {
    return null;
  }
  const what = humanLoop(latestRun.loop_id);
  const status = String(latestRun.status ?? "").toLowerCase();
  if (status && status !== "ok" && status !== "success") {
    return `Most recent: ${what}.`;
  }
  return `Most recent: ${what}, recorded clean.`;
}
