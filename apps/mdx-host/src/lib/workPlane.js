// The work plane, read-side. Items come from the kernel's projection
// (status already derived from move receipts - never stored, never
// derived twice here). These helpers group items for the two views the
// founder ratified: items inside their bet's drawer, and a flat
// personal "my work" cut. The rollup line is the bet's build truth:
// counted from records, prefilled nowhere.

export const WORK_STATUSES = [
  { key: "intake", label: "Intake" },
  { key: "up_next", label: "Up next" },
  { key: "in_motion", label: "In motion" },
  { key: "in_review", label: "In review" },
  { key: "done", label: "Done" },
  { key: "dropped", label: "Dropped" }
];

const STATUS_KEYS = WORK_STATUSES.map((status) => status.key);

export function statusLabel(key) {
  return WORK_STATUSES.find((status) => status.key === key)?.label ?? "Intake";
}

// The next natural move along the plane. Done and dropped rest.
export function nextStatusFor(key) {
  const order = ["intake", "up_next", "in_motion", "in_review", "done"];
  const at = order.indexOf(key);
  if (at === -1 || at === order.length - 1) return null;
  return order[at + 1];
}

export function normalizeItems(projection) {
  const rows = Array.isArray(projection?.items) ? projection.items : [];
  return rows
    .map((row) => ({
      itemId: String(row?.work_item_id ?? ""),
      title: String(row?.title ?? ""),
      description: String(row?.description ?? ""),
      betId: String(row?.bet_id ?? ""),
      ownerId: String(row?.owner_id ?? ""),
      approval: String(row?.approval ?? "full_signoff"),
      pageRef: String(row?.page_ref ?? ""),
      buildRef: String(row?.build_ref ?? ""),
      status: STATUS_KEYS.includes(String(row?.status ?? "")) ? String(row.status) : "intake",
      receiptId: String(row?.item_receipt_id ?? ""),
      moves: Array.isArray(row?.moves) ? row.moves : []
    }))
    .filter((item) => item.itemId !== "");
}

export function itemsForBet(items, betId) {
  if (!betId) return [];
  return items.filter((item) => item.betId === betId);
}

// One line of build truth for a bet card. Dropped items leave the
// denominator - dropping work is honest, not failure to finish.
export function rolloutLineFor(items, betId) {
  const within = itemsForBet(items, betId).filter((item) => item.status !== "dropped");
  if (within.length === 0) return null;
  const done = within.filter((item) => item.status === "done").length;
  return `${done} of ${within.length} done`;
}

// The personal cut: what this person owns that still moves. Done and
// dropped items rest out of the queue.
export function myWork(items, ownerId) {
  if (!ownerId) return [];
  return items.filter(
    (item) =>
      item.ownerId === ownerId && item.status !== "done" && item.status !== "dropped"
  );
}

// An agent owner is record truth, not a guess: the talent chain's
// runner ids end in _agent.
export function ownedByAgent(item) {
  return item.ownerId.endsWith("_agent");
}
