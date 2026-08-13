use super::harness_execute::{
    FakeInScopeApplier, FakeSandboxRunner, build_request, deterministic_provider_profile,
    full_worker_admission, receipt_ids_of_kind, valid_plan_execute_manifest,
};
use super::*;

fn admission_scale_policy() -> ScaleBackpressurePolicy {
    ScaleBackpressurePolicy {
        max_concurrent_workers: 10,
        max_queue_depth: 20,
        per_tenant_max_concurrent: 4,
        high_priority_reserved_slots: 2,
        low_priority_shed_queue_depth: 10,
    }
}

#[test]
fn worker_admission_admits_under_capacity_and_runs_the_worker() {
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
            summary: "apply the proposed patch",
        },
        HarnessPlanExecutionAction {
            action_id: "test_001",
            action_kind: HarnessPlanExecutionActionKind::TestExecution,
            target: "cargo test -p mdx-core",
            summary: "run the tests in a sandbox",
        },
    ];
    let build = build_request(&plan, &provider, &actions);
    let artifacts = ["crates/mdx-core/src/lib.rs"];
    let request = HarnessWorkerRunRequest {
        admission: full_worker_admission("worker_run_admit"),
        parent_loop_id: "forge_build_loop",
        worker_template_id: "clean_build_worker",
        max_steps: 4,
        build: &build,
        output_artifacts: &artifacts,
        handoff_summary: "clean build",
        next_owner: "forge_operator",
    };
    let scale = WorkerScaleAdmissionContext {
        policy_id: "scale_v1",
        policy: admission_scale_policy(),
        priority: WorkerPriority::Normal,
        global_running: 0,
        global_queued: 0,
        tenant_running: 0,
    };
    let report = LocalHarnessWorkerRun
        .admit_and_run(
            &mut kernel,
            &manifest,
            &scale,
            &request,
            HarnessExecutionDrivers {
                patch_applier: Some(&FakeInScopeApplier),
                sandbox_runner: Some(&FakeSandboxRunner),
            },
        )
        .expect("admit and run");
    assert_eq!(report.status, "WORKER_ADMITTED");
    assert_eq!(report.decision, "admit");
    assert_eq!(report.queue_position, None);
    let run = report.run.expect("worker ran");
    assert_eq!(run.status, "WORKER_BUILD_COMPLETED");
    assert_eq!(
        kernel
            .ledger()
            .query()
            .by_id(&report.admission_receipt_id)
            .map(|r| r.kind.clone())
            .unwrap_or_default(),
        "worker.admission.admitted"
    );
    assert!(kernel.ledger().verify().is_ok());
}

#[test]
fn worker_admission_queues_sheds_and_blocks_invalid_policy_without_starting_a_worker() {
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
    let actions = [HarnessPlanExecutionAction {
        action_id: "apply_patch_001",
        action_kind: HarnessPlanExecutionActionKind::PatchApplication,
        target: "crates/mdx-core/src/lib.rs",
        summary: "apply the proposed patch",
    }];
    let build = build_request(&plan, &provider, &actions);
    let artifacts = ["crates/mdx-core/src/lib.rs"];
    let request = HarnessWorkerRunRequest {
        admission: full_worker_admission("worker_run_gated"),
        parent_loop_id: "forge_build_loop",
        worker_template_id: "clean_build_worker",
        max_steps: 4,
        build: &build,
        output_artifacts: &artifacts,
        handoff_summary: "clean build",
        next_owner: "forge_operator",
    };
    // Admit with the given scale context; a non-admit decision never starts one.
    let mut admit = |scale: WorkerScaleAdmissionContext<'_>| {
        LocalHarnessWorkerRun
            .admit_and_run(
                &mut kernel,
                &manifest,
                &scale,
                &request,
                HarnessExecutionDrivers {
                    patch_applier: Some(&FakeInScopeApplier),
                    sandbox_runner: Some(&FakeSandboxRunner),
                },
            )
            .expect("admit")
    };
    let base = |running: u32, queued: u32, tenant: u32, priority: WorkerPriority| {
        WorkerScaleAdmissionContext {
            policy_id: "scale_v1",
            policy: admission_scale_policy(),
            priority,
            global_running: running,
            global_queued: queued,
            tenant_running: tenant,
        }
    };

    // At capacity with queue room: queued, with a position, no worker started.
    let queued = admit(base(10, 3, 0, WorkerPriority::Normal));
    assert_eq!(queued.status, "WORKER_QUEUED");
    assert_eq!(queued.queue_position, Some(4));
    assert!(queued.run.is_none());

    // Tenant at its cap with the queue full: shed, no worker started.
    let shed = admit(base(5, 20, 4, WorkerPriority::Normal));
    assert_eq!(shed.status, "WORKER_SHED");
    assert!(shed.run.is_none());

    // Below-High work will not take the last reserved slots: queued.
    let reserved = admit(base(9, 0, 0, WorkerPriority::Normal));
    assert_eq!(reserved.status, "WORKER_QUEUED");

    // An invalid policy sheds rather than admitting.
    let mut bad = base(0, 0, 0, WorkerPriority::High);
    bad.policy.max_concurrent_workers = 0;
    let invalid = admit(bad);
    assert_eq!(invalid.status, "WORKER_SHED");
    assert_eq!(
        invalid.reason.as_deref(),
        Some("invalid_policy:max_concurrent_zero")
    );

    // No worker lifecycle receipts exist: nothing started.
    assert!(receipt_ids_of_kind(&kernel, "worker.delegated").is_empty());
    assert!(kernel.ledger().verify().is_ok());
}
