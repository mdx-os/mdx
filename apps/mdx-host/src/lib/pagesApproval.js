// Pages approval runtime helpers. Approve/reject are real kernel writes that
// ride the same-origin proxy with actor admission built from the local
// session. The requests projection is the single source of truth for the
// "waiting on you" queue: every row carries its own decision_state and, once
// decided, its decision_outcome and decision receipt id - so one read gives
// both the pending queue and the per-document decision note.
import { createMdxClient } from "@mdx/client";

export const requestsProjectionPath = "/pages/approval-requests/projection.json";
export const decisionsProjectionPath = "/pages/approval-decisions/projection.json";
export const createRequestPath = "/pages/approval-requests.json";
export const approvePath = "/pages/approval-decisions/approve.json";
export const rejectPath = "/pages/approval-decisions/reject.json";

// Build a client that signs governed writes with the local session. The route
// catalog stays null here on purpose: the kernel proxy is the enforcement
// point for these exact-path writes, and the session is what makes admission
// real rather than relying on the local stub accepting unsigned posts.
export function createApprovalClient(session) {
  return createMdxClient({ baseUrl: "/api/kernel", session });
}

// A request row is still waiting on a human when no decision has landed.
export function isPending(row) {
  return String(row?.decision_state ?? "") === "PENDING_DECISION";
}

// Map a kernel decision_outcome to a human line + a flag for styling. We never
// show the raw terminal token on chrome; the exact token goes in disclosure.
export function decisionNote(outcome) {
  if (outcome === "approved") {
    return { kind: "approved", line: "Approved, on the record" };
  }
  if (outcome === "rejected") {
    return { kind: "rejected", line: "Declined, on the record" };
  }
  return null;
}

export function normalizeRequest(row) {
  const outcome = String(row?.decision_outcome ?? "");
  return {
    documentId: String(row?.document_id ?? ""),
    requestReceiptId: String(row?.approval_request_receipt_id ?? ""),
    sourceDraftReceiptId: String(row?.source_edit_draft_receipt_id ?? ""),
    draftId: String(row?.draft_id ?? ""),
    decisionReceiptId: String(row?.approval_decision_receipt_id ?? ""),
    decisionState: String(row?.decision_state ?? ""),
    outcome,
    terminalState: String(row?.terminal_state ?? ""),
    pending: isPending(row),
    reviewerIndependence: String(row?.reviewer_independence ?? ""),
    selfReviewPermitted: row?.self_review_permitted === true,
    note: decisionNote(outcome)
  };
}
