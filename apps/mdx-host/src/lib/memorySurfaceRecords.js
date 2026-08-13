const SURFACE_BY_SCOPE = {
  team_memory: "Message",
  company_memory: "Pages",
  project_memory: "Forge",
  agent_operational_memory: "Operations",
  private_user: "Twin",
  private_user_memory: "Twin"
};

function surfaceForScope(scope) {
  return SURFACE_BY_SCOPE[scope] ?? "Shared";
}

export function memorySurfaceRecords({ pending = [], ratifications = [], records = [] } = {}) {
  const groups = new Map();
  const represented = new Set();
  const groupFor = (scope) => {
    const surface = surfaceForScope(scope);
    if (!groups.has(surface)) groups.set(surface, { surface, waiting: [], kept: [] });
    return groups.get(surface);
  };

  for (const item of pending) {
    const memoryId = String(item.memory_id ?? "");
    represented.add(memoryId);
    groupFor(item.memory_scope).waiting.push({
      memoryId,
      content: String(item.content ?? ""),
      proposedBy: String(item.proposed_by ?? ""),
      sourceReceiptId: String(item.source_receipt_id ?? ""),
      gateReceiptId: String(item.gate_receipt_id ?? "")
    });
  }
  for (const item of ratifications) {
    if (item.ratification_decision !== "approve") continue;
    const memoryId = String(item.memory_id ?? "");
    represented.add(memoryId);
    groupFor(item.memory_scope).kept.push({
      memoryId,
      content: String(item.content ?? ""),
      sourceReceiptId: String(item.source_receipt_id ?? ""),
      gateReceiptId: String(item.gate_receipt_id ?? ""),
      ratifiedBy: String(item.ratified_by ?? ""),
      note: String(item.note ?? ""),
      receiptId: String(item.ratification_receipt_id ?? ""),
      recordBasis: "ratified"
    });
  }
  for (const item of records) {
    const memoryId = String(item.memory_id ?? "");
    if (!memoryId || represented.has(memoryId)) continue;
    groupFor(item.memory_scope).kept.push({
      memoryId,
      content: String(item.content ?? ""),
      sourceReceiptId: String(item.source_receipt_id ?? ""),
      gateReceiptId: String(item.gate_receipt_id ?? ""),
      ratifiedBy: "",
      note: "",
      receiptId: String(item.gate_receipt_id ?? ""),
      recordBasis: "consolidated"
    });
  }
  return [...groups.values()];
}
