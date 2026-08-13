use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_JOB_ID: &str = "dxr_job_dispatch_ready_001";
const DEFAULT_RUN_ID: &str = "dxr_run_dispatch_ready_001";
const DEFAULT_WORKFLOW_ORCHESTRATION_ID: &str = "dxr_workflow_orchestration_000001";
const DEFAULT_WORKFLOW_RUN_ID: &str = "dxr_workflow_run_000001";
const DEFAULT_DYNAMIC_WORKFLOW_PLAN_ID: &str = "dxr_dynamic_workflow_plan_000001";
const DEFAULT_EXECUTION_SUPERVISION_ID: &str = "dxr_execution_supervision_000001";
const DEFAULT_SANDBOX_RESULT_CONSUMPTION_ID: &str = "dxr_sandbox_result_consumption_000001";
const DEFAULT_CI_EVIDENCE_RECEIPT_ID: &str = "ci_verification_observed_000001";
const DEFAULT_CI_WORKFLOW_RUN_ID: &str = "27016450296";
const DEFAULT_CI_JOB_ID: &str = "79732614423";
const DEFAULT_CI_COMMIT_SHA: &str = "611c02a000d79fe874b2ebf80b33874116bb0bc9";
const DEFAULT_CI_BRANCH: &str = "codex/dxr-authority-apply-core";
const DEFAULT_CI_REPOSITORY: &str = "mdx-os/mdx";
const DEFAULT_CI_OBSERVED_AT: &str = "2026-06-05T13:11:13Z";
const DEFAULT_PATCH_PROPOSAL_ID: &str = "dxr_patch_proposal_000001";
const DEFAULT_PATCH_HASH: &str = "sha256:dxr_patch_proposal_hash";
const DEFAULT_PATCH_TARGET_PATH: &str = "crates/mdx-dxr-engine/src/main.rs";
const DEFAULT_PATCH_SCOPE: &str = "dxr_engine_runtime";
const DEFAULT_PATCH_MEDIATION_RECEIPT_ID: &str = "harness_tool_patch_allowed_000001";
const DEFAULT_HUMAN_RATIFICATION_RECEIPT_ID: &str = "forge_ship_ratified_000001";
const DEFAULT_HUMAN_RATIFIER_ID: &str = "human_operator";
const DEFAULT_SECURITY_CHECK_RECEIPT_ID: &str = "forge_security_checked_000001";
const DEFAULT_REVIEW_RECEIPT_ID: &str = "forge_review_completed_000001";
const DEFAULT_STAGE_TRANSITION_RECEIPT_ID: &str = "forge_stage_transition_recorded_000001";
const DEFAULT_POLICY_DECISION_ID: &str = "dxr_authority_policy_decision_000001";
const DEFAULT_DEPLOYMENT_AUTHORITY_RECEIPT_ID: &str = "forge_deployment_authorized_pending";
const DEFAULT_AUTHORITY_EXPIRES_AT: &str = "2026-06-05T14:11:13Z";
const DEFAULT_TARGET_CONCURRENT_ENGINEERS: usize = 1000;
const DEFAULT_PEAK_PARALLEL_FORGE_BUILDS: usize = 5000;
const DEFAULT_MAX_PATCH_PREVIEW_BYTES: usize = 1_048_576;

#[derive(Default)]
pub struct DxrAuthorityApplyRuntime {
    records: Vec<DxrAuthorityApplyRecord>,
    next_record: usize,
}

pub struct DxrAuthorityApplyResult {
    pub body: String,
    pub events: Vec<DxrAuthorityApplyRuntimeEvent>,
}

pub struct DxrAuthorityApplyRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrAuthorityApplyRecord {
    sequence: usize,
    authority_apply_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    idempotency_key: String,
    workflow_orchestration_id: String,
    workflow_run_id: String,
    dynamic_workflow_plan_id: String,
    execution_supervision_id: String,
    sandbox_result_consumption_id: String,
    ci_evidence_receipt_id: String,
    ci_workflow_run_id: String,
    ci_job_id: String,
    ci_commit_sha: String,
    ci_branch: String,
    ci_conclusion: String,
    ci_evidence_url: String,
    ci_observed_at: String,
    patch_proposal_id: String,
    patch_hash: String,
    patch_target_path: String,
    patch_scope: String,
    patch_preview_bytes: usize,
    patch_mediation_receipt_id: String,
    human_ratification_receipt_id: String,
    human_ratifier_id: String,
    human_decision: String,
    security_check_receipt_id: String,
    review_receipt_id: String,
    stage_transition_receipt_id: String,
    policy_decision_id: String,
    deployment_authority_receipt_id: String,
    authority_expires_at: String,
    workflow_orchestration_observed: bool,
    ci_evidence_observed: bool,
    commit_branch_bound: bool,
    security_check_observed: bool,
    review_completed_observed: bool,
    stage_transition_observed: bool,
    patch_proposal_observed: bool,
    patch_mediation_observed: bool,
    human_ratification_observed: bool,
    reviewer_separation_observed: bool,
    deployment_authority_observed: bool,
    durable_state_observed: bool,
    relay_event_stream_observed: bool,
    apply_preflight_observed: bool,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    status: String,
    terminal_state: String,
    authority_decision: String,
    rejected: bool,
    rejection_reason: String,
}

struct AuthorityApplyRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    idempotency_key: String,
    workflow_orchestration_id: String,
    workflow_run_id: String,
    dynamic_workflow_plan_id: String,
    execution_supervision_id: String,
    sandbox_result_consumption_id: String,
    ci_evidence_receipt_id: String,
    ci_workflow_run_id: String,
    ci_job_id: String,
    ci_commit_sha: String,
    ci_branch: String,
    ci_conclusion: String,
    ci_evidence_url: String,
    ci_observed_at: String,
    patch_proposal_id: String,
    patch_hash: String,
    patch_target_path: String,
    patch_scope: String,
    patch_preview_bytes: usize,
    patch_mediation_receipt_id: String,
    human_ratification_receipt_id: String,
    human_ratifier_id: String,
    human_decision: String,
    security_check_receipt_id: String,
    review_receipt_id: String,
    stage_transition_receipt_id: String,
    policy_decision_id: String,
    deployment_authority_receipt_id: String,
    authority_expires_at: String,
    workflow_orchestration_observed: bool,
    ci_evidence_observed: bool,
    commit_branch_bound: bool,
    security_check_observed: bool,
    review_completed_observed: bool,
    stage_transition_observed: bool,
    patch_proposal_observed: bool,
    patch_mediation_observed: bool,
    human_ratification_observed: bool,
    reviewer_separation_observed: bool,
    deployment_authority_observed: bool,
    durable_state_observed: bool,
    relay_event_stream_observed: bool,
    apply_preflight_observed: bool,
    target_concurrent_engineers: usize,
    peak_parallel_forge_builds: usize,
    ci_claim_requested: bool,
    patch_application_requested: bool,
    shell_execution_requested: bool,
    git_execution_requested: bool,
    network_requested: bool,
    secret_inheritance_requested: bool,
    filesystem_mutation_requested: bool,
    deployment_requested: bool,
    production_write_requested: bool,
    worker_spawn_requested: bool,
    provider_calls_requested: bool,
    tool_execution_requested: bool,
}

impl DxrAuthorityApplyRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrAuthorityApplyResult, String> {
        let request = parse_authority_apply_request(body)?;
        self.next_record += 1;

        let authority_requested = request.ci_claim_requested
            || request.patch_application_requested
            || request.shell_execution_requested
            || request.git_execution_requested
            || request.network_requested
            || request.secret_inheritance_requested
            || request.filesystem_mutation_requested
            || request.deployment_requested
            || request.production_write_requested
            || request.worker_spawn_requested
            || request.provider_calls_requested
            || request.tool_execution_requested;
        let core_ready = request.core_evidence_observed() && request.core_shape_valid();
        let patch_ready = request.patch_proposal_observed
            && request.patch_mediation_observed
            && request.patch_shape_valid();
        let human_ready = request.human_ratification_observed && request.human_shape_valid();

        let (status, terminal_state, authority_decision, rejected, rejection_reason) =
            if authority_requested {
                (
                    "DXR_AUTHORITY_APPLY_REJECTED_SECURITY_BOUNDARY",
                    "DXR_AUTHORITY_APPLY_REJECTED_SECURITY_BOUNDARY",
                    "rejected_security_boundary",
                    true,
                    "authority_apply_cannot_claim_ci_apply_patch_run_shell_git_network_secrets_filesystem_deploy_spawn_workers_call_providers_execute_tools_or_write_production",
                )
            } else if !core_ready {
                (
                    "DXR_AUTHORITY_APPLY_REJECTED_MISSING_EVIDENCE",
                    "DXR_AUTHORITY_APPLY_REJECTED_MISSING_EVIDENCE",
                    "rejected_missing_ci_or_workflow_evidence",
                    true,
                    "workflow_orchestration_ci_commit_security_review_stage_reviewer_durable_or_relay_evidence_missing",
                )
            } else if request.human_decision == "reject" && patch_ready && human_ready {
                (
                    "DXR_AUTHORITY_APPLY_RECORDED_REJECTED_BY_HUMAN",
                    "DXR_AUTHORITY_APPLY_RECORDED_REJECTED_BY_HUMAN",
                    "human_rejected_apply_preflight",
                    false,
                    "none",
                )
            } else if request.human_decision == "approve"
                && patch_ready
                && human_ready
                && request.apply_preflight_observed
            {
                (
                    "LIVE-LOCAL-DXR-AUTHORITY-APPLY-FLOOR",
                    "DXR_AUTHORITY_APPLY_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED",
                    "human_ratified_apply_preflight_recorded_mutation_blocked",
                    false,
                    "none",
                )
            } else if patch_ready {
                (
                    "DXR_AUTHORITY_APPLY_RECORDED_PATCH_PROPOSAL_READY",
                    "DXR_AUTHORITY_APPLY_RECORDED_PATCH_PROPOSAL_READY",
                    "patch_proposal_mediated_before_apply",
                    false,
                    "none",
                )
            } else {
                (
                    "DXR_AUTHORITY_APPLY_RECORDED_CI_EVIDENCE_READY",
                    "DXR_AUTHORITY_APPLY_RECORDED_CI_EVIDENCE_READY",
                    "ci_evidence_consumed_before_patch",
                    false,
                    "none",
                )
            };

        let record = DxrAuthorityApplyRecord {
            sequence: self.next_record,
            authority_apply_id: format!("dxr_authority_apply_{:06}", self.next_record),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            idempotency_key: request.idempotency_key,
            workflow_orchestration_id: request.workflow_orchestration_id,
            workflow_run_id: request.workflow_run_id,
            dynamic_workflow_plan_id: request.dynamic_workflow_plan_id,
            execution_supervision_id: request.execution_supervision_id,
            sandbox_result_consumption_id: request.sandbox_result_consumption_id,
            ci_evidence_receipt_id: request.ci_evidence_receipt_id,
            ci_workflow_run_id: request.ci_workflow_run_id,
            ci_job_id: request.ci_job_id,
            ci_commit_sha: request.ci_commit_sha,
            ci_branch: request.ci_branch,
            ci_conclusion: request.ci_conclusion,
            ci_evidence_url: request.ci_evidence_url,
            ci_observed_at: request.ci_observed_at,
            patch_proposal_id: request.patch_proposal_id,
            patch_hash: request.patch_hash,
            patch_target_path: request.patch_target_path,
            patch_scope: request.patch_scope,
            patch_preview_bytes: request.patch_preview_bytes,
            patch_mediation_receipt_id: request.patch_mediation_receipt_id,
            human_ratification_receipt_id: request.human_ratification_receipt_id,
            human_ratifier_id: request.human_ratifier_id,
            human_decision: request.human_decision,
            security_check_receipt_id: request.security_check_receipt_id,
            review_receipt_id: request.review_receipt_id,
            stage_transition_receipt_id: request.stage_transition_receipt_id,
            policy_decision_id: request.policy_decision_id,
            deployment_authority_receipt_id: request.deployment_authority_receipt_id,
            authority_expires_at: request.authority_expires_at,
            workflow_orchestration_observed: request.workflow_orchestration_observed,
            ci_evidence_observed: request.ci_evidence_observed,
            commit_branch_bound: request.commit_branch_bound,
            security_check_observed: request.security_check_observed,
            review_completed_observed: request.review_completed_observed,
            stage_transition_observed: request.stage_transition_observed,
            patch_proposal_observed: request.patch_proposal_observed,
            patch_mediation_observed: request.patch_mediation_observed,
            human_ratification_observed: request.human_ratification_observed,
            reviewer_separation_observed: request.reviewer_separation_observed,
            deployment_authority_observed: request.deployment_authority_observed,
            durable_state_observed: request.durable_state_observed,
            relay_event_stream_observed: request.relay_event_stream_observed,
            apply_preflight_observed: request.apply_preflight_observed,
            target_concurrent_engineers: request.target_concurrent_engineers,
            peak_parallel_forge_builds: request.peak_parallel_forge_builds,
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            authority_decision: authority_decision.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let events = authority_apply_events(&record);
        let body = render_authority_apply_response_json(&record);
        self.records.push(record);
        Ok(DxrAuthorityApplyResult { body, events })
    }

    pub fn authority_apply_json(&self) -> String {
        let ci_evidence_ready_count = self
            .records
            .iter()
            .filter(|record| {
                record.terminal_state == "DXR_AUTHORITY_APPLY_RECORDED_CI_EVIDENCE_READY"
            })
            .count();
        let patch_proposal_ready_count = self
            .records
            .iter()
            .filter(|record| {
                record.terminal_state == "DXR_AUTHORITY_APPLY_RECORDED_PATCH_PROPOSAL_READY"
            })
            .count();
        let human_ratified_count = self
            .records
            .iter()
            .filter(|record| {
                record.terminal_state == "DXR_AUTHORITY_APPLY_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED"
            })
            .count();
        let human_rejected_count = self
            .records
            .iter()
            .filter(|record| {
                record.terminal_state == "DXR_AUTHORITY_APPLY_RECORDED_REJECTED_BY_HUMAN"
            })
            .count();
        let rejected_count = self.records.iter().filter(|record| record.rejected).count();
        let latest_commit_sha = self
            .records
            .iter()
            .rev()
            .find(|record| !record.rejected)
            .map(|record| record.ci_commit_sha.as_str())
            .unwrap_or(DEFAULT_CI_COMMIT_SHA);
        let max_patch_preview_bytes = self
            .records
            .iter()
            .map(|record| record.patch_preview_bytes)
            .max()
            .unwrap_or(0);
        format!(
            r#"{{"name":"mdx-dxr-authority-apply","status":"LIVE-LOCAL-DXR-AUTHORITY-APPLY-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/authority-apply.json","submit_route":"/v1/dxr/authority-apply-runs","authority_apply_count":{},"ci_evidence_ready_count":{},"patch_proposal_ready_count":{},"human_ratified_count":{},"human_rejected_count":{},"rejected_count":{},"latest_commit_sha":{},"max_patch_preview_bytes":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"authority_policy":"observed_ci_and_human_ratification_to_guarded_apply_preflight","required_authority_gates":{},"workflow_orchestration_bound":{},"ci_evidence_consumed":{},"patch_proposal_mediated":{},"human_ratification_ready":{},"guarded_apply_preflight_ready":{},"ci_claim_allowed":false,"patch_application_allowed":false,"patch_applied":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"deployment_allowed":false,"worker_spawn_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"production_writes_allowed":false,"records":[{}]}}"#,
            self.records.len(),
            ci_evidence_ready_count,
            patch_proposal_ready_count,
            human_ratified_count,
            human_rejected_count,
            rejected_count,
            json_string_literal(latest_commit_sha),
            max_patch_preview_bytes,
            DEFAULT_TARGET_CONCURRENT_ENGINEERS,
            DEFAULT_PEAK_PARALLEL_FORGE_BUILDS,
            render_required_authority_gates_json(),
            self.records
                .iter()
                .any(|record| record.workflow_orchestration_observed && !record.rejected),
            self.records
                .iter()
                .any(|record| record.ci_evidence_observed && !record.rejected),
            self.records
                .iter()
                .any(|record| record.patch_mediation_observed && !record.rejected),
            self.records
                .iter()
                .any(|record| record.human_ratification_observed && !record.rejected),
            self.records
                .iter()
                .any(|record| record.apply_preflight_observed && !record.rejected),
            self.records
                .iter()
                .map(render_authority_apply_record_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

impl AuthorityApplyRequest {
    fn core_evidence_observed(&self) -> bool {
        self.workflow_orchestration_observed
            && self.ci_evidence_observed
            && self.commit_branch_bound
            && self.security_check_observed
            && self.review_completed_observed
            && self.stage_transition_observed
            && self.reviewer_separation_observed
            && self.durable_state_observed
            && self.relay_event_stream_observed
    }

    fn core_shape_valid(&self) -> bool {
        !self.idempotency_key.trim().is_empty()
            && !self.workflow_orchestration_id.trim().is_empty()
            && !self.workflow_run_id.trim().is_empty()
            && !self.dynamic_workflow_plan_id.trim().is_empty()
            && !self.execution_supervision_id.trim().is_empty()
            && !self.sandbox_result_consumption_id.trim().is_empty()
            && !self.ci_evidence_receipt_id.trim().is_empty()
            && !self.ci_workflow_run_id.trim().is_empty()
            && !self.ci_job_id.trim().is_empty()
            && self.ci_commit_sha.len() >= 12
            && !self.ci_branch.trim().is_empty()
            && self.ci_conclusion == "success"
            && self.ci_evidence_url.starts_with("https://")
            && !self.ci_observed_at.trim().is_empty()
            && !self.security_check_receipt_id.trim().is_empty()
            && !self.review_receipt_id.trim().is_empty()
            && !self.stage_transition_receipt_id.trim().is_empty()
            && !self.policy_decision_id.trim().is_empty()
            && self.target_concurrent_engineers >= DEFAULT_TARGET_CONCURRENT_ENGINEERS
            && self.peak_parallel_forge_builds >= DEFAULT_PEAK_PARALLEL_FORGE_BUILDS
    }

    fn patch_shape_valid(&self) -> bool {
        !self.patch_proposal_id.trim().is_empty()
            && self.patch_hash.starts_with("sha256:")
            && !self.patch_target_path.trim().is_empty()
            && !self.patch_scope.trim().is_empty()
            && self.patch_preview_bytes > 0
            && self.patch_preview_bytes <= DEFAULT_MAX_PATCH_PREVIEW_BYTES
            && !self.patch_mediation_receipt_id.trim().is_empty()
    }

    fn human_shape_valid(&self) -> bool {
        !self.human_ratification_receipt_id.trim().is_empty()
            && !self.human_ratifier_id.trim().is_empty()
            && matches!(self.human_decision.as_str(), "approve" | "reject")
            && !self.authority_expires_at.trim().is_empty()
    }
}

fn authority_apply_events(record: &DxrAuthorityApplyRecord) -> Vec<DxrAuthorityApplyRuntimeEvent> {
    let mut event_types = vec![
        "authority_apply_recorded",
        "authority_apply_workflow_orchestration_bound",
        "authority_apply_ci_evidence_consumed",
        "authority_apply_security_review_bound",
        "authority_apply_stage_transition_bound",
        "authority_apply_reviewer_separation_bound",
        "authority_apply_durable_relay_bound",
        "authority_apply_execution_authority_blocked",
    ];
    if record.patch_proposal_observed {
        event_types.push("authority_apply_patch_proposal_mediated");
    }
    if record.human_ratification_observed {
        event_types.push("authority_apply_human_ratification_bound");
    }
    if record.terminal_state == "DXR_AUTHORITY_APPLY_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED" {
        event_types.push("authority_apply_guarded_apply_preflight_recorded");
    }
    if record.terminal_state == "DXR_AUTHORITY_APPLY_RECORDED_REJECTED_BY_HUMAN" {
        event_types.push("authority_apply_human_rejection_recorded");
    }
    if record.rejected {
        event_types.push("authority_apply_rejected");
    } else {
        event_types.push("authority_apply_ready_before_mutation");
    }
    event_types
        .into_iter()
        .map(|event_type| DxrAuthorityApplyRuntimeEvent {
            event_type: event_type.to_string(),
            tenant_id: record.tenant_id.clone(),
            job_id: record.job_id.clone(),
            run_id: record.run_id.clone(),
            actor_id: record.actor_id.clone(),
        })
        .collect()
}

fn render_authority_apply_response_json(record: &DxrAuthorityApplyRecord) -> String {
    format!(
        r#"{{"name":"mdx-dxr-authority-apply","status":{},"runtime":"mdx-dxr-engine","authority_apply":{},"workflow_orchestration_bound":{},"ci_evidence_consumed":{},"patch_proposal_mediated":{},"human_ratification_ready":{},"guarded_apply_preflight_ready":{},"ci_claim_allowed":false,"patch_application_allowed":false,"patch_applied":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"deployment_allowed":false,"worker_spawn_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&record.status),
        render_authority_apply_record_json(record),
        record.workflow_orchestration_observed && !record.rejected,
        record.ci_evidence_observed && !record.rejected,
        record.patch_mediation_observed && !record.rejected,
        record.human_ratification_observed && !record.rejected,
        record.apply_preflight_observed && !record.rejected
    )
}

fn render_authority_apply_record_json(record: &DxrAuthorityApplyRecord) -> String {
    format!(
        r#"{{"sequence":{},"authority_apply_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"idempotency_key":{},"workflow_orchestration_id":{},"workflow_run_id":{},"dynamic_workflow_plan_id":{},"execution_supervision_id":{},"sandbox_result_consumption_id":{},"ci_evidence_receipt_id":{},"ci_workflow_run_id":{},"ci_job_id":{},"ci_commit_sha":{},"ci_branch":{},"ci_conclusion":{},"ci_evidence_url":{},"ci_observed_at":{},"patch_proposal_id":{},"patch_hash":{},"patch_target_path":{},"patch_scope":{},"patch_preview_bytes":{},"patch_mediation_receipt_id":{},"human_ratification_receipt_id":{},"human_ratifier_id":{},"human_decision":{},"security_check_receipt_id":{},"review_receipt_id":{},"stage_transition_receipt_id":{},"policy_decision_id":{},"deployment_authority_receipt_id":{},"authority_expires_at":{},"workflow_orchestration_observed":{},"ci_evidence_observed":{},"commit_branch_bound":{},"security_check_observed":{},"review_completed_observed":{},"stage_transition_observed":{},"patch_proposal_observed":{},"patch_mediation_observed":{},"human_ratification_observed":{},"reviewer_separation_observed":{},"deployment_authority_observed":{},"durable_state_observed":{},"relay_event_stream_observed":{},"apply_preflight_observed":{},"target_concurrent_engineers":{},"peak_parallel_forge_builds":{},"terminal_state":{},"status":{},"authority_decision":{},"rejected":{},"rejection_reason":{},"ci_claim_allowed":false,"patch_application_allowed":false,"patch_applied":false,"shell_execution_allowed":false,"git_execution_allowed":false,"network_allowed":false,"secret_inheritance_allowed":false,"filesystem_mutation_allowed":false,"deployment_allowed":false,"worker_spawn_allowed":false,"provider_calls_allowed":false,"tool_execution_allowed":false,"production_writes_allowed":false}}"#,
        record.sequence,
        json_string_literal(&record.authority_apply_id),
        json_string_literal(&record.tenant_id),
        json_string_literal(&record.actor_id),
        json_string_literal(&record.job_id),
        json_string_literal(&record.run_id),
        json_string_literal(&record.idempotency_key),
        json_string_literal(&record.workflow_orchestration_id),
        json_string_literal(&record.workflow_run_id),
        json_string_literal(&record.dynamic_workflow_plan_id),
        json_string_literal(&record.execution_supervision_id),
        json_string_literal(&record.sandbox_result_consumption_id),
        json_string_literal(&record.ci_evidence_receipt_id),
        json_string_literal(&record.ci_workflow_run_id),
        json_string_literal(&record.ci_job_id),
        json_string_literal(&record.ci_commit_sha),
        json_string_literal(&record.ci_branch),
        json_string_literal(&record.ci_conclusion),
        json_string_literal(&record.ci_evidence_url),
        json_string_literal(&record.ci_observed_at),
        json_string_literal(&record.patch_proposal_id),
        json_string_literal(&record.patch_hash),
        json_string_literal(&record.patch_target_path),
        json_string_literal(&record.patch_scope),
        record.patch_preview_bytes,
        json_string_literal(&record.patch_mediation_receipt_id),
        json_string_literal(&record.human_ratification_receipt_id),
        json_string_literal(&record.human_ratifier_id),
        json_string_literal(&record.human_decision),
        json_string_literal(&record.security_check_receipt_id),
        json_string_literal(&record.review_receipt_id),
        json_string_literal(&record.stage_transition_receipt_id),
        json_string_literal(&record.policy_decision_id),
        json_string_literal(&record.deployment_authority_receipt_id),
        json_string_literal(&record.authority_expires_at),
        record.workflow_orchestration_observed,
        record.ci_evidence_observed,
        record.commit_branch_bound,
        record.security_check_observed,
        record.review_completed_observed,
        record.stage_transition_observed,
        record.patch_proposal_observed,
        record.patch_mediation_observed,
        record.human_ratification_observed,
        record.reviewer_separation_observed,
        record.deployment_authority_observed,
        record.durable_state_observed,
        record.relay_event_stream_observed,
        record.apply_preflight_observed,
        record.target_concurrent_engineers,
        record.peak_parallel_forge_builds,
        json_string_literal(&record.terminal_state),
        json_string_literal(&record.status),
        json_string_literal(&record.authority_decision),
        record.rejected,
        json_string_literal(&record.rejection_reason)
    )
}

fn render_required_authority_gates_json() -> String {
    render_string_array(&[
        "workflow_orchestration",
        "ci_evidence_observed",
        "commit_branch_bound",
        "security_check",
        "review_completed",
        "stage_transition",
        "patch_proposal",
        "patch_mediation",
        "human_ratification",
        "reviewer_separation",
        "deployment_authority_boundary",
        "durable_state",
        "relay_event_stream",
        "guarded_apply_preflight",
    ])
}

fn parse_authority_apply_request(body: &str) -> Result<AuthorityApplyRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR authority apply json: {error}"))?;
    Ok(AuthorityApplyRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", DEFAULT_JOB_ID),
        run_id: string_value(&value, "run_id", DEFAULT_RUN_ID),
        idempotency_key: string_value(&value, "idempotency_key", "dxr-authority-apply-001"),
        workflow_orchestration_id: string_value(
            &value,
            "workflow_orchestration_id",
            DEFAULT_WORKFLOW_ORCHESTRATION_ID,
        ),
        workflow_run_id: string_value(&value, "workflow_run_id", DEFAULT_WORKFLOW_RUN_ID),
        dynamic_workflow_plan_id: string_value(
            &value,
            "dynamic_workflow_plan_id",
            DEFAULT_DYNAMIC_WORKFLOW_PLAN_ID,
        ),
        execution_supervision_id: string_value(
            &value,
            "execution_supervision_id",
            DEFAULT_EXECUTION_SUPERVISION_ID,
        ),
        sandbox_result_consumption_id: string_value(
            &value,
            "sandbox_result_consumption_id",
            DEFAULT_SANDBOX_RESULT_CONSUMPTION_ID,
        ),
        ci_evidence_receipt_id: string_value(
            &value,
            "ci_evidence_receipt_id",
            DEFAULT_CI_EVIDENCE_RECEIPT_ID,
        ),
        ci_workflow_run_id: string_value(&value, "ci_workflow_run_id", DEFAULT_CI_WORKFLOW_RUN_ID),
        ci_job_id: string_value(&value, "ci_job_id", DEFAULT_CI_JOB_ID),
        ci_commit_sha: string_value(&value, "ci_commit_sha", DEFAULT_CI_COMMIT_SHA),
        ci_branch: string_value(&value, "ci_branch", DEFAULT_CI_BRANCH),
        ci_conclusion: string_value(&value, "ci_conclusion", "success"),
        ci_evidence_url: value
            .get("ci_evidence_url")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(default_ci_evidence_url),
        ci_observed_at: string_value(&value, "ci_observed_at", DEFAULT_CI_OBSERVED_AT),
        patch_proposal_id: string_value(&value, "patch_proposal_id", DEFAULT_PATCH_PROPOSAL_ID),
        patch_hash: string_value(&value, "patch_hash", DEFAULT_PATCH_HASH),
        patch_target_path: string_value(&value, "patch_target_path", DEFAULT_PATCH_TARGET_PATH),
        patch_scope: string_value(&value, "patch_scope", DEFAULT_PATCH_SCOPE),
        patch_preview_bytes: usize_value(&value, "patch_preview_bytes", 4096),
        patch_mediation_receipt_id: string_value(
            &value,
            "patch_mediation_receipt_id",
            DEFAULT_PATCH_MEDIATION_RECEIPT_ID,
        ),
        human_ratification_receipt_id: string_value(
            &value,
            "human_ratification_receipt_id",
            DEFAULT_HUMAN_RATIFICATION_RECEIPT_ID,
        ),
        human_ratifier_id: string_value(&value, "human_ratifier_id", DEFAULT_HUMAN_RATIFIER_ID),
        human_decision: string_value(&value, "human_decision", "none"),
        security_check_receipt_id: string_value(
            &value,
            "security_check_receipt_id",
            DEFAULT_SECURITY_CHECK_RECEIPT_ID,
        ),
        review_receipt_id: string_value(&value, "review_receipt_id", DEFAULT_REVIEW_RECEIPT_ID),
        stage_transition_receipt_id: string_value(
            &value,
            "stage_transition_receipt_id",
            DEFAULT_STAGE_TRANSITION_RECEIPT_ID,
        ),
        policy_decision_id: string_value(&value, "policy_decision_id", DEFAULT_POLICY_DECISION_ID),
        deployment_authority_receipt_id: string_value(
            &value,
            "deployment_authority_receipt_id",
            DEFAULT_DEPLOYMENT_AUTHORITY_RECEIPT_ID,
        ),
        authority_expires_at: string_value(
            &value,
            "authority_expires_at",
            DEFAULT_AUTHORITY_EXPIRES_AT,
        ),
        workflow_orchestration_observed: bool_value(
            &value,
            "workflow_orchestration_observed",
            true,
        ),
        ci_evidence_observed: bool_value(&value, "ci_evidence_observed", true),
        commit_branch_bound: bool_value(&value, "commit_branch_bound", true),
        security_check_observed: bool_value(&value, "security_check_observed", true),
        review_completed_observed: bool_value(&value, "review_completed_observed", true),
        stage_transition_observed: bool_value(&value, "stage_transition_observed", true),
        patch_proposal_observed: bool_value(&value, "patch_proposal_observed", false),
        patch_mediation_observed: bool_value(&value, "patch_mediation_observed", false),
        human_ratification_observed: bool_value(&value, "human_ratification_observed", false),
        reviewer_separation_observed: bool_value(&value, "reviewer_separation_observed", true),
        deployment_authority_observed: bool_value(&value, "deployment_authority_observed", false),
        durable_state_observed: bool_value(&value, "durable_state_observed", true),
        relay_event_stream_observed: bool_value(&value, "relay_event_stream_observed", true),
        apply_preflight_observed: bool_value(&value, "apply_preflight_observed", false),
        target_concurrent_engineers: usize_value(
            &value,
            "target_concurrent_engineers",
            DEFAULT_TARGET_CONCURRENT_ENGINEERS,
        ),
        peak_parallel_forge_builds: usize_value(
            &value,
            "peak_parallel_forge_builds",
            DEFAULT_PEAK_PARALLEL_FORGE_BUILDS,
        ),
        ci_claim_requested: bool_value(&value, "ci_claim_requested", false),
        patch_application_requested: bool_value(&value, "patch_application_requested", false),
        shell_execution_requested: bool_value(&value, "shell_execution_requested", false),
        git_execution_requested: bool_value(&value, "git_execution_requested", false),
        network_requested: bool_value(&value, "network_requested", false),
        secret_inheritance_requested: bool_value(&value, "secret_inheritance_requested", false),
        filesystem_mutation_requested: bool_value(&value, "filesystem_mutation_requested", false),
        deployment_requested: bool_value(&value, "deployment_requested", false),
        production_write_requested: bool_value(&value, "production_write_requested", false),
        worker_spawn_requested: bool_value(&value, "worker_spawn_requested", false),
        provider_calls_requested: bool_value(&value, "provider_calls_requested", false),
        tool_execution_requested: bool_value(&value, "tool_execution_requested", false),
    })
}

fn string_value(value: &Value, key: &str, fallback: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

fn default_ci_evidence_url() -> String {
    format!("https://github.com/{DEFAULT_CI_REPOSITORY}/actions/runs/{DEFAULT_CI_WORKFLOW_RUN_ID}")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_apply_records_ci_patch_human_rejection_and_blocks_mutation() {
        let mut runtime = DxrAuthorityApplyRuntime::new();

        let ci = runtime
            .submit_json(r#"{"idempotency_key":"ci"}"#)
            .expect("ci authority apply");
        assert!(
            ci.body
                .contains("DXR_AUTHORITY_APPLY_RECORDED_CI_EVIDENCE_READY")
        );

        let patch = runtime
            .submit_json(
                r#"{"idempotency_key":"patch","patch_proposal_observed":true,"patch_mediation_observed":true}"#,
            )
            .expect("patch authority apply");
        assert!(
            patch
                .body
                .contains("DXR_AUTHORITY_APPLY_RECORDED_PATCH_PROPOSAL_READY")
        );

        let ratified = runtime
            .submit_json(
                r#"{"idempotency_key":"ratified","patch_proposal_observed":true,"patch_mediation_observed":true,"human_ratification_observed":true,"human_decision":"approve","apply_preflight_observed":true}"#,
            )
            .expect("ratified authority apply");
        assert!(
            ratified
                .body
                .contains("DXR_AUTHORITY_APPLY_RECORDED_HUMAN_RATIFIED_APPLY_BLOCKED")
        );
        assert!(
            ratified
                .events
                .iter()
                .any(|event| event.event_type
                    == "authority_apply_guarded_apply_preflight_recorded")
        );

        let rejected_by_human = runtime
            .submit_json(
                r#"{"idempotency_key":"human-reject","patch_proposal_observed":true,"patch_mediation_observed":true,"human_ratification_observed":true,"human_decision":"reject"}"#,
            )
            .expect("human rejected authority apply");
        assert!(
            rejected_by_human
                .body
                .contains("DXR_AUTHORITY_APPLY_RECORDED_REJECTED_BY_HUMAN")
        );

        let unsafe_request = runtime
            .submit_json(
                r#"{"idempotency_key":"unsafe","ci_claim_requested":true,"patch_application_requested":true,"shell_execution_requested":true,"git_execution_requested":true,"production_write_requested":true}"#,
            )
            .expect("unsafe authority apply");
        assert!(
            unsafe_request
                .body
                .contains("DXR_AUTHORITY_APPLY_REJECTED_SECURITY_BOUNDARY")
        );

        let projection = runtime.authority_apply_json();
        assert!(projection.contains(r#""authority_apply_count":5"#));
        assert!(projection.contains(r#""ci_evidence_ready_count":1"#));
        assert!(projection.contains(r#""patch_proposal_ready_count":1"#));
        assert!(projection.contains(r#""human_ratified_count":1"#));
        assert!(projection.contains(r#""human_rejected_count":1"#));
        assert!(projection.contains(r#""rejected_count":1"#));
        assert!(projection.contains(r#""ci_evidence_consumed":true"#));
        assert!(projection.contains(r#""patch_application_allowed":false"#));
        assert!(projection.contains(r#""patch_applied":false"#));
        assert!(projection.contains(r#""production_writes_allowed":false"#));
    }
}
