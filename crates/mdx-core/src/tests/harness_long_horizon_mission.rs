use super::*;

fn admission<'a>() -> ForgeLongHorizonMissionAdmission<'a> {
    ForgeLongHorizonMissionAdmission {
        tenant_id: "local_tenant",
        actor_id: "agent:codex",
        actor_role: "operator",
        mission_id: "long_horizon_mission_001",
        goal: "Build the Forge long-horizon mission cockpit",
        non_goals: "no live provider calls,no adapter execution,no production writes",
        constraints: "bounded scope,receipt-backed checkpoints,verification after every milestone",
        done_when: "mission admission,projection,dashboard,schemas,docs,and checks cohere",
        allowed_write_scope: "crates/mdx-core/src/,crates/mdx-server/src/,crates/mdx-codegen/src/,docs/,scripts/",
        blocked_paths: "provider secrets,production state,live adapter execution",
        validation_commands: "make forge-long-horizon-mission-check,cargo test -p mdx-core harness_long_horizon_mission,cargo check -p mdx-server",
        model_policy: "mdx_approved_responses_compatible_provider_profiles_only",
        provider_allowlist: "gemini,anthropic,xai,aws_bedrock",
        fleet_width: 128,
        max_runtime_ms: 86_400_000,
        max_cost_cents: 1_000,
        checkpoint_cadence_minutes: 30,
    }
}

#[test]
fn forge_long_horizon_mission_admission_records_goal_packets() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .admit_forge_long_horizon_mission_local(admission())
        .expect("mission admitted");

    assert_eq!(report.status, "FORGE_LONG_HORIZON_MISSION_ADMITTED");
    assert_eq!(report.milestone_count, 3);
    assert_eq!(report.fleet_width, 128);
    assert!(report.spec_packet_recorded);
    assert!(report.milestone_plan_recorded);
    assert!(report.runbook_recorded);
    assert!(report.status_log_recorded);
    assert!(report.dashboard_ready);
    assert!(!report.live_provider_calls_allowed);
    assert!(!report.adapter_execution_allowed);
    assert!(!report.production_write_allowed);

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.mission_receipt_id)
        .expect("mission receipt");
    assert_eq!(receipt.kind, "forge.long_horizon_mission.admitted");
    assert_eq!(
        receipt.payload["goal"],
        "Build the Forge long-horizon mission cockpit"
    );
    assert_eq!(receipt.payload["verification_after_each_milestone"], "true");
    assert_eq!(receipt.payload["dashboard_ready"], "true");
    assert_eq!(receipt.payload["adapter_execution_allowed"], "false");
}

#[test]
fn forge_long_horizon_mission_dashboard_projects_mission_context() {
    let mut kernel = MdxKernel::boot_local();
    kernel
        .admit_forge_long_horizon_mission_local(admission())
        .expect("mission admitted");

    let dashboard = kernel.project_forge_long_horizon_missions();
    assert_eq!(
        dashboard.status,
        "LIVE-LOCAL-LONG-HORIZON-MISSION-DASHBOARD"
    );
    assert_eq!(dashboard.mission_count, 1);
    assert_eq!(dashboard.active_mission_count, 1);
    assert_eq!(dashboard.milestone_count, 3);
    assert_eq!(dashboard.verification_cadence, "after_each_milestone");
    assert_eq!(dashboard.provider_matrix_count, 14);
    assert_eq!(dashboard.benchmark_task_count, 15);
    assert_eq!(dashboard.scoring_dimension_count, 11);
    assert_eq!(dashboard.runner_profile_count, 9);
    assert!(dashboard.human_checkpoint_required);
    assert!(!dashboard.live_provider_calls_allowed);

    let packet = dashboard.packets.first().expect("mission packet");
    assert_eq!(packet.mission_id, "long_horizon_mission_001");
    assert_eq!(packet.milestones.len(), 3);
    assert_eq!(packet.milestones[0].status, "PENDING_LOCAL_EXECUTION");
    assert_eq!(
        packet.model_policy,
        "mdx_approved_responses_compatible_provider_profiles_only"
    );
}

#[test]
fn forge_long_horizon_mission_checkpoint_updates_projection() {
    let mut kernel = MdxKernel::boot_local();
    kernel
        .admit_forge_long_horizon_mission_local(admission())
        .expect("mission admitted");

    let report = kernel
        .record_forge_long_horizon_mission_checkpoint_local(ForgeLongHorizonMissionCheckpoint {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            actor_role: "operator",
            mission_id: "long_horizon_mission_001",
            milestone_id: "mission_milestone_01",
            checkpoint_event: "milestone_completed",
            summary: "First proof passed in the isolated mission worktree",
            validation_status: "passed",
            related_run_id: "forge_run_001",
            related_fleet_id: "forge_fleet_001",
            steering_note: "",
        })
        .expect("checkpoint recorded");

    assert_eq!(report.status, "MISSION_MILESTONE_COMPLETED");
    assert_eq!(report.mission_state, "IN_PROGRESS_LOCAL_CHECKPOINTS");
    assert!(!report.live_provider_calls_allowed);
    assert!(!report.adapter_execution_allowed);
    assert!(!report.production_write_allowed);

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.checkpoint_receipt_id)
        .expect("checkpoint receipt");
    assert_eq!(receipt.kind, "forge.long_horizon_mission.checkpointed");
    assert_eq!(receipt.payload["checkpoint_event"], "milestone_completed");
    assert_eq!(receipt.payload["production_write_allowed"], "false");

    let dashboard = kernel.project_forge_long_horizon_missions();
    assert_eq!(dashboard.completed_milestone_count, 1);
    assert_eq!(dashboard.blocked_milestone_count, 0);
    assert_eq!(dashboard.active_mission_count, 1);

    let packet = dashboard.packets.first().expect("mission packet");
    assert_eq!(packet.mission_state, "IN_PROGRESS_LOCAL_CHECKPOINTS");
    assert_eq!(packet.checkpoint_count, 1);
    assert_eq!(packet.completed_milestone_count, 1);
    assert_eq!(packet.related_run_ids, vec!["forge_run_001".to_string()]);
    assert_eq!(
        packet.related_fleet_ids,
        vec!["forge_fleet_001".to_string()]
    );
    assert_eq!(
        packet.latest_checkpoint_summary,
        "First proof passed in the isolated mission worktree"
    );
    assert_eq!(packet.milestones[0].status, "COMPLETED_LOCAL_CHECKPOINT");
    assert_eq!(packet.milestones[0].validation_status, "passed");
    assert_eq!(packet.milestones[0].related_run_id, "forge_run_001");
}

#[test]
fn forge_long_horizon_mission_checkpoint_refuses_unknown_mission_or_milestone() {
    let mut kernel = MdxKernel::boot_local();
    let missing_mission = kernel
        .record_forge_long_horizon_mission_checkpoint_local(ForgeLongHorizonMissionCheckpoint {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            actor_role: "operator",
            mission_id: "missing_mission",
            milestone_id: "mission_milestone_01",
            checkpoint_event: "milestone_started",
            summary: "try a missing mission",
            validation_status: "",
            related_run_id: "",
            related_fleet_id: "",
            steering_note: "",
        })
        .expect_err("missing mission refused");
    assert!(matches!(
        missing_mission,
        ForgeLongHorizonMissionAdmissionError::UnknownMission(_)
    ));

    kernel
        .admit_forge_long_horizon_mission_local(admission())
        .expect("mission admitted");
    let missing_milestone = kernel
        .record_forge_long_horizon_mission_checkpoint_local(ForgeLongHorizonMissionCheckpoint {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            actor_role: "operator",
            mission_id: "long_horizon_mission_001",
            milestone_id: "mission_milestone_99",
            checkpoint_event: "milestone_started",
            summary: "try a missing milestone",
            validation_status: "",
            related_run_id: "",
            related_fleet_id: "",
            steering_note: "",
        })
        .expect_err("missing milestone refused");
    assert!(matches!(
        missing_milestone,
        ForgeLongHorizonMissionAdmissionError::UnknownMilestone(_)
    ));
    assert!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.long_horizon_mission.checkpointed")
            .is_empty()
    );
}

#[test]
fn forge_long_horizon_mission_refuses_invalid_fleet_width() {
    let mut kernel = MdxKernel::boot_local();
    let mut request = admission();
    request.fleet_width = 0;
    let error = kernel
        .admit_forge_long_horizon_mission_local(request)
        .expect_err("invalid fleet width refused");

    assert!(matches!(
        error,
        ForgeLongHorizonMissionAdmissionError::Invalid("fleet_width")
    ));
    assert!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.long_horizon_mission.admitted")
            .is_empty()
    );
}
