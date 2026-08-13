use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_JOB_ID: &str = "dxr_job_dispatch_ready_001";
const DEFAULT_RUN_ID: &str = "dxr_run_dispatch_ready_001";
const DEFAULT_WORKER_RUN_ID: &str = "dxr_worker_run_local_001";
const DEFAULT_SESSION_ID: &str = "dxr_sandbox_session_000001";
const DEFAULT_SESSION_LEASE_ID: &str = "dxr_sandbox_session_lease_000001";
const DEFAULT_COMMAND_PREFLIGHT_ID: &str = "dxr_sandbox_command_preflight_000001";
const DEFAULT_RESULT_HASH: &str = "sha256:dxr_sandbox_command_result_hash";
const DEFAULT_STDOUT_HASH: &str = "sha256:dxr_sandbox_command_stdout_hash";
const DEFAULT_STDERR_HASH: &str = "sha256:dxr_sandbox_command_stderr_hash";
const DEFAULT_ARTIFACT_HASH: &str = "sha256:dxr_sandbox_command_artifact_hash";
const DEFAULT_MAX_OUTPUT_BYTES: usize = 262_144;
const DEFAULT_MAX_DURATION_MS: usize = 60_000;
const DEFAULT_TARGET_ENGINEERS: usize = 1000;
const DEFAULT_PEAK_PARALLEL_FORGE_BUILDS: usize = 5000;
const DEFAULT_RESULT_BATCH_SIZE: usize = 320;

#[derive(Default)]
pub struct DxrSandboxCommandResultRuntime {
    results: Vec<DxrSandboxCommandResultRecord>,
    next_result: usize,
}

pub struct DxrSandboxCommandResultOutcome {
    pub body: String,
    pub events: Vec<DxrSandboxCommandResultRuntimeEvent>,
}

pub struct DxrSandboxCommandResultRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrSandboxCommandResultRecord {
    sequence: usize,
    command_result_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    idempotency_key: String,
    session_id: String,
    session_lease_id: String,
    command_preflight_id: String,
    command_intent: String,
    command_family: String,
    observed_outcome: String,
    exit_code: i64,
    duration_ms: usize,
    max_duration_ms: usize,
    stdout_chunk_count: usize,
    stderr_chunk_count: usize,
    artifact_count: usize,
    max_output_bytes: usize,
    observed_output_bytes: usize,
    command_result_hash: String,
    stdout_hash: String,
    stderr_hash: String,
    artifact_hash: String,
    adapter_driver: String,
    adapter_execution_receipt_id: String,
    command_preflight_observed: bool,
    session_lease_observed: bool,
    adapter_result_observed: bool,
    stdout_stream_observed: bool,
    stderr_stream_observed: bool,
    exit_code_observed: bool,
    duration_observed: bool,
    output_cap_enforced: bool,
    artifact_quarantine_observed: bool,
    log_quarantine_observed: bool,
    replay_cursor_observed: bool,
    audit_record_observed: bool,
    kill_switch_observed: bool,
    timeout_policy_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    result_batch_size: usize,
    result_batch_count: usize,
    status: String,
    terminal_state: String,
    result_decision: String,
    rejected: bool,
    rejection_reason: String,
}

struct SandboxCommandResultRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    idempotency_key: String,
    session_id: String,
    session_lease_id: String,
    command_preflight_id: String,
    command_intent: String,
    command_family: String,
    observed_outcome: String,
    exit_code: i64,
    duration_ms: usize,
    max_duration_ms: usize,
    stdout_chunk_count: usize,
    stderr_chunk_count: usize,
    artifact_count: usize,
    max_output_bytes: usize,
    observed_output_bytes: usize,
    command_result_hash: String,
    stdout_hash: String,
    stderr_hash: String,
    artifact_hash: String,
    adapter_driver: String,
    adapter_execution_receipt_id: String,
    command_preflight_observed: bool,
    session_lease_observed: bool,
    adapter_result_observed: bool,
    stdout_stream_observed: bool,
    stderr_stream_observed: bool,
    exit_code_observed: bool,
    duration_observed: bool,
    output_cap_enforced: bool,
    artifact_quarantine_observed: bool,
    log_quarantine_observed: bool,
    replay_cursor_observed: bool,
    audit_record_observed: bool,
    kill_switch_observed: bool,
    timeout_policy_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    result_batch_size: usize,
    host_process_start_requested: bool,
    dxr_command_execution_requested: bool,
    sandbox_process_start_requested: bool,
    unquarantined_output_requested: bool,
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

impl DxrSandboxCommandResultRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrSandboxCommandResultOutcome, String> {
        let request = parse_sandbox_command_result_request(body)?;
        self.next_result += 1;

        let authority_requested = request.host_process_start_requested
            || request.dxr_command_execution_requested
            || request.sandbox_process_start_requested
            || request.unquarantined_output_requested
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
        let result_shape_valid = request.result_shape_valid();

        let status = if authority_requested {
            "DXR_SANDBOX_COMMAND_RESULT_REJECTED_SECURITY_BOUNDARY"
        } else if !required_evidence || !result_shape_valid {
            "DXR_SANDBOX_COMMAND_RESULT_REJECTED_MISSING_EVIDENCE"
        } else {
            match request.observed_outcome.as_str() {
                "success" => "LIVE-LOCAL-DXR-SANDBOX-COMMAND-RESULT-FLOOR",
                "failure" => "DXR_SANDBOX_COMMAND_RESULT_RECORDED_FAILURE_QUARANTINED",
                "timeout" => "DXR_SANDBOX_COMMAND_RESULT_RECORDED_TIMEOUT_QUARANTINED",
                _ => "DXR_SANDBOX_COMMAND_RESULT_REJECTED_MISSING_EVIDENCE",
            }
        };
        let terminal_state = match status {
            "LIVE-LOCAL-DXR-SANDBOX-COMMAND-RESULT-FLOOR" => {
                "DXR_SANDBOX_COMMAND_RESULT_RECORDED_SUCCESS_QUARANTINED"
            }
            "DXR_SANDBOX_COMMAND_RESULT_RECORDED_FAILURE_QUARANTINED" => {
                "DXR_SANDBOX_COMMAND_RESULT_RECORDED_FAILURE_QUARANTINED"
            }
            "DXR_SANDBOX_COMMAND_RESULT_RECORDED_TIMEOUT_QUARANTINED" => {
                "DXR_SANDBOX_COMMAND_RESULT_RECORDED_TIMEOUT_QUARANTINED"
            }
            "DXR_SANDBOX_COMMAND_RESULT_REJECTED_SECURITY_BOUNDARY" => {
                "DXR_SANDBOX_COMMAND_RESULT_REJECTED_SECURITY_BOUNDARY"
            }
            _ => "DXR_SANDBOX_COMMAND_RESULT_REJECTED_MISSING_EVIDENCE",
        };
        let result_decision = match terminal_state {
            "DXR_SANDBOX_COMMAND_RESULT_RECORDED_SUCCESS_QUARANTINED" => {
                "success_observed_output_quarantined"
            }
            "DXR_SANDBOX_COMMAND_RESULT_RECORDED_FAILURE_QUARANTINED" => {
                "failure_observed_output_quarantined"
            }
            "DXR_SANDBOX_COMMAND_RESULT_RECORDED_TIMEOUT_QUARANTINED" => {
                "timeout_observed_output_quarantined"
            }
            "DXR_SANDBOX_COMMAND_RESULT_REJECTED_SECURITY_BOUNDARY" => "rejected_security_boundary",
            _ => "rejected_missing_evidence",
        };
        let rejection_reason = if authority_requested {
            "sandbox_command_result_cannot_start_host_or_sandbox_processes_or_open_tools_shell_patch_git_network_secrets_filesystem_ci_deployment_or_production_writes"
        } else if !required_evidence {
            "missing_preflight_session_adapter_stream_quarantine_replay_or_scale_evidence"
        } else if !result_shape_valid {
            "invalid_command_result_shape"
        } else {
            ""
        };

        let result_batch_count = ceil_div(
            request.peak_parallel_forge_builds,
            request.result_batch_size.max(1),
        );
        let record = DxrSandboxCommandResultRecord {
            sequence: self.next_result,
            command_result_id: format!("dxr_sandbox_command_result_{:06}", self.next_result),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            worker_run_id: request.worker_run_id,
            idempotency_key: request.idempotency_key,
            session_id: request.session_id,
            session_lease_id: request.session_lease_id,
            command_preflight_id: request.command_preflight_id,
            command_intent: request.command_intent,
            command_family: request.command_family,
            observed_outcome: request.observed_outcome,
            exit_code: request.exit_code,
            duration_ms: request.duration_ms,
            max_duration_ms: request.max_duration_ms,
            stdout_chunk_count: request.stdout_chunk_count,
            stderr_chunk_count: request.stderr_chunk_count,
            artifact_count: request.artifact_count,
            max_output_bytes: request.max_output_bytes,
            observed_output_bytes: request.observed_output_bytes,
            command_result_hash: request.command_result_hash,
            stdout_hash: request.stdout_hash,
            stderr_hash: request.stderr_hash,
            artifact_hash: request.artifact_hash,
            adapter_driver: request.adapter_driver,
            adapter_execution_receipt_id: request.adapter_execution_receipt_id,
            command_preflight_observed: request.command_preflight_observed,
            session_lease_observed: request.session_lease_observed,
            adapter_result_observed: request.adapter_result_observed,
            stdout_stream_observed: request.stdout_stream_observed,
            stderr_stream_observed: request.stderr_stream_observed,
            exit_code_observed: request.exit_code_observed,
            duration_observed: request.duration_observed,
            output_cap_enforced: request.output_cap_enforced,
            artifact_quarantine_observed: request.artifact_quarantine_observed,
            log_quarantine_observed: request.log_quarantine_observed,
            replay_cursor_observed: request.replay_cursor_observed,
            audit_record_observed: request.audit_record_observed,
            kill_switch_observed: request.kill_switch_observed,
            timeout_policy_observed: request.timeout_policy_observed,
            tenant_fairness_observed: request.tenant_fairness_observed,
            backpressure_observed: request.backpressure_observed,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            result_batch_size: request.result_batch_size,
            result_batch_count,
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            result_decision: result_decision.to_string(),
            rejected: status.starts_with("DXR_SANDBOX_COMMAND_RESULT_REJECTED"),
            rejection_reason: rejection_reason.to_string(),
        };
        let events = sandbox_command_result_events(&record);
        let body = render_sandbox_command_result_response_json(&record);
        self.results.push(record);
        Ok(DxrSandboxCommandResultOutcome { body, events })
    }

    pub fn results_json(&self) -> String {
        let success_count = self
            .results
            .iter()
            .filter(|result| result.observed_outcome == "success" && !result.rejected)
            .count();
        let failure_count = self
            .results
            .iter()
            .filter(|result| result.observed_outcome == "failure" && !result.rejected)
            .count();
        let timeout_count = self
            .results
            .iter()
            .filter(|result| result.observed_outcome == "timeout" && !result.rejected)
            .count();
        let rejected_count = self.results.iter().filter(|result| result.rejected).count();
        format!(
            r#"{{"name":"mdx-dxr-sandbox-command-results","status":"LIVE-LOCAL-DXR-SANDBOX-COMMAND-RESULT-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/sandbox-command-results.json","submit_route":"/v1/dxr/sandbox-command-results","result_count":{},"success_count":{},"failure_count":{},"timeout_count":{},"rejected_count":{},"result_policy":"adapter_observed_results_quarantined_before_consumption","scale_policy":"tenant_fair_1000_engineers_5000_forge_builds","allowed_outcomes":["success","failure","timeout"],"required_gates":["command_preflight","session_lease","adapter_result","stdout_stream","stderr_stream","exit_code","duration","output_cap","artifact_quarantine","log_quarantine","replay_cursor","audit_record","kill_switch","timeout_policy","tenant_fairness","backpressure"],"host_process_started":false,"dxr_executed_command":false,"sandbox_process_started":false,"unquarantined_output_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"results":[{}]}}"#,
            self.results.len(),
            success_count,
            failure_count,
            timeout_count,
            rejected_count,
            self.results
                .iter()
                .map(render_sandbox_command_result_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

impl SandboxCommandResultRequest {
    fn required_evidence_observed(&self) -> bool {
        self.command_preflight_observed
            && self.session_lease_observed
            && self.adapter_result_observed
            && self.stdout_stream_observed
            && self.stderr_stream_observed
            && self.exit_code_observed
            && self.duration_observed
            && self.output_cap_enforced
            && self.artifact_quarantine_observed
            && self.log_quarantine_observed
            && self.replay_cursor_observed
            && self.audit_record_observed
            && self.kill_switch_observed
            && self.timeout_policy_observed
            && self.tenant_fairness_observed
            && self.backpressure_observed
    }

    fn result_shape_valid(&self) -> bool {
        !self.session_id.trim().is_empty()
            && !self.session_lease_id.trim().is_empty()
            && !self.command_preflight_id.trim().is_empty()
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
                self.observed_outcome.as_str(),
                "success" | "failure" | "timeout"
            )
            && valid_exit_code(self.observed_outcome.as_str(), self.exit_code)
            && self.duration_ms > 0
            && self.duration_ms <= self.max_duration_ms
            && self.max_duration_ms <= DEFAULT_MAX_DURATION_MS
            && self.stdout_chunk_count + self.stderr_chunk_count > 0
            && self.observed_output_bytes <= self.max_output_bytes
            && self.max_output_bytes <= DEFAULT_MAX_OUTPUT_BYTES
            && self.command_result_hash.starts_with("sha256:")
            && self.stdout_hash.starts_with("sha256:")
            && self.stderr_hash.starts_with("sha256:")
            && self.artifact_hash.starts_with("sha256:")
            && adapter_driver_known(&self.adapter_driver)
            && !self.adapter_execution_receipt_id.trim().is_empty()
            && self.target_concurrent_engineers >= DEFAULT_TARGET_ENGINEERS
            && self.peak_parallel_forge_builds >= DEFAULT_PEAK_PARALLEL_FORGE_BUILDS
            && self.result_batch_size > 0
            && self.result_batch_size <= self.peak_parallel_forge_builds
    }
}

fn parse_sandbox_command_result_request(body: &str) -> Result<SandboxCommandResultRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR sandbox command result json: {error}"))?;
    Ok(SandboxCommandResultRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", DEFAULT_JOB_ID),
        run_id: string_value(&value, "run_id", DEFAULT_RUN_ID),
        worker_run_id: string_value(&value, "worker_run_id", DEFAULT_WORKER_RUN_ID),
        idempotency_key: string_value(&value, "idempotency_key", "sandbox-command-result-001"),
        session_id: string_value(&value, "session_id", DEFAULT_SESSION_ID),
        session_lease_id: string_value(&value, "session_lease_id", DEFAULT_SESSION_LEASE_ID),
        command_preflight_id: string_value(
            &value,
            "command_preflight_id",
            DEFAULT_COMMAND_PREFLIGHT_ID,
        ),
        command_intent: string_value(&value, "command_intent", "quality_gate"),
        command_family: string_value(&value, "command_family", "test_probe"),
        observed_outcome: string_value(&value, "observed_outcome", "success"),
        exit_code: i64_value(&value, "exit_code", 0),
        duration_ms: usize_value(&value, "duration_ms", 240),
        max_duration_ms: usize_value(&value, "max_duration_ms", DEFAULT_MAX_DURATION_MS),
        stdout_chunk_count: usize_value(&value, "stdout_chunk_count", 2),
        stderr_chunk_count: usize_value(&value, "stderr_chunk_count", 1),
        artifact_count: usize_value(&value, "artifact_count", 1),
        max_output_bytes: usize_value(&value, "max_output_bytes", DEFAULT_MAX_OUTPUT_BYTES),
        observed_output_bytes: usize_value(&value, "observed_output_bytes", 4096),
        command_result_hash: string_value(&value, "command_result_hash", DEFAULT_RESULT_HASH),
        stdout_hash: string_value(&value, "stdout_hash", DEFAULT_STDOUT_HASH),
        stderr_hash: string_value(&value, "stderr_hash", DEFAULT_STDERR_HASH),
        artifact_hash: string_value(&value, "artifact_hash", DEFAULT_ARTIFACT_HASH),
        adapter_driver: string_value(&value, "adapter_driver", "cloudflare_sandbox"),
        adapter_execution_receipt_id: string_value(
            &value,
            "adapter_execution_receipt_id",
            "dxr_sandbox_adapter_execution_000001",
        ),
        command_preflight_observed: bool_value(&value, "command_preflight_observed", false),
        session_lease_observed: bool_value(&value, "session_lease_observed", false),
        adapter_result_observed: bool_value(&value, "adapter_result_observed", false),
        stdout_stream_observed: bool_value(&value, "stdout_stream_observed", false),
        stderr_stream_observed: bool_value(&value, "stderr_stream_observed", false),
        exit_code_observed: bool_value(&value, "exit_code_observed", false),
        duration_observed: bool_value(&value, "duration_observed", false),
        output_cap_enforced: bool_value(&value, "output_cap_enforced", false),
        artifact_quarantine_observed: bool_value(&value, "artifact_quarantine_observed", false),
        log_quarantine_observed: bool_value(&value, "log_quarantine_observed", false),
        replay_cursor_observed: bool_value(&value, "replay_cursor_observed", false),
        audit_record_observed: bool_value(&value, "audit_record_observed", false),
        kill_switch_observed: bool_value(&value, "kill_switch_observed", false),
        timeout_policy_observed: bool_value(&value, "timeout_policy_observed", false),
        tenant_fairness_observed: bool_value(&value, "tenant_fairness_observed", false),
        backpressure_observed: bool_value(&value, "backpressure_observed", false),
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
        result_batch_size: usize_value(&value, "result_batch_size", DEFAULT_RESULT_BATCH_SIZE),
        host_process_start_requested: bool_value(&value, "host_process_start_requested", false),
        dxr_command_execution_requested: bool_value(
            &value,
            "dxr_command_execution_requested",
            false,
        ),
        sandbox_process_start_requested: bool_value(
            &value,
            "sandbox_process_start_requested",
            false,
        ),
        unquarantined_output_requested: bool_value(&value, "unquarantined_output_requested", false),
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

fn sandbox_command_result_events(
    result: &DxrSandboxCommandResultRecord,
) -> Vec<DxrSandboxCommandResultRuntimeEvent> {
    let mut event_types = vec!["sandbox_command_result_recorded"];
    if result.rejected {
        event_types.push("sandbox_command_result_rejected");
    } else {
        event_types.push("sandbox_command_result_preflight_bound");
        event_types.push("sandbox_command_result_streams_recorded");
        event_types.push("sandbox_command_result_output_quarantined");
        match result.observed_outcome.as_str() {
            "success" => event_types.push("sandbox_command_result_success_observed"),
            "failure" => event_types.push("sandbox_command_result_failure_observed"),
            "timeout" => event_types.push("sandbox_command_result_timeout_observed"),
            _ => event_types.push("sandbox_command_result_rejected"),
        }
        event_types.push("sandbox_command_result_authority_blocked");
    }
    event_types
        .iter()
        .map(|event_type| DxrSandboxCommandResultRuntimeEvent {
            event_type: (*event_type).to_string(),
            tenant_id: result.tenant_id.clone(),
            job_id: result.job_id.clone(),
            run_id: result.run_id.clone(),
            actor_id: result.actor_id.clone(),
        })
        .collect()
}

fn render_sandbox_command_result_response_json(result: &DxrSandboxCommandResultRecord) -> String {
    format!(
        r#"{{"name":"mdx-dxr-sandbox-command-result","status":{},"runtime":"mdx-dxr-engine","result":{},"command_result_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"session_id":{},"session_lease_id":{},"command_preflight_id":{},"terminal_state":{},"result_decision":{},"rejected":{},"rejection_reason":{},"host_process_started":false,"dxr_executed_command":false,"sandbox_process_started":false,"unquarantined_output_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&result.status),
        render_sandbox_command_result_json(result),
        json_string_literal(&result.command_result_id),
        json_string_literal(&result.tenant_id),
        json_string_literal(&result.actor_id),
        json_string_literal(&result.job_id),
        json_string_literal(&result.run_id),
        json_string_literal(&result.worker_run_id),
        json_string_literal(&result.session_id),
        json_string_literal(&result.session_lease_id),
        json_string_literal(&result.command_preflight_id),
        json_string_literal(&result.terminal_state),
        json_string_literal(&result.result_decision),
        result.rejected,
        json_string_literal(&result.rejection_reason)
    )
}

fn render_sandbox_command_result_json(result: &DxrSandboxCommandResultRecord) -> String {
    format!(
        r#"{{"sequence":{},"command_result_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"idempotency_key":{},"session_id":{},"session_lease_id":{},"command_preflight_id":{},"command_intent":{},"command_family":{},"observed_outcome":{},"exit_code":{},"duration_ms":{},"max_duration_ms":{},"stdout_chunk_count":{},"stderr_chunk_count":{},"artifact_count":{},"max_output_bytes":{},"observed_output_bytes":{},"command_result_hash":{},"stdout_hash":{},"stderr_hash":{},"artifact_hash":{},"adapter_driver":{},"adapter_execution_receipt_id":{},"command_preflight_observed":{},"session_lease_observed":{},"adapter_result_observed":{},"stdout_stream_observed":{},"stderr_stream_observed":{},"exit_code_observed":{},"duration_observed":{},"output_cap_enforced":{},"artifact_quarantine_observed":{},"log_quarantine_observed":{},"replay_cursor_observed":{},"audit_record_observed":{},"kill_switch_observed":{},"timeout_policy_observed":{},"tenant_fairness_observed":{},"backpressure_observed":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"result_batch_size":{},"result_batch_count":{},"status":{},"terminal_state":{},"result_decision":{},"rejected":{},"rejection_reason":{},"host_process_started":false,"dxr_executed_command":false,"sandbox_process_started":false,"unquarantined_output_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        result.sequence,
        json_string_literal(&result.command_result_id),
        json_string_literal(&result.tenant_id),
        json_string_literal(&result.actor_id),
        json_string_literal(&result.job_id),
        json_string_literal(&result.run_id),
        json_string_literal(&result.worker_run_id),
        json_string_literal(&result.idempotency_key),
        json_string_literal(&result.session_id),
        json_string_literal(&result.session_lease_id),
        json_string_literal(&result.command_preflight_id),
        json_string_literal(&result.command_intent),
        json_string_literal(&result.command_family),
        json_string_literal(&result.observed_outcome),
        result.exit_code,
        result.duration_ms,
        result.max_duration_ms,
        result.stdout_chunk_count,
        result.stderr_chunk_count,
        result.artifact_count,
        result.max_output_bytes,
        result.observed_output_bytes,
        json_string_literal(&result.command_result_hash),
        json_string_literal(&result.stdout_hash),
        json_string_literal(&result.stderr_hash),
        json_string_literal(&result.artifact_hash),
        json_string_literal(&result.adapter_driver),
        json_string_literal(&result.adapter_execution_receipt_id),
        result.command_preflight_observed,
        result.session_lease_observed,
        result.adapter_result_observed,
        result.stdout_stream_observed,
        result.stderr_stream_observed,
        result.exit_code_observed,
        result.duration_observed,
        result.output_cap_enforced,
        result.artifact_quarantine_observed,
        result.log_quarantine_observed,
        result.replay_cursor_observed,
        result.audit_record_observed,
        result.kill_switch_observed,
        result.timeout_policy_observed,
        result.tenant_fairness_observed,
        result.backpressure_observed,
        result.target_concurrent_engineers,
        result.peak_parallel_forge_builds,
        result.result_batch_size,
        result.result_batch_count,
        json_string_literal(&result.status),
        json_string_literal(&result.terminal_state),
        json_string_literal(&result.result_decision),
        result.rejected,
        json_string_literal(&result.rejection_reason)
    )
}

fn valid_exit_code(outcome: &str, exit_code: i64) -> bool {
    match outcome {
        "success" => exit_code == 0,
        "failure" => exit_code > 0,
        "timeout" => exit_code == 124,
        _ => false,
    }
}

fn adapter_driver_known(driver: &str) -> bool {
    matches!(
        driver,
        "local_docker"
            | "firecracker_microvm"
            | "cloudflare_sandbox"
            | "modal_sandbox"
            | "e2b_sandbox"
            | "codex_cloud_sandbox"
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

fn i64_value(value: &Value, key: &str, default: i64) -> i64 {
    value.get(key).and_then(Value::as_i64).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_command_results_record_outcomes_and_reject_authority() {
        let mut runtime = DxrSandboxCommandResultRuntime::new();
        let base = r#""command_preflight_observed":true,"session_lease_observed":true,"adapter_result_observed":true,"stdout_stream_observed":true,"stderr_stream_observed":true,"exit_code_observed":true,"duration_observed":true,"output_cap_enforced":true,"artifact_quarantine_observed":true,"log_quarantine_observed":true,"replay_cursor_observed":true,"audit_record_observed":true,"kill_switch_observed":true,"timeout_policy_observed":true,"tenant_fairness_observed":true,"backpressure_observed":true"#;
        let success = runtime
            .submit_json(&format!(r#"{{"idempotency_key":"result-001",{base}}}"#))
            .expect("success result");
        assert!(
            success
                .body
                .contains("\"status\":\"LIVE-LOCAL-DXR-SANDBOX-COMMAND-RESULT-FLOOR\"")
        );
        assert!(
            success
                .body
                .contains("\"result_decision\":\"success_observed_output_quarantined\"")
        );
        assert!(success.body.contains("\"host_process_started\":false"));

        let timeout = runtime
            .submit_json(&format!(
                r#"{{"idempotency_key":"result-002","observed_outcome":"timeout","exit_code":124,"duration_ms":60000,{base}}}"#
            ))
            .expect("timeout result");
        assert!(
            timeout
                .body
                .contains("\"status\":\"DXR_SANDBOX_COMMAND_RESULT_RECORDED_TIMEOUT_QUARANTINED\"")
        );

        let rejected = runtime
            .submit_json(&format!(
                r#"{{"idempotency_key":"result-003","host_process_start_requested":true,"dxr_command_execution_requested":true,{base}}}"#
            ))
            .expect("rejected result");
        assert!(
            rejected
                .body
                .contains("\"status\":\"DXR_SANDBOX_COMMAND_RESULT_REJECTED_SECURITY_BOUNDARY\"")
        );
        assert!(runtime.results_json().contains("\"result_count\":3"));
        assert!(runtime.results_json().contains("\"timeout_count\":1"));
    }
}
