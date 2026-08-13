use mdx_core::json_string_literal;
use serde_json::Value;

use crate::workflow::{DxrWorkflowEvent, DxrWorkflowRun};

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_MAX_CONCURRENT_AGENTS: usize = 16;
const DEFAULT_MAX_TOTAL_AGENTS: usize = 1000;
const DEFAULT_PHASE_COUNT: usize = 6;
const DEFAULT_TOKEN_BUDGET: usize = 1_000_000;

#[derive(Default)]
pub struct DxrDynamicWorkflowRuntime {
    plans: Vec<DxrDynamicWorkflowPlan>,
    controls: Vec<DxrDynamicWorkflowControl>,
    next_plan: usize,
    next_control: usize,
}

pub struct DxrDynamicWorkflowResult {
    pub body: String,
    pub events: Vec<DxrDynamicWorkflowRuntimeEvent>,
    pub durable_run: DxrWorkflowRun,
}

pub struct DxrDynamicWorkflowRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrDynamicWorkflowPlan {
    sequence: usize,
    workflow_plan_id: String,
    tenant_id: String,
    actor_id: String,
    workflow_name: String,
    source_job_id: String,
    source_run_id: String,
    pattern: String,
    phase_count: usize,
    requested_total_agents: usize,
    planned_agent_count: usize,
    max_concurrent_agents: usize,
    max_total_agents: usize,
    agent_batches_required: usize,
    token_budget: usize,
    script_persistence: String,
    intermediate_state_location: String,
    approval_mode: String,
    reviewer_context: String,
    artifact_retention_days: usize,
    resumable_within_session: bool,
    builder_can_self_accept: bool,
    untrusted_input_quarantined: bool,
    workflow_shell_access_allowed: bool,
    workflow_filesystem_access_allowed: bool,
    agent_tool_allowlist_inherited: bool,
    terminal_state: String,
    status: String,
    accepted: bool,
    phase_schedule: Vec<DxrDynamicWorkflowPhase>,
}

struct DynamicWorkflowRequest {
    tenant_id: String,
    actor_id: String,
    workflow_name: String,
    source_job_id: String,
    source_run_id: String,
    pattern: String,
    phase_count: usize,
    requested_total_agents: usize,
    planned_agent_count: usize,
    max_concurrent_agents: usize,
    max_total_agents: usize,
    token_budget: usize,
    script_persistence: String,
    intermediate_state_location: String,
    approval_mode: String,
    reviewer_context: String,
    artifact_retention_days: usize,
    resumable_within_session: bool,
    builder_can_self_accept: bool,
    untrusted_input_quarantined: bool,
    workflow_shell_access_allowed: bool,
    workflow_filesystem_access_allowed: bool,
    agent_tool_allowlist_inherited: bool,
}

#[derive(Clone)]
struct DxrDynamicWorkflowControl {
    sequence: usize,
    control_id: String,
    tenant_id: String,
    actor_id: String,
    workflow_plan_id: String,
    phase_id: String,
    control_action: String,
    status: String,
    terminal_state: String,
    progress_state_before: String,
    progress_state_after: String,
    resume_cache_key: String,
    idempotency_key: String,
    cached_result_reused: bool,
    phase_restart_allowed: bool,
    accepted: bool,
}

struct DynamicWorkflowControlRequest {
    tenant_id: String,
    actor_id: String,
    workflow_plan_id: String,
    phase_id: String,
    control_action: String,
    resume_cache_key: String,
    idempotency_key: String,
    cached_result_reused: bool,
    phase_restart_allowed: bool,
    user_input_required_mid_run: bool,
    workflow_shell_access_allowed: bool,
    workflow_filesystem_access_allowed: bool,
    agent_spawn_requested: bool,
    provider_calls_allowed: bool,
    tool_execution_allowed: bool,
    worker_execution_allowed: bool,
    production_writes_allowed: bool,
}

#[derive(Clone)]
struct DxrDynamicWorkflowPhase {
    phase_index: usize,
    phase_id: String,
    pattern: String,
    phase_role: String,
    requested_agents: usize,
    max_concurrent_agents: usize,
    batches_required: usize,
    checkpoint_policy: String,
    reviewer_context: String,
    authority_boundary: String,
    progress_state: String,
    token_budget: usize,
    elapsed_budget_ms: usize,
    resume_cache_key: String,
    phase_result_cached_for_replay: bool,
}

impl DxrDynamicWorkflowRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrDynamicWorkflowResult, String> {
        let request = parse_dynamic_workflow_request(body)?;
        self.next_plan += 1;
        let agent_batches_required = ceil_div(
            request.planned_agent_count,
            request.max_concurrent_agents.max(1),
        );
        let within_agent_caps = request.planned_agent_count <= request.max_total_agents
            && request.requested_total_agents <= request.max_total_agents
            && request.max_concurrent_agents <= DEFAULT_MAX_CONCURRENT_AGENTS
            && request.max_total_agents <= DEFAULT_MAX_TOTAL_AGENTS;
        let accepted = within_agent_caps
            && !request.builder_can_self_accept
            && request.untrusted_input_quarantined
            && !request.workflow_shell_access_allowed
            && !request.workflow_filesystem_access_allowed;
        let phase_schedule = build_phase_schedule(
            self.next_plan,
            request.phase_count,
            request.planned_agent_count,
            request.max_concurrent_agents,
            request.token_budget,
            &request.pattern,
            &request.reviewer_context,
        );
        let plan = DxrDynamicWorkflowPlan {
            sequence: self.next_plan,
            workflow_plan_id: format!("dxr_dynamic_workflow_plan_{:06}", self.next_plan),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            workflow_name: request.workflow_name,
            source_job_id: request.source_job_id,
            source_run_id: request.source_run_id,
            pattern: request.pattern,
            phase_count: request.phase_count,
            requested_total_agents: request.requested_total_agents,
            planned_agent_count: request.planned_agent_count,
            max_concurrent_agents: request.max_concurrent_agents,
            max_total_agents: request.max_total_agents,
            agent_batches_required,
            token_budget: request.token_budget,
            script_persistence: request.script_persistence,
            intermediate_state_location: request.intermediate_state_location,
            approval_mode: request.approval_mode,
            reviewer_context: request.reviewer_context,
            artifact_retention_days: request.artifact_retention_days,
            resumable_within_session: request.resumable_within_session,
            builder_can_self_accept: request.builder_can_self_accept,
            untrusted_input_quarantined: request.untrusted_input_quarantined,
            workflow_shell_access_allowed: request.workflow_shell_access_allowed,
            workflow_filesystem_access_allowed: request.workflow_filesystem_access_allowed,
            agent_tool_allowlist_inherited: request.agent_tool_allowlist_inherited,
            terminal_state: if accepted {
                "DXR_DYNAMIC_WORKFLOW_PLAN_RECORDED_BOUNDED".to_string()
            } else {
                "DXR_DYNAMIC_WORKFLOW_PLAN_RECORDED_AUTHORITY_BLOCKED".to_string()
            },
            status: if accepted {
                "DXR_DYNAMIC_WORKFLOW_PLAN_ACCEPTED_LOCAL".to_string()
            } else {
                "DXR_DYNAMIC_WORKFLOW_PLAN_AUTHORITY_BLOCKED_LOCAL".to_string()
            },
            accepted,
            phase_schedule,
        };
        let events = dynamic_workflow_events(&plan);
        let body = render_dynamic_workflow_response_json(&plan);
        let durable_run = durable_workflow_run_for_plan(&plan);
        self.plans.push(plan);
        Ok(DxrDynamicWorkflowResult {
            body,
            events,
            durable_run,
        })
    }

    pub fn control_json(&mut self, body: &str) -> Result<DxrDynamicWorkflowResult, String> {
        let request = parse_dynamic_workflow_control_request(body)?;
        self.next_control += 1;
        let matching_plan = self
            .plans
            .iter()
            .find(|plan| plan.workflow_plan_id == request.workflow_plan_id);
        let matching_phase = matching_plan.and_then(|plan| {
            plan.phase_schedule
                .iter()
                .find(|phase| phase.phase_id == request.phase_id)
        });
        let supported_action = matches!(
            request.control_action.as_str(),
            "pause" | "resume" | "stop" | "restart_phase"
        );
        let unsafe_authority_requested = request.user_input_required_mid_run
            || request.workflow_shell_access_allowed
            || request.workflow_filesystem_access_allowed
            || request.agent_spawn_requested
            || request.provider_calls_allowed
            || request.tool_execution_allowed
            || request.worker_execution_allowed
            || request.production_writes_allowed;
        let accepted = matching_plan.is_some()
            && matching_phase.is_some()
            && supported_action
            && !unsafe_authority_requested
            && !request.resume_cache_key.is_empty()
            && !request.idempotency_key.is_empty();
        let progress_state_after = if accepted {
            match request.control_action.as_str() {
                "pause" => "paused_checkpoint_recorded",
                "resume" => "resumed_from_phase_cache",
                "stop" => "stopped_operator_requested",
                "restart_phase" => "phase_restart_scheduled_from_cache",
                _ => "control_rejected",
            }
        } else {
            "control_rejected"
        };
        let terminal_state = if accepted {
            match request.control_action.as_str() {
                "pause" => "DXR_DYNAMIC_WORKFLOW_CONTROL_PAUSED_CHECKPOINT_RECORDED",
                "resume" => "DXR_DYNAMIC_WORKFLOW_CONTROL_RESUMED_FROM_CACHE",
                "stop" => "DXR_DYNAMIC_WORKFLOW_CONTROL_STOPPED_WITH_CHECKPOINT",
                "restart_phase" => "DXR_DYNAMIC_WORKFLOW_CONTROL_PHASE_RESTART_RECORDED",
                _ => "DXR_DYNAMIC_WORKFLOW_CONTROL_REJECTED",
            }
        } else {
            "DXR_DYNAMIC_WORKFLOW_CONTROL_REJECTED"
        };
        let control = DxrDynamicWorkflowControl {
            sequence: self.next_control,
            control_id: format!("dxr_dynamic_workflow_control_{:06}", self.next_control),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            workflow_plan_id: request.workflow_plan_id,
            phase_id: request.phase_id,
            control_action: request.control_action,
            status: if accepted {
                "LIVE-LOCAL-DXR-DYNAMIC-WORKFLOW-RUN-CONTROL".to_string()
            } else {
                "DXR_DYNAMIC_WORKFLOW_CONTROL_REJECTED_LOCAL".to_string()
            },
            terminal_state: terminal_state.to_string(),
            progress_state_before: matching_phase
                .map(|phase| phase.progress_state.clone())
                .unwrap_or_else(|| "unknown".to_string()),
            progress_state_after: progress_state_after.to_string(),
            resume_cache_key: request.resume_cache_key,
            idempotency_key: request.idempotency_key,
            cached_result_reused: accepted && request.cached_result_reused,
            phase_restart_allowed: accepted && request.phase_restart_allowed,
            accepted,
        };
        let events = dynamic_workflow_control_events(&control);
        let body = render_dynamic_workflow_control_response_json(&control);
        let durable_run = durable_workflow_run_for_control(&control, &events);
        self.controls.push(control);
        Ok(DxrDynamicWorkflowResult {
            body,
            events,
            durable_run,
        })
    }

    pub fn plans_json(&self) -> String {
        let accepted_count = self.plans.iter().filter(|plan| plan.accepted).count();
        let blocked_count = self.plans.len().saturating_sub(accepted_count);
        format!(
            r#"{{"name":"mdx-dxr-dynamic-workflows","status":"LIVE-LOCAL-DXR-DYNAMIC-WORKFLOW-PATTERN-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/dynamic-workflows.json","submit_route":"/v1/dxr/dynamic-workflow-plans","control_route":"/v1/dxr/dynamic-workflow-controls","controls_route":"/dxr/dynamic-workflow-controls.json","plan_count":{},"accepted_count":{},"blocked_count":{},"control_count":{},"accepted_control_count":{},"pattern_count":{},"patterns":[{}],"max_concurrent_agents_per_workflow":{},"max_total_agents_per_workflow":{},"default_phase_count":{},"script_orchestration":"repeatable_runtime_script","intermediate_state_location":"script_variables","phase_schedule_policy":"script_held_phase_schedule_with_checkpointed_replay","run_control_status":"LIVE-LOCAL-DXR-DYNAMIC-WORKFLOW-RUN-CONTROL","pause_resume_policy":"checkpoint_and_resume_cache_required","restart_policy":"phase_restart_from_cached_result_only","stop_policy":"operator_stop_records_checkpoint_before_terminal_state","progress_projection_status":"DXR_DYNAMIC_WORKFLOW_PROGRESS_PLANNED_LOCAL","workflow_progress_view_required":true,"phase_token_totals_required":true,"phase_elapsed_totals_required":true,"live_agent_progress_allowed":false,"fresh_reviewer_context_required":true,"builder_can_self_accept":false,"untrusted_input_quarantine_required":true,"workflow_shell_access_allowed":false,"workflow_filesystem_access_allowed":false,"agent_spawn_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"production_writes_allowed":false,"plans":[{}]}}"#,
            self.plans.len(),
            accepted_count,
            blocked_count,
            self.controls.len(),
            self.controls
                .iter()
                .filter(|control| control.accepted)
                .count(),
            workflow_patterns().len(),
            workflow_patterns()
                .iter()
                .map(render_workflow_pattern_json)
                .collect::<Vec<_>>()
                .join(","),
            DEFAULT_MAX_CONCURRENT_AGENTS,
            DEFAULT_MAX_TOTAL_AGENTS,
            DEFAULT_PHASE_COUNT,
            self.plans
                .iter()
                .map(render_dynamic_workflow_plan_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    pub fn controls_json(&self) -> String {
        let accepted_count = self
            .controls
            .iter()
            .filter(|control| control.accepted)
            .count();
        let rejected_count = self.controls.len().saturating_sub(accepted_count);
        format!(
            r#"{{"name":"mdx-dxr-dynamic-workflow-controls","status":"LIVE-LOCAL-DXR-DYNAMIC-WORKFLOW-RUN-CONTROL","runtime":"mdx-dxr-engine","route":"/dxr/dynamic-workflow-controls.json","submit_route":"/v1/dxr/dynamic-workflow-controls","control_count":{},"accepted_count":{},"rejected_count":{},"allowed_actions":["pause","resume","stop","restart_phase"],"pause_resume_policy":"checkpoint_and_resume_cache_required","restart_policy":"phase_restart_from_cached_result_only","stop_policy":"operator_stop_records_checkpoint_before_terminal_state","mid_run_user_input_allowed":false,"workflow_shell_access_allowed":false,"workflow_filesystem_access_allowed":false,"agent_spawn_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"production_writes_allowed":false,"controls":[{}]}}"#,
            self.controls.len(),
            accepted_count,
            rejected_count,
            self.controls
                .iter()
                .map(render_dynamic_workflow_control_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn parse_dynamic_workflow_request(body: &str) -> Result<DynamicWorkflowRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR dynamic workflow json: {error}"))?;
    let pattern = string_value(&value, "pattern", "fan_out_and_synthesize");
    if !workflow_patterns().iter().any(|item| item.name == pattern) {
        return Err("DXR dynamic workflow plan denied: unknown workflow pattern".to_string());
    }
    let max_concurrent_agents = usize_value(
        &value,
        "max_concurrent_agents",
        DEFAULT_MAX_CONCURRENT_AGENTS,
    );
    if max_concurrent_agents == 0 {
        return Err(
            "DXR dynamic workflow plan denied: max_concurrent_agents is required".to_string(),
        );
    }
    Ok(DynamicWorkflowRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        workflow_name: string_value(&value, "workflow_name", "forge_dynamic_build_review"),
        source_job_id: string_value(&value, "source_job_id", "dxr_job_dispatch_ready_001"),
        source_run_id: string_value(&value, "source_run_id", "dxr_run_dispatch_ready_001"),
        pattern,
        phase_count: usize_value(&value, "phase_count", DEFAULT_PHASE_COUNT),
        requested_total_agents: usize_value(&value, "requested_total_agents", 64),
        planned_agent_count: usize_value(&value, "planned_agent_count", 64),
        max_concurrent_agents,
        max_total_agents: usize_value(&value, "max_total_agents", DEFAULT_MAX_TOTAL_AGENTS),
        token_budget: usize_value(&value, "token_budget", DEFAULT_TOKEN_BUDGET),
        script_persistence: string_value(&value, "script_persistence", "reviewable_runtime_script"),
        intermediate_state_location: string_value(
            &value,
            "intermediate_state_location",
            "script_variables",
        ),
        approval_mode: string_value(
            &value,
            "approval_mode",
            "human_approval_required_before_run",
        ),
        reviewer_context: string_value(&value, "reviewer_context", "fresh_context_required"),
        artifact_retention_days: usize_value(&value, "artifact_retention_days", 30),
        resumable_within_session: bool_value(&value, "resumable_within_session", true),
        builder_can_self_accept: bool_value(&value, "builder_can_self_accept", false),
        untrusted_input_quarantined: bool_value(&value, "untrusted_input_quarantined", true),
        workflow_shell_access_allowed: bool_value(&value, "workflow_shell_access_allowed", false),
        workflow_filesystem_access_allowed: bool_value(
            &value,
            "workflow_filesystem_access_allowed",
            false,
        ),
        agent_tool_allowlist_inherited: bool_value(&value, "agent_tool_allowlist_inherited", true),
    })
}

fn parse_dynamic_workflow_control_request(
    body: &str,
) -> Result<DynamicWorkflowControlRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR dynamic workflow control json: {error}"))?;
    Ok(DynamicWorkflowControlRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        workflow_plan_id: string_value(
            &value,
            "workflow_plan_id",
            "dxr_dynamic_workflow_plan_000001",
        ),
        phase_id: string_value(
            &value,
            "phase_id",
            "dxr_dynamic_workflow_plan_000001_phase_001",
        ),
        control_action: string_value(&value, "control_action", "pause"),
        resume_cache_key: string_value(
            &value,
            "resume_cache_key",
            "dxr_dynamic_workflow_plan_000001_phase_001_resume_cache",
        ),
        idempotency_key: string_value(&value, "idempotency_key", "dynamic-control-0001"),
        cached_result_reused: bool_value(&value, "cached_result_reused", true),
        phase_restart_allowed: bool_value(&value, "phase_restart_allowed", true),
        user_input_required_mid_run: bool_value(&value, "user_input_required_mid_run", false),
        workflow_shell_access_allowed: bool_value(&value, "workflow_shell_access_allowed", false),
        workflow_filesystem_access_allowed: bool_value(
            &value,
            "workflow_filesystem_access_allowed",
            false,
        ),
        agent_spawn_requested: bool_value(&value, "agent_spawn_requested", false),
        provider_calls_allowed: bool_value(&value, "provider_calls_allowed", false),
        tool_execution_allowed: bool_value(&value, "tool_execution_allowed", false),
        worker_execution_allowed: bool_value(&value, "worker_execution_allowed", false),
        production_writes_allowed: bool_value(&value, "production_writes_allowed", false),
    })
}

fn dynamic_workflow_events(plan: &DxrDynamicWorkflowPlan) -> Vec<DxrDynamicWorkflowRuntimeEvent> {
    let mut events = vec![
        dynamic_workflow_event(plan, "dynamic_workflow_plan_recorded"),
        dynamic_workflow_event(plan, "dynamic_workflow_script_orchestration_bound"),
        dynamic_workflow_event(plan, "dynamic_workflow_agent_caps_enforced"),
        dynamic_workflow_event(plan, "dynamic_workflow_phase_schedule_recorded"),
        dynamic_workflow_event(plan, "dynamic_workflow_progress_projection_recorded"),
        dynamic_workflow_event(plan, "dynamic_workflow_replay_checkpoints_recorded"),
        dynamic_workflow_event(plan, "dynamic_workflow_reviewer_separation_required"),
        dynamic_workflow_event(plan, "dynamic_workflow_untrusted_input_quarantined"),
    ];
    if plan.accepted {
        events.push(dynamic_workflow_event(
            plan,
            "dynamic_workflow_plan_accepted_local",
        ));
    } else {
        events.push(dynamic_workflow_event(
            plan,
            "dynamic_workflow_authority_blocked",
        ));
    }
    events
}

fn dynamic_workflow_control_events(
    control: &DxrDynamicWorkflowControl,
) -> Vec<DxrDynamicWorkflowRuntimeEvent> {
    let mut events = vec![
        dynamic_workflow_control_event(control, "dynamic_workflow_control_recorded"),
        dynamic_workflow_control_event(control, "dynamic_workflow_control_checkpoint_bound"),
        dynamic_workflow_control_event(control, "dynamic_workflow_control_resume_cache_checked"),
    ];
    if control.accepted {
        match control.control_action.as_str() {
            "pause" => events.push(dynamic_workflow_control_event(
                control,
                "dynamic_workflow_paused",
            )),
            "resume" => events.push(dynamic_workflow_control_event(
                control,
                "dynamic_workflow_resumed_from_cache",
            )),
            "stop" => events.push(dynamic_workflow_control_event(
                control,
                "dynamic_workflow_stopped_with_checkpoint",
            )),
            "restart_phase" => events.push(dynamic_workflow_control_event(
                control,
                "dynamic_workflow_phase_restart_scheduled",
            )),
            _ => events.push(dynamic_workflow_control_event(
                control,
                "dynamic_workflow_control_rejected",
            )),
        }
    } else {
        events.push(dynamic_workflow_control_event(
            control,
            "dynamic_workflow_control_rejected",
        ));
    }
    events
}

fn dynamic_workflow_event(
    plan: &DxrDynamicWorkflowPlan,
    event_type: &str,
) -> DxrDynamicWorkflowRuntimeEvent {
    DxrDynamicWorkflowRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: plan.tenant_id.clone(),
        job_id: plan.source_job_id.clone(),
        run_id: plan.source_run_id.clone(),
        actor_id: plan.actor_id.clone(),
    }
}

fn dynamic_workflow_control_event(
    control: &DxrDynamicWorkflowControl,
    event_type: &str,
) -> DxrDynamicWorkflowRuntimeEvent {
    DxrDynamicWorkflowRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: control.tenant_id.clone(),
        job_id: control.workflow_plan_id.clone(),
        run_id: control.phase_id.clone(),
        actor_id: control.actor_id.clone(),
    }
}

fn render_dynamic_workflow_response_json(plan: &DxrDynamicWorkflowPlan) -> String {
    format!(
        r#"{{"name":"mdx-dxr-dynamic-workflow-plan","status":{},"runtime":"mdx-dxr-engine","workflow_plan":{},"workflow_plan_id":{},"tenant_id":{},"actor_id":{},"source_job_id":{},"source_run_id":{},"pattern":{},"terminal_state":{},"planned_agent_count":{},"max_concurrent_agents":{},"max_total_agents":{},"agent_batches_required":{},"phase_schedule_count":{},"phase_schedule":[{}],"progress_projection_status":"DXR_DYNAMIC_WORKFLOW_PROGRESS_PLANNED_LOCAL","workflow_progress_view_required":true,"phase_token_totals_required":true,"phase_elapsed_totals_required":true,"live_agent_progress_allowed":false,"fresh_reviewer_context_required":true,"builder_can_self_accept":{},"untrusted_input_quarantined":{},"workflow_shell_access_allowed":{},"workflow_filesystem_access_allowed":{},"agent_spawn_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"production_writes_allowed":false}}"#,
        json_string_literal(&plan.status),
        render_dynamic_workflow_plan_json(plan),
        json_string_literal(&plan.workflow_plan_id),
        json_string_literal(&plan.tenant_id),
        json_string_literal(&plan.actor_id),
        json_string_literal(&plan.source_job_id),
        json_string_literal(&plan.source_run_id),
        json_string_literal(&plan.pattern),
        json_string_literal(&plan.terminal_state),
        plan.planned_agent_count,
        plan.max_concurrent_agents,
        plan.max_total_agents,
        plan.agent_batches_required,
        plan.phase_schedule.len(),
        render_phase_schedule_json(&plan.phase_schedule),
        plan.builder_can_self_accept,
        plan.untrusted_input_quarantined,
        plan.workflow_shell_access_allowed,
        plan.workflow_filesystem_access_allowed
    )
}

fn render_dynamic_workflow_control_response_json(control: &DxrDynamicWorkflowControl) -> String {
    format!(
        r#"{{"name":"mdx-dxr-dynamic-workflow-control","status":{},"runtime":"mdx-dxr-engine","control":{},"control_id":{},"tenant_id":{},"actor_id":{},"workflow_plan_id":{},"phase_id":{},"control_action":{},"terminal_state":{},"progress_state_before":{},"progress_state_after":{},"resume_cache_key":{},"idempotency_key":{},"cached_result_reused":{},"phase_restart_allowed":{},"accepted":{},"mid_run_user_input_allowed":false,"workflow_shell_access_allowed":false,"workflow_filesystem_access_allowed":false,"agent_spawn_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"production_writes_allowed":false}}"#,
        json_string_literal(&control.status),
        render_dynamic_workflow_control_json(control),
        json_string_literal(&control.control_id),
        json_string_literal(&control.tenant_id),
        json_string_literal(&control.actor_id),
        json_string_literal(&control.workflow_plan_id),
        json_string_literal(&control.phase_id),
        json_string_literal(&control.control_action),
        json_string_literal(&control.terminal_state),
        json_string_literal(&control.progress_state_before),
        json_string_literal(&control.progress_state_after),
        json_string_literal(&control.resume_cache_key),
        json_string_literal(&control.idempotency_key),
        control.cached_result_reused,
        control.phase_restart_allowed,
        control.accepted
    )
}

fn render_dynamic_workflow_control_json(control: &DxrDynamicWorkflowControl) -> String {
    format!(
        r#"{{"sequence":{},"control_id":{},"tenant_id":{},"actor_id":{},"workflow_plan_id":{},"phase_id":{},"control_action":{},"status":{},"terminal_state":{},"progress_state_before":{},"progress_state_after":{},"resume_cache_key":{},"idempotency_key":{},"cached_result_reused":{},"phase_restart_allowed":{},"accepted":{},"mid_run_user_input_allowed":false,"workflow_shell_access_allowed":false,"workflow_filesystem_access_allowed":false,"agent_spawn_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"production_writes_allowed":false}}"#,
        control.sequence,
        json_string_literal(&control.control_id),
        json_string_literal(&control.tenant_id),
        json_string_literal(&control.actor_id),
        json_string_literal(&control.workflow_plan_id),
        json_string_literal(&control.phase_id),
        json_string_literal(&control.control_action),
        json_string_literal(&control.status),
        json_string_literal(&control.terminal_state),
        json_string_literal(&control.progress_state_before),
        json_string_literal(&control.progress_state_after),
        json_string_literal(&control.resume_cache_key),
        json_string_literal(&control.idempotency_key),
        control.cached_result_reused,
        control.phase_restart_allowed,
        control.accepted
    )
}

fn durable_workflow_run_for_plan(plan: &DxrDynamicWorkflowPlan) -> DxrWorkflowRun {
    let events = dynamic_workflow_events(plan)
        .iter()
        .enumerate()
        .map(|(index, event)| DxrWorkflowEvent {
            sequence: index + 1,
            workflow_event_id: format!("{}_durable_event_{:03}", plan.workflow_plan_id, index + 1),
            event_type: event.event_type.clone(),
            terminal_state: if event.event_type == "dynamic_workflow_authority_blocked" {
                "DXR_DYNAMIC_WORKFLOW_DURABLE_PLAN_AUTHORITY_BLOCKED"
            } else {
                "DXR_DYNAMIC_WORKFLOW_DURABLE_PLAN_EVENT_RECORDED"
            }
            .to_string(),
        })
        .collect::<Vec<_>>();
    DxrWorkflowRun {
        sequence: plan.sequence,
        workflow_run_id: plan.workflow_plan_id.clone(),
        tenant_id: plan.tenant_id.clone(),
        actor_id: plan.actor_id.clone(),
        workflow_type: "dxr_dynamic_workflow_plan".to_string(),
        workflow_runner_driver: "postgres_durable_workflow_runner".to_string(),
        workflow_runner_provider: "PostgresDurableWorkflowRunner".to_string(),
        workflow_runtime: "postgres_local_dynamic_workflow_control".to_string(),
        task_queue: "mdx-dxr-dynamic-workflows".to_string(),
        namespace: "mdx-local".to_string(),
        retry_policy: "idempotent_dynamic_workflow_plan".to_string(),
        timeout_seconds: 30,
        source_job_id: plan.source_job_id.clone(),
        source_run_id: plan.source_run_id.clone(),
        idempotency_key: format!("{}:{}", plan.tenant_id, plan.workflow_plan_id),
        target_concurrent_engineers: 1000,
        peak_parallel_forge_builds: 5000,
        requested_workflow_count: plan.requested_total_agents,
        accepted_workflow_count: plan.planned_agent_count,
        rejected_workflow_count: plan
            .requested_total_agents
            .saturating_sub(plan.planned_agent_count),
        active_tenant_count: 1,
        max_concurrent_workflows_per_tenant: plan.max_concurrent_agents,
        global_workflow_limit: plan.max_total_agents,
        workflow_batch_size: plan.max_concurrent_agents.max(1),
        workflow_batches_required: plan.agent_batches_required,
        durable_checkpoint_count: plan.phase_schedule.len().saturating_add(2),
        replay_window_seconds: 86_400,
        resume_checkpoint_interval_seconds: 30,
        resume_strategy: "checkpointed_phase_replay_with_resume_cache".to_string(),
        reviewer_context_mode: plan.reviewer_context.clone(),
        status: plan.status.clone(),
        terminal_state: plan.terminal_state.clone(),
        live_worker_execution_allowed: false,
        temporal_durability_claimed: false,
        events,
        shards: Vec::new(),
    }
}

fn durable_workflow_run_for_control(
    control: &DxrDynamicWorkflowControl,
    control_events: &[DxrDynamicWorkflowRuntimeEvent],
) -> DxrWorkflowRun {
    let events = control_events
        .iter()
        .enumerate()
        .map(|(index, event)| DxrWorkflowEvent {
            sequence: index + 1,
            workflow_event_id: format!("{}_durable_event_{:03}", control.control_id, index + 1),
            event_type: event.event_type.clone(),
            terminal_state: control.terminal_state.clone(),
        })
        .collect::<Vec<_>>();
    DxrWorkflowRun {
        sequence: control.sequence,
        workflow_run_id: control.control_id.clone(),
        tenant_id: control.tenant_id.clone(),
        actor_id: control.actor_id.clone(),
        workflow_type: "dxr_dynamic_workflow_control".to_string(),
        workflow_runner_driver: "postgres_durable_workflow_runner".to_string(),
        workflow_runner_provider: "PostgresDurableWorkflowRunner".to_string(),
        workflow_runtime: "postgres_local_dynamic_workflow_control".to_string(),
        task_queue: "mdx-dxr-dynamic-workflow-controls".to_string(),
        namespace: "mdx-local".to_string(),
        retry_policy: "idempotent_dynamic_workflow_control".to_string(),
        timeout_seconds: 30,
        source_job_id: control.workflow_plan_id.clone(),
        source_run_id: control.phase_id.clone(),
        idempotency_key: control.idempotency_key.clone(),
        target_concurrent_engineers: 1000,
        peak_parallel_forge_builds: 5000,
        requested_workflow_count: 1,
        accepted_workflow_count: usize::from(control.accepted),
        rejected_workflow_count: usize::from(!control.accepted),
        active_tenant_count: 1,
        max_concurrent_workflows_per_tenant: 1,
        global_workflow_limit: 5000,
        workflow_batch_size: 1,
        workflow_batches_required: 1,
        durable_checkpoint_count: if control.accepted { 2 } else { 1 },
        replay_window_seconds: 86_400,
        resume_checkpoint_interval_seconds: 30,
        resume_strategy: "control_checkpoint_resume_cache_replay".to_string(),
        reviewer_context_mode: "fresh_context_required".to_string(),
        status: control.status.clone(),
        terminal_state: control.terminal_state.clone(),
        live_worker_execution_allowed: false,
        temporal_durability_claimed: false,
        events,
        shards: Vec::new(),
    }
}

fn render_dynamic_workflow_plan_json(plan: &DxrDynamicWorkflowPlan) -> String {
    format!(
        r#"{{"sequence":{},"workflow_plan_id":{},"tenant_id":{},"actor_id":{},"workflow_name":{},"source_job_id":{},"source_run_id":{},"pattern":{},"phase_count":{},"requested_total_agents":{},"planned_agent_count":{},"max_concurrent_agents":{},"max_total_agents":{},"agent_batches_required":{},"phase_schedule_count":{},"phase_schedule":[{}],"token_budget":{},"script_persistence":{},"intermediate_state_location":{},"approval_mode":{},"reviewer_context":{},"artifact_retention_days":{},"resumable_within_session":{},"progress_projection_status":"DXR_DYNAMIC_WORKFLOW_PROGRESS_PLANNED_LOCAL","workflow_progress_view_required":true,"phase_token_totals_required":true,"phase_elapsed_totals_required":true,"live_agent_progress_allowed":false,"builder_can_self_accept":{},"untrusted_input_quarantined":{},"workflow_shell_access_allowed":{},"workflow_filesystem_access_allowed":{},"agent_tool_allowlist_inherited":{},"terminal_state":{},"status":{},"accepted":{},"agent_spawn_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"production_writes_allowed":false}}"#,
        plan.sequence,
        json_string_literal(&plan.workflow_plan_id),
        json_string_literal(&plan.tenant_id),
        json_string_literal(&plan.actor_id),
        json_string_literal(&plan.workflow_name),
        json_string_literal(&plan.source_job_id),
        json_string_literal(&plan.source_run_id),
        json_string_literal(&plan.pattern),
        plan.phase_count,
        plan.requested_total_agents,
        plan.planned_agent_count,
        plan.max_concurrent_agents,
        plan.max_total_agents,
        plan.agent_batches_required,
        plan.phase_schedule.len(),
        render_phase_schedule_json(&plan.phase_schedule),
        plan.token_budget,
        json_string_literal(&plan.script_persistence),
        json_string_literal(&plan.intermediate_state_location),
        json_string_literal(&plan.approval_mode),
        json_string_literal(&plan.reviewer_context),
        plan.artifact_retention_days,
        plan.resumable_within_session,
        plan.builder_can_self_accept,
        plan.untrusted_input_quarantined,
        plan.workflow_shell_access_allowed,
        plan.workflow_filesystem_access_allowed,
        plan.agent_tool_allowlist_inherited,
        json_string_literal(&plan.terminal_state),
        json_string_literal(&plan.status),
        plan.accepted
    )
}

fn build_phase_schedule(
    plan_sequence: usize,
    phase_count: usize,
    planned_agent_count: usize,
    max_concurrent_agents: usize,
    token_budget: usize,
    requested_pattern: &str,
    reviewer_context: &str,
) -> Vec<DxrDynamicWorkflowPhase> {
    let phase_count = phase_count.max(1);
    let base_agents = planned_agent_count / phase_count;
    let extra_agents = planned_agent_count % phase_count;
    let base_tokens = token_budget / phase_count;
    let extra_tokens = token_budget % phase_count;
    let patterns = workflow_patterns();
    (0..phase_count)
        .map(|index| {
            let requested_agents = base_agents + usize::from(index < extra_agents);
            let pattern = if index == 0 {
                requested_pattern.to_string()
            } else {
                patterns[index % patterns.len()].name.to_string()
            };
            let phase_role = patterns
                .iter()
                .find(|item| item.name == pattern)
                .map(|item| item.phase_role)
                .unwrap_or("script_phase")
                .to_string();
            DxrDynamicWorkflowPhase {
                phase_index: index + 1,
                phase_id: format!(
                    "dxr_dynamic_workflow_plan_{:06}_phase_{:03}",
                    plan_sequence,
                    index + 1
                ),
                pattern,
                phase_role,
                requested_agents,
                max_concurrent_agents,
                batches_required: ceil_div(requested_agents, max_concurrent_agents.max(1)),
                checkpoint_policy: "phase_result_cached_for_replay".to_string(),
                reviewer_context: reviewer_context.to_string(),
                authority_boundary:
                    "phase_agents_inherit_allowlist_no_workflow_shell_or_filesystem".to_string(),
                progress_state: "planned_local_not_started".to_string(),
                token_budget: base_tokens + usize::from(index < extra_tokens),
                elapsed_budget_ms: requested_agents.saturating_mul(1250),
                resume_cache_key: format!(
                    "dxr_dynamic_workflow_plan_{:06}_phase_{:03}_resume_cache",
                    plan_sequence,
                    index + 1
                ),
                phase_result_cached_for_replay: true,
            }
        })
        .collect()
}

fn render_phase_schedule_json(phases: &[DxrDynamicWorkflowPhase]) -> String {
    phases
        .iter()
        .map(render_phase_json)
        .collect::<Vec<_>>()
        .join(",")
}

fn render_phase_json(phase: &DxrDynamicWorkflowPhase) -> String {
    format!(
        r#"{{"phase_index":{},"phase_id":{},"pattern":{},"phase_role":{},"requested_agents":{},"max_concurrent_agents":{},"batches_required":{},"checkpoint_policy":{},"reviewer_context":{},"authority_boundary":{},"progress_state":{},"token_budget":{},"elapsed_budget_ms":{},"resume_cache_key":{},"phase_result_cached_for_replay":{},"live_agent_progress_allowed":false,"agent_spawn_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"workflow_shell_access_allowed":false,"workflow_filesystem_access_allowed":false}}"#,
        phase.phase_index,
        json_string_literal(&phase.phase_id),
        json_string_literal(&phase.pattern),
        json_string_literal(&phase.phase_role),
        phase.requested_agents,
        phase.max_concurrent_agents,
        phase.batches_required,
        json_string_literal(&phase.checkpoint_policy),
        json_string_literal(&phase.reviewer_context),
        json_string_literal(&phase.authority_boundary),
        json_string_literal(&phase.progress_state),
        phase.token_budget,
        phase.elapsed_budget_ms,
        json_string_literal(&phase.resume_cache_key),
        phase.phase_result_cached_for_replay
    )
}

struct WorkflowPattern {
    name: &'static str,
    purpose: &'static str,
    phase_role: &'static str,
}

fn workflow_patterns() -> Vec<WorkflowPattern> {
    vec![
        WorkflowPattern {
            name: "classify_and_act",
            purpose: "route small tasks to lean local rails and large tasks to scripted multi-agent runs",
            phase_role: "intake_router",
        },
        WorkflowPattern {
            name: "fan_out_and_synthesize",
            purpose: "parallelize codebase sweeps, migration slices, and evidence gathering",
            phase_role: "parallel_discovery",
        },
        WorkflowPattern {
            name: "adversarial_verification",
            purpose: "separate builder context from verifier context before merge readiness",
            phase_role: "fresh_context_review",
        },
        WorkflowPattern {
            name: "loop_until_done",
            purpose: "rerun deterministic gates until hard goal criteria pass or budget closes",
            phase_role: "bounded_iteration",
        },
        WorkflowPattern {
            name: "generate_and_filter",
            purpose: "rank alternatives with a verifier and retain only evidence-backed outputs",
            phase_role: "candidate_selection",
        },
        WorkflowPattern {
            name: "quarantine_untrusted_input",
            purpose: "keep external tickets, code, logs, and documents away from privileged agents",
            phase_role: "input_sanitization",
        },
    ]
}

fn render_workflow_pattern_json(pattern: &WorkflowPattern) -> String {
    format!(
        r#"{{"name":{},"purpose":{},"phase_role":{},"builder_can_self_accept":false,"fresh_reviewer_context_required":true,"human_ratification_boundary":"required_for_merge_or_production_write"}}"#,
        json_string_literal(pattern.name),
        json_string_literal(pattern.purpose),
        json_string_literal(pattern.phase_role)
    )
}

fn ceil_div(numerator: usize, denominator: usize) -> usize {
    numerator.saturating_add(denominator.saturating_sub(1)) / denominator
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
    fn dynamic_workflow_plan_enforces_agent_caps_and_reviewer_separation() {
        let mut runtime = DxrDynamicWorkflowRuntime::new();
        let result = runtime
            .submit_json(
                r#"{"pattern":"adversarial_verification","planned_agent_count":64,"max_concurrent_agents":16,"builder_can_self_accept":false}"#,
            )
            .expect("dynamic workflow plan");
        assert!(
            result
                .body
                .contains("DXR_DYNAMIC_WORKFLOW_PLAN_ACCEPTED_LOCAL")
        );
        assert!(result.body.contains("\"agent_batches_required\":4"));
        assert!(result.body.contains("\"phase_schedule_count\":6"));
        assert!(result.body.contains("phase_result_cached_for_replay"));
        assert!(
            result
                .body
                .contains("DXR_DYNAMIC_WORKFLOW_PROGRESS_PLANNED_LOCAL")
        );
        assert!(result.body.contains("planned_local_not_started"));
        assert!(result.body.contains("\"builder_can_self_accept\":false"));
        assert!(result.body.contains("\"agent_spawn_started\":false"));
        assert!(result.events.len() >= 9);
    }
}
