// Needs-you read helpers: rows normalize from the kernel projection,
// ready bets join from the pipeline fold shaped like every other row,
// groups keep attention order and empty groups vanish.
import test from "node:test";
import assert from "node:assert/strict";
import {
  normalizeNeedsYou,
  readyBetRows,
  groupNeedsYou,
  totalWaiting
} from "../src/lib/needsYou.js";

const projection = {
  items: [
    { reason: "page_approval", ref: "page_a", detail: "page_a", route: "/pages", receipt_id: "r1" },
    { reason: "triage_open", ref: "t1", detail: "Link sections to sources", route: "/product-direction", receipt_id: "r2" },
    { reason: "", ref: "ghost" }
  ]
};

const pipeline = {
  stages: [
    { key: "shaping", cards: [{ betId: "b0", bet: "Not ready" }] },
    { key: "ready", cards: [{ betId: "b1", bet: "The digest bet", receiptId: "r9" }] }
  ]
};

test("rows normalize and ghost reasons drop", () => {
  const items = normalizeNeedsYou(projection);
  assert.equal(items.length, 2);
  assert.equal(items[0].route, "/pages");
});

test("ready bets join shaped like every other row", () => {
  const rows = readyBetRows(pipeline);
  assert.deepEqual(rows, [
    { reason: "ready_bet", ref: "b1", detail: "The digest bet", route: "/product-direction", receiptId: "r9" }
  ]);
});

test("groups keep attention order and empty groups vanish", () => {
  const items = [...normalizeNeedsYou(projection), ...readyBetRows(pipeline)];
  const groups = groupNeedsYou(items);
  assert.deepEqual(
    groups.map((group) => group.key),
    ["ready_bet", "page_approval", "triage_open"]
  );
  assert.equal(totalWaiting(items), 3);
});
