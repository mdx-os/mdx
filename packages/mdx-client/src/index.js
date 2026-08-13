export class MdxClientError extends Error {
  constructor(message, details = {}) {
    super(message);
    this.name = "MdxClientError";
    this.details = details;
  }
}

export function createRequestId(prefix = "ui") {
  const random =
    typeof crypto !== "undefined" && crypto.randomUUID
      ? crypto.randomUUID()
      : `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  return `${prefix}-${random}`;
}

export function routeCatalogFromGenerated(generatedRoutes) {
  const routes = Array.isArray(generatedRoutes?.routes) ? generatedRoutes.routes : [];
  const byPath = new Map(routes.map((route) => [`${route.method} ${route.local_path}`, route]));
  const governedWrites = new Set(
    routes
      .filter((route) => route.method === "POST" && route.receipt_backed)
      .map((route) => route.local_path)
  );
  return {
    routes,
    byPath,
    governedWrites,
    get(method, path) {
      return byPath.get(`${method.toUpperCase()} ${path}`) ?? null;
    },
    isGovernedWrite(path) {
      return governedWrites.has(path);
    }
  };
}

export function buildActorAdmission({ session, sourceRoute, receiptIntent = "governed_local_write", policyDecisionId = "policy-local-actor-admission", requestId = createRequestId("actor") }) {
  if (!session?.tenant_id || !session?.user_id || !session?.role) {
    throw new MdxClientError("Cannot build actor admission without tenant, user, and role.", {
      reason: "missing_session_fields",
      sourceRoute
    });
  }
  return {
    tenant_id: session.tenant_id,
    actor_id: session.user_id,
    actor_role: session.role,
    policy_decision_id: policyDecisionId,
    receipt_intent: receiptIntent,
    source_route: sourceRoute,
    request_id: requestId
  };
}

export function createMdxClient({ baseUrl = "", fetchImpl = globalThis.fetch, routeCatalog = null, session = null } = {}) {
  if (typeof fetchImpl !== "function") {
    throw new MdxClientError("A fetch implementation is required.", { reason: "missing_fetch" });
  }

  async function request(method, path, { body, actorAdmission, signal } = {}) {
    const route = routeCatalog?.get?.(method, path);
    if (routeCatalog && !route) {
      throw new MdxClientError("Route is not declared in the generated catalog.", {
        method,
        path,
        reason: "undeclared_route"
      });
    }
    if (method === "POST" && routeCatalog?.isGovernedWrite?.(path) && !actorAdmission) {
      throw new MdxClientError("Governed writes require actor admission.", {
        method,
        path,
        reason: "missing_actor_admission"
      });
    }

    // Bounded by default: an unresponsive kernel must surface as an error,
    // not a forever-pending button. Callers can pass their own signal (for
    // example a longer stream deadline) to override.
    const boundedSignal = signal ?? (typeof AbortSignal?.timeout === "function" ? AbortSignal.timeout(15000) : undefined);
    const response = await fetchImpl(`${baseUrl}${path}`, {
      method,
      signal: boundedSignal,
      headers: {
        Accept: "application/json",
        ...(body ? { "Content-Type": "application/json" } : {}),
        ...(actorAdmission ? { "X-MDX-Actor-Admission": JSON.stringify(actorAdmission) } : {})
      },
      body: body ? JSON.stringify(body) : undefined
    });
    if (!response.ok) {
      throw new MdxClientError(`${method} ${path} answered ${response.status}`, {
        method,
        path,
        status: response.status
      });
    }
    const contentType = response.headers.get("content-type") ?? "";
    return contentType.includes("application/json") ? response.json() : response.text();
  }

  return {
    read(path, options = {}) {
      return request("GET", path, options);
    },
    write(path, body = {}, options = {}) {
      const actorAdmission =
        options.actorAdmission ??
        buildActorAdmission({
          session,
          sourceRoute: path,
          receiptIntent: options.receiptIntent,
          policyDecisionId: options.policyDecisionId,
          requestId: options.requestId
        });
      return request("POST", path, { ...options, body, actorAdmission });
    }
  };
}

export function createRealtimeCursor({ initialAfterCursor = 0 } = {}) {
  let cursor = Number(initialAfterCursor) || 0;
  return {
    value() {
      return cursor;
    },
    advance(next) {
      const value = Number(next) || 0;
      if (value > cursor) {
        cursor = value;
      }
      return cursor;
    }
  };
}
