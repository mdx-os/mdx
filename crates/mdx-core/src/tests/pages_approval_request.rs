use crate::*;

fn approval_test_draft(
    kernel: &mut MdxKernel,
    tenant_id: &str,
    document_id: &str,
    draft_id: &str,
) -> PagesEditDraftReport {
    let seed = kernel.run_evals_runner_agent().expect("seed evidence");
    let publication = kernel
        .save_pages_publication_local(PagesPublication {
            tenant_id,
            actor_id: "human:local_user",
            document_id,
            title: "Approval source",
            body_ref: "world_model://pages/approval-source/body/v1",
            source_receipt_id: &seed.receipts[0],
            revision_id: "rev_approval_source",
            page_type: "knowledge",
        })
        .expect("publication");
    kernel
        .save_pages_edit_draft_local(PagesEditDraft {
            tenant_id,
            actor_id: "human:local_user",
            draft_id,
            document_id,
            title: "Approval draft",
            body_ref: "world_model://pages/approval-source/body/draft",
            source_publication_receipt_id: &publication.publication_receipt_id,
            origin_receipt_id: "",
            origin_surface: "",
            revision_id: "rev_approval_draft",
        })
        .expect("edit draft")
}

#[test]
fn pages_approval_request_records_publication_blocked_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let seed = kernel.run_evals_runner_agent().expect("seed evidence");
    let publication = kernel
        .save_pages_publication_local(PagesPublication {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            document_id: "page_approval_source",
            title: "Approval Source",
            body_ref: "world_model://pages/page_approval_source/body/v1",
            source_receipt_id: &seed.receipts[0],
            revision_id: "rev_approval_source",
            page_type: "knowledge",
        })
        .expect("publication");
    let draft = kernel
        .save_pages_edit_draft_local(PagesEditDraft {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            draft_id: "draft_approval_source",
            document_id: "page_approval_source",
            title: "Approval Source",
            body_ref: "world_model://pages/page_approval_source/body/draft/v1",
            source_publication_receipt_id: &publication.publication_receipt_id,
            origin_receipt_id: "",
            origin_surface: "",
            revision_id: "rev_approval_draft",
        })
        .expect("edit draft");

    let report = kernel
        .save_pages_approval_request_local(PagesApprovalRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            approval_request_id: "approval_core_test",
            source_edit_draft_receipt_id: &draft.edit_draft_receipt_id,
            document_id: "page_approval_source",
            draft_id: "draft_approval_source",
            requested_visibility: "tenant_review",
        })
        .expect("approval request");

    assert_eq!(
        report.status,
        "PAGE_APPROVAL_REQUEST_RECORDED_PUBLICATION_BLOCKED"
    );
    assert_eq!(
        report.source_edit_draft_receipt_id,
        draft.edit_draft_receipt_id
    );
    assert!(!report.public_visibility_allowed);
    assert!(!report.search_indexing_allowed);
    assert!(!report.embedding_provider_allowed);
    assert!(!report.rich_editor_allowed);
    assert!(!report.standalone_store_allowed);
    assert!(!report.production_publish_allowed);
    assert!(!report.production_write_allowed);

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.approval_request_receipt_id)
        .expect("approval receipt");
    assert_eq!(receipt.kind, "pages.approval.requested");
    assert_eq!(receipt.payload["approval_request_id"], "approval_core_test");
    assert_eq!(
        receipt.payload["actor_admission_status"],
        "LOCAL_ACTOR_ADMITTED"
    );
    assert_eq!(
        receipt.payload["terminal_state"],
        "PAGE_APPROVAL_REQUEST_RECORDED_PUBLICATION_BLOCKED"
    );
}

#[test]
fn pages_approval_request_requires_known_edit_draft_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .save_pages_approval_request_local(PagesApprovalRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            approval_request_id: "approval_missing_source",
            source_edit_draft_receipt_id: "missing_receipt",
            document_id: "page_approval_source",
            draft_id: "draft_approval_source",
            requested_visibility: "tenant_review",
        })
        .expect_err("missing edit draft receipt should fail");

    assert!(matches!(
        error,
        PagesApprovalRequestError::UnknownEditDraftReceipt(_)
    ));
}

#[test]
fn pages_approval_request_requires_exact_draft_identity() {
    let mut kernel = MdxKernel::boot_local();
    let missing = kernel
        .save_pages_approval_request_local(PagesApprovalRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            approval_request_id: "approval_missing_exact_source",
            source_edit_draft_receipt_id: "",
            document_id: "page_exact_source",
            draft_id: "draft_exact_source",
            requested_visibility: "tenant_review",
        })
        .expect_err("an implicit draft must not be selected");
    assert!(matches!(missing, PagesApprovalRequestError::Missing(_)));

    let seed = kernel.run_evals_runner_agent().expect("seed evidence");
    let wrong_kind = kernel
        .save_pages_approval_request_local(PagesApprovalRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            approval_request_id: "approval_wrong_kind",
            source_edit_draft_receipt_id: &seed.receipts[0],
            document_id: "page_exact_source",
            draft_id: "draft_exact_source",
            requested_visibility: "tenant_review",
        })
        .expect_err("a non-draft receipt must not authorize review");
    assert!(matches!(
        wrong_kind,
        PagesApprovalRequestError::InvalidEditDraftReceipt(_)
    ));

    let draft = approval_test_draft(
        &mut kernel,
        "tenant_local",
        "page_exact_source",
        "draft_exact_source",
    );
    for (tenant_id, document_id, draft_id) in [
        ("tenant_other", "page_exact_source", "draft_exact_source"),
        ("tenant_local", "page_other", "draft_exact_source"),
        ("tenant_local", "page_exact_source", "draft_other"),
    ] {
        let error = kernel
            .save_pages_approval_request_local(PagesApprovalRequest {
                tenant_id,
                actor_id: "human:local_user",
                approval_request_id: "approval_mismatched_source",
                source_edit_draft_receipt_id: &draft.edit_draft_receipt_id,
                document_id,
                draft_id,
                requested_visibility: "tenant_review",
            })
            .expect_err("tenant, document, and draft must all match");
        assert!(matches!(
            error,
            PagesApprovalRequestError::InvalidEditDraftReceipt(_)
        ));
    }
}

#[test]
fn pages_approval_decision_refuses_stale_or_duplicate_requests() {
    let mut kernel = MdxKernel::boot_local();
    let first_draft = approval_test_draft(
        &mut kernel,
        "tenant_local",
        "page_decision_guard",
        "draft_decision_guard_v1",
    );
    let first_request = kernel
        .save_pages_approval_request_local(PagesApprovalRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            approval_request_id: "approval_decision_guard_v1",
            source_edit_draft_receipt_id: &first_draft.edit_draft_receipt_id,
            document_id: "page_decision_guard",
            draft_id: "draft_decision_guard_v1",
            requested_visibility: "tenant_review",
        })
        .expect("first approval request");
    let second_draft = approval_test_draft(
        &mut kernel,
        "tenant_local",
        "page_decision_guard",
        "draft_decision_guard_v2",
    );
    let second_request = kernel
        .save_pages_approval_request_local(PagesApprovalRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            approval_request_id: "approval_decision_guard_v2",
            source_edit_draft_receipt_id: &second_draft.edit_draft_receipt_id,
            document_id: "page_decision_guard",
            draft_id: "draft_decision_guard_v2",
            requested_visibility: "tenant_review",
        })
        .expect("second approval request");
    let stale = kernel
        .save_pages_approval_decision_local(PagesApprovalDecision {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            approval_decision_id: "decision_stale",
            approval_request_receipt_id: &first_request.approval_request_receipt_id,
            decision_outcome: "approved",
            decision_note: "This request is stale.",
            source_route: PAGES_APPROVAL_DECISION_APPROVE_ROUTE,
        })
        .expect_err("a superseded request must not be decided");
    assert!(matches!(
        stale,
        PagesApprovalRequestError::ApprovalRequestSuperseded(_)
    ));

    kernel
        .save_pages_approval_decision_local(PagesApprovalDecision {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            approval_decision_id: "decision_current",
            approval_request_receipt_id: &second_request.approval_request_receipt_id,
            decision_outcome: "approved",
            decision_note: "Approve the current draft.",
            source_route: PAGES_APPROVAL_DECISION_APPROVE_ROUTE,
        })
        .expect("current decision");
    let duplicate = kernel
        .save_pages_approval_decision_local(PagesApprovalDecision {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            approval_decision_id: "decision_duplicate",
            approval_request_receipt_id: &second_request.approval_request_receipt_id,
            decision_outcome: "rejected",
            decision_note: "A second decision is not allowed.",
            source_route: PAGES_APPROVAL_DECISION_REJECT_ROUTE,
        })
        .expect_err("a request must have one terminal decision");
    assert!(matches!(
        duplicate,
        PagesApprovalRequestError::ApprovalRequestAlreadyDecided(_)
    ));
}
