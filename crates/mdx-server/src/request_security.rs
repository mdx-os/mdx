//! Request-level security boundary for the HTTP serving path.
//!
//! This module threads the deployment mode into routing and gates governed
//! writes. In `local-secure` and `production` a governed write derives its
//! identity from a verified trusted session produced by the trusted-session
//! verifier (`auth_verifier`); a request with no verified session is refused, and
//! a request body that conflicts with the session is refused.
//!
//! We deliberately do not trust client-supplied identity headers in
//! `local-secure` or `production`, because a header is spoofable. The bearer token
//! is verified (its signature is checked), so it is not a spoofable header. A mode
//! with no configured verifier fails closed. `local-demo` keeps its explicit
//! fixture behavior and never passes through this gate.

use crate::RouteResponse;
use crate::auth_verifier;
use mdx_core::{
    AdmittedIdentity, BETA_WAITLIST_ROUTE, ClientClaimedIdentity, DeploymentMode,
    GOVERNED_WRITE_ROUTES, GovernedWriteIdentity, TrustedSession, admit_governed_write_identity,
};
use std::cell::RefCell;

/// The security context for a single request: the deployment mode, the verified
/// trusted session (if any), and the refusal code if verification failed.
#[derive(Clone, Debug)]
pub(crate) struct RequestSecurity {
    mode: DeploymentMode,
    trusted: Option<TrustedSession>,
    refusal_code: Option<&'static str>,
}

impl RequestSecurity {
    /// Build the security context for a live connection. In a mode that requires a
    /// trusted session, the bearer token is verified through the configured
    /// verifier; verification failure (or no verifier) leaves no session and a
    /// refusal code, so the governed-write gate fails closed.
    pub(crate) fn for_connection(mode: DeploymentMode, request: &str) -> Self {
        Self::for_connection_at(mode, request, &auth_verifier::now_rfc3339())
    }

    fn for_connection_at(mode: DeploymentMode, request: &str, now: &str) -> Self {
        let verification = auth_verifier::verify_for_connection(mode, request, now);
        Self {
            mode,
            trusted: verification.session,
            refusal_code: verification.refusal_code,
        }
    }

    /// The frictionless local context: local-demo, no trusted session. Used by
    /// the read-path test helper so existing behavior is unchanged.
    #[cfg(test)]
    pub(crate) fn local_demo() -> Self {
        Self {
            mode: DeploymentMode::LocalDemo,
            trusted: None,
            refusal_code: None,
        }
    }

    /// A production context with no verified session, for tests that exercise the
    /// fail-closed governed-write boundary.
    #[cfg(test)]
    pub(crate) fn production_unverified() -> Self {
        Self {
            mode: DeploymentMode::Production,
            trusted: None,
            refusal_code: Some("verifier_unconfigured_auth_profile"),
        }
    }

    /// Build the context for a connection at a fixed time with an injected auth
    /// profile and signing key, so HTTP tests exercise the full verifier path with
    /// a minted token and no process environment.
    #[cfg(test)]
    pub(crate) fn for_connection_verified_test(
        mode: DeploymentMode,
        request: &str,
        now: &str,
        config: &auth_verifier::VerifierConfig,
    ) -> Self {
        let verification = auth_verifier::verify_for_connection_with(mode, request, now, config);
        Self {
            mode,
            trusted: verification.session,
            refusal_code: verification.refusal_code,
        }
    }

    /// Classify a request as a governed write: ungated (local-demo, non-POST, or
    /// not a governed route), refused (no verified session, or a body that
    /// conflicts with the session), or admitted with the verified identity. The
    /// body can echo the session but never override it - identity is taken wholly
    /// from the session. OPTIONS preflight is ungated.
    pub(crate) fn classify_governed_write(
        &self,
        method: &str,
        path: &str,
        body: &str,
    ) -> GovernedWrite {
        if !self.mode.requires_trusted_session() || !method.eq_ignore_ascii_case("POST") {
            return GovernedWrite::Ungated;
        }
        if is_public_intake_write_route(path) {
            return GovernedWrite::Ungated;
        }
        // Fail closed by default: in a mode that requires a trusted session,
        // a POST to a route that is not declared governed is refused outright.
        // Membership in GOVERNED_WRITE_ROUTES is the admission contract, not a
        // best-effort list - a mutating route that someone forgets to declare
        // must surface as a refusal in production, never as an open endpoint.
        if !is_governed_write_route(path) {
            return GovernedWrite::Refused(RouteResponse::json(
                "403 Forbidden",
                concat!(
                    "{\"error\":\"production_post_route_not_governed\",",
                    "\"detail\":\"this deployment mode refuses any POST route that is not a ",
                    "declared governed write; declare the route in GOVERNED_WRITE_ROUTES to ",
                    "admit it through a verified session\",",
                    "\"production_write_allowed\":false}"
                )
                .to_string(),
            ));
        }
        let Some(session) = self.trusted.as_ref() else {
            let code = self
                .refusal_code
                .unwrap_or("production_governed_write_requires_verified_session");
            return GovernedWrite::Refused(RouteResponse::json(
                "401 Unauthorized",
                format!(
                    concat!(
                        "{{\"error\":\"production_governed_write_requires_verified_session\",",
                        "\"refusal\":\"{}\",",
                        "\"detail\":\"this deployment mode authorizes governed writes only from a verified ",
                        "trusted session; the request body is never identity\",",
                        "\"production_write_allowed\":false}}"
                    ),
                    code
                ),
            ));
        };
        if let Some(refusal) = suspended_actor_refusal(session) {
            return GovernedWrite::Refused(refusal);
        }
        if let Some(refusal) = authorize_verified_session_for_route(session, path) {
            return GovernedWrite::Refused(refusal);
        }
        let claimed = claimed_identity_from_body(body);
        match admit_governed_write_identity(self.mode, Some(session), &claimed) {
            Ok(identity) => GovernedWrite::Admitted(identity),
            Err(error) => GovernedWrite::Refused(RouteResponse::json(
                "403 Forbidden",
                format!(
                    "{{\"error\":\"{}\",\"detail\":\"{}\",\"production_write_allowed\":false}}",
                    error.code(),
                    error.message()
                ),
            )),
        }
    }

    /// The deployment mode this request is being served under.
    pub(crate) fn mode(&self) -> DeploymentMode {
        self.mode
    }

    /// The verified identity for read handlers that need tenant context. This
    /// carries no write authority; it only prevents a trusted read from falling
    /// back to local-demo identity when a route builds a tenant-scoped view.
    pub(crate) fn verified_identity_for_read(&self) -> Option<AdmittedIdentity> {
        let session = self.trusted.as_ref()?;
        admit_governed_write_identity(self.mode, Some(session), &ClientClaimedIdentity::default())
            .ok()
    }

    /// Stable key for per-principal admission controls after the request has
    /// been verified. This is intentionally not derived from spoofable headers:
    /// local-secure and production only produce it from a signed trusted session.
    pub(crate) fn rate_limit_key(&self) -> Option<String> {
        let session = self.trusted.as_ref()?;
        Some(format!("{}:{}", session.tenant_id, session.actor_id))
    }

    /// Presence-safe identity correlation for operational traces. Request or
    /// response bodies, token claims, and authorization values never cross this
    /// boundary.
    pub(crate) fn telemetry_identity(&self) -> Option<(&str, &str)> {
        let session = self.trusted.as_ref()?;
        Some((session.tenant_id.as_str(), session.actor_id.as_str()))
    }

    /// Whether this request carries a verified trusted session.
    pub(crate) fn has_verified_session(&self) -> bool {
        self.trusted.is_some()
    }

    /// Refusal code from verifier setup or token verification, if any.
    pub(crate) fn refusal_code(&self) -> Option<&'static str> {
        self.refusal_code
    }

    /// The fail-closed read boundary. In a trusted-session mode every read
    /// requires a verified session, except the operational probes a load
    /// balancer or operator needs before any session exists. local-demo reads
    /// stay frictionless. This closes the gap where /receipts.json and every
    /// other projection answered anonymously in production: the serving kernel
    /// is one shared world until tenant-scoped reads land, so an anonymous
    /// read is a cross-tenant read.
    pub(crate) fn classify_read(&self, method: &str, path: &str) -> Option<RouteResponse> {
        if !self.mode.requires_trusted_session() {
            return None;
        }
        // POST is the governed-write gate's job; OPTIONS preflight carries no
        // data and must answer for CORS to work at all.
        if method.eq_ignore_ascii_case("POST") || method.eq_ignore_ascii_case("OPTIONS") {
            return None;
        }
        if OPEN_OPERATIONAL_ROUTES.contains(&path) {
            return None;
        }
        if self.trusted.is_some() {
            if let Some(session) = self.trusted.as_ref()
                && let Some(refusal) = suspended_actor_refusal(session)
            {
                return Some(refusal);
            }
            return None;
        }
        let code = self
            .refusal_code
            .unwrap_or("production_read_requires_verified_session");
        Some(RouteResponse::json(
            "401 Unauthorized",
            format!(
                concat!(
                    "{{\"error\":\"production_read_requires_verified_session\",",
                    "\"refusal\":\"{}\",",
                    "\"detail\":\"this deployment mode answers reads only to a verified trusted ",
                    "session; operational probes stay open\",",
                    "\"production_write_allowed\":false}}"
                ),
                code
            ),
        ))
    }

    /// The fail-closed response for a governed write that must be refused, or None
    /// to proceed. A thin wrapper over [`classify_governed_write`], used by tests.
    #[cfg(test)]
    pub(crate) fn governed_write_refusal(
        &self,
        method: &str,
        path: &str,
        body: &str,
    ) -> Option<RouteResponse> {
        match self.classify_governed_write(method, path, body) {
            GovernedWrite::Refused(response) => Some(response),
            GovernedWrite::Ungated | GovernedWrite::Admitted(_) => None,
        }
    }
}

/// The reads that stay open in every mode: the probes an operator, load
/// balancer, or boot script needs before any session exists. Everything else
/// requires a verified session in a trusted-session mode.
pub(crate) const OPEN_OPERATIONAL_ROUTES: &[&str] = &["/health", "/status", "/status.json"];

/// Narrow anonymous intake routes. These are not governed application writes:
/// they accept safe demand-capture payloads, record no production authority, and
/// stay separately rate-limited by IP before a request body is read.
pub(crate) const PUBLIC_INTAKE_WRITE_ROUTES: &[&str] =
    &[BETA_WAITLIST_ROUTE, "/mobile/cloud/github-webhooks.json"];

/// The classification of a request as a governed write.
pub(crate) enum GovernedWrite {
    /// Not a gated governed write: proceed with no verified identity.
    Ungated,
    /// Refused, with the fail-closed response.
    Refused(RouteResponse),
    /// Admitted, with the verified identity taken from the trusted session.
    Admitted(AdmittedIdentity),
}

fn authorize_verified_session_for_route(
    session: &TrustedSession,
    path: &str,
) -> Option<RouteResponse> {
    if is_non_human_actor(&session.actor_kind) {
        let allowed = session
            .authority_scope
            .iter()
            .any(|scope| scope_allows_route(scope, path));
        if !allowed {
            return Some(governed_route_forbidden(
                "production_actor_scope_forbidden",
                "the verified non-human token does not carry authority for this governed route",
            ));
        }
    }
    if is_operator_route(path) && !is_operator_role(&session.actor_role) {
        return Some(governed_route_forbidden(
            "production_operator_route_forbidden",
            "this governed route requires an owner, admin, operator, security, cloud, or infra role",
        ));
    }
    None
}

fn suspended_actor_refusal(session: &TrustedSession) -> Option<RouteResponse> {
    if !actor_is_suspended(&session.tenant_id, &session.actor_id) {
        return None;
    }
    Some(governed_route_forbidden(
        "production_actor_suspended",
        "this verified actor is suspended by operator runtime control",
    ))
}

fn actor_is_suspended(tenant_id: &str, actor_id: &str) -> bool {
    let configured = std::env::var("MDX_SUSPENDED_ACTORS").unwrap_or_default();
    configured.split(',').map(str::trim).any(|entry| {
        !entry.is_empty()
            && (entry == actor_id
                || entry == format!("{tenant_id}:{actor_id}")
                || entry == format!("{tenant_id}/{actor_id}"))
    })
}

fn is_non_human_actor(kind: &str) -> bool {
    !matches!(
        kind.trim().to_ascii_lowercase().as_str(),
        "human" | "user" | "person"
    )
}

fn governed_route_forbidden(error: &str, detail: &str) -> RouteResponse {
    RouteResponse::json(
        "403 Forbidden",
        format!(
            "{{\"error\":\"{}\",\"detail\":\"{}\",\"production_write_allowed\":false}}",
            error, detail
        ),
    )
}

fn is_operator_role(role: &str) -> bool {
    matches!(
        role.trim().to_ascii_lowercase().as_str(),
        "owner"
            | "admin"
            | "tenant_admin"
            | "operator"
            | "security"
            | "cloud"
            | "infra"
            | "infrastructure"
    )
}

fn is_operator_route(path: &str) -> bool {
    path.starts_with("/install/model-connect")
        || path.starts_with("/install/owner-claims")
        || matches!(
            path,
            "/models/connect.json"
                | "/models/catalog.json"
                | "/models/test.json"
                | "/models/adaptive/evaluations.json"
                | "/models/adaptive/promotions.json"
                | "/models/outcomes.json"
                | "/models/connections.json"
                | "/models/deployments.json"
                | "/models/policies.json"
        )
}

fn scope_allows_route(scope: &str, path: &str) -> bool {
    let scope = scope.trim();
    if scope.is_empty() {
        return false;
    }
    if matches!(scope, "*" | "*:*" | "governed:*" | "governed_write:*") {
        return true;
    }
    if scope == path {
        return true;
    }
    if let Some(prefix) = scope.strip_suffix("/*")
        && path.starts_with(prefix)
    {
        return true;
    }
    let route_scope = route_scope(path);
    scope == route_scope
        || scope_write_verb_matches_route(scope, route_scope)
        || scope.strip_suffix(":*") == Some(route_scope.split(':').next().unwrap_or_default())
        || route_scope.starts_with(&format!("{scope}:"))
}

fn scope_write_verb_matches_route(scope: &str, route_scope: &str) -> bool {
    let Some((base, verb)) = scope.rsplit_once(':') else {
        return false;
    };
    base == route_scope
        && matches!(
            verb,
            "write" | "create" | "update" | "admin" | "manage" | "connect" | "turn-on"
        )
}

fn route_scope(path: &str) -> &'static str {
    if path.starts_with("/models/") {
        "models"
    } else if path.starts_with("/install/model-connect") {
        "install:model-connect"
    } else if path.starts_with("/install/owner-claims") {
        "install:owner"
    } else if path.starts_with("/install/") {
        "install"
    } else if path.starts_with("/forge/build") {
        "forge:build"
    } else if path.starts_with("/forge/run") {
        "forge:run"
    } else if path.starts_with("/forge/ship") {
        "forge:ship"
    } else if path.starts_with("/forge/deployment") {
        "forge:deployment"
    } else if path.starts_with("/forge/") {
        "forge"
    } else if path.starts_with("/messages/") {
        "messages"
    } else if path.starts_with("/pages/") {
        "pages"
    } else if path.starts_with("/auth/") {
        "auth"
    } else if path.starts_with("/marketplace/") {
        "marketplace"
    } else if path.starts_with("/studio/") {
        "studio"
    } else if path.starts_with("/twin/") {
        "twin"
    } else if path.starts_with("/product/") {
        "product"
    } else if path.starts_with("/strategy/") {
        "strategy"
    } else if path.starts_with("/audit/") {
        "audit"
    } else {
        "governed"
    }
}

thread_local! {
    /// The verified identity for the request currently being served on this
    /// thread. The server is thread-per-connection, so this is request-scoped: the
    /// secured entry sets it for a verified governed write and clears it when the
    /// request finishes, and a governed-write handler reads it to record the
    /// verified identity instead of the local fixture.
    static VERIFIED_IDENTITY: RefCell<Option<AdmittedIdentity>> = const { RefCell::new(None) };
}

/// Set the verified identity for the current request, returning a guard that
/// clears it on drop so it never leaks into the next request on this thread.
pub(crate) fn set_verified_identity(identity: Option<AdmittedIdentity>) -> VerifiedIdentityGuard {
    VERIFIED_IDENTITY.with(|cell| *cell.borrow_mut() = identity);
    VerifiedIdentityGuard
}

/// The verified identity for the current request, if a governed write was
/// admitted from a trusted session. None in local-demo and on read paths.
pub(crate) fn current_verified_identity() -> Option<AdmittedIdentity> {
    VERIFIED_IDENTITY.with(|cell| cell.borrow().clone())
}

/// The tenant, actor, role, and receipt identity a governed-write handler should
/// record. In local-secure/production this comes from the verified trusted
/// session; in local-demo it comes from the request body fixture (defaults when
/// absent). A handler resolves this once, then feeds the verified values into
/// both its response and the identity-aware kernel save.
pub(crate) struct ResolvedWriteIdentity {
    pub tenant_id: String,
    pub actor_id: String,
    pub actor_role: String,
    pub auth_session_status: &'static str,
    pub identity: GovernedWriteIdentity,
}

/// Resolve the identity a governed-write handler should record. A verified
/// trusted session is authoritative; otherwise the body fixture is used with the
/// given local-demo defaults. The body never overrides a verified session (the
/// gate already refused any conflict).
pub(crate) fn resolve_governed_write_identity(
    body: &str,
    default_tenant: &str,
    default_actor: &str,
    default_role: &str,
) -> ResolvedWriteIdentity {
    match current_verified_identity() {
        Some(verified) => {
            let identity = GovernedWriteIdentity::from_admitted(&verified);
            ResolvedWriteIdentity {
                tenant_id: verified.tenant_id,
                actor_id: verified.actor_id,
                actor_role: verified.actor_role,
                auth_session_status: "VERIFIED_TRUSTED_SESSION",
                identity,
            }
        }
        None => {
            let tenant_id = non_empty_or(json_string_field(body, "tenant_id"), default_tenant);
            let actor_id = non_empty_or(json_string_field(body, "actor_id"), default_actor);
            let actor_role = non_empty_or(json_string_field(body, "actor_role"), default_role);
            let identity = GovernedWriteIdentity::local_demo(&actor_id);
            ResolvedWriteIdentity {
                tenant_id,
                actor_id,
                actor_role,
                auth_session_status: "ACCEPTED_LOCAL_STUB",
                identity,
            }
        }
    }
}

fn non_empty_or(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

#[must_use]
pub(crate) struct VerifiedIdentityGuard;

impl Drop for VerifiedIdentityGuard {
    fn drop(&mut self) {
        VERIFIED_IDENTITY.with(|cell| *cell.borrow_mut() = None);
    }
}

/// The tenant, actor, and role a request body claims. In a mode that requires a
/// trusted session these carry no authority; they are only checked for conflict
/// with the verified session and otherwise ignored.
fn claimed_identity_from_body(body: &str) -> ClientClaimedIdentity {
    ClientClaimedIdentity {
        tenant_id: json_string_field(body, "tenant_id"),
        actor_id: json_string_field(body, "actor_id"),
        actor_role: json_string_field(body, "actor_role"),
    }
}

/// Read a JSON string field `"key":"value"` from a request body, tolerating
/// whitespace after the colon. Returns an empty string when absent.
fn json_string_field(body: &str, key: &str) -> String {
    let needle = format!("\"{key}\"");
    let Some(after_key) = body.split_once(&needle).map(|(_, rest)| rest) else {
        return String::new();
    };
    let after_colon = match after_key.split_once(':') {
        Some((_, rest)) => rest.trim_start(),
        None => return String::new(),
    };
    let Some(rest) = after_colon.strip_prefix('"') else {
        return String::new();
    };
    match rest.split_once('"') {
        Some((value, _)) => value.to_string(),
        None => String::new(),
    }
}

/// True when the path is a governed write route, including parameterized routes.
pub(crate) fn is_governed_write_route(path: &str) -> bool {
    GOVERNED_WRITE_ROUTES
        .iter()
        .any(|route| route_matches(route, path))
}

pub(crate) fn is_public_intake_write_route(path: &str) -> bool {
    PUBLIC_INTAKE_WRITE_ROUTES
        .iter()
        .any(|route| route_matches(route, path))
}

/// Match a route template against a concrete path. A `{segment}` in the template
/// matches any single non-empty path segment; everything else matches literally.
fn route_matches(template: &str, path: &str) -> bool {
    if !template.contains('{') {
        return template == path;
    }
    let template_segments: Vec<&str> = template.split('/').collect();
    let path_segments: Vec<&str> = path.split('/').collect();
    if template_segments.len() != path_segments.len() {
        return false;
    }
    template_segments
        .iter()
        .zip(path_segments.iter())
        .all(|(t, p)| (t.starts_with('{') && t.ends_with('}') && !p.is_empty()) || t == p)
}

/// The CORS headers for a response under a deployment mode. local-demo keeps the
/// permissive local wildcard. Secure and production never emit a wildcard origin:
/// they echo a configured allowed origin (from `MDX_CORS_ALLOW_ORIGINS`) when the
/// request origin is on the allowlist, and otherwise send no allow-origin header
/// at all.
pub(crate) fn cors_headers(mode: DeploymentMode, request: &str) -> String {
    let configured = std::env::var("MDX_CORS_ALLOW_ORIGINS").unwrap_or_default();
    cors_headers_with_config(mode, request, &configured)
}

const CORS_METHODS_AND_HEADERS: &str = "Access-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Accept, Content-Type";

/// The pure core of [`cors_headers`], with the configured origins passed in so it
/// can be tested without touching process environment. local-demo is wildcard;
/// secure and production never emit a wildcard origin.
fn cors_headers_with_config(mode: DeploymentMode, request: &str, configured: &str) -> String {
    if !mode.requires_trusted_session() {
        return format!("Access-Control-Allow-Origin: *\r\n{CORS_METHODS_AND_HEADERS}");
    }
    let allowed: Vec<&str> = configured
        .split(',')
        .map(|origin| origin.trim())
        .filter(|origin| !origin.is_empty() && *origin != "*")
        .collect();
    let chosen = request_origin(request)
        .filter(|origin| allowed.contains(&origin.as_str()))
        .or_else(|| allowed.first().map(|origin| origin.to_string()));
    match chosen {
        Some(origin) => {
            format!(
                "Access-Control-Allow-Origin: {origin}\r\n{CORS_METHODS_AND_HEADERS}\r\nVary: Origin"
            )
        }
        // No configured origin matched: emit no allow-origin header rather than a
        // wildcard. A browser cross-origin request is then correctly blocked.
        None => CORS_METHODS_AND_HEADERS.to_string(),
    }
}

/// Extract the `Origin` request header, if present.
fn request_origin(request: &str) -> Option<String> {
    request
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("Origin"))
        })
        .map(|(_, value)| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    static ENV_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn local_demo_never_gates_governed_writes() {
        let security = RequestSecurity::local_demo();
        assert!(
            security
                .governed_write_refusal("POST", "/forge/build-requests.json", "{}")
                .is_none()
        );
    }

    #[test]
    fn production_refuses_unverified_governed_write() {
        let security = RequestSecurity::production_unverified();
        let refusal = security
            .governed_write_refusal("POST", "/forge/build-requests.json", "{}")
            .expect("production must refuse an unverified governed write");
        assert_eq!(refusal.status_for_test(), "401 Unauthorized");
        assert!(
            refusal
                .body_text()
                .contains("production_governed_write_requires_verified_session")
        );
    }

    #[test]
    fn production_allows_reads_and_options_preflight() {
        let security = RequestSecurity::production_unverified();
        assert!(
            security
                .governed_write_refusal("GET", "/status.json", "{}")
                .is_none()
        );
        // OPTIONS preflight is not a write and must still flow to the 204 path.
        assert!(
            security
                .governed_write_refusal("OPTIONS", "/forge/build-requests.json", "{}")
                .is_none()
        );
    }

    #[test]
    fn parameterized_governed_route_is_matched() {
        assert!(is_governed_write_route(
            "/dxr/forge-runs/run_123/controls.json"
        ));
        assert!(!is_governed_write_route("/dxr/forge-runs/controls.json"));
        assert!(is_governed_write_route("/run-loop/evals_runner_agent"));
        assert!(is_governed_write_route(
            "/run-loop/forge_orchestrator_agent"
        ));
    }

    #[test]
    fn production_refuses_a_post_to_an_undeclared_route() {
        // The fail-closed default: a POST outside GOVERNED_WRITE_ROUTES is
        // refused in a trusted-session mode, so a mutating route someone
        // forgets to declare can never run unauthenticated in production.
        let security = RequestSecurity::production_unverified();
        let refusal = security
            .governed_write_refusal("POST", "/some/undeclared-route.json", "{}")
            .expect("production must refuse an undeclared POST route");
        assert_eq!(refusal.status_for_test(), "403 Forbidden");
        assert!(
            refusal
                .body_text()
                .contains("production_post_route_not_governed")
        );
    }

    #[test]
    fn production_allows_only_explicit_public_intake_without_session() {
        let security = RequestSecurity::production_unverified();
        assert!(
            security
                .governed_write_refusal("POST", BETA_WAITLIST_ROUTE, "{}")
                .is_none(),
            "waitlist capture is the explicit anonymous intake route"
        );
        assert!(is_public_intake_write_route(BETA_WAITLIST_ROUTE));
        assert!(
            security
                .governed_write_refusal("POST", "/mobile/cloud/github-webhooks.json", "{}")
                .is_none(),
            "the signature-verifying GitHub lifecycle webhook is explicit public intake"
        );
        assert!(is_public_intake_write_route(
            "/mobile/cloud/github-webhooks.json"
        ));

        let refusal = security
            .governed_write_refusal("POST", "/beta/enrollments.json", "{}")
            .expect("enrollment must still require a verified session");
        assert_eq!(refusal.status_for_test(), "401 Unauthorized");

        let refusal = security
            .governed_write_refusal(
                "POST",
                "/mobile/account-deletion-requests.json",
                r#"{"confirm":true,"user_presence":"biometric_verified"}"#,
            )
            .expect("account deletion must require a verified session");
        assert_eq!(refusal.status_for_test(), "401 Unauthorized");
        assert!(is_governed_write_route(
            "/mobile/account-deletion-requests.json"
        ));
    }

    #[test]
    fn suspended_verified_actor_is_refused() {
        let _env = ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::set_var("MDX_SUSPENDED_ACTORS", "tenant_a:human:suspended");
        }
        let session = TrustedSession {
            tenant_id: "tenant_a".to_string(),
            actor_id: "human:suspended".to_string(),
            actor_role: "owner".to_string(),
            actor_kind: "human".to_string(),
            subject_actor_id: "human:suspended".to_string(),
            authority_scope: vec!["governed:*".to_string()],
            delegation_id: None,
        };
        let refusal = suspended_actor_refusal(&session).expect("actor should be suspended");
        assert_eq!(refusal.status_for_test(), "403 Forbidden");
        assert!(refusal.body_text().contains("production_actor_suspended"));
        unsafe {
            std::env::remove_var("MDX_SUSPENDED_ACTORS");
        }
    }

    #[test]
    fn production_gates_the_formerly_ungated_writers() {
        // These mutating routes used to classify as Ungated and ran with no
        // session in production. They are governed writes now.
        let security = RequestSecurity::production_unverified();
        for path in [
            "/twin/capabilities.json",
            "/twin/skill-proposal-events.json",
            "/twin/guard-preflight.json",
            "/messages/controls.json",
            "/pages/stewardship.json",
            "/run-loop/evals_runner_agent",
        ] {
            let refusal = security
                .governed_write_refusal("POST", path, "{}")
                .unwrap_or_else(|| panic!("production must gate POST {path}"));
            assert_eq!(refusal.status_for_test(), "401 Unauthorized");
            assert!(
                refusal
                    .body_text()
                    .contains("production_governed_write_requires_verified_session")
            );
        }
    }

    #[test]
    fn production_refuses_an_anonymous_read() {
        let security = RequestSecurity::production_unverified();
        let refusal = security
            .classify_read("GET", "/receipts.json")
            .expect("production must refuse an anonymous read");
        assert_eq!(refusal.status_for_test(), "401 Unauthorized");
        assert!(
            refusal
                .body_text()
                .contains("production_read_requires_verified_session")
        );
    }

    #[test]
    fn production_keeps_operational_probes_open() {
        let security = RequestSecurity::production_unverified();
        for path in ["/health", "/status", "/status.json"] {
            assert!(
                security.classify_read("GET", path).is_none(),
                "operational probe {path} must stay open"
            );
        }
        // OPTIONS preflight carries no data and must answer for CORS.
        assert!(
            security
                .classify_read("OPTIONS", "/receipts.json")
                .is_none()
        );
    }

    #[test]
    fn local_demo_reads_stay_frictionless() {
        let security = RequestSecurity::local_demo();
        assert!(security.classify_read("GET", "/receipts.json").is_none());
        assert!(
            security
                .classify_read("GET", "/twin/sessions/projection.json")
                .is_none()
        );
    }

    #[test]
    fn local_demo_keeps_undeclared_posts_ungated() {
        let security = RequestSecurity::local_demo();
        assert!(
            security
                .governed_write_refusal("POST", "/twin/capabilities.json", "{}")
                .is_none()
        );
        assert!(
            security
                .governed_write_refusal("POST", "/run-loop/evals_runner_agent", "{}")
                .is_none()
        );
    }

    #[test]
    fn production_cors_never_wildcards_with_no_configured_origins() {
        let headers =
            cors_headers_with_config(DeploymentMode::Production, "GET / HTTP/1.1\r\n\r\n", "");
        assert!(!headers.contains("Access-Control-Allow-Origin: *"));
        assert!(!headers.contains("Access-Control-Allow-Origin"));
    }

    #[test]
    fn production_cors_echoes_a_configured_allowed_origin() {
        let configured = "https://app.tenant.example,https://admin.tenant.example";
        let request = "GET / HTTP/1.1\r\nOrigin: https://admin.tenant.example\r\n\r\n";
        let headers = cors_headers_with_config(DeploymentMode::Production, request, configured);
        assert!(headers.contains("Access-Control-Allow-Origin: https://admin.tenant.example"));
        assert!(!headers.contains("Access-Control-Allow-Origin: *"));
    }

    #[test]
    fn production_cors_refuses_an_unlisted_origin_without_wildcard() {
        let configured = "https://app.tenant.example";
        let request = "GET / HTTP/1.1\r\nOrigin: https://evil.example\r\n\r\n";
        let headers = cors_headers_with_config(DeploymentMode::Production, request, configured);
        // An unlisted origin is never echoed and never wildcarded; it falls back
        // to the first configured origin, so the browser blocks the evil origin.
        assert!(!headers.contains("Access-Control-Allow-Origin: *"));
        assert!(!headers.contains("https://evil.example"));
        assert!(headers.contains("Access-Control-Allow-Origin: https://app.tenant.example"));
    }

    #[test]
    fn local_demo_cors_is_wildcard() {
        let headers =
            cors_headers_with_config(DeploymentMode::LocalDemo, "GET / HTTP/1.1\r\n\r\n", "");
        assert!(headers.contains("Access-Control-Allow-Origin: *"));
    }
}
