import routes from "../../../../generated/routes/mdx-local-routes.json";
import handoffEvidence from "../../../../generated/evidence/handoff-evidence-chain.json";
import receiptFixtures from "../../../../generated/db/mdx-receipt-insert-fixtures.json";

const evidenceRoutePaths = [
  "/receipts.json",
  "/receipts/{receipt_id}",
  "/local/evidence-index.json",
  "/audit/evidence-checkpoints.json",
  "/audit/evidence-checkpoints/projection.json",
  "/audit/evidence-anchors.json"
];

function asArray(value) {
  return Array.isArray(value) ? value : [];
}

function routeByPath(path) {
  const route = asArray(routes.routes).find((candidate) => candidate.local_path === path);
  return {
    method: route?.method ?? "GET",
    path,
    operation: route?.operation_id ?? "declared",
    surface: route?.surface ?? "evidence",
    receiptBacked: route?.receipt_backed === true,
    schema: route?.response_schema_path ?? "inline contract"
  };
}

function receiptSummary(receipts) {
  const receiptIds = asArray(receipts?.receipt_ids);
  return {
    status: receipts ? "runtime receipts loaded" : "waiting for kernel",
    count: Number(receipts?.count ?? receiptIds.length),
    latest: receiptIds.slice(-12).reverse()
  };
}

function checkpointSummary(packet) {
  const checkpoints = asArray(packet?.checkpoints).map((checkpoint) => ({
    id: checkpoint.checkpoint_id,
    receipt: checkpoint.checkpoint_receipt_id,
    rangeStart: checkpoint.range_start_receipt_id,
    rangeEnd: checkpoint.range_end_receipt_id,
    receiptCount: Number(checkpoint.receipt_count ?? 0),
    merkleRoot: checkpoint.merkle_root,
    checkpointHash: checkpoint.checkpoint_hash,
    signed: checkpoint.signed === true,
    anchored: checkpoint.externally_anchored === true
  }));
  return {
    status: packet ? "checkpoint projection loaded" : "waiting for kernel",
    receiptKind: packet?.receipt_kind ?? "audit.evidence.checkpoint.recorded",
    count: Number(packet?.checkpoint_count ?? checkpoints.length),
    authorityOpened: packet?.authority_opened ?? "none",
    productionWriteAllowed: packet?.production_write_allowed === true,
    checkpoints
  };
}

function localEvidenceSummary(packet) {
  const counts = packet?.counts ?? packet?.summary ?? {};
  return {
    status: packet?.status ?? "waiting for kernel",
    total: Number(packet?.evidence_count ?? packet?.total ?? counts.total ?? 0),
    complete: Number(packet?.complete_count ?? counts.complete ?? 0),
    stale: Number(packet?.stale_count ?? counts.stale ?? 0)
  };
}

export function adminEvidence(packets = {}) {
  const receipts = receiptSummary(packets["/receipts.json"]);
  const checkpoints = checkpointSummary(packets["/audit/evidence-checkpoints/projection.json"]);
  const localEvidence = localEvidenceSummary(packets["/local/evidence-index.json"]);
  const receiptTables = asArray(receiptFixtures.receipt_backed_tables).map((table) => ({
    table: table.table,
    columns: asArray(table.receipt_columns),
    fixture: table.insert_sql_fixture
  }));
  const handoffSteps = asArray(handoffEvidence.steps).map((step) => ({
    id: step.id,
    evidence: step.evidence,
    receipt: step.required_receipt
  }));
  const routesForEvidence = evidenceRoutePaths.map(routeByPath);
  const receiptBackedRouteCount = routesForEvidence.filter((route) => route.receiptBacked).length;

  return {
    summary: {
      runtimeReceipts: receipts.count,
      checkpoints: checkpoints.count,
      evidenceTotal: localEvidence.total,
      receiptTables: receiptTables.length,
      evidenceRoutes: routesForEvidence.length,
      receiptBackedRoutes: receiptBackedRouteCount
    },
    receipts,
    checkpoints,
    localEvidence,
    chains: [
      {
        id: "handoff-evidence",
        name: handoffEvidence.name,
        status: handoffEvidence.status,
        source: handoffEvidence.source,
        requiredReceipts: handoffSteps.map((step) => step.receipt),
        steps: handoffSteps,
        forbiddenActions: handoffEvidence.forbidden_actions,
        rules: handoffEvidence.rules
      }
    ],
    receiptTables,
    routes: routesForEvidence,
    sources: [
      { label: "Routes", value: "generated/routes/mdx-local-routes.json" },
      { label: "Handoff evidence chain", value: "generated/evidence/handoff-evidence-chain.json" },
      { label: "Receipt insert fixtures", value: "generated/db/mdx-receipt-insert-fixtures.json" }
    ]
  };
}
