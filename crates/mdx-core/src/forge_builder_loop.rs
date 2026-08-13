//! Forge Builder Loops: productized, receipt-backed loop objects that wrap
//! the existing Forge runtime without creating a second execution system.
//!
//! A tick is real kernel state. A Golden Builder Loop can then attach the tick
//! to Forge run admission through the existing run route. Later PRs can attach
//! fleets, missions, checker verdicts, and external machine evidence to the
//! same loop id.

use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

pub const BUILDER_LOOP_TRIGGERS: &[&str] = &["manual", "schedule", "event", "chain"];
pub const BUILDER_LOOP_TICK_KIND: &str = "forge.builder_loop.tick_recorded";
pub const BUILDER_LOOP_RUN_ATTACHMENT_KIND: &str = "forge.builder_loop.run_attached";
pub const BUILDER_LOOP_CHECKER_ATTACHMENT_KIND: &str = "forge.builder_loop.checker_attached";
pub const BUILDER_LOOP_FLEET_ATTACHMENT_KIND: &str = "forge.builder_loop.fleet_attached";
pub const BUILDER_LOOP_EXTERNAL_TRIAL_ATTACHMENT_KIND: &str =
    "forge.builder_loop.external_trial_attached";

const MAX_REASON_CHARS: usize = 1000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuilderLoopTemplate {
    pub id: &'static str,
    pub title: &'static str,
    pub trigger: &'static str,
    pub goal: &'static str,
    pub constraints: &'static [&'static str],
    pub stop_condition: &'static str,
    pub authority_level: &'static str,
    pub execution_geometry: &'static str,
    pub allowed_tools: &'static [&'static str],
    pub denied_tools: &'static [&'static str],
    pub max_iterations: u32,
    pub time_budget_minutes: u32,
    pub maker_role: &'static str,
    pub checker_role: &'static str,
    pub human_gate: &'static str,
    pub metric: &'static str,
    pub output_policy: &'static str,
    pub legacy_recipe_id: &'static str,
}

pub const BUILDER_LOOP_TEMPLATES: &[BuilderLoopTemplate] = &[
    BuilderLoopTemplate {
        id: "quality_sweep",
        title: "Quality sweep",
        trigger: "manual",
        goal: "Find small quality issues, fix only the safe ones, and stop at human review.",
        constraints: &[
            "isolated Forge worktree only",
            "no deployment authority",
            "no production writes",
            "proof-before-finish required",
        ],
        stop_condition: "selected checks pass or the loop hits its iteration budget",
        authority_level: "assisted_local",
        execution_geometry: "single",
        allowed_tools: &["repo_read", "patch", "declared_checks"],
        denied_tools: &["deployment", "production_write", "secret_export"],
        max_iterations: 3,
        time_budget_minutes: 30,
        maker_role: "builder",
        checker_role: "reviewer",
        human_gate: "required_before_ship",
        metric: "check_passed_count",
        output_policy: "branch_or_honest_no_change",
        legacy_recipe_id: "quality_cleanup",
    },
    BuilderLoopTemplate {
        id: "local_check_recovery",
        title: "Local check recovery",
        trigger: "manual",
        goal: "Recover a failing local check with the smallest behavior-preserving change.",
        constraints: &[
            "operator-selected check only",
            "bounded repair attempts",
            "checker verdict required before ready state",
            "no source-host delivery",
        ],
        stop_condition: "target check passes or bounded repair refuses",
        authority_level: "assisted_local",
        execution_geometry: "single",
        allowed_tools: &["repo_read", "patch", "target_check"],
        denied_tools: &[
            "wide_fleet",
            "deployment",
            "production_write",
            "secret_export",
        ],
        max_iterations: 4,
        time_budget_minutes: 45,
        maker_role: "builder",
        checker_role: "reviewer",
        human_gate: "required_before_ship",
        metric: "target_check_status",
        output_policy: "branch_or_refusal_with_failure_receipt",
        legacy_recipe_id: "bug_hunt",
    },
    BuilderLoopTemplate {
        id: "pr_review_babysitter",
        title: "PR review babysitter",
        trigger: "manual",
        goal: "Watch review feedback, propose safe fixes, and keep source-host action behind approval.",
        constraints: &[
            "source-host writes disabled until explicit approval",
            "review comments are inputs, not authority",
            "checker must be separate from maker",
            "human decides final response",
        ],
        stop_condition: "review comments are answered by a patch proposal or blocked note",
        authority_level: "assisted_review",
        execution_geometry: "candidates",
        allowed_tools: &["repo_read", "patch", "declared_checks", "review_packet"],
        denied_tools: &[
            "source_host_write",
            "deployment",
            "production_write",
            "secret_export",
        ],
        max_iterations: 3,
        time_budget_minutes: 40,
        maker_role: "builder",
        checker_role: "reviewer",
        human_gate: "required_before_source_host_write",
        metric: "review_comment_resolution_count",
        output_policy: "review_packet_and_branch_only",
        legacy_recipe_id: "",
    },
    BuilderLoopTemplate {
        id: "fleet_quality_sweep",
        title: "Fleet quality sweep",
        trigger: "manual",
        goal: "Plan, ratify, and run a small Forge fleet through existing lane, repair, integration, and review machinery.",
        constraints: &[
            "existing fleet planner and conductor only",
            "wide fleets require existing wide-review gates",
            "fresh-context integration review required",
            "human ratification required before ship",
        ],
        stop_condition: "fleet lanes converge, review blocks, or the fleet budget is reached",
        authority_level: "assisted_fleet",
        execution_geometry: "fleet",
        allowed_tools: &[
            "repo_read",
            "fleet_plan",
            "fleet_run",
            "control_plane_projection",
            "review_packet",
        ],
        denied_tools: &["deployment", "production_write", "secret_export"],
        max_iterations: 4,
        time_budget_minutes: 60,
        maker_role: "fleet_builder",
        checker_role: "integration_reviewer",
        human_gate: "required_before_ship",
        metric: "fleet_lane_clean_count",
        output_policy: "fleet_receipt_rollup_and_human_decision",
        legacy_recipe_id: "",
    },
    BuilderLoopTemplate {
        id: "external_machine_trial",
        title: "External machine trial",
        trigger: "manual",
        goal: "Preflight a Codex CLI external worker and import one tiny quarantined eval result without trusting the output.",
        constraints: &[
            "external adapter execution denied until live approval",
            "isolated Codex home and isolated worktree required",
            "transcript and artifacts stay quarantined",
            "MDx quality gates and principal review decide acceptance",
        ],
        stop_condition: "external output is quarantined or the preflight refuses",
        authority_level: "assisted_external_trial",
        execution_geometry: "external_trial",
        allowed_tools: &["provider_preflight", "eval_result_ingestion", "quarantine"],
        denied_tools: &[
            "external_adapter_execution",
            "network_call",
            "secret_export",
            "production_write",
        ],
        max_iterations: 1,
        time_budget_minutes: 15,
        maker_role: "external_worker",
        checker_role: "mdx_principal_reviewer",
        human_gate: "required_before_scoreboard_acceptance",
        metric: "quarantined_eval_result_count",
        output_policy: "quarantined_eval_evidence_only",
        legacy_recipe_id: "",
    },
];

#[derive(Clone, Copy, Debug)]
pub struct BuilderLoopTick<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub loop_id: &'a str,
    pub trigger: &'a str,
    pub reason: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuilderLoopTickReport {
    pub status: &'static str,
    pub loop_id: String,
    pub tick_id: String,
    pub tick_receipt_id: String,
    pub policy_decision_id: String,
    pub terminal_state: &'static str,
    pub blocked_reason: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct BuilderLoopRunAttachment<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub loop_id: &'a str,
    pub tick_receipt_id: &'a str,
    pub run_admission_status: &'a str,
    pub run_id: &'a str,
    pub run_started_receipt_id: &'a str,
    pub blocked_reason: &'a str,
    pub run_projection_route: &'a str,
    pub stream_route: &'a str,
    pub review_packet_route: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuilderLoopRunAttachmentReport {
    pub status: &'static str,
    pub loop_id: String,
    pub attachment_receipt_id: String,
    pub policy_decision_id: String,
    pub run_admission_status: String,
    pub run_id: String,
    pub terminal_state: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct BuilderLoopCheckerAttachment<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub loop_id: &'a str,
    pub tick_receipt_id: &'a str,
    pub run_id: &'a str,
    pub maker_actor_id: &'a str,
    pub checker_actor_id: &'a str,
    pub checker_status: &'a str,
    pub checker_verdict: &'a str,
    pub checker_confidence: &'a str,
    pub checker_panel_receipt_id: &'a str,
    pub blocked_reason: &'a str,
    pub review_panel_route: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuilderLoopCheckerAttachmentReport {
    pub status: &'static str,
    pub loop_id: String,
    pub checker_attachment_receipt_id: String,
    pub policy_decision_id: String,
    pub checker_status: String,
    pub checker_verdict: String,
    pub terminal_state: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct BuilderLoopFleetAttachment<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub loop_id: &'a str,
    pub tick_receipt_id: &'a str,
    pub fleet_id: &'a str,
    pub fleet_plan_status: &'a str,
    pub fleet_plan_receipt_id: &'a str,
    pub fleet_run_status: &'a str,
    pub fleet_started_receipt_id: &'a str,
    pub blocked_reason: &'a str,
    pub fleet_plan_projection_route: &'a str,
    pub fleet_projection_route: &'a str,
    pub fleet_capacity_route: &'a str,
    pub control_plane_route: &'a str,
    pub lane_count: u32,
    pub lanes_needing_attention: u32,
    pub active_worker_count: u32,
    pub queue_depth: u32,
    pub repair_started_count: u32,
    pub repair_finished_count: u32,
    pub repair_target_count: u32,
    pub repair_repaired_count: u32,
    pub integration_state: &'a str,
    pub review_verdict: &'a str,
    pub wide_review_required: bool,
    pub human_ratification_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuilderLoopFleetAttachmentReport {
    pub status: &'static str,
    pub loop_id: String,
    pub fleet_attachment_receipt_id: String,
    pub policy_decision_id: String,
    pub fleet_id: String,
    pub fleet_run_status: String,
    pub terminal_state: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct BuilderLoopExternalTrialAttachment<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub loop_id: &'a str,
    pub tick_receipt_id: &'a str,
    pub runner_profile_id: &'a str,
    pub codex_profile_id: &'a str,
    pub adapter_status: &'a str,
    pub external_machine_status: &'a str,
    pub provider_preflight_status: &'a str,
    pub result_ingestion_status: &'a str,
    pub result_receipt_id: &'a str,
    pub output_artifact_ref: &'a str,
    pub transcript_ref: &'a str,
    pub blocked_reason: &'a str,
    pub external_machine_route: &'a str,
    pub provider_preflight_route: &'a str,
    pub result_ingestion_route: &'a str,
    pub result_projection_route: &'a str,
    pub scoreboard_route: &'a str,
    pub output_quarantined: bool,
    pub external_output_consumable: bool,
    pub mdx_quality_gates_required: bool,
    pub mdx_quality_gates_passed: bool,
    pub accepted_for_scoreboard: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuilderLoopExternalTrialAttachmentReport {
    pub status: &'static str,
    pub loop_id: String,
    pub external_trial_attachment_receipt_id: String,
    pub policy_decision_id: String,
    pub runner_profile_id: String,
    pub result_receipt_id: String,
    pub terminal_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BuilderLoopError {
    Missing(&'static str),
    UnknownLoop(String),
    UnknownTrigger(String),
    UnknownTickReceipt(String),
    UnknownRunAttachment(String),
    UnknownCheckerPanelReceipt(String),
    MakerCheckerSameActor(String),
    InvalidCheckerStatus(String),
    TooLong(&'static str, usize, usize),
}

impl BuilderLoopError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("builder loop tick missing {field}"),
            Self::UnknownLoop(loop_id) => format!("unknown Builder Loop template {loop_id}"),
            Self::UnknownTrigger(trigger) => format!(
                "builder loop trigger must be one of {}; got {trigger}",
                BUILDER_LOOP_TRIGGERS.join(", ")
            ),
            Self::UnknownTickReceipt(receipt_id) => {
                format!("builder loop tick receipt {receipt_id} is unknown")
            }
            Self::UnknownRunAttachment(run_id) => {
                format!("builder loop run attachment for run {run_id} is unknown")
            }
            Self::UnknownCheckerPanelReceipt(receipt_id) => {
                format!("builder loop checker panel receipt {receipt_id} is unknown or mismatched")
            }
            Self::MakerCheckerSameActor(actor_id) => {
                format!(
                    "builder loop maker and checker must be separate actors; both were {actor_id}"
                )
            }
            Self::InvalidCheckerStatus(status) => {
                format!("builder loop checker status {status} is unsupported")
            }
            Self::TooLong(field, len, max) => {
                format!("builder loop {field} is {len} characters; the limit is {max}")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuilderLoopState {
    pub loop_id: String,
    pub status: String,
    pub tick_count: usize,
    pub last_tick_id: String,
    pub last_tick_receipt_id: String,
    pub last_trigger: String,
    pub blocked_reason: String,
    pub needs_human: bool,
    pub attached_run_id: String,
    pub run_admission_status: String,
    pub run_started_receipt_id: String,
    pub run_projection_route: String,
    pub stream_route: String,
    pub attached_fleet_run_id: String,
    pub fleet_plan_status: String,
    pub fleet_plan_receipt_id: String,
    pub fleet_run_status: String,
    pub fleet_started_receipt_id: String,
    pub fleet_plan_projection_route: String,
    pub fleet_projection_route: String,
    pub fleet_capacity_route: String,
    pub control_plane_route: String,
    pub fleet_lane_count: u32,
    pub fleet_lanes_needing_attention: u32,
    pub fleet_active_worker_count: u32,
    pub fleet_queue_depth: u32,
    pub fleet_repair_started_count: u32,
    pub fleet_repair_finished_count: u32,
    pub fleet_repair_target_count: u32,
    pub fleet_repair_repaired_count: u32,
    pub fleet_integration_state: String,
    pub fleet_review_verdict: String,
    pub fleet_wide_review_required: bool,
    pub fleet_human_ratification_required: bool,
    pub external_runner_profile_id: String,
    pub external_codex_profile_id: String,
    pub external_adapter_status: String,
    pub external_machine_status: String,
    pub external_provider_preflight_status: String,
    pub external_result_ingestion_status: String,
    pub external_result_receipt_id: String,
    pub external_output_artifact_ref: String,
    pub external_transcript_ref: String,
    pub external_machine_route: String,
    pub external_provider_preflight_route: String,
    pub external_result_ingestion_route: String,
    pub external_result_projection_route: String,
    pub external_scoreboard_route: String,
    pub external_output_quarantined: bool,
    pub external_output_consumable: bool,
    pub external_mdx_quality_gates_required: bool,
    pub external_mdx_quality_gates_passed: bool,
    pub external_accepted_for_scoreboard: bool,
    pub attached_mission_id: String,
    pub attached_review_packet_id: String,
    pub review_packet_route: String,
    pub checker_status: String,
    pub checker_verdict: String,
    pub checker_confidence: String,
    pub checker_attachment_receipt_id: String,
    pub checker_panel_receipt_id: String,
    pub checker_blocked_reason: String,
    pub review_panel_route: String,
    pub source_receipt_ids: Vec<String>,
}

pub fn builder_loop_template(id: &str) -> Option<&'static BuilderLoopTemplate> {
    BUILDER_LOOP_TEMPLATES
        .iter()
        .find(|template| template.id == id.trim())
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_builder_loop_tick(
        &mut self,
        request: BuilderLoopTick<'_>,
    ) -> Result<BuilderLoopTickReport, BuilderLoopError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_builder_loop_tick_with_identity(request, &identity)
    }

    pub fn record_builder_loop_tick_with_identity(
        &mut self,
        request: BuilderLoopTick<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<BuilderLoopTickReport, BuilderLoopError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("loop_id", request.loop_id),
            ("trigger", request.trigger),
        ] {
            if value.trim().is_empty() {
                return Err(BuilderLoopError::Missing(field));
            }
        }
        let template = builder_loop_template(request.loop_id)
            .ok_or_else(|| BuilderLoopError::UnknownLoop(request.loop_id.trim().to_string()))?;
        let trigger = request.trigger.trim();
        if !BUILDER_LOOP_TRIGGERS.contains(&trigger) {
            return Err(BuilderLoopError::UnknownTrigger(trigger.to_string()));
        }
        let reason_len = request.reason.chars().count();
        if reason_len > MAX_REASON_CHARS {
            return Err(BuilderLoopError::TooLong(
                "reason",
                reason_len,
                MAX_REASON_CHARS,
            ));
        }

        let tick_id = self.ids.next("builder_loop_tick");
        let correlation = crate::CorrelationIds {
            tenant_id: crate::TenantId::new(request.tenant_id),
            trace_id: crate::TraceId::new(self.ids.next("trace")),
            actor_id: crate::ActorId::new(request.actor_id),
            loop_id: crate::LoopId::new("forge_builder_loop"),
            workflow_id: crate::WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(crate::LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordForgeBuilderLoopTick);
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_FORGE_BUILDER_LOOP_TICK",
            &correlation,
            &decision,
            BUILDER_LOOP_TICK_KIND,
            payload(&[
                ("tick_id", &tick_id),
                ("loop_id", template.id),
                ("loop_title", template.title),
                ("trigger", trigger),
                ("template_trigger", template.trigger),
                ("goal", template.goal),
                ("stop_condition", template.stop_condition),
                ("authority_level", template.authority_level),
                ("execution_geometry", template.execution_geometry),
                ("max_iterations", &template.max_iterations.to_string()),
                (
                    "time_budget_minutes",
                    &template.time_budget_minutes.to_string(),
                ),
                ("maker_role", template.maker_role),
                ("checker_role", template.checker_role),
                ("human_gate", template.human_gate),
                ("metric", template.metric),
                ("output_policy", template.output_policy),
                ("legacy_recipe_id", template.legacy_recipe_id),
                ("reason", request.reason),
                ("source_route", "/forge/builder-loops/tick.json"),
                ("projection_route", "/forge/builder-loops.json"),
                ("catalog_route", "/forge/builder-loops.json"),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("terminal_state", "TICK_RECORDED_EXECUTION_WIRING_PENDING"),
                (
                    "blocked_reason",
                    "golden_builder_loop_runtime_lands_in_pr_2",
                ),
                ("attached_run_id", ""),
                ("attached_fleet_run_id", ""),
                ("attached_mission_id", ""),
                ("attached_review_packet_id", ""),
                ("authority_opened", "none"),
                ("live_provider_calls_allowed", "false"),
                ("adapter_execution_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_builder_loop_tick_run(&run_id);
        Ok(BuilderLoopTickReport {
            status: "BUILDER_LOOP_TICK_RECORDED_EXECUTION_BLOCKED",
            loop_id: template.id.to_string(),
            tick_id,
            tick_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            terminal_state: "TICK_RECORDED_EXECUTION_WIRING_PENDING",
            blocked_reason: "golden_builder_loop_runtime_lands_in_pr_2",
        })
    }

    pub fn builder_loop_states(&self) -> Vec<BuilderLoopState> {
        BUILDER_LOOP_TEMPLATES
            .iter()
            .map(|template| self.builder_loop_state(template.id))
            .collect()
    }

    pub fn builder_loop_state(&self, loop_id: &str) -> BuilderLoopState {
        let tick_receipts = self
            .storage
            .ledger()
            .query()
            .by_kind(BUILDER_LOOP_TICK_KIND)
            .into_iter()
            .filter(|receipt| receipt.payload.get("loop_id").map(String::as_str) == Some(loop_id))
            .collect::<Vec<_>>();
        let attachment_receipts = self
            .storage
            .ledger()
            .query()
            .by_kind(BUILDER_LOOP_RUN_ATTACHMENT_KIND)
            .into_iter()
            .filter(|receipt| receipt.payload.get("loop_id").map(String::as_str) == Some(loop_id))
            .collect::<Vec<_>>();
        let checker_receipts = self
            .storage
            .ledger()
            .query()
            .by_kind(BUILDER_LOOP_CHECKER_ATTACHMENT_KIND)
            .into_iter()
            .filter(|receipt| receipt.payload.get("loop_id").map(String::as_str) == Some(loop_id))
            .collect::<Vec<_>>();
        let fleet_receipts = self
            .storage
            .ledger()
            .query()
            .by_kind(BUILDER_LOOP_FLEET_ATTACHMENT_KIND)
            .into_iter()
            .filter(|receipt| receipt.payload.get("loop_id").map(String::as_str) == Some(loop_id))
            .collect::<Vec<_>>();
        let external_receipts = self
            .storage
            .ledger()
            .query()
            .by_kind(BUILDER_LOOP_EXTERNAL_TRIAL_ATTACHMENT_KIND)
            .into_iter()
            .filter(|receipt| receipt.payload.get("loop_id").map(String::as_str) == Some(loop_id))
            .collect::<Vec<_>>();
        let mut source_receipt_ids = tick_receipts
            .iter()
            .map(|receipt| receipt.receipt_id.clone())
            .collect::<Vec<_>>();
        source_receipt_ids.extend(
            attachment_receipts
                .iter()
                .map(|receipt| receipt.receipt_id.clone()),
        );
        source_receipt_ids.extend(
            checker_receipts
                .iter()
                .map(|receipt| receipt.receipt_id.clone()),
        );
        source_receipt_ids.extend(
            fleet_receipts
                .iter()
                .map(|receipt| receipt.receipt_id.clone()),
        );
        source_receipt_ids.extend(
            external_receipts
                .iter()
                .map(|receipt| receipt.receipt_id.clone()),
        );
        if let Some(last) = tick_receipts.last() {
            let attachment = attachment_receipts.iter().rev().find(|receipt| {
                receipt.payload.get("tick_receipt_id").map(String::as_str)
                    == Some(last.receipt_id.as_str())
            });
            let checker = checker_receipts.iter().rev().find(|receipt| {
                receipt.payload.get("tick_receipt_id").map(String::as_str)
                    == Some(last.receipt_id.as_str())
            });
            let fleet = fleet_receipts.iter().rev().find(|receipt| {
                receipt.payload.get("tick_receipt_id").map(String::as_str)
                    == Some(last.receipt_id.as_str())
            });
            let external = external_receipts.iter().rev().find(|receipt| {
                receipt.payload.get("tick_receipt_id").map(String::as_str)
                    == Some(last.receipt_id.as_str())
            });
            let attachment_payload = |key: &str| {
                attachment
                    .and_then(|receipt| receipt.payload.get(key))
                    .cloned()
            };
            let checker_payload = |key: &str| {
                checker
                    .and_then(|receipt| receipt.payload.get(key))
                    .cloned()
            };
            let fleet_payload =
                |key: &str| fleet.and_then(|receipt| receipt.payload.get(key)).cloned();
            let external_payload = |key: &str| {
                external
                    .and_then(|receipt| receipt.payload.get(key))
                    .cloned()
            };
            let fleet_u32 = |key: &str| {
                fleet_payload(key)
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or_default()
            };
            let fleet_bool = |key: &str| fleet_payload(key).as_deref() == Some("true");
            let external_bool = |key: &str| external_payload(key).as_deref() == Some("true");
            let attached_run_id = attachment_payload("run_id").unwrap_or_else(|| {
                last.payload
                    .get("attached_run_id")
                    .cloned()
                    .unwrap_or_default()
            });
            let run_admission_status =
                attachment_payload("run_admission_status").unwrap_or_default();
            let fleet_run_status = fleet_payload("fleet_run_status").unwrap_or_default();
            let checker_status = checker_payload("checker_status").unwrap_or_else(|| {
                if run_admission_status == "RUN_STARTED" {
                    "CHECKER_REQUIRED".to_string()
                } else {
                    String::new()
                }
            });
            let status = checker_payload("terminal_state").unwrap_or_else(|| {
                if external.is_some() {
                    external_payload("terminal_state").unwrap_or_default()
                } else if fleet.is_some() {
                    fleet_payload("terminal_state").unwrap_or_default()
                } else if run_admission_status == "RUN_STARTED" && checker.is_none() {
                    "CHECKER_REQUIRED_BEFORE_HUMAN_REVIEW".to_string()
                } else {
                    attachment_payload("terminal_state").unwrap_or_else(|| {
                        last.payload
                            .get("terminal_state")
                            .cloned()
                            .unwrap_or_else(|| "TICK_RECORDED_EXECUTION_WIRING_PENDING".to_string())
                    })
                }
            });
            let blocked_reason = checker_payload("blocked_reason").unwrap_or_else(|| {
                if external.is_some() {
                    external_payload("blocked_reason").unwrap_or_default()
                } else if fleet.is_some() {
                    fleet_payload("blocked_reason").unwrap_or_default()
                } else if run_admission_status == "RUN_STARTED" && checker.is_none() {
                    "checker_verdict_required".to_string()
                } else {
                    attachment_payload("blocked_reason").unwrap_or_else(|| {
                        last.payload
                            .get("blocked_reason")
                            .cloned()
                            .unwrap_or_default()
                    })
                }
            });
            return BuilderLoopState {
                loop_id: loop_id.to_string(),
                status,
                tick_count: tick_receipts.len(),
                last_tick_id: last.payload.get("tick_id").cloned().unwrap_or_default(),
                last_tick_receipt_id: last.receipt_id.clone(),
                last_trigger: last.payload.get("trigger").cloned().unwrap_or_default(),
                blocked_reason,
                needs_human: true,
                attached_run_id,
                run_admission_status,
                run_started_receipt_id: attachment_payload("run_started_receipt_id")
                    .unwrap_or_default(),
                run_projection_route: attachment_payload("run_projection_route")
                    .unwrap_or_default(),
                stream_route: attachment_payload("stream_route").unwrap_or_default(),
                attached_fleet_run_id: fleet_payload("fleet_id").unwrap_or_else(|| {
                    last.payload
                        .get("attached_fleet_run_id")
                        .cloned()
                        .unwrap_or_default()
                }),
                fleet_plan_status: fleet_payload("fleet_plan_status").unwrap_or_default(),
                fleet_plan_receipt_id: fleet_payload("fleet_plan_receipt_id").unwrap_or_default(),
                fleet_run_status,
                fleet_started_receipt_id: fleet_payload("fleet_started_receipt_id")
                    .unwrap_or_default(),
                fleet_plan_projection_route: fleet_payload("fleet_plan_projection_route")
                    .unwrap_or_default(),
                fleet_projection_route: fleet_payload("fleet_projection_route").unwrap_or_default(),
                fleet_capacity_route: fleet_payload("fleet_capacity_route").unwrap_or_default(),
                control_plane_route: fleet_payload("control_plane_route").unwrap_or_default(),
                fleet_lane_count: fleet_u32("lane_count"),
                fleet_lanes_needing_attention: fleet_u32("lanes_needing_attention"),
                fleet_active_worker_count: fleet_u32("active_worker_count"),
                fleet_queue_depth: fleet_u32("queue_depth"),
                fleet_repair_started_count: fleet_u32("repair_started_count"),
                fleet_repair_finished_count: fleet_u32("repair_finished_count"),
                fleet_repair_target_count: fleet_u32("repair_target_count"),
                fleet_repair_repaired_count: fleet_u32("repair_repaired_count"),
                fleet_integration_state: fleet_payload("integration_state").unwrap_or_default(),
                fleet_review_verdict: fleet_payload("review_verdict").unwrap_or_default(),
                fleet_wide_review_required: fleet_bool("wide_review_required"),
                fleet_human_ratification_required: fleet_bool("human_ratification_required"),
                external_runner_profile_id: external_payload("runner_profile_id")
                    .unwrap_or_default(),
                external_codex_profile_id: external_payload("codex_profile_id").unwrap_or_default(),
                external_adapter_status: external_payload("adapter_status").unwrap_or_default(),
                external_machine_status: external_payload("external_machine_status")
                    .unwrap_or_default(),
                external_provider_preflight_status: external_payload("provider_preflight_status")
                    .unwrap_or_default(),
                external_result_ingestion_status: external_payload("result_ingestion_status")
                    .unwrap_or_default(),
                external_result_receipt_id: external_payload("result_receipt_id")
                    .unwrap_or_default(),
                external_output_artifact_ref: external_payload("output_artifact_ref")
                    .unwrap_or_default(),
                external_transcript_ref: external_payload("transcript_ref").unwrap_or_default(),
                external_machine_route: external_payload("external_machine_route")
                    .unwrap_or_default(),
                external_provider_preflight_route: external_payload("provider_preflight_route")
                    .unwrap_or_default(),
                external_result_ingestion_route: external_payload("result_ingestion_route")
                    .unwrap_or_default(),
                external_result_projection_route: external_payload("result_projection_route")
                    .unwrap_or_default(),
                external_scoreboard_route: external_payload("scoreboard_route").unwrap_or_default(),
                external_output_quarantined: external_bool("output_quarantined"),
                external_output_consumable: external_bool("external_output_consumable"),
                external_mdx_quality_gates_required: external_bool("mdx_quality_gates_required"),
                external_mdx_quality_gates_passed: external_bool("mdx_quality_gates_passed"),
                external_accepted_for_scoreboard: external_bool("accepted_for_scoreboard"),
                attached_mission_id: last
                    .payload
                    .get("attached_mission_id")
                    .cloned()
                    .unwrap_or_default(),
                attached_review_packet_id: attachment_payload("review_packet_id").unwrap_or_else(
                    || {
                        last.payload
                            .get("attached_review_packet_id")
                            .cloned()
                            .unwrap_or_default()
                    },
                ),
                review_packet_route: attachment_payload("review_packet_route").unwrap_or_default(),
                checker_status,
                checker_verdict: checker_payload("checker_verdict").unwrap_or_default(),
                checker_confidence: checker_payload("checker_confidence").unwrap_or_default(),
                checker_attachment_receipt_id: checker
                    .map(|receipt| receipt.receipt_id.clone())
                    .unwrap_or_default(),
                checker_panel_receipt_id: checker_payload("checker_panel_receipt_id")
                    .unwrap_or_default(),
                checker_blocked_reason: checker_payload("blocked_reason").unwrap_or_default(),
                review_panel_route: checker_payload("review_panel_route")
                    .unwrap_or_else(|| "/forge/review-panel.json".to_string()),
                source_receipt_ids,
            };
        }
        BuilderLoopState {
            loop_id: loop_id.to_string(),
            status: "READY_FOR_TICK".to_string(),
            tick_count: 0,
            last_tick_id: String::new(),
            last_tick_receipt_id: String::new(),
            last_trigger: String::new(),
            blocked_reason: String::new(),
            needs_human: false,
            attached_run_id: String::new(),
            run_admission_status: String::new(),
            run_started_receipt_id: String::new(),
            run_projection_route: String::new(),
            stream_route: String::new(),
            attached_fleet_run_id: String::new(),
            fleet_plan_status: String::new(),
            fleet_plan_receipt_id: String::new(),
            fleet_run_status: String::new(),
            fleet_started_receipt_id: String::new(),
            fleet_plan_projection_route: String::new(),
            fleet_projection_route: String::new(),
            fleet_capacity_route: String::new(),
            control_plane_route: String::new(),
            fleet_lane_count: 0,
            fleet_lanes_needing_attention: 0,
            fleet_active_worker_count: 0,
            fleet_queue_depth: 0,
            fleet_repair_started_count: 0,
            fleet_repair_finished_count: 0,
            fleet_repair_target_count: 0,
            fleet_repair_repaired_count: 0,
            fleet_integration_state: String::new(),
            fleet_review_verdict: String::new(),
            fleet_wide_review_required: false,
            fleet_human_ratification_required: false,
            external_runner_profile_id: String::new(),
            external_codex_profile_id: String::new(),
            external_adapter_status: String::new(),
            external_machine_status: String::new(),
            external_provider_preflight_status: String::new(),
            external_result_ingestion_status: String::new(),
            external_result_receipt_id: String::new(),
            external_output_artifact_ref: String::new(),
            external_transcript_ref: String::new(),
            external_machine_route: String::new(),
            external_provider_preflight_route: String::new(),
            external_result_ingestion_route: String::new(),
            external_result_projection_route: String::new(),
            external_scoreboard_route: String::new(),
            external_output_quarantined: false,
            external_output_consumable: false,
            external_mdx_quality_gates_required: false,
            external_mdx_quality_gates_passed: false,
            external_accepted_for_scoreboard: false,
            attached_mission_id: String::new(),
            attached_review_packet_id: String::new(),
            review_packet_route: String::new(),
            checker_status: String::new(),
            checker_verdict: String::new(),
            checker_confidence: String::new(),
            checker_attachment_receipt_id: String::new(),
            checker_panel_receipt_id: String::new(),
            checker_blocked_reason: String::new(),
            review_panel_route: "/forge/review-panel.json".to_string(),
            source_receipt_ids,
        }
    }

    pub fn record_builder_loop_run_attachment(
        &mut self,
        request: BuilderLoopRunAttachment<'_>,
    ) -> Result<BuilderLoopRunAttachmentReport, BuilderLoopError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_builder_loop_run_attachment_with_identity(request, &identity)
    }

    pub fn record_builder_loop_run_attachment_with_identity(
        &mut self,
        request: BuilderLoopRunAttachment<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<BuilderLoopRunAttachmentReport, BuilderLoopError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("loop_id", request.loop_id),
            ("tick_receipt_id", request.tick_receipt_id),
            ("run_admission_status", request.run_admission_status),
        ] {
            if value.trim().is_empty() {
                return Err(BuilderLoopError::Missing(field));
            }
        }
        let template = builder_loop_template(request.loop_id)
            .ok_or_else(|| BuilderLoopError::UnknownLoop(request.loop_id.trim().to_string()))?;
        let Some(tick_receipt) = self.storage.ledger().query().by_id(request.tick_receipt_id)
        else {
            return Err(BuilderLoopError::UnknownTickReceipt(
                request.tick_receipt_id.to_string(),
            ));
        };
        if tick_receipt.kind != BUILDER_LOOP_TICK_KIND
            || tick_receipt.payload.get("loop_id").map(String::as_str) != Some(template.id)
        {
            return Err(BuilderLoopError::UnknownTickReceipt(
                request.tick_receipt_id.to_string(),
            ));
        }
        let terminal_state = if request.run_admission_status == "RUN_STARTED" {
            "RUN_ATTACHED_WAITING_FOR_HUMAN_GATE"
        } else {
            "RUN_ADMISSION_REFUSED"
        };
        let correlation = crate::CorrelationIds {
            tenant_id: crate::TenantId::new(request.tenant_id),
            trace_id: crate::TraceId::new(self.ids.next("trace")),
            actor_id: crate::ActorId::new(request.actor_id),
            loop_id: crate::LoopId::new("forge_builder_loop"),
            workflow_id: crate::WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(crate::LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision = self.decide_with_receipt(
            &correlation,
            ActionKind::RecordForgeBuilderLoopRunAttachment,
        );
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_FORGE_BUILDER_LOOP_RUN_ATTACHMENT",
            &correlation,
            &decision,
            BUILDER_LOOP_RUN_ATTACHMENT_KIND,
            payload(&[
                ("loop_id", template.id),
                ("loop_title", template.title),
                ("tick_receipt_id", request.tick_receipt_id),
                ("run_admission_status", request.run_admission_status),
                ("run_id", request.run_id),
                ("run_started_receipt_id", request.run_started_receipt_id),
                ("blocked_reason", request.blocked_reason),
                ("run_projection_route", request.run_projection_route),
                ("stream_route", request.stream_route),
                ("review_packet_route", request.review_packet_route),
                ("review_packet_id", ""),
                ("source_route", "/forge/builder-loops/tick.json"),
                ("projection_route", "/forge/builder-loops.json"),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("terminal_state", terminal_state),
                ("human_gate", template.human_gate),
                ("authority_opened", "forge_run_admission_only"),
                ("adapter_execution_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_builder_loop_attachment_run(&run_id, terminal_state);
        Ok(BuilderLoopRunAttachmentReport {
            status: "BUILDER_LOOP_RUN_ATTACHMENT_RECORDED",
            loop_id: template.id.to_string(),
            attachment_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            run_admission_status: request.run_admission_status.to_string(),
            run_id: request.run_id.to_string(),
            terminal_state,
        })
    }

    pub fn record_builder_loop_checker_attachment(
        &mut self,
        request: BuilderLoopCheckerAttachment<'_>,
    ) -> Result<BuilderLoopCheckerAttachmentReport, BuilderLoopError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_builder_loop_checker_attachment_with_identity(request, &identity)
    }

    pub fn record_builder_loop_fleet_attachment(
        &mut self,
        request: BuilderLoopFleetAttachment<'_>,
    ) -> Result<BuilderLoopFleetAttachmentReport, BuilderLoopError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_builder_loop_fleet_attachment_with_identity(request, &identity)
    }

    pub fn record_builder_loop_external_trial_attachment(
        &mut self,
        request: BuilderLoopExternalTrialAttachment<'_>,
    ) -> Result<BuilderLoopExternalTrialAttachmentReport, BuilderLoopError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_builder_loop_external_trial_attachment_with_identity(request, &identity)
    }

    pub fn record_builder_loop_fleet_attachment_with_identity(
        &mut self,
        request: BuilderLoopFleetAttachment<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<BuilderLoopFleetAttachmentReport, BuilderLoopError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("loop_id", request.loop_id),
            ("tick_receipt_id", request.tick_receipt_id),
            ("fleet_id", request.fleet_id),
            ("fleet_plan_status", request.fleet_plan_status),
            ("fleet_run_status", request.fleet_run_status),
        ] {
            if value.trim().is_empty() {
                return Err(BuilderLoopError::Missing(field));
            }
        }
        let template = builder_loop_template(request.loop_id)
            .ok_or_else(|| BuilderLoopError::UnknownLoop(request.loop_id.trim().to_string()))?;
        let Some(tick_receipt) = self.storage.ledger().query().by_id(request.tick_receipt_id)
        else {
            return Err(BuilderLoopError::UnknownTickReceipt(
                request.tick_receipt_id.to_string(),
            ));
        };
        if tick_receipt.kind != BUILDER_LOOP_TICK_KIND
            || tick_receipt.payload.get("loop_id").map(String::as_str) != Some(template.id)
        {
            return Err(BuilderLoopError::UnknownTickReceipt(
                request.tick_receipt_id.to_string(),
            ));
        }
        let terminal_state = if request.fleet_run_status == "FLEET_STARTED" {
            "FLEET_ATTACHED_WAITING_FOR_CONDUCTOR"
        } else if request.fleet_plan_status == "RATIFIED" {
            "FLEET_PLAN_RATIFIED_READY_FOR_RUN"
        } else if request.human_ratification_required {
            "FLEET_PLAN_RATIFICATION_REQUIRED"
        } else {
            "FLEET_ADMISSION_REFUSED"
        };
        let correlation = crate::CorrelationIds {
            tenant_id: crate::TenantId::new(request.tenant_id),
            trace_id: crate::TraceId::new(self.ids.next("trace")),
            actor_id: crate::ActorId::new(request.actor_id),
            loop_id: crate::LoopId::new("forge_builder_loop"),
            workflow_id: crate::WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(crate::LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision = self.decide_with_receipt(
            &correlation,
            ActionKind::RecordForgeBuilderLoopFleetAttachment,
        );
        let lane_count = request.lane_count.to_string();
        let lanes_needing_attention = request.lanes_needing_attention.to_string();
        let active_worker_count = request.active_worker_count.to_string();
        let queue_depth = request.queue_depth.to_string();
        let repair_started_count = request.repair_started_count.to_string();
        let repair_finished_count = request.repair_finished_count.to_string();
        let repair_target_count = request.repair_target_count.to_string();
        let repair_repaired_count = request.repair_repaired_count.to_string();
        let wide_review_required = request.wide_review_required.to_string();
        let human_ratification_required = request.human_ratification_required.to_string();
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_FORGE_BUILDER_LOOP_FLEET_ATTACHMENT",
            &correlation,
            &decision,
            BUILDER_LOOP_FLEET_ATTACHMENT_KIND,
            payload(&[
                ("loop_id", template.id),
                ("loop_title", template.title),
                ("tick_receipt_id", request.tick_receipt_id),
                ("fleet_id", request.fleet_id),
                ("fleet_plan_status", request.fleet_plan_status),
                ("fleet_plan_receipt_id", request.fleet_plan_receipt_id),
                ("fleet_run_status", request.fleet_run_status),
                ("fleet_started_receipt_id", request.fleet_started_receipt_id),
                ("blocked_reason", request.blocked_reason),
                (
                    "fleet_plan_projection_route",
                    request.fleet_plan_projection_route,
                ),
                ("fleet_projection_route", request.fleet_projection_route),
                ("fleet_capacity_route", request.fleet_capacity_route),
                ("control_plane_route", request.control_plane_route),
                ("lane_count", &lane_count),
                ("lanes_needing_attention", &lanes_needing_attention),
                ("active_worker_count", &active_worker_count),
                ("queue_depth", &queue_depth),
                ("repair_started_count", &repair_started_count),
                ("repair_finished_count", &repair_finished_count),
                ("repair_target_count", &repair_target_count),
                ("repair_repaired_count", &repair_repaired_count),
                ("integration_state", request.integration_state),
                ("review_verdict", request.review_verdict),
                ("wide_review_required", &wide_review_required),
                ("human_ratification_required", &human_ratification_required),
                ("source_route", "/forge/builder-loops/tick.json"),
                ("projection_route", "/forge/builder-loops.json"),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("terminal_state", terminal_state),
                ("human_gate", template.human_gate),
                ("authority_opened", "forge_fleet_plan_and_run_only"),
                ("adapter_execution_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_builder_loop_attachment_run(&run_id, terminal_state);
        Ok(BuilderLoopFleetAttachmentReport {
            status: "BUILDER_LOOP_FLEET_ATTACHMENT_RECORDED",
            loop_id: template.id.to_string(),
            fleet_attachment_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            fleet_id: request.fleet_id.to_string(),
            fleet_run_status: request.fleet_run_status.to_string(),
            terminal_state,
        })
    }

    pub fn record_builder_loop_external_trial_attachment_with_identity(
        &mut self,
        request: BuilderLoopExternalTrialAttachment<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<BuilderLoopExternalTrialAttachmentReport, BuilderLoopError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("loop_id", request.loop_id),
            ("tick_receipt_id", request.tick_receipt_id),
            ("runner_profile_id", request.runner_profile_id),
            ("codex_profile_id", request.codex_profile_id),
            ("adapter_status", request.adapter_status),
            ("external_machine_status", request.external_machine_status),
            (
                "provider_preflight_status",
                request.provider_preflight_status,
            ),
            ("result_ingestion_status", request.result_ingestion_status),
        ] {
            if value.trim().is_empty() {
                return Err(BuilderLoopError::Missing(field));
            }
        }
        let template = builder_loop_template(request.loop_id)
            .ok_or_else(|| BuilderLoopError::UnknownLoop(request.loop_id.trim().to_string()))?;
        let Some(tick_receipt) = self.storage.ledger().query().by_id(request.tick_receipt_id)
        else {
            return Err(BuilderLoopError::UnknownTickReceipt(
                request.tick_receipt_id.to_string(),
            ));
        };
        if tick_receipt.kind != BUILDER_LOOP_TICK_KIND
            || tick_receipt.payload.get("loop_id").map(String::as_str) != Some(template.id)
        {
            return Err(BuilderLoopError::UnknownTickReceipt(
                request.tick_receipt_id.to_string(),
            ));
        }
        let terminal_state = if request.output_quarantined && !request.external_output_consumable {
            "EXTERNAL_TRIAL_QUARANTINED_PENDING_MDX_GATES"
        } else {
            "EXTERNAL_TRIAL_REFUSED_OR_UNSAFE"
        };
        let correlation = crate::CorrelationIds {
            tenant_id: crate::TenantId::new(request.tenant_id),
            trace_id: crate::TraceId::new(self.ids.next("trace")),
            actor_id: crate::ActorId::new(request.actor_id),
            loop_id: crate::LoopId::new("forge_builder_loop"),
            workflow_id: crate::WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(crate::LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision = self.decide_with_receipt(
            &correlation,
            ActionKind::RecordForgeBuilderLoopExternalTrialAttachment,
        );
        let output_quarantined = request.output_quarantined.to_string();
        let external_output_consumable = request.external_output_consumable.to_string();
        let mdx_quality_gates_required = request.mdx_quality_gates_required.to_string();
        let mdx_quality_gates_passed = request.mdx_quality_gates_passed.to_string();
        let accepted_for_scoreboard = request.accepted_for_scoreboard.to_string();
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_FORGE_BUILDER_LOOP_EXTERNAL_TRIAL_ATTACHMENT",
            &correlation,
            &decision,
            BUILDER_LOOP_EXTERNAL_TRIAL_ATTACHMENT_KIND,
            payload(&[
                ("loop_id", template.id),
                ("loop_title", template.title),
                ("tick_receipt_id", request.tick_receipt_id),
                ("runner_profile_id", request.runner_profile_id),
                ("codex_profile_id", request.codex_profile_id),
                ("adapter_status", request.adapter_status),
                ("external_machine_status", request.external_machine_status),
                (
                    "provider_preflight_status",
                    request.provider_preflight_status,
                ),
                ("result_ingestion_status", request.result_ingestion_status),
                ("result_receipt_id", request.result_receipt_id),
                ("output_artifact_ref", request.output_artifact_ref),
                ("transcript_ref", request.transcript_ref),
                ("blocked_reason", request.blocked_reason),
                ("external_machine_route", request.external_machine_route),
                ("provider_preflight_route", request.provider_preflight_route),
                ("result_ingestion_route", request.result_ingestion_route),
                ("result_projection_route", request.result_projection_route),
                ("scoreboard_route", request.scoreboard_route),
                ("output_quarantined", &output_quarantined),
                ("external_output_consumable", &external_output_consumable),
                ("mdx_quality_gates_required", &mdx_quality_gates_required),
                ("mdx_quality_gates_passed", &mdx_quality_gates_passed),
                ("accepted_for_scoreboard", &accepted_for_scoreboard),
                ("source_route", "/forge/builder-loops/tick.json"),
                ("projection_route", "/forge/builder-loops.json"),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("terminal_state", terminal_state),
                ("human_gate", template.human_gate),
                ("authority_opened", "none"),
                ("external_adapter_execution_allowed", "false"),
                ("network_calls_allowed", "false"),
                ("secret_export_allowed", "false"),
                ("adapter_execution_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_builder_loop_attachment_run(&run_id, terminal_state);
        Ok(BuilderLoopExternalTrialAttachmentReport {
            status: "BUILDER_LOOP_EXTERNAL_TRIAL_ATTACHMENT_RECORDED",
            loop_id: template.id.to_string(),
            external_trial_attachment_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            runner_profile_id: request.runner_profile_id.to_string(),
            result_receipt_id: request.result_receipt_id.to_string(),
            terminal_state,
        })
    }

    pub fn record_builder_loop_checker_attachment_with_identity(
        &mut self,
        request: BuilderLoopCheckerAttachment<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<BuilderLoopCheckerAttachmentReport, BuilderLoopError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("loop_id", request.loop_id),
            ("tick_receipt_id", request.tick_receipt_id),
            ("run_id", request.run_id),
            ("maker_actor_id", request.maker_actor_id),
            ("checker_actor_id", request.checker_actor_id),
            ("checker_status", request.checker_status),
        ] {
            if value.trim().is_empty() {
                return Err(BuilderLoopError::Missing(field));
            }
        }
        let template = builder_loop_template(request.loop_id)
            .ok_or_else(|| BuilderLoopError::UnknownLoop(request.loop_id.trim().to_string()))?;
        let Some(tick_receipt) = self.storage.ledger().query().by_id(request.tick_receipt_id)
        else {
            return Err(BuilderLoopError::UnknownTickReceipt(
                request.tick_receipt_id.to_string(),
            ));
        };
        if tick_receipt.kind != BUILDER_LOOP_TICK_KIND
            || tick_receipt.payload.get("loop_id").map(String::as_str) != Some(template.id)
        {
            return Err(BuilderLoopError::UnknownTickReceipt(
                request.tick_receipt_id.to_string(),
            ));
        }
        let has_run_attachment = self
            .storage
            .ledger()
            .query()
            .by_kind(BUILDER_LOOP_RUN_ATTACHMENT_KIND)
            .into_iter()
            .any(|receipt| {
                receipt.payload.get("loop_id").map(String::as_str) == Some(template.id)
                    && receipt.payload.get("tick_receipt_id").map(String::as_str)
                        == Some(request.tick_receipt_id)
                    && receipt.payload.get("run_id").map(String::as_str) == Some(request.run_id)
                    && receipt
                        .payload
                        .get("run_admission_status")
                        .map(String::as_str)
                        == Some("RUN_STARTED")
            });
        if !has_run_attachment {
            return Err(BuilderLoopError::UnknownRunAttachment(
                request.run_id.to_string(),
            ));
        }
        let checker_status = request.checker_status.trim();
        let terminal_state = match checker_status {
            "CHECKER_READY" => "READY_FOR_HUMAN_REVIEW",
            "CHECKER_NEEDS_WORK" => "CHECKER_NEEDS_WORK_REPAIR_OR_HUMAN",
            "CHECKER_INCONCLUSIVE" => "CHECKER_INCONCLUSIVE_HUMAN_DECISION",
            "CHECKER_UNAVAILABLE" => "CHECKER_UNAVAILABLE",
            "CHECKER_PENDING" => "CHECKER_PENDING_FRESH_CONTEXT_REVIEW",
            other => return Err(BuilderLoopError::InvalidCheckerStatus(other.to_string())),
        };
        if matches!(
            checker_status,
            "CHECKER_READY" | "CHECKER_NEEDS_WORK" | "CHECKER_INCONCLUSIVE"
        ) {
            if request.maker_actor_id.trim() == request.checker_actor_id.trim() {
                return Err(BuilderLoopError::MakerCheckerSameActor(
                    request.maker_actor_id.to_string(),
                ));
            }
            let Some(panel) = self
                .storage
                .ledger()
                .query()
                .by_id(request.checker_panel_receipt_id)
            else {
                return Err(BuilderLoopError::UnknownCheckerPanelReceipt(
                    request.checker_panel_receipt_id.to_string(),
                ));
            };
            if panel.kind != "forge.review.panel.recorded"
                || panel.payload.get("run_id").map(String::as_str) != Some(request.run_id)
            {
                return Err(BuilderLoopError::UnknownCheckerPanelReceipt(
                    request.checker_panel_receipt_id.to_string(),
                ));
            }
            let consensus = panel.payload.get("consensus").map(String::as_str);
            let consensus_matches = match checker_status {
                "CHECKER_READY" => consensus == Some("ready"),
                "CHECKER_NEEDS_WORK" => consensus == Some("needs_work"),
                "CHECKER_INCONCLUSIVE" => consensus == Some("inconclusive"),
                _ => false,
            };
            if !consensus_matches {
                return Err(BuilderLoopError::UnknownCheckerPanelReceipt(
                    request.checker_panel_receipt_id.to_string(),
                ));
            }
        }
        let correlation = crate::CorrelationIds {
            tenant_id: crate::TenantId::new(request.tenant_id),
            trace_id: crate::TraceId::new(self.ids.next("trace")),
            actor_id: crate::ActorId::new(request.actor_id),
            loop_id: crate::LoopId::new("forge_builder_loop"),
            workflow_id: crate::WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(crate::LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision = self.decide_with_receipt(
            &correlation,
            ActionKind::RecordForgeBuilderLoopCheckerAttachment,
        );
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_FORGE_BUILDER_LOOP_CHECKER_ATTACHMENT",
            &correlation,
            &decision,
            BUILDER_LOOP_CHECKER_ATTACHMENT_KIND,
            payload(&[
                ("loop_id", template.id),
                ("loop_title", template.title),
                ("tick_receipt_id", request.tick_receipt_id),
                ("run_id", request.run_id),
                ("maker_actor_id", request.maker_actor_id),
                ("checker_actor_id", request.checker_actor_id),
                ("checker_role", template.checker_role),
                ("checker_status", checker_status),
                ("checker_verdict", request.checker_verdict),
                ("checker_confidence", request.checker_confidence),
                ("checker_panel_receipt_id", request.checker_panel_receipt_id),
                ("blocked_reason", request.blocked_reason),
                ("review_panel_route", request.review_panel_route),
                ("source_route", "/forge/builder-loops/tick.json"),
                ("projection_route", "/forge/builder-loops.json"),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("terminal_state", terminal_state),
                ("checker_required_before_human_review", "true"),
                ("maker_can_self_accept", "false"),
                (
                    "ready_for_human_review",
                    if checker_status == "CHECKER_READY" {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("authority_opened", "none"),
                ("adapter_execution_allowed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_builder_loop_attachment_run(&run_id, terminal_state);
        Ok(BuilderLoopCheckerAttachmentReport {
            status: "BUILDER_LOOP_CHECKER_ATTACHMENT_RECORDED",
            loop_id: template.id.to_string(),
            checker_attachment_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            checker_status: checker_status.to_string(),
            checker_verdict: request.checker_verdict.to_string(),
            terminal_state,
        })
    }

    fn finish_builder_loop_attachment_run(&mut self, run_id: &str, status: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = status.to_string();
        }
    }

    fn finish_builder_loop_tick_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "BUILDER_LOOP_TICK_RECORDED_EXECUTION_BLOCKED".to_string();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_loop_catalog_has_the_pr_one_and_fleet_templates() {
        assert_eq!(BUILDER_LOOP_TEMPLATES.len(), 5);
        for id in [
            "quality_sweep",
            "local_check_recovery",
            "pr_review_babysitter",
            "fleet_quality_sweep",
            "external_machine_trial",
        ] {
            let template = builder_loop_template(id).expect("known template");
            assert!(template.authority_level.starts_with("assisted_"));
            assert!(!template.goal.is_empty());
            assert!(!template.allowed_tools.is_empty());
            assert!(template.denied_tools.contains(&"production_write"));
        }
    }

    #[test]
    fn builder_loop_tick_records_receipt_without_execution_authority() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .record_builder_loop_tick(BuilderLoopTick {
                tenant_id: "tenant_local",
                actor_id: "local_user",
                loop_id: "local_check_recovery",
                trigger: "manual",
                reason: "operator clicked run",
            })
            .expect("tick records");
        assert_eq!(
            report.status,
            "BUILDER_LOOP_TICK_RECORDED_EXECUTION_BLOCKED"
        );
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.tick_receipt_id)
            .expect("tick receipt");
        assert_eq!(receipt.kind, BUILDER_LOOP_TICK_KIND);
        assert_eq!(receipt.payload["authority_opened"], "none");
        assert_eq!(receipt.payload["production_write_allowed"], "false");
        assert_eq!(
            receipt.payload["blocked_reason"],
            "golden_builder_loop_runtime_lands_in_pr_2"
        );
        let state = kernel.builder_loop_state("local_check_recovery");
        assert_eq!(state.tick_count, 1);
        assert_eq!(state.last_tick_receipt_id, report.tick_receipt_id);
        assert!(state.needs_human);
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn builder_loop_run_attachment_folds_into_state() {
        let mut kernel = MdxKernel::boot_local();
        let tick = kernel
            .record_builder_loop_tick(BuilderLoopTick {
                tenant_id: "tenant_local",
                actor_id: "local_user",
                loop_id: "quality_sweep",
                trigger: "manual",
                reason: "operator clicked run",
            })
            .expect("tick records");
        let attachment = kernel
            .record_builder_loop_run_attachment(BuilderLoopRunAttachment {
                tenant_id: "tenant_local",
                actor_id: "local_user",
                loop_id: "quality_sweep",
                tick_receipt_id: &tick.tick_receipt_id,
                run_admission_status: "RUN_STARTED",
                run_id: "forge_run_builder_loop_1",
                run_started_receipt_id: "receipt_run_started",
                blocked_reason: "",
                run_projection_route: "/forge/runs/projection.json",
                stream_route: "/forge/runs/forge_run_builder_loop_1/stream",
                review_packet_route: "/forge/review-packets/forge_run_builder_loop_1.json",
            })
            .expect("attachment records");
        assert_eq!(attachment.status, "BUILDER_LOOP_RUN_ATTACHMENT_RECORDED");
        let state = kernel.builder_loop_state("quality_sweep");
        assert_eq!(state.status, "CHECKER_REQUIRED_BEFORE_HUMAN_REVIEW");
        assert_eq!(state.attached_run_id, "forge_run_builder_loop_1");
        assert_eq!(state.run_admission_status, "RUN_STARTED");
        assert_eq!(state.checker_status, "CHECKER_REQUIRED");
        assert_eq!(state.blocked_reason, "checker_verdict_required");
        assert_eq!(state.source_receipt_ids.len(), 2);
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn builder_loop_external_trial_attachment_folds_quarantined_eval_result() {
        let mut kernel = MdxKernel::boot_local();
        let tick = kernel
            .record_builder_loop_tick(BuilderLoopTick {
                tenant_id: "tenant_local",
                actor_id: "local_user",
                loop_id: "external_machine_trial",
                trigger: "manual",
                reason: "operator clicked external trial",
            })
            .expect("tick records");
        let attachment = kernel
            .record_builder_loop_external_trial_attachment(BuilderLoopExternalTrialAttachment {
                tenant_id: "tenant_local",
                actor_id: "local_user",
                loop_id: "external_machine_trial",
                tick_receipt_id: &tick.tick_receipt_id,
                runner_profile_id: "codex_cli_external_worker",
                codex_profile_id: "codex_cli_responses_worker",
                adapter_status: "PROFILE_ADMITTED_ADAPTER_DENIED",
                external_machine_status: "EXTERNAL_MACHINE_BLOCKED",
                provider_preflight_status: "MISSING_PROVIDER_CREDENTIALS",
                result_ingestion_status: "FLEET_EVAL_RESULT_QUARANTINED_PENDING_MDX_GATES",
                result_receipt_id: "receipt_eval_result",
                output_artifact_ref: "quarantine://forge/fleet-eval/builder-loop.patch",
                transcript_ref: "quarantine://forge/fleet-eval/builder-loop.jsonl",
                blocked_reason: "pending_mdx_quality_gates",
                external_machine_route: "/harness/external-machine.json",
                provider_preflight_route: "/forge/fleet-eval-provider-preflight.json",
                result_ingestion_route: "/forge/fleet-eval-results.json",
                result_projection_route: "/forge/fleet-eval-results/projection.json",
                scoreboard_route: "/forge/fleet-eval-scoreboard.json",
                output_quarantined: true,
                external_output_consumable: false,
                mdx_quality_gates_required: true,
                mdx_quality_gates_passed: false,
                accepted_for_scoreboard: false,
            })
            .expect("external trial attachment records");
        assert_eq!(
            attachment.status,
            "BUILDER_LOOP_EXTERNAL_TRIAL_ATTACHMENT_RECORDED"
        );
        let state = kernel.builder_loop_state("external_machine_trial");
        assert_eq!(state.status, "EXTERNAL_TRIAL_QUARANTINED_PENDING_MDX_GATES");
        assert_eq!(
            state.external_runner_profile_id,
            "codex_cli_external_worker"
        );
        assert_eq!(
            state.external_codex_profile_id,
            "codex_cli_responses_worker"
        );
        assert_eq!(state.external_result_receipt_id, "receipt_eval_result");
        assert!(state.external_output_quarantined);
        assert!(!state.external_output_consumable);
        assert!(state.external_mdx_quality_gates_required);
        assert!(!state.external_mdx_quality_gates_passed);
        assert!(!state.external_accepted_for_scoreboard);
        assert_eq!(state.source_receipt_ids.len(), 2);
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn builder_loop_fleet_attachment_folds_existing_fleet_rollup() {
        let mut kernel = MdxKernel::boot_local();
        let tick = kernel
            .record_builder_loop_tick(BuilderLoopTick {
                tenant_id: "tenant_local",
                actor_id: "local_user",
                loop_id: "fleet_quality_sweep",
                trigger: "manual",
                reason: "operator clicked fleet run",
            })
            .expect("tick records");
        let attachment = kernel
            .record_builder_loop_fleet_attachment(BuilderLoopFleetAttachment {
                tenant_id: "tenant_local",
                actor_id: "local_user",
                loop_id: "fleet_quality_sweep",
                tick_receipt_id: &tick.tick_receipt_id,
                fleet_id: "builder_loop_fleet_1",
                fleet_plan_status: "RATIFIED",
                fleet_plan_receipt_id: "receipt_plan",
                fleet_run_status: "FLEET_STARTED",
                fleet_started_receipt_id: "receipt_fleet_started",
                blocked_reason: "",
                fleet_plan_projection_route: "/forge/fleet-plans/projection.json",
                fleet_projection_route: "/forge/fleet-runs/projection.json",
                fleet_capacity_route: "/forge/fleet-capacity.json",
                control_plane_route: "/forge/control-plane/projection.json",
                lane_count: 2,
                lanes_needing_attention: 1,
                active_worker_count: 2,
                queue_depth: 0,
                repair_started_count: 1,
                repair_finished_count: 1,
                repair_target_count: 1,
                repair_repaired_count: 0,
                integration_state: "pending",
                review_verdict: "not_recorded",
                wide_review_required: false,
                human_ratification_required: true,
            })
            .expect("fleet attachment records");
        assert_eq!(attachment.status, "BUILDER_LOOP_FLEET_ATTACHMENT_RECORDED");
        let state = kernel.builder_loop_state("fleet_quality_sweep");
        assert_eq!(state.status, "FLEET_ATTACHED_WAITING_FOR_CONDUCTOR");
        assert_eq!(state.attached_fleet_run_id, "builder_loop_fleet_1");
        assert_eq!(state.fleet_run_status, "FLEET_STARTED");
        assert_eq!(state.fleet_lane_count, 2);
        assert_eq!(state.fleet_lanes_needing_attention, 1);
        assert_eq!(state.fleet_active_worker_count, 2);
        assert_eq!(state.fleet_integration_state, "pending");
        assert!(state.fleet_human_ratification_required);
        assert_eq!(state.source_receipt_ids.len(), 2);
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn builder_loop_checker_attachment_requires_fresh_separate_panel() {
        let mut kernel = MdxKernel::boot_local();
        let tick = kernel
            .record_builder_loop_tick(BuilderLoopTick {
                tenant_id: "tenant_local",
                actor_id: "maker",
                loop_id: "quality_sweep",
                trigger: "manual",
                reason: "operator clicked run",
            })
            .expect("tick records");
        kernel
            .record_builder_loop_run_attachment(BuilderLoopRunAttachment {
                tenant_id: "tenant_local",
                actor_id: "maker",
                loop_id: "quality_sweep",
                tick_receipt_id: &tick.tick_receipt_id,
                run_admission_status: "RUN_STARTED",
                run_id: "forge_run_builder_loop_2",
                run_started_receipt_id: "receipt_run_started",
                blocked_reason: "",
                run_projection_route: "/forge/runs/projection.json",
                stream_route: "/forge/runs/forge_run_builder_loop_2/stream",
                review_packet_route: "/forge/review-packets/forge_run_builder_loop_2.json",
            })
            .expect("attachment records");
        let members = vec![crate::PanelMember {
            model: "reviewer-model".to_string(),
            stance: "fresh".to_string(),
            verdict: "ready".to_string(),
            concern: String::new(),
        }];
        let panel = kernel
            .record_forge_review_panel(crate::ForgeReviewPanel {
                tenant_id: "tenant_local",
                actor_id: "checker",
                run_id: "forge_run_builder_loop_2",
                consensus: "ready",
                confidence: "high",
                residency: "local",
                members: &members,
                dissent: &[],
                blind_spots: "",
            })
            .expect("panel records");
        let self_accept =
            kernel.record_builder_loop_checker_attachment(BuilderLoopCheckerAttachment {
                tenant_id: "tenant_local",
                actor_id: "maker",
                loop_id: "quality_sweep",
                tick_receipt_id: &tick.tick_receipt_id,
                run_id: "forge_run_builder_loop_2",
                maker_actor_id: "maker",
                checker_actor_id: "maker",
                checker_status: "CHECKER_READY",
                checker_verdict: "ready",
                checker_confidence: "high",
                checker_panel_receipt_id: &panel.receipt_id,
                blocked_reason: "",
                review_panel_route: "/forge/review-panel.json",
            });
        assert!(matches!(
            self_accept,
            Err(BuilderLoopError::MakerCheckerSameActor(_))
        ));
        let checker = kernel
            .record_builder_loop_checker_attachment(BuilderLoopCheckerAttachment {
                tenant_id: "tenant_local",
                actor_id: "checker",
                loop_id: "quality_sweep",
                tick_receipt_id: &tick.tick_receipt_id,
                run_id: "forge_run_builder_loop_2",
                maker_actor_id: "maker",
                checker_actor_id: "checker",
                checker_status: "CHECKER_READY",
                checker_verdict: "ready",
                checker_confidence: "high",
                checker_panel_receipt_id: &panel.receipt_id,
                blocked_reason: "",
                review_panel_route: "/forge/review-panel.json",
            })
            .expect("checker records");
        assert_eq!(checker.terminal_state, "READY_FOR_HUMAN_REVIEW");
        let state = kernel.builder_loop_state("quality_sweep");
        assert_eq!(state.status, "READY_FOR_HUMAN_REVIEW");
        assert_eq!(state.checker_status, "CHECKER_READY");
        assert_eq!(state.checker_verdict, "ready");
        assert_eq!(state.checker_panel_receipt_id, panel.receipt_id);
        assert_eq!(state.source_receipt_ids.len(), 3);
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn builder_loop_tick_refuses_unknown_loop_and_trigger() {
        let mut kernel = MdxKernel::boot_local();
        let bad_loop = kernel.record_builder_loop_tick(BuilderLoopTick {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            loop_id: "not_real",
            trigger: "manual",
            reason: "",
        });
        assert!(matches!(bad_loop, Err(BuilderLoopError::UnknownLoop(_))));
        let bad_trigger = kernel.record_builder_loop_tick(BuilderLoopTick {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            loop_id: "quality_sweep",
            trigger: "whenever",
            reason: "",
        });
        assert!(matches!(
            bad_trigger,
            Err(BuilderLoopError::UnknownTrigger(_))
        ));
    }
}
