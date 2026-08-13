//! Fixture graduation draft for the golden-set regression floor. A triaged
//! finding is real evidence that MDx behaved wrong once; the sim program
//! ratified turning such findings into curated golden fixtures that every
//! future harness run must keep passing, versioned in-repo and never edited
//! retroactively. This governed write records the accountable DRAFT of that
//! intent: it names the source finding receipt (verified in the ledger), a
//! proposed fixture id, a short scenario, the expected behavior, and a
//! proposed fixture family. It writes NO registry entry and opens NO
//! allowlist: GRADUATION - the binding write that adds the fixture to the
//! Machine League and makes it a regression floor - is a separate human-
//! curated write reserved for the sim program owner. Drafting stays advisory.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

/// The receipt kind a fixture graduation draft may cite as its source: a
/// triaged beta feedback capture. A finding is the evidence that a scenario
/// deserves a regression floor, so the draft must point at one that exists.
pub const FIXTURE_GRADUATION_SOURCE_KINDS: &[&str] = &["beta.feedback.captured"];

/// The receipt kind this write records.
pub const FIXTURE_GRADUATION_DRAFTED_KIND: &str = "fixture.graduation.drafted";

/// The reserved receipt kind for the binding graduation write. It is declared
/// here so the state machine is legible, but this crate never records it: the
/// human-curated graduation that makes a fixture a regression floor is the sim
/// program owner's lane.
pub const FIXTURE_GRADUATION_GRADUATED_KIND_RESERVED: &str = "fixture.graduation.graduated";

const MAX_TEXT_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct FixtureGraduationDraft<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub source_receipt_id: &'a str,
    pub fixture_id: &'a str,
    pub scenario_description: &'a str,
    pub expected_behavior: &'a str,
    pub proposed_fixture_family: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixtureGraduationDraftReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub fixture_graduation_id: String,
    pub fixture_id: String,
    pub source_receipt_id: String,
    pub lane_state: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FixtureGraduationDraftError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownSourceReceipt(String),
    NotAFindingSourceReceipt(String, String),
    AlreadyDrafted(String),
}

impl FixtureGraduationDraftError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("draft fixture graduation: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownSourceReceipt(receipt_id) => {
                format!("fixture graduation source receipt {receipt_id} is unknown")
            }
            Self::NotAFindingSourceReceipt(receipt_id, kind) => format!(
                "fixture graduation source receipt {receipt_id} is {kind}; expected one of {}",
                FIXTURE_GRADUATION_SOURCE_KINDS.join(", ")
            ),
            Self::AlreadyDrafted(fixture_id) => {
                format!("fixture {fixture_id} already has a graduation draft")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn draft_fixture_graduation(
        &mut self,
        request: FixtureGraduationDraft<'_>,
    ) -> Result<FixtureGraduationDraftReport, FixtureGraduationDraftError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.draft_fixture_graduation_with_identity(request, &identity)
    }

    pub fn draft_fixture_graduation_with_identity(
        &mut self,
        request: FixtureGraduationDraft<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<FixtureGraduationDraftReport, FixtureGraduationDraftError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("source_receipt_id", request.source_receipt_id),
            ("fixture_id", request.fixture_id),
            ("scenario_description", request.scenario_description),
            ("expected_behavior", request.expected_behavior),
            ("proposed_fixture_family", request.proposed_fixture_family),
        ] {
            if value.trim().is_empty() {
                return Err(FixtureGraduationDraftError::Missing(field));
            }
        }
        for (field, value) in [
            ("fixture_id", request.fixture_id),
            ("scenario_description", request.scenario_description),
            ("expected_behavior", request.expected_behavior),
            ("proposed_fixture_family", request.proposed_fixture_family),
        ] {
            let len = value.chars().count();
            if len > MAX_TEXT_CHARS {
                return Err(FixtureGraduationDraftError::TooLong(
                    field,
                    len,
                    MAX_TEXT_CHARS,
                ));
            }
        }
        let source_receipt_id = request.source_receipt_id.trim();
        let source_receipt_kind = {
            let Some(source) = self.ledger().query().by_id(source_receipt_id) else {
                return Err(FixtureGraduationDraftError::UnknownSourceReceipt(
                    source_receipt_id.to_string(),
                ));
            };
            if !FIXTURE_GRADUATION_SOURCE_KINDS.contains(&source.kind.as_str()) {
                return Err(FixtureGraduationDraftError::NotAFindingSourceReceipt(
                    source_receipt_id.to_string(),
                    source.kind.clone(),
                ));
            }
            source.kind.clone()
        };
        let fixture_id = request.fixture_id.trim();
        let already_drafted = self
            .ledger()
            .query()
            .by_kind(FIXTURE_GRADUATION_DRAFTED_KIND)
            .iter()
            .any(|receipt| {
                receipt.payload.get("fixture_id").map(String::as_str) == Some(fixture_id)
            });
        if already_drafted {
            return Err(FixtureGraduationDraftError::AlreadyDrafted(
                fixture_id.to_string(),
            ));
        }
        let fixture_graduation_id = format!(
            "fixture_graduation_draft_{}",
            self.ledger()
                .query()
                .by_kind(FIXTURE_GRADUATION_DRAFTED_KIND)
                .len()
                + 1
        );
        let lane_state = "drafted";
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "learning_loop",
                action: ActionKind::DraftFixtureGraduation,
                transition: "DRAFT_FIXTURE_GRADUATION",
                kind: FIXTURE_GRADUATION_DRAFTED_KIND,
            },
            payload(&[
                ("fixture_graduation_id", &fixture_graduation_id),
                ("fixture_id", fixture_id),
                ("source_receipt_id", source_receipt_id),
                ("source_receipt_kind", &source_receipt_kind),
                ("scenario_description", request.scenario_description.trim()),
                ("expected_behavior", request.expected_behavior.trim()),
                (
                    "proposed_fixture_family",
                    request.proposed_fixture_family.trim(),
                ),
                ("proposed_origin", "graduated"),
                ("lane_state", lane_state),
                (
                    "reserved_graduated_kind",
                    FIXTURE_GRADUATION_GRADUATED_KIND_RESERVED,
                ),
                ("identity_source", &identity.identity_source),
                ("graduation_write_allowed", "false"),
                ("registry_write_allowed", "false"),
                ("allowlist_change_allowed", "false"),
                ("memory_write_allowed", "false"),
                ("adaptation_allowed", "false"),
                ("runtime_behavior_change_allowed", "false"),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(FixtureGraduationDraftReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            fixture_graduation_id,
            fixture_id: fixture_id.to_string(),
            source_receipt_id: source_receipt_id.to_string(),
            lane_state: lane_state.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BetaFeedbackCapture;

    fn finding_receipt_id(kernel: &mut MdxKernel) -> String {
        kernel
            .record_beta_feedback_local(&BetaFeedbackCapture {
                surface: "forge",
                route: "/forge",
                tenant_id: "local_tenant",
                actor_id: "human:beta",
                session_ref: "session_1",
                occurred_at: "2026-07-03T00:00:00Z",
                category: "bug",
                context: &[],
                note: "A no_change run inherited a misdiagnosed fix location.",
            })
            .expect("feedback")
            .feedback_receipt_id
    }

    #[test]
    fn drafts_graduation_with_receipt_and_no_authority() {
        let mut kernel = MdxKernel::boot_local();
        let source_receipt_id = finding_receipt_id(&mut kernel);
        let report = kernel
            .draft_fixture_graduation(FixtureGraduationDraft {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                source_receipt_id: &source_receipt_id,
                fixture_id: "rust_no_change_premise_guard",
                scenario_description: "A self-fix run inherited a wrong fix location and produced no_change.",
                expected_behavior: "The compose seam verifies the fix location against the current file before building.",
                proposed_fixture_family: "rust_compose_seam",
            })
            .expect("fixture graduation draft");
        assert_eq!(report.lane_state, "drafted");
        assert_eq!(report.source_receipt_id, source_receipt_id);
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, "fixture.graduation.drafted");
        assert_eq!(
            receipt
                .payload
                .get("source_receipt_kind")
                .map(String::as_str),
            Some("beta.feedback.captured")
        );
        assert_eq!(
            receipt.payload.get("proposed_origin").map(String::as_str),
            Some("graduated")
        );
        assert_eq!(
            receipt
                .payload
                .get("graduation_write_allowed")
                .map(String::as_str),
            Some("false")
        );
        assert_eq!(
            receipt
                .payload
                .get("registry_write_allowed")
                .map(String::as_str),
            Some("false")
        );
        assert_eq!(
            receipt
                .payload
                .get("reserved_graduated_kind")
                .map(String::as_str),
            Some("fixture.graduation.graduated")
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_unknown_source_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.draft_fixture_graduation(FixtureGraduationDraft {
            tenant_id: "local_tenant",
            actor_id: "human:eng",
            source_receipt_id: "receipt_that_does_not_exist",
            fixture_id: "rust_missing_source",
            scenario_description: "No source finding exists.",
            expected_behavior: "The draft must refuse.",
            proposed_fixture_family: "rust_compose_seam",
        });
        assert!(matches!(
            bad,
            Err(FixtureGraduationDraftError::UnknownSourceReceipt(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_source_receipt_with_wrong_kind() {
        let mut kernel = MdxKernel::boot_local();
        let finding = finding_receipt_id(&mut kernel);
        // Drafting once yields a fixture.graduation.drafted receipt; that
        // receipt is not itself a valid finding source for a second draft.
        let drafted = kernel
            .draft_fixture_graduation(FixtureGraduationDraft {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                source_receipt_id: &finding,
                fixture_id: "rust_first_fixture",
                scenario_description: "A first curated scenario.",
                expected_behavior: "The harness keeps passing this fixture.",
                proposed_fixture_family: "rust_compose_seam",
            })
            .expect("first draft")
            .receipt_id;
        let bad = kernel.draft_fixture_graduation(FixtureGraduationDraft {
            tenant_id: "local_tenant",
            actor_id: "human:eng",
            source_receipt_id: &drafted,
            fixture_id: "rust_second_fixture",
            scenario_description: "A draft receipt is not a finding.",
            expected_behavior: "The draft must refuse.",
            proposed_fixture_family: "rust_compose_seam",
        });
        assert_eq!(
            bad,
            Err(FixtureGraduationDraftError::NotAFindingSourceReceipt(
                drafted,
                "fixture.graduation.drafted".to_string()
            ))
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_double_draft_of_the_same_fixture() {
        let mut kernel = MdxKernel::boot_local();
        let finding = finding_receipt_id(&mut kernel);
        kernel
            .draft_fixture_graduation(FixtureGraduationDraft {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                source_receipt_id: &finding,
                fixture_id: "rust_duplicate_fixture",
                scenario_description: "A curated scenario.",
                expected_behavior: "The harness keeps passing this fixture.",
                proposed_fixture_family: "rust_compose_seam",
            })
            .expect("first draft");
        let second_finding = finding_receipt_id(&mut kernel);
        let bad = kernel.draft_fixture_graduation(FixtureGraduationDraft {
            tenant_id: "local_tenant",
            actor_id: "human:eng",
            source_receipt_id: &second_finding,
            fixture_id: "rust_duplicate_fixture",
            scenario_description: "The same fixture id again.",
            expected_behavior: "The draft must refuse.",
            proposed_fixture_family: "rust_compose_seam",
        });
        assert!(matches!(
            bad,
            Err(FixtureGraduationDraftError::AlreadyDrafted(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }
}
