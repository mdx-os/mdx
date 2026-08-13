// Guided run recovery: a stopped or proof-troubled run must always read as
// "here is what happened and here are your sane moves", never a dead end. These
// cover the web port of RunRecoverySurface and the invariant the macOS side
// keeps (keepsFailedBranchRecoveryReadable).
import test from "node:test";
import assert from "node:assert/strict";
import {
  normalizeRuns,
  runRecovery,
  needsRecovery,
  keepsFailedBranchRecoveryReadable
} from "../src/lib/forgeRuns.js";
import { actions } from "../src/routes/forge/runs/+page.server.js";

function oneRun(row) {
  return normalizeRuns({ runs: [row] })[0];
}

test("a clean finished run needs no recovery banner", () => {
  const run = oneRun({
    run_id: "r_clean",
    status: "done",
    branch: "forge/clean",
    checks_passed: 3,
    checks_failed: 0,
    controls: [{ action: "ship", allowed: true }]
  });
  assert.equal(needsRecovery(run), false);
  assert.equal(runRecovery(run), null);
});

test("a running run is steered, not recovered", () => {
  const run = oneRun({ run_id: "r_live", status: "running", checks_failed: 0 });
  assert.equal(needsRecovery(run), false);
});

test("an interrupted run keeps its exact cloud resume contract visible", () => {
  const run = oneRun({
    run_id: "r_interrupted",
    status: "interrupted",
    operator_status: "needs_you",
    operator_intent: "Add one focused regression test",
    repo_id: "repo_classnames",
    selected_checks: ["npm ci", "npm test"],
    execution_backend_kind: "hosted_sandbox",
    cloud_environment_id: "cloud_classnames",
    events: [
      {
        turn: 5,
        event: "evidence_appended",
        detail: "startup found an interrupted run after check_started; explicit resume can recover its scoped workspace and transcript"
      }
    ]
  });
  assert.equal(needsRecovery(run), true);
  const surface = runRecovery(run);
  assert.equal(surface.showsResume, true);
  assert.match(surface.title, /paused after a restart/i);
  assert.match(surface.recoveryLine, /workspace and transcript are safe/i);
});

test("resume rebuilds the exact hosted run contract on the server", async () => {
  const form = new FormData();
  form.set("run_id", "forge_run_interrupted");
  const requests = [];
  const fetch = async (path, options = {}) => {
    requests.push({ path, options });
    if (path.endsWith("/projection.json")) {
      return Response.json({
        runs: [
          {
            run_id: "forge_run_interrupted",
            operator_intent: "Add one focused regression test",
            run_title: "Focused regression",
            repo_id: "repo_classnames",
            selected_checks: ["npm ci", "npm test"],
            execution_backend_kind: "hosted_sandbox",
            cloud_environment_id: "cloud_classnames"
          }
        ]
      });
    }
    return Response.json({ status: "RUN_STARTED" });
  };
  const result = await actions.resume({
    request: new Request("https://mdx.test/forge/runs?/resume", { method: "POST", body: form }),
    fetch,
    locals: { session: { user_id: "founder" } }
  });
  assert.deepEqual(result, { resumeOk: true, runId: "forge_run_interrupted" });
  assert.equal(requests.length, 2);
  assert.equal(requests[1].path, "/api/kernel/forge/runs.json");
  assert.deepEqual(JSON.parse(requests[1].options.body), {
    intent: "Add one focused regression test",
    run_title: "Focused regression",
    repo_id: "repo_classnames",
    allowed_commands: ["npm ci", "npm test"],
    actor_id: "founder",
    fleet_width: 1,
    execution_backend: "hosted_sandbox",
    cloud_environment_id: "cloud_classnames",
    requested_run_id: "forge_run_interrupted",
    resume: true
  });
});

test("a proof-caveat run that turned green stays reviewable with a revision", () => {
  const run = oneRun({
    run_id: "r_caveat_green",
    status: "done",
    operator_status: "ready_for_review",
    branch: "forge/caveat-green",
    checks_passed: 2,
    checks_failed: 1,
    controls: [{ action: "revise", allowed: true }],
    events: [
      { turn: 1, event: "check_failed", detail: "baseline_run_command cargo test exit=1" },
      { turn: 6, event: "check_passed", detail: "run_command cargo test exit=0" }
    ]
  });
  const surface = runRecovery(run);
  assert.ok(surface, "a caveat run produces a recovery surface");
  assert.equal(surface.isProofCaveat, true);
  assert.equal(surface.proofTurnedGreen, true);
  assert.match(surface.title, /turned green/i);
  assert.equal(surface.revisionControlLabel, "Request revision");
  assert.match(surface.recoveryLine, /revision/i);
  assert.equal(keepsFailedBranchRecoveryReadable(surface), true);
});

test("an unrelated setup pass does not claim the selected proof turned green", () => {
  const run = oneRun({
    run_id: "r_caveat_setup_only",
    status: "done",
    operator_status: "ready_for_review",
    branch: "forge/caveat-setup-only",
    checks_passed: 2,
    checks_failed: 1,
    events: [
      { turn: 0, event: "check_failed", detail: "baseline_run_command npm ci\\nnpm test exit=1" },
      { turn: 4, event: "check_passed", detail: "run_command npm ci exit=0" }
    ]
  });
  const surface = runRecovery(run);
  assert.ok(surface);
  assert.equal(surface.proofTurnedGreen, false);
  assert.match(surface.title, /proof caveat/i);
  assert.doesNotMatch(surface.recoveryLine, /turned green/i);
});

test("a branch whose baseline proof was red keeps a readable recovery", () => {
  const run = oneRun({
    run_id: "r_baseline_red",
    status: "done",
    operator_status: "ready_for_review",
    branch: "forge/baseline-red",
    checks_passed: 0,
    checks_failed: 1,
    controls: [{ action: "revise", allowed: true }],
    events: [{ turn: 1, event: "check_failed", detail: "baseline_run_command npm test exit=1" }]
  });
  const surface = runRecovery(run);
  assert.equal(surface.isProofCaveat, true);
  assert.equal(surface.proofTurnedGreen, false);
  assert.match(surface.title, /proof caveat/i);
  assert.equal(keepsFailedBranchRecoveryReadable(surface), true);
});

test("a stopped run with a branch offers review-and-revise, never a dead end", () => {
  const run = oneRun({
    run_id: "r_cannot",
    status: "cannot_proceed",
    branch: "forge/cannot",
    checks_passed: 0,
    checks_failed: 1,
    final_line: "status=cannot_proceed the change did not build",
    events: [{ turn: 3, event: "check_failed", detail: "run_command cargo build exit=1" }]
  });
  const surface = runRecovery(run);
  assert.ok(surface);
  assert.equal(surface.isProofCaveat, false);
  assert.equal(surface.branchIdentity, "forge/cannot");
  assert.match(surface.recoveryLine, /branch/i);
  assert.equal(surface.showsRevisionControl, true);
  assert.equal(keepsFailedBranchRecoveryReadable(surface), true);
});

test("a run with no branch leads with starting smaller", () => {
  const run = oneRun({
    run_id: "r_nobranch",
    status: "budget_exhausted",
    checks_passed: 0,
    checks_failed: 0,
    final_line: "status=budget_exhausted ran out of turns"
  });
  const surface = runRecovery(run);
  assert.ok(surface);
  assert.equal(surface.branchIdentity, "");
  assert.equal(surface.showsStartSmaller, true);
  assert.match(surface.recoveryLine, /narrower/i);
});
