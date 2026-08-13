use mdx_core::{
    GovernedWriteIdentity, MdxKernel, TwinLiveAnswer, TwinSessionDraftRequest, json_string_literal,
};

pub(crate) fn render_json() -> Result<String, String> {
    Ok(
        include_str!("../../../generated/companions/twin-session-persona-contract.json")
            .trim()
            .to_string(),
    )
}

pub(crate) fn render_local_command_json() -> Result<String, String> {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent()?;
    let evidence_receipt_id = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.kind == "credential.minted")
        .map(|receipt| receipt.receipt_id.clone())
        .ok_or_else(|| "local Twin draft command missing credential evidence".to_string())?;
    let report = kernel
        .save_twin_session_draft_local(TwinSessionDraftRequest {
            tenant_id: "tenant_local",
            actor_id: "local_operator",
            session_id: "twin_session_local_001",
            companion_id: "twin_architect",
            companion_stance: "risk_and_architecture",
            persona_profile_id: "architect_stress_tests_assumptions",
            prompt_shape: "help me think through the next migration slice",
            evidence_receipt_id: &evidence_receipt_id,
            draft_text: "Local Twin draft saved from evidence-backed session admission proof.",
            live_answer: None,
            recall_packet: None,
        })
        .map_err(|error| error.message())?;
    Ok(report.render_json())
}

/// Parsed Twin draft POST state, including resolved identity and any prepared
/// live call, carried across the unlocked provider boundary.
pub(crate) struct PreparedDraftPost {
    pub evidence_receipt_id: String,
    pub session_id: String,
    pub companion_id: String,
    pub companion_stance: String,
    pub persona_profile_id: String,
    pub prompt_shape: String,
    pub draft_text: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub identity: GovernedWriteIdentity,
    pub resolved: crate::request_security::ResolvedWriteIdentity,
    pub live_call: Option<crate::twin_live_gateway::PreparedLiveCall>,
}

pub(crate) fn prepare_local_post(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<PreparedDraftPost, String> {
    let evidence_receipt_id = credential_receipt_id(kernel)?;
    let session_id = field_or(body, "session_id", "twin_session_local_post_001");
    let companion_id = field_or(body, "companion_id", "twin_architect");
    let companion_stance = field_or(body, "companion_stance", "risk_and_architecture");
    let persona_profile_id = field_or(
        body,
        "persona_profile_id",
        "architect_stress_tests_assumptions",
    );
    let prompt_shape = field_or(
        body,
        "prompt_shape",
        "help me think through the next migration slice",
    );
    let draft_text = field_or(
        body,
        "draft_text",
        "Local Twin draft saved from authenticated local POST route.",
    );
    // The verified trusted session is the identity. The local-demo fallback
    // uses the same tenant as the install and model-fabric routes so a model
    // connected from the native setup flow can actually answer Twin.
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let verified = resolved.auth_session_status == "VERIFIED_TRUSTED_SESSION";
    let tenant_id = if verified {
        resolved.tenant_id.clone()
    } else {
        field_or(body, "tenant_id", "local_tenant")
    };
    let actor_id = if verified {
        resolved.actor_id.clone()
    } else {
        field_or(body, "actor_id", "local_user")
    };
    let identity = if verified {
        resolved.identity.clone()
    } else {
        GovernedWriteIdentity::local_demo(&actor_id)
    };
    // Live gateway when turn-on evidence + key hold; else deterministic.
    // Prompt-only layers (the person's own from You, a custom companion's
    // own), capped; never written to receipts - those carry the ids.
    let persona_context = capped_field(body, "persona_context");
    let companion_context = capped_field(body, "companion_context");
    let world_context = capped_field(body, "world_context");
    let document_context = capped_document_field(body, "document_context");
    let document_name = field_or(body, "document_name", "");
    let sensitivity_tier = field_or(body, "sensitivity_tier", "internal");
    let live_call = crate::twin_live_gateway::prepare_live_call(
        kernel,
        &crate::twin_live_gateway::LiveAnswerRequest {
            actor_id: &actor_id,
            session_id: &session_id,
            companion_id: &companion_id,
            companion_stance: &companion_stance,
            prompt_shape: &prompt_shape,
            persona_context: &persona_context,
            companion_context: &companion_context,
            world_context: &world_context,
            document_context: &document_context,
            document_name: &document_name,
            sensitivity_tier: &sensitivity_tier,
            tenant_id: &tenant_id,
            draft_text: &draft_text,
        },
    );
    Ok(PreparedDraftPost {
        evidence_receipt_id,
        session_id,
        companion_id,
        companion_stance,
        persona_profile_id,
        prompt_shape,
        draft_text,
        tenant_id,
        actor_id,
        identity,
        resolved,
        live_call,
    })
}

pub(crate) fn complete_local_post(
    prepared: &PreparedDraftPost,
    live_answer: Option<TwinLiveAnswer>,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    let PreparedDraftPost {
        evidence_receipt_id,
        session_id,
        companion_id,
        companion_stance,
        persona_profile_id,
        prompt_shape,
        draft_text,
        tenant_id,
        actor_id,
        identity,
        resolved,
        ..
    } = prepared;
    // The recall packet rides ONLY when the prepared live call actually
    // answered: a deterministic fallback had no prompt, so no memory claim.
    let recall_packet = live_answer
        .as_ref()
        .and(prepared.live_call.as_ref())
        .map(|call| &call.recall);
    let report = kernel
        .save_twin_session_draft_local_with_identity(
            TwinSessionDraftRequest {
                tenant_id,
                actor_id,
                session_id,
                companion_id,
                companion_stance,
                persona_profile_id,
                prompt_shape,
                evidence_receipt_id,
                draft_text,
                live_answer: live_answer.as_ref(),
                recall_packet,
            },
            identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-twin-session-draft-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"session_id":{},"draft_text":{},"draft_receipt_id":{},"memory_consolidation_proposal_receipt_id":{},"memory_consolidation_review_receipt_id":{},"memory_gate_receipt_id":{},"memory_retrieval_receipt_id":{},"memory_scoring_receipt_id":{},"brain_recall_receipt_id":{},"recall_packet_id":{},"brain_recall_scope":{},"brain_recall_policy":{},"brain_recall_source_count":{},"brain_recall_token_budget":{},"memory_record_id":{},"answer_receipt_id":{},"persona_drift_receipt_id":{},"conversation_summary_receipt_id":{},"created_at":{},"grounded_answer":{},"conversation_summary":{},"memory_relevance_score":{},"memory_decay_state":{},"memory_decay_policy":{},"persona_contract_status":{},"voice_drift_status":{},"voice_drift_score":{},"world_model_source":{},"compaction_policy":{},"compaction_state":{},"model_gateway_driver":{},"model_gateway_provider":{},"model_gateway_model_id":{},"model_gateway_routing":{},"model_gateway_inference_id":{},"trusted_context_used":{},"trusted_context_source_count":{},"trusted_context_source_ids":{},"trusted_context_receipt_ids":{},"trusted_context_projection_route":{},"memory_driver":{},"memory_provider":{},"provider_call_allowed":{},"worker_spawn_allowed":{},"live_substrate_required":false,"production_write_allowed":false}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.session_id),
        json_string_literal(&report.draft_text),
        json_string_literal(&report.draft_receipt_id),
        json_string_literal(&report.memory_consolidation_proposal_receipt_id),
        json_string_literal(&report.memory_consolidation_review_receipt_id),
        json_string_literal(&report.memory_gate_receipt_id),
        json_string_literal(&report.memory_retrieval_receipt_id),
        json_string_literal(&report.memory_scoring_receipt_id),
        json_string_literal(&report.brain_recall_receipt_id),
        json_string_literal(&report.recall_packet_id),
        json_string_literal(report.brain_recall_scope),
        json_string_literal(report.brain_recall_policy),
        report.brain_recall_source_count,
        report.brain_recall_token_budget,
        json_string_literal(&report.memory_record_id),
        json_string_literal(&report.answer_receipt_id),
        json_string_literal(&report.persona_drift_receipt_id),
        json_string_literal(&report.conversation_summary_receipt_id),
        json_string_literal(&report.created_at),
        json_string_literal(&report.grounded_answer),
        json_string_literal(&report.conversation_summary),
        report.memory_relevance_score,
        json_string_literal(report.memory_decay_state),
        json_string_literal(report.memory_decay_policy),
        json_string_literal(report.persona_contract_status),
        json_string_literal(report.voice_drift_status),
        report.voice_drift_score,
        json_string_literal(report.world_model_source),
        json_string_literal(report.compaction_policy),
        json_string_literal(report.compaction_state),
        json_string_literal(&report.model_gateway_driver),
        json_string_literal(&report.model_gateway_provider),
        json_string_literal(&report.model_gateway_model_id),
        json_string_literal(&report.model_gateway_routing),
        json_string_literal(&report.model_gateway_inference_id),
        report.trusted_context_used,
        report.trusted_context_source_count,
        json_string_literal(&report.trusted_context_source_ids),
        json_string_literal(&report.trusted_context_receipt_ids),
        json_string_literal(report.trusted_context_projection_route),
        json_string_literal(report.memory_driver),
        json_string_literal(report.memory_provider),
        report.provider_call_allowed,
        report.worker_spawn_allowed
    ))
}

pub(crate) fn credential_receipt_id(kernel: &mut MdxKernel) -> Result<String, String> {
    if let Some(receipt_id) = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.kind == "credential.minted")
        .map(|receipt| receipt.receipt_id.clone())
    {
        return Ok(receipt_id);
    }
    kernel.run_evals_runner_agent()?;
    kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.kind == "credential.minted")
        .map(|receipt| receipt.receipt_id.clone())
        .ok_or_else(|| "local Twin draft POST route missing credential evidence".to_string())
}

pub(crate) fn field_or(body: &str, key: &str, default: &str) -> String {
    json_string_field(body, key).unwrap_or_else(|| default.to_string())
}

pub(crate) fn capped_field(body: &str, key: &str) -> String {
    json_string_field(body, key)
        .unwrap_or_default()
        .chars()
        .take(4000)
        .collect()
}

// Attached documents get a larger but still bounded prompt layer.
pub(crate) fn capped_document_field(body: &str, key: &str) -> String {
    json_string_field(body, key)
        .unwrap_or_default()
        .chars()
        .take(16000)
        .collect()
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let after_key = body.split(&format!("\"{key}\"")).nth(1)?;
    let after_colon = after_key.split_once(':')?.1.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            out.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Some(out);
        } else {
            out.push(character);
        }
    }
    None
}
