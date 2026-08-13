//! Production deployment profile and fail-closed boot.
//!
//! Phase 1 gave us [`DeploymentMode`](crate::DeploymentMode) and a boot gate that
//! refuses a production start without an auth profile. Phase 5 widens that gate
//! from a single check into the full posture that makes an insecure production
//! start impossible: production refuses to boot when auth, database, storage,
//! secrets, telemetry, or migration posture is missing, and refuses to boot with
//! a wildcard CORS origin, local fixtures, or a request-path service role.
//!
//! Local stays easy. `local-demo` boots with nothing configured so a fresh
//! download runs in one command. `local-secure` is a faithful local rehearsal of
//! production auth: it requires a real auth profile and refuses the insecure
//! shortcuts (fixtures, wildcard CORS, request-path service role), but it does
//! not require managed cloud database, storage, secrets, or telemetry backends
//! because the laptop provides those locally.
//!
//! The gate collects every refusal rather than stopping at the first, so an
//! operator sees all blockers at once. Nothing here flips a live substrate to
//! ready: `production_write_allowed` stays false because readiness is observed
//! evidence, not a satisfied posture.

use crate::production_auth_boundary::DeploymentMode;

/// The observable security posture of the environment at boot. Each field is a
/// fact the operator's environment either satisfies or does not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartupPosture {
    /// An auth profile (IdP/OIDC/control-plane trust) is configured.
    pub auth_profile_configured: bool,
    /// A database connection is configured.
    pub database_configured: bool,
    /// A durable object/blob storage backend is configured.
    pub storage_configured: bool,
    /// A secrets backend (manager/vault) is configured, not inline values.
    pub secrets_backend_configured: bool,
    /// A telemetry/observability endpoint is configured.
    pub telemetry_configured: bool,
    /// Schema migrations are applied and current.
    pub migrations_current: bool,
    /// CORS is configured to allow any origin (`*`). Unsafe in production.
    pub cors_wildcard: bool,
    /// Local fixture identities are enabled. Unsafe in production.
    pub local_fixtures_enabled: bool,
    /// A service role can authorize requests in the request path. Unsafe in
    /// production: a service role is never request-path authority.
    pub service_role_request_path_enabled: bool,
}

impl StartupPosture {
    /// A fully configured, safe production posture. Useful as a test baseline and
    /// to document what production requires.
    pub fn secure_production() -> Self {
        Self {
            auth_profile_configured: true,
            database_configured: true,
            storage_configured: true,
            secrets_backend_configured: true,
            telemetry_configured: true,
            migrations_current: true,
            cors_wildcard: false,
            local_fixtures_enabled: false,
            service_role_request_path_enabled: false,
        }
    }

    /// The frictionless local posture: nothing configured, shortcuts allowed.
    pub fn local_demo() -> Self {
        Self {
            auth_profile_configured: false,
            database_configured: false,
            storage_configured: false,
            secrets_backend_configured: false,
            telemetry_configured: false,
            migrations_current: false,
            cors_wildcard: true,
            local_fixtures_enabled: true,
            service_role_request_path_enabled: false,
        }
    }
}

/// A single reason production (or local-secure) refused to start. Fail-closed:
/// the presence of any refusal stops the boot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileRefusal {
    MissingAuthProfile,
    MissingDatabase,
    MissingStorage,
    MissingSecretsBackend,
    MissingTelemetry,
    MigrationsNotCurrent,
    WildcardCors,
    LocalFixturesEnabled,
    ServiceRoleRequestPath,
}

impl ProfileRefusal {
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingAuthProfile => "production_missing_auth_profile",
            Self::MissingDatabase => "production_missing_database",
            Self::MissingStorage => "production_missing_storage",
            Self::MissingSecretsBackend => "production_missing_secrets_backend",
            Self::MissingTelemetry => "production_missing_telemetry",
            Self::MigrationsNotCurrent => "production_migrations_not_current",
            Self::WildcardCors => "production_wildcard_cors",
            Self::LocalFixturesEnabled => "production_local_fixtures_enabled",
            Self::ServiceRoleRequestPath => "production_service_role_request_path",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::MissingAuthProfile => {
                "production refuses to start without a configured auth profile"
            }
            Self::MissingDatabase => "production refuses to start without a configured database",
            Self::MissingStorage => {
                "production refuses to start without a configured storage backend"
            }
            Self::MissingSecretsBackend => {
                "production refuses to start without a configured secrets backend"
            }
            Self::MissingTelemetry => "production refuses to start without configured telemetry",
            Self::MigrationsNotCurrent => "production refuses to start with migrations not current",
            Self::WildcardCors => "production refuses to start with a wildcard CORS origin",
            Self::LocalFixturesEnabled => "production refuses to start with local fixtures enabled",
            Self::ServiceRoleRequestPath => {
                "production refuses to start with a request-path service role"
            }
        }
    }
}

/// Every refusal the profile gate can raise. Declared so the contract check and
/// the docs stay in lockstep with the code.
pub const PROFILE_REFUSAL_CASES: &[&str] = &[
    "production_missing_auth_profile",
    "production_missing_database",
    "production_missing_storage",
    "production_missing_secrets_backend",
    "production_missing_telemetry",
    "production_migrations_not_current",
    "production_wildcard_cors",
    "production_local_fixtures_enabled",
    "production_service_role_request_path",
];

/// The three formal deployment profiles. Names are stable identifiers.
pub const DEPLOYMENT_PROFILES: &[&str] = &["local-demo", "local-secure", "production"];

/// The admitted profile a node boots with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeploymentReadiness {
    pub mode: &'static str,
    /// Always false: a satisfied posture does not make a live substrate ready.
    pub production_write_allowed: bool,
}

/// Evaluate the startup profile for a deployment mode against the observed
/// posture. Returns the admitted readiness, or every refusal that must stop the
/// boot. Fail-closed: production requires the full posture; local-secure refuses
/// the insecure shortcuts; local-demo always boots.
pub fn evaluate_startup_profile(
    mode: DeploymentMode,
    posture: StartupPosture,
) -> Result<DeploymentReadiness, Vec<ProfileRefusal>> {
    let mut refusals = Vec::new();

    // The insecure shortcuts are refused in any mode that claims production-like
    // security (local-secure and production).
    if mode.requires_trusted_session() {
        if !posture.auth_profile_configured {
            refusals.push(ProfileRefusal::MissingAuthProfile);
        }
        if posture.cors_wildcard {
            refusals.push(ProfileRefusal::WildcardCors);
        }
        if posture.local_fixtures_enabled {
            refusals.push(ProfileRefusal::LocalFixturesEnabled);
        }
        if posture.service_role_request_path_enabled {
            refusals.push(ProfileRefusal::ServiceRoleRequestPath);
        }
    }

    // The managed-infrastructure posture is required only for production. A
    // local-secure laptop supplies database, storage, secrets, and telemetry
    // locally and is not asked for managed backends.
    if mode.is_production() {
        if !posture.database_configured {
            refusals.push(ProfileRefusal::MissingDatabase);
        }
        if !posture.storage_configured {
            refusals.push(ProfileRefusal::MissingStorage);
        }
        if !posture.secrets_backend_configured {
            refusals.push(ProfileRefusal::MissingSecretsBackend);
        }
        if !posture.telemetry_configured {
            refusals.push(ProfileRefusal::MissingTelemetry);
        }
        if !posture.migrations_current {
            refusals.push(ProfileRefusal::MigrationsNotCurrent);
        }
    }

    if refusals.is_empty() {
        Ok(DeploymentReadiness {
            mode: mode.as_str(),
            production_write_allowed: false,
        })
    } else {
        Err(refusals)
    }
}
