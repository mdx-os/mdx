// The integration phase: where a fleet's parallel work becomes ONE
// branch. Per-stream green does not compose for free - this phase is
// first-class, with its own agent, its own checks, and its own receipts.
//
// Three movements, after the streams land:
//   1. MERGE - the green streams' branches merge onto fleet/<fleet_id>
//      in dependency order. Scopes were disjoint by law, so these merges
//      are structurally clean; a conflict here means the law was broken
//      and the fleet stops honestly.
//   2. WIRE - the integration agent (an ordinary forge run opened AT the
//      fleet branch) edits ONLY the integration-owned paths - the shared
//      registration files every stream declared into but never touched -
//      and proves the whole with the plan's full-suite checks.
//   3. REVIEW - a fresh-context reviewer (the reviewer role slot, a
//      different set of eyes by design) reads the integrated diff cold
//      and returns findings; the verdict is recorded, the human decides.
//
// The fleet still never ships: the integrated branch waits for the same
// recorded human ship decision as any forge run.
use crate::fleet_run_conductor::FleetRunPlan;
use crate::forge_loop_runner::ForgeRunRequest;
use crate::forge_turn_client::TurnClient;
use mdx_core::{GovernedWriteIdentity, MdxKernel, fleet_integration_order};
use std::path::Path;
use std::sync::{Arc, RwLock};

const INTEGRATION_REQUIRED_SEMANTIC_OPERATIONS: &[&str] = &[
    "file_outline",
    "dependency_map",
    "references",
    "diagnostics",
];

pub(crate) struct IntegrationOutcome {
    pub fleet_branch: Option<String>,
    pub integration_run_id: String,
    pub merged_streams: usize,
    pub wired: bool,
    pub review_verdict: String,
    pub detail: String,
}

pub(crate) struct IntegrationRepairOutcome {
    pub run_id: String,
    pub status: &'static str,
    pub review_verdict: String,
}

struct FullSuiteProof {
    passed: bool,
    detail: String,
}

fn integration_run_started_evidence_fields(plan: &FleetRunPlan) -> Vec<(String, String)> {
    let repo_profile = crate::forge_repo_profile::profile_repo(&plan.repo_root);
    let semantic_session = crate::forge_semantic_query_route::semantic_session_evidence(
        &plan.repo_root,
        &repo_profile,
    );
    let selected_checks = if plan.full_suite_checks.is_empty() {
        repo_profile.suggested_checks.join("\n")
    } else {
        plan.full_suite_checks.join("\n")
    };
    vec![
        ("fleet_id".to_string(), plan.fleet_id.clone()),
        ("repo_id".to_string(), plan.repo_id.clone()),
        (
            "execution_backend_kind".to_string(),
            plan.execution_backend.clone(),
        ),
        (
            "execution_target_kind".to_string(),
            if plan.hosted_execution() {
                "mdx_cloud".to_string()
            } else {
                "paired_host".to_string()
            },
        ),
        (
            "execution_target_id".to_string(),
            if plan.hosted_execution() {
                plan.cloud_environment_id.clone()
            } else {
                "local_host".to_string()
            },
        ),
        (
            "cloud_environment_id".to_string(),
            plan.cloud_environment_id.clone(),
        ),
        (
            "fleet_integration_strategy_id".to_string(),
            "integration_contract_wiring".to_string(),
        ),
        (
            "fleet_integration_strategy_summary".to_string(),
            "preserve stream interfaces, apply declared registrations, and prove the merged branch as one system".to_string(),
        ),
        (
            "fleet_integration_required_semantic_operations".to_string(),
            INTEGRATION_REQUIRED_SEMANTIC_OPERATIONS.join("\n"),
        ),
        (
            "fleet_integration_proof_bias".to_string(),
            "full_suite_integration_and_contract_alignment".to_string(),
        ),
        (
            "repo_root".to_string(),
            plan.repo_root.display().to_string(),
        ),
        (
            "repo_primary_language".to_string(),
            repo_profile.primary_language.to_string(),
        ),
        (
            "language_pack_id".to_string(),
            repo_profile.language_pack_id.to_string(),
        ),
        (
            "repo_profile_detected_language_packs".to_string(),
            repo_profile.detected_language_packs.join("\n"),
        ),
        (
            "repo_profile_suggested_checks".to_string(),
            repo_profile.suggested_checks.join("\n"),
        ),
        (
            "repo_profile_artifact_patterns".to_string(),
            repo_profile.artifact_patterns.join("\n"),
        ),
        (
            "repo_profile_semantic_intelligence".to_string(),
            repo_profile.semantic_intelligence.join("\n"),
        ),
        (
            "repo_profile_semantic_tool_readiness".to_string(),
            repo_profile.semantic_tool_readiness.join("\n"),
        ),
        (
            "repo_profile_semantic_session_id".to_string(),
            semantic_session.session_id,
        ),
        (
            "repo_profile_semantic_fallback_index_status".to_string(),
            semantic_session.fallback_index_status.to_string(),
        ),
        (
            "repo_profile_semantic_source_file_count".to_string(),
            semantic_session.source_file_count.to_string(),
        ),
        (
            "repo_profile_semantic_indexed_file_count".to_string(),
            semantic_session.indexed_file_count.to_string(),
        ),
        (
            "repo_profile_semantic_indexed_symbol_count".to_string(),
            semantic_session.indexed_symbol_count.to_string(),
        ),
        (
            "repo_profile_semantic_related_test_anchor_count".to_string(),
            semantic_session.related_test_anchor_count.to_string(),
        ),
        (
            "repo_profile_semantic_session_grants_execution_authority".to_string(),
            semantic_session.grants_execution_authority.to_string(),
        ),
        (
            "repo_profile_toolchain_readiness".to_string(),
            repo_profile.toolchain_readiness.join("\n"),
        ),
        (
            "repo_profile_proof_plan_status".to_string(),
            repo_profile.proof_plan_status.to_string(),
        ),
        (
            "repo_profile_proof_plan_summary".to_string(),
            repo_profile.proof_plan_summary.to_string(),
        ),
        ("selected_checks".to_string(), selected_checks),
        (
            "selected_checks_source".to_string(),
            if plan.full_suite_checks.is_empty() {
                "repo_profile_suggested_checks"
            } else {
                "fleet_integration_full_suite_checks"
            }
            .to_string(),
        ),
        (
            "principal_orientation_gate_required".to_string(),
            "true".to_string(),
        ),
        (
            "principal_orientation_gate_tool".to_string(),
            "semantic_query".to_string(),
        ),
        (
            "principal_orientation_gate_grants_authority".to_string(),
            "false".to_string(),
        ),
        (
            "execution_geometry_requested_workers".to_string(),
            plan.requested_width.max(1).to_string(),
        ),
        (
            "execution_geometry_effective_workers".to_string(),
            "1".to_string(),
        ),
        (
            "execution_geometry_lane".to_string(),
            "fleet_integration".to_string(),
        ),
        (
            "execution_geometry_route".to_string(),
            "/forge/fleet-runs.json".to_string(),
        ),
        (
            "execution_geometry_reason".to_string(),
            "integration_from_governed_fleet_plan".to_string(),
        ),
        (
            "execution_geometry_fleet_required".to_string(),
            "false".to_string(),
        ),
        (
            "execution_geometry_grants_execution_authority".to_string(),
            "false".to_string(),
        ),
    ]
}

/// Run the integration phase for the fleet's GREEN streams. Returns the
/// outcome for the conductor to witness; holds no kernel lock except
/// through the forge run it delegates to.
pub(crate) fn integrate_fleet(
    plan: &FleetRunPlan,
    green_branches: &[(String, String)], // (stream_id, branch)
    kernel: &Arc<RwLock<MdxKernel>>,
) -> IntegrationOutcome {
    let fleet_id = &plan.fleet_id;
    let tenant_id = &plan.tenant_id;
    let actor_id = &plan.actor_id;
    let repo_root = plan.repo_root.as_path();
    let streams = &plan.streams;
    let integration_owned_paths = &plan.integration_owned_paths;
    let full_suite_checks = &plan.full_suite_checks;
    let mut outcome = IntegrationOutcome {
        fleet_branch: None,
        integration_run_id: String::new(),
        merged_streams: 0,
        wired: false,
        review_verdict: String::new(),
        detail: String::new(),
    };
    if green_branches.is_empty() {
        outcome.detail = "no green streams to integrate".to_string();
        return outcome;
    }

    // 1. MERGE, in dependency order, inside a throwaway worktree checked
    // out at a fleet branch. The live tree is never touched. A restarted
    // local kernel can see branch refs left behind by a previous attempt,
    // so pick a retry branch instead of failing the whole integration.
    let fleet_branch = next_integration_branch(repo_root, fleet_id);
    let worktree = std::env::temp_dir().join(format!(
        "mdx_fleet_integration_{fleet_id}_{}",
        std::process::id()
    ));
    let cleanup = |worktree: &Path| {
        let _ = run_git(
            repo_root,
            &["worktree", "remove", "--force", &worktree.to_string_lossy()],
        );
    };
    cleanup(&worktree);
    if let Err(error) = run_git_checked(
        repo_root,
        &[
            "worktree",
            "add",
            "-b",
            &fleet_branch,
            &worktree.to_string_lossy(),
            "HEAD",
        ],
    ) {
        outcome.detail = format!(
            "could not open the integration worktree - {}",
            shell_detail(&tail_chars(&error, 260))
        );
        return outcome;
    }
    for stream_id in fleet_integration_order(streams) {
        let Some((_, branch)) = green_branches.iter().find(|(id, _)| id == &stream_id) else {
            continue; // not green - its work stays on its own branch
        };
        let merged = run_git_in(
            &worktree,
            &[
                "-c",
                "user.email=fleet@mdx.local",
                "-c",
                "user.name=mdx-fleet",
                "merge",
                "--no-ff",
                "--no-edit",
                branch,
            ],
        );
        if merged.is_none() {
            // Disjoint scopes make this structurally impossible; reaching
            // it means the law was broken somewhere. Stop honestly.
            let _ = run_git_in(&worktree, &["merge", "--abort"]);
            cleanup(&worktree);
            let _ = run_git(repo_root, &["branch", "-D", &fleet_branch]);
            outcome.detail = format!(
                "merge of {stream_id} did not apply cleanly - the scopes were not truly disjoint"
            );
            return outcome;
        }
        outcome.merged_streams += 1;
    }
    let full_suite_proof = if hosted_full_suite_required(plan) {
        // The Render conductor intentionally does not carry every repository
        // toolchain. A hosted fleet must prove its integrated branch in the
        // same verified AgentCore environment as its lanes, even when no
        // shared wiring edit is required.
        FullSuiteProof {
            passed: false,
            detail: "full_suite=deferred_to_hosted_backend".to_string(),
        }
    } else {
        run_full_suite_checks(&worktree, full_suite_checks)
    };
    cleanup(&worktree);
    outcome.fleet_branch = Some(fleet_branch.clone());

    // 2. WIRE - the integration agent works AT the fleet branch (the
    // revise-run shape), confined to the integration-owned paths, proving
    // the whole with the full-suite checks. Skipped when the plan owns no
    // shared paths and names no suite - nothing to wire, nothing to prove.
    // The integration RUN exists whenever a branch landed - it is the run
    // the revise, diff, and ship rails hang off, so the merge itself
    // witnesses the branch on it even before (or without) any wiring
    // commit. Round two of the live showcase found the gap: an agent that
    // wires nothing leaves the fleet branch with no run to revise.
    let integration_run_id = {
        let Ok(mut kernel) = kernel.write() else {
            outcome.detail = "kernel lock poisoned before wiring".to_string();
            return outcome;
        };
        let run_id = kernel.mint_id("forge_run");
        let evidence_fields = integration_run_started_evidence_fields(plan);
        let evidence_refs: Vec<(&str, &str)> = evidence_fields
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let _ = kernel.record_forge_run_event_with_evidence_fields(
            mdx_core::ForgeRunEvent {
                tenant_id,
                actor_id,
                run_id: &run_id,
                event: "run_started",
                work_item_id: &format!("work_item_{}_integration", fleet_id),
                detail: &format!(
                    "fleet={fleet_id} integration repo_root={}",
                    repo_root.display()
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo(actor_id),
            &evidence_refs,
        );
        let tip = run_git(repo_root, &["rev-parse", &fleet_branch])
            .unwrap_or_default()
            .trim()
            .to_string();
        let _ = kernel.record_forge_run_event(mdx_core::ForgeRunEvent {
            tenant_id,
            actor_id,
            run_id: &run_id,
            event: "evidence_appended",
            work_item_id: "",
            detail: &format!("branch={fleet_branch} sha={}", &tip[..tip.len().min(12)]),
            turn: 0,
            input_tokens: 0,
            output_tokens: 0,
        });
        run_id
    };
    outcome.integration_run_id = integration_run_id.clone();

    if integration_owned_paths.is_empty()
        && !full_suite_checks.is_empty()
        && !hosted_full_suite_required(plan)
    {
        if full_suite_proof.passed {
            outcome.wired = true;
            outcome.detail = format!("{} wiring=not_needed", full_suite_proof.detail);
        } else {
            outcome.wired = false;
            outcome.detail = format!(
                "{} wiring=unavailable_no_integration_owned_paths",
                full_suite_proof.detail
            );
        }
    } else if !integration_owned_paths.is_empty() || !full_suite_checks.is_empty() {
        let declared: Vec<String> = streams
            .iter()
            .map(|stream| {
                format!(
                    "- {} landed in: {} (contract: {})",
                    stream.stream_id,
                    stream.write_scope.join(", "),
                    if stream.interface_contract.is_empty() {
                        "none"
                    } else {
                        &stream.interface_contract
                    }
                )
            })
            .collect();
        let intent = format!(
            "Fleet {fleet_id} integration: wire the streams and prove the whole\n\nYou are the INTEGRATION agent for a fleet build. The parallel streams have landed their work in this workspace already - do not redo it.\n\nIntegration semantic strategy: integration_contract_wiring - preserve stream interfaces, apply declared registrations, and prove the merged branch as one system.\nRequired semantic operations: {}.\nProof bias: full_suite_integration_and_contract_alignment.\n\nWhat the streams built (read the TOP COMMENTS of every file listed - the streams left explicit registration notes there, and you MUST apply every declared registration before finishing; finishing without applying them is a failed integration):\n{}\n\nYou may edit ONLY these paths:\n{}\n\nVerification protocol: run exactly these allowed commands and make them pass:\n{}\n\nOnly if you have read every stream file above, applied every declared registration, and the checks pass, call finish with outcome done.",
            INTEGRATION_REQUIRED_SEMANTIC_OPERATIONS.join(", "),
            declared.join("\n"),
            if integration_owned_paths.is_empty() {
                "none - registration is not needed; just prove the whole".to_string()
            } else {
                integration_owned_paths.join("\n")
            },
            if full_suite_checks.is_empty() {
                "none allowed - verify by reading, then finish".to_string()
            } else {
                full_suite_checks.join("\n")
            },
        );
        let run_id = integration_run_id.clone();
        let request = ForgeRunRequest {
            run_id,
            tenant_id: tenant_id.to_string(),
            actor_id: actor_id.to_string(),
            work_item_id: String::new(),
            intent,
            allowed_commands: full_suite_checks.to_vec(),
            max_turns: 30,
            revise_branch: Some(fleet_branch.clone()),
            resume: false,
            write_scope: integration_owned_paths.to_vec(),
            check_target_dir: if plan.hosted_execution() {
                Some(format!("hosted://{}", plan.cloud_environment_id))
            } else {
                Some(
                    repo_root
                        .join(".mdx-local/fleet-targets/integration")
                        .to_string_lossy()
                        .to_string(),
                )
            },
            // Integration is the hardest, most failure-expensive step in a
            // fleet. Cast it through the role resolver so process-session
            // provider keys can power the integrator, while env-only
            // deployments keep the same fail-closed named-slot behavior.
            builder_slot: {
                let guard = kernel.read().ok();
                guard
                    .as_deref()
                    .map(|kernel| {
                        crate::forge_model_provider_routing::preferred_slot_for_role(
                            kernel,
                            tenant_id,
                            "integrator",
                        )
                    })
                    .unwrap_or_else(TurnClient::strongest_configured_builder_slot)
            },
            work_complexity_tier: "large".to_string(),
            semantic_policy_required_operations: INTEGRATION_REQUIRED_SEMANTIC_OPERATIONS
                .iter()
                .map(|operation| (*operation).to_string())
                .collect(),
            semantic_policy_source: "fleet_integration".to_string(),
            execution_geometry_requested_workers: plan.requested_width.max(1),
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "fleet_integration".to_string(),
            execution_geometry_route: "/forge/fleet-runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        };
        let wire_outcome = crate::fleet_executor::run_blocking(
            request,
            repo_root.to_path_buf(),
            kernel,
            format!("{}:integration", plan.fleet_id),
        );
        outcome.wired =
            integration_run_completed(&wire_outcome, integration_owned_paths, full_suite_checks);
        if !outcome.wired {
            outcome.detail = format!(
                "{} wiring run ended {}",
                full_suite_proof.detail, wire_outcome.status
            );
        } else {
            outcome.detail = integration_detail_after_wiring(&full_suite_proof, &wire_outcome);
        }
    } else {
        outcome.wired = true;
        outcome.detail = "full_suite=not_declared wiring=not_needed".to_string();
    }

    if !outcome.wired {
        outcome.review_verdict = "review skipped: integration wiring did not finish".to_string();
        return outcome;
    }

    if integration_owned_paths.is_empty()
        && full_suite_checks.is_empty()
        && let Some(verdict) = clean_structural_integration_review(plan, outcome.merged_streams)
    {
        outcome.review_verdict = verdict;
        return outcome;
    }

    // 3. REVIEW - fresh eyes on the integrated diff. For an ordinary fleet,
    // one fresh-context reviewer. For a HIGH-STAKES fleet (any sensitive
    // stream, or MDX_FLEET_REVIEW_PANEL=1), a PANEL of diverse elite models,
    // each reading the SAME diff from a different stance. Different model
    // families catch different things - one is strict and process-driven, one
    // is holistic and fast - so two senior reviewers from different schools
    // find more than either alone. The panel's clean context is the point.
    let diff = run_git(
        repo_root,
        &[
            "diff",
            &format!("HEAD...{fleet_branch}"),
            "--stat",
            "--patch",
        ],
    )
    .unwrap_or_default();
    let capped: String = diff.chars().take(60_000).collect();
    outcome.review_verdict = review_integrated_diff(&capped, plan, kernel);
    outcome
}

fn hosted_full_suite_required(plan: &FleetRunPlan) -> bool {
    plan.hosted_execution() && !plan.full_suite_checks.is_empty()
}

fn integration_run_completed(
    outcome: &crate::forge_loop_runner::ForgeRunOutcome,
    integration_owned_paths: &[String],
    full_suite_checks: &[String],
) -> bool {
    outcome.status == "RUN_FINISHED_DONE"
        || (outcome.status == "RUN_FINISHED_NO_CHANGE"
            && integration_owned_paths.is_empty()
            && !full_suite_checks.is_empty()
            && outcome.check_runs > 0
            && outcome.last_check_passed)
}

fn clean_structural_integration_review(
    plan: &FleetRunPlan,
    merged_streams: usize,
) -> Option<String> {
    if merged_streams == 0
        || mdx_core::validate_fleet_streams(&plan.streams, &plan.integration_owned_paths).is_err()
    {
        return None;
    }
    Some(format!(
        "VERDICT: ready\n1. Structural integration complete: merged {merged_streams} disjoint stream(s) for fleet {} with no integration-owned paths and no full-suite checks declared, so no wiring run is required.",
        plan.fleet_id
    ))
}

pub(crate) fn repair_integrated_fleet(
    plan: &FleetRunPlan,
    fleet_branch: &str,
    review_verdict: &str,
    attempt: usize,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<IntegrationRepairOutcome> {
    if fleet_branch.trim().is_empty() || !review_verdict_needs_work(review_verdict) {
        return None;
    }
    let write_scope = integration_repair_write_scope(plan);
    let allowed_commands = integration_repair_checks(plan);
    if write_scope.is_empty() || allowed_commands.is_empty() {
        return None;
    }
    let repair_run_id = {
        let Ok(mut kernel) = kernel.write() else {
            return None;
        };
        let run_id = kernel.mint_id("forge_run");
        let mut evidence_fields = integration_run_started_evidence_fields(plan);
        evidence_fields.push((
            "fleet_integration_repair_attempt".to_string(),
            attempt.to_string(),
        ));
        evidence_fields.push((
            "fleet_integration_repair_source".to_string(),
            "fresh_context_review".to_string(),
        ));
        let evidence_refs: Vec<(&str, &str)> = evidence_fields
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let _ = kernel.record_forge_run_event_with_evidence_fields(
            mdx_core::ForgeRunEvent {
                tenant_id: &plan.tenant_id,
                actor_id: &plan.actor_id,
                run_id: &run_id,
                event: "run_started",
                work_item_id: &format!("work_item_{}_integration_repair_{attempt}", plan.fleet_id),
                detail: &format!(
                    "fleet={} integration_repair attempt={} repo_root={} revising={}",
                    plan.fleet_id,
                    attempt,
                    plan.repo_root.display(),
                    fleet_branch
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo(&plan.actor_id),
            &evidence_refs,
        );
        run_id
    };
    let intent = format!(
        "Fleet {} integration repair attempt {attempt}: fix the integrated branch after fresh-context review.\n\nReview findings to satisfy before finishing:\n{}\n\nYou are revising branch {}. You may edit only the bounded fleet repair scope. Preserve each stream's public interface, fix the concrete issues, run the selected proof, and finish only when the proof passes and the review findings are addressed.",
        plan.fleet_id,
        review_verdict.trim(),
        fleet_branch,
    );
    let builder_slot = {
        let guard = kernel.read().ok();
        guard
            .as_deref()
            .map(|kernel| {
                crate::forge_model_provider_routing::preferred_slot_for_role(
                    kernel,
                    &plan.tenant_id,
                    "integrator",
                )
            })
            .unwrap_or_else(TurnClient::strongest_configured_builder_slot)
    };
    let request = ForgeRunRequest {
        run_id: repair_run_id.clone(),
        tenant_id: plan.tenant_id.clone(),
        actor_id: plan.actor_id.clone(),
        work_item_id: format!("work_item_{}_integration_repair_{attempt}", plan.fleet_id),
        intent,
        allowed_commands,
        max_turns: 45,
        revise_branch: Some(fleet_branch.to_string()),
        resume: false,
        write_scope,
        check_target_dir: plan
            .hosted_execution()
            .then(|| format!("hosted://{}", plan.cloud_environment_id)),
        builder_slot,
        work_complexity_tier: "large".to_string(),
        semantic_policy_required_operations: INTEGRATION_REQUIRED_SEMANTIC_OPERATIONS
            .iter()
            .map(|operation| (*operation).to_string())
            .collect(),
        semantic_policy_source: "fleet_integration_repair".to_string(),
        execution_geometry_requested_workers: plan.requested_width.max(1),
        execution_geometry_effective_workers: 1,
        execution_geometry_lane: "fleet_integration_repair".to_string(),
        execution_geometry_route: "/forge/fleet-runs.json".to_string(),
        mission_id: String::new(),
        mission_milestone_id: String::new(),
        max_cost_cents: 0,
        max_runtime_ms: 0,
        envelope_id: String::new(),
        plan_only: false,
        reasoning_effort: String::new(),
    };
    let outcome = crate::fleet_executor::run_blocking(
        request,
        plan.repo_root.clone(),
        kernel,
        format!("{}:integration-repair-{attempt}", plan.fleet_id),
    );
    let review_verdict = if outcome.status == "RUN_FINISHED_DONE" {
        let diff = run_git(
            &plan.repo_root,
            &[
                "diff",
                &format!("HEAD...{fleet_branch}"),
                "--stat",
                "--patch",
            ],
        )
        .unwrap_or_default();
        let capped: String = diff.chars().take(60_000).collect();
        review_integrated_diff(&capped, plan, kernel)
    } else {
        format!(
            "VERDICT: needs work\n1. integration repair attempt {attempt} ended {}",
            outcome.status
        )
    };
    Some(IntegrationRepairOutcome {
        run_id: repair_run_id,
        status: outcome.status,
        review_verdict,
    })
}

pub(crate) fn review_verdict_needs_work(review_verdict: &str) -> bool {
    let lower = review_verdict.to_ascii_lowercase();
    lower.contains("verdict: needs work") || lower.contains("panel verdict: needs work")
}

fn integration_repair_write_scope(plan: &FleetRunPlan) -> Vec<String> {
    let mut scope = Vec::<String>::new();
    for path in plan
        .streams
        .iter()
        .flat_map(|stream| stream.write_scope.iter())
        .chain(plan.integration_owned_paths.iter())
    {
        let trimmed = path.trim();
        if !trimmed.is_empty() && !scope.iter().any(|existing| existing == trimmed) {
            scope.push(trimmed.to_string());
        }
    }
    scope
}

fn integration_repair_checks(plan: &FleetRunPlan) -> Vec<String> {
    if !plan.full_suite_checks.is_empty() {
        return dedup_commands(plan.full_suite_checks.iter());
    }
    let profile = crate::forge_repo_profile::profile_repo(&plan.repo_root);
    if !profile.suggested_checks.is_empty() {
        return dedup_commands(profile.suggested_checks.iter());
    }
    dedup_commands(plan.streams.iter().flat_map(|stream| stream.checks.iter()))
}

fn dedup_commands<'a>(commands: impl Iterator<Item = &'a String>) -> Vec<String> {
    let mut result = Vec::<String>::new();
    for command in commands {
        let trimmed = command.trim();
        if !trimmed.is_empty() && !result.iter().any(|existing| existing == trimmed) {
            result.push(trimmed.to_string());
        }
    }
    result
}

/// One reviewer in the panel: its model, its client, and the lens it reads
/// with. Distinct lenses on the same diff are what make a panel worth more
/// than one reviewer.
pub(crate) struct PanelReviewer {
    pub(crate) model: String,
    pub(crate) client: TurnClient,
    pub(crate) stance: &'static str,
}

pub(crate) const REVIEW_BASE: &str = "You are a fresh-context code reviewer reading an integrated change produced by parallel build agents. You have NO prior context - that is deliberate; read the diff cold. Answer with one line exactly 'VERDICT: ready' or 'VERDICT: needs work', then at most four numbered findings, real problems only (bugs, contract mismatches between the pieces, missing wiring, security or correctness gaps - no style nits).";

pub(crate) const PANEL_STANCES: [&str; 3] = [
    "Your lens: HOLISTIC. Does the whole change actually work end to end - are the parallel pieces wired together, do their contracts line up, would it run?",
    "Your lens: STRICT and process-driven. Correctness, edge cases, error handling, anything under-tested or sloppy that would fail a demanding review. Be exacting.",
    "Your lens: ADVERSARIAL. Assume something is wrong; find the one thing that breaks this in production - the missed case, the wrong assumption, the silent failure.",
];

/// A high-stakes fleet earns a panel: any stream the planner marked
/// "sensitive", or an explicit opt-in. Ordinary fleets get a single
/// reviewer - a panel on every fleet would multiply cost for no gain on
/// low-stakes work.
fn should_convene_panel(plan: &FleetRunPlan) -> bool {
    if std::env::var("MDX_FLEET_REVIEW_PANEL").ok().as_deref() == Some("1") {
        return true;
    }
    plan.streams
        .iter()
        .any(|s| s.data_sensitivity.trim().eq_ignore_ascii_case("sensitive"))
}

/// The panel: distinct elite models in a family-diverse preference (the
/// configured reviewer role, then a different-family coder, then the top
/// model), deduped by resolved model so it is genuinely diverse - not one
/// model three times - and capped at three.
pub(crate) fn panel_reviewers(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
) -> Vec<PanelReviewer> {
    let mut seen: Vec<String> = Vec::new();
    let mut panel: Vec<PanelReviewer> = Vec::new();
    let routed_reviewer = kernel
        .read()
        .ok()
        .and_then(|guard| TurnClient::for_role(&guard, tenant_id, "reviewer"));
    let slot_candidates = kernel
        .read()
        .ok()
        .map(|guard| {
            ["GPT", "OPUS", "GROK", "XAI", "SONNET", "CODEXMINI"]
                .iter()
                .filter_map(|slot| {
                    TurnClient::named_slot_for_role(&guard, tenant_id, "reviewer", slot)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let candidates = std::iter::once(routed_reviewer)
        .chain(slot_candidates.into_iter().map(Some))
        .chain([
            TurnClient::builder_from_env("CODEXMINI"),
            TurnClient::builder_from_env("OPUS"),
            TurnClient::builder_from_env("SONNET"),
        ]);
    for client in candidates.into_iter().flatten() {
        let model = client.model_id().to_string();
        if model.is_empty() || seen.contains(&model) {
            continue;
        }
        let stance = PANEL_STANCES[panel.len() % PANEL_STANCES.len()];
        seen.push(model.clone());
        panel.push(PanelReviewer {
            model,
            client,
            stance,
        });
        if panel.len() >= 3 {
            break;
        }
    }
    panel
}

/// Read one member's first-line verdict: ready / needs work / unclear.
pub(crate) fn parse_member_verdict(text: &str) -> &'static str {
    let lower = text.to_lowercase();
    if lower.contains("verdict: needs work") {
        "needs work"
    } else if lower.contains("verdict: ready") {
        "ready"
    } else {
        "unclear"
    }
}

/// Run the review - a panel for a high-stakes fleet, a single reviewer
/// otherwise - and fold the result into one verdict line plus findings,
/// capped for the receipt. The panel is conservative: it is "ready" only if
/// no member says "needs work"; any one reviewer flagging a problem holds
/// the merge for a human, which is the whole point of diverse eyes.
fn review_integrated_diff(
    diff: &str,
    plan: &FleetRunPlan,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> String {
    let panel = panel_reviewers(kernel, &plan.tenant_id);
    let user =
        serde_json::json!({"role":"user","content":format!("The integrated diff:\n\n{diff}")});
    if !should_convene_panel(plan) || panel.len() < 2 {
        let Some(reviewer) = panel.into_iter().next() else {
            return "review skipped: no reviewer model configured".to_string();
        };
        let messages = vec![
            serde_json::json!({"role":"system","content":REVIEW_BASE}),
            user,
        ];
        return match reviewer.client.complete(&messages) {
            Ok(turn) => turn.text.chars().take(900).collect(),
            Err(error) => format!("review unavailable: {error}"),
        };
    }
    let mut labels: Vec<String> = Vec::new();
    let mut findings: Vec<String> = Vec::new();
    let mut any_needs_work = false;
    let mut any_ready = false;
    for member in &panel {
        let messages = vec![
            serde_json::json!({"role":"system","content":format!("{REVIEW_BASE} {}", member.stance)}),
            user.clone(),
        ];
        match member.client.complete(&messages) {
            Ok(turn) => {
                let verdict = parse_member_verdict(&turn.text);
                any_needs_work |= verdict == "needs work";
                any_ready |= verdict == "ready";
                labels.push(format!("{}={verdict}", member.model));
                if verdict == "needs work" {
                    let trimmed: String = turn.text.trim().chars().take(280).collect();
                    findings.push(format!("[{}] {trimmed}", member.model));
                }
            }
            Err(error) => labels.push(format!("{}=unavailable ({error})", member.model)),
        }
    }
    let combined = if any_needs_work {
        "needs work"
    } else if any_ready {
        "ready"
    } else {
        "inconclusive"
    };
    let header = format!(
        "PANEL VERDICT: {combined} [{}]",
        labels.join("; ").chars().take(280).collect::<String>()
    );
    if findings.is_empty() {
        header.chars().take(900).collect()
    } else {
        format!("{header}\n{}", findings.join("\n"))
            .chars()
            .take(900)
            .collect()
    }
}

fn next_integration_branch(repo_root: &Path, fleet_id: &str) -> String {
    let base = format!("fleet/{fleet_id}");
    if git_ref_exists(repo_root, &base) {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        format!("{base}-{millis}-{}", std::process::id())
    } else {
        base
    }
}

fn git_ref_exists(repo_root: &Path, git_ref: &str) -> bool {
    std::process::Command::new("git")
        .current_dir(repo_root)
        .args(["rev-parse", "--verify", "--quiet", git_ref])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn run_git(repo_root: &Path, args: &[&str]) -> Option<String> {
    run_git_checked(repo_root, args).ok()
}

fn run_git_checked(repo_root: &Path, args: &[&str]) -> Result<String, String> {
    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        let mut text = String::from_utf8_lossy(&output.stderr).to_string();
        if text.trim().is_empty() {
            text = String::from_utf8_lossy(&output.stdout).to_string();
        }
        return Err(text);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn run_git_in(worktree: &Path, args: &[&str]) -> Option<String> {
    run_git(worktree, args)
}

fn run_full_suite_checks(worktree: &Path, checks: &[String]) -> FullSuiteProof {
    if checks.is_empty() {
        return FullSuiteProof {
            passed: true,
            detail: "full_suite=not_declared".to_string(),
        };
    }

    for check in checks {
        let output = match std::process::Command::new("sh")
            .arg("-lc")
            .arg(check)
            .current_dir(worktree)
            .output()
        {
            Ok(output) => output,
            Err(error) => {
                return FullSuiteProof {
                    passed: false,
                    detail: format!(
                        "full_suite=failed command={} error={}",
                        shell_detail(check),
                        shell_detail(&error.to_string())
                    ),
                };
            }
        };
        if !output.status.success() {
            let mut text = String::from_utf8_lossy(&output.stderr).to_string();
            if text.trim().is_empty() {
                text = String::from_utf8_lossy(&output.stdout).to_string();
            }
            return FullSuiteProof {
                passed: false,
                detail: format!(
                    "full_suite=failed command={} exit={} tail={}",
                    shell_detail(check),
                    output.status.code().unwrap_or(-1),
                    shell_detail(&tail_chars(&text, 260))
                ),
            };
        }
    }

    FullSuiteProof {
        passed: true,
        detail: format!("full_suite=passed commands={}", checks.len()),
    }
}

fn integration_detail_after_wiring(
    pre_wiring_proof: &FullSuiteProof,
    wire_outcome: &crate::forge_loop_runner::ForgeRunOutcome,
) -> String {
    if wire_outcome.last_check_passed {
        if pre_wiring_proof.passed {
            format!("{} wiring=done", pre_wiring_proof.detail)
        } else {
            format!(
                "pre_wiring_{} post_wiring_full_suite=passed wiring=done",
                pre_wiring_proof.detail
            )
        }
    } else {
        format!(
            "{} post_wiring_full_suite=not_observed wiring=done",
            pre_wiring_proof.detail
        )
    }
}

fn tail_chars(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.trim().chars().collect();
    let start = chars.len().saturating_sub(max_chars);
    chars[start..].iter().collect()
}

fn shell_detail(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_with(sensitivities: &[&str]) -> FleetRunPlan {
        FleetRunPlan {
            fleet_id: "f".into(),
            tenant_id: "t".into(),
            actor_id: "a".into(),
            repo_root: std::path::PathBuf::from("."),
            repo_id: String::new(),
            execution_backend: "local".into(),
            cloud_environment_id: String::new(),
            requested_width: sensitivities.len().max(1) as u32,
            streams: sensitivities
                .iter()
                .enumerate()
                .map(|(i, s)| mdx_core::FleetStream {
                    stream_id: format!("s{i}"),
                    objective: "x".into(),
                    write_scope: vec![format!("f{i}.rs")],
                    interface_contract: String::new(),
                    depends_on: Vec::new(),
                    checks: Vec::new(),
                    max_turns: 0,
                    builder_slot: String::new(),
                    data_sensitivity: (*s).into(),
                })
                .collect(),
            integration_owned_paths: Vec::new(),
            full_suite_checks: Vec::new(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
        }
    }

    #[test]
    fn a_members_first_line_verdict_is_read_conservatively() {
        assert_eq!(parse_member_verdict("VERDICT: ready\n1. ..."), "ready");
        assert_eq!(
            parse_member_verdict("VERDICT: needs work\n1. a bug"),
            "needs work"
        );
        // No clear verdict line reads as unclear, never as a pass.
        assert_eq!(parse_member_verdict("looks fine to me"), "unclear");
    }

    #[test]
    fn a_panel_is_convened_for_a_sensitive_fleet_not_an_ordinary_one() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        unsafe {
            std::env::remove_var("MDX_FLEET_REVIEW_PANEL");
        }
        // Ordinary fleet, no opt-in -> single reviewer.
        assert!(!should_convene_panel(&plan_with(&["open", "internal"])));
        // Any sensitive stream -> panel.
        assert!(should_convene_panel(&plan_with(&["open", "sensitive"])));
        // Explicit opt-in -> panel even when nothing is sensitive.
        unsafe {
            std::env::set_var("MDX_FLEET_REVIEW_PANEL", "1");
        }
        assert!(should_convene_panel(&plan_with(&["open"])));
        unsafe {
            std::env::remove_var("MDX_FLEET_REVIEW_PANEL");
        }
    }

    #[test]
    fn hosted_fleet_proves_an_integrated_branch_on_the_hosted_backend() {
        let mut plan = plan_with(&["open", "open"]);
        plan.execution_backend = "hosted_sandbox".to_string();
        plan.cloud_environment_id = "cloud_env_test".to_string();
        plan.full_suite_checks = vec!["cargo test".to_string()];
        plan.integration_owned_paths.clear();

        assert!(hosted_full_suite_required(&plan));

        plan.execution_backend = "local".to_string();
        assert!(!hosted_full_suite_required(&plan));
    }

    #[test]
    fn proof_only_integration_accepts_a_green_no_change_run() {
        let outcome = crate::forge_loop_runner::ForgeRunOutcome {
            run_id: "forge_run_integration".to_string(),
            status: "RUN_FINISHED_NO_CHANGE",
            turns_used: 2,
            files_changed: 0,
            check_runs: 1,
            check_duration_ms: 12,
            branch: Some("fleet/demo".to_string()),
            commit_sha: Some("abc".to_string()),
            finish_summary: "checks passed on unchanged integrated branch".to_string(),
            last_check_passed: true,
        };

        assert!(integration_run_completed(
            &outcome,
            &[],
            &["cargo test".to_string()]
        ));
        assert!(!integration_run_completed(
            &outcome,
            &["src/lib.rs".to_string()],
            &["cargo test".to_string()]
        ));
        assert!(!integration_run_completed(&outcome, &[], &[]));

        let failed = crate::forge_loop_runner::ForgeRunOutcome {
            last_check_passed: false,
            ..outcome
        };
        assert!(!integration_run_completed(
            &failed,
            &[],
            &["cargo test".to_string()]
        ));
    }

    #[test]
    fn integration_repair_scope_is_union_of_stream_and_integration_paths() {
        let mut plan = plan_with(&["open", "internal"]);
        plan.streams[0].write_scope = vec!["src/a.js".into(), "src/shared.js".into()];
        plan.streams[1].write_scope = vec!["src/b.js".into(), "src/shared.js".into()];
        plan.integration_owned_paths = vec!["src/index.js".into(), "src/a.js".into()];

        assert_eq!(
            integration_repair_write_scope(&plan),
            vec!["src/a.js", "src/shared.js", "src/b.js", "src/index.js"]
        );
    }

    #[test]
    fn integration_repair_checks_prefer_full_suite_then_repo_profile_then_stream_checks() {
        let repo = std::env::temp_dir().join(format!(
            "mdx_fleet_integration_repair_checks_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&repo).expect("repo");
        std::fs::write(
            repo.join("package.json"),
            r#"{"type":"module","scripts":{"test":"node --test"}}"#,
        )
        .expect("package");
        let mut plan = plan_with(&["open", "internal"]);
        plan.repo_root = repo.clone();
        plan.streams[0].checks = vec!["npm run test:a".into()];
        plan.streams[1].checks = vec!["npm run test:b".into()];
        assert_eq!(integration_repair_checks(&plan), vec!["npm test"]);

        plan.full_suite_checks = vec!["npm run verify".into()];
        assert_eq!(integration_repair_checks(&plan), vec!["npm run verify"]);

        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn review_needs_work_detector_accepts_single_or_panel_verdicts() {
        assert!(review_verdict_needs_work("VERDICT: needs work\n1. bug"));
        assert!(review_verdict_needs_work(
            "PANEL VERDICT: needs work [gpt=ready; opus=needs work]"
        ));
        assert!(!review_verdict_needs_work("VERDICT: ready"));
    }

    #[test]
    fn clean_structural_integration_review_is_a_concrete_ready_verdict() {
        let plan = plan_with(&["open", "internal", "open"]);

        let verdict = clean_structural_integration_review(&plan, 3).expect("ready verdict");

        assert!(verdict.starts_with("VERDICT: ready"));
        assert!(verdict.contains("merged 3 disjoint stream(s)"));
        assert!(verdict.contains("no integration-owned paths"));
        assert!(!review_verdict_needs_work(&verdict));
    }

    #[test]
    fn clean_structural_integration_review_requires_valid_disjoint_streams() {
        let mut plan = plan_with(&["open", "internal"]);
        plan.streams[1].write_scope = plan.streams[0].write_scope.clone();

        assert!(clean_structural_integration_review(&plan, 2).is_none());
        assert!(clean_structural_integration_review(&plan_with(&["open"]), 0).is_none());
    }

    #[test]
    fn integration_run_started_evidence_is_repo_intelligent_and_strategy_aware() {
        let repo = std::env::temp_dir().join(format!(
            "mdx_fleet_integration_evidence_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(repo.join("src/main/java/app")).expect("mkdir");
        std::fs::write(repo.join("pom.xml"), "<project></project>\n").expect("pom");
        std::fs::write(
            repo.join("src/main/java/app/App.java"),
            "package app; public class App {}\n",
        )
        .expect("source");
        let mut plan = FleetRunPlan {
            fleet_id: "fleet_java".into(),
            tenant_id: "t".into(),
            actor_id: "human:eng".into(),
            repo_root: repo.clone(),
            repo_id: "github_test".into(),
            execution_backend: "local".into(),
            cloud_environment_id: String::new(),
            requested_width: 6,
            streams: vec![mdx_core::FleetStream {
                stream_id: "s1_service".into(),
                objective: "Build service".into(),
                write_scope: vec!["src/main/java/app/App.java".into()],
                interface_contract: "Keep App API stable".into(),
                depends_on: Vec::new(),
                checks: vec!["mvn test".into()],
                max_turns: 20,
                builder_slot: String::new(),
                data_sensitivity: "internal".into(),
            }],
            integration_owned_paths: vec!["pom.xml".into()],
            full_suite_checks: vec!["mvn test".into()],
            mission_id: String::new(),
            mission_milestone_id: String::new(),
        };

        let evidence = integration_run_started_evidence_fields(&plan);
        let get = |key: &str| {
            evidence
                .iter()
                .find(|(field, _)| field == key)
                .map(|(_, value)| value.as_str())
                .unwrap_or("")
        };

        assert_eq!(
            get("fleet_integration_strategy_id"),
            "integration_contract_wiring"
        );
        assert_eq!(
            get("fleet_integration_required_semantic_operations"),
            "file_outline\ndependency_map\nreferences\ndiagnostics"
        );
        assert_eq!(get("language_pack_id"), "java-maven");
        assert_eq!(get("selected_checks"), "mvn test");
        assert_eq!(
            get("selected_checks_source"),
            "fleet_integration_full_suite_checks"
        );
        assert_eq!(get("execution_geometry_lane"), "fleet_integration");
        assert_eq!(get("execution_geometry_requested_workers"), "6");

        plan.execution_backend = "hosted_sandbox".into();
        plan.cloud_environment_id = "env_cloud_beta".into();
        let hosted_evidence = integration_run_started_evidence_fields(&plan);
        let hosted_get = |key: &str| {
            hosted_evidence
                .iter()
                .find(|(field, _)| field == key)
                .map(|(_, value)| value.as_str())
                .unwrap_or("")
        };
        assert_eq!(hosted_get("execution_backend_kind"), "hosted_sandbox");
        assert_eq!(hosted_get("execution_target_kind"), "mdx_cloud");
        assert_eq!(hosted_get("execution_target_id"), "env_cloud_beta");
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn retry_branch_is_used_when_fleet_branch_already_exists() {
        let repo = std::env::temp_dir().join(format!(
            "mdx_fleet_integration_branch_retry_{}_{}",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).expect("mkdir");
        git_ok(&repo, &["init", "-b", "main"]);
        git_ok(&repo, &["config", "user.email", "test@mdx.local"]);
        git_ok(&repo, &["config", "user.name", "mdx-test"]);
        std::fs::write(repo.join("README.md"), "test\n").expect("write");
        git_ok(&repo, &["add", "."]);
        git_ok(&repo, &["commit", "-m", "init"]);
        git_ok(&repo, &["branch", "fleet/demo"]);

        let branch = next_integration_branch(&repo, "demo");

        assert_ne!(branch, "fleet/demo");
        assert!(branch.starts_with("fleet/demo-"));
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn integration_detail_reports_final_post_wiring_proof() {
        let pre = FullSuiteProof {
            passed: false,
            detail: "full_suite=failed command=npm_test".to_string(),
        };
        let outcome = crate::forge_loop_runner::ForgeRunOutcome {
            run_id: "forge_run_test".to_string(),
            status: "RUN_FINISHED_DONE",
            turns_used: 3,
            files_changed: 1,
            check_runs: 1,
            check_duration_ms: 12,
            branch: Some("fleet/demo".to_string()),
            commit_sha: Some("abc".to_string()),
            finish_summary: "done".to_string(),
            last_check_passed: true,
        };

        let detail = integration_detail_after_wiring(&pre, &outcome);

        assert!(detail.contains("pre_wiring_full_suite=failed"));
        assert!(detail.contains("post_wiring_full_suite=passed"));
        assert!(!detail.starts_with("full_suite=failed"));
    }

    fn git_ok(repo: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .current_dir(repo)
            .args(args)
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "git {:?} failed: {}{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn panel_reviewers_use_session_provider_keys_for_diverse_models() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        unsafe {
            for key in [
                "OPENAI_API_KEY",
                "ANTHROPIC_API_KEY",
                "XAI_API_KEY",
                "MDX_OPENAI_MODEL",
                "MDX_ANTHROPIC_MODEL",
                "MDX_XAI_BUILD_MODEL",
            ] {
                std::env::remove_var(key);
            }
        }
        crate::secret_store::global().clear_tenant("local_tenant");
        crate::secret_store::global().set_for_tenant(
            "local_tenant",
            "OPENAI_API_KEY",
            "openai-session-key",
        );
        crate::secret_store::global().set_for_tenant(
            "local_tenant",
            "ANTHROPIC_API_KEY",
            "anthropic-session-key",
        );
        crate::secret_store::global().set_for_tenant(
            "local_tenant",
            "XAI_API_KEY",
            "xai-session-key",
        );
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let panel = panel_reviewers(&kernel, "local_tenant");
        let models = panel
            .iter()
            .map(|member| member.model.as_str())
            .collect::<Vec<_>>();

        assert!(models.contains(&"gpt-5.6-sol"), "{models:?}");
        assert!(models.contains(&"claude-opus-4-8"), "{models:?}");
        assert!(models.contains(&"grok-4.5"), "{models:?}");
        assert_eq!(panel.len(), 3);
        crate::secret_store::global().clear_tenant("local_tenant");
    }
}
