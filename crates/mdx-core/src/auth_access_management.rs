use crate::*;

pub const AUTH_INVITE_REQUEST_ROUTE: &str = "/auth/invite-requests.json";
pub const AUTH_INVITE_REDEMPTION_ROUTE: &str = "/auth/invite-redemptions.json";
pub const AUTH_ROLE_ASSIGNMENT_ROUTE: &str = "/auth/role-assignments.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthInviteRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub invite_id: &'a str,
    pub invited_actor_id: &'a str,
    pub invited_email_hash: &'a str,
    pub requested_role: &'a str,
    pub source_tenant_policy_preflight_receipt_id: &'a str,
    pub expires_at: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthInviteRequestReport {
    pub status: &'static str,
    pub invite_id: String,
    pub invite_request_receipt_id: String,
    pub policy_decision_id: String,
    pub source_tenant_policy_preflight_receipt_id: String,
    pub invited_actor_id: String,
    pub invited_email_hash: String,
    pub requested_role: String,
    pub expires_at: String,
    pub email_delivery_allowed: bool,
    pub role_assignment_allowed: bool,
    pub session_activation_allowed: bool,
    pub service_role_shortcut_allowed: bool,
    pub production_auth_provider_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthInviteRedemption<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub redemption_id: &'a str,
    pub source_invite_request_receipt_id: &'a str,
    pub authenticated_actor_id: &'a str,
    pub cohort_id: &'a str,
    pub risk_tier: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthInviteRedemptionReport {
    pub status: &'static str,
    pub redemption_id: String,
    pub invite_redemption_receipt_id: String,
    pub policy_decision_id: String,
    pub source_invite_request_receipt_id: String,
    pub invite_id: String,
    pub authenticated_actor_id: String,
    pub requested_role: String,
    pub cohort_id: String,
    pub risk_tier: String,
    pub enrollment_record_allowed: bool,
    pub session_cookie_write_allowed: bool,
    pub production_auth_provider_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthRoleAssignment<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub role_assignment_id: &'a str,
    pub source_invite_request_receipt_id: &'a str,
    pub target_actor_id: &'a str,
    pub assigned_role: &'a str,
    pub assignment_note: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthRoleAssignmentReport {
    pub status: &'static str,
    pub role_assignment_id: String,
    pub role_assignment_receipt_id: String,
    pub policy_decision_id: String,
    pub source_invite_request_receipt_id: String,
    pub invite_id: String,
    pub target_actor_id: String,
    pub assigned_role: String,
    pub assignment_note: String,
    pub membership_state: String,
    pub role_mapping_recorded: bool,
    pub tenant_membership_recorded: bool,
    pub session_cookie_write_allowed: bool,
    pub role_escalation_allowed: bool,
    pub production_auth_provider_allowed: bool,
    pub production_role_write_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthAccessManagementError {
    Missing(&'static str),
    UnknownTenantPolicyPreflightReceipt(String),
    UnknownInviteRequestReceipt(String),
    ActorAdmission(String),
}

impl AuthAccessManagementError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("auth access management missing {field}"),
            Self::UnknownTenantPolicyPreflightReceipt(id) => {
                format!("auth invite source tenant policy preflight receipt {id} is unknown")
            }
            Self::UnknownInviteRequestReceipt(id) => {
                format!("auth access source invite receipt {id} is unknown")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_auth_invite_request_local(
        &mut self,
        request: AuthInviteRequest<'_>,
    ) -> Result<AuthInviteRequestReport, AuthAccessManagementError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_auth_invite_request_local_with_identity(request, &identity)
    }

    pub fn save_auth_invite_request_local_with_identity(
        &mut self,
        request: AuthInviteRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<AuthInviteRequestReport, AuthAccessManagementError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("invite_id", request.invite_id),
            ("invited_actor_id", request.invited_actor_id),
            ("invited_email_hash", request.invited_email_hash),
            ("requested_role", request.requested_role),
            ("expires_at", request.expires_at),
        ] {
            if value.trim().is_empty() {
                return Err(AuthAccessManagementError::Missing(field));
            }
        }
        let source_tenant_policy_preflight_receipt_id = self
            .ensure_auth_tenant_policy_preflight_source(
                request.source_tenant_policy_preflight_receipt_id,
            )?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "owner",
            AUTH_INVITE_REQUEST_ROUTE,
            "auth.invite.requested",
            request.invite_id,
        )
        .map_err(|error| AuthAccessManagementError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("auth_invite_request"),
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
        let decision = self.decide_with_receipt(&correlation, ActionKind::RequestAuthInvite);
        let invite_receipt = self.transition_receipt(
            &run_id,
            "REQUEST_AUTH_INVITE",
            &correlation,
            &decision,
            "auth.invite.requested",
            payload(&[
                ("invite_id", request.invite_id),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", AUTH_INVITE_REQUEST_ROUTE),
                ("projection_route", "/auth/invite-requests/projection.json"),
                (
                    "source_tenant_policy_preflight_receipt_id",
                    &source_tenant_policy_preflight_receipt_id,
                ),
                ("invited_actor_id", request.invited_actor_id),
                ("invited_email_hash", request.invited_email_hash),
                ("requested_role", request.requested_role),
                ("expires_at", request.expires_at),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("terminal_state", "AUTH_INVITE_RECORDED_DELIVERY_BLOCKED"),
                ("email_delivery_allowed", "false"),
                ("role_assignment_allowed", "false"),
                ("session_activation_allowed", "false"),
                ("service_role_shortcut_allowed", "false"),
                ("production_auth_provider_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_auth_access_run(&run_id, "AUTH_INVITE_RECORDED_DELIVERY_BLOCKED");
        Ok(AuthInviteRequestReport {
            status: "AUTH_INVITE_RECORDED_DELIVERY_BLOCKED",
            invite_id: request.invite_id.to_string(),
            invite_request_receipt_id: invite_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_tenant_policy_preflight_receipt_id,
            invited_actor_id: request.invited_actor_id.to_string(),
            invited_email_hash: request.invited_email_hash.to_string(),
            requested_role: request.requested_role.to_string(),
            expires_at: request.expires_at.to_string(),
            email_delivery_allowed: false,
            role_assignment_allowed: false,
            session_activation_allowed: false,
            service_role_shortcut_allowed: false,
            production_auth_provider_allowed: false,
            production_write_allowed: false,
        })
    }

    pub fn save_auth_role_assignment_local(
        &mut self,
        request: AuthRoleAssignment<'_>,
    ) -> Result<AuthRoleAssignmentReport, AuthAccessManagementError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_auth_role_assignment_local_with_identity(request, &identity)
    }

    pub fn save_auth_invite_redemption_local(
        &mut self,
        request: AuthInviteRedemption<'_>,
    ) -> Result<AuthInviteRedemptionReport, AuthAccessManagementError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_auth_invite_redemption_local_with_identity(request, &identity)
    }

    pub fn save_auth_invite_redemption_local_with_identity(
        &mut self,
        request: AuthInviteRedemption<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<AuthInviteRedemptionReport, AuthAccessManagementError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("redemption_id", request.redemption_id),
            (
                "source_invite_request_receipt_id",
                request.source_invite_request_receipt_id,
            ),
            ("authenticated_actor_id", request.authenticated_actor_id),
            ("cohort_id", request.cohort_id),
            ("risk_tier", request.risk_tier),
        ] {
            if value.trim().is_empty() {
                return Err(AuthAccessManagementError::Missing(field));
            }
        }
        let invite_receipt = self
            .storage
            .ledger()
            .query()
            .by_id(request.source_invite_request_receipt_id)
            .filter(|receipt| receipt.kind == "auth.invite.requested")
            .cloned()
            .ok_or_else(|| {
                AuthAccessManagementError::UnknownInviteRequestReceipt(
                    request.source_invite_request_receipt_id.to_string(),
                )
            })?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "owner",
            AUTH_INVITE_REDEMPTION_ROUTE,
            "auth.invite.redeemed",
            request.redemption_id,
        )
        .map_err(|error| AuthAccessManagementError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("auth_invite_redemption"),
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
        let decision = self.decide_with_receipt(&correlation, ActionKind::RequestAuthInvite);
        let invite_id = invite_receipt
            .payload
            .get("invite_id")
            .map(String::as_str)
            .unwrap_or("");
        let requested_role = invite_receipt
            .payload
            .get("requested_role")
            .map(String::as_str)
            .unwrap_or("");
        let redemption_receipt = self.transition_receipt(
            &run_id,
            "REDEEM_AUTH_INVITE",
            &correlation,
            &decision,
            "auth.invite.redeemed",
            payload(&[
                ("redemption_id", request.redemption_id),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", AUTH_INVITE_REDEMPTION_ROUTE),
                (
                    "projection_route",
                    "/auth/invite-redemptions/projection.json",
                ),
                (
                    "source_invite_request_receipt_id",
                    request.source_invite_request_receipt_id,
                ),
                ("invite_id", invite_id),
                ("authenticated_actor_id", request.authenticated_actor_id),
                ("requested_role", requested_role),
                ("cohort_id", request.cohort_id),
                ("risk_tier", request.risk_tier),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("terminal_state", "AUTH_INVITE_REDEEMED_ENROLLMENT_READY"),
                ("enrollment_record_allowed", "true"),
                ("session_cookie_write_allowed", "false"),
                ("production_auth_provider_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_auth_access_run(&run_id, "AUTH_INVITE_REDEEMED_ENROLLMENT_READY");
        Ok(AuthInviteRedemptionReport {
            status: "AUTH_INVITE_REDEEMED_ENROLLMENT_READY",
            redemption_id: request.redemption_id.to_string(),
            invite_redemption_receipt_id: redemption_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_invite_request_receipt_id: request.source_invite_request_receipt_id.to_string(),
            invite_id: invite_id.to_string(),
            authenticated_actor_id: request.authenticated_actor_id.to_string(),
            requested_role: requested_role.to_string(),
            cohort_id: request.cohort_id.to_string(),
            risk_tier: request.risk_tier.to_string(),
            enrollment_record_allowed: true,
            session_cookie_write_allowed: false,
            production_auth_provider_allowed: false,
            production_write_allowed: false,
        })
    }

    pub fn save_auth_role_assignment_local_with_identity(
        &mut self,
        request: AuthRoleAssignment<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<AuthRoleAssignmentReport, AuthAccessManagementError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("role_assignment_id", request.role_assignment_id),
            (
                "source_invite_request_receipt_id",
                request.source_invite_request_receipt_id,
            ),
            ("target_actor_id", request.target_actor_id),
            ("assigned_role", request.assigned_role),
            ("assignment_note", request.assignment_note),
        ] {
            if value.trim().is_empty() {
                return Err(AuthAccessManagementError::Missing(field));
            }
        }
        let invite_receipt = self
            .storage
            .ledger()
            .query()
            .by_id(request.source_invite_request_receipt_id)
            .filter(|receipt| receipt.kind == "auth.invite.requested")
            .cloned()
            .ok_or_else(|| {
                AuthAccessManagementError::UnknownInviteRequestReceipt(
                    request.source_invite_request_receipt_id.to_string(),
                )
            })?;
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "owner",
            AUTH_ROLE_ASSIGNMENT_ROUTE,
            "auth.role_assignment.recorded",
            request.role_assignment_id,
        )
        .map_err(|error| AuthAccessManagementError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("auth_role_assignment"),
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
        let decision = self.decide_with_receipt(&correlation, ActionKind::RecordAuthRoleAssignment);
        let assignment_receipt = self.transition_receipt(
            &run_id,
            "RECORD_AUTH_ROLE_ASSIGNMENT",
            &correlation,
            &decision,
            "auth.role_assignment.recorded",
            payload(&[
                ("role_assignment_id", request.role_assignment_id),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", AUTH_ROLE_ASSIGNMENT_ROUTE),
                ("projection_route", "/auth/role-assignments/projection.json"),
                (
                    "source_invite_request_receipt_id",
                    request.source_invite_request_receipt_id,
                ),
                (
                    "invite_id",
                    invite_receipt
                        .payload
                        .get("invite_id")
                        .map(String::as_str)
                        .unwrap_or(""),
                ),
                ("target_actor_id", request.target_actor_id),
                ("assigned_role", request.assigned_role),
                ("assignment_note", request.assignment_note),
                ("membership_state", "local_role_recorded_session_blocked"),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                (
                    "terminal_state",
                    "AUTH_ROLE_ASSIGNMENT_RECORDED_SESSION_BLOCKED",
                ),
                ("role_mapping_recorded", "true"),
                ("tenant_membership_recorded", "true"),
                ("session_cookie_write_allowed", "false"),
                ("role_escalation_allowed", "false"),
                ("production_auth_provider_allowed", "false"),
                ("production_role_write_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_auth_access_run(&run_id, "AUTH_ROLE_ASSIGNMENT_RECORDED_SESSION_BLOCKED");
        Ok(AuthRoleAssignmentReport {
            status: "AUTH_ROLE_ASSIGNMENT_RECORDED_SESSION_BLOCKED",
            role_assignment_id: request.role_assignment_id.to_string(),
            role_assignment_receipt_id: assignment_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_invite_request_receipt_id: request.source_invite_request_receipt_id.to_string(),
            invite_id: invite_receipt
                .payload
                .get("invite_id")
                .cloned()
                .unwrap_or_default(),
            target_actor_id: request.target_actor_id.to_string(),
            assigned_role: request.assigned_role.to_string(),
            assignment_note: request.assignment_note.to_string(),
            membership_state: "local_role_recorded_session_blocked".to_string(),
            role_mapping_recorded: true,
            tenant_membership_recorded: true,
            session_cookie_write_allowed: false,
            role_escalation_allowed: false,
            production_auth_provider_allowed: false,
            production_role_write_allowed: false,
            production_write_allowed: false,
        })
    }

    fn ensure_auth_tenant_policy_preflight_source(
        &mut self,
        requested_id: &str,
    ) -> Result<String, AuthAccessManagementError> {
        if !requested_id.trim().is_empty() {
            if self
                .storage
                .ledger()
                .query()
                .by_id(requested_id)
                .filter(|receipt| receipt.kind == "auth.tenant_policy.preflighted")
                .is_some()
            {
                return Ok(requested_id.to_string());
            }
            return Err(
                AuthAccessManagementError::UnknownTenantPolicyPreflightReceipt(
                    requested_id.to_string(),
                ),
            );
        }
        if let Some(receipt_id) = self
            .storage
            .ledger()
            .query()
            .by_kind("auth.tenant_policy.preflighted")
            .first()
            .map(|receipt| receipt.receipt_id.clone())
        {
            return Ok(receipt_id);
        }
        let report = self
            .save_auth_tenant_policy_preflight_local(AuthTenantPolicyPreflight {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                preflight_id: "auth_tenant_policy_preflight_for_access_management",
                source_auth_session_evidence_id: "auth.session.local.stub",
                tenant_membership_scope: "tenant_org_members",
                role_mapping_scope: "mdx_roles",
                invite_state_scope: "invites",
                approved_model_scope: "tenant_approved_models",
            })
            .map_err(|_| AuthAccessManagementError::Missing("tenant_policy_preflight_receipt"))?;
        Ok(report.preflight_receipt_id)
    }

    fn finish_auth_access_run(&mut self, run_id: &str, status: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = status.to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_access_management_records_invite_then_role_assignment() {
        let mut kernel = MdxKernel::boot_local();
        let invite = kernel
            .save_auth_invite_request_local(AuthInviteRequest {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                invite_id: "auth_invite_core_test",
                invited_actor_id: "human:new_engineer",
                invited_email_hash: "sha256:new-engineer",
                requested_role: "operator",
                source_tenant_policy_preflight_receipt_id: "",
                expires_at: "2026-01-02T00:00:00Z",
            })
            .expect("invite");
        assert_eq!(invite.status, "AUTH_INVITE_RECORDED_DELIVERY_BLOCKED");
        assert!(!invite.email_delivery_allowed);
        assert!(!invite.production_auth_provider_allowed);
        let redemption = kernel
            .save_auth_invite_redemption_local(AuthInviteRedemption {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                redemption_id: "auth_invite_redemption_core_test",
                source_invite_request_receipt_id: &invite.invite_request_receipt_id,
                authenticated_actor_id: "human:new_engineer",
                cohort_id: "cohort_a",
                risk_tier: "trusted",
            })
            .expect("redemption");
        assert_eq!(redemption.status, "AUTH_INVITE_REDEEMED_ENROLLMENT_READY");
        assert_eq!(redemption.requested_role, "operator");
        assert!(redemption.enrollment_record_allowed);
        assert!(!redemption.session_cookie_write_allowed);
        let role = kernel
            .save_auth_role_assignment_local(AuthRoleAssignment {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                role_assignment_id: "auth_role_assignment_core_test",
                source_invite_request_receipt_id: &invite.invite_request_receipt_id,
                target_actor_id: "human:new_engineer",
                assigned_role: "operator",
                assignment_note: "Local assignment proof only.",
            })
            .expect("role assignment");
        assert_eq!(role.status, "AUTH_ROLE_ASSIGNMENT_RECORDED_SESSION_BLOCKED");
        assert!(role.role_mapping_recorded);
        assert!(role.tenant_membership_recorded);
        assert!(!role.session_cookie_write_allowed);
        assert_eq!(
            kernel
                .ledger()
                .query()
                .by_id(&role.role_assignment_receipt_id)
                .map(|receipt| receipt.kind.as_str()),
            Some("auth.role_assignment.recorded")
        );
    }

    #[test]
    fn auth_role_assignment_requires_invite_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let error = kernel
            .save_auth_role_assignment_local(AuthRoleAssignment {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                role_assignment_id: "auth_role_assignment_missing_invite",
                source_invite_request_receipt_id: "missing",
                target_actor_id: "human:new_engineer",
                assigned_role: "operator",
                assignment_note: "Should fail.",
            })
            .expect_err("missing invite should fail");
        assert!(matches!(
            error,
            AuthAccessManagementError::UnknownInviteRequestReceipt(_)
        ));
    }

    #[test]
    fn auth_invite_redemption_requires_invite_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let error = kernel
            .save_auth_invite_redemption_local(AuthInviteRedemption {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                redemption_id: "auth_invite_redemption_missing_invite",
                source_invite_request_receipt_id: "missing",
                authenticated_actor_id: "human:new_engineer",
                cohort_id: "cohort_a",
                risk_tier: "trusted",
            })
            .expect_err("missing invite should fail");
        assert!(matches!(
            error,
            AuthAccessManagementError::UnknownInviteRequestReceipt(_)
        ));
    }
}
