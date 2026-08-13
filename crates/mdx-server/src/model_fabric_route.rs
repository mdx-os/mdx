use crate::RouteResponse;
use mdx_core::{
    ActorId, CapabilityProfile, CorrelationIds, LoopId, MdxKernel, Model, ModelDeployment,
    ModelPriceObservation, ProviderConnection, RoutePolicy, TenantId, TraceId, WorkflowId,
    model_provider,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::sync::{Arc, RwLock};

const FABRIC_JSON: &str = include_str!("../../../generated/architecture/model-fabric.json");
const DEFAULT_ACCESS_MODES: &str = "byok,managed,enterprise,local";
const DEFAULT_MAX_LIVE_TEST_MICROUSD: u64 = 100_000;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ModelAccessPolicy {
    modes: BTreeSet<String>,
    providers: BTreeSet<String>,
    max_live_test_microusd: u64,
}

impl ModelAccessPolicy {
    fn as_json(&self) -> Value {
        json!({
            "name": "mdx-model-access-policy",
            "allowed_connection_modes": self.modes,
            "allowed_providers": self.providers,
            "max_live_test_microusd": self.max_live_test_microusd,
            "credential_backends": ["session", "local_keychain", "environment", "mounted", "managed", "broker"],
            "tenant_scoped": true,
            "secret_values_exposed": false,
            "grants_execution_authority": false
        })
    }
}

fn csv_set(value: &str) -> BTreeSet<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| entry.to_ascii_lowercase())
        .collect()
}

fn access_policy() -> ModelAccessPolicy {
    let modes = std::env::var("MDX_MODEL_ACCESS_MODES")
        .ok()
        .map(|value| csv_set(&value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| csv_set(DEFAULT_ACCESS_MODES));
    let providers = std::env::var("MDX_MODEL_PROVIDER_ALLOWLIST")
        .ok()
        .map(|value| csv_set(&value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            mdx_core::MODEL_PROVIDER_CATALOG
                .iter()
                .map(|provider| provider.provider_id.to_string())
                .collect()
        });
    let max_live_test_microusd = std::env::var("MDX_MODEL_MAX_LIVE_TEST_MICROUSD")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0 && *value <= DEFAULT_MAX_LIVE_TEST_MICROUSD)
        .unwrap_or(DEFAULT_MAX_LIVE_TEST_MICROUSD);
    ModelAccessPolicy {
        modes,
        providers,
        max_live_test_microusd,
    }
}

fn connection_access_mode(
    provider_id: &str,
    supplied_api_key: &str,
    credential_ref: &str,
) -> &'static str {
    if matches!(provider_id, "ollama" | "vllm" | "tensorzero") {
        "local"
    } else if credential_ref.starts_with("secret:managed/") {
        "managed"
    } else if !supplied_api_key.is_empty()
        || credential_ref.starts_with("secret:session/")
        || credential_ref.starts_with("secret:local_keychain/")
        || credential_ref.starts_with("keychain:")
    {
        "byok"
    } else {
        "enterprise"
    }
}

fn enforce_model_access(provider_id: &str, mode: &str) -> Result<(), String> {
    let policy = access_policy();
    if !policy.providers.contains(provider_id) {
        return Err(format!(
            "provider {provider_id} is not allowed by MDX_MODEL_PROVIDER_ALLOWLIST"
        ));
    }
    if !policy.modes.contains(mode) {
        return Err(format!(
            "connection mode {mode} is not allowed by MDX_MODEL_ACCESS_MODES"
        ));
    }
    Ok(())
}

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/models/inferences.json" => Some(if method == "POST" {
            crate::model_inference_route::infer(body, kernel)
        } else {
            Ok(method_not_allowed())
        }),
        "/models/fabric.json" => Some(if method == "GET" {
            Ok(RouteResponse::json("200 OK", FABRIC_JSON.to_string()))
        } else {
            Ok(method_not_allowed())
        }),
        "/models/readiness.json" => Some(if method == "GET" {
            readiness(kernel)
        } else {
            Ok(method_not_allowed())
        }),
        "/models/access-policy.json" => Some(if method == "GET" {
            Ok(RouteResponse::json(
                "200 OK",
                access_policy().as_json().to_string(),
            ))
        } else {
            Ok(method_not_allowed())
        }),
        "/models/routes/resolve.json" => Some(if method == "POST" {
            resolve(body, kernel)
        } else {
            Ok(method_not_allowed())
        }),
        "/models/connect.json" => Some(if method == "POST" {
            connect_model(body, kernel)
        } else {
            Ok(method_not_allowed())
        }),
        "/models/catalog.json" => Some(if method == "POST" {
            discover_catalog(body)
        } else {
            Ok(method_not_allowed())
        }),
        "/models/test.json" => Some(if method == "POST" {
            test_model(body, kernel)
        } else {
            Ok(method_not_allowed())
        }),
        "/models/connections.json" => Some(match method {
            "GET" => connections(kernel),
            "POST" => configure_connection(body, kernel),
            _ => Ok(method_not_allowed()),
        }),
        "/models/deployments.json" => Some(match method {
            "GET" => deployments(kernel),
            "POST" => register_deployment(body, kernel),
            _ => Ok(method_not_allowed()),
        }),
        "/models/policies.json" => Some(match method {
            "GET" => policies(kernel),
            "POST" => configure_policy(body, kernel),
            _ => Ok(method_not_allowed()),
        }),
        "/models/adaptive/evaluations.json" => Some(if method == "POST" {
            evaluate_adaptive_policy(body, kernel)
        } else {
            Ok(method_not_allowed())
        }),
        "/models/adaptive/promotions.json" => Some(if method == "POST" {
            promote_adaptive_policy(body, kernel)
        } else {
            Ok(method_not_allowed())
        }),
        "/models/outcomes.json" => Some(match method {
            "GET" => outcomes(kernel),
            "POST" => evaluate_outcome(body, kernel),
            _ => Ok(method_not_allowed()),
        }),
        _ => None,
    }
}

fn outcomes(kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let tenant_id = tenant_for_read();
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let receipts = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            receipt.tenant_id.as_str() == tenant_id
                && receipt.kind == mdx_core::MODEL_OUTCOME_RECORDED_RECEIPT_KIND
        })
        .collect::<Vec<_>>();
    let source_receipt_ids = receipts
        .iter()
        .map(|receipt| receipt.receipt_id.clone())
        .collect::<Vec<_>>();
    let rows = receipts.into_iter().map(|receipt| json!({
        "outcome_id": receipt.receipt_id,
        "decision_id": receipt.payload.get("decision_id"),
        "workload_id": receipt.payload.get("workload_id"),
        "deployment_id": receipt.payload.get("deployment_id"),
        "latency_ms": receipt.payload.get("latency_ms"),
        "cost_microusd": receipt.payload.get("cost_microusd"),
        "quality_score": receipt.payload.get("quality_score").filter(|value| !value.is_empty()),
        "safety_status": receipt.payload.get("safety_status"),
        "task_status": receipt.payload.get("task_status"),
        "correction_status": receipt.payload.get("correction_status"),
        "provenance": receipt.payload.get("provenance"),
        "grants_execution_authority": false
    })).collect::<Vec<_>>();
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-model-outcomes",
            "tenant_id": tenant_id,
            "outcome_count": rows.len(),
            "outcomes": rows,
            "source_receipt_ids": source_receipt_ids,
            "grants_execution_authority": false
        })
        .to_string(),
    ))
}

fn evaluate_outcome(body: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid model outcome evaluation request: {error}"))?;
    let (tenant_id, actor_id) = tenant_and_actor(&value);
    let source_outcome_id = string(&value, "outcome_id")?;
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let source = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| {
            receipt.receipt_id == source_outcome_id
                && receipt.tenant_id.as_str() == tenant_id
                && receipt.kind == mdx_core::MODEL_OUTCOME_RECORDED_RECEIPT_KIND
        })
        .cloned();
    let Some(source) = source else {
        let reason = "model outcome evaluation requires a same-tenant runtime outcome";
        let correlation = model_correlation(&mut kernel, &tenant_id, &actor_id);
        let receipt =
            kernel.record_model_outcome_evaluation_refused(&correlation, source_outcome_id, reason);
        return Ok(RouteResponse::json(
            "200 OK",
            json!({
                "status": "MODEL_OUTCOME_EVALUATION_REFUSED",
                "reason": reason,
                "receipt_id": receipt.receipt_id,
                "grants_execution_authority": false
            })
            .to_string(),
        ));
    };
    let source_marker = format!(":source:{source_outcome_id}");
    if kernel.ledger().entries().iter().any(|receipt| {
        receipt.tenant_id.as_str() == tenant_id
            && receipt.kind == mdx_core::MODEL_OUTCOME_RECORDED_RECEIPT_KIND
            && receipt
                .payload
                .get("provenance")
                .is_some_and(|provenance| provenance.ends_with(&source_marker))
    }) {
        let reason = "that runtime outcome already has an evaluator judgment";
        let correlation = model_correlation(&mut kernel, &tenant_id, &actor_id);
        let receipt =
            kernel.record_model_outcome_evaluation_refused(&correlation, source_outcome_id, reason);
        return Ok(RouteResponse::json(
            "200 OK",
            json!({
                "status": "MODEL_OUTCOME_EVALUATION_REFUSED",
                "reason": reason,
                "receipt_id": receipt.receipt_id,
                "grants_execution_authority": false
            })
            .to_string(),
        ));
    }
    let quality_score = match value["quality_score"].as_u64() {
        Some(score) if score <= 100 => Some(score as u32),
        Some(_) => return Err("model outcome quality score must be between 0 and 100".to_string()),
        None => None,
    };
    let evaluated = mdx_core::ModelOutcome {
        decision_id: source
            .payload
            .get("decision_id")
            .cloned()
            .unwrap_or_default(),
        workload_id: source
            .payload
            .get("workload_id")
            .cloned()
            .unwrap_or_default(),
        app_id: source.payload.get("app_id").cloned().unwrap_or_default(),
        environment: source
            .payload
            .get("environment")
            .cloned()
            .unwrap_or_default(),
        session_id: source
            .payload
            .get("session_id")
            .cloned()
            .unwrap_or_default(),
        deployment_id: source
            .payload
            .get("deployment_id")
            .cloned()
            .unwrap_or_default(),
        latency_ms: source
            .payload
            .get("latency_ms")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0),
        cost_microusd: source
            .payload
            .get("cost_microusd")
            .and_then(|value| value.parse().ok()),
        quality_score,
        safety_status: value["safety_status"]
            .as_str()
            .unwrap_or("not_evaluated")
            .to_string(),
        task_status: value["task_status"]
            .as_str()
            .unwrap_or("not_evaluated")
            .to_string(),
        correction_status: value["correction_status"]
            .as_str()
            .unwrap_or("not_observed")
            .to_string(),
        provenance: format!("operator_evaluation:{actor_id}:source:{source_outcome_id}"),
    };
    let correlation = model_correlation(&mut kernel, &tenant_id, &actor_id);
    let receipt = kernel
        .record_model_outcome(&correlation, &evaluated)
        .map_err(str::to_string)?;
    let adaptive_rollback_receipt_id = crate::model_adaptive_service::monitor_evaluated_outcome(
        &mut kernel,
        &tenant_id,
        &actor_id,
        &receipt.receipt_id,
        &evaluated,
    )?;
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "status": "MODEL_OUTCOME_EVALUATED",
            "outcome_id": receipt.receipt_id,
            "source_outcome_id": source_outcome_id,
            "adaptive_rollback_receipt_id": adaptive_rollback_receipt_id,
            "grants_execution_authority": false
        })
        .to_string(),
    ))
}

fn evaluate_adaptive_policy(
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid adaptive policy evaluation request: {error}"))?;
    let (tenant_id, actor_id) = tenant_and_actor(&value);
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let policy_id = string(&value, "policy_id")?;
    let policy_version = string(&value, "policy_version")?;
    let report = crate::model_adaptive_service::evaluate(
        &mut kernel,
        &tenant_id,
        &actor_id,
        policy_id,
        policy_version,
    );
    let report = match report {
        Ok(report) => report,
        Err(reason) => {
            let correlation = model_correlation(&mut kernel, &tenant_id, &actor_id);
            let receipt = kernel.record_model_adaptive_policy_refused(
                &correlation,
                policy_id,
                policy_version,
                &reason,
            );
            return Ok(RouteResponse::json(
                "200 OK",
                json!({
                    "status": "MODEL_ADAPTIVE_POLICY_REFUSED",
                    "reason": reason,
                    "receipt_id": receipt.receipt_id,
                    "grants_execution_authority": false
                })
                .to_string(),
            ));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "status": "MODEL_ADAPTIVE_POLICY_EVALUATED",
            "policy_id": report.evidence.policy_id,
            "policy_version": report.evidence.policy_version,
            "previous_state": report.evidence.state,
            "state": report.next_state,
            "recommended_transition": report.recommended_transition,
            "replay_cases": report.evidence.replay_cases,
            "shadow_decisions": report.evidence.shadow_decisions,
            "canary_decisions": report.evidence.canary_decisions,
            "guardrail_evidence_sufficient": report.evidence.guardrail_evidence_sufficient,
            "guardrails": {
                "quality": report.evidence.quality_guardrail_passed,
                "safety": report.evidence.safety_guardrail_passed,
                "latency": report.evidence.latency_guardrail_passed,
                "cost": report.evidence.cost_guardrail_passed
            },
            "receipt_id": report.receipt_id,
            "grants_execution_authority": false
        })
        .to_string(),
    ))
}

fn promote_adaptive_policy(
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid adaptive policy promotion request: {error}"))?;
    let (tenant_id, actor_id) = tenant_and_actor(&value);
    let policy_id = string(&value, "policy_id")?;
    let policy_version = string(&value, "policy_version")?;
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    match crate::model_adaptive_service::approve_promotion(
        &mut kernel,
        &tenant_id,
        &actor_id,
        policy_id,
        policy_version,
    ) {
        Ok(report) => Ok(RouteResponse::json(
            "200 OK",
            json!({
                "status": "MODEL_ADAPTIVE_POLICY_PROMOTED",
                "policy_id": policy_id,
                "policy_version": policy_version,
                "state": report.next_state,
                "receipt_id": report.receipt_id,
                "grants_execution_authority": false
            })
            .to_string(),
        )),
        Err(reason) => {
            let correlation = model_correlation(&mut kernel, &tenant_id, &actor_id);
            let receipt = kernel.record_model_adaptive_policy_refused(
                &correlation,
                policy_id,
                policy_version,
                &reason,
            );
            Ok(RouteResponse::json(
                "200 OK",
                json!({
                    "status": "MODEL_ADAPTIVE_POLICY_PROMOTION_REFUSED",
                    "reason": reason,
                    "receipt_id": receipt.receipt_id,
                    "grants_execution_authority": false
                })
                .to_string(),
            ))
        }
    }
}

pub(crate) fn connect_model(
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid model connection request: {error}"))?;
    let (tenant_id, actor_id) = tenant_and_actor(&value);
    let requested_provider = string(&value, "provider_id")?;
    let provider = model_provider(requested_provider)
        .ok_or_else(|| "model connection provider is not declared".to_string())?;
    let supplied_api_key = value["api_key"].as_str().unwrap_or("").trim();
    let supplied_credential_ref = value["credential_ref"].as_str().unwrap_or("").trim();
    let protocol = crate::model_gateway_runtime::protocol_for_provider(provider.provider_id)
        .ok_or_else(|| "provider has no live Model Fabric adapter".to_string())?;
    let base_url = value["endpoint_base_url"]
        .as_str()
        .filter(|url| !url.trim().is_empty())
        .or_else(|| crate::model_gateway_runtime::default_base_url(provider.provider_id))
        .ok_or_else(|| "provider requires an endpoint base URL".to_string())?;
    let existing_connection = crate::model_fabric_registry::global()
        .snapshot_for_tenant(&tenant_id)?
        .connections
        .into_iter()
        .find(|connection| connection.provider_id == provider.provider_id);
    let effective_credential_ref = if supplied_credential_ref.is_empty() {
        existing_connection
            .as_ref()
            .map(|connection| connection.credential_ref.as_str())
            .unwrap_or("")
    } else {
        supplied_credential_ref
    };
    let access_mode = connection_access_mode(
        provider.provider_id,
        supplied_api_key,
        effective_credential_ref,
    );
    enforce_model_access(provider.provider_id, access_mode)?;
    let resolved_existing = existing_connection.as_ref().and_then(|connection| {
        credential_for_operator_action(
            &tenant_id,
            &connection.credential_ref,
            provider.credential_env,
        )
    });
    let resolved_supplied_ref = (!supplied_credential_ref.is_empty())
        .then(|| {
            credential_for_operator_action(
                &tenant_id,
                supplied_credential_ref,
                provider.credential_env,
            )
        })
        .flatten();
    let api_key = if !supplied_api_key.is_empty() {
        supplied_api_key.to_string()
    } else {
        resolved_supplied_ref
            .as_ref()
            .or(resolved_existing.as_ref())
            .map(|credential| credential.value.clone())
            .unwrap_or_default()
    };
    if api_key.is_empty() && !matches!(provider.provider_id, "ollama" | "vllm" | "tensorzero") {
        return refuse_model_connection(
            kernel,
            &tenant_id,
            &actor_id,
            provider.provider_id,
            "provider API key is required",
        );
    }

    let discovered_models = if value["model_id"]
        .as_str()
        .is_none_or(|model| model.trim().is_empty())
    {
        discover_provider_models(provider.provider_id, base_url, &api_key)?
    } else {
        Vec::new()
    };
    let provider_model_id = value["model_id"]
        .as_str()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string)
        .or_else(|| recommended_model(&discovered_models))
        .ok_or_else(|| {
            "the provider returned no usable text models; choose a model ID explicitly".to_string()
        })?;
    if !discovered_models.is_empty() {
        crate::secret_store::global().set_models_for_tenant(
            &tenant_id,
            provider.credential_env,
            &discovered_models,
        );
    }

    let test_mode = value["test_mode"].as_bool() == Some(true);
    let tool_probe = connection_probe(
        test_mode,
        crate::model_gateway_runtime::GatewayCallRequest {
            provider_id: provider.provider_id.to_string(),
            adapter_id: provider.adapter.to_string(),
            protocol,
            base_url: base_url.to_string(),
            api_key: api_key.clone(),
            model_id: provider_model_id.clone(),
            system: "You are performing an MDx model connection check.".to_string(),
            messages: vec![crate::model_gateway_runtime::GatewayMessage {
                role: "user".to_string(),
                content: "Call report_model_ready with ready=true. Do not answer in text."
                    .to_string(),
            }],
            tools: vec![crate::model_gateway_runtime::GatewayTool {
                name: "report_model_ready".to_string(),
                description: "Report that the model connection is ready.".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {"ready": {"type": "boolean"}},
                    "required": ["ready"],
                    "additionalProperties": false
                }),
            }],
            max_output_tokens: 128,
            timeout: std::time::Duration::from_secs(20),
            session_id: None,
        },
    );
    let (observed, tools_verified) = match tool_probe {
        Ok(observed) => {
            let verified = observed.tool_calls.iter().any(|call| {
                call.name == "report_model_ready" && call.arguments["ready"].as_bool() == Some(true)
            });
            (observed, verified)
        }
        Err(_) => {
            let observed = match connection_probe(
                test_mode,
                crate::model_gateway_runtime::GatewayCallRequest {
                    provider_id: provider.provider_id.to_string(),
                    adapter_id: provider.adapter.to_string(),
                    protocol,
                    base_url: base_url.to_string(),
                    api_key: api_key.clone(),
                    model_id: provider_model_id.clone(),
                    system: "You are performing an MDx model connection check.".to_string(),
                    messages: vec![crate::model_gateway_runtime::GatewayMessage {
                        role: "user".to_string(),
                        content: "Reply only with OK.".to_string(),
                    }],
                    tools: Vec::new(),
                    max_output_tokens: 128,
                    timeout: std::time::Duration::from_secs(20),
                    session_id: None,
                },
            ) {
                Ok(observed) => observed,
                Err(reason) => {
                    return refuse_model_connection(
                        kernel,
                        &tenant_id,
                        &actor_id,
                        provider.provider_id,
                        &format!("provider connection probe failed: {reason}"),
                    );
                }
            };
            (observed, false)
        }
    };

    let (credential_source, credential_ref) =
        if matches!(provider.provider_id, "ollama" | "vllm" | "tensorzero") {
            (
                "not_required",
                format!("secret:none/{}", provider.credential_env),
            )
        } else if !supplied_api_key.is_empty() && value["persist_key"].as_bool().unwrap_or(true) {
            crate::secret_store::global().set_for_tenant(
                &tenant_id,
                provider.credential_env,
                &api_key,
            );
            crate::secret_store::global()
                .persist_for_tenant(&tenant_id, provider.credential_env, &api_key)
                .map(|source| {
                    (
                        source,
                        format!("secret:{source}/{}", provider.credential_env),
                    )
                })
                .unwrap_or_else(|_| {
                    (
                        "session",
                        format!("secret:session/{}", provider.credential_env),
                    )
                })
        } else if !supplied_api_key.is_empty() {
            crate::secret_store::global().set_for_tenant(
                &tenant_id,
                provider.credential_env,
                &api_key,
            );
            (
                "session",
                format!("secret:session/{}", provider.credential_env),
            )
        } else if !supplied_credential_ref.is_empty() {
            (
                resolved_supplied_ref
                    .as_ref()
                    .map(|credential| credential.source)
                    .unwrap_or("configured"),
                supplied_credential_ref.to_string(),
            )
        } else if let Some(connection) = existing_connection.as_ref() {
            (
                resolved_existing
                    .as_ref()
                    .map(|credential| credential.source)
                    .unwrap_or("configured"),
                connection.credential_ref.clone(),
            )
        } else {
            return Err("model connection has no resolvable credential reference".to_string());
        };
    let local = matches!(provider.provider_id, "ollama" | "vllm");
    let (mut data_retention, mut training_policy, mut data_policy_provenance) =
        crate::install_model_connect::observed_data_policy(provider.provider_id, local);
    if provider.provider_id == "google" && value["paid_service_data_terms"].as_bool() == Some(true)
    {
        data_retention = "provider";
        training_policy = "none";
        data_policy_provenance = "https://ai.google.dev/gemini-api/docs/zdr";
    }
    if data_retention == "unknown"
        && training_policy == "unknown"
        && value["privacy_controls_attested"].as_bool() == Some(true)
    {
        data_retention = "none";
        training_policy = "none";
        data_policy_provenance = "operator_attestation:no_content_storage_or_training_controls";
    }
    let connection_id = format!("{tenant_id}:{}", provider.provider_id);
    let model_id = format!("{}/{}", provider.provider_id, provider_model_id);
    let deployment_id = format!("{tenant_id}:{model_id}");
    let connection = ProviderConnection {
        connection_id: connection_id.clone(),
        tenant_id: tenant_id.clone(),
        provider_id: provider.provider_id.to_string(),
        credential_ref,
        endpoint_base_url: base_url.to_string(),
        region: value["region"]
            .as_str()
            .unwrap_or(if local { "local" } else { "global" })
            .to_string(),
        residency: value["residency"]
            .as_str()
            .unwrap_or(if local { "local" } else { "provider_managed" })
            .to_string(),
        data_retention: data_retention.to_string(),
        training_policy: training_policy.to_string(),
        data_policy_provenance: data_policy_provenance.to_string(),
        data_policy_observed_at: if data_policy_provenance.starts_with("operator_attestation:") {
            "connect_receipt_timestamp".to_string()
        } else {
            "2026-07-12".to_string()
        },
        health: "healthy".to_string(),
        live_call_allowed: true,
    };
    let model = Model {
        model_id: model_id.clone(),
        provider_model_id: provider_model_id.clone(),
        provider_id: provider.provider_id.to_string(),
        display_name: value["display_name"]
            .as_str()
            .unwrap_or(&provider_model_id)
            .to_string(),
        lifecycle: "active".to_string(),
        modality: "text".to_string(),
    };
    let deployment = ModelDeployment {
        deployment_id: deployment_id.clone(),
        connection_id,
        provider_id: provider.provider_id.to_string(),
        model_id,
        privacy_class: value["privacy_class"]
            .as_str()
            .unwrap_or(if local { "local" } else { "frontier" })
            .to_string(),
        region: value["region"]
            .as_str()
            .unwrap_or(if local { "local" } else { "global" })
            .to_string(),
        residency: value["residency"]
            .as_str()
            .unwrap_or(if local { "local" } else { "provider_managed" })
            .to_string(),
        enabled: true,
        capabilities: CapabilityProfile {
            tools: tools_verified && value["tools"].as_bool() != Some(false),
            structured_output: value["structured_output"].as_bool().unwrap_or(false),
            vision: value["vision"].as_bool().unwrap_or(false),
            context_tokens: value["context_tokens"].as_u64().unwrap_or(0),
            provenance: format!(
                "text_generation_live_verified; tools_{}; structured_output_{}; vision_{}",
                if tools_verified {
                    "live_verified"
                } else {
                    "unverified"
                },
                if value["structured_output"].is_boolean() {
                    "operator_asserted"
                } else {
                    "unverified"
                },
                if value["vision"].is_boolean() {
                    "operator_asserted"
                } else {
                    "unverified"
                }
            ),
            observed_at: "connect_receipt_timestamp".to_string(),
        },
    };
    let price = connection_price_observation(
        &deployment_id,
        provider.provider_id,
        &provider_model_id,
        &value,
        local,
    );
    let registry = crate::model_fabric_registry::global();
    registry.upsert_connection(connection.clone())?;
    registry.upsert_model(&tenant_id, model.clone())?;
    registry.upsert_deployment(&tenant_id, deployment.clone(), price.clone())?;
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let correlation = model_correlation(&mut kernel, &tenant_id, &actor_id);
    let connection_receipt = kernel.record_model_connection_configured(&correlation, &connection);
    let deployment_receipt =
        kernel.record_model_deployment_registered(&correlation, &model, &deployment, &price);
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "status": "MODEL_CONNECTED",
            "provider_id": provider.provider_id,
            "model_id": provider_model_id,
            "deployment_id": deployment_id,
            "connection_receipt_id": connection_receipt.receipt_id,
            "deployment_receipt_id": deployment_receipt.receipt_id,
            "probe_inference_id": observed.inference_id,
            "capability_verification": {
                "text_generation": "live_verified",
                "tools": if tools_verified {
                    if value["tools"].as_bool() == Some(false) { "live_verified_but_disabled" } else { "live_verified" }
                } else { "unverified" },
                "structured_output": if value["structured_output"].is_boolean() { "operator_asserted" } else { "unverified" },
                "vision": if value["vision"].is_boolean() { "operator_asserted" } else { "unverified" }
            },
            "secret_value_recorded": false,
            "credential_source": credential_source,
            "access_mode": access_mode,
            "models_discovered": discovered_models.len(),
            "model_selected_automatically": value["model_id"].as_str().is_none_or(|model| model.trim().is_empty()),
            "grants_execution_authority": false
        })
        .to_string(),
    ))
}

fn connection_probe(
    test_mode: bool,
    request: crate::model_gateway_runtime::GatewayCallRequest,
) -> Result<crate::model_gateway_runtime::GatewayCallResult, String> {
    #[cfg(test)]
    if test_mode {
        return Ok(crate::model_gateway_runtime::GatewayCallResult {
            provider_id: request.provider_id,
            adapter_id: request.adapter_id,
            model_id: request.model_id,
            inference_id: "model_connection_test_probe".to_string(),
            text: "OK".to_string(),
            tool_calls: request
                .tools
                .first()
                .map(|tool| {
                    vec![crate::model_gateway_runtime::GatewayToolCall {
                        call_id: "model_connection_test_tool_call".to_string(),
                        name: tool.name.clone(),
                        arguments: json!({"ready": true}),
                    }]
                })
                .unwrap_or_default(),
            input_tokens: 1,
            output_tokens: 1,
            latency_ms: 1,
            finish_reason: "stop".to_string(),
        });
    }
    #[cfg(not(test))]
    let _ = test_mode;
    crate::model_gateway_runtime::execute(&request)
}

fn discover_catalog(body: &str) -> Result<RouteResponse, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid model catalog request: {error}"))?;
    let (tenant_id, _) = tenant_and_actor(&value);
    let requested_provider = string(&value, "provider_id")?;
    let provider = model_provider(requested_provider)
        .ok_or_else(|| "model catalog provider is not declared".to_string())?;
    let supplied_api_key = value["api_key"].as_str().unwrap_or("").trim();
    let credential_ref = value["credential_ref"].as_str().unwrap_or("").trim();
    let mut access_mode =
        connection_access_mode(provider.provider_id, supplied_api_key, credential_ref);
    let base_url = value["endpoint_base_url"]
        .as_str()
        .filter(|url| !url.trim().is_empty())
        .or_else(|| crate::model_gateway_runtime::default_base_url(provider.provider_id))
        .ok_or_else(|| "provider requires an endpoint base URL".to_string())?;
    if value["dry_run"].as_bool() == Some(true) {
        enforce_model_access(provider.provider_id, access_mode)?;
        return Ok(RouteResponse::json(
            "200 OK",
            json!({
                "name": "mdx-model-catalog",
                "status": "CATALOG_DISCOVERY_READY",
                "provider_id": provider.provider_id,
                "endpoint_base_url": base_url,
                "provider_call_performed": false,
                "secret_values_exposed": false,
                "grants_execution_authority": false
            })
            .to_string(),
        ));
    }
    crate::model_gateway_runtime::validate_base_url(
        base_url,
        matches!(provider.provider_id, "ollama" | "vllm" | "tensorzero"),
    )?;
    let existing_connection = crate::model_fabric_registry::global()
        .snapshot_for_tenant(&tenant_id)?
        .connections
        .into_iter()
        .find(|connection| connection.provider_id == provider.provider_id);
    if supplied_api_key.is_empty() && credential_ref.is_empty() {
        access_mode = connection_access_mode(
            provider.provider_id,
            supplied_api_key,
            existing_connection
                .as_ref()
                .map(|connection| connection.credential_ref.as_str())
                .unwrap_or(""),
        );
    }
    enforce_model_access(provider.provider_id, access_mode)?;
    let resolved = if !supplied_api_key.is_empty() {
        Some(supplied_api_key.to_string())
    } else if !credential_ref.is_empty() {
        credential_for_operator_action(&tenant_id, credential_ref, provider.credential_env)
            .map(|credential| credential.value)
    } else {
        existing_connection.as_ref().and_then(|connection| {
            credential_for_operator_action(
                &tenant_id,
                &connection.credential_ref,
                provider.credential_env,
            )
            .map(|credential| credential.value)
        })
    };
    let api_key = resolved.unwrap_or_default();
    if api_key.is_empty() && !matches!(provider.provider_id, "ollama" | "vllm" | "tensorzero") {
        return Err("model discovery requires a resolvable provider credential".to_string());
    }
    let models = discover_provider_models(provider.provider_id, base_url, &api_key)?;
    crate::secret_store::global().set_models_for_tenant(
        &tenant_id,
        provider.credential_env,
        &models,
    );
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-model-catalog",
            "status": "CATALOG_OBSERVED",
            "provider_id": provider.provider_id,
            "access_mode": access_mode,
            "model_count": models.len(),
            "models": models,
            "recommended_model_id": recommended_model(&models),
            "recommendation_basis": "provider_catalog_text_model_heuristic",
            "secret_values_exposed": false,
            "grants_execution_authority": false
        })
        .to_string(),
    ))
}

fn credential_for_operator_action(
    tenant_id: &str,
    credential_ref: &str,
    fallback_env: &str,
) -> Option<crate::secret_store::CredentialResolution> {
    let store = crate::secret_store::global();
    if let Some(credential) =
        store.credential_for_connection(tenant_id, credential_ref, fallback_env)
    {
        return Some(credential);
    }
    if let Some(credential) =
        store.authorize_keychain_credential_for_connection(tenant_id, credential_ref, fallback_env)
    {
        return Some(credential);
    }
    store.request_credential_refresh_for_connection(tenant_id, credential_ref, fallback_env);
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if let Some(credential) =
            store.credential_for_connection(tenant_id, credential_ref, fallback_env)
        {
            return Some(credential);
        }
    }
    None
}

fn test_model(body: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid model test request: {error}"))?;
    if value["allow_live"].as_bool() != Some(true) {
        return Ok(RouteResponse::json(
            "200 OK",
            json!({
                "name": "mdx-model-test",
                "status": "MODEL_TEST_NOT_RUN",
                "reason": "Set allow_live=true and a cost cap to run one live model proof.",
                "provider_call_performed": false,
                "cost_microusd": 0,
                "source_receipt_ids": [],
                "receipt_reason": "No receipt is created until explicit live authorization is supplied.",
                "secret_values_exposed": false,
                "grants_execution_authority": false
            })
            .to_string(),
        ));
    }
    let max_cost_microusd = value["max_cost_microusd"].as_u64().unwrap_or(1_000);
    let policy = access_policy();
    if max_cost_microusd == 0 || max_cost_microusd > policy.max_live_test_microusd {
        return Err(format!(
            "model test max_cost_microusd must be between 1 and {}",
            policy.max_live_test_microusd
        ));
    }
    let workload_id = value["workload_id"]
        .as_str()
        .unwrap_or("mdx/twin/conversation");
    let prompt = value["prompt"]
        .as_str()
        .unwrap_or("Reply with exactly: MDx model access ready");
    let request = json!({
        "tenant_id": value["tenant_id"],
        "actor_id": value["actor_id"],
        "workload_id": workload_id,
        "preset": value["preset"].as_str().unwrap_or("balanced"),
        "pinned_deployment_id": value["pinned_deployment_id"],
        "allow_fallback": false,
        "messages": [{"role": "user", "content": prompt}],
        "max_output_tokens": value["max_output_tokens"].as_u64().unwrap_or(128).clamp(1, 512),
        "max_cost_microusd": max_cost_microusd
    });
    crate::model_inference_route::infer(&request.to_string(), kernel)
}

fn discover_provider_models(
    provider_id: &str,
    base_url: &str,
    api_key: &str,
) -> Result<Vec<String>, String> {
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(20))
        .build();
    let mut request = agent
        .get(&format!("{}/models", base_url.trim_end_matches('/')))
        .set("Accept", "application/json");
    request = match provider_id {
        "anthropic" => request
            .set("x-api-key", api_key)
            .set("anthropic-version", "2023-06-01"),
        "google" | "gemini" => request.set("x-goog-api-key", api_key),
        "ollama" | "vllm" => request,
        _ => request.set("Authorization", &format!("Bearer {api_key}")),
    };
    let response = request
        .call()
        .map_err(|error| format!("the provider model catalog could not be read ({error})"))?;
    let parsed: Value = response
        .into_json()
        .map_err(|error| format!("the provider model catalog could not be parsed ({error})"))?;
    Ok(parse_provider_models(&parsed))
}

fn parse_provider_models(value: &Value) -> Vec<String> {
    let rows = value["data"]
        .as_array()
        .or_else(|| value["models"].as_array())
        .cloned()
        .unwrap_or_default();
    let mut models = rows
        .iter()
        .filter_map(|row| {
            row["id"]
                .as_str()
                .or_else(|| row["name"].as_str())
                .map(|model| model.trim_start_matches("models/").trim().to_string())
        })
        .filter(|model| !model.is_empty() && model.len() <= 256)
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    models
}

fn recommended_model(models: &[String]) -> Option<String> {
    models
        .iter()
        .filter(|model| {
            let lower = model.to_ascii_lowercase();
            ![
                "embedding",
                "moderation",
                "whisper",
                "tts",
                "image",
                "dall-e",
                "rerank",
            ]
            .iter()
            .any(|excluded| lower.contains(excluded))
        })
        .max_by_key(|model| {
            let lower = model.to_ascii_lowercase();
            (
                usize::from(lower.contains("latest")) * 8
                    + usize::from(lower.contains("pro")) * 4
                    + usize::from(lower.contains("sonnet")) * 4
                    + usize::from(lower.contains("gpt")) * 2
                    + usize::from(lower.contains("grok")) * 2,
                *model,
            )
        })
        .cloned()
        .or_else(|| models.first().cloned())
}

fn refuse_model_connection(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    actor_id: &str,
    provider_id: &str,
    reason: &str,
) -> Result<RouteResponse, String> {
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let correlation = model_correlation(&mut kernel, tenant_id, actor_id);
    let receipt = kernel.record_model_connection_refused(&correlation, provider_id, reason);
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "status": "MODEL_CONNECTION_REFUSED",
            "provider_id": provider_id,
            "reason": reason,
            "receipt_id": receipt.receipt_id,
            "secret_value_recorded": false,
            "grants_execution_authority": false
        })
        .to_string(),
    ))
}

fn tenant_and_actor(value: &Value) -> (String, String) {
    crate::request_security::current_verified_identity()
        .map(|identity| (identity.tenant_id, identity.actor_id))
        .unwrap_or_else(|| {
            (
                value["tenant_id"]
                    .as_str()
                    .unwrap_or("local_tenant")
                    .to_string(),
                value["actor_id"]
                    .as_str()
                    .unwrap_or("human:local_user")
                    .to_string(),
            )
        })
}

fn tenant_for_read() -> String {
    crate::request_security::current_verified_identity()
        .map(|identity| identity.tenant_id)
        .unwrap_or_else(|| "local_tenant".to_string())
}

fn model_receipt_ids(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    kinds: &[&str],
) -> Result<Vec<String>, String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    Ok(kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            receipt.tenant_id.as_str() == tenant_id && kinds.contains(&receipt.kind.as_str())
        })
        .map(|receipt| receipt.receipt_id.clone())
        .collect())
}

fn connection_json(
    store: &crate::secret_store::SecretStore,
    tenant_id: &str,
    connection: &ProviderConnection,
) -> Value {
    let provider = model_provider(&connection.provider_id);
    let credential = provider.and_then(|provider| {
        if matches!(provider.provider_id, "ollama" | "vllm" | "tensorzero") {
            Some(crate::secret_store::CredentialResolution {
                value: String::new(),
                source: "not_required",
            })
        } else {
            store.credential_for_connection(
                tenant_id,
                &connection.credential_ref,
                provider.credential_env,
            )
        }
    });
    let credential_source = credential
        .as_ref()
        .map(|credential| credential.source)
        .unwrap_or("none");
    let credential_available = credential.is_some();
    json!({
        "connection_id": connection.connection_id,
        "provider_id": connection.provider_id,
        "credential_ref": connection.credential_ref,
        "endpoint_base_url": connection.endpoint_base_url,
        "region": connection.region,
        "residency": connection.residency,
        "data_retention": connection.data_retention,
        "training_policy": connection.training_policy,
        "data_policy_provenance": connection.data_policy_provenance,
        "data_policy_observed_at": connection.data_policy_observed_at,
        "health": connection.health,
        "live_call_allowed": connection.live_call_allowed,
        "credential_available": credential_available,
        "credential_source": credential_source,
        "credential_durable": credential_available && credential_source != "session",
        "secret_value_exposed": false
    })
}

pub(crate) fn readiness_value(kernel: &Arc<RwLock<MdxKernel>>) -> Result<Value, String> {
    let tenant_id = tenant_for_read();
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    readiness_value_for_tenant(&kernel, &tenant_id, crate::secret_store::global())
}

fn workload_block_reason(
    workload: &mdx_core::ModelWorkloadContract,
    deployments: &[&ModelDeployment],
    connections: &[ProviderConnection],
    ready_connection_ids: &std::collections::BTreeSet<String>,
) -> Option<&'static str> {
    let deployment_reason = |deployment: &&ModelDeployment| {
        let connection = connections
            .iter()
            .find(|connection| connection.connection_id == deployment.connection_id);
        if !deployment.enabled || !ready_connection_ids.contains(&deployment.connection_id) {
            return Some("needs_connection");
        }
        if workload.surface == "forge"
            && !crate::forge_turn_client::TurnClient::supports_forge_turns(&deployment.provider_id)
        {
            return Some("needs_forge_compatible_adapter");
        }
        if workload.sensitivity == "sensitive"
            && !matches!(deployment.privacy_class.as_str(), "sovereign" | "local")
        {
            return Some("needs_private_deployment");
        }
        if workload.sensitivity == "internal"
            && !matches!(
                deployment.privacy_class.as_str(),
                "frontier" | "sovereign" | "local"
            )
        {
            return Some("needs_internal_data_deployment");
        }
        if connection.is_none_or(|connection| {
            !matches!(connection.data_retention.as_str(), "none" | "provider")
                || !matches!(connection.training_policy.as_str(), "none" | "allowed")
        }) {
            return Some("needs_data_policy_confirmation");
        }
        if workload.tools_required && !deployment.capabilities.tools {
            return Some("needs_verified_tool_calling");
        }
        if workload.structured_output_required && !deployment.capabilities.structured_output {
            return Some("needs_structured_output_confirmation");
        }
        if workload.vision_required && !deployment.capabilities.vision {
            return Some("needs_verified_vision");
        }
        None
    };
    if deployments.is_empty() {
        return Some("needs_connection");
    }
    let deployment_statuses = deployments
        .iter()
        .map(deployment_reason)
        .collect::<Vec<_>>();
    if deployment_statuses.iter().any(Option::is_none) {
        return None;
    }
    let reasons = deployment_statuses
        .into_iter()
        .flatten()
        .collect::<BTreeSet<_>>();
    [
        "needs_verified_tool_calling",
        "needs_structured_output_confirmation",
        "needs_private_deployment",
        "needs_forge_compatible_adapter",
        "needs_data_policy_confirmation",
        "needs_internal_data_deployment",
        "needs_verified_vision",
        "needs_connection",
    ]
    .into_iter()
    .find(|reason| reasons.contains(reason))
    .or(Some("needs_eligible_deployment"))
}

fn workload_next_action(reason: Option<&str>) -> &'static str {
    match reason {
        None => "This work can route through a verified deployment.",
        Some("needs_verified_tool_calling") => {
            "Reconnect and let MDx verify tool calling for this model."
        }
        Some("needs_structured_output_confirmation") => {
            "Confirm structured output support for a connected model."
        }
        Some("needs_private_deployment") => {
            "Connect a local or sovereign model for sensitive work."
        }
        Some("needs_forge_compatible_adapter") => {
            "Connect a model with a Forge-compatible live adapter."
        }
        Some("needs_data_policy_confirmation") => {
            "Confirm the provider's storage and training controls."
        }
        Some("needs_verified_vision") => "Connect and verify a vision-capable model.",
        _ => "Connect or restore an eligible model in Model Center.",
    }
}

pub(crate) fn readiness_value_for_tenant(
    kernel: &MdxKernel,
    tenant_id: &str,
    store: &crate::secret_store::SecretStore,
) -> Result<Value, String> {
    let snapshot = crate::model_fabric_registry::global().snapshot_for_tenant(tenant_id)?;
    let connection_rows = snapshot
        .connections
        .iter()
        .map(|connection| connection_json(store, tenant_id, connection))
        .collect::<Vec<_>>();
    let ready_connections = connection_rows
        .iter()
        .filter(|connection| {
            connection["credential_available"].as_bool() == Some(true)
                && connection["live_call_allowed"].as_bool() == Some(true)
                && matches!(connection["health"].as_str(), Some("healthy" | "observed"))
        })
        .collect::<Vec<_>>();
    let ready_connection_count = ready_connections.len();
    let ready_connection_ids = ready_connections
        .iter()
        .filter_map(|connection| connection["connection_id"].as_str().map(str::to_string))
        .collect::<std::collections::BTreeSet<_>>();
    let ready_deployments = snapshot
        .deployments
        .iter()
        .filter(|deployment| {
            deployment.enabled && ready_connection_ids.contains(&deployment.connection_id)
        })
        .collect::<Vec<_>>();
    let consumer_readiness = mdx_core::MODEL_WORKLOAD_CATALOG
        .iter()
        .map(|workload| {
            let reason = workload_block_reason(
                workload,
                &ready_deployments,
                &snapshot.connections,
                &ready_connection_ids,
            );
            json!({
                "consumer": workload.surface,
                "workload_id": workload.workload_id,
                "stage": workload.stage,
                "ready": reason.is_none(),
                "block_reason": reason,
                "next_action": workload_next_action(reason),
            })
        })
        .collect::<Vec<_>>();
    let ready_surface_count = consumer_readiness
        .iter()
        .filter_map(|row| {
            (row["ready"].as_bool() == Some(true))
                .then(|| row["consumer"].as_str())
                .flatten()
        })
        .collect::<BTreeSet<_>>()
        .len();
    let surface_count = mdx_core::MODEL_WORKLOAD_CATALOG
        .iter()
        .map(|workload| workload.surface)
        .collect::<BTreeSet<_>>()
        .len();
    let primary_deployment = ready_deployments.first();
    let primary_model = primary_deployment.and_then(|deployment| {
        snapshot
            .models
            .iter()
            .find(|model| model.model_id == deployment.model_id)
    });
    let primary_connection = primary_deployment.and_then(|deployment| {
        snapshot
            .connections
            .iter()
            .find(|connection| connection.connection_id == deployment.connection_id)
    });
    let ready = primary_deployment.is_some();
    let credential_missing = !snapshot.connections.is_empty()
        && connection_rows
            .iter()
            .all(|connection| connection["credential_available"].as_bool() != Some(true));
    let status = if ready {
        "READY"
    } else if snapshot.connections.is_empty() {
        "NEEDS_CONNECTION"
    } else if credential_missing {
        "NEEDS_CREDENTIAL"
    } else if snapshot.deployments.is_empty() {
        "NEEDS_MODEL"
    } else {
        "NEEDS_HEALTH_CHECK"
    };
    let next_action = match status {
        "READY" => {
            "Run a budget-capped model test or choose which workloads should prefer this model."
        }
        "NEEDS_CONNECTION" => {
            "Connect a managed, provider, enterprise, or local model in Model Center."
        }
        "NEEDS_CREDENTIAL" => {
            "Restore or rotate the credential named by the connection without changing app code."
        }
        "NEEDS_MODEL" => "Discover and select a model for the connected provider.",
        _ => "Run the connection health check and review its actionable failure.",
    };
    let source_receipt_ids = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            receipt.tenant_id.as_str() == tenant_id
                && matches!(
                    receipt.kind.as_str(),
                    mdx_core::MODEL_CONNECTION_CONFIGURED_RECEIPT_KIND
                        | mdx_core::MODEL_DEPLOYMENT_REGISTERED_RECEIPT_KIND
                )
        })
        .map(|receipt| receipt.receipt_id.clone())
        .collect::<Vec<_>>();
    Ok(json!({
        "name": "mdx-model-readiness",
        "status": status,
        "ready": ready,
        "connected": ready,
        "tenant_id": tenant_id,
        "connection_count": snapshot.connections.len(),
        "ready_connection_count": ready_connection_count,
        "deployment_count": snapshot.deployments.len(),
        "ready_deployment_count": ready_deployments.len(),
        "credential_available": primary_connection.is_some(),
        "provider_id": primary_connection.map(|connection| connection.provider_id.as_str()).unwrap_or(""),
        "model_id": primary_model.map(|model| model.provider_model_id.as_str()).unwrap_or(""),
        "deployment_id": primary_deployment.map(|deployment| deployment.deployment_id.as_str()).unwrap_or(""),
        "available_models": snapshot.models.iter().map(|model| model.provider_model_id.as_str()).collect::<Vec<_>>(),
        "connections": connection_rows,
        "consumer_readiness": consumer_readiness,
        "ready_surface_count": ready_surface_count,
        "surface_count": surface_count,
        "recommended_next_action": next_action,
        "access_policy": access_policy().as_json(),
        "canonical_route": "/models/readiness.json",
        "consumers": ["model_center", "twin", "forge", "web", "macos", "iphone"],
        "source_receipt_ids": source_receipt_ids,
        "secret_values_exposed": false,
        "grants_execution_authority": false,
        "production_write_allowed": false
    }))
}

fn readiness(kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    Ok(RouteResponse::json(
        "200 OK",
        readiness_value(kernel)?.to_string(),
    ))
}

fn connections(kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let tenant_id = tenant_for_read();
    let snapshot = crate::model_fabric_registry::global().snapshot_for_tenant(&tenant_id)?;
    let source_receipt_ids = model_receipt_ids(
        kernel,
        &tenant_id,
        &[
            mdx_core::MODEL_CONNECTION_CONFIGURED_RECEIPT_KIND,
            mdx_core::MODEL_CONNECTION_REFUSED_RECEIPT_KIND,
        ],
    )?;
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-model-connections",
            "tenant_id": tenant_id,
            "connection_count": snapshot.connections.len(),
            "connections": snapshot.connections.iter().map(|connection| connection_json(crate::secret_store::global(), &tenant_id, connection)).collect::<Vec<_>>(),
            "source_receipt_ids": source_receipt_ids,
            "secret_values_exposed": false,
            "grants_execution_authority": false
        })
        .to_string(),
    ))
}

fn configure_connection(
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid model connection request: {error}"))?;
    let (tenant_id, actor_id) = tenant_and_actor(&value);
    let provider_id = string(&value, "provider_id")?;
    let Some(provider) = model_provider(provider_id) else {
        return Err("model connection provider is not declared".to_string());
    };
    let action = value["action"].as_str().unwrap_or("configure");
    if matches!(action, "disconnect" | "revoke") {
        let snapshot = crate::model_fabric_registry::global().snapshot_for_tenant(&tenant_id)?;
        let mut connection = snapshot
            .connections
            .into_iter()
            .find(|connection| connection.provider_id == provider.provider_id)
            .ok_or_else(|| "model connection does not exist for this tenant".to_string())?;
        connection.health = if action == "revoke" {
            "revoked".to_string()
        } else {
            "disconnected".to_string()
        };
        connection.live_call_allowed = false;
        let local_credential_forgotten = if action == "revoke" {
            crate::secret_store::global().revoke_local_credential_for_connection(
                &tenant_id,
                &connection.credential_ref,
                provider.credential_env,
            )?
        } else {
            false
        };
        crate::model_fabric_registry::global().upsert_connection(connection.clone())?;
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let correlation = model_correlation(&mut kernel, &tenant_id, &actor_id);
        let receipt = kernel.record_model_connection_configured(&correlation, &connection);
        return Ok(RouteResponse::json(
            "200 OK",
            json!({
                "status": if action == "revoke" { "MODEL_CONNECTION_REVOKED" } else { "MODEL_CONNECTION_DISCONNECTED" },
                "connection": connection_json(crate::secret_store::global(), &tenant_id, &connection),
                "receipt_id": receipt.receipt_id,
                "local_credential_forgotten": local_credential_forgotten,
                "external_credential_deleted": false,
                "live_call_allowed": false,
                "secret_value_recorded": false,
                "grants_execution_authority": false
            })
            .to_string(),
        ));
    }
    if action != "configure" {
        return Err("model connection action must be configure, disconnect, or revoke".to_string());
    }
    let credential_ref = string(&value, "credential_ref")?;
    if !matches!(
        credential_ref.split(':').next(),
        Some("env" | "keychain" | "vault" | "secret" | "broker" | "file")
    ) {
        return Err("credential_ref must name an approved secret reference scheme".to_string());
    }
    enforce_model_access(
        provider.provider_id,
        connection_access_mode(provider.provider_id, "", credential_ref),
    )?;
    let endpoint_base_url = string(&value, "endpoint_base_url")?;
    crate::model_gateway_runtime::validate_base_url(
        endpoint_base_url,
        matches!(provider.provider_id, "ollama" | "vllm" | "tensorzero"),
    )?;
    let connection = ProviderConnection {
        connection_id: value["connection_id"]
            .as_str()
            .unwrap_or(&format!("{tenant_id}:{provider_id}"))
            .to_string(),
        tenant_id: tenant_id.clone(),
        provider_id: provider_id.to_string(),
        credential_ref: credential_ref.to_string(),
        endpoint_base_url: endpoint_base_url.to_string(),
        region: value["region"].as_str().unwrap_or("global").to_string(),
        residency: value["residency"]
            .as_str()
            .unwrap_or("provider_managed")
            .to_string(),
        data_retention: value["data_retention"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        training_policy: value["training_policy"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        data_policy_provenance: string(&value, "data_policy_provenance")?.to_string(),
        data_policy_observed_at: string(&value, "data_policy_observed_at")?.to_string(),
        health: "disconnected".to_string(),
        live_call_allowed: false,
    };
    crate::model_fabric_registry::global().upsert_connection(connection.clone())?;
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let correlation = model_correlation(&mut kernel, &tenant_id, &actor_id);
    let receipt = kernel.record_model_connection_configured(&correlation, &connection);
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "status": "MODEL_CONNECTION_CONFIGURED_CALL_BLOCKED",
            "connection": connection_json(crate::secret_store::global(), &tenant_id, &connection),
            "receipt_id": receipt.receipt_id,
            "health_check_required": true,
            "live_call_allowed": false,
            "secret_value_recorded": false
        })
        .to_string(),
    ))
}

fn deployments(kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let tenant_id = tenant_for_read();
    let snapshot = crate::model_fabric_registry::global().snapshot_for_tenant(&tenant_id)?;
    let prices = snapshot
        .prices
        .iter()
        .map(|price| (price.deployment_id.as_str(), price))
        .collect::<std::collections::BTreeMap<_, _>>();
    let rows = snapshot
        .deployments
        .iter()
        .map(|deployment| {
            let price = prices.get(deployment.deployment_id.as_str());
            json!({
                "deployment_id": deployment.deployment_id,
                "connection_id": deployment.connection_id,
                "provider_id": deployment.provider_id,
                "model_id": deployment.model_id,
                "privacy_class": deployment.privacy_class,
                "region": deployment.region,
                "residency": deployment.residency,
                "enabled": deployment.enabled,
                "capabilities": {
                    "tools": deployment.capabilities.tools,
                    "structured_output": deployment.capabilities.structured_output,
                    "vision": deployment.capabilities.vision,
                    "context_tokens": deployment.capabilities.context_tokens,
                    "provenance": deployment.capabilities.provenance,
                    "observed_at": deployment.capabilities.observed_at
                },
                "price": price.map(|price| json!({
                    "input_microusd_per_million": price.input_microusd_per_million,
                    "output_microusd_per_million": price.output_microusd_per_million,
                    "currency": price.currency,
                    "source": price.source,
                    "observed_at": price.observed_at
                }))
            })
        })
        .collect::<Vec<_>>();
    let source_receipt_ids = model_receipt_ids(
        kernel,
        &tenant_id,
        &[mdx_core::MODEL_DEPLOYMENT_REGISTERED_RECEIPT_KIND],
    )?;
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-model-deployments",
            "tenant_id": tenant_id,
            "model_count": snapshot.models.len(),
            "deployment_count": rows.len(),
            "deployments": rows,
            "source_receipt_ids": source_receipt_ids,
            "grants_execution_authority": false
        })
        .to_string(),
    ))
}

fn register_deployment(
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid model deployment request: {error}"))?;
    let (tenant_id, actor_id) = tenant_and_actor(&value);
    let provider_id = string(&value, "provider_id")?;
    if model_provider(provider_id).is_none() {
        return Err("model deployment provider is not declared".to_string());
    }
    let model_id = string(&value, "model_id")?;
    let deployment_id = string(&value, "deployment_id")?;
    let model = Model {
        model_id: model_id.to_string(),
        provider_model_id: value["provider_model_id"]
            .as_str()
            .unwrap_or(model_id)
            .to_string(),
        provider_id: provider_id.to_string(),
        display_name: value["display_name"]
            .as_str()
            .unwrap_or(model_id)
            .to_string(),
        lifecycle: value["lifecycle"].as_str().unwrap_or("active").to_string(),
        modality: value["modality"].as_str().unwrap_or("text").to_string(),
    };
    let deployment = ModelDeployment {
        deployment_id: deployment_id.to_string(),
        connection_id: string(&value, "connection_id")?.to_string(),
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        privacy_class: value["privacy_class"]
            .as_str()
            .unwrap_or("frontier")
            .to_string(),
        region: value["region"].as_str().unwrap_or("global").to_string(),
        residency: value["residency"]
            .as_str()
            .unwrap_or("provider_managed")
            .to_string(),
        enabled: value["enabled"].as_bool().unwrap_or(true),
        capabilities: CapabilityProfile {
            tools: boolean(&value, "tools"),
            structured_output: boolean(&value, "structured_output"),
            vision: boolean(&value, "vision"),
            context_tokens: number(&value, "context_tokens"),
            provenance: string(&value, "capability_provenance")?.to_string(),
            observed_at: string(&value, "capability_observed_at")?.to_string(),
        },
    };
    let price = ModelPriceObservation {
        deployment_id: deployment_id.to_string(),
        input_microusd_per_million: number(&value, "input_microusd_per_million"),
        output_microusd_per_million: number(&value, "output_microusd_per_million"),
        currency: value["currency"].as_str().unwrap_or("USD").to_string(),
        source: string(&value, "price_source")?.to_string(),
        observed_at: string(&value, "price_observed_at")?.to_string(),
    };
    let registry = crate::model_fabric_registry::global();
    registry.upsert_model(&tenant_id, model.clone())?;
    registry.upsert_deployment(&tenant_id, deployment.clone(), price.clone())?;
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let correlation = model_correlation(&mut kernel, &tenant_id, &actor_id);
    let receipt =
        kernel.record_model_deployment_registered(&correlation, &model, &deployment, &price);
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "status": "MODEL_DEPLOYMENT_REGISTERED_CALL_BLOCKED",
            "deployment_id": deployment.deployment_id,
            "model_id": model.model_id,
            "receipt_id": receipt.receipt_id,
            "live_call_allowed": false,
            "grants_execution_authority": false
        })
        .to_string(),
    ))
}

fn policies(kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let tenant_id = tenant_for_read();
    let snapshot = crate::model_fabric_registry::global().snapshot_for_tenant(&tenant_id)?;
    let rows = snapshot
        .policies
        .iter()
        .map(|policy| {
            json!({
                "policy_id": policy.policy_id,
                "policy_version": policy.policy_version,
                "lifecycle": policy.lifecycle,
                "state": policy.state,
                "canary_percent": policy.canary_percent,
                "workload_ids": policy.workload_ids,
                "allowed_app_ids": policy.allowed_app_ids,
                "allowed_environments": policy.allowed_environments,
                "allowed_provider_ids": policy.allowed_provider_ids,
                "preferred_deployment_ids": policy.preferred_deployment_ids,
                "denied_provider_ids": policy.denied_provider_ids,
                "denied_deployment_ids": policy.denied_deployment_ids,
                "allowed_regions": policy.allowed_regions,
                "required_residency": policy.required_residency,
                "allow_data_retention": policy.allow_data_retention,
                "allow_training": policy.allow_training,
                "max_input_cost_microusd_per_million": policy.max_input_cost_microusd_per_million,
                "max_output_cost_microusd_per_million": policy.max_output_cost_microusd_per_million,
                "grants_execution_authority": false
            })
        })
        .collect::<Vec<_>>();
    let source_receipt_ids = model_receipt_ids(
        kernel,
        &tenant_id,
        &[mdx_core::MODEL_ROUTE_POLICY_CONFIGURED_RECEIPT_KIND],
    )?;
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-model-route-policies",
            "tenant_id": tenant_id,
            "policy_count": rows.len(),
            "policies": rows,
            "source_receipt_ids": source_receipt_ids,
            "grants_execution_authority": false
        })
        .to_string(),
    ))
}

fn configure_policy(body: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid model policy request: {error}"))?;
    let (tenant_id, actor_id) = tenant_and_actor(&value);
    let allowed_provider_ids = strings(&value, "allowed_provider_ids");
    let denied_provider_ids = strings(&value, "denied_provider_ids");
    for provider_id in allowed_provider_ids
        .iter()
        .chain(denied_provider_ids.iter())
    {
        if model_provider(provider_id).is_none() {
            return Err(format!("model policy names unknown provider {provider_id}"));
        }
    }
    let workload_ids = strings(&value, "workload_ids");
    for workload_id in &workload_ids {
        if mdx_core::model_workload(workload_id).is_none() {
            return Err(format!("model policy names unknown workload {workload_id}"));
        }
    }
    let lifecycle = value["lifecycle"].as_str().unwrap_or("static");
    if !matches!(lifecycle, "static" | "adaptive") {
        return Err("model policy lifecycle must be static or adaptive".to_string());
    }
    let canary_percent = value["canary_percent"].as_u64().unwrap_or(0);
    if canary_percent > 100 {
        return Err("model policy canary percent must be between 0 and 100".to_string());
    }
    let policy = RoutePolicy {
        policy_id: string(&value, "policy_id")?.to_string(),
        policy_version: string(&value, "policy_version")?.to_string(),
        lifecycle: lifecycle.to_string(),
        state: if lifecycle == "adaptive" {
            "draft"
        } else {
            "promoted"
        }
        .to_string(),
        canary_percent: canary_percent as u8,
        tenant_id: tenant_id.clone(),
        workload_ids,
        allowed_app_ids: strings(&value, "allowed_app_ids"),
        allowed_environments: strings(&value, "allowed_environments"),
        allowed_provider_ids,
        preferred_deployment_ids: strings(&value, "preferred_deployment_ids"),
        denied_provider_ids,
        denied_deployment_ids: strings(&value, "denied_deployment_ids"),
        allowed_regions: strings(&value, "allowed_regions"),
        required_residency: value["required_residency"].as_str().map(str::to_string),
        allow_data_retention: value["allow_data_retention"].as_bool().unwrap_or(true),
        allow_training: value["allow_training"].as_bool().unwrap_or(false),
        max_input_cost_microusd_per_million: number(&value, "max_input_cost_microusd_per_million"),
        max_output_cost_microusd_per_million: number(
            &value,
            "max_output_cost_microusd_per_million",
        ),
    };
    crate::model_fabric_registry::global().upsert_policy(policy.clone())?;
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let correlation = model_correlation(&mut kernel, &tenant_id, &actor_id);
    let receipt = kernel.record_model_route_policy_configured(&correlation, &policy);
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "status": "MODEL_ROUTE_POLICY_CONFIGURED",
            "policy_id": policy.policy_id,
            "policy_version": policy.policy_version,
            "receipt_id": receipt.receipt_id,
            "grants_execution_authority": false
        })
        .to_string(),
    ))
}

fn model_correlation(kernel: &mut MdxKernel, tenant_id: &str, actor_id: &str) -> CorrelationIds {
    CorrelationIds {
        tenant_id: TenantId::new(tenant_id),
        trace_id: TraceId::new(kernel.mint_id("trace")),
        actor_id: ActorId::new(actor_id),
        loop_id: LoopId::new("model_fabric"),
        workflow_id: WorkflowId::new(kernel.mint_id("workflow")),
    }
}

fn method_not_allowed() -> RouteResponse {
    RouteResponse::text("405 Method Not Allowed", "method not allowed\n".to_string())
}

fn resolve(body: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let value: Value = serde_json::from_str(body)
        .map_err(|error| format!("invalid model route request: {error}"))?;
    let (tenant_id, actor_id) = tenant_and_actor(&value);
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let selected = crate::model_fabric_service::select(
        &mut kernel,
        &crate::model_fabric_service::ModelSelectionRequest {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            workload_id: string(&value, "workload_id")?,
            preset: value["preset"].as_str().unwrap_or(""),
            policy_id: value["policy_id"].as_str().unwrap_or("core"),
            expected_policy_version: value["policy_version"].as_str(),
            required_region: value["required_region"].as_str(),
            required_residency: value["required_residency"].as_str(),
            session_id: value["session_id"].as_str(),
            pinned_deployment_id: value["pinned_deployment_id"].as_str(),
            required_context_tokens: number(&value, "required_context_tokens"),
            max_input_cost_microusd_per_million: number(
                &value,
                "max_input_cost_microusd_per_million",
            ),
            max_output_cost_microusd_per_million: number(
                &value,
                "max_output_cost_microusd_per_million",
            ),
        },
    )?;
    let decision = selected.decision;
    let response = json!({
        "name": "mdx-model-route-decision-local-post",
        "status": decision.status,
        "workload_id": decision.workload_id,
        "app_id": decision.app_id,
        "environment": decision.environment,
        "session_id": decision.session_id,
        "preset": decision.preset,
        "policy_id": decision.policy_id,
        "policy_version": decision.policy_version,
        "selected_deployment_id": decision.selected_deployment_id,
        "selected_provider_id": decision.selected_provider_id,
        "selected_model_id": decision.selected_model_id,
        "selection_reason": decision.selection_reason,
        "session_sticky_applied": decision.session_sticky_applied,
        "provider_failover_deployment_ids": decision.provider_failover_deployment_ids,
        "model_fallback_deployment_ids": decision.model_fallback_deployment_ids,
        "fallback_deployment_ids": decision.fallback_deployment_ids,
        "exclusions": decision.exclusions.iter().map(|row| json!({"deployment_id": row.deployment_id, "reason": row.reason})).collect::<Vec<_>>(),
        "grants_execution_authority": decision.grants_execution_authority,
        "adaptive_policy_state": "static",
        "production_call_performed": false,
        "decision_receipt_id": selected.decision_receipt_id
    });
    Ok(RouteResponse::json("200 OK", response.to_string()))
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value[key]
        .as_str()
        .filter(|entry| !entry.trim().is_empty())
        .ok_or_else(|| format!("model route request requires {key}"))
}

fn number(value: &Value, key: &str) -> u64 {
    value[key].as_u64().unwrap_or(0)
}

fn connection_price_observation(
    deployment_id: &str,
    provider_id: &str,
    provider_model_id: &str,
    value: &Value,
    local: bool,
) -> ModelPriceObservation {
    let supplied_input = number(value, "input_microusd_per_million");
    let supplied_output = number(value, "output_microusd_per_million");
    let supplied_source = value["price_source"].as_str().unwrap_or("").trim();
    if supplied_input > 0 && supplied_output > 0 && !supplied_source.is_empty() {
        return ModelPriceObservation {
            deployment_id: deployment_id.to_string(),
            input_microusd_per_million: supplied_input,
            output_microusd_per_million: supplied_output,
            currency: "USD".to_string(),
            source: supplied_source.to_string(),
            observed_at: value["price_observed_at"]
                .as_str()
                .unwrap_or("operator_supplied")
                .to_string(),
        };
    }
    if local {
        return ModelPriceObservation {
            deployment_id: deployment_id.to_string(),
            input_microusd_per_million: 0,
            output_microusd_per_million: 0,
            currency: "USD".to_string(),
            source: "self_hosted_local".to_string(),
            observed_at: "connect_receipt_timestamp".to_string(),
        };
    }

    // These are conservative enforcement rates, not billing statements. The
    // higher published long-context rate is used where the provider has one so
    // a workload cannot escape its cap by crossing a context threshold. An
    // operator can supply a newer observed rate in the connect request.
    let model = provider_model_id.to_ascii_lowercase();
    let (input, output, source) = match (provider_id, model.as_str()) {
        ("xai", id) if id.contains("grok-4.5") => (
            4_000_000,
            12_000_000,
            "provider_catalog_long_context:https://docs.x.ai/developers/pricing",
        ),
        ("anthropic", id) if id.contains("claude-sonnet-5") => (
            6_000_000,
            22_500_000,
            "provider_catalog_long_context:https://docs.anthropic.com/en/docs/about-claude/pricing",
        ),
        ("openai", id) if id.contains("gpt-5.6-terra") => (
            5_000_000,
            22_500_000,
            "provider_catalog_long_context:https://developers.openai.com/api/docs/pricing",
        ),
        _ => (
            15_000_000,
            75_000_000,
            "conservative_unknown_model_upper_bound:v1",
        ),
    };
    ModelPriceObservation {
        deployment_id: deployment_id.to_string(),
        input_microusd_per_million: input,
        output_microusd_per_million: output,
        currency: "USD".to_string(),
        source: source.to_string(),
        observed_at: "2026-07-18".to_string(),
    }
}

fn boolean(value: &Value, key: &str) -> bool {
    value[key].as_bool().unwrap_or(false)
}

fn strings(value: &Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed_deployment(tenant_id: &str, deployment_id: &str, provider_id: &str) {
        let registry = crate::model_fabric_registry::global();
        registry.clear_tenant(tenant_id);
        if let Some(provider) = mdx_core::model_provider(provider_id) {
            crate::secret_store::global().set_for_tenant(
                tenant_id,
                provider.credential_env,
                "test-key",
            );
        }
        let connection_id = format!("{tenant_id}:{provider_id}");
        registry
            .upsert_connection(mdx_core::ProviderConnection {
                connection_id: connection_id.clone(),
                tenant_id: tenant_id.to_string(),
                provider_id: provider_id.to_string(),
                credential_ref: mdx_core::model_provider(provider_id)
                    .map(|provider| format!("secret:session/{}", provider.credential_env))
                    .unwrap_or_else(|| format!("secret:session/{provider_id}")),
                endpoint_base_url: "https://provider.invalid".to_string(),
                region: "us-east".to_string(),
                residency: "us".to_string(),
                data_retention: "none".to_string(),
                training_policy: "none".to_string(),
                data_policy_provenance: "test_fixture".to_string(),
                data_policy_observed_at: "2026-07-12".to_string(),
                health: "observed".to_string(),
                live_call_allowed: true,
            })
            .expect("connection");
        registry
            .upsert_model(
                tenant_id,
                mdx_core::Model {
                    model_id: deployment_id.to_string(),
                    provider_model_id: deployment_id.to_string(),
                    provider_id: provider_id.to_string(),
                    display_name: deployment_id.to_string(),
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
                    connection_id,
                    provider_id: provider_id.to_string(),
                    model_id: deployment_id.to_string(),
                    privacy_class: "frontier".to_string(),
                    region: "us-east".to_string(),
                    residency: "us".to_string(),
                    enabled: true,
                    capabilities: crate::model_fabric_registry::observed_capability_profile(
                        true,
                        true,
                        false,
                        100_000,
                        "test",
                        "test-time",
                    ),
                },
                mdx_core::ModelPriceObservation {
                    deployment_id: deployment_id.to_string(),
                    input_microusd_per_million: 500,
                    output_microusd_per_million: 1_000,
                    currency: "USD".to_string(),
                    source: "test".to_string(),
                    observed_at: "test-time".to_string(),
                },
            )
            .expect("deployment");
    }

    #[test]
    fn embedded_contract_and_route_are_parseable() {
        let contract: Value = serde_json::from_str(FABRIC_JSON).expect("fabric json");
        assert_eq!(contract["authority"], "selection-only");
        seed_deployment("route-test", "fast-open", "openai");
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = resolve(r#"{"tenant_id":"route-test","app_id":"forge","workload_id":"mdx/forge/builder","preset":"fast","required_context_tokens":1000}"#, &kernel).expect("route");
        assert!(
            response
                .body_text()
                .contains(r#""selected_deployment_id":"fast-open""#)
        );
        assert!(
            response
                .body_text()
                .contains(r#""grants_execution_authority":false"#)
        );
        assert!(response.body_text().contains("decision_receipt_id"));
        assert_eq!(
            kernel
                .read()
                .unwrap()
                .ledger()
                .query()
                .by_kind("model.route.selected")
                .len(),
            1
        );
    }

    #[test]
    fn model_test_without_live_authority_cites_its_receipt_policy() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = test_model("{}", &kernel).expect("model test response");
        let body: Value = serde_json::from_str(response.body_text()).expect("response json");

        assert_eq!(body["status"], "MODEL_TEST_NOT_RUN");
        assert_eq!(body["provider_call_performed"], false);
        assert_eq!(body["source_receipt_ids"], json!([]));
        assert!(
            body["receipt_reason"]
                .as_str()
                .is_some_and(|reason| reason.contains("explicit live authorization"))
        );
    }

    #[test]
    fn connection_prices_flagship_models_conservatively_and_unknown_models_fail_safe() {
        let empty = json!({});
        for (provider, model, input, output) in [
            ("xai", "grok-4.5", 4_000_000, 12_000_000),
            ("anthropic", "claude-sonnet-5", 6_000_000, 22_500_000),
            ("openai", "gpt-5.6-terra", 5_000_000, 22_500_000),
            ("future-provider", "future-model", 15_000_000, 75_000_000),
        ] {
            let price = connection_price_observation("deployment", provider, model, &empty, false);
            assert_eq!(price.input_microusd_per_million, input);
            assert_eq!(price.output_microusd_per_million, output);
            assert_ne!(price.source, "unknown");
        }
    }

    #[test]
    fn connection_price_accepts_complete_operator_observation_only() {
        let supplied = json!({
            "input_microusd_per_million": 123,
            "output_microusd_per_million": 456,
            "price_source": "operator_rate_card",
            "price_observed_at": "2026-07-18"
        });
        let price = connection_price_observation("deployment", "xai", "grok-4.5", &supplied, false);
        assert_eq!(price.input_microusd_per_million, 123);
        assert_eq!(price.output_microusd_per_million, 456);
        assert_eq!(price.source, "operator_rate_card");

        let incomplete = json!({
            "input_microusd_per_million": 123,
            "price_source": "partial"
        });
        let fallback =
            connection_price_observation("deployment", "xai", "grok-4.5", &incomplete, false);
        assert_eq!(fallback.input_microusd_per_million, 4_000_000);
        assert_eq!(fallback.output_microusd_per_million, 12_000_000);
    }

    #[test]
    fn provider_catalog_parsing_is_normalized_and_recommended_for_text() {
        let models = parse_provider_models(&json!({
            "models": [
                {"name": "models/text-standard"},
                {"name": "models/text-pro-latest"},
                {"name": "models/embedding-latest"},
                {"name": "models/text-standard"}
            ]
        }));
        assert_eq!(
            models,
            vec![
                "embedding-latest".to_string(),
                "text-pro-latest".to_string(),
                "text-standard".to_string()
            ]
        );
        assert_eq!(
            recommended_model(&models).as_deref(),
            Some("text-pro-latest")
        );
    }

    #[test]
    fn canonical_readiness_uses_exact_connection_credential() {
        let tenant_id = "readiness-exact-credential";
        seed_deployment(tenant_id, "ready-openai", "openai");
        let kernel = MdxKernel::boot_local();
        let readiness =
            readiness_value_for_tenant(&kernel, tenant_id, crate::secret_store::global())
                .expect("readiness");
        assert_eq!(readiness["status"], "READY");
        assert_eq!(readiness["ready"], true);
        assert_eq!(readiness["provider_id"], "openai");
        assert_eq!(readiness["model_id"], "ready-openai");
        assert_eq!(readiness["secret_values_exposed"], false);
        let forge_builder = readiness["consumer_readiness"]
            .as_array()
            .expect("consumer readiness")
            .iter()
            .find(|row| row["workload_id"] == "mdx/forge/builder")
            .expect("forge builder readiness");
        assert_eq!(forge_builder["ready"], true);

        let registry = crate::model_fabric_registry::global();
        let snapshot = registry.snapshot_for_tenant(tenant_id).expect("snapshot");
        let mut deployment = snapshot.deployments[0].clone();
        deployment.capabilities.structured_output = false;
        registry
            .upsert_deployment(tenant_id, deployment, snapshot.prices[0].clone())
            .expect("deployment without structured output");
        let readiness =
            readiness_value_for_tenant(&kernel, tenant_id, crate::secret_store::global())
                .expect("readiness after capability change");
        let forge_builder = readiness["consumer_readiness"]
            .as_array()
            .expect("consumer readiness")
            .iter()
            .find(|row| row["workload_id"] == "mdx/forge/builder")
            .expect("forge builder readiness");
        assert_eq!(forge_builder["ready"], false);
        assert_eq!(
            forge_builder["block_reason"],
            "needs_structured_output_confirmation"
        );
    }

    #[test]
    fn access_modes_are_explicit_and_connection_shaped() {
        assert_eq!(
            csv_set("byok, managed,enterprise"),
            ["byok", "enterprise", "managed"]
                .into_iter()
                .map(str::to_string)
                .collect()
        );
        assert_eq!(connection_access_mode("openai", "sk-test", ""), "byok");
        assert_eq!(
            connection_access_mode("openai", "", "secret:managed/OPENAI_API_KEY"),
            "managed"
        );
        assert_eq!(
            connection_access_mode("openai", "", "vault:teams/mdx/openai"),
            "enterprise"
        );
        assert_eq!(connection_access_mode("ollama", "", ""), "local");
    }

    #[test]
    fn disconnect_stops_routing_without_a_restart() {
        let tenant_id = "disconnect-without-restart";
        seed_deployment(tenant_id, "openai-disconnect", "openai");
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = configure_connection(
            &format!(
                r#"{{"tenant_id":"{tenant_id}","actor_id":"human:operator","provider_id":"openai","action":"disconnect"}}"#
            ),
            &kernel,
        )
        .expect("disconnect");
        assert!(
            response
                .body_text()
                .contains("MODEL_CONNECTION_DISCONNECTED")
        );
        let guard = kernel.read().expect("kernel");
        let readiness =
            readiness_value_for_tenant(&guard, tenant_id, crate::secret_store::global())
                .expect("readiness");
        assert_eq!(readiness["ready"], false);
        assert_eq!(readiness["status"], "NEEDS_HEALTH_CHECK");
    }

    #[test]
    fn server_registry_refuses_an_unknown_provider_before_routing() {
        let error = crate::model_fabric_registry::global()
            .upsert_connection(mdx_core::ProviderConnection {
                connection_id: "unknown-test:mystery".to_string(),
                tenant_id: "unknown-test".to_string(),
                provider_id: "mystery".to_string(),
                credential_ref: "env:MYSTERY_KEY".to_string(),
                endpoint_base_url: "https://provider.invalid".to_string(),
                region: "global".to_string(),
                residency: "provider_managed".to_string(),
                data_retention: "unknown".to_string(),
                training_policy: "unknown".to_string(),
                data_policy_provenance: "unknown".to_string(),
                data_policy_observed_at: "test-time".to_string(),
                health: "disconnected".to_string(),
                live_call_allowed: false,
            })
            .expect_err("unknown provider");
        assert!(error.contains("not declared"));
    }

    #[test]
    fn connection_refusal_is_receipt_backed_without_recording_a_secret() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = connect_model(
            r#"{"tenant_id":"connect-refusal-test","actor_id":"human:operator","provider_id":"openai","model_id":"gpt-test"}"#,
            &kernel,
        )
        .expect("refusal");
        assert!(response.body_text().contains("MODEL_CONNECTION_REFUSED"));
        assert!(response.body_text().contains("receipt_id"));
        assert!(
            response
                .body_text()
                .contains(r#""secret_value_recorded":false"#)
        );
        let guard = kernel.read().expect("kernel");
        let receipts = guard
            .ledger()
            .query()
            .by_kind(mdx_core::MODEL_CONNECTION_REFUSED_RECEIPT_KIND);
        assert_eq!(receipts.len(), 1);
        assert_eq!(
            receipts[0].payload.get("provider_id").map(String::as_str),
            Some("openai")
        );
        assert_eq!(
            receipts[0]
                .payload
                .get("secret_value_recorded")
                .map(String::as_str),
            Some("false")
        );
    }

    #[test]
    fn route_uses_server_owned_policy_and_ignores_client_policy_assertions() {
        seed_deployment("policy-test", "openai-test", "openai");
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let configured = configure_policy(
            r#"{"tenant_id":"policy-test","actor_id":"human:operator","policy_id":"deny-openai","policy_version":"v1","denied_provider_ids":["openai"]}"#,
            &kernel,
        )
        .expect("policy");
        assert!(
            configured
                .body_text()
                .contains("MODEL_ROUTE_POLICY_CONFIGURED")
        );
        let response = resolve(
            r#"{"tenant_id":"policy-test","workload_id":"mdx/twin/conversation","policy_id":"deny-openai","denied_provider_ids":[],"allowed_provider_ids":["openai"],"allow_training":true}"#,
            &kernel,
        )
        .expect("route");
        assert!(response.body_text().contains("ROUTE_DENIED"));
        assert!(response.body_text().contains("provider_denied"));
    }

    #[test]
    fn one_runtime_outcome_cannot_be_counted_as_repeated_evaluator_evidence() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let source_id = {
            let mut guard = kernel.write().expect("kernel");
            let correlation = model_correlation(&mut guard, "outcome-eval-test", "human:operator");
            guard
                .record_model_outcome(
                    &correlation,
                    &mdx_core::ModelOutcome {
                        decision_id: "decision-1".to_string(),
                        workload_id: "mdx/forge/builder".to_string(),
                        app_id: "forge".to_string(),
                        environment: "local".to_string(),
                        session_id: "session-1".to_string(),
                        deployment_id: "deployment-1".to_string(),
                        latency_ms: 100,
                        cost_microusd: Some(10),
                        quality_score: None,
                        safety_status: "not_evaluated".to_string(),
                        task_status: "not_evaluated".to_string(),
                        correction_status: "not_observed".to_string(),
                        provenance: "test_runtime".to_string(),
                    },
                )
                .expect("runtime outcome")
                .receipt_id
        };
        let body = format!(
            r#"{{"tenant_id":"outcome-eval-test","actor_id":"human:operator","outcome_id":"{source_id}","quality_score":90,"safety_status":"passed","task_status":"succeeded"}}"#
        );
        assert!(
            evaluate_outcome(&body, &kernel)
                .expect("first evaluation")
                .body_text()
                .contains("MODEL_OUTCOME_EVALUATED")
        );
        assert!(
            evaluate_outcome(&body, &kernel)
                .expect("duplicate evaluation")
                .body_text()
                .contains("already has an evaluator judgment")
        );
    }
}
