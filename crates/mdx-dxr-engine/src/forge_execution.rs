use mdx_core::json_string_literal;
use serde_json::Value;

const STATUS: &str = "LIVE-LOCAL-DXR-FORGE-EXECUTION-PROOF-FLOOR";
const VALID_PROVIDER_MEMORY_TERMINAL_STATES: [&str; 2] = [
    "DXR_PROVIDER_MEMORY_INTEGRATION_RECORDED_LOCAL_DRIVER_READY_PROVIDER_BLOCKED",
    "DXR_PROVIDER_MEMORY_INTEGRATION_RECORDED_PROVIDER_MEMORY_READY_EXECUTION_BLOCKED",
];
const VALID_WORKFLOW_ORCHESTRATION_TERMINAL_STATES: [&str; 4] = [
    "DXR_WORKFLOW_ORCHESTRATION_RECORDED_READY_AUTHORITY_BLOCKED",
    "DXR_WORKFLOW_ORCHESTRATION_RECORDED_BACKPRESSURE_SUPERVISED",
    "DXR_WORKFLOW_ORCHESTRATION_RECORDED_CANCELLED_REPLAY_READY",
    "DXR_WORKFLOW_ORCHESTRATION_RECORDED_REPLAY_READY",
];
const VALID_AUTHORITY_APPLY_TERMINAL_STATES: [&str; 4] = [
    "DXR_AUTHORITY_APPLY_RECORDED_CI_EVIDENCE_READY",
    "DXR_AUTHORITY_APPLY_RECORDED_PATCH_PROPOSAL_READY",
    "DXR_AUTHORITY_APPLY_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED",
    "DXR_AUTHORITY_APPLY_RECORDED_REJECTED_BY_HUMAN",
];

#[derive(Default)]
pub struct DxrForgeExecutionRuntime {
    proofs: Vec<DxrForgeExecutionProof>,
    next_proof: usize,
}

pub struct DxrForgeExecutionResult {
    pub body: String,
    pub durable_record: DxrForgeExecutionDurableRecord,
    pub events: Vec<DxrForgeExecutionRuntimeEvent>,
}

pub struct DxrForgeExecutionDurableRecord {
    pub proof_id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub job_id: String,
    pub run_id: String,
    pub idempotency_key: String,
    pub status: String,
    pub terminal_state: String,
    pub execution_decision: String,
}

pub struct DxrForgeExecutionRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrForgeExecutionProof {
    sequence: usize,
    proof_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    idempotency_key: String,
    forge_build_request_receipt_id: String,
    forge_build_approval_receipt_id: String,
    forge_workflow_plan_proof_id: String,
    talent_authorization_receipt_id: String,
    worker_credential_receipt_id: String,
    worker_spawn_preflight_receipt_id: String,
    forge_ci_evidence_receipt_id: String,
    forge_human_ratification_receipt_id: String,
    forge_deployment_preflight_receipt_id: String,
    ctx_context_input_id: String,
    provider_memory_integration_id: String,
    provider_memory_terminal_state: String,
    workflow_orchestration_id: String,
    workflow_orchestration_terminal_state: String,
    authority_apply_id: String,
    authority_apply_terminal_state: String,
    forge_build_request_observed: bool,
    forge_build_approval_observed: bool,
    forge_workflow_plan_observed: bool,
    talent_authorization_observed: bool,
    worker_credential_observed: bool,
    worker_spawn_preflight_observed: bool,
    forge_ci_evidence_observed: bool,
    forge_human_ratification_observed: bool,
    forge_deployment_preflight_observed: bool,
    ctx_context_observed: bool,
    provider_memory_integration_observed: bool,
    workflow_orchestration_observed: bool,
    authority_apply_observed: bool,
    reviewer_separation_observed: bool,
    durable_state_observed: bool,
    relay_event_stream_observed: bool,
    evidence_chain_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    performance_budget_observed: bool,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    sandbox_pool_size: usize,
    workflow_script_slots: usize,
    provider_stream_slots: usize,
    retained_backpressure_count: usize,
    medium_build_p95_target_ms: usize,
    complex_build_p95_target_ms: usize,
    observed_governance_overhead_ratio_bps: usize,
    terminal_state: String,
    status: String,
    execution_decision: String,
    rejected: bool,
    rejection_reason: String,
}

struct ForgeExecutionRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    idempotency_key: String,
    forge_build_request_receipt_id: String,
    forge_build_approval_receipt_id: String,
    forge_workflow_plan_proof_id: String,
    talent_authorization_receipt_id: String,
    worker_credential_receipt_id: String,
    worker_spawn_preflight_receipt_id: String,
    forge_ci_evidence_receipt_id: String,
    forge_human_ratification_receipt_id: String,
    forge_deployment_preflight_receipt_id: String,
    ctx_context_input_id: String,
    provider_memory_integration_id: String,
    provider_memory_terminal_state: String,
    workflow_orchestration_id: String,
    workflow_orchestration_terminal_state: String,
    authority_apply_id: String,
    authority_apply_terminal_state: String,
    forge_build_request_observed: bool,
    forge_build_approval_observed: bool,
    forge_workflow_plan_observed: bool,
    talent_authorization_observed: bool,
    worker_credential_observed: bool,
    worker_spawn_preflight_observed: bool,
    forge_ci_evidence_observed: bool,
    forge_human_ratification_observed: bool,
    forge_deployment_preflight_observed: bool,
    ctx_context_observed: bool,
    provider_memory_integration_observed: bool,
    workflow_orchestration_observed: bool,
    authority_apply_observed: bool,
    reviewer_separation_observed: bool,
    durable_state_observed: bool,
    relay_event_stream_observed: bool,
    evidence_chain_observed: bool,
    tenant_fairness_observed: bool,
    backpressure_observed: bool,
    performance_budget_observed: bool,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    sandbox_pool_size: usize,
    workflow_script_slots: usize,
    provider_stream_slots: usize,
    retained_backpressure_count: usize,
    medium_build_p95_target_ms: usize,
    complex_build_p95_target_ms: usize,
    observed_governance_overhead_ratio_bps: usize,
    worker_spawn_requested: bool,
    sandbox_start_requested: bool,
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

impl DxrForgeExecutionRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrForgeExecutionResult, String> {
        let request = parse_forge_execution_request(body)?;
        self.next_proof += 1;

        let unsafe_authority = request.worker_spawn_requested
            || request.sandbox_start_requested
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
        let core_ready = request.core_evidence_observed() && request.core_shape_valid();
        let ci_ready = core_ready
            && request.authority_apply_observed
            && request.forge_ci_evidence_observed
            && !request.authority_apply_id.trim().is_empty()
            && !request.forge_ci_evidence_receipt_id.trim().is_empty()
            && VALID_AUTHORITY_APPLY_TERMINAL_STATES
                .contains(&request.authority_apply_terminal_state.as_str());
        let human_rejected = ci_ready
            && request.authority_apply_terminal_state
                == "DXR_AUTHORITY_APPLY_RECORDED_REJECTED_BY_HUMAN";
        let human_ratified = ci_ready
            && request.forge_human_ratification_observed
            && request.forge_deployment_preflight_observed
            && !request
                .forge_human_ratification_receipt_id
                .trim()
                .is_empty()
            && !request
                .forge_deployment_preflight_receipt_id
                .trim()
                .is_empty()
            && request.authority_apply_terminal_state
                == "DXR_AUTHORITY_APPLY_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED";

        let (status, terminal_state, execution_decision, rejected, rejection_reason) =
            if unsafe_authority {
                (
                    "DXR_FORGE_EXECUTION_PROOF_REJECTED_SECURITY_BOUNDARY",
                    "DXR_FORGE_EXECUTION_PROOF_REJECTED_SECURITY_BOUNDARY",
                    "rejected_security_boundary",
                    true,
                    "forge_execution_cannot_spawn_workers_start_sandboxes_call_providers_execute_tools_run_shell_git_network_secrets_filesystem_patch_ci_deploy_or_write_production",
                )
            } else if !core_ready {
                (
                    "DXR_FORGE_EXECUTION_PROOF_REJECTED_MISSING_EVIDENCE",
                    "DXR_FORGE_EXECUTION_PROOF_REJECTED_MISSING_EVIDENCE",
                    "rejected_missing_forge_or_dxr_evidence",
                    true,
                    "forge_receipts_talent_worker_ctx_provider_memory_workflow_orchestration_reviewer_durable_relay_evidence_required",
                )
            } else if human_rejected {
                (
                    "DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_REJECTED_CLOSED",
                    "DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_REJECTED_CLOSED",
                    "human_rejected_forge_execution_closed",
                    false,
                    "none",
                )
            } else if human_ratified {
                (
                    STATUS,
                    "DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED",
                    "forge_execution_ratified_apply_still_mutation_blocked",
                    false,
                    "none",
                )
            } else if ci_ready {
                (
                    "DXR_FORGE_EXECUTION_PROOF_RECORDED_CI_READY",
                    "DXR_FORGE_EXECUTION_PROOF_RECORDED_CI_READY",
                    "forge_execution_ci_ready_before_human_ratification",
                    false,
                    "none",
                )
            } else {
                (
                    "DXR_FORGE_EXECUTION_PROOF_RECORDED_READY_AUTHORITY_BLOCKED",
                    "DXR_FORGE_EXECUTION_PROOF_RECORDED_READY_AUTHORITY_BLOCKED",
                    "forge_execution_ready_before_ci_apply_authority",
                    false,
                    "none",
                )
            };

        let proof = DxrForgeExecutionProof {
            sequence: self.next_proof,
            proof_id: format!("dxr_forge_execution_proof_{:06}", self.next_proof),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            idempotency_key: request.idempotency_key,
            forge_build_request_receipt_id: request.forge_build_request_receipt_id,
            forge_build_approval_receipt_id: request.forge_build_approval_receipt_id,
            forge_workflow_plan_proof_id: request.forge_workflow_plan_proof_id,
            talent_authorization_receipt_id: request.talent_authorization_receipt_id,
            worker_credential_receipt_id: request.worker_credential_receipt_id,
            worker_spawn_preflight_receipt_id: request.worker_spawn_preflight_receipt_id,
            forge_ci_evidence_receipt_id: request.forge_ci_evidence_receipt_id,
            forge_human_ratification_receipt_id: request.forge_human_ratification_receipt_id,
            forge_deployment_preflight_receipt_id: request.forge_deployment_preflight_receipt_id,
            ctx_context_input_id: request.ctx_context_input_id,
            provider_memory_integration_id: request.provider_memory_integration_id,
            provider_memory_terminal_state: request.provider_memory_terminal_state,
            workflow_orchestration_id: request.workflow_orchestration_id,
            workflow_orchestration_terminal_state: request.workflow_orchestration_terminal_state,
            authority_apply_id: request.authority_apply_id,
            authority_apply_terminal_state: request.authority_apply_terminal_state,
            forge_build_request_observed: request.forge_build_request_observed,
            forge_build_approval_observed: request.forge_build_approval_observed,
            forge_workflow_plan_observed: request.forge_workflow_plan_observed,
            talent_authorization_observed: request.talent_authorization_observed,
            worker_credential_observed: request.worker_credential_observed,
            worker_spawn_preflight_observed: request.worker_spawn_preflight_observed,
            forge_ci_evidence_observed: request.forge_ci_evidence_observed,
            forge_human_ratification_observed: request.forge_human_ratification_observed,
            forge_deployment_preflight_observed: request.forge_deployment_preflight_observed,
            ctx_context_observed: request.ctx_context_observed,
            provider_memory_integration_observed: request.provider_memory_integration_observed,
            workflow_orchestration_observed: request.workflow_orchestration_observed,
            authority_apply_observed: request.authority_apply_observed,
            reviewer_separation_observed: request.reviewer_separation_observed,
            durable_state_observed: request.durable_state_observed,
            relay_event_stream_observed: request.relay_event_stream_observed,
            evidence_chain_observed: request.evidence_chain_observed,
            tenant_fairness_observed: request.tenant_fairness_observed,
            backpressure_observed: request.backpressure_observed,
            performance_budget_observed: request.performance_budget_observed,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            sandbox_pool_size: request.sandbox_pool_size,
            workflow_script_slots: request.workflow_script_slots,
            provider_stream_slots: request.provider_stream_slots,
            retained_backpressure_count: request.retained_backpressure_count,
            medium_build_p95_target_ms: request.medium_build_p95_target_ms,
            complex_build_p95_target_ms: request.complex_build_p95_target_ms,
            observed_governance_overhead_ratio_bps: request.observed_governance_overhead_ratio_bps,
            terminal_state: terminal_state.to_string(),
            status: status.to_string(),
            execution_decision: execution_decision.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let body = render_forge_execution_response_json(&proof);
        let durable_record = DxrForgeExecutionDurableRecord {
            proof_id: proof.proof_id.clone(),
            tenant_id: proof.tenant_id.clone(),
            actor_id: proof.actor_id.clone(),
            job_id: proof.job_id.clone(),
            run_id: proof.run_id.clone(),
            idempotency_key: proof.idempotency_key.clone(),
            status: proof.status.clone(),
            terminal_state: proof.terminal_state.clone(),
            execution_decision: proof.execution_decision.clone(),
        };
        let events = forge_execution_events(&proof);
        self.proofs.push(proof);
        Ok(DxrForgeExecutionResult {
            body,
            durable_record,
            events,
        })
    }

    pub fn proofs_json(&self) -> String {
        let ready_count = self
            .proofs
            .iter()
            .filter(|proof| {
                proof.terminal_state == "DXR_FORGE_EXECUTION_PROOF_RECORDED_READY_AUTHORITY_BLOCKED"
            })
            .count();
        let ci_ready_count = self
            .proofs
            .iter()
            .filter(|proof| proof.terminal_state == "DXR_FORGE_EXECUTION_PROOF_RECORDED_CI_READY")
            .count();
        let human_ratified_count = self
            .proofs
            .iter()
            .filter(|proof| {
                proof.terminal_state
                    == "DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED"
            })
            .count();
        let human_rejected_count = self
            .proofs
            .iter()
            .filter(|proof| {
                proof.terminal_state == "DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_REJECTED_CLOSED"
            })
            .count();
        let rejected_count = self.proofs.iter().filter(|proof| proof.rejected).count();
        format!(
            r#"{{"name":"mdx-dxr-forge-execution-proofs","status":"{}","runtime":"mdx-dxr-engine","route":"/dxr/forge-execution-proofs.json","submit_route":"/v1/dxr/forge-execution-proofs","proof_count":{},"ready_count":{},"ci_ready_count":{},"human_ratified_count":{},"human_rejected_count":{},"rejected_count":{},"execution_policy":"forge_receipts_to_dxr_workflow_authority_apply_before_mutation","required_execution_gates":{},"forge_receipts_bound":{},"dxr_workflow_bound":{},"authority_apply_bound":{},"ci_evidence_bound":{},"human_ratification_bound":{},"deployment_preflight_bound":{},"scale_floor_bound":{},"target_concurrent_engineers":1000,"peak_parallel_forge_builds":5000,"sandbox_pool_size_floor":2048,"workflow_script_slots_floor":320,"provider_stream_slots_floor":512,"medium_build_p95_target_ms":180000,"complex_build_p95_target_ms":600000,"max_governance_overhead_ratio_bps":11500,"worker_spawn_allowed":false,"sandbox_start_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"patch_application_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"proofs":[{}]}}"#,
            STATUS,
            self.proofs.len(),
            ready_count,
            ci_ready_count,
            human_ratified_count,
            human_rejected_count,
            rejected_count,
            render_required_execution_gates_json(),
            self.proofs.iter().any(|proof| proof.forge_receipts_ready()),
            self.proofs.iter().any(|proof| proof.dxr_workflow_ready()),
            self.proofs
                .iter()
                .any(|proof| proof.authority_apply_ready()),
            self.proofs
                .iter()
                .any(|proof| proof.forge_ci_evidence_observed),
            self.proofs
                .iter()
                .any(|proof| proof.forge_human_ratification_observed),
            self.proofs
                .iter()
                .any(|proof| proof.forge_deployment_preflight_observed),
            self.proofs.iter().any(|proof| proof.scale_floor_ready()),
            self.proofs
                .iter()
                .map(render_forge_execution_proof_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

impl ForgeExecutionRequest {
    fn core_evidence_observed(&self) -> bool {
        self.forge_build_request_observed
            && self.forge_build_approval_observed
            && self.forge_workflow_plan_observed
            && self.talent_authorization_observed
            && self.worker_credential_observed
            && self.worker_spawn_preflight_observed
            && self.ctx_context_observed
            && self.provider_memory_integration_observed
            && self.workflow_orchestration_observed
            && self.reviewer_separation_observed
            && self.durable_state_observed
            && self.relay_event_stream_observed
            && self.evidence_chain_observed
            && self.tenant_fairness_observed
            && self.backpressure_observed
            && self.performance_budget_observed
    }

    fn core_shape_valid(&self) -> bool {
        !self.forge_build_request_receipt_id.trim().is_empty()
            && !self.forge_build_approval_receipt_id.trim().is_empty()
            && !self.forge_workflow_plan_proof_id.trim().is_empty()
            && !self.talent_authorization_receipt_id.trim().is_empty()
            && !self.worker_credential_receipt_id.trim().is_empty()
            && !self.worker_spawn_preflight_receipt_id.trim().is_empty()
            && !self.ctx_context_input_id.trim().is_empty()
            && !self.provider_memory_integration_id.trim().is_empty()
            && !self.workflow_orchestration_id.trim().is_empty()
            && VALID_PROVIDER_MEMORY_TERMINAL_STATES
                .contains(&self.provider_memory_terminal_state.as_str())
            && VALID_WORKFLOW_ORCHESTRATION_TERMINAL_STATES
                .contains(&self.workflow_orchestration_terminal_state.as_str())
            && self.target_concurrent_engineers >= 1000
            && self.peak_parallel_forge_builds >= 5000
            && self.sandbox_pool_size >= 2048
            && self.workflow_script_slots >= 320
            && self.provider_stream_slots >= 512
            && self.medium_build_p95_target_ms <= 180_000
            && self.complex_build_p95_target_ms <= 600_000
            && self.observed_governance_overhead_ratio_bps <= 11_500
    }
}

impl DxrForgeExecutionProof {
    fn forge_receipts_ready(&self) -> bool {
        self.forge_build_request_observed
            && self.forge_build_approval_observed
            && self.forge_workflow_plan_observed
            && self.talent_authorization_observed
            && self.worker_credential_observed
            && self.worker_spawn_preflight_observed
    }

    fn dxr_workflow_ready(&self) -> bool {
        self.ctx_context_observed
            && self.provider_memory_integration_observed
            && self.workflow_orchestration_observed
            && VALID_PROVIDER_MEMORY_TERMINAL_STATES
                .contains(&self.provider_memory_terminal_state.as_str())
            && VALID_WORKFLOW_ORCHESTRATION_TERMINAL_STATES
                .contains(&self.workflow_orchestration_terminal_state.as_str())
    }

    fn authority_apply_ready(&self) -> bool {
        self.authority_apply_observed
            && VALID_AUTHORITY_APPLY_TERMINAL_STATES
                .contains(&self.authority_apply_terminal_state.as_str())
    }

    fn scale_floor_ready(&self) -> bool {
        self.tenant_fairness_observed
            && self.backpressure_observed
            && self.performance_budget_observed
            && self.target_concurrent_engineers >= 1000
            && self.peak_parallel_forge_builds >= 5000
            && self.sandbox_pool_size >= 2048
            && self.workflow_script_slots >= 320
            && self.provider_stream_slots >= 512
    }
}

fn forge_execution_events(proof: &DxrForgeExecutionProof) -> Vec<DxrForgeExecutionRuntimeEvent> {
    let mut event_types = vec!["forge_execution_proof_recorded"];
    if proof.forge_receipts_ready() {
        event_types.push("forge_execution_forge_receipts_bound");
    }
    if proof.dxr_workflow_ready() {
        event_types.push("forge_execution_dxr_workflow_bound");
        event_types.push("forge_execution_provider_memory_bound");
        event_types.push("forge_execution_workflow_orchestration_bound");
    }
    if proof.authority_apply_ready() {
        event_types.push("forge_execution_authority_apply_bound");
    }
    if proof.forge_ci_evidence_observed {
        event_types.push("forge_execution_ci_evidence_bound");
    }
    if proof.forge_human_ratification_observed {
        event_types.push("forge_execution_human_ratification_bound");
    }
    if proof.forge_deployment_preflight_observed {
        event_types.push("forge_execution_deployment_preflight_bound");
    }
    if proof.reviewer_separation_observed
        && proof.durable_state_observed
        && proof.relay_event_stream_observed
        && proof.evidence_chain_observed
    {
        event_types.push("forge_execution_governance_evidence_bound");
    }
    if proof.scale_floor_ready() {
        event_types.push("forge_execution_scale_backpressure_bound");
    }
    if proof.terminal_state == "DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_REJECTED_CLOSED" {
        event_types.push("forge_execution_human_rejection_closed");
    }
    if proof.terminal_state == "DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED" {
        event_types.push("forge_execution_ready_before_mutation");
    }
    if proof.rejected {
        event_types.push("forge_execution_rejected");
    } else {
        event_types.push("forge_execution_authority_blocked");
    }
    event_types
        .into_iter()
        .map(|event_type| DxrForgeExecutionRuntimeEvent {
            event_type: event_type.to_string(),
            tenant_id: proof.tenant_id.clone(),
            job_id: proof.job_id.clone(),
            run_id: proof.run_id.clone(),
            actor_id: proof.actor_id.clone(),
        })
        .collect()
}

fn render_forge_execution_response_json(proof: &DxrForgeExecutionProof) -> String {
    format!(
        r#"{{"status":{},"proof_id":{},"execution_decision":{},"forge_receipts_bound":{},"dxr_workflow_bound":{},"authority_apply_bound":{},"ci_evidence_bound":{},"human_ratification_bound":{},"deployment_preflight_bound":{},"scale_floor_bound":{},"worker_spawn_allowed":false,"sandbox_start_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"patch_application_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"proof":{}}}"#,
        json_string_literal(&proof.status),
        json_string_literal(&proof.proof_id),
        json_string_literal(&proof.execution_decision),
        proof.forge_receipts_ready(),
        proof.dxr_workflow_ready(),
        proof.authority_apply_ready(),
        proof.forge_ci_evidence_observed,
        proof.forge_human_ratification_observed,
        proof.forge_deployment_preflight_observed,
        proof.scale_floor_ready(),
        render_forge_execution_proof_json(proof)
    )
}

fn render_forge_execution_proof_json(proof: &DxrForgeExecutionProof) -> String {
    format!(
        r#"{{"sequence":{},"proof_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"idempotency_key":{},"forge_build_request_receipt_id":{},"forge_build_approval_receipt_id":{},"forge_workflow_plan_proof_id":{},"talent_authorization_receipt_id":{},"worker_credential_receipt_id":{},"worker_spawn_preflight_receipt_id":{},"forge_ci_evidence_receipt_id":{},"forge_human_ratification_receipt_id":{},"forge_deployment_preflight_receipt_id":{},"ctx_context_input_id":{},"provider_memory_integration_id":{},"provider_memory_terminal_state":{},"workflow_orchestration_id":{},"workflow_orchestration_terminal_state":{},"authority_apply_id":{},"authority_apply_terminal_state":{},"forge_build_request_observed":{},"forge_build_approval_observed":{},"forge_workflow_plan_observed":{},"talent_authorization_observed":{},"worker_credential_observed":{},"worker_spawn_preflight_observed":{},"forge_ci_evidence_observed":{},"forge_human_ratification_observed":{},"forge_deployment_preflight_observed":{},"ctx_context_observed":{},"provider_memory_integration_observed":{},"workflow_orchestration_observed":{},"authority_apply_observed":{},"reviewer_separation_observed":{},"durable_state_observed":{},"relay_event_stream_observed":{},"evidence_chain_observed":{},"tenant_fairness_observed":{},"backpressure_observed":{},"performance_budget_observed":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"sandbox_pool_size":{},"workflow_script_slots":{},"provider_stream_slots":{},"retained_backpressure_count":{},"medium_build_p95_target_ms":{},"complex_build_p95_target_ms":{},"observed_governance_overhead_ratio_bps":{},"terminal_state":{},"status":{},"execution_decision":{},"rejected":{},"rejection_reason":{},"worker_spawn_allowed":false,"sandbox_start_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"patch_application_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false}}"#,
        proof.sequence,
        json_string_literal(&proof.proof_id),
        json_string_literal(&proof.tenant_id),
        json_string_literal(&proof.actor_id),
        json_string_literal(&proof.job_id),
        json_string_literal(&proof.run_id),
        json_string_literal(&proof.idempotency_key),
        json_string_literal(&proof.forge_build_request_receipt_id),
        json_string_literal(&proof.forge_build_approval_receipt_id),
        json_string_literal(&proof.forge_workflow_plan_proof_id),
        json_string_literal(&proof.talent_authorization_receipt_id),
        json_string_literal(&proof.worker_credential_receipt_id),
        json_string_literal(&proof.worker_spawn_preflight_receipt_id),
        json_string_literal(&proof.forge_ci_evidence_receipt_id),
        json_string_literal(&proof.forge_human_ratification_receipt_id),
        json_string_literal(&proof.forge_deployment_preflight_receipt_id),
        json_string_literal(&proof.ctx_context_input_id),
        json_string_literal(&proof.provider_memory_integration_id),
        json_string_literal(&proof.provider_memory_terminal_state),
        json_string_literal(&proof.workflow_orchestration_id),
        json_string_literal(&proof.workflow_orchestration_terminal_state),
        json_string_literal(&proof.authority_apply_id),
        json_string_literal(&proof.authority_apply_terminal_state),
        proof.forge_build_request_observed,
        proof.forge_build_approval_observed,
        proof.forge_workflow_plan_observed,
        proof.talent_authorization_observed,
        proof.worker_credential_observed,
        proof.worker_spawn_preflight_observed,
        proof.forge_ci_evidence_observed,
        proof.forge_human_ratification_observed,
        proof.forge_deployment_preflight_observed,
        proof.ctx_context_observed,
        proof.provider_memory_integration_observed,
        proof.workflow_orchestration_observed,
        proof.authority_apply_observed,
        proof.reviewer_separation_observed,
        proof.durable_state_observed,
        proof.relay_event_stream_observed,
        proof.evidence_chain_observed,
        proof.tenant_fairness_observed,
        proof.backpressure_observed,
        proof.performance_budget_observed,
        proof.target_concurrent_engineers,
        proof.peak_parallel_forge_builds,
        proof.sandbox_pool_size,
        proof.workflow_script_slots,
        proof.provider_stream_slots,
        proof.retained_backpressure_count,
        proof.medium_build_p95_target_ms,
        proof.complex_build_p95_target_ms,
        proof.observed_governance_overhead_ratio_bps,
        json_string_literal(&proof.terminal_state),
        json_string_literal(&proof.status),
        json_string_literal(&proof.execution_decision),
        proof.rejected,
        json_string_literal(&proof.rejection_reason)
    )
}

fn render_required_execution_gates_json() -> String {
    [
        "forge_build_request",
        "forge_build_approval",
        "forge_workflow_plan_proof",
        "talent_authorization",
        "worker_credential",
        "worker_spawn_preflight",
        "ctx_context",
        "provider_memory_integration",
        "workflow_orchestration",
        "authority_apply",
        "ci_evidence",
        "human_ratification",
        "deployment_preflight",
        "reviewer_separation",
        "durable_state",
        "relay_event_stream",
        "evidence_chain",
        "tenant_fairness",
        "backpressure",
        "performance_budget",
    ]
    .iter()
    .map(|gate| json_string_literal(gate))
    .collect::<Vec<_>>()
    .join(",")
    .pipe(|gates| format!("[{}]", gates))
}

fn parse_forge_execution_request(body: &str) -> Result<ForgeExecutionRequest, String> {
    let value = if body.trim().is_empty() {
        Value::Object(Default::default())
    } else {
        serde_json::from_str::<Value>(body).map_err(|error| format!("parse json: {error}"))?
    };
    Ok(ForgeExecutionRequest {
        tenant_id: string_value(&value, "tenant_id", "tenant_local"),
        actor_id: string_value(&value, "actor_id", "forge_operator"),
        job_id: string_value(&value, "job_id", "dxr_job_dispatch_ready_001"),
        run_id: string_value(&value, "run_id", "dxr_run_dispatch_ready_001"),
        idempotency_key: string_value(&value, "idempotency_key", "dxr-forge-execution-001"),
        forge_build_request_receipt_id: string_value(
            &value,
            "forge_build_request_receipt_id",
            "forge_build_request_receipt_000001",
        ),
        forge_build_approval_receipt_id: string_value(
            &value,
            "forge_build_approval_receipt_id",
            "forge_build_approval_receipt_000001",
        ),
        forge_workflow_plan_proof_id: string_value(
            &value,
            "forge_workflow_plan_proof_id",
            "forge_workflow_plan_proof_000001",
        ),
        talent_authorization_receipt_id: string_value(
            &value,
            "talent_authorization_receipt_id",
            "talent_authorization_receipt_000001",
        ),
        worker_credential_receipt_id: string_value(
            &value,
            "worker_credential_receipt_id",
            "worker_credential_receipt_000001",
        ),
        worker_spawn_preflight_receipt_id: string_value(
            &value,
            "worker_spawn_preflight_receipt_id",
            "worker_spawn_preflight_receipt_000001",
        ),
        forge_ci_evidence_receipt_id: string_value(
            &value,
            "forge_ci_evidence_receipt_id",
            "ci_verification_observed_000001",
        ),
        forge_human_ratification_receipt_id: string_value(
            &value,
            "forge_human_ratification_receipt_id",
            "forge_ship_ratified_000001",
        ),
        forge_deployment_preflight_receipt_id: string_value(
            &value,
            "forge_deployment_preflight_receipt_id",
            "forge_deployment_preflight_000001",
        ),
        ctx_context_input_id: string_value(
            &value,
            "ctx_context_input_id",
            "dxr_ctx_context_input_000001",
        ),
        provider_memory_integration_id: string_value(
            &value,
            "provider_memory_integration_id",
            "dxr_provider_memory_integration_000001",
        ),
        provider_memory_terminal_state: string_value(
            &value,
            "provider_memory_terminal_state",
            "DXR_PROVIDER_MEMORY_INTEGRATION_RECORDED_PROVIDER_MEMORY_READY_EXECUTION_BLOCKED",
        ),
        workflow_orchestration_id: string_value(
            &value,
            "workflow_orchestration_id",
            "dxr_workflow_orchestration_000001",
        ),
        workflow_orchestration_terminal_state: string_value(
            &value,
            "workflow_orchestration_terminal_state",
            "DXR_WORKFLOW_ORCHESTRATION_RECORDED_READY_AUTHORITY_BLOCKED",
        ),
        authority_apply_id: string_value(
            &value,
            "authority_apply_id",
            "dxr_authority_apply_000001",
        ),
        authority_apply_terminal_state: string_value(
            &value,
            "authority_apply_terminal_state",
            "DXR_AUTHORITY_APPLY_RECORDED_CI_EVIDENCE_READY",
        ),
        forge_build_request_observed: bool_value(&value, "forge_build_request_observed", true),
        forge_build_approval_observed: bool_value(&value, "forge_build_approval_observed", true),
        forge_workflow_plan_observed: bool_value(&value, "forge_workflow_plan_observed", true),
        talent_authorization_observed: bool_value(&value, "talent_authorization_observed", true),
        worker_credential_observed: bool_value(&value, "worker_credential_observed", true),
        worker_spawn_preflight_observed: bool_value(
            &value,
            "worker_spawn_preflight_observed",
            true,
        ),
        forge_ci_evidence_observed: bool_value(&value, "forge_ci_evidence_observed", false),
        forge_human_ratification_observed: bool_value(
            &value,
            "forge_human_ratification_observed",
            false,
        ),
        forge_deployment_preflight_observed: bool_value(
            &value,
            "forge_deployment_preflight_observed",
            false,
        ),
        ctx_context_observed: bool_value(&value, "ctx_context_observed", true),
        provider_memory_integration_observed: bool_value(
            &value,
            "provider_memory_integration_observed",
            true,
        ),
        workflow_orchestration_observed: bool_value(
            &value,
            "workflow_orchestration_observed",
            true,
        ),
        authority_apply_observed: bool_value(&value, "authority_apply_observed", false),
        reviewer_separation_observed: bool_value(&value, "reviewer_separation_observed", true),
        durable_state_observed: bool_value(&value, "durable_state_observed", true),
        relay_event_stream_observed: bool_value(&value, "relay_event_stream_observed", true),
        evidence_chain_observed: bool_value(&value, "evidence_chain_observed", true),
        tenant_fairness_observed: bool_value(&value, "tenant_fairness_observed", true),
        backpressure_observed: bool_value(&value, "backpressure_observed", true),
        performance_budget_observed: bool_value(&value, "performance_budget_observed", true),
        target_concurrent_engineers: usize_value(&value, "target_concurrent_engineers", 1000),
        peak_parallel_forge_builds: usize_value(&value, "peak_parallel_forge_builds", 5000),
        sandbox_pool_size: usize_value(&value, "sandbox_pool_size", 2048),
        workflow_script_slots: usize_value(&value, "workflow_script_slots", 320),
        provider_stream_slots: usize_value(&value, "provider_stream_slots", 512),
        retained_backpressure_count: usize_value(&value, "retained_backpressure_count", 2952),
        medium_build_p95_target_ms: usize_value(&value, "medium_build_p95_target_ms", 180_000),
        complex_build_p95_target_ms: usize_value(&value, "complex_build_p95_target_ms", 600_000),
        observed_governance_overhead_ratio_bps: usize_value(
            &value,
            "observed_governance_overhead_ratio_bps",
            11_500,
        ),
        worker_spawn_requested: bool_value(&value, "worker_spawn_requested", false),
        sandbox_start_requested: bool_value(&value, "sandbox_start_requested", false),
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
        .and_then(|number| usize::try_from(number).ok())
        .unwrap_or(default)
}

trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}

impl<T> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forge_execution_records_stages_and_rejects_unsafe_authority() {
        let mut runtime = DxrForgeExecutionRuntime::new();

        let ready = runtime
            .submit_json(r#"{"idempotency_key":"ready"}"#)
            .expect("ready forge execution proof");
        assert!(
            ready
                .body
                .contains("DXR_FORGE_EXECUTION_PROOF_RECORDED_READY_AUTHORITY_BLOCKED")
        );

        let ci_ready = runtime
            .submit_json(
                r#"{"idempotency_key":"ci","authority_apply_observed":true,"forge_ci_evidence_observed":true}"#,
            )
            .expect("ci forge execution proof");
        assert!(
            ci_ready
                .body
                .contains("DXR_FORGE_EXECUTION_PROOF_RECORDED_CI_READY")
        );

        let ratified = runtime.submit_json(
            r#"{"idempotency_key":"ratified","authority_apply_observed":true,"forge_ci_evidence_observed":true,"forge_human_ratification_observed":true,"forge_deployment_preflight_observed":true,"authority_apply_terminal_state":"DXR_AUTHORITY_APPLY_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED"}"#,
        ).expect("ratified forge execution proof");
        assert!(
            ratified
                .body
                .contains("DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED")
        );
        assert!(
            ratified
                .events
                .iter()
                .any(|event| event.event_type == "forge_execution_ready_before_mutation")
        );

        let human_rejected = runtime.submit_json(
            r#"{"idempotency_key":"human-rejected","authority_apply_observed":true,"forge_ci_evidence_observed":true,"authority_apply_terminal_state":"DXR_AUTHORITY_APPLY_RECORDED_REJECTED_BY_HUMAN"}"#,
        ).expect("human rejected forge execution proof");
        assert!(
            human_rejected
                .body
                .contains("DXR_FORGE_EXECUTION_PROOF_RECORDED_HUMAN_REJECTED_CLOSED")
        );

        let missing = runtime
            .submit_json(
                r#"{"idempotency_key":"missing","provider_memory_integration_observed":false}"#,
            )
            .expect("missing forge execution proof");
        assert!(
            missing
                .body
                .contains("DXR_FORGE_EXECUTION_PROOF_REJECTED_MISSING_EVIDENCE")
        );

        let unsafe_request = runtime
            .submit_json(
                r#"{"idempotency_key":"unsafe","worker_spawn_requested":true,"sandbox_start_requested":true,"patch_application_requested":true,"production_write_requested":true}"#,
            )
            .expect("unsafe forge execution proof");
        assert!(
            unsafe_request
                .body
                .contains("DXR_FORGE_EXECUTION_PROOF_REJECTED_SECURITY_BOUNDARY")
        );

        let projection = runtime.proofs_json();
        assert!(projection.contains(r#""proof_count":6"#));
        assert!(projection.contains(r#""ready_count":1"#));
        assert!(projection.contains(r#""ci_ready_count":1"#));
        assert!(projection.contains(r#""human_ratified_count":1"#));
        assert!(projection.contains(r#""human_rejected_count":1"#));
        assert!(projection.contains(r#""rejected_count":2"#));
        assert!(projection.contains("provider_memory_integration"));
        assert!(projection.contains(r#""patch_application_allowed":false"#));
        assert!(projection.contains(r#""production_writes_allowed":false"#));
    }
}
