// The board's column logic is the product's honesty about readiness, so
// it gets executable tests: stage falls out of records alone, verdicts
// move cards, a broken premise still reaches the human, and resolution
// beats everything.
import test from "node:test";
import assert from "node:assert/strict";
import { boardFrom, stageFor, timelineFrom } from "../src/lib/strategyBoard.js";

const proposal = (id, extra = {}) => ({
  proposal_id: id,
  proposal_receipt_id: `strategy_ratification_receipt_${id.slice(-3)}`,
  direction: `Direction ${id}`,
  ...extra
});

test("a bare one-liner sits in Signals; thinking work moves it to Shaping", () => {
  const bare = boardFrom({ proposals: { proposals: [proposal("p_001")] } });
  assert.equal(bare.cards[0].stage, "signals");

  const shaped = boardFrom({
    proposals: { proposals: [proposal("p_002", { problem: "We are losing focus." })] }
  });
  assert.equal(shaped.cards[0].stage, "shaping");
});

test("a make-or-break claim under test moves the card to Testing", () => {
  const { cards } = boardFrom({
    proposals: { proposals: [proposal("p_003")] },
    conditions: {
      conditions: [
        {
          condition_id: "c1",
          proposal_id: "p_003",
          claim: "Customers will pay.",
          make_or_break: true,
          status: "being_tested",
          updates: []
        }
      ]
    }
  });
  assert.equal(cards[0].stage, "testing");
});

test("verdicts on every make-or-break claim make the card Ready - broken included", () => {
  const ready = boardFrom({
    proposals: { proposals: [proposal("p_004")] },
    conditions: {
      conditions: [
        { condition_id: "c1", proposal_id: "p_004", claim: "A", make_or_break: true, status: "holds", updates: [] },
        { condition_id: "c2", proposal_id: "p_004", claim: "B", make_or_break: true, status: "broken", updates: [] },
        { condition_id: "c3", proposal_id: "p_004", claim: "C", make_or_break: false, status: "assumed", updates: [] }
      ]
    }
  });
  assert.equal(ready.cards[0].stage, "ready", "a broken premise is a kill decision waiting for its name");
  assert.equal(ready.cards[0].scoreboard.holds, 1);
  assert.equal(ready.cards[0].scoreboard.broken, 1);
});

test("a ratify decision for THIS card moves it to In motion; resolution beats everything", () => {
  const records = {
    proposals: { proposals: [proposal("p_005")] },
    decisions: {
      decisions: [{ proposal_id: "p_005", decision: "ratify_next_local_strategy_option", decision_receipt_id: "r1" }]
    }
  };
  assert.equal(boardFrom(records).cards[0].stage, "in_motion");

  const resolved = boardFrom({
    ...records,
    resolutions: {
      resolutions: [{ proposal_id: "p_005", outcome: "completed", learned: "It worked.", resolution_receipt_id: "r2" }]
    }
  });
  assert.equal(resolved.cards[0].stage, "resolved");
});

test("hold and more-evidence calls do not move a card to In motion", () => {
  const { cards } = boardFrom({
    proposals: { proposals: [proposal("p_006", { problem: "x" })] },
    decisions: {
      decisions: [{ proposal_id: "p_006", decision: "hold_current_direction", decision_receipt_id: "r1" }]
    }
  });
  assert.equal(cards[0].stage, "shaping");
});

test("stageFor never invents readiness from non-make-or-break conditions", () => {
  const card = {
    resolution: null,
    ratified: false,
    problem: "",
    whereWePlay: "",
    howWeWin: "",
    conditions: [{ makeOrBreak: false, status: "holds" }]
  };
  assert.equal(stageFor(card), "shaping", "ordinary claims shape a card but never declare it ready");
});

test("the timeline tells the story newest-first from receipt order", () => {
  const events = timelineFrom({
    proposals: { proposals: [proposal("p_007")] },
    conditions: {
      conditions: [
        {
          condition_id: "c1",
          proposal_id: "p_007",
          claim: "It matters.",
          make_or_break: true,
          status: "holds",
          stated_receipt_id: "strategy_ratification_receipt_010",
          updates: [
            { update_receipt_id: "strategy_ratification_receipt_020", status: "holds", evidence_ref: "page_x", note: "Checked." }
          ]
        }
      ]
    }
  });
  assert.ok(events.length >= 3);
  assert.ok(events[0].order >= events[events.length - 1].order, "newest first");
  assert.ok(events.some((event) => /held up/.test(event.line)));
});
