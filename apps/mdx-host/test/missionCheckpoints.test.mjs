// Mission checkpoint controls: the operator can pause, resume, steer, and mark
// milestones on a long-horizon mission. The payload builder must match the
// kernel's allow-list (milestone events carry a milestone; steering carries a
// note), and a refusal must read as a refusal.
import test from "node:test";
import assert from "node:assert/strict";
import {
  buildCheckpointPayload,
  checkpointLanded,
  missionState,
  isMissionPaused,
  latestCheckpointLine
} from "../src/lib/missionCheckpoints.js";

test("steering carries the note as both summary and steering note", () => {
  const { body, error } = buildCheckpointPayload({
    missionId: "m1",
    event: "mission_steered",
    note: "Focus the last lane on the flaky test",
    actor: "u1"
  });
  assert.equal(error, undefined);
  assert.equal(body.checkpoint_event, "mission_steered");
  assert.equal(body.summary, "Focus the last lane on the flaky test");
  assert.equal(body.steering_note, "Focus the last lane on the flaky test");
  assert.equal(body.actor_id, "u1");
  assert.equal("milestone_id" in body, false);
});

test("steering without a note is refused before it is sent", () => {
  const result = buildCheckpointPayload({ missionId: "m1", event: "mission_steered", note: "  " });
  assert.match(result.error, /note/i);
  assert.equal(result.body, undefined);
});

test("a milestone event requires a milestone id", () => {
  const missing = buildCheckpointPayload({ missionId: "m1", event: "milestone_completed" });
  assert.match(missing.error, /milestone/i);
  const ok = buildCheckpointPayload({ missionId: "m1", event: "milestone_completed", milestoneId: "ms1" });
  assert.equal(ok.error, undefined);
  assert.equal(ok.body.milestone_id, "ms1");
  assert.equal(ok.body.checkpoint_event, "milestone_completed");
});

test("pause and resume act on the whole mission", () => {
  const paused = buildCheckpointPayload({ missionId: "m1", event: "mission_paused" });
  assert.equal(paused.error, undefined);
  assert.equal(paused.body.checkpoint_event, "mission_paused");
  assert.equal("milestone_id" in paused.body, false);
});

test("an unknown event and a missing mission are refused", () => {
  assert.match(buildCheckpointPayload({ missionId: "m1", event: "delete_mission" }).error, /unknown/i);
  assert.match(buildCheckpointPayload({ missionId: "", event: "mission_paused" }).error, /id/i);
});

test("a checkpoint lands unless the kernel refuses it", () => {
  assert.equal(checkpointLanded({ status: "MISSION_STEERED", checkpoint_receipt_id: "r1" }), true);
  assert.equal(checkpointLanded({ status: "MISSION_PAUSED_FOR_OPERATOR", checkpoint_receipt_id: "" }), true);
  assert.equal(checkpointLanded({ status: "REFUSED", reason: "unknown mission" }), false);
  assert.equal(checkpointLanded(null), false);
});

test("mission state reads as a calm human label", () => {
  assert.equal(missionState("PAUSED_FOR_OPERATOR").label, "Paused");
  assert.equal(missionState("COMPLETED_LOCAL_CHECKPOINTS").tone, "ok");
  assert.equal(missionState("SOMETHING_NEW").label, "something new");
});

test("pause state and the latest note drive the header", () => {
  assert.equal(isMissionPaused({ missionState: "PAUSED_FOR_OPERATOR" }), true);
  assert.equal(isMissionPaused({ latestCheckpointEvent: "mission_paused" }), true);
  assert.equal(isMissionPaused({ missionState: "IN_PROGRESS_LOCAL_CHECKPOINTS" }), false);
  assert.equal(
    latestCheckpointLine({ latestCheckpointEvent: "mission_steered", latestCheckpointSummary: "keep it tight" }),
    "Last note to the crew: keep it tight"
  );
});
