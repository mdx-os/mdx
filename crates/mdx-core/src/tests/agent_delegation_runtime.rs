use super::*;

fn sponsor() -> SponsorAuthority {
    SponsorAuthority {
        tenant_id: "acme".to_string(),
        sponsor_actor_id: "user_alice".to_string(),
        scope: vec![
            "forge:build:read".to_string(),
            "forge:build:update".to_string(),
            "pages:read".to_string(),
        ],
    }
}

fn lease() -> DelegationLease {
    DelegationLease {
        tenant_id: "acme".to_string(),
        agent_actor_id: "agent_forge_worker_abc".to_string(),
        subject_actor_id: "user_alice".to_string(),
        sponsor_actor_id: "user_alice".to_string(),
        delegation_id: "lease_456".to_string(),
        authority_scope: vec![
            "forge:build:read".to_string(),
            "forge:build:update".to_string(),
        ],
        resource_constraints: vec!["forge:build:build_789".to_string()],
        issued_at: "2026-01-01T00:00:00Z".to_string(),
        expires_at: "2026-12-31T23:59:59Z".to_string(),
        policy_decision_id: "policy_delegation_1".to_string(),
        upstream_control_plane_ref: None,
    }
}

const NOW: &str = "2026-06-01T00:00:00Z";

#[test]
fn actor_kind_parses_and_only_agents_require_delegation() {
    assert_eq!(ActorKind::parse("human"), Some(ActorKind::Human));
    assert_eq!(ActorKind::parse("agent"), Some(ActorKind::Agent));
    assert_eq!(ActorKind::parse("service"), Some(ActorKind::Service));
    assert_eq!(
        ActorKind::parse("control_plane"),
        Some(ActorKind::ControlPlane)
    );
    assert_eq!(ActorKind::parse("robot"), None);
    assert!(ActorKind::Agent.requires_delegation());
    assert!(!ActorKind::Human.requires_delegation());
    assert!(!ActorKind::Service.requires_delegation());
}

#[test]
fn valid_delegation_carries_separate_actor_and_subject() {
    let admitted =
        admit_agent_delegation(&lease(), &sponsor(), NOW, false).expect("valid lease is admitted");
    assert_eq!(admitted.actor_kind, "agent");
    // The agent acts; the human is the subject. They are distinct.
    assert_eq!(admitted.actor_id, "agent_forge_worker_abc");
    assert_eq!(admitted.subject_actor_id, "user_alice");
    assert_ne!(admitted.actor_id, admitted.subject_actor_id);
    assert_eq!(admitted.sponsor_actor_id, "user_alice");
    assert_eq!(admitted.delegation_id, "lease_456");
    assert_eq!(admitted.authority_scope.len(), 2);
    assert!(!admitted.production_write_allowed);
}

#[test]
fn expired_lease_is_refused() {
    let past = "2027-01-01T00:00:01Z";
    let err = admit_agent_delegation(&lease(), &sponsor(), past, false)
        .expect_err("an expired lease must be refused");
    assert_eq!(err, DelegationError::Expired);
}

#[test]
fn revoked_lease_is_refused() {
    let err = admit_agent_delegation(&lease(), &sponsor(), NOW, true)
        .expect_err("a revoked lease must be refused");
    assert_eq!(err, DelegationError::Revoked);
}

#[test]
fn scope_cannot_exceed_sponsor_authority() {
    let mut lease = lease();
    lease.authority_scope.push("strategy:write".to_string());
    let err = admit_agent_delegation(&lease, &sponsor(), NOW, false)
        .expect_err("scope beyond the sponsor must be refused");
    assert_eq!(
        err,
        DelegationError::ScopeExceedsSponsor("strategy:write".to_string())
    );
}

#[test]
fn an_agent_cannot_sponsor_itself() {
    let mut lease = lease();
    lease.sponsor_actor_id = lease.agent_actor_id.clone();
    lease.subject_actor_id = lease.agent_actor_id.clone();
    let mut sponsor = sponsor();
    sponsor.sponsor_actor_id = lease.agent_actor_id.clone();
    let err = admit_agent_delegation(&lease, &sponsor, NOW, false)
        .expect_err("self-sponsorship must be refused");
    assert_eq!(err, DelegationError::SelfSponsorship);
}

#[test]
fn subject_must_be_the_sponsor_not_a_borrowed_identity() {
    let mut lease = lease();
    // An agent trying to claim a different subject than its sponsor (piggybacking).
    lease.subject_actor_id = "user_bob".to_string();
    let err = admit_agent_delegation(&lease, &sponsor(), NOW, false)
        .expect_err("subject that is not the sponsor must be refused");
    assert_eq!(err, DelegationError::SubjectMustBeSponsor);
}

#[test]
fn tenant_and_sponsor_must_match_the_authority() {
    let mut wrong_tenant = lease();
    wrong_tenant.tenant_id = "evil".to_string();
    assert_eq!(
        admit_agent_delegation(&wrong_tenant, &sponsor(), NOW, false).unwrap_err(),
        DelegationError::TenantMismatch
    );

    let mut wrong_sponsor = lease();
    wrong_sponsor.sponsor_actor_id = "user_carol".to_string();
    wrong_sponsor.subject_actor_id = "user_carol".to_string();
    assert_eq!(
        admit_agent_delegation(&wrong_sponsor, &sponsor(), NOW, false).unwrap_err(),
        DelegationError::SponsorMismatch
    );
}

#[test]
fn empty_scope_and_missing_fields_are_refused() {
    let mut empty = lease();
    empty.authority_scope = vec![];
    assert_eq!(
        admit_agent_delegation(&empty, &sponsor(), NOW, false).unwrap_err(),
        DelegationError::EmptyScope
    );

    let mut missing = lease();
    missing.delegation_id = String::new();
    assert_eq!(
        admit_agent_delegation(&missing, &sponsor(), NOW, false).unwrap_err(),
        DelegationError::Missing("delegation_id")
    );
}

#[test]
fn control_plane_reference_is_carried_into_the_chain() {
    let mut lease = lease();
    lease.upstream_control_plane_ref = Some("control_plane_enterprise".to_string());
    let admitted = admit_agent_delegation(&lease, &sponsor(), NOW, false)
        .expect("control-plane delegation is admitted");
    assert_eq!(
        admitted.upstream_control_plane_ref.as_deref(),
        Some("control_plane_enterprise")
    );
}

#[test]
fn refusal_cases_are_declared_for_the_contract_check() {
    for expected in [
        "delegation_expired",
        "delegation_revoked",
        "delegation_scope_exceeds_sponsor",
        "delegation_self_sponsorship",
    ] {
        assert!(
            DELEGATION_REFUSAL_CASES.contains(&expected),
            "missing refusal case {expected}"
        );
    }
}
