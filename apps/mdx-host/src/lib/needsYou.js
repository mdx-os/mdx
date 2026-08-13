// Needs-you, read-side. The kernel projection carries the record-backed
// reasons (approvals, signatures, work in review, open triage); the
// ready-to-back bets join here from the pipeline fold the Product page
// already uses - one derivation per truth, merged at the surface. The
// groups read in attention order: decisions first, then judgment, then
// reading.

export const REASON_GROUPS = [
  { key: "ready_bet", title: "Ready for your call", line: "Claims judged. These bets wait on a yes or a no." },
  { key: "talent_signature", title: "Waiting on your signature", line: "Authority requests the chain holds until you sign." },
  { key: "work_review", title: "Work in review", line: "Finished pieces waiting for someone to accept them." },
  { key: "page_approval", title: "Pages to approve", line: "Drafts waiting for a publish decision." },
  { key: "triage_open", title: "Incoming signal", line: "New entries waiting for a call in the stream." }
];

const ROUTE_BY_REASON = {
  ready_bet: "/product-direction",
  talent_signature: "/talent",
  work_review: "/product-direction",
  page_approval: "/pages",
  triage_open: "/product-direction/triage"
};

export function normalizeNeedsYou(projection) {
  const rows = Array.isArray(projection?.items) ? projection.items : [];
  return rows
    .map((row) => ({
      reason: String(row?.reason ?? ""),
      ref: String(row?.ref ?? ""),
      detail: String(row?.detail ?? ""),
      route: String(row?.route ?? "") || ROUTE_BY_REASON[String(row?.reason ?? "")] || "/",
      receiptId: String(row?.receipt_id ?? "")
    }))
    .filter((item) => item.reason !== "");
}

// Ready bets come from the pipeline fold, shaped like the other rows.
export function readyBetRows(pipeline) {
  const ready = pipeline?.stages?.find((stage) => stage.key === "ready")?.cards ?? [];
  return ready.map((card) => ({
    reason: "ready_bet",
    ref: card.betId,
    detail: card.bet,
    route: "/product-direction",
    receiptId: card.receiptId ?? ""
  }));
}

// One list, grouped by reason in attention order. Empty groups vanish.
export function groupNeedsYou(items) {
  return REASON_GROUPS.map((group) => ({
    ...group,
    items: items.filter((item) => item.reason === group.key)
  })).filter((group) => group.items.length > 0);
}

export function totalWaiting(items) {
  return items.length;
}
