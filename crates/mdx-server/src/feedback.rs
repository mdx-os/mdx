// The governed beta feedback capture route. It is the server half of the
// PR #895 beta feedback contract: it derives identity from the verified trusted
// session (local-secure/production) or the local-demo fixture, builds a capture,
// and records it ONLY through `record_beta_feedback_local_with_identity`, which
// runs `build_safe_feedback_payload` first. An unsafe payload is refused with a
// helpful message and records nothing; it is never silently stripped. The client
// never sends, and the receipt never holds, prompt text, model output, a message
// or page body, a secret, or a token.
use crate::RouteResponse;
use mdx_core::{
    BetaFeedbackCapture, BetaFeedbackError, FEEDBACK_RECEIPT_KIND, FORBIDDEN_CONTEXT_KEYS,
    FeedbackContextEntry, MdxKernel, feedback_autonomy_feedback, json_string_literal,
};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// The allowlisted, non-secret context fields a client may prefill. Anything
/// else is ignored here and, if smuggled as a value, refused by the kernel scan.
const CONTEXT_KEYS: &[&str] = &[
    "surface_state",
    "route_status",
    "deployment_mode",
    "app_version",
    "receipt_kind",
    "blocked_reason",
    "feature_area",
    "taxonomy_class",
    "scenario_ref",
    "simulation_run_id",
    "outcome_id",
    "verdict",
    "evidence_receipt_ids",
];

/// The ONLY top-level body keys a feedback request may carry: the capture shape,
/// the allowlisted context hints, and the identity-echo fields (which carry no
/// authority - the trusted session decides identity). A request body with any
/// other top-level key, or any key that names forbidden content, is refused and
/// records nothing; nothing is silently dropped.
const ACCEPTED_TOP_LEVEL: &[&str] = &[
    "surface",
    "route",
    "session_ref",
    "occurred_at",
    "category",
    "note",
    "surface_state",
    "route_status",
    "deployment_mode",
    "app_version",
    "receipt_kind",
    "blocked_reason",
    "feature_area",
    "taxonomy_class",
    "scenario_ref",
    "simulation_run_id",
    "outcome_id",
    "verdict",
    "evidence_receipt_ids",
    "tenant_id",
    "actor_id",
    "actor_role",
];

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/feedback/captures.json" => Some(handle_capture_post(method, body, kernel)),
        "/feedback/captures/projection.json" => Some(handle_capture_projection(method, kernel)),
        _ => None,
    }
}

fn handle_capture_post(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    Ok(RouteResponse::json(
        "200 OK",
        render_local_post_json(body, &mut kernel)?,
    ))
}

fn handle_capture_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    Ok(RouteResponse::json(
        "200 OK",
        render_capture_projection_json(&kernel),
    ))
}

/// Reject a body whose top-level keys carry forbidden content (e.g. `prompt`,
/// `secret`, `token`) or fall outside the accepted feedback shape. Returns the
/// refusal `(code, marker)` of the first offending key, or None if every
/// top-level key is accepted. A non-object body has no keys to reject here; the
/// kernel's `build_safe_feedback_payload` is the final enforcement layer.
fn reject_top_level_fields(body: &str) -> Option<(&'static str, String)> {
    let value = serde_json::from_str::<serde_json::Value>(body).ok()?;
    let object = value.as_object()?;
    for key in object.keys() {
        let lower = key.to_ascii_lowercase();
        if let Some(marker) = FORBIDDEN_CONTEXT_KEYS
            .iter()
            .find(|forbidden| lower.contains(**forbidden))
        {
            return Some(("unsafe_content", (*marker).to_string()));
        }
        if !ACCEPTED_TOP_LEVEL.contains(&key.as_str()) {
            // The key name is client-controlled; never echo it back.
            return Some(("unknown_field", String::new()));
        }
    }
    None
}

pub(crate) fn render_local_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    // Identity: the verified trusted session in local-secure/production; the
    // local-demo fixture otherwise. The body is never authority for identity.
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "tenant_local",
        "local_user",
        "viewer",
    );
    let verified = resolved.auth_session_status == "VERIFIED_TRUSTED_SESSION";

    // Refuse unsafe or unknown top-level fields before building anything. This is
    // a first gate; the kernel's build_safe_feedback_payload is the final one.
    if let Some((code, marker)) = reject_top_level_fields(body) {
        return Ok(render_refusal(
            resolved.auth_session_status,
            code,
            "body",
            &marker,
        ));
    }

    let surface = field(body, "surface").unwrap_or_default();
    let route = field(body, "route").unwrap_or_default();
    let occurred_at = field(body, "occurred_at").unwrap_or_default();
    let category = field(body, "category").unwrap_or_default();
    let note = field(body, "note").unwrap_or_default();
    // The session reference: in the verified case it references the trusted
    // session's subject, never a client claim; in local-demo it is the body
    // marker or a safe default. Either way the kernel scans it.
    let session_ref = if verified {
        resolved.identity.subject_actor_id.clone()
    } else {
        field(body, "session_ref").unwrap_or_else(|| "local-demo-session".to_string())
    };

    // Collect present, non-empty allowlisted context values as owned strings, then
    // borrow them into the capture. Unknown client keys are dropped here; a value
    // carrying a secret or private body is refused by the kernel scan below.
    let owned_context: Vec<(&'static str, String)> = CONTEXT_KEYS
        .iter()
        .filter_map(|&key| field(body, key).map(|value| (key, value)))
        .filter(|(_, value)| !value.trim().is_empty())
        .collect();
    let context: Vec<FeedbackContextEntry> = owned_context
        .iter()
        .map(|(key, value)| FeedbackContextEntry {
            key,
            value: value.as_str(),
        })
        .collect();

    let capture = BetaFeedbackCapture {
        surface: &surface,
        route: &route,
        tenant_id: &resolved.tenant_id,
        actor_id: &resolved.actor_id,
        session_ref: &session_ref,
        occurred_at: &occurred_at,
        category: &category,
        context: &context,
        note: &note,
    };

    match kernel.record_beta_feedback_local_with_identity(&capture, &resolved.identity) {
        Ok(report) => Ok(format!(
            r#"{{"name":"mdx-beta-feedback-capture-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"surface":{},"category":{},"feedback_receipt_id":{},"policy_decision_id":{},"context_count":{},"has_note":{},"provider_call_allowed":{},"worker_spawn_allowed":false,"live_substrate_required":false,"production_write_allowed":{}}}"#,
            json_string_literal(report.status),
            json_string_literal(&report.auth_session_status),
            json_string_literal(&resolved.tenant_id),
            json_string_literal(&resolved.actor_id),
            json_string_literal(&resolved.actor_role),
            json_string_literal(&report.surface),
            json_string_literal(&report.category),
            json_string_literal(&report.feedback_receipt_id),
            json_string_literal(&report.policy_decision_id),
            report.context_count,
            report.has_note,
            report.provider_call_allowed,
            report.production_write_allowed,
        )),
        Err(error) => Ok(refusal_json(resolved.auth_session_status, &error)),
    }
}

pub(crate) fn render_capture_projection_json(kernel: &MdxKernel) -> String {
    let current_identity = crate::request_security::current_verified_identity();
    let current_tenant = current_identity
        .as_ref()
        .map(|identity| identity.tenant_id.as_str());
    let current_actor = current_identity.as_ref().and_then(|identity| {
        (!is_feedback_operator_role(&identity.actor_role)).then_some(identity.actor_id.as_str())
    });
    let feedback = feedback_autonomy_feedback(kernel.ledger().entries());
    let projection = kernel.feedback_autonomy_projection();
    let lifecycle_by_ref: BTreeMap<String, String> = projection
        .rows
        .into_iter()
        .map(|row| (row.feedback_ref, row.lifecycle))
        .collect();
    let triage_by_ref: BTreeMap<String, BTreeMap<String, String>> = kernel
        .ledger()
        .query()
        .by_kind("triage.verdict.recorded")
        .iter()
        .filter_map(|receipt| {
            let entry_ref = receipt.payload.get("entry_ref")?;
            Some((entry_ref.clone(), receipt.payload.clone()))
        })
        .collect();
    let severity_by_ref: BTreeMap<String, String> = kernel
        .ledger()
        .query()
        .by_kind(FEEDBACK_RECEIPT_KIND)
        .iter()
        .filter_map(|receipt| {
            let severity = receipt.payload.get("context.taxonomy_class")?;
            ["P0", "P1", "P2", "P3"]
                .contains(&severity.as_str())
                .then(|| (receipt.receipt_id.clone(), severity.clone()))
        })
        .collect();
    let mut reports = Vec::new();
    for item in feedback {
        if let Some(tenant_id) = current_tenant
            && item.tenant_id != tenant_id
        {
            continue;
        }
        if let Some(actor_id) = current_actor
            && item.actor_id != actor_id
        {
            continue;
        }
        let lifecycle = lifecycle_by_ref
            .get(&item.feedback_ref)
            .map(String::as_str)
            .unwrap_or("captured");
        let (status, note) = capture_status(lifecycle, triage_by_ref.get(&item.feedback_ref));
        let severity = severity_by_ref
            .get(&item.feedback_ref)
            .map(String::as_str)
            .unwrap_or("unset");
        reports.push(format!(
            r#"{{"feedback_receipt_id":{},"surface":{},"route":{},"category":{},"severity":{},"status":{},"lifecycle":{},"status_note":{},"captured_at":{},"session_ref":{},"triage_entry_ref":{},"triage_transition_route":"/product/triage-verdicts.json","production_write_allowed":false}}"#,
            json_string_literal(&item.feedback_ref),
            json_string_literal(&item.surface),
            json_string_literal(&item.route),
            json_string_literal(&item.category),
            json_string_literal(severity),
            json_string_literal(status),
            json_string_literal(lifecycle),
            json_string_literal(&note),
            json_string_literal(&item.captured_at),
            json_string_literal(&item.session_ref),
            json_string_literal(&item.feedback_ref),
        ));
    }
    format!(
        r#"{{"name":"mdx-beta-feedback-capture-local-projection","status":"OK","route":"/feedback/captures/projection.json","writes_route":"/feedback/captures.json","autonomy_route":"/feedback/autonomy-runs.json","triage_transition_route":"/product/triage-verdicts.json","filtered_to_current_actor":{},"report_count":{},"reports":[{}],"production_write_allowed":false}}"#,
        current_actor.is_some(),
        reports.len(),
        reports.join(","),
    )
}

fn is_feedback_operator_role(role: &str) -> bool {
    matches!(
        role.trim().to_ascii_lowercase().as_str(),
        "owner" | "admin" | "tenant_admin" | "operator" | "security" | "cloud" | "infra"
    )
}

fn capture_status(
    lifecycle: &str,
    triage_verdict: Option<&BTreeMap<String, String>>,
) -> (&'static str, String) {
    match lifecycle {
        "closed_loop_replied" | "closeout_published" => {
            return (
                "resolved",
                "MDx recorded a closeout and linked the feedback back to the loop.".to_string(),
            );
        }
        "forge_requested" | "evidence_attached" => {
            return (
                "working",
                "MDx shaped this into governed work and attached evidence.".to_string(),
            );
        }
        "acknowledged" | "assessed" | "assessment_published" | "work_shaped" => {
            return (
                "triaged",
                "MDx has assessed the report and is deciding the next governed move.".to_string(),
            );
        }
        _ => {}
    }
    if let Some(verdict) = triage_verdict {
        let value = |key: &str| verdict.get(key).map(String::as_str).unwrap_or("");
        return match value("verdict") {
            "accepted" if !value("accepted_record_id").trim().is_empty() => (
                "working",
                "A human accepted this report into governed product work.".to_string(),
            ),
            "accepted" => (
                "triaged",
                "A human accepted this report and it is waiting for shaped work.".to_string(),
            ),
            "duplicate" => (
                "resolved",
                "A human marked this report as a duplicate.".to_string(),
            ),
            "declined" => (
                "resolved",
                "A human declined this report for now.".to_string(),
            ),
            "snoozed" => (
                "triaged",
                "A human snoozed this report for later review.".to_string(),
            ),
            _ => ("triaged", "A human triage verdict is recorded.".to_string()),
        };
    }
    (
        "received",
        "MDx received the report and kept the receipt.".to_string(),
    )
}

/// Map a kernel refusal to a response.
fn refusal_json(auth_session_status: &str, error: &BetaFeedbackError) -> String {
    let (code, field_name, marker) = classify(error);
    render_refusal(auth_session_status, code, &field_name, &marker)
}

/// Build a refusal response. It names WHY (a code, a field, and a generic marker
/// like "secret") but never echoes the offending content. HTTP stays 200 with an
/// explicit refused status so the UI can show the helpful message.
fn render_refusal(auth_session_status: &str, code: &str, field_name: &str, marker: &str) -> String {
    format!(
        r#"{{"name":"mdx-beta-feedback-capture-local-post","status":"BETA_FEEDBACK_REFUSED","auth_session_required":true,"auth_session_status":{},"recorded":false,"refusal_code":{},"refused_field":{},"refused_marker":{},"message":{},"production_write_allowed":false}}"#,
        json_string_literal(auth_session_status),
        json_string_literal(code),
        json_string_literal(field_name),
        json_string_literal(marker),
        json_string_literal(user_message(code)),
    )
}

fn classify(error: &BetaFeedbackError) -> (&'static str, String, String) {
    match error {
        BetaFeedbackError::Missing(field) => ("missing_field", (*field).to_string(), String::new()),
        BetaFeedbackError::UnknownSurface(_) => {
            ("unknown_surface", "surface".to_string(), String::new())
        }
        BetaFeedbackError::UnknownCategory(_) => {
            ("unknown_category", "category".to_string(), String::new())
        }
        BetaFeedbackError::UnknownContextKey(_) => {
            ("unknown_context_key", "context".to_string(), String::new())
        }
        BetaFeedbackError::ForbiddenContextKey(_) => {
            ("unsafe_content", "context".to_string(), String::new())
        }
        BetaFeedbackError::ForbiddenValue { field, marker } => {
            ("unsafe_content", field.clone(), marker.clone())
        }
        BetaFeedbackError::RouteNotPathOnly { reason } => {
            ("invalid_route", "route".to_string(), (*reason).to_string())
        }
        BetaFeedbackError::NoteTooLong { .. } => {
            ("note_too_long", "note".to_string(), String::new())
        }
    }
}

fn user_message(code: &str) -> &'static str {
    match code {
        "unsafe_content" => {
            "That feedback includes content MDx will not record. Remove secrets, prompts, outputs, or private bodies and try again."
        }
        "invalid_route" => {
            "That feedback could not be linked to a page. Please try again from the page you are on."
        }
        "note_too_long" => "Your note is a little long. Please shorten it and try again.",
        "unknown_surface" => "Feedback from this area is not supported yet.",
        "unknown_category" => "Please pick one of the offered feedback types.",
        "unknown_field" => {
            "That feedback included details MDx does not accept. Please use the feedback form and try again."
        }
        "missing_field" => "Some feedback details are missing. Please try again.",
        _ => "That feedback could not be recorded. Please try again.",
    }
}

/// Read a JSON string field from a flat object body. Mirrors the other local
/// route handlers; no nested parsing is needed because the client sends flat
/// safe fields only.
fn field(body: &str, key: &str) -> Option<String> {
    let after_key = body.split(&format!("\"{key}\"")).nth(1)?;
    let after_colon = after_key.split_once(':')?.1.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            out.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Some(out);
        } else {
            out.push(character);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{AdmittedIdentity, GovernedWriteIdentity, TriageVerdict};

    fn read_identity(actor_id: &str, actor_role: &str) -> AdmittedIdentity {
        AdmittedIdentity {
            deployment_mode: "local-secure",
            tenant_id: "tenant_local".to_string(),
            actor_id: actor_id.to_string(),
            actor_role: actor_role.to_string(),
            actor_kind: "human".to_string(),
            subject_actor_id: actor_id.to_string(),
            authority_scope: Vec::new(),
            delegation_id: None,
            identity_source: "trusted_session",
            production_write_allowed: false,
        }
    }

    #[test]
    fn capture_projection_reports_native_macos_feedback_status() {
        let mut kernel = MdxKernel::boot_local();
        let context = [FeedbackContextEntry {
            key: "taxonomy_class",
            value: "P1",
        }];
        let capture = BetaFeedbackCapture {
            surface: "native_macos",
            route: "/forge",
            tenant_id: "tenant_local",
            actor_id: "human:engineer",
            session_ref: "session:macos",
            occurred_at: "2026-07-01T11:00:00Z",
            category: "blocked",
            context: &context,
            note: "Please make the proof trail easier to read.",
        };
        let report = kernel
            .record_beta_feedback_local_with_identity(
                &capture,
                &GovernedWriteIdentity::local_demo("human:engineer"),
            )
            .expect("feedback recorded");

        let body = render_capture_projection_json(&kernel);
        let value: serde_json::Value = serde_json::from_str(&body).expect("projection json");

        assert_eq!(value["status"], "OK");
        assert_eq!(value["report_count"], 1);
        assert_eq!(
            value["reports"][0]["feedback_receipt_id"],
            report.feedback_receipt_id
        );
        assert_eq!(value["reports"][0]["surface"], "native_macos");
        assert_eq!(value["reports"][0]["route"], "/forge");
        assert_eq!(value["reports"][0]["severity"], "P1");
        assert_eq!(value["reports"][0]["status"], "received");
        assert_eq!(value["reports"][0]["production_write_allowed"], false);
    }

    #[test]
    fn participant_feedback_read_is_self_only_while_operator_read_is_tenant_wide() {
        let mut kernel = MdxKernel::boot_local();
        for actor in ["engineer_1", "engineer_2"] {
            kernel
                .record_beta_feedback_local_with_identity(
                    &BetaFeedbackCapture {
                        surface: "native_macos",
                        route: "/forge",
                        tenant_id: "tenant_local",
                        actor_id: actor,
                        session_ref: "session:macos",
                        occurred_at: "2026-07-01T11:00:00Z",
                        category: "bug",
                        context: &[],
                        note: "The run did not reach review.",
                    },
                    &GovernedWriteIdentity::local_demo(actor),
                )
                .expect("feedback recorded");
        }

        {
            let _guard = crate::request_security::set_verified_identity(Some(read_identity(
                "engineer_1",
                "member",
            )));
            let body = render_capture_projection_json(&kernel);
            let value: serde_json::Value = serde_json::from_str(&body).expect("projection json");
            assert_eq!(value["report_count"], 1);
            assert_eq!(value["filtered_to_current_actor"], true);
        }

        let _guard =
            crate::request_security::set_verified_identity(Some(read_identity("founder", "owner")));
        let body = render_capture_projection_json(&kernel);
        let value: serde_json::Value = serde_json::from_str(&body).expect("projection json");
        assert_eq!(value["report_count"], 2);
        assert_eq!(value["filtered_to_current_actor"], false);
    }

    #[test]
    fn capture_projection_moves_with_manual_triage_verdicts() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:engineer");
        let capture = BetaFeedbackCapture {
            surface: "forge",
            route: "/forge",
            tenant_id: "tenant_local",
            actor_id: "human:engineer",
            session_ref: "session:triage",
            occurred_at: "2026-07-01T11:00:00Z",
            category: "bug",
            context: &[],
            note: "The run list needs clearer controls.",
        };
        let report = kernel
            .record_beta_feedback_local_with_identity(&capture, &identity)
            .expect("feedback recorded");
        kernel
            .save_triage_verdict_local_with_identity(
                TriageVerdict {
                    tenant_id: "tenant_local",
                    actor_id: "human:engineer",
                    entry_ref: &report.feedback_receipt_id,
                    verdict: "accepted",
                    note: "Taking this into product work.",
                    accept_as: "work_item",
                    accept_title: "Clarify run list controls",
                    bet_id: "",
                    duplicate_of: "",
                },
                &identity,
            )
            .expect("triage verdict");

        let body = render_capture_projection_json(&kernel);
        let value: serde_json::Value = serde_json::from_str(&body).expect("projection json");

        assert_eq!(
            value["triage_transition_route"],
            "/product/triage-verdicts.json"
        );
        assert_eq!(value["reports"][0]["status"], "working");
        assert_eq!(
            value["reports"][0]["triage_entry_ref"],
            report.feedback_receipt_id
        );
        assert_eq!(
            value["reports"][0]["triage_transition_route"],
            "/product/triage-verdicts.json"
        );
    }
}
