use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_JOB_ID: &str = "dxr_job_dispatch_ready_001";
const DEFAULT_RUN_ID: &str = "dxr_run_dispatch_ready_001";
const DEFAULT_WORKER_RUN_ID: &str = "dxr_worker_run_local_001";
const DEFAULT_COMMAND_RESULT_ID: &str = "dxr_sandbox_command_result_000001";
const DEFAULT_COMMAND_RESULT_HASH: &str = "sha256:dxr_sandbox_command_result_hash";
const DEFAULT_STDOUT_HASH: &str = "sha256:dxr_sandbox_command_stdout_hash";
const DEFAULT_STDERR_HASH: &str = "sha256:dxr_sandbox_command_stderr_hash";
const DEFAULT_ARTIFACT_HASH: &str = "sha256:dxr_sandbox_command_artifact_hash";
const DEFAULT_HARNESS_VERDICT_RECEIPT_ID: &str = "dxr_harness_verdict_000001";
const DEFAULT_REVIEWER_SEPARATION_RECEIPT_ID: &str = "dxr_reviewer_separation_000001";
const DEFAULT_MAX_SUMMARY_BYTES: usize = 16_384;
const DEFAULT_TARGET_ENGINEERS: usize = 1000;
const DEFAULT_PEAK_PARALLEL_FORGE_BUILDS: usize = 5000;
const DEFAULT_CONSUMPTION_BATCH_SIZE: usize = 320;

#[derive(Default)]
pub struct DxrSandboxResultConsumptionRuntime {
    consumptions: Vec<DxrSandboxResultConsumptionRecord>,
    next_consumption: usize,
}

pub struct DxrSandboxResultConsumptionOutcome {
    pub body: String,
    pub events: Vec<DxrSandboxResultConsumptionRuntimeEvent>,
}

pub struct DxrSandboxResultConsumptionRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrSandboxResultConsumptionRecord {
    sequence: usize,
    consumption_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    idempotency_key: String,
    command_result_id: String,
    command_result_terminal_state: String,
    command_result_outcome: String,
    command_result_hash: String,
    stdout_hash: String,
    stderr_hash: String,
    artifact_hash: String,
    harness_verdict_receipt_id: String,
    reviewer_separation_receipt_id: String,
    consumed_result_count: usize,
    failed_result_count: usize,
    timeout_result_count: usize,
    max_summary_bytes: usize,
    observed_summary_bytes: usize,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    consumption_batch_size: usize,
    consumption_batch_count: usize,
    command_result_observed: bool,
    quarantine_retained: bool,
    output_hash_verified: bool,
    artifact_hash_verified: bool,
    replay_cursor_observed: bool,
    audit_record_observed: bool,
    harness_verdict_recorded: bool,
    reviewer_separation_observed: bool,
    failure_triage_observed: bool,
    human_escalation_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    status: String,
    terminal_state: String,
    consumption_decision: String,
    rejected: bool,
    rejection_reason: String,
}

struct SandboxResultConsumptionRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    idempotency_key: String,
    command_result_id: String,
    command_result_terminal_state: String,
    command_result_outcome: String,
    command_result_hash: String,
    stdout_hash: String,
    stderr_hash: String,
    artifact_hash: String,
    harness_verdict_receipt_id: String,
    reviewer_separation_receipt_id: String,
    consumed_result_count: usize,
    failed_result_count: usize,
    timeout_result_count: usize,
    max_summary_bytes: usize,
    observed_summary_bytes: usize,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    consumption_batch_size: usize,
    command_result_observed: bool,
    quarantine_retained: bool,
    output_hash_verified: bool,
    artifact_hash_verified: bool,
    replay_cursor_observed: bool,
    audit_record_observed: bool,
    harness_verdict_recorded: bool,
    reviewer_separation_observed: bool,
    failure_triage_observed: bool,
    human_escalation_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    unquarantined_output_requested: bool,
    ci_claim_requested: bool,
    patch_application_requested: bool,
    shell_execution_requested: bool,
    git_execution_requested: bool,
    network_requested: bool,
    secret_inheritance_requested: bool,
    filesystem_mutation_requested: bool,
    deployment_requested: bool,
    production_write_requested: bool,
}

impl DxrSandboxResultConsumptionRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(
        &mut self,
        body: &str,
    ) -> Result<DxrSandboxResultConsumptionOutcome, String> {
        let request = parse_sandbox_result_consumption_request(body)?;
        self.next_consumption += 1;

        let authority_requested = request.unquarantined_output_requested
            || request.ci_claim_requested
            || request.patch_application_requested
            || request.shell_execution_requested
            || request.git_execution_requested
            || request.network_requested
            || request.secret_inheritance_requested
            || request.filesystem_mutation_requested
            || request.deployment_requested
            || request.production_write_requested;
        let required_evidence = request.required_evidence_observed();
        let shape_valid = request.shape_valid();

        let status = if authority_requested {
            "DXR_SANDBOX_RESULT_CONSUMPTION_REJECTED_SECURITY_BOUNDARY"
        } else if !required_evidence || !shape_valid {
            "DXR_SANDBOX_RESULT_CONSUMPTION_REJECTED_MISSING_EVIDENCE"
        } else {
            match request.command_result_outcome.as_str() {
                "success" => "LIVE-LOCAL-DXR-SANDBOX-RESULT-CONSUMPTION-FLOOR",
                "failure" => "DXR_SANDBOX_RESULT_CONSUMPTION_RECORDED_FAILURE_TRIAGE_READY",
                "timeout" => "DXR_SANDBOX_RESULT_CONSUMPTION_RECORDED_TIMEOUT_TRIAGE_READY",
                _ => "DXR_SANDBOX_RESULT_CONSUMPTION_REJECTED_MISSING_EVIDENCE",
            }
        };
        let terminal_state = match status {
            "LIVE-LOCAL-DXR-SANDBOX-RESULT-CONSUMPTION-FLOOR" => {
                "DXR_SANDBOX_RESULT_CONSUMPTION_RECORDED_VERDICT_READY"
            }
            "DXR_SANDBOX_RESULT_CONSUMPTION_RECORDED_FAILURE_TRIAGE_READY" => {
                "DXR_SANDBOX_RESULT_CONSUMPTION_RECORDED_FAILURE_TRIAGE_READY"
            }
            "DXR_SANDBOX_RESULT_CONSUMPTION_RECORDED_TIMEOUT_TRIAGE_READY" => {
                "DXR_SANDBOX_RESULT_CONSUMPTION_RECORDED_TIMEOUT_TRIAGE_READY"
            }
            "DXR_SANDBOX_RESULT_CONSUMPTION_REJECTED_SECURITY_BOUNDARY" => {
                "DXR_SANDBOX_RESULT_CONSUMPTION_REJECTED_SECURITY_BOUNDARY"
            }
            _ => "DXR_SANDBOX_RESULT_CONSUMPTION_REJECTED_MISSING_EVIDENCE",
        };
        let consumption_decision = match terminal_state {
            "DXR_SANDBOX_RESULT_CONSUMPTION_RECORDED_VERDICT_READY" => {
                "harness_verdict_ready_quarantine_retained"
            }
            "DXR_SANDBOX_RESULT_CONSUMPTION_RECORDED_FAILURE_TRIAGE_READY" => {
                "failure_triage_ready_quarantine_retained"
            }
            "DXR_SANDBOX_RESULT_CONSUMPTION_RECORDED_TIMEOUT_TRIAGE_READY" => {
                "timeout_triage_ready_quarantine_retained"
            }
            "DXR_SANDBOX_RESULT_CONSUMPTION_REJECTED_SECURITY_BOUNDARY" => {
                "rejected_security_boundary"
            }
            _ => "rejected_missing_evidence",
        };
        let rejection_reason = if authority_requested {
            "sandbox_result_consumption_cannot_expose_output_claim_ci_apply_patch_run_shell_git_network_secrets_filesystem_deploy_or_write_production"
        } else if !required_evidence {
            "missing_command_result_quarantine_hash_replay_audit_verdict_reviewer_or_scale_evidence"
        } else if !shape_valid {
            "invalid_sandbox_result_consumption_shape"
        } else {
            ""
        };

        let consumption_batch_count = ceil_div(
            request.peak_parallel_forge_builds,
            request.consumption_batch_size.max(1),
        );
        let record = DxrSandboxResultConsumptionRecord {
            sequence: self.next_consumption,
            consumption_id: format!(
                "dxr_sandbox_result_consumption_{:06}",
                self.next_consumption
            ),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            worker_run_id: request.worker_run_id,
            idempotency_key: request.idempotency_key,
            command_result_id: request.command_result_id,
            command_result_terminal_state: request.command_result_terminal_state,
            command_result_outcome: request.command_result_outcome,
            command_result_hash: request.command_result_hash,
            stdout_hash: request.stdout_hash,
            stderr_hash: request.stderr_hash,
            artifact_hash: request.artifact_hash,
            harness_verdict_receipt_id: request.harness_verdict_receipt_id,
            reviewer_separation_receipt_id: request.reviewer_separation_receipt_id,
            consumed_result_count: request.consumed_result_count,
            failed_result_count: request.failed_result_count,
            timeout_result_count: request.timeout_result_count,
            max_summary_bytes: request.max_summary_bytes,
            observed_summary_bytes: request.observed_summary_bytes,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            consumption_batch_size: request.consumption_batch_size,
            consumption_batch_count,
            command_result_observed: request.command_result_observed,
            quarantine_retained: request.quarantine_retained,
            output_hash_verified: request.output_hash_verified,
            artifact_hash_verified: request.artifact_hash_verified,
            replay_cursor_observed: request.replay_cursor_observed,
            audit_record_observed: request.audit_record_observed,
            harness_verdict_recorded: request.harness_verdict_recorded,
            reviewer_separation_observed: request.reviewer_separation_observed,
            failure_triage_observed: request.failure_triage_observed,
            human_escalation_observed: request.human_escalation_observed,
            tenant_fairness_observed: request.tenant_fairness_observed,
            backpressure_observed: request.backpressure_observed,
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            consumption_decision: consumption_decision.to_string(),
            rejected: status.starts_with("DXR_SANDBOX_RESULT_CONSUMPTION_REJECTED"),
            rejection_reason: rejection_reason.to_string(),
        };
        let events = sandbox_result_consumption_events(&record);
        let body = render_sandbox_result_consumption_response_json(&record);
        self.consumptions.push(record);
        Ok(DxrSandboxResultConsumptionOutcome { body, events })
    }

    pub fn consumptions_json(&self) -> String {
        let verdict_ready_count = self
            .consumptions
            .iter()
            .filter(|consumption| {
                consumption.terminal_state
                    == "DXR_SANDBOX_RESULT_CONSUMPTION_RECORDED_VERDICT_READY"
            })
            .count();
        let failure_triage_count = self
            .consumptions
            .iter()
            .filter(|consumption| {
                consumption.terminal_state
                    == "DXR_SANDBOX_RESULT_CONSUMPTION_RECORDED_FAILURE_TRIAGE_READY"
            })
            .count();
        let timeout_triage_count = self
            .consumptions
            .iter()
            .filter(|consumption| {
                consumption.terminal_state
                    == "DXR_SANDBOX_RESULT_CONSUMPTION_RECORDED_TIMEOUT_TRIAGE_READY"
            })
            .count();
        let rejected_count = self
            .consumptions
            .iter()
            .filter(|consumption| consumption.rejected)
            .count();
        format!(
            r#"{{"name":"mdx-dxr-sandbox-result-consumptions","status":"LIVE-LOCAL-DXR-SANDBOX-RESULT-CONSUMPTION-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/sandbox-result-consumptions.json","submit_route":"/v1/dxr/sandbox-result-consumptions","consumption_count":{},"verdict_ready_count":{},"failure_triage_count":{},"timeout_triage_count":{},"rejected_count":{},"consumption_policy":"quarantined_command_result_metadata_to_harness_verdict_before_ci_or_patch_authority","scale_policy":"tenant_fair_1000_engineers_5000_forge_builds","required_gates":["command_result","quarantine_retained","output_hash","artifact_hash","replay_cursor","audit_record","harness_verdict","reviewer_separation","failure_triage","human_escalation","tenant_fairness","backpressure"],"unquarantined_output_allowed":false,"ci_claim_allowed":false,"patch_application_allowed":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"consumptions":[{}]}}"#,
            self.consumptions.len(),
            verdict_ready_count,
            failure_triage_count,
            timeout_triage_count,
            rejected_count,
            self.consumptions
                .iter()
                .map(render_sandbox_result_consumption_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

impl SandboxResultConsumptionRequest {
    fn required_evidence_observed(&self) -> bool {
        self.command_result_observed
            && self.quarantine_retained
            && self.output_hash_verified
            && self.artifact_hash_verified
            && self.replay_cursor_observed
            && self.audit_record_observed
            && self.harness_verdict_recorded
            && self.reviewer_separation_observed
            && self.failure_triage_observed
            && self.human_escalation_observed
            && self.tenant_fairness_observed
            && self.backpressure_observed
    }

    fn shape_valid(&self) -> bool {
        !self.command_result_id.trim().is_empty()
            && command_result_terminal_state_known(&self.command_result_terminal_state)
            && matches!(
                self.command_result_outcome.as_str(),
                "success" | "failure" | "timeout"
            )
            && self.command_result_hash.starts_with("sha256:")
            && self.stdout_hash.starts_with("sha256:")
            && self.stderr_hash.starts_with("sha256:")
            && self.artifact_hash.starts_with("sha256:")
            && !self.harness_verdict_receipt_id.trim().is_empty()
            && !self.reviewer_separation_receipt_id.trim().is_empty()
            && self.consumed_result_count > 0
            && self.failed_result_count <= self.consumed_result_count
            && self.timeout_result_count <= self.consumed_result_count
            && self.observed_summary_bytes <= self.max_summary_bytes
            && self.max_summary_bytes <= DEFAULT_MAX_SUMMARY_BYTES
            && self.target_concurrent_engineers >= DEFAULT_TARGET_ENGINEERS
            && self.peak_parallel_forge_builds >= DEFAULT_PEAK_PARALLEL_FORGE_BUILDS
            && self.consumption_batch_size > 0
            && self.consumption_batch_size <= self.peak_parallel_forge_builds
    }
}

fn parse_sandbox_result_consumption_request(
    body: &str,
) -> Result<SandboxResultConsumptionRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR sandbox result consumption json: {error}"))?;
    Ok(SandboxResultConsumptionRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", DEFAULT_JOB_ID),
        run_id: string_value(&value, "run_id", DEFAULT_RUN_ID),
        worker_run_id: string_value(&value, "worker_run_id", DEFAULT_WORKER_RUN_ID),
        idempotency_key: string_value(&value, "idempotency_key", "sandbox-result-consumption-001"),
        command_result_id: string_value(&value, "command_result_id", DEFAULT_COMMAND_RESULT_ID),
        command_result_terminal_state: string_value(
            &value,
            "command_result_terminal_state",
            "DXR_SANDBOX_COMMAND_RESULT_RECORDED_SUCCESS_QUARANTINED",
        ),
        command_result_outcome: string_value(&value, "command_result_outcome", "success"),
        command_result_hash: string_value(
            &value,
            "command_result_hash",
            DEFAULT_COMMAND_RESULT_HASH,
        ),
        stdout_hash: string_value(&value, "stdout_hash", DEFAULT_STDOUT_HASH),
        stderr_hash: string_value(&value, "stderr_hash", DEFAULT_STDERR_HASH),
        artifact_hash: string_value(&value, "artifact_hash", DEFAULT_ARTIFACT_HASH),
        harness_verdict_receipt_id: string_value(
            &value,
            "harness_verdict_receipt_id",
            DEFAULT_HARNESS_VERDICT_RECEIPT_ID,
        ),
        reviewer_separation_receipt_id: string_value(
            &value,
            "reviewer_separation_receipt_id",
            DEFAULT_REVIEWER_SEPARATION_RECEIPT_ID,
        ),
        consumed_result_count: usize_value(&value, "consumed_result_count", 1),
        failed_result_count: usize_value(&value, "failed_result_count", 0),
        timeout_result_count: usize_value(&value, "timeout_result_count", 0),
        max_summary_bytes: usize_value(&value, "max_summary_bytes", DEFAULT_MAX_SUMMARY_BYTES),
        observed_summary_bytes: usize_value(&value, "observed_summary_bytes", 2048),
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
        consumption_batch_size: usize_value(
            &value,
            "consumption_batch_size",
            DEFAULT_CONSUMPTION_BATCH_SIZE,
        ),
        command_result_observed: bool_value(&value, "command_result_observed", false),
        quarantine_retained: bool_value(&value, "quarantine_retained", false),
        output_hash_verified: bool_value(&value, "output_hash_verified", false),
        artifact_hash_verified: bool_value(&value, "artifact_hash_verified", false),
        replay_cursor_observed: bool_value(&value, "replay_cursor_observed", false),
        audit_record_observed: bool_value(&value, "audit_record_observed", false),
        harness_verdict_recorded: bool_value(&value, "harness_verdict_recorded", false),
        reviewer_separation_observed: bool_value(&value, "reviewer_separation_observed", false),
        failure_triage_observed: bool_value(&value, "failure_triage_observed", false),
        human_escalation_observed: bool_value(&value, "human_escalation_observed", false),
        tenant_fairness_observed: bool_value(&value, "tenant_fairness_observed", false),
        backpressure_observed: bool_value(&value, "backpressure_observed", false),
        unquarantined_output_requested: bool_value(&value, "unquarantined_output_requested", false),
        ci_claim_requested: bool_value(&value, "ci_claim_requested", false),
        patch_application_requested: bool_value(&value, "patch_application_requested", false),
        shell_execution_requested: bool_value(&value, "shell_execution_requested", false),
        git_execution_requested: bool_value(&value, "git_execution_requested", false),
        network_requested: bool_value(&value, "network_requested", false),
        secret_inheritance_requested: bool_value(&value, "secret_inheritance_requested", false),
        filesystem_mutation_requested: bool_value(&value, "filesystem_mutation_requested", false),
        deployment_requested: bool_value(&value, "deployment_requested", false),
        production_write_requested: bool_value(&value, "production_write_requested", false),
    })
}

fn sandbox_result_consumption_events(
    consumption: &DxrSandboxResultConsumptionRecord,
) -> Vec<DxrSandboxResultConsumptionRuntimeEvent> {
    let mut event_types = vec!["sandbox_result_consumption_recorded"];
    if consumption.rejected {
        event_types.push("sandbox_result_consumption_rejected");
    } else {
        event_types.push("sandbox_result_command_result_consumed");
        event_types.push("sandbox_result_quarantine_retained");
        event_types.push("sandbox_result_hashes_verified");
        event_types.push("sandbox_result_harness_verdict_recorded");
        match consumption.command_result_outcome.as_str() {
            "success" => event_types.push("sandbox_result_verdict_ready"),
            "failure" => event_types.push("sandbox_result_failure_triage_ready"),
            "timeout" => event_types.push("sandbox_result_timeout_triage_ready"),
            _ => event_types.push("sandbox_result_consumption_rejected"),
        }
        event_types.push("sandbox_result_reviewer_separation_observed");
        event_types.push("sandbox_result_consumption_authority_blocked");
    }
    event_types
        .iter()
        .map(|event_type| DxrSandboxResultConsumptionRuntimeEvent {
            event_type: (*event_type).to_string(),
            tenant_id: consumption.tenant_id.clone(),
            job_id: consumption.job_id.clone(),
            run_id: consumption.run_id.clone(),
            actor_id: consumption.actor_id.clone(),
        })
        .collect()
}

fn render_sandbox_result_consumption_response_json(
    consumption: &DxrSandboxResultConsumptionRecord,
) -> String {
    format!(
        r#"{{"name":"mdx-dxr-sandbox-result-consumption","status":{},"runtime":"mdx-dxr-engine","consumption":{},"consumption_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"command_result_id":{},"terminal_state":{},"consumption_decision":{},"rejected":{},"rejection_reason":{},"unquarantined_output_allowed":false,"ci_claim_allowed":false,"patch_application_allowed":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&consumption.status),
        render_sandbox_result_consumption_json(consumption),
        json_string_literal(&consumption.consumption_id),
        json_string_literal(&consumption.tenant_id),
        json_string_literal(&consumption.actor_id),
        json_string_literal(&consumption.job_id),
        json_string_literal(&consumption.run_id),
        json_string_literal(&consumption.worker_run_id),
        json_string_literal(&consumption.command_result_id),
        json_string_literal(&consumption.terminal_state),
        json_string_literal(&consumption.consumption_decision),
        consumption.rejected,
        json_string_literal(&consumption.rejection_reason)
    )
}

fn render_sandbox_result_consumption_json(
    consumption: &DxrSandboxResultConsumptionRecord,
) -> String {
    format!(
        r#"{{"sequence":{},"consumption_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"idempotency_key":{},"command_result_id":{},"command_result_terminal_state":{},"command_result_outcome":{},"command_result_hash":{},"stdout_hash":{},"stderr_hash":{},"artifact_hash":{},"harness_verdict_receipt_id":{},"reviewer_separation_receipt_id":{},"consumed_result_count":{},"failed_result_count":{},"timeout_result_count":{},"max_summary_bytes":{},"observed_summary_bytes":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"consumption_batch_size":{},"consumption_batch_count":{},"command_result_observed":{},"quarantine_retained":{},"output_hash_verified":{},"artifact_hash_verified":{},"replay_cursor_observed":{},"audit_record_observed":{},"harness_verdict_recorded":{},"reviewer_separation_observed":{},"failure_triage_observed":{},"human_escalation_observed":{},"tenant_fairness_observed":{},"backpressure_observed":{},"status":{},"terminal_state":{},"consumption_decision":{},"rejected":{},"rejection_reason":{},"unquarantined_output_allowed":false,"ci_claim_allowed":false,"patch_application_allowed":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        consumption.sequence,
        json_string_literal(&consumption.consumption_id),
        json_string_literal(&consumption.tenant_id),
        json_string_literal(&consumption.actor_id),
        json_string_literal(&consumption.job_id),
        json_string_literal(&consumption.run_id),
        json_string_literal(&consumption.worker_run_id),
        json_string_literal(&consumption.idempotency_key),
        json_string_literal(&consumption.command_result_id),
        json_string_literal(&consumption.command_result_terminal_state),
        json_string_literal(&consumption.command_result_outcome),
        json_string_literal(&consumption.command_result_hash),
        json_string_literal(&consumption.stdout_hash),
        json_string_literal(&consumption.stderr_hash),
        json_string_literal(&consumption.artifact_hash),
        json_string_literal(&consumption.harness_verdict_receipt_id),
        json_string_literal(&consumption.reviewer_separation_receipt_id),
        consumption.consumed_result_count,
        consumption.failed_result_count,
        consumption.timeout_result_count,
        consumption.max_summary_bytes,
        consumption.observed_summary_bytes,
        consumption.target_concurrent_engineers,
        consumption.peak_parallel_forge_builds,
        consumption.consumption_batch_size,
        consumption.consumption_batch_count,
        consumption.command_result_observed,
        consumption.quarantine_retained,
        consumption.output_hash_verified,
        consumption.artifact_hash_verified,
        consumption.replay_cursor_observed,
        consumption.audit_record_observed,
        consumption.harness_verdict_recorded,
        consumption.reviewer_separation_observed,
        consumption.failure_triage_observed,
        consumption.human_escalation_observed,
        consumption.tenant_fairness_observed,
        consumption.backpressure_observed,
        json_string_literal(&consumption.status),
        json_string_literal(&consumption.terminal_state),
        json_string_literal(&consumption.consumption_decision),
        consumption.rejected,
        json_string_literal(&consumption.rejection_reason)
    )
}

fn command_result_terminal_state_known(state: &str) -> bool {
    matches!(
        state,
        "DXR_SANDBOX_COMMAND_RESULT_RECORDED_SUCCESS_QUARANTINED"
            | "DXR_SANDBOX_COMMAND_RESULT_RECORDED_FAILURE_QUARANTINED"
            | "DXR_SANDBOX_COMMAND_RESULT_RECORDED_TIMEOUT_QUARANTINED"
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
    fn sandbox_result_consumption_records_verdict_triage_and_rejects_authority() {
        let mut runtime = DxrSandboxResultConsumptionRuntime::new();
        let base = r#""command_result_observed":true,"quarantine_retained":true,"output_hash_verified":true,"artifact_hash_verified":true,"replay_cursor_observed":true,"audit_record_observed":true,"harness_verdict_recorded":true,"reviewer_separation_observed":true,"failure_triage_observed":true,"human_escalation_observed":true,"tenant_fairness_observed":true,"backpressure_observed":true"#;
        let accepted = runtime
            .submit_json(&format!(r#"{{"idempotency_key":"consume-001",{base}}}"#))
            .expect("accepted consumption");
        assert!(
            accepted
                .body
                .contains("\"status\":\"LIVE-LOCAL-DXR-SANDBOX-RESULT-CONSUMPTION-FLOOR\"")
        );
        assert!(
            accepted
                .body
                .contains("\"consumption_decision\":\"harness_verdict_ready_quarantine_retained\"")
        );
        assert!(accepted.body.contains("\"ci_claim_allowed\":false"));

        let failure = runtime
            .submit_json(&format!(
                r#"{{"idempotency_key":"consume-002","command_result_outcome":"failure","command_result_terminal_state":"DXR_SANDBOX_COMMAND_RESULT_RECORDED_FAILURE_QUARANTINED","failed_result_count":1,{base}}}"#
            ))
            .expect("failure triage");
        assert!(failure.body.contains(
            "\"status\":\"DXR_SANDBOX_RESULT_CONSUMPTION_RECORDED_FAILURE_TRIAGE_READY\""
        ));

        let rejected = runtime
            .submit_json(&format!(
                r#"{{"idempotency_key":"consume-003","ci_claim_requested":true,"patch_application_requested":true,{base}}}"#
            ))
            .expect("rejected consumption");
        assert!(
            rejected.body.contains(
                "\"status\":\"DXR_SANDBOX_RESULT_CONSUMPTION_REJECTED_SECURITY_BOUNDARY\""
            )
        );
        assert!(
            runtime
                .consumptions_json()
                .contains("\"consumption_count\":3")
        );
        assert!(
            runtime
                .consumptions_json()
                .contains("\"failure_triage_count\":1")
        );
        assert!(runtime.consumptions_json().contains("\"rejected_count\":1"));
    }
}
