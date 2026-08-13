use crate::*;

#[test]
fn pages_publication_records_projection_receipt_without_shadow_store() {
    let mut kernel = MdxKernel::boot_local();
    let seed = kernel.run_evals_runner_agent().expect("seed evidence");
    let source_receipt_id = seed.receipts[0].clone();

    let report = kernel
        .save_pages_publication_local(PagesPublication {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            document_id: "page_local_operator_note",
            title: "Local Operator Note",
            body_ref: "world_model://pages/page_local_operator_note/body/v1",
            source_receipt_id: &source_receipt_id,
            revision_id: "rev_local_ui_001",
            page_type: "decision",
        })
        .expect("pages publication");

    assert_eq!(report.status, "PAGE_PUBLISHED_EDITOR_BLOCKED");
    assert_eq!(report.page_type, "decision");
    assert_eq!(report.visibility, "tenant_only");
    assert!(!report.standalone_store_allowed);
    assert!(!report.rich_editor_allowed);
    assert!(!report.production_write_allowed);

    let publication = kernel
        .ledger()
        .query()
        .by_id(&report.publication_receipt_id)
        .expect("publication receipt");
    assert_eq!(publication.kind, "pages.document.published");
    assert_eq!(
        publication.payload["document_id"],
        "page_local_operator_note"
    );
    assert_eq!(publication.payload["source_receipt_id"], source_receipt_id);
    assert_eq!(publication.payload["page_type"], "decision");
    assert_eq!(
        publication.payload["actor_admission_status"],
        "LOCAL_ACTOR_ADMITTED"
    );
    assert_eq!(publication.payload["standalone_store_allowed"], "false");
    assert_eq!(publication.payload["rich_editor_allowed"], "false");
}

#[test]
fn pages_publication_requires_source_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .save_pages_publication_local(PagesPublication {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            document_id: "page_local_operator_note",
            title: "Local Operator Note",
            body_ref: "world_model://pages/page_local_operator_note/body/v1",
            source_receipt_id: "missing_receipt",
            revision_id: "rev_local_ui_001",
            page_type: "knowledge",
        })
        .expect_err("missing source receipt should fail");

    assert!(matches!(
        error,
        PagesPublicationError::UnknownSourceReceipt(_)
    ));
}

#[test]
fn pages_publication_page_type_defaults_and_validates() {
    // Empty type is the honest default `knowledge`; a typo outside the closed
    // vocabulary is refused rather than coerced into the wrong shape.
    assert_eq!(normalize_page_type("").unwrap(), "knowledge");
    assert_eq!(normalize_page_type("  SPEC ").unwrap(), "spec");
    assert!(matches!(
        normalize_page_type("memo"),
        Err(PagesPublicationError::InvalidType(_))
    ));

    let mut kernel = MdxKernel::boot_local();
    kernel.run_evals_runner_agent().expect("seed");
    let source_receipt_id = kernel
        .ledger()
        .entries()
        .first()
        .map(|receipt| receipt.receipt_id.clone())
        .expect("source receipt");
    let untyped = kernel
        .save_pages_publication_local(PagesPublication {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            document_id: "page_untyped_note",
            title: "Untyped Note",
            body_ref: "world_model://pages/page_untyped_note/body/v1",
            source_receipt_id: &source_receipt_id,
            revision_id: "rev_untyped_001",
            page_type: "",
        })
        .expect("untyped publication defaults");
    assert_eq!(untyped.page_type, "knowledge");
}

fn pages_review_chain(
    kernel: &mut MdxKernel,
    document_id: &str,
    outcome: &str,
) -> (PagesEditDraftReport, PagesApprovalDecisionReport) {
    let seed = kernel.run_evals_runner_agent().expect("seed evidence");
    let publication = kernel
        .save_pages_publication_local(PagesPublication {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            document_id,
            title: "Original title",
            body_ref: "world_model://pages/original/body/v1",
            source_receipt_id: &seed.receipts[0],
            revision_id: "rev_original",
            page_type: "knowledge",
        })
        .expect("original publication");
    let draft = kernel
        .save_pages_edit_draft_local(PagesEditDraft {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            draft_id: "draft_reviewed",
            document_id,
            title: "Approved title",
            body_ref: "world_model://pages/approved/body/v2",
            source_publication_receipt_id: &publication.publication_receipt_id,
            origin_receipt_id: "",
            origin_surface: "",
            revision_id: "rev_approved",
        })
        .expect("draft");
    let approval_request = kernel
        .save_pages_approval_request_local(PagesApprovalRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            approval_request_id: "approval_reviewed",
            source_edit_draft_receipt_id: &draft.edit_draft_receipt_id,
            document_id,
            draft_id: "draft_reviewed",
            requested_visibility: "tenant_review",
        })
        .expect("approval request");
    let decision = kernel
        .save_pages_approval_decision_local(PagesApprovalDecision {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            approval_decision_id: "decision_reviewed",
            approval_request_receipt_id: &approval_request.approval_request_receipt_id,
            decision_outcome: outcome,
            decision_note: "Reviewed the exact saved draft.",
            source_route: "/pages/approval-decisions/approve.json",
        })
        .expect("decision");
    (draft, decision)
}

#[test]
fn approved_publication_uses_the_exact_draft_chain() {
    let mut kernel = MdxKernel::boot_local();
    let (draft, decision) = pages_review_chain(&mut kernel, "page_reviewed", "approved");

    let report = kernel
        .save_pages_approved_publication_local(PagesApprovedPublication {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            document_id: "page_reviewed",
            approval_decision_receipt_id: &decision.approval_decision_receipt_id,
            page_type: "decision",
        })
        .expect("approved publication");

    assert_eq!(report.title, "Approved title");
    assert_eq!(report.body_ref, "world_model://pages/approved/body/v2");
    assert_eq!(report.revision_id, "rev_approved");
    assert_eq!(report.source_receipt_id, draft.edit_draft_receipt_id);
    assert_eq!(
        report.approval_decision_receipt_id,
        decision.approval_decision_receipt_id
    );
    assert_eq!(report.approval_binding, "approved_draft_content_bound");
    assert_eq!(report.reviewer_independence, "self_review_permitted");

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.publication_receipt_id)
        .expect("publication receipt");
    assert_eq!(receipt.payload["human_approval_granted"], "true");
    assert_eq!(
        receipt.payload["source_edit_draft_receipt_id"],
        draft.edit_draft_receipt_id
    );
}

#[test]
fn approved_publication_preserves_the_message_origin_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let seed = kernel.run_evals_runner_agent().expect("seed evidence");
    let message = kernel
        .save_message_thread_message_local(MessageThreadMessage {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            thread_id: "thread_page_origin",
            channel_id: "general",
            message_id: "message_page_origin",
            content_type: "text",
            body: "Ship the launch checklist after review.",
            content_payload: "Ship the launch checklist after review.",
            source_receipt_id: &seed.receipts[0],
            outbox_topic: "message.human.posted",
        })
        .expect("message");
    let draft = kernel
        .save_pages_edit_draft_local(PagesEditDraft {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            draft_id: "draft_from_message",
            document_id: "page_from_message",
            title: "Launch checklist",
            body_ref: "world_model://pages/from-message/body/v1",
            source_publication_receipt_id: "",
            origin_receipt_id: &message.message_receipt_id,
            origin_surface: "message",
            revision_id: "rev_from_message",
        })
        .expect("draft from message");
    let request = kernel
        .save_pages_approval_request_local(PagesApprovalRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            approval_request_id: "approval_from_message",
            source_edit_draft_receipt_id: &draft.edit_draft_receipt_id,
            document_id: "page_from_message",
            draft_id: "draft_from_message",
            requested_visibility: "tenant_review",
        })
        .expect("approval request");
    let decision = kernel
        .save_pages_approval_decision_local(PagesApprovalDecision {
            tenant_id: "tenant_local",
            actor_id: "human:reviewer",
            approval_decision_id: "decision_from_message",
            approval_request_receipt_id: &request.approval_request_receipt_id,
            decision_outcome: "approved",
            decision_note: "The page matches the source message.",
            source_route: "/pages/approval-decisions/approve.json",
        })
        .expect("approval decision");
    let publication = kernel
        .save_pages_approved_publication_local(PagesApprovedPublication {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            document_id: "page_from_message",
            approval_decision_receipt_id: &decision.approval_decision_receipt_id,
            page_type: "decision",
        })
        .expect("approved publication");

    assert_eq!(publication.origin_receipt_id, message.message_receipt_id);
    assert_eq!(publication.origin_surface, "message");
    let receipt = kernel
        .ledger()
        .query()
        .by_id(&publication.publication_receipt_id)
        .expect("publication receipt");
    assert_eq!(
        receipt.payload["origin_receipt_id"],
        message.message_receipt_id
    );
    assert_eq!(receipt.payload["origin_surface"], "message");
    assert_eq!(receipt.payload["origin_grants_authority"], "false");
}

#[test]
fn rejected_or_replayed_approval_cannot_publish() {
    let mut rejected_kernel = MdxKernel::boot_local();
    let (_, rejected) = pages_review_chain(&mut rejected_kernel, "page_rejected", "rejected");
    assert!(matches!(
        rejected_kernel.save_pages_approved_publication_local(PagesApprovedPublication {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            document_id: "page_rejected",
            approval_decision_receipt_id: &rejected.approval_decision_receipt_id,
            page_type: "knowledge",
        }),
        Err(PagesPublicationError::ApprovalNotGranted(_))
    ));

    let mut approved_kernel = MdxKernel::boot_local();
    let (_, approved) = pages_review_chain(&mut approved_kernel, "page_once", "approved");
    let request = PagesApprovedPublication {
        tenant_id: "tenant_local",
        actor_id: "human:local_user",
        document_id: "page_once",
        approval_decision_receipt_id: &approved.approval_decision_receipt_id,
        page_type: "knowledge",
    };
    approved_kernel
        .save_pages_approved_publication_local(request.clone())
        .expect("first publication");
    assert!(matches!(
        approved_kernel.save_pages_approved_publication_local(request),
        Err(PagesPublicationError::ApprovalAlreadyConsumed(_))
    ));
}

#[test]
fn changed_draft_supersedes_an_old_approval() {
    let mut kernel = MdxKernel::boot_local();
    let (approved_draft, decision) = pages_review_chain(&mut kernel, "page_changed", "approved");
    kernel
        .save_pages_edit_draft_local(PagesEditDraft {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            draft_id: "draft_changed_after_review",
            document_id: "page_changed",
            title: "Words changed after approval",
            body_ref: "world_model://pages/changed/body/v3",
            source_publication_receipt_id: "",
            origin_receipt_id: "",
            origin_surface: "",
            revision_id: "rev_changed_after_review",
        })
        .expect("newer draft");

    let before = kernel
        .ledger()
        .query()
        .by_kind("pages.document.published")
        .len();
    assert!(matches!(
        kernel.save_pages_approved_publication_local(PagesApprovedPublication {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            document_id: "page_changed",
            approval_decision_receipt_id: &decision.approval_decision_receipt_id,
            page_type: "knowledge",
        }),
        Err(PagesPublicationError::ApprovalSuperseded(_))
    ));
    assert_eq!(
        kernel
            .ledger()
            .query()
            .by_kind("pages.document.published")
            .len(),
        before
    );
    assert!(!approved_draft.edit_draft_receipt_id.is_empty());
}
