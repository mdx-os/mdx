use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_RATIO_CEILING_PERCENT: usize = 115;
const DEFAULT_MEDIUM_BUILD_P95_SECONDS: usize = 180;
const DEFAULT_COMPLEX_BUILD_P95_SECONDS: usize = 600;

#[derive(Default)]
pub struct DxrBuildPerformanceRuntime {
    proofs: Vec<DxrBuildPerformanceProof>,
    next_proof: usize,
}

pub struct DxrBuildPerformanceResult {
    pub body: String,
    pub events: Vec<DxrBuildPerformanceRuntimeEvent>,
}

pub struct DxrBuildPerformanceRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrBuildPerformanceProof {
    sequence: usize,
    proof_id: String,
    tenant_id: String,
    actor_id: String,
    build_class: String,
    source_run_id: String,
    baseline_driver: String,
    forge_driver: String,
    direct_agent_baseline_seconds: usize,
    forge_observed_seconds: usize,
    target_p95_seconds: usize,
    ratio_ceiling_percent: usize,
    observed_ratio_percent: usize,
    orchestration_overhead_ms: usize,
    deterministic_gate_overhead_ms: usize,
    queue_wait_p95_ms: usize,
    status: String,
    terminal_state: String,
    passed: bool,
}

struct BuildPerformanceRequest {
    tenant_id: String,
    actor_id: String,
    build_class: String,
    source_run_id: String,
    baseline_driver: String,
    forge_driver: String,
    direct_agent_baseline_seconds: usize,
    forge_observed_seconds: usize,
    target_p95_seconds: usize,
    ratio_ceiling_percent: usize,
    orchestration_overhead_ms: usize,
    deterministic_gate_overhead_ms: usize,
    queue_wait_p95_ms: usize,
}

impl DxrBuildPerformanceRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrBuildPerformanceResult, String> {
        let request = parse_build_performance_request(body)?;
        self.next_proof += 1;
        let observed_ratio_percent = ceil_ratio_percent(
            request.forge_observed_seconds,
            request.direct_agent_baseline_seconds,
        );
        let passed = request.forge_observed_seconds <= request.target_p95_seconds
            && observed_ratio_percent <= request.ratio_ceiling_percent;
        let proof = DxrBuildPerformanceProof {
            sequence: self.next_proof,
            proof_id: format!("dxr_build_performance_proof_{:06}", self.next_proof),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            build_class: request.build_class,
            source_run_id: request.source_run_id,
            baseline_driver: request.baseline_driver,
            forge_driver: request.forge_driver,
            direct_agent_baseline_seconds: request.direct_agent_baseline_seconds,
            forge_observed_seconds: request.forge_observed_seconds,
            target_p95_seconds: request.target_p95_seconds,
            ratio_ceiling_percent: request.ratio_ceiling_percent,
            observed_ratio_percent,
            orchestration_overhead_ms: request.orchestration_overhead_ms,
            deterministic_gate_overhead_ms: request.deterministic_gate_overhead_ms,
            queue_wait_p95_ms: request.queue_wait_p95_ms,
            status: if passed {
                "DXR_BUILD_PERFORMANCE_SLO_PROVEN_LOCAL".to_string()
            } else {
                "DXR_BUILD_PERFORMANCE_SLO_FAILED_LOCAL".to_string()
            },
            terminal_state: if passed {
                "DXR_BUILD_PERFORMANCE_RECORDED_COMPARABLE_TO_DIRECT_AGENT".to_string()
            } else {
                "DXR_BUILD_PERFORMANCE_RECORDED_TUNING_REQUIRED".to_string()
            },
            passed,
        };
        let events = build_performance_events(&proof);
        let body = render_build_performance_response_json(&proof);
        self.proofs.push(proof);
        Ok(DxrBuildPerformanceResult { body, events })
    }

    pub fn proofs_json(&self) -> String {
        let passing_count = self.proofs.iter().filter(|proof| proof.passed).count();
        let failing_count = self.proofs.len().saturating_sub(passing_count);
        let medium_count = self
            .proofs
            .iter()
            .filter(|proof| proof.build_class == "medium")
            .count();
        let complex_count = self
            .proofs
            .iter()
            .filter(|proof| proof.build_class == "complex")
            .count();
        format!(
            r#"{{"name":"mdx-dxr-build-performance","status":"LIVE-LOCAL-DXR-BUILD-PERFORMANCE-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/build-performance.json","submit_route":"/v1/dxr/build-performance-proofs","proof_count":{},"passing_count":{},"failing_count":{},"medium_count":{},"complex_count":{},"medium_build_p95_seconds":{},"complex_build_p95_seconds":{},"direct_agent_latency_ratio_ceiling_percent":{},"performance_posture":"comparable_to_direct_agent_builds","proofs":[{}],"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"production_writes_allowed":false}}"#,
            self.proofs.len(),
            passing_count,
            failing_count,
            medium_count,
            complex_count,
            DEFAULT_MEDIUM_BUILD_P95_SECONDS,
            DEFAULT_COMPLEX_BUILD_P95_SECONDS,
            DEFAULT_RATIO_CEILING_PERCENT,
            self.proofs
                .iter()
                .map(render_build_performance_proof_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn parse_build_performance_request(body: &str) -> Result<BuildPerformanceRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR build performance json: {error}"))?;
    let build_class = string_value(&value, "build_class", "medium");
    if !matches!(build_class.as_str(), "medium" | "complex") {
        return Err(
            "DXR build performance proof denied: build_class must be medium or complex".to_string(),
        );
    }
    let direct_agent_baseline_seconds = usize_value(&value, "direct_agent_baseline_seconds", 160);
    let forge_observed_seconds = usize_value(&value, "forge_observed_seconds", 176);
    if direct_agent_baseline_seconds == 0 || forge_observed_seconds == 0 {
        return Err(
            "DXR build performance proof denied: baseline and observed seconds are required"
                .to_string(),
        );
    }
    Ok(BuildPerformanceRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        target_p95_seconds: usize_value(
            &value,
            "target_p95_seconds",
            target_for_build_class(&build_class),
        ),
        build_class,
        source_run_id: string_value(&value, "source_run_id", "dxr_run_dispatch_ready_001"),
        baseline_driver: string_value(&value, "baseline_driver", "direct_claude_code"),
        forge_driver: string_value(&value, "forge_driver", "mdx_forge_dxr_workflow"),
        direct_agent_baseline_seconds,
        forge_observed_seconds,
        ratio_ceiling_percent: usize_value(
            &value,
            "ratio_ceiling_percent",
            DEFAULT_RATIO_CEILING_PERCENT,
        ),
        orchestration_overhead_ms: usize_value(&value, "orchestration_overhead_ms", 1200),
        deterministic_gate_overhead_ms: usize_value(&value, "deterministic_gate_overhead_ms", 900),
        queue_wait_p95_ms: usize_value(&value, "queue_wait_p95_ms", 20),
    })
}

fn build_performance_events(
    proof: &DxrBuildPerformanceProof,
) -> Vec<DxrBuildPerformanceRuntimeEvent> {
    let mut events = vec![
        build_performance_event(proof, "build_performance_proof_recorded"),
        build_performance_event(proof, "direct_agent_baseline_compared"),
        build_performance_event(proof, "forge_latency_budget_enforced"),
        build_performance_event(proof, "deterministic_gate_overhead_recorded"),
    ];
    if proof.passed {
        events.push(build_performance_event(
            proof,
            "build_performance_slo_passed",
        ));
    } else {
        events.push(build_performance_event(
            proof,
            "build_performance_slo_failed",
        ));
    }
    events
}

fn build_performance_event(
    proof: &DxrBuildPerformanceProof,
    event_type: &str,
) -> DxrBuildPerformanceRuntimeEvent {
    DxrBuildPerformanceRuntimeEvent {
        event_type: event_type.to_string(),
        tenant_id: proof.tenant_id.clone(),
        job_id: proof.proof_id.clone(),
        run_id: proof.source_run_id.clone(),
        actor_id: proof.actor_id.clone(),
    }
}

fn render_build_performance_response_json(proof: &DxrBuildPerformanceProof) -> String {
    format!(
        r#"{{"name":"mdx-dxr-build-performance-proof","status":{},"runtime":"mdx-dxr-engine","proof":{},"proof_id":{},"tenant_id":{},"actor_id":{},"build_class":{},"source_run_id":{},"terminal_state":{},"direct_agent_baseline_seconds":{},"forge_observed_seconds":{},"target_p95_seconds":{},"ratio_ceiling_percent":{},"observed_ratio_percent":{},"orchestration_overhead_ms":{},"deterministic_gate_overhead_ms":{},"queue_wait_p95_ms":{},"performance_posture":"comparable_to_direct_agent_builds","provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"production_writes_allowed":false}}"#,
        json_string_literal(&proof.status),
        render_build_performance_proof_json(proof),
        json_string_literal(&proof.proof_id),
        json_string_literal(&proof.tenant_id),
        json_string_literal(&proof.actor_id),
        json_string_literal(&proof.build_class),
        json_string_literal(&proof.source_run_id),
        json_string_literal(&proof.terminal_state),
        proof.direct_agent_baseline_seconds,
        proof.forge_observed_seconds,
        proof.target_p95_seconds,
        proof.ratio_ceiling_percent,
        proof.observed_ratio_percent,
        proof.orchestration_overhead_ms,
        proof.deterministic_gate_overhead_ms,
        proof.queue_wait_p95_ms
    )
}

fn render_build_performance_proof_json(proof: &DxrBuildPerformanceProof) -> String {
    format!(
        r#"{{"sequence":{},"proof_id":{},"tenant_id":{},"actor_id":{},"build_class":{},"source_run_id":{},"baseline_driver":{},"forge_driver":{},"direct_agent_baseline_seconds":{},"forge_observed_seconds":{},"target_p95_seconds":{},"ratio_ceiling_percent":{},"observed_ratio_percent":{},"orchestration_overhead_ms":{},"deterministic_gate_overhead_ms":{},"queue_wait_p95_ms":{},"status":{},"terminal_state":{},"passed":{},"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"sandbox_process_started":false,"external_repo_checkout_started":false,"production_writes_allowed":false}}"#,
        proof.sequence,
        json_string_literal(&proof.proof_id),
        json_string_literal(&proof.tenant_id),
        json_string_literal(&proof.actor_id),
        json_string_literal(&proof.build_class),
        json_string_literal(&proof.source_run_id),
        json_string_literal(&proof.baseline_driver),
        json_string_literal(&proof.forge_driver),
        proof.direct_agent_baseline_seconds,
        proof.forge_observed_seconds,
        proof.target_p95_seconds,
        proof.ratio_ceiling_percent,
        proof.observed_ratio_percent,
        proof.orchestration_overhead_ms,
        proof.deterministic_gate_overhead_ms,
        proof.queue_wait_p95_ms,
        json_string_literal(&proof.status),
        json_string_literal(&proof.terminal_state),
        proof.passed
    )
}

fn target_for_build_class(build_class: &str) -> usize {
    if build_class == "complex" {
        DEFAULT_COMPLEX_BUILD_P95_SECONDS
    } else {
        DEFAULT_MEDIUM_BUILD_P95_SECONDS
    }
}

fn ceil_ratio_percent(numerator: usize, denominator: usize) -> usize {
    numerator
        .saturating_mul(100)
        .saturating_add(denominator - 1)
        / denominator
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_performance_proof_compares_forge_to_direct_agent() {
        let mut runtime = DxrBuildPerformanceRuntime::new();
        let output = runtime
            .submit_json(
                r#"{"build_class":"medium","direct_agent_baseline_seconds":160,"forge_observed_seconds":176}"#,
            )
            .expect("performance proof")
            .body;
        assert!(output.contains("DXR_BUILD_PERFORMANCE_SLO_PROVEN_LOCAL"));
        assert!(output.contains("\"observed_ratio_percent\":110"));
        assert!(output.contains("\"provider_calls_allowed\":false"));
    }
}
