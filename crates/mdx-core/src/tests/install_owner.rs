use crate::*;

const TENANT: &str = "tenant_local";
const ACTOR: &str = "human:local_user";

fn claim<'a>(name: &'a str, actor: &'a str) -> ClaimInstallOwner<'a> {
    ClaimInstallOwner {
        tenant_id: TENANT,
        actor_id: actor,
        owner_name: name,
    }
}

#[test]
fn a_fresh_install_has_no_owner() {
    let kernel = MdxKernel::boot_local();
    let state = kernel.install_owner_state();
    assert!(!state.claimed);
    assert_eq!(state.owner_name, "");
}

#[test]
fn the_first_claim_establishes_the_owner_on_the_record() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .claim_install_owner(claim("Dana", ACTOR))
        .expect("claim");
    assert_eq!(report.status, "INSTALL_OWNER_CLAIMED");
    assert!(report.claimed);
    assert!(!report.already_claimed);
    assert_eq!(report.owner_name, "Dana");

    // It is on the record, and the derived state agrees.
    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.receipt_id)
        .expect("owner receipt");
    assert_eq!(receipt.kind, INSTALL_OWNER_RECEIPT_KIND);
    assert_eq!(receipt.payload["owner_name"], "Dana");
    assert_eq!(receipt.payload["auth_method"], "claim_no_password");

    let state = kernel.install_owner_state();
    assert!(state.claimed);
    assert_eq!(state.owner_name, "Dana");
    assert_eq!(state.owner_actor_id, ACTOR);
}

#[test]
fn a_second_claim_is_refused_and_returns_the_existing_owner() {
    let mut kernel = MdxKernel::boot_local();
    kernel
        .claim_install_owner(claim("Dana", ACTOR))
        .expect("first claim");
    let second = kernel
        .claim_install_owner(claim("Imposter", "human:someone_else"))
        .expect("second claim returns a report");
    assert_eq!(second.status, "INSTALL_OWNER_ALREADY_CLAIMED");
    assert!(second.already_claimed);
    assert_eq!(second.owner_name, "Dana", "ownership is never overwritten");

    // Exactly one owner receipt exists: the second claim wrote nothing.
    let owner_receipts = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == INSTALL_OWNER_RECEIPT_KIND)
        .count();
    assert_eq!(owner_receipts, 1);
}

#[test]
fn a_blank_owner_name_is_refused() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .claim_install_owner(claim("   ", ACTOR))
        .expect_err("blank name should fail");
    assert!(matches!(error, InstallOwnerError::Missing("owner_name")));
}
