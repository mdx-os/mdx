// External sources, read-side. A connector brings content from OUTSIDE MDx
// into the graph; every item is recorded as a receipt marked origin=external,
// traceable to its source, and never trusted as internal truth. These helpers
// fold the connector projection into a per-source view a person can scan.

const KIND_LABEL = {
  x: "X",
  github: "GitHub",
  web: "Web",
  confluence: "Confluence",
  notion: "Notion",
  rss: "RSS"
};

export function sourceKindLabel(kind) {
  const key = String(kind ?? "").toLowerCase();
  return KIND_LABEL[key] ?? (key ? key : "source");
}

const GRADE_LABEL = {
  signal: "Signal",
  directional: "Directional",
  operational: "Operational"
};

export function gradeLabel(grade) {
  return GRADE_LABEL[String(grade ?? "").toLowerCase()] ?? "";
}

// The corpus rollup plus one row per source, each with its scope (whose source
// it is), kind, item count, and how many items are sensitive.
export function connectorView(packet) {
  const sourceRows = Array.isArray(packet?.sources) ? packet.sources : [];
  const sources = sourceRows.map((row) => ({
    sourceId: String(row?.source_id ?? ""),
    kindLabel: sourceKindLabel(row?.source_kind),
    scope: String(row?.scope ?? "tenant"),
    itemCount: Number(row?.item_count ?? 0),
    sensitiveCount: Number(row?.sensitive_count ?? 0)
  }));
  return {
    itemCount: Number(packet?.item_count ?? 0),
    tenantItems: Number(packet?.tenant_item_count ?? 0),
    // Personal items are withheld from this shared feed by design - we surface
    // only the count so their existence is acknowledged, never their content.
    personalWithheld: Number(packet?.personal_item_count_withheld ?? 0),
    sources
  };
}

// The most recent ingested items, newest first (the projection already orders
// them), each carrying its provenance for the disclosure panel.
export function connectorItems(packet, limit = 8) {
  const rows = Array.isArray(packet?.items) ? packet.items : [];
  return rows.slice(0, limit).map((row) => ({
    id: String(row?.item_receipt_id ?? ""),
    sourceId: String(row?.source_id ?? ""),
    kindLabel: sourceKindLabel(row?.source_kind),
    title: String(row?.title ?? ""),
    summary: String(row?.summary ?? ""),
    externalRef: String(row?.external_ref ?? ""),
    sensitivity: String(row?.data_sensitivity ?? "normal"),
    handling: String(row?.handling ?? "frontier"),
    grade: gradeLabel(row?.grade),
    scope: String(row?.scope ?? "tenant"),
    ingestedBy: String(row?.ingested_by ?? "")
  }));
}
