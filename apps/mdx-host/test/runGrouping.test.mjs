// Run list grouping: a wide parallel build keeps one row per candidate in the
// projection, but the operator list collapses each group to one build-level
// representative so a width-N fleet does not flood the list. A deep link to a
// specific candidate still lands on that candidate.
import test from "node:test";
import assert from "node:assert/strict";
import {
  normalizeRuns,
  normalizeParallelGroups,
  runListRepresentatives,
  groupCandidates,
  groupSummaryLine,
  isWideGroup
} from "../src/lib/forgeRuns.js";

const projection = {
  parallel_execution_groups: [
    {
      primary_run_id: "run_primary",
      requested_workers: 3,
      effective_workers: 3,
      lane: "bounded_parallel_exploration",
      planned_candidate_count: 3,
      observed_candidate_count: 3,
      finished_candidate_count: 1,
      running_candidate_count: 1,
      done_candidate_count: 1,
      failed_candidate_count: 1,
      candidates: [{ run_id: "run_primary" }, { run_id: "run_c2" }, { run_id: "run_c3" }]
    }
  ],
  runs: [
    { run_id: "run_primary", status: "done", branch: "forge/a", parallel_candidate: { role: "primary", primary_run_id: "run_primary", index: 1, count: 3 } },
    { run_id: "run_c2", status: "running", parallel_candidate: { role: "candidate", primary_run_id: "run_primary", index: 2, count: 3 } },
    { run_id: "run_c3", status: "cannot_proceed", parallel_candidate: { role: "candidate", primary_run_id: "run_primary", index: 3, count: 3 } },
    { run_id: "run_single", status: "done", branch: "forge/single" }
  ]
};

const allRuns = normalizeRuns(projection);
const groups = normalizeParallelGroups(projection);

test("a wide group collapses to one representative row", () => {
  assert.equal(groups.length, 1);
  assert.equal(isWideGroup(groups[0]), true);
  const reps = runListRepresentatives(allRuns, allRuns, groups);
  const ids = reps.map((r) => r.runId);
  assert.deepEqual(ids, ["run_primary", "run_single"]);
});

test("a deep link to a candidate promotes it to lead its group", () => {
  const reps = runListRepresentatives(allRuns, allRuns, groups, ["run_c3"]);
  const ids = reps.map((r) => r.runId);
  assert.deepEqual(ids, ["run_c3", "run_single"]);
});

test("an explicitly opened candidate outranks the just-started primary", () => {
  const reps = runListRepresentatives(allRuns, allRuns, groups, ["run_c3", "run_primary"]);
  assert.deepEqual(reps.map((run) => run.runId), ["run_c3", "run_single"]);
});

test("the group exposes its candidates in lane order for the expander", () => {
  const candidates = groupCandidates(groups[0], allRuns);
  assert.deepEqual(candidates.map((c) => c.runId), ["run_primary", "run_c2", "run_c3"]);
});

test("the group summary rolls up lanes in plain words", () => {
  const line = groupSummaryLine(groups[0]);
  assert.match(line, /1 of 3 lanes done/);
  assert.match(line, /1 still working/);
  assert.match(line, /1 failed/);
});

test("a single run without a group passes through untouched", () => {
  const single = normalizeRuns({ runs: [{ run_id: "solo", status: "done" }] });
  const reps = runListRepresentatives(single, single, []);
  assert.deepEqual(reps.map((r) => r.runId), ["solo"]);
});
