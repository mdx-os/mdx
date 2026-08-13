use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_TARGET_ENGINEERS: usize = 1000;
const DEFAULT_PEAK_PARALLEL_FORGE_BUILDS: usize = 5000;
const DEFAULT_REQUESTED_FORGE_RUNS: usize = 5200;
const DEFAULT_GLOBAL_FORGE_RUN_LIMIT: usize = 5000;
const DEFAULT_TENANT_FORGE_RUN_LIMIT: usize = 500;
const DEFAULT_ACTIVE_TENANT_COUNT: usize = 10;
const DEFAULT_SANDBOX_POOL_SIZE: usize = 2048;
const DEFAULT_QUEUE_DEPTH_LIMIT: usize = 50_000;
const DEFAULT_AGENT_CONCURRENCY_PER_WORKFLOW: usize = 16;
const DEFAULT_TOTAL_AGENTS_PER_WORKFLOW: usize = 1000;
const DEFAULT_MEDIUM_BUILD_P95_SECONDS: usize = 180;
const DEFAULT_COMPLEX_BUILD_P95_SECONDS: usize = 600;
const DEFAULT_DIRECT_AGENT_LATENCY_RATIO_BPS: usize = 115;
const DEFAULT_EXTERNAL_SANDBOX_READY_P95_MS: usize = 2_500;
const DEFAULT_EXTERNAL_SANDBOX_MAX_RUNTIME_SECONDS: usize = 3_600;
const DEFAULT_EXTERNAL_SANDBOX_ARTIFACT_RETENTION_HOURS: usize = 24;

#[derive(Default)]
pub struct DxrCapacityRuntime {
    plans: Vec<DxrCapacityPlan>,
    admissions: Vec<DxrSandboxAdmission>,
    sandbox_adapter_turn_ons: Vec<DxrSandboxAdapterTurnOn>,
    external_preflights: Vec<DxrExternalSandboxPreflight>,
    sandbox_authority_envelopes: Vec<DxrSandboxAuthorityEnvelope>,
    execution_admissions: Vec<DxrExecutionAdmission>,
    next_plan: usize,
    next_admission: usize,
    next_sandbox_adapter_turn_on: usize,
    next_external_preflight: usize,
    next_sandbox_authority_envelope: usize,
    next_execution_admission: usize,
}

pub struct DxrCapacityResult {
    pub body: String,
    pub events: Vec<DxrCapacityRuntimeEvent>,
}

pub struct DxrCapacityRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrCapacityPlan {
    sequence: usize,
    capacity_plan_id: String,
    tenant_id: String,
    actor_id: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    requested_concurrent_forge_runs: usize,
    global_max_concurrent_forge_runs: usize,
    max_concurrent_forge_runs_per_tenant: usize,
    active_tenant_count: usize,
    queue_depth_limit: usize,
    requested_sandbox_count: usize,
    sandbox_pool_size: usize,
    lease_duration_ms: u64,
    heartbeat_interval_ms: u64,
    stale_run_recovery_ms: u64,
    max_concurrent_agents_per_workflow: usize,
    max_total_agents_per_workflow: usize,
    medium_build_p95_seconds: usize,
    complex_build_p95_seconds: usize,
    direct_agent_latency_ratio_bps: usize,
    workflow_phase_count: usize,
    workflow_pattern_count: usize,
    fairness_policy: String,
    backpressure_policy: String,
    repo_isolation_mode: String,
    external_repo_mode: String,
    sandbox_isolation_policy: String,
    sandbox_driver_registry: Vec<SandboxDriver>,
    workflow_patterns: Vec<WorkflowPattern>,
    admission_decision: String,
    terminal_state: String,
    overloaded: bool,
    accepted_forge_runs: usize,
    queued_forge_runs: usize,
}

#[derive(Clone)]
struct SandboxDriver {
    name: &'static str,
    isolation: &'static str,
    status: &'static str,
    execution_surface: &'static str,
}

#[derive(Clone)]
struct DxrSandboxAdapterTurnOn {
    sequence: usize,
    turn_on_id: String,
    receipt_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    sandbox_driver: String,
    sandbox_provider: String,
    adapter_version: String,
    driver_is_registered: bool,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    max_parallel_sandboxes: usize,
    warm_pool_target: usize,
    ready_p95_ms: usize,
    ready_p95_ms_ceiling: usize,
    max_runtime_seconds: usize,
    artifact_retention_hours: usize,
    capability_count: usize,
    status: String,
    terminal_state: String,
    admission_decision: String,
    rejected: bool,
    rejection_reason: String,
    adapter_ready_for_preflight: bool,
    adapter_registry_observed: bool,
    isolation_boundary_observed: bool,
    warm_pool_observed: bool,
    snapshot_restore_observed: bool,
    filesystem_watch_observed: bool,
    preview_service_observed: bool,
    suspend_resume_observed: bool,
    log_stream_observed: bool,
    artifact_retention_observed: bool,
    network_policy_observed: bool,
    secret_policy_observed: bool,
    egress_policy_observed: bool,
    ready_p95_observed: bool,
    scale_capacity_observed: bool,
    human_ratification_gate_observed: bool,
    evidence_quarantine_observed: bool,
}

struct SandboxAdapterTurnOnRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    sandbox_driver: String,
    sandbox_provider: String,
    adapter_version: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    max_parallel_sandboxes: usize,
    warm_pool_target: usize,
    ready_p95_ms: usize,
    ready_p95_ms_ceiling: usize,
    max_runtime_seconds: usize,
    artifact_retention_hours: usize,
    adapter_registry_observed: bool,
    isolation_boundary_observed: bool,
    warm_pool_observed: bool,
    snapshot_restore_observed: bool,
    filesystem_watch_observed: bool,
    preview_service_observed: bool,
    suspend_resume_observed: bool,
    log_stream_observed: bool,
    artifact_retention_observed: bool,
    network_policy_observed: bool,
    secret_policy_observed: bool,
    egress_policy_observed: bool,
    ready_p95_observed: bool,
    scale_capacity_observed: bool,
    human_ratification_gate_observed: bool,
    evidence_quarantine_observed: bool,
    adapter_execution_requested: bool,
    sandbox_process_start_requested: bool,
    external_repo_checkout_requested: bool,
    network_requested: bool,
    secret_inheritance_requested: bool,
    filesystem_mutation_requested: bool,
    provider_call_requested: bool,
    tool_execution_requested: bool,
    worker_execution_requested: bool,
    ci_claim_requested: bool,
    deployment_requested: bool,
    production_write_requested: bool,
}

#[derive(Clone)]
struct WorkflowPattern {
    name: &'static str,
    purpose: &'static str,
    reviewer_context: &'static str,
}

struct CapacityRequest {
    tenant_id: String,
    actor_id: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    requested_concurrent_forge_runs: usize,
    global_max_concurrent_forge_runs: usize,
    max_concurrent_forge_runs_per_tenant: usize,
    active_tenant_count: usize,
    queue_depth_limit: usize,
    requested_sandbox_count: usize,
    sandbox_pool_size: usize,
    lease_duration_ms: u64,
    heartbeat_interval_ms: u64,
    stale_run_recovery_ms: u64,
    max_concurrent_agents_per_workflow: usize,
    max_total_agents_per_workflow: usize,
    medium_build_p95_seconds: usize,
    complex_build_p95_seconds: usize,
    direct_agent_latency_ratio_bps: usize,
    workflow_phase_count: usize,
    fairness_policy: String,
    backpressure_policy: String,
    repo_isolation_mode: String,
    external_repo_mode: String,
    sandbox_isolation_policy: String,
}

#[derive(Clone)]
struct DxrSandboxAdmission {
    sequence: usize,
    admission_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    requested_sandbox_count: usize,
    active_sandbox_count: usize,
    sandbox_pool_size: usize,
    tenant_active_forge_runs: usize,
    global_active_forge_runs: usize,
    queued_forge_runs: usize,
    global_max_concurrent_forge_runs: usize,
    max_concurrent_forge_runs_per_tenant: usize,
    queue_depth_limit: usize,
    repo_ref: String,
    repo_isolation_mode: String,
    sandbox_driver: String,
    sandbox_isolation_policy: String,
    admission_decision: String,
    terminal_state: String,
    admitted_to_queue: bool,
    admitted_to_sandbox_pool: bool,
    rejected: bool,
    rejection_reason: String,
}

struct SandboxAdmissionRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    requested_sandbox_count: usize,
    active_sandbox_count: usize,
    sandbox_pool_size: usize,
    tenant_active_forge_runs: usize,
    global_active_forge_runs: usize,
    queued_forge_runs: usize,
    global_max_concurrent_forge_runs: usize,
    max_concurrent_forge_runs_per_tenant: usize,
    queue_depth_limit: usize,
    repo_ref: String,
    repo_isolation_mode: String,
    sandbox_driver: String,
    sandbox_isolation_policy: String,
    network_allowed: bool,
    secret_inheritance_allowed: bool,
    production_write_authority: bool,
}

#[derive(Clone)]
struct DxrExternalSandboxPreflight {
    sequence: usize,
    preflight_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    sandbox_driver: String,
    sandbox_provider: String,
    driver_is_registered: bool,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    requested_parallel_sandboxes: usize,
    max_parallel_sandboxes: usize,
    warm_pool_target: usize,
    external_repo_ref: String,
    external_repo_url_hash: String,
    repo_checkout_mode: String,
    evidence_import_policy: String,
    network_policy: String,
    filesystem_policy: String,
    secret_policy: String,
    egress_policy: String,
    workspace_snapshot_policy: String,
    filesystem_watch_policy: String,
    preview_service_policy: String,
    suspend_resume_policy: String,
    max_runtime_seconds: usize,
    ready_p95_ms: usize,
    artifact_retention_hours: usize,
    adapter_turn_on_receipt_id: String,
    adapter_turn_on_status: String,
    adapter_turn_on_observed: bool,
    adapter_capability_count: usize,
    sandbox_authority_receipt_id: String,
    human_ratification_receipt_id: String,
    admission_decision: String,
    status: String,
    terminal_state: String,
    authority_rejected: bool,
    snapshot_restore_required: bool,
    filesystem_watch_required: bool,
    preview_service_receipt_required: bool,
    suspend_resume_required: bool,
}

struct ExternalSandboxPreflightRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    sandbox_driver: String,
    sandbox_provider: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    requested_parallel_sandboxes: usize,
    max_parallel_sandboxes: usize,
    warm_pool_target: usize,
    external_repo_ref: String,
    external_repo_url_hash: String,
    repo_checkout_mode: String,
    evidence_import_policy: String,
    network_policy: String,
    filesystem_policy: String,
    secret_policy: String,
    egress_policy: String,
    workspace_snapshot_policy: String,
    filesystem_watch_policy: String,
    preview_service_policy: String,
    suspend_resume_policy: String,
    max_runtime_seconds: usize,
    ready_p95_ms: usize,
    artifact_retention_hours: usize,
    adapter_turn_on_receipt_id: String,
    adapter_turn_on_status: String,
    adapter_turn_on_observed: bool,
    adapter_capability_count: usize,
    sandbox_authority_receipt_id: String,
    human_ratification_receipt_id: String,
    adapter_execution_allowed: bool,
    external_repo_checkout_allowed: bool,
    snapshot_restore_required: bool,
    filesystem_watch_required: bool,
    preview_service_receipt_required: bool,
    suspend_resume_required: bool,
    network_allowed: bool,
    secret_inheritance_allowed: bool,
    filesystem_mutation_allowed: bool,
    production_write_authority: bool,
}

#[derive(Clone)]
struct DxrSandboxAuthorityEnvelope {
    sequence: usize,
    envelope_id: String,
    receipt_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    sandbox_admission_id: String,
    sandbox_admission_decision: String,
    sandbox_admission_observed: bool,
    adapter_turn_on_receipt_id: String,
    adapter_turn_on_status: String,
    adapter_turn_on_observed: bool,
    external_sandbox_preflight_id: String,
    external_sandbox_preflight_status: String,
    external_sandbox_preflight_observed: bool,
    human_ratification_receipt_id: String,
    tool_policy_receipt_id: String,
    reviewer_separation_receipt_id: String,
    dispatch_claim_id: String,
    heartbeat_receipt_id: String,
    durable_workflow_receipt_id: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    sandbox_authority_staged: bool,
    status: String,
    terminal_state: String,
    envelope_decision: String,
    rejected: bool,
    rejection_reason: String,
}

struct SandboxAuthorityEnvelopeRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    sandbox_admission_id: String,
    sandbox_admission_decision: String,
    sandbox_admission_observed: bool,
    adapter_turn_on_receipt_id: String,
    adapter_turn_on_status: String,
    adapter_turn_on_observed: bool,
    external_sandbox_preflight_id: String,
    external_sandbox_preflight_status: String,
    external_sandbox_preflight_observed: bool,
    human_ratification_receipt_id: String,
    tool_policy_receipt_id: String,
    reviewer_separation_receipt_id: String,
    dispatch_claim_id: String,
    heartbeat_receipt_id: String,
    durable_workflow_receipt_id: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    sandbox_process_start_requested: bool,
    adapter_execution_requested: bool,
    external_repo_checkout_requested: bool,
    network_requested: bool,
    secret_inheritance_requested: bool,
    filesystem_mutation_requested: bool,
    provider_call_requested: bool,
    tool_execution_requested: bool,
    worker_execution_requested: bool,
    ci_claim_requested: bool,
    deployment_requested: bool,
    production_write_requested: bool,
}

#[derive(Clone)]
struct DxrExecutionAdmission {
    sequence: usize,
    admission_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    dispatch_recovery_plan_observed: bool,
    dispatch_claim_id: String,
    heartbeat_receipt_id: String,
    durable_workflow_receipt_id: String,
    live_worker_preflight_id: String,
    live_worker_preflight_status: String,
    sandbox_admission_id: String,
    sandbox_admission_decision: String,
    sandbox_authority_envelope_id: String,
    sandbox_authority_receipt_id: String,
    sandbox_authority_status: String,
    sandbox_authority_observed: bool,
    external_sandbox_preflight_id: String,
    tool_policy_receipt_id: String,
    reviewer_separation_receipt_id: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    tenant_active_forge_runs: usize,
    global_active_forge_runs: usize,
    queued_forge_runs: usize,
    global_max_concurrent_forge_runs: usize,
    max_concurrent_forge_runs_per_tenant: usize,
    queue_depth_limit: usize,
    admission_decision: String,
    terminal_state: String,
    admitted_to_execution_preflight: bool,
    admitted_to_queue: bool,
    rejected: bool,
    rejection_reason: String,
}

struct ExecutionAdmissionRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    dispatch_recovery_plan_observed: bool,
    dispatch_claim_id: String,
    heartbeat_receipt_id: String,
    durable_workflow_receipt_id: String,
    live_worker_preflight_id: String,
    live_worker_preflight_status: String,
    sandbox_admission_id: String,
    sandbox_admission_decision: String,
    sandbox_authority_envelope_id: String,
    sandbox_authority_receipt_id: String,
    sandbox_authority_status: String,
    sandbox_authority_observed: bool,
    external_sandbox_preflight_id: String,
    tool_policy_receipt_id: String,
    reviewer_separation_receipt_id: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    tenant_active_forge_runs: usize,
    global_active_forge_runs: usize,
    queued_forge_runs: usize,
    global_max_concurrent_forge_runs: usize,
    max_concurrent_forge_runs_per_tenant: usize,
    queue_depth_limit: usize,
    network_allowed: bool,
    secret_inheritance_allowed: bool,
    filesystem_mutation_allowed: bool,
    production_write_authority: bool,
}

impl DxrCapacityRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrCapacityResult, String> {
        let request = parse_capacity_request(body)?;
        self.next_plan += 1;
        let accepted_forge_runs = request
            .requested_concurrent_forge_runs
            .min(request.global_max_concurrent_forge_runs)
            .min(
                request
                    .max_concurrent_forge_runs_per_tenant
                    .saturating_mul(request.active_tenant_count),
            );
        let sandbox_cap = request.sandbox_pool_size.min(accepted_forge_runs);
        let overload_by_runs =
            request.requested_concurrent_forge_runs > request.global_max_concurrent_forge_runs;
        let overload_by_sandbox = request.requested_sandbox_count > request.sandbox_pool_size;
        let overloaded = overload_by_runs || overload_by_sandbox;
        let queued_forge_runs = request
            .requested_concurrent_forge_runs
            .saturating_sub(sandbox_cap);
        let admission_decision = if overloaded {
            "backpressure_applied"
        } else {
            "admitted_under_capacity"
        };
        let terminal_state = if overloaded {
            "DXR_CAPACITY_SANDBOX_PLAN_RECORDED_BACKPRESSURE_APPLIED"
        } else {
            "DXR_CAPACITY_SANDBOX_PLAN_RECORDED_ADMITTED"
        };
        let plan = DxrCapacityPlan {
            sequence: self.next_plan,
            capacity_plan_id: format!("dxr_capacity_plan_{:06}", self.next_plan),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            requested_concurrent_forge_runs: request.requested_concurrent_forge_runs,
            global_max_concurrent_forge_runs: request.global_max_concurrent_forge_runs,
            max_concurrent_forge_runs_per_tenant: request.max_concurrent_forge_runs_per_tenant,
            active_tenant_count: request.active_tenant_count,
            queue_depth_limit: request.queue_depth_limit,
            requested_sandbox_count: request.requested_sandbox_count,
            sandbox_pool_size: request.sandbox_pool_size,
            lease_duration_ms: request.lease_duration_ms,
            heartbeat_interval_ms: request.heartbeat_interval_ms,
            stale_run_recovery_ms: request.stale_run_recovery_ms,
            max_concurrent_agents_per_workflow: request.max_concurrent_agents_per_workflow,
            max_total_agents_per_workflow: request.max_total_agents_per_workflow,
            medium_build_p95_seconds: request.medium_build_p95_seconds,
            complex_build_p95_seconds: request.complex_build_p95_seconds,
            direct_agent_latency_ratio_bps: request.direct_agent_latency_ratio_bps,
            workflow_phase_count: request.workflow_phase_count,
            workflow_pattern_count: workflow_patterns().len(),
            fairness_policy: request.fairness_policy,
            backpressure_policy: request.backpressure_policy,
            repo_isolation_mode: request.repo_isolation_mode,
            external_repo_mode: request.external_repo_mode,
            sandbox_isolation_policy: request.sandbox_isolation_policy,
            sandbox_driver_registry: sandbox_drivers(),
            workflow_patterns: workflow_patterns(),
            admission_decision: admission_decision.to_string(),
            terminal_state: terminal_state.to_string(),
            overloaded,
            accepted_forge_runs: sandbox_cap,
            queued_forge_runs,
        };
        let events = capacity_events(&plan);
        let body = render_capacity_plan_response_json(&plan);
        self.plans.push(plan);
        Ok(DxrCapacityResult { body, events })
    }

    pub fn submit_sandbox_admission_json(
        &mut self,
        body: &str,
    ) -> Result<DxrCapacityResult, String> {
        let request = parse_sandbox_admission_request(body)?;
        self.next_admission += 1;
        let security_rejected = request.network_allowed
            || request.secret_inheritance_allowed
            || request.production_write_authority;
        let tenant_over_limit =
            request.tenant_active_forge_runs >= request.max_concurrent_forge_runs_per_tenant;
        let global_over_limit =
            request.global_active_forge_runs >= request.global_max_concurrent_forge_runs;
        let sandbox_pool_full = request
            .active_sandbox_count
            .saturating_add(request.requested_sandbox_count)
            > request.sandbox_pool_size;
        let queue_full = request.queued_forge_runs >= request.queue_depth_limit;
        let (
            admission_decision,
            terminal_state,
            admitted_to_queue,
            admitted_to_sandbox_pool,
            rejected,
            rejection_reason,
        ) = if security_rejected {
            (
                "rejected_security_boundary",
                "DXR_SANDBOX_ADMISSION_REJECTED_SECURITY_BOUNDARY",
                false,
                false,
                true,
                "network_secret_or_production_write_authority_requested",
            )
        } else if queue_full {
            (
                "rejected_queue_full",
                "DXR_SANDBOX_ADMISSION_REJECTED_QUEUE_FULL",
                false,
                false,
                true,
                "queue_depth_limit_reached",
            )
        } else if tenant_over_limit || global_over_limit || sandbox_pool_full {
            (
                "queued_backpressure",
                "DXR_SANDBOX_ADMISSION_QUEUED_BACKPRESSURE",
                true,
                false,
                false,
                "tenant_global_or_sandbox_pool_pressure",
            )
        } else {
            (
                "admitted_to_sandbox_pool_without_start",
                "DXR_SANDBOX_ADMISSION_RECORDED_POOL_SLOT_RESERVED",
                false,
                true,
                false,
                "none",
            )
        };
        let admission = DxrSandboxAdmission {
            sequence: self.next_admission,
            admission_id: format!("dxr_sandbox_admission_{:06}", self.next_admission),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            requested_sandbox_count: request.requested_sandbox_count,
            active_sandbox_count: request.active_sandbox_count,
            sandbox_pool_size: request.sandbox_pool_size,
            tenant_active_forge_runs: request.tenant_active_forge_runs,
            global_active_forge_runs: request.global_active_forge_runs,
            queued_forge_runs: request.queued_forge_runs,
            global_max_concurrent_forge_runs: request.global_max_concurrent_forge_runs,
            max_concurrent_forge_runs_per_tenant: request.max_concurrent_forge_runs_per_tenant,
            queue_depth_limit: request.queue_depth_limit,
            repo_ref: request.repo_ref,
            repo_isolation_mode: request.repo_isolation_mode,
            sandbox_driver: request.sandbox_driver,
            sandbox_isolation_policy: request.sandbox_isolation_policy,
            admission_decision: admission_decision.to_string(),
            terminal_state: terminal_state.to_string(),
            admitted_to_queue,
            admitted_to_sandbox_pool,
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let events = sandbox_admission_events(&admission);
        let body = render_sandbox_admission_response_json(&admission);
        self.admissions.push(admission);
        Ok(DxrCapacityResult { body, events })
    }

    pub fn submit_sandbox_adapter_turn_on_json(
        &mut self,
        body: &str,
    ) -> Result<DxrCapacityResult, String> {
        let request = parse_sandbox_adapter_turn_on_request(body)?;
        self.next_sandbox_adapter_turn_on += 1;
        let driver_is_registered = sandbox_drivers()
            .iter()
            .any(|driver| driver.name == request.sandbox_driver);
        let observed_capabilities = sandbox_adapter_observed_capability_count(&request);
        let all_required_capabilities_observed = observed_capabilities == 16;
        let authority_requested = request.adapter_execution_requested
            || request.sandbox_process_start_requested
            || request.external_repo_checkout_requested
            || request.network_requested
            || request.secret_inheritance_requested
            || request.filesystem_mutation_requested
            || request.provider_call_requested
            || request.tool_execution_requested
            || request.worker_execution_requested
            || request.ci_claim_requested
            || request.deployment_requested
            || request.production_write_requested;
        let scale_rejected = request.max_parallel_sandboxes < request.peak_parallel_forge_builds
            || request.warm_pool_target > request.max_parallel_sandboxes
            || request.ready_p95_ms > request.ready_p95_ms_ceiling
            || request.ready_p95_ms_ceiling > DEFAULT_EXTERNAL_SANDBOX_READY_P95_MS;
        let rejected = !driver_is_registered
            || !all_required_capabilities_observed
            || authority_requested
            || scale_rejected;
        let (status, terminal_state, admission_decision, rejection_reason) = if rejected {
            (
                "DXR_SANDBOX_ADAPTER_TURN_ON_REJECTED_BOUNDARY",
                "DXR_SANDBOX_ADAPTER_TURN_ON_RECORDED_EXECUTION_BLOCKED",
                "rejected_sandbox_adapter_turn_on_boundary",
                if !driver_is_registered {
                    "sandbox_driver_not_registered"
                } else if !all_required_capabilities_observed {
                    "required_adapter_capability_evidence_missing"
                } else if authority_requested {
                    "sandbox_adapter_authority_requested"
                } else {
                    "sandbox_adapter_scale_or_latency_floor_not_met"
                },
            )
        } else {
            (
                "LIVE-LOCAL-DXR-SANDBOX-ADAPTER-TURN-ON-FLOOR",
                "DXR_SANDBOX_ADAPTER_TURN_ON_RECORDED_EXECUTION_BLOCKED",
                "adapter_turn_on_recorded_for_preflight_execution_blocked",
                "none",
            )
        };
        let turn_on = DxrSandboxAdapterTurnOn {
            sequence: self.next_sandbox_adapter_turn_on,
            turn_on_id: format!(
                "dxr_sandbox_adapter_turn_on_{:06}",
                self.next_sandbox_adapter_turn_on
            ),
            receipt_id: format!(
                "dxr_sandbox_adapter_turn_on_receipt_{:06}",
                self.next_sandbox_adapter_turn_on
            ),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            sandbox_driver: request.sandbox_driver,
            sandbox_provider: request.sandbox_provider,
            adapter_version: request.adapter_version,
            driver_is_registered,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            max_parallel_sandboxes: request.max_parallel_sandboxes,
            warm_pool_target: request.warm_pool_target,
            ready_p95_ms: request.ready_p95_ms,
            ready_p95_ms_ceiling: request.ready_p95_ms_ceiling,
            max_runtime_seconds: request.max_runtime_seconds,
            artifact_retention_hours: request.artifact_retention_hours,
            capability_count: observed_capabilities,
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            admission_decision: admission_decision.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
            adapter_ready_for_preflight: !rejected,
            adapter_registry_observed: request.adapter_registry_observed,
            isolation_boundary_observed: request.isolation_boundary_observed,
            warm_pool_observed: request.warm_pool_observed,
            snapshot_restore_observed: request.snapshot_restore_observed,
            filesystem_watch_observed: request.filesystem_watch_observed,
            preview_service_observed: request.preview_service_observed,
            suspend_resume_observed: request.suspend_resume_observed,
            log_stream_observed: request.log_stream_observed,
            artifact_retention_observed: request.artifact_retention_observed,
            network_policy_observed: request.network_policy_observed,
            secret_policy_observed: request.secret_policy_observed,
            egress_policy_observed: request.egress_policy_observed,
            ready_p95_observed: request.ready_p95_observed,
            scale_capacity_observed: request.scale_capacity_observed,
            human_ratification_gate_observed: request.human_ratification_gate_observed,
            evidence_quarantine_observed: request.evidence_quarantine_observed,
        };
        let events = sandbox_adapter_turn_on_events(&turn_on);
        let body = render_sandbox_adapter_turn_on_response_json(&turn_on);
        self.sandbox_adapter_turn_ons.push(turn_on);
        Ok(DxrCapacityResult { body, events })
    }

    pub fn submit_external_sandbox_preflight_json(
        &mut self,
        body: &str,
    ) -> Result<DxrCapacityResult, String> {
        let request = parse_external_sandbox_preflight_request(body)?;
        self.next_external_preflight += 1;
        let driver_is_registered = sandbox_drivers()
            .iter()
            .any(|driver| driver.name == request.sandbox_driver);
        let adapter_turn_on_matches = self.sandbox_adapter_turn_ons.iter().any(|turn_on| {
            !turn_on.rejected
                && turn_on.receipt_id == request.adapter_turn_on_receipt_id
                && turn_on.status == request.adapter_turn_on_status
                && turn_on.sandbox_driver == request.sandbox_driver
                && turn_on.adapter_ready_for_preflight
        });
        let boundary_rejected = !driver_is_registered
            || !request.adapter_turn_on_observed
            || !adapter_turn_on_matches
            || request.adapter_execution_allowed
            || request.external_repo_checkout_allowed
            || !request.snapshot_restore_required
            || !request.filesystem_watch_required
            || !request.preview_service_receipt_required
            || !request.suspend_resume_required
            || request.network_allowed
            || request.secret_inheritance_allowed
            || request.filesystem_mutation_allowed
            || request.production_write_authority
            || request.ready_p95_ms > DEFAULT_EXTERNAL_SANDBOX_READY_P95_MS
            || request.requested_parallel_sandboxes > request.max_parallel_sandboxes;
        let (admission_decision, status, terminal_state) = if boundary_rejected {
            (
                "rejected_external_sandbox_boundary",
                "DXR_EXTERNAL_SANDBOX_PREFLIGHT_REJECTED_BOUNDARY",
                "DXR_EXTERNAL_SANDBOX_PREFLIGHT_RECORDED_AUTHORITY_BLOCKED",
            )
        } else {
            (
                "adapter_candidate_recorded_authority_blocked",
                "LIVE-LOCAL-DXR-EXTERNAL-SANDBOX-PREFLIGHT",
                "DXR_EXTERNAL_SANDBOX_PREFLIGHT_RECORDED_AUTHORITY_BLOCKED",
            )
        };
        let preflight = DxrExternalSandboxPreflight {
            sequence: self.next_external_preflight,
            preflight_id: format!(
                "dxr_external_sandbox_preflight_{:06}",
                self.next_external_preflight
            ),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            sandbox_driver: request.sandbox_driver,
            sandbox_provider: request.sandbox_provider,
            driver_is_registered,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            requested_parallel_sandboxes: request.requested_parallel_sandboxes,
            max_parallel_sandboxes: request.max_parallel_sandboxes,
            warm_pool_target: request.warm_pool_target,
            external_repo_ref: request.external_repo_ref,
            external_repo_url_hash: request.external_repo_url_hash,
            repo_checkout_mode: request.repo_checkout_mode,
            evidence_import_policy: request.evidence_import_policy,
            network_policy: request.network_policy,
            filesystem_policy: request.filesystem_policy,
            secret_policy: request.secret_policy,
            egress_policy: request.egress_policy,
            workspace_snapshot_policy: request.workspace_snapshot_policy,
            filesystem_watch_policy: request.filesystem_watch_policy,
            preview_service_policy: request.preview_service_policy,
            suspend_resume_policy: request.suspend_resume_policy,
            max_runtime_seconds: request.max_runtime_seconds,
            ready_p95_ms: request.ready_p95_ms,
            artifact_retention_hours: request.artifact_retention_hours,
            adapter_turn_on_receipt_id: request.adapter_turn_on_receipt_id,
            adapter_turn_on_status: request.adapter_turn_on_status,
            adapter_turn_on_observed: request.adapter_turn_on_observed,
            adapter_capability_count: request.adapter_capability_count,
            sandbox_authority_receipt_id: request.sandbox_authority_receipt_id,
            human_ratification_receipt_id: request.human_ratification_receipt_id,
            admission_decision: admission_decision.to_string(),
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            authority_rejected: boundary_rejected,
            snapshot_restore_required: request.snapshot_restore_required,
            filesystem_watch_required: request.filesystem_watch_required,
            preview_service_receipt_required: request.preview_service_receipt_required,
            suspend_resume_required: request.suspend_resume_required,
        };
        let events = external_sandbox_preflight_events(&preflight);
        let body = render_external_sandbox_preflight_response_json(&preflight);
        self.external_preflights.push(preflight);
        Ok(DxrCapacityResult { body, events })
    }

    pub fn submit_sandbox_authority_envelope_json(
        &mut self,
        body: &str,
    ) -> Result<DxrCapacityResult, String> {
        let request = parse_sandbox_authority_envelope_request(body)?;
        self.next_sandbox_authority_envelope += 1;
        let sandbox_admission_ready = self.admissions.iter().any(|admission| {
            admission.admission_id == request.sandbox_admission_id
                && admission.admission_decision == request.sandbox_admission_decision
                && admission.admitted_to_sandbox_pool
                && !admission.rejected
        });
        let adapter_turn_on_ready = self.sandbox_adapter_turn_ons.iter().any(|turn_on| {
            turn_on.receipt_id == request.adapter_turn_on_receipt_id
                && turn_on.status == request.adapter_turn_on_status
                && turn_on.adapter_ready_for_preflight
                && !turn_on.rejected
        });
        let external_preflight_ready = self.external_preflights.iter().any(|preflight| {
            preflight.preflight_id == request.external_sandbox_preflight_id
                && preflight.status == request.external_sandbox_preflight_status
                && preflight.adapter_turn_on_receipt_id == request.adapter_turn_on_receipt_id
                && !preflight.authority_rejected
        });
        let authority_requested = request.sandbox_process_start_requested
            || request.adapter_execution_requested
            || request.external_repo_checkout_requested
            || request.network_requested
            || request.secret_inheritance_requested
            || request.filesystem_mutation_requested
            || request.provider_call_requested
            || request.tool_execution_requested
            || request.worker_execution_requested
            || request.ci_claim_requested
            || request.deployment_requested
            || request.production_write_requested;
        let evidence_missing = !request.sandbox_admission_observed
            || !request.adapter_turn_on_observed
            || !request.external_sandbox_preflight_observed
            || !sandbox_admission_ready
            || !adapter_turn_on_ready
            || !external_preflight_ready
            || request.human_ratification_receipt_id.trim().is_empty()
            || request
                .human_ratification_receipt_id
                .starts_with("PENDING_")
            || request.tool_policy_receipt_id.trim().is_empty()
            || request.reviewer_separation_receipt_id.trim().is_empty()
            || request.dispatch_claim_id.trim().is_empty()
            || request.heartbeat_receipt_id.trim().is_empty()
            || request.durable_workflow_receipt_id.trim().is_empty();
        let rejected = authority_requested || evidence_missing;
        let (status, terminal_state, envelope_decision, rejection_reason) = if authority_requested {
            (
                "DXR_SANDBOX_AUTHORITY_ENVELOPE_REJECTED_BOUNDARY",
                "DXR_SANDBOX_AUTHORITY_ENVELOPE_RECORDED_EXECUTION_BLOCKED",
                "rejected_sandbox_authority_boundary",
                "sandbox_execution_or_authority_requested",
            )
        } else if evidence_missing {
            (
                "DXR_SANDBOX_AUTHORITY_ENVELOPE_REJECTED_MISSING_EVIDENCE",
                "DXR_SANDBOX_AUTHORITY_ENVELOPE_RECORDED_EXECUTION_BLOCKED",
                "rejected_sandbox_authority_missing_evidence",
                "sandbox_admission_adapter_preflight_human_policy_dispatch_or_reviewer_evidence_missing",
            )
        } else {
            (
                "LIVE-LOCAL-DXR-SANDBOX-AUTHORITY-ENVELOPE-FLOOR",
                "DXR_SANDBOX_AUTHORITY_ENVELOPE_RECORDED_EXECUTION_BLOCKED",
                "sandbox_authority_staged_execution_blocked",
                "none",
            )
        };
        let envelope = DxrSandboxAuthorityEnvelope {
            sequence: self.next_sandbox_authority_envelope,
            envelope_id: format!(
                "dxr_sandbox_authority_envelope_{:06}",
                self.next_sandbox_authority_envelope
            ),
            receipt_id: format!(
                "dxr_sandbox_authority_receipt_{:06}",
                self.next_sandbox_authority_envelope
            ),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            worker_run_id: request.worker_run_id,
            sandbox_admission_id: request.sandbox_admission_id,
            sandbox_admission_decision: request.sandbox_admission_decision,
            sandbox_admission_observed: request.sandbox_admission_observed,
            adapter_turn_on_receipt_id: request.adapter_turn_on_receipt_id,
            adapter_turn_on_status: request.adapter_turn_on_status,
            adapter_turn_on_observed: request.adapter_turn_on_observed,
            external_sandbox_preflight_id: request.external_sandbox_preflight_id,
            external_sandbox_preflight_status: request.external_sandbox_preflight_status,
            external_sandbox_preflight_observed: request.external_sandbox_preflight_observed,
            human_ratification_receipt_id: request.human_ratification_receipt_id,
            tool_policy_receipt_id: request.tool_policy_receipt_id,
            reviewer_separation_receipt_id: request.reviewer_separation_receipt_id,
            dispatch_claim_id: request.dispatch_claim_id,
            heartbeat_receipt_id: request.heartbeat_receipt_id,
            durable_workflow_receipt_id: request.durable_workflow_receipt_id,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            sandbox_authority_staged: !rejected,
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            envelope_decision: envelope_decision.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let events = sandbox_authority_envelope_events(&envelope);
        let body = render_sandbox_authority_envelope_response_json(&envelope);
        self.sandbox_authority_envelopes.push(envelope);
        Ok(DxrCapacityResult { body, events })
    }

    pub fn submit_execution_admission_json(
        &mut self,
        body: &str,
    ) -> Result<DxrCapacityResult, String> {
        let request = parse_execution_admission_request(body)?;
        self.next_execution_admission += 1;
        let sandbox_authority_ready = self.sandbox_authority_envelopes.iter().any(|envelope| {
            envelope.envelope_id == request.sandbox_authority_envelope_id
                && envelope.receipt_id == request.sandbox_authority_receipt_id
                && envelope.status == request.sandbox_authority_status
                && envelope.sandbox_authority_staged
                && !envelope.rejected
        });
        let security_rejected = request.network_allowed
            || request.secret_inheritance_allowed
            || request.filesystem_mutation_allowed
            || request.production_write_authority;
        let evidence_missing = !request.dispatch_recovery_plan_observed
            || request.dispatch_claim_id.trim().is_empty()
            || request.heartbeat_receipt_id.trim().is_empty()
            || request.durable_workflow_receipt_id.trim().is_empty()
            || request.live_worker_preflight_id.trim().is_empty()
            || request.sandbox_admission_id.trim().is_empty()
            || request.sandbox_authority_envelope_id.trim().is_empty()
            || request.sandbox_authority_receipt_id.trim().is_empty()
            || request.sandbox_authority_status.trim().is_empty()
            || !request.sandbox_authority_observed
            || !sandbox_authority_ready
            || request.external_sandbox_preflight_id.trim().is_empty()
            || request.tool_policy_receipt_id.trim().is_empty()
            || request.reviewer_separation_receipt_id.trim().is_empty();
        let queued_by_pressure = request.sandbox_admission_decision == "queued_backpressure"
            || request.global_active_forge_runs >= request.global_max_concurrent_forge_runs
            || request.tenant_active_forge_runs >= request.max_concurrent_forge_runs_per_tenant
            || request.queued_forge_runs >= request.queue_depth_limit;
        let sandbox_pool_ready =
            request.sandbox_admission_decision == "admitted_to_sandbox_pool_without_start";
        let worker_preflight_ready = request.live_worker_preflight_status
            == "DXR_LIVE_WORKER_EXECUTION_PREFLIGHT_RECORDED_AUTHORITY_ENVELOPE_STAGED";
        let (
            admission_decision,
            terminal_state,
            admitted_to_execution_preflight,
            admitted_to_queue,
            rejected,
            rejection_reason,
        ) = if security_rejected {
            (
                "rejected_security_boundary",
                "DXR_EXECUTION_ADMISSION_REJECTED_SECURITY_BOUNDARY",
                false,
                false,
                true,
                "network_secret_filesystem_or_production_write_authority_requested",
            )
        } else if evidence_missing {
            (
                "rejected_missing_authority_evidence",
                "DXR_EXECUTION_ADMISSION_REJECTED_MISSING_EVIDENCE",
                false,
                false,
                true,
                "dispatch_worker_sandbox_tool_or_reviewer_evidence_missing",
            )
        } else if queued_by_pressure {
            (
                "queued_backpressure",
                "DXR_EXECUTION_ADMISSION_QUEUED_BACKPRESSURE",
                false,
                true,
                false,
                "tenant_global_queue_or_sandbox_pressure",
            )
        } else if sandbox_pool_ready && worker_preflight_ready {
            (
                "staged_execution_preflight_authority_blocked",
                "DXR_EXECUTION_ADMISSION_STAGED_EXECUTION_BLOCKED",
                true,
                false,
                false,
                "none",
            )
        } else {
            (
                "rejected_incomplete_execution_envelope",
                "DXR_EXECUTION_ADMISSION_REJECTED_INCOMPLETE_ENVELOPE",
                false,
                false,
                true,
                "sandbox_pool_or_live_worker_preflight_not_ready",
            )
        };
        let admission = DxrExecutionAdmission {
            sequence: self.next_execution_admission,
            admission_id: format!(
                "dxr_execution_admission_{:06}",
                self.next_execution_admission
            ),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            worker_run_id: request.worker_run_id,
            dispatch_recovery_plan_observed: request.dispatch_recovery_plan_observed,
            dispatch_claim_id: request.dispatch_claim_id,
            heartbeat_receipt_id: request.heartbeat_receipt_id,
            durable_workflow_receipt_id: request.durable_workflow_receipt_id,
            live_worker_preflight_id: request.live_worker_preflight_id,
            live_worker_preflight_status: request.live_worker_preflight_status,
            sandbox_admission_id: request.sandbox_admission_id,
            sandbox_admission_decision: request.sandbox_admission_decision,
            sandbox_authority_envelope_id: request.sandbox_authority_envelope_id,
            sandbox_authority_receipt_id: request.sandbox_authority_receipt_id,
            sandbox_authority_status: request.sandbox_authority_status,
            sandbox_authority_observed: request.sandbox_authority_observed,
            external_sandbox_preflight_id: request.external_sandbox_preflight_id,
            tool_policy_receipt_id: request.tool_policy_receipt_id,
            reviewer_separation_receipt_id: request.reviewer_separation_receipt_id,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            tenant_active_forge_runs: request.tenant_active_forge_runs,
            global_active_forge_runs: request.global_active_forge_runs,
            queued_forge_runs: request.queued_forge_runs,
            global_max_concurrent_forge_runs: request.global_max_concurrent_forge_runs,
            max_concurrent_forge_runs_per_tenant: request.max_concurrent_forge_runs_per_tenant,
            queue_depth_limit: request.queue_depth_limit,
            admission_decision: admission_decision.to_string(),
            terminal_state: terminal_state.to_string(),
            admitted_to_execution_preflight,
            admitted_to_queue,
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let events = execution_admission_events(&admission);
        let body = render_execution_admission_response_json(&admission);
        self.execution_admissions.push(admission);
        Ok(DxrCapacityResult { body, events })
    }

    pub fn plans_json(&self) -> String {
        let latest = self.plans.last();
        let overload_count = self.plans.iter().filter(|plan| plan.overloaded).count();
        format!(
            r#"{{"name":"mdx-dxr-capacity-sandbox","status":"LIVE-LOCAL-DXR-CAPACITY-SANDBOX-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/capacity.json","submit_route":"/v1/dxr/capacity-plans","plan_count":{},"overload_count":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"global_max_concurrent_forge_runs":{},"max_concurrent_forge_runs_per_tenant":{},"active_tenant_count":{},"max_concurrent_agents_per_workflow":{},"max_total_agents_per_workflow":{},"medium_build_p95_seconds":{},"complex_build_p95_seconds":{},"direct_agent_latency_ratio_bps":{},"queue_depth_limit":{},"hot_path_budget_ms":20,"performance_posture":"comparable_to_direct_agent_builds","fairness_policy":"tenant_weighted_fair_queue","backpressure_policy":"admit_until_budget_then_queue_and_reject_overflow","repo_isolation_modes":["current_repo_worktree","external_repo_checkout_planned","ephemeral_per_run_workspace"],"sandbox_driver_registry":[{}],"workflow_patterns":[{}],"latest_plan":{},"plans":[{}],"dynamic_workflow_runtime_status":"LIVE-LOCAL-DXR-DYNAMIC-WORKFLOW-PATTERN-FLOOR","sandbox_process_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
            self.plans.len(),
            overload_count,
            latest
                .map(|plan| plan.target_concurrent_engineers)
                .unwrap_or(DEFAULT_TARGET_ENGINEERS),
            latest
                .map(|plan| plan.peak_parallel_forge_builds)
                .unwrap_or(DEFAULT_PEAK_PARALLEL_FORGE_BUILDS),
            latest
                .map(|plan| plan.global_max_concurrent_forge_runs)
                .unwrap_or(DEFAULT_GLOBAL_FORGE_RUN_LIMIT),
            latest
                .map(|plan| plan.max_concurrent_forge_runs_per_tenant)
                .unwrap_or(DEFAULT_TENANT_FORGE_RUN_LIMIT),
            latest
                .map(|plan| plan.active_tenant_count)
                .unwrap_or(DEFAULT_ACTIVE_TENANT_COUNT),
            latest
                .map(|plan| plan.max_concurrent_agents_per_workflow)
                .unwrap_or(DEFAULT_AGENT_CONCURRENCY_PER_WORKFLOW),
            latest
                .map(|plan| plan.max_total_agents_per_workflow)
                .unwrap_or(DEFAULT_TOTAL_AGENTS_PER_WORKFLOW),
            latest
                .map(|plan| plan.medium_build_p95_seconds)
                .unwrap_or(DEFAULT_MEDIUM_BUILD_P95_SECONDS),
            latest
                .map(|plan| plan.complex_build_p95_seconds)
                .unwrap_or(DEFAULT_COMPLEX_BUILD_P95_SECONDS),
            latest
                .map(|plan| plan.direct_agent_latency_ratio_bps)
                .unwrap_or(DEFAULT_DIRECT_AGENT_LATENCY_RATIO_BPS),
            latest
                .map(|plan| plan.queue_depth_limit)
                .unwrap_or(DEFAULT_QUEUE_DEPTH_LIMIT),
            sandbox_drivers()
                .iter()
                .map(render_sandbox_driver_json)
                .collect::<Vec<_>>()
                .join(","),
            workflow_patterns()
                .iter()
                .map(render_workflow_pattern_json)
                .collect::<Vec<_>>()
                .join(","),
            latest
                .map(render_capacity_plan_json)
                .unwrap_or_else(|| "null".to_string()),
            self.plans
                .iter()
                .map(render_capacity_plan_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    pub fn sandbox_admissions_json(&self) -> String {
        let admitted_count = self
            .admissions
            .iter()
            .filter(|admission| admission.admitted_to_sandbox_pool)
            .count();
        let queued_count = self
            .admissions
            .iter()
            .filter(|admission| admission.admitted_to_queue)
            .count();
        let rejected_count = self
            .admissions
            .iter()
            .filter(|admission| admission.rejected)
            .count();
        format!(
            r#"{{"name":"mdx-dxr-sandbox-admissions","status":"LIVE-LOCAL-DXR-SANDBOX-ADMISSION-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/sandbox-admissions.json","submit_route":"/v1/dxr/sandbox-admissions","admission_count":{},"admitted_count":{},"queued_count":{},"rejected_count":{},"global_max_concurrent_forge_runs":{},"max_concurrent_forge_runs_per_tenant":{},"queue_depth_limit":{},"sandbox_pool_size":{},"fairness_policy":"tenant_weighted_fair_queue","backpressure_policy":"admit_until_budget_then_queue_and_reject_overflow","sandbox_process_started":false,"external_repo_checkout_started":false,"network_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false,"admissions":[{}]}}"#,
            self.admissions.len(),
            admitted_count,
            queued_count,
            rejected_count,
            self.admissions
                .last()
                .map(|admission| admission.global_max_concurrent_forge_runs)
                .unwrap_or(DEFAULT_GLOBAL_FORGE_RUN_LIMIT),
            self.admissions
                .last()
                .map(|admission| admission.max_concurrent_forge_runs_per_tenant)
                .unwrap_or(DEFAULT_TENANT_FORGE_RUN_LIMIT),
            self.admissions
                .last()
                .map(|admission| admission.queue_depth_limit)
                .unwrap_or(DEFAULT_QUEUE_DEPTH_LIMIT),
            self.admissions
                .last()
                .map(|admission| admission.sandbox_pool_size)
                .unwrap_or(DEFAULT_SANDBOX_POOL_SIZE),
            self.admissions
                .iter()
                .map(render_sandbox_admission_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    pub fn sandbox_adapter_turn_ons_json(&self) -> String {
        let accepted_count = self
            .sandbox_adapter_turn_ons
            .iter()
            .filter(|turn_on| !turn_on.rejected)
            .count();
        let rejected_count = self
            .sandbox_adapter_turn_ons
            .len()
            .saturating_sub(accepted_count);
        format!(
            r#"{{"name":"mdx-dxr-sandbox-adapter-turn-ons","status":"LIVE-LOCAL-DXR-SANDBOX-ADAPTER-TURN-ON-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/sandbox-adapter-turn-ons.json","submit_route":"/v1/dxr/sandbox-adapter-turn-ons","turn_on_count":{},"accepted_count":{},"rejected_count":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"max_parallel_sandboxes":{},"warm_pool_target":{},"ready_p95_ms_ceiling":{},"max_runtime_seconds":{},"artifact_retention_hours":{},"capability_floor_count":16,"sandbox_driver_registry":[{}],"turn_on_policy":"secret_safe_adapter_evidence_before_external_sandbox_preflight","execution_policy":"record_readiness_without_starting_sandbox_or_adapter","adapter_registry_observed_required":true,"isolation_boundary_observed_required":true,"warm_pool_observed_required":true,"snapshot_restore_observed_required":true,"filesystem_watch_observed_required":true,"preview_service_observed_required":true,"suspend_resume_observed_required":true,"log_stream_observed_required":true,"artifact_retention_observed_required":true,"network_policy_observed_required":true,"secret_policy_observed_required":true,"egress_policy_observed_required":true,"ready_p95_observed_required":true,"scale_capacity_observed_required":true,"human_ratification_gate_observed_required":true,"evidence_quarantine_observed_required":true,"external_preflight_requires_turn_on_receipt":true,"adapter_execution_allowed":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"network_allowed":false,"filesystem_mutation_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false,"turn_ons":[{}]}}"#,
            self.sandbox_adapter_turn_ons.len(),
            accepted_count,
            rejected_count,
            DEFAULT_TARGET_ENGINEERS,
            DEFAULT_PEAK_PARALLEL_FORGE_BUILDS,
            DEFAULT_GLOBAL_FORGE_RUN_LIMIT,
            DEFAULT_SANDBOX_POOL_SIZE,
            DEFAULT_EXTERNAL_SANDBOX_READY_P95_MS,
            DEFAULT_EXTERNAL_SANDBOX_MAX_RUNTIME_SECONDS,
            DEFAULT_EXTERNAL_SANDBOX_ARTIFACT_RETENTION_HOURS,
            sandbox_drivers()
                .iter()
                .map(render_sandbox_driver_json)
                .collect::<Vec<_>>()
                .join(","),
            self.sandbox_adapter_turn_ons
                .iter()
                .map(render_sandbox_adapter_turn_on_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    pub fn external_sandbox_preflights_json(&self) -> String {
        let accepted_count = self
            .external_preflights
            .iter()
            .filter(|preflight| !preflight.authority_rejected)
            .count();
        let rejected_count = self
            .external_preflights
            .len()
            .saturating_sub(accepted_count);
        format!(
            r#"{{"name":"mdx-dxr-external-sandbox-preflights","status":"LIVE-LOCAL-DXR-EXTERNAL-SANDBOX-PREFLIGHT","runtime":"mdx-dxr-engine","route":"/dxr/external-sandbox-preflights.json","submit_route":"/v1/dxr/external-sandbox-preflights","preflight_count":{},"accepted_count":{},"rejected_count":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"max_parallel_sandboxes":{},"warm_pool_target":{},"ready_p95_ms_ceiling":{},"max_runtime_seconds":{},"artifact_retention_hours":{},"sandbox_driver_registry":[{}],"snapshot_restore_required":true,"filesystem_watch_required":true,"preview_service_receipt_required":true,"suspend_resume_required":true,"adapter_turn_on_required":true,"adapter_turn_on_observed_required":true,"adapter_turn_on_receipt_must_match_recorded_turn_on":true,"sandbox_authority_receipt_required":true,"human_ratification_required":true,"external_repo_checkout_allowed":false,"adapter_execution_allowed":false,"sandbox_process_started":false,"network_allowed":false,"filesystem_mutation_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false,"preflights":[{}]}}"#,
            self.external_preflights.len(),
            accepted_count,
            rejected_count,
            DEFAULT_TARGET_ENGINEERS,
            DEFAULT_PEAK_PARALLEL_FORGE_BUILDS,
            DEFAULT_GLOBAL_FORGE_RUN_LIMIT,
            DEFAULT_SANDBOX_POOL_SIZE,
            DEFAULT_EXTERNAL_SANDBOX_READY_P95_MS,
            DEFAULT_EXTERNAL_SANDBOX_MAX_RUNTIME_SECONDS,
            DEFAULT_EXTERNAL_SANDBOX_ARTIFACT_RETENTION_HOURS,
            sandbox_drivers()
                .iter()
                .map(render_sandbox_driver_json)
                .collect::<Vec<_>>()
                .join(","),
            self.external_preflights
                .iter()
                .map(render_external_sandbox_preflight_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    pub fn sandbox_authority_envelopes_json(&self) -> String {
        let staged_count = self
            .sandbox_authority_envelopes
            .iter()
            .filter(|envelope| envelope.sandbox_authority_staged)
            .count();
        let rejected_count = self
            .sandbox_authority_envelopes
            .len()
            .saturating_sub(staged_count);
        format!(
            r#"{{"name":"mdx-dxr-sandbox-authority-envelopes","status":"LIVE-LOCAL-DXR-SANDBOX-AUTHORITY-ENVELOPE-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/sandbox-authority-envelopes.json","submit_route":"/v1/dxr/sandbox-authority-envelopes","envelope_count":{},"staged_count":{},"rejected_count":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"required_receipts":["sandbox.admission.recorded","dxr.sandbox_adapter.turn_on.recorded","external_sandbox.preflight.recorded","human.ratification.recorded","tool_policy.enforced","reviewer_separation.observed","dispatch.claim.recorded","dispatch.heartbeat.renewed","dxr.durable_workflow.recorded"],"authority_policy":"staged_sandbox_authority_without_starting_process","execution_admission_requires_sandbox_authority":true,"sandbox_process_started":false,"adapter_execution_allowed":false,"external_repo_checkout_started":false,"network_allowed":false,"filesystem_mutation_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"envelopes":[{}]}}"#,
            self.sandbox_authority_envelopes.len(),
            staged_count,
            rejected_count,
            DEFAULT_TARGET_ENGINEERS,
            DEFAULT_PEAK_PARALLEL_FORGE_BUILDS,
            self.sandbox_authority_envelopes
                .iter()
                .map(render_sandbox_authority_envelope_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    pub fn execution_admissions_json(&self) -> String {
        let staged_count = self
            .execution_admissions
            .iter()
            .filter(|admission| admission.admitted_to_execution_preflight)
            .count();
        let queued_count = self
            .execution_admissions
            .iter()
            .filter(|admission| admission.admitted_to_queue)
            .count();
        let rejected_count = self
            .execution_admissions
            .iter()
            .filter(|admission| admission.rejected)
            .count();
        format!(
            r#"{{"name":"mdx-dxr-execution-admissions","status":"LIVE-LOCAL-DXR-EXECUTION-ADMISSION-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/execution-admissions.json","submit_route":"/v1/dxr/execution-admissions","admission_count":{},"staged_count":{},"queued_count":{},"rejected_count":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"global_max_concurrent_forge_runs":{},"max_concurrent_forge_runs_per_tenant":{},"queue_depth_limit":{},"dispatch_recovery_plan_required":true,"sandbox_admission_required":true,"sandbox_authority_envelope_required":true,"external_sandbox_preflight_required":true,"live_worker_preflight_required":true,"claim_before_execute_required":true,"heartbeat_before_worker_execution_required":true,"tool_policy_required":true,"reviewer_separation_required":true,"worker_process_started":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"production_writes_allowed":false,"admissions":[{}]}}"#,
            self.execution_admissions.len(),
            staged_count,
            queued_count,
            rejected_count,
            self.execution_admissions
                .last()
                .map(|admission| admission.target_concurrent_engineers)
                .unwrap_or(DEFAULT_TARGET_ENGINEERS),
            self.execution_admissions
                .last()
                .map(|admission| admission.peak_parallel_forge_builds)
                .unwrap_or(DEFAULT_PEAK_PARALLEL_FORGE_BUILDS),
            self.execution_admissions
                .last()
                .map(|admission| admission.global_max_concurrent_forge_runs)
                .unwrap_or(DEFAULT_GLOBAL_FORGE_RUN_LIMIT),
            self.execution_admissions
                .last()
                .map(|admission| admission.max_concurrent_forge_runs_per_tenant)
                .unwrap_or(DEFAULT_TENANT_FORGE_RUN_LIMIT),
            self.execution_admissions
                .last()
                .map(|admission| admission.queue_depth_limit)
                .unwrap_or(DEFAULT_QUEUE_DEPTH_LIMIT),
            self.execution_admissions
                .iter()
                .map(render_execution_admission_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn parse_capacity_request(body: &str) -> Result<CapacityRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR capacity json: {error}"))?;
    Ok(CapacityRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
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
        requested_concurrent_forge_runs: usize_value(
            &value,
            "requested_concurrent_forge_runs",
            DEFAULT_REQUESTED_FORGE_RUNS,
        ),
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
        active_tenant_count: usize_value(
            &value,
            "active_tenant_count",
            DEFAULT_ACTIVE_TENANT_COUNT,
        ),
        queue_depth_limit: usize_value(&value, "queue_depth_limit", DEFAULT_QUEUE_DEPTH_LIMIT),
        requested_sandbox_count: usize_value(
            &value,
            "requested_sandbox_count",
            DEFAULT_REQUESTED_FORGE_RUNS,
        ),
        sandbox_pool_size: usize_value(&value, "sandbox_pool_size", DEFAULT_SANDBOX_POOL_SIZE),
        lease_duration_ms: u64_value(&value, "lease_duration_ms", 30_000),
        heartbeat_interval_ms: u64_value(&value, "heartbeat_interval_ms", 5_000),
        stale_run_recovery_ms: u64_value(&value, "stale_run_recovery_ms", 1_800_000),
        max_concurrent_agents_per_workflow: usize_value(
            &value,
            "max_concurrent_agents_per_workflow",
            DEFAULT_AGENT_CONCURRENCY_PER_WORKFLOW,
        ),
        max_total_agents_per_workflow: usize_value(
            &value,
            "max_total_agents_per_workflow",
            DEFAULT_TOTAL_AGENTS_PER_WORKFLOW,
        ),
        medium_build_p95_seconds: usize_value(
            &value,
            "medium_build_p95_seconds",
            DEFAULT_MEDIUM_BUILD_P95_SECONDS,
        ),
        complex_build_p95_seconds: usize_value(
            &value,
            "complex_build_p95_seconds",
            DEFAULT_COMPLEX_BUILD_P95_SECONDS,
        ),
        direct_agent_latency_ratio_bps: usize_value(
            &value,
            "direct_agent_latency_ratio_bps",
            DEFAULT_DIRECT_AGENT_LATENCY_RATIO_BPS,
        ),
        workflow_phase_count: usize_value(&value, "workflow_phase_count", 6),
        fairness_policy: string_value(&value, "fairness_policy", "tenant_weighted_fair_queue"),
        backpressure_policy: string_value(
            &value,
            "backpressure_policy",
            "admit_until_budget_then_queue_and_reject_overflow",
        ),
        repo_isolation_mode: string_value(&value, "repo_isolation_mode", "current_repo_worktree"),
        external_repo_mode: string_value(&value, "external_repo_mode", "planned_external_repo_ref"),
        sandbox_isolation_policy: string_value(
            &value,
            "sandbox_isolation_policy",
            "ephemeral_workspace_per_run_no_secret_inheritance_no_network_by_default",
        ),
    })
}

fn parse_sandbox_adapter_turn_on_request(
    body: &str,
) -> Result<SandboxAdapterTurnOnRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR sandbox adapter turn-on json: {error}"))?;
    Ok(SandboxAdapterTurnOnRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", "dxr_job_sandbox_adapter_turn_on_001"),
        run_id: string_value(&value, "run_id", "dxr_run_sandbox_adapter_turn_on_001"),
        sandbox_driver: string_value(&value, "sandbox_driver", "firecracker_microvm"),
        sandbox_provider: string_value(&value, "sandbox_provider", "local_firecracker_adapter"),
        adapter_version: string_value(&value, "adapter_version", "local-proof-adapter-v1"),
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
        max_parallel_sandboxes: usize_value(
            &value,
            "max_parallel_sandboxes",
            DEFAULT_GLOBAL_FORGE_RUN_LIMIT,
        ),
        warm_pool_target: usize_value(&value, "warm_pool_target", DEFAULT_SANDBOX_POOL_SIZE),
        ready_p95_ms: usize_value(
            &value,
            "ready_p95_ms",
            DEFAULT_EXTERNAL_SANDBOX_READY_P95_MS,
        ),
        ready_p95_ms_ceiling: usize_value(
            &value,
            "ready_p95_ms_ceiling",
            DEFAULT_EXTERNAL_SANDBOX_READY_P95_MS,
        ),
        max_runtime_seconds: usize_value(
            &value,
            "max_runtime_seconds",
            DEFAULT_EXTERNAL_SANDBOX_MAX_RUNTIME_SECONDS,
        ),
        artifact_retention_hours: usize_value(
            &value,
            "artifact_retention_hours",
            DEFAULT_EXTERNAL_SANDBOX_ARTIFACT_RETENTION_HOURS,
        ),
        adapter_registry_observed: bool_value(&value, "adapter_registry_observed", false),
        isolation_boundary_observed: bool_value(&value, "isolation_boundary_observed", false),
        warm_pool_observed: bool_value(&value, "warm_pool_observed", false),
        snapshot_restore_observed: bool_value(&value, "snapshot_restore_observed", false),
        filesystem_watch_observed: bool_value(&value, "filesystem_watch_observed", false),
        preview_service_observed: bool_value(&value, "preview_service_observed", false),
        suspend_resume_observed: bool_value(&value, "suspend_resume_observed", false),
        log_stream_observed: bool_value(&value, "log_stream_observed", false),
        artifact_retention_observed: bool_value(&value, "artifact_retention_observed", false),
        network_policy_observed: bool_value(&value, "network_policy_observed", false),
        secret_policy_observed: bool_value(&value, "secret_policy_observed", false),
        egress_policy_observed: bool_value(&value, "egress_policy_observed", false),
        ready_p95_observed: bool_value(&value, "ready_p95_observed", false),
        scale_capacity_observed: bool_value(&value, "scale_capacity_observed", false),
        human_ratification_gate_observed: bool_value(
            &value,
            "human_ratification_gate_observed",
            false,
        ),
        evidence_quarantine_observed: bool_value(&value, "evidence_quarantine_observed", false),
        adapter_execution_requested: bool_value(&value, "adapter_execution_requested", false),
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
        network_requested: bool_value(&value, "network_requested", false),
        secret_inheritance_requested: bool_value(&value, "secret_inheritance_requested", false),
        filesystem_mutation_requested: bool_value(&value, "filesystem_mutation_requested", false),
        provider_call_requested: bool_value(&value, "provider_call_requested", false),
        tool_execution_requested: bool_value(&value, "tool_execution_requested", false),
        worker_execution_requested: bool_value(&value, "worker_execution_requested", false),
        ci_claim_requested: bool_value(&value, "ci_claim_requested", false),
        deployment_requested: bool_value(&value, "deployment_requested", false),
        production_write_requested: bool_value(&value, "production_write_requested", false),
    })
}

fn parse_external_sandbox_preflight_request(
    body: &str,
) -> Result<ExternalSandboxPreflightRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR external sandbox preflight json: {error}"))?;
    let requested_parallel_sandboxes = usize_value(
        &value,
        "requested_parallel_sandboxes",
        DEFAULT_GLOBAL_FORGE_RUN_LIMIT,
    );
    let max_parallel_sandboxes = usize_value(
        &value,
        "max_parallel_sandboxes",
        DEFAULT_GLOBAL_FORGE_RUN_LIMIT,
    );
    if max_parallel_sandboxes == 0 {
        return Err(
            "DXR external sandbox preflight denied: max_parallel_sandboxes is required".to_string(),
        );
    }
    Ok(ExternalSandboxPreflightRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", "dxr_job_external_sandbox_001"),
        run_id: string_value(&value, "run_id", "dxr_run_external_sandbox_001"),
        sandbox_driver: string_value(&value, "sandbox_driver", "firecracker_microvm"),
        sandbox_provider: string_value(&value, "sandbox_provider", "local_or_cloud_adapter"),
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
        requested_parallel_sandboxes,
        max_parallel_sandboxes,
        warm_pool_target: usize_value(&value, "warm_pool_target", DEFAULT_SANDBOX_POOL_SIZE),
        external_repo_ref: string_value(
            &value,
            "external_repo_ref",
            "github.com/example/repo#main",
        ),
        external_repo_url_hash: string_value(
            &value,
            "external_repo_url_hash",
            "sha256:external_repo_url_hash_redacted",
        ),
        repo_checkout_mode: string_value(
            &value,
            "repo_checkout_mode",
            "quarantined_manifest_only_no_checkout",
        ),
        evidence_import_policy: string_value(
            &value,
            "evidence_import_policy",
            "quarantine_external_evidence_until_mdx_verdict",
        ),
        network_policy: string_value(
            &value,
            "network_policy",
            "deny_by_default_proxy_receipt_required",
        ),
        filesystem_policy: string_value(
            &value,
            "filesystem_policy",
            "ephemeral_workspace_readonly_rootfs_no_host_mount",
        ),
        secret_policy: string_value(
            &value,
            "secret_policy",
            "no_secret_inheritance_short_lived_brokered_tokens_only",
        ),
        egress_policy: string_value(
            &value,
            "egress_policy",
            "deny_all_until_allowlisted_proxy_receipt",
        ),
        workspace_snapshot_policy: string_value(
            &value,
            "workspace_snapshot_policy",
            "snapshot_restore_required_before_reuse",
        ),
        filesystem_watch_policy: string_value(
            &value,
            "filesystem_watch_policy",
            "watch_events_quarantined_until_receipted",
        ),
        preview_service_policy: string_value(
            &value,
            "preview_service_policy",
            "preview_urls_blocked_until_human_ratification",
        ),
        suspend_resume_policy: string_value(
            &value,
            "suspend_resume_policy",
            "pause_resume_requires_checkpoint_and_idempotency_key",
        ),
        max_runtime_seconds: usize_value(
            &value,
            "max_runtime_seconds",
            DEFAULT_EXTERNAL_SANDBOX_MAX_RUNTIME_SECONDS,
        ),
        ready_p95_ms: usize_value(
            &value,
            "ready_p95_ms",
            DEFAULT_EXTERNAL_SANDBOX_READY_P95_MS,
        ),
        artifact_retention_hours: usize_value(
            &value,
            "artifact_retention_hours",
            DEFAULT_EXTERNAL_SANDBOX_ARTIFACT_RETENTION_HOURS,
        ),
        adapter_turn_on_receipt_id: string_value(
            &value,
            "adapter_turn_on_receipt_id",
            "PENDING_ADAPTER_TURN_ON_RECEIPT",
        ),
        adapter_turn_on_status: string_value(
            &value,
            "adapter_turn_on_status",
            "PENDING_SANDBOX_ADAPTER_TURN_ON",
        ),
        adapter_turn_on_observed: bool_value(&value, "adapter_turn_on_observed", false),
        adapter_capability_count: usize_value(&value, "adapter_capability_count", 0),
        sandbox_authority_receipt_id: string_value(
            &value,
            "sandbox_authority_receipt_id",
            "PENDING_SANDBOX_AUTHORITY_RECEIPT",
        ),
        human_ratification_receipt_id: string_value(
            &value,
            "human_ratification_receipt_id",
            "PENDING_HUMAN_RATIFICATION_RECEIPT",
        ),
        adapter_execution_allowed: bool_value(&value, "adapter_execution_allowed", false),
        external_repo_checkout_allowed: bool_value(&value, "external_repo_checkout_allowed", false),
        snapshot_restore_required: bool_value(&value, "snapshot_restore_required", true),
        filesystem_watch_required: bool_value(&value, "filesystem_watch_required", true),
        preview_service_receipt_required: bool_value(
            &value,
            "preview_service_receipt_required",
            true,
        ),
        suspend_resume_required: bool_value(&value, "suspend_resume_required", true),
        network_allowed: bool_value(&value, "network_allowed", false),
        secret_inheritance_allowed: bool_value(&value, "secret_inheritance_allowed", false),
        filesystem_mutation_allowed: bool_value(&value, "filesystem_mutation_allowed", false),
        production_write_authority: bool_value(&value, "production_write_authority", false),
    })
}

fn parse_sandbox_authority_envelope_request(
    body: &str,
) -> Result<SandboxAuthorityEnvelopeRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR sandbox authority envelope json: {error}"))?;
    Ok(SandboxAuthorityEnvelopeRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", "dxr_job_sandbox_authority_001"),
        run_id: string_value(&value, "run_id", "dxr_run_sandbox_authority_001"),
        worker_run_id: string_value(&value, "worker_run_id", "dxr_worker_run_local_001"),
        sandbox_admission_id: string_value(&value, "sandbox_admission_id", ""),
        sandbox_admission_decision: string_value(
            &value,
            "sandbox_admission_decision",
            "admitted_to_sandbox_pool_without_start",
        ),
        sandbox_admission_observed: bool_value(&value, "sandbox_admission_observed", false),
        adapter_turn_on_receipt_id: string_value(&value, "adapter_turn_on_receipt_id", ""),
        adapter_turn_on_status: string_value(&value, "adapter_turn_on_status", ""),
        adapter_turn_on_observed: bool_value(&value, "adapter_turn_on_observed", false),
        external_sandbox_preflight_id: string_value(&value, "external_sandbox_preflight_id", ""),
        external_sandbox_preflight_status: string_value(
            &value,
            "external_sandbox_preflight_status",
            "",
        ),
        external_sandbox_preflight_observed: bool_value(
            &value,
            "external_sandbox_preflight_observed",
            false,
        ),
        human_ratification_receipt_id: string_value(&value, "human_ratification_receipt_id", ""),
        tool_policy_receipt_id: string_value(&value, "tool_policy_receipt_id", ""),
        reviewer_separation_receipt_id: string_value(&value, "reviewer_separation_receipt_id", ""),
        dispatch_claim_id: string_value(&value, "dispatch_claim_id", ""),
        heartbeat_receipt_id: string_value(&value, "heartbeat_receipt_id", ""),
        durable_workflow_receipt_id: string_value(&value, "durable_workflow_receipt_id", ""),
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
        sandbox_process_start_requested: bool_value(
            &value,
            "sandbox_process_start_requested",
            false,
        ),
        adapter_execution_requested: bool_value(&value, "adapter_execution_requested", false),
        external_repo_checkout_requested: bool_value(
            &value,
            "external_repo_checkout_requested",
            false,
        ),
        network_requested: bool_value(&value, "network_requested", false),
        secret_inheritance_requested: bool_value(&value, "secret_inheritance_requested", false),
        filesystem_mutation_requested: bool_value(&value, "filesystem_mutation_requested", false),
        provider_call_requested: bool_value(&value, "provider_call_requested", false),
        tool_execution_requested: bool_value(&value, "tool_execution_requested", false),
        worker_execution_requested: bool_value(&value, "worker_execution_requested", false),
        ci_claim_requested: bool_value(&value, "ci_claim_requested", false),
        deployment_requested: bool_value(&value, "deployment_requested", false),
        production_write_requested: bool_value(&value, "production_write_requested", false),
    })
}

fn parse_execution_admission_request(body: &str) -> Result<ExecutionAdmissionRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR execution admission json: {error}"))?;
    Ok(ExecutionAdmissionRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", "dxr_job_execution_admission_001"),
        run_id: string_value(&value, "run_id", "dxr_run_execution_admission_001"),
        worker_run_id: string_value(&value, "worker_run_id", "dxr_worker_run_local_001"),
        dispatch_recovery_plan_observed: bool_value(
            &value,
            "dispatch_recovery_plan_observed",
            false,
        ),
        dispatch_claim_id: string_value(&value, "dispatch_claim_id", ""),
        heartbeat_receipt_id: string_value(&value, "heartbeat_receipt_id", ""),
        durable_workflow_receipt_id: string_value(&value, "durable_workflow_receipt_id", ""),
        live_worker_preflight_id: string_value(&value, "live_worker_preflight_id", ""),
        live_worker_preflight_status: string_value(
            &value,
            "live_worker_preflight_status",
            "DXR_LIVE_WORKER_EXECUTION_PREFLIGHT_RECORDED_AUTHORITY_ENVELOPE_BLOCKED",
        ),
        sandbox_admission_id: string_value(&value, "sandbox_admission_id", ""),
        sandbox_admission_decision: string_value(
            &value,
            "sandbox_admission_decision",
            "queued_backpressure",
        ),
        sandbox_authority_envelope_id: string_value(&value, "sandbox_authority_envelope_id", ""),
        sandbox_authority_receipt_id: string_value(&value, "sandbox_authority_receipt_id", ""),
        sandbox_authority_status: string_value(&value, "sandbox_authority_status", ""),
        sandbox_authority_observed: bool_value(&value, "sandbox_authority_observed", false),
        external_sandbox_preflight_id: string_value(&value, "external_sandbox_preflight_id", ""),
        tool_policy_receipt_id: string_value(&value, "tool_policy_receipt_id", ""),
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
        tenant_active_forge_runs: usize_value(&value, "tenant_active_forge_runs", 12),
        global_active_forge_runs: usize_value(&value, "global_active_forge_runs", 900),
        queued_forge_runs: usize_value(&value, "queued_forge_runs", 120),
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
        queue_depth_limit: usize_value(&value, "queue_depth_limit", DEFAULT_QUEUE_DEPTH_LIMIT),
        network_allowed: bool_value(&value, "network_allowed", false),
        secret_inheritance_allowed: bool_value(&value, "secret_inheritance_allowed", false),
        filesystem_mutation_allowed: bool_value(&value, "filesystem_mutation_allowed", false),
        production_write_authority: bool_value(&value, "production_write_authority", false),
    })
}

fn sandbox_adapter_observed_capability_count(request: &SandboxAdapterTurnOnRequest) -> usize {
    [
        request.adapter_registry_observed,
        request.isolation_boundary_observed,
        request.warm_pool_observed,
        request.snapshot_restore_observed,
        request.filesystem_watch_observed,
        request.preview_service_observed,
        request.suspend_resume_observed,
        request.log_stream_observed,
        request.artifact_retention_observed,
        request.network_policy_observed,
        request.secret_policy_observed,
        request.egress_policy_observed,
        request.ready_p95_observed,
        request.scale_capacity_observed,
        request.human_ratification_gate_observed,
        request.evidence_quarantine_observed,
    ]
    .iter()
    .filter(|observed| **observed)
    .count()
}

fn capacity_events(plan: &DxrCapacityPlan) -> Vec<DxrCapacityRuntimeEvent> {
    let mut events = vec![
        capacity_event(plan, "capacity_plan_recorded"),
        capacity_event(plan, "sandbox_admission_evaluated"),
        capacity_event(plan, "dynamic_workflow_pattern_library_bound"),
        capacity_event(plan, "workflow_reviewer_separation_required"),
        capacity_event(plan, "untrusted_input_quarantined"),
    ];
    if plan.overloaded {
        events.push(capacity_event(plan, "sandbox_pool_backpressure_applied"));
        events.push(capacity_event(plan, "capacity_overload_rejected"));
    }
    events
}

fn capacity_event(plan: &DxrCapacityPlan, event_type: &str) -> DxrCapacityRuntimeEvent {
    DxrCapacityRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: plan.tenant_id.clone(),
        job_id: plan.capacity_plan_id.clone(),
        run_id: plan.capacity_plan_id.clone(),
        actor_id: plan.actor_id.clone(),
    }
}

fn sandbox_admission_events(admission: &DxrSandboxAdmission) -> Vec<DxrCapacityRuntimeEvent> {
    let mut events = vec![sandbox_admission_event(
        admission,
        "sandbox_admission_recorded",
    )];
    if admission.admitted_to_sandbox_pool {
        events.push(sandbox_admission_event(
            admission,
            "sandbox_pool_slot_reserved",
        ));
    }
    if admission.admitted_to_queue {
        events.push(sandbox_admission_event(
            admission,
            "sandbox_admission_queued_backpressure",
        ));
    }
    if admission.rejected {
        events.push(sandbox_admission_event(
            admission,
            "sandbox_admission_rejected",
        ));
    }
    events.push(sandbox_admission_event(
        admission,
        "sandbox_authority_remained_blocked",
    ));
    events
}

fn sandbox_adapter_turn_on_events(
    turn_on: &DxrSandboxAdapterTurnOn,
) -> Vec<DxrCapacityRuntimeEvent> {
    let mut events = vec![
        sandbox_adapter_turn_on_event(turn_on, "sandbox_adapter_turn_on_recorded"),
        sandbox_adapter_turn_on_event(turn_on, "sandbox_adapter_driver_registered"),
        sandbox_adapter_turn_on_event(turn_on, "sandbox_adapter_isolation_observed"),
        sandbox_adapter_turn_on_event(turn_on, "sandbox_adapter_warm_pool_observed"),
        sandbox_adapter_turn_on_event(turn_on, "sandbox_adapter_snapshot_restore_observed"),
        sandbox_adapter_turn_on_event(turn_on, "sandbox_adapter_filesystem_watch_observed"),
        sandbox_adapter_turn_on_event(turn_on, "sandbox_adapter_preview_service_observed"),
        sandbox_adapter_turn_on_event(turn_on, "sandbox_adapter_suspend_resume_observed"),
        sandbox_adapter_turn_on_event(turn_on, "sandbox_adapter_log_stream_observed"),
        sandbox_adapter_turn_on_event(turn_on, "sandbox_adapter_network_policy_blocked"),
        sandbox_adapter_turn_on_event(turn_on, "sandbox_adapter_secret_policy_blocked"),
        sandbox_adapter_turn_on_event(turn_on, "sandbox_adapter_execution_authority_blocked"),
    ];
    if turn_on.rejected {
        events.push(sandbox_adapter_turn_on_event(
            turn_on,
            "sandbox_adapter_turn_on_rejected",
        ));
    } else {
        events.push(sandbox_adapter_turn_on_event(
            turn_on,
            "sandbox_adapter_external_preflight_ready",
        ));
    }
    events
}

fn external_sandbox_preflight_events(
    preflight: &DxrExternalSandboxPreflight,
) -> Vec<DxrCapacityRuntimeEvent> {
    let mut events = vec![
        external_sandbox_preflight_event(preflight, "external_sandbox_preflight_recorded"),
        external_sandbox_preflight_event(preflight, "external_sandbox_driver_selected"),
        external_sandbox_preflight_event(preflight, "external_repo_checkout_quarantined"),
        external_sandbox_preflight_event(preflight, "sandbox_credentials_not_read"),
        external_sandbox_preflight_event(preflight, "sandbox_network_policy_blocked"),
        external_sandbox_preflight_event(preflight, "sandbox_execution_authority_blocked"),
        external_sandbox_preflight_event(preflight, "sandbox_adapter_turn_on_required"),
        external_sandbox_preflight_event(preflight, "sandbox_adapter_turn_on_observed"),
        external_sandbox_preflight_event(preflight, "sandbox_snapshot_restore_required"),
        external_sandbox_preflight_event(preflight, "sandbox_filesystem_watch_bound"),
        external_sandbox_preflight_event(preflight, "sandbox_preview_service_blocked"),
    ];
    if preflight.authority_rejected {
        events.push(external_sandbox_preflight_event(
            preflight,
            "external_sandbox_preflight_rejected",
        ));
    }
    events
}

fn sandbox_authority_envelope_events(
    envelope: &DxrSandboxAuthorityEnvelope,
) -> Vec<DxrCapacityRuntimeEvent> {
    let mut events = vec![
        sandbox_authority_envelope_event(envelope, "sandbox_authority_envelope_recorded"),
        sandbox_authority_envelope_event(envelope, "sandbox_authority_admission_observed"),
        sandbox_authority_envelope_event(envelope, "sandbox_authority_adapter_turn_on_observed"),
        sandbox_authority_envelope_event(envelope, "sandbox_authority_external_preflight_observed"),
        sandbox_authority_envelope_event(envelope, "sandbox_authority_human_ratification_observed"),
        sandbox_authority_envelope_event(envelope, "sandbox_authority_tool_policy_observed"),
        sandbox_authority_envelope_event(
            envelope,
            "sandbox_authority_reviewer_separation_observed",
        ),
        sandbox_authority_envelope_event(envelope, "sandbox_authority_execution_blocked"),
    ];
    if envelope.sandbox_authority_staged {
        events.push(sandbox_authority_envelope_event(
            envelope,
            "sandbox_authority_envelope_staged",
        ));
    }
    if envelope.rejected {
        events.push(sandbox_authority_envelope_event(
            envelope,
            "sandbox_authority_envelope_rejected",
        ));
    }
    events
}

fn execution_admission_events(admission: &DxrExecutionAdmission) -> Vec<DxrCapacityRuntimeEvent> {
    let mut events = vec![execution_admission_event(
        admission,
        "execution_admission_recorded",
    )];
    if admission.admitted_to_execution_preflight {
        events.push(execution_admission_event(
            admission,
            "execution_admission_staged_authority_blocked",
        ));
    }
    if admission.admitted_to_queue {
        events.push(execution_admission_event(
            admission,
            "execution_admission_queued_backpressure",
        ));
    }
    if admission.rejected {
        events.push(execution_admission_event(
            admission,
            "execution_admission_rejected",
        ));
    }
    events.push(execution_admission_event(
        admission,
        "execution_authority_remained_blocked",
    ));
    events
}

fn execution_admission_event(
    admission: &DxrExecutionAdmission,
    event_type: &str,
) -> DxrCapacityRuntimeEvent {
    DxrCapacityRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: admission.tenant_id.clone(),
        job_id: admission.job_id.clone(),
        run_id: admission.run_id.clone(),
        actor_id: admission.actor_id.clone(),
    }
}

fn external_sandbox_preflight_event(
    preflight: &DxrExternalSandboxPreflight,
    event_type: &str,
) -> DxrCapacityRuntimeEvent {
    DxrCapacityRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: preflight.tenant_id.clone(),
        job_id: preflight.job_id.clone(),
        run_id: preflight.run_id.clone(),
        actor_id: preflight.actor_id.clone(),
    }
}

fn sandbox_authority_envelope_event(
    envelope: &DxrSandboxAuthorityEnvelope,
    event_type: &str,
) -> DxrCapacityRuntimeEvent {
    DxrCapacityRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: envelope.tenant_id.clone(),
        job_id: envelope.job_id.clone(),
        run_id: envelope.run_id.clone(),
        actor_id: envelope.actor_id.clone(),
    }
}

fn sandbox_adapter_turn_on_event(
    turn_on: &DxrSandboxAdapterTurnOn,
    event_type: &str,
) -> DxrCapacityRuntimeEvent {
    DxrCapacityRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: turn_on.tenant_id.clone(),
        job_id: turn_on.job_id.clone(),
        run_id: turn_on.run_id.clone(),
        actor_id: turn_on.actor_id.clone(),
    }
}

fn sandbox_admission_event(
    admission: &DxrSandboxAdmission,
    event_type: &str,
) -> DxrCapacityRuntimeEvent {
    DxrCapacityRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: admission.tenant_id.clone(),
        job_id: admission.job_id.clone(),
        run_id: admission.run_id.clone(),
        actor_id: admission.actor_id.clone(),
    }
}

fn sandbox_drivers() -> Vec<SandboxDriver> {
    vec![
        SandboxDriver {
            name: "local_docker",
            isolation: "container_per_run_no_network_readonly_rootfs_secret_strip",
            status: "LIVE-LOCAL-SANDBOX-DRIVER-REFERENCE",
            execution_surface: "local_container",
        },
        SandboxDriver {
            name: "firecracker_microvm",
            isolation: "microvm_per_run_jailer_cgroup_netns_planned",
            status: "PENDING-PRODUCTION-SANDBOX-DRIVER",
            execution_surface: "microvm",
        },
        SandboxDriver {
            name: "cloudflare_sandbox",
            isolation: "remote_isolated_container_full_linux_planned",
            status: "PENDING-PRODUCTION-SANDBOX-DRIVER",
            execution_surface: "remote_container",
        },
        SandboxDriver {
            name: "modal_sandbox",
            isolation: "remote_secure_container_untrusted_code_planned",
            status: "PENDING-PRODUCTION-SANDBOX-DRIVER",
            execution_surface: "remote_container",
        },
        SandboxDriver {
            name: "e2b_sandbox",
            isolation: "remote_secure_linux_vm_agent_code_planned",
            status: "PENDING-PRODUCTION-SANDBOX-DRIVER",
            execution_surface: "remote_vm",
        },
        SandboxDriver {
            name: "codex_cloud_sandbox",
            isolation: "isolated_cloud_task_reference",
            status: "REFERENCE-PATTERN-NOT-RUNTIME-AUTHORITY",
            execution_surface: "managed_agent_sandbox_reference",
        },
    ]
}

fn workflow_patterns() -> Vec<WorkflowPattern> {
    vec![
        WorkflowPattern {
            name: "classify_and_act",
            purpose: "route simple tasks to lean harnesses and complex tasks to dynamic workflows",
            reviewer_context: "fresh_context_required_for_escalated_path",
        },
        WorkflowPattern {
            name: "fan_out_and_synthesize",
            purpose: "parallelize codebase sweeps, migration slices, and evidence gathering",
            reviewer_context: "synthesizer_cannot_self_accept",
        },
        WorkflowPattern {
            name: "adversarial_verification",
            purpose: "separate builder context from verifier context before merge readiness",
            reviewer_context: "independent_reviewer_required",
        },
        WorkflowPattern {
            name: "loop_until_done",
            purpose: "rerun deterministic gates until hard goal criteria pass or budget closes",
            reviewer_context: "goal_contract_required",
        },
        WorkflowPattern {
            name: "generate_and_filter",
            purpose: "rank alternatives with a verifier and retain only evidence-backed outputs",
            reviewer_context: "ranking_context_separate_from_generator",
        },
        WorkflowPattern {
            name: "quarantine_untrusted_input",
            purpose: "keep external tickets, code, logs, and documents away from privileged agents",
            reviewer_context: "privileged_agent_sees_sanitized_summary_only",
        },
    ]
}

fn render_capacity_plan_response_json(plan: &DxrCapacityPlan) -> String {
    format!(
        r#"{{"name":"mdx-dxr-capacity-sandbox-plan","status":"LIVE-LOCAL-DXR-CAPACITY-SANDBOX-FLOOR","runtime":"mdx-dxr-engine","capacity_plan":{},"capacity_plan_id":{},"tenant_id":{},"actor_id":{},"terminal_state":{},"admission_decision":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"requested_concurrent_forge_runs":{},"accepted_forge_runs":{},"queued_forge_runs":{},"global_max_concurrent_forge_runs":{},"max_concurrent_forge_runs_per_tenant":{},"active_tenant_count":{},"requested_sandbox_count":{},"sandbox_pool_size":{},"queue_depth_limit":{},"lease_duration_ms":{},"heartbeat_interval_ms":{},"stale_run_recovery_ms":{},"max_concurrent_agents_per_workflow":{},"max_total_agents_per_workflow":{},"medium_build_p95_seconds":{},"complex_build_p95_seconds":{},"direct_agent_latency_ratio_bps":{},"performance_posture":"comparable_to_direct_agent_builds","workflow_pattern_count":{},"workflow_phase_count":{},"hot_path_budget_ms":20,"overloaded":{},"sandbox_process_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        render_capacity_plan_json(plan),
        json_string_literal(&plan.capacity_plan_id),
        json_string_literal(&plan.tenant_id),
        json_string_literal(&plan.actor_id),
        json_string_literal(&plan.terminal_state),
        json_string_literal(&plan.admission_decision),
        plan.target_concurrent_engineers,
        plan.peak_parallel_forge_builds,
        plan.requested_concurrent_forge_runs,
        plan.accepted_forge_runs,
        plan.queued_forge_runs,
        plan.global_max_concurrent_forge_runs,
        plan.max_concurrent_forge_runs_per_tenant,
        plan.active_tenant_count,
        plan.requested_sandbox_count,
        plan.sandbox_pool_size,
        plan.queue_depth_limit,
        plan.lease_duration_ms,
        plan.heartbeat_interval_ms,
        plan.stale_run_recovery_ms,
        plan.max_concurrent_agents_per_workflow,
        plan.max_total_agents_per_workflow,
        plan.medium_build_p95_seconds,
        plan.complex_build_p95_seconds,
        plan.direct_agent_latency_ratio_bps,
        plan.workflow_pattern_count,
        plan.workflow_phase_count,
        plan.overloaded
    )
}

fn render_capacity_plan_json(plan: &DxrCapacityPlan) -> String {
    format!(
        r#"{{"sequence":{},"capacity_plan_id":{},"tenant_id":{},"actor_id":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"requested_concurrent_forge_runs":{},"accepted_forge_runs":{},"queued_forge_runs":{},"global_max_concurrent_forge_runs":{},"max_concurrent_forge_runs_per_tenant":{},"active_tenant_count":{},"queue_depth_limit":{},"requested_sandbox_count":{},"sandbox_pool_size":{},"lease_duration_ms":{},"heartbeat_interval_ms":{},"stale_run_recovery_ms":{},"max_concurrent_agents_per_workflow":{},"max_total_agents_per_workflow":{},"medium_build_p95_seconds":{},"complex_build_p95_seconds":{},"direct_agent_latency_ratio_bps":{},"performance_posture":"comparable_to_direct_agent_builds","workflow_phase_count":{},"workflow_pattern_count":{},"fairness_policy":{},"backpressure_policy":{},"repo_isolation_mode":{},"external_repo_mode":{},"sandbox_isolation_policy":{},"sandbox_driver_registry":[{}],"workflow_patterns":[{}],"admission_decision":{},"terminal_state":{},"overloaded":{},"sandbox_process_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        plan.sequence,
        json_string_literal(&plan.capacity_plan_id),
        json_string_literal(&plan.tenant_id),
        json_string_literal(&plan.actor_id),
        plan.target_concurrent_engineers,
        plan.peak_parallel_forge_builds,
        plan.requested_concurrent_forge_runs,
        plan.accepted_forge_runs,
        plan.queued_forge_runs,
        plan.global_max_concurrent_forge_runs,
        plan.max_concurrent_forge_runs_per_tenant,
        plan.active_tenant_count,
        plan.queue_depth_limit,
        plan.requested_sandbox_count,
        plan.sandbox_pool_size,
        plan.lease_duration_ms,
        plan.heartbeat_interval_ms,
        plan.stale_run_recovery_ms,
        plan.max_concurrent_agents_per_workflow,
        plan.max_total_agents_per_workflow,
        plan.medium_build_p95_seconds,
        plan.complex_build_p95_seconds,
        plan.direct_agent_latency_ratio_bps,
        plan.workflow_phase_count,
        plan.workflow_pattern_count,
        json_string_literal(&plan.fairness_policy),
        json_string_literal(&plan.backpressure_policy),
        json_string_literal(&plan.repo_isolation_mode),
        json_string_literal(&plan.external_repo_mode),
        json_string_literal(&plan.sandbox_isolation_policy),
        plan.sandbox_driver_registry
            .iter()
            .map(render_sandbox_driver_json)
            .collect::<Vec<_>>()
            .join(","),
        plan.workflow_patterns
            .iter()
            .map(render_workflow_pattern_json)
            .collect::<Vec<_>>()
            .join(","),
        json_string_literal(&plan.admission_decision),
        json_string_literal(&plan.terminal_state),
        plan.overloaded
    )
}

fn render_sandbox_admission_response_json(admission: &DxrSandboxAdmission) -> String {
    format!(
        r#"{{"name":"mdx-dxr-sandbox-admission","status":"LIVE-LOCAL-DXR-SANDBOX-ADMISSION-FLOOR","runtime":"mdx-dxr-engine","admission":{},"admission_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"terminal_state":{},"admission_decision":{},"admitted_to_queue":{},"admitted_to_sandbox_pool":{},"rejected":{},"rejection_reason":{},"global_max_concurrent_forge_runs":{},"max_concurrent_forge_runs_per_tenant":{},"queue_depth_limit":{},"sandbox_pool_size":{},"sandbox_process_started":false,"external_repo_checkout_started":false,"network_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        render_sandbox_admission_json(admission),
        json_string_literal(&admission.admission_id),
        json_string_literal(&admission.tenant_id),
        json_string_literal(&admission.actor_id),
        json_string_literal(&admission.job_id),
        json_string_literal(&admission.run_id),
        json_string_literal(&admission.terminal_state),
        json_string_literal(&admission.admission_decision),
        admission.admitted_to_queue,
        admission.admitted_to_sandbox_pool,
        admission.rejected,
        json_string_literal(&admission.rejection_reason),
        admission.global_max_concurrent_forge_runs,
        admission.max_concurrent_forge_runs_per_tenant,
        admission.queue_depth_limit,
        admission.sandbox_pool_size
    )
}

fn render_sandbox_admission_json(admission: &DxrSandboxAdmission) -> String {
    format!(
        r#"{{"sequence":{},"admission_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"requested_sandbox_count":{},"active_sandbox_count":{},"sandbox_pool_size":{},"tenant_active_forge_runs":{},"global_active_forge_runs":{},"queued_forge_runs":{},"global_max_concurrent_forge_runs":{},"max_concurrent_forge_runs_per_tenant":{},"queue_depth_limit":{},"repo_ref":{},"repo_isolation_mode":{},"sandbox_driver":{},"sandbox_isolation_policy":{},"admission_decision":{},"terminal_state":{},"admitted_to_queue":{},"admitted_to_sandbox_pool":{},"rejected":{},"rejection_reason":{},"sandbox_process_started":false,"external_repo_checkout_started":false,"network_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        admission.sequence,
        json_string_literal(&admission.admission_id),
        json_string_literal(&admission.tenant_id),
        json_string_literal(&admission.actor_id),
        json_string_literal(&admission.job_id),
        json_string_literal(&admission.run_id),
        admission.requested_sandbox_count,
        admission.active_sandbox_count,
        admission.sandbox_pool_size,
        admission.tenant_active_forge_runs,
        admission.global_active_forge_runs,
        admission.queued_forge_runs,
        admission.global_max_concurrent_forge_runs,
        admission.max_concurrent_forge_runs_per_tenant,
        admission.queue_depth_limit,
        json_string_literal(&admission.repo_ref),
        json_string_literal(&admission.repo_isolation_mode),
        json_string_literal(&admission.sandbox_driver),
        json_string_literal(&admission.sandbox_isolation_policy),
        json_string_literal(&admission.admission_decision),
        json_string_literal(&admission.terminal_state),
        admission.admitted_to_queue,
        admission.admitted_to_sandbox_pool,
        admission.rejected,
        json_string_literal(&admission.rejection_reason)
    )
}

fn render_sandbox_adapter_turn_on_response_json(turn_on: &DxrSandboxAdapterTurnOn) -> String {
    format!(
        r#"{{"name":"mdx-dxr-sandbox-adapter-turn-on","status":{},"runtime":"mdx-dxr-engine","turn_on":{},"turn_on_id":{},"receipt_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"terminal_state":{},"admission_decision":{},"sandbox_driver":{},"sandbox_provider":{},"adapter_version":{},"driver_is_registered":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"max_parallel_sandboxes":{},"warm_pool_target":{},"ready_p95_ms":{},"ready_p95_ms_ceiling":{},"max_runtime_seconds":{},"artifact_retention_hours":{},"capability_count":{},"capability_floor_count":16,"adapter_ready_for_preflight":{},"rejected":{},"rejection_reason":{},"adapter_execution_allowed":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"network_allowed":false,"filesystem_mutation_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&turn_on.status),
        render_sandbox_adapter_turn_on_json(turn_on),
        json_string_literal(&turn_on.turn_on_id),
        json_string_literal(&turn_on.receipt_id),
        json_string_literal(&turn_on.tenant_id),
        json_string_literal(&turn_on.actor_id),
        json_string_literal(&turn_on.job_id),
        json_string_literal(&turn_on.run_id),
        json_string_literal(&turn_on.terminal_state),
        json_string_literal(&turn_on.admission_decision),
        json_string_literal(&turn_on.sandbox_driver),
        json_string_literal(&turn_on.sandbox_provider),
        json_string_literal(&turn_on.adapter_version),
        turn_on.driver_is_registered,
        turn_on.target_concurrent_engineers,
        turn_on.peak_parallel_forge_builds,
        turn_on.max_parallel_sandboxes,
        turn_on.warm_pool_target,
        turn_on.ready_p95_ms,
        turn_on.ready_p95_ms_ceiling,
        turn_on.max_runtime_seconds,
        turn_on.artifact_retention_hours,
        turn_on.capability_count,
        turn_on.adapter_ready_for_preflight,
        turn_on.rejected,
        json_string_literal(&turn_on.rejection_reason)
    )
}

fn render_sandbox_adapter_turn_on_json(turn_on: &DxrSandboxAdapterTurnOn) -> String {
    format!(
        r#"{{"sequence":{},"turn_on_id":{},"receipt_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"sandbox_driver":{},"sandbox_provider":{},"adapter_version":{},"driver_is_registered":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"max_parallel_sandboxes":{},"warm_pool_target":{},"ready_p95_ms":{},"ready_p95_ms_ceiling":{},"max_runtime_seconds":{},"artifact_retention_hours":{},"capability_count":{},"capability_floor_count":16,"status":{},"terminal_state":{},"admission_decision":{},"rejected":{},"rejection_reason":{},"adapter_ready_for_preflight":{},"adapter_registry_observed":{},"isolation_boundary_observed":{},"warm_pool_observed":{},"snapshot_restore_observed":{},"filesystem_watch_observed":{},"preview_service_observed":{},"suspend_resume_observed":{},"log_stream_observed":{},"artifact_retention_observed":{},"network_policy_observed":{},"secret_policy_observed":{},"egress_policy_observed":{},"ready_p95_observed":{},"scale_capacity_observed":{},"human_ratification_gate_observed":{},"evidence_quarantine_observed":{},"adapter_execution_allowed":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"network_allowed":false,"filesystem_mutation_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        turn_on.sequence,
        json_string_literal(&turn_on.turn_on_id),
        json_string_literal(&turn_on.receipt_id),
        json_string_literal(&turn_on.tenant_id),
        json_string_literal(&turn_on.actor_id),
        json_string_literal(&turn_on.job_id),
        json_string_literal(&turn_on.run_id),
        json_string_literal(&turn_on.sandbox_driver),
        json_string_literal(&turn_on.sandbox_provider),
        json_string_literal(&turn_on.adapter_version),
        turn_on.driver_is_registered,
        turn_on.target_concurrent_engineers,
        turn_on.peak_parallel_forge_builds,
        turn_on.max_parallel_sandboxes,
        turn_on.warm_pool_target,
        turn_on.ready_p95_ms,
        turn_on.ready_p95_ms_ceiling,
        turn_on.max_runtime_seconds,
        turn_on.artifact_retention_hours,
        turn_on.capability_count,
        json_string_literal(&turn_on.status),
        json_string_literal(&turn_on.terminal_state),
        json_string_literal(&turn_on.admission_decision),
        turn_on.rejected,
        json_string_literal(&turn_on.rejection_reason),
        turn_on.adapter_ready_for_preflight,
        turn_on.adapter_registry_observed,
        turn_on.isolation_boundary_observed,
        turn_on.warm_pool_observed,
        turn_on.snapshot_restore_observed,
        turn_on.filesystem_watch_observed,
        turn_on.preview_service_observed,
        turn_on.suspend_resume_observed,
        turn_on.log_stream_observed,
        turn_on.artifact_retention_observed,
        turn_on.network_policy_observed,
        turn_on.secret_policy_observed,
        turn_on.egress_policy_observed,
        turn_on.ready_p95_observed,
        turn_on.scale_capacity_observed,
        turn_on.human_ratification_gate_observed,
        turn_on.evidence_quarantine_observed
    )
}

fn render_external_sandbox_preflight_response_json(
    preflight: &DxrExternalSandboxPreflight,
) -> String {
    format!(
        r#"{{"name":"mdx-dxr-external-sandbox-preflight","status":{},"runtime":"mdx-dxr-engine","preflight":{},"preflight_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"terminal_state":{},"admission_decision":{},"sandbox_driver":{},"sandbox_provider":{},"driver_is_registered":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"requested_parallel_sandboxes":{},"max_parallel_sandboxes":{},"warm_pool_target":{},"ready_p95_ms":{},"ready_p95_ms_ceiling":{},"max_runtime_seconds":{},"artifact_retention_hours":{},"adapter_turn_on_receipt_id":{},"adapter_turn_on_status":{},"adapter_turn_on_observed":{},"adapter_capability_count":{},"repo_checkout_mode":{},"evidence_import_policy":{},"workspace_snapshot_policy":{},"filesystem_watch_policy":{},"preview_service_policy":{},"suspend_resume_policy":{},"snapshot_restore_required":{},"filesystem_watch_required":{},"preview_service_receipt_required":{},"suspend_resume_required":{},"adapter_turn_on_required":true,"sandbox_authority_receipt_required":true,"human_ratification_required":true,"external_repo_checkout_allowed":false,"adapter_execution_allowed":false,"sandbox_process_started":false,"network_allowed":false,"filesystem_mutation_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&preflight.status),
        render_external_sandbox_preflight_json(preflight),
        json_string_literal(&preflight.preflight_id),
        json_string_literal(&preflight.tenant_id),
        json_string_literal(&preflight.actor_id),
        json_string_literal(&preflight.job_id),
        json_string_literal(&preflight.run_id),
        json_string_literal(&preflight.terminal_state),
        json_string_literal(&preflight.admission_decision),
        json_string_literal(&preflight.sandbox_driver),
        json_string_literal(&preflight.sandbox_provider),
        preflight.driver_is_registered,
        preflight.target_concurrent_engineers,
        preflight.peak_parallel_forge_builds,
        preflight.requested_parallel_sandboxes,
        preflight.max_parallel_sandboxes,
        preflight.warm_pool_target,
        preflight.ready_p95_ms,
        DEFAULT_EXTERNAL_SANDBOX_READY_P95_MS,
        preflight.max_runtime_seconds,
        preflight.artifact_retention_hours,
        json_string_literal(&preflight.adapter_turn_on_receipt_id),
        json_string_literal(&preflight.adapter_turn_on_status),
        preflight.adapter_turn_on_observed,
        preflight.adapter_capability_count,
        json_string_literal(&preflight.repo_checkout_mode),
        json_string_literal(&preflight.evidence_import_policy),
        json_string_literal(&preflight.workspace_snapshot_policy),
        json_string_literal(&preflight.filesystem_watch_policy),
        json_string_literal(&preflight.preview_service_policy),
        json_string_literal(&preflight.suspend_resume_policy),
        preflight.snapshot_restore_required,
        preflight.filesystem_watch_required,
        preflight.preview_service_receipt_required,
        preflight.suspend_resume_required
    )
}

fn render_external_sandbox_preflight_json(preflight: &DxrExternalSandboxPreflight) -> String {
    format!(
        r#"{{"sequence":{},"preflight_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"sandbox_driver":{},"sandbox_provider":{},"driver_is_registered":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"requested_parallel_sandboxes":{},"max_parallel_sandboxes":{},"warm_pool_target":{},"external_repo_ref":{},"external_repo_url_hash":{},"repo_checkout_mode":{},"evidence_import_policy":{},"network_policy":{},"filesystem_policy":{},"secret_policy":{},"egress_policy":{},"workspace_snapshot_policy":{},"filesystem_watch_policy":{},"preview_service_policy":{},"suspend_resume_policy":{},"max_runtime_seconds":{},"ready_p95_ms":{},"artifact_retention_hours":{},"adapter_turn_on_receipt_id":{},"adapter_turn_on_status":{},"adapter_turn_on_observed":{},"adapter_capability_count":{},"sandbox_authority_receipt_id":{},"human_ratification_receipt_id":{},"admission_decision":{},"status":{},"terminal_state":{},"authority_rejected":{},"snapshot_restore_required":{},"filesystem_watch_required":{},"preview_service_receipt_required":{},"suspend_resume_required":{},"adapter_turn_on_required":true,"sandbox_authority_receipt_required":true,"human_ratification_required":true,"external_repo_checkout_allowed":false,"adapter_execution_allowed":false,"sandbox_process_started":false,"network_allowed":false,"filesystem_mutation_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        preflight.sequence,
        json_string_literal(&preflight.preflight_id),
        json_string_literal(&preflight.tenant_id),
        json_string_literal(&preflight.actor_id),
        json_string_literal(&preflight.job_id),
        json_string_literal(&preflight.run_id),
        json_string_literal(&preflight.sandbox_driver),
        json_string_literal(&preflight.sandbox_provider),
        preflight.driver_is_registered,
        preflight.target_concurrent_engineers,
        preflight.peak_parallel_forge_builds,
        preflight.requested_parallel_sandboxes,
        preflight.max_parallel_sandboxes,
        preflight.warm_pool_target,
        json_string_literal(&preflight.external_repo_ref),
        json_string_literal(&preflight.external_repo_url_hash),
        json_string_literal(&preflight.repo_checkout_mode),
        json_string_literal(&preflight.evidence_import_policy),
        json_string_literal(&preflight.network_policy),
        json_string_literal(&preflight.filesystem_policy),
        json_string_literal(&preflight.secret_policy),
        json_string_literal(&preflight.egress_policy),
        json_string_literal(&preflight.workspace_snapshot_policy),
        json_string_literal(&preflight.filesystem_watch_policy),
        json_string_literal(&preflight.preview_service_policy),
        json_string_literal(&preflight.suspend_resume_policy),
        preflight.max_runtime_seconds,
        preflight.ready_p95_ms,
        preflight.artifact_retention_hours,
        json_string_literal(&preflight.adapter_turn_on_receipt_id),
        json_string_literal(&preflight.adapter_turn_on_status),
        preflight.adapter_turn_on_observed,
        preflight.adapter_capability_count,
        json_string_literal(&preflight.sandbox_authority_receipt_id),
        json_string_literal(&preflight.human_ratification_receipt_id),
        json_string_literal(&preflight.admission_decision),
        json_string_literal(&preflight.status),
        json_string_literal(&preflight.terminal_state),
        preflight.authority_rejected,
        preflight.snapshot_restore_required,
        preflight.filesystem_watch_required,
        preflight.preview_service_receipt_required,
        preflight.suspend_resume_required
    )
}

fn render_sandbox_authority_envelope_response_json(
    envelope: &DxrSandboxAuthorityEnvelope,
) -> String {
    format!(
        r#"{{"name":"mdx-dxr-sandbox-authority-envelope","status":{},"runtime":"mdx-dxr-engine","envelope":{},"envelope_id":{},"receipt_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"terminal_state":{},"envelope_decision":{},"sandbox_authority_staged":{},"rejected":{},"rejection_reason":{},"sandbox_admission_id":{},"sandbox_admission_decision":{},"sandbox_admission_observed":{},"adapter_turn_on_receipt_id":{},"adapter_turn_on_status":{},"adapter_turn_on_observed":{},"external_sandbox_preflight_id":{},"external_sandbox_preflight_status":{},"external_sandbox_preflight_observed":{},"human_ratification_receipt_id":{},"tool_policy_receipt_id":{},"reviewer_separation_receipt_id":{},"dispatch_claim_id":{},"heartbeat_receipt_id":{},"durable_workflow_receipt_id":{},"sandbox_process_started":false,"adapter_execution_allowed":false,"external_repo_checkout_started":false,"network_allowed":false,"filesystem_mutation_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&envelope.status),
        render_sandbox_authority_envelope_json(envelope),
        json_string_literal(&envelope.envelope_id),
        json_string_literal(&envelope.receipt_id),
        json_string_literal(&envelope.tenant_id),
        json_string_literal(&envelope.actor_id),
        json_string_literal(&envelope.job_id),
        json_string_literal(&envelope.run_id),
        json_string_literal(&envelope.worker_run_id),
        json_string_literal(&envelope.terminal_state),
        json_string_literal(&envelope.envelope_decision),
        envelope.sandbox_authority_staged,
        envelope.rejected,
        json_string_literal(&envelope.rejection_reason),
        json_string_literal(&envelope.sandbox_admission_id),
        json_string_literal(&envelope.sandbox_admission_decision),
        envelope.sandbox_admission_observed,
        json_string_literal(&envelope.adapter_turn_on_receipt_id),
        json_string_literal(&envelope.adapter_turn_on_status),
        envelope.adapter_turn_on_observed,
        json_string_literal(&envelope.external_sandbox_preflight_id),
        json_string_literal(&envelope.external_sandbox_preflight_status),
        envelope.external_sandbox_preflight_observed,
        json_string_literal(&envelope.human_ratification_receipt_id),
        json_string_literal(&envelope.tool_policy_receipt_id),
        json_string_literal(&envelope.reviewer_separation_receipt_id),
        json_string_literal(&envelope.dispatch_claim_id),
        json_string_literal(&envelope.heartbeat_receipt_id),
        json_string_literal(&envelope.durable_workflow_receipt_id)
    )
}

fn render_sandbox_authority_envelope_json(envelope: &DxrSandboxAuthorityEnvelope) -> String {
    format!(
        r#"{{"sequence":{},"envelope_id":{},"receipt_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"sandbox_admission_id":{},"sandbox_admission_decision":{},"sandbox_admission_observed":{},"adapter_turn_on_receipt_id":{},"adapter_turn_on_status":{},"adapter_turn_on_observed":{},"external_sandbox_preflight_id":{},"external_sandbox_preflight_status":{},"external_sandbox_preflight_observed":{},"human_ratification_receipt_id":{},"tool_policy_receipt_id":{},"reviewer_separation_receipt_id":{},"dispatch_claim_id":{},"heartbeat_receipt_id":{},"durable_workflow_receipt_id":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"sandbox_authority_staged":{},"status":{},"terminal_state":{},"envelope_decision":{},"rejected":{},"rejection_reason":{},"sandbox_process_started":false,"adapter_execution_allowed":false,"external_repo_checkout_started":false,"network_allowed":false,"filesystem_mutation_allowed":false,"secret_inheritance_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        envelope.sequence,
        json_string_literal(&envelope.envelope_id),
        json_string_literal(&envelope.receipt_id),
        json_string_literal(&envelope.tenant_id),
        json_string_literal(&envelope.actor_id),
        json_string_literal(&envelope.job_id),
        json_string_literal(&envelope.run_id),
        json_string_literal(&envelope.worker_run_id),
        json_string_literal(&envelope.sandbox_admission_id),
        json_string_literal(&envelope.sandbox_admission_decision),
        envelope.sandbox_admission_observed,
        json_string_literal(&envelope.adapter_turn_on_receipt_id),
        json_string_literal(&envelope.adapter_turn_on_status),
        envelope.adapter_turn_on_observed,
        json_string_literal(&envelope.external_sandbox_preflight_id),
        json_string_literal(&envelope.external_sandbox_preflight_status),
        envelope.external_sandbox_preflight_observed,
        json_string_literal(&envelope.human_ratification_receipt_id),
        json_string_literal(&envelope.tool_policy_receipt_id),
        json_string_literal(&envelope.reviewer_separation_receipt_id),
        json_string_literal(&envelope.dispatch_claim_id),
        json_string_literal(&envelope.heartbeat_receipt_id),
        json_string_literal(&envelope.durable_workflow_receipt_id),
        envelope.target_concurrent_engineers,
        envelope.peak_parallel_forge_builds,
        envelope.sandbox_authority_staged,
        json_string_literal(&envelope.status),
        json_string_literal(&envelope.terminal_state),
        json_string_literal(&envelope.envelope_decision),
        envelope.rejected,
        json_string_literal(&envelope.rejection_reason)
    )
}

fn render_execution_admission_response_json(admission: &DxrExecutionAdmission) -> String {
    format!(
        r#"{{"name":"mdx-dxr-execution-admission","status":"LIVE-LOCAL-DXR-EXECUTION-ADMISSION-FLOOR","runtime":"mdx-dxr-engine","admission":{},"admission_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"terminal_state":{},"admission_decision":{},"admitted_to_execution_preflight":{},"admitted_to_queue":{},"rejected":{},"rejection_reason":{},"dispatch_recovery_plan_required":true,"sandbox_admission_required":true,"sandbox_authority_envelope_required":true,"external_sandbox_preflight_required":true,"live_worker_preflight_required":true,"claim_before_execute_required":true,"heartbeat_before_worker_execution_required":true,"tool_policy_required":true,"reviewer_separation_required":true,"dispatch_recovery_plan_observed":{},"dispatch_claim_id":{},"heartbeat_receipt_id":{},"durable_workflow_receipt_id":{},"live_worker_preflight_id":{},"live_worker_preflight_status":{},"sandbox_admission_id":{},"sandbox_admission_decision":{},"sandbox_authority_envelope_id":{},"sandbox_authority_receipt_id":{},"sandbox_authority_status":{},"sandbox_authority_observed":{},"external_sandbox_preflight_id":{},"tool_policy_receipt_id":{},"reviewer_separation_receipt_id":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"tenant_active_forge_runs":{},"global_active_forge_runs":{},"queued_forge_runs":{},"global_max_concurrent_forge_runs":{},"max_concurrent_forge_runs_per_tenant":{},"queue_depth_limit":{},"dispatch_recovery_plan_route":"/dxr/dispatch/recovery-plan.json","worker_process_started":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        render_execution_admission_json(admission),
        json_string_literal(&admission.admission_id),
        json_string_literal(&admission.tenant_id),
        json_string_literal(&admission.actor_id),
        json_string_literal(&admission.job_id),
        json_string_literal(&admission.run_id),
        json_string_literal(&admission.worker_run_id),
        json_string_literal(&admission.terminal_state),
        json_string_literal(&admission.admission_decision),
        admission.admitted_to_execution_preflight,
        admission.admitted_to_queue,
        admission.rejected,
        json_string_literal(&admission.rejection_reason),
        admission.dispatch_recovery_plan_observed,
        json_string_literal(&admission.dispatch_claim_id),
        json_string_literal(&admission.heartbeat_receipt_id),
        json_string_literal(&admission.durable_workflow_receipt_id),
        json_string_literal(&admission.live_worker_preflight_id),
        json_string_literal(&admission.live_worker_preflight_status),
        json_string_literal(&admission.sandbox_admission_id),
        json_string_literal(&admission.sandbox_admission_decision),
        json_string_literal(&admission.sandbox_authority_envelope_id),
        json_string_literal(&admission.sandbox_authority_receipt_id),
        json_string_literal(&admission.sandbox_authority_status),
        admission.sandbox_authority_observed,
        json_string_literal(&admission.external_sandbox_preflight_id),
        json_string_literal(&admission.tool_policy_receipt_id),
        json_string_literal(&admission.reviewer_separation_receipt_id),
        admission.target_concurrent_engineers,
        admission.peak_parallel_forge_builds,
        admission.tenant_active_forge_runs,
        admission.global_active_forge_runs,
        admission.queued_forge_runs,
        admission.global_max_concurrent_forge_runs,
        admission.max_concurrent_forge_runs_per_tenant,
        admission.queue_depth_limit
    )
}

fn render_execution_admission_json(admission: &DxrExecutionAdmission) -> String {
    format!(
        r#"{{"sequence":{},"admission_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"dispatch_recovery_plan_observed":{},"dispatch_claim_id":{},"heartbeat_receipt_id":{},"durable_workflow_receipt_id":{},"live_worker_preflight_id":{},"live_worker_preflight_status":{},"sandbox_admission_id":{},"sandbox_admission_decision":{},"sandbox_authority_envelope_id":{},"sandbox_authority_receipt_id":{},"sandbox_authority_status":{},"sandbox_authority_observed":{},"external_sandbox_preflight_id":{},"tool_policy_receipt_id":{},"reviewer_separation_receipt_id":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"tenant_active_forge_runs":{},"global_active_forge_runs":{},"queued_forge_runs":{},"global_max_concurrent_forge_runs":{},"max_concurrent_forge_runs_per_tenant":{},"queue_depth_limit":{},"admission_decision":{},"terminal_state":{},"admitted_to_execution_preflight":{},"admitted_to_queue":{},"rejected":{},"rejection_reason":{},"dispatch_recovery_plan_route":"/dxr/dispatch/recovery-plan.json","dispatch_recovery_plan_required":true,"sandbox_admission_required":true,"sandbox_authority_envelope_required":true,"external_sandbox_preflight_required":true,"live_worker_preflight_required":true,"claim_before_execute_required":true,"heartbeat_before_worker_execution_required":true,"tool_policy_required":true,"reviewer_separation_required":true,"worker_process_started":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        admission.sequence,
        json_string_literal(&admission.admission_id),
        json_string_literal(&admission.tenant_id),
        json_string_literal(&admission.actor_id),
        json_string_literal(&admission.job_id),
        json_string_literal(&admission.run_id),
        json_string_literal(&admission.worker_run_id),
        admission.dispatch_recovery_plan_observed,
        json_string_literal(&admission.dispatch_claim_id),
        json_string_literal(&admission.heartbeat_receipt_id),
        json_string_literal(&admission.durable_workflow_receipt_id),
        json_string_literal(&admission.live_worker_preflight_id),
        json_string_literal(&admission.live_worker_preflight_status),
        json_string_literal(&admission.sandbox_admission_id),
        json_string_literal(&admission.sandbox_admission_decision),
        json_string_literal(&admission.sandbox_authority_envelope_id),
        json_string_literal(&admission.sandbox_authority_receipt_id),
        json_string_literal(&admission.sandbox_authority_status),
        admission.sandbox_authority_observed,
        json_string_literal(&admission.external_sandbox_preflight_id),
        json_string_literal(&admission.tool_policy_receipt_id),
        json_string_literal(&admission.reviewer_separation_receipt_id),
        admission.target_concurrent_engineers,
        admission.peak_parallel_forge_builds,
        admission.tenant_active_forge_runs,
        admission.global_active_forge_runs,
        admission.queued_forge_runs,
        admission.global_max_concurrent_forge_runs,
        admission.max_concurrent_forge_runs_per_tenant,
        admission.queue_depth_limit,
        json_string_literal(&admission.admission_decision),
        json_string_literal(&admission.terminal_state),
        admission.admitted_to_execution_preflight,
        admission.admitted_to_queue,
        admission.rejected,
        json_string_literal(&admission.rejection_reason)
    )
}

fn render_sandbox_driver_json(driver: &SandboxDriver) -> String {
    format!(
        r#"{{"name":{},"isolation":{},"status":{},"execution_surface":{},"snapshot_restore_supported":true,"filesystem_watch_supported":true,"preview_service_supported":true,"network_allowed_by_default":false,"secret_inheritance_allowed":false,"production_write_authority":false}}"#,
        json_string_literal(driver.name),
        json_string_literal(driver.isolation),
        json_string_literal(driver.status),
        json_string_literal(driver.execution_surface)
    )
}

fn render_workflow_pattern_json(pattern: &WorkflowPattern) -> String {
    format!(
        r#"{{"name":{},"purpose":{},"reviewer_context":{},"builder_can_self_accept":false,"human_ratification_boundary":"required_for_merge_or_production_write"}}"#,
        json_string_literal(pattern.name),
        json_string_literal(pattern.purpose),
        json_string_literal(pattern.reviewer_context)
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

fn u64_value(value: &Value, key: &str, default: u64) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(default)
}

fn parse_sandbox_admission_request(body: &str) -> Result<SandboxAdmissionRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR sandbox admission json: {error}"))?;
    Ok(SandboxAdmissionRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", "dxr_job_sandbox_admission_001"),
        run_id: string_value(&value, "run_id", "dxr_run_sandbox_admission_001"),
        requested_sandbox_count: usize_value(&value, "requested_sandbox_count", 1),
        active_sandbox_count: usize_value(&value, "active_sandbox_count", 0),
        sandbox_pool_size: usize_value(&value, "sandbox_pool_size", DEFAULT_SANDBOX_POOL_SIZE),
        tenant_active_forge_runs: usize_value(&value, "tenant_active_forge_runs", 0),
        global_active_forge_runs: usize_value(&value, "global_active_forge_runs", 0),
        queued_forge_runs: usize_value(&value, "queued_forge_runs", 0),
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
        queue_depth_limit: usize_value(&value, "queue_depth_limit", DEFAULT_QUEUE_DEPTH_LIMIT),
        repo_ref: string_value(&value, "repo_ref", "current_repo"),
        repo_isolation_mode: string_value(
            &value,
            "repo_isolation_mode",
            "ephemeral_per_run_workspace",
        ),
        sandbox_driver: string_value(&value, "sandbox_driver", "local_docker"),
        sandbox_isolation_policy: string_value(
            &value,
            "sandbox_isolation_policy",
            "container_per_run_no_network_readonly_rootfs_secret_strip",
        ),
        network_allowed: bool_value(&value, "network_allowed", false),
        secret_inheritance_allowed: bool_value(&value, "secret_inheritance_allowed", false),
        production_write_authority: bool_value(&value, "production_write_authority", false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capacity_plan_applies_backpressure_without_authority() {
        let mut runtime = DxrCapacityRuntime::new();
        let result = runtime.submit_json("{}").expect("capacity plan");
        assert!(
            result
                .body
                .contains("LIVE-LOCAL-DXR-CAPACITY-SANDBOX-FLOOR")
        );
        assert!(result.body.contains("backpressure_applied"));
        assert!(result.body.contains("\"sandbox_process_started\":false"));
        assert!(
            result
                .body
                .contains("\"max_concurrent_agents_per_workflow\":16")
        );
        assert!(result.body.contains("\"peak_parallel_forge_builds\":5000"));
        assert!(
            result
                .body
                .contains("\"max_total_agents_per_workflow\":1000")
        );
        assert!(result.events.len() >= 7);
    }

    #[test]
    fn sandbox_admission_records_admit_queue_and_security_reject() {
        let mut runtime = DxrCapacityRuntime::new();
        let admitted = runtime
            .submit_sandbox_admission_json(
                r#"{"global_active_forge_runs":42,"tenant_active_forge_runs":4,"active_sandbox_count":10}"#,
            )
            .expect("admitted");
        assert!(
            admitted
                .body
                .contains("DXR_SANDBOX_ADMISSION_RECORDED_POOL_SLOT_RESERVED")
        );
        assert!(admitted.body.contains("\"admitted_to_sandbox_pool\":true"));
        let queued = runtime
            .submit_sandbox_admission_json(
                r#"{"global_active_forge_runs":5000,"active_sandbox_count":2048}"#,
            )
            .expect("queued");
        assert!(
            queued
                .body
                .contains("DXR_SANDBOX_ADMISSION_QUEUED_BACKPRESSURE")
        );
        assert!(queued.body.contains("\"admitted_to_queue\":true"));
        let rejected = runtime
            .submit_sandbox_admission_json(r#"{"secret_inheritance_allowed":true}"#)
            .expect("rejected");
        assert!(
            rejected
                .body
                .contains("DXR_SANDBOX_ADMISSION_REJECTED_SECURITY_BOUNDARY")
        );
        assert!(rejected.body.contains("\"sandbox_process_started\":false"));
        let projection = runtime.sandbox_admissions_json();
        assert!(projection.contains("\"admission_count\":3"));
        assert!(projection.contains("\"admitted_count\":1"));
        assert!(projection.contains("\"queued_count\":1"));
        assert!(projection.contains("\"rejected_count\":1"));
    }

    #[test]
    fn sandbox_adapter_turn_on_records_secret_safe_readiness_and_rejects_authority() {
        let mut runtime = DxrCapacityRuntime::new();
        let accepted = runtime
            .submit_sandbox_adapter_turn_on_json(
                r#"{"sandbox_driver":"firecracker_microvm","sandbox_provider":"local_firecracker_adapter","adapter_version":"local-proof-adapter-v1","target_concurrent_engineers":1000,"peak_parallel_forge_builds":5000,"max_parallel_sandboxes":5000,"warm_pool_target":2048,"ready_p95_ms":2400,"ready_p95_ms_ceiling":2500,"adapter_registry_observed":true,"isolation_boundary_observed":true,"warm_pool_observed":true,"snapshot_restore_observed":true,"filesystem_watch_observed":true,"preview_service_observed":true,"suspend_resume_observed":true,"log_stream_observed":true,"artifact_retention_observed":true,"network_policy_observed":true,"secret_policy_observed":true,"egress_policy_observed":true,"ready_p95_observed":true,"scale_capacity_observed":true,"human_ratification_gate_observed":true,"evidence_quarantine_observed":true}"#,
            )
            .expect("accepted turn-on");
        assert!(
            accepted
                .body
                .contains("LIVE-LOCAL-DXR-SANDBOX-ADAPTER-TURN-ON-FLOOR")
        );
        assert!(
            accepted
                .body
                .contains("DXR_SANDBOX_ADAPTER_TURN_ON_RECORDED_EXECUTION_BLOCKED")
        );
        assert!(
            accepted
                .body
                .contains("\"adapter_ready_for_preflight\":true")
        );
        assert!(accepted.body.contains("\"capability_count\":16"));
        assert!(
            accepted
                .body
                .contains("\"adapter_execution_allowed\":false")
        );
        assert!(accepted.body.contains("\"sandbox_process_started\":false"));
        assert!(
            accepted
                .body
                .contains("\"secret_inheritance_allowed\":false")
        );

        let rejected = runtime
            .submit_sandbox_adapter_turn_on_json(
                r#"{"sandbox_driver":"firecracker_microvm","adapter_registry_observed":true,"isolation_boundary_observed":true,"warm_pool_observed":true,"snapshot_restore_observed":true,"filesystem_watch_observed":true,"preview_service_observed":true,"suspend_resume_observed":true,"log_stream_observed":true,"artifact_retention_observed":true,"network_policy_observed":true,"secret_policy_observed":true,"egress_policy_observed":true,"ready_p95_observed":true,"scale_capacity_observed":true,"human_ratification_gate_observed":true,"evidence_quarantine_observed":true,"network_requested":true}"#,
            )
            .expect("rejected turn-on");
        assert!(
            rejected
                .body
                .contains("DXR_SANDBOX_ADAPTER_TURN_ON_REJECTED_BOUNDARY")
        );
        assert!(
            rejected
                .body
                .contains("sandbox_adapter_authority_requested")
        );
        assert!(
            rejected
                .body
                .contains("\"adapter_ready_for_preflight\":false")
        );

        let projection = runtime.sandbox_adapter_turn_ons_json();
        assert!(projection.contains("\"turn_on_count\":2"));
        assert!(projection.contains("\"accepted_count\":1"));
        assert!(projection.contains("\"rejected_count\":1"));
        assert!(projection.contains("\"external_preflight_requires_turn_on_receipt\":true"));
        assert!(projection.contains("cloudflare_sandbox"));
        assert!(projection.contains("modal_sandbox"));
        assert!(projection.contains("e2b_sandbox"));
    }

    #[test]
    fn external_sandbox_preflight_records_adapter_candidate_and_rejects_authority() {
        let mut runtime = DxrCapacityRuntime::new();
        runtime
            .submit_sandbox_adapter_turn_on_json(
                r#"{"sandbox_driver":"firecracker_microvm","sandbox_provider":"local_firecracker_adapter","adapter_version":"local-proof-adapter-v1","target_concurrent_engineers":1000,"peak_parallel_forge_builds":5000,"max_parallel_sandboxes":5000,"warm_pool_target":2048,"ready_p95_ms":2400,"ready_p95_ms_ceiling":2500,"adapter_registry_observed":true,"isolation_boundary_observed":true,"warm_pool_observed":true,"snapshot_restore_observed":true,"filesystem_watch_observed":true,"preview_service_observed":true,"suspend_resume_observed":true,"log_stream_observed":true,"artifact_retention_observed":true,"network_policy_observed":true,"secret_policy_observed":true,"egress_policy_observed":true,"ready_p95_observed":true,"scale_capacity_observed":true,"human_ratification_gate_observed":true,"evidence_quarantine_observed":true}"#,
            )
            .expect("turn-on");
        let accepted = runtime
            .submit_external_sandbox_preflight_json(
                r#"{"sandbox_driver":"firecracker_microvm","requested_parallel_sandboxes":5000,"max_parallel_sandboxes":5000,"ready_p95_ms":2400,"adapter_turn_on_receipt_id":"dxr_sandbox_adapter_turn_on_receipt_000001","adapter_turn_on_status":"LIVE-LOCAL-DXR-SANDBOX-ADAPTER-TURN-ON-FLOOR","adapter_turn_on_observed":true,"adapter_capability_count":16}"#,
            )
            .expect("accepted preflight");
        assert!(
            accepted
                .body
                .contains("LIVE-LOCAL-DXR-EXTERNAL-SANDBOX-PREFLIGHT")
        );
        assert!(
            accepted
                .body
                .contains("DXR_EXTERNAL_SANDBOX_PREFLIGHT_RECORDED_AUTHORITY_BLOCKED")
        );
        assert!(accepted.body.contains("\"driver_is_registered\":true"));
        assert!(
            accepted
                .body
                .contains("\"external_repo_checkout_allowed\":false")
        );
        assert!(
            accepted
                .body
                .contains("\"adapter_execution_allowed\":false")
        );
        assert!(accepted.body.contains("\"adapter_turn_on_observed\":true"));
        assert!(accepted.body.contains("\"adapter_capability_count\":16"));
        assert!(accepted.body.contains("\"network_allowed\":false"));
        assert!(
            accepted
                .body
                .contains("\"secret_inheritance_allowed\":false")
        );

        let rejected = runtime
            .submit_external_sandbox_preflight_json(
                r#"{"sandbox_driver":"e2b_sandbox","adapter_execution_allowed":true,"external_repo_checkout_allowed":true,"network_allowed":true,"secret_inheritance_allowed":true,"filesystem_mutation_allowed":true,"production_write_authority":true}"#,
            )
            .expect("rejected preflight");
        assert!(
            rejected
                .body
                .contains("DXR_EXTERNAL_SANDBOX_PREFLIGHT_REJECTED_BOUNDARY")
        );
        assert!(rejected.body.contains("rejected_external_sandbox_boundary"));
        assert!(rejected.body.contains("\"sandbox_process_started\":false"));

        let projection = runtime.external_sandbox_preflights_json();
        assert!(projection.contains("\"preflight_count\":2"));
        assert!(projection.contains("\"accepted_count\":1"));
        assert!(projection.contains("\"rejected_count\":1"));
        assert!(projection.contains("cloudflare_sandbox"));
        assert!(projection.contains("modal_sandbox"));
        assert!(projection.contains("e2b_sandbox"));
    }

    #[test]
    fn sandbox_authority_envelope_stages_from_recorded_evidence_and_rejects_authority() {
        let mut runtime = DxrCapacityRuntime::new();
        runtime
            .submit_sandbox_admission_json(
                r#"{"global_active_forge_runs":42,"tenant_active_forge_runs":4,"active_sandbox_count":10}"#,
            )
            .expect("admission");
        runtime
            .submit_sandbox_adapter_turn_on_json(
                r#"{"sandbox_driver":"firecracker_microvm","sandbox_provider":"local_firecracker_adapter","adapter_version":"local-proof-adapter-v1","target_concurrent_engineers":1000,"peak_parallel_forge_builds":5000,"max_parallel_sandboxes":5000,"warm_pool_target":2048,"ready_p95_ms":2400,"ready_p95_ms_ceiling":2500,"adapter_registry_observed":true,"isolation_boundary_observed":true,"warm_pool_observed":true,"snapshot_restore_observed":true,"filesystem_watch_observed":true,"preview_service_observed":true,"suspend_resume_observed":true,"log_stream_observed":true,"artifact_retention_observed":true,"network_policy_observed":true,"secret_policy_observed":true,"egress_policy_observed":true,"ready_p95_observed":true,"scale_capacity_observed":true,"human_ratification_gate_observed":true,"evidence_quarantine_observed":true}"#,
            )
            .expect("turn-on");
        runtime
            .submit_external_sandbox_preflight_json(
                r#"{"sandbox_driver":"firecracker_microvm","requested_parallel_sandboxes":5000,"max_parallel_sandboxes":5000,"ready_p95_ms":2400,"adapter_turn_on_receipt_id":"dxr_sandbox_adapter_turn_on_receipt_000001","adapter_turn_on_status":"LIVE-LOCAL-DXR-SANDBOX-ADAPTER-TURN-ON-FLOOR","adapter_turn_on_observed":true,"adapter_capability_count":16}"#,
            )
            .expect("preflight");
        let staged = runtime
            .submit_sandbox_authority_envelope_json(
                r#"{"sandbox_admission_id":"dxr_sandbox_admission_000001","sandbox_admission_decision":"admitted_to_sandbox_pool_without_start","sandbox_admission_observed":true,"adapter_turn_on_receipt_id":"dxr_sandbox_adapter_turn_on_receipt_000001","adapter_turn_on_status":"LIVE-LOCAL-DXR-SANDBOX-ADAPTER-TURN-ON-FLOOR","adapter_turn_on_observed":true,"external_sandbox_preflight_id":"dxr_external_sandbox_preflight_000001","external_sandbox_preflight_status":"LIVE-LOCAL-DXR-EXTERNAL-SANDBOX-PREFLIGHT","external_sandbox_preflight_observed":true,"human_ratification_receipt_id":"human_ratification_receipt_001","tool_policy_receipt_id":"dxr_tool_policy_receipt_001","reviewer_separation_receipt_id":"dxr_reviewer_separation_receipt_001","dispatch_claim_id":"dxr_dispatch_claim_001","heartbeat_receipt_id":"dxr_dispatch_heartbeat_receipt_001","durable_workflow_receipt_id":"dxr_durable_workflow_receipt_001"}"#,
            )
            .expect("authority");
        assert!(
            staged
                .body
                .contains("LIVE-LOCAL-DXR-SANDBOX-AUTHORITY-ENVELOPE-FLOOR")
        );
        assert!(staged.body.contains("\"sandbox_authority_staged\":true"));
        assert!(staged.body.contains("\"sandbox_process_started\":false"));
        assert!(staged.body.contains("\"network_allowed\":false"));

        let rejected = runtime
            .submit_sandbox_authority_envelope_json(
                r#"{"sandbox_admission_id":"dxr_sandbox_admission_000001","sandbox_admission_decision":"admitted_to_sandbox_pool_without_start","sandbox_admission_observed":true,"adapter_turn_on_receipt_id":"dxr_sandbox_adapter_turn_on_receipt_000001","adapter_turn_on_status":"LIVE-LOCAL-DXR-SANDBOX-ADAPTER-TURN-ON-FLOOR","adapter_turn_on_observed":true,"external_sandbox_preflight_id":"dxr_external_sandbox_preflight_000001","external_sandbox_preflight_status":"LIVE-LOCAL-DXR-EXTERNAL-SANDBOX-PREFLIGHT","external_sandbox_preflight_observed":true,"human_ratification_receipt_id":"human_ratification_receipt_001","tool_policy_receipt_id":"dxr_tool_policy_receipt_001","reviewer_separation_receipt_id":"dxr_reviewer_separation_receipt_001","dispatch_claim_id":"dxr_dispatch_claim_001","heartbeat_receipt_id":"dxr_dispatch_heartbeat_receipt_001","durable_workflow_receipt_id":"dxr_durable_workflow_receipt_001","sandbox_process_start_requested":true}"#,
            )
            .expect("rejected authority");
        assert!(
            rejected
                .body
                .contains("DXR_SANDBOX_AUTHORITY_ENVELOPE_REJECTED_BOUNDARY")
        );
        assert!(rejected.body.contains("\"sandbox_authority_staged\":false"));

        let projection = runtime.sandbox_authority_envelopes_json();
        assert!(projection.contains("\"envelope_count\":2"));
        assert!(projection.contains("\"staged_count\":1"));
        assert!(projection.contains("\"rejected_count\":1"));
        assert!(projection.contains("\"execution_admission_requires_sandbox_authority\":true"));
    }

    #[test]
    fn execution_admission_records_staged_queue_and_reject_without_execution() {
        let mut runtime = DxrCapacityRuntime::new();
        runtime
            .submit_sandbox_admission_json(
                r#"{"global_active_forge_runs":42,"tenant_active_forge_runs":4,"active_sandbox_count":10}"#,
            )
            .expect("admission");
        runtime
            .submit_sandbox_adapter_turn_on_json(
                r#"{"sandbox_driver":"firecracker_microvm","sandbox_provider":"local_firecracker_adapter","adapter_version":"local-proof-adapter-v1","target_concurrent_engineers":1000,"peak_parallel_forge_builds":5000,"max_parallel_sandboxes":5000,"warm_pool_target":2048,"ready_p95_ms":2400,"ready_p95_ms_ceiling":2500,"adapter_registry_observed":true,"isolation_boundary_observed":true,"warm_pool_observed":true,"snapshot_restore_observed":true,"filesystem_watch_observed":true,"preview_service_observed":true,"suspend_resume_observed":true,"log_stream_observed":true,"artifact_retention_observed":true,"network_policy_observed":true,"secret_policy_observed":true,"egress_policy_observed":true,"ready_p95_observed":true,"scale_capacity_observed":true,"human_ratification_gate_observed":true,"evidence_quarantine_observed":true}"#,
            )
            .expect("turn-on");
        runtime
            .submit_external_sandbox_preflight_json(
                r#"{"sandbox_driver":"firecracker_microvm","requested_parallel_sandboxes":5000,"max_parallel_sandboxes":5000,"ready_p95_ms":2400,"adapter_turn_on_receipt_id":"dxr_sandbox_adapter_turn_on_receipt_000001","adapter_turn_on_status":"LIVE-LOCAL-DXR-SANDBOX-ADAPTER-TURN-ON-FLOOR","adapter_turn_on_observed":true,"adapter_capability_count":16}"#,
            )
            .expect("preflight");
        runtime
            .submit_sandbox_authority_envelope_json(
                r#"{"sandbox_admission_id":"dxr_sandbox_admission_000001","sandbox_admission_decision":"admitted_to_sandbox_pool_without_start","sandbox_admission_observed":true,"adapter_turn_on_receipt_id":"dxr_sandbox_adapter_turn_on_receipt_000001","adapter_turn_on_status":"LIVE-LOCAL-DXR-SANDBOX-ADAPTER-TURN-ON-FLOOR","adapter_turn_on_observed":true,"external_sandbox_preflight_id":"dxr_external_sandbox_preflight_000001","external_sandbox_preflight_status":"LIVE-LOCAL-DXR-EXTERNAL-SANDBOX-PREFLIGHT","external_sandbox_preflight_observed":true,"human_ratification_receipt_id":"human_ratification_receipt_001","tool_policy_receipt_id":"dxr_tool_policy_receipt_001","reviewer_separation_receipt_id":"dxr_reviewer_separation_receipt_001","dispatch_claim_id":"dxr_dispatch_claim_001","heartbeat_receipt_id":"dxr_dispatch_heartbeat_receipt_001","durable_workflow_receipt_id":"dxr_durable_workflow_receipt_001"}"#,
            )
            .expect("authority");
        let staged = runtime
            .submit_execution_admission_json(
                r#"{"dispatch_recovery_plan_observed":true,"dispatch_claim_id":"dxr_dispatch_claim_001","heartbeat_receipt_id":"dxr_dispatch_heartbeat_receipt_001","durable_workflow_receipt_id":"dxr_durable_workflow_receipt_001","live_worker_preflight_id":"dxr_live_worker_preflight_001","live_worker_preflight_status":"DXR_LIVE_WORKER_EXECUTION_PREFLIGHT_RECORDED_AUTHORITY_ENVELOPE_STAGED","sandbox_admission_id":"dxr_sandbox_admission_000001","sandbox_admission_decision":"admitted_to_sandbox_pool_without_start","sandbox_authority_envelope_id":"dxr_sandbox_authority_envelope_000001","sandbox_authority_receipt_id":"dxr_sandbox_authority_receipt_000001","sandbox_authority_status":"LIVE-LOCAL-DXR-SANDBOX-AUTHORITY-ENVELOPE-FLOOR","sandbox_authority_observed":true,"external_sandbox_preflight_id":"dxr_external_sandbox_preflight_000001","tool_policy_receipt_id":"dxr_tool_policy_receipt_001","reviewer_separation_receipt_id":"dxr_reviewer_separation_receipt_001"}"#,
            )
            .expect("staged admission");
        assert!(
            staged
                .body
                .contains("DXR_EXECUTION_ADMISSION_STAGED_EXECUTION_BLOCKED")
        );
        assert!(
            staged
                .body
                .contains("\"admitted_to_execution_preflight\":true")
        );
        assert!(staged.body.contains("\"worker_process_started\":false"));
        assert!(staged.body.contains("\"sandbox_process_started\":false"));

        let queued = runtime
            .submit_execution_admission_json(
                r#"{"dispatch_recovery_plan_observed":true,"dispatch_claim_id":"dxr_dispatch_claim_002","heartbeat_receipt_id":"dxr_dispatch_heartbeat_receipt_002","durable_workflow_receipt_id":"dxr_durable_workflow_receipt_002","live_worker_preflight_id":"dxr_live_worker_preflight_002","live_worker_preflight_status":"DXR_LIVE_WORKER_EXECUTION_PREFLIGHT_RECORDED_AUTHORITY_ENVELOPE_STAGED","sandbox_admission_id":"dxr_sandbox_admission_002","sandbox_admission_decision":"queued_backpressure","sandbox_authority_envelope_id":"dxr_sandbox_authority_envelope_000001","sandbox_authority_receipt_id":"dxr_sandbox_authority_receipt_000001","sandbox_authority_status":"LIVE-LOCAL-DXR-SANDBOX-AUTHORITY-ENVELOPE-FLOOR","sandbox_authority_observed":true,"external_sandbox_preflight_id":"dxr_external_sandbox_preflight_002","tool_policy_receipt_id":"dxr_tool_policy_receipt_002","reviewer_separation_receipt_id":"dxr_reviewer_separation_receipt_002","global_active_forge_runs":4999,"tenant_active_forge_runs":499,"queued_forge_runs":2400}"#,
            )
            .expect("queued admission");
        assert!(
            queued
                .body
                .contains("DXR_EXECUTION_ADMISSION_QUEUED_BACKPRESSURE")
        );
        assert!(queued.body.contains("\"admitted_to_queue\":true"));

        let rejected = runtime
            .submit_execution_admission_json(
                r#"{"dispatch_recovery_plan_observed":true,"dispatch_claim_id":"dxr_dispatch_claim_003","heartbeat_receipt_id":"dxr_dispatch_heartbeat_receipt_003","durable_workflow_receipt_id":"dxr_durable_workflow_receipt_003","live_worker_preflight_id":"dxr_live_worker_preflight_003","live_worker_preflight_status":"DXR_LIVE_WORKER_EXECUTION_PREFLIGHT_RECORDED_AUTHORITY_ENVELOPE_STAGED","sandbox_admission_id":"dxr_sandbox_admission_003","sandbox_admission_decision":"admitted_to_sandbox_pool_without_start","external_sandbox_preflight_id":"dxr_external_sandbox_preflight_003","tool_policy_receipt_id":"dxr_tool_policy_receipt_003","reviewer_separation_receipt_id":"dxr_reviewer_separation_receipt_003","network_allowed":true}"#,
            )
            .expect("rejected admission");
        assert!(
            rejected
                .body
                .contains("DXR_EXECUTION_ADMISSION_REJECTED_SECURITY_BOUNDARY")
        );
        assert!(rejected.body.contains("\"rejected\":true"));

        let projection = runtime.execution_admissions_json();
        assert!(projection.contains("LIVE-LOCAL-DXR-EXECUTION-ADMISSION-FLOOR"));
        assert!(projection.contains("\"admission_count\":3"));
        assert!(projection.contains("\"staged_count\":1"));
        assert!(projection.contains("\"queued_count\":1"));
        assert!(projection.contains("\"rejected_count\":1"));
        assert!(projection.contains("\"target_concurrent_engineers\":1000"));
        assert!(projection.contains("\"peak_parallel_forge_builds\":5000"));
        assert!(projection.contains("\"worker_process_started\":false"));
        assert!(projection.contains("\"sandbox_process_started\":false"));
        assert!(projection.contains("\"provider_calls_allowed\":false"));
        assert!(projection.contains("\"production_writes_allowed\":false"));
    }
}
