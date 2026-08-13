// The home pulse helpers turn runtime counts into the landing's one warm
// sentence. These are the product's first-impression words, so they get
// executable tests: the pulse refuses to recite zeros, counts pluralize,
// and recency falls out of receipt ids without wall-clock claims.
import test from "node:test";
import assert from "node:assert/strict";
import { greetingFor, pulseLine, receiptOrder } from "../src/lib/homeToday.js";

test("the pulse is null when nothing has happened, so the page invites", () => {
  assert.equal(pulseLine({}), null);
  assert.equal(pulseLine({ twinCount: 0, messageCount: 0, pagesCount: 0 }), null);
});

test("one activity reads as one plain sentence", () => {
  const line = pulseLine({ twinCount: 1 });
  assert.ok(line.includes("Twin has answered 1 question"));
  assert.ok(!line.includes("questions"), "singular count stays singular");
});

test("several activities join naturally with an and", () => {
  const line = pulseLine({ twinCount: 3, messageCount: 2, pagesCount: 1 });
  assert.ok(line.includes("3 questions"));
  assert.ok(line.includes(" and "));
  assert.ok(!line.includes(", and "), "the join is natural, not a list dump");
});

test("greetings follow the hour", () => {
  assert.equal(greetingFor(6), "Good morning.");
  assert.equal(greetingFor(13), "Good afternoon.");
  assert.equal(greetingFor(22), "Good evening.");
  assert.equal(greetingFor(2), "Good evening.");
});

test("recency comes from the receipt id counter, not a clock", () => {
  assert.ok(receiptOrder("twin_receipt_000042") > receiptOrder("twin_receipt_000007"));
  assert.equal(receiptOrder("no-trailing-number"), 0);
  assert.equal(receiptOrder(null), 0);
});
