use serde_json::{Value, json};

const PROVIDER_STATUS: &str = "LIVE-LOCAL-CTX-PROVIDER-TURN-ON-FLOOR";

#[derive(Default)]
pub struct CtxProviderRuntime {
    preflights: Vec<CtxProviderPreflight>,
    observations: Vec<CtxProviderObservation>,
    next_preflight: usize,
    next_observation: usize,
}

#[derive(Clone)]
struct CtxProviderPreflight {
    preflight_id: String,
    tenant_id: String,
    actor_id: String,
    provider_id: String,
    adapter: String,
    required_receipt_kind: String,
    approval_receipt_id: String,
    status: String,
}

#[derive(Clone)]
struct CtxProviderObservation {
    observation_id: String,
    tenant_id: String,
    actor_id: String,
    provider_id: String,
    adapter: String,
    required_receipt_kind: String,
    approval_receipt_id: String,
    observed_receipt_id: String,
    evidence_file: String,
    model_id: String,
    status: String,
    observed: bool,
    vector_dimensions: usize,
    total_tokens: usize,
}

struct ProviderSlot {
    id: &'static str,
    substrate: &'static str,
    adapter: &'static str,
    profile_kind: &'static str,
    env_var: &'static str,
    required_receipt_kind: &'static str,
    turn_on_signal: &'static str,
    local_fallback: &'static str,
    v1_floor: &'static str,
}

impl CtxProviderRuntime {
    pub fn status(&self) -> &'static str {
        PROVIDER_STATUS
    }

    pub fn slots_json(&self) -> String {
        json!({
            "name": "mdx-ctx-provider-turn-on-floor",
            "status": PROVIDER_STATUS,
            "runtime": "mdx-ctx-engine",
            "route": "/ctx/provider-slots.json",
            "preflight_route": "/v1/ctx/provider-turn-on-preflights",
            "observation_route": "/v1/ctx/provider-observations",
            "observations_route": "/ctx/provider-observations.json",
            "provider_slot_count": provider_slots().len(),
            "preflight_count": self.preflights.len(),
            "observation_count": self.observations.len(),
            "observed_provider_count": self.observed_provider_count(),
            "ready_for_live_call_count": self.observed_provider_count(),
            "provider_call_allowed_now": self.observed_provider_count() > 0,
            "live_network_allowed_now": self.observed_provider_count() > 0,
            "provider_connected_local_allowed": self.observed_provider_count() > 0,
            "credential_presence_only": true,
            "credential_values_recorded": false,
            "provider_secret_values_recorded": false,
            "production_writes_allowed": false,
            "provider_slots": provider_slots().iter().map(slot_value).collect::<Vec<_>>(),
            "preflights": self.preflights.iter().map(preflight_value).collect::<Vec<_>>(),
            "observations": self.observations.iter().map(observation_value).collect::<Vec<_>>()
        })
        .to_string()
    }

    pub fn preflights_json(&self) -> String {
        json!({
            "name": "mdx-ctx-provider-turn-on-preflights",
            "status": "CTX_PROVIDER_TURN_ON_PREFLIGHTS_RECORDED",
            "runtime": "mdx-ctx-engine",
            "preflight_count": self.preflights.len(),
            "provider_call_allowed_now": false,
            "live_network_allowed_now": false,
            "credential_values_recorded": false,
            "production_writes_allowed": false,
            "preflights": self.preflights.iter().map(preflight_value).collect::<Vec<_>>()
        })
        .to_string()
    }

    pub fn observations_json(&self) -> String {
        json!({
            "name": "mdx-ctx-provider-observations",
            "status": if self.observed_provider_count() > 0 { "CTX_PROVIDER_TURN_ON_OBSERVATIONS_RECORDED" } else { "CTX_PROVIDER_TURN_ON_OBSERVATIONS_EMPTY" },
            "runtime": "mdx-ctx-engine",
            "observation_count": self.observations.len(),
            "observed_provider_count": self.observed_provider_count(),
            "provider_connected_local_allowed": self.observed_provider_count() > 0,
            "provider_call_allowed_now": self.observed_provider_count() > 0,
            "live_network_allowed_now": self.observed_provider_count() > 0,
            "credential_values_recorded": false,
            "provider_secret_values_recorded": false,
            "production_writes_allowed": false,
            "observations": self.observations.iter().map(observation_value).collect::<Vec<_>>()
        })
        .to_string()
    }

    pub fn preflight_json(&mut self, body: &str) -> Result<String, String> {
        let value = serde_json::from_str::<Value>(body).map_err(|error| error.to_string())?;
        let provider_id = string_value(&value, "provider_id", "openai_embeddings");
        let Some(slot) = provider_slots()
            .into_iter()
            .find(|slot| slot.id == provider_id)
        else {
            return Err(format!("unknown ctx provider slot {provider_id}"));
        };
        self.next_preflight += 1;
        let approval_receipt_id = string_value(
            &value,
            "approval_receipt_id",
            &format!("ctx_provider_preflight_approval_{:06}", self.next_preflight),
        );
        let preflight = CtxProviderPreflight {
            preflight_id: format!("ctx_provider_preflight_{:06}", self.next_preflight),
            tenant_id: string_value(&value, "tenant_id", "tenant_local"),
            actor_id: string_value(&value, "actor_id", "local_user"),
            provider_id: slot.id.to_string(),
            adapter: slot.adapter.to_string(),
            required_receipt_kind: slot.required_receipt_kind.to_string(),
            approval_receipt_id,
            status: if env_present(slot.env_var) {
                "CTX_PROVIDER_ENV_READY_TURN_ON_RECEIPT_REQUIRED".to_string()
            } else {
                "CTX_PROVIDER_ENV_MISSING_TURN_ON_RECEIPT_REQUIRED".to_string()
            },
        };
        self.preflights.push(preflight.clone());
        Ok(json!({
            "name": "mdx-ctx-provider-turn-on-preflight",
            "status": preflight.status,
            "runtime": "mdx-ctx-engine",
            "preflight": preflight_value(&preflight),
            "provider_slot": slot_value(&slot),
            "provider_call_allowed_now": false,
            "live_network_allowed_now": false,
            "credential_presence_only": true,
            "credential_values_recorded": false,
            "provider_secret_values_recorded": false,
            "turn_on_receipt_required": true,
            "production_writes_allowed": false
        })
        .to_string())
    }

    pub fn observation_json(&mut self, body: &str) -> Result<String, String> {
        let value = serde_json::from_str::<Value>(body).map_err(|error| error.to_string())?;
        let provider_id = string_value(&value, "provider_id", "openai_embeddings");
        let Some(slot) = provider_slots()
            .into_iter()
            .find(|slot| slot.id == provider_id)
        else {
            return Err(format!("unknown ctx provider slot {provider_id}"));
        };

        self.next_observation += 1;
        let observed = bool_value(&value, "observed", false);
        let credential_values_recorded = bool_value(&value, "credential_values_recorded", false);
        let provider_secret_values_recorded =
            bool_value(&value, "provider_secret_values_recorded", false);
        let requested_secret_values_recorded =
            bool_value(&value, "requested_secret_values_recorded", false);
        let production_writes_allowed = bool_value(&value, "production_writes_allowed", false);
        let receipt_kind = string_value(&value, "receipt_kind", "");
        let approval_receipt_id = string_value(&value, "approval_receipt_id", "");
        let observed_receipt_id = string_value(
            &value,
            "observed_receipt_id",
            &format!("ctx_provider_observed_receipt_{:06}", self.next_observation),
        );
        let vector_dimensions = usize_value(&value, "vector_dimensions", 0);
        let embedding_values_recorded = bool_value(&value, "embedding_values_recorded", false);
        let output_text_recorded = bool_value(&value, "output_text_recorded", false);
        let total_tokens = usize_value(&value, "total_tokens", 0);
        let safe_secret_posture = !credential_values_recorded
            && !provider_secret_values_recorded
            && !requested_secret_values_recorded;
        let valid_openai_embedding = slot.id != "openai_embeddings"
            || (vector_dimensions > 0 && !embedding_values_recorded && !output_text_recorded);
        let accepted = observed
            && receipt_kind == slot.required_receipt_kind
            && !approval_receipt_id.trim().is_empty()
            && safe_secret_posture
            && !production_writes_allowed
            && valid_openai_embedding;
        let observation = CtxProviderObservation {
            observation_id: format!("ctx_provider_observation_{:06}", self.next_observation),
            tenant_id: string_value(&value, "tenant_id", "tenant_local"),
            actor_id: string_value(&value, "actor_id", "local_user"),
            provider_id: slot.id.to_string(),
            adapter: slot.adapter.to_string(),
            required_receipt_kind: slot.required_receipt_kind.to_string(),
            approval_receipt_id,
            observed_receipt_id,
            evidence_file: string_value(&value, "evidence_file", ""),
            model_id: string_value(&value, "model_id", ""),
            status: if accepted {
                "CTX_PROVIDER_TURN_ON_OBSERVED"
            } else {
                "CTX_PROVIDER_TURN_ON_OBSERVATION_REJECTED"
            }
            .to_string(),
            observed: accepted,
            vector_dimensions,
            total_tokens,
        };
        self.observations.push(observation.clone());
        Ok(json!({
            "name": "mdx-ctx-provider-observation",
            "status": observation.status,
            "runtime": "mdx-ctx-engine",
            "observation": observation_value(&observation),
            "provider_slot": slot_value(&slot),
            "provider_connected_local_allowed": accepted,
            "provider_call_allowed_now": accepted,
            "live_network_allowed_now": accepted,
            "credential_values_recorded": false,
            "provider_secret_values_recorded": false,
            "requested_secret_values_recorded": false,
            "embedding_values_recorded": false,
            "output_text_recorded": false,
            "production_writes_allowed": false
        })
        .to_string())
    }

    fn observed_provider_count(&self) -> usize {
        self.observations
            .iter()
            .filter(|observation| observation.observed)
            .map(|observation| observation.provider_id.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    }
}

fn provider_slots() -> Vec<ProviderSlot> {
    vec![
        ProviderSlot {
            id: "openai_embeddings",
            substrate: "openai",
            adapter: "OpenAIEmbeddingsProvider",
            profile_kind: "embedding_provider",
            env_var: "OPENAI_API_KEY",
            required_receipt_kind: "openai.embedding.observed",
            turn_on_signal: "live embedding vector observed for CTX memory or signal ingest",
            local_fallback: "deterministic_local_embedding",
            v1_floor: "text-embedding-3-small with pgvector semantic memory and signal search",
        },
        ProviderSlot {
            id: "mem0_memory",
            substrate: "mem0",
            adapter: "Mem0MemoryProvider",
            profile_kind: "vendor_memory",
            env_var: "MEM0_API_KEY",
            required_receipt_kind: "mem0.memory.write.observed",
            turn_on_signal: "live memory write observed through the consolidation gate",
            local_fallback: "local_memory_store",
            v1_floor: "vendor memory slot beyond v1 local memory tiers",
        },
        ProviderSlot {
            id: "grok_research",
            substrate: "xai",
            adapter: "GrokResearchProvider",
            profile_kind: "research_provider",
            env_var: "XAI_API_KEY",
            required_receipt_kind: "ctx.grok_research.observed",
            turn_on_signal: "live research result observed for focus intake",
            local_fallback: "local_focus_watchlist",
            v1_floor: "focus research provider slot",
        },
        ProviderSlot {
            id: "anthropic_vision",
            substrate: "anthropic",
            adapter: "AnthropicVisionProvider",
            profile_kind: "vision_provider",
            env_var: "ANTHROPIC_API_KEY",
            required_receipt_kind: "ctx.anthropic_vision.observed",
            turn_on_signal: "live vision extraction observed for focus intake",
            local_fallback: "local_focus_text_intake",
            v1_floor: "focus vision provider slot",
        },
    ]
}

fn slot_value(slot: &ProviderSlot) -> Value {
    let env_ready = env_present(slot.env_var);
    json!({
        "id": slot.id,
        "substrate": slot.substrate,
        "adapter": slot.adapter,
        "profile_kind": slot.profile_kind,
        "status": if env_ready { "ENV_READY_TURN_ON_RECEIPT_REQUIRED" } else { "ENV_MISSING_TURN_ON_RECEIPT_REQUIRED" },
        "env_ready": env_ready,
        "env_requirements": [{
            "name": slot.env_var,
            "present": env_ready,
            "value_recorded": false
        }],
        "required_receipt_kind": slot.required_receipt_kind,
        "turn_on_signal": slot.turn_on_signal,
        "local_fallback": slot.local_fallback,
        "v1_floor": slot.v1_floor,
        "provider_call_allowed_now": false,
        "live_network_allowed_now": false,
        "turn_on_allowed_now": false,
        "credential_presence_only": true,
        "credential_values_recorded": false,
        "provider_secret_values_recorded": false,
        "requires_human_approval": true,
        "requires_turn_on_receipt": true,
        "production_writes_allowed": false
    })
}

fn preflight_value(preflight: &CtxProviderPreflight) -> Value {
    json!({
        "preflight_id": preflight.preflight_id,
        "tenant_id": preflight.tenant_id,
        "actor_id": preflight.actor_id,
        "provider_id": preflight.provider_id,
        "adapter": preflight.adapter,
        "required_receipt_kind": preflight.required_receipt_kind,
        "approval_receipt_id": preflight.approval_receipt_id,
        "status": preflight.status,
        "provider_call_allowed_now": false,
        "live_network_allowed_now": false,
        "credential_values_recorded": false,
        "provider_secret_values_recorded": false,
        "production_writes_allowed": false
    })
}

fn observation_value(observation: &CtxProviderObservation) -> Value {
    json!({
        "observation_id": observation.observation_id,
        "tenant_id": observation.tenant_id,
        "actor_id": observation.actor_id,
        "provider_id": observation.provider_id,
        "adapter": observation.adapter,
        "required_receipt_kind": observation.required_receipt_kind,
        "approval_receipt_id": observation.approval_receipt_id,
        "observed_receipt_id": observation.observed_receipt_id,
        "evidence_file": observation.evidence_file,
        "model_id": observation.model_id,
        "status": observation.status,
        "observed": observation.observed,
        "vector_dimensions": observation.vector_dimensions,
        "total_tokens": observation.total_tokens,
        "provider_connected_local_allowed": observation.observed,
        "provider_call_allowed_now": observation.observed,
        "live_network_allowed_now": observation.observed,
        "credential_values_recorded": false,
        "provider_secret_values_recorded": false,
        "requested_secret_values_recorded": false,
        "embedding_values_recorded": false,
        "output_text_recorded": false,
        "production_writes_allowed": false
    })
}

fn env_present(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn bool_value(value: &Value, field: &str, default: bool) -> bool {
    value.get(field).and_then(Value::as_bool).unwrap_or(default)
}

fn usize_value(value: &Value, field: &str, default: usize) -> usize {
    value
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|number| usize::try_from(number).ok())
        .unwrap_or(default)
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
    use super::CtxProviderRuntime;
    use serde_json::Value;

    #[test]
    fn rejects_non_observed_provider_evidence_without_authority() {
        let mut runtime = CtxProviderRuntime::default();
        let body = r#"{
          "provider_id": "openai_embeddings",
          "receipt_kind": "openai.embedding.observed",
          "approval_receipt_id": "ctx_provider_human_approval_receipt_000001",
          "observed": false,
          "vector_dimensions": 0,
          "embedding_values_recorded": false,
          "output_text_recorded": false,
          "credential_values_recorded": false,
          "provider_secret_values_recorded": false,
          "requested_secret_values_recorded": false,
          "production_writes_allowed": false
        }"#;

        let response: Value =
            serde_json::from_str(&runtime.observation_json(body).unwrap()).unwrap();

        assert_eq!(
            response["status"],
            "CTX_PROVIDER_TURN_ON_OBSERVATION_REJECTED"
        );
        assert_eq!(response["provider_connected_local_allowed"], false);
        assert_eq!(response["provider_secret_values_recorded"], false);
        assert_eq!(response["production_writes_allowed"], false);
    }

    #[test]
    fn accepts_secret_safe_openai_embedding_observation() {
        let mut runtime = CtxProviderRuntime::default();
        let body = r#"{
          "tenant_id": "tenant_local",
          "actor_id": "local_user",
          "provider_id": "openai_embeddings",
          "receipt_kind": "openai.embedding.observed",
          "approval_receipt_id": "ctx_provider_human_approval_receipt_000001",
          "observed_receipt_id": "ctx_openai_embedding_observed_000001",
          "evidence_file": ".mdx-local/provider-turn-on/openai-embedding-observed.json",
          "model_id": "text-embedding-3-small",
          "observed": true,
          "vector_dimensions": 1536,
          "total_tokens": 12,
          "embedding_values_recorded": false,
          "output_text_recorded": false,
          "credential_values_recorded": false,
          "provider_secret_values_recorded": false,
          "requested_secret_values_recorded": false,
          "production_writes_allowed": false
        }"#;

        let response: Value =
            serde_json::from_str(&runtime.observation_json(body).unwrap()).unwrap();
        let observations: Value =
            serde_json::from_str(&runtime.observations_json()).expect("observations json");

        assert_eq!(response["status"], "CTX_PROVIDER_TURN_ON_OBSERVED");
        assert_eq!(response["provider_connected_local_allowed"], true);
        assert_eq!(response["provider_secret_values_recorded"], false);
        assert_eq!(response["embedding_values_recorded"], false);
        assert_eq!(response["production_writes_allowed"], false);
        assert_eq!(observations["observed_provider_count"], 1);
        assert_eq!(observations["provider_connected_local_allowed"], true);
    }
}
