//! Production auth boundary.
//!
//! In `local-secure` and `production` deployment modes, a governed write derives
//! its identity from a trusted session established by an authentication boundary
//! (an enterprise IdP, OIDC/JWT, or an upstream control plane). The request body
//! and the client-supplied actor-admission fields are never authority in those
//! modes - they are only checked for conflict with the trusted session and
//! otherwise ignored.
//!
//! `local-demo` preserves the explicit local fixture path so a downloaded copy
//! stays frictionless to run. Nothing here flips a live substrate or production
//! profile to "ready": `production_write_allowed` stays false because readiness
//! requires observed evidence recorded elsewhere, not a mode string.
//!
//! Token verification itself (signature, issuer, audience, expiry) is the
//! boundary's responsibility and produces the [`TrustedSession`] fed in here.
//! The presence of a `TrustedSession` IS the proof that the boundary
//! authenticated the request; this module enforces what happens next.

/// How identity is trusted for governed writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeploymentMode {
    /// Frictionless local exploration: explicit local fixtures are identity.
    LocalDemo,
    /// Production-equivalent security on a laptop, using a signed test identity.
    LocalSecure,
    /// Enterprise deployment: identity comes only from a trusted boundary.
    Production,
}

impl DeploymentMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LocalDemo => "local-demo",
            Self::LocalSecure => "local-secure",
            Self::Production => "production",
        }
    }

    /// Parse a deployment-mode string. An empty/unset value defaults to
    /// `local-demo` so a fresh download is easy; an unknown value is rejected
    /// rather than silently downgraded to demo.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "" | "local-demo" | "local_demo" => Some(Self::LocalDemo),
            "local-secure" | "local_secure" => Some(Self::LocalSecure),
            "production" => Some(Self::Production),
            _ => None,
        }
    }

    /// Read the deployment mode from `MDX_DEPLOYMENT_MODE`. Unset is
    /// `local-demo`; an unparseable value is an error the caller must surface
    /// (production must never silently fall back to demo).
    pub fn from_env() -> Result<Self, ProductionAuthError> {
        let raw = std::env::var("MDX_DEPLOYMENT_MODE").unwrap_or_default();
        Self::parse(&raw).ok_or(ProductionAuthError::UnknownDeploymentMode)
    }

    /// True when identity must come from a trusted session, not the request.
    pub fn requires_trusted_session(&self) -> bool {
        matches!(self, Self::LocalSecure | Self::Production)
    }

    pub fn is_production(&self) -> bool {
        matches!(self, Self::Production)
    }
}

/// Identity established by the authentication boundary. For human requests
/// `actor_id` and `subject_actor_id` are the same; for agent requests
/// `actor_id` is the agent and `subject_actor_id` is the sponsoring principal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrustedSession {
    pub tenant_id: String,
    pub actor_id: String,
    pub actor_role: String,
    pub actor_kind: String,
    pub subject_actor_id: String,
    /// Route/action scopes verified from the trusted session token.
    pub authority_scope: Vec<String>,
    /// The delegation lease id for an agent session; None for a human session.
    pub delegation_id: Option<String>,
}

/// The identity metadata a governed-write receipt records: where the identity
/// came from, and the verified actor kind, subject, and delegation. For a
/// verified session it is built from the [`AdmittedIdentity`]; for local-demo it
/// is the fixture. Tenant, actor, and role flow through the existing request
/// fields; this carries the rest so a consequential action preserves who acted
/// and on whose behalf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GovernedWriteIdentity {
    pub identity_source: String,
    pub actor_kind: String,
    pub subject_actor_id: String,
    pub delegation_id: String,
    pub authority_scope: Vec<String>,
}

impl GovernedWriteIdentity {
    /// The local-demo fixture identity: a human acting for itself, no delegation.
    pub fn local_demo(actor_id: &str) -> Self {
        Self {
            identity_source: "local_demo_fixture".to_string(),
            actor_kind: "human".to_string(),
            subject_actor_id: actor_id.to_string(),
            delegation_id: String::new(),
            authority_scope: vec!["local-demo:*".to_string()],
        }
    }

    /// The verified identity from an admitted trusted session.
    pub fn from_admitted(admitted: &AdmittedIdentity) -> Self {
        Self {
            identity_source: admitted.identity_source.to_string(),
            actor_kind: admitted.actor_kind.clone(),
            subject_actor_id: admitted.subject_actor_id.clone(),
            delegation_id: admitted.delegation_id.clone().unwrap_or_default(),
            authority_scope: admitted.authority_scope.clone(),
        }
    }
}

/// What the request body / client-supplied admission claimed. In a mode that
/// requires a trusted session these carry no authority - they are only checked
/// against the trusted session and otherwise ignored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClientClaimedIdentity {
    pub tenant_id: String,
    pub actor_id: String,
    pub actor_role: String,
}

/// The admitted identity a governed write proceeds with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedIdentity {
    pub deployment_mode: &'static str,
    pub tenant_id: String,
    pub actor_id: String,
    pub actor_role: String,
    pub actor_kind: String,
    pub subject_actor_id: String,
    /// Route/action scopes carried from the trusted session token.
    pub authority_scope: Vec<String>,
    /// The delegation lease id for an agent session; None for a human session.
    pub delegation_id: Option<String>,
    /// Where identity came from: `trusted_session` or `local_demo_fixture`.
    pub identity_source: &'static str,
    /// Always false: a mode string does not make a live substrate ready.
    /// Production readiness is gated on observed evidence recorded elsewhere.
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProductionAuthError {
    UnknownDeploymentMode,
    MissingTrustedSession,
    TrustedSessionMissingField(&'static str),
    BodySuppliedTenantAuthority,
    BodySuppliedActorAuthority,
    BodySuppliedRoleAuthority,
    MissingAuthProfile,
}

impl ProductionAuthError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownDeploymentMode => "unknown_deployment_mode",
            Self::MissingTrustedSession => "production_missing_trusted_session",
            Self::TrustedSessionMissingField(_) => "production_trusted_session_incomplete",
            Self::BodySuppliedTenantAuthority => "production_body_supplied_tenant",
            Self::BodySuppliedActorAuthority => "production_body_supplied_actor",
            Self::BodySuppliedRoleAuthority => "production_body_supplied_role",
            Self::MissingAuthProfile => "production_missing_auth_profile",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::UnknownDeploymentMode => {
                "deployment mode is not one of local-demo, local-secure, production".to_string()
            }
            Self::MissingTrustedSession => {
                "governed write requires a trusted session in this deployment mode".to_string()
            }
            Self::TrustedSessionMissingField(field) => {
                format!("trusted session is missing {field}")
            }
            Self::BodySuppliedTenantAuthority => {
                "request-body tenant is not authority in this deployment mode".to_string()
            }
            Self::BodySuppliedActorAuthority => {
                "request-body actor is not authority in this deployment mode".to_string()
            }
            Self::BodySuppliedRoleAuthority => {
                "request-body role is not authority in this deployment mode".to_string()
            }
            Self::MissingAuthProfile => {
                "production refuses to start without a configured auth profile".to_string()
            }
        }
    }
}

/// The refusal cases this boundary adds on top of the local actor-admission
/// refusals. Declared here so the contract check and the generated descriptor
/// stay in lockstep with the code.
pub const PRODUCTION_AUTH_REFUSAL_CASES: &[&str] = &[
    "production_missing_trusted_session",
    "production_trusted_session_incomplete",
    "production_body_supplied_tenant",
    "production_body_supplied_actor",
    "production_body_supplied_role",
    "production_missing_auth_profile",
];

/// Boot gate: production refuses to start without a configured auth profile, so
/// an operator can never bring up a production node that authorizes from request
/// bodies or local fixtures. Non-production modes always boot.
pub fn production_boot_gate(
    mode: DeploymentMode,
    auth_profile_configured: bool,
) -> Result<(), ProductionAuthError> {
    if mode.is_production() && !auth_profile_configured {
        return Err(ProductionAuthError::MissingAuthProfile);
    }
    Ok(())
}

/// Admit a governed write's identity under the deployment mode.
///
/// - `local-demo`: the explicit local fixture is the identity (current behavior).
/// - `local-secure` / `production`: identity comes only from the trusted
///   session. A missing trusted session is refused. Any non-empty client-claimed
///   tenant/actor/role that disagrees with the trusted session is refused - the
///   request body can never escalate or switch identity.
pub fn admit_governed_write_identity(
    mode: DeploymentMode,
    trusted: Option<&TrustedSession>,
    claimed: &ClientClaimedIdentity,
) -> Result<AdmittedIdentity, ProductionAuthError> {
    if mode.requires_trusted_session() {
        let session = trusted.ok_or(ProductionAuthError::MissingTrustedSession)?;
        for (field, value) in [
            ("tenant_id", &session.tenant_id),
            ("actor_id", &session.actor_id),
            ("actor_role", &session.actor_role),
            ("actor_kind", &session.actor_kind),
            ("subject_actor_id", &session.subject_actor_id),
        ] {
            if value.trim().is_empty() {
                return Err(ProductionAuthError::TrustedSessionMissingField(field));
            }
        }
        if !claimed.tenant_id.trim().is_empty() && claimed.tenant_id != session.tenant_id {
            return Err(ProductionAuthError::BodySuppliedTenantAuthority);
        }
        if !claimed.actor_id.trim().is_empty() && claimed.actor_id != session.actor_id {
            return Err(ProductionAuthError::BodySuppliedActorAuthority);
        }
        if !claimed.actor_role.trim().is_empty() && claimed.actor_role != session.actor_role {
            return Err(ProductionAuthError::BodySuppliedRoleAuthority);
        }
        Ok(AdmittedIdentity {
            deployment_mode: mode.as_str(),
            tenant_id: session.tenant_id.clone(),
            actor_id: session.actor_id.clone(),
            actor_role: session.actor_role.clone(),
            actor_kind: session.actor_kind.clone(),
            subject_actor_id: session.subject_actor_id.clone(),
            authority_scope: session.authority_scope.clone(),
            delegation_id: session.delegation_id.clone(),
            identity_source: "trusted_session",
            production_write_allowed: false,
        })
    } else {
        Ok(AdmittedIdentity {
            deployment_mode: mode.as_str(),
            tenant_id: claimed.tenant_id.clone(),
            actor_id: claimed.actor_id.clone(),
            actor_role: claimed.actor_role.clone(),
            actor_kind: "human".to_string(),
            subject_actor_id: claimed.actor_id.clone(),
            authority_scope: vec!["local-demo:*".to_string()],
            delegation_id: None,
            identity_source: "local_demo_fixture",
            production_write_allowed: false,
        })
    }
}
