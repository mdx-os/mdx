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
fn twin_session_draft_cites_active_trusted_context_sources() {
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
    let source = kernel
        .save_twin_artifact_context_local(TwinArtifactContextRequest {
            tenant_id: "tenant_local",
            actor_id: "local_operator",
            session_id: "twin_context_source_session",
            artifact_name: "architecture-standard.md",
            mime_type: "text/markdown",
            artifact_text: "Twin answers should cite active trusted Context sources.",
            privacy_class: "user_private_sensitive",
            company_context: "Pages says trusted context must be receipt backed.",
            company_context_source: "Pages:Twin trusted answer standard",
        })
        .expect("source");
    let trust = kernel
        .save_pages_context_source_trust_decision_local(PagesContextSourceTrustDecision {
            tenant_id: "tenant_local",
            actor_id: "local_operator",
            trust_decision_id: "twin_trusted_answer_standard",
            source_id: &source.artifact_id,
            decision_outcome: "trusted_for_ai",
            decision_note: "Approved for Twin answers.",
            source_route: "/pages/context-sources/trust-decisions.json",
        })
        .expect("trust");

    let report = kernel
        .save_twin_session_draft_local(request(&evidence_receipt_id))
        .expect("save draft");

    assert!(report.trusted_context_used);
    assert_eq!(report.trusted_context_source_count, 1);
    assert_eq!(report.trusted_context_source_ids, source.artifact_id);
    assert_eq!(
        report.trusted_context_receipt_ids,
        trust.trust_decision_receipt_id
    );

    let answer = kernel
        .ledger()
        .query()
        .by_id(&report.answer_receipt_id)
        .expect("answer receipt");
    assert_eq!(
        answer
            .payload
            .get("trusted_context_policy")
            .map(String::as_str),
        Some("active_pages_context_trust_decisions_only")
    );
    assert_eq!(
        answer
            .payload
            .get("trusted_context_source_count")
            .map(String::as_str),
        Some("1")
    );
    assert_eq!(
        answer
            .payload
            .get("trusted_context_receipt_ids")
            .map(String::as_str),
        Some(trust.trust_decision_receipt_id.as_str())
    );

    kernel
        .delete_twin_artifact_local(TwinArtifactDeletionRequest {
            tenant_id: "tenant_local",
            actor_id: "local_operator",
            artifact_id: &source.artifact_id,
            reason: "Remove trusted context before another Twin answer.",
            local_blob_bytes_deleted: false,
        })
        .expect("delete source");

    let after_delete = kernel
        .save_twin_session_draft_local(TwinSessionDraftRequest {
            session_id: "twin_session_after_context_delete",
            ..request(&evidence_receipt_id)
        })
        .expect("save draft after delete");
    assert!(!after_delete.trusted_context_used);
    assert_eq!(after_delete.trusted_context_source_count, 0);
    assert!(after_delete.trusted_context_source_ids.is_empty());
    assert!(after_delete.trusted_context_receipt_ids.is_empty());
}
