use mdx_core::json_string_literal;
use serde_json::Value;

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "forge_operator";
const DEFAULT_JOB_ID: &str = "dxr_job_dispatch_ready_001";
const DEFAULT_RUN_ID: &str = "dxr_run_dispatch_ready_001";
const DEFAULT_WORKER_RUN_ID: &str = "dxr_worker_run_local_001";
const DEFAULT_TOOL_NAME: &str = "shell";
const DEFAULT_TOOL_KIND: &str = "shell";
const DEFAULT_POLICY_DECISION_ID: &str = "policy_tool_execution_denied_local_001";
const DEFAULT_POLICY_SOURCE_RECEIPT_ID: &str = "dxr_worker_execution_proof_000001";
const DEFAULT_WORKER_EXECUTION_PROOF_ID: &str = "dxr_worker_execution_proof_000001";

#[derive(Default)]
pub struct DxrToolExecutionRuntime {
    executions: Vec<DxrToolExecutionProof>,
    next_execution: usize,
}

pub struct DxrToolExecutionResult {
    pub body: String,
    pub events: Vec<DxrToolExecutionRuntimeEvent>,
}

#[derive(Clone)]
pub struct DxrToolExecutionRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
pub struct DxrToolExecutionProof {
    pub sequence: usize,
    pub execution_id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub job_id: String,
    pub run_id: String,
    pub worker_run_id: String,
    pub worker_execution_proof_id: String,
    pub tool_name: String,
    pub tool_kind: String,
    pub requested_outcome: String,
    pub status: String,
    pub terminal_state: String,
    pub policy_decision_id: String,
    pub policy_source_receipt_id: String,
    pub max_duration_ms: usize,
    pub observed_duration_ms: usize,
    pub tool_input_hash: String,
    pub tool_output_hash: String,
    pub human_escalation_required: bool,
}

struct ToolExecutionRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    worker_run_id: String,
    worker_execution_proof_id: String,
    tool_name: String,
    tool_kind: String,
    requested_outcome: String,
    policy_decision_id: String,
    policy_source_receipt_id: String,
    max_duration_ms: usize,
    observed_duration_ms: usize,
    tool_input_hash: String,
    tool_output_hash: String,
    tool_execution_allowed: bool,
    shell_execution_allowed: bool,
    patch_application_allowed: bool,
    git_execution_allowed: bool,
    network_allowed: bool,
    ci_claim_allowed: bool,
    deployment_allowed: bool,
    production_writes_allowed: bool,
    command_started: bool,
    external_process_started: bool,
    file_system_mutated: bool,
    secret_values_exported: bool,
}

impl DxrToolExecutionRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrToolExecutionResult, String> {
        let request = parse_tool_execution_request(body)?;
        self.next_execution += 1;
        let accepted = request.is_valid();
        let status = if accepted && request.requested_outcome == "escalated_to_human" {
            "DXR_TOOL_EXECUTION_ESCALATED_TO_HUMAN_LOCAL"
        } else if accepted {
            "DXR_TOOL_EXECUTION_DENIED_BY_POLICY_LOCAL"
        } else {
            "DXR_TOOL_EXECUTION_PROOF_REJECTED"
        };
        let terminal_state = if accepted && request.requested_outcome == "escalated_to_human" {
            "DXR_TOOL_EXECUTION_RECORDED_HUMAN_ESCALATION_AUTHORITY_BLOCKED"
        } else if accepted {
            "DXR_TOOL_EXECUTION_RECORDED_DENIED_BY_POLICY_AUTHORITY_BLOCKED"
        } else {
            "DXR_TOOL_EXECUTION_REJECTED_AUTHORITY_BLOCKED"
        };
        let proof = DxrToolExecutionProof {
            sequence: self.next_execution,
            execution_id: format!("dxr_tool_execution_{:06}", self.next_execution),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            worker_run_id: request.worker_run_id,
            worker_execution_proof_id: request.worker_execution_proof_id,
            tool_name: request.tool_name,
            tool_kind: request.tool_kind,
            requested_outcome: request.requested_outcome,
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            policy_decision_id: request.policy_decision_id,
            policy_source_receipt_id: request.policy_source_receipt_id,
            max_duration_ms: request.max_duration_ms,
            observed_duration_ms: request.observed_duration_ms,
            tool_input_hash: request.tool_input_hash,
            tool_output_hash: request.tool_output_hash,
            human_escalation_required: status == "DXR_TOOL_EXECUTION_ESCALATED_TO_HUMAN_LOCAL",
        };
        let events = tool_execution_runtime_events(&proof);
        let body = render_tool_execution_proof_response_json(&proof);
        self.executions.push(proof.clone());
        Ok(DxrToolExecutionResult { body, events })
    }

    pub fn executions_json(&self) -> String {
        let denied_count = self
            .executions
            .iter()
            .filter(|proof| proof.status == "DXR_TOOL_EXECUTION_DENIED_BY_POLICY_LOCAL")
            .count();
        let escalated_count = self
            .executions
            .iter()
            .filter(|proof| proof.status == "DXR_TOOL_EXECUTION_ESCALATED_TO_HUMAN_LOCAL")
            .count();
        format!(
            r#"{{"name":"mdx-dxr-tool-executions","status":"LIVE-LOCAL-DXR-TOOL-EXECUTION-FLOOR","runtime":"mdx-dxr-engine","route":"/dxr/tool-executions.json","submit_route":"/v1/dxr/tool-execution-proofs","execution_count":{},"denied_by_policy_count":{},"escalated_to_human_count":{},"v1_outcome_type":"ToolOutcome","allowed_outcomes":["denied_by_policy","escalated_to_human"],"tool_outcome_floor":["success","failure","timeout","denied_by_policy","escalated_to_human"],"max_duration_ms_ceiling":60000,"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"command_started":false,"external_process_started":false,"file_system_mutated":false,"secret_values_exported":false,"executions":[{}]}}"#,
            self.executions.len(),
            denied_count,
            escalated_count,
            self.executions
                .iter()
                .map(render_tool_execution_proof_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

impl ToolExecutionRequest {
    fn is_valid(&self) -> bool {
        !self.job_id.trim().is_empty()
            && !self.run_id.trim().is_empty()
            && !self.worker_run_id.trim().is_empty()
            && !self.worker_execution_proof_id.trim().is_empty()
            && !self.tool_name.trim().is_empty()
            && tool_kind_known(&self.tool_kind)
            && matches!(
                self.requested_outcome.as_str(),
                "denied_by_policy" | "escalated_to_human"
            )
            && !self.policy_decision_id.trim().is_empty()
            && !self.policy_source_receipt_id.trim().is_empty()
            && self.max_duration_ms > 0
            && self.max_duration_ms <= 60000
            && self.observed_duration_ms > 0
            && self.observed_duration_ms <= self.max_duration_ms
            && !self.tool_input_hash.trim().is_empty()
            && !self.tool_output_hash.trim().is_empty()
            && !self.tool_execution_allowed
            && !self.shell_execution_allowed
            && !self.patch_application_allowed
            && !self.git_execution_allowed
            && !self.network_allowed
            && !self.ci_claim_allowed
            && !self.deployment_allowed
            && !self.production_writes_allowed
            && !self.command_started
            && !self.external_process_started
            && !self.file_system_mutated
            && !self.secret_values_exported
    }
}

fn parse_tool_execution_request(body: &str) -> Result<ToolExecutionRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR tool execution json: {error}"))?;
    Ok(ToolExecutionRequest {
        tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
        actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
        job_id: string_value(&value, "job_id", DEFAULT_JOB_ID),
        run_id: string_value(&value, "run_id", DEFAULT_RUN_ID),
        worker_run_id: string_value(&value, "worker_run_id", DEFAULT_WORKER_RUN_ID),
        worker_execution_proof_id: string_value(
            &value,
            "worker_execution_proof_id",
            DEFAULT_WORKER_EXECUTION_PROOF_ID,
        ),
        tool_name: string_value(&value, "tool_name", DEFAULT_TOOL_NAME),
        tool_kind: string_value(&value, "tool_kind", DEFAULT_TOOL_KIND),
        requested_outcome: string_value(&value, "requested_outcome", "denied_by_policy"),
        policy_decision_id: string_value(&value, "policy_decision_id", DEFAULT_POLICY_DECISION_ID),
        policy_source_receipt_id: string_value(
            &value,
            "policy_source_receipt_id",
            DEFAULT_POLICY_SOURCE_RECEIPT_ID,
        ),
        max_duration_ms: usize_value(&value, "max_duration_ms", 5000),
        observed_duration_ms: usize_value(&value, "observed_duration_ms", 25),
        tool_input_hash: string_value(&value, "tool_input_hash", "sha256:local_tool_input_hash"),
        tool_output_hash: string_value(&value, "tool_output_hash", "sha256:local_tool_output_hash"),
        tool_execution_allowed: bool_value(&value, "tool_execution_allowed", false),
        shell_execution_allowed: bool_value(&value, "shell_execution_allowed", false),
        patch_application_allowed: bool_value(&value, "patch_application_allowed", false),
        git_execution_allowed: bool_value(&value, "git_execution_allowed", false),
        network_allowed: bool_value(&value, "network_allowed", false),
        ci_claim_allowed: bool_value(&value, "ci_claim_allowed", false),
        deployment_allowed: bool_value(&value, "deployment_allowed", false),
        production_writes_allowed: bool_value(&value, "production_writes_allowed", false),
        command_started: bool_value(&value, "command_started", false),
        external_process_started: bool_value(&value, "external_process_started", false),
        file_system_mutated: bool_value(&value, "file_system_mutated", false),
        secret_values_exported: bool_value(&value, "secret_values_exported", false),
    })
}

fn tool_execution_runtime_events(
    proof: &DxrToolExecutionProof,
) -> Vec<DxrToolExecutionRuntimeEvent> {
    let mut event_types = vec!["tool_execution_preflight_recorded"];
    match proof.status.as_str() {
        "DXR_TOOL_EXECUTION_DENIED_BY_POLICY_LOCAL" => {
            event_types.push("tool_execution_policy_denied");
            event_types.push("tool_execution_authority_blocked");
        }
        "DXR_TOOL_EXECUTION_ESCALATED_TO_HUMAN_LOCAL" => {
            event_types.push("tool_execution_human_escalated");
            event_types.push("tool_execution_authority_blocked");
        }
        _ => event_types.push("tool_execution_proof_rejected"),
    }
    event_types
        .iter()
        .map(|event_type| DxrToolExecutionRuntimeEvent {
            event_type: (*event_type).to_string(),
            tenant_id: proof.tenant_id.clone(),
            job_id: proof.job_id.clone(),
            run_id: proof.run_id.clone(),
            actor_id: proof.actor_id.clone(),
        })
        .collect()
}

pub fn render_tool_execution_proof_response_json(proof: &DxrToolExecutionProof) -> String {
    format!(
        r#"{{"name":"mdx-dxr-tool-execution","status":{},"runtime":"mdx-dxr-engine","tool_execution":{},"execution_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"worker_execution_proof_id":{},"tool_name":{},"tool_kind":{},"requested_outcome":{},"terminal_state":{},"policy_decision_id":{},"policy_source_receipt_id":{},"max_duration_ms":{},"observed_duration_ms":{},"tool_input_hash":{},"tool_output_hash":{},"human_escalation_required":{},"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"command_started":false,"external_process_started":false,"file_system_mutated":false,"secret_values_exported":false}}"#,
        json_string_literal(&proof.status),
        render_tool_execution_proof_json(proof),
        json_string_literal(&proof.execution_id),
        json_string_literal(&proof.tenant_id),
        json_string_literal(&proof.actor_id),
        json_string_literal(&proof.job_id),
        json_string_literal(&proof.run_id),
        json_string_literal(&proof.worker_run_id),
        json_string_literal(&proof.worker_execution_proof_id),
        json_string_literal(&proof.tool_name),
        json_string_literal(&proof.tool_kind),
        json_string_literal(&proof.requested_outcome),
        json_string_literal(&proof.terminal_state),
        json_string_literal(&proof.policy_decision_id),
        json_string_literal(&proof.policy_source_receipt_id),
        proof.max_duration_ms,
        proof.observed_duration_ms,
        json_string_literal(&proof.tool_input_hash),
        json_string_literal(&proof.tool_output_hash),
        proof.human_escalation_required
    )
}

pub fn render_tool_execution_proof_json(proof: &DxrToolExecutionProof) -> String {
    format!(
        r#"{{"sequence":{},"execution_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"worker_run_id":{},"worker_execution_proof_id":{},"tool_name":{},"tool_kind":{},"requested_outcome":{},"status":{},"terminal_state":{},"policy_decision_id":{},"policy_source_receipt_id":{},"max_duration_ms":{},"observed_duration_ms":{},"tool_input_hash":{},"tool_output_hash":{},"human_escalation_required":{},"tool_execution_allowed":false,"shell_execution_allowed":false,"patch_application_allowed":false,"git_execution_allowed":false,"network_allowed":false,"ci_claim_allowed":false,"deployment_allowed":false,"production_writes_allowed":false,"command_started":false,"external_process_started":false,"file_system_mutated":false,"secret_values_exported":false}}"#,
        proof.sequence,
        json_string_literal(&proof.execution_id),
        json_string_literal(&proof.tenant_id),
        json_string_literal(&proof.actor_id),
        json_string_literal(&proof.job_id),
        json_string_literal(&proof.run_id),
        json_string_literal(&proof.worker_run_id),
        json_string_literal(&proof.worker_execution_proof_id),
        json_string_literal(&proof.tool_name),
        json_string_literal(&proof.tool_kind),
        json_string_literal(&proof.requested_outcome),
        json_string_literal(&proof.status),
        json_string_literal(&proof.terminal_state),
        json_string_literal(&proof.policy_decision_id),
        json_string_literal(&proof.policy_source_receipt_id),
        proof.max_duration_ms,
        proof.observed_duration_ms,
        json_string_literal(&proof.tool_input_hash),
        json_string_literal(&proof.tool_output_hash),
        proof.human_escalation_required
    )
}

fn tool_kind_known(tool_kind: &str) -> bool {
    matches!(
        tool_kind,
        "shell"
            | "patch"
            | "git"
            | "network"
            | "ci"
            | "deployment"
            | "filesystem_read"
            | "filesystem_write"
            | "external_adapter"
            | "model_tool"
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
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(default)
}
