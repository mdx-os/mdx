use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_TARGET_ENGINEERS: usize = 1000;
const DEFAULT_PEAK_PARALLEL_FORGE_BUILDS: usize = 5000;
const DEFAULT_SCHEDULED_BUILD_COUNT: usize = 5000;
const DEFAULT_SCHEDULABLE_BUILD_COUNT: usize = 2048;
const DEFAULT_NEXT_DISPATCH_BATCH_SIZE: usize = 320;
const DEFAULT_DISPATCH_WINDOW_COUNT: usize = 7;
const DEFAULT_RETAINED_BACKPRESSURE_COUNT: usize = 2952;
const DEFAULT_ACTIVE_TENANT_COUNT: usize = 10;
const DEFAULT_LEASE_DURATION_MS: usize = 30_000;
const DEFAULT_HEARTBEAT_INTERVAL_MS: usize = 5_000;
const DEFAULT_HEARTBEAT_SWEEP_INTERVAL_MS: usize = 2_500;
const DEFAULT_STALE_RECOVERY_WINDOW_MS: usize = 30_000;
const DEFAULT_PROVIDER_STREAM_SLOTS: usize = 512;
const DEFAULT_WORKFLOW_SCRIPT_SLOTS: usize = 320;
const DEFAULT_MAX_SUPERVISED_LEASES_PER_SWEEP: usize = 2048;

#[derive(Default)]
pub struct DxrExecutionSupervisionRuntime {
    runs: Vec<DxrExecutionSupervisionRun>,
    next_run: usize,
}

pub struct DxrExecutionSupervisionResult {
    pub body: String,
    pub events: Vec<DxrExecutionSupervisionRuntimeEvent>,
}

pub struct DxrExecutionSupervisionRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrExecutionSupervisionRun {
    sequence: usize,
    supervision_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    idempotency_key: String,
    execution_schedule_projection_observed: bool,
    dispatch_claim_observed: bool,
    dispatch_heartbeat_observed: bool,
    dispatch_recovery_plan_observed: bool,
    dynamic_workflow_control_recovery_observed: bool,
    model_streaming_observed: bool,
    provider_failover_observed: bool,
    event_relay_observed: bool,
    durable_state_observed: bool,
    evidence_chain_observed: bool,
    sandbox_preflight_observed: bool,
    tool_policy_observed: bool,
    reviewer_separation_observed: bool,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    scheduled_build_count: usize,
    schedulable_build_count: usize,
    next_dispatch_batch_size: usize,
    dispatch_window_count: usize,
    retained_backpressure_count: usize,
    active_tenant_count: usize,
    lease_duration_ms: usize,
    heartbeat_interval_ms: usize,
    heartbeat_sweep_interval_ms: usize,
    stale_recovery_window_ms: usize,
    max_supervised_leases_per_sweep: usize,
    provider_stream_slots: usize,
    workflow_script_slots: usize,
    heartbeat_sweep_count: usize,
    provider_stream_batches_required: usize,
    workflow_batches_required: usize,
    stale_recovery_batch_count: usize,
    supervision_decision: String,
    terminal_state: String,
    rejected: bool,
    rejection_reason: String,
}

struct ExecutionSupervisionRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    idempotency_key: String,
    execution_schedule_projection_observed: bool,
    dispatch_claim_observed: bool,
    dispatch_heartbeat_observed: bool,
    dispatch_recovery_plan_observed: bool,
    dynamic_workflow_control_recovery_observed: bool,
    model_streaming_observed: bool,
    provider_failover_observed: bool,
    event_relay_observed: bool,
    durable_state_observed: bool,
    evidence_chain_observed: bool,
    sandbox_preflight_observed: bool,
    tool_policy_observed: bool,
    reviewer_separation_observed: bool,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    scheduled_build_count: usize,
    schedulable_build_count: usize,
    next_dispatch_batch_size: usize,
    dispatch_window_count: usize,
    retained_backpressure_count: usize,
    active_tenant_count: usize,
    lease_duration_ms: usize,
    heartbeat_interval_ms: usize,
    heartbeat_sweep_interval_ms: usize,
    stale_recovery_window_ms: usize,
    max_supervised_leases_per_sweep: usize,
    provider_stream_slots: usize,
    workflow_script_slots: usize,
    worker_process_start_requested: bool,
    sandbox_process_start_requested: bool,
    provider_call_requested: bool,
    tool_execution_requested: bool,
    network_requested: bool,
    secret_inheritance_requested: bool,
    filesystem_mutation_requested: bool,
    production_write_requested: bool,
}

impl DxrExecutionSupervisionRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrExecutionSupervisionResult, String> {
        let request = parse_execution_supervision_request(body)?;
        self.next_run += 1;

        let authority_requested = request.worker_process_start_requested
            || request.sandbox_process_start_requested
            || request.provider_call_requested
            || request.tool_execution_requested
            || request.network_requested
            || request.secret_inheritance_requested
            || request.filesystem_mutation_requested
            || request.production_write_requested;
        let missing_evidence = request.idempotency_key.trim().is_empty()
            || !request.execution_schedule_projection_observed
            || !request.dispatch_claim_observed
            || !request.dispatch_heartbeat_observed
            || !request.dispatch_recovery_plan_observed
            || !request.dynamic_workflow_control_recovery_observed
            || !request.model_streaming_observed
            || !request.provider_failover_observed
            || !request.event_relay_observed
            || !request.durable_state_observed
            || !request.evidence_chain_observed
            || !request.sandbox_preflight_observed
            || !request.tool_policy_observed
            || !request.reviewer_separation_observed
            || request.scheduled_build_count == 0
            || request.heartbeat_interval_ms == 0
            || request.heartbeat_sweep_interval_ms == 0
            || request.provider_stream_slots == 0
            || request.workflow_script_slots == 0;
        let heartbeat_sweep_count = ceil_div(
            request.schedulable_build_count,
            request.max_supervised_leases_per_sweep.max(1),
        );
        let provider_stream_batches_required = ceil_div(
            request.scheduled_build_count,
            request.provider_stream_slots.max(1),
        );
        let workflow_batches_required = ceil_div(
            request.scheduled_build_count,
            request.workflow_script_slots.max(1),
        );
        let stale_recovery_batch_count = ceil_div(
            request.retained_backpressure_count,
            request.next_dispatch_batch_size.max(1),
        );

        let (supervision_decision, terminal_state, rejected, rejection_reason) =
            if authority_requested {
                (
                    "rejected_execution_authority_requested",
                    "DXR_EXECUTION_SUPERVISION_REJECTED_SECURITY_BOUNDARY",
                    true,
                    "supervision_cannot_start_workers_sandboxes_providers_tools_network_secrets_filesystem_or_production_writes",
                )
            } else if missing_evidence {
                (
                    "rejected_missing_supervision_evidence",
                    "DXR_EXECUTION_SUPERVISION_REJECTED_MISSING_EVIDENCE",
                    true,
                    "schedule_dispatch_recovery_workflow_streaming_failover_relay_durable_evidence_sandbox_tool_or_reviewer_gate_missing",
                )
            } else if request.retained_backpressure_count > 0 {
                (
                    "supervised_backpressure_with_recovery_windows",
                    "DXR_EXECUTION_SUPERVISION_RECORDED_BACKPRESSURE_SUPERVISED",
                    false,
                    "none",
                )
            } else {
                (
                    "supervised_under_capacity",
                    "DXR_EXECUTION_SUPERVISION_RECORDED_UNDER_CAPACITY",
                    false,
                    "none",
                )
            };

        let run = DxrExecutionSupervisionRun {
            sequence: self.next_run,
            supervision_id: format!("dxr_execution_supervision_{:06}", self.next_run),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            idempotency_key: request.idempotency_key,
            execution_schedule_projection_observed: request.execution_schedule_projection_observed,
            dispatch_claim_observed: request.dispatch_claim_observed,
            dispatch_heartbeat_observed: request.dispatch_heartbeat_observed,
            dispatch_recovery_plan_observed: request.dispatch_recovery_plan_observed,
            dynamic_workflow_control_recovery_observed: request
                .dynamic_workflow_control_recovery_observed,
            model_streaming_observed: request.model_streaming_observed,
            provider_failover_observed: request.provider_failover_observed,
            event_relay_observed: request.event_relay_observed,
            durable_state_observed: request.durable_state_observed,
            evidence_chain_observed: request.evidence_chain_observed,
            sandbox_preflight_observed: request.sandbox_preflight_observed,
            tool_policy_observed: request.tool_policy_observed,
            reviewer_separation_observed: request.reviewer_separation_observed,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            scheduled_build_count: request.scheduled_build_count,
            schedulable_build_count: request.schedulable_build_count,
            next_dispatch_batch_size: request.next_dispatch_batch_size,
            dispatch_window_count: request.dispatch_window_count,
            retained_backpressure_count: request.retained_backpressure_count,
            active_tenant_count: request.active_tenant_count,
            lease_duration_ms: request.lease_duration_ms,
            heartbeat_interval_ms: request.heartbeat_interval_ms,
            heartbeat_sweep_interval_ms: request.heartbeat_sweep_interval_ms,
            stale_recovery_window_ms: request.stale_recovery_window_ms,
            max_supervised_leases_per_sweep: request.max_supervised_leases_per_sweep,
            provider_stream_slots: request.provider_stream_slots,
            workflow_script_slots: request.workflow_script_slots,
            heartbeat_sweep_count,
            provider_stream_batches_required,
            workflow_batches_required,
            stale_recovery_batch_count,
            supervision_decision: supervision_decision.to_string(),
            terminal_state: terminal_state.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let events = execution_supervision_events(&run);
        let body = render_execution_supervision_response_json(&run);
        self.runs.push(run);
        Ok(DxrExecutionSupervisionResult { body, events })
    }

    pub fn supervision_json(&self) -> String {
        let supervised_count = self.runs.iter().filter(|run| !run.rejected).count();
        let backpressure_count = self
            .runs
            .iter()
            .filter(|run| run.retained_backpressure_count > 0 && !run.rejected)
            .count();
        let rejected_count = self.runs.len().saturating_sub(supervised_count);
        format!(
            r#"{{"name":"mdx-dxr-execution-supervision","status":"LIVE-LOCAL-DXR-EXECUTION-SUPERVISION-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/execution-supervision.json","submit_route":"/v1/dxr/execution-supervision-runs","supervision_count":{},"supervised_count":{},"backpressure_count":{},"rejected_count":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"scheduled_build_count":{},"schedulable_build_count":{},"retained_backpressure_count":{},"active_tenant_count":{},"lease_duration_ms":{},"heartbeat_interval_ms":{},"heartbeat_sweep_interval_ms":{},"stale_recovery_window_ms":{},"max_supervised_leases_per_sweep":{},"provider_stream_slots":{},"workflow_script_slots":{},"supervision_policy":"lease_heartbeat_recovery_before_worker_start","provider_policy":"streaming_with_failover_before_live_provider_authority","evidence_policy":"relay_and_durable_hash_chain_required","operator_view_policy":"progress_and_recovery_projection_without_live_execution","worker_process_started":false,"sandbox_process_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"production_writes_allowed":false,"runs":[{}]}}"#,
            self.runs.len(),
            supervised_count,
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
                .map(|run| run.active_tenant_count)
                .unwrap_or(DEFAULT_ACTIVE_TENANT_COUNT),
            self.runs
                .last()
                .map(|run| run.lease_duration_ms)
                .unwrap_or(DEFAULT_LEASE_DURATION_MS),
            self.runs
                .last()
                .map(|run| run.heartbeat_interval_ms)
                .unwrap_or(DEFAULT_HEARTBEAT_INTERVAL_MS),
            self.runs
                .last()
                .map(|run| run.heartbeat_sweep_interval_ms)
                .unwrap_or(DEFAULT_HEARTBEAT_SWEEP_INTERVAL_MS),
            self.runs
                .last()
                .map(|run| run.stale_recovery_window_ms)
                .unwrap_or(DEFAULT_STALE_RECOVERY_WINDOW_MS),
            self.runs
                .last()
                .map(|run| run.max_supervised_leases_per_sweep)
                .unwrap_or(DEFAULT_MAX_SUPERVISED_LEASES_PER_SWEEP),
            self.runs
                .last()
                .map(|run| run.provider_stream_slots)
                .unwrap_or(DEFAULT_PROVIDER_STREAM_SLOTS),
            self.runs
                .last()
                .map(|run| run.workflow_script_slots)
                .unwrap_or(DEFAULT_WORKFLOW_SCRIPT_SLOTS),
            self.runs
                .iter()
                .map(render_execution_supervision_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn parse_execution_supervision_request(body: &str) -> Result<ExecutionSupervisionRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR execution supervision json: {error}"))?;
    Ok(ExecutionSupervisionRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", "dxr_job_execution_supervision_001"),
        run_id: string_value(&value, "run_id", "dxr_run_execution_supervision_001"),
        idempotency_key: string_value(&value, "idempotency_key", ""),
        execution_schedule_projection_observed: bool_value(
            &value,
            "execution_schedule_projection_observed",
            false,
        ),
        dispatch_claim_observed: bool_value(&value, "dispatch_claim_observed", false),
        dispatch_heartbeat_observed: bool_value(&value, "dispatch_heartbeat_observed", false),
        dispatch_recovery_plan_observed: bool_value(
            &value,
            "dispatch_recovery_plan_observed",
            false,
        ),
        dynamic_workflow_control_recovery_observed: bool_value(
            &value,
            "dynamic_workflow_control_recovery_observed",
            false,
        ),
        model_streaming_observed: bool_value(&value, "model_streaming_observed", false),
        provider_failover_observed: bool_value(&value, "provider_failover_observed", false),
        event_relay_observed: bool_value(&value, "event_relay_observed", false),
        durable_state_observed: bool_value(&value, "durable_state_observed", false),
        evidence_chain_observed: bool_value(&value, "evidence_chain_observed", false),
        sandbox_preflight_observed: bool_value(&value, "sandbox_preflight_observed", false),
        tool_policy_observed: bool_value(&value, "tool_policy_observed", false),
        reviewer_separation_observed: bool_value(&value, "reviewer_separation_observed", false),
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
        next_dispatch_batch_size: usize_value(
            &value,
            "next_dispatch_batch_size",
            DEFAULT_NEXT_DISPATCH_BATCH_SIZE,
        ),
        dispatch_window_count: usize_value(
            &value,
            "dispatch_window_count",
            DEFAULT_DISPATCH_WINDOW_COUNT,
        ),
        retained_backpressure_count: usize_value(
            &value,
            "retained_backpressure_count",
            DEFAULT_RETAINED_BACKPRESSURE_COUNT,
        ),
        active_tenant_count: usize_value(
            &value,
            "active_tenant_count",
            DEFAULT_ACTIVE_TENANT_COUNT,
        ),
        lease_duration_ms: usize_value(&value, "lease_duration_ms", DEFAULT_LEASE_DURATION_MS),
        heartbeat_interval_ms: usize_value(
            &value,
            "heartbeat_interval_ms",
            DEFAULT_HEARTBEAT_INTERVAL_MS,
        ),
        heartbeat_sweep_interval_ms: usize_value(
            &value,
            "heartbeat_sweep_interval_ms",
            DEFAULT_HEARTBEAT_SWEEP_INTERVAL_MS,
        ),
        stale_recovery_window_ms: usize_value(
            &value,
            "stale_recovery_window_ms",
            DEFAULT_STALE_RECOVERY_WINDOW_MS,
        ),
        max_supervised_leases_per_sweep: usize_value(
            &value,
            "max_supervised_leases_per_sweep",
            DEFAULT_MAX_SUPERVISED_LEASES_PER_SWEEP,
        ),
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
        worker_process_start_requested: bool_value(&value, "worker_process_start_requested", false),
        sandbox_process_start_requested: bool_value(
            &value,
            "sandbox_process_start_requested",
            false,
        ),
        provider_call_requested: bool_value(&value, "provider_call_requested", false),
        tool_execution_requested: bool_value(&value, "tool_execution_requested", false),
        network_requested: bool_value(&value, "network_requested", false),
        secret_inheritance_requested: bool_value(&value, "secret_inheritance_requested", false),
        filesystem_mutation_requested: bool_value(&value, "filesystem_mutation_requested", false),
        production_write_requested: bool_value(&value, "production_write_requested", false),
    })
}

fn execution_supervision_events(
    run: &DxrExecutionSupervisionRun,
) -> Vec<DxrExecutionSupervisionRuntimeEvent> {
    let mut events = vec![execution_supervision_event(
        run,
        "execution_supervision_recorded",
    )];
    if run.rejected {
        events.push(execution_supervision_event(
            run,
            "execution_supervision_rejected",
        ));
    } else {
        events.extend([
            execution_supervision_event(run, "execution_supervision_lease_window_planned"),
            execution_supervision_event(run, "execution_supervision_heartbeat_sweep_planned"),
            execution_supervision_event(run, "execution_supervision_stale_recovery_planned"),
            execution_supervision_event(run, "execution_supervision_provider_failover_retained"),
            execution_supervision_event(run, "execution_supervision_evidence_chain_retained"),
        ]);
        if run.retained_backpressure_count > 0 {
            events.push(execution_supervision_event(
                run,
                "execution_supervision_backpressure_retained",
            ));
        }
    }
    events.push(execution_supervision_event(
        run,
        "execution_supervision_authority_remained_blocked",
    ));
    events
}

fn execution_supervision_event(
    run: &DxrExecutionSupervisionRun,
    event_type: &str,
) -> DxrExecutionSupervisionRuntimeEvent {
    DxrExecutionSupervisionRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: run.tenant_id.clone(),
        job_id: run.job_id.clone(),
        run_id: run.run_id.clone(),
        actor_id: run.actor_id.clone(),
    }
}

fn render_execution_supervision_response_json(run: &DxrExecutionSupervisionRun) -> String {
    format!(
        r#"{{"name":"mdx-dxr-execution-supervision-run","status":"LIVE-LOCAL-DXR-EXECUTION-SUPERVISION-FLOOR","runtime":"mdx-dxr-engine","supervision":{},"supervision_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"terminal_state":{},"supervision_decision":{},"idempotency_key":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"scheduled_build_count":{},"schedulable_build_count":{},"next_dispatch_batch_size":{},"dispatch_window_count":{},"retained_backpressure_count":{},"heartbeat_sweep_count":{},"provider_stream_batches_required":{},"workflow_batches_required":{},"stale_recovery_batch_count":{},"lease_duration_ms":{},"heartbeat_interval_ms":{},"heartbeat_sweep_interval_ms":{},"stale_recovery_window_ms":{},"max_supervised_leases_per_sweep":{},"execution_schedule_projection_required":true,"dispatch_claim_required":true,"dispatch_heartbeat_required":true,"dispatch_recovery_plan_required":true,"dynamic_workflow_control_recovery_required":true,"model_streaming_required":true,"provider_failover_required":true,"event_relay_required":true,"durable_state_required":true,"evidence_chain_required":true,"sandbox_preflight_required":true,"tool_policy_required":true,"reviewer_separation_required":true,"rejected":{},"rejection_reason":{},"worker_process_started":false,"sandbox_process_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"production_writes_allowed":false}}"#,
        render_execution_supervision_json(run),
        json_string_literal(&run.supervision_id),
        json_string_literal(&run.tenant_id),
        json_string_literal(&run.actor_id),
        json_string_literal(&run.job_id),
        json_string_literal(&run.run_id),
        json_string_literal(&run.terminal_state),
        json_string_literal(&run.supervision_decision),
        json_string_literal(&run.idempotency_key),
        run.target_concurrent_engineers,
        run.peak_parallel_forge_builds,
        run.scheduled_build_count,
        run.schedulable_build_count,
        run.next_dispatch_batch_size,
        run.dispatch_window_count,
        run.retained_backpressure_count,
        run.heartbeat_sweep_count,
        run.provider_stream_batches_required,
        run.workflow_batches_required,
        run.stale_recovery_batch_count,
        run.lease_duration_ms,
        run.heartbeat_interval_ms,
        run.heartbeat_sweep_interval_ms,
        run.stale_recovery_window_ms,
        run.max_supervised_leases_per_sweep,
        run.rejected,
        json_string_literal(&run.rejection_reason)
    )
}

fn render_execution_supervision_json(run: &DxrExecutionSupervisionRun) -> String {
    format!(
        r#"{{"sequence":{},"supervision_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"idempotency_key":{},"execution_schedule_projection_observed":{},"dispatch_claim_observed":{},"dispatch_heartbeat_observed":{},"dispatch_recovery_plan_observed":{},"dynamic_workflow_control_recovery_observed":{},"model_streaming_observed":{},"provider_failover_observed":{},"event_relay_observed":{},"durable_state_observed":{},"evidence_chain_observed":{},"sandbox_preflight_observed":{},"tool_policy_observed":{},"reviewer_separation_observed":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"scheduled_build_count":{},"schedulable_build_count":{},"next_dispatch_batch_size":{},"dispatch_window_count":{},"retained_backpressure_count":{},"active_tenant_count":{},"lease_duration_ms":{},"heartbeat_interval_ms":{},"heartbeat_sweep_interval_ms":{},"stale_recovery_window_ms":{},"max_supervised_leases_per_sweep":{},"provider_stream_slots":{},"workflow_script_slots":{},"heartbeat_sweep_count":{},"provider_stream_batches_required":{},"workflow_batches_required":{},"stale_recovery_batch_count":{},"supervision_decision":{},"terminal_state":{},"rejected":{},"rejection_reason":{},"supervision_policy":"lease_heartbeat_recovery_before_worker_start","provider_policy":"streaming_with_failover_before_live_provider_authority","evidence_policy":"relay_and_durable_hash_chain_required","worker_process_started":false,"sandbox_process_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"production_writes_allowed":false}}"#,
        run.sequence,
        json_string_literal(&run.supervision_id),
        json_string_literal(&run.tenant_id),
        json_string_literal(&run.actor_id),
        json_string_literal(&run.job_id),
        json_string_literal(&run.run_id),
        json_string_literal(&run.idempotency_key),
        run.execution_schedule_projection_observed,
        run.dispatch_claim_observed,
        run.dispatch_heartbeat_observed,
        run.dispatch_recovery_plan_observed,
        run.dynamic_workflow_control_recovery_observed,
        run.model_streaming_observed,
        run.provider_failover_observed,
        run.event_relay_observed,
        run.durable_state_observed,
        run.evidence_chain_observed,
        run.sandbox_preflight_observed,
        run.tool_policy_observed,
        run.reviewer_separation_observed,
        run.target_concurrent_engineers,
        run.peak_parallel_forge_builds,
        run.scheduled_build_count,
        run.schedulable_build_count,
        run.next_dispatch_batch_size,
        run.dispatch_window_count,
        run.retained_backpressure_count,
        run.active_tenant_count,
        run.lease_duration_ms,
        run.heartbeat_interval_ms,
        run.heartbeat_sweep_interval_ms,
        run.stale_recovery_window_ms,
        run.max_supervised_leases_per_sweep,
        run.provider_stream_slots,
        run.workflow_script_slots,
        run.heartbeat_sweep_count,
        run.provider_stream_batches_required,
        run.workflow_batches_required,
        run.stale_recovery_batch_count,
        json_string_literal(&run.supervision_decision),
        json_string_literal(&run.terminal_state),
        run.rejected,
        json_string_literal(&run.rejection_reason)
    )
}

fn ceil_div(numerator: usize, denominator: usize) -> usize {
    if numerator == 0 {
        0
    } else {
        1 + ((numerator - 1) / denominator.max(1))
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
        .map(|value| value as usize)
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_supervision_records_scale_recovery_and_rejects_authority() {
        let mut runtime = DxrExecutionSupervisionRuntime::new();
        let accepted = runtime
            .submit_json(
                r#"{"idempotency_key":"supervision-001","execution_schedule_projection_observed":true,"dispatch_claim_observed":true,"dispatch_heartbeat_observed":true,"dispatch_recovery_plan_observed":true,"dynamic_workflow_control_recovery_observed":true,"model_streaming_observed":true,"provider_failover_observed":true,"event_relay_observed":true,"durable_state_observed":true,"evidence_chain_observed":true,"sandbox_preflight_observed":true,"tool_policy_observed":true,"reviewer_separation_observed":true}"#,
            )
            .expect("accepted supervision");
        assert!(
            accepted
                .body
                .contains("DXR_EXECUTION_SUPERVISION_RECORDED_BACKPRESSURE_SUPERVISED")
        );
        assert!(accepted.body.contains("\"heartbeat_sweep_count\":1"));
        assert!(
            accepted
                .body
                .contains("\"provider_stream_batches_required\":10")
        );
        assert!(accepted.body.contains("\"workflow_batches_required\":16"));
        assert!(accepted.body.contains("\"stale_recovery_batch_count\":10"));
        assert!(accepted.body.contains("\"worker_process_started\":false"));
        assert!(accepted.body.contains("\"provider_calls_allowed\":false"));

        let rejected = runtime
            .submit_json(
                r#"{"idempotency_key":"supervision-002","execution_schedule_projection_observed":true,"dispatch_claim_observed":true,"dispatch_heartbeat_observed":true,"dispatch_recovery_plan_observed":true,"dynamic_workflow_control_recovery_observed":true,"model_streaming_observed":true,"provider_failover_observed":true,"event_relay_observed":true,"durable_state_observed":true,"evidence_chain_observed":true,"sandbox_preflight_observed":true,"tool_policy_observed":true,"reviewer_separation_observed":true,"worker_process_start_requested":true}"#,
            )
            .expect("rejected supervision");
        assert!(
            rejected
                .body
                .contains("DXR_EXECUTION_SUPERVISION_REJECTED_SECURITY_BOUNDARY")
        );
        assert!(rejected.body.contains("\"worker_process_started\":false"));

        let projection = runtime.supervision_json();
        assert!(projection.contains("LIVE-LOCAL-DXR-EXECUTION-SUPERVISION-FLOOR"));
        assert!(projection.contains("\"supervision_count\":2"));
        assert!(projection.contains("\"supervised_count\":1"));
        assert!(projection.contains("\"backpressure_count\":1"));
        assert!(projection.contains("\"rejected_count\":1"));
        assert!(projection.contains("\"production_writes_allowed\":false"));
    }
}
