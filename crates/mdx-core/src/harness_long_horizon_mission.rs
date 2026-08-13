use crate::{
    ActionKind, ActorId, CorrelationIds, GovernedWriteIdentity, LoopId, LoopRun, MdxKernel,
    StorageProvider, TenantId, TraceId, WorkflowId, admit_local_route_actor,
    forge_fleet_benchmark_tasks, forge_fleet_model_matrix_profiles, forge_fleet_runner_profiles,
    forge_fleet_scoring_dimensions, json_string_literal, payload,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeLongHorizonMissionAdmission<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub actor_role: &'a str,
    pub mission_id: &'a str,
    pub goal: &'a str,
    pub non_goals: &'a str,
    pub constraints: &'a str,
    pub done_when: &'a str,
    pub allowed_write_scope: &'a str,
    pub blocked_paths: &'a str,
    pub validation_commands: &'a str,
    pub model_policy: &'a str,
    pub provider_allowlist: &'a str,
    pub fleet_width: u32,
    pub max_runtime_ms: u64,
    pub max_cost_cents: u32,
    pub checkpoint_cadence_minutes: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeLongHorizonMissionAdmissionReport {
    pub status: &'static str,
    pub mission_id: String,
    pub mission_receipt_id: String,
    pub policy_decision_id: String,
    pub milestone_count: u32,
    pub fleet_width: u32,
    pub max_runtime_ms: u64,
    pub max_cost_cents: u32,
    pub checkpoint_cadence_minutes: u32,
    pub spec_packet_recorded: bool,
    pub milestone_plan_recorded: bool,
    pub runbook_recorded: bool,
    pub status_log_recorded: bool,
    pub dashboard_ready: bool,
    pub live_provider_calls_allowed: bool,
    pub adapter_execution_allowed: bool,
    pub production_write_allowed: bool,
    pub blocked_reason: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeLongHorizonMissionMilestone {
    pub milestone_id: String,
    pub title: String,
    pub validation_command: String,
    pub status: &'static str,
    pub acceptance_criteria: String,
    pub checkpoint_receipt_id: String,
    pub validation_status: String,
    pub related_run_id: String,
    pub related_fleet_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeLongHorizonMissionPacket {
    pub mission_id: String,
    pub goal: String,
    pub non_goals: String,
    pub constraints: String,
    pub done_when: String,
    pub allowed_write_scope: String,
    pub blocked_paths: String,
    pub validation_commands: String,
    pub model_policy: String,
    pub provider_allowlist: String,
    pub fleet_width: u32,
    pub max_runtime_ms: u64,
    pub max_cost_cents: u32,
    pub checkpoint_cadence_minutes: u32,
    pub mission_receipt_id: String,
    pub policy_decision_id: String,
    pub mission_state: &'static str,
    pub latest_checkpoint_event: String,
    pub latest_checkpoint_summary: String,
    pub checkpoint_count: u32,
    pub completed_milestone_count: u32,
    pub blocked_milestone_count: u32,
    pub steering_note_count: u32,
    pub pause_count: u32,
    pub resume_count: u32,
    pub related_run_ids: Vec<String>,
    pub related_fleet_ids: Vec<String>,
    pub milestones: Vec<ForgeLongHorizonMissionMilestone>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeLongHorizonMissionCheckpoint<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub actor_role: &'a str,
    pub mission_id: &'a str,
    pub milestone_id: &'a str,
    pub checkpoint_event: &'a str,
    pub summary: &'a str,
    pub validation_status: &'a str,
    pub related_run_id: &'a str,
    pub related_fleet_id: &'a str,
    pub steering_note: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeLongHorizonMissionCheckpointReport {
    pub status: &'static str,
    pub mission_id: String,
    pub milestone_id: String,
    pub checkpoint_event: String,
    pub checkpoint_receipt_id: String,
    pub policy_decision_id: String,
    pub mission_state: &'static str,
    pub live_provider_calls_allowed: bool,
    pub adapter_execution_allowed: bool,
    pub production_write_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeLongHorizonMissionDashboard {
    pub status: &'static str,
    pub mission_count: u32,
    pub active_mission_count: u32,
    pub milestone_count: u32,
    pub completed_milestone_count: u32,
    pub blocked_milestone_count: u32,
    pub verification_cadence: &'static str,
    pub dashboard_ready: bool,
    pub live_provider_calls_allowed: bool,
    pub adapter_execution_allowed: bool,
    pub production_write_allowed: bool,
    pub human_checkpoint_required: bool,
    pub provider_matrix_count: u32,
    pub benchmark_task_count: u32,
    pub scoring_dimension_count: u32,
    pub runner_profile_count: u32,
    pub packets: Vec<ForgeLongHorizonMissionPacket>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForgeLongHorizonMissionAdmissionError {
    Missing(&'static str),
    Invalid(&'static str),
    UnknownMission(String),
    UnknownMilestone(String),
    ActorAdmission(String),
}

impl ForgeLongHorizonMissionAdmissionError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("forge long-horizon mission missing {field}"),
            Self::Invalid(field) => format!("forge long-horizon mission invalid {field}"),
            Self::UnknownMission(mission_id) => {
                format!("forge long-horizon mission is unknown: {mission_id}")
            }
            Self::UnknownMilestone(milestone_id) => {
                format!("forge long-horizon mission milestone is unknown: {milestone_id}")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalForgeLongHorizonMission;

impl LocalForgeLongHorizonMission {
    pub fn dashboard<S: StorageProvider>(
        &self,
        kernel: &MdxKernel<S>,
    ) -> ForgeLongHorizonMissionDashboard {
        kernel.project_forge_long_horizon_missions()
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn admit_forge_long_horizon_mission_local(
        &mut self,
        admission: ForgeLongHorizonMissionAdmission<'_>,
    ) -> Result<ForgeLongHorizonMissionAdmissionReport, ForgeLongHorizonMissionAdmissionError> {
        let identity = GovernedWriteIdentity::local_demo(admission.actor_id);
        self.admit_forge_long_horizon_mission_local_with_identity(admission, &identity)
    }

    pub fn admit_forge_long_horizon_mission_local_with_identity(
        &mut self,
        admission: ForgeLongHorizonMissionAdmission<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeLongHorizonMissionAdmissionReport, ForgeLongHorizonMissionAdmissionError> {
        for (field, value) in [
            ("tenant_id", admission.tenant_id),
            ("actor_id", admission.actor_id),
            ("actor_role", admission.actor_role),
            ("mission_id", admission.mission_id),
            ("goal", admission.goal),
            ("non_goals", admission.non_goals),
            ("constraints", admission.constraints),
            ("done_when", admission.done_when),
            ("allowed_write_scope", admission.allowed_write_scope),
            ("blocked_paths", admission.blocked_paths),
            ("validation_commands", admission.validation_commands),
            ("model_policy", admission.model_policy),
            ("provider_allowlist", admission.provider_allowlist),
        ] {
            if value.trim().is_empty() {
                return Err(ForgeLongHorizonMissionAdmissionError::Missing(field));
            }
        }
        if admission.fleet_width == 0 || admission.fleet_width > 512 {
            return Err(ForgeLongHorizonMissionAdmissionError::Invalid(
                "fleet_width",
            ));
        }
        if admission.max_runtime_ms < 60_000 {
            return Err(ForgeLongHorizonMissionAdmissionError::Invalid(
                "max_runtime_ms",
            ));
        }
        if admission.max_cost_cents == 0 {
            return Err(ForgeLongHorizonMissionAdmissionError::Invalid(
                "max_cost_cents",
            ));
        }
        if admission.checkpoint_cadence_minutes == 0 {
            return Err(ForgeLongHorizonMissionAdmissionError::Invalid(
                "checkpoint_cadence_minutes",
            ));
        }
        let actor_admission = admit_local_route_actor(
            admission.tenant_id,
            admission.actor_id,
            admission.actor_role,
            "/forge/long-horizon-missions.json",
            "forge.long_horizon_mission.admitted",
            admission.mission_id,
        )
        .map_err(|error| ForgeLongHorizonMissionAdmissionError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(admission.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(admission.actor_id),
            loop_id: LoopId::new("forge_long_horizon_mission"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision =
            self.decide_with_receipt(&correlation, ActionKind::AdmitForgeLongHorizonMission);
        let milestone_count = mission_milestones(admission.validation_commands).len() as u32;
        let fleet_width = admission.fleet_width.to_string();
        let max_runtime_ms = admission.max_runtime_ms.to_string();
        let max_cost_cents = admission.max_cost_cents.to_string();
        let checkpoint_cadence_minutes = admission.checkpoint_cadence_minutes.to_string();
        let milestone_count_string = milestone_count.to_string();
        let receipt = self.transition_receipt(
            &run_id,
            "ADMIT_FORGE_LONG_HORIZON_MISSION",
            &correlation,
            &decision,
            "forge.long_horizon_mission.admitted",
            payload(&[
                ("mission_id", admission.mission_id),
                ("source_route", "/forge/long-horizon-missions.json"),
                (
                    "projection_route",
                    "/forge/long-horizon-missions/projection.json",
                ),
                (
                    "dashboard_route",
                    "/forge/long-horizon-mission-dashboard.json",
                ),
                ("source_contract", "docs/FORGE-LONG-HORIZON-MISSIONS.md"),
                (
                    "architecture_contract",
                    "generated/architecture/forge-long-horizon-missions.json",
                ),
                ("goal", admission.goal),
                ("non_goals", admission.non_goals),
                ("constraints", admission.constraints),
                ("done_when", admission.done_when),
                ("allowed_write_scope", admission.allowed_write_scope),
                ("blocked_paths", admission.blocked_paths),
                ("validation_commands", admission.validation_commands),
                ("model_policy", admission.model_policy),
                ("provider_allowlist", admission.provider_allowlist),
                ("fleet_width", &fleet_width),
                ("max_runtime_ms", &max_runtime_ms),
                ("max_cost_cents", &max_cost_cents),
                ("checkpoint_cadence_minutes", &checkpoint_cadence_minutes),
                ("milestone_count", &milestone_count_string),
                ("spec_packet_recorded", "true"),
                ("milestone_plan_recorded", "true"),
                ("runbook_recorded", "true"),
                ("status_log_recorded", "true"),
                ("verification_after_each_milestone", "true"),
                ("dashboard_ready", "true"),
                ("human_checkpoint_required", "true"),
                ("live_provider_calls_allowed", "false"),
                ("adapter_execution_allowed", "false"),
                ("production_write_allowed", "false"),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                (
                    "blocked_reason",
                    "mission_admitted_waiting_for_live_execution_turn_on",
                ),
            ]),
        );
        self.finish_forge_long_horizon_mission_run(&run_id);
        Ok(ForgeLongHorizonMissionAdmissionReport {
            status: "FORGE_LONG_HORIZON_MISSION_ADMITTED",
            mission_id: admission.mission_id.to_string(),
            mission_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            milestone_count,
            fleet_width: admission.fleet_width,
            max_runtime_ms: admission.max_runtime_ms,
            max_cost_cents: admission.max_cost_cents,
            checkpoint_cadence_minutes: admission.checkpoint_cadence_minutes,
            spec_packet_recorded: true,
            milestone_plan_recorded: true,
            runbook_recorded: true,
            status_log_recorded: true,
            dashboard_ready: true,
            live_provider_calls_allowed: false,
            adapter_execution_allowed: false,
            production_write_allowed: false,
            blocked_reason: "mission_admitted_waiting_for_live_execution_turn_on",
        })
    }

    pub fn record_forge_long_horizon_mission_checkpoint_local(
        &mut self,
        checkpoint: ForgeLongHorizonMissionCheckpoint<'_>,
    ) -> Result<ForgeLongHorizonMissionCheckpointReport, ForgeLongHorizonMissionAdmissionError>
    {
        let identity = GovernedWriteIdentity::local_demo(checkpoint.actor_id);
        self.record_forge_long_horizon_mission_checkpoint_local_with_identity(checkpoint, &identity)
    }

    pub fn record_forge_long_horizon_mission_checkpoint_local_with_identity(
        &mut self,
        checkpoint: ForgeLongHorizonMissionCheckpoint<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeLongHorizonMissionCheckpointReport, ForgeLongHorizonMissionAdmissionError>
    {
        for (field, value) in [
            ("tenant_id", checkpoint.tenant_id),
            ("actor_id", checkpoint.actor_id),
            ("actor_role", checkpoint.actor_role),
            ("mission_id", checkpoint.mission_id),
            ("checkpoint_event", checkpoint.checkpoint_event),
            ("summary", checkpoint.summary),
        ] {
            if value.trim().is_empty() {
                return Err(ForgeLongHorizonMissionAdmissionError::Missing(field));
            }
        }
        if !checkpoint_event_allowed(checkpoint.checkpoint_event) {
            return Err(ForgeLongHorizonMissionAdmissionError::Invalid(
                "checkpoint_event",
            ));
        }
        if checkpoint_event_requires_milestone(checkpoint.checkpoint_event)
            && checkpoint.milestone_id.trim().is_empty()
        {
            return Err(ForgeLongHorizonMissionAdmissionError::Missing(
                "milestone_id",
            ));
        }

        let mission_receipt = self
            .ledger()
            .entries()
            .iter()
            .find(|receipt| {
                receipt.kind == "forge.long_horizon_mission.admitted"
                    && payload_value(receipt, "mission_id") == checkpoint.mission_id
            })
            .ok_or_else(|| {
                ForgeLongHorizonMissionAdmissionError::UnknownMission(
                    checkpoint.mission_id.to_string(),
                )
            })?;
        if !checkpoint.milestone_id.trim().is_empty() {
            let milestones =
                mission_milestones(payload_value(mission_receipt, "validation_commands"));
            if !milestones
                .iter()
                .any(|milestone| milestone.milestone_id == checkpoint.milestone_id)
            {
                return Err(ForgeLongHorizonMissionAdmissionError::UnknownMilestone(
                    checkpoint.milestone_id.to_string(),
                ));
            }
        }

        let actor_admission = admit_local_route_actor(
            checkpoint.tenant_id,
            checkpoint.actor_id,
            checkpoint.actor_role,
            "/forge/long-horizon-mission-checkpoints.json",
            "forge.long_horizon_mission.checkpointed",
            checkpoint.mission_id,
        )
        .map_err(|error| ForgeLongHorizonMissionAdmissionError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(checkpoint.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(checkpoint.actor_id),
            loop_id: LoopId::new("forge_long_horizon_mission_checkpoint"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision = self.decide_with_receipt(
            &correlation,
            ActionKind::RecordForgeLongHorizonMissionCheckpoint,
        );
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_FORGE_LONG_HORIZON_MISSION_CHECKPOINT",
            &correlation,
            &decision,
            "forge.long_horizon_mission.checkpointed",
            payload(&[
                ("mission_id", checkpoint.mission_id),
                ("milestone_id", checkpoint.milestone_id),
                ("checkpoint_event", checkpoint.checkpoint_event),
                (
                    "source_route",
                    "/forge/long-horizon-mission-checkpoints.json",
                ),
                (
                    "projection_route",
                    "/forge/long-horizon-missions/projection.json",
                ),
                (
                    "dashboard_route",
                    "/forge/long-horizon-mission-dashboard.json",
                ),
                ("source_contract", "docs/FORGE-LONG-HORIZON-MISSIONS.md"),
                ("summary", checkpoint.summary),
                ("validation_status", checkpoint.validation_status),
                ("related_run_id", checkpoint.related_run_id),
                ("related_fleet_id", checkpoint.related_fleet_id),
                ("steering_note", checkpoint.steering_note),
                ("checkpoint_recorded", "true"),
                ("live_provider_calls_allowed", "false"),
                ("adapter_execution_allowed", "false"),
                ("production_write_allowed", "false"),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
            ]),
        );
        self.finish_forge_long_horizon_mission_checkpoint_run(&run_id, checkpoint.checkpoint_event);
        Ok(ForgeLongHorizonMissionCheckpointReport {
            status: checkpoint_report_status(checkpoint.checkpoint_event),
            mission_id: checkpoint.mission_id.to_string(),
            milestone_id: checkpoint.milestone_id.to_string(),
            checkpoint_event: checkpoint.checkpoint_event.to_string(),
            checkpoint_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            mission_state: mission_state_for_checkpoint_event(checkpoint.checkpoint_event),
            live_provider_calls_allowed: false,
            adapter_execution_allowed: false,
            production_write_allowed: false,
        })
    }

    pub fn project_forge_long_horizon_missions(&self) -> ForgeLongHorizonMissionDashboard {
        let checkpoint_receipts = self
            .ledger()
            .entries()
            .iter()
            .filter(|receipt| receipt.kind == "forge.long_horizon_mission.checkpointed")
            .collect::<Vec<_>>();
        let packets = self
            .ledger()
            .entries()
            .iter()
            .filter(|receipt| receipt.kind == "forge.long_horizon_mission.admitted")
            .map(|receipt| {
                let validation_commands = payload_value(receipt, "validation_commands").to_string();
                let mission_id = payload_value(receipt, "mission_id").to_string();
                let mut packet = ForgeLongHorizonMissionPacket {
                    mission_id: mission_id.clone(),
                    goal: payload_value(receipt, "goal").to_string(),
                    non_goals: payload_value(receipt, "non_goals").to_string(),
                    constraints: payload_value(receipt, "constraints").to_string(),
                    done_when: payload_value(receipt, "done_when").to_string(),
                    allowed_write_scope: payload_value(receipt, "allowed_write_scope").to_string(),
                    blocked_paths: payload_value(receipt, "blocked_paths").to_string(),
                    validation_commands: validation_commands.clone(),
                    model_policy: payload_value(receipt, "model_policy").to_string(),
                    provider_allowlist: payload_value(receipt, "provider_allowlist").to_string(),
                    fleet_width: payload_value(receipt, "fleet_width").parse().unwrap_or(0),
                    max_runtime_ms: payload_value(receipt, "max_runtime_ms")
                        .parse()
                        .unwrap_or(0),
                    max_cost_cents: payload_value(receipt, "max_cost_cents")
                        .parse()
                        .unwrap_or(0),
                    checkpoint_cadence_minutes: payload_value(
                        receipt,
                        "checkpoint_cadence_minutes",
                    )
                    .parse()
                    .unwrap_or(0),
                    mission_receipt_id: receipt.receipt_id.clone(),
                    policy_decision_id: receipt.policy_decision_id.clone().unwrap_or_default(),
                    mission_state: "ADMITTED_WAITING_FOR_LOCAL_EXECUTION",
                    latest_checkpoint_event: String::new(),
                    latest_checkpoint_summary: String::new(),
                    checkpoint_count: 0,
                    completed_milestone_count: 0,
                    blocked_milestone_count: 0,
                    steering_note_count: 0,
                    pause_count: 0,
                    resume_count: 0,
                    related_run_ids: Vec::new(),
                    related_fleet_ids: Vec::new(),
                    milestones: mission_milestones(&validation_commands),
                };
                fold_mission_checkpoints(&mut packet, &checkpoint_receipts);
                packet
            })
            .collect::<Vec<_>>();
        let milestone_count = packets
            .iter()
            .map(|packet| packet.milestones.len() as u32)
            .sum();
        let completed_milestone_count = packets
            .iter()
            .map(|packet| packet.completed_milestone_count)
            .sum();
        let blocked_milestone_count = packets
            .iter()
            .map(|packet| packet.blocked_milestone_count)
            .sum();
        let active_mission_count = packets
            .iter()
            .filter(|packet| packet.mission_state != "COMPLETED_LOCAL_CHECKPOINTS")
            .count() as u32;
        ForgeLongHorizonMissionDashboard {
            status: if packets.is_empty() {
                "NO_MISSIONS_ADMITTED"
            } else {
                "LIVE-LOCAL-LONG-HORIZON-MISSION-DASHBOARD"
            },
            mission_count: packets.len() as u32,
            active_mission_count,
            milestone_count,
            completed_milestone_count,
            blocked_milestone_count,
            verification_cadence: "after_each_milestone",
            dashboard_ready: true,
            live_provider_calls_allowed: false,
            adapter_execution_allowed: false,
            production_write_allowed: false,
            human_checkpoint_required: true,
            provider_matrix_count: forge_fleet_model_matrix_profiles().len() as u32,
            benchmark_task_count: forge_fleet_benchmark_tasks().len() as u32,
            scoring_dimension_count: forge_fleet_scoring_dimensions().len() as u32,
            runner_profile_count: forge_fleet_runner_profiles().len() as u32,
            packets,
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    fn finish_forge_long_horizon_mission_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "FORGE_LONG_HORIZON_MISSION_ADMITTED".to_string();
        }
    }

    fn finish_forge_long_horizon_mission_checkpoint_run(
        &mut self,
        run_id: &str,
        checkpoint_event: &str,
    ) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = checkpoint_report_status(checkpoint_event).to_string();
        }
    }
}

pub fn mission_milestones(validation_commands: &str) -> Vec<ForgeLongHorizonMissionMilestone> {
    validation_commands
        .split(',')
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .enumerate()
        .map(|(index, command)| ForgeLongHorizonMissionMilestone {
            milestone_id: format!("mission_milestone_{:02}", index + 1),
            title: format!("Checkpoint {}", index + 1),
            validation_command: command.to_string(),
            status: "PENDING_LOCAL_EXECUTION",
            acceptance_criteria: format!("{command} passes and receipts remain coherent"),
            checkpoint_receipt_id: String::new(),
            validation_status: String::new(),
            related_run_id: String::new(),
            related_fleet_id: String::new(),
        })
        .collect()
}

pub fn mission_milestones_json(packet: &ForgeLongHorizonMissionPacket) -> String {
    packet
        .milestones
        .iter()
        .map(|milestone| {
            format!(
                r#"{{"milestone_id":{},"title":{},"validation_command":{},"status":{},"acceptance_criteria":{},"checkpoint_receipt_id":{},"validation_status":{},"related_run_id":{},"related_fleet_id":{}}}"#,
                json_string_literal(&milestone.milestone_id),
                json_string_literal(&milestone.title),
                json_string_literal(&milestone.validation_command),
                json_string_literal(milestone.status),
                json_string_literal(&milestone.acceptance_criteria),
                json_string_literal(&milestone.checkpoint_receipt_id),
                json_string_literal(&milestone.validation_status),
                json_string_literal(&milestone.related_run_id),
                json_string_literal(&milestone.related_fleet_id)
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn fold_mission_checkpoints(
    packet: &mut ForgeLongHorizonMissionPacket,
    checkpoint_receipts: &[&crate::Receipt],
) {
    for receipt in checkpoint_receipts
        .iter()
        .copied()
        .filter(|receipt| payload_value(receipt, "mission_id") == packet.mission_id)
    {
        let event = payload_value(receipt, "checkpoint_event");
        let milestone_id = payload_value(receipt, "milestone_id");
        packet.checkpoint_count += 1;
        packet.latest_checkpoint_event = event.to_string();
        packet.latest_checkpoint_summary = payload_value(receipt, "summary").to_string();
        if event == "mission_steered" {
            packet.steering_note_count += 1;
        }
        if event == "mission_paused" {
            packet.pause_count += 1;
        }
        if event == "mission_resumed" {
            packet.resume_count += 1;
        }
        push_unique(
            &mut packet.related_run_ids,
            payload_value(receipt, "related_run_id"),
        );
        push_unique(
            &mut packet.related_fleet_ids,
            payload_value(receipt, "related_fleet_id"),
        );
        if !milestone_id.is_empty()
            && let Some(milestone) = packet
                .milestones
                .iter_mut()
                .find(|milestone| milestone.milestone_id == milestone_id)
        {
            milestone.status = milestone_status_for_checkpoint_event(event);
            milestone.checkpoint_receipt_id = receipt.receipt_id.clone();
            milestone.validation_status = payload_value(receipt, "validation_status").to_string();
            milestone.related_run_id = payload_value(receipt, "related_run_id").to_string();
            milestone.related_fleet_id = payload_value(receipt, "related_fleet_id").to_string();
        }
    }
    packet.completed_milestone_count = packet
        .milestones
        .iter()
        .filter(|milestone| milestone.status == "COMPLETED_LOCAL_CHECKPOINT")
        .count() as u32;
    packet.blocked_milestone_count = packet
        .milestones
        .iter()
        .filter(|milestone| milestone.status == "BLOCKED_NEEDS_OPERATOR")
        .count() as u32;
    packet.mission_state = mission_state_for_packet(packet);
}

fn push_unique(values: &mut Vec<String>, value: &str) {
    if !value.trim().is_empty() && !values.iter().any(|existing| existing == value) {
        values.push(value.to_string());
    }
}

fn checkpoint_event_allowed(event: &str) -> bool {
    matches!(
        event,
        "milestone_started"
            | "milestone_completed"
            | "milestone_blocked"
            | "mission_paused"
            | "mission_resumed"
            | "mission_steered"
    )
}

fn checkpoint_event_requires_milestone(event: &str) -> bool {
    matches!(
        event,
        "milestone_started" | "milestone_completed" | "milestone_blocked"
    )
}

fn checkpoint_report_status(event: &str) -> &'static str {
    match event {
        "milestone_started" => "MISSION_MILESTONE_STARTED",
        "milestone_completed" => "MISSION_MILESTONE_COMPLETED",
        "milestone_blocked" => "MISSION_MILESTONE_BLOCKED",
        "mission_paused" => "MISSION_PAUSED_FOR_OPERATOR",
        "mission_resumed" => "MISSION_RESUMED",
        "mission_steered" => "MISSION_STEERED",
        _ => "MISSION_CHECKPOINT_RECORDED",
    }
}

fn milestone_status_for_checkpoint_event(event: &str) -> &'static str {
    match event {
        "milestone_started" => "RUNNING_LOCAL_CHECKPOINT",
        "milestone_completed" => "COMPLETED_LOCAL_CHECKPOINT",
        "milestone_blocked" => "BLOCKED_NEEDS_OPERATOR",
        _ => "PENDING_LOCAL_EXECUTION",
    }
}

fn mission_state_for_checkpoint_event(event: &str) -> &'static str {
    match event {
        "milestone_blocked" => "BLOCKED_NEEDS_OPERATOR",
        "mission_paused" => "PAUSED_FOR_OPERATOR",
        "mission_resumed" | "mission_steered" | "milestone_started" | "milestone_completed" => {
            "IN_PROGRESS_LOCAL_CHECKPOINTS"
        }
        _ => "ADMITTED_WAITING_FOR_LOCAL_EXECUTION",
    }
}

fn mission_state_for_packet(packet: &ForgeLongHorizonMissionPacket) -> &'static str {
    if !packet.milestones.is_empty()
        && packet.completed_milestone_count == packet.milestones.len() as u32
    {
        return "COMPLETED_LOCAL_CHECKPOINTS";
    }
    if packet.blocked_milestone_count > 0 {
        return "BLOCKED_NEEDS_OPERATOR";
    }
    if packet.latest_checkpoint_event == "mission_paused" {
        return "PAUSED_FOR_OPERATOR";
    }
    if !packet.latest_checkpoint_event.is_empty() {
        return "IN_PROGRESS_LOCAL_CHECKPOINTS";
    }
    "ADMITTED_WAITING_FOR_LOCAL_EXECUTION"
}

fn payload_value<'a>(receipt: &'a crate::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}
