// A kernel status word is system truth, but it must never read as an alarm
// when the truth is "set up later" or "nothing happened yet". These map a
// raw status string to a calm semantic tone and a human label, so by-design
// state renders as the dashed-grey "bydesign" badge (intentional, not broken)
// and only a genuine fault gets the danger tone. The raw value always stays
// available one disclosure away - this only changes how the first read frames it.

export function statusTone(raw) {
  const s = String(raw ?? "").toLowerCase();
  if (/(error|failed|unreachable|down|broken|fault|denied|blocked)/.test(s)) {
    // "blocked by design" is a safety success, not a fault - exclude it.
    if (/by design|by-design/.test(s)) return "bydesign";
    return "danger";
  }
  if (/(missing|not instrumented|not set|not wired|pending|waiting|empty|unconfigured|absent)/.test(s)) {
    return "bydesign";
  }
  if (/(ok|steady|live|ready|observed|complete|checked|healthy|done|pass|signed|verified|loaded|enforced|active|required)/.test(s)) {
    return "ok";
  }
  return "neutral";
}

export function statusLabel(raw) {
  const s = String(raw ?? "").toLowerCase().trim();
  const map = {
    missing: "Not set up yet",
    "not instrumented": "Not measured yet",
    "waiting for kernel": "Waiting for the local server",
    empty: "Nothing recorded yet",
    "env missing": "Not connected yet",
    "env ready": "Connected"
  };
  if (map[s]) return map[s];
  // A raw opcode-style status (LOCAL-PAGES-RUNTIME-MISSING-...) reads as alarm.
  // If it carries "missing", surface the calm by-design label instead.
  if (/missing/.test(s)) return "Not set up yet";
  return String(raw ?? "")
    .replace(/[-_]+/g, " ")
    .toLowerCase()
    .replace(/^\w/, (c) => c.toUpperCase());
}
