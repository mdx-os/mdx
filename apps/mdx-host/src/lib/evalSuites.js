// Evals: the checks the company runs on itself. The suites are generated
// contract truth - one gate per loop runner, complete without the kernel.
// What each suite checks is humanized here; the exact tokens stay for the
// per-suite disclosure. Verdicts are runtime truth and live in the page's
// server load (receipt ids), never invented here.
import aegis from "../../../../generated/eval-suites/aegis_scanner_agent.json";
import charter from "../../../../generated/eval-suites/charter_attestation_agent.json";
import evals from "../../../../generated/eval-suites/evals_runner_agent.json";
import forge from "../../../../generated/eval-suites/forge_orchestrator_agent.json";
import product from "../../../../generated/eval-suites/product_shaping_agent.json";
import talent from "../../../../generated/eval-suites/talent_autonomy_agent.json";

// One human line per suite: what part of the company it checks, in the
// words a person would use, plus the receipt-id prefix its runs leave.
const SUITE_META = {
  evals_runner_agent: {
    name: "The quality loop itself",
    line: "Checks that every run earns a verdict before anything downstream trusts it - the quality loop holding itself to its own bar.",
    prefix: "evals"
  },
  aegis_scanner_agent: {
    name: "Security sweeps",
    line: "Checks that every security finding traces back to a real scan, and that fixes stay tied to what they answer.",
    prefix: "aegis"
  },
  charter_attestation_agent: {
    name: "Promises kept",
    line: "Checks that what the company said it would do is attested with evidence, and exceptions are reviewed, not waved through.",
    prefix: "charter"
  },
  forge_orchestrator_agent: {
    name: "How work gets built",
    line: "Checks that a build only moves forward on an accepted spec, and never hires a worker on its own - that needs Talent's say-so.",
    prefix: "forge"
  },
  product_shaping_agent: {
    name: "What to build next",
    line: "Checks that shaped bets trace to real signal, and that direction waits for a person to ratify it - product proposes, never decides.",
    prefix: "product"
  },
  talent_autonomy_agent: {
    name: "Who is allowed to act",
    line: "Checks that authority is fully accounted for before anyone is authorized, and that no worker runs without vetting first.",
    prefix: "talent"
  }
};

// A check token becomes a short human phrase. We keep meaning honest and
// readable; the exact token stays visible in the suite's quiet disclosure.
const CHECK_PHRASES = [
  [/^policy_decision_before/, "A vetting decision is recorded before the work happens"],
  [/^receipt_for_each/, "Every step in the loop leaves a receipt"],
  [/stays_within_local_budget$/, "The loop stays inside its local budget"],
  [/credential_requires_eval_verdict/, "No keys are issued without a passing verdict"],
  [/memory_write_requires_consolidation_gate/, "Memory only writes after the consolidation gate"],
  [/charter_record_links_evidence_receipt/, "Every charter record points back to its evidence"],
  [/concierge_answer_cites_receipts/, "Answers cite the receipts they stand on"],
  [/finding_requires_scan_receipt/, "Every finding points back to a real scan"],
  [/charter_record_links_finding_receipt/, "Charter records link to the finding behind them"],
  [/remediation_plan_links_charter_receipt/, "Fix plans link to the obligation they answer"],
  [/attestation_links_obligation_receipt/, "Each attestation links to the obligation it covers"],
  [/charter_record_links_attestation_receipt/, "Charter records link to their attestation"],
  [/exception_review_links_charter_receipt/, "Exceptions are reviewed and linked, not waved through"],
  [/stage_transition_links_spec_receipt/, "Each build stage links to its accepted spec"],
  [/delegation_request_blocks_without_talent_authority/, "A build cannot delegate without Talent's authority"],
  [/forge_never_spawns_worker_directly/, "A build never hires a worker on its own"],
  [/shaped_bet_links_signal_receipt/, "Each shaped bet links to the signal behind it"],
  [/handoff_request_blocks_before_human_ratification/, "Handoffs wait for a person to ratify them"],
  [/product_never_sets_direction_directly/, "Product proposes direction; it never sets it"],
  [/authorization_consumes_all_authority_receipts/, "Authorization accounts for every authority request"],
  [/credential_check_precedes_worker_spawn_request/, "Vetting comes before any request to run a worker"],
  [/talent_blocks_before_worker_runtime/, "No worker runs until vetting clears"]
];

function humanizeCheck(token) {
  for (const [pattern, phrase] of CHECK_PHRASES) {
    if (pattern.test(token)) {
      return phrase;
    }
  }
  // Fail-soft: a readable fallback from the token, never a raw enum on chrome.
  return token
    .replace(/_/g, " ")
    .replace(/\b\w/, (ch) => ch.toUpperCase());
}

function buildSuite(raw) {
  const meta = SUITE_META[raw.loop] ?? {
    name: humanizeCheck(String(raw.loop).replace(/_agent$/, "")),
    line: "A registered gate this company holds itself to.",
    prefix: String(raw.loop).replace(/_agent$/, "")
  };
  const tests = Array.isArray(raw.tests) ? raw.tests : [];
  return {
    id: raw.loop,
    name: meta.name,
    line: meta.line,
    prefix: meta.prefix,
    checkCount: tests.length,
    checks: tests.map((token) => ({ token, phrase: humanizeCheck(token) }))
  };
}

export function evalSuites() {
  return [evals, talent, forge, product, aegis, charter].map(buildSuite);
}

// Given the receipt ids the install has on record, attach a verdict to each
// suite. A suite with run evidence reads as "checked"; without, it is
// honestly "not yet run" - never green by default.
export function withVerdicts(suites, receiptIds) {
  const ids = Array.isArray(receiptIds) ? receiptIds : [];
  return suites.map((suite) => {
    const runs = ids.filter((id) => id.startsWith(suite.prefix));
    return {
      ...suite,
      runCount: runs.length,
      lastRuns: runs.slice(-3).reverse()
    };
  });
}
