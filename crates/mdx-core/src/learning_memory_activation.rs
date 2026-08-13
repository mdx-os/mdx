//! Memory activation for the learning loop. This is the first governed learning
//! write that records active MDx-owned memory, while still keeping generated
//! refresh, provider memory, prompts, routing, and adaptation blocked.

use crate::learning_memory_promotion::{LEARNING_MEMORY_TARGET_TYPES, LearningMemoryApplicability};
use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

const MAX_TEXT_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct LearningMemoryActivation<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub memory_promotion_id: &'a str,
    pub memory_promotion_receipt_id: &'a str,
    pub judgment_decision_receipt_id: &'a str,
    pub target_type: &'a str,
    pub target_path: &'a str,
    pub lesson_summary: &'a str,
    pub evidence_refs: &'a str,
    pub activation_basis: &'a str,
    pub rollback_plan: &'a str,
    pub local_checks: &'a str,
    pub approval_refs: &'a str,
    pub review_owner: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LearningMemoryActivationReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub active_memory_id: String,
    pub active_memory_state: String,
    pub target_type: String,
    pub target_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LearningMemoryActivationError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownTargetType(String),
    UnknownPromotionReceipt(String),
    WrongPromotionReceiptKind(String, String),
}

impl LearningMemoryActivationError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("activate learning memory: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownTargetType(other) => format!(
                "a learning memory target type must be one of {}; got {other}",
                LEARNING_MEMORY_TARGET_TYPES.join(", ")
            ),
            Self::UnknownPromotionReceipt(receipt_id) => {
                format!("memory promotion receipt {receipt_id} is unknown")
            }
            Self::WrongPromotionReceiptKind(receipt_id, kind) => format!(
                "memory promotion receipt {receipt_id} has kind {kind}; expected learning.memory.promotion.requested"
            ),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn activate_learning_memory(
        &mut self,
        request: LearningMemoryActivation<'_>,
    ) -> Result<LearningMemoryActivationReport, LearningMemoryActivationError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.activate_learning_memory_with_identity(request, &identity)
    }

    pub fn activate_learning_memory_with_identity(
        &mut self,
        request: LearningMemoryActivation<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<LearningMemoryActivationReport, LearningMemoryActivationError> {
        self.activate_learning_memory_with_applicability(
            request,
            LearningMemoryApplicability::default(),
            identity,
        )
    }

    pub fn activate_learning_memory_with_applicability(
        &mut self,
        request: LearningMemoryActivation<'_>,
        applicability: LearningMemoryApplicability<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<LearningMemoryActivationReport, LearningMemoryActivationError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("memory_promotion_id", request.memory_promotion_id),
            (
                "memory_promotion_receipt_id",
                request.memory_promotion_receipt_id,
            ),
            (
                "judgment_decision_receipt_id",
                request.judgment_decision_receipt_id,
            ),
            ("target_type", request.target_type),
            ("target_path", request.target_path),
            ("lesson_summary", request.lesson_summary),
            ("evidence_refs", request.evidence_refs),
            ("activation_basis", request.activation_basis),
            ("rollback_plan", request.rollback_plan),
            ("local_checks", request.local_checks),
            ("approval_refs", request.approval_refs),
            ("review_owner", request.review_owner),
        ] {
            if value.trim().is_empty() {
                return Err(LearningMemoryActivationError::Missing(field));
            }
        }
        let target_type = request.target_type.trim();
        if !LEARNING_MEMORY_TARGET_TYPES.contains(&target_type) {
            return Err(LearningMemoryActivationError::UnknownTargetType(
                target_type.to_string(),
            ));
        }
        let memory_promotion_receipt_id = request.memory_promotion_receipt_id.trim();
        match self.ledger().query().by_id(memory_promotion_receipt_id) {
            None => {
                return Err(LearningMemoryActivationError::UnknownPromotionReceipt(
                    memory_promotion_receipt_id.to_string(),
                ));
            }
            Some(receipt) if receipt.kind != "learning.memory.promotion.requested" => {
                return Err(LearningMemoryActivationError::WrongPromotionReceiptKind(
                    memory_promotion_receipt_id.to_string(),
                    receipt.kind.clone(),
                ));
            }
            Some(_) => {}
        }
        for (field, value) in [
            ("target_path", request.target_path),
            ("lesson_summary", request.lesson_summary),
            ("evidence_refs", request.evidence_refs),
            ("activation_basis", request.activation_basis),
            ("rollback_plan", request.rollback_plan),
            ("local_checks", request.local_checks),
            ("approval_refs", request.approval_refs),
            ("review_owner", request.review_owner),
            ("applicability_work_tiers", applicability.work_tiers),
            ("applicability_language_packs", applicability.language_packs),
            ("applicability_notes", applicability.notes),
        ] {
            let len = value.chars().count();
            if len > MAX_TEXT_CHARS {
                return Err(LearningMemoryActivationError::TooLong(
                    field,
                    len,
                    MAX_TEXT_CHARS,
                ));
            }
        }
        let active_memory_id = format!(
            "learning_active_memory_{}",
            self.ledger()
                .query()
                .by_kind("learning.memory.activated")
                .len()
                + 1
        );
        let active_memory_state = "active";
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "learning_loop",
                action: ActionKind::ActivateLearningMemory,
                transition: "ACTIVATE_LEARNING_MEMORY",
                kind: "learning.memory.activated",
            },
            payload(&[
                ("active_memory_id", &active_memory_id),
                ("memory_promotion_id", request.memory_promotion_id.trim()),
                (
                    "memory_promotion_receipt_id",
                    request.memory_promotion_receipt_id.trim(),
                ),
                (
                    "judgment_decision_receipt_id",
                    request.judgment_decision_receipt_id.trim(),
                ),
                ("target_type", target_type),
                ("target_path", request.target_path.trim()),
                ("lesson_summary", request.lesson_summary.trim()),
                ("evidence_refs", request.evidence_refs.trim()),
                ("activation_basis", request.activation_basis.trim()),
                ("rollback_plan", request.rollback_plan.trim()),
                ("local_checks", request.local_checks.trim()),
                ("approval_refs", request.approval_refs.trim()),
                ("review_owner", request.review_owner.trim()),
                // Optional applicability keys: empty means universal, so
                // lessons recorded before these fields ride exactly as before.
                ("applicability_work_tiers", applicability.work_tiers.trim()),
                (
                    "applicability_language_packs",
                    applicability.language_packs.trim(),
                ),
                ("applicability_notes", applicability.notes.trim()),
                ("active_memory_state", active_memory_state),
                ("identity_source", &identity.identity_source),
                ("active_memory_written", "true"),
                ("generated_refresh_allowed", "false"),
                ("provider_memory_write_allowed", "false"),
                ("adaptation_allowed", "false"),
                ("runtime_behavior_change_allowed", "false"),
                ("authority_opened", "memory_receipt_only"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(LearningMemoryActivationReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            active_memory_id,
            active_memory_state: active_memory_state.to_string(),
            target_type: target_type.to_string(),
            target_path: request.target_path.trim().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LearningJudgmentDecision, LearningMemoryPromotion};

    fn promotion_chain_receipt_ids(kernel: &mut MdxKernel) -> (String, String) {
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
        (judgment.receipt_id, promotion.receipt_id)
    }

    #[test]
    fn activates_memory_without_adaptation_or_runtime_change() {
        let mut kernel = MdxKernel::boot_local();
        let (judgment_receipt_id, promotion_receipt_id) = promotion_chain_receipt_ids(&mut kernel);
        let report = kernel
            .activate_learning_memory(LearningMemoryActivation {
                tenant_id: "t",
                actor_id: "human:eng",
                memory_promotion_id: "memory_promotion_1",
                memory_promotion_receipt_id: &promotion_receipt_id,
                judgment_decision_receipt_id: &judgment_receipt_id,
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
            .expect("memory activation");
        assert_eq!(report.active_memory_state, "active");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, "learning.memory.activated");
        assert_eq!(
            receipt
                .payload
                .get("active_memory_written")
                .map(String::as_str),
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
    fn records_applicability_scoping_and_defaults_to_universal() {
        use crate::GovernedWriteIdentity;
        let mut kernel = MdxKernel::boot_local();
        let (judgment_receipt_id, promotion_receipt_id) = promotion_chain_receipt_ids(&mut kernel);
        let activation = LearningMemoryActivation {
            tenant_id: "t",
            actor_id: "human:eng",
            memory_promotion_id: "memory_promotion_1",
            memory_promotion_receipt_id: &promotion_receipt_id,
            judgment_decision_receipt_id: &judgment_receipt_id,
            target_type: "decision_record",
            target_path: "generated/learning/forge-outcome-memory-targets.json",
            lesson_summary: "Scope this lesson to medium Rust work.",
            evidence_refs: "receipt_1, make learning-loop-check",
            activation_basis: "human approval with local proof",
            rollback_plan: "supersede through a later memory receipt",
            local_checks: "make learning-loop-check",
            approval_refs: "approval:local",
            review_owner: "human:eng",
        };
        let scoped = kernel
            .activate_learning_memory_with_applicability(
                activation,
                LearningMemoryApplicability {
                    work_tiers: "small, medium",
                    language_packs: "rust-cargo",
                    notes: "Backend Rust slices only.",
                },
                &GovernedWriteIdentity::local_demo("human:eng"),
            )
            .expect("scoped activation");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&scoped.receipt_id)
            .expect("receipt");
        assert_eq!(
            receipt
                .payload
                .get("applicability_work_tiers")
                .map(String::as_str),
            Some("small, medium")
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
            Some("Backend Rust slices only.")
        );
        // The pre-existing path stays universal: empty applicability keys.
        let universal = kernel
            .activate_learning_memory(activation)
            .expect("universal activation");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&universal.receipt_id)
            .expect("receipt");
        assert_eq!(
            receipt
                .payload
                .get("applicability_work_tiers")
                .map(String::as_str),
            Some("")
        );
        assert_eq!(
            receipt
                .payload
                .get("applicability_language_packs")
                .map(String::as_str),
            Some("")
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_provider_prompt_as_active_memory() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.activate_learning_memory(LearningMemoryActivation {
            tenant_id: "t",
            actor_id: "human:eng",
            memory_promotion_id: "memory_promotion_1",
            memory_promotion_receipt_id: "receipt_1",
            judgment_decision_receipt_id: "receipt_0",
            target_type: "provider_prompt",
            target_path: "external",
            lesson_summary: "Bad memory target.",
            evidence_refs: "receipt_1",
            activation_basis: "none",
            rollback_plan: "none",
            local_checks: "make learning-loop-check",
            approval_refs: "approval:local",
            review_owner: "human:eng",
        });
        assert!(matches!(
            bad,
            Err(LearningMemoryActivationError::UnknownTargetType(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_promotion_receipt_missing_from_ledger() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.activate_learning_memory(LearningMemoryActivation {
            tenant_id: "t",
            actor_id: "human:eng",
            memory_promotion_id: "memory_promotion_1",
            memory_promotion_receipt_id: "receipt_missing",
            judgment_decision_receipt_id: "receipt_0",
            target_type: "decision_record",
            target_path: "generated/learning/forge-outcome-memory-targets.json",
            lesson_summary: "Cited promotion receipt does not exist.",
            evidence_refs: "make learning-loop-check",
            activation_basis: "human approval with local proof",
            rollback_plan: "supersede through a later memory receipt",
            local_checks: "make learning-loop-check",
            approval_refs: "approval:local",
            review_owner: "human:eng",
        });
        assert_eq!(
            bad,
            Err(LearningMemoryActivationError::UnknownPromotionReceipt(
                "receipt_missing".to_string()
            ))
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_promotion_receipt_with_wrong_kind() {
        let mut kernel = MdxKernel::boot_local();
        let (judgment_receipt_id, _promotion_receipt_id) = promotion_chain_receipt_ids(&mut kernel);
        let bad = kernel.activate_learning_memory(LearningMemoryActivation {
            tenant_id: "t",
            actor_id: "human:eng",
            memory_promotion_id: "memory_promotion_1",
            memory_promotion_receipt_id: &judgment_receipt_id,
            judgment_decision_receipt_id: &judgment_receipt_id,
            target_type: "decision_record",
            target_path: "generated/learning/forge-outcome-memory-targets.json",
            lesson_summary: "Cited receipt exists but is not a promotion request.",
            evidence_refs: "make learning-loop-check",
            activation_basis: "human approval with local proof",
            rollback_plan: "supersede through a later memory receipt",
            local_checks: "make learning-loop-check",
            approval_refs: "approval:local",
            review_owner: "human:eng",
        });
        assert_eq!(
            bad,
            Err(LearningMemoryActivationError::WrongPromotionReceiptKind(
                judgment_receipt_id.clone(),
                "learning.judgment.decided".to_string()
            ))
        );
        assert!(kernel.ledger().verify().is_ok());
    }
}
