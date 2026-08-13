use mdx_core::{
    ActorId, CorrelationIds, LoopId, MdxKernel, ModelPolicyContext, ModelRouteCandidate,
    ModelRouteDecision, ModelRouteRequest, TenantId, TraceId, WorkflowId, model_provider,
    model_workload, resolve_model_route,
};

pub(crate) struct ModelSelectionRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub workload_id: &'a str,
    pub preset: &'a str,
    pub policy_id: &'a str,
    pub expected_policy_version: Option<&'a str>,
    pub required_region: Option<&'a str>,
    pub required_residency: Option<&'a str>,
    pub session_id: Option<&'a str>,
    pub pinned_deployment_id: Option<&'a str>,
    pub required_context_tokens: u64,
    pub max_input_cost_microusd_per_million: u64,
    pub max_output_cost_microusd_per_million: u64,
}

pub(crate) struct ModelSelectionResult {
    pub decision: ModelRouteDecision,
    pub decision_receipt_id: String,
    pub endpoint: Option<crate::model_fabric_registry::ResolvedRegistryDeployment>,
    pub api_key: Option<String>,
    pub fallbacks: Vec<ModelExecutionTarget>,
}

pub(crate) struct ModelExecutionTarget {
    pub endpoint: crate::model_fabric_registry::ResolvedRegistryDeployment,
    pub api_key: String,
}

pub(crate) fn select(
    kernel: &mut MdxKernel,
    request: &ModelSelectionRequest<'_>,
) -> Result<ModelSelectionResult, String> {
    let registry = crate::model_fabric_registry::global();
    persist_pending_model_runtime_evidence(kernel, request.tenant_id, request.actor_id)?;
    let mut owned_candidates = registry.candidates_for_tenant(request.tenant_id)?;
    enrich_observed_metrics(
        kernel,
        request.tenant_id,
        request.workload_id,
        &mut owned_candidates,
    );
    for candidate in &mut owned_candidates {
        // A disconnected or administratively blocked deployment is already
        // ineligible. Avoid touching an external secret store for it. Besides
        // keeping selection cheap, this prevents a local Keychain prompt from
        // delaying a route that must fail closed anyway.
        let credential_available = candidate.connected
            && candidate.live_call_allowed
            && model_provider(&candidate.provider_id)
                .map(|provider| {
                    matches!(provider.provider_id, "ollama" | "vllm" | "tensorzero")
                        || crate::secret_store::global().has_credential_for_connection(
                            request.tenant_id,
                            &candidate.credential_ref,
                            provider.credential_env,
                        )
                })
                .unwrap_or(false);
        if !credential_available {
            candidate.connected = false;
            candidate.live_call_allowed = false;
        }
        if model_workload(request.workload_id).is_some_and(|workload| workload.surface == "forge")
            && !crate::forge_turn_client::TurnClient::supports_forge_turns(&candidate.provider_id)
        {
            candidate.live_call_allowed = false;
        }
    }
    let candidates = owned_candidates
        .iter()
        .map(|row| ModelRouteCandidate {
            deployment_id: &row.deployment_id,
            connection_id: &row.connection_id,
            tenant_id: &row.tenant_id,
            provider_id: &row.provider_id,
            model_id: &row.model_id,
            privacy_class: &row.privacy_class,
            region: &row.region,
            residency: &row.residency,
            data_retention: &row.data_retention,
            training_policy: &row.training_policy,
            connected: row.connected,
            healthy: row.healthy,
            live_call_allowed: row.live_call_allowed,
            tools: row.tools,
            structured_output: row.structured_output,
            vision: row.vision,
            context_tokens: row.context_tokens,
            input_cost_microusd_per_million: row.input_cost_microusd_per_million,
            output_cost_microusd_per_million: row.output_cost_microusd_per_million,
            price_known: row.price_known,
            p95_latency_ms: row.p95_latency_ms,
            latency_known: row.latency_known,
            quality_score: row.quality_score,
            quality_known: row.quality_known,
        })
        .collect::<Vec<_>>();
    let policy_id = if request.policy_id == "auto" {
        registry
            .policy_id_for_workload(request.tenant_id, request.workload_id)?
            .unwrap_or_else(|| "core".to_string())
    } else {
        request.policy_id.to_string()
    };
    let policy = registry.policy_for_tenant(
        request.tenant_id,
        &policy_id,
        request.expected_policy_version,
        request.session_id.or(Some(request.actor_id)),
    )?;
    let app_id = model_workload(request.workload_id)
        .map(|workload| workload.surface)
        .unwrap_or("unknown");
    let environment = current_environment();
    let prior_deployment_id = request.session_id.and_then(|session_id| {
        prior_deployment(kernel, request.tenant_id, request.workload_id, session_id)
    });
    let decision = resolve_model_route(&ModelRouteRequest {
        workload_id: request.workload_id,
        preset: request.preset,
        policy: &policy,
        expected_policy_version: request.expected_policy_version,
        context: ModelPolicyContext {
            tenant_id: request.tenant_id,
            app_id,
            environment: &environment,
            required_region: request.required_region,
            required_residency: request.required_residency,
            session_id: request.session_id,
            prior_deployment_id: prior_deployment_id.as_deref(),
        },
        required_context_tokens: request.required_context_tokens,
        max_input_cost_microusd_per_million: request.max_input_cost_microusd_per_million,
        max_output_cost_microusd_per_million: request.max_output_cost_microusd_per_million,
        pinned_deployment_id: request.pinned_deployment_id,
        candidates: &candidates,
    });
    let correlation = CorrelationIds {
        tenant_id: TenantId::new(request.tenant_id),
        trace_id: TraceId::new(kernel.mint_id("trace")),
        actor_id: ActorId::new(request.actor_id),
        loop_id: LoopId::new("model_fabric"),
        workflow_id: WorkflowId::new(kernel.mint_id("workflow")),
    };
    let receipt = kernel.record_model_route_decision(&correlation, &decision);
    if decision.status == "ROUTE_SELECTED" {
        let _ = crate::model_adaptive_service::record_shadow_comparisons(
            kernel,
            &crate::model_adaptive_service::ShadowComparisonContext {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                baseline_receipt_id: &receipt.receipt_id,
                baseline_deployment_id: decision.selected_deployment_id.as_deref().unwrap_or(""),
                workload_id: &decision.workload_id,
                app_id: &decision.app_id,
                environment: &decision.environment,
                session_id: (!decision.session_id.is_empty())
                    .then_some(decision.session_id.as_str()),
            },
        );
    }
    let endpoint = match decision.selected_deployment_id.as_deref() {
        Some(deployment_id) => registry.deployment_for_tenant(request.tenant_id, deployment_id)?,
        None => None,
    };
    let api_key = endpoint
        .as_ref()
        .and_then(|endpoint| credential_for_endpoint(request.tenant_id, endpoint));
    let mut fallbacks = Vec::new();
    for deployment_id in &decision.fallback_deployment_ids {
        let Some(fallback) = registry.deployment_for_tenant(request.tenant_id, deployment_id)?
        else {
            continue;
        };
        let Some(key) = credential_for_endpoint(request.tenant_id, &fallback) else {
            continue;
        };
        fallbacks.push(ModelExecutionTarget {
            endpoint: fallback,
            api_key: key,
        });
    }
    Ok(ModelSelectionResult {
        decision,
        decision_receipt_id: receipt.receipt_id,
        endpoint,
        api_key,
        fallbacks,
    })
}

fn credential_for_endpoint(
    tenant_id: &str,
    endpoint: &crate::model_fabric_registry::ResolvedRegistryDeployment,
) -> Option<String> {
    model_provider(&endpoint.connection.provider_id).and_then(|provider| {
        if matches!(provider.provider_id, "ollama" | "vllm" | "tensorzero") {
            Some(String::new())
        } else {
            crate::secret_store::global()
                .credential_for_connection(
                    tenant_id,
                    &endpoint.connection.credential_ref,
                    provider.credential_env,
                )
                .map(|credential| credential.value)
        }
    })
}

pub(crate) fn queue_failed_inference_attempt(
    tenant_id: &str,
    decision_id: &str,
    decision: &ModelRouteDecision,
    deployment_id: &str,
    latency_ms: u64,
    error: &str,
    provenance: &str,
) -> Result<(), String> {
    crate::model_fabric_registry::global().report_failed_attempt(
        tenant_id,
        crate::model_fabric_registry::FailedInferenceAttempt {
            decision_id: decision_id.to_string(),
            workload_id: decision.workload_id.clone(),
            app_id: decision.app_id.clone(),
            environment: decision.environment.clone(),
            session_id: decision.session_id.clone(),
            deployment_id: deployment_id.to_string(),
            latency_ms,
            failure_class: inference_failure_class(error).to_string(),
            provenance: provenance.to_string(),
        },
    )
}

pub(crate) fn inference_failure_class(error: &str) -> &'static str {
    let normalized = error.to_ascii_lowercase();
    if normalized.contains("paused by operator") {
        "operator_paused"
    } else if normalized.contains("deadline") || normalized.contains("timeout") {
        "deadline"
    } else if normalized.contains("429") || normalized.contains("rate limit") {
        "rate_limited"
    } else if normalized.contains("transport") || normalized.contains("connection") {
        "transport"
    } else if normalized.contains("parse") || normalized.contains("invalid response") {
        "invalid_response"
    } else if normalized.contains("adapter") || normalized.contains("unsupported") {
        "adapter_unavailable"
    } else if normalized.contains("status 4") || normalized.contains("http 4") {
        "upstream_4xx"
    } else if normalized.contains("status 5") || normalized.contains("http 5") {
        "upstream_5xx"
    } else {
        "provider_failure"
    }
}

pub(crate) fn persist_pending_model_runtime_evidence(
    kernel: &mut MdxKernel,
    tenant_id: &str,
    actor_id: &str,
) -> Result<Vec<String>, String> {
    let failures =
        crate::model_fabric_registry::global().take_pending_failed_attempts(tenant_id)?;
    let mut receipt_ids = Vec::with_capacity(failures.len());
    for failure in failures {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(tenant_id),
            trace_id: TraceId::new(kernel.mint_id("trace")),
            actor_id: ActorId::new(actor_id),
            loop_id: LoopId::new("model_fabric"),
            workflow_id: WorkflowId::new(kernel.mint_id("workflow")),
        };
        let receipt = kernel
            .record_model_outcome(
                &correlation,
                &mdx_core::ModelOutcome {
                    decision_id: failure.decision_id,
                    workload_id: failure.workload_id,
                    app_id: failure.app_id,
                    environment: failure.environment,
                    session_id: failure.session_id,
                    deployment_id: failure.deployment_id,
                    latency_ms: failure.latency_ms,
                    cost_microusd: None,
                    quality_score: None,
                    safety_status: "not_evaluated".to_string(),
                    task_status: "failed".to_string(),
                    correction_status: "not_observed".to_string(),
                    provenance: format!("{}:{}", failure.provenance, failure.failure_class),
                },
            )
            .map_err(str::to_string)?;
        receipt_ids.push(receipt.receipt_id);
    }
    let transitions =
        crate::model_fabric_registry::global().take_pending_circuit_transitions(tenant_id)?;
    receipt_ids.reserve(transitions.len());
    for transition in transitions {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(tenant_id),
            trace_id: TraceId::new(kernel.mint_id("trace")),
            actor_id: ActorId::new(actor_id),
            loop_id: LoopId::new("model_fabric"),
            workflow_id: WorkflowId::new(kernel.mint_id("workflow")),
        };
        let receipt = kernel
            .record_model_circuit_transition(
                &correlation,
                &transition.connection_id,
                &transition.deployment_id,
                &transition.state,
                transition.consecutive_failures,
                transition.open_until_epoch_seconds,
            )
            .map_err(str::to_string)?;
        receipt_ids.push(receipt.receipt_id);
    }
    Ok(receipt_ids)
}

fn enrich_observed_metrics(
    kernel: &MdxKernel,
    tenant_id: &str,
    workload_id: &str,
    candidates: &mut [crate::model_fabric_registry::OwnedRouteCandidate],
) {
    for candidate in candidates {
        let mut latencies = Vec::new();
        let mut quality_total = 0_u64;
        let mut quality_count = 0_u64;
        for receipt in kernel.ledger().entries().iter().filter(|receipt| {
            receipt.tenant_id.as_str() == tenant_id
                && receipt.kind == mdx_core::MODEL_OUTCOME_RECORDED_RECEIPT_KIND
                && receipt.payload.get("workload_id").map(String::as_str) == Some(workload_id)
                && receipt.payload.get("deployment_id").map(String::as_str)
                    == Some(candidate.deployment_id.as_str())
        }) {
            let evaluator_judgment = receipt
                .payload
                .get("provenance")
                .is_some_and(|value| value.starts_with("operator_evaluation:"));
            let execution_failed = receipt
                .payload
                .get("provenance")
                .is_some_and(|value| value.contains("_runtime_failure:"));
            if !evaluator_judgment
                && !execution_failed
                && let Some(latency) = receipt
                    .payload
                    .get("latency_ms")
                    .and_then(|value| value.parse::<u64>().ok())
            {
                latencies.push(latency);
            }
            if let Some(quality) = receipt
                .payload
                .get("quality_score")
                .and_then(|value| value.parse::<u64>().ok())
            {
                quality_total += quality;
                quality_count += 1;
            }
        }
        if !latencies.is_empty() {
            latencies.sort_unstable();
            let index = latencies.len().saturating_mul(95).div_ceil(100) - 1;
            candidate.p95_latency_ms = latencies[index];
            candidate.latency_known = true;
        }
        if let Some(average) = quality_total.checked_div(quality_count) {
            candidate.quality_score = average.min(100) as u32;
            candidate.quality_known = true;
        }
    }
}

fn prior_deployment(
    kernel: &MdxKernel,
    tenant_id: &str,
    workload_id: &str,
    session_id: &str,
) -> Option<String> {
    kernel
        .ledger()
        .entries()
        .iter()
        .rev()
        .find(|receipt| {
            receipt.kind == mdx_core::MODEL_ROUTE_SELECTED_RECEIPT_KIND
                && receipt.tenant_id.as_str() == tenant_id
                && receipt.payload.get("workload_id").map(String::as_str) == Some(workload_id)
                && receipt.payload.get("session_id").map(String::as_str) == Some(session_id)
        })
        .and_then(|receipt| receipt.payload.get("selected_deployment_id").cloned())
        .filter(|deployment_id| !deployment_id.is_empty())
}

pub(crate) fn current_environment() -> String {
    std::env::var("MDX_ENVIRONMENT")
        .or_else(|_| std::env::var("MDX_DEPLOYMENT_MODE"))
        .unwrap_or_else(|_| "local".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_attempt_is_receipted_with_a_sanitized_failure_class() {
        let tenant_id = "failed-attempt-evidence-tenant";
        let deployment_id = "failed-attempt-evidence-tenant:ollama/test";
        let registry = crate::model_fabric_registry::global();
        registry.clear_tenant(tenant_id);
        registry
            .upsert_connection(mdx_core::ProviderConnection {
                connection_id: format!("{tenant_id}:ollama"),
                tenant_id: tenant_id.to_string(),
                provider_id: "ollama".to_string(),
                credential_ref: "local:none".to_string(),
                endpoint_base_url: "http://127.0.0.1:11434/v1".to_string(),
                region: "local".to_string(),
                residency: "local".to_string(),
                data_retention: "none".to_string(),
                training_policy: "none".to_string(),
                data_policy_provenance: "test".to_string(),
                data_policy_observed_at: "2026-07-13".to_string(),
                health: "healthy".to_string(),
                live_call_allowed: true,
            })
            .expect("connection");
        registry
            .upsert_model(
                tenant_id,
                mdx_core::Model {
                    model_id: "ollama/test".to_string(),
                    provider_model_id: "test".to_string(),
                    provider_id: "ollama".to_string(),
                    display_name: "Test".to_string(),
                    lifecycle: "active".to_string(),
                    modality: "text".to_string(),
                },
            )
            .expect("model");
        registry
            .upsert_deployment(
                tenant_id,
                mdx_core::ModelDeployment {
                    deployment_id: deployment_id.to_string(),
                    connection_id: format!("{tenant_id}:ollama"),
                    provider_id: "ollama".to_string(),
                    model_id: "ollama/test".to_string(),
                    privacy_class: "local".to_string(),
                    region: "local".to_string(),
                    residency: "local".to_string(),
                    enabled: true,
                    capabilities: crate::model_fabric_registry::observed_capability_profile(
                        true,
                        true,
                        false,
                        8_192,
                        "test",
                        "2026-07-13",
                    ),
                },
                mdx_core::ModelPriceObservation {
                    deployment_id: deployment_id.to_string(),
                    input_microusd_per_million: 0,
                    output_microusd_per_million: 0,
                    currency: "USD".to_string(),
                    source: "test".to_string(),
                    observed_at: "2026-07-13".to_string(),
                },
            )
            .expect("deployment");
        let decision = ModelRouteDecision {
            status: "ROUTE_SELECTED",
            workload_id: "mdx/forge/builder".to_string(),
            app_id: "forge".to_string(),
            environment: "local".to_string(),
            session_id: "session-test".to_string(),
            preset: "balanced".to_string(),
            policy_id: "core".to_string(),
            policy_version: "core-v1".to_string(),
            selected_deployment_id: Some(deployment_id.to_string()),
            selected_provider_id: Some("ollama".to_string()),
            selected_model_id: Some("ollama/test".to_string()),
            selection_reason: "policy_objective",
            session_sticky_applied: false,
            provider_failover_deployment_ids: Vec::new(),
            model_fallback_deployment_ids: Vec::new(),
            fallback_deployment_ids: Vec::new(),
            exclusions: Vec::new(),
            grants_execution_authority: false,
        };
        queue_failed_inference_attempt(
            tenant_id,
            "decision-receipt",
            &decision,
            deployment_id,
            37,
            "HTTP 503 secret-token-must-not-land",
            "test_runtime_failure",
        )
        .expect("queue failure");
        let mut kernel = MdxKernel::boot_local();
        let receipts = persist_pending_model_runtime_evidence(&mut kernel, tenant_id, "agent:test")
            .expect("persist failure");
        assert_eq!(receipts.len(), 1);
        let receipt = kernel
            .ledger()
            .entries()
            .iter()
            .find(|receipt| receipt.receipt_id == receipts[0])
            .expect("failed attempt receipt");
        assert_eq!(receipt.kind, mdx_core::MODEL_OUTCOME_RECORDED_RECEIPT_KIND);
        assert_eq!(
            receipt.payload.get("task_status").map(String::as_str),
            Some("failed")
        );
        assert_eq!(
            receipt.payload.get("provenance").map(String::as_str),
            Some("test_runtime_failure:upstream_5xx")
        );
        assert!(
            !receipt
                .payload
                .values()
                .any(|value| value.contains("secret-token"))
        );
        registry.clear_tenant(tenant_id);
    }
}
