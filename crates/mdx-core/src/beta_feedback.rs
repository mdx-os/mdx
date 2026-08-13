// The beta feedback capture contract.
//
// Every major beta surface needs a way for an engineer to report a failure, ask
// for help, or leave product feedback. The danger is that a feedback payload is
// the easiest place to leak private content: a prompt, a model answer, a private
// message body, a private page body, or a secret. This contract makes that leak
// structurally hard.
//
// A feedback capture carries ONLY safe-context fields: which surface, which
// route, which tenant, who (actor and session reference), when, a bounded
// category, an allowlisted set of non-secret context keys, and the user's own
// optional note. The boundary is enforced at TWO levels, because a key allowlist
// alone is not enough:
//   - structural: required fields, surface and category allowlists, an allowlist
//     of context keys, and a path-only route; and
//   - value-level: the note, every context value, the route, and the session
//     reference are all scanned for forbidden content markers (secret, token,
//     api_key, password, prompt, output, private body, ...) and for token-like
//     strings (sk-..., bearer ..., JWTs, PEM blocks). Anything matching is
//     REFUSED, never silently stripped, so a wiring mistake or a pasted secret
//     fails the contract instead of leaking.
//
// This module is pure: no network, no storage, no secret access. It is the
// kernel-side boundary the server feedback route and the quiet UI affordance both
// sit behind. It is locked to `docs/BETA-FEEDBACK-CONTRACT.md` by
// `make beta-feedback-contract-check`.

use crate::{
    ActionKind, ActorId, CorrelationIds, GovernedWriteIdentity, LoopId, LoopRun, MdxKernel,
    StorageProvider, TenantId, TraceId, WorkflowId, payload,
};

/// The governed local POST route that records a feedback capture.
pub const BETA_FEEDBACK_ROUTE: &str = "/feedback/captures.json";

/// The surfaces a beta engineer can send feedback from.
pub const FEEDBACK_SURFACES: &[&str] = &[
    "twin",
    "forge",
    "message",
    "pages",
    "marketplace",
    "auth",
    "home",
    "native_macos",
    "telemetry",
];

/// The bounded set of feedback categories. Free-form category strings are
/// refused so triage stays sortable.
pub const FEEDBACK_CATEGORIES: &[&str] = &["bug", "blocked", "confusing", "idea", "other"];

/// Context keys a feedback payload MAY carry. Everything here is non-secret
/// product telemetry: it describes the situation, never its private content.
pub const ALLOWED_CONTEXT_KEYS: &[&str] = &[
    "surface_state",   // e.g. empty, populated, error
    "route_status",    // e.g. 200, blocked, fail_closed
    "deployment_mode", // local-demo | local-secure | production
    "app_version",     // build identifier
    "receipt_kind",    // the kind of a receipt in view, not its payload
    "blocked_reason",  // a refusal CODE (e.g. production_missing_trusted_session)
    "feature_area",    // a coarse area tag
    "taxonomy_class",  // P0 | P1 | P2 | P3 | instrumentation_gap | scenario_flaw
    "scenario_ref",    // simulation scenario id, e.g. CL-3
    "simulation_run_id",
    "outcome_id",
    "verdict",
    "evidence_receipt_ids", // comma-separated receipt ids only, never payloads
];

/// Context keys (and key substrings) that are FORBIDDEN in a feedback payload
/// because they would carry private content or a secret. A capture naming any of
/// these is refused. Kept in lockstep with `docs/BETA-FEEDBACK-CONTRACT.md`.
pub const FORBIDDEN_CONTEXT_KEYS: &[&str] = &[
    "prompt",
    "output",
    "answer",
    "completion",
    "message_body",
    "page_body",
    "draft_text",
    "secret",
    "token",
    "api_key",
    "password",
    "credential",
    "persona_context",
];

/// Case-insensitive substrings that, if they appear in ANY value (the note, a
/// context value, the route, or the session reference), mark that value as
/// carrying a secret or private content. Matching a marker refuses the whole
/// capture. Kept in lockstep with `docs/BETA-FEEDBACK-CONTRACT.md`.
pub const FORBIDDEN_VALUE_MARKERS: &[&str] = &[
    // secrets and credentials
    "secret",
    "password",
    "passwd",
    "api_key",
    "apikey",
    "api key",
    "bearer ",
    "authorization:",
    "private key",
    "-----begin",
    "client_secret",
    "access_token",
    "refresh_token",
    "credential",
    "token=",
    "token:",
    // prompt, model output, and private-body content
    "system prompt",
    "prompt:",
    "model output",
    "output:",
    "completion:",
    "assistant:",
    "ignore previous instructions",
    "message body",
    "page body",
    "message_body",
    "page_body",
    "draft_text",
];

/// The recorded receipt kind for an admitted feedback capture.
pub const FEEDBACK_RECEIPT_KIND: &str = "beta.feedback.captured";

/// The maximum length of the user's own note. The note is the only free text in
/// a feedback payload; it is the user's own words, length-bounded and scanned.
pub const FEEDBACK_NOTE_MAX_CHARS: usize = 2000;

/// A single safe context entry. Both key and value are caller-supplied product
/// telemetry; the key must be allowlisted and the value must be marker-free.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackContextEntry<'a> {
    pub key: &'a str,
    pub value: &'a str,
}

/// A feedback capture request from a beta surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaFeedbackCapture<'a> {
    pub surface: &'a str,
    /// The route the user was on. Must be a PATH only: no scheme, query string,
    /// fragment, whitespace, or token-bearing string.
    pub route: &'a str,
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// A reference to the session (e.g. a session id or verified-session marker),
    /// NOT its contents. Required, and scanned for markers.
    pub session_ref: &'a str,
    /// ISO-8601 timestamp supplied by the caller (the kernel does not read a clock).
    pub occurred_at: &'a str,
    pub category: &'a str,
    pub context: &'a [FeedbackContextEntry<'a>],
    /// The user's own optional note. Empty means no note. Scanned for markers.
    pub note: &'a str,
}

/// The sanitized, safe-to-record feedback payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeFeedbackPayload {
    pub receipt_kind: &'static str,
    pub surface: String,
    pub route: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub session_ref: String,
    pub occurred_at: String,
    pub category: String,
    pub context: Vec<(String, String)>,
    pub note: String,
}

/// Why a feedback capture was refused. Every refusal is explicit; nothing is
/// silently stripped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BetaFeedbackError {
    Missing(&'static str),
    UnknownSurface(String),
    UnknownCategory(String),
    ForbiddenContextKey(String),
    UnknownContextKey(String),
    NoteTooLong {
        len: usize,
        max: usize,
    },
    /// A value (the note, a context value, the route, or the session reference)
    /// contained a forbidden content marker or a token-like string. `field`
    /// names where it was found; `marker` is the matched marker.
    ForbiddenValue {
        field: String,
        marker: String,
    },
    /// The route was not a bare path (it had a scheme, query, fragment, or
    /// whitespace). `reason` names which.
    RouteNotPathOnly {
        reason: &'static str,
    },
}

fn forbidden_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    FORBIDDEN_CONTEXT_KEYS
        .iter()
        .any(|forbidden| lower.contains(forbidden))
}

/// True if a single whitespace/quote-delimited word looks like a credential or
/// token: a known secret prefix with enough length, or a JWT, or a PEM block.
fn token_like(word: &str) -> bool {
    let trimmed =
        word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != '.');
    if trimmed.len() < 12 {
        // A JWT or PEM marker is caught by FORBIDDEN_VALUE_MARKERS / below; short
        // words are not token-like.
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    const SECRET_PREFIXES: &[&str] = &["sk-", "xai-", "ghp_", "gho_", "github_pat_", "akia"];
    if SECRET_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return true;
    }
    // JWT: three dot-separated base64url segments beginning with the standard
    // `{"alg"...}` header encoding `eyJ`.
    if lower.starts_with("eyj") && trimmed.matches('.').count() >= 2 {
        return true;
    }
    false
}

/// Scan a value for a forbidden content marker or a token-like string. Returns
/// the matched marker, or None if the value is clean. Exposed to sibling modules
/// (e.g. the app-health telemetry contract) so a second metrics-only ingest can
/// reuse the EXACT same value-level scan rather than reimplementing it.
pub(crate) fn value_forbidden_marker(value: &str) -> Option<String> {
    let lower = value.to_ascii_lowercase();
    if let Some(marker) = FORBIDDEN_VALUE_MARKERS
        .iter()
        .find(|marker| lower.contains(**marker))
    {
        return Some((*marker).to_string());
    }
    for word in value.split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '`') {
        if token_like(word) {
            return Some("token_like_string".to_string());
        }
    }
    None
}

/// Check that a route is a bare path: starts with `/`, no scheme, query,
/// fragment, or whitespace. Shared with sibling modules so a route-path field on
/// another metrics-only contract enforces the identical bare-path rule.
pub(crate) fn route_path_only_reason(route: &str) -> Option<&'static str> {
    if route.contains("://") {
        return Some("scheme");
    }
    if route.contains('?') {
        return Some("query_string");
    }
    if route.contains('#') {
        return Some("fragment");
    }
    if route.chars().any(|c| c.is_whitespace()) {
        return Some("whitespace");
    }
    if !route.starts_with('/') {
        return Some("not_a_path");
    }
    None
}

/// Turn a feedback capture into a safe payload, or refuse it. The boundary:
/// required fields present (including session_ref), surface and category from
/// their allowlists, a path-only route, every context key allowlisted and never
/// forbidden, the note within its cap, and EVERY value (note, context values,
/// route, session_ref) free of forbidden content markers and token-like strings.
/// Nothing is silently stripped; a value carrying private content is refused.
pub fn build_safe_feedback_payload(
    capture: &BetaFeedbackCapture<'_>,
) -> Result<SafeFeedbackPayload, BetaFeedbackError> {
    if capture.surface.trim().is_empty() {
        return Err(BetaFeedbackError::Missing("surface"));
    }
    if capture.route.trim().is_empty() {
        return Err(BetaFeedbackError::Missing("route"));
    }
    if capture.tenant_id.trim().is_empty() {
        return Err(BetaFeedbackError::Missing("tenant_id"));
    }
    if capture.actor_id.trim().is_empty() {
        return Err(BetaFeedbackError::Missing("actor_id"));
    }
    if capture.session_ref.trim().is_empty() {
        return Err(BetaFeedbackError::Missing("session_ref"));
    }
    if capture.occurred_at.trim().is_empty() {
        return Err(BetaFeedbackError::Missing("occurred_at"));
    }
    if capture.category.trim().is_empty() {
        return Err(BetaFeedbackError::Missing("category"));
    }
    if !FEEDBACK_SURFACES.contains(&capture.surface) {
        return Err(BetaFeedbackError::UnknownSurface(
            capture.surface.to_string(),
        ));
    }
    if !FEEDBACK_CATEGORIES.contains(&capture.category) {
        return Err(BetaFeedbackError::UnknownCategory(
            capture.category.to_string(),
        ));
    }

    // The route must be a bare path, and then marker-clean (a token-bearing path
    // like /x/sk-abc... is still refused).
    if let Some(reason) = route_path_only_reason(capture.route) {
        return Err(BetaFeedbackError::RouteNotPathOnly { reason });
    }
    if let Some(marker) = value_forbidden_marker(capture.route) {
        return Err(BetaFeedbackError::ForbiddenValue {
            field: "route".to_string(),
            marker,
        });
    }

    // The session reference must be a reference, not its contents.
    if let Some(marker) = value_forbidden_marker(capture.session_ref) {
        return Err(BetaFeedbackError::ForbiddenValue {
            field: "session_ref".to_string(),
            marker,
        });
    }

    let mut context = Vec::with_capacity(capture.context.len());
    for entry in capture.context {
        if forbidden_key(entry.key) {
            return Err(BetaFeedbackError::ForbiddenContextKey(
                entry.key.to_string(),
            ));
        }
        if !ALLOWED_CONTEXT_KEYS.contains(&entry.key) {
            return Err(BetaFeedbackError::UnknownContextKey(entry.key.to_string()));
        }
        if let Some(marker) = value_forbidden_marker(entry.value) {
            return Err(BetaFeedbackError::ForbiddenValue {
                field: format!("context:{}", entry.key),
                marker,
            });
        }
        context.push((entry.key.to_string(), entry.value.to_string()));
    }

    let note_len = capture.note.chars().count();
    if note_len > FEEDBACK_NOTE_MAX_CHARS {
        return Err(BetaFeedbackError::NoteTooLong {
            len: note_len,
            max: FEEDBACK_NOTE_MAX_CHARS,
        });
    }
    if let Some(marker) = value_forbidden_marker(capture.note) {
        return Err(BetaFeedbackError::ForbiddenValue {
            field: "note".to_string(),
            marker,
        });
    }

    Ok(SafeFeedbackPayload {
        receipt_kind: FEEDBACK_RECEIPT_KIND,
        surface: capture.surface.to_string(),
        route: capture.route.to_string(),
        tenant_id: capture.tenant_id.to_string(),
        actor_id: capture.actor_id.to_string(),
        session_ref: capture.session_ref.to_string(),
        occurred_at: capture.occurred_at.to_string(),
        category: capture.category.to_string(),
        context,
        note: capture.note.to_string(),
    })
}

/// The outcome of an admitted feedback capture: the receipt that recorded it and
/// the safe, non-content summary fields. It never carries the note text or any
/// context value back out; the receipt holds the safe payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BetaFeedbackReport {
    pub status: &'static str,
    pub surface: String,
    pub category: String,
    pub feedback_receipt_id: String,
    pub policy_decision_id: String,
    pub auth_session_status: String,
    pub context_count: usize,
    pub has_note: bool,
    pub provider_call_allowed: bool,
    pub production_write_allowed: bool,
}

impl<S: StorageProvider> MdxKernel<S> {
    /// Record a feedback capture from local-demo (the fixture identity is the
    /// reporter). local-secure and production use the identity-aware variant.
    pub fn record_beta_feedback_local(
        &mut self,
        capture: &BetaFeedbackCapture<'_>,
    ) -> Result<BetaFeedbackReport, BetaFeedbackError> {
        let identity = GovernedWriteIdentity::local_demo(capture.actor_id);
        self.record_beta_feedback_local_with_identity(capture, &identity)
    }

    /// Record a feedback capture, recording the verified trusted session in the
    /// receipt. The safety boundary runs FIRST: `build_safe_feedback_payload`
    /// refuses any unsafe capture before a single field is written, so a prompt,
    /// model output, private body, or secret never reaches the ledger.
    pub fn record_beta_feedback_local_with_identity(
        &mut self,
        capture: &BetaFeedbackCapture<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<BetaFeedbackReport, BetaFeedbackError> {
        let safe = build_safe_feedback_payload(capture)?;
        let auth_session_status = if identity.identity_source == "local_demo_fixture" {
            "ACCEPTED_LOCAL_STUB"
        } else {
            "VERIFIED_TRUSTED_SESSION"
        };

        let correlation = CorrelationIds {
            tenant_id: TenantId::new(&safe.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(&safe.actor_id),
            loop_id: LoopId::new("beta_feedback"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision = self.decide_with_receipt(&correlation, ActionKind::RecordBetaFeedback);

        let has_note = !safe.note.trim().is_empty();
        // Only safe, scanned fields ride onto the receipt. The note and every
        // context value already passed the value-level scan in
        // `build_safe_feedback_payload`; no prompt, output, or private body
        // reaches here.
        let mut receipt_payload = payload(&[
            ("surface", safe.surface.as_str()),
            ("route", safe.route.as_str()),
            ("session_ref", safe.session_ref.as_str()),
            ("occurred_at", safe.occurred_at.as_str()),
            ("category", safe.category.as_str()),
            ("note", safe.note.as_str()),
            ("auth_session_status", auth_session_status),
            ("identity_source", identity.identity_source.as_str()),
            ("identity_actor_kind", identity.actor_kind.as_str()),
            (
                "identity_subject_actor_id",
                identity.subject_actor_id.as_str(),
            ),
            ("identity_delegation_id", identity.delegation_id.as_str()),
            ("source_route", BETA_FEEDBACK_ROUTE),
            ("terminal_state", "BETA_FEEDBACK_RECORDED"),
            ("provider_call_allowed", "false"),
            ("worker_spawn_allowed", "false"),
            ("production_write_allowed", "false"),
        ]);
        for (key, value) in &safe.context {
            receipt_payload.insert(format!("context.{key}"), value.clone());
        }

        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_BETA_FEEDBACK",
            &correlation,
            &decision,
            FEEDBACK_RECEIPT_KIND,
            receipt_payload,
        );
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "BETA_FEEDBACK_RECORDED".to_string();
        }

        Ok(BetaFeedbackReport {
            status: "BETA_FEEDBACK_RECORDED",
            surface: safe.surface,
            category: safe.category,
            feedback_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            auth_session_status: auth_session_status.to_string(),
            context_count: safe.context.len(),
            has_note,
            provider_call_allowed: false,
            production_write_allowed: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid<'a>(
        context: &'a [FeedbackContextEntry<'a>],
        note: &'a str,
    ) -> BetaFeedbackCapture<'a> {
        BetaFeedbackCapture {
            surface: "forge",
            route: "/forge",
            tenant_id: "tenant_local",
            actor_id: "engineer_1",
            session_ref: "sess_abc",
            occurred_at: "2026-06-09T00:00:00Z",
            category: "blocked",
            context,
            note,
        }
    }

    #[test]
    fn a_valid_capture_produces_a_safe_payload() {
        let context = [
            FeedbackContextEntry {
                key: "surface_state",
                value: "error",
            },
            FeedbackContextEntry {
                key: "blocked_reason",
                value: "production_missing_trusted_session",
            },
        ];
        let payload = build_safe_feedback_payload(&valid(&context, "could not request a build"))
            .expect("valid capture");
        assert_eq!(payload.receipt_kind, "beta.feedback.captured");
        assert_eq!(payload.surface, "forge");
        assert_eq!(payload.category, "blocked");
        assert_eq!(payload.context.len(), 2);
        assert_eq!(payload.note, "could not request a build");
    }

    #[test]
    fn a_forbidden_context_key_is_refused_not_stripped() {
        for bad in [
            "prompt",
            "model_output",
            "page_body",
            "user_password",
            "api_key_value",
        ] {
            let context = [FeedbackContextEntry {
                key: bad,
                value: "x",
            }];
            let result = build_safe_feedback_payload(&valid(&context, ""));
            assert!(
                matches!(result, Err(BetaFeedbackError::ForbiddenContextKey(_))),
                "key {bad} must be refused as forbidden, got {result:?}"
            );
        }
    }

    #[test]
    fn an_unknown_context_key_is_refused() {
        let context = [FeedbackContextEntry {
            key: "random_extra_field",
            value: "x",
        }];
        let result = build_safe_feedback_payload(&valid(&context, ""));
        assert!(matches!(
            result,
            Err(BetaFeedbackError::UnknownContextKey(_))
        ));
    }

    #[test]
    fn missing_required_fields_are_refused() {
        let mut capture = valid(&[], "");
        capture.tenant_id = "";
        assert_eq!(
            build_safe_feedback_payload(&capture),
            Err(BetaFeedbackError::Missing("tenant_id"))
        );
        let mut capture = valid(&[], "");
        capture.actor_id = "  ";
        assert_eq!(
            build_safe_feedback_payload(&capture),
            Err(BetaFeedbackError::Missing("actor_id"))
        );
    }

    #[test]
    fn an_empty_session_ref_is_refused() {
        let mut capture = valid(&[], "");
        capture.session_ref = "";
        assert_eq!(
            build_safe_feedback_payload(&capture),
            Err(BetaFeedbackError::Missing("session_ref"))
        );
        let mut capture = valid(&[], "");
        capture.session_ref = "   ";
        assert_eq!(
            build_safe_feedback_payload(&capture),
            Err(BetaFeedbackError::Missing("session_ref"))
        );
    }

    #[test]
    fn an_unknown_surface_or_category_is_refused() {
        let mut capture = valid(&[], "");
        capture.surface = "secret_admin_console";
        // "secret_admin_console" is an unknown surface; the surface allowlist
        // catches it before any value scan.
        assert!(matches!(
            build_safe_feedback_payload(&capture),
            Err(BetaFeedbackError::UnknownSurface(_))
        ));
        let mut capture = valid(&[], "");
        capture.category = "exfiltrate";
        assert!(matches!(
            build_safe_feedback_payload(&capture),
            Err(BetaFeedbackError::UnknownCategory(_))
        ));
    }

    #[test]
    fn an_over_long_note_is_refused() {
        let long = "x".repeat(FEEDBACK_NOTE_MAX_CHARS + 1);
        let capture = valid(&[], &long);
        assert!(matches!(
            build_safe_feedback_payload(&capture),
            Err(BetaFeedbackError::NoteTooLong { .. })
        ));
    }

    #[test]
    fn a_note_with_prompt_output_secret_or_body_markers_is_refused() {
        let jwt_like = [
            "token: eyJhbGciOiJIUzI1NiIs",
            "InR5cCI6IkpXVCJ9",
            "abcdefghij",
            "klmnop",
        ]
        .join(".");
        let cases = vec![
            "here is the system prompt: you are a helpful assistant".to_string(),
            "the model output: the salary is 200k".to_string(),
            "my password is hunter2".to_string(),
            "the private page body: confidential salary data".to_string(),
            "stash this api_key for me".to_string(),
            jwt_like,
        ];
        for leaky in cases {
            let capture = valid(&[], leaky.as_str());
            assert!(
                matches!(
                    build_safe_feedback_payload(&capture),
                    Err(BetaFeedbackError::ForbiddenValue { .. })
                ),
                "note {leaky:?} must be refused, got {:?}",
                build_safe_feedback_payload(&capture)
            );
        }
    }

    // A token-like string built at runtime so no contiguous secret-looking
    // literal is committed to source (the repo-wide secret scanner flags
    // `sk-[A-Za-z0-9]{20,}`). The assembled value still exercises `token_like`.
    fn synthetic_token() -> String {
        format!("{}{}", "sk", "-ABCDEFGHIJKLMNOPqrstuvwx")
    }

    #[test]
    fn a_context_value_with_a_secret_or_token_is_refused() {
        let token = synthetic_token();
        let bad_values = [
            "secret-sauce-xyz".to_string(),
            format!("token={}", "abc123def456"),
            "api_key:zzz".to_string(),
            "password=hunter2".to_string(),
            token,
        ];
        for bad_value in bad_values {
            let context = [FeedbackContextEntry {
                key: "feature_area",
                value: bad_value.as_str(),
            }];
            let result = build_safe_feedback_payload(&valid(&context, ""));
            assert!(
                matches!(result, Err(BetaFeedbackError::ForbiddenValue { .. })),
                "context value {bad_value:?} must be refused, got {result:?}"
            );
        }
    }

    #[test]
    fn a_route_that_is_not_a_bare_path_is_refused() {
        for bad_route in [
            "https://app.example.com/forge?token=abc",
            "/forge?token=secret123",
            "/forge#section",
            "http://localhost/twin",
            "/forge with spaces",
            "forge",
        ] {
            let mut capture = valid(&[], "");
            capture.route = bad_route;
            let result = build_safe_feedback_payload(&capture);
            assert!(
                matches!(
                    result,
                    Err(BetaFeedbackError::RouteNotPathOnly { .. })
                        | Err(BetaFeedbackError::ForbiddenValue { .. })
                ),
                "route {bad_route:?} must be refused, got {result:?}"
            );
        }
    }

    #[test]
    fn a_session_ref_carrying_a_token_is_refused() {
        let token = synthetic_token();
        // Bare token: refused by `token_like` (no marker substring matches it).
        let mut capture = valid(&[], "");
        capture.session_ref = token.as_str();
        assert!(
            matches!(
                build_safe_feedback_payload(&capture),
                Err(BetaFeedbackError::ForbiddenValue { .. })
            ),
            "a bare token in session_ref must be refused"
        );
        // Bearer-prefixed: refused by the `bearer ` marker.
        let with_bearer = format!("bearer {token}");
        capture.session_ref = with_bearer.as_str();
        assert!(
            matches!(
                build_safe_feedback_payload(&capture),
                Err(BetaFeedbackError::ForbiddenValue { .. })
            ),
            "a bearer-prefixed token in session_ref must be refused"
        );
    }

    #[test]
    fn allowed_and_forbidden_key_sets_do_not_overlap() {
        for allowed in ALLOWED_CONTEXT_KEYS {
            assert!(
                !forbidden_key(allowed),
                "allowed key {allowed} must not match a forbidden substring"
            );
        }
    }

    #[test]
    fn benign_values_are_not_falsely_refused() {
        // The allowlisted context values used in practice, and a plain human
        // note, must pass the value scanner.
        for ok in [
            "could not request a build",
            "the page would not load and I was confused",
            "empty",
            "production_missing_trusted_session",
            "local-secure",
        ] {
            assert!(
                value_forbidden_marker(ok).is_none(),
                "benign value {ok:?} should not be flagged"
            );
        }
    }

    #[test]
    fn recording_a_valid_capture_writes_a_safe_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let context = [FeedbackContextEntry {
            key: "surface_state",
            value: "error",
        }];
        let capture = BetaFeedbackCapture {
            surface: "forge",
            route: "/forge",
            tenant_id: "tenant_local",
            actor_id: "engineer_1",
            session_ref: "sess_abc",
            occurred_at: "2026-06-09T00:00:00Z",
            category: "blocked",
            context: &context,
            note: "could not request a build",
        };
        let report = kernel
            .record_beta_feedback_local(&capture)
            .expect("valid capture recorded");
        assert_eq!(report.status, "BETA_FEEDBACK_RECORDED");
        assert_eq!(report.surface, "forge");
        assert_eq!(report.category, "blocked");
        assert_eq!(report.auth_session_status, "ACCEPTED_LOCAL_STUB");
        assert!(report.has_note);
        assert!(!report.feedback_receipt_id.is_empty());

        let receipt = kernel
            .ledger()
            .entries()
            .iter()
            .find(|r| r.kind == FEEDBACK_RECEIPT_KIND)
            .expect("feedback receipt recorded");
        assert_eq!(
            receipt.payload.get("surface").map(String::as_str),
            Some("forge")
        );
        assert_eq!(
            receipt
                .payload
                .get("context.surface_state")
                .map(String::as_str),
            Some("error")
        );
        assert_eq!(
            receipt
                .payload
                .get("production_write_allowed")
                .map(String::as_str),
            Some("false")
        );
        // No receipt field carries a forbidden marker.
        for value in receipt.payload.values() {
            assert!(
                value_forbidden_marker(value).is_none(),
                "receipt value {value:?} must be marker-clean"
            );
        }
    }

    #[test]
    fn an_unsafe_capture_records_nothing() {
        let mut kernel = MdxKernel::boot_local();
        let before = kernel
            .ledger()
            .entries()
            .iter()
            .filter(|r| r.kind == FEEDBACK_RECEIPT_KIND)
            .count();
        let capture = BetaFeedbackCapture {
            surface: "twin",
            route: "/",
            tenant_id: "tenant_local",
            actor_id: "engineer_1",
            session_ref: "sess_abc",
            occurred_at: "2026-06-09T00:00:00Z",
            category: "bug",
            context: &[],
            note: "the model output: my password is hunter2",
        };
        let result = kernel.record_beta_feedback_local(&capture);
        assert!(matches!(
            result,
            Err(BetaFeedbackError::ForbiddenValue { .. })
        ));
        let after = kernel
            .ledger()
            .entries()
            .iter()
            .filter(|r| r.kind == FEEDBACK_RECEIPT_KIND)
            .count();
        assert_eq!(before, after, "an unsafe capture must record no receipt");
    }

    #[test]
    fn record_beta_feedback_local_with_identity_produces_report_for_valid_capture() {
        let mut kernel = MdxKernel::boot_local();
        let capture = valid(&[], "could not request a build");
        let identity = crate::GovernedWriteIdentity::local_demo(capture.actor_id);
        let report = kernel
            .record_beta_feedback_local_with_identity(&capture, &identity)
            .expect("valid capture with identity recorded");
        assert_eq!(report.status, "BETA_FEEDBACK_RECORDED");
        assert_eq!(report.surface, "forge");
        assert_eq!(report.category, "blocked");
        assert_eq!(report.auth_session_status, "ACCEPTED_LOCAL_STUB");
        assert!(report.has_note);
        assert!(!report.feedback_receipt_id.is_empty());
    }

    #[test]
    fn missing_surface_is_refused() {
        let mut capture = valid(&[], "");
        capture.surface = "";
        assert_eq!(
            build_safe_feedback_payload(&capture),
            Err(BetaFeedbackError::Missing("surface"))
        );
    }

    #[test]
    fn missing_route_is_refused() {
        let mut capture = valid(&[], "");
        capture.route = "";
        assert_eq!(
            build_safe_feedback_payload(&capture),
            Err(BetaFeedbackError::Missing("route"))
        );
        let mut capture = valid(&[], "");
        capture.route = "   ";
        assert_eq!(
            build_safe_feedback_payload(&capture),
            Err(BetaFeedbackError::Missing("route"))
        );
    }

    #[test]
    fn missing_occurred_at_is_refused() {
        let mut capture = valid(&[], "");
        capture.occurred_at = "";
        assert_eq!(
            build_safe_feedback_payload(&capture),
            Err(BetaFeedbackError::Missing("occurred_at"))
        );
    }

    #[test]
    fn missing_category_is_refused() {
        let mut capture = valid(&[], "");
        capture.category = "";
        assert_eq!(
            build_safe_feedback_payload(&capture),
            Err(BetaFeedbackError::Missing("category"))
        );
    }

    #[test]
    fn a_bare_path_route_with_forbidden_marker_value_is_refused() {
        let mut capture = valid(&[], "");
        capture.route = "/forge/with-secret-in-path";
        let result = build_safe_feedback_payload(&capture);
        assert!(
            matches!(
                result,
                Err(BetaFeedbackError::ForbiddenValue { ref field, .. })
                    if field == "route"
            ),
            "bare path route with marker must hit ForbiddenValue on route, got {result:?}"
        );
    }
}
