// The work plane's read helpers get executable tests: items normalize
// from the kernel projection, the rollup line counts only honest
// denominators (dropped work leaves the count), my-work rests finished
// items, and the next-move ladder stops at done.
import test from "node:test";
import assert from "node:assert/strict";
import {
  normalizeItems,
  itemsForBet,
  rolloutLineFor,
  myWork,
  nextStatusFor,
  ownedByAgent,
  statusLabel
} from "../src/lib/workPlane.js";

const projection = {
  items: [
    { work_item_id: "w1", title: "Draft layout", bet_id: "b1", owner_id: "human:ana", status: "done" },
    { work_item_id: "w2", title: "Wire sections", bet_id: "b1", owner_id: "human:ana", status: "in_motion" },
    { work_item_id: "w3", title: "Old spike", bet_id: "b1", owner_id: "human:ana", status: "dropped" },
    { work_item_id: "w4", title: "Ship notes", bet_id: "b2", owner_id: "page_runner_agent", status: "up_next" },
    { work_item_id: "", title: "ghost row" },
    { work_item_id: "w5", title: "Weird status", bet_id: "b2", status: "almost" }
  ]
};

test("normalize keeps only real items and coerces unknown status to intake", () => {
  const items = normalizeItems(projection);
  assert.equal(items.length, 5);
  assert.equal(items.find((item) => item.itemId === "w5").status, "intake");
  assert.equal(items.find((item) => item.itemId === "w1").status, "done");
});

test("the rollup counts done over an honest denominator - dropped leaves the count", () => {
  const items = normalizeItems(projection);
  assert.equal(rolloutLineFor(items, "b1"), "1 of 2 done");
  assert.equal(rolloutLineFor(items, "b_nothing"), null);
});

test("a bet's drawer sees exactly its own items", () => {
  const items = normalizeItems(projection);
  assert.deepEqual(
    itemsForBet(items, "b1").map((item) => item.itemId),
    ["w1", "w2", "w3"]
  );
  assert.deepEqual(itemsForBet(items, ""), []);
});

test("my work rests done and dropped items", () => {
  const items = normalizeItems(projection);
  assert.deepEqual(
    myWork(items, "human:ana").map((item) => item.itemId),
    ["w2"]
  );
  assert.deepEqual(myWork(items, ""), []);
});

test("the next move walks the ladder and rests at done", () => {
  assert.equal(nextStatusFor("intake"), "up_next");
  assert.equal(nextStatusFor("in_review"), "done");
  assert.equal(nextStatusFor("done"), null);
  assert.equal(nextStatusFor("dropped"), null);
});

test("agent ownership is record truth from the actor id shape", () => {
  const items = normalizeItems(projection);
  assert.equal(ownedByAgent(items.find((item) => item.itemId === "w4")), true);
  assert.equal(ownedByAgent(items.find((item) => item.itemId === "w2")), false);
  assert.equal(statusLabel("in_motion"), "In motion");
});
