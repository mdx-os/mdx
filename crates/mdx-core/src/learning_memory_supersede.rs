//! Memory supersession for the learning loop. This is the governed retirement
//! write for active MDx-owned memory: it cites the activation receipt being
//! retired so a bad lesson stops informing later work without deleting its
//! history, while keeping generated refresh, provider memory, prompts,
//! routing, and adaptation blocked.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

const MAX_TEXT_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct LearningMemorySupersede<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub activation_receipt_id: &'a str,
    pub reason: &'a str,
    pub review_owner: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LearningMemorySupersedeReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub memory_supersede_id: String,
    pub activation_receipt_id: String,
    pub active_memory_id: String,
    pub active_memory_state: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LearningMemorySupersedeError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownActivationReceipt(String),
    NotAMemoryActivationReceipt(String, String),
    AlreadySuperseded(String),
}

impl LearningMemorySupersedeError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("supersede learning memory: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownActivationReceipt(receipt_id) => {
                format!("learning memory activation receipt {receipt_id} is unknown")
            }
            Self::NotAMemoryActivationReceipt(receipt_id, kind) => {
                format!("receipt {receipt_id} is {kind}, not learning.memory.activated")
            }
            Self::AlreadySuperseded(receipt_id) => {
                format!("learning memory activation receipt {receipt_id} is already superseded")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn supersede_learning_memory(
        &mut self,
        request: LearningMemorySupersede<'_>,
    ) -> Result<LearningMemorySupersedeReport, LearningMemorySupersedeError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.supersede_learning_memory_with_identity(request, &identity)
    }

    pub fn supersede_learning_memory_with_identity(
        &mut self,
        request: LearningMemorySupersede<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<LearningMemorySupersedeReport, LearningMemorySupersedeError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("activation_receipt_id", request.activation_receipt_id),
            ("reason", request.reason),
            ("review_owner", request.review_owner),
        ] {
            if value.trim().is_empty() {
                return Err(LearningMemorySupersedeError::Missing(field));
            }
        }
        for (field, value) in [
            ("reason", request.reason),
            ("review_owner", request.review_owner),
        ] {
            let len = value.chars().count();
            if len > MAX_TEXT_CHARS {
                return Err(LearningMemorySupersedeError::TooLong(
                    field,
                    len,
                    MAX_TEXT_CHARS,
                ));
            }
        }
        let activation_receipt_id = request.activation_receipt_id.trim();
        let (active_memory_id, lesson_summary) = {
            let Some(activation) = self.ledger().query().by_id(activation_receipt_id) else {
                return Err(LearningMemorySupersedeError::UnknownActivationReceipt(
                    activation_receipt_id.to_string(),
                ));
            };
            if activation.kind != "learning.memory.activated" {
                return Err(LearningMemorySupersedeError::NotAMemoryActivationReceipt(
                    activation_receipt_id.to_string(),
                    activation.kind.clone(),
                ));
            }
            (
                activation
                    .payload
                    .get("active_memory_id")
                    .cloned()
                    .unwrap_or_default(),
                activation
                    .payload
                    .get("lesson_summary")
                    .cloned()
                    .unwrap_or_default(),
            )
        };
        let already_superseded = self
            .ledger()
            .query()
            .by_kind("learning.memory.superseded")
            .iter()
            .any(|receipt| {
                receipt
                    .payload
                    .get("activation_receipt_id")
                    .map(String::as_str)
                    == Some(activation_receipt_id)
            });
        if already_superseded {
            return Err(LearningMemorySupersedeError::AlreadySuperseded(
                activation_receipt_id.to_string(),
            ));
        }
        let memory_supersede_id = format!(
            "learning_memory_supersede_{}",
            self.ledger()
                .query()
                .by_kind("learning.memory.superseded")
                .len()
                + 1
        );
        let active_memory_state = "superseded";
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "learning_loop",
                action: ActionKind::SupersedeLearningMemory,
                transition: "SUPERSEDE_LEARNING_MEMORY",
                kind: "learning.memory.superseded",
            },
            payload(&[
                ("memory_supersede_id", &memory_supersede_id),
                ("activation_receipt_id", activation_receipt_id),
                ("active_memory_id", &active_memory_id),
                ("lesson_summary", &lesson_summary),
                ("reason", request.reason.trim()),
                ("review_owner", request.review_owner.trim()),
                ("active_memory_state", active_memory_state),
                ("identity_source", &identity.identity_source),
                ("memory_superseded", "true"),
                ("receipts_deleted", "false"),
                ("generated_refresh_allowed", "false"),
                ("provider_memory_write_allowed", "false"),
                ("adaptation_allowed", "false"),
                ("runtime_behavior_change_allowed", "false"),
                ("authority_opened", "memory_receipt_only"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(LearningMemorySupersedeReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            memory_supersede_id,
            activation_receipt_id: activation_receipt_id.to_string(),
            active_memory_id,
            active_memory_state: active_memory_state.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LearningJudgmentDecision, LearningMemoryActivation, LearningMemoryPromotion};

    fn activate(kernel: &mut MdxKernel) -> String {
        let judgment = kernel
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
        let promotion = kernel
            .request_learning_memory_promotion(LearningMemoryPromotion {
                tenant_id: "t",
                actor_id: "human:eng",
                judgment_decision_id: &judgment.judgment_decision_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                judgment_id: "judgment_1",
                promotion_id: "promotion_1",
                target_type: "decision_record",
                target_path: "generated/learning/forge-outcome-memory-targets.json",
                lesson_summary: "Queue this lesson as a decision-record memory candidate.",
                evidence_refs: "make learning-loop-check",
                review_cadence: "review before activation",
                expiry_rule: "supersede when stale",
            })
            .expect("memory promotion");
        kernel
            .activate_learning_memory(LearningMemoryActivation {
                tenant_id: "t",
                actor_id: "human:eng",
                memory_promotion_id: "memory_promotion_1",
                memory_promotion_receipt_id: &promotion.receipt_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                target_type: "decision_record",
                target_path: "generated/learning/forge-outcome-memory-targets.json",
                lesson_summary: "Activate this as MDx-owned memory.",
                evidence_refs: "receipt_1, make learning-loop-check",
                activation_basis: "human approval with local proof",
                rollback_plan: "supersede through a later memory receipt",
                local_checks: "make learning-loop-check",
                approval_refs: "approval:local",
                review_owner: "human:eng",
            })
            .expect("memory activation")
            .receipt_id
    }

    #[test]
    fn supersedes_activated_memory_without_adaptation_or_runtime_change() {
        let mut kernel = MdxKernel::boot_local();
        let activation_receipt_id = activate(&mut kernel);
        let report = kernel
            .supersede_learning_memory(LearningMemorySupersede {
                tenant_id: "t",
                actor_id: "human:eng",
                activation_receipt_id: &activation_receipt_id,
                reason: "The lesson misdiagnosed the intent and steered later runs wrong.",
                review_owner: "human:eng",
            })
            .expect("memory supersede");
        assert_eq!(report.active_memory_state, "superseded");
        assert_eq!(report.activation_receipt_id, activation_receipt_id);
        assert_eq!(report.active_memory_id, "learning_active_memory_1");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, "learning.memory.superseded");
        assert_eq!(
            receipt
                .payload
                .get("activation_receipt_id")
                .map(String::as_str),
            Some(activation_receipt_id.as_str())
        );
        assert_eq!(
            receipt.payload.get("memory_superseded").map(String::as_str),
            Some("true")
        );
        assert_eq!(
            receipt
                .payload
                .get("adaptation_allowed")
                .map(String::as_str),
            Some("false")
        );
        assert_eq!(
            receipt
                .payload
                .get("runtime_behavior_change_allowed")
                .map(String::as_str),
            Some("false")
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_unknown_activation_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.supersede_learning_memory(LearningMemorySupersede {
            tenant_id: "t",
            actor_id: "human:eng",
            activation_receipt_id: "receipt_that_does_not_exist",
            reason: "This lesson is stale.",
            review_owner: "human:eng",
        });
        assert!(matches!(
            bad,
            Err(LearningMemorySupersedeError::UnknownActivationReceipt(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_receipt_that_is_not_a_memory_activation() {
        let mut kernel = MdxKernel::boot_local();
        let activation_receipt_id = activate(&mut kernel);
        let supersede_receipt_id = kernel
            .supersede_learning_memory(LearningMemorySupersede {
                tenant_id: "t",
                actor_id: "human:eng",
                activation_receipt_id: &activation_receipt_id,
                reason: "This lesson is stale.",
                review_owner: "human:eng",
            })
            .expect("memory supersede")
            .receipt_id;
        let bad = kernel.supersede_learning_memory(LearningMemorySupersede {
            tenant_id: "t",
            actor_id: "human:eng",
            activation_receipt_id: &supersede_receipt_id,
            reason: "A supersede receipt is not an activation.",
            review_owner: "human:eng",
        });
        assert!(matches!(
            bad,
            Err(LearningMemorySupersedeError::NotAMemoryActivationReceipt(
                _,
                _
            ))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_double_supersede_of_the_same_activation() {
        let mut kernel = MdxKernel::boot_local();
        let activation_receipt_id = activate(&mut kernel);
        kernel
            .supersede_learning_memory(LearningMemorySupersede {
                tenant_id: "t",
                actor_id: "human:eng",
                activation_receipt_id: &activation_receipt_id,
                reason: "The lesson no longer holds.",
                review_owner: "human:eng",
            })
            .expect("first supersede");
        let bad = kernel.supersede_learning_memory(LearningMemorySupersede {
            tenant_id: "t",
            actor_id: "human:eng",
            activation_receipt_id: &activation_receipt_id,
            reason: "Trying to retire it twice.",
            review_owner: "human:eng",
        });
        assert!(matches!(
            bad,
            Err(LearningMemorySupersedeError::AlreadySuperseded(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }
}
