use crate::RouteResponse;
use crate::secret_store::{SecretStore, global};
use crate::studio::json_string_field;
use mdx_core::{
    MdxKernel, StorageProvider, canonical_model_provider_id, json_string_literal, model_workload,
};
use std::sync::{Arc, RwLock};

const ROUTE: &str = "/forge/model-providers.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EngineeringProfile {
    id: &'static str,
    label: &'static str,
    summary: &'static str,
    assignments: &'static [(&'static str, &'static str)],
}

const PREMIER_ASSIGNMENTS: [(&str, &str); 8] = [
    ("planner", "GPT"),
    ("conductor", "GPT"),
    ("executor", "GROK"),
    ("integrator", "GROK"),
    ("reviewer", "OPUS"),
    ("evaluator", "OPUS"),
    ("advisor", "OPUS"),
    ("voice", "CODEXMINI"),
];

const COST_EFFICIENT_ASSIGNMENTS: [(&str, &str); 8] = [
    ("planner", "GROK"),
    ("conductor", "GROK"),
    ("executor", "GROK"),
    ("integrator", "GROK"),
    ("reviewer", "GPT"),
    ("evaluator", "GPT"),
    ("advisor", "OPUS"),
    ("voice", "CODEXMINI"),
];

const SINGLE_MODEL_ASSIGNMENTS: [(&str, &str); 8] = [
    ("planner", "AUTO"),
    ("conductor", "AUTO"),
    ("executor", "AUTO"),
    ("integrator", "AUTO"),
    ("reviewer", "AUTO"),
    ("evaluator", "AUTO"),
    ("advisor", "AUTO"),
    ("voice", "AUTO"),
];

const ENGINEERING_PROFILES: [EngineeringProfile; 3] = [
    EngineeringProfile {
        id: "premier",
        label: "Premier frontier team",
        summary: "GPT-5.6 plans, Grok-4.5 builds, MDx verifies, and Opus 4.8 reviews with fresh eyes.",
        assignments: &PREMIER_ASSIGNMENTS,
    },
    EngineeringProfile {
        id: "cost_efficient",
        label: "Cost-efficient frontier",
        summary: "Grok-4.5 carries planning and execution, GPT-5.6 reviews, and Opus 4.8 stays available for hard judgment.",
        assignments: &COST_EFFICIENT_ASSIGNMENTS,
    },
    EngineeringProfile {
        id: "automatic",
        label: "Automatic",
        summary: "Forge casts every role from connected-model readiness and measured evidence as it accumulates.",
        assignments: &SINGLE_MODEL_ASSIGNMENTS,
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProviderFamily {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) env_key: &'static str,
    pub(crate) model_env_key: &'static str,
    pub(crate) default_chat_base_url: Option<&'static str>,
    pub(crate) native_adapter_status: &'static str,
}

const PROVIDERS: [ProviderFamily; 7] = [
    ProviderFamily {
        id: "openai",
        label: "OpenAI",
        env_key: "OPENAI_API_KEY",
        model_env_key: "MDX_OPENAI_MODEL",
        default_chat_base_url: Some("https://api.openai.com/v1"),
        native_adapter_status: "openai_compatible_ready",
    },
    ProviderFamily {
        id: "anthropic",
        label: "Anthropic",
        env_key: "ANTHROPIC_API_KEY",
        model_env_key: "MDX_ANTHROPIC_MODEL",
        default_chat_base_url: Some("https://api.anthropic.com/v1"),
        native_adapter_status: "native_messages_ready",
    },
    ProviderFamily {
        id: "xai",
        label: "xAI",
        env_key: "XAI_API_KEY",
        model_env_key: "MDX_XAI_MODEL",
        default_chat_base_url: Some("https://api.x.ai/v1"),
        native_adapter_status: "openai_compatible_ready",
    },
    ProviderFamily {
        id: "moonshot",
        label: "Kimi / Moonshot AI",
        env_key: "MOONSHOT_API_KEY",
        model_env_key: "MDX_MOONSHOT_MODEL",
        default_chat_base_url: Some("https://api.moonshot.ai/v1"),
        native_adapter_status: "openai_compatible_ready",
    },
    ProviderFamily {
        id: "gemini",
        label: "Gemini",
        env_key: "GEMINI_API_KEY",
        model_env_key: "MDX_GEMINI_MODEL",
        default_chat_base_url: None,
        native_adapter_status: "native_generate_content_adapter_pending_or_openai_compatible_base_required",
    },
    ProviderFamily {
        id: "bedrock",
        label: "AWS Bedrock",
        env_key: "AWS_BEDROCK_MODEL_ACCESS",
        model_env_key: "MDX_BEDROCK_MODEL",
        default_chat_base_url: None,
        native_adapter_status: "bedrock_runtime_adapter_pending_or_openai_compatible_base_required",
    },
    ProviderFamily {
        id: "ollama",
        label: "Ollama",
        env_key: "MDX_OLLAMA_API_KEY",
        model_env_key: "MDX_OLLAMA_MODEL",
        default_chat_base_url: Some("http://127.0.0.1:11434/v1"),
        native_adapter_status: "openai_compatible_ready",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NamedSlot {
    slot: &'static str,
    provider_family: &'static str,
    default_model_env_key: &'static str,
}

const NAMED_SLOTS: [NamedSlot; 11] = [
    NamedSlot {
        slot: "OPUS",
        provider_family: "anthropic",
        default_model_env_key: "MDX_ANTHROPIC_OPUS_MODEL",
    },
    NamedSlot {
        slot: "SONNET",
        provider_family: "anthropic",
        default_model_env_key: "MDX_ANTHROPIC_SONNET_MODEL",
    },
    NamedSlot {
        slot: "GPT",
        provider_family: "openai",
        default_model_env_key: "MDX_OPENAI_MODEL",
    },
    NamedSlot {
        slot: "CODEX",
        provider_family: "openai",
        default_model_env_key: "MDX_OPENAI_CODEX_MODEL",
    },
    NamedSlot {
        slot: "CODEXMINI",
        provider_family: "openai",
        default_model_env_key: "MDX_OPENAI_CODEX_MINI_MODEL",
    },
    NamedSlot {
        slot: "GROK",
        provider_family: "xai",
        default_model_env_key: "MDX_XAI_BUILD_MODEL",
    },
    NamedSlot {
        slot: "XAI",
        provider_family: "xai",
        default_model_env_key: "MDX_XAI_BUILD_MODEL",
    },
    NamedSlot {
        slot: "KIMI",
        provider_family: "moonshot",
        default_model_env_key: "MDX_MOONSHOT_MODEL",
    },
    NamedSlot {
        slot: "GEMINI",
        provider_family: "gemini",
        default_model_env_key: "MDX_GEMINI_MODEL",
    },
    NamedSlot {
        slot: "BEDROCK",
        provider_family: "bedrock",
        default_model_env_key: "MDX_BEDROCK_MODEL",
    },
    NamedSlot {
        slot: "LOCAL",
        provider_family: "ollama",
        default_model_env_key: "MDX_OLLAMA_MODEL",
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedModelEndpoint {
    pub(crate) role: String,
    pub(crate) slot: String,
    pub(crate) provider_family: String,
    pub(crate) base_url: String,
    pub(crate) api_key: String,
    pub(crate) model_id: String,
    pub(crate) privacy_class: String,
    pub(crate) source: String,
    pub(crate) openai_compatible: bool,
}

impl ResolvedModelEndpoint {
    pub(crate) fn public_json(&self) -> String {
        let model_class =
            crate::forge_run_stream::model_class(&self.provider_family, &self.slot, &self.model_id);
        let workload_id = forge_workload_for_role(&self.role);
        let canonical_provider_id = canonical_model_provider_id(&self.provider_family)
            .unwrap_or(self.provider_family.as_str());
        let preset = model_workload(workload_id)
            .map(|workload| workload.default_preset)
            .unwrap_or("balanced");
        format!(
            r#"{{"role":{},"workload_id":{},"route_preset":{},"slot":{},"provider_family":{},"canonical_provider_id":{},"base_url":{},"model_id":{},"model_class":{},"privacy_class":{},"source":{},"openai_compatible":{},"credential_available":{},"secret_value_exposed":false,"routing_grants_execution_authority":false}}"#,
            json_string_literal(&self.role),
            json_string_literal(workload_id),
            json_string_literal(preset),
            json_string_literal(&self.slot),
            json_string_literal(&self.provider_family),
            json_string_literal(canonical_provider_id),
            json_string_literal(&redacted_base_url(&self.base_url)),
            json_string_literal(&self.model_id),
            json_string_literal(model_class),
            json_string_literal(&self.privacy_class),
            json_string_literal(&self.source),
            self.openai_compatible,
            !self.api_key.trim().is_empty(),
        )
    }
}

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != ROUTE {
        return None;
    }
    Some(match method {
        "GET" => {
            let kernel = match kernel.read() {
                Ok(kernel) => kernel,
                Err(_) => return Some(Err("kernel lock poisoned".to_string())),
            };
            Ok(RouteResponse::json(
                "200 OK",
                provider_projection(&kernel, global()),
            ))
        }
        "POST" => {
            let kernel = match kernel.read() {
                Ok(kernel) => kernel,
                Err(_) => return Some(Err("kernel lock poisoned".to_string())),
            };
            Ok(RouteResponse::json(
                "200 OK",
                store_session_key(body, &kernel, global()),
            ))
        }
        _ => Ok(RouteResponse::text(
            "405 Method Not Allowed",
            "method not allowed\n".to_string(),
        )),
    })
}

pub(crate) fn provider_projection<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    store: &SecretStore,
) -> String {
    let tenant_id = current_tenant_id();
    let providers = PROVIDERS
        .iter()
        .map(|provider| provider_json(provider, &tenant_id, store))
        .collect::<Vec<_>>()
        .join(",");
    let roles = role_names()
        .iter()
        .map(|role| role_resolution_json(kernel, store, &tenant_id, role))
        .collect::<Vec<_>>()
        .join(",");
    let slots = NAMED_SLOTS
        .iter()
        .map(|slot| slot_json(slot, &tenant_id, store))
        .collect::<Vec<_>>()
        .join(",");
    let ready_provider_count = PROVIDERS
        .iter()
        .filter(|provider| store.has_provider_key_for_tenant(&tenant_id, provider.env_key))
        .count();
    let profiles = ENGINEERING_PROFILES
        .iter()
        .map(|profile| engineering_profile_json(profile, &tenant_id, store))
        .collect::<Vec<_>>()
        .join(",");
    let active_profile = active_engineering_profile(&tenant_id, store).unwrap_or("custom");
    format!(
        r#"{{"name":"mdx-forge-model-provider-routing","status":"OK","route":{},"writes_route":{},"read_only":false,"tenant_id":{},"provider_count":{},"ready_provider_count":{},"role_count":{},"providers":[{}],"roles":[{}],"slots":[{}],"engineering_stack":{{"recommended_profile_id":"premier","active_profile_id":{},"harness_selection_independent":true,"default_harness":"mdx_native","premier_harnesses":["mdx_native","codex_cli","grok_build"],"verification_authority":"mdx_deterministic_gates","profiles":[{}]}},"routing_policy_basis":"model_fabric_workload_alias_then_seeded_preferences_until_eval_scorecard_has_enough_samples","model_fabric_route":"/models/routes/resolve.json","eval_informed_routing_ready":false,"secret_values_recorded":false,"secret_values_exposed":false,"production_write_allowed":false,"authority_boundary":"Stores provider credentials and role choices without secret values in the tenant-scoped session store. Harness selection and model selection are independent. Forge roles map to stable Model Fabric workloads before configured endpoints are selected. Selection does not grant shell, patch, deployment, or provider authority beyond the existing Forge run gates."}}"#,
        json_string_literal(ROUTE),
        json_string_literal(ROUTE),
        json_string_literal(&tenant_id),
        PROVIDERS.len(),
        ready_provider_count,
        role_names().len(),
        providers,
        roles,
        slots,
        json_string_literal(active_profile),
        profiles,
    )
}

pub(crate) fn resolve_builder_endpoint<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    tenant_id: &str,
    requested_slot: &str,
) -> Option<ResolvedModelEndpoint> {
    resolve_role_endpoint(
        kernel,
        crate::secret_store::global(),
        tenant_id,
        "executor",
        requested_slot,
    )
}

pub(crate) fn resolve_role_endpoint<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    store: &SecretStore,
    tenant_id: &str,
    role: &str,
    requested_slot: &str,
) -> Option<ResolvedModelEndpoint> {
    resolve_role_env(role).or_else(|| {
        let requested = requested_slot.trim();
        if !requested.is_empty() {
            resolve_named_slot(kernel, store, tenant_id, requested, role)
        } else if let Some(slot) = store.role_slot_for_tenant(tenant_id, role) {
            resolve_named_slot(kernel, store, tenant_id, &slot, role)
        } else {
            role_preferences(role)
                .iter()
                .find_map(|slot| resolve_named_slot(kernel, store, tenant_id, slot, role))
        }
    })
}

pub(crate) fn resolve_role_endpoint_from_env(role: &str) -> Option<ResolvedModelEndpoint> {
    resolve_role_env(role).or_else(|| {
        role_preferences(role)
            .iter()
            .find_map(|slot| resolve_named_slot_from_env(slot, role))
    })
}

pub(crate) fn configured_slots_from_env_and_store<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    tenant_id: &str,
) -> Vec<String> {
    let mut seen_endpoint_identities = std::collections::BTreeSet::new();
    NAMED_SLOTS
        .iter()
        .filter_map(|slot| {
            let endpoint = resolve_named_slot(kernel, global(), tenant_id, slot.slot, "executor")?;
            let identity = format!(
                "{}\u{1f}{}\u{1f}{}",
                endpoint.provider_family, endpoint.base_url, endpoint.model_id
            );
            if seen_endpoint_identities.insert(identity) {
                Some(slot.slot.to_string())
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn endpoint_capabilities_json<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    tenant_id: &str,
) -> String {
    endpoint_capabilities_json_with_store(kernel, global(), tenant_id)
}

fn endpoint_capabilities_json_with_store<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    store: &SecretStore,
    tenant_id: &str,
) -> String {
    let mut seen_endpoint_identities = std::collections::BTreeSet::new();
    NAMED_SLOTS
        .iter()
        .filter_map(|slot| {
            let endpoint = resolve_named_slot(kernel, store, tenant_id, slot.slot, "executor")?;
            let identity = format!(
                "{}\u{1f}{}\u{1f}{}",
                endpoint.provider_family, endpoint.base_url, endpoint.model_id
            );
            if !seen_endpoint_identities.insert(identity) {
                return None;
            }
            Some(endpoint_capability_json(&endpoint))
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn endpoint_capability_json(endpoint: &ResolvedModelEndpoint) -> String {
    let provider = endpoint.provider_family.as_str();
    let coding_model = endpoint.openai_compatible || provider == "anthropic";
    let web_search = matches!(provider, "xai" | "openai" | "anthropic");
    let x_search = provider == "xai";
    let google_search = provider == "gemini";
    let local_private = provider == "ollama";
    let model_class =
        crate::forge_run_stream::model_class(provider, &endpoint.slot, &endpoint.model_id);
    format!(
        r#"{{"slot":{},"provider_family":{},"model_id":{},"model_class":{},"source":{},"privacy_class":{},"coding_model":{},"web_search":{},"x_search":{},"google_search":{},"local_private":{},"secret_value_exposed":false}}"#,
        json_string_literal(&endpoint.slot),
        json_string_literal(provider),
        json_string_literal(&endpoint.model_id),
        json_string_literal(model_class),
        json_string_literal(&endpoint.source),
        json_string_literal(&endpoint.privacy_class),
        coding_model,
        web_search,
        x_search,
        google_search,
        local_private,
    )
}

pub(crate) fn strongest_configured_execution_slot() -> String {
    for slot in [
        "OPUS",
        "GPT",
        "SONNET",
        "CODEX",
        "CODEXMINI",
        "KIMI",
        "GEMINI",
        "GROK",
        "XAI",
    ] {
        if resolve_named_slot_from_env(slot, "integrator").is_some() {
            return slot.to_string();
        }
    }
    String::new()
}

pub(crate) fn preferred_slot_for_role<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    tenant_id: &str,
    role: &str,
) -> String {
    resolve_role_endpoint(kernel, global(), tenant_id, role, "")
        .map(|endpoint| endpoint.slot)
        .filter(|slot| named_slot(slot).is_some())
        .unwrap_or_else(strongest_configured_execution_slot)
}

pub(crate) fn slot_privacy_class(slot: &str) -> &'static str {
    let slot = normalize_slot(slot);
    let var = if slot.is_empty() {
        "MDX_FLEET_BUILDER_PRIVACY_CLASS".to_string()
    } else {
        format!("MDX_FLEET_BUILDER_{slot}_PRIVACY_CLASS")
    };
    match std::env::var(&var).ok().as_deref().map(str::trim) {
        Some("sovereign") => "sovereign",
        _ => "frontier",
    }
}

pub(crate) fn sovereign_slot_ready(slot: &str) -> bool {
    slot_privacy_class(slot) == "sovereign"
        && resolve_named_slot_from_env(slot, "executor").is_some()
}

/// Resolve an endpoint allowed to see boundary-locked content (Twin,
/// Message, and Pages memory text is the most sensitive class). Only
/// endpoints that provably keep the data inside the boundary qualify: a
/// loopback base_url (a local model server such as Ollama) or a slot whose
/// sovereign class is READY (label plus its own complete config, so no
/// frontier fallback is possible). Anything else resolves to None and the
/// caller must skip the model stage entirely - fail closed, never frontier.
pub(crate) fn resolve_boundary_locked_endpoint<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    tenant_id: &str,
    role: &str,
) -> Option<ResolvedModelEndpoint> {
    let mut candidates: Vec<ResolvedModelEndpoint> = Vec::new();
    if let Some(endpoint) = resolve_role_env(role) {
        candidates.push(endpoint);
    }
    // The LOCAL slot (Ollama family) via env or a stored session key.
    if let Some(endpoint) = resolve_named_slot(kernel, global(), tenant_id, "LOCAL", role) {
        candidates.push(endpoint);
    }
    // Any named slot declared AND provably configured as sovereign.
    for slot in NAMED_SLOTS.iter() {
        if sovereign_slot_ready(slot.slot)
            && let Some(endpoint) = resolve_named_slot_from_env(slot.slot, role)
        {
            candidates.push(endpoint);
        }
    }
    candidates.into_iter().find(endpoint_is_boundary_locked)
}

fn endpoint_is_boundary_locked(endpoint: &ResolvedModelEndpoint) -> bool {
    base_url_is_loopback(&endpoint.base_url)
        || (endpoint.privacy_class == "sovereign" && sovereign_slot_ready(&endpoint.slot))
}

fn base_url_is_loopback(base_url: &str) -> bool {
    let after_scheme = match base_url.trim().split_once("://") {
        Some((_, rest)) => rest,
        None => base_url.trim(),
    };
    let authority = after_scheme.split('/').next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    let host = host
        .strip_prefix('[')
        .map(|inner| inner.split(']').next().unwrap_or(""))
        .unwrap_or_else(|| host.split(':').next().unwrap_or(""));
    matches!(host, "localhost" | "127.0.0.1" | "0.0.0.0" | "::1")
}

fn store_session_key<S: StorageProvider>(
    body: &str,
    kernel: &MdxKernel<S>,
    store: &SecretStore,
) -> String {
    let tenant_id = current_tenant_id();
    if let Some(profile_id) = json_string_field(body, "profile")
        .or_else(|| json_string_field(body, "profile_id"))
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    {
        let Some(profile) = ENGINEERING_PROFILES
            .iter()
            .find(|profile| profile.id == profile_id)
        else {
            return format!(
                r#"{{"name":"mdx-forge-engineering-profile","status":"UNKNOWN_PROFILE","profile_id":{},"secret_values_recorded":false,"secret_values_exposed":false,"production_write_allowed":false}}"#,
                json_string_literal(&profile_id),
            );
        };
        for (role, slot) in profile.assignments {
            store.set_role_slot_for_tenant(&tenant_id, role, slot);
        }
        return format!(
            r#"{{"name":"mdx-forge-engineering-profile","status":"ENGINEERING_PROFILE_STORED","profile_id":{},"assignment_count":{},"projection_route":{},"secret_values_recorded":false,"secret_values_exposed":false,"production_write_allowed":false}}"#,
            json_string_literal(profile.id),
            profile.assignments.len(),
            json_string_literal(ROUTE),
        );
    }
    if let (Some(role), Some(slot)) = (
        json_string_field(body, "role").filter(|value| !value.trim().is_empty()),
        json_string_field(body, "slot").filter(|value| !value.trim().is_empty()),
    ) {
        let normalized_slot = normalize_slot(&slot);
        if normalized_slot != "AUTO" && named_slot(&normalized_slot).is_none() {
            return format!(
                r#"{{"name":"mdx-forge-model-role-routing","status":"UNKNOWN_SLOT","role":{},"slot":{},"credential_available":false,"secret_values_recorded":false,"secret_values_exposed":false,"production_write_allowed":false}}"#,
                json_string_literal(&role),
                json_string_literal(&normalized_slot),
            );
        }
        store.set_role_slot_for_tenant(&tenant_id, &role, &normalized_slot);
        return format!(
            r#"{{"name":"mdx-forge-model-role-routing","status":"ROLE_SLOT_STORED","role":{},"slot":{},"credential_available":false,"secret_values_recorded":false,"secret_values_exposed":false,"production_write_allowed":false,"projection_route":{}}}"#,
            json_string_literal(&role),
            json_string_literal(&normalized_slot),
            json_string_literal(ROUTE),
        );
    }

    let provider_id = json_string_field(body, "provider")
        .or_else(|| json_string_field(body, "provider_id"))
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "xai".to_string());
    let Some(provider) = provider_family(&provider_id) else {
        return format!(
            r#"{{"name":"mdx-forge-model-provider-session-key","status":"UNKNOWN_PROVIDER","provider_family":{},"credential_available":false,"secret_values_recorded":false,"secret_values_exposed":false,"production_write_allowed":false}}"#,
            json_string_literal(&provider_id),
        );
    };
    let action = json_string_field(body, "action")
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(action.as_str(), "remove" | "disconnect" | "clear") {
        return remove_session_key(kernel, store, &tenant_id, provider);
    }
    let Some(api_key) = json_string_field(body, "api_key").filter(|value| !value.trim().is_empty())
    else {
        return format!(
            r#"{{"name":"mdx-forge-model-provider-session-key","status":"KEY_REQUIRED","provider_family":{},"env_key":{},"credential_available":false,"secret_values_recorded":false,"secret_values_exposed":false,"production_write_allowed":false}}"#,
            json_string_literal(provider.id),
            json_string_literal(provider.env_key),
        );
    };
    store.set_for_tenant(&tenant_id, provider.env_key, &api_key);
    let remember_on_machine = json_bool_field(body, "remember_on_machine")
        || json_bool_field(body, "persist_on_machine")
        || json_bool_field(body, "save_to_keychain");
    let mut credential_scope = "process_memory_only";
    let mut persistence_status = "not_requested";
    let mut persistence_reason = String::new();
    if remember_on_machine {
        match store.persist_for_tenant(&tenant_id, provider.env_key, &api_key) {
            Ok(scope) => {
                credential_scope = scope;
                persistence_status = "stored";
            }
            Err(reason) => {
                persistence_status = "refused";
                persistence_reason = reason;
            }
        }
    }
    if let Some(model) = json_string_field(body, "model_id")
        .or_else(|| json_string_field(body, "model"))
        .filter(|value| !value.trim().is_empty())
    {
        store.set_models_for_tenant(&tenant_id, provider.env_key, &[model]);
    }
    format!(
        r#"{{"name":"mdx-forge-model-provider-session-key","status":"SESSION_KEY_STORED","provider_family":{},"env_key":{},"credential_available":true,"credential_scope":{},"persistence":{{"requested":{},"status":{},"reason":{}}},"secret_values_recorded":false,"secret_values_exposed":false,"provider_call_performed":false,"production_write_allowed":false,"projection_route":{}}}"#,
        json_string_literal(provider.id),
        json_string_literal(provider.env_key),
        json_string_literal(credential_scope),
        remember_on_machine,
        json_string_literal(persistence_status),
        json_string_literal(&persistence_reason),
        json_string_literal(ROUTE),
    )
}

fn remove_session_key<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    store: &SecretStore,
    tenant_id: &str,
    provider: &ProviderFamily,
) -> String {
    let removed_session_key = store.remove_for_tenant(tenant_id, provider.env_key);
    let removed_role_slot_count = store.remove_role_slots_for_tenant_matching(tenant_id, |slot| {
        named_slot(slot)
            .map(|named| named.provider_family == provider.id)
            .unwrap_or(false)
    });
    let credential_source = credential_source_for_provider(store, tenant_id, provider);
    let credential_available = credential_source != "none";
    let recast_roles = role_names()
        .iter()
        .map(|role| role_resolution_json(kernel, store, tenant_id, role))
        .collect::<Vec<_>>()
        .join(",");
    let status = if removed_session_key || removed_role_slot_count > 0 {
        "SESSION_KEY_REMOVED"
    } else {
        "NOTHING_TO_REMOVE"
    };
    format!(
        r#"{{"name":"mdx-forge-model-provider-session-key","status":{},"provider_family":{},"env_key":{},"credential_available":{},"credential_source":{},"removed_role_slot_count":{},"roles":[{}],"projection_route":{},"secret_values_recorded":false,"secret_values_exposed":false,"production_write_allowed":false}}"#,
        json_string_literal(status),
        json_string_literal(provider.id),
        json_string_literal(provider.env_key),
        credential_available,
        json_string_literal(credential_source),
        removed_role_slot_count,
        recast_roles,
        json_string_literal(ROUTE),
    )
}

fn role_resolution_json<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    store: &SecretStore,
    tenant_id: &str,
    role: &str,
) -> String {
    // Expose the explicitly stored slot (empty when unassigned) so a surface can
    // tell an operator-assigned role from one resolving to the strongest
    // available fallback. This matters for the advisor role (ADR 0490), whose
    // "auto = strongest configured" state must read differently from a named
    // assignment.
    let assigned_slot = store
        .role_slot_for_tenant(tenant_id, role)
        .map(|slot| normalize_slot(&slot))
        .filter(|slot| !slot.is_empty());
    let assigned_field = format!(
        r#""assigned_slot":{}"#,
        json_string_literal(assigned_slot.as_deref().unwrap_or("")),
    );
    match resolve_role_endpoint(kernel, store, tenant_id, role, "") {
        Some(endpoint) => {
            let base = endpoint.public_json();
            match base.strip_suffix('}') {
                Some(open) => format!("{open},{assigned_field}}}"),
                None => base,
            }
        }
        None => format!(
            r#"{{"role":{},"status":"not_configured","credential_available":false,"secret_value_exposed":false,"preferred_slots":[{}],"recommendation_basis":"seeded_preference_order_pending_eval_scorecard","authority_granted":false,{}}}"#,
            json_string_literal(role),
            role_preferences(role)
                .iter()
                .map(|slot| json_string_literal(slot))
                .collect::<Vec<_>>()
                .join(","),
            assigned_field,
        ),
    }
}

fn provider_json(provider: &ProviderFamily, tenant_id: &str, store: &SecretStore) -> String {
    let credential_available = store.has_provider_key_for_tenant(tenant_id, provider.env_key);
    let model_hint = provider_model_hint(provider, tenant_id, store).unwrap_or_default();
    let credential_source = credential_source_for_provider(store, tenant_id, provider);
    format!(
        r#"{{"provider_family":{},"label":{},"env_key":{},"credential_available":{},"credential_source":{},"session_key_available":{},"model_hint":{},"default_chat_base_url":{},"native_adapter_status":{},"secret_value_exposed":false}}"#,
        json_string_literal(provider.id),
        json_string_literal(provider.label),
        json_string_literal(provider.env_key),
        credential_available,
        json_string_literal(credential_source),
        store
            .in_memory_key_for_tenant(tenant_id, provider.env_key)
            .is_some(),
        json_string_literal(&model_hint),
        provider
            .default_chat_base_url
            .map(json_string_literal)
            .unwrap_or_else(|| "null".to_string()),
        json_string_literal(provider.native_adapter_status),
    )
}

fn engineering_profile_json(
    profile: &EngineeringProfile,
    tenant_id: &str,
    store: &SecretStore,
) -> String {
    let assignments = profile
        .assignments
        .iter()
        .map(|(role, slot)| {
            format!(
                r#"{{"role":{},"slot":{}}}"#,
                json_string_literal(role),
                json_string_literal(slot),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let required_slots = profile
        .assignments
        .iter()
        .map(|(_, slot)| *slot)
        .filter(|slot| *slot != "AUTO")
        .collect::<std::collections::BTreeSet<_>>();
    let ready_slots = required_slots
        .iter()
        .filter(|slot| {
            named_slot(slot)
                .and_then(|named| provider_family(named.provider_family))
                .map(|provider| store.has_provider_key_for_tenant(tenant_id, provider.env_key))
                .unwrap_or(false)
        })
        .count();
    format!(
        r#"{{"profile_id":{},"label":{},"summary":{},"assignments":[{}],"required_slot_count":{},"ready_slot_count":{},"ready":{}}}"#,
        json_string_literal(profile.id),
        json_string_literal(profile.label),
        json_string_literal(profile.summary),
        assignments,
        required_slots.len(),
        ready_slots,
        ready_slots == required_slots.len(),
    )
}

fn active_engineering_profile(tenant_id: &str, store: &SecretStore) -> Option<&'static str> {
    ENGINEERING_PROFILES.iter().find_map(|profile| {
        let matches = profile.assignments.iter().all(|(role, slot)| {
            let assigned = store
                .role_slot_for_tenant(tenant_id, role)
                .unwrap_or_else(|| "AUTO".to_string());
            assigned == *slot
        });
        matches.then_some(profile.id)
    })
}

fn credential_source_for_provider(
    store: &SecretStore,
    tenant_id: &str,
    provider: &ProviderFamily,
) -> &'static str {
    store.provider_key_source_for_tenant(tenant_id, provider.env_key)
}

fn slot_json(slot: &NamedSlot, tenant_id: &str, store: &SecretStore) -> String {
    let full_env_ready = named_slot_env_endpoint(slot.slot, "executor").is_some();
    let provider = provider_family(slot.provider_family);
    let provider_key_ready = provider
        .map(|provider| store.has_provider_key_for_tenant(tenant_id, provider.env_key))
        .unwrap_or(false);
    format!(
        r#"{{"slot":{},"provider_family":{},"full_env_endpoint_ready":{},"provider_key_ready":{},"privacy_class":{},"default_model_env_key":{},"secret_value_exposed":false}}"#,
        json_string_literal(slot.slot),
        json_string_literal(slot.provider_family),
        full_env_ready,
        provider_key_ready,
        json_string_literal(slot_privacy_class(slot.slot)),
        json_string_literal(slot.default_model_env_key),
    )
}

fn resolve_role_env(role: &str) -> Option<ResolvedModelEndpoint> {
    let upper = normalize_role(role);
    let prefix = format!("MDX_FLEET_{upper}");
    let base_url = env_trimmed(&format!("{prefix}_BASE_URL"))?;
    let api_key = env_trimmed(&format!("{prefix}_API_KEY"))?;
    let model_id = env_trimmed(&format!("{prefix}_MODEL"))?;
    let provider_family = env_trimmed(&format!("{prefix}_PROVIDER"))
        .unwrap_or_else(|| infer_provider_family(&model_id, &base_url).to_string());
    Some(ResolvedModelEndpoint {
        role: role.to_string(),
        slot: upper,
        provider_family,
        base_url,
        api_key,
        model_id,
        privacy_class: "frontier".to_string(),
        source: "role_env".to_string(),
        openai_compatible: true,
    })
}

fn resolve_named_slot<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    store: &SecretStore,
    tenant_id: &str,
    slot: &str,
    role: &str,
) -> Option<ResolvedModelEndpoint> {
    resolve_named_slot_from_env(slot, role)
        .or_else(|| resolve_named_slot_from_provider_key(kernel, store, tenant_id, slot, role))
}

fn resolve_named_slot_from_env(slot: &str, role: &str) -> Option<ResolvedModelEndpoint> {
    let slot = normalize_slot(slot);
    let endpoint = named_slot_env_endpoint(&slot, role)?;
    Some(endpoint)
}

fn named_slot_env_endpoint(slot: &str, role: &str) -> Option<ResolvedModelEndpoint> {
    let slot = normalize_slot(slot);
    let prefix = if slot.is_empty() {
        "MDX_FLEET_BUILDER".to_string()
    } else {
        format!("MDX_FLEET_BUILDER_{slot}")
    };
    let base_url = env_trimmed(&format!("{prefix}_BASE_URL"))?;
    let api_key = env_trimmed(&format!("{prefix}_API_KEY"))?;
    let model_id = env_trimmed(&format!("{prefix}_MODEL"))?;
    let privacy_class = slot_privacy_class(&slot).to_string();
    Some(ResolvedModelEndpoint {
        role: role.to_string(),
        slot,
        provider_family: infer_provider_family(&model_id, &base_url).to_string(),
        base_url,
        api_key,
        model_id,
        privacy_class,
        source: "slot_env".to_string(),
        openai_compatible: true,
    })
}

fn resolve_named_slot_from_provider_key<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    store: &SecretStore,
    tenant_id: &str,
    slot: &str,
    role: &str,
) -> Option<ResolvedModelEndpoint> {
    let named = named_slot(slot)?;
    let provider = provider_family(named.provider_family)?;
    let base_url = provider.default_chat_base_url?.to_string();
    let api_key = store.provider_key_for_tenant(tenant_id, provider.env_key)?;
    let model_id = env_trimmed(named.default_model_env_key)
        .or_else(|| env_trimmed(provider.model_env_key))
        .or_else(|| accepted_model_for_provider(kernel, tenant_id, provider.id))
        .or_else(|| {
            store
                .models_for_tenant(tenant_id, provider.env_key)
                .into_iter()
                .next()
        })
        .or_else(|| seeded_default_model_for_slot(named.slot).map(str::to_string))?;
    Some(ResolvedModelEndpoint {
        role: role.to_string(),
        slot: named.slot.to_string(),
        provider_family: provider.id.to_string(),
        base_url,
        api_key,
        model_id,
        privacy_class: slot_privacy_class(named.slot).to_string(),
        source: "provider_session_or_env_key".to_string(),
        openai_compatible: provider.native_adapter_status == "openai_compatible_ready",
    })
}

fn accepted_model_for_provider<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    tenant_id: &str,
    provider_id: &str,
) -> Option<String> {
    crate::twin_model_gateway::accepted_provider_observations_for_tenant(kernel, tenant_id)
        .into_iter()
        .rev()
        .find(|receipt| receipt.payload.get("provider_id").map(String::as_str) == Some(provider_id))
        .and_then(|receipt| receipt.payload.get("model_id").cloned())
        .filter(|model| !model.trim().is_empty())
}

fn provider_model_hint(
    provider: &ProviderFamily,
    tenant_id: &str,
    store: &SecretStore,
) -> Option<String> {
    env_trimmed(provider.model_env_key)
        .or_else(|| {
            store
                .models_for_tenant(tenant_id, provider.env_key)
                .into_iter()
                .next()
        })
        .or_else(|| seeded_default_model_for_provider(provider.id).map(str::to_string))
}

fn seeded_default_model_for_slot(slot: &str) -> Option<&'static str> {
    match normalize_slot(slot).as_str() {
        "OPUS" => Some("claude-opus-4-8"),
        "SONNET" => Some("claude-sonnet-4-6"),
        "GPT" | "CODEX" => Some("gpt-5.6-sol"),
        "CODEXMINI" => Some("gpt-5-mini"),
        "GROK" | "XAI" => Some("grok-4.5"),
        "KIMI" => Some("kimi-k3"),
        "GEMINI" => Some("gemini-2.5-pro"),
        "BEDROCK" | "LOCAL" => None,
        _ => None,
    }
}

fn seeded_default_model_for_provider(provider_id: &str) -> Option<&'static str> {
    match provider_id {
        "anthropic" => Some("claude-opus-4-8"),
        "openai" => Some("gpt-5.6-sol"),
        "xai" => Some("grok-4.5"),
        "moonshot" => Some("kimi-k3"),
        "gemini" => Some("gemini-2.5-pro"),
        _ => None,
    }
}

fn role_names() -> &'static [&'static str] {
    &[
        "planner",
        "executor",
        "conductor",
        "integrator",
        "reviewer",
        "evaluator",
        "voice",
        "advisor",
    ]
}

pub(crate) fn forge_workload_for_role(role: &str) -> &'static str {
    match normalize_role(role).as_str() {
        "PLANNER" | "CONDUCTOR" => "mdx/forge/planner",
        "REVIEWER" => "mdx/forge/reviewer",
        "EVALUATOR" | "JUDGE" => "mdx/forge/evaluator",
        "ADVISOR" => "mdx/forge/advisor",
        "VOICE" => "mdx/forge/voice",
        "EXECUTOR" | "BUILDER" | "INTEGRATOR" => "mdx/forge/builder",
        _ => "mdx/forge/builder",
    }
}

fn role_preferences(role: &str) -> &'static [&'static str] {
    match normalize_role(role).as_str() {
        "EXECUTOR" | "BUILDER" => &[
            "GROK", "XAI", "GPT", "CODEX", "KIMI", "SONNET", "GEMINI", "LOCAL",
        ],
        "REVIEWER" | "EVALUATOR" | "JUDGE" => &[
            "OPUS",
            "GPT",
            "CODEX",
            "CODEXMINI",
            "KIMI",
            "GEMINI",
            "SONNET",
            "GROK",
            "XAI",
        ],
        "PLANNER" | "CONDUCTOR" => &[
            "GPT",
            "OPUS",
            "SONNET",
            "CODEX",
            "CODEXMINI",
            "KIMI",
            "GEMINI",
            "GROK",
            "XAI",
        ],
        "INTEGRATOR" => &[
            "GROK", "XAI", "GPT", "CODEX", "KIMI", "OPUS", "SONNET", "GEMINI",
        ],
        "VOICE" => &[
            "CODEXMINI",
            "GPT",
            "SONNET",
            "OPUS",
            "KIMI",
            "GEMINI",
            "GROK",
            "XAI",
        ],
        // The advisor is the premium judgment role (ADR 0490). It is
        // operator-assigned and provider-agnostic; when unassigned it falls back
        // to the strongest configured model. The preference order mirrors
        // strongest_configured_execution_slot so an unassigned advisor lands on
        // the same "strongest available" model, never a hard-coded vendor.
        "ADVISOR" => &[
            "OPUS",
            "GPT",
            "SONNET",
            "CODEX",
            "CODEXMINI",
            "KIMI",
            "GEMINI",
            "GROK",
            "XAI",
            "LOCAL",
        ],
        _ => &["GROK", "XAI", "GPT", "KIMI", "SONNET", "GEMINI"],
    }
}

fn named_slot(slot: &str) -> Option<NamedSlot> {
    let slot = normalize_slot(slot);
    NAMED_SLOTS.iter().copied().find(|item| item.slot == slot)
}

fn provider_family(provider: &str) -> Option<&'static ProviderFamily> {
    PROVIDERS.iter().find(|item| item.id == provider)
}

pub(crate) fn infer_provider_family(model_id: &str, base_url: &str) -> &'static str {
    let haystack = format!("{model_id} {base_url}").to_ascii_lowercase();
    if haystack.contains("moonshot") || haystack.contains("kimi") {
        "moonshot"
    } else if haystack.contains("anthropic")
        || haystack.contains("claude")
        || haystack.contains("opus")
        || haystack.contains("sonnet")
    {
        "anthropic"
    } else if haystack.contains("openai") || haystack.contains("gpt") {
        "openai"
    } else if haystack.contains("gemini") || haystack.contains("google") {
        "gemini"
    } else if haystack.contains("bedrock") || haystack.contains("aws") {
        "bedrock"
    } else if haystack.contains("ollama")
        || haystack.contains("localhost")
        || haystack.contains("127.0.0.1")
    {
        "ollama"
    } else {
        "xai"
    }
}

fn current_tenant_id() -> String {
    crate::request_security::current_verified_identity()
        .map(|identity| identity.tenant_id)
        .unwrap_or_else(|| "local_tenant".to_string())
}

fn env_trimmed(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn json_bool_field(body: &str, key: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value.get(key).cloned())
        .and_then(|value| match value {
            serde_json::Value::Bool(value) => Some(value),
            serde_json::Value::String(value) => {
                let lowered = value.trim().to_ascii_lowercase();
                Some(matches!(lowered.as_str(), "1" | "true" | "yes" | "on"))
            }
            _ => None,
        })
        .unwrap_or(false)
}

fn normalize_role(role: &str) -> String {
    role.trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn normalize_slot(slot: &str) -> String {
    slot.trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn redacted_base_url(base_url: &str) -> String {
    base_url
        .split('?')
        .next()
        .unwrap_or(base_url)
        .trim_end_matches('/')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forge_roles_use_stable_model_fabric_workloads() {
        assert_eq!(forge_workload_for_role("planner"), "mdx/forge/planner");
        assert_eq!(forge_workload_for_role("executor"), "mdx/forge/builder");
        assert_eq!(forge_workload_for_role("reviewer"), "mdx/forge/reviewer");
        assert_eq!(forge_workload_for_role("evaluator"), "mdx/forge/evaluator");
        assert_eq!(forge_workload_for_role("advisor"), "mdx/forge/advisor");
        assert_eq!(forge_workload_for_role("voice"), "mdx/forge/voice");
    }
    use mdx_core::MdxKernel;

    #[test]
    fn session_key_route_records_presence_only_and_resolves_executor() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::remove_var("XAI_API_KEY");
            std::env::remove_var("MDX_XAI_BUILD_MODEL");
            std::env::set_var("MDX_XAI_MODEL", "grok-test");
        }
        let kernel = MdxKernel::boot_local();
        let store = isolated_store();
        let response = store_session_key(
            r#"{"provider":"xai","api_key":"xai-session-key","model_id":"grok-test"}"#,
            &kernel,
            &store,
        );
        assert!(
            response.contains("\"status\":\"SESSION_KEY_STORED\""),
            "{response}"
        );
        assert!(
            !response.contains("xai-session-key"),
            "secret value leaked: {response}"
        );
        let endpoint = resolve_role_endpoint(&kernel, &store, "local_tenant", "executor", "GROK")
            .expect("executor endpoint");
        assert_eq!(endpoint.provider_family, "xai");
        assert_eq!(endpoint.model_id, "grok-test");
        assert_eq!(endpoint.base_url, "https://api.x.ai/v1");
        unsafe {
            std::env::remove_var("MDX_XAI_MODEL");
        }
    }

    #[test]
    fn role_env_overrides_provider_preferences_for_conductor() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            for key in [
                "MDX_FLEET_CONDUCTOR_BASE_URL",
                "MDX_FLEET_CONDUCTOR_API_KEY",
                "MDX_FLEET_CONDUCTOR_MODEL",
                "MDX_FLEET_CONDUCTOR_PROVIDER",
            ] {
                std::env::remove_var(key);
            }
            std::env::set_var("MDX_FLEET_CONDUCTOR_BASE_URL", "https://api.openai.test/v1");
            std::env::set_var("MDX_FLEET_CONDUCTOR_API_KEY", "openai-test");
            std::env::set_var("MDX_FLEET_CONDUCTOR_MODEL", "gpt-orchestrator-test");
            std::env::set_var("MDX_FLEET_CONDUCTOR_PROVIDER", "openai");
        }
        let endpoint = resolve_role_endpoint_from_env("conductor").expect("role env endpoint");
        assert_eq!(endpoint.source, "role_env");
        assert_eq!(endpoint.provider_family, "openai");
        assert_eq!(endpoint.model_id, "gpt-orchestrator-test");
        unsafe {
            for key in [
                "MDX_FLEET_CONDUCTOR_BASE_URL",
                "MDX_FLEET_CONDUCTOR_API_KEY",
                "MDX_FLEET_CONDUCTOR_MODEL",
                "MDX_FLEET_CONDUCTOR_PROVIDER",
            ] {
                std::env::remove_var(key);
            }
        }
    }

    #[test]
    fn anthropic_key_resolves_to_native_messages_endpoint() {
        let kernel = MdxKernel::boot_local();
        let store = isolated_store();
        let response = store_session_key(
            r#"{"provider":"anthropic","api_key":"anthropic-session-key","model_id":"claude-opus-test"}"#,
            &kernel,
            &store,
        );
        assert!(
            response.contains("\"status\":\"SESSION_KEY_STORED\""),
            "{response}"
        );
        let projection = provider_projection(&kernel, &store);
        assert!(
            projection.contains("\"provider_family\":\"anthropic\""),
            "{projection}"
        );
        assert!(
            projection.contains("\"native_adapter_status\":\"native_messages_ready\""),
            "{projection}"
        );
        let endpoint = resolve_role_endpoint(&kernel, &store, "local_tenant", "integrator", "OPUS")
            .expect("native anthropic endpoint");
        assert_eq!(endpoint.provider_family, "anthropic");
        assert_eq!(endpoint.base_url, "https://api.anthropic.com/v1");
        assert_eq!(endpoint.model_id, "claude-opus-test");
        assert!(!endpoint.openai_compatible);
    }

    #[test]
    fn moonshot_key_resolves_kimi_k3_without_coupling_the_harness() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::remove_var("MOONSHOT_API_KEY");
            std::env::remove_var("MDX_MOONSHOT_MODEL");
        }
        let kernel = MdxKernel::boot_local();
        let store = isolated_store();
        let response = store_session_key(
            r#"{"provider":"moonshot","api_key":"moonshot-session-key"}"#,
            &kernel,
            &store,
        );
        assert!(response.contains("\"status\":\"SESSION_KEY_STORED\""));
        assert!(!response.contains("moonshot-session-key"));

        let endpoint = resolve_role_endpoint(&kernel, &store, "local_tenant", "executor", "KIMI")
            .expect("Kimi endpoint");
        assert_eq!(endpoint.provider_family, "moonshot");
        assert_eq!(endpoint.base_url, "https://api.moonshot.ai/v1");
        assert_eq!(endpoint.model_id, "kimi-k3");
        assert!(endpoint.openai_compatible);
        assert_eq!(
            infer_provider_family("kimi-k3", endpoint.base_url.as_str()),
            "moonshot"
        );

        let projection = provider_projection(&kernel, &store);
        assert!(projection.contains("\"provider_family\":\"moonshot\""));
        assert!(projection.contains("\"model_hint\":\"kimi-k3\""));
        assert!(projection.contains("\"harness_selection_independent\":true"));
    }

    #[test]
    fn provider_key_without_model_id_uses_seeded_defaults() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            for key in [
                "ANTHROPIC_API_KEY",
                "OPENAI_API_KEY",
                "MDX_ANTHROPIC_MODEL",
                "MDX_ANTHROPIC_OPUS_MODEL",
                "MDX_ANTHROPIC_SONNET_MODEL",
                "MDX_OPENAI_MODEL",
                "MDX_OPENAI_CODEX_MODEL",
                "MDX_OPENAI_CODEX_MINI_MODEL",
            ] {
                std::env::remove_var(key);
            }
        }
        let kernel = MdxKernel::boot_local();
        let store = isolated_store();
        store_session_key(
            r#"{"provider":"anthropic","api_key":"anthropic-session-key"}"#,
            &kernel,
            &store,
        );
        store_session_key(
            r#"{"provider":"openai","api_key":"openai-session-key"}"#,
            &kernel,
            &store,
        );

        let reviewer = resolve_role_endpoint(&kernel, &store, "local_tenant", "reviewer", "")
            .expect("reviewer endpoint");
        assert_eq!(reviewer.slot, "OPUS");
        assert_eq!(reviewer.provider_family, "anthropic");
        assert_eq!(reviewer.model_id, "claude-opus-4-8");
        assert!(!reviewer.openai_compatible);

        let planner = resolve_role_endpoint(&kernel, &store, "local_tenant", "planner", "")
            .expect("planner endpoint");
        assert_eq!(planner.slot, "GPT");
        assert_eq!(planner.provider_family, "openai");
        assert_eq!(planner.model_id, "gpt-5.6-sol");
        assert!(planner.openai_compatible);

        let executor = resolve_role_endpoint(&kernel, &store, "local_tenant", "executor", "")
            .expect("executor endpoint");
        assert_eq!(executor.slot, "GPT");
        assert_eq!(executor.provider_family, "openai");
        assert_eq!(executor.model_id, "gpt-5.6-sol");

        let projection = provider_projection(&kernel, &store);
        assert!(
            projection.contains("\"ready_provider_count\":2"),
            "{projection}"
        );
        assert!(
            projection.contains("\"model_hint\":\"claude-opus-4-8\""),
            "{projection}"
        );
        assert!(
            projection.contains("\"model_hint\":\"gpt-5.6-sol\""),
            "{projection}"
        );
    }

    #[test]
    fn premier_profile_assigns_frontier_roles_without_coupling_harness_choice() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .expect("env test lock");
        let kernel = MdxKernel::boot_local();
        let store = isolated_store();

        let response = store_session_key(r#"{"profile":"premier"}"#, &kernel, &store);
        assert!(
            response.contains("ENGINEERING_PROFILE_STORED"),
            "{response}"
        );
        assert_eq!(
            store
                .role_slot_for_tenant("local_tenant", "planner")
                .as_deref(),
            Some("GPT")
        );
        assert_eq!(
            store
                .role_slot_for_tenant("local_tenant", "executor")
                .as_deref(),
            Some("GROK")
        );
        assert_eq!(
            store
                .role_slot_for_tenant("local_tenant", "reviewer")
                .as_deref(),
            Some("OPUS")
        );

        let projection = provider_projection(&kernel, &store);
        assert!(
            projection.contains("\"active_profile_id\":\"premier\""),
            "{projection}"
        );
        assert!(
            projection.contains("\"harness_selection_independent\":true"),
            "{projection}"
        );
        assert!(
            projection
                .contains("\"premier_harnesses\":[\"mdx_native\",\"codex_cli\",\"grok_build\"]"),
            "{projection}"
        );
    }

    #[test]
    fn endpoint_capabilities_are_reported_per_connected_endpoint() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            clear_provider_test_env();
        }
        let kernel = MdxKernel::boot_local();
        let store = isolated_store();
        store_session_key(
            r#"{"provider":"xai","api_key":"xai-session-key","model_id":"grok-capability-test"}"#,
            &kernel,
            &store,
        );
        store_session_key(
            r#"{"provider":"openai","api_key":"openai-session-key","model_id":"gpt-capability-test"}"#,
            &kernel,
            &store,
        );
        store_session_key(
            r#"{"provider":"anthropic","api_key":"anthropic-session-key","model_id":"claude-capability-test"}"#,
            &kernel,
            &store,
        );

        let capabilities = endpoint_capabilities_json_with_store(&kernel, &store, "local_tenant");

        for expected in [
            "\"provider_family\":\"xai\"",
            "\"provider_family\":\"openai\"",
            "\"provider_family\":\"anthropic\"",
            "\"coding_model\":true",
            "\"web_search\":true",
            "\"x_search\":true",
            "\"secret_value_exposed\":false",
        ] {
            assert!(
                capabilities.contains(expected),
                "{expected}: {capabilities}"
            );
        }
    }

    #[test]
    fn session_role_override_changes_cast_without_storing_secrets() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
            std::env::remove_var("OPENAI_API_KEY");
            std::env::remove_var("MDX_ANTHROPIC_MODEL");
            std::env::remove_var("MDX_OPENAI_MODEL");
        }
        let kernel = MdxKernel::boot_local();
        let store = isolated_store();
        store_session_key(
            r#"{"provider":"anthropic","api_key":"anthropic-session-key","model_id":"claude-opus-test"}"#,
            &kernel,
            &store,
        );
        store_session_key(
            r#"{"provider":"openai","api_key":"openai-session-key","model_id":"gpt-review-test"}"#,
            &kernel,
            &store,
        );
        let automatic = resolve_role_endpoint(&kernel, &store, "local_tenant", "reviewer", "")
            .expect("automatic reviewer endpoint");
        assert_eq!(automatic.slot, "OPUS");

        let response = store_session_key(r#"{"role":"reviewer","slot":"GPT"}"#, &kernel, &store);
        assert!(
            response.contains("\"status\":\"ROLE_SLOT_STORED\""),
            "{response}"
        );
        assert!(!response.contains("openai-session-key"), "{response}");
        let overridden = resolve_role_endpoint(&kernel, &store, "local_tenant", "reviewer", "")
            .expect("overridden reviewer endpoint");
        assert_eq!(overridden.slot, "GPT");
        assert_eq!(overridden.provider_family, "openai");
    }

    #[test]
    fn advisor_falls_back_to_strongest_then_honors_assignment() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            clear_provider_test_env();
        }
        let kernel = MdxKernel::boot_local();
        let store = isolated_store();
        store_session_key(
            r#"{"provider":"anthropic","api_key":"anthropic-session-key","model_id":"claude-opus-test"}"#,
            &kernel,
            &store,
        );
        store_session_key(
            r#"{"provider":"openai","api_key":"openai-session-key","model_id":"gpt-advisor-test"}"#,
            &kernel,
            &store,
        );

        // Unassigned advisor lands on the strongest configured model (OPUS wins
        // over GPT in the advisor preference order), never a hard-coded vendor.
        let unassigned = resolve_role_endpoint(&kernel, &store, "local_tenant", "advisor", "")
            .expect("advisor endpoint");
        assert_eq!(unassigned.slot, "OPUS");
        assert_eq!(unassigned.provider_family, "anthropic");

        // The projection carries the advisor role with an empty assignment.
        let projection = provider_projection(&kernel, &store);
        assert!(projection.contains("\"role\":\"advisor\""), "{projection}");
        assert!(
            projection.contains("\"assigned_slot\":\"\""),
            "{projection}"
        );

        // Operator assigns the advisor to a specific model; resolution honors it.
        let response = store_session_key(r#"{"role":"advisor","slot":"GPT"}"#, &kernel, &store);
        assert!(
            response.contains("\"status\":\"ROLE_SLOT_STORED\""),
            "{response}"
        );
        let assigned = resolve_role_endpoint(&kernel, &store, "local_tenant", "advisor", "")
            .expect("assigned advisor endpoint");
        assert_eq!(assigned.slot, "GPT");
        assert_eq!(assigned.provider_family, "openai");

        let after = provider_projection(&kernel, &store);
        assert!(after.contains("\"assigned_slot\":\"GPT\""), "{after}");
    }

    #[test]
    fn remove_provider_session_key_clears_bound_role_override_and_recasts() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            clear_provider_test_env();
        }
        let kernel = MdxKernel::boot_local();
        let store = isolated_store();
        store_session_key(
            r#"{"provider":"anthropic","api_key":"anthropic-session-key","model_id":"claude-opus-test"}"#,
            &kernel,
            &store,
        );
        store_session_key(
            r#"{"provider":"openai","api_key":"openai-session-key","model_id":"gpt-review-test"}"#,
            &kernel,
            &store,
        );
        store_session_key(r#"{"role":"reviewer","slot":"GPT"}"#, &kernel, &store);

        let before = resolve_role_endpoint(&kernel, &store, "local_tenant", "reviewer", "")
            .expect("reviewer before removal");
        assert_eq!(before.slot, "GPT");
        assert_eq!(before.provider_family, "openai");

        let response = store_session_key(
            r#"{"provider":"openai","action":"remove"}"#,
            &kernel,
            &store,
        );

        assert!(
            response.contains("\"status\":\"SESSION_KEY_REMOVED\""),
            "{response}"
        );
        assert!(
            response.contains("\"credential_source\":\"none\""),
            "{response}"
        );
        assert!(
            response.contains("\"removed_role_slot_count\":1"),
            "{response}"
        );
        assert!(!response.contains("openai-session-key"), "{response}");
        let after = resolve_role_endpoint(&kernel, &store, "local_tenant", "reviewer", "")
            .expect("reviewer after removal");
        assert_eq!(after.slot, "OPUS");
        assert_eq!(after.provider_family, "anthropic");
    }

    #[test]
    fn remove_provider_session_key_reports_env_credential_honestly() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            clear_provider_test_env();
            std::env::set_var("OPENAI_API_KEY", "openai-env-key");
        }
        let kernel = MdxKernel::boot_local();
        let store = isolated_store();
        store_session_key(
            r#"{"provider":"openai","api_key":"openai-session-key","model_id":"gpt-session-test"}"#,
            &kernel,
            &store,
        );

        let response = store_session_key(
            r#"{"provider":"openai","action":"remove"}"#,
            &kernel,
            &store,
        );

        assert!(
            response.contains("\"status\":\"SESSION_KEY_REMOVED\""),
            "{response}"
        );
        assert!(
            response.contains("\"credential_available\":true"),
            "{response}"
        );
        assert!(
            response.contains("\"credential_source\":\"env\""),
            "{response}"
        );
        assert!(
            store
                .in_memory_key_for_tenant("local_tenant", "OPENAI_API_KEY")
                .is_none()
        );
        assert!(
            store
                .provider_key_for_tenant("local_tenant", "OPENAI_API_KEY")
                .as_deref()
                == Some("openai-env-key")
        );
        unsafe {
            std::env::remove_var("OPENAI_API_KEY");
        }
    }

    #[test]
    fn openai_session_key_can_cast_the_integrator_to_gpt() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::remove_var("OPENAI_API_KEY");
            std::env::remove_var("MDX_OPENAI_MODEL");
        }
        let kernel = MdxKernel::boot_local();
        let store = isolated_store();
        store_session_key(
            r#"{"provider":"openai","api_key":"openai-session-key","model_id":"gpt-integrator-test"}"#,
            &kernel,
            &store,
        );

        let endpoint = resolve_role_endpoint(&kernel, &store, "local_tenant", "integrator", "")
            .expect("integrator endpoint");
        assert_eq!(endpoint.slot, "GPT");
        assert_eq!(endpoint.provider_family, "openai");
        assert_eq!(endpoint.model_id, "gpt-integrator-test");
        assert_eq!(endpoint.base_url, "https://api.openai.com/v1");
        let planner = resolve_role_endpoint(&kernel, &store, "local_tenant", "planner", "")
            .expect("planner endpoint");
        assert_eq!(planner.slot, "GPT");
        let reviewer = resolve_role_endpoint(&kernel, &store, "local_tenant", "reviewer", "")
            .expect("reviewer endpoint");
        assert_eq!(reviewer.slot, "GPT");
    }

    #[test]
    fn reviewer_falls_back_to_opus_when_no_fresh_family_is_connected() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
            std::env::remove_var("OPENAI_API_KEY");
            std::env::remove_var("MDX_ANTHROPIC_MODEL");
            std::env::remove_var("MDX_OPENAI_MODEL");
        }
        let kernel = MdxKernel::boot_local();
        let store = isolated_store();
        store_session_key(
            r#"{"provider":"anthropic","api_key":"anthropic-session-key"}"#,
            &kernel,
            &store,
        );

        let reviewer = resolve_role_endpoint(&kernel, &store, "local_tenant", "reviewer", "")
            .expect("reviewer endpoint");
        assert_eq!(reviewer.slot, "OPUS");
        assert_eq!(reviewer.provider_family, "anthropic");
    }

    unsafe fn clear_provider_test_env() {
        for key in [
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "XAI_API_KEY",
            "MOONSHOT_API_KEY",
            "GEMINI_API_KEY",
            "MDX_ANTHROPIC_MODEL",
            "MDX_ANTHROPIC_OPUS_MODEL",
            "MDX_ANTHROPIC_SONNET_MODEL",
            "MDX_OPENAI_MODEL",
            "MDX_OPENAI_CODEX_MODEL",
            "MDX_OPENAI_CODEX_MINI_MODEL",
            "MDX_XAI_MODEL",
            "MDX_XAI_BUILD_MODEL",
            "MDX_MOONSHOT_MODEL",
            "MDX_GEMINI_MODEL",
            "MDX_FLEET_REVIEWER_BASE_URL",
            "MDX_FLEET_REVIEWER_API_KEY",
            "MDX_FLEET_REVIEWER_MODEL",
            "MDX_FLEET_REVIEWER_PROVIDER",
            "MDX_FLEET_BUILDER_GPT_BASE_URL",
            "MDX_FLEET_BUILDER_GPT_API_KEY",
            "MDX_FLEET_BUILDER_GPT_MODEL",
            "MDX_FLEET_BUILDER_OPUS_BASE_URL",
            "MDX_FLEET_BUILDER_OPUS_API_KEY",
            "MDX_FLEET_BUILDER_OPUS_MODEL",
        ] {
            unsafe {
                std::env::remove_var(key);
            }
        }
    }

    fn isolated_store() -> SecretStore {
        let store = SecretStore::default();
        for provider in PROVIDERS {
            store.disable_keychain_lookup_for_tenant("local_tenant", provider.env_key);
        }
        store
    }
}
