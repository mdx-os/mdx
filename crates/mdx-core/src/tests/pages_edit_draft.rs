use crate::*;

#[test]
fn pages_edit_draft_records_draft_without_editor_authority() {
    let mut kernel = MdxKernel::boot_local();
    let seed = kernel.run_evals_runner_agent().expect("seed evidence");
    let publication = kernel
        .save_pages_publication_local(PagesPublication {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            document_id: "page_edit_source",
            title: "Page Edit Source",
            body_ref: "world_model://pages/page_edit_source/body/v1",
            source_receipt_id: &seed.receipts[0],
            revision_id: "rev_source",
            page_type: "knowledge",
        })
        .expect("publication");

    let report = kernel
        .save_pages_edit_draft_local(PagesEditDraft {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            draft_id: "page_draft_core_test",
            document_id: "page_edit_source",
            title: "Page Edit Draft",
            body_ref: "world_model://pages/page_edit_source/body/v2",
            source_publication_receipt_id: &publication.publication_receipt_id,
            origin_receipt_id: "",
            origin_surface: "",
            revision_id: "rev_draft",
        })
        .expect("edit draft");

    assert_eq!(report.status, "PAGE_EDIT_DRAFT_SAVED_EDITOR_BLOCKED");
    assert_eq!(
        report.source_publication_receipt_id,
        publication.publication_receipt_id
    );
    assert!(!report.rich_editor_allowed);
    assert!(!report.approval_rail_allowed);
    assert!(!report.standalone_store_allowed);
    assert!(!report.production_publish_allowed);

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.edit_draft_receipt_id)
        .expect("draft receipt");
    assert_eq!(receipt.kind, "pages.edit.draft.saved");
    assert_eq!(receipt.payload["draft_id"], "page_draft_core_test");
    assert_eq!(
        receipt.payload["actor_admission_status"],
        "LOCAL_ACTOR_ADMITTED"
    );
    assert_eq!(
        receipt.payload["terminal_state"],
        "PAGE_EDIT_DRAFT_SAVED_EDITOR_BLOCKED"
    );
}

#[test]
fn pages_edit_draft_requires_known_publication_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .save_pages_edit_draft_local(PagesEditDraft {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            draft_id: "page_draft_missing_source",
            document_id: "page_edit_source",
            title: "Page Edit Draft",
            body_ref: "world_model://pages/page_edit_source/body/v2",
            source_publication_receipt_id: "missing_receipt",
            origin_receipt_id: "",
            origin_surface: "",
            revision_id: "rev_draft",
        })
        .expect_err("missing source should fail");

    assert!(matches!(
        error,
        PagesEditDraftError::UnknownPublicationReceipt(_)
    ));
}

#[test]
fn new_pages_draft_does_not_fabricate_a_publication_source() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .save_pages_edit_draft_local(PagesEditDraft {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            draft_id: "page_new_draft",
            document_id: "page_new_document",
            title: "New document",
            body_ref: "world_model://pages/page_new_document/body/draft",
            source_publication_receipt_id: "",
            origin_receipt_id: "",
            origin_surface: "",
            revision_id: "rev_new_draft",
        })
        .expect("new draft");
    assert!(report.source_publication_receipt_id.is_empty());
    assert!(
        kernel
            .ledger()
            .query()
            .by_kind("pages.document.published")
            .is_empty()
    );
}

#[test]
fn pages_edit_draft_rejects_a_publication_from_another_document() {
    let mut kernel = MdxKernel::boot_local();
    let seed = kernel.run_evals_runner_agent().expect("seed evidence");
    let publication = kernel
        .save_pages_publication_local(PagesPublication {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            document_id: "page_other_document",
            title: "Other document",
            body_ref: "world_model://pages/page_other_document/body/v1",
            source_receipt_id: &seed.receipts[0],
            revision_id: "rev_other_document",
            page_type: "knowledge",
        })
        .expect("publication");
    let error = kernel
        .save_pages_edit_draft_local(PagesEditDraft {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            draft_id: "page_mismatched_draft",
            document_id: "page_named_document",
            title: "Named document",
            body_ref: "world_model://pages/page_named_document/body/draft",
            source_publication_receipt_id: &publication.publication_receipt_id,
            origin_receipt_id: "",
            origin_surface: "",
            revision_id: "rev_named_document",
        })
        .expect_err("cross-document publication source must fail");
    assert!(matches!(
        error,
        PagesEditDraftError::InvalidPublicationReceipt(_)
    ));
}
