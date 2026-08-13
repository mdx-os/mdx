import assert from "node:assert/strict";
import test from "node:test";
import { normalizeRequest } from "../src/lib/pagesApproval.js";

test("approval rows retain the exact request, draft, and decision chain", () => {
  assert.deepEqual(
    normalizeRequest({
      document_id: "page_one",
      approval_request_receipt_id: "request_1",
      source_edit_draft_receipt_id: "draft_receipt_1",
      draft_id: "draft_1",
      approval_decision_receipt_id: "decision_1",
      decision_state: "APPROVED_PUBLICATION_BLOCKED",
      decision_outcome: "approved",
      reviewer_independence: "self_review_permitted",
      self_review_permitted: true
    }),
    {
      documentId: "page_one",
      requestReceiptId: "request_1",
      sourceDraftReceiptId: "draft_receipt_1",
      draftId: "draft_1",
      decisionReceiptId: "decision_1",
      decisionState: "APPROVED_PUBLICATION_BLOCKED",
      outcome: "approved",
      terminalState: "",
      pending: false,
      reviewerIndependence: "self_review_permitted",
      selfReviewPermitted: true,
      note: { kind: "approved", line: "Approved, on the record" }
    }
  );
});
