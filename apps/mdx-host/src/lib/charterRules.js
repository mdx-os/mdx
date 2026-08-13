// Charter: the constitution is generated truth - the rules this company
// runs by are complete without the kernel. We read the declared security
// posture and translate each control into a human principle: what the
// company holds itself to, and the check that proves it still holds.
// Runtime truth (how many attestations are on the record) is read
// separately in +page.server.js and stays fail-soft.
import posture from "../../../../generated/security/mdx-security-posture.json";
import charterSuite from "../../../../generated/eval-suites/charter_attestation_agent.json";

// Each control becomes a principle in plain language. The promise is what
// a person can understand; the proof is the check that keeps it honest;
// raw fields stay for the per-item quiet disclosure.
const PRINCIPLE_COPY = {
  policy_receipt_path: {
    promise: "Nothing consequential happens without a recorded decision and a receipt behind it.",
    proof: "Every loop or tool action is checked against the rules first, then leaves a durable record."
  },
  tenant_rls: {
    promise: "Your company's data stays yours - walled off at the database, not just in the app.",
    proof: "Every owned table carries its tenant marker and isolation rules, with guards against quietly dropping them."
  },
  live_substrate_evidence: {
    promise: "A rail only counts as on once its live proof has actually been seen.",
    proof: "Substrates stay pending or local-only until observed provider evidence exists."
  },
  secret_handling: {
    promise: "The company runs and proves itself locally with no live keys required.",
    proof: "Deterministic local stubs are allowed; live keys are never the price of a clean local run."
  },
  durable_receipt_storage: {
    promise: "Every record is written through one trusted door, so the trail can't be forged.",
    proof: "Receipt writes route through the single storage boundary and its owned projection."
  },
  generated_security_secret_scan: {
    promise: "The company's own evidence files can never smuggle a secret.",
    proof: "Generated security artifacts stay flat, plain JSON, and free of anything that looks like a key."
  },
  companion_read_only: {
    promise: "Advisors advise - they never act on their own.",
    proof: "Companions stay read-only and grounded in the record until action rails allow more."
  },
  worker_bounds: {
    promise: "Any hired worker is bounded on every axis before it ever starts.",
    proof: "Workers require scope, expiry, budget, an allowlist, a parent, a sponsor chain, and receipts at both ends."
  },
  waiver_bounds: {
    promise: "An exception is still on the record, with an owner and an expiry.",
    proof: "Waivers require an owner, a reason, a scope, an expiry, a mitigation check, and a linked risk."
  }
};

// Vetting language replaces credential vocabulary on primary chrome.
const PROOF_LABEL = {
  "make security-check": "runtime security review",
  "make migrate-check": "database isolation review",
  "make live-turn-on-check": "live turn-on review",
  "make no-fake-green-check": "no-fake-green review",
  "make waiver-check": "exception review"
};

export function charterPrinciples() {
  const controls = Array.isArray(posture.controls) ? posture.controls : [];
  return controls.map((control) => {
    const copy = PRINCIPLE_COPY[control.id] ?? {
      promise: control.control,
      proof: "Proven by a recorded check."
    };
    return {
      id: control.id,
      promise: copy.promise,
      proof: copy.proof,
      provingLabel: PROOF_LABEL[control.check] ?? "recorded check",
      raw: {
        commitment: control.control,
        check: control.check,
        evidence: control.evidence
      }
    };
  });
}

// The hard stops are the lines the company will not cross. Stated as
// plain, confident sentences.
const NEVER_COPY = {
  "no direct action path that skips policy": "No action ever skips the rules.",
  "no receipt mint outside the storage boundary": "No record is ever written outside the one trusted door.",
  "no companion or worker standing authority": "No advisor or worker ever holds standing power.",
  "no live substrate status beyond pending without observed provider evidence":
    "No rail is ever called live without seeing its live proof.",
  "no generated security artifact with credential-looking payloads or private key material":
    "No evidence file ever carries a key.",
  "no nested, symlinked, or non-JSON generated security artifact that can bypass scanning":
    "No evidence file ever hides in a shape that dodges scanning.",
  "no security waiver without expiry and mitigation check":
    "No exception ever stands without an expiry and a mitigation check."
};

export function charterLines() {
  const stops = Array.isArray(posture.hard_stops) ? posture.hard_stops : [];
  return stops.map((stop) => ({
    line: NEVER_COPY[stop] ?? stop,
    raw: stop
  }));
}

// The attestation loop proves itself, too. These are the checks the
// charter loop must pass each time it runs - shown as what the record
// guarantees, with the raw tokens behind a disclosure.
const ATTEST_COPY = {
  policy_decision_before_charter_check: "The rules are checked before any obligation is.",
  receipt_for_each_charter_transition: "Every step of an attestation leaves its own record.",
  attestation_links_obligation_receipt: "Each attestation links back to the obligation it proves.",
  charter_record_links_attestation_receipt: "Each charter record links to its attestation.",
  exception_review_links_charter_receipt: "Each exception review links back to the charter.",
  charter_attestation_stays_within_local_budget: "The whole attestation stays within its local budget."
};

export function charterAttestationGuarantees() {
  const tests = Array.isArray(charterSuite.tests) ? charterSuite.tests : [];
  return {
    loop: charterSuite.loop ?? "charter_attestation_agent",
    guarantees: tests.map((test) => ({
      line: ATTEST_COPY[test] ?? test,
      raw: test
    }))
  };
}
