import assert from "node:assert/strict";
import test from "node:test";

import { timeoutForPath } from "../src/lib/kernelProxyTimeout.js";

test("hosted environment verification receives its isolated long-running budget", () => {
  assert.equal(timeoutForPath("/mobile/cloud/environments.json"), 30 * 60 * 1000);
  assert.equal(timeoutForPath("/mobile/cloud/environments.json?retry=1"), 30 * 60 * 1000);
});

test("the hosted verification budget does not widen ordinary proxy routes", () => {
  assert.equal(timeoutForPath("/mobile/cloud/environments-extra.json"), 15_000);
  assert.equal(timeoutForPath("/mobile/cloud/setup.json"), 15_000);
  assert.equal(timeoutForPath("/forge/work-classifications.json"), 120_000);
});
