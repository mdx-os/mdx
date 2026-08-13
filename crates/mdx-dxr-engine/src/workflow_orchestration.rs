use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_JOB_ID: &str = "dxr_job_dispatch_ready_001";
const DEFAULT_RUN_ID: &str = "dxr_run_dispatch_ready_001";
const DEFAULT_WORKFLOW_RUN_ID: &str = "dxr_workflow_run_000001";
const DEFAULT_DYNAMIC_WORKFLOW_PLAN_ID: &str = "dxr_dynamic_workflow_plan_000001";
const DEFAULT_EXECUTION_SCHEDULE_ID: &str = "dxr_execution_schedule_000001";
const DEFAULT_EXECUTION_SUPERVISION_ID: &str = "dxr_execution_supervision_000001";
const DEFAULT_SANDBOX_SESSION_ID: &str = "dxr_sandbox_session_000001";
const DEFAULT_SANDBOX_RESULT_CONSUMPTION_ID: &str = "dxr_sandbox_result_consumption_000001";
const DEFAULT_CLAIM_ID: &str = "dxr_dispatch_claim_000001";
const DEFAULT_CTX_CONTEXT_INPUT_ID: &str = "dxr_ctx_context_input_000001";
const DEFAULT_PROVIDER_MEMORY_INTEGRATION_ID: &str = "dxr_provider_memory_integration_000001";
const VALID_PROVIDER_MEMORY_TERMINAL_STATES: [&str; 2] = [
    "DXR_PROVIDER_MEMORY_INTEGRATION_RECORDED_LOCAL_DRIVER_READY_PROVIDER_BLOCKED",
    "DXR_PROVIDER_MEMORY_INTEGRATION_RECORDED_PROVIDER_MEMORY_READY_EXECUTION_BLOCKED",
];
const DEFAULT_TARGET_ENGINEERS: usize = 1000;
const DEFAULT_PEAK_PARALLEL_FORGE_BUILDS: usize = 5000;
const DEFAULT_ACTIVE_TENANT_COUNT: usize = 10;
const DEFAULT_WORKFLOW_SCRIPT_SLOTS: usize = 320;
const DEFAULT_PROVIDER_STREAM_SLOTS: usize = 512;
const DEFAULT_SANDBOX_POOL_SIZE: usize = 2048;
const DEFAULT_NEXT_DISPATCH_BATCH_SIZE: usize = 320;
const DEFAULT_RETAINED_BACKPRESSURE_COUNT: usize = 2952;
const DEFAULT_RETRY_BUDGET: usize = 3;
const DEFAULT_REPLAY_WINDOW_SECONDS: usize = 86_400;
const DEFAULT_RESUME_CHECKPOINT_INTERVAL_SECONDS: usize = 30;
const DEFAULT_LEASE_DURATION_MS: usize = 30_000;
const DEFAULT_HEARTBEAT_INTERVAL_MS: usize = 5_000;
const DEFAULT_HOT_PATH_BUDGET_MS: usize = 20;
const DEFAULT_ORCHESTRATION_BATCH_SIZE: usize = 320;

#[derive(Default)]
pub struct DxrWorkflowOrchestrationRuntime {
    runs: Vec<DxrWorkflowOrchestrationRun>,
    next_run: usize,
}

pub struct DxrWorkflowOrchestrationResult {
    pub body: String,
    pub events: Vec<DxrWorkflowOrchestrationRuntimeEvent>,
}

pub struct DxrWorkflowOrchestrationRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrWorkflowOrchestrationRun {
    sequence: usize,
    orchestration_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    idempotency_key: String,
    workflow_run_id: String,
    dynamic_workflow_plan_id: String,
    execution_schedule_id: String,
    execution_supervision_id: String,
    sandbox_session_id: String,
    sandbox_result_consumption_id: String,
    dispatch_claim_id: String,
    ctx_context_input_id: String,
    provider_memory_integration_id: String,
    provider_memory_terminal_state: String,
    workflow_observed: bool,
    dynamic_workflow_observed: bool,
    execution_schedule_observed: bool,
    execution_supervision_observed: bool,
    dispatch_claim_observed: bool,
    dispatch_heartbeat_observed: bool,
    dispatch_recovery_observed: bool,
    ctx_context_observed: bool,
    provider_memory_integration_observed: bool,
    provider_streaming_observed: bool,
    provider_failover_observed: bool,
    multi_judge_observed: bool,
    sandbox_authority_observed: bool,
    sandbox_session_observed: bool,
    sandbox_command_result_observed: bool,
    sandbox_result_consumption_observed: bool,
    evidence_chain_observed: bool,
    relay_event_stream_observed: bool,
    durable_state_observed: bool,
    reviewer_separation_observed: bool,
    human_ratification_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    replay_cursor_observed: bool,
    cancellation_checkpoint_observed: bool,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    active_tenant_count: usize,
    requested_workflow_count: usize,
    accepted_workflow_count: usize,
    retained_backpressure_count: usize,
    workflow_script_slots: usize,
    provider_stream_slots: usize,
    sandbox_pool_size: usize,
    next_dispatch_batch_size: usize,
    orchestration_batch_size: usize,
    dispatch_window_count: usize,
    workflow_batches_required: usize,
    provider_stream_batches_required: usize,
    sandbox_batches_required: usize,
    retry_budget: usize,
    replay_window_seconds: usize,
    resume_checkpoint_interval_seconds: usize,
    lease_duration_ms: usize,
    heartbeat_interval_ms: usize,
    hot_path_budget_ms: usize,
    orchestrated_phase_count: usize,
    cancel_requested: bool,
    replay_requested: bool,
    terminal_state: String,
    status: String,
    orchestration_decision: String,
    rejected: bool,
    rejection_reason: String,
}

struct WorkflowOrchestrationRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    idempotency_key: String,
    workflow_run_id: String,
    dynamic_workflow_plan_id: String,
    execution_schedule_id: String,
    execution_supervision_id: String,
    sandbox_session_id: String,
    sandbox_result_consumption_id: String,
    dispatch_claim_id: String,
    ctx_context_input_id: String,
    provider_memory_integration_id: String,
    provider_memory_terminal_state: String,
    workflow_observed: bool,
    dynamic_workflow_observed: bool,
    execution_schedule_observed: bool,
    execution_supervision_observed: bool,
    dispatch_claim_observed: bool,
    dispatch_heartbeat_observed: bool,
    dispatch_recovery_observed: bool,
    ctx_context_observed: bool,
    provider_memory_integration_observed: bool,
    provider_streaming_observed: bool,
    provider_failover_observed: bool,
    multi_judge_observed: bool,
    sandbox_authority_observed: bool,
    sandbox_session_observed: bool,
    sandbox_command_result_observed: bool,
    sandbox_result_consumption_observed: bool,
    evidence_chain_observed: bool,
    relay_event_stream_observed: bool,
    durable_state_observed: bool,
    reviewer_separation_observed: bool,
    human_ratification_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    replay_cursor_observed: bool,
    cancellation_checkpoint_observed: bool,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    active_tenant_count: usize,
    requested_workflow_count: usize,
    accepted_workflow_count: usize,
    retained_backpressure_count: usize,
    workflow_script_slots: usize,
    provider_stream_slots: usize,
    sandbox_pool_size: usize,
    next_dispatch_batch_size: usize,
    orchestration_batch_size: usize,
    retry_budget: usize,
    replay_window_seconds: usize,
    resume_checkpoint_interval_seconds: usize,
    lease_duration_ms: usize,
    heartbeat_interval_ms: usize,
    hot_path_budget_ms: usize,
    cancel_requested: bool,
    replay_requested: bool,
    worker_spawn_requested: bool,
    provider_calls_requested: bool,
    tool_execution_requested: bool,
    shell_execution_requested: bool,
    git_execution_requested: bool,
    network_requested: bool,
    secret_inheritance_requested: bool,
    filesystem_mutation_requested: bool,
    patch_application_requested: bool,
    ci_claim_requested: bool,
    deployment_requested: bool,
    production_write_requested: bool,
}

impl DxrWorkflowOrchestrationRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrWorkflowOrchestrationResult, String> {
        let request = parse_workflow_orchestration_request(body)?;
        self.next_run += 1;

        let authority_requested = request.worker_spawn_requested
            || request.provider_calls_requested
            || request.tool_execution_requested
            || request.shell_execution_requested
            || request.git_execution_requested
            || request.network_requested
            || request.secret_inheritance_requested
            || request.filesystem_mutation_requested
            || request.patch_application_requested
            || request.ci_claim_requested
            || request.deployment_requested
            || request.production_write_requested;
        let required_evidence_observed = request.required_evidence_observed();
        let shape_valid = request.shape_valid();
        let dispatch_window_count = ceil_div(
            request.accepted_workflow_count,
            request.next_dispatch_batch_size.max(1),
        );
        let workflow_batches_required = ceil_div(
            request.requested_workflow_count,
            request.workflow_script_slots.max(1),
        );
        let provider_stream_batches_required = ceil_div(
            request.requested_workflow_count,
            request.provider_stream_slots.max(1),
        );
        let sandbox_batches_required = ceil_div(
            request.requested_workflow_count,
            request.sandbox_pool_size.max(1),
        );
        let orchestrated_phase_count = 9
            + usize::from(request.cancel_requested)
            + usize::from(request.replay_requested)
            + usize::from(request.retained_backpressure_count > 0);

        let (status, terminal_state, orchestration_decision, rejected, rejection_reason) =
            if authority_requested {
                (
                    "DXR_WORKFLOW_ORCHESTRATION_REJECTED_SECURITY_BOUNDARY",
                    "DXR_WORKFLOW_ORCHESTRATION_REJECTED_SECURITY_BOUNDARY",
                    "rejected_security_boundary",
                    true,
                    "workflow_orchestration_cannot_start_workers_providers_tools_shell_git_network_secrets_filesystem_patch_ci_deploy_or_write_production",
                )
            } else if !required_evidence_observed || !shape_valid {
                (
                    "DXR_WORKFLOW_ORCHESTRATION_REJECTED_MISSING_EVIDENCE",
                    "DXR_WORKFLOW_ORCHESTRATION_REJECTED_MISSING_EVIDENCE",
                    "rejected_missing_orchestration_evidence",
                    true,
                    "workflow_dynamic_schedule_supervision_dispatch_ctx_provider_memory_provider_sandbox_result_evidence_relay_durable_reviewer_human_fairness_replay_or_checkpoint_evidence_missing",
                )
            } else if request.cancel_requested {
                (
                    "DXR_WORKFLOW_ORCHESTRATION_RECORDED_CANCELLED_REPLAY_READY",
                    "DXR_WORKFLOW_ORCHESTRATION_RECORDED_CANCELLED_REPLAY_READY",
                    "cancelled_with_checkpoint_and_replay_cursor",
                    false,
                    "none",
                )
            } else if request.replay_requested {
                (
                    "DXR_WORKFLOW_ORCHESTRATION_RECORDED_REPLAY_READY",
                    "DXR_WORKFLOW_ORCHESTRATION_RECORDED_REPLAY_READY",
                    "replay_ready_from_durable_cursor",
                    false,
                    "none",
                )
            } else if request.retained_backpressure_count > 0 {
                (
                    "DXR_WORKFLOW_ORCHESTRATION_RECORDED_BACKPRESSURE_SUPERVISED",
                    "DXR_WORKFLOW_ORCHESTRATION_RECORDED_BACKPRESSURE_SUPERVISED",
                    "ready_with_tenant_fair_backpressure",
                    false,
                    "none",
                )
            } else {
                (
                    "LIVE-LOCAL-DXR-WORKFLOW-ORCHESTRATION-FLOOR",
                    "DXR_WORKFLOW_ORCHESTRATION_RECORDED_READY_AUTHORITY_BLOCKED",
                    "ready_authority_blocked_before_apply",
                    false,
                    "none",
                )
            };

        let run = DxrWorkflowOrchestrationRun {
            sequence: self.next_run,
            orchestration_id: format!("dxr_workflow_orchestration_{:06}", self.next_run),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            idempotency_key: request.idempotency_key,
            workflow_run_id: request.workflow_run_id,
            dynamic_workflow_plan_id: request.dynamic_workflow_plan_id,
            execution_schedule_id: request.execution_schedule_id,
            execution_supervision_id: request.execution_supervision_id,
            sandbox_session_id: request.sandbox_session_id,
            sandbox_result_consumption_id: request.sandbox_result_consumption_id,
            dispatch_claim_id: request.dispatch_claim_id,
            ctx_context_input_id: request.ctx_context_input_id,
            provider_memory_integration_id: request.provider_memory_integration_id,
            provider_memory_terminal_state: request.provider_memory_terminal_state,
            workflow_observed: request.workflow_observed,
            dynamic_workflow_observed: request.dynamic_workflow_observed,
            execution_schedule_observed: request.execution_schedule_observed,
            execution_supervision_observed: request.execution_supervision_observed,
            dispatch_claim_observed: request.dispatch_claim_observed,
            dispatch_heartbeat_observed: request.dispatch_heartbeat_observed,
            dispatch_recovery_observed: request.dispatch_recovery_observed,
            ctx_context_observed: request.ctx_context_observed,
            provider_memory_integration_observed: request.provider_memory_integration_observed,
            provider_streaming_observed: request.provider_streaming_observed,
            provider_failover_observed: request.provider_failover_observed,
            multi_judge_observed: request.multi_judge_observed,
            sandbox_authority_observed: request.sandbox_authority_observed,
            sandbox_session_observed: request.sandbox_session_observed,
            sandbox_command_result_observed: request.sandbox_command_result_observed,
            sandbox_result_consumption_observed: request.sandbox_result_consumption_observed,
            evidence_chain_observed: request.evidence_chain_observed,
            relay_event_stream_observed: request.relay_event_stream_observed,
            durable_state_observed: request.durable_state_observed,
            reviewer_separation_observed: request.reviewer_separation_observed,
            human_ratification_observed: request.human_ratification_observed,
            tenant_fairness_observed: request.tenant_fairness_observed,
            backpressure_observed: request.backpressure_observed,
            replay_cursor_observed: request.replay_cursor_observed,
            cancellation_checkpoint_observed: request.cancellation_checkpoint_observed,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            active_tenant_count: request.active_tenant_count,
            requested_workflow_count: request.requested_workflow_count,
            accepted_workflow_count: request.accepted_workflow_count,
            retained_backpressure_count: request.retained_backpressure_count,
            workflow_script_slots: request.workflow_script_slots,
            provider_stream_slots: request.provider_stream_slots,
            sandbox_pool_size: request.sandbox_pool_size,
            next_dispatch_batch_size: request.next_dispatch_batch_size,
            orchestration_batch_size: request.orchestration_batch_size,
            dispatch_window_count,
            workflow_batches_required,
            provider_stream_batches_required,
            sandbox_batches_required,
            retry_budget: request.retry_budget,
            replay_window_seconds: request.replay_window_seconds,
            resume_checkpoint_interval_seconds: request.resume_checkpoint_interval_seconds,
            lease_duration_ms: request.lease_duration_ms,
            heartbeat_interval_ms: request.heartbeat_interval_ms,
            hot_path_budget_ms: request.hot_path_budget_ms,
            orchestrated_phase_count,
            cancel_requested: request.cancel_requested,
            replay_requested: request.replay_requested,
            terminal_state: terminal_state.to_string(),
            status: status.to_string(),
            orchestration_decision: orchestration_decision.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let events = workflow_orchestration_events(&run);
        let body = render_workflow_orchestration_response_json(&run);
        self.runs.push(run);
        Ok(DxrWorkflowOrchestrationResult { body, events })
    }

    pub fn orchestrations_json(&self) -> String {
        let ready_count = self
            .runs
            .iter()
            .filter(|run| {
                run.terminal_state == "DXR_WORKFLOW_ORCHESTRATION_RECORDED_READY_AUTHORITY_BLOCKED"
            })
            .count();
        let backpressure_count = self
            .runs
            .iter()
            .filter(|run| {
                run.terminal_state == "DXR_WORKFLOW_ORCHESTRATION_RECORDED_BACKPRESSURE_SUPERVISED"
            })
            .count();
        let cancellation_count = self
            .runs
            .iter()
            .filter(|run| {
                run.terminal_state == "DXR_WORKFLOW_ORCHESTRATION_RECORDED_CANCELLED_REPLAY_READY"
            })
            .count();
        let replay_ready_count = self
            .runs
            .iter()
            .filter(|run| run.terminal_state == "DXR_WORKFLOW_ORCHESTRATION_RECORDED_REPLAY_READY")
            .count();
        let rejected_count = self.runs.iter().filter(|run| run.rejected).count();
        let max_accepted_workflow_count = self
            .runs
            .iter()
            .map(|run| run.accepted_workflow_count)
            .max()
            .unwrap_or(0);
        let max_retained_backpressure_count = self
            .runs
            .iter()
            .map(|run| run.retained_backpressure_count)
            .max()
            .unwrap_or(0);
        format!(
            r#"{{"name":"mdx-dxr-workflow-orchestrations","status":"LIVE-LOCAL-DXR-WORKFLOW-ORCHESTRATION-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/workflow-orchestrations.json","submit_route":"/v1/dxr/workflow-orchestrations","orchestration_count":{},"ready_count":{},"backpressure_count":{},"cancellation_count":{},"replay_ready_count":{},"rejected_count":{},"max_accepted_workflow_count":{},"max_retained_backpressure_count":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"orchestration_policy":"durable_workflow_to_sandbox_result_consumption_before_ci_patch_apply_authority","required_runtime_gates":{},"workflow_orchestration_engine_ready":{},"authority_packet_complete":false,"ci_claim_allowed":false,"patch_application_allowed":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"deployment_allowed":false,"worker_spawn_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"production_writes_allowed":false,"orchestrations":[{}]}}"#,
            self.runs.len(),
            ready_count,
            backpressure_count,
            cancellation_count,
            replay_ready_count,
            rejected_count,
            max_accepted_workflow_count,
            max_retained_backpressure_count,
            DEFAULT_TARGET_ENGINEERS,
            DEFAULT_PEAK_PARALLEL_FORGE_BUILDS,
            render_required_runtime_gates_json(),
            self.runs.iter().any(|run| !run.rejected),
            self.runs
                .iter()
                .map(render_workflow_orchestration_run_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

impl WorkflowOrchestrationRequest {
    fn required_evidence_observed(&self) -> bool {
        self.workflow_observed
            && self.dynamic_workflow_observed
            && self.execution_schedule_observed
            && self.execution_supervision_observed
            && self.dispatch_claim_observed
            && self.dispatch_heartbeat_observed
            && self.dispatch_recovery_observed
            && self.ctx_context_observed
            && self.provider_memory_integration_observed
            && VALID_PROVIDER_MEMORY_TERMINAL_STATES
                .contains(&self.provider_memory_terminal_state.as_str())
            && self.provider_streaming_observed
            && self.provider_failover_observed
            && self.multi_judge_observed
            && self.sandbox_authority_observed
            && self.sandbox_session_observed
            && self.sandbox_command_result_observed
            && self.sandbox_result_consumption_observed
            && self.evidence_chain_observed
            && self.relay_event_stream_observed
            && self.durable_state_observed
            && self.reviewer_separation_observed
            && self.human_ratification_observed
            && self.tenant_fairness_observed
            && self.backpressure_observed
            && self.replay_cursor_observed
            && self.cancellation_checkpoint_observed
    }

    fn shape_valid(&self) -> bool {
        !self.idempotency_key.trim().is_empty()
            && !self.workflow_run_id.trim().is_empty()
            && !self.dynamic_workflow_plan_id.trim().is_empty()
            && !self.execution_schedule_id.trim().is_empty()
            && !self.execution_supervision_id.trim().is_empty()
            && !self.sandbox_session_id.trim().is_empty()
            && !self.sandbox_result_consumption_id.trim().is_empty()
            && !self.dispatch_claim_id.trim().is_empty()
            && !self.ctx_context_input_id.trim().is_empty()
            && !self.provider_memory_integration_id.trim().is_empty()
            && self.active_tenant_count > 0
            && self.requested_workflow_count > 0
            && self.accepted_workflow_count > 0
            && self.accepted_workflow_count <= self.peak_parallel_forge_builds
            && self.workflow_script_slots > 0
            && self.provider_stream_slots > 0
            && self.sandbox_pool_size > 0
            && self.next_dispatch_batch_size > 0
            && self.orchestration_batch_size > 0
            && self.retry_budget > 0
            && self.replay_window_seconds > 0
            && self.resume_checkpoint_interval_seconds > 0
            && self.lease_duration_ms > self.heartbeat_interval_ms
            && self.heartbeat_interval_ms > 0
            && self.hot_path_budget_ms <= DEFAULT_HOT_PATH_BUDGET_MS
    }
}

fn workflow_orchestration_events(
    run: &DxrWorkflowOrchestrationRun,
) -> Vec<DxrWorkflowOrchestrationRuntimeEvent> {
    let mut event_types = vec![
        "workflow_orchestration_recorded",
        "workflow_orchestration_dependencies_bound",
        "workflow_orchestration_ctx_bound",
        "workflow_orchestration_provider_memory_bound",
        "workflow_orchestration_dispatch_bound",
        "workflow_orchestration_dynamic_workflow_bound",
        "workflow_orchestration_schedule_bound",
        "workflow_orchestration_supervision_bound",
        "workflow_orchestration_sandbox_bound",
        "workflow_orchestration_result_consumption_bound",
        "workflow_orchestration_relay_durable_bound",
        "workflow_orchestration_reviewer_separation_bound",
        "workflow_orchestration_tenant_fairness_enforced",
        "workflow_orchestration_replay_cursor_recorded",
        "workflow_orchestration_authority_blocked",
    ];
    if run.retained_backpressure_count > 0 {
        event_types.push("workflow_orchestration_backpressure_retained");
    }
    if run.cancel_requested {
        event_types.push("workflow_orchestration_cancellation_checkpoint_recorded");
    }
    if run.replay_requested {
        event_types.push("workflow_orchestration_replay_ready");
    }
    if run.rejected {
        event_types.push("workflow_orchestration_rejected");
    } else {
        event_types.push("workflow_orchestration_ready_before_apply_authority");
    }
    event_types
        .into_iter()
        .map(|event_type| DxrWorkflowOrchestrationRuntimeEvent {
            event_type: event_type.to_string(),
            tenant_id: run.tenant_id.clone(),
            job_id: run.job_id.clone(),
            run_id: run.run_id.clone(),
            actor_id: run.actor_id.clone(),
        })
        .collect()
}

fn render_workflow_orchestration_response_json(run: &DxrWorkflowOrchestrationRun) -> String {
    format!(
        r#"{{"name":"mdx-dxr-workflow-orchestration","status":{},"runtime":"mdx-dxr-engine","orchestration":{},"workflow_orchestration_engine_ready":{},"authority_packet_complete":false,"ci_claim_allowed":false,"patch_application_allowed":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"deployment_allowed":false,"worker_spawn_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&run.status),
        render_workflow_orchestration_run_json(run),
        !run.rejected
    )
}

fn render_workflow_orchestration_run_json(run: &DxrWorkflowOrchestrationRun) -> String {
    format!(
        r#"{{"sequence":{},"orchestration_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"idempotency_key":{},"workflow_run_id":{},"dynamic_workflow_plan_id":{},"execution_schedule_id":{},"execution_supervision_id":{},"sandbox_session_id":{},"sandbox_result_consumption_id":{},"dispatch_claim_id":{},"ctx_context_input_id":{},"provider_memory_integration_id":{},"provider_memory_terminal_state":{},"workflow_observed":{},"dynamic_workflow_observed":{},"execution_schedule_observed":{},"execution_supervision_observed":{},"dispatch_claim_observed":{},"dispatch_heartbeat_observed":{},"dispatch_recovery_observed":{},"ctx_context_observed":{},"provider_memory_integration_observed":{},"provider_streaming_observed":{},"provider_failover_observed":{},"multi_judge_observed":{},"sandbox_authority_observed":{},"sandbox_session_observed":{},"sandbox_command_result_observed":{},"sandbox_result_consumption_observed":{},"evidence_chain_observed":{},"relay_event_stream_observed":{},"durable_state_observed":{},"reviewer_separation_observed":{},"human_ratification_observed":{},"tenant_fairness_observed":{},"backpressure_observed":{},"replay_cursor_observed":{},"cancellation_checkpoint_observed":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"active_tenant_count":{},"requested_workflow_count":{},"accepted_workflow_count":{},"retained_backpressure_count":{},"workflow_script_slots":{},"provider_stream_slots":{},"sandbox_pool_size":{},"next_dispatch_batch_size":{},"orchestration_batch_size":{},"dispatch_window_count":{},"workflow_batches_required":{},"provider_stream_batches_required":{},"sandbox_batches_required":{},"retry_budget":{},"replay_window_seconds":{},"resume_checkpoint_interval_seconds":{},"lease_duration_ms":{},"heartbeat_interval_ms":{},"hot_path_budget_ms":{},"orchestrated_phase_count":{},"cancel_requested":{},"replay_requested":{},"terminal_state":{},"status":{},"orchestration_decision":{},"rejected":{},"rejection_reason":{},"ci_claim_allowed":false,"patch_application_allowed":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"deployment_allowed":false,"worker_spawn_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"production_writes_allowed":false}}"#,
        run.sequence,
        json_string_literal(&run.orchestration_id),
        json_string_literal(&run.tenant_id),
        json_string_literal(&run.actor_id),
        json_string_literal(&run.job_id),
        json_string_literal(&run.run_id),
        json_string_literal(&run.idempotency_key),
        json_string_literal(&run.workflow_run_id),
        json_string_literal(&run.dynamic_workflow_plan_id),
        json_string_literal(&run.execution_schedule_id),
        json_string_literal(&run.execution_supervision_id),
        json_string_literal(&run.sandbox_session_id),
        json_string_literal(&run.sandbox_result_consumption_id),
        json_string_literal(&run.dispatch_claim_id),
        json_string_literal(&run.ctx_context_input_id),
        json_string_literal(&run.provider_memory_integration_id),
        json_string_literal(&run.provider_memory_terminal_state),
        run.workflow_observed,
        run.dynamic_workflow_observed,
        run.execution_schedule_observed,
        run.execution_supervision_observed,
        run.dispatch_claim_observed,
        run.dispatch_heartbeat_observed,
        run.dispatch_recovery_observed,
        run.ctx_context_observed,
        run.provider_memory_integration_observed,
        run.provider_streaming_observed,
        run.provider_failover_observed,
        run.multi_judge_observed,
        run.sandbox_authority_observed,
        run.sandbox_session_observed,
        run.sandbox_command_result_observed,
        run.sandbox_result_consumption_observed,
        run.evidence_chain_observed,
        run.relay_event_stream_observed,
        run.durable_state_observed,
        run.reviewer_separation_observed,
        run.human_ratification_observed,
        run.tenant_fairness_observed,
        run.backpressure_observed,
        run.replay_cursor_observed,
        run.cancellation_checkpoint_observed,
        run.target_concurrent_engineers,
        run.peak_parallel_forge_builds,
        run.active_tenant_count,
        run.requested_workflow_count,
        run.accepted_workflow_count,
        run.retained_backpressure_count,
        run.workflow_script_slots,
        run.provider_stream_slots,
        run.sandbox_pool_size,
        run.next_dispatch_batch_size,
        run.orchestration_batch_size,
        run.dispatch_window_count,
        run.workflow_batches_required,
        run.provider_stream_batches_required,
        run.sandbox_batches_required,
        run.retry_budget,
        run.replay_window_seconds,
        run.resume_checkpoint_interval_seconds,
        run.lease_duration_ms,
        run.heartbeat_interval_ms,
        run.hot_path_budget_ms,
        run.orchestrated_phase_count,
        run.cancel_requested,
        run.replay_requested,
        json_string_literal(&run.terminal_state),
        json_string_literal(&run.status),
        json_string_literal(&run.orchestration_decision),
        run.rejected,
        json_string_literal(&run.rejection_reason)
    )
}

fn render_required_runtime_gates_json() -> String {
    render_string_array(&[
        "durable_workflow",
        "dynamic_workflow",
        "execution_schedule",
        "execution_supervision",
        "dispatch_claim",
        "dispatch_heartbeat",
        "dispatch_recovery",
        "ctx_context",
        "provider_memory_integration",
        "provider_streaming",
        "provider_failover",
        "multi_judge",
        "sandbox_authority",
        "sandbox_session",
        "sandbox_command_result",
        "sandbox_result_consumption",
        "evidence_chain",
        "relay_event_stream",
        "durable_state",
        "reviewer_separation",
        "human_ratification",
        "tenant_fairness",
        "backpressure",
        "replay_cursor",
        "cancellation_checkpoint",
    ])
}

fn parse_workflow_orchestration_request(
    body: &str,
) -> Result<WorkflowOrchestrationRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR workflow orchestration json: {error}"))?;
    Ok(WorkflowOrchestrationRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", DEFAULT_JOB_ID),
        run_id: string_value(&value, "run_id", DEFAULT_RUN_ID),
        idempotency_key: string_value(&value, "idempotency_key", "dxr-workflow-orchestration-001"),
        workflow_run_id: string_value(&value, "workflow_run_id", DEFAULT_WORKFLOW_RUN_ID),
        dynamic_workflow_plan_id: string_value(
            &value,
            "dynamic_workflow_plan_id",
            DEFAULT_DYNAMIC_WORKFLOW_PLAN_ID,
        ),
        execution_schedule_id: string_value(
            &value,
            "execution_schedule_id",
            DEFAULT_EXECUTION_SCHEDULE_ID,
        ),
        execution_supervision_id: string_value(
            &value,
            "execution_supervision_id",
            DEFAULT_EXECUTION_SUPERVISION_ID,
        ),
        sandbox_session_id: string_value(&value, "sandbox_session_id", DEFAULT_SANDBOX_SESSION_ID),
        sandbox_result_consumption_id: string_value(
            &value,
            "sandbox_result_consumption_id",
            DEFAULT_SANDBOX_RESULT_CONSUMPTION_ID,
        ),
        dispatch_claim_id: string_value(&value, "dispatch_claim_id", DEFAULT_CLAIM_ID),
        ctx_context_input_id: string_value(
            &value,
            "ctx_context_input_id",
            DEFAULT_CTX_CONTEXT_INPUT_ID,
        ),
        provider_memory_integration_id: string_value(
            &value,
            "provider_memory_integration_id",
            DEFAULT_PROVIDER_MEMORY_INTEGRATION_ID,
        ),
        provider_memory_terminal_state: string_value(
            &value,
            "provider_memory_terminal_state",
            "DXR_PROVIDER_MEMORY_INTEGRATION_RECORDED_LOCAL_DRIVER_READY_PROVIDER_BLOCKED",
        ),
        workflow_observed: bool_value(&value, "workflow_observed", true),
        dynamic_workflow_observed: bool_value(&value, "dynamic_workflow_observed", true),
        execution_schedule_observed: bool_value(&value, "execution_schedule_observed", true),
        execution_supervision_observed: bool_value(&value, "execution_supervision_observed", true),
        dispatch_claim_observed: bool_value(&value, "dispatch_claim_observed", true),
        dispatch_heartbeat_observed: bool_value(&value, "dispatch_heartbeat_observed", true),
        dispatch_recovery_observed: bool_value(&value, "dispatch_recovery_observed", true),
        ctx_context_observed: bool_value(&value, "ctx_context_observed", true),
        provider_memory_integration_observed: bool_value(
            &value,
            "provider_memory_integration_observed",
            true,
        ),
        provider_streaming_observed: bool_value(&value, "provider_streaming_observed", true),
        provider_failover_observed: bool_value(&value, "provider_failover_observed", true),
        multi_judge_observed: bool_value(&value, "multi_judge_observed", true),
        sandbox_authority_observed: bool_value(&value, "sandbox_authority_observed", true),
        sandbox_session_observed: bool_value(&value, "sandbox_session_observed", true),
        sandbox_command_result_observed: bool_value(
            &value,
            "sandbox_command_result_observed",
            true,
        ),
        sandbox_result_consumption_observed: bool_value(
            &value,
            "sandbox_result_consumption_observed",
            true,
        ),
        evidence_chain_observed: bool_value(&value, "evidence_chain_observed", true),
        relay_event_stream_observed: bool_value(&value, "relay_event_stream_observed", true),
        durable_state_observed: bool_value(&value, "durable_state_observed", true),
        reviewer_separation_observed: bool_value(&value, "reviewer_separation_observed", true),
        human_ratification_observed: bool_value(&value, "human_ratification_observed", true),
        tenant_fairness_observed: bool_value(&value, "tenant_fairness_observed", true),
        backpressure_observed: bool_value(&value, "backpressure_observed", true),
        replay_cursor_observed: bool_value(&value, "replay_cursor_observed", true),
        cancellation_checkpoint_observed: bool_value(
            &value,
            "cancellation_checkpoint_observed",
            true,
        ),
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
        requested_workflow_count: usize_value(
            &value,
            "requested_workflow_count",
            DEFAULT_PEAK_PARALLEL_FORGE_BUILDS,
        ),
        accepted_workflow_count: usize_value(
            &value,
            "accepted_workflow_count",
            DEFAULT_SANDBOX_POOL_SIZE,
        ),
        retained_backpressure_count: usize_value(
            &value,
            "retained_backpressure_count",
            DEFAULT_RETAINED_BACKPRESSURE_COUNT,
        ),
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
        sandbox_pool_size: usize_value(&value, "sandbox_pool_size", DEFAULT_SANDBOX_POOL_SIZE),
        next_dispatch_batch_size: usize_value(
            &value,
            "next_dispatch_batch_size",
            DEFAULT_NEXT_DISPATCH_BATCH_SIZE,
        ),
        orchestration_batch_size: usize_value(
            &value,
            "orchestration_batch_size",
            DEFAULT_ORCHESTRATION_BATCH_SIZE,
        ),
        retry_budget: usize_value(&value, "retry_budget", DEFAULT_RETRY_BUDGET),
        replay_window_seconds: usize_value(
            &value,
            "replay_window_seconds",
            DEFAULT_REPLAY_WINDOW_SECONDS,
        ),
        resume_checkpoint_interval_seconds: usize_value(
            &value,
            "resume_checkpoint_interval_seconds",
            DEFAULT_RESUME_CHECKPOINT_INTERVAL_SECONDS,
        ),
        lease_duration_ms: usize_value(&value, "lease_duration_ms", DEFAULT_LEASE_DURATION_MS),
        heartbeat_interval_ms: usize_value(
            &value,
            "heartbeat_interval_ms",
            DEFAULT_HEARTBEAT_INTERVAL_MS,
        ),
        hot_path_budget_ms: usize_value(&value, "hot_path_budget_ms", DEFAULT_HOT_PATH_BUDGET_MS),
        cancel_requested: bool_value(&value, "cancel_requested", false),
        replay_requested: bool_value(&value, "replay_requested", false),
        worker_spawn_requested: bool_value(&value, "worker_spawn_requested", false),
        provider_calls_requested: bool_value(&value, "provider_calls_requested", false),
        tool_execution_requested: bool_value(&value, "tool_execution_requested", false),
        shell_execution_requested: bool_value(&value, "shell_execution_requested", false),
        git_execution_requested: bool_value(&value, "git_execution_requested", false),
        network_requested: bool_value(&value, "network_requested", false),
        secret_inheritance_requested: bool_value(&value, "secret_inheritance_requested", false),
        filesystem_mutation_requested: bool_value(&value, "filesystem_mutation_requested", false),
        patch_application_requested: bool_value(&value, "patch_application_requested", false),
        ci_claim_requested: bool_value(&value, "ci_claim_requested", false),
        deployment_requested: bool_value(&value, "deployment_requested", false),
        production_write_requested: bool_value(&value, "production_write_requested", false),
    })
}

fn string_value(value: &Value, key: &str, fallback: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

fn bool_value(value: &Value, key: &str, fallback: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}

fn usize_value(value: &Value, key: &str, fallback: usize) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(fallback)
}

fn render_string_array(values: &[&str]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| json_string_literal(value))
            .collect::<Vec<_>>()
            .join(",")
    )
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
    fn workflow_orchestration_records_ready_backpressure_cancel_replay_and_rejects_authority() {
        let mut runtime = DxrWorkflowOrchestrationRuntime::new();

        let ready = runtime.submit_json(
            r#"{"idempotency_key":"ready","requested_workflow_count":2048,"accepted_workflow_count":2048,"retained_backpressure_count":0}"#,
        ).expect("ready orchestration");
        assert!(
            ready
                .body
                .contains("DXR_WORKFLOW_ORCHESTRATION_RECORDED_READY_AUTHORITY_BLOCKED")
        );
        assert!(
            ready
                .events
                .iter()
                .any(|event| event.event_type == "workflow_orchestration_provider_memory_bound")
        );
        assert!(
            ready
                .events
                .iter()
                .any(|event| event.event_type
                    == "workflow_orchestration_ready_before_apply_authority")
        );

        let backpressure = runtime.submit_json(
            r#"{"idempotency_key":"backpressure","requested_workflow_count":5000,"accepted_workflow_count":2048,"retained_backpressure_count":2952}"#,
        ).expect("backpressure orchestration");
        assert!(
            backpressure
                .body
                .contains("DXR_WORKFLOW_ORCHESTRATION_RECORDED_BACKPRESSURE_SUPERVISED")
        );

        let cancel = runtime
            .submit_json(r#"{"idempotency_key":"cancel","cancel_requested":true}"#)
            .expect("cancel orchestration");
        assert!(
            cancel
                .body
                .contains("DXR_WORKFLOW_ORCHESTRATION_RECORDED_CANCELLED_REPLAY_READY")
        );

        let replay = runtime
            .submit_json(r#"{"idempotency_key":"replay","replay_requested":true}"#)
            .expect("replay orchestration");
        assert!(
            replay
                .body
                .contains("DXR_WORKFLOW_ORCHESTRATION_RECORDED_REPLAY_READY")
        );

        let rejected = runtime.submit_json(
            r#"{"idempotency_key":"unsafe","provider_calls_requested":true,"patch_application_requested":true,"production_write_requested":true}"#,
        ).expect("rejected orchestration");
        assert!(
            rejected
                .body
                .contains("DXR_WORKFLOW_ORCHESTRATION_REJECTED_SECURITY_BOUNDARY")
        );

        let missing_provider_memory = runtime.submit_json(
            r#"{"idempotency_key":"missing-provider-memory","provider_memory_integration_observed":false,"provider_memory_terminal_state":"DXR_PROVIDER_MEMORY_INTEGRATION_REJECTED_SECURITY_BOUNDARY"}"#,
        ).expect("missing provider memory orchestration");
        assert!(
            missing_provider_memory
                .body
                .contains("DXR_WORKFLOW_ORCHESTRATION_REJECTED_MISSING_EVIDENCE")
        );

        let projection = runtime.orchestrations_json();
        assert!(projection.contains(r#""orchestration_count":6"#));
        assert!(projection.contains(r#""ready_count":1"#));
        assert!(projection.contains(r#""backpressure_count":1"#));
        assert!(projection.contains(r#""cancellation_count":1"#));
        assert!(projection.contains(r#""replay_ready_count":1"#));
        assert!(projection.contains(r#""rejected_count":2"#));
        assert!(projection.contains("provider_memory_integration"));
        assert!(projection.contains(r#""workflow_orchestration_engine_ready":true"#));
        assert!(projection.contains(r#""production_writes_allowed":false"#));
    }
}
