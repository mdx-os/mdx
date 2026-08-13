import routes from "../../../../generated/routes/mdx-local-routes.json";
import readonlyRoutes from "../../../../generated/routes/pages-readonly-route-contract.json";
import projectionFixtures from "../../../../generated/world-model/pages-projection-fixtures.json";
import lifecycleContract from "../../../../generated/pages/pages-lifecycle-contract.json";
import citationPolicy from "../../../../generated/pages/pages-agent-citation-policy.json";
import templateRegistry from "../../../../generated/pages/pages-template-registry.json";
import parityMatrix from "../../../../generated/pages/pages-v1-parity-matrix.json";
import worldModelProjection from "../../../../generated/world-model/pages-world-model-projection.json";
import conciergeAnswers from "../../../../generated/read-surfaces/pages-concierge-answer-contract.json";

const runtimeRoutes = [
  "/pages.json",
  "/pages/runtime-readiness.json",
  "/pages/lifecycle/projection.json",
  "/pages/agent-citations.json",
  "/pages/search-preflights/projection.json",
  "/pages/embedding-provider-refusals/projection.json",
  "/pages/world-model/projection.json",
  "/pages/stewardship-changes/projection.json"
];

function asArray(value) {
  return Array.isArray(value) ? value : [];
}

function routeByPath(path) {
  const route =
    asArray(routes.routes).find((candidate) => candidate.local_path === path) ??
    asArray(readonlyRoutes.routes).find((candidate) => candidate.local_path === path);
  return {
    method: route?.method ?? "GET",
    path,
    operation: route?.operation_id ?? "declared",
    surface: route?.surface ?? "pages",
    receiptBacked: route?.receipt_backed === true,
    writesAllowed: route?.writes_allowed === true,
    schema: route?.response_schema_path ?? "inline contract"
  };
}

function fixtureDocuments() {
  return [projectionFixtures.allowed_projection, ...asArray(projectionFixtures.developer_onboarding_corpus)].map(
    (doc) => ({
      id: doc.document_id,
      title: doc.title,
      summary: doc.summary ?? doc.body_ref,
      category: doc.category ?? "evidence",
      bodyRef: doc.body_ref,
      visibility: doc.visibility,
      receipts: asArray(doc.source_receipt_ids),
      charterEvidence: asArray(doc.charter_evidence_ids),
      owner: "declared by fixture",
      stale: false,
      citable: true
    })
  );
}

function runtimeDocuments(lifecyclePacket, pagesPacket) {
  const lifecycleDocs = asArray(lifecyclePacket?.documents);
  if (lifecycleDocs.length > 0) {
    return lifecycleDocs.map((doc) => ({
      id: doc.document_id,
      title: doc.title ?? doc.document_id,
      summary: doc.summary ?? doc.body_ref ?? doc.state,
      category: doc.category ?? doc.state ?? "pages",
      bodyRef: doc.body_ref ?? doc.route ?? "",
      visibility: doc.visibility ?? "tenant_only",
      receipts: asArray(doc.source_receipt_ids ?? doc.receipt_ids),
      charterEvidence: asArray(doc.charter_evidence_ids),
      owner: doc.trust?.owner ?? doc.owner_actor_id ?? "",
      stale: doc.freshness?.stale === true,
      citable: doc.trust?.trusted_for_ai === true || doc.citable === true
    }));
  }

  const pages = asArray(pagesPacket?.pages ?? pagesPacket?.documents);
  if (pages.length > 0) {
    return pages.map((doc) => ({
      id: doc.document_id,
      title: doc.title ?? doc.document_id,
      summary: doc.summary ?? doc.body_ref ?? "",
      category: doc.category ?? "pages",
      bodyRef: doc.body_ref ?? "",
      visibility: doc.visibility ?? "tenant_only",
      receipts: asArray(doc.source_receipt_ids ?? doc.receipt_ids),
      charterEvidence: asArray(doc.charter_evidence_ids),
      owner: doc.owner_actor_id ?? "",
      stale: false,
      citable: doc.trusted_for_ai === true || doc.citable === true
    }));
  }

  return fixtureDocuments();
}

function citationSummary(packet, documents) {
  const citableCount = Number(packet?.citable_count ?? documents.filter((doc) => doc.citable).length);
  const excluded = asArray(packet?.excluded).map((entry) => ({
    id: entry.document_id,
    whyNot: asArray(entry.why_not)
  }));
  return {
    status: packet ? "citation packet loaded" : "generated citation policy",
    rule: citationPolicy.trusted_for_ai_rule,
    route: citationPolicy.citable_list_route,
    citable: citableCount,
    heldBack: Number(packet?.excluded_count ?? excluded.length),
    excluded
  };
}

function readinessSummary(packet) {
  return {
    status: packet?.status ?? "waiting for kernel",
    summary: packet?.summary ?? citationPolicy.human_line,
    route: "/pages/runtime-readiness.json"
  };
}

function gapList(searchPreflightPacket, embeddingRefusalPacket) {
  const deferred = parityMatrix.entries
    .filter((entry) => entry.status === "deferred")
    .map((entry) => ({
      id: entry.id,
      title: entry.id.replaceAll("_", " "),
      source: "v1 parity",
      line: entry.note,
      route: entry.where
    }));
  const notBuilt = Object.entries(citationPolicy.declared_not_built).map(([id, line]) => ({
    id,
    title: id.replaceAll("_", " "),
    source: "citation policy",
    line,
    route: "declared not built"
  }));
  const blocked = asArray(projectionFixtures.blocked_projections).map((entry) => ({
    id: entry.id,
    title: entry.id.replaceAll("_", " "),
    source: "projection fixture",
    line: entry.blocked_by,
    route: "blocked projection"
  }));
  const runtime = [
    ...asArray(searchPreflightPacket?.items ?? searchPreflightPacket?.preflights),
    ...asArray(embeddingRefusalPacket?.items ?? embeddingRefusalPacket?.refusals)
  ].map((entry) => ({
    id: entry.receipt_id ?? entry.id ?? entry.kind ?? "runtime-record",
    title: entry.kind ?? entry.status ?? "runtime record",
    source: "runtime projection",
    line: entry.reason ?? entry.blocked_by ?? entry.summary ?? "recorded by the local kernel",
    route: entry.projection_route ?? ""
  }));

  return [...runtime, ...deferred, ...notBuilt, ...blocked];
}

function learningSummary(worldModelPacket) {
  const answerShapes = asArray(conciergeAnswers.answer_shapes).filter((shape) => shape.allowed === true);
  return {
    status: worldModelPacket ? "world model packet loaded" : "generated world model contract",
    line: worldModelProjection.human_line,
    projectionRoute: worldModelProjection.projection_route,
    relationshipTypes: Object.keys(worldModelProjection.relationships ?? {}),
    blocked: Object.values(worldModelProjection.declared_not_built ?? {}),
    answerShapes: answerShapes.slice(0, 6).map((shape) => ({
      id: shape.id,
      question: shape.question,
      documentId: shape.document_id,
      citations: asArray(shape.required_citations)
    })),
    rules: conciergeAnswers.rules
  };
}

function shapeList() {
  return asArray(templateRegistry.templates).map((template) => ({
    id: template.id,
    title: template.title,
    line: template.line,
    cadence: template.review_interval_days,
    requiredSections: asArray(template.required_sections)
  }));
}

export function adminKnowledge(packets = {}) {
  const documents = runtimeDocuments(
    packets["/pages/lifecycle/projection.json"],
    packets["/pages.json"]
  );
  const citations = citationSummary(packets["/pages/agent-citations.json"], documents);
  const stale = documents.filter((doc) => doc.stale);
  const unowned = documents.filter((doc) => !doc.owner);
  const routesForPages = runtimeRoutes.map(routeByPath);
  const receiptBackedRoutes = routesForPages.filter((route) => route.receiptBacked);
  const gaps = gapList(
    packets["/pages/search-preflights/projection.json"],
    packets["/pages/embedding-provider-refusals/projection.json"]
  );

  return {
    summary: {
      documents: documents.length,
      citable: citations.citable,
      heldBack: citations.heldBack,
      gaps: gaps.length,
      shapes: shapeList().length,
      receiptBackedRoutes: receiptBackedRoutes.length,
      routes: routesForPages.length
    },
    readiness: readinessSummary(packets["/pages/runtime-readiness.json"]),
    documents,
    stewardship: {
      stale,
      unowned,
      changeCount: asArray(packets["/pages/stewardship-changes/projection.json"]?.changes).length,
      lifecycle: lifecycleContract.human_line,
      receiptKinds: lifecycleContract.receipt_kinds
    },
    citations,
    gaps,
    learning: learningSummary(packets["/pages/world-model/projection.json"]),
    shapes: shapeList(),
    parity: {
      line: parityMatrix.human_line,
      shipped: parityMatrix.entries.filter((entry) => entry.status === "shipped").length,
      shippedDifferently: parityMatrix.entries.filter((entry) => entry.status === "shipped_differently").length,
      deferred: parityMatrix.entries.filter((entry) => entry.status === "deferred"),
      surpluses: parityMatrix.v2_surpluses
    },
    routes: routesForPages,
    sources: [
      { label: "Pages fixtures", value: "generated/world-model/pages-projection-fixtures.json" },
      { label: "Citation policy", value: "generated/pages/pages-agent-citation-policy.json" },
      { label: "Template registry", value: "generated/pages/pages-template-registry.json" },
      { label: "V1 parity matrix", value: "generated/pages/pages-v1-parity-matrix.json" },
      { label: "Readonly routes", value: "generated/routes/pages-readonly-route-contract.json" },
      { label: "World model projection", value: "generated/world-model/pages-world-model-projection.json" },
      { label: "Concierge answers", value: "generated/read-surfaces/pages-concierge-answer-contract.json" }
    ]
  };
}
