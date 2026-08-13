use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinModelGatewayProviderObservation<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub provider_id: &'a str,
    pub adapter: &'a str,
    pub receipt_kind: &'a str,
    pub approval_receipt_id: &'a str,
    pub evidence_file: &'a str,
    pub model_id: &'a str,
    pub response_id: &'a str,
    pub response_status: &'a str,
    pub observed: bool,
    pub provider_call_attempted: bool,
    pub network_call_attempted: bool,
    pub credential_presence_only: bool,
    pub credential_values_recorded: bool,
    pub provider_secret_values_recorded: bool,
    pub requested_secret_values_recorded: bool,
    pub output_text_recorded: bool,
    pub production_write_allowed: bool,
    pub total_tokens: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinModelGatewayProviderObservationReport {
    pub status: &'static str,
    pub provider_id: String,
    pub adapter: String,
    pub receipt_kind: String,
    pub approval_receipt_id: String,
    pub evidence_file: String,
    pub model_id: String,
    pub response_id: String,
    pub response_status: String,
    pub observed: bool,
    pub accepted: bool,
    pub provider_connected_local_allowed: bool,
    pub provider_call_allowed_now: bool,
    pub live_network_allowed_now: bool,
    pub credential_presence_only: bool,
    pub credential_values_recorded: bool,
    pub provider_secret_values_recorded: bool,
    pub requested_secret_values_recorded: bool,
    pub output_text_recorded: bool,
    pub production_write_allowed: bool,
    pub total_tokens: usize,
    pub denial_reason: &'static str,
    pub observation_receipt_id: String,
    pub policy_decision_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TwinModelGatewayProviderObservationError {
    MissingField(&'static str),
    UnknownProvider(String),
}

impl TwinModelGatewayProviderObservationError {
    pub fn message(&self) -> String {
        match self {
            Self::MissingField(field) => {
                format!("missing Twin ModelGateway observation field {field}")
            }
            Self::UnknownProvider(provider) => {
                format!("unknown Twin ModelGateway provider {provider}")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TwinModelGatewayProviderSlot {
    pub provider_id: &'static str,
    pub adapter: &'static str,
    pub required_receipt_kind: &'static str,
    pub env_key: &'static str,
    pub stream_contract: &'static str,
    pub turn_on_signal: &'static str,
    /// Top models for this provider, best first. A first connect with no
    /// explicit choice and no env preference picks the first of these the
    /// provider actually lists - so MDx opens on a good model, not whatever
    /// happens to sort first. Validated against the live list every time, so a
    /// name going stale here can never connect a model the provider does not
    /// offer: it just falls through to the next, then to the live list.
    pub suggested_models: &'static [&'static str],
}

pub const TWIN_MODEL_GATEWAY_PROVIDER_SLOTS: [TwinModelGatewayProviderSlot; 7] = [
    TwinModelGatewayProviderSlot {
        provider_id: "ollama",
        adapter: "OllamaLocalModelGateway",
        required_receipt_kind: "ollama.chat.observed",
        env_key: "MDX_OLLAMA_API_KEY",
        stream_contract: "normalized_model_stream_v1",
        turn_on_signal: "Ollama localhost chat completion observed through the governed local turn-on",
        suggested_models: &["qwen3.5:27b", "qwen3.5:9b", "qwen3:14b"],
    },
    TwinModelGatewayProviderSlot {
        provider_id: "openai",
        adapter: "OpenAIResponsesModelGateway",
        required_receipt_kind: "openai.response.observed",
        env_key: "OPENAI_API_KEY",
        stream_contract: "normalized_model_stream_v1",
        turn_on_signal: "OpenAI response observed through the governed Responses API turn-on",
        suggested_models: &["gpt-5", "gpt-5-mini"],
    },
    TwinModelGatewayProviderSlot {
        provider_id: "anthropic",
        adapter: "AnthropicMessagesModelGateway",
        required_receipt_kind: "anthropic.message.observed",
        env_key: "ANTHROPIC_API_KEY",
        stream_contract: "normalized_model_stream_v1",
        turn_on_signal: "Anthropic message observed through the governed Messages turn-on",
        suggested_models: &["claude-opus-4-8", "claude-sonnet-4-6"],
    },
    TwinModelGatewayProviderSlot {
        provider_id: "xai",
        adapter: "XaiChatModelGateway",
        required_receipt_kind: "xai.chat.observed",
        env_key: "XAI_API_KEY",
        stream_contract: "normalized_model_stream_v1",
        turn_on_signal: "xAI chat completion observed through the governed turn-on",
        suggested_models: &["grok-4.5", "grok-4"],
    },
    TwinModelGatewayProviderSlot {
        provider_id: "gemini",
        adapter: "GeminiModelGateway",
        required_receipt_kind: "gemini.generate_content.observed",
        env_key: "GEMINI_API_KEY",
        stream_contract: "normalized_model_stream_v1",
        turn_on_signal: "Gemini content generation observed through the governed turn-on",
        suggested_models: &["gemini-2.5-pro", "gemini-2.5-flash"],
    },
    TwinModelGatewayProviderSlot {
        provider_id: "mistral",
        adapter: "MistralChatModelGateway",
        required_receipt_kind: "mistral.chat.observed",
        env_key: "MISTRAL_API_KEY",
        stream_contract: "normalized_model_stream_v1",
        turn_on_signal: "Mistral chat completion observed through the governed turn-on",
        suggested_models: &["mistral-large-latest"],
    },
    TwinModelGatewayProviderSlot {
        provider_id: "tensorzero",
        adapter: "TensorZeroModelGateway",
        required_receipt_kind: "tensorzero.inference.observed",
        env_key: "TENSORZERO_GATEWAY_URL",
        stream_contract: "normalized_model_stream_v1",
        turn_on_signal: "TensorZero inference observed through the governed gateway turn-on",
        suggested_models: &[],
    },
];

pub fn twin_model_gateway_provider_slots() -> &'static [TwinModelGatewayProviderSlot] {
    &TWIN_MODEL_GATEWAY_PROVIDER_SLOTS
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_twin_model_gateway_provider_observation_local(
        &mut self,
        observation: TwinModelGatewayProviderObservation<'_>,
    ) -> Result<TwinModelGatewayProviderObservationReport, TwinModelGatewayProviderObservationError>
    {
        let provider_id = required(observation.provider_id, "provider_id")?;
        let slot = TWIN_MODEL_GATEWAY_PROVIDER_SLOTS
            .iter()
            .find(|slot| slot.provider_id == provider_id)
            .ok_or_else(|| {
                TwinModelGatewayProviderObservationError::UnknownProvider(provider_id.to_string())
            })?;
        let approval_receipt_id = required(observation.approval_receipt_id, "approval_receipt_id")?;
        let evidence_file = required(observation.evidence_file, "evidence_file")?;
        let model_id = required(observation.model_id, "model_id")?;
        let response_id = required(observation.response_id, "response_id")?;
        let response_status = required(observation.response_status, "response_status")?;

        let safe_secret_posture = observation.credential_presence_only
            && !observation.credential_values_recorded
            && !observation.provider_secret_values_recorded
            && !observation.requested_secret_values_recorded;
        let safe_output_posture = !observation.output_text_recorded;
        let safe_authority_posture = !observation.production_write_allowed;
        let adapter_matches = observation.adapter == slot.adapter;
        let receipt_matches = observation.receipt_kind == slot.required_receipt_kind;
        let accepted = observation.observed
            && observation.provider_call_attempted
            && observation.network_call_attempted
            && adapter_matches
            && receipt_matches
            && safe_secret_posture
            && safe_output_posture
            && safe_authority_posture
            && observation.total_tokens > 0;
        let denial_reason = if accepted {
            "accepted"
        } else if !adapter_matches {
            "adapter_mismatch"
        } else if !receipt_matches {
            "receipt_kind_mismatch"
        } else if !observation.observed {
            "provider_observation_missing"
        } else if !observation.provider_call_attempted || !observation.network_call_attempted {
            "network_observation_missing"
        } else if !safe_secret_posture {
            "secret_recording_rejected"
        } else if !safe_output_posture {
            "output_text_recording_rejected"
        } else if !safe_authority_posture {
            "production_authority_rejected"
        } else {
            "token_usage_missing"
        };
        let status = if accepted {
            "TWIN_MODEL_GATEWAY_PROVIDER_OBSERVED"
        } else {
            "TWIN_MODEL_GATEWAY_PROVIDER_OBSERVATION_REJECTED"
        };
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(required(observation.tenant_id, "tenant_id")?),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(required(observation.actor_id, "actor_id")?),
            loop_id: LoopId::new("twin_model_gateway_provider"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let decision = self.decide_with_receipt(&correlation, ActionKind::AdmitTwinSessionDraft);
        let total_tokens = observation.total_tokens.to_string();
        let receipt = self.storage.ledger_mut().append(
            &mut self.ids,
            &correlation,
            "twin.model_gateway.provider.observed",
            Some(decision.policy_decision_id.clone()),
            payload(&[
                ("provider_id", slot.provider_id),
                ("adapter", slot.adapter),
                ("receipt_kind", slot.required_receipt_kind),
                ("approval_receipt_id", approval_receipt_id),
                ("evidence_file", evidence_file),
                ("model_id", model_id),
                ("response_id", response_id),
                ("response_status", response_status),
                (
                    "observed",
                    if observation.observed {
                        "true"
                    } else {
                        "false"
                    },
                ),
                (
                    "provider_call_attempted",
                    if observation.provider_call_attempted {
                        "true"
                    } else {
                        "false"
                    },
                ),
                (
                    "network_call_attempted",
                    if observation.network_call_attempted {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("credential_presence_only", "true"),
                ("credential_values_recorded", "false"),
                ("provider_secret_values_recorded", "false"),
                ("requested_secret_values_recorded", "false"),
                ("output_text_recorded", "false"),
                ("total_tokens", &total_tokens),
                (
                    "provider_connected_local_allowed",
                    if accepted { "true" } else { "false" },
                ),
                (
                    "provider_call_allowed_now",
                    if accepted { "true" } else { "false" },
                ),
                (
                    "live_network_allowed_now",
                    if accepted { "true" } else { "false" },
                ),
                ("production_write_allowed", "false"),
                ("terminal_state", status),
                ("denial_reason", denial_reason),
            ]),
        );
        self.storage
            .ledger()
            .verify()
            .map_err(|_| TwinModelGatewayProviderObservationError::MissingField("ledger_verify"))?;
        Ok(TwinModelGatewayProviderObservationReport {
            status,
            provider_id: slot.provider_id.to_string(),
            adapter: slot.adapter.to_string(),
            receipt_kind: slot.required_receipt_kind.to_string(),
            approval_receipt_id: approval_receipt_id.to_string(),
            evidence_file: evidence_file.to_string(),
            model_id: model_id.to_string(),
            response_id: response_id.to_string(),
            response_status: response_status.to_string(),
            observed: observation.observed,
            accepted,
            provider_connected_local_allowed: accepted,
            provider_call_allowed_now: accepted,
            live_network_allowed_now: accepted,
            credential_presence_only: true,
            credential_values_recorded: false,
            provider_secret_values_recorded: false,
            requested_secret_values_recorded: false,
            output_text_recorded: false,
            production_write_allowed: false,
            total_tokens: observation.total_tokens,
            denial_reason,
            observation_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
        })
    }
}

fn required<'a>(
    value: &'a str,
    field: &'static str,
) -> Result<&'a str, TwinModelGatewayProviderObservationError> {
    let value = value.trim();
    if value.is_empty() {
        Err(TwinModelGatewayProviderObservationError::MissingField(
            field,
        ))
    } else {
        Ok(value)
    }
}
