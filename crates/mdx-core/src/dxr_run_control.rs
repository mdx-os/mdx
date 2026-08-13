// A run control - pause, resume, stop - recorded at the same governance
// bar as the rest of the Forge chain: actor admission, a policy
// decision, and a real ledger receipt that binds to the run's OWN build
// request receipt. The idempotency key rides the payload so a retry can
// be answered with the original receipt instead of a duplicate.
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxrRunControl<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub actor_role: &'a str,
    pub run_id: &'a str,
    pub control_action: &'a str,
    pub idempotency_key: &'a str,
    pub bound_build_request_receipt_id: &'a str,
    pub line: &'a str,
    pub terminal_state: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DxrRunControlReport {
    pub status: &'static str,
    pub control_receipt_id: String,
    pub policy_decision_id: String,
    pub run_id: String,
    pub control_action: String,
    pub terminal_state: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DxrRunControlError {
    Missing(&'static str),
    UnknownAction(String),
    UnknownBuildRequestReceipt(String),
    ActorAdmission(String),
}

impl DxrRunControlError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("dxr run control missing {field}"),
            Self::UnknownAction(action) => {
                format!("dxr run control action {action} is not pause, resume, or stop")
            }
            Self::UnknownBuildRequestReceipt(id) => {
                format!("dxr run control bound receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_dxr_run_control(
        &mut self,
        request: DxrRunControl<'_>,
    ) -> Result<DxrRunControlReport, DxrRunControlError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("actor_role", request.actor_role),
            ("run_id", request.run_id),
            ("idempotency_key", request.idempotency_key),
            (
                "bound_build_request_receipt_id",
                request.bound_build_request_receipt_id,
            ),
            ("terminal_state", request.terminal_state),
        ] {
            if value.trim().is_empty() {
                return Err(DxrRunControlError::Missing(field));
            }
        }
        if !["pause", "resume", "stop"].contains(&request.control_action) {
            return Err(DxrRunControlError::UnknownAction(
                request.control_action.to_string(),
            ));
        }
        // The per-run binding is real or the control does not record: the
        // bound receipt must exist and be this run's own build request.
        let bound = self
            .storage
            .ledger()
            .query()
            .by_id(request.bound_build_request_receipt_id)
            .cloned();
        let Some(bound) = bound else {
            return Err(DxrRunControlError::UnknownBuildRequestReceipt(
                request.bound_build_request_receipt_id.to_string(),
            ));
        };
        if bound.kind != "forge.build.requested" {
            return Err(DxrRunControlError::UnknownBuildRequestReceipt(
                request.bound_build_request_receipt_id.to_string(),
            ));
        }
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            request.actor_role,
            "/dxr/forge-runs/{run_id}/controls.json",
            "dxr.run.control.recorded",
            request.idempotency_key,
        )
        .map_err(|error| DxrRunControlError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("dxr_run_control"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let decision = self.decide_with_receipt(&correlation, ActionKind::RecordDxrRunControl);
        let control_receipt = self.storage.append_receipt(
            &mut self.ids,
            &correlation,
            "dxr.run.control.recorded",
            Some(decision.policy_decision_id.clone()),
            payload(&[
                ("run_id", request.run_id),
                ("control_action", request.control_action),
                ("idempotency_key", request.idempotency_key),
                ("actor_role", request.actor_role),
                ("source_route", "/dxr/forge-runs/{run_id}/controls.json"),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                (
                    "bound_build_request_receipt_id",
                    request.bound_build_request_receipt_id,
                ),
                ("line", request.line),
                ("terminal_state", request.terminal_state),
                ("worker_spawn_allowed", "false"),
                ("workflow_execution_allowed", "false"),
            ]),
        );
        Ok(DxrRunControlReport {
            status: "DXR_RUN_CONTROL_RECORDED",
            control_receipt_id: control_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            run_id: request.run_id.to_string(),
            control_action: request.control_action.to_string(),
            terminal_state: request.terminal_state.to_string(),
        })
    }
}
