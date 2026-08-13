// Aegis: the declared security posture is generated contract truth - it is
// complete without the kernel running. This helper translates the generated
// threat model and posture into people language (what is watching, what it
// looks for, what is locked by design), keeping every raw token for the
// quiet disclosures. Generated JSON is imported here, server-side only.
import posture from "../../../../generated/security/mdx-security-posture.json";
import threatModel from "../../../../generated/security/mdx-threat-model.json";

// Human one-liners for each declared control. Falls back to the generated
// control sentence when a control id is new, so the surface never goes blank.
const WATCH_LINES = {
  policy_receipt_path: "Every consequential move is checked before it runs, and leaves a receipt after.",
  tenant_rls: "Each tenant's data stays walled off in the database, by row, not by trust.",
  live_substrate_evidence: "A rail only counts as live once its provider has actually answered.",
  secret_handling: "Local proof never needs a real key, so secrets stay out of the loop.",
  durable_receipt_storage: "Receipts are written one way, through one door, so the trail can't be forged.",
  generated_security_secret_scan: "The security files themselves are scanned for anything that looks like a key.",
  companion_read_only: "The advisors stay advisors - they read and reason, they never act.",
  worker_bounds: "Hired help arrives with a scope, a budget, and an expiry, or it doesn't arrive.",
  waiver_bounds: "Any exception carries an owner, a reason, and a date it expires."
};

// Human one-liners for each modeled threat - what would go wrong, plainly.
const THREAT_LINES = {
  policy_bypass: "Something acting without being checked first.",
  receipt_forgery: "A receipt written outside the one trusted path.",
  tenant_boundary_break: "One tenant's data leaking into another's.",
  live_substrate_fake_green: "A rail claiming it's live before it really is.",
  companion_action_drift: "An advisor quietly starting to act instead of advise.",
  worker_scope_creep: "Hired help reaching past the job it was hired for.",
  legacy_topology_shadow_state: "An old system becoming a second, competing source of truth.",
  secret_exposure: "A real key slipping into a file, a log, or a receipt.",
  generated_security_secret_payload: "A key or payload hiding inside the security files."
};

function humanToken(id) {
  return String(id ?? "")
    .replace(/_/g, " ")
    .replace(/\b\w/g, (ch) => ch.toUpperCase());
}

function impactLabel(impact) {
  const text = String(impact ?? "").toLowerCase();
  if (text === "high") return "Serious if missed";
  if (text === "medium") return "Worth watching";
  return "Lower stakes";
}

export function watchers() {
  return (posture.controls ?? []).map((control) => ({
    title: humanToken(control.id),
    line: WATCH_LINES[control.id] ?? control.control,
    raw: {
      id: control.id,
      control: control.control,
      check: control.check,
      evidence: control.evidence
    }
  }));
}

export function threats() {
  const order = { high: 0, medium: 1, low: 2 };
  return (threatModel.threats ?? [])
    .slice()
    .sort((a, b) => (order[a.impact] ?? 3) - (order[b.impact] ?? 3))
    .map((threat) => ({
      title: humanToken(threat.id),
      line: THREAT_LINES[threat.id] ?? threat.description,
      impactLabel: impactLabel(threat.impact),
      serious: String(threat.impact ?? "").toLowerCase() === "high",
      raw: {
        id: threat.id,
        description: threat.description,
        impact: threat.impact,
        mitigation_check: threat.mitigation_check,
        evidence_artifact: threat.evidence_artifact
      }
    }));
}

export function hardStops() {
  return (posture.hard_stops ?? []).map((stop) => ({
    // Generated hard stops read as "no X" lines. Present the rule verbatim in
    // the disclosure; the chrome above it stays a plain human sentence.
    line: stop.replace(/^no /i, "").replace(/\b\w/, (ch) => ch.toUpperCase()),
    raw: stop
  }));
}

export const postureSource = {
  controls: posture.source,
  threats: threatModel.source
};
