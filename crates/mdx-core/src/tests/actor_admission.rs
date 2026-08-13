use crate::*;

fn valid_admission() -> ActorAdmission<'static> {
    ActorAdmission {
        tenant_id: "local_tenant",
        actor_id: "local_user",
        actor_role: "owner",
        policy_decision_id: "policy-local-actor-admission",
        receipt_intent: "governed_local_write",
        source_route: "/messages/fanout-requests.json",
        request_id: "local-request-fixture",
        requested_tenant_id: "local_tenant",
        requested_actor_role: "owner",
        service_role_claimed: false,
        user_metadata_authorization_claimed: false,
    }
}

#[test]
fn local_actor_admission_accepts_governed_write_packet() {
    let report = admit_local_actor_for_governed_write(valid_admission()).expect("admitted");

    assert_eq!(report.status, "LOCAL_ACTOR_ADMITTED");
    assert_eq!(report.tenant_id, "local_tenant");
    assert_eq!(report.actor_id, "local_user");
    assert_eq!(report.source_route, "/messages/fanout-requests.json");
    assert_eq!(report.policy_decision_id, "policy-local-actor-admission");
    assert!(!report.hidden_privilege_bypass_allowed);
    assert!(!report.production_write_allowed);
}

#[test]
fn local_route_actor_helper_accepts_declared_route_families() {
    for route in GOVERNED_WRITE_ROUTES {
        let report = admit_local_route_actor(
            "local_tenant",
            "local_user",
            "owner",
            route,
            "governed_local_write",
            "request_id",
        )
        .expect("declared route admitted");
        assert_eq!(report.source_route, *route);
        assert_eq!(report.policy_decision_id, "policy-local-actor-admission");
    }
}

#[test]
fn local_actor_admission_refuses_hidden_privilege_paths() {
    let mut service_role = valid_admission();
    service_role.service_role_claimed = true;
    assert!(matches!(
        admit_local_actor_for_governed_write(service_role),
        Err(ActorAdmissionError::ServiceRoleBypass)
    ));

    let mut user_metadata = valid_admission();
    user_metadata.user_metadata_authorization_claimed = true;
    assert!(matches!(
        admit_local_actor_for_governed_write(user_metadata),
        Err(ActorAdmissionError::UserMetadataAuthorization)
    ));
}

#[test]
fn local_actor_admission_refuses_route_tenant_and_role_drift() {
    let mut route = valid_admission();
    route.source_route = "/unsafe/write.json";
    assert!(matches!(
        admit_local_actor_for_governed_write(route),
        Err(ActorAdmissionError::SourceRouteMismatch(_))
    ));

    let mut tenant = valid_admission();
    tenant.requested_tenant_id = "other_tenant";
    assert!(matches!(
        admit_local_actor_for_governed_write(tenant),
        Err(ActorAdmissionError::TenantSwitchWithoutPolicy)
    ));

    let mut role = valid_admission();
    role.requested_actor_role = "service_role";
    assert!(matches!(
        admit_local_actor_for_governed_write(role),
        Err(ActorAdmissionError::RoleEscalationWithoutPolicy)
    ));
}
