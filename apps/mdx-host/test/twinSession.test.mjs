import assert from "node:assert/strict";
import test from "node:test";

import { answerFirstPromptShape } from "../src/lib/twinSession.js";

test("Twin requires a direct answer before optional proposals", () => {
  const shape = answerFirstPromptShape("keep it concise", "; stay in the strategy lane");
  assert.match(shape, /^keep it concise;/);
  assert.match(shape, /answer the person's request directly/);
  assert.match(shape, /Never return only a tool call/);
  assert.match(shape, /stay in the strategy lane$/);
});
