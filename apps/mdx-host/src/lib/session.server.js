import authBoundary from "../../../../generated/auth/mdx-auth-session-boundary.json" with { type: "json" };
import { normalizeRole, isOperatorSession } from "./session.js";

const trustedHeaderEnabled = () => process.env.MDX_HOST_TRUSTED_IDENTITY_HEADERS === "1";
const localDemoModes = new Set(["", "local", "local-demo", "demo"]);
const localKernelAuthorizationModes = new Set([...localDemoModes, "local-secure"]);
const oauthProviders = new Set(["google", "apple", "azure"]);
const deploymentMode = () =>
  String(process.env.MDX_DEPLOYMENT_MODE ?? process.env.MDX_HOST_AUTH_MODE ?? "local-demo")
    .trim()
    .toLowerCase();

export function verifiedHostSessionRequired() {
  return !localKernelAuthorizationModes.has(deploymentMode());
}

export function safeReturnPath(value) {
  return typeof value === "string" && /^\/(?!\/)[^\r\n\\]*$/.test(value) ? value : "/";
}

export function resolveOAuthProvider(
  value,
  fallback = process.env.MDX_SUPABASE_AUTH_PROVIDER ?? "google"
) {
  const requested = String(value ?? fallback).trim().toLowerCase();
  return oauthProviders.has(requested) ? requested : "google";
}

function localDemoAllowed() {
  return localDemoModes.has(deploymentMode());
}

function supabaseConfig() {
  const url = String(process.env.MDX_SUPABASE_URL ?? "").trim();
  const publishableKey = String(
    process.env.MDX_SUPABASE_PUBLISHABLE_KEY ?? process.env.MDX_SUPABASE_ANON_KEY ?? ""
  ).trim();
  return url && publishableKey ? { url, publishableKey } : null;
}

function bearerToken(request) {
  const authorization = String(request?.headers?.get("authorization") ?? "").trim();
  const match = /^Bearer\s+([^\s]+)$/i.exec(authorization);
  return match?.[1] ?? "";
}

async function createSupabaseBearerClient(accessToken) {
  const config = supabaseConfig();
  if (!config || !accessToken) return null;
  const { createClient } = await import("@supabase/supabase-js");
  return createClient(config.url, config.publishableKey, {
    auth: { persistSession: false, autoRefreshToken: false, detectSessionInUrl: false },
    global: { headers: { Authorization: `Bearer ${accessToken}` } }
  });
}

export async function createSupabaseServerClient(event) {
  const config = supabaseConfig();
  if (!config || !event?.cookies) return null;
  const { createServerClient } = await import("@supabase/ssr");
  return createServerClient(config.url, config.publishableKey, {
    cookies: {
      getAll: () => event.cookies.getAll(),
      setAll: (cookiesToSet, cacheHeaders = {}) => {
        for (const { name, value, options } of cookiesToSet) {
          event.cookies.set(name, value, { ...options, path: options?.path ?? "/" });
        }
        if (Object.keys(cacheHeaders).length > 0) {
          event.setHeaders(cacheHeaders);
        }
      }
    }
  });
}

function sessionFromTrustedHeaders(event) {
  if (!trustedHeaderEnabled()) return null;
  const verified = event.request.headers.get("x-mdx-session-verified");
  if (!["1", "true", "yes"].includes(String(verified ?? "").toLowerCase())) {
    return null;
  }
  const tenantId = event.request.headers.get("x-mdx-tenant-id");
  const userId = event.request.headers.get("x-mdx-user-id");
  const role = normalizeRole(event.request.headers.get("x-mdx-user-role"));
  if (!tenantId || !userId) return null;
  return {
    status: "verified-host-session",
    authenticated: true,
    verified: true,
    production_auth_ready: true,
    identity_source: "trusted_identity_headers",
    tenant_id: tenantId,
    user_id: userId,
    actor_id: userId,
    actor_kind: "human",
    subject_actor_id: userId,
    role,
    issued_at: event.request.headers.get("x-mdx-session-issued-at") ?? "",
    expires_at: event.request.headers.get("x-mdx-session-expires-at") ?? ""
  };
}

function localFixtureSession() {
  const session = authBoundary.local_session ?? {};
  return {
    ...session,
    status: "local-proof-session",
    authenticated: true,
    verified: false,
    production_auth_ready: false,
    identity_source: "local_demo_fixture",
    actor_id: session.user_id ?? "local_user",
    actor_kind: "human",
    subject_actor_id: session.user_id ?? "local_user",
    role: normalizeRole(session.role)
  };
}

function missingSession(status = "missing_verified_session") {
  return {
    status,
    authenticated: false,
    verified: false,
    production_auth_ready: false,
    identity_source: "none",
    tenant_id: "",
    user_id: "",
    actor_id: "",
    actor_kind: "",
    subject_actor_id: "",
    role: "viewer",
    issued_at: "",
    expires_at: ""
  };
}

function claimAt(claims, configuredName, fallbackName) {
  const path = String(configuredName ?? fallbackName)
    .split(".")
    .map((part) => part.trim())
    .filter(Boolean);
  let value = claims;
  for (const part of path) value = value?.[part];
  return typeof value === "string" ? value.trim() : "";
}

export function hasMdxAdmissionClaims(claims) {
  const tenantId = claimAt(claims, process.env.MDX_AUTH_TENANT_CLAIM, "mdx_tenant_id");
  const mappedRole = claimAt(claims, process.env.MDX_AUTH_ROLE_CLAIM, "mdx_role");
  const actorKind = claimAt(claims, process.env.MDX_AUTH_KIND_CLAIM, "mdx_actor_kind");
  return Boolean(tenantId && mappedRole && actorKind === "human");
}

function sessionFromClaims(claims) {
  const tenantId = claimAt(claims, process.env.MDX_AUTH_TENANT_CLAIM, "mdx_tenant_id");
  const actorId = claimAt(claims, process.env.MDX_AUTH_ACTOR_CLAIM, "sub");
  const mappedRole = claimAt(claims, process.env.MDX_AUTH_ROLE_CLAIM, "mdx_role");
  const role = normalizeRole(mappedRole);
  const actorKind = claimAt(claims, process.env.MDX_AUTH_KIND_CLAIM, "mdx_actor_kind");
  const subjectActorId = claimAt(claims, process.env.MDX_AUTH_SUBJECT_CLAIM, "sub");
  if (!hasMdxAdmissionClaims(claims) || !actorId || !subjectActorId) return null;
  return {
    status: "verified-supabase-session",
    authenticated: true,
    verified: true,
    production_auth_ready: true,
    identity_source: "supabase_signed_jwt",
    tenant_id: tenantId,
    user_id: actorId,
    actor_id: actorId,
    actor_kind: actorKind,
    subject_actor_id: subjectActorId,
    role,
    email: claimAt(claims, process.env.MDX_AUTH_EMAIL_CLAIM, "email"),
    issued_at: String(claims?.iat ?? ""),
    expires_at: String(claims?.exp ?? "")
  };
}

export async function resolveHostSession(event, injectedSupabase = null) {
  if (localDemoAllowed()) {
    return { session: localFixtureSession(), accessToken: null, supabase: null };
  }
  const trusted = sessionFromTrustedHeaders(event);
  if (trusted) return { session: trusted, accessToken: null, supabase: null };

  const requestBearer = bearerToken(event?.request);
  const supabase =
    injectedSupabase ??
    (requestBearer
      ? await createSupabaseBearerClient(requestBearer)
      : await createSupabaseServerClient(event));
  if (!supabase) {
    return { session: missingSession("supabase_auth_not_configured"), accessToken: null, supabase: null };
  }
  const { data: claimsData, error: claimsError } = await supabase.auth.getClaims(
    requestBearer || undefined
  );
  const session = claimsError ? null : sessionFromClaims(claimsData?.claims);
  if (!session) {
    return { session: missingSession("supabase_session_not_verified"), accessToken: null, supabase };
  }
  if (requestBearer) {
    return { session, accessToken: requestBearer, supabase };
  }
  const { data: sessionData, error: sessionError } = await supabase.auth.getSession();
  const accessToken = sessionError ? "" : String(sessionData?.session?.access_token ?? "").trim();
  if (!accessToken) {
    return { session: missingSession("supabase_access_token_missing"), accessToken: null, supabase };
  }
  return { session, accessToken, supabase };
}

export function applyKernelIdentityHeaders(headers, request, locals) {
  const authorization = request?.headers.get("authorization");
  const admission = request?.headers.get("x-mdx-actor-admission");
  const mode = deploymentMode();
  if (locals?.kernelAccessToken) {
    headers.Authorization = `Bearer ${locals.kernelAccessToken}`;
  } else if (localKernelAuthorizationModes.has(mode) && authorization) {
    headers.Authorization = authorization;
  }
  if (localDemoModes.has(mode) && admission) {
    headers["X-MDX-Actor-Admission"] = admission;
  }
  return headers;
}

export function adminAccess(session) {
  return {
    allowed: isOperatorSession(session),
    status: isOperatorSession(session) ? "ADMIN_ACCESS_ALLOWED" : "ADMIN_ACCESS_REFUSED",
    role: normalizeRole(session?.role),
    identity_source: session?.identity_source ?? "none",
    verified: session?.verified === true,
    production_auth_ready: session?.production_auth_ready === true
  };
}
