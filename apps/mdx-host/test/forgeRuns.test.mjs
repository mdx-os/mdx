// The Forge run viewer's read helpers: runs normalize from the kernel
// projection, the DXR event grammar translates into human lines, and the
// summary reflects the derived status.
import test from "node:test";
import assert from "node:assert/strict";
import {
  contextFill,
  normalizeRuns,
  summaryLine,
  statusLabel,
  isRunning,
  humanizeBlockedSummary
} from "../src/lib/forgeRuns.js";

const projection = {
  runs: [
    {
      run_id: "forge_run_1",
      work_item_id: "w1",
      status: "done",
      turns: 7,
      tool_calls: 6,
      checks_passed: 1,
      checks_failed: 0,
      branch: "forge/run-1",
      final_line: "status=RUN_FINISHED_DONE turns=7",
      events: [
        { turn: 1, event: "model_called", detail: "finish_reason=tool_calls tool_calls=1" },
        { turn: 1, event: "tool_executed", detail: "read_file crates/mdx-core/src/forge_run.rs" },
        { turn: 5, event: "tool_executed", detail: "write_file crates/mdx-core/src/forge_run.rs" },
        { turn: 6, event: "check_passed", detail: "run_command cargo build -p mdx-core exit=0" },
        { turn: 7, event: "evidence_appended", detail: "branch=forge/run-1 sha=abc" }
      ]
    }
  ]
};

test("runs normalize with their event trail", () => {
  const runs = normalizeRuns(projection);
  assert.equal(runs.length, 1);
  assert.equal(runs[0].events.length, 5);
});

test("the DXR grammar translates into human lines", () => {
  const run = normalizeRuns(projection)[0];
  const lines = run.events.map((e) => e.line);
  assert.ok(lines.some((l) => l.startsWith("Read crates/mdx-core")));
  assert.ok(lines.some((l) => l.startsWith("Edited crates/mdx-core")));
  assert.ok(lines.some((l) => l === "Check passed - cargo build -p mdx-core"));
  assert.ok(lines.some((l) => l === "Committed the work to a branch"));
});

test("the summary reflects the derived status", () => {
  const run = normalizeRuns(projection)[0];
  assert.ok(summaryLine(run).startsWith("Done in 7 turns"));
  assert.equal(statusLabel("done"), "Done - branch ready");
  assert.equal(isRunning("running"), true);
});

test("a newly admitted run does not claim zero work as progress", () => {
  assert.equal(
    summaryLine({ status: "running", turns: 0, toolCalls: 0 }),
    "Starting - waiting for the first recorded step"
  );
  assert.equal(summaryLine({ status: "running", turns: 1, toolCalls: 0 }), "Working - 1 turn");
});

test("context fill does not round known low usage to zero", () => {
  const fill = contextFill({
    latestInputTokens: 700,
    contextTelemetry: {
      latest: { input_tokens: 700, output_tokens: 20, context_window: 256000, model_id: "grok-build-0.1" },
      peak: { input_tokens: 700, context_window: 256000, model_id: "grok-build-0.1" },
      total: { input_tokens: 700, output_tokens: 20, model_calls: 1, model_count: 1 },
      models: [],
      source: "provider_usage_receipts"
    }
  });
  assert.equal(fill.label, "<1%");
  assert.equal(fill.pct, 1);
  assert.equal(fill.title, "Latest context: 700 / 256,000 input tokens");
  assert.equal(contextFill({ latestInputTokens: 700, model: "grok-build-0.1" }), null);
  assert.equal(contextFill({ latestInputTokens: 0, contextTelemetry: null }), null);
});

test("a blocked run never shows raw telemetry as its headline", () => {
  const rawTail = "turns=0 files_changed=0 check_runs=1 check_duration_ms=177 cost_cents=2";
  const line = humanizeBlockedSummary({ status: "cannot_proceed", finalLine: rawTail });
  assert.doesNotMatch(line, /cost_cents|check_duration_ms|files_changed=/);
  assert.match(line, /Forge stopped/);
});

test("a blocked run prefers a human reason with its telemetry tail stripped", () => {
  const line = humanizeBlockedSummary({
    status: "cannot_proceed",
    blockedReason:
      "selected proof is red on arrival before model turns: `npm test` exited 1. turns=0 cost_cents=2"
  });
  assert.match(line, /red on arrival/);
  assert.doesNotMatch(line, /cost_cents|turns=0/);
});

test("context fill uses peak context for mixed model runs", () => {
  const fill = contextFill({
    contextTelemetry: {
      latest: { input_tokens: 1000, output_tokens: 50, context_window: 256000, model_id: "grok-build-0.1" },
      peak: { input_tokens: 100000, output_tokens: 1500, context_window: 200000, model_id: "claude-opus-4.8" },
      total: { input_tokens: 101000, output_tokens: 1550, model_calls: 2, model_count: 2 },
      models: [
        { model_id: "grok-build-0.1", peak_input_tokens: 1000, context_window: 256000 },
        { model_id: "claude-opus-4.8", peak_input_tokens: 100000, context_window: 200000 }
      ],
      source: "provider_usage_receipts"
    }
  });
  assert.equal(fill.label, "peak 50%");
  assert.equal(fill.pct, 50);
  assert.equal(fill.title, "Peak context: 100,000 / 200,000 input tokens");
});
