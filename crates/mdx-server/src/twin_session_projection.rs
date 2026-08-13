// The Twin session projection: every draft receipt joined to its answer,
// memory, scoring, drift, and summary receipts - the saved-conversation
// truth the surfaces restore from. Split from twin_session.rs so the POST
// path stays within its line budget as request fields grow.
use mdx_core::{MdxKernel, json_string_literal};

pub(crate) fn render_local_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let drafts = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "twin.session.draft.admitted")
        .map(|receipt| {
            let memory = kernel
                .memory_records()
                .iter()
                .find(|record| record.source_receipt_id == receipt.receipt_id);
            let answer = kernel.ledger().entries().iter().find(|answer| {
                answer.kind == "twin.session.answer.grounded"
                    && payload_value(answer, "source_draft_receipt_id") == receipt.receipt_id
            });
            let retrieval = kernel.ledger().entries().iter().find(|retrieval| {
                retrieval.kind == "twin.session.memory.retrieved"
                    && payload_value(retrieval, "source_draft_receipt_id") == receipt.receipt_id
            });
            let summary = kernel.ledger().entries().iter().find(|summary| {
                summary.kind == "twin.session.conversation.summarized"
                    && payload_value(summary, "source_draft_receipt_id") == receipt.receipt_id
            });
            let scoring = kernel.ledger().entries().iter().find(|scoring| {
                scoring.kind == "twin.session.memory.scored"
                    && payload_value(scoring, "source_draft_receipt_id") == receipt.receipt_id
            });
            let brain_recall = kernel.ledger().entries().iter().find(|brain_recall| {
                brain_recall.kind == "twin.session.brain_recall.preflighted"
                    && payload_value(brain_recall, "source_draft_receipt_id") == receipt.receipt_id
            });
            let drift = kernel.ledger().entries().iter().find(|drift| {
                drift.kind == "twin.session.persona.drift_checked"
                    && payload_value(drift, "source_draft_receipt_id") == receipt.receipt_id
            });
            let recall_ranking = memory.and_then(|record| {
                kernel
                    .memory_recall_rankings()
                    .iter()
                    .rev()
                    .find(|ranking| ranking.memory_id == record.memory_id)
            });
            let recall_explanation = recall_ranking
                .map(|ranking| {
                    format!(
                        "rank={} lexical={} content_checksum={} graph={} recency={} importance={} scope={} authority={}",
                        ranking.rank,
                        ranking.lexical_score,
                        ranking.content_checksum_score,
                        ranking.graph_score,
                        ranking.recency_score,
                        ranking.importance_score,
                        ranking.scope_score,
                        ranking.source_authority_score
                    )
                })
                .unwrap_or_else(|| "no ranked memory selected".to_string());
            format!(
                r#"{{"session_id":{},"draft_text":{},"draft_receipt_id":{},"memory_gate_receipt_id":{},"memory_retrieval_receipt_id":{},"memory_scoring_receipt_id":{},"brain_recall_receipt_id":{},"recall_packet_id":{},"brain_recall_scope":{},"brain_recall_policy":{},"brain_recall_source_count":{},"brain_recall_token_budget":{},"memory_recall_explanation":{},"memory_recall_final_score":{},"memory_review_boundary":{},"memory_source_receipt_id":{},"memory_record_id":{},"answer_receipt_id":{},"persona_drift_receipt_id":{},"conversation_summary_receipt_id":{},"created_at":{},"grounded_answer":{},"conversation_summary":{},"memory_relevance_score":{},"memory_decay_state":{},"memory_decay_policy":{},"persona_contract_status":{},"voice_drift_status":{},"voice_drift_score":{},"world_model_source":{},"compaction_policy":{},"compaction_state":{},"model_gateway_driver":{},"model_gateway_provider":{},"model_gateway_model_id":{},"model_gateway_routing":{},"model_gateway_inference_id":{},"trusted_context_used":{},"trusted_context_source_count":{},"trusted_context_source_ids":{},"trusted_context_receipt_ids":{},"trusted_context_projection_route":{},"memory_driver":{},"memory_provider":{},"retrieval_driver":{},"retrieval_scope":{},"summary_state":{},"message_count":{},"memory_reference_count":{},"model_trace_count":{},"world_model_source_count":{},"companion_id":{},"companion_stance":{},"persona_profile_id":{},"prompt_shape":{},"source_evidence_receipt_id":{},"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "session_id")),
                json_string_literal(payload_value(receipt, "draft_text")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(memory.map(|record| record.provenance.gate_receipt_id.as_str()).unwrap_or("")),
                json_string_literal(retrieval.map(|receipt| receipt.receipt_id.as_str()).unwrap_or("")),
                json_string_literal(scoring.map(|receipt| receipt.receipt_id.as_str()).unwrap_or("")),
                json_string_literal(brain_recall.map(|receipt| receipt.receipt_id.as_str()).unwrap_or("")),
                json_string_literal(brain_recall.map(|receipt| payload_value(receipt, "recall_packet_id")).unwrap_or("")),
                json_string_literal(brain_recall.map(|receipt| payload_value(receipt, "brain_recall_scope")).unwrap_or("private_user_session")),
                json_string_literal(brain_recall.map(|receipt| payload_value(receipt, "brain_recall_policy")).unwrap_or("local_brain_recall_packet_v1")),
                brain_recall.map(|receipt| payload_value(receipt, "brain_recall_source_count")).unwrap_or("0"),
                brain_recall.map(|receipt| payload_value(receipt, "brain_recall_token_budget")).unwrap_or("0"),
                json_string_literal(&recall_explanation),
                recall_ranking.map(|ranking| ranking.final_score).unwrap_or_default(),
                json_string_literal("approved_local_review_required_for_promotion"),
                json_string_literal(memory.map(|record| record.source_receipt_id.as_str()).unwrap_or("")),
                json_string_literal(memory.map(|record| record.memory_id.as_str()).unwrap_or("")),
                json_string_literal(answer.map(|receipt| receipt.receipt_id.as_str()).unwrap_or("")),
                json_string_literal(drift.map(|receipt| receipt.receipt_id.as_str()).unwrap_or("")),
                json_string_literal(summary.map(|receipt| receipt.receipt_id.as_str()).unwrap_or("")),
                json_string_literal(payload_value(receipt, "created_at")),
                json_string_literal(answer.map(|receipt| payload_value(receipt, "grounded_answer")).unwrap_or("")),
                json_string_literal(summary.map(|receipt| payload_value(receipt, "summary_text")).unwrap_or("")),
                scoring.map(|receipt| payload_value(receipt, "memory_relevance_score")).unwrap_or("0"),
                json_string_literal(scoring.map(|receipt| payload_value(receipt, "memory_decay_state")).unwrap_or("fresh_session_memory")),
                json_string_literal(scoring.map(|receipt| payload_value(receipt, "memory_decay_policy")).unwrap_or("local_recent_session_decay_v1")),
                json_string_literal(drift.map(|receipt| payload_value(receipt, "persona_contract_status")).unwrap_or("MATCHED_DECLARED_STANCE")),
                json_string_literal(drift.map(|receipt| payload_value(receipt, "voice_drift_status")).unwrap_or("IN_BOUNDS")),
                drift.map(|receipt| payload_value(receipt, "voice_drift_score")).unwrap_or("0"),
                json_string_literal(scoring.map(|receipt| payload_value(receipt, "world_model_source")).unwrap_or("generated/world-model/pages-projection-fixtures.json")),
                json_string_literal(summary.map(|receipt| payload_value(receipt, "compaction_policy")).unwrap_or("local_session_summary_v1")),
                json_string_literal(summary.map(|receipt| payload_value(receipt, "compaction_state")).unwrap_or("COMPACTED_LOCAL")),
                json_string_literal(answer.map(|receipt| payload_value(receipt, "model_gateway_driver")).unwrap_or("local_model_gateway")),
                json_string_literal(answer.map(|receipt| payload_value(receipt, "model_gateway_provider")).unwrap_or("DeterministicModelGateway")),
                json_string_literal(answer.map(|receipt| payload_value(receipt, "model_gateway_model_id")).unwrap_or("deterministic_local_v1")),
                json_string_literal(answer.map(|receipt| payload_value(receipt, "model_gateway_routing")).unwrap_or("single_deterministic_stub")),
                json_string_literal(answer.map(|receipt| payload_value(receipt, "model_gateway_inference_id")).unwrap_or("")),
                answer.map(|receipt| payload_value(receipt, "trusted_context_used")).unwrap_or("false"),
                answer.map(|receipt| payload_value(receipt, "trusted_context_source_count")).unwrap_or("0"),
                json_string_literal(answer.map(|receipt| payload_value(receipt, "trusted_context_source_ids")).unwrap_or("")),
                json_string_literal(answer.map(|receipt| payload_value(receipt, "trusted_context_receipt_ids")).unwrap_or("")),
                json_string_literal(answer.map(|receipt| payload_value(receipt, "trusted_context_projection_route")).unwrap_or("/pages/context-sources/projection.json")),
                json_string_literal(memory.map(|record| record.provenance.driver_id).unwrap_or("local_memory_store")),
                json_string_literal(memory.map(|record| record.provenance.provider).unwrap_or("InMemoryProvider")),
                json_string_literal(retrieval.map(|receipt| payload_value(receipt, "retrieval_driver")).unwrap_or("local_memory_store")),
                json_string_literal(retrieval.map(|receipt| payload_value(receipt, "retrieval_scope")).unwrap_or("session_local")),
                json_string_literal(summary.map(|receipt| payload_value(receipt, "summary_state")).unwrap_or("LOCAL_ONLY")),
                summary.map(|receipt| payload_value(receipt, "message_count")).unwrap_or("0"),
                summary.map(|receipt| payload_value(receipt, "memory_reference_count")).unwrap_or("0"),
                summary.map(|receipt| payload_value(receipt, "model_trace_count")).unwrap_or("0"),
                summary.map(|receipt| payload_value(receipt, "world_model_source_count")).unwrap_or("0"),
                json_string_literal(payload_value(receipt, "companion_id")),
                json_string_literal(payload_value(receipt, "companion_stance")),
                json_string_literal(payload_value(receipt, "persona_profile_id")),
                json_string_literal(payload_value(receipt, "prompt_shape")),
                json_string_literal(payload_value(receipt, "evidence_receipt_id"))
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-twin-session-draft-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/twin/session-drafts.json","draft_count":{},"memory_record_count":{},"provider_call_allowed":false,"worker_spawn_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"drafts":[{}]}}"#,
        drafts.len(),
        kernel
            .memory_records()
            .iter()
            .filter(|record| record.provenance.source_receipt_kind == "twin.session.draft.admitted")
            .count(),
        drafts.join(",")
    ))
}

fn payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}
