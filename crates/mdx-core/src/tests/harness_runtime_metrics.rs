use super::harness_execute::{
    FakeInScopeApplier, FakeSandboxRunner, build_request, deterministic_provider_profile,
    full_worker_admission, run_to_review_packet, valid_plan_execute_manifest,
};
use super::*;

#[test]
fn metrics_project_builds_sandbox_durations_and_standing_targets() {
    // A full build (worker.build.recorded + harness.test.executed). The
    // autonomous-completion dispositions this test once exercised belonged
    // to the deleted DXR/Talent generation.
    let (kernel, _manifest, _packet_id) = run_to_review_packet();

    let m = LocalRuntimeMetrics.project(&kernel);
    assert!(m.builds_completed >= 1);
    assert!(
        m.sandbox.count >= 1,
        "sandbox duration observed from receipts"
    );
    assert!(m.sandbox.max_ms >= m.sandbox.min_ms);
    // Honest standing targets and posture, no invented latency percentiles.
    assert_eq!(m.measurement_source, "observed_local");
    assert!(!m.latency_percentiles_observed);
    assert_eq!(m.target_engineers, 1000);
    assert_eq!(m.target_peak_forge_builds, 5000);
    assert!(m.not_yet_instrumented.contains(&"model_latency_ms"));
    assert!(!m.overload_posture.is_empty());
}

#[test]
fn metrics_count_admission_outcomes() {
    let mut kernel = MdxKernel::boot_local();
    let manifest = valid_plan_execute_manifest();
    let plan = LocalHarnessPlanRunner
        .run_plan_only(&mut kernel, &manifest)
        .expect("plan-only");
    let profiles = [deterministic_provider_profile()];
    let packs = [deterministic_harness_enterprise_pack()];
    let provider = LocalHarnessProviderGateway
        .select_profile(
            &mut kernel,
            &manifest,
            &HarnessProviderGatewayRequest {
                requested_profile_id: "deterministic_stub",
                requested_mode: "plan_only",
                profiles: &profiles,
                enterprise_packs: &packs,
            },
        )
        .expect("provider");
    let actions = [
        HarnessPlanExecutionAction {
            action_id: "apply_patch_001",
            action_kind: HarnessPlanExecutionActionKind::PatchApplication,
            target: "crates/mdx-core/src/lib.rs",
            summary: "apply",
        },
        HarnessPlanExecutionAction {
            action_id: "test_001",
            action_kind: HarnessPlanExecutionActionKind::TestExecution,
            target: "cargo test -p mdx-core",
            summary: "test",
        },
    ];
    let build = build_request(&plan, &provider, &actions);
    let artifacts = ["crates/mdx-core/src/lib.rs"];
    let request = HarnessWorkerRunRequest {
        admission: full_worker_admission("worker_run_metrics"),
        parent_loop_id: "forge_build_loop",
        worker_template_id: "clean_build_worker",
        max_steps: 4,
        build: &build,
        output_artifacts: &artifacts,
        handoff_summary: "clean build",
        next_owner: "forge_operator",
    };
    let policy = ScaleBackpressurePolicy {
        max_concurrent_workers: 10,
        max_queue_depth: 20,
        per_tenant_max_concurrent: 4,
        high_priority_reserved_slots: 2,
        low_priority_shed_queue_depth: 10,
    };
    let drivers = || HarnessExecutionDrivers {
        patch_applier: Some(&FakeInScopeApplier),
        sandbox_runner: Some(&FakeSandboxRunner),
    };
    // One admit (runs), one queue (at capacity).
    LocalHarnessWorkerRun
        .admit_and_run(
            &mut kernel,
            &manifest,
            &WorkerScaleAdmissionContext {
                policy_id: "scale_v1",
                policy,
                priority: WorkerPriority::Normal,
                global_running: 0,
                global_queued: 0,
                tenant_running: 0,
            },
            &request,
            drivers(),
        )
        .expect("admit");
    LocalHarnessWorkerRun
        .admit_and_run(
            &mut kernel,
            &manifest,
            &WorkerScaleAdmissionContext {
                policy_id: "scale_v1",
                policy,
                priority: WorkerPriority::Normal,
                global_running: 10,
                global_queued: 1,
                tenant_running: 0,
            },
            &request,
            drivers(),
        )
        .expect("queue");

    let m = LocalRuntimeMetrics.project(&kernel);
    assert_eq!(m.admitted, 1);
    assert_eq!(m.queued, 1);
    assert_eq!(m.shed, 0);
    assert!(m.builds_completed >= 1, "the admitted worker built");
}

#[test]
fn metrics_project_timed_forge_run_events_without_fabricating_cost() {
    let mut kernel = MdxKernel::boot_local();
    let event = |kernel: &mut MdxKernel, event, detail, duration_ms, input, output| {
        kernel
            .record_forge_run_event_with_duration(
                ForgeRunEvent {
                    tenant_id: "tenant",
                    actor_id: "agent:forge",
                    run_id: "forge_run_metrics",
                    event,
                    work_item_id: "work_1",
                    detail,
                    turn: 1,
                    input_tokens: input,
                    output_tokens: output,
                },
                duration_ms,
            )
            .expect("timed forge event");
    };
    event(
        &mut kernel,
        "evidence_appended",
        "phase=intake context_chars=12 standards=0 outcomes=0 active_memories=0 installed_capabilities=0",
        3,
        0,
        0,
    );
    event(
        &mut kernel,
        "model_called",
        "model=qwen finish_reason=tool_calls tool_calls=1",
        44,
        1200,
        220,
    );
    event(
        &mut kernel,
        "tool_executed",
        "read_file crates/mdx-core/src/lib.rs",
        5,
        0,
        0,
    );
    event(
        &mut kernel,
        "check_passed",
        "run_command cargo test -p mdx-core exit=0",
        80,
        0,
        0,
    );
    event(
        &mut kernel,
        "run_finished",
        "status=RUN_FINISHED_DONE turns=1 files_changed=0",
        140,
        0,
        0,
    );
    let m = LocalRuntimeMetrics.project(&kernel);
    assert_eq!(m.forge_intake.mean_ms, 3);
    assert_eq!(m.forge_plan.mean_ms, 44);
    assert_eq!(m.forge_model.mean_ms, 44);
    assert_eq!(m.forge_execute.count, 2);
    assert_eq!(m.forge_execute.mean_ms, 42);
    assert_eq!(m.forge_total_run.mean_ms, 140);
    assert_eq!(m.forge_model_input_tokens, 1200);
    assert_eq!(m.forge_model_output_tokens, 220);
    assert_eq!(m.forge_model_attribution[0].model, "qwen");
    assert_eq!(m.forge_tool_attribution.len(), 2);
    assert!(!m.not_yet_instrumented.contains(&"model_latency_ms"));
    assert!(!m.not_yet_instrumented.contains(&"total_run_time_ms"));
    assert!(m.not_yet_instrumented.contains(&"cost_budget_use"));
    assert!(m.not_yet_instrumented.contains(&"review_phase_latency_ms"));
    assert!(m.not_yet_instrumented.contains(&"ship_phase_latency_ms"));
}
