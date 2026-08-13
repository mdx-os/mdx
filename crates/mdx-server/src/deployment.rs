//! Startup deployment gate.
//!
//! The server resolves its deployment mode from `MDX_DEPLOYMENT_MODE` and runs
//! the production boot gate before it listens. A `production` node refuses to
//! start without a configured auth profile, so it can never come up authorizing
//! governed writes from request bodies or local fixtures. Local modes always
//! boot. See `docs/AUTH-PRODUCTION-BOUNDARY.md`.
//!
//! Phase 5 adds the full deployment-profile gate on top of the auth boot gate:
//! a production node also refuses to start when database, storage, secrets,
//! telemetry, or migration posture is missing, or when a wildcard CORS origin,
//! local fixtures, or a request-path service role is enabled. The posture is read
//! from the documented environment contract in `docs/RUNTIME-TURN-ON-BLOCKERS.md`.

use mdx_core::{
    DeploymentMode, ProductionAuthError, StartupPosture, evaluate_startup_profile,
    production_boot_gate,
};

/// Whether a production auth profile is configured, from the environment.
pub fn auth_profile_configured() -> bool {
    std::env::var("MDX_AUTH_PROFILE")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

/// True when an environment variable is present and non-empty.
fn env_present(key: &str) -> bool {
    std::env::var(key)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

/// True when an environment variable is set to a truthy value.
fn env_flag(key: &str) -> bool {
    matches!(
        std::env::var(key).unwrap_or_default().trim(),
        "1" | "true" | "yes" | "on"
    )
}

/// Read the observed startup posture from the documented environment contract.
/// See `docs/RUNTIME-TURN-ON-BLOCKERS.md`.
pub fn startup_posture_from_env() -> StartupPosture {
    StartupPosture {
        auth_profile_configured: auth_profile_configured(),
        database_configured: env_present("MDX_DATABASE_URL"),
        storage_configured: crate::twin_artifact_blob_store::production_configuration_ready(),
        secrets_backend_configured: env_present("MDX_SECRETS_BACKEND"),
        telemetry_configured: env_present("MDX_TELEMETRY_ENDPOINT"),
        migrations_current: env_flag("MDX_MIGRATIONS_CURRENT"),
        cors_wildcard: std::env::var("MDX_CORS_ALLOW_ORIGINS")
            .map(|value| value.trim() == "*")
            .unwrap_or(false),
        local_fixtures_enabled: env_flag("MDX_LOCAL_FIXTURES"),
        service_role_request_path_enabled: env_flag("MDX_SERVICE_ROLE_REQUEST_PATH"),
    }
}

/// Run the Phase 5 deployment-profile gate. Fail-closed: a production node that
/// is missing required posture or carrying an unsafe shortcut refuses to start,
/// reporting every blocker at once. Local modes pass through their own posture.
pub fn startup_profile_gate(mode: DeploymentMode, posture: StartupPosture) -> Result<(), String> {
    evaluate_startup_profile(mode, posture)
        .map(|_| ())
        .map_err(|refusals| {
            let detail = refusals
                .iter()
                .map(|refusal| refusal.message())
                .collect::<Vec<_>>()
                .join("; ");
            format!("insecure {} startup refused: {detail}", mode.as_str())
        })
}

/// Resolve the deployment mode and run the production boot gate. Returns the
/// resolved mode for logging, or an error message that must stop startup.
pub fn startup_deployment_gate(
    mode_raw: &str,
    auth_profile_configured: bool,
) -> Result<DeploymentMode, String> {
    let mode = DeploymentMode::parse(mode_raw)
        .ok_or_else(|| ProductionAuthError::UnknownDeploymentMode.message())?;
    production_boot_gate(mode, auth_profile_configured).map_err(|error| error.message())?;
    Ok(mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_mode_defaults_to_local_demo_and_boots() {
        let mode = startup_deployment_gate("", false).expect("local demo boots");
        assert_eq!(mode, DeploymentMode::LocalDemo);
    }

    #[test]
    fn local_secure_boots_without_an_auth_profile() {
        let mode = startup_deployment_gate("local-secure", false).expect("local secure boots");
        assert_eq!(mode, DeploymentMode::LocalSecure);
    }

    #[test]
    fn production_refuses_to_boot_without_an_auth_profile() {
        let error = startup_deployment_gate("production", false)
            .expect_err("production must refuse to boot without an auth profile");
        assert!(error.contains("auth profile"), "unexpected error: {error}");
    }

    #[test]
    fn production_boots_with_an_auth_profile() {
        let mode =
            startup_deployment_gate("production", true).expect("production boots with profile");
        assert_eq!(mode, DeploymentMode::Production);
    }

    #[test]
    fn an_unknown_mode_stops_startup() {
        let error = startup_deployment_gate("staging", true)
            .expect_err("an unknown mode must stop startup");
        assert!(
            error.contains("deployment mode"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn profile_gate_lets_local_demo_through() {
        startup_profile_gate(DeploymentMode::LocalDemo, StartupPosture::local_demo())
            .expect("local demo passes the profile gate");
    }

    #[test]
    fn profile_gate_refuses_an_insecure_production_start() {
        let error = startup_profile_gate(DeploymentMode::Production, StartupPosture::local_demo())
            .expect_err("insecure production must be refused");
        assert!(error.contains("insecure production startup refused"));
        assert!(error.contains("auth profile"), "unexpected error: {error}");
        assert!(error.contains("wildcard CORS"), "unexpected error: {error}");
    }

    #[test]
    fn profile_gate_admits_a_fully_configured_production_start() {
        startup_profile_gate(
            DeploymentMode::Production,
            StartupPosture::secure_production(),
        )
        .expect("a fully configured production posture passes the profile gate");
    }
}
