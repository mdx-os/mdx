import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";

import { load } from "../src/routes/security/+page.server.js";

test("the public security page bundles its committed evidence into production", async () => {
  const data = await load();
  assert.equal(data.available, true);
  assert.equal(data.controls.length, 9);
  assert.equal(data.hardStops.length, 7);
  assert.equal(data.scanners.length, 9);
  assert.equal(data.minPostureScore, 85);
  assert.equal(data.trust?.current?.status, "passed");

  const source = readFileSync(
    new URL("../src/routes/security/+page.server.js", import.meta.url),
    "utf8"
  );
  assert.doesNotMatch(source, /readFileSync|process\.cwd/);
  assert.match(source, /generated\/security\/mdx-security-posture\.json/);
});
