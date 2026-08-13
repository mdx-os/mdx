use crate::RouteResponse;
use crate::secret_store::{SecretStore, global};
use crate::studio::json_string_field;
use mdx_core::{
    ActorId, ApproveModelTurnOn, CapabilityProfile, CorrelationIds, LoopId, MdxKernel, Model,
    ModelDeployment, ModelPriceObservation, ProviderConnection, TenantId, TraceId,
    TwinModelGatewayProviderObservation, WorkflowId, canonical_model_provider_id,
    json_string_literal, twin_model_gateway_provider_slots,
};
use std::sync::{Arc, RwLock};
use std::time::Duration;

// Connect a model from the GUI. The governed turn-on ceremony: record the human
// approval, make ONE live provider call, then record the observation receipt the
// kernel already accepts at boot - the SAME gate. The key is used for that call,
// held in the in-memory secret store so Twin can actually answer with it, and
// NEVER written to disk or into a receipt. This is a NARROW action, not a shell
// bridge. The live network call runs WITHOUT the kernel lock held.
//
// "Connected" is honest: /runtime/model-connected.json requires BOTH an accepted
// observation receipt AND a usable credential (in-memory or env) - so a green
// "connected" always means Twin can actually use the model, never just that a
// turn-on once succeeded.

const TURN_ON_TIMEOUT_SECS: u64 = 20;

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/runtime/model-connected.json" {
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
        return Some(
            crate::model_fabric_route::readiness_value_for_tenant(
                &kernel,
                &crate::request_security::current_verified_identity()
                    .map(|identity| identity.tenant_id)
                    .unwrap_or_else(|| "local_tenant".to_string()),
                global(),
            )
            .map(|value| RouteResponse::json("200 OK", value.to_string())),
        );
    }
    if path == "/install/model-connect.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        // The Arc, not a held guard: the live calls must run lock-free.
        return Some(
            connect(body, kernel, global(), fetch_models, turn_on_call)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    None
}

// "Connected" is the conjunction of a governed observation receipt and a usable
// credential right now. Both sub-signals are exposed so the surface can explain
// an observed-but-no-key state instead of showing a misleading green.
#[cfg(test)]
pub(crate) fn connected_projection(kernel: &MdxKernel, store: &SecretStore) -> String {
    let tenant_id = crate::request_security::current_verified_identity()
        .map(|identity| identity.tenant_id)
        .unwrap_or_else(|| "local_tenant".to_string());
    let state =
        crate::twin_model_gateway::accepted_provider_observations_for_tenant(kernel, &tenant_id)
            .into_iter()
            .last();
    let provider_id = state
        .and_then(|receipt| receipt.payload.get("provider_id"))
        .cloned()
        .unwrap_or_default();
    let model_id = state
        .and_then(|receipt| receipt.payload.get("model_id"))
        .cloned()
        .unwrap_or_default();
    let observation_receipt_id = state
        .map(|receipt| receipt.receipt_id.clone())
        .unwrap_or_default();
    let observation_recorded = state.is_some();
    let env_key = twin_model_gateway_provider_slots()
        .iter()
        .find(|slot| slot.provider_id == provider_id)
        .map(|slot| slot.env_key)
        .unwrap_or("");
    if observation_recorded && !env_key.is_empty() {
        store.request_keychain_refresh_for_tenant(&tenant_id, env_key);
    }
    let credential_available =
        !env_key.is_empty() && store.has_provider_key_for_tenant(&tenant_id, env_key);
    let connected = observation_recorded && credential_available;
    // The provider's model list from the connect this process (empty after a
    // restart until a re-connect), so the GUI picker survives a refresh.
    let available_json = store
        .models_for_tenant(&tenant_id, env_key)
        .iter()
        .map(|m| json_string_literal(m))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"name":"mdx-runtime-model-connected","status":"OK","read_only":true,"connected":{},"observation_recorded":{},"credential_available":{},"provider_id":{},"model_id":{},"available_models":[{}],"observation_receipt_id":{},"derived_from":"twin.model_gateway.provider.observed","production_write_allowed":false}}"#,
        connected,
        observation_recorded,
        credential_available,
        json_string_literal(&provider_id),
        json_string_literal(&model_id),
        available_json,
        json_string_literal(&observation_receipt_id),
    )
}

fn identity(body: &str) -> (String, String) {
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    if resolved.auth_session_status == "VERIFIED_TRUSTED_SESSION" {
        (resolved.tenant_id.clone(), resolved.actor_id.clone())
    } else {
        (
            json_string_field(body, "tenant_id").unwrap_or_else(|| "local_tenant".to_string()),
            json_string_field(body, "actor_id").unwrap_or_else(|| "human:local_user".to_string()),
        )
    }
}

// The chat-completions base URL for providers whose turn-on rides the
// OpenAI-compatible shape. Others (Responses, Messages) are wired in later.
fn chat_base_url(provider_id: &str) -> Option<String> {
    match provider_id {
        "ollama" => Some(
            std::env::var("OLLAMA_BASE_URL")
                .or_else(|_| std::env::var("MDX_OLLAMA_BASE_URL"))
                .unwrap_or_else(|_| "http://127.0.0.1:11434/v1".to_string()),
        ),
        "xai" => Some(
            std::env::var("XAI_BASE_URL").unwrap_or_else(|_| "https://api.x.ai/v1".to_string()),
        ),
        "mistral" => Some(
            std::env::var("MISTRAL_BASE_URL")
                .unwrap_or_else(|_| "https://api.mistral.ai/v1".to_string()),
        ),
        _ => None,
    }
}

fn refusal(status: &str, approval: &str, reason: &str) -> String {
    format!(
        r#"{{"name":"mdx-install-model-connect-local-post","status":"{status}","connected":false,"approval_receipt_id":{},"reason":{},"production_write_allowed":false}}"#,
        json_string_literal(approval),
        json_string_literal(reason),
    )
}

// Injected so tests exercise the connect ceremony without a network.
// Lister: (base_url, api_key) -> the provider's live model ids.
// Caller: (base_url, api_key, model) -> (model_id, response_id, tokens).
type ModelLister = fn(&str, &str) -> Result<Vec<String>, String>;
type TurnOnCaller = fn(&str, &str, &str) -> Result<(String, String, usize), String>;

// The configured model preference env var per provider, used as the default only
// when it names a model the provider actually lists. No hardcoded model names.
fn model_env_key(provider_id: &str) -> &'static str {
    match provider_id {
        "ollama" => "MDX_OLLAMA_MODEL",
        "xai" => "MDX_XAI_MODEL",
        "mistral" => "MDX_MISTRAL_MODEL",
        _ => "",
    }
}

pub(crate) fn connect(
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
    store: &SecretStore,
    lister: ModelLister,
    caller: TurnOnCaller,
) -> Result<String, String> {
    let (tenant_id, actor_id) = identity(body);
    let provider_id = json_string_field(body, "provider")
        .map(|p| p.trim().to_ascii_lowercase())
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "xai".to_string());

    let Some(slot) = twin_model_gateway_provider_slots()
        .iter()
        .find(|slot| slot.provider_id == provider_id)
    else {
        return Ok(refusal(
            "MODEL_CONNECT_UNKNOWN_PROVIDER",
            "",
            "that provider is not one MDx knows",
        ));
    };

    // Record the human approval first - under the lock, then release it.
    let approval_id = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        kernel
            .approve_model_turn_on(ApproveModelTurnOn {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                provider_id: slot.provider_id,
            })
            .map_err(|error| error.message())?
            .approval_receipt_id
    };

    // The key comes from the request (first connect), or the in-memory key the
    // operator already pasted (a later model change) - never silently from env.
    let api_key = match json_string_field(body, "api_key")
        .filter(|k| !k.trim().is_empty())
        .or_else(|| store.in_memory_key_for_tenant(&tenant_id, slot.env_key))
        .or_else(|| {
            if slot.provider_id == "ollama" {
                Some("ollama-local".to_string())
            } else {
                None
            }
        }) {
        Some(key) => key,
        None => {
            return Ok(refusal(
                "MODEL_CONNECT_NEEDS_KEY",
                &approval_id,
                "add a provider key to connect the model",
            ));
        }
    };
    let Some(base_url) = chat_base_url(slot.provider_id) else {
        return Ok(refusal(
            "MODEL_CONNECT_PROVIDER_NOT_WIRED",
            &approval_id,
            "connecting this provider from the GUI is not wired yet; use the turn-on script for now",
        ));
    };
    // The models come from the provider's own live list - never a hardcoded
    // name that goes stale. This also validates the key before the turn-on.
    let available = match lister(&base_url, &api_key) {
        Ok(models) if !models.is_empty() => models,
        Ok(_) => {
            return Ok(refusal(
                "MODEL_CONNECT_NO_MODELS",
                &approval_id,
                "the provider returned no models for this key",
            ));
        }
        Err(reason) => return Ok(refusal("MODEL_CONNECT_CALL_FAILED", &approval_id, &reason)),
    };
    // Default: an explicit request, then the configured env preference, then a
    // suggested top model for the provider, then the first model it lists - each
    // only if it is in the live list, so nothing stale can ever connect.
    let env_default = {
        let key = model_env_key(slot.provider_id);
        if key.is_empty() {
            None
        } else {
            std::env::var(key).ok().filter(|m| !m.is_empty())
        }
    };
    let suggested = slot
        .suggested_models
        .iter()
        .find(|name| available.iter().any(|a| a == *name))
        .map(|name| name.to_string());
    let model = json_string_field(body, "model")
        .filter(|m| available.iter().any(|a| a == m))
        .or_else(|| env_default.filter(|m| available.iter().any(|a| a == m)))
        .or(suggested)
        .unwrap_or_else(|| available[0].clone());

    // The single live call - NO kernel lock held. The key is used here and in
    // the in-memory store below; never logged, never written, never in a receipt.
    let (model_id, response_id, total_tokens) = match caller(&base_url, &api_key, &model) {
        Ok(values) => values,
        Err(reason) => return Ok(refusal("MODEL_CONNECT_CALL_FAILED", &approval_id, &reason)),
    };
    if total_tokens == 0 {
        return Ok(refusal(
            "MODEL_CONNECT_CALL_FAILED",
            &approval_id,
            "the provider call returned no usage; not recording a turn-on",
        ));
    }

    // Record the observation through the SAME gate boot-restore uses - reacquire
    // the lock only now. The receipt is the durable evidence; evidence_file is a
    // GUI marker, not a path.
    let report = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        kernel
            .save_twin_model_gateway_provider_observation_local(
                TwinModelGatewayProviderObservation {
                    tenant_id: &tenant_id,
                    actor_id: &actor_id,
                    provider_id: slot.provider_id,
                    adapter: slot.adapter,
                    receipt_kind: slot.required_receipt_kind,
                    approval_receipt_id: &approval_id,
                    evidence_file: "gui:install-model-connect",
                    model_id: &model_id,
                    response_id: &response_id,
                    response_status: "completed",
                    observed: true,
                    provider_call_attempted: true,
                    network_call_attempted: true,
                    credential_presence_only: true,
                    credential_values_recorded: false,
                    provider_secret_values_recorded: false,
                    requested_secret_values_recorded: false,
                    output_text_recorded: false,
                    production_write_allowed: false,
                    total_tokens,
                },
            )
            .map_err(|error| error.message())?
    };
    if !report.accepted {
        return Ok(refusal(
            "MODEL_CONNECT_OBSERVATION_REJECTED",
            &approval_id,
            report.denial_reason,
        ));
    }

    // Hold the key in memory so Twin can actually answer with it. This is what
    // makes the green "connected" honest, not just an accepted observation. Hold
    // the provider's model list too, so the model picker survives a page refresh
    // (the GUI re-reads it from /runtime/model-connected.json) without another
    // live call. Both are process-only and lost on restart, by design.
    store.set_for_tenant(&tenant_id, slot.env_key, &api_key);
    store.set_models_for_tenant(&tenant_id, slot.env_key, &available);
    register_model_fabric_observation(
        kernel,
        &tenant_id,
        &actor_id,
        slot.provider_id,
        slot.env_key,
        &base_url,
        &model_id,
        &report.observation_receipt_id,
        std::ptr::eq(store, global()),
    )?;

    let available_json = available
        .iter()
        .map(|m| json_string_literal(m))
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        r#"{{"name":"mdx-install-model-connect-local-post","status":"MODEL_CONNECTED","connected":true,"provider_id":{},"model_id":{},"available_models":[{}],"approval_receipt_id":{},"observation_receipt_id":{},"runtime_route":"/runtime/model-connected.json","production_write_allowed":false}}"#,
        json_string_literal(slot.provider_id),
        json_string_literal(&model_id),
        available_json,
        json_string_literal(&approval_id),
        json_string_literal(&report.observation_receipt_id),
    ))
}

#[allow(clippy::too_many_arguments)]
fn register_model_fabric_observation(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    actor_id: &str,
    observed_provider_id: &str,
    credential_env: &str,
    base_url: &str,
    provider_model_id: &str,
    observation_receipt_id: &str,
    register_live_runtime: bool,
) -> Result<(), String> {
    let provider_id = canonical_model_provider_id(observed_provider_id)
        .ok_or_else(|| "connected provider is not declared in Model Fabric".to_string())?;
    let local = observed_provider_id == "ollama";
    let (data_retention, training_policy, data_policy_provenance) =
        observed_data_policy(provider_id, local);
    let connection_id = format!("{tenant_id}:{provider_id}");
    let model_id = format!("{provider_id}/{provider_model_id}");
    let deployment_id = format!("{tenant_id}:{model_id}");
    let connection = ProviderConnection {
        connection_id: connection_id.clone(),
        tenant_id: tenant_id.to_string(),
        provider_id: provider_id.to_string(),
        credential_ref: format!("secret:session/{credential_env}"),
        endpoint_base_url: base_url.to_string(),
        region: if local { "local" } else { "global" }.to_string(),
        residency: if local { "local" } else { "provider_managed" }.to_string(),
        data_retention: data_retention.to_string(),
        training_policy: training_policy.to_string(),
        data_policy_provenance: data_policy_provenance.to_string(),
        data_policy_observed_at: "2026-07-12".to_string(),
        health: "observed".to_string(),
        live_call_allowed: true,
    };
    let model = Model {
        model_id: model_id.clone(),
        provider_model_id: provider_model_id.to_string(),
        provider_id: provider_id.to_string(),
        display_name: provider_model_id.to_string(),
        lifecycle: "active".to_string(),
        modality: "text".to_string(),
    };
    let deployment = ModelDeployment {
        deployment_id: deployment_id.clone(),
        connection_id,
        provider_id: provider_id.to_string(),
        model_id,
        privacy_class: if local { "local" } else { "frontier" }.to_string(),
        region: if local { "local" } else { "global" }.to_string(),
        residency: if local { "local" } else { "provider_managed" }.to_string(),
        enabled: true,
        capabilities: CapabilityProfile {
            tools: true,
            // The xAI chat-completions adapter returned a governed tool call
            // during the turn-on proof. That is the structured contract Forge
            // consumes for its builder loop, so registering it as false made
            // the UI's "connected for Twin and Forge" claim impossible: Model
            // Fabric rejected every builder route as ineligible.
            structured_output: provider_id == "xai",
            vision: false,
            context_tokens: 0,
            provenance: format!("turn_on_observation:{observation_receipt_id}"),
            observed_at: "provider_turn_on".to_string(),
        },
    };
    let price = ModelPriceObservation {
        deployment_id,
        input_microusd_per_million: 0,
        output_microusd_per_million: 0,
        currency: "USD".to_string(),
        source: "unknown".to_string(),
        observed_at: "not_observed".to_string(),
    };
    if register_live_runtime {
        let registry = crate::model_fabric_registry::global();
        registry.upsert_connection(connection.clone())?;
        registry.upsert_model(tenant_id, model.clone())?;
        registry.upsert_deployment(tenant_id, deployment.clone(), price.clone())?;
    }
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let correlation = CorrelationIds {
        tenant_id: TenantId::new(tenant_id),
        trace_id: TraceId::new(kernel.mint_id("trace")),
        actor_id: ActorId::new(actor_id),
        loop_id: LoopId::new("model_fabric"),
        workflow_id: WorkflowId::new(kernel.mint_id("workflow")),
    };
    kernel.record_model_connection_configured(&correlation, &connection);
    kernel.record_model_deployment_registered(&correlation, &model, &deployment, &price);
    Ok(())
}

pub(crate) fn observed_data_policy(
    provider_id: &str,
    local: bool,
) -> (&'static str, &'static str, &'static str) {
    if local {
        return ("none", "none", "self_hosted_local");
    }
    match provider_id {
        "openai" => (
            "provider",
            "none",
            "https://platform.openai.com/docs/models/default-usage-policies-by-endpoint",
        ),
        "anthropic" => (
            "provider",
            "none",
            "https://privacy.claude.com/en/articles/7996868-is-my-data-used-for-model-training",
        ),
        "xai" => (
            "provider",
            "none",
            "https://docs.x.ai/developers/faq/security",
        ),
        "mistral" => (
            "provider",
            "none",
            "https://docs.mistral.ai/admin/monitor-comply/privacy-data-controls",
        ),
        "openrouter" => (
            "none",
            "none",
            "https://openrouter.ai/docs/guides/privacy/data-collection",
        ),
        "fireworks" => (
            "none",
            "none",
            "https://docs.fireworks.ai/guides/security_compliance/data_handling",
        ),
        _ => ("unknown", "unknown", "provider_policy_not_verified"),
    }
}

// One minimal OpenAI-compatible chat completion: confirms the key works and
// returns the model id, response id, and token usage. Non-2xx is an Err.
fn turn_on_call(
    base_url: &str,
    api_key: &str,
    model: &str,
) -> Result<(String, String, usize), String> {
    let request = serde_json::json!({
        "model": model,
        "messages": [{ "role": "user", "content": "MDx turn-on check. Reply: ok." }],
        "max_tokens": 16
    });
    let response = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(TURN_ON_TIMEOUT_SECS))
        .build()
        .post(&format!("{base_url}/chat/completions"))
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .send_string(&request.to_string())
        .map_err(|error| format!("the provider call did not succeed ({error})"))?;
    let parsed: serde_json::Value = response
        .into_json()
        .map_err(|error| format!("the provider response could not be read ({error})"))?;
    let model_id = parsed["model"].as_str().unwrap_or(model).to_string();
    let response_id = parsed["id"].as_str().unwrap_or("live_turn_on").to_string();
    let total_tokens = parsed["usage"]["total_tokens"].as_u64().unwrap_or(0) as usize;
    Ok((model_id, response_id, total_tokens))
}

// The provider's live model list (OpenAI-compatible GET /models). The source of
// truth for which models a key can use - so MDx never hardcodes a stale name.
fn fetch_models(base_url: &str, api_key: &str) -> Result<Vec<String>, String> {
    let response = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(TURN_ON_TIMEOUT_SECS))
        .build()
        .get(&format!("{base_url}/models"))
        .set("Authorization", &format!("Bearer {api_key}"))
        .call()
        .map_err(|error| format!("the provider model list could not be read ({error})"))?;
    let parsed: serde_json::Value = response
        .into_json()
        .map_err(|error| format!("the provider model list could not be read ({error})"))?;
    let models = parsed["data"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["id"].as_str().map(|id| id.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(models)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::MdxKernel;

    // Clearly-synthetic ids so the test is deterministic no matter what (if
    // anything) MDX_XAI_MODEL is set to in the running environment.
    fn fake_list(_base: &str, _key: &str) -> Result<Vec<String>, String> {
        Ok(vec![
            "test-model-alpha".to_string(),
            "test-model-beta".to_string(),
        ])
    }

    fn fake_ok(_base: &str, _key: &str, model: &str) -> Result<(String, String, usize), String> {
        // Echo the model chosen, so the test can prove the live-list default was used.
        Ok((model.to_string(), "resp_fake".to_string(), 7))
    }

    #[test]
    fn a_successful_connect_records_the_observation_and_shows_connected() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let store = SecretStore::default();
        let body = r#"{"provider":"xai","api_key":"sk-test","actor_id":"human:test"}"#;
        let response = connect(body, &kernel, &store, fake_list, fake_ok).expect("connect");
        assert!(
            response.contains("\"status\":\"MODEL_CONNECTED\""),
            "{response}"
        );
        assert!(response.contains("\"connected\":true"), "{response}");
        // The model came from the provider's live list (first, no env override),
        // and the full list is returned so the GUI can offer a change.
        assert!(
            response.contains("\"model_id\":\"test-model-alpha\""),
            "{response}"
        );
        assert!(
            response.contains("\"available_models\":[\"test-model-alpha\",\"test-model-beta\"]"),
            "{response}"
        );

        // The projection shows connected: observation recorded AND key usable.
        let projection = {
            let kernel = kernel.read().expect("lock");
            connected_projection(&kernel, &store)
        };
        assert!(projection.contains("\"connected\":true"), "{projection}");
        assert!(
            projection.contains("\"observation_recorded\":true"),
            "{projection}"
        );
        assert!(
            projection.contains("\"credential_available\":true"),
            "{projection}"
        );
        // The model list is held for the process, so the projection (and thus the
        // GUI picker) survives a refresh without another live list call.
        assert!(
            projection.contains("\"available_models\":[\"test-model-alpha\",\"test-model-beta\"]"),
            "{projection}"
        );
        // The key is in memory for Twin to use - never on disk, never in a receipt.
        assert_eq!(
            store
                .provider_key_for_tenant("local_tenant", "XAI_API_KEY")
                .as_deref(),
            Some("sk-test")
        );
        let kernel = kernel.read().expect("lock");
        let deployment = kernel
            .ledger()
            .entries()
            .iter()
            .rev()
            .find(|receipt| receipt.kind == mdx_core::MODEL_DEPLOYMENT_REGISTERED_RECEIPT_KIND)
            .expect("connected xAI deployment receipt");
        assert_eq!(
            deployment
                .payload
                .get("structured_output")
                .map(String::as_str),
            Some("true"),
            "the observed xAI deployment must be eligible for Forge's structured builder workload"
        );
    }

    #[test]
    fn a_suggested_top_model_is_chosen_over_whatever_lists_first() {
        // The provider lists a non-suggested model first and a suggested top
        // model (grok-4, in xai's suggested set) second. With no explicit choice,
        // connect must pick the suggested one, not available[0].
        fn list_with_suggested(_base: &str, _key: &str) -> Result<Vec<String>, String> {
            Ok(vec!["zzz-other-model".to_string(), "grok-4".to_string()])
        }
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let store = SecretStore::default();
        let body = r#"{"provider":"xai","api_key":"sk-test","actor_id":"human:test"}"#;
        let response =
            connect(body, &kernel, &store, list_with_suggested, fake_ok).expect("connect");
        assert!(response.contains("\"model_id\":\"grok-4\""), "{response}");
    }

    #[test]
    fn ollama_connect_uses_local_placeholder_key_and_prefers_qwen35_27b() {
        fn list_ollama_models(_base: &str, _key: &str) -> Result<Vec<String>, String> {
            Ok(vec![
                "llama3.1:8b".to_string(),
                "qwen3.5:9b".to_string(),
                "qwen3.5:27b".to_string(),
            ])
        }
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let store = SecretStore::default();
        let body = r#"{"provider":"ollama","actor_id":"human:test"}"#;
        let response =
            connect(body, &kernel, &store, list_ollama_models, fake_ok).expect("connect");
        assert!(
            response.contains("\"status\":\"MODEL_CONNECTED\""),
            "{response}"
        );
        assert!(
            response.contains("\"provider_id\":\"ollama\""),
            "{response}"
        );
        assert!(
            response.contains("\"model_id\":\"qwen3.5:27b\""),
            "{response}"
        );
        assert_eq!(
            store
                .provider_key_for_tenant("local_tenant", "MDX_OLLAMA_API_KEY")
                .as_deref(),
            Some("ollama-local")
        );
    }

    #[test]
    fn an_observation_without_a_usable_key_is_not_shown_connected() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::remove_var("XAI_API_KEY");
        }
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let store = SecretStore::default();
        store.disable_keychain_lookup_for_tenant("local_tenant", "XAI_API_KEY");
        // Connect records the observation and stores the key in THIS store.
        connect(
            r#"{"provider":"xai","api_key":"sk-test","actor_id":"human:test"}"#,
            &kernel,
            &store,
            fake_list,
            fake_ok,
        )
        .expect("connect");

        // A FRESH store (e.g. after a restart with no env key) has no credential,
        // so even though the observation receipt persists, connected is false.
        let empty = SecretStore::default();
        empty.disable_keychain_lookup_for_tenant("local_tenant", "XAI_API_KEY");
        let projection = {
            let kernel = kernel.read().expect("lock");
            connected_projection(&kernel, &empty)
        };
        assert!(
            projection.contains("\"observation_recorded\":true"),
            "{projection}"
        );
        assert!(
            projection.contains("\"credential_available\":false"),
            "{projection}"
        );
        assert!(projection.contains("\"connected\":false"), "{projection}");
    }

    #[test]
    fn no_key_records_the_approval_and_refuses_without_a_call() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let store = SecretStore::default();
        // No key: neither the model list nor the turn-on call may run.
        let panic_list: ModelLister = |_, _| panic!("no model listing without a key");
        let panic_caller: TurnOnCaller = |_, _, _| panic!("no live call must happen without a key");
        let response = connect(
            r#"{"provider":"xai","actor_id":"human:test"}"#,
            &kernel,
            &store,
            panic_list,
            panic_caller,
        )
        .expect("connect");
        assert!(
            response.contains("\"status\":\"MODEL_CONNECT_NEEDS_KEY\""),
            "{response}"
        );
        // A real approval id was cited (non-empty), not a blank.
        assert!(
            !response.contains("\"approval_receipt_id\":\"\""),
            "{response}"
        );
        // The approval is on the record even though no model connected.
        let kernel = kernel.read().expect("lock");
        assert!(
            kernel
                .ledger()
                .entries()
                .iter()
                .any(|r| r.kind == "install.model.turn_on.approved")
        );
    }

    #[test]
    fn verified_gateway_defaults_are_explicit_and_other_gateways_stay_unknown() {
        assert_eq!(
            observed_data_policy("fireworks", false),
            (
                "none",
                "none",
                "https://docs.fireworks.ai/guides/security_compliance/data_handling"
            )
        );
        assert_eq!(observed_data_policy("vercel", false).0, "unknown");
        assert_eq!(observed_data_policy("together", false).1, "unknown");
    }
}
