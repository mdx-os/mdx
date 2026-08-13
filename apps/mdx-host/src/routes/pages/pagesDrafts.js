export const PAGES_DRAFT_INDEX_KEY = "mdx-pages-draft-index";

export function normalizeDraftIndex(value) {
  if (!Array.isArray(value)) return [];
  const byId = new Map();
  for (const row of value) {
    const documentId = String(row?.documentId ?? "").trim();
    if (!documentId) continue;
    const next = {
      documentId,
      title: String(row?.title ?? "Untitled page").trim() || "Untitled page",
      pageType: String(row?.pageType ?? "knowledge"),
      updatedAt: Number(row?.updatedAt ?? 0),
      draftId: String(row?.draftId ?? ""),
      draftReceiptId: String(row?.draftReceiptId ?? "")
    };
    const current = byId.get(documentId);
    if (!current || next.updatedAt >= current.updatedAt) byId.set(documentId, next);
  }
  return [...byId.values()].sort((a, b) => b.updatedAt - a.updatedAt);
}

export function upsertDraftIndex(index, draft) {
  return normalizeDraftIndex([...(Array.isArray(index) ? index : []), draft]);
}

export function removeDraftFromIndex(index, documentId) {
  return normalizeDraftIndex((Array.isArray(index) ? index : []).filter((row) => row.documentId !== documentId));
}

export function approvalCoversRecordedDraft({
  lifecycleState,
  approval,
  draftId,
  draftReceiptId,
  currentTitle,
  currentText,
  recordedTitle,
  recordedText
}) {
  return (
    lifecycleState === "approved" &&
    Boolean(approval?.decisionReceiptId) &&
    approval?.draftId === draftId &&
    approval?.sourceDraftReceiptId === draftReceiptId &&
    currentTitle === recordedTitle &&
    currentText === recordedText
  );
}
