use crate::*;

const TENANT: &str = "tenant_local";
const ACTOR: &str = "human:local_user";

fn seed(kernel: &mut MdxKernel) -> String {
    kernel.run_evals_runner_agent().expect("seed").receipts[0].clone()
}

fn open(kernel: &mut MdxKernel, run_id: &str, source: &str) {
    kernel
        .open_studio_run(StudioOpenRun {
            tenant_id: TENANT,
            actor_id: ACTOR,
            run_id,
            goal: "Draft the board memo from our decisions",
            artifact_kind: "document",
            source_receipt_id: source,
            source_kind: "",
            source_title: "",
        })
        .expect("open studio run");
}

fn control<'a>(run_id: &'a str, key: &'a str) -> StudioControl<'a> {
    StudioControl {
        tenant_id: TENANT,
        actor_id: ACTOR,
        run_id,
        operator_id: ACTOR,
        idempotency_key: key,
    }
}

fn landing<'a>(run_id: &'a str, key: &'a str, document_id: &'a str) -> StudioLanding<'a> {
    StudioLanding {
        tenant_id: TENANT,
        actor_id: ACTOR,
        run_id,
        operator_id: ACTOR,
        idempotency_key: key,
        document_id,
        title: "Q3 Board Memo",
        body_ref: "world_model://pages/page_steer_memo/body/v1",
        revision_id: "rev_steer_001",
    }
}

fn has_receipt(kernel: &MdxKernel, run_id: &str, kind: &str) -> bool {
    kernel.ledger().entries().iter().any(|receipt| {
        receipt.kind == kind
            && receipt.payload.get("studio_run_id").map(String::as_str) == Some(run_id)
    })
}

#[test]
fn steerable_run_opens_pauses_steers_resumes_and_lands() {
    let mut kernel = MdxKernel::boot_local();
    let source = seed(&mut kernel);
    let run = "studio_run_steer_1";

    open(&mut kernel, run, &source);
    assert_eq!(kernel.studio_run_state(run), StudioRunState::Drafting);

    let paused = kernel
        .pause_studio_run(control(run, "pause_1"))
        .expect("pause");
    assert_eq!(paused.status, "STUDIO_RUN_PAUSED");
    assert!(!paused.refused);
    assert_eq!(kernel.studio_run_state(run), StudioRunState::Paused);

    let steered = kernel
        .steer_studio_run(StudioSteering {
            tenant_id: TENANT,
            actor_id: ACTOR,
            run_id: run,
            operator_id: ACTOR,
            idempotency_key: "steer_1",
            instruction: "Lead with the risk",
            target_scope: "document",
            reason: "the board reads risk first",
            source_refs: "",
        })
        .expect("steer");
    assert_eq!(steered.status, "STUDIO_STEERING_RECORDED");
    assert!(!steered.refused);
    assert_eq!(kernel.studio_run_state(run), StudioRunState::Paused);

    let resumed = kernel
        .resume_studio_run(control(run, "resume_1"))
        .expect("resume");
    assert_eq!(resumed.status, "STUDIO_RUN_RESUMED");
    assert_eq!(kernel.studio_run_state(run), StudioRunState::Drafting);

    let landed = kernel
        .land_studio_run(landing(run, "land_1", "page_steer_memo"))
        .expect("land");
    assert_eq!(landed.status, "STUDIO_DOCUMENT_RUN_LANDED");
    assert!(!landed.refused);
    assert_eq!(kernel.studio_run_state(run), StudioRunState::Landed);

    for kind in [
        "studio.run.opened",
        "studio.run.pause.accepted",
        "studio.run.steering.added",
        "studio.run.resume.accepted",
        "pages.document.published",
        "studio.run.landed",
    ] {
        assert!(has_receipt(&kernel, run, kind), "chain missing {kind}");
    }
}

#[test]
fn steered_landing_is_one_body_of_truth_in_pages() {
    let mut kernel = MdxKernel::boot_local();
    let source = seed(&mut kernel);
    let run = "studio_run_steer_2";
    open(&mut kernel, run, &source);
    kernel.pause_studio_run(control(run, "p")).expect("pause");
    kernel.resume_studio_run(control(run, "r")).expect("resume");
    kernel
        .land_studio_run(landing(run, "l", "page_steer_two"))
        .expect("land");

    let published = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "pages.document.published")
        .collect::<Vec<_>>();
    assert_eq!(published.len(), 1, "exactly one body of truth");
    assert_eq!(published[0].payload["origin"], "studio");
    assert_eq!(published[0].payload["document_id"], "page_steer_two");
}

#[test]
fn controls_are_refused_after_a_terminal_state() {
    let mut kernel = MdxKernel::boot_local();
    let source = seed(&mut kernel);
    let run = "studio_run_terminal";
    open(&mut kernel, run, &source);
    kernel
        .land_studio_run(landing(run, "l", "page_terminal"))
        .expect("land");
    assert_eq!(kernel.studio_run_state(run), StudioRunState::Landed);

    let paused = kernel
        .pause_studio_run(control(run, "late_pause"))
        .expect("pause call");
    assert!(paused.refused);
    assert_eq!(paused.refusal_reason, "run already landed");
    assert!(has_receipt(&kernel, run, "studio.run.steering.refused"));
    // The run stays landed; refusal performed no partial execution.
    assert_eq!(kernel.studio_run_state(run), StudioRunState::Landed);
}

#[test]
fn pause_is_idempotent_on_the_same_key() {
    let mut kernel = MdxKernel::boot_local();
    let source = seed(&mut kernel);
    let run = "studio_run_idem_pause";
    open(&mut kernel, run, &source);

    let first = kernel
        .pause_studio_run(control(run, "same_key"))
        .expect("first pause");
    let second = kernel
        .pause_studio_run(control(run, "same_key"))
        .expect("replay pause");
    assert!(!first.idempotent_replay);
    assert!(second.idempotent_replay);
    assert_eq!(first.receipt_id, second.receipt_id);

    let accepted = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| {
            receipt.kind == "studio.run.pause.accepted"
                && receipt.payload.get("studio_run_id").map(String::as_str) == Some(run)
        })
        .count();
    assert_eq!(accepted, 1, "replay records no second pause");
}

#[test]
fn steering_is_refused_off_a_paused_run() {
    let mut kernel = MdxKernel::boot_local();
    let source = seed(&mut kernel);
    let run = "studio_run_not_paused";
    open(&mut kernel, run, &source);

    let steered = kernel
        .steer_studio_run(StudioSteering {
            tenant_id: TENANT,
            actor_id: ACTOR,
            run_id: run,
            operator_id: ACTOR,
            idempotency_key: "s1",
            instruction: "do a thing",
            target_scope: "document",
            reason: "because",
            source_refs: "",
        })
        .expect("steer call");
    assert!(steered.refused);
    assert_eq!(kernel.studio_run_state(run), StudioRunState::Drafting);
}

#[test]
fn landing_a_paused_run_is_refused_until_resumed() {
    let mut kernel = MdxKernel::boot_local();
    let source = seed(&mut kernel);
    let run = "studio_run_paused_land";
    open(&mut kernel, run, &source);
    kernel.pause_studio_run(control(run, "p")).expect("pause");

    let refused = kernel
        .land_studio_run(landing(run, "l", "page_paused"))
        .expect("land call");
    assert!(refused.refused);
    assert_eq!(refused.refusal_reason, "resume the run before landing it");
    assert!(!has_receipt(&kernel, run, "pages.document.published"));

    kernel.resume_studio_run(control(run, "r")).expect("resume");
    let landed = kernel
        .land_studio_run(landing(run, "l2", "page_paused"))
        .expect("land");
    assert!(!landed.refused);
    assert_eq!(kernel.studio_run_state(run), StudioRunState::Landed);
}

#[test]
fn reusing_an_idempotency_key_with_a_different_payload_is_refused() {
    let mut kernel = MdxKernel::boot_local();
    let source = seed(&mut kernel);
    let run = "studio_run_idem_conflict";
    open(&mut kernel, run, &source);
    kernel.pause_studio_run(control(run, "p")).expect("pause");

    let steer = |instruction: &'static str| StudioSteering {
        tenant_id: TENANT,
        actor_id: ACTOR,
        run_id: run,
        operator_id: ACTOR,
        idempotency_key: "reused",
        instruction,
        target_scope: "document",
        reason: "r",
        source_refs: "",
    };
    let first = kernel
        .steer_studio_run(steer("first instruction"))
        .expect("first");
    assert!(!first.refused);
    let conflict = kernel
        .steer_studio_run(steer("different instruction"))
        .expect("conflict");
    assert!(conflict.refused);
    assert_eq!(
        conflict.refusal_reason,
        "idempotency key reused with a different payload"
    );
}

#[test]
fn landing_replay_returns_the_same_landing_without_a_second_publication() {
    let mut kernel = MdxKernel::boot_local();
    let source = seed(&mut kernel);
    let run = "studio_run_land_replay";
    open(&mut kernel, run, &source);

    let first = kernel
        .land_studio_run(landing(run, "l", "page_replay"))
        .expect("first land");
    let replay = kernel
        .land_studio_run(landing(run, "l", "page_replay"))
        .expect("replay land");
    assert!(!first.idempotent_replay);
    assert!(replay.idempotent_replay);
    assert_eq!(first.publication_receipt_id, replay.publication_receipt_id);

    let published = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "pages.document.published")
        .count();
    assert_eq!(published, 1, "replay never creates a second publication");
}

#[test]
fn opening_an_already_open_run_is_refused() {
    let mut kernel = MdxKernel::boot_local();
    let source = seed(&mut kernel);
    let run = "studio_run_double_open";
    open(&mut kernel, run, &source);
    let error = kernel
        .open_studio_run(StudioOpenRun {
            tenant_id: TENANT,
            actor_id: ACTOR,
            run_id: run,
            goal: "again",
            artifact_kind: "document",
            source_receipt_id: &source,
            source_kind: "",
            source_title: "",
        })
        .expect_err("second open should fail");
    assert!(matches!(error, StudioRunError::RunAlreadyOpen(_)));
}

#[test]
fn opening_a_run_records_the_bound_source_kind_and_title() {
    let mut kernel = MdxKernel::boot_local();
    let source = seed(&mut kernel);
    let run = "studio_run_grounded";
    let report = kernel
        .open_studio_run(StudioOpenRun {
            tenant_id: TENANT,
            actor_id: ACTOR,
            run_id: run,
            goal: "Draft from the Q2 board page",
            artifact_kind: "document",
            source_receipt_id: &source,
            source_kind: "connector_item",
            source_title: "Q2 board pack",
        })
        .expect("open grounded run");
    assert_eq!(report.source_kind, "connector_item");
    let opened = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| {
            receipt.kind == "studio.run.opened"
                && receipt.payload.get("studio_run_id").map(String::as_str) == Some(run)
        })
        .expect("opened receipt")
        .clone();
    assert_eq!(
        opened.payload.get("source_kind").map(String::as_str),
        Some("connector_item")
    );
    assert_eq!(
        opened.payload.get("source_title").map(String::as_str),
        Some("Q2 board pack")
    );
}

#[test]
fn an_unknown_source_kind_is_refused() {
    let mut kernel = MdxKernel::boot_local();
    let source = seed(&mut kernel);
    let error = kernel
        .open_studio_run(StudioOpenRun {
            tenant_id: TENANT,
            actor_id: ACTOR,
            run_id: "studio_run_bad_source",
            goal: "Draft",
            artifact_kind: "document",
            source_receipt_id: &source,
            source_kind: "spreadsheet_from_mars",
            source_title: "",
        })
        .expect_err("unknown source kind should fail");
    assert!(matches!(error, StudioRunError::InvalidSourceKind(_)));
}

#[test]
fn a_decision_kind_run_lands_as_a_typed_pages_object() {
    let mut kernel = MdxKernel::boot_local();
    let source = seed(&mut kernel);
    let run = "studio_run_decision";
    kernel
        .open_studio_run(StudioOpenRun {
            tenant_id: TENANT,
            actor_id: ACTOR,
            run_id: run,
            goal: "Record the vendor decision",
            artifact_kind: "decision",
            source_receipt_id: &source,
            source_kind: "",
            source_title: "",
        })
        .expect("open decision run");
    kernel
        .land_studio_run(landing(run, "l", "page_vendor_decision"))
        .expect("land decision");

    let publication = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| {
            receipt.kind == "pages.document.published"
                && receipt.payload.get("studio_run_id").map(String::as_str) == Some(run)
        })
        .expect("publication")
        .clone();
    // A decision lands as a decision page, with the authoritative Studio kind
    // carried alongside; it is still one body of truth in Pages.
    assert_eq!(
        publication.payload.get("page_type").map(String::as_str),
        Some("decision")
    );
    assert_eq!(
        publication
            .payload
            .get("studio_artifact_kind")
            .map(String::as_str),
        Some("decision")
    );
    assert_eq!(
        publication.payload.get("origin").map(String::as_str),
        Some("studio")
    );
}
