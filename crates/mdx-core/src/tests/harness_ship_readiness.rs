use super::*;

// Pure pre-ledger gates (they return before any receipt lookup). The full
// ledger-binding green path and tamper cases live in tests/harness_execute.rs
// where a real governed worker build produces the receipts to bind to.

fn ci(conclusion: &'static str) -> HarnessCiEvidence<'static> {
    HarnessCiEvidence {
        workflow_run_id: "27106096662",
        commit_sha: "e06d4f02",
        branch: "main",
        conclusion,
        evidence_url: "https://github.com/mdx-os/mdx/actions/runs/27106096662",
        observed_at: "2026-06-07T21:30:00Z",
        observed_via: "github_actions_adapter",
    }
}

#[test]
fn missing_test_evidence_blocks_before_any_ledger_lookup() {
    let mut kernel = MdxKernel::boot_local();
    let manifest = valid_harness_run_manifest();
    let request = HarnessShipReadinessRequest {
        worker_run_id: "worker_run_alpha",
        worker_build_receipt_id: "receipt_worker_build",
        patch_receipt_ids: &["receipt_patch_1"],
        test_receipt_ids: &[],
        approved_plan_hash: "sha256:plan",
        build_commit_sha: "e06d4f02",
        build_branch: "main",
        ci_evidence: ci("success"),
    };
    let report = LocalHarnessShipReadiness
        .assemble(&mut kernel, &manifest, &request)
        .expect("ship readiness");
    assert_eq!(report.status, "SHIP_READINESS_BLOCKED");
    assert_eq!(
        report.denied_reason.as_deref(),
        Some("missing_test_evidence")
    );
}

#[test]
fn a_fabricated_worker_build_receipt_blocks() {
    let mut kernel = MdxKernel::boot_local();
    let manifest = valid_harness_run_manifest();
    // No such receipt exists in the ledger - caller-provided strings do not
    // pass. This is the core of the ledger-binding fix.
    let request = HarnessShipReadinessRequest {
        worker_run_id: "worker_run_alpha",
        worker_build_receipt_id: "receipt_that_does_not_exist",
        patch_receipt_ids: &["receipt_patch_1"],
        test_receipt_ids: &["receipt_test_1"],
        approved_plan_hash: "sha256:plan",
        build_commit_sha: "e06d4f02",
        build_branch: "main",
        ci_evidence: ci("success"),
    };
    let report = LocalHarnessShipReadiness
        .assemble(&mut kernel, &manifest, &request)
        .expect("ship readiness");
    assert_eq!(report.status, "SHIP_READINESS_BLOCKED");
    assert_eq!(
        report.denied_reason.as_deref(),
        Some("missing_worker_build_receipt")
    );
    assert!(!report.ci_evidence_observed);
}
