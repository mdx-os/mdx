import routes from "../../../../generated/routes/mdx-local-routes.json";
import securityPosture from "../../../../generated/security/mdx-security-posture.json";
import threatModel from "../../../../generated/security/mdx-threat-model.json";
import authBoundary from "../../../../generated/auth/mdx-auth-session-boundary.json";
import messageNoisePolicy from "../../../../generated/message/message-agent-noise-policy.json";
import pagesCitationPolicy from "../../../../generated/pages/pages-agent-citation-policy.json";

const routeLabels = {
  "/auth/tenant-policy-preflights.json": "Tenant policy write",
  "/auth/tenant-policy-preflights/projection.json": "Tenant policy view",
  "/auth/session-controls.json": "Session control write",
  "/auth/session-controls/projection.json": "Session control view",
  "/twin/provider-call-refusals.json": "Provider call refusal write",
  "/twin/provider-call-refusals/projection.json": "Provider call refusal view",
  "/twin/production-memory-refusals.json": "Production memory refusal write",
  "/twin/production-memory-refusals/projection.json": "Production memory refusal view",
  "/twin/tool-execution-refusals.json": "Tool execution refusal write",
  "/twin/tool-execution-refusals/projection.json": "Tool execution refusal view",
  "/messages/service-role-fanout-refusals.json": "Service-role fanout refusal write",
  "/messages/service-role-fanout-refusals/projection.json": "Service-role fanout refusal view",
  "/pages/embedding-provider-refusals.json": "Embedding provider refusal write",
  "/pages/embedding-provider-refusals/projection.json": "Embedding provider refusal view"
};

const countFields = [
  "refusal_count",
  "preflight_count",
  "control_count",
  "tenant_switch_refusal_count",
  "role_escalation_refusal_count"
];

function routeByPath(path) {
  const route = (routes.routes ?? []).find((candidate) => candidate.local_path === path);
  return {
    label: routeLabels[path] ?? route?.operation_id ?? path,
    method: route?.method ?? (path.includes("/projection") ? "GET" : "POST"),
    path,
    operation: route?.operation_id ?? "declared",
    receiptBacked: route?.receipt_backed === true
  };
}

function routePair(writeRoute, projectionRoute) {
  return [routeByPath(writeRoute), routeByPath(projectionRoute)];
}

function controlById(id) {
  return (securityPosture.controls ?? []).find((control) => control.id === id);
}

function threatById(id) {
  return (threatModel.threats ?? []).find((threat) => threat.id === id);
}

function projectionCount(packet) {
  if (!packet) return null;
  return countFields.reduce((sum, field) => sum + Number(packet[field] ?? 0), 0);
}

function guardrail({
  id,
  group,
  name,
  line,
  routeWrite,
  routeProjection,
  receiptKinds,
  evidence,
  threatId,
  controlId,
  blocked
}) {
  const control = controlId ? controlById(controlId) : null;
  const threat = threatId ? threatById(threatId) : null;
  return {
    id,
    group,
    name,
    line,
    routes: routePair(routeWrite, routeProjection),
    projectionRoute: routeProjection,
    receiptKinds,
    evidence: evidence ?? control?.evidence ?? threat?.evidence_artifact,
    check: control?.check ?? threat?.mitigation_check,
    blocked: blocked ?? [],
    runtimeCount: null
  };
}

export function adminGuardrails(projections = {}) {
  const guardrails = [
    guardrail({
      id: "tenant-policy",
      group: "Input",
      name: "Tenant Policy",
      line: "Every access-changing request proves tenant, actor, policy, source route, and receipt intent first.",
      routeWrite: "/auth/tenant-policy-preflights.json",
      routeProjection: "/auth/tenant-policy-preflights/projection.json",
      receiptKinds: ["auth.tenant_policy.preflighted"],
      controlId: "policy_receipt_path",
      threatId: "policy_bypass",
      blocked: ["service_role_bypass", "user_metadata_authorization", "implicit_tenant_inference"]
    }),
    guardrail({
      id: "session-control",
      group: "Input",
      name: "Session Control",
      line: "Tenant switches and role escalation are recorded as refusals unless policy explicitly permits them.",
      routeWrite: "/auth/session-controls.json",
      routeProjection: "/auth/session-controls/projection.json",
      receiptKinds: [
        "auth.session.activation.recorded",
        "auth.session.revocation.recorded",
        "auth.tenant_switch.refused",
        "auth.role_escalation.refused"
      ],
      evidence: "generated/auth/mdx-auth-session-boundary.json",
      threatId: "tenant_boundary_break",
      blocked: ["tenant_switch_without_policy", "role_escalation_without_policy"]
    }),
    guardrail({
      id: "provider-call",
      group: "Output",
      name: "Provider Call Refusal",
      line: "Live model calls are refused until provider turn-on evidence and session authority exist.",
      routeWrite: "/twin/provider-call-refusals.json",
      routeProjection: "/twin/provider-call-refusals/projection.json",
      receiptKinds: ["twin.session.provider_call.refused"],
      controlId: "live_substrate_evidence",
      threatId: "live_substrate_fake_green",
      blocked: ["provider_call_allowed", "live_provider_call_allowed"]
    }),
    guardrail({
      id: "production-memory",
      group: "Output",
      name: "Production Memory Refusal",
      line: "Memory stays local and receipt-grounded until production memory authority is explicitly turned on.",
      routeWrite: "/twin/production-memory-refusals.json",
      routeProjection: "/twin/production-memory-refusals/projection.json",
      receiptKinds: ["twin.session.production_memory.refused"],
      controlId: "companion_read_only",
      threatId: "companion_action_drift",
      blocked: ["production_memory_write_allowed", "production_write_allowed"]
    }),
    guardrail({
      id: "tool-execution",
      group: "Output",
      name: "Tool Execution Refusal",
      line: "Tools remain blocked for companions until a governed rail opens execution authority.",
      routeWrite: "/twin/tool-execution-refusals.json",
      routeProjection: "/twin/tool-execution-refusals/projection.json",
      receiptKinds: ["twin.session.tool_execution.refused"],
      controlId: "worker_bounds",
      threatId: "worker_scope_creep",
      blocked: ["tool_execution_allowed", "worker_spawn_allowed"]
    }),
    guardrail({
      id: "service-role-fanout",
      group: "Privacy",
      name: "Service-role Fanout Refusal",
      line: "Realtime fanout refuses service-role shortcuts and keeps delivery evidence on the governed rail.",
      routeWrite: "/messages/service-role-fanout-refusals.json",
      routeProjection: "/messages/service-role-fanout-refusals/projection.json",
      receiptKinds: ["message.service_role.fanout.refused"],
      evidence: "docs/MESSAGE-REALTIME-CUTOVER-PREFLIGHT-CONTRACT.md",
      threatId: "tenant_boundary_break",
      blocked: ["service_role_runtime_allowed", "production_delivery_allowed"]
    }),
    guardrail({
      id: "embedding-provider",
      group: "Privacy",
      name: "Embedding Provider Refusal",
      line: "Page content cannot move into embedding providers before the Pages rail proves source and citation policy.",
      routeWrite: "/pages/embedding-provider-refusals.json",
      routeProjection: "/pages/embedding-provider-refusals/projection.json",
      receiptKinds: ["pages.embedding_provider.refused"],
      evidence: "generated/pages/pages-agent-citation-policy.json",
      threatId: "secret_exposure",
      blocked: ["embedding_provider_call_allowed", "search_indexing_allowed"]
    })
  ].map((item) => ({
    ...item,
    runtimeCount: projectionCount(projections[item.projectionRoute])
  }));

  const writeRoutes = guardrails.flatMap((item) => item.routes).filter((route) => route.method === "POST");
  const receiptBackedWrites = writeRoutes.filter((route) => route.receiptBacked);
  const liveCounts = guardrails.filter((item) => item.runtimeCount !== null);
  const refusalTotal = liveCounts.reduce((sum, item) => sum + item.runtimeCount, 0);
  const groups = ["Input", "Output", "Privacy"].map((name) => ({
    name,
    guardrails: guardrails.filter((item) => item.group === name)
  }));

  return {
    summary: {
      guardrails: guardrails.length,
      receiptBackedWrites: receiptBackedWrites.length,
      blockedCases: new Set(guardrails.flatMap((item) => item.blocked)).size,
      liveRefusals: liveCounts.length > 0 ? refusalTotal : null
    },
    groups,
    policies: {
      actorAdmission: {
        status: authBoundary.actor_admission.status,
        requiredFields: authBoundary.actor_admission.required_fields,
        requiredControls: authBoundary.required_controls,
        forbiddenUntilLiveGate: authBoundary.forbidden_until_live_gate
      },
      messageNoise: {
        line: messageNoisePolicy.human_line,
        lanes: messageNoisePolicy.lanes,
        threshold: messageNoisePolicy.folding.threshold_consecutive_non_human
      },
      pageCitation: {
        line: pagesCitationPolicy.human_line,
        citableListRoute: pagesCitationPolicy.citable_list_route,
        derivedAt: pagesCitationPolicy.derived_at
      }
    },
    sources: [
      { label: "Routes", value: "generated/routes/mdx-local-routes.json" },
      { label: "Security posture", value: "generated/security/mdx-security-posture.json" },
      { label: "Threat model", value: "generated/security/mdx-threat-model.json" },
      { label: "Auth boundary", value: "generated/auth/mdx-auth-session-boundary.json" },
      { label: "Message policy", value: "generated/message/message-agent-noise-policy.json" },
      { label: "Pages policy", value: "generated/pages/pages-agent-citation-policy.json" }
    ]
  };
}
