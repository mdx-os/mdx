use mdx_core::json_string_literal;
use serde_json::Value;

const DEFAULT_MEMORY_TIERS: [&str; 4] = [
    "working_memory",
    "episodic_memory",
    "semantic_memory",
    "procedural_memory",
];
const DEFAULT_WORLD_MODEL_SOURCE_TYPES: [&str; 4] = [
    "pages_world_model_document",
    "episodic_memory",
    "signal_intelligence",
    "rag_projection",
];
const VALID_WORLD_MODEL_TERMINAL_STATES: [&str; 3] = [
    "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_RANKED_LOCAL",
    "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_BUDGET_TRUNCATED",
    "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_VECTOR_PROVIDER_BLOCKED",
];

#[derive(Default)]
pub struct DxrCtxContextRuntime {
    inputs: Vec<DxrCtxContextInput>,
    next_input: usize,
}

pub struct DxrCtxContextResult {
    pub input: DxrCtxContextInput,
    pub events: Vec<DxrCtxContextRuntimeEvent>,
    pub body: String,
}

#[derive(Clone)]
pub struct DxrCtxContextRuntimeEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
pub struct DxrCtxContextInput {
    pub sequence: usize,
    pub ctx_context_input_id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub job_id: String,
    pub run_id: String,
    pub ctx_assembly_receipt_id: String,
    pub ctx_session_id: String,
    pub ctx_handoff_id: String,
    pub ctx_state_keys: Vec<String>,
    pub ctx_background_job_id: String,
    pub ctx_memory_tiers: Vec<String>,
    pub ctx_world_model_retrieval_id: String,
    pub ctx_world_model_terminal_state: String,
    pub ctx_world_model_source_types: Vec<String>,
    pub ctx_world_model_source_receipt_count: usize,
    pub ctx_world_model_included_source_count: usize,
    pub ctx_world_model_truncated_source_count: usize,
    pub ctx_world_model_vector_provider_blocked: bool,
    pub ctx_world_model_rank_formula: String,
    pub ctx_retrieval_dependency_observed: bool,
    pub context_payload_json: String,
    pub context_integrity_hash: String,
    pub status: String,
    pub terminal_state: String,
    pub rejected: bool,
    pub rejection_reason: String,
}

impl DxrCtxContextRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrCtxContextResult, String> {
        let value = parse_json(body)?;
        self.next_input += 1;
        let ctx_world_model_terminal_state = string_value(
            &value,
            "ctx_world_model_terminal_state",
            "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_RANKED_LOCAL",
        );
        let ctx_world_model_source_types = string_array(
            &value,
            "ctx_world_model_source_types",
            &DEFAULT_WORLD_MODEL_SOURCE_TYPES,
        );
        let source_receipt_count = usize_value(&value, "ctx_world_model_source_receipt_count", 4);
        let included_source_count = usize_value(&value, "ctx_world_model_included_source_count", 4);
        let truncated_source_count =
            usize_value(&value, "ctx_world_model_truncated_source_count", 0);
        let vector_provider_blocked =
            bool_value(&value, "ctx_world_model_vector_provider_blocked", false);
        let unsafe_authority = bool_value(&value, "provider_calls_requested", false)
            || bool_value(&value, "provider_calls_allowed", false)
            || bool_value(&value, "tool_execution_requested", false)
            || bool_value(&value, "tool_execution_allowed", false)
            || bool_value(&value, "worker_execution_requested", false)
            || bool_value(&value, "worker_execution_allowed", false)
            || bool_value(&value, "production_writes_allowed", false)
            || bool_value(&value, "embedding_values_recorded", false)
            || bool_value(&value, "secret_export_requested", false);
        let retrieval_dependency_observed = retrieval_dependency_observed(
            &ctx_world_model_terminal_state,
            &ctx_world_model_source_types,
            source_receipt_count,
            included_source_count,
        );
        let (status, terminal_state, rejected, rejection_reason) = if unsafe_authority {
            (
                "DXR_CTX_OPERATIONAL_CONTEXT_REJECTED_SECURITY_BOUNDARY",
                "DXR_CTX_OPERATIONAL_CONTEXT_REJECTED_SECURITY_BOUNDARY",
                true,
                "ctx_context_input_cannot_request_provider_tool_worker_secret_embedding_or_production_authority",
            )
        } else if !retrieval_dependency_observed {
            (
                "DXR_CTX_OPERATIONAL_CONTEXT_REJECTED_MISSING_RETRIEVAL",
                "DXR_CTX_OPERATIONAL_CONTEXT_REJECTED_MISSING_RETRIEVAL",
                true,
                "ctx_world_model_retrieval_id_terminal_state_source_mix_and_receipts_required",
            )
        } else {
            (
                "LIVE-LOCAL-DXR-CTX-OPERATIONAL-CONTEXT",
                "DXR_CTX_OPERATIONAL_CONTEXT_RECORDED_EXECUTION_BLOCKED",
                false,
                "none",
            )
        };
        let input = DxrCtxContextInput {
            sequence: self.next_input,
            ctx_context_input_id: format!("dxr_ctx_context_input_{:06}", self.next_input),
            tenant_id: string_value(&value, "tenant_id", "tenant_local"),
            actor_id: string_value(&value, "actor_id", "forge_operator"),
            job_id: string_value(&value, "job_id", "dxr_job_context_bound_001"),
            run_id: string_value(&value, "run_id", "dxr_run_context_bound_001"),
            ctx_assembly_receipt_id: string_value(
                &value,
                "ctx_assembly_receipt_id",
                "ctx_context_assembly_receipt_local_001",
            ),
            ctx_session_id: string_value(&value, "ctx_session_id", "ctx_session_local_001"),
            ctx_handoff_id: string_value(&value, "ctx_handoff_id", "ctx_handoff_local_001"),
            ctx_state_keys: string_array(
                &value,
                "ctx_state_keys",
                &["goal", "current_plan", "open_questions"],
            ),
            ctx_background_job_id: string_value(
                &value,
                "ctx_background_job_id",
                "ctx_background_job_local_001",
            ),
            ctx_memory_tiers: string_array(&value, "ctx_memory_tiers", &DEFAULT_MEMORY_TIERS),
            ctx_world_model_retrieval_id: string_value(
                &value,
                "ctx_world_model_retrieval_id",
                "ctx_world_model_retrieval_000001",
            ),
            ctx_world_model_terminal_state,
            ctx_world_model_source_types,
            ctx_world_model_source_receipt_count: source_receipt_count,
            ctx_world_model_included_source_count: included_source_count,
            ctx_world_model_truncated_source_count: truncated_source_count,
            ctx_world_model_vector_provider_blocked: vector_provider_blocked,
            ctx_world_model_rank_formula: string_value(
                &value,
                "ctx_world_model_rank_formula",
                "lexical_42_recency_24_importance_17_graph_17_plus_vector_hint",
            ),
            ctx_retrieval_dependency_observed: retrieval_dependency_observed,
            context_payload_json: context_payload_json(&value),
            context_integrity_hash: String::new(),
            status: status.to_string(),
            terminal_state: terminal_state.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let input = input.with_hash();
        let body = render_ctx_context_input_response_json(&input);
        self.inputs.push(input.clone());
        Ok(DxrCtxContextResult {
            events: ctx_context_runtime_events(&input),
            input,
            body,
        })
    }

    pub fn inputs_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-dxr-ctx-operational-context","status":"LIVE-LOCAL-DXR-CTX-OPERATIONAL-CONTEXT","runtime":"mdx-dxr-engine","ctx_runtime":"mdx-ctx-engine","route":"/dxr/ctx-context-inputs.json","submit_route":"/v1/dxr/ctx-context-inputs","input_count":{},"retrieval_dependency_count":{},"rejected_count":{},"inputs":[{}],"ctx_receipt_required":true,"ctx_world_model_retrieval_required":true,"context_hash_required":true,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
            self.inputs.len(),
            self.inputs
                .iter()
                .filter(|input| input.ctx_retrieval_dependency_observed)
                .count(),
            self.inputs.iter().filter(|input| input.rejected).count(),
            self.inputs
                .iter()
                .map(render_ctx_context_input_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

impl DxrCtxContextInput {
    fn with_hash(mut self) -> Self {
        self.context_integrity_hash = stable_hash(&[
            &self.tenant_id,
            &self.job_id,
            &self.run_id,
            &self.ctx_assembly_receipt_id,
            &self.ctx_session_id,
            &self.ctx_handoff_id,
            &self.ctx_state_keys.join("|"),
            &self.ctx_memory_tiers.join("|"),
            &self.ctx_world_model_retrieval_id,
            &self.ctx_world_model_terminal_state,
            &self.ctx_world_model_source_types.join("|"),
            &self.ctx_world_model_source_receipt_count.to_string(),
            &self.ctx_world_model_included_source_count.to_string(),
            &self.ctx_world_model_truncated_source_count.to_string(),
            if self.ctx_world_model_vector_provider_blocked {
                "vector_provider_blocked"
            } else {
                "vector_provider_not_requested"
            },
            &self.ctx_world_model_rank_formula,
            &self.context_payload_json,
        ]);
        self
    }
}

pub fn render_ctx_context_input_response_json(input: &DxrCtxContextInput) -> String {
    format!(
        r#"{{"name":"mdx-dxr-ctx-operational-context-input","status":{},"runtime":"mdx-dxr-engine","ctx_runtime":"mdx-ctx-engine","ctx_context_input":{},"ctx_context_input_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"ctx_assembly_receipt_id":{},"ctx_session_id":{},"ctx_handoff_id":{},"ctx_state_keys":{},"ctx_background_job_id":{},"ctx_memory_tiers":{},"ctx_memory_tier_count":{},"ctx_world_model_retrieval_id":{},"ctx_world_model_terminal_state":{},"ctx_world_model_source_types":{},"ctx_world_model_source_type_count":{},"ctx_world_model_source_receipt_count":{},"ctx_world_model_included_source_count":{},"ctx_world_model_truncated_source_count":{},"ctx_world_model_vector_provider_blocked":{},"ctx_world_model_rank_formula":{},"ctx_retrieval_dependency_observed":{},"context_payload":{},"context_integrity_hash":{},"terminal_state":{},"rejected":{},"rejection_reason":{},"ctx_receipt_required":true,"ctx_world_model_retrieval_required":true,"context_hash_required":true,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&input.status),
        render_ctx_context_input_json(input),
        json_string_literal(&input.ctx_context_input_id),
        json_string_literal(&input.tenant_id),
        json_string_literal(&input.actor_id),
        json_string_literal(&input.job_id),
        json_string_literal(&input.run_id),
        json_string_literal(&input.ctx_assembly_receipt_id),
        json_string_literal(&input.ctx_session_id),
        json_string_literal(&input.ctx_handoff_id),
        render_string_array_json(&input.ctx_state_keys),
        json_string_literal(&input.ctx_background_job_id),
        render_string_array_json(&input.ctx_memory_tiers),
        input.ctx_memory_tiers.len(),
        json_string_literal(&input.ctx_world_model_retrieval_id),
        json_string_literal(&input.ctx_world_model_terminal_state),
        render_string_array_json(&input.ctx_world_model_source_types),
        input.ctx_world_model_source_types.len(),
        input.ctx_world_model_source_receipt_count,
        input.ctx_world_model_included_source_count,
        input.ctx_world_model_truncated_source_count,
        input.ctx_world_model_vector_provider_blocked,
        json_string_literal(&input.ctx_world_model_rank_formula),
        input.ctx_retrieval_dependency_observed,
        input.context_payload_json,
        json_string_literal(&input.context_integrity_hash),
        json_string_literal(&input.terminal_state),
        input.rejected,
        json_string_literal(&input.rejection_reason)
    )
}

pub fn render_ctx_context_input_json(input: &DxrCtxContextInput) -> String {
    format!(
        r#"{{"sequence":{},"ctx_context_input_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"ctx_assembly_receipt_id":{},"ctx_session_id":{},"ctx_handoff_id":{},"ctx_state_keys":{},"ctx_background_job_id":{},"ctx_memory_tiers":{},"ctx_memory_tier_count":{},"ctx_world_model_retrieval_id":{},"ctx_world_model_terminal_state":{},"ctx_world_model_source_types":{},"ctx_world_model_source_type_count":{},"ctx_world_model_source_receipt_count":{},"ctx_world_model_included_source_count":{},"ctx_world_model_truncated_source_count":{},"ctx_world_model_vector_provider_blocked":{},"ctx_world_model_rank_formula":{},"ctx_retrieval_dependency_observed":{},"context_payload":{},"context_integrity_hash":{},"status":{},"terminal_state":{},"rejected":{},"rejection_reason":{},"ctx_receipt_required":true,"ctx_world_model_retrieval_required":true,"context_hash_required":true,"provider_calls_allowed":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        input.sequence,
        json_string_literal(&input.ctx_context_input_id),
        json_string_literal(&input.tenant_id),
        json_string_literal(&input.actor_id),
        json_string_literal(&input.job_id),
        json_string_literal(&input.run_id),
        json_string_literal(&input.ctx_assembly_receipt_id),
        json_string_literal(&input.ctx_session_id),
        json_string_literal(&input.ctx_handoff_id),
        render_string_array_json(&input.ctx_state_keys),
        json_string_literal(&input.ctx_background_job_id),
        render_string_array_json(&input.ctx_memory_tiers),
        input.ctx_memory_tiers.len(),
        json_string_literal(&input.ctx_world_model_retrieval_id),
        json_string_literal(&input.ctx_world_model_terminal_state),
        render_string_array_json(&input.ctx_world_model_source_types),
        input.ctx_world_model_source_types.len(),
        input.ctx_world_model_source_receipt_count,
        input.ctx_world_model_included_source_count,
        input.ctx_world_model_truncated_source_count,
        input.ctx_world_model_vector_provider_blocked,
        json_string_literal(&input.ctx_world_model_rank_formula),
        input.ctx_retrieval_dependency_observed,
        input.context_payload_json,
        json_string_literal(&input.context_integrity_hash),
        json_string_literal(&input.status),
        json_string_literal(&input.terminal_state),
        input.rejected,
        json_string_literal(&input.rejection_reason)
    )
}

fn ctx_context_runtime_events(input: &DxrCtxContextInput) -> Vec<DxrCtxContextRuntimeEvent> {
    let mut event_types = vec![
        "ctx_operational_context_recorded",
        "ctx_context_bound_to_run",
        "ctx_world_model_retrieval_dependency_recorded",
        "ctx_world_model_source_mix_bound",
    ];
    if input.ctx_world_model_vector_provider_blocked {
        event_types.push("ctx_world_model_vector_boundary_retained");
    }
    if input.rejected {
        event_types.push("ctx_context_input_rejected");
    }
    event_types
        .iter()
        .map(|event_type| DxrCtxContextRuntimeEvent {
            event_type: (*event_type).to_string(),
            tenant_id: input.tenant_id.clone(),
            job_id: input.job_id.clone(),
            run_id: input.run_id.clone(),
            actor_id: input.actor_id.clone(),
        })
        .collect()
}

fn retrieval_dependency_observed(
    terminal_state: &str,
    source_types: &[String],
    source_receipt_count: usize,
    included_source_count: usize,
) -> bool {
    VALID_WORLD_MODEL_TERMINAL_STATES.contains(&terminal_state)
        && source_receipt_count > 0
        && included_source_count > 0
        && source_types
            .iter()
            .any(|source_type| source_type == "pages_world_model_document")
        && source_types
            .iter()
            .any(|source_type| source_type.ends_with("_memory"))
        && source_types
            .iter()
            .any(|source_type| source_type == "signal_intelligence")
}

fn parse_json(body: &str) -> Result<Value, String> {
    serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR CTX operational context json: {error}"))
}

fn string_value(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn string_array(value: &Value, key: &str, defaults: &[&str]) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| defaults.iter().map(|value| (*value).to_string()).collect())
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

fn context_payload_json(value: &Value) -> String {
    value
        .get("context_payload")
        .cloned()
        .unwrap_or_else(|| {
            serde_json::json!({
                "goal": "prove DXR consumed CTX operational context",
                "boundary": "execution blocked until governed worker authority lands",
                "source": "mdx-ctx-engine"
            })
        })
        .to_string()
}

fn render_string_array_json(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| json_string_literal(value))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn stable_hash(parts: &[&str]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash ^= 0xff;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctx_context_input_preserves_receipts_and_context_shape() {
        let mut runtime = DxrCtxContextRuntime::new();
        let result = runtime
            .submit_json(
                r#"{"ctx_assembly_receipt_id":"ctx_receipt_001","ctx_state_keys":["goal"],"ctx_memory_tiers":["working_memory","semantic_memory"]}"#,
            )
            .expect("ctx context input");

        assert_eq!(result.input.ctx_assembly_receipt_id, "ctx_receipt_001");
        assert_eq!(result.input.ctx_state_keys, vec!["goal"]);
        assert_eq!(result.input.ctx_memory_tiers.len(), 2);
        assert!(result.input.ctx_retrieval_dependency_observed);
        assert_eq!(
            result.input.ctx_world_model_terminal_state,
            "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_RANKED_LOCAL"
        );
        assert_eq!(
            result.input.terminal_state,
            "DXR_CTX_OPERATIONAL_CONTEXT_RECORDED_EXECUTION_BLOCKED"
        );
        assert!(
            result
                .body
                .contains("LIVE-LOCAL-DXR-CTX-OPERATIONAL-CONTEXT")
        );
        assert!(
            runtime
                .inputs_json()
                .contains("dxr_ctx_context_input_000001")
        );
        assert!(runtime.inputs_json().contains("retrieval_dependency_count"));
    }

    #[test]
    fn ctx_context_input_rejects_missing_retrieval_dependency_and_unsafe_authority() {
        let mut runtime = DxrCtxContextRuntime::new();
        let missing = runtime
            .submit_json(
                r#"{"ctx_world_model_terminal_state":"CTX_WORLD_MODEL_RETRIEVAL_REJECTED_SECURITY_BOUNDARY","ctx_world_model_source_types":["pages_world_model_document"]}"#,
            )
            .expect("missing retrieval dependency should record rejection");
        assert_eq!(
            missing.input.terminal_state,
            "DXR_CTX_OPERATIONAL_CONTEXT_REJECTED_MISSING_RETRIEVAL"
        );
        let unsafe_input = runtime
            .submit_json(r#"{"provider_calls_requested":true,"embedding_values_recorded":true}"#)
            .expect("unsafe authority should record rejection");
        assert_eq!(
            unsafe_input.input.terminal_state,
            "DXR_CTX_OPERATIONAL_CONTEXT_REJECTED_SECURITY_BOUNDARY"
        );
        assert!(runtime.inputs_json().contains(r#""rejected_count":2"#));
    }
}
