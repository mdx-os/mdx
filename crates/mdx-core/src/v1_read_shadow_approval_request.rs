use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct V1ReadShadowApprovalRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub actor_role: &'a str,
    pub approval_request_id: &'a str,
    pub preflight_name: &'a str,
    pub requested_scope: &'a str,
    pub requested_window: &'a str,
    pub evidence_plan: &'a str,
    pub approver_id: &'a str,
    pub approver_role: &'a str,
    pub expires_at: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct V1ReadShadowApprovalRequestReport {
    pub status: &'static str,
    pub approval_request_id: String,
    pub approval_request_receipt_id: String,
    pub policy_decision_id: String,
    pub preflight_name: String,
    pub requested_scope: String,
    pub requested_window: String,
    pub approver_id: String,
    pub approver_role: String,
    pub terminal_state: &'static str,
    pub approval_request_shape_valid: bool,
    pub human_approval_granted: bool,
    pub live_read_allowed: bool,
    pub row_payload_allowed: bool,
    pub service_role_runtime_allowed: bool,
    pub credential_export_allowed: bool,
    pub production_cutover_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct V1ReadShadowApprovalDecision<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub actor_role: &'a str,
    pub approval_decision_id: &'a str,
    pub approval_request_receipt_id: &'a str,
    pub decision_scope: &'a str,
    pub decision_outcome: &'a str,
    pub decision_rationale: &'a str,
    pub decided_by: &'a str,
    pub decided_role: &'a str,
    pub expires_at: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct V1ReadShadowApprovalDecisionReport {
    pub status: &'static str,
    pub approval_decision_id: String,
    pub approval_decision_receipt_id: String,
    pub policy_decision_id: String,
    pub approval_request_receipt_id: String,
    pub decision_scope: String,
    pub decision_outcome: String,
    pub terminal_state: &'static str,
    pub approval_decision_shape_valid: bool,
    pub human_decision_recorded: bool,
    pub human_approval_granted: bool,
    pub live_read_allowed: bool,
    pub row_payload_allowed: bool,
    pub service_role_runtime_allowed: bool,
    pub credential_export_allowed: bool,
    pub production_cutover_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct V1WriteMirrorApprovalRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub actor_role: &'a str,
    pub approval_request_id: &'a str,
    pub policy_control_name: &'a str,
    pub requested_scope: &'a str,
    pub requested_window: &'a str,
    pub reconciliation_plan: &'a str,
    pub rollback_owner: &'a str,
    pub approver_id: &'a str,
    pub approver_role: &'a str,
    pub expires_at: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct V1WriteMirrorApprovalRequestReport {
    pub status: &'static str,
    pub approval_request_id: String,
    pub approval_request_receipt_id: String,
    pub policy_decision_id: String,
    pub policy_control_name: String,
    pub requested_scope: String,
    pub requested_window: String,
    pub rollback_owner: String,
    pub approver_id: String,
    pub approver_role: String,
    pub terminal_state: &'static str,
    pub approval_request_shape_valid: bool,
    pub human_approval_granted: bool,
    pub write_mirror_allowed: bool,
    pub production_write_allowed: bool,
    pub service_role_runtime_allowed: bool,
    pub production_cutover_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct V1WriteMirrorApprovalDecision<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub actor_role: &'a str,
    pub approval_decision_id: &'a str,
    pub approval_request_receipt_id: &'a str,
    pub decision_scope: &'a str,
    pub decision_outcome: &'a str,
    pub decision_rationale: &'a str,
    pub decided_by: &'a str,
    pub decided_role: &'a str,
    pub expires_at: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct V1WriteMirrorApprovalDecisionReport {
    pub status: &'static str,
    pub approval_decision_id: String,
    pub approval_decision_receipt_id: String,
    pub policy_decision_id: String,
    pub approval_request_receipt_id: String,
    pub decision_scope: String,
    pub decision_outcome: String,
    pub terminal_state: &'static str,
    pub approval_decision_shape_valid: bool,
    pub human_decision_recorded: bool,
    pub human_approval_granted: bool,
    pub write_mirror_allowed: bool,
    pub production_write_allowed: bool,
    pub service_role_runtime_allowed: bool,
    pub production_cutover_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum V1ReadShadowApprovalRequestError {
    Missing(&'static str),
    UnknownApprovalRequestReceipt(String),
    UnknownWriteMirrorApprovalRequestReceipt(String),
    ActorAdmission(String),
}

impl V1ReadShadowApprovalRequestError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("v1 read-shadow approval request missing {field}"),
            Self::UnknownApprovalRequestReceipt(id) => {
                format!("v1 read-shadow approval request source receipt {id} is unknown")
            }
            Self::UnknownWriteMirrorApprovalRequestReceipt(id) => {
                format!("v1 write-mirror approval request source receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_v1_read_shadow_approval_request_local(
        &mut self,
        request: V1ReadShadowApprovalRequest<'_>,
    ) -> Result<V1ReadShadowApprovalRequestReport, V1ReadShadowApprovalRequestError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_v1_read_shadow_approval_request_local_with_identity(request, &identity)
    }

    pub fn save_v1_read_shadow_approval_request_local_with_identity(
        &mut self,
        request: V1ReadShadowApprovalRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<V1ReadShadowApprovalRequestReport, V1ReadShadowApprovalRequestError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("actor_role", request.actor_role),
            ("approval_request_id", request.approval_request_id),
            ("preflight_name", request.preflight_name),
            ("requested_scope", request.requested_scope),
            ("requested_window", request.requested_window),
            ("evidence_plan", request.evidence_plan),
            ("approver_id", request.approver_id),
            ("approver_role", request.approver_role),
            ("expires_at", request.expires_at),
        ] {
            if value.trim().is_empty() {
                return Err(V1ReadShadowApprovalRequestError::Missing(field));
            }
        }

        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            request.actor_role,
            "/v1/read-shadow-approval-requests.json",
            "v1.read_shadow.approval.requested",
            request.approval_request_id,
        )
        .map_err(|error| V1ReadShadowApprovalRequestError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("v1_read_shadow_approval_request"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision =
            self.decide_with_receipt(&correlation, ActionKind::RequestV1ReadShadowApproval);
        let request_receipt = self.transition_receipt(
            &run_id,
            "REQUEST_V1_READ_SHADOW_APPROVAL",
            &correlation,
            &decision,
            "v1.read_shadow.approval.requested",
            payload(&[
                ("approval_request_id", request.approval_request_id),
                ("actor_role", request.actor_role),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", "/v1/read-shadow-approval-requests.json"),
                (
                    "projection_route",
                    "/v1/read-shadow-approval-requests/projection.json",
                ),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("preflight_name", request.preflight_name),
                (
                    "preflight_control",
                    "internal://migration-evidence-omitted",
                ),
                ("requested_scope", request.requested_scope),
                ("requested_window", request.requested_window),
                ("evidence_plan", request.evidence_plan),
                ("approver_id", request.approver_id),
                ("approver_role", request.approver_role),
                ("expires_at", request.expires_at),
                ("requested_receipt_kind", "v1.read_shadow.approved"),
                (
                    "terminal_state",
                    "READ_SHADOW_APPROVAL_REQUEST_RECORDED_LIVE_READ_BLOCKED",
                ),
                ("approval_request_shape_valid", "true"),
                ("human_approval_granted", "false"),
                ("live_read_allowed", "false"),
                ("row_payload_allowed", "false"),
                ("service_role_runtime_allowed", "false"),
                ("credential_export_allowed", "false"),
                ("production_cutover_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_v1_read_shadow_approval_request_run(&run_id);
        Ok(V1ReadShadowApprovalRequestReport {
            status: "READ_SHADOW_APPROVAL_REQUEST_RECORDED_LIVE_READ_BLOCKED",
            approval_request_id: request.approval_request_id.to_string(),
            approval_request_receipt_id: request_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            preflight_name: request.preflight_name.to_string(),
            requested_scope: request.requested_scope.to_string(),
            requested_window: request.requested_window.to_string(),
            approver_id: request.approver_id.to_string(),
            approver_role: request.approver_role.to_string(),
            terminal_state: "READ_SHADOW_APPROVAL_REQUEST_RECORDED_LIVE_READ_BLOCKED",
            approval_request_shape_valid: true,
            human_approval_granted: false,
            live_read_allowed: false,
            row_payload_allowed: false,
            service_role_runtime_allowed: false,
            credential_export_allowed: false,
            production_cutover_allowed: false,
            production_write_allowed: false,
        })
    }

    pub fn save_v1_read_shadow_approval_decision_local(
        &mut self,
        request: V1ReadShadowApprovalDecision<'_>,
    ) -> Result<V1ReadShadowApprovalDecisionReport, V1ReadShadowApprovalRequestError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_v1_read_shadow_approval_decision_local_with_identity(request, &identity)
    }

    pub fn save_v1_read_shadow_approval_decision_local_with_identity(
        &mut self,
        request: V1ReadShadowApprovalDecision<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<V1ReadShadowApprovalDecisionReport, V1ReadShadowApprovalRequestError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("actor_role", request.actor_role),
            ("approval_decision_id", request.approval_decision_id),
            ("decision_scope", request.decision_scope),
            ("decision_outcome", request.decision_outcome),
            ("decision_rationale", request.decision_rationale),
            ("decided_by", request.decided_by),
            ("decided_role", request.decided_role),
            ("expires_at", request.expires_at),
        ] {
            if value.trim().is_empty() {
                return Err(V1ReadShadowApprovalRequestError::Missing(field));
            }
        }

        let approval_request_receipt_id = self
            .ensure_v1_read_shadow_approval_request_source(request.approval_request_receipt_id)?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            request.actor_role,
            "/v1/read-shadow-approval-decisions.json",
            "v1.read_shadow.approval.decision.recorded",
            request.approval_decision_id,
        )
        .map_err(|error| V1ReadShadowApprovalRequestError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("v1_read_shadow_approval_decision"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordV1ReadShadowApprovalDecision);
        let decision_receipt = self.transition_receipt(
            &run_id,
            "RECORD_V1_READ_SHADOW_APPROVAL_DECISION",
            &correlation,
            &decision,
            "v1.read_shadow.approval.decision.recorded",
            payload(&[
                ("approval_decision_id", request.approval_decision_id),
                ("actor_role", request.actor_role),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", "/v1/read-shadow-approval-decisions.json"),
                (
                    "projection_route",
                    "/v1/read-shadow-approval-decisions/projection.json",
                ),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("approval_request_receipt_id", &approval_request_receipt_id),
                ("decision_scope", request.decision_scope),
                ("decision_outcome", request.decision_outcome),
                ("decision_rationale", request.decision_rationale),
                ("decided_by", request.decided_by),
                ("decided_role", request.decided_role),
                ("expires_at", request.expires_at),
                ("requested_receipt_kind", "v1.read_shadow.approved"),
                (
                    "terminal_state",
                    "READ_SHADOW_APPROVAL_DECISION_RECORDED_LIVE_READ_BLOCKED",
                ),
                ("approval_decision_shape_valid", "true"),
                ("human_decision_recorded", "true"),
                ("human_approval_granted", "false"),
                ("live_read_allowed", "false"),
                ("row_payload_allowed", "false"),
                ("service_role_runtime_allowed", "false"),
                ("credential_export_allowed", "false"),
                ("production_cutover_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_v1_read_shadow_approval_decision_run(&run_id);
        Ok(V1ReadShadowApprovalDecisionReport {
            status: "READ_SHADOW_APPROVAL_DECISION_RECORDED_LIVE_READ_BLOCKED",
            approval_decision_id: request.approval_decision_id.to_string(),
            approval_decision_receipt_id: decision_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            approval_request_receipt_id,
            decision_scope: request.decision_scope.to_string(),
            decision_outcome: request.decision_outcome.to_string(),
            terminal_state: "READ_SHADOW_APPROVAL_DECISION_RECORDED_LIVE_READ_BLOCKED",
            approval_decision_shape_valid: true,
            human_decision_recorded: true,
            human_approval_granted: false,
            live_read_allowed: false,
            row_payload_allowed: false,
            service_role_runtime_allowed: false,
            credential_export_allowed: false,
            production_cutover_allowed: false,
            production_write_allowed: false,
        })
    }

    fn ensure_v1_read_shadow_approval_request_source(
        &mut self,
        requested_id: &str,
    ) -> Result<String, V1ReadShadowApprovalRequestError> {
        if !requested_id.trim().is_empty() {
            if self.storage.ledger().query().by_id(requested_id).is_some() {
                return Ok(requested_id.to_string());
            }
            return Err(
                V1ReadShadowApprovalRequestError::UnknownApprovalRequestReceipt(
                    requested_id.to_string(),
                ),
            );
        }
        if let Some(receipt_id) = self
            .storage
            .ledger()
            .query()
            .by_kind("v1.read_shadow.approval.requested")
            .first()
            .map(|receipt| receipt.receipt_id.clone())
        {
            return Ok(receipt_id);
        }
        let report =
            self.save_v1_read_shadow_approval_request_local(V1ReadShadowApprovalRequest {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                actor_role: "owner",
                approval_request_id: "v1_read_shadow_approval_request_for_decision",
                preflight_name: "v1_production_read_shadow_preflight",
                requested_scope: "message_shadow_replay",
                requested_window: "2026-06-02T00:00:00Z/2026-06-02T01:00:00Z",
                evidence_plan: "counts only with no row payload export",
                approver_id: "human:local_owner",
                approver_role: "owner",
                expires_at: "2030-01-01T00:00:00Z",
            })?;
        Ok(report.approval_request_receipt_id)
    }

    fn finish_v1_read_shadow_approval_request_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "COMPLETED".to_string();
        }
    }

    fn finish_v1_read_shadow_approval_decision_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "READ_SHADOW_APPROVAL_DECISION_RECORDED_LIVE_READ_BLOCKED".to_string();
        }
    }

    pub fn save_v1_write_mirror_approval_request_local(
        &mut self,
        request: V1WriteMirrorApprovalRequest<'_>,
    ) -> Result<V1WriteMirrorApprovalRequestReport, V1ReadShadowApprovalRequestError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_v1_write_mirror_approval_request_local_with_identity(request, &identity)
    }

    pub fn save_v1_write_mirror_approval_request_local_with_identity(
        &mut self,
        request: V1WriteMirrorApprovalRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<V1WriteMirrorApprovalRequestReport, V1ReadShadowApprovalRequestError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("actor_role", request.actor_role),
            ("approval_request_id", request.approval_request_id),
            ("policy_control_name", request.policy_control_name),
            ("requested_scope", request.requested_scope),
            ("requested_window", request.requested_window),
            ("reconciliation_plan", request.reconciliation_plan),
            ("rollback_owner", request.rollback_owner),
            ("approver_id", request.approver_id),
            ("approver_role", request.approver_role),
            ("expires_at", request.expires_at),
        ] {
            if value.trim().is_empty() {
                return Err(V1ReadShadowApprovalRequestError::Missing(field));
            }
        }
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            request.actor_role,
            "/v1/write-mirror-approval-requests.json",
            "v1.write_mirror.approval.requested",
            request.approval_request_id,
        )
        .map_err(|error| V1ReadShadowApprovalRequestError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("v1_write_mirror_approval_request"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision =
            self.decide_with_receipt(&correlation, ActionKind::RequestV1WriteMirrorApproval);
        let request_receipt = self.transition_receipt(
            &run_id,
            "REQUEST_V1_WRITE_MIRROR_APPROVAL",
            &correlation,
            &decision,
            "v1.write_mirror.approval.requested",
            payload(&[
                ("approval_request_id", request.approval_request_id),
                ("actor_role", request.actor_role),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", "/v1/write-mirror-approval-requests.json"),
                (
                    "projection_route",
                    "/v1/write-mirror-approval-requests/projection.json",
                ),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("policy_control_name", request.policy_control_name),
                (
                    "policy_control",
                    "internal://migration-evidence-omitted",
                ),
                ("requested_scope", request.requested_scope),
                ("requested_window", request.requested_window),
                ("reconciliation_plan", request.reconciliation_plan),
                ("rollback_owner", request.rollback_owner),
                ("approver_id", request.approver_id),
                ("approver_role", request.approver_role),
                ("expires_at", request.expires_at),
                ("requested_receipt_kind", "v1.write_mirror.approved"),
                (
                    "terminal_state",
                    "WRITE_MIRROR_APPROVAL_REQUEST_RECORDED_MIRROR_BLOCKED",
                ),
                ("approval_request_shape_valid", "true"),
                ("human_approval_granted", "false"),
                ("write_mirror_allowed", "false"),
                ("production_write_allowed", "false"),
                ("service_role_runtime_allowed", "false"),
                ("production_cutover_allowed", "false"),
            ]),
        );
        self.finish_v1_write_mirror_approval_request_run(&run_id);
        Ok(V1WriteMirrorApprovalRequestReport {
            status: "WRITE_MIRROR_APPROVAL_REQUEST_RECORDED_MIRROR_BLOCKED",
            approval_request_id: request.approval_request_id.to_string(),
            approval_request_receipt_id: request_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            policy_control_name: request.policy_control_name.to_string(),
            requested_scope: request.requested_scope.to_string(),
            requested_window: request.requested_window.to_string(),
            rollback_owner: request.rollback_owner.to_string(),
            approver_id: request.approver_id.to_string(),
            approver_role: request.approver_role.to_string(),
            terminal_state: "WRITE_MIRROR_APPROVAL_REQUEST_RECORDED_MIRROR_BLOCKED",
            approval_request_shape_valid: true,
            human_approval_granted: false,
            write_mirror_allowed: false,
            production_write_allowed: false,
            service_role_runtime_allowed: false,
            production_cutover_allowed: false,
        })
    }

    pub fn save_v1_write_mirror_approval_decision_local(
        &mut self,
        request: V1WriteMirrorApprovalDecision<'_>,
    ) -> Result<V1WriteMirrorApprovalDecisionReport, V1ReadShadowApprovalRequestError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_v1_write_mirror_approval_decision_local_with_identity(request, &identity)
    }

    pub fn save_v1_write_mirror_approval_decision_local_with_identity(
        &mut self,
        request: V1WriteMirrorApprovalDecision<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<V1WriteMirrorApprovalDecisionReport, V1ReadShadowApprovalRequestError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("actor_role", request.actor_role),
            ("approval_decision_id", request.approval_decision_id),
            ("decision_scope", request.decision_scope),
            ("decision_outcome", request.decision_outcome),
            ("decision_rationale", request.decision_rationale),
            ("decided_by", request.decided_by),
            ("decided_role", request.decided_role),
            ("expires_at", request.expires_at),
        ] {
            if value.trim().is_empty() {
                return Err(V1ReadShadowApprovalRequestError::Missing(field));
            }
        }
        let approval_request_receipt_id = self
            .ensure_v1_write_mirror_approval_request_source(request.approval_request_receipt_id)?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            request.actor_role,
            "/v1/write-mirror-approval-decisions.json",
            "v1.write_mirror.approval.decision.recorded",
            request.approval_decision_id,
        )
        .map_err(|error| V1ReadShadowApprovalRequestError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("v1_write_mirror_approval_decision"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision = self.decide_with_receipt(
            &correlation,
            ActionKind::RecordV1WriteMirrorApprovalDecision,
        );
        let decision_receipt = self.transition_receipt(
            &run_id,
            "RECORD_V1_WRITE_MIRROR_APPROVAL_DECISION",
            &correlation,
            &decision,
            "v1.write_mirror.approval.decision.recorded",
            payload(&[
                ("approval_decision_id", request.approval_decision_id),
                ("actor_role", request.actor_role),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", "/v1/write-mirror-approval-decisions.json"),
                (
                    "projection_route",
                    "/v1/write-mirror-approval-decisions/projection.json",
                ),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("approval_request_receipt_id", &approval_request_receipt_id),
                ("decision_scope", request.decision_scope),
                ("decision_outcome", request.decision_outcome),
                ("decision_rationale", request.decision_rationale),
                ("decided_by", request.decided_by),
                ("decided_role", request.decided_role),
                ("expires_at", request.expires_at),
                ("requested_receipt_kind", "v1.write_mirror.approved"),
                (
                    "terminal_state",
                    "WRITE_MIRROR_APPROVAL_DECISION_RECORDED_MIRROR_BLOCKED",
                ),
                ("approval_decision_shape_valid", "true"),
                ("human_decision_recorded", "true"),
                ("human_approval_granted", "false"),
                ("write_mirror_allowed", "false"),
                ("production_write_allowed", "false"),
                ("service_role_runtime_allowed", "false"),
                ("production_cutover_allowed", "false"),
            ]),
        );
        self.finish_v1_write_mirror_approval_decision_run(&run_id);
        Ok(V1WriteMirrorApprovalDecisionReport {
            status: "WRITE_MIRROR_APPROVAL_DECISION_RECORDED_MIRROR_BLOCKED",
            approval_decision_id: request.approval_decision_id.to_string(),
            approval_decision_receipt_id: decision_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            approval_request_receipt_id,
            decision_scope: request.decision_scope.to_string(),
            decision_outcome: request.decision_outcome.to_string(),
            terminal_state: "WRITE_MIRROR_APPROVAL_DECISION_RECORDED_MIRROR_BLOCKED",
            approval_decision_shape_valid: true,
            human_decision_recorded: true,
            human_approval_granted: false,
            write_mirror_allowed: false,
            production_write_allowed: false,
            service_role_runtime_allowed: false,
            production_cutover_allowed: false,
        })
    }

    fn ensure_v1_write_mirror_approval_request_source(
        &mut self,
        requested_id: &str,
    ) -> Result<String, V1ReadShadowApprovalRequestError> {
        if !requested_id.trim().is_empty() {
            if self.storage.ledger().query().by_id(requested_id).is_some() {
                return Ok(requested_id.to_string());
            }
            return Err(
                V1ReadShadowApprovalRequestError::UnknownWriteMirrorApprovalRequestReceipt(
                    requested_id.to_string(),
                ),
            );
        }
        if let Some(receipt_id) = self
            .storage
            .ledger()
            .query()
            .by_kind("v1.write_mirror.approval.requested")
            .first()
            .map(|receipt| receipt.receipt_id.clone())
        {
            return Ok(receipt_id);
        }
        let report =
            self.save_v1_write_mirror_approval_request_local(V1WriteMirrorApprovalRequest {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                actor_role: "owner",
                approval_request_id: "v1_write_mirror_approval_request_for_decision",
                policy_control_name: "v1_write_shadow_mirror_policy_control",
                requested_scope: "message_shadow_replay",
                requested_window: "2026-06-02T00:00:00Z/2026-06-09T00:00:00Z",
                reconciliation_plan: "receipt comparison only with production writes blocked",
                rollback_owner: "human:local_owner",
                approver_id: "human:local_owner",
                approver_role: "owner",
                expires_at: "2030-01-01T00:00:00Z",
            })?;
        Ok(report.approval_request_receipt_id)
    }

    fn finish_v1_write_mirror_approval_request_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "COMPLETED".to_string();
        }
    }

    fn finish_v1_write_mirror_approval_decision_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "WRITE_MIRROR_APPROVAL_DECISION_RECORDED_MIRROR_BLOCKED".to_string();
        }
    }
}

#[cfg(test)]
fn payload_value<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_read_shadow_approval_request_records_blocked_local_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .save_v1_read_shadow_approval_request_local(V1ReadShadowApprovalRequest {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                actor_role: "owner",
                approval_request_id: "read_shadow_approval_test",
                preflight_name: "v1_production_read_shadow_preflight",
                requested_scope: "message_shadow_replay",
                requested_window: "2026-06-02T00:00:00Z/2026-06-02T01:00:00Z",
                evidence_plan: "counts only with no row payload export",
                approver_id: "human:owner",
                approver_role: "owner",
                expires_at: "2030-01-01T00:00:00Z",
            })
            .expect("read-shadow approval request");
        assert_eq!(
            report.status,
            "READ_SHADOW_APPROVAL_REQUEST_RECORDED_LIVE_READ_BLOCKED"
        );
        assert!(!report.human_approval_granted);
        assert!(!report.live_read_allowed);
        assert!(!report.row_payload_allowed);
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.approval_request_receipt_id)
            .expect("approval request receipt");
        assert_eq!(receipt.kind, "v1.read_shadow.approval.requested");
        assert_eq!(
            payload_value(receipt, "source_route"),
            "/v1/read-shadow-approval-requests.json"
        );
        assert_eq!(payload_value(receipt, "human_approval_granted"), "false");
    }

    #[test]
    fn v1_read_shadow_approval_request_requires_identity() {
        let mut kernel = MdxKernel::boot_local();
        let error = kernel
            .save_v1_read_shadow_approval_request_local(V1ReadShadowApprovalRequest {
                tenant_id: "",
                actor_id: "local_user",
                actor_role: "owner",
                approval_request_id: "missing_tenant",
                preflight_name: "v1_production_read_shadow_preflight",
                requested_scope: "message_shadow_replay",
                requested_window: "2026-06-02T00:00:00Z/2026-06-02T01:00:00Z",
                evidence_plan: "counts only",
                approver_id: "human:owner",
                approver_role: "owner",
                expires_at: "2030-01-01T00:00:00Z",
            })
            .expect_err("missing tenant must fail");
        assert_eq!(
            error.message(),
            "v1 read-shadow approval request missing tenant_id"
        );
    }

    #[test]
    fn v1_read_shadow_approval_decision_records_shape_without_live_read_authority() {
        let mut kernel = MdxKernel::boot_local();
        let request = kernel
            .save_v1_read_shadow_approval_request_local(V1ReadShadowApprovalRequest {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                actor_role: "owner",
                approval_request_id: "read_shadow_approval_decision_source",
                preflight_name: "v1_production_read_shadow_preflight",
                requested_scope: "message_shadow_replay",
                requested_window: "2026-06-02T00:00:00Z/2026-06-02T01:00:00Z",
                evidence_plan: "counts only with no row payload export",
                approver_id: "human:owner",
                approver_role: "owner",
                expires_at: "2030-01-01T00:00:00Z",
            })
            .expect("approval request");
        let report = kernel
            .save_v1_read_shadow_approval_decision_local(V1ReadShadowApprovalDecision {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                actor_role: "owner",
                approval_decision_id: "read_shadow_approval_decision_test",
                approval_request_receipt_id: &request.approval_request_receipt_id,
                decision_scope: "message_shadow_replay",
                decision_outcome: "hold_for_live_turn_on",
                decision_rationale: "shape recorded before live read approval",
                decided_by: "human:owner",
                decided_role: "owner",
                expires_at: "2030-01-01T00:00:00Z",
            })
            .expect("read-shadow approval decision");
        assert_eq!(
            report.status,
            "READ_SHADOW_APPROVAL_DECISION_RECORDED_LIVE_READ_BLOCKED"
        );
        assert!(report.human_decision_recorded);
        assert!(!report.human_approval_granted);
        assert!(!report.live_read_allowed);
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.approval_decision_receipt_id)
            .expect("approval decision receipt");
        assert_eq!(receipt.kind, "v1.read_shadow.approval.decision.recorded");
        assert_eq!(payload_value(receipt, "live_read_allowed"), "false");
    }

    #[test]
    fn v1_write_mirror_approval_request_records_blocked_local_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .save_v1_write_mirror_approval_request_local(V1WriteMirrorApprovalRequest {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                actor_role: "owner",
                approval_request_id: "write_mirror_approval_test",
                policy_control_name: "v1_write_shadow_mirror_policy_control",
                requested_scope: "message_shadow_replay",
                requested_window: "2026-06-02T00:00:00Z/2026-06-09T00:00:00Z",
                reconciliation_plan: "receipt comparison only",
                rollback_owner: "human:owner",
                approver_id: "human:owner",
                approver_role: "owner",
                expires_at: "2030-01-01T00:00:00Z",
            })
            .expect("write-mirror approval request");
        assert_eq!(
            report.status,
            "WRITE_MIRROR_APPROVAL_REQUEST_RECORDED_MIRROR_BLOCKED"
        );
        assert!(!report.human_approval_granted);
        assert!(!report.write_mirror_allowed);
        assert!(!report.production_write_allowed);
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.approval_request_receipt_id)
            .expect("write mirror approval request receipt");
        assert_eq!(receipt.kind, "v1.write_mirror.approval.requested");
        assert_eq!(
            payload_value(receipt, "source_route"),
            "/v1/write-mirror-approval-requests.json"
        );
        assert_eq!(payload_value(receipt, "write_mirror_allowed"), "false");
    }

    #[test]
    fn v1_write_mirror_approval_decision_records_shape_without_mirror_authority() {
        let mut kernel = MdxKernel::boot_local();
        let request = kernel
            .save_v1_write_mirror_approval_request_local(V1WriteMirrorApprovalRequest {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                actor_role: "owner",
                approval_request_id: "write_mirror_approval_decision_source",
                policy_control_name: "v1_write_shadow_mirror_policy_control",
                requested_scope: "message_shadow_replay",
                requested_window: "2026-06-02T00:00:00Z/2026-06-09T00:00:00Z",
                reconciliation_plan: "receipt comparison only",
                rollback_owner: "human:owner",
                approver_id: "human:owner",
                approver_role: "owner",
                expires_at: "2030-01-01T00:00:00Z",
            })
            .expect("approval request");
        let report = kernel
            .save_v1_write_mirror_approval_decision_local(V1WriteMirrorApprovalDecision {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                actor_role: "owner",
                approval_decision_id: "write_mirror_approval_decision_test",
                approval_request_receipt_id: &request.approval_request_receipt_id,
                decision_scope: "message_shadow_replay",
                decision_outcome: "hold_for_mirror_turn_on",
                decision_rationale: "shape recorded before write mirror approval",
                decided_by: "human:owner",
                decided_role: "owner",
                expires_at: "2030-01-01T00:00:00Z",
            })
            .expect("write-mirror approval decision");
        assert_eq!(
            report.status,
            "WRITE_MIRROR_APPROVAL_DECISION_RECORDED_MIRROR_BLOCKED"
        );
        assert!(report.human_decision_recorded);
        assert!(!report.human_approval_granted);
        assert!(!report.write_mirror_allowed);
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.approval_decision_receipt_id)
            .expect("approval decision receipt");
        assert_eq!(receipt.kind, "v1.write_mirror.approval.decision.recorded");
        assert_eq!(payload_value(receipt, "write_mirror_allowed"), "false");
    }
}
