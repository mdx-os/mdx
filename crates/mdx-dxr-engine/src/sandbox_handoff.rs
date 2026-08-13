use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_TARGET_ENGINEERS: usize = 1000;
const DEFAULT_PEAK_PARALLEL_FORGE_BUILDS: usize = 5000;
const DEFAULT_SCHEDULED_BUILD_COUNT: usize = 5000;
const DEFAULT_SCHEDULABLE_BUILD_COUNT: usize = 2048;
const DEFAULT_RETAINED_BACKPRESSURE_COUNT: usize = 2952;
const DEFAULT_DRIVER_BATCH_SIZE: usize = 320;
const DEFAULT_MAX_RUNTIME_MS: usize = 300_000;
const DEFAULT_HEARTBEAT_INTERVAL_MS: usize = 5_000;
const DEFAULT_KILL_SWITCH_SWEEP_INTERVAL_MS: usize = 2_500;

#[derive(Default)]
pub struct DxrSandboxHandoffRuntime {
    handoffs: Vec<DxrSandboxHandoff>,
    next_handoff: usize,
}

pub struct DxrSandboxHandoffResult {
    pub body: String,
    pub events: Vec<DxrSandboxHandoffRuntimeEvent>,
}

pub struct DxrSandboxHandoffRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrSandboxHandoff {
    sequence: usize,
    handoff_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    idempotency_key: String,
    sandbox_authority_envelope_id: String,
    sandbox_authority_receipt_id: String,
    sandbox_authority_status: String,
    execution_admission_id: String,
    execution_admission_status: String,
    execution_schedule_id: String,
    execution_supervision_id: String,
    worker_driver_run_id: String,
    dispatch_claim_id: String,
    heartbeat_receipt_id: String,
    durable_workflow_receipt_id: String,
    external_sandbox_preflight_id: String,
    sandbox_adapter_turn_on_receipt_id: String,
    ctx_context_input_id: String,
    tool_policy_receipt_id: String,
    provider_failover_proof_id: String,
    harness_verdict_receipt_id: String,
    reviewer_separation_receipt_id: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    scheduled_build_count: usize,
    schedulable_build_count: usize,
    retained_backpressure_count: usize,
    driver_batch_size: usize,
    max_runtime_ms: usize,
    heartbeat_interval_ms: usize,
    kill_switch_sweep_interval_ms: usize,
    launch_window_count: usize,
    quarantine_artifact_count: usize,
    sandbox_authority_observed: bool,
    execution_admission_observed: bool,
    execution_schedule_observed: bool,
    execution_supervision_observed: bool,
    worker_driver_observed: bool,
    dispatch_claim_observed: bool,
    heartbeat_observed: bool,
    durable_workflow_observed: bool,
    external_sandbox_preflight_observed: bool,
    sandbox_adapter_turn_on_observed: bool,
    ctx_context_observed: bool,
    tool_policy_observed: bool,
    provider_failover_observed: bool,
    harness_verdict_observed: bool,
    reviewer_separation_observed: bool,
    event_relay_observed: bool,
    reconnect_replay_observed: bool,
    durable_restart_replay_observed: bool,
    evidence_chain_observed: bool,
    quarantine_observed: bool,
    artifact_sink_observed: bool,
    log_stream_observed: bool,
    kill_switch_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    performance_budget_observed: bool,
    status: String,
    terminal_state: String,
    handoff_decision: String,
    rejected: bool,
    rejection_reason: String,
}

struct SandboxHandoffRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    idempotency_key: String,
    sandbox_authority_envelope_id: String,
    sandbox_authority_receipt_id: String,
    sandbox_authority_status: String,
    execution_admission_id: String,
    execution_admission_status: String,
    execution_schedule_id: String,
    execution_supervision_id: String,
    worker_driver_run_id: String,
    dispatch_claim_id: String,
    heartbeat_receipt_id: String,
    durable_workflow_receipt_id: String,
    external_sandbox_preflight_id: String,
    sandbox_adapter_turn_on_receipt_id: String,
    ctx_context_input_id: String,
    tool_policy_receipt_id: String,
    provider_failover_proof_id: String,
    harness_verdict_receipt_id: String,
    reviewer_separation_receipt_id: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    scheduled_build_count: usize,
    schedulable_build_count: usize,
    retained_backpressure_count: usize,
    driver_batch_size: usize,
    max_runtime_ms: usize,
    heartbeat_interval_ms: usize,
    kill_switch_sweep_interval_ms: usize,
    sandbox_authority_observed: bool,
    execution_admission_observed: bool,
    execution_schedule_observed: bool,
    execution_supervision_observed: bool,
    worker_driver_observed: bool,
    dispatch_claim_observed: bool,
    heartbeat_observed: bool,
    durable_workflow_observed: bool,
    external_sandbox_preflight_observed: bool,
    sandbox_adapter_turn_on_observed: bool,
    ctx_context_observed: bool,
    tool_policy_observed: bool,
    provider_failover_observed: bool,
    harness_verdict_observed: bool,
    reviewer_separation_observed: bool,
    event_relay_observed: bool,
    reconnect_replay_observed: bool,
    durable_restart_replay_observed: bool,
    evidence_chain_observed: bool,
    quarantine_observed: bool,
    artifact_sink_observed: bool,
    log_stream_observed: bool,
    kill_switch_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    performance_budget_observed: bool,
    worker_process_start_requested: bool,
    sandbox_process_start_requested: bool,
    external_repo_checkout_requested: bool,
    provider_call_requested: bool,
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

impl DxrSandboxHandoffRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrSandboxHandoffResult, String> {
        let request = parse_sandbox_handoff_request(body)?;
        self.next_handoff += 1;

        let authority_requested = request.worker_process_start_requested
            || request.sandbox_process_start_requested
            || request.external_repo_checkout_requested
            || request.provider_call_requested
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
        let missing_evidence = request.idempotency_key.trim().is_empty()
            || request.sandbox_authority_envelope_id.trim().is_empty()
            || request.sandbox_authority_receipt_id.trim().is_empty()
            || request.sandbox_authority_status.trim().is_empty()
            || request.sandbox_authority_status.contains("REJECTED")
            || request.execution_admission_id.trim().is_empty()
            || request.execution_admission_status.trim().is_empty()
            || request.execution_schedule_id.trim().is_empty()
            || request.execution_supervision_id.trim().is_empty()
            || request.worker_driver_run_id.trim().is_empty()
            || request.dispatch_claim_id.trim().is_empty()
            || request.heartbeat_receipt_id.trim().is_empty()
            || request.durable_workflow_receipt_id.trim().is_empty()
            || request.external_sandbox_preflight_id.trim().is_empty()
            || request.sandbox_adapter_turn_on_receipt_id.trim().is_empty()
            || request.ctx_context_input_id.trim().is_empty()
            || request.tool_policy_receipt_id.trim().is_empty()
            || request.provider_failover_proof_id.trim().is_empty()
            || request.harness_verdict_receipt_id.trim().is_empty()
            || request.reviewer_separation_receipt_id.trim().is_empty()
            || request.scheduled_build_count == 0
            || request.schedulable_build_count == 0
            || request.driver_batch_size == 0
            || request.max_runtime_ms == 0
            || request.heartbeat_interval_ms == 0
            || request.kill_switch_sweep_interval_ms == 0
            || !request.sandbox_authority_observed
            || !request.execution_admission_observed
            || !request.execution_schedule_observed
            || !request.execution_supervision_observed
            || !request.worker_driver_observed
            || !request.dispatch_claim_observed
            || !request.heartbeat_observed
            || !request.durable_workflow_observed
            || !request.external_sandbox_preflight_observed
            || !request.sandbox_adapter_turn_on_observed
            || !request.ctx_context_observed
            || !request.tool_policy_observed
            || !request.provider_failover_observed
            || !request.harness_verdict_observed
            || !request.reviewer_separation_observed
            || !request.event_relay_observed
            || !request.reconnect_replay_observed
            || !request.durable_restart_replay_observed
            || !request.evidence_chain_observed
            || !request.quarantine_observed
            || !request.artifact_sink_observed
            || !request.log_stream_observed
            || !request.kill_switch_observed
            || !request.tenant_fairness_observed
            || !request.backpressure_observed
            || !request.performance_budget_observed;
        let launch_window_count = ceil_div(
            request.schedulable_build_count,
            request.driver_batch_size.max(1),
        );
        let quarantine_artifact_count = request.schedulable_build_count
            + request
                .retained_backpressure_count
                .min(request.scheduled_build_count);

        let (status, terminal_state, handoff_decision, rejected, rejection_reason) =
            if authority_requested {
                (
                    "DXR_SUPERVISED_SANDBOX_HANDOFF_REJECTED_SECURITY_BOUNDARY",
                    "DXR_SUPERVISED_SANDBOX_HANDOFF_RECORDED_EXECUTION_BLOCKED",
                    "rejected_execution_authority_requested",
                    true,
                    "handoff_cannot_start_worker_sandbox_checkout_provider_tool_shell_patch_git_network_secret_filesystem_ci_deployment_or_production_writes",
                )
            } else if missing_evidence {
                (
                    "DXR_SUPERVISED_SANDBOX_HANDOFF_REJECTED_MISSING_EVIDENCE",
                    "DXR_SUPERVISED_SANDBOX_HANDOFF_RECORDED_EXECUTION_BLOCKED",
                    "rejected_missing_handoff_evidence",
                    true,
                    "authority_execution_schedule_supervision_driver_policy_quarantine_relay_or_budget_evidence_missing",
                )
            } else if request.retained_backpressure_count > 0 {
                (
                    "LIVE-LOCAL-DXR-SUPERVISED-SANDBOX-HANDOFF-FLOOR",
                    "DXR_SUPERVISED_SANDBOX_HANDOFF_RECORDED_BACKPRESSURE_RETAINED",
                    "supervised_handoff_backpressure_retained_execution_blocked",
                    false,
                    "none",
                )
            } else {
                (
                    "LIVE-LOCAL-DXR-SUPERVISED-SANDBOX-HANDOFF-FLOOR",
                    "DXR_SUPERVISED_SANDBOX_HANDOFF_RECORDED_UNDER_CAPACITY",
                    "supervised_handoff_under_capacity_execution_blocked",
                    false,
                    "none",
                )
            };

        let handoff = DxrSandboxHandoff {
            sequence: self.next_handoff,
            handoff_id: format!("dxr_supervised_sandbox_handoff_{:06}", self.next_handoff),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            worker_run_id: request.worker_run_id,
            idempotency_key: request.idempotency_key,
            sandbox_authority_envelope_id: request.sandbox_authority_envelope_id,
            sandbox_authority_receipt_id: request.sandbox_authority_receipt_id,
            sandbox_authority_status: request.sandbox_authority_status,
            execution_admission_id: request.execution_admission_id,
            execution_admission_status: request.execution_admission_status,
            execution_schedule_id: request.execution_schedule_id,
            execution_supervision_id: request.execution_supervision_id,
            worker_driver_run_id: request.worker_driver_run_id,
            dispatch_claim_id: request.dispatch_claim_id,
            heartbeat_receipt_id: request.heartbeat_receipt_id,
            durable_workflow_receipt_id: request.durable_workflow_receipt_id,
            external_sandbox_preflight_id: request.external_sandbox_preflight_id,
            sandbox_adapter_turn_on_receipt_id: request.sandbox_adapter_turn_on_receipt_id,
            ctx_context_input_id: request.ctx_context_input_id,
            tool_policy_receipt_id: request.tool_policy_receipt_id,
            provider_failover_proof_id: request.provider_failover_proof_id,
            harness_verdict_receipt_id: request.harness_verdict_receipt_id,
            reviewer_separation_receipt_id: request.reviewer_separation_receipt_id,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            scheduled_build_count: request.scheduled_build_count,
            schedulable_build_count: request.schedulable_build_count,
            retained_backpressure_count: request.retained_backpressure_count,
            driver_batch_size: request.driver_batch_size,
            max_runtime_ms: request.max_runtime_ms,
            heartbeat_interval_ms: request.heartbeat_interval_ms,
            kill_switch_sweep_interval_ms: request.kill_switch_sweep_interval_ms,
            launch_window_count,
            quarantine_artifact_count,
            sandbox_authority_observed: request.sandbox_authority_observed,
            execution_admission_observed: request.execution_admission_observed,
            execution_schedule_observed: request.execution_schedule_observed,
            execution_supervision_observed: request.execution_supervision_observed,
            worker_driver_observed: request.worker_driver_observed,
            dispatch_claim_observed: request.dispatch_claim_observed,
            heartbeat_observed: request.heartbeat_observed,
            durable_workflow_observed: request.durable_workflow_observed,
            external_sandbox_preflight_observed: request.external_sandbox_preflight_observed,
            sandbox_adapter_turn_on_observed: request.sandbox_adapter_turn_on_observed,
            ctx_context_observed: request.ctx_context_observed,
            tool_policy_observed: request.tool_policy_observed,
            provider_failover_observed: request.provider_failover_observed,
            harness_verdict_observed: request.harness_verdict_observed,
            reviewer_separation_observed: request.reviewer_separation_observed,
            event_relay_observed: request.event_relay_observed,
            reconnect_replay_observed: request.reconnect_replay_observed,
            durable_restart_replay_observed: request.durable_restart_replay_observed,
            evidence_chain_observed: request.evidence_chain_observed,
            quarantine_observed: request.quarantine_observed,
            artifact_sink_observed: request.artifact_sink_observed,
            log_stream_observed: request.log_stream_observed,
            kill_switch_observed: request.kill_switch_observed,
            tenant_fairness_observed: request.tenant_fairness_observed,
            backpressure_observed: request.backpressure_observed,
            performance_budget_observed: request.performance_budget_observed,
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            handoff_decision: handoff_decision.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let events = sandbox_handoff_events(&handoff);
        let body = render_sandbox_handoff_response_json(&handoff);
        self.handoffs.push(handoff);
        Ok(DxrSandboxHandoffResult { body, events })
    }

    pub fn handoffs_json(&self) -> String {
        let staged_count = self
            .handoffs
            .iter()
            .filter(|handoff| !handoff.rejected)
            .count();
        let rejected_count = self.handoffs.len().saturating_sub(staged_count);
        let backpressure_count = self
            .handoffs
            .iter()
            .filter(|handoff| handoff.retained_backpressure_count > 0 && !handoff.rejected)
            .count();
        format!(
            r#"{{"name":"mdx-dxr-supervised-sandbox-handoffs","status":"LIVE-LOCAL-DXR-SUPERVISED-SANDBOX-HANDOFF-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/supervised-sandbox-handoffs.json","submit_route":"/v1/dxr/supervised-sandbox-handoffs","handoff_count":{},"staged_count":{},"backpressure_count":{},"rejected_count":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"scheduled_build_count":{},"schedulable_build_count":{},"retained_backpressure_count":{},"launch_window_count":{},"quarantine_artifact_count":{},"handoff_policy":"supervised_handoff_before_sandbox_process_start","sandbox_authority_envelope_required":true,"execution_admission_required":true,"execution_schedule_required":true,"execution_supervision_required":true,"worker_driver_required":true,"dispatch_claim_required":true,"heartbeat_required":true,"durable_workflow_required":true,"external_sandbox_preflight_required":true,"sandbox_adapter_turn_on_required":true,"ctx_context_required":true,"tool_policy_required":true,"provider_failover_required":true,"harness_verdict_required":true,"reviewer_separation_required":true,"event_relay_required":true,"reconnect_replay_required":true,"durable_restart_replay_required":true,"evidence_chain_required":true,"quarantine_required":true,"artifact_sink_required":true,"log_stream_required":true,"kill_switch_required":true,"tenant_fairness_required":true,"backpressure_required":true,"performance_budget_required":true,"worker_process_started":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"handoffs":[{}]}}"#,
            self.handoffs.len(),
            staged_count,
            backpressure_count,
            rejected_count,
            self.handoffs
                .last()
                .map(|handoff| handoff.target_concurrent_engineers)
                .unwrap_or(DEFAULT_TARGET_ENGINEERS),
            self.handoffs
                .last()
                .map(|handoff| handoff.peak_parallel_forge_builds)
                .unwrap_or(DEFAULT_PEAK_PARALLEL_FORGE_BUILDS),
            self.handoffs
                .last()
                .map(|handoff| handoff.scheduled_build_count)
                .unwrap_or(DEFAULT_SCHEDULED_BUILD_COUNT),
            self.handoffs
                .last()
                .map(|handoff| handoff.schedulable_build_count)
                .unwrap_or(DEFAULT_SCHEDULABLE_BUILD_COUNT),
            self.handoffs
                .last()
                .map(|handoff| handoff.retained_backpressure_count)
                .unwrap_or(DEFAULT_RETAINED_BACKPRESSURE_COUNT),
            self.handoffs
                .last()
                .map(|handoff| handoff.launch_window_count)
                .unwrap_or(ceil_div(
                    DEFAULT_SCHEDULABLE_BUILD_COUNT,
                    DEFAULT_DRIVER_BATCH_SIZE
                )),
            self.handoffs
                .last()
                .map(|handoff| handoff.quarantine_artifact_count)
                .unwrap_or(DEFAULT_SCHEDULED_BUILD_COUNT),
            self.handoffs
                .iter()
                .map(render_sandbox_handoff_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn parse_sandbox_handoff_request(body: &str) -> Result<SandboxHandoffRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR sandbox handoff json: {error}"))?;
    Ok(SandboxHandoffRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", "dxr_job_sandbox_handoff_001"),
        run_id: string_value(&value, "run_id", "dxr_run_sandbox_handoff_001"),
        worker_run_id: string_value(&value, "worker_run_id", "dxr_worker_run_handoff_001"),
        idempotency_key: string_value(&value, "idempotency_key", ""),
        sandbox_authority_envelope_id: string_value(&value, "sandbox_authority_envelope_id", ""),
        sandbox_authority_receipt_id: string_value(&value, "sandbox_authority_receipt_id", ""),
        sandbox_authority_status: string_value(&value, "sandbox_authority_status", ""),
        execution_admission_id: string_value(&value, "execution_admission_id", ""),
        execution_admission_status: string_value(&value, "execution_admission_status", ""),
        execution_schedule_id: string_value(&value, "execution_schedule_id", ""),
        execution_supervision_id: string_value(&value, "execution_supervision_id", ""),
        worker_driver_run_id: string_value(&value, "worker_driver_run_id", ""),
        dispatch_claim_id: string_value(&value, "dispatch_claim_id", ""),
        heartbeat_receipt_id: string_value(&value, "heartbeat_receipt_id", ""),
        durable_workflow_receipt_id: string_value(&value, "durable_workflow_receipt_id", ""),
        external_sandbox_preflight_id: string_value(&value, "external_sandbox_preflight_id", ""),
        sandbox_adapter_turn_on_receipt_id: string_value(
            &value,
            "sandbox_adapter_turn_on_receipt_id",
            "",
        ),
        ctx_context_input_id: string_value(&value, "ctx_context_input_id", ""),
        tool_policy_receipt_id: string_value(&value, "tool_policy_receipt_id", ""),
        provider_failover_proof_id: string_value(&value, "provider_failover_proof_id", ""),
        harness_verdict_receipt_id: string_value(&value, "harness_verdict_receipt_id", ""),
        reviewer_separation_receipt_id: string_value(&value, "reviewer_separation_receipt_id", ""),
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
        scheduled_build_count: usize_value(
            &value,
            "scheduled_build_count",
            DEFAULT_SCHEDULED_BUILD_COUNT,
        ),
        schedulable_build_count: usize_value(
            &value,
            "schedulable_build_count",
            DEFAULT_SCHEDULABLE_BUILD_COUNT,
        ),
        retained_backpressure_count: usize_value(
            &value,
            "retained_backpressure_count",
            DEFAULT_RETAINED_BACKPRESSURE_COUNT,
        ),
        driver_batch_size: usize_value(&value, "driver_batch_size", DEFAULT_DRIVER_BATCH_SIZE),
        max_runtime_ms: usize_value(&value, "max_runtime_ms", DEFAULT_MAX_RUNTIME_MS),
        heartbeat_interval_ms: usize_value(
            &value,
            "heartbeat_interval_ms",
            DEFAULT_HEARTBEAT_INTERVAL_MS,
        ),
        kill_switch_sweep_interval_ms: usize_value(
            &value,
            "kill_switch_sweep_interval_ms",
            DEFAULT_KILL_SWITCH_SWEEP_INTERVAL_MS,
        ),
        sandbox_authority_observed: bool_value(&value, "sandbox_authority_observed", false),
        execution_admission_observed: bool_value(&value, "execution_admission_observed", false),
        execution_schedule_observed: bool_value(&value, "execution_schedule_observed", false),
        execution_supervision_observed: bool_value(&value, "execution_supervision_observed", false),
        worker_driver_observed: bool_value(&value, "worker_driver_observed", false),
        dispatch_claim_observed: bool_value(&value, "dispatch_claim_observed", false),
        heartbeat_observed: bool_value(&value, "heartbeat_observed", false),
        durable_workflow_observed: bool_value(&value, "durable_workflow_observed", false),
        external_sandbox_preflight_observed: bool_value(
            &value,
            "external_sandbox_preflight_observed",
            false,
        ),
        sandbox_adapter_turn_on_observed: bool_value(
            &value,
            "sandbox_adapter_turn_on_observed",
            false,
        ),
        ctx_context_observed: bool_value(&value, "ctx_context_observed", false),
        tool_policy_observed: bool_value(&value, "tool_policy_observed", false),
        provider_failover_observed: bool_value(&value, "provider_failover_observed", false),
        harness_verdict_observed: bool_value(&value, "harness_verdict_observed", false),
        reviewer_separation_observed: bool_value(&value, "reviewer_separation_observed", false),
        event_relay_observed: bool_value(&value, "event_relay_observed", false),
        reconnect_replay_observed: bool_value(&value, "reconnect_replay_observed", false),
        durable_restart_replay_observed: bool_value(
            &value,
            "durable_restart_replay_observed",
            false,
        ),
        evidence_chain_observed: bool_value(&value, "evidence_chain_observed", false),
        quarantine_observed: bool_value(&value, "quarantine_observed", false),
        artifact_sink_observed: bool_value(&value, "artifact_sink_observed", false),
        log_stream_observed: bool_value(&value, "log_stream_observed", false),
        kill_switch_observed: bool_value(&value, "kill_switch_observed", false),
        tenant_fairness_observed: bool_value(&value, "tenant_fairness_observed", false),
        backpressure_observed: bool_value(&value, "backpressure_observed", false),
        performance_budget_observed: bool_value(&value, "performance_budget_observed", false),
        worker_process_start_requested: bool_value(&value, "worker_process_start_requested", false),
        sandbox_process_start_requested: bool_value(
            &value,
            "sandbox_process_start_requested",
            false,
        ),
        external_repo_checkout_requested: bool_value(
            &value,
            "external_repo_checkout_requested",
            false,
        ),
        provider_call_requested: bool_value(&value, "provider_call_requested", false),
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

fn sandbox_handoff_events(handoff: &DxrSandboxHandoff) -> Vec<DxrSandboxHandoffRuntimeEvent> {
    let mut events = vec![sandbox_handoff_event(
        handoff,
        "supervised_sandbox_handoff_recorded",
    )];
    if handoff.rejected {
        events.push(sandbox_handoff_event(
            handoff,
            "supervised_sandbox_handoff_rejected",
        ));
    } else {
        for event_type in [
            "supervised_sandbox_handoff_authority_envelope_bound",
            "supervised_sandbox_handoff_schedule_bound",
            "supervised_sandbox_handoff_supervision_bound",
            "supervised_sandbox_handoff_driver_bound",
            "supervised_sandbox_handoff_quarantine_bound",
            "supervised_sandbox_handoff_kill_switch_armed",
        ] {
            events.push(sandbox_handoff_event(handoff, event_type));
        }
        if handoff.retained_backpressure_count > 0 {
            events.push(sandbox_handoff_event(
                handoff,
                "supervised_sandbox_handoff_backpressure_retained",
            ));
        }
    }
    events.push(sandbox_handoff_event(
        handoff,
        "supervised_sandbox_execution_blocked",
    ));
    events
}

fn sandbox_handoff_event(
    handoff: &DxrSandboxHandoff,
    event_type: &str,
) -> DxrSandboxHandoffRuntimeEvent {
    DxrSandboxHandoffRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: handoff.tenant_id.clone(),
        job_id: handoff.job_id.clone(),
        run_id: handoff.run_id.clone(),
        actor_id: handoff.actor_id.clone(),
    }
}

fn render_sandbox_handoff_response_json(handoff: &DxrSandboxHandoff) -> String {
    format!(
        r#"{{"name":"mdx-dxr-supervised-sandbox-handoff","status":{},"runtime":"mdx-dxr-engine","handoff":{},"handoff_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"terminal_state":{},"handoff_decision":{},"v1_stage":"supervised_sandbox_handoff","sandbox_authority_envelope_required":true,"execution_admission_required":true,"execution_schedule_required":true,"execution_supervision_required":true,"worker_driver_required":true,"dispatch_claim_required":true,"heartbeat_required":true,"durable_workflow_required":true,"external_sandbox_preflight_required":true,"sandbox_adapter_turn_on_required":true,"ctx_context_required":true,"tool_policy_required":true,"provider_failover_required":true,"harness_verdict_required":true,"reviewer_separation_required":true,"event_relay_required":true,"reconnect_replay_required":true,"durable_restart_replay_required":true,"evidence_chain_required":true,"quarantine_required":true,"artifact_sink_required":true,"log_stream_required":true,"kill_switch_required":true,"tenant_fairness_required":true,"backpressure_required":true,"performance_budget_required":true,"sandbox_authority_observed":{},"execution_admission_observed":{},"execution_schedule_observed":{},"execution_supervision_observed":{},"worker_driver_observed":{},"event_relay_observed":{},"durable_restart_replay_observed":{},"quarantine_observed":{},"kill_switch_observed":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"scheduled_build_count":{},"schedulable_build_count":{},"retained_backpressure_count":{},"launch_window_count":{},"quarantine_artifact_count":{},"rejected":{},"rejection_reason":{},"worker_process_started":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&handoff.status),
        render_sandbox_handoff_json(handoff),
        json_string_literal(&handoff.handoff_id),
        json_string_literal(&handoff.tenant_id),
        json_string_literal(&handoff.actor_id),
        json_string_literal(&handoff.job_id),
        json_string_literal(&handoff.run_id),
        json_string_literal(&handoff.worker_run_id),
        json_string_literal(&handoff.terminal_state),
        json_string_literal(&handoff.handoff_decision),
        handoff.sandbox_authority_observed,
        handoff.execution_admission_observed,
        handoff.execution_schedule_observed,
        handoff.execution_supervision_observed,
        handoff.worker_driver_observed,
        handoff.event_relay_observed,
        handoff.durable_restart_replay_observed,
        handoff.quarantine_observed,
        handoff.kill_switch_observed,
        handoff.target_concurrent_engineers,
        handoff.peak_parallel_forge_builds,
        handoff.scheduled_build_count,
        handoff.schedulable_build_count,
        handoff.retained_backpressure_count,
        handoff.launch_window_count,
        handoff.quarantine_artifact_count,
        handoff.rejected,
        json_string_literal(&handoff.rejection_reason)
    )
}

fn render_sandbox_handoff_json(handoff: &DxrSandboxHandoff) -> String {
    format!(
        r#"{{"sequence":{},"handoff_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"idempotency_key":{},"sandbox_authority_envelope_id":{},"sandbox_authority_receipt_id":{},"sandbox_authority_status":{},"execution_admission_id":{},"execution_admission_status":{},"execution_schedule_id":{},"execution_supervision_id":{},"worker_driver_run_id":{},"dispatch_claim_id":{},"heartbeat_receipt_id":{},"durable_workflow_receipt_id":{},"external_sandbox_preflight_id":{},"sandbox_adapter_turn_on_receipt_id":{},"ctx_context_input_id":{},"tool_policy_receipt_id":{},"provider_failover_proof_id":{},"harness_verdict_receipt_id":{},"reviewer_separation_receipt_id":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"scheduled_build_count":{},"schedulable_build_count":{},"retained_backpressure_count":{},"driver_batch_size":{},"max_runtime_ms":{},"heartbeat_interval_ms":{},"kill_switch_sweep_interval_ms":{},"launch_window_count":{},"quarantine_artifact_count":{},"sandbox_authority_observed":{},"execution_admission_observed":{},"execution_schedule_observed":{},"execution_supervision_observed":{},"worker_driver_observed":{},"dispatch_claim_observed":{},"heartbeat_observed":{},"durable_workflow_observed":{},"external_sandbox_preflight_observed":{},"sandbox_adapter_turn_on_observed":{},"ctx_context_observed":{},"tool_policy_observed":{},"provider_failover_observed":{},"harness_verdict_observed":{},"reviewer_separation_observed":{},"event_relay_observed":{},"reconnect_replay_observed":{},"durable_restart_replay_observed":{},"evidence_chain_observed":{},"quarantine_observed":{},"artifact_sink_observed":{},"log_stream_observed":{},"kill_switch_observed":{},"tenant_fairness_observed":{},"backpressure_observed":{},"performance_budget_observed":{},"status":{},"terminal_state":{},"handoff_decision":{},"rejected":{},"rejection_reason":{},"worker_process_started":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        handoff.sequence,
        json_string_literal(&handoff.handoff_id),
        json_string_literal(&handoff.tenant_id),
        json_string_literal(&handoff.actor_id),
        json_string_literal(&handoff.job_id),
        json_string_literal(&handoff.run_id),
        json_string_literal(&handoff.worker_run_id),
        json_string_literal(&handoff.idempotency_key),
        json_string_literal(&handoff.sandbox_authority_envelope_id),
        json_string_literal(&handoff.sandbox_authority_receipt_id),
        json_string_literal(&handoff.sandbox_authority_status),
        json_string_literal(&handoff.execution_admission_id),
        json_string_literal(&handoff.execution_admission_status),
        json_string_literal(&handoff.execution_schedule_id),
        json_string_literal(&handoff.execution_supervision_id),
        json_string_literal(&handoff.worker_driver_run_id),
        json_string_literal(&handoff.dispatch_claim_id),
        json_string_literal(&handoff.heartbeat_receipt_id),
        json_string_literal(&handoff.durable_workflow_receipt_id),
        json_string_literal(&handoff.external_sandbox_preflight_id),
        json_string_literal(&handoff.sandbox_adapter_turn_on_receipt_id),
        json_string_literal(&handoff.ctx_context_input_id),
        json_string_literal(&handoff.tool_policy_receipt_id),
        json_string_literal(&handoff.provider_failover_proof_id),
        json_string_literal(&handoff.harness_verdict_receipt_id),
        json_string_literal(&handoff.reviewer_separation_receipt_id),
        handoff.target_concurrent_engineers,
        handoff.peak_parallel_forge_builds,
        handoff.scheduled_build_count,
        handoff.schedulable_build_count,
        handoff.retained_backpressure_count,
        handoff.driver_batch_size,
        handoff.max_runtime_ms,
        handoff.heartbeat_interval_ms,
        handoff.kill_switch_sweep_interval_ms,
        handoff.launch_window_count,
        handoff.quarantine_artifact_count,
        handoff.sandbox_authority_observed,
        handoff.execution_admission_observed,
        handoff.execution_schedule_observed,
        handoff.execution_supervision_observed,
        handoff.worker_driver_observed,
        handoff.dispatch_claim_observed,
        handoff.heartbeat_observed,
        handoff.durable_workflow_observed,
        handoff.external_sandbox_preflight_observed,
        handoff.sandbox_adapter_turn_on_observed,
        handoff.ctx_context_observed,
        handoff.tool_policy_observed,
        handoff.provider_failover_observed,
        handoff.harness_verdict_observed,
        handoff.reviewer_separation_observed,
        handoff.event_relay_observed,
        handoff.reconnect_replay_observed,
        handoff.durable_restart_replay_observed,
        handoff.evidence_chain_observed,
        handoff.quarantine_observed,
        handoff.artifact_sink_observed,
        handoff.log_stream_observed,
        handoff.kill_switch_observed,
        handoff.tenant_fairness_observed,
        handoff.backpressure_observed,
        handoff.performance_budget_observed,
        json_string_literal(&handoff.status),
        json_string_literal(&handoff.terminal_state),
        json_string_literal(&handoff.handoff_decision),
        handoff.rejected,
        json_string_literal(&handoff.rejection_reason)
    )
}

fn ceil_div(value: usize, divisor: usize) -> usize {
    if value == 0 {
        0
    } else {
        ((value - 1) / divisor.max(1)) + 1
    }
}

fn string_value(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn usize_value(value: &Value, key: &str, default: usize) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
}

fn bool_value(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervised_sandbox_handoff_records_floor_and_rejects_authority() {
        let mut runtime = DxrSandboxHandoffRuntime::new();
        let accepted = runtime
            .submit_json(
                r#"{"idempotency_key":"handoff-001","sandbox_authority_envelope_id":"dxr_sandbox_authority_envelope_000001","sandbox_authority_receipt_id":"dxr_sandbox_authority_receipt_000001","sandbox_authority_status":"LIVE-LOCAL-DXR-SANDBOX-AUTHORITY-ENVELOPE-FLOOR","execution_admission_id":"dxr_execution_admission_000001","execution_admission_status":"LIVE-LOCAL-DXR-EXECUTION-ADMISSION-FLOOR","execution_schedule_id":"dxr_execution_schedule_000001","execution_supervision_id":"dxr_execution_supervision_000001","worker_driver_run_id":"dxr_worker_driver_run_000001","dispatch_claim_id":"dxr_dispatch_claim_001","heartbeat_receipt_id":"dxr_dispatch_heartbeat_receipt_001","durable_workflow_receipt_id":"dxr_workflow_run_000001","external_sandbox_preflight_id":"dxr_external_sandbox_preflight_000001","sandbox_adapter_turn_on_receipt_id":"dxr_sandbox_adapter_turn_on_receipt_000001","ctx_context_input_id":"dxr_ctx_context_input_000001","tool_policy_receipt_id":"dxr_tool_execution_000001","provider_failover_proof_id":"dxr_provider_failover_proof_000001","harness_verdict_receipt_id":"dxr_harness_verdict_000001","reviewer_separation_receipt_id":"dxr_reviewer_separation_000001","sandbox_authority_observed":true,"execution_admission_observed":true,"execution_schedule_observed":true,"execution_supervision_observed":true,"worker_driver_observed":true,"dispatch_claim_observed":true,"heartbeat_observed":true,"durable_workflow_observed":true,"external_sandbox_preflight_observed":true,"sandbox_adapter_turn_on_observed":true,"ctx_context_observed":true,"tool_policy_observed":true,"provider_failover_observed":true,"harness_verdict_observed":true,"reviewer_separation_observed":true,"event_relay_observed":true,"reconnect_replay_observed":true,"durable_restart_replay_observed":true,"evidence_chain_observed":true,"quarantine_observed":true,"artifact_sink_observed":true,"log_stream_observed":true,"kill_switch_observed":true,"tenant_fairness_observed":true,"backpressure_observed":true,"performance_budget_observed":true}"#,
            )
            .expect("accepted handoff");
        assert!(
            accepted
                .body
                .contains("LIVE-LOCAL-DXR-SUPERVISED-SANDBOX-HANDOFF-FLOOR")
        );
        assert!(
            accepted
                .body
                .contains("DXR_SUPERVISED_SANDBOX_HANDOFF_RECORDED_BACKPRESSURE_RETAINED")
        );
        assert!(accepted.body.contains(r#""worker_process_started":false"#));
        assert!(accepted.body.contains(r#""sandbox_process_started":false"#));
        assert!(
            accepted
                .events
                .iter()
                .any(|event| event.event_type == "supervised_sandbox_handoff_driver_bound")
        );

        let rejected = runtime
            .submit_json(
                r#"{"idempotency_key":"handoff-002","sandbox_authority_envelope_id":"dxr_sandbox_authority_envelope_000001","sandbox_authority_receipt_id":"dxr_sandbox_authority_receipt_000001","sandbox_authority_status":"LIVE-LOCAL-DXR-SANDBOX-AUTHORITY-ENVELOPE-FLOOR","execution_admission_id":"dxr_execution_admission_000001","execution_admission_status":"LIVE-LOCAL-DXR-EXECUTION-ADMISSION-FLOOR","execution_schedule_id":"dxr_execution_schedule_000001","execution_supervision_id":"dxr_execution_supervision_000001","worker_driver_run_id":"dxr_worker_driver_run_000001","dispatch_claim_id":"dxr_dispatch_claim_001","heartbeat_receipt_id":"dxr_dispatch_heartbeat_receipt_001","durable_workflow_receipt_id":"dxr_workflow_run_000001","external_sandbox_preflight_id":"dxr_external_sandbox_preflight_000001","sandbox_adapter_turn_on_receipt_id":"dxr_sandbox_adapter_turn_on_receipt_000001","ctx_context_input_id":"dxr_ctx_context_input_000001","tool_policy_receipt_id":"dxr_tool_execution_000001","provider_failover_proof_id":"dxr_provider_failover_proof_000001","harness_verdict_receipt_id":"dxr_harness_verdict_000001","reviewer_separation_receipt_id":"dxr_reviewer_separation_000001","sandbox_authority_observed":true,"execution_admission_observed":true,"execution_schedule_observed":true,"execution_supervision_observed":true,"worker_driver_observed":true,"dispatch_claim_observed":true,"heartbeat_observed":true,"durable_workflow_observed":true,"external_sandbox_preflight_observed":true,"sandbox_adapter_turn_on_observed":true,"ctx_context_observed":true,"tool_policy_observed":true,"provider_failover_observed":true,"harness_verdict_observed":true,"reviewer_separation_observed":true,"event_relay_observed":true,"reconnect_replay_observed":true,"durable_restart_replay_observed":true,"evidence_chain_observed":true,"quarantine_observed":true,"artifact_sink_observed":true,"log_stream_observed":true,"kill_switch_observed":true,"tenant_fairness_observed":true,"backpressure_observed":true,"performance_budget_observed":true,"sandbox_process_start_requested":true,"network_requested":true}"#,
            )
            .expect("rejected handoff");
        assert!(
            rejected
                .body
                .contains("DXR_SUPERVISED_SANDBOX_HANDOFF_REJECTED_SECURITY_BOUNDARY")
        );
        assert!(runtime.handoffs_json().contains(r#""handoff_count":2"#));
        assert!(runtime.handoffs_json().contains(r#""staged_count":1"#));
        assert!(runtime.handoffs_json().contains(r#""rejected_count":1"#));
    }
}
