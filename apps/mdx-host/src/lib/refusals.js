// One voice for failure. Kernel refusal reasons are engineering codes
// (snake_case) by design - the ledger keeps the precise truth - but a person
// mid-task needs what happened and what to do next. Known codes get real
// copy; unknown codes get humanized instead of leaking raw; and the generic
// fallback distinguishes "MDx said no" from "MDx did not answer".
const KNOWN = {
  production_missing_trusted_session:
    "You need a signed-in session for this in production. Locally, restart the stack to refresh the session.",
  pending_mdx_quality_gates: "MDx's own quality gates have to pass first. Check Review for what is waiting.",
  too_long: "That text is longer than this record allows. Trim it and try again.",
  missing_field: "Something required was left empty. Fill every field and try again.",
  repo_intake_missing: "This repo has not finished intake yet. Open it in Forge and let the scan finish.",
  candidate_selection_missing: "Pick which run's change to take forward first.",
  source_host_credentials_missing:
    "No credentials for the code host yet. Connect them in setup, or copy the branch and push it yourself."
};

function humanizeCode(code) {
  const words = code.replaceAll("_", " ").trim();
  return words.charAt(0).toUpperCase() + words.slice(1) + ".";
}

export function refusalLine(reasonOrPacket, { fallback = "That did not land. Try again.", kernelDown = false } = {}) {
  if (kernelDown) {
    return "MDx is not answering right now. It may be restarting - this page reconnects on its own.";
  }
  const reason = String(
    typeof reasonOrPacket === "string" ? reasonOrPacket : (reasonOrPacket?.reason ?? "")
  ).trim();
  if (!reason) return fallback;
  if (KNOWN[reason]) return KNOWN[reason];
  // A bare code (snake_case, no spaces) must never reach a person raw.
  if (/^[a-z0-9_]+$/.test(reason) && reason.includes("_")) return humanizeCode(reason);
  return reason;
}
