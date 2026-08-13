use super::*;

fn honest_evidence<'a>(human_copy: &'a [&'a str]) -> SensorRunEvidence<'a> {
    SensorRunEvidence {
        all_executed_stages_have_receipts: true,
        executed_only_when_mediated: true,
        all_authority_flags_closed: true,
        admitted: true,
        status_claims_completion: true,
        human_copy,
    }
}

#[test]
fn no_fake_green_passes_an_honest_run_and_blocks_a_dishonest_one() {
    let copy = ["a quiet human line"];
    let outcomes = evaluate_pipeline_sensors(&["no_fake_green"], &honest_evidence(&copy));
    assert_eq!(outcomes[0].status, "PASSED");
    assert!(!outcomes[0].blocking);

    // An open authority flag is the canonical fake-green: the never-advisory
    // receipts sensor must FAIL and BLOCK.
    let mut dishonest = honest_evidence(&copy);
    dishonest.all_authority_flags_closed = false;
    let outcomes = evaluate_pipeline_sensors(&["no_fake_green"], &dishonest);
    assert_eq!(outcomes[0].status, "FAILED");
    assert!(outcomes[0].blocking);
    assert!(outcomes[0].reason.contains("forbidden-authority flag"));
    assert!(summarize_sensors(&outcomes).blocked);
}

#[test]
fn no_fake_green_catches_a_stage_that_claims_execution_without_a_receipt() {
    let copy = ["fine"];
    let mut dishonest = honest_evidence(&copy);
    dishonest.all_executed_stages_have_receipts = false;
    let outcomes = evaluate_pipeline_sensors(&["no_fake_green"], &dishonest);
    assert_eq!(outcomes[0].status, "FAILED");
    assert!(outcomes[0].reason.contains("without a receipt"));
}

#[test]
fn no_fake_green_catches_completion_claimed_for_an_unadmitted_pipeline() {
    let copy = ["fine"];
    let mut dishonest = honest_evidence(&copy);
    dishonest.admitted = false;
    dishonest.status_claims_completion = true;
    let outcomes = evaluate_pipeline_sensors(&["no_fake_green"], &dishonest);
    assert_eq!(outcomes[0].status, "FAILED");
    assert!(outcomes[0].reason.contains("unadmitted"));
}

#[test]
fn product_copy_quiet_screens_each_banned_first_read_phrase() {
    for phrase in SENSOR_BANNED_FIRST_READ_PHRASES {
        let line = format!("the operator sees {phrase} immediately");
        let copy = [line.as_str()];
        let outcomes = evaluate_pipeline_sensors(&["product_copy_quiet"], &honest_evidence(&copy));
        assert_eq!(
            outcomes[0].status, "FAILED",
            "phrase should have tripped: {phrase}"
        );
    }
    let clean = ["a calm, human first read"];
    let outcomes = evaluate_pipeline_sensors(&["product_copy_quiet"], &honest_evidence(&clean));
    assert_eq!(outcomes[0].status, "PASSED");
}

#[test]
fn external_and_advisory_sensors_skip_with_a_named_reason() {
    let copy = ["fine"];
    let outcomes = evaluate_pipeline_sensors(
        &["contract_battery", "taste_and_voice_review", "unknown_xyz"],
        &honest_evidence(&copy),
    );
    assert_eq!(outcomes[0].status, "SKIPPED");
    assert!(outcomes[0].reason.contains("external check runner"));
    assert_eq!(outcomes[1].status, "SKIPPED");
    assert!(outcomes[1].reason.contains("advisory"));
    assert_eq!(outcomes[2].status, "SKIPPED");
    assert!(
        outcomes[2]
            .reason
            .contains("not in the quality sensor registry")
    );
    let summary = summarize_sensors(&outcomes);
    assert_eq!(summary.skipped, 3);
    assert!(!summary.blocked);
}
