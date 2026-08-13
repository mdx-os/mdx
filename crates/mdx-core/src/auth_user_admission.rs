use crate::*;

pub const AUTH_USER_ADMISSION_RECEIPT_KIND: &str = "auth.user_admission.approved";
pub const AUTH_USER_ADMISSION_PROVIDERS: &[&str] = &["supabase", "oidc", "saml_assertion_gateway"];
pub const AUTH_USER_ADMISSION_ROLES: &[&str] = &[
    "owner",
    "operator",
    "admin",
    "auditor",
    "viewer",
    "tenant_owner",
    "tenant_admin",
    "security_operator",
    "workspace_admin",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthUserAdmission<'a> {
    pub tenant_id: &'a str,
    pub approver_actor_id: &'a str,
    pub target_auth_user_id: &'a str,
    pub mapped_role: &'a str,
    pub auth_provider: &'a str,
    pub membership_state: &'a str,
    pub human_approval_reference: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthUserAdmissionReport {
    pub admission_receipt_id: String,
    pub policy_decision_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordedAuthUserAdmission {
    pub report: AuthUserAdmissionReport,
    pub receipts: Vec<Receipt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthUserAdmissionError {
    Missing(&'static str),
    InvalidAuthUserId,
    InvalidAuthProvider,
    InvalidRole,
    InvalidMembershipState,
    AlreadyAdmitted,
    PolicyRefused(String),
    InvalidExistingChain(String),
}

impl AuthUserAdmissionError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("auth user admission missing {field}"),
            Self::InvalidAuthUserId => {
                "auth user admission target_auth_user_id must be a UUID".to_string()
            }
            Self::InvalidAuthProvider => {
                "auth user admission auth_provider is not allowed".to_string()
            }
            Self::InvalidRole => "auth user admission mapped_role is not allowed".to_string(),
            Self::InvalidMembershipState => {
                "auth user admission membership_state must be active, beta_active, or production_active"
                    .to_string()
            }
            Self::AlreadyAdmitted => {
                "auth user admission target already has an admission receipt".to_string()
            }
            Self::PolicyRefused(outcome) => {
                format!("auth user admission policy outcome was {outcome}")
            }
            Self::InvalidExistingChain(error) => {
                format!("auth user admission refused invalid existing chain: {error}")
            }
        }
    }
}

fn uuid_like(value: &str) -> bool {
    value.len() == 36
        && value.chars().enumerate().all(|(index, ch)| match index {
            8 | 13 | 18 | 23 => ch == '-',
            _ => ch.is_ascii_hexdigit(),
        })
}

fn validate_auth_user_admission<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    request: AuthUserAdmission<'_>,
) -> Result<(), AuthUserAdmissionError> {
    for (field, value) in [
        ("tenant_id", request.tenant_id),
        ("approver_actor_id", request.approver_actor_id),
        ("target_auth_user_id", request.target_auth_user_id),
        ("mapped_role", request.mapped_role),
        ("auth_provider", request.auth_provider),
        ("membership_state", request.membership_state),
        ("human_approval_reference", request.human_approval_reference),
    ] {
        if value.trim().is_empty() {
            return Err(AuthUserAdmissionError::Missing(field));
        }
    }
    if !uuid_like(request.target_auth_user_id) {
        return Err(AuthUserAdmissionError::InvalidAuthUserId);
    }
    if !AUTH_USER_ADMISSION_PROVIDERS.contains(&request.auth_provider) {
        return Err(AuthUserAdmissionError::InvalidAuthProvider);
    }
    if !AUTH_USER_ADMISSION_ROLES.contains(&request.mapped_role) {
        return Err(AuthUserAdmissionError::InvalidRole);
    }
    if !["active", "beta_active", "production_active"].contains(&request.membership_state) {
        return Err(AuthUserAdmissionError::InvalidMembershipState);
    }
    if kernel.ledger().entries().iter().any(|receipt| {
        receipt.kind == AUTH_USER_ADMISSION_RECEIPT_KIND
            && receipt.tenant_id.as_str() == request.tenant_id
            && receipt
                .payload
                .get("target_auth_user_id")
                .is_some_and(|value| value == request.target_auth_user_id)
    }) {
        return Err(AuthUserAdmissionError::AlreadyAdmitted);
    }
    Ok(())
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn approve_auth_user_admission(
        &mut self,
        request: AuthUserAdmission<'_>,
    ) -> Result<AuthUserAdmissionReport, AuthUserAdmissionError> {
        validate_auth_user_admission(self, request)?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace_auth_user_admission")),
            actor_id: ActorId::new(request.approver_actor_id),
            loop_id: LoopId::new("auth_user_admission"),
            workflow_id: WorkflowId::new(self.ids.next("workflow_auth_user_admission")),
        };
        let decision = self.decide_with_receipt(&correlation, ActionKind::ApproveAuthUserAdmission);
        if decision.outcome != PolicyOutcome::Allow {
            return Err(AuthUserAdmissionError::PolicyRefused(
                decision.outcome.as_str().to_string(),
            ));
        }
        let receipt = self.storage.append_receipt(
            &mut self.ids,
            &correlation,
            AUTH_USER_ADMISSION_RECEIPT_KIND,
            Some(decision.policy_decision_id.clone()),
            payload(&[
                ("target_auth_user_id", request.target_auth_user_id),
                ("mapped_role", request.mapped_role),
                ("human_approval_reference", request.human_approval_reference),
                ("auth_provider", request.auth_provider),
                ("tenant_membership_scope", "tenant"),
                ("membership_state", request.membership_state),
                ("role_mapping_scope", "mdx_roles"),
                ("service_role_shortcut_allowed", "false"),
                ("user_metadata_authorization_allowed", "false"),
                ("production_auth_provider_allowed", "true"),
                ("hosted_auth_claim_authority_allowed", "true"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.ledger()
            .verify()
            .map_err(AuthUserAdmissionError::InvalidExistingChain)?;
        Ok(AuthUserAdmissionReport {
            admission_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
        })
    }
}

pub fn record_auth_user_admission(
    existing_receipts: &[Receipt],
    ids_counter: u64,
    request: AuthUserAdmission<'_>,
) -> Result<RecordedAuthUserAdmission, AuthUserAdmissionError> {
    let mut kernel = MdxKernel::boot_local();
    kernel
        .restore_ledger_entries(existing_receipts.to_vec())
        .map_err(AuthUserAdmissionError::InvalidExistingChain)?;
    kernel.restore_ids_counter(ids_counter);
    let original_receipt_count = kernel.ledger().entries().len();
    let report = kernel.approve_auth_user_admission(request)?;
    Ok(RecordedAuthUserAdmission {
        report,
        receipts: kernel.ledger().entries()[original_receipt_count..].to_vec(),
    })
}

#[cfg(test)]
pub(crate) fn auth_user_admission_policy_coverage_request<'a>() -> AuthUserAdmission<'a> {
    AuthUserAdmission {
        tenant_id: "local_tenant",
        approver_actor_id: "local_user",
        target_auth_user_id: "11111111-2222-3333-4444-555555555555",
        mapped_role: "viewer",
        auth_provider: "oidc",
        membership_state: "active",
        human_approval_reference: "policy_coverage_auth_admission",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request<'a>() -> AuthUserAdmission<'a> {
        AuthUserAdmission {
            tenant_id: "tenant_beta",
            approver_actor_id: "human:founder",
            target_auth_user_id: "11111111-2222-3333-4444-555555555555",
            mapped_role: "owner",
            auth_provider: "supabase",
            membership_state: "beta_active",
            human_approval_reference: "approval_beta_auth_001",
        }
    }

    #[test]
    fn records_a_chain_valid_presence_safe_admission_receipt() {
        let recorded = record_auth_user_admission(&[], 0, request()).expect("admission receipt");
        assert_eq!(recorded.receipts.len(), 2);
        assert_eq!(recorded.receipts[0].kind, POLICY_DECISION_RECEIPT_KIND);
        let receipt = &recorded.receipts[1];
        assert_eq!(receipt.kind, AUTH_USER_ADMISSION_RECEIPT_KIND);
        assert_eq!(
            receipt.policy_decision_id.as_deref(),
            Some(recorded.report.policy_decision_id.as_str())
        );
        assert_eq!(receipt.payload["auth_provider"], "supabase");
        assert_eq!(receipt.payload["membership_state"], "beta_active");
        assert_eq!(receipt.payload["service_role_shortcut_allowed"], "false");
        assert_eq!(
            receipt.payload["user_metadata_authorization_allowed"],
            "false"
        );
        assert_eq!(
            receipt.payload["hosted_auth_claim_authority_allowed"],
            "true"
        );
        let mut restored = Ledger::default();
        restored
            .restore_entries(recorded.receipts)
            .expect("receipt chain verifies");
    }

    #[test]
    fn projects_only_receipt_backed_membership_and_role_authority() {
        let recorded = record_auth_user_admission(&[], 0, request()).expect("admission receipt");
        let sql = crate::render_postgres_app_state_export_sql(&recorded.receipts, &[]);
        assert!(sql.contains("INSERT INTO auth_tenant_orgs"));
        assert!(sql.contains("INSERT INTO auth_tenant_memberships"));
        assert!(sql.contains("INSERT INTO auth_role_mappings"));
        assert!(sql.contains("'beta_active'"));
        assert!(sql.contains("'11111111-2222-3333-4444-555555555555'"));
        assert!(sql.contains("'owner'"));
        assert!(sql.contains("'beta_active', false)"));

        let mut unbacked_receipt = recorded.receipts[1].clone();
        unbacked_receipt.policy_decision_id = None;
        let refused_sql = crate::render_postgres_app_state_export_sql(&[unbacked_receipt], &[]);
        assert!(!refused_sql.contains("INSERT INTO auth_tenant_orgs"));
        assert!(!refused_sql.contains("INSERT INTO auth_tenant_memberships"));
        assert!(!refused_sql.contains("INSERT INTO auth_role_mappings"));
    }

    #[test]
    fn refuses_invalid_users_roles_states_and_duplicate_admission() {
        let mut invalid_user = request();
        invalid_user.target_auth_user_id = "not-a-uuid";
        assert_eq!(
            record_auth_user_admission(&[], 0, invalid_user),
            Err(AuthUserAdmissionError::InvalidAuthUserId)
        );
        let mut invalid_role = request();
        invalid_role.mapped_role = "superuser";
        assert_eq!(
            record_auth_user_admission(&[], 0, invalid_role),
            Err(AuthUserAdmissionError::InvalidRole)
        );
        let mut invalid_provider = request();
        invalid_provider.auth_provider = "arbitrary";
        assert_eq!(
            record_auth_user_admission(&[], 0, invalid_provider),
            Err(AuthUserAdmissionError::InvalidAuthProvider)
        );
        let mut invalid_state = request();
        invalid_state.membership_state = "invited";
        assert_eq!(
            record_auth_user_admission(&[], 0, invalid_state),
            Err(AuthUserAdmissionError::InvalidMembershipState)
        );
        let first = record_auth_user_admission(&[], 0, request()).expect("first admission");
        assert_eq!(
            record_auth_user_admission(&first.receipts, 5, request()),
            Err(AuthUserAdmissionError::AlreadyAdmitted)
        );
    }
}
