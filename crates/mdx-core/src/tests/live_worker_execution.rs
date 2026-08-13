use crate::{
    LiveWorkerExecutionAdmission, LiveWorkerExecutionRejection, admit_live_worker_execution,
};

fn valid_live_worker_execution_admission() -> LiveWorkerExecutionAdmission<'static> {
    LiveWorkerExecutionAdmission {
        worker_run_id: "worker_run_001",
        spawn_receipt_id: "receipt_spawn",
        credential_check_receipt_id: "receipt_credential_check",
        handoff_receipt_id: "receipt_handoff",
        retirement_receipt_id: "receipt_retired",
        provider_turn_on_receipt_id: "receipt_provider_turn_on",
        dispatch_claim_id: "dispatch_claim_001",
        heartbeat_receipt_id: "heartbeat_receipt_001",
        durable_workflow_receipt_id: "durable_workflow_receipt_001",
        sandbox_authority_receipt_id: "sandbox_authority_receipt_001",
        external_sandbox_preflight_id: "dxr_external_sandbox_preflight_001",
        tool_policy_receipt_id: "tool_policy_receipt_001",
        reviewer_separation_receipt_id: "reviewer_separation_receipt_001",
        requested_execution_mode: "live_worker_execution",
        requested_receipt_kind: "worker.live_execution.authorized",
    }
}

#[test]
fn live_worker_execution_admission_requires_provider_turn_on_evidence() {
    let mut request = valid_live_worker_execution_admission();
    request.provider_turn_on_receipt_id = "";
    assert_eq!(
        admit_live_worker_execution(&request),
        Err(LiveWorkerExecutionRejection::MissingProviderTurnOnReceipt)
    );

    request = valid_live_worker_execution_admission();
    request.requested_execution_mode = "local_receipt_runtime";
    assert_eq!(
        admit_live_worker_execution(&request),
        Err(LiveWorkerExecutionRejection::InvalidExecutionMode)
    );
}

#[test]
fn live_worker_execution_admission_allows_evidenced_live_boundary() {
    let request = valid_live_worker_execution_admission();

    assert_eq!(admit_live_worker_execution(&request), Ok(()));
}
