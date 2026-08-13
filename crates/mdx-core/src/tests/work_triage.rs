//! Unit tests for the triage stream's two governed verbs: recording an
//! entry and recording a verdict, including every refusal arm.
use crate::*;

fn local_identity() -> GovernedWriteIdentity {
    GovernedWriteIdentity::local_demo("human:local_user")
}

fn valid_triage_entry() -> TriageEntry<'static> {
    TriageEntry {
        tenant_id: "tenant_local",
        actor_id: "human:local_user",
        text: "Customers keep asking for a faster export.",
        source: "manual",
        source_ref: "message_local_001",
    }
}

fn valid_triage_verdict() -> TriageVerdict<'static> {
    TriageVerdict {
        tenant_id: "tenant_local",
        actor_id: "human:local_user",
        entry_ref: "triage_entry_unknown",
        verdict: "declined",
        note: "Not worth pursuing right now.",
        accept_as: "",
        accept_title: "",
        bet_id: "",
        duplicate_of: "",
    }
}

#[test]
fn triage_stream_entry_records_governed_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();

    let report = kernel
        .save_triage_entry_local_with_identity(valid_triage_entry(), &identity)
        .expect("triage entry recorded");

    assert_eq!(report.terminal_state, "TRIAGE_ENTRY_RECORDED");
    assert!(report.accepted_record_id.is_none());
    assert!(report.accepted_receipt_id.is_none());

    let entry = kernel
        .ledger()
        .query()
        .by_id(&report.receipt_id)
        .expect("entry receipt");
    assert_eq!(entry.kind, "triage.entry.recorded");
    assert_eq!(entry.payload["source"], "manual");
    assert_eq!(entry.payload["authority_opened"], "none");
    assert_eq!(entry.payload["production_write_allowed"], "false");
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn triage_stream_entry_defaults_blank_source_to_manual() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();
    let mut request = valid_triage_entry();
    request.source = "";

    let report = kernel
        .save_triage_entry_local_with_identity(request, &identity)
        .expect("triage entry recorded");

    let entry = kernel
        .ledger()
        .query()
        .by_id(&report.receipt_id)
        .expect("entry receipt");
    assert_eq!(entry.payload["source"], "manual");
}

#[test]
fn triage_stream_entry_requires_text() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();
    let mut request = valid_triage_entry();
    request.text = "   ";

    let error = kernel
        .save_triage_entry_local_with_identity(request, &identity)
        .expect_err("missing text should fail");

    assert_eq!(error, TriageError::Missing("text"));
}

#[test]
fn triage_stream_entry_rejects_overlong_text() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();
    let long_text = "x".repeat(1001);
    let mut request = valid_triage_entry();
    request.text = &long_text;

    let error = kernel
        .save_triage_entry_local_with_identity(request, &identity)
        .expect_err("overlong text should fail");

    assert_eq!(error, TriageError::TooLong("text", 1001, 1000));
}

#[test]
fn triage_stream_entry_rejects_overlong_source_ref() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();
    let long_ref = "r".repeat(501);
    let mut request = valid_triage_entry();
    request.source_ref = &long_ref;

    let error = kernel
        .save_triage_entry_local_with_identity(request, &identity)
        .expect_err("overlong source_ref should fail");

    assert_eq!(error, TriageError::TooLong("source_ref", 501, 500));
}

#[test]
fn triage_stream_entry_rejects_unknown_source() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();
    let mut request = valid_triage_entry();
    request.source = "telepathy";

    let error = kernel
        .save_triage_entry_local_with_identity(request, &identity)
        .expect_err("unknown source should fail");

    assert_eq!(error, TriageError::UnknownSource("telepathy".to_string()));
}

#[test]
fn triage_stream_verdict_records_governed_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();

    let report = kernel
        .save_triage_verdict_local_with_identity(valid_triage_verdict(), &identity)
        .expect("triage verdict recorded");

    assert_eq!(report.terminal_state, "TRIAGE_VERDICT_RECORDED");
    assert!(report.accepted_record_id.is_none());

    let verdict = kernel
        .ledger()
        .query()
        .by_id(&report.receipt_id)
        .expect("verdict receipt");
    assert_eq!(verdict.kind, "triage.verdict.recorded");
    assert_eq!(verdict.payload["verdict"], "declined");
    assert_eq!(verdict.payload["authority_opened"], "none");
    assert_eq!(verdict.payload["production_write_allowed"], "false");
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn triage_stream_verdict_accept_mints_work_item() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();
    let entry = kernel
        .save_triage_entry_local_with_identity(valid_triage_entry(), &identity)
        .expect("triage entry recorded");
    let entry_ref = entry.record_id.clone();

    let report = kernel
        .save_triage_verdict_local_with_identity(
            TriageVerdict {
                tenant_id: "tenant_local",
                actor_id: "human:local_user",
                entry_ref: &entry_ref,
                verdict: "accepted",
                note: "Turn this into work.",
                accept_as: "work_item",
                accept_title: "",
                bet_id: "",
                duplicate_of: "",
            },
            &identity,
        )
        .expect("accepted verdict recorded");

    assert_eq!(report.terminal_state, "TRIAGE_VERDICT_RECORDED");
    assert!(report.accepted_record_id.is_some());
    assert!(report.accepted_receipt_id.is_some());

    let verdict = kernel
        .ledger()
        .query()
        .by_id(&report.receipt_id)
        .expect("verdict receipt");
    assert_eq!(verdict.payload["accepted_as"], "work_item");
}

#[test]
fn triage_stream_verdict_requires_verdict() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();
    let mut request = valid_triage_verdict();
    request.verdict = "   ";

    let error = kernel
        .save_triage_verdict_local_with_identity(request, &identity)
        .expect_err("missing verdict should fail");

    assert_eq!(error, TriageError::Missing("verdict"));
}

#[test]
fn triage_stream_verdict_rejects_unknown_verdict() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();
    let mut request = valid_triage_verdict();
    request.verdict = "maybe";

    let error = kernel
        .save_triage_verdict_local_with_identity(request, &identity)
        .expect_err("unknown verdict should fail");

    assert_eq!(error, TriageError::UnknownVerdict("maybe".to_string()));
}

#[test]
fn triage_stream_verdict_rejects_overlong_note() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();
    let long_note = "n".repeat(501);
    let mut request = valid_triage_verdict();
    request.note = &long_note;

    let error = kernel
        .save_triage_verdict_local_with_identity(request, &identity)
        .expect_err("overlong note should fail");

    assert_eq!(error, TriageError::TooLong("note", 501, 500));
}

#[test]
fn triage_stream_verdict_duplicate_requires_duplicate_of() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();
    let mut request = valid_triage_verdict();
    request.verdict = "duplicate";
    request.duplicate_of = "   ";

    let error = kernel
        .save_triage_verdict_local_with_identity(request, &identity)
        .expect_err("duplicate without reference should fail");

    assert_eq!(error, TriageError::DuplicateRefRequired);
}

#[test]
fn triage_stream_verdict_accept_requires_accept_as() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();
    let mut request = valid_triage_verdict();
    request.verdict = "accepted";
    request.accept_as = "   ";

    let error = kernel
        .save_triage_verdict_local_with_identity(request, &identity)
        .expect_err("accept without target should fail");

    assert_eq!(error, TriageError::Missing("accept_as"));
}

#[test]
fn triage_stream_verdict_accept_rejects_unknown_target() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();
    let mut request = valid_triage_verdict();
    request.verdict = "accepted";
    request.accept_as = "epic";
    request.accept_title = "Faster export";

    let error = kernel
        .save_triage_verdict_local_with_identity(request, &identity)
        .expect_err("unknown accept target should fail");

    assert_eq!(error, TriageError::UnknownAcceptTarget("epic".to_string()));
}

#[test]
fn triage_stream_verdict_accept_requires_title_when_entry_missing() {
    let mut kernel = MdxKernel::boot_local();
    let identity = local_identity();
    let mut request = valid_triage_verdict();
    request.verdict = "accepted";
    request.accept_as = "work_item";
    request.entry_ref = "triage_entry_not_on_stream";
    request.accept_title = "";

    let error = kernel
        .save_triage_verdict_local_with_identity(request, &identity)
        .expect_err("accept without title for missing entry should fail");

    assert_eq!(error, TriageError::AcceptTitleRequired);
}
