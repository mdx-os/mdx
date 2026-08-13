//! Evidence requests for the learning loop. This is the first write
//! primitive on Learn: it records that a person asked for stronger proof,
//! without promoting memory or changing runtime behavior.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

const MAX_TEXT_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct LearningEvidenceRequest<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub judgment_id: &'a str,
    pub promotion_id: &'a str,
    pub reason: &'a str,
    pub requested_evidence: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LearningEvidenceRequestReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub evidence_request_id: String,
    pub judgment_id: String,
    pub promotion_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LearningEvidenceRequestError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
}

impl LearningEvidenceRequestError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("ask for more evidence: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_learning_evidence_request(
        &mut self,
        request: LearningEvidenceRequest<'_>,
    ) -> Result<LearningEvidenceRequestReport, LearningEvidenceRequestError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_learning_evidence_request_with_identity(request, &identity)
    }

    pub fn record_learning_evidence_request_with_identity(
        &mut self,
        request: LearningEvidenceRequest<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<LearningEvidenceRequestReport, LearningEvidenceRequestError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("judgment_id", request.judgment_id),
            ("promotion_id", request.promotion_id),
            ("reason", request.reason),
            ("requested_evidence", request.requested_evidence),
        ] {
            if value.trim().is_empty() {
                return Err(LearningEvidenceRequestError::Missing(field));
            }
        }
        for (field, value) in [
            ("reason", request.reason),
            ("requested_evidence", request.requested_evidence),
        ] {
            let len = value.chars().count();
            if len > MAX_TEXT_CHARS {
                return Err(LearningEvidenceRequestError::TooLong(
                    field,
                    len,
                    MAX_TEXT_CHARS,
                ));
            }
        }
        let evidence_request_id = format!(
            "learning_evidence_request_{}",
            self.ledger()
                .query()
                .by_kind("learning.evidence.requested")
                .len()
                + 1
        );
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "learning_loop",
                action: ActionKind::RecordLearningEvidenceRequest,
                transition: "RECORD_LEARNING_EVIDENCE_REQUEST",
                kind: "learning.evidence.requested",
            },
            payload(&[
                ("evidence_request_id", &evidence_request_id),
                ("judgment_id", request.judgment_id.trim()),
                ("promotion_id", request.promotion_id.trim()),
                ("reason", request.reason.trim()),
                ("requested_evidence", request.requested_evidence.trim()),
                ("identity_source", &identity.identity_source),
                ("memory_promotion_allowed", "false"),
                ("adaptation_allowed", "false"),
                ("runtime_behavior_change_allowed", "false"),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(LearningEvidenceRequestReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            evidence_request_id,
            judgment_id: request.judgment_id.trim().to_string(),
            promotion_id: request.promotion_id.trim().to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_evidence_request_without_opening_authority() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .record_learning_evidence_request(LearningEvidenceRequest {
                tenant_id: "t",
                actor_id: "human:eng",
                judgment_id: "judgment_1",
                promotion_id: "promotion_1",
                reason: "Need private check proof.",
                requested_evidence: "Attach local smoke and review packet receipts.",
            })
            .expect("evidence request");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, "learning.evidence.requested");
        assert_eq!(
            receipt
                .payload
                .get("memory_promotion_allowed")
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
    fn refuses_missing_judgment_id() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.record_learning_evidence_request(LearningEvidenceRequest {
            tenant_id: "t",
            actor_id: "human:eng",
            judgment_id: "",
            promotion_id: "promotion_1",
            reason: "Need proof.",
            requested_evidence: "Attach receipts.",
        });
        assert!(matches!(
            bad,
            Err(LearningEvidenceRequestError::Missing("judgment_id"))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }
}
