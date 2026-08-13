// Rail Slice 1: the governed human ship-ratification write - the human edge.
//
// This is the ONE place forge.ship.ratified may ever be emitted, and only when
// an authorized HUMAN decides at the ship door with the full evidence chain
// bound: the authorization, the assembled review packet, the ship-readiness
// record, the worker build, and the observed CI evidence, all bound to the same
// build and the same target commit. It is not autonomous, not deployment, and
// not a production write.
//
// The five decisions are first-class. Reject, request_revision, rerun_tests, and
// escalate each record their own receipt and never masquerade as ratification.
// No decision here emits forge.deployment.authorized or forge.production.deployed
// - deployment is a separate governed chain. An autonomous actor, a missing or
// mismatched receipt, a wrong commit, a red or absent CI result, or a stale
// decision all block; a denial is a first-class outcome, not a failure.
use crate::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanShipDecision {
    Ratify,
    Reject,
    RequestRevision,
    RerunTests,
    Escalate,
}

impl HumanShipDecision {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ratify => "ratify",
            Self::Reject => "reject",
            Self::RequestRevision => "request_revision",
            Self::RerunTests => "rerun_tests",
            Self::Escalate => "escalate",
        }
    }

    pub fn parse_decision(value: &str) -> Option<Self> {
        match value.trim() {
            "ratify" => Some(Self::Ratify),
            "reject" => Some(Self::Reject),
            "request_revision" => Some(Self::RequestRevision),
            "rerun_tests" => Some(Self::RerunTests),
            "escalate" => Some(Self::Escalate),
            _ => None,
        }
    }

    // The receipt kind each decision records. Only Ratify is the ship-ratified
    // receipt; the rest are their own first-class kinds.
    fn receipt_kind(self) -> &'static str {
        match self {
            Self::Ratify => "forge.ship.ratified",
            Self::Reject => "forge.ship.rejected",
            Self::RequestRevision => "forge.ship.revision_requested",
            Self::RerunTests => "forge.ship.tests_rerun_requested",
            Self::Escalate => "forge.ship.escalated",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForgeShipDecisionRequest<'a> {
    pub tenant_id: &'a str,
    /// The human making the call, and the attestation that they are human.
    pub human_actor_id: &'a str,
    pub human_actor_kind: &'a str,
    pub decision: HumanShipDecision,
    /// The authority this decision claims; only ship_ratification is handled
    /// here. A deployment/production request is refused - that is another chain.
    pub requested_authority: &'a str,
    pub authorization_receipt_id: &'a str,
    pub review_packet_receipt_id: &'a str,
    pub ship_readiness_receipt_id: &'a str,
    pub worker_build_receipt_id: &'a str,
    pub ci_evidence_receipt_id: &'a str,
    pub target_commit_sha: &'a str,
    pub scope: &'a str,
    pub decision_reason: &'a str,
    pub decided_at: &'a str,
    pub expires_at: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeShipDecisionReport {
    pub status: &'static str,
    pub terminal_state: String,
    pub decision: String,
    pub denied_reason: Option<String>,
    pub decision_receipt_id: String,
    pub policy_decision_id: String,
    pub next_safe_action: String,
    // The human edge. forge_ship_ratified is true only on a passing ratify;
    // deployment and production are NEVER granted here.
    pub forge_ship_ratified: bool,
    pub ship_authority_allowed: bool,
    pub deployment_authority_granted: bool,
    pub production_write_allowed: bool,
    pub receipt_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalForgeShipRatification;

impl LocalForgeShipRatification {
    pub fn decide<S: StorageProvider>(
        &self,
        kernel: &mut MdxKernel<S>,
        request: &ForgeShipDecisionRequest<'_>,
    ) -> Result<ForgeShipDecisionReport, String> {
        kernel.record_forge_human_ship_decision(request)
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    /// The human ship decision from the local fixture identity. Internal and
    /// test callers use this; the HTTP handler uses the identity-aware variant.
    pub fn record_forge_human_ship_decision(
        &mut self,
        request: &ForgeShipDecisionRequest<'_>,
    ) -> Result<ForgeShipDecisionReport, String> {
        let identity = GovernedWriteIdentity::local_demo(request.human_actor_id);
        self.record_forge_human_ship_decision_with_identity(request, &identity)
    }

    /// Record the human ship decision, preserving the established identity in the
    /// decision receipt (the ratify/reject path and the blocked path both).
    pub fn record_forge_human_ship_decision_with_identity(
        &mut self,
        request: &ForgeShipDecisionRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeShipDecisionReport, String> {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.human_actor_id),
            loop_id: LoopId::new("forge_human_ship_decision"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("ship_decision");
        let decision = request.decision;

        // Judge the human-edge requirements. A denial returns a first-class
        // blocked decision receipt - never a ratification.
        if let Some(reason) = self.judge_ship_decision(request) {
            return Ok(self.record_blocked_ship_decision(
                &correlation,
                &run_id,
                request,
                identity,
                reason,
            ));
        }

        let granted_ratify = decision == HumanShipDecision::Ratify;
        let terminal_state = match decision {
            HumanShipDecision::Ratify => "SHIP_RATIFIED",
            HumanShipDecision::Reject => "SHIP_REJECTED",
            HumanShipDecision::RequestRevision => "SHIP_REVISION_REQUESTED",
            HumanShipDecision::RerunTests => "SHIP_TESTS_RERUN_REQUESTED",
            HumanShipDecision::Escalate => "SHIP_ESCALATED",
        };
        let next_safe_action = match decision {
            // Ratification clears the human ship door; deployment is a SEPARATE
            // governed chain and is not performed or authorized here.
            HumanShipDecision::Ratify => {
                "hand the ratified change to the separate governed deployment chain - deployment is not authorized here"
            }
            HumanShipDecision::Reject => {
                "return to the engineer with the reason; the build does not ship"
            }
            HumanShipDecision::RequestRevision => "the engineer revises and re-requests review",
            HumanShipDecision::RerunTests => {
                "re-run the declared tests, then return for a decision"
            }
            HumanShipDecision::Escalate => "a second human reviews before the ship door",
        };

        let policy =
            self.decide_with_receipt(&correlation, ActionKind::RecordForgeHumanShipDecision);
        let receipt = self.transition_receipt(
            &run_id,
            terminal_state,
            &correlation,
            &policy,
            decision.receipt_kind(),
            payload(&[
                ("decision", decision.as_str()),
                ("human_actor_id", request.human_actor_id),
                ("human_actor_kind", request.human_actor_kind),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("authorization_receipt_id", request.authorization_receipt_id),
                ("review_packet_receipt_id", request.review_packet_receipt_id),
                (
                    "ship_readiness_receipt_id",
                    request.ship_readiness_receipt_id,
                ),
                ("worker_build_receipt_id", request.worker_build_receipt_id),
                ("ci_evidence_receipt_id", request.ci_evidence_receipt_id),
                ("target_commit_sha", request.target_commit_sha),
                ("scope", request.scope),
                ("decision_reason", request.decision_reason),
                ("decided_at", request.decided_at),
                ("expires_at", request.expires_at),
                ("terminal_state", terminal_state),
                (
                    "forge_ship_ratified",
                    if granted_ratify { "true" } else { "false" },
                ),
                (
                    "ship_authority_allowed",
                    if granted_ratify { "true" } else { "false" },
                ),
                // Never granted here, on any decision.
                ("deployment_authority_granted", "false"),
                ("production_write_allowed", "false"),
                ("deployment_performed", "false"),
                ("self_ratified", "false"),
            ]),
        );
        self.storage.ledger().verify()?;
        Ok(ForgeShipDecisionReport {
            status: terminal_state,
            terminal_state: terminal_state.to_string(),
            decision: decision.as_str().to_string(),
            denied_reason: None,
            decision_receipt_id: receipt.receipt_id.clone(),
            policy_decision_id: policy.policy_decision_id,
            next_safe_action: next_safe_action.to_string(),
            forge_ship_ratified: granted_ratify,
            ship_authority_allowed: granted_ratify,
            deployment_authority_granted: false,
            production_write_allowed: false,
            receipt_ids: vec![receipt.receipt_id],
        })
    }

    // The human-edge gate. Returns a denial reason, or None when the decision
    // may proceed. Binding integrity (the receipts exist and bind to the same
    // build and commit) is required for every decision; CI success, freshness,
    // and human attestation are additionally required to RATIFY.
    fn judge_ship_decision(&self, request: &ForgeShipDecisionRequest<'_>) -> Option<String> {
        // Required fields.
        for (field, value) in [
            ("human_actor_id", request.human_actor_id),
            ("human_actor_kind", request.human_actor_kind),
            ("authorization_receipt_id", request.authorization_receipt_id),
            ("review_packet_receipt_id", request.review_packet_receipt_id),
            (
                "ship_readiness_receipt_id",
                request.ship_readiness_receipt_id,
            ),
            ("worker_build_receipt_id", request.worker_build_receipt_id),
            ("ci_evidence_receipt_id", request.ci_evidence_receipt_id),
            ("target_commit_sha", request.target_commit_sha),
            ("scope", request.scope),
            ("decision_reason", request.decision_reason),
            ("decided_at", request.decided_at),
        ] {
            if value.trim().is_empty() {
                return Some(format!("missing_{field}"));
            }
        }

        // This door ratifies a ship; it never authorizes deployment or a
        // production write. A request for that authority is refused here.
        let authority = request.requested_authority.trim();
        if !authority.is_empty() && authority != "ship_ratification" {
            return Some(format!("authority_not_granted_here:{authority}"));
        }

        // Binding integrity - required for any decision about this build.
        let review = self.bound_receipt(
            request.review_packet_receipt_id,
            "harness.review.packet.assembled",
            "review_packet",
        );
        let review = match review {
            Ok(receipt) => receipt,
            Err(reason) => return Some(reason),
        };
        let readiness = match self.bound_receipt(
            request.ship_readiness_receipt_id,
            "harness.ship.readiness.assembled",
            "ship_readiness",
        ) {
            Ok(receipt) => receipt,
            Err(reason) => return Some(reason),
        };
        let build = match self.bound_receipt(
            request.worker_build_receipt_id,
            "worker.build.recorded",
            "worker_build",
        ) {
            Ok(receipt) => receipt,
            Err(reason) => return Some(reason),
        };
        let ci = match self.bound_receipt(
            request.ci_evidence_receipt_id,
            "harness.ci.evidence.observed",
            "ci_evidence",
        ) {
            Ok(receipt) => receipt,
            Err(reason) => return Some(reason),
        };
        // The authorization receipt must simply exist (a recorded human approval).
        if self
            .storage
            .ledger()
            .query()
            .by_id(request.authorization_receipt_id.trim())
            .is_none()
        {
            return Some("missing_authorization_receipt".to_string());
        }

        // The chain must bind to one build: the review packet was assembled from
        // this readiness and this build; the readiness names this build and this
        // CI evidence.
        if field(&review, "readiness_receipt_id") != request.ship_readiness_receipt_id.trim() {
            return Some("review_packet_readiness_mismatch".to_string());
        }
        if field(&review, "worker_build_receipt_id") != request.worker_build_receipt_id.trim() {
            return Some("review_packet_build_mismatch".to_string());
        }
        if field(&readiness, "worker_build_receipt_id") != request.worker_build_receipt_id.trim() {
            return Some("readiness_build_mismatch".to_string());
        }
        if field(&readiness, "ci_evidence_receipt_id") != request.ci_evidence_receipt_id.trim() {
            return Some("readiness_ci_evidence_mismatch".to_string());
        }
        // The decision's target commit must match the build's CI commit on both
        // the readiness record and the observed CI evidence.
        let target = request.target_commit_sha.trim();
        if !commit_binds(field(&readiness, "ci_commit_sha"), target) {
            return Some("ship_readiness_commit_mismatch".to_string());
        }
        if !commit_binds(field(&ci, "commit_sha"), target) {
            return Some("ci_evidence_commit_mismatch".to_string());
        }
        let _ = build;

        // The non-ratify decisions (reject, revise, rerun, escalate) may proceed
        // on the bound chain alone - a human can reject a red or unfinished
        // build. Ratification needs more: a human actor, an unexpired decision,
        // and a successful CI result.
        if request.decision == HumanShipDecision::Ratify {
            if request.human_actor_kind.trim() != "human" {
                return Some(format!(
                    "non_human_actor:{}",
                    request.human_actor_kind.trim()
                ));
            }
            if is_autonomous_actor(request.human_actor_id) {
                return Some("autonomous_actor_cannot_ratify".to_string());
            }
            if request.expires_at.trim().is_empty() {
                return Some("missing_expires_at".to_string());
            }
            // String compare is correct for zero-padded ISO-8601 UTC.
            if request.decided_at.trim() >= request.expires_at.trim() {
                return Some("decision_expired".to_string());
            }
            if field(&ci, "conclusion") != "success" {
                return Some(format!("ci_not_successful:{}", field(&ci, "conclusion")));
            }
            if field(&readiness, "terminal_state") != "READY_FOR_HUMAN_RATIFICATION" {
                return Some("ship_readiness_not_ready".to_string());
            }
        }
        None
    }

    // Look up a receipt by id and require its kind. Returns the cloned receipt
    // or a missing/wrong-kind denial reason.
    fn bound_receipt(
        &self,
        receipt_id: &str,
        expected_kind: &str,
        label: &str,
    ) -> Result<Receipt, String> {
        let Some(receipt) = self
            .storage
            .ledger()
            .query()
            .by_id(receipt_id.trim())
            .cloned()
        else {
            return Err(format!("missing_{label}_receipt"));
        };
        if receipt.kind != expected_kind {
            return Err(format!("{label}_wrong_kind:{}", receipt.kind));
        }
        Ok(receipt)
    }

    fn record_blocked_ship_decision(
        &mut self,
        correlation: &CorrelationIds,
        run_id: &str,
        request: &ForgeShipDecisionRequest<'_>,
        identity: &GovernedWriteIdentity,
        reason: String,
    ) -> ForgeShipDecisionReport {
        let policy =
            self.decide_with_receipt(correlation, ActionKind::RecordForgeHumanShipDecision);
        let receipt = self.transition_receipt(
            run_id,
            "SHIP_DECISION_BLOCKED",
            correlation,
            &policy,
            "forge.ship.decision_blocked",
            payload(&[
                ("decision", request.decision.as_str()),
                ("human_actor_id", request.human_actor_id),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("denied_reason", &reason),
                ("forge_ship_ratified", "false"),
                ("ship_authority_allowed", "false"),
                ("deployment_authority_granted", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        let _ = self.storage.ledger().verify();
        ForgeShipDecisionReport {
            status: "SHIP_DECISION_BLOCKED",
            terminal_state: "SHIP_DECISION_BLOCKED".to_string(),
            decision: request.decision.as_str().to_string(),
            denied_reason: Some(reason),
            decision_receipt_id: receipt.receipt_id.clone(),
            policy_decision_id: policy.policy_decision_id,
            next_safe_action: "resolve the blocked evidence or authority, then decide again"
                .to_string(),
            forge_ship_ratified: false,
            ship_authority_allowed: false,
            deployment_authority_granted: false,
            production_write_allowed: false,
            receipt_ids: vec![receipt.receipt_id],
        }
    }
}

fn field<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

const MIN_COMMIT_PREFIX_LEN: usize = 12;

// A target commit may be an abbreviated sha, but only when the human/requested
// target is a long-enough prefix of the observed receipt commit. Short prefixes
// are too weak for a ship-ratification boundary.
fn commit_binds(observed: &str, target: &str) -> bool {
    let observed = observed.trim();
    let target = target.trim();
    if observed.is_empty() || target.is_empty() {
        return false;
    }
    observed == target || (target.len() >= MIN_COMMIT_PREFIX_LEN && observed.starts_with(target))
}

// An actor that looks like a loop, worker, or agent is not a human ratifier.
fn is_autonomous_actor(actor_id: &str) -> bool {
    let actor = actor_id.trim().to_ascii_lowercase();
    actor == "codex"
        || actor == "system"
        || actor == "autonomous"
        || actor.starts_with("agent_")
        || actor.starts_with("worker_")
        || actor.starts_with("loop_")
        || actor.starts_with("autonomous_")
}
