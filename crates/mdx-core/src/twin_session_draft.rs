use crate::twin_session_live::gateway_choice;
use crate::twin_session_trusted_context::*;
use crate::*;

const TWIN_MEMORY_DECAY_STATE: &str = "fresh_session_memory";
const TWIN_MEMORY_DECAY_POLICY: &str = "local_recent_session_decay_v1";
const TWIN_PERSONA_CONTRACT_STATUS: &str = "MATCHED_DECLARED_STANCE";
const TWIN_VOICE_DRIFT_STATUS: &str = "IN_BOUNDS";
const TWIN_VOICE_DRIFT_SCORE: u8 = 0;
const TWIN_WORLD_MODEL_SOURCE: &str = "generated/world-model/pages-projection-fixtures.json";
const TWIN_COMPACTION_POLICY: &str = "local_session_summary_v1";
const TWIN_COMPACTION_STATE: &str = "COMPACTED_LOCAL";
const TWIN_BRAIN_RECALL_SCOPE: &str = "private_user_session";
const TWIN_BRAIN_RECALL_POLICY: &str = "local_brain_recall_packet_v1";
// The byte cap on the live prompt's memory layer: 1200 bytes of ranked
// private memory plus 1200 bytes of active lesson summaries, matching the
// Forge advisory-context cap. The receipt records the actual bytes used.
const TWIN_BRAIN_RECALL_BYTE_BUDGET: u16 = 2400;
// A recall packet's final_score sums seven 0-100 components; dividing by
// the component count maps it back to the 0-100 scale the surfaces show.
const RECALL_SCORE_COMPONENTS: u16 = 7;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TwinSessionDraftRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub session_id: &'a str,
    pub companion_id: &'a str,
    pub companion_stance: &'a str,
    pub persona_profile_id: &'a str,
    pub prompt_shape: &'a str,
    pub evidence_receipt_id: &'a str,
    pub draft_text: &'a str,
    pub live_answer: Option<&'a crate::twin_session_live::TwinLiveAnswer>,
    // The memory packet actually assembled into the live prompt at prepare
    // time, when one was. None means no memory content reached any prompt
    // (deterministic answers), and the recall receipt says so honestly.
    pub recall_packet: Option<&'a TwinRecallPacket>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TwinSessionDraftError {
    Missing(&'static str),
    UnknownEvidenceReceipt(String),
    ActorAdmission(String),
}

impl TwinSessionDraftError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("twin session draft missing {field}"),
            Self::UnknownEvidenceReceipt(id) => {
                format!("twin session draft evidence receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_twin_session_draft_local(
        &mut self,
        request: TwinSessionDraftRequest<'_>,
    ) -> Result<TwinSessionDraftReport, TwinSessionDraftError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_twin_session_draft_local_with_identity(request, &identity)
    }

    pub fn save_twin_session_draft_local_with_identity(
        &mut self,
        request: TwinSessionDraftRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<TwinSessionDraftReport, TwinSessionDraftError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("session_id", request.session_id),
            ("companion_id", request.companion_id),
            ("companion_stance", request.companion_stance),
            ("persona_profile_id", request.persona_profile_id),
            ("prompt_shape", request.prompt_shape),
            ("evidence_receipt_id", request.evidence_receipt_id),
            ("draft_text", request.draft_text),
        ] {
            if value.trim().is_empty() {
                return Err(TwinSessionDraftError::Missing(field));
            }
        }
        if self
            .storage
            .ledger()
            .query()
            .by_id(request.evidence_receipt_id)
            .is_none()
        {
            return Err(TwinSessionDraftError::UnknownEvidenceReceipt(
                request.evidence_receipt_id.to_string(),
            ));
        }
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "operator",
            "/twin/session-drafts.json",
            "twin.session.draft.admitted",
            request.session_id,
        )
        .map_err(|error| TwinSessionDraftError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("twin_session_draft"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision = self.decide_with_receipt(&correlation, ActionKind::AdmitTwinSessionDraft);
        let created_at = local_created_at(
            self.storage
                .ledger()
                .query()
                .by_kind("twin.session.draft.admitted")
                .len()
                + 1,
        );
        let draft_receipt = self.transition_receipt(
            &run_id,
            "ADMIT_DRAFT",
            &correlation,
            &decision,
            "twin.session.draft.admitted",
            payload(&[
                ("session_id", request.session_id),
                ("companion_id", request.companion_id),
                ("companion_stance", request.companion_stance),
                ("persona_profile_id", request.persona_profile_id),
                ("prompt_shape", request.prompt_shape),
                ("draft_text", request.draft_text),
                ("evidence_receipt_id", request.evidence_receipt_id),
                ("created_at", &created_at),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("provider_call_allowed", "false"),
                ("worker_spawn_allowed", "false"),
            ]),
        );
        let proposal_decision = self.decide_with_receipt(&correlation, ActionKind::WriteMemory);
        let proposal_receipt = self.transition_receipt(
            &run_id,
            "PROPOSE_MEMORY_CONSOLIDATION",
            &correlation,
            &proposal_decision,
            "memory.consolidation.proposed",
            payload(&[
                ("proposal_policy", "local_session_summary_v1"),
                ("proposal_type", "retain_episode"),
                ("source_surface", "twin"),
                ("source_receipt_id", &draft_receipt.receipt_id),
                ("memory_scope", "private_user_memory"),
                ("memory_tier", "episodic"),
                ("atom_origin", "asserted"),
                ("world_model_origin_taxonomy", "derived,asserted,external"),
                ("decay_policy", TWIN_MEMORY_DECAY_POLICY),
                ("importance_score", "70"),
                ("human_review_required", "true"),
                ("promotion_allowed_without_review", "false"),
                ("provider_call_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        let review_decision = self.decide_with_receipt(&correlation, ActionKind::WriteMemory);
        let review_receipt = self.transition_receipt(
            &run_id,
            "REVIEW_MEMORY_CONSOLIDATION",
            &correlation,
            &review_decision,
            "memory.consolidation.reviewed",
            payload(&[
                ("proposal_receipt_id", &proposal_receipt.receipt_id),
                ("review_state", "APPROVED_LOCAL_REVIEW"),
                ("reviewer_actor_id", request.actor_id),
                ("review_policy", "local_human_review_stub_v1"),
                ("approved_memory_scope", "private_user_memory"),
                ("approved_memory_tier", "episodic"),
                ("approved_atom_origin", "asserted"),
                ("approved_decay_policy", TWIN_MEMORY_DECAY_POLICY),
                ("shared_scope_allowed", "false"),
                ("team_memory_allowed", "false"),
                ("company_memory_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        let memory_decision = self.decide_with_receipt(&correlation, ActionKind::WriteMemory);
        let gate_receipt = self.transition_receipt(
            &run_id,
            "CONSOLIDATE_DRAFT",
            &correlation,
            &memory_decision,
            "twin.session.draft.memory_consolidated",
            payload(&[
                ("decision", "RETAIN"),
                ("source_receipt_id", &draft_receipt.receipt_id),
                (
                    "consolidation_proposal_receipt_id",
                    &proposal_receipt.receipt_id,
                ),
                (
                    "consolidation_review_receipt_id",
                    &review_receipt.receipt_id,
                ),
                ("atom_origin", "asserted"),
                (
                    "valid_from_receipt_timestamp",
                    &draft_receipt.receipt_timestamp,
                ),
                ("memory_driver", LOCAL_MEMORY_DRIVER.driver_id),
                ("memory_provider", LOCAL_MEMORY_DRIVER.provider),
                ("memory_durable_driver", LOCAL_MEMORY_DRIVER.durable_driver),
            ]),
        );
        self.write_memory(
            &correlation,
            &draft_receipt.receipt_id,
            &gate_receipt.receipt_id,
            ConsolidationDecision::Retain,
            "twin",
            "private_user_memory",
            request.draft_text,
            MEMORY_CONSOLIDATION_ACTIVE,
        )
        .map_err(|_| TwinSessionDraftError::Missing("memory_write"))?;
        let memory_record_id = self
            .storage
            .memory_records()
            .last()
            .map(|record| record.memory_id.clone())
            .unwrap_or_default();
        let retrieval_decision = self.decide_with_receipt(&correlation, ActionKind::WriteMemory);
        let retrieval_receipt = self.transition_receipt(
            &run_id,
            "RETRIEVE_MEMORY",
            &correlation,
            &retrieval_decision,
            "twin.session.memory.retrieved",
            payload(&[
                ("session_id", request.session_id),
                ("source_draft_receipt_id", &draft_receipt.receipt_id),
                ("memory_gate_receipt_id", &gate_receipt.receipt_id),
                ("memory_record_id", &memory_record_id),
                ("memory_driver", LOCAL_MEMORY_DRIVER.driver_id),
                ("memory_provider", LOCAL_MEMORY_DRIVER.provider),
                ("retrieval_driver", "local_memory_store"),
                ("retrieval_scope", "session_local"),
                ("provider_call_allowed", "false"),
            ]),
        );
        // Everything below derives from the recall packet ACTUALLY assembled
        // into the live prompt at prepare time. No packet means no memory
        // content reached any prompt, and every value says so.
        let packet_sources = request
            .recall_packet
            .map(|packet| packet.sources.as_slice())
            .unwrap_or(&[]);
        let packet_lesson_count = request
            .recall_packet
            .map(|packet| packet.lesson_count)
            .unwrap_or(0);
        let relevance_score = packet_sources
            .first()
            .map(|source| (source.final_score / RECALL_SCORE_COMPONENTS).min(100) as u8)
            .unwrap_or(0);
        let recall_source_count = (packet_sources.len() + packet_lesson_count).min(255) as u8;
        let mut included = Vec::new();
        if !packet_sources.is_empty() {
            included.push("session_memory");
        }
        if packet_lesson_count > 0 {
            included.push("learn_active_memory");
        }
        let included_sources = included.join(",");
        let recall_memory_ids = packet_sources
            .iter()
            .map(|source| source.memory_id.as_str())
            .collect::<Vec<_>>()
            .join(",");
        let private_memory_prompt_context = bool_string(!packet_sources.is_empty());
        let (stale_count, superseded_count) =
            self.memory_recall_health("private_user_memory", request.tenant_id);
        let stale_memory_count = stale_count.to_string();
        let superseded_memory_count = superseded_count.to_string();
        let prompt_context_build_ms = request
            .recall_packet
            .map(|packet| packet.build_ms)
            .unwrap_or(0)
            .to_string();
        let brain_recall_context_bytes = request
            .recall_packet
            .map(|packet| packet.memory_bytes + packet.lesson_bytes)
            .unwrap_or(0)
            .to_string();
        let memory_relevance_basis = if packet_sources.is_empty() {
            "no_memory_in_live_prompt"
        } else {
            "live_prompt_recall_packet"
        };
        let scoring_decision = self.decide_with_receipt(&correlation, ActionKind::WriteMemory);
        let memory_relevance_score = relevance_score.to_string();
        let scoring_receipt = self.transition_receipt(
            &run_id,
            "SCORE_MEMORY",
            &correlation,
            &scoring_decision,
            "twin.session.memory.scored",
            payload(&[
                ("session_id", request.session_id),
                ("source_draft_receipt_id", &draft_receipt.receipt_id),
                ("memory_retrieval_receipt_id", &retrieval_receipt.receipt_id),
                ("memory_record_id", &memory_record_id),
                ("memory_relevance_score", &memory_relevance_score),
                ("memory_relevance_basis", memory_relevance_basis),
                ("memory_decay_state", TWIN_MEMORY_DECAY_STATE),
                ("memory_decay_policy", TWIN_MEMORY_DECAY_POLICY),
                ("world_model_source", TWIN_WORLD_MODEL_SOURCE),
                ("live_substrate_required", "false"),
            ]),
        );
        self.record_memory_recall_ranking(
            &correlation,
            "twin",
            request.draft_text,
            "private_user_memory",
            &scoring_receipt.receipt_id,
        );
        let recall_decision =
            self.decide_with_receipt(&correlation, ActionKind::AssembleLocalContext);
        let recall_packet_id = self.ids.next("brain_recall_packet");
        let brain_recall_source_count = recall_source_count.to_string();
        let brain_recall_token_budget = TWIN_BRAIN_RECALL_BYTE_BUDGET.to_string();
        let packet_top_recall_score = packet_sources
            .first()
            .map(|source| source.final_score.to_string())
            .unwrap_or_else(|| "0".to_string());
        let brain_recall_receipt = self.transition_receipt(
            &run_id,
            "PREFLIGHT_BRAIN_RECALL",
            &correlation,
            &recall_decision,
            "twin.session.brain_recall.preflighted",
            payload(&[
                ("session_id", request.session_id),
                ("source_draft_receipt_id", &draft_receipt.receipt_id),
                ("memory_retrieval_receipt_id", &retrieval_receipt.receipt_id),
                ("memory_scoring_receipt_id", &scoring_receipt.receipt_id),
                ("memory_record_id", &memory_record_id),
                ("recall_packet_id", &recall_packet_id),
                ("top_recall_score", &packet_top_recall_score),
                ("brain_recall_scope", TWIN_BRAIN_RECALL_SCOPE),
                ("brain_recall_policy", TWIN_BRAIN_RECALL_POLICY),
                ("brain_recall_source_count", &brain_recall_source_count),
                ("brain_recall_token_budget", &brain_recall_token_budget),
                ("brain_recall_context_bytes", &brain_recall_context_bytes),
                ("prompt_context_build_ms", &prompt_context_build_ms),
                ("brain_map_route", "/memory/brain-map.json"),
                ("included_sources", &included_sources),
                ("recall_memory_ids", &recall_memory_ids),
                ("memory_driver", LOCAL_MEMORY_DRIVER.driver_id),
                ("memory_provider", LOCAL_MEMORY_DRIVER.provider),
                ("world_model_source", TWIN_WORLD_MODEL_SOURCE),
                ("memory_decay_policy", TWIN_MEMORY_DECAY_POLICY),
                ("stale_memory_count", &stale_memory_count),
                ("superseded_memory_count", &superseded_memory_count),
                ("provider_call_allowed", "false"),
                ("provider_export_allowed", "false"),
                // Private memory rides the live prompt exactly like
                // persona_context does: prompt-only, presence-tagged. Export
                // stays refused - no receipt or projection carries content.
                (
                    "private_memory_prompt_context",
                    private_memory_prompt_context,
                ),
                ("private_memory_export_allowed", "false"),
                ("runtime_influence_scope", "prompt_context_only"),
                ("adaptation_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        let trusted_context_sources =
            trusted_context_sources_from_receipts(self.storage.ledger().entries());
        let trusted_context_source_count = trusted_context_sources.len();
        let trusted_context_used = trusted_context_source_count > 0;
        let trusted_context_source_ids =
            trusted_context_sources_joined(&trusted_context_sources, |source| &source.source_id);
        let trusted_context_receipt_ids =
            trusted_context_sources_joined(&trusted_context_sources, |source| {
                &source.trust_decision_receipt_id
            });
        let model_trace = DeterministicModelGateway
            .run_eval_suite("twin_session_grounded_answer")
            .map_err(|_| TwinSessionDraftError::Missing("model_gateway"))?;
        let gateway = gateway_choice(&request, &draft_receipt.receipt_id, &model_trace);
        let answer_decision = self.decide_with_receipt(&correlation, ActionKind::RunEvalSuite);
        let answer_receipt = self.transition_receipt(
            &run_id,
            "ANSWER",
            &correlation,
            &answer_decision,
            "twin.session.answer.grounded",
            payload(&[
                ("session_id", request.session_id),
                ("source_draft_receipt_id", &draft_receipt.receipt_id),
                ("memory_retrieval_receipt_id", &retrieval_receipt.receipt_id),
                ("memory_scoring_receipt_id", &scoring_receipt.receipt_id),
                ("brain_recall_receipt_id", &brain_recall_receipt.receipt_id),
                ("recall_packet_id", &recall_packet_id),
                ("brain_recall_scope", TWIN_BRAIN_RECALL_SCOPE),
                ("brain_recall_source_count", &brain_recall_source_count),
                ("brain_recall_policy", TWIN_BRAIN_RECALL_POLICY),
                ("grounded_answer", &gateway.answer_text),
                ("model_gateway_driver", &gateway.driver),
                ("model_gateway_provider", &gateway.provider),
                ("model_gateway_model_id", &gateway.model_id),
                ("model_gateway_routing", &gateway.routing),
                ("model_gateway_inference_id", &gateway.inference_id),
                ("live_provider_used", gateway.live_provider_used),
                ("trusted_context_used", bool_string(trusted_context_used)),
                (
                    "trusted_context_source_count",
                    &trusted_context_source_count.to_string(),
                ),
                ("trusted_context_source_ids", &trusted_context_source_ids),
                ("trusted_context_receipt_ids", &trusted_context_receipt_ids),
                (
                    "trusted_context_projection_route",
                    "/pages/context-sources/projection.json",
                ),
                (
                    "trusted_context_policy",
                    "active_pages_context_trust_decisions_only",
                ),
                ("provider_call_allowed", "false"),
                ("world_model_source", TWIN_WORLD_MODEL_SOURCE),
            ]),
        );
        let persona_drift_score = TWIN_VOICE_DRIFT_SCORE.to_string();
        let persona_drift_decision =
            self.decide_with_receipt(&correlation, ActionKind::RunEvalSuite);
        let persona_drift_receipt = self.transition_receipt(
            &run_id,
            "CHECK_PERSONA_DRIFT",
            &correlation,
            &persona_drift_decision,
            "twin.session.persona.drift_checked",
            payload(&[
                ("session_id", request.session_id),
                ("source_draft_receipt_id", &draft_receipt.receipt_id),
                ("answer_receipt_id", &answer_receipt.receipt_id),
                ("companion_stance", request.companion_stance),
                ("persona_profile_id", request.persona_profile_id),
                ("persona_contract_status", TWIN_PERSONA_CONTRACT_STATUS),
                ("voice_drift_status", TWIN_VOICE_DRIFT_STATUS),
                ("voice_drift_score", &persona_drift_score),
                ("forbidden_action_drift", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        let conversation_summary = format!(
            "Session {} has one local operator prompt, one retained memory record, and one grounded local answer.",
            request.session_id
        );
        let summary_decision = self.decide_with_receipt(&correlation, ActionKind::RunEvalSuite);
        let summary_receipt = self.transition_receipt(
            &run_id,
            "SUMMARIZE_CONVERSATION",
            &correlation,
            &summary_decision,
            "twin.session.conversation.summarized",
            payload(&[
                ("session_id", request.session_id),
                ("source_draft_receipt_id", &draft_receipt.receipt_id),
                ("answer_receipt_id", &answer_receipt.receipt_id),
                ("memory_retrieval_receipt_id", &retrieval_receipt.receipt_id),
                ("memory_scoring_receipt_id", &scoring_receipt.receipt_id),
                ("brain_recall_receipt_id", &brain_recall_receipt.receipt_id),
                ("recall_packet_id", &recall_packet_id),
                (
                    "persona_drift_receipt_id",
                    &persona_drift_receipt.receipt_id,
                ),
                ("summary_text", &conversation_summary),
                ("summary_state", "LOCAL_ONLY"),
                ("compaction_policy", TWIN_COMPACTION_POLICY),
                ("compaction_state", TWIN_COMPACTION_STATE),
                ("message_count", "2"),
                ("memory_reference_count", "1"),
                ("model_trace_count", "1"),
                ("world_model_source_count", "1"),
                ("provider_call_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        if let Some(run) = self.storage.loop_runs_mut().last_mut() {
            run.status = "COMPLETED".to_string();
        }
        Ok(TwinSessionDraftReport {
            status: "SAVED_LOCAL_DRAFT",
            session_id: request.session_id.to_string(),
            draft_text: request.draft_text.to_string(),
            draft_receipt_id: draft_receipt.receipt_id,
            memory_consolidation_proposal_receipt_id: proposal_receipt.receipt_id,
            memory_consolidation_review_receipt_id: review_receipt.receipt_id,
            memory_gate_receipt_id: gate_receipt.receipt_id,
            memory_retrieval_receipt_id: retrieval_receipt.receipt_id,
            brain_recall_receipt_id: brain_recall_receipt.receipt_id,
            recall_packet_id,
            brain_recall_scope: TWIN_BRAIN_RECALL_SCOPE,
            brain_recall_policy: TWIN_BRAIN_RECALL_POLICY,
            brain_recall_source_count: recall_source_count,
            brain_recall_token_budget: TWIN_BRAIN_RECALL_BYTE_BUDGET,
            memory_record_id,
            answer_receipt_id: answer_receipt.receipt_id,
            conversation_summary_receipt_id: summary_receipt.receipt_id,
            memory_scoring_receipt_id: scoring_receipt.receipt_id,
            persona_drift_receipt_id: persona_drift_receipt.receipt_id,
            created_at,
            grounded_answer: gateway.answer_text.clone(),
            conversation_summary,
            memory_relevance_score: relevance_score,
            memory_decay_state: TWIN_MEMORY_DECAY_STATE,
            memory_decay_policy: TWIN_MEMORY_DECAY_POLICY,
            persona_contract_status: TWIN_PERSONA_CONTRACT_STATUS,
            voice_drift_status: TWIN_VOICE_DRIFT_STATUS,
            voice_drift_score: TWIN_VOICE_DRIFT_SCORE,
            world_model_source: TWIN_WORLD_MODEL_SOURCE,
            compaction_policy: TWIN_COMPACTION_POLICY,
            compaction_state: TWIN_COMPACTION_STATE,
            model_gateway_driver: gateway.driver,
            model_gateway_provider: gateway.provider,
            model_gateway_model_id: gateway.model_id,
            model_gateway_routing: gateway.routing,
            model_gateway_inference_id: gateway.inference_id,
            trusted_context_used,
            trusted_context_source_count,
            trusted_context_source_ids,
            trusted_context_receipt_ids,
            trusted_context_projection_route: "/pages/context-sources/projection.json",
            memory_driver: LOCAL_MEMORY_DRIVER.driver_id,
            memory_provider: LOCAL_MEMORY_DRIVER.provider,
            provider_call_allowed: false,
            worker_spawn_allowed: false,
        })
    }
}
