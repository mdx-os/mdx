//! The Forge harness run, on the record. A run is the native coding loop:
//! the build model reads, edits, and runs checks in an isolated workspace
//! until the gates pass or a budget stops it, and every turn the loop takes
//! lands here as a receipt in the DXR event grammar - so a run reads back as
//! the activity trail the dynamic-workflow architecture always specified,
//! and its current state is derived from those receipts, never stored.
//!
//! Presence-first, always: this rail records the shape of what happened -
//! which event, which tool, exit codes, token counts, the branch a run
//! produced - plus bounded, redacted operator-facing prose for agent turn
//! updates and finish summaries. It never records prompts, command output,
//! secrets, or production writes.
//!
//! The kernel opens no network and runs no command here. The server's
//! separately-gated drivers do the touching; this rail just witnesses it.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

/// The DXR event grammar, fixed: every receipt a run emits is one of these.
/// run_started and run_finished bracket the run; the middle events are the
/// loop's own heartbeat plus bounded human-facing summaries.
pub const FORGE_RUN_EVENTS: &[&str] = &[
    "run_started",
    "model_called",
    "agent_turn_prose",
    "turn_executed",
    "tool_executed",
    "check_started",
    "check_passed",
    "check_failed",
    "evidence_appended",
    "transcript_persisted",
    "transcript_compacted",
    "planning_contract_recorded",
    "planning_contract_unavailable",
    "finish_evaluated",
    "plan_updated",
    "plan_proposed",
    "run_recovery_pending",
    "run_resumed",
    "run_summary",
    "run_finished",
];

pub const FORGE_RUN_REFUSED_KIND: &str = "forge.run.refused";

const MAX_FIELD_CHARS: usize = 500;
const MAX_DETAIL_CHARS: usize = 2000;

/// One run event to witness. Every field is shape, not content.
#[derive(Clone, Copy, Debug)]
pub struct ForgeRunEvent<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub run_id: &'a str,
    /// One of FORGE_RUN_EVENTS.
    pub event: &'a str,
    /// The work item this run serves, when there is one.
    pub work_item_id: &'a str,
    /// A short line: the tool name, the check command, the finish outcome,
    /// or bounded, redacted human prose for agent_turn_prose/run_summary.
    pub detail: &'a str,
    /// Monotonic turn index within the run.
    pub turn: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeRunEventReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub event: String,
}

/// A refused Forge run admission. Refusals are product-significant events:
/// they explain why a run did not start and keep that answer on the same
/// governed receipt chain as successful run starts.
#[derive(Clone, Copy, Debug)]
pub struct ForgeRunRefusal<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub route: &'a str,
    pub reason: &'a str,
    pub repo_id: &'a str,
    pub requested_run_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeRunRefusalReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForgeRunError {
    Missing(&'static str),
    UnknownEvent(String),
    InvalidEvidenceField(String),
    TooLong(&'static str, usize, usize),
}

impl ForgeRunError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("forge run event missing {field}"),
            Self::UnknownEvent(other) => format!(
                "forge run event must be one of {}; got {other}",
                FORGE_RUN_EVENTS.join(", ")
            ),
            Self::InvalidEvidenceField(field) => {
                format!("forge run evidence field is not allowed: {field}")
            }
            Self::TooLong(field, len, max) => {
                format!("forge run {field} is {len} characters; the limit is {max}")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_forge_run_event(
        &mut self,
        request: ForgeRunEvent<'_>,
    ) -> Result<ForgeRunEventReport, ForgeRunError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_forge_run_event_with_identity(request, &identity)
    }

    pub fn record_forge_run_event_with_identity(
        &mut self,
        request: ForgeRunEvent<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeRunEventReport, ForgeRunError> {
        self.record_forge_run_event_internal(request, identity, None, &[])
    }

    pub fn record_forge_run_event_with_evidence_fields(
        &mut self,
        request: ForgeRunEvent<'_>,
        identity: &GovernedWriteIdentity,
        evidence_fields: &[(&str, &str)],
    ) -> Result<ForgeRunEventReport, ForgeRunError> {
        self.record_forge_run_event_internal(request, identity, None, evidence_fields)
    }

    pub fn record_forge_run_event_with_duration(
        &mut self,
        request: ForgeRunEvent<'_>,
        duration_ms: u64,
    ) -> Result<ForgeRunEventReport, ForgeRunError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_forge_run_event_internal(request, &identity, Some(duration_ms), &[])
    }

    pub fn record_forge_run_event_with_duration_and_evidence_fields(
        &mut self,
        request: ForgeRunEvent<'_>,
        duration_ms: u64,
        evidence_fields: &[(&str, &str)],
    ) -> Result<ForgeRunEventReport, ForgeRunError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_forge_run_event_internal(request, &identity, Some(duration_ms), evidence_fields)
    }

    fn record_forge_run_event_internal(
        &mut self,
        request: ForgeRunEvent<'_>,
        identity: &GovernedWriteIdentity,
        duration_ms: Option<u64>,
        evidence_fields: &[(&str, &str)],
    ) -> Result<ForgeRunEventReport, ForgeRunError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("run_id", request.run_id),
            ("event", request.event),
        ] {
            if value.trim().is_empty() {
                return Err(ForgeRunError::Missing(field));
            }
        }
        if !FORGE_RUN_EVENTS.contains(&request.event) {
            return Err(ForgeRunError::UnknownEvent(request.event.to_string()));
        }
        let detail_len = request.detail.chars().count();
        if detail_len > MAX_DETAIL_CHARS {
            return Err(ForgeRunError::TooLong(
                "detail",
                detail_len,
                MAX_DETAIL_CHARS,
            ));
        }
        let work_len = request.work_item_id.chars().count();
        if work_len > MAX_FIELD_CHARS {
            return Err(ForgeRunError::TooLong(
                "work_item_id",
                work_len,
                MAX_FIELD_CHARS,
            ));
        }
        for (key, value) in evidence_fields {
            if !valid_evidence_key(key) || reserved_evidence_key(key) {
                return Err(ForgeRunError::InvalidEvidenceField((*key).to_string()));
            }
            let value_len = value.chars().count();
            if value_len > MAX_DETAIL_CHARS {
                return Err(ForgeRunError::TooLong(
                    "evidence_field",
                    value_len,
                    MAX_DETAIL_CHARS,
                ));
            }
        }
        let turn = request.turn.to_string();
        let tokens_in = request.input_tokens.to_string();
        let tokens_out = request.output_tokens.to_string();
        let duration = duration_ms.map(|ms| ms.to_string());
        let terminal_status =
            (request.event == "run_finished").then(|| terminal_status_from_detail(request.detail));
        let mut fields = vec![
            ("run_id", request.run_id),
            ("event", request.event),
            ("work_item_id", request.work_item_id),
            ("detail", request.detail),
            ("turn", &turn),
            ("tokens_in", &tokens_in),
            ("tokens_out", &tokens_out),
            ("identity_source", &identity.identity_source),
            ("owner_user_id", &identity.subject_actor_id),
            ("output_text_recorded", "false"),
            ("authority_opened", "none"),
            ("production_write_allowed", "false"),
        ];
        if let Some(duration) = &duration {
            fields.push(("duration_ms", duration));
        }
        if let Some(terminal_status) = terminal_status {
            fields.push(("terminal_status", terminal_status));
        }
        for (key, value) in evidence_fields {
            fields.push((key, value));
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "forge_harness_run",
                action: ActionKind::RecordForgeRunEvent,
                transition: "RECORD_FORGE_RUN_EVENT",
                kind: "forge.run.event",
            },
            payload(&fields),
        );
        Ok(ForgeRunEventReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            event: request.event.to_string(),
        })
    }

    pub fn record_forge_run_refusal_with_identity(
        &mut self,
        request: ForgeRunRefusal<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeRunRefusalReport, ForgeRunError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("route", request.route),
            ("reason", request.reason),
        ] {
            if value.trim().is_empty() {
                return Err(ForgeRunError::Missing(field));
            }
        }
        let reason_len = request.reason.chars().count();
        if reason_len > MAX_DETAIL_CHARS {
            return Err(ForgeRunError::TooLong(
                "reason",
                reason_len,
                MAX_DETAIL_CHARS,
            ));
        }
        for (field, value) in [
            ("repo_id", request.repo_id),
            ("requested_run_id", request.requested_run_id),
        ] {
            let len = value.chars().count();
            if len > MAX_FIELD_CHARS {
                return Err(ForgeRunError::TooLong(field, len, MAX_FIELD_CHARS));
            }
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "forge_run_refusal",
                action: ActionKind::RecordForgeRunRefusal,
                transition: "RECORD_FORGE_RUN_REFUSAL",
                kind: FORGE_RUN_REFUSED_KIND,
            },
            payload(&[
                ("route", request.route),
                ("reason", request.reason),
                ("repo_id", request.repo_id),
                ("requested_run_id", request.requested_run_id),
                ("identity_source", &identity.identity_source),
                ("output_text_recorded", "false"),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ForgeRunRefusalReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            reason: request.reason.to_string(),
        })
    }
}

fn valid_evidence_key(key: &str) -> bool {
    !key.trim().is_empty()
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn terminal_status_from_detail(detail: &str) -> &str {
    detail
        .split_whitespace()
        .find_map(|part| part.strip_prefix("status="))
        .map(|status| status.trim_matches(','))
        .filter(|status| {
            !status.is_empty()
                && status.len() <= 100
                && status
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        })
        .unwrap_or("RUN_FINISHED_UNKNOWN")
}

fn reserved_evidence_key(key: &str) -> bool {
    matches!(
        key,
        "run_id"
            | "event"
            | "work_item_id"
            | "detail"
            | "turn"
            | "tokens_in"
            | "tokens_out"
            | "identity_source"
            | "owner_user_id"
            | "terminal_status"
            | "output_text_recorded"
            | "authority_opened"
            | "production_write_allowed"
            | "duration_ms"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_event_is_refused_and_a_known_one_records_presence_only() {
        let mut kernel = MdxKernel::boot_local();
        let err = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:dev",
                run_id: "forge_run_001",
                event: "exfiltrated_secrets",
                work_item_id: "w1",
                detail: "nope",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect_err("unknown event must refuse");
        assert!(err.message().contains("must be one of"));

        let report = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:dev",
                run_id: "forge_run_001",
                event: "tool_executed",
                work_item_id: "w1",
                detail: "write_file src/lib.rs",
                turn: 3,
                input_tokens: 120,
                output_tokens: 40,
            })
            .expect("known event records");
        assert_eq!(report.event, "tool_executed");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .cloned()
            .expect("receipt exists");
        // Presence only: the shape is recorded, never output text.
        let output_recorded = receipt
            .payload
            .get("output_text_recorded")
            .map(String::as_str);
        assert_eq!(output_recorded, Some("false"));
        assert_eq!(receipt.payload.get("turn").map(String::as_str), Some("3"));
        let proof_started = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:dev",
                run_id: "forge_run_001",
                event: "check_started",
                work_item_id: "w1",
                detail: "run_command_started cargo test",
                turn: 4,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("check start records");
        assert_eq!(proof_started.event, "check_started");
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn startup_recovery_pending_is_a_governed_run_event() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "agent:forge_recovery",
                run_id: "forge_run_interrupted",
                event: "run_recovery_pending",
                work_item_id: "w1",
                detail: "startup found an interrupted run; explicit resume can recover",
                turn: 2,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("startup recovery event records");

        assert_eq!(report.event, "run_recovery_pending");
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn tenant_id_missing_refuses_with_missing_error() {
        let mut kernel = MdxKernel::boot_local();
        let err = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "",
                actor_id: "human:dev",
                run_id: "forge_run_001",
                event: "tool_executed",
                work_item_id: "w1",
                detail: "write_file src/lib.rs",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect_err("missing tenant must refuse");
        assert!(err.message().contains("missing tenant_id"));
    }

    #[test]
    fn optional_duration_is_recorded_when_the_driver_measured_it() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .record_forge_run_event_with_duration(
                ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "human:dev",
                    run_id: "forge_run_001",
                    event: "model_called",
                    work_item_id: "w1",
                    detail: "model=qwen finish_reason=tool_calls tool_calls=1",
                    turn: 1,
                    input_tokens: 300,
                    output_tokens: 40,
                },
                27,
            )
            .expect("timed event records");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .cloned()
            .expect("receipt exists");
        assert_eq!(
            receipt.payload.get("duration_ms").map(String::as_str),
            Some("27")
        );
        assert_eq!(
            receipt
                .payload
                .get("output_text_recorded")
                .map(String::as_str),
            Some("false")
        );
    }

    #[test]
    fn evidence_fields_record_repo_profile_shape_without_output_text() {
        let mut kernel = MdxKernel::boot_local();
        let identity = crate::GovernedWriteIdentity::local_demo("human:dev");
        let evidence = [
            ("language_pack_id", "swift-spm"),
            ("selected_checks", "swift test"),
            ("repo_profile_grants_execution_authority", "false"),
        ];

        let report = kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "human:dev",
                    run_id: "forge_run_001",
                    event: "run_started",
                    work_item_id: "w1",
                    detail: "accepted",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &evidence,
            )
            .expect("evidence fields record");

        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .cloned()
            .expect("receipt exists");
        assert_eq!(
            receipt.payload.get("language_pack_id").map(String::as_str),
            Some("swift-spm")
        );
        assert_eq!(
            receipt.payload.get("selected_checks").map(String::as_str),
            Some("swift test")
        );
        assert_eq!(
            receipt
                .payload
                .get("output_text_recorded")
                .map(String::as_str),
            Some("false")
        );
    }

    #[test]
    fn evidence_fields_cannot_override_core_run_event_fields() {
        let mut kernel = MdxKernel::boot_local();
        let identity = crate::GovernedWriteIdentity::local_demo("human:dev");
        let evidence = [("output_text_recorded", "true")];

        let error = kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "human:dev",
                    run_id: "forge_run_001",
                    event: "run_started",
                    work_item_id: "w1",
                    detail: "accepted",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &evidence,
            )
            .expect_err("reserved evidence field refused");

        assert!(error.message().contains("output_text_recorded"));
    }

    #[test]
    fn actor_id_missing_refuses_with_missing_error() {
        let mut kernel = MdxKernel::boot_local();
        let err = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "   ",
                run_id: "forge_run_001",
                event: "tool_executed",
                work_item_id: "w1",
                detail: "write_file src/lib.rs",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect_err("missing actor must refuse");
        assert!(err.message().contains("missing actor_id"));
    }

    #[test]
    fn run_id_missing_refuses_with_missing_error() {
        let mut kernel = MdxKernel::boot_local();
        let err = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:dev",
                run_id: "",
                event: "tool_executed",
                work_item_id: "w1",
                detail: "write_file src/lib.rs",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect_err("missing run must refuse");
        assert!(err.message().contains("missing run_id"));
    }

    #[test]
    fn event_missing_refuses_with_missing_error() {
        let mut kernel = MdxKernel::boot_local();
        let err = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:dev",
                run_id: "forge_run_001",
                event: "   ",
                work_item_id: "w1",
                detail: "write_file src/lib.rs",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect_err("missing event must refuse");
        assert!(err.message().contains("missing event"));
    }

    #[test]
    fn detail_too_long_refuses_with_too_long_error() {
        let mut kernel = MdxKernel::boot_local();
        let long_detail = "x".repeat(2001);
        let err = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:dev",
                run_id: "forge_run_001",
                event: "tool_executed",
                work_item_id: "w1",
                detail: &long_detail,
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect_err("too long detail must refuse");
        assert!(err.message().contains("is 2001 characters"));
    }

    #[test]
    fn work_item_id_too_long_refuses_with_too_long_error() {
        let mut kernel = MdxKernel::boot_local();
        let long_work = "y".repeat(501);
        let err = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:dev",
                run_id: "forge_run_001",
                event: "tool_executed",
                work_item_id: &long_work,
                detail: "write_file src/lib.rs",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect_err("too long work_item must refuse");
        assert!(err.message().contains("is 501 characters"));
    }

    #[test]
    fn record_forge_run_event_with_identity_records_presence_only() {
        let mut kernel = MdxKernel::boot_local();
        let identity = crate::GovernedWriteIdentity::local_demo("human:dev");
        let report = kernel
            .record_forge_run_event_with_identity(
                ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "human:dev",
                    run_id: "forge_run_001",
                    event: "run_started",
                    work_item_id: "w1",
                    detail: "start",
                    turn: 0,
                    input_tokens: 10,
                    output_tokens: 0,
                },
                &identity,
            )
            .expect("with_identity happy path records");
        assert_eq!(report.event, "run_started");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .cloned()
            .expect("receipt exists");
        // Presence only: the shape is recorded, never output text.
        assert_eq!(
            receipt
                .payload
                .get("output_text_recorded")
                .map(String::as_str),
            Some("false")
        );
        assert_eq!(
            receipt.payload.get("owner_user_id").map(String::as_str),
            Some("human:dev")
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn terminal_outcome_and_sponsoring_owner_are_explicit_receipt_fields() {
        let mut kernel = MdxKernel::boot_local();
        let identity = crate::GovernedWriteIdentity {
            identity_source: "trusted_session".to_string(),
            actor_kind: "agent".to_string(),
            subject_actor_id: "human:owner".to_string(),
            delegation_id: "delegation_one".to_string(),
            authority_scope: vec!["forge:run".to_string()],
        };
        let report = kernel
            .record_forge_run_event_with_identity(
                ForgeRunEvent {
                    tenant_id: "tenant_one",
                    actor_id: "agent:builder",
                    run_id: "forge_run_terminal_truth",
                    event: "run_finished",
                    work_item_id: "mobile_terminal_truth",
                    detail: "status=RUN_FAILED_CHECKS turns=3",
                    turn: 3,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
            )
            .expect("terminal event records");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("terminal receipt");
        assert_eq!(
            receipt.payload.get("owner_user_id").map(String::as_str),
            Some("human:owner")
        );
        assert_eq!(
            receipt.payload.get("terminal_status").map(String::as_str),
            Some("RUN_FAILED_CHECKS")
        );
        assert_eq!(receipt.actor_id.as_str(), "agent:builder");
    }

    #[test]
    fn terminal_event_without_a_structured_status_fails_closed_to_unknown() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "tenant_one",
                actor_id: "human:owner",
                run_id: "forge_run_terminal_unknown",
                event: "run_finished",
                work_item_id: "mobile_terminal_truth",
                detail: "finished without a structured outcome",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("terminal event records");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("terminal receipt");
        assert_eq!(
            receipt.payload.get("terminal_status").map(String::as_str),
            Some("RUN_FINISHED_UNKNOWN")
        );
    }

    #[test]
    fn record_forge_run_refusal_with_identity_records_reason_and_actor() {
        let mut kernel = MdxKernel::boot_local();
        let identity = crate::GovernedWriteIdentity::local_demo("human:dev");
        let report = kernel
            .record_forge_run_refusal_with_identity(
                ForgeRunRefusal {
                    tenant_id: "t",
                    actor_id: "human:dev",
                    route: "/forge/runs.json",
                    reason: "Connect a model first.",
                    repo_id: "mdx",
                    requested_run_id: "",
                },
                &identity,
            )
            .expect("refusal records");
        assert_eq!(report.reason, "Connect a model first.");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .cloned()
            .expect("receipt exists");
        assert_eq!(receipt.kind, FORGE_RUN_REFUSED_KIND);
        assert_eq!(receipt.actor_id.as_str(), "human:dev");
        assert_eq!(
            receipt.payload.get("reason").map(String::as_str),
            Some("Connect a model first.")
        );
        assert!(kernel.ledger().verify().is_ok());
    }
}
