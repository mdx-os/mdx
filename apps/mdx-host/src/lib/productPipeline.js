// The product pipeline: five stages derived from records, one altitude
// below the strategy board. A bet's column falls out of what exists on
// the chain - its shaped anatomy, its assumptions and their verdicts,
// the human's backing decision, the resolution. Nothing stores a stage.

import { normalizeItems } from "./workPlane.js";

export const PIPELINE_STAGES = [
  { key: "radar", label: "Radar", cue: "Spotted - one sentence, not yet a bet." },
  { key: "shaping", label: "Shaping", cue: "Earning its anatomy and its claims." },
  { key: "ready", label: "Ready to back", cue: "Spelled out, claims judged. Your call." },
  { key: "in_build", label: "In build", cue: "Backed - now it needs its proof." },
  { key: "landed", label: "Landed", cue: "Done or killed - with the metric and the lesson." }
];

// The anatomy a bet must spell out before it deserves a backing call.
// Each section names itself so the meter can say what is missing.
export function sectionsFor(bet) {
  return [
    { key: "for_whom", label: "For whom", done: Boolean(bet.forWhom) },
    { key: "success_metric", label: "Success looks like", done: Boolean(bet.successMetric) },
    { key: "kill_condition", label: "We stop if", done: Boolean(bet.killCondition) },
    { key: "slice", label: "What ships first", done: Boolean(bet.slice) },
    { key: "direction_ref", label: "The direction it serves", done: Boolean(bet.directionRef) },
    {
      key: "conditions",
      label: "What would have to be true",
      done: bet.conditions.some((condition) => condition.makeOrBreak)
    }
  ];
}

export function stageFor(bet) {
  if (bet.resolution) {
    return "landed";
  }
  if (bet.backed) {
    return "in_build";
  }
  const sections = sectionsFor(bet);
  const spelledOut = sections.every((section) => section.done);
  const makeOrBreak = bet.conditions.filter((condition) => condition.makeOrBreak);
  const judged =
    makeOrBreak.length > 0 &&
    makeOrBreak.every(
      (condition) => condition.status === "holds" || condition.status === "broken"
    );
  if (spelledOut && judged) {
    return "ready";
  }
  const touched =
    sections.some((section) => section.done) || bet.conditions.length > 0;
  return touched ? "shaping" : "radar";
}

function receiptOrder(receiptId) {
  const match = /(\d+)$/.exec(String(receiptId ?? ""));
  return match ? Number(match[1]) : 0;
}

export function pipelineFrom({ bets, conditions, decisions, resolutions, work } = {}) {
  const conditionList = Array.isArray(conditions?.conditions) ? conditions.conditions : [];
  const decisionList = Array.isArray(decisions?.decisions) ? decisions.decisions : [];
  const resolutionList = Array.isArray(resolutions?.resolutions) ? resolutions.resolutions : [];
  const betList = Array.isArray(bets?.bets) ? bets.bets : [];
  const workList = normalizeItems(work);

  const cards = betList.map((entry) => {
    const id = entry.bet_id ?? "";
    const cardConditions = conditionList
      .filter((condition) => condition.bet_id === id)
      .map((condition) => ({
        conditionId: condition.condition_id ?? "",
        claim: condition.claim ?? "",
        makeOrBreak: condition.make_or_break === true,
        status: condition.status ?? "assumed",
        evidenceRef: condition.evidence_ref ?? "",
        updates: Array.isArray(condition.updates) ? condition.updates : []
      }));
    const backed = decisionList.some(
      (decision) => decision.bet_id === id && decision.decision === "back_the_bet"
    );
    const resolution =
      resolutionList.filter((entry2) => entry2.bet_id === id).at(-1) ?? null;
    const activeWork = workList.filter(
      (item) => item.betId === id && item.status !== "done" && item.status !== "dropped"
    );
    const recordedWork = workList.filter(
      (item) => item.betId === id && item.status !== "dropped"
    );
    const card = {
      betId: id,
      bet: entry.bet ?? "",
      forWhom: entry.for_whom ?? "",
      successMetric: entry.success_metric ?? "",
      killCondition: entry.kill_condition ?? "",
      directionRef: entry.direction_ref ?? "",
      slice: entry.slice ?? "",
      sliceNot: entry.slice_not ?? "",
      signalRef: entry.signal_ref ?? "",
      receiptId: entry.bet_receipt_id ?? "",
      order: receiptOrder(entry.bet_receipt_id),
      conditions: cardConditions,
      backed,
      resolution,
      recordedWorkCount: recordedWork.length,
      needsStaffing:
        backed &&
        (recordedWork.length === 0 || activeWork.some((item) => item.ownerId === ""))
    };
    card.stage = stageFor(card);
    const sections = sectionsFor(card);
    const mob = cardConditions.filter((condition) => condition.makeOrBreak);
    card.meter = {
      done: sections.filter((section) => section.done).length,
      total: sections.length,
      missing: sections.filter((section) => !section.done).map((section) => section.label)
    };
    card.scoreboard = {
      makeOrBreak: mob.length,
      holds: mob.filter((condition) => condition.status === "holds").length,
      broken: mob.filter((condition) => condition.status === "broken").length
    };
    return card;
  });

  cards.sort((a, b) => b.order - a.order);
  const stages = PIPELINE_STAGES.map((stage) => ({
    ...stage,
    cards: cards.filter((card) => card.stage === stage.key)
  }));
  return { stages, cards };
}

// The Next-decision hero: walk the pipeline backwards and name the one
// move that matters most right now - v1's best pattern, revived.
export function heroFor(pipeline) {
  const byStage = (key) => pipeline.stages.find((stage) => stage.key === key)?.cards ?? [];
  const unstaffed = byStage("in_build").find((card) => card.needsStaffing);
  if (unstaffed) {
    const needsBreakout = unstaffed.recordedWorkCount === 0;
    return {
      kind: "staff",
      betId: unstaffed.betId,
      label: needsBreakout ? "Break out the work" : "Staff the work",
      line: needsBreakout
        ? `"${unstaffed.bet}" is backed but has no pieces of work yet.`
        : `"${unstaffed.bet}" has work waiting for an owner.`
    };
  }
  const ready = byStage("ready")[0];
  if (ready) {
    return {
      kind: "back",
      betId: ready.betId,
      label: "Make the call",
      line: `"${ready.bet}" is spelled out and its claims are judged.`
    };
  }
  const shaping = byStage("shaping")[0];
  if (shaping) {
    const next = shaping.meter.missing[0];
    return {
      kind: "shape",
      betId: shaping.betId,
      label: "Keep shaping",
      line: next
        ? `"${shaping.bet}" still needs: ${next.toLowerCase()}.`
        : `"${shaping.bet}" needs its make-or-break claims judged.`
    };
  }
  const radar = byStage("radar")[0];
  if (radar) {
    return {
      kind: "shape",
      betId: radar.betId,
      label: "Shape it",
      line: `"${radar.bet}" is on the radar - turn it into something decidable.`
    };
  }
  if (pipeline.cards.length > 0) {
    return {
      kind: "new",
      betId: "",
      label: "Shape another",
      line: "Everything in flight has what it needs. Nothing waits on you."
    };
  }
  return {
    kind: "new",
    betId: "",
    label: "Shape the first bet",
    line: "Nothing is on the radar yet. One sentence starts it."
  };
}
