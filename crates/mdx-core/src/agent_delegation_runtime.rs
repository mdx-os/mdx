//! Agent delegation runtime.
//!
//! Agents, services, and control planes are first-class principals - they act
//! through scoped, expiring delegation leases, never by borrowing a human
//! session. An agent action carries a separate `actor_id` (the agent) and
//! `subject_actor_id` (the sponsoring principal), and its authority is the
//! intersection of the sponsor's authority and the lease scope: it can never
//! exceed the sponsor, approve its own authority, or outlive its lease.
//!
//! See `docs/AGENT-IDENTITY-DELEGATION-CONTRACT.md`. This module is the local
//! enforcement; an upstream control plane may attest identity and delegation,
//! but MDx still authorizes here.

/// The kind of principal behind a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorKind {
    Human,
    Agent,
    Service,
    ControlPlane,
}

impl ActorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent => "agent",
            Self::Service => "service",
            Self::ControlPlane => "control_plane",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "human" => Some(Self::Human),
            "agent" => Some(Self::Agent),
            "service" => Some(Self::Service),
            "control_plane" => Some(Self::ControlPlane),
            _ => None,
        }
    }

    /// True when the principal must act through a delegation lease rather than a
    /// session of its own.
    pub fn requires_delegation(&self) -> bool {
        matches!(self, Self::Agent)
    }
}

/// A scoped, expiring grant from a sponsoring principal to an agent.
///
/// Timestamps are RFC 3339 in UTC (`...Z`); expiry is a lexicographic compare,
/// which is correct for that fixed-width zulu form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegationLease {
    pub tenant_id: String,
    pub agent_actor_id: String,
    pub subject_actor_id: String,
    pub sponsor_actor_id: String,
    pub delegation_id: String,
    pub authority_scope: Vec<String>,
    pub resource_constraints: Vec<String>,
    pub issued_at: String,
    pub expires_at: String,
    pub policy_decision_id: String,
    pub upstream_control_plane_ref: Option<String>,
}

/// The sponsor's own authority, the ceiling an agent lease cannot exceed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SponsorAuthority {
    pub tenant_id: String,
    pub sponsor_actor_id: String,
    pub scope: Vec<String>,
}

/// The admitted agent identity, with the delegation chain recorded for receipts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedDelegation {
    pub actor_kind: &'static str,
    pub tenant_id: String,
    /// The agent itself.
    pub actor_id: String,
    /// The sponsoring principal the agent acts on behalf of.
    pub subject_actor_id: String,
    pub sponsor_actor_id: String,
    pub delegation_id: String,
    pub authority_scope: Vec<String>,
    pub upstream_control_plane_ref: Option<String>,
    /// Always false: delegation never enables a live production write.
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DelegationError {
    Missing(&'static str),
    EmptyScope,
    TenantMismatch,
    SponsorMismatch,
    SelfSponsorship,
    SubjectMustBeSponsor,
    ScopeExceedsSponsor(String),
    Expired,
    Revoked,
}

impl DelegationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Missing(_) => "delegation_missing_field",
            Self::EmptyScope => "delegation_empty_scope",
            Self::TenantMismatch => "delegation_tenant_mismatch",
            Self::SponsorMismatch => "delegation_sponsor_mismatch",
            Self::SelfSponsorship => "delegation_self_sponsorship",
            Self::SubjectMustBeSponsor => "delegation_subject_must_be_sponsor",
            Self::ScopeExceedsSponsor(_) => "delegation_scope_exceeds_sponsor",
            Self::Expired => "delegation_expired",
            Self::Revoked => "delegation_revoked",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("delegation lease is missing {field}"),
            Self::EmptyScope => "delegation lease grants no authority scope".to_string(),
            Self::TenantMismatch => {
                "delegation lease tenant does not match the sponsor".to_string()
            }
            Self::SponsorMismatch => {
                "delegation lease sponsor does not match the sponsor authority".to_string()
            }
            Self::SelfSponsorship => {
                "an agent cannot sponsor or approve its own authority".to_string()
            }
            Self::SubjectMustBeSponsor => {
                "delegation subject must be the sponsoring principal, not the agent".to_string()
            }
            Self::ScopeExceedsSponsor(scope) => {
                format!("delegation scope {scope} exceeds the sponsor's authority")
            }
            Self::Expired => "delegation lease has expired".to_string(),
            Self::Revoked => "delegation lease has been revoked".to_string(),
        }
    }
}

/// The delegation refusal cases, declared for the contract check.
pub const DELEGATION_REFUSAL_CASES: &[&str] = &[
    "delegation_missing_field",
    "delegation_empty_scope",
    "delegation_tenant_mismatch",
    "delegation_sponsor_mismatch",
    "delegation_self_sponsorship",
    "delegation_subject_must_be_sponsor",
    "delegation_scope_exceeds_sponsor",
    "delegation_expired",
    "delegation_revoked",
];

/// Admit an agent action under a delegation lease, against the sponsor's
/// authority and a clock. Returns the admitted identity with separate actor and
/// subject, and the delegation chain for the receipt.
///
/// `now` is an RFC 3339 UTC timestamp; `revoked` is whether a revocation record
/// exists for this delegation.
pub fn admit_agent_delegation(
    lease: &DelegationLease,
    sponsor: &SponsorAuthority,
    now: &str,
    revoked: bool,
) -> Result<AdmittedDelegation, DelegationError> {
    for (field, value) in [
        ("tenant_id", lease.tenant_id.as_str()),
        ("agent_actor_id", lease.agent_actor_id.as_str()),
        ("subject_actor_id", lease.subject_actor_id.as_str()),
        ("sponsor_actor_id", lease.sponsor_actor_id.as_str()),
        ("delegation_id", lease.delegation_id.as_str()),
        ("issued_at", lease.issued_at.as_str()),
        ("expires_at", lease.expires_at.as_str()),
        ("policy_decision_id", lease.policy_decision_id.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(DelegationError::Missing(field));
        }
    }
    if lease.authority_scope.is_empty()
        || lease
            .authority_scope
            .iter()
            .all(|scope| scope.trim().is_empty())
    {
        return Err(DelegationError::EmptyScope);
    }
    if lease.tenant_id != sponsor.tenant_id {
        return Err(DelegationError::TenantMismatch);
    }
    if lease.sponsor_actor_id != sponsor.sponsor_actor_id {
        return Err(DelegationError::SponsorMismatch);
    }
    // The agent may never be its own sponsor or subject - no self-approval and
    // no piggybacking on a human session by claiming to be the human.
    if lease.agent_actor_id == lease.sponsor_actor_id {
        return Err(DelegationError::SelfSponsorship);
    }
    if lease.subject_actor_id != lease.sponsor_actor_id {
        return Err(DelegationError::SubjectMustBeSponsor);
    }
    if revoked {
        return Err(DelegationError::Revoked);
    }
    if now >= lease.expires_at.as_str() {
        return Err(DelegationError::Expired);
    }
    // Agent authority is the intersection with the sponsor: every granted scope
    // must be within the sponsor's authority.
    for scope in &lease.authority_scope {
        if scope.trim().is_empty() {
            continue;
        }
        if !sponsor.scope.iter().any(|owned| owned == scope) {
            return Err(DelegationError::ScopeExceedsSponsor(scope.clone()));
        }
    }

    Ok(AdmittedDelegation {
        actor_kind: ActorKind::Agent.as_str(),
        tenant_id: lease.tenant_id.clone(),
        actor_id: lease.agent_actor_id.clone(),
        subject_actor_id: lease.subject_actor_id.clone(),
        sponsor_actor_id: lease.sponsor_actor_id.clone(),
        delegation_id: lease.delegation_id.clone(),
        authority_scope: lease
            .authority_scope
            .iter()
            .filter(|scope| !scope.trim().is_empty())
            .cloned()
            .collect(),
        upstream_control_plane_ref: lease.upstream_control_plane_ref.clone(),
        production_write_allowed: false,
    })
}
