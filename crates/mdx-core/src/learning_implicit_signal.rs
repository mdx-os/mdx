//! Implicit learning signals: the frontier's primary learning source, at zero
//! user cost. A human never writes these. They are DERIVED from dispositions
//! the product already records - a Forge change accepted, rejected, or sent
//! back for revision, an edit after an accept, a Twin answer a user corrected -
//! and each one cites the exact source receipt it was derived from in the
//! hash-chained ledger. The signal is strictly advisory: it drafts a lesson
//! candidate a human still gates. No memory activation, prompt mutation,
//! execution authority, or production write opens here.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

const MAX_TEXT_CHARS: usize = 2000;

/// The disposition classes an implicit signal can carry. The rich ones for
/// learning are the disagreement shapes: a rejection and an edit-after-accept
/// say the produced change was not right, which is the highest-signal outcome.
pub const LEARNING_IMPLICIT_SIGNAL_KINDS: &[&str] = &[
    "change_accepted",
    "change_rejected",
    "revision_requested",
    "edit_after_accept",
    "twin_answer_corrected",
];

/// The source receipt kinds each signal kind may be derived from. This binds a
/// signal to the disposition that actually produced it: a change_rejected can
/// only come from a real ship rejection receipt, never a fabricated id or an
/// unrelated receipt. The ledger membership check plus this coherence table are
/// the integrity floor for the whole rail.
fn allowed_source_kinds(signal_kind: &str) -> &'static [&'static str] {
    match signal_kind {
        "change_accepted" => &["forge.ship.ratified", "forge.run.ship.decided"],
        "change_rejected" => &["forge.ship.rejected"],
        "revision_requested" => &["forge.ship.revision_requested", "forge.run.event"],
        "edit_after_accept" => &["forge.run.event"],
        "twin_answer_corrected" => &[
            "twin.session.answer.grounded",
            "twin.session.draft.admitted",
        ],
        _ => &[],
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LearningImplicitSignal<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// The receipt this signal is derived from. Must exist in the ledger and
    /// its kind must be coherent with signal_kind.
    pub source_receipt_id: &'a str,
    pub signal_kind: &'a str,
    /// The work the disposition was about (a run id, a scope, a session id).
    /// May be empty when the source receipt does not name one.
    pub work_item: &'a str,
    /// What changed, or what the disagreement was - the delta that carries the
    /// learning. Empty for a clean acceptance.
    pub delta_summary: &'a str,
    /// The deterministic draft lesson this signal proposes. The lane folds it
    /// into a DRAFTED candidate; a human still edits and gates it.
    pub lesson_candidate: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LearningImplicitSignalReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub implicit_signal_id: String,
    pub signal_kind: String,
    pub source_receipt_id: String,
    pub source_receipt_kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LearningImplicitSignalError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownSignalKind(String),
    UnknownSourceReceipt(String),
    MismatchedSourceKind(String, String, String),
    AlreadyRecorded(String),
}

impl LearningImplicitSignalError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("implicit signal: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownSignalKind(kind) => format!(
                "implicit signal_kind must be one of {}; got {kind}",
                LEARNING_IMPLICIT_SIGNAL_KINDS.join(", ")
            ),
            Self::UnknownSourceReceipt(receipt_id) => {
                format!("implicit signal source receipt {receipt_id} is unknown")
            }
            Self::MismatchedSourceKind(signal_kind, source_kind, receipt_id) => format!(
                "implicit signal {signal_kind} cannot derive from a {source_kind} receipt ({receipt_id}); expected one of {}",
                allowed_source_kinds(signal_kind).join(", ")
            ),
            Self::AlreadyRecorded(receipt_id) => {
                format!("an implicit signal already cites source receipt {receipt_id}")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_learning_implicit_signal(
        &mut self,
        request: LearningImplicitSignal<'_>,
    ) -> Result<LearningImplicitSignalReport, LearningImplicitSignalError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_learning_implicit_signal_with_identity(request, &identity)
    }

    pub fn record_learning_implicit_signal_with_identity(
        &mut self,
        request: LearningImplicitSignal<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<LearningImplicitSignalReport, LearningImplicitSignalError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("source_receipt_id", request.source_receipt_id),
            ("signal_kind", request.signal_kind),
            ("lesson_candidate", request.lesson_candidate),
        ] {
            if value.trim().is_empty() {
                return Err(LearningImplicitSignalError::Missing(field));
            }
        }
        for (field, value) in [
            ("work_item", request.work_item),
            ("delta_summary", request.delta_summary),
            ("lesson_candidate", request.lesson_candidate),
        ] {
            let len = value.chars().count();
            if len > MAX_TEXT_CHARS {
                return Err(LearningImplicitSignalError::TooLong(
                    field,
                    len,
                    MAX_TEXT_CHARS,
                ));
            }
        }
        let signal_kind = request.signal_kind.trim();
        if !LEARNING_IMPLICIT_SIGNAL_KINDS.contains(&signal_kind) {
            return Err(LearningImplicitSignalError::UnknownSignalKind(
                signal_kind.to_string(),
            ));
        }
        let source_receipt_id = request.source_receipt_id.trim();
        let source_receipt_kind = {
            let Some(source) = self.ledger().query().by_id(source_receipt_id) else {
                return Err(LearningImplicitSignalError::UnknownSourceReceipt(
                    source_receipt_id.to_string(),
                ));
            };
            source.kind.clone()
        };
        if !allowed_source_kinds(signal_kind).contains(&source_receipt_kind.as_str()) {
            return Err(LearningImplicitSignalError::MismatchedSourceKind(
                signal_kind.to_string(),
                source_receipt_kind,
                source_receipt_id.to_string(),
            ));
        }
        let already_recorded = self
            .ledger()
            .query()
            .by_kind("learning.implicit.signal")
            .iter()
            .any(|receipt| {
                receipt.payload.get("source_receipt_id").map(String::as_str)
                    == Some(source_receipt_id)
            });
        if already_recorded {
            return Err(LearningImplicitSignalError::AlreadyRecorded(
                source_receipt_id.to_string(),
            ));
        }
        let implicit_signal_id = format!(
            "learning_implicit_signal_{}",
            self.ledger()
                .query()
                .by_kind("learning.implicit.signal")
                .len()
                + 1
        );
        // A clean acceptance is a quieter signal than a disagreement; the
        // receipt records which so a reader can weight it, but every kind
        // still drafts a candidate the human gates.
        let disagreement = matches!(signal_kind, "change_rejected" | "edit_after_accept");
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "learning_loop",
                action: ActionKind::RecordLearningImplicitSignal,
                transition: "RECORD_LEARNING_IMPLICIT_SIGNAL",
                kind: "learning.implicit.signal",
            },
            payload(&[
                ("implicit_signal_id", &implicit_signal_id),
                ("signal_kind", signal_kind),
                ("source_receipt_id", source_receipt_id),
                ("source_receipt_kind", &source_receipt_kind),
                ("work_item", request.work_item.trim()),
                ("delta_summary", request.delta_summary.trim()),
                ("lesson_candidate", request.lesson_candidate.trim()),
                (
                    "high_signal_disagreement",
                    if disagreement { "true" } else { "false" },
                ),
                ("identity_source", &identity.identity_source),
                ("learning_candidate_allowed", "true"),
                ("active_memory_write_allowed", "false"),
                ("generated_refresh_allowed", "false"),
                ("provider_memory_write_allowed", "false"),
                ("adaptation_allowed", "false"),
                ("runtime_behavior_change_allowed", "false"),
                ("execution_authority_opened", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(LearningImplicitSignalReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            implicit_signal_id,
            signal_kind: signal_kind.to_string(),
            source_receipt_id: source_receipt_id.to_string(),
            source_receipt_kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ForgeRunEvent, GovernedWriteIdentity};

    // A forge.run.event receipt to derive a revision signal from, returning its
    // receipt id.
    fn seed_run_event(kernel: &mut MdxKernel, run_id: &str) -> String {
        let identity = GovernedWriteIdentity::local_demo("agent:forge");
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "agent:forge",
                    run_id,
                    event: "run_started",
                    work_item_id: "",
                    detail: "accepted",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[],
            )
            .expect("run event");
        kernel
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .last()
            .expect("run receipt")
            .receipt_id
            .clone()
    }

    #[test]
    fn records_revision_signal_from_run_event_without_opening_authority() {
        let mut kernel = MdxKernel::boot_local();
        let source = seed_run_event(&mut kernel, "forge_run_1");
        let report = kernel
            .record_learning_implicit_signal(LearningImplicitSignal {
                tenant_id: "t",
                actor_id: "human:eng",
                source_receipt_id: &source,
                signal_kind: "revision_requested",
                work_item: "forge_run_1",
                delta_summary: "The reviewer asked for a smaller diff.",
                lesson_candidate: "This kind of change needed revision because the diff was too broad.",
            })
            .expect("implicit signal");
        assert_eq!(report.signal_kind, "revision_requested");
        assert_eq!(report.source_receipt_kind, "forge.run.event");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, "learning.implicit.signal");
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
                .get("active_memory_write_allowed")
                .map(String::as_str),
            Some("false")
        );
        assert_eq!(
            receipt
                .payload
                .get("high_signal_disagreement")
                .map(String::as_str),
            Some("false")
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn edit_after_accept_is_marked_high_signal() {
        let mut kernel = MdxKernel::boot_local();
        let source = seed_run_event(&mut kernel, "forge_run_2");
        let report = kernel
            .record_learning_implicit_signal(LearningImplicitSignal {
                tenant_id: "t",
                actor_id: "human:eng",
                source_receipt_id: &source,
                signal_kind: "edit_after_accept",
                work_item: "forge_run_2",
                delta_summary: "The human accepted then immediately revised it.",
                lesson_candidate: "An accepted change was edited right after; the accept missed something.",
            })
            .expect("implicit signal");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(
            receipt
                .payload
                .get("high_signal_disagreement")
                .map(String::as_str),
            Some("true")
        );
    }

    #[test]
    fn refuses_unknown_source_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.record_learning_implicit_signal(LearningImplicitSignal {
            tenant_id: "t",
            actor_id: "human:eng",
            source_receipt_id: "receipt_that_does_not_exist",
            signal_kind: "change_rejected",
            work_item: "",
            delta_summary: "",
            lesson_candidate: "No source for this.",
        });
        assert!(matches!(
            bad,
            Err(LearningImplicitSignalError::UnknownSourceReceipt(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_source_receipt_whose_kind_does_not_match_signal() {
        let mut kernel = MdxKernel::boot_local();
        // A forge.run.event cannot back a change_rejected (that needs a real
        // ship rejection receipt).
        let source = seed_run_event(&mut kernel, "forge_run_3");
        let bad = kernel.record_learning_implicit_signal(LearningImplicitSignal {
            tenant_id: "t",
            actor_id: "human:eng",
            source_receipt_id: &source,
            signal_kind: "change_rejected",
            work_item: "forge_run_3",
            delta_summary: "",
            lesson_candidate: "Trying to forge a rejection from a run event.",
        });
        assert!(matches!(
            bad,
            Err(LearningImplicitSignalError::MismatchedSourceKind(_, _, _))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_unknown_signal_kind() {
        let mut kernel = MdxKernel::boot_local();
        let source = seed_run_event(&mut kernel, "forge_run_4");
        let bad = kernel.record_learning_implicit_signal(LearningImplicitSignal {
            tenant_id: "t",
            actor_id: "human:eng",
            source_receipt_id: &source,
            signal_kind: "secretly_activated",
            work_item: "",
            delta_summary: "",
            lesson_candidate: "Bad kind.",
        });
        assert!(matches!(
            bad,
            Err(LearningImplicitSignalError::UnknownSignalKind(_))
        ));
    }

    #[test]
    fn refuses_double_signal_on_the_same_source() {
        let mut kernel = MdxKernel::boot_local();
        let source = seed_run_event(&mut kernel, "forge_run_5");
        kernel
            .record_learning_implicit_signal(LearningImplicitSignal {
                tenant_id: "t",
                actor_id: "human:eng",
                source_receipt_id: &source,
                signal_kind: "revision_requested",
                work_item: "forge_run_5",
                delta_summary: "",
                lesson_candidate: "First derivation.",
            })
            .expect("first");
        let bad = kernel.record_learning_implicit_signal(LearningImplicitSignal {
            tenant_id: "t",
            actor_id: "human:eng",
            source_receipt_id: &source,
            signal_kind: "revision_requested",
            work_item: "forge_run_5",
            delta_summary: "",
            lesson_candidate: "Second derivation.",
        });
        assert!(matches!(
            bad,
            Err(LearningImplicitSignalError::AlreadyRecorded(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }
}
