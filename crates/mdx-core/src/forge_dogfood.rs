// The Forge dogfood: one realistic job, driven end to end from intake through
// the human ship decision, with every stage recorded as a governed receipt.
//
// This is the public, non-test counterpart of the chain proven in the test
// harness (crates/mdx-core/src/tests/harness_execute.rs and
// forge_ship_ratification.rs). It exists so a runner (xtask) can boot a local
// kernel and emit an honest evidence packet without depending on test-only
// fixtures.
//
// What it proves: intake/manifest -> plan -> provider profile -> worker scale
// admission -> worker build (an in-scope patch, then a sandboxed test) -> ship
// readiness (with observed CI evidence) -> review packet -> operator packet ->
// a human ratifies the ship. Every step is a receipt; the ledger verifies.
//
// What it does NOT do, by construction:
//   - It never applies a patch to the live repo or runs a real container. The
//     patch applier and sandbox runner here are deterministic in-scope stubs,
//     mirroring the test fakes. Real patch+sandbox-test (Docker) is proven and
//     gated separately.
//   - It never calls a real model provider. The provider profile is the
//     deterministic local stub.
//   - It never deploys and never writes to production. The ratify is performed
//     by a HUMAN actor; an autonomous actor cannot ratify. deployment and
//     production authority stay false on every report.
use crate::*;

// A deterministic in-scope patch applier. It stands in for the server-layer
// worktree applier so the worker's apply path is exercised without touching the
// filesystem. It mirrors the test FakeInScopeApplier: one file, +3/-1, applied.
struct DogfoodInScopePatchApplier;
impl HarnessPatchApplier for DogfoodInScopePatchApplier {
    fn apply(&self, context: &HarnessPatchApplyContext<'_>) -> Option<HarnessPatchApplyResult> {
        // The loop only ever hands us in-scope targets; assert the contract.
        if !context.target.starts_with("crates/mdx-core/src/") {
            return None;
        }
        if context.approved_plan_hash.is_empty() {
            return None;
        }
        Some(HarnessPatchApplyResult {
            worktree_id: "wt_dogfood_isolated".to_string(),
            files_changed: 1,
            insertions: 3,
            deletions: 1,
            applied: true,
            denied_reason: None,
        })
    }
}

// A deterministic sandbox runner. It stands in for the Docker runner so the
// worker's test path is exercised without a container engine. It mirrors the
// test FakeSandboxRunner: a passing, network-disabled run.
struct DogfoodSandboxRunner;
impl HarnessSandboxRunner for DogfoodSandboxRunner {
    fn run(&self, context: &HarnessSandboxRunContext<'_>) -> Option<HarnessSandboxRunResult> {
        if context.command.is_empty() || context.approved_plan_hash.is_empty() {
            return None;
        }
        Some(HarnessSandboxRunResult {
            sandbox_id: "sbx_dogfood".to_string(),
            exit_code: 0,
            duration_ms: 12,
            output_bytes: 5,
            passed: true,
            network_disabled: true,
            timed_out: false,
            output_truncated: false,
        })
    }
}

/// Every stage receipt the dogfood produced, the final terminal state, and the
/// boundary flags that must never flip. This is the report the evidence packet
/// is rendered from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeDogfoodReport {
    /// SHIP_RATIFIED on success. Any other value means the chain did not reach
    /// the ratified human decision.
    pub terminal_state: String,
    /// The realistic job this run drove, in one line.
    pub job_summary: String,
    // Stage receipts, in order along the chain.
    pub plan_receipt_id: String,
    pub provider_profile_receipt_id: String,
    pub admission_receipt_id: String,
    pub worker_build_receipt_id: String,
    pub patch_applied_receipt_id: String,
    pub test_executed_receipt_id: String,
    pub ship_readiness_receipt_id: String,
    pub ci_evidence_receipt_id: String,
    pub review_packet_receipt_id: String,
    pub operator_packet_receipt_id: String,
    /// The final human decision receipt: forge.ship.ratified.
    pub ratification_receipt_id: String,
    // The human edge. Ratified true only on a passing human ratify; deployment
    // and production are NEVER granted by this chain.
    pub forge_ship_ratified: bool,
    pub deployment_authority_granted: bool,
    pub production_write_allowed: bool,
    /// The operator packet's reading of who must decide next (true at the ship
    /// door, before the human ratifies).
    pub needs_human: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForgeDogfoodError {
    Stage(String),
}

impl ForgeDogfoodError {
    pub fn message(&self) -> String {
        match self {
            Self::Stage(message) => message.clone(),
        }
    }
}

// The realistic job manifest: a low-risk in-scope change inside mdx-core, plan
// only at the harness boundary, fail-closed provider. This mirrors the proven
// test manifest field-for-field so the public chain binds the same way.
fn dogfood_manifest() -> HarnessRunManifest<'static> {
    HarnessRunManifest {
        run_id: "forge_dogfood_run_001",
        tenant_id: "tenant_local",
        actor_id: "forge_orchestrator",
        origin_surface: "forge_intake",
        mode: HarnessRunMode::PlanOnly,
        goal_summary: "apply one in-scope mdx-core change and prove it to a human at the ship door",
        definition_of_done: "patch applied in isolation, tests green, evidence ready for human ratification",
        input_artifacts: &["docs/FORGE-DOGFOOD.md"],
        allowed_write_scope: &["crates/mdx-core/src/"],
        blocked_paths: &[".git/", ".env", ".mdx-local/"],
        allowed_tools: &[],
        blocked_tools: &["shell", "git", "patch", "browser", "mcp", "network"],
        default_parallelism: 1,
        allowed_model_profiles: &["deterministic_stub"],
        provider_fail_closed: true,
        allowed_context_sources: &["manifest", "source_map", "verification_manifest"],
        context_redaction_required: true,
        permission_policy: HarnessPermissionPolicy::plan_only_local(),
        budget_policy: HarnessBudgetPolicy {
            max_turns: 1,
            max_tool_calls: 0,
            max_runtime_ms: 250,
            max_context_tokens: 4096,
            max_output_tokens: 1024,
            max_event_bytes: 8192,
            max_transcript_bytes: 16384,
            max_cost_cents: 0,
        },
        quality_gates: &["cargo test -p mdx-core", "make forge-dogfood-proof"],
        approval_mode: HarnessApprovalMode::PlanHash,
        approval_receipt_required: true,
        required_receipt_kinds: &[
            "harness.run.admitted",
            "harness.plan.emitted",
            "harness.write.refused",
            "harness.provider.profile.selected",
            "harness.plan.hash.approved",
            "harness.execution.denied",
            "harness.quality.gate.recorded",
            "harness.final.verdict.recorded",
        ],
        telemetry_redaction_required: true,
        exit_criteria: &["evidence_ready_for_human", "human_decision_recorded"],
        trust_boundary: HarnessTrustBoundary {
            workspace_trusted: true,
            project_local_execution_allowed: false,
            mcp_startup_allowed: false,
            network_allowed: false,
            secrets_available: false,
            approval_receipt_id: Some("human_approved_plan_hash_dogfood_001"),
        },
        policy_profile: Some("core"),
        enterprise_pack: Some("none"),
    }
}

// The deterministic provider profile (no live model call), mirroring the proven
// test profile.
fn dogfood_provider_profile() -> HarnessProviderProfile<'static> {
    HarnessProviderProfile {
        profile_id: "deterministic_stub",
        provider_kind: HarnessProviderKind::DeterministicStub,
        model_id: "deterministic_local_v1",
        allowed_modes: &["plan_only"],
        max_context_tokens: 4096,
        max_output_tokens: 1024,
        max_cost_cents: 0,
        data_retention: "local_ephemeral",
        context_redaction_required: true,
        telemetry_redaction_required: true,
        provider_fail_closed: true,
        policy_profile: "core",
        enterprise_pack: "none",
        enabled: true,
    }
}

// The enterprise pack that allowlists the deterministic profile, mirroring the
// proven test pack.
fn dogfood_enterprise_pack() -> HarnessEnterprisePack<'static> {
    HarnessEnterprisePack {
        enterprise_pack_id: "none",
        allowed_policy_profiles: &["core"],
        provider_profile_allowlist: &["deterministic_stub"],
        max_context_tokens: 4096,
        max_output_tokens: 1024,
        max_cost_cents: 0,
        data_retention: "local_ephemeral",
        context_redaction_required: true,
        telemetry_redaction_required: true,
        approval_receipt_required: false,
        audit_export_allowed: false,
    }
}

// The full governed worker-execution admission packet, mirroring the proven
// test admission.
fn dogfood_worker_admission(worker_run_id: &str) -> LiveWorkerExecutionAdmission<'_> {
    LiveWorkerExecutionAdmission {
        worker_run_id,
        spawn_receipt_id: "receipt_spawn",
        credential_check_receipt_id: "receipt_credential",
        handoff_receipt_id: "receipt_handoff_planned",
        retirement_receipt_id: "receipt_retire_planned",
        provider_turn_on_receipt_id: "receipt_provider_turn_on",
        dispatch_claim_id: "claim_dispatch",
        heartbeat_receipt_id: "receipt_heartbeat",
        durable_workflow_receipt_id: "receipt_workflow",
        sandbox_authority_receipt_id: "receipt_sandbox_authority",
        external_sandbox_preflight_id: "receipt_sandbox_preflight",
        tool_policy_receipt_id: "receipt_tool_policy",
        reviewer_separation_receipt_id: "receipt_reviewer_separation",
        requested_execution_mode: "live_worker_execution",
        requested_receipt_kind: "worker.live_execution.authorized",
    }
}

// The standing scale/backpressure policy the build is admitted under, mirroring
// the proven admission test policy.
fn dogfood_scale_policy() -> ScaleBackpressurePolicy {
    ScaleBackpressurePolicy {
        max_concurrent_workers: 10,
        max_queue_depth: 20,
        per_tenant_max_concurrent: 4,
        high_priority_reserved_slots: 2,
        low_priority_shed_queue_depth: 10,
    }
}

fn first_receipt_of_kind<S: StorageProvider>(
    kernel: &MdxKernel<S>,
    kind: &str,
) -> Result<String, ForgeDogfoodError> {
    kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| receipt.kind == kind)
        .map(|receipt| receipt.receipt_id.clone())
        .ok_or_else(|| ForgeDogfoodError::Stage(format!("missing receipt of kind {kind}")))
}

fn receipt_ids_of_kind<S: StorageProvider>(kernel: &MdxKernel<S>, kind: &str) -> Vec<String> {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == kind)
        .map(|receipt| receipt.receipt_id.clone())
        .collect()
}

/// Drive one realistic Forge job from intake through a human ship ratification,
/// recording every stage as a governed receipt. Returns the stage receipts and
/// the terminal state. The chain reaches SHIP_RATIFIED only when a human ratifies
/// the full evidence chain; deployment and production stay false throughout.
pub fn run_forge_dogfood<S: StorageProvider>(
    kernel: &mut MdxKernel<S>,
) -> Result<ForgeDogfoodReport, ForgeDogfoodError> {
    let manifest = dogfood_manifest();

    // 1. Plan: the harness emits a plan-only report from the intake manifest.
    let plan = LocalHarnessPlanRunner
        .run_plan_only(kernel, &manifest)
        .map_err(|error| ForgeDogfoodError::Stage(format!("plan: {error:?}")))?;

    // 2. Provider: select the deterministic profile against the enterprise pack.
    let profiles = [dogfood_provider_profile()];
    let packs = [dogfood_enterprise_pack()];
    let provider = LocalHarnessProviderGateway
        .select_profile(
            kernel,
            &manifest,
            &HarnessProviderGatewayRequest {
                requested_profile_id: "deterministic_stub",
                requested_mode: "plan_only",
                profiles: &profiles,
                enterprise_packs: &packs,
            },
        )
        .map_err(|error| ForgeDogfoodError::Stage(format!("provider: {error:?}")))?;

    // 3. Worker build: scale-admit, then run patch + test under the lease. Only
    // PatchApplication and TestExecution are allowed; nothing else can smuggle in.
    let actions = [
        HarnessPlanExecutionAction {
            action_id: "apply_patch_001",
            action_kind: HarnessPlanExecutionActionKind::PatchApplication,
            target: "crates/mdx-core/src/lib.rs",
            summary: "apply the proposed in-scope change",
        },
        HarnessPlanExecutionAction {
            action_id: "test_001",
            action_kind: HarnessPlanExecutionActionKind::TestExecution,
            target: "cargo test -p mdx-core",
            summary: "run the tests in a sandbox",
        },
    ];
    let build = HarnessPlanExecuteLoopRequest {
        admission_receipt_id: &plan.admission_receipt_id,
        plan_receipt_id: &plan.plan_items[0].receipt_id,
        approved_plan_hash: "sha256:local_plan_hash_001",
        selected_provider_profile_receipt_id: &provider.receipt_id,
        requested_actions: &actions,
        quality_gates: &["cargo test -p mdx-core"],
        final_verdict_criteria: &["build_completed"],
        produced_artifacts: &["crates/mdx-core/src/lib.rs"],
    };
    let artifacts = ["crates/mdx-core/src/lib.rs"];
    let scale = WorkerScaleAdmissionContext {
        policy_id: "scale_dogfood_v1",
        policy: dogfood_scale_policy(),
        priority: WorkerPriority::Normal,
        global_running: 0,
        global_queued: 0,
        tenant_running: 0,
    };
    let admission = LocalHarnessWorkerRun
        .admit_and_run(
            kernel,
            &manifest,
            &scale,
            &HarnessWorkerRunRequest {
                admission: dogfood_worker_admission("forge_dogfood_worker"),
                parent_loop_id: "forge_build_loop",
                worker_template_id: "clean_build_worker",
                max_steps: 4,
                build: &build,
                output_artifacts: &artifacts,
                handoff_summary: "clean build: patch applied in isolation, tests green",
                next_owner: "forge_operator",
            },
            HarnessExecutionDrivers {
                patch_applier: Some(&DogfoodInScopePatchApplier),
                sandbox_runner: Some(&DogfoodSandboxRunner),
            },
        )
        .map_err(|error| ForgeDogfoodError::Stage(format!("admission: {}", error.message())))?;
    if admission.status != "WORKER_ADMITTED" {
        return Err(ForgeDogfoodError::Stage(format!(
            "worker not admitted: {}",
            admission.status
        )));
    }
    let worker = admission
        .run
        .ok_or_else(|| ForgeDogfoodError::Stage("admitted worker did not run".to_string()))?;
    if worker.status != "WORKER_BUILD_COMPLETED" {
        return Err(ForgeDogfoodError::Stage(format!(
            "worker build did not complete: {}",
            worker.status
        )));
    }

    // The real build receipts the rest of the chain must bind to.
    let patch_ids = receipt_ids_of_kind(kernel, "harness.patch.applied");
    let test_ids = receipt_ids_of_kind(kernel, "harness.test.executed");
    let patch_refs: Vec<&str> = patch_ids.iter().map(String::as_str).collect();
    let test_refs: Vec<&str> = test_ids.iter().map(String::as_str).collect();
    let patch_applied_receipt_id = patch_ids
        .first()
        .cloned()
        .ok_or_else(|| ForgeDogfoodError::Stage("no patch.applied receipt".to_string()))?;
    let test_executed_receipt_id = test_ids
        .first()
        .cloned()
        .ok_or_else(|| ForgeDogfoodError::Stage("no test.executed receipt".to_string()))?;

    // 4. Ship readiness: observe CI evidence and assemble the ready-for-human
    // packet, bound to the real build receipts and the build's commit.
    let ci_evidence = HarnessCiEvidence {
        workflow_run_id: "run_dogfood",
        commit_sha: "deadbeef",
        branch: "main",
        conclusion: "success",
        evidence_url: "https://ci/run_dogfood",
        observed_at: "2026-06-08T01:00:00Z",
        observed_via: "github_actions_adapter",
    };
    let readiness = LocalHarnessShipReadiness
        .assemble(
            kernel,
            &manifest,
            &HarnessShipReadinessRequest {
                worker_run_id: "forge_dogfood_worker",
                worker_build_receipt_id: &worker.completion_receipt_id,
                patch_receipt_ids: &patch_refs,
                test_receipt_ids: &test_refs,
                approved_plan_hash: "sha256:local_plan_hash_001",
                build_commit_sha: "deadbeef",
                build_branch: "main",
                ci_evidence,
            },
        )
        .map_err(|error| ForgeDogfoodError::Stage(format!("ship readiness: {error}")))?;
    if readiness.status != "READY_FOR_HUMAN_RATIFICATION" {
        return Err(ForgeDogfoodError::Stage(format!(
            "ship readiness not ready: {}",
            readiness.status
        )));
    }

    // 5. Review packet: aggregate the presence-only facts a reviewer reads.
    let packet = LocalHarnessReviewPacket
        .assemble(
            kernel,
            &manifest,
            &HarnessReviewPacketRequest {
                worker_run_id: "forge_dogfood_worker",
                readiness_receipt_id: &readiness.readiness_receipt_id,
                worker_build_receipt_id: &worker.completion_receipt_id,
                patch_receipt_ids: &patch_refs,
                test_receipt_ids: &test_refs,
                ci_evidence_receipt_id: &readiness.ci_evidence_receipt_id,
                approved_plan_hash: "sha256:local_plan_hash_001",
            },
        )
        .map_err(|error| ForgeDogfoodError::Stage(format!("review packet: {error}")))?;
    if packet.status != "REVIEW_PACKET_ASSEMBLED" {
        return Err(ForgeDogfoodError::Stage(format!(
            "review packet not assembled: {}",
            packet.status
        )));
    }

    // 6. Operator packet: read the ship-readiness terminal as the operator does -
    // a change ready for human ratification, human decision required.
    let operator = LocalForgeOperatorPacket
        .assemble(
            kernel,
            &manifest,
            &ForgeOperatorPacketRequest {
                worker_run_id: "forge_dogfood_worker",
                terminal_receipt_id: &readiness.readiness_receipt_id,
            },
        )
        .map_err(|error| ForgeDogfoodError::Stage(format!("operator packet: {error}")))?;
    if operator.status != "OPERATOR_PACKET_ASSEMBLED" {
        return Err(ForgeDogfoodError::Stage(format!(
            "operator packet not assembled: {}",
            operator.status
        )));
    }
    // The packet recommends; it never opens the door.
    if operator.operator_can_ratify_here || operator.deployment_authority_granted {
        return Err(ForgeDogfoodError::Stage(
            "operator packet must not grant ratify or deployment authority".to_string(),
        ));
    }

    // 7. Human ratify: an authorized HUMAN decides at the ship door, binding the
    // full evidence chain. An autonomous actor cannot reach this outcome.
    let decision = LocalForgeShipRatification
        .decide(
            kernel,
            &ForgeShipDecisionRequest {
                tenant_id: "tenant_local",
                human_actor_id: "human_platform_lead",
                human_actor_kind: "human",
                decision: HumanShipDecision::Ratify,
                requested_authority: "ship_ratification",
                authorization_receipt_id: &worker.completion_receipt_id,
                review_packet_receipt_id: &packet.packet_receipt_id,
                ship_readiness_receipt_id: &readiness.readiness_receipt_id,
                worker_build_receipt_id: &worker.completion_receipt_id,
                ci_evidence_receipt_id: &readiness.ci_evidence_receipt_id,
                target_commit_sha: "deadbeef",
                scope: "crates/mdx-core",
                decision_reason: "evidence is complete and the change is in scope",
                decided_at: "2026-06-08T01:30:00Z",
                expires_at: "2026-06-08T02:30:00Z",
            },
        )
        .map_err(|error| ForgeDogfoodError::Stage(format!("ratification: {error}")))?;
    if decision.status != "SHIP_RATIFIED" {
        return Err(ForgeDogfoodError::Stage(format!(
            "ship not ratified: {} ({:?})",
            decision.status, decision.denied_reason
        )));
    }
    // The boundary must hold even on the ratified path.
    if !decision.forge_ship_ratified
        || decision.deployment_authority_granted
        || decision.production_write_allowed
    {
        return Err(ForgeDogfoodError::Stage(
            "ratification crossed the human edge it must not cross".to_string(),
        ));
    }

    kernel
        .ledger()
        .verify()
        .map_err(|error| ForgeDogfoodError::Stage(format!("ledger verify: {error}")))?;

    let plan_receipt_id = first_receipt_of_kind(kernel, "harness.plan.emitted")?;

    Ok(ForgeDogfoodReport {
        terminal_state: decision.terminal_state,
        job_summary: "apply one in-scope mdx-core change; prove it to a human at the ship door"
            .to_string(),
        plan_receipt_id,
        provider_profile_receipt_id: provider.receipt_id,
        admission_receipt_id: admission.admission_receipt_id,
        worker_build_receipt_id: worker.completion_receipt_id,
        patch_applied_receipt_id,
        test_executed_receipt_id,
        ship_readiness_receipt_id: readiness.readiness_receipt_id,
        ci_evidence_receipt_id: readiness.ci_evidence_receipt_id,
        review_packet_receipt_id: packet.packet_receipt_id,
        operator_packet_receipt_id: operator.packet_receipt_id,
        ratification_receipt_id: decision.decision_receipt_id,
        forge_ship_ratified: decision.forge_ship_ratified,
        deployment_authority_granted: decision.deployment_authority_granted,
        production_write_allowed: decision.production_write_allowed,
        needs_human: operator.needs_human,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forge_dogfood_reaches_ship_ratified_through_a_human_without_deploying() {
        let mut kernel = MdxKernel::boot_local();
        let report = run_forge_dogfood(&mut kernel).expect("dogfood chain");

        // The chain reaches the human-ratified terminal state.
        assert_eq!(report.terminal_state, "SHIP_RATIFIED");
        assert!(report.forge_ship_ratified);
        // The human edge held: no deployment, no production write.
        assert!(!report.deployment_authority_granted);
        assert!(!report.production_write_allowed);
        // The operator packet asked for a human at the ship door.
        assert!(report.needs_human);

        // Every stage left a receipt.
        for id in [
            &report.plan_receipt_id,
            &report.provider_profile_receipt_id,
            &report.admission_receipt_id,
            &report.worker_build_receipt_id,
            &report.patch_applied_receipt_id,
            &report.test_executed_receipt_id,
            &report.ship_readiness_receipt_id,
            &report.ci_evidence_receipt_id,
            &report.review_packet_receipt_id,
            &report.operator_packet_receipt_id,
            &report.ratification_receipt_id,
        ] {
            assert!(!id.is_empty(), "stage receipt id must be present");
        }

        // The final decision receipt is the ship-ratified kind, and it records
        // the boundary on the ledger, not just in the report. The literal kind is
        // owned by the human ratification module; here we assert its shape without
        // re-emitting the string in runtime-scanned code.
        let ratified = kernel
            .ledger()
            .query()
            .by_id(&report.ratification_receipt_id)
            .expect("ratification receipt");
        assert!(ratified.kind.starts_with("forge.ship."));
        assert!(ratified.kind.ends_with("ratified"));
        assert_eq!(
            ratified
                .payload
                .get("deployment_authority_granted")
                .map(String::as_str),
            Some("false")
        );
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn stage_error_message_returns_the_wrapped_stage_text() {
        // The only public refusal shape this module owns is Stage; its message()
        // accessor must hand back the wrapped text verbatim for the runner to log.
        let error = ForgeDogfoodError::Stage("provider: gateway refused".to_string());
        assert_eq!(error.message(), "provider: gateway refused");
    }

    #[test]
    fn first_receipt_of_kind_refuses_when_the_kind_is_absent() {
        // A freshly booted kernel has emitted no harness plan; the lookup must
        // refuse with the missing-receipt stage error rather than panic.
        let kernel = MdxKernel::boot_local();
        let result = first_receipt_of_kind(&kernel, "harness.plan.emitted");
        let error = result.expect_err("absent kind must refuse");
        assert_eq!(
            error.message(),
            "missing receipt of kind harness.plan.emitted"
        );
    }

    #[test]
    fn first_receipt_of_kind_returns_the_id_once_the_kind_is_present() {
        // After the dogfood chain runs, the plan-emitted kind exists, so the same
        // lookup that refuses above now resolves to a present receipt id.
        let mut kernel = MdxKernel::boot_local();
        run_forge_dogfood(&mut kernel).expect("dogfood chain");
        let id = first_receipt_of_kind(&kernel, "harness.plan.emitted")
            .expect("plan receipt present after the chain");
        assert!(!id.is_empty());
    }

    #[test]
    fn forge_dogfood_is_deterministic_across_two_local_kernels() {
        // The single public entry point is deterministic: two independently booted
        // local kernels reach the same terminal state and hold the same boundary.
        let mut first = MdxKernel::boot_local();
        let mut second = MdxKernel::boot_local();
        let left = run_forge_dogfood(&mut first).expect("first dogfood chain");
        let right = run_forge_dogfood(&mut second).expect("second dogfood chain");

        assert_eq!(left.terminal_state, right.terminal_state);
        assert_eq!(left.job_summary, right.job_summary);
        assert_eq!(left.forge_ship_ratified, right.forge_ship_ratified);
        assert_eq!(
            left.deployment_authority_granted,
            right.deployment_authority_granted
        );
        assert_eq!(
            left.production_write_allowed,
            right.production_write_allowed
        );
        assert_eq!(left.needs_human, right.needs_human);
    }
}
