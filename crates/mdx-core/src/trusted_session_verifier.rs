//! Trusted-session verifier contract.
//!
//! The production access boundary (`production_auth_boundary`) admits a governed
//! write only from a [`TrustedSession`]. This module is the contract for where
//! that session comes from: a verifier turns request token material into a
//! verified [`TrustedSession`], or an explicit, fail-closed refusal. It never
//! trusts the request body or a spoofable header.
//!
//! The shared structural admission ([`admit_verified_claims`]) runs after a
//! concrete verifier parses and signature-checks a token: issuer, audience,
//! expiry, tenant mapping, subject mapping, actor kind, and - for an agent - full
//! delegation-lease validation through the delegation runtime (separate agent and
//! subject identities, scope within the sponsor, valid expiry, not revoked, no
//! self-sponsorship). The concrete verifiers are layered on top:
//!
//! - a local-secure signed test-token verifier (no real IdP credentials), and
//! - a production OIDC/JWT verifier configured by an auth profile.
//!
//! Nothing here flips a live substrate to ready: `production_write_allowed` stays
//! false. A mode with no configured verifier fails closed.

use crate::agent_delegation_runtime::{
    ActorKind, DelegationLease, SponsorAuthority, admit_agent_delegation,
};
use crate::production_auth_boundary::TrustedSession;

/// The configured authentication profile a verifier checks a token against. A
/// production node configures this from its IdP; an unconfigured profile is a
/// fail-closed refusal, never an open door.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuthProfile {
    pub configured: bool,
    pub issuer: String,
    pub audience: String,
}

impl AuthProfile {
    pub fn new(issuer: &str, audience: &str) -> Self {
        Self {
            configured: true,
            issuer: issuer.to_string(),
            audience: audience.to_string(),
        }
    }
}

/// The claims a verifier extracts from a token before mapping to a
/// [`TrustedSession`]. A concrete verifier parses these from its token format
/// (signed test token, or OIDC/JWT) and hands them to [`admit_verified_claims`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VerifiedClaims {
    pub issuer: String,
    pub audience: String,
    /// RFC 3339 UTC (`...Z`); compared lexicographically like the delegation runtime.
    pub expires_at: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub actor_role: String,
    pub actor_kind: String,
    pub subject_actor_id: String,
    pub delegation_id: Option<String>,
    pub authority_scope: Vec<String>,
    /// Agent tokens only: the sponsoring principal's own authority, the ceiling an
    /// agent's delegated scope cannot exceed. Attested by the control plane / IdP.
    pub sponsor_scope: Vec<String>,
    /// Agent tokens only: when the delegation lease was issued (RFC 3339 UTC).
    pub issued_at: String,
    /// Agent tokens only: the policy decision that authorized the delegation.
    pub policy_decision_id: String,
    /// Agent tokens only: whether the delegation has been revoked.
    pub revoked: bool,
}

/// Every way a verifier can refuse, fail-closed. There is no "accept on doubt".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VerifierRefusal {
    MissingToken,
    MalformedToken,
    UnconfiguredAuthProfile,
    IssuerMismatch,
    AudienceMismatch,
    ExpiredToken,
    TenantMappingMissing,
    ActorMappingMissing,
    RoleMappingMissing,
    SubjectMappingMissing,
    ActorKindUnsupported,
    DelegationMissingForAgent,
    /// The verifier could not obtain JWKS material to verify the token's
    /// signature (the configured JWKS endpoint did not return a usable key set).
    /// Production fails closed: with no key material, no token is trusted.
    JwksFetchFailed,
    /// The token's `kid` names a signing key that is not in the JWKS, even after a
    /// rotation refresh. The key is unknown (rotated out, or never issued by this
    /// IdP), so the token is not trusted.
    SigningKeyUnknown,
    /// An agent token's delegation lease was rejected by the delegation runtime.
    /// The wrapped code is the specific `DelegationError` code (for example
    /// `delegation_scope_exceeds_sponsor`, `delegation_expired`,
    /// `delegation_revoked`, or `delegation_self_sponsorship`).
    AgentDelegationRejected(&'static str),
}

impl VerifierRefusal {
    pub fn code(&self) -> &'static str {
        match self {
            Self::MissingToken => "verifier_missing_token",
            Self::MalformedToken => "verifier_malformed_token",
            Self::UnconfiguredAuthProfile => "verifier_unconfigured_auth_profile",
            Self::IssuerMismatch => "verifier_issuer_mismatch",
            Self::AudienceMismatch => "verifier_audience_mismatch",
            Self::ExpiredToken => "verifier_expired_token",
            Self::TenantMappingMissing => "verifier_tenant_mapping_missing",
            Self::ActorMappingMissing => "verifier_actor_mapping_missing",
            Self::RoleMappingMissing => "verifier_role_mapping_missing",
            Self::SubjectMappingMissing => "verifier_subject_mapping_missing",
            Self::ActorKindUnsupported => "verifier_actor_kind_unsupported",
            Self::DelegationMissingForAgent => "verifier_delegation_missing_for_agent",
            Self::JwksFetchFailed => "verifier_jwks_fetch_failed",
            Self::SigningKeyUnknown => "verifier_signing_key_unknown",
            Self::AgentDelegationRejected(code) => code,
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            Self::MissingToken => "no trusted-session token was presented",
            Self::MalformedToken => "the trusted-session token could not be parsed",
            Self::UnconfiguredAuthProfile => {
                "no auth profile is configured to verify the token against"
            }
            Self::IssuerMismatch => "the token issuer does not match the configured profile",
            Self::AudienceMismatch => "the token audience does not match the configured profile",
            Self::ExpiredToken => "the token has expired",
            Self::TenantMappingMissing => "the token has no tenant mapping",
            Self::ActorMappingMissing => "the token has no actor mapping",
            Self::RoleMappingMissing => "the token has no role mapping",
            Self::SubjectMappingMissing => "the token has no subject mapping",
            Self::ActorKindUnsupported => "the token actor kind is not supported",
            Self::DelegationMissingForAgent => {
                "an agent token must carry delegation claims and cannot act as a human session"
            }
            Self::JwksFetchFailed => {
                "the verifier could not obtain JWKS material to check the signature"
            }
            Self::SigningKeyUnknown => {
                "the token's signing key is not in the JWKS, even after a rotation refresh"
            }
            Self::AgentDelegationRejected(_) => "the agent token's delegation lease was rejected",
        }
    }
}

/// Every refusal case, declared for the contract check and the docs.
pub const VERIFIER_REFUSAL_CASES: &[&str] = &[
    "verifier_missing_token",
    "verifier_malformed_token",
    "verifier_unconfigured_auth_profile",
    "verifier_issuer_mismatch",
    "verifier_audience_mismatch",
    "verifier_expired_token",
    "verifier_tenant_mapping_missing",
    "verifier_actor_mapping_missing",
    "verifier_role_mapping_missing",
    "verifier_subject_mapping_missing",
    "verifier_actor_kind_unsupported",
    "verifier_delegation_missing_for_agent",
    "verifier_jwks_fetch_failed",
    "verifier_signing_key_unknown",
];

/// A verifier turns request token material into a verified [`TrustedSession`] or
/// an explicit refusal. The deployment mode and the configured auth profile are
/// baked into the concrete verifier; the request supplies only the token and the
/// current time.
pub trait TrustedSessionVerifier {
    fn verify(&self, token: Option<&str>, now: &str) -> Result<TrustedSession, VerifierRefusal>;
}

/// Reject a missing token or an unconfigured profile before any parse. A verifier
/// calls this first so the cheap, fail-closed refusals happen before crypto.
pub fn precheck_token_presence<'a>(
    profile: &AuthProfile,
    token: Option<&'a str>,
) -> Result<&'a str, VerifierRefusal> {
    if !profile.configured {
        return Err(VerifierRefusal::UnconfiguredAuthProfile);
    }
    match token {
        Some(token) if !token.trim().is_empty() => Ok(token),
        _ => Err(VerifierRefusal::MissingToken),
    }
}

/// The shared structural admission every concrete verifier runs after parsing a
/// token into [`VerifiedClaims`]. It checks issuer, audience, expiry, tenant and
/// subject mappings, the actor kind, and the presence of delegation claims for an
/// agent, then maps to a [`TrustedSession`]. It does not check the signature -
/// that is the concrete verifier's job before calling this.
pub fn admit_verified_claims(
    profile: &AuthProfile,
    claims: &VerifiedClaims,
    now: &str,
) -> Result<TrustedSession, VerifierRefusal> {
    if !profile.configured {
        return Err(VerifierRefusal::UnconfiguredAuthProfile);
    }
    if claims.issuer.trim().is_empty() || claims.issuer != profile.issuer {
        return Err(VerifierRefusal::IssuerMismatch);
    }
    if claims.audience.trim().is_empty() || claims.audience != profile.audience {
        return Err(VerifierRefusal::AudienceMismatch);
    }
    if claims.expires_at.trim().is_empty() || now >= claims.expires_at.as_str() {
        return Err(VerifierRefusal::ExpiredToken);
    }
    if claims.tenant_id.trim().is_empty() {
        return Err(VerifierRefusal::TenantMappingMissing);
    }
    if claims.actor_id.trim().is_empty() {
        return Err(VerifierRefusal::ActorMappingMissing);
    }
    if claims.actor_role.trim().is_empty() {
        return Err(VerifierRefusal::RoleMappingMissing);
    }
    if claims.subject_actor_id.trim().is_empty() {
        return Err(VerifierRefusal::SubjectMappingMissing);
    }
    let kind = ActorKind::parse(&claims.actor_kind).ok_or(VerifierRefusal::ActorKindUnsupported)?;
    if kind.requires_delegation() {
        // An agent acts only through a delegation lease, validated by the
        // delegation runtime: separate agent and subject identities, scope within
        // the sponsor's authority, a valid expiry, and not revoked. The subject is
        // the sponsoring principal; an agent that names itself as its own subject
        // is rejected as self-sponsorship and cannot act as a human session.
        let delegation_id = claims
            .delegation_id
            .as_deref()
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .ok_or(VerifierRefusal::DelegationMissingForAgent)?;
        admit_agent_token_delegation(claims, delegation_id, now)?;
    }
    Ok(TrustedSession {
        tenant_id: claims.tenant_id.clone(),
        actor_id: claims.actor_id.clone(),
        actor_role: claims.actor_role.clone(),
        actor_kind: kind.as_str().to_string(),
        subject_actor_id: claims.subject_actor_id.clone(),
        authority_scope: claims.authority_scope.clone(),
        // An agent carries its delegation lease id; a human carries none.
        delegation_id: if kind.requires_delegation() {
            claims.delegation_id.clone()
        } else {
            None
        },
    })
}

/// Validate an agent token's delegation lease through the delegation runtime. The
/// sponsoring principal (the subject) and its attested authority come from the
/// token; the runtime enforces that the delegated scope is within the sponsor's,
/// the lease has not expired or been revoked, and the agent is not its own
/// sponsor.
fn admit_agent_token_delegation(
    claims: &VerifiedClaims,
    delegation_id: &str,
    now: &str,
) -> Result<(), VerifierRefusal> {
    let lease = DelegationLease {
        tenant_id: claims.tenant_id.clone(),
        agent_actor_id: claims.actor_id.clone(),
        subject_actor_id: claims.subject_actor_id.clone(),
        // The subject is the sponsoring principal.
        sponsor_actor_id: claims.subject_actor_id.clone(),
        delegation_id: delegation_id.to_string(),
        authority_scope: claims.authority_scope.clone(),
        resource_constraints: vec![],
        issued_at: claims.issued_at.clone(),
        expires_at: claims.expires_at.clone(),
        policy_decision_id: claims.policy_decision_id.clone(),
        upstream_control_plane_ref: None,
    };
    let sponsor = SponsorAuthority {
        tenant_id: claims.tenant_id.clone(),
        sponsor_actor_id: claims.subject_actor_id.clone(),
        scope: claims.sponsor_scope.clone(),
    };
    admit_agent_delegation(&lease, &sponsor, now, claims.revoked)
        .map(|_| ())
        .map_err(|error| VerifierRefusal::AgentDelegationRejected(error.code()))
}

/// Parse the reference claim-token format: `key=value` pairs joined by `;`. This
/// is the deterministic token body the layered verifiers carry inside their
/// signed envelope; a structurally broken body is a [`VerifierRefusal::MalformedToken`].
/// Unknown keys are ignored; `delegation_id` and `authority_scope` are optional.
pub fn parse_claim_token(token: &str) -> Result<VerifiedClaims, VerifierRefusal> {
    let mut claims = VerifiedClaims::default();
    let mut saw_any = false;
    for part in token.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (key, value) = part
            .split_once('=')
            .ok_or(VerifierRefusal::MalformedToken)?;
        saw_any = true;
        let value = value.trim().to_string();
        match key.trim() {
            "iss" => claims.issuer = value,
            "aud" => claims.audience = value,
            "exp" => claims.expires_at = value,
            "tenant" => claims.tenant_id = value,
            "actor" => claims.actor_id = value,
            "role" => claims.actor_role = value,
            "kind" => claims.actor_kind = value,
            "subject" => claims.subject_actor_id = value,
            "delegation_id" => {
                claims.delegation_id = if value.is_empty() { None } else { Some(value) }
            }
            "scope" => {
                claims.authority_scope = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
            _ => {}
        }
    }
    if !saw_any {
        return Err(VerifierRefusal::MalformedToken);
    }
    Ok(claims)
}
