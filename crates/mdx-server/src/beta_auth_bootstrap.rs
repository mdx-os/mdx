use mdx_core::{AuthUserAdmission, record_auth_user_admission};

fn require_operator_gates(
    enabled: Option<&str>,
    maintenance_ack: Option<&str>,
) -> Result<(), String> {
    if enabled != Some("1") {
        return Err("MDX_BETA_AUTH_BOOTSTRAP_ENABLED=1 is required".to_string());
    }
    if maintenance_ack != Some("1") {
        return Err(
            "MDX_BETA_AUTH_BOOTSTRAP_MAINTENANCE_ACK=1 is required; hold governed writes and restart the kernel after admission"
                .to_string(),
        );
    }
    Ok(())
}

pub(crate) fn run(args: &[String]) -> Result<String, String> {
    let enabled = std::env::var("MDX_BETA_AUTH_BOOTSTRAP_ENABLED").ok();
    let maintenance_ack = std::env::var("MDX_BETA_AUTH_BOOTSTRAP_MAINTENANCE_ACK").ok();
    require_operator_gates(enabled.as_deref(), maintenance_ack.as_deref())?;
    let tenant_id = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| "bootstrap-beta-auth requires tenant_id".to_string())?;
    let target_auth_user_id = args
        .get(1)
        .map(String::as_str)
        .ok_or_else(|| "bootstrap-beta-auth requires target_auth_user_id".to_string())?;
    let mapped_role = args
        .get(2)
        .map(String::as_str)
        .ok_or_else(|| "bootstrap-beta-auth requires mapped_role".to_string())?;
    let approver_actor_id = std::env::var("MDX_BETA_AUTH_BOOTSTRAP_APPROVER_ACTOR_ID")
        .map_err(|_| "MDX_BETA_AUTH_BOOTSTRAP_APPROVER_ACTOR_ID is required".to_string())?;
    let human_approval_reference = std::env::var("MDX_BETA_AUTH_BOOTSTRAP_APPROVAL_REFERENCE")
        .map_err(|_| "MDX_BETA_AUTH_BOOTSTRAP_APPROVAL_REFERENCE is required".to_string())?;
    let database_url =
        std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is required".to_string())?;
    let existing = crate::postgres_exec::query_ledger_entries(&database_url)?;
    let ids_counter = crate::postgres_exec::max_id_suffix(&existing);
    let recorded = record_auth_user_admission(
        &existing,
        ids_counter,
        AuthUserAdmission {
            tenant_id,
            approver_actor_id: &approver_actor_id,
            target_auth_user_id,
            mapped_role,
            auth_provider: "supabase",
            membership_state: "beta_active",
            human_approval_reference: &human_approval_reference,
        },
    )
    .map_err(|error| error.message())?;
    crate::app_state_export::write_auth_user_admission_postgres(&recorded.receipts)?;
    Ok(format!(
        "beta_auth_bootstrap: OK receipt_id={} policy_decision_id={} role={} service_role_shortcut_allowed=false user_metadata_authorization_allowed=false",
        recorded.report.admission_receipt_id, recorded.report.policy_decision_id, mapped_role
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_command_requires_enablement_and_a_maintenance_window() {
        let error = require_operator_gates(None, None).expect_err("bootstrap must be held");
        assert!(error.contains("MDX_BETA_AUTH_BOOTSTRAP_ENABLED=1"));
        let error = require_operator_gates(Some("1"), None)
            .expect_err("bootstrap must require held writes");
        assert!(error.contains("MDX_BETA_AUTH_BOOTSTRAP_MAINTENANCE_ACK=1"));
        assert_eq!(require_operator_gates(Some("1"), Some("1")), Ok(()));
    }
}
