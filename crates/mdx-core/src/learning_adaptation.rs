//! Governed adaptation for the learning loop. This is the first slice where a
//! human-ratified lesson may change runtime behavior: it flips a per-activated
//! memory adaptation flag from its default false so that later fleet plans and
//! builder casting may prefer or avoid the runners the lesson names. It stays
//! narrow on purpose. Only fleet_casting is opened; model routing, budgets,
//! check policy, and review depth stay closed. Every grant cites the upstream
//! learning.memory.activated receipt in the ledger, refuses when that
//! activation is superseded, and refuses a second open grant for the same
//! activation. The rollback verb (supersede) closes a grant so the consuming
//! seams stop honoring it immediately.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

const MAX_TEXT_CHARS: usize = 2000;

/// The single adaptation type this slice opens. Everything else in the
/// adaptation proposal contract stays reserved and blocked.
pub const LEARNING_ADAPTATION_TYPE: &str = "fleet_casting";

/// The memory target types a fleet_casting grant may name. These are the two
/// scorecard target types reserved in LEARNING_MEMORY_TARGET_TYPES; this slice
/// gives them their first meaning.
pub const LEARNING_ADAPTATION_TARGET_TYPES: &[&str] = &["model_scorecard", "worker_scorecard"];

#[derive(Clone, Copy, Debug)]
pub struct LearningAdaptationGrant<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub activation_receipt_id: &'a str,
    pub adaptation_type: &'a str,
    pub target_type: &'a str,
    /// Optional: the builder slot the lesson prefers. When set and configured,
    /// builder casting selects it over automatic evidence (but never over an
    /// explicit per-run human slot). Empty means the grant only informs the
    /// fleet planner's ratified-lessons context, changing nothing by itself.
    pub preferred_builder_slot: &'a str,
    pub reason: &'a str,
    pub review_owner: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LearningAdaptationGrantReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub adaptation_grant_id: String,
    pub activation_receipt_id: String,
    pub active_memory_id: String,
    pub adaptation_type: String,
    pub target_type: String,
    pub preferred_builder_slot: String,
    pub grant_state: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LearningAdaptationGrantError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownAdaptationType(String),
    UnknownTargetType(String),
    UnknownActivationReceipt(String),
    NotAMemoryActivationReceipt(String, String),
    ActivationSuperseded(String),
    AlreadyGranted(String),
}

impl LearningAdaptationGrantError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("grant learning adaptation: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownAdaptationType(other) => format!(
                "a learning adaptation type must be {LEARNING_ADAPTATION_TYPE}; got {other}"
            ),
            Self::UnknownTargetType(other) => format!(
                "a fleet casting grant target type must be one of {}; got {other}",
                LEARNING_ADAPTATION_TARGET_TYPES.join(", ")
            ),
            Self::UnknownActivationReceipt(receipt_id) => {
                format!("learning memory activation receipt {receipt_id} is unknown")
            }
            Self::NotAMemoryActivationReceipt(receipt_id, kind) => {
                format!("receipt {receipt_id} is {kind}, not learning.memory.activated")
            }
            Self::ActivationSuperseded(receipt_id) => format!(
                "learning memory activation receipt {receipt_id} is superseded; retire is not a casting grant"
            ),
            Self::AlreadyGranted(receipt_id) => format!(
                "learning memory activation receipt {receipt_id} already steers casting; withdraw it before granting again"
            ),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LearningAdaptationSupersede<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub grant_receipt_id: &'a str,
    pub reason: &'a str,
    pub review_owner: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LearningAdaptationSupersedeReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub adaptation_supersede_id: String,
    pub grant_receipt_id: String,
    pub activation_receipt_id: String,
    pub grant_state: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LearningAdaptationSupersedeError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownGrantReceipt(String),
    NotAnAdaptationGrantReceipt(String, String),
    AlreadySuperseded(String),
}

impl LearningAdaptationSupersedeError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("withdraw learning adaptation: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownGrantReceipt(receipt_id) => {
                format!("learning adaptation grant receipt {receipt_id} is unknown")
            }
            Self::NotAnAdaptationGrantReceipt(receipt_id, kind) => {
                format!("receipt {receipt_id} is {kind}, not learning.adaptation.granted")
            }
            Self::AlreadySuperseded(receipt_id) => {
                format!("learning adaptation grant receipt {receipt_id} is already withdrawn")
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LearningAdaptationApplied<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub grant_receipt_id: &'a str,
    /// What kind of artifact the grant influenced: "fleet_plan" or
    /// "builder_slot".
    pub artifact_kind: &'a str,
    /// The concrete artifact id: a fleet plan id, or a builder slot decision.
    pub artifact_id: &'a str,
    pub detail: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LearningAdaptationAppliedReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub adaptation_applied_id: String,
    pub grant_receipt_id: String,
    pub artifact_kind: String,
    pub artifact_id: String,
}

/// One open fleet_casting grant, read at decision time by the consuming seams.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveAdaptationGrant {
    pub grant_receipt_id: String,
    pub activation_receipt_id: String,
    pub active_memory_id: String,
    pub lesson_summary: String,
    pub target_type: String,
    pub preferred_builder_slot: String,
    pub review_owner: String,
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn grant_learning_adaptation(
        &mut self,
        request: LearningAdaptationGrant<'_>,
    ) -> Result<LearningAdaptationGrantReport, LearningAdaptationGrantError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.grant_learning_adaptation_with_identity(request, &identity)
    }

    pub fn grant_learning_adaptation_with_identity(
        &mut self,
        request: LearningAdaptationGrant<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<LearningAdaptationGrantReport, LearningAdaptationGrantError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("activation_receipt_id", request.activation_receipt_id),
            ("adaptation_type", request.adaptation_type),
            ("target_type", request.target_type),
            ("reason", request.reason),
            ("review_owner", request.review_owner),
        ] {
            if value.trim().is_empty() {
                return Err(LearningAdaptationGrantError::Missing(field));
            }
        }
        for (field, value) in [
            ("reason", request.reason),
            ("review_owner", request.review_owner),
            ("preferred_builder_slot", request.preferred_builder_slot),
        ] {
            let len = value.chars().count();
            if len > MAX_TEXT_CHARS {
                return Err(LearningAdaptationGrantError::TooLong(
                    field,
                    len,
                    MAX_TEXT_CHARS,
                ));
            }
        }
        let adaptation_type = request.adaptation_type.trim();
        if adaptation_type != LEARNING_ADAPTATION_TYPE {
            return Err(LearningAdaptationGrantError::UnknownAdaptationType(
                adaptation_type.to_string(),
            ));
        }
        let target_type = request.target_type.trim();
        if !LEARNING_ADAPTATION_TARGET_TYPES.contains(&target_type) {
            return Err(LearningAdaptationGrantError::UnknownTargetType(
                target_type.to_string(),
            ));
        }
        let activation_receipt_id = request.activation_receipt_id.trim();
        // Ledger referential integrity (the Arc 0 pattern, non-negotiable):
        // the cited activation must exist AND carry the right kind.
        let (active_memory_id, lesson_summary) = {
            let Some(activation) = self.ledger().query().by_id(activation_receipt_id) else {
                return Err(LearningAdaptationGrantError::UnknownActivationReceipt(
                    activation_receipt_id.to_string(),
                ));
            };
            if activation.kind != "learning.memory.activated" {
                return Err(LearningAdaptationGrantError::NotAMemoryActivationReceipt(
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
        // A superseded lesson is retired: it cannot steer casting.
        let activation_superseded = self
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
        if activation_superseded {
            return Err(LearningAdaptationGrantError::ActivationSuperseded(
                activation_receipt_id.to_string(),
            ));
        }
        // Refuse a second OPEN grant for the same activation. A grant that was
        // later withdrawn (superseded) is closed and does not block re-granting.
        let withdrawn_grants = self.withdrawn_grant_receipt_ids();
        let already_open = self
            .ledger()
            .query()
            .by_kind("learning.adaptation.granted")
            .iter()
            .any(|receipt| {
                receipt
                    .payload
                    .get("activation_receipt_id")
                    .map(String::as_str)
                    == Some(activation_receipt_id)
                    && !withdrawn_grants.contains(&receipt.receipt_id)
            });
        if already_open {
            return Err(LearningAdaptationGrantError::AlreadyGranted(
                activation_receipt_id.to_string(),
            ));
        }
        let adaptation_grant_id = format!(
            "learning_adaptation_grant_{}",
            self.ledger()
                .query()
                .by_kind("learning.adaptation.granted")
                .len()
                + 1
        );
        let grant_state = "granted";
        let preferred_builder_slot = request.preferred_builder_slot.trim();
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "learning_loop",
                action: ActionKind::GrantLearningAdaptation,
                transition: "GRANT_LEARNING_ADAPTATION",
                kind: "learning.adaptation.granted",
            },
            payload(&[
                ("adaptation_grant_id", &adaptation_grant_id),
                ("activation_receipt_id", activation_receipt_id),
                ("active_memory_id", &active_memory_id),
                ("lesson_summary", &lesson_summary),
                ("adaptation_type", adaptation_type),
                ("target_type", target_type),
                ("preferred_builder_slot", preferred_builder_slot),
                ("reason", request.reason.trim()),
                ("review_owner", request.review_owner.trim()),
                ("grant_state", grant_state),
                ("identity_source", &identity.identity_source),
                // The per-memory flag flip: this activation may now steer
                // casting, and only casting.
                ("adaptation_allowed", "true"),
                ("adaptation_scope", LEARNING_ADAPTATION_TYPE),
                ("fleet_casting_change_allowed", "true"),
                ("model_routing_change_allowed", "false"),
                ("budget_change_allowed", "false"),
                ("check_policy_change_allowed", "false"),
                ("review_depth_change_allowed", "false"),
                ("reversible", "true"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(LearningAdaptationGrantReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            adaptation_grant_id,
            activation_receipt_id: activation_receipt_id.to_string(),
            active_memory_id,
            adaptation_type: adaptation_type.to_string(),
            target_type: target_type.to_string(),
            preferred_builder_slot: preferred_builder_slot.to_string(),
            grant_state: grant_state.to_string(),
        })
    }

    pub fn supersede_learning_adaptation(
        &mut self,
        request: LearningAdaptationSupersede<'_>,
    ) -> Result<LearningAdaptationSupersedeReport, LearningAdaptationSupersedeError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.supersede_learning_adaptation_with_identity(request, &identity)
    }

    pub fn supersede_learning_adaptation_with_identity(
        &mut self,
        request: LearningAdaptationSupersede<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<LearningAdaptationSupersedeReport, LearningAdaptationSupersedeError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("grant_receipt_id", request.grant_receipt_id),
            ("reason", request.reason),
            ("review_owner", request.review_owner),
        ] {
            if value.trim().is_empty() {
                return Err(LearningAdaptationSupersedeError::Missing(field));
            }
        }
        for (field, value) in [
            ("reason", request.reason),
            ("review_owner", request.review_owner),
        ] {
            let len = value.chars().count();
            if len > MAX_TEXT_CHARS {
                return Err(LearningAdaptationSupersedeError::TooLong(
                    field,
                    len,
                    MAX_TEXT_CHARS,
                ));
            }
        }
        let grant_receipt_id = request.grant_receipt_id.trim();
        let (activation_receipt_id, active_memory_id, lesson_summary) = {
            let Some(grant) = self.ledger().query().by_id(grant_receipt_id) else {
                return Err(LearningAdaptationSupersedeError::UnknownGrantReceipt(
                    grant_receipt_id.to_string(),
                ));
            };
            if grant.kind != "learning.adaptation.granted" {
                return Err(
                    LearningAdaptationSupersedeError::NotAnAdaptationGrantReceipt(
                        grant_receipt_id.to_string(),
                        grant.kind.clone(),
                    ),
                );
            }
            (
                grant
                    .payload
                    .get("activation_receipt_id")
                    .cloned()
                    .unwrap_or_default(),
                grant
                    .payload
                    .get("active_memory_id")
                    .cloned()
                    .unwrap_or_default(),
                grant
                    .payload
                    .get("lesson_summary")
                    .cloned()
                    .unwrap_or_default(),
            )
        };
        if self
            .withdrawn_grant_receipt_ids()
            .contains(grant_receipt_id)
        {
            return Err(LearningAdaptationSupersedeError::AlreadySuperseded(
                grant_receipt_id.to_string(),
            ));
        }
        let adaptation_supersede_id = format!(
            "learning_adaptation_supersede_{}",
            self.ledger()
                .query()
                .by_kind("learning.adaptation.superseded")
                .len()
                + 1
        );
        let grant_state = "withdrawn";
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "learning_loop",
                action: ActionKind::SupersedeLearningAdaptation,
                transition: "SUPERSEDE_LEARNING_ADAPTATION",
                kind: "learning.adaptation.superseded",
            },
            payload(&[
                ("adaptation_supersede_id", &adaptation_supersede_id),
                ("grant_receipt_id", grant_receipt_id),
                ("activation_receipt_id", &activation_receipt_id),
                ("active_memory_id", &active_memory_id),
                ("lesson_summary", &lesson_summary),
                ("reason", request.reason.trim()),
                ("review_owner", request.review_owner.trim()),
                ("grant_state", grant_state),
                ("identity_source", &identity.identity_source),
                ("adaptation_allowed", "false"),
                ("adaptation_withdrawn", "true"),
                ("receipts_deleted", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(LearningAdaptationSupersedeReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            adaptation_supersede_id,
            grant_receipt_id: grant_receipt_id.to_string(),
            activation_receipt_id,
            grant_state: grant_state.to_string(),
        })
    }

    /// Record that an open grant actually influenced a casting decision. The
    /// consuming seams (fleet planner, builder casting) call this at decision
    /// time, citing the grant receipt and naming the artifact changed. This is
    /// receipt-only bookkeeping; it changes nothing on its own.
    pub fn record_learning_adaptation_applied(
        &mut self,
        request: LearningAdaptationApplied<'_>,
        identity: &GovernedWriteIdentity,
    ) -> LearningAdaptationAppliedReport {
        let adaptation_applied_id = format!(
            "learning_adaptation_applied_{}",
            self.ledger()
                .query()
                .by_kind("learning.adaptation.applied")
                .len()
                + 1
        );
        let activation_receipt_id = self
            .ledger()
            .query()
            .by_id(request.grant_receipt_id.trim())
            .and_then(|grant| grant.payload.get("activation_receipt_id").cloned())
            .unwrap_or_default();
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "learning_loop",
                action: ActionKind::ApplyLearningAdaptation,
                transition: "APPLY_LEARNING_ADAPTATION",
                kind: "learning.adaptation.applied",
            },
            payload(&[
                ("adaptation_applied_id", &adaptation_applied_id),
                ("grant_receipt_id", request.grant_receipt_id.trim()),
                ("activation_receipt_id", &activation_receipt_id),
                ("adaptation_type", LEARNING_ADAPTATION_TYPE),
                ("artifact_kind", request.artifact_kind.trim()),
                ("artifact_id", request.artifact_id.trim()),
                ("detail", request.detail.trim()),
                ("identity_source", &identity.identity_source),
                ("production_write_allowed", "false"),
            ]),
        );
        LearningAdaptationAppliedReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            adaptation_applied_id,
            grant_receipt_id: request.grant_receipt_id.trim().to_string(),
            artifact_kind: request.artifact_kind.trim().to_string(),
            artifact_id: request.artifact_id.trim().to_string(),
        }
    }

    /// The grant receipt ids that a later learning.adaptation.superseded
    /// receipt has withdrawn. Consumers skip these.
    pub fn withdrawn_grant_receipt_ids(&self) -> std::collections::BTreeSet<String> {
        self.ledger()
            .query()
            .by_kind("learning.adaptation.superseded")
            .iter()
            .filter_map(|receipt| receipt.payload.get("grant_receipt_id").cloned())
            .filter(|id| !id.is_empty())
            .collect()
    }

    /// Every open fleet_casting grant, read at decision time. A grant is open
    /// when it is not withdrawn and its underlying activation is not
    /// superseded. This is the single reader both consuming seams use.
    pub fn active_fleet_casting_grants(&self) -> Vec<ActiveAdaptationGrant> {
        let withdrawn = self.withdrawn_grant_receipt_ids();
        let superseded_activations: std::collections::BTreeSet<String> = self
            .ledger()
            .query()
            .by_kind("learning.memory.superseded")
            .iter()
            .filter_map(|receipt| receipt.payload.get("activation_receipt_id").cloned())
            .filter(|id| !id.is_empty())
            .collect();
        let mut grants = Vec::new();
        for receipt in self
            .ledger()
            .query()
            .by_kind("learning.adaptation.granted")
            .iter()
        {
            if withdrawn.contains(&receipt.receipt_id) {
                continue;
            }
            let get = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
            if get("adaptation_type") != LEARNING_ADAPTATION_TYPE {
                continue;
            }
            let activation_receipt_id = get("activation_receipt_id");
            if superseded_activations.contains(&activation_receipt_id) {
                continue;
            }
            grants.push(ActiveAdaptationGrant {
                grant_receipt_id: receipt.receipt_id.clone(),
                activation_receipt_id,
                active_memory_id: get("active_memory_id"),
                lesson_summary: get("lesson_summary"),
                target_type: get("target_type"),
                preferred_builder_slot: get("preferred_builder_slot"),
                review_owner: get("review_owner"),
            });
        }
        grants
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LearningJudgmentDecision, LearningMemoryActivation, LearningMemoryPromotion,
        LearningMemorySupersede,
    };

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
                target_type: "model_scorecard",
                target_path: "generated/learning/model-worker-scorecard-targets.json",
                lesson_summary: "Prefer the sovereign builder for sensitive Rust work.",
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
                target_type: "model_scorecard",
                target_path: "generated/learning/model-worker-scorecard-targets.json",
                lesson_summary: "Prefer the sovereign builder for sensitive Rust work.",
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

    fn grant(activation_receipt_id: &str) -> LearningAdaptationGrant<'_> {
        LearningAdaptationGrant {
            tenant_id: "t",
            actor_id: "human:eng",
            activation_receipt_id,
            adaptation_type: "fleet_casting",
            target_type: "model_scorecard",
            preferred_builder_slot: "OPUS",
            reason: "This lesson should prefer the sovereign builder on sensitive work.",
            review_owner: "human:eng",
        }
    }

    #[test]
    fn grants_open_a_single_fleet_casting_adaptation() {
        let mut kernel = MdxKernel::boot_local();
        let activation_receipt_id = activate(&mut kernel);
        let request = grant(&activation_receipt_id);
        let report = kernel
            .grant_learning_adaptation(request)
            .expect("adaptation grant");
        assert_eq!(report.adaptation_type, "fleet_casting");
        assert_eq!(report.grant_state, "granted");
        assert_eq!(report.preferred_builder_slot, "OPUS");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, "learning.adaptation.granted");
        assert_eq!(
            receipt
                .payload
                .get("adaptation_allowed")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            receipt
                .payload
                .get("model_routing_change_allowed")
                .map(String::as_str),
            Some("false")
        );
        let grants = kernel.active_fleet_casting_grants();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].preferred_builder_slot, "OPUS");
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn grant_refuses_unknown_activation() {
        let mut kernel = MdxKernel::boot_local();
        let mut request = grant("receipt_missing");
        request.activation_receipt_id = "receipt_missing";
        let bad = kernel.grant_learning_adaptation(request);
        assert!(matches!(
            bad,
            Err(LearningAdaptationGrantError::UnknownActivationReceipt(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn grant_refuses_receipt_that_is_not_an_activation() {
        let mut kernel = MdxKernel::boot_local();
        let activation_receipt_id = activate(&mut kernel);
        // A grant receipt is not an activation.
        let grant_receipt_id = kernel
            .grant_learning_adaptation(grant(&activation_receipt_id))
            .expect("grant")
            .receipt_id;
        let mut request = grant(&grant_receipt_id);
        request.activation_receipt_id = &grant_receipt_id;
        let bad = kernel.grant_learning_adaptation(request);
        assert!(matches!(
            bad,
            Err(LearningAdaptationGrantError::NotAMemoryActivationReceipt(
                _,
                _
            ))
        ));
    }

    #[test]
    fn grant_refuses_wrong_adaptation_type_and_target_type() {
        let mut kernel = MdxKernel::boot_local();
        let activation_receipt_id = activate(&mut kernel);
        let mut wrong_type = grant(&activation_receipt_id);
        wrong_type.adaptation_type = "model_routing";
        assert!(matches!(
            kernel.grant_learning_adaptation(wrong_type),
            Err(LearningAdaptationGrantError::UnknownAdaptationType(_))
        ));
        let mut wrong_target = grant(&activation_receipt_id);
        wrong_target.target_type = "decision_record";
        assert!(matches!(
            kernel.grant_learning_adaptation(wrong_target),
            Err(LearningAdaptationGrantError::UnknownTargetType(_))
        ));
    }

    #[test]
    fn grant_refuses_superseded_activation() {
        let mut kernel = MdxKernel::boot_local();
        let activation_receipt_id = activate(&mut kernel);
        kernel
            .supersede_learning_memory(LearningMemorySupersede {
                tenant_id: "t",
                actor_id: "human:eng",
                activation_receipt_id: &activation_receipt_id,
                reason: "The lesson stopped holding.",
                review_owner: "human:eng",
            })
            .expect("supersede");
        let request = grant(&activation_receipt_id);
        assert!(matches!(
            kernel.grant_learning_adaptation(request),
            Err(LearningAdaptationGrantError::ActivationSuperseded(_))
        ));
    }

    #[test]
    fn grant_refuses_a_second_open_grant_for_the_same_activation() {
        let mut kernel = MdxKernel::boot_local();
        let activation_receipt_id = activate(&mut kernel);
        kernel
            .grant_learning_adaptation(grant(&activation_receipt_id))
            .expect("first grant");
        assert!(matches!(
            kernel.grant_learning_adaptation(grant(&activation_receipt_id)),
            Err(LearningAdaptationGrantError::AlreadyGranted(_))
        ));
    }

    #[test]
    fn rollback_closes_the_grant_and_consumers_stop_honoring_it() {
        let mut kernel = MdxKernel::boot_local();
        let activation_receipt_id = activate(&mut kernel);
        let grant_receipt_id = kernel
            .grant_learning_adaptation(grant(&activation_receipt_id))
            .expect("grant")
            .receipt_id;
        assert_eq!(kernel.active_fleet_casting_grants().len(), 1);
        let rollback = kernel
            .supersede_learning_adaptation(LearningAdaptationSupersede {
                tenant_id: "t",
                actor_id: "human:eng",
                grant_receipt_id: &grant_receipt_id,
                reason: "The lesson was wrong; stop steering casting.",
                review_owner: "human:eng",
            })
            .expect("rollback");
        assert_eq!(rollback.grant_state, "withdrawn");
        assert!(kernel.active_fleet_casting_grants().is_empty());
        // After withdrawal a fresh grant for the same activation is allowed.
        kernel
            .grant_learning_adaptation(grant(&activation_receipt_id))
            .expect("re-grant after withdrawal");
        assert_eq!(kernel.active_fleet_casting_grants().len(), 1);
    }

    #[test]
    fn rollback_refuses_unknown_grant_wrong_kind_and_double_withdrawal() {
        let mut kernel = MdxKernel::boot_local();
        let activation_receipt_id = activate(&mut kernel);
        let unknown = kernel.supersede_learning_adaptation(LearningAdaptationSupersede {
            tenant_id: "t",
            actor_id: "human:eng",
            grant_receipt_id: "receipt_missing",
            reason: "Nothing to withdraw.",
            review_owner: "human:eng",
        });
        assert!(matches!(
            unknown,
            Err(LearningAdaptationSupersedeError::UnknownGrantReceipt(_))
        ));
        let wrong_kind = kernel.supersede_learning_adaptation(LearningAdaptationSupersede {
            tenant_id: "t",
            actor_id: "human:eng",
            grant_receipt_id: &activation_receipt_id,
            reason: "An activation is not a grant.",
            review_owner: "human:eng",
        });
        assert!(matches!(
            wrong_kind,
            Err(LearningAdaptationSupersedeError::NotAnAdaptationGrantReceipt(_, _))
        ));
        let grant_receipt_id = kernel
            .grant_learning_adaptation(grant(&activation_receipt_id))
            .expect("grant")
            .receipt_id;
        kernel
            .supersede_learning_adaptation(LearningAdaptationSupersede {
                tenant_id: "t",
                actor_id: "human:eng",
                grant_receipt_id: &grant_receipt_id,
                reason: "First withdrawal.",
                review_owner: "human:eng",
            })
            .expect("first withdrawal");
        let double = kernel.supersede_learning_adaptation(LearningAdaptationSupersede {
            tenant_id: "t",
            actor_id: "human:eng",
            grant_receipt_id: &grant_receipt_id,
            reason: "Trying to withdraw twice.",
            review_owner: "human:eng",
        });
        assert!(matches!(
            double,
            Err(LearningAdaptationSupersedeError::AlreadySuperseded(_))
        ));
    }

    #[test]
    fn applied_receipt_cites_the_grant_and_artifact() {
        let mut kernel = MdxKernel::boot_local();
        let activation_receipt_id = activate(&mut kernel);
        let grant_receipt_id = kernel
            .grant_learning_adaptation(grant(&activation_receipt_id))
            .expect("grant")
            .receipt_id;
        let applied = kernel.record_learning_adaptation_applied(
            LearningAdaptationApplied {
                tenant_id: "t",
                actor_id: "human:eng",
                grant_receipt_id: &grant_receipt_id,
                artifact_kind: "fleet_plan",
                artifact_id: "fleet_alpha",
                detail: "ratified lesson informed the plan",
            },
            &GovernedWriteIdentity::local_demo("human:eng"),
        );
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&applied.receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, "learning.adaptation.applied");
        assert_eq!(
            receipt.payload.get("grant_receipt_id").map(String::as_str),
            Some(grant_receipt_id.as_str())
        );
        assert_eq!(
            receipt.payload.get("artifact_id").map(String::as_str),
            Some("fleet_alpha")
        );
        assert_eq!(
            receipt
                .payload
                .get("activation_receipt_id")
                .map(String::as_str),
            Some(activation_receipt_id.as_str())
        );
        assert!(kernel.ledger().verify().is_ok());
    }
}
