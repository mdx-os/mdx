use crate::*;

const TENANT: &str = "tenant_local";
const ACTOR: &str = "human:local_user";

fn request<'a>(role: &'a str, mode: &'a str, ack: bool) -> RecordFirstRunProfile<'a> {
    RecordFirstRunProfile {
        tenant_id: TENANT,
        actor_id: ACTOR,
        role,
        goal: "See how MDx handles a quarterly plan.",
        org_context: "A mid-size team.",
        working_mode: mode,
        privacy_ack: ack,
    }
}

#[test]
fn a_fresh_install_has_no_first_run_profile() {
    let kernel = MdxKernel::boot_local();
    assert!(!kernel.first_run_profile_local().recorded);
}

#[test]
fn recording_the_guided_start_lands_on_the_record_and_derives() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .record_first_run_profile(request("leadership", "deciding", true))
        .expect("record");
    assert_eq!(report.status, "FIRST_RUN_PROFILE_RECORDED");
    assert_eq!(report.role, "leadership");

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.receipt_id)
        .expect("profile receipt");
    assert_eq!(receipt.kind, FIRST_RUN_PROFILE_RECEIPT_KIND);
    assert_eq!(receipt.payload["role"], "leadership");
    assert_eq!(receipt.payload["working_mode"], "deciding");
    assert_eq!(receipt.payload["privacy_acknowledged"], "true");

    let state = kernel.first_run_profile_local();
    assert!(state.recorded);
    assert_eq!(state.role, "leadership");
    assert_eq!(state.goal, "See how MDx handles a quarterly plan.");
}

#[test]
fn the_local_posture_acknowledgement_is_required() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .record_first_run_profile(request("product", "exploring", false))
        .expect_err("no ack should fail");
    assert!(matches!(
        error,
        FirstRunProfileError::PrivacyNotAcknowledged
    ));
    assert!(!kernel.first_run_profile_local().recorded);
}

#[test]
fn an_unknown_role_or_mode_is_refused() {
    let mut kernel = MdxKernel::boot_local();
    assert!(matches!(
        kernel
            .record_first_run_profile(request("astronaut", "deciding", true))
            .expect_err("bad role"),
        FirstRunProfileError::UnknownRole(_)
    ));
    assert!(matches!(
        kernel
            .record_first_run_profile(request("design", "vibing", true))
            .expect_err("bad mode"),
        FirstRunProfileError::UnknownWorkingMode(_)
    ));
    assert!(!kernel.first_run_profile_local().recorded);
}

#[test]
fn an_overlong_goal_is_refused() {
    let mut kernel = MdxKernel::boot_local();
    let long_goal = "x".repeat(401);
    let error = kernel
        .record_first_run_profile(RecordFirstRunProfile {
            tenant_id: TENANT,
            actor_id: ACTOR,
            role: "engineering",
            goal: &long_goal,
            org_context: "",
            working_mode: "hands_on",
            privacy_ack: true,
        })
        .expect_err("overlong goal should fail");
    assert!(matches!(error, FirstRunProfileError::TooLong("goal", _, _)));
}
