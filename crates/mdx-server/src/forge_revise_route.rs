// Review and revise: a reviewer read a run's diff and wants changes, so a
// new run picks up that run's branch and addresses the feedback - building
// on the prior work, committing back onto the same branch. The Linear-Diffs
// "iterate with agents from the review surface" pattern, governed: the
// revise run is its own governed run with its own trail; it bypasses no
// gate, and the updated branch still waits for the human's ship decision.
use crate::RouteResponse;
use crate::forge_loop_runner::{ForgeRunRequest, run_forge_loop};
use mdx_core::{MdxKernel, json_string_literal};
use std::path::Path;
use std::sync::{Arc, RwLock};

const REVISION_REQUIRED_SEMANTIC_OPERATIONS: &[&str] =
    &["file_outline", "related_tests", "diagnostics"];
const REVISION_RUNTIME_MAX_REPAIR_ATTEMPTS: usize = 2;

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/run-revisions.json" {
        return None;
    }
    Some(handle(method, body, kernel))
}

fn handle(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let field = |name: &str| json_string_field(body, name).unwrap_or_default();
    let original_run = field("run_id");
    let comment = field("comment");
    let execution_backend = field("execution_backend");
    let cloud_environment_id = field("cloud_environment_id");
    let hosted_execution = execution_backend == "hosted_sandbox";
    if !execution_backend.is_empty() && execution_backend != "local" && !hosted_execution {
        return record_refusal_and_return(
            kernel,
            &resolved,
            "revision execution backend must be local or hosted_sandbox",
            "",
        );
    }
    if original_run.trim().is_empty() || comment.trim().is_empty() {
        return record_refusal_and_return(
            kernel,
            &resolved,
            "name the run to revise and the change you want",
            "",
        );
    }
    let mut allowed_commands = parse_commands(body);
    let operator_supplied_checks = allowed_commands
        .iter()
        .any(|command| !command.trim().is_empty());

    // Resolve the original run's branch and repo from its receipts first;
    // repo profiling and semantic fallback indexing can touch the filesystem,
    // so keep them outside the kernel lock.
    let (branch, repo_root, base_work_item_id, repo_id) = {
        let kernel_guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let Some((branch, repo_root)) = run_branch_and_root(&kernel_guard, original_run.trim())
        else {
            drop(kernel_guard);
            return record_refusal_and_return(
                kernel,
                &resolved,
                "that run has no branch to revise - it is still working or stopped without committing",
                "",
            );
        };
        let base_work_item_id = run_work_item_id(&kernel_guard, original_run.trim());
        let repo_id = run_repo_id(&kernel_guard, original_run.trim());
        (branch, repo_root, base_work_item_id, repo_id)
    };
    if hosted_execution {
        if cloud_environment_id.trim().is_empty() || repo_id.trim().is_empty() {
            return record_refusal_and_return(
                kernel,
                &resolved,
                "hosted revision requires the run repository and cloud environment",
                "",
            );
        }
        let guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned while admitting hosted revision".to_string())?;
        if let Err(reason) = crate::mobile_cloud_route::require_verified_environment(
            &guard,
            &resolved.tenant_id,
            &cloud_environment_id,
            &repo_id,
        ) {
            return record_refusal_and_return(kernel, &resolved, &reason, "");
        }
    }
    // Inherit the original run's write scope. A revise run that skipped the
    // work-queue scope preflight could edit outside the item's boundary, so
    // resolve the scope from the base run's work_item_id and refuse the
    // revision if the preflight refuses. No recorded work item keeps the
    // empty scope the loop already treats as unscoped.
    let write_scope = if base_work_item_id.trim().is_empty() {
        Vec::new()
    } else {
        match crate::forge_run_route::mdx_work_queue_scope_preflight(
            &comment,
            &base_work_item_id,
            &repo_root,
        ) {
            Ok(scope) => scope,
            Err(reason) => {
                return record_refusal_and_return(kernel, &resolved, &reason, "");
            }
        }
    };
    if json_bool_field(body, "enforce_repair_budget").unwrap_or(false) {
        let attempts = {
            let kernel_guard = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            revision_attempt_count_for_branch(&kernel_guard, &branch)
        };
        if attempts >= REVISION_RUNTIME_MAX_REPAIR_ATTEMPTS {
            return record_refusal_and_return(
                kernel,
                &resolved,
                &format!(
                    "revision repair budget exhausted for {branch}; attempts={attempts} max={REVISION_RUNTIME_MAX_REPAIR_ATTEMPTS}"
                ),
                "",
            );
        }
    }
    let repo_root_path = std::path::PathBuf::from(&repo_root);
    let repo_profile = crate::forge_repo_profile::profile_repo(&repo_root_path);
    if allowed_commands.is_empty() {
        allowed_commands = repo_profile.suggested_checks.clone();
    }
    normalize_revision_commands(&mut allowed_commands, &repo_profile);
    let setup_added =
        ensure_revision_setup_commands(&mut allowed_commands, &repo_profile, &repo_root_path);
    if let Some(reason) = missing_revision_proof_refusal(&allowed_commands) {
        return Ok(refusal(&reason));
    }
    let selected_checks_source =
        selected_checks_source(operator_supplied_checks, &allowed_commands, setup_added);
    let semantic_session = crate::forge_semantic_query_route::semantic_session_evidence(
        &repo_root_path,
        &repo_profile,
    );
    let requested_builder_slot = json_string_field(body, "builder_slot").unwrap_or_default();
    let (builder_slot, builder_casting) = {
        let kernel_guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let slot = if requested_builder_slot.trim().is_empty() {
            crate::forge_model_provider_routing::preferred_slot_for_role(
                &kernel_guard,
                &resolved.tenant_id,
                "integrator",
            )
        } else {
            requested_builder_slot
        };
        let casting = crate::forge_model_scorecard_route::builder_casting_evidence(
            &kernel_guard,
            repo_profile.language_pack_id,
            "revision_repair",
            "medium",
            &slot,
        );
        if !crate::forge_turn_client::TurnClient::builder_configured_for_tenant(
            &kernel_guard,
            &resolved.tenant_id,
            casting.selected_slot_for_execution(),
        ) {
            drop(kernel_guard);
            return record_refusal_and_return(
                kernel,
                &resolved,
                "Connect a model for the selected repair role before starting a revision run.",
                "",
            );
        }
        (slot, casting)
    };
    let mut evidence_fields = revision_started_evidence_fields(RevisionStartedEvidenceContext {
        original_run_id: original_run.trim(),
        branch: &branch,
        operator_intent: &comment,
        repo_id: &repo_id,
        repo_root: &repo_root,
        repo_profile: &repo_profile,
        semantic_session: &semantic_session,
        selected_checks: &allowed_commands,
        selected_checks_source,
        builder_casting: &builder_casting,
    });
    evidence_fields.push((
        "execution_backend_kind".to_string(),
        if hosted_execution {
            "hosted_sandbox".to_string()
        } else {
            "local".to_string()
        },
    ));
    evidence_fields.push((
        "execution_target_kind".to_string(),
        if hosted_execution {
            "mdx_cloud".to_string()
        } else {
            "paired_host".to_string()
        },
    ));
    evidence_fields.push((
        "execution_target_id".to_string(),
        if hosted_execution {
            cloud_environment_id.clone()
        } else {
            "local_host".to_string()
        },
    ));

    // Mint the revise run and witness its evidence-rich start under one lock.
    let run_id = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let run_id = kernel.mint_id("forge_run");
        let evidence_refs: Vec<(&str, &str)> = evidence_fields
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str()))
            .collect();
        let run_started = kernel.record_forge_run_event_with_evidence_fields(
            mdx_core::ForgeRunEvent {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                run_id: &run_id,
                event: "run_started",
                work_item_id: &base_work_item_id,
                detail: &format!(
                    "accepted: {} selected_checks repo_root={repo_root} revising={branch}",
                    allowed_commands.len()
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &resolved.identity,
            &evidence_refs,
        );
        let _ = kernel.record_forge_run_event(mdx_core::ForgeRunEvent {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            run_id: &run_id,
            event: "evidence_appended",
            work_item_id: "",
            detail: "revision_semantic_strategy_assigned",
            turn: 0,
            input_tokens: 0,
            output_tokens: 0,
        });
        // Derive the implicit learning signal from the disposition that just
        // happened: a human read the diff and asked for changes. If the run
        // being revised was already shipped, the edit-after-accept is the
        // richer Tesla-style disagreement signal. Fail-soft: a derivation
        // miss never breaks the revise path.
        if let Ok(run_started) = run_started {
            let base_run = original_run.trim();
            let signal_kind = if base_run_was_shipped(&kernel, base_run) {
                "edit_after_accept"
            } else {
                "revision_requested"
            };
            let lesson = crate::learning_routes::implicit_signal::disposition_lesson(
                signal_kind,
                base_run,
                &comment,
            );
            crate::learning_routes::implicit_signal::derive_implicit_signal(
                &mut kernel,
                &resolved.identity,
                mdx_core::LearningImplicitSignal {
                    tenant_id: &resolved.tenant_id,
                    actor_id: &resolved.actor_id,
                    source_receipt_id: &run_started.receipt_id,
                    signal_kind,
                    work_item: base_run,
                    delta_summary: &comment,
                    lesson_candidate: &lesson,
                },
            );
        }
        run_id
    };

    let request = ForgeRunRequest {
        run_id: run_id.clone(),
        tenant_id: resolved.tenant_id.clone(),
        actor_id: resolved.actor_id.clone(),
        work_item_id: base_work_item_id,
        intent: comment,
        allowed_commands,
        max_turns: 30,
        revise_branch: Some(branch.clone()),
        resume: false,
        write_scope,
        check_target_dir: hosted_execution.then(|| format!("hosted://{cloud_environment_id}")),
        builder_slot,
        work_complexity_tier: "medium".to_string(),
        semantic_policy_required_operations: REVISION_REQUIRED_SEMANTIC_OPERATIONS
            .iter()
            .map(|operation| (*operation).to_string())
            .collect(),
        semantic_policy_source: "revision_repair".to_string(),
        execution_geometry_requested_workers: 1,
        execution_geometry_effective_workers: 1,
        execution_geometry_lane: "revision_repair".to_string(),
        execution_geometry_route: "/forge/run-revisions.json".to_string(),
        mission_id: String::new(),
        mission_milestone_id: String::new(),
        max_cost_cents: 0,
        max_runtime_ms: 0,
        envelope_id: String::new(),
        plan_only: false,
        reasoning_effort: String::new(),
    };
    let kernel_for_thread = Arc::clone(kernel);
    std::thread::Builder::new()
        .name(format!("forge-revise-{run_id}"))
        .spawn(move || {
            // Catch a loop panic so the revise thread never aborts or escapes;
            // the boundary snapshot still runs.
            if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let outcome = run_forge_loop(&request, &repo_root_path, &kernel_for_thread);
                crate::forge_outcome_signal_route::record_run_outcome_signal(
                    &kernel_for_thread,
                    &request,
                    &outcome,
                );
            })) {
                eprintln!("forge revise thread panicked, recorded short of finish: {panic:?}");
            }
            crate::kernel_snapshot::snapshot_at_boundary(&kernel_for_thread);
        })
        .map_err(|error| format!("could not start the revise thread: {error}"))?;

    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":{},"status":"RUN_STARTED","run_id":{},"revising_branch":{},"projection_route":"/forge/runs/projection.json","authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(if hosted_execution {
                "mdx-forge-run-revise-hosted-post"
            } else {
                "mdx-forge-run-revise-local-post"
            }),
            json_string_literal(&run_id),
            json_string_literal(&branch),
        ),
    ))
}

// The branch a run produced and the repo it lives in, from its receipts.
pub(crate) fn run_branch_and_root(kernel: &MdxKernel, run_id: &str) -> Option<(String, String)> {
    let mut branch = String::new();
    let mut root = String::new();
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        if receipt.payload.get("run_id").map(String::as_str) != Some(run_id) {
            continue;
        }
        let detail = receipt
            .payload
            .get("detail")
            .map(String::as_str)
            .unwrap_or("");
        if detail.starts_with("branch=") {
            branch = detail
                .trim_start_matches("branch=")
                .split(' ')
                .next()
                .unwrap_or("")
                .to_string();
        }
        if let Some(rest) = detail.split("repo_root=").nth(1) {
            root = rest.split(' ').next().unwrap_or("").to_string();
        }
    }
    if branch.is_empty() {
        return None;
    }
    if root.is_empty() {
        let repo_id = run_repo_id(kernel, run_id);
        if let Some(connected_root) = kernel.forge_repo_root(&repo_id) {
            root = connected_root;
        } else {
            root = repo_root().to_string_lossy().to_string();
        }
    }
    Some((branch, root))
}

// The work_item_id a run started under, from its run_started receipt. An
// empty string means the run recorded no work item and stays unscoped.
fn run_work_item_id(kernel: &MdxKernel, run_id: &str) -> String {
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        if receipt.payload.get("run_id").map(String::as_str) != Some(run_id) {
            continue;
        }
        if receipt.payload.get("event").map(String::as_str) != Some("run_started") {
            continue;
        }
        if let Some(work_item_id) = receipt.payload.get("work_item_id")
            && !work_item_id.trim().is_empty()
        {
            return work_item_id.clone();
        }
    }
    String::new()
}

fn run_repo_id(kernel: &MdxKernel, run_id: &str) -> String {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| receipt.payload.get("run_id").map(String::as_str) == Some(run_id))
        .rev()
        .find_map(|receipt| {
            ["handoff_repository_id", "repo_id"].iter().find_map(|key| {
                receipt
                    .payload
                    .get(*key)
                    .filter(|value| !value.trim().is_empty())
                    .cloned()
            })
        })
        .unwrap_or_default()
}

/// Whether the run being revised was already shipped. A revision on top of an
/// accepted change is an edit-after-accept: the accept missed something the
/// reviewer now wants fixed, which is the higher-signal disagreement class.
fn base_run_was_shipped(kernel: &MdxKernel, run_id: &str) -> bool {
    if run_id.is_empty() {
        return false;
    }
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.ship.decided")
        .iter()
        .any(|receipt| receipt.payload.get("run_id").map(String::as_str) == Some(run_id))
}

fn revision_attempt_count_for_branch(kernel: &MdxKernel, branch: &str) -> usize {
    let branch = branch.trim();
    if branch.is_empty() {
        return 0;
    }
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| {
            receipt.payload.get("event").map(String::as_str) == Some("run_started")
                && detail_field(
                    receipt
                        .payload
                        .get("detail")
                        .map(String::as_str)
                        .unwrap_or(""),
                    "revising",
                ) == branch
        })
        .count()
}

fn detail_field(detail: &str, key: &str) -> String {
    let needle = format!("{key}=");
    detail
        .split_whitespace()
        .find_map(|token| token.strip_prefix(&needle))
        .unwrap_or("")
        .to_string()
}

fn repo_root() -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    cwd.ancestors()
        .find(|path| path.join(".git").exists())
        .map(|path| path.to_path_buf())
        .unwrap_or(cwd)
}

struct RevisionStartedEvidenceContext<'a> {
    original_run_id: &'a str,
    branch: &'a str,
    operator_intent: &'a str,
    repo_id: &'a str,
    repo_root: &'a str,
    repo_profile: &'a crate::forge_repo_profile::ForgeRepoProfile,
    semantic_session: &'a crate::forge_semantic_query_route::SemanticSessionEvidence,
    selected_checks: &'a [String],
    selected_checks_source: &'a str,
    builder_casting: &'a crate::forge_model_scorecard_route::BuilderCastingEvidence,
}

fn revision_started_evidence_fields(
    context: RevisionStartedEvidenceContext<'_>,
) -> Vec<(String, String)> {
    let RevisionStartedEvidenceContext {
        original_run_id,
        branch,
        operator_intent,
        repo_id,
        repo_root,
        repo_profile,
        semantic_session,
        selected_checks,
        selected_checks_source,
        builder_casting,
    } = context;
    vec![
        (
            "revision_base_run_id".to_string(),
            original_run_id.to_string(),
        ),
        ("revision_base_branch".to_string(), branch.to_string()),
        ("operator_intent".to_string(), operator_intent.to_string()),
        ("operator_ask".to_string(), operator_intent.to_string()),
        (
            "run_title".to_string(),
            crate::forge_run_route::run_title_from_intent(operator_intent),
        ),
        (
            "run_title_source".to_string(),
            "revision_comment".to_string(),
        ),
        (
            "run_title_grants_execution_authority".to_string(),
            "false".to_string(),
        ),
        ("repo_id".to_string(), repo_id.to_string()),
        ("repo_root".to_string(), repo_root.to_string()),
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
            repo_profile.detected_language_packs.join(","),
        ),
        (
            "repo_profile_quality_signals".to_string(),
            repo_profile.quality_signals.join(","),
        ),
        (
            "repo_profile_review_axes".to_string(),
            repo_profile.review_axes.join(","),
        ),
        (
            "repo_profile_principal_review_gates".to_string(),
            repo_profile.principal_review_gates.join(","),
        ),
        (
            "repo_profile_language_pack_guidance".to_string(),
            repo_profile.language_pack_guidance.join(","),
        ),
        (
            "repo_profile_semantic_intelligence".to_string(),
            repo_profile.semantic_intelligence.join(","),
        ),
        (
            "repo_profile_semantic_tool_readiness".to_string(),
            repo_profile.semantic_tool_readiness.join(","),
        ),
        (
            "repo_profile_semantic_session_id".to_string(),
            semantic_session.session_id.clone(),
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
            repo_profile.toolchain_readiness.join(","),
        ),
        (
            "repo_profile_suggested_checks".to_string(),
            repo_profile.suggested_checks.join(","),
        ),
        (
            "repo_profile_artifact_patterns".to_string(),
            repo_profile.artifact_patterns.join(","),
        ),
        (
            "repo_profile_proof_plan_status".to_string(),
            repo_profile.proof_plan_status.to_string(),
        ),
        (
            "repo_profile_proof_plan_next_action".to_string(),
            repo_profile.proof_plan_next_action.clone(),
        ),
        (
            "repo_profile_proof_plan_summary".to_string(),
            repo_profile.proof_plan_summary.clone(),
        ),
        (
            "repo_profile_source".to_string(),
            "revision_repair_profile".to_string(),
        ),
        (
            "repo_profile_grants_execution_authority".to_string(),
            "false".to_string(),
        ),
        ("selected_checks".to_string(), selected_checks.join(",")),
        (
            "selected_checks_source".to_string(),
            selected_checks_source.to_string(),
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
            "principal_orientation_required_semantic_operations".to_string(),
            REVISION_REQUIRED_SEMANTIC_OPERATIONS.join("\n"),
        ),
        (
            "builder_casting_status".to_string(),
            builder_casting.status.clone(),
        ),
        (
            "builder_casting_requested_slot".to_string(),
            builder_casting.requested_builder_slot.clone(),
        ),
        (
            "builder_casting_selected_slot".to_string(),
            builder_casting.selected_builder_slot.clone(),
        ),
        (
            "builder_casting_selected_model_profile_id".to_string(),
            builder_casting.selected_model_profile_id.clone(),
        ),
        (
            "builder_casting_selected_provider_family".to_string(),
            builder_casting.selected_provider_family.clone(),
        ),
        (
            "builder_casting_selected_model_id".to_string(),
            builder_casting.selected_model_id.clone(),
        ),
        (
            "builder_casting_recommended_slot".to_string(),
            builder_casting.recommended_builder_slot.clone(),
        ),
        (
            "builder_casting_recommended_model_profile_id".to_string(),
            builder_casting.recommended_model_profile_id.clone(),
        ),
        (
            "builder_casting_recommended_provider_family".to_string(),
            builder_casting.recommended_provider_family.clone(),
        ),
        (
            "builder_casting_recommended_model_id".to_string(),
            builder_casting.recommended_model_id.clone(),
        ),
        (
            "builder_casting_basis".to_string(),
            builder_casting.basis.clone(),
        ),
        (
            "builder_casting_matching_eval_score_count".to_string(),
            builder_casting.matching_eval_score_count.to_string(),
        ),
        (
            "builder_casting_accepted_eval_score_count".to_string(),
            builder_casting.accepted_eval_score_count.to_string(),
        ),
        (
            "builder_casting_matching_run_count".to_string(),
            builder_casting.matching_run_count.to_string(),
        ),
        (
            "builder_casting_done_rate_pct".to_string(),
            builder_casting.done_rate_pct.to_string(),
        ),
        (
            "builder_casting_requested_slot_matches_evidence".to_string(),
            builder_casting.requested_slot_matches_evidence.to_string(),
        ),
        (
            "builder_casting_grants_execution_authority".to_string(),
            "false".to_string(),
        ),
    ]
}

fn normalize_revision_commands(
    selected_checks: &mut [String],
    repo_profile: &crate::forge_repo_profile::ForgeRepoProfile,
) {
    if repo_profile.language_pack_id != "python" {
        return;
    }
    for check in selected_checks {
        if check.trim() == "pytest" {
            *check = crate::forge_repo_profile::python_proof_command().to_string();
        }
    }
}

fn ensure_revision_setup_commands(
    selected_checks: &mut Vec<String>,
    repo_profile: &crate::forge_repo_profile::ForgeRepoProfile,
    repo_root: &Path,
) -> bool {
    let mut added = false;
    let required_setup = crate::forge_repo_profile::required_setup_commands(
        repo_root,
        repo_profile.language_pack_id,
    );
    for setup in required_setup.into_iter().rev() {
        if selected_checks.iter().any(|check| check.trim() == setup) {
            continue;
        }
        if selected_checks
            .iter()
            .any(|check| !crate::forge_repo_profile::is_setup_command(check))
        {
            selected_checks.insert(0, setup);
            added = true;
        }
    }
    added
}

fn missing_revision_proof_refusal(selected_checks: &[String]) -> Option<String> {
    if selected_checks.iter().any(|check| {
        !check.trim().is_empty() && !crate::forge_repo_profile::is_setup_command(check)
    }) {
        None
    } else {
        Some(
            "revision needs at least one behavioral proof command; setup alone is not evidence"
                .to_string(),
        )
    }
}

fn selected_checks_source(
    operator_supplied_checks: bool,
    selected_checks: &[String],
    setup_commands_added: bool,
) -> &'static str {
    if operator_supplied_checks && setup_commands_added {
        "operator_supplied_plus_repo_setup"
    } else if operator_supplied_checks {
        "operator_supplied"
    } else if selected_checks.iter().any(|check| !check.trim().is_empty()) {
        "repo_profile_inferred"
    } else {
        "none_recorded"
    }
}

fn parse_commands(body: &str) -> Vec<String> {
    let Ok(payload) = serde_json::from_str::<serde_json::Value>(body) else {
        return Vec::new();
    };
    payload
        .get("allowed_commands")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .flat_map(str::lines)
        .map(str::trim)
        .filter(|command| !command.is_empty())
        .map(str::to_string)
        .collect()
}

fn record_refusal_and_return(
    kernel: &Arc<RwLock<MdxKernel>>,
    resolved: &crate::request_security::ResolvedWriteIdentity,
    reason: &str,
    requested_run_id: &str,
) -> Result<RouteResponse, String> {
    let receipt_id = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        kernel
            .record_forge_run_refusal_with_identity(
                mdx_core::ForgeRunRefusal {
                    tenant_id: &resolved.tenant_id,
                    actor_id: &resolved.actor_id,
                    requested_run_id,
                    reason,
                    route: "/forge/run-revisions.json",
                    repo_id: "mdx",
                },
                &resolved.identity,
            )
            .map(|r| r.receipt_id)
            .unwrap_or_default()
    };
    Ok(refusal_with_receipt(reason, &receipt_id))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-run-revise-local-post","status":"REFUSED","reason":{},"run_started_receipt_id":"","production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

fn refusal_with_receipt(reason: &str, receipt_id: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-run-revise-local-post","status":"REFUSED","reason":{},"run_started_receipt_id":{},"production_write_allowed":false}}"#,
            json_string_literal(reason),
            json_string_literal(receipt_id)
        ),
    )
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get(key)?
        .as_str()
        .map(str::to_string)
}

fn json_bool_field(body: &str, key: &str) -> Option<bool> {
    let marker = format!("\"{key}\":");
    let after = body.split(&marker).nth(1)?.trim_start();
    if after.starts_with("true") {
        Some(true)
    } else if after.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_commands_decode_json_and_split_multiline_values() {
        assert_eq!(
            parse_commands(
                r#"{"comment":"retry","allowed_commands":["npm ci\n npm test","node --test \"test file.js\""]}"#,
            ),
            vec![
                "npm ci".to_string(),
                "npm test".to_string(),
                "node --test \"test file.js\"".to_string(),
            ]
        );
    }

    #[test]
    fn revision_request_preserves_json_escaped_newlines() {
        assert_eq!(
            json_string_field(
                r#"{"comment":"First review note.\nSecond review note."}"#,
                "comment"
            )
            .as_deref(),
            Some("First review note.\nSecond review note.")
        );
    }

    #[test]
    fn run_branch_resolves_the_connected_repo_when_event_detail_omits_root() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .connect_forge_repo(mdx_core::ForgeRepoConnect {
                tenant_id: "local_tenant",
                actor_id: "local_user",
                repo_id: "demo_repo",
                label: "Demo",
                root: "/tmp/mdx-demo-repo",
                kind: "local",
                origin: "",
            })
            .expect("connect repo");
        kernel
            .record_forge_run_event_with_evidence_fields(
                mdx_core::ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id: "forge_run_demo",
                    event: "run_started",
                    work_item_id: "",
                    detail: "branch=forge/demo",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &mdx_core::GovernedWriteIdentity::local_demo("local_user"),
                &[("repo_id", "demo_repo")],
            )
            .expect("record run");

        assert_eq!(
            run_branch_and_root(&kernel, "forge_run_demo"),
            Some(("forge/demo".to_string(), "/tmp/mdx-demo-repo".to_string()))
        );
    }

    #[test]
    fn revision_dependency_free_node_proof_does_not_get_setup_prefix() {
        let repo = temp_repo("revision_node_no_deps");
        std::fs::write(
            repo.join("package.json"),
            r#"{"scripts":{"test":"node --test"}}"#,
        )
        .expect("package");
        let profile = crate::forge_repo_profile::profile_repo(&repo);
        let mut commands = vec!["npm test".to_string()];

        let added = ensure_revision_setup_commands(&mut commands, &profile, &repo);

        assert!(!added);
        assert_eq!(commands, vec!["npm test"]);
        assert_eq!(
            selected_checks_source(true, &commands, added),
            "operator_supplied"
        );
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn revision_setup_is_added_before_operator_node_proof_when_lockfile_exists() {
        let repo = temp_repo("revision_node_setup");
        std::fs::write(
            repo.join("package.json"),
            r#"{"scripts":{"test":"node --test"}}"#,
        )
        .expect("package");
        std::fs::write(repo.join("package-lock.json"), "{}\n").expect("lockfile");
        let profile = crate::forge_repo_profile::profile_repo(&repo);
        let mut commands = vec!["npm test".to_string()];

        let added = ensure_revision_setup_commands(&mut commands, &profile, &repo);

        assert!(added);
        assert_eq!(commands, vec!["npm ci", "npm test"]);
        assert_eq!(
            selected_checks_source(true, &commands, added),
            "operator_supplied_plus_repo_setup"
        );
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn revision_refuses_setup_only_proof() {
        let reason = missing_revision_proof_refusal(&["npm install --no-package-lock".to_string()])
            .expect("setup only should refuse");

        assert!(reason.contains("setup alone"));
    }

    #[test]
    fn revision_evidence_names_repo_and_semantic_contract() {
        let repo = temp_repo("revision_evidence");
        std::fs::write(
            repo.join("package.json"),
            r#"{"scripts":{"test":"node --test"}}"#,
        )
        .expect("package");
        std::fs::create_dir_all(repo.join("src")).expect("src");
        std::fs::write(repo.join("src/index.js"), "export const x = 1;\n").expect("source");
        let profile = crate::forge_repo_profile::profile_repo(&repo);
        let semantic =
            crate::forge_semantic_query_route::semantic_session_evidence(&repo, &profile);
        let casting = crate::forge_model_scorecard_route::BuilderCastingEvidence {
            status: "INSUFFICIENT_EVIDENCE".to_string(),
            requested_builder_slot: "OPUS".to_string(),
            selected_builder_slot: "OPUS".to_string(),
            recommended_builder_slot: "".to_string(),
            selected_model_profile_id: "anthropic_opus_profile".to_string(),
            selected_provider_family: "anthropic".to_string(),
            selected_model_id: "claude-opus-4-8".to_string(),
            recommended_model_profile_id: "".to_string(),
            recommended_provider_family: "".to_string(),
            recommended_model_id: "".to_string(),
            basis: "requested_slot".to_string(),
            matching_eval_score_count: 0,
            accepted_eval_score_count: 0,
            matching_run_count: 0,
            done_rate_pct: 0,
            requested_slot_matches_evidence: true,
            ratified_grant_receipt_id: String::new(),
        };

        let selected_checks = [
            "npm install --no-package-lock".to_string(),
            "npm test".to_string(),
        ];
        let evidence = revision_started_evidence_fields(RevisionStartedEvidenceContext {
            original_run_id: "forge_run_base",
            branch: "fleet/demo",
            operator_intent: "Clarify the mobile acceptance heading.",
            repo_id: "demo_repo",
            repo_root: repo.to_string_lossy().as_ref(),
            repo_profile: &profile,
            semantic_session: &semantic,
            selected_checks: &selected_checks,
            selected_checks_source: "operator_supplied_plus_repo_setup",
            builder_casting: &casting,
        });
        let get = |key: &str| {
            evidence
                .iter()
                .find(|(field, _)| field == key)
                .map(|(_, value)| value.as_str())
                .unwrap_or("")
        };

        assert_eq!(get("revision_base_run_id"), "forge_run_base");
        assert_eq!(get("revision_base_branch"), "fleet/demo");
        assert_eq!(
            get("operator_intent"),
            "Clarify the mobile acceptance heading."
        );
        assert_eq!(get("run_title"), "Clarify the mobile acceptance heading.");
        assert_eq!(get("run_title_source"), "revision_comment");
        assert_eq!(get("repo_id"), "demo_repo");
        assert_eq!(get("language_pack_id"), "node");
        assert_eq!(
            get("principal_orientation_required_semantic_operations"),
            "file_outline\nrelated_tests\ndiagnostics"
        );
        assert_eq!(
            get("selected_checks_source"),
            "operator_supplied_plus_repo_setup"
        );
        assert_eq!(get("repo_profile_grants_execution_authority"), "false");
        assert_eq!(get("builder_casting_selected_slot"), "OPUS");
        assert_eq!(get("builder_casting_selected_provider_family"), "anthropic");
        assert_eq!(get("builder_casting_selected_model_id"), "claude-opus-4-8");
        let _ = std::fs::remove_dir_all(repo);
    }

    #[test]
    fn revision_attempt_count_is_scoped_to_revised_branch() {
        let mut kernel = MdxKernel::boot_local();
        for (run_id, branch) in [
            ("forge_run_revision_1", "fleet/demo"),
            ("forge_run_revision_2", "fleet/demo"),
            ("forge_run_other", "fleet/other"),
        ] {
            kernel
                .record_forge_run_event(mdx_core::ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    run_id,
                    event: "run_started",
                    work_item_id: "",
                    detail: &format!(
                        "accepted: 1 selected_checks repo_root=/tmp/repo revising={branch}"
                    ),
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("revision start");
        }

        assert_eq!(revision_attempt_count_for_branch(&kernel, "fleet/demo"), 2);
        assert_eq!(revision_attempt_count_for_branch(&kernel, "fleet/other"), 1);
        assert_eq!(
            revision_attempt_count_for_branch(&kernel, "fleet/missing"),
            0
        );
    }

    #[test]
    fn json_bool_field_reads_runtime_budget_flag() {
        let body = r#"{"run_id":"forge_run_1","enforce_repair_budget":true}"#;

        assert_eq!(json_bool_field(body, "enforce_repair_budget"), Some(true));
        assert_eq!(json_bool_field(body, "missing"), None);
    }

    #[test]
    fn revise_inherits_base_run_work_item_scope() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: "forge_run_scoped_base",
                event: "run_started",
                work_item_id: "tier0_dev_101",
                detail: "accepted: 1 selected_checks repo_root=/tmp/repo revising=fleet/demo",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("scoped run start");

        assert_eq!(
            run_work_item_id(&kernel, "forge_run_scoped_base"),
            "tier0_dev_101"
        );
    }

    #[test]
    fn revise_run_started_receipt_carries_inherited_work_item_id() {
        // The transitivity guarantee: handle() records the revise run's own
        // run_started receipt with work_item_id set to the base run's resolved
        // id, so a later revise-of-a-revise resolves scope from it instead of
        // an empty string. Record a receipt shaped like a revise run's start
        // and assert run_work_item_id reads it back - the same query path a
        // subsequent revision would take.
        let mut kernel = MdxKernel::boot_local();
        let base_work_item_id = "tier0_dev_202";
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: "forge_run_revise_child",
                event: "run_started",
                work_item_id: base_work_item_id,
                detail: "accepted: 1 selected_checks repo_root=/tmp/repo revising=fleet/demo",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("revise run start");

        assert_eq!(
            run_work_item_id(&kernel, "forge_run_revise_child"),
            "tier0_dev_202"
        );
    }

    #[test]
    fn revise_with_no_base_work_item_keeps_empty_scope() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event(mdx_core::ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:eng",
                run_id: "forge_run_unscoped_base",
                event: "run_started",
                work_item_id: "",
                detail: "accepted: 1 selected_checks repo_root=/tmp/repo revising=fleet/demo",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("unscoped run start");

        assert!(run_work_item_id(&kernel, "forge_run_unscoped_base").is_empty());
    }

    #[test]
    fn revise_empty_input_refusal_carries_receipt_id() {
        // The receipt guarantee: an empty run_id+comment refusal must still
        // record a refusal receipt and surface it, so the "no" is auditable.
        // record_refusal_and_return puts that receipt id in the
        // run_started_receipt_id field of the REFUSED body.
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let body = r#"{"run_id":"","comment":""}"#;

        let response = handle("POST", body, &kernel).expect("refusal response");

        let receipt_id = json_string_field(&response.body, "run_started_receipt_id")
            .expect("run_started_receipt_id field present");
        assert!(
            !receipt_id.trim().is_empty(),
            "refusal must carry a non-empty receipt id, got {response:?}"
        );
    }

    fn temp_repo(label: &str) -> std::path::PathBuf {
        let repo = std::env::temp_dir().join(format!(
            "mdx_forge_revise_{label}_{}_{}",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).expect("mkdir");
        repo
    }
}
