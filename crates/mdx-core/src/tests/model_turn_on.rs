use crate::*;

const TENANT: &str = "tenant_local";
const ACTOR: &str = "human:local_user";

#[test]
fn approving_a_turn_on_records_who_and_which_provider_no_secret() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .approve_model_turn_on(ApproveModelTurnOn {
            tenant_id: TENANT,
            actor_id: ACTOR,
            provider_id: "xai",
        })
        .expect("approve");
    assert_eq!(report.status, "MODEL_TURN_ON_APPROVED");
    assert_eq!(report.provider_id, "xai");
    assert!(!report.approval_receipt_id.is_empty());

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.approval_receipt_id)
        .expect("approval receipt");
    assert_eq!(receipt.kind, MODEL_TURN_ON_APPROVAL_RECEIPT_KIND);
    assert_eq!(receipt.payload["provider_id"], "xai");
    assert_eq!(receipt.payload["approved_by"], ACTOR);
    assert_eq!(receipt.payload["credential_recorded"], "false");
    // The approval never carries a key or any secret-bearing field.
    assert!(
        !receipt
            .payload
            .keys()
            .any(|k| k.contains("key") || k.contains("secret"))
    );
}

#[test]
fn a_blank_provider_is_refused() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .approve_model_turn_on(ApproveModelTurnOn {
            tenant_id: TENANT,
            actor_id: ACTOR,
            provider_id: "  ",
        })
        .expect_err("blank provider should fail");
    assert!(matches!(error, ModelTurnOnError::Missing("provider_id")));
}
