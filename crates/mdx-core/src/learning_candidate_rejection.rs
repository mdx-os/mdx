//! Candidate rejection for the learning loop. Rejection is signal, not
//! silence: this governed write records the accountable no on a drafted
//! lesson candidate, cites the source receipt in the ledger, and removes the
//! candidate from the draft lane without deleting any history. Memory writes,
//! generated refresh, provider memory, and adaptation stay blocked.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

/// The receipt kinds a drafted lesson candidate may cite as its source. These
/// are the three evidence rails the draft lane folds: Forge outcome signals,
/// beta feedback captures, and fleet-eval run evidence (distillation notes).
pub const LEARNING_CANDIDATE_SOURCE_KINDS: &[&str] = &[
    "forge.outcome.signal.recorded",
    "beta.feedback.captured",
    "forge.run.event",
    "learning.implicit.signal",
];

const MAX_TEXT_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct LearningCandidateRejection<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub candidate_id: &'a str,
    pub source_receipt_id: &'a str,
    pub reason: &'a str,
    pub review_owner: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LearningCandidateRejectionReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub candidate_rejection_id: String,
    pub candidate_id: String,
    pub source_receipt_id: String,
    pub lane_state: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LearningCandidateRejectionError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownSourceReceipt(String),
    NotACandidateSourceReceipt(String, String),
    AlreadyRejected(String),
}

impl LearningCandidateRejectionError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("reject lesson candidate: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownSourceReceipt(receipt_id) => {
                format!("candidate source receipt {receipt_id} is unknown")
            }
            Self::NotACandidateSourceReceipt(receipt_id, kind) => format!(
                "candidate source receipt {receipt_id} is {kind}; expected one of {}",
                LEARNING_CANDIDATE_SOURCE_KINDS.join(", ")
            ),
            Self::AlreadyRejected(receipt_id) => {
                format!("candidate source receipt {receipt_id} is already rejected")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn reject_learning_candidate(
        &mut self,
        request: LearningCandidateRejection<'_>,
    ) -> Result<LearningCandidateRejectionReport, LearningCandidateRejectionError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.reject_learning_candidate_with_identity(request, &identity)
    }

    pub fn reject_learning_candidate_with_identity(
        &mut self,
        request: LearningCandidateRejection<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<LearningCandidateRejectionReport, LearningCandidateRejectionError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("candidate_id", request.candidate_id),
            ("source_receipt_id", request.source_receipt_id),
            ("reason", request.reason),
            ("review_owner", request.review_owner),
        ] {
            if value.trim().is_empty() {
                return Err(LearningCandidateRejectionError::Missing(field));
            }
        }
        for (field, value) in [
            ("candidate_id", request.candidate_id),
            ("reason", request.reason),
            ("review_owner", request.review_owner),
        ] {
            let len = value.chars().count();
            if len > MAX_TEXT_CHARS {
                return Err(LearningCandidateRejectionError::TooLong(
                    field,
                    len,
                    MAX_TEXT_CHARS,
                ));
            }
        }
        let source_receipt_id = request.source_receipt_id.trim();
        let source_receipt_kind = {
            let Some(source) = self.ledger().query().by_id(source_receipt_id) else {
                return Err(LearningCandidateRejectionError::UnknownSourceReceipt(
                    source_receipt_id.to_string(),
                ));
            };
            if !LEARNING_CANDIDATE_SOURCE_KINDS.contains(&source.kind.as_str()) {
                return Err(LearningCandidateRejectionError::NotACandidateSourceReceipt(
                    source_receipt_id.to_string(),
                    source.kind.clone(),
                ));
            }
            source.kind.clone()
        };
        let already_rejected = self
            .ledger()
            .query()
            .by_kind("learning.candidate.rejected")
            .iter()
            .any(|receipt| {
                receipt.payload.get("source_receipt_id").map(String::as_str)
                    == Some(source_receipt_id)
            });
        if already_rejected {
            return Err(LearningCandidateRejectionError::AlreadyRejected(
                source_receipt_id.to_string(),
            ));
        }
        let candidate_rejection_id = format!(
            "learning_candidate_rejection_{}",
            self.ledger()
                .query()
                .by_kind("learning.candidate.rejected")
                .len()
                + 1
        );
        let lane_state = "rejected";
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "learning_loop",
                action: ActionKind::RejectLearningCandidate,
                transition: "REJECT_LEARNING_CANDIDATE",
                kind: "learning.candidate.rejected",
            },
            payload(&[
                ("candidate_rejection_id", &candidate_rejection_id),
                ("candidate_id", request.candidate_id.trim()),
                ("source_receipt_id", source_receipt_id),
                ("source_receipt_kind", &source_receipt_kind),
                ("reason", request.reason.trim()),
                ("review_owner", request.review_owner.trim()),
                ("lane_state", lane_state),
                ("identity_source", &identity.identity_source),
                ("candidate_excluded_from_drafts", "true"),
                ("receipts_deleted", "false"),
                ("memory_write_allowed", "false"),
                ("generated_refresh_allowed", "false"),
                ("provider_memory_write_allowed", "false"),
                ("adaptation_allowed", "false"),
                ("runtime_behavior_change_allowed", "false"),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(LearningCandidateRejectionReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            candidate_rejection_id,
            candidate_id: request.candidate_id.trim().to_string(),
            source_receipt_id: source_receipt_id.to_string(),
            lane_state: lane_state.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ForgeOutcomeSignal;

    fn outcome_signal_receipt_id(kernel: &mut MdxKernel) -> String {
        kernel.run_evals_runner_agent().expect("seed receipt");
        let source_receipt_id = kernel
            .ledger()
            .entries()
            .first()
            .expect("source receipt")
            .receipt_id
            .clone();
        kernel
            .record_forge_outcome_signal(ForgeOutcomeSignal {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_run_1",
                source_receipt_id: &source_receipt_id,
                source_receipt_kind: "forge.run.event",
                disposition: "completed",
                summary: "Run completed.",
                capability_ids: "svelte_ui_pack",
                model_or_worker: "local_forge_worker",
                lesson_candidate: "Run UI checks for similar work.",
                lesson_source: "",
                message_channel_id: "forge",
            })
            .expect("outcome signal")
            .receipt_id
    }

    #[test]
    fn rejects_candidate_with_receipt_and_no_authority() {
        let mut kernel = MdxKernel::boot_local();
        let source_receipt_id = outcome_signal_receipt_id(&mut kernel);
        let report = kernel
            .reject_learning_candidate(LearningCandidateRejection {
                tenant_id: "t",
                actor_id: "human:eng",
                candidate_id: "learning_candidate_forge_outcome_signal_1",
                source_receipt_id: &source_receipt_id,
                reason: "The lesson overfits one run and would mislead later work.",
                review_owner: "human:eng",
            })
            .expect("candidate rejection");
        assert_eq!(report.lane_state, "rejected");
        assert_eq!(report.source_receipt_id, source_receipt_id);
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, "learning.candidate.rejected");
        assert_eq!(
            receipt
                .payload
                .get("source_receipt_kind")
                .map(String::as_str),
            Some("forge.outcome.signal.recorded")
        );
        assert_eq!(
            receipt
                .payload
                .get("candidate_excluded_from_drafts")
                .map(String::as_str),
            Some("true")
        );
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
    fn refuses_unknown_source_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.reject_learning_candidate(LearningCandidateRejection {
            tenant_id: "t",
            actor_id: "human:eng",
            candidate_id: "learning_candidate_missing",
            source_receipt_id: "receipt_that_does_not_exist",
            reason: "No source evidence exists for this candidate.",
            review_owner: "human:eng",
        });
        assert!(matches!(
            bad,
            Err(LearningCandidateRejectionError::UnknownSourceReceipt(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_source_receipt_with_wrong_kind() {
        let mut kernel = MdxKernel::boot_local();
        let source_receipt_id = outcome_signal_receipt_id(&mut kernel);
        let rejection_receipt_id = kernel
            .reject_learning_candidate(LearningCandidateRejection {
                tenant_id: "t",
                actor_id: "human:eng",
                candidate_id: "learning_candidate_forge_outcome_signal_1",
                source_receipt_id: &source_receipt_id,
                reason: "The lesson overfits one run.",
                review_owner: "human:eng",
            })
            .expect("candidate rejection")
            .receipt_id;
        let bad = kernel.reject_learning_candidate(LearningCandidateRejection {
            tenant_id: "t",
            actor_id: "human:eng",
            candidate_id: "learning_candidate_rejection_1",
            source_receipt_id: &rejection_receipt_id,
            reason: "A rejection receipt is not a candidate source.",
            review_owner: "human:eng",
        });
        assert_eq!(
            bad,
            Err(LearningCandidateRejectionError::NotACandidateSourceReceipt(
                rejection_receipt_id,
                "learning.candidate.rejected".to_string()
            ))
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_double_rejection_of_the_same_source() {
        let mut kernel = MdxKernel::boot_local();
        let source_receipt_id = outcome_signal_receipt_id(&mut kernel);
        kernel
            .reject_learning_candidate(LearningCandidateRejection {
                tenant_id: "t",
                actor_id: "human:eng",
                candidate_id: "learning_candidate_forge_outcome_signal_1",
                source_receipt_id: &source_receipt_id,
                reason: "The lesson overfits one run.",
                review_owner: "human:eng",
            })
            .expect("first rejection");
        let bad = kernel.reject_learning_candidate(LearningCandidateRejection {
            tenant_id: "t",
            actor_id: "human:eng",
            candidate_id: "learning_candidate_forge_outcome_signal_1",
            source_receipt_id: &source_receipt_id,
            reason: "Trying to reject it twice.",
            review_owner: "human:eng",
        });
        assert!(matches!(
            bad,
            Err(LearningCandidateRejectionError::AlreadyRejected(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }
}
