// The Forge harness run over HTTP. POST starts a run from a work item
// and returns its run_id immediately - the loop runs on its own thread
// (a real run takes minutes), streaming receipts into the ledger as it
// goes. GET projects a run's event trail and derived status from those
// receipts. The browser watches a run the same way it reads anything
// else here: by folding the chain.
use crate::RouteResponse;
use crate::forge_loop_runner::{ForgeRunRequest, run_forge_loop};
use crate::forge_turn_client::TurnClient;
use mdx_core::{
    DIRECT_RUN_MAX_WORKERS, ForgeExecutionGeometry, ForgeLanguageTaskCorpusEntry,
    ForgeLongHorizonMissionCheckpoint, ForgeRunRefusal, MdxKernel, classify_forge_work,
    forge_execution_geometry_for_width, forge_language_task_corpus, json_string_literal,
    language_task_contamination_policy, language_task_engineering_facets,
    language_task_evaluation_oracle, language_task_human_timebox_minutes,
};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

type ExecutionGeometry = ForgeExecutionGeometry;

static ACTIVE_FORGE_RUNS: OnceLock<Mutex<ActiveForgeRunCounts>> = OnceLock::new();

#[cfg(test)]
thread_local! {
    static SKIP_BACKGROUND_RUNS_FOR_TEST: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) struct SkipBackgroundRunsGuard {
    previous: bool,
}

#[cfg(test)]
impl Drop for SkipBackgroundRunsGuard {
    fn drop(&mut self) {
        SKIP_BACKGROUND_RUNS_FOR_TEST.set(self.previous);
    }
}

#[cfg(test)]
pub(crate) fn skip_background_runs_for_test() -> SkipBackgroundRunsGuard {
    let previous = SKIP_BACKGROUND_RUNS_FOR_TEST.replace(true);
    SkipBackgroundRunsGuard { previous }
}

#[derive(Default)]
struct ActiveForgeRunCounts {
    total: usize,
    by_actor: HashMap<String, usize>,
}

#[derive(Debug)]
struct ForgeRunPermit {
    actor_id: String,
}

impl Drop for ForgeRunPermit {
    fn drop(&mut self) {
        let registry =
            ACTIVE_FORGE_RUNS.get_or_init(|| Mutex::new(ActiveForgeRunCounts::default()));
        let Ok(mut active) = registry.lock() else {
            return;
        };
        active.total = active.total.saturating_sub(1);
        if let Some(count) = active.by_actor.get_mut(&self.actor_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                active.by_actor.remove(&self.actor_id);
            }
        }
    }
}

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/forge/runs.json" => Some(handle_start(method, body, kernel)),
        "/forge/runs/projection.json" => Some(handle_projection(method, kernel)),
        _ => None,
    }
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn env_flag_enabled(name: &str, default: bool) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off" | "disabled"
            )
        })
        .unwrap_or(default)
}

fn reserve_forge_run_permits(
    actor_id: &str,
    requested_workers: u32,
) -> Result<Vec<ForgeRunPermit>, String> {
    let per_actor = env_usize(
        "MDX_FORGE_RUN_MAX_ACTIVE_PER_ACTOR",
        DIRECT_RUN_MAX_WORKERS as usize,
    );
    let global = env_usize("MDX_FORGE_RUN_MAX_ACTIVE_GLOBAL", 64);
    reserve_forge_run_permits_with_caps(actor_id, requested_workers, per_actor, global)
}

fn reserve_forge_run_permits_with_caps(
    actor_id: &str,
    requested_workers: u32,
    per_actor_cap: usize,
    global_cap: usize,
) -> Result<Vec<ForgeRunPermit>, String> {
    let count = (requested_workers as usize).max(1);
    if count > per_actor_cap {
        return Err(format!(
            "This run asks for {count} active workers, above the per-user cap of {per_actor_cap}."
        ));
    }
    let registry = ACTIVE_FORGE_RUNS.get_or_init(|| Mutex::new(ActiveForgeRunCounts::default()));
    let mut active = registry
        .lock()
        .map_err(|_| "Forge run admission is temporarily unavailable.".to_string())?;
    let actor_active = active.by_actor.get(actor_id).copied().unwrap_or(0);
    if actor_active + count > per_actor_cap {
        return Err(format!(
            "You already have {actor_active} active Forge run workers; this request would exceed the per-user cap of {per_actor_cap}."
        ));
    }
    if active.total + count > global_cap {
        return Err(format!(
            "Forge is at its active run cap of {global_cap}; try again after a run finishes."
        ));
    }
    active.total += count;
    *active.by_actor.entry(actor_id.to_string()).or_insert(0) += count;
    Ok((0..count)
        .map(|_| ForgeRunPermit {
            actor_id: actor_id.to_string(),
        })
        .collect())
}

fn handle_start(
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
    let repo_id = field("repo_id");
    let requested_run_id = json_string_field(body, "requested_run_id").unwrap_or_default();
    // Resume continues a prior run of the same id: the loop loads that run's
    // persisted transcript and picks up from the last turn. It is meaningful
    // only alongside a requested_run_id that names a run with a stored
    // transcript; a resume with no prior transcript falls through to a fresh
    // start. Accepted as JSON true or the string "true".
    let resume_requested = matches!(
        json_string_field(body, "resume").as_deref(),
        Some("true") | Some("1")
    ) || serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value["resume"].as_bool())
        .unwrap_or(false);
    // Every run start re-observes the execution posture; a flipped switch
    // (host exec, live delivery, run start) lands on the ledger before
    // any work runs under it.
    crate::forge_execution_posture::record_current_posture(kernel, "run_start");
    if crate::forge_loop_runner::bench_open_commands_env_enabled() {
        return record_refusal_and_return(
            kernel,
            &resolved,
            "Benchmark open-command posture is only allowed through mdx-server forge-bench; restart the served kernel without MDX_FORGE_BENCH_OPEN_COMMANDS=1 before starting governed runs.",
            &repo_id,
            &requested_run_id,
        );
    }
    if !env_flag_enabled("MDX_FORGE_RUN_START_ENABLED", true) {
        return record_refusal_and_return(
            kernel,
            &resolved,
            "Forge run start is paused by the operator.",
            &repo_id,
            &requested_run_id,
        );
    }
    let mut max_cost_cents = json_u32_field(body, "max_cost_cents").unwrap_or(0);
    let mut max_runtime_ms = json_u32_field(body, "max_runtime_ms").unwrap_or(0) as u64;
    // Plan mode + per-run reasoning effort from the operator's control surface.
    let plan_only = matches!(
        json_string_field(body, "plan_only").as_deref(),
        Some("true") | Some("1")
    );
    let run_reasoning_effort = json_string_field(body, "reasoning_effort").unwrap_or_default();
    // Autonomy envelope, optional but VERIFIED when named: a run may bind
    // itself to a recorded envelope, and a revoked or expired envelope
    // refuses admission instead of decorating it. Mid-run revocation is
    // polled by the loop alongside operator controls.
    let envelope_id = json_string_field(body, "envelope_id")
        .unwrap_or_default()
        .trim()
        .to_string();
    if !envelope_id.is_empty() {
        let blocker = {
            let kernel = kernel
                .read()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            kernel.autonomy_envelope_authority_blocker(&envelope_id)
        };
        if let Some(reason) = blocker {
            return record_refusal_and_return(
                kernel,
                &resolved,
                &format!("autonomy envelope refused: {reason}"),
                &repo_id,
                &requested_run_id,
            );
        }
    }
    let intent = field("intent");
    if intent.trim().is_empty() {
        return record_refusal_and_return(
            kernel,
            &resolved,
            "a run needs an intent - what should the build agent do?",
            &repo_id,
            &requested_run_id,
        );
    }
    let requested_run_title = json_string_field(body, "run_title").unwrap_or_default();
    let requested_execution_backend =
        json_string_field(body, "execution_backend").unwrap_or_else(|| "local".to_string());
    let cloud_environment_id = json_string_field(body, "cloud_environment_id").unwrap_or_default();
    let hosted_execution = matches!(
        requested_execution_backend.as_str(),
        "hosted" | "hosted_sandbox" | "cloud" | "aws_sandbox"
    );
    if requested_execution_backend != "local" && !hosted_execution {
        return record_refusal_and_return(
            kernel,
            &resolved,
            "execution_backend must be local or hosted_sandbox",
            &repo_id,
            &requested_run_id,
        );
    }
    let mut execution_geometry = match execution_geometry_from_body(body) {
        Ok(geometry) => geometry,
        Err(reason) => {
            return record_refusal_and_return(
                kernel,
                &resolved,
                &reason,
                &repo_id,
                &requested_run_id,
            );
        }
    };
    let work_item_id = field("work_item_id");
    let mission_attachment = MissionAttachment::from_body(body);
    let mut allowed_commands = parse_commands(body);
    let operator_supplied_checks = allowed_commands
        .iter()
        .any(|check| !check.trim().is_empty());
    let requested_builder_slot = json_string_field(body, "builder_slot").unwrap_or_default();
    if !repo_id.trim().is_empty() {
        let recovered = match crate::mobile_cloud_route::ensure_managed_cloud_checkout(
            kernel,
            &resolved.tenant_id,
            &repo_id,
        ) {
            Ok(recovered) => recovered,
            Err(reason) => {
                return record_refusal_and_return(
                    kernel,
                    &resolved,
                    &format!("connected repository checkout could not be recovered: {reason}"),
                    &repo_id,
                    &requested_run_id,
                );
            }
        };
        if recovered.is_none()
            && let Err(reason) =
                crate::forge_repo_route::ensure_managed_remote_checkout(kernel, &repo_id)
        {
            return record_refusal_and_return(
                kernel,
                &resolved,
                &format!("connected repository checkout could not be recovered: {reason}"),
                &repo_id,
                &requested_run_id,
            );
        }
    }
    let resolved_repo = {
        let kernel_guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let resolved_repo = match resolve_run_repo(&kernel_guard, &repo_id) {
            Ok(repo) => repo,
            Err(reason) => {
                drop(kernel_guard);
                return record_refusal_and_return(
                    kernel,
                    &resolved,
                    &reason,
                    &repo_id,
                    &requested_run_id,
                );
            }
        };
        if let Err(reason) = mission_attachment.validate(&kernel_guard) {
            drop(kernel_guard);
            return record_refusal_and_return(
                kernel,
                &resolved,
                &reason,
                &repo_id,
                &requested_run_id,
            );
        }
        if hosted_execution {
            if let Err(reason) = crate::mobile_cloud_route::require_verified_environment(
                &kernel_guard,
                &resolved.tenant_id,
                &cloud_environment_id,
                &repo_id,
            ) {
                drop(kernel_guard);
                return record_refusal_and_return(
                    kernel,
                    &resolved,
                    &reason,
                    &repo_id,
                    &requested_run_id,
                );
            }
            let definition_path = Path::new(&resolved_repo.root).join(".mdx/environment.json");
            if std::fs::symlink_metadata(&definition_path).is_err()
                && let Err(reason) =
                    crate::mobile_cloud_route::restore_verified_environment_definition(
                        &kernel_guard,
                        &resolved.tenant_id,
                        &cloud_environment_id,
                        &repo_id,
                        Path::new(&resolved_repo.root),
                    )
            {
                drop(kernel_guard);
                return record_refusal_and_return(
                    kernel,
                    &resolved,
                    &reason,
                    &repo_id,
                    &requested_run_id,
                );
            }
            let definition = match crate::mobile_hosted_sandbox::load_environment(Path::new(
                &resolved_repo.root,
            )) {
                Ok(definition) => definition,
                Err(reason) => {
                    drop(kernel_guard);
                    return record_refusal_and_return(
                        kernel,
                        &resolved,
                        &reason,
                        &repo_id,
                        &requested_run_id,
                    );
                }
            };
            if cloud_environment_id != definition.environment_id
                || definition.repository_id != repo_id
            {
                drop(kernel_guard);
                return record_refusal_and_return(
                    kernel,
                    &resolved,
                    "hosted execution must name the verified environment for this repository",
                    &repo_id,
                    &requested_run_id,
                );
            }
        }
        resolved_repo
    };
    let run_repo_root = resolved_repo.root.clone();
    let write_scope = match mdx_work_queue_scope_preflight(&intent, &work_item_id, &run_repo_root) {
        Ok(scope) => scope,
        Err(reason) => {
            return record_refusal_and_return(
                kernel,
                &resolved,
                &reason,
                &repo_id,
                &requested_run_id,
            );
        }
    };
    let base_repo_profile = crate::forge_repo_profile::profile_repo(Path::new(&run_repo_root));
    let mut stack_aware_checks_inferred = false;
    if allowed_commands.is_empty() {
        if let Some(stack_checks) =
            stack_aware_selected_checks(&intent, &write_scope, Path::new(&run_repo_root))
        {
            allowed_commands = stack_checks;
            stack_aware_checks_inferred = true;
        } else {
            allowed_commands = base_repo_profile
                .suggested_checks
                .iter()
                .map(|check| (*check).to_string())
                .collect();
        }
    }
    let repo_profile = crate::forge_repo_profile::profile_repo_for_task(
        Path::new(&run_repo_root),
        &intent,
        &write_scope,
        &allowed_commands,
    );
    let repo_profile_value = repo_profile_json_value(&repo_profile);
    normalize_selected_commands_for_repo(&mut allowed_commands, &repo_profile);
    let setup_commands_added = ensure_required_setup_commands(
        &mut allowed_commands,
        &repo_profile,
        Path::new(&run_repo_root),
    );
    let selected_checks_source = selected_checks_source_for(
        operator_supplied_checks,
        &allowed_commands,
        setup_commands_added,
        stack_aware_checks_inferred,
    );
    if let Some(reason) = missing_selected_checks_refusal(&allowed_commands, &repo_profile) {
        return record_refusal_and_return(kernel, &resolved, &reason, &repo_id, &requested_run_id);
    }
    if let Some(reason) = missing_selected_check_toolchain_refusal(
        &allowed_commands,
        &repo_profile.toolchain_readiness,
        !hosted_execution,
    ) {
        return record_refusal_and_return(kernel, &resolved, &reason, &repo_id, &requested_run_id);
    }
    if !repo_has_committed_head(Path::new(&run_repo_root)) {
        return record_refusal_and_return(
            kernel,
            &resolved,
            "Forge needs a git repo with an initial commit so it can create an isolated worktree. Commit the repo once, then start the run again.",
            &repo_id,
            &requested_run_id,
        );
    }
    let repo_profile_detected_files = repo_profile.detected_files.join(",");
    let repo_profile_detected_language_packs = repo_profile.detected_language_packs.join(",");
    let repo_profile_quality_signals = repo_profile.quality_signals.join(",");
    let repo_profile_standards_sources = repo_profile.standards_sources.join(",");
    let repo_profile_standards_source_fingerprints =
        repo_profile.standards_source_fingerprints.join(",");
    let repo_profile_standards_source_summaries = repo_profile.standards_source_summaries.join(",");
    let repo_profile_review_axes = repo_profile.review_axes.join(",");
    let repo_profile_principal_review_gates = repo_profile.principal_review_gates.join(",");
    let repo_profile_language_pack_guidance = repo_profile.language_pack_guidance.join(",");
    let repo_profile_semantic_intelligence = repo_profile.semantic_intelligence.join(",");
    let repo_profile_semantic_tool_readiness = repo_profile.semantic_tool_readiness.join(",");
    let semantic_session_source_file_count = "0".to_string();
    let semantic_session_indexed_file_count = "0".to_string();
    let semantic_session_indexed_symbol_count = "0".to_string();
    let semantic_session_related_test_anchor_count = "0".to_string();
    let repo_profile_toolchain_readiness = repo_profile.toolchain_readiness.join(",");
    let repo_profile_suggested_checks = repo_profile.suggested_checks.join(",");
    let repo_profile_artifact_patterns = repo_profile.artifact_patterns.join(",");
    let repo_profile_proof_plan_status = repo_profile.proof_plan_status;
    let repo_profile_proof_plan_next_action = repo_profile.proof_plan_next_action.clone();
    let repo_profile_proof_plan_summary = repo_profile.proof_plan_summary.clone();
    let principal_orientation_gate_required = repo_profile.language_pack_id != "generic"
        && !repo_profile.semantic_intelligence.is_empty();
    let principal_orientation_gate_required = if principal_orientation_gate_required {
        "true"
    } else {
        "false"
    };
    let selected_checks = allowed_commands.join(",");
    let run_intake = run_intake_evidence(
        &resolved_repo,
        &repo_profile,
        &repo_profile_value,
        &allowed_commands,
        selected_checks_source,
        hosted_execution,
    );
    let work_classification = classify_forge_work(&intent, &selected_checks);
    let run_strategy_enabled = crate::forge_run_strategy::strategy_enabled(body);
    let run_strategy = crate::forge_run_strategy::resolve_strategy(
        &work_classification,
        body,
        Some(execution_geometry.requested_workers),
    );
    if run_strategy_enabled {
        execution_geometry = match forge_execution_geometry_for_width(run_strategy.width) {
            Ok(geometry) => geometry,
            Err(reason) => {
                return record_refusal_and_return(
                    kernel,
                    &resolved,
                    &reason,
                    &repo_id,
                    &requested_run_id,
                );
            }
        };
        max_cost_cents = run_strategy.max_cost_cents;
        max_runtime_ms = run_strategy.max_runtime_ms;
    }
    if plan_only && run_strategy.harness_id != "mdx_native" {
        return record_refusal_and_return(
            kernel,
            &resolved,
            "Manual plan-first runs use the MDx Native read-only planning loop. Choose MDx Native for this plan, or turn off Plan first and let the adaptive planner guide the selected external harness.",
            &repo_id,
            &requested_run_id,
        );
    }
    if execution_geometry.fleet_required {
        let reason = if run_strategy.harness_id != "mdx_native" {
            format!(
                "{} is currently admitted for solo and direct candidate runs up to {} workers. Choose MDx Native for governed fleet or mission execution; Forge will not silently replace the locked harness.",
                if run_strategy.harness_id == "grok_build" {
                    "Grok Build"
                } else {
                    "Codex CLI"
                },
                DIRECT_RUN_MAX_WORKERS
            )
        } else {
            format!(
                "Forge resolved this request to {} workers, which requires the governed {} route. Start the recommended {} through /forge/fleet-plans.json or use /forge/long-horizon-missions.json for checkpointed work; /forge/runs.json admits at most {} workers directly.",
                execution_geometry.requested_workers,
                run_strategy.execution_shape,
                run_strategy.execution_shape,
                DIRECT_RUN_MAX_WORKERS
            )
        };
        return record_refusal_and_return(kernel, &resolved, &reason, &repo_id, &requested_run_id);
    }
    let mut external_harness_runtime = if run_strategy.harness_id == "mdx_native" {
        None
    } else {
        match crate::forge_external_harness_runner::readiness(
            &run_strategy.harness_id,
            &resolved.tenant_id,
        ) {
            Ok(runtime) => Some(runtime),
            Err(reason) => {
                return record_refusal_and_return(
                    kernel,
                    &resolved,
                    &reason,
                    &repo_id,
                    &requested_run_id,
                );
            }
        }
    };
    let external_run_authorization = if external_harness_runtime.is_some() {
        let kernel_guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        match crate::forge_fleet_eval_scoreboard::authorize_forge_external_run(
            body,
            &kernel_guard,
            &resolved.tenant_id,
            &run_strategy.harness_id,
            run_strategy.max_cost_cents,
            execution_geometry.effective_workers,
        ) {
            Ok(authorization) => Some(authorization),
            Err(reason) => {
                drop(kernel_guard);
                return record_refusal_and_return(
                    kernel,
                    &resolved,
                    &reason,
                    &repo_id,
                    &requested_run_id,
                );
            }
        }
    } else {
        None
    };
    let work_classification_confidence_pct = work_classification.confidence_pct.to_string();
    let run_strategy_width = run_strategy.width.to_string();
    let run_strategy_max_turns = run_strategy.max_turns.to_string();
    let run_strategy_max_cost_cents = run_strategy.max_cost_cents.to_string();
    let run_strategy_max_runtime_ms = run_strategy.max_runtime_ms.to_string();
    let run_strategy_operator_locked_fields = run_strategy.operator_locked_fields.join(",");
    let run_strategy_policy_floor_fields = run_strategy.policy_floor_fields.join(",");
    let language_task_alignment =
        align_language_task(repo_profile.language_pack_id, &work_classification);
    let language_task_human_timebox_minutes = language_task_alignment
        .task
        .as_ref()
        .map(language_task_human_timebox_minutes)
        .unwrap_or(0)
        .to_string();
    let language_task_corpus_id = language_task_alignment
        .task
        .as_ref()
        .map(|task| task.task_corpus_id)
        .unwrap_or("");
    let language_task_class = language_task_alignment
        .task
        .as_ref()
        .map(|task| task.task_class)
        .unwrap_or("");
    let language_task_complexity_tier = language_task_alignment
        .task
        .as_ref()
        .map(|task| task.complexity_tier)
        .unwrap_or("");
    let effective_complexity_tier = if language_task_complexity_tier.is_empty() {
        work_classification.complexity_tier.as_str()
    } else {
        language_task_complexity_tier
    };
    let semantic_policy_required_operations =
        semantic_policy_operations_from_intake(&run_intake.semantic_orientation_operations);
    let semantic_policy_source = if semantic_policy_required_operations.is_empty() {
        "none".to_string()
    } else {
        "repo_intake.semantic_orientation_operations".to_string()
    };
    let language_task_visible_check = language_task_alignment
        .task
        .as_ref()
        .map(|task| task.visible_check)
        .unwrap_or("");
    let language_task_hidden_check_slot = language_task_alignment
        .task
        .as_ref()
        .map(|task| task.hidden_check_slot)
        .unwrap_or("");
    let language_task_artifact_noise_expected = language_task_alignment
        .task
        .as_ref()
        .map(|task| task.artifact_noise_expected)
        .unwrap_or("");
    let language_task_required_principal_review_gates = language_task_alignment
        .task
        .as_ref()
        .map(|task| task.required_principal_review_gates)
        .unwrap_or("");
    let language_task_engineering_facets = language_task_alignment
        .task
        .as_ref()
        .map(language_task_engineering_facets)
        .unwrap_or("");
    let language_task_evaluation_oracle = language_task_alignment
        .task
        .as_ref()
        .map(language_task_evaluation_oracle)
        .unwrap_or("");
    let language_task_contamination_policy = language_task_alignment
        .task
        .as_ref()
        .map(language_task_contamination_policy)
        .unwrap_or("");
    let mut builder_casting = {
        let kernel_guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let casting = crate::forge_model_scorecard_route::builder_casting_evidence(
            &kernel_guard,
            repo_profile.language_pack_id,
            if language_task_class.is_empty() {
                &work_classification.task_class
            } else {
                language_task_class
            },
            if language_task_complexity_tier.is_empty() {
                &work_classification.complexity_tier
            } else {
                language_task_complexity_tier
            },
            &requested_builder_slot,
        );
        if external_harness_runtime.is_none()
            && !TurnClient::builder_configured_for_tenant(
                &kernel_guard,
                &resolved.tenant_id,
                casting.selected_slot_for_execution(),
            )
        {
            drop(kernel_guard);
            return record_refusal_and_return(
                kernel,
                &resolved,
                "Connect a model first. A Welcome-connected model, or XAI_API_KEY plus MDX_DEFAULT_MODEL_PROVIDER=xai in the server environment, can power Forge.",
                &repo_id,
                &requested_run_id,
            );
        }
        casting
    };
    if let Some(runtime) = external_harness_runtime.as_mut()
        && let Err(reason) = crate::forge_external_harness_runner::bind_model_choice(
            runtime,
            &mut builder_casting,
            &requested_builder_slot,
        )
    {
        return record_refusal_and_return(kernel, &resolved, &reason, &repo_id, &requested_run_id);
    }
    let builder_slot = builder_casting.selected_builder_slot.clone();
    let builder_casting_matching_eval_score_count =
        builder_casting.matching_eval_score_count.to_string();
    let builder_casting_accepted_eval_score_count =
        builder_casting.accepted_eval_score_count.to_string();
    let builder_casting_matching_run_count = builder_casting.matching_run_count.to_string();
    let builder_casting_done_rate_pct = builder_casting.done_rate_pct.to_string();
    let builder_casting_selected_model_class = crate::forge_run_stream::model_class(
        &builder_casting.selected_provider_family,
        &builder_casting.selected_builder_slot,
        &builder_casting.selected_model_id,
    );
    let (
        run_strategy_runner_id,
        run_strategy_runner_kind,
        run_strategy_runner_display_name,
        run_strategy_runner_adapter_kind,
        run_strategy_runner_execution_mode,
    ) = match external_harness_runtime.as_ref() {
        Some(runtime) if runtime.harness_id == "codex_cli" => (
            runtime.runner_id,
            "external_harness",
            "Codex CLI",
            "codex_cli",
            "isolated_external_harness",
        ),
        Some(runtime) => (
            runtime.runner_id,
            "external_harness",
            "Grok Build",
            "grok_build_cli",
            "isolated_external_harness",
        ),
        None => (
            "mdx_native_harness_runner",
            "mdx_native",
            "Forge native builder",
            "mdx_native_harness",
            "local_receipt_gated",
        ),
    };
    let builder_casting_requested_slot_matches_evidence =
        if builder_casting.requested_slot_matches_evidence {
            "true"
        } else {
            "false"
        };
    let live_run_approval_id = external_run_authorization
        .as_ref()
        .map(|value| value.approval_id.as_str())
        .unwrap_or("");
    let live_run_approval_receipt_id = external_run_authorization
        .as_ref()
        .map(|value| value.approval_receipt_id.as_str())
        .unwrap_or("");
    let runner_execution_clearance_receipt_id = external_run_authorization
        .as_ref()
        .map(|value| value.clearance_receipt_id.as_str())
        .unwrap_or("");
    let live_execution_provider = external_run_authorization
        .as_ref()
        .map(|value| value.provider)
        .unwrap_or("");
    let live_run_max_spend_cents = external_run_authorization
        .as_ref()
        .map(|value| value.max_spend_cents.to_string())
        .unwrap_or_else(|| "0".to_string());
    let live_run_max_tasks = external_run_authorization
        .as_ref()
        .map(|value| value.max_tasks.to_string())
        .unwrap_or_else(|| "0".to_string());
    let live_run_max_parallel_agents = external_run_authorization
        .as_ref()
        .map(|value| value.max_parallel_agents.to_string())
        .unwrap_or_else(|| "0".to_string());
    let live_run_task_ordinal = external_run_authorization
        .as_ref()
        .map(|value| value.task_ordinal.to_string())
        .unwrap_or_else(|| "0".to_string());
    let repo_id_for_receipt = if repo_id.trim().is_empty() {
        "mdx"
    } else {
        repo_id.as_str()
    };
    let activation_first_mission_id =
        json_string_field(body, "activation_first_mission_id").unwrap_or_default();
    if !requested_run_id.trim().is_empty() {
        if !is_reserved_forge_run_id(&requested_run_id) {
            return record_refusal_and_return(
                kernel,
                &resolved,
                "reserved run ids must be minted by Forge and start with forge_run_",
                &repo_id,
                &requested_run_id,
            );
        }
        let kernel_guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        if forge_run_id_exists(&kernel_guard, &requested_run_id) && !resume_requested {
            drop(kernel_guard);
            return record_refusal_and_return(
                kernel,
                &resolved,
                "reserved run id is already on the receipt chain; start a new run instead.",
                &repo_id,
                &requested_run_id,
            );
        }
    }
    let execution_geometry_requested_workers = execution_geometry.requested_workers.to_string();
    let execution_geometry_effective_workers = execution_geometry.effective_workers.to_string();
    let operator_intent = bounded_human_text(&intent, 1200);
    let run_title = if requested_run_title.trim().is_empty() {
        run_title_from_intent(&operator_intent)
    } else {
        bounded_run_title(&requested_run_title, 140)
    };
    let mut run_permits =
        match reserve_forge_run_permits(&resolved.actor_id, execution_geometry.effective_workers) {
            Ok(permits) => permits.into_iter(),
            Err(reason) => {
                return record_refusal_and_return(
                    kernel,
                    &resolved,
                    &reason,
                    &repo_id,
                    &requested_run_id,
                );
            }
        };
    // The acceptance of the run is on the record SYNCHRONOUSLY, before the
    // thread does anything: the run id is minted and the run_started event
    // is witnessed here, so the response cites a real receipt and the
    // chain has the run from its first instant. The thread continues with
    // the model turns from there. The repo root is resolved and recorded
    // here so the diff route can find it later.
    let (
        run_id,
        started_receipt_id,
        run_started_detail,
        stream_provider_family,
        stream_model_id,
        stream_builder_slot,
        stream_model_class,
    ) = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let run_id = if requested_run_id.trim().is_empty() {
            kernel.mint_id("forge_run")
        } else if forge_run_id_exists(&kernel, &requested_run_id) && !resume_requested {
            return record_refusal_on_kernel_and_return(
                &mut kernel,
                &resolved,
                "reserved run id is already on the receipt chain; start a new run instead.",
                &repo_id,
                &requested_run_id,
            );
        } else {
            requested_run_id.clone()
        };
        let primary_strategy = candidate_strategy(1, execution_geometry.effective_workers);
        let primary_required_semantic_operations =
            primary_strategy.required_semantic_operations.join(",");
        let parallel_candidate_index = "1".to_string();
        let parallel_candidate_count = execution_geometry.effective_workers.to_string();
        let parallel_candidate_write_scope = write_scope.join("\n");
        let max_cost_cents_field = max_cost_cents.to_string();
        let max_runtime_ms_field = max_runtime_ms.to_string();
        let evidence_fields = [
            ("repo_id", repo_id_for_receipt),
            (
                "execution_backend_kind",
                if hosted_execution {
                    "hosted_sandbox"
                } else {
                    "local"
                },
            ),
            (
                "execution_target_kind",
                if hosted_execution {
                    "mdx_cloud"
                } else {
                    "paired_host"
                },
            ),
            (
                "execution_target_id",
                if hosted_execution {
                    cloud_environment_id.as_str()
                } else {
                    "local_host"
                },
            ),
            ("cloud_environment_id", cloud_environment_id.as_str()),
            // The enforced ceilings, recorded at admission with the same
            // values the loop checks at every turn boundary (0 = server
            // default). Recorded AND enforced, per the elevation audit.
            ("max_cost_cents", max_cost_cents_field.as_str()),
            ("max_runtime_ms", max_runtime_ms_field.as_str()),
            ("budget_enforcement", "turn_boundary"),
            ("autonomy_envelope_id", envelope_id.as_str()),
            ("operator_intent", operator_intent.as_str()),
            ("operator_ask", operator_intent.as_str()),
            ("run_title", run_title.as_str()),
            ("run_title_source", "operator_intent"),
            ("run_title_grants_execution_authority", "false"),
            (
                "activation_first_mission_id",
                activation_first_mission_id.as_str(),
            ),
            ("repo_root", run_repo_root.as_str()),
            ("repo_primary_language", repo_profile.primary_language),
            ("language_pack_id", repo_profile.language_pack_id),
            (
                "repo_profile_detected_language_packs",
                repo_profile_detected_language_packs.as_str(),
            ),
            (
                "repo_profile_detected_files",
                repo_profile_detected_files.as_str(),
            ),
            (
                "repo_profile_quality_signals",
                repo_profile_quality_signals.as_str(),
            ),
            (
                "repo_profile_standards_sources",
                repo_profile_standards_sources.as_str(),
            ),
            (
                "repo_profile_standards_source_fingerprints",
                repo_profile_standards_source_fingerprints.as_str(),
            ),
            (
                "repo_profile_standards_source_summaries",
                repo_profile_standards_source_summaries.as_str(),
            ),
            (
                "repo_profile_review_axes",
                repo_profile_review_axes.as_str(),
            ),
            (
                "repo_profile_principal_review_gates",
                repo_profile_principal_review_gates.as_str(),
            ),
            (
                "repo_profile_language_pack_guidance",
                repo_profile_language_pack_guidance.as_str(),
            ),
            (
                "repo_profile_semantic_intelligence",
                repo_profile_semantic_intelligence.as_str(),
            ),
            (
                "repo_profile_semantic_tool_readiness",
                repo_profile_semantic_tool_readiness.as_str(),
            ),
            ("repo_profile_semantic_session_id", "deferred_to_worker"),
            (
                "repo_profile_semantic_fallback_index_status",
                "deferred_to_worker",
            ),
            (
                "repo_profile_semantic_source_file_count",
                semantic_session_source_file_count.as_str(),
            ),
            (
                "repo_profile_semantic_indexed_file_count",
                semantic_session_indexed_file_count.as_str(),
            ),
            (
                "repo_profile_semantic_indexed_symbol_count",
                semantic_session_indexed_symbol_count.as_str(),
            ),
            (
                "repo_profile_semantic_related_test_anchor_count",
                semantic_session_related_test_anchor_count.as_str(),
            ),
            (
                "repo_profile_semantic_session_grants_execution_authority",
                "false",
            ),
            (
                "repo_profile_toolchain_readiness",
                repo_profile_toolchain_readiness.as_str(),
            ),
            (
                "repo_profile_suggested_checks",
                repo_profile_suggested_checks.as_str(),
            ),
            (
                "repo_profile_artifact_patterns",
                repo_profile_artifact_patterns.as_str(),
            ),
            (
                "repo_profile_proof_plan_status",
                repo_profile_proof_plan_status,
            ),
            (
                "repo_profile_proof_plan_next_action",
                repo_profile_proof_plan_next_action.as_str(),
            ),
            (
                "repo_profile_proof_plan_summary",
                repo_profile_proof_plan_summary.as_str(),
            ),
            ("selected_checks", selected_checks.as_str()),
            ("selected_checks_source", selected_checks_source),
            (
                "work_classification_recommended_shape",
                &work_classification.recommended_shape,
            ),
            (
                "work_classification_task_class",
                &work_classification.task_class,
            ),
            (
                "work_classification_complexity_tier",
                &work_classification.complexity_tier,
            ),
            (
                "work_classification_confidence_pct",
                &work_classification_confidence_pct,
            ),
            (
                "work_classification_rationale",
                &work_classification.rationale,
            ),
            (
                "work_classification_source",
                "mdx_core::classify_forge_work",
            ),
            ("work_classification_grants_execution_authority", "false"),
            ("run_strategy_version", run_strategy.version),
            (
                "run_strategy_enabled",
                if run_strategy_enabled {
                    "true"
                } else {
                    "false"
                },
            ),
            ("run_strategy_mode", run_strategy.mode),
            (
                "run_strategy_outcome_preference",
                run_strategy.outcome_preference.as_str(),
            ),
            ("run_strategy_execution_shape", run_strategy.execution_shape),
            ("run_strategy_width", run_strategy_width.as_str()),
            ("run_strategy_harness_id", run_strategy.harness_id.as_str()),
            ("run_strategy_harness_source", run_strategy.harness_source),
            ("run_strategy_planner_mode", run_strategy.planner_mode),
            ("run_strategy_planner_source", run_strategy.planner_source),
            ("run_strategy_review_mode", run_strategy.review_mode),
            ("run_strategy_review_source", run_strategy.review_source),
            ("run_strategy_advisor_mode", run_strategy.advisor_mode),
            ("run_strategy_max_turns", run_strategy_max_turns.as_str()),
            (
                "run_strategy_max_cost_cents",
                run_strategy_max_cost_cents.as_str(),
            ),
            (
                "run_strategy_max_runtime_ms",
                run_strategy_max_runtime_ms.as_str(),
            ),
            (
                "run_strategy_operator_locked_fields",
                run_strategy_operator_locked_fields.as_str(),
            ),
            (
                "run_strategy_policy_floor_fields",
                run_strategy_policy_floor_fields.as_str(),
            ),
            ("run_strategy_rationale", run_strategy.rationale.as_str()),
            ("run_strategy_grants_execution_authority", "false"),
            ("live_run_approval_id", live_run_approval_id),
            ("live_run_approval_receipt_id", live_run_approval_receipt_id),
            (
                "runner_execution_clearance_receipt_id",
                runner_execution_clearance_receipt_id,
            ),
            (
                "closed_runner_clearance_receipt_id",
                runner_execution_clearance_receipt_id,
            ),
            ("live_execution_provider", live_execution_provider),
            (
                "live_run_max_spend_cents",
                live_run_max_spend_cents.as_str(),
            ),
            ("live_run_max_tasks", live_run_max_tasks.as_str()),
            (
                "live_run_max_parallel_agents",
                live_run_max_parallel_agents.as_str(),
            ),
            ("live_run_task_ordinal", live_run_task_ordinal.as_str()),
            (
                "live_execution_authority_scope",
                if external_run_authorization.is_some() {
                    "single_governed_forge_run"
                } else {
                    ""
                },
            ),
            (
                "external_harness_cost_enforcement",
                if external_run_authorization.is_some() {
                    if run_strategy.harness_id == "grok_build" {
                        "approval_ceiling_plus_turn_and_runtime_bounds_usage_metering_unavailable"
                    } else {
                        "approval_ceiling_plus_runtime_and_scope_bounds_usage_and_turn_metering_unavailable"
                    }
                } else {
                    "native_priced_token_meter"
                },
            ),
            ("language_task_corpus_id", language_task_corpus_id),
            (
                "language_task_alignment_status",
                language_task_alignment.status,
            ),
            (
                "language_task_alignment_source",
                "mdx_core::forge_language_task_corpus",
            ),
            ("language_task_class", language_task_class),
            (
                "language_task_complexity_tier",
                language_task_complexity_tier,
            ),
            ("language_task_visible_check", language_task_visible_check),
            (
                "language_task_hidden_check_slot",
                language_task_hidden_check_slot,
            ),
            (
                "language_task_artifact_noise_expected",
                language_task_artifact_noise_expected,
            ),
            (
                "language_task_required_principal_review_gates",
                language_task_required_principal_review_gates,
            ),
            (
                "language_task_engineering_facets",
                language_task_engineering_facets,
            ),
            (
                "language_task_evaluation_oracle",
                language_task_evaluation_oracle,
            ),
            (
                "language_task_human_timebox_minutes",
                &language_task_human_timebox_minutes,
            ),
            (
                "language_task_contamination_policy",
                language_task_contamination_policy,
            ),
            (
                "language_task_alignment_grants_execution_authority",
                "false",
            ),
            ("repo_profile_source", "mdx_server::forge_repo_profile"),
            ("repo_profile_grants_execution_authority", "false"),
            ("suggested_checks_are_authority", "false"),
            (
                "principal_orientation_gate_required",
                principal_orientation_gate_required,
            ),
            ("principal_orientation_gate_tool", "semantic_query"),
            ("principal_orientation_gate_grants_authority", "false"),
            (
                "repo_intake_generated_from",
                run_intake.generated_from.as_str(),
            ),
            (
                "repo_intake_readiness_status",
                run_intake.readiness_status.as_str(),
            ),
            (
                "repo_intake_medium_high_work_ready",
                run_intake.medium_high_work_ready.as_str(),
            ),
            (
                "repo_intake_safe_next_move",
                run_intake.safe_next_move.as_str(),
            ),
            (
                "repo_intake_semantic_orientation_operations",
                run_intake.semantic_orientation_operations.as_str(),
            ),
            (
                "repo_intake_proof_strategy",
                run_intake.proof_strategy.as_str(),
            ),
            (
                "repo_intake_first_run_task_ids",
                run_intake.first_run_task_ids.as_str(),
            ),
            ("repo_intake_scout_status", run_intake.scout_status.as_str()),
            (
                "repo_intake_scout_candidate_count",
                run_intake.scout_candidate_count.as_str(),
            ),
            (
                "repo_intake_scout_candidate_kinds",
                run_intake.scout_candidate_kinds.as_str(),
            ),
            (
                "repo_intake_scout_candidate_paths",
                run_intake.scout_candidate_paths.as_str(),
            ),
            (
                "repo_intake_write_scope_hint",
                run_intake.write_scope_hint.as_str(),
            ),
            (
                "repo_intake_off_limits_patterns",
                run_intake.off_limits_patterns.as_str(),
            ),
            ("repo_intake_review_focus", run_intake.review_focus.as_str()),
            ("repo_intake_source_host", run_intake.source_host.as_str()),
            (
                "repo_intake_origin_url_present",
                run_intake.origin_url_present.as_str(),
            ),
            ("repo_intake_origin_url_recorded", "false"),
            (
                "repo_intake_source_host_readiness_route",
                "/forge/source-host-readiness.json",
            ),
            (
                "repo_intake_source_host_pr_draft_route",
                "/forge/source-host-pr-drafts.json",
            ),
            ("repo_intake_provider_calls_allowed", "false"),
            ("repo_intake_run_started", "true"),
            ("repo_intake_network_call_allowed", "false"),
            ("repo_intake_production_write_allowed", "false"),
            ("repo_intake_grants_execution_authority", "false"),
            (
                "execution_geometry_requested_workers",
                execution_geometry_requested_workers.as_str(),
            ),
            (
                "execution_geometry_effective_workers",
                execution_geometry_effective_workers.as_str(),
            ),
            ("execution_geometry_lane", execution_geometry.lane),
            ("execution_geometry_route", execution_geometry.route),
            ("execution_geometry_reason", execution_geometry.reason),
            (
                "execution_geometry_fleet_required",
                execution_geometry.fleet_required_str(),
            ),
            ("execution_geometry_grants_execution_authority", "false"),
            ("parallel_candidate_primary_run_id", run_id.as_str()),
            (
                "parallel_candidate_index",
                parallel_candidate_index.as_str(),
            ),
            (
                "parallel_candidate_count",
                parallel_candidate_count.as_str(),
            ),
            (
                "parallel_candidate_write_scope",
                parallel_candidate_write_scope.as_str(),
            ),
            ("parallel_candidate_strategy_id", primary_strategy.id),
            (
                "parallel_candidate_strategy_summary",
                primary_strategy.summary,
            ),
            (
                "parallel_candidate_required_semantic_operations",
                primary_required_semantic_operations.as_str(),
            ),
            ("parallel_candidate_proof_bias", primary_strategy.proof_bias),
            ("builder_casting_status", builder_casting.status.as_str()),
            (
                "builder_casting_requested_slot",
                builder_casting.requested_builder_slot.as_str(),
            ),
            (
                "builder_casting_selected_slot",
                builder_casting.selected_builder_slot.as_str(),
            ),
            (
                "builder_casting_selected_model_profile_id",
                builder_casting.selected_model_profile_id.as_str(),
            ),
            (
                "builder_casting_selected_provider_family",
                builder_casting.selected_provider_family.as_str(),
            ),
            (
                "builder_casting_selected_model_id",
                builder_casting.selected_model_id.as_str(),
            ),
            (
                "builder_casting_selected_model_class",
                builder_casting_selected_model_class,
            ),
            (
                "builder_casting_recommended_slot",
                builder_casting.recommended_builder_slot.as_str(),
            ),
            (
                "builder_casting_recommended_model_profile_id",
                builder_casting.recommended_model_profile_id.as_str(),
            ),
            (
                "builder_casting_recommended_provider_family",
                builder_casting.recommended_provider_family.as_str(),
            ),
            (
                "builder_casting_recommended_model_id",
                builder_casting.recommended_model_id.as_str(),
            ),
            ("builder_casting_basis", builder_casting.basis.as_str()),
            (
                "builder_casting_matching_eval_score_count",
                builder_casting_matching_eval_score_count.as_str(),
            ),
            (
                "builder_casting_accepted_eval_score_count",
                builder_casting_accepted_eval_score_count.as_str(),
            ),
            (
                "builder_casting_matching_run_count",
                builder_casting_matching_run_count.as_str(),
            ),
            (
                "builder_casting_done_rate_pct",
                builder_casting_done_rate_pct.as_str(),
            ),
            (
                "builder_casting_requested_slot_matches_evidence",
                builder_casting_requested_slot_matches_evidence,
            ),
            ("builder_casting_grants_execution_authority", "false"),
            ("runner_profile_runner_id", run_strategy_runner_id),
            ("runner_profile_runner_kind", run_strategy_runner_kind),
            (
                "runner_profile_display_name",
                run_strategy_runner_display_name,
            ),
            (
                "runner_profile_adapter_kind",
                run_strategy_runner_adapter_kind,
            ),
            (
                "runner_profile_execution_mode",
                run_strategy_runner_execution_mode,
            ),
            (
                "runner_profile_model_profile_id",
                builder_casting.selected_model_profile_id.as_str(),
            ),
            ("runner_profile_grants_execution_authority", "false"),
            ("mission_id", mission_attachment.mission_id.as_str()),
            (
                "mission_milestone_id",
                mission_attachment.milestone_id.as_str(),
            ),
            (
                "mission_checkpoint_route",
                mission_attachment.checkpoint_route(),
            ),
            ("mission_checkpoint_grants_execution_authority", "false"),
        ];
        let run_started_detail = format!(
            "accepted: {} selected_checks language_pack={} repo_id={repo_id_for_receipt} execution_geometry={} workers={}",
            allowed_commands.len(),
            repo_profile.language_pack_id,
            execution_geometry.lane,
            execution_geometry.effective_workers
        );
        let report = kernel
            .record_forge_run_event_with_evidence_fields(
                mdx_core::ForgeRunEvent {
                    tenant_id: &resolved.tenant_id,
                    actor_id: &resolved.actor_id,
                    run_id: &run_id,
                    event: "run_started",
                    work_item_id: &work_item_id,
                    detail: &run_started_detail,
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &resolved.identity,
                &evidence_fields,
            )
            .map_err(|error| error.message())?;
        // When a human-ratified fleet_casting lesson chose the builder, record
        // that the adaptation was applied, naming the run it steered and the
        // grant that authorized it. Receipt-only bookkeeping - the casting
        // decision above already stands on the grant.
        if !builder_casting.ratified_grant_receipt_id.trim().is_empty() {
            kernel.record_learning_adaptation_applied(
                mdx_core::LearningAdaptationApplied {
                    tenant_id: &resolved.tenant_id,
                    actor_id: &resolved.actor_id,
                    grant_receipt_id: &builder_casting.ratified_grant_receipt_id,
                    artifact_kind: "builder_slot",
                    artifact_id: &run_id,
                    detail: &format!(
                        "ratified lesson steered builder casting to slot {}",
                        builder_casting.selected_builder_slot
                    ),
                },
                &resolved.identity,
            );
        }
        let stream_provider_family = external_harness_runtime
            .as_ref()
            .map(|runtime| runtime.provider_family.to_string())
            .unwrap_or_else(|| builder_casting.selected_provider_family.clone());
        let stream_model_id = external_harness_runtime
            .as_ref()
            .map(|runtime| runtime.model_id.clone())
            .unwrap_or_else(|| builder_casting.selected_model_id.clone());
        let stream_builder_slot = external_harness_runtime
            .as_ref()
            .map(|runtime| runtime.runner_id.to_string())
            .unwrap_or_else(|| builder_casting.selected_builder_slot.clone());
        let stream_model_class = crate::forge_run_stream::model_class(
            &stream_provider_family,
            &stream_builder_slot,
            &stream_model_id,
        )
        .to_string();
        (
            run_id,
            report.receipt_id,
            run_started_detail,
            stream_provider_family,
            stream_model_id,
            stream_builder_slot,
            stream_model_class,
        )
    };
    crate::forge_run_stream::publish_run_started(
        &resolved.tenant_id,
        &run_id,
        &started_receipt_id,
        &crate::forge_run_stream::RunStreamModel {
            provider_family: &stream_provider_family,
            model_id: &stream_model_id,
            slot: &stream_builder_slot,
            model_class: &stream_model_class,
        },
        &run_started_detail,
    );

    let max_turns = if crate::forge_run_strategy::strategy_enabled(body) {
        run_strategy.max_turns
    } else {
        json_u32_field(body, "max_turns").unwrap_or(0)
    };
    let base_intent = intent.clone();
    let primary_intent = if execution_geometry.effective_workers > 1 {
        let strategy = candidate_strategy(1, execution_geometry.effective_workers);
        format!(
            "Parallel candidate 1/{} for this Forge run.\nStrategy: {} - {} {}\nRequired semantic operations: {}.\nProof bias: {}.\nExplore an independent solution under this strategy, keep the diff minimal, prove it with the allowed checks, and finish with the strongest reviewable result.\n\n{}",
            execution_geometry.effective_workers,
            strategy.id,
            strategy.summary,
            strategy.instructions,
            strategy.required_semantic_operations.join(", "),
            strategy.proof_bias,
            base_intent
        )
    } else {
        base_intent.clone()
    };
    let resume_branch = if resume_requested {
        kernel
            .read()
            .ok()
            .and_then(|guard| crate::forge_diff_route::branch_for_run(&guard, &run_id))
    } else {
        None
    };
    let request = ForgeRunRequest {
        run_id: run_id.clone(),
        tenant_id: resolved.tenant_id.clone(),
        actor_id: resolved.actor_id.clone(),
        work_item_id,
        intent: primary_intent,
        allowed_commands,
        max_turns,
        revise_branch: resume_branch,
        resume: resume_requested,
        write_scope,
        check_target_dir: hosted_execution.then(|| format!("hosted://{cloud_environment_id}")),
        builder_slot,
        work_complexity_tier: effective_complexity_tier.to_string(),
        semantic_policy_required_operations,
        semantic_policy_source,
        execution_geometry_requested_workers: execution_geometry.requested_workers,
        execution_geometry_effective_workers: execution_geometry.effective_workers,
        execution_geometry_lane: execution_geometry.lane.to_string(),
        execution_geometry_route: execution_geometry.route.to_string(),
        mission_id: mission_attachment.mission_id.clone(),
        mission_milestone_id: mission_attachment.milestone_id.clone(),
        max_cost_cents,
        max_runtime_ms,
        envelope_id: envelope_id.clone(),
        plan_only,
        reasoning_effort: run_reasoning_effort.clone(),
    };

    // The run works in an isolated worktree of its target repo (a
    // connected repo, or MDx itself). The live tree is never touched.
    let repo_root = std::path::PathBuf::from(run_repo_root);
    let mut candidate_run_ids = vec![run_id.clone()];
    start_forge_run_thread(
        request.clone(),
        repo_root.clone(),
        Arc::clone(kernel),
        run_permits
            .next()
            .ok_or_else(|| "Forge run admission permit missing".to_string())?,
    )?;
    if execution_geometry.effective_workers > 1 {
        for worker_index in 2..=execution_geometry.effective_workers {
            let sibling_builder_slot = candidate_builder_slot(
                builder_casting.selected_slot_for_execution(),
                &requested_builder_slot,
                worker_index,
            );
            let mut sibling_builder_casting = {
                let kernel = kernel
                    .read()
                    .map_err(|_| "kernel lock poisoned".to_string())?;
                crate::forge_model_scorecard_route::builder_casting_evidence(
                    &kernel,
                    repo_profile.language_pack_id,
                    if language_task_class.is_empty() {
                        &work_classification.task_class
                    } else {
                        language_task_class
                    },
                    if language_task_complexity_tier.is_empty() {
                        &work_classification.complexity_tier
                    } else {
                        language_task_complexity_tier
                    },
                    &sibling_builder_slot,
                )
            };
            if let Some(primary_runtime) = external_harness_runtime.as_ref() {
                let mut sibling_runtime = primary_runtime.clone();
                if let Err(reason) = crate::forge_external_harness_runner::bind_model_choice(
                    &mut sibling_runtime,
                    &mut sibling_builder_casting,
                    &requested_builder_slot,
                ) {
                    return record_refusal_and_return(
                        kernel,
                        &resolved,
                        &reason,
                        &repo_id,
                        &requested_run_id,
                    );
                }
            }
            let sibling_run_id = record_parallel_candidate_run_started(
                kernel,
                &resolved,
                &request,
                worker_index,
                &run_id,
                &base_intent,
                &run_title,
                repo_id_for_receipt,
                &repo_root,
                &repo_profile,
                selected_checks_source,
                &selected_checks,
                &execution_geometry,
                &run_strategy,
                run_strategy_enabled,
                external_run_authorization.as_ref(),
                &sibling_builder_casting,
            )?;
            let strategy = candidate_strategy(worker_index, execution_geometry.effective_workers);
            let mut sibling_request = request.clone();
            sibling_request.run_id = sibling_run_id.clone();
            sibling_request.builder_slot = sibling_builder_casting.selected_builder_slot.clone();
            sibling_request.work_item_id =
                format!("{}_candidate_{worker_index}", sibling_request.work_item_id);
            // Candidate siblings are evidence for the primary run. They must
            // not race each other to update the same mission milestone.
            sibling_request.mission_id.clear();
            sibling_request.mission_milestone_id.clear();
            sibling_request.intent = format!(
                "Parallel candidate {worker_index}/{} for this Forge run.\nStrategy: {} - {} {}\nRequired semantic operations: {}.\nProof bias: {}.\nExplore an independent solution from the same request under this strategy, do not coordinate with the other candidates, keep the diff minimal, prove it with the allowed checks, and finish with the strongest reviewable result.\n\n{}",
                execution_geometry.effective_workers,
                strategy.id,
                strategy.summary,
                strategy.instructions,
                strategy.required_semantic_operations.join(", "),
                strategy.proof_bias,
                base_intent
            );
            sibling_request.check_target_dir = if hosted_execution {
                Some(format!("hosted://{cloud_environment_id}"))
            } else {
                Some(
                    repo_root
                        .join(".mdx-local/direct-parallel-targets")
                        .join(&sibling_run_id)
                        .to_string_lossy()
                        .to_string(),
                )
            };
            start_forge_run_thread(
                sibling_request,
                repo_root.clone(),
                Arc::clone(kernel),
                run_permits
                    .next()
                    .ok_or_else(|| "Forge run admission permit missing".to_string())?,
            )?;
            candidate_run_ids.push(sibling_run_id);
        }
    }
    let candidate_run_ids_json = json_string_vec(&candidate_run_ids);
    let response_name = if hosted_execution {
        "mdx-forge-run-hosted-post"
    } else {
        "mdx-forge-run-local-post"
    };

    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":{},"status":"RUN_STARTED","run_id":{},"candidate_run_ids":[{}],"run_started_receipt_id":{},"projection_route":"/forge/runs/projection.json","strategy":{},"execution_geometry":{{"requested_workers":{},"effective_workers":{},"lane":{},"route":{},"reason":{},"fleet_required":{},"grants_execution_authority":false}},"authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(response_name),
            json_string_literal(&run_id),
            candidate_run_ids_json,
            json_string_literal(&started_receipt_id),
            crate::forge_run_strategy::strategy_json(&run_strategy),
            execution_geometry.requested_workers,
            execution_geometry.effective_workers,
            json_string_literal(execution_geometry.lane),
            json_string_literal(execution_geometry.route),
            json_string_literal(execution_geometry.reason),
            execution_geometry.fleet_required,
        ),
    ))
}

// Spawn one governed fix run for a self-delivery CI convergence step (ADR 0488,
// SelfConvergeCi). The fix run revises the base run's branch under the SAME
// envelope, its writes bounded to the envelope's allowed path scopes; a fix that
// touches a protected path is caught at the merge gate and never lands, so the
// firewall holds. It records the fix run's run_started with the envelope, repo
// root, and branch so the diff route and scope lookup can find it, reserves a
// permit, and runs it on its own thread. Returns the fix run id.
#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_self_delivery_fix_run(
    kernel: &Arc<RwLock<MdxKernel>>,
    base_run_id: &str,
    envelope_id: &str,
    fix_intent: &str,
    repo_root: std::path::PathBuf,
    revise_branch: &str,
    write_scope: Vec<String>,
    allowed_commands: Vec<String>,
    iteration: u32,
) -> Result<String, String> {
    let fix_run_id = format!("{base_run_id}_ci_fix_{iteration:03}");
    let mut permits = reserve_forge_run_permits("agent:forge_self_delivery", 1)?;
    let permit = permits
        .pop()
        .ok_or_else(|| "no run permit available for the CI fix run".to_string())?;

    // Witness the fix run's start with the evidence the diff route and the
    // self-delivery scope lookup need to discover it.
    {
        let mut guard = kernel
            .write()
            .map_err(|_| "kernel lock poisoned before fix run start".to_string())?;
        let repo_root_str = repo_root.to_string_lossy().to_string();
        guard
            .record_forge_run_event_with_evidence_fields(
                mdx_core::ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "agent:forge_self_delivery",
                    run_id: &fix_run_id,
                    event: "run_started",
                    work_item_id: "",
                    detail: &format!("branch={revise_branch} repo_root={repo_root_str}"),
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &mdx_core::GovernedWriteIdentity::local_demo("agent:forge_self_delivery"),
                &[
                    ("autonomy_envelope_id", envelope_id),
                    ("repo_root", &repo_root_str),
                    ("self_delivery_ci_fix", "true"),
                    ("self_delivery_base_run_id", base_run_id),
                ],
            )
            .map_err(|error| format!("fix run start not recorded: {error:?}"))?;
    }

    let request = ForgeRunRequest {
        run_id: fix_run_id.clone(),
        tenant_id: "local_tenant".to_string(),
        actor_id: "agent:forge_self_delivery".to_string(),
        work_item_id: String::new(),
        intent: fix_intent.to_string(),
        allowed_commands,
        max_turns: 30,
        revise_branch: Some(revise_branch.to_string()),
        resume: false,
        write_scope,
        check_target_dir: None,
        builder_slot: String::new(),
        work_complexity_tier: "medium".to_string(),
        semantic_policy_required_operations: Vec::new(),
        semantic_policy_source: "self_delivery_ci_fix".to_string(),
        execution_geometry_requested_workers: 1,
        execution_geometry_effective_workers: 1,
        execution_geometry_lane: "self_delivery_ci_fix".to_string(),
        execution_geometry_route: "/forge/self-delivery/convergences.json".to_string(),
        mission_id: String::new(),
        mission_milestone_id: String::new(),
        max_cost_cents: 0,
        max_runtime_ms: 0,
        envelope_id: envelope_id.to_string(),
        plan_only: false,
        reasoning_effort: String::new(),
    };
    start_forge_run_thread(request, repo_root, Arc::clone(kernel), permit)?;
    Ok(fix_run_id)
}

fn start_forge_run_thread(
    request: ForgeRunRequest,
    repo_root: std::path::PathBuf,
    kernel: Arc<RwLock<MdxKernel>>,
    permit: ForgeRunPermit,
) -> Result<(), String> {
    #[cfg(test)]
    if SKIP_BACKGROUND_RUNS_FOR_TEST.get() {
        drop(permit);
        return Ok(());
    }
    let run_id = request.run_id.clone();
    std::thread::Builder::new()
        .name(format!("forge-run-{run_id}"))
        .spawn(move || {
            let _permit = permit;
            // A panic in the loop must not abort the thread (orphaning the run)
            // or escape the worker - catch it, leave the receipts the run did
            // record intact, and snapshot the boundary either way.
            if let Err(panic) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let outcome = run_forge_loop(&request, &repo_root, &kernel);
                crate::forge_outcome_signal_route::record_run_outcome_signal(
                    &kernel, &request, &outcome,
                );
                crate::activation_route::record_first_mission_result_from_run_outcome(
                    &kernel, &request, &outcome,
                );
                if matches!(
                    outcome.status,
                    "RUN_FINISHED_DONE" | "RUN_FINISHED_CANNOT_PROCEED" | "RUN_STOPPED"
                ) {
                    match crate::forge_fleet_eval_scoreboard::record_result_from_run_evidence(
                        &kernel,
                        &request.run_id,
                    ) {
                        Ok(report) if report.status == "REFUSED" => {
                            eprintln!(
                                "forge run {} not submitted to eval lane: {}",
                                request.run_id,
                                if report.reason.is_empty() {
                                    "not eligible"
                                } else {
                                    report.reason.as_str()
                                }
                            );
                        }
                        Ok(report) if !report.result_receipt_id.is_empty() => {
                            eprintln!(
                                "forge run {} submitted to eval lane as quarantined evidence {}",
                                request.run_id, report.result_receipt_id
                            );
                        }
                        Ok(_) => {}
                        Err(error) => {
                            eprintln!(
                                "forge run {} eval evidence bridge failed: {error}",
                                request.run_id
                            );
                        }
                    }
                }
                record_mission_checkpoint_from_run_outcome(&kernel, &request, &outcome);
            })) {
                eprintln!("forge run thread panicked, recorded short of finish: {panic:?}");
            }
            // The run's terminal receipts must survive a restart - the
            // POST-path snapshot never sees thread writes.
            crate::kernel_snapshot::snapshot_at_boundary(&kernel);
        })
        .map(|_| ())
        .map_err(|error| format!("could not start the run thread: {error}"))
}

fn record_mission_checkpoint_from_run_outcome(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    outcome: &crate::forge_loop_runner::ForgeRunOutcome,
) {
    if request.mission_id.trim().is_empty() || request.mission_milestone_id.trim().is_empty() {
        return;
    }
    let checkpoint_event = if outcome.status == "RUN_FINISHED_DONE" {
        "milestone_completed"
    } else {
        "milestone_blocked"
    };
    let validation_status = match outcome.status {
        "RUN_FINISHED_DONE" if outcome.last_check_passed => "passed",
        "RUN_FINISHED_DONE" => "terminal_done_without_last_check",
        "RUN_FINISHED_NO_CHANGE" => "no_change",
        "RUN_FINISHED_CANNOT_PROCEED" => "cannot_proceed",
        "RUN_STOPPED" => "stopped",
        _ => "failed",
    };
    let summary = if outcome.finish_summary.trim().is_empty() {
        format!("Forge run {} ended with {}", outcome.run_id, outcome.status)
    } else {
        cap_chars(&outcome.finish_summary, 360)
    };
    let Ok(mut kernel) = kernel.write() else {
        return;
    };
    if let Err(error) = kernel.record_forge_long_horizon_mission_checkpoint_local(
        ForgeLongHorizonMissionCheckpoint {
            tenant_id: &request.tenant_id,
            actor_id: &request.actor_id,
            actor_role: "operator",
            mission_id: &request.mission_id,
            milestone_id: &request.mission_milestone_id,
            checkpoint_event,
            summary: &summary,
            validation_status,
            related_run_id: &outcome.run_id,
            related_fleet_id: "",
            steering_note: "",
        },
    ) {
        eprintln!(
            "forge run {} mission checkpoint bridge failed: {}",
            outcome.run_id,
            error.message()
        );
    }
}

#[derive(Clone, Copy)]
struct CandidateStrategy {
    id: &'static str,
    summary: &'static str,
    instructions: &'static str,
    required_semantic_operations: &'static [&'static str],
    proof_bias: &'static str,
}

fn candidate_strategy(worker_index: u32, worker_count: u32) -> CandidateStrategy {
    if worker_count <= 1 {
        return CandidateStrategy {
            id: "single_principal_path",
            summary: "one focused implementation path",
            instructions: "Use normal principal-engineer judgment: orient, make the smallest correct change, prove it, and hand off clearly.",
            required_semantic_operations: &["symbol_graph", "related_tests"],
            proof_bias: "balanced_behavioral_proof",
        };
    }
    match worker_index {
        1 => CandidateStrategy {
            id: "minimal_safe_patch",
            summary: "smallest correct diff with the lowest integration risk",
            instructions: "Prefer the narrowest source change that satisfies the behavior. Avoid broad refactors unless the task cannot be completed cleanly without them.",
            required_semantic_operations: &[
                "symbol_graph",
                "definition",
                "dependency_map",
                "related_tests",
            ],
            proof_bias: "minimal_diff_with_behavioral_check",
        },
        2 => CandidateStrategy {
            id: "test_first_behavior",
            summary: "behavior-proof-led implementation",
            instructions: "Start from the selected check and likely related tests. Add or update focused proof when the repo shape allows it, then implement to that proof.",
            required_semantic_operations: &[
                "related_tests",
                "references",
                "dependency_map",
                "diagnostics",
            ],
            proof_bias: "red_green_or_related_test_first",
        },
        3 => CandidateStrategy {
            id: "idiomatic_refactor",
            summary: "language-pack idioms and maintainability",
            instructions: "Use the language pack's idioms and principal review gates to improve the local design while preserving public behavior and scope.",
            required_semantic_operations: &[
                "file_outline",
                "definition",
                "references",
                "diagnostics",
            ],
            proof_bias: "language_idiom_and_maintainability_review",
        },
        _ => CandidateStrategy {
            id: "risk_review_repair",
            summary: "edge cases, compatibility, and failure-mode repair",
            instructions: "Hunt for boundary cases, compatibility hazards, security or performance risks, and fragile tests. Make the safest repair that proves those risks are handled.",
            required_semantic_operations: &[
                "dependency_map",
                "references",
                "related_tests",
                "diagnostics",
            ],
            proof_bias: "edge_case_security_performance_risk_review",
        },
    }
}

fn candidate_builder_slot(
    primary_selected_slot: &str,
    requested_builder_slot: &str,
    worker_index: u32,
) -> String {
    candidate_builder_slot_from_slots(
        primary_selected_slot,
        requested_builder_slot,
        worker_index,
        &TurnClient::configured_builder_slots(),
    )
}

fn candidate_builder_slot_from_slots(
    primary_selected_slot: &str,
    requested_builder_slot: &str,
    worker_index: u32,
    configured_slots: &[String],
) -> String {
    let primary_selected_slot = primary_selected_slot.trim();
    if !requested_builder_slot.trim().is_empty() {
        return primary_selected_slot.to_string();
    }
    let mut slots = Vec::new();
    if !primary_selected_slot.is_empty() {
        slots.push(primary_selected_slot.to_string());
    }
    for slot in configured_slots {
        let slot = slot.trim();
        if slot.is_empty() || slots.iter().any(|existing| existing == slot) {
            continue;
        }
        slots.push(slot.to_string());
    }
    if slots.is_empty() {
        return primary_selected_slot.to_string();
    }
    let index = if primary_selected_slot.is_empty() {
        worker_index.saturating_sub(2) as usize
    } else {
        worker_index.saturating_sub(1) as usize
    };
    slots[index % slots.len()].clone()
}

#[allow(clippy::too_many_arguments)]
fn record_parallel_candidate_run_started(
    kernel: &Arc<RwLock<MdxKernel>>,
    resolved: &crate::request_security::ResolvedWriteIdentity,
    primary_request: &ForgeRunRequest,
    worker_index: u32,
    primary_run_id: &str,
    base_intent: &str,
    run_title: &str,
    repo_id_for_receipt: &str,
    repo_root: &Path,
    repo_profile: &crate::forge_repo_profile::ForgeRepoProfile,
    selected_checks_source: &str,
    selected_checks: &str,
    execution_geometry: &ExecutionGeometry,
    run_strategy: &mdx_core::ForgeRunStrategy,
    run_strategy_enabled: bool,
    external_run_authorization: Option<&crate::forge_fleet_eval_scoreboard::LiveTrialAuthorization>,
    builder_casting: &crate::forge_model_scorecard_route::BuilderCastingEvidence,
) -> Result<String, String> {
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let run_id = kernel.mint_id("forge_run");
    let work_item_id = format!("{}_candidate_{worker_index}", primary_request.work_item_id);
    let requested_workers = execution_geometry.requested_workers.to_string();
    let effective_workers = execution_geometry.effective_workers.to_string();
    let worker_index_string = worker_index.to_string();
    let worker_count_string = execution_geometry.effective_workers.to_string();
    let write_scope = primary_request.write_scope.join("\n");
    let strategy = candidate_strategy(worker_index, execution_geometry.effective_workers);
    let repo_profile_artifact_patterns = repo_profile.artifact_patterns.join(",");
    let repo_profile_semantic_intelligence = repo_profile.semantic_intelligence.join(",");
    let repo_profile_semantic_tool_readiness = repo_profile.semantic_tool_readiness.join(",");
    let semantic_session_source_file_count = "0".to_string();
    let semantic_session_indexed_file_count = "0".to_string();
    let semantic_session_indexed_symbol_count = "0".to_string();
    let semantic_session_related_test_anchor_count = "0".to_string();
    let repo_profile_toolchain_readiness = repo_profile.toolchain_readiness.join(",");
    let repo_root_string = repo_root.to_string_lossy().to_string();
    let required_semantic_operations = strategy.required_semantic_operations.join(",");
    let operator_intent = bounded_human_text(base_intent, 1200);
    let work_classification = classify_forge_work(base_intent, selected_checks);
    let work_classification_confidence_pct = work_classification.confidence_pct.to_string();
    let run_strategy_width = run_strategy.width.to_string();
    let run_strategy_max_turns = run_strategy.max_turns.to_string();
    let run_strategy_max_cost_cents = run_strategy.max_cost_cents.to_string();
    let run_strategy_max_runtime_ms = run_strategy.max_runtime_ms.to_string();
    let run_strategy_operator_locked_fields = run_strategy.operator_locked_fields.join(",");
    let run_strategy_policy_floor_fields = run_strategy.policy_floor_fields.join(",");
    let live_run_max_spend_cents = external_run_authorization
        .map(|value| value.max_spend_cents.to_string())
        .unwrap_or_else(|| "0".to_string());
    let live_run_max_tasks = external_run_authorization
        .map(|value| value.max_tasks.to_string())
        .unwrap_or_else(|| "0".to_string());
    let live_run_max_parallel_agents = external_run_authorization
        .map(|value| value.max_parallel_agents.to_string())
        .unwrap_or_else(|| "0".to_string());
    let live_run_task_ordinal = external_run_authorization
        .map(|value| {
            value
                .task_ordinal
                .saturating_add(worker_index - 1)
                .to_string()
        })
        .unwrap_or_else(|| "0".to_string());
    let (runner_id, runner_kind, runner_display_name, runner_adapter_kind, runner_execution_mode) =
        match run_strategy.harness_id.as_str() {
            "codex_cli" => (
                "codex_cli_external_worker",
                "external_harness",
                "Codex CLI",
                "codex_cli",
                "isolated_external_harness",
            ),
            "grok_build" => (
                "grok_build_cli_external_worker",
                "external_harness",
                "Grok Build",
                "grok_build_cli",
                "isolated_external_harness",
            ),
            _ => (
                "mdx_native_harness_runner",
                "mdx_native",
                "Forge native builder",
                "mdx_native_harness",
                "local_receipt_gated",
            ),
        };
    let language_task_alignment =
        align_language_task(repo_profile.language_pack_id, &work_classification);
    let language_task_human_timebox_minutes = language_task_alignment
        .task
        .as_ref()
        .map(language_task_human_timebox_minutes)
        .unwrap_or(0)
        .to_string();
    let language_task_corpus_id = language_task_alignment
        .task
        .as_ref()
        .map(|task| task.task_corpus_id)
        .unwrap_or("");
    let language_task_class = language_task_alignment
        .task
        .as_ref()
        .map(|task| task.task_class)
        .unwrap_or("");
    let language_task_complexity_tier = language_task_alignment
        .task
        .as_ref()
        .map(|task| task.complexity_tier)
        .unwrap_or("");
    let language_task_visible_check = language_task_alignment
        .task
        .as_ref()
        .map(|task| task.visible_check)
        .unwrap_or("");
    let language_task_hidden_check_slot = language_task_alignment
        .task
        .as_ref()
        .map(|task| task.hidden_check_slot)
        .unwrap_or("");
    let language_task_artifact_noise_expected = language_task_alignment
        .task
        .as_ref()
        .map(|task| task.artifact_noise_expected)
        .unwrap_or("");
    let language_task_required_principal_review_gates = language_task_alignment
        .task
        .as_ref()
        .map(|task| task.required_principal_review_gates)
        .unwrap_or("");
    let language_task_engineering_facets = language_task_alignment
        .task
        .as_ref()
        .map(language_task_engineering_facets)
        .unwrap_or("");
    let language_task_evaluation_oracle = language_task_alignment
        .task
        .as_ref()
        .map(language_task_evaluation_oracle)
        .unwrap_or("");
    let language_task_contamination_policy = language_task_alignment
        .task
        .as_ref()
        .map(language_task_contamination_policy)
        .unwrap_or("");
    let builder_casting_matching_eval_score_count =
        builder_casting.matching_eval_score_count.to_string();
    let builder_casting_accepted_eval_score_count =
        builder_casting.accepted_eval_score_count.to_string();
    let builder_casting_matching_run_count = builder_casting.matching_run_count.to_string();
    let builder_casting_done_rate_pct = builder_casting.done_rate_pct.to_string();
    let builder_casting_selected_model_class = crate::forge_run_stream::model_class(
        &builder_casting.selected_provider_family,
        &builder_casting.selected_builder_slot,
        &builder_casting.selected_model_id,
    );
    let builder_casting_requested_slot_matches_evidence =
        if builder_casting.requested_slot_matches_evidence {
            "true"
        } else {
            "false"
        };
    let hosted_environment_id = primary_request
        .check_target_dir
        .as_deref()
        .and_then(|value| value.strip_prefix("hosted://"));
    let evidence_fields = [
        ("repo_id", repo_id_for_receipt),
        (
            "execution_backend_kind",
            if hosted_environment_id.is_some() {
                "hosted_sandbox"
            } else {
                "local"
            },
        ),
        (
            "execution_target_kind",
            if hosted_environment_id.is_some() {
                "mdx_cloud"
            } else {
                "paired_host"
            },
        ),
        (
            "execution_target_id",
            hosted_environment_id.unwrap_or("local_host"),
        ),
        ("operator_intent", operator_intent.as_str()),
        ("operator_ask", operator_intent.as_str()),
        ("run_title", run_title),
        ("run_title_source", "operator_intent"),
        ("run_title_grants_execution_authority", "false"),
        ("repo_root", repo_root_string.as_str()),
        ("repo_primary_language", repo_profile.primary_language),
        ("language_pack_id", repo_profile.language_pack_id),
        ("selected_checks", selected_checks),
        ("selected_checks_source", selected_checks_source),
        (
            "work_classification_recommended_shape",
            &work_classification.recommended_shape,
        ),
        (
            "work_classification_task_class",
            &work_classification.task_class,
        ),
        (
            "work_classification_complexity_tier",
            &work_classification.complexity_tier,
        ),
        (
            "work_classification_confidence_pct",
            &work_classification_confidence_pct,
        ),
        (
            "work_classification_rationale",
            &work_classification.rationale,
        ),
        (
            "work_classification_source",
            "mdx_core::classify_forge_work",
        ),
        ("work_classification_grants_execution_authority", "false"),
        ("run_strategy_version", run_strategy.version),
        (
            "run_strategy_enabled",
            if run_strategy_enabled {
                "true"
            } else {
                "false"
            },
        ),
        ("run_strategy_mode", run_strategy.mode),
        (
            "run_strategy_outcome_preference",
            run_strategy.outcome_preference.as_str(),
        ),
        ("run_strategy_execution_shape", run_strategy.execution_shape),
        ("run_strategy_width", run_strategy_width.as_str()),
        ("run_strategy_harness_id", run_strategy.harness_id.as_str()),
        ("run_strategy_harness_source", run_strategy.harness_source),
        ("run_strategy_planner_mode", run_strategy.planner_mode),
        ("run_strategy_planner_source", run_strategy.planner_source),
        ("run_strategy_review_mode", run_strategy.review_mode),
        ("run_strategy_review_source", run_strategy.review_source),
        ("run_strategy_advisor_mode", run_strategy.advisor_mode),
        ("run_strategy_max_turns", run_strategy_max_turns.as_str()),
        (
            "run_strategy_max_cost_cents",
            run_strategy_max_cost_cents.as_str(),
        ),
        (
            "run_strategy_max_runtime_ms",
            run_strategy_max_runtime_ms.as_str(),
        ),
        (
            "run_strategy_operator_locked_fields",
            run_strategy_operator_locked_fields.as_str(),
        ),
        (
            "run_strategy_policy_floor_fields",
            run_strategy_policy_floor_fields.as_str(),
        ),
        ("run_strategy_rationale", run_strategy.rationale.as_str()),
        ("run_strategy_grants_execution_authority", "false"),
        (
            "live_run_approval_id",
            external_run_authorization
                .map(|value| value.approval_id.as_str())
                .unwrap_or(""),
        ),
        (
            "live_run_approval_receipt_id",
            external_run_authorization
                .map(|value| value.approval_receipt_id.as_str())
                .unwrap_or(""),
        ),
        (
            "runner_execution_clearance_receipt_id",
            external_run_authorization
                .map(|value| value.clearance_receipt_id.as_str())
                .unwrap_or(""),
        ),
        (
            "closed_runner_clearance_receipt_id",
            external_run_authorization
                .map(|value| value.clearance_receipt_id.as_str())
                .unwrap_or(""),
        ),
        (
            "live_execution_provider",
            external_run_authorization
                .map(|value| value.provider)
                .unwrap_or(""),
        ),
        (
            "live_run_max_spend_cents",
            live_run_max_spend_cents.as_str(),
        ),
        ("live_run_max_tasks", live_run_max_tasks.as_str()),
        (
            "live_run_max_parallel_agents",
            live_run_max_parallel_agents.as_str(),
        ),
        ("live_run_task_ordinal", live_run_task_ordinal.as_str()),
        (
            "live_execution_authority_scope",
            if external_run_authorization.is_some() {
                "single_governed_forge_run"
            } else {
                ""
            },
        ),
        (
            "external_harness_cost_enforcement",
            if external_run_authorization.is_some() {
                if run_strategy.harness_id == "grok_build" {
                    "approval_ceiling_plus_turn_and_runtime_bounds_usage_metering_unavailable"
                } else {
                    "approval_ceiling_plus_runtime_and_scope_bounds_usage_and_turn_metering_unavailable"
                }
            } else {
                "native_priced_token_meter"
            },
        ),
        ("language_task_corpus_id", language_task_corpus_id),
        (
            "language_task_alignment_status",
            language_task_alignment.status,
        ),
        (
            "language_task_alignment_source",
            "mdx_core::forge_language_task_corpus",
        ),
        ("language_task_class", language_task_class),
        (
            "language_task_complexity_tier",
            language_task_complexity_tier,
        ),
        ("language_task_visible_check", language_task_visible_check),
        (
            "language_task_hidden_check_slot",
            language_task_hidden_check_slot,
        ),
        (
            "language_task_artifact_noise_expected",
            language_task_artifact_noise_expected,
        ),
        (
            "language_task_required_principal_review_gates",
            language_task_required_principal_review_gates,
        ),
        (
            "language_task_engineering_facets",
            language_task_engineering_facets,
        ),
        (
            "language_task_evaluation_oracle",
            language_task_evaluation_oracle,
        ),
        (
            "language_task_human_timebox_minutes",
            &language_task_human_timebox_minutes,
        ),
        (
            "language_task_contamination_policy",
            language_task_contamination_policy,
        ),
        (
            "language_task_alignment_grants_execution_authority",
            "false",
        ),
        (
            "repo_profile_artifact_patterns",
            repo_profile_artifact_patterns.as_str(),
        ),
        (
            "repo_profile_semantic_intelligence",
            repo_profile_semantic_intelligence.as_str(),
        ),
        (
            "repo_profile_semantic_tool_readiness",
            repo_profile_semantic_tool_readiness.as_str(),
        ),
        ("repo_profile_semantic_session_id", "deferred_to_worker"),
        (
            "repo_profile_semantic_fallback_index_status",
            "deferred_to_worker",
        ),
        (
            "repo_profile_semantic_source_file_count",
            semantic_session_source_file_count.as_str(),
        ),
        (
            "repo_profile_semantic_indexed_file_count",
            semantic_session_indexed_file_count.as_str(),
        ),
        (
            "repo_profile_semantic_indexed_symbol_count",
            semantic_session_indexed_symbol_count.as_str(),
        ),
        (
            "repo_profile_semantic_related_test_anchor_count",
            semantic_session_related_test_anchor_count.as_str(),
        ),
        (
            "repo_profile_semantic_session_grants_execution_authority",
            "false",
        ),
        (
            "repo_profile_toolchain_readiness",
            repo_profile_toolchain_readiness.as_str(),
        ),
        (
            "repo_profile_proof_plan_status",
            repo_profile.proof_plan_status,
        ),
        (
            "repo_profile_proof_plan_summary",
            repo_profile.proof_plan_summary.as_str(),
        ),
        ("principal_orientation_gate_required", "true"),
        ("principal_orientation_gate_tool", "semantic_query"),
        ("principal_orientation_gate_grants_authority", "false"),
        ("parallel_candidate_primary_run_id", primary_run_id),
        ("parallel_candidate_index", worker_index_string.as_str()),
        ("parallel_candidate_count", worker_count_string.as_str()),
        ("parallel_candidate_write_scope", write_scope.as_str()),
        ("parallel_candidate_strategy_id", strategy.id),
        ("parallel_candidate_strategy_summary", strategy.summary),
        (
            "parallel_candidate_required_semantic_operations",
            required_semantic_operations.as_str(),
        ),
        ("parallel_candidate_proof_bias", strategy.proof_bias),
        ("builder_casting_status", builder_casting.status.as_str()),
        (
            "builder_casting_requested_slot",
            builder_casting.requested_builder_slot.as_str(),
        ),
        (
            "builder_casting_selected_slot",
            builder_casting.selected_builder_slot.as_str(),
        ),
        (
            "builder_casting_selected_model_profile_id",
            builder_casting.selected_model_profile_id.as_str(),
        ),
        (
            "builder_casting_selected_provider_family",
            builder_casting.selected_provider_family.as_str(),
        ),
        (
            "builder_casting_selected_model_id",
            builder_casting.selected_model_id.as_str(),
        ),
        (
            "builder_casting_selected_model_class",
            builder_casting_selected_model_class,
        ),
        (
            "builder_casting_recommended_slot",
            builder_casting.recommended_builder_slot.as_str(),
        ),
        (
            "builder_casting_recommended_model_profile_id",
            builder_casting.recommended_model_profile_id.as_str(),
        ),
        (
            "builder_casting_recommended_provider_family",
            builder_casting.recommended_provider_family.as_str(),
        ),
        (
            "builder_casting_recommended_model_id",
            builder_casting.recommended_model_id.as_str(),
        ),
        ("builder_casting_basis", builder_casting.basis.as_str()),
        (
            "builder_casting_matching_eval_score_count",
            builder_casting_matching_eval_score_count.as_str(),
        ),
        (
            "builder_casting_accepted_eval_score_count",
            builder_casting_accepted_eval_score_count.as_str(),
        ),
        (
            "builder_casting_matching_run_count",
            builder_casting_matching_run_count.as_str(),
        ),
        (
            "builder_casting_done_rate_pct",
            builder_casting_done_rate_pct.as_str(),
        ),
        (
            "builder_casting_requested_slot_matches_evidence",
            builder_casting_requested_slot_matches_evidence,
        ),
        ("builder_casting_grants_execution_authority", "false"),
        ("runner_profile_runner_id", runner_id),
        ("runner_profile_runner_kind", runner_kind),
        ("runner_profile_display_name", runner_display_name),
        ("runner_profile_adapter_kind", runner_adapter_kind),
        ("runner_profile_execution_mode", runner_execution_mode),
        ("runner_profile_grants_execution_authority", "false"),
        (
            "execution_geometry_requested_workers",
            requested_workers.as_str(),
        ),
        (
            "execution_geometry_effective_workers",
            effective_workers.as_str(),
        ),
        ("execution_geometry_lane", execution_geometry.lane),
        ("execution_geometry_route", execution_geometry.route),
        ("execution_geometry_reason", execution_geometry.reason),
        (
            "execution_geometry_fleet_required",
            execution_geometry.fleet_required_str(),
        ),
        ("execution_geometry_grants_execution_authority", "false"),
        ("mission_id", ""),
        ("mission_milestone_id", ""),
        ("mission_checkpoint_route", ""),
        ("mission_checkpoint_grants_execution_authority", "false"),
    ];
    let run_started_detail = format!(
        "parallel_candidate={worker_index}/{} primary_run_id={} language_pack={} intent_chars={}",
        execution_geometry.effective_workers,
        primary_run_id,
        repo_profile.language_pack_id,
        base_intent.chars().count()
    );
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            mdx_core::ForgeRunEvent {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                run_id: &run_id,
                event: "run_started",
                work_item_id: &work_item_id,
                detail: &run_started_detail,
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &resolved.identity,
            &evidence_fields,
        )
        .map_err(|error| error.message())?;
    drop(kernel);
    crate::forge_run_stream::publish_run_started(
        &resolved.tenant_id,
        &run_id,
        &report.receipt_id,
        &crate::forge_run_stream::RunStreamModel {
            provider_family: &builder_casting.selected_provider_family,
            model_id: &builder_casting.selected_model_id,
            slot: &builder_casting.selected_builder_slot,
            model_class: builder_casting_selected_model_class,
        },
        &run_started_detail,
    );
    Ok(run_id)
}

fn handle_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let (ledger_entries, model_configured) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        (
            kernel.ledger().entries().to_vec(),
            forge_model_configured(&kernel),
        )
    };
    // Fold the event chain into per-run summaries, newest run first. The
    // status is derived from the latest event, never stored.
    let mut runs: BTreeMap<String, RunFold> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for receipt in ledger_entries
        .iter()
        .filter(|receipt| receipt.kind == "forge.run.event")
    {
        let value = |key: &str| receipt.payload.get(key).map(String::as_str).unwrap_or("");
        let run_id = value("run_id").to_string();
        if run_id.is_empty() {
            continue;
        }
        if !value("cloud_record_kind").is_empty() {
            continue;
        }
        if !runs.contains_key(&run_id) {
            order.push(run_id.clone());
        }
        let fold = runs.entry(run_id).or_default();
        fold.event_count += 1;
        let event = value("event");
        let detail = value("detail");
        match event {
            "model_called" => {
                fold.model_calls += 1;
                fold.record_builder_model_call(detail);
                fold.record_model_context_call(detail, value("tokens_in"), value("tokens_out"));
            }
            "tool_executed" => fold.tool_calls += 1,
            "check_passed" => fold.checks_passed += 1,
            "check_failed" => fold.checks_failed += 1,
            _ => {}
        }
        if matches!(event, "check_passed" | "check_failed") {
            fold.check_duration_ms = fold
                .check_duration_ms
                .saturating_add(value("duration_ms").parse::<u64>().unwrap_or(0));
        }
        if let Ok(duration) = value("check_duration_ms").parse::<u64>() {
            fold.check_duration_ms = duration;
        }
        fold.latest_event = event.to_string();
        fold.latest_detail = detail.to_string();
        if !value("builder_loop_tick_id").trim().is_empty()
            || !value("builder_loop_tick_receipt_id").trim().is_empty()
            || detail.contains("builder_loop_tick=")
        {
            fold.origin = "system".to_string();
            fold.system_origin = "builder_loop_tick".to_string();
        }
        let authored_intent = first_non_empty(&[value("operator_intent"), value("operator_ask")]);
        if authored_intent.trim().is_empty()
            && fold.origin.trim().is_empty()
            && !actor_is_operator(receipt.actor_id.as_str())
        {
            fold.origin = "system".to_string();
            fold.system_origin = "forge_system".to_string();
        }
        if !value("work_item_id").trim().is_empty() {
            fold.work_item_id = value("work_item_id").to_string();
        }
        if let Ok(turn) = value("turn").parse::<u32>() {
            fold.turns = fold.turns.max(turn);
        }
        if fold.operator_intent.trim().is_empty() && !authored_intent.trim().is_empty() {
            fold.operator_intent = authored_intent.to_string();
            fold.intent_hint = fold.operator_intent.clone();
        }
        if fold.run_title.trim().is_empty() && !value("run_title").trim().is_empty() {
            fold.run_title = value("run_title").to_string();
        }
        if !value("forge_run_voice_profile_id").trim().is_empty() {
            fold.forge_run_voice_profile_id = value("forge_run_voice_profile_id").to_string();
        }
        if !value("operator_run_summary").trim().is_empty() {
            fold.operator_run_summary = value("operator_run_summary").to_string();
        }
        if !value("voice_rewrite_status").trim().is_empty() {
            fold.voice_rewrite_status = value("voice_rewrite_status").to_string();
        }
        if !value("voice_rewrite_model_id").trim().is_empty() {
            fold.voice_rewrite_model_id = value("voice_rewrite_model_id").to_string();
        }
        // The event trail is the run's activity feed - the run viewer
        // reads it the way Linear reads an agent session. Capped so a
        // long run's projection stays bounded.
        let (event_kind, event_stage) =
            crate::forge_run_stream::event_kind_and_stage(event, detail);
        let event_summary = crate::forge_run_stream::human_event_summary(event, detail);
        if fold.events.len() < 80 {
            fold.events.push(format!(
                r#"{{"id":{},"turn":{},"event":{},"detail":{},"stage":{},"kind":{},"summary":{},"receipt_id":{},"receipt_route":{}}}"#,
                json_string_literal(&receipt.receipt_id),
                value("turn").parse::<u32>().unwrap_or(0),
                json_string_literal(event),
                json_string_literal(detail),
                json_string_literal(event_stage),
                json_string_literal(event_kind),
                json_string_literal(&event_summary),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(&format!("/receipts/{}", receipt.receipt_id)),
            ));
        }
        if matches!(event, "check_passed" | "check_failed") {
            fold.proof_checks.insert(
                proof_check_name(detail),
                if event == "check_passed" {
                    "pass".to_string()
                } else {
                    "fail".to_string()
                },
            );
        }
        if event == "run_started" {
            let operator_intent = value("operator_intent");
            let operator_ask = value("operator_ask");
            let authored_operator_intent = if !operator_intent.trim().is_empty() {
                operator_intent.to_string()
            } else if !operator_ask.trim().is_empty() {
                operator_ask.to_string()
            } else {
                String::new()
            };
            fold.operator_intent = if !authored_operator_intent.trim().is_empty() {
                fold.origin = "operator".to_string();
                authored_operator_intent
            } else if fold.system_origin.trim().is_empty() {
                detail.to_string()
            } else {
                String::new()
            };
            fold.run_title = if !value("run_title").trim().is_empty() {
                value("run_title").to_string()
            } else if !fold.operator_intent.trim().is_empty() {
                run_title_from_intent(&fold.operator_intent)
            } else if !fold.system_origin.trim().is_empty() {
                system_run_title(fold).to_string()
            } else {
                run_title_from_intent(detail)
            };
            fold.intent_hint = fold.operator_intent.clone();
            fold.repo_id = value("repo_id").to_string();
            fold.repo_root = value("repo_root").to_string();
            fold.execution_backend_kind = value("execution_backend_kind").to_string();
            fold.cloud_environment_id = value("cloud_environment_id").to_string();
            fold.repo_primary_language = value("repo_primary_language").to_string();
            fold.language_pack_id = value("language_pack_id").to_string();
            fold.repo_profile_detected_language_packs =
                value("repo_profile_detected_language_packs").to_string();
            fold.repo_profile_detected_files = value("repo_profile_detected_files").to_string();
            fold.repo_profile_quality_signals = value("repo_profile_quality_signals").to_string();
            fold.repo_profile_standards_sources =
                value("repo_profile_standards_sources").to_string();
            fold.repo_profile_standards_source_fingerprints =
                value("repo_profile_standards_source_fingerprints").to_string();
            fold.repo_profile_standards_source_summaries =
                value("repo_profile_standards_source_summaries").to_string();
            fold.repo_profile_review_axes = value("repo_profile_review_axes").to_string();
            fold.repo_profile_principal_review_gates =
                value("repo_profile_principal_review_gates").to_string();
            fold.repo_profile_language_pack_guidance =
                value("repo_profile_language_pack_guidance").to_string();
            fold.repo_profile_semantic_intelligence =
                value("repo_profile_semantic_intelligence").to_string();
            fold.repo_profile_semantic_tool_readiness =
                value("repo_profile_semantic_tool_readiness").to_string();
            fold.repo_profile_semantic_session_id =
                value("repo_profile_semantic_session_id").to_string();
            fold.repo_profile_semantic_fallback_index_status =
                value("repo_profile_semantic_fallback_index_status").to_string();
            fold.repo_profile_semantic_source_file_count =
                value("repo_profile_semantic_source_file_count").to_string();
            fold.repo_profile_semantic_indexed_file_count =
                value("repo_profile_semantic_indexed_file_count").to_string();
            fold.repo_profile_semantic_indexed_symbol_count =
                value("repo_profile_semantic_indexed_symbol_count").to_string();
            fold.repo_profile_semantic_related_test_anchor_count =
                value("repo_profile_semantic_related_test_anchor_count").to_string();
            fold.repo_profile_semantic_session_grants_execution_authority =
                value("repo_profile_semantic_session_grants_execution_authority") == "true";
            fold.repo_profile_toolchain_readiness =
                value("repo_profile_toolchain_readiness").to_string();
            fold.repo_profile_suggested_checks = value("repo_profile_suggested_checks").to_string();
            fold.repo_profile_artifact_patterns =
                value("repo_profile_artifact_patterns").to_string();
            fold.repo_profile_proof_plan_status =
                value("repo_profile_proof_plan_status").to_string();
            fold.repo_profile_proof_plan_next_action =
                value("repo_profile_proof_plan_next_action").to_string();
            fold.repo_profile_proof_plan_summary =
                value("repo_profile_proof_plan_summary").to_string();
            fold.selected_checks = value("selected_checks").to_string();
            fold.selected_checks_source = value("selected_checks_source").to_string();
            fold.work_classification_recommended_shape =
                value("work_classification_recommended_shape").to_string();
            fold.work_classification_task_class =
                value("work_classification_task_class").to_string();
            fold.work_classification_complexity_tier =
                value("work_classification_complexity_tier").to_string();
            fold.work_classification_confidence_pct =
                value("work_classification_confidence_pct").to_string();
            fold.work_classification_rationale = value("work_classification_rationale").to_string();
            fold.work_classification_source = value("work_classification_source").to_string();
            fold.work_classification_grants_execution_authority =
                value("work_classification_grants_execution_authority") == "true";
            fold.run_strategy_json = run_strategy_receipt_json(receipt);
            fold.language_task_corpus_id = value("language_task_corpus_id").to_string();
            fold.language_task_alignment_status =
                value("language_task_alignment_status").to_string();
            fold.language_task_alignment_source =
                value("language_task_alignment_source").to_string();
            fold.language_task_class = value("language_task_class").to_string();
            fold.language_task_complexity_tier = value("language_task_complexity_tier").to_string();
            fold.language_task_visible_check = value("language_task_visible_check").to_string();
            fold.language_task_hidden_check_slot =
                value("language_task_hidden_check_slot").to_string();
            fold.language_task_artifact_noise_expected =
                value("language_task_artifact_noise_expected").to_string();
            fold.language_task_required_principal_review_gates =
                value("language_task_required_principal_review_gates").to_string();
            fold.language_task_engineering_facets =
                value("language_task_engineering_facets").to_string();
            fold.language_task_evaluation_oracle =
                value("language_task_evaluation_oracle").to_string();
            fold.language_task_human_timebox_minutes =
                value("language_task_human_timebox_minutes").to_string();
            fold.language_task_contamination_policy =
                value("language_task_contamination_policy").to_string();
            fold.language_task_alignment_grants_execution_authority =
                value("language_task_alignment_grants_execution_authority") == "true";
            fold.repo_profile_source = value("repo_profile_source").to_string();
            fold.repo_profile_grants_execution_authority =
                value("repo_profile_grants_execution_authority") == "true";
            fold.suggested_checks_are_authority = value("suggested_checks_are_authority") == "true";
            fold.principal_orientation_gate_required =
                value("principal_orientation_gate_required") == "true";
            fold.principal_orientation_gate_tool =
                value("principal_orientation_gate_tool").to_string();
            fold.principal_orientation_gate_grants_authority =
                value("principal_orientation_gate_grants_authority") == "true";
            fold.repo_intake_generated_from = value("repo_intake_generated_from").to_string();
            fold.repo_intake_readiness_status = value("repo_intake_readiness_status").to_string();
            fold.repo_intake_medium_high_work_ready =
                value("repo_intake_medium_high_work_ready") == "true";
            fold.repo_intake_safe_next_move = value("repo_intake_safe_next_move").to_string();
            fold.repo_intake_semantic_orientation_operations =
                value("repo_intake_semantic_orientation_operations").to_string();
            fold.repo_intake_proof_strategy = value("repo_intake_proof_strategy").to_string();
            fold.repo_intake_first_run_task_ids =
                value("repo_intake_first_run_task_ids").to_string();
            fold.repo_intake_scout_status = value("repo_intake_scout_status").to_string();
            fold.repo_intake_scout_candidate_count =
                value("repo_intake_scout_candidate_count").to_string();
            fold.repo_intake_scout_candidate_kinds =
                value("repo_intake_scout_candidate_kinds").to_string();
            fold.repo_intake_scout_candidate_paths =
                value("repo_intake_scout_candidate_paths").to_string();
            fold.repo_intake_write_scope_hint = value("repo_intake_write_scope_hint").to_string();
            fold.repo_intake_off_limits_patterns =
                value("repo_intake_off_limits_patterns").to_string();
            fold.repo_intake_review_focus = value("repo_intake_review_focus").to_string();
            fold.repo_intake_source_host = value("repo_intake_source_host").to_string();
            fold.repo_intake_origin_url_present = value("repo_intake_origin_url_present") == "true";
            fold.repo_intake_origin_url_recorded =
                value("repo_intake_origin_url_recorded") == "true";
            fold.repo_intake_source_host_readiness_route =
                value("repo_intake_source_host_readiness_route").to_string();
            fold.repo_intake_source_host_pr_draft_route =
                value("repo_intake_source_host_pr_draft_route").to_string();
            fold.repo_intake_provider_calls_allowed =
                value("repo_intake_provider_calls_allowed") == "true";
            fold.repo_intake_run_started = value("repo_intake_run_started") == "true";
            fold.repo_intake_network_call_allowed =
                value("repo_intake_network_call_allowed") == "true";
            fold.repo_intake_production_write_allowed =
                value("repo_intake_production_write_allowed") == "true";
            fold.repo_intake_grants_execution_authority =
                value("repo_intake_grants_execution_authority") == "true";
            fold.execution_geometry_requested_workers =
                value("execution_geometry_requested_workers").to_string();
            fold.execution_geometry_effective_workers =
                value("execution_geometry_effective_workers").to_string();
            fold.execution_geometry_lane = value("execution_geometry_lane").to_string();
            fold.execution_geometry_route = value("execution_geometry_route").to_string();
            fold.execution_geometry_reason = value("execution_geometry_reason").to_string();
            fold.execution_geometry_fleet_required =
                value("execution_geometry_fleet_required") == "true";
            fold.execution_geometry_grants_execution_authority =
                value("execution_geometry_grants_execution_authority") == "true";
            fold.parallel_candidate_primary_run_id =
                value("parallel_candidate_primary_run_id").to_string();
            fold.parallel_candidate_index = value("parallel_candidate_index").to_string();
            fold.parallel_candidate_count = value("parallel_candidate_count").to_string();
            fold.parallel_candidate_write_scope =
                value("parallel_candidate_write_scope").to_string();
            fold.parallel_candidate_strategy_id =
                value("parallel_candidate_strategy_id").to_string();
            fold.parallel_candidate_strategy_summary =
                value("parallel_candidate_strategy_summary").to_string();
            fold.parallel_candidate_required_semantic_operations =
                value("parallel_candidate_required_semantic_operations").to_string();
            fold.parallel_candidate_proof_bias = value("parallel_candidate_proof_bias").to_string();
            fold.builder_casting_status = value("builder_casting_status").to_string();
            fold.builder_casting_requested_slot =
                value("builder_casting_requested_slot").to_string();
            fold.builder_casting_selected_slot = value("builder_casting_selected_slot").to_string();
            fold.builder_casting_selected_model_profile_id =
                value("builder_casting_selected_model_profile_id").to_string();
            fold.builder_casting_selected_provider_family =
                value("builder_casting_selected_provider_family").to_string();
            fold.builder_casting_selected_model_id =
                value("builder_casting_selected_model_id").to_string();
            fold.builder_casting_recommended_slot =
                value("builder_casting_recommended_slot").to_string();
            fold.builder_casting_recommended_model_profile_id =
                value("builder_casting_recommended_model_profile_id").to_string();
            fold.builder_casting_recommended_provider_family =
                value("builder_casting_recommended_provider_family").to_string();
            fold.builder_casting_recommended_model_id =
                value("builder_casting_recommended_model_id").to_string();
            fold.builder_casting_basis = value("builder_casting_basis").to_string();
            fold.builder_casting_matching_eval_score_count =
                value("builder_casting_matching_eval_score_count").to_string();
            fold.builder_casting_accepted_eval_score_count =
                value("builder_casting_accepted_eval_score_count").to_string();
            fold.builder_casting_matching_run_count =
                value("builder_casting_matching_run_count").to_string();
            fold.builder_casting_done_rate_pct = value("builder_casting_done_rate_pct").to_string();
            fold.builder_casting_requested_slot_matches_evidence =
                value("builder_casting_requested_slot_matches_evidence") == "true";
            fold.builder_casting_grants_execution_authority =
                value("builder_casting_grants_execution_authority") == "true";
            fold.machine_league_trial = value("machine_league_trial") == "true";
            fold.machine_league_run_kind = value("machine_league_run_kind").to_string();
            fold.runner_profile_runner_id = value("runner_profile_runner_id").to_string();
            fold.runner_profile_runner_kind = value("runner_profile_runner_kind").to_string();
            fold.runner_profile_display_name = value("runner_profile_display_name").to_string();
            fold.runner_profile_adapter_kind = value("runner_profile_adapter_kind").to_string();
            fold.runner_profile_execution_mode = value("runner_profile_execution_mode").to_string();
            fold.runner_profile_model_profile_id =
                value("runner_profile_model_profile_id").to_string();
            fold.machine_runtime_fingerprint_id =
                value("machine_runtime_fingerprint_id").to_string();
            fold.machine_runtime_runner_id = value("machine_runtime_runner_id").to_string();
            fold.machine_runtime_adapter_kind = value("machine_runtime_adapter_kind").to_string();
            fold.machine_runtime_binary_name = value("machine_runtime_binary_name").to_string();
            fold.machine_runtime_binary_path = value("machine_runtime_binary_path").to_string();
            fold.machine_runtime_binary_present = value("machine_runtime_binary_present") == "true";
            fold.machine_runtime_version_command =
                value("machine_runtime_version_command").to_string();
            fold.machine_runtime_version_observed =
                value("machine_runtime_version_observed").to_string();
            fold.machine_runtime_version_raw_output =
                value("machine_runtime_version_raw_output").to_string();
            fold.machine_runtime_checksum_sha256 =
                value("machine_runtime_checksum_sha256").to_string();
            fold.machine_runtime_adapter_contract_version =
                value("machine_runtime_adapter_contract_version").to_string();
            fold.machine_runtime_command_contract =
                value("machine_runtime_command_contract").to_string();
            fold.machine_runtime_drift_status = value("machine_runtime_drift_status").to_string();
            fold.machine_runtime_last_fingerprint_id =
                value("machine_runtime_last_fingerprint_id").to_string();
            fold.machine_runtime_compatibility_status =
                value("machine_runtime_compatibility_status").to_string();
            fold.quarantine_status = value("quarantine_status").to_string();
            fold.quarantine_output_quarantined = value("quarantine_output_quarantined") == "true";
            fold.quarantine_external_output_consumable =
                value("quarantine_external_output_consumable") == "true";
            fold.quarantine_acceptance_gate = value("quarantine_acceptance_gate").to_string();
            fold.quarantine_result_projection_route =
                value("quarantine_result_projection_route").to_string();
            fold.quarantine_blocked_reason = value("quarantine_blocked_reason").to_string();
            fold.league_context_visibility_tier =
                value("league_context_visibility_tier").to_string();
            fold.league_context_recommendation_rationale =
                value("league_context_recommendation_rationale").to_string();
            fold.league_context_fallback_runner_id =
                value("league_context_fallback_runner_id").to_string();
            fold.league_context_scorecard_evidence_count =
                value("league_context_scorecard_evidence_count").to_string();
            fold.league_context_quarantine_posture =
                value("league_context_quarantine_posture").to_string();
            fold.eval_principal_review_status = value("eval_principal_review_status").to_string();
            fold.eval_result_receipt_id = value("eval_result_receipt_id").to_string();
            fold.accepted_for_scoreboard = value("accepted_for_scoreboard") == "true";
            fold.scorecard_total_score = value("total_score").to_string();
            fold.mission_id = value("mission_id").to_string();
            fold.mission_milestone_id = value("mission_milestone_id").to_string();
            fold.mission_checkpoint_route = value("mission_checkpoint_route").to_string();
            fold.mission_checkpoint_grants_execution_authority =
                value("mission_checkpoint_grants_execution_authority") == "true";
        }
        if !value("eval_principal_review_status").trim().is_empty()
            || value("accepted_for_scoreboard") == "true"
        {
            fold.eval_principal_review_status = value("eval_principal_review_status").to_string();
            fold.eval_result_receipt_id = value("eval_result_receipt_id").to_string();
            fold.accepted_for_scoreboard = value("accepted_for_scoreboard") == "true";
            fold.scorecard_total_score = value("total_score").to_string();
        }
        if event == "evidence_appended" && detail.starts_with("branch=") {
            fold.branch = detail
                .trim_start_matches("branch=")
                .split(' ')
                .next()
                .unwrap_or("")
                .to_string();
            if let Some(sha) = detail
                .split_whitespace()
                .find_map(|part| part.strip_prefix("sha="))
                && !sha.is_empty()
            {
                fold.branch_sha = sha.to_string();
            }
            if let Some(paths) = detail
                .split_whitespace()
                .find_map(|part| part.strip_prefix("paths="))
                && !paths.trim().is_empty()
            {
                fold.changed_paths = paths.to_string();
            }
        }
        if event == "run_finished" {
            fold.finished = true;
            fold.recorded_conclusion = value("terminal_state").eq_ignore_ascii_case("RECORDED");
            fold.final_line = detail.to_string();
            if fold.branch.trim().is_empty()
                && let Some(branch) = detail
                    .split_whitespace()
                    .find_map(|part| part.strip_prefix("branch="))
                && !branch.is_empty()
            {
                fold.branch = branch.to_string();
            }
            if !value("run_summary").trim().is_empty() {
                fold.run_summary = value("run_summary").to_string();
            }
        }
        if event == "run_summary" && !detail.trim().is_empty() {
            fold.run_summary = detail.to_string();
        }
    }
    for receipt in ledger_entries
        .iter()
        .filter(|receipt| receipt.kind == "forge.run.control")
    {
        let value = |key: &str| receipt.payload.get(key).map(String::as_str).unwrap_or("");
        let run_id = value("run_id");
        if run_id.is_empty() || value("control") != "stop" {
            continue;
        }
        let Some(fold) = runs.get_mut(run_id) else {
            continue;
        };
        if fold.finished {
            continue;
        }
        let note = first_non_empty(&[value("note"), "stopped by operator control"]);
        fold.finished = true;
        fold.latest_event = "operator_stop".to_string();
        fold.latest_detail = note.to_string();
        fold.final_line = format!(
            "status=RUN_STOPPED_BY_OPERATOR control_receipt_id={} note={}",
            receipt.receipt_id, note
        );
    }
    for fold in runs.values_mut() {
        if fold.origin.trim().is_empty() {
            fold.origin = if fold.operator_intent.trim().is_empty() {
                "system".to_string()
            } else {
                "operator".to_string()
            };
        }
        if fold.system_origin.trim().is_empty() && fold.origin == "system" {
            fold.system_origin = "forge_system".to_string();
        }
        if fold.operator_intent.trim().is_empty() && fold.system_origin.trim().is_empty() {
            fold.operator_intent =
                first_non_empty(&[&fold.intent_hint, &fold.latest_detail]).to_string();
        }
        if fold.intent_hint.trim().is_empty() {
            fold.intent_hint = first_non_empty(&[
                &fold.operator_intent,
                system_run_title(fold),
                &fold.latest_detail,
            ])
            .to_string();
        }
        let legacy_title_was_clipped = fold.run_title.chars().count() == 140
            && !fold.operator_intent.trim().is_empty()
            && fold.operator_intent.starts_with(&fold.run_title);
        if fold.run_title.trim().is_empty() || legacy_title_was_clipped {
            fold.run_title = run_title_from_intent(first_non_empty(&[
                &fold.operator_intent,
                system_run_title(fold),
                &fold.latest_detail,
            ]));
        }
    }
    let mut outcomes: BTreeMap<String, RunOutcomeFold> = BTreeMap::new();
    for receipt in ledger_entries
        .iter()
        .filter(|receipt| receipt.kind == "forge.outcome.signal.recorded")
    {
        let value = |key: &str| receipt.payload.get(key).map(String::as_str).unwrap_or("");
        let run_id = value("run_id");
        if run_id.is_empty() {
            continue;
        }
        outcomes.insert(
            run_id.to_string(),
            RunOutcomeFold {
                receipt_id: receipt.receipt_id.clone(),
                disposition: value("disposition").to_string(),
                lesson_candidate: value("lesson_candidate").to_string(),
            },
        );
    }
    let (parallel_execution_group_count, parallel_execution_groups) =
        parallel_execution_groups_json(&runs, &order);
    let entries: Vec<String> = order
        .iter()
        .rev()
        .filter_map(|run_id| {
            let fold = &runs[run_id];
            if fold.hidden_from_run_list() {
                return None;
            }
            let outcome = outcomes.get(run_id);
            let controls = controls_json(fold);
            let allowed_controls = allowed_controls_json(fold);
            Some(format!(
                r#"{{"run_id":{},"work_item_id":{},"status":{},"terminal_state":{},"origin":{},"system_origin":{},"latest_event":{},"latest_detail":{},"turns":{},"model_calls":{},"tool_calls":{},"checks_passed":{},"checks_failed":{},"check_duration_ms":{},"event_count":{},"finished":{},"branch":{},"stream_route":{},"intent":{},"operator_intent":{},"title":{},"run_title":{},"run_summary":{},"operator_run_summary":{},"forge_run_voice_profile_id":{},"voice_rewrite_status":{},"voice_rewrite_model_id":{},"repo":{},"model_or_worker":{},"runner_profile":{},"machine_runtime":{},"quarantine":{},"league_context":{},"stages":[{}],"operator_status":{},"proof":{},"diff":{},"controls":[{}],"allowed_controls":[{}],"final_line":{},"repo_id":{},"execution_backend_kind":{},"cloud_environment_id":{},"repo_primary_language":{},"language_pack_id":{},"repo_profile_detected_language_packs":[{}],"repo_profile_detected_files":[{}],"repo_profile_quality_signals":[{}],"repo_profile_standards_sources":[{}],"repo_profile_standards_source_fingerprints":[{}],"repo_profile_standards_source_summaries":[{}],"repo_profile_review_axes":[{}],"repo_profile_principal_review_gates":[{}],"repo_profile_language_pack_guidance":[{}],"repo_profile_semantic_intelligence":[{}],"repo_profile_semantic_tool_readiness":[{}],"repo_profile_semantic_session":{{"session_id":{},"fallback_index_status":{},"source_file_count":{},"indexed_file_count":{},"indexed_symbol_count":{},"related_test_anchor_count":{},"grants_execution_authority":{}}},"repo_profile_toolchain_readiness":[{}],"repo_profile_suggested_checks":[{}],"repo_profile_artifact_patterns":[{}],"repo_profile_proof_plan":{{"status":{},"next_action":{},"summary":{}}},"selected_checks":[{}],"selected_checks_source":{},"work_classification":{{"recommended_shape":{},"task_class":{},"complexity_tier":{},"confidence_pct":{},"rationale":{},"source":{},"grants_execution_authority":{}}},"language_task_alignment":{{"task_corpus_id":{},"status":{},"source":{},"task_class":{},"complexity_tier":{},"visible_check":{},"hidden_check_slot":{},"artifact_noise_expected":[{}],"required_principal_review_gates":[{}],"engineering_facets":{},"evaluation_oracle":{},"human_timebox_minutes":{},"contamination_policy":{},"grants_execution_authority":{}}},"repo_profile_source":{},"repo_profile_grants_execution_authority":{},"suggested_checks_are_authority":{},"principal_orientation_gate":{{"required":{},"tool":{},"grants_authority":{}}},"repo_intake":{{"generated_from":{},"readiness_status":{},"medium_high_work_ready":{},"safe_next_move":{},"semantic_orientation_operations":[{}],"proof_strategy":{},"first_run_task_ids":[{}],"scout_status":{},"scout_candidate_count":{},"scout_candidate_kinds":[{}],"scout_candidate_paths":[{}],"write_scope_hint":[{}],"off_limits_patterns":[{}],"review_focus":[{}],"source_host":{},"origin_url_present":{},"origin_url_recorded":{},"source_host_readiness_route":{},"source_host_pr_draft_route":{},"provider_calls_allowed":{},"run_started":{},"network_call_allowed":{},"production_write_allowed":{},"grants_execution_authority":{}}},"execution_geometry":{{"requested_workers":{},"effective_workers":{},"lane":{},"route":{},"reason":{},"fleet_required":{},"grants_execution_authority":{}}},"mission":{{"mission_id":{},"milestone_id":{},"checkpoint_route":{},"checkpoint_grants_execution_authority":{}}},"parallel_candidate":{{"role":{},"primary_run_id":{},"index":{},"count":{},"write_scope":[{}],"strategy_id":{},"strategy_summary":{},"required_semantic_operations":[{}],"proof_bias":{},"grants_execution_authority":false}},"builder_casting":{{"status":{},"requested_slot":{},"selected_slot":{},"selected_model_profile_id":{},"selected_provider_family":{},"selected_model_id":{},"recommended_slot":{},"recommended_model_profile_id":{},"recommended_provider_family":{},"recommended_model_id":{},"basis":{},"matching_eval_score_count":{},"accepted_eval_score_count":{},"matching_run_count":{},"done_rate_pct":{},"requested_slot_matches_evidence":{},"grants_execution_authority":{}}},"outcome_signal_receipt_id":{},"outcome_disposition":{},"lesson_candidate":{},"events":[{}],"latest_input_tokens":{},"context_telemetry":{}}}"#,
                json_string_literal(run_id),
                json_string_literal(&fold.work_item_id),
                json_string_literal(fold.derived_status()),
                json_string_literal(fold.terminal_state()),
                json_string_literal(&fold.origin),
                json_string_literal(&fold.system_origin),
                json_string_literal(&fold.latest_event),
                json_string_literal(&fold.latest_detail),
                fold.turns,
                fold.model_calls,
                fold.tool_calls,
                fold.checks_passed,
                fold.checks_failed,
                fold.check_duration_ms,
                fold.event_count,
                fold.finished,
                json_string_literal(&fold.branch),
                json_string_literal(&forge_run_stream_route(run_id)),
                json_string_literal(&fold.intent_hint),
                json_string_literal(&fold.operator_intent),
                json_string_literal(&fold.run_title),
                json_string_literal(&fold.run_title),
                json_string_literal(&fold.run_summary),
                json_string_literal(&fold.operator_run_summary),
                json_string_literal(&fold.forge_run_voice_profile_id),
                json_string_literal(&fold.voice_rewrite_status),
                json_string_literal(&fold.voice_rewrite_model_id),
                repo_json(fold),
                model_or_worker_json(fold),
                runner_profile_json(fold),
                machine_runtime_json(fold),
                quarantine_json(fold),
                league_context_json(fold),
                live_stage_json(fold),
                json_string_literal(operator_status(fold)),
                proof_json(fold),
                diff_json(fold),
                controls,
                allowed_controls,
                json_string_literal(&fold.final_line),
                json_string_literal(&fold.repo_id),
                json_string_literal(&fold.execution_backend_kind),
                json_string_literal(&fold.cloud_environment_id),
                json_string_literal(&fold.repo_primary_language),
                json_string_literal(&fold.language_pack_id),
                csv_json_array(&fold.repo_profile_detected_language_packs),
                csv_json_array(&fold.repo_profile_detected_files),
                csv_json_array(&fold.repo_profile_quality_signals),
                csv_json_array(&fold.repo_profile_standards_sources),
                csv_json_array(&fold.repo_profile_standards_source_fingerprints),
                csv_json_array(&fold.repo_profile_standards_source_summaries),
                csv_json_array(&fold.repo_profile_review_axes),
                csv_json_array(&fold.repo_profile_principal_review_gates),
                csv_json_array(&fold.repo_profile_language_pack_guidance),
                csv_json_array(&fold.repo_profile_semantic_intelligence),
                csv_json_array(&fold.repo_profile_semantic_tool_readiness),
                json_string_literal(&fold.repo_profile_semantic_session_id),
                json_string_literal(&fold.repo_profile_semantic_fallback_index_status),
                fold.repo_profile_semantic_source_file_count
                    .parse::<u32>()
                    .unwrap_or(0),
                fold.repo_profile_semantic_indexed_file_count
                    .parse::<u32>()
                    .unwrap_or(0),
                fold.repo_profile_semantic_indexed_symbol_count
                    .parse::<u32>()
                    .unwrap_or(0),
                fold.repo_profile_semantic_related_test_anchor_count
                    .parse::<u32>()
                    .unwrap_or(0),
                fold.repo_profile_semantic_session_grants_execution_authority,
                csv_json_array(&fold.repo_profile_toolchain_readiness),
                csv_json_array(&fold.repo_profile_suggested_checks),
                csv_json_array(&fold.repo_profile_artifact_patterns),
                json_string_literal(&fold.repo_profile_proof_plan_status),
                json_string_literal(&fold.repo_profile_proof_plan_next_action),
                json_string_literal(&fold.repo_profile_proof_plan_summary),
                csv_json_array(&fold.selected_checks),
                json_string_literal(selected_checks_source(&fold.selected_checks_source)),
                json_string_literal(&fold.work_classification_recommended_shape),
                json_string_literal(&fold.work_classification_task_class),
                json_string_literal(&fold.work_classification_complexity_tier),
                fold.work_classification_confidence_pct
                    .parse::<u32>()
                    .unwrap_or(0),
                json_string_literal(&fold.work_classification_rationale),
                json_string_literal(&fold.work_classification_source),
                fold.work_classification_grants_execution_authority,
                json_string_literal(&fold.language_task_corpus_id),
                json_string_literal(&fold.language_task_alignment_status),
                json_string_literal(&fold.language_task_alignment_source),
                json_string_literal(&fold.language_task_class),
                json_string_literal(&fold.language_task_complexity_tier),
                json_string_literal(&fold.language_task_visible_check),
                json_string_literal(&fold.language_task_hidden_check_slot),
                csv_json_array(&fold.language_task_artifact_noise_expected),
                csv_json_array(&fold.language_task_required_principal_review_gates),
                json_string_literal(&fold.language_task_engineering_facets),
                json_string_literal(&fold.language_task_evaluation_oracle),
                fold.language_task_human_timebox_minutes
                    .parse::<u32>()
                    .unwrap_or(0),
                json_string_literal(&fold.language_task_contamination_policy),
                fold.language_task_alignment_grants_execution_authority,
                json_string_literal(&fold.repo_profile_source),
                fold.repo_profile_grants_execution_authority,
                fold.suggested_checks_are_authority,
                fold.principal_orientation_gate_required,
                json_string_literal(&fold.principal_orientation_gate_tool),
                fold.principal_orientation_gate_grants_authority,
                json_string_literal(&fold.repo_intake_generated_from),
                json_string_literal(&fold.repo_intake_readiness_status),
                fold.repo_intake_medium_high_work_ready,
                json_string_literal(&fold.repo_intake_safe_next_move),
                csv_json_array(&fold.repo_intake_semantic_orientation_operations),
                json_string_literal(&fold.repo_intake_proof_strategy),
                csv_json_array(&fold.repo_intake_first_run_task_ids),
                json_string_literal(&fold.repo_intake_scout_status),
                fold.repo_intake_scout_candidate_count
                    .parse::<u32>()
                    .unwrap_or(0),
                csv_json_array(&fold.repo_intake_scout_candidate_kinds),
                csv_json_array(&fold.repo_intake_scout_candidate_paths),
                csv_json_array(&fold.repo_intake_write_scope_hint),
                csv_json_array(&fold.repo_intake_off_limits_patterns),
                csv_json_array(&fold.repo_intake_review_focus),
                json_string_literal(&fold.repo_intake_source_host),
                fold.repo_intake_origin_url_present,
                fold.repo_intake_origin_url_recorded,
                json_string_literal(&fold.repo_intake_source_host_readiness_route),
                json_string_literal(&fold.repo_intake_source_host_pr_draft_route),
                fold.repo_intake_provider_calls_allowed,
                fold.repo_intake_run_started,
                fold.repo_intake_network_call_allowed,
                fold.repo_intake_production_write_allowed,
                fold.repo_intake_grants_execution_authority,
                fold
                    .execution_geometry_requested_workers
                    .parse::<u32>()
                    .unwrap_or(1),
                fold
                    .execution_geometry_effective_workers
                    .parse::<u32>()
                    .unwrap_or(1),
                json_string_literal(execution_geometry_lane(&fold.execution_geometry_lane)),
                json_string_literal(execution_geometry_route(&fold.execution_geometry_route)),
                json_string_literal(execution_geometry_reason(&fold.execution_geometry_reason)),
                fold.execution_geometry_fleet_required,
                fold.execution_geometry_grants_execution_authority,
                json_string_literal(&fold.mission_id),
                json_string_literal(&fold.mission_milestone_id),
                json_string_literal(&fold.mission_checkpoint_route),
                fold.mission_checkpoint_grants_execution_authority,
                json_string_literal(fold.parallel_candidate_role(run_id)),
                json_string_literal(fold.parallel_candidate_primary(run_id)),
                fold.parallel_candidate_index.parse::<u32>().unwrap_or(1),
                fold.parallel_candidate_count.parse::<u32>().unwrap_or(1),
                newline_json_array(&fold.parallel_candidate_write_scope),
                json_string_literal(parallel_candidate_strategy_id(
                    &fold.parallel_candidate_strategy_id,
                    fold.parallel_candidate_index.parse::<u32>().unwrap_or(1),
                    fold.parallel_candidate_count.parse::<u32>().unwrap_or(1),
                )),
                json_string_literal(parallel_candidate_strategy_summary(
                    &fold.parallel_candidate_strategy_summary,
                    fold.parallel_candidate_index.parse::<u32>().unwrap_or(1),
                    fold.parallel_candidate_count.parse::<u32>().unwrap_or(1),
                )),
                csv_json_array(&parallel_candidate_required_semantic_operations(
                    &fold.parallel_candidate_required_semantic_operations,
                    fold.parallel_candidate_index.parse::<u32>().unwrap_or(1),
                    fold.parallel_candidate_count.parse::<u32>().unwrap_or(1),
                )),
                json_string_literal(parallel_candidate_proof_bias(
                    &fold.parallel_candidate_proof_bias,
                    fold.parallel_candidate_index.parse::<u32>().unwrap_or(1),
                    fold.parallel_candidate_count.parse::<u32>().unwrap_or(1),
                )),
                json_string_literal(&fold.builder_casting_status),
                json_string_literal(&fold.builder_casting_requested_slot),
                json_string_literal(&fold.builder_casting_selected_slot),
                json_string_literal(&fold.builder_casting_selected_model_profile_id),
                json_string_literal(&fold.builder_casting_selected_provider_family),
                json_string_literal(&fold.builder_casting_selected_model_id),
                json_string_literal(&fold.builder_casting_recommended_slot),
                json_string_literal(&fold.builder_casting_recommended_model_profile_id),
                json_string_literal(&fold.builder_casting_recommended_provider_family),
                json_string_literal(&fold.builder_casting_recommended_model_id),
                json_string_literal(&fold.builder_casting_basis),
                fold.builder_casting_matching_eval_score_count
                    .parse::<u32>()
                    .unwrap_or(0),
                fold.builder_casting_accepted_eval_score_count
                    .parse::<u32>()
                    .unwrap_or(0),
                fold.builder_casting_matching_run_count
                    .parse::<u32>()
                    .unwrap_or(0),
                fold.builder_casting_done_rate_pct
                    .parse::<u32>()
                    .unwrap_or(0),
                fold.builder_casting_requested_slot_matches_evidence,
                fold.builder_casting_grants_execution_authority,
                json_string_literal(outcome.map(|item| item.receipt_id.as_str()).unwrap_or("")),
                json_string_literal(outcome.map(|item| item.disposition.as_str()).unwrap_or("")),
                json_string_literal(outcome.map(|item| item.lesson_candidate.as_str()).unwrap_or("")),
                fold.events.join(","),
                fold.latest_input_tokens,
                context_telemetry_json(fold),
            ))
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-run-local-projection","receipt_kind":"forge.run.event","outcome_receipt_kind":"forge.outcome.signal.recorded","run_count":{},"parallel_execution_group_count":{},"parallel_execution_groups":[{}],"runs":[{}],"local_real":{{"loop":"forge_loop_runner","worktree_isolated":true,"live_repo_mutated":false,"model_configured":{},"host_checks_enabled":{},"sandbox_tests_enabled":{},"patch_apply_enabled":{},"untrusted_capability_execution_requires_stronger_isolation":true,"deployment_allowed":false}},"production_write_allowed":false}}"#,
            entries.len(),
            parallel_execution_group_count,
            parallel_execution_groups,
            entries.join(","),
            model_configured,
            forge_host_checks_enabled(),
            env_enabled("MDX_PLAN_TEST_EXEC"),
            env_enabled("MDX_PLAN_PATCH_APPLY"),
        ),
    ))
}

struct ResolvedRunRepo {
    root: String,
    origin_url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct MissionAttachment {
    mission_id: String,
    milestone_id: String,
}

impl MissionAttachment {
    fn from_body(body: &str) -> Self {
        Self {
            mission_id: json_string_field(body, "mission_id").unwrap_or_default(),
            milestone_id: json_string_field(body, "mission_milestone_id")
                .or_else(|| json_string_field(body, "milestone_id"))
                .unwrap_or_default(),
        }
    }

    fn checkpoint_route(&self) -> &'static str {
        if self.mission_id.trim().is_empty() {
            ""
        } else {
            "/forge/long-horizon-mission-checkpoints.json"
        }
    }

    fn validate(&self, kernel: &MdxKernel) -> Result<(), String> {
        if self.mission_id.trim().is_empty() {
            return Ok(());
        }
        if self.milestone_id.trim().is_empty() {
            return Err(
                "mission-attached runs need mission_milestone_id so Forge can checkpoint the right milestone".to_string(),
            );
        }
        let dashboard = kernel.project_forge_long_horizon_missions();
        let Some(packet) = dashboard
            .packets
            .iter()
            .find(|packet| packet.mission_id == self.mission_id)
        else {
            return Err(format!(
                "mission {} is not admitted - create it before starting an attached run",
                self.mission_id
            ));
        };
        if !packet
            .milestones
            .iter()
            .any(|milestone| milestone.milestone_id == self.milestone_id)
        {
            return Err(format!(
                "mission milestone {} is not part of mission {}",
                self.milestone_id, self.mission_id
            ));
        }
        Ok(())
    }
}

fn execution_geometry_from_body(body: &str) -> Result<ExecutionGeometry, String> {
    let requested_workers = json_u32_field(body, "fleet_width")
        .or_else(|| json_u32_field(body, "worker_count"))
        .or_else(|| json_u32_field(body, "requested_workers"))
        .unwrap_or(1);
    execution_geometry_for_width(requested_workers)
}

fn execution_geometry_for_width(requested_workers: u32) -> Result<ExecutionGeometry, String> {
    forge_execution_geometry_for_width(requested_workers)
}

fn resolve_run_repo(kernel: &MdxKernel, repo_id: &str) -> Result<ResolvedRunRepo, String> {
    let mut latest: Option<std::collections::BTreeMap<String, String>> = None;
    let repo_id = repo_id.trim();
    if repo_id.is_empty() {
        return Ok(ResolvedRunRepo {
            root: repo_root().to_string_lossy().to_string(),
            origin_url: String::new(),
        });
    }
    if !repo_id.is_empty() {
        for receipt in kernel
            .ledger()
            .query()
            .by_kind("forge.repo.connected")
            .iter()
        {
            if receipt.payload.get("repo_id").map(String::as_str) == Some(repo_id) {
                latest = Some(receipt.payload.clone());
            }
        }
    }
    if let Some(fields) = latest
        && let Some(root) = fields.get("root").filter(|root| !root.trim().is_empty())
    {
        return Ok(ResolvedRunRepo {
            root: root.clone(),
            origin_url: fields.get("origin_url").cloned().unwrap_or_default(),
        });
    }
    Err(format!(
        "Forge cannot start this run because repo_id `{repo_id}` is not connected. Connect the repo before starting a run."
    ))
}

struct RunIntakeEvidence {
    generated_from: String,
    readiness_status: String,
    medium_high_work_ready: String,
    safe_next_move: String,
    semantic_orientation_operations: String,
    proof_strategy: String,
    first_run_task_ids: String,
    scout_status: String,
    scout_candidate_count: String,
    scout_candidate_kinds: String,
    scout_candidate_paths: String,
    write_scope_hint: String,
    off_limits_patterns: String,
    review_focus: String,
    source_host: String,
    origin_url_present: String,
}

fn repo_profile_json_value(
    profile: &crate::forge_repo_profile::ForgeRepoProfile,
) -> serde_json::Value {
    serde_json::json!({
        "primary_language": profile.primary_language,
        "language_pack_id": profile.language_pack_id,
        "detected_language_packs": &profile.detected_language_packs,
        "detected_files": &profile.detected_files,
        "quality_signals": &profile.quality_signals,
        "standards_sources": &profile.standards_sources,
        "standards_source_fingerprints": &profile.standards_source_fingerprints,
        "standards_source_summaries": &profile.standards_source_summaries,
        "review_axes": &profile.review_axes,
        "principal_review_gates": &profile.principal_review_gates,
        "language_pack_guidance": &profile.language_pack_guidance,
        "semantic_intelligence": &profile.semantic_intelligence,
        "semantic_tool_readiness": &profile.semantic_tool_readiness,
        "toolchain_readiness": &profile.toolchain_readiness,
        "proof_plan": {
            "status": profile.proof_plan_status,
            "next_action": &profile.proof_plan_next_action,
            "summary": &profile.proof_plan_summary,
        },
        "suggested_checks": &profile.suggested_checks,
        "artifact_patterns": &profile.artifact_patterns,
    })
}

fn run_intake_evidence(
    repo: &ResolvedRunRepo,
    profile: &crate::forge_repo_profile::ForgeRepoProfile,
    profile_value: &serde_json::Value,
    selected_checks: &[String],
    selected_checks_source: &str,
    verified_hosted_execution: bool,
) -> RunIntakeEvidence {
    // Hosted runs have already passed require_verified_environment before this
    // evidence is assembled. Do not leak the Render host's missing toolchain
    // into a run whose checks execute in that verified sandbox.
    let readiness_status = if verified_hosted_execution {
        "READY_FOR_MEDIUM_HIGH_WORK"
    } else {
        repo_readiness_status(profile.proof_plan_status)
    };
    let safe_next_move = if verified_hosted_execution {
        "Run the selected checks in the verified hosted sandbox, orient with semantic_query, then review the generated PR handoff."
    } else {
        repo_safe_next_move(readiness_status)
    }
    .to_string();
    let first_run_tasks = crate::forge_repo_onboarding_packet_route::first_run_tasks(
        profile.language_pack_id,
        profile_value,
    );
    let first_run_task_ids = join_json_field(&first_run_tasks, "task_id");
    let artifact_patterns = profile
        .artifact_patterns
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    let scout = crate::forge_repo_task_scout_route::scout_repo(
        Path::new(&repo.root),
        selected_checks,
        &artifact_patterns,
    );
    let scout_status = if scout.candidates.is_empty() {
        "NO_DETERMINISTIC_TASKS_FOUND"
    } else {
        "TASKS_FOUND"
    };
    let first_candidate = scout.candidates.first();
    let mut semantic_orientation_operations = first_candidate
        .map(|candidate| {
            json_array_object_field_values(candidate, "semantic_orientation_queries", "operation")
        })
        .unwrap_or_default();
    if semantic_orientation_operations.is_empty() {
        semantic_orientation_operations = default_semantic_orientation_operations(profile);
    }
    let write_scope_hint = first_candidate
        .map(|candidate| json_array_string_values(&candidate["write_scope_hint"]))
        .unwrap_or_else(|| vec![".".to_string()]);
    let review_focus = first_candidate
        .map(|candidate| json_array_string_values(&candidate["review_focus"]))
        .filter(|items| !items.is_empty())
        .unwrap_or_else(|| {
            profile
                .principal_review_gates
                .iter()
                .map(|gate| (*gate).to_string())
                .collect()
        });
    let proof_strategy = if selected_checks.iter().any(|check| !check.trim().is_empty()) {
        format!(
            "{selected_checks_source}:selected_checks_then_successful_semantic_orientation_then_review_packet"
        )
    } else {
        "operator_required:choose_proof_before_provider_spend".to_string()
    };
    RunIntakeEvidence {
        generated_from: "repo_readiness+repo_task_scout+language_task_alignment".to_string(),
        readiness_status: readiness_status.to_string(),
        medium_high_work_ready: (readiness_status == "READY_FOR_MEDIUM_HIGH_WORK").to_string(),
        safe_next_move,
        semantic_orientation_operations: join_strings(&semantic_orientation_operations),
        proof_strategy,
        first_run_task_ids,
        scout_status: scout_status.to_string(),
        scout_candidate_count: scout.candidates.len().to_string(),
        scout_candidate_kinds: join_json_field(&scout.candidates, "kind"),
        scout_candidate_paths: join_json_field(&scout.candidates, "path"),
        write_scope_hint: join_strings(&write_scope_hint),
        off_limits_patterns: join_strings(&artifact_patterns),
        review_focus: join_strings(&review_focus),
        source_host: crate::forge_repo_onboarding_packet_route::infer_source_host(&repo.origin_url)
            .to_string(),
        origin_url_present: (!repo.origin_url.trim().is_empty()).to_string(),
    }
}

fn repo_readiness_status(proof_plan_status: &str) -> &'static str {
    match proof_plan_status {
        "ready" => "READY_FOR_MEDIUM_HIGH_WORK",
        "setup_required" => "SETUP_REQUIRED",
        _ => "OPERATOR_CHECK_REQUIRED",
    }
}

fn repo_safe_next_move(readiness_status: &str) -> &'static str {
    match readiness_status {
        "READY_FOR_MEDIUM_HIGH_WORK" => {
            "Start with the selected check, orient with semantic_query, then review the generated PR handoff."
        }
        "SETUP_REQUIRED" => {
            "Install or expose the missing proof toolchain before spending provider calls."
        }
        _ => "Choose an explicit proof command before starting medium or high-complexity work.",
    }
}

fn default_semantic_orientation_operations(
    profile: &crate::forge_repo_profile::ForgeRepoProfile,
) -> Vec<String> {
    if profile.language_pack_id == "generic" || profile.semantic_intelligence.is_empty() {
        return vec!["capabilities".to_string()];
    }
    vec![
        "capabilities".to_string(),
        "file_outline".to_string(),
        "related_tests".to_string(),
        "diagnostics".to_string(),
    ]
}

fn semantic_policy_operations_from_intake(operations: &str) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    operations
        .split(',')
        .map(str::trim)
        .map(|operation| operation.trim_end_matches('.'))
        .filter(|operation| !operation.is_empty())
        .filter(|operation| !matches!(*operation, "capabilities" | "lsp_probe"))
        .filter_map(|operation| {
            if seen.insert(operation.to_string()) {
                Some(operation.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn join_json_field(values: &[serde_json::Value], field: &str) -> String {
    join_strings(
        &values
            .iter()
            .filter_map(|value| value[field].as_str().map(str::to_string))
            .collect::<Vec<_>>(),
    )
}

fn json_array_object_field_values(
    value: &serde_json::Value,
    array_field: &str,
    object_field: &str,
) -> Vec<String> {
    value[array_field]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item[object_field].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn json_array_string_values(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn join_strings(values: &[String]) -> String {
    values
        .iter()
        .map(|value| value.replace(',', " "))
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

struct RunOutcomeFold {
    receipt_id: String,
    disposition: String,
    lesson_candidate: String,
}

#[derive(Default)]
struct RunFold {
    work_item_id: String,
    event_count: u32,
    model_calls: u32,
    latest_input_tokens: u64,
    tool_calls: u32,
    checks_passed: u32,
    checks_failed: u32,
    check_duration_ms: u64,
    turns: u32,
    recorded_conclusion: bool,
    latest_event: String,
    latest_detail: String,
    intent_hint: String,
    operator_intent: String,
    run_title: String,
    origin: String,
    system_origin: String,
    run_summary: String,
    operator_run_summary: String,
    forge_run_voice_profile_id: String,
    voice_rewrite_status: String,
    voice_rewrite_model_id: String,
    observed_builder_provider_family: String,
    observed_builder_model_id: String,
    repo_id: String,
    repo_root: String,
    execution_backend_kind: String,
    cloud_environment_id: String,
    repo_primary_language: String,
    language_pack_id: String,
    repo_profile_detected_language_packs: String,
    repo_profile_detected_files: String,
    repo_profile_quality_signals: String,
    repo_profile_standards_sources: String,
    repo_profile_standards_source_fingerprints: String,
    repo_profile_standards_source_summaries: String,
    repo_profile_review_axes: String,
    repo_profile_principal_review_gates: String,
    repo_profile_language_pack_guidance: String,
    repo_profile_semantic_intelligence: String,
    repo_profile_semantic_tool_readiness: String,
    repo_profile_semantic_session_id: String,
    repo_profile_semantic_fallback_index_status: String,
    repo_profile_semantic_source_file_count: String,
    repo_profile_semantic_indexed_file_count: String,
    repo_profile_semantic_indexed_symbol_count: String,
    repo_profile_semantic_related_test_anchor_count: String,
    repo_profile_semantic_session_grants_execution_authority: bool,
    repo_profile_toolchain_readiness: String,
    repo_profile_suggested_checks: String,
    repo_profile_artifact_patterns: String,
    repo_profile_proof_plan_status: String,
    repo_profile_proof_plan_next_action: String,
    repo_profile_proof_plan_summary: String,
    selected_checks: String,
    selected_checks_source: String,
    work_classification_recommended_shape: String,
    work_classification_task_class: String,
    work_classification_complexity_tier: String,
    work_classification_confidence_pct: String,
    work_classification_rationale: String,
    work_classification_source: String,
    work_classification_grants_execution_authority: bool,
    run_strategy_json: String,
    language_task_corpus_id: String,
    language_task_alignment_status: String,
    language_task_alignment_source: String,
    language_task_class: String,
    language_task_complexity_tier: String,
    language_task_visible_check: String,
    language_task_hidden_check_slot: String,
    language_task_artifact_noise_expected: String,
    language_task_required_principal_review_gates: String,
    language_task_engineering_facets: String,
    language_task_evaluation_oracle: String,
    language_task_human_timebox_minutes: String,
    language_task_contamination_policy: String,
    language_task_alignment_grants_execution_authority: bool,
    repo_profile_source: String,
    repo_profile_grants_execution_authority: bool,
    suggested_checks_are_authority: bool,
    principal_orientation_gate_required: bool,
    principal_orientation_gate_tool: String,
    principal_orientation_gate_grants_authority: bool,
    repo_intake_generated_from: String,
    repo_intake_readiness_status: String,
    repo_intake_medium_high_work_ready: bool,
    repo_intake_safe_next_move: String,
    repo_intake_semantic_orientation_operations: String,
    repo_intake_proof_strategy: String,
    repo_intake_first_run_task_ids: String,
    repo_intake_scout_status: String,
    repo_intake_scout_candidate_count: String,
    repo_intake_scout_candidate_kinds: String,
    repo_intake_scout_candidate_paths: String,
    repo_intake_write_scope_hint: String,
    repo_intake_off_limits_patterns: String,
    repo_intake_review_focus: String,
    repo_intake_source_host: String,
    repo_intake_origin_url_present: bool,
    repo_intake_origin_url_recorded: bool,
    repo_intake_source_host_readiness_route: String,
    repo_intake_source_host_pr_draft_route: String,
    repo_intake_provider_calls_allowed: bool,
    repo_intake_run_started: bool,
    repo_intake_network_call_allowed: bool,
    repo_intake_production_write_allowed: bool,
    repo_intake_grants_execution_authority: bool,
    execution_geometry_requested_workers: String,
    execution_geometry_effective_workers: String,
    execution_geometry_lane: String,
    execution_geometry_route: String,
    execution_geometry_reason: String,
    execution_geometry_fleet_required: bool,
    execution_geometry_grants_execution_authority: bool,
    parallel_candidate_primary_run_id: String,
    parallel_candidate_index: String,
    parallel_candidate_count: String,
    parallel_candidate_write_scope: String,
    parallel_candidate_strategy_id: String,
    parallel_candidate_strategy_summary: String,
    parallel_candidate_required_semantic_operations: String,
    parallel_candidate_proof_bias: String,
    builder_casting_status: String,
    builder_casting_requested_slot: String,
    builder_casting_selected_slot: String,
    builder_casting_selected_model_profile_id: String,
    builder_casting_selected_provider_family: String,
    builder_casting_selected_model_id: String,
    builder_casting_recommended_slot: String,
    builder_casting_recommended_model_profile_id: String,
    builder_casting_recommended_provider_family: String,
    builder_casting_recommended_model_id: String,
    builder_casting_basis: String,
    builder_casting_matching_eval_score_count: String,
    builder_casting_accepted_eval_score_count: String,
    builder_casting_matching_run_count: String,
    builder_casting_done_rate_pct: String,
    builder_casting_requested_slot_matches_evidence: bool,
    builder_casting_grants_execution_authority: bool,
    machine_league_trial: bool,
    machine_league_run_kind: String,
    runner_profile_runner_id: String,
    runner_profile_runner_kind: String,
    runner_profile_display_name: String,
    runner_profile_adapter_kind: String,
    runner_profile_execution_mode: String,
    runner_profile_model_profile_id: String,
    machine_runtime_fingerprint_id: String,
    machine_runtime_runner_id: String,
    machine_runtime_adapter_kind: String,
    machine_runtime_binary_name: String,
    machine_runtime_binary_path: String,
    machine_runtime_binary_present: bool,
    machine_runtime_version_command: String,
    machine_runtime_version_observed: String,
    machine_runtime_version_raw_output: String,
    machine_runtime_checksum_sha256: String,
    machine_runtime_adapter_contract_version: String,
    machine_runtime_command_contract: String,
    machine_runtime_drift_status: String,
    machine_runtime_last_fingerprint_id: String,
    machine_runtime_compatibility_status: String,
    quarantine_status: String,
    quarantine_output_quarantined: bool,
    quarantine_external_output_consumable: bool,
    quarantine_acceptance_gate: String,
    quarantine_result_projection_route: String,
    quarantine_blocked_reason: String,
    eval_principal_review_status: String,
    eval_result_receipt_id: String,
    accepted_for_scoreboard: bool,
    scorecard_total_score: String,
    league_context_visibility_tier: String,
    league_context_recommendation_rationale: String,
    league_context_fallback_runner_id: String,
    league_context_scorecard_evidence_count: String,
    league_context_quarantine_posture: String,
    mission_id: String,
    mission_milestone_id: String,
    mission_checkpoint_route: String,
    mission_checkpoint_grants_execution_authority: bool,
    branch: String,
    branch_sha: String,
    changed_paths: String,
    final_line: String,
    finished: bool,
    events: Vec<String>,
    model_contexts: BTreeMap<String, ModelContextFold>,
    latest_context_model_id: String,
    latest_output_tokens: u64,
    peak_input_tokens: u64,
    peak_context_model_id: String,
    total_input_tokens: u64,
    total_output_tokens: u64,
    proof_checks: std::collections::BTreeMap<String, String>,
}

#[derive(Default)]
struct ModelContextFold {
    model_id: String,
    call_count: u32,
    latest_input_tokens: u64,
    latest_output_tokens: u64,
    peak_input_tokens: u64,
    total_input_tokens: u64,
    total_output_tokens: u64,
}

impl RunFold {
    fn record_builder_model_call(&mut self, detail: &str) {
        if detail.contains("voice rewrite") {
            return;
        }
        let Some(model_id) = model_id_from_detail(detail) else {
            return;
        };
        self.observed_builder_provider_family = provider_family_from_model_id(&model_id);
        self.observed_builder_model_id = model_id;
    }

    fn record_model_context_call(&mut self, detail: &str, input_raw: &str, output_raw: &str) {
        let input_tokens = input_raw.parse::<u64>().unwrap_or(0);
        let output_tokens = output_raw.parse::<u64>().unwrap_or(0);
        if input_tokens == 0 && output_tokens == 0 {
            return;
        }
        let model_id = model_id_from_detail(detail).unwrap_or_else(|| {
            if self.builder_casting_selected_model_id.trim().is_empty() {
                "unknown_model".to_string()
            } else {
                self.builder_casting_selected_model_id.clone()
            }
        });
        self.total_input_tokens = self.total_input_tokens.saturating_add(input_tokens);
        self.total_output_tokens = self.total_output_tokens.saturating_add(output_tokens);
        if input_tokens > 0 {
            self.latest_input_tokens = input_tokens;
            self.latest_context_model_id = model_id.clone();
            self.latest_output_tokens = output_tokens;
            if input_tokens > self.peak_input_tokens {
                self.peak_input_tokens = input_tokens;
                self.peak_context_model_id = model_id.clone();
            }
        }
        let entry = self
            .model_contexts
            .entry(model_id.clone())
            .or_insert_with(|| ModelContextFold {
                model_id: model_id.clone(),
                ..Default::default()
            });
        entry.call_count = entry.call_count.saturating_add(1);
        entry.latest_input_tokens = input_tokens;
        entry.latest_output_tokens = output_tokens;
        entry.peak_input_tokens = entry.peak_input_tokens.max(input_tokens);
        entry.total_input_tokens = entry.total_input_tokens.saturating_add(input_tokens);
        entry.total_output_tokens = entry.total_output_tokens.saturating_add(output_tokens);
    }
}

fn model_id_from_detail(detail: &str) -> Option<String> {
    detail.split_whitespace().find_map(|part| {
        part.strip_prefix("model=")
            .map(|model| model.trim_matches(',').to_string())
            .filter(|model| !model.trim().is_empty())
    })
}

fn provider_family_from_model_id(model_id: &str) -> String {
    let normalized = model_id.trim().to_ascii_lowercase();
    if normalized.starts_with("grok") {
        "xai".to_string()
    } else if normalized.starts_with("claude") {
        "anthropic".to_string()
    } else if normalized.starts_with("gpt")
        || normalized.starts_with("o1")
        || normalized.starts_with("o3")
        || normalized.starts_with("o4")
    {
        "openai".to_string()
    } else {
        String::new()
    }
}

fn context_pct(input_tokens: u64, context_window: u64) -> u32 {
    if input_tokens == 0 || context_window == 0 {
        return 0;
    }
    let raw = ((input_tokens as f64 / context_window as f64) * 100.0).round() as u32;
    raw.clamp(1, 100)
}

fn context_telemetry_json(fold: &RunFold) -> String {
    if fold.total_input_tokens == 0 {
        return "null".to_string();
    }
    let latest_context_window = TurnClient::context_window_for_model(&fold.latest_context_model_id);
    let peak_context_window = TurnClient::context_window_for_model(&fold.peak_context_model_id);
    let models = fold
        .model_contexts
        .values()
        .map(model_context_json)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"latest":{{"model_id":{},"input_tokens":{},"output_tokens":{},"context_window":{},"pct":{}}},"peak":{{"model_id":{},"input_tokens":{},"context_window":{},"pct":{}}},"total":{{"input_tokens":{},"output_tokens":{},"model_calls":{},"model_count":{}}},"models":[{}],"source":"provider_usage_receipts","window_source":"mdx_model_window_table"}}"#,
        json_string_literal(&fold.latest_context_model_id),
        fold.latest_input_tokens,
        fold.latest_output_tokens,
        latest_context_window,
        context_pct(fold.latest_input_tokens, latest_context_window),
        json_string_literal(&fold.peak_context_model_id),
        fold.peak_input_tokens,
        peak_context_window,
        context_pct(fold.peak_input_tokens, peak_context_window),
        fold.total_input_tokens,
        fold.total_output_tokens,
        fold.model_calls,
        fold.model_contexts.len(),
        models,
    )
}

fn model_context_json(model: &ModelContextFold) -> String {
    let context_window = TurnClient::context_window_for_model(&model.model_id);
    format!(
        r#"{{"model_id":{},"call_count":{},"latest_input_tokens":{},"latest_output_tokens":{},"peak_input_tokens":{},"context_window":{},"peak_pct":{},"total_input_tokens":{},"total_output_tokens":{}}}"#,
        json_string_literal(&model.model_id),
        model.call_count,
        model.latest_input_tokens,
        model.latest_output_tokens,
        model.peak_input_tokens,
        context_window,
        context_pct(model.peak_input_tokens, context_window),
        model.total_input_tokens,
        model.total_output_tokens,
    )
}

fn cap_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn first_non_empty<'a>(values: &[&'a str]) -> &'a str {
    values
        .iter()
        .copied()
        .find(|value| !value.trim().is_empty())
        .unwrap_or("")
}

fn bounded_human_text(value: &str, max: usize) -> String {
    let compact = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let redacted = redact_sensitive_human_text(&compact);
    cap_chars(&redacted, max)
}

fn bounded_run_title(value: &str, max: usize) -> String {
    let compact = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let redacted = redact_sensitive_human_text(&compact);
    if redacted.chars().count() <= max {
        return redacted;
    }

    let suffix = "...";
    let content_limit = max.saturating_sub(suffix.chars().count());
    let candidate = redacted.chars().take(content_limit).collect::<String>();
    let word_boundary = candidate
        .char_indices()
        .rev()
        .find_map(|(index, character)| character.is_whitespace().then_some(index));
    let clipped = word_boundary
        .filter(|index| *index >= content_limit / 2)
        .map(|index| &candidate[..index])
        .unwrap_or(candidate.as_str())
        .trim_end_matches(|character: char| character.is_whitespace() || character == '.');
    format!("{clipped}{suffix}")
}

fn redact_sensitive_human_text(text: &str) -> String {
    text.split_whitespace()
        .map(|token| {
            let lower = token.to_ascii_lowercase();
            if lower.contains("api_key")
                || lower.contains("apikey")
                || lower.contains("authorization:")
                || lower.contains("bearer")
                || token.starts_with("sk-")
                || token.starts_with("xai-")
            {
                "[redacted]".to_string()
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn run_title_from_intent(intent: &str) -> String {
    let title = intent
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    if title.is_empty() {
        "Forge run".to_string()
    } else {
        bounded_run_title(title, 140)
    }
}

fn system_run_title(fold: &RunFold) -> &'static str {
    match fold.system_origin.as_str() {
        "builder_loop_tick" => "Native improvement loop tick",
        "forge_system" => "System Forge run",
        _ => "",
    }
}

fn actor_is_operator(actor_id: &str) -> bool {
    actor_id.starts_with("human:") || actor_id == "local_user" || actor_id == "human"
}

fn selected_checks_source(source: &str) -> &str {
    if source.trim().is_empty() {
        "unknown_legacy"
    } else {
        source
    }
}

fn execution_geometry_lane(lane: &str) -> &str {
    if lane.trim().is_empty() {
        "single_worker"
    } else {
        lane
    }
}

fn run_strategy_receipt_json(receipt: &mdx_core::Receipt) -> String {
    let value = |key: &str| receipt.payload.get(key).map(String::as_str).unwrap_or("");
    let version = value("run_strategy_version");
    if version.is_empty() {
        return r#"{"version":"legacy","mode":"legacy","harness_id":"mdx_native","planner_mode":"bounded","review_mode":"conditional_independent","grants_execution_authority":false}"#.to_string();
    }
    format!(
        r#"{{"version":{},"mode":{},"outcome_preference":{},"execution_shape":{},"width":{},"harness_id":{},"harness_source":{},"planner_mode":{},"planner_source":{},"review_mode":{},"review_source":{},"advisor_mode":{},"max_turns":{},"max_cost_cents":{},"max_runtime_ms":{},"operator_locked_fields":[{}],"policy_floor_fields":[{}],"rationale":{},"grants_execution_authority":{}}}"#,
        json_string_literal(version),
        json_string_literal(value("run_strategy_mode")),
        json_string_literal(value("run_strategy_outcome_preference")),
        json_string_literal(value("run_strategy_execution_shape")),
        value("run_strategy_width").parse::<u32>().unwrap_or(1),
        json_string_literal(value("run_strategy_harness_id")),
        json_string_literal(value("run_strategy_harness_source")),
        json_string_literal(value("run_strategy_planner_mode")),
        json_string_literal(value("run_strategy_planner_source")),
        json_string_literal(value("run_strategy_review_mode")),
        json_string_literal(value("run_strategy_review_source")),
        json_string_literal(value("run_strategy_advisor_mode")),
        value("run_strategy_max_turns").parse::<u32>().unwrap_or(0),
        value("run_strategy_max_cost_cents")
            .parse::<u32>()
            .unwrap_or(0),
        value("run_strategy_max_runtime_ms")
            .parse::<u64>()
            .unwrap_or(0),
        csv_json_array(value("run_strategy_operator_locked_fields")),
        csv_json_array(value("run_strategy_policy_floor_fields")),
        json_string_literal(value("run_strategy_rationale")),
        value("run_strategy_grants_execution_authority") == "true",
    )
}

fn execution_geometry_route(route: &str) -> &str {
    if route.trim().is_empty() {
        "/forge/runs.json"
    } else {
        route
    }
}

fn execution_geometry_reason(reason: &str) -> &str {
    if reason.trim().is_empty() {
        "legacy_run_receipt_before_execution_geometry"
    } else {
        reason
    }
}

fn parallel_candidate_strategy_id(value: &str, index: u32, count: u32) -> &str {
    if value.trim().is_empty() {
        candidate_strategy(index, count).id
    } else {
        value
    }
}

fn parallel_candidate_strategy_summary(value: &str, index: u32, count: u32) -> &str {
    if value.trim().is_empty() {
        candidate_strategy(index, count).summary
    } else {
        value
    }
}

fn parallel_candidate_required_semantic_operations(value: &str, index: u32, count: u32) -> String {
    if value.trim().is_empty() {
        candidate_strategy(index, count)
            .required_semantic_operations
            .join(",")
    } else {
        value.to_string()
    }
}

fn parallel_candidate_proof_bias(value: &str, index: u32, count: u32) -> &str {
    if value.trim().is_empty() {
        candidate_strategy(index, count).proof_bias
    } else {
        value
    }
}

fn forge_run_stream_route(run_id: &str) -> String {
    if run_id.trim().is_empty() {
        "/forge/runs/stream".to_string()
    } else {
        format!("/forge/runs/stream?run_id={run_id}")
    }
}

fn operator_status(fold: &RunFold) -> &'static str {
    match fold.derived_status() {
        "running" => "working",
        "cannot_proceed" | "budget_exhausted" | "error" | "interrupted" => "needs_you",
        "stopped" => "stopped",
        "recorded" => "recorded",
        "done" | "finished" | "no_change" => "ready_for_review",
        _ => "needs_you",
    }
}

fn live_stage_json(fold: &RunFold) -> String {
    let stages = [
        ("reading", "Reading the repo"),
        ("planning", "Planning"),
        ("writing", "Writing the change"),
        ("proof", "Proving"),
        ("done", "Done"),
    ];
    let active_stage =
        crate::forge_run_stream::event_kind_and_stage(&fold.latest_event, &fold.latest_detail).1;
    let active_index = stage_index(active_stage).unwrap_or(0);
    let terminal = fold.finished;
    let blocked = matches!(
        fold.derived_status(),
        "cannot_proceed" | "budget_exhausted" | "error" | "stopped" | "interrupted"
    );
    stages
        .iter()
        .enumerate()
        .map(|(index, (key, label))| {
            let state = if (terminal && !blocked) || index < active_index {
                "done"
            } else if index == active_index {
                if blocked { "blocked" } else { "active" }
            } else {
                "pending"
            };
            format!(
                r#"{{"key":{},"label":{},"state":{}}}"#,
                json_string_literal(key),
                json_string_literal(label),
                json_string_literal(state),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn stage_index(stage: &str) -> Option<usize> {
    match stage {
        "reading" => Some(0),
        "planning" => Some(1),
        "writing" => Some(2),
        "proof" => Some(3),
        "done" => Some(4),
        _ => None,
    }
}

fn proof_json(fold: &RunFold) -> String {
    let mut names = selected_check_names(&fold.selected_checks);
    for name in fold.proof_checks.keys() {
        if !names.iter().any(|existing| existing == name) {
            names.push(name.clone());
        }
    }
    let checks = names
        .iter()
        .map(|name| {
            let state = fold
                .proof_checks
                .get(name)
                .map(String::as_str)
                .unwrap_or_else(|| {
                    let active_stage = crate::forge_run_stream::event_kind_and_stage(
                        &fold.latest_event,
                        &fold.latest_detail,
                    )
                    .1;
                    if !fold.finished && active_stage == "proof" {
                        "running"
                    } else {
                        "pending"
                    }
                });
            format!(
                r#"{{"name":{},"state":{}}}"#,
                json_string_literal(name),
                json_string_literal(state),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"checks":[{}],"summary_route":"/forge/review-packet.json"}}"#,
        checks
    )
}

fn selected_check_names(value: &str) -> Vec<String> {
    value
        .split([',', '\n'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn proof_check_name(detail: &str) -> String {
    let command = detail
        .strip_prefix("run_command ")
        .unwrap_or(detail)
        .split(" exit=")
        .next()
        .unwrap_or(detail)
        .trim();
    if command.is_empty() {
        "selected proof check".to_string()
    } else {
        command.to_string()
    }
}

fn diff_json(fold: &RunFold) -> String {
    let file_count = detail_number(&fold.final_line, "files_changed").unwrap_or(0);
    format!(
        r#"{{"ready":{},"file_count":{},"changed_paths":[{}],"commit_sha":{},"diff_route":"/forge/run-diff.json"}}"#,
        diff_ready(fold),
        file_count,
        csv_json_array(&fold.changed_paths),
        json_string_literal(&fold.branch_sha),
    )
}

fn diff_ready(fold: &RunFold) -> bool {
    !fold.branch.trim().is_empty()
        || fold
            .events
            .iter()
            .any(|event| event.contains(r#""kind":"diff_ready""#))
}

fn detail_number(detail: &str, name: &str) -> Option<u32> {
    let prefix = format!("{name}=");
    detail.split_whitespace().find_map(|part| {
        part.strip_prefix(&prefix)
            .and_then(|value| value.trim_matches(',').parse::<u32>().ok())
    })
}

fn controls_json(fold: &RunFold) -> String {
    control_specs(fold)
        .iter()
        .map(|(action, allowed, route)| {
            format!(
                r#"{{"action":{},"allowed":{},"route":{}}}"#,
                json_string_literal(action),
                allowed,
                json_string_literal(route),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn allowed_controls_json(fold: &RunFold) -> String {
    control_specs(fold)
        .iter()
        .filter(|(_, allowed, _)| *allowed)
        .map(|(action, _, _)| json_string_literal(action))
        .collect::<Vec<_>>()
        .join(",")
}

fn control_specs(fold: &RunFold) -> [(&'static str, bool, &'static str); 5] {
    let running = fold.derived_status() == "running";
    let review_ready = operator_status(fold) == "ready_for_review";
    let ship_ready = review_ready
        && fold.checks_failed == 0
        && !fold.branch.trim().is_empty()
        && diff_ready(fold);
    [
        ("steer", running, "/forge/run-controls.json"),
        ("stop", running, "/forge/run-controls.json"),
        ("revise", review_ready, "/forge/run-revisions.json"),
        ("review", review_ready, "/forge/review-packet.json"),
        ("ship", ship_ready, "/forge/run-ship-decisions.json"),
    ]
}

fn repo_json(fold: &RunFold) -> String {
    format!(
        r#"{{"repo_id":{},"root":{},"primary_language":{},"language_pack_id":{}}}"#,
        json_string_literal(&fold.repo_id),
        json_string_literal(&fold.repo_root),
        json_string_literal(&fold.repo_primary_language),
        json_string_literal(&fold.language_pack_id),
    )
}

fn model_or_worker_json(fold: &RunFold) -> String {
    let provider_family = first_non_empty(&[
        &fold.builder_casting_selected_provider_family,
        &fold.observed_builder_provider_family,
    ]);
    let model_id = first_non_empty(&[
        &fold.builder_casting_selected_model_id,
        &fold.observed_builder_model_id,
    ]);
    let model_class = crate::forge_run_stream::model_class(
        provider_family,
        &fold.builder_casting_selected_slot,
        model_id,
    );
    format!(
        r#"{{"provider_family":{},"model_id":{},"slot":{},"model_class":{}}}"#,
        json_string_literal(provider_family),
        json_string_literal(model_id),
        json_string_literal(&fold.builder_casting_selected_slot),
        json_string_literal(model_class),
    )
}

fn runner_profile_json(fold: &RunFold) -> String {
    let runner_id = if fold.runner_profile_runner_id.trim().is_empty() {
        "mdx_native_harness_runner"
    } else {
        &fold.runner_profile_runner_id
    };
    let runner_kind = if fold.runner_profile_runner_kind.trim().is_empty() {
        "mdx_native"
    } else {
        &fold.runner_profile_runner_kind
    };
    let display_name = if fold.runner_profile_display_name.trim().is_empty() {
        "Forge native builder"
    } else {
        &fold.runner_profile_display_name
    };
    let adapter_kind = if fold.runner_profile_adapter_kind.trim().is_empty() {
        "mdx_native"
    } else {
        &fold.runner_profile_adapter_kind
    };
    let execution_mode = if fold.runner_profile_execution_mode.trim().is_empty() {
        "local_receipt_gated"
    } else {
        &fold.runner_profile_execution_mode
    };
    let model_profile_id = if fold.runner_profile_model_profile_id.trim().is_empty() {
        if fold
            .builder_casting_selected_model_profile_id
            .trim()
            .is_empty()
        {
            "mdx_native_local_model_profile"
        } else {
            &fold.builder_casting_selected_model_profile_id
        }
    } else {
        &fold.runner_profile_model_profile_id
    };
    let strategy = if fold.run_strategy_json.trim().is_empty() {
        r#"{"version":"legacy","mode":"legacy","harness_id":"mdx_native","planner_mode":"bounded","review_mode":"conditional_independent","grants_execution_authority":false}"#
    } else {
        fold.run_strategy_json.as_str()
    };
    format!(
        r#"{{"runner_id":{},"runner_kind":{},"display_name":{},"adapter_kind":{},"execution_mode":{},"model_profile_id":{},"strategy":{},"leads_header":{},"model_disclosed_second":true}}"#,
        json_string_literal(runner_id),
        json_string_literal(runner_kind),
        json_string_literal(display_name),
        json_string_literal(adapter_kind),
        json_string_literal(execution_mode),
        json_string_literal(model_profile_id),
        strategy,
        fold.machine_league_trial,
    )
}

fn machine_runtime_json(fold: &RunFold) -> String {
    let external_harness = fold.runner_profile_runner_kind == "external_harness";
    let runner_id = if fold.machine_runtime_runner_id.trim().is_empty() {
        if fold.runner_profile_runner_id.trim().is_empty() {
            "mdx_native_harness_runner"
        } else {
            &fold.runner_profile_runner_id
        }
    } else {
        &fold.machine_runtime_runner_id
    };
    let adapter_kind = if fold.machine_runtime_adapter_kind.trim().is_empty() {
        if fold.runner_profile_adapter_kind.trim().is_empty() {
            "mdx_native"
        } else {
            &fold.runner_profile_adapter_kind
        }
    } else {
        &fold.machine_runtime_adapter_kind
    };
    let compatibility_status = if fold.machine_runtime_compatibility_status.trim().is_empty() {
        if fold.machine_league_trial || external_harness {
            "runtime_unverified"
        } else {
            "native_runtime"
        }
    } else {
        &fold.machine_runtime_compatibility_status
    };
    let drift_status = if fold.machine_runtime_drift_status.trim().is_empty() {
        if fold.machine_league_trial || external_harness {
            "runtime_unverified"
        } else {
            "runtime_native"
        }
    } else {
        &fold.machine_runtime_drift_status
    };
    format!(
        r#"{{"runner_id":{},"adapter_kind":{},"binary_name":{},"binary_path":{},"binary_present":{},"version_command":{},"version_observed":{},"version_raw_output":{},"checksum_sha256":{},"adapter_contract_version":{},"command_contract":{},"fingerprint_id":{},"drift_status":{},"last_fingerprint_id":{},"compatibility_status":{}}}"#,
        json_string_literal(runner_id),
        json_string_literal(adapter_kind),
        json_string_literal(&fold.machine_runtime_binary_name),
        json_string_literal(&fold.machine_runtime_binary_path),
        fold.machine_runtime_binary_present,
        json_string_literal(&fold.machine_runtime_version_command),
        json_string_literal(&fold.machine_runtime_version_observed),
        json_string_literal(&fold.machine_runtime_version_raw_output),
        json_string_literal(&fold.machine_runtime_checksum_sha256),
        json_string_literal(&fold.machine_runtime_adapter_contract_version),
        json_string_literal(&fold.machine_runtime_command_contract),
        json_string_literal(&fold.machine_runtime_fingerprint_id),
        json_string_literal(drift_status),
        json_string_literal(&fold.machine_runtime_last_fingerprint_id),
        json_string_literal(compatibility_status),
    )
}

fn quarantine_json(fold: &RunFold) -> String {
    let external_harness = fold.runner_profile_runner_kind == "external_harness";
    let status = if fold.accepted_for_scoreboard {
        "accepted_for_scoreboard_output_still_quarantined"
    } else if fold.quarantine_status.trim().is_empty() {
        if fold.machine_league_trial || external_harness {
            "output_quarantined_pending_mdx_gates"
        } else {
            "not_quarantined_native_run"
        }
    } else {
        &fold.quarantine_status
    };
    let acceptance_gate = if fold.accepted_for_scoreboard {
        "principal_review_accepted_for_scoreboard"
    } else if fold.quarantine_acceptance_gate.trim().is_empty() {
        if fold.machine_league_trial || external_harness {
            "mdx_quality_gates_then_principal_review"
        } else {
            "forge_native_quality_gates"
        }
    } else {
        &fold.quarantine_acceptance_gate
    };
    let result_projection_route = if fold.quarantine_result_projection_route.trim().is_empty() {
        "/forge/fleet-eval-results/projection.json"
    } else {
        &fold.quarantine_result_projection_route
    };
    let blocked_reason = if fold.accepted_for_scoreboard {
        "accepted_scoreboard_evidence_not_production_output"
    } else if fold.quarantine_blocked_reason.trim().is_empty() {
        if fold.machine_league_trial || external_harness {
            "pending_mdx_quality_gates"
        } else {
            ""
        }
    } else {
        &fold.quarantine_blocked_reason
    };
    let output_quarantined = if fold.accepted_for_scoreboard {
        true
    } else {
        fold.quarantine_output_quarantined || external_harness
    };
    format!(
        r#"{{"status":{},"output_quarantined":{},"external_output_consumable":{},"acceptance_gate":{},"result_projection_route":{},"blocked_reason":{},"accepted_for_scoreboard":{},"eval_principal_review_status":{},"eval_result_receipt_id":{},"scorecard_total_score":{}}}"#,
        json_string_literal(status),
        output_quarantined,
        fold.quarantine_external_output_consumable,
        json_string_literal(acceptance_gate),
        json_string_literal(result_projection_route),
        json_string_literal(blocked_reason),
        fold.accepted_for_scoreboard,
        json_string_literal(&fold.eval_principal_review_status),
        json_string_literal(&fold.eval_result_receipt_id),
        json_string_literal(&fold.scorecard_total_score),
    )
}

fn league_context_json(fold: &RunFold) -> String {
    if !fold.machine_league_trial {
        return "null".to_string();
    }
    let visibility_tier = if fold.league_context_visibility_tier.trim().is_empty() {
        "product_recommendation"
    } else {
        &fold.league_context_visibility_tier
    };
    let rationale = if fold
        .league_context_recommendation_rationale
        .trim()
        .is_empty()
    {
        "Forge is gathering accepted evidence for this machine league trial."
    } else {
        &fold.league_context_recommendation_rationale
    };
    let fallback_runner = if fold.league_context_fallback_runner_id.trim().is_empty() {
        "mdx_native_harness_runner"
    } else {
        &fold.league_context_fallback_runner_id
    };
    let quarantine_posture = if fold.league_context_quarantine_posture.trim().is_empty() {
        "external_output_held_until_mdx_gates"
    } else {
        &fold.league_context_quarantine_posture
    };
    format!(
        r#"{{"required_for_trial":true,"visibility_tier":{},"recommendation_rationale":{},"fallback_runner_id":{},"scorecard_evidence_count":{},"quarantine_posture":{},"recommendation_route":"/forge/machine-league/recommendations.json","learning_ledger_route":"/forge/machine-league/learning-ledger.json"}}"#,
        json_string_literal(visibility_tier),
        json_string_literal(rationale),
        json_string_literal(fallback_runner),
        fold.league_context_scorecard_evidence_count
            .parse::<u32>()
            .unwrap_or(0),
        json_string_literal(quarantine_posture),
    )
}

fn json_string_vec(values: &[String]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",")
}

fn parallel_execution_groups_json(
    runs: &std::collections::BTreeMap<String, RunFold>,
    order: &[String],
) -> (usize, String) {
    let mut seen_primary: Vec<String> = Vec::new();
    let mut groups: Vec<String> = Vec::new();
    for run_id in order.iter().rev() {
        let Some(fold) = runs.get(run_id) else {
            continue;
        };
        if fold.hidden_from_run_list() {
            continue;
        }
        let expected_count = fold.parallel_candidate_count.parse::<u32>().unwrap_or(1);
        let effective_workers = fold
            .execution_geometry_effective_workers
            .parse::<u32>()
            .unwrap_or(expected_count.max(1));
        if expected_count <= 1 && effective_workers <= 1 {
            continue;
        }
        let primary_run_id = fold.parallel_candidate_primary(run_id).to_string();
        if seen_primary.iter().any(|seen| seen == &primary_run_id) {
            continue;
        }
        seen_primary.push(primary_run_id.clone());
        let group = parallel_execution_group_json(runs, &primary_run_id, expected_count);
        groups.push(group);
    }
    (groups.len(), groups.join(","))
}

fn parallel_execution_group_json(
    runs: &std::collections::BTreeMap<String, RunFold>,
    primary_run_id: &str,
    expected_count: u32,
) -> String {
    let mut candidates = runs
        .iter()
        .filter(|(run_id, fold)| fold.parallel_candidate_primary(run_id) == primary_run_id)
        .filter(|(_, fold)| !fold.hidden_from_run_list())
        .collect::<Vec<_>>();
    candidates.sort_by(|(left_id, left), (right_id, right)| {
        let left_index = left.parallel_candidate_index.parse::<u32>().unwrap_or(1);
        let right_index = right.parallel_candidate_index.parse::<u32>().unwrap_or(1);
        left_index
            .cmp(&right_index)
            .then_with(|| left_id.cmp(right_id))
    });

    let mut finished_count = 0_u32;
    let mut running_count = 0_u32;
    let mut done_count = 0_u32;
    let mut no_change_count = 0_u32;
    let mut successful_no_change_count = 0_u32;
    let mut cannot_proceed_count = 0_u32;
    let mut failed_count = 0_u32;
    let mut checks_passed_total = 0_u32;
    let mut checks_failed_total = 0_u32;
    let mut best: Option<(&str, &RunFold, i64)> = None;
    let candidate_json = candidates
        .iter()
        .map(|(run_id, fold)| {
            let status = fold.derived_status();
            if fold.finished {
                finished_count += 1;
            } else {
                running_count += 1;
            }
            match status {
                "done" => done_count += 1,
                "no_change" => {
                    no_change_count += 1;
                    if fold.checks_failed == 0 && fold.checks_passed > 0 {
                        successful_no_change_count += 1;
                    }
                }
                "cannot_proceed" => cannot_proceed_count += 1,
                "budget_exhausted" | "error" | "stopped" => failed_count += 1,
                _ => {}
            }
            checks_passed_total += fold.checks_passed;
            checks_failed_total += fold.checks_failed;
            let score = projection_candidate_score(fold);
            if best
                .as_ref()
                .map(|(_, _, best_score)| score > *best_score)
                .unwrap_or(true)
            {
                best = Some((run_id.as_str(), fold, score));
            }
            let index = fold.parallel_candidate_index.parse::<u32>().unwrap_or(1);
            let count = fold
                .parallel_candidate_count
                .parse::<u32>()
                .unwrap_or(expected_count.max(1));
            format!(
                r#"{{"run_id":{},"role":{},"index":{},"count":{},"status":{},"finished":{},"strategy_id":{},"strategy_summary":{},"required_semantic_operations":[{}],"proof_bias":{},"checks_passed":{},"checks_failed":{},"turns":{},"branch":{},"score_basis":{},"grants_execution_authority":false}}"#,
                json_string_literal(run_id),
                json_string_literal(fold.parallel_candidate_role(run_id)),
                index,
                count,
                json_string_literal(status),
                fold.finished,
                json_string_literal(parallel_candidate_strategy_id(
                    &fold.parallel_candidate_strategy_id,
                    index,
                    count,
                )),
                json_string_literal(parallel_candidate_strategy_summary(
                    &fold.parallel_candidate_strategy_summary,
                    index,
                    count,
                )),
                csv_json_array(&parallel_candidate_required_semantic_operations(
                    &fold.parallel_candidate_required_semantic_operations,
                    index,
                    count,
                )),
                json_string_literal(parallel_candidate_proof_bias(
                    &fold.parallel_candidate_proof_bias,
                    index,
                    count,
                )),
                fold.checks_passed,
                fold.checks_failed,
                fold.turns,
                json_string_literal(&fold.branch),
                json_string_literal(projection_candidate_score_basis(fold)),
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    let observed_count = candidates.len() as u32;
    let planned_count = expected_count.max(observed_count).max(1);
    let selection_status = if observed_count < planned_count {
        "waiting_for_all_candidates_to_start"
    } else if running_count > 0 {
        "waiting_for_candidates"
    } else if done_count > 0 || successful_no_change_count > 0 {
        "ready_for_review"
    } else {
        "blocked_no_successful_candidate"
    };
    let recommendation_basis =
        "projection_status_only; review_packet adds diff_quality, proof_quality, and eval evidence";
    let (recommended_run_id, recommended_strategy_id, recommended_strategy_summary) = best
        .map(|(run_id, fold, _)| {
            let index = fold.parallel_candidate_index.parse::<u32>().unwrap_or(1);
            let count = fold
                .parallel_candidate_count
                .parse::<u32>()
                .unwrap_or(expected_count.max(1));
            (
                run_id,
                parallel_candidate_strategy_id(&fold.parallel_candidate_strategy_id, index, count),
                parallel_candidate_strategy_summary(
                    &fold.parallel_candidate_strategy_summary,
                    index,
                    count,
                ),
            )
        })
        .unwrap_or(("", "", ""));
    let primary = runs.get(primary_run_id);
    let requested_workers = primary
        .and_then(|fold| {
            fold.execution_geometry_requested_workers
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(planned_count);
    let effective_workers = primary
        .and_then(|fold| {
            fold.execution_geometry_effective_workers
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(planned_count);
    let lane = primary
        .map(|fold| execution_geometry_lane(&fold.execution_geometry_lane))
        .unwrap_or("bounded_parallel_exploration");
    format!(
        r#"{{"generated_from":"forge.run.event.parallel_candidates","primary_run_id":{},"requested_workers":{},"effective_workers":{},"lane":{},"planned_candidate_count":{},"observed_candidate_count":{},"finished_candidate_count":{},"running_candidate_count":{},"done_candidate_count":{},"no_change_candidate_count":{},"cannot_proceed_candidate_count":{},"failed_candidate_count":{},"checks_passed_total":{},"checks_failed_total":{},"selection_status":{},"recommended_run_id":{},"recommended_strategy_id":{},"recommended_strategy_summary":{},"recommendation_basis":{},"review_packet_route":"/forge/review-packet.json","candidates":[{}],"grants_execution_authority":false}}"#,
        json_string_literal(primary_run_id),
        requested_workers,
        effective_workers,
        json_string_literal(lane),
        planned_count,
        observed_count,
        finished_count,
        running_count,
        done_count,
        no_change_count,
        cannot_proceed_count,
        failed_count,
        checks_passed_total,
        checks_failed_total,
        json_string_literal(selection_status),
        json_string_literal(recommended_run_id),
        json_string_literal(recommended_strategy_id),
        json_string_literal(recommended_strategy_summary),
        json_string_literal(recommendation_basis),
        candidate_json,
    )
}

fn projection_candidate_score(fold: &RunFold) -> i64 {
    let mut score = 0_i64;
    match fold.derived_status() {
        "done" => score += 1_000,
        "running" => score += 25,
        "finished" => score += 10,
        "no_change" if fold.checks_failed == 0 && fold.checks_passed > 0 => score += 250,
        "no_change" => score -= 250,
        "cannot_proceed" => score -= 400,
        "budget_exhausted" => score -= 450,
        "stopped" => score -= 500,
        "error" => score -= 600,
        _ => {}
    }
    score += i64::from(fold.checks_passed) * 60;
    score -= i64::from(fold.checks_failed) * 120;
    if !fold.branch.trim().is_empty() {
        score += 30;
    }
    if fold.finished {
        score += 10;
    }
    score - i64::from(fold.turns.min(200))
}

fn projection_candidate_score_basis(fold: &RunFold) -> &'static str {
    match fold.derived_status() {
        "done" => "terminal_done_plus_observed_checks",
        "running" => "still_running_projection_only",
        "no_change" if fold.checks_failed == 0 && fold.checks_passed > 0 => {
            "verified_no_change_candidate"
        }
        "no_change" => "no_change_penalty",
        "cannot_proceed" => "cannot_proceed_penalty",
        "budget_exhausted" => "budget_exhausted_penalty",
        "stopped" => "stopped_penalty",
        "error" => "error_penalty",
        _ => "terminal_status_projection_only",
    }
}

fn selected_checks_source_for(
    operator_supplied_checks: bool,
    selected_checks: &[String],
    setup_commands_added: bool,
    stack_aware_checks_inferred: bool,
) -> &'static str {
    if operator_supplied_checks && setup_commands_added {
        "operator_supplied_plus_repo_setup"
    } else if operator_supplied_checks {
        "operator_supplied"
    } else if stack_aware_checks_inferred && setup_commands_added {
        "stack_aware_scope_inferred_plus_repo_setup"
    } else if stack_aware_checks_inferred {
        "stack_aware_scope_inferred"
    } else if selected_checks.iter().any(|check| !check.trim().is_empty()) {
        "repo_profile_inferred"
    } else {
        "none_recorded"
    }
}

fn stack_aware_selected_checks(
    intent: &str,
    write_scope: &[String],
    repo_root: &Path,
) -> Option<Vec<String>> {
    if let Some(command) = explicit_make_command_from_intent(intent, repo_root) {
        return Some(vec![command]);
    }
    if scope_or_intent_targets_forge_docs(intent, write_scope)
        && make_target_exists(repo_root, "forge-flagship-acceptance-check")
    {
        return Some(vec!["make forge-flagship-acceptance-check".to_string()]);
    }
    if scope_or_intent_targets_native_macos(intent, write_scope)
        && make_target_exists(repo_root, "native-macos-operator-check")
    {
        return Some(vec!["make native-macos-operator-check".to_string()]);
    }
    if repo_root.join("Cargo.toml").exists() && intent_mentions_rust_path(intent) {
        return Some(vec![
            explicit_cargo_command_from_intent(intent)
                .unwrap_or_else(|| rust_check_for_scope(intent, write_scope).to_string()),
        ]);
    }
    // Node/JS takes precedence when the ask clearly targets a JS area, so a
    // polyglot repo (Rust at the root, JS apps under apps/) proves a JS change
    // with a node check instead of falling back to a stale cargo default.
    if scope_or_intent_targets_node(intent, write_scope)
        && let Some(command) = explicit_node_command_from_intent(intent)
            .or_else(|| node_check_for_scope(intent, write_scope))
    {
        return Some(vec![command]);
    }
    if repo_root.join("Cargo.toml").exists() && scope_or_intent_targets_rust(intent, write_scope) {
        return Some(vec![
            explicit_cargo_command_from_intent(intent)
                .unwrap_or_else(|| rust_check_for_scope(intent, write_scope).to_string()),
        ]);
    }
    if repo_root.join("Cargo.toml").exists() && intent_targets_forge_runtime(intent) {
        return Some(vec![
            explicit_cargo_command_from_intent(intent).unwrap_or_else(|| {
                "cargo test -p mdx-server forge_run_route -- --test-threads=1".to_string()
            }),
        ]);
    }
    None
}

fn scope_or_intent_targets_forge_docs(intent: &str, write_scope: &[String]) -> bool {
    write_scope
        .iter()
        .any(|target| path_targets_forge_docs(target))
        || mentioned_repo_paths(intent)
            .iter()
            .any(|target| path_targets_forge_docs(target))
}

fn path_targets_forge_docs(target: &str) -> bool {
    let target = target.trim().trim_start_matches("./");
    target == "docs/FORGE-OPERATOR-EXPERIENCE.md" || target.starts_with("docs/FORGE-")
}

fn make_target_exists(repo_root: &Path, target: &str) -> bool {
    let Ok(makefile) = std::fs::read_to_string(repo_root.join("Makefile")) else {
        return false;
    };
    let needle = format!("{target}:");
    makefile.lines().any(|line| line.starts_with(&needle))
}

fn explicit_make_command_from_intent(intent: &str, repo_root: &Path) -> Option<String> {
    let mut previous_was_make = false;
    for token in intent.split_whitespace() {
        let token = token
            .trim_matches(|ch: char| {
                matches!(
                    ch,
                    '`' | '\'' | '"' | ',' | '.' | ':' | ';' | ')' | '(' | '[' | ']'
                )
            })
            .trim();
        if previous_was_make {
            if make_target_token_is_safe(token) && make_target_exists(repo_root, token) {
                return Some(format!("make {token}"));
            }
            previous_was_make = false;
            continue;
        }
        previous_was_make = token == "make";
    }
    None
}

fn make_target_token_is_safe(token: &str) -> bool {
    !token.is_empty()
        && token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/'))
}

fn scope_or_intent_targets_native_macos(intent: &str, write_scope: &[String]) -> bool {
    write_scope
        .iter()
        .any(|target| path_targets_native_macos(target))
        || mentioned_repo_paths(intent)
            .iter()
            .any(|target| path_targets_native_macos(target))
        || intent.to_ascii_lowercase().contains("mac operator")
        || intent.to_ascii_lowercase().contains("native macos")
        || intent.to_ascii_lowercase().contains("macos app")
}

fn path_targets_native_macos(target: &str) -> bool {
    let target = target.trim().trim_start_matches("./").to_ascii_lowercase();
    target.starts_with("apps/mdx-operator-macos/")
        || target == "apps/mdx-operator-macos"
        || target.ends_with(".swift")
}

fn intent_targets_forge_runtime(intent: &str) -> bool {
    let lower = intent.to_ascii_lowercase();
    let mentions_forge_run = lower.contains("forge run")
        || lower.contains("forge/runs")
        || lower.contains("run projection")
        || lower.contains("runs/projection")
        || lower.contains("run controls")
        || lower.contains("run-controls")
        || lower.contains("allowed_controls")
        || lower.contains("no-change")
        || lower.contains("no_change");
    let mentions_runtime_surface = lower.contains("projection")
        || lower.contains("control")
        || lower.contains("ship")
        || lower.contains("review packet")
        || lower.contains("server test")
        || lower.contains("focused server");
    lower.contains("forge_run_route") || (mentions_forge_run && mentions_runtime_surface)
}

fn scope_or_intent_targets_node(intent: &str, write_scope: &[String]) -> bool {
    write_scope.iter().any(|target| path_targets_node(target))
        || mentioned_repo_paths(intent)
            .iter()
            .any(|target| path_targets_node(target))
}

fn path_targets_node(target: &str) -> bool {
    let target = target.trim().trim_start_matches("./").to_ascii_lowercase();
    target.ends_with(".js")
        || target.ends_with(".mjs")
        || target.ends_with(".cjs")
        || target.ends_with(".ts")
        || target.ends_with(".tsx")
        || target.ends_with(".svelte")
        || target == "package.json"
        || target.starts_with("apps/")
}

/// A node test file named in the ask becomes the check verbatim (node <path>).
fn explicit_node_command_from_intent(intent: &str) -> Option<String> {
    for raw in intent
        .split(|c: char| c.is_whitespace() || matches!(c, '`' | '"' | '\'' | '(' | ')' | ',' | ';'))
    {
        let token = raw.trim();
        let lower = token.to_ascii_lowercase();
        let is_test_file = lower.ends_with(".test.mjs")
            || lower.ends_with(".test.js")
            || lower.ends_with(".spec.mjs")
            || lower.ends_with(".spec.js");
        if is_test_file && token.contains('/') && !token.starts_with('/') {
            return Some(format!("node {token}"));
        }
    }
    None
}

/// Fall back to running an app's node test directory when the ask targets an
/// apps/<name> area but names no specific test file.
fn node_check_for_scope(intent: &str, write_scope: &[String]) -> Option<String> {
    let mentioned = mentioned_repo_paths(intent);
    let candidates = write_scope
        .iter()
        .map(String::as_str)
        .chain(mentioned.iter().map(String::as_str));
    for target in candidates {
        let target = target.trim().trim_start_matches("./");
        let mut parts = target.split('/');
        if parts.next() == Some("apps")
            && let Some(app) = parts.next().filter(|name| !name.is_empty())
        {
            return Some(format!("node --test apps/{app}/test"));
        }
    }
    None
}

fn scope_or_intent_targets_rust(intent: &str, write_scope: &[String]) -> bool {
    write_scope.iter().any(|target| path_targets_rust(target))
        || mentioned_repo_paths(intent)
            .iter()
            .any(|target| path_targets_rust(target))
        || intent.to_ascii_lowercase().contains("rust")
}

fn intent_mentions_rust_path(intent: &str) -> bool {
    mentioned_repo_paths(intent)
        .iter()
        .any(|target| path_targets_rust(target))
}

fn path_targets_rust(target: &str) -> bool {
    let target = target.trim().trim_start_matches("./").to_ascii_lowercase();
    target.ends_with(".rs")
        || target == "cargo.toml"
        || target == "cargo.lock"
        || target.starts_with("crates/")
}

fn rust_check_for_scope(intent: &str, write_scope: &[String]) -> &'static str {
    let haystack = write_scope
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(intent))
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    if haystack.contains("forge_loop_runner") && haystack.contains("test") {
        "cargo test -p mdx-server forge_loop_runner"
    } else if haystack.contains("crates/mdx-server") {
        "cargo check -p mdx-server"
    } else if haystack.contains("crates/mdx-core") {
        "cargo check -p mdx-core"
    } else {
        "cargo check"
    }
}

fn explicit_cargo_command_from_intent(intent: &str) -> Option<String> {
    let lower = intent.to_ascii_lowercase();
    let start = lower
        .find("cargo test")
        .or_else(|| lower.find("cargo check"))?;
    let raw = &intent[start..];
    let mut command = raw
        .split(['\n', '\r', '`'])
        .next()
        .unwrap_or(raw)
        .trim()
        .to_string();
    for marker in [" to ", " before ", " after ", " so ", " and ", " as "] {
        if let Some(index) = command.to_ascii_lowercase().find(marker) {
            command.truncate(index);
        }
    }
    command = command
        .trim()
        .trim_end_matches(['.', ',', ';', ':', ')', ']'])
        .to_string();
    if command.starts_with("cargo test") || command.starts_with("cargo check") {
        Some(command)
    } else {
        None
    }
}

fn normalize_selected_commands_for_repo(
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

fn ensure_required_setup_commands(
    selected_checks: &mut Vec<String>,
    repo_profile: &crate::forge_repo_profile::ForgeRepoProfile,
    repo_root: &Path,
) -> bool {
    if selected_checks
        .iter()
        .any(|check| check.trim() == "make native-macos-operator-check")
    {
        return false;
    }
    let required_setup = crate::forge_repo_profile::required_setup_commands(
        repo_root,
        repo_profile.language_pack_id,
    );
    let mut added = false;
    for setup in required_setup.into_iter().rev() {
        if selected_checks.iter().any(|check| check.trim() == setup) {
            continue;
        }
        if selected_checks
            .iter()
            .any(|check| !crate::forge_repo_profile::is_setup_command(check))
        {
            selected_checks.insert(0, setup.to_string());
            added = true;
        }
    }
    added
}

struct LanguageTaskAlignment {
    status: &'static str,
    task: Option<ForgeLanguageTaskCorpusEntry>,
}

fn align_language_task(
    language_pack_id: &str,
    classification: &mdx_core::ForgeWorkClassificationDraft,
) -> LanguageTaskAlignment {
    if language_pack_id == "generic" || language_pack_id.trim().is_empty() {
        return LanguageTaskAlignment {
            status: "no_known_language_pack",
            task: None,
        };
    }
    let corpus = forge_language_task_corpus()
        .into_iter()
        .filter(|task| task.language_pack_id == language_pack_id)
        .collect::<Vec<_>>();
    if corpus.is_empty() {
        return LanguageTaskAlignment {
            status: "no_corpus_for_language_pack",
            task: None,
        };
    }
    let corpus_class = corpus_class_for(&classification.task_class);
    let corpus_tier = corpus_tier_for(&classification.complexity_tier);
    if let Some(task) = corpus
        .iter()
        .find(|task| task.task_class == corpus_class && task.complexity_tier == corpus_tier)
    {
        return LanguageTaskAlignment {
            status: "exact_class_and_tier",
            task: Some(*task),
        };
    }
    if let Some(task) = corpus
        .iter()
        .find(|task| task.complexity_tier == corpus_tier)
    {
        return LanguageTaskAlignment {
            status: "tier_fallback",
            task: Some(*task),
        };
    }
    LanguageTaskAlignment {
        status: "language_pack_fallback",
        task: corpus.first().copied(),
    }
}

fn corpus_class_for(class: &str) -> &'static str {
    match class {
        "bug_fix" => "bug_fix",
        "security" => "security",
        "performance" => "performance",
        "ci_repair" => "ci_repair",
        "feature" => "feature",
        "refactor" => "refactor",
        "docs_code" | "product_ux" => "feature",
        "multi_file" | "architecture" | "api_compat" | "migration" | "concurrency"
        | "observability" | "long_horizon" => "refactor",
        _ => "feature",
    }
}

fn corpus_tier_for(tier: &str) -> &'static str {
    match tier {
        "small" => "small",
        "medium" => "medium",
        "large" | "xl" | "extreme" => "large",
        _ => "medium",
    }
}

impl RunFold {
    fn hidden_from_run_list(&self) -> bool {
        self.system_origin == "builder_loop_tick"
    }

    fn derived_status(&self) -> &'static str {
        if self.finished && self.recorded_conclusion {
            return "recorded";
        }
        if !self.finished && self.latest_event == "run_recovery_pending" {
            return "interrupted";
        }
        if !self.finished {
            return "running";
        }
        if self.final_line.contains("RUN_PLAN_PROPOSED") {
            "plan_proposed"
        } else if self.final_line.contains("RUN_FINISHED_DONE") {
            "done"
        } else if self.final_line.contains("RUN_FINISHED_NO_CHANGE") {
            "no_change"
        } else if self.final_line.contains("RUN_STOPPED") {
            "stopped"
        } else if self.final_line.contains("CANNOT_PROCEED") {
            "cannot_proceed"
        } else if self.final_line.contains("BUDGET_EXHAUSTED") {
            "budget_exhausted"
        } else if self.final_line.contains("ERROR")
            || self.final_line.contains("FAILED")
            || self.final_line.starts_with("model call failed")
            || self.final_line.starts_with("could not create")
        {
            "error"
        } else {
            "finished"
        }
    }

    fn terminal_state(&self) -> &'static str {
        match self.derived_status() {
            "running" => "IN_PROGRESS",
            "done" => "SUCCEEDED",
            "no_change" => "NO_CHANGE",
            "recorded" => "RECORDED",
            "stopped" => "STOPPED",
            "cannot_proceed" => "CANNOT_PROCEED",
            "budget_exhausted" => "BUDGET_EXHAUSTED",
            "error" => "FAILED",
            _ => "FINISHED",
        }
    }

    fn parallel_candidate_role(&self, run_id: &str) -> &'static str {
        if !self.parallel_candidate_primary_run_id.trim().is_empty()
            && self.parallel_candidate_primary_run_id != run_id
        {
            "candidate"
        } else if self
            .execution_geometry_effective_workers
            .parse::<u32>()
            .unwrap_or(1)
            > 1
        {
            "primary"
        } else if !run_id.trim().is_empty() {
            "single"
        } else {
            "unknown"
        }
    }

    fn parallel_candidate_primary<'a>(&'a self, run_id: &'a str) -> &'a str {
        if self.parallel_candidate_primary_run_id.trim().is_empty() {
            run_id
        } else {
            self.parallel_candidate_primary_run_id.as_str()
        }
    }
}

fn record_refusal_and_return(
    kernel: &Arc<RwLock<MdxKernel>>,
    resolved: &crate::request_security::ResolvedWriteIdentity,
    reason: &str,
    repo_id: &str,
    requested_run_id: &str,
) -> Result<RouteResponse, String> {
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    record_refusal_on_kernel_and_return(&mut kernel, resolved, reason, repo_id, requested_run_id)
}

fn record_refusal_on_kernel_and_return(
    kernel: &mut MdxKernel,
    resolved: &crate::request_security::ResolvedWriteIdentity,
    reason: &str,
    repo_id: &str,
    requested_run_id: &str,
) -> Result<RouteResponse, String> {
    let report = kernel
        .record_forge_run_refusal_with_identity(
            ForgeRunRefusal {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                route: "/forge/runs.json",
                reason,
                repo_id: if repo_id.trim().is_empty() {
                    "mdx"
                } else {
                    repo_id
                },
                requested_run_id,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    Ok(refusal_with_receipt(reason, &report.receipt_id))
}

fn refusal_with_receipt(reason: &str, refusal_receipt_id: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-run-local-post","status":"REFUSED","reason":{},"run_started_receipt_id":"","refusal_receipt_id":{},"production_write_allowed":false}}"#,
            json_string_literal(reason),
            json_string_literal(refusal_receipt_id)
        ),
    )
}

fn missing_selected_check_toolchain_refusal(
    selected_checks: &[String],
    toolchain_readiness: &[String],
    local_toolchain_required: bool,
) -> Option<String> {
    if !local_toolchain_required {
        return None;
    }
    selected_checks
        .iter()
        .filter(|check| !check.trim().is_empty())
        .filter(|check| !crate::forge_repo_profile::is_setup_command(check))
        .filter_map(|check| {
            let key = crate::forge_repo_profile::readiness_check_key(check);
            let missing = format!("{key}=missing");
            if toolchain_readiness.iter().any(|item| item == &missing) {
                Some(format!(
                    "Forge cannot start this run because selected check `{}` needs local proof readiness `{}` but it is missing. Install or activate the repo toolchain, or choose a check this machine can run.",
                    check, key
                ))
            } else {
                None
            }
        })
        .next()
}

fn missing_selected_checks_refusal(
    selected_checks: &[String],
    repo_profile: &crate::forge_repo_profile::ForgeRepoProfile,
) -> Option<String> {
    if selected_checks.iter().any(|check| {
        !check.trim().is_empty() && !crate::forge_repo_profile::is_setup_command(check)
    }) {
        return None;
    }
    Some(format!(
        "Forge cannot start this run because no proof command was selected or inferred for language pack `{}`. {}",
        repo_profile.language_pack_id, repo_profile.proof_plan_next_action
    ))
}

fn repo_root() -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    cwd.ancestors()
        .find(|path| path.join(".git").exists())
        .map(|path| path.to_path_buf())
        .unwrap_or(cwd)
}

fn env_enabled(name: &str) -> bool {
    std::env::var(name).ok().as_deref() == Some("1")
}

fn forge_host_checks_enabled() -> bool {
    match std::env::var("MDX_FORGE_HOST_CHECKS") {
        Ok(value) => value == "1",
        Err(_) => {
            std::env::var("MDX_FORGE_DISABLE_HOST_CHECKS")
                .ok()
                .as_deref()
                != Some("1")
        }
    }
}

fn csv_json_array(values: &str) -> String {
    values
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(json_string_literal)
        .collect::<Vec<_>>()
        .join(",")
}

fn newline_json_array(values: &str) -> String {
    values
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(json_string_literal)
        .collect::<Vec<_>>()
        .join(",")
}

fn forge_model_configured(kernel: &MdxKernel) -> bool {
    let tenant_id = crate::request_security::current_verified_identity()
        .map(|identity| identity.tenant_id)
        .unwrap_or_else(|| "local_tenant".to_string());
    TurnClient::default_builder_configured_for_tenant(kernel, &tenant_id)
        || std::env::vars().any(|(key, value)| {
            key.starts_with("MDX_FLEET_BUILDER_")
                && key.ends_with("_API_KEY")
                && !value.trim().is_empty()
        })
}

pub(crate) fn mdx_work_queue_scope_preflight(
    intent: &str,
    work_item_id: &str,
    repo_root: &str,
) -> Result<Vec<String>, String> {
    let Some(items) = load_work_queue_items(repo_root) else {
        return Ok(Vec::new());
    };
    let work_item_id = work_item_id.trim();
    if !work_item_id.is_empty() {
        if let Some((_, scope)) = items.iter().find(|(id, _)| id == work_item_id) {
            return Ok(scope.clone());
        }
        return Err(format!(
            "Unknown MDx work item {work_item_id}. Pick one from generated/agents/mdx-agent-work-queue.json before spending model turns."
        ));
    }
    let targets = mentioned_repo_paths(intent);
    if targets.is_empty() {
        return Ok(Vec::new());
    }
    // Self-unblock instead of dead-ending. A senior engineer handing Forge a
    // real ask should not have to go pick a work-queue id first: when the ask
    // targets paths a work item already covers, bind that item's scope; when
    // no item covers them, grant exactly the paths the ask named. Either way
    // the run stays governed - its allowed_write_scope is recorded and every
    // off-scope write is still refused downstream - it just no longer refuses
    // to start. (External repos have no MDx queue and returned above.)
    let matches = matching_work_items(&targets, &items);
    if matches.is_empty() {
        return Ok(scope_from_named_paths(&targets));
    }
    let mut scope: Vec<String> = Vec::new();
    for (id, item_scope) in &items {
        if matches.contains(id) {
            for entry in item_scope {
                if !scope.contains(entry) {
                    scope.push(entry.clone());
                }
            }
        }
    }
    Ok(scope)
}

/// Grant ONLY the exact files an ask named when no work item pre-authorizes
/// them, so Forge can write what it was asked to touch. Deliberately does NOT
/// widen to the containing directory: a `{dir}/**` grant would hand write
/// authority over sibling files the ask never mentioned (auth, ledger,
/// migrations, CI that may live in the same directory), which is a broader and
/// less-reviewed grant than the exact named path. Used only for the MDx-self
/// dogfood target; external repos never reach here. Off-scope writes are still
/// refused downstream.
fn scope_from_named_paths(targets: &[String]) -> Vec<String> {
    let mut scope: Vec<String> = Vec::new();
    for target in targets {
        let target = target.trim_start_matches("./").trim_end_matches('/');
        if target.is_empty() || path_is_sensitive_for_self_grant(target) {
            continue;
        }
        if !scope.iter().any(|entry| entry == target) {
            scope.push(target.to_string());
        }
    }
    scope
}

/// The protection ring that self-unblock must never hand out from ask text. A
/// human ask can self-grant scope for ordinary product code, but editing
/// migrations, secrets, auth/identity wiring, ledger/receipt/evidence code, or
/// CI workflow definitions still requires a curated work item - matching the
/// PR do-not-touch boundary. Fail-closed: an uncertain match just means the
/// operator must route that file through a work item, which is the safe result.
fn path_is_sensitive_for_self_grant(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    const SENSITIVE: &[&str] = &[
        "migration",
        ".sql",
        "secret",
        "credential",
        "provider.env",
        ".env",
        ".github/",
        "workflows/",
        "request_security",
        "oidc",
        "jwks",
        "auth_verifier",
        "evidence_checkpoint",
        ".mdx-local",
    ];
    if SENSITIVE.iter().any(|needle| p.contains(needle)) {
        return true;
    }
    // Kernel-side auth, ledger, and receipt code (not UI views that merely
    // display receipts) stays behind a curated work item.
    p.starts_with("crates/")
        && (p.contains("/auth")
            || p.contains("ledger")
            || p.contains("receipt")
            || p.contains("identity"))
}

fn load_work_queue_items(repo_root: &str) -> Option<Vec<(String, Vec<String>)>> {
    let path = std::path::Path::new(repo_root).join("generated/agents/mdx-agent-work-queue.json");
    let text = std::fs::read_to_string(path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&text).ok()?;
    let items = parsed["work_items"].as_array()?;
    Some(
        items
            .iter()
            .filter_map(|item| {
                let id = item["id"].as_str()?.to_string();
                let scope = item["write_scope"]
                    .as_array()
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| value.as_str().map(str::to_string))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if scope.is_empty() {
                    None
                } else {
                    Some((id, scope))
                }
            })
            .collect(),
    )
}

fn mentioned_repo_paths(intent: &str) -> Vec<String> {
    let mut paths = Vec::new();
    for raw in intent.split_whitespace() {
        let token = raw
            .trim_matches(|c: char| {
                matches!(
                    c,
                    '`' | '"' | '\'' | ',' | ';' | ':' | '(' | ')' | '[' | ']' | '{' | '}'
                )
            })
            .trim_start_matches("./")
            .trim_end_matches('.');
        if token.contains("://") || !token.contains('/') {
            continue;
        }
        if token.starts_with('/')
            || token.starts_with("../")
            || token.contains("/../")
            || token.ends_with('/')
        {
            continue;
        }
        if token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.' | '+'))
            && !paths.iter().any(|path| path == token)
        {
            paths.push(token.to_string());
        }
    }
    paths
}

fn matching_work_items(targets: &[String], items: &[(String, Vec<String>)]) -> Vec<String> {
    let mut matches = Vec::new();
    for (id, scope) in items {
        if targets
            .iter()
            .any(|target| path_in_work_scope(target, scope))
        {
            matches.push(id.clone());
        }
    }
    matches
}

fn path_in_work_scope(path: &str, scope: &[String]) -> bool {
    let path = path.trim_start_matches("./").trim_end_matches('/');
    scope.iter().any(|entry| {
        let entry = entry.trim_start_matches("./").trim_end_matches('/');
        if entry.contains('*') {
            return simple_glob_match(entry, path);
        }
        path == entry || path.starts_with(&format!("{entry}/"))
    })
}

fn simple_glob_match(pattern: &str, value: &str) -> bool {
    let mut rest = value;
    let mut parts = pattern.split('*').peekable();
    let first = parts.next().unwrap_or("");
    if !rest.starts_with(first) {
        return false;
    }
    rest = &rest[first.len()..];
    while let Some(part) = parts.next() {
        if part.is_empty() {
            continue;
        }
        let Some(index) = rest.find(part) else {
            return false;
        };
        rest = &rest[index + part.len()..];
        if parts.peek().is_none() && !pattern.ends_with('*') {
            return rest.is_empty();
        }
    }
    pattern.ends_with('*') || rest.is_empty()
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

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":");
    let after = body.split(&marker).nth(1)?;
    let after = after.trim_start();
    let rest = after.strip_prefix('"')?;
    let mut value = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(escaped) = chars.next() {
                    value.push(escaped);
                }
            }
            '"' => return Some(value),
            other => value.push(other),
        }
    }
    None
}

fn json_u32_field(body: &str, key: &str) -> Option<u32> {
    if let Some(value) = json_string_field(body, key).and_then(|value| value.parse().ok()) {
        return Some(value);
    }
    let marker = format!("\"{key}\":");
    let after = body.split(&marker).nth(1)?.trim_start();
    let digits = after
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

fn repo_has_committed_head(repo_root: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "--verify", "HEAD"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn is_reserved_forge_run_id(run_id: &str) -> bool {
    run_id.starts_with("forge_run_")
        && run_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn forge_run_id_exists(kernel: &MdxKernel, run_id: &str) -> bool {
    kernel.ledger().entries().iter().any(|receipt| {
        receipt.kind == "forge.run.event" && receipt_payload_value(receipt, "run_id") == run_id
    })
}

fn receipt_payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_pending_run_is_not_reported_as_still_working() {
        let fold = RunFold {
            latest_event: "run_recovery_pending".to_string(),
            ..RunFold::default()
        };
        assert_eq!(fold.derived_status(), "interrupted");
        assert_eq!(operator_status(&fold), "needs_you");
    }

    #[test]
    fn cloud_setup_receipts_do_not_appear_as_active_builds() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:dev");
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_cloud_environment_example",
                    event: "evidence_appended",
                    work_item_id: "mobile_cloud_environment",
                    detail: "Cloud environment definition recorded",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[("cloud_record_kind", "cloud_environment")],
            )
            .expect("cloud environment receipt");
        let kernel = Arc::new(RwLock::new(kernel));

        let response = handle_projection("GET", &kernel).expect("projection");

        assert!(
            !response
                .body
                .contains("forge_run_cloud_environment_example")
        );
        assert!(response.body.contains(r#""runs":[]"#));
    }
    use mdx_core::{
        ApproveModelTurnOn, ForgeLongHorizonMissionAdmission, ForgeOutcomeSignal, ForgeRepoConnect,
        ForgeRunControl, ForgeRunEvent, GovernedWriteIdentity, TwinModelGatewayProviderObservation,
    };
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn route_model_test_guard() -> std::sync::MutexGuard<'static, ()> {
        crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    #[test]
    fn start_route_records_receipt_for_refused_request() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = handle_start(
            "POST",
            r#"{"actor_id":"human:sim_persona_005","actor_role":"owner"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains(r#""refusal_receipt_id":"#));
        let guard = kernel.read().expect("kernel");
        let receipts = guard.ledger().query().by_kind("forge.run.refused");
        let receipt = receipts.first().expect("refusal receipt");
        assert_eq!(receipt.actor_id.as_str(), "human:sim_persona_005");
        assert_eq!(
            receipt.payload.get("reason").map(String::as_str),
            Some("a run needs an intent - what should the build agent do?")
        );
    }

    #[test]
    fn start_route_refuses_global_bench_open_command_posture() {
        let _guard = route_model_test_guard();
        let previous = crate::forge_loop_runner::set_bench_open_commands_test_override(Some(true));
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = handle_start(
            "POST",
            r#"{"actor_id":"human:dev","actor_role":"owner","intent":"fix src/service.rs"}"#,
            &kernel,
        )
        .expect("route");
        crate::forge_loop_runner::set_bench_open_commands_test_override(previous);

        assert!(
            response.body.contains(r#""status":"REFUSED""#),
            "{}",
            response.body
        );
        assert!(
            response.body.contains("mdx-server forge-bench"),
            "{}",
            response.body
        );
        let guard = kernel.read().expect("kernel");
        let receipts = guard.ledger().query().by_kind("forge.run.refused");
        let receipt = receipts.first().expect("refusal receipt");
        assert!(
            receipt
                .payload
                .get("reason")
                .map(String::as_str)
                .unwrap_or("")
                .contains("Benchmark open-command posture")
        );
    }

    #[test]
    fn start_route_refuses_a_named_envelope_that_is_not_active() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        // Naming an envelope binds the run to it, so a nonexistent (or
        // revoked, or expired) envelope refuses admission with a receipt
        // instead of decorating the run - authority is consulted, not
        // recorded-and-ignored.
        let response = handle_start(
            "POST",
            r#"{"actor_id":"human:dev","actor_role":"owner","intent":"fix the thing","envelope_id":"env_never_recorded"}"#,
            &kernel,
        )
        .expect("route");
        assert!(
            response.body.contains(r#""status":"REFUSED""#),
            "{}",
            response.body
        );
        assert!(
            response
                .body
                .contains("autonomy envelope refused: envelope_not_found"),
            "{}",
            response.body
        );
        let guard = kernel.read().expect("kernel");
        let receipts = guard.ledger().query().by_kind("forge.run.refused");
        assert!(
            receipts.iter().any(|receipt| receipt
                .payload
                .get("reason")
                .is_some_and(|reason| reason.contains("envelope_not_found"))),
            "refusal must land on the chain"
        );
    }

    #[test]
    fn start_route_refuses_unknown_repo_id_with_receipt() {
        let _guard = route_model_test_guard();
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = handle_start(
            "POST",
            r#"{"intent":"fix src/service.rs","repo_id":"missing-repo"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("missing-repo"));
        assert!(response.body.contains(r#""refusal_receipt_id":"#));
        let guard = kernel.read().expect("kernel");
        let receipts = guard.ledger().query().by_kind("forge.run.refused");
        let receipt = receipts.first().expect("refusal receipt");
        assert_eq!(
            receipt.payload.get("repo_id").map(String::as_str),
            Some("missing-repo")
        );
        assert_eq!(guard.ledger().query().by_kind("forge.run.event").len(), 0);
    }

    #[test]
    fn projection_requires_an_explicit_receipt_to_conclude_system_evidence() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            let identity = GovernedWriteIdentity::local_demo("agent:forge_eval");
            guard
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "agent:forge_eval",
                        run_id: "forge_system_evidence_contract",
                        event: "evidence_appended",
                        work_item_id: "system_evidence_contract",
                        detail: "arbitrary human prose that names no status",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &identity,
                    &[],
                )
                .expect("system evidence");
        }

        let response = handle_projection("GET", &kernel).expect("projection");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("valid projection json");
        let run = &parsed["runs"][0];
        assert_eq!(run["status"], "running");
        assert_eq!(run["terminal_state"], "IN_PROGRESS");
        assert_eq!(run["finished"], false);
        assert_eq!(run["origin"], "system");

        {
            let mut guard = kernel.write().expect("kernel");
            let identity = GovernedWriteIdentity::local_demo("agent:forge_eval");
            guard
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "agent:forge_eval",
                        run_id: "forge_system_evidence_contract",
                        event: "run_finished",
                        work_item_id: "system_evidence_contract",
                        detail: "arbitrary human conclusion prose",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &identity,
                    &[("terminal_state", "RECORDED")],
                )
                .expect("recorded conclusion");
        }

        let response = handle_projection("GET", &kernel).expect("projection");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("valid projection json");
        let run = &parsed["runs"][0];
        assert_eq!(run["status"], "recorded");
        assert_eq!(run["terminal_state"], "RECORDED");
        assert_eq!(run["finished"], true);
    }

    #[test]
    fn projection_exposes_live_forge_run_contract() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            let identity = GovernedWriteIdentity::local_demo("human:dev");
            guard
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "human:dev",
                        run_id: "forge_run_live_contract_running",
                        event: "run_started",
                        work_item_id: "wi_live_contract_running",
                        detail: "Build the live run surface",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &identity,
                    &[
                        ("repo_id", "mdx-native-harness-elevation"),
                        ("repo_root", "/repo"),
                        ("repo_primary_language", "rust"),
                        ("language_pack_id", "rust-cargo"),
                        ("selected_checks", "cargo test -p mdx-core"),
                        ("builder_casting_selected_provider_family", "xai"),
                        ("builder_casting_selected_model_id", "grok-4.3"),
                        ("builder_casting_selected_slot", "GROK"),
                    ],
                )
                .expect("running start");
            guard
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_live_contract_running",
                    event: "tool_executed",
                    work_item_id: "wi_live_contract_running",
                    detail: "read_file crates/mdx-server/src/forge_run_route.rs",
                    turn: 1,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("running event");
            guard
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "human:dev",
                        run_id: "forge_run_live_contract_done",
                        event: "run_started",
                        work_item_id: "wi_live_contract_done",
                        detail: "Ship the live run surface",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &identity,
                    &[
                        ("repo_id", "mdx-native-harness-elevation"),
                        ("repo_root", "/repo"),
                        ("repo_primary_language", "rust"),
                        ("language_pack_id", "rust-cargo"),
                        ("selected_checks", "cargo test -p mdx-core"),
                        ("builder_casting_selected_provider_family", "anthropic"),
                        ("builder_casting_selected_model_id", "claude-opus-4.8"),
                        ("builder_casting_selected_slot", "OPUS"),
                    ],
                )
                .expect("done start");
            guard
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_live_contract_done",
                    event: "check_passed",
                    work_item_id: "wi_live_contract_done",
                    detail: "run_command cargo test -p mdx-core exit=0",
                    turn: 1,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("done check");
            guard
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_live_contract_done",
                    event: "evidence_appended",
                    work_item_id: "wi_live_contract_done",
                    detail: "branch=forge/run-live diff=ready files_changed=2 paths=docs/FORGE-MACOS-DOGFOOD-RUN.md,crates/mdx-server/src/forge_run_route.rs",
                    turn: 1,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("done evidence");
            guard
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_live_contract_done",
                    event: "run_finished",
                    work_item_id: "wi_live_contract_done",
                    detail: "status=RUN_FINISHED_DONE files_changed=2",
                    turn: 2,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("done finish");
            guard
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "agent:forge_eval",
                    run_id: "forge_run_live_contract_done",
                    event: "evidence_appended",
                    work_item_id: "wi_live_contract_done",
                    detail: "eval_quality_gate_assessed status=READY_FOR_PRINCIPAL_REVIEW machine_gates_passed=true failed_gates=none",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("post-run eval evidence");
        }

        let response = handle_projection("GET", &kernel).expect("projection");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("valid projection json");
        assert_eq!(parsed["run_count"].as_u64(), Some(2));
        assert!(response.body.contains(r#""operator_status":"working""#));
        assert!(
            response
                .body
                .contains(r#""operator_status":"ready_for_review""#)
        );
        let done_run = parsed["runs"]
            .as_array()
            .expect("runs array")
            .iter()
            .find(|run| run["run_id"].as_str() == Some("forge_run_live_contract_done"))
            .expect("done run");
        assert_eq!(done_run["turns"].as_u64(), Some(2));
        assert!(response.body.contains(r#""key":"proof","label":"Proving""#));
        assert!(
            response
                .body
                .contains(r#""proof":{"checks":[{"name":"cargo test -p mdx-core","state":"pass"}"#)
        );
        assert!(response.body.contains(r#""diff":{"ready":true"#));
        assert!(
            response
                .body
                .contains(r#""changed_paths":["docs/FORGE-MACOS-DOGFOOD-RUN.md","crates/mdx-server/src/forge_run_route.rs"]"#)
        );
        assert!(
            response
                .body
                .contains(r#""diff_route":"/forge/run-diff.json""#)
        );
        assert!(response.body.contains(r#""action":"steer","allowed":true"#));
        assert!(
            response
                .body
                .contains(r#""action":"review","allowed":true"#)
        );
        assert!(response.body.contains(r#""receipt_route":"/receipts/"#));
        assert!(
            response
                .body
                .contains(r#""summary":"Read crates/mdx-server/src/forge_run_route.rs""#)
        );
        assert!(response.body.contains(r#""model_class":"frontier_strong""#));
    }

    #[test]
    fn projection_prefers_operator_intent_and_run_summary_over_metadata() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            let identity = GovernedWriteIdentity::local_demo("human:dev");
            guard
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "human:dev",
                        run_id: "forge_run_human_voice",
                        event: "run_started",
                        work_item_id: "wi_human_voice",
                        detail: "accepted: 2 selected_checks language_pack=node repo_id=mdx",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &identity,
                    &[
                        (
                            "operator_intent",
                            "Build a sample MDx Forge page and add a simple game.",
                        ),
                        (
                            "operator_ask",
                            "Build a sample MDx Forge page and add a simple game.",
                        ),
                        (
                            "run_title",
                            "Build a sample MDx Forge page and add a simple game.",
                        ),
                        ("repo_id", "mdx-native-harness-elevation"),
                    ],
                )
                .expect("start");
            guard
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_human_voice",
                    event: "agent_turn_prose",
                    work_item_id: "wi_human_voice",
                    detail: "I will create the page and add a tiny Rock-Paper-Scissors interaction.",
                    turn: 1,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("agent prose");
            guard
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "human:dev",
                        run_id: "forge_run_human_voice",
                        event: "run_finished",
                        work_item_id: "wi_human_voice",
                        detail: "status=RUN_FINISHED_DONE turns=4 files_changed=3",
                        turn: 4,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &identity,
                    &[
                        (
                            "run_summary",
                            "Built the sample page with the requested copy and added a simple game. Checks passed.",
                        ),
                        (
                            "operator_run_summary",
                            "Yo, built you the sample page and threw in a simple game. Checks passed.",
                        ),
                        ("run_summary_source", "finish_summary"),
                        ("forge_run_voice_profile_id", "md_chill"),
                        ("voice_rewrite_status", "rewritten"),
                        ("voice_rewrite_model_id", "gpt-5-mini"),
                    ],
                )
                .expect("finish");
        }

        let response = handle_projection("GET", &kernel).expect("projection");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("valid projection json");
        let run = &parsed["runs"][0];
        assert_eq!(
            run["intent"].as_str(),
            Some("Build a sample MDx Forge page and add a simple game.")
        );
        assert_eq!(
            run["run_title"].as_str(),
            Some("Build a sample MDx Forge page and add a simple game.")
        );
        assert_eq!(
            run["run_summary"].as_str(),
            Some(
                "Built the sample page with the requested copy and added a simple game. Checks passed."
            )
        );
        assert_eq!(
            run["operator_run_summary"].as_str(),
            Some("Yo, built you the sample page and threw in a simple game. Checks passed.")
        );
        assert_eq!(run["forge_run_voice_profile_id"].as_str(), Some("md_chill"));
        assert_eq!(run["voice_rewrite_status"].as_str(), Some("rewritten"));
        assert_eq!(run["voice_rewrite_model_id"].as_str(), Some("gpt-5-mini"));
        assert!(response.body.contains(r#""event":"agent_turn_prose""#));
        assert!(
            !run["intent"]
                .as_str()
                .unwrap_or_default()
                .contains("selected_checks")
        );
    }

    #[test]
    fn projection_uses_observed_builder_model_when_casting_metadata_is_absent() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            guard
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_observed_builder_model",
                    event: "run_started",
                    work_item_id: "wi_observed_builder_model",
                    detail: "Build a small branch",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("start");
            guard
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_observed_builder_model",
                    event: "model_called",
                    work_item_id: "wi_observed_builder_model",
                    detail: "model=grok-4.5 finish_reason=tool_calls tool_calls=2",
                    turn: 1,
                    input_tokens: 2000,
                    output_tokens: 200,
                })
                .expect("builder model call");
            guard
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "human:dev",
                        run_id: "forge_run_observed_builder_model",
                        event: "model_called",
                        work_item_id: "wi_observed_builder_model",
                        detail: "voice rewrite model=gpt-5-mini",
                        turn: 2,
                        input_tokens: 500,
                        output_tokens: 80,
                    },
                    &GovernedWriteIdentity::local_demo("human:dev"),
                    &[("voice_rewrite_model_id", "gpt-5-mini")],
                )
                .expect("voice rewrite call");
        }

        let response = handle_projection("GET", &kernel).expect("projection");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("valid projection json");
        let run = &parsed["runs"][0];
        assert_eq!(
            run["model_or_worker"]["provider_family"].as_str(),
            Some("xai")
        );
        assert_eq!(
            run["model_or_worker"]["model_id"].as_str(),
            Some("grok-4.5")
        );
        assert_eq!(run["voice_rewrite_model_id"].as_str(), Some("gpt-5-mini"));
    }

    #[test]
    fn restored_run_projection_keeps_authored_title_intent_and_controls() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:dev");
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_restored_fields",
                    event: "run_started",
                    work_item_id: "wi_restored_fields",
                    detail: "accepted restored run",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[
                    ("operator_intent", "Fix restored run fields."),
                    ("operator_ask", "Fix restored run fields."),
                    ("run_title", "Fix restored run fields."),
                    (
                        "selected_checks",
                        "cargo test -p mdx-server forge_run_route",
                    ),
                ],
            )
            .expect("start");
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:dev",
                run_id: "forge_run_restored_fields",
                event: "run_finished",
                work_item_id: "wi_restored_fields",
                detail: "status=RUN_FINISHED_DONE turns=2 files_changed=1",
                turn: 2,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("finish");

        let entries = kernel.ledger().entries().to_vec();
        let mut restored = MdxKernel::boot_local();
        restored
            .restore_ledger_entries(entries)
            .expect("restore ledger");
        let restored = Arc::new(RwLock::new(restored));
        let response = handle_projection("GET", &restored).expect("projection");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("valid projection json");
        let run = &parsed["runs"][0];
        assert_eq!(run["intent"].as_str(), Some("Fix restored run fields."));
        assert_eq!(run["run_title"].as_str(), Some("Fix restored run fields."));
        assert!(
            run["controls"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert!(
            run["allowed_controls"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item.as_str() == Some("review")))
        );
    }

    #[test]
    fn no_change_projection_does_not_allow_ship_without_branch() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            guard
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_no_change",
                    event: "run_started",
                    work_item_id: "wi_no_change",
                    detail: "accepted no change run",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("start");
            guard
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_no_change",
                    event: "run_finished",
                    work_item_id: "wi_no_change",
                    detail: "status=RUN_FINISHED_NO_CHANGE turns=3 files_changed=0 check_runs=1",
                    turn: 3,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("finish");
        }

        let response = handle_projection("GET", &kernel).expect("projection");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("valid projection json");
        let run = &parsed["runs"][0];
        assert_eq!(run["status"].as_str(), Some("no_change"));
        assert_eq!(run["operator_status"].as_str(), Some("ready_for_review"));
        assert!(
            run["allowed_controls"]
                .as_array()
                .is_some_and(|items| items.iter().all(|item| item.as_str() != Some("ship")))
        );
    }

    #[test]
    fn green_no_change_parallel_candidate_is_reviewable() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:dev");
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_group_primary",
                    event: "run_started",
                    work_item_id: "wi_group",
                    detail: "accepted primary candidate",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[
                    (
                        "parallel_candidate_primary_run_id",
                        "forge_run_group_primary",
                    ),
                    ("parallel_candidate_index", "1"),
                    ("parallel_candidate_count", "2"),
                    ("parallel_candidate_role", "primary"),
                    ("execution_geometry_effective_workers", "2"),
                ],
            )
            .expect("primary start");
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:dev",
                run_id: "forge_run_group_primary",
                event: "check_passed",
                work_item_id: "wi_group",
                detail: "run_command cargo test -p mdx-server forge_run_route exit=0",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("primary check");
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:dev",
                run_id: "forge_run_group_primary",
                event: "run_finished",
                work_item_id: "wi_group",
                detail: "status=RUN_FINISHED_NO_CHANGE turns=2 files_changed=0 check_runs=1",
                turn: 2,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("primary finish");
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_group_candidate",
                    event: "run_started",
                    work_item_id: "wi_group",
                    detail: "accepted secondary candidate",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[
                    (
                        "parallel_candidate_primary_run_id",
                        "forge_run_group_primary",
                    ),
                    ("parallel_candidate_index", "2"),
                    ("parallel_candidate_count", "2"),
                    ("parallel_candidate_role", "candidate"),
                    ("execution_geometry_effective_workers", "2"),
                ],
            )
            .expect("candidate start");
        kernel
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "human:dev",
                run_id: "forge_run_group_candidate",
                event: "run_finished",
                work_item_id: "wi_group",
                detail: "status=RUN_BUDGET_EXHAUSTED turns=12",
                turn: 12,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("candidate finish");

        let kernel = Arc::new(RwLock::new(kernel));
        let response = handle_projection("GET", &kernel).expect("projection");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("valid projection json");
        let group = &parsed["parallel_execution_groups"][0];
        assert_eq!(group["selection_status"].as_str(), Some("ready_for_review"));
        assert_eq!(
            group["recommended_run_id"].as_str(),
            Some("forge_run_group_primary")
        );
        assert_eq!(
            group["candidates"][0]["score_basis"].as_str(),
            Some("verified_no_change_candidate")
        );
    }

    #[test]
    fn stopped_control_closes_restored_running_run_projection() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:dev");
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_restored_stuck",
                    event: "run_started",
                    work_item_id: "wi_restored_stuck",
                    detail: "accepted restored stuck run",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &identity,
                &[
                    ("operator_intent", "Sweep a restored stuck run."),
                    ("operator_ask", "Sweep a restored stuck run."),
                    ("run_title", "Sweep a restored stuck run."),
                ],
            )
            .expect("start");
        let entries = kernel.ledger().entries().to_vec();
        let mut restored = MdxKernel::boot_local();
        restored
            .restore_ledger_entries(entries)
            .expect("restore ledger");
        restored
            .record_forge_run_control(ForgeRunControl {
                tenant_id: "local_tenant",
                actor_id: "human:dev",
                run_id: "forge_run_restored_stuck",
                control: "stop",
                note: "kernel restart left this run abandoned",
            })
            .expect("stop control");

        let restored = Arc::new(RwLock::new(restored));
        let response = handle_projection("GET", &restored).expect("projection");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("valid projection json");
        let run = &parsed["runs"][0];
        assert_eq!(run["status"].as_str(), Some("stopped"));
        assert_eq!(run["operator_status"].as_str(), Some("stopped"));
        assert_eq!(run["finished"].as_bool(), Some(true));
        assert_eq!(run["latest_event"].as_str(), Some("operator_stop"));
        assert!(
            run["final_line"]
                .as_str()
                .is_some_and(|line| line.contains("RUN_STOPPED_BY_OPERATOR"))
        );
        assert!(
            run["allowed_controls"]
                .as_array()
                .is_some_and(|items| items.is_empty())
        );
    }

    #[test]
    fn projection_recovers_legacy_authored_fields_from_non_start_receipts() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            let identity = GovernedWriteIdentity::local_demo("human:dev");
            guard
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "human:dev",
                        run_id: "forge_run_legacy_fields",
                        event: "run_finished",
                        work_item_id: "wi_legacy_fields",
                        detail: "status=RUN_FINISHED_CANNOT_PROCEED turns=1",
                        turn: 1,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &identity,
                    &[
                        ("operator_intent", "Recover legacy projection fields."),
                        ("run_title", "Recover legacy projection fields."),
                    ],
                )
                .expect("finish");
        }

        let response = handle_projection("GET", &kernel).expect("projection");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("valid projection json");
        let run = &parsed["runs"][0];
        assert_eq!(
            run["operator_intent"].as_str(),
            Some("Recover legacy projection fields.")
        );
        assert_eq!(
            run["run_title"].as_str(),
            Some("Recover legacy projection fields.")
        );
        assert!(
            run["controls"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
    }

    #[test]
    fn projection_recovers_external_review_branch_and_quarantine_from_terminal_receipt() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            let identity = GovernedWriteIdentity::local_demo("human:dev");
            guard
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "human:dev",
                        run_id: "forge_run_external_projection",
                        event: "run_started",
                        work_item_id: "external_projection",
                        detail: "accepted",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &identity,
                    &[
                        ("runner_profile_runner_id", "grok_build_cli_external_worker"),
                        ("runner_profile_runner_kind", "external_harness"),
                        ("runner_profile_display_name", "Grok Build"),
                        ("runner_profile_adapter_kind", "grok_build_cli"),
                    ],
                )
                .expect("start");
            guard
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "human:dev",
                        run_id: "forge_run_external_projection",
                        event: "run_finished",
                        work_item_id: "external_projection",
                        detail: "status=RUN_FINISHED_DONE files_changed=1 branch=forge/run-external external_harness=grok_build",
                        turn: 1,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &identity,
                    &[],
                )
                .expect("finish");
        }

        let response = handle_projection("GET", &kernel).expect("projection");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("valid projection json");
        let run = &parsed["runs"][0];
        assert_eq!(run["branch"], "forge/run-external");
        assert_eq!(
            run["machine_runtime"]["compatibility_status"],
            "runtime_unverified"
        );
        assert_eq!(
            run["quarantine"]["status"],
            "output_quarantined_pending_mdx_gates"
        );
        assert_eq!(run["quarantine"]["output_quarantined"], true);
        assert_eq!(run["quarantine"]["external_output_consumable"], false);
    }

    #[test]
    fn projection_marks_accepted_machine_league_output_as_scoreboard_only() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            let identity = GovernedWriteIdentity::local_demo("agent:forge_eval");
            guard
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "agent:forge_eval",
                        run_id: "forge_machine_league_codex_trial_accepted",
                        event: "run_started",
                        work_item_id: "machine_league_codex_trial",
                        detail: "machine_league_trial runner=codex_cli_external_worker",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &identity,
                    &[
                        ("machine_league_trial", "true"),
                        ("machine_league_run_kind", "live_external_trial"),
                        ("runner_profile_runner_id", "codex_cli_external_worker"),
                        ("runner_profile_display_name", "Codex CLI"),
                        ("quarantine_status", "output_quarantined_pending_mdx_gates"),
                        ("quarantine_output_quarantined", "true"),
                        ("quarantine_external_output_consumable", "false"),
                        (
                            "quarantine_acceptance_gate",
                            "mdx_quality_gates_then_principal_review",
                        ),
                        ("quarantine_blocked_reason", "pending_mdx_quality_gates"),
                    ],
                )
                .expect("trial start");
            guard
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "agent:forge_eval",
                        run_id: "forge_machine_league_codex_trial_accepted",
                        event: "evidence_appended",
                        work_item_id: "forge_eval_principal_review",
                        detail: "eval_principal_reviewed status=accepted_for_scoreboard",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &identity,
                    &[
                        ("eval_principal_review_status", "accepted_for_scoreboard"),
                        (
                            "eval_result_receipt_id",
                            "forge_fleet_eval_result_receipt_1",
                        ),
                        ("total_score", "94"),
                        ("accepted_for_scoreboard", "true"),
                        ("external_output_consumable", "false"),
                    ],
                )
                .expect("principal review");
        }

        let response = handle_projection("GET", &kernel).expect("projection");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("valid projection json");
        let run = parsed["runs"]
            .as_array()
            .expect("runs")
            .iter()
            .find(|run| run["run_id"].as_str() == Some("forge_machine_league_codex_trial_accepted"))
            .expect("accepted trial");

        assert_eq!(
            run["quarantine"]["status"].as_str(),
            Some("accepted_for_scoreboard_output_still_quarantined")
        );
        assert_eq!(
            run["quarantine"]["accepted_for_scoreboard"].as_bool(),
            Some(true)
        );
        assert_eq!(
            run["quarantine"]["external_output_consumable"].as_bool(),
            Some(false)
        );
        assert_eq!(
            run["quarantine"]["blocked_reason"].as_str(),
            Some("accepted_scoreboard_evidence_not_production_output")
        );
        assert_eq!(
            run["quarantine"]["scorecard_total_score"].as_str(),
            Some("94")
        );
    }

    fn admit_test_mission(kernel: &mut MdxKernel) {
        kernel
            .admit_forge_long_horizon_mission_local(ForgeLongHorizonMissionAdmission {
                tenant_id: "local_tenant",
                actor_id: "human:dev",
                actor_role: "operator",
                mission_id: "mission_for_run",
                goal: "Prove a mission-attached Forge run",
                non_goals: "no provider calls,no production writes",
                constraints: "record checkpoints from governed outcomes",
                done_when: "the attached run updates milestone progress",
                allowed_write_scope: "crates/mdx-server/src/",
                blocked_paths: "provider secrets,production state",
                validation_commands: "cargo test -p mdx-server forge_run_route",
                model_policy: "mdx_approved_responses_compatible_provider_profiles_only",
                provider_allowlist: "xai,anthropic,gemini,aws_bedrock",
                fleet_width: 1,
                max_runtime_ms: 60_000,
                max_cost_cents: 100,
                checkpoint_cadence_minutes: 15,
            })
            .expect("mission admitted");
    }

    #[test]
    fn json_u32_field_accepts_number_and_string_values() {
        assert_eq!(json_u32_field(r#"{"max_turns":12}"#, "max_turns"), Some(12));
        assert_eq!(
            json_u32_field(r#"{"max_turns":"13"}"#, "max_turns"),
            Some(13)
        );
    }

    #[test]
    fn repo_head_preflight_requires_an_initial_commit() {
        let repo = TempRepo::empty();
        std::fs::create_dir_all(repo.path()).expect("repo dir");
        std::process::Command::new("git")
            .arg("init")
            .current_dir(repo.path())
            .output()
            .expect("git init");

        assert!(!repo_has_committed_head(repo.path()));

        std::fs::write(repo.path().join("README.md"), "canary\n").expect("readme");
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(repo.path())
            .output()
            .expect("git add");
        std::process::Command::new("git")
            .args([
                "-c",
                "user.name=Forge Test",
                "-c",
                "user.email=forge-test@example.com",
                "commit",
                "-m",
                "initial",
            ])
            .current_dir(repo.path())
            .output()
            .expect("git commit");

        assert!(repo_has_committed_head(repo.path()));
    }

    #[test]
    fn projection_includes_outcome_signal_and_local_real_boundary() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut k = kernel.write().expect("kernel");
            let repo_evidence = [
                ("repo_id", "swift_canary"),
                ("repo_primary_language", "swift"),
                ("language_pack_id", "swift-spm"),
                ("repo_profile_detected_files", "Package.swift"),
                (
                    "repo_profile_quality_signals",
                    "ci:github-actions,tests:swiftpm-tests",
                ),
                (
                    "repo_profile_standards_sources",
                    "AGENTS.md,.github/CODEOWNERS",
                ),
                (
                    "repo_profile_standards_source_fingerprints",
                    "AGENTS.md=fnv1a64:1111111111111111,.github/CODEOWNERS=fnv1a64:2222222222222222",
                ),
                (
                    "repo_profile_standards_source_summaries",
                    "AGENTS.md=agent_instructions+mentions_tests,.github/CODEOWNERS=ownership_review+mentions_review",
                ),
                (
                    "repo_profile_review_axes",
                    "behavioral-xctest-coverage,idiomatic-swift-api-shape",
                ),
                (
                    "repo_profile_principal_review_gates",
                    "behavior:changed_behavior_is_explicit,tests:checks_cover_changed_behavior",
                ),
                (
                    "repo_profile_language_pack_guidance",
                    "source_of_truth:package_swift_targets_tests,proof:swift_test",
                ),
                (
                    "repo_profile_toolchain_readiness",
                    "tool:swift=available,check:swift_test=available",
                ),
                ("repo_profile_suggested_checks", "swift test"),
                ("repo_profile_artifact_patterns", ".build/**,DerivedData/**"),
                ("repo_profile_proof_plan_status", "ready"),
                ("repo_profile_proof_plan_next_action", "Run swift test."),
                (
                    "repo_profile_proof_plan_summary",
                    "Forge inferred swift test and the local Swift toolchain is available.",
                ),
                ("selected_checks", "swift test"),
                ("selected_checks_source", "repo_profile_inferred"),
                ("work_classification_recommended_shape", "run"),
                ("work_classification_task_class", "feature"),
                ("work_classification_complexity_tier", "medium"),
                ("work_classification_confidence_pct", "82"),
                (
                    "work_classification_rationale",
                    "matched a bounded work class, so Forge can start with the normal run flow",
                ),
                (
                    "work_classification_source",
                    "mdx_core::classify_forge_work",
                ),
                ("work_classification_grants_execution_authority", "false"),
                ("language_task_corpus_id", "swift-spm-feature-medium"),
                ("language_task_alignment_status", "exact_class_and_tier"),
                (
                    "language_task_alignment_source",
                    "mdx_core::forge_language_task_corpus",
                ),
                ("language_task_class", "feature"),
                ("language_task_complexity_tier", "medium"),
                ("language_task_visible_check", "swift test"),
                ("language_task_hidden_check_slot", "API behavior fixture"),
                ("language_task_artifact_noise_expected", ".build/**"),
                (
                    "language_task_required_principal_review_gates",
                    "behavior:changed_behavior_is_explicit,tests:checks_cover_changed_behavior",
                ),
                (
                    "language_task_engineering_facets",
                    "behavior design, integration fit, regression coverage",
                ),
                (
                    "language_task_evaluation_oracle",
                    "visible native check plus hidden integration or compatibility fixture",
                ),
                ("language_task_human_timebox_minutes", "90"),
                (
                    "language_task_contamination_policy",
                    "fresh repo fixture or held-out mutation required; no training-set benchmark claim accepted as sole evidence",
                ),
                (
                    "language_task_alignment_grants_execution_authority",
                    "false",
                ),
                ("repo_profile_source", "mdx_server::forge_repo_profile"),
                ("repo_profile_grants_execution_authority", "false"),
                ("suggested_checks_are_authority", "false"),
                ("principal_orientation_gate_required", "true"),
                ("principal_orientation_gate_tool", "semantic_query"),
                ("principal_orientation_gate_grants_authority", "false"),
                ("execution_backend_kind", "hosted_sandbox"),
                ("cloud_environment_id", "cloud_projection"),
                ("execution_geometry_requested_workers", "4"),
                ("execution_geometry_effective_workers", "4"),
                ("execution_geometry_lane", "bounded_parallel_exploration"),
                ("execution_geometry_route", "/forge/runs.json"),
                (
                    "execution_geometry_reason",
                    "direct_run_width_recorded_for_bounded_parallel_execution",
                ),
                ("execution_geometry_fleet_required", "false"),
                ("execution_geometry_grants_execution_authority", "false"),
                ("parallel_candidate_primary_run_id", "forge_run_projection"),
                ("parallel_candidate_index", "1"),
                ("parallel_candidate_count", "4"),
                (
                    "parallel_candidate_write_scope",
                    "Sources/App/Slug.swift\nTests/AppTests/SlugTests.swift",
                ),
                ("parallel_candidate_strategy_id", "minimal_safe_patch"),
                (
                    "parallel_candidate_strategy_summary",
                    "smallest correct diff with the lowest integration risk",
                ),
            ];
            let started = k
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "t",
                        actor_id: "human:dev",
                        run_id: "forge_run_projection",
                        event: "run_started",
                        work_item_id: "wi_1",
                        detail: "accepted",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &GovernedWriteIdentity::local_demo("human:dev"),
                    &repo_evidence,
                )
                .expect("run started");
            k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_run_projection",
                event: "run_finished",
                work_item_id: "wi_1",
                detail: "status=RUN_FINISHED_DONE turns=1 files_changed=1",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("run finished");
            k.record_forge_outcome_signal(ForgeOutcomeSignal {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_run_projection",
                source_receipt_id: &started.receipt_id,
                source_receipt_kind: "forge.run.event",
                disposition: "completed",
                summary: "Projection test finished.",
                capability_ids: "",
                model_or_worker: "local_forge_worker",
                lesson_candidate: "Cite outcome signals in future planning.",
                lesson_source: "",
                message_channel_id: "forge",
            })
            .expect("outcome signal");
        }
        let response = handle_projection("GET", &kernel).expect("projection");
        assert_eq!(response.status, "200 OK");
        assert!(
            response
                .body
                .contains("\"outcome_receipt_kind\":\"forge.outcome.signal.recorded\"")
        );
        assert!(
            response
                .body
                .contains("\"outcome_disposition\":\"completed\"")
        );
        assert!(
            response
                .body
                .contains("Cite outcome signals in future planning.")
        );
        assert!(response.body.contains("\"worktree_isolated\":true"));
        assert!(response.body.contains("\"live_repo_mutated\":false"));
        assert!(response.body.contains("\"deployment_allowed\":false"));
        assert!(response.body.contains("\"repo_id\":\"swift_canary\""));
        assert!(
            response
                .body
                .contains("\"repo_primary_language\":\"swift\"")
        );
        assert!(response.body.contains("\"language_pack_id\":\"swift-spm\""));
        assert!(
            response
                .body
                .contains("\"repo_profile_detected_files\":[\"Package.swift\"]")
        );
        assert!(response.body.contains(
            "\"repo_profile_quality_signals\":[\"ci:github-actions\",\"tests:swiftpm-tests\"]"
        ));
        assert!(
            response.body.contains(
                "\"repo_profile_standards_sources\":[\"AGENTS.md\",\".github/CODEOWNERS\"]"
            )
        );
        assert!(response.body.contains(
            "\"repo_profile_standards_source_fingerprints\":[\"AGENTS.md=fnv1a64:1111111111111111\",\".github/CODEOWNERS=fnv1a64:2222222222222222\"]"
        ));
        assert!(response.body.contains(
            "\"repo_profile_standards_source_summaries\":[\"AGENTS.md=agent_instructions+mentions_tests\",\".github/CODEOWNERS=ownership_review+mentions_review\"]"
        ));
        assert!(response.body.contains(
            "\"repo_profile_review_axes\":[\"behavioral-xctest-coverage\",\"idiomatic-swift-api-shape\"]"
        ));
        assert!(response.body.contains(
            "\"repo_profile_principal_review_gates\":[\"behavior:changed_behavior_is_explicit\",\"tests:checks_cover_changed_behavior\"]"
        ));
        assert!(response.body.contains(
            "\"repo_profile_language_pack_guidance\":[\"source_of_truth:package_swift_targets_tests\",\"proof:swift_test\"]"
        ));
        assert!(response.body.contains(
            "\"repo_profile_toolchain_readiness\":[\"tool:swift=available\",\"check:swift_test=available\"]"
        ));
        assert!(
            response
                .body
                .contains("\"repo_profile_suggested_checks\":[\"swift test\"]")
        );
        assert!(response.body.contains(
            "\"work_classification\":{\"recommended_shape\":\"run\",\"task_class\":\"feature\",\"complexity_tier\":\"medium\",\"confidence_pct\":82"
        ));
        assert!(response
            .body
            .contains("\"language_task_alignment\":{\"task_corpus_id\":\"swift-spm-feature-medium\",\"status\":\"exact_class_and_tier\""));
        assert!(
            response
                .body
                .contains("\"hidden_check_slot\":\"API behavior fixture\"")
        );
        assert!(
            response
                .body
                .contains("\"grants_execution_authority\":false")
        );
        assert!(response.body.contains(
            "\"repo_profile_proof_plan\":{\"status\":\"ready\",\"next_action\":\"Run swift test.\",\"summary\":\"Forge inferred swift test and the local Swift toolchain is available.\"}"
        ));
        assert!(
            response
                .body
                .contains("\"selected_checks\":[\"swift test\"]")
        );
        assert!(
            response
                .body
                .contains("\"selected_checks_source\":\"repo_profile_inferred\"")
        );
        assert!(
            response
                .body
                .contains("\"repo_profile_grants_execution_authority\":false")
        );
        assert!(
            response
                .body
                .contains("\"suggested_checks_are_authority\":false")
        );
        assert!(response.body.contains(
            "\"principal_orientation_gate\":{\"required\":true,\"tool\":\"semantic_query\",\"grants_authority\":false}"
        ));
        assert!(response.body.contains(
            "\"execution_geometry\":{\"requested_workers\":4,\"effective_workers\":4,\"lane\":\"bounded_parallel_exploration\",\"route\":\"/forge/runs.json\""
        ));
        assert!(response.body.contains(
            "\"execution_backend_kind\":\"hosted_sandbox\",\"cloud_environment_id\":\"cloud_projection\""
        ));
        assert!(response.body.contains(
            "\"parallel_candidate\":{\"role\":\"primary\",\"primary_run_id\":\"forge_run_projection\",\"index\":1,\"count\":4,\"write_scope\":[\"Sources/App/Slug.swift\",\"Tests/AppTests/SlugTests.swift\"],\"strategy_id\":\"minimal_safe_patch\",\"strategy_summary\":\"smallest correct diff with the lowest integration risk\",\"required_semantic_operations\":[\"symbol_graph\",\"definition\",\"dependency_map\",\"related_tests\"],\"proof_bias\":\"minimal_diff_with_behavioral_check\",\"grants_execution_authority\":false}"
        ));
        assert!(
            response
                .body
                .contains("\"parallel_execution_group_count\":1")
        );
        assert!(response.body.contains(
            "\"parallel_execution_groups\":[{\"generated_from\":\"forge.run.event.parallel_candidates\",\"primary_run_id\":\"forge_run_projection\""
        ));
        assert!(response.body.contains("\"planned_candidate_count\":4"));
        assert!(response.body.contains("\"observed_candidate_count\":1"));
        assert!(
            response
                .body
                .contains("\"selection_status\":\"waiting_for_all_candidates_to_start\"")
        );
        assert!(
            response
                .body
                .contains("\"recommended_run_id\":\"forge_run_projection\"")
        );
        assert!(response.body.contains(
            "\"recommendation_basis\":\"projection_status_only; review_packet adds diff_quality, proof_quality, and eval evidence\""
        ));
        assert!(
            response
                .body
                .contains("\"grants_execution_authority\":false")
        );
    }

    #[test]
    fn execution_geometry_routes_width_without_pretending_direct_runs_are_fleets() {
        assert_eq!(
            execution_geometry_for_width(1).expect("single").lane,
            "single_worker"
        );
        let bounded = execution_geometry_for_width(4).expect("bounded");
        assert_eq!(bounded.lane, "bounded_parallel_exploration");
        assert_eq!(bounded.route, "/forge/runs.json");
        assert!(!bounded.fleet_required);

        let wide = execution_geometry_for_width(8).expect("wide");
        assert_eq!(wide.lane, "fleet_required");
        assert_eq!(wide.route, "/forge/fleet-plans.json");
        assert!(wide.fleet_required);

        assert!(execution_geometry_for_width(0).is_err());
        assert!(execution_geometry_for_width(mdx_core::FLEET_GEOMETRY_MAX_WORKERS + 1).is_err());
    }

    #[test]
    fn candidate_builder_slots_rotate_only_when_not_human_pinned() {
        let configured = vec![
            "OPUS".to_string(),
            "SONNET".to_string(),
            "GEMINI".to_string(),
        ];

        assert_eq!(
            candidate_builder_slot_from_slots("OPUS", "", 2, &configured),
            "SONNET"
        );
        assert_eq!(
            candidate_builder_slot_from_slots("OPUS", "", 3, &configured),
            "GEMINI"
        );
        assert_eq!(
            candidate_builder_slot_from_slots("OPUS", "", 4, &configured),
            "OPUS"
        );
        assert_eq!(
            candidate_builder_slot_from_slots("", "", 2, &configured),
            "OPUS"
        );
        assert_eq!(
            candidate_builder_slot_from_slots("OPUS", "OPUS", 2, &configured),
            "OPUS"
        );
    }

    #[test]
    fn start_route_refuses_wide_direct_run_before_model_or_receipt() {
        let _guard = route_model_test_guard();
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = handle_start(
            "POST",
            r#"{"intent":"refactor the service","fleet_width":8,"allowed_commands":["cargo test -p mdx-server"]}"#,
            &kernel,
        )
        .expect("route");

        assert!(
            response.body.contains(r#""status":"REFUSED""#),
            "{}",
            response.body
        );
        assert!(
            response.body.contains("/forge/fleet-plans.json"),
            "{}",
            response.body
        );
        assert!(
            response.body.contains("/forge/long-horizon-missions.json"),
            "{}",
            response.body
        );
        let guard = kernel.read().expect("kernel");
        assert_eq!(guard.ledger().query().by_kind("forge.run.event").len(), 0);
    }

    #[test]
    fn parallel_candidate_run_started_records_candidate_geometry() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let repo = temp_repo("parallel_candidate_java");
        std::fs::write(repo.join("pom.xml"), "<project></project>\n").expect("pom");
        let repo_profile = crate::forge_repo_profile::profile_repo(&repo);
        let resolved = crate::request_security::resolve_governed_write_identity(
            r#"{"tenant_id":"t","actor_id":"human:dev"}"#,
            "local_tenant",
            "local_user",
            "owner",
        );
        let execution_geometry = execution_geometry_for_width(4).expect("geometry");
        let primary = ForgeRunRequest {
            run_id: "forge_run_primary".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:dev".to_string(),
            work_item_id: "wi_parallel".to_string(),
            intent: "Refactor the service.".to_string(),
            allowed_commands: vec!["mvn test".to_string()],
            max_turns: 12,
            revise_branch: None,
            resume: false,
            write_scope: vec!["src/main/java/app/Service.java".to_string()],
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "medium".to_string(),
            semantic_policy_required_operations: vec![
                "file_outline".to_string(),
                "related_tests".to_string(),
                "diagnostics".to_string(),
            ],
            semantic_policy_source: "test".to_string(),
            execution_geometry_requested_workers: 4,
            execution_geometry_effective_workers: 4,
            execution_geometry_lane: "bounded_parallel_exploration".to_string(),
            execution_geometry_route: "/forge/runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        };
        let builder_casting = crate::forge_model_scorecard_route::BuilderCastingEvidence {
            status: "LOCAL_RUN_EVIDENCE_REQUESTED_SLOT_OVERRIDES_EVIDENCE".to_string(),
            requested_builder_slot: "SONNET".to_string(),
            selected_builder_slot: "SONNET".to_string(),
            recommended_builder_slot: "OPUS".to_string(),
            selected_model_profile_id: "codex_anthropic_responses_profile".to_string(),
            selected_provider_family: "anthropic".to_string(),
            selected_model_id: "claude-sonnet-4-6".to_string(),
            recommended_model_profile_id: "codex_anthropic_responses_profile".to_string(),
            recommended_provider_family: "anthropic".to_string(),
            recommended_model_id: "claude-opus-4-8".to_string(),
            basis: "local_run_track_record".to_string(),
            matching_eval_score_count: 2,
            accepted_eval_score_count: 1,
            matching_run_count: 3,
            done_rate_pct: 67,
            requested_slot_matches_evidence: false,
            ratified_grant_receipt_id: String::new(),
        };
        let run_strategy = crate::forge_run_strategy::resolve_strategy(
            &classify_forge_work("Refactor the service.", "mvn test"),
            r#"{"strategy_mode":"auto"}"#,
            Some(4),
        );

        let sibling_run_id = record_parallel_candidate_run_started(
            &kernel,
            &resolved,
            &primary,
            3,
            "forge_run_primary",
            "Refactor the service.",
            "Refactor the service.",
            "java_canary",
            &repo,
            &repo_profile,
            "operator_supplied",
            "mvn test",
            &execution_geometry,
            &run_strategy,
            true,
            None,
            &builder_casting,
        )
        .expect("candidate");

        let guard = kernel.read().expect("kernel");
        let receipt = guard
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .into_iter()
            .find(|receipt| {
                receipt.payload.get("run_id").map(String::as_str) == Some(sibling_run_id.as_str())
            })
            .expect("candidate receipt");
        assert_eq!(
            receipt
                .payload
                .get("parallel_candidate_primary_run_id")
                .map(String::as_str),
            Some("forge_run_primary")
        );
        assert_eq!(
            receipt
                .payload
                .get("parallel_candidate_index")
                .map(String::as_str),
            Some("3")
        );
        assert_eq!(
            receipt
                .payload
                .get("execution_geometry_effective_workers")
                .map(String::as_str),
            Some("4")
        );
        assert_eq!(
            receipt.payload.get("language_pack_id").map(String::as_str),
            Some("java-maven")
        );
        assert_eq!(
            receipt
                .payload
                .get("work_classification_task_class")
                .map(String::as_str),
            Some("refactor")
        );
        assert_eq!(
            receipt
                .payload
                .get("work_classification_source")
                .map(String::as_str),
            Some("mdx_core::classify_forge_work")
        );
        assert!(
            receipt
                .payload
                .get("language_task_alignment_status")
                .map(|value| !value.is_empty())
                .unwrap_or(false),
            "sibling candidates should carry language task alignment for eval and native chips"
        );
        assert!(
            receipt
                .payload
                .get("language_task_visible_check")
                .map(|value| !value.is_empty())
                .unwrap_or(false),
            "sibling candidates should carry visible proof guidance"
        );
        assert_eq!(
            receipt
                .payload
                .get("parallel_candidate_strategy_id")
                .map(String::as_str),
            Some("idiomatic_refactor")
        );
        assert_eq!(
            receipt
                .payload
                .get("parallel_candidate_strategy_summary")
                .map(String::as_str),
            Some("language-pack idioms and maintainability")
        );
        assert_eq!(
            receipt
                .payload
                .get("parallel_candidate_required_semantic_operations")
                .map(String::as_str),
            Some("file_outline,definition,references,diagnostics")
        );
        assert_eq!(
            receipt
                .payload
                .get("repo_profile_semantic_session_id")
                .map(String::as_str),
            Some("deferred_to_worker")
        );
        assert_eq!(
            receipt
                .payload
                .get("repo_profile_semantic_fallback_index_status")
                .map(String::as_str),
            Some("deferred_to_worker")
        );
        assert_eq!(
            receipt
                .payload
                .get("parallel_candidate_proof_bias")
                .map(String::as_str),
            Some("language_idiom_and_maintainability_review")
        );
        assert_eq!(
            receipt
                .payload
                .get("builder_casting_selected_slot")
                .map(String::as_str),
            Some("SONNET")
        );
        assert_eq!(
            receipt
                .payload
                .get("builder_casting_recommended_slot")
                .map(String::as_str),
            Some("OPUS")
        );
        assert_eq!(
            receipt
                .payload
                .get("builder_casting_requested_slot_matches_evidence")
                .map(String::as_str),
            Some("false")
        );
    }

    #[test]
    fn mission_attachment_validates_against_admitted_milestones() {
        let mut kernel = MdxKernel::boot_local();
        admit_test_mission(&mut kernel);

        let attached = MissionAttachment::from_body(
            r#"{"mission_id":"mission_for_run","mission_milestone_id":"mission_milestone_01"}"#,
        );
        assert!(attached.validate(&kernel).is_ok());

        let missing_milestone = MissionAttachment::from_body(
            r#"{"mission_id":"mission_for_run","mission_milestone_id":"mission_milestone_99"}"#,
        );
        assert!(
            missing_milestone
                .validate(&kernel)
                .expect_err("missing milestone refused")
                .contains("is not part of mission")
        );

        let missing_mission = MissionAttachment::from_body(
            r#"{"mission_id":"missing_mission","mission_milestone_id":"mission_milestone_01"}"#,
        );
        assert!(
            missing_mission
                .validate(&kernel)
                .expect_err("missing mission refused")
                .contains("is not admitted")
        );
    }

    #[test]
    fn run_terminal_outcome_records_mission_checkpoint() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            admit_test_mission(&mut guard);
        }
        let request = ForgeRunRequest {
            run_id: "forge_run_mission_001".to_string(),
            tenant_id: "local_tenant".to_string(),
            actor_id: "human:dev".to_string(),
            work_item_id: "wi_mission".to_string(),
            intent: "Ship checkpoint one.".to_string(),
            allowed_commands: vec!["cargo test -p mdx-core".to_string()],
            max_turns: 12,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "medium".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "test".to_string(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "single_worker".to_string(),
            execution_geometry_route: "/forge/runs.json".to_string(),
            mission_id: "mission_for_run".to_string(),
            mission_milestone_id: "mission_milestone_01".to_string(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        };
        let outcome = crate::forge_loop_runner::ForgeRunOutcome {
            run_id: request.run_id.clone(),
            status: "RUN_FINISHED_DONE",
            turns_used: 3,
            files_changed: 1,
            check_runs: 1,
            check_duration_ms: 42,
            branch: Some("forge/run-mission".to_string()),
            commit_sha: Some("abc123".to_string()),
            finish_summary: "Implemented the first milestone and checks passed.".to_string(),
            last_check_passed: true,
        };

        record_mission_checkpoint_from_run_outcome(&kernel, &request, &outcome);

        let guard = kernel.read().expect("kernel");
        let dashboard = guard.project_forge_long_horizon_missions();
        assert_eq!(dashboard.completed_milestone_count, 1);
        let packet = dashboard.packets.first().expect("mission");
        assert_eq!(packet.mission_state, "COMPLETED_LOCAL_CHECKPOINTS");
        assert_eq!(packet.milestones[0].status, "COMPLETED_LOCAL_CHECKPOINT");
        assert_eq!(packet.milestones[0].validation_status, "passed");
        assert_eq!(packet.milestones[0].related_run_id, "forge_run_mission_001");
    }

    #[test]
    fn run_projection_exposes_mission_attachment() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            let identity = GovernedWriteIdentity::local_demo("human:dev");
            guard
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "human:dev",
                        run_id: "forge_run_mission_projection",
                        event: "run_started",
                        work_item_id: "wi_mission_projection",
                        detail: "accepted mission attached run",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &identity,
                    &[
                        ("mission_id", "mission_for_projection"),
                        ("mission_milestone_id", "mission_milestone_02"),
                        (
                            "mission_checkpoint_route",
                            "/forge/long-horizon-mission-checkpoints.json",
                        ),
                        ("mission_checkpoint_grants_execution_authority", "false"),
                    ],
                )
                .expect("run started");
        }

        let response = handle_projection("GET", &kernel).expect("projection");
        assert!(
            response
                .body
                .contains(r#""mission_id":"mission_for_projection""#)
        );
        assert!(
            response
                .body
                .contains(r#""milestone_id":"mission_milestone_02""#)
        );
        assert!(
            response
                .body
                .contains(r#""checkpoint_grants_execution_authority":false"#)
        );
    }

    #[test]
    fn run_projection_exposes_context_telemetry() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            guard
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_context_projection",
                    event: "model_called",
                    work_item_id: "wi_context_projection",
                    detail: "model=grok-build-0.1 finish_reason=tool_calls tool_calls=1",
                    turn: 1,
                    input_tokens: 700,
                    output_tokens: 20,
                })
                .expect("grok model call");
            guard
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "human:dev",
                    run_id: "forge_run_context_projection",
                    event: "model_called",
                    work_item_id: "wi_context_projection",
                    detail: "delegated investigate model=claude-opus-4.8 call=1/1",
                    turn: 2,
                    input_tokens: 100000,
                    output_tokens: 2000,
                })
                .expect("opus model call");
        }

        let response = handle_projection("GET", &kernel).expect("projection");
        assert!(response.body.contains(r#""context_telemetry":{"#));
        assert!(
            response
                .body
                .contains(r#""latest":{"model_id":"claude-opus-4.8","input_tokens":100000"#),
            "{}",
            response.body
        );
        assert!(
            response
                .body
                .contains(r#""peak":{"model_id":"claude-opus-4.8","input_tokens":100000"#),
            "{}",
            response.body
        );
        assert!(
            response
                .body
                .contains(r#""total":{"input_tokens":100700,"output_tokens":2020,"model_calls":2,"model_count":2}"#),
            "{}",
            response.body
        );
        assert!(response.body.contains(r#""model_id":"grok-build-0.1""#));
        assert!(response.body.contains(r#""model_id":"claude-opus-4.8""#));
        assert!(
            response
                .body
                .contains(r#""source":"provider_usage_receipts""#)
        );
        assert!(
            response
                .body
                .contains(r#""window_source":"mdx_model_window_table""#)
        );
    }

    #[test]
    fn work_queue_preflight_accepts_known_item_and_returns_scope() {
        let repo = TempRepo::with_work_queue(
            r#"{"work_items":[{"id":"DEV-101","write_scope":["crates/mdx-server/src/**","docs/forge.md"]}]}"#,
        );

        let scope = mdx_work_queue_scope_preflight(
            "touch crates/mdx-server/src/forge_run_route.rs",
            "DEV-101",
            repo.path().to_str().unwrap(),
        )
        .expect("known work item");

        assert_eq!(
            scope,
            vec![
                "crates/mdx-server/src/**".to_string(),
                "docs/forge.md".to_string()
            ]
        );
    }

    #[test]
    fn work_queue_preflight_refuses_unknown_item_before_a_run_starts() {
        let repo = TempRepo::with_work_queue(
            r#"{"work_items":[{"id":"DEV-101","write_scope":["crates/mdx-server/src/**"]}]}"#,
        );

        let reason = mdx_work_queue_scope_preflight(
            "touch crates/mdx-server/src/forge_run_route.rs",
            "DEV-999",
            repo.path().to_str().unwrap(),
        )
        .expect_err("unknown item refuses");

        assert!(reason.contains("Unknown MDx work item DEV-999"));
    }

    #[test]
    fn work_queue_preflight_self_binds_scope_from_matching_items() {
        let repo = TempRepo::with_work_queue(
            r#"{"work_items":[{"id":"DEV-101","write_scope":["crates/mdx-server/src/**"]},{"id":"UI-201","write_scope":["apps/mdx-host/src/routes/forge/**"]}]}"#,
        );

        // A path-targeted MDx ask with no work_item_id no longer dead-ends: it
        // binds the scope of the covering item and starts.
        let scope = mdx_work_queue_scope_preflight(
            "fix crates/mdx-server/src/forge_run_route.rs",
            "",
            repo.path().to_str().unwrap(),
        )
        .expect("path-targeted MDx run self-binds a scope");

        assert!(scope.contains(&"crates/mdx-server/src/**".to_string()));
        assert!(!scope.contains(&"apps/mdx-host/src/routes/forge/**".to_string()));
    }

    #[test]
    fn work_queue_preflight_grants_named_paths_when_no_item_covers_them() {
        let repo = TempRepo::with_work_queue(
            r#"{"work_items":[{"id":"DEV-101","write_scope":["crates/mdx-server/src/**"]}]}"#,
        );

        // An ask that touches paths outside every work item still starts, scoped
        // to exactly the file it named - never widened to the containing
        // directory, so sibling files stay out of scope.
        let scope = mdx_work_queue_scope_preflight(
            "add apps/mdx-host/src/lib/new_helper.js and a test",
            "",
            repo.path().to_str().unwrap(),
        )
        .expect("uncovered path self-scopes instead of refusing");

        assert!(scope.contains(&"apps/mdx-host/src/lib/new_helper.js".to_string()));
        assert!(
            !scope.iter().any(|entry| entry.ends_with("/**")),
            "must not widen an uncovered named path to a directory glob"
        );
    }

    #[test]
    fn work_queue_preflight_never_self_grants_sensitive_paths() {
        // Work queue covers only docs, so the sensitive paths below are
        // uncovered and hit the self-grant branch (where the denylist applies).
        let repo = TempRepo::with_work_queue(
            r#"{"work_items":[{"id":"DOC-1","write_scope":["docs/**"]}]}"#,
        );

        // Sensitive protection-ring paths named in an ask are never self-granted;
        // they still require a curated work item, matching the do-not-touch list.
        for ask in [
            "edit crates/mdx-server/migrations/003_add.sql",
            "change crates/mdx-core/src/secret_store.rs",
            "touch .github/workflows/ci.yml",
            "update crates/mdx-server/src/auth_verifier.rs",
            "tweak crates/mdx-core/src/ledger.rs",
        ] {
            let scope = mdx_work_queue_scope_preflight(ask, "", repo.path().to_str().unwrap())
                .expect("sensitive uncovered path still resolves");
            assert!(
                scope.is_empty(),
                "sensitive path must not be self-granted from ask text: {ask} -> {scope:?}"
            );
        }
    }

    #[test]
    fn work_queue_preflight_allows_external_repos_without_an_mdx_queue() {
        let repo = TempRepo::empty();

        let scope = mdx_work_queue_scope_preflight(
            "fix src/service.rs and run tests",
            "",
            repo.path().to_str().unwrap(),
        )
        .expect("external repo has no MDx queue");

        assert!(scope.is_empty());
    }

    #[test]
    fn start_route_refuses_missing_readiness_before_recording_a_run() {
        let _guard = route_model_test_guard();
        let tenant_id = "forge_missing_readiness_test";
        crate::secret_store::global().clear_tenant(tenant_id);
        unsafe {
            for key in [
                "ANTHROPIC_API_KEY",
                "OPENAI_API_KEY",
                "XAI_API_KEY",
                "GEMINI_API_KEY",
                "AWS_BEDROCK_MODEL_ACCESS",
                "MDX_OLLAMA_API_KEY",
                "MDX_DEFAULT_MODEL_PROVIDER",
                "MDX_ANTHROPIC_MODEL",
                "MDX_ANTHROPIC_OPUS_MODEL",
                "MDX_ANTHROPIC_SONNET_MODEL",
                "MDX_OPENAI_MODEL",
                "MDX_OPENAI_CODEX_MODEL",
                "MDX_OPENAI_CODEX_MINI_MODEL",
                "MDX_XAI_BUILD_MODEL",
                "MDX_XAI_MODEL",
                "MDX_GEMINI_MODEL",
                "MDX_FLEET_BUILDER_BASE_URL",
                "MDX_FLEET_BUILDER_API_KEY",
                "MDX_FLEET_BUILDER_MODEL",
                "MDX_FLEET_BUILDER_OPUS_BASE_URL",
                "MDX_FLEET_BUILDER_OPUS_API_KEY",
                "MDX_FLEET_BUILDER_OPUS_MODEL",
                "MDX_FLEET_BUILDER_SONNET_BASE_URL",
                "MDX_FLEET_BUILDER_SONNET_API_KEY",
                "MDX_FLEET_BUILDER_SONNET_MODEL",
                "MDX_FLEET_BUILDER_GPT_BASE_URL",
                "MDX_FLEET_BUILDER_GPT_API_KEY",
                "MDX_FLEET_BUILDER_GPT_MODEL",
                "MDX_FLEET_BUILDER_CODEX_BASE_URL",
                "MDX_FLEET_BUILDER_CODEX_API_KEY",
                "MDX_FLEET_BUILDER_CODEX_MODEL",
                "MDX_FLEET_BUILDER_CODEXMINI_BASE_URL",
                "MDX_FLEET_BUILDER_CODEXMINI_API_KEY",
                "MDX_FLEET_BUILDER_CODEXMINI_MODEL",
                "MDX_FLEET_BUILDER_GROK_BASE_URL",
                "MDX_FLEET_BUILDER_GROK_API_KEY",
                "MDX_FLEET_BUILDER_GROK_MODEL",
                "MDX_FLEET_BUILDER_XAI_BASE_URL",
                "MDX_FLEET_BUILDER_XAI_API_KEY",
                "MDX_FLEET_BUILDER_XAI_MODEL",
                "MDX_FLEET_BUILDER_GEMINI_BASE_URL",
                "MDX_FLEET_BUILDER_GEMINI_API_KEY",
                "MDX_FLEET_BUILDER_GEMINI_MODEL",
                "MDX_FLEET_BUILDER_BEDROCK_BASE_URL",
                "MDX_FLEET_BUILDER_BEDROCK_API_KEY",
                "MDX_FLEET_BUILDER_BEDROCK_MODEL",
                "MDX_FLEET_BUILDER_LOCAL_BASE_URL",
                "MDX_FLEET_BUILDER_LOCAL_API_KEY",
                "MDX_FLEET_BUILDER_LOCAL_MODEL",
                "MDX_FLEET_EXECUTOR_BASE_URL",
                "MDX_FLEET_EXECUTOR_API_KEY",
                "MDX_FLEET_EXECUTOR_MODEL",
            ] {
                std::env::remove_var(key);
                crate::secret_store::global().disable_keychain_lookup_for_tenant(tenant_id, key);
            }
        }
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let builder_ready = {
            let guard = kernel.read().expect("kernel");
            TurnClient::any_builder_configured_for_tenant(&guard, tenant_id)
        };

        assert!(
            !builder_ready,
            "isolated tenant must not inherit a test model"
        );
        let response = handle_start(
            "POST",
            r#"{"tenant_id":"forge_missing_readiness_test","intent":"fix src/service.rs"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("Connect a model first"));
        let guard = kernel.read().expect("kernel");
        assert_eq!(guard.ledger().query().by_kind("forge.run.event").len(), 0);
        crate::secret_store::global().clear_tenant(tenant_id);
    }

    #[test]
    fn start_route_refuses_bad_work_item_without_recording_a_run() {
        let _guard = route_model_test_guard();
        let repo = TempRepo::with_work_queue(
            r#"{"work_items":[{"id":"DEV-101","write_scope":["crates/mdx-server/src/**"]}]}"#,
        );
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            seed_connected_xai_builder(&mut guard, "local_tenant");
            guard
                .connect_forge_repo(ForgeRepoConnect {
                    tenant_id: "local_tenant",
                    actor_id: "human:test",
                    repo_id: "temp-mdx",
                    label: "Temp MDx",
                    root: repo.path().to_str().unwrap(),
                    kind: "local",
                    origin: "",
                })
                .expect("repo connected");
        }

        let response = handle_start(
            "POST",
            r#"{"intent":"touch crates/mdx-server/src/forge_run_route.rs","work_item_id":"DEV-999","repo_id":"temp-mdx"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("Unknown MDx work item DEV-999"));
        let guard = kernel.read().expect("kernel");
        assert_eq!(guard.ledger().query().by_kind("forge.run.event").len(), 0);
    }

    #[test]
    fn start_route_refuses_missing_selected_check_toolchain_before_recording_a_run() {
        let _guard = route_model_test_guard();
        let repo = TempRepo::empty();
        std::fs::create_dir_all(repo.path()).expect("repo dir");
        std::fs::write(repo.path().join("build.gradle.kts"), "plugins { java }\n")
            .expect("gradle marker");
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            seed_connected_xai_builder(&mut guard, "local_tenant");
            guard
                .connect_forge_repo(ForgeRepoConnect {
                    tenant_id: "local_tenant",
                    actor_id: "human:test",
                    repo_id: "temp-gradle",
                    label: "Temp Gradle",
                    root: repo.path().to_str().unwrap(),
                    kind: "local",
                    origin: "",
                })
                .expect("repo connected");
        }

        let response = handle_start(
            "POST",
            r#"{"intent":"add a feature and prove it","repo_id":"temp-gradle","allowed_commands":[]}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("local proof readiness"));
        assert!(response.body.contains("check:gradlew_test"));
        let guard = kernel.read().expect("kernel");
        assert_eq!(guard.ledger().query().by_kind("forge.run.event").len(), 0);
    }

    #[test]
    fn start_route_refuses_missing_selected_or_inferred_checks_before_recording_a_run() {
        let _guard = route_model_test_guard();
        let repo = TempRepo::empty();
        std::fs::create_dir_all(repo.path()).expect("repo dir");
        std::fs::write(repo.path().join("README.md"), "# Generic repo\n").expect("readme");
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            seed_connected_xai_builder(&mut guard, "local_tenant");
            guard
                .connect_forge_repo(ForgeRepoConnect {
                    tenant_id: "local_tenant",
                    actor_id: "human:test",
                    repo_id: "temp-generic",
                    label: "Temp Generic",
                    root: repo.path().to_str().unwrap(),
                    kind: "local",
                    origin: "",
                })
                .expect("repo connected");
        }

        let response = handle_start(
            "POST",
            r#"{"intent":"make a medium complexity change","repo_id":"temp-generic"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(
            response
                .body
                .contains("no proof command was selected or inferred")
        );
        assert!(response.body.contains("generic"));
        assert!(
            response
                .body
                .contains("Choose the check command this repo uses")
        );
        let guard = kernel.read().expect("kernel");
        assert_eq!(guard.ledger().query().by_kind("forge.run.event").len(), 0);
    }

    #[test]
    fn explicit_empty_allowed_commands_does_not_fall_back_to_rust() {
        assert_eq!(
            parse_commands(r#"{"intent":"fix it","allowed_commands":[]}"#),
            Vec::<String>::new()
        );
        assert_eq!(
            parse_commands(r#"{"intent":"fix it"}"#),
            Vec::<String>::new()
        );
    }

    #[test]
    fn allowed_commands_decode_json_and_split_multiline_values() {
        assert_eq!(
            parse_commands(
                r#"{"intent":"fix it","allowed_commands":["npm ci\n npm test","node --test \"test file.js\""]}"#,
            ),
            vec![
                "npm ci".to_string(),
                "npm test".to_string(),
                "node --test \"test file.js\"".to_string(),
            ]
        );
    }

    #[test]
    fn selected_checks_source_tracks_operator_vs_repo_profile() {
        assert_eq!(
            selected_checks_source_for(true, &["swift test".to_string()], false, false),
            "operator_supplied"
        );
        assert_eq!(
            selected_checks_source_for(
                true,
                &["npm ci".to_string(), "npm test".to_string()],
                true,
                false
            ),
            "operator_supplied_plus_repo_setup"
        );
        assert_eq!(
            selected_checks_source_for(false, &["swift test".to_string()], false, false),
            "repo_profile_inferred"
        );
        assert_eq!(
            selected_checks_source_for(false, &Vec::<String>::new(), false, false),
            "none_recorded"
        );
        assert_eq!(
            selected_checks_source_for(
                false,
                &["cargo check -p mdx-server".to_string()],
                false,
                true
            ),
            "stack_aware_scope_inferred"
        );
    }

    #[test]
    fn stack_aware_rust_scope_prefers_cargo_in_mixed_repo() {
        let repo = TempRepo::empty();
        std::fs::create_dir_all(repo.path().join("crates/mdx-server/src")).expect("repo dir");
        std::fs::write(
            repo.path().join("package.json"),
            r#"{"scripts":{"test":"pnpm test"}}"#,
        )
        .expect("package");
        std::fs::write(
            repo.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/mdx-server\"]\n",
        )
        .expect("cargo");

        let checks = stack_aware_selected_checks(
            "Fix crates/mdx-server/src/forge_run_route.rs",
            &["crates/mdx-server/src/**".to_string()],
            repo.path(),
        )
        .expect("rust scope gets checks");

        assert_eq!(checks, vec!["cargo check -p mdx-server"]);
        assert_eq!(
            stack_aware_selected_checks(
                "Fix the scoreboard tests, run cargo test -p mdx-server forge_fleet_eval_scoreboard to see them first.",
                &["crates/mdx-server/src/**".to_string()],
                repo.path()
            )
            .expect("explicit cargo test"),
            vec!["cargo test -p mdx-server forge_fleet_eval_scoreboard"]
        );
        assert_eq!(
            stack_aware_selected_checks(
                "Medium native Forge retry task: in crates/mdx-server/src/forge_loop_runner.rs, strengthen the existing progress-gate test coverage. Use the existing forge_loop_runner tests as the validation target.",
                &[
                    "crates/mdx-server/src/".to_string(),
                    "apps/mdx-host/src/routes/welcome/".to_string(),
                    "apps/mdx-host/src/lib/".to_string()
                ],
                repo.path()
            ),
            Some(vec![
                "cargo test -p mdx-server forge_loop_runner".to_string()
            ])
        );
        assert_eq!(
            stack_aware_selected_checks(
                "High native Forge dogfood task: improve run-diff proof confidence in crates/mdx-server/src/forge_diff_route.rs. Use cargo test -p mdx-server forge_diff_route as the validation target.",
                &["crates/mdx-server/src/forge_diff_route.rs".to_string()],
                repo.path()
            ),
            Some(vec![
                "cargo test -p mdx-server forge_diff_route".to_string()
            ])
        );
        assert_eq!(
            stack_aware_selected_checks(
                "Verify the Forge run projection prevents no-change runs without a branch from offering Ship. If the focused server test is already present, run it.",
                &[],
                repo.path()
            ),
            Some(vec![
                "cargo test -p mdx-server forge_run_route -- --test-threads=1".to_string()
            ])
        );
        std::fs::write(
            repo.path().join("Makefile"),
            "forge-flagship-acceptance-check:\n\tnode scripts/forge-flagship-acceptance-check.mjs\n",
        )
        .expect("makefile");
        assert_eq!(
            stack_aware_selected_checks(
                "Add one short sentence to docs/FORGE-OPERATOR-EXPERIENCE.md",
                &["docs/FORGE-OPERATOR-EXPERIENCE.md".to_string()],
                repo.path()
            ),
            Some(vec!["make forge-flagship-acceptance-check".to_string()])
        );
        // A JS ask on apps/ now derives a node check instead of falling back to
        // the repo's cargo default (F5). With no test file named, it runs the
        // app's node test directory.
        assert_eq!(
            stack_aware_selected_checks(
                "Fix apps/mdx-host/src/routes/forge/+page.svelte",
                &["apps/mdx-host/src/**".to_string()],
                repo.path()
            ),
            Some(vec!["node --test apps/mdx-host/test".to_string()])
        );
        std::fs::write(
            repo.path().join("Makefile"),
            "forge-flagship-acceptance-check:\n\tnode scripts/forge-flagship-acceptance-check.mjs\nnative-macos-operator-check:\n\tsh scripts/native-macos-operator-check.sh\n",
        )
        .expect("makefile with native target");
        assert_eq!(
            stack_aware_selected_checks(
                "Add a native macOS mapper check in apps/mdx-operator-macos/Sources/MDxOperator/Support/MapperChecks.swift",
                &[
                    "apps/mdx-operator-macos/Sources/MDxOperator/Support/MapperChecks.swift"
                        .to_string()
                ],
                repo.path()
            ),
            Some(vec!["make native-macos-operator-check".to_string()])
        );
        assert_eq!(
            stack_aware_selected_checks(
                "Native Forge dogfood task: add one focused mapper check that proves Mac operator repair lane identity. Keep the UX unchanged and use make native-macos-operator-check as the proof target.",
                &[],
                repo.path()
            ),
            Some(vec!["make native-macos-operator-check".to_string()])
        );
        // A named node test file becomes the check verbatim.
        assert_eq!(
            stack_aware_selected_checks(
                "Add a helper and prove it with node apps/mdx-host/test/scheduler.test.mjs",
                &["apps/mdx-host/src/lib/scheduler/".to_string()],
                repo.path()
            ),
            Some(vec![
                "node apps/mdx-host/test/scheduler.test.mjs".to_string()
            ])
        );
    }

    #[test]
    fn operator_selected_node_proof_gets_required_setup_prefix() {
        let repo = TempRepo::empty();
        std::fs::create_dir_all(repo.path()).expect("repo dir");
        std::fs::write(
            repo.path().join("package.json"),
            r#"{"scripts":{"test":"node --test"}}"#,
        )
        .expect("package");
        std::fs::write(repo.path().join("package-lock.json"), "{}\n").expect("lock");
        let profile = crate::forge_repo_profile::profile_repo(repo.path());
        let mut selected_checks = vec!["npm test".to_string()];

        let added = ensure_required_setup_commands(&mut selected_checks, &profile, repo.path());

        assert!(added);
        assert_eq!(selected_checks, vec!["npm ci", "npm test"]);
        assert_eq!(
            selected_checks_source_for(true, &selected_checks, added, false),
            "operator_supplied_plus_repo_setup"
        );
    }

    #[test]
    fn operator_selected_dependency_free_node_proof_does_not_get_setup_prefix() {
        let repo = TempRepo::empty();
        std::fs::create_dir_all(repo.path()).expect("repo dir");
        std::fs::write(
            repo.path().join("package.json"),
            r#"{"scripts":{"test":"node --test"}}"#,
        )
        .expect("package");
        let profile = crate::forge_repo_profile::profile_repo(repo.path());
        let mut selected_checks = vec!["npm test".to_string()];

        let added = ensure_required_setup_commands(&mut selected_checks, &profile, repo.path());

        assert!(!added);
        assert_eq!(selected_checks, vec!["npm test"]);
        assert_eq!(
            selected_checks_source_for(true, &selected_checks, added, false),
            "operator_supplied"
        );
    }

    #[test]
    fn native_macos_operator_proof_does_not_get_node_setup_prefix() {
        let repo = TempRepo::empty();
        std::fs::create_dir_all(repo.path()).expect("repo dir");
        std::fs::write(
            repo.path().join("package.json"),
            r#"{"scripts":{"test":"node --test"}}"#,
        )
        .expect("package");
        std::fs::write(
            repo.path().join("pnpm-lock.yaml"),
            "lockfileVersion: '9.0'\n",
        )
        .expect("lock");
        let profile = crate::forge_repo_profile::profile_repo(repo.path());
        let mut selected_checks = vec!["make native-macos-operator-check".to_string()];

        let added = ensure_required_setup_commands(&mut selected_checks, &profile, repo.path());

        assert!(!added);
        assert_eq!(
            selected_checks,
            vec!["make native-macos-operator-check".to_string()]
        );
    }

    #[test]
    fn operator_selected_python_proof_gets_requirements_setup_prefix() {
        let repo = TempRepo::empty();
        std::fs::create_dir_all(repo.path()).expect("repo dir");
        std::fs::write(repo.path().join("requirements.txt"), "pytest\nrequests\n")
            .expect("requirements");
        let profile = crate::forge_repo_profile::profile_repo(repo.path());
        let mut selected_checks = vec!["pytest".to_string()];

        normalize_selected_commands_for_repo(&mut selected_checks, &profile);
        let added = ensure_required_setup_commands(&mut selected_checks, &profile, repo.path());

        assert!(added);
        assert_eq!(
            selected_checks,
            vec![
                "python3 -m venv .venv && . .venv/bin/activate && python -m pip install -r requirements.txt pytest",
                ". .venv/bin/activate && pytest"
            ]
        );
        assert_eq!(
            selected_checks_source_for(true, &selected_checks, added, false),
            "operator_supplied_plus_repo_setup"
        );
    }

    #[test]
    fn operator_selected_python_pyproject_proof_gets_editable_setup_prefix() {
        let repo = TempRepo::empty();
        std::fs::create_dir_all(repo.path()).expect("repo dir");
        std::fs::write(
            repo.path().join("pyproject.toml"),
            "[project]\nname = \"canary\"\nversion = \"0.1.0\"\n",
        )
        .expect("pyproject");
        let profile = crate::forge_repo_profile::profile_repo(repo.path());
        let mut selected_checks = vec!["pytest".to_string()];

        normalize_selected_commands_for_repo(&mut selected_checks, &profile);
        let added = ensure_required_setup_commands(&mut selected_checks, &profile, repo.path());

        assert!(added);
        assert_eq!(
            selected_checks,
            vec![
                "python3 -m venv .venv && . .venv/bin/activate && python -m pip install -e . pytest",
                ". .venv/bin/activate && pytest"
            ]
        );
        assert_eq!(
            selected_checks_source_for(true, &selected_checks, added, false),
            "operator_supplied_plus_repo_setup"
        );
    }

    #[test]
    fn operator_selected_maven_proof_gets_dependency_setup_prefix() {
        let repo = TempRepo::empty();
        std::fs::create_dir_all(repo.path()).expect("repo dir");
        std::fs::write(repo.path().join("pom.xml"), "<project />\n").expect("pom");
        let profile = crate::forge_repo_profile::profile_repo(repo.path());
        let mut selected_checks = vec!["mvn test".to_string()];

        let added = ensure_required_setup_commands(&mut selected_checks, &profile, repo.path());

        assert!(added);
        assert_eq!(
            selected_checks,
            vec!["mvn -q -DskipTests dependency:go-offline", "mvn test"]
        );
        assert_eq!(
            selected_checks_source_for(true, &selected_checks, added, false),
            "operator_supplied_plus_repo_setup"
        );
    }

    #[test]
    fn operator_selected_gradle_proof_gets_dependency_setup_prefix() {
        let repo = TempRepo::empty();
        std::fs::create_dir_all(repo.path()).expect("repo dir");
        std::fs::write(repo.path().join("build.gradle.kts"), "plugins { java }\n").expect("gradle");
        std::fs::write(repo.path().join("gradlew"), "#!/bin/sh\n").expect("wrapper");
        let profile = crate::forge_repo_profile::profile_repo(repo.path());
        let mut selected_checks = vec!["./gradlew test".to_string()];

        let added = ensure_required_setup_commands(&mut selected_checks, &profile, repo.path());

        assert!(added);
        assert_eq!(
            selected_checks,
            vec!["./gradlew dependencies", "./gradlew test"]
        );
        assert_eq!(
            selected_checks_source_for(true, &selected_checks, added, false),
            "operator_supplied_plus_repo_setup"
        );
    }

    #[test]
    fn setup_only_selection_still_refuses_without_behavioral_proof() {
        let repo = TempRepo::empty();
        std::fs::create_dir_all(repo.path()).expect("repo dir");
        std::fs::write(
            repo.path().join("package.json"),
            r#"{"scripts":{"test":"node --test"}}"#,
        )
        .expect("package");
        let profile = crate::forge_repo_profile::profile_repo(repo.path());

        let reason = missing_selected_checks_refusal(&["npm install".to_string()], &profile)
            .expect("setup only is not proof");

        assert!(reason.contains("no proof command"));
    }

    #[test]
    fn selected_check_toolchain_refusal_only_blocks_known_missing_check() {
        let readiness = vec![
            "tool:swift=available".to_string(),
            "check:swift_test=missing".to_string(),
        ];

        let reason =
            missing_selected_check_toolchain_refusal(&["swift test".to_string()], &readiness, true)
                .expect("missing readiness refuses");
        assert!(reason.contains("check:swift_test"));
        assert!(
            missing_selected_check_toolchain_refusal(
                &["swift build".to_string()],
                &readiness,
                true,
            )
            .is_none()
        );
        assert!(
            missing_selected_check_toolchain_refusal(
                &["swift test".to_string()],
                &readiness,
                false,
            )
            .is_none()
        );
        assert!(
            missing_selected_checks_refusal(
                &["swift build".to_string()],
                &crate::forge_repo_profile::profile_repo(std::path::Path::new(""))
            )
            .is_none()
        );
    }

    #[test]
    fn run_intake_evidence_carries_repo_readiness_and_scout_context() {
        let repo = TempRepo::empty();
        std::fs::create_dir_all(repo.path().join("Sources/App")).expect("source dir");
        std::fs::write(repo.path().join("Package.swift"), "// swift package\n")
            .expect("package marker");
        std::fs::write(
            repo.path().join("Sources/App/Thing.swift"),
            "struct Thing {\n  // TODO: make the slug stable\n}\n",
        )
        .expect("source");
        let profile = crate::forge_repo_profile::profile_repo(repo.path());
        let profile_value: serde_json::Value =
            serde_json::from_str(&crate::forge_repo_profile::profile_json(repo.path()))
                .expect("profile json");
        let resolved = ResolvedRunRepo {
            root: repo.path().to_str().unwrap_or("").to_string(),
            origin_url: "git@github.com:acme/swift-canary.git".to_string(),
        };

        let intake = run_intake_evidence(
            &resolved,
            &profile,
            &profile_value,
            &["swift test".to_string()],
            "repo_profile_inferred",
            false,
        );

        assert_eq!(
            intake.generated_from,
            "repo_readiness+repo_task_scout+language_task_alignment"
        );
        assert_eq!(intake.source_host, "github");
        assert_eq!(intake.origin_url_present, "true");
        assert!(intake.first_run_task_ids.contains("swift-spm-first-run"));
        assert_eq!(intake.scout_status, "TASKS_FOUND");
        assert!(
            intake
                .scout_candidate_count
                .parse::<u32>()
                .expect("candidate count")
                >= 1
        );
        assert!(intake.scout_candidate_kinds.contains("todo_cleanup"));
        assert!(
            intake
                .scout_candidate_paths
                .contains("Sources/App/Thing.swift")
        );
        assert!(
            intake
                .semantic_orientation_operations
                .contains("file_outline")
        );
        assert!(
            intake
                .semantic_orientation_operations
                .contains("related_tests")
        );
        assert!(intake.write_scope_hint.contains("Sources/App/Thing.swift"));
        assert!(intake.off_limits_patterns.contains(".build/**"));
        assert!(intake.review_focus.contains("TODO/FIXME"));
        assert!(intake.proof_strategy.contains("repo_profile_inferred"));
    }

    #[test]
    fn verified_hosted_run_uses_sandbox_readiness_not_render_host_toolchain() {
        let repo = TempRepo::empty();
        std::fs::create_dir_all(repo.path()).expect("repo root");
        std::fs::write(repo.path().join("Package.swift"), "// swift package\n")
            .expect("package marker");
        let mut profile = crate::forge_repo_profile::profile_repo(repo.path());
        profile.proof_plan_status = "setup_required";
        let profile_value = repo_profile_json_value(&profile);
        let resolved = ResolvedRunRepo {
            root: repo.path().to_str().unwrap_or("").to_string(),
            origin_url: "https://github.com/acme/swift-canary".to_string(),
        };

        let local = run_intake_evidence(
            &resolved,
            &profile,
            &profile_value,
            &["swift test".to_string()],
            "operator_supplied",
            false,
        );
        let hosted = run_intake_evidence(
            &resolved,
            &profile,
            &profile_value,
            &["swift test".to_string()],
            "operator_supplied",
            true,
        );

        assert_eq!(local.readiness_status, "SETUP_REQUIRED");
        assert_eq!(hosted.readiness_status, "READY_FOR_MEDIUM_HIGH_WORK");
        assert_eq!(hosted.medium_high_work_ready, "true");
        assert!(hosted.safe_next_move.contains("verified hosted sandbox"));
    }

    fn seed_connected_xai_builder(kernel: &mut MdxKernel, tenant_id: &str) {
        let approval = kernel
            .approve_model_turn_on(ApproveModelTurnOn {
                tenant_id,
                actor_id: "human:test",
                provider_id: "xai",
            })
            .expect("approval");
        kernel
            .save_twin_model_gateway_provider_observation_local(
                TwinModelGatewayProviderObservation {
                    tenant_id,
                    actor_id: "human:test",
                    provider_id: "xai",
                    adapter: "XaiChatModelGateway",
                    receipt_kind: "xai.chat.observed",
                    approval_receipt_id: &approval.approval_receipt_id,
                    evidence_file: "test:welcome-connect",
                    model_id: "grok-connected-test",
                    response_id: "resp_connected_test",
                    response_status: "completed",
                    observed: true,
                    provider_call_attempted: true,
                    network_call_attempted: true,
                    credential_presence_only: true,
                    credential_values_recorded: false,
                    provider_secret_values_recorded: false,
                    requested_secret_values_recorded: false,
                    output_text_recorded: false,
                    production_write_allowed: false,
                    total_tokens: 3,
                },
            )
            .expect("observation");
        crate::secret_store::global().set_for_tenant(tenant_id, "XAI_API_KEY", "xai-test-key");
    }

    #[test]
    fn run_titles_clip_at_a_human_boundary() {
        let intent = "Add a Reset sparks button next to Add a spark. Reset returns the counter to 0 and is styled as a secondary control. Update the test script to prove the reset control and behavior. Keep the change small.";
        let title = run_title_from_intent(intent);
        assert!(title.ends_with("..."), "{title}");
        assert!(title.chars().count() <= 140, "{title}");
        assert!(!title.ends_with(" t..."), "{title}");
        assert!(intent.starts_with(title.trim_end_matches('.')), "{title}");
    }

    #[test]
    fn short_run_titles_stay_unchanged() {
        assert_eq!(
            bounded_run_title("Fix the reset control.", 140),
            "Fix the reset control."
        );
    }

    #[test]
    fn forge_run_admission_caps_active_workers_per_actor() {
        let actor = "forge_run_admission_caps_active_workers_per_actor";
        let permits =
            reserve_forge_run_permits_with_caps(actor, 2, 2, 64).expect("first run admits");
        let err = reserve_forge_run_permits_with_caps(actor, 1, 2, 64)
            .expect_err("second active run should exceed actor cap");
        assert!(err.contains("per-user cap"));
        drop(permits);
        let next =
            reserve_forge_run_permits_with_caps(actor, 1, 2, 64).expect("cap releases on drop");
        assert_eq!(next.len(), 1);
    }

    struct TempRepo {
        path: std::path::PathBuf,
    }

    impl TempRepo {
        fn empty() -> Self {
            static NEXT_TEMP_REPO_ID: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            let sequence = NEXT_TEMP_REPO_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let id = format!(
                "mdx-forge-run-route-test-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("time")
                    .as_nanos(),
                sequence
            );
            Self {
                path: std::env::temp_dir().join(id),
            }
        }

        fn with_work_queue(json: &str) -> Self {
            let repo = Self::empty();
            let path = repo.path.clone();
            let queue_dir = path.join("generated/agents");
            std::fs::create_dir_all(&queue_dir).expect("queue dir");
            let mut file =
                std::fs::File::create(queue_dir.join("mdx-agent-work-queue.json")).expect("queue");
            file.write_all(json.as_bytes()).expect("write queue");
            repo
        }

        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn temp_repo(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("mdx_{label}_{}_{}", std::process::id(), nonce));
        std::fs::create_dir_all(dir.join(".git")).expect("git");
        dir
    }
}
