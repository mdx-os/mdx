use crate::*;

#[test]
fn message_presence_request_records_realtime_blocked_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let seed = kernel.run_evals_runner_agent().expect("seed evidence");
    let message = kernel
        .save_message_thread_message_local(MessageThreadMessage {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            thread_id: "thread_local_receipts",
            channel_id: "local-ops",
            message_id: "message_presence_source",
            content_type: "text",
            body: "Message presence source",
            content_payload: "Message presence source",
            source_receipt_id: &seed.receipts[0],
            outbox_topic: "message.human.posted",
        })
        .expect("message");
    let fanout = kernel
        .save_message_fanout_request_local(MessageFanoutRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            fanout_request_id: "fanout_presence_source",
            message_receipt_id: &message.message_receipt_id,
            channel_id: "local-ops",
            delivery_scope: "local_thread",
        })
        .expect("fanout request");

    let report = kernel
        .save_message_presence_request_local(MessagePresenceRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            presence_request_id: "presence_core_test",
            fanout_request_receipt_id: &fanout.fanout_request_receipt_id,
            thread_id: "thread_local_receipts",
            channel_id: "local-ops",
            presence_scope: "local_thread",
        })
        .expect("presence request");

    assert_eq!(report.status, "PRESENCE_REQUEST_RECORDED_REALTIME_BLOCKED");
    assert_eq!(
        report.fanout_request_receipt_id,
        fanout.fanout_request_receipt_id
    );
    assert!(!report.realtime_presence_allowed);
    assert!(!report.websocket_fanout_allowed);
    assert!(!report.typing_indicator_allowed);
    assert!(!report.provider_call_allowed);
    assert!(!report.production_delivery_allowed);
    assert!(!report.production_write_allowed);

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.presence_request_receipt_id)
        .expect("presence receipt");
    assert_eq!(receipt.kind, "message.presence.requested");
    assert_eq!(receipt.payload["presence_request_id"], "presence_core_test");
    assert_eq!(
        receipt.payload["actor_admission_status"],
        "LOCAL_ACTOR_ADMITTED"
    );
    assert_eq!(
        receipt.payload["terminal_state"],
        "PRESENCE_REQUEST_RECORDED_REALTIME_BLOCKED"
    );
}

#[test]
fn message_presence_request_requires_known_fanout_receipt() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .save_message_presence_request_local(MessagePresenceRequest {
            tenant_id: "tenant_local",
            actor_id: "human:local_user",
            presence_request_id: "presence_missing_source",
            fanout_request_receipt_id: "missing_receipt",
            thread_id: "thread_local_receipts",
            channel_id: "local-ops",
            presence_scope: "local_thread",
        })
        .expect_err("missing fanout receipt should fail");

    assert!(matches!(
        error,
        MessagePresenceRequestError::UnknownFanoutReceipt(_)
    ));
}
