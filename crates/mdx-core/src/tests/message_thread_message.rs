use crate::*;

#[test]
fn message_thread_message_records_hot_path_safe_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let seed = kernel.run_evals_runner_agent().expect("seed evidence");
    let source_receipt_id = seed.receipts[0].clone();

    let report = kernel
        .save_message_thread_message_local(MessageThreadMessage {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            thread_id: "thread_local_receipts",
            channel_id: "local-ops",
            message_id: "message_core_test",
            content_type: "text",
            body: "Please keep moving on the governed write rails.",
            content_payload: "Please keep moving on the governed write rails.",
            source_receipt_id: &source_receipt_id,
            outbox_topic: "message.human.posted",
        })
        .expect("message thread message");

    assert_eq!(report.status, "MESSAGE_APPENDED_FANOUT_BLOCKED");
    assert_eq!(report.sequence, 1);
    assert_eq!(report.message_receipt_kind, "message.human.posted");
    assert_eq!(report.actor_type, "human");
    assert_eq!(report.actor_role, "operator");
    assert_eq!(report.content_type, "text");
    assert_eq!(
        report.content_payload,
        "Please keep moving on the governed write rails."
    );
    assert!(!report.llm_allowed_on_hot_path);
    assert!(!report.realtime_fanout_allowed);
    assert!(!report.production_write_allowed);
    assert!(!report.trusted_context_citation);
    assert!(report.trusted_context_source_id.is_empty());
    assert!(report.trusted_context_receipt_id.is_empty());

    let message = kernel
        .ledger()
        .query()
        .by_id(&report.message_receipt_id)
        .expect("message receipt");
    assert_eq!(message.kind, "message.human.posted");
    assert_eq!(
        message.payload["message_receipt_kind"],
        "message.human.posted"
    );
    assert_eq!(message.payload["thread_id"], "thread_local_receipts");
    assert_eq!(message.payload["content_type"], "text");
    assert_eq!(
        message.payload["content_payload"],
        "Please keep moving on the governed write rails."
    );
    assert_eq!(
        message.payload["body"],
        "Please keep moving on the governed write rails."
    );
    assert_eq!(message.payload["source_receipt_id"], source_receipt_id);
    assert_eq!(
        message.payload["source_receipt_kind"],
        "loop.action.policy_decided"
    );
    assert_eq!(message.payload["trusted_context_citation"], "false");
    assert_eq!(
        message.payload["actor_admission_status"],
        "LOCAL_ACTOR_ADMITTED"
    );
    assert_eq!(message.payload["llm_allowed_on_hot_path"], "false");
    assert_eq!(message.payload["realtime_fanout_allowed"], "false");
}

#[test]
fn message_thread_message_records_agent_authored_event_kind() {
    let mut kernel = MdxKernel::boot_local();
    let seed = kernel.run_evals_runner_agent().expect("seed evidence");
    let source_receipt_id = seed.receipts[0].clone();

    let report = kernel
        .save_message_thread_message_local(MessageThreadMessage {
            tenant_id: "tenant_local",
            actor_id: "agent:forge",
            thread_id: "thread_forge_agent",
            channel_id: "forge",
            message_id: "message_agent_core_test",
            content_type: "card",
            body: "Forge prepared a governed build plan.",
            content_payload: "{\"title\":\"Governed build plan\",\"state\":\"ready_for_review\"}",
            source_receipt_id: &source_receipt_id,
            outbox_topic: "message.agent.posted",
        })
        .expect("agent message thread message");

    assert_eq!(report.message_receipt_kind, "message.agent.posted");
    assert_eq!(report.actor_type, "agent");
    assert_eq!(report.actor_role, "agent");
    assert_eq!(report.content_type, "card");
    assert_eq!(report.sequence, 1);

    let message = kernel
        .ledger()
        .query()
        .by_id(&report.message_receipt_id)
        .expect("agent message receipt");
    assert_eq!(message.kind, "message.agent.posted");
    assert_eq!(message.payload["actor_type"], "agent");
    assert_eq!(message.payload["actor_role"], "agent");
    assert_eq!(message.payload["content_type"], "card");
    assert_eq!(
        message.payload["content_payload"],
        "{\"title\":\"Governed build plan\",\"state\":\"ready_for_review\"}"
    );
    assert_eq!(
        message.payload["actor_admission_status"],
        "LOCAL_ACTOR_ADMITTED"
    );
}

#[test]
fn message_thread_message_requires_source_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .save_message_thread_message_local(MessageThreadMessage {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            thread_id: "thread_local_receipts",
            channel_id: "local-ops",
            message_id: "message_core_missing_receipt",
            content_type: "text",
            body: "This should fail.",
            content_payload: "This should fail.",
            source_receipt_id: "missing_receipt",
            outbox_topic: "message.human.posted",
        })
        .expect_err("missing source receipt should fail");

    assert!(matches!(
        error,
        MessageThreadMessageError::UnknownSourceReceipt(_)
    ));
}

#[test]
fn message_thread_message_cites_only_active_trusted_context_sources() {
    let mut kernel = MdxKernel::boot_local();
    let source = kernel
        .save_twin_artifact_context_local(TwinArtifactContextRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            session_id: "message_context_source_session",
            artifact_name: "message-context-standard.md",
            mime_type: "text/markdown",
            artifact_text: "Message threads may cite trusted Context sources without becoming storage.",
            privacy_class: "user_private_sensitive",
            company_context: "Pages says Message citations must be receipt backed.",
            company_context_source: "Pages:Message citation standard",
        })
        .expect("source");
    let trust = kernel
        .save_pages_context_source_trust_decision_local(PagesContextSourceTrustDecision {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            trust_decision_id: "message_context_standard_trust",
            source_id: &source.artifact_id,
            decision_outcome: "trusted_for_ai",
            decision_note: "Approved for Message citation.",
            source_route: "/pages/context-sources/trust-decisions.json",
        })
        .expect("trust");

    let report = kernel
        .save_message_thread_message_local(MessageThreadMessage {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            thread_id: "thread_context_citations",
            channel_id: "local-ops",
            message_id: "message_context_citation",
            content_type: "text",
            body: "Sharing the trusted Context source for this discussion.",
            content_payload: "Sharing the trusted Context source for this discussion.",
            source_receipt_id: &trust.trust_decision_receipt_id,
            outbox_topic: "message.human.posted",
        })
        .expect("message citation");

    assert!(report.trusted_context_citation);
    assert_eq!(report.trusted_context_source_id, source.artifact_id);
    assert_eq!(
        report.trusted_context_receipt_id,
        trust.trust_decision_receipt_id
    );
    let message = kernel
        .ledger()
        .query()
        .by_id(&report.message_receipt_id)
        .expect("message receipt");
    assert_eq!(message.payload["trusted_context_citation"], "true");
    assert_eq!(
        message.payload["trusted_context_source_id"],
        source.artifact_id
    );
    assert_eq!(
        message.payload["trusted_context_receipt_id"],
        trust.trust_decision_receipt_id
    );

    kernel
        .delete_twin_artifact_local(TwinArtifactDeletionRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            artifact_id: &source.artifact_id,
            reason: "Remove trusted context before another Message citation.",
            local_blob_bytes_deleted: false,
        })
        .expect("delete source");

    let error = kernel
        .save_message_thread_message_local(MessageThreadMessage {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            thread_id: "thread_context_citations",
            channel_id: "local-ops",
            message_id: "message_deleted_context_citation",
            content_type: "text",
            body: "This should not cite deleted Context.",
            content_payload: "This should not cite deleted Context.",
            source_receipt_id: &trust.trust_decision_receipt_id,
            outbox_topic: "message.human.posted",
        })
        .expect_err("deleted trusted context should fail closed");

    assert!(matches!(
        error,
        MessageThreadMessageError::InactiveTrustedContextSource(_)
    ));
}
