//! Judgment decisions for the learning loop. These receipts make human or
//! policy judgment explicit while keeping memory promotion and adaptation
//! behind later governed writes.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

pub const LEARNING_JUDGMENT_DECISIONS: &[&str] = &[
    "promote_candidate",
    "reject_candidate",
    "supersede_candidate",
];

const MAX_TEXT_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct LearningJudgmentDecision<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub judgment_id: &'a str,
    pub promotion_id: &'a str,
    pub decision: &'a str,
    pub rationale: &'a str,
    pub evidence_refs: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LearningJudgmentDecisionReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub judgment_decision_id: String,
    pub judgment_id: String,
    pub promotion_id: String,
    pub decision: String,
    pub memory_queue_state: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LearningJudgmentDecisionError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownDecision(String),
}

impl LearningJudgmentDecisionError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("record judgment decision: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownDecision(other) => format!(
                "a learning judgment decision must be one of {}; got {other}",
                LEARNING_JUDGMENT_DECISIONS.join(", ")
            ),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_learning_judgment_decision(
        &mut self,
        request: LearningJudgmentDecision<'_>,
    ) -> Result<LearningJudgmentDecisionReport, LearningJudgmentDecisionError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_learning_judgment_decision_with_identity(request, &identity)
    }

    pub fn record_learning_judgment_decision_with_identity(
        &mut self,
        request: LearningJudgmentDecision<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<LearningJudgmentDecisionReport, LearningJudgmentDecisionError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("judgment_id", request.judgment_id),
            ("promotion_id", request.promotion_id),
            ("decision", request.decision),
            ("rationale", request.rationale),
            ("evidence_refs", request.evidence_refs),
        ] {
            if value.trim().is_empty() {
                return Err(LearningJudgmentDecisionError::Missing(field));
            }
        }
        let decision = request.decision.trim();
        if !LEARNING_JUDGMENT_DECISIONS.contains(&decision) {
            return Err(LearningJudgmentDecisionError::UnknownDecision(
                decision.to_string(),
            ));
        }
        for (field, value) in [
            ("rationale", request.rationale),
            ("evidence_refs", request.evidence_refs),
        ] {
            let len = value.chars().count();
            if len > MAX_TEXT_CHARS {
                return Err(LearningJudgmentDecisionError::TooLong(
                    field,
                    len,
                    MAX_TEXT_CHARS,
                ));
            }
        }
        let judgment_decision_id = format!(
            "learning_judgment_decision_{}",
            self.ledger()
                .query()
                .by_kind("learning.judgment.decided")
                .len()
                + 1
        );
        let memory_queue_state = match decision {
            "promote_candidate" => "ready_for_memory",
            "reject_candidate" => "rejected_no_memory",
            "supersede_candidate" => "superseded_no_memory",
            _ => "blocked",
        };
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "learning_loop",
                action: ActionKind::RecordLearningJudgmentDecision,
                transition: "RECORD_LEARNING_JUDGMENT_DECISION",
                kind: "learning.judgment.decided",
            },
            payload(&[
                ("judgment_decision_id", &judgment_decision_id),
                ("judgment_id", request.judgment_id.trim()),
                ("promotion_id", request.promotion_id.trim()),
                ("decision", decision),
                ("rationale", request.rationale.trim()),
                ("evidence_refs", request.evidence_refs.trim()),
                ("memory_queue_state", memory_queue_state),
                ("identity_source", &identity.identity_source),
                ("memory_write_allowed", "false"),
                ("adaptation_allowed", "false"),
                ("runtime_behavior_change_allowed", "false"),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(LearningJudgmentDecisionReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            judgment_decision_id,
            judgment_id: request.judgment_id.trim().to_string(),
            promotion_id: request.promotion_id.trim().to_string(),
            decision: decision.to_string(),
            memory_queue_state: memory_queue_state.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_promote_candidate_without_writing_memory() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .record_learning_judgment_decision(LearningJudgmentDecision {
                tenant_id: "t",
                actor_id: "human:eng",
                judgment_id: "judgment_1",
                promotion_id: "promotion_1",
                decision: "promote_candidate",
                rationale: "The evidence is enough to queue memory review.",
                evidence_refs: "make learning-loop-check",
            })
            .expect("judgment decision");
        assert_eq!(report.memory_queue_state, "ready_for_memory");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, "learning.judgment.decided");
        assert_eq!(
            receipt
                .payload
                .get("memory_write_allowed")
                .map(String::as_str),
            Some("false")
        );
        assert_eq!(
            receipt
                .payload
                .get("adaptation_allowed")
                .map(String::as_str),
            Some("false")
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_unknown_decision() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.record_learning_judgment_decision(LearningJudgmentDecision {
            tenant_id: "t",
            actor_id: "human:eng",
            judgment_id: "judgment_1",
            promotion_id: "promotion_1",
            decision: "write_memory_now",
            rationale: "Bad shortcut.",
            evidence_refs: "none",
        });
        assert!(matches!(
            bad,
            Err(LearningJudgmentDecisionError::UnknownDecision(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }
}
