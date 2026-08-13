//! Forge outcome signals are the producer edge for the learning flywheel.
//! They summarize what a Forge run or decision produced, link back to the
//! source receipts, and stay strictly advisory: no memory activation, prompt
//! mutation, execution authority, or production write opens here.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

pub const FORGE_OUTCOME_DISPOSITIONS: &[&str] = &[
    "completed",
    "no_change",
    "blocked",
    "failed",
    "stopped",
    "budget_exhausted",
    "escalated",
    "approved",
    "declined",
    "revised",
];

const MAX_TEXT_CHARS: usize = 2000;

/// Where the lesson_candidate text came from. Deterministic assembly from
/// ledger evidence is the guaranteed floor; a model distillation pass may
/// sharpen it, and the receipt must say which one the reader is holding.
pub const FORGE_OUTCOME_LESSON_SOURCES: &[&str] = &["deterministic", "model_distilled"];

#[derive(Clone, Copy, Debug)]
pub struct ForgeOutcomeSignal<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub run_id: &'a str,
    pub source_receipt_id: &'a str,
    pub source_receipt_kind: &'a str,
    pub disposition: &'a str,
    pub summary: &'a str,
    pub capability_ids: &'a str,
    pub model_or_worker: &'a str,
    pub lesson_candidate: &'a str,
    /// Empty means deterministic; the kernel normalizes it on the receipt.
    pub lesson_source: &'a str,
    pub message_channel_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeOutcomeSignalReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub outcome_signal_id: String,
    pub disposition: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForgeOutcomeSignalError {
    Missing(&'static str),
    UnknownDisposition(String),
    UnknownLessonSource(String),
    UnknownSourceReceipt(String),
    TooLong(&'static str, usize, usize),
}

impl ForgeOutcomeSignalError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("forge outcome signal missing {field}"),
            Self::UnknownDisposition(other) => format!(
                "forge outcome disposition must be one of {}; got {other}",
                FORGE_OUTCOME_DISPOSITIONS.join(", ")
            ),
            Self::UnknownLessonSource(other) => format!(
                "forge outcome lesson_source must be one of {}; got {other}",
                FORGE_OUTCOME_LESSON_SOURCES.join(", ")
            ),
            Self::UnknownSourceReceipt(receipt_id) => {
                format!("forge outcome source receipt {receipt_id} is unknown")
            }
            Self::TooLong(field, len, max) => {
                format!("forge outcome {field} is {len} characters; the limit is {max}")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_forge_outcome_signal(
        &mut self,
        request: ForgeOutcomeSignal<'_>,
    ) -> Result<ForgeOutcomeSignalReport, ForgeOutcomeSignalError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_forge_outcome_signal_with_identity(request, &identity)
    }

    pub fn record_forge_outcome_signal_with_identity(
        &mut self,
        request: ForgeOutcomeSignal<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeOutcomeSignalReport, ForgeOutcomeSignalError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("run_id", request.run_id),
            ("source_receipt_id", request.source_receipt_id),
            ("source_receipt_kind", request.source_receipt_kind),
            ("disposition", request.disposition),
            ("summary", request.summary),
            ("lesson_candidate", request.lesson_candidate),
        ] {
            if value.trim().is_empty() {
                return Err(ForgeOutcomeSignalError::Missing(field));
            }
        }
        let disposition = request.disposition.trim();
        if !FORGE_OUTCOME_DISPOSITIONS.contains(&disposition) {
            return Err(ForgeOutcomeSignalError::UnknownDisposition(
                disposition.to_string(),
            ));
        }
        let lesson_source = match request.lesson_source.trim() {
            "" => "deterministic",
            trimmed => trimmed,
        };
        if !FORGE_OUTCOME_LESSON_SOURCES.contains(&lesson_source) {
            return Err(ForgeOutcomeSignalError::UnknownLessonSource(
                lesson_source.to_string(),
            ));
        }
        if self
            .ledger()
            .query()
            .by_id(request.source_receipt_id.trim())
            .is_none()
        {
            return Err(ForgeOutcomeSignalError::UnknownSourceReceipt(
                request.source_receipt_id.trim().to_string(),
            ));
        }
        for (field, value) in [
            ("summary", request.summary),
            ("capability_ids", request.capability_ids),
            ("model_or_worker", request.model_or_worker),
            ("lesson_candidate", request.lesson_candidate),
            ("message_channel_id", request.message_channel_id),
        ] {
            let len = value.chars().count();
            if len > MAX_TEXT_CHARS {
                return Err(ForgeOutcomeSignalError::TooLong(field, len, MAX_TEXT_CHARS));
            }
        }
        let outcome_signal_id = format!(
            "forge_outcome_signal_{}",
            self.ledger()
                .query()
                .by_kind("forge.outcome.signal.recorded")
                .len()
                + 1
        );
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "forge_outcome",
                action: ActionKind::RecordForgeOutcomeSignal,
                transition: "RECORD_FORGE_OUTCOME_SIGNAL",
                kind: "forge.outcome.signal.recorded",
            },
            payload(&[
                ("outcome_signal_id", &outcome_signal_id),
                ("run_id", request.run_id.trim()),
                ("source_receipt_id", request.source_receipt_id.trim()),
                ("source_receipt_kind", request.source_receipt_kind.trim()),
                ("disposition", disposition),
                ("summary", request.summary.trim()),
                ("capability_ids", request.capability_ids.trim()),
                ("model_or_worker", request.model_or_worker.trim()),
                ("lesson_candidate", request.lesson_candidate.trim()),
                ("lesson_source", lesson_source),
                ("message_channel_id", request.message_channel_id.trim()),
                ("identity_source", &identity.identity_source),
                ("learning_candidate_allowed", "true"),
                ("message_activity_allowed", "true"),
                ("active_memory_write_allowed", "false"),
                ("adaptation_allowed", "false"),
                ("execution_authority_opened", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ForgeOutcomeSignalReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            outcome_signal_id,
            disposition: disposition.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_outcome_signal_without_opening_authority() {
        let mut kernel = MdxKernel::boot_local();
        kernel.run_evals_runner_agent().expect("seed receipt");
        let source_receipt_id = kernel
            .ledger()
            .entries()
            .first()
            .expect("source receipt")
            .receipt_id
            .clone();
        let report = kernel
            .record_forge_outcome_signal(ForgeOutcomeSignal {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_run_1",
                source_receipt_id: &source_receipt_id,
                source_receipt_kind: "forge.run.event",
                disposition: "completed",
                summary: "Run completed with sandbox checks.",
                capability_ids: "svelte_ui_pack",
                model_or_worker: "local_forge_worker",
                lesson_candidate: "Similar UI work should run the UI smoke check.",
                lesson_source: "",
                message_channel_id: "forge",
            })
            .expect("outcome records");
        assert_eq!(report.disposition, "completed");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, "forge.outcome.signal.recorded");
        assert_eq!(
            receipt.payload.get("lesson_source").map(String::as_str),
            Some("deterministic"),
            "an empty lesson_source normalizes to deterministic"
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
                .get("execution_authority_opened")
                .map(String::as_str),
            Some("false")
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn refuses_unknown_disposition() {
        let mut kernel = MdxKernel::boot_local();
        kernel.run_evals_runner_agent().expect("seed receipt");
        let source_receipt_id = kernel
            .ledger()
            .entries()
            .first()
            .expect("source receipt")
            .receipt_id
            .clone();
        let error = kernel
            .record_forge_outcome_signal(ForgeOutcomeSignal {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_run_1",
                source_receipt_id: &source_receipt_id,
                source_receipt_kind: "forge.run.event",
                disposition: "secretly_deployed",
                summary: "Bad.",
                capability_ids: "",
                model_or_worker: "",
                lesson_candidate: "Bad.",
                lesson_source: "",
                message_channel_id: "forge",
            })
            .expect_err("unknown disposition refused");
        assert!(error.message().contains("must be one of"));
    }

    #[test]
    fn refuses_unknown_lesson_source() {
        let mut kernel = MdxKernel::boot_local();
        kernel.run_evals_runner_agent().expect("seed receipt");
        let source_receipt_id = kernel
            .ledger()
            .entries()
            .first()
            .expect("source receipt")
            .receipt_id
            .clone();
        let error = kernel
            .record_forge_outcome_signal(ForgeOutcomeSignal {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_run_1",
                source_receipt_id: &source_receipt_id,
                source_receipt_kind: "forge.run.event",
                disposition: "completed",
                summary: "Run completed.",
                capability_ids: "",
                model_or_worker: "local_forge_worker",
                lesson_candidate: "A lesson candidate exists.",
                lesson_source: "hallucinated",
                message_channel_id: "forge",
            })
            .expect_err("unknown lesson source refused");
        assert!(error.message().contains("lesson_source must be one of"));
    }

    #[test]
    fn refuses_unknown_source_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let error = kernel
            .record_forge_outcome_signal(ForgeOutcomeSignal {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_run_1",
                source_receipt_id: "receipt_missing",
                source_receipt_kind: "forge.run.event",
                disposition: "completed",
                summary: "Run completed with sandbox checks.",
                capability_ids: "",
                model_or_worker: "local_forge_worker",
                lesson_candidate: "A lesson candidate exists.",
                lesson_source: "",
                message_channel_id: "forge",
            })
            .expect_err("unknown source refused");
        assert!(
            error
                .message()
                .contains("source receipt receipt_missing is unknown")
        );
    }
}
