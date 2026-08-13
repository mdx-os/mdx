use super::*;

#[test]
fn local_demo_always_boots_even_with_nothing_configured() {
    let readiness =
        evaluate_startup_profile(DeploymentMode::LocalDemo, StartupPosture::local_demo())
            .expect("local demo boots frictionlessly");
    assert_eq!(readiness.mode, "local-demo");
    assert!(!readiness.production_write_allowed);
}

#[test]
fn production_boots_only_with_the_full_secure_posture() {
    let readiness = evaluate_startup_profile(
        DeploymentMode::Production,
        StartupPosture::secure_production(),
    )
    .expect("a fully configured production posture boots");
    assert_eq!(readiness.mode, "production");
    // A satisfied posture never flips live readiness.
    assert!(!readiness.production_write_allowed);
}

#[test]
fn production_refuses_every_missing_or_unsafe_posture_at_once() {
    // Nothing configured, every shortcut on: the gate must surface all blockers.
    let refusals =
        evaluate_startup_profile(DeploymentMode::Production, StartupPosture::local_demo())
            .expect_err("production must refuse an insecure posture");
    let codes: Vec<&str> = refusals.iter().map(|r| r.code()).collect();
    for expected in [
        "production_missing_auth_profile",
        "production_missing_database",
        "production_missing_storage",
        "production_missing_secrets_backend",
        "production_missing_telemetry",
        "production_migrations_not_current",
        "production_wildcard_cors",
        "production_local_fixtures_enabled",
    ] {
        assert!(codes.contains(&expected), "missing refusal {expected}");
    }
}

#[test]
fn production_rejects_each_unsafe_shortcut_individually() {
    let mut posture = StartupPosture::secure_production();
    posture.cors_wildcard = true;
    let refusals =
        evaluate_startup_profile(DeploymentMode::Production, posture).expect_err("wildcard cors");
    assert!(refusals.contains(&ProfileRefusal::WildcardCors));

    let mut posture = StartupPosture::secure_production();
    posture.local_fixtures_enabled = true;
    let refusals =
        evaluate_startup_profile(DeploymentMode::Production, posture).expect_err("fixtures");
    assert!(refusals.contains(&ProfileRefusal::LocalFixturesEnabled));

    let mut posture = StartupPosture::secure_production();
    posture.service_role_request_path_enabled = true;
    let refusals =
        evaluate_startup_profile(DeploymentMode::Production, posture).expect_err("service role");
    assert!(refusals.contains(&ProfileRefusal::ServiceRoleRequestPath));
}

#[test]
fn local_secure_requires_real_auth_but_not_managed_cloud_backends() {
    // A laptop rehearsal: auth profile on, shortcuts off, no managed backends.
    let posture = StartupPosture {
        auth_profile_configured: true,
        database_configured: false,
        storage_configured: false,
        secrets_backend_configured: false,
        telemetry_configured: false,
        migrations_current: false,
        cors_wildcard: false,
        local_fixtures_enabled: false,
        service_role_request_path_enabled: false,
    };
    let readiness = evaluate_startup_profile(DeploymentMode::LocalSecure, posture)
        .expect("local secure boots with real auth and no managed backends");
    assert_eq!(readiness.mode, "local-secure");
}

#[test]
fn local_secure_refuses_the_insecure_shortcuts() {
    let mut posture = StartupPosture::secure_production();
    posture.auth_profile_configured = false;
    posture.local_fixtures_enabled = true;
    let refusals = evaluate_startup_profile(DeploymentMode::LocalSecure, posture)
        .expect_err("local secure refuses fixtures and missing auth");
    assert!(refusals.contains(&ProfileRefusal::MissingAuthProfile));
    assert!(refusals.contains(&ProfileRefusal::LocalFixturesEnabled));
    // Local-secure does not demand managed cloud backends.
    assert!(!refusals.contains(&ProfileRefusal::MissingStorage));
}

#[test]
fn three_profiles_and_all_refusal_cases_are_declared() {
    assert_eq!(
        DEPLOYMENT_PROFILES,
        &["local-demo", "local-secure", "production"]
    );
    for expected in [
        "production_missing_auth_profile",
        "production_missing_database",
        "production_missing_storage",
        "production_missing_secrets_backend",
        "production_missing_telemetry",
        "production_migrations_not_current",
        "production_wildcard_cors",
        "production_local_fixtures_enabled",
        "production_service_role_request_path",
    ] {
        assert!(
            PROFILE_REFUSAL_CASES.contains(&expected),
            "missing refusal case {expected}"
        );
    }
}
