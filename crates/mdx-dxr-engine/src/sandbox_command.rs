use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_JOB_ID: &str = "dxr_job_dispatch_ready_001";
const DEFAULT_RUN_ID: &str = "dxr_run_dispatch_ready_001";
const DEFAULT_WORKER_RUN_ID: &str = "dxr_worker_run_local_001";
const DEFAULT_SESSION_ID: &str = "dxr_sandbox_session_000001";
const DEFAULT_SESSION_LEASE_ID: &str = "dxr_sandbox_session_lease_000001";
const DEFAULT_RUNNER_ID: &str = "dxr_sandbox_runner_000001";
const DEFAULT_POLICY_DECISION_ID: &str = "dxr_sandbox_command_policy_000001";
const DEFAULT_TOOL_POLICY_RECEIPT_ID: &str = "dxr_tool_execution_000001";
const DEFAULT_COMMAND_PLAN_HASH: &str = "sha256:dxr_sandbox_command_plan_hash";
const DEFAULT_INPUT_HASH: &str = "sha256:dxr_sandbox_command_input_hash";
const DEFAULT_OUTPUT_HASH: &str = "sha256:dxr_sandbox_command_output_hash";
const DEFAULT_ALLOWED_PATH_ROOT: &str = "/workspace";
const DEFAULT_OUTPUT_CAP_BYTES: usize = 262_144;
const DEFAULT_TIMEOUT_MS: usize = 60_000;
const DEFAULT_READY_P95_MS: usize = 2500;
const DEFAULT_TARGET_ENGINEERS: usize = 1000;
const DEFAULT_PEAK_PARALLEL_FORGE_BUILDS: usize = 5000;
const DEFAULT_COMMAND_BATCH_SIZE: usize = 320;

#[derive(Default)]
pub struct DxrSandboxCommandRuntime {
    preflights: Vec<DxrSandboxCommandPreflight>,
    next_preflight: usize,
}

pub struct DxrSandboxCommandResult {
    pub body: String,
    pub events: Vec<DxrSandboxCommandRuntimeEvent>,
}

pub struct DxrSandboxCommandRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrSandboxCommandPreflight {
    sequence: usize,
    preflight_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    idempotency_key: String,
    session_id: String,
    session_lease_id: String,
    runner_id: String,
    command_intent: String,
    command_family: String,
    requested_outcome: String,
    policy_decision_id: String,
    tool_policy_receipt_id: String,
    command_plan_hash: String,
    command_input_hash: String,
    command_output_hash: String,
    allowed_path_root: String,
    output_cap_bytes: usize,
    timeout_ms: usize,
    ready_p95_ms: usize,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    command_batch_size: usize,
    command_batch_count: usize,
    session_ready_observed: bool,
    session_lease_observed: bool,
    runner_ready_observed: bool,
    command_policy_observed: bool,
    tool_policy_observed: bool,
    path_mediation_observed: bool,
    output_cap_observed: bool,
    timeout_observed: bool,
    terminal_stream_observed: bool,
    command_stream_observed: bool,
    log_stream_observed: bool,
    artifact_stream_observed: bool,
    filesystem_watch_observed: bool,
    preview_service_observed: bool,
    human_escalation_observed: bool,
    audit_record_observed: bool,
    replay_cursor_observed: bool,
    kill_switch_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    status: String,
    terminal_state: String,
    command_decision: String,
    rejected: bool,
    rejection_reason: String,
}

struct SandboxCommandRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    idempotency_key: String,
    session_id: String,
    session_lease_id: String,
    runner_id: String,
    command_intent: String,
    command_family: String,
    requested_outcome: String,
    policy_decision_id: String,
    tool_policy_receipt_id: String,
    command_plan_hash: String,
    command_input_hash: String,
    command_output_hash: String,
    allowed_path_root: String,
    output_cap_bytes: usize,
    timeout_ms: usize,
    ready_p95_ms: usize,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    command_batch_size: usize,
    session_ready_observed: bool,
    session_lease_observed: bool,
    runner_ready_observed: bool,
    command_policy_observed: bool,
    tool_policy_observed: bool,
    path_mediation_observed: bool,
    output_cap_observed: bool,
    timeout_observed: bool,
    terminal_stream_observed: bool,
    command_stream_observed: bool,
    log_stream_observed: bool,
    artifact_stream_observed: bool,
    filesystem_watch_observed: bool,
    preview_service_observed: bool,
    human_escalation_observed: bool,
    audit_record_observed: bool,
    replay_cursor_observed: bool,
    kill_switch_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    command_start_requested: bool,
    sandbox_process_start_requested: bool,
    external_process_start_requested: bool,
    tool_execution_requested: bool,
    shell_execution_requested: bool,
    patch_application_requested: bool,
    git_execution_requested: bool,
    network_requested: bool,
    secret_inheritance_requested: bool,
    filesystem_mutation_requested: bool,
    ci_claim_requested: bool,
    deployment_requested: bool,
    production_write_requested: bool,
}

impl DxrSandboxCommandRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrSandboxCommandResult, String> {
        let request = parse_sandbox_command_request(body)?;
        self.next_preflight += 1;

        let authority_requested = request.command_start_requested
            || request.sandbox_process_start_requested
            || request.external_process_start_requested
            || request.tool_execution_requested
            || request.shell_execution_requested
            || request.patch_application_requested
            || request.git_execution_requested
            || request.network_requested
            || request.secret_inheritance_requested
            || request.filesystem_mutation_requested
            || request.ci_claim_requested
            || request.deployment_requested
            || request.production_write_requested;
        let required_evidence = request.required_evidence_observed();
        let command_shape_valid = request.command_shape_valid();
        let accepted = !authority_requested && required_evidence && command_shape_valid;

        let requested_human = request.requested_outcome == "human_escalation_required";
        let status = if authority_requested {
            "DXR_SANDBOX_COMMAND_PREFLIGHT_REJECTED_SECURITY_BOUNDARY"
        } else if !required_evidence || !command_shape_valid {
            "DXR_SANDBOX_COMMAND_PREFLIGHT_REJECTED_MISSING_EVIDENCE"
        } else if requested_human {
            "DXR_SANDBOX_COMMAND_PREFLIGHT_RECORDED_HUMAN_ESCALATION_REQUIRED"
        } else {
            "LIVE-LOCAL-DXR-SANDBOX-COMMAND-PREFLIGHT-FLOOR"
        };
        let terminal_state = if authority_requested {
            "DXR_SANDBOX_COMMAND_PREFLIGHT_REJECTED_SECURITY_BOUNDARY"
        } else if !required_evidence || !command_shape_valid {
            "DXR_SANDBOX_COMMAND_PREFLIGHT_REJECTED_MISSING_EVIDENCE"
        } else if requested_human {
            "DXR_SANDBOX_COMMAND_PREFLIGHT_RECORDED_HUMAN_ESCALATION_EXECUTION_BLOCKED"
        } else {
            "DXR_SANDBOX_COMMAND_PREFLIGHT_RECORDED_POLICY_DENIED_EXECUTION_BLOCKED"
        };
        let command_decision = if authority_requested {
            "rejected_security_boundary"
        } else if !required_evidence || !command_shape_valid {
            "rejected_missing_evidence"
        } else if requested_human {
            "human_escalation_required_execution_blocked"
        } else {
            "policy_denied_execution_blocked"
        };
        let rejection_reason = if authority_requested {
            "sandbox_command_preflight_cannot_start_processes_tools_shell_patch_git_network_secrets_filesystem_ci_deployments_or_production_writes"
        } else if !required_evidence {
            "missing_session_policy_stream_audit_or_scale_evidence"
        } else if !command_shape_valid {
            "invalid_command_policy_shape"
        } else {
            ""
        };

        let command_batch_count = ceil_div(
            request.peak_parallel_forge_builds,
            request.command_batch_size.max(1),
        );
        let preflight = DxrSandboxCommandPreflight {
            sequence: self.next_preflight,
            preflight_id: format!("dxr_sandbox_command_preflight_{:06}", self.next_preflight),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            worker_run_id: request.worker_run_id,
            idempotency_key: request.idempotency_key,
            session_id: request.session_id,
            session_lease_id: request.session_lease_id,
            runner_id: request.runner_id,
            command_intent: request.command_intent,
            command_family: request.command_family,
            requested_outcome: request.requested_outcome,
            policy_decision_id: request.policy_decision_id,
            tool_policy_receipt_id: request.tool_policy_receipt_id,
            command_plan_hash: request.command_plan_hash,
            command_input_hash: request.command_input_hash,
            command_output_hash: request.command_output_hash,
            allowed_path_root: request.allowed_path_root,
            output_cap_bytes: request.output_cap_bytes,
            timeout_ms: request.timeout_ms,
            ready_p95_ms: request.ready_p95_ms,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            command_batch_size: request.command_batch_size,
            command_batch_count,
            session_ready_observed: request.session_ready_observed,
            session_lease_observed: request.session_lease_observed,
            runner_ready_observed: request.runner_ready_observed,
            command_policy_observed: request.command_policy_observed,
            tool_policy_observed: request.tool_policy_observed,
            path_mediation_observed: request.path_mediation_observed,
            output_cap_observed: request.output_cap_observed,
            timeout_observed: request.timeout_observed,
            terminal_stream_observed: request.terminal_stream_observed,
            command_stream_observed: request.command_stream_observed,
            log_stream_observed: request.log_stream_observed,
            artifact_stream_observed: request.artifact_stream_observed,
            filesystem_watch_observed: request.filesystem_watch_observed,
            preview_service_observed: request.preview_service_observed,
            human_escalation_observed: request.human_escalation_observed,
            audit_record_observed: request.audit_record_observed,
            replay_cursor_observed: request.replay_cursor_observed,
            kill_switch_observed: request.kill_switch_observed,
            tenant_fairness_observed: request.tenant_fairness_observed,
            backpressure_observed: request.backpressure_observed,
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            command_decision: command_decision.to_string(),
            rejected: !accepted,
            rejection_reason: rejection_reason.to_string(),
        };
        let events = sandbox_command_runtime_events(&preflight);
        let body = render_sandbox_command_response_json(&preflight);
        self.preflights.push(preflight);
        Ok(DxrSandboxCommandResult { body, events })
    }

    pub fn preflights_json(&self) -> String {
        let accepted_count = self
            .preflights
            .iter()
            .filter(|preflight| !preflight.rejected)
            .count();
        let human_escalation_count = self
            .preflights
            .iter()
            .filter(|preflight| {
                preflight.status
                    == "DXR_SANDBOX_COMMAND_PREFLIGHT_RECORDED_HUMAN_ESCALATION_REQUIRED"
            })
            .count();
        let rejected_count = self.preflights.len().saturating_sub(accepted_count);
        format!(
            r#"{{"name":"mdx-dxr-sandbox-command-preflights","status":"LIVE-LOCAL-DXR-SANDBOX-COMMAND-PREFLIGHT-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/sandbox-command-preflights.json","submit_route":"/v1/dxr/sandbox-command-preflights","preflight_count":{},"accepted_count":{},"human_escalation_count":{},"rejected_count":{},"command_policy":"session_bound_policy_denial_before_command_start","scale_policy":"tenant_fair_1000_engineers_5000_forge_builds","required_gates":["session_ready","session_lease","runner_ready","command_policy","tool_policy","path_mediation","output_cap","timeout","terminal_stream","command_stream","log_stream","artifact_stream","filesystem_watch","preview_service","human_escalation","audit_record","replay_cursor","kill_switch","tenant_fairness","backpressure"],"command_started":false,"sandbox_process_started":false,"external_process_started":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"preflights":[{}]}}"#,
            self.preflights.len(),
            accepted_count,
            human_escalation_count,
            rejected_count,
            self.preflights
                .iter()
                .map(render_sandbox_command_preflight_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

impl SandboxCommandRequest {
    fn required_evidence_observed(&self) -> bool {
        self.session_ready_observed
            && self.session_lease_observed
            && self.runner_ready_observed
            && self.command_policy_observed
            && self.tool_policy_observed
            && self.path_mediation_observed
            && self.output_cap_observed
            && self.timeout_observed
            && self.terminal_stream_observed
            && self.command_stream_observed
            && self.log_stream_observed
            && self.artifact_stream_observed
            && self.filesystem_watch_observed
            && self.preview_service_observed
            && self.human_escalation_observed
            && self.audit_record_observed
            && self.replay_cursor_observed
            && self.kill_switch_observed
            && self.tenant_fairness_observed
            && self.backpressure_observed
    }

    fn command_shape_valid(&self) -> bool {
        !self.session_id.trim().is_empty()
            && !self.session_lease_id.trim().is_empty()
            && !self.runner_id.trim().is_empty()
            && matches!(
                self.command_intent.as_str(),
                "read_only_probe" | "quality_gate" | "artifact_read" | "human_escalation"
            )
            && matches!(
                self.command_family.as_str(),
                "filesystem_read"
                    | "test_probe"
                    | "build_probe"
                    | "lint_probe"
                    | "artifact_probe"
                    | "human_review"
            )
            && matches!(
                self.requested_outcome.as_str(),
                "policy_denied" | "human_escalation_required"
            )
            && !self.policy_decision_id.trim().is_empty()
            && !self.tool_policy_receipt_id.trim().is_empty()
            && self.command_plan_hash.starts_with("sha256:")
            && self.command_input_hash.starts_with("sha256:")
            && self.command_output_hash.starts_with("sha256:")
            && self.allowed_path_root.starts_with("/workspace")
            && self.output_cap_bytes > 0
            && self.output_cap_bytes <= DEFAULT_OUTPUT_CAP_BYTES
            && self.timeout_ms > 0
            && self.timeout_ms <= DEFAULT_TIMEOUT_MS
            && self.ready_p95_ms > 0
            && self.ready_p95_ms <= DEFAULT_READY_P95_MS
            && self.target_concurrent_engineers >= DEFAULT_TARGET_ENGINEERS
            && self.peak_parallel_forge_builds >= DEFAULT_PEAK_PARALLEL_FORGE_BUILDS
            && self.command_batch_size > 0
            && self.command_batch_size <= self.peak_parallel_forge_builds
    }
}

fn parse_sandbox_command_request(body: &str) -> Result<SandboxCommandRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR sandbox command json: {error}"))?;
    Ok(SandboxCommandRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", DEFAULT_JOB_ID),
        run_id: string_value(&value, "run_id", DEFAULT_RUN_ID),
        worker_run_id: string_value(&value, "worker_run_id", DEFAULT_WORKER_RUN_ID),
        idempotency_key: string_value(&value, "idempotency_key", "sandbox-command-preflight-001"),
        session_id: string_value(&value, "session_id", DEFAULT_SESSION_ID),
        session_lease_id: string_value(&value, "session_lease_id", DEFAULT_SESSION_LEASE_ID),
        runner_id: string_value(&value, "runner_id", DEFAULT_RUNNER_ID),
        command_intent: string_value(&value, "command_intent", "quality_gate"),
        command_family: string_value(&value, "command_family", "test_probe"),
        requested_outcome: string_value(&value, "requested_outcome", "policy_denied"),
        policy_decision_id: string_value(&value, "policy_decision_id", DEFAULT_POLICY_DECISION_ID),
        tool_policy_receipt_id: string_value(
            &value,
            "tool_policy_receipt_id",
            DEFAULT_TOOL_POLICY_RECEIPT_ID,
        ),
        command_plan_hash: string_value(&value, "command_plan_hash", DEFAULT_COMMAND_PLAN_HASH),
        command_input_hash: string_value(&value, "command_input_hash", DEFAULT_INPUT_HASH),
        command_output_hash: string_value(&value, "command_output_hash", DEFAULT_OUTPUT_HASH),
        allowed_path_root: string_value(&value, "allowed_path_root", DEFAULT_ALLOWED_PATH_ROOT),
        output_cap_bytes: usize_value(&value, "output_cap_bytes", DEFAULT_OUTPUT_CAP_BYTES),
        timeout_ms: usize_value(&value, "timeout_ms", DEFAULT_TIMEOUT_MS),
        ready_p95_ms: usize_value(&value, "ready_p95_ms", DEFAULT_READY_P95_MS),
        target_concurrent_engineers: usize_value(
            &value,
            "target_concurrent_engineers",
            DEFAULT_TARGET_ENGINEERS,
        ),
        peak_parallel_forge_builds: usize_value(
            &value,
            "peak_parallel_forge_builds",
            DEFAULT_PEAK_PARALLEL_FORGE_BUILDS,
        ),
        command_batch_size: usize_value(&value, "command_batch_size", DEFAULT_COMMAND_BATCH_SIZE),
        session_ready_observed: bool_value(&value, "session_ready_observed", false),
        session_lease_observed: bool_value(&value, "session_lease_observed", false),
        runner_ready_observed: bool_value(&value, "runner_ready_observed", false),
        command_policy_observed: bool_value(&value, "command_policy_observed", false),
        tool_policy_observed: bool_value(&value, "tool_policy_observed", false),
        path_mediation_observed: bool_value(&value, "path_mediation_observed", false),
        output_cap_observed: bool_value(&value, "output_cap_observed", false),
        timeout_observed: bool_value(&value, "timeout_observed", false),
        terminal_stream_observed: bool_value(&value, "terminal_stream_observed", false),
        command_stream_observed: bool_value(&value, "command_stream_observed", false),
        log_stream_observed: bool_value(&value, "log_stream_observed", false),
        artifact_stream_observed: bool_value(&value, "artifact_stream_observed", false),
        filesystem_watch_observed: bool_value(&value, "filesystem_watch_observed", false),
        preview_service_observed: bool_value(&value, "preview_service_observed", false),
        human_escalation_observed: bool_value(&value, "human_escalation_observed", false),
        audit_record_observed: bool_value(&value, "audit_record_observed", false),
        replay_cursor_observed: bool_value(&value, "replay_cursor_observed", false),
        kill_switch_observed: bool_value(&value, "kill_switch_observed", false),
        tenant_fairness_observed: bool_value(&value, "tenant_fairness_observed", false),
        backpressure_observed: bool_value(&value, "backpressure_observed", false),
        command_start_requested: bool_value(&value, "command_start_requested", false),
        sandbox_process_start_requested: bool_value(
            &value,
            "sandbox_process_start_requested",
            false,
        ),
        external_process_start_requested: bool_value(
            &value,
            "external_process_start_requested",
            false,
        ),
        tool_execution_requested: bool_value(&value, "tool_execution_requested", false),
        shell_execution_requested: bool_value(&value, "shell_execution_requested", false),
        patch_application_requested: bool_value(&value, "patch_application_requested", false),
        git_execution_requested: bool_value(&value, "git_execution_requested", false),
        network_requested: bool_value(&value, "network_requested", false),
        secret_inheritance_requested: bool_value(&value, "secret_inheritance_requested", false),
        filesystem_mutation_requested: bool_value(&value, "filesystem_mutation_requested", false),
        ci_claim_requested: bool_value(&value, "ci_claim_requested", false),
        deployment_requested: bool_value(&value, "deployment_requested", false),
        production_write_requested: bool_value(&value, "production_write_requested", false),
    })
}

fn sandbox_command_runtime_events(
    preflight: &DxrSandboxCommandPreflight,
) -> Vec<DxrSandboxCommandRuntimeEvent> {
    let mut event_types = vec!["sandbox_command_preflight_recorded"];
    if preflight.rejected {
        event_types.push("sandbox_command_preflight_rejected");
    } else {
        event_types.push("sandbox_command_session_bound");
        event_types.push("sandbox_command_policy_evaluated");
        event_types.push("sandbox_command_path_mediation_bound");
        event_types.push("sandbox_command_output_cap_bound");
        event_types.push("sandbox_command_streams_bound");
        if preflight.requested_outcome == "human_escalation_required" {
            event_types.push("sandbox_command_human_escalation_required");
        } else {
            event_types.push("sandbox_command_policy_denied");
        }
        event_types.push("sandbox_command_authority_blocked");
    }
    event_types
        .iter()
        .map(|event_type| DxrSandboxCommandRuntimeEvent {
            event_type: (*event_type).to_string(),
            tenant_id: preflight.tenant_id.clone(),
            job_id: preflight.job_id.clone(),
            run_id: preflight.run_id.clone(),
            actor_id: preflight.actor_id.clone(),
        })
        .collect()
}

fn render_sandbox_command_response_json(preflight: &DxrSandboxCommandPreflight) -> String {
    format!(
        r#"{{"name":"mdx-dxr-sandbox-command-preflight","status":{},"runtime":"mdx-dxr-engine","preflight":{},"preflight_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"session_id":{},"session_lease_id":{},"runner_id":{},"terminal_state":{},"command_decision":{},"rejected":{},"rejection_reason":{},"command_started":false,"sandbox_process_started":false,"external_process_started":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&preflight.status),
        render_sandbox_command_preflight_json(preflight),
        json_string_literal(&preflight.preflight_id),
        json_string_literal(&preflight.tenant_id),
        json_string_literal(&preflight.actor_id),
        json_string_literal(&preflight.job_id),
        json_string_literal(&preflight.run_id),
        json_string_literal(&preflight.worker_run_id),
        json_string_literal(&preflight.session_id),
        json_string_literal(&preflight.session_lease_id),
        json_string_literal(&preflight.runner_id),
        json_string_literal(&preflight.terminal_state),
        json_string_literal(&preflight.command_decision),
        preflight.rejected,
        json_string_literal(&preflight.rejection_reason)
    )
}

fn render_sandbox_command_preflight_json(preflight: &DxrSandboxCommandPreflight) -> String {
    format!(
        r#"{{"sequence":{},"preflight_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"idempotency_key":{},"session_id":{},"session_lease_id":{},"runner_id":{},"command_intent":{},"command_family":{},"requested_outcome":{},"policy_decision_id":{},"tool_policy_receipt_id":{},"command_plan_hash":{},"command_input_hash":{},"command_output_hash":{},"allowed_path_root":{},"output_cap_bytes":{},"timeout_ms":{},"ready_p95_ms":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"command_batch_size":{},"command_batch_count":{},"session_ready_observed":{},"session_lease_observed":{},"runner_ready_observed":{},"command_policy_observed":{},"tool_policy_observed":{},"path_mediation_observed":{},"output_cap_observed":{},"timeout_observed":{},"terminal_stream_observed":{},"command_stream_observed":{},"log_stream_observed":{},"artifact_stream_observed":{},"filesystem_watch_observed":{},"preview_service_observed":{},"human_escalation_observed":{},"audit_record_observed":{},"replay_cursor_observed":{},"kill_switch_observed":{},"tenant_fairness_observed":{},"backpressure_observed":{},"status":{},"terminal_state":{},"command_decision":{},"rejected":{},"rejection_reason":{},"command_started":false,"sandbox_process_started":false,"external_process_started":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        preflight.sequence,
        json_string_literal(&preflight.preflight_id),
        json_string_literal(&preflight.tenant_id),
        json_string_literal(&preflight.actor_id),
        json_string_literal(&preflight.job_id),
        json_string_literal(&preflight.run_id),
        json_string_literal(&preflight.worker_run_id),
        json_string_literal(&preflight.idempotency_key),
        json_string_literal(&preflight.session_id),
        json_string_literal(&preflight.session_lease_id),
        json_string_literal(&preflight.runner_id),
        json_string_literal(&preflight.command_intent),
        json_string_literal(&preflight.command_family),
        json_string_literal(&preflight.requested_outcome),
        json_string_literal(&preflight.policy_decision_id),
        json_string_literal(&preflight.tool_policy_receipt_id),
        json_string_literal(&preflight.command_plan_hash),
        json_string_literal(&preflight.command_input_hash),
        json_string_literal(&preflight.command_output_hash),
        json_string_literal(&preflight.allowed_path_root),
        preflight.output_cap_bytes,
        preflight.timeout_ms,
        preflight.ready_p95_ms,
        preflight.target_concurrent_engineers,
        preflight.peak_parallel_forge_builds,
        preflight.command_batch_size,
        preflight.command_batch_count,
        preflight.session_ready_observed,
        preflight.session_lease_observed,
        preflight.runner_ready_observed,
        preflight.command_policy_observed,
        preflight.tool_policy_observed,
        preflight.path_mediation_observed,
        preflight.output_cap_observed,
        preflight.timeout_observed,
        preflight.terminal_stream_observed,
        preflight.command_stream_observed,
        preflight.log_stream_observed,
        preflight.artifact_stream_observed,
        preflight.filesystem_watch_observed,
        preflight.preview_service_observed,
        preflight.human_escalation_observed,
        preflight.audit_record_observed,
        preflight.replay_cursor_observed,
        preflight.kill_switch_observed,
        preflight.tenant_fairness_observed,
        preflight.backpressure_observed,
        json_string_literal(&preflight.status),
        json_string_literal(&preflight.terminal_state),
        json_string_literal(&preflight.command_decision),
        preflight.rejected,
        json_string_literal(&preflight.rejection_reason)
    )
}

fn ceil_div(value: usize, divisor: usize) -> usize {
    if value == 0 {
        0
    } else {
        ((value - 1) / divisor) + 1
    }
}

fn string_value(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn bool_value(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn usize_value(value: &Value, key: &str, default: usize) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_command_preflight_records_policy_denial_and_rejects_authority() {
        let mut runtime = DxrSandboxCommandRuntime::new();
        let accepted = runtime
            .submit_json(
                r#"{"idempotency_key":"cmd-001","session_ready_observed":true,"session_lease_observed":true,"runner_ready_observed":true,"command_policy_observed":true,"tool_policy_observed":true,"path_mediation_observed":true,"output_cap_observed":true,"timeout_observed":true,"terminal_stream_observed":true,"command_stream_observed":true,"log_stream_observed":true,"artifact_stream_observed":true,"filesystem_watch_observed":true,"preview_service_observed":true,"human_escalation_observed":true,"audit_record_observed":true,"replay_cursor_observed":true,"kill_switch_observed":true,"tenant_fairness_observed":true,"backpressure_observed":true}"#,
            )
            .expect("accepted command preflight");
        assert!(
            accepted
                .body
                .contains("\"status\":\"LIVE-LOCAL-DXR-SANDBOX-COMMAND-PREFLIGHT-FLOOR\"")
        );
        assert!(
            accepted
                .body
                .contains("\"command_decision\":\"policy_denied_execution_blocked\"")
        );
        assert!(accepted.body.contains("\"command_started\":false"));
        assert!(
            accepted
                .events
                .iter()
                .any(|event| event.event_type == "sandbox_command_policy_denied")
        );

        let rejected = runtime
            .submit_json(
                r#"{"idempotency_key":"cmd-002","session_ready_observed":true,"session_lease_observed":true,"runner_ready_observed":true,"command_policy_observed":true,"tool_policy_observed":true,"path_mediation_observed":true,"output_cap_observed":true,"timeout_observed":true,"terminal_stream_observed":true,"command_stream_observed":true,"log_stream_observed":true,"artifact_stream_observed":true,"filesystem_watch_observed":true,"preview_service_observed":true,"human_escalation_observed":true,"audit_record_observed":true,"replay_cursor_observed":true,"kill_switch_observed":true,"tenant_fairness_observed":true,"backpressure_observed":true,"command_start_requested":true,"shell_execution_requested":true}"#,
            )
            .expect("rejected command preflight");
        assert!(
            rejected.body.contains(
                "\"status\":\"DXR_SANDBOX_COMMAND_PREFLIGHT_REJECTED_SECURITY_BOUNDARY\""
            )
        );
        assert!(rejected.body.contains("\"rejected\":true"));
        assert!(runtime.preflights_json().contains("\"preflight_count\":2"));
    }
}
