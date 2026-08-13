import test from "node:test";
import assert from "node:assert/strict";
import { forgeNextAction, isComposerPrimary } from "../src/lib/forgeNextAction.js";

test("a human decision outranks every other Forge move", () => {
  const action = forgeNextAction({
    decisions: [{ label: "2 changes ready", href: "/forge/review", line: "Branches are waiting." }],
    runs: [{ runId: "r1", status: "running", operatorStatus: "working" }]
  });
  assert.equal(action.kind, "decision");
  assert.equal(action.href, "/forge/review");
  assert.equal(isComposerPrimary(action), false);
});

test("the primary decision keeps the rest of the waiting load visible", () => {
  const action = forgeNextAction({
    decisions: [
      { kind: "fleet_repair_escalation", count: 2, label: "2 fleets to unblock", href: "/forge/fleets" },
      { kind: "ship_call", count: 3, label: "3 changes ready", href: "/forge/review" }
    ]
  });
  assert.equal(action.kind, "decision");
  assert.equal(action.additionalWaiting, 3);
  assert.equal(action.queueHref, "/needs-you");
});

test("projected priority and blockers survive the next-move compression", () => {
  const action = forgeNextAction({
    decisions: [
      { kind: "ship_call", priority: 40, label: "Review change", href: "/forge/review" },
      {
        kind: "ratify_plan",
        priority: 30,
        label: "Approve fleet split",
        href: "/forge/fleets",
        actionState: "blocked",
        blockerCodes: ["missing_proof"]
      }
    ]
  });
  assert.equal(action.label, "Approve fleet split");
  assert.equal(action.actionState, "blocked");
  assert.deepEqual(action.blockerCodes, ["missing_proof"]);
});

test("a reviewable branch outranks active work", () => {
  const action = forgeNextAction({
    runs: [
      { runId: "active", status: "running", operatorStatus: "working" },
      { runId: "ready", status: "done", operatorStatus: "ready_for_review", branch: "forge/ready" }
    ]
  });
  assert.equal(action.kind, "review");
  assert.equal(action.href, "/forge/review?run_id=ready");
});

test("a reviewable branch outranks ordinary recovery", () => {
  const action = forgeNextAction({
    runs: [
      { runId: "stopped", status: "cannot_proceed" },
      { runId: "ready", status: "done", operatorStatus: "ready_for_review", branch: "forge/ready" }
    ]
  });
  assert.equal(action.kind, "review");
  assert.equal(action.href, "/forge/review?run_id=ready");
});

test("a stopped build becomes a readable recovery move", () => {
  const action = forgeNextAction({
    runs: [{ runId: "needs space", status: "cannot_proceed", runTitle: "Repair login" }]
  });
  assert.equal(action.kind, "recovery");
  assert.equal(action.label, "Repair login");
  assert.equal(action.href, "/forge/runs?run_id=needs%20space");
});

test("an empty Forge keeps the composer as the primary action", () => {
  const action = forgeNextAction();
  assert.equal(action.kind, "start");
  assert.equal(action.primary, false);
  assert.equal(isComposerPrimary(action), true);
});
