// The Forge harness loop: the conductor that composes the proven
// primitives into a native coding agent. One run does the work a human
// engineer would hand to a coding harness - read the code, edit it, run
// the checks, read the failures, fix, repeat - in an isolated worktree
// the live repo never sees, until the gates pass or a budget stops it.
//
// What this module owns: the conversation, the turn loop, dispatching
// each tool the model calls to its gated primitive, and witnessing every
// turn as a kernel receipt in the DXR event grammar. What it does NOT
// own: the network (the turn client holds that, separately gated), the
// filesystem-of-record (the workspace is a throwaway worktree), or the
// ship decision (that stays a human verb).
//
// Prompts and command output never reach a receipt. Bounded, redacted
// operator-facing model prose does, so the product can feel like a teammate
// while the chain still holds receipt-backed facts.
#![allow(dead_code)]
use crate::forge_turn_client::{ParsedToolCall, TurnClient, tool_result_message};
use crate::harness_worker_workspace::WorkerWorkspace;
use mdx_core::{ForgeRunEvent, MdxKernel};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

const DEFAULT_MAX_TURNS: u32 = 60;
const DEFAULT_MODEL_TURN_TIMEOUT_SECS: u64 = 300;
const DEFAULT_MAX_OUTSTANDING_MODEL_TURNS: usize = 64;
const MAX_READ_BYTES: usize = 48_000;
const MAX_READ_LINES: usize = 900;
const SYSTEM_PROMPT: &str = "\
You are the Forge build agent for the MDx codebase - a serious coding agent, not a toy. You \
take real, multi-file work and land it correctly, proven by the project's own checks.\n\n\
Method:\n\
1. ORIENT first, but do not get stuck orienting. Use one or two semantic_query calls such as orientation, symbol_graph, file_outline, dependency_map, or related_tests to anchor the target, then read the files that matter. Once you have read the target file and know the edit, move to edit_file or run_command. Do not repeat semantic_query just to be safe. For a multi-file change, map all the \
sites before editing any. Before finishing a language-pack run that changed code, use semantic_query related_tests \
for the touched source path or symbol so review can see which tests were considered.\n\
2. PLAN. For multi-step work, call update_plan with 3 to 7 concrete steps (one in_progress) before your first edit, and update it as you finish each step - it makes your progress legible. Skip it for a one-step change. The plan tracks how you carry out the task; it does not change what the task asks, and a plan alone is never the deliverable. For a hard question that would take many reads (how does X work, why is this failing), delegate it with investigate so your own context stays on the change; after a non-trivial edit, you can verify it for an independent review before finishing.\n\
3. EDIT precisely. For an existing file use edit_file with an exact, uniquely-matching span - \
do not rewrite whole files. Use write_file only for brand-new files.\n\
4. PROVE it. Run the allowed check commands. Read the failures and fix them - iterate until \
the checks are green. A real engineer does not stop at the first error.\n\
5. FINISH. When the checks you ran pass, call finish with outcome \"done\" and a short \
operator-facing summary. Only call \"cannot_proceed\" if the task is genuinely impossible, and \
say precisely why.\n\n\
Rules: only run commands from the allowed list; keep changes minimal and idiomatic to the \
surrounding code; never invent files, paths, or commands. Match the existing style exactly. \
Make every source change through edit_file, write_file, or apply_patch - never rewrite files \
with shell tricks (cat, sed, echo, a python one-liner), so each change stays scoped and \
reviewable. Stay with the work end to end: do not stop at analysis, a diagnosis, or a \
half-finished fix - carry it through until the checks are green.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ForgeRunVoiceProfile {
    id: &'static str,
    turn_placeholder: &'static str,
    prompt_contract: &'static str,
}

const MD_CHILL_VOICE_PROMPT: &str = "\
Operator voice profile: md_chill.\n\
- Write visible turn prose and finish summaries like a capable teammate giving a quick engineering update in chat. Warm, plain, and first-person when useful. Avoid corporate status-report language and avoid slang.\n\
- Keep finish summaries short: 2 to 4 direct sentences. Lead with what changed or what blocked the run, then name the useful wiring such as files, routes, registry entries, or scripts, then name the checks that passed or failed.\n\
- Target energy example: \"I wired the welcome banner into Home.svelte and added the test alias the checks expect. Install and tests are green, so this is ready for review.\"\n\
- For blocked, failed, red-baseline, or cannot-proceed runs, stay especially calm and precise. Do not make serious outcomes sound casual.\n\
- Keep it engineer-legible. Facts first, human around the facts. Do not invent routes, files, checks, screenshots, shipped status, or production effects.\n\
- Do not use em dashes or en dashes. Use commas, periods, colons, or simple hyphens.";

const NEUTRAL_ENGINEERING_VOICE_PROMPT: &str = "\
Operator voice profile: neutral_engineering.\n\
- Write visible turn prose and finish summaries as concise engineering status.\n\
- Keep finish summaries short: 2 to 4 sentences. Name what changed, the files or routes involved, and the checks that passed.\n\
- Stay plain, specific, and honest. Do not invent routes, files, checks, screenshots, shipped status, or production effects.\n\
- Do not use em dashes or en dashes. Use commas, periods, colons, or simple hyphens.";

fn forge_run_voice_profile() -> ForgeRunVoiceProfile {
    match std::env::var("MDX_FORGE_RUN_VOICE_PROFILE")
        .unwrap_or_default()
        .trim()
    {
        "neutral_engineering" => ForgeRunVoiceProfile {
            id: "neutral_engineering",
            turn_placeholder: "Turn {turn}: choosing the next useful step.",
            prompt_contract: NEUTRAL_ENGINEERING_VOICE_PROMPT,
        },
        _ => ForgeRunVoiceProfile {
            id: "md_chill",
            turn_placeholder: "Turn {turn}: I'm picking the smallest useful next move.",
            prompt_contract: MD_CHILL_VOICE_PROMPT,
        },
    }
}

fn forge_run_system_prompt(profile: &ForgeRunVoiceProfile) -> String {
    format!("{SYSTEM_PROMPT}\n\n{}", profile.prompt_contract)
}

fn voice_turn_placeholder(profile: &ForgeRunVoiceProfile, turn: u32) -> String {
    profile
        .turn_placeholder
        .replace("{turn}", &turn.to_string())
}

#[derive(Clone, Debug)]
struct OperatorSummaryRender {
    summary: String,
    status: &'static str,
    model_provider_family: String,
    model_id: String,
    source: &'static str,
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Clone, Debug)]
struct PlanningContract {
    text: String,
    provider_family: String,
    model_id: String,
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Clone, Debug)]
struct RecordedRunStrategy {
    adaptive_enabled: bool,
    harness_id: String,
    model_provider_family: String,
    model_id: String,
    planner_mode: String,
    review_mode: String,
}

impl Default for RecordedRunStrategy {
    fn default() -> Self {
        Self {
            adaptive_enabled: false,
            harness_id: "mdx_native".to_string(),
            model_provider_family: String::new(),
            model_id: String::new(),
            planner_mode: "bounded".to_string(),
            review_mode: "conditional_independent".to_string(),
        }
    }
}

fn recorded_run_strategy(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    run_id: &str,
) -> RecordedRunStrategy {
    let Ok(kernel) = kernel.read() else {
        return RecordedRunStrategy::default();
    };
    let Some(receipt) = kernel.ledger().entries().iter().rev().find(|receipt| {
        receipt.kind == "forge.run.event"
            && receipt.tenant_id.as_str() == tenant_id
            && receipt.payload.get("run_id").map(String::as_str) == Some(run_id)
            && receipt.payload.get("event").map(String::as_str) == Some("run_started")
    }) else {
        return RecordedRunStrategy::default();
    };
    let value = |key: &str, fallback: &str| {
        receipt
            .payload
            .get(key)
            .filter(|value| !value.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| fallback.to_string())
    };
    RecordedRunStrategy {
        adaptive_enabled: value("run_strategy_enabled", "false") == "true",
        harness_id: value("run_strategy_harness_id", "mdx_native"),
        model_provider_family: value("builder_casting_selected_provider_family", ""),
        model_id: value("builder_casting_selected_model_id", ""),
        planner_mode: value("run_strategy_planner_mode", "bounded"),
        review_mode: value("run_strategy_review_mode", "conditional_independent"),
    }
}

fn create_planning_contract(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    builder: &TurnClient,
    context: &str,
    required: bool,
) -> Option<PlanningContract> {
    if request.plan_only || request.resume {
        return None;
    }
    let planner = {
        let mut guard = kernel.write().ok()?;
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
        record_with_evidence_fields(
            kernel,
            request,
            "planning_contract_unavailable",
            "the independent planning model was unavailable",
            0,
            &[
                ("planning_role", "planner"),
                (
                    "planning_fail_open",
                    if required { "false" } else { "true" },
                ),
            ],
        );
        return None;
    };
    let provider_family = planner.provider_family().to_string();
    let model_id = planner.model_id().to_string();
    let context = context.chars().take(12_000).collect::<String>();
    let messages = vec![
        serde_json::json!({
            "role": "system",
            "content": "You are the planning engineer in a governed software delivery team. Produce a compact implementation contract for the builder. Use exactly these headings: Intent, Constraints, Approach, Risks, Proof, Done. Name likely files or subsystems only when supported by the supplied context. Do not claim you inspected anything outside that context. Do not use tools. Do not grant authority. Output only the contract."
        }),
        serde_json::json!({
            "role": "user",
            "content": format!(
                "Task:\n{}\n\nAllowed proof commands:\n{}\n\nGoverned repository context:\n{}",
                request.intent,
                formatted_allowed_commands(&request.allowed_commands, request),
                context,
            )
        }),
    ];
    let started = Instant::now();
    let turn = match planner.complete(&messages) {
        Ok(turn) => turn,
        Err(_) => {
            record_with_evidence_fields(
                kernel,
                request,
                "planning_contract_unavailable",
                "the independent planning call failed",
                0,
                &[
                    ("planning_role", "planner"),
                    (
                        "planning_fail_open",
                        if required { "false" } else { "true" },
                    ),
                ],
            );
            return None;
        }
    };
    if let Ok(mut guard) = kernel.write() {
        let _ = planner.record_route_outcome(
            &mut guard,
            &request.tenant_id,
            &request.actor_id,
            &turn,
            started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            "forge_planning_runtime",
        );
    }
    let text = bounded_human_run_text(&turn.text, 6_000);
    if text.trim().is_empty() {
        record_with_evidence_fields(
            kernel,
            request,
            "planning_contract_unavailable",
            "the independent planning model returned an empty contract",
            0,
            &[
                ("planning_role", "planner"),
                (
                    "planning_fail_open",
                    if required { "false" } else { "true" },
                ),
            ],
        );
        return None;
    }
    let independent = model_id != builder.model_id();
    let detail = format!(
        "planning contract recorded by model={} independent_from_builder={independent}",
        model_id
    );
    record_timed(
        kernel,
        request,
        TimedRunEvent {
            event: "model_called",
            detail: &detail,
            turn: 0,
            input_tokens: turn.input_tokens,
            output_tokens: turn.output_tokens,
            duration_ms: started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        },
    );
    record_with_evidence_fields(
        kernel,
        request,
        "planning_contract_recorded",
        "a bounded planning contract was handed to the builder as guidance before execution",
        0,
        &[
            ("planning_role", "planner"),
            ("planning_provider_family", &provider_family),
            ("planning_model_id", &model_id),
            (
                "planning_model_independent_from_builder",
                if independent { "true" } else { "false" },
            ),
            ("planning_grants_authority", "false"),
        ],
    );
    Some(PlanningContract {
        text,
        provider_family,
        model_id,
        input_tokens: turn.input_tokens,
        output_tokens: turn.output_tokens,
    })
}

fn render_operator_run_summary(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    outcome: &ForgeRunOutcome,
    profile: &ForgeRunVoiceProfile,
    factual_summary: &str,
) -> OperatorSummaryRender {
    if factual_summary.trim().is_empty() {
        return OperatorSummaryRender {
            summary: String::new(),
            status: "skipped_empty_summary",
            model_provider_family: String::new(),
            model_id: String::new(),
            source: "raw_run_summary",
            input_tokens: 0,
            output_tokens: 0,
        };
    }
    let Some(client) = voice_render_client(kernel, request) else {
        return OperatorSummaryRender {
            summary: factual_summary.to_string(),
            status: "fallback_no_voice_model",
            model_provider_family: String::new(),
            model_id: String::new(),
            source: "raw_run_summary",
            input_tokens: 0,
            output_tokens: 0,
        };
    };
    let provider_family = client.provider_family().to_string();
    let model_id = client.model_id().to_string();
    let fact_source = voice_fact_source(request, outcome, factual_summary);
    let messages = vec![
        serde_json::json!({
            "role": "system",
            "content": format!(
                "You rewrite Forge run summaries for the operator. This is a tone pass only. Preserve the same facts and do not add claims, files, routes, checks, screenshots, shipping status, deployment status, or production effects. Output only the rewritten summary. Keep it 2 to 4 short sentences. No em dashes or en dashes.\n\n{}",
                profile.prompt_contract
            )
        }),
        serde_json::json!({
            "role": "user",
            "content": format!(
                "Voice profile: {}\n\nFactual source:\n{}\n\nRewrite the factual source into the selected voice. Same facts only.",
                profile.id, fact_source
            )
        }),
    ];
    let started = Instant::now();
    let turn = match client.complete(&messages) {
        Ok(turn) => turn,
        Err(_) => {
            if let Ok(mut guard) = kernel.write() {
                let _ = crate::model_fabric_service::persist_pending_model_runtime_evidence(
                    &mut guard,
                    &request.tenant_id,
                    &request.actor_id,
                );
            }
            return OperatorSummaryRender {
                summary: factual_summary.to_string(),
                status: "fallback_voice_model_error",
                model_provider_family: provider_family,
                model_id,
                source: "raw_run_summary",
                input_tokens: 0,
                output_tokens: 0,
            };
        }
    };
    if let Ok(mut guard) = kernel.write() {
        let _ = client.record_route_outcome(
            &mut guard,
            &request.tenant_id,
            &request.actor_id,
            &turn,
            started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
            "forge_voice_runtime",
        );
    }
    let candidate = bounded_human_run_text(&turn.text, 700);
    if operator_summary_claims_safe(&fact_source, &candidate) {
        OperatorSummaryRender {
            summary: candidate,
            status: "rewritten",
            model_provider_family: provider_family,
            model_id,
            source: "voice_model_tone_rewrite",
            input_tokens: turn.input_tokens,
            output_tokens: turn.output_tokens,
        }
    } else {
        OperatorSummaryRender {
            summary: factual_summary.to_string(),
            status: "fallback_claim_guard",
            model_provider_family: provider_family,
            model_id,
            source: "raw_run_summary",
            input_tokens: turn.input_tokens,
            output_tokens: turn.output_tokens,
        }
    }
}

fn voice_render_client(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
) -> Option<TurnClient> {
    let mut guard = kernel.write().ok()?;
    match TurnClient::routed_for_role(
        &mut guard,
        &request.tenant_id,
        &request.actor_id,
        &request.run_id,
        "voice",
        &request.reasoning_effort,
    ) {
        Ok(Some(client)) => Some(client),
        Ok(None) => TurnClient::for_role_with_reasoning_effort(
            &guard,
            &request.tenant_id,
            "voice",
            &request.reasoning_effort,
        )
        .or_else(|| {
            TurnClient::for_role_with_reasoning_effort(
                &guard,
                &request.tenant_id,
                "reviewer",
                &request.reasoning_effort,
            )
        })
        .or_else(|| {
            TurnClient::for_role_with_reasoning_effort(
                &guard,
                &request.tenant_id,
                "planner",
                &request.reasoning_effort,
            )
        }),
        Err(_) => None,
    }
}

fn voice_fact_source(
    request: &ForgeRunRequest,
    outcome: &ForgeRunOutcome,
    factual_summary: &str,
) -> String {
    format!(
        "Task: {}\nStatus: {}\nTurns: {}\nFiles changed: {}\nCheck cost: {} runs / {} ms\nLast selected proof passed: {}\nFactual builder summary: {}",
        first_line(&request.intent),
        outcome.status,
        outcome.turns_used,
        outcome.files_changed,
        outcome.check_runs,
        outcome.check_duration_ms,
        outcome.last_check_passed,
        factual_summary
    )
}

fn operator_summary_claims_safe(fact_source: &str, candidate: &str) -> bool {
    let trimmed = candidate.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 700 {
        return false;
    }
    if trimmed.contains('\u{2014}') || trimmed.contains('\u{2013}') {
        return false;
    }
    let fact_lower = fact_source.to_ascii_lowercase();
    let candidate_lower = trimmed.to_ascii_lowercase();
    if operator_summary_tone_is_too_casual(&candidate_lower) {
        return false;
    }
    for risky in [
        "deployed",
        "deployment",
        "merged",
        "released",
        "published",
        "production",
        "screenshot",
        "screenshotted",
        "shipped",
    ] {
        if candidate_lower.contains(risky) && !fact_lower.contains(risky) {
            return false;
        }
    }
    for token in codeish_claim_tokens(trimmed) {
        if !fact_source.contains(&token) {
            return false;
        }
    }
    true
}

fn operator_summary_tone_is_too_casual(candidate_lower: &str) -> bool {
    [
        "yo,",
        "yo ",
        "we good",
        "lil ",
        "snuck in",
        "threw in",
        "vibes",
        "\u{2728}",
        "\u{1f680}",
    ]
    .iter()
    .any(|marker| candidate_lower.contains(marker))
}

fn codeish_claim_tokens(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|token| {
            token
                .trim_matches(|c: char| {
                    matches!(
                        c,
                        ',' | '.'
                            | ';'
                            | ':'
                            | '('
                            | ')'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | '\''
                            | '"'
                            | '`'
                    )
                })
                .to_string()
        })
        .filter(|token| {
            let lower = token.to_ascii_lowercase();
            token.contains('/')
                || token.contains("#/")
                || lower.ends_with(".svelte")
                || lower.ends_with(".js")
                || lower.ends_with(".ts")
                || lower.ends_with(".tsx")
                || lower.ends_with(".rs")
                || lower.ends_with(".json")
                || lower.ends_with(".md")
                || matches!(
                    lower.as_str(),
                    "pnpm" | "npm" | "cargo" | "pytest" | "make" | "yarn"
                )
        })
        .collect()
}

/// What the caller asks the run to do.
#[derive(Clone, Debug)]
pub(crate) struct ForgeRunRequest {
    pub run_id: String,
    pub tenant_id: String,
    pub actor_id: String,
    pub work_item_id: String,
    pub intent: String,
    /// Exact command strings the model may run (allowlist). Each runs in
    /// the sandbox against the workspace.
    pub allowed_commands: Vec<String>,
    /// Turn budget for this run; 0 means the default. Bigger work asks
    /// for more.
    pub max_turns: u32,
    /// When set, this run REVISES an existing branch instead of starting
    /// fresh: the workspace opens at that branch and the result commits
    /// back onto it. The intent is the reviewer's request.
    pub revise_branch: Option<String>,
    /// When true, this run RESUMES a prior run of the same run_id: the loop
    /// loads the persisted transcript and continues the conversation from the
    /// last turn instead of redoing from turn 0. A verified durable workspace
    /// checkpoint restores in-scope filesystem state at its original base; if
    /// no checkpoint exists, resume still restores the reasoning transcript.
    pub resume: bool,
    /// When non-empty, the ONLY paths this run may edit or create - the
    /// fleet conflict law enforced, not asked. Reads stay workspace-wide
    /// (a stream may study its neighbors; it may not write them). Empty
    /// keeps the original single-run behavior: the whole worktree.
    pub write_scope: Vec<String>,
    /// Execution target for admitted checks. A `hosted://` reference routes
    /// every proof through the hosted sandbox. Otherwise this is the Cargo
    /// target dir override for host checks. Parallel fleet streams each get
    /// their own so concurrent builds never fight over one cache; empty keeps
    /// the shared warm cache of a single run.
    pub check_target_dir: Option<String>,
    /// Which builder slot codes this run. Empty = the default builder;
    /// a name selects MDX_FLEET_BUILDER_<NAME>_* - the A/B/C/D lever:
    /// same task, different coder, scores on the same chain.
    pub builder_slot: String,
    /// Structured semantic policy from run intake/classification. This is
    /// separate from prompt text so medium/high work can require concrete
    /// semantic orientation even for a single worker.
    pub work_complexity_tier: String,
    pub semantic_policy_required_operations: Vec<String>,
    pub semantic_policy_source: String,
    /// Execution geometry recorded at admission. Direct run width is advisory
    /// until the fleet conductor launches multiple workers; fleet streams use
    /// this to know they are one worker inside a wider governed run.
    pub execution_geometry_requested_workers: u32,
    pub execution_geometry_effective_workers: u32,
    pub execution_geometry_lane: String,
    pub execution_geometry_route: String,
    /// Optional long-horizon mission attachment. When present, the route
    /// validates the mission and milestone before admission, then the run
    /// terminal outcome can record a mission checkpoint receipt.
    pub mission_id: String,
    pub mission_milestone_id: String,
    /// Cost ceiling for this run in cents; 0 takes the server default
    /// (MDX_FORGE_RUN_MAX_COST_CENTS, default 2000). Enforced at every
    /// turn boundary against priced token spend - exhaustion ends the run
    /// as RUN_BUDGET_EXHAUSTED with the spend on the receipt.
    pub max_cost_cents: u32,
    /// Wall-clock ceiling for this run in milliseconds; 0 takes the
    /// server default (MDX_FORGE_RUN_MAX_RUNTIME_MS, default 45 minutes).
    /// Enforced at every turn boundary.
    pub max_runtime_ms: u64,
    /// Optional autonomy envelope this run is bound to. Validated at
    /// admission; the loop polls revocation alongside operator controls,
    /// so revoking the envelope stops in-flight work.
    pub envelope_id: String,
    /// Plan mode: when true the run PROPOSES a plan (steps + a recommended
    /// casting) and stops WITHOUT editing or running proof - the edit tools are
    /// refused. Execution is a separate, human-ratified run. This is the
    /// governed "propose then execute" workflow.
    pub plan_only: bool,
    /// Per-run reasoning effort override (minimal|low|medium|high|xhigh); empty
    /// falls back to the MDX_FORGE_REASONING_EFFORT env default. Set per run so
    /// the operator's control-surface choice binds to this run, not the whole
    /// process (concurrent runs can differ).
    pub reasoning_effort: String,
}

/// The run's terminal shape - presence only, for the projection.
#[derive(Clone, Debug)]
pub(crate) struct ForgeRunOutcome {
    pub run_id: String,
    pub status: &'static str,
    pub turns_used: u32,
    pub files_changed: u32,
    pub check_runs: u32,
    pub check_duration_ms: u64,
    pub branch: Option<String>,
    pub commit_sha: Option<String>,
    pub finish_summary: String,
    pub last_check_passed: bool,
}

fn selected_proof_status(outcome: &ForgeRunOutcome) -> &'static str {
    if outcome.check_runs == 0 {
        "not_run"
    } else if outcome.last_check_passed {
        "passed"
    } else {
        "failed"
    }
}

fn preserve_selected_proof_truth_on_budget_exhaustion(outcome: &mut ForgeRunOutcome) {
    if outcome.status != "RUN_BUDGET_EXHAUSTED" || outcome.check_runs == 0 {
        return;
    }
    let proof_status = selected_proof_status(outcome);
    let existing = outcome.finish_summary.trim().trim_end_matches('.');
    outcome.finish_summary = if existing.is_empty() {
        format!("The latest selected check {proof_status} before the run exhausted its budget.")
    } else {
        format!("{existing}. The latest selected check {proof_status}.")
    };
}

#[derive(Clone, Debug)]
struct BaselineProofPreflight {
    note: String,
    selected_proof_red_on_arrival: bool,
    baseline_repair_expected: bool,
}

/// Run the loop to completion. Blocking: the caller runs this on its own
/// thread (a run takes minutes). Each receipt is written under a brief
/// kernel lock; the model calls and sandbox runs happen with NO lock held
/// - the suspend-for-external-call discipline, applied to the whole loop.
pub(crate) fn run_forge_loop(
    request: &ForgeRunRequest,
    repo_root: &Path,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> ForgeRunOutcome {
    // Keep this run addressable for its entire worker lifetime. The durable
    // run-control receipt still decides the loop outcome; this token is the
    // fast path that interrupts a supervised command between turn boundaries.
    let _run_cancellation = crate::process_supervisor::register_run_cancellation(&request.run_id);
    let run_started_at = Instant::now();
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
    let run_strategy = recorded_run_strategy(kernel, &request.tenant_id, &request.run_id);
    record_with_evidence_fields(
        kernel,
        request,
        "evidence_appended",
        "the recorded adaptive run strategy was bound to the worker loop",
        0,
        &[
            ("run_strategy_harness_id", run_strategy.harness_id.as_str()),
            (
                "run_strategy_enabled",
                if run_strategy.adaptive_enabled {
                    "true"
                } else {
                    "false"
                },
            ),
            (
                "run_strategy_planner_mode",
                run_strategy.planner_mode.as_str(),
            ),
            (
                "run_strategy_review_mode",
                run_strategy.review_mode.as_str(),
            ),
            ("run_strategy_grants_execution_authority", "false"),
        ],
    );
    if run_strategy.harness_id != "mdx_native" {
        let mut runtime = match crate::forge_external_harness_runner::readiness(
            &run_strategy.harness_id,
            &request.tenant_id,
        ) {
            Ok(runtime) => runtime,
            Err(reason) => {
                record(kernel, request, "run_finished", &reason, 0, 0, 0);
                outcome.status = "RUN_FINISHED_CANNOT_PROCEED";
                outcome.finish_summary = reason;
                return outcome;
            }
        };
        if run_strategy.model_provider_family != runtime.provider_family
            || run_strategy.model_id.trim().is_empty()
        {
            let reason = format!(
                "{} cannot execute because its admitted model pairing is missing or incompatible",
                run_strategy.harness_id
            );
            record(kernel, request, "run_finished", &reason, 0, 0, 0);
            outcome.status = "RUN_FINISHED_CANNOT_PROCEED";
            outcome.finish_summary = reason;
            return outcome;
        }
        runtime.model_id = run_strategy.model_id.clone();
        return crate::forge_external_harness_runner::run(
            request,
            repo_root,
            kernel,
            &runtime,
            &run_strategy.planner_mode,
            &run_strategy.review_mode,
        );
    }

    // Resolve the model while holding the kernel write lock, but carry any
    // refusal out of the scope before recording it. Recording also takes the
    // write lock, so attempting it from the error arm below used to deadlock
    // the entire local kernel when provider routing refused a builder.
    let client_resolution = {
        let mut guard = match kernel.write() {
            Ok(guard) => guard,
            Err(_) => {
                record(
                    kernel,
                    request,
                    "run_finished",
                    "kernel lock poisoned before model resolution",
                    0,
                    0,
                    0,
                );
                outcome.finish_summary = "kernel lock poisoned before model resolution".to_string();
                return outcome;
            }
        };
        match TurnClient::routed_builder_for_tenant(
            &mut guard,
            &request.tenant_id,
            &request.actor_id,
            &request.run_id,
            &request.builder_slot,
        ) {
            Ok(Some(client)) => Ok(Some(client)),
            Ok(None) => Ok(TurnClient::builder_for_tenant(
                &guard,
                &request.tenant_id,
                &request.builder_slot,
            )),
            Err(error) => Err(error),
        }
    };
    let client = match client_resolution {
        Ok(client) => client,
        Err(error) => {
            record(kernel, request, "run_finished", &error, 0, 0, 0);
            outcome.finish_summary = error;
            return outcome;
        }
    };
    let Some(client) = client else {
        record(
            kernel,
            request,
            "run_finished",
            "no live build model configured",
            0,
            0,
            0,
        );
        outcome.finish_summary = "no live build model configured".to_string();
        return outcome;
    };
    // Bind the operator's per-run reasoning effort (control surface) to this
    // client, overriding the env default for this run only.
    let client = client.with_reasoning_effort(&request.reasoning_effort);
    // The subagent role registry (Amp's Librarian pattern): read-only
    // investigations resolve to a dedicated investigator role when one is
    // configured (MDX_FLEET_INVESTIGATOR_*), so exploration can run on a fast,
    // cheap model distinct from the builder. Falls back to the builder client
    // when no such role is set - no behavior change by default.
    let investigator_client = {
        let guard = kernel.write().ok();
        guard.and_then(|mut k| {
            match TurnClient::routed_for_role(
                &mut k,
                &request.tenant_id,
                &request.actor_id,
                &request.run_id,
                "investigator",
                &request.reasoning_effort,
            ) {
                Ok(Some(client)) => Some(client),
                Ok(None) => TurnClient::for_role_with_reasoning_effort(
                    &k,
                    &request.tenant_id,
                    "investigator",
                    &request.reasoning_effort,
                )
                .or_else(|| Some(client.clone())),
                Err(_) => None,
            }
        })
    };
    // A revise run opens at the branch it is revising; a fresh run opens at
    // HEAD. Resume first verifies and reapplies any durable scoped checkpoint.
    let workspace = match WorkerWorkspace::create_for_forge(
        repo_root,
        &request.run_id,
        request.revise_branch.as_deref(),
        request.resume,
        &request.write_scope,
    ) {
        Ok(workspace) => workspace,
        Err(error) => {
            let detail = format!("could not create or recover the isolated workspace: {error}");
            record(kernel, request, "run_finished", &detail, 0, 0, 0);
            outcome.finish_summary = detail;
            return outcome;
        }
    };
    if let Some(checkpoint) = workspace.recovered_checkpoint() {
        let sequence = checkpoint.sequence.to_string();
        let path_count = checkpoint.paths.len().to_string();
        record(
            kernel,
            request,
            "workspace_rehydrated",
            &format!(
                "rehydrated checkpoint {} at parent {} with {} in-scope path(s)",
                &checkpoint.checkpoint_sha[..checkpoint.checkpoint_sha.len().min(12)],
                &checkpoint.parent_sha[..checkpoint.parent_sha.len().min(12)],
                checkpoint.paths.len()
            ),
            0,
            0,
            0,
        );
        record_with_evidence_fields(
            kernel,
            request,
            "evidence_appended",
            "verified durable workspace checkpoint before resume",
            0,
            &[
                ("workspace_checkpoint_sequence", sequence.as_str()),
                ("workspace_checkpoint_path_count", path_count.as_str()),
                (
                    "workspace_checkpoint_sha",
                    checkpoint.checkpoint_sha.as_str(),
                ),
                ("workspace_rehydrated", "true"),
            ],
        );
    }
    let workspace_path = workspace.path().to_path_buf();
    if let Some(snapshot) = workspace.dirty_base_snapshot() {
        record(
            kernel,
            request,
            "evidence_appended",
            &format!(
                "local_dirty_base_snapshot applied tracked_files={} untracked_files={} commit_sha={} untracked_included={} live_repo_mutated=false",
                snapshot.file_count,
                snapshot.untracked_file_count,
                snapshot.commit_sha,
                snapshot.untracked_file_count > 0
            ),
            0,
            0,
            0,
        );
    }

    // The run_started bracket was already witnessed synchronously by the
    // route when it accepted the run; the loop opens with its work. The
    // model identity rides the first model_called event.

    // Context the agent could not get anywhere else: the repo's own
    // conventions and the work item this run serves, assembled from MDx's
    // memory before a single line is written. This is what a generic
    // coding agent cannot do - it has no company memory to draw on.
    let assembled = assemble_context(&workspace_path, request, kernel);
    let context = assembled.text;
    if assembled.flywheel_context_suppressed {
        // Memory-off baseline arm of the ablation harness: the two flywheel
        // injection blocks were withheld. Receipt it so no arm runs silently
        // and /forge/flywheel-proof.json can classify this run into the off arm.
        record(
            kernel,
            request,
            "evidence_appended",
            "flywheel context suppressed for ablation (MDX_FORGE_FLYWHEEL_CONTEXT=off): outcome-signal and active-learning-memory blocks withheld from this run",
            0,
            0,
            0,
        );
    }
    let mut orientation_gate = PrincipalOrientationGate::for_request(&workspace_path, request);
    record_semantic_strategy_assignment(kernel, request, &orientation_gate);
    record_timed(
        kernel,
        request,
        TimedRunEvent {
            event: "evidence_appended",
            detail: &format!(
                "phase=intake context_chars={} language_pack={} quality_signals={} review_axes={} principal_review_gates={} semantic_anchors={} semantic_tool_readiness={} toolchain_readiness={} repo_standards_sources={} standards={} outcomes={} active_memories={} installed_capabilities={}",
                context.len(),
                if assembled.language_pack_id.is_empty() {
                    "none"
                } else {
                    assembled.language_pack_id.as_str()
                },
                assembled.quality_signal_count,
                assembled.review_axis_count,
                assembled.principal_review_gate_count,
                assembled.semantic_anchor_count,
                assembled.semantic_tool_readiness_count,
                assembled.toolchain_readiness_count,
                assembled.repo_standards_source_count,
                assembled.standards.len(),
                assembled.outcome_signal_count,
                assembled.active_memory_count,
                assembled.installed_capability_count
            ),
            turn: 0,
            input_tokens: 0,
            output_tokens: 0,
            duration_ms: run_started_at.elapsed().as_millis() as u64,
        },
    );
    if !context.is_empty() {
        record(
            kernel,
            request,
            "evidence_appended",
            &format!("context assembled ({} chars)", context.len()),
            0,
            0,
            0,
        );
        if assembled.outcome_signal_count > 0
            || assembled.active_memory_count > 0
            || assembled.installed_capability_count > 0
        {
            record(
                kernel,
                request,
                "evidence_appended",
                &format!(
                    "flywheel context assembled: outcomes={} active_memories={} installed_capabilities={}",
                    assembled.outcome_signal_count,
                    assembled.active_memory_count,
                    assembled.installed_capability_count
                ),
                0,
                0,
                0,
            );
        }
        if assembled.active_memory_count > 0 {
            // Honest citation as selection turns selective: the receipt names
            // which lessons rode in and which active lessons were passed
            // over, so /forge/flywheel-proof.json can show the real basis.
            record(
                kernel,
                request,
                "evidence_appended",
                &format!(
                    "active learning memory cited advisory_count={} selection_basis=relevance_v1 matched_memory_ids={} passed_over_count={} passed_over_memory_ids={}",
                    assembled.active_memory_count,
                    joined_memory_ids_or_none(&assembled.matched_memory_ids),
                    assembled.passed_over_memory_ids.len(),
                    joined_memory_ids_or_none(&assembled.passed_over_memory_ids)
                ),
                0,
                0,
                0,
            );
        }
        if assembled.distilled_skill_count > 0 {
            // Procedural-memory citation: name which ratified distilled skills
            // rode into this run so the provenance is auditable from the run's
            // own trace, exactly like the advisory-lesson citation above.
            record(
                kernel,
                request,
                "evidence_appended",
                &format!(
                    "distilled skill procedure cited advisory_count={} selection_basis=relevance_v1 matched_skill_ids={}",
                    assembled.distilled_skill_count,
                    joined_memory_ids_or_none(&assembled.cited_distilled_skill_ids)
                ),
                0,
                0,
                0,
            );
        }
        if !assembled.language_pack_id.is_empty() {
            record(
                kernel,
                request,
                "evidence_appended",
                &format!(
                    "language pack guidance applied: {}",
                    assembled.language_pack_id
                ),
                0,
                0,
                0,
            );
        }
        if assembled.quality_signal_count > 0 {
            record(
                kernel,
                request,
                "evidence_appended",
                &format!(
                    "repo quality signals applied: count={}",
                    assembled.quality_signal_count
                ),
                0,
                0,
                0,
            );
        }
        if assembled.repo_standards_source_count > 0 {
            record(
                kernel,
                request,
                "evidence_appended",
                &format!(
                    "repo standards sources applied: count={}",
                    assembled.repo_standards_source_count
                ),
                0,
                0,
                0,
            );
        }
        if assembled.review_axis_count > 0 {
            record(
                kernel,
                request,
                "evidence_appended",
                &format!(
                    "language pack review axes applied: count={}",
                    assembled.review_axis_count
                ),
                0,
                0,
                0,
            );
        }
        if assembled.principal_review_gate_count > 0 {
            record(
                kernel,
                request,
                "evidence_appended",
                &format!(
                    "principal review gates applied: count={}",
                    assembled.principal_review_gate_count
                ),
                0,
                0,
                0,
            );
        }
        if assembled.semantic_anchor_count > 0 {
            record_semantic_preflight(
                kernel,
                request,
                &assembled.semantic_preflight,
                run_started_at.elapsed().as_millis() as u64,
            );
        }
        if assembled.toolchain_readiness_count > 0 {
            record(
                kernel,
                request,
                "evidence_appended",
                &format!(
                    "toolchain readiness applied: count={}",
                    assembled.toolchain_readiness_count
                ),
                0,
                0,
                0,
            );
        }
        if assembled.semantic_tool_readiness_count > 0 {
            record(
                kernel,
                request,
                "evidence_appended",
                &format!(
                    "semantic tool readiness applied: count={}",
                    assembled.semantic_tool_readiness_count
                ),
                0,
                0,
                0,
            );
        }
    }
    // Cite the standards this build is following, in the run's own trail, so
    // an engineer sees it is building to your rules - the credibility hook.
    if !assembled.standards.is_empty() {
        record(
            kernel,
            request,
            "evidence_appended",
            &format!(
                "building to your standards: {}",
                assembled.standards.join(", ")
            ),
            0,
            0,
            0,
        );
    }

    // The conversation. The system prompt and the task open it; tool
    // results extend it each turn. A revise run frames the task as
    // addressing review feedback on work already in the workspace.
    if bench_open_commands_enabled(request) {
        record(
            kernel,
            request,
            "evidence_appended",
            "execution posture: open command allowlist (benchmark calibration, MDX_FORGE_BENCH_OPEN_COMMANDS=1) - sandbox confinement unchanged",
            0,
            0,
            0,
        );
    }
    let voice_profile = forge_run_voice_profile();
    let system_prompt = forge_run_system_prompt(&voice_profile);
    // Patch-trained families (GPT/Codex) get the edit surface they were
    // RL-trained on; the method text must name the tool they actually have.
    let system_prompt = match client.tool_surface() {
        crate::forge_turn_client::ToolSurface::OpenAiPatch => format!(
            "{system_prompt}\n\nEdit surface: this run's ONLY edit tool is apply_patch (the *** Begin Patch envelope). Where the method above says edit_file or write_file, use apply_patch instead: Update File sections with @@ hunks for existing files, Add File sections for new files. A patch that fails to match applies nothing, so copy context lines exactly from what you read."
        ),
        crate::forge_turn_client::ToolSurface::StrReplace => system_prompt,
    };
    // Plan mode: the run PROPOSES, it does not build. Edits and proof commands
    // are refused; the coder investigates and returns a plan plus a recommended
    // casting for a human to ratify. Execution is a separate, approved run.
    let system_prompt = if request.plan_only {
        format!(
            "{system_prompt}\n\nPLAN MODE: do NOT edit files or run proof commands - those tools are disabled. Investigate what you need (read_file, search, semantic_query, investigate), then propose a plan and STOP. Call update_plan with the concrete steps, then call finish with outcome \"done\" and a summary that also RECOMMENDS THE CASTING: which kind of model or machine should do this (a fast model, a strong frontier model, or - if any part touches sensitive data - a sovereign model the data never leaves), and whether to run it as a single worker or fan it out into a fleet and how wide. A human will ratify your plan and casting before any code is written."
        )
    } else {
        system_prompt
    };
    let task_line = if request.revise_branch.is_some() {
        format!(
            "You are revising a change that already exists in this workspace. A reviewer asked for this: {}",
            request.intent
        )
    } else {
        format!("Task: {}", request.intent)
    };
    let mut messages: Vec<serde_json::Value> = vec![
        serde_json::json!({ "role": "system", "content": system_prompt }),
        serde_json::json!({
            "role": "user",
            "content": format!(
                "{context}{task_line}\n\nAllowed check commands (run exactly as written, in listed order when setup commands precede proof commands):\n{}\n\nThe repository is at the workspace root. Begin by reading the files you need.",
                formatted_allowed_commands(&request.allowed_commands, request)
            )
        }),
    ];

    let mut spent_microcents: u64 = 0;
    let mut turn_index: u32 = 0;
    // Resume: if this run carries prior conversation, continue it. The model
    // gets its own history back - the files it read, the edits it decided on,
    // the checks it ran - and picks up from the last turn rather than
    // re-orienting from scratch. turn_index advances past the turns already
    // taken so the budget accounts for the whole run, not just the resumed leg.
    if request.resume
        && let Some(prior) = crate::forge_transcript_store::load_transcript_by_run(&request.run_id)
        && !prior.is_empty()
    {
        let prior_turns = prior
            .iter()
            .filter(|message| message["role"].as_str() == Some("assistant"))
            .count() as u32;
        messages = prior;
        turn_index = prior_turns;
        let resumed_from = prior_turns.to_string();
        record_with_evidence_fields(
            kernel,
            request,
            "run_resumed",
            &format!("resumed the conversation from turn {prior_turns}"),
            turn_index,
            &[("resumed_from_turn", &resumed_from)],
        );
    }
    // Checkpoint and transcript publication are independently atomic. If the
    // process died between them, one may be a turn newer than the other. Make
    // the restored filesystem state explicit in the resumed conversation and
    // charge its turn against the original budget, so the model inspects the
    // current files instead of blindly replaying a possibly missing tool call.
    if request.resume
        && let Some(checkpoint) = workspace.recovered_checkpoint()
    {
        let checkpoint_turn = (checkpoint.sequence >> 32) as u32;
        turn_index = turn_index.max(checkpoint_turn);
        messages.push(serde_json::json!({
            "role": "user",
            "content": format!(
                "Forge rehydrated a verified workspace checkpoint through turn {checkpoint_turn}. The current uncommitted filesystem state includes these in-scope paths: {}. The transcript and workspace are independently atomic and may differ by one interrupted publication boundary. Inspect current file contents before editing and do not blindly replay an earlier edit.",
                checkpoint.paths.join(", ")
            )
        }));
    }

    let mut finished = false;
    let mut baseline_selected_proof_red_on_arrival = false;
    let mut baseline_repair_expected = false;
    if let Some(preflight) =
        run_baseline_proof_preflight(kernel, request, &workspace_path, repo_root, &mut outcome)
    {
        baseline_selected_proof_red_on_arrival = preflight.selected_proof_red_on_arrival;
        baseline_repair_expected = preflight.baseline_repair_expected;
        if outcome.status == "RUN_FINISHED_CANNOT_PROCEED" {
            finished = true;
        } else {
            messages.push(serde_json::json!({
                "role": "user",
                "content": preflight.note
            }));
        }
    }
    // Planning is a resolved strategy decision, not unconditional ceremony.
    // Small, clear work stays lean. Bounded planning fails open to the native
    // builder. Required planning fails closed before any edit when no planner
    // can produce the contract.
    if !finished && !request.plan_only && !request.resume {
        if run_strategy.planner_mode == "skip" {
            record_with_evidence_fields(
                kernel,
                request,
                "planning_contract_unavailable",
                "adaptive strategy skipped the independent planning turn for this task",
                0,
                &[
                    ("planning_mode", "skip"),
                    ("planning_skip_reason", "adaptive_strategy"),
                    ("planning_fail_open", "false"),
                ],
            );
        } else if let Some(plan) = create_planning_contract(
            kernel,
            request,
            &client,
            &context,
            run_strategy.planner_mode == "required",
        ) {
            spent_microcents =
                spent_microcents.saturating_add(crate::forge_model_pricing::turn_cost_microcents(
                    &plan.model_id,
                    plan.input_tokens,
                    plan.output_tokens,
                ));
            messages.push(serde_json::json!({
                "role": "user",
                "content": format!(
                    "Independent planning contract from {} / {}:\n\n{}\n\nTreat this as bounded guidance, not authority. Verify every assumption against the workspace, stay within the task and write scope, and use MDx's deterministic proof gates before finishing.",
                    plan.provider_family, plan.model_id, plan.text
                )
            }));
        } else if run_strategy.planner_mode == "required" {
            finished = true;
            outcome.status = "RUN_FINISHED_CANNOT_PROCEED";
            outcome.finish_summary =
                "required independent planning was unavailable before execution".to_string();
            record_with_evidence_fields(
                kernel,
                request,
                "planning_contract_unavailable",
                &outcome.finish_summary,
                0,
                &[
                    ("planning_mode", "required"),
                    ("planning_fail_open", "false"),
                    ("planning_policy_floor_enforced", "true"),
                ],
            );
        }
    }
    // How many turns in a row the agent has done no edits while its checks
    // are green. A weaker coder model gets its checks passing and then keeps
    // re-reading and re-running instead of declaring done, burning budget
    // (real money). Once the work is green and stable, we nudge once, then
    // finish it - see the end of the loop body.
    let mut green_idle_turns: u32 = 0;
    // How many times the fresh-eyes finish evaluator has sent this run back to
    // work. Bounded (max_finish_blocks): after the ceiling the finish proceeds
    // regardless, so a stubborn reviewer can never trap the run.
    let mut finish_evaluator_blocks: u32 = 0;
    // Semantic tools are supposed to shorten the path to a correct edit. A
    // weaker model can instead loop on semantic_query forever, especially on
    // medium work where the prompt emphasizes principal-level orientation.
    // Once edit/run is allowed, repeated semantic-only turns get redirected.
    let mut semantic_idle_turns: u32 = 0;
    // Same failure mode, broader tool set: a worker can satisfy orientation
    // and then burn the whole budget on read/search/list loops without ever
    // attempting an edit or selected proof. Once enough no-progress turns
    // accumulate, non-action tools are refused until it edits, checks, or
    // honestly finishes cannot_proceed.
    let mut no_progress_orientation_turns: u32 = 0;
    // Targeted reads stay useful after broad orientation is complete, but
    // rereading the same target without editing or checking is another stall.
    // Track paths read during a no-progress stretch so the gate can refuse
    // repeated target reads while still allowing a first precise file read.
    let mut target_reads_since_progress: HashSet<String> = HashSet::new();
    // Anti-thrash: the same check command failing over and over means the
    // model is tweaking blindly (the budget-exhausted cluster ran ~1 check
    // per turn, 36 checks in 40 turns, never converging). Count consecutive
    // failures of the SAME command; after the threshold, nudge the model to
    // stop tweaking and form a new approach. Resets when the command changes
    // or a check passes.
    let mut last_failed_check: Option<String> = None;
    let mut consecutive_same_check_fails: u32 = 0;
    // How many read-only investigation subagents this run has spawned, capped
    // so delegation cannot consume the whole budget.
    let mut investigations_used: u32 = 0;
    // The last prompt's real input-token count (from provider usage), the
    // signal token-budget compaction fires on. 0 until the first turn reports.
    let mut last_input_tokens: u64 = 0;
    // Files this run has edited, in most-recent order (deduped). After a
    // compaction the most recent are re-read so the model works from real code.
    let mut edited_paths: Vec<String> = Vec::new();
    // Operator controls already applied, by receipt id, so a steer lands
    // exactly once even though it stays on the chain.
    let mut applied_controls =
        initial_applied_control_receipts(kernel, &request.run_id, request.resume);
    let mut passed_setup_commands: HashSet<String> = HashSet::new();
    let mut attempted_non_setup_proof = false;
    // A run may carry its own turn budget for bigger work; default is
    // generous enough for a real multi-file change.
    let max_turns = if request.max_turns > 0 {
        request.max_turns
    } else {
        DEFAULT_MAX_TURNS
    };
    // The enforced ceilings. Every number recorded on the admission
    // receipt is CHECKED here at each turn boundary - recorded-but-not-
    // enforced budgets are exactly the pathology the audit named. 0 on
    // the request takes the server default; the env vars exist so an
    // operator can widen or tighten the defaults deliberately.
    let max_cost_cents: u64 = if request.max_cost_cents > 0 {
        request.max_cost_cents as u64
    } else {
        env_u64("MDX_FORGE_RUN_MAX_COST_CENTS", 2000)
    };
    let max_runtime_ms: u64 = if request.max_runtime_ms > 0 {
        request.max_runtime_ms
    } else {
        env_u64("MDX_FORGE_RUN_MAX_RUNTIME_MS", 45 * 60 * 1000)
    };
    while turn_index < max_turns && !finished {
        turn_index += 1;

        // Budget gates, checked before any further spend. Both end the
        // run cleanly through the same exit ramp as a stop: whatever was
        // built stays on a branch worth reviewing.
        let breach = budget_breach(
            run_started_at.elapsed().as_millis() as u64,
            max_runtime_ms,
            crate::forge_model_pricing::microcents_to_cents_rounded_up(spent_microcents),
            max_cost_cents,
        );
        if let Some(reason) = breach {
            record(
                kernel,
                request,
                "evidence_appended",
                &reason,
                turn_index,
                0,
                0,
            );
            outcome.status = "RUN_BUDGET_EXHAUSTED";
            outcome.finish_summary = reason;
            finished = true;
            break;
        }

        // Between turns - lock held only to read - the loop checks whether
        // the operator steered or stopped it. A stop ends the run cleanly
        // here; a steer is handed to the agent as its next instruction.
        // The lock is dropped before any model work resumes.
        let pending: Vec<mdx_core::PendingForgeRunControl> = {
            if let Ok(kernel) = kernel.read() {
                kernel
                    .forge_run_controls(&request.run_id)
                    .into_iter()
                    .filter(|c| !applied_controls.contains(&c.receipt_id))
                    .collect()
            } else {
                Vec::new()
            }
        };
        let mut stop_now: Option<String> = None;
        // Envelope revocation is a stop with teeth: the human ratified an
        // envelope, the human revoked it, and in-flight work under it
        // halts at the next turn boundary - same rail as an operator
        // stop, same receipt trail.
        if !request.envelope_id.is_empty()
            && let Ok(kernel) = kernel.read()
            && kernel
                .autonomy_envelope_authority_blocker(&request.envelope_id)
                .is_some()
        {
            stop_now = Some(format!(
                "autonomy envelope {} no longer authorizes this run",
                request.envelope_id
            ));
        }
        for control in pending {
            applied_controls.insert(control.receipt_id.clone());
            match control.control.as_str() {
                "stop" => stop_now = Some(control.note.clone()),
                "steer" if !control.note.trim().is_empty() => {
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": format!("Operator guidance, mid-run - take it into account from here: {}", control.note.trim())
                    }));
                    record(
                        kernel,
                        request,
                        "evidence_appended",
                        "operator steered the run",
                        turn_index,
                        0,
                        0,
                    );
                }
                _ => {}
            }
        }
        if let Some(note) = stop_now {
            let reason = if note.trim().is_empty() {
                "stopped by the operator".to_string()
            } else {
                format!("stopped by the operator: {}", note.trim())
            };
            // One run_finished per run is recorded below the loop; here we
            // just mark why it ended and stop. A stop after edits still
            // leaves a branch worth reviewing - the commit step handles it.
            record(
                kernel,
                request,
                "evidence_appended",
                &reason,
                turn_index,
                0,
                0,
            );
            outcome.status = "RUN_STOPPED";
            outcome.finish_summary = reason;
            finished = true;
            break;
        }

        // The model call: NO kernel lock held.
        let stream_model = crate::forge_run_stream::RunStreamModel::from_slot(
            &request.builder_slot,
            &request.work_complexity_tier,
        );
        crate::forge_run_stream::publish_worker_thought(
            &request.run_id,
            "planning",
            turn_index,
            &voice_turn_placeholder(&voice_profile, turn_index),
            &stream_model,
        );
        let model_started_at = Instant::now();
        let turn = match model_turn_with_watchdog(&client, &messages, request, turn_index) {
            Ok(turn) => turn,
            Err(error) => {
                if let Ok(mut guard) = kernel.write()
                    && let Err(persist_error) =
                        crate::model_fabric_service::persist_pending_model_runtime_evidence(
                            &mut guard,
                            &request.tenant_id,
                            &request.actor_id,
                        )
                {
                    eprintln!(
                        "forge_loop_runner: model runtime evidence receipt failed: {persist_error}"
                    );
                }
                record_timed(
                    kernel,
                    request,
                    TimedRunEvent {
                        event: "run_finished",
                        detail: &format!("model call failed: {error}"),
                        turn: turn_index,
                        input_tokens: 0,
                        output_tokens: 0,
                        duration_ms: run_started_at.elapsed().as_millis() as u64,
                    },
                );
                outcome.status = "RUN_ERROR";
                outcome.finish_summary = format!("model call failed: {error}");
                outcome.turns_used = turn_index;
                return outcome;
            }
        };
        if let Ok(mut guard) = kernel.write()
            && let Err(error) = client.record_route_outcome(
                &mut guard,
                &request.tenant_id,
                &request.actor_id,
                &turn,
                model_started_at
                    .elapsed()
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64,
                "forge_builder_runtime",
            )
        {
            eprintln!("forge_loop_runner: model outcome receipt failed: {error}");
        }
        if !turn.text.trim().is_empty() {
            crate::forge_run_stream::publish_worker_thought(
                &request.run_id,
                "planning",
                turn_index,
                turn.text.trim(),
                &stream_model,
            );
            let agent_turn_prose = bounded_human_run_text(&turn.text, 700);
            if !agent_turn_prose.is_empty() {
                record_with_evidence_fields(
                    kernel,
                    request,
                    "agent_turn_prose",
                    &agent_turn_prose,
                    turn_index,
                    &[
                        ("agent_turn_prose", agent_turn_prose.as_str()),
                        ("agent_turn_prose_source", "model_turn_text"),
                        ("forge_run_voice_profile_id", voice_profile.id),
                        ("forge_run_voice_profile_source", "server_profile"),
                        ("agent_turn_prose_grants_execution_authority", "false"),
                    ],
                );
            }
        }
        let model_duration_ms = model_started_at.elapsed().as_millis() as u64;
        // The real size of the prompt we just sent - what token-budget
        // compaction watches. Providers that report no usage leave it 0 and
        // the message-count fallback still applies.
        if turn.input_tokens > 0 {
            last_input_tokens = turn.input_tokens;
        }
        record_timed(
            kernel,
            request,
            TimedRunEvent {
                event: "model_called",
                detail: &format!(
                    "model={} finish_reason={} tool_calls={}",
                    turn.model_id,
                    turn.finish_reason,
                    turn.tool_calls.len()
                ),
                turn: turn_index,
                input_tokens: turn.input_tokens,
                output_tokens: turn.output_tokens,
                duration_ms: model_duration_ms,
            },
        );
        messages.push(turn.assistant_message.clone());
        spent_microcents += crate::forge_model_pricing::turn_cost_microcents(
            &turn.model_id,
            turn.input_tokens,
            turn.output_tokens,
        );

        if turn.tool_calls.is_empty() {
            // No tools, no finish: nudge once, then the budget ends it.
            messages.push(serde_json::json!({
                "role": "user",
                "content": "Call a tool to continue, or call finish when the checks are green."
            }));
            record(
                kernel,
                request,
                "turn_executed",
                "no tool calls",
                turn_index,
                0,
                0,
            );
            continue;
        }

        let files_before_turn = outcome.files_changed;
        let mut edited_this_turn = false;
        let mut ran_check_this_turn = false;
        // Parallel read-only fan-out: when a turn asks for several
        // investigations at once, run them concurrently (bounded by the
        // remaining delegation budget) instead of one after another. They are
        // read-only and independent, so this only saves wall-clock; findings
        // are matched back by call_id and consumed in call order below.
        let mut precomputed_investigations: HashMap<String, crate::forge_subagent::SubagentReport> =
            HashMap::new();
        {
            let pending: Vec<(String, String)> = turn
                .tool_calls
                .iter()
                .filter(|call| call.name == "investigate")
                .filter_map(|call| {
                    let question = call.arguments["question"].as_str().unwrap_or("").trim();
                    (!question.is_empty()).then(|| (call.call_id.clone(), question.to_string()))
                })
                .collect();
            let remaining = crate::forge_subagent::investigation_max_calls()
                .saturating_sub(investigations_used) as usize;
            let run_now = pending.len().min(remaining);
            if run_now >= 2
                && let Some(investigator_client) = investigator_client.as_ref()
            {
                let started = Instant::now();
                let results = crate::forge_subagent::run_investigations_parallel(
                    investigator_client,
                    &pending[..run_now],
                    &workspace_path,
                );
                for (call_id, report) in results {
                    account_delegated_model_usage(
                        kernel,
                        request,
                        turn_index,
                        "parallel_investigate",
                        &report,
                        &mut spent_microcents,
                    );
                    precomputed_investigations.insert(call_id, report);
                }
                investigations_used += run_now as u32;
                record_timed(
                    kernel,
                    request,
                    TimedRunEvent {
                        event: "tool_executed",
                        detail: &format!("parallel investigate x{run_now}"),
                        turn: turn_index,
                        input_tokens: 0,
                        output_tokens: 0,
                        duration_ms: started.elapsed().as_millis() as u64,
                    },
                );
            }
        }
        for call in &turn.tool_calls {
            if no_progress_orientation_turns >= 3
                && repeated_stalled_target_read(call, &target_reads_since_progress)
                && orientation_gate.ready_to_edit_or_check()
                && outcome.files_changed == 0
            {
                let path = target_read_key(call).unwrap_or_else(|| "the same file".to_string());
                messages.push(tool_result_message(
                    &call.call_id,
                    &format!(
                        "error: orientation is complete and `{path}` was already read during this stalled stretch. Do not reread it until after an edit or selected proof attempt. Use edit_file for the scoped change, run_command for one selected proof command, or finish cannot_proceed with concrete evidence."
                    ),
                ));
                record_timed(
                    kernel,
                    request,
                    TimedRunEvent {
                        event: "tool_executed",
                        detail: &format!("progress_gate refused repeated read_file {path}"),
                        turn: turn_index,
                        input_tokens: 0,
                        output_tokens: 0,
                        duration_ms: 0,
                    },
                );
                continue;
            }
            if no_progress_orientation_turns >= 3
                && stalled_orientation_call(call, &request.allowed_commands)
                && orientation_gate.ready_to_edit_or_check()
                && outcome.files_changed == 0
            {
                messages.push(tool_result_message(
                    &call.call_id,
                    "error: orientation is complete and this run is stuck in broad search/list/semantic/investigate loops, including read-only shell commands. If you already know the target file, read that exact file or make the edit. Your next useful tool must be read_file for a specific target, edit_file for the scoped code, run_command for one selected proof command, or finish with outcome cannot_proceed if you have concrete evidence that no safe edit is possible.",
                ));
                record_timed(
                    kernel,
                    request,
                    TimedRunEvent {
                        event: "tool_executed",
                        detail: &format!(
                            "progress_gate refused repeated {}",
                            tool_call_summary(call)
                        ),
                        turn: turn_index,
                        input_tokens: 0,
                        output_tokens: 0,
                        duration_ms: 0,
                    },
                );
                continue;
            }
            if semantic_idle_turns >= 2
                && call.name == "semantic_query"
                && orientation_gate.ready_to_edit_or_check()
                && outcome.files_changed == 0
                && !outcome.last_check_passed
            {
                messages.push(tool_result_message(
                    &call.call_id,
                    "error: enough semantic orientation has landed for this run. Stop calling semantic_query for now. Your next useful tool should be edit_file for the target code or run_command for the selected check.",
                ));
                record_timed(
                    kernel,
                    request,
                    TimedRunEvent {
                        event: "tool_executed",
                        detail: "semantic_progress_gate refused repeated semantic_query",
                        turn: turn_index,
                        input_tokens: 0,
                        output_tokens: 0,
                        duration_ms: 0,
                    },
                );
                continue;
            }
            if let Some(reason) = orientation_gate.refusal_for(&call.name) {
                messages.push(tool_result_message(
                    &call.call_id,
                    &format!("error: {reason}"),
                ));
                record_timed(
                    kernel,
                    request,
                    TimedRunEvent {
                        event: "tool_executed",
                        detail: &format!("principal_orientation_gate refused {}", call.name),
                        turn: turn_index,
                        input_tokens: 0,
                        output_tokens: 0,
                        duration_ms: 0,
                    },
                );
                continue;
            }
            // The plan tool: the coder maintains a legible step list for
            // multi-step work. It tracks HOW the task is being carried out and
            // grants no authority - the brief and mission stay the governed
            // truth the plan serves. Recorded so operators can watch progress
            // and it survives resume; the coder gets a compact ack.
            if call.name == "update_plan" {
                let rendered = render_plan(&call.arguments["steps"]);
                record(kernel, request, "plan_updated", &rendered, turn_index, 0, 0);
                messages.push(tool_result_message(
                    &call.call_id,
                    "plan recorded. Keep it current: mark a step completed when it is done and move the next to in_progress. Now do the in_progress step.",
                ));
                continue;
            }
            // Delegate a read-only investigation to a fresh subagent (the
            // context firewall): the exploration runs in its own clean context
            // and only the distilled finding returns here, so the parent's
            // window stays on the change. The subagent cannot edit - the parent
            // keeps sole write authority. Bounded per run so a coder cannot
            // spend its whole budget delegating.
            if call.name == "investigate" {
                // A finding computed in the parallel fan-out above: use it.
                if let Some(report) = precomputed_investigations.remove(&call.call_id) {
                    messages.push(tool_result_message(&call.call_id, &report.text));
                    continue;
                }
                let question = call.arguments["question"].as_str().unwrap_or("").trim();
                let report = if question.is_empty() {
                    crate::forge_subagent::SubagentReport {
                        text: "error: investigate needs a question.".to_string(),
                        model_usage: Vec::new(),
                    }
                } else if investigations_used >= crate::forge_subagent::investigation_max_calls() {
                    crate::forge_subagent::SubagentReport {
                        text: "error: this run has used its investigation budget; investigate further yourself with read_file and search, or make the change.".to_string(),
                        model_usage: Vec::new(),
                    }
                } else if let Some(investigator_client) = investigator_client.as_ref() {
                    investigations_used += 1;
                    let started = Instant::now();
                    let report = crate::forge_subagent::run_investigation(
                        investigator_client,
                        question,
                        &workspace_path,
                    );
                    account_delegated_model_usage(
                        kernel,
                        request,
                        turn_index,
                        "investigate",
                        &report,
                        &mut spent_microcents,
                    );
                    record_timed(
                        kernel,
                        request,
                        TimedRunEvent {
                            event: "tool_executed",
                            detail: &format!(
                                "investigate #{investigations_used}: {}",
                                question.chars().take(80).collect::<String>()
                            ),
                            turn: turn_index,
                            input_tokens: 0,
                            output_tokens: 0,
                            duration_ms: started.elapsed().as_millis() as u64,
                        },
                    );
                    report
                } else {
                    crate::forge_subagent::SubagentReport {
                        text: "note: the governed investigator route is unavailable, so investigate is disabled for this run; continue with the read-only tools in this session.".to_string(),
                        model_usage: Vec::new(),
                    }
                };
                messages.push(tool_result_message(&call.call_id, &report.text));
                continue;
            }
            // Independent verification (the Oracle pattern): a fresh reviewer,
            // resolved to the reviewer role so a DIFFERENT/stronger model can
            // review than the one that wrote the change, sees the task and the
            // uncommitted diff and returns a verdict. Read-only - the coder
            // acts on the verdict, keeping sole write authority. Shares the
            // investigation budget so delegation stays bounded.
            if call.name == "verify" {
                let focus = call.arguments["focus"].as_str().unwrap_or("");
                let report = if investigations_used
                    >= crate::forge_subagent::investigation_max_calls()
                {
                    crate::forge_subagent::SubagentReport {
                        text: "error: this run has used its delegation budget; review the change yourself and finish.".to_string(),
                        model_usage: Vec::new(),
                    }
                } else if let Some(diff) =
                    crate::forge_finish_evaluator::workspace_diff(&workspace_path)
                {
                    let reviewer = {
                        let guard = kernel.write().ok();
                        guard.and_then(|mut k| {
                            match TurnClient::routed_for_role(
                                &mut k,
                                &request.tenant_id,
                                &request.actor_id,
                                &request.run_id,
                                "reviewer",
                                &request.reasoning_effort,
                            ) {
                                Ok(Some(client)) => Some(client),
                                Ok(None) => TurnClient::for_role_with_reasoning_effort(
                                    &k,
                                    &request.tenant_id,
                                    "reviewer",
                                    &request.reasoning_effort,
                                )
                                .or_else(|| {
                                    TurnClient::for_role_with_reasoning_effort(
                                        &k,
                                        &request.tenant_id,
                                        "planner",
                                        &request.reasoning_effort,
                                    )
                                }),
                                Err(_) => None,
                            }
                        })
                    };
                    match reviewer {
                        Some(reviewer) => {
                            investigations_used += 1;
                            let started = Instant::now();
                            let report = crate::forge_subagent::run_verification(
                                &reviewer,
                                &request.intent,
                                &diff,
                                focus,
                                &workspace_path,
                            );
                            account_delegated_model_usage(
                                kernel,
                                request,
                                turn_index,
                                "verify",
                                &report,
                                &mut spent_microcents,
                            );
                            record_timed(
                                kernel,
                                request,
                                TimedRunEvent {
                                    event: "tool_executed",
                                    detail: &format!(
                                        "verify #{investigations_used} by {}",
                                        reviewer.model_id()
                                    ),
                                    turn: turn_index,
                                    input_tokens: 0,
                                    output_tokens: 0,
                                    duration_ms: started.elapsed().as_millis() as u64,
                                },
                            );
                            report
                        }
                        None => {
                            crate::forge_subagent::SubagentReport {
                                text: "note: no independent reviewer model is configured, so verify is unavailable; review the change yourself.".to_string(),
                                model_usage: Vec::new(),
                            }
                        }
                    }
                } else {
                    crate::forge_subagent::SubagentReport {
                        text:
                            "note: there is no change to verify yet - make your edit first, then verify."
                                .to_string(),
                        model_usage: Vec::new(),
                    }
                };
                messages.push(tool_result_message(&call.call_id, &report.text));
                continue;
            }
            if call.name == "finish" {
                let finish_outcome = call.arguments["outcome"].as_str().unwrap_or("done");
                // Plan mode: finishing means the plan is ready to propose. Record
                // it and end as PLAN_PROPOSED - no edits happened, no branch is
                // cut; a human ratifies before an execution run.
                if request.plan_only {
                    let summary = call.arguments["summary"].as_str().unwrap_or("");
                    record(kernel, request, "plan_proposed", summary, turn_index, 0, 0);
                    finished = true;
                    outcome.status = "RUN_PLAN_PROPOSED";
                    outcome.finish_summary = summary.to_string();
                    messages.push(tool_result_message(&call.call_id, "plan proposed"));
                    break;
                }
                if premature_cannot_proceed_refusal(
                    request,
                    &outcome,
                    &orientation_gate,
                    finish_outcome,
                    attempted_non_setup_proof,
                ) {
                    messages.push(tool_result_message(
                        &call.call_id,
                        "error: cannot_proceed refused before a concrete attempt. You have scoped files and a selected proof. Read or edit the scoped file, or run the selected proof to capture the real blocker. Only call cannot_proceed after a concrete edit/proof attempt shows the task is impossible.",
                    ));
                    record_timed(
                        kernel,
                        request,
                        TimedRunEvent {
                            event: "tool_executed",
                            detail: "finish refused premature cannot_proceed",
                            turn: turn_index,
                            input_tokens: 0,
                            output_tokens: 0,
                            duration_ms: 0,
                        },
                    );
                    continue;
                }
                if let Some(reason) = finish_proof_refusal(request, &outcome, finish_outcome) {
                    messages.push(tool_result_message(
                        &call.call_id,
                        &format!("error: {reason}"),
                    ));
                    record_timed(
                        kernel,
                        request,
                        TimedRunEvent {
                            event: "tool_executed",
                            detail: "finish refused missing post-change proof",
                            turn: turn_index,
                            input_tokens: 0,
                            output_tokens: 0,
                            duration_ms: 0,
                        },
                    );
                    continue;
                }
                // Adaptive strategy can select the independent close gate for
                // ordinary runs. Legacy callers retain the historical bench-only
                // explicit-finish review. The gate reads the task and workspace
                // change, stays bounded, and shares the run cost ceiling.
                let adaptive_review_selected = run_strategy.adaptive_enabled
                    && run_strategy.review_mode != "deterministic_only";
                if finish_outcome == "done"
                    && run_strategy.adaptive_enabled
                    && run_strategy.review_mode == "deterministic_only"
                {
                    record_with_evidence_fields(
                        kernel,
                        request,
                        "finish_evaluated",
                        "deterministic proof completed the adaptive review policy without a model review",
                        turn_index,
                        &[
                            ("finish_evaluator_verdict", "skipped_deterministic_only"),
                            ("review_mode", "deterministic_only"),
                        ],
                    );
                }
                if finish_outcome == "done"
                    && (adaptive_review_selected
                        || (!run_strategy.adaptive_enabled && bench_open_commands_enabled(request)))
                {
                    match evaluate_finish_before_close(
                        kernel,
                        request,
                        &workspace_path,
                        &mut finish_evaluator_blocks,
                        turn_index,
                        FinishGatePolicy {
                            required: run_strategy.review_mode == "required_independent",
                            max_cost_cents,
                        },
                        &mut spent_microcents,
                    ) {
                        FinishGateResult::Revise(concern) => {
                            messages.push(tool_result_message(
                                &call.call_id,
                                &format!(
                                    "Not done yet. A fresh-eyes reviewer checked your work against the task and found a gap: {concern} Address it concretely, verify it, then call finish again."
                                ),
                            ));
                            record_timed(
                                kernel,
                                request,
                                TimedRunEvent {
                                    event: "tool_executed",
                                    detail: "finish held: fresh-eyes reviewer found an unmet task gap",
                                    turn: turn_index,
                                    input_tokens: 0,
                                    output_tokens: 0,
                                    duration_ms: 0,
                                },
                            );
                            continue;
                        }
                        FinishGateResult::RequiredUnavailable(reason) => {
                            finished = true;
                            outcome.status = "RUN_FINISHED_CANNOT_PROCEED";
                            outcome.finish_summary = reason;
                            messages.push(tool_result_message(
                                &call.call_id,
                                "required independent review was unavailable; the run closed without claiming done",
                            ));
                            break;
                        }
                        FinishGateResult::BudgetExhausted(reason) => {
                            finished = true;
                            outcome.status = "RUN_BUDGET_EXHAUSTED";
                            outcome.finish_summary = reason;
                            messages.push(tool_result_message(
                                &call.call_id,
                                "the independent review exhausted the run budget; the run closed without claiming done",
                            ));
                            break;
                        }
                        FinishGateResult::Proceed => {}
                    }
                }
                finished = true;
                apply_finish_outcome(
                    &mut outcome,
                    finish_outcome,
                    call.arguments["summary"].as_str().unwrap_or(""),
                );
                record(
                    kernel,
                    request,
                    "evidence_appended",
                    &format!("finish={finish_outcome}"),
                    turn_index,
                    0,
                    0,
                );
                messages.push(tool_result_message(&call.call_id, "acknowledged"));
                break;
            }
            if call.name == "run_command" {
                let command = call.arguments["command"].as_str().unwrap_or("");
                if let Some(reason) = (!bench_open_commands_enabled(request))
                    .then(|| {
                        setup_order_refusal(
                            command,
                            &request.allowed_commands,
                            &passed_setup_commands,
                        )
                    })
                    .flatten()
                {
                    messages.push(tool_result_message(
                        &call.call_id,
                        &format!("error: {reason}"),
                    ));
                    record_timed(
                        kernel,
                        request,
                        TimedRunEvent {
                            event: "tool_executed",
                            detail: &format!("setup_order_gate refused {command}"),
                            turn: turn_index,
                            input_tokens: 0,
                            output_tokens: 0,
                            duration_ms: 0,
                        },
                    );
                    continue;
                }
            }
            let files_before_call = outcome.files_changed;
            crate::forge_run_stream::publish_tool_call(
                &request.run_id,
                turn_index,
                &call.name,
                &tool_call_summary(call),
                &stream_model,
            );
            let tool_started_at = Instant::now();
            if call.name == "run_command" {
                let command = call.arguments["command"].as_str().unwrap_or("");
                if !command.trim().is_empty() {
                    record(
                        kernel,
                        request,
                        "check_started",
                        &format!("run_command_started {command}"),
                        turn_index,
                        0,
                        0,
                    );
                }
            }
            let (result_text, event, detail, check_passed) =
                dispatch_tool(call, &workspace_path, repo_root, request, &mut outcome);
            if matches!(
                call.name.as_str(),
                "edit_file" | "write_file" | "apply_patch"
            ) {
                let landed = outcome.files_changed > files_before_call;
                edited_this_turn |= landed;
                // Record which files this edit touched (most-recent order), so a
                // compaction can re-read the current ones. Dedup: an already
                // tracked path moves to the most-recent end.
                if landed {
                    for path in edited_paths_of(call) {
                        edited_paths.retain(|existing| existing != &path);
                        edited_paths.push(path);
                    }
                    let sequence = ((turn_index as u64) << 32) | outcome.files_changed as u64;
                    match workspace.checkpoint(sequence) {
                        Ok(Some(checkpoint)) => record(
                            kernel,
                            request,
                            "workspace_checkpointed",
                            &format!(
                                "checkpoint={} sequence={} paths={}",
                                &checkpoint.checkpoint_sha
                                    [..checkpoint.checkpoint_sha.len().min(12)],
                                checkpoint.sequence,
                                checkpoint.paths.join(",")
                            ),
                            turn_index,
                            0,
                            0,
                        ),
                        Ok(None) => record(
                            kernel,
                            request,
                            "workspace_checkpoint_cleared",
                            "the in-scope diff is empty after the latest edit",
                            turn_index,
                            0,
                            0,
                        ),
                        Err(error) => record(
                            kernel,
                            request,
                            "workspace_checkpoint_failed",
                            &format!("could not durably checkpoint the latest edit: {error}"),
                            turn_index,
                            0,
                            0,
                        ),
                    }
                }
            }
            if call.name == "run_command" {
                let command = call.arguments["command"].as_str().unwrap_or("");
                if selected_proof_command(command, &request.allowed_commands) {
                    ran_check_this_turn = true;
                }
            }
            if call.name == "read_file"
                && orientation_gate.ready_to_edit_or_check()
                && outcome.files_changed == 0
                && !edited_this_turn
                && !ran_check_this_turn
                && let Some(path) = target_read_key(call)
            {
                target_reads_since_progress.insert(path);
            }
            orientation_gate.observe_tool(
                &call.name,
                &call.arguments,
                outcome.files_changed > files_before_call,
                &result_text,
            );
            let tool_duration_ms = tool_started_at.elapsed().as_millis() as u64;
            messages.push(tool_result_message(&call.call_id, &result_text));
            if call.name == "run_command" {
                outcome.check_runs += 1;
                outcome.check_duration_ms =
                    outcome.check_duration_ms.saturating_add(tool_duration_ms);
            }
            record_timed(
                kernel,
                request,
                TimedRunEvent {
                    event,
                    detail: &detail,
                    turn: turn_index,
                    input_tokens: 0,
                    output_tokens: 0,
                    duration_ms: tool_duration_ms,
                },
            );
            // A green check only describes the code that was checked. Track
            // it precisely, respecting order within the turn: an edit
            // invalidates a prior green (a re-check is needed), a pass sets
            // it, a fail clears it.
            if outcome.files_changed > files_before_call {
                outcome.last_check_passed = false;
            }
            if check_passed {
                if call.name == "run_command" {
                    let command = call.arguments["command"].as_str().unwrap_or("");
                    if crate::forge_repo_profile::is_setup_command(command) {
                        passed_setup_commands.insert(command.to_string());
                    } else {
                        attempted_non_setup_proof = true;
                        outcome.last_check_passed = true;
                        // Real progress: the check went green. Clear the thrash counter.
                        last_failed_check = None;
                        consecutive_same_check_fails = 0;
                    }
                }
            } else if event == "check_failed" {
                attempted_non_setup_proof = true;
                outcome.last_check_passed = false;
                // Anti-thrash bookkeeping: same failing command in a row?
                let command = call.arguments["command"].as_str().unwrap_or("").to_string();
                if last_failed_check.as_deref() == Some(command.as_str()) {
                    consecutive_same_check_fails += 1;
                } else {
                    last_failed_check = Some(command);
                    consecutive_same_check_fails = 1;
                }
            }
        }
        record(
            kernel,
            request,
            "turn_executed",
            &format!("tools={}", turn.tool_calls.len()),
            turn_index,
            0,
            0,
        );
        // Anti-thrash step-back: the same check has failed several times in a
        // row without going green. That is the run-check-fail-tweak loop the
        // budget-exhausted cluster died in. Once, at the threshold, tell the
        // model to stop tweaking and reconsider the whole approach; reset so
        // it fires again only after another run of failures, never every turn.
        if !finished && consecutive_same_check_fails >= thrash_step_back_threshold() {
            let command = last_failed_check.clone().unwrap_or_default();
            messages.push(serde_json::json!({
                "role": "user",
                "content": format!(
                    "You have run `{command}` {consecutive_same_check_fails} times and it is still failing. Small tweaks are not converging. Stop editing for a moment: read the failing output in full, state the ROOT cause in one line, and choose a different approach than the one you have been repeating. If the current path is wrong, revert it before trying the new one."
                )
            }));
            record(
                kernel,
                request,
                "evidence_appended",
                &format!(
                    "anti-thrash step-back nudge after {consecutive_same_check_fails} same-check fails"
                ),
                turn_index,
                0,
                0,
            );
            consecutive_same_check_fails = 0;
            last_failed_check = None;
        }
        if !finished
            && outcome.files_changed == 0
            && !edited_this_turn
            && !ran_check_this_turn
            && orientation_gate.ready_to_edit_or_check()
        {
            semantic_idle_turns += 1;
            no_progress_orientation_turns += 1;
            if semantic_idle_turns == 1 {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": "You have enough orientation to act. Stop expanding the map. Make the smallest relevant edit now, or run the selected check if you need the failing signal first."
                }));
                record(
                    kernel,
                    request,
                    "evidence_appended",
                    "semantic orientation complete; nudged worker toward edit or check",
                    turn_index,
                    0,
                    0,
                );
            } else if no_progress_orientation_turns == 3 {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": "You are now repeating orientation without progress. Do not call search, list_dir, semantic_query, investigate, or read-only shell commands again until after an edit or selected proof attempt. If you know the target, read that exact file, then use edit_file, run_command for one selected proof command, or finish cannot_proceed with concrete evidence."
                }));
                record(
                    kernel,
                    request,
                    "evidence_appended",
                    "progress gate armed after repeated read/search/shell turns",
                    turn_index,
                    0,
                    0,
                );
            } else if no_progress_orientation_turns > 3 {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": "Last turn made no progress because orientation tools were refused. Pick exactly one now: edit the scoped file, run one selected proof command, or finish cannot_proceed with the blocker. Do not ask for more context first."
                }));
                record(
                    kernel,
                    request,
                    "evidence_appended",
                    "progress gate repeated hard nudge after another no-progress turn",
                    turn_index,
                    0,
                    0,
                );
            }
        } else if edited_this_turn || ran_check_this_turn || outcome.files_changed > 0 {
            semantic_idle_turns = 0;
            no_progress_orientation_turns = 0;
            target_reads_since_progress.clear();
        }

        // The convergence guard: if the checks are green AND nothing was
        // edited this turn (so the green still describes exactly this code),
        // the work is provably done and stable. Nudge once to finish; if the
        // agent still will not declare done on the next stable turn, finish
        // it. This only ever fires on a run with real, verified work, and the
        // branch is reviewed before anything ships - so finishing for a
        // spinning agent costs nothing and stops it burning budget.
        if !finished
            && outcome.last_check_passed
            && outcome.files_changed == files_before_turn
            && outcome.files_changed == 0
            && !bench_open_commands_enabled(request)
        {
            green_idle_turns += 1;
            if green_idle_turns == 1 {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": "The checks passed on unchanged code, but this task asked for a code change. That is baseline health, not task completion. Edit the code and add or update proof that covers the requested behavior, or call finish with outcome cannot_proceed and explain why no change is possible."
                }));
                record(
                    kernel,
                    request,
                    "evidence_appended",
                    "checks passed on unchanged code; finish held because no task diff exists",
                    turn_index,
                    0,
                    0,
                );
            } else {
                finished = true;
                outcome.status = "RUN_FINISHED_NO_CHANGE";
                outcome.finish_summary =
                    "checks passed on unchanged code; no task diff was produced".to_string();
                record(
                    kernel,
                    request,
                    "evidence_appended",
                    "auto-closed as no-change after repeated unchanged green checks",
                    turn_index,
                    0,
                    0,
                );
            }
        } else if !finished
            && outcome.last_check_passed
            && outcome.files_changed == files_before_turn
            && !bench_open_commands_enabled(request)
        {
            if let Some(reason) = orientation_gate.refusal_for("finish") {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": format!("{reason} Your checks are green, but this evidence is required before Forge can close the run.")
                }));
                record(
                    kernel,
                    request,
                    "evidence_appended",
                    "checks green and stable; finish held for semantic related-test orientation",
                    turn_index,
                    0,
                    0,
                );
                continue;
            }
            green_idle_turns += 1;
            if green_idle_turns == 1 {
                messages.push(serde_json::json!({
                    "role": "user",
                    "content": "Your checks are green and you made no further edits, so the code is verified and stable. If the objective is complete, call finish with outcome done NOW. Do not re-read files or re-run checks unless something is genuinely unresolved."
                }));
                record(
                    kernel,
                    request,
                    "evidence_appended",
                    "checks green and stable; nudged the agent to finish",
                    turn_index,
                    0,
                    0,
                );
            } else {
                let review = if run_strategy.adaptive_enabled
                    && run_strategy.review_mode == "deterministic_only"
                {
                    record_with_evidence_fields(
                        kernel,
                        request,
                        "finish_evaluated",
                        "deterministic proof completed the adaptive review policy without a model review",
                        turn_index,
                        &[
                            ("finish_evaluator_verdict", "skipped_deterministic_only"),
                            ("review_mode", "deterministic_only"),
                        ],
                    );
                    FinishGateResult::Proceed
                } else {
                    evaluate_finish_before_close(
                        kernel,
                        request,
                        &workspace_path,
                        &mut finish_evaluator_blocks,
                        turn_index,
                        FinishGatePolicy {
                            required: run_strategy.review_mode == "required_independent",
                            max_cost_cents,
                        },
                        &mut spent_microcents,
                    )
                };
                match review {
                    FinishGateResult::Revise(concern) => {
                        messages.push(serde_json::json!({
                            "role": "user",
                            "content": format!(
                                "A fresh-eyes reviewer looked at your change against the task and flagged one thing before this run can close: {concern}\n\nAddress it if it is real, or run the selected check again to confirm the change already covers it, then finish."
                            )
                        }));
                        green_idle_turns = 0;
                    }
                    FinishGateResult::RequiredUnavailable(reason) => {
                        finished = true;
                        outcome.status = "RUN_FINISHED_CANNOT_PROCEED";
                        outcome.finish_summary = reason;
                    }
                    FinishGateResult::BudgetExhausted(reason) => {
                        finished = true;
                        outcome.status = "RUN_BUDGET_EXHAUSTED";
                        outcome.finish_summary = reason;
                    }
                    FinishGateResult::Proceed => {
                        finished = true;
                        outcome.status = "RUN_FINISHED_DONE";
                        outcome.finish_summary =
                            "auto-finished: checks stayed green and stable with no further edits"
                                .to_string();
                        record(
                            kernel,
                            request,
                            "evidence_appended",
                            "auto-finished after green, stable checks (agent kept spinning)",
                            turn_index,
                            0,
                            0,
                        );
                    }
                }
            }
        } else if !finished {
            green_idle_turns = 0;
        }
        if !finished {
            let breach = budget_breach(
                run_started_at.elapsed().as_millis() as u64,
                max_runtime_ms,
                crate::forge_model_pricing::microcents_to_cents_rounded_up(spent_microcents),
                max_cost_cents,
            );
            if let Some(reason) = breach {
                record(
                    kernel,
                    request,
                    "evidence_appended",
                    &reason,
                    turn_index,
                    0,
                    0,
                );
                outcome.status = "RUN_BUDGET_EXHAUSTED";
                outcome.finish_summary = reason;
                finished = true;
            }
        }
        // End of the turn: the conversation just grew by an assistant turn
        // and its tool results. Compact it first if it has grown large (the
        // authority region and recent turns are preserved; the middle
        // collapses into one witnessed summary), then persist the whole thing
        // so a crash or restart resumes from here instead of redoing turn 0.
        maybe_compact_run_transcript(
            kernel,
            request,
            &mut messages,
            turn_index,
            last_input_tokens,
            client.context_window(),
            client.max_output(),
            &workspace_path,
            &edited_paths,
        );
        persist_run_transcript(kernel, request, &messages, turn_index);
    }

    outcome.turns_used = turn_index;
    if !finished {
        outcome.status = "RUN_BUDGET_EXHAUSTED";
        outcome.finish_summary = format!(
            "the turn budget ran out before the work declared itself done; selected checks consumed {} ms across {} run(s)",
            outcome.check_duration_ms, outcome.check_runs
        );
    }
    // The exit ramp: a run that changed something becomes a branch - a
    // clean finish, or ANY terminal state that left partial work worth
    // reviewing (stopped, out of budget, cannot-proceed). Three live
    // fleets lost correct work to the old done-or-stopped rule before
    // this widened. A revise run commits ONTO the branch it revised; a
    // fresh run cuts a new one; an empty run produces none either way.
    // Only a clean finish is shippable: the ship gate still requires
    // RUN_FINISHED_DONE - everything else is review-only evidence.
    if !request.plan_only
        && (outcome.files_changed > 0
            || outcome.status == "RUN_FINISHED_DONE"
            || bench_open_commands_enabled(request))
    {
        let branch = match request.revise_branch.as_deref() {
            Some(existing) => existing.to_string(),
            None => fresh_review_branch_name(&request.run_id, workspace.id()),
        };
        let message = format!(
            "[forge] {}\n\nForge harness run {} for work item {}.\n{}",
            first_line(&request.intent),
            request.run_id,
            request.work_item_id,
            outcome.finish_summary
        );
        let committed = match request.revise_branch.as_deref() {
            Some(_) => workspace.commit_onto_branch(&branch, &message),
            None => workspace.commit_to_branch(&branch, &message),
        };
        if let Some(sha) = committed {
            outcome.branch = Some(branch.clone());
            outcome.commit_sha = Some(sha.clone());
            if let Err(error) = workspace.clear_checkpoint() {
                record(
                    kernel,
                    request,
                    "workspace_checkpoint_cleanup_failed",
                    &format!("review branch is durable but checkpoint cleanup failed: {error}"),
                    turn_index,
                    0,
                    0,
                );
            }
            // Reconcile the per-edit tally to the distinct files actually
            // committed, so the run-list card matches the Review page and git
            // (the tally counts edit operations, not committed files).
            let committed_paths = workspace.committed_file_paths(&sha).unwrap_or_default();
            if !committed_paths.is_empty() {
                outcome.files_changed = committed_paths.len() as u32;
            } else if let Some(count) = workspace.committed_file_count(&sha) {
                outcome.files_changed = count;
            }
            record(
                kernel,
                request,
                "evidence_appended",
                &format!(
                    "branch={branch} sha={} paths={}",
                    &sha[..sha.len().min(12)],
                    committed_paths.join(",")
                ),
                turn_index,
                0,
                0,
            );
        } else if let Some(demoted) = commit_failure_status(outcome.files_changed, false) {
            // The run edited files but could not persist them to a branch
            // (a held branch ref, a failed checkout, a commit error). Its
            // work has vanished; letting the status stand - RUN_FINISHED_DONE
            // for a green run - would report success for a change no one can
            // review. Demote to a non-shippable terminal with an honest
            // reason so the operator sees the truth, not a phantom DONE.
            record(
                kernel,
                request,
                "evidence_appended",
                &format!(
                    "commit_failed: {} file change(s) could not be persisted to branch {branch}",
                    outcome.files_changed
                ),
                turn_index,
                0,
                0,
            );
            outcome.status = demoted;
            outcome.branch = None;
            outcome.commit_sha = None;
        }
    }

    run_last_chance_proof_if_changed_branch_passes(
        kernel,
        request,
        &workspace_path,
        repo_root,
        &mut outcome,
    );
    promote_preexisting_red_branch_to_review_if_needed(
        kernel,
        request,
        &mut outcome,
        baseline_selected_proof_red_on_arrival,
        baseline_repair_expected,
    );
    preserve_selected_proof_truth_on_budget_exhaustion(&mut outcome);

    let run_summary = bounded_human_run_text(&outcome.finish_summary, 700);
    let operator_summary =
        render_operator_run_summary(kernel, request, &outcome, &voice_profile, &run_summary);
    if operator_summary.input_tokens > 0 || operator_summary.output_tokens > 0 {
        spent_microcents =
            spent_microcents.saturating_add(crate::forge_model_pricing::turn_cost_microcents(
                &operator_summary.model_id,
                operator_summary.input_tokens,
                operator_summary.output_tokens,
            ));
        record_timed(
            kernel,
            request,
            TimedRunEvent {
                event: "model_called",
                detail: &format!("voice rewrite model={}", operator_summary.model_id),
                turn: outcome.turns_used,
                input_tokens: operator_summary.input_tokens,
                output_tokens: operator_summary.output_tokens,
                duration_ms: 0,
            },
        );
    }
    let check_runs = outcome.check_runs.to_string();
    let check_duration_ms = outcome.check_duration_ms.to_string();
    let selected_proof_status = selected_proof_status(&outcome);
    let voice_rewrite_input_tokens = operator_summary.input_tokens.to_string();
    let voice_rewrite_output_tokens = operator_summary.output_tokens.to_string();
    if !run_summary.is_empty() {
        record_with_evidence_fields(
            kernel,
            request,
            "run_summary",
            &run_summary,
            outcome.turns_used,
            &[
                ("run_summary", run_summary.as_str()),
                ("run_summary_source", "finish_summary"),
                ("operator_run_summary", operator_summary.summary.as_str()),
                ("operator_run_summary_source", operator_summary.source),
                ("voice_rewrite_status", operator_summary.status),
                (
                    "voice_rewrite_model_provider_family",
                    operator_summary.model_provider_family.as_str(),
                ),
                ("voice_rewrite_model_id", operator_summary.model_id.as_str()),
                (
                    "voice_rewrite_input_tokens",
                    voice_rewrite_input_tokens.as_str(),
                ),
                (
                    "voice_rewrite_output_tokens",
                    voice_rewrite_output_tokens.as_str(),
                ),
                ("voice_rewrite_kind", "tone_only"),
                ("voice_rewrite_claims_expansion_allowed", "false"),
                ("forge_run_voice_profile_id", voice_profile.id),
                ("forge_run_voice_profile_source", "server_profile"),
                ("run_summary_grants_execution_authority", "false"),
                ("operator_run_summary_grants_execution_authority", "false"),
                ("check_runs", check_runs.as_str()),
                ("check_duration_ms", check_duration_ms.as_str()),
                ("selected_proof_status", selected_proof_status),
            ],
        );
    }

    let run_cost_cents =
        crate::forge_model_pricing::microcents_to_cents_rounded_up(spent_microcents).to_string();
    record_timed_with_evidence_fields(
        kernel,
        request,
        TimedRunEvent {
            event: "run_finished",
            detail: &format!(
                "status={} turns={} files_changed={} check_runs={} check_duration_ms={} selected_proof_status={} cost_cents={}",
                outcome.status,
                outcome.turns_used,
                outcome.files_changed,
                outcome.check_runs,
                outcome.check_duration_ms,
                selected_proof_status,
                crate::forge_model_pricing::microcents_to_cents_rounded_up(spent_microcents)
            ),
            turn: outcome.turns_used,
            input_tokens: 0,
            output_tokens: 0,
            duration_ms: run_started_at.elapsed().as_millis() as u64,
        },
        &[
            ("run_summary", run_summary.as_str()),
            ("run_summary_source", "finish_summary"),
            ("run_cost_cents", run_cost_cents.as_str()),
            ("run_cost_basis", "static_rate_table_upper_bound"),
            ("operator_run_summary", operator_summary.summary.as_str()),
            ("operator_run_summary_source", operator_summary.source),
            ("voice_rewrite_status", operator_summary.status),
            (
                "voice_rewrite_model_provider_family",
                operator_summary.model_provider_family.as_str(),
            ),
            ("voice_rewrite_model_id", operator_summary.model_id.as_str()),
            (
                "voice_rewrite_input_tokens",
                voice_rewrite_input_tokens.as_str(),
            ),
            (
                "voice_rewrite_output_tokens",
                voice_rewrite_output_tokens.as_str(),
            ),
            ("voice_rewrite_kind", "tone_only"),
            ("voice_rewrite_claims_expansion_allowed", "false"),
            ("forge_run_voice_profile_id", voice_profile.id),
            ("forge_run_voice_profile_source", "server_profile"),
            ("run_summary_grants_execution_authority", "false"),
            ("operator_run_summary_grants_execution_authority", "false"),
            ("selected_proof_status", selected_proof_status),
            ("check_runs", check_runs.as_str()),
            ("check_duration_ms", check_duration_ms.as_str()),
        ],
    );
    outcome
}

fn initial_applied_control_receipts(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
    resume: bool,
) -> HashSet<String> {
    if !resume {
        return HashSet::new();
    }
    kernel
        .read()
        .map(|kernel| {
            kernel
                .forge_run_controls(run_id)
                .into_iter()
                .map(|control| control.receipt_id)
                .collect()
        })
        .unwrap_or_default()
}

fn premature_cannot_proceed_refusal(
    request: &ForgeRunRequest,
    outcome: &ForgeRunOutcome,
    orientation_gate: &PrincipalOrientationGate,
    finish_outcome: &str,
    attempted_non_setup_proof: bool,
) -> bool {
    finish_outcome == "cannot_proceed"
        && outcome.files_changed == 0
        && !attempted_non_setup_proof
        && orientation_gate.ready_to_edit_or_check()
        && !request.write_scope.is_empty()
        && request
            .allowed_commands
            .iter()
            .any(|command| !crate::forge_repo_profile::is_setup_command(command))
}

/// Benchmark calibration posture: when MDX_FORGE_BENCH_OPEN_COMMANDS=1 the
/// run_command allowlist opens to any command. External benchmarks
/// (Terminal-Bench and friends) pose arbitrary terminal work that cannot be
/// pre-enumerated; the exact-match allowlist would make every score a
/// measurement of the allowlist, not the harness. Commands still execute
/// through the same scrubbed sandbox path - this opens the LIST, never the
/// confinement - and the flip is receipted on the run so the posture is
/// never invisible. Off by default; never set it on a governed run.
pub(crate) fn bench_open_commands_env_enabled() -> bool {
    #[cfg(test)]
    if let Some(value) = BENCH_OPEN_COMMANDS_TEST_OVERRIDE.with(std::cell::Cell::get) {
        return value;
    }
    std::env::var("MDX_FORGE_BENCH_OPEN_COMMANDS")
        .ok()
        .as_deref()
        == Some("1")
}

#[cfg(test)]
thread_local! {
    static BENCH_OPEN_COMMANDS_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn set_bench_open_commands_test_override(value: Option<bool>) -> Option<bool> {
    BENCH_OPEN_COMMANDS_TEST_OVERRIDE.with(|current| current.replace(value))
}

fn bench_open_commands_enabled(request: &ForgeRunRequest) -> bool {
    bench_open_commands_env_enabled() && request.execution_geometry_route == "cli:forge-bench"
}

/// Consecutive failures of the SAME check command before the anti-thrash
/// step-back nudge fires. Small, env-overridable; floor 2 so a single
/// fail-then-fix never trips it.
fn thrash_step_back_threshold() -> u32 {
    std::env::var("MDX_FORGE_THRASH_STEP_BACK_AFTER")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v| *v >= 2)
        .unwrap_or(3)
}

fn stalled_orientation_tool(name: &str) -> bool {
    matches!(
        name,
        "search" | "list_dir" | "semantic_query" | "investigate"
    )
}

fn stalled_orientation_call(call: &ParsedToolCall, allowed_commands: &[String]) -> bool {
    if stalled_orientation_tool(&call.name) {
        return true;
    }
    if !matches!(call.name.as_str(), "run_command" | "shell") {
        return false;
    }
    let command = call.arguments["command"].as_str().unwrap_or("");
    stalled_orientation_shell_command(command, allowed_commands)
}

fn repeated_stalled_target_read(call: &ParsedToolCall, already_read: &HashSet<String>) -> bool {
    target_read_key(call)
        .map(|path| already_read.contains(&path))
        .unwrap_or(false)
}

fn target_read_key(call: &ParsedToolCall) -> Option<String> {
    if call.name != "read_file" {
        return None;
    }
    let path = call.arguments["path"].as_str()?.trim();
    if path.is_empty() {
        return None;
    }
    Some(path.trim_start_matches("./").to_string())
}

fn stalled_orientation_shell_command(command: &str, allowed_commands: &[String]) -> bool {
    if selected_proof_command(command, allowed_commands) {
        return false;
    }
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }
    let first = trimmed
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_');
    matches!(
        first,
        "cat"
            | "egrep"
            | "fd"
            | "find"
            | "grep"
            | "head"
            | "ls"
            | "pwd"
            | "rg"
            | "sed"
            | "tail"
            | "tree"
            | "wc"
    )
}

fn selected_proof_command(command: &str, allowed_commands: &[String]) -> bool {
    let command = command.trim();
    !command.is_empty()
        && allowed_commands
            .iter()
            .map(String::as_str)
            .filter(|allowed| !crate::forge_repo_profile::is_setup_command(allowed))
            .any(|allowed| allowed.trim() == command)
}

fn run_baseline_proof_preflight(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    workspace_path: &Path,
    repo_root: &Path,
    outcome: &mut ForgeRunOutcome,
) -> Option<BaselineProofPreflight> {
    if !env_flag_enabled("MDX_FORGE_BASELINE_PROOF_PREFLIGHT", true) {
        return None;
    }
    let commands = request
        .allowed_commands
        .iter()
        .map(String::as_str)
        .filter(|command| !command.trim().is_empty())
        .collect::<Vec<_>>();
    if !commands
        .iter()
        .any(|command| !crate::forge_repo_profile::is_setup_command(command))
    {
        return None;
    }
    record(
        kernel,
        request,
        "evidence_appended",
        "baseline selected proof preflight started",
        0,
        0,
        0,
    );
    for command in commands {
        record(
            kernel,
            request,
            "check_started",
            &format!("baseline_run_command_started {command}"),
            0,
            0,
            0,
        );
        let (passed, exit_code, duration_ms, tail) = run_selected_check(
            &request.run_id,
            command,
            workspace_path,
            repo_root,
            request.check_target_dir.as_deref(),
        );
        outcome.check_runs += 1;
        outcome.check_duration_ms = outcome.check_duration_ms.saturating_add(duration_ms);
        let setup = crate::forge_repo_profile::is_setup_command(command);
        record_timed(
            kernel,
            request,
            TimedRunEvent {
                event: if passed {
                    "check_passed"
                } else {
                    "check_failed"
                },
                detail: &format!(
                    "baseline_run_command {command} exit={exit_code} tail={}",
                    compact_check_tail(&tail)
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
                duration_ms,
            },
        );
        if passed {
            continue;
        }
        let summary = if setup {
            format!(
                "selected proof setup failed before model turns: `{command}` exited {exit_code} after {duration_ms} ms"
            )
        } else {
            format!(
                "selected proof is red on arrival before model turns: `{command}` exited {exit_code} after {duration_ms} ms"
            )
        };
        if !setup {
            // Investigate, then proceed - the way a capable engineer would. A
            // red test/check on arrival is recorded as evidence (the
            // check_failed event above) but does not stop the run: Forge makes
            // the requested change and surfaces the pre-existing failure as
            // context, instead of dead-ending at turn 0 and blaming the
            // operator. If the ask is explicitly about repairing the baseline,
            // the failure is framed as the first proof signal.
            if intent_allows_baseline_red_repair(&request.intent) {
                return Some(BaselineProofPreflight {
                    note: format!(
                        "{summary}. The task explicitly asks to repair that failing baseline, so continue using this failure as the first proof signal."
                    ),
                    selected_proof_red_on_arrival: true,
                    baseline_repair_expected: true,
                });
            }
            return Some(BaselineProofPreflight {
                note: format!(
                    "{summary}. This baseline was already red before your ask, not caused by it - Forge will make the requested change and report this pre-existing failure separately."
                ),
                selected_proof_red_on_arrival: true,
                baseline_repair_expected: false,
            });
        }
        // A failing SETUP command (broken install/toolchain) is different: the
        // environment itself cannot run, so there is nothing safe to build on.
        outcome.status = "RUN_FINISHED_CANNOT_PROCEED";
        outcome.finish_summary = format!(
            "{summary}. This is a setup/toolchain failure, so the run cannot proceed until the environment is fixed."
        );
        return Some(BaselineProofPreflight {
            note: outcome.finish_summary.clone(),
            selected_proof_red_on_arrival: false,
            baseline_repair_expected: false,
        });
    }
    Some(BaselineProofPreflight {
        note: format!(
            "Baseline selected proof passed before model turns. Check cost so far: {} ms across {} run(s). Make the requested change and rerun proof after editing.",
            outcome.check_duration_ms, outcome.check_runs
        ),
        selected_proof_red_on_arrival: false,
        baseline_repair_expected: false,
    })
}

/// Why the run must stop spending, or None while inside both ceilings.
/// Pure so the enforcement rule itself is testable: recorded numbers are
/// enforced numbers.
fn budget_breach(
    elapsed_ms: u64,
    max_runtime_ms: u64,
    spent_cents: u64,
    max_cost_cents: u64,
) -> Option<String> {
    if max_runtime_ms > 0 && elapsed_ms > max_runtime_ms {
        return Some(format!(
            "runtime budget exhausted: elapsed_ms={elapsed_ms} max_runtime_ms={max_runtime_ms}"
        ));
    }
    if max_cost_cents > 0 && spent_cents >= max_cost_cents {
        return Some(format!(
            "cost budget exhausted: spent_cents={spent_cents} max_cost_cents={max_cost_cents}"
        ));
    }
    None
}

fn account_delegated_model_usage(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    turn_index: u32,
    label: &str,
    report: &crate::forge_subagent::SubagentReport,
    spent_microcents: &mut u64,
) {
    debug_assert_eq!(
        report
            .model_usage
            .iter()
            .map(|usage| usage.input_tokens)
            .sum::<u64>(),
        report.input_tokens()
    );
    debug_assert_eq!(
        report
            .model_usage
            .iter()
            .map(|usage| usage.output_tokens)
            .sum::<u64>(),
        report.output_tokens()
    );
    let total = report.model_call_count();
    for (index, usage) in report.model_usage.iter().enumerate() {
        *spent_microcents =
            spent_microcents.saturating_add(crate::forge_model_pricing::turn_cost_microcents(
                &usage.model_id,
                usage.input_tokens,
                usage.output_tokens,
            ));
        let detail = format!(
            "delegated {label} model={} call={}/{}",
            usage.model_id,
            index + 1,
            total
        );
        record_timed(
            kernel,
            request,
            TimedRunEvent {
                event: "model_called",
                detail: &detail,
                turn: turn_index,
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                duration_ms: 0,
            },
        );
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
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

fn intent_allows_baseline_red_repair(intent: &str) -> bool {
    let intent = intent.to_ascii_lowercase();
    if [
        "fix failing",
        "repair failing",
        "fix the failing",
        "repair the failing",
        "diagnose and repair",
        "make the failing",
        "turn the failing",
    ]
    .iter()
    .any(|needle| intent.contains(needle))
    {
        return true;
    }

    ["this is a forge hard-gauntlet task"]
        .iter()
        .any(|needle| intent.contains(needle))
}

fn apply_finish_outcome(outcome: &mut ForgeRunOutcome, finish_outcome: &str, summary: &str) {
    outcome.finish_summary = summary.to_string();
    outcome.status = if finish_outcome == "done" {
        if outcome.files_changed == 0 {
            if outcome.finish_summary.trim().is_empty() {
                outcome.finish_summary =
                    "the agent declared done, but no task diff was produced".to_string();
            }
            "RUN_FINISHED_NO_CHANGE"
        } else {
            "RUN_FINISHED_DONE"
        }
    } else if outcome.files_changed > 0 && outcome.last_check_passed {
        if outcome.finish_summary.trim().is_empty() {
            outcome.finish_summary =
                "the agent hesitated, but the changed branch passed its selected proof".to_string();
        }
        "RUN_FINISHED_DONE"
    } else {
        "RUN_FINISHED_CANNOT_PROCEED"
    };
}

fn run_last_chance_proof_if_changed_branch_passes(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    workspace_path: &Path,
    repo_root: &Path,
    outcome: &mut ForgeRunOutcome,
) {
    if outcome.status != "RUN_FINISHED_CANNOT_PROCEED"
        || outcome.files_changed == 0
        || outcome.last_check_passed
        || outcome.branch.is_none()
    {
        // A last-chance pass can only promote to a shippable DONE when there
        // is a committed branch to ship. A run whose commit failed had its
        // branch cleared (the honesty demotion above); promoting it would
        // resurrect the exact phantom DONE - a green-looking terminal with no
        // reviewable branch - this function must never mint.
        return;
    }
    let proof_commands = request
        .allowed_commands
        .iter()
        .map(String::as_str)
        .filter(|command| !crate::forge_repo_profile::is_setup_command(command))
        .collect::<Vec<_>>();
    if proof_commands.is_empty() {
        return;
    }
    record(
        kernel,
        request,
        "evidence_appended",
        "last_chance_proof_started",
        outcome.turns_used,
        0,
        0,
    );
    for command in proof_commands {
        let (passed, exit_code, duration_ms, tail) = run_selected_check(
            &request.run_id,
            command,
            workspace_path,
            repo_root,
            request.check_target_dir.as_deref(),
        );
        outcome.check_runs += 1;
        outcome.check_duration_ms = outcome.check_duration_ms.saturating_add(duration_ms);
        record_timed(
            kernel,
            request,
            TimedRunEvent {
                event: if passed {
                    "check_passed"
                } else {
                    "check_failed"
                },
                detail: &format!(
                    "last_chance_run_command {command} exit={exit_code} tail={}",
                    compact_check_tail(&tail)
                ),
                turn: outcome.turns_used,
                input_tokens: 0,
                output_tokens: 0,
                duration_ms,
            },
        );
        if passed {
            outcome.last_check_passed = true;
            outcome.status = "RUN_FINISHED_DONE";
            if outcome.finish_summary.trim().is_empty() {
                outcome.finish_summary =
                    "last-chance selected proof passed on the changed branch".to_string();
            }
            record(
                kernel,
                request,
                "evidence_appended",
                "last_chance_proof_passed",
                outcome.turns_used,
                0,
                0,
            );
            break;
        }
    }
}

fn promote_preexisting_red_branch_to_review_if_needed(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    outcome: &mut ForgeRunOutcome,
    baseline_selected_proof_red_on_arrival: bool,
    baseline_repair_expected: bool,
) {
    if !promote_preexisting_red_branch_to_review(
        outcome,
        baseline_selected_proof_red_on_arrival,
        baseline_repair_expected,
    ) {
        return;
    }
    record(
        kernel,
        request,
        "evidence_appended",
        "pre-existing selected proof stayed red after a reviewable branch landed; classifying as ready for review with proof caveat, not cannot_proceed",
        outcome.turns_used,
        0,
        0,
    );
}

fn promote_preexisting_red_branch_to_review(
    outcome: &mut ForgeRunOutcome,
    baseline_selected_proof_red_on_arrival: bool,
    baseline_repair_expected: bool,
) -> bool {
    if outcome.status != "RUN_FINISHED_CANNOT_PROCEED"
        || !baseline_selected_proof_red_on_arrival
        || baseline_repair_expected
        || outcome.files_changed == 0
        || outcome.last_check_passed
        || outcome.branch.is_none()
    {
        return false;
    }

    outcome.status = "RUN_FINISHED_DONE";
    let caveat = "Reviewable branch landed, but the selected proof was already red before the run and stayed red. Treat this as ready for review with a proof caveat, not a hard stop.";
    if outcome.finish_summary.trim().is_empty() {
        outcome.finish_summary = caveat.to_string();
    } else {
        outcome.finish_summary =
            reviewable_preexisting_red_summary(&outcome.finish_summary, caveat);
    }
    true
}

fn reviewable_preexisting_red_summary(summary: &str, caveat: &str) -> String {
    let mut cleaned = summary.trim().to_string();
    for (blocked, reviewable) in [
        (
            "cannot mark done because",
            "ready for review with a proof caveat because",
        ),
        (
            "cannot mark it done because",
            "ready for review with a proof caveat because",
        ),
        (
            "cannot finish as done because",
            "ready for review with a proof caveat because",
        ),
    ] {
        cleaned = cleaned.replace(blocked, reviewable);
    }
    if cleaned.to_ascii_lowercase().contains("proof caveat") {
        cleaned
    } else {
        format!("{cleaned} {caveat}")
    }
}

fn fresh_review_branch_name(run_id: &str, workspace_id: &str) -> String {
    format!(
        "forge/run-{}-{}",
        sanitize_review_branch_component(run_id),
        sanitize_review_branch_component(workspace_id)
    )
}

fn sanitize_review_branch_component(value: &str) -> String {
    let mut out = String::new();
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "run".to_string()
    } else {
        out
    }
}

static OUTSTANDING_MODEL_TURN_THREADS: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
struct OutstandingModelTurnGuard;

impl Drop for OutstandingModelTurnGuard {
    fn drop(&mut self) {
        OUTSTANDING_MODEL_TURN_THREADS.fetch_sub(1, Ordering::SeqCst);
    }
}

fn max_outstanding_model_turns() -> usize {
    std::env::var("MDX_FORGE_MAX_OUTSTANDING_MODEL_TURNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_OUTSTANDING_MODEL_TURNS)
}

fn try_enter_model_turn_thread() -> Result<OutstandingModelTurnGuard, String> {
    let max = max_outstanding_model_turns();
    loop {
        let current = OUTSTANDING_MODEL_TURN_THREADS.load(Ordering::SeqCst);
        if current >= max {
            return Err(format!(
                "model call worker limit reached: outstanding={current} max={max}"
            ));
        }
        if OUTSTANDING_MODEL_TURN_THREADS
            .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return Ok(OutstandingModelTurnGuard);
        }
    }
}

fn model_turn_with_watchdog(
    client: &TurnClient,
    messages: &[serde_json::Value],
    request: &ForgeRunRequest,
    turn_index: u32,
) -> Result<crate::forge_turn_client::ModelTurn, String> {
    let timeout = model_turn_timeout();
    let outstanding_guard = try_enter_model_turn_thread()?;
    let client = client.clone();
    let messages = messages.to_vec();
    let run_id = request.run_id.clone();
    let builder_slot = request.builder_slot.clone();
    let (tx, rx) = mpsc::channel();
    let deadline = Instant::now() + timeout;
    std::thread::spawn(move || {
        let _outstanding_guard = outstanding_guard;
        let provider_family = client.provider_family().to_string();
        let model_id = client.model_id().to_string();
        let model_class =
            crate::forge_run_stream::model_class(&provider_family, &builder_slot, &model_id)
                .to_string();
        let mut sink = |delta: &str| {
            crate::forge_run_stream::publish_token_delta(
                &run_id,
                "planning",
                turn_index,
                delta,
                &crate::forge_run_stream::RunStreamModel {
                    provider_family: &provider_family,
                    model_id: &model_id,
                    slot: &builder_slot,
                    model_class: &model_class,
                },
            );
        };
        let _ = tx.send(crate::forge_turn_client::with_model_turn_deadline(
            deadline,
            || client.turn_streaming(&messages, &mut sink),
        ));
    });
    match rx.recv_timeout(timeout) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
            "model call timed out after {} seconds",
            timeout.as_secs()
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("model call worker disconnected before returning".to_string())
        }
    }
}

fn tool_call_summary(call: &ParsedToolCall) -> String {
    let string_arg = |name: &str| {
        call.arguments[name]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(140)
            .collect::<String>()
    };
    match call.name.as_str() {
        "read_file" => {
            let path = string_arg("path");
            if path.is_empty() {
                "Read a workspace file".to_string()
            } else {
                format!("Read {path}")
            }
        }
        "list_dir" => {
            let path = string_arg("path");
            if path.is_empty() {
                "List the repo root".to_string()
            } else {
                format!("List {path}")
            }
        }
        "search" => {
            let pattern = string_arg("pattern");
            if pattern.is_empty() {
                "Search the workspace".to_string()
            } else {
                format!("Search for {pattern}")
            }
        }
        "semantic_query" => {
            let operation = string_arg("operation");
            if operation.is_empty() {
                "Ask the semantic index for orientation".to_string()
            } else {
                format!("Ask semantic index: {operation}")
            }
        }
        "edit_file" => {
            let path = string_arg("path");
            if path.is_empty() {
                "Edit a scoped file".to_string()
            } else {
                format!("Edit {path}")
            }
        }
        "write_file" => {
            let path = string_arg("path");
            if path.is_empty() {
                "Write a scoped file".to_string()
            } else {
                format!("Write {path}")
            }
        }
        "apply_patch" => {
            let input = string_arg("input");
            if input.is_empty() {
                "Apply a patch to scoped files".to_string()
            } else {
                "Apply a patch".to_string()
            }
        }
        "run_command" => {
            let command = string_arg("command");
            if command.is_empty() {
                "Run the selected proof command".to_string()
            } else {
                format!("Run {command}")
            }
        }
        "finish" => {
            let outcome = string_arg("outcome");
            if outcome.is_empty() {
                "Finish the run".to_string()
            } else {
                format!("Finish as {outcome}")
            }
        }
        other => format!("Call {other}"),
    }
}

fn model_turn_timeout() -> Duration {
    std::env::var("MDX_FORGE_MODEL_TURN_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| *seconds >= 30)
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_secs(DEFAULT_MODEL_TURN_TIMEOUT_SECS))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PrincipalOrientationGate {
    required: bool,
    language_pack_id: String,
    semantic_query_seen: bool,
    semantic_anchor_seen: bool,
    semantic_operations_seen: HashSet<String>,
    required_semantic_operations: Vec<String>,
    semantic_strategy_required_before_write: bool,
    structured_policy_required_before_finish: bool,
    dependency_map_seen: bool,
    dependency_map_required_before_write: bool,
    related_tests_seen: bool,
    changed_files_seen: bool,
}

impl PrincipalOrientationGate {
    fn for_workspace(workspace_path: &Path) -> Self {
        let profile = crate::forge_repo_profile::profile_repo(workspace_path);
        Self {
            required: profile.language_pack_id != "generic"
                && !profile.semantic_intelligence.is_empty(),
            language_pack_id: profile.language_pack_id.to_string(),
            semantic_query_seen: false,
            semantic_anchor_seen: false,
            semantic_operations_seen: HashSet::new(),
            required_semantic_operations: Vec::new(),
            semantic_strategy_required_before_write: false,
            structured_policy_required_before_finish: false,
            dependency_map_seen: false,
            dependency_map_required_before_write: false,
            related_tests_seen: false,
            changed_files_seen: false,
        }
    }

    fn for_request(workspace_path: &Path, request: &ForgeRunRequest) -> Self {
        let profile = crate::forge_repo_profile::profile_repo_for_task(
            workspace_path,
            &request.intent,
            &request.write_scope,
            &request.allowed_commands,
        );
        let mut gate = Self {
            required: profile.language_pack_id != "generic"
                && !profile.semantic_intelligence.is_empty(),
            language_pack_id: profile.language_pack_id.to_string(),
            semantic_query_seen: false,
            semantic_anchor_seen: false,
            semantic_operations_seen: HashSet::new(),
            required_semantic_operations: Vec::new(),
            semantic_strategy_required_before_write: false,
            structured_policy_required_before_finish: false,
            dependency_map_seen: false,
            dependency_map_required_before_write: false,
            related_tests_seen: false,
            changed_files_seen: false,
        };
        gate.dependency_map_required_before_write = request.execution_geometry_lane
            == "bounded_parallel_exploration"
            && request.execution_geometry_effective_workers > 1;
        gate.required_semantic_operations = normalized_required_semantic_operations(
            required_semantic_operations_from_intent(&request.intent)
                .into_iter()
                .chain(structured_semantic_policy_operations(request)),
        );
        gate.structured_policy_required_before_finish =
            should_require_structured_semantic_policy(request);
        gate.semantic_strategy_required_before_write =
            matches!(
                request.execution_geometry_lane.as_str(),
                "bounded_parallel_exploration" | "fleet_stream" | "fleet_integration"
            ) && !gate.required_semantic_operations.is_empty();
        gate
    }

    fn observe_tool(
        &mut self,
        name: &str,
        arguments: &serde_json::Value,
        changed_files: bool,
        result_text: &str,
    ) {
        if name == "semantic_query" {
            let query_succeeded = semantic_query_succeeded(result_text);
            if query_succeeded {
                self.semantic_query_seen = true;
                if let Some(operation) = semantic_query_operation(result_text) {
                    self.semantic_operations_seen.insert(operation.clone());
                    if semantic_query_operation_anchors_work(&operation) {
                        self.semantic_anchor_seen = true;
                    }
                    if operation == "dependency_map" {
                        self.dependency_map_seen = true;
                    }
                }
                if arguments["operation"].as_str() == Some("related_tests") {
                    self.related_tests_seen = true;
                }
            }
        }
        if changed_files && matches!(name, "edit_file" | "write_file" | "apply_patch") {
            self.changed_files_seen = true;
        }
    }

    fn refusal_for(&self, name: &str) -> Option<String> {
        if !self.required {
            return None;
        }
        if !self.semantic_anchor_seen {
            if !matches!(
                name,
                "edit_file" | "write_file" | "apply_patch" | "run_command" | "finish"
            ) {
                return None;
            }
            return Some(format!(
                "principal orientation required before {name}: this repo uses language pack {}. Call semantic_query first with an anchoring operation such as symbol_graph, orientation, impact_radius, dependency_map, file_outline, related_tests, workspace_symbol, path+line definition, path+line references, or diagnostics. Capabilities and lsp_probe are readiness checks only. This does not grant authority or mark checks passed.",
                self.language_pack_id
            ));
        }
        if self.semantic_strategy_required_before_write
            && !self.required_semantic_operations.is_empty()
            && !self.required_semantic_operations_satisfied()
            && matches!(
                name,
                "edit_file" | "write_file" | "apply_patch" | "run_command" | "finish"
            )
        {
            let missing = self.missing_required_semantic_operations().join(", ");
            return Some(format!(
                "assigned semantic policy required before {name}: call semantic_query for the assigned operations first: {missing}. This keeps serious, parallel, and fleet work distinct and reviewable without granting authority."
            ));
        }
        if self.dependency_map_required_before_write
            && !self.dependency_map_seen
            && matches!(
                name,
                "edit_file" | "write_file" | "apply_patch" | "run_command" | "finish"
            )
        {
            return Some(format!(
                "parallel candidate dependency map required before {name}: this bounded parallel run must call semantic_query with operation dependency_map for the target path or symbol before editing, checking, or finishing. This keeps fleet candidates grounded in imports, manifests, neighboring source, and related tests without granting authority."
            ));
        }
        if name == "finish" && self.changed_files_seen && !self.related_tests_seen {
            return Some(format!(
                "principal related-test orientation required before finish: this repo uses language pack {} and files changed. Call semantic_query with operation related_tests for a touched source path or symbol. This does not grant authority or mark checks passed.",
                self.language_pack_id
            ));
        }
        if name == "finish"
            && self.structured_policy_required_before_finish
            && !self.required_semantic_operations.is_empty()
            && !self.required_semantic_operations_satisfied()
        {
            let missing = self.missing_required_semantic_operations().join(", ");
            return Some(format!(
                "assigned semantic policy required before finish: call semantic_query for the assigned operations first: {missing}. Medium and high complexity work can edit after orientation, but final handoff needs this semantic evidence."
            ));
        }
        None
    }

    fn required_semantic_operations_satisfied(&self) -> bool {
        self.required_semantic_operations.iter().all(|operation| {
            self.semantic_operations_seen
                .iter()
                .any(|seen| seen == operation)
        })
    }

    fn missing_required_semantic_operations(&self) -> Vec<String> {
        self.required_semantic_operations
            .iter()
            .filter(|operation| {
                !self
                    .semantic_operations_seen
                    .iter()
                    .any(|seen| seen == *operation)
            })
            .cloned()
            .collect()
    }

    fn ready_to_edit_or_check(&self) -> bool {
        self.refusal_for("edit_file").is_none() && self.refusal_for("run_command").is_none()
    }
}

fn record_semantic_strategy_assignment(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    gate: &PrincipalOrientationGate,
) {
    if !gate.required
        || (!gate.semantic_strategy_required_before_write
            && !gate.structured_policy_required_before_finish
            && !gate.dependency_map_required_before_write)
    {
        return;
    }
    let required_operations = if gate.required_semantic_operations.is_empty() {
        "none".to_string()
    } else {
        gate.required_semantic_operations.join("\n")
    };
    let dependency_map_required = if gate.dependency_map_required_before_write {
        "true"
    } else {
        "false"
    };
    let strategy_required = if gate.semantic_strategy_required_before_write {
        "true"
    } else {
        "false"
    };
    let structured_policy_required_before_finish = if gate.structured_policy_required_before_finish
    {
        "true"
    } else {
        "false"
    };
    record_with_evidence_fields(
        kernel,
        request,
        "evidence_appended",
        "semantic_strategy_assigned",
        0,
        &[
            (
                "principal_orientation_required_semantic_operations",
                required_operations.as_str(),
            ),
            (
                "principal_orientation_semantic_strategy_required",
                strategy_required,
            ),
            (
                "principal_orientation_structured_policy_required_before_finish",
                structured_policy_required_before_finish,
            ),
            (
                "principal_orientation_dependency_map_required",
                dependency_map_required,
            ),
            (
                "principal_orientation_assignment_source",
                "forge_loop_runner::PrincipalOrientationGate",
            ),
            (
                "principal_orientation_complexity_tier",
                request.work_complexity_tier.as_str(),
            ),
            (
                "principal_orientation_policy_source",
                request.semantic_policy_source.as_str(),
            ),
            ("principal_orientation_grants_authority", "false"),
        ],
    );
}

fn semantic_query_succeeded(result_text: &str) -> bool {
    result_text
        .lines()
        .next()
        .is_some_and(|line| line.split_whitespace().any(|token| token == "status=OK"))
}

fn semantic_query_operation(result_text: &str) -> Option<String> {
    let first_line = result_text
        .lines()
        .find(|line| line.starts_with("semantic_query "))?;
    for token in first_line.split_whitespace() {
        if let Some(operation) = token.strip_prefix("operation=") {
            let operation = operation.trim();
            if !operation.is_empty() {
                return Some(operation.to_string());
            }
        }
    }
    first_line
        .split_whitespace()
        .nth(1)
        .map(str::trim)
        .filter(|operation| !operation.is_empty() && *operation != "status=OK")
        .map(str::to_string)
}

fn semantic_query_operation_anchors_work(operation: &str) -> bool {
    matches!(
        operation,
        "orientation"
            | "symbol_graph"
            | "workspace_symbol"
            | "definition"
            | "references"
            | "impact_radius"
            | "dependency_map"
            | "hover"
            | "file_outline"
            | "related_tests"
            | "diagnostics"
    )
}

fn required_semantic_operations_from_intent(intent: &str) -> Vec<String> {
    intent
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("Required semantic operations:")
                .map(str::trim)
        })
        .map(|line| line.trim_end_matches('.'))
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|operation| !operation.is_empty())
        .map(str::to_string)
        .collect()
}

fn should_require_structured_semantic_policy(request: &ForgeRunRequest) -> bool {
    matches!(
        request.work_complexity_tier.as_str(),
        "medium" | "large" | "xl"
    ) && !request.semantic_policy_required_operations.is_empty()
}

fn structured_semantic_policy_operations(request: &ForgeRunRequest) -> Vec<String> {
    if !should_require_structured_semantic_policy(request) {
        return Vec::new();
    }
    request.semantic_policy_required_operations.clone()
}

fn normalized_required_semantic_operations<I>(operations: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut normalized = Vec::new();
    let mut seen = HashSet::new();
    for operation in operations {
        let operation = operation.trim().trim_end_matches('.').to_string();
        if operation.is_empty() || semantic_query_operation_is_readiness_only(&operation) {
            continue;
        }
        if seen.insert(operation.clone()) {
            normalized.push(operation);
        }
    }
    normalized
}

fn semantic_query_operation_is_readiness_only(operation: &str) -> bool {
    matches!(operation, "capabilities" | "lsp_probe")
}

/// Dispatch one tool call to its gated primitive. Returns the text the
/// model sees, plus the receipt event/detail/check-passed for the chain.
fn dispatch_tool(
    call: &ParsedToolCall,
    workspace_path: &Path,
    repo_root: &Path,
    request: &ForgeRunRequest,
    outcome: &mut ForgeRunOutcome,
) -> (String, &'static str, String, bool) {
    // Plan mode disables the tools that change or prove the workspace - the run
    // proposes, it does not build.
    if request.plan_only
        && matches!(
            call.name.as_str(),
            "edit_file" | "write_file" | "apply_patch" | "run_command" | "shell"
        )
    {
        return (
            format!(
                "error: {} is disabled in plan mode. Investigate and propose a plan (update_plan, then finish); a human ratifies it before any code is written.",
                call.name
            ),
            "tool_executed",
            format!("plan_mode refused {}", call.name),
            false,
        );
    }
    match call.name.as_str() {
        "read_file" => {
            let rel = call.arguments["path"].as_str().unwrap_or("");
            match read_confined(workspace_path, rel) {
                Ok(content) => (content, "tool_executed", format!("read_file {rel}"), false),
                Err(reason) => (
                    format!("error: {reason}"),
                    "tool_executed",
                    format!("read_file refused {rel}"),
                    false,
                ),
            }
        }
        "list_dir" => {
            let rel = call.arguments["path"].as_str().unwrap_or("");
            match list_confined(workspace_path, rel) {
                Ok(listing) => (listing, "tool_executed", format!("list_dir {rel}"), false),
                Err(reason) => (
                    format!("error: {reason}"),
                    "tool_executed",
                    format!("list_dir refused {rel}"),
                    false,
                ),
            }
        }
        "search" => {
            let pattern = call.arguments["pattern"].as_str().unwrap_or("");
            let prefix = call.arguments["path_prefix"].as_str().unwrap_or("");
            let hits = search_confined(workspace_path, pattern, prefix);
            (hits, "tool_executed", format!("search {pattern}"), false)
        }
        "semantic_query" => {
            let operation = call.arguments["operation"]
                .as_str()
                .unwrap_or("capabilities");
            let query = call.arguments["query"].as_str().unwrap_or("");
            let path = call.arguments["path"].as_str().unwrap_or("");
            let line = call.arguments["line"].as_u64().unwrap_or(0) as usize;
            let result = crate::forge_semantic_query_route::semantic_query_tool_text(
                workspace_path,
                operation,
                query,
                path,
                line,
            );
            let detail = result
                .lines()
                .next()
                .filter(|line| line.starts_with("semantic_query "))
                .unwrap_or("semantic_query status=REFUSED operation=unknown")
                .to_string();
            (result, "tool_executed", detail, false)
        }
        "edit_file" => {
            let rel = call.arguments["path"].as_str().unwrap_or("");
            if let Some(refused) = scope_refusal(request, rel, "edit_file") {
                return refused;
            }
            let old_str = call.arguments["old_str"].as_str().unwrap_or("");
            let new_str = call.arguments["new_str"].as_str().unwrap_or("");
            match edit_confined(workspace_path, rel, old_str, new_str) {
                Ok(()) => {
                    outcome.files_changed += 1;
                    (
                        format!("edited {rel} - replaced 1 span"),
                        "tool_executed",
                        format!("edit_file {rel}"),
                        false,
                    )
                }
                Err(reason) => (
                    // The model gets the precise reason so it can correct -
                    // not-found or not-unique are recoverable next turn.
                    format!("error: {reason}"),
                    "tool_executed",
                    format!("edit_file refused {rel}"),
                    false,
                ),
            }
        }
        "write_file" => {
            let rel = call.arguments["path"].as_str().unwrap_or("");
            if let Some(refused) = scope_refusal(request, rel, "write_file") {
                return refused;
            }
            let content = call.arguments["content"].as_str().unwrap_or("");
            match write_confined(workspace_path, rel, content) {
                Ok(()) => {
                    outcome.files_changed += 1;
                    (
                        format!("wrote {} ({} bytes)", rel, content.len()),
                        "tool_executed",
                        format!("write_file {rel}"),
                        false,
                    )
                }
                Err(reason) => (
                    format!("error: {reason}"),
                    "tool_executed",
                    format!("write_file refused {rel}"),
                    false,
                ),
            }
        }
        "apply_patch" => {
            let input = call.arguments["input"].as_str().unwrap_or("");
            let ops = match crate::forge_apply_patch::parse_patch(input) {
                Ok(ops) => ops,
                Err(reason) => {
                    // The model gets the precise parse failure so it can
                    // re-emit a well-formed envelope next turn.
                    return (
                        format!("error: {reason}"),
                        "tool_executed",
                        "apply_patch refused malformed envelope".to_string(),
                        false,
                    );
                }
            };
            // The write-scope law holds for every path the patch touches,
            // including move targets, before anything applies.
            for op in &ops {
                for rel in op.touched_paths() {
                    if let Some(refused) = scope_refusal(request, rel, "apply_patch") {
                        return refused;
                    }
                }
            }
            match apply_patch_ops_confined(workspace_path, &ops) {
                Ok(applied) => {
                    outcome.files_changed += applied.len() as u32;
                    (
                        format!("applied patch: {}", applied.join(", ")),
                        "tool_executed",
                        format!("apply_patch {}", applied.join(" ")),
                        false,
                    )
                }
                Err(reason) => (
                    format!("error: {reason}"),
                    "tool_executed",
                    "apply_patch failed to apply".to_string(),
                    false,
                ),
            }
        }
        "run_command" => {
            let command = call.arguments["command"].as_str().unwrap_or("");
            if !bench_open_commands_enabled(request)
                && !request.allowed_commands.iter().any(|c| c == command)
            {
                return (
                    format!(
                        "error: command not in the allowed list. Allowed: {}",
                        request.allowed_commands.join(", ")
                    ),
                    "tool_executed",
                    format!("run_command refused {command}"),
                    false,
                );
            }
            // Execution posture, safest first: scrubbed env + OS
            // confinement is the default on the host (secrets never reach
            // the subprocess, proof commands get no network). Raw host
            // execution is an explicit MDX_FORGE_HOST_CHECKS=1 opt-in,
            // and MDX_FORGE_DISABLE_HOST_CHECKS=1 forces the container
            // path. Posture flips land on the ledger as receipts.
            let mut check = run_selected_check(
                &request.run_id,
                command,
                workspace_path,
                repo_root,
                request.check_target_dir.as_deref(),
            );
            // Auto-escalation on a network denial (opt-in, MDX_FORGE_AUTO_ESCALATE=1,
            // off by default). When a command failed only because the sandbox
            // blocked network, re-run it ONCE with network allowed (the same
            // scrubbed path setup commands use - no new capability, just
            // network). The escalation is named in the receipt detail below.
            let mut escalated = false;
            if !check.0
                && !hosted_check_target(request.check_target_dir.as_deref())
                && auto_escalate_enabled()
                && crate::forge_exec_sandbox::is_network_denial(check.1, &check.3)
            {
                escalated = true;
                check = crate::forge_exec_sandbox::run_scrubbed_confined_for_run(
                    &request.run_id,
                    command,
                    workspace_path,
                    repo_root,
                    request.check_target_dir.as_deref(),
                    true,
                );
            }
            let outcome = Some(check);
            match outcome {
                Some((passed, exit_code, duration_ms, tail)) => {
                    let event = if passed { "check_passed" } else { "check_failed" };
                    // Escalation-on-failure: if a failed command looks like the
                    // SANDBOX blocked it (no network, a write outside the
                    // workspace) rather than the code being wrong, tell the
                    // model what to do instead of leaving it to guess at a raw
                    // error - the difference between a dead end and a one-step
                    // unblock.
                    let hint = if passed {
                        String::new()
                    } else {
                        crate::forge_exec_sandbox::sandbox_denial_hint(exit_code, &tail)
                            .map(|h| format!("\n--- sandbox note ---\n{h}"))
                            .unwrap_or_default()
                    };
                    let escalation_note = if escalated {
                        "\n--- auto-escalated: re-ran with network allowed ---"
                    } else {
                        ""
                    };
                    let text = format!(
                        "exit_code={exit_code} duration_ms={duration_ms} passed={passed}{escalation_note}\n--- output tail ---\n{tail}{hint}"
                    );
                    let escalated_tag = if escalated { " auto_escalated_network" } else { "" };
                    (
                        text,
                        event,
                        format!(
                            "run_command {command} exit={exit_code}{escalated_tag} tail={}",
                            compact_check_tail(&tail)
                        ),
                        passed,
                    )
                }
                None => (
                    "error: command execution is not enabled (local host checks are disabled; set MDX_PLAN_TEST_EXEC=1 with a sandbox image, or remove MDX_FORGE_DISABLE_HOST_CHECKS)".to_string(),
                    "check_failed",
                    format!("run_command unavailable {command}"),
                    false,
                ),
            }
        }
        "shell" => {
            let command = call.arguments["command"].as_str().unwrap_or("");
            if command.trim().is_empty() {
                return (
                    "error: shell needs a command".to_string(),
                    "tool_executed",
                    "shell refused empty".to_string(),
                    false,
                );
            }
            // The policied exploration shell: scrubbed env, network
            // denied, workspace READ-ONLY under OS confinement. It exists
            // so the model can look around (ls, git diff, run one
            // narrowed test into the build cache) without the write-scope
            // law growing a second enforcement point. It never counts as
            // proof - ship eligibility still comes from the allowlisted
            // check commands.
            match crate::forge_exec_sandbox::run_shell_readonly_for_run(
                &request.run_id,
                command,
                workspace_path,
                repo_root,
                local_check_target_dir(request.check_target_dir.as_deref()),
            ) {
                Ok((exit_code, duration_ms, tail)) => (
                    format!(
                        "exit_code={exit_code} duration_ms={duration_ms} (exploration shell: read-only workspace, no network, not a proof)\n--- output tail ---\n{tail}"
                    ),
                    "tool_executed",
                    format!(
                        "shell {command} exit={exit_code} tail={}",
                        compact_check_tail(&tail)
                    ),
                    false,
                ),
                Err(reason) => (
                    format!("error: {reason}"),
                    "tool_executed",
                    format!("shell unavailable {command}"),
                    false,
                ),
            }
        }
        other => (
            format!("error: unknown tool {other}"),
            "tool_executed",
            format!("unknown tool {other}"),
            false,
        ),
    }
}

/// The fleet conflict law at the write site: when a run carries a write
/// scope, an edit outside it is refused with the scope spelled out so
/// the model can correct course instead of fighting the wall.
/// The honest terminal a run must take when it changed files but its commit
/// did not land. `committed` false with `files_changed > 0` means work was
/// produced and then lost (a held branch ref, a failed checkout, a commit
/// error), so the run may not keep a shippable status. `files_changed == 0`
/// leaves an empty commit, which is benign; a successful commit needs no
/// demotion. Returns the demoted status, or None to leave the status as-is.
fn commit_failure_status(files_changed: u32, committed: bool) -> Option<&'static str> {
    if !committed && files_changed > 0 {
        Some("RUN_FINISHED_CANNOT_PROCEED")
    } else {
        None
    }
}

fn scope_refusal(
    request: &ForgeRunRequest,
    rel: &str,
    tool: &str,
) -> Option<(String, &'static str, String, bool)> {
    if request.write_scope.is_empty() || path_in_scope(rel, &request.write_scope) {
        return None;
    }
    Some((
        format!(
            "error: {rel} is outside this stream's write scope. You may only edit or create:\n{}\nIf something must change elsewhere (registrations, exports), declare it in a comment in YOUR files - the integration phase owns the rest.",
            request.write_scope.join("\n")
        ),
        "tool_executed",
        format!("{tool} refused out-of-scope {rel}"),
        false,
    ))
}

/// A path is in scope when it equals a scope entry or sits inside one
/// (prefix at a path boundary) - the same containment rule the plan's
/// disjointness validation uses.
pub(crate) fn path_in_scope(rel: &str, scope: &[String]) -> bool {
    let rel = rel.trim_start_matches("./").trim_end_matches('/');
    scope.iter().any(|entry| {
        let entry = entry.trim_start_matches("./").trim_end_matches('/');
        rel == entry || rel.starts_with(&format!("{entry}/"))
    })
}

// --- Workspace-confined filesystem. Every path resolves inside the
// worktree or it is refused; the worktree boundary is the isolation. ---

fn confined_path(workspace_path: &Path, rel: &str) -> Result<PathBuf, String> {
    // The not-yet-existing tail of a new path cannot be canonicalized, so
    // traversal is rejected lexically up front.
    if rel.split(['/', '\\']).any(|part| part == "..") {
        return Err("path escapes the workspace".to_string());
    }
    let candidate = workspace_path.join(rel);
    let probe = if candidate.exists() {
        candidate.canonicalize().map_err(|e| e.to_string())?
    } else {
        // A new file may sit in a directory that does not exist yet (a
        // whole new route folder, say) - walk up to the nearest EXISTING
        // ancestor, canonicalize that, and re-append the remainder. The
        // fleet's page stream was doomed by the old immediate-parent rule
        // before any model error could even matter.
        let mut existing = candidate.parent().ok_or("no parent")?;
        while !existing.exists() {
            existing = existing.parent().ok_or("no parent")?;
        }
        let canonical = existing.canonicalize().map_err(|e| e.to_string())?;
        let suffix = candidate
            .strip_prefix(existing)
            .map_err(|e| e.to_string())?;
        canonical.join(suffix)
    };
    let root = workspace_path.canonicalize().map_err(|e| e.to_string())?;
    if probe.starts_with(&root) {
        Ok(probe)
    } else {
        Err("path escapes the workspace".to_string())
    }
}

pub(crate) fn read_confined(workspace_path: &Path, rel: &str) -> Result<String, String> {
    let path = confined_path(workspace_path, rel)?;
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    // A big file is shown by line range, never silently cut mid-edit: the
    // model gets the head and an honest pointer to read on, so an edit is
    // always made against text it actually saw.
    let total_lines = content.lines().count();
    if content.len() > MAX_READ_BYTES {
        let head: String = content
            .lines()
            .take(MAX_READ_LINES)
            .collect::<Vec<_>>()
            .join("\n");
        Ok(format!(
            "{head}\n[...{} of {total_lines} lines shown. The file is large; search for a landmark, then edit_file against an exact span you have seen...]",
            MAX_READ_LINES.min(total_lines)
        ))
    } else {
        Ok(content)
    }
}

/// Paths the coder may never WRITE, even inside its workspace: version-control
/// internals and agent/harness config. Editing these is never legitimate task
/// work and can corrupt the run's own history or governance (Codex keeps
/// .git/.codex/.agents read-only in the same spirit). Reads are unaffected.
fn write_protected_reason(rel: &str) -> Option<String> {
    let normalized = rel.replace('\\', "/");
    let first = normalized.trim_start_matches("./");
    const PROTECTED: [&str; 6] = [".git", ".hg", ".svn", ".claude", ".codex", ".mdx"];
    for prefix in PROTECTED {
        if first == prefix || first.starts_with(&format!("{prefix}/")) {
            return Some(format!(
                "`{rel}` is protected: {prefix} holds version-control or agent internals and is never edited by task work. Make your change in the project's own source files."
            ));
        }
    }
    None
}

fn write_confined(workspace_path: &Path, rel: &str, content: &str) -> Result<(), String> {
    if let Some(reason) = write_protected_reason(rel) {
        return Err(reason);
    }
    let path = confined_path(workspace_path, rel)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

/// A surgical edit: replace exactly one occurrence of old_str with
/// new_str. Refuses if old_str is absent or appears more than once - the
/// model must include enough context to make the span unique, the same
/// discipline a precise patch tool enforces. This is what makes edits to
/// large files reliable: no whole-file rewrite, no ambiguity.
fn edit_confined(
    workspace_path: &Path,
    rel: &str,
    old_str: &str,
    new_str: &str,
) -> Result<(), String> {
    if let Some(reason) = write_protected_reason(rel) {
        return Err(reason);
    }
    if old_str.is_empty() {
        return Err("old_str is empty - name the exact text to replace".to_string());
    }
    let path = confined_path(workspace_path, rel)?;
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let count = content.matches(old_str).count();
    if count == 0 {
        return Err(
            "old_str was not found - read the file and copy the exact text, whitespace included"
                .to_string(),
        );
    }
    if count > 1 {
        return Err(format!(
            "old_str appears {count} times - include more surrounding context so it matches exactly once"
        ));
    }
    let edited = content.replacen(old_str, new_str, 1);
    std::fs::write(&path, edited).map_err(|e| e.to_string())
}

/// Apply a parsed V4A envelope inside the workspace, all-or-nothing: every
/// operation is computed (full reads, hunk matching, existence checks)
/// before a single byte is written, so a failing hunk never leaves a
/// half-applied patch. Reads here are FULL file content - the read_file
/// tool's display cap must never corrupt an edit.
fn apply_patch_ops_confined(
    workspace_path: &Path,
    ops: &[crate::forge_apply_patch::PatchOp],
) -> Result<Vec<String>, String> {
    use crate::forge_apply_patch::PatchOp;
    enum Planned {
        Write { rel: String, content: String },
        Delete { rel: String },
    }
    let mut planned: Vec<Planned> = Vec::new();
    let mut applied: Vec<String> = Vec::new();
    // Protected paths are refused before anything applies (the patch is
    // all-or-nothing, so a protected path anywhere in the envelope stops it).
    for op in ops {
        for rel in op.touched_paths() {
            if let Some(reason) = write_protected_reason(rel) {
                return Err(reason);
            }
        }
    }
    for op in ops {
        match op {
            PatchOp::AddFile { path, content } => {
                planned.push(Planned::Write {
                    rel: path.clone(),
                    content: content.clone(),
                });
                applied.push(path.clone());
            }
            PatchOp::DeleteFile { path } => {
                let full = confined_path(workspace_path, path)?;
                if !full.is_file() {
                    return Err(format!(
                        "delete failed: {path} is not a file in the workspace"
                    ));
                }
                planned.push(Planned::Delete { rel: path.clone() });
                applied.push(path.clone());
            }
            PatchOp::UpdateFile {
                path,
                move_to,
                chunks,
            } => {
                let full = confined_path(workspace_path, path)?;
                let original = std::fs::read_to_string(&full).map_err(|_| {
                    format!(
                        "no file to update at path `{path}`. Use an Add File section to create a new file, or fix the path (it must be workspace-relative and already exist)."
                    )
                })?;
                let updated = crate::forge_apply_patch::apply_update(&original, chunks)
                    .map_err(|reason| format!("{path}: {reason}"))?;
                match move_to {
                    Some(target) => {
                        planned.push(Planned::Write {
                            rel: target.clone(),
                            content: updated,
                        });
                        planned.push(Planned::Delete { rel: path.clone() });
                        applied.push(format!("{path} -> {target}"));
                    }
                    None => {
                        planned.push(Planned::Write {
                            rel: path.clone(),
                            content: updated,
                        });
                        applied.push(path.clone());
                    }
                }
            }
        }
    }
    for step in &planned {
        match step {
            Planned::Write { rel, content } => write_confined(workspace_path, rel, content)?,
            Planned::Delete { rel } => {
                let full = confined_path(workspace_path, rel)?;
                std::fs::remove_file(&full)
                    .map_err(|error| format!("delete failed for {rel}: {error}"))?;
            }
        }
    }
    Ok(applied)
}

pub(crate) fn list_confined(workspace_path: &Path, rel: &str) -> Result<String, String> {
    let path = confined_path(workspace_path, rel)?;
    let mut entries: Vec<String> = std::fs::read_dir(&path)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == ".git" {
                return None;
            }
            let suffix = if entry.path().is_dir() { "/" } else { "" };
            Some(format!("{name}{suffix}"))
        })
        .collect();
    entries.sort();
    Ok(entries.join("\n"))
}

pub(crate) fn search_confined(workspace_path: &Path, pattern: &str, prefix: &str) -> String {
    let target = if prefix.is_empty() {
        workspace_path.to_path_buf()
    } else {
        workspace_path.join(prefix)
    };
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(workspace_path)
        .args(["grep", "-n", "--no-color", "-e", pattern, "--"])
        .arg(&target)
        .output();
    match output {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            let capped: String = text.lines().take(60).collect::<Vec<_>>().join("\n");
            if capped.is_empty() {
                "no matches".to_string()
            } else {
                capped
            }
        }
        Err(error) => format!("search error: {error}"),
    }
}

/// Assemble the context a run starts from, drawn from MDx's own memory:
/// the target repo's conventions (so the agent matches the house style)
/// and the work item this run serves (so it knows the why, not just the
/// what). Capped, with a small repo-intelligence contract always present.
/// This is the moat - a generic coding agent has none of this.
const MAX_CONVENTIONS_BYTES: usize = 6000;
const MAX_STANDARD_BYTES: usize = 1800;
const MAX_FLYWHEEL_BYTES: usize = 2400;
const MAX_SEMANTIC_MISSION_BYTES: usize = 12_000;

struct AssembledContext {
    text: String,
    standards: Vec<String>,
    language_pack_id: String,
    quality_signal_count: usize,
    repo_standards_source_count: usize,
    repo_standards_summary_count: usize,
    review_axis_count: usize,
    principal_review_gate_count: usize,
    semantic_anchor_count: usize,
    semantic_preflight: SemanticMissionPack,
    semantic_tool_readiness_count: usize,
    toolchain_readiness_count: usize,
    outcome_signal_count: usize,
    active_memory_count: usize,
    matched_memory_ids: Vec<String>,
    passed_over_memory_ids: Vec<String>,
    installed_capability_count: usize,
    distilled_skill_count: usize,
    cited_distilled_skill_ids: Vec<String>,
    flywheel_context_suppressed: bool,
}

/// The two flywheel injection blocks (recent Forge outcome signals and
/// relevance-matched active learning memory) ride into every Forge run by
/// default. The memory-on/off ablation harness sets
/// MDX_FORGE_FLYWHEEL_CONTEXT=off to run the memory-off baseline arm; the off
/// arm withholds both blocks and the run emits a receipted suppression event
/// so no arm is ever measured silently. Env gating follows the same idiom as
/// MDX_FORGE_HOST_CHECKS and MDX_PLAN_TEST_EXEC in this file.
fn flywheel_context_enabled() -> bool {
    !matches!(
        std::env::var("MDX_FORGE_FLYWHEEL_CONTEXT")
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "off" | "0" | "false"
    )
}

fn assemble_context(
    workspace_path: &Path,
    request: &ForgeRunRequest,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> AssembledContext {
    let mut blocks: Vec<String> = Vec::new();
    let mut standards: Vec<String> = Vec::new();
    let mut language_pack_id = String::new();
    let mut outcome_signal_count = 0usize;
    let mut active_memory_count = 0usize;
    let mut matched_memory_ids: Vec<String> = Vec::new();
    let mut passed_over_memory_ids: Vec<String> = Vec::new();
    let mut installed_capability_count = 0usize;
    let mut distilled_skill_count = 0usize;
    let mut cited_distilled_skill_ids: Vec<String> = Vec::new();
    let flywheel_context_enabled = flywheel_context_enabled();

    let repo_profile = crate::forge_repo_profile::profile_repo_for_task(
        workspace_path,
        &request.intent,
        &request.write_scope,
        &request.allowed_commands,
    );
    blocks.push(repo_intelligence_contract(&repo_profile, request));
    blocks.push(execution_geometry_contract(request));
    if let Some(policy) = structured_semantic_policy_contract(request) {
        blocks.push(policy);
    }
    let semantic_mission = semantic_mission_pack(workspace_path, request, &repo_profile);
    if !semantic_mission.text.is_empty() {
        blocks.push(semantic_mission.text.clone());
    }
    if let Some(guidance) = language_pack_guidance(repo_profile.language_pack_id) {
        language_pack_id = repo_profile.language_pack_id.to_string();
        blocks.push(format!(
            "Language pack guidance ({}) - use this to plan and review the work:\n{}",
            repo_profile.language_pack_id, guidance
        ));
    }
    if repo_profile.detected_language_packs.len() > 1 {
        let packs = repo_profile
            .detected_language_packs
            .iter()
            .map(|pack| format!("- {pack}"))
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(format!(
            "Mixed-stack repo evidence detected - primary defaults come from {}, but review for neighboring pack conventions too:\n{packs}\nThis evidence does not widen scope, choose extra checks, or grant authority.",
            repo_profile.language_pack_id
        ));
    }
    let quality_signal_count = repo_profile.quality_signals.len();
    if !repo_profile.quality_signals.is_empty() {
        let signals = repo_profile
            .quality_signals
            .iter()
            .map(|signal| format!("- {signal}"))
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(format!(
            "Repo quality signals detected - use these as planning and review hints, not proof:\n{signals}\nThey do not let you skip checks, widen scope, or mark the run passed."
        ));
    }
    let repo_standards_source_count = repo_profile.standards_sources.len();
    if !repo_profile.standards_sources.is_empty() {
        let sources = repo_profile
            .standards_sources
            .iter()
            .map(|source| format!("- {source}"))
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(format!(
            "Repo standards sources detected - consult these before changing behavior or public surfaces:\n{sources}\nThese sources guide planning and review, but they do not grant authority or mark checks passed."
        ));
    }
    let repo_standards_summary_count = repo_profile.standards_source_summaries.len();
    if !repo_profile.standards_source_summaries.is_empty() {
        let summaries = repo_profile
            .standards_source_summaries
            .iter()
            .map(|summary| format!("- {summary}"))
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(format!(
            "Repo standards summaries detected - use these compact categories to decide what to read next, not as a substitute for reading the relevant source:\n{summaries}\nThey summarize allowlisted standards files only and do not grant authority, replace checks, or prove compliance."
        ));
    }
    let mut review_axis_count = 0usize;
    if repo_profile.language_pack_id != "generic" && !repo_profile.review_axes.is_empty() {
        review_axis_count = repo_profile.review_axes.len();
        let axes = repo_profile
            .review_axes
            .iter()
            .map(|axis| format!("- {axis}"))
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(format!(
            "Principal engineer review axes for this language pack - use these to plan and self-review before handing off:\n{axes}\nThese axes do not replace the user's checks, policy, or human review."
        ));
    }
    let mut principal_review_gate_count = 0usize;
    if repo_profile.language_pack_id != "generic" && !repo_profile.principal_review_gates.is_empty()
    {
        principal_review_gate_count = repo_profile.principal_review_gates.len();
        let gates = repo_profile
            .principal_review_gates
            .iter()
            .map(|gate| format!("- {gate}"))
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(format!(
            "Principal review gates for this run - plan and self-review against each one before handoff:\n{gates}\nThese gates are evidence guidance only; they do not mark checks passed or grant ship authority."
        ));
    }
    let mut semantic_tool_readiness_count = 0usize;
    if repo_profile.language_pack_id != "generic" && !repo_profile.semantic_intelligence.is_empty()
    {
        semantic_tool_readiness_count = repo_profile.semantic_tool_readiness.len();
        let semantic_plan = repo_profile
            .semantic_intelligence
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n");
        let readiness = repo_profile
            .semantic_tool_readiness
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(format!(
            "Semantic intelligence for this language pack - prefer language-server facts for navigation and self-review when available:\n{semantic_plan}\nSemantic tool readiness:\n{readiness}\nMissing semantic readiness means fall back to anchored search and file-tree evidence. It does not let you skip checks, fabricate proof, start servers outside the tool policy, or mark the run passed."
        ));
    }
    let mut toolchain_readiness_count = 0usize;
    if repo_profile.language_pack_id != "generic" && !repo_profile.toolchain_readiness.is_empty() {
        toolchain_readiness_count = repo_profile.toolchain_readiness.len();
        let readiness = repo_profile
            .toolchain_readiness
            .iter()
            .map(|item| format!("- {item}"))
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(format!(
            "Local toolchain readiness for this language pack - use this to decide whether local proof can run before handoff:\n{readiness}\nMissing readiness is a setup warning only. It does not let you skip checks, fabricate proof, install tools, or mark the run passed."
        ));
    }

    // The repo's own conventions: AGENTS.md is the agent contract; CLAUDE.md
    // and README are fallbacks. Read from the workspace, capped.
    for name in ["AGENTS.md", "CLAUDE.md"] {
        let path = workspace_path.join(name);
        if let Ok(text) = std::fs::read_to_string(&path) {
            let capped: String = text.chars().take(MAX_CONVENTIONS_BYTES).collect();
            blocks.push(format!(
                "This repository's conventions ({name}) - follow them:\n{capped}"
            ));
            break;
        }
    }

    // The work item this run serves AND the bet behind it - the strategic
    // why, walked up the work-plane graph from MDx's memory. Folded to the
    // latest shape of each.
    if !request.work_item_id.trim().is_empty()
        && let Ok(kernel) = kernel.read()
    {
        let mut title = String::new();
        let mut description = String::new();
        let mut bet_id = String::new();
        for receipt in kernel.ledger().query().by_kind("work.item.shaped").iter() {
            if receipt.payload.get("work_item_id").map(String::as_str)
                == Some(request.work_item_id.trim())
            {
                title = receipt.payload.get("title").cloned().unwrap_or_default();
                description = receipt
                    .payload
                    .get("description")
                    .cloned()
                    .unwrap_or_default();
                bet_id = receipt.payload.get("bet_id").cloned().unwrap_or_default();
            }
        }
        if !title.is_empty() {
            let mut block = format!("The work item this serves: {title}");
            if !description.is_empty() {
                block.push_str(&format!("\n{description}"));
            }
            blocks.push(block);
        }
        // Walk up to the bet: what we are building and why, so the agent
        // knows the intent behind the task, not just the task.
        if !bet_id.is_empty() {
            let mut bet = String::new();
            let mut for_whom = String::new();
            let mut success = String::new();
            for receipt in kernel.ledger().query().by_kind("product.bet.shaped").iter() {
                if receipt.payload.get("bet_id").map(String::as_str) == Some(bet_id.as_str()) {
                    bet = receipt.payload.get("bet").cloned().unwrap_or_default();
                    for_whom = receipt.payload.get("for_whom").cloned().unwrap_or_default();
                    success = receipt
                        .payload
                        .get("success_metric")
                        .cloned()
                        .unwrap_or_default();
                }
            }
            if !bet.is_empty() {
                let mut block = format!("The bet this work serves (the why): {bet}");
                if !for_whom.is_empty() {
                    block.push_str(&format!("\nFor: {for_whom}"));
                }
                if !success.is_empty() {
                    block.push_str(&format!("\nSuccess looks like: {success}"));
                }
                blocks.push(block);
            }
        }
    }

    // The standards this build must follow: the engineering guidelines
    // published in Pages (titled "Standard: ..."). They ride into the agent's
    // own context so it builds to your rules, not generic instinct - and the
    // titles come back so the run's trail can cite them. Latest revision per
    // document wins.
    if let Ok(kernel) = kernel.read() {
        let mut seen: Vec<String> = Vec::new();
        for receipt in kernel
            .ledger()
            .query()
            .by_kind("pages.document.published")
            .iter()
        {
            let title = receipt.payload.get("title").cloned().unwrap_or_default();
            if !title.trim().to_lowercase().starts_with("standard") {
                continue;
            }
            let document_id = receipt
                .payload
                .get("document_id")
                .cloned()
                .unwrap_or_default();
            if seen.contains(&document_id) {
                continue;
            }
            seen.push(document_id);
            let name = title
                .trim()
                .trim_start_matches("Standard:")
                .trim_start_matches("standard:")
                .trim()
                .to_string();
            let body = receipt
                .payload
                .get("body_ref")
                .and_then(|body_ref| crate::pages_body_store::read_stored_body(body_ref))
                .unwrap_or_default();
            let capped: String = body.chars().take(MAX_STANDARD_BYTES).collect();
            blocks.push(format!(
                "A standard this build MUST follow - {name}:\n{capped}"
            ));
            standards.push(name);
        }
    }

    if let Ok(kernel) = kernel.read() {
        // The memory-on arm: both flywheel blocks ride in unless the ablation
        // harness set MDX_FORGE_FLYWHEEL_CONTEXT=off (the memory-off baseline).
        // When suppressed, the counts stay zero and run_forge_loop emits a
        // receipted suppression event so the off arm is never measured silently.
        // The Marketplace block below is not part of the flywheel ablation.
        if flywheel_context_enabled {
            let outcome_lines: Vec<String> = kernel
                .ledger()
                .query()
                .by_kind("forge.outcome.signal.recorded")
                .iter()
                .rev()
                .take(3)
                .map(|receipt| {
                    format!(
                        "- {}: {}. Lesson candidate: {}",
                        payload_value(receipt, "disposition"),
                        payload_value(receipt, "summary"),
                        payload_value(receipt, "lesson_candidate")
                    )
                })
                .collect();
            outcome_signal_count = outcome_lines.len();
            if !outcome_lines.is_empty() {
                blocks.push(capped_flywheel_block(
                    "Recent Forge outcomes to consider before planning",
                    &outcome_lines.join("\n"),
                ));
            }

            // Relevance-first lesson read-back (selection_basis relevance_v1):
            // every active, non-superseded lesson is scored against this run
            // instead of the latest five riding in globally, so a UI lesson
            // stops boarding backend runs as the lane fills. Both the matched
            // and the passed-over ids ride up so the citation receipt stays
            // honest.
            let selected_memory =
                select_active_learning_memory(&kernel, request, repo_profile.language_pack_id);
            active_memory_count = selected_memory.lines.len();
            matched_memory_ids = selected_memory.matched_memory_ids;
            passed_over_memory_ids = selected_memory.passed_over_memory_ids;
            if !selected_memory.lines.is_empty() {
                blocks.push(capped_flywheel_block(
                    "Active MDx learning memory, advisory only",
                    &format!(
                        "{}\nUse these as citations and judgment context. They do not let you skip checks, expand scope, or open execution authority.",
                        selected_memory.lines.join("\n")
                    ),
                ));
            }

            // Procedural memory read-back: a ratified distilled skill whose
            // applicability matches this run injects its procedure as an
            // available, advisory procedure. This is the Marketplace skill
            // library reuse seam - the Voyager skill-library-add, cited like a
            // lesson. Each skill's provenance chain (source run -> draft ->
            // ratification) is already on the ledger.
            let selected_skills =
                select_ratified_distilled_skills(&kernel, request, repo_profile.language_pack_id);
            distilled_skill_count = selected_skills.lines.len();
            cited_distilled_skill_ids = selected_skills.matched_skill_ids;
            if !selected_skills.lines.is_empty() {
                blocks.push(capped_flywheel_block(
                    "Distilled skills your team proved, advisory only",
                    &format!(
                        "{}\nEach step comes from a run that passed its checks. Follow the procedure as guidance - it does not let you skip checks, expand scope, or open execution authority.",
                        selected_skills.lines.join("\n")
                    ),
                ));
            }
        }

        let installed_lines = marketplace_installed_capability_lines(&kernel);
        installed_capability_count = installed_lines.len();
        if !installed_lines.is_empty() {
            blocks.push(capped_flywheel_block(
                "Installed Marketplace capabilities, advisory only",
                &format!(
                    "{}\nThese installs do not grant secrets, inherited agent permissions, production writes, or untrusted capability execution.",
                    installed_lines.join("\n")
                ),
            ));
        }
    }

    let text = if blocks.is_empty() {
        String::new()
    } else {
        format!(
            "--- Context from MDx ---\n{}\n--- end context ---\n\n",
            blocks.join("\n\n")
        )
    };
    AssembledContext {
        text,
        standards,
        language_pack_id,
        quality_signal_count,
        repo_standards_source_count,
        repo_standards_summary_count,
        review_axis_count,
        principal_review_gate_count,
        semantic_anchor_count: semantic_mission.anchor_count,
        semantic_preflight: semantic_mission,
        semantic_tool_readiness_count,
        toolchain_readiness_count,
        outcome_signal_count,
        active_memory_count,
        matched_memory_ids,
        passed_over_memory_ids,
        installed_capability_count,
        distilled_skill_count,
        cited_distilled_skill_ids,
        flywheel_context_suppressed: !flywheel_context_enabled,
    }
}

struct SemanticMissionPack {
    text: String,
    anchor_count: usize,
    planned_operations: Vec<String>,
    observed_operations: Vec<String>,
    session_id: String,
    indexed_file_count: usize,
    indexed_symbol_count: usize,
    related_test_anchor_count: usize,
}

fn semantic_mission_pack(
    workspace_path: &Path,
    request: &ForgeRunRequest,
    repo_profile: &crate::forge_repo_profile::ForgeRepoProfile,
) -> SemanticMissionPack {
    if repo_profile.language_pack_id == "generic" && repo_profile.semantic_intelligence.is_empty() {
        return SemanticMissionPack {
            text: String::new(),
            anchor_count: 0,
            planned_operations: Vec::new(),
            observed_operations: Vec::new(),
            session_id: String::new(),
            indexed_file_count: 0,
            indexed_symbol_count: 0,
            related_test_anchor_count: 0,
        };
    }

    let artifact_patterns = repo_profile
        .artifact_patterns
        .iter()
        .map(|value| (*value).to_string())
        .collect::<Vec<_>>();
    let scout = crate::forge_repo_task_scout_route::scout_repo(
        workspace_path,
        &request.allowed_commands,
        &artifact_patterns,
    );
    let mut lines = vec![
        "Semantic mission pack - concrete read-only anchors to inspect before editing:".to_string(),
        format!(
            "- Language pack: {}; primary language: {}",
            repo_profile.language_pack_id, repo_profile.primary_language
        ),
        "- Use these anchors to plan. They do not widen scope, mark checks passed, or grant authority.".to_string(),
    ];
    let intent_symbols = intent_symbol_queries(&request.intent)
        .into_iter()
        .take(5)
        .collect::<Vec<_>>();
    let mut planned_queries = intent_symbols
        .iter()
        .map(|symbol| {
            format!("Preflight semantic_query operation=symbol_graph path=none query={symbol}")
        })
        .collect::<Vec<_>>();
    for candidate in scout.candidates.iter().take(3) {
        let path = candidate["path"].as_str().unwrap_or("");
        for query in candidate["semantic_orientation_queries"]
            .as_array()
            .into_iter()
            .flatten()
            .take(5)
        {
            let operation = query["operation"].as_str().unwrap_or("capabilities");
            let query_text = query["query"].as_str().unwrap_or("");
            let query_path = query["path"].as_str().unwrap_or("");
            planned_queries.push(format!(
                "Preflight semantic_query operation={} path={} query={}",
                operation,
                if query_path.is_empty() {
                    "none"
                } else {
                    query_path
                },
                if query_text.is_empty() {
                    "none"
                } else {
                    query_text
                }
            ));
        }
        if !path.is_empty() {
            planned_queries.extend([
                format!("Preflight semantic_query operation=symbol_graph path={path} query=none"),
                format!("Preflight semantic_query operation=hover path={path} line=1 query=none"),
                format!("Preflight semantic_query operation=dependency_map path={path} query=none"),
                format!("Preflight semantic_query operation=diagnostics path={path} query=none"),
            ]);
        }
    }
    planned_queries.sort();
    planned_queries.dedup();
    let planned_operations = semantic_operations_from_planned_queries(&planned_queries);
    if !planned_queries.is_empty() {
        lines.push("Planned semantic query anchors for this run:".to_string());
        lines.extend(planned_queries.iter().map(|value| format!("- {value}")));
    }

    let mut anchor_count = 0usize;
    let mut observed_operations: Vec<String> = Vec::new();
    let mut session_id = String::new();
    let mut indexed_file_count = 0usize;
    let mut indexed_symbol_count = 0usize;
    let mut related_test_anchor_count = 0usize;

    for symbol in intent_symbols {
        lines.push(format!(
            "Intent semantic orientation: symbol_graph query={symbol}"
        ));
        let result = crate::forge_semantic_query_route::semantic_query_tool_text(
            workspace_path,
            "symbol_graph",
            &symbol,
            "",
            0,
        );
        capture_semantic_preflight_result(
            &result,
            &mut observed_operations,
            &mut session_id,
            &mut indexed_file_count,
            &mut indexed_symbol_count,
            &mut related_test_anchor_count,
        );
        lines.extend(capped_semantic_query_lines(&result));
        anchor_count += 1;
    }

    if !scout.candidates.is_empty() {
        lines.push(format!(
            "- Deterministic first-run candidates found: {}",
            scout.candidates.len()
        ));
        for (index, candidate) in scout.candidates.iter().take(3).enumerate() {
            let kind = candidate["kind"].as_str().unwrap_or("candidate");
            let path = candidate["path"].as_str().unwrap_or("");
            let why = candidate["why_this_first"].as_str().unwrap_or("");
            lines.push(format!(
                "Candidate {}: kind={} path={} why={}",
                index + 1,
                kind,
                if path.is_empty() { "none" } else { path },
                if why.is_empty() { "repo evidence" } else { why }
            ));
            for query in candidate["semantic_orientation_queries"]
                .as_array()
                .into_iter()
                .flatten()
                .take(5)
            {
                let operation = query["operation"].as_str().unwrap_or("capabilities");
                let query_text = query["query"].as_str().unwrap_or("");
                let query_path = query["path"].as_str().unwrap_or("");
                let line = query["line"].as_u64().unwrap_or(0) as usize;
                lines.push(format!(
                    "Preflight semantic_query operation={} path={} query={}",
                    operation,
                    if query_path.is_empty() {
                        "none"
                    } else {
                        query_path
                    },
                    if query_text.is_empty() {
                        "none"
                    } else {
                        query_text
                    }
                ));
                let result = crate::forge_semantic_query_route::semantic_query_tool_text(
                    workspace_path,
                    operation,
                    query_text,
                    query_path,
                    line,
                );
                capture_semantic_preflight_result(
                    &result,
                    &mut observed_operations,
                    &mut session_id,
                    &mut indexed_file_count,
                    &mut indexed_symbol_count,
                    &mut related_test_anchor_count,
                );
                lines.extend(capped_semantic_query_lines(&result));
                anchor_count += 1;
            }
            if !path.is_empty() {
                lines.push(format!(
                    "Preflight semantic_query operation=symbol_graph path={} query=none",
                    path
                ));
                let result = crate::forge_semantic_query_route::semantic_query_tool_text(
                    workspace_path,
                    "symbol_graph",
                    "",
                    path,
                    0,
                );
                capture_semantic_preflight_result(
                    &result,
                    &mut observed_operations,
                    &mut session_id,
                    &mut indexed_file_count,
                    &mut indexed_symbol_count,
                    &mut related_test_anchor_count,
                );
                lines.extend(capped_semantic_query_lines(&result));
                anchor_count += 1;
                lines.push(format!(
                    "Preflight semantic_query operation=hover path={} line=1 query=none",
                    path
                ));
                let result = crate::forge_semantic_query_route::semantic_query_tool_text(
                    workspace_path,
                    "hover",
                    "",
                    path,
                    1,
                );
                capture_semantic_preflight_result(
                    &result,
                    &mut observed_operations,
                    &mut session_id,
                    &mut indexed_file_count,
                    &mut indexed_symbol_count,
                    &mut related_test_anchor_count,
                );
                lines.extend(capped_semantic_query_lines(&result));
                anchor_count += 1;
                lines.push(format!(
                    "Preflight semantic_query operation=dependency_map path={} query=none",
                    path
                ));
                let result = crate::forge_semantic_query_route::semantic_query_tool_text(
                    workspace_path,
                    "dependency_map",
                    "",
                    path,
                    0,
                );
                capture_semantic_preflight_result(
                    &result,
                    &mut observed_operations,
                    &mut session_id,
                    &mut indexed_file_count,
                    &mut indexed_symbol_count,
                    &mut related_test_anchor_count,
                );
                lines.extend(capped_semantic_query_lines(&result));
                anchor_count += 1;
                lines.push(format!(
                    "Preflight semantic_query operation=diagnostics path={} query=none",
                    path
                ));
                let result = crate::forge_semantic_query_route::semantic_query_tool_text(
                    workspace_path,
                    "diagnostics",
                    "",
                    path,
                    0,
                );
                capture_semantic_preflight_result(
                    &result,
                    &mut observed_operations,
                    &mut session_id,
                    &mut indexed_file_count,
                    &mut indexed_symbol_count,
                    &mut related_test_anchor_count,
                );
                lines.extend(capped_semantic_query_lines(&result));
                anchor_count += 1;
            }
        }
    }

    if anchor_count == 0 {
        lines.push(
            "Preflight semantic_query operation=capabilities path=none query=none".to_string(),
        );
        let result = crate::forge_semantic_query_route::semantic_query_tool_text(
            workspace_path,
            "capabilities",
            "",
            "",
            0,
        );
        capture_semantic_preflight_result(
            &result,
            &mut observed_operations,
            &mut session_id,
            &mut indexed_file_count,
            &mut indexed_symbol_count,
            &mut related_test_anchor_count,
        );
        lines.extend(capped_semantic_query_lines(&result));
        anchor_count = 1;
    }
    if anchor_count == 1 && !repo_profile.semantic_intelligence.is_empty() {
        lines.push(
            "Preflight semantic_query operation=diagnostics path=auto query=none".to_string(),
        );
        let result = crate::forge_semantic_query_route::semantic_query_tool_text(
            workspace_path,
            "diagnostics",
            "",
            "",
            0,
        );
        capture_semantic_preflight_result(
            &result,
            &mut observed_operations,
            &mut session_id,
            &mut indexed_file_count,
            &mut indexed_symbol_count,
            &mut related_test_anchor_count,
        );
        lines.extend(capped_semantic_query_lines(&result));
        anchor_count += 1;
    }

    let text = lines.join("\n");
    SemanticMissionPack {
        text: cap_chars(&text, MAX_SEMANTIC_MISSION_BYTES),
        anchor_count,
        planned_operations,
        observed_operations,
        session_id,
        indexed_file_count,
        indexed_symbol_count,
        related_test_anchor_count,
    }
}

fn semantic_operations_from_planned_queries(planned_queries: &[String]) -> Vec<String> {
    let mut operations = Vec::new();
    for query in planned_queries {
        let Some(operation) = query
            .split_whitespace()
            .find_map(|token| token.strip_prefix("operation="))
        else {
            continue;
        };
        if !operations.iter().any(|seen| seen == operation) {
            operations.push(operation.to_string());
        }
    }
    operations
}

fn capture_semantic_preflight_result(
    result: &str,
    observed_operations: &mut Vec<String>,
    session_id: &mut String,
    indexed_file_count: &mut usize,
    indexed_symbol_count: &mut usize,
    related_test_anchor_count: &mut usize,
) {
    if semantic_query_succeeded(result)
        && let Some(operation) = semantic_query_operation(result)
        && !observed_operations.iter().any(|seen| seen == &operation)
    {
        observed_operations.push(operation);
    }
    for line in result.lines() {
        if line.contains("semantic_session_id=") {
            for token in line.split_whitespace() {
                if let Some(value) = token.strip_prefix("semantic_session_id=")
                    && session_id.trim().is_empty()
                {
                    *session_id = value.to_string();
                }
                if let Some(value) = token.strip_prefix("indexed_file_count=") {
                    *indexed_file_count = value.parse().unwrap_or(0);
                }
                if let Some(value) = token.strip_prefix("indexed_symbol_count=") {
                    *indexed_symbol_count = value.parse().unwrap_or(0);
                }
                if let Some(value) = token.strip_prefix("related_test_anchor_count=") {
                    *related_test_anchor_count = value.parse().unwrap_or(0);
                }
            }
        }
    }
}

fn capped_semantic_query_lines(text: &str) -> Vec<String> {
    text.lines()
        .take(7)
        .map(|line| format!("  {line}"))
        .collect()
}

fn intent_symbol_queries(intent: &str) -> Vec<String> {
    let stopwords = [
        "add", "fix", "make", "the", "and", "for", "with", "that", "this", "from", "into", "when",
        "then", "should", "would", "could", "repo", "test", "tests", "file", "files", "code",
        "work", "build", "run", "runs", "using", "without", "inside",
    ];
    let mut seen = std::collections::BTreeSet::new();
    let mut symbols = Vec::new();
    for token in intent
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '.'))
        .map(|token| token.trim_matches(|ch: char| ch == '.' || ch == '-' || ch == '_'))
        .filter(|token| token.len() >= 4)
    {
        let lower = token.to_ascii_lowercase();
        if stopwords.contains(&lower.as_str()) {
            continue;
        }
        let looks_symbolic = token.chars().any(|ch| ch.is_ascii_uppercase())
            || token.contains('_')
            || token.contains('-')
            || token.contains('.')
            || token.len() >= 7;
        if looks_symbolic && seen.insert(lower) {
            symbols.push(token.to_string());
        }
        if symbols.len() >= 8 {
            break;
        }
    }
    symbols
}

fn cap_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut capped = text.chars().take(max_chars).collect::<String>();
    capped.push_str("\n... semantic mission pack capped ...");
    capped
}

fn repo_intelligence_contract(
    repo_profile: &crate::forge_repo_profile::ForgeRepoProfile,
    request: &ForgeRunRequest,
) -> String {
    let detected_packs = if repo_profile.detected_language_packs.is_empty() {
        "none".to_string()
    } else {
        repo_profile.detected_language_packs.join(", ")
    };
    let detected_files = if repo_profile.detected_files.is_empty() {
        "none".to_string()
    } else {
        repo_profile.detected_files.join(", ")
    };
    let selected_checks = if request.allowed_commands.is_empty() {
        "none recorded".to_string()
    } else {
        request.allowed_commands.join(" | ")
    };
    let selected_setup_commands = command_phase_list(&request.allowed_commands, true);
    let selected_proof_commands = command_phase_list(&request.allowed_commands, false);
    let suggested_checks = if repo_profile.suggested_checks.is_empty() {
        "none inferred".to_string()
    } else {
        repo_profile.suggested_checks.join(" | ")
    };
    let inferred_setup_commands = repo_profile
        .suggested_checks
        .iter()
        .filter(|check| crate::forge_repo_profile::is_setup_command(check))
        .cloned()
        .collect::<Vec<_>>();
    let inferred_proof_commands = repo_profile
        .suggested_checks
        .iter()
        .filter(|check| !crate::forge_repo_profile::is_setup_command(check))
        .cloned()
        .collect::<Vec<_>>();
    let inferred_setup_commands = if inferred_setup_commands.is_empty() {
        "none inferred".to_string()
    } else {
        inferred_setup_commands.join(" | ")
    };
    let inferred_proof_commands = if inferred_proof_commands.is_empty() {
        "none inferred".to_string()
    } else {
        inferred_proof_commands.join(" | ")
    };
    let artifact_patterns = if repo_profile.artifact_patterns.is_empty() {
        "none inferred".to_string()
    } else {
        repo_profile.artifact_patterns.join(", ")
    };
    let semantic_plan = if repo_profile.semantic_intelligence.is_empty() {
        "none inferred".to_string()
    } else {
        repo_profile.semantic_intelligence.join(", ")
    };
    let semantic_readiness = if repo_profile.semantic_tool_readiness.is_empty() {
        "none recorded".to_string()
    } else {
        repo_profile.semantic_tool_readiness.join(", ")
    };
    let standards_summaries = if repo_profile.standards_source_summaries.is_empty() {
        "none recorded".to_string()
    } else {
        repo_profile.standards_source_summaries.join(", ")
    };
    format!(
        "Repo intelligence contract - use this before planning, editing, and finishing:\n\
- Primary language: {}\n\
- Language pack: {}\n\
- Detected language packs: {detected_packs}\n\
- Detected stack files: {detected_files}\n\
- Selected command sequence: {selected_checks}\n\
- Selected setup commands: {selected_setup_commands}\n\
- Selected proof commands: {selected_proof_commands}\n\
- Inferred command sequence: {suggested_checks}\n\
- Inferred setup commands: {inferred_setup_commands}\n\
- Inferred proof commands: {inferred_proof_commands}\n\
- Semantic intelligence: {semantic_plan}\n\
- Semantic tool readiness: {semantic_readiness}\n\
- Standards source summaries: {standards_summaries}\n\
- Proof plan: status={}, next_action={}, summary={}\n\
- Artifact noise to ignore in review: {artifact_patterns}\n\
This contract is advisory context. It does not widen write scope, add runnable commands, grant secrets, grant production authority, or mark work passed. Only selected proof commands that actually pass can support finish.",
        repo_profile.primary_language,
        repo_profile.language_pack_id,
        repo_profile.proof_plan_status,
        repo_profile.proof_plan_next_action,
        repo_profile.proof_plan_summary,
    )
}

fn formatted_allowed_commands(commands: &[String], request: &ForgeRunRequest) -> String {
    if bench_open_commands_enabled(request) {
        let listed = if commands.is_empty() {
            String::new()
        } else {
            let suggested = commands
                .iter()
                .map(|command| format!("- {command}"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("Suggested checks:\n{suggested}\n")
        };
        return format!(
            "{listed}- open command posture: this calibration run may run ANY command through run_command. Install/setup commands get network; proof commands run without it. Files your commands create in the workspace count as deliverables. When the task's outputs are in place and verified, call finish with outcome done."
        );
    }
    if commands.is_empty() {
        return "- none selected".to_string();
    }
    commands
        .iter()
        .map(|command| {
            let phase = if crate::forge_repo_profile::is_setup_command(command) {
                "setup"
            } else {
                "proof"
            };
            format!("- [{phase}] {command}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn command_phase_list(commands: &[String], setup: bool) -> String {
    let commands = commands
        .iter()
        .filter(|command| crate::forge_repo_profile::is_setup_command(command) == setup)
        .map(String::as_str)
        .collect::<Vec<_>>();
    if commands.is_empty() {
        if setup {
            "none selected".to_string()
        } else {
            "none recorded".to_string()
        }
    } else {
        commands.join(" | ")
    }
}

fn setup_order_refusal(
    command: &str,
    allowed_commands: &[String],
    passed_setup_commands: &HashSet<String>,
) -> Option<String> {
    let command = command.trim();
    if command.is_empty() || crate::forge_repo_profile::is_setup_command(command) {
        return None;
    }
    let command_index = allowed_commands
        .iter()
        .position(|allowed| allowed.trim() == command)?;
    let missing_setup = allowed_commands[..command_index]
        .iter()
        .filter(|allowed| crate::forge_repo_profile::is_setup_command(allowed))
        .filter(|allowed| !passed_setup_commands.contains(allowed.trim()))
        .map(|allowed| allowed.trim().to_string())
        .collect::<Vec<_>>();
    if missing_setup.is_empty() {
        None
    } else {
        Some(format!(
            "setup command(s) must pass before proof command `{command}`: {}",
            missing_setup.join(", ")
        ))
    }
}

fn finish_proof_refusal(
    request: &ForgeRunRequest,
    outcome: &ForgeRunOutcome,
    finish_outcome: &str,
) -> Option<String> {
    if finish_outcome != "done" || outcome.files_changed == 0 {
        return None;
    }
    let proof_commands = request
        .allowed_commands
        .iter()
        .map(String::as_str)
        .filter(|command| !crate::forge_repo_profile::is_setup_command(command))
        .collect::<Vec<_>>();
    if proof_commands.is_empty() || outcome.last_check_passed {
        return None;
    }
    Some(format!(
        "cannot finish as done yet: files changed, but no selected proof command has passed after the latest edit. Run one of the selected proof commands first: {}",
        proof_commands.join(", ")
    ))
}

fn execution_geometry_review_rule(request: &ForgeRunRequest) -> String {
    if request.execution_geometry_lane == "bounded_parallel_exploration"
        && request.execution_geometry_effective_workers > 1
    {
        "Parallel candidate review rule: Review ranks candidates with receipt facts, behavioral proof, proof scope, semantic orientation, dependency maps, check outcomes, and deterministic diff quality. A candidate that only reports done or passes setup without behavior proof should expect to lose.".to_string()
    } else {
        "Review rule: make the change easy to audit with concrete proof, semantic orientation, and a focused diff.".to_string()
    }
}

fn execution_geometry_contract(request: &ForgeRunRequest) -> String {
    let lane = if request.execution_geometry_lane.trim().is_empty() {
        "single_worker"
    } else {
        request.execution_geometry_lane.as_str()
    };
    let route = if request.execution_geometry_route.trim().is_empty() {
        "/forge/runs.json"
    } else {
        request.execution_geometry_route.as_str()
    };
    let role = match lane {
        "fleet_stream" => {
            "You are one bounded stream in a wider fleet. Own only your write scope, assume neighboring streams are active, and leave integration to the fleet integrator."
        }
        "fleet_integration" => {
            "You are the fleet integration worker. Read stream outputs, wire only integration-owned paths, and prove the whole without redoing stream work."
        }
        "bounded_parallel_exploration" => {
            "This direct run is one bounded parallel candidate. Follow your assigned candidate strategy, solve independently from neighboring candidates, and make the result easy to compare in review."
        }
        _ => {
            "You are a single isolated worker. Keep the change focused, prove it with selected checks, and do not assume other workers are handling neighboring work."
        }
    };
    let strategy_line = candidate_strategy_line(&request.intent);
    let review_rule = execution_geometry_review_rule(request);
    format!(
        "Execution geometry contract - use this to plan the amount of work and your coordination posture:\n\
- Requested workers: {}\n\
- Effective workers for this loop: {}\n\
- Lane: {lane}\n\
- Admission route: {route}\n\
- Worker role: {role}\n\
- Candidate strategy: {strategy_line}\n\
- Candidate comparison: {review_rule}\n\
This geometry does not widen write scope, grant extra commands, grant secrets, mark checks passed, or open production authority.",
        request.execution_geometry_requested_workers.max(1),
        request.execution_geometry_effective_workers.max(1),
    )
}

fn structured_semantic_policy_contract(request: &ForgeRunRequest) -> Option<String> {
    if !should_require_structured_semantic_policy(request) {
        return None;
    }
    let operations = normalized_required_semantic_operations(
        request.semantic_policy_required_operations.iter().cloned(),
    );
    if operations.is_empty() {
        return None;
    }
    Some(format!(
        "Structured semantic policy - required before finish for this {} work:\n\
- Source: {}\n\
- Required semantic_query operations: {}\n\
Medium and high complexity runs may edit after an anchoring semantic_query, but final handoff needs the assigned operations. Readiness-only operations such as capabilities and lsp_probe do not satisfy this policy. This policy does not widen write scope, grant authority, or mark proof passed.",
        request.work_complexity_tier,
        if request.semantic_policy_source.trim().is_empty() {
            "repo_intake"
        } else {
            request.semantic_policy_source.as_str()
        },
        operations.join(", ")
    ))
}

fn candidate_strategy_line(intent: &str) -> String {
    for line in intent.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Strategy:") {
            let rest = rest.trim();
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
        if let Some(rest) = trimmed.strip_prefix("Fleet semantic strategy:") {
            let rest = rest.trim();
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
        if let Some(rest) = trimmed.strip_prefix("Integration semantic strategy:") {
            let rest = rest.trim();
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
    }
    "single principal path".to_string()
}

fn language_pack_guidance(language_pack_id: &str) -> Option<&'static str> {
    match language_pack_id {
        "ios-xcode" => Some(
            "- Treat the Xcode workspace/project, targets, schemes, entitlements, and test bundles as the source of truth.\n\
- Preserve Swift style, UIKit/SwiftUI boundaries, lifecycle behavior, permissions, and public app contracts unless the task explicitly changes them.\n\
- Prefer focused XCTest or UI test coverage in the existing target shape.\n\
- Run xcodebuild test with the appropriate scheme and destination before finishing, and ignore DerivedData and .build output as build noise.",
        ),
        "swift-spm" => Some(
            "- Treat Package.swift as the source of truth for targets and tests.\n\
- Prefer idiomatic Foundation, XCTest, and small focused types.\n\
- Run swift test before finishing.\n\
- Do not treat .build or DerivedData output as reviewable source.",
        ),
        "java-maven" => Some(
            "- Treat pom.xml as the source of truth for modules, dependencies, and test setup.\n\
- Keep changes idiomatic Java, preserving public APIs unless the task explicitly changes them.\n\
- Prefer focused JUnit tests for behavior and regressions.\n\
- Run mvn test before finishing and ignore target output as build noise.",
        ),
        "gradle-jvm" => Some(
            "- Treat Gradle files and the wrapper as the source of truth for Java/Kotlin builds.\n\
- Preserve existing package layout, JVM style, and public API compatibility.\n\
- Prefer focused unit or integration tests in the existing test framework.\n\
- Run ./gradlew test before finishing and ignore build and .gradle output as build noise.",
        ),
        "android-gradle" => Some(
            "- Treat Gradle files, AndroidManifest.xml, resources, modules, and the wrapper as the source of truth.\n\
- Preserve Android lifecycle, permissions, resource naming, Kotlin/Java style, and public app contracts unless the task explicitly changes them.\n\
- Prefer local unit tests or the repo's existing Android test target for behavior and regressions.\n\
- Run ./gradlew testDebugUnitTest before finishing and ignore build and .gradle output as build noise.",
        ),
        "dotnet" => Some(
            "- Treat the solution, project files, and existing test project layout as the source of truth.\n\
- Preserve public contracts, nullable/async conventions, and dependency injection style.\n\
- Prefer xUnit, NUnit, or the repo's existing test style for behavior and regressions.\n\
- Run dotnet test before finishing and ignore bin, obj, and TestResults output as build noise.",
        ),
        "node" => Some(
            "- Treat package.json and the lockfile as the source of truth for scripts and package manager.\n\
- Preserve the repo's TypeScript, lint, module, and test conventions.\n\
- Prefer behavior-level tests in the existing runner before changing implementation broadly.\n\
- Run the selected package test command before finishing and ignore node_modules, dist, build, .next, and coverage output as build noise.",
        ),
        "rust-cargo" => Some(
            "- Treat Cargo.toml and workspace layout as the source of truth.\n\
- Keep changes idiomatic Rust: small functions, explicit errors, ownership-aware APIs, and narrow public surface changes.\n\
- Prefer focused unit tests or integration tests for regressions.\n\
- Run cargo test before finishing and ignore target output as build noise.",
        ),
        "python" => Some(
            "- Treat pyproject.toml or requirements.txt as the source of truth for tooling.\n\
- Preserve the repo's typing, formatting, import, and test conventions.\n\
- Prefer focused pytest tests or fixtures for behavior and regressions.\n\
- Run pytest before finishing and ignore .pytest_cache, __pycache__, dist, and build output as build noise.",
        ),
        "go" => Some(
            "- Treat go.mod as the source of truth for modules.\n\
- Keep changes idiomatic Go: small functions, explicit errors, table-driven tests when useful, and stable public APIs.\n\
- Prefer focused go tests for behavior and regressions.\n\
- Run go test ./... before finishing and ignore bin and coverage output as build noise.",
        ),
        _ => None,
    }
}

fn capped_flywheel_block(title: &str, body: &str) -> String {
    let capped: String = body.chars().take(MAX_FLYWHEEL_BYTES).collect();
    format!("{title}:\n{capped}")
}

const MAX_ACTIVE_LESSONS_PER_RUN: usize = 5;

struct SelectedLearningMemory {
    lines: Vec<String>,
    matched_memory_ids: Vec<String>,
    passed_over_memory_ids: Vec<String>,
}

/// Relevance-first selection over the active learning memory. Every active,
/// non-superseded lesson is scored against this run's pre-run match keys -
/// work complexity tier, repo language pack, and lexical overlap between the
/// lesson summary and the run intent - and the top five ride, latest first on
/// ties. A lesson whose explicit applicability list does not name this run is
/// excluded even if it is the most recent; a lesson with no applicability
/// (every lesson recorded before the fields existed) stays universal. The
/// passed-over ids come back so the citation receipt can name them.
fn select_active_learning_memory(
    kernel: &MdxKernel,
    request: &ForgeRunRequest,
    run_language_pack: &str,
) -> SelectedLearningMemory {
    // A retired lesson stops guiding new runs: skip any activation whose
    // receipt is cited by a learning.memory.superseded receipt.
    let superseded_activations =
        crate::learning_routes::memory_supersede::superseded_activation_receipt_ids(kernel);
    let intent_tokens = relevance_tokens(&request.intent);
    let mut scored: Vec<(u32, usize, &mdx_core::Receipt)> = Vec::new();
    let mut passed_over_memory_ids: Vec<String> = Vec::new();
    for (recency, receipt) in kernel
        .ledger()
        .query()
        .by_kind("learning.memory.activated")
        .iter()
        .enumerate()
    {
        if payload_value(receipt, "active_memory_state") != "active"
            || superseded_activations.contains(&receipt.receipt_id)
        {
            continue;
        }
        match lesson_relevance_score(
            receipt,
            &request.work_complexity_tier,
            run_language_pack,
            &intent_tokens,
        ) {
            Some(score) => scored.push((score, recency, receipt)),
            None => passed_over_memory_ids.push(active_memory_id_of(receipt)),
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    let mut lines = Vec::new();
    let mut matched_memory_ids = Vec::new();
    for (index, (_, _, receipt)) in scored.iter().enumerate() {
        if index < MAX_ACTIVE_LESSONS_PER_RUN {
            matched_memory_ids.push(active_memory_id_of(receipt));
            lines.push(format!(
                "- {} Evidence: {}",
                payload_value(receipt, "lesson_summary"),
                payload_value(receipt, "evidence_refs")
            ));
        } else {
            passed_over_memory_ids.push(active_memory_id_of(receipt));
        }
    }
    SelectedLearningMemory {
        lines,
        matched_memory_ids,
        passed_over_memory_ids,
    }
}

/// One active lesson's relevance score, or None when the lesson declared an
/// explicit tier or language pack list that does not include this run. An
/// explicit match outranks a universal lesson, and lexical overlap between
/// the lesson summary and the run intent only breaks ties inside one match
/// level - it can never lift a mismatched lesson past the exclusion.
fn lesson_relevance_score(
    receipt: &mdx_core::Receipt,
    run_work_tier: &str,
    run_language_pack: &str,
    intent_tokens: &std::collections::BTreeSet<String>,
) -> Option<u32> {
    let tier_match = applicability_list_match(
        payload_value(receipt, "applicability_work_tiers"),
        run_work_tier,
    )?;
    let pack_match = applicability_list_match(
        payload_value(receipt, "applicability_language_packs"),
        run_language_pack,
    )?;
    let overlap = relevance_tokens(payload_value(receipt, "lesson_summary"))
        .intersection(intent_tokens)
        .count() as u32;
    Some((tier_match + pack_match) * 1_000 + overlap.min(999))
}

/// Some(1) when the comma-joined list names the run's value, Some(0) when the
/// list is empty (universal), None when the list is explicit and misses.
fn applicability_list_match(list: &str, run_value: &str) -> Option<u32> {
    let entries: Vec<&str> = list
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect();
    if entries.is_empty() {
        return Some(0);
    }
    let run_value = run_value.trim();
    (!run_value.is_empty()
        && entries
            .iter()
            .any(|entry| entry.eq_ignore_ascii_case(run_value)))
    .then_some(1)
}

/// Lowercased alphanumeric-trimmed tokens for lexical overlap, with the
/// highest-frequency English filler dropped so shared filler cannot rank one
/// lesson over another.
fn relevance_tokens(text: &str) -> std::collections::BTreeSet<String> {
    const FILLER: &[&str] = &["the", "and", "for", "that", "this", "with"];
    text.split_whitespace()
        .map(|token| {
            token
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|token| token.len() >= 3 && !FILLER.contains(&token.as_str()))
        .collect()
}

fn active_memory_id_of(receipt: &mdx_core::Receipt) -> String {
    let memory_id = payload_value(receipt, "active_memory_id");
    if memory_id.is_empty() {
        receipt.receipt_id.clone()
    } else {
        memory_id.to_string()
    }
}

/// Comma-joined ids for one run-event detail token. The receipt detail is
/// hard-capped at 2000 chars and an overlong detail refuses the whole event,
/// so a long passed-over list is truncated with an explicit +N_more marker
/// instead of silently losing the citation receipt.
fn joined_memory_ids_or_none(ids: &[String]) -> String {
    const MAX_IDS_IN_DETAIL: usize = 24;
    if ids.is_empty() {
        return "none".to_string();
    }
    if ids.len() <= MAX_IDS_IN_DETAIL {
        return ids.join(",");
    }
    format!(
        "{},+{}_more",
        ids[..MAX_IDS_IN_DETAIL].join(","),
        ids.len() - MAX_IDS_IN_DETAIL
    )
}

struct SelectedDistilledSkills {
    lines: Vec<String>,
    matched_skill_ids: Vec<String>,
}

/// The most relevant ratified distilled skills for this run, top three, scored
/// on the same match keys as advisory lessons: work complexity tier, repo
/// language pack, and lexical overlap between the skill summary and the run
/// intent. A skill whose explicit applicability list does not name this run is
/// excluded; a universal skill (no applicability) rides for everyone. A
/// boundary-locked (sensitive) skill is the exception: it rides only on an
/// EXPLICIT applicability match, so sensitive trajectory content never boards
/// an unrelated run.
fn select_ratified_distilled_skills(
    kernel: &MdxKernel,
    request: &ForgeRunRequest,
    run_language_pack: &str,
) -> SelectedDistilledSkills {
    const MAX_DISTILLED_SKILLS_PER_RUN: usize = 3;
    let intent_tokens = relevance_tokens(&request.intent);
    let mut scored: Vec<(u32, mdx_core::DistilledSkillView)> = Vec::new();
    for view in kernel.distilled_skill_views() {
        if view.state != "ratified" {
            continue;
        }
        let tier_match = match applicability_list_match(
            &view.applicability_work_tiers,
            &request.work_complexity_tier,
        ) {
            Some(value) => value,
            None => continue,
        };
        let pack_match =
            match applicability_list_match(&view.applicability_language_packs, run_language_pack) {
                Some(value) => value,
                None => continue,
            };
        // A boundary-locked skill needs an explicit match on tier or pack.
        if view.boundary_locked && tier_match + pack_match == 0 {
            continue;
        }
        let overlap = relevance_tokens(&view.summary)
            .intersection(&intent_tokens)
            .count() as u32;
        let score = (tier_match + pack_match) * 1_000 + overlap.min(999);
        scored.push((score, view));
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.skill_id.cmp(&b.1.skill_id)));
    let mut lines = Vec::new();
    let mut matched_skill_ids = Vec::new();
    for (_, view) in scored.into_iter().take(MAX_DISTILLED_SKILLS_PER_RUN) {
        matched_skill_ids.push(view.skill_id.clone());
        lines.push(format!(
            "- {} ({}). Proven procedure:\n{}",
            view.skill_name, view.summary, view.procedure
        ));
    }
    SelectedDistilledSkills {
        lines,
        matched_skill_ids,
    }
}

fn marketplace_installed_capability_lines(kernel: &MdxKernel) -> Vec<String> {
    let mut installed: std::collections::BTreeMap<String, (String, String)> =
        std::collections::BTreeMap::new();
    for receipt in kernel.ledger().query().by_kind("marketplace.act.recorded") {
        let capability_id = payload_value(receipt, "capability_id");
        let scope = payload_value(receipt, "scope");
        if capability_id.is_empty() || scope.is_empty() {
            continue;
        }
        let key = format!("{capability_id}@{scope}");
        match payload_value(receipt, "act") {
            "install" | "pack_install" => {
                installed.insert(key, (capability_id.to_string(), "installed".to_string()));
            }
            "approval" => {
                installed.insert(key, (capability_id.to_string(), "approved".to_string()));
            }
            "revocation" => {
                installed.remove(&key);
            }
            _ => {}
        }
    }
    installed
        .into_iter()
        .take(5)
        .map(|(key, (capability_id, status))| {
            format!("- {capability_id} ({key}) status={status}, execution_allowed=false")
        })
        .collect()
}

fn payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

pub(crate) fn run_selected_check(
    run_id: &str,
    command: &str,
    workspace_path: &Path,
    repo_root: &Path,
    target_dir_override: Option<&str>,
) -> (bool, i32, u64, String) {
    if hosted_check_target(target_dir_override) {
        let result = crate::mobile_hosted_sandbox::run_hosted_command(
            run_id,
            workspace_path,
            repo_root,
            command,
        );
        return (
            result.passed,
            result.exit_code,
            result.duration_ms,
            format!(
                "mdx_hosted_evidence provider={} session_ref={} source_fingerprint={} output_sha256={}\n{}",
                result.provider,
                result.session_ref,
                result.source_fingerprint,
                result.output_sha256,
                result.output_tail
            ),
        );
    }
    if !forge_host_checks_enabled() {
        // Host execution disabled outright: only the container path may
        // run anything, and it fails closed when no sandbox image is
        // configured.
        return crate::harness_sandbox_runner::run_in_container_with_output(
            run_id,
            command,
            Some(workspace_path),
        )
        .map(|(result, tail)| (result.passed, result.exit_code, result.duration_ms, tail))
        .unwrap_or_else(|| {
            (
                false,
                -1,
                0,
                "command execution is not enabled (local host checks are disabled; set MDX_PLAN_TEST_EXEC=1 with a sandbox image, or remove MDX_FORGE_DISABLE_HOST_CHECKS)".to_string(),
            )
        });
    }
    match crate::forge_exec_sandbox::host_exec_mode() {
        // Legacy raw host execution: full inherited env, no confinement.
        // Explicit opt-in only (MDX_FORGE_HOST_CHECKS=1) - the flip is
        // recorded as an execution-posture receipt.
        crate::forge_exec_sandbox::HostExecMode::RawHost => run_check_on_host(
            run_id,
            command,
            workspace_path,
            repo_root,
            target_dir_override,
        ),
        // The default: scrubbed environment (no provider keys or
        // credentials reach the subprocess) and, when the OS provides it,
        // Seatbelt confinement. Setup commands keep network so dependency
        // installs work; proof commands run with network denied.
        _ => crate::forge_exec_sandbox::run_scrubbed_confined_for_run(
            run_id,
            command,
            workspace_path,
            repo_root,
            target_dir_override,
            crate::forge_repo_profile::is_setup_command(command),
        ),
    }
}

fn hosted_check_target(target_dir_override: Option<&str>) -> bool {
    target_dir_override.is_some_and(|value| value.starts_with("hosted://"))
}

fn local_check_target_dir(target_dir_override: Option<&str>) -> Option<&str> {
    target_dir_override.filter(|value| !value.starts_with("hosted://"))
}

/// Opt-in auto-escalation: re-run a network-denied check once with network
/// allowed. Off by default (governed default = proof commands run offline);
/// the operator turns it on and every escalation is named in the receipt.
fn auto_escalate_enabled() -> bool {
    std::env::var("MDX_FORGE_AUTO_ESCALATE").ok().as_deref() == Some("1")
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

pub(crate) fn compact_check_tail(tail: &str) -> String {
    let normalized = tail.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "empty".to_string();
    }
    const LIMIT: usize = 240;
    if normalized.len() <= LIMIT {
        normalized
    } else {
        let mut end = LIMIT;
        while !normalized.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &normalized[..end])
    }
}

/// Run one allowlisted check on the host, cwd'd to the throwaway
/// worktree. Returns (passed, exit_code, duration_ms, output_tail). The
/// build cache is shared with the live repo via CARGO_TARGET_DIR so an
/// incremental check is fast - the worktree is for source isolation, not
/// a cold rebuild. Output is capped to a tail the loop can read; the
/// receipts stay presence-only.
fn run_check_on_host(
    run_id: &str,
    command: &str,
    workspace_path: &Path,
    repo_root: &Path,
    target_dir_override: Option<&str>,
) -> (bool, i32, u64, String) {
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c").arg(command).current_dir(workspace_path);
    // Reuse the live repo's warm target dir so an incremental check is
    // fast - the worktree isolates source, not the build cache. A fleet
    // stream overrides this with its own dir so parallel builds never
    // fight over one cache.
    match target_dir_override {
        Some(dir) if !dir.is_empty() => cmd.env("CARGO_TARGET_DIR", dir),
        _ => cmd.env("CARGO_TARGET_DIR", repo_root.join("target")),
    };
    crate::forge_exec_sandbox::run_and_cap_for_run(cmd, run_id)
}

fn first_line(text: &str) -> String {
    text.lines()
        .next()
        .unwrap_or("a forge change")
        .chars()
        .take(72)
        .collect()
}

fn bounded_human_run_text(text: &str, max_chars: usize) -> String {
    let compact = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let redacted = redact_sensitive_run_text(&compact);
    redacted.chars().take(max_chars).collect()
}

fn redact_sensitive_run_text(text: &str) -> String {
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

/// Witness one run event as a kernel receipt under a brief lock.
fn record(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    event: &str,
    detail: &str,
    turn: u32,
    input_tokens: u64,
    output_tokens: u64,
) {
    let model_evidence = run_model_evidence_fields(request);
    let report = {
        let Ok(mut kernel) = kernel.write() else {
            return;
        };
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: &request.tenant_id,
                    actor_id: &request.actor_id,
                    run_id: &request.run_id,
                    event,
                    work_item_id: &request.work_item_id,
                    detail,
                    turn,
                    input_tokens,
                    output_tokens,
                },
                &mdx_core::GovernedWriteIdentity::local_demo(&request.actor_id),
                &model_evidence,
            )
            .ok()
    };
    if let Some(report) = report {
        let model = crate::forge_run_stream::RunStreamModel::from_slot(
            &request.builder_slot,
            &request.work_complexity_tier,
        );
        crate::forge_run_stream::publish_forge_run_event(
            &request.run_id,
            event,
            detail,
            turn,
            &report.receipt_id,
            &model,
        );
    }
}

/// Witness one measured run event when a real wall-clock boundary exists.
struct TimedRunEvent<'a> {
    event: &'a str,
    detail: &'a str,
    turn: u32,
    input_tokens: u64,
    output_tokens: u64,
    duration_ms: u64,
}

fn record_timed(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    event: TimedRunEvent<'_>,
) {
    record_timed_with_evidence_fields(kernel, request, event, &[]);
}

fn record_timed_with_evidence_fields(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    event: TimedRunEvent<'_>,
    evidence_fields: &[(&str, &str)],
) {
    let mut model_evidence = run_model_evidence_fields(request);
    model_evidence.extend_from_slice(evidence_fields);
    let report = {
        let Ok(mut kernel) = kernel.write() else {
            return;
        };
        kernel
            .record_forge_run_event_with_duration_and_evidence_fields(
                ForgeRunEvent {
                    tenant_id: &request.tenant_id,
                    actor_id: &request.actor_id,
                    run_id: &request.run_id,
                    event: event.event,
                    work_item_id: &request.work_item_id,
                    detail: event.detail,
                    turn: event.turn,
                    input_tokens: event.input_tokens,
                    output_tokens: event.output_tokens,
                },
                event.duration_ms,
                &model_evidence,
            )
            .ok()
    };
    if let Some(report) = report {
        let model = crate::forge_run_stream::RunStreamModel::from_slot(
            &request.builder_slot,
            &request.work_complexity_tier,
        );
        crate::forge_run_stream::publish_forge_run_event(
            &request.run_id,
            event.event,
            event.detail,
            event.turn,
            &report.receipt_id,
            &model,
        );
    }
}

// Persist the full conversation for this run and witness it on the ledger.
// The transcript file holds the content; the receipt holds only the
// reference plus shape (turn, message count) - the same split Pages uses for
// authored bodies. A store that fails (unnameable run id, disk error) is not
// fatal: the run keeps going in memory exactly as before, it just loses the
// resume anchor for this turn, which the next successful persist restores.
/// The outcome of compacting a transcript: the new message list, plus the
/// durable facts pulled from the summarized turns so the ledger holds what a
/// summary might blur.
struct TranscriptCompaction {
    messages: Vec<serde_json::Value>,
    turns_compacted: u32,
    files_touched: Vec<String>,
    checks_run: Vec<String>,
}

// Compact only past this many messages: a long run grows the Vec every turn
// and re-sends the whole thing to the model, so an unbounded transcript both
// costs money and eventually overflows the context window. 0 or unset keeps
// the default. The floor guarantees compaction never fires on a short run.
fn transcript_compact_after_messages() -> usize {
    std::env::var("MDX_FORGE_COMPACT_AFTER_MESSAGES")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value >= 24)
        .unwrap_or(80)
}

fn transcript_keep_recent_turns() -> usize {
    std::env::var("MDX_FORGE_COMPACT_KEEP_RECENT_TURNS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value >= 2)
        .unwrap_or(6)
}

/// Governed anchored compaction. The system prompt and the original task -
/// everything before the first assistant turn - are the authority region and
/// are NEVER summarized: the run's constraints (do not skip checks, do not
/// widen scope, do not open authority) survive verbatim across every
/// compaction. The most recent `keep_recent_turns` turns stay verbatim too.
/// The middle - whole turns only, so no tool call is ever orphaned from its
/// result - collapses into one structured summary message that names the
/// files touched and checks run rather than inventing prose. Returns None when
/// there is nothing meaningful to compact (too short, or no clean boundary).
fn compact_transcript(
    messages: &[serde_json::Value],
    keep_recent_turns: usize,
) -> Option<TranscriptCompaction> {
    // Turn starts: each assistant message begins a turn (its tool results
    // follow as `tool` messages). The messages before the first are the
    // authority anchor.
    let turn_starts: Vec<usize> = messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message["role"].as_str() == Some("assistant"))
        .map(|(index, _)| index)
        .collect();
    if turn_starts.len() <= keep_recent_turns + 1 {
        return None;
    }
    let anchor_end = turn_starts[0];
    let tail_start = turn_starts[turn_starts.len() - keep_recent_turns];
    let middle = &messages[anchor_end..tail_start];
    let turns_compacted = turn_starts.len().saturating_sub(keep_recent_turns) as u32;

    // Pull durable facts from the middle before it collapses.
    let mut files_touched: Vec<String> = Vec::new();
    let mut checks_run: Vec<String> = Vec::new();
    for message in middle {
        if let Some(calls) = message["tool_calls"].as_array() {
            for call in calls {
                let name = call["function"]["name"].as_str().unwrap_or("");
                let args_raw = call["function"]["arguments"].as_str().unwrap_or("{}");
                let args: serde_json::Value =
                    serde_json::from_str(args_raw).unwrap_or(serde_json::Value::Null);
                if name == "edit_file" || name == "write_file" || name == "create_file" {
                    if let Some(path) = args["path"].as_str()
                        && !files_touched.iter().any(|existing| existing == path)
                    {
                        files_touched.push(path.to_string());
                    }
                } else if name == "apply_patch" {
                    if let Some(input) = args["input"].as_str()
                        && let Ok(ops) = crate::forge_apply_patch::parse_patch(input)
                    {
                        for op in &ops {
                            for path in op.touched_paths() {
                                if !files_touched.iter().any(|existing| existing == path) {
                                    files_touched.push(path.to_string());
                                }
                            }
                        }
                    }
                } else if name == "run_command"
                    && let Some(command) = args["command"].as_str()
                    && !checks_run.iter().any(|existing| existing == command)
                {
                    checks_run.push(command.to_string());
                }
            }
        }
    }

    let files_line = if files_touched.is_empty() {
        "none recorded".to_string()
    } else {
        files_touched.join(", ")
    };
    let checks_line = if checks_run.is_empty() {
        "none recorded".to_string()
    } else {
        checks_run.join("; ")
    };
    let summary = format!(
        "Compacted earlier progress ({turns_compacted} turns summarized to fit context; the full transcript is on the ledger). \
Files touched so far: {files_line}. Checks run so far: {checks_line}. \
Continue from the recent turns below - do not re-derive this groundwork, and the task and its constraints above still hold."
    );

    let mut compacted = Vec::with_capacity(anchor_end + 1 + (messages.len() - tail_start));
    compacted.extend_from_slice(&messages[..anchor_end]);
    compacted.push(serde_json::json!({ "role": "user", "content": summary }));
    compacted.extend_from_slice(&messages[tail_start..]);

    Some(TranscriptCompaction {
        messages: compacted,
        turns_compacted,
        files_touched,
        checks_run,
    })
}

// Compact in place when the transcript has grown large, witnessing the event
// on the ledger. Compaction is a governed, receipted act - not a silent
// context trim - so the durable facts pulled from the summarized turns are
// recorded before they collapse, and the authority region is provably intact.
/// The token count at which the run should compact: the context window minus a
/// reserve for the next completion minus headroom (Codex's shape). Compaction
/// fires when the LAST prompt's real input-token count crosses this, so it
/// tracks the true edge of the window instead of a blind message count.
fn compaction_token_threshold(context_window: u64, max_output: u32) -> u64 {
    let reserve = (max_output as u64).max(20_000);
    context_window
        .saturating_sub(reserve)
        .saturating_sub(13_000)
        .max(8_000)
}

/// Should this run compact now? Token-budget first (the real signal), with the
/// message count as a fallback for providers that report no usage.
fn should_compact(
    message_count: usize,
    last_input_tokens: u64,
    context_window: u64,
    max_output: u32,
) -> bool {
    if last_input_tokens > 0
        && last_input_tokens >= compaction_token_threshold(context_window, max_output)
    {
        return true;
    }
    message_count >= transcript_compact_after_messages()
}

/// How many recently edited files to re-read into context after a compaction,
/// and the per-file byte cap. After the middle collapses, the model can lose
/// the current contents of files it edited; re-reading the most recent ones
/// keeps it working from the real code, not memory (Codex does the same).
const COMPACT_REREAD_FILES: usize = 5;
const COMPACT_REREAD_BYTES_PER_FILE: usize = 5_000;

#[allow(clippy::too_many_arguments)]
fn maybe_compact_run_transcript(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    messages: &mut Vec<serde_json::Value>,
    turn: u32,
    last_input_tokens: u64,
    context_window: u64,
    max_output: u32,
    workspace_path: &Path,
    edited_paths: &[String],
) {
    if !should_compact(
        messages.len(),
        last_input_tokens,
        context_window,
        max_output,
    ) {
        return;
    }
    let Some(compaction) = compact_transcript(messages, transcript_keep_recent_turns()) else {
        return;
    };
    let before = messages.len().to_string();
    *messages = compaction.messages;
    let after = messages.len().to_string();
    let turns = compaction.turns_compacted.to_string();
    let files = if compaction.files_touched.is_empty() {
        "none".to_string()
    } else {
        compaction.files_touched.join(", ")
    };
    let checks = if compaction.checks_run.is_empty() {
        "none".to_string()
    } else {
        compaction.checks_run.join("; ")
    };
    record_with_evidence_fields(
        kernel,
        request,
        "transcript_compacted",
        &format!("compacted {turns} turns to keep the run inside its context window"),
        turn,
        &[
            ("transcript_compacted_turns", &turns),
            ("transcript_messages_before", &before),
            ("transcript_messages_after", &after),
            ("transcript_compacted_files_touched", &files),
            ("transcript_compacted_checks_run", &checks),
            ("authority_region_preserved", "true"),
        ],
    );
    // Re-read the most recently edited files so the model works from the real
    // on-disk code after the middle collapsed, not from a memory the summary
    // dropped. Most-recent first, bounded per file and in count.
    if let Some(reread) = recent_edits_reread(workspace_path, edited_paths) {
        messages.push(serde_json::json!({ "role": "user", "content": reread }));
    }
}

/// Render a plan (the update_plan `steps` array) into a compact, legible line
/// for the receipt: each step with a glyph for its status. Tolerant of a
/// malformed payload so a bad plan call never breaks the run.
fn render_plan(steps: &serde_json::Value) -> String {
    let Some(items) = steps.as_array() else {
        return "plan updated (unreadable steps)".to_string();
    };
    if items.is_empty() {
        return "plan cleared".to_string();
    }
    let lines: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let step = item["step"].as_str().unwrap_or("(step)");
            let glyph = match item["status"].as_str().unwrap_or("pending") {
                "completed" => "[x]",
                "in_progress" => "[>]",
                _ => "[ ]",
            };
            format!("{}. {glyph} {step}", index + 1)
        })
        .collect();
    lines.join(" | ")
}

/// The workspace-relative paths one edit tool call touched: the path arg for
/// edit_file/write_file, or every file the apply_patch envelope changed.
fn edited_paths_of(call: &ParsedToolCall) -> Vec<String> {
    match call.name.as_str() {
        "edit_file" | "write_file" => call.arguments["path"]
            .as_str()
            .filter(|p| !p.is_empty())
            .map(|p| vec![p.to_string()])
            .unwrap_or_default(),
        "apply_patch" => call.arguments["input"]
            .as_str()
            .and_then(|input| crate::forge_apply_patch::parse_patch(input).ok())
            .map(|ops| {
                ops.iter()
                    .flat_map(|op| op.touched_paths().into_iter().map(|p| p.to_string()))
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Build a single message with the current contents of the most recently
/// edited files (most recent first, capped). Returns None when nothing was
/// edited or none can be read.
fn recent_edits_reread(workspace_path: &Path, edited_paths: &[String]) -> Option<String> {
    let mut sections: Vec<String> = Vec::new();
    for rel in edited_paths.iter().rev().take(COMPACT_REREAD_FILES) {
        let Ok(content) = read_confined(workspace_path, rel) else {
            continue;
        };
        let shown: String = content
            .chars()
            .take(COMPACT_REREAD_BYTES_PER_FILE)
            .collect();
        let note = if content.len() > COMPACT_REREAD_BYTES_PER_FILE {
            "\n... (truncated; read_file for the rest)"
        } else {
            ""
        };
        sections.push(format!("--- {rel} ---\n{shown}{note}"));
    }
    if sections.is_empty() {
        return None;
    }
    Some(format!(
        "Context was compacted to stay inside the window. Current on-disk state of the files you most recently edited (work from this, not memory):\n\n{}",
        sections.join("\n\n")
    ))
}

enum FinishGateResult {
    Proceed,
    Revise(String),
    RequiredUnavailable(String),
    BudgetExhausted(String),
}

#[derive(Clone, Copy)]
struct FinishGatePolicy {
    required: bool,
    max_cost_cents: u64,
}

// Run the fresh-eyes finish evaluator at the close gate. Conditional review
// preserves the historical fail-open posture. Required review fails closed
// when no diff or reviewer is available. Reviewer tokens are charged to the
// same run budget as builder and planner tokens.
fn evaluate_finish_before_close(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    workspace_path: &Path,
    finish_evaluator_blocks: &mut u32,
    turn: u32,
    policy: FinishGatePolicy,
    spent_microcents: &mut u64,
) -> FinishGateResult {
    let Some(diff) = crate::forge_finish_evaluator::workspace_diff(workspace_path) else {
        if policy.required {
            let reason = "required independent review could not inspect a workspace diff";
            record_with_evidence_fields(
                kernel,
                request,
                "finish_evaluated",
                reason,
                turn,
                &[
                    ("finish_evaluator_verdict", "required_unavailable"),
                    ("review_fail_open", "false"),
                ],
            );
            return FinishGateResult::RequiredUnavailable(reason.to_string());
        }
        return FinishGateResult::Proceed;
    };
    let Some(verdict) = crate::forge_finish_evaluator::evaluate_finish(
        kernel,
        &request.tenant_id,
        &request.intent,
        &diff,
        &request.reasoning_effort,
    ) else {
        if policy.required {
            let reason = "required independent review model was unavailable";
            record_with_evidence_fields(
                kernel,
                request,
                "finish_evaluated",
                reason,
                turn,
                &[
                    ("finish_evaluator_verdict", "required_unavailable"),
                    ("review_fail_open", "false"),
                ],
            );
            return FinishGateResult::RequiredUnavailable(reason.to_string());
        }
        return FinishGateResult::Proceed;
    };
    *spent_microcents =
        spent_microcents.saturating_add(crate::forge_model_pricing::turn_cost_microcents(
            &verdict.model_id,
            verdict.input_tokens,
            verdict.output_tokens,
        ));
    if let Some(reason) = budget_breach(
        0,
        0,
        crate::forge_model_pricing::microcents_to_cents_rounded_up(*spent_microcents),
        policy.max_cost_cents,
    ) {
        record_with_evidence_fields(
            kernel,
            request,
            "finish_evaluated",
            &reason,
            turn,
            &[
                ("finish_evaluator_verdict", "budget_exhausted"),
                ("review_fail_open", "false"),
            ],
        );
        return FinishGateResult::BudgetExhausted(reason);
    }
    record_timed(
        kernel,
        request,
        TimedRunEvent {
            event: "model_called",
            detail: &format!("independent finish review by {}", verdict.model_id),
            turn,
            input_tokens: verdict.input_tokens,
            output_tokens: verdict.output_tokens,
            duration_ms: 0,
        },
    );
    if verdict.approved {
        record_with_evidence_fields(
            kernel,
            request,
            "finish_evaluated",
            "fresh-eyes reviewer approved the finish",
            turn,
            &[("finish_evaluator_verdict", "approved")],
        );
        return FinishGateResult::Proceed;
    }
    let max_blocks = crate::forge_finish_evaluator::max_finish_blocks();
    if *finish_evaluator_blocks >= max_blocks {
        // The reviewer still objects, but the seam advises - it does not gate.
        // The builder and its green checks close the run; the unmet concern
        // stays on the ledger for a human to weigh.
        record_with_evidence_fields(
            kernel,
            request,
            "finish_evaluated",
            "fresh-eyes reviewer concern unresolved at the block ceiling; finishing on green checks",
            turn,
            &[
                ("finish_evaluator_verdict", "revise_exhausted"),
                ("finish_evaluator_concern", &verdict.concern),
            ],
        );
        return FinishGateResult::Proceed;
    }
    *finish_evaluator_blocks += 1;
    let blocks = finish_evaluator_blocks.to_string();
    record_with_evidence_fields(
        kernel,
        request,
        "finish_evaluated",
        "fresh-eyes reviewer sent the run back to address a concrete gap",
        turn,
        &[
            ("finish_evaluator_verdict", "revise"),
            ("finish_evaluator_concern", &verdict.concern),
            ("finish_evaluator_block_count", &blocks),
        ],
    );
    FinishGateResult::Revise(verdict.concern)
}

fn persist_run_transcript(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    messages: &[serde_json::Value],
    turn: u32,
) {
    let Some(transcript_ref) =
        crate::forge_transcript_store::store_transcript(&request.run_id, messages)
    else {
        return;
    };
    let message_count = messages.len().to_string();
    record_with_evidence_fields(
        kernel,
        request,
        "transcript_persisted",
        &format!("transcript persisted through turn {turn}"),
        turn,
        &[
            ("transcript_ref", &transcript_ref),
            ("transcript_message_count", &message_count),
        ],
    );
}

fn record_with_evidence_fields(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    event: &str,
    detail: &str,
    turn: u32,
    evidence_fields: &[(&str, &str)],
) {
    let mut fields = run_model_evidence_fields(request);
    fields.extend_from_slice(evidence_fields);
    let report = {
        let Ok(mut kernel) = kernel.write() else {
            return;
        };
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: &request.tenant_id,
                    actor_id: &request.actor_id,
                    run_id: &request.run_id,
                    event,
                    work_item_id: &request.work_item_id,
                    detail,
                    turn,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &mdx_core::GovernedWriteIdentity::local_demo(&request.actor_id),
                &fields,
            )
            .ok()
    };
    if let Some(report) = report {
        let model = crate::forge_run_stream::RunStreamModel::from_slot(
            &request.builder_slot,
            &request.work_complexity_tier,
        );
        crate::forge_run_stream::publish_forge_run_event(
            &request.run_id,
            event,
            detail,
            turn,
            &report.receipt_id,
            &model,
        );
    }
}

fn run_model_evidence_fields(request: &ForgeRunRequest) -> Vec<(&'static str, &str)> {
    let model = crate::forge_run_stream::RunStreamModel::from_slot(
        &request.builder_slot,
        &request.work_complexity_tier,
    );
    vec![
        ("model_slot", model.slot),
        ("model_provider_family", model.provider_family),
        ("model_id", model.model_id),
        ("model_class", model.model_class),
        ("model_class_source", "forge_run_request"),
        ("model_class_grants_execution_authority", "false"),
    ]
}

fn record_semantic_preflight(
    kernel: &Arc<RwLock<MdxKernel>>,
    request: &ForgeRunRequest,
    preflight: &SemanticMissionPack,
    duration_ms: u64,
) {
    if preflight.anchor_count == 0 {
        return;
    }
    let anchor_count = preflight.anchor_count.to_string();
    let planned_operation_count = preflight.planned_operations.len().to_string();
    let observed_operation_count = preflight.observed_operations.len().to_string();
    let indexed_file_count = preflight.indexed_file_count.to_string();
    let indexed_symbol_count = preflight.indexed_symbol_count.to_string();
    let related_test_anchor_count = preflight.related_test_anchor_count.to_string();
    let duration_ms = duration_ms.to_string();
    let planned_operations = preflight.planned_operations.join(",");
    let observed_operations = preflight.observed_operations.join(",");
    let evidence_fields = [
        ("semantic_preflight_status", "observed"),
        (
            "semantic_preflight_source",
            "forge_loop_runner::semantic_mission_pack",
        ),
        (
            "semantic_preflight_session_id",
            preflight.session_id.as_str(),
        ),
        ("semantic_preflight_anchor_count", anchor_count.as_str()),
        (
            "semantic_preflight_planned_operation_count",
            planned_operation_count.as_str(),
        ),
        (
            "semantic_preflight_observed_operation_count",
            observed_operation_count.as_str(),
        ),
        (
            "semantic_preflight_planned_operations",
            planned_operations.as_str(),
        ),
        (
            "semantic_preflight_observed_operations",
            observed_operations.as_str(),
        ),
        (
            "semantic_preflight_indexed_file_count",
            indexed_file_count.as_str(),
        ),
        (
            "semantic_preflight_indexed_symbol_count",
            indexed_symbol_count.as_str(),
        ),
        (
            "semantic_preflight_related_test_anchor_count",
            related_test_anchor_count.as_str(),
        ),
        ("semantic_preflight_duration_ms", duration_ms.as_str()),
        ("semantic_preflight_grants_execution_authority", "false"),
    ];
    record_with_evidence_fields(
        kernel,
        request,
        "evidence_appended",
        &format!(
            "semantic_preflight_observed anchors={} observed_ops={} planned_ops={}",
            preflight.anchor_count,
            preflight.observed_operations.len(),
            preflight.planned_operations.len()
        ),
        0,
        &evidence_fields,
    );
}

#[cfg(test)]
mod tests {
    use super::{Arc, MdxKernel, RwLock, initial_applied_control_receipts};

    #[test]
    fn resumed_runs_do_not_reapply_historical_stop_or_steer_controls() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let run_id = "forge_run_resume_control_test";
        let mut receipt_ids = Vec::new();
        for (control, note) in [
            ("steer", "finish the focused proof"),
            ("stop", "[mobile_pause] pause at the safe boundary"),
        ] {
            let report = kernel
                .write()
                .unwrap()
                .record_forge_run_control(mdx_core::ForgeRunControl {
                    tenant_id: "local_tenant",
                    actor_id: "local_user",
                    run_id,
                    control,
                    note,
                })
                .unwrap();
            receipt_ids.push(report.receipt_id);
        }
        assert!(initial_applied_control_receipts(&kernel, run_id, false).is_empty());
        let resumed = initial_applied_control_receipts(&kernel, run_id, true);
        assert_eq!(resumed.len(), 2);
        assert!(
            receipt_ids
                .iter()
                .all(|receipt_id| resumed.contains(receipt_id))
        );
    }

    fn test_request() -> super::ForgeRunRequest {
        super::ForgeRunRequest {
            run_id: "r".into(),
            tenant_id: "t".into(),
            actor_id: "a".into(),
            work_item_id: "w".into(),
            intent: "i".into(),
            allowed_commands: vec![],
            max_turns: 10,
            revise_branch: None,
            resume: false,
            write_scope: vec![],
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "medium".into(),
            semantic_policy_required_operations: vec![],
            semantic_policy_source: String::new(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: String::new(),
            execution_geometry_route: String::new(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        }
    }

    #[test]
    fn model_fabric_denial_records_terminal_outcome_without_deadlocking_kernel() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let deterministic_provider = std::env::var_os("MDX_FORGE_DETERMINISTIC_PROVIDER");
        unsafe {
            std::env::set_var("MDX_FORGE_DETERMINISTIC_PROVIDER", "0");
        }

        let tenant_id = "forge-loop-routing-denial-test";
        let registry = crate::model_fabric_registry::global();
        registry.clear_tenant(tenant_id);
        registry
            .upsert_connection(mdx_core::ProviderConnection {
                connection_id: format!("{tenant_id}:offline"),
                tenant_id: tenant_id.to_string(),
                provider_id: "ollama".to_string(),
                credential_ref: "local:none".to_string(),
                endpoint_base_url: "http://127.0.0.1:11434/v1".to_string(),
                region: "local".to_string(),
                residency: "local".to_string(),
                data_retention: "none".to_string(),
                training_policy: "none".to_string(),
                data_policy_provenance: "test".to_string(),
                data_policy_observed_at: "2026-07-14".to_string(),
                health: "disconnected".to_string(),
                live_call_allowed: false,
            })
            .expect("configured denied route");

        let mut request = test_request();
        request.run_id = "forge_run_routing_denial_test".to_string();
        request.tenant_id = tenant_id.to_string();
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let kernel_for_run = Arc::clone(&kernel);
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let outcome = super::run_forge_loop(
                &request,
                std::path::Path::new("/unused-for-routing-denial"),
                &kernel_for_run,
            );
            let _ = sender.send(outcome);
        });
        let result = receiver.recv_timeout(std::time::Duration::from_secs(2));

        registry.clear_tenant(tenant_id);
        unsafe {
            match deterministic_provider {
                Some(value) => std::env::set_var("MDX_FORGE_DETERMINISTIC_PROVIDER", value),
                None => std::env::remove_var("MDX_FORGE_DETERMINISTIC_PROVIDER"),
            }
        }

        let outcome = result.expect("routing denial must return instead of deadlocking the kernel");
        assert!(
            outcome
                .finish_summary
                .contains("model fabric denied builder route")
        );
        assert!(kernel.try_read().is_ok(), "kernel lock must be released");
    }

    fn new_outcome(run_id: &str) -> super::ForgeRunOutcome {
        super::ForgeRunOutcome {
            run_id: run_id.to_string(),
            status: "RUN_STARTED",
            turns_used: 0,
            files_changed: 0,
            check_runs: 0,
            check_duration_ms: 0,
            branch: None,
            commit_sha: None,
            finish_summary: String::new(),
            last_check_passed: false,
        }
    }

    #[test]
    fn budget_exhaustion_preserves_the_latest_selected_proof_result() {
        let mut failed = new_outcome("forge_run_budget_failed_proof");
        failed.status = "RUN_BUDGET_EXHAUSTED";
        failed.check_runs = 1;
        failed.finish_summary = "the turn budget ran out before completion".to_string();
        super::preserve_selected_proof_truth_on_budget_exhaustion(&mut failed);
        assert!(
            failed
                .finish_summary
                .contains("latest selected check failed")
        );
        assert_eq!(super::selected_proof_status(&failed), "failed");

        let mut passed = new_outcome("forge_run_budget_green_proof");
        passed.status = "RUN_BUDGET_EXHAUSTED";
        passed.check_runs = 2;
        passed.last_check_passed = true;
        passed.finish_summary = "the review budget ran out".to_string();
        super::preserve_selected_proof_truth_on_budget_exhaustion(&mut passed);
        assert!(
            passed
                .finish_summary
                .contains("latest selected check passed")
        );
        assert_eq!(super::selected_proof_status(&passed), "passed");
    }

    /// Serializes the tests that toggle MDX_FORGE_FLYWHEEL_CONTEXT so the
    /// process-global env var never leaks into a parallel assemble_context test.
    static FLYWHEEL_CONTEXT_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn thrash_step_back_threshold_defaults_and_floors() {
        assert_eq!(super::thrash_step_back_threshold(), 3);
        unsafe {
            std::env::set_var("MDX_FORGE_THRASH_STEP_BACK_AFTER", "1");
        }
        assert_eq!(
            super::thrash_step_back_threshold(),
            3,
            "floor of 2 keeps a 1 from disabling it"
        );
        unsafe {
            std::env::set_var("MDX_FORGE_THRASH_STEP_BACK_AFTER", "5");
        }
        assert_eq!(super::thrash_step_back_threshold(), 5);
        unsafe {
            std::env::remove_var("MDX_FORGE_THRASH_STEP_BACK_AFTER");
        }
    }

    #[test]
    fn apply_patch_ops_confined_is_all_or_nothing() {
        let dir = std::env::temp_dir().join(format!(
            "forge-apply-patch-confined-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("keep.txt"), "one\ntwo\n").expect("write");
        std::fs::write(dir.join("gone.txt"), "bye\n").expect("write");

        // Add + update + delete + move in one envelope, applied atomically.
        let ops = crate::forge_apply_patch::parse_patch(
            "*** Begin Patch\n\
             *** Add File: added.txt\n+hello\n\
             *** Update File: keep.txt\n*** Move to: kept.txt\n@@\n-two\n+three\n\
             *** Delete File: gone.txt\n\
             *** End Patch",
        )
        .expect("parses");
        let applied = super::apply_patch_ops_confined(&dir, &ops).expect("applies");
        assert_eq!(
            applied,
            vec!["added.txt", "keep.txt -> kept.txt", "gone.txt"]
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("added.txt")).expect("added"),
            "hello\n"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("kept.txt")).expect("moved"),
            "one\nthree\n"
        );
        assert!(!dir.join("keep.txt").exists(), "move removes the source");
        assert!(!dir.join("gone.txt").exists(), "delete removes the file");

        // A failing hunk in the SECOND op must leave the first unapplied.
        std::fs::write(dir.join("a.txt"), "alpha\n").expect("write");
        let failing = crate::forge_apply_patch::parse_patch(
            "*** Begin Patch\n\
             *** Add File: never.txt\n+nope\n\
             *** Update File: a.txt\n@@\n-not in the file\n+x\n\
             *** End Patch",
        )
        .expect("parses");
        let error = super::apply_patch_ops_confined(&dir, &failing).expect_err("must fail");
        assert!(error.contains("a.txt"), "{error}");
        assert!(
            !dir.join("never.txt").exists(),
            "nothing may apply when any hunk fails"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn budget_breach_enforces_both_ceilings_and_zero_means_unlimited() {
        // Inside both ceilings: no breach.
        assert!(super::budget_breach(1_000, 10_000, 5, 100).is_none());
        // Runtime ceiling wins first and names its numbers.
        let runtime = super::budget_breach(10_001, 10_000, 0, 100).expect("runtime breach");
        assert!(runtime.contains("elapsed_ms=10001"), "{runtime}");
        // Cost ceiling: spending the budget exactly is exhaustion, not
        // one-more-turn - a run may never end OVER its ceiling.
        let cost = super::budget_breach(1, 10_000, 100, 100).expect("cost breach");
        assert!(cost.contains("spent_cents=100"), "{cost}");
        // 0 = unlimited for the explicit-opt-out case.
        assert!(super::budget_breach(u64::MAX, 0, u64::MAX, 0).is_none());
    }

    use super::*;

    #[test]
    fn delegated_model_usage_records_spend_and_model_events() {
        use std::sync::{Arc, RwLock};

        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let request = test_request_with_checks(vec![]);
        let report = crate::forge_subagent::SubagentReport {
            text: "finding".to_string(),
            model_usage: vec![
                crate::forge_subagent::SubagentModelUsage {
                    model_id: "claude-sonnet-5".to_string(),
                    input_tokens: 10,
                    output_tokens: 2,
                },
                crate::forge_subagent::SubagentModelUsage {
                    model_id: "gpt-5-mini".to_string(),
                    input_tokens: 5,
                    output_tokens: 1,
                },
            ],
        };
        let mut spent_microcents = 0;

        account_delegated_model_usage(
            &kernel,
            &request,
            3,
            "investigate",
            &report,
            &mut spent_microcents,
        );

        let expected_spend =
            crate::forge_model_pricing::turn_cost_microcents("claude-sonnet-5", 10, 2)
                + crate::forge_model_pricing::turn_cost_microcents("gpt-5-mini", 5, 1);
        assert_eq!(spent_microcents, expected_spend);

        let guard = kernel.read().expect("kernel");
        let delegated_events: Vec<_> = guard
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .into_iter()
            .filter(|receipt| {
                payload_value(receipt, "run_id") == request.run_id
                    && payload_value(receipt, "event") == "model_called"
                    && payload_value(receipt, "detail").contains("delegated investigate")
            })
            .collect();
        assert_eq!(
            delegated_events.len(),
            2,
            "each delegated model call must be receipted"
        );
        assert_eq!(payload_value(delegated_events[0], "tokens_in"), "10");
        assert_eq!(payload_value(delegated_events[0], "tokens_out"), "2");
        assert_eq!(payload_value(delegated_events[1], "tokens_in"), "5");
        assert_eq!(payload_value(delegated_events[1], "tokens_out"), "1");
    }

    fn test_request_with_checks(commands: Vec<&str>) -> ForgeRunRequest {
        ForgeRunRequest {
            run_id: "run".to_string(),
            tenant_id: "tenant".to_string(),
            actor_id: "actor".to_string(),
            work_item_id: String::new(),
            intent: "test intent".to_string(),
            allowed_commands: commands.into_iter().map(str::to_string).collect(),
            max_turns: 4,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "medium".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "single_worker".to_string(),
            execution_geometry_route: "/forge/runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        }
    }

    fn ready_orientation_gate() -> PrincipalOrientationGate {
        PrincipalOrientationGate {
            required: true,
            language_pack_id: "node".to_string(),
            semantic_query_seen: true,
            semantic_anchor_seen: true,
            semantic_operations_seen: HashSet::new(),
            required_semantic_operations: Vec::new(),
            semantic_strategy_required_before_write: false,
            structured_policy_required_before_finish: false,
            dependency_map_seen: false,
            dependency_map_required_before_write: false,
            related_tests_seen: false,
            changed_files_seen: false,
        }
    }

    #[test]
    fn forge_run_voice_profile_defaults_chill_and_can_switch_to_neutral() {
        let _guard = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::remove_var("MDX_FORGE_RUN_VOICE_PROFILE");
        }
        let default_profile = forge_run_voice_profile();
        assert_eq!(default_profile.id, "md_chill");
        assert!(
            default_profile
                .prompt_contract
                .contains("quick engineering update")
        );
        assert!(default_profile.prompt_contract.contains("avoid slang"));
        assert!(default_profile.prompt_contract.contains("cannot-proceed"));
        assert!(!default_profile.prompt_contract.contains("we good"));
        assert!(
            default_profile
                .prompt_contract
                .contains("Do not use em dashes")
        );
        assert!(!default_profile.prompt_contract.contains('\u{2014}'));
        assert!(!default_profile.prompt_contract.contains('\u{2013}'));

        unsafe {
            std::env::set_var("MDX_FORGE_RUN_VOICE_PROFILE", "neutral_engineering");
        }
        let neutral_profile = forge_run_voice_profile();
        assert_eq!(neutral_profile.id, "neutral_engineering");
        assert!(forge_run_system_prompt(&neutral_profile).contains("concise engineering status"));
        assert!(!neutral_profile.prompt_contract.contains('\u{2014}'));
        assert!(!neutral_profile.prompt_contract.contains('\u{2013}'));

        unsafe {
            std::env::remove_var("MDX_FORGE_RUN_VOICE_PROFILE");
        }
    }

    #[test]
    fn operator_summary_claim_guard_rejects_new_deployment_or_file_claims() {
        let facts = "Factual builder summary: Updated apps/mdx/src/routes/Test.svelte and ran pnpm test. Checks passed.";
        assert!(operator_summary_claims_safe(
            facts,
            "Updated apps/mdx/src/routes/Test.svelte and ran pnpm test. Checks passed."
        ));
        assert!(!operator_summary_claims_safe(
            facts,
            "yo, updated apps/mdx/src/routes/Test.svelte and ran pnpm test. we good."
        ));
        assert!(!operator_summary_claims_safe(
            facts,
            "Yo, deployed apps/mdx/src/routes/Test.svelte to production and ran pnpm test."
        ));
        assert!(!operator_summary_claims_safe(
            facts,
            "Yo, updated apps/mdx/src/routes/Other.svelte and ran pnpm test."
        ));
    }

    fn unchanged_outcome() -> ForgeRunOutcome {
        ForgeRunOutcome {
            run_id: "run_unchanged".to_string(),
            status: "RUN_FAILED_TO_START",
            turns_used: 0,
            files_changed: 0,
            check_runs: 0,
            check_duration_ms: 0,
            branch: None,
            commit_sha: None,
            finish_summary: String::new(),
            last_check_passed: false,
        }
    }

    #[test]
    fn a_failed_commit_with_changes_demotes_off_a_shippable_status() {
        // Work produced then lost (held branch ref, failed checkout) must not
        // report DONE - the operator would review a change that never landed.
        assert_eq!(
            commit_failure_status(2, false),
            Some("RUN_FINISHED_CANNOT_PROCEED"),
        );
        // An empty run that commits nothing is benign - no demotion.
        assert_eq!(commit_failure_status(0, false), None);
        // A commit that landed keeps its status.
        assert_eq!(commit_failure_status(2, true), None);
    }

    #[test]
    fn finish_done_without_a_diff_is_no_change_not_done() {
        let mut outcome = ForgeRunOutcome {
            run_id: "run_no_change".to_string(),
            status: "RUN_FAILED_TO_START",
            turns_used: 0,
            files_changed: 0,
            check_runs: 0,
            check_duration_ms: 0,
            branch: None,
            commit_sha: None,
            finish_summary: String::new(),
            last_check_passed: true,
        };

        apply_finish_outcome(&mut outcome, "done", "tests passed");

        assert_eq!(outcome.status, "RUN_FINISHED_NO_CHANGE");
        assert_eq!(outcome.finish_summary, "tests passed");
        assert!(outcome.branch.is_none());
    }

    #[test]
    fn finish_done_with_a_diff_remains_done() {
        let mut outcome = ForgeRunOutcome {
            run_id: "run_changed".to_string(),
            status: "RUN_FAILED_TO_START",
            turns_used: 0,
            files_changed: 1,
            check_runs: 0,
            check_duration_ms: 0,
            branch: None,
            commit_sha: None,
            finish_summary: String::new(),
            last_check_passed: true,
        };

        apply_finish_outcome(&mut outcome, "done", "implemented and tested");

        assert_eq!(outcome.status, "RUN_FINISHED_DONE");
        assert_eq!(outcome.finish_summary, "implemented and tested");
    }

    #[test]
    fn finish_done_after_diff_requires_selected_proof_not_setup_only() {
        let request = test_request_with_checks(vec!["npm install", "npm test"]);
        let outcome = ForgeRunOutcome {
            run_id: "run_changed_setup_only".to_string(),
            status: "RUN_FAILED_TO_START",
            turns_used: 0,
            files_changed: 1,
            check_runs: 0,
            check_duration_ms: 0,
            branch: None,
            commit_sha: None,
            finish_summary: String::new(),
            last_check_passed: false,
        };

        let refusal = finish_proof_refusal(&request, &outcome, "done").expect("finish refused");

        assert!(refusal.contains("no selected proof command has passed"));
        assert!(refusal.contains("npm test"));
    }

    #[test]
    fn finish_done_after_diff_allows_post_change_proof() {
        let request = test_request_with_checks(vec!["npm install", "npm test"]);
        let outcome = ForgeRunOutcome {
            run_id: "run_changed_proven".to_string(),
            status: "RUN_FAILED_TO_START",
            turns_used: 0,
            files_changed: 1,
            check_runs: 0,
            check_duration_ms: 0,
            branch: None,
            commit_sha: None,
            finish_summary: String::new(),
            last_check_passed: true,
        };

        assert!(finish_proof_refusal(&request, &outcome, "done").is_none());
    }

    #[test]
    fn finish_cannot_proceed_stays_cannot_proceed_even_without_a_diff() {
        let mut outcome = ForgeRunOutcome {
            run_id: "run_blocked".to_string(),
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

        apply_finish_outcome(&mut outcome, "cannot_proceed", "missing write authority");

        assert_eq!(outcome.status, "RUN_FINISHED_CANNOT_PROCEED");
        assert_eq!(outcome.finish_summary, "missing write authority");
    }

    #[test]
    fn premature_cannot_proceed_is_refused_before_concrete_attempt() {
        let mut request = test_request_with_checks(vec!["npm ci", "npm run test:lane:08_theta"]);
        request.write_scope = vec!["src/lane_08_theta.js".to_string()];
        let gate = ready_orientation_gate();
        let outcome = unchanged_outcome();

        assert!(premature_cannot_proceed_refusal(
            &request,
            &outcome,
            &gate,
            "cannot_proceed",
            false,
        ));
    }

    #[test]
    fn cannot_proceed_is_allowed_after_selected_proof_attempt() {
        let mut request = test_request_with_checks(vec!["npm ci", "npm run test:lane:08_theta"]);
        request.write_scope = vec!["src/lane_08_theta.js".to_string()];
        let gate = ready_orientation_gate();
        let outcome = unchanged_outcome();

        assert!(!premature_cannot_proceed_refusal(
            &request,
            &outcome,
            &gate,
            "cannot_proceed",
            true,
        ));
    }

    #[test]
    fn baseline_red_selected_proof_proceeds_and_reports_preexisting_failure() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::set_var("MDX_FORGE_BASELINE_PROOF_PREFLIGHT", "1");
            std::env::set_var("MDX_FORGE_HOST_CHECKS", "1");
        }
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_baseline_red_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let request = test_request_with_checks(vec!["sh -c 'echo baseline-red; exit 7'"]);
        let mut outcome = unchanged_outcome();

        let note = run_baseline_proof_preflight(&kernel, &request, &dir, &dir, &mut outcome)
            .expect("baseline note");

        // A red test/check on arrival no longer dead-ends the run: the status
        // is left unchanged for the model turns and the pre-existing failure is
        // reported as context rather than blamed on the operator.
        assert_ne!(outcome.status, "RUN_FINISHED_CANNOT_PROCEED");
        assert_eq!(outcome.turns_used, 0);
        assert_eq!(outcome.check_runs, 1);
        assert!(note.selected_proof_red_on_arrival);
        assert!(!note.baseline_repair_expected);
        assert!(outcome.check_duration_ms > 0 || note.note.contains("after 0 ms"));
        assert!(note.note.contains("red on arrival before model turns"));
        assert!(note.note.contains("already red before your ask"));
        assert!(!note.note.contains("baseline health"));
        assert!(outcome.finish_summary.is_empty());

        unsafe {
            std::env::remove_var("MDX_FORGE_BASELINE_PROOF_PREFLIGHT");
            std::env::remove_var("MDX_FORGE_HOST_CHECKS");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn baseline_red_hard_gauntlet_task_continues_to_model_turns() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::set_var("MDX_FORGE_BASELINE_PROOF_PREFLIGHT", "1");
            std::env::set_var("MDX_FORGE_HOST_CHECKS", "1");
        }
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_baseline_red_repair_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let mut request = test_request_with_checks(vec!["sh -c 'echo baseline-red; exit 7'"]);
        request.intent = "Implement a policy bundle evaluator and add tests.\n\nThis is a Forge hard-gauntlet task. Make a real source change and prove the behavior with:\nsh -c 'echo baseline-red; exit 7'\n\nVisible proof contract:\nAPI\nExpected behavior".to_string();
        let mut outcome = unchanged_outcome();

        let note = run_baseline_proof_preflight(&kernel, &request, &dir, &dir, &mut outcome)
            .expect("baseline note");

        assert_eq!(outcome.status, "RUN_FAILED_TO_START");
        assert_eq!(outcome.turns_used, 0);
        assert_eq!(outcome.check_runs, 1);
        assert!(note.selected_proof_red_on_arrival);
        assert!(note.baseline_repair_expected);
        assert!(
            note.note
                .contains("continue using this failure as the first proof signal")
        );
        assert!(outcome.finish_summary.is_empty());

        unsafe {
            std::env::remove_var("MDX_FORGE_BASELINE_PROOF_PREFLIGHT");
            std::env::remove_var("MDX_FORGE_HOST_CHECKS");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn baseline_red_fleet_stream_with_explicit_repair_continues_to_model_turns() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::set_var("MDX_FORGE_BASELINE_PROOF_PREFLIGHT", "1");
            std::env::set_var("MDX_FORGE_HOST_CHECKS", "1");
        }
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_baseline_red_fleet_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let mut request = test_request_with_checks(vec!["sh -c 'echo lane-red; exit 7'"]);
        request.intent =
            "Fleet stream s01: repair the failing lane proof by implementing the alpha lane."
                .to_string();
        request.execution_geometry_lane = "fleet_stream".to_string();
        request.execution_geometry_route = "/forge/fleet-runs.json".to_string();
        request.semantic_policy_required_operations = vec![
            "symbol_graph".to_string(),
            "definition".to_string(),
            "dependency_map".to_string(),
            "related_tests".to_string(),
        ];
        request.semantic_policy_source = "fleet_stream_semantic_strategy".to_string();
        let mut outcome = unchanged_outcome();

        let note = run_baseline_proof_preflight(&kernel, &request, &dir, &dir, &mut outcome)
            .expect("baseline note");

        assert_eq!(outcome.status, "RUN_FAILED_TO_START");
        assert_eq!(outcome.turns_used, 0);
        assert_eq!(outcome.check_runs, 1);
        assert!(note.selected_proof_red_on_arrival);
        assert!(note.baseline_repair_expected);
        assert!(
            note.note
                .contains("continue using this failure as the first proof signal")
        );
        assert!(outcome.finish_summary.is_empty());

        unsafe {
            std::env::remove_var("MDX_FORGE_BASELINE_PROOF_PREFLIGHT");
            std::env::remove_var("MDX_FORGE_HOST_CHECKS");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn baseline_red_fleet_stream_without_repair_proceeds() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::set_var("MDX_FORGE_BASELINE_PROOF_PREFLIGHT", "1");
            std::env::set_var("MDX_FORGE_HOST_CHECKS", "1");
        }
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_baseline_red_fleet_stop_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let mut request = test_request_with_checks(vec!["sh -c 'echo lane-red; exit 7'"]);
        request.intent = "Fleet stream s01: implement the alpha lane.".to_string();
        request.execution_geometry_lane = "fleet_stream".to_string();
        request.execution_geometry_route = "/forge/fleet-runs.json".to_string();
        request.semantic_policy_required_operations = vec![
            "symbol_graph".to_string(),
            "definition".to_string(),
            "dependency_map".to_string(),
            "related_tests".to_string(),
        ];
        request.semantic_policy_source = "fleet_stream_semantic_strategy".to_string();
        let mut outcome = unchanged_outcome();

        let note = run_baseline_proof_preflight(&kernel, &request, &dir, &dir, &mut outcome)
            .expect("baseline note");

        // Investigate, then proceed: a red lane check on arrival no longer
        // stops the stream at turn 0 - it runs and reports the pre-existing
        // failure as context.
        assert_ne!(outcome.status, "RUN_FINISHED_CANNOT_PROCEED");
        assert_eq!(outcome.turns_used, 0);
        assert_eq!(outcome.check_runs, 1);
        assert!(note.selected_proof_red_on_arrival);
        assert!(!note.baseline_repair_expected);
        assert!(note.note.contains("already red before your ask"));
        assert!(outcome.finish_summary.is_empty());

        unsafe {
            std::env::remove_var("MDX_FORGE_BASELINE_PROOF_PREFLIGHT");
            std::env::remove_var("MDX_FORGE_HOST_CHECKS");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn baseline_red_ordinary_test_request_proceeds_before_model_turns() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::set_var("MDX_FORGE_BASELINE_PROOF_PREFLIGHT", "1");
            std::env::set_var("MDX_FORGE_HOST_CHECKS", "1");
        }
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_baseline_red_ordinary_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let mut request = test_request_with_checks(vec!["sh -c 'echo baseline-red; exit 7'"]);
        request.intent = "Add a new endpoint and add tests for it.".to_string();
        let mut outcome = unchanged_outcome();

        let note = run_baseline_proof_preflight(&kernel, &request, &dir, &dir, &mut outcome)
            .expect("baseline note");

        // An ordinary ask against a red baseline proceeds and reports the
        // pre-existing failure, rather than dead-ending at turn 0.
        assert_ne!(outcome.status, "RUN_FINISHED_CANNOT_PROCEED");
        assert_eq!(outcome.turns_used, 0);
        assert_eq!(outcome.check_runs, 1);
        assert!(note.selected_proof_red_on_arrival);
        assert!(!note.baseline_repair_expected);
        assert!(note.note.contains("red on arrival before model turns"));
        assert!(note.note.contains("already red before your ask"));
        assert!(outcome.finish_summary.is_empty());

        unsafe {
            std::env::remove_var("MDX_FORGE_BASELINE_PROOF_PREFLIGHT");
            std::env::remove_var("MDX_FORGE_HOST_CHECKS");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn baseline_red_untrusted_hard_gauntlet_words_get_no_repair_framing() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::set_var("MDX_FORGE_BASELINE_PROOF_PREFLIGHT", "1");
            std::env::set_var("MDX_FORGE_HOST_CHECKS", "1");
        }
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_baseline_red_untrusted_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let mut request = test_request_with_checks(vec!["sh -c 'echo baseline-red; exit 7'"]);
        request.intent =
            "This hard gauntlet task from the code review still needs implementation.".to_string();
        let mut outcome = unchanged_outcome();

        let note = run_baseline_proof_preflight(&kernel, &request, &dir, &dir, &mut outcome)
            .expect("baseline note");

        // Untrusted "hard gauntlet" phrasing (not the trusted marker) does not
        // earn the repair framing: the run proceeds like any ordinary ask and
        // reports the pre-existing failure - it never claims the failure is the
        // "first proof signal".
        assert_ne!(outcome.status, "RUN_FINISHED_CANNOT_PROCEED");
        assert_eq!(outcome.turns_used, 0);
        assert_eq!(outcome.check_runs, 1);
        assert!(note.selected_proof_red_on_arrival);
        assert!(!note.baseline_repair_expected);
        assert!(note.note.contains("already red before your ask"));
        assert!(!note.note.contains("first proof signal"));
        assert!(outcome.finish_summary.is_empty());

        unsafe {
            std::env::remove_var("MDX_FORGE_BASELINE_PROOF_PREFLIGHT");
            std::env::remove_var("MDX_FORGE_HOST_CHECKS");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stalled_orientation_tools_include_broad_orientation_but_not_target_reads() {
        assert!(!stalled_orientation_tool("read_file"));
        assert!(stalled_orientation_tool("search"));
        assert!(stalled_orientation_tool("list_dir"));
        assert!(stalled_orientation_tool("semantic_query"));
        assert!(stalled_orientation_tool("investigate"));
        assert!(!stalled_orientation_tool("edit_file"));
        assert!(!stalled_orientation_tool("run_command"));
        assert!(!stalled_orientation_tool("finish"));
    }

    #[test]
    fn stalled_orientation_call_blocks_read_only_shell_but_allows_selected_proof() {
        let allowed = vec![
            "pnpm install --frozen-lockfile".to_string(),
            "pnpm test".to_string(),
        ];
        let read_shell = ParsedToolCall {
            call_id: "call_rg".to_string(),
            name: "shell".to_string(),
            arguments: serde_json::json!({
                "command": "rg -n -i 'baseline|proof' docs crates | head -40"
            }),
        };
        let read_command = ParsedToolCall {
            call_id: "call_sed".to_string(),
            name: "run_command".to_string(),
            arguments: serde_json::json!({
                "command": "sed -n '1,120p' docs/FORGE-OPERATOR-EXPERIENCE.md"
            }),
        };
        let selected_proof = ParsedToolCall {
            call_id: "call_test".to_string(),
            name: "run_command".to_string(),
            arguments: serde_json::json!({
                "command": "pnpm test"
            }),
        };

        assert!(stalled_orientation_call(&read_shell, &allowed));
        assert!(stalled_orientation_call(&read_command, &allowed));
        assert!(!stalled_orientation_call(&selected_proof, &allowed));
    }

    #[test]
    fn repeated_stalled_target_read_blocks_same_file_but_allows_first_or_different_file() {
        let first_read = ParsedToolCall {
            call_id: "call_read_first".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({
                "path": "apps/mdx-operator-macos/Sources/MDxOperator/Views/RunDetailView.swift"
            }),
        };
        let repeated_read = ParsedToolCall {
            call_id: "call_read_repeat".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({
                "path": "./apps/mdx-operator-macos/Sources/MDxOperator/Views/RunDetailView.swift"
            }),
        };
        let different_read = ParsedToolCall {
            call_id: "call_read_other".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({
                "path": "apps/mdx-operator-macos/Sources/MDxOperator/Models/ForgeRunModels.swift"
            }),
        };
        let mut already_read = HashSet::new();

        assert!(!repeated_stalled_target_read(&first_read, &already_read));
        already_read.insert(target_read_key(&first_read).expect("read key"));
        assert!(repeated_stalled_target_read(&repeated_read, &already_read));
        assert!(!repeated_stalled_target_read(
            &different_read,
            &already_read
        ));
        assert!(!stalled_orientation_call(&first_read, &[]));
    }

    #[test]
    fn finish_cannot_proceed_after_green_changed_proof_becomes_done() {
        let mut outcome = ForgeRunOutcome {
            run_id: "run_changed_green".to_string(),
            status: "RUN_FAILED_TO_START",
            turns_used: 0,
            files_changed: 1,
            check_runs: 0,
            check_duration_ms: 0,
            branch: None,
            commit_sha: None,
            finish_summary: String::new(),
            last_check_passed: true,
        };

        apply_finish_outcome(&mut outcome, "cannot_proceed", "uncertain after proof");

        assert_eq!(outcome.status, "RUN_FINISHED_DONE");
        assert_eq!(outcome.finish_summary, "uncertain after proof");
    }

    #[test]
    fn preexisting_red_branch_promotes_to_review_with_proof_caveat() {
        let mut outcome = ForgeRunOutcome {
            run_id: "run_preexisting_red".to_string(),
            status: "RUN_FINISHED_CANNOT_PROCEED",
            turns_used: 6,
            files_changed: 1,
            check_runs: 2,
            check_duration_ms: 42,
            branch: Some("forge/run_preexisting_red".to_string()),
            commit_sha: Some("abc123".to_string()),
            finish_summary: "change landed, but selected proof stayed red".to_string(),
            last_check_passed: false,
        };

        assert!(promote_preexisting_red_branch_to_review(
            &mut outcome,
            true,
            false,
        ));

        assert_eq!(outcome.status, "RUN_FINISHED_DONE");
        assert!(!outcome.last_check_passed);
        assert!(outcome.finish_summary.contains("proof caveat"));
    }

    #[test]
    fn preexisting_red_promotion_rewrites_cannot_mark_done_copy() {
        let summary = reviewable_preexisting_red_summary(
            "doc change landed. cannot mark done because pnpm test was already red.",
            "Reviewable branch landed with proof caveat.",
        );

        assert!(summary.contains("ready for review with a proof caveat because"));
        assert!(!summary.contains("cannot mark done"));
    }

    #[test]
    fn preexisting_red_repair_task_stays_blocked_when_proof_remains_red() {
        let mut outcome = ForgeRunOutcome {
            run_id: "run_repair_still_red".to_string(),
            status: "RUN_FINISHED_CANNOT_PROCEED",
            turns_used: 6,
            files_changed: 1,
            check_runs: 2,
            check_duration_ms: 42,
            branch: Some("forge/run_repair_still_red".to_string()),
            commit_sha: Some("abc123".to_string()),
            finish_summary: "attempted repair, but selected proof stayed red".to_string(),
            last_check_passed: false,
        };

        assert!(!promote_preexisting_red_branch_to_review(
            &mut outcome,
            true,
            true,
        ));

        assert_eq!(outcome.status, "RUN_FINISHED_CANNOT_PROCEED");
    }

    #[test]
    fn fresh_review_branch_name_stays_run_scoped_but_avoids_restart_collisions() {
        let branch =
            fresh_review_branch_name("forge_run_000022", "mdx_ws_forge_run_000022_68374_0");

        assert!(branch.starts_with("forge/run-forge_run_000022-"));
        assert!(branch.contains("mdx_ws_forge_run_000022_68374_0"));
        assert_ne!(branch, "forge/run-forge_run_000022");
    }

    #[test]
    fn the_write_scope_law_holds_at_the_edit_site() {
        // In scope: the exact file, a file inside a scoped directory.
        let scope = vec![
            "crates/mdx-core/src/foo.rs".to_string(),
            "apps/mdx-host/src/routes/foo/".to_string(),
        ];
        assert!(path_in_scope("crates/mdx-core/src/foo.rs", &scope));
        assert!(path_in_scope("./crates/mdx-core/src/foo.rs", &scope));
        assert!(path_in_scope(
            "apps/mdx-host/src/routes/foo/+page.svelte",
            &scope
        ));
        // Out of scope: a sibling, a name-prefix cousin, a registration file.
        assert!(!path_in_scope("crates/mdx-core/src/bar.rs", &scope));
        assert!(!path_in_scope("crates/mdx-core/src/foo_extra.rs", &scope));
        assert!(!path_in_scope("crates/mdx-core/src/lib.rs", &scope));

        // An unscoped run (the original single-run behavior) refuses nothing.
        let open = ForgeRunRequest {
            run_id: "r".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "a".to_string(),
            work_item_id: String::new(),
            intent: String::new(),
            allowed_commands: Vec::new(),
            max_turns: 1,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "unknown".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "single_worker".to_string(),
            execution_geometry_route: "/forge/runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        };
        assert!(scope_refusal(&open, "anything/at/all.rs", "edit_file").is_none());

        // A scoped run refuses the out-of-scope edit with the scope spelled
        // out, and the refusal is a recoverable tool result, not a crash.
        let scoped = ForgeRunRequest {
            write_scope: scope,
            ..open
        };
        let refused =
            scope_refusal(&scoped, "crates/mdx-core/src/lib.rs", "edit_file").expect("must refuse");
        assert!(refused.0.contains("outside this stream's write scope"));
        assert!(refused.2.contains("refused out-of-scope"));
        assert!(scope_refusal(&scoped, "crates/mdx-core/src/foo.rs", "edit_file").is_none());
    }

    #[test]
    fn a_write_can_create_a_whole_new_directory_and_traversal_still_cannot() {
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_newdir_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        // A file in a directory that does not exist yet - a new route
        // folder, a new module tree - is creatable, not refused.
        assert!(write_confined(&dir, "routes/changelog/+page.svelte", "<h1/>\n").is_ok());
        assert!(dir.join("routes/changelog/+page.svelte").exists());
        // Deeper nesting too.
        assert!(write_confined(&dir, "a/b/c/d.txt", "deep\n").is_ok());
        // Traversal through a not-yet-existing path is still refused.
        assert!(write_confined(&dir, "newdir/../../escape.txt", "no").is_err());
        assert!(confined_path(&dir, "../escape.txt").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn semantic_query_tool_returns_workspace_symbol_without_writing_or_passing_checks() {
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_semantic_tool_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(dir.join("Sources/Canary")).expect("mkdir");
        std::fs::write(dir.join("Package.swift"), "// swift-tools-version: 5.9\n")
            .expect("package");
        std::fs::write(
            dir.join("Sources/Canary/Slugger.swift"),
            "public struct Slugger {\n    public func run() {}\n}\n",
        )
        .expect("source");
        let call = ParsedToolCall {
            call_id: "tool_1".to_string(),
            name: "semantic_query".to_string(),
            arguments: serde_json::json!({
                "operation": "workspace_symbol",
                "query": "Slugger"
            }),
        };
        let request = ForgeRunRequest {
            run_id: "run".to_string(),
            tenant_id: "tenant".to_string(),
            actor_id: "actor".to_string(),
            work_item_id: String::new(),
            intent: String::new(),
            allowed_commands: Vec::new(),
            max_turns: 1,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "unknown".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "single_worker".to_string(),
            execution_geometry_route: "/forge/runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        };
        let mut outcome = ForgeRunOutcome {
            run_id: "run".to_string(),
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

        let (text, event, detail, check_passed) =
            dispatch_tool(&call, &dir, &dir, &request, &mut outcome);

        assert!(text.contains("semantic_query status=OK"));
        assert!(text.contains("language_pack=swift-spm"));
        assert!(text.contains("Sources/Canary/Slugger.swift:1:15"));
        assert_eq!(event, "tool_executed");
        assert_eq!(
            detail,
            "semantic_query status=OK operation=workspace_symbol"
        );
        assert!(!check_passed);
        assert_eq!(outcome.files_changed, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn selected_check_runs_in_local_worktree_when_host_checks_are_enabled() {
        if !forge_host_checks_enabled() {
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_host_check_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");

        let (passed, exit_code, _duration_ms, tail) = run_selected_check(
            "run_host_check",
            "printf proof-ok > proof.out",
            &dir,
            &dir,
            None,
        );

        assert!(passed, "tail={tail}");
        assert_eq!(exit_code, 0);
        assert_eq!(
            std::fs::read_to_string(dir.join("proof.out")).expect("proof file"),
            "proof-ok"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn execution_geometry_contract_carries_parallel_candidate_strategy() {
        let request = ForgeRunRequest {
            run_id: "run_strategy".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:test".to_string(),
            work_item_id: "wi".to_string(),
            intent: "Parallel candidate 2/4 for this Forge run.\nStrategy: test_first_behavior - behavior-proof-led implementation Start from tests.\n\nFix it.".to_string(),
            allowed_commands: vec!["swift test".to_string()],
            max_turns: 4,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "unknown".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
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

        let text = execution_geometry_contract(&request);

        assert!(text.contains("bounded parallel candidate"));
        assert!(text.contains(
            "Candidate strategy: test_first_behavior - behavior-proof-led implementation Start from tests."
        ));
        assert!(text.contains("Candidate comparison: Parallel candidate review rule"));
        assert!(text.contains("behavioral proof"));
        assert!(text.contains("does not widen write scope"));
    }

    #[test]
    fn principal_orientation_gate_requires_semantic_query_before_known_pack_writes() {
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_orientation_gate_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(dir.join("Sources/Canary")).expect("mkdir");
        std::fs::write(dir.join("Package.swift"), "// swift-tools-version: 5.9\n")
            .expect("package");
        std::fs::write(
            dir.join("Sources/Canary/Slugger.swift"),
            "public struct Slugger {}\n",
        )
        .expect("source");

        let mut gate = PrincipalOrientationGate::for_workspace(&dir);

        assert!(gate.required);
        assert_eq!(gate.language_pack_id, "swift-spm");
        assert!(
            gate.refusal_for("edit_file")
                .expect("edit refused")
                .contains("principal orientation required")
        );
        assert!(gate.refusal_for("run_command").is_some());
        assert!(gate.refusal_for("finish").is_some());
        assert!(gate.refusal_for("read_file").is_none());
        assert!(gate.refusal_for("semantic_query").is_none());

        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"capabilities"}),
            false,
            "semantic_query status=OK operation=capabilities",
        );
        assert!(
            gate.refusal_for("edit_file")
                .expect("capabilities alone refused")
                .contains("Capabilities and lsp_probe are readiness checks only")
        );

        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"workspace_symbol"}),
            false,
            "semantic_query status=OK operation=workspace_symbol",
        );

        assert!(gate.refusal_for("edit_file").is_none());
        assert!(gate.refusal_for("run_command").is_none());
        assert!(gate.refusal_for("finish").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn principal_orientation_gate_requires_dependency_map_for_parallel_candidates() {
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_parallel_dependency_gate_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(dir.join("Sources/Canary")).expect("mkdir");
        std::fs::write(dir.join("Package.swift"), "// swift-tools-version: 5.9\n")
            .expect("package");
        std::fs::write(
            dir.join("Sources/Canary/Slugger.swift"),
            "public struct Slugger {}\n",
        )
        .expect("source");
        let request = ForgeRunRequest {
            run_id: "parallel_run".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:test".to_string(),
            work_item_id: "wi".to_string(),
            intent: "Parallel candidate 2/4\nStrategy: test_first_behavior - behavior proof.\n\nFix Slugger.".to_string(),
            allowed_commands: vec!["swift test".to_string()],
            max_turns: 10,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "unknown".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
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
        let mut gate = PrincipalOrientationGate::for_request(&dir, &request);

        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"orientation","query":"Slugger"}),
            false,
            "semantic_query status=OK operation=orientation",
        );
        assert!(
            gate.refusal_for("edit_file")
                .expect("dependency map refused")
                .contains("parallel candidate dependency map required")
        );

        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"dependency_map","path":"Sources/Canary/Slugger.swift"}),
            false,
            "semantic_query status=OK operation=dependency_map",
        );
        assert!(gate.refusal_for("edit_file").is_none());
        assert!(gate.refusal_for("run_command").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn principal_orientation_gate_enforces_parallel_candidate_strategy_operations() {
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_parallel_strategy_gate_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(dir.join("Sources/Canary")).expect("mkdir");
        std::fs::write(dir.join("Package.swift"), "// swift-tools-version: 5.9\n")
            .expect("package");
        std::fs::write(
            dir.join("Sources/Canary/Slugger.swift"),
            "public struct Slugger {}\n",
        )
        .expect("source");
        let request = ForgeRunRequest {
            run_id: "parallel_run".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:test".to_string(),
            work_item_id: "wi".to_string(),
            intent: "Parallel candidate 2/4\nStrategy: test_first_behavior - behavior proof.\nRequired semantic operations: related_tests, references, dependency_map, diagnostics.\nProof bias: red_green_or_related_test_first.\n\nFix Slugger.".to_string(),
            allowed_commands: vec!["swift test".to_string()],
            max_turns: 10,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "unknown".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
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
        let mut gate = PrincipalOrientationGate::for_request(&dir, &request);

        for operation in ["related_tests", "references", "dependency_map"] {
            gate.observe_tool(
                "semantic_query",
                &serde_json::json!({"operation": operation}),
                false,
                &format!("semantic_query status=OK operation={operation}"),
            );
        }

        assert!(
            gate.refusal_for("edit_file")
                .expect("diagnostics still required")
                .contains("diagnostics")
        );

        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"diagnostics"}),
            false,
            "semantic_query status=OK operation=diagnostics",
        );
        assert!(gate.refusal_for("edit_file").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn principal_orientation_gate_allows_medium_edit_after_anchor_but_holds_finish_for_policy() {
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_medium_semantic_policy_gate_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(dir.join("Sources/Canary")).expect("mkdir");
        std::fs::write(dir.join("Package.swift"), "// swift-tools-version: 5.9\n")
            .expect("package");
        std::fs::write(
            dir.join("Sources/Canary/Slugger.swift"),
            "public struct Slugger {}\n",
        )
        .expect("source");
        let request = ForgeRunRequest {
            run_id: "medium_policy_run".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:test".to_string(),
            work_item_id: "wi".to_string(),
            intent: "Refactor Slugger and add coverage.".to_string(),
            allowed_commands: vec!["swift test".to_string()],
            max_turns: 10,
            revise_branch: None,
            resume: false,
            write_scope: vec!["Sources/Canary/Slugger.swift".to_string()],
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "medium".to_string(),
            semantic_policy_required_operations: vec![
                "capabilities".to_string(),
                "file_outline".to_string(),
                "related_tests".to_string(),
                "diagnostics".to_string(),
                "file_outline".to_string(),
            ],
            semantic_policy_source: "repo_intake.semantic_orientation_operations".to_string(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "single_worker".to_string(),
            execution_geometry_route: "/forge/runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        };
        let mut gate = PrincipalOrientationGate::for_request(&dir, &request);

        assert!(
            gate.refusal_for("edit_file")
                .expect("medium policy refused before semantic operations")
                .contains("principal orientation required")
        );
        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"capabilities"}),
            false,
            "semantic_query status=OK operation=capabilities",
        );
        assert!(
            gate.refusal_for("edit_file")
                .expect("capabilities do not satisfy policy")
                .contains("principal orientation required")
        );

        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"orientation"}),
            false,
            "semantic_query status=OK operation=orientation",
        );
        assert!(gate.refusal_for("edit_file").is_none());
        assert!(gate.refusal_for("run_command").is_none());

        gate.observe_tool("edit_file", &serde_json::json!({}), true, "edited");
        assert!(
            gate.refusal_for("finish")
                .expect("related tests required after edits")
                .contains("related-test orientation required")
        );

        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"related_tests"}),
            false,
            "semantic_query status=OK operation=related_tests",
        );
        assert!(
            gate.refusal_for("finish")
                .expect("structured policy still requires remaining operations")
                .contains("file_outline, diagnostics")
        );

        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"file_outline"}),
            false,
            "semantic_query status=OK operation=file_outline",
        );
        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"diagnostics"}),
            false,
            "semantic_query status=OK operation=diagnostics",
        );
        assert!(gate.refusal_for("finish").is_none());

        let policy = structured_semantic_policy_contract(&request).expect("policy contract");
        assert!(policy.contains("Structured semantic policy"));
        assert!(policy.contains("required before finish"));
        assert!(policy.contains("file_outline, related_tests, diagnostics"));
        assert!(!policy.contains("capabilities,"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn principal_orientation_gate_enforces_fleet_stream_strategy_operations() {
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_fleet_stream_strategy_gate_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(dir.join("src/main/java/app")).expect("mkdir");
        std::fs::write(dir.join("pom.xml"), "<project></project>\n").expect("pom");
        std::fs::write(
            dir.join("src/main/java/app/Service.java"),
            "package app; public class Service {}\n",
        )
        .expect("source");
        let request = ForgeRunRequest {
            run_id: "fleet_stream_run".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:test".to_string(),
            work_item_id: "wi".to_string(),
            intent: "Fleet stream s1_service: Refactor service boundary.\nFleet semantic strategy: stream_contract_guard - preserve interfaces while improving the local design Map definitions and references.\nRequired semantic operations: file_outline, definition, references, diagnostics.\nProof bias: interface_compatibility_and_behavioral_check.\n\nFix Service.".to_string(),
            allowed_commands: vec!["mvn test".to_string()],
            max_turns: 10,
            revise_branch: None,
            resume: false,
            write_scope: vec!["src/main/java/app/Service.java".to_string()],
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "unknown".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
            execution_geometry_requested_workers: 4,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "fleet_stream".to_string(),
            execution_geometry_route: "/forge/fleet-runs.json".to_string(),
        mission_id: String::new(),
        mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        };
        let mut gate = PrincipalOrientationGate::for_request(&dir, &request);

        for operation in ["file_outline", "definition", "references"] {
            gate.observe_tool(
                "semantic_query",
                &serde_json::json!({"operation": operation}),
                false,
                &format!("semantic_query status=OK operation={operation}"),
            );
        }

        assert!(
            gate.refusal_for("run_command")
                .expect("diagnostics still required")
                .contains("assigned semantic policy required")
        );
        assert!(
            gate.refusal_for("run_command")
                .expect("diagnostics named")
                .contains("diagnostics")
        );

        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"diagnostics"}),
            false,
            "semantic_query status=OK operation=diagnostics",
        );
        assert!(gate.refusal_for("run_command").is_none());
        let text = execution_geometry_contract(&request);
        assert!(text.contains("bounded stream in a wider fleet"));
        assert!(text.contains("stream_contract_guard"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn principal_orientation_gate_enforces_fleet_integration_strategy_operations() {
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_fleet_integration_strategy_gate_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(dir.join("src/main/java/app")).expect("mkdir");
        std::fs::write(dir.join("pom.xml"), "<project></project>\n").expect("pom");
        std::fs::write(
            dir.join("src/main/java/app/App.java"),
            "package app; public class App {}\n",
        )
        .expect("source");
        let request = ForgeRunRequest {
            run_id: "fleet_integration_run".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:test".to_string(),
            work_item_id: "wi".to_string(),
            intent: "Fleet f integration.\nIntegration semantic strategy: integration_contract_wiring - preserve stream interfaces.\nRequired semantic operations: file_outline, dependency_map, references, diagnostics.\nProof bias: full_suite_integration_and_contract_alignment.\n\nWire the streams.".to_string(),
            allowed_commands: vec!["mvn test".to_string()],
            max_turns: 10,
            revise_branch: Some("fleet/f".to_string()),
            resume: false,
            write_scope: vec!["pom.xml".to_string()],
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "unknown".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
            execution_geometry_requested_workers: 4,
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
        let mut gate = PrincipalOrientationGate::for_request(&dir, &request);

        for operation in ["file_outline", "dependency_map", "references"] {
            gate.observe_tool(
                "semantic_query",
                &serde_json::json!({"operation": operation}),
                false,
                &format!("semantic_query status=OK operation={operation}"),
            );
        }

        assert!(
            gate.refusal_for("run_command")
                .expect("diagnostics still required")
                .contains("assigned semantic policy required")
        );
        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"diagnostics"}),
            false,
            "semantic_query status=OK operation=diagnostics",
        );
        assert!(gate.refusal_for("run_command").is_none());
        let text = execution_geometry_contract(&request);
        assert!(text.contains("fleet integration worker"));
        assert!(text.contains("integration_contract_wiring"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn principal_orientation_gate_requires_related_tests_before_finish_after_edits() {
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_related_tests_gate_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(dir.join("Sources/Canary")).expect("mkdir");
        std::fs::write(dir.join("Package.swift"), "// swift-tools-version: 5.9\n")
            .expect("package");
        std::fs::write(
            dir.join("Sources/Canary/Slugger.swift"),
            "public struct Slugger {}\n",
        )
        .expect("source");

        let mut gate = PrincipalOrientationGate::for_workspace(&dir);
        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"workspace_symbol"}),
            false,
            "semantic_query status=OK operation=workspace_symbol",
        );
        gate.observe_tool("edit_file", &serde_json::json!({}), true, "edited source");

        assert!(gate.refusal_for("edit_file").is_none());
        assert!(gate.refusal_for("run_command").is_none());
        assert!(
            gate.refusal_for("finish")
                .expect("finish refused")
                .contains("related-test orientation required")
        );

        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"related_tests","path":"Sources/Canary/Slugger.swift"}),
            false,
            "semantic_query status=REFUSED operation=related_tests",
        );
        assert!(
            gate.refusal_for("finish")
                .expect("finish still refused")
                .contains("related-test orientation required")
        );

        gate.observe_tool(
            "semantic_query",
            &serde_json::json!({"operation":"related_tests","path":"Sources/Canary/Slugger.swift"}),
            false,
            "semantic_query status=OK operation=related_tests",
        );

        assert!(gate.refusal_for("finish").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn principal_orientation_gate_does_not_block_generic_repos() {
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_orientation_generic_{}_{}",
            std::process::id(),
            line!()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");

        let gate = PrincipalOrientationGate::for_workspace(&dir);

        assert!(!gate.required);
        assert_eq!(gate.language_pack_id, "generic");
        assert!(gate.refusal_for("edit_file").is_none());
        assert!(gate.refusal_for("run_command").is_none());
        assert!(gate.refusal_for("finish").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn allowed_command_prompt_labels_setup_before_proof() {
        let request = test_request();
        let formatted = formatted_allowed_commands(
            &[
                "npm ci".to_string(),
                "npm test".to_string(),
                "dotnet restore".to_string(),
                "dotnet test".to_string(),
            ],
            &request,
        );

        assert!(formatted.contains("- [setup] npm ci"));
        assert!(formatted.contains("- [proof] npm test"));
        assert!(formatted.contains("- [setup] dotnet restore"));
        assert!(formatted.contains("- [proof] dotnet test"));
    }

    #[test]
    fn bench_open_commands_require_bench_route() {
        let previous = set_bench_open_commands_test_override(Some(true));
        let mut governed = test_request();
        governed.execution_geometry_route = "http:/forge/runs.json".to_string();
        assert!(!bench_open_commands_enabled(&governed));

        let mut bench = test_request();
        bench.execution_geometry_route = "cli:forge-bench".to_string();
        assert!(bench_open_commands_enabled(&bench));
        set_bench_open_commands_test_override(previous);
    }

    #[test]
    fn setup_order_gate_refuses_proof_before_prior_setup_passes() {
        let allowed = vec!["npm ci".to_string(), "npm test".to_string()];
        let passed = HashSet::new();

        let reason = setup_order_refusal("npm test", &allowed, &passed).expect("refused");

        assert!(reason.contains("npm ci"));
        assert!(setup_order_refusal("npm ci", &allowed, &passed).is_none());
    }

    #[test]
    fn setup_order_gate_allows_proof_after_setup_passes() {
        let allowed = vec!["dotnet restore".to_string(), "dotnet test".to_string()];
        let mut passed = HashSet::new();
        passed.insert("dotnet restore".to_string());

        assert!(setup_order_refusal("dotnet test", &allowed, &passed).is_none());
    }

    #[test]
    fn the_workspace_confines_every_path_traversal_is_refused() {
        let dir = std::env::temp_dir().join(format!("mdx_forge_confine_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).expect("mkdir");
        std::fs::write(dir.join("inside.txt"), "in\n").expect("write");
        // A real read inside the workspace works.
        assert!(read_confined(&dir, "inside.txt").is_ok());
        // A write creates a confined file.
        assert!(write_confined(&dir, "sub/new.txt", "made\n").is_ok());
        assert!(dir.join("sub/new.txt").exists());
        // Traversal out of the workspace is refused, read and write alike.
        assert!(read_confined(&dir, "../../../etc/passwd").is_err());
        assert!(write_confined(&dir, "../escape.txt", "no").is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn edit_file_replaces_a_unique_span_and_refuses_ambiguous_or_missing() {
        let dir = std::env::temp_dir().join(format!("mdx_forge_edit_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("f.rs"), "let a = 1;\nlet b = 2;\nlet a = 1;\n").expect("write");
        // Missing span: refused with a recoverable reason.
        let missing = edit_confined(&dir, "f.rs", "let z = 9;", "let z = 0;");
        assert!(missing.unwrap_err().contains("not found"));
        // Ambiguous span (appears twice): refused, asks for more context.
        let ambiguous = edit_confined(&dir, "f.rs", "let a = 1;", "let a = 7;");
        assert!(ambiguous.unwrap_err().contains("appears 2 times"));
        // Unique span: replaced exactly once.
        edit_confined(&dir, "f.rs", "let b = 2;", "let b = 22;").expect("edit lands");
        let after = std::fs::read_to_string(dir.join("f.rs")).expect("read");
        assert!(after.contains("let b = 22;"));
        assert_eq!(after.matches("let a = 1;").count(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }

    fn assistant_tool_turn(
        call_id: &str,
        name: &str,
        args: serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({
            "role": "assistant",
            "content": "",
            "tool_calls": [{
                "id": call_id,
                "type": "function",
                "function": { "name": name, "arguments": args.to_string() }
            }]
        })
    }

    fn assistant_multi_tool_thinking_turn(call_prefix: &str) -> serde_json::Value {
        serde_json::json!({
            "role": "assistant",
            "content": "",
            "thinking_blocks": [{
                "type": "thinking",
                "thinking": "checked the patch shape",
                "signature": format!("sig_{call_prefix}")
            }],
            "tool_calls": [
                {
                    "id": format!("{call_prefix}_read"),
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "arguments": serde_json::json!({"path": "src/lib.rs"}).to_string()
                    }
                },
                {
                    "id": format!("{call_prefix}_edit"),
                    "type": "function",
                    "function": {
                        "name": "edit_file",
                        "arguments": serde_json::json!({
                            "path": "src/lib.rs",
                            "old_str": "old",
                            "new_str": "new"
                        }).to_string()
                    }
                }
            ]
        })
    }

    fn built_transcript(edit_turns: usize) -> Vec<serde_json::Value> {
        let mut messages = vec![
            serde_json::json!({ "role": "system", "content": "Do not skip checks. Do not widen scope." }),
            serde_json::json!({ "role": "user", "content": "Task: fix the failing test." }),
        ];
        for i in 0..edit_turns {
            messages.push(assistant_tool_turn(
                &format!("call_{i}"),
                "edit_file",
                serde_json::json!({ "path": format!("src/mod_{i}.rs") }),
            ));
            messages.push(tool_result_message(&format!("call_{i}"), "edit applied"));
        }
        messages
    }

    fn assert_no_orphaned_tool_results(messages: &[serde_json::Value]) {
        let mut open_call_ids: HashSet<String> = HashSet::new();
        for message in messages {
            if message["role"].as_str() == Some("assistant") {
                open_call_ids.clear();
                if let Some(calls) = message["tool_calls"].as_array() {
                    open_call_ids.extend(
                        calls
                            .iter()
                            .filter_map(|call| call["id"].as_str())
                            .map(str::to_string),
                    );
                }
            } else if message["role"].as_str() == Some("tool") {
                let call_id = message["tool_call_id"].as_str().unwrap_or("");
                assert!(
                    open_call_ids.remove(call_id),
                    "tool result {call_id} orphaned from its call"
                );
            }
        }
    }

    #[test]
    fn plan_mode_disables_write_and_proof_tools() {
        let mut req = test_request();
        req.plan_only = true;
        let mut outcome = new_outcome(&req.run_id);
        for tool in [
            "edit_file",
            "write_file",
            "apply_patch",
            "run_command",
            "shell",
        ] {
            let call = ParsedToolCall {
                call_id: "c".into(),
                name: tool.into(),
                arguments: serde_json::json!({"path":"x","command":"y","input":"z"}),
            };
            let (text, _e, detail, _p) = super::dispatch_tool(
                &call,
                std::path::Path::new("."),
                std::path::Path::new("."),
                &req,
                &mut outcome,
            );
            assert!(text.contains("disabled in plan mode"), "{tool}: {text}");
            assert!(detail.contains("plan_mode refused"));
        }
        // reads still work in plan mode (no refusal)
        let read = ParsedToolCall {
            call_id: "c".into(),
            name: "list_dir".into(),
            arguments: serde_json::json!({"path":"."}),
        };
        let (_t, _e, detail, _p) = super::dispatch_tool(
            &read,
            std::path::Path::new("."),
            std::path::Path::new("."),
            &req,
            &mut outcome,
        );
        assert!(!detail.contains("plan_mode refused"));
    }

    #[test]
    fn write_protected_reason_blocks_vcs_and_agent_config_not_source() {
        assert!(super::write_protected_reason(".git/config").is_some());
        assert!(super::write_protected_reason(".git").is_some());
        assert!(super::write_protected_reason("./.codex/agents/x.toml").is_some());
        assert!(super::write_protected_reason(".claude/settings.json").is_some());
        // real source is never blocked, incl. paths that merely contain .git as a substring
        assert!(super::write_protected_reason("src/lib.rs").is_none());
        assert!(super::write_protected_reason("gitignore_helper.py").is_none());
        assert!(super::write_protected_reason("app/.gitkeep_notreally/x").is_none());
    }

    #[test]
    fn render_plan_marks_status_and_tolerates_garbage() {
        let steps = serde_json::json!([
            {"step": "read the module", "status": "completed"},
            {"step": "make the edit", "status": "in_progress"},
            {"step": "run the test", "status": "pending"}
        ]);
        let out = super::render_plan(&steps);
        assert!(out.contains("[x] read the module"));
        assert!(out.contains("[>] make the edit"));
        assert!(out.contains("[ ] run the test"));
        assert_eq!(super::render_plan(&serde_json::json!([])), "plan cleared");
        assert_eq!(
            super::render_plan(&serde_json::json!("nope")),
            "plan updated (unreadable steps)"
        );
    }

    #[test]
    fn token_budget_triggers_before_message_count() {
        // Below threshold and below message count: no compaction.
        assert!(!super::should_compact(10, 100_000, 200_000, 32_000));
        // Over the token threshold (window 200k, reserve 32k, headroom 13k ->
        // ~155k) even with few messages: compact.
        assert!(super::should_compact(10, 160_000, 200_000, 32_000));
        // No usage reported (0 tokens) falls back to the message count.
        assert!(!super::should_compact(10, 0, 200_000, 32_000));
        assert!(super::should_compact(80, 0, 200_000, 32_000));
    }

    #[test]
    fn token_threshold_reserves_output_and_headroom() {
        // Reserve is max(max_output, 20k). With max_output 8k -> reserve 20k.
        assert_eq!(
            super::compaction_token_threshold(200_000, 8_000),
            200_000 - 20_000 - 13_000
        );
        // With max_output 40k -> reserve 40k.
        assert_eq!(
            super::compaction_token_threshold(200_000, 40_000),
            200_000 - 40_000 - 13_000
        );
        // Never collapses below a floor.
        assert_eq!(super::compaction_token_threshold(10_000, 40_000), 8_000);
    }

    #[test]
    fn recent_edits_reread_is_most_recent_first_and_bounded() {
        let dir = std::env::temp_dir().join(format!("forge-reread-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), "alpha").unwrap();
        std::fs::write(dir.join("b.txt"), "beta").unwrap();
        // edited order a then b -> reread shows b (most recent) before a.
        let out =
            super::recent_edits_reread(&dir, &["a.txt".to_string(), "b.txt".to_string()]).unwrap();
        assert!(out.find("b.txt").unwrap() < out.find("a.txt").unwrap());
        assert!(out.contains("beta") && out.contains("alpha"));
        assert!(super::recent_edits_reread(&dir, &[]).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edited_paths_of_reads_edit_write_and_patch() {
        let ef = ParsedToolCall {
            call_id: "1".into(),
            name: "edit_file".into(),
            arguments: serde_json::json!({"path": "src/x.rs"}),
        };
        assert_eq!(super::edited_paths_of(&ef), vec!["src/x.rs"]);
        let patch = ParsedToolCall {
            call_id: "2".into(),
            name: "apply_patch".into(),
            arguments: serde_json::json!({"input": "*** Begin Patch\n*** Add File: new.txt\n+hi\n*** End Patch"}),
        };
        assert_eq!(super::edited_paths_of(&patch), vec!["new.txt"]);
        let read = ParsedToolCall {
            call_id: "3".into(),
            name: "read_file".into(),
            arguments: serde_json::json!({"path": "y"}),
        };
        assert!(super::edited_paths_of(&read).is_empty());
    }

    #[test]
    fn compaction_preserves_authority_anchor_and_recent_turns() {
        let messages = built_transcript(12);
        let compaction = compact_transcript(&messages, 4).expect("compacts a long transcript");
        // Authority anchor verbatim: system prompt + task survive unchanged.
        assert_eq!(compaction.messages[0], messages[0]);
        assert_eq!(compaction.messages[1], messages[1]);
        assert_eq!(
            compaction.messages[0]["content"].as_str(),
            Some("Do not skip checks. Do not widen scope.")
        );
        // One summary message replaces the middle.
        assert_eq!(compaction.messages[2]["role"].as_str(), Some("user"));
        assert!(
            compaction.messages[2]["content"]
                .as_str()
                .unwrap()
                .contains("Compacted earlier progress")
        );
        // The last 4 turns (assistant + tool = 8 messages) are kept verbatim.
        let tail = &compaction.messages[3..];
        assert_eq!(tail, &messages[messages.len() - 8..]);
    }

    #[test]
    fn compaction_never_orphans_a_tool_result_from_its_call() {
        let messages = built_transcript(20);
        let compaction = compact_transcript(&messages, 5).expect("compacts");
        assert_no_orphaned_tool_results(&compaction.messages);
    }

    #[test]
    fn compaction_preserves_multi_tool_thinking_tail() {
        let mut messages = built_transcript(12);
        messages.push(assistant_multi_tool_thinking_turn("tail"));
        messages.push(tool_result_message("tail_read", "read ok"));
        messages.push(tool_result_message("tail_edit", "edit ok"));

        let compaction = compact_transcript(&messages, 1).expect("compacts");
        assert_no_orphaned_tool_results(&compaction.messages);
        let assistant = compaction
            .messages
            .iter()
            .find(|message| {
                message["tool_calls"]
                    .as_array()
                    .map(|calls| calls.len() == 2)
                    .unwrap_or(false)
            })
            .expect("multi-tool assistant survives in recent tail");
        assert_eq!(
            assistant["thinking_blocks"][0]["type"].as_str(),
            Some("thinking")
        );
    }

    #[test]
    fn compaction_extracts_durable_facts_before_collapsing() {
        let messages = built_transcript(10);
        let compaction = compact_transcript(&messages, 3).expect("compacts");
        // Files edited in the summarized middle are pulled out as durable
        // facts, not lost in the summary.
        assert!(
            compaction
                .files_touched
                .contains(&"src/mod_0.rs".to_string())
        );
        assert!(compaction.turns_compacted >= 7);
        let summary = compaction.messages[2]["content"].as_str().unwrap();
        assert!(summary.contains("src/mod_0.rs"));
    }

    #[test]
    fn short_transcripts_do_not_compact() {
        let messages = built_transcript(3);
        assert!(
            compact_transcript(&messages, 6).is_none(),
            "a short run keeps its full conversation"
        );
    }

    #[test]
    fn persist_run_transcript_stores_conversation_and_witnesses_the_reference() {
        use std::sync::{Arc, RwLock};
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let request = ForgeRunRequest {
            run_id: "forge_run_persist_witness_test".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:dev".to_string(),
            work_item_id: String::new(),
            intent: "fix".to_string(),
            allowed_commands: vec![],
            max_turns: 0,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "unknown".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "single_worker".to_string(),
            execution_geometry_route: "/forge/runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        };
        let messages = vec![
            serde_json::json!({ "role": "system", "content": "You are a builder." }),
            serde_json::json!({ "role": "user", "content": "Fix the failing test." }),
            serde_json::json!({ "role": "assistant", "content": "Editing lib.rs now." }),
        ];

        persist_run_transcript(&kernel, &request, &messages, 1);

        // The content is on disk and reloads by run id.
        let loaded = crate::forge_transcript_store::load_transcript_by_run(&request.run_id)
            .expect("transcript reloads by run id");
        assert_eq!(loaded, messages);

        // The receipt carries the reference and shape, never the prose.
        let guard = kernel.read().expect("kernel");
        let persisted: Vec<_> = guard
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .into_iter()
            .filter(|receipt| {
                payload_value(receipt, "event") == "transcript_persisted"
                    && payload_value(receipt, "run_id") == request.run_id
            })
            .collect();
        assert_eq!(persisted.len(), 1, "one transcript receipt for this turn");
        let transcript_ref = payload_value(persisted[0], "transcript_ref");
        assert!(
            transcript_ref.starts_with(crate::forge_transcript_store::TRANSCRIPTS_DIR),
            "receipt records the store reference, not the prose: {transcript_ref}"
        );
        assert_eq!(payload_value(persisted[0], "transcript_message_count"), "3");
        drop(guard);

        // Resume counts one prior assistant turn, so it continues from turn 1.
        let resumed_turns = loaded
            .iter()
            .filter(|message| message["role"].as_str() == Some("assistant"))
            .count() as u32;
        assert_eq!(resumed_turns, 1);

        let _ = std::fs::remove_file(format!(
            "{}/{}.json",
            crate::forge_transcript_store::TRANSCRIPTS_DIR,
            request.run_id
        ));
    }

    #[test]
    fn context_assembly_pulls_the_repos_conventions_and_always_states_repo_contract() {
        use std::sync::{Arc, RwLock};
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let dir = std::env::temp_dir().join(format!("mdx_forge_ctx_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let request = ForgeRunRequest {
            run_id: "r_ctx".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:dev".to_string(),
            work_item_id: String::new(),
            intent: "x".to_string(),
            allowed_commands: vec![],
            max_turns: 0,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "unknown".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "single_worker".to_string(),
            execution_geometry_route: "/forge/runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        };
        // No conventions, no work item: the worker still gets an explicit
        // generic repo contract, but nothing is invented beyond that.
        let empty = assemble_context(&dir, &request, &kernel);
        assert!(empty.text.contains("Repo intelligence contract"));
        assert!(empty.text.contains("Execution geometry contract"));
        assert!(empty.text.contains("Lane: single_worker"));
        assert!(empty.text.contains("Language pack: generic"));
        assert!(
            empty
                .text
                .contains("Selected proof commands: none recorded")
        );
        assert!(
            empty
                .text
                .contains("does not widen write scope, add runnable commands")
        );
        assert_eq!(empty.quality_signal_count, 0);
        assert_eq!(empty.repo_standards_source_count, 0);
        assert_eq!(empty.review_axis_count, 0);
        assert_eq!(empty.principal_review_gate_count, 0);
        assert_eq!(empty.outcome_signal_count, 0);
        assert_eq!(empty.active_memory_count, 0);
        assert_eq!(empty.installed_capability_count, 0);
        // With an AGENTS.md, the conventions are pulled in verbatim.
        std::fs::write(
            dir.join("AGENTS.md"),
            "Use tabs, never spaces. No em dashes.\n",
        )
        .expect("write");
        let ctx = assemble_context(&dir, &request, &kernel).text;
        assert!(ctx.contains("Context from MDx"));
        assert!(ctx.contains("AGENTS.md"));
        assert!(ctx.contains("Repo standards sources detected"));
        assert!(ctx.contains("No em dashes"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn context_assembly_adds_language_pack_guidance_for_known_repo_shapes() {
        use std::sync::{Arc, RwLock};
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let dir = std::env::temp_dir().join(format!(
            "mdx_forge_language_pack_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::create_dir_all(dir.join("Tests/AppTests")).expect("tests dir");
        std::fs::create_dir_all(dir.join("Sources/App")).expect("sources dir");
        std::fs::write(dir.join("Package.swift"), "// swift package\n").expect("package");
        std::fs::write(
            dir.join("Sources/App/Slugger.swift"),
            "public struct Slugger {\n    // TODO: normalize repeated separators\n    public init() {}\n}\n",
        )
        .expect("source");
        std::fs::write(
            dir.join("Tests/AppTests/SluggerTests.swift"),
            "import XCTest\nfinal class SluggerTests: XCTestCase {}\n",
        )
        .expect("tests");
        std::fs::write(dir.join(".editorconfig"), "root = true\n").expect("editorconfig");
        let request = ForgeRunRequest {
            run_id: "r_language_pack".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:dev".to_string(),
            work_item_id: String::new(),
            intent: "fix the Slugger helper".to_string(),
            allowed_commands: vec!["swift test".to_string()],
            max_turns: 0,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "unknown".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "single_worker".to_string(),
            execution_geometry_route: "/forge/runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        };

        let assembled = assemble_context(&dir, &request, &kernel);

        assert_eq!(assembled.language_pack_id, "swift-spm");
        assert_eq!(assembled.quality_signal_count, 1);
        assert_eq!(assembled.repo_standards_source_count, 1);
        assert_eq!(assembled.repo_standards_summary_count, 1);
        assert_eq!(assembled.review_axis_count, 4);
        assert_eq!(assembled.principal_review_gate_count, 6);
        assert!(assembled.semantic_anchor_count >= 3);
        assert!(assembled.semantic_tool_readiness_count >= 2);
        assert_eq!(assembled.toolchain_readiness_count, 2);
        assert!(assembled.text.contains("Repo intelligence contract"));
        assert!(assembled.text.contains("Language pack: swift-spm"));
        assert!(
            assembled
                .text
                .contains("Selected proof commands: swift test")
        );
        assert!(assembled.text.contains("Proof plan: status=ready"));
        assert!(
            assembled
                .text
                .contains("Artifact noise to ignore in review: .build/**, DerivedData/**")
        );
        assert!(
            assembled
                .text
                .contains("Language pack guidance (swift-spm)")
        );
        assert!(
            assembled
                .text
                .contains("Principal engineer review axes for this language pack")
        );
        assert!(
            assembled
                .text
                .contains("Principal review gates for this run")
        );
        assert!(
            assembled
                .text
                .contains("Semantic intelligence for this language pack")
        );
        assert!(assembled.text.contains("Semantic mission pack"));
        assert!(assembled.text.contains("Candidate 1:"));
        assert!(
            assembled
                .text
                .contains("Preflight semantic_query operation=file_outline")
        );
        assert!(
            assembled
                .text
                .contains("Preflight semantic_query operation=definition")
        );
        assert!(
            assembled
                .text
                .contains("Preflight semantic_query operation=references")
        );
        assert!(
            assembled
                .text
                .contains("Preflight semantic_query operation=related_tests")
        );
        assert!(
            assembled
                .text
                .contains("Preflight semantic_query operation=symbol_graph")
        );
        assert!(
            assembled
                .text
                .contains("Preflight semantic_query operation=hover")
        );
        assert!(
            assembled
                .text
                .contains("Preflight semantic_query operation=dependency_map")
        );
        assert!(
            assembled
                .text
                .contains("Preflight semantic_query operation=diagnostics")
        );
        assert!(
            assembled
                .text
                .contains("Intent semantic orientation: symbol_graph query=Slugger")
        );
        assert!(
            assembled
                .text
                .contains("semantic_query status=OK operation=symbol_graph")
        );
        assert!(assembled.text.contains("Slugger.swift"));
        assert!(assembled.text.contains("SluggerTests.swift"));
        assert!(assembled.text.contains("lsp:sourcekit-lsp"));
        assert!(assembled.text.contains("Semantic tool readiness:"));
        assert!(
            assembled
                .text
                .contains("Local toolchain readiness for this language pack")
        );
        assert!(assembled.text.contains("check:swift_test="));
        assert!(
            assembled
                .text
                .contains("tests:checks_cover_changed_behavior")
        );
        assert!(assembled.text.contains(".editorconfig"));
        assert!(
            assembled
                .text
                .contains("Standards source summaries: .editorconfig=style_config")
        );
        assert!(assembled.text.contains("Repo standards summaries detected"));
        assert!(assembled.text.contains("behavioral-xctest-coverage"));
        assert!(assembled.text.contains("Repo quality signals detected"));
        assert!(assembled.text.contains("tests:swiftpm-tests"));
        assert!(assembled.text.contains("Package.swift"));
        assert!(assembled.text.contains("Run swift test before finishing"));
        assert!(assembled.text.contains(".build"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn context_assembly_adds_flywheel_context_as_advisory() {
        use mdx_core::{
            ForgeOutcomeSignal, LearningJudgmentDecision, LearningMemoryActivation,
            LearningMemoryPromotion, MarketplaceAct,
        };
        use std::sync::{Arc, RwLock};
        let _env_guard = FLYWHEEL_CONTEXT_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        unsafe {
            std::env::remove_var("MDX_FORGE_FLYWHEEL_CONTEXT");
        }
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut k = kernel.write().expect("lock");
            k.run_evals_runner_agent().expect("seed receipt");
            let source_receipt_id = k
                .ledger()
                .entries()
                .first()
                .expect("source receipt")
                .receipt_id
                .clone();
            k.record_forge_outcome_signal(ForgeOutcomeSignal {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_run_done",
                source_receipt_id: &source_receipt_id,
                source_receipt_kind: "forge.run.event",
                disposition: "completed",
                summary: "UI route landed after the screenshot proof.",
                capability_ids: "svelte_ui_skill",
                model_or_worker: "local_forge_worker",
                lesson_candidate: "Run the screenshot proof for visual route work.",
                lesson_source: "",
                message_channel_id: "forge",
            })
            .expect("outcome");
            let judgment = k
                .record_learning_judgment_decision(LearningJudgmentDecision {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    judgment_id: "judgment_1",
                    promotion_id: "promotion_1",
                    decision: "promote_candidate",
                    rationale: "Screenshot proof lesson is worth promoting.",
                    evidence_refs: "receipt:forge_run_done",
                })
                .expect("judgment decision");
            let promotion = k
                .request_learning_memory_promotion(LearningMemoryPromotion {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    judgment_decision_id: &judgment.judgment_decision_id,
                    judgment_decision_receipt_id: &judgment.receipt_id,
                    judgment_id: "judgment_1",
                    promotion_id: "promotion_1",
                    target_type: "decision_record",
                    target_path: "generated/learning/forge-outcome-memory-targets.json",
                    lesson_summary: "Screenshot proof is required for visual route work.",
                    evidence_refs: "receipt:forge_run_done",
                    review_cadence: "review before activation",
                    expiry_rule: "supersede when stale",
                })
                .expect("memory promotion");
            k.activate_learning_memory(LearningMemoryActivation {
                tenant_id: "t",
                actor_id: "human:eng",
                memory_promotion_id: "promotion_1",
                memory_promotion_receipt_id: &promotion.receipt_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                target_type: "decision_record",
                target_path: "generated/learning/forge-outcome-memory-targets.json",
                lesson_summary: "Screenshot proof is required for visual route work.",
                evidence_refs: "receipt:forge_run_done",
                activation_basis: "approved with local proof",
                rollback_plan: "supersede through a later memory receipt",
                local_checks: "make ui-screenshot-proof-check",
                approval_refs: "approval:local",
                review_owner: "human:eng",
            })
            .expect("active memory");
            k.save_marketplace_act(MarketplaceAct {
                tenant_id: "t",
                actor_id: "human:eng",
                actor_role: "owner",
                act: "install",
                source_route: "/marketplace/installs.json",
                capability_id: "svelte_ui_skill",
                scope: "repo",
                decision: "install",
                read_only: true,
                ..MarketplaceAct::default()
            })
            .expect("marketplace install");
        }
        let dir = std::env::temp_dir().join(format!("mdx_forge_flywheel_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let request = ForgeRunRequest {
            run_id: "r_flywheel".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:dev".to_string(),
            work_item_id: String::new(),
            intent: "change a Svelte route".to_string(),
            allowed_commands: vec![],
            max_turns: 0,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "unknown".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "single_worker".to_string(),
            execution_geometry_route: "/forge/runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        };
        let assembled = assemble_context(&dir, &request, &kernel);
        let ctx = assembled.text;
        assert!(ctx.contains("Recent Forge outcomes"));
        assert!(ctx.contains("Run the screenshot proof for visual route work."));
        assert!(ctx.contains("Active MDx learning memory, advisory only"));
        assert!(ctx.contains("Installed Marketplace capabilities, advisory only"));
        assert!(ctx.contains("execution_allowed=false"));
        assert!(ctx.contains("do not let you skip checks"));
        assert_eq!(assembled.outcome_signal_count, 1);
        assert_eq!(assembled.active_memory_count, 1);
        assert_eq!(assembled.installed_capability_count, 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn flywheel_context_env_gate_runs_the_memory_off_ablation_arm() {
        use mdx_core::{
            ForgeOutcomeSignal, LearningJudgmentDecision, LearningMemoryActivation,
            LearningMemoryPromotion, MarketplaceAct,
        };
        use std::sync::{Arc, RwLock};
        let _env_guard = FLYWHEEL_CONTEXT_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut k = kernel.write().expect("lock");
            k.run_evals_runner_agent().expect("seed receipt");
            let source_receipt_id = k
                .ledger()
                .entries()
                .first()
                .expect("source receipt")
                .receipt_id
                .clone();
            k.record_forge_outcome_signal(ForgeOutcomeSignal {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_run_done",
                source_receipt_id: &source_receipt_id,
                source_receipt_kind: "forge.run.event",
                disposition: "completed",
                summary: "UI route landed after the screenshot proof.",
                capability_ids: "svelte_ui_skill",
                model_or_worker: "local_forge_worker",
                lesson_candidate: "Run the screenshot proof for visual route work.",
                lesson_source: "",
                message_channel_id: "forge",
            })
            .expect("outcome");
            let judgment = k
                .record_learning_judgment_decision(LearningJudgmentDecision {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    judgment_id: "judgment_1",
                    promotion_id: "promotion_1",
                    decision: "promote_candidate",
                    rationale: "Screenshot proof lesson is worth promoting.",
                    evidence_refs: "receipt:forge_run_done",
                })
                .expect("judgment decision");
            let promotion = k
                .request_learning_memory_promotion(LearningMemoryPromotion {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    judgment_decision_id: &judgment.judgment_decision_id,
                    judgment_decision_receipt_id: &judgment.receipt_id,
                    judgment_id: "judgment_1",
                    promotion_id: "promotion_1",
                    target_type: "decision_record",
                    target_path: "generated/learning/forge-outcome-memory-targets.json",
                    lesson_summary: "Screenshot proof is required for visual route work.",
                    evidence_refs: "receipt:forge_run_done",
                    review_cadence: "review before activation",
                    expiry_rule: "supersede when stale",
                })
                .expect("memory promotion");
            k.activate_learning_memory(LearningMemoryActivation {
                tenant_id: "t",
                actor_id: "human:eng",
                memory_promotion_id: "promotion_1",
                memory_promotion_receipt_id: &promotion.receipt_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                target_type: "decision_record",
                target_path: "generated/learning/forge-outcome-memory-targets.json",
                lesson_summary: "Screenshot proof is required for visual route work.",
                evidence_refs: "receipt:forge_run_done",
                activation_basis: "approved with local proof",
                rollback_plan: "supersede through a later memory receipt",
                local_checks: "make ui-screenshot-proof-check",
                approval_refs: "approval:local",
                review_owner: "human:eng",
            })
            .expect("active memory");
            k.save_marketplace_act(MarketplaceAct {
                tenant_id: "t",
                actor_id: "human:eng",
                actor_role: "owner",
                act: "install",
                source_route: "/marketplace/installs.json",
                capability_id: "svelte_ui_skill",
                scope: "repo",
                decision: "install",
                read_only: true,
                ..MarketplaceAct::default()
            })
            .expect("marketplace install");
        }
        let dir = std::env::temp_dir().join(format!("mdx_forge_ablation_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let request = ForgeRunRequest {
            run_id: "r_ablation".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:dev".to_string(),
            work_item_id: String::new(),
            intent: "change a Svelte route".to_string(),
            allowed_commands: vec![],
            max_turns: 0,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "unknown".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "single_worker".to_string(),
            execution_geometry_route: "/forge/runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        };

        // Memory-on arm (default): both flywheel blocks ride in, nothing suppressed.
        unsafe {
            std::env::remove_var("MDX_FORGE_FLYWHEEL_CONTEXT");
        }
        let on = assemble_context(&dir, &request, &kernel);
        assert!(!on.flywheel_context_suppressed);
        assert_eq!(on.outcome_signal_count, 1);
        assert_eq!(on.active_memory_count, 1);
        assert!(on.text.contains("Recent Forge outcomes"));
        assert!(
            on.text
                .contains("Active MDx learning memory, advisory only")
        );

        // Memory-off baseline arm: both flywheel blocks withheld, Marketplace kept.
        unsafe {
            std::env::set_var("MDX_FORGE_FLYWHEEL_CONTEXT", "off");
        }
        let off = assemble_context(&dir, &request, &kernel);
        unsafe {
            std::env::remove_var("MDX_FORGE_FLYWHEEL_CONTEXT");
        }
        assert!(off.flywheel_context_suppressed);
        assert_eq!(off.outcome_signal_count, 0);
        assert_eq!(off.active_memory_count, 0);
        assert!(off.matched_memory_ids.is_empty());
        assert!(!off.text.contains("Recent Forge outcomes"));
        assert!(!off.text.contains("Active MDx learning memory"));
        // The Marketplace block is not part of the flywheel ablation.
        assert_eq!(off.installed_capability_count, 1);
        assert!(
            off.text
                .contains("Installed Marketplace capabilities, advisory only")
        );

        // The off arm never runs silently: the suppression event is receipted.
        record(
            &kernel,
            &request,
            "evidence_appended",
            "flywheel context suppressed for ablation (MDX_FORGE_FLYWHEEL_CONTEXT=off): outcome-signal and active-learning-memory blocks withheld from this run",
            0,
            0,
            0,
        );
        let suppressed_receipted = {
            let k = kernel.read().expect("lock");
            k.ledger()
                .query()
                .by_kind("forge.run.event")
                .iter()
                .any(|receipt| {
                    payload_value(receipt, "detail")
                        .contains("flywheel context suppressed for ablation")
                })
        };
        assert!(suppressed_receipted);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Runs the full governed chain for one lesson - judgment, promotion,
    /// activation - with optional applicability scoping, and returns the
    /// active_memory_id the read-back selection will cite.
    fn seed_active_lesson(
        kernel: &std::sync::Arc<std::sync::RwLock<MdxKernel>>,
        tag: &str,
        lesson_summary: &str,
        work_tiers: &str,
        language_packs: &str,
    ) -> String {
        use mdx_core::{
            GovernedWriteIdentity, LearningJudgmentDecision, LearningMemoryActivation,
            LearningMemoryApplicability, LearningMemoryPromotion,
        };
        let mut k = kernel.write().expect("lock");
        let judgment = k
            .record_learning_judgment_decision(LearningJudgmentDecision {
                tenant_id: "t",
                actor_id: "human:eng",
                judgment_id: &format!("judgment_{tag}"),
                promotion_id: &format!("promotion_{tag}"),
                decision: "promote_candidate",
                rationale: "The evidence is enough to queue memory review.",
                evidence_refs: "make learning-loop-check",
            })
            .expect("judgment decision");
        let promotion = k
            .request_learning_memory_promotion(LearningMemoryPromotion {
                tenant_id: "t",
                actor_id: "human:eng",
                judgment_decision_id: &judgment.judgment_decision_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                judgment_id: &format!("judgment_{tag}"),
                promotion_id: &format!("promotion_{tag}"),
                target_type: "decision_record",
                target_path: "generated/learning/forge-outcome-memory-targets.json",
                lesson_summary,
                evidence_refs: "make learning-loop-check",
                review_cadence: "review before activation",
                expiry_rule: "supersede when stale",
            })
            .expect("memory promotion");
        let report = k
            .activate_learning_memory_with_applicability(
                LearningMemoryActivation {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    memory_promotion_id: &format!("promotion_{tag}"),
                    memory_promotion_receipt_id: &promotion.receipt_id,
                    judgment_decision_receipt_id: &judgment.receipt_id,
                    target_type: "decision_record",
                    target_path: "generated/learning/forge-outcome-memory-targets.json",
                    lesson_summary,
                    evidence_refs: &format!("receipt:{tag}"),
                    activation_basis: "approved with local proof",
                    rollback_plan: "supersede through a later memory receipt",
                    local_checks: "make learning-loop-check",
                    approval_refs: "approval:local",
                    review_owner: "human:eng",
                },
                LearningMemoryApplicability {
                    work_tiers,
                    language_packs,
                    notes: "",
                },
                &GovernedWriteIdentity::local_demo("human:eng"),
            )
            .expect("active memory");
        report.active_memory_id
    }

    fn relevance_test_request(intent: &str, work_complexity_tier: &str) -> ForgeRunRequest {
        ForgeRunRequest {
            run_id: "r_relevance".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:dev".to_string(),
            work_item_id: String::new(),
            intent: intent.to_string(),
            allowed_commands: vec![],
            max_turns: 0,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: work_complexity_tier.to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "single_worker".to_string(),
            execution_geometry_route: "/forge/runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        }
    }

    #[test]
    fn context_assembly_selects_lessons_by_relevance_and_receipts_the_passed_over() {
        use std::sync::{Arc, RwLock};
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        // Oldest first. The run below is medium tier on a generic repo with
        // the intent "tighten the memory selection loop".
        let universal_no_overlap = seed_active_lesson(
            &kernel,
            "universal_plain",
            "Keep worktrees clean after checks finish.",
            "",
            "",
        );
        let universal_overlap = seed_active_lesson(
            &kernel,
            "universal_overlap",
            "Review the memory selection loop before merging.",
            "",
            "",
        );
        let tier_matched = seed_active_lesson(
            &kernel,
            "tier_match",
            "Split medium work into checkable slices.",
            "small, medium",
            "",
        );
        let tier_mismatched = seed_active_lesson(
            &kernel,
            "tier_mismatch",
            "Plan large work with an explicit integration lap.",
            "large",
            "",
        );
        let pack_mismatched = seed_active_lesson(
            &kernel,
            "pack_mismatch",
            "Run cargo clippy before every Rust handoff.",
            "",
            "rust-cargo",
        );
        let dir = std::env::temp_dir().join(format!("mdx_forge_relevance_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let request = relevance_test_request("tighten the memory selection loop", "medium");
        let assembled = assemble_context(&dir, &request, &kernel);
        // Explicit tier match outranks universal; lexical overlap breaks the
        // tie between the two universals; mismatched lists are excluded even
        // though they are the most recent activations.
        assert_eq!(assembled.active_memory_count, 3);
        assert_eq!(
            assembled.matched_memory_ids,
            vec![
                tier_matched.clone(),
                universal_overlap.clone(),
                universal_no_overlap.clone()
            ]
        );
        assert_eq!(
            assembled.passed_over_memory_ids,
            vec![tier_mismatched, pack_mismatched]
        );
        let ctx = assembled.text;
        assert!(ctx.contains("Split medium work into checkable slices."));
        assert!(ctx.contains("Review the memory selection loop before merging."));
        assert!(ctx.contains("Keep worktrees clean after checks finish."));
        assert!(!ctx.contains("Plan large work with an explicit integration lap."));
        assert!(!ctx.contains("Run cargo clippy before every Rust handoff."));
        let tier_at = ctx
            .find("Split medium work")
            .expect("tier-matched lesson rides");
        let overlap_at = ctx
            .find("Review the memory selection loop")
            .expect("overlap lesson rides");
        let plain_at = ctx
            .find("Keep worktrees clean")
            .expect("universal lesson rides");
        assert!(tier_at < overlap_at && overlap_at < plain_at);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn context_assembly_takes_top_five_and_receipts_the_overflow() {
        use std::sync::{Arc, RwLock};
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let mut memory_ids = Vec::new();
        for index in 0..6 {
            memory_ids.push(seed_active_lesson(
                &kernel,
                &format!("overflow_{index}"),
                &format!("Prefer focused checks in slice {index}."),
                "",
                "",
            ));
        }
        let dir = std::env::temp_dir().join(format!("mdx_forge_top5_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let request = relevance_test_request("do the work", "medium");
        let assembled = assemble_context(&dir, &request, &kernel);
        // Equal scores tie-break latest first: the five most recent ride and
        // the oldest is passed over, receipted rather than silently dropped.
        assert_eq!(assembled.active_memory_count, 5);
        assert_eq!(
            assembled.passed_over_memory_ids,
            vec![memory_ids[0].clone()]
        );
        assert!(assembled.text.contains("Prefer focused checks in slice 5."));
        assert!(!assembled.text.contains("Prefer focused checks in slice 0."));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn context_assembly_holds_the_flywheel_byte_cap_over_selected_lessons() {
        use std::sync::{Arc, RwLock};
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        // Two summaries at the per-field limit overflow MAX_FLYWHEEL_BYTES
        // together; the later (higher-recency) one rides whole, the older is
        // cut at the cap.
        let older = "a".repeat(2000);
        let newer = "b".repeat(2000);
        seed_active_lesson(&kernel, "cap_older", &older, "", "");
        seed_active_lesson(&kernel, "cap_newer", &newer, "", "");
        let dir = std::env::temp_dir().join(format!("mdx_forge_cap_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let request = relevance_test_request("do the work", "medium");
        let assembled = assemble_context(&dir, &request, &kernel);
        assert_eq!(assembled.active_memory_count, 2);
        assert!(assembled.text.contains(&"b".repeat(2000)));
        assert!(!assembled.text.contains(&"a".repeat(500)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn joined_memory_ids_stay_within_one_receipt_detail() {
        assert_eq!(joined_memory_ids_or_none(&[]), "none");
        assert_eq!(
            joined_memory_ids_or_none(&["m1".to_string(), "m2".to_string()]),
            "m1,m2"
        );
        let many: Vec<String> = (0..30).map(|n| format!("m{n}")).collect();
        let joined = joined_memory_ids_or_none(&many);
        assert!(joined.ends_with(",+6_more"));
        assert!(joined.len() < 2000);
    }

    #[test]
    fn context_walks_up_from_a_work_item_to_the_bet_behind_it() {
        use mdx_core::{ForgeRunEvent, ProductBetDraft, WorkItemShape};
        use std::sync::{Arc, RwLock};
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let dir = std::env::temp_dir().join(format!("mdx_forge_bet_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        {
            let mut k = kernel.write().expect("lock");
            let identity = mdx_core::GovernedWriteIdentity::local_demo("human:dev");
            k.save_product_bet_draft_local_with_identity(
                ProductBetDraft {
                    tenant_id: "t",
                    actor_id: "human:dev",
                    bet_id: "bet_ctx",
                    bet: "Build the weekly digest",
                    for_whom: "The whole team",
                    success_metric: "60 percent open it",
                    kill_condition: "",
                    direction_ref: "",
                    slice: "",
                    slice_not: "",
                    signal_ref: "",
                },
                &identity,
            )
            .expect("bet");
            k.save_work_item_local_with_identity(
                WorkItemShape {
                    tenant_id: "t",
                    actor_id: "human:dev",
                    work_item_id: "wi_ctx",
                    title: "Draft the layout",
                    description: "",
                    bet_id: "bet_ctx",
                    owner_id: "",
                    approval: "",
                    page_ref: "",
                    build_ref: "",
                },
                &identity,
            )
            .expect("item");
            // The work item must carry the run's id for the fold; the run's
            // request names it.
            let _ = k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "human:dev",
                run_id: "r_bet",
                event: "run_started",
                work_item_id: "wi_ctx",
                detail: "accepted",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            });
        }
        let request = ForgeRunRequest {
            run_id: "r_bet".to_string(),
            tenant_id: "t".to_string(),
            actor_id: "human:dev".to_string(),
            work_item_id: "wi_ctx".to_string(),
            intent: "do the thing".to_string(),
            allowed_commands: vec![],
            max_turns: 0,
            revise_branch: None,
            resume: false,
            write_scope: Vec::new(),
            check_target_dir: None,
            builder_slot: String::new(),
            work_complexity_tier: "unknown".to_string(),
            semantic_policy_required_operations: Vec::new(),
            semantic_policy_source: "none".to_string(),
            execution_geometry_requested_workers: 1,
            execution_geometry_effective_workers: 1,
            execution_geometry_lane: "single_worker".to_string(),
            execution_geometry_route: "/forge/runs.json".to_string(),
            mission_id: String::new(),
            mission_milestone_id: String::new(),
            max_cost_cents: 0,
            max_runtime_ms: 0,
            envelope_id: String::new(),
            plan_only: false,
            reasoning_effort: String::new(),
        };
        let ctx = assemble_context(&dir, &request, &kernel).text;
        assert!(ctx.contains("The work item this serves: Draft the layout"));
        assert!(ctx.contains("The bet this work serves"));
        assert!(ctx.contains("Build the weekly digest"));
        assert!(ctx.contains("Success looks like: 60 percent open it"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn model_turn_timeout_uses_env_with_a_sane_lower_bound() {
        let _guard = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        unsafe {
            std::env::set_var("MDX_FORGE_MODEL_TURN_TIMEOUT_SECS", "45");
        }
        assert_eq!(model_turn_timeout(), Duration::from_secs(45));
        unsafe {
            std::env::set_var("MDX_FORGE_MODEL_TURN_TIMEOUT_SECS", "5");
        }
        assert_eq!(
            model_turn_timeout(),
            Duration::from_secs(DEFAULT_MODEL_TURN_TIMEOUT_SECS)
        );
        unsafe {
            std::env::remove_var("MDX_FORGE_MODEL_TURN_TIMEOUT_SECS");
        }
    }

    #[test]
    fn outstanding_model_turn_threads_are_capped_and_released() {
        let _guard = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        OUTSTANDING_MODEL_TURN_THREADS.store(0, Ordering::SeqCst);
        unsafe {
            std::env::set_var("MDX_FORGE_MAX_OUTSTANDING_MODEL_TURNS", "1");
        }
        let first = try_enter_model_turn_thread().expect("first worker enters");
        let error = try_enter_model_turn_thread().expect_err("second worker is capped");
        assert!(error.contains("worker limit reached"), "{error}");
        drop(first);
        let second = try_enter_model_turn_thread().expect("worker slot releases on drop");
        drop(second);
        unsafe {
            std::env::remove_var("MDX_FORGE_MAX_OUTSTANDING_MODEL_TURNS");
        }
        OUTSTANDING_MODEL_TURN_THREADS.store(0, Ordering::SeqCst);
    }
}
