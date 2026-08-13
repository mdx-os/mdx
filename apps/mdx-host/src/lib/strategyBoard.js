// The strategy board: six columns derived entirely from records. A card
// is a direction proposal; its column falls out of what exists on the
// chain - conditions and their verdicts, ratification decisions,
// resolutions. Nothing here stores a position and nothing can be
// dragged; the board is a projection of evidence, which is why it can
// never lie about readiness.

export const BOARD_COLUMNS = [
  { key: "signals", label: "Signals", cue: "Worth a look - not yet a bet." },
  { key: "shaping", label: "Shaping", cue: "Being turned into something decidable." },
  { key: "testing", label: "Testing", cue: "The make-or-break claims, being checked." },
  { key: "ready", label: "Ready to decide", cue: "The evidence is in. A person calls it." },
  { key: "in_motion", label: "In motion", cue: "Decided and moving." },
  { key: "resolved", label: "Resolved", cue: "Done, killed, or shelved - with what we learned." }
];

// A card is "shaped" once someone has done the thinking work beyond the
// one-liner: a problem statement, a field of play, or a first condition.
function isShaped(card) {
  return Boolean(
    card.problem || card.whereWePlay || card.howWeWin || card.conditions.length > 0
  );
}

export function stageFor(card) {
  if (card.resolution) {
    return "resolved";
  }
  if (card.ratified) {
    return "in_motion";
  }
  const makeOrBreak = card.conditions.filter((condition) => condition.makeOrBreak);
  if (makeOrBreak.length > 0) {
    const undecided = makeOrBreak.filter(
      (condition) => condition.status === "assumed" || condition.status === "being_tested"
    );
    if (undecided.length === 0) {
      // Every make-or-break claim has a verdict - holds or broken, the
      // human decides either way (a broken premise is a kill decision
      // waiting for its name).
      return "ready";
    }
    if (makeOrBreak.some((condition) => condition.status === "being_tested")) {
      return "testing";
    }
  }
  return isShaped(card) ? "shaping" : "signals";
}

function receiptOrder(receiptId) {
  const match = /(\d+)$/.exec(String(receiptId ?? ""));
  return match ? Number(match[1]) : 0;
}

export function boardFrom({ proposals, conditions, decisions, resolutions } = {}) {
  const conditionList = Array.isArray(conditions?.conditions) ? conditions.conditions : [];
  const decisionList = Array.isArray(decisions?.decisions) ? decisions.decisions : [];
  const resolutionList = Array.isArray(resolutions?.resolutions) ? resolutions.resolutions : [];
  const proposalList = Array.isArray(proposals?.proposals) ? proposals.proposals : [];

  const cards = proposalList.map((proposal) => {
    const id = proposal.proposal_id ?? "";
    const cardConditions = conditionList
      .filter((condition) => condition.proposal_id === id)
      .map((condition) => ({
        conditionId: condition.condition_id ?? "",
        claim: condition.claim ?? "",
        makeOrBreak: condition.make_or_break === true,
        status: condition.status ?? "assumed",
        evidenceRef: condition.evidence_ref ?? "",
        updates: Array.isArray(condition.updates) ? condition.updates : []
      }));
    const ratified = decisionList.some(
      (decision) =>
        decision.proposal_id === id &&
        decision.decision === "ratify_next_local_strategy_option"
    );
    const latestCall = decisionList.filter((decision) => decision.proposal_id === id).at(-1);
    const resolution =
      resolutionList.filter((entry) => entry.proposal_id === id).at(-1) ?? null;
    const card = {
      proposalId: id,
      title: proposal.direction ?? "",
      whyNow: proposal.why_now ?? "",
      problem: proposal.problem ?? "",
      whereWePlay: proposal.where_we_play ?? "",
      howWeWin: proposal.how_we_win ?? "",
      horizon: proposal.horizon ?? "",
      sponsor: proposal.sponsor ?? "",
      successMetric: proposal.success_metric ?? "",
      killCondition: proposal.kill_condition ?? "",
      fundingAsk: proposal.funding_ask ?? "",
      actorId: proposal.actor_id ?? "",
      receiptId: proposal.proposal_receipt_id ?? "",
      order: receiptOrder(proposal.proposal_receipt_id),
      conditions: cardConditions,
      ratified,
      latestCall: latestCall ?? null,
      resolution
    };
    card.stage = stageFor(card);
    // The scoreboard: how the make-or-break claims stand, in one glance.
    const mob = cardConditions.filter((condition) => condition.makeOrBreak);
    card.scoreboard = {
      total: cardConditions.length,
      makeOrBreak: mob.length,
      holds: mob.filter((condition) => condition.status === "holds").length,
      broken: mob.filter((condition) => condition.status === "broken").length,
      testing: mob.filter((condition) => condition.status === "being_tested").length
    };
    return card;
  });

  cards.sort((a, b) => b.order - a.order);
  const columns = BOARD_COLUMNS.map((column) => ({
    ...column,
    cards: cards.filter((card) => card.stage === column.key)
  }));
  return { columns, cards };
}

// The narrative twin: the board's events as a story, newest first. All
// strategy receipts share one counter, so order falls out of the ids.
export function timelineFrom({ proposals, conditions, decisions, resolutions } = {}) {
  const events = [];
  for (const proposal of proposals?.proposals ?? []) {
    events.push({
      order: receiptOrder(proposal.proposal_receipt_id),
      line: `A direction went on the table: "${proposal.direction}"`,
      receiptId: proposal.proposal_receipt_id ?? "",
      kind: "proposed"
    });
  }
  for (const condition of conditions?.conditions ?? []) {
    events.push({
      order: receiptOrder(condition.stated_receipt_id),
      line: `${condition.make_or_break ? "Make-or-break claim" : "Claim"} stated: "${condition.claim}"`,
      receiptId: condition.stated_receipt_id ?? "",
      kind: "condition"
    });
    for (const update of condition.updates ?? []) {
      const verb =
        update.status === "holds"
          ? "held up"
          : update.status === "broken"
            ? "broke"
            : "went under test";
      events.push({
        order: receiptOrder(update.update_receipt_id),
        line: `A claim ${verb}: "${condition.claim}"${update.note ? ` - ${update.note}` : ""}`,
        receiptId: update.update_receipt_id ?? "",
        kind: `condition_${update.status}`
      });
    }
  }
  for (const decision of decisions?.decisions ?? []) {
    const lines = {
      hold_current_direction: "The call: hold the current direction.",
      request_more_evidence: "The call: gather more evidence first.",
      ratify_next_local_strategy_option: "Decided: the direction is ratified, on the record."
    };
    events.push({
      order: receiptOrder(decision.decision_receipt_id),
      line: lines[decision.decision] ?? "A call went on the record.",
      receiptId: decision.decision_receipt_id ?? "",
      kind: "decision"
    });
  }
  for (const resolution of resolutions?.resolutions ?? []) {
    const lines = {
      archived: "Shelved, with reasons kept",
      killed: "Killed, and the lesson kept",
      completed: "Completed"
    };
    events.push({
      order: receiptOrder(resolution.resolution_receipt_id),
      line: `${lines[resolution.outcome] ?? "Resolved"}${resolution.learned ? `: ${resolution.learned}` : "."}`,
      receiptId: resolution.resolution_receipt_id ?? "",
      kind: "resolution"
    });
  }
  events.sort((a, b) => b.order - a.order);
  return events;
}
