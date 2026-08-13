//! Operator controls for a running Forge harness run. A run executes a
//! multi-turn loop that can take minutes; this rail lets the engineer
//! steer it without killing it from the outside. Two controls:
//!
//! - stop: end the run cleanly at the next turn boundary, with a reason
//!   on the record - no half-applied edit, no orphaned worktree.
//! - steer: hand the agent a note it picks up on its next turn, the same
//!   way you would lean over and redirect a pairing partner.
//!
//! The control is a governed record. The loop holds no lock while the
//! model thinks; between turns it reads the latest control for its run
//! and acts on it once. The receipt is the durable channel, so a control
//! survives a restart and reads back exactly as it was given. A server may
//! additionally signal its local process supervisor to interrupt an active
//! command without weakening receipt authority.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

/// The controls an operator can send a running run.
pub const FORGE_RUN_CONTROLS: &[&str] = &["stop", "steer"];

const MAX_NOTE_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct ForgeRunControl<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub run_id: &'a str,
    /// One of FORGE_RUN_CONTROLS.
    pub control: &'a str,
    /// Guidance for a steer; ignored for a stop (but a stop may carry a
    /// reason here for the record).
    pub note: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeRunControlReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub run_id: String,
    pub control: String,
}

/// A control read back off the chain, for the loop to apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingForgeRunControl {
    pub receipt_id: String,
    pub control: String,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForgeRunControlError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownControl(String),
}

impl ForgeRunControlError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("steer a run: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownControl(other) => format!(
                "a control must be one of {}; got {other}",
                FORGE_RUN_CONTROLS.join(", ")
            ),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    /// The controls an operator has sent this run, oldest first, so the
    /// loop can apply any it has not seen yet. A stop wins over a steer
    /// when both are pending; the caller decides, this just reports.
    pub fn forge_run_controls(&self, run_id: &str) -> Vec<PendingForgeRunControl> {
        let run_id = run_id.trim();
        if run_id.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for receipt in self.ledger().query().by_kind("forge.run.control").iter() {
            if receipt.payload.get("run_id").map(String::as_str) == Some(run_id) {
                out.push(PendingForgeRunControl {
                    receipt_id: receipt.receipt_id.clone(),
                    control: receipt.payload.get("control").cloned().unwrap_or_default(),
                    note: receipt.payload.get("note").cloned().unwrap_or_default(),
                });
            }
        }
        out
    }

    pub fn record_forge_run_control(
        &mut self,
        request: ForgeRunControl<'_>,
    ) -> Result<ForgeRunControlReport, ForgeRunControlError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_forge_run_control_with_identity(request, &identity)
    }

    pub fn record_forge_run_control_with_identity(
        &mut self,
        request: ForgeRunControl<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeRunControlReport, ForgeRunControlError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("run_id", request.run_id),
            ("control", request.control),
        ] {
            if value.trim().is_empty() {
                return Err(ForgeRunControlError::Missing(field));
            }
        }
        let control = request.control.trim();
        if !FORGE_RUN_CONTROLS.contains(&control) {
            return Err(ForgeRunControlError::UnknownControl(control.to_string()));
        }
        if request.note.chars().count() > MAX_NOTE_CHARS {
            return Err(ForgeRunControlError::TooLong(
                "note",
                request.note.chars().count(),
                MAX_NOTE_CHARS,
            ));
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "forge_run_control",
                action: ActionKind::RecordForgeRunControl,
                transition: "RECORD_FORGE_RUN_CONTROL",
                kind: "forge.run.control",
            },
            payload(&[
                ("run_id", request.run_id.trim()),
                ("control", control),
                ("note", request.note),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ForgeRunControlReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            run_id: request.run_id.trim().to_string(),
            control: control.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_reads_back_the_controls_an_operator_sent_it() {
        let mut kernel = MdxKernel::boot_local();
        assert!(kernel.forge_run_controls("forge_run_1").is_empty());

        kernel
            .record_forge_run_control(ForgeRunControl {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: "forge_run_1",
                control: "steer",
                note: "also cover the empty-input case",
            })
            .expect("steer");
        kernel
            .record_forge_run_control(ForgeRunControl {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: "forge_run_1",
                control: "stop",
                note: "good enough, wrap it up",
            })
            .expect("stop");
        // A control for a different run is not picked up here.
        kernel
            .record_forge_run_control(ForgeRunControl {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: "forge_run_2",
                control: "stop",
                note: "",
            })
            .expect("other run");

        let controls = kernel.forge_run_controls("forge_run_1");
        assert_eq!(controls.len(), 2);
        assert_eq!(controls[0].control, "steer");
        assert_eq!(controls[0].note, "also cover the empty-input case");
        assert_eq!(controls[1].control, "stop");

        // An unknown control is refused.
        let bad = kernel.record_forge_run_control(ForgeRunControl {
            tenant_id: "t",
            actor_id: "human:eng",
            run_id: "forge_run_1",
            control: "delete-everything",
            note: "",
        });
        assert!(matches!(bad, Err(ForgeRunControlError::UnknownControl(_))));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn record_forge_run_control_refuses_missing_tenant_id() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.record_forge_run_control(ForgeRunControl {
            tenant_id: "",
            actor_id: "human:eng",
            run_id: "r1",
            control: "stop",
            note: "",
        });
        assert!(matches!(
            bad,
            Err(ForgeRunControlError::Missing("tenant_id"))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn record_forge_run_control_refuses_missing_actor_id() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.record_forge_run_control(ForgeRunControl {
            tenant_id: "t",
            actor_id: " ",
            run_id: "r1",
            control: "stop",
            note: "",
        });
        assert!(matches!(
            bad,
            Err(ForgeRunControlError::Missing("actor_id"))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn record_forge_run_control_refuses_missing_run_id() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.record_forge_run_control(ForgeRunControl {
            tenant_id: "t",
            actor_id: "human:eng",
            run_id: "",
            control: "stop",
            note: "",
        });
        assert!(matches!(bad, Err(ForgeRunControlError::Missing("run_id"))));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn record_forge_run_control_refuses_missing_control() {
        let mut kernel = MdxKernel::boot_local();
        let bad = kernel.record_forge_run_control(ForgeRunControl {
            tenant_id: "t",
            actor_id: "human:eng",
            run_id: "r1",
            control: "  ",
            note: "",
        });
        assert!(matches!(bad, Err(ForgeRunControlError::Missing("control"))));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn record_forge_run_control_refuses_too_long_note() {
        let mut kernel = MdxKernel::boot_local();
        let long_note = "x".repeat(2001);
        let bad = kernel.record_forge_run_control(ForgeRunControl {
            tenant_id: "t",
            actor_id: "human:eng",
            run_id: "r1",
            control: "steer",
            note: &long_note,
        });
        assert!(matches!(
            bad,
            Err(ForgeRunControlError::TooLong("note", 2001, 2000))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn record_forge_run_control_with_identity_records_happy_path() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:eng");
        let report = kernel
            .record_forge_run_control_with_identity(
                ForgeRunControl {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    run_id: "r1",
                    control: "stop",
                    note: "wrap it",
                },
                &identity,
            )
            .expect("with_identity happy");
        assert_eq!(report.run_id, "r1");
        assert_eq!(report.control, "stop");
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn forge_run_controls_returns_empty_vec_for_blank_trimmed_run_id() {
        let kernel = MdxKernel::boot_local();
        assert!(kernel.forge_run_controls("").is_empty());
        assert!(kernel.forge_run_controls("   ").is_empty());
    }
}
