// The incoming stream's read helpers get executable tests: entries
// normalize from the projection, snoozed comes back to the open queue,
// the called history speaks in past tense, and every verdict has its
// single key.
import test from "node:test";
import assert from "node:assert/strict";
import {
  normalizeStream,
  openEntries,
  calledEntries,
  calledLine,
  verdictByHotkey,
  sourceLine
} from "../src/lib/triageStream.js";

const projection = {
  entries: [
    { entry_ref: "t1", text: "Link sections to sources", source: "manual", verdict: null },
    {
      entry_ref: "t2",
      text: "Pin answers to pages",
      source: "feedback",
      verdict: { verdict: "accepted", accepted_as: "bet", accepted_record_id: "product_bet_for_triage_t2" }
    },
    { entry_ref: "t3", text: "Old idea", source: "message", verdict: { verdict: "snoozed" } },
    { entry_ref: "t4", text: "Same as t1", source: "manual", verdict: { verdict: "duplicate", duplicate_of: "t1" } },
    { entry_ref: "", text: "ghost" }
  ]
};

test("normalize keeps real entries and folds the verdict shape", () => {
  const entries = normalizeStream(projection);
  assert.equal(entries.length, 4);
  assert.equal(entries[1].verdict.acceptedAs, "bet");
});

test("snoozed comes back to the open queue - not now, never never", () => {
  const entries = normalizeStream(projection);
  assert.deepEqual(
    openEntries(entries).map((entry) => entry.entryRef),
    ["t1", "t3"]
  );
  assert.deepEqual(
    calledEntries(entries).map((entry) => entry.entryRef),
    ["t2", "t4"]
  );
});

test("the called history speaks in past tense", () => {
  const entries = normalizeStream(projection);
  assert.equal(calledLine(entries[1]), "Accepted - now a bet on the Radar");
  assert.equal(calledLine(entries[3]), "Duplicate of t1");
});

test("every verdict has its single key", () => {
  assert.equal(verdictByHotkey("1").key, "accepted");
  assert.equal(verdictByHotkey("H").key, "snoozed");
  assert.equal(verdictByHotkey("x"), null);
});

test("sources speak in human words", () => {
  assert.equal(sourceLine("feedback"), "From the feedback button");
  assert.equal(sourceLine("unknown"), "Pasted in by hand");
});
