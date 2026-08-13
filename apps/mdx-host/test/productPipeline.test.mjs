// The pipeline's stage logic and the Next-decision hero get executable
// tests: stages fall out of records, the readiness meter names what is
// missing, and the hero always points at the one move that matters.
import test from "node:test";
import assert from "node:assert/strict";
import { heroFor, pipelineFrom, stageFor } from "../src/lib/productPipeline.js";

const bet = (id, extra = {}) => ({
  bet_id: id,
  bet_receipt_id: `strategy_ratification_receipt_${id.slice(-3)}`,
  bet: `Bet ${id}`,
  ...extra
});

const fullAnatomy = {
  for_whom: "The team",
  success_metric: "60 percent open rate",
  kill_condition: "Under 20 percent after four issues",
  slice: "One digest in #general",
  direction_ref: "strategy_direction_proposal_000001"
};

test("a bare sentence sits on the Radar; any anatomy moves it to Shaping", () => {
  const bare = pipelineFrom({ bets: { bets: [bet("b_001")] } });
  assert.equal(bare.cards[0].stage, "radar");

  const touched = pipelineFrom({ bets: { bets: [bet("b_002", { slice: "x" })] } });
  assert.equal(touched.cards[0].stage, "shaping");
});

test("ready needs the full anatomy AND judged make-or-break claims", () => {
  const spelledOnly = pipelineFrom({
    bets: { bets: [bet("b_003", fullAnatomy)] },
    conditions: {
      conditions: [
        { condition_id: "c1", bet_id: "b_003", claim: "A", make_or_break: true, status: "assumed", updates: [] }
      ]
    }
  });
  assert.equal(spelledOnly.cards[0].stage, "shaping", "an unjudged claim keeps it shaping");
  assert.equal(spelledOnly.cards[0].meter.missing.length, 0);

  const judged = pipelineFrom({
    bets: { bets: [bet("b_004", fullAnatomy)] },
    conditions: {
      conditions: [
        { condition_id: "c1", bet_id: "b_004", claim: "A", make_or_break: true, status: "holds", updates: [] }
      ]
    }
  });
  assert.equal(judged.cards[0].stage, "ready");
});

test("the meter names exactly what is missing", () => {
  const { cards } = pipelineFrom({
    bets: { bets: [bet("b_005", { slice: "x", success_metric: "y" })] }
  });
  assert.ok(cards[0].meter.missing.includes("We stop if"));
  assert.ok(cards[0].meter.missing.includes("What would have to be true"));
  assert.equal(cards[0].meter.done, 2);
});

test("backing moves a bet to In build; resolution beats everything", () => {
  const records = {
    bets: { bets: [bet("b_006", fullAnatomy)] },
    decisions: { decisions: [{ bet_id: "b_006", decision: "back_the_bet", decision_receipt_id: "r1" }] }
  };
  assert.equal(pipelineFrom(records).cards[0].stage, "in_build");

  const landed = pipelineFrom({
    ...records,
    resolutions: {
      resolutions: [{ bet_id: "b_006", outcome: "landed", metric_result: "68 percent", resolution_receipt_id: "r2" }]
    }
  });
  assert.equal(landed.cards[0].stage, "landed");
});

test("hold and send-back decisions do not move a bet to In build", () => {
  const { cards } = pipelineFrom({
    bets: { bets: [bet("b_007", { slice: "x" })] },
    decisions: { decisions: [{ bet_id: "b_007", decision: "hold_for_more_signal", decision_receipt_id: "r1" }] }
  });
  assert.equal(cards[0].stage, "shaping");
});

test("the hero walks the pipeline backwards: staffing beats backing beats shaping", () => {
  const pipeline = pipelineFrom({
    bets: { bets: [bet("b_010", fullAnatomy), bet("b_011"), bet("b_012", fullAnatomy)] },
    conditions: {
      conditions: [
        { condition_id: "c1", bet_id: "b_012", claim: "A", make_or_break: true, status: "holds", updates: [] }
      ]
    },
    decisions: { decisions: [{ bet_id: "b_010", decision: "back_the_bet", decision_receipt_id: "r1" }] }
  });
  const hero = heroFor(pipeline);
  assert.equal(hero.kind, "staff", "an unstaffed backed bet outranks everything");
  assert.equal(hero.betId, "b_010");
  assert.equal(hero.label, "Break out the work");
});

test("with an empty pipeline the hero invites the first bet", () => {
  const hero = heroFor(pipelineFrom({}));
  assert.equal(hero.kind, "new");
});

test("an active unowned work item asks for an owner", () => {
  const pipeline = pipelineFrom({
    bets: { bets: [bet("b_013", fullAnatomy)] },
    decisions: {
      decisions: [{ bet_id: "b_013", decision: "back_the_bet", decision_receipt_id: "r1" }]
    },
    work: {
      items: [{ work_item_id: "w_013", bet_id: "b_013", title: "Build it", status: "up_next" }]
    }
  });
  const hero = heroFor(pipeline);
  assert.equal(hero.kind, "staff");
  assert.equal(hero.label, "Staff the work");
});

test("dropped work does not pretend a backed bet is staffed", () => {
  const pipeline = pipelineFrom({
    bets: { bets: [bet("b_014", fullAnatomy)] },
    decisions: {
      decisions: [{ bet_id: "b_014", decision: "back_the_bet", decision_receipt_id: "r1" }]
    },
    work: {
      items: [
        {
          work_item_id: "w_014",
          bet_id: "b_014",
          owner_id: "page_runner_agent",
          title: "Discarded slice",
          status: "dropped"
        }
      ]
    }
  });
  const hero = heroFor(pipeline);
  assert.equal(hero.kind, "staff");
  assert.equal(hero.label, "Break out the work");
});

test("a resolved backed bet does not re-enter the staffing queue", () => {
  const pipeline = pipelineFrom({
    bets: { bets: [bet("b_015", fullAnatomy)] },
    decisions: {
      decisions: [{ bet_id: "b_015", decision: "back_the_bet", decision_receipt_id: "r1" }]
    },
    resolutions: {
      resolutions: [{ bet_id: "b_015", outcome: "landed", resolution_receipt_id: "r2" }]
    },
    work: {
      items: [{ work_item_id: "w_015", bet_id: "b_015", title: "Historical work" }]
    }
  });
  const hero = heroFor(pipeline);
  assert.equal(pipeline.cards[0].stage, "landed");
  assert.equal(hero.kind, "new");
});

test("stageFor never invents readiness from ordinary claims", () => {
  const card = {
    resolution: null,
    backed: false,
    forWhom: "x", successMetric: "x", killCondition: "x", slice: "x", directionRef: "x",
    conditions: [{ makeOrBreak: false, status: "holds" }]
  };
  assert.equal(stageFor(card), "shaping", "no make-or-break claim, no readiness");
});

test("with everything in flight the hero rests instead of lying", () => {
  const pipeline = pipelineFrom({
    bets: { bets: [{ bet_id: "b_020", bet: "Bet", bet_receipt_id: "r_020", for_whom: "x", success_metric: "x", kill_condition: "x", slice: "x", direction_ref: "x" }] },
    conditions: { conditions: [{ condition_id: "c1", bet_id: "b_020", claim: "A", make_or_break: true, status: "holds", updates: [] }] },
    decisions: { decisions: [{ bet_id: "b_020", decision: "back_the_bet", decision_receipt_id: "r1" }] },
    work: {
      items: [
        {
          work_item_id: "w_020",
          bet_id: "b_020",
          title: "Build the bet",
          owner_id: "page_runner_agent",
          status: "in_motion"
        }
      ]
    }
  });
  const hero = heroFor(pipeline);
  assert.ok(/Nothing waits on you/.test(hero.line));
});
