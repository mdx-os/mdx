use super::*;
fn request<'a>(evidence_receipt_id: &'a str) -> TwinSessionDraftRequest<'a> {
    TwinSessionDraftRequest {
        tenant_id: "tenant_local",
        actor_id: "local_operator",
        session_id: "twin_session_test",
        companion_id: "twin_architect",
        companion_stance: "risk_and_architecture",
        persona_profile_id: "architect_stress_tests_assumptions",
        prompt_shape: "help me test the draft rail",
        evidence_receipt_id,
        draft_text: "Save this as a governed local draft.",
        live_answer: None,
        recall_packet: None,
    }
}

#[test]
fn twin_session_draft_writes_receipt_backed_memory() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("seed evidence");
    let evidence_receipt_id = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.kind == "credential.minted")
        .expect("credential evidence")
        .receipt_id
        .clone();
    let starting_memory = kernel.memory_records().len();

    let report = kernel
        .save_twin_session_draft_local(request(&evidence_receipt_id))
        .expect("save draft");

    assert_eq!(report.status, "SAVED_LOCAL_DRAFT");
    assert_eq!(report.model_gateway_model_id, "deterministic_local_v1");
    assert_eq!(report.model_gateway_routing, "single_deterministic_stub");
    assert_eq!(report.memory_driver, LOCAL_MEMORY_DRIVER.driver_id);
    assert_eq!(report.memory_provider, LOCAL_MEMORY_DRIVER.provider);
    assert!(!report.provider_call_allowed);
    assert!(!report.worker_spawn_allowed);
    assert!(!report.trusted_context_used);
    assert_eq!(report.trusted_context_source_count, 0);
    assert!(report.trusted_context_source_ids.is_empty());
    assert!(report.trusted_context_receipt_ids.is_empty());
    assert_eq!(kernel.memory_records().len(), starting_memory + 1);
    let memory = kernel.memory_records().last().expect("draft memory");
    assert_eq!(memory.source_receipt_id, report.draft_receipt_id);
    assert_eq!(memory.episode_id, format!("episode_{}", memory.memory_id));
    assert_eq!(memory.memory_scope, "private_user_memory");
    assert_eq!(memory.memory_tier, "episodic");
    assert_eq!(memory.decay_policy, "local_recent_session_decay_v1");
    assert_eq!(memory.importance_score, 70);
    let draft = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.receipt_id == report.draft_receipt_id)
        .expect("draft receipt");
    assert_eq!(
        draft.payload["actor_admission_status"],
        "LOCAL_ACTOR_ADMITTED"
    );
    assert_eq!(memory.provenance.driver_id, LOCAL_MEMORY_DRIVER.driver_id);
    assert_eq!(
        memory.provenance.source_receipt_kind,
        "twin.session.draft.admitted"
    );
    assert_eq!(
        memory.provenance.gate_receipt_id,
        report.memory_gate_receipt_id
    );
    let answer = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.receipt_id == report.answer_receipt_id)
        .expect("answer receipt");
    assert_eq!(answer.kind, "twin.session.answer.grounded");
    assert_eq!(
        answer
            .payload
            .get("model_gateway_driver")
            .map(String::as_str),
        Some("local_model_gateway")
    );
    assert_eq!(
        answer
            .payload
            .get("trusted_context_used")
            .map(String::as_str),
        Some("false")
    );
    let retrieval = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.receipt_id == report.memory_retrieval_receipt_id)
        .expect("memory retrieval receipt");
    assert_eq!(retrieval.kind, "twin.session.memory.retrieved");
    assert_eq!(
        retrieval
            .payload
            .get("retrieval_driver")
            .map(String::as_str),
        Some("local_memory_store")
    );
    let scoring = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.receipt_id == report.memory_scoring_receipt_id)
        .expect("memory scoring receipt");
    assert_eq!(scoring.kind, "twin.session.memory.scored");
    // No recall packet rode this save: no memory reached any prompt, and
    // the score says so instead of wearing a constant.
    assert_eq!(report.memory_relevance_score, 0);
    assert_eq!(
        scoring
            .payload
            .get("memory_relevance_basis")
            .map(String::as_str),
        Some("no_memory_in_live_prompt")
    );
    assert_eq!(
        scoring
            .payload
            .get("memory_decay_policy")
            .map(String::as_str),
        Some("local_recent_session_decay_v1")
    );
    assert_eq!(
        scoring
            .payload
            .get("world_model_source")
            .map(String::as_str),
        Some("generated/world-model/pages-projection-fixtures.json")
    );
    let brain_recall = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.receipt_id == report.brain_recall_receipt_id)
        .expect("Brain recall preflight receipt");
    assert_eq!(brain_recall.kind, "twin.session.brain_recall.preflighted");
    assert_eq!(report.brain_recall_scope, "private_user_session");
    assert_eq!(report.brain_recall_policy, "local_brain_recall_packet_v1");
    // Deterministic save: zero sources, empty included list, zero bytes.
    assert_eq!(report.brain_recall_source_count, 0);
    assert_eq!(report.brain_recall_token_budget, 2400);
    for (key, expected) in [
        ("included_sources", ""),
        ("recall_memory_ids", ""),
        ("brain_recall_context_bytes", "0"),
        ("prompt_context_build_ms", "0"),
        ("top_recall_score", "0"),
        ("stale_memory_count", "0"),
        ("superseded_memory_count", "0"),
        ("private_memory_prompt_context", "false"),
        ("private_memory_export_allowed", "false"),
        ("runtime_influence_scope", "prompt_context_only"),
        ("adaptation_allowed", "false"),
    ] {
        assert_eq!(
            brain_recall.payload.get(key).map(String::as_str),
            Some(expected),
            "{key}"
        );
    }
    assert_eq!(
        brain_recall
            .payload
            .get("recall_packet_id")
            .map(String::as_str),
        Some(report.recall_packet_id.as_str())
    );
    let drift = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.receipt_id == report.persona_drift_receipt_id)
        .expect("persona drift receipt");
    assert_eq!(drift.kind, "twin.session.persona.drift_checked");
    assert_eq!(report.persona_contract_status, "MATCHED_DECLARED_STANCE");
    assert_eq!(report.voice_drift_status, "IN_BOUNDS");
    assert_eq!(report.voice_drift_score, 0);
    let summary = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.receipt_id == report.conversation_summary_receipt_id)
        .expect("conversation summary receipt");
    assert_eq!(summary.kind, "twin.session.conversation.summarized");
    assert_eq!(
        summary.payload.get("message_count").map(String::as_str),
        Some("2")
    );
}

#[test]
fn twin_recall_packet_lands_real_values_on_the_brain_recall_receipt() {
    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("seed evidence");
    let evidence_receipt_id = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.kind == "credential.minted")
        .expect("credential evidence")
        .receipt_id
        .clone();
    // A prior conversation retained as active private memory.
    kernel
        .save_twin_session_draft_local(TwinSessionDraftRequest {
            session_id: "twin_session_prior",
            ..request(&evidence_receipt_id)
        })
        .expect("prior draft");
    // What prepare time computes: the read-only ranking over PRIOR records.
    let candidates = kernel.rank_memory_recall(
        "twin",
        "governed local draft",
        "private_user_memory",
        "tenant_local",
    );
    assert!(!candidates.is_empty(), "prior memory should rank");
    let packet = TwinRecallPacket {
        sources: candidates.clone(),
        lesson_count: 2,
        memory_bytes: 480,
        lesson_bytes: 220,
        build_ms: 3,
    };
    let report = kernel
        .save_twin_session_draft_local(TwinSessionDraftRequest {
            session_id: "twin_session_packet",
            recall_packet: Some(&packet),
            ..request(&evidence_receipt_id)
        })
        .expect("packet draft");
    // The receipt carries the packet's observed values - the constants are gone.
    let expected_score = (candidates[0].final_score / 7).min(100) as u8;
    assert_eq!(report.memory_relevance_score, expected_score);
    assert!(report.memory_relevance_score > 0);
    assert_eq!(
        report.brain_recall_source_count,
        (candidates.len() + 2) as u8
    );
    let brain_recall = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.receipt_id == report.brain_recall_receipt_id)
        .expect("brain recall receipt")
        .clone();
    let expected_ids = candidates
        .iter()
        .map(|candidate| candidate.memory_id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let expected_top = candidates[0].final_score.to_string();
    for (key, expected) in [
        ("included_sources", "session_memory,learn_active_memory"),
        ("recall_memory_ids", expected_ids.as_str()),
        ("top_recall_score", expected_top.as_str()),
        ("brain_recall_context_bytes", "700"),
        ("prompt_context_build_ms", "3"),
        ("private_memory_prompt_context", "true"),
        ("private_memory_export_allowed", "false"),
    ] {
        assert_eq!(
            brain_recall.payload.get(key).map(String::as_str),
            Some(expected),
            "{key}"
        );
    }
    // The packet content itself never lands on the receipt.
    assert!(
        brain_recall
            .payload
            .values()
            .all(|value| !value.contains("Save this as a governed local draft"))
    );
    // learn_active_memory is claimed ONLY when lessons actually rode.
    let no_lessons = TwinRecallPacket {
        lesson_count: 0,
        ..packet.clone()
    };
    let report = kernel
        .save_twin_session_draft_local(TwinSessionDraftRequest {
            session_id: "twin_session_no_lessons",
            recall_packet: Some(&no_lessons),
            ..request(&evidence_receipt_id)
        })
        .expect("no-lesson draft");
    let brain_recall = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.receipt_id == report.brain_recall_receipt_id)
        .expect("brain recall receipt");
    assert_eq!(
        brain_recall
            .payload
            .get("included_sources")
            .map(String::as_str),
        Some("session_memory")
    );
}

#[test]
fn twin_session_draft_fails_closed_without_evidence_receipt() {
    let mut kernel = MdxKernel::boot_local();

    let error = kernel
        .save_twin_session_draft_local(request("missing_receipt"))
        .expect_err("missing evidence blocks draft");

    assert_eq!(
        error.message(),
        "twin session draft evidence receipt missing_receipt is unknown"
    );
    assert!(kernel.memory_records().is_empty());
    assert!(
        kernel
            .ledger()
            .entries()
            .iter()
            .all(|receipt| receipt.kind != "twin.session.draft.admitted")
    );
}

#[test]
#[rustfmt::skip]
fn twin_boundary_refusals_record_local_denials_and_reject_unknown_source() {
    let mut kernel = MdxKernel::boot_local();
    let request = TwinBoundaryRefusalRequest { tenant_id: "tenant_local", actor_id: "local_operator", session_id: "twin_boundary_session", source_draft_receipt_id: "" };
    let provider = kernel.save_twin_provider_call_refusal_local(request.clone()).expect("provider refusal");
    let memory = kernel.save_twin_production_memory_refusal_local(TwinBoundaryRefusalRequest { source_draft_receipt_id: &provider.source_draft_receipt_id, ..request.clone() }).expect("memory refusal");
    let tool = kernel.save_twin_tool_execution_refusal_local(TwinBoundaryRefusalRequest { source_draft_receipt_id: &provider.source_draft_receipt_id, ..request }).expect("tool refusal");
    for (report, capability, kind) in [(&provider, "live_provider_call", "twin.session.provider_call.refused"), (&memory, "production_memory_write", "twin.session.production_memory.refused"), (&tool, "tool_execution", "twin.session.tool_execution.refused")] { let receipt = kernel.ledger().entries().iter().find(|receipt| receipt.receipt_id == report.refusal_receipt_id).expect("refusal receipt"); assert_eq!(receipt.kind, kind); assert_eq!(receipt.payload["refused_capability"], capability); assert_eq!(receipt.payload["source_draft_receipt_id"], provider.source_draft_receipt_id); assert_eq!(receipt.payload["production_write_allowed"], "false"); }
    let error = kernel.save_twin_provider_call_refusal_local(TwinBoundaryRefusalRequest { tenant_id: "tenant_local", actor_id: "local_operator", session_id: "twin_boundary_session", source_draft_receipt_id: "missing_draft" }).expect_err("unknown source blocks");
    assert_eq!(error.message(), "twin boundary refusal source receipt missing_draft is unknown");
}
