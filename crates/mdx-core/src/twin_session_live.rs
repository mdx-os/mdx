// The live half of the Twin answer gateway plus the draft report shape.
// A TwinLiveAnswer is produced OUTSIDE the kernel (the serve layer calls
// the provider under the provider-connected-local contract) and handed in
// as input - like draft_text. The kernel stays deterministic by default;
// when a live answer is present the receipts record the live gateway
// truthfully.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinLiveAnswer {
    pub answer_text: String,
    pub provider: String,
    pub adapter: String,
    pub model_id: String,
    pub inference_id: String,
    // The residency decision that produced this answer (tier + provider
    // privacy class), carried onto the receipt so data residency is
    // provable per conversation, not asserted.
    pub routing: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinSessionDraftReport {
    pub status: &'static str,
    pub session_id: String,
    pub draft_text: String,
    pub draft_receipt_id: String,
    pub memory_consolidation_proposal_receipt_id: String,
    pub memory_consolidation_review_receipt_id: String,
    pub memory_gate_receipt_id: String,
    pub memory_retrieval_receipt_id: String,
    pub brain_recall_receipt_id: String,
    pub recall_packet_id: String,
    pub brain_recall_scope: &'static str,
    pub brain_recall_policy: &'static str,
    pub brain_recall_source_count: u8,
    pub brain_recall_token_budget: u16,
    pub memory_record_id: String,
    pub answer_receipt_id: String,
    pub conversation_summary_receipt_id: String,
    pub memory_scoring_receipt_id: String,
    pub persona_drift_receipt_id: String,
    pub created_at: String,
    pub grounded_answer: String,
    pub conversation_summary: String,
    pub memory_relevance_score: u8,
    pub memory_decay_state: &'static str,
    pub memory_decay_policy: &'static str,
    pub persona_contract_status: &'static str,
    pub voice_drift_status: &'static str,
    pub voice_drift_score: u8,
    pub world_model_source: &'static str,
    pub compaction_policy: &'static str,
    pub compaction_state: &'static str,
    pub model_gateway_driver: String,
    pub model_gateway_provider: String,
    pub model_gateway_model_id: String,
    pub model_gateway_routing: String,
    pub model_gateway_inference_id: String,
    pub trusted_context_used: bool,
    pub trusted_context_source_count: usize,
    pub trusted_context_source_ids: String,
    pub trusted_context_receipt_ids: String,
    pub trusted_context_projection_route: &'static str,
    pub memory_driver: &'static str,
    pub memory_provider: &'static str,
    pub provider_call_allowed: bool,
    pub worker_spawn_allowed: bool,
}

pub(crate) struct GatewayAnswer {
    pub answer_text: String,
    pub driver: String,
    pub provider: String,
    pub model_id: String,
    pub routing: String,
    pub inference_id: String,
    pub live_provider_used: &'static str,
}

pub(crate) fn gateway_choice(
    request: &TwinSessionDraftRequest<'_>,
    _draft_receipt_id: &str,
    model_trace: &ModelGatewayTrace,
) -> GatewayAnswer {
    match request.live_answer {
        Some(live) => GatewayAnswer {
            answer_text: live.answer_text.clone(),
            driver: "live_model_gateway".to_string(),
            provider: live.adapter.clone(),
            model_id: live.model_id.clone(),
            // The residency label from the routing rail (tier + class +
            // provider). Falls back to the legacy marker only if a caller
            // somehow left it empty, so the field is never blank.
            routing: if live.routing.trim().is_empty() {
                "provider_connected_local".to_string()
            } else {
                live.routing.clone()
            },
            inference_id: live.inference_id.clone(),
            live_provider_used: "true",
        },
        // No live model admitted the call. The saved answer says so in
        // human words - never a receipt id or session token dressed up as
        // a reply. The receipts still ride the response fields; the words
        // are for the person who just asked a question and deserves the
        // truth about why no real answer came back.
        None => GatewayAnswer {
            answer_text: "No model is connected yet, so this is a placeholder, not a real \
                answer. Your message is saved - connect a model, ask again, and a real \
                answer will land here."
                .to_string(),
            driver: "local_model_gateway".to_string(),
            provider: "DeterministicModelGateway".to_string(),
            model_id: model_trace.variant.clone(),
            routing: model_trace.routing_strategy.clone(),
            inference_id: model_trace.inference_id.clone(),
            live_provider_used: "false",
        },
    }
}
