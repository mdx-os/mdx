use super::harness_execute::{receipt_ids_of_kind, run_to_ship_readiness};
use super::*;

// Drive the full governed chain to the human ship door and return every id a
// ship decision must bind to: the review packet, the ship-readiness record, the
// worker build, the observed CI evidence, and the target commit. The kernel is
// returned separately from the ids so a request can borrow the ids while the
// decision borrows the kernel.
struct ShipIds {
    review_packet_id: String,
    readiness_id: String,
    build_id: String,
    ci_evidence_id: String,
    commit: String,
}

fn at_ship_door() -> (MdxKernel, ShipIds) {
    let (mut kernel, manifest, build_id, readiness) = run_to_ship_readiness();
    let patch_ids = receipt_ids_of_kind(&kernel, "harness.patch.applied");
    let test_ids = receipt_ids_of_kind(&kernel, "harness.test.executed");
    let patch_refs: Vec<&str> = patch_ids.iter().map(String::as_str).collect();
    let test_refs: Vec<&str> = test_ids.iter().map(String::as_str).collect();
    let packet = LocalHarnessReviewPacket
        .assemble(
            &mut kernel,
            &manifest,
            &HarnessReviewPacketRequest {
                worker_run_id: "worker_run_review",
                readiness_receipt_id: &readiness.readiness_receipt_id,
                worker_build_receipt_id: &build_id,
                patch_receipt_ids: &patch_refs,
                test_receipt_ids: &test_refs,
                ci_evidence_receipt_id: &readiness.ci_evidence_receipt_id,
                approved_plan_hash: "sha256:local_plan_hash_001",
            },
        )
        .expect("review packet");
    let ids = ShipIds {
        review_packet_id: packet.packet_receipt_id,
        readiness_id: readiness.readiness_receipt_id,
        ci_evidence_id: readiness.ci_evidence_receipt_id,
        build_id,
        commit: "deadbeef".to_string(),
    };
    (kernel, ids)
}

impl ShipIds {
    // A fully valid ratify request bound to this door's chain. Negative tests
    // mutate one field.
    fn request(&self, decision: HumanShipDecision) -> ForgeShipDecisionRequest<'_> {
        ForgeShipDecisionRequest {
            tenant_id: "tenant_local",
            human_actor_id: "human_platform_lead",
            human_actor_kind: "human",
            decision,
            requested_authority: "ship_ratification",
            authorization_receipt_id: &self.build_id,
            review_packet_receipt_id: &self.review_packet_id,
            ship_readiness_receipt_id: &self.readiness_id,
            worker_build_receipt_id: &self.build_id,
            ci_evidence_receipt_id: &self.ci_evidence_id,
            target_commit_sha: &self.commit,
            scope: "crates/mdx-core",
            decision_reason: "evidence is complete and the change is in scope",
            decided_at: "2026-06-08T01:00:00Z",
            expires_at: "2026-06-08T02:00:00Z",
        }
    }
}

#[test]
fn an_authorized_human_can_ratify_with_the_full_evidence_chain() {
    let (mut kernel, ids) = at_ship_door();
    let req = ids.request(HumanShipDecision::Ratify);
    let report = LocalForgeShipRatification
        .decide(&mut kernel, &req)
        .expect("decision");
    assert_eq!(report.status, "SHIP_RATIFIED");
    assert!(report.forge_ship_ratified);
    assert!(report.ship_authority_allowed);
    // The human edge ratifies a ship; it never authorizes deployment or prod.
    assert!(!report.deployment_authority_granted);
    assert!(!report.production_write_allowed);
    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.decision_receipt_id)
        .expect("receipt");
    assert_eq!(receipt.kind, "forge.ship.ratified");
    assert_eq!(
        receipt
            .payload
            .get("deployment_authority_granted")
            .map(String::as_str),
        Some("false")
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn the_other_decisions_record_their_own_kinds_and_never_ratify() {
    for (decision, kind, terminal) in [
        (
            HumanShipDecision::Reject,
            "forge.ship.rejected",
            "SHIP_REJECTED",
        ),
        (
            HumanShipDecision::RequestRevision,
            "forge.ship.revision_requested",
            "SHIP_REVISION_REQUESTED",
        ),
        (
            HumanShipDecision::RerunTests,
            "forge.ship.tests_rerun_requested",
            "SHIP_TESTS_RERUN_REQUESTED",
        ),
        (
            HumanShipDecision::Escalate,
            "forge.ship.escalated",
            "SHIP_ESCALATED",
        ),
    ] {
        let (mut kernel, ids) = at_ship_door();
        let req = ids.request(decision);
        let report = LocalForgeShipRatification
            .decide(&mut kernel, &req)
            .expect("decision");
        assert_eq!(report.terminal_state, terminal);
        assert!(!report.forge_ship_ratified, "{kind} must not ratify");
        assert!(!report.ship_authority_allowed);
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.decision_receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, kind);
        assert!(kernel.ledger().verify().is_ok());
    }
}

fn blocked_reason(mutate: impl FnOnce(&mut ForgeShipDecisionRequest)) -> String {
    let (mut kernel, ids) = at_ship_door();
    let mut req = ids.request(HumanShipDecision::Ratify);
    mutate(&mut req);
    let report = LocalForgeShipRatification
        .decide(&mut kernel, &req)
        .expect("decision");
    assert_eq!(report.status, "SHIP_DECISION_BLOCKED");
    assert!(!report.forge_ship_ratified);
    assert!(!report.deployment_authority_granted);
    report.denied_reason.expect("a denial reason")
}

#[test]
fn ratification_blocks_on_a_missing_human_actor() {
    assert_eq!(
        blocked_reason(|r| r.human_actor_id = "  "),
        "missing_human_actor_id"
    );
}

#[test]
fn ratification_blocks_on_a_missing_review_packet() {
    assert_eq!(
        blocked_reason(|r| r.review_packet_receipt_id = "no_such_receipt"),
        "missing_review_packet_receipt"
    );
}

#[test]
fn ratification_blocks_on_missing_ci_evidence() {
    assert_eq!(
        blocked_reason(|r| r.ci_evidence_receipt_id = "no_such_receipt"),
        "missing_ci_evidence_receipt"
    );
}

#[test]
fn ratification_blocks_on_a_wrong_commit() {
    // A different target commit no longer binds to the readiness or CI evidence.
    assert_eq!(
        blocked_reason(|r| r.target_commit_sha = "0000000"),
        "ship_readiness_commit_mismatch"
    );
}

#[test]
fn ratification_blocks_short_or_reversed_commit_prefixes() {
    assert_eq!(
        blocked_reason(|r| r.target_commit_sha = "dead"),
        "ship_readiness_commit_mismatch"
    );
    assert_eq!(
        blocked_reason(|r| r.target_commit_sha = "deadbeef0000"),
        "ship_readiness_commit_mismatch"
    );
}

#[test]
fn ratification_blocks_on_a_stale_decision() {
    assert_eq!(
        blocked_reason(|r| {
            r.decided_at = "2026-06-08T03:00:00Z";
            r.expires_at = "2026-06-08T02:00:00Z";
        }),
        "decision_expired"
    );
}

#[test]
fn an_autonomous_actor_cannot_ratify() {
    // A non-human attestation blocks...
    assert_eq!(
        blocked_reason(|r| r.human_actor_kind = "autonomous"),
        "non_human_actor:autonomous"
    );
    // ...and so does a human-attested but autonomous-looking actor id.
    assert_eq!(
        blocked_reason(|r| r.human_actor_id = "worker_42"),
        "autonomous_actor_cannot_ratify"
    );
}

#[test]
fn deployment_cannot_be_requested_through_ratification() {
    assert_eq!(
        blocked_reason(|r| r.requested_authority = "deployment"),
        "authority_not_granted_here:deployment"
    );
}
