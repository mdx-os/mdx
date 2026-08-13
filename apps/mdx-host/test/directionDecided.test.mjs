// The decided state on Strategy and Product is the loop's step one: it
// gates the decision room. Both helpers once counted the packet's
// receipt_ids - hundreds of harness evidence receipts - as a human call,
// so the pages claimed "decided" while the kernel's decisions projection
// said decision_count: 0, and the only verb on the surface disappeared.
// These tests pin the honest rule: only the ratification-decisions
// projection can mark a direction or a bet decided.
import test from "node:test";
import assert from "node:assert/strict";
import { directionFrom } from "../src/lib/strategyDirection.js";

const strategyPacket = {
  proposal_id: "strategy_local_ratification_001",
  question: "What strategic option needs human ratification?",
  options: ["hold_current_direction", "request_more_evidence", "ratify_next_local_strategy_option"],
  blocked_actions: ["set_company_direction"],
  ratification_required: true,
  receipt_ids: Array.from({ length: 422 }, (_, i) => `harness_pipeline_receipt_${i}`),
  source_contracts: ["generated/strategy/strategy-ratification-surface.json"]
};

test("evidence receipts alone never mark a direction decided", () => {
  const direction = directionFrom(strategyPacket, { decision_count: 0, decisions: [] });
  assert.equal(direction.decided, false, "422 harness receipts are not a human call");
  assert.equal(direction.decisionReceiptIds.length, 0);
});

test("a missing decisions projection reads as undecided, never invented", () => {
  const direction = directionFrom(strategyPacket, null);
  assert.equal(direction.decided, false);
});

test("only the committing option marks a direction decided", () => {
  const held = directionFrom(strategyPacket, {
    decisions: [{ decision: "hold_current_direction", decision_receipt_id: "r1" }]
  });
  assert.equal(held.decided, false, "holding keeps the question open");

  const ratified = directionFrom(strategyPacket, {
    decisions: [
      { decision: "hold_current_direction", decision_receipt_id: "r1" },
      { decision: "ratify_next_local_strategy_option", decision_receipt_id: "r2" }
    ]
  });
  assert.equal(ratified.decided, true);
  assert.deepEqual(ratified.decisionReceiptIds, ["r2"], "the record shows the deciding receipt only");
});

test("with evidence in but no call, human ratification is the active step", () => {
  const direction = directionFrom(strategyPacket, { decisions: [] });
  const active = direction.motion.stages[direction.motion.activeIndex];
  assert.equal(active.key, "ratified", "the company is waiting on a person, and says so");
});
