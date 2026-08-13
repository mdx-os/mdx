import routes from "../../../../generated/routes/mdx-local-routes.json";
import changeGates from "../../../../generated/change-gates/mdx-change-gates.json";
import authBoundary from "../../../../generated/auth/mdx-auth-session-boundary.json";
import liveExecutionGate from "../../../../generated/workers/live-execution-gate.json";

const gateRoutePaths = [
  "/local/auth-session.json",
  "/auth/tenant-policy-preflights.json",
  "/auth/session-controls.json",
  "/forge/ship-decisions.json",
  "/v1/write-mirror-approval-requests.json",
  "/v1/write-mirror-approval-decisions.json",
  "/audit/evidence-checkpoints.json",
  "/audit/evidence-anchors.json"
];

const gateIds = [
  "auth_session",
  "security_posture",
  "threat_model",
  "forge_talent",
  "forge_delegation",
  "forge_receipt_chain",
  "worker",
  "worker_handoff",
  "talent",
  "talent_authorization",
  "human_edge",
  "ui_app",
  "local_postgres",
  "route_or_schema"
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
    receiptBacked: route?.receipt_backed === true
  };
}

function routeDomain(path) {
  return path.split("/").filter(Boolean)[0] ?? "root";
}

function writeDomains(writeRoutes) {
  const domains = new Map();
  for (const route of writeRoutes) {
    const domain = routeDomain(route.local_path);
    const current = domains.get(domain) ?? {
      domain,
      total: 0,
      receiptBacked: 0,
      exceptions: 0,
      routes: []
    };
    current.total += 1;
    if (route.receipt_backed === true) {
      current.receiptBacked += 1;
    } else {
      current.exceptions += 1;
    }
    if (current.routes.length < 4) {
      current.routes.push(route.local_path);
    }
    domains.set(domain, current);
  }
  return [...domains.values()].sort((left, right) => right.total - left.total);
}

function changeGateById(id) {
  const gate = asArray(changeGates.gates).find((candidate) => candidate.id === id);
  return {
    id,
    changeType: gate?.change_type ?? "declared change",
    requiredChecks: asArray(gate?.required_checks)
  };
}

function authorityCard({ id, name, status, receiptKind, required, forbidden, rules, route }) {
  return {
    id,
    name,
    status,
    receiptKind,
    required: asArray(required),
    forbidden: asArray(forbidden),
    rules: asArray(rules),
    route
  };
}

export function adminGate() {
  const allRoutes = asArray(routes.routes);
  const writes = allRoutes.filter((route) => route.method === "POST");
  const receiptBackedWrites = writes.filter((route) => route.receipt_backed === true);
  const exceptions = writes.filter((route) => route.receipt_backed !== true);

  const authorityCards = [
    authorityCard({
      id: "actor-admission",
      name: "Actor Admission",
      status: authBoundary.actor_admission.status,
      receiptKind: "auth.session.local",
      required: authBoundary.actor_admission.required_fields,
      forbidden: authBoundary.forbidden_until_live_gate,
      rules: authBoundary.required_controls,
      route: "/local/auth-session.json"
    }),
    authorityCard({
      id: "live-worker-execution",
      name: "Live Worker Execution",
      status: liveExecutionGate.runtime_status,
      receiptKind: liveExecutionGate.requested_receipt_kind,
      required: liveExecutionGate.required_receipts,
      forbidden: liveExecutionGate.forbidden_paths,
      rules: liveExecutionGate.rules,
      route: liveExecutionGate.source
    })
  ];

  const blockedAuthorityCount = authorityCards.reduce(
    (sum, card) => sum + card.forbidden.length,
    0
  );

  return {
    summary: {
      routes: allRoutes.length,
      writes: writes.length,
      receiptBackedWrites: receiptBackedWrites.length,
      exceptions: exceptions.length,
      changeGates: asArray(changeGates.gates).length,
      blockedAuthorities: blockedAuthorityCount
    },
    stance: {
      name: changeGates.name,
      mode: changeGates.mode,
      source: changeGates.source,
      rules: changeGates.rules
    },
    writeDomains: writeDomains(writes),
    exceptions: exceptions.map((route) => ({
      path: route.local_path,
      operation: route.operation_id,
      domain: routeDomain(route.local_path),
      note: "not counted as receipt-backed admin authority"
    })),
    authorityCards,
    changeGates: gateIds.map(changeGateById),
    routes: gateRoutePaths.map(routeByPath),
    sources: [
      { label: "Routes", value: "generated/routes/mdx-local-routes.json" },
      { label: "Change gates", value: "generated/change-gates/mdx-change-gates.json" },
      { label: "Auth boundary", value: "generated/auth/mdx-auth-session-boundary.json" },
      { label: "Live worker gate", value: "generated/workers/live-execution-gate.json" }
    ]
  };
}
