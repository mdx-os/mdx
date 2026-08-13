use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthTenantPolicyPreflight<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub preflight_id: &'a str,
    pub source_auth_session_evidence_id: &'a str,
    pub tenant_membership_scope: &'a str,
    pub role_mapping_scope: &'a str,
    pub invite_state_scope: &'a str,
    pub approved_model_scope: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthTenantPolicyPreflightReport {
    pub status: &'static str,
    pub preflight_id: String,
    pub preflight_receipt_id: String,
    pub policy_decision_id: String,
    pub source_auth_session_evidence_id: String,
    pub tenant_membership_scope: String,
    pub role_mapping_scope: String,
    pub invite_state_scope: String,
    pub approved_model_scope: String,
    pub v1_helper_mapping_recorded: bool,
    pub tenant_membership_policy_recorded: bool,
    pub role_mapping_policy_recorded: bool,
    pub invite_policy_recorded: bool,
    pub approved_model_policy_recorded: bool,
    pub service_role_shortcut_allowed: bool,
    pub user_metadata_authorization_allowed: bool,
    pub implicit_tenant_inference_allowed: bool,
    pub production_role_write_allowed: bool,
    pub production_auth_provider_allowed: bool,
    pub supabase_auth_claim_authority_allowed: bool,
    pub cutover_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthTenantPolicyPreflightError {
    Missing(&'static str),
    ActorAdmission(String),
}

impl AuthTenantPolicyPreflightError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("auth tenant policy preflight missing {field}"),
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_auth_tenant_policy_preflight_local(
        &mut self,
        request: AuthTenantPolicyPreflight<'_>,
    ) -> Result<AuthTenantPolicyPreflightReport, AuthTenantPolicyPreflightError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_auth_tenant_policy_preflight_local_with_identity(request, &identity)
    }

    pub fn save_auth_tenant_policy_preflight_local_with_identity(
        &mut self,
        request: AuthTenantPolicyPreflight<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<AuthTenantPolicyPreflightReport, AuthTenantPolicyPreflightError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("preflight_id", request.preflight_id),
            (
                "source_auth_session_evidence_id",
                request.source_auth_session_evidence_id,
            ),
            ("tenant_membership_scope", request.tenant_membership_scope),
            ("role_mapping_scope", request.role_mapping_scope),
            ("invite_state_scope", request.invite_state_scope),
            ("approved_model_scope", request.approved_model_scope),
        ] {
            if value.trim().is_empty() {
                return Err(AuthTenantPolicyPreflightError::Missing(field));
            }
        }
        let actor_admission = admit_local_route_actor(
            request.tenant_id,
            request.actor_id,
            "owner",
            "/auth/tenant-policy-preflights.json",
            "auth.tenant_policy.preflighted",
            request.preflight_id,
        )
        .map_err(|error| AuthTenantPolicyPreflightError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("auth_tenant_policy_preflight"),
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
            self.decide_with_receipt(&correlation, ActionKind::PreflightAuthTenantPolicy);
        let preflight_receipt = self.transition_receipt(
            &run_id,
            "PREFLIGHT_AUTH_TENANT_POLICY",
            &correlation,
            &decision,
            "auth.tenant_policy.preflighted",
            payload(&[
                ("preflight_id", request.preflight_id),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("source_route", "/auth/tenant-policy-preflights.json"),
                (
                    "projection_route",
                    "/auth/tenant-policy-preflights/projection.json",
                ),
                (
                    "source_auth_session_evidence_id",
                    request.source_auth_session_evidence_id,
                ),
                ("tenant_membership_scope", request.tenant_membership_scope),
                ("role_mapping_scope", request.role_mapping_scope),
                ("invite_state_scope", request.invite_state_scope),
                ("approved_model_scope", request.approved_model_scope),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                (
                    "terminal_state",
                    "AUTH_TENANT_POLICY_PREFLIGHT_RECORDED_PRODUCTION_AUTH_BLOCKED",
                ),
                ("v1_helper_mapping_recorded", "true"),
                ("tenant_membership_policy_recorded", "true"),
                ("role_mapping_policy_recorded", "true"),
                ("invite_policy_recorded", "true"),
                ("approved_model_policy_recorded", "true"),
                ("service_role_shortcut_allowed", "false"),
                ("user_metadata_authorization_allowed", "false"),
                ("implicit_tenant_inference_allowed", "false"),
                ("production_role_write_allowed", "false"),
                ("production_auth_provider_allowed", "false"),
                ("supabase_auth_claim_authority_allowed", "false"),
                ("cutover_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_auth_tenant_policy_preflight_run(&run_id);
        Ok(AuthTenantPolicyPreflightReport {
            status: "AUTH_TENANT_POLICY_PREFLIGHT_RECORDED_PRODUCTION_AUTH_BLOCKED",
            preflight_id: request.preflight_id.to_string(),
            preflight_receipt_id: preflight_receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            source_auth_session_evidence_id: request.source_auth_session_evidence_id.to_string(),
            tenant_membership_scope: request.tenant_membership_scope.to_string(),
            role_mapping_scope: request.role_mapping_scope.to_string(),
            invite_state_scope: request.invite_state_scope.to_string(),
            approved_model_scope: request.approved_model_scope.to_string(),
            v1_helper_mapping_recorded: true,
            tenant_membership_policy_recorded: true,
            role_mapping_policy_recorded: true,
            invite_policy_recorded: true,
            approved_model_policy_recorded: true,
            service_role_shortcut_allowed: false,
            user_metadata_authorization_allowed: false,
            implicit_tenant_inference_allowed: false,
            production_role_write_allowed: false,
            production_auth_provider_allowed: false,
            supabase_auth_claim_authority_allowed: false,
            cutover_allowed: false,
            production_write_allowed: false,
        })
    }

    fn finish_auth_tenant_policy_preflight_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status =
                "AUTH_TENANT_POLICY_PREFLIGHT_RECORDED_PRODUCTION_AUTH_BLOCKED".to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_tenant_policy_preflight_records_production_auth_blocked_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .save_auth_tenant_policy_preflight_local(AuthTenantPolicyPreflight {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                preflight_id: "auth_tenant_policy_preflight_core_test",
                source_auth_session_evidence_id: "auth.session.local.stub",
                tenant_membership_scope: "tenant_org_members",
                role_mapping_scope: "mdx_roles",
                invite_state_scope: "invites",
                approved_model_scope: "tenant_approved_models",
            })
            .expect("auth tenant policy preflight");

        assert_eq!(
            report.status,
            "AUTH_TENANT_POLICY_PREFLIGHT_RECORDED_PRODUCTION_AUTH_BLOCKED"
        );
        assert!(report.v1_helper_mapping_recorded);
        assert!(report.tenant_membership_policy_recorded);
        assert!(report.role_mapping_policy_recorded);
        assert!(report.invite_policy_recorded);
        assert!(report.approved_model_policy_recorded);
        assert!(!report.service_role_shortcut_allowed);
        assert!(!report.user_metadata_authorization_allowed);
        assert!(!report.implicit_tenant_inference_allowed);
        assert!(!report.production_auth_provider_allowed);
        assert!(!report.cutover_allowed);
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.preflight_receipt_id)
            .expect("preflight receipt");
        assert_eq!(receipt.kind, "auth.tenant_policy.preflighted");
        assert_eq!(
            receipt.payload.get("source_route").map(String::as_str),
            Some("/auth/tenant-policy-preflights.json")
        );
        assert_eq!(
            receipt
                .payload
                .get("actor_admission_status")
                .map(String::as_str),
            Some("LOCAL_ACTOR_ADMITTED")
        );
    }

    #[test]
    fn auth_tenant_policy_preflight_requires_source_auth_session_evidence() {
        let mut kernel = MdxKernel::boot_local();
        let error = kernel
            .save_auth_tenant_policy_preflight_local(AuthTenantPolicyPreflight {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                preflight_id: "auth_tenant_policy_preflight_missing_source",
                source_auth_session_evidence_id: "",
                tenant_membership_scope: "tenant_org_members",
                role_mapping_scope: "mdx_roles",
                invite_state_scope: "invites",
                approved_model_scope: "tenant_approved_models",
            })
            .expect_err("missing source auth evidence should fail");
        assert!(matches!(
            error,
            AuthTenantPolicyPreflightError::Missing("source_auth_session_evidence_id")
        ));
    }
}
