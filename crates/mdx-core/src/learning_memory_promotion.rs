//! Memory promotion requests for the learning loop. These receipts create a
//! reviewable MDx-owned memory candidate while keeping active memory writes,
//! generated refresh, and runtime adaptation behind later governed routes.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

pub const LEARNING_MEMORY_TARGET_TYPES: &[&str] = &[
    "page",
    "decision_record",
    "generated_contract",
    "source_map",
    "model_scorecard",
    "worker_scorecard",
];

const MAX_TEXT_CHARS: usize = 2000;

/// Optional applicability for a lesson: comma-joined work complexity tiers
/// and language pack ids the lesson applies to, plus a free-text note. Empty
/// fields mean universal - the lesson matches every run, which preserves the
/// behavior of every lesson recorded before these fields existed. Advisory
/// match keys only: they scope read-back selection and never open authority.
#[derive(Clone, Copy, Debug, Default)]
pub struct LearningMemoryApplicability<'a> {
    pub work_tiers: &'a str,
    pub language_packs: &'a str,
    pub notes: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct LearningMemoryPromotion<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub judgment_decision_id: &'a str,
    pub judgment_decision_receipt_id: &'a str,
    pub judgment_id: &'a str,
    pub promotion_id: &'a str,
    pub target_type: &'a str,
    pub target_path: &'a str,
    pub lesson_summary: &'a str,
    pub evidence_refs: &'a str,
    pub review_cadence: &'a str,
    pub expiry_rule: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LearningMemoryPromotionReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub memory_promotion_id: String,
    pub memory_candidate_state: String,
    pub target_type: String,
    pub target_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LearningMemoryPromotionError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownTargetType(String),
    UnknownJudgmentReceipt(String),
    WrongJudgmentReceiptKind(String, String),
}

impl LearningMemoryPromotionError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("request memory promotion: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownTargetType(other) => format!(
                "a learning memory target type must be one of {}; got {other}",
                LEARNING_MEMORY_TARGET_TYPES.join(", ")
            ),
            Self::UnknownJudgmentReceipt(receipt_id) => {
                format!("judgment decision receipt {receipt_id} is unknown")
            }
            Self::WrongJudgmentReceiptKind(receipt_id, kind) => format!(
                "judgment decision receipt {receipt_id} has kind {kind}; expected learning.judgment.decided"
            ),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn request_learning_memory_promotion(
        &mut self,
        request: LearningMemoryPromotion<'_>,
    ) -> Result<LearningMemoryPromotionReport, LearningMemoryPromotionError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.request_learning_memory_promotion_with_identity(request, &identity)
    }

    pub fn request_learning_memory_promotion_with_identity(
        &mut self,
        request: LearningMemoryPromotion<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<LearningMemoryPromotionReport, LearningMemoryPromotionError> {
        self.request_learning_memory_promotion_with_applicability(
            request,
            LearningMemoryApplicability::default(),
            identity,
        )
    }

    pub fn request_learning_memory_promotion_with_applicability(
        &mut self,
        request: LearningMemoryPromotion<'_>,
        applicability: LearningMemoryApplicability<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<LearningMemoryPromotionReport, LearningMemoryPromotionError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("judgment_decision_id", request.judgment_decision_id),
            (
                "judgment_decision_receipt_id",
                request.judgment_decision_receipt_id,
            ),
            ("judgment_id", request.judgment_id),
            ("promotion_id", request.promotion_id),
            ("target_type", request.target_type),
            ("target_path", request.target_path),
            ("lesson_summary", request.lesson_summary),
            ("evidence_refs", request.evidence_refs),
            ("review_cadence", request.review_cadence),
            ("expiry_rule", request.expiry_rule),
        ] {
            if value.trim().is_empty() {
                return Err(LearningMemoryPromotionError::Missing(field));
            }
        }
        let target_type = request.target_type.trim();
        if !LEARNING_MEMORY_TARGET_TYPES.contains(&target_type) {
            return Err(LearningMemoryPromotionError::UnknownTargetType(
                target_type.to_string(),
            ));
        }
        let judgment_decision_receipt_id = request.judgment_decision_receipt_id.trim();
        match self.ledger().query().by_id(judgment_decision_receipt_id) {
            None => {
                return Err(LearningMemoryPromotionError::UnknownJudgmentReceipt(
                    judgment_decision_receipt_id.to_string(),
                ));
            }
            Some(receipt) if receipt.kind != "learning.judgment.decided" => {
                return Err(LearningMemoryPromotionError::WrongJudgmentReceiptKind(
                    judgment_decision_receipt_id.to_string(),
                    receipt.kind.clone(),
                ));
            }
            Some(_) => {}
        }
        for (field, value) in [
            ("target_path", request.target_path),
            ("lesson_summary", request.lesson_summary),
            ("evidence_refs", request.evidence_refs),
            ("review_cadence", request.review_cadence),
            ("expiry_rule", request.expiry_rule),
            ("applicability_work_tiers", applicability.work_tiers),
            ("applicability_language_packs", applicability.language_packs),
            ("applicability_notes", applicability.notes),
        ] {
            let len = value.chars().count();
            if len > MAX_TEXT_CHARS {
                return Err(LearningMemoryPromotionError::TooLong(
                    field,
                    len,
                    MAX_TEXT_CHARS,
                ));
            }
        }
        let memory_promotion_id = format!(
            "learning_memory_promotion_{}",
            self.ledger()
                .query()
                .by_kind("learning.memory.promotion.requested")
                .len()
                + 1
        );
        let memory_candidate_state = "candidate";
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "learning_loop",
                action: ActionKind::RequestLearningMemoryPromotion,
                transition: "REQUEST_LEARNING_MEMORY_PROMOTION",
                kind: "learning.memory.promotion.requested",
            },
            payload(&[
                ("memory_promotion_id", &memory_promotion_id),
                ("judgment_decision_id", request.judgment_decision_id.trim()),
                (
                    "judgment_decision_receipt_id",
                    request.judgment_decision_receipt_id.trim(),
                ),
                ("judgment_id", request.judgment_id.trim()),
                ("promotion_id", request.promotion_id.trim()),
                ("target_type", target_type),
                ("target_path", request.target_path.trim()),
                ("lesson_summary", request.lesson_summary.trim()),
                ("evidence_refs", request.evidence_refs.trim()),
                ("review_cadence", request.review_cadence.trim()),
                ("expiry_rule", request.expiry_rule.trim()),
                // Optional applicability keys: empty means universal, so
                // lessons recorded before these fields ride exactly as before.
                ("applicability_work_tiers", applicability.work_tiers.trim()),
                (
                    "applicability_language_packs",
                    applicability.language_packs.trim(),
                ),
                ("applicability_notes", applicability.notes.trim()),
                ("memory_candidate_state", memory_candidate_state),
                ("identity_source", &identity.identity_source),
                ("active_memory_write_allowed", "false"),
                ("generated_refresh_allowed", "false"),
                ("adaptation_allowed", "false"),
                ("runtime_behavior_change_allowed", "false"),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(LearningMemoryPromotionReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            memory_promotion_id,
            memory_candidate_state: memory_candidate_state.to_string(),
            target_type: target_type.to_string(),
            target_path: request.target_path.trim().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LearningJudgmentDecision;

    fn judgment_receipt_id(kernel: &mut MdxKernel) -> String {
        kernel
            .record_learning_judgment_decision(LearningJudgmentDecision {
                tenant_id: "t",
                actor_id: "human:eng",
                judgment_id: "judgment_1",
                promotion_id: "promotion_1",
                decision: "promote_candidate",
                rationale: "The evidence is enough to queue memory review.",
                evidence_refs: "make learning-loop-check",
            })
            .expect("judgment decision")
            .receipt_id
    }

    #[test]
    fn records_memory_promotion_candidate_without_writing_memory() {
        let mut kernel = MdxKernel::boot_local();
        let judgment_receipt_id = judgment_receipt_id(&mut kernel);
        let report = kernel
            .request_learning_memory_promotion(LearningMemoryPromotion {
                tenant_id: "t",
                actor_id: "human:eng",
                judgment_decision_id: "decision_1",
                judgment_decision_receipt_id: &judgment_receipt_id,
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
        assert_eq!(report.memory_candidate_state, "candidate");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, "learning.memory.promotion.requested");
        assert_eq!(
            receipt
                .payload
                .get("active_memory_write_allowed")
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
    fn records_applicability_scoping_on_the_promotion_receipt() {
        use crate::GovernedWriteIdentity;
        let mut kernel = MdxKernel::boot_local();
        let judgment_receipt_id = judgment_receipt_id(&mut kernel);
        let report = kernel
            .request_learning_memory_promotion_with_applicability(
                LearningMemoryPromotion {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    judgment_decision_id: "decision_1",
                    judgment_decision_receipt_id: &judgment_receipt_id,
                    judgment_id: "judgment_1",
                    promotion_id: "promotion_1",
                    target_type: "decision_record",
                    target_path: "generated/learning/forge-outcome-memory-targets.json",
                    lesson_summary: "Scope this candidate to medium Rust work.",
                    evidence_refs: "make learning-loop-check",
                    review_cadence: "review before activation",
                    expiry_rule: "supersede when stale",
                },
                LearningMemoryApplicability {
                    work_tiers: "medium",
                    language_packs: "rust-cargo",
                    notes: "",
                },
                &GovernedWriteIdentity::local_demo("human:eng"),
            )
            .expect("scoped promotion");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(
            receipt
                .payload
                .get("applicability_work_tiers")
                .map(String::as_str),
            Some("medium")
        );
        assert_eq!(
            receipt
                .payload
                .get("applicability_language_packs")
                .map(String::as_str),
            Some("rust-cargo")
        );
        assert_eq!(
            receipt
                .payload
                .get("applicability_notes")
                .map(String::as_str),
            Some("")
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_unknown_target_type() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.request_learning_memory_promotion(LearningMemoryPromotion {
            tenant_id: "t",
            actor_id: "human:eng",
            judgment_decision_id: "decision_1",
            judgment_decision_receipt_id: "receipt_1",
            judgment_id: "judgment_1",
            promotion_id: "promotion_1",
            target_type: "provider_prompt",
            target_path: "external",
            lesson_summary: "Bad memory target.",
            evidence_refs: "none",
            review_cadence: "review before activation",
            expiry_rule: "supersede when stale",
        });
        assert!(matches!(
            bad,
            Err(LearningMemoryPromotionError::UnknownTargetType(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_judgment_receipt_missing_from_ledger() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.request_learning_memory_promotion(LearningMemoryPromotion {
            tenant_id: "t",
            actor_id: "human:eng",
            judgment_decision_id: "decision_1",
            judgment_decision_receipt_id: "receipt_missing",
            judgment_id: "judgment_1",
            promotion_id: "promotion_1",
            target_type: "decision_record",
            target_path: "generated/learning/forge-outcome-memory-targets.json",
            lesson_summary: "Cited judgment receipt does not exist.",
            evidence_refs: "make learning-loop-check",
            review_cadence: "review before activation",
            expiry_rule: "supersede when stale",
        });
        assert_eq!(
            bad,
            Err(LearningMemoryPromotionError::UnknownJudgmentReceipt(
                "receipt_missing".to_string()
            ))
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_judgment_receipt_with_wrong_kind() {
        let mut kernel = MdxKernel::boot_local();
        let judgment_receipt_id = judgment_receipt_id(&mut kernel);
        let promotion = kernel
            .request_learning_memory_promotion(LearningMemoryPromotion {
                tenant_id: "t",
                actor_id: "human:eng",
                judgment_decision_id: "decision_1",
                judgment_decision_receipt_id: &judgment_receipt_id,
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
        let bad = kernel.request_learning_memory_promotion(LearningMemoryPromotion {
            tenant_id: "t",
            actor_id: "human:eng",
            judgment_decision_id: "decision_2",
            judgment_decision_receipt_id: &promotion.receipt_id,
            judgment_id: "judgment_2",
            promotion_id: "promotion_2",
            target_type: "decision_record",
            target_path: "generated/learning/forge-outcome-memory-targets.json",
            lesson_summary: "Cited receipt exists but is not a judgment decision.",
            evidence_refs: "make learning-loop-check",
            review_cadence: "review before activation",
            expiry_rule: "supersede when stale",
        });
        assert_eq!(
            bad,
            Err(LearningMemoryPromotionError::WrongJudgmentReceiptKind(
                promotion.receipt_id.clone(),
                "learning.memory.promotion.requested".to_string()
            ))
        );
        assert!(kernel.ledger().verify().is_ok());
    }
}
