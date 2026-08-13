use crate::*;

#[test]
fn message_fanout_request_records_realtime_blocked_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let seed = kernel.run_evals_runner_agent().expect("seed evidence");
    let message = kernel
        .save_message_thread_message_local(MessageThreadMessage {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            thread_id: "thread_local_receipts",
            channel_id: "local-ops",
            message_id: "message_fanout_source",
            content_type: "text",
            body: "Message fanout source",
            content_payload: "Message fanout source",
            source_receipt_id: &seed.receipts[0],
            outbox_topic: "message.human.posted",
        })
        .expect("message");

    let report = kernel
        .save_message_fanout_request_local(MessageFanoutRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            fanout_request_id: "fanout_core_test",
            message_receipt_id: &message.message_receipt_id,
            channel_id: "local-ops",
            delivery_scope: "local_thread",
        })
        .expect("fanout request");

    assert_eq!(report.status, "FANOUT_REQUEST_RECORDED_REALTIME_BLOCKED");
    assert_eq!(report.message_receipt_id, message.message_receipt_id);
    assert!(!report.realtime_fanout_allowed);
    assert!(!report.presence_allowed);
    assert!(!report.typing_state_allowed);
    assert!(!report.provider_call_allowed);
    assert!(!report.production_delivery_allowed);

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.fanout_request_receipt_id)
        .expect("fanout receipt");
    assert_eq!(receipt.kind, "message.fanout.requested");
    assert_eq!(receipt.payload["fanout_request_id"], "fanout_core_test");
    assert_eq!(
        receipt.payload["actor_admission_status"],
        "LOCAL_ACTOR_ADMITTED"
    );
    assert_eq!(
        receipt.payload["terminal_state"],
        "FANOUT_REQUEST_RECORDED_REALTIME_BLOCKED"
    );
}

#[test]
fn message_fanout_request_requires_known_message_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .save_message_fanout_request_local(MessageFanoutRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            fanout_request_id: "fanout_missing_source",
            message_receipt_id: "missing_receipt",
            channel_id: "local-ops",
            delivery_scope: "local_thread",
        })
        .expect_err("missing message receipt should fail");

    assert!(matches!(
        error,
        MessageFanoutRequestError::UnknownMessageReceipt(_)
    ));
}
