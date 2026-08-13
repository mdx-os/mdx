import assert from "node:assert/strict";
import test from "node:test";
import {
  approvalCoversRecordedDraft,
  normalizeDraftIndex,
  removeDraftFromIndex,
  upsertDraftIndex
} from "../src/routes/pages/pagesDrafts.js";

test("draft workspace keeps the newest record for each document", () => {
  const rows = normalizeDraftIndex([
    { documentId: "page_alpha", title: "Old", updatedAt: 1 },
    { documentId: "page_beta", title: "Beta", updatedAt: 2 },
    { documentId: "page_alpha", title: "Alpha", updatedAt: 3, pageType: "decision" }
  ]);
  assert.deepEqual(rows.map((row) => row.documentId), ["page_alpha", "page_beta"]);
  assert.equal(rows[0].title, "Alpha");
  assert.equal(rows[0].pageType, "decision");
});

test("draft workspace updates and retires lifecycle entries", () => {
  const inserted = upsertDraftIndex([], { documentId: "page_one", title: "One", updatedAt: 5 });
  const updated = upsertDraftIndex(inserted, { documentId: "page_one", title: "One revised", updatedAt: 6, draftId: "draft_1", draftReceiptId: "receipt_draft_1" });
  assert.equal(updated.length, 1);
  assert.equal(updated[0].draftId, "draft_1");
  assert.equal(updated[0].draftReceiptId, "receipt_draft_1");
  assert.deepEqual(removeDraftFromIndex(updated, "page_one"), []);
});

test("publication approval covers only the exact recorded words", () => {
  const exact = {
    lifecycleState: "approved",
    approval: {
      decisionReceiptId: "decision_1",
      draftId: "draft_1",
      sourceDraftReceiptId: "receipt_draft_1"
    },
    draftId: "draft_1",
    draftReceiptId: "receipt_draft_1",
    currentTitle: "Approved title",
    currentText: "Approved words",
    recordedTitle: "Approved title",
    recordedText: "Approved words"
  };
  assert.equal(approvalCoversRecordedDraft(exact), true);
  assert.equal(approvalCoversRecordedDraft({ ...exact, currentText: "Edited words" }), false);
  assert.equal(approvalCoversRecordedDraft({ ...exact, currentTitle: "Edited title" }), false);
  assert.equal(approvalCoversRecordedDraft({ ...exact, draftReceiptId: "receipt_draft_2" }), false);
  assert.equal(
    approvalCoversRecordedDraft({ ...exact, approval: { ...exact.approval, decisionReceiptId: "" } }),
    false
  );
});
