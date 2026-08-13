use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_TARGET_ENGINEERS: usize = 1000;
const DEFAULT_PEAK_PARALLEL_FORGE_BUILDS: usize = 5000;
const DEFAULT_MAX_PARALLEL_SANDBOXES: usize = 5000;
const DEFAULT_WARM_POOL_TARGET: usize = 2048;
const DEFAULT_READY_P95_MS: usize = 2500;
const DEFAULT_MAX_RUNTIME_MS: usize = 300_000;
const DEFAULT_IDLE_TIMEOUT_MS: usize = 60_000;
const DEFAULT_CPU_MILLICORES: usize = 2000;
const DEFAULT_MEMORY_MB: usize = 4096;
const DEFAULT_DISK_MB: usize = 16_384;

#[derive(Default)]
pub struct DxrSandboxRunnerRuntime {
    runners: Vec<DxrSandboxRunner>,
    next_runner: usize,
}

pub struct DxrSandboxRunnerResult {
    pub body: String,
    pub events: Vec<DxrSandboxRunnerRuntimeEvent>,
}

pub struct DxrSandboxRunnerRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrSandboxRunner {
    sequence: usize,
    runner_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    idempotency_key: String,
    sandbox_handoff_id: String,
    sandbox_handoff_status: String,
    sandbox_authority_envelope_id: String,
    sandbox_authority_receipt_id: String,
    runner_provider: String,
    runner_driver: String,
    runner_region: String,
    image_ref: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    max_parallel_sandboxes: usize,
    warm_pool_target: usize,
    ready_p95_ms: usize,
    ready_p95_ms_ceiling: usize,
    max_runtime_ms: usize,
    idle_timeout_ms: usize,
    cpu_millicores: usize,
    memory_mb: usize,
    disk_mb: usize,
    command_stream_count: usize,
    runner_provider_registered: bool,
    sandbox_handoff_observed: bool,
    sandbox_authority_observed: bool,
    isolation_boundary_observed: bool,
    snapshot_restore_observed: bool,
    filesystem_watch_observed: bool,
    terminal_websocket_observed: bool,
    command_stream_observed: bool,
    log_stream_observed: bool,
    artifact_sink_observed: bool,
    suspend_resume_observed: bool,
    preview_service_observed: bool,
    resource_limits_observed: bool,
    network_policy_observed: bool,
    secret_policy_observed: bool,
    egress_policy_observed: bool,
    kill_switch_observed: bool,
    health_probe_observed: bool,
    ready_p95_observed: bool,
    scale_capacity_observed: bool,
    runner_ready: bool,
    status: String,
    terminal_state: String,
    runner_decision: String,
    rejected: bool,
    rejection_reason: String,
}

struct SandboxRunnerRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    idempotency_key: String,
    sandbox_handoff_id: String,
    sandbox_handoff_status: String,
    sandbox_authority_envelope_id: String,
    sandbox_authority_receipt_id: String,
    runner_provider: String,
    runner_driver: String,
    runner_region: String,
    image_ref: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    max_parallel_sandboxes: usize,
    warm_pool_target: usize,
    ready_p95_ms: usize,
    ready_p95_ms_ceiling: usize,
    max_runtime_ms: usize,
    idle_timeout_ms: usize,
    cpu_millicores: usize,
    memory_mb: usize,
    disk_mb: usize,
    command_stream_count: usize,
    runner_provider_registered: bool,
    sandbox_handoff_observed: bool,
    sandbox_authority_observed: bool,
    isolation_boundary_observed: bool,
    snapshot_restore_observed: bool,
    filesystem_watch_observed: bool,
    terminal_websocket_observed: bool,
    command_stream_observed: bool,
    log_stream_observed: bool,
    artifact_sink_observed: bool,
    suspend_resume_observed: bool,
    preview_service_observed: bool,
    resource_limits_observed: bool,
    network_policy_observed: bool,
    secret_policy_observed: bool,
    egress_policy_observed: bool,
    kill_switch_observed: bool,
    health_probe_observed: bool,
    ready_p95_observed: bool,
    scale_capacity_observed: bool,
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

impl DxrSandboxRunnerRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrSandboxRunnerResult, String> {
        let request = parse_sandbox_runner_request(body)?;
        self.next_runner += 1;

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
        let provider_registered =
            request.runner_provider_registered && runner_provider_is_supported(&request);
        let missing_evidence = request.idempotency_key.trim().is_empty()
            || request.sandbox_handoff_id.trim().is_empty()
            || request.sandbox_handoff_status.trim().is_empty()
            || request.sandbox_handoff_status.contains("REJECTED")
            || request.sandbox_authority_envelope_id.trim().is_empty()
            || request.sandbox_authority_receipt_id.trim().is_empty()
            || request.image_ref.trim().is_empty()
            || !provider_registered
            || !request.sandbox_handoff_observed
            || !request.sandbox_authority_observed
            || !request.isolation_boundary_observed
            || !request.snapshot_restore_observed
            || !request.filesystem_watch_observed
            || !request.terminal_websocket_observed
            || !request.command_stream_observed
            || !request.log_stream_observed
            || !request.artifact_sink_observed
            || !request.suspend_resume_observed
            || !request.preview_service_observed
            || !request.resource_limits_observed
            || !request.network_policy_observed
            || !request.secret_policy_observed
            || !request.egress_policy_observed
            || !request.kill_switch_observed
            || !request.health_probe_observed
            || !request.ready_p95_observed
            || !request.scale_capacity_observed
            || request.max_parallel_sandboxes < request.peak_parallel_forge_builds
            || request.warm_pool_target == 0
            || request.ready_p95_ms > request.ready_p95_ms_ceiling
            || request.max_runtime_ms == 0
            || request.idle_timeout_ms == 0
            || request.cpu_millicores == 0
            || request.memory_mb == 0
            || request.disk_mb == 0
            || request.command_stream_count == 0;

        let (status, terminal_state, runner_decision, rejected, rejection_reason) =
            if authority_requested {
                (
                    "DXR_SUPERVISED_SANDBOX_RUNNER_REJECTED_SECURITY_BOUNDARY",
                    "DXR_SUPERVISED_SANDBOX_RUNNER_RECORDED_EXECUTION_BLOCKED",
                    "rejected_runner_execution_authority_requested",
                    true,
                    "runner_cannot_start_worker_sandbox_checkout_provider_tool_shell_patch_git_network_secret_filesystem_ci_deployment_or_production_writes",
                )
            } else if missing_evidence {
                (
                    "DXR_SUPERVISED_SANDBOX_RUNNER_REJECTED_MISSING_EVIDENCE",
                    "DXR_SUPERVISED_SANDBOX_RUNNER_RECORDED_EXECUTION_BLOCKED",
                    "rejected_missing_runner_evidence",
                    true,
                    "handoff_provider_lifecycle_stream_resource_policy_kill_switch_health_or_scale_evidence_missing",
                )
            } else {
                (
                    "LIVE-LOCAL-DXR-SUPERVISED-SANDBOX-RUNNER-FLOOR",
                    "DXR_SUPERVISED_SANDBOX_RUNNER_RECORDED_READY_EXECUTION_BLOCKED",
                    "runner_ready_execution_blocked",
                    false,
                    "none",
                )
            };

        let runner = DxrSandboxRunner {
            sequence: self.next_runner,
            runner_id: format!("dxr_supervised_sandbox_runner_{:06}", self.next_runner),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            worker_run_id: request.worker_run_id,
            idempotency_key: request.idempotency_key,
            sandbox_handoff_id: request.sandbox_handoff_id,
            sandbox_handoff_status: request.sandbox_handoff_status,
            sandbox_authority_envelope_id: request.sandbox_authority_envelope_id,
            sandbox_authority_receipt_id: request.sandbox_authority_receipt_id,
            runner_provider: request.runner_provider,
            runner_driver: request.runner_driver,
            runner_region: request.runner_region,
            image_ref: request.image_ref,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            max_parallel_sandboxes: request.max_parallel_sandboxes,
            warm_pool_target: request.warm_pool_target,
            ready_p95_ms: request.ready_p95_ms,
            ready_p95_ms_ceiling: request.ready_p95_ms_ceiling,
            max_runtime_ms: request.max_runtime_ms,
            idle_timeout_ms: request.idle_timeout_ms,
            cpu_millicores: request.cpu_millicores,
            memory_mb: request.memory_mb,
            disk_mb: request.disk_mb,
            command_stream_count: request.command_stream_count,
            runner_provider_registered: provider_registered,
            sandbox_handoff_observed: request.sandbox_handoff_observed,
            sandbox_authority_observed: request.sandbox_authority_observed,
            isolation_boundary_observed: request.isolation_boundary_observed,
            snapshot_restore_observed: request.snapshot_restore_observed,
            filesystem_watch_observed: request.filesystem_watch_observed,
            terminal_websocket_observed: request.terminal_websocket_observed,
            command_stream_observed: request.command_stream_observed,
            log_stream_observed: request.log_stream_observed,
            artifact_sink_observed: request.artifact_sink_observed,
            suspend_resume_observed: request.suspend_resume_observed,
            preview_service_observed: request.preview_service_observed,
            resource_limits_observed: request.resource_limits_observed,
            network_policy_observed: request.network_policy_observed,
            secret_policy_observed: request.secret_policy_observed,
            egress_policy_observed: request.egress_policy_observed,
            kill_switch_observed: request.kill_switch_observed,
            health_probe_observed: request.health_probe_observed,
            ready_p95_observed: request.ready_p95_observed,
            scale_capacity_observed: request.scale_capacity_observed,
            runner_ready: !rejected,
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            runner_decision: runner_decision.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let events = sandbox_runner_events(&runner);
        let body = render_sandbox_runner_response_json(&runner);
        self.runners.push(runner);
        Ok(DxrSandboxRunnerResult { body, events })
    }

    pub fn runners_json(&self) -> String {
        let ready_count = self
            .runners
            .iter()
            .filter(|runner| runner.runner_ready && !runner.rejected)
            .count();
        let rejected_count = self.runners.len().saturating_sub(ready_count);
        format!(
            r#"{{"name":"mdx-dxr-supervised-sandbox-runners","status":"LIVE-LOCAL-DXR-SUPERVISED-SANDBOX-RUNNER-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/supervised-sandbox-runners.json","submit_route":"/v1/dxr/supervised-sandbox-runners","runner_count":{},"ready_count":{},"rejected_count":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"max_parallel_sandboxes":{},"warm_pool_target":{},"ready_p95_ms_ceiling":{},"max_runtime_ms":{},"idle_timeout_ms":{},"runner_provider_registry":[{}],"runner_policy":"provider_backed_runner_ready_before_sandbox_start","handoff_required":true,"sandbox_authority_required":true,"isolation_boundary_required":true,"snapshot_restore_required":true,"filesystem_watch_required":true,"terminal_websocket_required":true,"command_stream_required":true,"log_stream_required":true,"artifact_sink_required":true,"suspend_resume_required":true,"preview_service_required":true,"resource_limits_required":true,"network_policy_required":true,"secret_policy_required":true,"egress_policy_required":true,"kill_switch_required":true,"health_probe_required":true,"ready_p95_required":true,"scale_capacity_required":true,"worker_process_started":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"runners":[{}]}}"#,
            self.runners.len(),
            ready_count,
            rejected_count,
            self.runners
                .last()
                .map(|runner| runner.target_concurrent_engineers)
                .unwrap_or(DEFAULT_TARGET_ENGINEERS),
            self.runners
                .last()
                .map(|runner| runner.peak_parallel_forge_builds)
                .unwrap_or(DEFAULT_PEAK_PARALLEL_FORGE_BUILDS),
            self.runners
                .last()
                .map(|runner| runner.max_parallel_sandboxes)
                .unwrap_or(DEFAULT_MAX_PARALLEL_SANDBOXES),
            self.runners
                .last()
                .map(|runner| runner.warm_pool_target)
                .unwrap_or(DEFAULT_WARM_POOL_TARGET),
            self.runners
                .last()
                .map(|runner| runner.ready_p95_ms_ceiling)
                .unwrap_or(DEFAULT_READY_P95_MS),
            self.runners
                .last()
                .map(|runner| runner.max_runtime_ms)
                .unwrap_or(DEFAULT_MAX_RUNTIME_MS),
            self.runners
                .last()
                .map(|runner| runner.idle_timeout_ms)
                .unwrap_or(DEFAULT_IDLE_TIMEOUT_MS),
            render_runner_provider_registry_json(),
            self.runners
                .iter()
                .map(render_sandbox_runner_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn parse_sandbox_runner_request(body: &str) -> Result<SandboxRunnerRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR sandbox runner json: {error}"))?;
    Ok(SandboxRunnerRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", "dxr_job_sandbox_runner_001"),
        run_id: string_value(&value, "run_id", "dxr_run_sandbox_runner_001"),
        worker_run_id: string_value(&value, "worker_run_id", "dxr_worker_run_runner_001"),
        idempotency_key: string_value(&value, "idempotency_key", ""),
        sandbox_handoff_id: string_value(&value, "sandbox_handoff_id", ""),
        sandbox_handoff_status: string_value(&value, "sandbox_handoff_status", ""),
        sandbox_authority_envelope_id: string_value(&value, "sandbox_authority_envelope_id", ""),
        sandbox_authority_receipt_id: string_value(&value, "sandbox_authority_receipt_id", ""),
        runner_provider: string_value(&value, "runner_provider", "local_docker"),
        runner_driver: string_value(&value, "runner_driver", "docker_exec_no_network"),
        runner_region: string_value(&value, "runner_region", "local"),
        image_ref: string_value(&value, "image_ref", ""),
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
            DEFAULT_MAX_PARALLEL_SANDBOXES,
        ),
        warm_pool_target: usize_value(&value, "warm_pool_target", DEFAULT_WARM_POOL_TARGET),
        ready_p95_ms: usize_value(&value, "ready_p95_ms", DEFAULT_READY_P95_MS),
        ready_p95_ms_ceiling: usize_value(&value, "ready_p95_ms_ceiling", DEFAULT_READY_P95_MS),
        max_runtime_ms: usize_value(&value, "max_runtime_ms", DEFAULT_MAX_RUNTIME_MS),
        idle_timeout_ms: usize_value(&value, "idle_timeout_ms", DEFAULT_IDLE_TIMEOUT_MS),
        cpu_millicores: usize_value(&value, "cpu_millicores", DEFAULT_CPU_MILLICORES),
        memory_mb: usize_value(&value, "memory_mb", DEFAULT_MEMORY_MB),
        disk_mb: usize_value(&value, "disk_mb", DEFAULT_DISK_MB),
        command_stream_count: usize_value(&value, "command_stream_count", 1),
        runner_provider_registered: bool_value(&value, "runner_provider_registered", false),
        sandbox_handoff_observed: bool_value(&value, "sandbox_handoff_observed", false),
        sandbox_authority_observed: bool_value(&value, "sandbox_authority_observed", false),
        isolation_boundary_observed: bool_value(&value, "isolation_boundary_observed", false),
        snapshot_restore_observed: bool_value(&value, "snapshot_restore_observed", false),
        filesystem_watch_observed: bool_value(&value, "filesystem_watch_observed", false),
        terminal_websocket_observed: bool_value(&value, "terminal_websocket_observed", false),
        command_stream_observed: bool_value(&value, "command_stream_observed", false),
        log_stream_observed: bool_value(&value, "log_stream_observed", false),
        artifact_sink_observed: bool_value(&value, "artifact_sink_observed", false),
        suspend_resume_observed: bool_value(&value, "suspend_resume_observed", false),
        preview_service_observed: bool_value(&value, "preview_service_observed", false),
        resource_limits_observed: bool_value(&value, "resource_limits_observed", false),
        network_policy_observed: bool_value(&value, "network_policy_observed", false),
        secret_policy_observed: bool_value(&value, "secret_policy_observed", false),
        egress_policy_observed: bool_value(&value, "egress_policy_observed", false),
        kill_switch_observed: bool_value(&value, "kill_switch_observed", false),
        health_probe_observed: bool_value(&value, "health_probe_observed", false),
        ready_p95_observed: bool_value(&value, "ready_p95_observed", false),
        scale_capacity_observed: bool_value(&value, "scale_capacity_observed", false),
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

fn runner_provider_is_supported(request: &SandboxRunnerRequest) -> bool {
    runner_provider_specs().iter().any(|spec| {
        spec.provider == request.runner_provider && spec.driver == request.runner_driver
    })
}

fn runner_provider_specs() -> Vec<RunnerProviderSpec> {
    vec![
        RunnerProviderSpec {
            provider: "local_docker",
            driver: "docker_exec_no_network",
            isolation: "local_container_userns_cgroup_readonly_secretless",
            stream: "stdout_stderr_terminal_websocket",
        },
        RunnerProviderSpec {
            provider: "firecracker_microvm",
            driver: "microvm_jailer_netns",
            isolation: "per_run_microvm_snapshot_restore",
            stream: "serial_console_and_artifact_stream",
        },
        RunnerProviderSpec {
            provider: "cloudflare_sandbox",
            driver: "workers_container_sandbox",
            isolation: "edge_container_with_file_watch_terminal_ws",
            stream: "worker_terminal_websocket_and_logs",
        },
        RunnerProviderSpec {
            provider: "modal_sandbox",
            driver: "modal_sandbox_exec",
            isolation: "managed_container_sandbox_with_snapshots",
            stream: "modal_stdout_stderr_file_stream",
        },
        RunnerProviderSpec {
            provider: "e2b_sandbox",
            driver: "e2b_secure_vm",
            isolation: "secure_linux_vm_for_agent_code",
            stream: "vm_command_and_artifact_stream",
        },
        RunnerProviderSpec {
            provider: "codex_cloud_sandbox",
            driver: "remote_isolated_repo_runner",
            isolation: "managed_agent_worktree_sandbox",
            stream: "remote_runner_event_stream",
        },
    ]
}

struct RunnerProviderSpec {
    provider: &'static str,
    driver: &'static str,
    isolation: &'static str,
    stream: &'static str,
}

fn render_runner_provider_registry_json() -> String {
    runner_provider_specs()
        .iter()
        .map(|spec| {
            format!(
                r#"{{"provider":{},"driver":{},"isolation":{},"stream":{},"sandbox_process_started":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"production_writes_allowed":false}}"#,
                json_string_literal(spec.provider),
                json_string_literal(spec.driver),
                json_string_literal(spec.isolation),
                json_string_literal(spec.stream)
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn sandbox_runner_events(runner: &DxrSandboxRunner) -> Vec<DxrSandboxRunnerRuntimeEvent> {
    let mut events = vec![sandbox_runner_event(
        runner,
        "supervised_sandbox_runner_recorded",
    )];
    if runner.rejected {
        events.push(sandbox_runner_event(
            runner,
            "supervised_sandbox_runner_rejected",
        ));
    } else {
        for event_type in [
            "supervised_sandbox_runner_provider_bound",
            "supervised_sandbox_runner_handoff_bound",
            "supervised_sandbox_runner_streams_bound",
            "supervised_sandbox_runner_lifecycle_bound",
            "supervised_sandbox_runner_resource_limits_bound",
            "supervised_sandbox_runner_kill_switch_armed",
            "supervised_sandbox_runner_ready",
        ] {
            events.push(sandbox_runner_event(runner, event_type));
        }
    }
    events.push(sandbox_runner_event(
        runner,
        "supervised_sandbox_runner_execution_blocked",
    ));
    events
}

fn sandbox_runner_event(
    runner: &DxrSandboxRunner,
    event_type: &str,
) -> DxrSandboxRunnerRuntimeEvent {
    DxrSandboxRunnerRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: runner.tenant_id.clone(),
        job_id: runner.job_id.clone(),
        run_id: runner.run_id.clone(),
        actor_id: runner.actor_id.clone(),
    }
}

fn render_sandbox_runner_response_json(runner: &DxrSandboxRunner) -> String {
    format!(
        r#"{{"name":"mdx-dxr-supervised-sandbox-runner","status":{},"runtime":"mdx-dxr-engine","runner":{},"runner_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"terminal_state":{},"runner_decision":{},"v1_stage":"supervised_sandbox_runner","runner_ready":{},"rejected":{},"rejection_reason":{},"sandbox_handoff_required":true,"sandbox_authority_required":true,"runner_provider_registered":{},"isolation_boundary_required":true,"snapshot_restore_required":true,"filesystem_watch_required":true,"terminal_websocket_required":true,"command_stream_required":true,"log_stream_required":true,"artifact_sink_required":true,"suspend_resume_required":true,"preview_service_required":true,"resource_limits_required":true,"network_policy_required":true,"secret_policy_required":true,"egress_policy_required":true,"kill_switch_required":true,"health_probe_required":true,"ready_p95_required":true,"scale_capacity_required":true,"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"max_parallel_sandboxes":{},"warm_pool_target":{},"ready_p95_ms":{},"ready_p95_ms_ceiling":{},"command_stream_count":{},"worker_process_started":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&runner.status),
        render_sandbox_runner_json(runner),
        json_string_literal(&runner.runner_id),
        json_string_literal(&runner.tenant_id),
        json_string_literal(&runner.actor_id),
        json_string_literal(&runner.job_id),
        json_string_literal(&runner.run_id),
        json_string_literal(&runner.worker_run_id),
        json_string_literal(&runner.terminal_state),
        json_string_literal(&runner.runner_decision),
        runner.runner_ready,
        runner.rejected,
        json_string_literal(&runner.rejection_reason),
        runner.runner_provider_registered,
        runner.target_concurrent_engineers,
        runner.peak_parallel_forge_builds,
        runner.max_parallel_sandboxes,
        runner.warm_pool_target,
        runner.ready_p95_ms,
        runner.ready_p95_ms_ceiling,
        runner.command_stream_count
    )
}

fn render_sandbox_runner_json(runner: &DxrSandboxRunner) -> String {
    format!(
        r#"{{"sequence":{},"runner_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"idempotency_key":{},"sandbox_handoff_id":{},"sandbox_handoff_status":{},"sandbox_authority_envelope_id":{},"sandbox_authority_receipt_id":{},"runner_provider":{},"runner_driver":{},"runner_region":{},"image_ref":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"max_parallel_sandboxes":{},"warm_pool_target":{},"ready_p95_ms":{},"ready_p95_ms_ceiling":{},"max_runtime_ms":{},"idle_timeout_ms":{},"cpu_millicores":{},"memory_mb":{},"disk_mb":{},"command_stream_count":{},"runner_provider_registered":{},"sandbox_handoff_observed":{},"sandbox_authority_observed":{},"isolation_boundary_observed":{},"snapshot_restore_observed":{},"filesystem_watch_observed":{},"terminal_websocket_observed":{},"command_stream_observed":{},"log_stream_observed":{},"artifact_sink_observed":{},"suspend_resume_observed":{},"preview_service_observed":{},"resource_limits_observed":{},"network_policy_observed":{},"secret_policy_observed":{},"egress_policy_observed":{},"kill_switch_observed":{},"health_probe_observed":{},"ready_p95_observed":{},"scale_capacity_observed":{},"runner_ready":{},"status":{},"terminal_state":{},"runner_decision":{},"rejected":{},"rejection_reason":{},"worker_process_started":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        runner.sequence,
        json_string_literal(&runner.runner_id),
        json_string_literal(&runner.tenant_id),
        json_string_literal(&runner.actor_id),
        json_string_literal(&runner.job_id),
        json_string_literal(&runner.run_id),
        json_string_literal(&runner.worker_run_id),
        json_string_literal(&runner.idempotency_key),
        json_string_literal(&runner.sandbox_handoff_id),
        json_string_literal(&runner.sandbox_handoff_status),
        json_string_literal(&runner.sandbox_authority_envelope_id),
        json_string_literal(&runner.sandbox_authority_receipt_id),
        json_string_literal(&runner.runner_provider),
        json_string_literal(&runner.runner_driver),
        json_string_literal(&runner.runner_region),
        json_string_literal(&runner.image_ref),
        runner.target_concurrent_engineers,
        runner.peak_parallel_forge_builds,
        runner.max_parallel_sandboxes,
        runner.warm_pool_target,
        runner.ready_p95_ms,
        runner.ready_p95_ms_ceiling,
        runner.max_runtime_ms,
        runner.idle_timeout_ms,
        runner.cpu_millicores,
        runner.memory_mb,
        runner.disk_mb,
        runner.command_stream_count,
        runner.runner_provider_registered,
        runner.sandbox_handoff_observed,
        runner.sandbox_authority_observed,
        runner.isolation_boundary_observed,
        runner.snapshot_restore_observed,
        runner.filesystem_watch_observed,
        runner.terminal_websocket_observed,
        runner.command_stream_observed,
        runner.log_stream_observed,
        runner.artifact_sink_observed,
        runner.suspend_resume_observed,
        runner.preview_service_observed,
        runner.resource_limits_observed,
        runner.network_policy_observed,
        runner.secret_policy_observed,
        runner.egress_policy_observed,
        runner.kill_switch_observed,
        runner.health_probe_observed,
        runner.ready_p95_observed,
        runner.scale_capacity_observed,
        runner.runner_ready,
        json_string_literal(&runner.status),
        json_string_literal(&runner.terminal_state),
        json_string_literal(&runner.runner_decision),
        runner.rejected,
        json_string_literal(&runner.rejection_reason)
    )
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
    fn supervised_sandbox_runner_records_floor_and_rejects_authority() {
        let mut runtime = DxrSandboxRunnerRuntime::new();
        let accepted = runtime
            .submit_json(
                r#"{"idempotency_key":"runner-001","sandbox_handoff_id":"dxr_supervised_sandbox_handoff_000001","sandbox_handoff_status":"LIVE-LOCAL-DXR-SUPERVISED-SANDBOX-HANDOFF-FLOOR","sandbox_authority_envelope_id":"dxr_sandbox_authority_envelope_000001","sandbox_authority_receipt_id":"dxr_sandbox_authority_receipt_000001","runner_provider":"cloudflare_sandbox","runner_driver":"workers_container_sandbox","runner_region":"iad","image_ref":"ghcr.io/mdx/sandbox-runner:local-proof","runner_provider_registered":true,"sandbox_handoff_observed":true,"sandbox_authority_observed":true,"isolation_boundary_observed":true,"snapshot_restore_observed":true,"filesystem_watch_observed":true,"terminal_websocket_observed":true,"command_stream_observed":true,"log_stream_observed":true,"artifact_sink_observed":true,"suspend_resume_observed":true,"preview_service_observed":true,"resource_limits_observed":true,"network_policy_observed":true,"secret_policy_observed":true,"egress_policy_observed":true,"kill_switch_observed":true,"health_probe_observed":true,"ready_p95_observed":true,"scale_capacity_observed":true}"#,
            )
            .expect("accepted runner");
        assert!(
            accepted
                .body
                .contains("LIVE-LOCAL-DXR-SUPERVISED-SANDBOX-RUNNER-FLOOR")
        );
        assert!(
            accepted
                .body
                .contains("DXR_SUPERVISED_SANDBOX_RUNNER_RECORDED_READY_EXECUTION_BLOCKED")
        );
        assert!(accepted.body.contains(r#""sandbox_process_started":false"#));
        assert!(
            accepted
                .events
                .iter()
                .any(|event| event.event_type == "supervised_sandbox_runner_ready")
        );

        let rejected = runtime
            .submit_json(
                r#"{"idempotency_key":"runner-002","sandbox_handoff_id":"dxr_supervised_sandbox_handoff_000001","sandbox_handoff_status":"LIVE-LOCAL-DXR-SUPERVISED-SANDBOX-HANDOFF-FLOOR","sandbox_authority_envelope_id":"dxr_sandbox_authority_envelope_000001","sandbox_authority_receipt_id":"dxr_sandbox_authority_receipt_000001","runner_provider":"cloudflare_sandbox","runner_driver":"workers_container_sandbox","runner_region":"iad","image_ref":"ghcr.io/mdx/sandbox-runner:local-proof","runner_provider_registered":true,"sandbox_handoff_observed":true,"sandbox_authority_observed":true,"isolation_boundary_observed":true,"snapshot_restore_observed":true,"filesystem_watch_observed":true,"terminal_websocket_observed":true,"command_stream_observed":true,"log_stream_observed":true,"artifact_sink_observed":true,"suspend_resume_observed":true,"preview_service_observed":true,"resource_limits_observed":true,"network_policy_observed":true,"secret_policy_observed":true,"egress_policy_observed":true,"kill_switch_observed":true,"health_probe_observed":true,"ready_p95_observed":true,"scale_capacity_observed":true,"sandbox_process_start_requested":true,"network_requested":true}"#,
            )
            .expect("rejected runner");
        assert!(
            rejected
                .body
                .contains("DXR_SUPERVISED_SANDBOX_RUNNER_REJECTED_SECURITY_BOUNDARY")
        );
        assert!(runtime.runners_json().contains(r#""runner_count":2"#));
        assert!(runtime.runners_json().contains(r#""ready_count":1"#));
        assert!(runtime.runners_json().contains(r#""rejected_count":1"#));
    }
}
