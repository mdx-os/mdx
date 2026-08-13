// The decision graph, read-side. Every node here is a real decision the
// company already recorded - a fleet ratified, a build shipped, a bet
// resolved - projected from the receipt ledger, never asserted. The edge
// that matters is decision -> outcome: what the call actually led to. These
// helpers fold the projection into rows a person can read at a glance.

const CLASS_LABEL = {
  fleet_ratification: "Ratified a build fleet",
  ship_decision: "Ship decision",
  deployment: "Deployed to production",
  product_ratification: "Product ratification",
  bet_resolution: "Resolved a bet",
  strategy_ratification: "Ratified a strategy",
  strategy_direction: "Set a strategy direction",
  strategy_resolution: "Resolved a proposal",
  pages_approval: "Approved a page",
  policy_decision: "Policy decision",
  asserted_decision: "Decision record"
};

export function decisionClassLabel(decisionClass) {
  return CLASS_LABEL[decisionClass] ?? String(decisionClass ?? "Decision");
}

// One readable row per recorded decision, freshest first. The outcome is the
// realized result the ledger links back through a shared subject id, so the
// row can say not just what was decided but what it produced.
export function decisionRows(packet) {
  const rows = Array.isArray(packet?.decisions) ? packet.decisions : [];
  return rows
    .map((row) => {
      const outcome = row?.outcome ?? null;
      return {
        id: String(row?.id ?? ""),
        classLabel: decisionClassLabel(row?.decision_class),
        decisionClass: String(row?.decision_class ?? ""),
        status: String(row?.status ?? ""),
        title: String(row?.title ?? ""),
        subject: String(row?.subject?.id ?? ""),
        reason: String(row?.reason ?? ""),
        decidedBy: String(row?.decided_by ?? ""),
        validFrom: Number(row?.valid_from ?? 0),
        origin: String(row?.provenance?.origin ?? "derived"),
        confidence: String(row?.provenance?.confidence ?? ""),
        bodyRoute: String(row?.body_route ?? ""),
        supersededBy: String(row?.superseded_by ?? ""),
        supersedes: String(row?.supersedes ?? ""),
        touches: Array.isArray(row?.touches) ? row.touches : [],
        outcome: outcome
          ? {
              event: outcomeLine(outcome),
              detail: String(outcome?.detail ?? ""),
              receiptId: String(outcome?.receipt_id ?? "")
            }
          : null
      };
    })
    .sort((a, b) => b.validFrom - a.validFrom);
}

// A human outcome line from the realized receipt: the event word made plain,
// e.g. "run_finished" -> "build finished". No invention beyond a friendlier word.
function outcomeLine(outcome) {
  const event = String(outcome?.event ?? "").trim();
  const friendly = {
    run_finished: "build finished",
    fleet_finished: "fleet finished",
    integration_done: "integration done",
    deployed: "deployed"
  }[event];
  return friendly ?? event ?? "recorded";
}

// The corpus-level rollup the projection already computes: live vs superseded
// per decision class, plus how many decisions carry a realized outcome edge.
export function decisionSummary(packet) {
  const classes = Array.isArray(packet?.classes) ? packet.classes : [];
  return {
    count: Number(packet?.decision_count ?? 0),
    withOutcome: Number(packet?.with_outcome ?? 0),
    derived: Number(packet?.derived_count ?? 0),
    asserted: Number(packet?.asserted_count ?? 0),
    classes: classes.map((entry) => ({
      label: decisionClassLabel(entry?.class),
      live: Number(entry?.live ?? 0),
      superseded: Number(entry?.superseded ?? 0)
    }))
  };
}
