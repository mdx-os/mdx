use super::*;

const NOW: &str = "2026-06-08T00:00:00Z";

fn profile() -> AuthProfile {
    AuthProfile::new("mdx-local-issuer", "mdx")
}

fn human_claims() -> VerifiedClaims {
    VerifiedClaims {
        issuer: "mdx-local-issuer".to_string(),
        audience: "mdx".to_string(),
        expires_at: "2030-01-01T00:00:00Z".to_string(),
        tenant_id: "acme".to_string(),
        actor_id: "user_alice".to_string(),
        actor_role: "member".to_string(),
        actor_kind: "human".to_string(),
        subject_actor_id: "user_alice".to_string(),
        delegation_id: None,
        authority_scope: vec![],
        ..Default::default()
    }
}

/// A valid agent token: the agent is distinct from the sponsoring subject, and
/// its delegated scope is within the sponsor's attested authority.
fn agent_claims() -> VerifiedClaims {
    VerifiedClaims {
        actor_kind: "agent".to_string(),
        actor_id: "agent_worker".to_string(),
        subject_actor_id: "user_alice".to_string(),
        delegation_id: Some("lease_1".to_string()),
        authority_scope: vec!["forge:build:read".to_string()],
        sponsor_scope: vec![
            "forge:build:read".to_string(),
            "forge:build:update".to_string(),
        ],
        issued_at: "2026-06-01T00:00:00Z".to_string(),
        policy_decision_id: "policy_1".to_string(),
        ..human_claims()
    }
}

#[test]
fn valid_human_claims_admit_a_trusted_session() {
    let session = admit_verified_claims(&profile(), &human_claims(), NOW).expect("admits");
    assert_eq!(session.tenant_id, "acme");
    assert_eq!(session.actor_id, "user_alice");
    assert_eq!(session.actor_role, "member");
    assert_eq!(session.actor_kind, "human");
    assert_eq!(session.subject_actor_id, "user_alice");
}

#[test]
fn valid_agent_claims_with_delegation_admit() {
    let session = admit_verified_claims(&profile(), &agent_claims(), NOW).expect("agent admits");
    assert_eq!(session.actor_kind, "agent");
    assert_eq!(session.actor_id, "agent_worker");
    assert_eq!(session.subject_actor_id, "user_alice");
}

#[test]
fn agent_scope_exceeding_sponsor_is_refused() {
    let mut claims = agent_claims();
    claims.authority_scope = vec!["forge:build:delete".to_string()];
    let refusal = admit_verified_claims(&profile(), &claims, NOW).expect_err("scope exceeds");
    assert_eq!(refusal.code(), "delegation_scope_exceeds_sponsor");
}

#[test]
fn agent_acting_as_its_own_sponsor_is_refused() {
    let mut claims = agent_claims();
    // The agent names itself as the subject: it cannot act as a human session.
    claims.subject_actor_id = "agent_worker".to_string();
    let refusal = admit_verified_claims(&profile(), &claims, NOW).expect_err("self sponsorship");
    assert_eq!(refusal.code(), "delegation_self_sponsorship");
}

#[test]
fn expired_agent_delegation_is_refused() {
    let mut claims = agent_claims();
    claims.expires_at = "2026-06-07T00:00:00Z".to_string();
    let refusal = admit_verified_claims(&profile(), &claims, NOW).expect_err("expired");
    // The token-level expiry fires first; both are fail-closed.
    assert!(refusal.code() == "verifier_expired_token" || refusal.code() == "delegation_expired");
}

#[test]
fn revoked_agent_delegation_is_refused() {
    let mut claims = agent_claims();
    claims.revoked = true;
    let refusal = admit_verified_claims(&profile(), &claims, NOW).expect_err("revoked");
    assert_eq!(refusal.code(), "delegation_revoked");
}

#[test]
fn agent_with_empty_scope_is_refused() {
    let mut claims = agent_claims();
    claims.authority_scope = vec![];
    let refusal = admit_verified_claims(&profile(), &claims, NOW).expect_err("empty scope");
    assert_eq!(refusal.code(), "delegation_empty_scope");
}

#[test]
fn missing_token_is_refused() {
    assert_eq!(
        precheck_token_presence(&profile(), None),
        Err(VerifierRefusal::MissingToken)
    );
    assert_eq!(
        precheck_token_presence(&profile(), Some("   ")),
        Err(VerifierRefusal::MissingToken)
    );
}

#[test]
fn unconfigured_auth_profile_is_refused() {
    let unconfigured = AuthProfile::default();
    assert_eq!(
        precheck_token_presence(&unconfigured, Some("iss=x")),
        Err(VerifierRefusal::UnconfiguredAuthProfile)
    );
    assert_eq!(
        admit_verified_claims(&unconfigured, &human_claims(), NOW),
        Err(VerifierRefusal::UnconfiguredAuthProfile)
    );
}

#[test]
fn malformed_token_is_refused() {
    // A body segment without `=`, and a wholly empty body, are malformed.
    assert_eq!(
        parse_claim_token("iss=x;this-has-no-equals"),
        Err(VerifierRefusal::MalformedToken)
    );
    assert_eq!(
        parse_claim_token("   "),
        Err(VerifierRefusal::MalformedToken)
    );
}

#[test]
fn issuer_mismatch_is_refused() {
    let mut claims = human_claims();
    claims.issuer = "evil-issuer".to_string();
    assert_eq!(
        admit_verified_claims(&profile(), &claims, NOW),
        Err(VerifierRefusal::IssuerMismatch)
    );
}

#[test]
fn audience_mismatch_is_refused() {
    let mut claims = human_claims();
    claims.audience = "some-other-app".to_string();
    assert_eq!(
        admit_verified_claims(&profile(), &claims, NOW),
        Err(VerifierRefusal::AudienceMismatch)
    );
}

#[test]
fn expired_token_is_refused() {
    let mut claims = human_claims();
    claims.expires_at = "2020-01-01T00:00:00Z".to_string();
    assert_eq!(
        admit_verified_claims(&profile(), &claims, NOW),
        Err(VerifierRefusal::ExpiredToken)
    );
}

#[test]
fn tenant_mapping_missing_is_refused() {
    let mut claims = human_claims();
    claims.tenant_id = String::new();
    assert_eq!(
        admit_verified_claims(&profile(), &claims, NOW),
        Err(VerifierRefusal::TenantMappingMissing)
    );
}

#[test]
fn actor_mapping_missing_is_refused() {
    let mut claims = human_claims();
    claims.actor_id = "   ".to_string();
    assert_eq!(
        admit_verified_claims(&profile(), &claims, NOW),
        Err(VerifierRefusal::ActorMappingMissing)
    );
}

#[test]
fn role_mapping_missing_is_refused() {
    let mut claims = human_claims();
    claims.actor_role = String::new();
    assert_eq!(
        admit_verified_claims(&profile(), &claims, NOW),
        Err(VerifierRefusal::RoleMappingMissing)
    );
}

#[test]
fn subject_mapping_missing_is_refused() {
    let mut claims = human_claims();
    claims.subject_actor_id = String::new();
    assert_eq!(
        admit_verified_claims(&profile(), &claims, NOW),
        Err(VerifierRefusal::SubjectMappingMissing)
    );
}

#[test]
fn unsupported_actor_kind_is_refused() {
    let mut claims = human_claims();
    claims.actor_kind = "robot".to_string();
    assert_eq!(
        admit_verified_claims(&profile(), &claims, NOW),
        Err(VerifierRefusal::ActorKindUnsupported)
    );
}

#[test]
fn agent_without_delegation_is_refused() {
    let mut claims = human_claims();
    claims.actor_kind = "agent".to_string();
    claims.actor_id = "agent_worker".to_string();
    claims.delegation_id = None;
    assert_eq!(
        admit_verified_claims(&profile(), &claims, NOW),
        Err(VerifierRefusal::DelegationMissingForAgent)
    );
    // An empty delegation id is also missing.
    claims.delegation_id = Some("  ".to_string());
    assert_eq!(
        admit_verified_claims(&profile(), &claims, NOW),
        Err(VerifierRefusal::DelegationMissingForAgent)
    );
}

#[test]
fn reference_token_parses_round_trip() {
    let token = "iss=mdx-local-issuer;aud=mdx;exp=2030-01-01T00:00:00Z;tenant=acme;\
                 actor=user_alice;role=member;kind=human;subject=user_alice";
    let claims = parse_claim_token(token).expect("parses");
    let session = admit_verified_claims(&profile(), &claims, NOW).expect("admits");
    assert_eq!(session.actor_id, "user_alice");
}

#[test]
fn every_refusal_case_is_declared() {
    for refusal in [
        VerifierRefusal::MissingToken,
        VerifierRefusal::MalformedToken,
        VerifierRefusal::UnconfiguredAuthProfile,
        VerifierRefusal::IssuerMismatch,
        VerifierRefusal::AudienceMismatch,
        VerifierRefusal::ExpiredToken,
        VerifierRefusal::TenantMappingMissing,
        VerifierRefusal::ActorMappingMissing,
        VerifierRefusal::RoleMappingMissing,
        VerifierRefusal::SubjectMappingMissing,
        VerifierRefusal::ActorKindUnsupported,
        VerifierRefusal::DelegationMissingForAgent,
        VerifierRefusal::JwksFetchFailed,
        VerifierRefusal::SigningKeyUnknown,
    ] {
        assert!(
            VERIFIER_REFUSAL_CASES.contains(&refusal.code()),
            "missing declared refusal {}",
            refusal.code()
        );
    }
    assert_eq!(VERIFIER_REFUSAL_CASES.len(), 14);
}

#[test]
fn production_write_stays_blocked() {
    // Producing a trusted session never flips live readiness; that is the auth
    // boundary's job downstream, and it keeps production_write_allowed false.
    let session = admit_verified_claims(&profile(), &human_claims(), NOW).expect("admits");
    let admitted = admit_governed_write_identity(
        DeploymentMode::Production,
        Some(&session),
        &ClientClaimedIdentity::default(),
    )
    .expect("trusted session admits");
    assert!(!admitted.production_write_allowed);
}
