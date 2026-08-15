import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  buildExecutionWidthOptions,
  buildProofPreflightView,
  resolveExecutionTarget
} from "../src/lib/forgeComposer.js";
import { forgeKernelReachable } from "../src/lib/forgeReachability.js";

const forgePage = readFileSync(new URL("../src/routes/forge/+page.svelte", import.meta.url), "utf8");
const forgeServer = readFileSync(new URL("../src/routes/forge/+page.server.js", import.meta.url), "utf8");

test("execution location stays explicit across auto, host, and cloud", () => {
  const cloudEnvironment = { environment_id: "env_beta", ready_for_cloud_builds: true };

  assert.deepEqual(resolveExecutionTarget({ mode: "auto", cloudEnvironment }), {
    mode: "auto",
    backend: "hosted_sandbox",
    cloudEnvironmentId: "env_beta",
    blocked: false,
    label: "Auto - cloud",
    line: "Forge will run in the repo's verified isolated cloud environment."
  });
  assert.equal(resolveExecutionTarget({ mode: "local", cloudEnvironment }).backend, "local");
  assert.equal(resolveExecutionTarget({ mode: "cloud" }).blocked, true);
});

test("hosted Forge never advertises a local-only playground", () => {
  assert.match(forgeServer, /playgroundAvailable: !verifiedHostSessionRequired\(\)/);
  assert.match(forgePage, /data\.playgroundAvailable === true/);
  assert.match(forgePage, /Hosted Forge needs a connected repo/);
});

test("Forge uses the dedicated layout liveness result instead of a product projection", () => {
  assert.equal(forgeKernelReachable({ reachable: true }), true);
  assert.equal(forgeKernelReachable({ reachable: false }), false);
  assert.match(forgeServer, /const parentData = await parent\(\)/);
  assert.doesNotMatch(forgeServer, /const kernelReachable = runs !== null/);
  assert.match(forgePage, /MDx is taking longer than expected/);
  assert.match(forgePage, /data\.playgroundAvailable/);
});

test("a bounded queue exposes governed fleet widths above active workers", () => {
  const options = buildExecutionWidthOptions({
    workerCapacity: 4,
    queueDepthLimit: 200
  });

  assert.deepEqual(options.map((option) => option.value), [1, 4, 8, 16, 24, 48]);
  assert.equal(options.find((option) => option.value === 8)?.line, "governed fleet");
});

test("no queue never advertises work beyond active capacity", () => {
  const options = buildExecutionWidthOptions({
    workerCapacity: 4,
    queueDepthLimit: 0
  });

  assert.deepEqual(options.map((option) => option.value), [1, 4]);
});

test("verified cloud readiness supersedes Render-host toolchain gaps", () => {
  const view = buildProofPreflightView({
    isExternalRepo: true,
    selectedChecks: "cargo test",
    selectedCloudEnvironment: { ready_for_cloud_builds: true },
    proofPreflight: {
      status: "SETUP_REQUIRED",
      selected_command: "cargo test",
      missing_readiness: ["tool:cargo=missing", "check:cargo_test=missing"]
    }
  });

  assert.equal(view.tone, "ready");
  assert.equal(view.command, "cargo test");
  assert.deepEqual(view.missing, []);
  assert.match(view.line, /AgentCore environment/);
});
