use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_TARGET_ENGINEERS: usize = 1000;
const DEFAULT_PEAK_PARALLEL_FORGE_BUILDS: usize = 5000;
const DEFAULT_REQUESTED_SESSION_COUNT: usize = 5000;
const DEFAULT_ACTIVE_SESSION_CAPACITY: usize = 2048;
const DEFAULT_WARM_POOL_TARGET: usize = 2048;
const DEFAULT_SESSION_BATCH_SIZE: usize = 320;
const DEFAULT_READY_P95_MS: usize = 2500;
const DEFAULT_LEASE_DURATION_MS: usize = 300_000;
const DEFAULT_HEARTBEAT_INTERVAL_MS: usize = 5_000;
const DEFAULT_IDLE_TIMEOUT_MS: usize = 60_000;
const DEFAULT_RESTART_REPLAY_WINDOW_MS: usize = 900_000;
const DEFAULT_CPU_MILLICORES: usize = 2000;
const DEFAULT_MEMORY_MB: usize = 4096;
const DEFAULT_DISK_MB: usize = 16_384;

#[derive(Default)]
pub struct DxrSandboxSessionRuntime {
    sessions: Vec<DxrSandboxSession>,
    next_session: usize,
}

pub struct DxrSandboxSessionResult {
    pub body: String,
    pub events: Vec<DxrSandboxSessionRuntimeEvent>,
}

pub struct DxrSandboxSessionRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrSandboxSession {
    sequence: usize,
    session_id: String,
    session_lease_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    idempotency_key: String,
    runner_id: String,
    runner_status: String,
    runner_provider: String,
    runner_driver: String,
    runner_region: String,
    sandbox_handoff_id: String,
    sandbox_authority_envelope_id: String,
    execution_schedule_id: String,
    execution_supervision_id: String,
    dispatch_claim_id: String,
    heartbeat_receipt_id: String,
    durable_workflow_receipt_id: String,
    external_sandbox_preflight_id: String,
    sandbox_adapter_turn_on_receipt_id: String,
    ctx_context_input_id: String,
    evidence_chain_receipt_id: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    requested_session_count: usize,
    active_session_capacity: usize,
    queued_session_count: usize,
    warm_pool_target: usize,
    session_batch_size: usize,
    session_batch_count: usize,
    ready_p95_ms: usize,
    ready_p95_ms_ceiling: usize,
    lease_duration_ms: usize,
    heartbeat_interval_ms: usize,
    idle_timeout_ms: usize,
    restart_replay_window_ms: usize,
    cpu_millicores: usize,
    memory_mb: usize,
    disk_mb: usize,
    runner_ready_observed: bool,
    session_identity_observed: bool,
    lease_observed: bool,
    warm_pool_slot_observed: bool,
    attach_handle_observed: bool,
    detach_handle_observed: bool,
    resume_token_observed: bool,
    terminal_websocket_observed: bool,
    command_stream_observed: bool,
    log_stream_observed: bool,
    artifact_stream_observed: bool,
    filesystem_watch_observed: bool,
    preview_service_observed: bool,
    restart_replay_observed: bool,
    state_loss_reinit_observed: bool,
    version_compatibility_observed: bool,
    resource_limits_observed: bool,
    network_policy_observed: bool,
    secret_policy_observed: bool,
    egress_policy_observed: bool,
    kill_switch_observed: bool,
    health_probe_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    ready_p95_observed: bool,
    scale_capacity_observed: bool,
    session_ready: bool,
    status: String,
    terminal_state: String,
    session_decision: String,
    rejected: bool,
    rejection_reason: String,
}

struct SandboxSessionRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    idempotency_key: String,
    runner_id: String,
    runner_status: String,
    runner_provider: String,
    runner_driver: String,
    runner_region: String,
    sandbox_handoff_id: String,
    sandbox_authority_envelope_id: String,
    execution_schedule_id: String,
    execution_supervision_id: String,
    dispatch_claim_id: String,
    heartbeat_receipt_id: String,
    durable_workflow_receipt_id: String,
    external_sandbox_preflight_id: String,
    sandbox_adapter_turn_on_receipt_id: String,
    ctx_context_input_id: String,
    evidence_chain_receipt_id: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    requested_session_count: usize,
    active_session_capacity: usize,
    warm_pool_target: usize,
    session_batch_size: usize,
    ready_p95_ms: usize,
    ready_p95_ms_ceiling: usize,
    lease_duration_ms: usize,
    heartbeat_interval_ms: usize,
    idle_timeout_ms: usize,
    restart_replay_window_ms: usize,
    cpu_millicores: usize,
    memory_mb: usize,
    disk_mb: usize,
    runner_ready_observed: bool,
    session_identity_observed: bool,
    lease_observed: bool,
    warm_pool_slot_observed: bool,
    attach_handle_observed: bool,
    detach_handle_observed: bool,
    resume_token_observed: bool,
    terminal_websocket_observed: bool,
    command_stream_observed: bool,
    log_stream_observed: bool,
    artifact_stream_observed: bool,
    filesystem_watch_observed: bool,
    preview_service_observed: bool,
    restart_replay_observed: bool,
    state_loss_reinit_observed: bool,
    version_compatibility_observed: bool,
    resource_limits_observed: bool,
    network_policy_observed: bool,
    secret_policy_observed: bool,
    egress_policy_observed: bool,
    kill_switch_observed: bool,
    health_probe_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    ready_p95_observed: bool,
    scale_capacity_observed: bool,
    worker_process_start_requested: bool,
    sandbox_process_start_requested: bool,
    sandbox_command_requested: bool,
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

impl DxrSandboxSessionRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrSandboxSessionResult, String> {
        let request = parse_sandbox_session_request(body)?;
        self.next_session += 1;

        let authority_requested = request.worker_process_start_requested
            || request.sandbox_process_start_requested
            || request.sandbox_command_requested
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
        let provider_supported = session_provider_is_supported(&request);
        let queued_session_count = request
            .requested_session_count
            .saturating_sub(request.active_session_capacity);
        let session_batch_count = ceil_div(
            request.requested_session_count,
            request.session_batch_size.max(1),
        );
        let missing_evidence = request.idempotency_key.trim().is_empty()
            || request.runner_id.trim().is_empty()
            || request.runner_status.trim().is_empty()
            || request.runner_status.contains("REJECTED")
            || request.sandbox_handoff_id.trim().is_empty()
            || request.sandbox_authority_envelope_id.trim().is_empty()
            || request.execution_schedule_id.trim().is_empty()
            || request.execution_supervision_id.trim().is_empty()
            || request.dispatch_claim_id.trim().is_empty()
            || request.heartbeat_receipt_id.trim().is_empty()
            || request.durable_workflow_receipt_id.trim().is_empty()
            || request.external_sandbox_preflight_id.trim().is_empty()
            || request.sandbox_adapter_turn_on_receipt_id.trim().is_empty()
            || request.ctx_context_input_id.trim().is_empty()
            || request.evidence_chain_receipt_id.trim().is_empty()
            || !provider_supported
            || request.requested_session_count == 0
            || request.active_session_capacity == 0
            || request.warm_pool_target == 0
            || request.session_batch_size == 0
            || request.ready_p95_ms > request.ready_p95_ms_ceiling
            || request.lease_duration_ms == 0
            || request.heartbeat_interval_ms == 0
            || request.idle_timeout_ms == 0
            || request.restart_replay_window_ms == 0
            || request.cpu_millicores == 0
            || request.memory_mb == 0
            || request.disk_mb == 0
            || !request.runner_ready_observed
            || !request.session_identity_observed
            || !request.lease_observed
            || !request.warm_pool_slot_observed
            || !request.attach_handle_observed
            || !request.detach_handle_observed
            || !request.resume_token_observed
            || !request.terminal_websocket_observed
            || !request.command_stream_observed
            || !request.log_stream_observed
            || !request.artifact_stream_observed
            || !request.filesystem_watch_observed
            || !request.preview_service_observed
            || !request.restart_replay_observed
            || !request.state_loss_reinit_observed
            || !request.version_compatibility_observed
            || !request.resource_limits_observed
            || !request.network_policy_observed
            || !request.secret_policy_observed
            || !request.egress_policy_observed
            || !request.kill_switch_observed
            || !request.health_probe_observed
            || !request.tenant_fairness_observed
            || !request.backpressure_observed
            || !request.ready_p95_observed
            || !request.scale_capacity_observed;

        let (status, terminal_state, session_decision, rejected, rejection_reason) =
            if authority_requested {
                (
                    "DXR_SANDBOX_SESSION_LEASE_REJECTED_SECURITY_BOUNDARY",
                    "DXR_SANDBOX_SESSION_LEASE_RECORDED_EXECUTION_BLOCKED",
                    "rejected_session_execution_authority_requested",
                    true,
                    "session_lease_cannot_start_worker_sandbox_command_checkout_provider_tool_shell_patch_git_network_secret_filesystem_ci_deployment_or_production_writes",
                )
            } else if missing_evidence {
                (
                    "DXR_SANDBOX_SESSION_LEASE_REJECTED_MISSING_EVIDENCE",
                    "DXR_SANDBOX_SESSION_LEASE_RECORDED_EXECUTION_BLOCKED",
                    "rejected_missing_session_evidence",
                    true,
                    "runner_session_lease_lifecycle_stream_resource_policy_restart_health_or_scale_evidence_missing",
                )
            } else {
                (
                    "LIVE-LOCAL-DXR-SANDBOX-SESSION-LEASE-FLOOR",
                    "DXR_SANDBOX_SESSION_LEASE_RECORDED_READY_EXECUTION_BLOCKED",
                    "session_lease_ready_execution_blocked",
                    false,
                    "none",
                )
            };

        let session = DxrSandboxSession {
            sequence: self.next_session,
            session_id: format!("dxr_sandbox_session_{:06}", self.next_session),
            session_lease_id: format!("dxr_sandbox_session_lease_{:06}", self.next_session),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            worker_run_id: request.worker_run_id,
            idempotency_key: request.idempotency_key,
            runner_id: request.runner_id,
            runner_status: request.runner_status,
            runner_provider: request.runner_provider,
            runner_driver: request.runner_driver,
            runner_region: request.runner_region,
            sandbox_handoff_id: request.sandbox_handoff_id,
            sandbox_authority_envelope_id: request.sandbox_authority_envelope_id,
            execution_schedule_id: request.execution_schedule_id,
            execution_supervision_id: request.execution_supervision_id,
            dispatch_claim_id: request.dispatch_claim_id,
            heartbeat_receipt_id: request.heartbeat_receipt_id,
            durable_workflow_receipt_id: request.durable_workflow_receipt_id,
            external_sandbox_preflight_id: request.external_sandbox_preflight_id,
            sandbox_adapter_turn_on_receipt_id: request.sandbox_adapter_turn_on_receipt_id,
            ctx_context_input_id: request.ctx_context_input_id,
            evidence_chain_receipt_id: request.evidence_chain_receipt_id,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            requested_session_count: request.requested_session_count,
            active_session_capacity: request.active_session_capacity,
            queued_session_count,
            warm_pool_target: request.warm_pool_target,
            session_batch_size: request.session_batch_size,
            session_batch_count,
            ready_p95_ms: request.ready_p95_ms,
            ready_p95_ms_ceiling: request.ready_p95_ms_ceiling,
            lease_duration_ms: request.lease_duration_ms,
            heartbeat_interval_ms: request.heartbeat_interval_ms,
            idle_timeout_ms: request.idle_timeout_ms,
            restart_replay_window_ms: request.restart_replay_window_ms,
            cpu_millicores: request.cpu_millicores,
            memory_mb: request.memory_mb,
            disk_mb: request.disk_mb,
            runner_ready_observed: request.runner_ready_observed,
            session_identity_observed: request.session_identity_observed,
            lease_observed: request.lease_observed,
            warm_pool_slot_observed: request.warm_pool_slot_observed,
            attach_handle_observed: request.attach_handle_observed,
            detach_handle_observed: request.detach_handle_observed,
            resume_token_observed: request.resume_token_observed,
            terminal_websocket_observed: request.terminal_websocket_observed,
            command_stream_observed: request.command_stream_observed,
            log_stream_observed: request.log_stream_observed,
            artifact_stream_observed: request.artifact_stream_observed,
            filesystem_watch_observed: request.filesystem_watch_observed,
            preview_service_observed: request.preview_service_observed,
            restart_replay_observed: request.restart_replay_observed,
            state_loss_reinit_observed: request.state_loss_reinit_observed,
            version_compatibility_observed: request.version_compatibility_observed,
            resource_limits_observed: request.resource_limits_observed,
            network_policy_observed: request.network_policy_observed,
            secret_policy_observed: request.secret_policy_observed,
            egress_policy_observed: request.egress_policy_observed,
            kill_switch_observed: request.kill_switch_observed,
            health_probe_observed: request.health_probe_observed,
            tenant_fairness_observed: request.tenant_fairness_observed,
            backpressure_observed: request.backpressure_observed,
            ready_p95_observed: request.ready_p95_observed,
            scale_capacity_observed: request.scale_capacity_observed,
            session_ready: !rejected,
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            session_decision: session_decision.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let events = sandbox_session_events(&session);
        let body = render_sandbox_session_response_json(&session);
        self.sessions.push(session);
        Ok(DxrSandboxSessionResult { body, events })
    }

    pub fn sessions_json(&self) -> String {
        let ready_count = self
            .sessions
            .iter()
            .filter(|session| session.session_ready && !session.rejected)
            .count();
        let rejected_count = self.sessions.len().saturating_sub(ready_count);
        format!(
            r#"{{"name":"mdx-dxr-sandbox-session-leases","status":"LIVE-LOCAL-DXR-SANDBOX-SESSION-LEASE-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/sandbox-sessions.json","submit_route":"/v1/dxr/sandbox-sessions","session_count":{},"ready_count":{},"rejected_count":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"requested_session_count":{},"active_session_capacity":{},"queued_session_count":{},"warm_pool_target":{},"session_batch_size":{},"session_batch_count":{},"ready_p95_ms_ceiling":{},"lease_duration_ms":{},"idle_timeout_ms":{},"restart_replay_window_ms":{},"session_policy":"leased_session_before_sandbox_process_or_command_start","pool_policy":"warm_pool_with_backpressure_and_resume_tokens","runner_required":true,"sandbox_authority_required":true,"session_identity_required":true,"lease_required":true,"warm_pool_slot_required":true,"attach_handle_required":true,"detach_handle_required":true,"resume_token_required":true,"terminal_websocket_required":true,"command_stream_required":true,"log_stream_required":true,"artifact_stream_required":true,"filesystem_watch_required":true,"preview_service_required":true,"restart_replay_required":true,"state_loss_reinit_required":true,"version_compatibility_required":true,"resource_limits_required":true,"network_policy_required":true,"secret_policy_required":true,"egress_policy_required":true,"kill_switch_required":true,"health_probe_required":true,"tenant_fairness_required":true,"backpressure_required":true,"ready_p95_required":true,"scale_capacity_required":true,"worker_process_started":false,"sandbox_process_started":false,"sandbox_command_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"sessions":[{}]}}"#,
            self.sessions.len(),
            ready_count,
            rejected_count,
            self.sessions
                .last()
                .map(|session| session.target_concurrent_engineers)
                .unwrap_or(DEFAULT_TARGET_ENGINEERS),
            self.sessions
                .last()
                .map(|session| session.peak_parallel_forge_builds)
                .unwrap_or(DEFAULT_PEAK_PARALLEL_FORGE_BUILDS),
            self.sessions
                .last()
                .map(|session| session.requested_session_count)
                .unwrap_or(DEFAULT_REQUESTED_SESSION_COUNT),
            self.sessions
                .last()
                .map(|session| session.active_session_capacity)
                .unwrap_or(DEFAULT_ACTIVE_SESSION_CAPACITY),
            self.sessions
                .last()
                .map(|session| session.queued_session_count)
                .unwrap_or(DEFAULT_REQUESTED_SESSION_COUNT - DEFAULT_ACTIVE_SESSION_CAPACITY),
            self.sessions
                .last()
                .map(|session| session.warm_pool_target)
                .unwrap_or(DEFAULT_WARM_POOL_TARGET),
            self.sessions
                .last()
                .map(|session| session.session_batch_size)
                .unwrap_or(DEFAULT_SESSION_BATCH_SIZE),
            self.sessions
                .last()
                .map(|session| session.session_batch_count)
                .unwrap_or(ceil_div(
                    DEFAULT_REQUESTED_SESSION_COUNT,
                    DEFAULT_SESSION_BATCH_SIZE
                )),
            self.sessions
                .last()
                .map(|session| session.ready_p95_ms_ceiling)
                .unwrap_or(DEFAULT_READY_P95_MS),
            self.sessions
                .last()
                .map(|session| session.lease_duration_ms)
                .unwrap_or(DEFAULT_LEASE_DURATION_MS),
            self.sessions
                .last()
                .map(|session| session.idle_timeout_ms)
                .unwrap_or(DEFAULT_IDLE_TIMEOUT_MS),
            self.sessions
                .last()
                .map(|session| session.restart_replay_window_ms)
                .unwrap_or(DEFAULT_RESTART_REPLAY_WINDOW_MS),
            self.sessions
                .iter()
                .map(render_sandbox_session_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn parse_sandbox_session_request(body: &str) -> Result<SandboxSessionRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR sandbox session json: {error}"))?;
    Ok(SandboxSessionRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", "dxr_job_sandbox_session_001"),
        run_id: string_value(&value, "run_id", "dxr_run_sandbox_session_001"),
        worker_run_id: string_value(&value, "worker_run_id", "dxr_worker_run_session_001"),
        idempotency_key: string_value(&value, "idempotency_key", ""),
        runner_id: string_value(&value, "runner_id", ""),
        runner_status: string_value(&value, "runner_status", ""),
        runner_provider: string_value(&value, "runner_provider", "local_docker"),
        runner_driver: string_value(&value, "runner_driver", "docker_exec_no_network"),
        runner_region: string_value(&value, "runner_region", "local"),
        sandbox_handoff_id: string_value(&value, "sandbox_handoff_id", ""),
        sandbox_authority_envelope_id: string_value(&value, "sandbox_authority_envelope_id", ""),
        execution_schedule_id: string_value(&value, "execution_schedule_id", ""),
        execution_supervision_id: string_value(&value, "execution_supervision_id", ""),
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
        evidence_chain_receipt_id: string_value(&value, "evidence_chain_receipt_id", ""),
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
        requested_session_count: usize_value(
            &value,
            "requested_session_count",
            DEFAULT_REQUESTED_SESSION_COUNT,
        ),
        active_session_capacity: usize_value(
            &value,
            "active_session_capacity",
            DEFAULT_ACTIVE_SESSION_CAPACITY,
        ),
        warm_pool_target: usize_value(&value, "warm_pool_target", DEFAULT_WARM_POOL_TARGET),
        session_batch_size: usize_value(&value, "session_batch_size", DEFAULT_SESSION_BATCH_SIZE),
        ready_p95_ms: usize_value(&value, "ready_p95_ms", DEFAULT_READY_P95_MS),
        ready_p95_ms_ceiling: usize_value(&value, "ready_p95_ms_ceiling", DEFAULT_READY_P95_MS),
        lease_duration_ms: usize_value(&value, "lease_duration_ms", DEFAULT_LEASE_DURATION_MS),
        heartbeat_interval_ms: usize_value(
            &value,
            "heartbeat_interval_ms",
            DEFAULT_HEARTBEAT_INTERVAL_MS,
        ),
        idle_timeout_ms: usize_value(&value, "idle_timeout_ms", DEFAULT_IDLE_TIMEOUT_MS),
        restart_replay_window_ms: usize_value(
            &value,
            "restart_replay_window_ms",
            DEFAULT_RESTART_REPLAY_WINDOW_MS,
        ),
        cpu_millicores: usize_value(&value, "cpu_millicores", DEFAULT_CPU_MILLICORES),
        memory_mb: usize_value(&value, "memory_mb", DEFAULT_MEMORY_MB),
        disk_mb: usize_value(&value, "disk_mb", DEFAULT_DISK_MB),
        runner_ready_observed: bool_value(&value, "runner_ready_observed", false),
        session_identity_observed: bool_value(&value, "session_identity_observed", false),
        lease_observed: bool_value(&value, "lease_observed", false),
        warm_pool_slot_observed: bool_value(&value, "warm_pool_slot_observed", false),
        attach_handle_observed: bool_value(&value, "attach_handle_observed", false),
        detach_handle_observed: bool_value(&value, "detach_handle_observed", false),
        resume_token_observed: bool_value(&value, "resume_token_observed", false),
        terminal_websocket_observed: bool_value(&value, "terminal_websocket_observed", false),
        command_stream_observed: bool_value(&value, "command_stream_observed", false),
        log_stream_observed: bool_value(&value, "log_stream_observed", false),
        artifact_stream_observed: bool_value(&value, "artifact_stream_observed", false),
        filesystem_watch_observed: bool_value(&value, "filesystem_watch_observed", false),
        preview_service_observed: bool_value(&value, "preview_service_observed", false),
        restart_replay_observed: bool_value(&value, "restart_replay_observed", false),
        state_loss_reinit_observed: bool_value(&value, "state_loss_reinit_observed", false),
        version_compatibility_observed: bool_value(&value, "version_compatibility_observed", false),
        resource_limits_observed: bool_value(&value, "resource_limits_observed", false),
        network_policy_observed: bool_value(&value, "network_policy_observed", false),
        secret_policy_observed: bool_value(&value, "secret_policy_observed", false),
        egress_policy_observed: bool_value(&value, "egress_policy_observed", false),
        kill_switch_observed: bool_value(&value, "kill_switch_observed", false),
        health_probe_observed: bool_value(&value, "health_probe_observed", false),
        tenant_fairness_observed: bool_value(&value, "tenant_fairness_observed", false),
        backpressure_observed: bool_value(&value, "backpressure_observed", false),
        ready_p95_observed: bool_value(&value, "ready_p95_observed", false),
        scale_capacity_observed: bool_value(&value, "scale_capacity_observed", false),
        worker_process_start_requested: bool_value(&value, "worker_process_start_requested", false),
        sandbox_process_start_requested: bool_value(
            &value,
            "sandbox_process_start_requested",
            false,
        ),
        sandbox_command_requested: bool_value(&value, "sandbox_command_requested", false),
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

fn session_provider_is_supported(request: &SandboxSessionRequest) -> bool {
    [
        ("local_docker", "docker_exec_no_network"),
        ("firecracker_microvm", "microvm_jailer_netns"),
        ("cloudflare_sandbox", "workers_container_sandbox"),
        ("modal_sandbox", "modal_sandbox_exec"),
        ("e2b_sandbox", "e2b_secure_vm"),
        ("codex_cloud_sandbox", "remote_isolated_repo_runner"),
    ]
    .iter()
    .any(|(provider, driver)| {
        *provider == request.runner_provider && *driver == request.runner_driver
    })
}

fn sandbox_session_events(session: &DxrSandboxSession) -> Vec<DxrSandboxSessionRuntimeEvent> {
    let mut events = vec![sandbox_session_event(
        session,
        "sandbox_session_lease_recorded",
    )];
    if session.rejected {
        events.push(sandbox_session_event(
            session,
            "sandbox_session_lease_rejected",
        ));
    } else {
        for event_type in [
            "sandbox_session_runner_bound",
            "sandbox_session_identity_bound",
            "sandbox_session_warm_pool_bound",
            "sandbox_session_attach_detach_bound",
            "sandbox_session_resume_token_recorded",
            "sandbox_session_streams_bound",
            "sandbox_session_restart_replay_bound",
            "sandbox_session_resource_limits_bound",
            "sandbox_session_kill_switch_armed",
            "sandbox_session_backpressure_retained",
            "sandbox_session_ready",
        ] {
            events.push(sandbox_session_event(session, event_type));
        }
    }
    events.push(sandbox_session_event(
        session,
        "sandbox_session_execution_blocked",
    ));
    events
}

fn sandbox_session_event(
    session: &DxrSandboxSession,
    event_type: &str,
) -> DxrSandboxSessionRuntimeEvent {
    DxrSandboxSessionRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: session.tenant_id.clone(),
        job_id: session.job_id.clone(),
        run_id: session.run_id.clone(),
        actor_id: session.actor_id.clone(),
    }
}

fn render_sandbox_session_response_json(session: &DxrSandboxSession) -> String {
    format!(
        r#"{{"name":"mdx-dxr-sandbox-session-lease","status":{},"runtime":"mdx-dxr-engine","session":{},"session_id":{},"session_lease_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"terminal_state":{},"session_decision":{},"v1_stage":"sandbox_session_lease","session_ready":{},"rejected":{},"rejection_reason":{},"runner_required":true,"sandbox_authority_required":true,"session_identity_required":true,"lease_required":true,"warm_pool_slot_required":true,"attach_handle_required":true,"detach_handle_required":true,"resume_token_required":true,"terminal_websocket_required":true,"command_stream_required":true,"log_stream_required":true,"artifact_stream_required":true,"filesystem_watch_required":true,"preview_service_required":true,"restart_replay_required":true,"state_loss_reinit_required":true,"version_compatibility_required":true,"resource_limits_required":true,"network_policy_required":true,"secret_policy_required":true,"egress_policy_required":true,"kill_switch_required":true,"health_probe_required":true,"tenant_fairness_required":true,"backpressure_required":true,"ready_p95_required":true,"scale_capacity_required":true,"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"requested_session_count":{},"active_session_capacity":{},"queued_session_count":{},"warm_pool_target":{},"session_batch_size":{},"session_batch_count":{},"ready_p95_ms":{},"ready_p95_ms_ceiling":{},"worker_process_started":false,"sandbox_process_started":false,"sandbox_command_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&session.status),
        render_sandbox_session_json(session),
        json_string_literal(&session.session_id),
        json_string_literal(&session.session_lease_id),
        json_string_literal(&session.tenant_id),
        json_string_literal(&session.actor_id),
        json_string_literal(&session.job_id),
        json_string_literal(&session.run_id),
        json_string_literal(&session.worker_run_id),
        json_string_literal(&session.terminal_state),
        json_string_literal(&session.session_decision),
        session.session_ready,
        session.rejected,
        json_string_literal(&session.rejection_reason),
        session.target_concurrent_engineers,
        session.peak_parallel_forge_builds,
        session.requested_session_count,
        session.active_session_capacity,
        session.queued_session_count,
        session.warm_pool_target,
        session.session_batch_size,
        session.session_batch_count,
        session.ready_p95_ms,
        session.ready_p95_ms_ceiling
    )
}

fn render_sandbox_session_json(session: &DxrSandboxSession) -> String {
    format!(
        r#"{{"sequence":{},"session_id":{},"session_lease_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"idempotency_key":{},"runner_id":{},"runner_status":{},"runner_provider":{},"runner_driver":{},"runner_region":{},"sandbox_handoff_id":{},"sandbox_authority_envelope_id":{},"execution_schedule_id":{},"execution_supervision_id":{},"dispatch_claim_id":{},"heartbeat_receipt_id":{},"durable_workflow_receipt_id":{},"external_sandbox_preflight_id":{},"sandbox_adapter_turn_on_receipt_id":{},"ctx_context_input_id":{},"evidence_chain_receipt_id":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"requested_session_count":{},"active_session_capacity":{},"queued_session_count":{},"warm_pool_target":{},"session_batch_size":{},"session_batch_count":{},"ready_p95_ms":{},"ready_p95_ms_ceiling":{},"lease_duration_ms":{},"heartbeat_interval_ms":{},"idle_timeout_ms":{},"restart_replay_window_ms":{},"cpu_millicores":{},"memory_mb":{},"disk_mb":{},"runner_ready_observed":{},"session_identity_observed":{},"lease_observed":{},"warm_pool_slot_observed":{},"attach_handle_observed":{},"detach_handle_observed":{},"resume_token_observed":{},"terminal_websocket_observed":{},"command_stream_observed":{},"log_stream_observed":{},"artifact_stream_observed":{},"filesystem_watch_observed":{},"preview_service_observed":{},"restart_replay_observed":{},"state_loss_reinit_observed":{},"version_compatibility_observed":{},"resource_limits_observed":{},"network_policy_observed":{},"secret_policy_observed":{},"egress_policy_observed":{},"kill_switch_observed":{},"health_probe_observed":{},"tenant_fairness_observed":{},"backpressure_observed":{},"ready_p95_observed":{},"scale_capacity_observed":{},"session_ready":{},"status":{},"terminal_state":{},"session_decision":{},"rejected":{},"rejection_reason":{},"worker_process_started":false,"sandbox_process_started":false,"sandbox_command_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        session.sequence,
        json_string_literal(&session.session_id),
        json_string_literal(&session.session_lease_id),
        json_string_literal(&session.tenant_id),
        json_string_literal(&session.actor_id),
        json_string_literal(&session.job_id),
        json_string_literal(&session.run_id),
        json_string_literal(&session.worker_run_id),
        json_string_literal(&session.idempotency_key),
        json_string_literal(&session.runner_id),
        json_string_literal(&session.runner_status),
        json_string_literal(&session.runner_provider),
        json_string_literal(&session.runner_driver),
        json_string_literal(&session.runner_region),
        json_string_literal(&session.sandbox_handoff_id),
        json_string_literal(&session.sandbox_authority_envelope_id),
        json_string_literal(&session.execution_schedule_id),
        json_string_literal(&session.execution_supervision_id),
        json_string_literal(&session.dispatch_claim_id),
        json_string_literal(&session.heartbeat_receipt_id),
        json_string_literal(&session.durable_workflow_receipt_id),
        json_string_literal(&session.external_sandbox_preflight_id),
        json_string_literal(&session.sandbox_adapter_turn_on_receipt_id),
        json_string_literal(&session.ctx_context_input_id),
        json_string_literal(&session.evidence_chain_receipt_id),
        session.target_concurrent_engineers,
        session.peak_parallel_forge_builds,
        session.requested_session_count,
        session.active_session_capacity,
        session.queued_session_count,
        session.warm_pool_target,
        session.session_batch_size,
        session.session_batch_count,
        session.ready_p95_ms,
        session.ready_p95_ms_ceiling,
        session.lease_duration_ms,
        session.heartbeat_interval_ms,
        session.idle_timeout_ms,
        session.restart_replay_window_ms,
        session.cpu_millicores,
        session.memory_mb,
        session.disk_mb,
        session.runner_ready_observed,
        session.session_identity_observed,
        session.lease_observed,
        session.warm_pool_slot_observed,
        session.attach_handle_observed,
        session.detach_handle_observed,
        session.resume_token_observed,
        session.terminal_websocket_observed,
        session.command_stream_observed,
        session.log_stream_observed,
        session.artifact_stream_observed,
        session.filesystem_watch_observed,
        session.preview_service_observed,
        session.restart_replay_observed,
        session.state_loss_reinit_observed,
        session.version_compatibility_observed,
        session.resource_limits_observed,
        session.network_policy_observed,
        session.secret_policy_observed,
        session.egress_policy_observed,
        session.kill_switch_observed,
        session.health_probe_observed,
        session.tenant_fairness_observed,
        session.backpressure_observed,
        session.ready_p95_observed,
        session.scale_capacity_observed,
        session.session_ready,
        json_string_literal(&session.status),
        json_string_literal(&session.terminal_state),
        json_string_literal(&session.session_decision),
        session.rejected,
        json_string_literal(&session.rejection_reason)
    )
}

fn ceil_div(value: usize, divisor: usize) -> usize {
    if divisor == 0 {
        0
    } else {
        value.div_ceil(divisor)
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
    fn sandbox_session_lease_records_floor_and_rejects_authority() {
        let mut runtime = DxrSandboxSessionRuntime::new();
        let accepted = runtime
            .submit_json(
                r#"{"idempotency_key":"session-001","runner_id":"dxr_supervised_sandbox_runner_000001","runner_status":"LIVE-LOCAL-DXR-SUPERVISED-SANDBOX-RUNNER-FLOOR","runner_provider":"cloudflare_sandbox","runner_driver":"workers_container_sandbox","runner_region":"iad","sandbox_handoff_id":"dxr_supervised_sandbox_handoff_000001","sandbox_authority_envelope_id":"dxr_sandbox_authority_envelope_000001","execution_schedule_id":"dxr_execution_schedule_000001","execution_supervision_id":"dxr_execution_supervision_000001","dispatch_claim_id":"dxr_dispatch_claim_000001","heartbeat_receipt_id":"dxr_dispatch_claim_000001","durable_workflow_receipt_id":"dxr_workflow_run_000001","external_sandbox_preflight_id":"dxr_external_sandbox_preflight_000001","sandbox_adapter_turn_on_receipt_id":"dxr_sandbox_adapter_turn_on_000001","ctx_context_input_id":"dxr_ctx_context_input_000001","evidence_chain_receipt_id":"dxr_evidence_record_000001","runner_ready_observed":true,"session_identity_observed":true,"lease_observed":true,"warm_pool_slot_observed":true,"attach_handle_observed":true,"detach_handle_observed":true,"resume_token_observed":true,"terminal_websocket_observed":true,"command_stream_observed":true,"log_stream_observed":true,"artifact_stream_observed":true,"filesystem_watch_observed":true,"preview_service_observed":true,"restart_replay_observed":true,"state_loss_reinit_observed":true,"version_compatibility_observed":true,"resource_limits_observed":true,"network_policy_observed":true,"secret_policy_observed":true,"egress_policy_observed":true,"kill_switch_observed":true,"health_probe_observed":true,"tenant_fairness_observed":true,"backpressure_observed":true,"ready_p95_observed":true,"scale_capacity_observed":true}"#,
            )
            .expect("accepted sandbox session");
        assert!(
            accepted
                .body
                .contains("LIVE-LOCAL-DXR-SANDBOX-SESSION-LEASE-FLOOR")
        );
        assert!(
            accepted
                .body
                .contains("DXR_SANDBOX_SESSION_LEASE_RECORDED_READY_EXECUTION_BLOCKED")
        );
        assert!(accepted.body.contains(r#""sandbox_command_started":false"#));
        assert!(
            accepted
                .events
                .iter()
                .any(|event| event.event_type == "sandbox_session_ready")
        );

        let rejected = runtime
            .submit_json(
                r#"{"idempotency_key":"session-002","runner_id":"dxr_supervised_sandbox_runner_000001","runner_status":"LIVE-LOCAL-DXR-SUPERVISED-SANDBOX-RUNNER-FLOOR","runner_provider":"cloudflare_sandbox","runner_driver":"workers_container_sandbox","runner_region":"iad","sandbox_handoff_id":"dxr_supervised_sandbox_handoff_000001","sandbox_authority_envelope_id":"dxr_sandbox_authority_envelope_000001","execution_schedule_id":"dxr_execution_schedule_000001","execution_supervision_id":"dxr_execution_supervision_000001","dispatch_claim_id":"dxr_dispatch_claim_000001","heartbeat_receipt_id":"dxr_dispatch_claim_000001","durable_workflow_receipt_id":"dxr_workflow_run_000001","external_sandbox_preflight_id":"dxr_external_sandbox_preflight_000001","sandbox_adapter_turn_on_receipt_id":"dxr_sandbox_adapter_turn_on_000001","ctx_context_input_id":"dxr_ctx_context_input_000001","evidence_chain_receipt_id":"dxr_evidence_record_000001","runner_ready_observed":true,"session_identity_observed":true,"lease_observed":true,"warm_pool_slot_observed":true,"attach_handle_observed":true,"detach_handle_observed":true,"resume_token_observed":true,"terminal_websocket_observed":true,"command_stream_observed":true,"log_stream_observed":true,"artifact_stream_observed":true,"filesystem_watch_observed":true,"preview_service_observed":true,"restart_replay_observed":true,"state_loss_reinit_observed":true,"version_compatibility_observed":true,"resource_limits_observed":true,"network_policy_observed":true,"secret_policy_observed":true,"egress_policy_observed":true,"kill_switch_observed":true,"health_probe_observed":true,"tenant_fairness_observed":true,"backpressure_observed":true,"ready_p95_observed":true,"scale_capacity_observed":true,"sandbox_command_requested":true,"network_requested":true}"#,
            )
            .expect("rejected sandbox session");
        assert!(
            rejected
                .body
                .contains("DXR_SANDBOX_SESSION_LEASE_REJECTED_SECURITY_BOUNDARY")
        );
        assert!(runtime.sessions_json().contains(r#""session_count":2"#));
        assert!(runtime.sessions_json().contains(r#""ready_count":1"#));
        assert!(runtime.sessions_json().contains(r#""rejected_count":1"#));
    }
}
