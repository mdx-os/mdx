use mdx_core::{
    LiveWorkerExecutionAdmission, LiveWorkerExecutionRejection, WorkerHandoffAdmission,
    WorkerRetirementAdmission, admit_live_worker_execution, admit_worker_handoff,
    admit_worker_retirement, json_string_literal,
};
use serde_json::Value;

const LIVE_WORKER_CREDENTIAL_CHECK_REJECTION_STATUS: &str =
    "LIVE_WORKER_EXECUTION_ADMISSION_REJECTED_MISSING_CREDENTIAL_CHECK_RECEIPT_ID";

pub struct DxrWorkerBoundary {
    pub sequence: usize,
    pub boundary_id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub parent_loop_id: String,
    pub worker_template_id: String,
    pub worker_run_id: String,
    pub spawn_receipt_id: String,
    pub credential_check_receipt_id: String,
    pub handoff_receipt_id: String,
    pub retirement_receipt_id: String,
    pub output_artifacts: Vec<String>,
    pub verification_evidence: Vec<String>,
    pub summary: String,
    pub next_owner: String,
}

#[derive(Clone)]
pub struct DxrLiveWorkerExecutionPreflight {
    pub sequence: usize,
    pub preflight_id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub worker_run_id: String,
    pub spawn_receipt_id: String,
    pub credential_check_receipt_id: String,
    pub handoff_receipt_id: String,
    pub retirement_receipt_id: String,
    pub provider_turn_on_receipt_id: String,
    pub dispatch_claim_id: String,
    pub heartbeat_receipt_id: String,
    pub durable_workflow_receipt_id: String,
    pub sandbox_authority_receipt_id: String,
    pub external_sandbox_preflight_id: String,
    pub tool_policy_receipt_id: String,
    pub reviewer_separation_receipt_id: String,
    pub requested_execution_mode: String,
    pub requested_receipt_kind: String,
    pub status: String,
    pub core_admission_status: String,
    pub provider_turn_on_observed: bool,
    pub authority_envelope_complete: bool,
}

struct WorkerBoundaryRequest {
    tenant_id: String,
    actor_id: String,
    parent_loop_id: String,
    worker_template_id: String,
    worker_run_id: String,
    spawn_receipt_id: String,
    credential_check_receipt_id: String,
    output_artifacts: Vec<String>,
    verification_evidence: Vec<String>,
    summary: String,
    next_owner: String,
}

struct LiveWorkerExecutionPreflightRequest {
    tenant_id: String,
    actor_id: String,
    worker_run_id: String,
    spawn_receipt_id: String,
    credential_check_receipt_id: String,
    handoff_receipt_id: String,
    retirement_receipt_id: String,
    provider_turn_on_receipt_id: String,
    dispatch_claim_id: String,
    heartbeat_receipt_id: String,
    durable_workflow_receipt_id: String,
    sandbox_authority_receipt_id: String,
    external_sandbox_preflight_id: String,
    tool_policy_receipt_id: String,
    reviewer_separation_receipt_id: String,
    requested_execution_mode: String,
    requested_receipt_kind: String,
}

pub fn parse_worker_boundary(body: &str, sequence: usize) -> Result<DxrWorkerBoundary, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR worker boundary json: {error}"))?;
    let request = WorkerBoundaryRequest {
        tenant_id: string_value(&value, "tenant_id", "tenant_local"),
        actor_id: string_value(&value, "actor_id", "forge_operator"),
        parent_loop_id: string_value(&value, "parent_loop_id", "forge_governed_loop"),
        worker_template_id: string_value(&value, "worker_template_id", "dxr_worker_template"),
        worker_run_id: string_value(
            &value,
            "worker_run_id",
            &format!("dxr_worker_run_{sequence:06}"),
        ),
        spawn_receipt_id: string_value(
            &value,
            "spawn_receipt_id",
            &format!("worker_spawn_request_receipt_{sequence:06}"),
        ),
        credential_check_receipt_id: string_value(
            &value,
            "credential_check_receipt_id",
            &format!("worker_credential_check_receipt_{sequence:06}"),
        ),
        output_artifacts: string_array(&value, "output_artifacts", &["dxr.local.worker.boundary"]),
        verification_evidence: string_array(
            &value,
            "verification_evidence",
            &["make worker-handoff-check", "make dxr-local-runtime-check"],
        ),
        summary: string_value(
            &value,
            "summary",
            "DXR recorded local worker handoff evidence and blocked live execution.",
        ),
        next_owner: string_value(&value, "next_owner", "human_operator"),
    };
    validate_worker_boundary(&request)?;
    Ok(DxrWorkerBoundary {
        sequence,
        boundary_id: format!("dxr_worker_boundary_{sequence:06}"),
        tenant_id: request.tenant_id,
        actor_id: request.actor_id,
        parent_loop_id: request.parent_loop_id,
        worker_template_id: request.worker_template_id,
        worker_run_id: request.worker_run_id,
        spawn_receipt_id: request.spawn_receipt_id,
        credential_check_receipt_id: request.credential_check_receipt_id,
        handoff_receipt_id: format!("worker_handoff_receipt_{sequence:06}"),
        retirement_receipt_id: format!("worker_retirement_receipt_{sequence:06}"),
        output_artifacts: request.output_artifacts,
        verification_evidence: request.verification_evidence,
        summary: request.summary,
        next_owner: request.next_owner,
    })
}

pub fn parse_live_worker_execution_preflight(
    body: &str,
    sequence: usize,
) -> Result<DxrLiveWorkerExecutionPreflight, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR live worker preflight json: {error}"))?;
    let request = LiveWorkerExecutionPreflightRequest {
        tenant_id: string_value(&value, "tenant_id", "tenant_local"),
        actor_id: string_value(&value, "actor_id", "forge_operator"),
        worker_run_id: string_value(
            &value,
            "worker_run_id",
            &format!("dxr_worker_run_{sequence:06}"),
        ),
        spawn_receipt_id: string_value(
            &value,
            "spawn_receipt_id",
            &format!("worker_spawn_request_receipt_{sequence:06}"),
        ),
        credential_check_receipt_id: string_value(&value, "credential_check_receipt_id", ""),
        handoff_receipt_id: string_value(
            &value,
            "handoff_receipt_id",
            &format!("worker_handoff_receipt_{sequence:06}"),
        ),
        retirement_receipt_id: string_value(
            &value,
            "retirement_receipt_id",
            &format!("worker_retirement_receipt_{sequence:06}"),
        ),
        provider_turn_on_receipt_id: string_value(&value, "provider_turn_on_receipt_id", ""),
        dispatch_claim_id: string_value(&value, "dispatch_claim_id", ""),
        heartbeat_receipt_id: string_value(&value, "heartbeat_receipt_id", ""),
        durable_workflow_receipt_id: string_value(&value, "durable_workflow_receipt_id", ""),
        sandbox_authority_receipt_id: string_value(&value, "sandbox_authority_receipt_id", ""),
        external_sandbox_preflight_id: string_value(&value, "external_sandbox_preflight_id", ""),
        tool_policy_receipt_id: string_value(&value, "tool_policy_receipt_id", ""),
        reviewer_separation_receipt_id: string_value(&value, "reviewer_separation_receipt_id", ""),
        requested_execution_mode: string_value(
            &value,
            "requested_execution_mode",
            "live_worker_execution",
        ),
        requested_receipt_kind: string_value(
            &value,
            "requested_receipt_kind",
            "worker.live_execution.authorized",
        ),
    };
    let admission = admit_live_worker_execution(&LiveWorkerExecutionAdmission {
        worker_run_id: &request.worker_run_id,
        spawn_receipt_id: &request.spawn_receipt_id,
        credential_check_receipt_id: &request.credential_check_receipt_id,
        handoff_receipt_id: &request.handoff_receipt_id,
        retirement_receipt_id: &request.retirement_receipt_id,
        provider_turn_on_receipt_id: &request.provider_turn_on_receipt_id,
        dispatch_claim_id: &request.dispatch_claim_id,
        heartbeat_receipt_id: &request.heartbeat_receipt_id,
        durable_workflow_receipt_id: &request.durable_workflow_receipt_id,
        sandbox_authority_receipt_id: &request.sandbox_authority_receipt_id,
        external_sandbox_preflight_id: &request.external_sandbox_preflight_id,
        tool_policy_receipt_id: &request.tool_policy_receipt_id,
        reviewer_separation_receipt_id: &request.reviewer_separation_receipt_id,
        requested_execution_mode: &request.requested_execution_mode,
        requested_receipt_kind: &request.requested_receipt_kind,
    });
    let provider_turn_on_observed = !request.provider_turn_on_receipt_id.trim().is_empty();
    let authority_envelope_complete = admission.is_ok();
    let (status, core_admission_status) = match admission {
        Ok(()) => (
            "DXR_LIVE_WORKER_EXECUTION_PREFLIGHT_RECORDED_AUTHORITY_ENVELOPE_STAGED",
            "LIVE_WORKER_EXECUTION_ADMISSION_ACCEPTED_PREFLIGHT_ONLY".to_string(),
        ),
        Err(error) => {
            let rejected_status = live_worker_rejection_status(error);
            let status = if matches!(
                error,
                LiveWorkerExecutionRejection::MissingProviderTurnOnReceipt
            ) {
                "DXR_LIVE_WORKER_EXECUTION_PREFLIGHT_RECORDED_PROVIDER_BLOCKED"
            } else {
                "DXR_LIVE_WORKER_EXECUTION_PREFLIGHT_RECORDED_AUTHORITY_ENVELOPE_BLOCKED"
            };
            (status, rejected_status)
        }
    };
    Ok(DxrLiveWorkerExecutionPreflight {
        sequence,
        preflight_id: format!("dxr_live_worker_preflight_{sequence:06}"),
        tenant_id: request.tenant_id,
        actor_id: request.actor_id,
        worker_run_id: request.worker_run_id,
        spawn_receipt_id: request.spawn_receipt_id,
        credential_check_receipt_id: request.credential_check_receipt_id,
        handoff_receipt_id: request.handoff_receipt_id,
        retirement_receipt_id: request.retirement_receipt_id,
        provider_turn_on_receipt_id: request.provider_turn_on_receipt_id,
        dispatch_claim_id: request.dispatch_claim_id,
        heartbeat_receipt_id: request.heartbeat_receipt_id,
        durable_workflow_receipt_id: request.durable_workflow_receipt_id,
        sandbox_authority_receipt_id: request.sandbox_authority_receipt_id,
        external_sandbox_preflight_id: request.external_sandbox_preflight_id,
        tool_policy_receipt_id: request.tool_policy_receipt_id,
        reviewer_separation_receipt_id: request.reviewer_separation_receipt_id,
        requested_execution_mode: request.requested_execution_mode,
        requested_receipt_kind: request.requested_receipt_kind,
        status: status.to_string(),
        core_admission_status,
        provider_turn_on_observed,
        authority_envelope_complete,
    })
}

pub fn worker_boundary_event_types() -> [&'static str; 4] {
    [
        "worker_lifecycle_started",
        "worker_handoff_recorded",
        "worker_retired",
        "worker_live_execution_blocked",
    ]
}

pub fn live_worker_preflight_event_types(
    preflight: &DxrLiveWorkerExecutionPreflight,
) -> Vec<&'static str> {
    let mut event_types = vec![
        "live_worker_execution_preflight_recorded",
        "live_worker_execution_authority_envelope_evaluated",
    ];
    match preflight.status.as_str() {
        "DXR_LIVE_WORKER_EXECUTION_PREFLIGHT_RECORDED_PROVIDER_BLOCKED" => {
            event_types.push("live_worker_execution_provider_blocked");
        }
        "DXR_LIVE_WORKER_EXECUTION_PREFLIGHT_RECORDED_AUTHORITY_ENVELOPE_STAGED" => {
            event_types.push("live_worker_execution_authority_envelope_staged");
            event_types.push("live_worker_execution_execution_blocked");
        }
        _ => {
            event_types.push("live_worker_execution_authority_envelope_blocked");
        }
    }
    event_types
}

pub fn render_worker_boundary_policy_json() -> String {
    r#"{"name":"mdx-dxr-worker-boundary","status":"LIVE-LOCAL-DXR-WORKER-LIFECYCLE","runtime":"mdx-dxr-engine","route":"/dxr/worker-boundary.json","submit_route":"/v1/dxr/worker-boundaries","list_route":"/dxr/worker-boundaries.json","live_worker_preflight_route":"/v1/dxr/live-worker-execution-preflights","live_worker_preflight_list_route":"/dxr/live-worker-execution-preflights.json","core_entrypoint":"mdx_core::MdxKernel::run_local_worker_runtime","handoff_admission":"mdx_core::admit_worker_handoff","retirement_admission":"mdx_core::admit_worker_retirement","live_execution_admission":"mdx_core::admit_live_worker_execution","required_source_receipts":["worker.credential.checked","worker.spawn_requested"],"required_live_authority_receipts":["provider.turn_on.observed","dispatch.claim.recorded","dispatch.heartbeat.renewed","dxr.durable_workflow.recorded","sandbox.authority.checked","external_sandbox.preflight.recorded","tool_policy.enforced","reviewer_separation.observed"],"recorded_receipts":["worker.handoff.recorded","worker.retired"],"live_worker_execution_status":"BLOCKED_UNTIL_PROVIDER_TURN_ON_FULL_AUTHORITY_ENVELOPE_AND_OPERATOR_RATIFICATION","live_worker_execution_allowed":false,"provider_turn_on_required":true,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}"#.to_string()
}

pub fn render_worker_boundary_json(boundary: &DxrWorkerBoundary) -> String {
    format!(
        r#"{{"sequence":{},"boundary_id":{},"tenant_id":{},"actor_id":{},"parent_loop_id":{},"worker_template_id":{},"worker_run_id":{},"runtime_status":"LIVE-LOCAL-DXR-WORKER-LIFECYCLE","terminal_state":"WORKER_LIFECYCLE_RECORDED_LIVE_EXECUTION_BLOCKED","spawn_receipt_id":{},"credential_check_receipt_id":{},"handoff_receipt_id":{},"retirement_receipt_id":{},"handoff_admission_status":"WORKER_HANDOFF_ADMISSION_ACCEPTED","retirement_admission_status":"WORKER_RETIREMENT_ADMISSION_ACCEPTED","required_source_receipts":["worker.credential.checked","worker.spawn_requested"],"recorded_receipts":["worker.handoff.recorded","worker.retired"],"output_artifacts":[{}],"verification_evidence":[{}],"summary":{},"next_owner":{},"provider_turn_on_required":true,"live_worker_execution_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"worker_runtime_doc":"docs/WORKER-RUNTIME-BOUNDARY.md","live_execution_gate_doc":"docs/WORKER-LIVE-EXECUTION-GATE.md"}}"#,
        boundary.sequence,
        json_string_literal(&boundary.boundary_id),
        json_string_literal(&boundary.tenant_id),
        json_string_literal(&boundary.actor_id),
        json_string_literal(&boundary.parent_loop_id),
        json_string_literal(&boundary.worker_template_id),
        json_string_literal(&boundary.worker_run_id),
        json_string_literal(&boundary.spawn_receipt_id),
        json_string_literal(&boundary.credential_check_receipt_id),
        json_string_literal(&boundary.handoff_receipt_id),
        json_string_literal(&boundary.retirement_receipt_id),
        render_string_array(&boundary.output_artifacts),
        render_string_array(&boundary.verification_evidence),
        json_string_literal(&boundary.summary),
        json_string_literal(&boundary.next_owner)
    )
}

pub fn render_live_worker_preflight_json(preflight: &DxrLiveWorkerExecutionPreflight) -> String {
    format!(
        r#"{{"sequence":{},"preflight_id":{},"tenant_id":{},"actor_id":{},"worker_run_id":{},"status":{},"terminal_state":"DXR_LIVE_WORKER_PREFLIGHT_RECORDED_EXECUTION_BLOCKED","core_admission":"mdx_core::admit_live_worker_execution","core_admission_status":{},"spawn_receipt_id":{},"credential_check_receipt_id":{},"handoff_receipt_id":{},"retirement_receipt_id":{},"provider_turn_on_receipt_id":{},"dispatch_claim_id":{},"heartbeat_receipt_id":{},"durable_workflow_receipt_id":{},"sandbox_authority_receipt_id":{},"external_sandbox_preflight_id":{},"tool_policy_receipt_id":{},"reviewer_separation_receipt_id":{},"requested_execution_mode":{},"requested_receipt_kind":{},"required_receipts":["worker.spawn_requested","worker.credential.checked","worker.handoff.recorded","worker.retired","provider.turn_on.observed","dispatch.claim.recorded","dispatch.heartbeat.renewed","dxr.durable_workflow.recorded","sandbox.authority.checked","external_sandbox.preflight.recorded","tool_policy.enforced","reviewer_separation.observed"],"provider_turn_on_required":true,"provider_turn_on_observed":{},"authority_envelope_complete":{},"preflight_only":true,"live_worker_execution_allowed":false,"worker_process_started":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"worker_authority_envelope_doc":"docs/WORKER-LIVE-EXECUTION-GATE.md","live_execution_gate_doc":"docs/WORKER-LIVE-EXECUTION-GATE.md"}}"#,
        preflight.sequence,
        json_string_literal(&preflight.preflight_id),
        json_string_literal(&preflight.tenant_id),
        json_string_literal(&preflight.actor_id),
        json_string_literal(&preflight.worker_run_id),
        json_string_literal(&preflight.status),
        json_string_literal(&preflight.core_admission_status),
        json_string_literal(&preflight.spawn_receipt_id),
        json_string_literal(&preflight.credential_check_receipt_id),
        json_string_literal(&preflight.handoff_receipt_id),
        json_string_literal(&preflight.retirement_receipt_id),
        json_string_literal(&preflight.provider_turn_on_receipt_id),
        json_string_literal(&preflight.dispatch_claim_id),
        json_string_literal(&preflight.heartbeat_receipt_id),
        json_string_literal(&preflight.durable_workflow_receipt_id),
        json_string_literal(&preflight.sandbox_authority_receipt_id),
        json_string_literal(&preflight.external_sandbox_preflight_id),
        json_string_literal(&preflight.tool_policy_receipt_id),
        json_string_literal(&preflight.reviewer_separation_receipt_id),
        json_string_literal(&preflight.requested_execution_mode),
        json_string_literal(&preflight.requested_receipt_kind),
        preflight.provider_turn_on_observed,
        preflight.authority_envelope_complete
    )
}

fn validate_worker_boundary(request: &WorkerBoundaryRequest) -> Result<(), String> {
    let output_artifacts = request
        .output_artifacts
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    let verification_evidence = request
        .verification_evidence
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    admit_worker_handoff(&WorkerHandoffAdmission {
        parent_loop_id: &request.parent_loop_id,
        worker_template_id: &request.worker_template_id,
        worker_run_id: &request.worker_run_id,
        spawn_receipt_id: &request.spawn_receipt_id,
        credential_check_receipt_id: &request.credential_check_receipt_id,
        output_artifacts: &output_artifacts,
        verification_evidence: &verification_evidence,
        summary: &request.summary,
        next_owner: &request.next_owner,
        requested_receipt_kind: "worker.handoff.recorded",
    })
    .map_err(|error| error.message())?;
    admit_worker_retirement(&WorkerRetirementAdmission {
        parent_loop_id: &request.parent_loop_id,
        worker_template_id: &request.worker_template_id,
        worker_run_id: &request.worker_run_id,
        spawn_receipt_id: &request.spawn_receipt_id,
        handoff_receipt_id: "worker_handoff_receipt_pending",
        requested_receipt_kind: "worker.retired",
    })
    .map_err(|error| error.message())
}

fn live_worker_rejection_status(error: LiveWorkerExecutionRejection) -> String {
    match error {
        LiveWorkerExecutionRejection::MissingField(field) => {
            if field == "credential_check_receipt_id" {
                return LIVE_WORKER_CREDENTIAL_CHECK_REJECTION_STATUS.to_string();
            }
            format!(
                "LIVE_WORKER_EXECUTION_ADMISSION_REJECTED_MISSING_{}",
                field.to_ascii_uppercase()
            )
        }
        LiveWorkerExecutionRejection::MissingProviderTurnOnReceipt => {
            "LIVE_WORKER_EXECUTION_ADMISSION_REJECTED_MISSING_PROVIDER_TURN_ON_RECEIPT".to_string()
        }
        LiveWorkerExecutionRejection::InvalidExecutionMode => {
            "LIVE_WORKER_EXECUTION_ADMISSION_REJECTED_INVALID_EXECUTION_MODE".to_string()
        }
        LiveWorkerExecutionRejection::InvalidReceiptKind => {
            "LIVE_WORKER_EXECUTION_ADMISSION_REJECTED_INVALID_RECEIPT_KIND".to_string()
        }
    }
}

fn string_value(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn string_array(value: &Value, key: &str, defaults: &[&str]) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| defaults.iter().map(|value| (*value).to_string()).collect())
}

fn render_string_array(values: &[String]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",")
}
