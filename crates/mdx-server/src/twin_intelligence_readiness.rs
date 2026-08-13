use crate::RouteResponse;
use mdx_core::{MdxKernel, Receipt, json_string_literal, twin_model_gateway_provider_slots};
use std::sync::{Arc, RwLock};

#[path = "twin_agent_runtime.rs"]
mod twin_agent_runtime;

const ROUTE: &str = "/twin/intelligence-readiness.json";
const DRAFT_KIND: &str = "twin.session.draft.admitted";
const MEMORY_GATE_KIND: &str = "twin.session.draft.memory_consolidated";
const MEMORY_RETRIEVAL_KIND: &str = "twin.session.memory.retrieved";
const MEMORY_SCORE_KIND: &str = "twin.session.memory.scored";
const ANSWER_KIND: &str = "twin.session.answer.grounded";
const PERSONA_DRIFT_KIND: &str = "twin.session.persona.drift_checked";
const SUMMARY_KIND: &str = "twin.session.conversation.summarized";
const PROVIDER_KIND: &str = "twin.model_gateway.provider.observed";
const PROVIDER_REFUSAL_KIND: &str = "twin.session.provider_call.refused";
const MEMORY_REFUSAL_KIND: &str = "twin.session.production_memory.refused";
const TOOL_REFUSAL_KIND: &str = "twin.session.tool_execution.refused";

pub(crate) fn route_response(
    method: &str,
    path: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if let Some(response) = twin_agent_runtime::route_response(method, path, kernel) {
        return Some(response);
    }
    if path != ROUTE {
        return None;
    }
    if !method.eq_ignore_ascii_case("GET") {
        return Some(Ok(RouteResponse::text(
            "405 Method Not Allowed",
            "method not allowed\n".to_string(),
        )));
    }
    let kernel = match kernel.read() {
        Ok(kernel) => kernel,
        Err(_) => return Some(Err("kernel lock poisoned".to_string())),
    };
    Some(
        render_intelligence_readiness_json(&kernel).map(|body| RouteResponse::json("200 OK", body)),
    )
}

pub(crate) fn render_intelligence_readiness_json(kernel: &MdxKernel) -> Result<String, String> {
    let draft_count = receipt_count(kernel, DRAFT_KIND);
    let memory_gate_count = receipt_count(kernel, MEMORY_GATE_KIND);
    let retrieval_count = receipt_count(kernel, MEMORY_RETRIEVAL_KIND);
    let scoring_count = receipt_count(kernel, MEMORY_SCORE_KIND);
    let answer_count = receipt_count(kernel, ANSWER_KIND);
    let persona_drift_count = receipt_count(kernel, PERSONA_DRIFT_KIND);
    let summary_count = receipt_count(kernel, SUMMARY_KIND);
    let provider_observation_count = receipt_count(kernel, PROVIDER_KIND);
    let accepted_provider_count = accepted_provider_count(kernel);
    let provider_refusal_count = receipt_count(kernel, PROVIDER_REFUSAL_KIND);
    let memory_refusal_count = receipt_count(kernel, MEMORY_REFUSAL_KIND);
    let tool_refusal_count = receipt_count(kernel, TOOL_REFUSAL_KIND);
    let conversation_ready = draft_count > 0
        && memory_gate_count > 0
        && retrieval_count > 0
        && scoring_count > 0
        && answer_count > 0
        && persona_drift_count > 0
        && summary_count > 0;
    let boundary_ready =
        provider_refusal_count > 0 && memory_refusal_count > 0 && tool_refusal_count > 0;
    let provider_connected_local_allowed = accepted_provider_count > 0;
    let local_ready = conversation_ready && boundary_ready && provider_connected_local_allowed;
    let status = if local_ready {
        "LIVE-LOCAL-TWIN-INTELLIGENCE-READY-PROVIDER-CONTROLLED"
    } else {
        "LOCAL-TWIN-INTELLIGENCE-MISSING-GOVERNED-EVIDENCE"
    };
    Ok(format!(
        r#"{{"name":"mdx-twin-intelligence-readiness","status":{},"route":"{}","read_only":true,"receipt_backed":true,"local_ready":{},"conversation_ready":{},"provider_connected_local_allowed":{},"local_model_gateway":{{"driver":"local_model_gateway","provider":"DeterministicModelGateway","model_id":"deterministic_local_v1","stream_contract":"normalized_model_stream_v1","routing":"local_first_fail_closed","provider_call_allowed":false,"network_allowed":false,"output_text_recorded":false}},"provider_runtime":{{"observation_route":"/twin/model-gateway-provider-observations.json","projection_route":"/twin/model-gateway-provider-observations/projection.json","provider_slot_count":{},"provider_observation_count":{},"accepted_provider_observation_count":{},"provider_connected_local_allowed":{},"provider_call_allowed_now":{},"live_network_allowed_now":{},"credential_presence_only":true,"credential_values_recorded":false,"provider_secret_values_recorded":false,"requested_secret_values_recorded":false,"output_text_recorded":false,"production_write_allowed":false,"provider_slots":[{}]}},"memory_runtime":{{"driver":"local_memory_store","provider":"InMemoryProvider","durable_driver":"postgres_memory_records","memory_record_count":{},"memory_gate_count":{},"memory_retrieval_count":{},"memory_scoring_count":{},"world_model_source":"generated/world-model/pages-projection-fixtures.json","vendor_memory_driver":"mem0_memory_store","vendor_status":"PENDING-LIVE-RUN","production_memory_write_allowed":false}},"conversation_runtime":{{"session_draft_route":"/twin/session-drafts.json","projection_route":"/twin/session-drafts/projection.json","draft_count":{},"grounded_answer_count":{},"persona_drift_count":{},"conversation_summary_count":{},"created_at_projected":true,"draft_text_projected":true,"provider_call_allowed":false,"worker_spawn_allowed":false,"production_write_allowed":false}},"boundary_refusals":{{"provider_call_refusal_count":{},"production_memory_refusal_count":{},"tool_execution_refusal_count":{},"required_receipt_kinds":["{}","{}","{}"],"source_receipt_ids":[{}]}},"blocked_authority":{{"live_provider_call_without_observation_allowed":false,"provider_secret_values_recorded":false,"requested_secret_values_recorded":false,"output_text_recorded":false,"production_memory_write_allowed":false,"tool_execution_allowed":false,"worker_spawn_allowed":false,"production_write_allowed":false,"production_cutover_allowed":false}},"source_routes":["/twin/session-drafts/projection.json","/twin/model-gateway-runtime.json","/twin/model-gateway-provider-observations/projection.json","/memory/records.json","/twin/provider-call-refusals/projection.json","/twin/production-memory-refusals/projection.json","/twin/tool-execution-refusals/projection.json"],"ui_handoff":{{"target_owner":"claude","target_surface":"apps/mdx Twin module","safe_to_show_local_intelligence_ready":{},"must_render_provider_slots":true,"must_not_show_secret_values":true,"must_not_render_output_text_from_provider_observation":true,"must_not_claim_production_provider_cutover":true,"must_keep_local_deterministic_fallback_visible":true,"must_keep_tool_execution_blocked":true}},"checks":["make twin-intelligence-readiness-check","make model-gateway-driver-check","make memory-store-driver-check","make twin-session-draft-check"]}}"#,
        json_string_literal(status),
        ROUTE,
        local_ready,
        conversation_ready,
        provider_connected_local_allowed,
        twin_model_gateway_provider_slots().len(),
        provider_observation_count,
        accepted_provider_count,
        provider_connected_local_allowed,
        provider_connected_local_allowed,
        provider_connected_local_allowed,
        provider_slots_json(kernel),
        kernel.memory_records().len(),
        memory_gate_count,
        retrieval_count,
        scoring_count,
        draft_count,
        answer_count,
        persona_drift_count,
        summary_count,
        provider_refusal_count,
        memory_refusal_count,
        tool_refusal_count,
        PROVIDER_REFUSAL_KIND,
        MEMORY_REFUSAL_KIND,
        TOOL_REFUSAL_KIND,
        source_receipt_ids_json(kernel),
        local_ready
    ))
}

fn receipt_count(kernel: &MdxKernel, kind: &str) -> usize {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == kind)
        .count()
}

fn accepted_provider_count(kernel: &MdxKernel) -> usize {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            receipt.kind == PROVIDER_KIND
                && receipt
                    .payload
                    .get("provider_connected_local_allowed")
                    .map(String::as_str)
                    == Some("true")
        })
        .count()
}

fn provider_slots_json(kernel: &MdxKernel) -> String {
    twin_model_gateway_provider_slots()
        .iter()
        .map(|slot| {
            let accepted = kernel.ledger().entries().iter().find(|receipt| {
                receipt.kind == PROVIDER_KIND
                    && payload_value(receipt, "provider_id") == slot.provider_id
                    && payload_value(receipt, "provider_connected_local_allowed") == "true"
            });
            format!(
                r#"{{"provider_id":{},"adapter":{},"required_receipt_kind":{},"env_key":{},"stream_contract":{},"turn_on_signal":{},"status":{},"env_present":{},"provider_connected_local_allowed":{},"observation_receipt_id":{},"model_id":{},"credential_values_recorded":false,"provider_secret_values_recorded":false,"output_text_recorded":false}}"#,
                json_string_literal(slot.provider_id),
                json_string_literal(slot.adapter),
                json_string_literal(slot.required_receipt_kind),
                json_string_literal(slot.env_key),
                json_string_literal(slot.stream_contract),
                json_string_literal(slot.turn_on_signal),
                json_string_literal(if accepted.is_some() { "OBSERVED" } else { "PENDING_TURN_ON" }),
                env_present(slot.env_key),
                accepted.is_some(),
                json_string_literal(accepted.map(|receipt| receipt.receipt_id.as_str()).unwrap_or("")),
                json_string_literal(accepted.map(|receipt| payload_value(receipt, "model_id")).unwrap_or(""))
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn source_receipt_ids_json(kernel: &MdxKernel) -> String {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            [
                DRAFT_KIND,
                MEMORY_GATE_KIND,
                MEMORY_RETRIEVAL_KIND,
                MEMORY_SCORE_KIND,
                ANSWER_KIND,
                PERSONA_DRIFT_KIND,
                SUMMARY_KIND,
                PROVIDER_KIND,
                PROVIDER_REFUSAL_KIND,
                MEMORY_REFUSAL_KIND,
                TOOL_REFUSAL_KIND,
            ]
            .contains(&receipt.kind.as_str())
        })
        .map(receipt_id_json)
        .collect::<Vec<_>>()
        .join(",")
}

fn receipt_id_json(receipt: &Receipt) -> String {
    json_string_literal(&receipt.receipt_id)
}

fn payload_value<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn env_present(key: &str) -> bool {
    std::env::var(key)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_request_with_body;

    #[test]
    fn twin_intelligence_readiness_reports_missing_then_ready_with_boundaries() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let empty = route_response("GET", ROUTE, &kernel)
            .expect("readiness route")
            .expect("readiness response");
        for expected in [
            "\"status\":\"LOCAL-TWIN-INTELLIGENCE-MISSING-GOVERNED-EVIDENCE\"",
            "\"local_ready\":false",
            "\"provider_connected_local_allowed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(empty.body.contains(expected), "{expected}");
        }

        let draft = route_request_with_body(
            "POST",
            "/twin/session-drafts.json",
            r#"{"session_id":"twin_intelligence_readiness_test","draft_text":"Help me reason about provider turn-on safely."}"#,
            &kernel,
        )
        .expect("draft route");
        let source_draft_receipt_id =
            json_field(&draft.body, "draft_receipt_id").expect("draft receipt");
        let observation = route_request_with_body(
            "POST",
            "/twin/model-gateway-provider-observations.json",
            r#"{"tenant_id":"tenant_local","actor_id":"local_operator","provider_id":"openai","adapter":"OpenAIResponsesModelGateway","receipt_kind":"openai.response.observed","approval_receipt_id":"human_approval_receipt_000001","evidence_file":".mdx-local/provider-turn-on/openai-response-observed.json","model_id":"gpt-5-mini","response_id":"resp_000001","response_status":"completed","observed":true,"provider_call_attempted":true,"network_call_attempted":true,"credential_presence_only":true,"credential_values_recorded":false,"provider_secret_values_recorded":false,"requested_secret_values_recorded":false,"output_text_recorded":false,"production_write_allowed":false,"total_tokens":12}"#,
            &kernel,
        )
        .expect("provider observation route");
        assert!(
            observation
                .body
                .contains("\"provider_connected_local_allowed\":true")
        );
        for path in [
            "/twin/provider-call-refusals.json",
            "/twin/production-memory-refusals.json",
            "/twin/tool-execution-refusals.json",
        ] {
            let body = format!(
                r#"{{"session_id":"twin_intelligence_readiness_test","source_draft_receipt_id":"{}"}}"#,
                source_draft_receipt_id
            );
            let response =
                route_request_with_body("POST", path, &body, &kernel).expect("boundary route");
            assert!(response.body.contains("\"production_write_allowed\":false"));
        }

        let ready = route_response("GET", ROUTE, &kernel)
            .expect("readiness route")
            .expect("readiness response");
        for expected in [
            "\"status\":\"LIVE-LOCAL-TWIN-INTELLIGENCE-READY-PROVIDER-CONTROLLED\"",
            "\"local_ready\":true",
            "\"conversation_ready\":true",
            "\"provider_connected_local_allowed\":true",
            "\"accepted_provider_observation_count\":1",
            "\"memory_retrieval_count\":1",
            "\"grounded_answer_count\":1",
            "\"provider_call_refusal_count\":1",
            "\"production_memory_refusal_count\":1",
            "\"tool_execution_refusal_count\":1",
            "\"must_not_show_secret_values\":true",
            "\"must_not_claim_production_provider_cutover\":true",
            "\"production_cutover_allowed\":false",
        ] {
            assert!(ready.body.contains(expected), "{expected}");
        }
    }

    fn json_field(body: &str, key: &str) -> Option<String> {
        let after_key = body.split(&format!("\"{key}\"")).nth(1)?;
        let after_colon = after_key.split_once(':')?.1.trim_start();
        let value = after_colon.strip_prefix('"')?;
        Some(value.split('"').next()?.to_string())
    }
}
