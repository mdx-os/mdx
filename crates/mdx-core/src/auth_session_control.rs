use crate::*;

pub const AUTH_SESSION_CONTROL_ROUTE: &str = "/auth/session-controls.json";
pub const AUTH_SESSION_CONTROL_PROJECTION_ROUTE: &str = "/auth/session-controls/projection.json";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthSessionControl<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub control_id: &'a str,
    pub control_kind: &'a str,
    pub source_role_assignment_receipt_id: &'a str,
    pub source_session_activation_receipt_id: &'a str,
    pub target_actor_id: &'a str,
    pub current_role: &'a str,
    pub requested_role: &'a str,
    pub requested_tenant_id: &'a str,
    pub expires_at: &'a str,
    pub reason: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthSessionControlReport {
    pub status: &'static str,
    pub control_id: String,
    pub control_kind: String,
    pub control_receipt_kind: &'static str,
    pub control_receipt_id: String,
    pub policy_decision_id: String,
    pub source_role_assignment_receipt_id: String,
    pub source_session_activation_receipt_id: String,
    pub target_actor_id: String,
    pub current_role: String,
    pub requested_role: String,
    pub requested_tenant_id: String,
    pub expires_at: String,
    pub reason: String,
    pub session_activation_recorded: bool,
    pub session_revocation_recorded: bool,
    pub tenant_switch_refused: bool,
    pub role_escalation_refused: bool,
    pub actor_admission_status: String,
    pub session_cookie_write_allowed: bool,
    pub production_auth_provider_allowed: bool,
    pub production_role_write_allowed: bool,
    pub service_role_shortcut_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthSessionControlError {
    Missing(&'static str),
    UnknownRoleAssignmentReceipt(String),
    UnknownSessionActivationReceipt(String),
    UnknownControlKind(String),
    TenantSwitchMustChangeTenant,
    RoleEscalationMustChangeRole,
    ActorAdmission(String),
}

impl AuthSessionControlError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("auth session control missing {field}"),
            Self::UnknownRoleAssignmentReceipt(id) => {
                format!("auth session control source role assignment receipt {id} is unknown")
            }
            Self::UnknownSessionActivationReceipt(id) => {
                format!("auth session control source activation receipt {id} is unknown")
            }
            Self::UnknownControlKind(kind) => {
                format!("auth session control kind {kind} is not supported")
            }
            Self::TenantSwitchMustChangeTenant => {
                "auth session tenant switch refusal requires a different requested tenant"
                    .to_string()
            }
            Self::RoleEscalationMustChangeRole => {
                "auth session role escalation refusal requires a different requested role"
                    .to_string()
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

#[derive(Clone)]
struct AuthSessionControlMeta {
    receipt_kind: &'static str,
    action: ActionKind,
    transition: &'static str,
    terminal_state: &'static str,
}

impl AuthSessionControlMeta {
    fn for_kind(kind: &str) -> Result<Self, AuthSessionControlError> {
        match kind {
            "session_activation" => Ok(Self {
                receipt_kind: "auth.session.activation.recorded",
                action: ActionKind::RecordAuthSessionActivation,
                transition: "RECORD_AUTH_SESSION_ACTIVATION",
                terminal_state: "AUTH_SESSION_ACTIVATION_RECORDED_COOKIE_BLOCKED",
            }),
            "session_revocation" => Ok(Self {
                receipt_kind: "auth.session.revocation.recorded",
                action: ActionKind::RecordAuthSessionRevocation,
                transition: "RECORD_AUTH_SESSION_REVOCATION",
                terminal_state: "AUTH_SESSION_REVOCATION_RECORDED_PROVIDER_BLOCKED",
            }),
            "tenant_switch_refusal" => Ok(Self {
                receipt_kind: "auth.tenant_switch.refused",
                action: ActionKind::RefuseAuthTenantSwitch,
                transition: "REFUSE_AUTH_TENANT_SWITCH",
                terminal_state: "AUTH_TENANT_SWITCH_REFUSED_POLICY_REQUIRED",
            }),
            "role_escalation_refusal" => Ok(Self {
                receipt_kind: "auth.role_escalation.refused",
                action: ActionKind::RefuseAuthRoleEscalation,
                transition: "REFUSE_AUTH_ROLE_ESCALATION",
                terminal_state: "AUTH_ROLE_ESCALATION_REFUSED_POLICY_REQUIRED",
            }),
            other => Err(AuthSessionControlError::UnknownControlKind(
                other.to_string(),
            )),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_auth_session_control_local(
        &mut self,
        request: AuthSessionControl<'_>,
    ) -> Result<AuthSessionControlReport, AuthSessionControlError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_auth_session_control_local_with_identity(request, &identity)
    }

    pub fn save_auth_session_control_local_with_identity(
        &mut self,
        request: AuthSessionControl<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<AuthSessionControlReport, AuthSessionControlError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("control_id", request.control_id),
            ("control_kind", request.control_kind),
            ("target_actor_id", request.target_actor_id),
            ("current_role", request.current_role),
            ("requested_role", request.requested_role),
            ("requested_tenant_id", request.requested_tenant_id),
            ("expires_at", request.expires_at),
            ("reason", request.reason),
        ] {
            if value.trim().is_empty() {
                return Err(AuthSessionControlError::Missing(field));
            }
        }
        let meta = AuthSessionControlMeta::for_kind(request.control_kind)?;
        if request.control_kind == "tenant_switch_refusal"
            && request.requested_tenant_id == request.tenant_id
        {
            return Err(AuthSessionControlError::TenantSwitchMustChangeTenant);
        }
        if request.control_kind == "role_escalation_refusal"
            && request.requested_role == request.current_role
        {
            return Err(AuthSessionControlError::RoleEscalationMustChangeRole);
        }
        let role_assignment_receipt_id =
            self.ensure_auth_role_assignment_source(request.source_role_assignment_receipt_id)?;
        let session_activation_receipt_id = self
            .ensure_auth_session_activation_source(request.source_session_activation_receipt_id)?;
        let actor_admission_status = self.auth_session_control_actor_admission_status(&request)?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("auth_session_control"),
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
        let decision = self.decide_with_receipt(&correlation, meta.action);
        let receipt = self.transition_receipt(
            &run_id,
            meta.transition,
            &correlation,
            &decision,
            meta.receipt_kind,
            payload(&[
                ("control_id", request.control_id),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("control_kind", request.control_kind),
                ("source_route", AUTH_SESSION_CONTROL_ROUTE),
                ("projection_route", AUTH_SESSION_CONTROL_PROJECTION_ROUTE),
                (
                    "source_role_assignment_receipt_id",
                    &role_assignment_receipt_id,
                ),
                (
                    "source_session_activation_receipt_id",
                    &session_activation_receipt_id,
                ),
                ("target_actor_id", request.target_actor_id),
                ("current_role", request.current_role),
                ("requested_role", request.requested_role),
                ("requested_tenant_id", request.requested_tenant_id),
                ("expires_at", request.expires_at),
                ("reason", request.reason),
                ("actor_admission_status", &actor_admission_status),
                ("terminal_state", meta.terminal_state),
                (
                    "session_activation_recorded",
                    bool_string(request.control_kind == "session_activation"),
                ),
                (
                    "session_revocation_recorded",
                    bool_string(request.control_kind == "session_revocation"),
                ),
                (
                    "tenant_switch_refused",
                    bool_string(request.control_kind == "tenant_switch_refusal"),
                ),
                (
                    "role_escalation_refused",
                    bool_string(request.control_kind == "role_escalation_refusal"),
                ),
                ("session_cookie_write_allowed", "false"),
                ("production_auth_provider_allowed", "false"),
                ("production_role_write_allowed", "false"),
                ("service_role_shortcut_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_auth_session_control_run(&run_id, meta.terminal_state);
        Ok(AuthSessionControlReport {
            status: meta.terminal_state,
            control_id: request.control_id.to_string(),
            control_kind: request.control_kind.to_string(),
            control_receipt_kind: meta.receipt_kind,
            control_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_role_assignment_receipt_id: role_assignment_receipt_id,
            source_session_activation_receipt_id: session_activation_receipt_id,
            target_actor_id: request.target_actor_id.to_string(),
            current_role: request.current_role.to_string(),
            requested_role: request.requested_role.to_string(),
            requested_tenant_id: request.requested_tenant_id.to_string(),
            expires_at: request.expires_at.to_string(),
            reason: request.reason.to_string(),
            session_activation_recorded: request.control_kind == "session_activation",
            session_revocation_recorded: request.control_kind == "session_revocation",
            tenant_switch_refused: request.control_kind == "tenant_switch_refusal",
            role_escalation_refused: request.control_kind == "role_escalation_refusal",
            actor_admission_status,
            session_cookie_write_allowed: false,
            production_auth_provider_allowed: false,
            production_role_write_allowed: false,
            service_role_shortcut_allowed: false,
            production_write_allowed: false,
        })
    }

    fn ensure_auth_role_assignment_source(
        &mut self,
        requested_id: &str,
    ) -> Result<String, AuthSessionControlError> {
        if !requested_id.trim().is_empty() {
            if self
                .storage
                .ledger()
                .query()
                .by_id(requested_id)
                .filter(|receipt| receipt.kind == "auth.role_assignment.recorded")
                .is_some()
            {
                return Ok(requested_id.to_string());
            }
            return Err(AuthSessionControlError::UnknownRoleAssignmentReceipt(
                requested_id.to_string(),
            ));
        }
        if let Some(receipt_id) = self
            .storage
            .ledger()
            .query()
            .by_kind("auth.role_assignment.recorded")
            .first()
            .map(|receipt| receipt.receipt_id.clone())
        {
            return Ok(receipt_id);
        }
        let invite = self
            .save_auth_invite_request_local(AuthInviteRequest {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                invite_id: "auth_invite_for_session_control",
                invited_actor_id: "human:session_control_user",
                invited_email_hash: "sha256:session-control-user",
                requested_role: "operator",
                source_tenant_policy_preflight_receipt_id: "",
                expires_at: "2026-01-02T00:00:00Z",
            })
            .map_err(|_| AuthSessionControlError::Missing("invite_request_receipt"))?;
        let role = self
            .save_auth_role_assignment_local(AuthRoleAssignment {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                role_assignment_id: "auth_role_assignment_for_session_control",
                source_invite_request_receipt_id: &invite.invite_request_receipt_id,
                target_actor_id: "human:session_control_user",
                assigned_role: "operator",
                assignment_note: "Local session control proof only.",
            })
            .map_err(|_| AuthSessionControlError::Missing("role_assignment_receipt"))?;
        Ok(role.role_assignment_receipt_id)
    }

    fn ensure_auth_session_activation_source(
        &self,
        requested_id: &str,
    ) -> Result<String, AuthSessionControlError> {
        if !requested_id.trim().is_empty() {
            if self
                .storage
                .ledger()
                .query()
                .by_id(requested_id)
                .filter(|receipt| receipt.kind == "auth.session.activation.recorded")
                .is_some()
            {
                return Ok(requested_id.to_string());
            }
            return Err(AuthSessionControlError::UnknownSessionActivationReceipt(
                requested_id.to_string(),
            ));
        }
        Ok(self
            .storage
            .ledger()
            .query()
            .by_kind("auth.session.activation.recorded")
            .first()
            .map(|receipt| receipt.receipt_id.clone())
            .unwrap_or_else(|| "local_session_activation_pending".to_string()))
    }

    fn auth_session_control_actor_admission_status(
        &self,
        request: &AuthSessionControl<'_>,
    ) -> Result<String, AuthSessionControlError> {
        if request.control_kind == "tenant_switch_refusal" {
            let error = admit_local_actor_for_governed_write(ActorAdmission {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                actor_role: request.current_role,
                policy_decision_id: "policy-local-actor-admission",
                receipt_intent: "auth.tenant_switch.refused",
                source_route: AUTH_SESSION_CONTROL_ROUTE,
                request_id: request.control_id,
                requested_tenant_id: request.requested_tenant_id,
                requested_actor_role: request.current_role,
                service_role_claimed: false,
                user_metadata_authorization_claimed: false,
            })
            .expect_err("tenant switch must be refused");
            return Ok(format!("REFUSED:{}", error.message()));
        }
        if request.control_kind == "role_escalation_refusal" {
            let error = admit_local_actor_for_governed_write(ActorAdmission {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                actor_role: request.current_role,
                policy_decision_id: "policy-local-actor-admission",
                receipt_intent: "auth.role_escalation.refused",
                source_route: AUTH_SESSION_CONTROL_ROUTE,
                request_id: request.control_id,
                requested_tenant_id: request.tenant_id,
                requested_actor_role: request.requested_role,
                service_role_claimed: false,
                user_metadata_authorization_claimed: false,
            })
            .expect_err("role escalation must be refused");
            return Ok(format!("REFUSED:{}", error.message()));
        }
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            request.current_role,
            AUTH_SESSION_CONTROL_ROUTE,
            control_receipt_intent(request.control_kind),
            request.control_id,
        )
        .map_err(|error| AuthSessionControlError::ActorAdmission(error.message()))?;
        Ok(actor_admission.status.to_string())
    }

    fn finish_auth_session_control_run(&mut self, run_id: &str, status: &str) {
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

fn control_receipt_intent(control_kind: &str) -> &str {
    match control_kind {
        "session_activation" => "auth.session.activation.recorded",
        "session_revocation" => "auth.session.revocation.recorded",
        "tenant_switch_refusal" => "auth.tenant_switch.refused",
        "role_escalation_refusal" => "auth.role_escalation.refused",
        _ => "auth.session.control",
    }
}

fn bool_string(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_session_controls_record_lifecycle_and_denials() {
        let mut kernel = MdxKernel::boot_local();
        for (kind, expected_status, expected_receipt_kind) in [
            (
                "session_activation",
                "AUTH_SESSION_ACTIVATION_RECORDED_COOKIE_BLOCKED",
                "auth.session.activation.recorded",
            ),
            (
                "session_revocation",
                "AUTH_SESSION_REVOCATION_RECORDED_PROVIDER_BLOCKED",
                "auth.session.revocation.recorded",
            ),
            (
                "tenant_switch_refusal",
                "AUTH_TENANT_SWITCH_REFUSED_POLICY_REQUIRED",
                "auth.tenant_switch.refused",
            ),
            (
                "role_escalation_refusal",
                "AUTH_ROLE_ESCALATION_REFUSED_POLICY_REQUIRED",
                "auth.role_escalation.refused",
            ),
        ] {
            let report = kernel
                .save_auth_session_control_local(AuthSessionControl {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    control_id: &format!("auth_session_control_{kind}"),
                    control_kind: kind,
                    source_role_assignment_receipt_id: "",
                    source_session_activation_receipt_id: "",
                    target_actor_id: "human:session_control_user",
                    current_role: "operator",
                    requested_role: "owner",
                    requested_tenant_id: "other_tenant",
                    expires_at: "2026-01-02T00:00:00Z",
                    reason: "Local governed session proof.",
                })
                .expect("session control");
            assert_eq!(report.status, expected_status);
            assert_eq!(report.control_receipt_kind, expected_receipt_kind);
            assert!(!report.session_cookie_write_allowed);
            assert!(!report.production_auth_provider_allowed);
            let receipt = kernel
                .ledger()
                .query()
                .by_id(&report.control_receipt_id)
                .expect("control receipt");
            assert_eq!(receipt.kind, expected_receipt_kind);
            assert_eq!(
                receipt.payload.get("source_route").map(String::as_str),
                Some(AUTH_SESSION_CONTROL_ROUTE)
            );
        }
        assert_eq!(
            kernel
                .ledger()
                .query()
                .by_kind("auth.session.activation.recorded")
                .len(),
            1
        );
        assert_eq!(
            kernel
                .ledger()
                .query()
                .by_kind("auth.tenant_switch.refused")
                .len(),
            1
        );
    }

    #[test]
    fn auth_session_controls_reject_unknown_source_or_unsafe_refusal_shape() {
        let mut kernel = MdxKernel::boot_local();
        let error = kernel
            .save_auth_session_control_local(AuthSessionControl {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                control_id: "auth_session_control_bad_source",
                control_kind: "session_activation",
                source_role_assignment_receipt_id: "missing",
                source_session_activation_receipt_id: "",
                target_actor_id: "human:session_control_user",
                current_role: "operator",
                requested_role: "owner",
                requested_tenant_id: "other_tenant",
                expires_at: "2026-01-02T00:00:00Z",
                reason: "Should fail.",
            })
            .expect_err("missing role assignment should fail");
        assert!(matches!(
            error,
            AuthSessionControlError::UnknownRoleAssignmentReceipt(_)
        ));

        let error = kernel
            .save_auth_session_control_local(AuthSessionControl {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                control_id: "auth_session_control_same_tenant",
                control_kind: "tenant_switch_refusal",
                source_role_assignment_receipt_id: "",
                source_session_activation_receipt_id: "",
                target_actor_id: "human:session_control_user",
                current_role: "operator",
                requested_role: "owner",
                requested_tenant_id: "local_tenant",
                expires_at: "2026-01-02T00:00:00Z",
                reason: "Should fail.",
            })
            .expect_err("same tenant refusal should fail");
        assert!(matches!(
            error,
            AuthSessionControlError::TenantSwitchMustChangeTenant
        ));
    }
}
