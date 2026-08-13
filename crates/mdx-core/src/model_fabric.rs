use crate::{CorrelationIds, MdxKernel, Receipt, StorageProvider};
use std::collections::BTreeMap;

pub const MODEL_FABRIC_STATUS: &str = "PRODUCTION-CANDIDATE";
pub const MODEL_ROUTE_PRESETS: [&str; 4] = ["balanced", "quality", "fast", "private"];
pub const MODEL_ROUTE_SELECTED_RECEIPT_KIND: &str = "model.route.selected";
pub const MODEL_ROUTE_DENIED_RECEIPT_KIND: &str = "model.route.denied";
pub const MODEL_CONNECTION_CONFIGURED_RECEIPT_KIND: &str = "model.connection.configured";
pub const MODEL_CONNECTION_REFUSED_RECEIPT_KIND: &str = "model.connection.refused";
pub const MODEL_DEPLOYMENT_REGISTERED_RECEIPT_KIND: &str = "model.deployment.registered";
pub const MODEL_ROUTE_POLICY_CONFIGURED_RECEIPT_KIND: &str = "model.route_policy.configured";
pub const MODEL_OUTCOME_RECORDED_RECEIPT_KIND: &str = "model.outcome.recorded";
pub const MODEL_OUTCOME_EVALUATION_REFUSED_RECEIPT_KIND: &str = "model.outcome_evaluation.refused";
pub const MODEL_ADAPTIVE_POLICY_EVALUATED_RECEIPT_KIND: &str = "model.adaptive_policy.evaluated";
pub const MODEL_ADAPTIVE_POLICY_REFUSED_RECEIPT_KIND: &str = "model.adaptive_policy.refused";
pub const MODEL_ADAPTIVE_POLICY_PROMOTED_RECEIPT_KIND: &str = "model.adaptive_policy.promoted";
pub const MODEL_ADAPTIVE_POLICY_AUTO_ROLLBACK_RECEIPT_KIND: &str =
    "model.adaptive_policy.auto_rollback";
pub const MODEL_ADAPTIVE_REPLAY_RECEIPT_KIND: &str = "model.adaptive_policy.replay";
pub const MODEL_ADAPTIVE_SHADOW_RECEIPT_KIND: &str = "model.adaptive_policy.shadow";
pub const MODEL_CIRCUIT_TRANSITION_RECEIPT_KIND: &str = "model.circuit.transition";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderConnection {
    pub connection_id: String,
    pub tenant_id: String,
    pub provider_id: String,
    pub credential_ref: String,
    pub endpoint_base_url: String,
    pub region: String,
    pub residency: String,
    pub data_retention: String,
    pub training_policy: String,
    pub data_policy_provenance: String,
    pub data_policy_observed_at: String,
    pub health: String,
    pub live_call_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    pub model_id: String,
    pub provider_model_id: String,
    pub provider_id: String,
    pub display_name: String,
    pub lifecycle: String,
    pub modality: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapabilityProfile {
    pub tools: bool,
    pub structured_output: bool,
    pub vision: bool,
    pub context_tokens: u64,
    pub provenance: String,
    pub observed_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelDeployment {
    pub deployment_id: String,
    pub connection_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub privacy_class: String,
    pub region: String,
    pub residency: String,
    pub enabled: bool,
    pub capabilities: CapabilityProfile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutePolicy {
    pub policy_id: String,
    pub policy_version: String,
    pub lifecycle: String,
    pub state: String,
    pub canary_percent: u8,
    pub tenant_id: String,
    pub workload_ids: Vec<String>,
    pub allowed_app_ids: Vec<String>,
    pub allowed_environments: Vec<String>,
    pub allowed_provider_ids: Vec<String>,
    pub preferred_deployment_ids: Vec<String>,
    pub denied_provider_ids: Vec<String>,
    pub denied_deployment_ids: Vec<String>,
    pub allowed_regions: Vec<String>,
    pub required_residency: Option<String>,
    pub allow_data_retention: bool,
    pub allow_training: bool,
    pub max_input_cost_microusd_per_million: u64,
    pub max_output_cost_microusd_per_million: u64,
}

impl RoutePolicy {
    pub fn core(tenant_id: impl Into<String>) -> Self {
        Self {
            policy_id: "core".to_string(),
            policy_version: "core-v1".to_string(),
            lifecycle: "static".to_string(),
            state: "promoted".to_string(),
            canary_percent: 0,
            tenant_id: tenant_id.into(),
            workload_ids: Vec::new(),
            allowed_app_ids: Vec::new(),
            allowed_environments: Vec::new(),
            allowed_provider_ids: Vec::new(),
            preferred_deployment_ids: Vec::new(),
            denied_provider_ids: Vec::new(),
            denied_deployment_ids: Vec::new(),
            allowed_regions: Vec::new(),
            required_residency: None,
            allow_data_retention: true,
            allow_training: false,
            max_input_cost_microusd_per_million: 0,
            max_output_cost_microusd_per_million: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelPriceObservation {
    pub deployment_id: String,
    pub input_microusd_per_million: u64,
    pub output_microusd_per_million: u64,
    pub currency: String,
    pub source: String,
    pub observed_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelOutcome {
    pub decision_id: String,
    pub workload_id: String,
    pub app_id: String,
    pub environment: String,
    pub session_id: String,
    pub deployment_id: String,
    pub latency_ms: u64,
    pub cost_microusd: Option<u64>,
    pub quality_score: Option<u32>,
    pub safety_status: String,
    pub task_status: String,
    pub correction_status: String,
    pub provenance: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdaptivePolicyEvidence {
    pub policy_id: String,
    pub policy_version: String,
    pub state: String,
    pub replay_cases: u64,
    pub shadow_decisions: u64,
    pub canary_decisions: u64,
    pub guardrail_evidence_sufficient: bool,
    pub quality_guardrail_passed: bool,
    pub safety_guardrail_passed: bool,
    pub latency_guardrail_passed: bool,
    pub cost_guardrail_passed: bool,
}

pub fn next_adaptive_policy_state(evidence: &AdaptivePolicyEvidence) -> &'static str {
    let guardrails = evidence.quality_guardrail_passed
        && evidence.safety_guardrail_passed
        && evidence.latency_guardrail_passed
        && evidence.cost_guardrail_passed;
    if !evidence.guardrail_evidence_sufficient {
        return "hold";
    }
    if !guardrails {
        return "rollback_required";
    }
    match evidence.state.as_str() {
        "draft" if evidence.replay_cases >= 100 => "shadow",
        "shadow" if evidence.shadow_decisions >= 500 => "canary",
        "canary" if evidence.canary_decisions >= 100 => "promoted",
        "promoted" => "promoted",
        _ => "hold",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelProviderDefinition {
    pub provider_id: &'static str,
    pub label: &'static str,
    pub adapter: &'static str,
    pub wire_api: &'static str,
    pub credential_env: &'static str,
    pub discovery: &'static str,
    pub hosting_class: &'static str,
    pub open_model_host: bool,
}

pub const MODEL_PROVIDER_CATALOG: [ModelProviderDefinition; 16] = [
    provider(
        "openai",
        "OpenAI",
        "OpenAIResponsesModelGateway",
        "responses",
        "OPENAI_API_KEY",
        "frontier_managed",
        false,
    ),
    provider(
        "anthropic",
        "Anthropic",
        "AnthropicMessagesModelGateway",
        "messages",
        "ANTHROPIC_API_KEY",
        "frontier_managed",
        false,
    ),
    provider(
        "xai",
        "xAI",
        "XaiChatModelGateway",
        "chat_completions",
        "XAI_API_KEY",
        "frontier_managed",
        false,
    ),
    provider(
        "moonshot",
        "Kimi / Moonshot AI",
        "MoonshotChatModelGateway",
        "chat_completions",
        "MOONSHOT_API_KEY",
        "frontier_and_open",
        true,
    ),
    provider(
        "google",
        "Google",
        "GeminiModelGateway",
        "generate_content",
        "GEMINI_API_KEY",
        "frontier_managed",
        false,
    ),
    provider(
        "mistral",
        "Mistral",
        "MistralChatModelGateway",
        "chat_completions",
        "MISTRAL_API_KEY",
        "frontier_and_open",
        true,
    ),
    provider(
        "aws_bedrock",
        "AWS Bedrock",
        "BedrockModelGateway",
        "bedrock_converse",
        "AWS_BEDROCK_MODEL_ACCESS",
        "managed_catalog",
        true,
    ),
    provider(
        "azure_foundry",
        "Microsoft Foundry",
        "AzureFoundryModelGateway",
        "responses",
        "AZURE_FOUNDRY_ENDPOINT",
        "managed_catalog",
        true,
    ),
    provider(
        "huggingface",
        "Hugging Face",
        "HuggingFaceModelGateway",
        "chat_completions",
        "HF_TOKEN",
        "open_model_router",
        true,
    ),
    provider(
        "together",
        "Together AI",
        "TogetherModelGateway",
        "chat_completions",
        "TOGETHER_API_KEY",
        "open_model_host",
        true,
    ),
    provider(
        "fireworks",
        "Fireworks AI",
        "FireworksModelGateway",
        "chat_completions",
        "FIREWORKS_API_KEY",
        "open_model_host",
        true,
    ),
    provider(
        "openrouter",
        "OpenRouter",
        "OpenRouterModelGateway",
        "chat_completions",
        "OPENROUTER_API_KEY",
        "model_router",
        true,
    ),
    provider(
        "vercel",
        "Vercel AI Gateway",
        "VercelAiGateway",
        "chat_completions",
        "AI_GATEWAY_API_KEY",
        "model_gateway",
        true,
    ),
    provider(
        "tensorzero",
        "TensorZero",
        "TensorZeroModelGateway",
        "tensorzero_inference",
        "TENSORZERO_GATEWAY_URL",
        "self_hosted_gateway",
        true,
    ),
    provider(
        "ollama",
        "Ollama",
        "OllamaLocalModelGateway",
        "chat_completions",
        "MDX_OLLAMA_API_KEY",
        "local",
        true,
    ),
    provider(
        "vllm",
        "vLLM",
        "VllmModelGateway",
        "chat_completions",
        "MDX_VLLM_API_KEY",
        "self_hosted",
        true,
    ),
];

const fn provider(
    provider_id: &'static str,
    label: &'static str,
    adapter: &'static str,
    wire_api: &'static str,
    credential_env: &'static str,
    hosting_class: &'static str,
    open_model_host: bool,
) -> ModelProviderDefinition {
    ModelProviderDefinition {
        provider_id,
        label,
        adapter,
        wire_api,
        credential_env,
        discovery: "provider_or_gateway_catalog",
        hosting_class,
        open_model_host,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelWorkloadContract {
    pub workload_id: &'static str,
    pub surface: &'static str,
    pub stage: &'static str,
    pub default_preset: &'static str,
    pub sensitivity: &'static str,
    pub tools_required: bool,
    pub structured_output_required: bool,
    pub vision_required: bool,
    pub session_sticky: bool,
    pub cascade_allowed: bool,
}

// Catalog declarations stay one row per workload; the helper keeps the table
// auditable without duplicating struct field names eighteen times.
#[allow(clippy::too_many_arguments)]
const fn workload(
    id: &'static str,
    surface: &'static str,
    stage: &'static str,
    preset: &'static str,
    sensitivity: &'static str,
    tools: bool,
    structured: bool,
    sticky: bool,
    cascade: bool,
) -> ModelWorkloadContract {
    ModelWorkloadContract {
        workload_id: id,
        surface,
        stage,
        default_preset: preset,
        sensitivity,
        tools_required: tools,
        structured_output_required: structured,
        vision_required: false,
        session_sticky: sticky,
        cascade_allowed: cascade,
    }
}

pub const MODEL_WORKLOAD_CATALOG: [ModelWorkloadContract; 18] = [
    workload(
        "mdx/twin/conversation",
        "twin",
        "conversation",
        "balanced",
        "internal",
        true,
        false,
        true,
        false,
    ),
    workload(
        "mdx/twin/private-reasoning",
        "twin",
        "private_reasoning",
        "private",
        "sensitive",
        false,
        false,
        true,
        false,
    ),
    workload(
        "mdx/forge/planner",
        "forge",
        "planner",
        "quality",
        "internal",
        true,
        true,
        true,
        false,
    ),
    workload(
        "mdx/forge/builder",
        "forge",
        "builder",
        "balanced",
        "internal",
        true,
        true,
        true,
        true,
    ),
    workload(
        "mdx/forge/reviewer",
        "forge",
        "reviewer",
        "quality",
        "internal",
        true,
        true,
        false,
        false,
    ),
    workload(
        "mdx/forge/evaluator",
        "forge",
        "evaluator",
        "quality",
        "internal",
        false,
        true,
        false,
        false,
    ),
    workload(
        "mdx/forge/advisor",
        "forge",
        "advisor",
        "quality",
        "internal",
        false,
        true,
        false,
        false,
    ),
    workload(
        "mdx/forge/voice",
        "forge",
        "voice",
        "fast",
        "internal",
        false,
        false,
        false,
        true,
    ),
    workload(
        "mdx/pages/compose",
        "pages",
        "compose",
        "balanced",
        "internal",
        false,
        false,
        true,
        true,
    ),
    workload(
        "mdx/pages/summarize",
        "pages",
        "summarize",
        "fast",
        "internal",
        false,
        true,
        false,
        true,
    ),
    workload(
        "mdx/message/triage",
        "message",
        "triage",
        "fast",
        "internal",
        false,
        true,
        false,
        true,
    ),
    workload(
        "mdx/message/compose",
        "message",
        "compose",
        "balanced",
        "internal",
        false,
        false,
        true,
        true,
    ),
    workload(
        "mdx/product/synthesis",
        "product",
        "synthesis",
        "quality",
        "internal",
        false,
        true,
        false,
        false,
    ),
    workload(
        "mdx/product/feedback-triage",
        "product",
        "feedback_triage",
        "fast",
        "internal",
        false,
        true,
        false,
        true,
    ),
    workload(
        "mdx/strategy/deliberation",
        "strategy",
        "deliberation",
        "quality",
        "internal",
        false,
        true,
        true,
        false,
    ),
    workload(
        "mdx/observatory/explain",
        "observatory",
        "explain",
        "fast",
        "internal",
        false,
        true,
        false,
        true,
    ),
    workload(
        "mdx/marketplace/review",
        "marketplace",
        "review",
        "quality",
        "open",
        false,
        true,
        false,
        false,
    ),
    workload(
        "mdx/talent/assessment",
        "talent",
        "assessment",
        "quality",
        "sensitive",
        false,
        true,
        false,
        false,
    ),
];

pub fn model_provider(provider_id: &str) -> Option<&'static ModelProviderDefinition> {
    let provider_id = match provider_id {
        "gemini" => "google",
        "bedrock" => "aws_bedrock",
        "azure" | "azure_openai" => "azure_foundry",
        other => other,
    };
    MODEL_PROVIDER_CATALOG
        .iter()
        .find(|row| row.provider_id == provider_id)
}
pub fn canonical_model_provider_id(provider_id: &str) -> Option<&'static str> {
    model_provider(provider_id).map(|provider| provider.provider_id)
}
pub fn model_workload(workload_id: &str) -> Option<&'static ModelWorkloadContract> {
    MODEL_WORKLOAD_CATALOG
        .iter()
        .find(|row| row.workload_id == workload_id)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRouteCandidate<'a> {
    pub deployment_id: &'a str,
    pub connection_id: &'a str,
    pub tenant_id: &'a str,
    pub provider_id: &'a str,
    pub model_id: &'a str,
    pub privacy_class: &'a str,
    pub region: &'a str,
    pub residency: &'a str,
    pub data_retention: &'a str,
    pub training_policy: &'a str,
    pub connected: bool,
    pub healthy: bool,
    pub live_call_allowed: bool,
    pub tools: bool,
    pub structured_output: bool,
    pub vision: bool,
    pub context_tokens: u64,
    pub input_cost_microusd_per_million: u64,
    pub output_cost_microusd_per_million: u64,
    pub price_known: bool,
    pub p95_latency_ms: u64,
    pub latency_known: bool,
    pub quality_score: u32,
    pub quality_known: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelPolicyContext<'a> {
    pub tenant_id: &'a str,
    pub app_id: &'a str,
    pub environment: &'a str,
    pub required_region: Option<&'a str>,
    pub required_residency: Option<&'a str>,
    pub session_id: Option<&'a str>,
    pub prior_deployment_id: Option<&'a str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRouteRequest<'a> {
    pub workload_id: &'a str,
    pub preset: &'a str,
    pub policy: &'a RoutePolicy,
    pub expected_policy_version: Option<&'a str>,
    pub context: ModelPolicyContext<'a>,
    pub required_context_tokens: u64,
    pub max_input_cost_microusd_per_million: u64,
    pub max_output_cost_microusd_per_million: u64,
    pub pinned_deployment_id: Option<&'a str>,
    pub candidates: &'a [ModelRouteCandidate<'a>],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRouteExclusion {
    pub deployment_id: String,
    pub reason: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelRouteDecision {
    pub status: &'static str,
    pub workload_id: String,
    pub app_id: String,
    pub environment: String,
    pub session_id: String,
    pub preset: String,
    pub policy_id: String,
    pub policy_version: String,
    pub selected_deployment_id: Option<String>,
    pub selected_provider_id: Option<String>,
    pub selected_model_id: Option<String>,
    pub selection_reason: &'static str,
    pub session_sticky_applied: bool,
    pub provider_failover_deployment_ids: Vec<String>,
    pub model_fallback_deployment_ids: Vec<String>,
    pub fallback_deployment_ids: Vec<String>,
    pub exclusions: Vec<ModelRouteExclusion>,
    pub grants_execution_authority: bool,
}

fn allows(required: &str, actual: &str) -> bool {
    match required {
        "sensitive" => matches!(actual, "sovereign" | "local"),
        "internal" => matches!(actual, "frontier" | "sovereign" | "local"),
        _ => matches!(actual, "open" | "frontier" | "sovereign" | "local"),
    }
}

fn reject(
    workload: &ModelWorkloadContract,
    request: &ModelRouteRequest<'_>,
    candidate: &ModelRouteCandidate<'_>,
) -> Option<&'static str> {
    let policy = request.policy;
    let context = &request.context;
    let required_residency = context
        .required_residency
        .or(policy.required_residency.as_deref());
    if policy.tenant_id != context.tenant_id || candidate.tenant_id != context.tenant_id {
        Some("tenant_scope_mismatch")
    } else if !policy.allowed_app_ids.is_empty()
        && !policy.allowed_app_ids.iter().any(|id| id == context.app_id)
    {
        Some("app_not_allowed")
    } else if !policy.allowed_environments.is_empty()
        && !policy
            .allowed_environments
            .iter()
            .any(|environment| environment == context.environment)
    {
        Some("environment_not_allowed")
    } else if model_provider(candidate.provider_id).is_none() {
        Some("unknown_provider")
    } else if !policy.allowed_provider_ids.is_empty()
        && !policy
            .allowed_provider_ids
            .iter()
            .any(|provider| provider == candidate.provider_id)
    {
        Some("provider_not_allowed")
    } else if policy
        .denied_provider_ids
        .iter()
        .any(|provider| provider == candidate.provider_id)
    {
        Some("provider_denied")
    } else if policy
        .denied_deployment_ids
        .iter()
        .any(|deployment| deployment == candidate.deployment_id)
    {
        Some("deployment_denied")
    } else if !candidate.connected {
        Some("provider_not_connected")
    } else if !candidate.healthy {
        Some("deployment_unhealthy")
    } else if !candidate.live_call_allowed {
        Some("live_call_not_allowed")
    } else if !allows(workload.sensitivity, candidate.privacy_class) {
        Some("privacy_class_not_allowed")
    } else if let Some(region) = context.required_region
        && candidate.region != region
    {
        Some("required_region_not_available")
    } else if !policy.allowed_regions.is_empty()
        && !policy
            .allowed_regions
            .iter()
            .any(|region| region == candidate.region)
    {
        Some("region_not_allowed")
    } else if let Some(residency) = required_residency
        && candidate.residency != residency
    {
        Some("residency_not_allowed")
    } else if !matches!(candidate.data_retention, "none" | "provider") {
        Some("data_retention_unknown")
    } else if !policy.allow_data_retention && candidate.data_retention != "none" {
        Some("data_retention_not_allowed")
    } else if !matches!(candidate.training_policy, "none" | "allowed") {
        Some("training_policy_unknown")
    } else if !policy.allow_training && candidate.training_policy != "none" {
        Some("training_not_allowed")
    } else if workload.tools_required && !candidate.tools {
        Some("tools_not_supported")
    } else if workload.structured_output_required && !candidate.structured_output {
        Some("structured_output_not_supported")
    } else if workload.vision_required && !candidate.vision {
        Some("vision_not_supported")
    } else if candidate.context_tokens < request.required_context_tokens {
        Some("context_window_too_small")
    } else if candidate.quality_score > 100 {
        Some("quality_score_out_of_range")
    } else if (request.max_input_cost_microusd_per_million > 0
        || request.max_output_cost_microusd_per_million > 0
        || policy.max_input_cost_microusd_per_million > 0
        || policy.max_output_cost_microusd_per_million > 0)
        && !candidate.price_known
    {
        Some("price_provenance_required")
    } else if request.max_input_cost_microusd_per_million > 0
        && candidate.input_cost_microusd_per_million > request.max_input_cost_microusd_per_million
    {
        Some("input_cost_ceiling_exceeded")
    } else if request.max_output_cost_microusd_per_million > 0
        && candidate.output_cost_microusd_per_million > request.max_output_cost_microusd_per_million
    {
        Some("cost_ceiling_exceeded")
    } else if policy.max_input_cost_microusd_per_million > 0
        && candidate.input_cost_microusd_per_million > policy.max_input_cost_microusd_per_million
    {
        Some("policy_input_cost_ceiling_exceeded")
    } else if policy.max_output_cost_microusd_per_million > 0
        && candidate.output_cost_microusd_per_million > policy.max_output_cost_microusd_per_million
    {
        Some("policy_output_cost_ceiling_exceeded")
    } else if request.preset == "private"
        && !matches!(candidate.privacy_class, "sovereign" | "local")
    {
        Some("private_preset_requires_sovereign_or_local")
    } else {
        None
    }
}

fn rank<'a>(preset: &str, row: &'a ModelRouteCandidate<'a>) -> (u128, u128, u128, &'a str) {
    let unknown_quality = u128::from(!row.quality_known) * 1_000_000;
    let unknown_latency = u128::from(!row.latency_known) * 1_000_000;
    let unknown_price = u128::from(!row.price_known) * 1_000_000;
    match preset {
        "quality" | "private" => (
            unknown_quality + u128::from(100 - row.quality_score),
            unknown_latency + u128::from(row.p95_latency_ms),
            unknown_price + u128::from(row.output_cost_microusd_per_million),
            row.deployment_id,
        ),
        "fast" => (
            unknown_latency + u128::from(row.p95_latency_ms),
            unknown_quality + u128::from(100 - row.quality_score),
            unknown_price + u128::from(row.output_cost_microusd_per_million),
            row.deployment_id,
        ),
        _ => {
            let quality = u128::from(100 - row.quality_score) * 550;
            let latency = u128::from(row.p95_latency_ms.min(10_000)) * 25_000 / 10_000;
            let cost =
                u128::from(row.output_cost_microusd_per_million.min(100_000)) * 20_000 / 100_000;
            (
                unknown_quality + unknown_latency + unknown_price + quality + latency + cost,
                unknown_latency + u128::from(row.p95_latency_ms),
                unknown_price + u128::from(row.output_cost_microusd_per_million),
                row.deployment_id,
            )
        }
    }
}

fn denied(
    request: &ModelRouteRequest<'_>,
    preset: &str,
    reason: &'static str,
    exclusions: Vec<ModelRouteExclusion>,
) -> ModelRouteDecision {
    ModelRouteDecision {
        status: "ROUTE_DENIED",
        workload_id: request.workload_id.to_string(),
        app_id: request.context.app_id.to_string(),
        environment: request.context.environment.to_string(),
        session_id: request.context.session_id.unwrap_or_default().to_string(),
        preset: preset.to_string(),
        policy_id: request.policy.policy_id.clone(),
        policy_version: request.policy.policy_version.clone(),
        selected_deployment_id: None,
        selected_provider_id: None,
        selected_model_id: None,
        selection_reason: reason,
        session_sticky_applied: false,
        provider_failover_deployment_ids: Vec::new(),
        model_fallback_deployment_ids: Vec::new(),
        fallback_deployment_ids: Vec::new(),
        exclusions,
        grants_execution_authority: false,
    }
}

pub fn resolve_model_route(request: &ModelRouteRequest<'_>) -> ModelRouteDecision {
    let Some(workload) = model_workload(request.workload_id) else {
        return denied(request, request.preset, "unknown_workload", Vec::new());
    };
    if workload.surface != request.context.app_id {
        return denied(request, request.preset, "workload_app_mismatch", Vec::new());
    }
    if let Some(expected) = request.expected_policy_version
        && expected != request.policy.policy_version
    {
        return denied(
            request,
            request.preset,
            "policy_version_mismatch",
            Vec::new(),
        );
    }
    let preset = if request.preset.is_empty() {
        workload.default_preset
    } else {
        request.preset
    };
    if !MODEL_ROUTE_PRESETS.contains(&preset) {
        return denied(request, preset, "unknown_preset", Vec::new());
    }
    let effective = ModelRouteRequest {
        preset,
        ..request.clone()
    };
    let mut eligible = Vec::new();
    let mut exclusions = Vec::new();
    for row in request.candidates {
        if let Some(reason) = reject(workload, &effective, row) {
            exclusions.push(ModelRouteExclusion {
                deployment_id: row.deployment_id.to_string(),
                reason,
            });
        } else {
            eligible.push(row);
        }
    }
    eligible.sort_by_key(|row| rank(preset, row));
    let mut selection_reason = "policy_objective";
    let mut session_sticky_applied = false;
    if let Some(pin) = request.pinned_deployment_id {
        let Some(index) = eligible.iter().position(|row| row.deployment_id == pin) else {
            return denied(request, preset, "pinned_deployment_ineligible", exclusions);
        };
        let row = eligible.remove(index);
        eligible.insert(0, row);
        selection_reason = "explicit_pin";
    } else if workload.session_sticky
        && request.context.session_id.is_some()
        && let Some(prior) = request.context.prior_deployment_id
        && let Some(index) = eligible.iter().position(|row| row.deployment_id == prior)
    {
        let row = eligible.remove(index);
        eligible.insert(0, row);
        selection_reason = "eligible_session_sticky";
        session_sticky_applied = true;
    } else if let Some(index) =
        request
            .policy
            .preferred_deployment_ids
            .iter()
            .find_map(|preferred| {
                eligible
                    .iter()
                    .position(|row| row.deployment_id == preferred)
            })
    {
        let row = eligible.remove(index);
        eligible.insert(0, row);
        selection_reason = "policy_deployment_preference";
    }
    let Some(selected) = eligible.first() else {
        return denied(request, preset, "no_eligible_deployment", exclusions);
    };
    let provider_failover_deployment_ids = eligible
        .iter()
        .skip(1)
        .filter(|row| row.model_id == selected.model_id)
        .map(|row| row.deployment_id.to_string())
        .collect::<Vec<_>>();
    let model_fallback_deployment_ids = eligible
        .iter()
        .skip(1)
        .filter(|row| row.model_id != selected.model_id)
        .map(|row| row.deployment_id.to_string())
        .collect::<Vec<_>>();
    let fallback_deployment_ids = provider_failover_deployment_ids
        .iter()
        .chain(model_fallback_deployment_ids.iter())
        .cloned()
        .collect();
    ModelRouteDecision {
        status: "ROUTE_SELECTED",
        workload_id: request.workload_id.to_string(),
        app_id: request.context.app_id.to_string(),
        environment: request.context.environment.to_string(),
        session_id: request.context.session_id.unwrap_or_default().to_string(),
        preset: preset.to_string(),
        policy_id: request.policy.policy_id.clone(),
        policy_version: request.policy.policy_version.clone(),
        selected_deployment_id: Some(selected.deployment_id.to_string()),
        selected_provider_id: canonical_model_provider_id(selected.provider_id).map(str::to_string),
        selected_model_id: Some(selected.model_id.to_string()),
        selection_reason,
        session_sticky_applied,
        provider_failover_deployment_ids,
        model_fallback_deployment_ids,
        fallback_deployment_ids,
        exclusions,
        grants_execution_authority: false,
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_model_adaptive_comparison(
        &mut self,
        correlation: &CorrelationIds,
        kind: &'static str,
        policy: &RoutePolicy,
        baseline_decision_id: &str,
        baseline_deployment_id: &str,
        candidate: &ModelRouteDecision,
    ) -> Result<Receipt, &'static str> {
        if !matches!(
            kind,
            MODEL_ADAPTIVE_REPLAY_RECEIPT_KIND | MODEL_ADAPTIVE_SHADOW_RECEIPT_KIND
        ) || policy.lifecycle != "adaptive"
            || baseline_decision_id.trim().is_empty()
        {
            return Err("adaptive comparison identity is invalid");
        }
        let mut values = BTreeMap::new();
        values.insert("policy_id".to_string(), policy.policy_id.clone());
        values.insert("policy_version".to_string(), policy.policy_version.clone());
        values.insert(
            "baseline_decision_id".to_string(),
            baseline_decision_id.to_string(),
        );
        values.insert(
            "baseline_deployment_id".to_string(),
            baseline_deployment_id.to_string(),
        );
        values.insert("workload_id".to_string(), candidate.workload_id.clone());
        values.insert("candidate_status".to_string(), candidate.status.to_string());
        values.insert(
            "candidate_deployment_id".to_string(),
            candidate.selected_deployment_id.clone().unwrap_or_default(),
        );
        values.insert(
            "candidate_changed_route".to_string(),
            (candidate.selected_deployment_id.as_deref() != Some(baseline_deployment_id))
                .to_string(),
        );
        values.insert(
            "grants_execution_authority".to_string(),
            "false".to_string(),
        );
        Ok(self
            .storage
            .append_receipt(&mut self.ids, correlation, kind, None, values))
    }

    pub fn record_model_outcome(
        &mut self,
        correlation: &CorrelationIds,
        outcome: &ModelOutcome,
    ) -> Result<Receipt, &'static str> {
        if outcome.decision_id.trim().is_empty()
            || outcome.workload_id.trim().is_empty()
            || outcome.deployment_id.trim().is_empty()
            || outcome.provenance.trim().is_empty()
        {
            return Err("model outcome identity and provenance are required");
        }
        if outcome.quality_score.is_some_and(|score| score > 100) {
            return Err("model outcome quality score must be between 0 and 100");
        }
        if !matches!(
            outcome.safety_status.as_str(),
            "not_evaluated" | "passed" | "failed"
        ) || !matches!(
            outcome.task_status.as_str(),
            "not_evaluated" | "succeeded" | "failed"
        ) || !matches!(
            outcome.correction_status.as_str(),
            "not_observed" | "accepted" | "corrected"
        ) {
            return Err("model outcome evaluation status is invalid");
        }
        let mut values = BTreeMap::new();
        values.insert("decision_id".to_string(), outcome.decision_id.clone());
        values.insert("workload_id".to_string(), outcome.workload_id.clone());
        values.insert("app_id".to_string(), outcome.app_id.clone());
        values.insert("environment".to_string(), outcome.environment.clone());
        values.insert("session_id".to_string(), outcome.session_id.clone());
        values.insert("deployment_id".to_string(), outcome.deployment_id.clone());
        values.insert("latency_ms".to_string(), outcome.latency_ms.to_string());
        values.insert(
            "cost_microusd".to_string(),
            outcome
                .cost_microusd
                .map(|cost| cost.to_string())
                .unwrap_or_default(),
        );
        values.insert(
            "quality_score".to_string(),
            outcome
                .quality_score
                .map(|score| score.to_string())
                .unwrap_or_default(),
        );
        values.insert("safety_status".to_string(), outcome.safety_status.clone());
        values.insert("task_status".to_string(), outcome.task_status.clone());
        values.insert(
            "correction_status".to_string(),
            outcome.correction_status.clone(),
        );
        values.insert("provenance".to_string(), outcome.provenance.clone());
        values.insert(
            "grants_execution_authority".to_string(),
            "false".to_string(),
        );
        Ok(self.storage.append_receipt(
            &mut self.ids,
            correlation,
            MODEL_OUTCOME_RECORDED_RECEIPT_KIND,
            None,
            values,
        ))
    }

    pub fn record_adaptive_policy_evaluation(
        &mut self,
        correlation: &CorrelationIds,
        evidence: &AdaptivePolicyEvidence,
    ) -> Result<Receipt, &'static str> {
        if evidence.policy_id.trim().is_empty() || evidence.policy_version.trim().is_empty() {
            return Err("adaptive policy id and version are required");
        }
        if !matches!(
            evidence.state.as_str(),
            "draft" | "shadow" | "canary" | "promoted"
        ) {
            return Err("adaptive policy state is invalid");
        }
        let next_state = next_adaptive_policy_state(evidence);
        let guardrails_passed = evidence.quality_guardrail_passed
            && evidence.safety_guardrail_passed
            && evidence.latency_guardrail_passed
            && evidence.cost_guardrail_passed;
        let mut values = BTreeMap::new();
        values.insert("policy_id".to_string(), evidence.policy_id.clone());
        values.insert(
            "policy_version".to_string(),
            evidence.policy_version.clone(),
        );
        values.insert("previous_state".to_string(), evidence.state.clone());
        // Evaluation may advance pre-production stages or fail a canary closed,
        // but promotion is a separate operator-approved action.
        let applied_state = match next_state {
            "hold" | "promoted" => evidence.state.clone(),
            transition => transition.to_string(),
        };
        values.insert("state".to_string(), applied_state);
        values.insert("recommended_transition".to_string(), next_state.to_string());
        values.insert(
            "replay_cases".to_string(),
            evidence.replay_cases.to_string(),
        );
        values.insert(
            "shadow_decisions".to_string(),
            evidence.shadow_decisions.to_string(),
        );
        values.insert(
            "canary_decisions".to_string(),
            evidence.canary_decisions.to_string(),
        );
        values.insert(
            "guardrails_passed".to_string(),
            guardrails_passed.to_string(),
        );
        values.insert(
            "guardrail_evidence_sufficient".to_string(),
            evidence.guardrail_evidence_sufficient.to_string(),
        );
        values.insert(
            "grants_execution_authority".to_string(),
            "false".to_string(),
        );
        Ok(self.storage.append_receipt(
            &mut self.ids,
            correlation,
            MODEL_ADAPTIVE_POLICY_EVALUATED_RECEIPT_KIND,
            None,
            values,
        ))
    }

    pub fn record_model_route_policy_configured(
        &mut self,
        correlation: &CorrelationIds,
        policy: &RoutePolicy,
    ) -> Receipt {
        let mut values = BTreeMap::new();
        values.insert("policy_id".to_string(), policy.policy_id.clone());
        values.insert("policy_version".to_string(), policy.policy_version.clone());
        values.insert("lifecycle".to_string(), policy.lifecycle.clone());
        values.insert("state".to_string(), policy.state.clone());
        values.insert(
            "canary_percent".to_string(),
            policy.canary_percent.to_string(),
        );
        values.insert("workload_ids".to_string(), policy.workload_ids.join(","));
        values.insert(
            "allowed_app_ids".to_string(),
            policy.allowed_app_ids.join(","),
        );
        values.insert(
            "allowed_environments".to_string(),
            policy.allowed_environments.join(","),
        );
        values.insert(
            "allowed_provider_ids".to_string(),
            policy.allowed_provider_ids.join(","),
        );
        values.insert(
            "preferred_deployment_ids".to_string(),
            policy.preferred_deployment_ids.join(","),
        );
        values.insert(
            "denied_provider_ids".to_string(),
            policy.denied_provider_ids.join(","),
        );
        values.insert(
            "denied_deployment_ids".to_string(),
            policy.denied_deployment_ids.join(","),
        );
        values.insert(
            "allowed_regions".to_string(),
            policy.allowed_regions.join(","),
        );
        values.insert(
            "required_residency".to_string(),
            policy.required_residency.clone().unwrap_or_default(),
        );
        values.insert(
            "allow_data_retention".to_string(),
            policy.allow_data_retention.to_string(),
        );
        values.insert(
            "allow_training".to_string(),
            policy.allow_training.to_string(),
        );
        values.insert(
            "max_input_cost_microusd_per_million".to_string(),
            policy.max_input_cost_microusd_per_million.to_string(),
        );
        values.insert(
            "max_output_cost_microusd_per_million".to_string(),
            policy.max_output_cost_microusd_per_million.to_string(),
        );
        values.insert(
            "grants_execution_authority".to_string(),
            "false".to_string(),
        );
        self.storage.append_receipt(
            &mut self.ids,
            correlation,
            MODEL_ROUTE_POLICY_CONFIGURED_RECEIPT_KIND,
            None,
            values,
        )
    }

    pub fn record_model_connection_configured(
        &mut self,
        correlation: &CorrelationIds,
        connection: &ProviderConnection,
    ) -> Receipt {
        let mut values = BTreeMap::new();
        values.insert(
            "connection_id".to_string(),
            connection.connection_id.clone(),
        );
        values.insert("provider_id".to_string(), connection.provider_id.clone());
        values.insert(
            "credential_ref".to_string(),
            connection.credential_ref.clone(),
        );
        values.insert(
            "endpoint_base_url".to_string(),
            connection.endpoint_base_url.clone(),
        );
        values.insert("region".to_string(), connection.region.clone());
        values.insert("residency".to_string(), connection.residency.clone());
        values.insert(
            "data_retention".to_string(),
            connection.data_retention.clone(),
        );
        values.insert(
            "training_policy".to_string(),
            connection.training_policy.clone(),
        );
        values.insert(
            "data_policy_provenance".to_string(),
            connection.data_policy_provenance.clone(),
        );
        values.insert(
            "data_policy_observed_at".to_string(),
            connection.data_policy_observed_at.clone(),
        );
        values.insert("health".to_string(), connection.health.clone());
        values.insert(
            "live_call_allowed".to_string(),
            connection.live_call_allowed.to_string(),
        );
        values.insert("secret_value_recorded".to_string(), "false".to_string());
        self.storage.append_receipt(
            &mut self.ids,
            correlation,
            MODEL_CONNECTION_CONFIGURED_RECEIPT_KIND,
            None,
            values,
        )
    }

    pub fn record_model_connection_refused(
        &mut self,
        correlation: &CorrelationIds,
        provider_id: &str,
        reason: &str,
    ) -> Receipt {
        let mut values = BTreeMap::new();
        values.insert("provider_id".to_string(), provider_id.to_string());
        values.insert("reason".to_string(), reason.to_string());
        values.insert("secret_value_recorded".to_string(), "false".to_string());
        values.insert(
            "grants_execution_authority".to_string(),
            "false".to_string(),
        );
        self.storage.append_receipt(
            &mut self.ids,
            correlation,
            MODEL_CONNECTION_REFUSED_RECEIPT_KIND,
            None,
            values,
        )
    }

    pub fn record_model_outcome_evaluation_refused(
        &mut self,
        correlation: &CorrelationIds,
        source_outcome_id: &str,
        reason: &str,
    ) -> Receipt {
        let mut values = BTreeMap::new();
        values.insert(
            "source_outcome_id".to_string(),
            source_outcome_id.to_string(),
        );
        values.insert("reason".to_string(), reason.to_string());
        values.insert(
            "grants_execution_authority".to_string(),
            "false".to_string(),
        );
        self.storage.append_receipt(
            &mut self.ids,
            correlation,
            MODEL_OUTCOME_EVALUATION_REFUSED_RECEIPT_KIND,
            None,
            values,
        )
    }

    pub fn record_model_adaptive_policy_refused(
        &mut self,
        correlation: &CorrelationIds,
        policy_id: &str,
        policy_version: &str,
        reason: &str,
    ) -> Receipt {
        let mut values = BTreeMap::new();
        values.insert("policy_id".to_string(), policy_id.to_string());
        values.insert("policy_version".to_string(), policy_version.to_string());
        values.insert("reason".to_string(), reason.to_string());
        values.insert(
            "grants_execution_authority".to_string(),
            "false".to_string(),
        );
        self.storage.append_receipt(
            &mut self.ids,
            correlation,
            MODEL_ADAPTIVE_POLICY_REFUSED_RECEIPT_KIND,
            None,
            values,
        )
    }

    pub fn record_model_adaptive_policy_promoted(
        &mut self,
        correlation: &CorrelationIds,
        policy_id: &str,
        policy_version: &str,
        evaluation_receipt_id: &str,
    ) -> Result<Receipt, &'static str> {
        if policy_id.trim().is_empty()
            || policy_version.trim().is_empty()
            || evaluation_receipt_id.trim().is_empty()
        {
            return Err("adaptive promotion requires policy and evaluation identity");
        }
        let mut values = BTreeMap::new();
        values.insert("policy_id".to_string(), policy_id.to_string());
        values.insert("policy_version".to_string(), policy_version.to_string());
        values.insert("previous_state".to_string(), "canary".to_string());
        values.insert("state".to_string(), "promoted".to_string());
        values.insert(
            "evaluation_receipt_id".to_string(),
            evaluation_receipt_id.to_string(),
        );
        values.insert(
            "approval_mode".to_string(),
            "explicit_operator_action".to_string(),
        );
        values.insert(
            "grants_execution_authority".to_string(),
            "false".to_string(),
        );
        Ok(self.storage.append_receipt(
            &mut self.ids,
            correlation,
            MODEL_ADAPTIVE_POLICY_PROMOTED_RECEIPT_KIND,
            None,
            values,
        ))
    }

    pub fn record_model_adaptive_policy_auto_rollback(
        &mut self,
        correlation: &CorrelationIds,
        policy_id: &str,
        policy_version: &str,
        trigger_outcome_id: &str,
        reason: &str,
    ) -> Result<Receipt, &'static str> {
        if policy_id.trim().is_empty()
            || policy_version.trim().is_empty()
            || trigger_outcome_id.trim().is_empty()
            || reason.trim().is_empty()
        {
            return Err("adaptive rollback requires policy, outcome, and reason");
        }
        let mut values = BTreeMap::new();
        values.insert("policy_id".to_string(), policy_id.to_string());
        values.insert("policy_version".to_string(), policy_version.to_string());
        values.insert("previous_state".to_string(), "canary".to_string());
        values.insert("state".to_string(), "rollback_required".to_string());
        values.insert(
            "trigger_outcome_id".to_string(),
            trigger_outcome_id.to_string(),
        );
        values.insert("reason".to_string(), reason.to_string());
        values.insert("trigger".to_string(), "evaluated_outcome".to_string());
        values.insert(
            "grants_execution_authority".to_string(),
            "false".to_string(),
        );
        Ok(self.storage.append_receipt(
            &mut self.ids,
            correlation,
            MODEL_ADAPTIVE_POLICY_AUTO_ROLLBACK_RECEIPT_KIND,
            None,
            values,
        ))
    }

    pub fn record_model_circuit_transition(
        &mut self,
        correlation: &CorrelationIds,
        connection_id: &str,
        deployment_id: &str,
        state: &str,
        consecutive_failures: u32,
        open_until_epoch_seconds: u64,
    ) -> Result<Receipt, &'static str> {
        if connection_id.trim().is_empty()
            || deployment_id.trim().is_empty()
            || !matches!(state, "open" | "closed")
        {
            return Err("model circuit transition is invalid");
        }
        let mut values = BTreeMap::new();
        values.insert("connection_id".to_string(), connection_id.to_string());
        values.insert("deployment_id".to_string(), deployment_id.to_string());
        values.insert("state".to_string(), state.to_string());
        values.insert(
            "consecutive_failures".to_string(),
            consecutive_failures.to_string(),
        );
        values.insert(
            "open_until_epoch_seconds".to_string(),
            open_until_epoch_seconds.to_string(),
        );
        values.insert(
            "grants_execution_authority".to_string(),
            "false".to_string(),
        );
        Ok(self.storage.append_receipt(
            &mut self.ids,
            correlation,
            MODEL_CIRCUIT_TRANSITION_RECEIPT_KIND,
            None,
            values,
        ))
    }

    pub fn record_model_deployment_registered(
        &mut self,
        correlation: &CorrelationIds,
        model: &Model,
        deployment: &ModelDeployment,
        price: &ModelPriceObservation,
    ) -> Receipt {
        let mut values = BTreeMap::new();
        values.insert(
            "deployment_id".to_string(),
            deployment.deployment_id.clone(),
        );
        values.insert(
            "connection_id".to_string(),
            deployment.connection_id.clone(),
        );
        values.insert("provider_id".to_string(), deployment.provider_id.clone());
        values.insert("model_id".to_string(), model.model_id.clone());
        values.insert(
            "provider_model_id".to_string(),
            model.provider_model_id.clone(),
        );
        values.insert("display_name".to_string(), model.display_name.clone());
        values.insert("lifecycle".to_string(), model.lifecycle.clone());
        values.insert("modality".to_string(), model.modality.clone());
        values.insert(
            "privacy_class".to_string(),
            deployment.privacy_class.clone(),
        );
        values.insert("region".to_string(), deployment.region.clone());
        values.insert("residency".to_string(), deployment.residency.clone());
        values.insert("enabled".to_string(), deployment.enabled.to_string());
        values.insert(
            "capability_provenance".to_string(),
            deployment.capabilities.provenance.clone(),
        );
        values.insert(
            "capability_observed_at".to_string(),
            deployment.capabilities.observed_at.clone(),
        );
        values.insert(
            "tools".to_string(),
            deployment.capabilities.tools.to_string(),
        );
        values.insert(
            "structured_output".to_string(),
            deployment.capabilities.structured_output.to_string(),
        );
        values.insert(
            "vision".to_string(),
            deployment.capabilities.vision.to_string(),
        );
        values.insert(
            "context_tokens".to_string(),
            deployment.capabilities.context_tokens.to_string(),
        );
        values.insert(
            "input_microusd_per_million".to_string(),
            price.input_microusd_per_million.to_string(),
        );
        values.insert(
            "output_microusd_per_million".to_string(),
            price.output_microusd_per_million.to_string(),
        );
        values.insert("currency".to_string(), price.currency.clone());
        values.insert("price_source".to_string(), price.source.clone());
        values.insert("price_observed_at".to_string(), price.observed_at.clone());
        values.insert(
            "grants_execution_authority".to_string(),
            "false".to_string(),
        );
        self.storage.append_receipt(
            &mut self.ids,
            correlation,
            MODEL_DEPLOYMENT_REGISTERED_RECEIPT_KIND,
            None,
            values,
        )
    }

    pub fn record_model_route_decision(
        &mut self,
        correlation: &CorrelationIds,
        decision: &ModelRouteDecision,
    ) -> Receipt {
        let mut values = BTreeMap::new();
        values.insert("workload_id".to_string(), decision.workload_id.clone());
        values.insert("app_id".to_string(), decision.app_id.clone());
        values.insert("environment".to_string(), decision.environment.clone());
        values.insert("session_id".to_string(), decision.session_id.clone());
        values.insert("preset".to_string(), decision.preset.clone());
        values.insert("policy_id".to_string(), decision.policy_id.clone());
        values.insert(
            "policy_version".to_string(),
            decision.policy_version.clone(),
        );
        values.insert(
            "selected_deployment_id".to_string(),
            decision.selected_deployment_id.clone().unwrap_or_default(),
        );
        values.insert(
            "selected_provider_id".to_string(),
            decision.selected_provider_id.clone().unwrap_or_default(),
        );
        values.insert(
            "selected_model_id".to_string(),
            decision.selected_model_id.clone().unwrap_or_default(),
        );
        values.insert(
            "selection_reason".to_string(),
            decision.selection_reason.to_string(),
        );
        values.insert(
            "session_sticky_applied".to_string(),
            decision.session_sticky_applied.to_string(),
        );
        values.insert(
            "provider_failover_deployment_ids".to_string(),
            decision.provider_failover_deployment_ids.join(","),
        );
        values.insert(
            "model_fallback_deployment_ids".to_string(),
            decision.model_fallback_deployment_ids.join(","),
        );
        values.insert(
            "fallback_deployment_ids".to_string(),
            decision.fallback_deployment_ids.join(","),
        );
        values.insert(
            "exclusion_count".to_string(),
            decision.exclusions.len().to_string(),
        );
        values.insert(
            "exclusions".to_string(),
            decision
                .exclusions
                .iter()
                .map(|row| format!("{}:{}", row.deployment_id, row.reason))
                .collect::<Vec<_>>()
                .join(","),
        );
        values.insert(
            "grants_execution_authority".to_string(),
            "false".to_string(),
        );
        self.storage.append_receipt(
            &mut self.ids,
            correlation,
            if decision.status == "ROUTE_SELECTED" {
                MODEL_ROUTE_SELECTED_RECEIPT_KIND
            } else {
                MODEL_ROUTE_DENIED_RECEIPT_KIND
            },
            None,
            values,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActorId, LoopId, TenantId, TraceId, WorkflowId};
    fn row<'a>(
        id: &'a str,
        provider: &'a str,
        privacy: &'a str,
        quality: u32,
        latency: u64,
        cost: u64,
    ) -> ModelRouteCandidate<'a> {
        ModelRouteCandidate {
            deployment_id: id,
            connection_id: provider,
            tenant_id: "local_tenant",
            provider_id: provider,
            model_id: id,
            privacy_class: privacy,
            region: "us-east",
            residency: "us",
            data_retention: "none",
            training_policy: "none",
            connected: true,
            healthy: true,
            live_call_allowed: true,
            tools: true,
            structured_output: true,
            vision: true,
            context_tokens: 200_000,
            input_cost_microusd_per_million: cost / 2,
            output_cost_microusd_per_million: cost,
            price_known: true,
            p95_latency_ms: latency,
            latency_known: true,
            quality_score: quality,
            quality_known: true,
        }
    }

    #[test]
    fn routes_by_objective_and_respects_pin() {
        let policy = RoutePolicy::core("local_tenant");
        let rows = [
            row("strong", "anthropic", "frontier", 98, 900, 10_000),
            row("fast", "openai", "frontier", 88, 120, 1_000),
        ];
        let base = ModelRouteRequest {
            workload_id: "mdx/forge/builder",
            preset: "balanced",
            policy: &policy,
            expected_policy_version: None,
            context: ModelPolicyContext {
                tenant_id: "local_tenant",
                app_id: "forge",
                environment: "local",
                required_region: None,
                required_residency: None,
                session_id: None,
                prior_deployment_id: None,
            },
            required_context_tokens: 100_000,
            max_input_cost_microusd_per_million: 0,
            max_output_cost_microusd_per_million: 0,
            pinned_deployment_id: None,
            candidates: &rows,
        };
        assert_eq!(
            resolve_model_route(&base).selected_deployment_id.as_deref(),
            Some("strong")
        );
        assert_eq!(
            resolve_model_route(&ModelRouteRequest {
                preset: "fast",
                ..base.clone()
            })
            .selected_deployment_id
            .as_deref(),
            Some("fast")
        );
        assert_eq!(
            resolve_model_route(&ModelRouteRequest {
                pinned_deployment_id: Some("fast"),
                ..base
            })
            .selection_reason,
            "explicit_pin"
        );
    }

    #[test]
    fn unknown_metrics_never_win_objective_tie_breaks_as_zero() {
        let policy = RoutePolicy::core("local_tenant");
        let known = row("known", "openai", "frontier", 90, 200, 1_000);
        let mut unknown = row("unknown", "anthropic", "frontier", 90, 0, 0);
        unknown.latency_known = false;
        unknown.price_known = false;
        let rows = [unknown, known];
        let request = ModelRouteRequest {
            workload_id: "mdx/twin/conversation",
            preset: "quality",
            policy: &policy,
            expected_policy_version: None,
            context: ModelPolicyContext {
                tenant_id: "local_tenant",
                app_id: "twin",
                environment: "local",
                required_region: None,
                required_residency: None,
                session_id: None,
                prior_deployment_id: None,
            },
            required_context_tokens: 1,
            max_input_cost_microusd_per_million: 0,
            max_output_cost_microusd_per_million: 0,
            pinned_deployment_id: None,
            candidates: &rows,
        };
        assert_eq!(
            resolve_model_route(&request)
                .selected_deployment_id
                .as_deref(),
            Some("known")
        );
        assert_eq!(
            resolve_model_route(&ModelRouteRequest {
                preset: "balanced",
                ..request.clone()
            })
            .selected_deployment_id
            .as_deref(),
            Some("known")
        );

        let mut unknown_quality = row("unknown-quality", "anthropic", "frontier", 0, 200, 0);
        unknown_quality.quality_known = false;
        unknown_quality.price_known = false;
        let fast_rows = [
            unknown_quality,
            row("known-fast", "openai", "frontier", 90, 200, 1_000),
        ];
        assert_eq!(
            resolve_model_route(&ModelRouteRequest {
                preset: "fast",
                candidates: &fast_rows,
                ..request
            })
            .selected_deployment_id
            .as_deref(),
            Some("known-fast")
        );
    }

    #[test]
    fn legacy_provider_names_resolve_to_canonical_ids() {
        assert_eq!(canonical_model_provider_id("gemini"), Some("google"));
        assert_eq!(canonical_model_provider_id("bedrock"), Some("aws_bedrock"));
        assert_eq!(
            canonical_model_provider_id("azure_openai"),
            Some("azure_foundry")
        );
        assert_eq!(canonical_model_provider_id("moonshot"), Some("moonshot"));
    }

    #[test]
    fn privacy_and_unknown_provider_fail_closed() {
        let policy = RoutePolicy::core("local_tenant");
        let rows = [
            row("frontier", "openai", "frontier", 99, 100, 100),
            row("unknown", "mystery", "local", 100, 1, 1),
        ];
        let decision = resolve_model_route(&ModelRouteRequest {
            workload_id: "mdx/twin/private-reasoning",
            preset: "private",
            policy: &policy,
            expected_policy_version: None,
            context: ModelPolicyContext {
                tenant_id: "local_tenant",
                app_id: "twin",
                environment: "local",
                required_region: None,
                required_residency: None,
                session_id: None,
                prior_deployment_id: None,
            },
            required_context_tokens: 1,
            max_input_cost_microusd_per_million: 0,
            max_output_cost_microusd_per_million: 0,
            pinned_deployment_id: None,
            candidates: &rows,
        });
        assert_eq!(decision.status, "ROUTE_DENIED");
        assert!(
            decision
                .exclusions
                .iter()
                .any(|row| row.reason == "privacy_class_not_allowed")
        );
        assert!(
            decision
                .exclusions
                .iter()
                .any(|row| row.reason == "unknown_provider")
        );
    }

    #[test]
    fn unknown_provider_data_terms_fail_closed_even_when_retention_is_allowed() {
        let policy = RoutePolicy::core("local_tenant");
        let mut unknown_retention =
            row("unknown-retention", "openrouter", "frontier", 90, 100, 100);
        unknown_retention.data_retention = "unknown";
        let mut unknown_training = row("unknown-training", "vercel", "frontier", 90, 100, 100);
        unknown_training.training_policy = "unknown";
        let rows = [unknown_retention, unknown_training];
        let decision = resolve_model_route(&ModelRouteRequest {
            workload_id: "mdx/pages/summarize",
            preset: "balanced",
            policy: &policy,
            expected_policy_version: None,
            context: ModelPolicyContext {
                tenant_id: "local_tenant",
                app_id: "pages",
                environment: "local",
                required_region: None,
                required_residency: None,
                session_id: None,
                prior_deployment_id: None,
            },
            required_context_tokens: 1,
            max_input_cost_microusd_per_million: 0,
            max_output_cost_microusd_per_million: 0,
            pinned_deployment_id: None,
            candidates: &rows,
        });
        assert_eq!(decision.status, "ROUTE_DENIED");
        assert!(
            decision
                .exclusions
                .iter()
                .any(|row| row.reason == "data_retention_unknown")
        );
        assert!(
            decision
                .exclusions
                .iter()
                .any(|row| row.reason == "training_policy_unknown")
        );
    }

    #[test]
    fn workload_alias_cannot_be_relabelled_as_another_app() {
        let policy = RoutePolicy::core("local_tenant");
        let rows = [row("candidate", "openai", "frontier", 90, 100, 100)];
        let decision = resolve_model_route(&ModelRouteRequest {
            workload_id: "mdx/forge/builder",
            preset: "balanced",
            policy: &policy,
            expected_policy_version: None,
            context: ModelPolicyContext {
                tenant_id: "local_tenant",
                app_id: "twin",
                environment: "local",
                required_region: None,
                required_residency: None,
                session_id: None,
                prior_deployment_id: None,
            },
            required_context_tokens: 1,
            max_input_cost_microusd_per_million: 0,
            max_output_cost_microusd_per_million: 0,
            pinned_deployment_id: None,
            candidates: &rows,
        });
        assert_eq!(decision.status, "ROUTE_DENIED");
        assert_eq!(decision.selection_reason, "workload_app_mismatch");
    }

    #[test]
    fn policy_context_region_residency_and_quality_fail_closed() {
        let mut policy = RoutePolicy::core("tenant-a");
        policy.allowed_regions = vec!["ca-central".to_string()];
        policy.required_residency = Some("ca".to_string());
        let mut candidate = row("frontier", "openai", "frontier", 101, 100, 100);
        candidate.tenant_id = "tenant-a";
        candidate.region = "us-east";
        candidate.residency = "us";
        let rows = [candidate];
        let decision = resolve_model_route(&ModelRouteRequest {
            workload_id: "mdx/twin/conversation",
            preset: "balanced",
            policy: &policy,
            expected_policy_version: None,
            context: ModelPolicyContext {
                tenant_id: "tenant-a",
                app_id: "twin",
                environment: "production",
                required_region: Some("ca-central"),
                required_residency: Some("ca"),
                session_id: None,
                prior_deployment_id: None,
            },
            required_context_tokens: 1,
            max_input_cost_microusd_per_million: 0,
            max_output_cost_microusd_per_million: 0,
            pinned_deployment_id: None,
            candidates: &rows,
        });
        assert_eq!(decision.status, "ROUTE_DENIED");
        assert_eq!(
            decision.exclusions[0].reason,
            "required_region_not_available"
        );
    }

    #[test]
    fn sticky_selection_and_fallback_classes_are_explicit() {
        let mut policy = RoutePolicy::core("local_tenant");
        policy.preferred_deployment_ids = vec!["primary-a".to_string()];
        let mut primary = row("primary-a", "anthropic", "frontier", 98, 100, 1_000);
        primary.model_id = "shared-model";
        let mut provider_failover = row("primary-b", "aws_bedrock", "frontier", 95, 200, 900);
        provider_failover.model_id = "shared-model";
        let alternative = row("alternative", "openai", "frontier", 99, 80, 500);
        let rows = [primary, provider_failover, alternative];
        let decision = resolve_model_route(&ModelRouteRequest {
            workload_id: "mdx/forge/builder",
            preset: "quality",
            policy: &policy,
            expected_policy_version: None,
            context: ModelPolicyContext {
                tenant_id: "local_tenant",
                app_id: "forge",
                environment: "local",
                required_region: None,
                required_residency: None,
                session_id: Some("build-1"),
                prior_deployment_id: Some("primary-b"),
            },
            required_context_tokens: 1,
            max_input_cost_microusd_per_million: 0,
            max_output_cost_microusd_per_million: 0,
            pinned_deployment_id: None,
            candidates: &rows,
        });
        assert_eq!(
            decision.selected_deployment_id.as_deref(),
            Some("primary-b")
        );
        assert!(decision.session_sticky_applied);
        assert_eq!(decision.selection_reason, "eligible_session_sticky");
        assert_eq!(decision.provider_failover_deployment_ids, ["primary-a"]);
        assert_eq!(decision.model_fallback_deployment_ids, ["alternative"]);
        assert_eq!(
            decision.fallback_deployment_ids,
            ["primary-a", "alternative"]
        );
        let preferred = resolve_model_route(&ModelRouteRequest {
            context: ModelPolicyContext {
                tenant_id: "local_tenant",
                app_id: "forge",
                environment: "local",
                required_region: None,
                required_residency: None,
                session_id: None,
                prior_deployment_id: None,
            },
            workload_id: "mdx/forge/builder",
            preset: "quality",
            policy: &policy,
            expected_policy_version: None,
            required_context_tokens: 1,
            max_input_cost_microusd_per_million: 0,
            max_output_cost_microusd_per_million: 0,
            pinned_deployment_id: None,
            candidates: &rows,
        });
        assert_eq!(
            preferred.selected_deployment_id.as_deref(),
            Some("primary-a")
        );
        assert_eq!(preferred.selection_reason, "policy_deployment_preference");
    }

    #[test]
    fn adaptive_policy_requires_replay_shadow_canary_and_all_guardrails() {
        let evidence = AdaptivePolicyEvidence {
            policy_id: "candidate".to_string(),
            policy_version: "candidate-1".to_string(),
            state: "draft".to_string(),
            replay_cases: 100,
            shadow_decisions: 0,
            canary_decisions: 0,
            guardrail_evidence_sufficient: true,
            quality_guardrail_passed: true,
            safety_guardrail_passed: true,
            latency_guardrail_passed: true,
            cost_guardrail_passed: true,
        };
        assert_eq!(next_adaptive_policy_state(&evidence), "shadow");
        assert_eq!(
            next_adaptive_policy_state(&AdaptivePolicyEvidence {
                state: "shadow".to_string(),
                shadow_decisions: 500,
                ..evidence.clone()
            }),
            "canary"
        );
        assert_eq!(
            next_adaptive_policy_state(&AdaptivePolicyEvidence {
                state: "canary".to_string(),
                canary_decisions: 100,
                ..evidence.clone()
            }),
            "promoted"
        );
        assert_eq!(
            next_adaptive_policy_state(&AdaptivePolicyEvidence {
                safety_guardrail_passed: false,
                ..evidence
            }),
            "rollback_required"
        );
    }

    #[test]
    fn canary_evaluation_recommends_but_does_not_apply_promotion() {
        let mut kernel = MdxKernel::boot_local();
        let evidence = AdaptivePolicyEvidence {
            policy_id: "forge-default".to_string(),
            policy_version: "v2".to_string(),
            state: "canary".to_string(),
            replay_cases: 100,
            shadow_decisions: 500,
            canary_decisions: 100,
            guardrail_evidence_sufficient: true,
            quality_guardrail_passed: true,
            safety_guardrail_passed: true,
            latency_guardrail_passed: true,
            cost_guardrail_passed: true,
        };
        let receipt = kernel
            .record_adaptive_policy_evaluation(
                &CorrelationIds {
                    tenant_id: TenantId::new("local_tenant"),
                    trace_id: TraceId::new("trace_adaptive_evaluation"),
                    actor_id: ActorId::new("human:operator"),
                    loop_id: LoopId::new("model_fabric"),
                    workflow_id: WorkflowId::new("workflow_adaptive_evaluation"),
                },
                &evidence,
            )
            .expect("evaluation receipt");
        assert_eq!(receipt.payload["state"], "canary");
        assert_eq!(receipt.payload["recommended_transition"], "promoted");
        assert_eq!(receipt.payload["grants_execution_authority"], "false");
    }

    #[test]
    fn route_decision_receipt_is_append_only_and_grants_no_authority() {
        let policy = RoutePolicy::core("local_tenant");
        let rows = [row("local", "ollama", "local", 80, 20, 0)];
        let decision = resolve_model_route(&ModelRouteRequest {
            workload_id: "mdx/twin/private-reasoning",
            preset: "private",
            policy: &policy,
            expected_policy_version: None,
            context: ModelPolicyContext {
                tenant_id: "local_tenant",
                app_id: "twin",
                environment: "local",
                required_region: None,
                required_residency: None,
                session_id: Some("session-1"),
                prior_deployment_id: Some("local"),
            },
            required_context_tokens: 1,
            max_input_cost_microusd_per_million: 0,
            max_output_cost_microusd_per_million: 0,
            pinned_deployment_id: None,
            candidates: &rows,
        });
        let mut kernel = MdxKernel::boot_local();
        let receipt = kernel.record_model_route_decision(
            &CorrelationIds {
                tenant_id: TenantId::new("local_tenant"),
                trace_id: TraceId::new("trace_model_route"),
                actor_id: ActorId::new("human:local_user"),
                loop_id: LoopId::new("model_fabric"),
                workflow_id: WorkflowId::new("workflow_model_route"),
            },
            &decision,
        );
        assert_eq!(receipt.kind, MODEL_ROUTE_SELECTED_RECEIPT_KIND);
        assert_eq!(receipt.payload["grants_execution_authority"], "false");
        assert_eq!(kernel.ledger().entries().last(), Some(&receipt));
    }

    #[test]
    fn unknown_price_cannot_satisfy_a_cost_ceiling() {
        let policy = RoutePolicy::core("local_tenant");
        let mut unknown = row("unknown-price", "openai", "frontier", 90, 100, 0);
        unknown.price_known = false;
        let rows = [unknown];
        let decision = resolve_model_route(&ModelRouteRequest {
            workload_id: "mdx/twin/conversation",
            preset: "balanced",
            policy: &policy,
            expected_policy_version: None,
            context: ModelPolicyContext {
                tenant_id: "local_tenant",
                app_id: "twin",
                environment: "local",
                required_region: None,
                required_residency: None,
                session_id: None,
                prior_deployment_id: None,
            },
            required_context_tokens: 1,
            max_input_cost_microusd_per_million: 1,
            max_output_cost_microusd_per_million: 1,
            pinned_deployment_id: None,
            candidates: &rows,
        });
        assert_eq!(decision.status, "ROUTE_DENIED");
        assert_eq!(decision.exclusions[0].reason, "price_provenance_required");
    }

    #[test]
    fn runtime_outcome_keeps_unevaluated_judgments_explicit() {
        let mut kernel = MdxKernel::boot_local();
        let correlation = CorrelationIds {
            tenant_id: TenantId::new("local_tenant"),
            trace_id: TraceId::new("trace_model_outcome"),
            actor_id: ActorId::new("human:local_user"),
            loop_id: LoopId::new("model_fabric"),
            workflow_id: WorkflowId::new("workflow_model_outcome"),
        };
        let receipt = kernel
            .record_model_outcome(
                &correlation,
                &ModelOutcome {
                    decision_id: "receipt_route_1".to_string(),
                    workload_id: "mdx/twin/conversation".to_string(),
                    app_id: "twin".to_string(),
                    environment: "local".to_string(),
                    session_id: "session-1".to_string(),
                    deployment_id: "deployment-1".to_string(),
                    latency_ms: 120,
                    cost_microusd: Some(3),
                    quality_score: None,
                    safety_status: "not_evaluated".to_string(),
                    task_status: "not_evaluated".to_string(),
                    correction_status: "not_observed".to_string(),
                    provenance: "provider_runtime:inference-1".to_string(),
                },
            )
            .expect("outcome receipt");
        assert_eq!(receipt.payload["quality_score"], "");
        assert_eq!(receipt.payload["safety_status"], "not_evaluated");
        assert_eq!(receipt.payload["grants_execution_authority"], "false");
    }
}
