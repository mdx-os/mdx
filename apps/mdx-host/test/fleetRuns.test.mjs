// The fleet surface's read helpers: plans join with runs into one card,
// lanes cover even unstarted streams, the phase reflects where the fleet
// stands, and the reviewer's verdict splits into a line plus findings.
import test from "node:test";
import assert from "node:assert/strict";
import { joinFleets, phaseLabel, laneLabel, repairSummary, reviewSummary } from "../src/lib/fleetRuns.js";

const plans = {
  fleets: [
    {
      fleet_id: "fleet_1",
      status: "ratified",
      spec: "Build the thing.",
      justification: "Two disjoint layers.",
      planner_model: "grok-4.3",
      stream_count: 2,
      streams: [
        {
          stream_id: "s1",
          objective: "Layer one",
          write_scope: ["a.rs"],
          interface_contract: "fn one()",
          depends_on: [],
          checks: ["cargo test"]
        },
        {
          stream_id: "s2",
          objective: "Layer two",
          write_scope: ["b.rs"],
          interface_contract: "fn two()",
          depends_on: ["s1"],
          checks: []
        }
      ]
    },
    { fleet_id: "fleet_2", status: "drafted", spec: "Another.", streams: [] }
  ]
};

const runs = {
  fleet_runs: [
    {
      fleet_id: "fleet_1",
      running: false,
      finished: true,
      final_detail: "streams=2 green=2 needs_attention=0 fleet_branch=fleet/fleet_1",
      integration_state: "done",
      integration_detail: "branch=fleet/fleet_1 merged=2 wired=true",
      review_verdict: "VERDICT: ready\n1. Consider a test for the empty case.",
      lanes: [
        { stream_id: "s1", state: "done", forge_run_id: "forge_run_9", detail: "status=RUN_FINISHED_DONE" }
      ]
    }
  ]
};

test("plans join with runs into one card per fleet", () => {
  const fleets = joinFleets(plans, runs);
  assert.equal(fleets.length, 2);
  const [one, two] = fleets;
  assert.equal(one.fleetId, "fleet_1");
  assert.equal(one.ratified, true);
  assert.equal(one.finished, true);
  assert.equal(one.phase, "finished");
  assert.equal(one.fleetBranch, "fleet/fleet_1");
  assert.equal(one.integrationState, "done");
  // s2 never reported a lane event; it still gets a lane row.
  assert.equal(one.lanes.length, 2);
  assert.equal(one.lanes.find((l) => l.streamId === "s2").state, "waiting");
  assert.equal(one.lanes.find((l) => l.streamId === "s1").forgeRunId, "forge_run_9");
  // A drafted, unstarted fleet is a proposal awaiting the call.
  assert.equal(two.phase, "proposed");
  assert.equal(two.started, false);
});

test("a fleet lifecycle projection renders one latest card", () => {
  const repeatedPlans = {
    fleets: [
      { fleet_id: "fleet_repeat", status: "drafted", spec: "First draft.", streams: [] },
      { fleet_id: "fleet_repeat", status: "ratified", spec: "Ratified plan.", streams: [] },
      { fleet_id: "fleet_repeat", status: "ratified", spec: "Latest projection.", streams: [] }
    ]
  };

  const fleets = joinFleets(repeatedPlans, { fleet_runs: [] });
  assert.equal(fleets.length, 1);
  assert.equal(fleets[0].fleetId, "fleet_repeat");
  assert.equal(fleets[0].spec, "Latest projection.");
  assert.equal(fleets[0].ratified, true);
});

test("pending fleet plans remain planning until a draft lands", () => {
  const [fleet] = joinFleets(
    {
      fleets: [
        {
          fleet_id: "fleet_planning_1",
          status: "planning_delayed",
          spec: "Split this into 24 lanes.",
          execution_geometry: { requested_width: 24, stream_count: 0 },
          planning_progress: {
            stage: "planner_slow",
            detail: "The planner is taking longer than usual.",
            planner_model: "grok-4",
            attempt: 2,
            age_seconds: 95,
            stale_seconds: 31
          },
          streams: []
        }
      ]
    },
    { fleet_runs: [] }
  );

  assert.equal(fleet.phase, "planning_delayed");
  assert.equal(fleet.executionGeometry.requestedWidth, 24);
  assert.equal(fleet.planningProgress.stage, "planner_slow");
  assert.equal(fleet.planningProgress.attempt, 2);
  assert.equal(phaseLabel(fleet.phase), "Planner is taking longer");
});

test("phases and lanes speak human", () => {
  assert.equal(phaseLabel("proposed"), "Plan proposed - your call");
  assert.equal(phaseLabel("planning"), "Planning the fleet");
  assert.equal(phaseLabel("building"), "Building");
  assert.equal(laneLabel("needs_attention"), "Needs attention");
  assert.equal(laneLabel("waiting"), "Waiting");
});

test("the reviewer's verdict splits into line and findings", () => {
  const { line, findings } = reviewSummary("VERDICT: ready\n1. One thing.\n2. Another.");
  assert.equal(line, "VERDICT: ready");
  assert.equal(findings.length, 2);
  assert.deepEqual(reviewSummary(""), { line: "", findings: [], panel: false, ready: false, members: [] });
});

test("a verified repair is not mislabeled as an exhausted human interrupt", () => {
  assert.deepEqual(
    repairSummary({
      status: "ready_for_principal_review",
      revisionRepaired: true,
      laneRevisionRepaired: false,
      runtimeRecovery: {
        recoveryClass: "ready_for_human_review",
        recoveryHealth: "healthy_reviewable",
        interruptRequired: true,
        repairAttemptCount: 1,
        maxRepairAttempts: 2,
        nextAction: "review_integrated_branch"
      }
    }),
    {
      show: true,
      tone: "ok",
      line: "Forge repaired this within budget (1 of 2 attempts) - ready for your review."
    }
  );
});

test("an exhausted repair remains a human escalation", () => {
  const summary = repairSummary({
    status: "needs_principal_attention",
    revisionRepaired: false,
    laneRevisionRepaired: false,
    runtimeRecovery: {
      recoveryClass: "integration_repair_exhausted",
      recoveryHealth: "exhausted",
      interruptRequired: true,
      repairAttemptCount: 2,
      maxRepairAttempts: 2,
      nextAction: "group_integration_findings_for_human_unblock"
    }
  });
  assert.equal(summary.tone, "attention");
  assert.match(summary.line, /tried 2 of 2 repairs/);
});
