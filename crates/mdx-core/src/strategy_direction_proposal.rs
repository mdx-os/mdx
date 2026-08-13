//! The Strategy direction proposal: the loop's beginning. Until now the
//! only open question on the strategy surface was a repo fixture - a
//! human could decide, but never say in their own words what the company
//! should do. This records a proposed direction on the receipt chain,
//! with the verified identity, so the decision room has something real
//! to decide.
//!
//! Proposing opens no authority and decides nothing: the proposal waits
//! for human ratification exactly like the fixture did. The kernel mints
//! the proposal id so the chain owns the vocabulary.

use crate::{
    ActionKind, ActorId, CorrelationIds, GovernedWriteIdentity, LoopId, LoopRun, MdxKernel,
    StorageProvider, TenantId, TraceId, WorkflowId, payload,
};

const MAX_DIRECTION_CHARS: usize = 500;
const MAX_WHY_NOW_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct StrategyDirectionProposal<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// The direction in the proposer's own words - one sentence that can
    /// be ratified, held, or sent back for more evidence.
    pub direction: &'a str,
    /// Why this call is on the table now. Optional.
    pub why_now: &'a str,
    /// The card fields (all optional): the problem being answered, where
    /// we play, how we win, a Now/Next/Someday horizon tag, and the ask
    /// (sponsor, success metric, kill condition, funding ask). Recorded
    /// verbatim on the receipt; the board derives readiness from them.
    pub problem: &'a str,
    pub where_we_play: &'a str,
    pub how_we_win: &'a str,
    pub horizon: &'a str,
    pub sponsor: &'a str,
    pub success_metric: &'a str,
    pub kill_condition: &'a str,
    pub funding_ask: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrategyDirectionProposalReport {
    pub proposal_id: String,
    pub proposal_receipt_id: String,
    pub policy_decision_id: String,
    pub terminal_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StrategyDirectionProposalError {
    Missing(&'static str),
    DirectionTooLong(usize),
    WhyNowTooLong(usize),
    UnknownHorizon(String),
}

impl StrategyDirectionProposalError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("strategy direction proposal missing {field}"),
            Self::DirectionTooLong(len) => format!(
                "strategy direction is {len} characters; the limit is {MAX_DIRECTION_CHARS} - \
                 a direction is one decidable sentence, not a document"
            ),
            Self::WhyNowTooLong(len) => format!(
                "strategy direction why_now is {len} characters; the limit is {MAX_WHY_NOW_CHARS}"
            ),
            Self::UnknownHorizon(other) => {
                format!("strategy horizon must be now, next, or someday; got {other}")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_strategy_direction_proposal_local(
        &mut self,
        request: StrategyDirectionProposal<'_>,
    ) -> Result<StrategyDirectionProposalReport, StrategyDirectionProposalError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.save_strategy_direction_proposal_local_with_identity(request, &identity)
    }

    pub fn save_strategy_direction_proposal_local_with_identity(
        &mut self,
        request: StrategyDirectionProposal<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<StrategyDirectionProposalReport, StrategyDirectionProposalError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("direction", request.direction),
        ] {
            if value.trim().is_empty() {
                return Err(StrategyDirectionProposalError::Missing(field));
            }
        }
        if request.direction.chars().count() > MAX_DIRECTION_CHARS {
            return Err(StrategyDirectionProposalError::DirectionTooLong(
                request.direction.chars().count(),
            ));
        }
        if request.why_now.chars().count() > MAX_WHY_NOW_CHARS {
            return Err(StrategyDirectionProposalError::WhyNowTooLong(
                request.why_now.chars().count(),
            ));
        }
        if !request.horizon.is_empty() && !["now", "next", "someday"].contains(&request.horizon) {
            return Err(StrategyDirectionProposalError::UnknownHorizon(
                request.horizon.to_string(),
            ));
        }
        let proposal_id = self.ids.next("strategy_direction_proposal");
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
            self.decide_with_receipt(&correlation, ActionKind::RecordStrategyDirectionProposal);
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_STRATEGY_DIRECTION_PROPOSAL",
            &correlation,
            &decision,
            "strategy.direction.proposed",
            payload(&[
                ("proposal_id", &proposal_id),
                ("direction", request.direction),
                ("why_now", request.why_now),
                ("problem", request.problem),
                ("where_we_play", request.where_we_play),
                ("how_we_win", request.how_we_win),
                ("horizon", request.horizon),
                ("sponsor", request.sponsor),
                ("success_metric", request.success_metric),
                ("kill_condition", request.kill_condition),
                ("funding_ask", request.funding_ask),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("ratification_required", "true"),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(StrategyDirectionProposalReport {
            proposal_id,
            proposal_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            terminal_state: "STRATEGY_DIRECTION_PROPOSED_AWAITING_HUMAN_RATIFICATION",
        })
    }
}
