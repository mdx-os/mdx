use super::*;

fn trusted() -> TrustedSession {
    TrustedSession {
        tenant_id: "acme".to_string(),
        actor_id: "user_alice".to_string(),
        actor_role: "member".to_string(),
        actor_kind: "human".to_string(),
        subject_actor_id: "user_alice".to_string(),
        authority_scope: vec![],
        delegation_id: None,
    }
}

fn claimed(tenant: &str, actor: &str, role: &str) -> ClientClaimedIdentity {
    ClientClaimedIdentity {
        tenant_id: tenant.to_string(),
        actor_id: actor.to_string(),
        actor_role: role.to_string(),
    }
}

#[test]
fn deployment_mode_parses_and_defaults_to_local_demo() {
    assert_eq!(DeploymentMode::parse(""), Some(DeploymentMode::LocalDemo));
    assert_eq!(
        DeploymentMode::parse("local-demo"),
        Some(DeploymentMode::LocalDemo)
    );
    assert_eq!(
        DeploymentMode::parse("local-secure"),
        Some(DeploymentMode::LocalSecure)
    );
    assert_eq!(
        DeploymentMode::parse("production"),
        Some(DeploymentMode::Production)
    );
    assert_eq!(DeploymentMode::parse("staging"), None);
}

#[test]
fn only_secure_and_production_require_a_trusted_session() {
    assert!(!DeploymentMode::LocalDemo.requires_trusted_session());
    assert!(DeploymentMode::LocalSecure.requires_trusted_session());
    assert!(DeploymentMode::Production.requires_trusted_session());
}

#[test]
fn local_demo_admits_the_explicit_fixture_identity() {
    let admitted = admit_governed_write_identity(
        DeploymentMode::LocalDemo,
        None,
        &claimed("local_tenant", "local_user", "owner"),
    )
    .expect("local demo admits the fixture");
    assert_eq!(admitted.identity_source, "local_demo_fixture");
    assert_eq!(admitted.tenant_id, "local_tenant");
    assert_eq!(admitted.actor_id, "local_user");
    assert_eq!(admitted.actor_role, "owner");
    assert!(!admitted.production_write_allowed);
}

#[test]
fn secure_and_production_reject_missing_trusted_session() {
    for mode in [DeploymentMode::LocalSecure, DeploymentMode::Production] {
        let err =
            admit_governed_write_identity(mode, None, &claimed("acme", "user_alice", "member"))
                .expect_err("missing trusted session must be refused");
        assert_eq!(err, ProductionAuthError::MissingTrustedSession);
    }
}

#[test]
fn production_takes_identity_from_the_trusted_session_not_the_body() {
    let session = trusted();
    // Body claims nothing - identity comes wholly from the trusted session.
    let admitted = admit_governed_write_identity(
        DeploymentMode::Production,
        Some(&session),
        &ClientClaimedIdentity::default(),
    )
    .expect("trusted session is admitted");
    assert_eq!(admitted.identity_source, "trusted_session");
    assert_eq!(admitted.tenant_id, "acme");
    assert_eq!(admitted.actor_id, "user_alice");
    assert_eq!(admitted.actor_role, "member");
    assert_eq!(admitted.subject_actor_id, "user_alice");
    // A mode string never flips live readiness.
    assert!(!admitted.production_write_allowed);
}

#[test]
fn production_refuses_body_supplied_tenant_actor_or_role() {
    let session = trusted();
    let tenant_switch = admit_governed_write_identity(
        DeploymentMode::Production,
        Some(&session),
        &claimed("evil_tenant", "", ""),
    )
    .expect_err("body tenant is refused");
    assert_eq!(
        tenant_switch,
        ProductionAuthError::BodySuppliedTenantAuthority
    );

    let actor_switch = admit_governed_write_identity(
        DeploymentMode::Production,
        Some(&session),
        &claimed("", "user_bob", ""),
    )
    .expect_err("body actor is refused");
    assert_eq!(
        actor_switch,
        ProductionAuthError::BodySuppliedActorAuthority
    );

    let role_escalation = admit_governed_write_identity(
        DeploymentMode::Production,
        Some(&session),
        &claimed("", "", "tenant_admin"),
    )
    .expect_err("body role is refused");
    assert_eq!(
        role_escalation,
        ProductionAuthError::BodySuppliedRoleAuthority
    );
}

#[test]
fn production_allows_body_fields_that_echo_the_trusted_session() {
    let session = trusted();
    let admitted = admit_governed_write_identity(
        DeploymentMode::Production,
        Some(&session),
        &claimed("acme", "user_alice", "member"),
    )
    .expect("echoing the trusted session is allowed");
    assert_eq!(admitted.tenant_id, "acme");
    assert_eq!(admitted.identity_source, "trusted_session");
}

#[test]
fn production_refuses_an_incomplete_trusted_session() {
    let mut session = trusted();
    session.subject_actor_id = String::new();
    let err = admit_governed_write_identity(
        DeploymentMode::Production,
        Some(&session),
        &ClientClaimedIdentity::default(),
    )
    .expect_err("incomplete trusted session is refused");
    assert_eq!(
        err,
        ProductionAuthError::TrustedSessionMissingField("subject_actor_id")
    );
}

#[test]
fn production_boot_gate_requires_an_auth_profile() {
    assert_eq!(
        production_boot_gate(DeploymentMode::Production, false),
        Err(ProductionAuthError::MissingAuthProfile)
    );
    assert!(production_boot_gate(DeploymentMode::Production, true).is_ok());
    // Local modes boot without an auth profile.
    assert!(production_boot_gate(DeploymentMode::LocalDemo, false).is_ok());
    assert!(production_boot_gate(DeploymentMode::LocalSecure, false).is_ok());
}

#[test]
fn refusal_cases_are_declared_for_the_contract_check() {
    for expected in [
        "production_missing_trusted_session",
        "production_body_supplied_tenant",
        "production_body_supplied_actor",
        "production_body_supplied_role",
        "production_missing_auth_profile",
    ] {
        assert!(
            PRODUCTION_AUTH_REFUSAL_CASES.contains(&expected),
            "missing refusal case {expected}"
        );
    }
}
