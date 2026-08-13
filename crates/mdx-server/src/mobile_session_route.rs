use crate::{RouteResponse, mobile_pairing, request_security};
use jsonwebtoken::{EncodingKey, Header, encode};
use mdx_core::{MdxKernel, Receipt};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const RELAY_CREDENTIAL_LIFETIME_SECONDS: u64 = 15 * 60;
const MAX_SESSION_SCOPE: usize = 32;
const MAX_MOBILE_SESSIONS: usize = 32;

#[derive(Serialize)]
struct RelayClaims<'a> {
    sub: &'a str,
    tenant_id: &'a str,
    actor_id: &'a str,
    device_id: &'a str,
    purpose: &'static str,
    allowed_stream_ids: &'a [String],
    iss: &'static str,
    aud: &'static str,
    iat: usize,
    exp: usize,
}

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/mobile/sessions.json" => Some(session_projection(method, kernel)),
        "/mobile/relay-credentials.json" => Some(relay_credential(method, body, kernel)),
        _ => None,
    }
}

fn session_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("GET") {
        return Ok(method_not_allowed());
    }
    let verified = request_security::current_verified_identity();
    let tenant_id = verified
        .as_ref()
        .map(|identity| identity.tenant_id.as_str())
        .unwrap_or("local_tenant");
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned while projecting mobile sessions".to_string())?;
    let sessions = project_sessions(&kernel, tenant_id);
    validate_projected_sessions(&sessions)?;
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-forge-sessions",
            "status": "OK",
            "schema_version": 1,
            "tenant_id": tenant_id,
            "sessions": sessions,
            "canonical_source": "forge.run.event receipts",
            "raw_model_output_included": false,
            "raw_tool_output_included": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn relay_credential(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("POST") {
        return Ok(method_not_allowed());
    }
    let parsed: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => {
            return Ok(refusal(
                "mobile_relay_body_invalid",
                "request body must be JSON",
            ));
        }
    };
    let identity = request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let device_id = match identifier(&parsed, "device_id") {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let host_id = match identifier(&parsed, "host_id") {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    let session_ids = match session_scope(&parsed) {
        Ok(value) => value,
        Err(response) => return Ok(response),
    };
    if let Err(code) =
        mobile_pairing::require_active_pairing(&identity, &device_id, &host_id, kernel)
    {
        return Ok(refusal(
            &code,
            "an active device to host pairing is required before relay access",
        ));
    }
    {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned while authorizing mobile relay".to_string())?;
        for session_id in &session_ids {
            let belongs_to_tenant = kernel.ledger().entries().iter().any(|receipt| {
                receipt.kind == "forge.run.event"
                    && receipt.tenant_id.as_str() == identity.tenant_id
                    && !payload(receipt, "work_item_id").starts_with("mobile_cloud_")
                    && payload(receipt, "run_id") == session_id
            });
            if !belongs_to_tenant {
                return Ok(refusal(
                    "mobile_relay_session_scope_denied",
                    "every requested session must exist in the verified tenant",
                ));
            }
        }
    }
    let secret = std::env::var("MDX_MESSAGE_RELAY_JWT_SECRET")
        .or_else(|_| std::env::var("JWT_SECRET"))
        .ok()
        .filter(|secret| secret.len() >= 32);
    let Some(secret) = secret else {
        return Ok(refusal(
            "mobile_relay_signing_key_missing",
            "relay credentials require an explicit MDX_MESSAGE_RELAY_JWT_SECRET of at least 32 characters",
        ));
    };
    let relay_url = std::env::var("MDX_MOBILE_RELAY_PUBLIC_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:9010".to_string());
    if !relay_url.starts_with("wss://") && !local_websocket_url(&relay_url) {
        return Ok(refusal(
            "mobile_relay_tls_required",
            "non-loopback mobile relay URLs must use wss",
        ));
    }
    let now = now_epoch();
    let expires = now + RELAY_CREDENTIAL_LIFETIME_SECONDS;
    let claims = RelayClaims {
        sub: &identity.actor_id,
        tenant_id: &identity.tenant_id,
        actor_id: &identity.actor_id,
        device_id: &device_id,
        purpose: "mobile_forge_relay",
        allowed_stream_ids: &session_ids,
        iss: "mdx-control-plane",
        aud: "mdx-message-relay",
        iat: now as usize,
        exp: expires as usize,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|error| format!("mobile relay credential signing failed: {error}"))?;
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-relay-credential",
            "status": "MINTED",
            "relay_url": relay_url,
            "relay_token": token,
            "tenant_id": identity.tenant_id,
            "device_id": device_id,
            "host_id": host_id,
            "allowed_stream_ids": session_ids,
            "purpose": "mobile_forge_relay",
            "issued_at_epoch": now,
            "expires_at_epoch": expires,
            "ttl_seconds": RELAY_CREDENTIAL_LIFETIME_SECONDS,
            "persist_beyond_expiry": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn local_websocket_url(url: &str) -> bool {
    ["ws://127.0.0.1:", "ws://localhost:", "ws://[::1]:"]
        .iter()
        .any(|prefix| url.starts_with(prefix))
}

fn project_sessions(kernel: &MdxKernel, tenant_id: &str) -> Vec<Value> {
    let mut grouped: BTreeMap<String, Vec<&Receipt>> = BTreeMap::new();
    for receipt in kernel.ledger().entries().iter().filter(|receipt| {
        receipt.kind == "forge.run.event"
            && receipt.tenant_id.as_str() == tenant_id
            && !payload(receipt, "work_item_id").starts_with("mobile_cloud_")
    }) {
        let run_id = payload(receipt, "run_id");
        if run_id.starts_with("forge_run_") && valid_identifier(run_id) {
            grouped.entry(run_id.to_string()).or_default().push(receipt);
        }
    }
    let mut sessions = grouped
        .into_iter()
        .map(|(session_id, receipts)| project_session(&session_id, &receipts, tenant_id))
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| {
        mobile_session_priority(left)
            .cmp(&mobile_session_priority(right))
            .then_with(|| {
                right["updated_at"]
                    .as_str()
                    .cmp(&left["updated_at"].as_str())
            })
            .then_with(|| {
                left["session_id"]
                    .as_str()
                    .cmp(&right["session_id"].as_str())
            })
    });
    sessions.truncate(MAX_MOBILE_SESSIONS);
    sessions
}

fn mobile_session_priority(session: &Value) -> u8 {
    match session["state"].as_str().unwrap_or_default() {
        "needs_user" => 0,
        "review_ready" => 1,
        "running" => 2,
        "queued" => 3,
        "paused" => 4,
        "draft" => 5,
        "completed" => 6,
        "failed" => 7,
        "stopped" => 8,
        _ => 9,
    }
}

fn project_session(session_id: &str, receipts: &[&Receipt], tenant_id: &str) -> Value {
    let first = receipts[0];
    let last = receipts[receipts.len() - 1];
    let last_event = payload(last, "event");
    let last_detail = payload(last, "detail");
    let latest_terminal_receipt = receipts
        .iter()
        .rev()
        .find(|receipt| payload(receipt, "event") == "run_finished");
    let last_mobile_command = evidence_value(receipts, &["mobile_command_kind"]);
    let reviewable_change_recorded = receipts.iter().any(|receipt| {
        payload(receipt, "event") == "evidence_appended" && {
            let detail = payload(receipt, "detail");
            detail.starts_with("branch=") || detail.contains("diff_ready")
        }
    });
    let (state, needs_user) = if matches!(last_mobile_command, Some("pause" | "request_handoff")) {
        ("paused", false)
    } else if last_mobile_command == Some("stop") {
        ("stopped", false)
    } else if let Some(terminal_receipt) = latest_terminal_receipt {
        if payload(terminal_receipt, "terminal_status") == "RUN_FINISHED_DONE"
            && reviewable_change_recorded
        {
            ("review_ready", false)
        } else {
            mobile_terminal_state(payload(terminal_receipt, "terminal_status"))
        }
    } else if last_event == "evidence_appended"
        && (last_detail.contains("diff") || last_detail.contains("branch="))
    {
        ("review_ready", false)
    } else {
        ("running", false)
    };
    let (_, stage) = crate::forge_run_stream::event_kind_and_stage(last_event, last_detail);
    let mobile_stage = if last_mobile_command == Some("request_handoff") {
        "handoff"
    } else {
        match stage {
            "reading" => "orientation",
            "planning" | "writing" => "build",
            "proof" => "check",
            "done" if state == "review_ready" => "review",
            "done" => "done",
            _ => "build",
        }
    };
    let work_item = receipts
        .iter()
        .map(|receipt| payload(receipt, "work_item_id"))
        .find(|value| !value.is_empty())
        .unwrap_or("forge_work");
    let owner_user_id = match payload(first, "owner_user_id") {
        "" => first.actor_id.as_str(),
        owner => owner,
    };
    let repository_id = evidence_value(
        receipts,
        &[
            "handoff_repository_id",
            "repo_id",
            "repository_id",
            "repo_root",
        ],
    )
    .unwrap_or("MDx workspace");
    let goal = evidence_value(receipts, &["operator_intent", "operator_ask"]).unwrap_or(work_item);
    let source_revision = evidence_value(receipts, &["head_commit", "source_revision"])
        .filter(|value| value.len() >= 7)
        .unwrap_or("working-copy");
    let target_kind = canonical_target_kind(
        evidence_value(receipts, &["execution_target_kind", "mobile_target_kind"])
            .unwrap_or("paired_host"),
    )
    .to_string();
    let target_id = evidence_value(receipts, &["execution_target_id", "mobile_target_id"])
        .unwrap_or("local_host")
        .to_string();
    let target_name = if target_kind == "mdx_cloud" {
        "MDx Cloud"
    } else {
        "This Mac"
    };
    let target_history = project_target_history(receipts, tenant_id);
    let durable_cursor = receipt_sequence(&last.receipt_id).unwrap_or(receipts.len());
    json!({
        "schema_version": 1,
        "session_id": session_id,
        "session_version": durable_cursor,
        "tenant_id": tenant_id,
        "owner_user_id": owner_user_id,
        "repository_id": repository_id,
        "source_revision": source_revision,
        "goal": mdx_mobile_relay::sanitize_summary(goal),
        "state": state,
        "stage": mobile_stage,
        "active_target": {
            "kind": target_kind,
            "target_id": target_id,
            "tenant_id": tenant_id,
            "display_name": target_name,
            "capability_revision": 1
        },
        "target_history": if target_history.is_empty() { vec![json!({
            "kind": target_kind,
            "target_id": target_id,
            "tenant_id": tenant_id,
            "display_name": target_name,
            "capability_revision": 1
        })] } else { target_history },
        "last_event_sequence": durable_cursor,
        "replay_cursor": durable_cursor,
        "needs_user": needs_user,
        "latest_checkpoint_ref": evidence_value(receipts, &["checkpoint_id", "branch"]),
        "created_at": first.receipt_timestamp,
        "updated_at": last.receipt_timestamp,
        "execution_authority_open": false,
        "deployment_authority_open": false
    })
}

fn mobile_terminal_state(terminal_status: &str) -> (&'static str, bool) {
    (
        mdx_mobile_relay::terminal_event_kind(Some(terminal_status)),
        terminal_status == "RUN_FINISHED_CANNOT_PROCEED",
    )
}

fn project_target_history(receipts: &[&Receipt], tenant_id: &str) -> Vec<Value> {
    let mut history: Vec<Value> = Vec::new();
    let mut last_key = String::new();
    for receipt in receipts {
        let kind = ["execution_target_kind", "mobile_target_kind"]
            .iter()
            .find_map(|key| {
                let value = payload(receipt, key);
                (!value.is_empty()).then_some(value)
            });
        let id = ["execution_target_id", "mobile_target_id"]
            .iter()
            .find_map(|key| {
                let value = payload(receipt, key);
                (!value.is_empty()).then_some(value)
            });
        let (Some(kind), Some(id)) = (kind, id) else {
            continue;
        };
        let kind = canonical_target_kind(kind);
        let key = format!("{kind}:{id}");
        if key == last_key {
            continue;
        }
        last_key = key;
        let capability_revision = [
            "execution_target_capability_revision",
            "mobile_target_capability_revision",
        ]
        .iter()
        .find_map(|key| payload(receipt, key).parse::<u64>().ok())
        .unwrap_or(1);
        history.push(json!({
            "kind": kind,
            "target_id": id,
            "tenant_id": tenant_id,
            "display_name": if kind == "mdx_cloud" { "MDx Cloud" } else { "Paired Mac" },
            "capability_revision": capability_revision
        }));
    }
    history
}

fn canonical_target_kind(value: &str) -> &'static str {
    match value {
        "mdx_cloud" => "mdx_cloud",
        "customer_managed" => "customer_managed",
        _ => "paired_host",
    }
}

fn validate_projected_sessions(sessions: &[Value]) -> Result<(), String> {
    static VALIDATOR: OnceLock<Result<jsonschema::Validator, String>> = OnceLock::new();
    let validator = VALIDATOR.get_or_init(|| {
        let mut schema: Value = serde_json::from_str(include_str!(
            "../../../generated/response-schemas/forge-work-session.schema.json"
        ))
        .map_err(|error| format!("forge work session schema is invalid JSON: {error}"))?;
        let mut target_schema: Value = serde_json::from_str(include_str!(
            "../../../generated/response-schemas/execution-target.schema.json"
        ))
        .map_err(|error| format!("execution target schema is invalid JSON: {error}"))?;
        if let Some(target) = target_schema.as_object_mut() {
            target.remove("$schema");
            target.remove("$id");
        }
        schema["properties"]["active_target"] = target_schema.clone();
        schema["properties"]["target_history"]["items"] = target_schema;
        jsonschema::validator_for(&schema)
            .map_err(|error| format!("forge work session schema did not compile: {error}"))
    });
    let validator = validator.as_ref().map_err(Clone::clone)?;
    for session in sessions {
        if let Err(error) = validator.validate(session) {
            return Err(format!(
                "mobile session projection violated forge-work-session.schema.json: {error}"
            ));
        }
        let version = session["session_version"]
            .as_u64()
            .ok_or_else(|| "mobile session version must be an integer".to_string())?;
        if session["last_event_sequence"].as_u64() != Some(version)
            || session["replay_cursor"].as_u64() != Some(version)
        {
            return Err(
                "mobile session version, last event sequence, and replay cursor must match"
                    .to_string(),
            );
        }
        let tenant_id = session["tenant_id"]
            .as_str()
            .ok_or_else(|| "mobile session tenant must be a string".to_string())?;
        if session["active_target"]["tenant_id"].as_str() != Some(tenant_id)
            || !session["target_history"].as_array().is_some_and(|targets| {
                !targets.is_empty()
                    && targets
                        .iter()
                        .all(|target| target["tenant_id"].as_str() == Some(tenant_id))
            })
        {
            return Err("mobile session targets must match the session tenant".to_string());
        }
        for field in ["created_at", "updated_at"] {
            let timestamp = session[field]
                .as_str()
                .ok_or_else(|| format!("mobile session {field} must be a string"))?;
            OffsetDateTime::parse(timestamp, &Rfc3339)
                .map_err(|_| format!("mobile session {field} must be RFC3339"))?;
        }
    }
    Ok(())
}

fn evidence_value<'a>(receipts: &'a [&Receipt], keys: &[&str]) -> Option<&'a str> {
    for receipt in receipts.iter().rev() {
        for key in keys {
            let value = payload(receipt, key);
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

fn session_scope(parsed: &Value) -> Result<Vec<String>, RouteResponse> {
    let Some(values) = parsed.get("session_ids").and_then(Value::as_array) else {
        return Err(refusal(
            "mobile_relay_session_scope_required",
            "session_ids must be a non-empty array",
        ));
    };
    if values.is_empty() || values.len() > MAX_SESSION_SCOPE {
        return Err(refusal(
            "mobile_relay_session_scope_invalid",
            "between 1 and 32 sessions may be requested",
        ));
    }
    let mut output = Vec::with_capacity(values.len());
    for value in values {
        let Some(value) = value.as_str().map(str::trim) else {
            return Err(refusal(
                "mobile_relay_session_scope_invalid",
                "session ids must be strings",
            ));
        };
        if !value.starts_with("forge_run_") || !valid_identifier(value) {
            return Err(refusal(
                "mobile_relay_session_scope_invalid",
                "session ids must be valid Forge run ids",
            ));
        }
        if !output.iter().any(|existing| existing == value) {
            output.push(value.to_string());
        }
    }
    Ok(output)
}

fn identifier(parsed: &Value, key: &str) -> Result<String, RouteResponse> {
    let Some(value) = parsed.get(key).and_then(Value::as_str).map(str::trim) else {
        return Err(refusal(
            "mobile_relay_identifier_required",
            &format!("{key} is required"),
        ));
    };
    if !valid_identifier(value) {
        return Err(refusal(
            "mobile_relay_identifier_invalid",
            &format!("{key} contains unsupported characters"),
        ));
    }
    Ok(value.to_string())
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn payload<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn receipt_sequence(receipt_id: &str) -> Option<usize> {
    receipt_id
        .rsplit('_')
        .next()
        .filter(|suffix| !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|suffix| suffix.parse().ok())
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn refusal(code: &str, detail: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        json!({
            "status": "REFUSED",
            "error": code,
            "detail": detail,
            "production_write_allowed": false
        })
        .to_string(),
    )
}

fn method_not_allowed() -> RouteResponse {
    RouteResponse::text("405 Method Not Allowed", "method not allowed\n".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::ForgeRunEvent;

    #[test]
    fn projection_is_canonical_tenant_scoped_and_secret_safe() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:local_user",
                run_id: "forge_run_mobile_projection",
                event: "run_started",
                work_item_id: "mobile_relay_vertical",
                detail: "token=do-not-project",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("run start");
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "other_tenant",
                actor_id: "human:other",
                run_id: "forge_run_other_tenant",
                event: "run_started",
                work_item_id: "hidden",
                detail: "hidden",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("other tenant run");
        let sessions = project_sessions(&kernel, "local_tenant");
        let wire = serde_json::to_string(&sessions).expect("projection JSON");
        assert!(wire.contains("forge_run_mobile_projection"));
        assert!(wire.contains("mobile_relay_vertical"));
        assert!(!wire.contains("do-not-project"));
        assert!(!wire.contains("forge_run_other_tenant"));
        assert!(wire.contains("\"execution_authority_open\":false"));
        assert!(wire.contains("\"owner_user_id\":\"human:local_user\""));
        validate_projected_sessions(&sessions).expect("schema-valid sessions");
    }

    #[test]
    fn projection_is_recent_first_bounded_and_excludes_non_run_evidence() {
        let mut kernel = MdxKernel::boot_local();
        for index in 0..(MAX_MOBILE_SESSIONS + 3) {
            kernel
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:local_user",
                    run_id: &format!("forge_run_mobile_bound_{index:02}"),
                    event: "run_started",
                    work_item_id: "mobile_projection_bound",
                    detail: "accepted",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("run starts");
        }
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:dev-seed",
                run_id: "forge_dev_seed_mobile_bound",
                event: "run_started",
                work_item_id: "fixture evidence",
                detail: "not an operator run",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("fixture evidence records");

        let sessions = project_sessions(&kernel, "local_tenant");

        assert_eq!(sessions.len(), MAX_MOBILE_SESSIONS);
        assert_eq!(sessions[0]["session_id"], "forge_run_mobile_bound_34");
        assert!(sessions.iter().all(|session| {
            session["session_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("forge_run_"))
        }));
        assert!(!sessions.iter().any(|session| {
            session["session_id"].as_str() == Some("forge_dev_seed_mobile_bound")
        }));
        validate_projected_sessions(&sessions).expect("schema-valid sessions");
    }

    #[test]
    fn projection_preserves_mobile_pause_state_and_original_goal() {
        let mut kernel = MdxKernel::boot_local();
        let identity = mdx_core::GovernedWriteIdentity::local_demo("local_user");
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id: "forge_run_mobile_paused_projection",
                    event: "run_started",
                    work_item_id: "mobile_pause_vertical",
                    detail: "accepted run",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[("operator_intent", "Build the iPhone steering surface")],
            )
            .unwrap();
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "other_tenant",
                actor_id: "other_user",
                run_id: "forge_run_interleaved_version",
                event: "run_started",
                work_item_id: "mobile_version_interleave",
                detail: "interleaved",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .unwrap();
        let latest = kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id: "forge_run_mobile_paused_projection",
                    event: "agent_turn_prose",
                    work_item_id: "mobile_session_command",
                    detail: "Pausing at the next safe turn boundary",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[("mobile_command_kind", "pause")],
            )
            .unwrap();
        let sessions = project_sessions(&kernel, "local_tenant");
        let expected_version = receipt_sequence(&latest.receipt_id).unwrap();
        assert_eq!(sessions[0]["state"], "paused");
        assert_eq!(sessions[0]["goal"], "Build the iPhone steering surface");
        assert_eq!(sessions[0]["session_version"], expected_version);
        assert_eq!(sessions[0]["last_event_sequence"], expected_version);
        assert_eq!(sessions[0]["replay_cursor"], expected_version);
        assert_ne!(expected_version, 2);
        validate_projected_sessions(&sessions).expect("schema-valid sessions");
    }

    #[test]
    fn projection_uses_durable_owner_and_explicit_terminal_status() {
        let mut kernel = MdxKernel::boot_local();
        let identity = mdx_core::GovernedWriteIdentity {
            identity_source: "trusted_session".to_string(),
            actor_kind: "agent".to_string(),
            subject_actor_id: "human:session_owner".to_string(),
            delegation_id: "delegation_one".to_string(),
            authority_scope: vec!["forge:run".to_string()],
        };
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "agent:builder",
                    run_id: "forge_run_owned_failure",
                    event: "run_started",
                    work_item_id: "mobile_terminal_truth",
                    detail: "accepted",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[("operator_intent", "Prove the recorded owner")],
            )
            .expect("run starts");
        kernel
            .record_forge_run_event_with_identity(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "agent:builder",
                    run_id: "forge_run_owned_failure",
                    event: "run_finished",
                    work_item_id: "mobile_terminal_truth",
                    detail: "status=RUN_BUDGET_EXHAUSTED budget spent",
                    turn: 2,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &mdx_core::GovernedWriteIdentity::local_demo("agent:builder"),
            )
            .expect("run finishes");
        let sessions = project_sessions(&kernel, "local_tenant");
        assert_eq!(sessions[0]["owner_user_id"], "human:session_owner");
        assert_eq!(sessions[0]["state"], "failed");
        assert!(!sessions[0]["needs_user"].as_bool().unwrap_or(true));
        validate_projected_sessions(&sessions).expect("schema-valid sessions");
    }

    #[test]
    fn projection_preserves_terminal_state_after_follow_up_evidence() {
        let mut kernel = MdxKernel::boot_local();
        let identity = mdx_core::GovernedWriteIdentity::local_demo("local_user");
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id: "forge_run_terminal_then_evidence",
                    event: "run_started",
                    work_item_id: "mobile_terminal_projection",
                    detail: "accepted",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[("operator_intent", "Keep terminal state truthful")],
            )
            .expect("run starts");
        kernel
            .record_forge_run_event_with_identity(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id: "forge_run_terminal_then_evidence",
                    event: "run_finished",
                    work_item_id: "mobile_terminal_projection",
                    detail: "status=RUN_FINISHED_DONE branch ready",
                    turn: 2,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
            )
            .expect("run finishes");
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id: "forge_run_terminal_then_evidence",
                    event: "evidence_appended",
                    work_item_id: "forge_eval_from_run",
                    detail: "eval quality gate assessed after completion",
                    turn: 2,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[("evaluation_status", "recorded")],
            )
            .expect("follow-up evidence records");

        let sessions = project_sessions(&kernel, "local_tenant");

        assert_eq!(sessions[0]["state"], "completed");
        assert_eq!(sessions[0]["stage"], "done");
        validate_projected_sessions(&sessions).expect("schema-valid sessions");
    }

    #[test]
    fn successful_change_stops_at_the_mobile_review_boundary() {
        let mut kernel = MdxKernel::boot_local();
        let identity = mdx_core::GovernedWriteIdentity::local_demo("local_user");
        let run_id = "forge_run_mobile_review_boundary";
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id,
                    event: "run_started",
                    work_item_id: "mobile_review_boundary",
                    detail: "accepted",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[("operator_intent", "Prepare a reviewable mobile change")],
            )
            .expect("run starts");
        kernel
            .record_forge_run_event_with_identity(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id,
                    event: "evidence_appended",
                    work_item_id: "mobile_review_boundary",
                    detail: "branch=forge/mobile-review sha=1234567 paths=README.md",
                    turn: 2,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
            )
            .expect("diff records");
        kernel
            .record_forge_run_event_with_identity(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id,
                    event: "run_finished",
                    work_item_id: "mobile_review_boundary",
                    detail: "status=RUN_FINISHED_DONE turns=2 files_changed=1",
                    turn: 2,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
            )
            .expect("run finishes");

        let sessions = project_sessions(&kernel, "local_tenant");

        assert_eq!(sessions[0]["state"], "review_ready");
        assert_eq!(sessions[0]["stage"], "review");
        validate_projected_sessions(&sessions).expect("schema-valid sessions");
    }

    #[test]
    fn projection_follows_the_destination_repository_after_handoff() {
        let mut kernel = MdxKernel::boot_local();
        let identity = mdx_core::GovernedWriteIdentity::local_demo("local_user");
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id: "forge_run_handoff_repository",
                    event: "run_started",
                    work_item_id: "mobile_handoff_repository",
                    detail: "accepted",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[("repo_id", "source_repo")],
            )
            .expect("run starts");
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id: "forge_run_handoff_repository",
                    event: "evidence_appended",
                    work_item_id: "mobile_target_handoff",
                    detail: "Target handoff checkpoint admitted",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[
                    ("handoff_repository_id", "destination_repo"),
                    ("execution_target_kind", "mdx_cloud"),
                    ("execution_target_id", "cloud_destination"),
                ],
            )
            .expect("handoff records");

        let sessions = project_sessions(&kernel, "local_tenant");

        assert_eq!(sessions[0]["repository_id"], "destination_repo");
        assert_eq!(sessions[0]["active_target"]["kind"], "mdx_cloud");
        validate_projected_sessions(&sessions).expect("schema-valid sessions");
    }

    #[test]
    fn cannot_proceed_terminal_status_is_failed_and_needs_user() {
        assert_eq!(
            mobile_terminal_state("RUN_FINISHED_CANNOT_PROCEED"),
            ("failed", true)
        );
        assert_eq!(
            mobile_terminal_state("RUN_FINISHED_UNKNOWN"),
            ("failed", false)
        );
    }

    #[test]
    fn projection_schema_validation_fails_closed_on_contract_drift() {
        let mut session = json!({
            "schema_version": 1,
            "session_id": "forge_run_one",
            "session_version": 1,
            "tenant_id": "local_tenant",
            "owner_user_id": "local_user",
            "repository_id": "mdx-native",
            "source_revision": "working-copy",
            "goal": "Prove the mobile projection",
            "state": "running",
            "stage": "build",
            "active_target": {
                "kind": "paired_host",
                "target_id": "host_one",
                "tenant_id": "local_tenant",
                "display_name": "Paired Mac",
                "capability_revision": 1
            },
            "target_history": [{
                "kind": "paired_host",
                "target_id": "host_one",
                "tenant_id": "local_tenant",
                "display_name": "Paired Mac",
                "capability_revision": 1
            }],
            "last_event_sequence": 1,
            "replay_cursor": 1,
            "needs_user": false,
            "latest_checkpoint_ref": null,
            "created_at": "2026-07-13T12:00:00Z",
            "updated_at": "2026-07-13T12:00:00Z",
            "execution_authority_open": false,
            "deployment_authority_open": false
        });
        validate_projected_sessions(&[session.clone()]).expect("valid fixture");

        let mut missing_owner = session.clone();
        missing_owner
            .as_object_mut()
            .expect("session object")
            .remove("owner_user_id");
        let error =
            validate_projected_sessions(&[missing_owner]).expect_err("missing owner must fail");
        assert!(error.contains("owner_user_id"));

        session["replay_cursor"] = json!(2);
        let error = validate_projected_sessions(&[session.clone()])
            .expect_err("cursor disagreement must fail");
        assert!(error.contains("must match"));

        session["replay_cursor"] = json!(1);
        session["active_target"]["tenant_id"] = json!("other_tenant");
        let error = validate_projected_sessions(&[session.clone()])
            .expect_err("cross-tenant target must fail");
        assert!(error.contains("targets must match"));

        session["active_target"]["tenant_id"] = json!("local_tenant");
        session["updated_at"] = json!("not-a-timestamp");
        let error =
            validate_projected_sessions(&[session]).expect_err("invalid timestamp must fail");
        assert!(error.contains("updated_at must be RFC3339"));
    }

    #[test]
    fn unpaired_device_cannot_mint_relay_access() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = relay_credential(
            "POST",
            r#"{"device_id":"iphone_unpaired","host_id":"host_unpaired","session_ids":["forge_run_missing"]}"#,
            &kernel,
        )
        .expect("refusal response");
        assert!(response.body.contains("mobile_device_unknown"));
        assert!(!response.body.contains("relay_token"));
    }

    #[test]
    fn only_loopback_websocket_urls_can_skip_tls() {
        assert!(local_websocket_url("ws://127.0.0.1:9010"));
        assert!(local_websocket_url("ws://localhost:9010"));
        assert!(local_websocket_url("ws://[::1]:9010"));
        assert!(!local_websocket_url("ws://relay.example.com:9010"));
        assert!(!local_websocket_url("wss://relay.example.com"));
    }
}
