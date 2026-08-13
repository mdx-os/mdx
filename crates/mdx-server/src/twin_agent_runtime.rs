use crate::RouteResponse;
use mdx_core::{MdxKernel, Receipt, json_string_literal, twin_model_gateway_provider_slots};
use std::sync::{Arc, RwLock};

const ROUTE: &str = "/twin/agent-runtime.json";
const DRAFT_KIND: &str = "twin.session.draft.admitted";
const PROVIDER_KIND: &str = "twin.model_gateway.provider.observed";
const PROVIDER_REFUSAL_KIND: &str = "twin.session.provider_call.refused";
const MEMORY_REFUSAL_KIND: &str = "twin.session.production_memory.refused";
const TOOL_REFUSAL_KIND: &str = "twin.session.tool_execution.refused";

pub(crate) fn route_response(
    method: &str,
    path: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
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
    Some(render_twin_agent_runtime_json(&kernel).map(|body| RouteResponse::json("200 OK", body)))
}

pub(crate) fn render_twin_agent_runtime_json(kernel: &MdxKernel) -> Result<String, String> {
    let provider_observation_count = receipt_count(kernel, PROVIDER_KIND);
    let accepted_provider_count = accepted_provider_count(kernel);
    let provider_connected_local_allowed = accepted_provider_count > 0;
    Ok(format!(
        r#"{{"name":"mdx-twin-agent-runtime","status":"LIVE-LOCAL-TWIN-AGENT-RUNTIME-FLOOR","route":"{}","read_only":true,"receipt_backed":true,"runtime":{{"id":"local_twin_agent_runtime","selected_mode":"companion_copilot","selected_companion_id":"local_twin_companion","selected_stance":"grounded_partner","model_gateway_driver":"local_model_gateway","model_gateway_provider":"DeterministicModelGateway","model_gateway_model_id":"deterministic_local_v1","memory_driver":"local_memory_store","memory_provider":"InMemoryProvider","world_model_source":"pages_world_model","stream_contract":"normalized_model_stream_v1","fallback_strategy":"local_first_fail_closed","provider_connected_local_allowed":{},"live_provider_call_allowed":false,"tool_execution_allowed":false,"connector_execution_allowed":false,"production_memory_write_allowed":false,"worker_spawn_allowed":false,"production_write_allowed":false}},"agent_modes":[{}],"provider_runtime":{{"provider_family":["ollama","openai","anthropic","xai","gemini","mistral","tensorzero"],"provider_slot_count":{},"provider_observation_count":{},"accepted_provider_observation_count":{},"provider_connected_local_allowed":{},"provider_call_allowed_now":{},"live_provider_call_allowed_now":{},"live_network_allowed_now":{},"credential_presence_only":true,"credential_values_recorded":false,"provider_secret_values_recorded":false,"requested_secret_values_recorded":false,"output_text_recorded":false,"provider_slots":[{}]}},"memory_runtime":{{"driver":"local_memory_store","provider":"InMemoryProvider","durable_driver":"postgres_memory_records","memory_record_count":{},"session_draft_count":{},"vendor_memory_driver":"mem0_memory_store","vendor_status":"PENDING-LIVE-RUN","production_memory_write_allowed":false,"memory_consolidation_allowed_now":true,"production_memory_turn_on_required":true}},"hooks":[{}],"skills":[{}],"connectors":[{}],"multi_agent_runtime":{{"status":"LOCAL_HANDOFF_CANDIDATES_ONLY","handoff_allowed_now":false,"swarm_launch_allowed":false,"worker_spawn_allowed":false,"external_agent_execution_allowed":false,"candidate_agents":["forge_reviewer","memory_curator","research_scout"],"required_receipt_kind":"twin.agent_handoff.approved"}},"boundary_refusals":{{"provider_call_refusal_count":{},"production_memory_refusal_count":{},"tool_execution_refusal_count":{},"provider_call_refusal_route":"/twin/provider-call-refusals.json","production_memory_refusal_route":"/twin/production-memory-refusals.json","tool_execution_refusal_route":"/twin/tool-execution-refusals.json"}},"blocked_authority":{{"live_provider_call_without_observation_allowed":false,"tool_execution_allowed":false,"connector_execution_allowed":false,"network_allowed":false,"secret_values_recorded":false,"provider_secret_values_recorded":false,"output_text_recorded":false,"production_memory_write_allowed":false,"worker_spawn_allowed":false,"agent_swarm_allowed":false,"production_write_allowed":false,"production_cutover_allowed":false}},"source_routes":["/twin/intelligence-readiness.json","/twin/model-gateway-runtime.json","/twin/model-gateway-provider-observations/projection.json","/twin/session-drafts/projection.json","/memory/records.json","/twin/provider-call-refusals/projection.json","/twin/production-memory-refusals/projection.json","/twin/tool-execution-refusals/projection.json","/v2/provider-turn-on-matrix.json"],"ui_handoff":{{"target_owner":"claude","target_surface":"apps/mdx Twin module","safe_to_render":true,"should_render_agent_modes":true,"should_render_hooks":true,"should_render_skills":true,"should_render_connectors":true,"should_render_provider_slots":true,"must_not_claim_live_tools":true,"must_not_claim_live_connectors":true,"must_not_claim_agent_swarm":true,"must_not_show_secret_values":true,"must_keep_local_deterministic_fallback_visible":true}},"evidence":{{"source_contract":"docs/TWIN-AGENT-RUNTIME-CONTRACT.md","response_schema":"generated/response-schemas/twin-agent-runtime-response.schema.json","proof_check":"make twin-agent-runtime-check","source_receipt_ids":[{}]}}}}"#,
        ROUTE,
        provider_connected_local_allowed,
        agent_modes_json(),
        twin_model_gateway_provider_slots().len(),
        provider_observation_count,
        accepted_provider_count,
        provider_connected_local_allowed,
        provider_connected_local_allowed,
        provider_connected_local_allowed,
        provider_connected_local_allowed,
        provider_slots_json(kernel),
        kernel.memory_records().len(),
        receipt_count(kernel, DRAFT_KIND),
        hooks_json(),
        skills_json(),
        connectors_json(),
        receipt_count(kernel, PROVIDER_REFUSAL_KIND),
        receipt_count(kernel, MEMORY_REFUSAL_KIND),
        receipt_count(kernel, TOOL_REFUSAL_KIND),
        source_receipt_ids_json(kernel)
    ))
}

fn agent_modes_json() -> String {
    [
        ("companion_copilot", "SELECTED_LOCAL", "none", true),
        ("research_scout", "BLOCKED_PENDING_PROVIDER_AND_CONNECTOR_AUTHORITY", "twin.research_scout.approved", false),
        ("product_partner", "BLOCKED_PENDING_PRODUCT_CONTEXT_AUTHORITY", "twin.product_partner.approved", false),
        ("memory_curator", "BLOCKED_PENDING_PRODUCTION_MEMORY_AUTHORITY", "twin.memory_curator.approved", false),
        ("forge_reviewer", "BLOCKED_PENDING_FORGE_HANDOFF_AUTHORITY", "twin.forge_reviewer.approved", false),
    ]
    .iter()
    .map(|(id, status, receipt, selected)| {
        format!(
            r#"{{"id":{},"kind":"agent_mode","status":{},"selected":{},"required_receipt_kind":{},"authority_required":{},"allowed_now":{},"execution_allowed":false,"provider_call_allowed":false,"network_allowed":false,"secret_values_recorded":false,"production_write_allowed":false}}"#,
            json_string_literal(id),
            json_string_literal(status),
            selected,
            json_string_literal(receipt),
            !selected,
            selected
        )
    })
    .collect::<Vec<_>>()
    .join(",")
}

fn hooks_json() -> String {
    runtime_items_json(&[
        (
            "session_start",
            "hook",
            "LOCAL_READY",
            "twin.hook.session_start.observed",
            true,
        ),
        (
            "pre_provider_call",
            "hook",
            "BLOCKED_PENDING_PROVIDER_TURN_ON",
            "twin.hook.pre_provider_call.approved",
            false,
        ),
        (
            "memory_consolidation",
            "hook",
            "LOCAL_READY_PRODUCTION_BLOCKED",
            "twin.hook.memory_consolidation.observed",
            true,
        ),
        (
            "pre_tool_execution",
            "hook",
            "BLOCKED_PENDING_TOOL_AUTHORITY",
            "twin.hook.pre_tool_execution.approved",
            false,
        ),
        (
            "handoff_review",
            "hook",
            "BLOCKED_PENDING_HANDOFF_AUTHORITY",
            "twin.hook.handoff_review.approved",
            false,
        ),
    ])
}

fn skills_json() -> String {
    runtime_items_json(&[
        (
            "grounded_answer",
            "skill",
            "LOCAL_READY",
            "twin.skill.grounded_answer.observed",
            true,
        ),
        (
            "memory_recall",
            "skill",
            "LOCAL_READY",
            "twin.skill.memory_recall.observed",
            true,
        ),
        (
            "provider_reasoning",
            "skill",
            "BLOCKED_PENDING_PROVIDER_TURN_ON",
            "twin.skill.provider_reasoning.approved",
            false,
        ),
        (
            "tool_use",
            "skill",
            "BLOCKED_PENDING_TOOL_AUTHORITY",
            "twin.skill.tool_use.approved",
            false,
        ),
        (
            "forge_review",
            "skill",
            "BLOCKED_PENDING_FORGE_HANDOFF_AUTHORITY",
            "twin.skill.forge_review.approved",
            false,
        ),
    ])
}

fn connectors_json() -> String {
    runtime_items_json(&[
        (
            "pages_world_model",
            "connector",
            "LOCAL_READY",
            "twin.connector.pages_world_model.observed",
            true,
        ),
        (
            "message_context",
            "connector",
            "LOCAL_READ_READY",
            "twin.connector.message_context.observed",
            true,
        ),
        (
            "provider_gateway",
            "connector",
            "BLOCKED_PENDING_PROVIDER_TURN_ON",
            "twin.connector.provider_gateway.approved",
            false,
        ),
        (
            "mem0_memory",
            "connector",
            "BLOCKED_PENDING_MEMORY_PROVIDER_TURN_ON",
            "twin.connector.mem0_memory.approved",
            false,
        ),
        (
            "external_tools",
            "connector",
            "BLOCKED_PENDING_TOOL_AUTHORITY",
            "twin.connector.external_tools.approved",
            false,
        ),
    ])
}

fn runtime_items_json(items: &[(&str, &str, &str, &str, bool)]) -> String {
    items
        .iter()
        .map(|(id, kind, status, receipt, allowed)| {
            format!(
                r#"{{"id":{},"kind":{},"status":{},"required_receipt_kind":{},"authority_required":{},"allowed_now":{},"execution_allowed":false,"provider_call_allowed":false,"network_allowed":false,"secret_values_recorded":false,"production_write_allowed":false}}"#,
                json_string_literal(id),
                json_string_literal(kind),
                json_string_literal(status),
                json_string_literal(receipt),
                !allowed,
                allowed
            )
        })
        .collect::<Vec<_>>()
        .join(",")
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
                r#"{{"provider_id":{},"adapter":{},"required_receipt_kind":{},"env_key":{},"stream_contract":{},"status":{},"env_present":{},"provider_connected_local_allowed":{},"provider_call_allowed_now":{},"credential_values_recorded":false,"provider_secret_values_recorded":false,"output_text_recorded":false}}"#,
                json_string_literal(slot.provider_id),
                json_string_literal(slot.adapter),
                json_string_literal(slot.required_receipt_kind),
                json_string_literal(slot.env_key),
                json_string_literal(slot.stream_contract),
                json_string_literal(if accepted.is_some() { "OBSERVED" } else { "PENDING_TURN_ON" }),
                env_present(slot.env_key),
                accepted.is_some(),
                accepted.is_some()
            )
        })
        .collect::<Vec<_>>()
        .join(",")
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

fn source_receipt_ids_json(kernel: &MdxKernel) -> String {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            [
                DRAFT_KIND,
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
    fn twin_agent_runtime_route_exposes_capabilities_and_blocks_authority() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let initial = route_response("GET", ROUTE, &kernel)
            .expect("agent runtime route")
            .expect("agent runtime response");
        for expected in [
            "\"name\":\"mdx-twin-agent-runtime\"",
            "\"status\":\"LIVE-LOCAL-TWIN-AGENT-RUNTIME-FLOOR\"",
            "\"selected_mode\":\"companion_copilot\"",
            "\"provider_slot_count\":7",
            "\"id\":\"provider_reasoning\"",
            "\"id\":\"external_tools\"",
            "\"agent_swarm_allowed\":false",
            "\"connector_execution_allowed\":false",
            "\"must_not_claim_live_tools\":true",
        ] {
            assert!(initial.body.contains(expected), "{expected}");
        }

        let draft = route_request_with_body(
            "POST",
            "/twin/session-drafts.json",
            r#"{"session_id":"twin_agent_runtime_test","draft_text":"Show the runtime shape."}"#,
            &kernel,
        )
        .expect("draft route");
        let source_draft_receipt_id =
            json_field(&draft.body, "draft_receipt_id").expect("draft receipt");
        route_request_with_body(
            "POST",
            "/twin/model-gateway-provider-observations.json",
            r#"{"tenant_id":"tenant_local","actor_id":"local_operator","provider_id":"openai","adapter":"OpenAIResponsesModelGateway","receipt_kind":"openai.response.observed","approval_receipt_id":"human_approval_receipt_000001","evidence_file":".mdx-local/provider-turn-on/openai-response-observed.json","model_id":"gpt-5-mini","response_id":"resp_000001","response_status":"completed","observed":true,"provider_call_attempted":true,"network_call_attempted":true,"credential_presence_only":true,"credential_values_recorded":false,"provider_secret_values_recorded":false,"requested_secret_values_recorded":false,"output_text_recorded":false,"production_write_allowed":false,"total_tokens":12}"#,
            &kernel,
        )
        .expect("provider observation route");
        for path in [
            "/twin/provider-call-refusals.json",
            "/twin/production-memory-refusals.json",
            "/twin/tool-execution-refusals.json",
        ] {
            let body = format!(
                r#"{{"session_id":"twin_agent_runtime_test","source_draft_receipt_id":"{}"}}"#,
                source_draft_receipt_id
            );
            route_request_with_body("POST", path, &body, &kernel).expect("boundary route");
        }

        let ready = route_response("GET", ROUTE, &kernel)
            .expect("agent runtime route")
            .expect("agent runtime response");
        for expected in [
            "\"provider_connected_local_allowed\":true",
            "\"accepted_provider_observation_count\":1",
            "\"provider_call_refusal_count\":1",
            "\"production_memory_refusal_count\":1",
            "\"tool_execution_refusal_count\":1",
            "\"production_write_allowed\":false",
        ] {
            assert!(ready.body.contains(expected), "{expected}");
        }
    }

    fn json_field(body: &str, key: &str) -> Option<String> {
        let after_key = body.split(&format!("\"{key}\"")).nth(1)?;
        let after_colon = after_key.split_once(':')?.1.trim_start();
        let value = after_colon.strip_prefix('"')?;
        let mut out = String::new();
        let mut escaped = false;
        for character in value.chars() {
            if escaped {
                out.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                return Some(out);
            } else {
                out.push(character);
            }
        }
        None
    }
}
