//! Fleet run events: the witnessed lifecycle of one ratified plan being
//! built by parallel streams. The grammar is small and fixed - a fleet
//! starts, each stream starts and ends (well or needing attention), the
//! fleet finishes with a rollup. Each stream's actual work is witnessed
//! by its own forge.run.* receipts; the fleet events carry the LINKAGE
//! (fleet_id -> stream_id -> forge_run_id) so the projection can fold
//! lanes without guessing.
//!
//! Build spend discipline: a fleet run only starts against a RATIFIED
//! plan, and only once - the start verb checks both, on the chain.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

/// The fleet lifecycle grammar. Anything else is refused.
pub const FLEET_RUN_EVENTS: &[&str] = &[
    "run_started",
    "run_resumed",
    "stream_started",
    "stream_finished",
    "stream_needs_attention",
    "lane_repair_started",
    "lane_repair_finished",
    "integration_started",
    "integration_finished",
    "review_finished",
    "run_finished",
];

const MAX_DETAIL_CHARS: usize = 1_000;

#[derive(Clone, Copy, Debug)]
pub struct FleetRunEvent<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub fleet_id: &'a str,
    /// One of FLEET_RUN_EVENTS.
    pub event: &'a str,
    /// Which stream this concerns; empty for fleet-level events.
    pub stream_id: &'a str,
    /// The underlying forge run for a stream event; empty otherwise.
    pub forge_run_id: &'a str,
    /// Presence-only detail (status words, counts) - never model text.
    pub detail: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetRunEventReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FleetRunError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownEvent(String),
    NotRatified(String),
    AlreadyStarted(String),
}

impl FleetRunError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("a fleet run event needs {field}"),
            Self::TooLong(field, len, max) => {
                format!("the {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownEvent(other) => format!(
                "a fleet event must be one of {}; got {other}",
                FLEET_RUN_EVENTS.join(", ")
            ),
            Self::NotRatified(id) => {
                format!("the plan for {id} is not ratified - build spend starts only after the yes")
            }
            Self::AlreadyStarted(id) => {
                format!("the fleet {id} already ran - re-plan as a new fleet to run again")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    /// Has this fleet's run already been started on the chain?
    pub fn fleet_run_started(&self, fleet_id: &str) -> bool {
        self.ledger()
            .query()
            .by_kind("fleet.run.event")
            .iter()
            .any(|receipt| {
                receipt.payload.get("fleet_id").map(String::as_str) == Some(fleet_id.trim())
                    && receipt.payload.get("event").map(String::as_str) == Some("run_started")
            })
    }

    pub fn record_fleet_run_event(
        &mut self,
        request: FleetRunEvent<'_>,
    ) -> Result<FleetRunEventReport, FleetRunError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_fleet_run_event_with_identity(request, &identity)
    }

    pub fn record_fleet_run_event_with_identity(
        &mut self,
        request: FleetRunEvent<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<FleetRunEventReport, FleetRunError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("fleet_id", request.fleet_id),
            ("event", request.event),
        ] {
            if value.trim().is_empty() {
                return Err(FleetRunError::Missing(field));
            }
        }
        let event = request.event.trim();
        if !FLEET_RUN_EVENTS.contains(&event) {
            return Err(FleetRunError::UnknownEvent(event.to_string()));
        }
        if request.detail.chars().count() > MAX_DETAIL_CHARS {
            return Err(FleetRunError::TooLong(
                "detail",
                request.detail.chars().count(),
                MAX_DETAIL_CHARS,
            ));
        }
        let fleet_id = request.fleet_id.trim();
        // The spend gates live on the start event: ratified plan, one run.
        if event == "run_started" {
            if !self.fleet_plan_ratified(fleet_id) {
                return Err(FleetRunError::NotRatified(fleet_id.to_string()));
            }
            if self.fleet_run_started(fleet_id) {
                return Err(FleetRunError::AlreadyStarted(fleet_id.to_string()));
            }
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "fleet_run",
                action: ActionKind::RecordFleetRunEvent,
                transition: "RECORD_FLEET_RUN_EVENT",
                kind: "fleet.run.event",
            },
            payload(&[
                ("fleet_id", fleet_id),
                ("event", event),
                ("stream_id", request.stream_id),
                ("forge_run_id", request.forge_run_id),
                ("detail", request.detail),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(FleetRunEventReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FleetPlanDraft, FleetStream};

    fn ratified_fleet(kernel: &mut MdxKernel) -> String {
        let streams = vec![FleetStream {
            stream_id: "s1".to_string(),
            objective: "do the thing".to_string(),
            write_scope: vec!["src/thing.rs".to_string()],
            ..Default::default()
        }];
        let report = kernel
            .record_fleet_plan_draft(
                FleetPlanDraft {
                    tenant_id: "t",
                    actor_id: "human:founder",
                    fleet_id: "",
                    spec: "the thing",
                    goal: "the thing",
                    checks: &[],
                    integration_owned_paths: &[],
                    full_suite_checks: &[],
                    justification: "one stream",
                    requested_width: 0,
                    planner_model: "",
                    bet_id: "",
                    repo_id: "",
                    repo_primary_language: "",
                    language_pack_id: "",
                    repo_profile_suggested_checks: "",
                    repo_profile_artifact_patterns: "",
                    repo_profile_semantic_intelligence: "",
                    repo_profile_semantic_tool_readiness: "",
                    repo_profile_toolchain_readiness: "",
                    repo_profile_proof_plan_status: "",
                    repo_profile_proof_plan_summary: "",
                    repo_profile_source: "",
                    wide_plan_review_required: false,
                    wide_plan_review_status: "not_required",
                    wide_plan_review_reviewer_model: "",
                    wide_plan_review_verdict: "not_required",
                    wide_plan_review_confidence: "none",
                    wide_plan_review_concerns: "",
                },
                &streams,
            )
            .expect("draft");
        report.fleet_id
    }

    #[test]
    fn a_fleet_run_starts_once_and_only_after_the_yes() {
        let mut kernel = MdxKernel::boot_local();
        let fleet_id = ratified_fleet(&mut kernel);
        let start = |kernel: &mut MdxKernel| {
            kernel.record_fleet_run_event(FleetRunEvent {
                tenant_id: "t",
                actor_id: "human:founder",
                fleet_id: &fleet_id,
                event: "run_started",
                stream_id: "",
                forge_run_id: "",
                detail: "streams=1",
            })
        };
        // Before ratification: refused. Build spend starts after the yes.
        assert!(matches!(
            start(&mut kernel),
            Err(FleetRunError::NotRatified(_))
        ));
        let identity = GovernedWriteIdentity::local_demo("human:founder");
        kernel
            .ratify_fleet_plan_with_identity("t", "human:founder", &fleet_id, "go", &identity)
            .expect("ratify");
        assert!(start(&mut kernel).is_ok());
        assert!(kernel.fleet_run_started(&fleet_id));
        // A second start is refused - one run per fleet.
        assert!(matches!(
            start(&mut kernel),
            Err(FleetRunError::AlreadyStarted(_))
        ));

        // Stream linkage rides the event; unknown grammar is refused.
        assert!(
            kernel
                .record_fleet_run_event(FleetRunEvent {
                    tenant_id: "t",
                    actor_id: "agent:fleet_conductor",
                    fleet_id: &fleet_id,
                    event: "stream_started",
                    stream_id: "s1",
                    forge_run_id: "forge_run_000123",
                    detail: "",
                })
                .is_ok()
        );
        assert!(
            kernel
                .record_fleet_run_event(FleetRunEvent {
                    tenant_id: "t",
                    actor_id: "agent:fleet_conductor",
                    fleet_id: &fleet_id,
                    event: "lane_repair_started",
                    stream_id: "",
                    forge_run_id: "",
                    detail: "targets=1 max_attempts=2",
                })
                .is_ok()
        );
        assert!(
            kernel
                .record_fleet_run_event(FleetRunEvent {
                    tenant_id: "t",
                    actor_id: "agent:fleet_conductor",
                    fleet_id: &fleet_id,
                    event: "lane_repair_finished",
                    stream_id: "",
                    forge_run_id: "",
                    detail: "targets=1 repaired=1",
                })
                .is_ok()
        );
        assert!(matches!(
            kernel.record_fleet_run_event(FleetRunEvent {
                tenant_id: "t",
                actor_id: "agent:fleet_conductor",
                fleet_id: &fleet_id,
                event: "stream_exploded",
                stream_id: "s1",
                forge_run_id: "",
                detail: "",
            }),
            Err(FleetRunError::UnknownEvent(_))
        ));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn fleet_run_event_refuses_missing_tenant_id() {
        let mut kernel = MdxKernel::boot_local();
        let fleet_id = ratified_fleet(&mut kernel);
        let res = kernel.record_fleet_run_event(FleetRunEvent {
            tenant_id: "",
            actor_id: "human:founder",
            fleet_id: &fleet_id,
            event: "run_started",
            stream_id: "",
            forge_run_id: "",
            detail: "",
        });
        assert!(matches!(res, Err(FleetRunError::Missing("tenant_id"))));
    }

    #[test]
    fn fleet_run_event_refuses_missing_actor_id() {
        let mut kernel = MdxKernel::boot_local();
        let fleet_id = ratified_fleet(&mut kernel);
        let res = kernel.record_fleet_run_event(FleetRunEvent {
            tenant_id: "t",
            actor_id: "",
            fleet_id: &fleet_id,
            event: "run_started",
            stream_id: "",
            forge_run_id: "",
            detail: "",
        });
        assert!(matches!(res, Err(FleetRunError::Missing("actor_id"))));
    }

    #[test]
    fn fleet_run_event_refuses_missing_fleet_id() {
        let mut kernel = MdxKernel::boot_local();
        let res = kernel.record_fleet_run_event(FleetRunEvent {
            tenant_id: "t",
            actor_id: "human:founder",
            fleet_id: "",
            event: "run_started",
            stream_id: "",
            forge_run_id: "",
            detail: "",
        });
        assert!(matches!(res, Err(FleetRunError::Missing("fleet_id"))));
    }

    #[test]
    fn fleet_run_event_refuses_missing_event() {
        let mut kernel = MdxKernel::boot_local();
        let fleet_id = ratified_fleet(&mut kernel);
        let res = kernel.record_fleet_run_event(FleetRunEvent {
            tenant_id: "t",
            actor_id: "human:founder",
            fleet_id: &fleet_id,
            event: "",
            stream_id: "",
            forge_run_id: "",
            detail: "",
        });
        assert!(matches!(res, Err(FleetRunError::Missing("event"))));
    }

    #[test]
    fn fleet_run_event_refuses_detail_that_is_too_long() {
        let mut kernel = MdxKernel::boot_local();
        let fleet_id = ratified_fleet(&mut kernel);
        let identity = GovernedWriteIdentity::local_demo("human:founder");
        kernel
            .ratify_fleet_plan_with_identity("t", "human:founder", &fleet_id, "go", &identity)
            .expect("ratify");
        let long_detail = "x".repeat(1001);
        let res = kernel.record_fleet_run_event(FleetRunEvent {
            tenant_id: "t",
            actor_id: "human:founder",
            fleet_id: &fleet_id,
            event: "run_started",
            stream_id: "",
            forge_run_id: "",
            detail: &long_detail,
        });
        assert!(matches!(
            res,
            Err(FleetRunError::TooLong("detail", 1001, 1000))
        ));
    }

    #[test]
    fn record_fleet_run_event_with_identity_happy_path_after_ratify() {
        let mut kernel = MdxKernel::boot_local();
        let fleet_id = ratified_fleet(&mut kernel);
        let identity = GovernedWriteIdentity::local_demo("human:founder");
        kernel
            .ratify_fleet_plan_with_identity("t", "human:founder", &fleet_id, "go", &identity)
            .expect("ratify");
        let report = kernel.record_fleet_run_event_with_identity(
            FleetRunEvent {
                tenant_id: "t",
                actor_id: "human:founder",
                fleet_id: &fleet_id,
                event: "run_started",
                stream_id: "",
                forge_run_id: "",
                detail: "streams=1",
            },
            &identity,
        );
        assert!(report.is_ok());
        assert!(kernel.fleet_run_started(&fleet_id));
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn fleet_run_started_returns_false_before_and_true_after_start() {
        let mut kernel = MdxKernel::boot_local();
        let fleet_id = ratified_fleet(&mut kernel);
        let identity = GovernedWriteIdentity::local_demo("human:founder");
        kernel
            .ratify_fleet_plan_with_identity("t", "human:founder", &fleet_id, "go", &identity)
            .expect("ratify");
        assert!(!kernel.fleet_run_started(&fleet_id));
        kernel
            .record_fleet_run_event_with_identity(
                FleetRunEvent {
                    tenant_id: "t",
                    actor_id: "human:founder",
                    fleet_id: &fleet_id,
                    event: "run_started",
                    stream_id: "",
                    forge_run_id: "",
                    detail: "",
                },
                &identity,
            )
            .expect("start");
        assert!(kernel.fleet_run_started(&fleet_id));
    }
}
