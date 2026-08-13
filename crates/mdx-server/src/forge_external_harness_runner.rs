//! Direct execution for first-class external coding harnesses.
//!
//! Codex CLI and Grok Build receive the same isolated worker workspace,
//! write-scope check, selected proof, review policy, receipt trail, and
//! review-branch exit ramp as the native loop. They never receive ship
//! authority. Adapter output is bounded and kept out of receipts.

use crate::forge_loop_runner::{ForgeRunOutcome, ForgeRunRequest};
use crate::forge_turn_client::TurnClient;
use crate::harness_worker_workspace::WorkerWorkspace;
use mdx_core::{ForgeRunEvent, GovernedWriteIdentity, MdxKernel};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};
use std::time::Duration;

pub(crate) fn readiness(
    harness_id: &str,
    tenant_id: &str,
) -> Result<ExternalHarnessRuntime, String> {
    match harness_id {
        "codex_cli" => {
            let binary = codex_binary();
            if !binary_available(&binary) {
                return Err(
                    "Codex CLI is selected but its binary is not installed or visible to Forge."
                        .to_string(),
                );
            }
            Ok(ExternalHarnessRuntime {
                harness_id: "codex_cli",
                runner_id: "codex_cli_external_worker",
                provider_family: "openai",
                model_id: std::env::var("MDX_FORGE_CODEX_MODEL")
                    .unwrap_or_else(|_| "gpt-5.6-sol".to_string()),
                binary,
            })
        }
        "grok_build" => {
            let binary = PathBuf::from(
                std::env::var("MDX_GROK_BINARY").unwrap_or_else(|_| "grok".to_string()),
            );
            if !binary_available(&binary) {
                return Err(
                    "Grok Build is selected but its binary is not installed or visible to Forge."
                        .to_string(),
                );
            }
            if crate::secret_store::global()
                .provider_key_for_tenant(tenant_id, "XAI_API_KEY")
                .is_none()
            {
                return Err(
                    "Grok Build needs a tenant-scoped XAI_API_KEY from Model Center.".to_string(),
                );
            }
            Ok(ExternalHarnessRuntime {
                harness_id: "grok_build",
                runner_id: "grok_build_cli_external_worker",
                provider_family: "xai",
                model_id: std::env::var("MDX_FORGE_GROK_BUILD_MODEL")
                    .unwrap_or_else(|_| "grok-4.5".to_string()),
                binary,
            })
        }
        _ => Err(format!("unsupported external Forge harness: {harness_id}")),
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalHarnessRuntime {
    pub(crate) harness_id: &'static str,
    pub(crate) runner_id: &'static str,
    pub(crate) provider_family: &'static str,
    pub(crate) model_id: String,
    binary: PathBuf,
}

pub(crate) fn bind_model_choice(
    runtime: &mut ExternalHarnessRuntime,
    casting: &mut crate::forge_model_scorecard_route::BuilderCastingEvidence,
    requested_builder_slot: &str,
) -> Result<(), String> {
    if requested_builder_slot.trim().is_empty() {
        casting.status = "EXTERNAL_HARNESS_DEFAULT_MODEL_READY".to_string();
        casting.selected_builder_slot = runtime.runner_id.to_string();
        casting.selected_model_profile_id = match runtime.provider_family {
            "xai" => "codex_xai_responses_profile",
            _ => "codex_openai_responses_profile",
        }
        .to_string();
        casting.selected_provider_family = runtime.provider_family.to_string();
        casting.selected_model_id = runtime.model_id.clone();
        casting.basis = "external_harness_default_model".to_string();
        casting.requested_slot_matches_evidence = true;
        casting.ratified_grant_receipt_id.clear();
        return Ok(());
    }
    if casting.selected_model_id.trim().is_empty() {
        let slot = requested_builder_slot.trim().to_ascii_uppercase();
        let compatible_default = match runtime.provider_family {
            "xai" => matches!(slot.as_str(), "GROK" | "XAI"),
            "openai" => matches!(slot.as_str(), "GPT" | "CODEX" | "CODEXMINI"),
            _ => false,
        };
        if !compatible_default {
            return Err(format!(
                "The selected model slot {} has no runnable model identity.",
                requested_builder_slot.trim()
            ));
        }
        casting.selected_model_profile_id = match runtime.provider_family {
            "xai" => "codex_xai_responses_profile",
            _ => "codex_openai_responses_profile",
        }
        .to_string();
        casting.selected_provider_family = runtime.provider_family.to_string();
        casting.selected_model_id = runtime.model_id.clone();
    }
    if casting.selected_provider_family != runtime.provider_family {
        return Err(format!(
            "{} currently supports {} models, but slot {} resolves to {}. Choose a compatible model or MDx Native for cross-provider execution.",
            runtime.harness_id,
            runtime.provider_family,
            requested_builder_slot.trim(),
            casting.selected_provider_family
        ));
    }
    runtime.model_id = casting.selected_model_id.clone();
    casting.status = "EXTERNAL_HARNESS_OPERATOR_MODEL_READY".to_string();
    casting.basis = "operator_model_choice_compatible_with_harness".to_string();
    Ok(())
}

pub(crate) fn run(
    request: &ForgeRunRequest,
    repo_root: &Path,
    kernel: &Arc<RwLock<MdxKernel>>,
    runtime: &ExternalHarnessRuntime,
    planner_mode: &str,
    review_mode: &str,
) -> ForgeRunOutcome {
    let mut outcome = ForgeRunOutcome {
        run_id: request.run_id.clone(),
        status: "RUN_FAILED_TO_START",
        turns_used: 0,
        files_changed: 0,
        check_runs: 0,
        check_duration_ms: 0,
        branch: None,
        commit_sha: None,
        finish_summary: String::new(),
        last_check_passed: false,
    };
    if request.plan_only {
        return finish_without_workspace(
            kernel,
            request,
            outcome,
            "external harness execution is not admitted for a plan-only run",
        );
    }
    if !request
        .allowed_commands
        .iter()
        .any(|command| !command.trim().is_empty())
    {
        return finish_without_workspace(
            kernel,
            request,
            outcome,
            "external harness execution requires at least one selected proof command",
        );
    }
    let authority = match recorded_external_authority(kernel, request, runtime) {
        Ok(authority) => authority,
        Err(reason) => return finish_without_workspace(kernel, request, outcome, &reason),
    };
    let planning_contract = match external_planning_contract(kernel, request, planner_mode) {
        Ok(plan) => plan,
        Err(reason) => return finish_without_workspace(kernel, request, outcome, &reason),
    };
    let mut known_spend_microcents = planning_contract
        .as_ref()
        .map(ExternalPlanningContract::cost_microcents)
        .unwrap_or(0);
    if let Some(reason) = known_cost_breach(request, known_spend_microcents) {
        return finish_without_workspace(kernel, request, outcome, &reason);
    }
    let workspace = match WorkerWorkspace::create_for_forge(
        repo_root,
        &request.run_id,
        request.revise_branch.as_deref(),
        request.resume,
        &request.write_scope,
    ) {
        Ok(workspace) => workspace,
        Err(error) => {
            return finish_without_workspace(
                kernel,
                request,
                outcome,
                &format!("could not create the isolated external harness workspace: {error}"),
            );
        }
    };
    let workspace_path = workspace.path().to_path_buf();
    let prompt = external_prompt(
        request,
        runtime,
        planning_contract
            .as_ref()
            .map(|contract| contract.text.as_str()),
    );
    let artifact_dir = std::env::temp_dir()
        .join("mdx-forge-external-harness")
        .join(&request.run_id);
    if let Err(error) = std::fs::create_dir_all(&artifact_dir) {
        return finish_with_workspace(
            kernel,
            request,
            outcome,
            &workspace,
            &format!("could not prepare external harness evidence: {error}"),
        );
    }

    let mut command =
        match external_command(runtime, request, &workspace_path, &artifact_dir, &prompt) {
            Ok(command) => command,
            Err(reason) => {
                return finish_with_workspace(kernel, request, outcome, &workspace, &reason);
            }
        };
    record(
        kernel,
        request,
        "evidence_appended",
        "the external harness crossed its recorded live-execution boundary",
        1,
        0,
        0,
        &[
            ("adapter_execution_requested", "true"),
            ("adapter_execution_performed", "true"),
            ("adapter_execution_allowed", "true"),
            ("live_run_approval_id", authority.approval_id.as_str()),
            (
                "live_run_approval_receipt_id",
                authority.approval_receipt_id.as_str(),
            ),
            (
                "runner_execution_clearance_receipt_id",
                authority.clearance_receipt_id.as_str(),
            ),
            ("live_execution_provider", authority.provider.as_str()),
            (
                "live_run_max_spend_cents",
                authority.max_spend_cents.as_str(),
            ),
            ("live_run_task_ordinal", authority.task_ordinal.as_str()),
            (
                "live_execution_authority_scope",
                "single_governed_forge_run",
            ),
            ("quarantine_output_quarantined", "true"),
            ("quarantine_external_output_consumable", "false"),
        ],
    );
    let timeout_ms = request.max_runtime_ms.clamp(30_000, 4 * 60 * 60 * 1_000);
    let cancellation = crate::process_supervisor::cancellation_for_run(&request.run_id);
    let report = match crate::process_supervisor::run_supervised(
        &mut command,
        crate::process_supervisor::ProcessLimits::bounded(
            Duration::from_millis(timeout_ms),
            2 * 1024 * 1024,
        ),
        cancellation.as_ref(),
    ) {
        Ok(report) => report,
        Err(error) => {
            return finish_with_workspace(
                kernel,
                request,
                outcome,
                &workspace,
                &format!("{} could not start: {error}", runtime.harness_id),
            );
        }
    };
    // One supervised adapter invocation crossed the model boundary. The
    // configured turn ceiling is recorded separately because neither external
    // adapter currently exposes a trustworthy completed-turn count.
    outcome.turns_used = 1;
    let transcript_path = artifact_dir.join(format!("{}.jsonl", runtime.harness_id));
    let _ = std::fs::write(&transcript_path, &report.output_tail);
    record(
        kernel,
        request,
        "model_called",
        &format!(
            "external harness={} model={} exit={} duration_ms={}",
            runtime.harness_id,
            runtime.model_id,
            report.effective_exit_code(),
            report.duration_ms
        ),
        1,
        0,
        0,
        &[
            ("external_harness_id", runtime.harness_id),
            ("external_harness_runner_id", runtime.runner_id),
            ("external_harness_model_id", runtime.model_id.as_str()),
            (
                "external_harness_transcript_ref",
                transcript_path.to_string_lossy().as_ref(),
            ),
            ("external_harness_output_recorded", "false"),
            (
                "external_harness_cost_metering",
                "adapter_usage_unavailable",
            ),
            (
                "external_harness_cost_enforcement",
                if runtime.harness_id == "grok_build" {
                    "approval_ceiling_plus_turn_and_runtime_bounds_usage_metering_unavailable"
                } else {
                    "approval_ceiling_plus_runtime_and_scope_bounds_usage_and_turn_metering_unavailable"
                },
            ),
        ],
    );
    if !report.passed() {
        return finish_with_workspace(
            kernel,
            request,
            outcome,
            &workspace,
            &format!(
                "{} stopped before proof with exit code {}",
                runtime.harness_id,
                report.effective_exit_code()
            ),
        );
    }

    let changed_paths = git_changed_paths(&workspace_path);
    outcome.files_changed = changed_paths.len() as u32;
    let outside_scope = changed_paths
        .iter()
        .filter(|path| {
            !request.write_scope.is_empty()
                && !crate::forge_loop_runner::path_in_scope(path, &request.write_scope)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !outside_scope.is_empty() {
        return finish_with_workspace(
            kernel,
            request,
            outcome,
            &workspace,
            &format!(
                "external harness wrote outside the admitted scope: {}",
                outside_scope.join(", ")
            ),
        );
    }
    if changed_paths.is_empty() {
        return finish_with_workspace(
            kernel,
            request,
            outcome,
            &workspace,
            "external harness completed without a reviewable change",
        );
    }

    for command in request
        .allowed_commands
        .iter()
        .filter(|value| !value.trim().is_empty())
    {
        record(kernel, request, "check_started", command, 1, 0, 0, &[]);
        let (passed, exit_code, duration_ms, tail) = crate::forge_loop_runner::run_selected_check(
            &request.run_id,
            command,
            &workspace_path,
            repo_root,
            request.check_target_dir.as_deref(),
        );
        outcome.check_runs += 1;
        outcome.check_duration_ms = outcome.check_duration_ms.saturating_add(duration_ms);
        outcome.last_check_passed = passed;
        record(
            kernel,
            request,
            if passed {
                "check_passed"
            } else {
                "check_failed"
            },
            &format!(
                "run_command {command} exit={exit_code} tail={}",
                crate::forge_loop_runner::compact_check_tail(&tail)
            ),
            1,
            0,
            0,
            &[("external_harness_id", runtime.harness_id)],
        );
        if !passed {
            return finish_with_workspace(
                kernel,
                request,
                outcome,
                &workspace,
                &format!("selected proof failed after external harness execution: {command}"),
            );
        }
    }

    if review_mode != "deterministic_only" {
        let Some(diff) = crate::forge_finish_evaluator::workspace_diff(&workspace_path) else {
            record(
                kernel,
                request,
                "finish_evaluated",
                "independent review could not inspect the external harness diff",
                1,
                0,
                0,
                &[
                    ("finish_evaluator_verdict", "unavailable"),
                    (
                        "review_fail_open",
                        if review_mode == "required_independent" {
                            "false"
                        } else {
                            "true"
                        },
                    ),
                ],
            );
            if review_mode == "required_independent" {
                return finish_with_workspace(
                    kernel,
                    request,
                    outcome,
                    &workspace,
                    "required independent review could not inspect the external harness diff",
                );
            }
            return commit_external_result(kernel, request, outcome, workspace, runtime);
        };
        match crate::forge_finish_evaluator::evaluate_finish(
            kernel,
            &request.tenant_id,
            &request.intent,
            &diff,
            &request.reasoning_effort,
        ) {
            Some(verdict) if verdict.approved => {
                known_spend_microcents = known_spend_microcents.saturating_add(
                    crate::forge_model_pricing::turn_cost_microcents(
                        &verdict.model_id,
                        verdict.input_tokens,
                        verdict.output_tokens,
                    ),
                );
                record(
                    kernel,
                    request,
                    "finish_evaluated",
                    "independent reviewer approved the external harness result",
                    1,
                    verdict.input_tokens,
                    verdict.output_tokens,
                    &[
                        ("finish_evaluator_verdict", "approved"),
                        ("finish_evaluator_model_id", verdict.model_id.as_str()),
                    ],
                );
                if let Some(reason) = known_cost_breach(request, known_spend_microcents) {
                    return finish_with_workspace(kernel, request, outcome, &workspace, &reason);
                }
            }
            Some(verdict) => {
                record(
                    kernel,
                    request,
                    "finish_evaluated",
                    "independent reviewer found a gap in the external harness result",
                    1,
                    verdict.input_tokens,
                    verdict.output_tokens,
                    &[
                        ("finish_evaluator_verdict", "revise"),
                        ("finish_evaluator_concern", verdict.concern.as_str()),
                    ],
                );
                return finish_with_workspace(
                    kernel,
                    request,
                    outcome,
                    &workspace,
                    &format!("independent review requested revision: {}", verdict.concern),
                );
            }
            None if review_mode == "required_independent" => {
                record(
                    kernel,
                    request,
                    "finish_evaluated",
                    "required independent review model was unavailable",
                    1,
                    0,
                    0,
                    &[
                        ("finish_evaluator_verdict", "required_unavailable"),
                        ("review_fail_open", "false"),
                    ],
                );
                return finish_with_workspace(
                    kernel,
                    request,
                    outcome,
                    &workspace,
                    "required independent review model was unavailable",
                );
            }
            None => record(
                kernel,
                request,
                "finish_evaluated",
                "conditional independent review was unavailable and failed open",
                1,
                0,
                0,
                &[
                    ("finish_evaluator_verdict", "unavailable"),
                    ("review_fail_open", "true"),
                ],
            ),
        }
    } else {
        record(
            kernel,
            request,
            "finish_evaluated",
            "deterministic proof completed the selected review policy",
            1,
            0,
            0,
            &[("finish_evaluator_verdict", "skipped_deterministic_only")],
        );
    }

    commit_external_result(kernel, request, outcome, workspace, runtime)
}

#[derive(Clone, Debug)]
struct ExternalPlanningContract {
    text: String,
    model_id: String,
    input_tokens: u64,
    output_tokens: u64,
}

impl ExternalPlanningContract {
    fn cost_microcents(&self) -> u64 {
        crate::forge_model_pricing::turn_cost_microcents(
            &self.model_id,
            self.input_tokens,
            self.output_tokens,
        )
    }
}

fn external_planning_contract(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    mode: &str,
) -> Result<Option<ExternalPlanningContract>, String> {
    if mode == "skip" || request.resume {
        record(
            kernel,
            request,
            "planning_contract_unavailable",
            "adaptive strategy skipped a separate planning model for the external harness",
            0,
            0,
            0,
            &[("planning_mode", "skip"), ("planning_fail_open", "false")],
        );
        return Ok(None);
    }
    let planner = {
        let mut guard = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        match TurnClient::routed_for_role(
            &mut guard,
            &request.tenant_id,
            &request.actor_id,
            &request.run_id,
            "planner",
            &request.reasoning_effort,
        ) {
            Ok(Some(client)) => Some(client),
            Ok(None) => TurnClient::for_role_with_reasoning_effort(
                &guard,
                &request.tenant_id,
                "planner",
                &request.reasoning_effort,
            ),
            Err(_) => None,
        }
    };
    let Some(planner) = planner else {
        record(
            kernel,
            request,
            "planning_contract_unavailable",
            "the independent planning model was unavailable for the external harness",
            0,
            0,
            0,
            &[
                ("planning_mode", mode),
                (
                    "planning_fail_open",
                    if mode == "required" { "false" } else { "true" },
                ),
            ],
        );
        if mode == "required" {
            return Err("required independent planning model was unavailable".to_string());
        }
        return Ok(None);
    };
    let messages = vec![
        serde_json::json!({
            "role": "system",
            "content": "Create a compact implementation contract for a separate coding harness. Use headings Intent, Constraints, Approach, Risks, Proof, Done. Do not use tools, claim repo inspection, or grant authority."
        }),
        serde_json::json!({
            "role": "user",
            "content": format!("Task:\n{}\n\nAllowed proof:\n{}", request.intent, request.allowed_commands.join("\n"))
        }),
    ];
    let turn = match planner.complete(&messages) {
        Ok(turn) => turn,
        Err(_) => {
            record(
                kernel,
                request,
                "planning_contract_unavailable",
                "the independent planning call failed for the external harness",
                0,
                0,
                0,
                &[
                    ("planning_mode", mode),
                    (
                        "planning_fail_open",
                        if mode == "required" { "false" } else { "true" },
                    ),
                ],
            );
            if mode == "required" {
                return Err("required independent planning call failed".to_string());
            }
            return Ok(None);
        }
    };
    let text = turn.text.chars().take(6_000).collect::<String>();
    if text.trim().is_empty() {
        record(
            kernel,
            request,
            "planning_contract_unavailable",
            "the independent planning model returned an empty contract",
            0,
            turn.input_tokens,
            turn.output_tokens,
            &[
                ("planning_mode", mode),
                (
                    "planning_fail_open",
                    if mode == "required" { "false" } else { "true" },
                ),
            ],
        );
        if mode == "required" {
            return Err("required independent planning returned an empty contract".to_string());
        }
        return Ok(None);
    }
    record(
        kernel,
        request,
        "planning_contract_recorded",
        "independent planning contract was handed to the external harness",
        0,
        turn.input_tokens,
        turn.output_tokens,
        &[
            ("planning_model_id", planner.model_id()),
            ("planning_grants_authority", "false"),
        ],
    );
    Ok(Some(ExternalPlanningContract {
        text,
        model_id: planner.model_id().to_string(),
        input_tokens: turn.input_tokens,
        output_tokens: turn.output_tokens,
    }))
}

fn known_cost_breach(request: &ForgeRunRequest, spent_microcents: u64) -> Option<String> {
    let max_cost_cents = if request.max_cost_cents > 0 {
        request.max_cost_cents as u64
    } else {
        2_000
    };
    let spent_cents = crate::forge_model_pricing::microcents_to_cents_rounded_up(spent_microcents);
    (spent_cents >= max_cost_cents).then(|| {
        format!(
            "known planning and review spend exhausted the run budget: spent_cents={spent_cents} max_cost_cents={max_cost_cents}"
        )
    })
}

fn external_command(
    runtime: &ExternalHarnessRuntime,
    request: &ForgeRunRequest,
    workspace: &Path,
    artifact_dir: &Path,
    prompt: &str,
) -> Result<Command, String> {
    match runtime.harness_id {
        "codex_cli" => {
            let mut command = Command::new(&runtime.binary);
            crate::forge_exec_sandbox::apply_scrubbed_environment(&mut command);
            command
                .arg("--sandbox")
                .arg("workspace-write")
                .arg("--ask-for-approval")
                .arg("never")
                .arg("--cd")
                .arg(workspace)
                .arg("exec")
                .arg("--json")
                .arg("--ephemeral")
                .arg("--ignore-user-config");
            if !runtime.model_id.trim().is_empty() {
                command.arg("--model").arg(&runtime.model_id);
            }
            command.arg(prompt).current_dir(workspace);
            Ok(command)
        }
        "grok_build" => {
            let key = crate::secret_store::global()
                .provider_key_for_tenant(&request.tenant_id, "XAI_API_KEY")
                .ok_or_else(|| "tenant-scoped XAI_API_KEY became unavailable".to_string())?;
            let state = artifact_dir.join("grok-home");
            std::fs::create_dir_all(&state).map_err(|error| error.to_string())?;
            let adapter = repo_source_root().join("scripts/external-machine-adapter.mjs");
            if !adapter.is_file() {
                return Err(format!(
                    "Grok Build adapter is missing at {}",
                    adapter.display()
                ));
            }
            let mut command = crate::forge_exec_sandbox::networked_workspace_command(
                "node",
                workspace,
                std::slice::from_ref(&state),
            );
            command
                .arg(adapter)
                .arg("--adapter")
                .arg("grok-acp")
                .arg("--workspace")
                .arg(workspace)
                .env("MDX_EXTERNAL_MACHINE_PROMPT", prompt)
                .env("MDX_GROK_BINARY", &runtime.binary)
                .env("MDX_GROK_MODEL", &runtime.model_id)
                .env("MDX_GROK_MAX_TURNS", request.max_turns.max(1).to_string())
                .env(
                    "MDX_EXTERNAL_MACHINE_TIMEOUT_MS",
                    request
                        .max_runtime_ms
                        .clamp(30_000, 4 * 60 * 60 * 1_000)
                        .to_string(),
                )
                .env("MDX_GROK_HOME", &state)
                .env("XAI_API_KEY", key)
                .current_dir(workspace);
            Ok(command)
        }
        _ => Err("unsupported external harness".to_string()),
    }
}

fn external_prompt(
    request: &ForgeRunRequest,
    runtime: &ExternalHarnessRuntime,
    planning_contract: Option<&str>,
) -> String {
    format!(
        "You are executing one governed Forge build through {}. Work only in the current isolated repository. Do not access the web, spawn subagents, change git configuration, commit, push, publish, deploy, or touch paths outside the admitted scope. Make the smallest complete implementation. You may use the harness file tools. MDx will run the selected proof and create the review branch after you exit.\n\nTask:\n{}\n\nAdmitted write scope:\n{}\n\nSelected proof MDx will run:\n{}\n\nIndependent planning guidance (verify every assumption):\n{}",
        runtime.harness_id,
        request.intent,
        if request.write_scope.is_empty() {
            "repository scope".to_string()
        } else {
            request.write_scope.join("\n")
        },
        request.allowed_commands.join("\n"),
        planning_contract.unwrap_or("No separate planning turn was selected."),
    )
}

fn commit_external_result(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    mut outcome: ForgeRunOutcome,
    workspace: WorkerWorkspace,
    runtime: &ExternalHarnessRuntime,
) -> ForgeRunOutcome {
    let changed_paths = git_changed_paths(workspace.path());
    let branch = request.revise_branch.clone().unwrap_or_else(|| {
        format!(
            "forge/run-{}-{}",
            sanitize(&request.run_id),
            sanitize(workspace.id())
        )
    });
    let message = format!(
        "[forge] {}\n\nExternal harness {} completed run {} under MDx proof gates.",
        request
            .intent
            .lines()
            .next()
            .unwrap_or("external harness change"),
        runtime.harness_id,
        request.run_id
    );
    let committed = if request.revise_branch.is_some() {
        workspace.commit_onto_branch(&branch, &message)
    } else {
        workspace.commit_to_branch(&branch, &message)
    };
    let Some(sha) = committed else {
        return finish_with_workspace(
            kernel,
            request,
            outcome,
            &workspace,
            "external harness result could not be persisted to a review branch",
        );
    };
    outcome.files_changed = workspace
        .committed_file_count(&sha)
        .unwrap_or(outcome.files_changed);
    outcome.branch = Some(branch.clone());
    outcome.commit_sha = Some(sha.clone());
    outcome.status = "RUN_FINISHED_DONE";
    outcome.finish_summary = format!(
        "{} completed the change and MDx proof passed; the result is ready for human review",
        runtime.harness_id
    );
    let changed_paths_csv = changed_paths.join(",");
    record(
        kernel,
        request,
        "evidence_appended",
        &format!(
            "branch={branch} sha={} paths={changed_paths_csv}",
            &sha[..sha.len().min(12)]
        ),
        1,
        0,
        0,
        &[
            ("external_harness_id", runtime.harness_id),
            (
                "quarantine_status",
                "mdx_gates_passed_pending_principal_review",
            ),
            ("quarantine_output_quarantined", "true"),
            ("quarantine_external_output_consumable", "false"),
        ],
    );
    record(
        kernel,
        request,
        "run_finished",
        &format!(
            "status=RUN_FINISHED_DONE files_changed={} branch={branch} external_harness={}",
            outcome.files_changed, runtime.harness_id
        ),
        1,
        0,
        0,
        &[("external_harness_id", runtime.harness_id)],
    );
    outcome
}

#[derive(Clone, Debug)]
struct RecordedExternalAuthority {
    approval_id: String,
    approval_receipt_id: String,
    clearance_receipt_id: String,
    provider: String,
    max_spend_cents: String,
    task_ordinal: String,
}

fn recorded_external_authority(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    runtime: &ExternalHarnessRuntime,
) -> Result<RecordedExternalAuthority, String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let receipt = kernel
        .ledger()
        .entries()
        .iter()
        .rev()
        .find(|receipt| {
            receipt.kind == "forge.run.event"
                && receipt.tenant_id.as_str() == request.tenant_id
                && receipt.payload.get("run_id").map(String::as_str)
                    == Some(request.run_id.as_str())
                && receipt.payload.get("event").map(String::as_str) == Some("run_started")
        })
        .ok_or_else(|| "external harness run is missing its admission receipt".to_string())?;
    let value = |key: &str| {
        receipt
            .payload
            .get(key)
            .map(String::as_str)
            .unwrap_or("")
            .trim()
            .to_string()
    };
    let authority = RecordedExternalAuthority {
        approval_id: value("live_run_approval_id"),
        approval_receipt_id: value("live_run_approval_receipt_id"),
        clearance_receipt_id: value("runner_execution_clearance_receipt_id"),
        provider: value("live_execution_provider"),
        max_spend_cents: value("live_run_max_spend_cents"),
        task_ordinal: value("live_run_task_ordinal"),
    };
    let expected_provider = runtime.provider_family;
    if authority.approval_id.is_empty()
        || authority.approval_receipt_id.is_empty()
        || authority.clearance_receipt_id.is_empty()
        || authority.provider != expected_provider
        || authority
            .max_spend_cents
            .parse::<u32>()
            .ok()
            .filter(|value| *value > 0)
            .is_none()
        || authority
            .task_ordinal
            .parse::<u32>()
            .ok()
            .filter(|value| *value > 0)
            .is_none()
    {
        return Err(format!(
            "{} cannot execute because its clearance and bounded live-run approval were not fully bound at admission",
            runtime.harness_id
        ));
    }
    Ok(authority)
}

fn finish_without_workspace(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    mut outcome: ForgeRunOutcome,
    reason: &str,
) -> ForgeRunOutcome {
    outcome.status = "RUN_FINISHED_CANNOT_PROCEED";
    outcome.finish_summary = reason.to_string();
    record(
        kernel,
        request,
        "run_finished",
        &format!("status=RUN_FINISHED_CANNOT_PROCEED {reason}"),
        0,
        0,
        0,
        &[],
    );
    outcome
}

fn finish_with_workspace(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    mut outcome: ForgeRunOutcome,
    workspace: &WorkerWorkspace,
    reason: &str,
) -> ForgeRunOutcome {
    let changed = git_changed_paths(workspace.path());
    outcome.files_changed = changed.len() as u32;
    outcome.status = "RUN_FINISHED_CANNOT_PROCEED";
    outcome.finish_summary = reason.to_string();
    record(
        kernel,
        request,
        "run_finished",
        &format!(
            "status=RUN_FINISHED_CANNOT_PROCEED files_changed={} {reason}",
            outcome.files_changed
        ),
        1,
        0,
        0,
        &[],
    );
    outcome
}

#[allow(clippy::too_many_arguments)]
fn record(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    event: &str,
    detail: &str,
    turn: u32,
    input_tokens: u64,
    output_tokens: u64,
    evidence: &[(&str, &str)],
) {
    let Ok(mut kernel) = kernel.write() else {
        return;
    };
    let _ = kernel.record_forge_run_event_with_evidence_fields(
        ForgeRunEvent {
            tenant_id: &request.tenant_id,
            actor_id: &request.actor_id,
            run_id: &request.run_id,
            event,
            work_item_id: &request.work_item_id,
            detail: &detail.chars().take(1_900).collect::<String>(),
            turn,
            input_tokens,
            output_tokens,
        },
        &GovernedWriteIdentity::local_demo(&request.actor_id),
        evidence,
    );
}

fn git_changed_paths(workspace: &Path) -> Vec<String> {
    Command::new("git")
        .arg("-C")
        .arg(workspace)
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(|line| line.get(3..))
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(|line| line.split(" -> ").last().unwrap_or(line).to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn codex_binary() -> PathBuf {
    if let Ok(path) = std::env::var("MDX_FORGE_CODEX_BINARY") {
        return PathBuf::from(path);
    }
    let app = PathBuf::from("/Applications/Codex.app/Contents/Resources/codex");
    if app.is_file() {
        app
    } else if Path::new("/Applications/ChatGPT.app/Contents/Resources/codex").is_file() {
        PathBuf::from("/Applications/ChatGPT.app/Contents/Resources/codex")
    } else {
        PathBuf::from("codex")
    }
}

fn binary_available(binary: &Path) -> bool {
    if binary.components().count() > 1 {
        return binary.is_file();
    }
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|path| path.join(binary).is_file()))
}

fn repo_source_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn sanitize(value: &str) -> String {
    let cleaned = value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let cleaned = cleaned.trim_matches('-');
    if cleaned.is_empty() {
        "run".to_string()
    } else {
        cleaned.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> ForgeRunRequest {
        ForgeRunRequest {
            run_id: "forge_external_test".to_string(),
            tenant_id: "local_tenant".to_string(),
            actor_id: "agent:test".to_string(),
            work_item_id: "external_test".to_string(),
            intent: "Make a bounded test change".to_string(),
            allowed_commands: vec!["cargo check -p mdx-server".to_string()],
            max_turns: 2,
            revise_branch: None,
            resume: false,
            write_scope: vec!["crates/mdx-server/src".to_string()],
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "small".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "single".to_string(),
            execution_geometry_route: "http:/forge/runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 500,
            max_runtime_ms: 60_000,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: "low".to_string(),
        }
    }

    fn casting(
        provider: &str,
        model: &str,
    ) -> crate::forge_model_scorecard_route::BuilderCastingEvidence {
        crate::forge_model_scorecard_route::BuilderCastingEvidence {
            status: String::new(),
            requested_builder_slot: "MODEL".to_string(),
            selected_builder_slot: "MODEL".to_string(),
            recommended_builder_slot: String::new(),
            selected_model_profile_id: String::new(),
            selected_provider_family: provider.to_string(),
            selected_model_id: model.to_string(),
            recommended_model_profile_id: String::new(),
            recommended_provider_family: String::new(),
            recommended_model_id: String::new(),
            basis: String::new(),
            matching_eval_score_count: 0,
            accepted_eval_score_count: 0,
            matching_run_count: 0,
            done_rate_pct: 0,
            requested_slot_matches_evidence: false,
            ratified_grant_receipt_id: String::new(),
        }
    }

    #[test]
    fn external_branch_components_are_safe() {
        assert_eq!(sanitize(" run / grok "), "run---grok");
        assert_eq!(sanitize(""), "run");
    }

    #[test]
    fn unsupported_harness_fails_closed() {
        assert!(readiness("unknown", "local_tenant").is_err());
    }

    #[test]
    fn external_worker_refuses_plan_only_and_unproven_execution_before_authority() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let runtime = ExternalHarnessRuntime {
            harness_id: "codex_cli",
            runner_id: "codex_cli_external_worker",
            provider_family: "openai",
            model_id: "gpt-5.6-sol".to_string(),
            binary: PathBuf::from("codex"),
        };
        let mut plan = request();
        plan.plan_only = true;
        let outcome = run(
            &plan,
            Path::new("."),
            &kernel,
            &runtime,
            "bounded",
            "conditional_independent",
        );
        assert_eq!(outcome.status, "RUN_FINISHED_CANNOT_PROCEED");
        assert!(outcome.finish_summary.contains("plan-only"));

        let mut unproven = request();
        unproven.allowed_commands.clear();
        let outcome = run(
            &unproven,
            Path::new("."),
            &kernel,
            &runtime,
            "bounded",
            "conditional_independent",
        );
        assert_eq!(outcome.status, "RUN_FINISHED_CANNOT_PROCEED");
        assert!(outcome.finish_summary.contains("selected proof"));
    }

    #[test]
    fn external_harness_model_pairing_is_explicit_and_compatible() {
        let mut runtime = ExternalHarnessRuntime {
            harness_id: "grok_build",
            runner_id: "grok_build_cli_external_worker",
            provider_family: "xai",
            model_id: "grok-4.5".to_string(),
            binary: PathBuf::from("grok"),
        };
        let mut opus = casting("anthropic", "claude-opus-4.8");
        assert!(bind_model_choice(&mut runtime, &mut opus, "OPUS").is_err());

        let mut grok = casting("xai", "grok-4.5-fast");
        bind_model_choice(&mut runtime, &mut grok, "GROK").expect("compatible model");
        assert_eq!(runtime.model_id, "grok-4.5-fast");
        assert_eq!(grok.status, "EXTERNAL_HARNESS_OPERATOR_MODEL_READY");

        let mut defaulted_grok = casting("", "");
        bind_model_choice(&mut runtime, &mut defaulted_grok, "GROK")
            .expect("compatible slot inherits validated harness default");
        assert_eq!(defaulted_grok.selected_provider_family, "xai");
        assert_eq!(defaulted_grok.selected_model_id, "grok-4.5-fast");
        assert_eq!(
            defaulted_grok.status,
            "EXTERNAL_HARNESS_OPERATOR_MODEL_READY"
        );

        let mut unknown = casting("", "");
        assert!(bind_model_choice(&mut runtime, &mut unknown, "UNKNOWN").is_err());
    }

    #[test]
    fn external_review_branch_evidence_uses_admitted_quarantine_fields() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let request = request();
        record(
            &kernel,
            &request,
            "evidence_appended",
            "branch=forge/run-proof sha=abc123 paths=src/lib.rs",
            1,
            0,
            0,
            &[
                (
                    "quarantine_status",
                    "mdx_gates_passed_pending_principal_review",
                ),
                ("quarantine_output_quarantined", "true"),
                ("quarantine_external_output_consumable", "false"),
            ],
        );

        let guard = kernel.read().expect("kernel");
        let receipt = guard
            .ledger()
            .entries()
            .iter()
            .find(|receipt| {
                receipt.kind == "forge.run.event"
                    && receipt.payload.get("run_id").map(String::as_str)
                        == Some(request.run_id.as_str())
                    && receipt.payload.get("detail").map(String::as_str)
                        == Some("branch=forge/run-proof sha=abc123 paths=src/lib.rs")
            })
            .expect("branch evidence receipt");
        assert_eq!(receipt.payload["quarantine_output_quarantined"], "true");
        assert_eq!(
            receipt.payload["quarantine_external_output_consumable"],
            "false"
        );
        assert_eq!(receipt.payload["production_write_allowed"], "false");
    }
}
