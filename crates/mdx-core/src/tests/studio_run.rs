use crate::*;

fn seed_source_receipt(kernel: &mut MdxKernel) -> String {
    let seed = kernel.run_evals_runner_agent().expect("seed evidence");
    seed.receipts[0].clone()
}

fn document_run<'a>(source_receipt_id: &'a str) -> StudioDocumentRun<'a> {
    StudioDocumentRun {
        tenant_id: "tenant_local",
        actor_id: "human:local_user",
        run_id: "studio_run_local_001",
        goal: "Draft the Q3 board memo from our decisions",
        artifact_kind: "document",
        document_id: "page_studio_board_memo",
        title: "Q3 Board Memo",
        body_ref: "world_model://pages/page_studio_board_memo/body/v1",
        source_receipt_id,
        revision_id: "rev_studio_001",
    }
}

#[test]
fn studio_document_run_records_full_chain_and_lands_one_body_of_truth() {
    let mut kernel = MdxKernel::boot_local();
    let source_receipt_id = seed_source_receipt(&mut kernel);

    let report = kernel
        .run_studio_document_local(document_run(&source_receipt_id))
        .expect("studio document run");

    assert_eq!(report.status, "STUDIO_DOCUMENT_RUN_LANDED");
    assert_eq!(report.artifact_kind, "document");
    assert_eq!(report.kind_selection_mode, "propose_and_confirm");
    assert!(!report.standalone_document_store_allowed);
    assert!(!report.live_co_edit_allowed);
    assert!(!report.production_write_allowed);

    // The receipt chain is recorded in order, each a distinct receipt.
    let chain = [
        (&report.run_opened_receipt_id, "studio.run.opened"),
        (&report.kind_selected_receipt_id, "studio.kind.selected"),
        (
            &report.artifact_drafted_receipt_id,
            "studio.artifact.drafted",
        ),
        (&report.publication_receipt_id, "pages.document.published"),
        (&report.run_landed_receipt_id, "studio.run.landed"),
    ];
    let mut seen = std::collections::BTreeSet::new();
    for (receipt_id, expected_kind) in chain {
        let receipt = kernel
            .ledger()
            .query()
            .by_id(receipt_id)
            .unwrap_or_else(|| panic!("receipt {receipt_id} for {expected_kind} should exist"));
        assert_eq!(receipt.kind, expected_kind);
        assert!(seen.insert(receipt_id.clone()), "receipt ids are distinct");
    }
}

#[test]
fn studio_landing_is_a_pages_document_published_receipt_not_a_second_store() {
    let mut kernel = MdxKernel::boot_local();
    let source_receipt_id = seed_source_receipt(&mut kernel);

    let report = kernel
        .run_studio_document_local(document_run(&source_receipt_id))
        .expect("studio document run");

    // The landed artifact is the same receipt kind Pages publishes, so it is one
    // body of truth that shows up in the Pages projection.
    let publication = kernel
        .ledger()
        .query()
        .by_id(&report.publication_receipt_id)
        .expect("publication receipt");
    assert_eq!(publication.kind, "pages.document.published");
    assert_eq!(publication.payload["document_id"], "page_studio_board_memo");
    assert_eq!(publication.payload["page_type"], "knowledge");
    assert_eq!(
        publication.payload["body_ref"],
        "world_model://pages/page_studio_board_memo/body/v1"
    );
    assert_eq!(publication.payload["origin"], "studio");
    assert_eq!(publication.payload["studio_run_id"], "studio_run_local_001");
    assert_eq!(publication.payload["standalone_store_allowed"], "false");

    // The terminal landing receipt references the body-of-truth publication.
    let landed = kernel
        .ledger()
        .query()
        .by_id(&report.run_landed_receipt_id)
        .expect("run landed receipt");
    assert_eq!(
        landed.payload["publication_receipt_id"],
        report.publication_receipt_id
    );

    // Exactly one pages.document.published receipt exists: no shadow copy.
    let published = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "pages.document.published")
        .count();
    assert_eq!(published, 1);
}

#[test]
fn studio_document_run_requires_source_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let mut request = document_run("missing_receipt");
    request.source_receipt_id = "missing_receipt";
    let error = kernel
        .run_studio_document_local(request)
        .expect_err("missing source receipt should fail");
    assert!(matches!(error, StudioRunError::UnknownSourceReceipt(_)));
}

#[test]
fn studio_document_run_requires_fields() {
    let mut kernel = MdxKernel::boot_local();
    let source_receipt_id = seed_source_receipt(&mut kernel);
    let mut request = document_run(&source_receipt_id);
    request.goal = "   ";
    let error = kernel
        .run_studio_document_local(request)
        .expect_err("blank goal should fail");
    assert!(matches!(error, StudioRunError::Missing("goal")));
}

#[test]
fn studio_artifact_kind_defaults_and_refuses_unimplemented_and_unknown() {
    // Default and normalization.
    assert_eq!(normalize_studio_artifact_kind("").unwrap(), "document");
    assert_eq!(
        normalize_studio_artifact_kind("  DOCUMENT ").unwrap(),
        "document"
    );
    assert_eq!(normalize_studio_artifact_kind("sheet").unwrap(), "sheet");
    // An unknown kind is refused outright.
    assert!(matches!(
        normalize_studio_artifact_kind("memo"),
        Err(StudioRunError::InvalidKind(_))
    ));

    let mut kernel = MdxKernel::boot_local();
    let source_receipt_id = seed_source_receipt(&mut kernel);

    // Slice 4: a sheet is a real kind now and lands like any other Studio
    // artifact - one typed Pages object.
    let mut sheet = document_run(&source_receipt_id);
    sheet.artifact_kind = "sheet";
    let landed = kernel
        .run_studio_document_local(sheet)
        .expect("sheet lands in slice 4");
    assert_eq!(landed.artifact_kind, "sheet");

    // `code` is declared but opens in Forge, not Studio: refused as not landed here.
    let mut code = document_run(&source_receipt_id);
    code.run_id = "studio_run_code";
    code.document_id = "page_studio_code";
    code.artifact_kind = "code";
    let error = kernel
        .run_studio_document_local(code)
        .expect_err("code opens in Forge, not Studio");
    assert!(matches!(error, StudioRunError::KindNotImplemented(_)));
}
