use crate::{RouteResponse, mobile_pairing, request_security};
use mdx_core::{ForgeRunControl, ForgeRunEvent, MdxKernel, MobilePushRegistration, Receipt};
use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex, RwLock};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const MAX_COMMAND_LIFETIME_SECONDS: i64 = 5 * 60;
const MAX_CLOCK_SKEW_SECONDS: i64 = 30;
const MAX_PUSH_REGISTRATIONS: usize = 16_384;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExecutionTargetInput {
    kind: String,
    target_id: String,
    tenant_id: String,
    display_name: String,
    capability_revision: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MobileSessionCommand {
    schema_version: u64,
    command_id: String,
    idempotency_key: String,
    session_id: String,
    expected_session_version: usize,
    tenant_id: String,
    actor_user_id: String,
    actor_device_id: String,
    target: ExecutionTargetInput,
    kind: String,
    payload: Value,
    issued_at: String,
    expires_at: String,
    user_presence: String,
    delegation_envelope_ref: String,
    grants_execution_authority: bool,
    device_signature: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MobilePushRegistrationRequest {
    schema_version: u64,
    device_id: String,
    host_id: String,
    push_token: String,
    kind: String,
    environment: String,
    issued_at: String,
    expires_at: String,
    device_signature: String,
}

static COMMAND_GATE: Mutex<()> = Mutex::new(());

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/mobile/session-commands.json" => Some(handle_command(method, body, kernel)),
        "/mobile/needs-you.json" => Some(handle_needs_you(method, kernel)),
        "/mobile/push-registrations.json" => Some(handle_push_registration(method, body, kernel)),
        "/mobile/notification-intents.json" => Some(handle_notification_intents(method, kernel)),
        _ => None,
    }
}

fn handle_command(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("POST") {
        return Ok(method_not_allowed());
    }
    let parsed: Value = match serde_json::from_str(body) {
        Ok(parsed) => parsed,
        Err(error) => {
            return Ok(refusal(
                "mobile_command_shape_invalid",
                &format!("command must be valid JSON: {error}"),
                None,
            ));
        }
    };
    let command: MobileSessionCommand = match serde_json::from_value(parsed.clone()) {
        Ok(command) => command,
        Err(error) => {
            return Ok(refusal(
                "mobile_command_shape_invalid",
                &format!("command does not match SessionCommand: {error}"),
                None,
            ));
        }
    };
    let resolved = request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    if let Err((code, detail)) = validate_command(&command, &resolved) {
        return Ok(refusal(code, &detail, Some(&command)));
    }
    let attestation_payload = command_attestation_payload(&parsed)?;
    if let Err(code) = mobile_pairing::verify_registered_device_signature(
        &resolved,
        &command.actor_device_id,
        &attestation_payload,
        &command.device_signature,
        kernel,
    ) {
        return Ok(refusal(
            &code,
            "the command must be signed by this account's active registered device key",
            Some(&command),
        ));
    }
    if command.target.kind == "paired_host"
        && let Err(code) = mobile_pairing::require_active_pairing(
            &resolved,
            &command.actor_device_id,
            &command.target.target_id,
            kernel,
        )
    {
        return Ok(refusal(
            &code,
            "an active pairing to the selected Mac is required",
            Some(&command),
        ));
    }
    if command.target.kind == "mdx_cloud" {
        let repository_id = command_repository_id(&command, kernel)?;
        if repository_id.is_empty() {
            return Ok(refusal(
                "mobile_command_cloud_repository_unproven",
                "the session must have a durable repository before it can claim an MDx Cloud target",
                Some(&command),
            ));
        }
        let guard = kernel.read().map_err(|_| {
            "kernel lock poisoned while authorizing mobile cloud target".to_string()
        })?;
        if let Err(reason) = crate::mobile_cloud_route::require_verified_environment(
            &guard,
            &command.tenant_id,
            &command.target.target_id,
            &repository_id,
        ) {
            return Ok(refusal(
                "mobile_command_cloud_target_unverified",
                &reason,
                Some(&command),
            ));
        }
    }
    let _command_guard = COMMAND_GATE
        .lock()
        .map_err(|_| "mobile command gate lock poisoned".to_string())?;
    let fingerprint = digest_hex(&attestation_payload);
    if let Some(response) = idempotent_response(kernel, &command, &fingerprint)? {
        return Ok(response);
    }
    let current_version = session_version(kernel, &command.tenant_id, &command.session_id)?;
    if command.kind == "start_build" {
        if current_version != 0 || command.expected_session_version != 1 {
            return Ok(refusal(
                "mobile_command_stale_session",
                "a start command must name a new session at expected version 1",
                Some(&command),
            ));
        }
    } else if current_version == 0 || current_version != command.expected_session_version {
        return Ok(refusal(
            "mobile_command_stale_session",
            &format!(
                "expected session version {}, current version is {current_version}",
                command.expected_session_version
            ),
            Some(&command),
        ));
    }
    if command.kind != "start_build"
        && let Some((target_kind, target_id)) =
            session_execution_target(kernel, &command.tenant_id, &command.session_id)?
        && (target_kind != command.target.kind || target_id != command.target.target_id)
    {
        return Ok(refusal(
            "mobile_command_target_mismatch",
            "command target must match the session's current execution target",
            Some(&command),
        ));
    }

    let delegated = match command.kind.as_str() {
        "start_build" => delegate_start(&command, kernel)?,
        "follow_up" => delegate_control(
            &command,
            &resolved,
            "steer",
            payload_text(&command.payload, "instruction"),
            kernel,
        )?,
        "answer_question" => delegate_control(
            &command,
            &resolved,
            "steer",
            payload_text(&command.payload, "answer"),
            kernel,
        )?,
        "stop" => delegate_control(
            &command,
            &resolved,
            "stop",
            payload_text(&command.payload, "reason"),
            kernel,
        )?,
        "pause" => {
            let reason = payload_text(&command.payload, "reason");
            delegate_control(
                &command,
                &resolved,
                "stop",
                &format!(
                    "[mobile_pause] {}",
                    if reason.is_empty() {
                        "pause at the next safe turn boundary"
                    } else {
                        &reason
                    }
                ),
                kernel,
            )?
        }
        "resume" => delegate_resume(&command, kernel)?,
        "record_decision" => delegate_decision(&command, kernel)?,
        "request_handoff" => delegate_handoff(&command, &resolved, kernel)?,
        "request_changes" => delegate_request_changes(&command, kernel)?,
        "approve_and_open_pr" => delegate_approve_and_open_pr(&command, kernel)?,
        _ => {
            return Ok(refusal(
                "mobile_command_kind_invalid",
                "unsupported mobile session command",
                Some(&command),
            ));
        }
    };
    if delegated.refused {
        return Ok(refusal(
            "mobile_command_delegated_route_refused",
            &delegated.detail,
            Some(&command),
        ));
    }
    let audit = record_command_audit(kernel, &resolved, &command, &fingerprint, &delegated)?;
    let session_version = session_version(kernel, &command.tenant_id, &command.session_id)?;
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-session-command",
            "status": "ACCEPTED",
            "command_id": command.command_id,
            "idempotency_key": command.idempotency_key,
            "idempotent_replay": false,
            "session_id": command.session_id,
            "session_version": session_version,
            "kind": command.kind,
            "target": command.target,
            "control_receipt_id": delegated.receipt_id,
            "command_receipt_id": audit.receipt_id,
            "policy_decision_id": audit.policy_decision_id,
            "action_url": delegated.action_url,
            "checkpoint": delegated.checkpoint,
            "resulting_target": delegated.resulting_target,
            "safe_summary": safe_command_summary(&command.kind),
            "detail": delegated.detail,
            "user_presence": command.user_presence,
            "user_presence_verification": "client_asserted_device_key_bound",
            "device_signature_verification": "verified_registered_device_key",
            "stale_state_accepted": false,
            "grants_execution_authority": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

struct DelegatedResult {
    receipt_id: String,
    detail: String,
    refused: bool,
    action_url: Option<String>,
    checkpoint: Option<Value>,
    resulting_target: Option<ExecutionTargetInput>,
}

fn delegate_start(
    command: &MobileSessionCommand,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<DelegatedResult, String> {
    let goal = payload_text(&command.payload, "goal");
    let repository_id = payload_text(&command.payload, "repository_id");
    if goal.is_empty() || repository_id.is_empty() {
        return Ok(DelegatedResult {
            receipt_id: String::new(),
            detail: "start_build requires goal and repository_id".to_string(),
            refused: true,
            action_url: None,
            checkpoint: None,
            resulting_target: None,
        });
    }
    let allowed_commands = command
        .payload
        .get("allowed_commands")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut body = json!({
        "intent": goal,
        "repo_id": repository_id,
        "requested_run_id": command.session_id,
        "allowed_commands": allowed_commands,
        "tenant_id": command.tenant_id,
        "actor_id": command.actor_user_id,
        "fleet_width": command.payload.get("fleet_width").and_then(Value::as_u64).unwrap_or(1),
        "max_cost_cents": command.payload.get("max_cost_cents").and_then(Value::as_u64).unwrap_or(0),
        "max_runtime_ms": command.payload.get("max_runtime_ms").and_then(Value::as_u64).unwrap_or(0)
    });
    if command.target.kind == "mdx_cloud" {
        body["execution_backend"] = json!("hosted_sandbox");
        body["cloud_environment_id"] = json!(command.target.target_id);
    }
    if command.delegation_envelope_ref != "session_default" {
        body["envelope_id"] = json!(command.delegation_envelope_ref);
    }
    let body = body.to_string();
    let response =
        crate::forge_run_route::route_response("POST", "/forge/runs.json", &body, kernel)
            .ok_or_else(|| "Forge start route unavailable".to_string())??;
    let value: Value = serde_json::from_str(&response.body).unwrap_or_default();
    Ok(DelegatedResult {
        receipt_id: value["run_started_receipt_id"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        detail: value["reason"]
            .as_str()
            .unwrap_or("Forge accepted the build")
            .to_string(),
        refused: value["status"].as_str() != Some("RUN_STARTED"),
        action_url: None,
        checkpoint: None,
        resulting_target: None,
    })
}

fn delegate_control(
    command: &MobileSessionCommand,
    resolved: &request_security::ResolvedWriteIdentity,
    control: &str,
    note: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<DelegatedResult, String> {
    if control == "steer" && note.trim().is_empty() {
        return Ok(DelegatedResult {
            receipt_id: String::new(),
            detail: "direction must not be empty".to_string(),
            refused: true,
            action_url: None,
            checkpoint: None,
            resulting_target: None,
        });
    }
    if note.chars().count() > 1_500 {
        return Ok(DelegatedResult {
            receipt_id: String::new(),
            detail: "mobile direction exceeds 1500 characters".to_string(),
            refused: true,
            action_url: None,
            checkpoint: None,
            resulting_target: None,
        });
    }
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned while recording mobile control".to_string())?;
    let report = match kernel.record_forge_run_control_with_identity(
        ForgeRunControl {
            tenant_id: &command.tenant_id,
            actor_id: &command.actor_user_id,
            run_id: &command.session_id,
            control,
            note,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(DelegatedResult {
                receipt_id: String::new(),
                detail: error.message(),
                refused: true,
                action_url: None,
                checkpoint: None,
                resulting_target: None,
            });
        }
    };
    drop(kernel);
    if report.control == "stop" {
        crate::process_supervisor::cancel_run(&report.run_id);
    }
    Ok(DelegatedResult {
        receipt_id: report.receipt_id,
        detail: safe_command_summary(&command.kind).to_string(),
        refused: false,
        action_url: None,
        checkpoint: None,
        resulting_target: None,
    })
}

fn delegate_resume(
    command: &MobileSessionCommand,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<DelegatedResult, String> {
    let (repository_id, goal, allowed_commands, envelope_id) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned while preparing resume".to_string())?;
        let last_mobile_command = kernel.ledger().entries().iter().rev().find_map(|receipt| {
            (receipt.kind == "forge.run.event"
                && receipt.tenant_id.as_str() == command.tenant_id
                && payload(receipt, "run_id") == command.session_id)
                .then(|| payload(receipt, "mobile_command_kind"))
                .filter(|kind| !kind.is_empty())
        });
        if !matches!(last_mobile_command, Some("pause" | "request_handoff")) {
            return Ok(DelegatedResult {
                receipt_id: String::new(),
                detail: "only a session paused or handed off from mobile can be resumed here"
                    .to_string(),
                refused: true,
                action_url: None,
                checkpoint: None,
                resulting_target: None,
            });
        }
        let started = kernel.ledger().entries().iter().find(|receipt| {
            receipt.kind == "forge.run.event"
                && receipt.tenant_id.as_str() == command.tenant_id
                && payload(receipt, "run_id") == command.session_id
                && payload(receipt, "event") == "run_started"
        });
        let Some(started) = started else {
            return Ok(DelegatedResult {
                receipt_id: String::new(),
                detail: "session has no durable run start".to_string(),
                refused: true,
                action_url: None,
                checkpoint: None,
                resulting_target: None,
            });
        };
        let handoff_repo_id = kernel.ledger().entries().iter().rev().find_map(|receipt| {
            (receipt.kind == "forge.run.event"
                && receipt.tenant_id.as_str() == command.tenant_id
                && payload(receipt, "run_id") == command.session_id)
                .then(|| payload(receipt, "handoff_repository_id"))
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        });
        (
            handoff_repo_id.unwrap_or_else(|| payload(started, "repo_id").to_string()),
            payload(started, "operator_intent").to_string(),
            payload(started, "selected_checks")
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>(),
            payload(started, "autonomy_envelope_id").to_string(),
        )
    };
    let mut body = json!({
        "intent": goal,
        "repo_id": repository_id,
        "requested_run_id": command.session_id,
        "allowed_commands": allowed_commands,
        "tenant_id": command.tenant_id,
        "actor_id": command.actor_user_id,
        "resume": true
    });
    if !envelope_id.trim().is_empty() {
        body["envelope_id"] = json!(envelope_id);
    }
    if command.target.kind == "mdx_cloud" {
        body["execution_backend"] = json!("hosted_sandbox");
        body["cloud_environment_id"] = json!(command.target.target_id);
    }
    let body = body.to_string();
    let response =
        crate::forge_run_route::route_response("POST", "/forge/runs.json", &body, kernel)
            .ok_or_else(|| "Forge resume route unavailable".to_string())??;
    let value: Value = serde_json::from_str(&response.body).unwrap_or_default();
    Ok(DelegatedResult {
        receipt_id: value["run_started_receipt_id"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        detail: value["reason"]
            .as_str()
            .unwrap_or("Forge resumed the build")
            .to_string(),
        refused: value["status"].as_str() != Some("RUN_STARTED"),
        action_url: None,
        checkpoint: None,
        resulting_target: None,
    })
}

fn delegate_decision(
    command: &MobileSessionCommand,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<DelegatedResult, String> {
    if command.user_presence != "biometric_verified" {
        return Ok(DelegatedResult {
            receipt_id: String::new(),
            detail: "this decision requires biometric user presence".to_string(),
            refused: true,
            action_url: None,
            checkpoint: None,
            resulting_target: None,
        });
    }
    let decision_kind = payload_text(&command.payload, "decision_kind");
    if decision_kind != "ratify_fleet_plan" {
        return Ok(DelegatedResult {
            receipt_id: String::new(),
            detail: "only the bounded fleet plan ratification decision is supported".to_string(),
            refused: true,
            action_url: None,
            checkpoint: None,
            resulting_target: None,
        });
    }
    let fleet_id = payload_text(&command.payload, "fleet_id");
    let expected_receipt = payload_text(&command.payload, "decision_subject_receipt_id");
    let reason = payload_text(&command.payload, "reason");
    let current_receipt = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned while checking decision freshness".to_string())?;
        kernel
            .ledger()
            .query()
            .by_kind("fleet.plan.drafted")
            .iter()
            .rev()
            .find(|receipt| {
                receipt.tenant_id.as_str() == command.tenant_id
                    && payload(receipt, "fleet_id") == fleet_id
            })
            .map(|receipt| receipt.receipt_id.clone())
    };
    if current_receipt.as_deref() != Some(expected_receipt) {
        return Ok(DelegatedResult {
            receipt_id: String::new(),
            detail: "fleet plan changed after this decision card was loaded".to_string(),
            refused: true,
            action_url: None,
            checkpoint: None,
            resulting_target: None,
        });
    }
    let body = json!({ "fleet_id": fleet_id, "reason": reason }).to_string();
    let response = crate::fleet_plan_route::route_response(
        "POST",
        "/forge/fleet-plan-ratifications.json",
        &body,
        kernel,
    )
    .ok_or_else(|| "fleet plan ratification route unavailable".to_string())??;
    let value: Value = serde_json::from_str(&response.body).unwrap_or_default();
    Ok(DelegatedResult {
        receipt_id: value["ratification_receipt_id"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        detail: value["reason"]
            .as_str()
            .unwrap_or("Fleet plan ratified without starting workers")
            .to_string(),
        refused: value["status"].as_str() != Some("RATIFIED"),
        action_url: None,
        checkpoint: None,
        resulting_target: None,
    })
}

fn delegate_handoff(
    command: &MobileSessionCommand,
    resolved: &request_security::ResolvedWriteIdentity,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<DelegatedResult, String> {
    if command.user_presence != "biometric_verified" {
        return Ok(delegated_refusal(
            "Moving a build between execution targets requires biometric user presence",
        ));
    }
    let destination: ExecutionTargetInput = match command
        .payload
        .get("destination_target")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
    {
        Some(target) => target,
        None => return Ok(delegated_refusal("Handoff requires a destination target")),
    };
    if destination.tenant_id != command.tenant_id
        || destination.kind == command.target.kind
            && destination.target_id == command.target.target_id
        || !matches!(destination.kind.as_str(), "paired_host" | "mdx_cloud")
        || !valid_identifier(&destination.target_id)
        || destination.capability_revision == 0
    {
        return Ok(delegated_refusal(
            "Handoff destination must be a different paired Mac or verified MDx Cloud target in this tenant",
        ));
    }
    if destination.kind == "paired_host"
        && let Err(code) = mobile_pairing::require_active_pairing(
            resolved,
            &command.actor_device_id,
            &destination.target_id,
            kernel,
        )
    {
        return Ok(delegated_refusal(&format!(
            "Destination Mac pairing is not active: {code}"
        )));
    }
    let destination_repo_id = payload_text(&command.payload, "destination_repository_id");
    let facts = match handoff_facts(kernel, command, destination_repo_id)? {
        Ok(facts) => facts,
        Err(reason) => return Ok(delegated_refusal(&reason)),
    };
    if destination.kind == "mdx_cloud" {
        let guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned while admitting cloud handoff".to_string())?;
        if let Err(reason) = crate::mobile_cloud_route::require_verified_environment(
            &guard,
            &command.tenant_id,
            &destination.target_id,
            &facts.destination_repo_id,
        ) {
            return Ok(delegated_refusal(&reason));
        }
    }
    if let Err(reason) = transfer_checkpoint_branch(&facts) {
        return Ok(delegated_refusal(&reason));
    }
    let checkpoint_id = format!(
        "handoff_{}",
        &digest_hex(
            format!(
                "{}|{}|{}|{}|{}",
                command.session_id,
                command.expected_session_version,
                facts.commit_sha,
                destination.kind,
                destination.target_id
            )
            .as_bytes()
        )[..32]
    );
    let created_at = now_rfc3339();
    let expires_at = (OffsetDateTime::now_utc() + time::Duration::hours(24))
        .format(&Rfc3339)
        .unwrap_or_else(|_| created_at.clone());
    let evidence_refs = facts.evidence_refs.clone();
    let workspace_snapshot_ref = format!(
        "git-checkpoint:{}:{}@{}",
        facts.destination_repo_id, facts.branch, facts.commit_sha
    );
    let event_cursor = facts.event_cursor.to_string();
    let capability_revision = destination.capability_revision.to_string();
    let report = kernel
        .write()
        .map_err(|_| "kernel lock poisoned while recording handoff".to_string())?
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: &command.tenant_id,
                actor_id: &command.actor_user_id,
                run_id: &command.session_id,
                event: "evidence_appended",
                work_item_id: "mobile_target_handoff",
                detail: "Target handoff checkpoint admitted",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &resolved.identity,
            &[
                ("checkpoint_id", checkpoint_id.as_str()),
                ("handoff_status", "ADMITTED"),
                ("handoff_source_target_kind", command.target.kind.as_str()),
                (
                    "handoff_source_target_id",
                    command.target.target_id.as_str(),
                ),
                ("execution_target_kind", destination.kind.as_str()),
                ("execution_target_id", destination.target_id.as_str()),
                (
                    "execution_target_capability_revision",
                    capability_revision.as_str(),
                ),
                ("handoff_repository_id", facts.destination_repo_id.as_str()),
                ("source_revision", facts.commit_sha.as_str()),
                ("branch", facts.branch.as_str()),
                ("workspace_snapshot_ref", workspace_snapshot_ref.as_str()),
                ("handoff_event_cursor", event_cursor.as_str()),
                ("handoff_contains_secret_values", "false"),
                ("handoff_grants_execution_authority", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    let checkpoint = json!({
        "checkpoint_id": checkpoint_id,
        "session_id": command.session_id,
        "session_version": command.expected_session_version,
        "source_target": command.target,
        "destination_target": destination,
        "source_revision": facts.commit_sha,
        "branch": facts.branch,
        "workspace_snapshot_ref": workspace_snapshot_ref,
        "event_cursor": facts.event_cursor,
        "evidence_refs": evidence_refs,
        "created_by_device_id": command.actor_device_id,
        "created_at": created_at,
        "expires_at": expires_at,
        "contains_secret_values": false,
        "grants_execution_authority": false
    });
    Ok(DelegatedResult {
        receipt_id: report.receipt_id,
        detail: format!(
            "Checkpoint is ready on {}. Resume when you want Forge to continue there.",
            destination.display_name
        ),
        refused: false,
        action_url: None,
        checkpoint: Some(checkpoint),
        resulting_target: Some(destination),
    })
}

fn delegate_request_changes(
    command: &MobileSessionCommand,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<DelegatedResult, String> {
    let comment = payload_text(&command.payload, "comment");
    if comment.is_empty() || comment.chars().count() > 2_000 {
        return Ok(delegated_refusal(
            "Request changes needs a focused comment of at most 2000 characters",
        ));
    }
    let body = json!({
        "run_id": command.session_id,
        "comment": comment,
        "enforce_repair_budget": true,
        "execution_backend": if command.target.kind == "mdx_cloud" { "hosted_sandbox" } else { "local" },
        "cloud_environment_id": if command.target.kind == "mdx_cloud" { command.target.target_id.as_str() } else { "" },
        "tenant_id": command.tenant_id,
        "actor_id": command.actor_user_id
    })
    .to_string();
    let response = crate::forge_revise_route::route_response(
        "POST",
        "/forge/run-revisions.json",
        &body,
        kernel,
    )
    .ok_or_else(|| "Forge revision route unavailable".to_string())??;
    let value: Value = serde_json::from_str(&response.body).unwrap_or_default();
    let accepted = value["status"].as_str() == Some("RUN_STARTED")
        || value["status"].as_str() == Some("REVISION_STARTED");
    Ok(DelegatedResult {
        receipt_id: value["run_started_receipt_id"]
            .as_str()
            .or_else(|| value["revision_receipt_id"].as_str())
            .unwrap_or("")
            .to_string(),
        detail: value["reason"]
            .as_str()
            .unwrap_or(if accepted {
                "Forge started a governed revision from your review comment"
            } else {
                "Forge could not start the requested revision"
            })
            .to_string(),
        refused: !accepted,
        action_url: None,
        checkpoint: None,
        resulting_target: None,
    })
}

fn delegate_approve_and_open_pr(
    command: &MobileSessionCommand,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<DelegatedResult, String> {
    if command.user_presence != "biometric_verified" {
        return Ok(delegated_refusal(
            "Approving a source-host handoff requires biometric user presence",
        ));
    }
    let commit_sha = payload_text(&command.payload, "commit_sha");
    let reason = payload_text(&command.payload, "reason");
    let target_host = payload_text(&command.payload, "target_host");
    let base_branch = payload_text(&command.payload, "base_branch");
    if commit_sha.len() < 7 || reason.trim().len() < 8 {
        return Ok(delegated_refusal(
            "Approval must name the reviewed commit and include the reviewer's reason",
        ));
    }
    if target_host != "github" {
        return Ok(delegated_refusal(
            "Opening this pull request requires the recorded GitHub source host",
        ));
    }
    let (recorded_branch, branch_source) = {
        let guard = kernel.read().map_err(|_| {
            "kernel lock poisoned while resolving the review repository".to_string()
        })?;
        let repository_id = session_repository_id(&guard, command);
        if repository_id.is_empty() {
            return Ok(delegated_refusal(
                "The reviewed session does not name a connected repository",
            ));
        }
        match crate::forge_repo_route::recorded_default_branch(&guard, &repository_id) {
            Some(recorded) => recorded,
            None => {
                return Ok(delegated_refusal(
                    "Reconnect or re-index the repository so its default branch is recorded",
                ));
            }
        }
    };
    if branch_source == "current_head_fallback" {
        return Ok(delegated_refusal(
            "Reconnect the repository with an explicit or source-host default branch before opening a pull request",
        ));
    }
    if base_branch != recorded_branch {
        return Ok(delegated_refusal(
            "The requested base branch does not match the connected repository's recorded default branch",
        ));
    }
    let decision = crate::forge_run_ship_route::route_response(
        "POST",
        "/forge/run-ship-decisions.json",
        &json!({
            "run_id": command.session_id,
            "commit_sha": commit_sha,
            "reason": reason,
            "tenant_id": command.tenant_id,
            "actor_id": command.actor_user_id
        })
        .to_string(),
        kernel,
    )
    .ok_or_else(|| "Forge ship decision route unavailable".to_string())??;
    let decision_value: Value = serde_json::from_str(&decision.body).unwrap_or_default();
    if decision_value["status"].as_str() != Some("RECORDED") {
        return Ok(delegated_refusal(
            decision_value["reason"]
                .as_str()
                .unwrap_or("The existing ship gate refused this approval"),
        ));
    }
    let delivery = crate::forge_source_host_live_delivery_route::route_response(
        "POST",
        "/forge/source-host-live-deliveries.json",
        &json!({
            "run_id": command.session_id,
            "target_host": target_host,
            "base_branch": recorded_branch,
            "confirm_live_delivery": true,
            "tenant_id": command.tenant_id,
            "actor_id": command.actor_user_id
        })
        .to_string(),
        kernel,
    )
    .ok_or_else(|| "Forge source-host delivery route unavailable".to_string())??;
    let delivery_value: Value = serde_json::from_str(&delivery.body).unwrap_or_default();
    let action_url = opened_github_pull_request_url(&delivery_value);
    let opened = action_url.is_some();
    Ok(DelegatedResult {
        receipt_id: decision_value["ship_receipt_id"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        detail: if opened {
            "Reviewed commit ratified and pull request opened. Merge remains governed by source-host checks and policy."
                .to_string()
        } else {
            format!(
                "The reviewed commit is ratified, but source-host delivery remains blocked: {}",
                delivery_value["reason"]
                    .as_str()
                    .unwrap_or("delivery gate not satisfied")
            )
        },
        refused: false,
        action_url,
        checkpoint: None,
        resulting_target: None,
    })
}

fn opened_github_pull_request_url(delivery: &Value) -> Option<String> {
    if delivery["status"].as_str() != Some("LIVE_PR_OPENED") {
        return None;
    }
    delivery["pull_request_url"]
        .as_str()
        .map(str::trim)
        .filter(|url| url.starts_with("https://github.com/") && url.contains("/pull/"))
        .map(str::to_string)
}

fn session_repository_id(kernel: &MdxKernel, command: &MobileSessionCommand) -> String {
    kernel
        .ledger()
        .entries()
        .iter()
        .rev()
        .filter(|receipt| {
            receipt.kind == "forge.run.event"
                && receipt.tenant_id.as_str() == command.tenant_id
                && payload(receipt, "run_id") == command.session_id
        })
        .find_map(|receipt| {
            ["handoff_repository_id", "repo_id"].iter().find_map(|key| {
                let value = payload(receipt, key);
                (!value.is_empty()).then(|| value.to_string())
            })
        })
        .unwrap_or_default()
}

struct HandoffFacts {
    destination_repo_id: String,
    source_root: String,
    destination_root: String,
    branch: String,
    commit_sha: String,
    event_cursor: usize,
    evidence_refs: Vec<String>,
}

fn handoff_facts(
    kernel: &Arc<RwLock<MdxKernel>>,
    command: &MobileSessionCommand,
    requested_destination_repo_id: &str,
) -> Result<Result<HandoffFacts, String>, String> {
    let guard = kernel
        .read()
        .map_err(|_| "kernel lock poisoned while preparing handoff".to_string())?;
    let receipts = guard
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            receipt.kind == "forge.run.event"
                && receipt.tenant_id.as_str() == command.tenant_id
                && payload(receipt, "run_id") == command.session_id
        })
        .collect::<Vec<_>>();
    let source_repo_id = receipts
        .iter()
        .rev()
        .find_map(|receipt| {
            ["handoff_repository_id", "repo_id"].iter().find_map(|key| {
                let value = payload(receipt, key);
                (!value.is_empty()).then(|| value.to_string())
            })
        })
        .unwrap_or_default();
    let destination_repo_id = if requested_destination_repo_id.is_empty() {
        source_repo_id.clone()
    } else {
        requested_destination_repo_id.to_string()
    };
    let branch =
        crate::forge_diff_route::branch_for_run(&guard, &command.session_id).unwrap_or_default();
    let source_root = guard
        .forge_repo_root(&source_repo_id)
        .or_else(|| crate::forge_diff_route::repo_root_for_run(&guard, &command.session_id))
        .unwrap_or_default();
    let destination_root = guard
        .forge_repo_root(&destination_repo_id)
        .unwrap_or_default();
    if source_repo_id.is_empty()
        || destination_repo_id.is_empty()
        || branch.is_empty()
        || source_root.is_empty()
        || destination_root.is_empty()
    {
        return Ok(Err(
            "Handoff needs a committed Forge branch and connected source and destination repositories"
                .to_string(),
        ));
    }
    if !safe_git_branch(&branch) {
        return Ok(Err("Handoff branch is not a safe Forge branch".to_string()));
    }
    let commit_sha = git_revision(&source_root, &format!("refs/heads/{branch}"));
    if commit_sha.len() != 40 {
        return Ok(Err(
            "Handoff could not verify the exact source branch commit".to_string(),
        ));
    }
    let evidence_refs = receipts
        .iter()
        .rev()
        .take(50)
        .map(|receipt| receipt.receipt_id.clone())
        .collect::<Vec<_>>();
    Ok(Ok(HandoffFacts {
        destination_repo_id,
        source_root,
        destination_root,
        branch,
        commit_sha,
        event_cursor: receipts.len(),
        evidence_refs,
    }))
}

fn transfer_checkpoint_branch(facts: &HandoffFacts) -> Result<(), String> {
    if facts.source_root != facts.destination_root {
        let source_origin = git_origin(&facts.source_root);
        let destination_origin = git_origin(&facts.destination_root);
        if source_origin.is_empty()
            || destination_origin.is_empty()
            || normalize_git_origin(&source_origin) != normalize_git_origin(&destination_origin)
        {
            return Err(
                "Source and destination repositories do not share the same source-host identity"
                    .to_string(),
            );
        }
        let refspec = format!("+refs/heads/{}:refs/heads/{}", facts.branch, facts.branch);
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(&facts.destination_root)
            .args(["fetch", "--no-tags", "--force"])
            .arg(&facts.source_root)
            .arg(&refspec)
            .output()
            .map_err(|error| format!("checkpoint transfer could not start: {error}"))?;
        if !output.status.success() {
            return Err(
                "Checkpoint branch could not be copied to the destination repository".to_string(),
            );
        }
    }
    let destination_revision = git_revision(
        &facts.destination_root,
        &format!("refs/heads/{}", facts.branch),
    );
    if destination_revision != facts.commit_sha {
        return Err("Destination branch does not match the checkpoint commit".to_string());
    }
    Ok(())
}

fn git_origin(root: &str) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["config", "--get", "remote.origin.url"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}

fn normalize_git_origin(origin: &str) -> String {
    let value = origin
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_ascii_lowercase();
    if let Some((_, remainder)) = value.split_once("://") {
        return remainder
            .rsplit_once('@')
            .map(|(_, host_path)| host_path)
            .unwrap_or(remainder)
            .trim_end_matches('/')
            .to_string();
    }
    let without_user = value
        .rsplit_once('@')
        .map(|(_, host_path)| host_path)
        .unwrap_or(&value);
    if let Some((host, path)) = without_user.split_once(':') {
        return format!("{host}/{path}").trim_end_matches('/').to_string();
    }
    without_user.trim_end_matches('/').to_string()
}

fn git_revision(root: &str, reference: &str) -> String {
    std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--verify", reference])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .unwrap_or_default()
}

fn safe_git_branch(value: &str) -> bool {
    value.starts_with("forge/") || value.starts_with("fleet/")
}

fn delegated_refusal(detail: &str) -> DelegatedResult {
    DelegatedResult {
        receipt_id: String::new(),
        detail: detail.to_string(),
        refused: true,
        action_url: None,
        checkpoint: None,
        resulting_target: None,
    }
}

fn record_command_audit(
    kernel: &Arc<RwLock<MdxKernel>>,
    resolved: &request_security::ResolvedWriteIdentity,
    command: &MobileSessionCommand,
    fingerprint: &str,
    delegated: &DelegatedResult,
) -> Result<mdx_core::ForgeRunEventReport, String> {
    let expected_version = command.expected_session_version.to_string();
    let audited_target = delegated
        .resulting_target
        .as_ref()
        .unwrap_or(&command.target);
    let capability_revision = audited_target.capability_revision.to_string();
    let evidence = [
        ("mobile_command_id", command.command_id.as_str()),
        ("mobile_idempotency_key", command.idempotency_key.as_str()),
        ("mobile_command_fingerprint", fingerprint),
        ("mobile_command_kind", command.kind.as_str()),
        ("mobile_actor_device_id", command.actor_device_id.as_str()),
        ("mobile_user_presence", command.user_presence.as_str()),
        (
            "mobile_user_presence_verification",
            "client_asserted_device_key_bound",
        ),
        (
            "mobile_device_signature_verification",
            "verified_registered_device_key",
        ),
        ("mobile_expected_session_version", expected_version.as_str()),
        (
            "mobile_delegation_envelope_ref",
            command.delegation_envelope_ref.as_str(),
        ),
        ("mobile_target_kind", audited_target.kind.as_str()),
        ("mobile_target_id", audited_target.target_id.as_str()),
        (
            "mobile_target_capability_revision",
            capability_revision.as_str(),
        ),
        ("mobile_control_receipt_id", delegated.receipt_id.as_str()),
        ("mobile_command_expires_at", command.expires_at.as_str()),
    ];
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned while auditing mobile command".to_string())?;
    kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: &command.tenant_id,
                actor_id: &command.actor_user_id,
                run_id: &command.session_id,
                event: "agent_turn_prose",
                work_item_id: "mobile_session_command",
                detail: safe_command_summary(&command.kind),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &resolved.identity,
            &evidence,
        )
        .map_err(|error| error.message())
}

fn idempotent_response(
    kernel: &Arc<RwLock<MdxKernel>>,
    command: &MobileSessionCommand,
    fingerprint: &str,
) -> Result<Option<RouteResponse>, String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned while checking mobile idempotency".to_string())?;
    let existing = kernel.ledger().entries().iter().find(|receipt| {
        receipt.kind == "forge.run.event"
            && receipt.tenant_id.as_str() == command.tenant_id
            && payload(receipt, "mobile_idempotency_key") == command.idempotency_key
    });
    let Some(existing) = existing else {
        return Ok(None);
    };
    if payload(existing, "mobile_command_fingerprint") != fingerprint {
        return Ok(Some(refusal(
            "mobile_command_idempotency_conflict",
            "idempotency key was already used for a different command",
            Some(command),
        )));
    }
    Ok(Some(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-session-command",
            "status": "IDEMPOTENT_REPLAY",
            "command_id": command.command_id,
            "idempotency_key": command.idempotency_key,
            "idempotent_replay": true,
            "session_id": command.session_id,
            "session_version": session_version_from_entries(kernel.ledger().entries(), &command.tenant_id, &command.session_id),
            "kind": command.kind,
            "control_receipt_id": payload(existing, "mobile_control_receipt_id"),
            "command_receipt_id": existing.receipt_id,
            "safe_summary": safe_command_summary(&command.kind),
            "stale_state_accepted": false,
            "grants_execution_authority": false,
            "production_write_allowed": false
        })
        .to_string(),
    )))
}

fn handle_needs_you(
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
        .map_err(|_| "kernel lock poisoned while projecting mobile decisions".to_string())?;
    let sessions = tenant_session_ids(&kernel, tenant_id);
    let context_session = sessions.last().cloned().unwrap_or_default();
    let context_version =
        session_version_from_entries(kernel.ledger().entries(), tenant_id, &context_session);
    let items = crate::forge_control_plane_projection::forge_needs_you_items(&kernel)
        .into_iter()
        .filter(|item| {
            item.evidence_receipt_ids.iter().any(|receipt_id| {
                kernel.ledger().entries().iter().any(|receipt| {
                    receipt.receipt_id == *receipt_id && receipt.tenant_id.as_str() == tenant_id
                })
            })
        })
        .map(|item| {
            let supported = item.kind == "ratify_plan"
                && item.action_state == "review_required"
                && !context_session.is_empty();
            let decision_subject_receipt_id = item
                .evidence_receipt_ids
                .iter()
                .find(|receipt_id| {
                    kernel.ledger().entries().iter().any(|receipt| {
                        receipt.receipt_id == **receipt_id
                            && receipt.kind == "fleet.plan.drafted"
                    })
                })
                .cloned();
            json!({
                "id": item.id,
                "session_id": if context_session.is_empty() { Value::Null } else { json!(context_session) },
                "expected_session_version": if context_version == 0 { Value::Null } else { json!(context_version) },
                "kind": item.kind,
                "title": item.label,
                "detail": item.line,
                "recommended_choice": if supported { "Approve the reviewed split without starting workers" } else { "Review this decision on Mac" },
                "alternatives": if supported { vec!["Keep workers blocked", "Return to Mac for deeper review"] } else { vec!["Open on Mac"] },
                "subject_type": item.subject_type,
                "subject_id": item.subject_id,
                "subject": mdx_mobile_relay::sanitize_summary(&item.subject),
                "action_state": item.action_state,
                "blocker_codes": item.blocker_codes,
                "evidence_receipt_ids": item.evidence_receipt_ids,
                "mobile_action_supported": supported,
                "decision_kind": if supported { Some("ratify_fleet_plan") } else { None },
                "decision_subject_receipt_id": if supported { decision_subject_receipt_id } else { None },
                "requires_biometric": supported,
                "exact_action_allowed": if supported { "record fleet.plan.ratified only" } else { "none" },
                "grants_worker_start_authority": false,
                "production_write_allowed": false
            })
        })
        .collect::<Vec<_>>();
    let source_receipt_ids = projected_receipt_ids(&items, "evidence_receipt_ids");
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-needs-you",
            "status": "OK",
            "tenant_id": tenant_id,
            "source_receipt_ids": source_receipt_ids,
            "items": items,
            "stale_decisions_allowed": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn handle_push_registration(
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
                "mobile_push_shape_invalid",
                "body must be JSON",
                None,
            ));
        }
    };
    let registration: MobilePushRegistrationRequest = match serde_json::from_value(parsed.clone()) {
        Ok(registration) => registration,
        Err(error) => {
            return Ok(refusal(
                "mobile_push_shape_invalid",
                &format!("registration does not match the signed contract: {error}"),
                None,
            ));
        }
    };
    let resolved = request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let updated_at_epoch = match validate_push_registration(&registration) {
        Ok(updated_at_epoch) => updated_at_epoch,
        Err((code, detail)) => return Ok(refusal(code, &detail, None)),
    };
    let attestation_payload = push_registration_attestation_payload(&parsed)?;
    if let Err(code) = mobile_pairing::verify_registered_device_signature(
        &resolved,
        &registration.device_id,
        &attestation_payload,
        &registration.device_signature,
        kernel,
    ) {
        return Ok(refusal(
            &code,
            "the registration must be signed by this account's active registered device key",
            None,
        ));
    }
    let fingerprint = digest_hex(registration.push_token.as_bytes());
    let registration_id = format!(
        "mobile_push_{}",
        &digest_hex(
            format!(
                "{}|{}|{}",
                resolved.tenant_id, registration.device_id, registration.kind
            )
            .as_bytes()
        )[..24]
    );
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned while registering mobile push target".to_string())?;
    let projection = kernel.mobile_trust_projection(&resolved.tenant_id, &resolved.actor_id);
    if projection
        .push_registrations
        .iter()
        .filter(|registration| registration.status == "active")
        .count()
        >= MAX_PUSH_REGISTRATIONS
        && !projection
            .push_registrations
            .iter()
            .any(|registration| registration.registration_id == registration_id)
    {
        return Ok(refusal(
            "mobile_push_capacity_reached",
            "push registration capacity is temporarily exhausted",
            None,
        ));
    }
    let report = match kernel.record_mobile_push_registration(
        MobilePushRegistration {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            registration_id: &registration_id,
            device_id: &registration.device_id,
            host_id: &registration.host_id,
            kind: &registration.kind,
            environment: &registration.environment,
            token_fingerprint: &fingerprint,
            device_signature_verification: "verified_registered_device_key",
            updated_at_epoch,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(refusal(
                error.code(),
                "active registered device and pairing required",
                None,
            ));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-push-registration",
            "status": "RECORDED_DELIVERY_CLOSED",
            "registration_id": registration_id,
            "receipt_id": report.receipt_id,
            "idempotent_replay": report.idempotent_replay,
            "state_source": "kernel_receipt_projection",
            "restart_durable": true,
            "durable_scope": "registration_metadata_only",
            "token_echoed": false,
            "raw_token_persisted": false,
            "device_signature_verification": "verified_registered_device_key",
            "notification_categories": ["needs_user", "review_ready", "failed_after_recovery"],
            "routine_progress_notifications_allowed": false,
            "production_delivery_allowed": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn handle_notification_intents(
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
    let user_id = verified
        .as_ref()
        .map(|identity| identity.actor_id.as_str())
        .unwrap_or("local_user");
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned while projecting notifications".to_string())?;
    let mut intents = crate::forge_control_plane_projection::forge_needs_you_items(&kernel)
        .into_iter()
        .filter(|item| {
            item.evidence_receipt_ids.iter().any(|receipt_id| {
                kernel.ledger().entries().iter().any(|receipt| {
                    receipt.receipt_id == *receipt_id && receipt.tenant_id.as_str() == tenant_id
                })
            })
        })
        .map(|item| {
            json!({
                "intent_id": format!("notification_{}", item.id),
                "category": "needs_user",
                "title": "Forge needs your judgment",
                "safe_summary": item.line,
                "subject_ref": item.subject_id,
                "evidence_refs": item.evidence_receipt_ids,
                "contains_secret_values": false
            })
        })
        .collect::<Vec<_>>();
    intents.extend(terminal_notification_intents(&kernel, tenant_id));
    let source_receipt_ids = projected_receipt_ids(&intents, "evidence_refs");
    let registrations = kernel
        .mobile_trust_projection(tenant_id, user_id)
        .push_registrations;
    let registration_count = registrations
        .iter()
        .filter(|registration| registration.status == "active")
        .count();
    let registration_records = registrations
        .iter()
        .filter(|registration| registration.status == "active")
        .map(|registration| {
            json!({
                "registration_id": registration.registration_id,
                "device_id": registration.device_id,
                "host_id": registration.host_id,
                "kind": registration.kind,
                "environment": registration.environment,
                "token_fingerprint_recorded": !registration.token_fingerprint.is_empty(),
                "raw_token_persisted": false,
                "delivery_capability": "closed_missing_secure_token_store",
                "device_signature_verification": registration.device_signature_verification,
                "updated_at_epoch": registration.updated_at_epoch,
                "receipt_id": registration.receipt_id,
                "state_source": "kernel_receipt_projection"
            })
        })
        .collect::<Vec<_>>();
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-notification-intents",
            "status": "DELIVERY_CLOSED",
            "tenant_id": tenant_id,
            "source_receipt_ids": source_receipt_ids,
            "registration_count": registration_count,
            "registration_records": registration_records,
            "registration_state_source": "kernel_receipt_projection",
            "registration_restart_durable": true,
            "registration_durable_scope": "metadata_only",
            "intents": intents,
            "notification_categories": ["needs_user", "review_ready", "failed_after_recovery"],
            "routine_progress_notifications_allowed": false,
            "contains_secret_values": false,
            "production_delivery_allowed": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn validate_push_registration(
    registration: &MobilePushRegistrationRequest,
) -> Result<u64, (&'static str, String)> {
    if registration.schema_version != 1
        || !valid_identifier(&registration.device_id)
        || !valid_identifier(&registration.host_id)
        || registration.push_token.len() < 32
        || registration.push_token.len() > 512
        || !matches!(registration.kind.as_str(), "apns" | "live_activity")
        || !matches!(registration.environment.as_str(), "sandbox" | "production")
        || registration.device_signature.trim().is_empty()
        || registration.device_signature.len() > 256
    {
        return Err((
            "mobile_push_shape_invalid",
            "registration identifiers, token, environment, or signature shape is invalid"
                .to_string(),
        ));
    }
    let issued = OffsetDateTime::parse(&registration.issued_at, &Rfc3339).map_err(|_| {
        (
            "mobile_push_time_invalid",
            "issued_at must be RFC3339".to_string(),
        )
    })?;
    let expires = OffsetDateTime::parse(&registration.expires_at, &Rfc3339).map_err(|_| {
        (
            "mobile_push_time_invalid",
            "expires_at must be RFC3339".to_string(),
        )
    })?;
    let now = OffsetDateTime::now_utc();
    if issued > now + time::Duration::seconds(MAX_CLOCK_SKEW_SECONDS)
        || expires <= now
        || expires <= issued
        || (expires - issued).whole_seconds() > MAX_COMMAND_LIFETIME_SECONDS
    {
        return Err((
            "mobile_push_expired_or_future",
            "registration must be current and expire within five minutes".to_string(),
        ));
    }
    Ok(issued.unix_timestamp().max(0) as u64)
}

fn push_registration_attestation_payload(body: &Value) -> Result<Vec<u8>, String> {
    let mut unsigned = body.clone();
    let object = unsigned
        .as_object_mut()
        .ok_or_else(|| "mobile push attestation requires a JSON object".to_string())?;
    object.remove("device_signature");
    serde_json::to_vec(&unsigned)
        .map_err(|error| format!("mobile push attestation serialization failed: {error}"))
}

fn projected_receipt_ids(values: &[Value], field: &str) -> Vec<String> {
    values
        .iter()
        .filter_map(|value| value[field].as_array())
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn terminal_notification_intents(kernel: &MdxKernel, tenant_id: &str) -> Vec<Value> {
    let mut latest_by_run: BTreeMap<String, &Receipt> = BTreeMap::new();
    for receipt in kernel.ledger().entries().iter().filter(|receipt| {
        receipt.kind == "forge.run.event"
            && receipt.tenant_id.as_str() == tenant_id
            && valid_identifier(payload(receipt, "run_id"))
    }) {
        latest_by_run.insert(payload(receipt, "run_id").to_string(), receipt);
    }
    latest_by_run
        .into_iter()
        .filter_map(|(run_id, receipt)| {
            let event = payload(receipt, "event");
            let detail = payload(receipt, "detail").to_ascii_lowercase();
            let (category, title, summary) = if event == "run_finished"
                && (detail.contains("fail")
                    || detail.contains("cannot")
                    || detail.contains("budget"))
            {
                (
                    "failed_after_recovery",
                    "Forge stopped safely",
                    "The build stopped after its recovery boundary. Open MDx to review the evidence.",
                )
            } else if event == "run_finished" || detail.starts_with("branch=") {
                (
                    "review_ready",
                    "Your build is ready to review",
                    "Outcome, proof, risk, and the focused diff are ready in MDx.",
                )
            } else {
                return None;
            };
            Some(json!({
                "intent_id": format!("notification_{category}_{run_id}"),
                "category": category,
                "title": title,
                "safe_summary": summary,
                "subject_ref": run_id,
                "evidence_refs": [receipt.receipt_id.clone()],
                "contains_secret_values": false
            }))
        })
        .collect()
}

fn validate_command(
    command: &MobileSessionCommand,
    resolved: &request_security::ResolvedWriteIdentity,
) -> Result<(), (&'static str, String)> {
    if command.schema_version != 1
        || !valid_identifier(&command.command_id)
        || !valid_identifier(&command.session_id)
        || !command.session_id.starts_with("forge_run_")
        || command.idempotency_key.len() < 16
        || command.idempotency_key.len() > 200
        || !valid_identifier(&command.actor_device_id)
        || !valid_identifier(&command.target.target_id)
        || command.target.display_name.trim().is_empty()
        || command.target.capability_revision == 0
        || command.delegation_envelope_ref.trim().is_empty()
        || command.delegation_envelope_ref.len() > 200
        || command.device_signature.trim().is_empty()
        || command.device_signature.len() > 256
        || command.grants_execution_authority
    {
        return Err((
            "mobile_command_shape_invalid",
            "command identifiers, target, delegation, or authority shape is invalid".to_string(),
        ));
    }
    if command.tenant_id != resolved.tenant_id
        || command.actor_user_id != resolved.actor_id
        || command.target.tenant_id != resolved.tenant_id
    {
        return Err((
            "mobile_command_identity_mismatch",
            "command identity must match the verified tenant and user".to_string(),
        ));
    }
    if !matches!(command.target.kind.as_str(), "paired_host" | "mdx_cloud") {
        return Err((
            "mobile_command_target_not_available",
            "mobile commands require a paired_host or verified mdx_cloud target".to_string(),
        ));
    }
    if !matches!(
        command.kind.as_str(),
        "start_build"
            | "follow_up"
            | "pause"
            | "resume"
            | "stop"
            | "answer_question"
            | "record_decision"
            | "request_handoff"
            | "request_changes"
            | "approve_and_open_pr"
    ) || !matches!(
        command.user_presence.as_str(),
        "not_required" | "device_unlocked" | "biometric_verified"
    ) {
        return Err((
            "mobile_command_enum_invalid",
            "command kind or user presence is unsupported".to_string(),
        ));
    }
    let issued = OffsetDateTime::parse(&command.issued_at, &Rfc3339).map_err(|_| {
        (
            "mobile_command_time_invalid",
            "issued_at must be RFC3339".to_string(),
        )
    })?;
    let expires = OffsetDateTime::parse(&command.expires_at, &Rfc3339).map_err(|_| {
        (
            "mobile_command_time_invalid",
            "expires_at must be RFC3339".to_string(),
        )
    })?;
    let now = OffsetDateTime::now_utc();
    if issued > now + time::Duration::seconds(MAX_CLOCK_SKEW_SECONDS)
        || expires <= now
        || expires <= issued
        || (expires - issued).whole_seconds() > MAX_COMMAND_LIFETIME_SECONDS
    {
        return Err((
            "mobile_command_expired_or_future",
            "command must be current and expire within five minutes".to_string(),
        ));
    }
    Ok(())
}

fn command_attestation_payload(body: &Value) -> Result<Vec<u8>, String> {
    let mut unsigned = body.clone();
    let object = unsigned
        .as_object_mut()
        .ok_or_else(|| "mobile command attestation requires a JSON object".to_string())?;
    object.remove("device_signature");
    serde_json::to_vec(&unsigned)
        .map_err(|error| format!("mobile command attestation serialization failed: {error}"))
}

fn session_version(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    session_id: &str,
) -> Result<usize, String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned while reading mobile session version".to_string())?;
    Ok(session_version_from_entries(
        kernel.ledger().entries(),
        tenant_id,
        session_id,
    ))
}

fn session_execution_target(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    session_id: &str,
) -> Result<Option<(String, String)>, String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned while reading mobile session target".to_string())?;
    for receipt in kernel.ledger().entries().iter().rev().filter(|receipt| {
        receipt.kind == "forge.run.event"
            && receipt.tenant_id.as_str() == tenant_id
            && payload(receipt, "run_id") == session_id
    }) {
        let kind = ["mobile_target_kind", "execution_target_kind"]
            .iter()
            .find_map(|key| {
                let value = payload(receipt, key);
                (!value.is_empty()).then(|| value.to_string())
            });
        let id = ["mobile_target_id", "execution_target_id"]
            .iter()
            .find_map(|key| {
                let value = payload(receipt, key);
                (!value.is_empty()).then(|| value.to_string())
            });
        if let (Some(kind), Some(id)) = (kind, id) {
            return Ok(Some((kind, id)));
        }
    }
    Ok(None)
}

fn command_repository_id(
    command: &MobileSessionCommand,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<String, String> {
    if command.kind == "start_build" {
        return Ok(payload_text(&command.payload, "repository_id").to_string());
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned while reading mobile session repository".to_string())?;
    for receipt in kernel.ledger().entries().iter().rev().filter(|receipt| {
        receipt.kind == "forge.run.event"
            && receipt.tenant_id.as_str() == command.tenant_id
            && payload(receipt, "run_id") == command.session_id
    }) {
        for key in ["handoff_repository_id", "repo_id"] {
            let repository_id = payload(receipt, key);
            if !repository_id.is_empty() {
                return Ok(repository_id.to_string());
            }
        }
    }
    Ok(String::new())
}

fn session_version_from_entries(entries: &[Receipt], tenant_id: &str, session_id: &str) -> usize {
    let matching = entries
        .iter()
        .filter(|receipt| {
            receipt.kind == "forge.run.event"
                && receipt.tenant_id.as_str() == tenant_id
                && payload(receipt, "run_id") == session_id
        })
        .collect::<Vec<_>>();
    matching
        .iter()
        .rev()
        .find_map(|receipt| receipt_sequence(&receipt.receipt_id))
        .unwrap_or(matching.len())
}

fn receipt_sequence(receipt_id: &str) -> Option<usize> {
    receipt_id
        .rsplit('_')
        .next()
        .filter(|suffix| !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|suffix| suffix.parse().ok())
}

fn tenant_session_ids(kernel: &MdxKernel, tenant_id: &str) -> Vec<String> {
    let mut sessions = Vec::new();
    for receipt in kernel.ledger().entries().iter().filter(|receipt| {
        receipt.kind == "forge.run.event"
            && receipt.tenant_id.as_str() == tenant_id
            && !payload(receipt, "work_item_id").starts_with("mobile_cloud_")
    }) {
        let session_id = payload(receipt, "run_id");
        if valid_identifier(session_id) && !sessions.iter().any(|existing| existing == session_id) {
            sessions.push(session_id.to_string());
        }
    }
    sessions
}

fn payload_text<'a>(payload: &'a Value, key: &str) -> &'a str {
    payload
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
}

fn payload<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn digest_hex(bytes: &[u8]) -> String {
    digest(&SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

fn safe_command_summary(kind: &str) -> &'static str {
    match kind {
        "start_build" => "Started the governed build",
        "follow_up" => "Added your direction at the next safe turn",
        "pause" => "Pausing at the next safe turn boundary",
        "resume" => "Resumed the build from its durable transcript",
        "stop" => "Stopping cleanly at the next safe turn boundary",
        "answer_question" => "Recorded your answer for Forge",
        "record_decision" => "Recorded the bounded human decision",
        "request_handoff" => "Moved the session through a durable target checkpoint",
        "request_changes" => "Started a governed revision from your review",
        "approve_and_open_pr" => "Recorded pull request approval for the reviewed commit",
        _ => "Recorded the mobile command",
    }
}

fn refusal(code: &str, detail: &str, command: Option<&MobileSessionCommand>) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-session-command",
            "status": "REFUSED",
            "error": code,
            "detail": detail,
            "command_id": command.map(|command| command.command_id.as_str()).unwrap_or(""),
            "session_id": command.map(|command| command.session_id.as_str()).unwrap_or(""),
            "kind": command.map(|command| command.kind.as_str()).unwrap_or(""),
            "command_receipt_id": "",
            "stale_state_accepted": false,
            "grants_execution_authority": false,
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
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    use mdx_core::{ForgeRepoConnect, ForgeRepoIndex, GovernedWriteIdentity};
    use ring::rand::SystemRandom;
    use ring::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair, KeyPair};
    use std::sync::OnceLock;

    fn test_device_keys() -> &'static Mutex<BTreeMap<String, Vec<u8>>> {
        static KEYS: OnceLock<Mutex<BTreeMap<String, Vec<u8>>>> = OnceLock::new();
        KEYS.get_or_init(|| Mutex::new(BTreeMap::new()))
    }

    fn register_device(device_id: &str, kernel: &Arc<RwLock<MdxKernel>>) {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
        let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
            .unwrap();
        test_device_keys()
            .lock()
            .unwrap()
            .insert(device_id.to_string(), pkcs8.as_ref().to_vec());
        let public_key = key.public_key().as_ref();
        let public_key_thumbprint = digest(&SHA256, public_key)
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let device = json!({
            "device_id": device_id,
            "platform": "ios",
            "display_name": "Command test iPhone",
            "public_key": URL_SAFE_NO_PAD.encode(public_key),
            "public_key_thumbprint": public_key_thumbprint
        });
        mobile_pairing::route_response("POST", "/mobile/devices.json", &device.to_string(), kernel)
            .unwrap()
            .unwrap();
    }

    fn device_signature(device_id: &str, message: &[u8]) -> String {
        let pkcs8 = test_device_keys().lock().unwrap()[device_id].clone();
        let rng = SystemRandom::new();
        let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &pkcs8, &rng).unwrap();
        URL_SAFE_NO_PAD.encode(key.sign(&rng, message).unwrap().as_ref())
    }

    fn pair(device_id: &str, host_id: &str, kernel: &Arc<RwLock<MdxKernel>>) {
        register_device(device_id, kernel);
        let host = json!({
            "host_id": host_id,
            "display_name": "Command test Mac",
            "public_key_thumbprint": format!("host_thumbprint_{host_id}_123456"),
            "accepts_inbound_connections": false
        });
        mobile_pairing::route_response("POST", "/mobile/hosts.json", &host.to_string(), kernel)
            .unwrap()
            .unwrap();
        let challenge = mobile_pairing::route_response(
            "POST",
            "/mobile/pairing-challenges.json",
            &json!({"device_id": device_id, "host_id": host_id}).to_string(),
            kernel,
        )
        .unwrap()
        .unwrap();
        let challenge: Value = serde_json::from_str(challenge.body_text()).unwrap();
        let device_signature = device_signature(
            device_id,
            challenge["challenge_token"].as_str().unwrap().as_bytes(),
        );
        let redemption = json!({
            "challenge_id": challenge["challenge_id"],
            "challenge_token": challenge["challenge_token"],
            "signature": challenge["signature"],
            "device_signature": device_signature
        });
        let paired = mobile_pairing::route_response(
            "POST",
            "/mobile/pairings.json",
            &redemption.to_string(),
            kernel,
        )
        .unwrap()
        .unwrap();
        let paired: Value = serde_json::from_str(paired.body_text()).unwrap();
        assert_eq!(paired["status"], "PAIRED");
    }

    fn seed_run(kernel: &Arc<RwLock<MdxKernel>>, session_id: &str) -> usize {
        let identity = mdx_core::GovernedWriteIdentity::local_demo("local_user");
        let report = kernel
            .write()
            .unwrap()
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id: session_id,
                    event: "run_started",
                    work_item_id: "mobile_command_test",
                    detail: "test run admitted",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[("repo_id", "repo_mobile_command_test")],
            )
            .unwrap();
        receipt_sequence(&report.receipt_id).expect("test receipt has a durable cursor")
    }

    fn seed_verified_cloud_environment(kernel: &Arc<RwLock<MdxKernel>>, environment_id: &str) {
        let identity = mdx_core::GovernedWriteIdentity::local_demo("local_user");
        let definition = json!({
            "schema_version": 1,
            "environment_id": environment_id,
            "environment_version": 1,
            "repository_id": "repo_mobile_command_test",
            "base_image": std::env::var("MDX_CLOUD_BASE_IMAGE")
                .unwrap_or_else(|_| "public.ecr.aws/amazonlinux/amazonlinux:2023".to_string()),
            "architecture": "linux/arm64",
            "setup_commands": ["cargo fetch --locked"],
            "proof_commands": ["cargo test --workspace --locked"],
            "service_dependencies": [],
            "cache_paths": ["target"],
            "resource_class": "medium",
            "network_allowlist": ["crates.io"],
            "secret_binding_refs": [],
            "preview_ports": [],
            "retention_hours": 24,
            "snapshot_policy": "after_verified_setup",
            "grants_execution_authority": false
        });
        let definition_json = serde_json::to_string(&definition).expect("definition JSON");
        let fingerprint =
            digest_hex(&serde_json::to_vec(&definition).expect("definition fingerprint input"));
        kernel
            .write()
            .unwrap()
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id: "forge_run_cloud_environment_command_test",
                    event: "evidence_appended",
                    work_item_id: "mobile_cloud_environment",
                    detail: "Cloud environment verified for command test",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[
                    ("cloud_record_kind", "cloud_environment"),
                    ("cloud_environment_id", environment_id),
                    ("cloud_environment_status", "VERIFIED"),
                    ("cloud_environment_repo_id", "repo_mobile_command_test"),
                    ("cloud_environment_fingerprint", fingerprint.as_str()),
                    (
                        "cloud_environment_definition_json",
                        definition_json.as_str(),
                    ),
                    ("cloud_verification_secret_values_recorded", "false"),
                ],
            )
            .unwrap();
    }

    fn command(
        session_id: &str,
        device_id: &str,
        host_id: &str,
        idempotency_key: &str,
        expected_version: usize,
    ) -> Value {
        let now = OffsetDateTime::now_utc();
        json!({
            "schema_version": 1,
            "command_id": format!("mobile_command_{idempotency_key}"),
            "idempotency_key": idempotency_key,
            "session_id": session_id,
            "expected_session_version": expected_version,
            "tenant_id": "local_tenant",
            "actor_user_id": "local_user",
            "actor_device_id": device_id,
            "target": {
                "kind": "paired_host",
                "target_id": host_id,
                "tenant_id": "local_tenant",
                "display_name": "Command test Mac",
                "capability_revision": 1
            },
            "kind": "follow_up",
            "payload": {"instruction": "Run the focused tests next"},
            "issued_at": now.format(&Rfc3339).unwrap(),
            "expires_at": (now + time::Duration::minutes(2)).format(&Rfc3339).unwrap(),
            "user_presence": "device_unlocked",
            "delegation_envelope_ref": "session_default",
            "grants_execution_authority": false,
            "device_signature": "test_signature_pending"
        })
    }

    fn signed_command_response(mut body: Value, kernel: &Arc<RwLock<MdxKernel>>) -> RouteResponse {
        let device_id = body["actor_device_id"].as_str().unwrap().to_string();
        attach_device_signature(&mut body, &device_id);
        handle_command("POST", &body.to_string(), kernel).unwrap()
    }

    fn attach_device_signature(body: &mut Value, signing_device_id: &str) {
        let payload = command_attestation_payload(body).unwrap();
        body["device_signature"] = json!(device_signature(signing_device_id, &payload));
    }

    fn push_registration_body(device_id: &str, host_id: &str, token: &str) -> Value {
        let now = OffsetDateTime::now_utc();
        json!({
            "schema_version": 1,
            "device_id": device_id,
            "host_id": host_id,
            "push_token": token,
            "kind": "apns",
            "environment": "sandbox",
            "issued_at": now.format(&Rfc3339).unwrap(),
            "expires_at": (now + time::Duration::minutes(2)).format(&Rfc3339).unwrap(),
            "device_signature": "test_signature_pending"
        })
    }

    fn attach_push_registration_signature(body: &mut Value, signing_device_id: &str) {
        let payload = push_registration_attestation_payload(body).unwrap();
        body["device_signature"] = json!(device_signature(signing_device_id, &payload));
    }

    fn response_value(response: RouteResponse) -> Value {
        serde_json::from_str(response.body_text()).unwrap()
    }

    #[test]
    fn empty_mobile_projections_keep_explicit_receipt_provenance() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let needs_you = response_value(handle_needs_you("GET", &kernel).unwrap());
        let notifications = response_value(handle_notification_intents("GET", &kernel).unwrap());

        assert_eq!(needs_you["source_receipt_ids"], json!([]));
        assert_eq!(notifications["source_receipt_ids"], json!([]));
    }

    #[test]
    fn session_version_is_the_latest_durable_cursor() {
        let mut kernel = MdxKernel::boot_local();
        let first = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                run_id: "forge_run_version_one",
                event: "run_started",
                work_item_id: "mobile_version_test",
                detail: "started",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .unwrap();
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                run_id: "forge_run_version_other",
                event: "run_started",
                work_item_id: "mobile_version_test",
                detail: "interleaved",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .unwrap();
        let latest = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                run_id: "forge_run_version_one",
                event: "agent_turn_prose",
                work_item_id: "mobile_version_test",
                detail: "continued",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .unwrap();
        let expected = receipt_sequence(&latest.receipt_id).unwrap();
        assert!(expected > receipt_sequence(&first.receipt_id).unwrap());
        assert_eq!(
            session_version_from_entries(
                kernel.ledger().entries(),
                "local_tenant",
                "forge_run_version_one"
            ),
            expected
        );
        assert_ne!(expected, 2);
    }

    #[test]
    fn push_registration_is_device_bound_secret_free_and_restart_durable() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        pair("iphone_mobile_push_test", "mac_mobile_push_test", &kernel);
        register_device("iphone_mobile_push_wrong_key", &kernel);
        let raw_token = "a".repeat(64);
        let mut body = push_registration_body(
            "iphone_mobile_push_test",
            "mac_mobile_push_test",
            &raw_token,
        );
        attach_push_registration_signature(&mut body, "iphone_mobile_push_test");
        let registered =
            response_value(handle_push_registration("POST", &body.to_string(), &kernel).unwrap());
        assert_eq!(registered["status"], "RECORDED_DELIVERY_CLOSED");
        assert_eq!(registered["restart_durable"], true);
        assert_eq!(registered["durable_scope"], "registration_metadata_only");
        assert_eq!(registered["state_source"], "kernel_receipt_projection");
        assert_eq!(registered["token_echoed"], false);
        assert_eq!(registered["raw_token_persisted"], false);
        assert_eq!(
            registered["device_signature_verification"],
            "verified_registered_device_key"
        );
        let receipt_id = registered["receipt_id"].as_str().unwrap().to_string();
        assert!(!receipt_id.is_empty());

        let replay =
            response_value(handle_push_registration("POST", &body.to_string(), &kernel).unwrap());
        assert_eq!(replay["receipt_id"], receipt_id);
        assert_eq!(replay["idempotent_replay"], true);
        assert!(
            kernel
                .read()
                .unwrap()
                .ledger()
                .entries()
                .iter()
                .all(|receipt| { receipt.payload.values().all(|value| value != &raw_token) })
        );
        let registration_receipt = kernel
            .read()
            .unwrap()
            .ledger()
            .entries()
            .iter()
            .find(|receipt| receipt.receipt_id == receipt_id)
            .unwrap()
            .clone();
        assert_eq!(
            registration_receipt.payload["device_signature_verification"],
            "verified_registered_device_key"
        );
        assert_eq!(registration_receipt.payload["raw_token_persisted"], "false");
        assert_eq!(
            registration_receipt.payload["delivery_capability"],
            "closed_missing_secure_token_store"
        );

        let original_issued =
            OffsetDateTime::parse(body["issued_at"].as_str().unwrap(), &Rfc3339).unwrap();
        let replacement_token = "e".repeat(64);
        let mut replacement = body.clone();
        replacement["push_token"] = json!(replacement_token);
        replacement["issued_at"] = json!(
            (original_issued + time::Duration::seconds(1))
                .format(&Rfc3339)
                .unwrap()
        );
        replacement["expires_at"] = json!(
            (original_issued + time::Duration::minutes(2))
                .format(&Rfc3339)
                .unwrap()
        );
        attach_push_registration_signature(&mut replacement, "iphone_mobile_push_test");
        let replacement = response_value(
            handle_push_registration("POST", &replacement.to_string(), &kernel).unwrap(),
        );
        assert_eq!(replacement["status"], "RECORDED_DELIVERY_CLOSED");
        assert_eq!(replacement["idempotent_replay"], false);
        let latest_receipt_id = replacement["receipt_id"].as_str().unwrap().to_string();
        assert_ne!(latest_receipt_id, receipt_id);
        assert!(
            kernel
                .read()
                .unwrap()
                .ledger()
                .entries()
                .iter()
                .all(|receipt| {
                    receipt
                        .payload
                        .values()
                        .all(|value| value != &replacement_token)
                })
        );

        let stale_replay =
            response_value(handle_push_registration("POST", &body.to_string(), &kernel).unwrap());
        assert_eq!(stale_replay["error"], "mobile_push_registration_stale");

        let mut wrong_key = push_registration_body(
            "iphone_mobile_push_test",
            "mac_mobile_push_test",
            &"b".repeat(64),
        );
        attach_push_registration_signature(&mut wrong_key, "iphone_mobile_push_wrong_key");
        let wrong_key = response_value(
            handle_push_registration("POST", &wrong_key.to_string(), &kernel).unwrap(),
        );
        assert_eq!(wrong_key["error"], "mobile_device_signature_invalid");

        let mut mutated = push_registration_body(
            "iphone_mobile_push_test",
            "mac_mobile_push_test",
            &"c".repeat(64),
        );
        attach_push_registration_signature(&mut mutated, "iphone_mobile_push_test");
        mutated["push_token"] = json!("d".repeat(64));
        let mutated = response_value(
            handle_push_registration("POST", &mutated.to_string(), &kernel).unwrap(),
        );
        assert_eq!(mutated["error"], "mobile_device_signature_invalid");

        let mut unknown_field = body.clone();
        unknown_field["delivery_allowed"] = json!(true);
        let unknown_field = response_value(
            handle_push_registration("POST", &unknown_field.to_string(), &kernel).unwrap(),
        );
        assert_eq!(unknown_field["error"], "mobile_push_shape_invalid");

        let entries = kernel.read().unwrap().ledger().entries().to_vec();
        let mut restarted_kernel = MdxKernel::boot_local();
        restarted_kernel.restore_ledger_entries(entries).unwrap();
        let restarted = Arc::new(RwLock::new(restarted_kernel));
        let projection = response_value(handle_notification_intents("GET", &restarted).unwrap());
        assert_eq!(projection["registration_count"], 1);
        assert_eq!(projection["registration_restart_durable"], true);
        assert_eq!(projection["registration_durable_scope"], "metadata_only");
        assert_eq!(
            projection["registration_records"][0]["receipt_id"],
            latest_receipt_id
        );
        assert_eq!(
            projection["registration_records"][0]["token_fingerprint_recorded"],
            true
        );
        assert_eq!(
            projection["registration_records"][0]["raw_token_persisted"],
            false
        );
        assert_eq!(
            projection["registration_records"][0]["delivery_capability"],
            "closed_missing_secure_token_store"
        );
        assert_eq!(projection["production_delivery_allowed"], false);
    }

    #[test]
    fn command_attestation_payload_is_canonical_across_clients() {
        let body = json!({
            "schema_version": 1,
            "command_id": "mobile_command_fixture",
            "idempotency_key": "mobile_idempotency_fixture",
            "session_id": "forge_run_fixture",
            "expected_session_version": 1,
            "tenant_id": "local_tenant",
            "actor_user_id": "local_user",
            "actor_device_id": "iphone_fixture",
            "target": {
                "kind": "paired_host",
                "target_id": "mac_fixture",
                "tenant_id": "local_tenant",
                "display_name": "Fixture Mac",
                "capability_revision": 1
            },
            "kind": "follow_up",
            "payload": {"instruction": "Run tests / safely"},
            "issued_at": "2023-11-14T22:13:20Z",
            "expires_at": "2023-11-14T22:15:20Z",
            "user_presence": "biometric_verified",
            "delegation_envelope_ref": "session_default",
            "grants_execution_authority": false,
            "device_signature": "excluded-from-attestation"
        });
        let expected = r#"{"actor_device_id":"iphone_fixture","actor_user_id":"local_user","command_id":"mobile_command_fixture","delegation_envelope_ref":"session_default","expected_session_version":1,"expires_at":"2023-11-14T22:15:20Z","grants_execution_authority":false,"idempotency_key":"mobile_idempotency_fixture","issued_at":"2023-11-14T22:13:20Z","kind":"follow_up","payload":{"instruction":"Run tests / safely"},"schema_version":1,"session_id":"forge_run_fixture","target":{"capability_revision":1,"display_name":"Fixture Mac","kind":"paired_host","target_id":"mac_fixture","tenant_id":"local_tenant"},"tenant_id":"local_tenant","user_presence":"biometric_verified"}"#;
        assert_eq!(
            String::from_utf8(command_attestation_payload(&body).unwrap()).unwrap(),
            expected
        );
    }

    #[test]
    fn stale_state_is_refused_before_any_control_is_recorded() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let session_id = "forge_run_mobile_stale_test";
        pair("iphone_mobile_stale_test", "mac_mobile_stale_test", &kernel);
        let version = seed_run(&kernel, session_id);
        let body = command(
            session_id,
            "iphone_mobile_stale_test",
            "mac_mobile_stale_test",
            "idempotency_stale_test_1234",
            version + 1,
        );
        let response = response_value(signed_command_response(body, &kernel));
        assert_eq!(response["status"], "REFUSED");
        assert_eq!(response["error"], "mobile_command_stale_session");
        assert!(
            kernel
                .read()
                .unwrap()
                .ledger()
                .query()
                .by_kind("forge.run.control")
                .is_empty()
        );
    }

    #[test]
    fn idempotency_replays_once_and_conflicts_on_changed_payload() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let session_id = "forge_run_mobile_idempotency_test";
        pair(
            "iphone_mobile_idempotency_test",
            "mac_mobile_idempotency_test",
            &kernel,
        );
        let version = seed_run(&kernel, session_id);
        let body = command(
            session_id,
            "iphone_mobile_idempotency_test",
            "mac_mobile_idempotency_test",
            "idempotency_replay_test_1234",
            version,
        );
        let accepted = response_value(signed_command_response(body.clone(), &kernel));
        assert_eq!(accepted["status"], "ACCEPTED");
        let replay = response_value(signed_command_response(body.clone(), &kernel));
        assert_eq!(replay["status"], "IDEMPOTENT_REPLAY");
        let mut conflict = body;
        conflict["payload"]["instruction"] = json!("Change the implementation instead");
        let conflict = response_value(signed_command_response(conflict, &kernel));
        assert_eq!(conflict["status"], "REFUSED");
        assert_eq!(conflict["error"], "mobile_command_idempotency_conflict");
        assert_eq!(
            kernel
                .read()
                .unwrap()
                .ledger()
                .query()
                .by_kind("forge.run.control")
                .len(),
            1
        );
    }

    #[test]
    fn command_attestation_rejects_missing_wrong_or_mutated_device_signatures() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let session_id = "forge_run_mobile_attestation_test";
        pair(
            "iphone_mobile_attestation_test",
            "mac_mobile_attestation_test",
            &kernel,
        );
        register_device("iphone_mobile_other_key_test", &kernel);
        let version = seed_run(&kernel, session_id);
        let body = command(
            session_id,
            "iphone_mobile_attestation_test",
            "mac_mobile_attestation_test",
            "idempotency_attestation_test_1234",
            version,
        );

        let mut missing = body.clone();
        missing.as_object_mut().unwrap().remove("device_signature");
        let missing =
            response_value(handle_command("POST", &missing.to_string(), &kernel).unwrap());
        assert_eq!(missing["error"], "mobile_command_shape_invalid");

        let mut wrong_key = body.clone();
        attach_device_signature(&mut wrong_key, "iphone_mobile_other_key_test");
        let wrong_key =
            response_value(handle_command("POST", &wrong_key.to_string(), &kernel).unwrap());
        assert_eq!(wrong_key["error"], "mobile_device_signature_invalid");

        let mut mutated = body.clone();
        attach_device_signature(&mut mutated, "iphone_mobile_attestation_test");
        mutated["payload"]["instruction"] = json!("Tampered after signing");
        let mutated =
            response_value(handle_command("POST", &mutated.to_string(), &kernel).unwrap());
        assert_eq!(mutated["error"], "mobile_device_signature_invalid");

        let accepted = response_value(signed_command_response(body, &kernel));
        assert_eq!(accepted["status"], "ACCEPTED");
        assert_eq!(
            accepted["user_presence_verification"],
            "client_asserted_device_key_bound"
        );
        assert_eq!(
            accepted["device_signature_verification"],
            "verified_registered_device_key"
        );
        let receipt_id = accepted["command_receipt_id"].as_str().unwrap();
        let guard = kernel.read().unwrap();
        let receipt = guard
            .ledger()
            .entries()
            .iter()
            .find(|receipt| receipt.receipt_id == receipt_id)
            .unwrap();
        assert_eq!(
            receipt.payload["mobile_user_presence_verification"],
            "client_asserted_device_key_bound"
        );
        assert_eq!(
            receipt.payload["mobile_device_signature_verification"],
            "verified_registered_device_key"
        );
    }

    #[test]
    fn expired_commands_and_unverified_decisions_fail_closed() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let mut expired = command(
            "forge_run_mobile_expired_test",
            "iphone_mobile_expired_test",
            "mac_mobile_expired_test",
            "idempotency_expired_test_1234",
            1,
        );
        expired["issued_at"] = json!("2025-01-01T00:00:00Z");
        expired["expires_at"] = json!("2025-01-01T00:02:00Z");
        let expired =
            response_value(handle_command("POST", &expired.to_string(), &kernel).unwrap());
        assert_eq!(expired["error"], "mobile_command_expired_or_future");

        pair(
            "iphone_mobile_decision_test",
            "mac_mobile_decision_test",
            &kernel,
        );
        let version = seed_run(&kernel, "forge_run_mobile_decision_test");
        let mut decision = command(
            "forge_run_mobile_decision_test",
            "iphone_mobile_decision_test",
            "mac_mobile_decision_test",
            "idempotency_decision_test_1234",
            version,
        );
        decision["kind"] = json!("record_decision");
        decision["payload"] = json!({
            "decision_kind": "ratify_fleet_plan",
            "fleet_id": "fleet_mobile_decision_test",
            "decision_subject_receipt_id": "receipt_stale",
            "reason": "Reviewed on iPhone"
        });
        decision["user_presence"] = json!("device_unlocked");
        let decision = response_value(signed_command_response(decision, &kernel));
        assert_eq!(decision["status"], "REFUSED");
        assert!(
            decision["detail"]
                .as_str()
                .unwrap()
                .contains("biometric user presence")
        );
    }

    #[test]
    fn cloud_target_claim_requires_verified_environment_not_mac_pairing() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let session_id = "forge_run_mobile_cloud_steer_test";
        register_device("iphone_mobile_cloud_test", &kernel);
        let version = seed_run(&kernel, session_id);
        let mut body = command(
            session_id,
            "iphone_mobile_cloud_test",
            "cloud_env_repo_test",
            "idempotency_cloud_steer_1234",
            version,
        );
        body["target"]["kind"] = json!("mdx_cloud");
        body["target"]["display_name"] = json!("MDx Cloud");
        let refused = response_value(signed_command_response(body.clone(), &kernel));
        assert_eq!(refused["status"], "REFUSED");
        assert_eq!(refused["error"], "mobile_command_cloud_target_unverified");

        seed_verified_cloud_environment(&kernel, "cloud_env_repo_test");
        let response = response_value(signed_command_response(body, &kernel));
        assert_eq!(response["status"], "ACCEPTED");
        assert_eq!(response["kind"], "follow_up");
    }

    #[test]
    fn handoff_requires_biometric_presence_before_checkpoint_work() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let session_id = "forge_run_mobile_handoff_presence_test";
        pair(
            "iphone_mobile_handoff_test",
            "mac_mobile_handoff_test",
            &kernel,
        );
        let version = seed_run(&kernel, session_id);
        let mut body = command(
            session_id,
            "iphone_mobile_handoff_test",
            "mac_mobile_handoff_test",
            "idempotency_handoff_presence_1234",
            version,
        );
        body["kind"] = json!("request_handoff");
        body["payload"] = json!({
            "destination_target": {
                "kind": "mdx_cloud",
                "target_id": "cloud_env_handoff_test",
                "tenant_id": "local_tenant",
                "display_name": "MDx Cloud",
                "capability_revision": 1
            },
            "destination_repository_id": "github_handoff_test"
        });
        let response = response_value(signed_command_response(body, &kernel));
        assert_eq!(response["status"], "REFUSED");
        assert!(response["detail"].as_str().unwrap().contains("biometric"));
        assert!(
            !kernel
                .read()
                .unwrap()
                .ledger()
                .entries()
                .iter()
                .any(|receipt| { payload(receipt, "checkpoint_id").starts_with("handoff_") })
        );
    }

    #[test]
    fn pull_request_approval_rejects_a_branch_other_than_the_recorded_default() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let session_id = "forge_run_mobile_pr_branch_test";
        let identity = GovernedWriteIdentity::local_demo("local_user");
        {
            let mut guard = kernel.write().expect("kernel");
            let connected = guard
                .connect_forge_repo_with_identity(
                    ForgeRepoConnect {
                        tenant_id: "local_tenant",
                        actor_id: "local_user",
                        repo_id: "review_repo",
                        label: "Review Repo",
                        root: "/tmp/review-repo",
                        kind: "remote",
                        origin: "https://github.com/mdx/review-repo.git",
                    },
                    &identity,
                )
                .expect("repo connected");
            guard
                .index_forge_repo_with_identity(
                    ForgeRepoIndex {
                        tenant_id: "local_tenant",
                        actor_id: "local_user",
                        repo_id: "review_repo",
                        repo_receipt_id: &connected.receipt_id,
                        profile_json: r#"{"default_branch":"trunk","default_branch_source":"caller_provided"}"#,
                        profile_fingerprint: "fnv1a64:branch-test",
                        primary_language: "rust",
                        language_pack_id: "rust-cargo",
                        detected_language_packs: "rust-cargo",
                        semantic_tool_readiness: "",
                        toolchain_readiness: "cargo",
                        proof_plan_status: "ready",
                        standards_source_summaries: "",
                    },
                    &identity,
                )
                .expect("repo indexed");
            guard
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "local_user",
                        run_id: session_id,
                        event: "run_started",
                        work_item_id: "mobile_command_test",
                        detail: "test run admitted",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &identity,
                    &[("repo_id", "review_repo")],
                )
                .expect("run recorded");
        }
        let mut body = command(
            session_id,
            "iphone_mobile_pr_test",
            "mac_mobile_pr_test",
            "idempotency_mobile_pr_test_1234",
            1,
        );
        body["kind"] = json!("approve_and_open_pr");
        body["user_presence"] = json!("biometric_verified");
        body["payload"] = json!({
            "reason": "Reviewed the exact outcome and proof",
            "commit_sha": "0123456789abcdef0123456789abcdef01234567",
            "target_host": "github",
            "base_branch": "main"
        });
        let command: MobileSessionCommand = serde_json::from_value(body).expect("command");

        let result = delegate_approve_and_open_pr(&command, &kernel).expect("delegation result");

        assert!(result.refused);
        assert!(result.detail.contains("recorded default branch"));
    }

    #[test]
    fn pull_request_completion_requires_a_canonical_github_url() {
        assert_eq!(
            opened_github_pull_request_url(&json!({
                "status": "LIVE_PR_OPENED",
                "pull_request_url": "https://github.com/mdx/repo/pull/42"
            })),
            Some("https://github.com/mdx/repo/pull/42".to_string())
        );
        assert_eq!(
            opened_github_pull_request_url(&json!({
                "status": "LIVE_PR_OPENED",
                "pull_request_url": ""
            })),
            None
        );
        assert_eq!(
            opened_github_pull_request_url(&json!({
                "status": "REFUSED",
                "pull_request_url": "https://github.com/mdx/repo/pull/42"
            })),
            None
        );
    }

    #[test]
    fn terminal_notification_summaries_do_not_include_run_detail() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                run_id: "forge_run_mobile_notification_test",
                event: "run_finished",
                work_item_id: "mobile_notification_test",
                detail: "failed token=private-source-value",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .unwrap();
        let intents = terminal_notification_intents(&kernel, "local_tenant");
        let wire = serde_json::to_string(&intents).unwrap();
        assert!(wire.contains("failed_after_recovery"));
        assert!(!wire.contains("private-source-value"));
        assert!(!wire.contains("token="));
    }

    #[test]
    fn handoff_repository_identity_matches_https_and_ssh_origins() {
        assert_eq!(
            normalize_git_origin("https://github.com/Acme/Forge.git"),
            normalize_git_origin("git@github.com:acme/forge.git")
        );
        assert_ne!(
            normalize_git_origin("https://github.com/acme/forge.git"),
            normalize_git_origin("https://github.com/acme/other.git")
        );
    }

    #[test]
    fn handoff_transfers_exact_branch_only_between_same_origin_repositories() {
        let root = std::env::temp_dir().join(format!(
            "mdx-mobile-handoff-{}-{}",
            std::process::id(),
            OffsetDateTime::now_utc().unix_timestamp_nanos()
        ));
        let source = root.join("source");
        let destination = root.join("destination");
        let unrelated = root.join("unrelated");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::create_dir_all(&unrelated).unwrap();

        git_ok(&source, &["init", "--quiet"]);
        git_ok(&source, &["config", "user.name", "MDx Test"]);
        git_ok(
            &source,
            &["config", "user.email", "mdx-test@example.invalid"],
        );
        git_ok(
            &source,
            &["remote", "add", "origin", "git@github.com:acme/forge.git"],
        );
        git_ok(
            &source,
            &["checkout", "--quiet", "-b", "forge/mobile-proof"],
        );
        std::fs::write(source.join("proof.txt"), "exact mobile checkpoint\n").unwrap();
        git_ok(&source, &["add", "proof.txt"]);
        git_ok(&source, &["commit", "--quiet", "-m", "mobile proof"]);
        let commit_sha = git_revision(source.to_str().unwrap(), "refs/heads/forge/mobile-proof");

        git_ok(&destination, &["init", "--quiet"]);
        git_ok(
            &destination,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/acme/forge.git",
            ],
        );
        let facts = HandoffFacts {
            destination_repo_id: "destination".to_string(),
            source_root: source.to_string_lossy().to_string(),
            destination_root: destination.to_string_lossy().to_string(),
            branch: "forge/mobile-proof".to_string(),
            commit_sha: commit_sha.clone(),
            event_cursor: 1,
            evidence_refs: vec![],
        };
        transfer_checkpoint_branch(&facts).expect("same-origin handoff");
        assert_eq!(
            git_revision(
                destination.to_str().unwrap(),
                "refs/heads/forge/mobile-proof"
            ),
            commit_sha
        );

        git_ok(&unrelated, &["init", "--quiet"]);
        git_ok(
            &unrelated,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/acme/unrelated.git",
            ],
        );
        let unrelated_facts = HandoffFacts {
            destination_root: unrelated.to_string_lossy().to_string(),
            ..facts
        };
        let refusal = transfer_checkpoint_branch(&unrelated_facts).unwrap_err();
        assert!(refusal.contains("same source-host identity"));

        std::fs::remove_dir_all(root).unwrap();
    }

    fn git_ok(root: &std::path::Path, args: &[&str]) {
        let status = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git command failed: {args:?}");
    }
}
