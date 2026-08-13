import sessionBoundary from "../../../../generated/auth/mdx-auth-session-boundary.json";
import tenantPolicy from "../../../../generated/auth/auth-tenant-policy-preflight-contract.json";
import routes from "../../../../generated/routes/mdx-local-routes.json";

const routeLabels = {
  "/local/auth-session.json": "Session",
  "/auth/readiness.json": "Readiness",
  "/auth/tenant-policy-preflights.json": "Tenant policy write",
  "/auth/tenant-policy-preflights/projection.json": "Tenant policy view",
  "/auth/invite-requests.json": "Invite write",
  "/auth/invite-requests/projection.json": "Invite view",
  "/auth/role-assignments.json": "Role write",
  "/auth/role-assignments/projection.json": "Role view",
  "/auth/session-controls.json": "Session control write",
  "/auth/session-controls/projection.json": "Session control view"
};

const blockedLabels = {
  password_login: "Password login",
  oauth_login: "OAuth login",
  session_write_cookie: "Session cookie writes",
  tenant_switch_without_policy: "Tenant switches without policy",
  role_escalation: "Role escalation",
  service_role_bypass: "Service-role bypass"
};

function authRoutes() {
  return (routes.routes ?? [])
    .filter((route) => route.surface === "auth_session")
    .map((route) => ({
      label: routeLabels[route.local_path] ?? route.operation_id,
      method: route.method,
      path: route.local_path,
      operation: route.operation_id,
      receiptBacked: route.receipt_backed === true
    }));
}

function routeByPath(path) {
  return authRoutes().find((route) => route.path === path) ?? {
    label: routeLabels[path] ?? path,
    method: path.includes("/projection") || path.includes("/readiness") ? "GET" : "POST",
    path,
    operation: "declared",
    receiptBacked: true
  };
}

function blockedAuthority() {
  return (sessionBoundary.forbidden_until_live_gate ?? []).map((id) => ({
    id,
    label: blockedLabels[id] ?? id.replaceAll("_", " "),
    state: "blocked until the live gate records authority"
  }));
}

function accessArea({ id, name, line, writeRoute, projectionRoute, receipts, proof }) {
  return {
    id,
    name,
    line,
    proof,
    receipts,
    routes: [routeByPath(writeRoute), routeByPath(projectionRoute)]
  };
}

export function adminAccess() {
  const allAuthRoutes = authRoutes();
  const writes = allAuthRoutes.filter((route) => route.method === "POST");
  const receiptBacked = allAuthRoutes.filter((route) => route.receiptBacked);
  const localSession = sessionBoundary.local_session;
  const actorAdmission = sessionBoundary.actor_admission;

  return {
    session: {
      status: sessionBoundary.status,
      tenant: localSession.tenant_id,
      user: localSession.user_id,
      role: localSession.role,
      expiresAt: localSession.expires_at,
      route: sessionBoundary.local_route,
      roles: sessionBoundary.roles ?? []
    },
    summary: {
      roles: sessionBoundary.roles?.length ?? 0,
      authRoutes: allAuthRoutes.length,
      governedWrites: writes.filter((route) => route.receiptBacked).length,
      blockedAuthority: sessionBoundary.forbidden_until_live_gate?.length ?? 0
    },
    actorAdmission: {
      status: actorAdmission.status,
      scope: actorAdmission.scope,
      governedRouteCount: actorAdmission.governed_routes?.length ?? 0,
      requiredFields: actorAdmission.required_fields ?? [],
      requiredControls: sessionBoundary.required_controls ?? [],
      hiddenPrivilegeBypassAllowed: actorAdmission.hidden_privilege_bypass_allowed,
      productionWriteAllowed: actorAdmission.production_write_allowed
    },
    areas: [
      accessArea({
        id: "tenant-policy",
        name: "Tenant Policy",
        line: "Proves tenant membership, role mapping, invite state, and approved model scope before access changes.",
        writeRoute: tenantPolicy.write_route,
        projectionRoute: tenantPolicy.projection_route,
        receipts: [tenantPolicy.required_receipt_kind],
        proof: `${tenantPolicy.v1_sources_bound?.length ?? 0} v1 access sources are bound into policy.`
      }),
      accessArea({
        id: "invites",
        name: "Invites",
        line: "Records invite intent while email delivery and provider auth stay blocked.",
        writeRoute: tenantPolicy.access_management_routes.invite_write_route,
        projectionRoute: tenantPolicy.access_management_routes.invite_projection_route,
        receipts: [tenantPolicy.access_management_routes.invite_receipt_kind],
        proof: "Invites can be requested locally, but not delivered as production auth."
      }),
      accessArea({
        id: "roles",
        name: "Roles",
        line: "Records role assignment evidence after policy and invite context are present.",
        writeRoute: tenantPolicy.access_management_routes.role_assignment_write_route,
        projectionRoute: tenantPolicy.access_management_routes.role_assignment_projection_route,
        receipts: [tenantPolicy.access_management_routes.role_assignment_receipt_kind],
        proof: "Production role writes and hidden role escalation remain blocked."
      }),
      accessArea({
        id: "sessions",
        name: "Sessions",
        line: "Records activation, revocation, tenant-switch refusal, and role-escalation refusal evidence.",
        writeRoute: tenantPolicy.session_control_routes.write_route,
        projectionRoute: tenantPolicy.session_control_routes.projection_route,
        receipts: [
          tenantPolicy.session_control_routes.activation_receipt_kind,
          tenantPolicy.session_control_routes.revocation_receipt_kind,
          tenantPolicy.session_control_routes.tenant_switch_refusal_receipt_kind,
          tenantPolicy.session_control_routes.role_escalation_refusal_receipt_kind
        ],
        proof: "Session cookies and provider revocation stay blocked until live authority exists."
      })
    ],
    routes: allAuthRoutes,
    routeEvidence: {
      receiptBacked: receiptBacked.length,
      readOnlySession: allAuthRoutes.some((route) => route.path === sessionBoundary.local_route)
    },
    blockedAuthority: blockedAuthority(),
    sources: [
      { label: "Session boundary", value: "generated/auth/mdx-auth-session-boundary.json" },
      {
        label: "Tenant policy contract",
        value: "generated/auth/auth-tenant-policy-preflight-contract.json"
      },
      { label: "Routes", value: "generated/routes/mdx-local-routes.json" },
      { label: "Source map", value: "generated/source-map/mdx-source-map.json" }
    ]
  };
}
