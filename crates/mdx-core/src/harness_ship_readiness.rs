// Unlock 5: the autonomous loop reaches the human ship door - and stops. It
// observes a REAL CI result (never fabricates green) and assembles a
// ready-for-human-ratification packet that links the whole governed build:
// worker result, patch and worktree receipts, sandbox test receipts, and the
// CI run id, commit, conclusion, and evidence URL. Then it stops.
//
// It deliberately does NOT ratify, authorize deployment, or deploy. Those
// receipt kinds (forge.ship.ratified, forge.deployment.authorized,
// forge.production.deployed) stay forbidden in the autonomous path by
// committed contract: ratification is the human edge, deployment is its own
// later governed chain. The terminal state here is READY_FOR_HUMAN_RATIFICATION,
// never shipped, ratified, or deploy-authorized. Automate evidence; never
// automate judgment.
use crate::*;

/// A real, observed CI result. The autonomous path records these facts; it
/// never assumes or fabricates them (ADR 0138: CI evidence is observed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarnessCiEvidence<'a> {
    pub workflow_run_id: &'a str,
    pub commit_sha: &'a str,
    pub branch: &'a str,
    pub conclusion: &'a str,
    pub evidence_url: &'a str,
    pub observed_at: &'a str,
    /// How this evidence was obtained: "github_actions_adapter" when a real
    /// adapter fetched and verified the run, vs "caller_asserted". The
    /// autonomous path uses only adapter-observed evidence.
    pub observed_via: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HarnessShipReadinessRequest<'a> {
    pub worker_run_id: &'a str,
    pub worker_build_receipt_id: &'a str,
    pub patch_receipt_ids: &'a [&'a str],
    pub test_receipt_ids: &'a [&'a str],
    /// The build's approved plan hash - the key every build receipt binds to.
    pub approved_plan_hash: &'a str,
    /// The commit the build ran against (the server's real workspace HEAD).
    /// CI evidence must bind to this commit and branch, not free-floating text.
    pub build_commit_sha: &'a str,
    pub build_branch: &'a str,
    pub ci_evidence: HarnessCiEvidence<'a>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HarnessShipReadinessReport {
    pub status: &'static str,
    pub denied_reason: Option<String>,
    pub ci_evidence_observed: bool,
    pub ci_evidence_receipt_id: String,
    pub readiness_receipt_id: String,
    pub next_human_step: String,
    // The human edge stays the human edge. These are never granted here.
    pub forge_ship_ratified: bool,
    pub deployment_authority_granted: bool,
    pub production_write_allowed: bool,
    pub receipt_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalHarnessShipReadiness;

impl LocalHarnessShipReadiness {
    pub fn assemble<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessShipReadinessRequest<'_>,
    ) -> Result<HarnessShipReadinessReport, String> {
        kernel.assemble_harness_ship_readiness(manifest, request)
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn assemble_harness_ship_readiness(
        &mut self,
        manifest: &HarnessRunManifest<'_>,
        request: &HarnessShipReadinessRequest<'_>,
    ) -> Result<HarnessShipReadinessReport, String> {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(manifest.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(manifest.actor_id),
            loop_id: LoopId::new("harness_ship_readiness"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("ship_readiness");
        let mut receipt_ids: Vec<String> = Vec::new();

        // The build evidence must exist, and the CI result must be a real,
        // successful observation. Anything missing or red blocks readiness -
        // the loop never presents a red build to the human as ready.
        let denied = self.judge_ship_readiness(request);
        if let Some(reason) = denied {
            let decision =
                self.decide_with_receipt(&correlation, ActionKind::AssembleHarnessShipReadiness);
            let receipt = self.transition_receipt(
                &run_id,
                "SHIP_READINESS_BLOCKED",
                &correlation,
                &decision,
                "harness.ship.readiness.blocked",
                payload(&[
                    ("worker_run_id", request.worker_run_id),
                    ("denied_reason", &reason),
                    ("forge_ship_ratified", "false"),
                    ("deployment_authority_granted", "false"),
                ]),
            );
            self.storage.ledger().verify()?;
            return Ok(HarnessShipReadinessReport {
                status: "SHIP_READINESS_BLOCKED",
                denied_reason: Some(reason),
                ci_evidence_observed: false,
                ci_evidence_receipt_id: String::new(),
                readiness_receipt_id: receipt.receipt_id.clone(),
                next_human_step: "resolve the blocked build evidence before the human ship door"
                    .to_string(),
                forge_ship_ratified: false,
                deployment_authority_granted: false,
                production_write_allowed: false,
                receipt_ids: vec![receipt.receipt_id],
            });
        }

        // 1. Observe the real CI result - facts only, with the evidence URL
        // for audit. This is observation, never a claim of green we made up.
        let ci_decision =
            self.decide_with_receipt(&correlation, ActionKind::ObserveHarnessCiEvidence);
        let ci_receipt = self.transition_receipt(
            &run_id,
            "CI_EVIDENCE_OBSERVED",
            &correlation,
            &ci_decision,
            "harness.ci.evidence.observed",
            payload(&[
                ("worker_run_id", request.worker_run_id),
                ("workflow_run_id", request.ci_evidence.workflow_run_id),
                ("commit_sha", request.ci_evidence.commit_sha),
                ("branch", request.ci_evidence.branch),
                ("conclusion", request.ci_evidence.conclusion),
                ("evidence_url", request.ci_evidence.evidence_url),
                ("observed_at", request.ci_evidence.observed_at),
                ("observed_via", request.ci_evidence.observed_via),
                ("fabricated", "false"),
                ("ci_claim_allowed", "false"),
            ]),
        );
        receipt_ids.push(ci_receipt.receipt_id.clone());

        // 2. Assemble the ready-for-human packet: link every governed receipt
        // so a human sees the full chain at the ship door. Then stop.
        let patch_ids = request.patch_receipt_ids.join(",");
        let test_ids = request.test_receipt_ids.join(",");
        let readiness_decision =
            self.decide_with_receipt(&correlation, ActionKind::AssembleHarnessShipReadiness);
        let readiness = self.transition_receipt(
            &run_id,
            "READY_FOR_HUMAN_RATIFICATION",
            &correlation,
            &readiness_decision,
            "harness.ship.readiness.assembled",
            payload(&[
                ("worker_run_id", request.worker_run_id),
                ("worker_build_receipt_id", request.worker_build_receipt_id),
                ("patch_receipt_ids", &patch_ids),
                ("test_receipt_ids", &test_ids),
                ("ci_evidence_receipt_id", &ci_receipt.receipt_id),
                ("ci_workflow_run_id", request.ci_evidence.workflow_run_id),
                ("ci_commit_sha", request.ci_evidence.commit_sha),
                ("ci_conclusion", request.ci_evidence.conclusion),
                ("terminal_state", "READY_FOR_HUMAN_RATIFICATION"),
                ("next_step", "human_ratification"),
                // The human edge, never crossed by the autonomous path.
                ("forge_ship_ratified", "false"),
                ("ship_authority_allowed", "false"),
                ("deployment_authority_granted", "false"),
                ("production_write_allowed", "false"),
                ("autonomous_path_stops_here", "true"),
            ]),
        );
        receipt_ids.push(readiness.receipt_id.clone());

        self.storage.ledger().verify()?;

        Ok(HarnessShipReadinessReport {
            status: "READY_FOR_HUMAN_RATIFICATION",
            denied_reason: None,
            ci_evidence_observed: true,
            ci_evidence_receipt_id: ci_receipt.receipt_id,
            readiness_receipt_id: readiness.receipt_id,
            next_human_step: "an authorized human ratifies the ship at the Forge decision surface"
                .to_string(),
            forge_ship_ratified: false,
            deployment_authority_granted: false,
            production_write_allowed: false,
            receipt_ids,
        })
    }

    // Readiness binds to REAL ledger receipts, not caller-provided strings.
    // Every receipt must exist, carry the right kind and status, and bind to
    // the same plan, worker, and commit. Anything missing, red, mismatched, or
    // unrelated blocks the human ship door.
    fn judge_ship_readiness(&self, request: &HarnessShipReadinessRequest<'_>) -> Option<String> {
        if request.worker_run_id.trim().is_empty() {
            return Some("missing_worker_run_id".to_string());
        }
        if request.approved_plan_hash.trim().is_empty() {
            return Some("missing_approved_plan_hash".to_string());
        }
        if request.test_receipt_ids.is_empty() {
            return Some("missing_test_evidence".to_string());
        }
        let plan_hash = request.approved_plan_hash.trim();

        // The worker build: a real worker.build.recorded receipt for THIS
        // worker run and plan, with a completed (not failed) status.
        let Some(build) = self
            .storage
            .ledger()
            .query()
            .by_id(request.worker_build_receipt_id.trim())
            .cloned()
        else {
            return Some("missing_worker_build_receipt".to_string());
        };
        if build.kind != "worker.build.recorded" {
            return Some(format!("worker_build_wrong_kind:{}", build.kind));
        }
        if build.payload.get("worker_run_id").map(String::as_str) != Some(request.worker_run_id) {
            return Some("worker_build_run_mismatch".to_string());
        }
        if build.payload.get("approved_plan_hash").map(String::as_str) != Some(plan_hash) {
            return Some("worker_build_plan_mismatch".to_string());
        }
        match build.payload.get("worker_status").map(String::as_str) {
            Some(status) if status.contains("COMPLETED") => {}
            other => {
                return Some(format!(
                    "worker_build_not_completed:{}",
                    other.unwrap_or("")
                ));
            }
        }

        // Patch receipts: real, applied, bound to the same plan.
        for id in request.patch_receipt_ids {
            let Some(receipt) = self.storage.ledger().query().by_id(id.trim()).cloned() else {
                return Some(format!("missing_patch_receipt:{id}"));
            };
            if receipt.kind != "harness.patch.applied" {
                return Some(format!("patch_wrong_kind:{}", receipt.kind));
            }
            if receipt.payload.get("applied").map(String::as_str) != Some("true") {
                return Some(format!("patch_not_applied:{id}"));
            }
            if receipt
                .payload
                .get("approved_plan_hash")
                .map(String::as_str)
                != Some(plan_hash)
            {
                return Some(format!("patch_plan_mismatch:{id}"));
            }
        }

        // Test receipts: real, passed, bound to the same plan.
        for id in request.test_receipt_ids {
            let Some(receipt) = self.storage.ledger().query().by_id(id.trim()).cloned() else {
                return Some(format!("missing_test_receipt:{id}"));
            };
            if receipt.kind != "harness.test.executed" {
                return Some(format!("test_wrong_kind:{}", receipt.kind));
            }
            if receipt.payload.get("passed").map(String::as_str) != Some("true") {
                return Some(format!("test_not_passed:{id}"));
            }
            if receipt
                .payload
                .get("approved_plan_hash")
                .map(String::as_str)
                != Some(plan_hash)
            {
                return Some(format!("test_plan_mismatch:{id}"));
            }
        }

        // CI evidence: present, successful, and bound to the same commit and
        // branch the build ran against - never a free-floating green.
        let ci = &request.ci_evidence;
        for (field, value) in [
            ("workflow_run_id", ci.workflow_run_id),
            ("commit_sha", ci.commit_sha),
            ("conclusion", ci.conclusion),
            ("evidence_url", ci.evidence_url),
        ] {
            if value.trim().is_empty() {
                return Some(format!("missing_ci_field:{field}"));
            }
        }
        if ci.conclusion != "success" {
            return Some(format!("ci_not_successful:{}", ci.conclusion));
        }
        if ci.commit_sha.trim() != request.build_commit_sha.trim() {
            return Some("ci_commit_mismatch".to_string());
        }
        if ci.branch.trim() != request.build_branch.trim() {
            return Some("ci_branch_mismatch".to_string());
        }
        None
    }
}
