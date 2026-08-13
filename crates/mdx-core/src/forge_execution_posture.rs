// The execution posture receipt: which safety-relevant execution switches
// the Forge runtime is actually operating under, recorded on the ledger so
// a posture flip is an audit event instead of an invisible env change. The
// kernel mints a receipt only when the observed posture DIFFERS from the
// last recorded one, so the chain carries every transition exactly once.
// This module records posture; it grants nothing. Host execution, live
// delivery, and run-start authority are enforced at their own gates.
use crate::strategy_board::BoardWrite;
use crate::{ActionKind, MdxKernel, StorageProvider, payload};

/// One observed posture snapshot, assembled by the caller from the real
/// environment at a moment that matters (boot, run start, live delivery).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeExecutionPosture {
    /// How model-influenced commands execute on this host:
    /// "raw_host_opt_in" | "scrubbed_sandboxed" | "scrubbed_unconfined"
    /// | "container".
    pub host_exec: String,
    /// Whether live source-host delivery is enabled in this process.
    pub source_host_live: bool,
    /// Whether the run-start switch is open.
    pub run_start_enabled: bool,
    /// Whether fleet resume after crash is armed.
    pub fleet_resume: bool,
    /// Whether kernel boundary snapshots are armed.
    pub kernel_snapshot: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeExecutionPostureReport {
    pub receipt_id: String,
    pub changed: bool,
    pub changed_fields: Vec<String>,
}

const POSTURE_KIND: &str = "forge.execution_posture.recorded";
const FIELDS: [&str; 5] = [
    "host_exec",
    "source_host_live",
    "run_start_enabled",
    "fleet_resume",
    "kernel_snapshot",
];

impl<S: StorageProvider> MdxKernel<S> {
    /// Record the observed execution posture if it differs from the last
    /// recorded posture. Returns None when nothing changed (no receipt is
    /// minted), so callers can invoke this on every boot and gate entry
    /// without flooding the ledger.
    pub fn record_forge_execution_posture(
        &mut self,
        tenant_id: &str,
        actor_id: &str,
        trigger: &str,
        posture: &ForgeExecutionPosture,
    ) -> Option<ForgeExecutionPostureReport> {
        let bool_str = |value: bool| if value { "true" } else { "false" };
        let current: Vec<(&str, String)> = vec![
            ("host_exec", posture.host_exec.clone()),
            (
                "source_host_live",
                bool_str(posture.source_host_live).to_string(),
            ),
            (
                "run_start_enabled",
                bool_str(posture.run_start_enabled).to_string(),
            ),
            ("fleet_resume", bool_str(posture.fleet_resume).to_string()),
            (
                "kernel_snapshot",
                bool_str(posture.kernel_snapshot).to_string(),
            ),
        ];

        let last = self
            .ledger()
            .query()
            .by_kind(POSTURE_KIND)
            .iter()
            .last()
            .map(|receipt| receipt.payload.clone());
        let changed_fields: Vec<String> = match &last {
            None => FIELDS.iter().map(|field| field.to_string()).collect(),
            Some(previous) => current
                .iter()
                .filter(|(field, value)| {
                    previous.get(*field).map(String::as_str) != Some(value.as_str())
                })
                .map(|(field, _)| field.to_string())
                .collect(),
        };
        if changed_fields.is_empty() {
            return None;
        }

        let changed_list = changed_fields.join(",");
        let trigger_value = if trigger.trim().is_empty() {
            "unspecified"
        } else {
            trigger.trim()
        };
        let mut fields: Vec<(&str, &str)> = current
            .iter()
            .map(|(field, value)| (*field, value.as_str()))
            .collect();
        fields.push(("trigger", trigger_value));
        fields.push(("changed_fields", &changed_list));
        fields.push((
            "first_observation",
            if last.is_none() { "true" } else { "false" },
        ));
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id,
                actor_id,
                loop_id: "forge_execution_posture",
                action: ActionKind::RecordForgeExecutionPosture,
                transition: "RECORD_FORGE_EXECUTION_POSTURE",
                kind: POSTURE_KIND,
            },
            payload(&fields),
        );
        Some(ForgeExecutionPostureReport {
            receipt_id: receipt.0,
            changed: true,
            changed_fields,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn posture(host_exec: &str, live: bool) -> ForgeExecutionPosture {
        ForgeExecutionPosture {
            host_exec: host_exec.to_string(),
            source_host_live: live,
            run_start_enabled: true,
            fleet_resume: false,
            kernel_snapshot: false,
        }
    }

    #[test]
    fn the_first_observation_mints_and_an_unchanged_posture_does_not() {
        let mut kernel = MdxKernel::boot_local();
        let first = kernel
            .record_forge_execution_posture(
                "t",
                "server",
                "boot",
                &posture("scrubbed_sandboxed", false),
            )
            .expect("first observation mints a receipt");
        assert!(first.changed);
        assert_eq!(first.changed_fields.len(), 5);

        let unchanged = kernel.record_forge_execution_posture(
            "t",
            "server",
            "run_start",
            &posture("scrubbed_sandboxed", false),
        );
        assert!(unchanged.is_none(), "an unchanged posture mints nothing");
        assert_eq!(
            kernel
                .ledger()
                .query()
                .by_kind("forge.execution_posture.recorded")
                .len(),
            1
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn a_posture_flip_mints_a_receipt_naming_exactly_what_changed() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_execution_posture(
                "t",
                "server",
                "boot",
                &posture("scrubbed_sandboxed", false),
            )
            .expect("first observation mints");
        let flipped = kernel
            .record_forge_execution_posture(
                "t",
                "server",
                "live_delivery",
                &posture("raw_host_opt_in", true),
            )
            .expect("a flip mints a receipt");
        assert_eq!(
            flipped.changed_fields,
            vec!["host_exec".to_string(), "source_host_live".to_string()]
        );
        let receipts = kernel
            .ledger()
            .query()
            .by_kind("forge.execution_posture.recorded");
        let last = receipts.iter().last().expect("receipt exists");
        assert_eq!(
            last.payload.get("changed_fields").map(String::as_str),
            Some("host_exec,source_host_live")
        );
        assert_eq!(
            last.payload.get("trigger").map(String::as_str),
            Some("live_delivery")
        );
        assert_eq!(
            last.payload.get("first_observation").map(String::as_str),
            Some("false")
        );
        assert!(kernel.ledger().verify().is_ok());
    }
}
