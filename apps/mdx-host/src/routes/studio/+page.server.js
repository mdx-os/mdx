// Studio: the place you hand a goal and a draft to a coworker and it lands a
// document as a Page you can stand behind. The composer's one primary verb
// starts a governed run; from there each run carries the controls the kernel
// allows from its current state - pause, steer, resume, land. The projection
// reports state and allowed_controls, both derived from receipts, so the
// surface only offers moves the kernel would accept. Everything loads through
// the kernel proxy, fail-soft: a missing projection opens an inviting empty
// surface, never an invented one.
async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      signal: AbortSignal.timeout(1500)
    });
    return response.ok ? await response.json() : null;
  } catch (error) {
    return null;
  }
}

export async function load({ fetch }) {
  // Sources a run can be grounded in: internal Pages objects and external
  // connector items. Both are already-governed receipts; Studio binds one by
  // reference and records which kind it was, never re-ingesting the content.
  // The connector feed already withholds personal items by scope, so this list
  // only carries what the operator may ground a tenant run in.
  const [projection, pages, connectors] = await Promise.all([
    readJson(fetch, "/studio/runs/projection.json"),
    readJson(fetch, "/pages/publications/projection.json"),
    readJson(fetch, "/connectors/projection.json")
  ]);
  const pageSources = (pages?.publications ?? []).map((p) => ({
    receipt_id: p.publication_receipt_id,
    kind: "pages_object",
    title: p.title || p.document_id || "Untitled page"
  }));
  const connectorSources = (connectors?.items ?? []).map((c) => ({
    receipt_id: c.item_receipt_id,
    kind: "connector_item",
    title: c.title || c.external_ref || "External item",
    sensitivity: c.data_sensitivity || ""
  }));
  return {
    runs: projection?.runs ?? [],
    sources: [...pageSources, ...connectorSources].filter((s) => s.receipt_id),
    boundary:
      "A run lands one document as a single Page. Studio keeps the run and its trail, never a second copy.",
    safeNext: "give Studio a goal and a draft, and it lands a document you can stand behind"
  };
}
