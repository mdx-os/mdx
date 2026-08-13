use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_JOB_ID: &str = "dxr_job_worker_driver_001";
const DEFAULT_RUN_ID: &str = "dxr_run_worker_driver_001";
const DEFAULT_WORKER_RUN_ID: &str = "dxr_worker_run_driver_001";
const DEFAULT_TARGET_ENGINEERS: usize = 1000;
const DEFAULT_PEAK_PARALLEL_FORGE_BUILDS: usize = 5000;
const DEFAULT_SCHEDULED_BUILD_COUNT: usize = 5000;
const DEFAULT_SCHEDULABLE_BUILD_COUNT: usize = 2048;
const DEFAULT_RETAINED_BACKPRESSURE_COUNT: usize = 2952;
const DEFAULT_DRIVER_SHARD_SIZE: usize = 64;
const DEFAULT_DRIVER_BATCH_SIZE: usize = 320;
const DEFAULT_PROVIDER_STREAM_SLOTS: usize = 512;
const DEFAULT_WORKFLOW_SCRIPT_SLOTS: usize = 320;
const DEFAULT_MAX_DRIVER_RUNTIME_MS: usize = 300_000;
const DEFAULT_DRIVER_HEARTBEAT_INTERVAL_MS: usize = 5_000;
const DEFAULT_DRIVER_REPLAY_CHECKPOINT_INTERVAL_MS: usize = 15_000;
const DEFAULT_DRIVER_KILL_SWITCH_SWEEP_INTERVAL_MS: usize = 2_500;

#[derive(Default)]
pub struct DxrWorkerDriverRuntime {
    runs: Vec<DxrWorkerDriverRun>,
    next_run: usize,
}

pub struct DxrWorkerDriverResult {
    pub body: String,
    pub events: Vec<DxrWorkerDriverRuntimeEvent>,
}

pub struct DxrWorkerDriverRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrWorkerDriverRun {
    sequence: usize,
    driver_run_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    turn_loop_id: String,
    idempotency_key: String,
    execution_supervision_id: String,
    worker_turn_loop_id: String,
    ctx_context_input_id: String,
    dispatch_claim_id: String,
    heartbeat_receipt_id: String,
    durable_workflow_receipt_id: String,
    sandbox_admission_id: String,
    external_sandbox_preflight_id: String,
    tool_policy_receipt_id: String,
    provider_failover_proof_id: String,
    harness_verdict_receipt_id: String,
    reviewer_separation_receipt_id: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    scheduled_build_count: usize,
    schedulable_build_count: usize,
    retained_backpressure_count: usize,
    driver_shard_size: usize,
    driver_batch_size: usize,
    provider_stream_slots: usize,
    workflow_script_slots: usize,
    max_driver_runtime_ms: usize,
    driver_heartbeat_interval_ms: usize,
    driver_replay_checkpoint_interval_ms: usize,
    driver_kill_switch_sweep_interval_ms: usize,
    driver_shard_count: usize,
    driver_batch_count: usize,
    provider_stream_batches_required: usize,
    workflow_batches_required: usize,
    backpressure_replay_batch_count: usize,
    execution_supervision_observed: bool,
    worker_turn_loop_observed: bool,
    ctx_context_observed: bool,
    dispatch_claim_observed: bool,
    heartbeat_observed: bool,
    durable_workflow_observed: bool,
    sandbox_admission_observed: bool,
    external_sandbox_preflight_observed: bool,
    provider_stream_slot_observed: bool,
    provider_failover_observed: bool,
    tool_policy_observed: bool,
    harness_verdict_observed: bool,
    evidence_chain_observed: bool,
    event_relay_observed: bool,
    reconnect_replay_observed: bool,
    durable_restart_replay_observed: bool,
    reviewer_separation_observed: bool,
    dynamic_workflow_checkpoint_observed: bool,
    kill_switch_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    performance_budget_observed: bool,
    status: String,
    terminal_state: String,
    driver_decision: String,
    rejected: bool,
    rejection_reason: String,
}

struct WorkerDriverRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    turn_loop_id: String,
    idempotency_key: String,
    execution_supervision_id: String,
    worker_turn_loop_id: String,
    ctx_context_input_id: String,
    dispatch_claim_id: String,
    heartbeat_receipt_id: String,
    durable_workflow_receipt_id: String,
    sandbox_admission_id: String,
    external_sandbox_preflight_id: String,
    tool_policy_receipt_id: String,
    provider_failover_proof_id: String,
    harness_verdict_receipt_id: String,
    reviewer_separation_receipt_id: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    scheduled_build_count: usize,
    schedulable_build_count: usize,
    retained_backpressure_count: usize,
    driver_shard_size: usize,
    driver_batch_size: usize,
    provider_stream_slots: usize,
    workflow_script_slots: usize,
    max_driver_runtime_ms: usize,
    driver_heartbeat_interval_ms: usize,
    driver_replay_checkpoint_interval_ms: usize,
    driver_kill_switch_sweep_interval_ms: usize,
    execution_supervision_observed: bool,
    worker_turn_loop_observed: bool,
    ctx_context_observed: bool,
    dispatch_claim_observed: bool,
    heartbeat_observed: bool,
    durable_workflow_observed: bool,
    sandbox_admission_observed: bool,
    external_sandbox_preflight_observed: bool,
    provider_stream_slot_observed: bool,
    provider_failover_observed: bool,
    tool_policy_observed: bool,
    harness_verdict_observed: bool,
    evidence_chain_observed: bool,
    event_relay_observed: bool,
    reconnect_replay_observed: bool,
    durable_restart_replay_observed: bool,
    reviewer_separation_observed: bool,
    dynamic_workflow_checkpoint_observed: bool,
    kill_switch_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    performance_budget_observed: bool,
    worker_process_start_requested: bool,
    sandbox_process_start_requested: bool,
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

impl DxrWorkerDriverRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrWorkerDriverResult, String> {
        let request = parse_worker_driver_request(body)?;
        self.next_run += 1;

        let authority_requested = request.worker_process_start_requested
            || request.sandbox_process_start_requested
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
            || request.execution_supervision_id.trim().is_empty()
            || request.worker_turn_loop_id.trim().is_empty()
            || request.ctx_context_input_id.trim().is_empty()
            || request.dispatch_claim_id.trim().is_empty()
            || request.heartbeat_receipt_id.trim().is_empty()
            || request.durable_workflow_receipt_id.trim().is_empty()
            || request.sandbox_admission_id.trim().is_empty()
            || request.external_sandbox_preflight_id.trim().is_empty()
            || request.tool_policy_receipt_id.trim().is_empty()
            || request.provider_failover_proof_id.trim().is_empty()
            || request.harness_verdict_receipt_id.trim().is_empty()
            || request.reviewer_separation_receipt_id.trim().is_empty()
            || request.scheduled_build_count == 0
            || request.schedulable_build_count == 0
            || request.driver_shard_size == 0
            || request.driver_batch_size == 0
            || request.provider_stream_slots == 0
            || request.workflow_script_slots == 0
            || request.max_driver_runtime_ms == 0
            || request.driver_heartbeat_interval_ms == 0
            || request.driver_replay_checkpoint_interval_ms == 0
            || request.driver_kill_switch_sweep_interval_ms == 0
            || !request.execution_supervision_observed
            || !request.worker_turn_loop_observed
            || !request.ctx_context_observed
            || !request.dispatch_claim_observed
            || !request.heartbeat_observed
            || !request.durable_workflow_observed
            || !request.sandbox_admission_observed
            || !request.external_sandbox_preflight_observed
            || !request.provider_stream_slot_observed
            || !request.provider_failover_observed
            || !request.tool_policy_observed
            || !request.harness_verdict_observed
            || !request.evidence_chain_observed
            || !request.event_relay_observed
            || !request.reconnect_replay_observed
            || !request.durable_restart_replay_observed
            || !request.reviewer_separation_observed
            || !request.dynamic_workflow_checkpoint_observed
            || !request.kill_switch_observed
            || !request.tenant_fairness_observed
            || !request.backpressure_observed
            || !request.performance_budget_observed;

        let driver_shard_count = ceil_div(
            request.schedulable_build_count,
            request.driver_shard_size.max(1),
        );
        let driver_batch_count = ceil_div(
            request.scheduled_build_count,
            request.driver_batch_size.max(1),
        );
        let provider_stream_batches_required = ceil_div(
            request.scheduled_build_count,
            request.provider_stream_slots.max(1),
        );
        let workflow_batches_required = ceil_div(
            request.scheduled_build_count,
            request.workflow_script_slots.max(1),
        );
        let backpressure_replay_batch_count = ceil_div(
            request.retained_backpressure_count,
            request.driver_batch_size.max(1),
        );

        let (status, terminal_state, driver_decision, rejected, rejection_reason) =
            if authority_requested {
                (
                    "DXR_WORKER_DRIVER_REJECTED_SECURITY_BOUNDARY",
                    "DXR_WORKER_DRIVER_REJECTED_SECURITY_BOUNDARY",
                    "rejected_driver_authority_requested",
                    true,
                    "worker_driver_cannot_start_workers_sandboxes_providers_tools_shell_patch_git_network_secrets_filesystem_ci_deployments_or_production_writes",
                )
            } else if missing_evidence {
                (
                    "DXR_WORKER_DRIVER_REJECTED_MISSING_EVIDENCE",
                    "DXR_WORKER_DRIVER_REJECTED_MISSING_EVIDENCE",
                    "rejected_missing_worker_driver_evidence",
                    true,
                    "supervision_turn_loop_ctx_dispatch_heartbeat_workflow_sandbox_provider_tool_harness_relay_replay_reviewer_checkpoint_kill_switch_fairness_backpressure_or_performance_gate_missing",
                )
            } else {
                (
                    "LIVE-LOCAL-DXR-WORKER-DRIVER-FLOOR",
                    "DXR_WORKER_DRIVER_RECORDED_SHARDED_AUTHORITY_BLOCKED",
                    "worker_driver_lanes_planned_backpressure_retained",
                    false,
                    "none",
                )
            };

        let run = DxrWorkerDriverRun {
            sequence: self.next_run,
            driver_run_id: format!("dxr_worker_driver_run_{:06}", self.next_run),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            worker_run_id: request.worker_run_id,
            turn_loop_id: request.turn_loop_id,
            idempotency_key: request.idempotency_key,
            execution_supervision_id: request.execution_supervision_id,
            worker_turn_loop_id: request.worker_turn_loop_id,
            ctx_context_input_id: request.ctx_context_input_id,
            dispatch_claim_id: request.dispatch_claim_id,
            heartbeat_receipt_id: request.heartbeat_receipt_id,
            durable_workflow_receipt_id: request.durable_workflow_receipt_id,
            sandbox_admission_id: request.sandbox_admission_id,
            external_sandbox_preflight_id: request.external_sandbox_preflight_id,
            tool_policy_receipt_id: request.tool_policy_receipt_id,
            provider_failover_proof_id: request.provider_failover_proof_id,
            harness_verdict_receipt_id: request.harness_verdict_receipt_id,
            reviewer_separation_receipt_id: request.reviewer_separation_receipt_id,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            scheduled_build_count: request.scheduled_build_count,
            schedulable_build_count: request.schedulable_build_count,
            retained_backpressure_count: request.retained_backpressure_count,
            driver_shard_size: request.driver_shard_size,
            driver_batch_size: request.driver_batch_size,
            provider_stream_slots: request.provider_stream_slots,
            workflow_script_slots: request.workflow_script_slots,
            max_driver_runtime_ms: request.max_driver_runtime_ms,
            driver_heartbeat_interval_ms: request.driver_heartbeat_interval_ms,
            driver_replay_checkpoint_interval_ms: request.driver_replay_checkpoint_interval_ms,
            driver_kill_switch_sweep_interval_ms: request.driver_kill_switch_sweep_interval_ms,
            driver_shard_count,
            driver_batch_count,
            provider_stream_batches_required,
            workflow_batches_required,
            backpressure_replay_batch_count,
            execution_supervision_observed: request.execution_supervision_observed,
            worker_turn_loop_observed: request.worker_turn_loop_observed,
            ctx_context_observed: request.ctx_context_observed,
            dispatch_claim_observed: request.dispatch_claim_observed,
            heartbeat_observed: request.heartbeat_observed,
            durable_workflow_observed: request.durable_workflow_observed,
            sandbox_admission_observed: request.sandbox_admission_observed,
            external_sandbox_preflight_observed: request.external_sandbox_preflight_observed,
            provider_stream_slot_observed: request.provider_stream_slot_observed,
            provider_failover_observed: request.provider_failover_observed,
            tool_policy_observed: request.tool_policy_observed,
            harness_verdict_observed: request.harness_verdict_observed,
            evidence_chain_observed: request.evidence_chain_observed,
            event_relay_observed: request.event_relay_observed,
            reconnect_replay_observed: request.reconnect_replay_observed,
            durable_restart_replay_observed: request.durable_restart_replay_observed,
            reviewer_separation_observed: request.reviewer_separation_observed,
            dynamic_workflow_checkpoint_observed: request.dynamic_workflow_checkpoint_observed,
            kill_switch_observed: request.kill_switch_observed,
            tenant_fairness_observed: request.tenant_fairness_observed,
            backpressure_observed: request.backpressure_observed,
            performance_budget_observed: request.performance_budget_observed,
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            driver_decision: driver_decision.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let events = worker_driver_events(&run);
        let body = render_worker_driver_response_json(&run);
        self.runs.push(run);
        Ok(DxrWorkerDriverResult { body, events })
    }

    pub fn runs_json(&self) -> String {
        let accepted_count = self.runs.iter().filter(|run| !run.rejected).count();
        let backpressure_count = self
            .runs
            .iter()
            .filter(|run| !run.rejected && run.retained_backpressure_count > 0)
            .count();
        let rejected_count = self.runs.len().saturating_sub(accepted_count);
        format!(
            r#"{{"name":"mdx-dxr-worker-driver-runs","status":"LIVE-LOCAL-DXR-WORKER-DRIVER-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/worker-driver-runs.json","submit_route":"/v1/dxr/worker-driver-runs","driver_run_count":{},"accepted_count":{},"backpressure_count":{},"rejected_count":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"scheduled_build_count":{},"schedulable_build_count":{},"retained_backpressure_count":{},"driver_shard_size":{},"driver_batch_size":{},"driver_shard_count":{},"driver_batch_count":{},"provider_stream_slots":{},"workflow_script_slots":{},"provider_stream_batches_required":{},"workflow_batches_required":{},"backpressure_replay_batch_count":{},"driver_policy":"sharded_resumable_driver_lanes_before_worker_start","replay_policy":"checkpoint_replay_and_kill_switch_required","scale_policy":"tenant_fair_1000_engineers_5000_forge_builds","v1_stage":"worker_driver","required_gates":["execution_supervision","worker_turn_loop","ctx_context","dispatch_claim","heartbeat","durable_workflow","sandbox_admission","external_sandbox_preflight","provider_stream_slot","provider_failover","tool_policy","harness_verdict","evidence_chain","event_relay","reconnect_replay","durable_restart_replay","reviewer_separation","dynamic_workflow_checkpoint","kill_switch","tenant_fairness","backpressure","performance_budget"],"worker_process_started":false,"sandbox_process_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"runs":[{}]}}"#,
            self.runs.len(),
            accepted_count,
            backpressure_count,
            rejected_count,
            self.runs
                .last()
                .map(|run| run.target_concurrent_engineers)
                .unwrap_or(DEFAULT_TARGET_ENGINEERS),
            self.runs
                .last()
                .map(|run| run.peak_parallel_forge_builds)
                .unwrap_or(DEFAULT_PEAK_PARALLEL_FORGE_BUILDS),
            self.runs
                .last()
                .map(|run| run.scheduled_build_count)
                .unwrap_or(DEFAULT_SCHEDULED_BUILD_COUNT),
            self.runs
                .last()
                .map(|run| run.schedulable_build_count)
                .unwrap_or(DEFAULT_SCHEDULABLE_BUILD_COUNT),
            self.runs
                .last()
                .map(|run| run.retained_backpressure_count)
                .unwrap_or(DEFAULT_RETAINED_BACKPRESSURE_COUNT),
            self.runs
                .last()
                .map(|run| run.driver_shard_size)
                .unwrap_or(DEFAULT_DRIVER_SHARD_SIZE),
            self.runs
                .last()
                .map(|run| run.driver_batch_size)
                .unwrap_or(DEFAULT_DRIVER_BATCH_SIZE),
            self.runs
                .last()
                .map(|run| run.driver_shard_count)
                .unwrap_or(ceil_div(
                    DEFAULT_SCHEDULABLE_BUILD_COUNT,
                    DEFAULT_DRIVER_SHARD_SIZE
                )),
            self.runs
                .last()
                .map(|run| run.driver_batch_count)
                .unwrap_or(ceil_div(
                    DEFAULT_SCHEDULED_BUILD_COUNT,
                    DEFAULT_DRIVER_BATCH_SIZE
                )),
            self.runs
                .last()
                .map(|run| run.provider_stream_slots)
                .unwrap_or(DEFAULT_PROVIDER_STREAM_SLOTS),
            self.runs
                .last()
                .map(|run| run.workflow_script_slots)
                .unwrap_or(DEFAULT_WORKFLOW_SCRIPT_SLOTS),
            self.runs
                .last()
                .map(|run| run.provider_stream_batches_required)
                .unwrap_or(ceil_div(
                    DEFAULT_SCHEDULED_BUILD_COUNT,
                    DEFAULT_PROVIDER_STREAM_SLOTS
                )),
            self.runs
                .last()
                .map(|run| run.workflow_batches_required)
                .unwrap_or(ceil_div(
                    DEFAULT_SCHEDULED_BUILD_COUNT,
                    DEFAULT_WORKFLOW_SCRIPT_SLOTS
                )),
            self.runs
                .last()
                .map(|run| run.backpressure_replay_batch_count)
                .unwrap_or(ceil_div(
                    DEFAULT_RETAINED_BACKPRESSURE_COUNT,
                    DEFAULT_DRIVER_BATCH_SIZE
                )),
            self.runs
                .iter()
                .map(render_worker_driver_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn parse_worker_driver_request(body: &str) -> Result<WorkerDriverRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR worker driver json: {error}"))?;
    Ok(WorkerDriverRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", DEFAULT_JOB_ID),
        run_id: string_value(&value, "run_id", DEFAULT_RUN_ID),
        worker_run_id: string_value(&value, "worker_run_id", DEFAULT_WORKER_RUN_ID),
        turn_loop_id: string_value(&value, "turn_loop_id", ""),
        idempotency_key: string_value(&value, "idempotency_key", ""),
        execution_supervision_id: string_value(&value, "execution_supervision_id", ""),
        worker_turn_loop_id: string_value(&value, "worker_turn_loop_id", ""),
        ctx_context_input_id: string_value(&value, "ctx_context_input_id", ""),
        dispatch_claim_id: string_value(&value, "dispatch_claim_id", ""),
        heartbeat_receipt_id: string_value(&value, "heartbeat_receipt_id", ""),
        durable_workflow_receipt_id: string_value(&value, "durable_workflow_receipt_id", ""),
        sandbox_admission_id: string_value(&value, "sandbox_admission_id", ""),
        external_sandbox_preflight_id: string_value(&value, "external_sandbox_preflight_id", ""),
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
        driver_shard_size: usize_value(&value, "driver_shard_size", DEFAULT_DRIVER_SHARD_SIZE),
        driver_batch_size: usize_value(&value, "driver_batch_size", DEFAULT_DRIVER_BATCH_SIZE),
        provider_stream_slots: usize_value(
            &value,
            "provider_stream_slots",
            DEFAULT_PROVIDER_STREAM_SLOTS,
        ),
        workflow_script_slots: usize_value(
            &value,
            "workflow_script_slots",
            DEFAULT_WORKFLOW_SCRIPT_SLOTS,
        ),
        max_driver_runtime_ms: usize_value(
            &value,
            "max_driver_runtime_ms",
            DEFAULT_MAX_DRIVER_RUNTIME_MS,
        ),
        driver_heartbeat_interval_ms: usize_value(
            &value,
            "driver_heartbeat_interval_ms",
            DEFAULT_DRIVER_HEARTBEAT_INTERVAL_MS,
        ),
        driver_replay_checkpoint_interval_ms: usize_value(
            &value,
            "driver_replay_checkpoint_interval_ms",
            DEFAULT_DRIVER_REPLAY_CHECKPOINT_INTERVAL_MS,
        ),
        driver_kill_switch_sweep_interval_ms: usize_value(
            &value,
            "driver_kill_switch_sweep_interval_ms",
            DEFAULT_DRIVER_KILL_SWITCH_SWEEP_INTERVAL_MS,
        ),
        execution_supervision_observed: bool_value(&value, "execution_supervision_observed", false),
        worker_turn_loop_observed: bool_value(&value, "worker_turn_loop_observed", false),
        ctx_context_observed: bool_value(&value, "ctx_context_observed", false),
        dispatch_claim_observed: bool_value(&value, "dispatch_claim_observed", false),
        heartbeat_observed: bool_value(&value, "heartbeat_observed", false),
        durable_workflow_observed: bool_value(&value, "durable_workflow_observed", false),
        sandbox_admission_observed: bool_value(&value, "sandbox_admission_observed", false),
        external_sandbox_preflight_observed: bool_value(
            &value,
            "external_sandbox_preflight_observed",
            false,
        ),
        provider_stream_slot_observed: bool_value(&value, "provider_stream_slot_observed", false),
        provider_failover_observed: bool_value(&value, "provider_failover_observed", false),
        tool_policy_observed: bool_value(&value, "tool_policy_observed", false),
        harness_verdict_observed: bool_value(&value, "harness_verdict_observed", false),
        evidence_chain_observed: bool_value(&value, "evidence_chain_observed", false),
        event_relay_observed: bool_value(&value, "event_relay_observed", false),
        reconnect_replay_observed: bool_value(&value, "reconnect_replay_observed", false),
        durable_restart_replay_observed: bool_value(
            &value,
            "durable_restart_replay_observed",
            false,
        ),
        reviewer_separation_observed: bool_value(&value, "reviewer_separation_observed", false),
        dynamic_workflow_checkpoint_observed: bool_value(
            &value,
            "dynamic_workflow_checkpoint_observed",
            false,
        ),
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

fn worker_driver_events(run: &DxrWorkerDriverRun) -> Vec<DxrWorkerDriverRuntimeEvent> {
    let mut events = vec![worker_driver_event(run, "worker_driver_run_recorded")];
    if run.rejected {
        events.push(worker_driver_event(run, "worker_driver_rejected"));
    } else {
        events.extend([
            worker_driver_event(run, "worker_driver_shards_planned"),
            worker_driver_event(run, "worker_driver_turn_loop_bound"),
            worker_driver_event(run, "worker_driver_replay_checkpoint_recorded"),
            worker_driver_event(run, "worker_driver_kill_switch_armed"),
            worker_driver_event(run, "worker_driver_backpressure_retained"),
            worker_driver_event(run, "worker_driver_provider_stream_slots_retained"),
            worker_driver_event(run, "worker_driver_reviewer_separation_retained"),
        ]);
    }
    events.push(worker_driver_event(run, "worker_driver_authority_blocked"));
    events
}

fn worker_driver_event(run: &DxrWorkerDriverRun, event_type: &str) -> DxrWorkerDriverRuntimeEvent {
    DxrWorkerDriverRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: run.tenant_id.clone(),
        job_id: run.job_id.clone(),
        run_id: run.run_id.clone(),
        actor_id: run.actor_id.clone(),
    }
}

fn render_worker_driver_response_json(run: &DxrWorkerDriverRun) -> String {
    format!(
        r#"{{"name":"mdx-dxr-worker-driver-run","status":{},"runtime":"mdx-dxr-engine","driver_run":{},"driver_run_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"turn_loop_id":{},"terminal_state":{},"driver_decision":{},"idempotency_key":{},"v1_stage":"worker_driver","target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"scheduled_build_count":{},"schedulable_build_count":{},"retained_backpressure_count":{},"driver_shard_size":{},"driver_batch_size":{},"driver_shard_count":{},"driver_batch_count":{},"provider_stream_slots":{},"workflow_script_slots":{},"provider_stream_batches_required":{},"workflow_batches_required":{},"backpressure_replay_batch_count":{},"max_driver_runtime_ms":{},"driver_heartbeat_interval_ms":{},"driver_replay_checkpoint_interval_ms":{},"driver_kill_switch_sweep_interval_ms":{},"driver_policy":"sharded_resumable_driver_lanes_before_worker_start","replay_policy":"checkpoint_replay_and_kill_switch_required","scale_policy":"tenant_fair_1000_engineers_5000_forge_builds","execution_supervision_required":true,"worker_turn_loop_required":true,"ctx_context_required":true,"dispatch_claim_required":true,"heartbeat_required":true,"durable_workflow_required":true,"sandbox_admission_required":true,"external_sandbox_preflight_required":true,"provider_stream_slot_required":true,"provider_failover_required":true,"tool_policy_required":true,"harness_verdict_required":true,"evidence_chain_required":true,"event_relay_required":true,"reconnect_replay_required":true,"durable_restart_replay_required":true,"reviewer_separation_required":true,"dynamic_workflow_checkpoint_required":true,"kill_switch_required":true,"tenant_fairness_required":true,"backpressure_required":true,"performance_budget_required":true,"rejected":{},"rejection_reason":{},"worker_process_started":false,"sandbox_process_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&run.status),
        render_worker_driver_json(run),
        json_string_literal(&run.driver_run_id),
        json_string_literal(&run.tenant_id),
        json_string_literal(&run.actor_id),
        json_string_literal(&run.job_id),
        json_string_literal(&run.run_id),
        json_string_literal(&run.worker_run_id),
        json_string_literal(&run.turn_loop_id),
        json_string_literal(&run.terminal_state),
        json_string_literal(&run.driver_decision),
        json_string_literal(&run.idempotency_key),
        run.target_concurrent_engineers,
        run.peak_parallel_forge_builds,
        run.scheduled_build_count,
        run.schedulable_build_count,
        run.retained_backpressure_count,
        run.driver_shard_size,
        run.driver_batch_size,
        run.driver_shard_count,
        run.driver_batch_count,
        run.provider_stream_slots,
        run.workflow_script_slots,
        run.provider_stream_batches_required,
        run.workflow_batches_required,
        run.backpressure_replay_batch_count,
        run.max_driver_runtime_ms,
        run.driver_heartbeat_interval_ms,
        run.driver_replay_checkpoint_interval_ms,
        run.driver_kill_switch_sweep_interval_ms,
        run.rejected,
        json_string_literal(&run.rejection_reason)
    )
}

fn render_worker_driver_json(run: &DxrWorkerDriverRun) -> String {
    format!(
        r#"{{"sequence":{},"driver_run_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"turn_loop_id":{},"idempotency_key":{},"execution_supervision_id":{},"worker_turn_loop_id":{},"ctx_context_input_id":{},"dispatch_claim_id":{},"heartbeat_receipt_id":{},"durable_workflow_receipt_id":{},"sandbox_admission_id":{},"external_sandbox_preflight_id":{},"tool_policy_receipt_id":{},"provider_failover_proof_id":{},"harness_verdict_receipt_id":{},"reviewer_separation_receipt_id":{},"v1_stage":"worker_driver","target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"scheduled_build_count":{},"schedulable_build_count":{},"retained_backpressure_count":{},"driver_shard_size":{},"driver_batch_size":{},"provider_stream_slots":{},"workflow_script_slots":{},"max_driver_runtime_ms":{},"driver_heartbeat_interval_ms":{},"driver_replay_checkpoint_interval_ms":{},"driver_kill_switch_sweep_interval_ms":{},"driver_shard_count":{},"driver_batch_count":{},"provider_stream_batches_required":{},"workflow_batches_required":{},"backpressure_replay_batch_count":{},"execution_supervision_observed":{},"worker_turn_loop_observed":{},"ctx_context_observed":{},"dispatch_claim_observed":{},"heartbeat_observed":{},"durable_workflow_observed":{},"sandbox_admission_observed":{},"external_sandbox_preflight_observed":{},"provider_stream_slot_observed":{},"provider_failover_observed":{},"tool_policy_observed":{},"harness_verdict_observed":{},"evidence_chain_observed":{},"event_relay_observed":{},"reconnect_replay_observed":{},"durable_restart_replay_observed":{},"reviewer_separation_observed":{},"dynamic_workflow_checkpoint_observed":{},"kill_switch_observed":{},"tenant_fairness_observed":{},"backpressure_observed":{},"performance_budget_observed":{},"status":{},"terminal_state":{},"driver_decision":{},"rejected":{},"rejection_reason":{},"worker_process_started":false,"sandbox_process_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        run.sequence,
        json_string_literal(&run.driver_run_id),
        json_string_literal(&run.tenant_id),
        json_string_literal(&run.actor_id),
        json_string_literal(&run.job_id),
        json_string_literal(&run.run_id),
        json_string_literal(&run.worker_run_id),
        json_string_literal(&run.turn_loop_id),
        json_string_literal(&run.idempotency_key),
        json_string_literal(&run.execution_supervision_id),
        json_string_literal(&run.worker_turn_loop_id),
        json_string_literal(&run.ctx_context_input_id),
        json_string_literal(&run.dispatch_claim_id),
        json_string_literal(&run.heartbeat_receipt_id),
        json_string_literal(&run.durable_workflow_receipt_id),
        json_string_literal(&run.sandbox_admission_id),
        json_string_literal(&run.external_sandbox_preflight_id),
        json_string_literal(&run.tool_policy_receipt_id),
        json_string_literal(&run.provider_failover_proof_id),
        json_string_literal(&run.harness_verdict_receipt_id),
        json_string_literal(&run.reviewer_separation_receipt_id),
        run.target_concurrent_engineers,
        run.peak_parallel_forge_builds,
        run.scheduled_build_count,
        run.schedulable_build_count,
        run.retained_backpressure_count,
        run.driver_shard_size,
        run.driver_batch_size,
        run.provider_stream_slots,
        run.workflow_script_slots,
        run.max_driver_runtime_ms,
        run.driver_heartbeat_interval_ms,
        run.driver_replay_checkpoint_interval_ms,
        run.driver_kill_switch_sweep_interval_ms,
        run.driver_shard_count,
        run.driver_batch_count,
        run.provider_stream_batches_required,
        run.workflow_batches_required,
        run.backpressure_replay_batch_count,
        run.execution_supervision_observed,
        run.worker_turn_loop_observed,
        run.ctx_context_observed,
        run.dispatch_claim_observed,
        run.heartbeat_observed,
        run.durable_workflow_observed,
        run.sandbox_admission_observed,
        run.external_sandbox_preflight_observed,
        run.provider_stream_slot_observed,
        run.provider_failover_observed,
        run.tool_policy_observed,
        run.harness_verdict_observed,
        run.evidence_chain_observed,
        run.event_relay_observed,
        run.reconnect_replay_observed,
        run.durable_restart_replay_observed,
        run.reviewer_separation_observed,
        run.dynamic_workflow_checkpoint_observed,
        run.kill_switch_observed,
        run.tenant_fairness_observed,
        run.backpressure_observed,
        run.performance_budget_observed,
        json_string_literal(&run.status),
        json_string_literal(&run.terminal_state),
        json_string_literal(&run.driver_decision),
        run.rejected,
        json_string_literal(&run.rejection_reason)
    )
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
        .map(|value| value as usize)
        .unwrap_or(default)
}

fn ceil_div(value: usize, divisor: usize) -> usize {
    if value == 0 {
        0
    } else {
        ((value - 1) / divisor) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_driver_records_sharded_floor_and_rejects_authority() {
        let mut runtime = DxrWorkerDriverRuntime::new();
        let accepted = runtime
            .submit_json(
                r#"{"idempotency_key":"driver-001","execution_supervision_id":"dxr_execution_supervision_000001","worker_turn_loop_id":"dxr_worker_turn_loop_000001","turn_loop_id":"dxr_worker_turn_loop_000001","ctx_context_input_id":"dxr_ctx_context_input_000001","dispatch_claim_id":"dxr_dispatch_claim_001","heartbeat_receipt_id":"dxr_dispatch_heartbeat_receipt_001","durable_workflow_receipt_id":"dxr_workflow_run_000001","sandbox_admission_id":"dxr_sandbox_admission_000001","external_sandbox_preflight_id":"dxr_external_sandbox_preflight_000001","tool_policy_receipt_id":"dxr_tool_execution_000001","provider_failover_proof_id":"dxr_provider_failover_proof_000001","harness_verdict_receipt_id":"dxr_harness_verdict_000001","reviewer_separation_receipt_id":"dxr_reviewer_separation_000001","execution_supervision_observed":true,"worker_turn_loop_observed":true,"ctx_context_observed":true,"dispatch_claim_observed":true,"heartbeat_observed":true,"durable_workflow_observed":true,"sandbox_admission_observed":true,"external_sandbox_preflight_observed":true,"provider_stream_slot_observed":true,"provider_failover_observed":true,"tool_policy_observed":true,"harness_verdict_observed":true,"evidence_chain_observed":true,"event_relay_observed":true,"reconnect_replay_observed":true,"durable_restart_replay_observed":true,"reviewer_separation_observed":true,"dynamic_workflow_checkpoint_observed":true,"kill_switch_observed":true,"tenant_fairness_observed":true,"backpressure_observed":true,"performance_budget_observed":true}"#,
            )
            .expect("accepted worker driver");
        assert!(
            accepted
                .body
                .contains("DXR_WORKER_DRIVER_RECORDED_SHARDED_AUTHORITY_BLOCKED")
        );
        assert!(accepted.body.contains("\"driver_shard_count\":32"));
        assert!(accepted.body.contains("\"driver_batch_count\":16"));
        assert!(
            accepted
                .body
                .contains("\"provider_stream_batches_required\":10")
        );
        assert!(
            accepted
                .body
                .contains("\"backpressure_replay_batch_count\":10")
        );
        assert!(accepted.body.contains("\"worker_process_started\":false"));
        assert!(accepted.body.contains("\"sandbox_process_started\":false"));
        assert!(accepted.body.contains("\"tool_execution_allowed\":false"));
        assert!(
            accepted
                .events
                .iter()
                .any(|event| event.event_type == "worker_driver_kill_switch_armed")
        );

        let rejected = runtime
            .submit_json(
                r#"{"idempotency_key":"driver-002","execution_supervision_id":"dxr_execution_supervision_000001","worker_turn_loop_id":"dxr_worker_turn_loop_000001","turn_loop_id":"dxr_worker_turn_loop_000001","ctx_context_input_id":"dxr_ctx_context_input_000001","dispatch_claim_id":"dxr_dispatch_claim_001","heartbeat_receipt_id":"dxr_dispatch_heartbeat_receipt_001","durable_workflow_receipt_id":"dxr_workflow_run_000001","sandbox_admission_id":"dxr_sandbox_admission_000001","external_sandbox_preflight_id":"dxr_external_sandbox_preflight_000001","tool_policy_receipt_id":"dxr_tool_execution_000001","provider_failover_proof_id":"dxr_provider_failover_proof_000001","harness_verdict_receipt_id":"dxr_harness_verdict_000001","reviewer_separation_receipt_id":"dxr_reviewer_separation_000001","execution_supervision_observed":true,"worker_turn_loop_observed":true,"ctx_context_observed":true,"dispatch_claim_observed":true,"heartbeat_observed":true,"durable_workflow_observed":true,"sandbox_admission_observed":true,"external_sandbox_preflight_observed":true,"provider_stream_slot_observed":true,"provider_failover_observed":true,"tool_policy_observed":true,"harness_verdict_observed":true,"evidence_chain_observed":true,"event_relay_observed":true,"reconnect_replay_observed":true,"durable_restart_replay_observed":true,"reviewer_separation_observed":true,"dynamic_workflow_checkpoint_observed":true,"kill_switch_observed":true,"tenant_fairness_observed":true,"backpressure_observed":true,"performance_budget_observed":true,"worker_process_start_requested":true}"#,
            )
            .expect("rejected worker driver");
        assert!(
            rejected
                .body
                .contains("DXR_WORKER_DRIVER_REJECTED_SECURITY_BOUNDARY")
        );
        assert!(rejected.body.contains("\"provider_calls_allowed\":false"));

        let projection = runtime.runs_json();
        assert!(projection.contains("LIVE-LOCAL-DXR-WORKER-DRIVER-FLOOR"));
        assert!(projection.contains("\"driver_run_count\":2"));
        assert!(projection.contains("\"accepted_count\":1"));
        assert!(projection.contains("\"backpressure_count\":1"));
        assert!(projection.contains("\"rejected_count\":1"));
        assert!(
            projection
                .contains("\"scale_policy\":\"tenant_fair_1000_engineers_5000_forge_builds\"")
        );
        assert!(projection.contains("\"production_writes_allowed\":false"));
    }
}
