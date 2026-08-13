use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_TARGET_ENGINEERS: usize = 1000;
const DEFAULT_PEAK_PARALLEL_FORGE_BUILDS: usize = 5000;
const DEFAULT_ACTIVE_TENANT_COUNT: usize = 10;
const DEFAULT_TENANT_FORGE_RUN_LIMIT: usize = 500;
const DEFAULT_GLOBAL_FORGE_RUN_LIMIT: usize = 5000;
const DEFAULT_SANDBOX_POOL_SIZE: usize = 2048;
const DEFAULT_QUEUE_DEPTH_LIMIT: usize = 50_000;
const DEFAULT_WORKFLOW_SCRIPT_SLOTS: usize = 320;
const DEFAULT_PROVIDER_STREAM_SLOTS: usize = 512;
const DEFAULT_LEASE_DURATION_MS: usize = 30_000;
const DEFAULT_HEARTBEAT_INTERVAL_MS: usize = 5_000;
const DEFAULT_HOT_PATH_BUDGET_MS: usize = 20;

#[derive(Default)]
pub struct DxrExecutionSchedulerRuntime {
    schedules: Vec<DxrExecutionSchedule>,
    next_schedule: usize,
}

pub struct DxrExecutionSchedulerResult {
    pub body: String,
    pub events: Vec<DxrExecutionSchedulerRuntimeEvent>,
}

pub struct DxrExecutionSchedulerRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrExecutionSchedule {
    sequence: usize,
    schedule_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    idempotency_key: String,
    execution_admission_projection_observed: bool,
    dispatch_recovery_plan_observed: bool,
    scale_orchestration_observed: bool,
    reviewer_separation_observed: bool,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    active_tenant_count: usize,
    staged_admission_count: usize,
    queued_admission_count: usize,
    rejected_admission_count: usize,
    global_max_concurrent_forge_runs: usize,
    max_concurrent_forge_runs_per_tenant: usize,
    sandbox_pool_size: usize,
    queue_depth_limit: usize,
    workflow_script_slots: usize,
    provider_stream_slots: usize,
    lease_duration_ms: usize,
    heartbeat_interval_ms: usize,
    hot_path_budget_ms: usize,
    tenant_fair_share: usize,
    global_schedulable_count: usize,
    next_dispatch_batch_size: usize,
    dispatch_window_count: usize,
    retained_backpressure_count: usize,
    overflow_rejected_count: usize,
    provider_stream_batches_required: usize,
    workflow_batches_required: usize,
    scheduling_decision: String,
    terminal_state: String,
    rejected: bool,
    rejection_reason: String,
}

struct ExecutionScheduleRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    idempotency_key: String,
    execution_admission_projection_observed: bool,
    dispatch_recovery_plan_observed: bool,
    scale_orchestration_observed: bool,
    reviewer_separation_observed: bool,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    active_tenant_count: usize,
    staged_admission_count: usize,
    queued_admission_count: usize,
    rejected_admission_count: usize,
    global_max_concurrent_forge_runs: usize,
    max_concurrent_forge_runs_per_tenant: usize,
    sandbox_pool_size: usize,
    queue_depth_limit: usize,
    workflow_script_slots: usize,
    provider_stream_slots: usize,
    lease_duration_ms: usize,
    heartbeat_interval_ms: usize,
    hot_path_budget_ms: usize,
    worker_process_start_requested: bool,
    sandbox_process_start_requested: bool,
    provider_calls_requested: bool,
    tool_execution_requested: bool,
    network_requested: bool,
    secret_inheritance_requested: bool,
    filesystem_mutation_requested: bool,
    production_write_requested: bool,
}

impl DxrExecutionSchedulerRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrExecutionSchedulerResult, String> {
        let request = parse_execution_schedule_request(body)?;
        self.next_schedule += 1;

        let security_rejected = request.worker_process_start_requested
            || request.sandbox_process_start_requested
            || request.provider_calls_requested
            || request.tool_execution_requested
            || request.network_requested
            || request.secret_inheritance_requested
            || request.filesystem_mutation_requested
            || request.production_write_requested;
        let evidence_missing = !request.execution_admission_projection_observed
            || !request.dispatch_recovery_plan_observed
            || !request.scale_orchestration_observed
            || !request.reviewer_separation_observed
            || request.idempotency_key.trim().is_empty()
            || request.workflow_script_slots == 0
            || request.provider_stream_slots == 0
            || request.active_tenant_count == 0;
        let tenant_capacity = request
            .active_tenant_count
            .saturating_mul(request.max_concurrent_forge_runs_per_tenant);
        let global_schedulable_count = request
            .staged_admission_count
            .min(request.peak_parallel_forge_builds)
            .min(request.global_max_concurrent_forge_runs)
            .min(tenant_capacity)
            .min(request.sandbox_pool_size);
        let tenant_fair_share =
            ceil_div(global_schedulable_count, request.active_tenant_count.max(1))
                .min(request.max_concurrent_forge_runs_per_tenant);
        let next_dispatch_batch_size = global_schedulable_count
            .min(request.workflow_script_slots)
            .min(request.provider_stream_slots);
        let retained_backpressure_count = request
            .staged_admission_count
            .saturating_sub(global_schedulable_count)
            .saturating_add(request.queued_admission_count);
        let overflow_rejected_count =
            retained_backpressure_count.saturating_sub(request.queue_depth_limit);
        let dispatch_window_count =
            ceil_div(global_schedulable_count, next_dispatch_batch_size.max(1));
        let provider_stream_batches_required = ceil_div(
            request.staged_admission_count,
            request.provider_stream_slots.max(1),
        );
        let workflow_batches_required = ceil_div(
            request.staged_admission_count,
            request.workflow_script_slots.max(1),
        );
        let (scheduling_decision, terminal_state, rejected, rejection_reason) = if security_rejected
        {
            (
                "rejected_security_boundary",
                "DXR_EXECUTION_SCHEDULE_REJECTED_SECURITY_BOUNDARY",
                true,
                "execution_sandbox_provider_tool_network_secret_filesystem_or_production_authority_requested",
            )
        } else if evidence_missing {
            (
                "rejected_missing_schedule_evidence",
                "DXR_EXECUTION_SCHEDULE_REJECTED_MISSING_EVIDENCE",
                true,
                "execution_admission_dispatch_scale_reviewer_or_idempotency_evidence_missing",
            )
        } else if overflow_rejected_count > 0 {
            (
                "rejected_queue_overflow",
                "DXR_EXECUTION_SCHEDULE_REJECTED_QUEUE_OVERFLOW",
                true,
                "retained_backpressure_exceeds_queue_depth_limit",
            )
        } else if global_schedulable_count == 0 {
            (
                "queued_waiting_for_staged_admissions",
                "DXR_EXECUTION_SCHEDULE_QUEUED_WAITING_FOR_STAGED_ADMISSIONS",
                false,
                "none",
            )
        } else if retained_backpressure_count > 0 {
            (
                "scheduled_tenant_fair_window_with_retained_backpressure",
                "DXR_EXECUTION_SCHEDULE_PLANNED_BACKPRESSURE_RETAINED",
                false,
                "none",
            )
        } else {
            (
                "scheduled_tenant_fair_window_under_capacity",
                "DXR_EXECUTION_SCHEDULE_PLANNED_UNDER_CAPACITY",
                false,
                "none",
            )
        };

        let schedule = DxrExecutionSchedule {
            sequence: self.next_schedule,
            schedule_id: format!("dxr_execution_schedule_{:06}", self.next_schedule),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            idempotency_key: request.idempotency_key,
            execution_admission_projection_observed: request
                .execution_admission_projection_observed,
            dispatch_recovery_plan_observed: request.dispatch_recovery_plan_observed,
            scale_orchestration_observed: request.scale_orchestration_observed,
            reviewer_separation_observed: request.reviewer_separation_observed,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            active_tenant_count: request.active_tenant_count,
            staged_admission_count: request.staged_admission_count,
            queued_admission_count: request.queued_admission_count,
            rejected_admission_count: request.rejected_admission_count,
            global_max_concurrent_forge_runs: request.global_max_concurrent_forge_runs,
            max_concurrent_forge_runs_per_tenant: request.max_concurrent_forge_runs_per_tenant,
            sandbox_pool_size: request.sandbox_pool_size,
            queue_depth_limit: request.queue_depth_limit,
            workflow_script_slots: request.workflow_script_slots,
            provider_stream_slots: request.provider_stream_slots,
            lease_duration_ms: request.lease_duration_ms,
            heartbeat_interval_ms: request.heartbeat_interval_ms,
            hot_path_budget_ms: request.hot_path_budget_ms,
            tenant_fair_share,
            global_schedulable_count,
            next_dispatch_batch_size,
            dispatch_window_count,
            retained_backpressure_count,
            overflow_rejected_count,
            provider_stream_batches_required,
            workflow_batches_required,
            scheduling_decision: scheduling_decision.to_string(),
            terminal_state: terminal_state.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let events = execution_schedule_events(&schedule);
        let body = render_execution_schedule_response_json(&schedule);
        self.schedules.push(schedule);
        Ok(DxrExecutionSchedulerResult { body, events })
    }

    pub fn schedules_json(&self) -> String {
        let planned_count = self
            .schedules
            .iter()
            .filter(|schedule| !schedule.rejected)
            .count();
        let backpressure_count = self
            .schedules
            .iter()
            .filter(|schedule| schedule.retained_backpressure_count > 0 && !schedule.rejected)
            .count();
        let rejected_count = self
            .schedules
            .iter()
            .filter(|schedule| schedule.rejected)
            .count();
        format!(
            r#"{{"name":"mdx-dxr-execution-schedules","status":"LIVE-LOCAL-DXR-EXECUTION-SCHEDULER-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/execution-schedules.json","submit_route":"/v1/dxr/execution-schedules","schedule_count":{},"planned_count":{},"backpressure_count":{},"rejected_count":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"active_tenant_count":{},"tenant_fairness_policy":"tenant_weighted_fair_dispatch_window","dispatch_policy":"lease_then_heartbeat_before_worker_start","idempotency_required":true,"execution_admission_projection_required":true,"dispatch_recovery_plan_required":true,"scale_orchestration_required":true,"reviewer_separation_required":true,"worker_process_started":false,"sandbox_process_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"production_writes_allowed":false,"schedules":[{}]}}"#,
            self.schedules.len(),
            planned_count,
            backpressure_count,
            rejected_count,
            self.schedules
                .last()
                .map(|schedule| schedule.target_concurrent_engineers)
                .unwrap_or(DEFAULT_TARGET_ENGINEERS),
            self.schedules
                .last()
                .map(|schedule| schedule.peak_parallel_forge_builds)
                .unwrap_or(DEFAULT_PEAK_PARALLEL_FORGE_BUILDS),
            self.schedules
                .last()
                .map(|schedule| schedule.active_tenant_count)
                .unwrap_or(DEFAULT_ACTIVE_TENANT_COUNT),
            self.schedules
                .iter()
                .map(render_execution_schedule_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn parse_execution_schedule_request(body: &str) -> Result<ExecutionScheduleRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR execution schedule json: {error}"))?;
    Ok(ExecutionScheduleRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", "dxr_job_execution_schedule_001"),
        run_id: string_value(&value, "run_id", "dxr_run_execution_schedule_001"),
        idempotency_key: string_value(&value, "idempotency_key", ""),
        execution_admission_projection_observed: bool_value(
            &value,
            "execution_admission_projection_observed",
            false,
        ),
        dispatch_recovery_plan_observed: bool_value(
            &value,
            "dispatch_recovery_plan_observed",
            false,
        ),
        scale_orchestration_observed: bool_value(&value, "scale_orchestration_observed", false),
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
        active_tenant_count: usize_value(
            &value,
            "active_tenant_count",
            DEFAULT_ACTIVE_TENANT_COUNT,
        ),
        staged_admission_count: usize_value(&value, "staged_admission_count", 2048),
        queued_admission_count: usize_value(&value, "queued_admission_count", 2952),
        rejected_admission_count: usize_value(&value, "rejected_admission_count", 0),
        global_max_concurrent_forge_runs: usize_value(
            &value,
            "global_max_concurrent_forge_runs",
            DEFAULT_GLOBAL_FORGE_RUN_LIMIT,
        ),
        max_concurrent_forge_runs_per_tenant: usize_value(
            &value,
            "max_concurrent_forge_runs_per_tenant",
            DEFAULT_TENANT_FORGE_RUN_LIMIT,
        ),
        sandbox_pool_size: usize_value(&value, "sandbox_pool_size", DEFAULT_SANDBOX_POOL_SIZE),
        queue_depth_limit: usize_value(&value, "queue_depth_limit", DEFAULT_QUEUE_DEPTH_LIMIT),
        workflow_script_slots: usize_value(
            &value,
            "workflow_script_slots",
            DEFAULT_WORKFLOW_SCRIPT_SLOTS,
        ),
        provider_stream_slots: usize_value(
            &value,
            "provider_stream_slots",
            DEFAULT_PROVIDER_STREAM_SLOTS,
        ),
        lease_duration_ms: usize_value(&value, "lease_duration_ms", DEFAULT_LEASE_DURATION_MS),
        heartbeat_interval_ms: usize_value(
            &value,
            "heartbeat_interval_ms",
            DEFAULT_HEARTBEAT_INTERVAL_MS,
        ),
        hot_path_budget_ms: usize_value(&value, "hot_path_budget_ms", DEFAULT_HOT_PATH_BUDGET_MS),
        worker_process_start_requested: bool_value(&value, "worker_process_start_requested", false),
        sandbox_process_start_requested: bool_value(
            &value,
            "sandbox_process_start_requested",
            false,
        ),
        provider_calls_requested: bool_value(&value, "provider_calls_requested", false),
        tool_execution_requested: bool_value(&value, "tool_execution_requested", false),
        network_requested: bool_value(&value, "network_requested", false),
        secret_inheritance_requested: bool_value(&value, "secret_inheritance_requested", false),
        filesystem_mutation_requested: bool_value(&value, "filesystem_mutation_requested", false),
        production_write_requested: bool_value(&value, "production_write_requested", false),
    })
}

fn execution_schedule_events(
    schedule: &DxrExecutionSchedule,
) -> Vec<DxrExecutionSchedulerRuntimeEvent> {
    let mut events = vec![execution_schedule_event(
        schedule,
        "execution_schedule_recorded",
    )];
    if schedule.rejected {
        events.push(execution_schedule_event(
            schedule,
            "execution_schedule_rejected",
        ));
    } else {
        events.push(execution_schedule_event(
            schedule,
            "execution_schedule_dispatch_window_planned",
        ));
        if schedule.retained_backpressure_count > 0 {
            events.push(execution_schedule_event(
                schedule,
                "execution_schedule_backpressure_retained",
            ));
        }
    }
    events.push(execution_schedule_event(
        schedule,
        "execution_schedule_authority_remained_blocked",
    ));
    events
}

fn execution_schedule_event(
    schedule: &DxrExecutionSchedule,
    event_type: &str,
) -> DxrExecutionSchedulerRuntimeEvent {
    DxrExecutionSchedulerRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: schedule.tenant_id.clone(),
        job_id: schedule.job_id.clone(),
        run_id: schedule.run_id.clone(),
        actor_id: schedule.actor_id.clone(),
    }
}

fn render_execution_schedule_response_json(schedule: &DxrExecutionSchedule) -> String {
    format!(
        r#"{{"name":"mdx-dxr-execution-schedule","status":"LIVE-LOCAL-DXR-EXECUTION-SCHEDULER-FLOOR","runtime":"mdx-dxr-engine","schedule":{},"schedule_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"terminal_state":{},"scheduling_decision":{},"idempotency_key":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"active_tenant_count":{},"staged_admission_count":{},"queued_admission_count":{},"rejected_admission_count":{},"global_schedulable_count":{},"next_dispatch_batch_size":{},"dispatch_window_count":{},"tenant_fair_share":{},"retained_backpressure_count":{},"overflow_rejected_count":{},"workflow_batches_required":{},"provider_stream_batches_required":{},"lease_duration_ms":{},"heartbeat_interval_ms":{},"hot_path_budget_ms":{},"idempotency_required":true,"execution_admission_projection_required":true,"dispatch_recovery_plan_required":true,"scale_orchestration_required":true,"reviewer_separation_required":true,"rejected":{},"rejection_reason":{},"worker_process_started":false,"sandbox_process_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"production_writes_allowed":false}}"#,
        render_execution_schedule_json(schedule),
        json_string_literal(&schedule.schedule_id),
        json_string_literal(&schedule.tenant_id),
        json_string_literal(&schedule.actor_id),
        json_string_literal(&schedule.job_id),
        json_string_literal(&schedule.run_id),
        json_string_literal(&schedule.terminal_state),
        json_string_literal(&schedule.scheduling_decision),
        json_string_literal(&schedule.idempotency_key),
        schedule.target_concurrent_engineers,
        schedule.peak_parallel_forge_builds,
        schedule.active_tenant_count,
        schedule.staged_admission_count,
        schedule.queued_admission_count,
        schedule.rejected_admission_count,
        schedule.global_schedulable_count,
        schedule.next_dispatch_batch_size,
        schedule.dispatch_window_count,
        schedule.tenant_fair_share,
        schedule.retained_backpressure_count,
        schedule.overflow_rejected_count,
        schedule.workflow_batches_required,
        schedule.provider_stream_batches_required,
        schedule.lease_duration_ms,
        schedule.heartbeat_interval_ms,
        schedule.hot_path_budget_ms,
        schedule.rejected,
        json_string_literal(&schedule.rejection_reason)
    )
}

fn render_execution_schedule_json(schedule: &DxrExecutionSchedule) -> String {
    format!(
        r#"{{"sequence":{},"schedule_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"idempotency_key":{},"execution_admission_projection_observed":{},"dispatch_recovery_plan_observed":{},"scale_orchestration_observed":{},"reviewer_separation_observed":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"active_tenant_count":{},"staged_admission_count":{},"queued_admission_count":{},"rejected_admission_count":{},"global_max_concurrent_forge_runs":{},"max_concurrent_forge_runs_per_tenant":{},"sandbox_pool_size":{},"queue_depth_limit":{},"workflow_script_slots":{},"provider_stream_slots":{},"lease_duration_ms":{},"heartbeat_interval_ms":{},"hot_path_budget_ms":{},"tenant_fair_share":{},"global_schedulable_count":{},"next_dispatch_batch_size":{},"dispatch_window_count":{},"retained_backpressure_count":{},"overflow_rejected_count":{},"workflow_batches_required":{},"provider_stream_batches_required":{},"scheduling_decision":{},"terminal_state":{},"rejected":{},"rejection_reason":{},"tenant_fairness_policy":"tenant_weighted_fair_dispatch_window","dispatch_policy":"lease_then_heartbeat_before_worker_start","worker_process_started":false,"sandbox_process_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"production_writes_allowed":false}}"#,
        schedule.sequence,
        json_string_literal(&schedule.schedule_id),
        json_string_literal(&schedule.tenant_id),
        json_string_literal(&schedule.actor_id),
        json_string_literal(&schedule.job_id),
        json_string_literal(&schedule.run_id),
        json_string_literal(&schedule.idempotency_key),
        schedule.execution_admission_projection_observed,
        schedule.dispatch_recovery_plan_observed,
        schedule.scale_orchestration_observed,
        schedule.reviewer_separation_observed,
        schedule.target_concurrent_engineers,
        schedule.peak_parallel_forge_builds,
        schedule.active_tenant_count,
        schedule.staged_admission_count,
        schedule.queued_admission_count,
        schedule.rejected_admission_count,
        schedule.global_max_concurrent_forge_runs,
        schedule.max_concurrent_forge_runs_per_tenant,
        schedule.sandbox_pool_size,
        schedule.queue_depth_limit,
        schedule.workflow_script_slots,
        schedule.provider_stream_slots,
        schedule.lease_duration_ms,
        schedule.heartbeat_interval_ms,
        schedule.hot_path_budget_ms,
        schedule.tenant_fair_share,
        schedule.global_schedulable_count,
        schedule.next_dispatch_batch_size,
        schedule.dispatch_window_count,
        schedule.retained_backpressure_count,
        schedule.overflow_rejected_count,
        schedule.workflow_batches_required,
        schedule.provider_stream_batches_required,
        json_string_literal(&schedule.scheduling_decision),
        json_string_literal(&schedule.terminal_state),
        schedule.rejected,
        json_string_literal(&schedule.rejection_reason)
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
    fn execution_scheduler_plans_windows_retains_backpressure_and_rejects_authority() {
        let mut runtime = DxrExecutionSchedulerRuntime::new();
        let planned = runtime
            .submit_json(
                r#"{"idempotency_key":"execution-schedule-001","execution_admission_projection_observed":true,"dispatch_recovery_plan_observed":true,"scale_orchestration_observed":true,"reviewer_separation_observed":true,"staged_admission_count":5000,"queued_admission_count":0}"#,
            )
            .expect("planned schedule");
        assert!(
            planned
                .body
                .contains("DXR_EXECUTION_SCHEDULE_PLANNED_BACKPRESSURE_RETAINED")
        );
        assert!(planned.body.contains("\"global_schedulable_count\":2048"));
        assert!(planned.body.contains("\"next_dispatch_batch_size\":320"));
        assert!(planned.body.contains("\"dispatch_window_count\":7"));
        assert!(planned.body.contains("\"workflow_batches_required\":16"));
        assert!(
            planned
                .body
                .contains("\"provider_stream_batches_required\":10")
        );
        assert!(planned.body.contains("\"tenant_fair_share\":205"));
        assert!(
            planned
                .body
                .contains("\"retained_backpressure_count\":2952")
        );
        assert!(planned.body.contains("\"worker_process_started\":false"));
        assert!(planned.body.contains("\"sandbox_process_started\":false"));

        let waiting = runtime
            .submit_json(
                r#"{"idempotency_key":"execution-schedule-002","execution_admission_projection_observed":true,"dispatch_recovery_plan_observed":true,"scale_orchestration_observed":true,"reviewer_separation_observed":true,"staged_admission_count":0,"queued_admission_count":300}"#,
            )
            .expect("waiting schedule");
        assert!(
            waiting
                .body
                .contains("DXR_EXECUTION_SCHEDULE_QUEUED_WAITING_FOR_STAGED_ADMISSIONS")
        );

        let rejected = runtime
            .submit_json(
                r#"{"idempotency_key":"execution-schedule-003","execution_admission_projection_observed":true,"dispatch_recovery_plan_observed":true,"scale_orchestration_observed":true,"reviewer_separation_observed":true,"worker_process_start_requested":true}"#,
            )
            .expect("rejected schedule");
        assert!(
            rejected
                .body
                .contains("DXR_EXECUTION_SCHEDULE_REJECTED_SECURITY_BOUNDARY")
        );
        assert!(rejected.body.contains("\"provider_calls_allowed\":false"));

        let projection = runtime.schedules_json();
        assert!(projection.contains("LIVE-LOCAL-DXR-EXECUTION-SCHEDULER-FLOOR"));
        assert!(projection.contains("\"schedule_count\":3"));
        assert!(projection.contains("\"planned_count\":2"));
        assert!(projection.contains("\"backpressure_count\":2"));
        assert!(projection.contains("\"rejected_count\":1"));
        assert!(projection.contains("\"worker_process_started\":false"));
        assert!(projection.contains("\"production_writes_allowed\":false"));
    }
}
