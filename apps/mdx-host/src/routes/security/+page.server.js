// The public security / trust page reads the install's own committed security
// evidence - the governed controls and hard stops (generated/security/
// mdx-security-posture.json) and the scanner policy (security/scanner-policy.json:
// which scanners run, the posture-score bar, what blocks, and the accepted
// findings with reasons). These are committed source-of-truth, so the page is
// the REAL posture, never a brochure. Import them at build time because the
// production host image contains the compiled app, not the full repository.
//
// What it deliberately does NOT show: a live posture score from an unverified
// run. Local audit runs are gitignored, and a readiness dry-run (scanners
// "planned", not executed) would report a misleading 100/0. The live score and
// trend light up only from generated/security/mdx-security-trust.json, a
// sanitized committed artifact derived from a completed executed scan. No fake
// green.
import posture from "../../../../../generated/security/mdx-security-posture.json" with { type: "json" };
import trust from "../../../../../generated/security/mdx-security-trust.json" with { type: "json" };
import policy from "../../../../../security/scanner-policy.json" with { type: "json" };

function acceptedSummary(entry) {
  if (typeof entry === "string") return { id: entry, reason: "" };
  return {
    scanner: entry.scanner ?? "",
    category: entry.category ?? "",
    id: entry.id ?? "",
    reason: entry.reason ?? ""
  };
}

function trustSummary(trust) {
  if (trust?.name !== "mdx-security-trust" || trust.current?.scanners_executed !== true) {
    return null;
  }
  return {
    generatedAt: trust.generated_at ?? "",
    current: {
      runId: trust.current.run_id ?? "",
      generatedAt: trust.current.generated_at ?? "",
      status: trust.current.status ?? "",
      postureScore: Number(trust.current.posture_score ?? 0),
      findings: trust.current.findings ?? {},
      missingRequiredScanners: Array.isArray(trust.current.missing_required_scanners)
        ? trust.current.missing_required_scanners
        : [],
      scanners: Array.isArray(trust.current.scanners) ? trust.current.scanners : [],
      sourceIntegrity: trust.current.source_integrity ?? null
    },
    baseline: {
      runId: trust.baseline?.run_id ?? "",
      postureScore: Number(trust.baseline?.posture_score ?? 0),
      findingsOpen: Number(trust.baseline?.findings_open ?? 0)
    },
    trend: trust.trend ?? {}
  };
}

export async function load() {
  // scanner-policy.json keys scanners by id, each with a category + required
  // flag. Read those directly so the page stays in sync with the policy.
  let scanners = [];
  if (policy?.scanners && typeof policy.scanners === "object" && !Array.isArray(policy.scanners)) {
    scanners = Object.entries(policy.scanners).map(([id, meta]) => ({
      id,
      category: meta?.category ?? "",
      required: meta?.required === true
    }));
  } else if (Array.isArray(policy?.scanners)) {
    scanners = policy.scanners.map((s) => ({
      id: typeof s === "string" ? s : (s.id ?? s.name ?? ""),
      category: typeof s === "string" ? "" : (s.category ?? ""),
      required: typeof s === "string" ? false : s.required === true
    }));
  }

  return {
    available: posture !== null || policy !== null,
    controls: Array.isArray(posture?.controls) ? posture.controls : [],
    hardStops: Array.isArray(posture?.hard_stops) ? posture.hard_stops : [],
    scanners,
    minPostureScore: policy?.minimum_posture_score ?? null,
    blockSeverities: Array.isArray(policy?.block_severities) ? policy.block_severities : [],
    acceptedFindings: Array.isArray(policy?.accepted_findings)
      ? policy.accepted_findings.map(acceptedSummary)
      : [],
    trust: trustSummary(trust)
  };
}
