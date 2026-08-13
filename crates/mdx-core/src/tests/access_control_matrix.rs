use super::*;

// The leakage matrix is validated against the real RLS table sets declared in
// lib.rs, so a cell can never claim a database backstop the migrations do not
// provide. These accessors expose those private consts to the test.
fn resource_aware_tables() -> &'static [&'static str] {
    RESOURCE_AWARE_RLS_OVERRIDES
}
fn rls_tables() -> &'static [&'static str] {
    RLS_TABLES
}

#[test]
fn leakage_matrix_validates_against_the_real_rls_tables() {
    let report = validate_leakage_matrix(resource_aware_tables(), rls_tables())
        .expect("leakage matrix is internally consistent and DB-grounded");
    assert_eq!(report.cells, LEAKAGE_MATRIX.len());
    assert!(report.allow_cells >= 1, "matrix must prove allowed cases");
    assert!(report.deny_cells >= 1, "matrix must prove denied cases");
    assert_eq!(report.leakage_classes, LEAKAGE_CLASSES.len());
    // A mode string or a passing matrix never flips live readiness.
    assert!(!report.production_write_allowed);
}

#[test]
fn rule_engine_agrees_with_every_declared_cell() {
    for cell in LEAKAGE_MATRIX {
        assert_eq!(
            evaluate(cell.surface, cell.principal),
            cell.api,
            "rule engine disagrees with declared cell {}::{}",
            cell.surface.as_str(),
            cell.principal.as_str()
        );
    }
}

#[test]
fn every_leakage_class_is_covered() {
    for class in LEAKAGE_CLASSES {
        assert!(
            LEAKAGE_MATRIX.iter().any(|c| &c.leakage_class == class),
            "leakage class {class} has no cell"
        );
    }
}

#[test]
fn every_sensitive_surface_appears_in_the_matrix() {
    use Surface::*;
    for surface in [
        Twin,
        Message,
        Pages,
        Forge,
        Talent,
        Product,
        Strategy,
        Observatory,
        Charter,
        Receipt,
        Admin,
    ] {
        assert!(
            LEAKAGE_MATRIX.iter().any(|c| c.surface == surface),
            "surface {} has no matrix cell",
            surface.as_str()
        );
    }
}

#[test]
fn same_tenant_membership_is_never_access_to_a_private_surface() {
    // The core leak we refuse: a same-tenant outsider on any private surface is
    // always denied at the application layer.
    for cell in LEAKAGE_MATRIX {
        if cell.principal == Principal::SameTenantOutsider {
            assert_eq!(
                cell.api,
                Decision::Deny,
                "{} allows a same-tenant outsider",
                cell.surface.as_str()
            );
        }
    }
}

#[test]
fn cross_tenant_is_denied_everywhere() {
    for cell in LEAKAGE_MATRIX {
        if cell.principal == Principal::CrossTenantActor {
            assert_eq!(
                cell.api,
                Decision::Deny,
                "{} allows a cross-tenant actor",
                cell.surface.as_str()
            );
        }
    }
}

#[test]
fn tenant_admin_cannot_read_private_user_content() {
    for cell in LEAKAGE_MATRIX {
        if cell.principal == Principal::TenantAdminPrivateContent {
            assert_eq!(
                cell.api,
                Decision::Deny,
                "{} lets a tenant admin read private content",
                cell.surface.as_str()
            );
        }
    }
}

#[test]
fn unscoped_service_and_out_of_scope_agents_are_denied() {
    for cell in LEAKAGE_MATRIX {
        match cell.principal {
            Principal::ServiceActorUnscoped | Principal::DelegatedAgentOutOfScope => {
                assert_eq!(
                    cell.api,
                    Decision::Deny,
                    "{}::{} must be denied",
                    cell.surface.as_str(),
                    cell.principal.as_str()
                );
            }
            _ => {}
        }
    }
}

#[test]
fn resource_aware_backstop_is_claimed_only_where_rls_v2_provides_it() {
    // Twin, Message, Forge map to RLS v2 override tables; all other surfaces are
    // honestly tenant-only at the DB and tracked as pending.
    for cell in LEAKAGE_MATRIX {
        let table = cell.surface.private_table();
        match cell.db {
            DbBackstop::ResourceAware => assert!(
                RESOURCE_AWARE_RLS_OVERRIDES.contains(&table),
                "{} claims resource-aware but {} has no RLS v2 policy",
                cell.surface.as_str(),
                table
            ),
            DbBackstop::TenantOnly => {
                assert!(
                    !RESOURCE_AWARE_RLS_OVERRIDES.contains(&table),
                    "{} under-claims: {} has an RLS v2 policy",
                    cell.surface.as_str(),
                    table
                );
                assert!(
                    PENDING_RESOURCE_AWARE_RLS_SURFACES.contains(&cell.surface.as_str()),
                    "{} tenant-only DB gap is not tracked as pending",
                    cell.surface.as_str()
                );
            }
        }
    }
}

#[test]
fn deny_vocabulary_is_declared_for_the_contract_check() {
    for expected in [
        "deny_same_tenant_outsider",
        "deny_cross_tenant",
        "deny_delegated_agent_out_of_scope",
        "deny_service_actor_unscoped",
        "deny_tenant_admin_private_content",
        "deny_security_operator_content",
    ] {
        assert!(
            ACCESS_DENY_CASES.contains(&expected),
            "missing deny case {expected}"
        );
    }
}
