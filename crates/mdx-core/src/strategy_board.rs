//! The strategy board's readiness engine. A direction proposal is a card;
//! what moves a card across the board is evidence, not opinion. Three
//! verbs make that real:
//!
//! - state a CONDITION on a proposal: "what would have to be true" as a
//!   short testable claim, optionally flagged make-or-break. Conditions
//!   start as `assumed`.
//! - UPDATE a condition: `being_tested`, `holds`, or `broken`, with an
//!   evidence reference (a page, a signal, a receipt). Testing a
//!   condition is delegable work; judging it stays human.
//! - RESOLVE a proposal: `archived`, `killed`, or `completed`, with what
//!   was learned. Killing a card with reasons is a success outcome, so
//!   the board never silts up with zombies.
//!
//! None of these verbs opens authority. Stage is always derived from the
//! records - never stored, never dragged.

use crate::{
    ActionKind, ActorId, CorrelationIds, GovernedWriteIdentity, LoopId, LoopRun, MdxKernel,
    StorageProvider, TenantId, TraceId, WorkflowId, payload,
};

/// Condition statuses an update may set. A freshly stated condition is
/// `assumed`; only updates move it.
pub const STRATEGY_CONDITION_STATUSES: &[&str] = &["being_tested", "holds", "broken"];

/// How a card may leave the board.
pub const STRATEGY_RESOLUTION_OUTCOMES: &[&str] = &["archived", "killed", "completed"];

const MAX_CLAIM_CHARS: usize = 300;
const MAX_NOTE_CHARS: usize = 1000;
const MAX_LEARNED_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct StrategyCondition<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub proposal_id: &'a str,
    /// The claim, short and testable: "SMB owners will pay for this."
    pub claim: &'a str,
    pub make_or_break: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct StrategyConditionUpdate<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub condition_id: &'a str,
    pub status: &'a str,
    /// What backs the move: a page, a signal, a receipt id. Required for
    /// `holds` and `broken` - a verdict without evidence is an opinion.
    pub evidence_ref: &'a str,
    pub note: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct StrategyProposalResolution<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub proposal_id: &'a str,
    pub outcome: &'a str,
    pub learned: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrategyBoardWriteReport {
    pub record_id: String,
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub terminal_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StrategyBoardError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownStatus(String),
    UnknownOutcome(String),
    EvidenceRequired(String),
}

impl StrategyBoardError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("strategy board write missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("strategy board {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownStatus(other) => format!(
                "condition status must be one of {}; got {other}",
                STRATEGY_CONDITION_STATUSES.join(", ")
            ),
            Self::UnknownOutcome(other) => format!(
                "resolution outcome must be one of {}; got {other}",
                STRATEGY_RESOLUTION_OUTCOMES.join(", ")
            ),
            Self::EvidenceRequired(status) => format!(
                "a condition can only move to {status} with an evidence_ref - \
                 a verdict without evidence is an opinion"
            ),
        }
    }
}

fn check_len(field: &'static str, value: &str, max: usize) -> Result<(), StrategyBoardError> {
    let len = value.chars().count();
    if len > max {
        return Err(StrategyBoardError::TooLong(field, len, max));
    }
    Ok(())
}

/// One board write, described: who, which loop, which action, which
/// transition, which receipt kind. Keeps the shared writer's signature
/// honest as more boards reuse it.
#[derive(Debug)]
pub(crate) struct BoardWrite<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub loop_id: &'a str,
    pub action: ActionKind,
    pub transition: &'a str,
    pub kind: &'a str,
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_strategy_condition_local_with_identity(
        &mut self,
        request: StrategyCondition<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<StrategyBoardWriteReport, StrategyBoardError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("proposal_id", request.proposal_id),
            ("claim", request.claim),
        ] {
            if value.trim().is_empty() {
                return Err(StrategyBoardError::Missing(field));
            }
        }
        check_len("claim", request.claim, MAX_CLAIM_CHARS)?;
        let condition_id = self.ids.next("strategy_condition");
        let make_or_break = if request.make_or_break {
            "true"
        } else {
            "false"
        };
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "strategy_ratification",
                action: ActionKind::RecordStrategyCondition,
                transition: "RECORD_STRATEGY_CONDITION",
                kind: "strategy.condition.stated",
            },
            payload(&[
                ("condition_id", &condition_id),
                ("proposal_id", request.proposal_id),
                ("claim", request.claim),
                ("make_or_break", make_or_break),
                ("status", "assumed"),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(StrategyBoardWriteReport {
            record_id: condition_id,
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            terminal_state: "STRATEGY_CONDITION_STATED_ASSUMED",
        })
    }

    pub fn save_strategy_condition_update_local_with_identity(
        &mut self,
        request: StrategyConditionUpdate<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<StrategyBoardWriteReport, StrategyBoardError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("condition_id", request.condition_id),
            ("status", request.status),
        ] {
            if value.trim().is_empty() {
                return Err(StrategyBoardError::Missing(field));
            }
        }
        if !STRATEGY_CONDITION_STATUSES.contains(&request.status) {
            return Err(StrategyBoardError::UnknownStatus(
                request.status.to_string(),
            ));
        }
        if matches!(request.status, "holds" | "broken") && request.evidence_ref.trim().is_empty() {
            return Err(StrategyBoardError::EvidenceRequired(
                request.status.to_string(),
            ));
        }
        check_len("note", request.note, MAX_NOTE_CHARS)?;
        let update_id = self.ids.next("strategy_condition_update");
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "strategy_ratification",
                action: ActionKind::RecordStrategyConditionUpdate,
                transition: "RECORD_STRATEGY_CONDITION_UPDATE",
                kind: "strategy.condition.updated",
            },
            payload(&[
                ("update_id", &update_id),
                ("condition_id", request.condition_id),
                ("status", request.status),
                ("evidence_ref", request.evidence_ref),
                ("note", request.note),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(StrategyBoardWriteReport {
            record_id: update_id,
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            terminal_state: "STRATEGY_CONDITION_UPDATED",
        })
    }

    pub fn save_strategy_proposal_resolution_local_with_identity(
        &mut self,
        request: StrategyProposalResolution<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<StrategyBoardWriteReport, StrategyBoardError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("proposal_id", request.proposal_id),
            ("outcome", request.outcome),
        ] {
            if value.trim().is_empty() {
                return Err(StrategyBoardError::Missing(field));
            }
        }
        if !STRATEGY_RESOLUTION_OUTCOMES.contains(&request.outcome) {
            return Err(StrategyBoardError::UnknownOutcome(
                request.outcome.to_string(),
            ));
        }
        check_len("learned", request.learned, MAX_LEARNED_CHARS)?;
        let resolution_id = self.ids.next("strategy_resolution");
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "strategy_ratification",
                action: ActionKind::RecordStrategyProposalResolution,
                transition: "RECORD_STRATEGY_PROPOSAL_RESOLUTION",
                kind: "strategy.proposal.resolved",
            },
            payload(&[
                ("resolution_id", &resolution_id),
                ("proposal_id", request.proposal_id),
                ("outcome", request.outcome),
                ("learned", request.learned),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(StrategyBoardWriteReport {
            record_id: resolution_id,
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            terminal_state: "STRATEGY_PROPOSAL_RESOLVED",
        })
    }

    /// The shared spine of every board write - strategy and product
    /// alike: loop run, policy decision, transition receipt. Returns
    /// (receipt_id, policy_decision_id).
    pub(crate) fn record_board_write(
        &mut self,
        write: BoardWrite<'_>,
        body: std::collections::BTreeMap<String, String>,
    ) -> (String, String) {
        let BoardWrite {
            tenant_id,
            actor_id,
            loop_id,
            action,
            transition,
            kind,
        } = write;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(actor_id),
            loop_id: LoopId::new(loop_id),
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
        let decision = self.decide_with_receipt(&correlation, action);
        let receipt =
            self.transition_receipt(&run_id, transition, &correlation, &decision, kind, body);
        (receipt.receipt_id, decision.policy_decision_id)
    }
}
