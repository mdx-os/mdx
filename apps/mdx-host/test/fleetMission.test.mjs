// Fleet lane recovery into a mission: when a fleet lands only partially, the
// operator can carry the stuck remainder into a governed mission with the goal,
// write scope, and checks pre-filled from the fleet plan. These cover the
// recovery triggers and the pre-filled draft (shown before anything is created).
import test from "node:test";
import assert from "node:assert/strict";
import {
  joinFleets,
  fleetNeedsRecovery,
  fleetHasIntegrationFailure,
  fleetRemainderLanes,
  prepareMissionFromFleet,
  buildMissionCreatePayload
} from "../src/lib/fleetRuns.js";

const plans = {
  fleets: [
    {
      fleet_id: "fleet_1",
      spec: "Add rate limiting across the API",
      status: "ratified",
      repo_id: "repo_api",
      repo_profile_suggested_checks: "cargo test, cargo clippy",
      execution_geometry: { requested_width: 3, stream_count: 3 },
      streams: [
        { stream_id: "s1", objective: "limiter core", write_scope: ["crates/limiter/"], checks: ["cargo test -p limiter"] },
        { stream_id: "s2", objective: "edge wiring", write_scope: ["crates/edge/"], checks: [] },
        { stream_id: "s3", objective: "docs", write_scope: ["docs/"], checks: [] }
      ]
    }
  ]
};

const runs = {
  fleet_runs: [
    {
      fleet_id: "fleet_1",
      running: false,
      finished: true,
      integration_state: "did_not_land",
      integration_detail: "no branch produced - two lanes conflicted",
      lanes: [
        { stream_id: "s1", state: "done" },
        { stream_id: "s2", state: "needs_attention", detail: "checks failed" },
        { stream_id: "s3", state: "waiting" }
      ]
    }
  ]
};

const fleet = joinFleets(plans, runs)[0];

test("a partial landing with a failed merge needs recovery", () => {
  assert.equal(fleetHasIntegrationFailure(fleet), true);
  assert.equal(fleetNeedsRecovery(fleet), true);
});

test("the remainder carries every lane that did not finish cleanly", () => {
  const remainder = fleetRemainderLanes(fleet).map((lane) => lane.streamId);
  assert.deepEqual(remainder.sort(), ["s2", "s3"]);
});

test("the prepared mission pre-fills goal, write scope, and checks from the plan", () => {
  const draft = prepareMissionFromFleet(fleet);
  assert.equal(draft.integrationFailed, true);
  assert.match(draft.goal, /Bring the fleet's work together/);
  assert.match(draft.goal, /rate limiting/);
  // Write scope comes from the remaining lanes' streams, not the done lane.
  assert.deepEqual(draft.allowedWriteScope.sort(), ["crates/edge/", "docs/"]);
  assert.ok(draft.validationCommands.includes("cargo test"));
  assert.deepEqual(draft.remainderLaneIds.sort(), ["s2", "s3"]);
});

test("the create payload joins the draft into the route's comma form", () => {
  const draft = prepareMissionFromFleet(fleet);
  const body = buildMissionCreatePayload(draft, "u1");
  assert.equal(body.actor_id, "u1");
  assert.match(body.allowed_write_scope, /crates\/edge\//);
  assert.equal(typeof body.validation_commands, "string");
  assert.ok(body.fleet_width >= 1);
  assert.ok(body.max_runtime_ms >= 60_000);
});

test("a clean, unstarted, or single-branch fleet does not trigger recovery", () => {
  const cleanFleet = joinFleets(plans, {
    fleet_runs: [
      {
        fleet_id: "fleet_1",
        running: false,
        finished: true,
        integration_state: "done",
        integration_detail: "fleet_branch=forge/fleet-1 ready",
        lanes: [
          { stream_id: "s1", state: "done" },
          { stream_id: "s2", state: "done" },
          { stream_id: "s3", state: "done" }
        ]
      }
    ]
  })[0];
  assert.equal(fleetNeedsRecovery(cleanFleet), false);
});
