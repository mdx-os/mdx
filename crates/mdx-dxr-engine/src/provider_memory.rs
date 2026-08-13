use mdx_core::json_string_literal;
use serde_json::Value;

const STATUS: &str = "LIVE-LOCAL-DXR-PROVIDER-MEMORY-INTEGRATION-FLOOR";
const VALID_RETRIEVAL_STATES: [&str; 3] = [
    "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_RANKED_LOCAL",
    "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_BUDGET_TRUNCATED",
    "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_VECTOR_PROVIDER_BLOCKED",
];
const VALID_MEMORY_STATES: [&str; 2] = [
    "CTX_MEMORY_ADMISSION_RECORDED_ACCEPTED_LOCAL",
    "CTX_MEMORY_ADMISSION_QUEUED_BACKPRESSURE",
];

#[derive(Default)]
pub struct DxrProviderMemoryRuntime {
    integrations: Vec<DxrProviderMemoryIntegration>,
    next_integration: usize,
}

pub struct DxrProviderMemoryResult {
    pub body: String,
    pub events: Vec<DxrProviderMemoryEvent>,
}

pub struct DxrProviderMemoryEvent {
    pub event_type: String,
    pub tenant_id: String,
    pub job_id: String,
    pub run_id: String,
    pub actor_id: String,
}

#[derive(Clone)]
struct DxrProviderMemoryIntegration {
    sequence: usize,
    integration_id: String,
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    idempotency_key: String,
    ctx_context_input_id: String,
    ctx_world_model_retrieval_id: String,
    ctx_world_model_terminal_state: String,
    ctx_retrieval_dependency_observed: bool,
    ctx_memory_admission_id: String,
    ctx_memory_admission_terminal_state: String,
    ctx_memory_driver: String,
    ctx_memory_provider: String,
    ctx_provider_observation_id: String,
    ctx_provider_id: String,
    ctx_provider_observation_status: String,
    ctx_provider_required_receipt_kind: String,
    dxr_model_provider_observation_id: String,
    dxr_model_provider_status: String,
    local_model_gateway_driver: String,
    local_memory_store_driver: String,
    external_memory_provider: String,
    provider_connected_context_ready: bool,
    provider_connected_streaming_ready: bool,
    local_memory_ready: bool,
    local_model_gateway_ready: bool,
    vector_provider_blocked: bool,
    memory_provider_blocked: bool,
    integration_decision: String,
    terminal_state: String,
    rejected: bool,
    rejection_reason: String,
}

struct DxrProviderMemoryRequest {
    tenant_id: String,
    actor_id: String,
    job_id: String,
    run_id: String,
    idempotency_key: String,
    ctx_context_input_id: String,
    ctx_world_model_retrieval_id: String,
    ctx_world_model_terminal_state: String,
    ctx_retrieval_dependency_observed: bool,
    ctx_memory_admission_id: String,
    ctx_memory_admission_terminal_state: String,
    ctx_memory_driver: String,
    ctx_memory_provider: String,
    ctx_provider_observation_id: String,
    ctx_provider_id: String,
    ctx_provider_observation_status: String,
    ctx_provider_required_receipt_kind: String,
    dxr_model_provider_observation_id: String,
    dxr_model_provider_status: String,
    local_model_gateway_driver: String,
    local_memory_store_driver: String,
    external_memory_provider: String,
    provider_connected_context_ready: bool,
    provider_connected_streaming_ready: bool,
    vector_provider_blocked: bool,
    memory_provider_blocked: bool,
    provider_calls_requested: bool,
    provider_calls_allowed: bool,
    network_allowed: bool,
    secret_inheritance_allowed: bool,
    credential_values_recorded: bool,
    provider_secret_values_recorded: bool,
    requested_secret_values_recorded: bool,
    embedding_values_recorded: bool,
    output_text_recorded: bool,
    tool_execution_requested: bool,
    worker_execution_requested: bool,
    production_writes_allowed: bool,
}

impl DxrProviderMemoryRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit_json(&mut self, body: &str) -> Result<DxrProviderMemoryResult, String> {
        let request = parse_request(body)?;
        self.next_integration += 1;
        let retrieval_ready = request.ctx_retrieval_dependency_observed
            && VALID_RETRIEVAL_STATES.contains(&request.ctx_world_model_terminal_state.as_str())
            && !request.ctx_context_input_id.trim().is_empty()
            && !request.ctx_world_model_retrieval_id.trim().is_empty();
        let memory_ready = VALID_MEMORY_STATES
            .contains(&request.ctx_memory_admission_terminal_state.as_str())
            && !request.ctx_memory_admission_id.trim().is_empty()
            && request.local_memory_store_driver == "LocalPostgresMemoryStore";
        let model_ready = request.local_model_gateway_driver == "DeterministicModelGateway";
        let provider_observation_ready = request.provider_connected_context_ready
            && request.ctx_provider_observation_status == "CTX_PROVIDER_TURN_ON_OBSERVED"
            && !request.ctx_provider_observation_id.trim().is_empty();
        let streaming_observation_ready = request.provider_connected_streaming_ready
            && request.dxr_model_provider_status
                == "DXR_MODEL_PROVIDER_OBSERVED_PROVIDER_CONNECTED_LOCAL"
            && !request.dxr_model_provider_observation_id.trim().is_empty();
        let unsafe_authority = request.provider_calls_requested
            || request.provider_calls_allowed
            || request.network_allowed
            || request.secret_inheritance_allowed
            || request.credential_values_recorded
            || request.provider_secret_values_recorded
            || request.requested_secret_values_recorded
            || request.embedding_values_recorded
            || request.output_text_recorded
            || request.tool_execution_requested
            || request.worker_execution_requested
            || request.production_writes_allowed;
        let provider_blocked = !provider_observation_ready
            || !streaming_observation_ready
            || request.vector_provider_blocked;
        let memory_provider_blocked = request.memory_provider_blocked
            || request.external_memory_provider != "mem0_memory_store_observed";

        let (integration_decision, terminal_state, rejected, rejection_reason) = if unsafe_authority
        {
            (
                "rejected_security_boundary",
                "DXR_PROVIDER_MEMORY_INTEGRATION_REJECTED_SECURITY_BOUNDARY",
                true,
                "provider_network_secret_embedding_output_tool_worker_or_production_authority_requested",
            )
        } else if !retrieval_ready {
            (
                "rejected_missing_ctx_retrieval",
                "DXR_PROVIDER_MEMORY_INTEGRATION_REJECTED_MISSING_CTX_RETRIEVAL",
                true,
                "ctx_context_input_world_model_retrieval_terminal_state_and_source_dependency_required",
            )
        } else if !memory_ready {
            (
                "rejected_missing_memory_admission",
                "DXR_PROVIDER_MEMORY_INTEGRATION_REJECTED_MISSING_MEMORY_ADMISSION",
                true,
                "ctx_memory_admission_and_local_postgres_memory_store_required",
            )
        } else if !model_ready {
            (
                "rejected_missing_model_gateway",
                "DXR_PROVIDER_MEMORY_INTEGRATION_REJECTED_MISSING_MODEL_GATEWAY",
                true,
                "local_deterministic_model_gateway_required_before_vendor_provider",
            )
        } else if provider_blocked || memory_provider_blocked {
            (
                "recorded_local_drivers_ready_provider_blocked",
                "DXR_PROVIDER_MEMORY_INTEGRATION_RECORDED_LOCAL_DRIVER_READY_PROVIDER_BLOCKED",
                false,
                "none",
            )
        } else {
            (
                "recorded_provider_memory_ready_execution_blocked",
                "DXR_PROVIDER_MEMORY_INTEGRATION_RECORDED_PROVIDER_MEMORY_READY_EXECUTION_BLOCKED",
                false,
                "none",
            )
        };

        let integration = DxrProviderMemoryIntegration {
            sequence: self.next_integration,
            integration_id: format!(
                "dxr_provider_memory_integration_{:06}",
                self.next_integration
            ),
            tenant_id: request.tenant_id,
            actor_id: request.actor_id,
            job_id: request.job_id,
            run_id: request.run_id,
            idempotency_key: request.idempotency_key,
            ctx_context_input_id: request.ctx_context_input_id,
            ctx_world_model_retrieval_id: request.ctx_world_model_retrieval_id,
            ctx_world_model_terminal_state: request.ctx_world_model_terminal_state,
            ctx_retrieval_dependency_observed: request.ctx_retrieval_dependency_observed,
            ctx_memory_admission_id: request.ctx_memory_admission_id,
            ctx_memory_admission_terminal_state: request.ctx_memory_admission_terminal_state,
            ctx_memory_driver: request.ctx_memory_driver,
            ctx_memory_provider: request.ctx_memory_provider,
            ctx_provider_observation_id: request.ctx_provider_observation_id,
            ctx_provider_id: request.ctx_provider_id,
            ctx_provider_observation_status: request.ctx_provider_observation_status,
            ctx_provider_required_receipt_kind: request.ctx_provider_required_receipt_kind,
            dxr_model_provider_observation_id: request.dxr_model_provider_observation_id,
            dxr_model_provider_status: request.dxr_model_provider_status,
            local_model_gateway_driver: request.local_model_gateway_driver,
            local_memory_store_driver: request.local_memory_store_driver,
            external_memory_provider: request.external_memory_provider,
            provider_connected_context_ready: provider_observation_ready,
            provider_connected_streaming_ready: streaming_observation_ready,
            local_memory_ready: memory_ready,
            local_model_gateway_ready: model_ready,
            vector_provider_blocked: provider_blocked,
            memory_provider_blocked,
            integration_decision: integration_decision.to_string(),
            terminal_state: terminal_state.to_string(),
            rejected,
            rejection_reason: rejection_reason.to_string(),
        };
        let body = render_integration_response_json(&integration);
        let events = integration_events(&integration);
        self.integrations.push(integration);
        Ok(DxrProviderMemoryResult { body, events })
    }

    pub fn integrations_json(&self) -> String {
        let provider_memory_ready_count = self
            .integrations
            .iter()
            .filter(|integration| {
                integration.terminal_state
                    == "DXR_PROVIDER_MEMORY_INTEGRATION_RECORDED_PROVIDER_MEMORY_READY_EXECUTION_BLOCKED"
            })
            .count();
        let provider_blocked_count = self
            .integrations
            .iter()
            .filter(|integration| {
                integration.terminal_state
                    == "DXR_PROVIDER_MEMORY_INTEGRATION_RECORDED_LOCAL_DRIVER_READY_PROVIDER_BLOCKED"
            })
            .count();
        let rejected_count = self
            .integrations
            .iter()
            .filter(|integration| integration.rejected)
            .count();
        format!(
            r#"{{"name":"mdx-dxr-provider-memory-integrations","status":{},"runtime":"mdx-dxr-engine","ctx_runtime":"mdx-ctx-engine","route":"/dxr/provider-memory-integrations.json","submit_route":"/v1/dxr/provider-memory-integrations","integration_count":{},"provider_memory_ready_count":{},"provider_blocked_count":{},"rejected_count":{},"ctx_retrieval_required":true,"ctx_memory_admission_required":true,"local_memory_store_required":true,"local_model_gateway_required":true,"provider_turn_on_receipts_required":true,"local_model_gateway_driver":"DeterministicModelGateway","local_memory_store_driver":"LocalPostgresMemoryStore","external_memory_provider":"mem0_memory_store","provider_calls_allowed":false,"runtime_provider_calls_allowed_now":false,"live_network_calls_started_by_runtime":false,"credential_values_recorded":false,"provider_secret_values_recorded":false,"requested_secret_values_recorded":false,"embedding_values_recorded":false,"output_text_recorded":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false,"integrations":[{}]}}"#,
            json_string_literal(STATUS),
            self.integrations.len(),
            provider_memory_ready_count,
            provider_blocked_count,
            rejected_count,
            self.integrations
                .iter()
                .map(render_integration_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn parse_request(body: &str) -> Result<DxrProviderMemoryRequest, String> {
    let value: Value = serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid DXR provider-memory integration json: {error}"))?;
    Ok(DxrProviderMemoryRequest {
        tenant_id: string_value(&value, "tenant_id", "tenant_local"),
        actor_id: string_value(&value, "actor_id", "forge_operator"),
        job_id: string_value(&value, "job_id", "dxr_job_provider_memory_001"),
        run_id: string_value(&value, "run_id", "dxr_run_provider_memory_001"),
        idempotency_key: string_value(&value, "idempotency_key", "provider-memory-001"),
        ctx_context_input_id: string_value(
            &value,
            "ctx_context_input_id",
            "dxr_ctx_context_input_000001",
        ),
        ctx_world_model_retrieval_id: string_value(
            &value,
            "ctx_world_model_retrieval_id",
            "ctx_world_model_retrieval_000001",
        ),
        ctx_world_model_terminal_state: string_value(
            &value,
            "ctx_world_model_terminal_state",
            "CTX_WORLD_MODEL_RETRIEVAL_RECORDED_RANKED_LOCAL",
        ),
        ctx_retrieval_dependency_observed: bool_value(
            &value,
            "ctx_retrieval_dependency_observed",
            true,
        ),
        ctx_memory_admission_id: string_value(
            &value,
            "ctx_memory_admission_id",
            "ctx_memory_admission_000001",
        ),
        ctx_memory_admission_terminal_state: string_value(
            &value,
            "ctx_memory_admission_terminal_state",
            "CTX_MEMORY_ADMISSION_RECORDED_ACCEPTED_LOCAL",
        ),
        ctx_memory_driver: string_value(&value, "ctx_memory_driver", "local_memory_store"),
        ctx_memory_provider: string_value(&value, "ctx_memory_provider", "InMemoryProvider"),
        ctx_provider_observation_id: string_value(&value, "ctx_provider_observation_id", ""),
        ctx_provider_id: string_value(&value, "ctx_provider_id", "openai_embeddings"),
        ctx_provider_observation_status: string_value(
            &value,
            "ctx_provider_observation_status",
            "CTX_PROVIDER_TURN_ON_OBSERVATION_REJECTED",
        ),
        ctx_provider_required_receipt_kind: string_value(
            &value,
            "ctx_provider_required_receipt_kind",
            "openai.embedding.observed",
        ),
        dxr_model_provider_observation_id: string_value(
            &value,
            "dxr_model_provider_observation_id",
            "",
        ),
        dxr_model_provider_status: string_value(
            &value,
            "dxr_model_provider_status",
            "DXR_MODEL_PROVIDER_OBSERVATION_REJECTED_PROVIDER_BLOCKED",
        ),
        local_model_gateway_driver: string_value(
            &value,
            "local_model_gateway_driver",
            "DeterministicModelGateway",
        ),
        local_memory_store_driver: string_value(
            &value,
            "local_memory_store_driver",
            "LocalPostgresMemoryStore",
        ),
        external_memory_provider: string_value(
            &value,
            "external_memory_provider",
            "mem0_memory_store_pending_turn_on",
        ),
        provider_connected_context_ready: bool_value(
            &value,
            "provider_connected_context_ready",
            false,
        ),
        provider_connected_streaming_ready: bool_value(
            &value,
            "provider_connected_streaming_ready",
            false,
        ),
        vector_provider_blocked: bool_value(&value, "vector_provider_blocked", true),
        memory_provider_blocked: bool_value(&value, "memory_provider_blocked", true),
        provider_calls_requested: bool_value(&value, "provider_calls_requested", false),
        provider_calls_allowed: bool_value(&value, "provider_calls_allowed", false),
        network_allowed: bool_value(&value, "network_allowed", false),
        secret_inheritance_allowed: bool_value(&value, "secret_inheritance_allowed", false),
        credential_values_recorded: bool_value(&value, "credential_values_recorded", false),
        provider_secret_values_recorded: bool_value(
            &value,
            "provider_secret_values_recorded",
            false,
        ),
        requested_secret_values_recorded: bool_value(
            &value,
            "requested_secret_values_recorded",
            false,
        ),
        embedding_values_recorded: bool_value(&value, "embedding_values_recorded", false),
        output_text_recorded: bool_value(&value, "output_text_recorded", false),
        tool_execution_requested: bool_value(&value, "tool_execution_requested", false),
        worker_execution_requested: bool_value(&value, "worker_execution_requested", false),
        production_writes_allowed: bool_value(&value, "production_writes_allowed", false),
    })
}

fn integration_events(integration: &DxrProviderMemoryIntegration) -> Vec<DxrProviderMemoryEvent> {
    let mut event_types = vec![
        "provider_memory_integration_recorded",
        "ctx_memory_admission_dependency_bound",
        "ctx_retrieval_dependency_bound",
        "local_driver_conformance_bound",
    ];
    if integration.provider_connected_context_ready
        && integration.provider_connected_streaming_ready
    {
        event_types.push("provider_memory_turn_on_evidence_bound");
    } else {
        event_types.push("provider_memory_turn_on_boundary_retained");
    }
    if integration.rejected {
        event_types.push("provider_memory_integration_rejected");
    }
    event_types
        .iter()
        .map(|event_type| DxrProviderMemoryEvent {
            event_type: (*event_type).to_string(),
            tenant_id: integration.tenant_id.clone(),
            job_id: integration.job_id.clone(),
            run_id: integration.run_id.clone(),
            actor_id: integration.actor_id.clone(),
        })
        .collect()
}

fn render_integration_response_json(integration: &DxrProviderMemoryIntegration) -> String {
    format!(
        r#"{{"name":"mdx-dxr-provider-memory-integration","status":{},"runtime":"mdx-dxr-engine","ctx_runtime":"mdx-ctx-engine","integration":{},"integration_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"idempotency_key":{},"terminal_state":{},"integration_decision":{},"rejected":{},"rejection_reason":{},"ctx_retrieval_required":true,"ctx_memory_admission_required":true,"local_memory_store_required":true,"local_model_gateway_required":true,"provider_turn_on_receipts_required":true,"provider_calls_allowed":false,"runtime_provider_calls_allowed_now":false,"live_network_calls_started_by_runtime":false,"credential_values_recorded":false,"provider_secret_values_recorded":false,"requested_secret_values_recorded":false,"embedding_values_recorded":false,"output_text_recorded":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(STATUS),
        render_integration_json(integration),
        json_string_literal(&integration.integration_id),
        json_string_literal(&integration.tenant_id),
        json_string_literal(&integration.actor_id),
        json_string_literal(&integration.job_id),
        json_string_literal(&integration.run_id),
        json_string_literal(&integration.idempotency_key),
        json_string_literal(&integration.terminal_state),
        json_string_literal(&integration.integration_decision),
        integration.rejected,
        json_string_literal(&integration.rejection_reason)
    )
}

fn render_integration_json(integration: &DxrProviderMemoryIntegration) -> String {
    format!(
        r#"{{"sequence":{},"integration_id":{},"tenant_id":{},"actor_id":{},"job_id":{},"run_id":{},"idempotency_key":{},"ctx_context_input_id":{},"ctx_world_model_retrieval_id":{},"ctx_world_model_terminal_state":{},"ctx_retrieval_dependency_observed":{},"ctx_memory_admission_id":{},"ctx_memory_admission_terminal_state":{},"ctx_memory_driver":{},"ctx_memory_provider":{},"ctx_provider_observation_id":{},"ctx_provider_id":{},"ctx_provider_observation_status":{},"ctx_provider_required_receipt_kind":{},"dxr_model_provider_observation_id":{},"dxr_model_provider_status":{},"local_model_gateway_driver":{},"local_memory_store_driver":{},"external_memory_provider":{},"provider_connected_context_ready":{},"provider_connected_streaming_ready":{},"local_memory_ready":{},"local_model_gateway_ready":{},"vector_provider_blocked":{},"memory_provider_blocked":{},"integration_decision":{},"terminal_state":{},"rejected":{},"rejection_reason":{},"provider_calls_allowed":false,"runtime_provider_calls_allowed_now":false,"live_network_calls_started_by_runtime":false,"credential_values_recorded":false,"provider_secret_values_recorded":false,"requested_secret_values_recorded":false,"embedding_values_recorded":false,"output_text_recorded":false,"tool_execution_allowed":false,"worker_execution_allowed":false,"production_writes_allowed":false}}"#,
        integration.sequence,
        json_string_literal(&integration.integration_id),
        json_string_literal(&integration.tenant_id),
        json_string_literal(&integration.actor_id),
        json_string_literal(&integration.job_id),
        json_string_literal(&integration.run_id),
        json_string_literal(&integration.idempotency_key),
        json_string_literal(&integration.ctx_context_input_id),
        json_string_literal(&integration.ctx_world_model_retrieval_id),
        json_string_literal(&integration.ctx_world_model_terminal_state),
        integration.ctx_retrieval_dependency_observed,
        json_string_literal(&integration.ctx_memory_admission_id),
        json_string_literal(&integration.ctx_memory_admission_terminal_state),
        json_string_literal(&integration.ctx_memory_driver),
        json_string_literal(&integration.ctx_memory_provider),
        json_string_literal(&integration.ctx_provider_observation_id),
        json_string_literal(&integration.ctx_provider_id),
        json_string_literal(&integration.ctx_provider_observation_status),
        json_string_literal(&integration.ctx_provider_required_receipt_kind),
        json_string_literal(&integration.dxr_model_provider_observation_id),
        json_string_literal(&integration.dxr_model_provider_status),
        json_string_literal(&integration.local_model_gateway_driver),
        json_string_literal(&integration.local_memory_store_driver),
        json_string_literal(&integration.external_memory_provider),
        integration.provider_connected_context_ready,
        integration.provider_connected_streaming_ready,
        integration.local_memory_ready,
        integration.local_model_gateway_ready,
        integration.vector_provider_blocked,
        integration.memory_provider_blocked,
        json_string_literal(&integration.integration_decision),
        json_string_literal(&integration.terminal_state),
        integration.rejected,
        json_string_literal(&integration.rejection_reason)
    )
}

fn bool_value(value: &Value, field: &str, default: bool) -> bool {
    value.get(field).and_then(Value::as_bool).unwrap_or(default)
}

fn string_value(value: &Value, field: &str, default: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(default)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_local_driver_ready_provider_blocked_packet() {
        let mut runtime = DxrProviderMemoryRuntime::new();
        let result = runtime
            .submit_json(
                r#"{"ctx_context_input_id":"dxr_ctx_context_input_000001","ctx_memory_admission_id":"ctx_memory_admission_000001"}"#,
            )
            .expect("provider memory integration");

        assert!(result.body.contains(
            "DXR_PROVIDER_MEMORY_INTEGRATION_RECORDED_LOCAL_DRIVER_READY_PROVIDER_BLOCKED"
        ));
        assert!(result.body.contains(r#""provider_calls_allowed":false"#));
        assert!(
            runtime
                .integrations_json()
                .contains(r#""provider_blocked_count":1"#)
        );
    }

    #[test]
    fn records_provider_memory_ready_but_execution_blocked() {
        let mut runtime = DxrProviderMemoryRuntime::new();
        let result = runtime
            .submit_json(
                r#"{"ctx_provider_observation_id":"ctx_provider_observation_000001","ctx_provider_observation_status":"CTX_PROVIDER_TURN_ON_OBSERVED","ctx_provider_required_receipt_kind":"openai.embedding.observed","dxr_model_provider_observation_id":"dxr_model_provider_observation_000001","dxr_model_provider_status":"DXR_MODEL_PROVIDER_OBSERVED_PROVIDER_CONNECTED_LOCAL","external_memory_provider":"mem0_memory_store_observed","provider_connected_context_ready":true,"provider_connected_streaming_ready":true,"vector_provider_blocked":false,"memory_provider_blocked":false}"#,
            )
            .expect("provider ready integration");

        assert!(result.body.contains(
            "DXR_PROVIDER_MEMORY_INTEGRATION_RECORDED_PROVIDER_MEMORY_READY_EXECUTION_BLOCKED"
        ));
        assert!(
            runtime
                .integrations_json()
                .contains(r#""provider_memory_ready_count":1"#)
        );
    }

    #[test]
    fn rejects_missing_retrieval_and_unsafe_authority() {
        let mut runtime = DxrProviderMemoryRuntime::new();
        let missing = runtime
            .submit_json(
                r#"{"ctx_retrieval_dependency_observed":false,"ctx_world_model_terminal_state":"CTX_WORLD_MODEL_RETRIEVAL_REJECTED_SECURITY_BOUNDARY"}"#,
            )
            .expect("missing retrieval");
        assert!(
            missing
                .body
                .contains("DXR_PROVIDER_MEMORY_INTEGRATION_REJECTED_MISSING_CTX_RETRIEVAL")
        );
        let unsafe_result = runtime
            .submit_json(r#"{"provider_calls_requested":true,"credential_values_recorded":true}"#)
            .expect("unsafe boundary");
        assert!(
            unsafe_result
                .body
                .contains("DXR_PROVIDER_MEMORY_INTEGRATION_REJECTED_SECURITY_BOUNDARY")
        );
        assert!(
            runtime
                .integrations_json()
                .contains(r#""rejected_count":2"#)
        );
    }
}
