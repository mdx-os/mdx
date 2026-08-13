use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_TARGET_ENGINEERS: usize = 1000;
const DEFAULT_PEAK_PARALLEL_FORGE_BUILDS: usize = 5000;
const DEFAULT_REQUESTED_FORGE_BUILDS: usize = 5000;
const DEFAULT_ACTIVE_TENANT_COUNT: usize = 10;
const DEFAULT_TENANT_BUILD_LIMIT: usize = 500;
const DEFAULT_GLOBAL_BUILD_LIMIT: usize = 5000;
const DEFAULT_SANDBOX_POOL_SIZE: usize = 2048;
const DEFAULT_QUEUE_DEPTH_LIMIT: usize = 50_000;
const DEFAULT_WORKFLOW_SCRIPT_SLOTS: usize = 320;
const DEFAULT_PROVIDER_STREAM_SLOTS: usize = 512;
const DEFAULT_MAX_CONCURRENT_AGENTS_PER_WORKFLOW: usize = 16;
const DEFAULT_MAX_TOTAL_AGENTS_PER_WORKFLOW: usize = 1000;
const DEFAULT_MEDIUM_BUILD_P95_SECONDS: usize = 180;
const DEFAULT_COMPLEX_BUILD_P95_SECONDS: usize = 600;
const DEFAULT_DIRECT_AGENT_LATENCY_RATIO_BPS: usize = 115;
const DEFAULT_ORCHESTRATION_OVERHEAD_MS: usize = 1200;
const DEFAULT_DETERMINISTIC_GATE_OVERHEAD_MS: usize = 900;
const DEFAULT_QUEUE_WAIT_P95_MS: usize = 20;

#[derive(Default)]
pub struct DxrScaleOrchestrationRuntime {
    proofs: Vec<DxrScaleOrchestrationProof>,
    next_proof: usize,
}

pub struct DxrScaleOrchestrationResult {
    pub body: String,
    pub events: Vec<DxrScaleOrchestrationRuntimeEvent>,
}

pub struct DxrScaleOrchestrationRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrScaleOrchestrationProof {
    sequence: usize,
    proof_id: String,
    tenant_id: String,
    actor_id: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    requested_forge_builds: usize,
    accepted_forge_builds: usize,
    immediate_sandbox_reservations: usize,
    queued_forge_builds: usize,
    rejected_overflow_builds: usize,
    active_tenant_count: usize,
    max_concurrent_forge_builds_per_tenant: usize,
    global_max_concurrent_forge_builds: usize,
    sandbox_pool_size: usize,
    queue_depth_limit: usize,
    workflow_script_slots: usize,
    workflow_batches_required: usize,
    provider_stream_slots: usize,
    provider_stream_batches_required: usize,
    max_concurrent_agents_per_workflow: usize,
    max_total_agents_per_workflow: usize,
    max_parallel_agent_slots: usize,
    medium_build_p95_seconds: usize,
    complex_build_p95_seconds: usize,
    direct_agent_latency_ratio_bps: usize,
    orchestration_overhead_ms: usize,
    deterministic_gate_overhead_ms: usize,
    queue_wait_p95_ms: usize,
    dynamic_workflow_pattern_count: usize,
    reviewer_context_mode: String,
    builder_can_self_accept: bool,
    repo_isolation_mode: String,
    sandbox_isolation_policy: String,
    provider_slot_policy: String,
    admission_decision: String,
    status: String,
    terminal_state: String,
    security_rejected: bool,
    bounded_backpressure: bool,
}

struct ScaleOrchestrationRequest {
    tenant_id: String,
    actor_id: String,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    requested_forge_builds: usize,
    active_tenant_count: usize,
    max_concurrent_forge_builds_per_tenant: usize,
    global_max_concurrent_forge_builds: usize,
    sandbox_pool_size: usize,
    queue_depth_limit: usize,
    workflow_script_slots: usize,
    provider_stream_slots: usize,
    max_concurrent_agents_per_workflow: usize,
    max_total_agents_per_workflow: usize,
    medium_build_p95_seconds: usize,
    complex_build_p95_seconds: usize,
    direct_agent_latency_ratio_bps: usize,
    orchestration_overhead_ms: usize,
    deterministic_gate_overhead_ms: usize,
    queue_wait_p95_ms: usize,
    reviewer_context_mode: String,
    builder_can_self_accept: bool,
    repo_isolation_mode: String,
    sandbox_isolation_policy: String,
    provider_slot_policy: String,
    network_allowed: bool,
    secret_inheritance_allowed: bool,
    production_write_authority: bool,
    external_repo_checkout_allowed: bool,
    workflow_shell_access_allowed: bool,
    workflow_filesystem_access_allowed: bool,
}

impl DxrScaleOrchestrationRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrScaleOrchestrationResult, String> {
        let request = parse_scale_orchestration_request(body)?;
        self.next_proof += 1;

        let tenant_capacity = request
            .active_tenant_count
            .saturating_mul(request.max_concurrent_forge_builds_per_tenant);
        let accepted_forge_builds = request
            .requested_forge_builds
            .min(request.peak_parallel_forge_builds)
            .min(request.global_max_concurrent_forge_builds)
            .min(tenant_capacity);
        let immediate_sandbox_reservations = accepted_forge_builds.min(request.sandbox_pool_size);
        let queued_forge_builds =
            accepted_forge_builds.saturating_sub(immediate_sandbox_reservations);
        let rejected_overflow_builds = request
            .requested_forge_builds
            .saturating_sub(accepted_forge_builds)
            .saturating_add(queued_forge_builds.saturating_sub(request.queue_depth_limit));
        let workflow_batches_required =
            ceil_div(accepted_forge_builds, request.workflow_script_slots.max(1));
        let provider_stream_batches_required =
            ceil_div(accepted_forge_builds, request.provider_stream_slots.max(1));
        let max_parallel_agent_slots = request
            .workflow_script_slots
            .saturating_mul(request.max_concurrent_agents_per_workflow);
        let security_rejected = request.network_allowed
            || request.secret_inheritance_allowed
            || request.production_write_authority
            || request.external_repo_checkout_allowed
            || request.workflow_shell_access_allowed
            || request.workflow_filesystem_access_allowed
            || request.builder_can_self_accept;
        let bounded_backpressure =
            !security_rejected && queued_forge_builds > 0 && rejected_overflow_builds == 0;
        let (admission_decision, status, terminal_state) = if security_rejected {
            (
                "rejected_security_boundary",
                "DXR_SCALE_ORCHESTRATION_REJECTED_SECURITY_BOUNDARY",
                "DXR_SCALE_ORCHESTRATION_PROOF_REJECTED_SECURITY_BOUNDARY",
            )
        } else if rejected_overflow_builds > 0 {
            (
                "rejected_overflow",
                "DXR_SCALE_ORCHESTRATION_REJECTED_OVERFLOW_LOCAL",
                "DXR_SCALE_ORCHESTRATION_PROOF_RECORDED_OVERFLOW_REJECTED",
            )
        } else if bounded_backpressure {
            (
                "bounded_backpressure",
                "LIVE-LOCAL-DXR-SCALE-ORCHESTRATION-FLOOR",
                "DXR_SCALE_ORCHESTRATION_PROOF_RECORDED_BACKPRESSURE_BOUNDED",
            )
        } else {
            (
                "admitted_under_capacity",
                "LIVE-LOCAL-DXR-SCALE-ORCHESTRATION-FLOOR",
                "DXR_SCALE_ORCHESTRATION_PROOF_RECORDED_ADMITTED",
            )
        };

        let proof = DxrScaleOrchestrationProof {
            sequence: self.next_proof,
            proof_id: format!("dxr_scale_orchestration_proof_{:06}", self.next_proof),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            requested_forge_builds: request.requested_forge_builds,
            accepted_forge_builds,
            immediate_sandbox_reservations,
            queued_forge_builds,
            rejected_overflow_builds,
            active_tenant_count: request.active_tenant_count,
            max_concurrent_forge_builds_per_tenant: request.max_concurrent_forge_builds_per_tenant,
            global_max_concurrent_forge_builds: request.global_max_concurrent_forge_builds,
            sandbox_pool_size: request.sandbox_pool_size,
            queue_depth_limit: request.queue_depth_limit,
            workflow_script_slots: request.workflow_script_slots,
            workflow_batches_required,
            provider_stream_slots: request.provider_stream_slots,
            provider_stream_batches_required,
            max_concurrent_agents_per_workflow: request.max_concurrent_agents_per_workflow,
            max_total_agents_per_workflow: request.max_total_agents_per_workflow,
            max_parallel_agent_slots,
            medium_build_p95_seconds: request.medium_build_p95_seconds,
            complex_build_p95_seconds: request.complex_build_p95_seconds,
            direct_agent_latency_ratio_bps: request.direct_agent_latency_ratio_bps,
            orchestration_overhead_ms: request.orchestration_overhead_ms,
            deterministic_gate_overhead_ms: request.deterministic_gate_overhead_ms,
            queue_wait_p95_ms: request.queue_wait_p95_ms,
            dynamic_workflow_pattern_count: 6,
            reviewer_context_mode: request.reviewer_context_mode,
            builder_can_self_accept: request.builder_can_self_accept,
            repo_isolation_mode: request.repo_isolation_mode,
            sandbox_isolation_policy: request.sandbox_isolation_policy,
            provider_slot_policy: request.provider_slot_policy,
            admission_decision: admission_decision.to_string(),
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            security_rejected,
            bounded_backpressure,
        };
        let events = scale_orchestration_events(&proof);
        let body = render_scale_orchestration_response_json(&proof);
        self.proofs.push(proof);
        Ok(DxrScaleOrchestrationResult { body, events })
    }

    pub fn proofs_json(&self) -> String {
        let accepted_count = self
            .proofs
            .iter()
            .filter(|proof| !proof.security_rejected && proof.rejected_overflow_builds == 0)
            .count();
        let backpressure_count = self
            .proofs
            .iter()
            .filter(|proof| proof.bounded_backpressure)
            .count();
        let rejected_count = self.proofs.len().saturating_sub(accepted_count);
        format!(
            r#"{{"name":"mdx-dxr-scale-orchestration","status":"LIVE-LOCAL-DXR-SCALE-ORCHESTRATION-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/scale-orchestration.json","submit_route":"/v1/dxr/scale-orchestration-proofs","proof_count":{},"accepted_count":{},"backpressure_count":{},"rejected_count":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"global_max_concurrent_forge_builds":{},"sandbox_pool_size":{},"workflow_script_slots":{},"provider_stream_slots":{},"max_concurrent_agents_per_workflow":{},"max_total_agents_per_workflow":{},"medium_build_p95_seconds":{},"complex_build_p95_seconds":{},"direct_agent_latency_ratio_bps":{},"performance_posture":"comparable_to_direct_agent_builds","workflow_orchestration":"script_held_state_with_resumable_batches","reviewer_separation_required":true,"builder_can_self_accept":false,"tenant_fairness_policy":"tenant_weighted_fair_queue","backpressure_policy":"reserve_sandbox_slots_then_queue_without_starting_processes","sandbox_process_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false,"proofs":[{}]}}"#,
            self.proofs.len(),
            accepted_count,
            backpressure_count,
            rejected_count,
            DEFAULT_TARGET_ENGINEERS,
            DEFAULT_PEAK_PARALLEL_FORGE_BUILDS,
            DEFAULT_GLOBAL_BUILD_LIMIT,
            DEFAULT_SANDBOX_POOL_SIZE,
            DEFAULT_WORKFLOW_SCRIPT_SLOTS,
            DEFAULT_PROVIDER_STREAM_SLOTS,
            DEFAULT_MAX_CONCURRENT_AGENTS_PER_WORKFLOW,
            DEFAULT_MAX_TOTAL_AGENTS_PER_WORKFLOW,
            DEFAULT_MEDIUM_BUILD_P95_SECONDS,
            DEFAULT_COMPLEX_BUILD_P95_SECONDS,
            DEFAULT_DIRECT_AGENT_LATENCY_RATIO_BPS,
            self.proofs
                .iter()
                .map(render_scale_orchestration_proof_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn parse_scale_orchestration_request(body: &str) -> Result<ScaleOrchestrationRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR scale orchestration json: {error}"))?;
    let workflow_script_slots = usize_value(
        &value,
        "workflow_script_slots",
        DEFAULT_WORKFLOW_SCRIPT_SLOTS,
    );
    let provider_stream_slots = usize_value(
        &value,
        "provider_stream_slots",
        DEFAULT_PROVIDER_STREAM_SLOTS,
    );
    if workflow_script_slots == 0 || provider_stream_slots == 0 {
        return Err(
            "DXR scale orchestration proof denied: workflow and provider slots are required"
                .to_string(),
        );
    }
    Ok(ScaleOrchestrationRequest {
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
        requested_forge_builds: usize_value(
            &value,
            "requested_forge_builds",
            DEFAULT_REQUESTED_FORGE_BUILDS,
        ),
        active_tenant_count: usize_value(
            &value,
            "active_tenant_count",
            DEFAULT_ACTIVE_TENANT_COUNT,
        ),
        max_concurrent_forge_builds_per_tenant: usize_value(
            &value,
            "max_concurrent_forge_builds_per_tenant",
            DEFAULT_TENANT_BUILD_LIMIT,
        ),
        global_max_concurrent_forge_builds: usize_value(
            &value,
            "global_max_concurrent_forge_builds",
            DEFAULT_GLOBAL_BUILD_LIMIT,
        ),
        sandbox_pool_size: usize_value(&value, "sandbox_pool_size", DEFAULT_SANDBOX_POOL_SIZE),
        queue_depth_limit: usize_value(&value, "queue_depth_limit", DEFAULT_QUEUE_DEPTH_LIMIT),
        workflow_script_slots,
        provider_stream_slots,
        max_concurrent_agents_per_workflow: usize_value(
            &value,
            "max_concurrent_agents_per_workflow",
            DEFAULT_MAX_CONCURRENT_AGENTS_PER_WORKFLOW,
        ),
        max_total_agents_per_workflow: usize_value(
            &value,
            "max_total_agents_per_workflow",
            DEFAULT_MAX_TOTAL_AGENTS_PER_WORKFLOW,
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
        orchestration_overhead_ms: usize_value(
            &value,
            "orchestration_overhead_ms",
            DEFAULT_ORCHESTRATION_OVERHEAD_MS,
        ),
        deterministic_gate_overhead_ms: usize_value(
            &value,
            "deterministic_gate_overhead_ms",
            DEFAULT_DETERMINISTIC_GATE_OVERHEAD_MS,
        ),
        queue_wait_p95_ms: usize_value(&value, "queue_wait_p95_ms", DEFAULT_QUEUE_WAIT_P95_MS),
        reviewer_context_mode: string_value(
            &value,
            "reviewer_context_mode",
            "fresh_context_required",
        ),
        builder_can_self_accept: bool_value(&value, "builder_can_self_accept", false),
        repo_isolation_mode: string_value(
            &value,
            "repo_isolation_mode",
            "ephemeral_per_run_worktree_or_external_repo_workspace",
        ),
        sandbox_isolation_policy: string_value(
            &value,
            "sandbox_isolation_policy",
            "container_or_microvm_per_run_no_secret_inheritance_no_network_by_default",
        ),
        provider_slot_policy: string_value(
            &value,
            "provider_slot_policy",
            "bounded_stream_slots_with_provider_failover",
        ),
        network_allowed: bool_value(&value, "network_allowed", false),
        secret_inheritance_allowed: bool_value(&value, "secret_inheritance_allowed", false),
        production_write_authority: bool_value(&value, "production_write_authority", false),
        external_repo_checkout_allowed: bool_value(&value, "external_repo_checkout_allowed", false),
        workflow_shell_access_allowed: bool_value(&value, "workflow_shell_access_allowed", false),
        workflow_filesystem_access_allowed: bool_value(
            &value,
            "workflow_filesystem_access_allowed",
            false,
        ),
    })
}

fn scale_orchestration_events(
    proof: &DxrScaleOrchestrationProof,
) -> Vec<DxrScaleOrchestrationRuntimeEvent> {
    let mut events = vec![
        scale_orchestration_event(proof, "scale_orchestration_proof_recorded"),
        scale_orchestration_event(proof, "forge_build_pressure_modeled"),
        scale_orchestration_event(proof, "workflow_script_slots_bounded"),
        scale_orchestration_event(proof, "provider_stream_slots_bounded"),
        scale_orchestration_event(proof, "reviewer_separation_required"),
        scale_orchestration_event(proof, "forbidden_authority_remained_closed"),
    ];
    if proof.bounded_backpressure {
        events.push(scale_orchestration_event(
            proof,
            "sandbox_backpressure_planned",
        ));
    }
    if proof.security_rejected || proof.rejected_overflow_builds > 0 {
        events.push(scale_orchestration_event(
            proof,
            "scale_orchestration_rejected",
        ));
    }
    events
}

fn scale_orchestration_event(
    proof: &DxrScaleOrchestrationProof,
    event_type: &str,
) -> DxrScaleOrchestrationRuntimeEvent {
    DxrScaleOrchestrationRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: proof.tenant_id.clone(),
        job_id: proof.proof_id.clone(),
        run_id: proof.proof_id.clone(),
        actor_id: proof.actor_id.clone(),
    }
}

fn render_scale_orchestration_response_json(proof: &DxrScaleOrchestrationProof) -> String {
    format!(
        r#"{{"name":"mdx-dxr-scale-orchestration-proof","status":{},"runtime":"mdx-dxr-engine","proof":{},"proof_id":{},"tenant_id":{},"actor_id":{},"terminal_state":{},"admission_decision":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"requested_forge_builds":{},"accepted_forge_builds":{},"immediate_sandbox_reservations":{},"queued_forge_builds":{},"rejected_overflow_builds":{},"workflow_batches_required":{},"provider_stream_batches_required":{},"max_parallel_agent_slots":{},"performance_posture":"comparable_to_direct_agent_builds","sandbox_process_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&proof.status),
        render_scale_orchestration_proof_json(proof),
        json_string_literal(&proof.proof_id),
        json_string_literal(&proof.tenant_id),
        json_string_literal(&proof.actor_id),
        json_string_literal(&proof.terminal_state),
        json_string_literal(&proof.admission_decision),
        proof.target_concurrent_engineers,
        proof.peak_parallel_forge_builds,
        proof.requested_forge_builds,
        proof.accepted_forge_builds,
        proof.immediate_sandbox_reservations,
        proof.queued_forge_builds,
        proof.rejected_overflow_builds,
        proof.workflow_batches_required,
        proof.provider_stream_batches_required,
        proof.max_parallel_agent_slots
    )
}

fn render_scale_orchestration_proof_json(proof: &DxrScaleOrchestrationProof) -> String {
    format!(
        r#"{{"sequence":{},"proof_id":{},"tenant_id":{},"actor_id":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"requested_forge_builds":{},"accepted_forge_builds":{},"immediate_sandbox_reservations":{},"queued_forge_builds":{},"rejected_overflow_builds":{},"active_tenant_count":{},"max_concurrent_forge_builds_per_tenant":{},"global_max_concurrent_forge_builds":{},"sandbox_pool_size":{},"queue_depth_limit":{},"workflow_script_slots":{},"workflow_batches_required":{},"provider_stream_slots":{},"provider_stream_batches_required":{},"max_concurrent_agents_per_workflow":{},"max_total_agents_per_workflow":{},"max_parallel_agent_slots":{},"medium_build_p95_seconds":{},"complex_build_p95_seconds":{},"direct_agent_latency_ratio_bps":{},"orchestration_overhead_ms":{},"deterministic_gate_overhead_ms":{},"queue_wait_p95_ms":{},"dynamic_workflow_pattern_count":{},"reviewer_context_mode":{},"builder_can_self_accept":{},"repo_isolation_mode":{},"sandbox_isolation_policy":{},"provider_slot_policy":{},"admission_decision":{},"status":{},"terminal_state":{},"security_rejected":{},"bounded_backpressure":{},"workflow_orchestration":"script_held_state_with_resumable_batches","tenant_fairness_policy":"tenant_weighted_fair_queue","backpressure_policy":"reserve_sandbox_slots_then_queue_without_starting_processes","sandbox_process_started":false,"external_repo_checkout_started":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        proof.sequence,
        json_string_literal(&proof.proof_id),
        json_string_literal(&proof.tenant_id),
        json_string_literal(&proof.actor_id),
        proof.target_concurrent_engineers,
        proof.peak_parallel_forge_builds,
        proof.requested_forge_builds,
        proof.accepted_forge_builds,
        proof.immediate_sandbox_reservations,
        proof.queued_forge_builds,
        proof.rejected_overflow_builds,
        proof.active_tenant_count,
        proof.max_concurrent_forge_builds_per_tenant,
        proof.global_max_concurrent_forge_builds,
        proof.sandbox_pool_size,
        proof.queue_depth_limit,
        proof.workflow_script_slots,
        proof.workflow_batches_required,
        proof.provider_stream_slots,
        proof.provider_stream_batches_required,
        proof.max_concurrent_agents_per_workflow,
        proof.max_total_agents_per_workflow,
        proof.max_parallel_agent_slots,
        proof.medium_build_p95_seconds,
        proof.complex_build_p95_seconds,
        proof.direct_agent_latency_ratio_bps,
        proof.orchestration_overhead_ms,
        proof.deterministic_gate_overhead_ms,
        proof.queue_wait_p95_ms,
        proof.dynamic_workflow_pattern_count,
        json_string_literal(&proof.reviewer_context_mode),
        proof.builder_can_self_accept,
        json_string_literal(&proof.repo_isolation_mode),
        json_string_literal(&proof.sandbox_isolation_policy),
        json_string_literal(&proof.provider_slot_policy),
        json_string_literal(&proof.admission_decision),
        json_string_literal(&proof.status),
        json_string_literal(&proof.terminal_state),
        proof.security_rejected,
        proof.bounded_backpressure
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
    fn scale_orchestration_bounds_five_thousand_forge_builds_without_authority() {
        let mut runtime = DxrScaleOrchestrationRuntime::new();
        let result = runtime.submit_json("{}").expect("scale proof");
        assert!(
            result
                .body
                .contains("DXR_SCALE_ORCHESTRATION_PROOF_RECORDED_BACKPRESSURE_BOUNDED")
        );
        assert!(result.body.contains("\"target_concurrent_engineers\":1000"));
        assert!(result.body.contains("\"accepted_forge_builds\":5000"));
        assert!(
            result
                .body
                .contains("\"immediate_sandbox_reservations\":2048")
        );
        assert!(result.body.contains("\"queued_forge_builds\":2952"));
        assert!(result.body.contains("\"workflow_batches_required\":16"));
        assert!(
            result
                .body
                .contains("\"provider_stream_batches_required\":10")
        );
        assert!(result.body.contains("\"max_parallel_agent_slots\":5120"));
        assert!(result.body.contains("\"builder_can_self_accept\":false"));
        assert!(result.body.contains("\"sandbox_process_started\":false"));
        assert!(result.body.contains("\"provider_calls_allowed\":false"));
        assert!(result.events.len() >= 7);
    }

    #[test]
    fn scale_orchestration_rejects_boundary_widening() {
        let mut runtime = DxrScaleOrchestrationRuntime::new();
        let result = runtime
            .submit_json(r#"{"network_allowed":true,"builder_can_self_accept":true}"#)
            .expect("security rejected proof");
        assert!(
            result
                .body
                .contains("DXR_SCALE_ORCHESTRATION_REJECTED_SECURITY_BOUNDARY")
        );
        assert!(result.body.contains("\"security_rejected\":true"));
        assert!(result.body.contains("\"production_writes_allowed\":false"));
    }
}
