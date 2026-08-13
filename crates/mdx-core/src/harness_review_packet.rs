// Slice C: the reviewable change artifact. Once ship readiness has assembled a
// READY_FOR_HUMAN_RATIFICATION packet, a human still needs ONE record to look
// at - what changed, did the tests pass, did CI pass - before they ratify.
//
// This assembles that record from the ledger receipts (never from caller
// strings): it reads the worker build, every patch, every test, the observed
// CI evidence, and the readiness receipt, pulls the PRESENCE-ONLY facts (how
// many files and lines changed, how many tests passed, the CI conclusion and
// evidence URL), and emits a harness.review.packet.assembled receipt. It never
// reads a diff body, file content, or command output text - none of those are
// in the receipts to begin with.
//
// It is evidence, not authority. The packet states the human decision and the
// boundary; it never ratifies, authorizes deployment, or deploys. Those stay
// the human edge.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarnessReviewPacketRequest<'a> {
    pub worker_run_id: &'a str,
    /// The harness.ship.readiness.assembled receipt this packet reviews.
    pub readiness_receipt_id: &'a str,
    pub worker_build_receipt_id: &'a str,
    pub patch_receipt_ids: &'a [&'a str],
    pub test_receipt_ids: &'a [&'a str],
    /// The harness.ci.evidence.observed receipt (not caller-asserted CI).
    pub ci_evidence_receipt_id: &'a str,
    /// The plan hash every build receipt must bind to.
    pub approved_plan_hash: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessReviewPacketReport {
    pub status: &'static str,
    pub denied_reason: Option<String>,
    pub packet_receipt_id: String,
    // Presence-only change summary, aggregated from the patch receipts.
    pub patch_count: u64,
    pub files_changed: u64,
    pub lines_added: u64,
    pub lines_removed: u64,
    // Test outcome, aggregated from the test receipts.
    pub test_count: u64,
    pub tests_passed: u64,
    // The observed CI result.
    pub ci_conclusion: String,
    pub ci_evidence_url: String,
    // What the human is being asked to do, and the line the packet does not cross.
    pub human_decision: String,
    pub boundary: String,
    // The human edge. Never granted here.
    pub forge_ship_ratified: bool,
    pub deployment_authority_granted: bool,
    pub production_write_allowed: bool,
    pub receipt_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalHarnessReviewPacket;

impl LocalHarnessReviewPacket {
    pub fn assemble<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessReviewPacketRequest<'_>,
    ) -> Result<HarnessReviewPacketReport, String> {
        kernel.assemble_harness_review_packet(manifest, request)
    }
}

// The presence-only numbers a patch or test receipt carries. Summing them never
// touches the change content - only counts the receipts already recorded.
struct ChangeSummary {
    files_changed: u64,
    lines_added: u64,
    lines_removed: u64,
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn assemble_harness_review_packet(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessReviewPacketRequest<'_>,
    ) -> Result<HarnessReviewPacketReport, String> {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("harness_review_packet"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("review_packet");

        // Bind to real receipts; a missing or mismatched receipt blocks the
        // packet rather than presenting a half-built change to the human.
        match self.gather_review_facts(request) {
            Err(reason) => {
                let decision =
                    self.decide_with_receipt(&correlation, ActionKind::AssembleHarnessReviewPacket);
                let receipt = self.transition_receipt(
                    &run_id,
                    "REVIEW_PACKET_BLOCKED",
                    &correlation,
                    &decision,
                    "harness.review.packet.blocked",
                    payload(&[
                        ("worker_run_id", request.worker_run_id),
                        ("denied_reason", &reason),
                        ("forge_ship_ratified", "false"),
                        ("deployment_authority_granted", "false"),
                    ]),
                );
                self.storage.ledger().verify()?;
                Ok(HarnessReviewPacketReport {
                    status: "REVIEW_PACKET_BLOCKED",
                    denied_reason: Some(reason),
                    packet_receipt_id: receipt.receipt_id.clone(),
                    patch_count: 0,
                    files_changed: 0,
                    lines_added: 0,
                    lines_removed: 0,
                    test_count: 0,
                    tests_passed: 0,
                    ci_conclusion: String::new(),
                    ci_evidence_url: String::new(),
                    human_decision: "resolve the blocked change evidence before review".to_string(),
                    boundary: "the packet does not ratify, authorize deployment, or deploy"
                        .to_string(),
                    forge_ship_ratified: false,
                    deployment_authority_granted: false,
                    production_write_allowed: false,
                    receipt_ids: vec![receipt.receipt_id],
                })
            }
            Ok(facts) => {
                let patch_count = request.patch_receipt_ids.len() as u64;
                let test_count = request.test_receipt_ids.len() as u64;
                let files_changed = facts.summary.files_changed.to_string();
                let lines_added = facts.summary.lines_added.to_string();
                let lines_removed = facts.summary.lines_removed.to_string();
                let patch_count_s = patch_count.to_string();
                let test_count_s = test_count.to_string();
                let tests_passed_s = facts.tests_passed.to_string();
                let patch_ids = request.patch_receipt_ids.join(",");
                let test_ids = request.test_receipt_ids.join(",");
                let human_decision =
                    "ratify or reject this change at the Forge human ship door".to_string();
                let boundary =
                    "evidence only: the packet does not ratify, authorize deployment, or deploy"
                        .to_string();

                let decision =
                    self.decide_with_receipt(&correlation, ActionKind::AssembleHarnessReviewPacket);
                let receipt = self.transition_receipt(
                    &run_id,
                    "REVIEW_PACKET_ASSEMBLED",
                    &correlation,
                    &decision,
                    "harness.review.packet.assembled",
                    payload(&[
                        ("worker_run_id", request.worker_run_id),
                        ("readiness_receipt_id", request.readiness_receipt_id),
                        ("worker_build_receipt_id", request.worker_build_receipt_id),
                        ("patch_receipt_ids", &patch_ids),
                        ("test_receipt_ids", &test_ids),
                        ("ci_evidence_receipt_id", request.ci_evidence_receipt_id),
                        ("patch_count", &patch_count_s),
                        ("files_changed", &files_changed),
                        ("lines_added", &lines_added),
                        ("lines_removed", &lines_removed),
                        ("test_count", &test_count_s),
                        ("tests_passed", &tests_passed_s),
                        ("ci_conclusion", &facts.ci_conclusion),
                        ("ci_evidence_url", &facts.ci_evidence_url),
                        ("human_decision", &human_decision),
                        ("boundary", &boundary),
                        // Presence-only, by construction.
                        ("diff_body_recorded", "false"),
                        ("file_content_recorded", "false"),
                        ("command_output_recorded", "false"),
                        // The human edge, never crossed.
                        ("forge_ship_ratified", "false"),
                        ("ship_authority_allowed", "false"),
                        ("deployment_authority_granted", "false"),
                        ("production_write_allowed", "false"),
                    ]),
                );
                self.storage.ledger().verify()?;
                Ok(HarnessReviewPacketReport {
                    status: "REVIEW_PACKET_ASSEMBLED",
                    denied_reason: None,
                    packet_receipt_id: receipt.receipt_id.clone(),
                    patch_count,
                    files_changed: facts.summary.files_changed,
                    lines_added: facts.summary.lines_added,
                    lines_removed: facts.summary.lines_removed,
                    test_count,
                    tests_passed: facts.tests_passed,
                    ci_conclusion: facts.ci_conclusion,
                    ci_evidence_url: facts.ci_evidence_url,
                    human_decision,
                    boundary,
                    forge_ship_ratified: false,
                    deployment_authority_granted: false,
                    production_write_allowed: false,
                    receipt_ids: vec![receipt.receipt_id],
                })
            }
        }
    }

    // Read the presence-only facts from the bound receipts. Every receipt must
    // exist and carry the expected kind; the readiness receipt must be a real
    // READY_FOR_HUMAN_RATIFICATION packet. Caller strings never substitute for
    // a ledger receipt.
    fn gather_review_facts(
        &self,
        request: &HarnessReviewPacketRequest<'_>,
    ) -> Result<ReviewFacts, String> {
        if request.worker_run_id.trim().is_empty() {
            return Err("missing_worker_run_id".to_string());
        }
        let plan_hash = request.approved_plan_hash.trim();
        if plan_hash.is_empty() {
            return Err("missing_approved_plan_hash".to_string());
        }

        // The readiness packet must be real and ready - never assemble a review
        // for a change the ship-readiness gate has not cleared.
        let Some(readiness) = self
            .storage
            .ledger()
            .query()
            .by_id(request.readiness_receipt_id.trim())
            .cloned()
        else {
            return Err("missing_readiness_receipt".to_string());
        };
        if readiness.kind != "harness.ship.readiness.assembled" {
            return Err(format!("readiness_wrong_kind:{}", readiness.kind));
        }
        if readiness.payload.get("terminal_state").map(String::as_str)
            != Some("READY_FOR_HUMAN_RATIFICATION")
        {
            return Err("readiness_not_ready".to_string());
        }

        // The worker build, bound to this run and plan.
        let Some(build) = self
            .storage
            .ledger()
            .query()
            .by_id(request.worker_build_receipt_id.trim())
            .cloned()
        else {
            return Err("missing_worker_build_receipt".to_string());
        };
        if build.kind != "worker.build.recorded" {
            return Err(format!("worker_build_wrong_kind:{}", build.kind));
        }
        if build.payload.get("approved_plan_hash").map(String::as_str) != Some(plan_hash) {
            return Err("worker_build_plan_mismatch".to_string());
        }

        // Aggregate the change summary across the patch receipts.
        if request.patch_receipt_ids.is_empty() {
            return Err("missing_patch_evidence".to_string());
        }
        let mut summary = ChangeSummary {
            files_changed: 0,
            lines_added: 0,
            lines_removed: 0,
        };
        for id in request.patch_receipt_ids {
            let Some(receipt) = self.storage.ledger().query().by_id(id.trim()).cloned() else {
                return Err(format!("missing_patch_receipt:{id}"));
            };
            if receipt.kind != "harness.patch.applied" {
                return Err(format!("patch_wrong_kind:{}", receipt.kind));
            }
            if receipt.payload.get("applied").map(String::as_str) != Some("true") {
                return Err(format!("patch_not_applied:{id}"));
            }
            summary.files_changed += payload_u64(&receipt, "files_changed");
            summary.lines_added += payload_u64(&receipt, "insertions");
            summary.lines_removed += payload_u64(&receipt, "deletions");
        }

        // Count the passing tests.
        if request.test_receipt_ids.is_empty() {
            return Err("missing_test_evidence".to_string());
        }
        let mut tests_passed = 0u64;
        for id in request.test_receipt_ids {
            let Some(receipt) = self.storage.ledger().query().by_id(id.trim()).cloned() else {
                return Err(format!("missing_test_receipt:{id}"));
            };
            if receipt.kind != "harness.test.executed" {
                return Err(format!("test_wrong_kind:{}", receipt.kind));
            }
            if receipt.payload.get("passed").map(String::as_str) == Some("true") {
                tests_passed += 1;
            }
        }

        // The observed CI evidence receipt.
        let Some(ci) = self
            .storage
            .ledger()
            .query()
            .by_id(request.ci_evidence_receipt_id.trim())
            .cloned()
        else {
            return Err("missing_ci_evidence_receipt".to_string());
        };
        if ci.kind != "harness.ci.evidence.observed" {
            return Err(format!("ci_evidence_wrong_kind:{}", ci.kind));
        }
        let ci_conclusion = ci.payload.get("conclusion").cloned().unwrap_or_default();
        let ci_evidence_url = ci.payload.get("evidence_url").cloned().unwrap_or_default();

        Ok(ReviewFacts {
            summary,
            tests_passed,
            ci_conclusion,
            ci_evidence_url,
        })
    }
}

struct ReviewFacts {
    summary: ChangeSummary,
    tests_passed: u64,
    ci_conclusion: String,
    ci_evidence_url: String,
}

fn payload_u64(receipt: &Receipt, key: &str) -> u64 {
    receipt
        .payload
        .get(key)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
}
