use crate::RouteResponse;
use mdx_core::{ActorId, CorrelationIds, LoopId, MdxKernel, TenantId, TraceId, WorkflowId};
use serde_json::{Value, json};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

const MAX_MESSAGES: usize = 64;
const MAX_INPUT_CHARS: usize = 128_000;
const MAX_SYSTEM_CHARS: usize = 32_000;
const MAX_TOOLS: usize = 32;
const MAX_TOOL_CHARS: usize = 64_000;

pub(crate) fn infer(body: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid model inference request: {error}"))?;
    let identity = crate::request_security::current_verified_identity();
    let tenant_id = identity
        .as_ref()
        .map(|identity| identity.tenant_id.clone())
        .or_else(|| value["tenant_id"].as_str().map(str::to_string))
        .unwrap_or_else(|| "local_tenant".to_string());
    let actor_id = identity
        .as_ref()
        .map(|identity| identity.actor_id.clone())
        .or_else(|| value["actor_id"].as_str().map(str::to_string))
        .unwrap_or_else(|| "human:local_user".to_string());
    let workload_id = required(&value, "workload_id")?;
    if mdx_core::model_workload(workload_id).is_none() {
        return Err("model inference requires a declared workload alias".to_string());
    }
    let messages = parse_messages(&value)?;
    let tools = parse_tools(&value)?;
    let system = value["system"].as_str().unwrap_or("");
    if system.chars().count() > MAX_SYSTEM_CHARS {
        return Err(format!(
            "model inference system instruction exceeds {MAX_SYSTEM_CHARS} characters"
        ));
    }
    let session_id = value["session_id"].as_str();
    let selected = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        crate::model_fabric_service::select(
            &mut kernel,
            &crate::model_fabric_service::ModelSelectionRequest {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                workload_id,
                preset: value["preset"].as_str().unwrap_or(""),
                policy_id: "auto",
                expected_policy_version: None,
                required_region: value["required_region"].as_str(),
                required_residency: value["required_residency"].as_str(),
                session_id,
                pinned_deployment_id: value["pinned_deployment_id"].as_str(),
                required_context_tokens: value["required_context_tokens"].as_u64().unwrap_or(0),
                max_input_cost_microusd_per_million: value["max_input_cost_microusd_per_million"]
                    .as_u64()
                    .unwrap_or(0),
                max_output_cost_microusd_per_million: value["max_output_cost_microusd_per_million"]
                    .as_u64()
                    .unwrap_or(0),
            },
        )?
    };
    if selected.decision.status != "ROUTE_SELECTED" {
        return Ok(RouteResponse::json("200 OK", json!({
            "status": selected.decision.status,
            "reason": selected.decision.selection_reason,
            "exclusions": selected.decision.exclusions.iter().map(|row| json!({"deployment_id": row.deployment_id, "reason": row.reason})).collect::<Vec<_>>(),
            "decision_receipt_id": selected.decision_receipt_id,
            "grants_execution_authority": false
        }).to_string()));
    }
    let mut targets = Vec::new();
    if let (Some(endpoint), Some(api_key)) = (selected.endpoint, selected.api_key) {
        targets.push(crate::model_fabric_service::ModelExecutionTarget { endpoint, api_key });
    }
    if value["allow_fallback"].as_bool() != Some(false) {
        targets.extend(selected.fallbacks);
    }
    let max_output_tokens = value["max_output_tokens"]
        .as_u64()
        .unwrap_or(1024)
        .clamp(1, 16_384) as u32;
    let max_cost_microusd = value["max_cost_microusd"].as_u64().unwrap_or(0);
    let estimated_input_tokens = system
        .chars()
        .count()
        .saturating_add(
            messages
                .iter()
                .map(|message| message.content.chars().count())
                .sum::<usize>(),
        )
        .div_ceil(4) as u64;
    let mut failures = Vec::new();
    for (failover_index, target) in targets.into_iter().enumerate() {
        let Some(provider) = mdx_core::model_provider(&target.endpoint.connection.provider_id)
        else {
            continue;
        };
        if max_cost_microusd > 0 {
            if target.endpoint.price.source == "unknown"
                || target.endpoint.price.source.trim().is_empty()
            {
                failures.push(format!(
                    "{}:budget_requires_known_price",
                    target.endpoint.deployment.deployment_id
                ));
                continue;
            }
            let estimated_max_cost = estimated_cost(
                estimated_input_tokens,
                u64::from(max_output_tokens),
                target.endpoint.price.input_microusd_per_million,
                target.endpoint.price.output_microusd_per_million,
            );
            if estimated_max_cost > max_cost_microusd {
                failures.push(format!(
                    "{}:budget_exceeded_before_call",
                    target.endpoint.deployment.deployment_id
                ));
                continue;
            }
        }
        let Some(protocol) =
            crate::model_gateway_runtime::protocol_for_provider(provider.provider_id)
        else {
            crate::model_fabric_service::queue_failed_inference_attempt(
                &tenant_id,
                &selected.decision_receipt_id,
                &selected.decision,
                &target.endpoint.deployment.deployment_id,
                0,
                "adapter unavailable",
                "shared_inference_runtime_failure",
            )?;
            failures.push(format!(
                "{}:adapter_unavailable",
                target.endpoint.deployment.deployment_id
            ));
            continue;
        };
        let attempt_started = Instant::now();
        let call = crate::model_gateway_runtime::execute(
            &crate::model_gateway_runtime::GatewayCallRequest {
                provider_id: provider.provider_id.to_string(),
                adapter_id: provider.adapter.to_string(),
                protocol,
                base_url: target.endpoint.connection.endpoint_base_url.clone(),
                api_key: target.api_key,
                model_id: target.endpoint.model.provider_model_id.clone(),
                system: system.to_string(),
                messages: messages.clone(),
                tools: tools.clone(),
                max_output_tokens,
                timeout: Duration::from_secs(180),
                session_id: session_id.map(str::to_string),
            },
        );
        match call {
            Ok(result) => {
                let _ = crate::model_fabric_registry::global().report_execution(
                    &tenant_id,
                    &target.endpoint.deployment.deployment_id,
                    true,
                );
                let cost = estimated_cost(
                    result.input_tokens,
                    result.output_tokens,
                    target.endpoint.price.input_microusd_per_million,
                    target.endpoint.price.output_microusd_per_million,
                );
                let outcome_id = {
                    let mut kernel = kernel
                        .write()
                        .map_err(|_| "kernel lock poisoned".to_string())?;
                    let correlation = CorrelationIds {
                        tenant_id: TenantId::new(&tenant_id),
                        trace_id: TraceId::new(kernel.mint_id("trace")),
                        actor_id: ActorId::new(&actor_id),
                        loop_id: LoopId::new("model_fabric"),
                        workflow_id: WorkflowId::new(kernel.mint_id("workflow")),
                    };
                    crate::model_fabric_service::persist_pending_model_runtime_evidence(
                        &mut kernel,
                        &tenant_id,
                        &actor_id,
                    )?;
                    kernel
                        .record_model_outcome(
                            &correlation,
                            &mdx_core::ModelOutcome {
                                decision_id: selected.decision_receipt_id.clone(),
                                workload_id: selected.decision.workload_id.clone(),
                                app_id: selected.decision.app_id.clone(),
                                environment: selected.decision.environment.clone(),
                                session_id: selected.decision.session_id.clone(),
                                deployment_id: target.endpoint.deployment.deployment_id.clone(),
                                latency_ms: result.latency_ms,
                                cost_microusd: Some(cost),
                                quality_score: None,
                                safety_status: "not_evaluated".to_string(),
                                task_status: "not_evaluated".to_string(),
                                correction_status: "not_observed".to_string(),
                                provenance: format!(
                                    "shared_inference_runtime:{}",
                                    result.inference_id
                                ),
                            },
                        )
                        .map_err(str::to_string)?
                        .receipt_id
                };
                return Ok(RouteResponse::json("200 OK", json!({
                    "status": "MODEL_INFERENCE_COMPLETED",
                    "workload_id": selected.decision.workload_id,
                    "text": result.text,
                    "tool_calls": result.tool_calls.iter().map(|call| json!({"call_id": call.call_id, "name": call.name, "arguments": call.arguments})).collect::<Vec<_>>(),
                    "provider_id": provider.provider_id,
                    "model_id": result.model_id,
                    "deployment_id": target.endpoint.deployment.deployment_id,
                    "inference_id": result.inference_id,
                    "input_tokens": result.input_tokens,
                    "output_tokens": result.output_tokens,
                    "latency_ms": result.latency_ms,
                    "cost_microusd": cost,
                    "max_cost_microusd": max_cost_microusd,
                    "finish_reason": result.finish_reason,
                    "failover_index": failover_index,
                    "decision_receipt_id": selected.decision_receipt_id,
                    "outcome_receipt_id": outcome_id,
                    "grants_execution_authority": false
                }).to_string()));
            }
            Err(error) => {
                crate::model_fabric_service::queue_failed_inference_attempt(
                    &tenant_id,
                    &selected.decision_receipt_id,
                    &selected.decision,
                    &target.endpoint.deployment.deployment_id,
                    attempt_started
                        .elapsed()
                        .as_millis()
                        .min(u128::from(u64::MAX)) as u64,
                    &error,
                    "shared_inference_runtime_failure",
                )?;
                let _ = crate::model_fabric_registry::global().report_execution(
                    &tenant_id,
                    &target.endpoint.deployment.deployment_id,
                    false,
                );
                failures.push(format!(
                    "{}:{error}",
                    target.endpoint.deployment.deployment_id
                ));
            }
        }
    }
    eprintln!(
        "model inference exhausted {} eligible route attempts",
        failures.len()
    );
    {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        crate::model_fabric_service::persist_pending_model_runtime_evidence(
            &mut kernel,
            &tenant_id,
            &actor_id,
        )?;
    }
    Ok(RouteResponse::json(
        "502 Bad Gateway",
        json!({
            "status": "MODEL_INFERENCE_FAILED",
            "attempt_count": failures.len(),
            "failure_classes": failures
                .iter()
                .map(|failure| {
                    [
                        "budget_requires_known_price",
                        "budget_exceeded_before_call",
                        "adapter_unavailable",
                    ]
                    .into_iter()
                    .find(|class| failure.ends_with(class))
                    .unwrap_or("provider_call_failed")
                })
                .collect::<Vec<_>>(),
            "max_cost_microusd": max_cost_microusd,
            "decision_receipt_id": selected.decision_receipt_id,
            "grants_execution_authority": false
        })
        .to_string(),
    ))
}

fn parse_messages(
    value: &Value,
) -> Result<Vec<crate::model_gateway_runtime::GatewayMessage>, String> {
    let rows = value["messages"]
        .as_array()
        .ok_or_else(|| "model inference requires messages".to_string())?;
    if rows.is_empty() || rows.len() > MAX_MESSAGES {
        return Err(format!(
            "model inference accepts 1 to {MAX_MESSAGES} messages"
        ));
    }
    let mut total = 0_usize;
    rows.iter()
        .map(|row| {
            let role = required(row, "role")?;
            if !matches!(role, "user" | "assistant") {
                return Err("model inference message role must be user or assistant".to_string());
            }
            let content = required(row, "content")?;
            total = total.saturating_add(content.chars().count());
            if total > MAX_INPUT_CHARS {
                return Err(format!(
                    "model inference input exceeds {MAX_INPUT_CHARS} characters"
                ));
            }
            Ok(crate::model_gateway_runtime::GatewayMessage {
                role: role.to_string(),
                content: content.to_string(),
            })
        })
        .collect()
}

fn parse_tools(value: &Value) -> Result<Vec<crate::model_gateway_runtime::GatewayTool>, String> {
    let Some(rows) = value["tools"].as_array() else {
        return Ok(Vec::new());
    };
    if rows.len() > MAX_TOOLS {
        return Err(format!("model inference accepts at most {MAX_TOOLS} tools"));
    }
    let mut total = 0_usize;
    rows.iter()
        .map(|row| {
            let name = required(row, "name")?;
            let description = row["description"].as_str().unwrap_or("");
            if name.chars().count() > 128 || description.chars().count() > 4_096 {
                return Err("model inference tool name or description is too long".to_string());
            }
            if !row["parameters"].is_object() {
                return Err(
                    "model inference tool parameters must be a JSON Schema object".to_string(),
                );
            }
            total = total
                .saturating_add(name.chars().count())
                .saturating_add(description.chars().count())
                .saturating_add(row["parameters"].to_string().chars().count());
            if total > MAX_TOOL_CHARS {
                return Err(format!(
                    "model inference tool definitions exceed {MAX_TOOL_CHARS} characters"
                ));
            }
            Ok(crate::model_gateway_runtime::GatewayTool {
                name: name.to_string(),
                description: description.to_string(),
                parameters: row["parameters"].clone(),
            })
        })
        .collect()
}

fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("model inference requires {key}"))
}

fn estimated_cost(
    input_tokens: u64,
    output_tokens: u64,
    input_price: u64,
    output_price: u64,
) -> u64 {
    (u128::from(input_tokens) * u128::from(input_price)
        + u128::from(output_tokens) * u128::from(output_price))
    .div_ceil(1_000_000)
    .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_input_is_bounded_and_roles_are_normalized() {
        let parsed = parse_messages(&json!({"messages": [{"role": "user", "content": "hello"}]}))
            .expect("messages");
        assert_eq!(parsed[0].role, "user");
        assert!(
            parse_messages(&json!({"messages": [{"role": "system", "content": "no"}]})).is_err()
        );
    }

    #[test]
    fn cost_estimate_rounds_up_to_enforce_tiny_live_budgets() {
        assert_eq!(estimated_cost(1, 0, 1, 0), 1);
        assert_eq!(estimated_cost(1_000_000, 2_000_000, 2, 3), 8);
    }
}
