//! The Strategy ratification decision: the first governed verb on the
//! strategy surface. The surface has always presented the call on the table
//! with three options and `ratification_required: true`; this records which
//! option the human chose, on the receipt chain, with the verified identity.
//!
//! Recording a decision opens no authority: direction-setting, budget,
//! worker, Forge handoff, and deploy paths stay blocked exactly as before.
//! The decision is human judgment made durable, not a key that turns.

use crate::{
    ActionKind, ActorId, CorrelationIds, GovernedWriteIdentity, LoopId, LoopRun, MdxKernel,
    StorageProvider, TenantId, TraceId, WorkflowId, payload,
};

/// The three options the strategy surface presents. A decision outside this
/// set is refused: the vocabulary belongs to the surface contract.
pub const STRATEGY_RATIFICATION_DECISIONS: &[&str] = &[
    "hold_current_direction",
    "request_more_evidence",
    "ratify_next_local_strategy_option",
];

const MAX_REASON_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct StrategyRatificationDecision<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub proposal_id: &'a str,
    pub decision: &'a str,
    pub reason: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrategyRatificationDecisionReport {
    pub decision_receipt_id: String,
    pub policy_decision_id: String,
    pub decision: String,
    pub terminal_state: &'static str,
    /// When ratifying an authored direction, the handoff is the receipt:
    /// the product bet this decision seeded, ready for shaping.
    pub product_bet_id: Option<String>,
    pub product_bet_receipt_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StrategyRatificationDecisionError {
    Missing(&'static str),
    UnknownDecision(String),
    ReasonTooLong(usize),
}

impl StrategyRatificationDecisionError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("strategy ratification decision missing {field}"),
            Self::UnknownDecision(other) => format!(
                "strategy ratification decision must be one of {}; got {other}",
                STRATEGY_RATIFICATION_DECISIONS.join(", ")
            ),
            Self::ReasonTooLong(len) => format!(
                "strategy ratification reason is {len} characters; the limit is {MAX_REASON_CHARS}"
            ),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_strategy_ratification_decision_local(
        &mut self,
        request: StrategyRatificationDecision<'_>,
    ) -> Result<StrategyRatificationDecisionReport, StrategyRatificationDecisionError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_strategy_ratification_decision_local_with_identity(request, &identity)
    }

    pub fn save_strategy_ratification_decision_local_with_identity(
        &mut self,
        request: StrategyRatificationDecision<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<StrategyRatificationDecisionReport, StrategyRatificationDecisionError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("proposal_id", request.proposal_id),
            ("decision", request.decision),
        ] {
            if value.trim().is_empty() {
                return Err(StrategyRatificationDecisionError::Missing(field));
            }
        }
        if !STRATEGY_RATIFICATION_DECISIONS.contains(&request.decision) {
            return Err(StrategyRatificationDecisionError::UnknownDecision(
                request.decision.to_string(),
            ));
        }
        if request.reason.chars().count() > MAX_REASON_CHARS {
            return Err(StrategyRatificationDecisionError::ReasonTooLong(
                request.reason.chars().count(),
            ));
        }
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(request.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(request.actor_id),
            loop_id: LoopId::new("strategy_ratification"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordStrategyRatificationDecision);
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_STRATEGY_RATIFICATION_DECISION",
            &correlation,
            &decision,
            "strategy.ratification.decided",
            payload(&[
                ("proposal_id", request.proposal_id),
                ("decision", request.decision),
                ("reason", request.reason),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        // The handoff is the receipt: ratifying an AUTHORED direction
        // seeds its product bet, pre-filled from the proposal's own words
        // and landed on the Radar for shaping. The bet id is derived from
        // the proposal id so re-ratification re-shapes instead of
        // duplicating. The fixture question seeds nothing - only a
        // person's words start product work.
        let mut product_bet_id = None;
        let mut product_bet_receipt_id = None;
        if request.decision == "ratify_next_local_strategy_option" {
            let proposal = self
                .ledger()
                .query()
                .by_kind("strategy.direction.proposed")
                .iter()
                .find(|entry| {
                    entry.payload.get("proposal_id").map(String::as_str)
                        == Some(request.proposal_id)
                })
                .map(|entry| entry.payload.clone());
            if let Some(fields) = proposal {
                let value = |key: &str| fields.get(key).cloned().unwrap_or_default();
                let seeded_id = format!("product_bet_for_{}", request.proposal_id);
                if let Ok(bet) = self.save_product_bet_draft_local_with_identity(
                    crate::ProductBetDraft {
                        tenant_id: request.tenant_id,
                        actor_id: request.actor_id,
                        bet_id: &seeded_id,
                        bet: &value("direction"),
                        for_whom: "",
                        success_metric: &value("success_metric"),
                        kill_condition: &value("kill_condition"),
                        direction_ref: request.proposal_id,
                        slice: "",
                        slice_not: "",
                        signal_ref: &receipt.receipt_id,
                    },
                    identity,
                ) {
                    product_bet_id = Some(bet.record_id);
                    product_bet_receipt_id = Some(bet.receipt_id);
                }
            }
        }
        Ok(StrategyRatificationDecisionReport {
            decision_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            decision: request.decision.to_string(),
            terminal_state: "STRATEGY_RATIFICATION_DECISION_RECORDED_AUTHORITY_UNCHANGED",
            product_bet_id,
            product_bet_receipt_id,
        })
    }
}
