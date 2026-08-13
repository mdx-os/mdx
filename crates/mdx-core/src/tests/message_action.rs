use super::*;

// The message-action rails need a governed SOURCE receipt to anchor a
// card to. The old seed used the deleted intake/build lane; a live forge
// run event receipt anchors the same way.
fn seed_source_receipt(kernel: &mut MdxKernel, run_id: &str) -> String {
    kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:forge",
            run_id,
            event: "run_started",
            work_item_id: "w_message_action",
            detail: "accepted",
            turn: 0,
            input_tokens: 0,
            output_tokens: 0,
        })
        .expect("seed source receipt")
        .receipt_id
}

#[test]
fn message_action_request_and_verdict_record_blocked_execution_chain() {
    let mut kernel = MdxKernel::boot_local();
    let seed_receipt_id = seed_source_receipt(&mut kernel, "run_message_action_seed");

    let request = kernel
        .save_message_action_request_local(MessageActionRequest {
            tenant_id: "local_tenant",
            actor_id: "agent:forge",
            thread_id: "local",
            channel_id: "forge",
            action_request_id: "message_action_request_test",
            source_receipt_id: &seed_receipt_id,
            title: "Approve the Forge plan",
            summary: "Forge has a plan and needs a governed human verdict.",
            proposed_action: "Approve the plan boundary before any worker can run.",
            action_payload: r#"{"plan_hash":"plan-message-action-001"}"#,
        })
        .expect("message action request");

    assert_eq!(
        request.status,
        "MESSAGE_ACTION_REQUEST_RECORDED_EXECUTION_BLOCKED"
    );
    assert_eq!(request.channel_id, "forge");
    assert_eq!(request.source_receipt_kind, "forge.run.event");
    assert_eq!(request.allowed_verdicts, MESSAGE_ACTION_VERDICTS.join(","));
    assert!(!request.execution_allowed);
    assert!(!request.production_write_allowed);

    let verdict = kernel
        .save_message_action_verdict_local(MessageActionVerdict {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            actor_role: "owner",
            action_verdict_id: "message_action_verdict_test",
            action_request_receipt_id: &request.action_request_receipt_id,
            verdict: "approve",
            decision_note: "Approve the plan boundary, but do not execute.",
            edited_action_payload: "",
        })
        .expect("message action verdict");

    assert_eq!(
        verdict.status,
        "MESSAGE_ACTION_VERDICT_RECORDED_EXECUTION_BLOCKED"
    );
    assert_eq!(verdict.action_request_id, request.action_request_id);
    assert_eq!(verdict.thread_id, "local");
    assert_eq!(verdict.channel_id, "forge");
    assert_eq!(verdict.verdict, "approve");
    assert!(verdict.human_decision_recorded);
    assert!(verdict.human_approval_granted);
    assert!(!verdict.execution_allowed);
    assert!(!verdict.production_write_allowed);

    assert!(
        kernel
            .ledger()
            .query()
            .by_id(&request.action_request_receipt_id)
            .is_some_and(|receipt| receipt.kind == "message.action.requested")
    );
    assert!(
        kernel
            .ledger()
            .query()
            .by_id(&verdict.action_verdict_receipt_id)
            .is_some_and(|receipt| receipt.kind == "message.action.verdict.recorded")
    );
}

#[test]
fn message_action_verdict_rejects_unknown_verdict() {
    let mut kernel = MdxKernel::boot_local();
    let seed_receipt_id = seed_source_receipt(&mut kernel, "run_message_action_bad_seed");
    let request = kernel
        .save_message_action_request_local(MessageActionRequest {
            tenant_id: "local_tenant",
            actor_id: "agent:forge",
            thread_id: "local",
            channel_id: "forge",
            action_request_id: "message_action_bad_request",
            source_receipt_id: &seed_receipt_id,
            title: "Approve the Forge plan",
            summary: "Forge has a plan and needs a governed human verdict.",
            proposed_action: "Approve the plan boundary before any worker can run.",
            action_payload: r#"{"plan_hash":"plan-message-action-bad-001"}"#,
        })
        .expect("message action request");

    let error = kernel
        .save_message_action_verdict_local(MessageActionVerdict {
            tenant_id: "local_tenant",
            actor_id: "human:local_user",
            actor_role: "owner",
            action_verdict_id: "message_action_bad_verdict",
            action_request_receipt_id: &request.action_request_receipt_id,
            verdict: "ship",
            decision_note: "Try an unsupported verdict.",
            edited_action_payload: "",
        })
        .expect_err("invalid verdict rejected");

    assert_eq!(
        error.message(),
        "message action verdict must be one of approve, edit, reject, respond; got ship"
    );
}
