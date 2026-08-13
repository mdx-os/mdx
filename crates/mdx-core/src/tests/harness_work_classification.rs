use super::*;

fn request<'a>(ask: &'a str, declared_checks: &'a str) -> ForgeWorkClassificationRequest<'a> {
    ForgeWorkClassificationRequest {
        tenant_id: "local_tenant",
        actor_id: "agent:codex",
        actor_role: "operator",
        classification_id: "forge_work_classification_001",
        ask,
        repo: "MDx",
        declared_checks,
    }
}

fn external_request<'a>(
    repo: &'a str,
    ask: &'a str,
    declared_checks: &'a str,
) -> ForgeWorkClassificationRequest<'a> {
    ForgeWorkClassificationRequest {
        repo,
        ..request(ask, declared_checks)
    }
}

#[test]
fn forge_work_classification_routes_meaty_work_to_mission() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "Build a long horizon greenfield Forge mission with milestones and hundreds of agents",
            "make local-smoke",
        ))
        .expect("classification recorded");

    assert_eq!(report.status, "FORGE_WORK_CLASSIFICATION_RECORDED");
    assert_eq!(report.recommended_shape, "mission");
    assert_eq!(report.recommended_width, 8);
    assert_eq!(report.recommendation_label, "A mission");
    assert_eq!(report.next_route, "/forge/long-horizon-missions.json");
    assert_eq!(report.next_gate, "mission_admission");
    assert!(!report.new_project_supported);
    assert_eq!(report.task_class, "long_horizon");
    assert_eq!(report.complexity_tier, "xl");
    assert!(report.mission_recommended);
    assert_eq!(report.mission_route, "/forge/long-horizon-missions.json");
    assert_eq!(report.scoreboard_route, "/forge/fleet-eval-scoreboard.json");
    assert!(!report.classification_grants_execution_authority);
    assert!(!report.live_provider_calls_allowed);
    assert!(!report.adapter_execution_allowed);
    assert!(!report.production_write_allowed);

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.classification_receipt_id)
        .expect("classification receipt");
    assert_eq!(receipt.kind, "forge.work_classification.recorded");
    assert_eq!(receipt.payload["recommended_shape"], "mission");
    assert_eq!(receipt.payload["recommended_width"], "8");
    assert_eq!(
        receipt.payload["next_route"],
        "/forge/long-horizon-missions.json"
    );
    assert_eq!(receipt.payload["next_gate"], "mission_admission");
    assert_eq!(receipt.payload["task_class"], "long_horizon");
    assert_eq!(receipt.payload["complexity_tier"], "xl");
    assert_eq!(
        receipt.payload["classification_grants_execution_authority"],
        "false"
    );
}

#[test]
fn forge_work_classification_routes_extreme_work_to_decomposed_mission() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "Take on an extreme messy enterprise refactor: decompose the program, run multiple fleets, integrate the work, and have repair agents unblock failed lanes",
            "cargo test -p mdx-core",
        ))
        .expect("classification recorded");

    assert_eq!(report.recommended_shape, "mission");
    assert_eq!(report.recommended_width, 16);
    assert_eq!(report.next_gate, "mission_admission");
    assert_eq!(report.task_class, "refactor");
    assert_eq!(report.complexity_tier, "extreme");
    assert!(report.mission_recommended);
    assert!(report.rationale.contains("decompose"));
    assert!(report.rationale.contains("fan out workers"));
    assert!(report.rationale.contains("repair checkpoints"));

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.classification_receipt_id)
        .expect("classification receipt");
    assert_eq!(receipt.payload["complexity_tier"], "extreme");
    assert_eq!(receipt.payload["mission_recommended"], "true");
}

#[test]
fn forge_work_classification_keeps_small_bugfix_in_run_lane() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "Fix a small bug in the Forge route status copy",
            "cargo test -p mdx-core",
        ))
        .expect("classification recorded");

    assert_eq!(report.recommended_shape, "run");
    assert_eq!(report.recommended_width, 1);
    assert_eq!(report.recommendation_label, "A single run");
    assert_eq!(report.next_route, "/forge/runs.json");
    assert_eq!(report.next_gate, "run_start");
    assert_eq!(report.task_class, "bug_fix");
    assert_eq!(report.complexity_tier, "small");
    assert!(!report.mission_recommended);
    assert_eq!(report.run_route, "/forge/runs.json");
    assert_eq!(report.suggested_checks, "cargo test -p mdx-core");
}

#[test]
fn forge_work_classification_does_not_leak_mdx_scope_into_connected_repo() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(external_request(
            "classnames",
            "Add one focused regression test for nested arrays",
            "npm test",
        ))
        .expect("classification recorded");

    assert_eq!(report.repo, "classnames");
    assert_eq!(report.suggested_checks, "npm test");
    assert_eq!(report.allowed_write_scope, "");

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.classification_receipt_id)
        .expect("classification receipt");
    assert_eq!(receipt.payload["repo"], "classnames");
    assert_eq!(receipt.payload["allowed_write_scope"], "");
}

#[test]
fn connected_repo_named_mdx_does_not_gain_first_party_fallback_scope() {
    let mut kernel = MdxKernel::boot_local();
    kernel
        .connect_forge_repo(ForgeRepoConnect {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            repo_id: "mdx",
            label: "An unrelated MDx repository",
            root: "/tmp/external-mdx",
            kind: "remote",
            origin: "https://github.com/example/mdx",
        })
        .expect("external repository connection recorded");

    let report = kernel
        .classify_forge_work_local(external_request(
            "mdx",
            "Add one focused regression test",
            "npm test",
        ))
        .expect("classification recorded");

    assert_eq!(report.repo, "mdx");
    assert_eq!(report.allowed_write_scope, "");

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.classification_receipt_id)
        .expect("classification receipt");
    assert_eq!(receipt.payload["allowed_write_scope"], "");
}

#[test]
fn forge_work_classification_honors_docs_only_scope_before_path_and_pipeline_cues() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "Make two small documentation-only improvements: clarify README.md wording about isolated validation and docs/architecture.md wording about how evidence moves through the safety pipeline. Do not change Rust code or configuration.",
            "cargo test",
        ))
        .expect("classification recorded");

    assert_eq!(report.task_class, "docs_code");
    assert_eq!(report.complexity_tier, "small");
    assert_eq!(report.recommended_shape, "run");
    assert!(!report.mission_recommended);
    assert_eq!(report.allowed_write_scope, "readme.md,docs/architecture.md");
    assert_eq!(report.suggested_checks, "cargo test");
}

#[test]
fn forge_work_classification_keeps_simple_page_and_game_in_single_run_lane() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "Build a simple hello world test webpage and add a small game near the bottom",
            "npm test",
        ))
        .expect("classification recorded");

    assert_eq!(report.recommended_shape, "run");
    assert_eq!(report.recommended_width, 1);
    assert_eq!(report.recommendation_label, "A single run");
    assert_eq!(report.next_route, "/forge/runs.json");
    assert_eq!(report.next_gate, "run_start");
    assert_eq!(report.task_class, "product_ux");
    assert_eq!(report.complexity_tier, "small");
    assert!(!report.mission_recommended);
}

#[test]
fn forge_work_classification_keeps_small_native_mission_surface_polish_in_run_lane() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "Small bounded Mac Forge verification task: add a tiny readability polish to the native mission or run surface without changing behavior. Keep it to the macOS app, prefer the smallest safe edit, and prove it with make native-macos-operator-check.",
            "",
        ))
        .expect("classification recorded");

    assert_eq!(report.recommended_shape, "run");
    assert_eq!(report.recommended_width, 1);
    assert_eq!(report.next_route, "/forge/runs.json");
    assert_eq!(report.next_gate, "run_start");
    assert_eq!(report.task_class, "product_ux");
    assert_eq!(report.complexity_tier, "small");
    assert_eq!(report.suggested_checks, "make native-macos-operator-check");
    assert_eq!(report.allowed_write_scope, "apps/mdx-operator-macos/");
    assert!(!report.mission_recommended);
}

#[test]
fn forge_work_classification_routes_explicit_mission_setup_to_mission() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "Set up a mission for a bounded Forge implementation with milestones, checkpoints, and repair proof before workers start",
            "make local-smoke",
        ))
        .expect("classification recorded");

    assert_eq!(report.recommended_shape, "mission");
    assert_eq!(report.next_route, "/forge/long-horizon-missions.json");
    assert_eq!(report.next_gate, "mission_admission");
    assert!(report.mission_recommended);
}

#[test]
fn forge_work_classification_routes_serious_multi_file_work_to_mission() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "Do serious work across 10+ files with hundreds of lines of code and integrated tests",
            "make local-smoke",
        ))
        .expect("classification recorded");

    assert_eq!(report.recommended_shape, "mission");
    assert_eq!(report.recommended_width, 4);
    assert_eq!(report.next_route, "/forge/long-horizon-missions.json");
    assert_eq!(report.next_gate, "mission_admission");
    assert_eq!(report.task_class, "multi_file");
    assert_eq!(report.complexity_tier, "large");
    assert!(report.mission_recommended);
}

#[test]
fn forge_work_classification_does_not_downgrade_multi_file_bugfix_to_small() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "Fix the payment migration bug across dozens of files with integrated tests",
            "make local-smoke",
        ))
        .expect("classification recorded");

    assert_eq!(report.recommended_shape, "mission");
    assert_eq!(report.recommended_width, 4);
    assert_eq!(report.next_route, "/forge/long-horizon-missions.json");
    assert_eq!(report.next_gate, "mission_admission");
    assert_eq!(report.task_class, "multi_file");
    assert_eq!(report.complexity_tier, "large");
    assert!(report.mission_recommended);
}

#[test]
fn forge_work_classification_does_not_downgrade_high_or_wide_native_work_to_small() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "High complexity bounded implementation task: improve macOS Forge so completed high or wide runs make recovery and proof status readable in the native run detail without terminal polling.",
            "",
        ))
        .expect("classification recorded");

    assert_eq!(report.recommended_shape, "mission");
    assert_eq!(report.recommended_width, 4);
    assert_eq!(report.next_route, "/forge/long-horizon-missions.json");
    assert_eq!(report.next_gate, "mission_admission");
    assert_eq!(report.task_class, "multi_file");
    assert_eq!(report.complexity_tier, "large");
    assert_eq!(report.suggested_checks, "make native-macos-operator-check");
    assert_eq!(report.allowed_write_scope, "apps/mdx-operator-macos/");
    assert!(report.mission_recommended);
}

#[test]
fn forge_work_classification_does_not_treat_authority_copy_as_security() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "High complexity Mac Forge task: improve completed parallel run readability across the native run detail, inspector, and review flow without changing backend authority. This is high or wide work across multiple macOS files and should preserve the premium native feel. Prove it with make native-macos-operator-check.",
            "",
        ))
        .expect("classification recorded");

    assert_eq!(report.recommended_shape, "mission");
    assert_eq!(report.recommended_width, 4);
    assert_eq!(report.task_class, "multi_file");
    assert_eq!(report.complexity_tier, "large");
    assert_eq!(report.suggested_checks, "make native-macos-operator-check");
    assert_eq!(report.allowed_write_scope, "apps/mdx-operator-macos/");
    assert!(report.mission_recommended);
}

#[test]
fn forge_work_classification_keeps_real_auth_work_security() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "Fix the auth flow security regression for tenant authorization and prove it with cargo test -p mdx-server auth",
            "cargo test -p mdx-server auth",
        ))
        .expect("classification recorded");

    assert_eq!(report.task_class, "security");
    assert_eq!(report.suggested_checks, "cargo test -p mdx-server auth");
}

#[test]
fn forge_work_classification_projects_recorded_packets() {
    let mut kernel = MdxKernel::boot_local();
    kernel
        .classify_forge_work_local(request(
            "Refactor the model selector UI workflow",
            "make ui-build-check",
        ))
        .expect("classification recorded");

    let projection = kernel.project_forge_work_classifications();
    assert_eq!(
        projection.status,
        "LIVE-LOCAL-WORK-CLASSIFICATION-PROJECTION"
    );
    assert_eq!(projection.classification_count, 1);
    assert_eq!(projection.run_recommended_count, 1);
    assert_eq!(projection.mission_recommended_count, 0);
    assert!(!projection.live_provider_calls_allowed);
    assert!(!projection.adapter_execution_allowed);
    assert!(!projection.production_write_allowed);

    let packet = projection.packets.first().expect("classification packet");
    assert_eq!(packet.task_class, "refactor");
    assert_eq!(packet.recommended_shape, "run");
    assert_eq!(packet.recommended_width, 2);
    assert_eq!(packet.next_route, "/forge/runs.json");
    assert_eq!(packet.next_gate, "run_start");
    assert_eq!(packet.suggested_checks, "make ui-build-check");
}

#[test]
fn forge_work_classification_routes_five_plus_width_to_fleet_plan() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "Build 8 lanes of alternative implementations for the product UX workflow",
            "make ui-build-check",
        ))
        .expect("classification recorded");

    assert_eq!(report.recommended_shape, "run");
    assert_eq!(report.recommended_width, 8);
    assert_eq!(report.recommendation_label, "A fleet");
    assert_eq!(report.next_route, "/forge/fleet-plans.json");
    assert_eq!(report.next_gate, "fleet_plan_ratification");
}

#[test]
fn forge_work_classification_keeps_parallel_candidate_ui_work_in_run_lane() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "Medium Mac Forge implementation task: improve native run detail readability for completed parallel work without changing backend behavior. Keep the work scoped to apps/mdx-operator-macos/, use two candidate lanes, and prove it with make native-macos-operator-check.",
            "",
        ))
        .expect("classification recorded");

    assert_eq!(report.recommended_shape, "run");
    assert_eq!(report.recommended_width, 2);
    assert_eq!(report.recommendation_label, "Parallel lanes");
    assert_eq!(report.next_route, "/forge/runs.json");
    assert_eq!(report.next_gate, "run_start");
    assert_eq!(report.task_class, "product_ux");
    assert_eq!(report.complexity_tier, "medium");
    assert_eq!(report.suggested_checks, "make native-macos-operator-check");
    assert_eq!(report.allowed_write_scope, "apps/mdx-operator-macos/");
    assert!(!report.mission_recommended);
}

#[test]
fn forge_work_classification_routes_explicit_twenty_four_lane_fleet_to_fleet_plan() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .classify_forge_work_local(request(
            "Run a fleet of 24 lanes for independent product UX implementation candidates",
            "make ui-build-check",
        ))
        .expect("classification recorded");

    assert_eq!(report.recommended_shape, "run");
    assert_eq!(report.recommended_width, 24);
    assert_eq!(report.recommendation_label, "A fleet");
    assert_eq!(report.next_route, "/forge/fleet-plans.json");
    assert_eq!(report.next_gate, "fleet_plan_ratification");
    assert!(!report.mission_recommended);
}

#[test]
fn forge_work_classification_refuses_empty_ask() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .classify_forge_work_local(request("", "make local-smoke"))
        .expect_err("empty ask refused");

    assert!(matches!(
        error,
        ForgeWorkClassificationError::Missing("ask")
    ));
    assert!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.work_classification.recorded")
            .is_empty()
    );
}
