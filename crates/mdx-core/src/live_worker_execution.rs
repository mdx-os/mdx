#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LiveWorkerExecutionAdmission<'a> {
    pub worker_run_id: &'a str,
    pub spawn_receipt_id: &'a str,
    pub credential_check_receipt_id: &'a str,
    pub handoff_receipt_id: &'a str,
    pub retirement_receipt_id: &'a str,
    pub provider_turn_on_receipt_id: &'a str,
    pub dispatch_claim_id: &'a str,
    pub heartbeat_receipt_id: &'a str,
    pub durable_workflow_receipt_id: &'a str,
    pub sandbox_authority_receipt_id: &'a str,
    pub external_sandbox_preflight_id: &'a str,
    pub tool_policy_receipt_id: &'a str,
    pub reviewer_separation_receipt_id: &'a str,
    pub requested_execution_mode: &'a str,
    pub requested_receipt_kind: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveWorkerExecutionRejection {
    MissingField(&'static str),
    MissingProviderTurnOnReceipt,
    InvalidExecutionMode,
    InvalidReceiptKind,
}

pub fn admit_live_worker_execution(
    request: &LiveWorkerExecutionAdmission<'_>,
) -> Result<(), LiveWorkerExecutionRejection> {
    for (field, value) in [
        ("worker_run_id", request.worker_run_id),
        ("spawn_receipt_id", request.spawn_receipt_id),
        ("handoff_receipt_id", request.handoff_receipt_id),
        ("retirement_receipt_id", request.retirement_receipt_id),
    ] {
        if value.trim().is_empty() {
            return Err(LiveWorkerExecutionRejection::MissingField(field));
        }
    }
    if request.provider_turn_on_receipt_id.trim().is_empty() {
        return Err(LiveWorkerExecutionRejection::MissingProviderTurnOnReceipt);
    }
    for (field, value) in [
        (
            "credential_check_receipt_id",
            request.credential_check_receipt_id,
        ),
        ("dispatch_claim_id", request.dispatch_claim_id),
        ("heartbeat_receipt_id", request.heartbeat_receipt_id),
        (
            "durable_workflow_receipt_id",
            request.durable_workflow_receipt_id,
        ),
        (
            "sandbox_authority_receipt_id",
            request.sandbox_authority_receipt_id,
        ),
        (
            "external_sandbox_preflight_id",
            request.external_sandbox_preflight_id,
        ),
        ("tool_policy_receipt_id", request.tool_policy_receipt_id),
        (
            "reviewer_separation_receipt_id",
            request.reviewer_separation_receipt_id,
        ),
    ] {
        if value.trim().is_empty() {
            return Err(LiveWorkerExecutionRejection::MissingField(field));
        }
    }
    if request.requested_execution_mode != "live_worker_execution" {
        return Err(LiveWorkerExecutionRejection::InvalidExecutionMode);
    }
    if request.requested_receipt_kind != "worker.live_execution.authorized" {
        return Err(LiveWorkerExecutionRejection::InvalidReceiptKind);
    }
    Ok(())
}
