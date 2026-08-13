// The incoming stream, read-side. Entries arrive from the kernel's
// triage projection - posted signal and feedback captures interleaved
// in arrival order, each with its latest verdict already folded. These
// helpers split open from called, name the verdicts in human words,
// and carry the single-key map for the verdict bar (Linear's rhythm:
// one key per call, auto-advance).

export const VERDICTS = [
  { key: "accepted", label: "Accept", hint: "becomes work, or a bet on the Radar", hotkey: "1" },
  { key: "duplicate", label: "Duplicate", hint: "names the entry it repeats", hotkey: "2" },
  { key: "declined", label: "Decline", hint: "a polite no, kept on the record", hotkey: "3" },
  { key: "snoozed", label: "Snooze", hint: "not now - it stays in the stream", hotkey: "h" }
];

export function verdictByHotkey(key) {
  return VERDICTS.find((verdict) => verdict.hotkey === String(key).toLowerCase()) ?? null;
}

const SOURCE_LINES = {
  feedback: "From the feedback button",
  message: "Flagged from a channel",
  signal: "From a signal",
  manual: "Pasted in by hand"
};

export function sourceLine(source) {
  return SOURCE_LINES[source] ?? SOURCE_LINES.manual;
}

export function normalizeStream(projection) {
  const rows = Array.isArray(projection?.entries) ? projection.entries : [];
  return rows
    .map((row) => ({
      entryRef: String(row?.entry_ref ?? ""),
      text: String(row?.text ?? ""),
      source: String(row?.source ?? "manual"),
      sourceRef: String(row?.source_ref ?? ""),
      receiptId: String(row?.entry_receipt_id ?? ""),
      actorId: String(row?.actor_id ?? ""),
      verdict: row?.verdict
        ? {
            verdict: String(row.verdict.verdict ?? ""),
            note: String(row.verdict.note ?? ""),
            duplicateOf: String(row.verdict.duplicate_of ?? ""),
            acceptedAs: String(row.verdict.accepted_as ?? ""),
            acceptedRecordId: String(row.verdict.accepted_record_id ?? "")
          }
        : null
    }))
    .filter((entry) => entry.entryRef !== "");
}

// Open entries wait for a call. Snoozed entries come back to the open
// queue - snooze means not now, never never.
export function openEntries(entries) {
  return entries.filter(
    (entry) => entry.verdict === null || entry.verdict.verdict === "snoozed"
  );
}

export function calledEntries(entries) {
  return entries.filter(
    (entry) => entry.verdict !== null && entry.verdict.verdict !== "snoozed"
  );
}

// One past-tense line for the history list.
export function calledLine(entry) {
  const verdict = entry.verdict;
  if (!verdict) return "";
  if (verdict.verdict === "accepted") {
    return verdict.acceptedAs === "bet"
      ? "Accepted - now a bet on the Radar"
      : "Accepted - now a piece of work";
  }
  if (verdict.verdict === "duplicate") {
    return `Duplicate of ${verdict.duplicateOf}`;
  }
  return "Declined";
}
