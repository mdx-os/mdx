// The fleet plan over HTTP: a spec goes in, a decomposition comes back
// for the human to ratify. Three doors:
//
//   POST /forge/fleet-plans.json
//     {spec, actor_id, fleet_id?, bet_id?, feedback?, streams?}
//     With "streams" present this is a MANUAL plan (parsed, validated,
//     recorded - no model, no spend). Without it the planner model
//     drafts the decomposition; with fleet_id + feedback it re-plans.
//     The model call happens before any kernel lock is taken - the
//     suspend-for-external-call discipline.
//   POST /forge/fleet-plan-ratifications.json {fleet_id, reason}
//     The human yes. Mints the streams as work items.
//   GET /forge/fleet-plans/projection.json
//     Every fleet's latest plan with its streams and status.
//
// The kernel enforces the conflict law (disjoint write scopes,
// integration-owned paths); this route parses JSON and talks to the
// planner. Build spend starts only after ratification (F2).
use crate::RouteResponse;
use crate::forge_turn_client::{ModelTurn, TurnClient, with_model_turn_deadline};
use mdx_core::{
    AdvisorCall, AdvisorConsultContext, AdvisorPolicy, FleetPlanDraft, FleetStream, MdxKernel,
    RecordAdvisorCall, decide_advisor_consult, json_string_literal,
};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock, mpsc};
use std::time::{Duration, Instant, SystemTime};

/// The work class the wide-plan-review advisor consult runs under (ADR 0490).
/// A policy must allow this class for the consult to fire.
const ADVISOR_WORK_CLASS_WIDE_PLAN_REVIEW: &str = "wide_plan_review";

/// Paths only the integration phase may edit when a plan names none -
/// the registration files and generated manifests of this repo, the
/// known conflict hotspots.
const DEFAULT_INTEGRATION_OWNED: &[&str] = &[
    "crates/mdx-core/src/lib.rs",
    "crates/mdx-core/src/http_routes.rs",
    "crates/mdx-core/src/actor_admission.rs",
    "crates/mdx-server/src/main.rs",
    "crates/mdx-codegen/src/route_smoke_bodies.rs",
    "generated/",
];
const FLEET_PLAN_MAX_REQUESTED_WIDTH: u32 = 512;
const FLEET_PLAN_FIELD_STORAGE_CAP: usize = 500;
const WIDE_FLEET_REVIEW_REQUIRED_WIDTH: u32 = 8;
// How many times the planner may re-plan against its own rejection before the
// draft is refused. The planner is a model; a good one still occasionally
// returns a plan that fails the kernel's stream invariants, so it self-corrects
// with the exact reason as feedback instead of dead-ending the operator.
const PLANNER_MAX_ATTEMPTS: u32 = 3;
const DEFAULT_FLEET_REASONING_TIMEOUT_SECS: u64 = 90;
const WIDE_FLEET_REVIEW_CONCERNS_CAP: usize = 1_800;
const FLEET_PLANNING_DELAYED_AFTER: Duration = Duration::from_secs(30);
const FLEET_PLANNING_PROGRESS_TTL: Duration = Duration::from_secs(30 * 60);
const WIDE_PLAN_REVIEW_SYSTEM_PROMPT: &str = "\
You are the fresh-family senior reviewer for a wide Forge fleet plan. Challenge the decomposition \
before work scales out. Judge the plan against Forge's actual execution model.\n\n\
Forge execution model:\n\
1. Worker streams edit only their write_scope. Stream scopes must be disjoint.\n\
2. Forge also has a built-in integration phase. Files listed in integration_owned_paths are reserved \
for that integration phase and must NOT also be listed in a worker stream write_scope.\n\
3. A valid plan may use either a dedicated integration worker stream for integration files OR the \
built-in integration phase with integration_owned_paths and full_suite_checks. Do not require both.\n\
4. Existing committed integration tests can be used as proof. Do not require the plan to author new \
tests unless the stated checks cannot prove the seam.\n\
5. full_suite_checks run exactly once after all terminal green worker branches have been merged \
into the fleet branch. They do not run after each individual stream merge, and they do not run \
while later streams are still unimplemented. A finding that assumes per-stream full_suite_checks \
is incorrect and must not block the plan.\n\
6. Current live workers start from the same base branch. depends_on documents integration order and \
public seams, but a stream's own check must not require another stream's unmerged implementation. \
If one stream needs another stream's new code to pass its own check, require a different split: \
move that proof to full_suite_checks, duplicate a tiny local helper, or collapse the streams.\n\n\
Review readiness criteria:\n\
- Streams are genuinely independent for their own checks; depends_on edges do not hide a runtime \
dependency on another unmerged stream branch.\n\
- Write scopes are disjoint and do not collide with integration-owned paths.\n\
- Dependencies, public seams, and integration obligations are explicit enough for workers.\n\
- Checks prove each stream and the integrated result.\n\
- Model casting fits risk and data sensitivity.\n\n\
Answer exactly with: VERDICT: ready or VERDICT: revise. Then CONFIDENCE: high, medium, or low. \
Then up to four numbered findings. Findings must be actionable defects that would make this Forge \
execution unsafe, unprovable, or likely to fail. Do not block on optional polish, extra tests beyond \
the declared acceptance contract, or a preference for a different integration style when the plan \
uses Forge's built-in integration phase correctly.";

const PLANNER_SYSTEM_PROMPT: &str = "\
You are the fleet planner for a software build system. Given a product spec, decompose it \
into parallel build streams that independent coding agents will execute simultaneously, \
each confined to its own files.\n\n\
Laws you must obey:\n\
1. Write scopes MUST be disjoint - no two streams may touch the same file or directory, \
and no stream may touch the integration-owned paths (shared registration files); those are \
edited only by the integration phase after the streams land. Streams DECLARE what they need \
registered in their interface_contract instead.\n\
2. Decide every seam NOW: each stream's interface_contract states exactly what it provides \
and consumes (signatures, routes, payload shapes) so streams never guess about each other.\n\
3. Scale effort to complexity: use the FEWEST streams that genuinely parallelize the work. \
One stream is a valid answer for small specs - say so in the justification.\n\
4. Each stream's objective must be self-sufficient: a competent engineer who reads ONLY \
that objective plus the contracts can do the work.\n\
5. Every check command must be a SINGLE line - the runner matches allowed commands exactly, \
and multi-line commands cannot be reproduced reliably.\n\
6. A stream's own check must pass from the base repo plus that stream's write_scope only. \
depends_on may document integration order and public seams, but it does NOT make another \
stream's unmerged code available inside this stream's worker branch. Put cross-stream proof in \
full_suite_checks and integration_owned_paths, duplicate a tiny local helper, or collapse streams \
that cannot be independently verified.\n\
7. CAST each stream to a model. You are given a roster of available coders with their \
strengths and cost. For every stream choose a builder_slot from the roster and a max_turns \
sized to that model. Pick the CHEAPEST coder that can reliably do that stream. Crucially: \
break hard, cross-cutting work DOWN into leaf-sized, single-file streams a cheap coder can \
handle, and escalate to an expensive coder only for a stream that is irreducibly complex. \
The breakdown is your main lever on cost - decomposing well is cheaper than casting up.\n\
8. WEIGH STAKES, not only size. A stream that touches security, authentication, governance, \
access control, data integrity, money, or anything irreversible or hard to review is \
HIGH-STAKES - a failure there is expensive even when the code is small. For a high-stakes \
stream, reliability outweighs cost: cast a strong coder even for a tiny change, and name the \
stakes in the justification. A low-stakes, easily-reviewed stream takes the cheapest capable \
coder. Cost-minimize the ordinary work so you can afford to be careful on the dangerous work.\n\
9. CLASSIFY each stream's data_sensitivity: \"open\", \"internal\", or \"sensitive\". A stream is \
\"sensitive\" when its work would put regulated, customer, personal, secret, or otherwise \
confidential data into the model's context. A \"sensitive\" stream MUST be cast to a [sovereign] \
coder from the roster - one the data never leaves. If no [sovereign] coder is in the roster, \
still mark it \"sensitive\": the system fails closed and refuses to run it on a [frontier] coder \
rather than leak the data, and the human will connect a sovereign coder. Ordinary code work that \
touches no real data is \"internal\"; truly public work is \"open\".\n\n\
Answer with ONLY a JSON object, no prose around it:\n\
{\"streams\":[{\"stream_id\":\"s1_short_name\",\"objective\":\"...\",\"write_scope\":[\"path\", ...],\
\"interface_contract\":\"...\",\"depends_on\":[],\"checks\":[\"command\", ...],\"max_turns\":30,\
\"builder_slot\":\"\",\"data_sensitivity\":\"internal\"}, ...],\
\"integration_owned_paths\":[\"path\", ...],\"full_suite_checks\":[\"command\", ...],\
\"justification\":\"why this many streams AND why each was cast to its coder\"}";

#[derive(Clone, Debug)]
struct PlanRepoContext {
    repo_id: String,
    root: PathBuf,
    profile: crate::forge_repo_profile::ForgeRepoProfile,
    profile_source: String,
    verified_hosted_execution: bool,
}

fn plan_toolchain_readiness(context: &PlanRepoContext) -> Vec<String> {
    if context.verified_hosted_execution {
        vec![
            "execution:verified_hosted_sandbox".to_string(),
            "proof:selected_checks_run_in_hosted_sandbox".to_string(),
        ]
    } else {
        context.profile.toolchain_readiness.clone()
    }
}

fn plan_proof_status(context: &PlanRepoContext) -> &'static str {
    if context.verified_hosted_execution {
        "ready"
    } else {
        context.profile.proof_plan_status
    }
}

fn plan_proof_summary(context: &PlanRepoContext) -> String {
    if context.verified_hosted_execution {
        "The connected AgentCore environment is verified; lane and integration checks run in that hosted sandbox, not on the Render control-plane host.".to_string()
    } else {
        context.profile.proof_plan_summary.clone()
    }
}

fn default_integration_owned_paths(context: &PlanRepoContext) -> Vec<String> {
    if context.repo_id.is_empty() || context.repo_id == "mdx" {
        DEFAULT_INTEGRATION_OWNED
            .iter()
            .map(|path| path.to_string())
            .collect()
    } else {
        Vec::new()
    }
}

fn default_integration_owned_prompt(context: &PlanRepoContext) -> String {
    let paths = default_integration_owned_paths(context);
    if paths.is_empty() {
        "(none - leave integration_owned_paths empty unless repository intelligence identifies a real shared wiring file required by the planned streams)".to_string()
    } else {
        paths.join("\n")
    }
}

/// The roster of coders the planner casts streams to: the default builder
/// plus any configured named slots, each with the capability and cost
/// profile the planner reasons over. Profiles are seeded from observed runs
/// (the default coder nails leaf modules but not cross-cutting work; the top
/// model handles both but costs far more). Only slots that actually have
/// credentials configured are offered, so the planner can never cast a
/// stream to a model that cannot run - an unconfigured slot simply is not in
/// the roster, and an unknown slot degrades to the default at run time.
fn model_roster() -> String {
    let mut lines: Vec<String> = Vec::new();
    let default_model =
        std::env::var("MDX_XAI_BUILD_MODEL").unwrap_or_else(|_| "the default build model".into());
    let class = |slot: &str| TurnClient::slot_privacy_class(slot);
    lines.push(format!(
        "- builder_slot \"\" (default = {default_model}) [{}]: cheapest by far. excellent at well-scoped, single-file modules - self-contained logic plus comprehensive tests. weak at cross-cutting, multi-file features and wide refactors. give it a generous budget (max_turns 35-40); it converges slowly. cast the easy, leaf-sized majority here.",
        class("")
    ));
    for (slot, profile) in [
        (
            "SONNET",
            "strong, mid-cost. a step up for a moderately complex stream the default coder would struggle with but that does not need the top model. max_turns 20-30.",
        ),
        (
            "OPUS",
            "most capable, most expensive. handles cross-cutting multi-file work, tricky integration, and ambiguous specs; converges fast (max_turns 15-25). RESERVE for streams that are irreducibly hard - casting up is the expensive lever, breaking work down is the cheap one.",
        ),
        (
            "CODEXMINI",
            "cheap-to-mid capable coder; an alternative to the default for scoped work. max_turns 25-35.",
        ),
    ] {
        if let Ok(model) = std::env::var(format!("MDX_FLEET_BUILDER_{slot}_MODEL"))
            && !model.trim().is_empty()
        {
            lines.push(format!(
                "- builder_slot \"{slot}\" ({model}) [{}]: {profile}",
                class(slot)
            ));
        }
    }
    format!(
        "Coder roster - cast every stream's builder_slot to ONE of these EXACT slot tokens. The [class] is the data-residency class: [sovereign] runs on infrastructure the data never leaves; [frontier] is an external API:\n{}",
        lines.join("\n")
    )
}

/// Your organization's published engineering standards (Pages documents
/// titled "Standard: ..."), given to the PLANNER so the decomposition is
/// built to your rules from the first cut - not generic instinct. Each
/// stream the planner shapes can name the standard that governs it, so the
/// build agent follows and CITES it (the credibility hook: Forge builds to
/// your standards, provably). Latest revision per document; empty when none
/// are published. A short snippet of each is enough for the planner to know
/// the rule exists and route work around it.
fn org_standards_context(kernel: &Arc<RwLock<MdxKernel>>) -> String {
    const MAX_STANDARDS: usize = 8;
    const SNIPPET_CHARS: usize = 220;
    let Ok(guard) = kernel.read() else {
        return String::new();
    };
    let mut seen: Vec<String> = Vec::new();
    let mut lines: Vec<String> = Vec::new();
    for receipt in guard
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
        let snippet: String = receipt
            .payload
            .get("body_ref")
            .and_then(|body_ref| crate::pages_body_store::read_stored_body(body_ref))
            .unwrap_or_default()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(SNIPPET_CHARS)
            .collect();
        lines.push(format!("- {name}: {snippet}"));
        if lines.len() >= MAX_STANDARDS {
            break;
        }
    }
    if lines.is_empty() {
        return String::new();
    }
    format!(
        "Your organization's published engineering standards - decompose so the work RESPECTS them, and for any stream a standard governs, name that standard in the stream's objective so the build agent follows and cites it:\n{}",
        lines.join("\n")
    )
}

/// The casting feedback loop: every coder's REAL track record on this chain,
/// folded from the run receipts and SEGMENTED BY WORK TYPE so a coder's
/// record on the work it is good at is not blended with the work it is not.
/// The roster's profiles are seeded priors; the observed done-rate per
/// (model, work-type) is evidence and outweighs them. A coder that keeps
/// exhausting its budget on a kind of work should have that work broken
/// smaller or cast up; a coder with a proven done-rate on a kind of work
/// should take more of it. Empty until there are finished runs to learn
/// from. Work-type comes from the breadth of files a run edited.
fn observed_track_record(kernel: &Arc<RwLock<MdxKernel>>) -> String {
    // The fairness floor: a run that ended in fewer than this many turns
    // without finishing was starved (a throwaway probe, a tiny budget) - not
    // a fair test of the model. Crediting those would let load-test noise
    // bury a capable coder's real track record.
    const FAIR_ATTEMPT_MIN_TURNS: u64 = 5;
    let Ok(guard) = kernel.read() else {
        return String::new();
    };
    let scores = crate::forge_model_scorecard_route::model_scores_by_work_type(
        &guard,
        FAIR_ATTEMPT_MIN_TURNS,
    );
    let mut lines: Vec<String> = Vec::new();
    for ((model, work_type), score) in &scores {
        let n = score.finished();
        if n == 0 {
            continue;
        }
        // Confidence by sample size: a done-rate over one or two runs is barely
        // a signal and must not override the roster's seeded profile; only a
        // real sample is evidence to lean on. Telling the planner the
        // confidence stops it from over-fitting to noise (a single lucky or
        // unlucky run).
        let confidence = match n {
            1 => " [single run - barely a signal, trust the roster profile over this]",
            2..=4 => " [small sample - weigh lightly]",
            _ => " [solid sample]",
        };
        lines.push(format!(
            "- {model} on {work_type} work: {n} run(s), {:.0}% reached done, ~{:.0} turns avg{confidence}",
            score.done_rate() * 100.0,
            score.avg_turns(),
        ));
    }
    if lines.is_empty() {
        return String::new();
    }
    format!(
        "Observed track record on THIS chain, BY WORK TYPE (REAL outcomes - weigh above the roster's general profiles, BUT respect the confidence tag: a rate over one or two runs is noise, not evidence). Classify each stream by the breadth of its write_scope - one or two files is \"focused\", three to five is \"multi_file\", more is \"cross_cutting\" - then cast the CHEAPEST coder with a proven, well-sampled done-rate for THAT work type. If a coder keeps exhausting its budget on a work type, break that work smaller (cheap) before casting up (expensive):\n{}",
        lines.join("\n")
    )
}

/// The human-ratified lessons that may steer this casting. Each is a governed
/// fleet_casting grant an accountable person opened over an activated lesson;
/// the planner is told to prefer or avoid the runners a lesson names, and only
/// those. Empty until a lesson is granted casting authority on /learn. Read at
/// plan time so a withdrawn grant stops informing the very next plan.
fn ratified_lessons_grants(
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Vec<mdx_core::ActiveAdaptationGrant> {
    match kernel.read() {
        Ok(guard) => guard.active_fleet_casting_grants(),
        Err(_) => Vec::new(),
    }
}

fn ratified_lessons_block(grants: &[mdx_core::ActiveAdaptationGrant]) -> String {
    if grants.is_empty() {
        return String::new();
    }
    let mut lines: Vec<String> = Vec::new();
    for grant in grants {
        let slot = grant.preferred_builder_slot.trim();
        let preference = if slot.is_empty() {
            String::new()
        } else {
            format!(" (prefers the {slot} coder)")
        };
        lines.push(format!("- {}{preference}", grant.lesson_summary.trim()));
    }
    format!(
        "Human-ratified casting lessons for this plan - an accountable person approved each of these to steer casting, and ONLY casting (never model routing, budgets, checks, or review depth). Prefer or avoid the runners a lesson names when it fits a stream; otherwise cast on the evidence. Cite the lesson in the justification when it shapes a stream's coder:\n{}",
        lines.join("\n")
    )
}

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/forge/fleet-plans.json" => Some(handle_draft(method, body, kernel)),
        "/forge/fleet-plan-ratifications.json" => Some(handle_ratify(method, body, kernel)),
        "/forge/fleet-plans/projection.json" => Some(handle_projection(method, kernel)),
        _ => None,
    }
}

#[derive(Clone, Debug)]
struct FleetPlanningProgress {
    fleet_id: String,
    spec: String,
    goal: String,
    checks: Vec<String>,
    repo_id: String,
    requested_width: u32,
    status: String,
    stage: String,
    detail: String,
    planner_model: String,
    attempt: u32,
    started_at: SystemTime,
    updated_at: SystemTime,
}

struct FleetPlanningProgressGuard {
    fleet_id: String,
    clear_on_drop: bool,
}

type FleetPlanningProgressMap = std::collections::BTreeMap<String, FleetPlanningProgress>;

static FLEET_PLANNING_PROGRESS: OnceLock<Mutex<FleetPlanningProgressMap>> = OnceLock::new();

impl Drop for FleetPlanningProgressGuard {
    fn drop(&mut self) {
        if self.clear_on_drop {
            clear_fleet_planning_progress(&self.fleet_id);
        }
    }
}

impl FleetPlanningProgressGuard {
    fn update(&self, stage: &str, detail: &str, planner_model: Option<&str>, attempt: u32) {
        update_fleet_planning_progress(&self.fleet_id, stage, detail, planner_model, attempt);
    }

    fn retain_attention(&mut self, reason: &str) {
        retain_fleet_planning_attention(&self.fleet_id, reason);
        self.clear_on_drop = false;
    }
}

fn retain_fleet_planning_attention(fleet_id: &str, reason: &str) {
    update_fleet_planning_progress(fleet_id, "needs_attention", reason, None, 0);
    if let Ok(mut guard) = fleet_planning_progress_store().lock()
        && let Some(progress) = guard.get_mut(fleet_id)
    {
        progress.status = "planning_needs_attention".to_string();
    }
}

fn run_with_inherited_fleet_identity<T>(
    identity: Option<mdx_core::AdmittedIdentity>,
    run: impl FnOnce() -> T,
) -> T {
    let _identity_guard = crate::request_security::set_verified_identity(identity);
    run()
}

fn finish_background_fleet_plan(fleet_id: &str, result: Result<RouteResponse, String>) {
    let reason = match result {
        Ok(response) => serde_json::from_str::<serde_json::Value>(&response.body)
            .ok()
            .filter(|value| value["status"].as_str() == Some("REFUSED"))
            .and_then(|value| value["reason"].as_str().map(str::to_string)),
        Err(error) => Some(error),
    };
    if let Some(reason) = reason {
        retain_fleet_planning_attention(
            fleet_id,
            &format!(
                "The background planner stopped before a draft was recorded: {}",
                capped_plan_field(&reason)
            ),
        );
    }
}

fn fleet_planning_progress_store() -> &'static Mutex<FleetPlanningProgressMap> {
    FLEET_PLANNING_PROGRESS.get_or_init(|| Mutex::new(std::collections::BTreeMap::new()))
}

fn insert_fleet_planning_progress(progress: FleetPlanningProgress) {
    cleanup_fleet_planning_progress();
    if let Ok(mut guard) = fleet_planning_progress_store().lock() {
        guard.insert(progress.fleet_id.clone(), progress);
    }
}

fn begin_fleet_planning_progress(progress: FleetPlanningProgress) -> FleetPlanningProgressGuard {
    let fleet_id = progress.fleet_id.clone();
    insert_fleet_planning_progress(progress);
    FleetPlanningProgressGuard {
        fleet_id,
        clear_on_drop: true,
    }
}

fn update_fleet_planning_progress(
    fleet_id: &str,
    stage: &str,
    detail: &str,
    planner_model: Option<&str>,
    attempt: u32,
) {
    if let Ok(mut guard) = fleet_planning_progress_store().lock()
        && let Some(progress) = guard.get_mut(fleet_id)
    {
        progress.stage = stage.to_string();
        progress.detail = detail.to_string();
        if let Some(planner_model) = planner_model
            && !planner_model.trim().is_empty()
        {
            progress.planner_model = planner_model.trim().to_string();
        }
        if attempt > 0 {
            progress.attempt = attempt;
        }
        progress.updated_at = SystemTime::now();
    }
}

fn clear_fleet_planning_progress(fleet_id: &str) {
    if let Ok(mut guard) = fleet_planning_progress_store().lock() {
        guard.remove(fleet_id);
    }
}

fn cleanup_fleet_planning_progress() {
    let Ok(mut guard) = fleet_planning_progress_store().lock() else {
        return;
    };
    guard.retain(|_, progress| {
        progress
            .started_at
            .elapsed()
            .map(|age| age <= FLEET_PLANNING_PROGRESS_TTL)
            .unwrap_or(false)
    });
}

fn fleet_planning_progress_items() -> Vec<FleetPlanningProgress> {
    cleanup_fleet_planning_progress();
    let Ok(guard) = fleet_planning_progress_store().lock() else {
        return Vec::new();
    };
    guard.values().cloned().collect()
}

fn fleet_planning_id(
    fleet_id: &str,
    repo_id: &str,
    spec: &str,
    requested_width: Option<u32>,
) -> String {
    let fleet_id = fleet_id.trim();
    if !fleet_id.is_empty() {
        return fleet_id.to_string();
    }
    let seed = format!(
        "{}|{}|{}",
        repo_id.trim(),
        requested_width.unwrap_or(1),
        spec.trim()
    );
    format!("fleet_planning_{}", digest_hex(&seed))
}

fn fleet_planning_progress_from_request(
    fleet_id: &str,
    spec: &str,
    goal: &str,
    checks: &[String],
    repo_id: &str,
    requested_width: Option<u32>,
) -> FleetPlanningProgress {
    FleetPlanningProgress {
        fleet_id: fleet_planning_id(fleet_id, repo_id, spec, requested_width),
        spec: spec.to_string(),
        goal: goal.to_string(),
        checks: checks.to_vec(),
        repo_id: repo_id.to_string(),
        requested_width: requested_width.unwrap_or(1),
        status: "planning".to_string(),
        stage: "accepted".to_string(),
        detail: "Forge accepted the request and is preparing the fleet planner.".to_string(),
        planner_model: String::new(),
        attempt: 0,
        started_at: SystemTime::now(),
        updated_at: SystemTime::now(),
    }
}

fn fleet_plan_body_without_pending(parsed: &serde_json::Value) -> String {
    let mut body = parsed.clone();
    if let Some(object) = body.as_object_mut() {
        object.remove("return_pending");
    }
    serde_json::to_string(&body).unwrap_or_else(|_| "{}".to_string())
}

fn fleet_planning_accepted_json(
    progress: &FleetPlanningProgress,
    auth_session_status: &str,
) -> String {
    format!(
        r#"{{"name":"mdx-fleet-plan-local-post","status":"PLANNING","auth_session_status":{},"fleet_id":{},"stream_count":0,"requested_width":{},"plan_receipt_id":"","policy_decision_id":"","projection_route":"/forge/fleet-plans/projection.json","next_safe_action":"Forge is drafting lanes and asking for governed plan review before any workers start.","production_write_allowed":false}}"#,
        json_string_literal(auth_session_status),
        json_string_literal(&progress.fleet_id),
        progress.requested_width,
    )
}

fn pending_fleet_plan_json(progress: &FleetPlanningProgress) -> String {
    let progress = projected_fleet_planning_progress(progress);
    format!(
        r#"{{"fleet_id":{},"status":{},"spec":{},"goal":{},"checks":{},"justification":{},"planner_model":{},"bet_id":"","repo_id":{},"repo_primary_language":"","language_pack_id":"","repo_profile_source":"","repo_profile_suggested_checks":"","repo_profile_artifact_patterns":"","repo_profile_semantic_intelligence":"","repo_profile_semantic_tool_readiness":"","repo_profile_toolchain_readiness":"","repo_profile_proof_plan_status":"planning","repo_profile_proof_plan_summary":{},"stream_count":0,"streams":[],"execution_geometry":{},"wide_plan_review":{},"planning_progress":{},"plan_receipt_id":""}}"#,
        json_string_literal(&progress.fleet_id),
        json_string_literal(&progress.status),
        json_string_literal(&progress.spec),
        json_string_literal(&progress.goal),
        serde_json::to_string(&progress.checks).unwrap_or_else(|_| "[]".to_string()),
        json_string_literal(&progress.detail),
        json_string_literal(&progress.planner_model),
        json_string_literal(&progress.repo_id),
        json_string_literal(&progress.detail),
        pending_execution_geometry_json(progress.requested_width),
        pending_wide_plan_review_json(progress.requested_width),
        planning_progress_json(&progress),
    )
}

fn projected_fleet_planning_progress(progress: &FleetPlanningProgress) -> FleetPlanningProgress {
    let mut projected = progress.clone();
    let delayed = progress
        .updated_at
        .elapsed()
        .map(|elapsed| elapsed >= FLEET_PLANNING_DELAYED_AFTER)
        .unwrap_or(false);
    if delayed && progress.status == "planning" {
        projected.status = "planning_delayed".to_string();
        projected.stage = "planner_slow".to_string();
        projected.detail = "The planner is taking longer than usual. Forge is still waiting; you can keep waiting, retry with a narrower fleet, or switch to Mission if this is long-horizon work.".to_string();
    }
    projected
}

fn planning_progress_json(progress: &FleetPlanningProgress) -> String {
    let age_seconds = progress
        .started_at
        .elapsed()
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let stale_seconds = progress
        .updated_at
        .elapsed()
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    format!(
        r#"{{"stage":{},"detail":{},"planner_model":{},"attempt":{},"age_seconds":{},"stale_seconds":{},"production_write_allowed":false}}"#,
        json_string_literal(&progress.stage),
        json_string_literal(&progress.detail),
        json_string_literal(&progress.planner_model),
        progress.attempt,
        age_seconds,
        stale_seconds,
    )
}

fn pending_execution_geometry_json(requested_width: u32) -> String {
    format!(
        r#"{{"strategy":"planning","requested_width":{},"stream_count":0,"max_concurrent_workers":{},"disjoint_write_scopes_validated":false,"integration_owned_path_count":0,"stream_check_count":0,"full_suite_check_count":0,"dependency_edge_count":0,"max_turn_budget":0,"by_coder":{{}},"by_sensitivity":{{}},"production_write_allowed":false}}"#,
        requested_width,
        crate::fleet_executor::pool_status().capacity,
    )
}

fn pending_wide_plan_review_json(requested_width: u32) -> String {
    let required = requested_width >= WIDE_FLEET_REVIEW_REQUIRED_WIDTH;
    format!(
        r#"{{"required":{},"threshold":{},"status":"planning","reviewer_model":"","verdict":"not_recorded","confidence":"none","concerns":"","grants_execution_authority":false}}"#,
        required, WIDE_FLEET_REVIEW_REQUIRED_WIDTH,
    )
}

fn planning_refusal(guard: &mut Option<FleetPlanningProgressGuard>, reason: &str) -> RouteResponse {
    if let Some(guard) = guard.as_mut() {
        guard.retain_attention(reason);
    }
    refusal(reason)
}

fn fleet_reasoning_timeout() -> Duration {
    let seconds = std::env::var("MDX_FORGE_FLEET_REASONING_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| (5..=180).contains(seconds))
        .unwrap_or(DEFAULT_FLEET_REASONING_TIMEOUT_SECS);
    Duration::from_secs(seconds)
}

fn complete_fleet_reasoning_turn(
    client: &TurnClient,
    messages: &[serde_json::Value],
) -> Result<ModelTurn, String> {
    let timeout = fleet_reasoning_timeout();
    let deadline = Instant::now() + timeout;
    let client = client.clone();
    let messages = messages.to_vec();
    run_fleet_reasoning_with_timeout(timeout, move || {
        with_model_turn_deadline(deadline, || client.complete(&messages))
    })?
}

fn run_fleet_reasoning_with_timeout<T: Send + 'static>(
    timeout: Duration,
    run: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(run());
    });
    match rx.recv_timeout(timeout) {
        Ok(result) => Ok(result),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
            "model call timed out after {} seconds",
            timeout.as_secs()
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err("model call worker disconnected before returning".to_string())
        }
    }
}

fn handle_draft(
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
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::json!({}));
    let submitted_spec = parsed["spec"].as_str().unwrap_or("").trim();
    let submitted_goal = parsed["goal"].as_str().unwrap_or("").trim();
    let fleet_id = parsed["fleet_id"].as_str().unwrap_or("").trim().to_string();
    if submitted_spec.is_empty() {
        return Ok(refusal(
            "a fleet starts from a spec - describe the execution plan",
        ));
    }
    let goal = if submitted_goal.is_empty() && !fleet_id.is_empty() {
        prior_plan_goal(kernel, &resolved.tenant_id, &fleet_id)
    } else {
        submitted_goal.to_string()
    };
    if goal.is_empty() {
        return Ok(refusal("a fleet starts from a goal - say what to build"));
    }
    let spec = submitted_spec.to_string();
    let submitted_checks: Vec<String> = parsed["checks"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let requested_checks = if submitted_checks.is_empty() && !fleet_id.is_empty() {
        prior_plan_lines(kernel, &resolved.tenant_id, &fleet_id, "checks")
    } else {
        submitted_checks
    };
    let bet_id = parsed["bet_id"].as_str().unwrap_or("").trim().to_string();
    let submitted_width = match requested_width_from_plan(&parsed) {
        Ok(width) => width,
        Err(reason) => return Ok(refusal(&reason)),
    };
    // A same-tenant re-plan may refine feedback without repeating stable plan
    // metadata. Inheritance never crosses the resolved tenant boundary and
    // still produces only an unratified draft with no execution authority.
    let requested_width = submitted_width.or_else(|| {
        if fleet_id.is_empty() {
            None
        } else {
            prior_plan_requested_width(kernel, &resolved.tenant_id, &fleet_id)
        }
    });
    if let (Some(width), Some(streams)) = (requested_width, parsed["streams"].as_array())
        && streams.len() > width as usize
    {
        return Ok(refusal(&format!(
            "the plan has {} streams, which exceeds requested_width={width}",
            streams.len()
        )));
    }
    let requested_repo_id = parsed["repo_id"].as_str().unwrap_or("").trim();
    let cloud_environment_id = parsed["cloud_environment_id"].as_str().unwrap_or("").trim();
    let requested_execution_backend = parsed["execution_backend"]
        .as_str()
        .unwrap_or(if cloud_environment_id.is_empty() {
            "local"
        } else {
            "hosted_sandbox"
        })
        .trim();
    let verified_hosted_execution = matches!(
        requested_execution_backend,
        "hosted" | "hosted_sandbox" | "cloud" | "aws_sandbox"
    );
    if requested_execution_backend != "local" && !verified_hosted_execution {
        return Ok(refusal("execution_backend must be local or hosted_sandbox"));
    }
    if verified_hosted_execution {
        if requested_repo_id.is_empty() {
            return Ok(refusal(
                "hosted fleet planning requires the connected repository id",
            ));
        }
        let guard = kernel
            .read()
            .map_err(|_| "kernel lock poisoned while verifying hosted fleet plan".to_string())?;
        if let Err(reason) = crate::mobile_cloud_route::require_verified_environment(
            &guard,
            &resolved.tenant_id,
            cloud_environment_id,
            requested_repo_id,
        ) {
            return Ok(refusal(&reason));
        }
    }
    let repo_context = match plan_repo_context(
        kernel,
        requested_repo_id,
        &fleet_id,
        verified_hosted_execution,
    ) {
        Ok(context) => context,
        Err(reason) => return Ok(refusal(&reason)),
    };
    let has_manual_streams = parsed["streams"].is_array();
    let planning_progress = fleet_planning_progress_from_request(
        &fleet_id,
        &spec,
        &goal,
        &requested_checks,
        &repo_context.repo_id,
        requested_width,
    );
    if parsed["return_pending"].as_bool().unwrap_or(false) && !has_manual_streams {
        let planner_available = {
            let guard = kernel
                .read()
                .map_err(|_| "kernel lock poisoned while resolving planner".to_string())?;
            TurnClient::for_role(&guard, &resolved.tenant_id, "planner").is_some()
        };
        if !planner_available {
            return Ok(refusal(
                "no planner model is configured - connect a provider, or pass streams to plan manually",
            ));
        }
        insert_fleet_planning_progress(planning_progress.clone());
        let background_body = fleet_plan_body_without_pending(&parsed);
        let background_kernel = Arc::clone(kernel);
        let background_fleet_id = planning_progress.fleet_id.clone();
        // Trusted request identity is thread-local by design and therefore is
        // not inherited by std::thread::spawn. Carry the already-admitted
        // identity into this one background planning turn so production does
        // not silently fall back to the local tenant and fail hosted-environment
        // verification before the progress guard can report a terminal state.
        let background_identity = crate::request_security::current_verified_identity();
        std::thread::spawn(move || {
            let result = run_with_inherited_fleet_identity(background_identity, || {
                handle_draft("POST", &background_body, &background_kernel)
            });
            finish_background_fleet_plan(&background_fleet_id, result);
        });
        return Ok(RouteResponse::json(
            "202 Accepted",
            fleet_planning_accepted_json(&planning_progress, resolved.auth_session_status),
        ));
    }
    let mut planning_progress_guard = if has_manual_streams {
        None
    } else {
        Some(begin_fleet_planning_progress(planning_progress))
    };

    // Human-ratified casting lessons that may steer this plan. Read here so a
    // grant withdrawn on /learn stops informing the very next plan. Only the
    // model-drafted path can be steered; a manual plan is the human's own
    // streams and records no adaptation.
    let mut ratified_grants: Vec<mdx_core::ActiveAdaptationGrant> = Vec::new();

    // The plan body: manual streams when provided, the planner model
    // otherwise. The model call holds NO kernel lock.
    let (plan_value, planner_model) = if has_manual_streams {
        (parsed.clone(), String::new())
    } else if let Some(result) =
        exact_operator_split_plan(&goal, &repo_context, requested_width, &requested_checks)
    {
        result
    } else {
        let client = {
            let guard = kernel
                .read()
                .map_err(|_| "kernel lock poisoned while resolving planner".to_string())?;
            TurnClient::for_role(&guard, &resolved.tenant_id, "planner")
        };
        let Some(client) = client else {
            return Ok(planning_refusal(
                &mut planning_progress_guard,
                "no planner model is configured - connect a provider, or pass streams to plan manually",
            ));
        };
        if let Some(guard) = planning_progress_guard.as_ref() {
            guard.update(
                "preparing_planner",
                "Forge is gathering repo context, model roster, and prior lessons for the planner.",
                None,
                0,
            );
        }
        let feedback = parsed["feedback"].as_str().unwrap_or("").trim();
        let prior_context = if !fleet_id.is_empty() && !feedback.is_empty() {
            prior_plan_context(kernel, &resolved.tenant_id, &fleet_id)
        } else {
            String::new()
        };
        let observed = observed_track_record(kernel);
        let observed_block = if observed.is_empty() {
            String::new()
        } else {
            format!("\n\n{observed}")
        };
        ratified_grants = ratified_lessons_grants(kernel);
        let ratified_block = {
            let block = ratified_lessons_block(&ratified_grants);
            if block.is_empty() {
                String::new()
            } else {
                format!("\n\n{block}")
            }
        };
        let standards = org_standards_context(kernel);
        let standards_block = if standards.is_empty() {
            String::new()
        } else {
            format!("\n\n{standards}")
        };
        let repo_block = repo_planner_context(&repo_context);
        let width_block = requested_width
            .map(|width| {
                format!(
                    "\n\nOperator-requested execution width: {width} workers. Use at most {width} independent streams. You may use fewer when the seams are not real, but never exceed this width."
                )
            })
            .unwrap_or_default();
        let base_user = format!(
            "Spec:\n{spec}{}{}\n\n{}\n\n{}{}{}\n\nDefault integration-owned paths for this repository (use these unless the spec says otherwise):\n{}\n{}{}",
            standards_block,
            width_block,
            repo_block,
            model_roster(),
            observed_block,
            ratified_block,
            default_integration_owned_prompt(&repo_context),
            prior_context,
            if feedback.is_empty() {
                String::new()
            } else {
                format!(
                    "\nThe human reviewed the previous plan and asks:\n{feedback}\nRe-plan accordingly."
                )
            }
        );
        // Self-correct: re-plan against the exact rejection reason instead of
        // dead-ending. The planner call holds no kernel lock, so retries are
        // safe; each is a fresh model call, so this is capped.
        let mut correction = String::new();
        let mut last_error = "the planner did not return a usable plan".to_string();
        let mut planned: Option<(serde_json::Value, String)> = None;
        for attempt in 1..=PLANNER_MAX_ATTEMPTS {
            let user = if correction.is_empty() {
                base_user.clone()
            } else {
                format!(
                    "{base_user}\n\nYour previous plan was rejected: {correction}\nReturn a corrected plan. Every stream MUST declare a non-empty write_scope, and no path may appear in more than one stream's write_scope."
                )
            };
            if let Some(guard) = planning_progress_guard.as_ref() {
                guard.update(
                    "planner_drafting",
                    "The planner is drafting disjoint lanes for this fleet.",
                    None,
                    attempt,
                );
            }
            let messages = vec![
                serde_json::json!({"role": "system", "content": PLANNER_SYSTEM_PROMPT}),
                serde_json::json!({"role": "user", "content": user}),
            ];
            let turn = match complete_fleet_reasoning_turn(&client, &messages) {
                Ok(turn) => turn,
                Err(error) => {
                    last_error = format!("the planner did not answer: {error}");
                    if let Some(guard) = planning_progress_guard.as_ref() {
                        guard.update(
                            "planner_retry",
                            "The planner did not answer cleanly. Forge is retrying with the same governed request.",
                            None,
                            attempt,
                        );
                    }
                    continue;
                }
            };
            if let Some(guard) = planning_progress_guard.as_ref() {
                guard.update(
                    "checking_plan",
                    "The planner returned lanes. Forge is checking scope boundaries before recording them.",
                    Some(&turn.model_id),
                    attempt,
                );
            }
            let Some(value) = extract_json_object(&turn.text) else {
                last_error = "the planner answered, but not with a readable plan".to_string();
                correction = last_error.clone();
                if let Some(guard) = planning_progress_guard.as_ref() {
                    guard.update(
                        "planner_retry",
                        "The planner returned unreadable JSON. Forge is asking it to correct the plan.",
                        Some(&turn.model_id),
                        attempt,
                    );
                }
                continue;
            };
            if let Err(reason) = prevalidate_planner_streams_for_repo(
                &streams_from_value(&value["streams"]),
                &repo_context,
            ) {
                last_error = reason.clone();
                correction = reason;
                if let Some(guard) = planning_progress_guard.as_ref() {
                    guard.update(
                        "planner_retry",
                        "The lane split failed preflight. Forge is asking the planner to repair it.",
                        Some(&turn.model_id),
                        attempt,
                    );
                }
                continue;
            }
            planned = Some((value, turn.model_id));
            break;
        }
        match planned {
            Some(result) => result,
            None => {
                if let Some(guard) = planning_progress_guard.as_ref() {
                    guard.update(
                        "fallback_plan",
                        "The planner could not land a clean split, so Forge is drafting a conservative local plan for human review.",
                        None,
                        PLANNER_MAX_ATTEMPTS,
                    );
                }
                match fallback_plan_after_planner_failure(
                    &spec,
                    &repo_context,
                    requested_width,
                    &last_error,
                ) {
                    Ok(result) => result,
                    Err(reason) => {
                        return Ok(planning_refusal(&mut planning_progress_guard, &reason));
                    }
                }
            }
        }
    };

    let streams = streams_from_value(&plan_value["streams"]);
    let recorded_requested_width = requested_width.unwrap_or(streams.len() as u32);
    if !streams.is_empty() && streams.len() > recorded_requested_width as usize {
        return Ok(planning_refusal(
            &mut planning_progress_guard,
            &format!(
                "the plan has {} streams, which exceeds requested_width={recorded_requested_width}",
                streams.len()
            ),
        ));
    }
    let integration_owned: Vec<String> = match plan_value.get("integration_owned_paths") {
        Some(value) => string_list(value),
        None => default_integration_owned_paths(&repo_context),
    };
    let full_suite_checks = string_list(&plan_value["full_suite_checks"]);
    let justification = plan_value["justification"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    let justification = if justification.is_empty() {
        "manual plan".to_string()
    } else {
        capped_plan_field(&justification)
    };
    if let Some(guard) = planning_progress_guard.as_ref() {
        if recorded_requested_width >= WIDE_FLEET_REVIEW_REQUIRED_WIDTH {
            guard.update(
                "advisor_review",
                "A senior advisor is reviewing the wide fleet split before workers can start.",
                Some(&planner_model),
                0,
            );
        } else {
            guard.update(
                "recording_plan",
                "Forge is recording the drafted fleet plan for human review.",
                Some(&planner_model),
                0,
            );
        }
    }
    let mut wide_plan_review = review_wide_plan_if_required(WidePlanReviewRequest {
        kernel,
        tenant_id: &resolved.tenant_id,
        spec: &spec,
        repo_context: &repo_context,
        recorded_requested_width,
        streams: &streams,
        integration_owned: &integration_owned,
        full_suite_checks: &full_suite_checks,
        justification: &justification,
    });
    if let Some(guard) = planning_progress_guard.as_ref() {
        guard.update(
            "recording_plan",
            "Forge is recording the drafted fleet plan and its review evidence.",
            Some(&planner_model),
            0,
        );
    }

    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let suggested_checks = repo_context.profile.suggested_checks.join(",");
    let artifact_patterns = repo_context.profile.artifact_patterns.join(",");
    let semantic_intelligence = repo_context.profile.semantic_intelligence.join(",");
    let semantic_tool_readiness = repo_context.profile.semantic_tool_readiness.join(",");
    let toolchain_readiness = plan_toolchain_readiness(&repo_context).join(",");
    let proof_plan_status = plan_proof_status(&repo_context);
    let proof_plan_summary = plan_proof_summary(&repo_context);
    let profile_source = if repo_context.verified_hosted_execution {
        format!("{}+verified-hosted-sandbox", repo_context.profile_source)
    } else {
        repo_context.profile_source.clone()
    };
    let report = match kernel.record_fleet_plan_draft_with_identity(
        FleetPlanDraft {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            fleet_id: &fleet_id,
            spec: &spec,
            goal: &goal,
            checks: &requested_checks,
            integration_owned_paths: &integration_owned,
            full_suite_checks: &full_suite_checks,
            justification: &justification,
            requested_width: recorded_requested_width,
            planner_model: &planner_model,
            bet_id: &bet_id,
            repo_id: &repo_context.repo_id,
            repo_primary_language: repo_context.profile.primary_language,
            language_pack_id: repo_context.profile.language_pack_id,
            repo_profile_suggested_checks: &suggested_checks,
            repo_profile_artifact_patterns: &artifact_patterns,
            repo_profile_semantic_intelligence: &semantic_intelligence,
            repo_profile_semantic_tool_readiness: &semantic_tool_readiness,
            repo_profile_toolchain_readiness: &toolchain_readiness,
            repo_profile_proof_plan_status: proof_plan_status,
            repo_profile_proof_plan_summary: &proof_plan_summary,
            repo_profile_source: &profile_source,
            wide_plan_review_required: wide_plan_review.required,
            wide_plan_review_status: &wide_plan_review.status,
            wide_plan_review_reviewer_model: &wide_plan_review.reviewer_model,
            wide_plan_review_verdict: &wide_plan_review.verdict,
            wide_plan_review_confidence: &wide_plan_review.confidence,
            wide_plan_review_concerns: &wide_plan_review.concerns,
        },
        &streams,
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(planning_refusal(
                &mut planning_progress_guard,
                &error.message(),
            ));
        }
    };
    // Record the governed advisor consult for this wide plan review (ADR 0490).
    // The consult (advised or skipped) is a first-class receipt so premium
    // judgment spend is auditable. Receipt-only bookkeeping under the same write
    // lock that recorded the plan; a contract breach is logged, never fatal to
    // the plan draft.
    if let Some(consult) = wide_plan_review.advisor.as_mut() {
        match kernel.record_advisor_call(RecordAdvisorCall {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            call: &consult.call,
        }) {
            Ok(report) => consult.receipt_id = report.receipt_id,
            Err(_) => consult.receipt_id = "not_recorded_contract_breach".to_string(),
        }
    }
    // When ratified lessons steered this model-drafted plan, record that each
    // grant was applied, naming the plan it informed. Receipt-only bookkeeping
    // under the same write lock that recorded the plan.
    for grant in &ratified_grants {
        kernel.record_learning_adaptation_applied(
            mdx_core::LearningAdaptationApplied {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                grant_receipt_id: &grant.grant_receipt_id,
                artifact_kind: "fleet_plan",
                artifact_id: &report.fleet_id,
                detail: "ratified lesson informed the fleet plan casting",
            },
            &resolved.identity,
        );
    }
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-fleet-plan-local-post","status":"DRAFTED","auth_session_status":{},"fleet_id":{},"goal":{},"checks":{},"stream_count":{},"plan_receipt_id":{},"policy_decision_id":{},"streams":{},"execution_geometry":{},"justification":{},"planner_model":{},"repo_id":{},"repo_primary_language":{},"language_pack_id":{},"repo_profile_source":{},"casting_brief":{},"wide_plan_review":{},"projection_route":"/forge/fleet-plans/projection.json","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.fleet_id),
            json_string_literal(&goal),
            serde_json::to_string(&requested_checks).unwrap_or_else(|_| "[]".to_string()),
            report.stream_count,
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            streams_json(&streams),
            execution_geometry_json(
                &streams,
                recorded_requested_width,
                &integration_owned,
                &full_suite_checks
            ),
            json_string_literal(&justification),
            json_string_literal(&planner_model),
            json_string_literal(&repo_context.repo_id),
            json_string_literal(repo_context.profile.primary_language),
            json_string_literal(repo_context.profile.language_pack_id),
            json_string_literal(&repo_context.profile_source),
            casting_brief(&streams, &ratified_grants),
            wide_plan_review.json(),
        ),
    ))
}

fn capped_plan_field(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= FLEET_PLAN_FIELD_STORAGE_CAP {
        return trimmed.to_string();
    }
    let suffix = " [truncated]";
    let keep = FLEET_PLAN_FIELD_STORAGE_CAP.saturating_sub(suffix.chars().count());
    format!(
        "{}{}",
        trimmed.chars().take(keep).collect::<String>().trim_end(),
        suffix
    )
}

#[derive(Clone, Debug)]
struct WidePlanReview {
    required: bool,
    status: String,
    reviewer_model: String,
    verdict: String,
    confidence: String,
    concerns: String,
    /// The governed advisor consult for this review (ADR 0490). Present whenever
    /// a wide review ran; carries the gate decision and the consult to record.
    advisor: Option<AdvisorConsult>,
}

/// The advisor consult attached to a wide plan review: how the policy gate
/// decided, and the `AdvisorCall` to record. `receipt_id` is filled once the
/// consult is recorded under the write lock.
#[derive(Clone, Debug)]
struct AdvisorConsult {
    gate_decision: String,
    gate_reason: String,
    call: AdvisorCall,
    receipt_id: String,
}

impl WidePlanReview {
    fn not_required() -> Self {
        Self {
            required: false,
            status: "not_required".to_string(),
            reviewer_model: String::new(),
            verdict: "not_required".to_string(),
            confidence: "none".to_string(),
            concerns: "requested width is below the scrutiny threshold".to_string(),
            advisor: None,
        }
    }

    fn json(&self) -> String {
        format!(
            r#"{{"required":{},"threshold":{},"status":{},"reviewer_model":{},"verdict":{},"confidence":{},"concerns":{},"grants_execution_authority":false,"advisor":{}}}"#,
            self.required,
            WIDE_FLEET_REVIEW_REQUIRED_WIDTH,
            json_string_literal(&self.status),
            json_string_literal(&self.reviewer_model),
            json_string_literal(&self.verdict),
            json_string_literal(&self.confidence),
            json_string_literal(&self.concerns),
            self.advisor_json(),
        )
    }

    fn advisor_json(&self) -> String {
        match &self.advisor {
            None => "null".to_string(),
            Some(consult) => format!(
                r#"{{"gate_decision":{},"gate_reason":{},"trigger":{},"advisor_model_id":{},"provider_family":{},"outcome":{},"cost_cents":{},"input_tokens":{},"output_tokens":{},"receipt_id":{},"grants_execution_authority":false}}"#,
                json_string_literal(&consult.gate_decision),
                json_string_literal(&consult.gate_reason),
                json_string_literal(&consult.call.trigger),
                json_string_literal(&consult.call.advisor_model_id),
                json_string_literal(&consult.call.provider_family),
                json_string_literal(&consult.call.outcome),
                consult.call.cost_cents,
                consult.call.input_tokens,
                consult.call.output_tokens,
                json_string_literal(&consult.receipt_id),
            ),
        }
    }
}

/// Read the advisor policy from the environment. Fail-closed: unset means the
/// closed default (nothing allowed), so a consult only fires when an operator
/// has explicitly opted in. This is the local opt-in surface until a per-tenant
/// policy profile lands.
fn advisor_policy_from_env() -> AdvisorPolicy {
    let max_calls_per_run = std::env::var("MDX_ADVISOR_MAX_CALLS_PER_RUN")
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let max_premium_cents_per_mission = std::env::var("MDX_ADVISOR_MAX_PREMIUM_CENTS_PER_MISSION")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let allowed_work_classes = std::env::var("MDX_ADVISOR_ALLOWED_WORK_CLASSES")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(|entry| entry.trim().to_string())
                .filter(|entry| !entry.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    // ZDR is required unless the operator explicitly relaxes it.
    let require_zdr = !matches!(
        std::env::var("MDX_ADVISOR_REQUIRE_ZDR")
            .ok()
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("0") | Some("false") | Some("no") | Some("off")
    );
    AdvisorPolicy {
        max_calls_per_run,
        max_premium_cents_per_mission,
        allowed_work_classes,
        require_zdr,
    }
}

/// A deterministic, bounded, whitespace-free fingerprint (FNV-1a 64) so the
/// advice digest and input scope hash stay digests, never the content itself.
fn digest_hex(input: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a:{hash:016x}")
}

struct WidePlanReviewRequest<'a> {
    kernel: &'a Arc<RwLock<MdxKernel>>,
    tenant_id: &'a str,
    spec: &'a str,
    repo_context: &'a PlanRepoContext,
    recorded_requested_width: u32,
    streams: &'a [FleetStream],
    integration_owned: &'a [String],
    full_suite_checks: &'a [String],
    justification: &'a str,
}

/// Build the AdvisorCall for a consult that did not produce advice (policy
/// skipped it, no model was available, or the review call failed). Recorded, not
/// hidden: the reason CODE names why.
fn skipped_advisor_call(
    advisor_endpoint: &Option<crate::forge_model_provider_routing::ResolvedModelEndpoint>,
    input_scope_hash: &str,
    reason: &str,
) -> AdvisorCall {
    let (model, provider) = advisor_endpoint
        .as_ref()
        .map(|endpoint| (endpoint.model_id.clone(), endpoint.provider_family.clone()))
        .unwrap_or_else(|| ("unassigned".to_string(), "none".to_string()));
    AdvisorCall {
        trigger: "plan_review".to_string(),
        advisor_model_id: model,
        provider_family: provider,
        input_tokens: 0,
        output_tokens: 0,
        cost_cents: 0,
        advice_digest: String::new(),
        input_scope_hash: input_scope_hash.to_string(),
        outcome: "skipped".to_string(),
        fallback_model_id: String::new(),
        refusal_reason: if reason.is_empty() {
            "advisor_disabled_by_policy".to_string()
        } else {
            reason.to_string()
        },
    }
}

fn review_wide_plan_if_required(request: WidePlanReviewRequest<'_>) -> WidePlanReview {
    if request.recorded_requested_width < WIDE_FLEET_REVIEW_REQUIRED_WIDTH {
        return WidePlanReview::not_required();
    }
    let toolchain_readiness = plan_toolchain_readiness(request.repo_context);
    let (stream_summaries, stream_check_sets) = wide_plan_review_streams(request.streams);
    let plan_json = serde_json::json!({
        "requested_width": request.recorded_requested_width,
        "repo": {
            "repo_id": request.repo_context.repo_id,
            "primary_language": request.repo_context.profile.primary_language,
            "language_pack_id": request.repo_context.profile.language_pack_id,
            "suggested_checks": request.repo_context.profile.suggested_checks,
            "toolchain_readiness": toolchain_readiness,
        },
        "streams": stream_summaries,
        "stream_check_sets": stream_check_sets,
        "all_streams_share_one_check_set": stream_check_sets.as_array().is_some_and(|sets| sets.len() == 1),
        "integration_owned_paths": request.integration_owned,
        "full_suite_checks": request.full_suite_checks,
        "justification": request.justification,
    });
    let input_scope_hash = digest_hex(&plan_json.to_string());

    // Formalize the wide-plan reviewer as a governed advisor consult (ADR 0490):
    // resolve the advisor role, then let the fail-closed policy gate decide
    // whether premium judgment may run. When it allows, the advisor model runs
    // the review; otherwise the existing reviewer role does and the consult is
    // recorded as skipped.
    let policy = advisor_policy_from_env();
    let advisor_endpoint = match request.kernel.read() {
        Ok(kernel) => crate::forge_model_provider_routing::resolve_role_endpoint(
            &kernel,
            crate::secret_store::global(),
            request.tenant_id,
            "advisor",
            "",
        ),
        Err(_) => None,
    };
    let advisor_provider_zdr_eligible = advisor_endpoint
        .as_ref()
        .map(|endpoint| endpoint.privacy_class == "sovereign")
        .unwrap_or(false);
    // Repeated review and premium spend for the same bounded plan must shape
    // the gate. Advisor receipts do not yet carry a mission id, so the stable
    // input-scope hash is the narrow plan/run budget key. Unrelated plans must
    // not permanently exhaust a tenant's future advisor allowance.
    let (calls_made_this_run, premium_cents_spent_this_mission) = request
        .kernel
        .read()
        .map(|kernel| {
            kernel
                .ledger()
                .query()
                .by_kind(mdx_core::ADVISOR_CALL_RECEIPT_KIND)
                .iter()
                .filter(|receipt| receipt.tenant_id.as_str() == request.tenant_id)
                .fold((0_u32, 0_u64), |(calls, spend), receipt| {
                    let outcome = receipt
                        .payload
                        .get("outcome")
                        .map(String::as_str)
                        .unwrap_or("");
                    let advised = outcome != "skipped";
                    let same_scope = receipt.payload.get("input_scope_hash").map(String::as_str)
                        == Some(input_scope_hash.as_str());
                    let cost = receipt
                        .payload
                        .get("cost_cents")
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or(0);
                    (
                        calls.saturating_add(u32::from(advised && same_scope)),
                        spend.saturating_add(if advised && same_scope { cost } else { 0 }),
                    )
                })
        })
        .unwrap_or((u32::MAX, u64::MAX));
    let gate = decide_advisor_consult(
        &policy,
        &AdvisorConsultContext {
            work_class: ADVISOR_WORK_CLASS_WIDE_PLAN_REVIEW,
            calls_made_this_run,
            premium_cents_spent_this_mission,
            advisor_assigned: advisor_endpoint.is_some(),
            advisor_provider_zdr_eligible,
        },
    );
    let gate_allowed = gate.is_allowed();
    let gate_decision = if gate_allowed { "allow" } else { "skip" }.to_string();
    let gate_reason = gate.reason().to_string();
    let consult_role = if gate_allowed { "advisor" } else { "reviewer" };

    let client = {
        let Ok(kernel) = request.kernel.read() else {
            let call =
                skipped_advisor_call(&advisor_endpoint, &input_scope_hash, "consult_unavailable");
            return WidePlanReview {
                required: true,
                status: "review_unavailable".to_string(),
                reviewer_model: String::new(),
                verdict: "not_recorded".to_string(),
                confidence: "none".to_string(),
                concerns: "kernel lock unavailable while resolving reviewer".to_string(),
                advisor: Some(AdvisorConsult {
                    gate_decision,
                    gate_reason,
                    call,
                    receipt_id: String::new(),
                }),
            };
        };
        TurnClient::for_role(&kernel, request.tenant_id, consult_role)
    };
    let Some(client) = client else {
        let reason = if gate_allowed {
            "consult_unavailable"
        } else {
            gate_reason.as_str()
        };
        let call = skipped_advisor_call(&advisor_endpoint, &input_scope_hash, reason);
        return WidePlanReview {
            required: true,
            status: "missing_reviewer".to_string(),
            reviewer_model: String::new(),
            verdict: "not_recorded".to_string(),
            confidence: "none".to_string(),
            concerns: "wide fleet execution requires a reviewer model to challenge the decomposition before workers start".to_string(),
            advisor: Some(AdvisorConsult {
                gate_decision,
                gate_reason,
                call,
                receipt_id: String::new(),
            }),
        };
    };
    let messages = vec![
        serde_json::json!({"role":"system","content":WIDE_PLAN_REVIEW_SYSTEM_PROMPT}),
        serde_json::json!({"role":"user","content":format!("Spec:\n{}\n\nFleet plan JSON:\n{plan_json}", request.spec)}),
    ];
    match complete_fleet_reasoning_turn(&client, &messages) {
        Ok(turn) => {
            let text = turn.text.trim();
            let verdict = parse_plan_review_verdict(text).to_string();
            let call = if gate_allowed {
                let cost_cents = crate::forge_model_pricing::microcents_to_cents_rounded_up(
                    crate::forge_model_pricing::turn_cost_microcents(
                        &turn.model_id,
                        turn.input_tokens,
                        turn.output_tokens,
                    ),
                );
                AdvisorCall {
                    trigger: "plan_review".to_string(),
                    advisor_model_id: turn.model_id.clone(),
                    provider_family: advisor_endpoint
                        .as_ref()
                        .map(|endpoint| endpoint.provider_family.clone())
                        .unwrap_or_else(|| "none".to_string()),
                    input_tokens: turn.input_tokens,
                    output_tokens: turn.output_tokens,
                    cost_cents,
                    advice_digest: digest_hex(&format!("{verdict}|{text}")),
                    input_scope_hash: input_scope_hash.clone(),
                    outcome: "advised".to_string(),
                    fallback_model_id: String::new(),
                    refusal_reason: String::new(),
                }
            } else {
                skipped_advisor_call(&advisor_endpoint, &input_scope_hash, &gate_reason)
            };
            WidePlanReview {
                required: true,
                status: "recorded".to_string(),
                reviewer_model: turn.model_id,
                verdict,
                confidence: parse_plan_review_confidence(text).to_string(),
                concerns: text.chars().take(WIDE_FLEET_REVIEW_CONCERNS_CAP).collect(),
                advisor: Some(AdvisorConsult {
                    gate_decision,
                    gate_reason,
                    call,
                    receipt_id: String::new(),
                }),
            }
        }
        Err(error) => {
            let call =
                skipped_advisor_call(&advisor_endpoint, &input_scope_hash, "consult_unavailable");
            WidePlanReview {
                required: true,
                status: "review_unavailable".to_string(),
                reviewer_model: client.model_id().to_string(),
                verdict: "not_recorded".to_string(),
                confidence: "none".to_string(),
                concerns: error.chars().take(WIDE_FLEET_REVIEW_CONCERNS_CAP).collect(),
                advisor: Some(AdvisorConsult {
                    gate_decision,
                    gate_reason,
                    call,
                    receipt_id: String::new(),
                }),
            }
        }
    }
}

fn wide_plan_review_streams(streams: &[FleetStream]) -> (serde_json::Value, serde_json::Value) {
    let mut check_sets: Vec<Vec<String>> = Vec::new();
    let summaries = streams
        .iter()
        .map(|stream| {
            let check_set_index = check_sets
                .iter()
                .position(|checks| checks == &stream.checks)
                .unwrap_or_else(|| {
                    check_sets.push(stream.checks.clone());
                    check_sets.len() - 1
                });
            serde_json::json!({
                "stream_id": stream.stream_id,
                "objective": capped_plan_field(&stream.objective),
                "write_scope": stream.write_scope,
                "interface_contract": capped_plan_field(&stream.interface_contract),
                "depends_on": stream.depends_on,
                "check_set_index": check_set_index,
                "max_turns": stream.max_turns,
                "builder_slot": stream.builder_slot,
                "data_sensitivity": stream.data_sensitivity,
            })
        })
        .collect::<Vec<_>>();
    (
        serde_json::Value::Array(summaries),
        serde_json::to_value(check_sets).unwrap_or_else(|_| serde_json::json!([])),
    )
}

fn parse_plan_review_verdict(text: &str) -> &'static str {
    let lower = text.to_ascii_lowercase();
    if lower.contains("verdict: ready") {
        "ready"
    } else if lower.contains("verdict: revise") || lower.contains("verdict: needs work") {
        "revise"
    } else {
        "unclear"
    }
}

fn parse_plan_review_confidence(text: &str) -> &'static str {
    let lower = text.to_ascii_lowercase();
    if lower.contains("confidence: high") {
        "high"
    } else if lower.contains("confidence: medium") {
        "medium"
    } else if lower.contains("confidence: low") {
        "low"
    } else {
        "unknown"
    }
}

fn plan_repo_context(
    kernel: &Arc<RwLock<MdxKernel>>,
    requested_repo_id: &str,
    fleet_id: &str,
    verified_hosted_execution: bool,
) -> Result<PlanRepoContext, String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let repo_id = if !requested_repo_id.trim().is_empty() {
        requested_repo_id.trim().to_string()
    } else if !fleet_id.trim().is_empty() {
        kernel
            .latest_fleet_plan(fleet_id.trim())
            .and_then(|plan| plan.get("repo_id").cloned())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let (root, profile_source) = if repo_id.is_empty() {
        (repo_root(), "live-mdx-profile".to_string())
    } else {
        let Some(root) = kernel.forge_repo_root(&repo_id) else {
            return Err(format!(
                "connect repo {repo_id} before planning a fleet against it"
            ));
        };
        let source = if kernel.forge_repo_index_profile_json(&repo_id).is_some() {
            "repo-index"
        } else {
            "live-profile"
        };
        (PathBuf::from(root), source.to_string())
    };
    let profile = crate::forge_repo_profile::profile_repo(&root);
    Ok(PlanRepoContext {
        repo_id,
        root,
        profile,
        profile_source,
        verified_hosted_execution,
    })
}

fn requested_width_from_plan(parsed: &serde_json::Value) -> Result<Option<u32>, String> {
    let value = parsed["requested_width"]
        .as_u64()
        .or_else(|| parsed["fleet_width"].as_u64())
        .or_else(|| parsed["worker_count"].as_u64());
    let Some(value) = value else {
        return Ok(None);
    };
    if value == 0 {
        return Err("requested_width must be at least 1 worker".to_string());
    }
    if value > FLEET_PLAN_MAX_REQUESTED_WIDTH as u64 {
        return Err(format!(
            "requested_width={value} exceeds the current governed fleet ceiling of {FLEET_PLAN_MAX_REQUESTED_WIDTH} workers"
        ));
    }
    Ok(Some(value as u32))
}

fn repo_planner_context(context: &PlanRepoContext) -> String {
    let repo_label = if context.repo_id.is_empty() {
        "MDx itself".to_string()
    } else {
        context.repo_id.clone()
    };
    let inventory = repository_path_inventory(&context.root);
    let toolchain_readiness = plan_toolchain_readiness(context);
    let proof_status = plan_proof_status(context);
    let proof_summary = plan_proof_summary(context);
    format!(
        "Repository intelligence for this fleet plan - use it to choose seams, proof commands, review gates, and artifact exclusions. This context does not grant write authority or bypass disjoint scopes.\n\
- repo: {repo_label}\n\
- root: {}\n\
- primary_language: {}\n\
- language_pack_id: {}\n\
- suggested_checks: {}\n\
- proof_plan: {} - {}\n\
- semantic_intelligence: {}\n\
- semantic_tool_readiness: {}\n\
- toolchain_readiness: {}\n\
- artifact_patterns_to_avoid_in_review: {}\n\
- source: {}\n\
- bounded path inventory (existing repository paths; do not claim an existing file outside this inventory):\n{}",
        context.root.display(),
        context.profile.primary_language,
        context.profile.language_pack_id,
        join_strings(&context.profile.suggested_checks),
        proof_status,
        proof_summary,
        join_static(&context.profile.semantic_intelligence),
        join_strings(&context.profile.semantic_tool_readiness),
        join_strings(&toolchain_readiness),
        join_static(&context.profile.artifact_patterns),
        context.profile_source,
        inventory,
    )
}

const REPO_PATH_INVENTORY_CAP: usize = 500;
const REPO_PATH_INVENTORY_DEPTH: usize = 6;

fn repository_path_inventory(root: &Path) -> String {
    let mut paths = Vec::new();
    collect_repository_paths(root, root, 0, &mut paths);
    paths.sort();
    paths.truncate(REPO_PATH_INVENTORY_CAP);
    if paths.is_empty() {
        "(inventory unavailable - prefer scopes explicitly named by the operator)".to_string()
    } else {
        paths.join("\n")
    }
}

fn collect_repository_paths(root: &Path, dir: &Path, depth: usize, paths: &mut Vec<String>) {
    if depth > REPO_PATH_INVENTORY_DEPTH || paths.len() >= REPO_PATH_INVENTORY_CAP {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        if paths.len() >= REPO_PATH_INVENTORY_CAP {
            break;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(
            name.as_ref(),
            ".git" | ".mdx-local" | "node_modules" | "target" | "dist" | "build"
        ) {
            continue;
        }
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let display = relative.to_string_lossy().replace('\\', "/");
        if file_type.is_dir() {
            paths.push(format!("{display}/"));
            collect_repository_paths(root, &path, depth + 1, paths);
        } else if file_type.is_file() {
            paths.push(display);
        }
    }
}

fn fallback_plan_after_planner_failure(
    spec: &str,
    repo_context: &PlanRepoContext,
    requested_width: Option<u32>,
    last_error: &str,
) -> Result<(serde_json::Value, String), String> {
    let width = requested_width.unwrap_or(1).max(1);
    let integration_owned_paths = default_integration_owned_paths(repo_context);
    let scopes = fallback_write_scopes(repo_context, width as usize, &integration_owned_paths);
    if scopes.is_empty() {
        return Err(format!(
            "{last_error}; local fallback could not find safe disjoint repo scopes"
        ));
    }
    let checks = fallback_checks(repo_context);
    let check_json = serde_json::Value::Array(
        checks
            .iter()
            .map(|check| serde_json::Value::String(check.clone()))
            .collect(),
    );
    let streams = scopes
        .iter()
        .enumerate()
        .map(|(index, scope)| {
            let label = fallback_stream_label(scope);
            serde_json::json!({
                "stream_id": format!("s{}_{}", index + 1, label),
                "objective": format!("Work only inside {scope} toward the operator request: {}", capped_plan_field(spec)),
                "write_scope": [scope],
                "interface_contract": "Declare shared registrations, generated outputs, route wiring, and cross-scope assumptions for the integration phase. Do not edit integration-owned paths from this stream.",
                "depends_on": [],
                "checks": check_json.clone(),
                "max_turns": 35,
                "builder_slot": "",
                "data_sensitivity": "internal",
            })
        })
        .collect::<Vec<_>>();
    let value = serde_json::json!({
        "streams": streams,
        "integration_owned_paths": integration_owned_paths,
        "full_suite_checks": checks,
        "justification": format!(
            "Local fallback split after planner failure: {last_error}. This conservative draft is repo-aware, disjoint by top-level scope, and must be human-reviewed before any workers start."
        ),
    });
    prevalidate_planner_streams(&streams_from_value(&value["streams"]))?;
    Ok((value, "mdx-local-fleet-fallback".to_string()))
}

fn exact_operator_split_plan(
    spec: &str,
    repo_context: &PlanRepoContext,
    requested_width: Option<u32>,
    requested_checks: &[String],
) -> Option<(serde_json::Value, String)> {
    let width = requested_width? as usize;
    if width < 2 {
        return None;
    }
    let words = spec.split_whitespace().collect::<Vec<_>>();
    let through = words
        .iter()
        .position(|word| word.eq_ignore_ascii_case("through"))?;
    let start = clean_operator_path(words.get(through.checked_sub(1)?)?);
    let end = clean_operator_path(words.get(through + 1)?);
    let (start_prefix, start_number, start_digits, start_suffix) = numbered_path_parts(&start)?;
    let (end_prefix, end_number, end_digits, end_suffix) = numbered_path_parts(&end)?;
    if start_prefix != end_prefix
        || start_suffix != end_suffix
        || start_digits != end_digits
        || end_number < start_number
        || end_number.saturating_sub(start_number) as usize + 1 != width
    {
        return None;
    }

    let checks = if requested_checks.is_empty() {
        fallback_checks(repo_context)
    } else {
        requested_checks.to_vec()
    };
    let streams = (start_number..=end_number)
        .enumerate()
        .map(|(index, number)| {
            let path = format!(
                "{start_prefix}{number:0start_digits$}{start_suffix}",
                start_digits = start_digits
            );
            let parent_is_real = repo_context
                .root
                .join(&path)
                .parent()
                .is_some_and(Path::is_dir);
            if !parent_is_real {
                return None;
            }
            Some(serde_json::json!({
                "stream_id": format!("lane_{:02}", index + 1),
                "objective": format!("Complete only the operator-assigned scope {path}: {}", capped_plan_field(spec)),
                "write_scope": [path],
                "interface_contract": "This lane is independent. Do not edit shared, generated, integration-owned, manifest, lock, workflow, or release files.",
                "depends_on": [],
                "checks": checks.clone(),
                "max_turns": 12,
                "builder_slot": "",
                "data_sensitivity": "internal",
            }))
        })
        .collect::<Option<Vec<_>>>()?;
    let integration_owned_paths = default_integration_owned_paths(repo_context);
    let value = serde_json::json!({
        "streams": streams,
        "integration_owned_paths": integration_owned_paths,
        "full_suite_checks": checks,
        "justification": format!(
            "The operator supplied an exact {width}-path range with one disjoint scope per lane. Forge preserved that split directly and still requires wide-plan review plus human ratification before execution."
        ),
    });
    let parsed_streams = streams_from_value(&value["streams"]);
    prevalidate_planner_streams_for_repo(&parsed_streams, repo_context).ok()?;
    Some((value, "mdx-exact-operator-split".to_string()))
}

fn clean_operator_path(value: &str) -> String {
    value
        .trim_matches(|character: char| {
            character.is_ascii_whitespace()
                || matches!(
                    character,
                    '`' | '"' | '\'' | '(' | ')' | '[' | ']' | ',' | ';' | ':'
                )
        })
        .trim_end_matches(['.', '!', '?'])
        .to_string()
}

fn numbered_path_parts(path: &str) -> Option<(String, u32, usize, String)> {
    if path.is_empty()
        || Path::new(path).is_absolute()
        || Path::new(path).components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return None;
    }
    let bytes = path.as_bytes();
    let end = bytes.iter().rposition(u8::is_ascii_digit)? + 1;
    let start = bytes[..end]
        .iter()
        .rposition(|byte| !byte.is_ascii_digit())
        .map(|index| index + 1)
        .unwrap_or(0);
    let digits = &path[start..end];
    Some((
        path[..start].to_string(),
        digits.parse().ok()?,
        digits.len(),
        path[end..].to_string(),
    ))
}

fn fallback_checks(context: &PlanRepoContext) -> Vec<String> {
    let checks: Vec<String> = context
        .profile
        .suggested_checks
        .iter()
        .map(|check| check.trim())
        .filter(|check| !check.is_empty() && !check.contains('\n'))
        .take(2)
        .map(str::to_string)
        .collect();
    if checks.is_empty() {
        vec!["make local-smoke".to_string()]
    } else {
        checks
    }
}

fn fallback_write_scopes(
    context: &PlanRepoContext,
    max_scopes: usize,
    integration_owned_paths: &[String],
) -> Vec<String> {
    let preferred = [
        "apps/mdx-operator-macos/",
        "apps/mdx-host/src/routes/forge/",
        "apps/mdx-host/src/routes/admin/forge/",
        "apps/mdx-host/src/lib/",
        "crates/mdx-server/src/fleet_plan_route.rs",
        "crates/mdx-server/src/forge_run_route.rs",
        "crates/mdx-core/src/forge_run.rs",
        "scripts/",
        "docs/",
    ];
    let mut scopes = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for scope in preferred {
        push_fallback_scope(
            context,
            scope,
            &mut scopes,
            &mut seen,
            max_scopes,
            integration_owned_paths,
        );
    }
    if scopes.len() >= max_scopes {
        return scopes;
    }
    if let Ok(entries) = std::fs::read_dir(&context.root) {
        let mut names = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || matches!(name.as_str(), "target" | "node_modules") {
                    return None;
                }
                let path = entry.path();
                if path.is_dir() {
                    Some(format!("{name}/"))
                } else if path.is_file() {
                    Some(name)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        names.sort();
        for scope in names {
            push_fallback_scope(
                context,
                &scope,
                &mut scopes,
                &mut seen,
                max_scopes,
                integration_owned_paths,
            );
        }
    }
    scopes
}

fn push_fallback_scope(
    context: &PlanRepoContext,
    scope: &str,
    scopes: &mut Vec<String>,
    seen: &mut std::collections::BTreeSet<String>,
    max_scopes: usize,
    integration_owned_paths: &[String],
) {
    if scopes.len() >= max_scopes {
        return;
    }
    let normalized = scope.trim().trim_start_matches("./").to_string();
    if normalized.is_empty()
        || fallback_scope_conflicts_with_integration(&normalized, integration_owned_paths)
        || !seen.insert(normalized.clone())
    {
        return;
    }
    let local = normalized.trim_end_matches('/');
    if context.root.join(local).exists() {
        scopes.push(normalized);
    }
}

fn fallback_scope_conflicts_with_integration(
    scope: &str,
    integration_owned_paths: &[String],
) -> bool {
    integration_owned_paths
        .iter()
        .any(|owned| fallback_paths_overlap(scope, owned))
}

fn fallback_paths_overlap(a: &str, b: &str) -> bool {
    let a = a.trim().trim_end_matches('/');
    let b = b.trim().trim_end_matches('/');
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}

fn fallback_stream_label(scope: &str) -> String {
    let mut label: String = scope
        .trim_matches('/')
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    while label.contains("__") {
        label = label.replace("__", "_");
    }
    let label = label.trim_matches('_');
    if label.is_empty() {
        "repo".to_string()
    } else {
        label.chars().take(36).collect()
    }
}

fn join_static(items: &[&str]) -> String {
    if items.is_empty() {
        "none detected".to_string()
    } else {
        items.join(", ")
    }
}

fn join_strings(items: &[String]) -> String {
    if items.is_empty() {
        "none detected".to_string()
    } else {
        items.join(", ")
    }
}

fn repo_root() -> std::path::PathBuf {
    std::env::current_dir()
        .ok()
        .and_then(|dir| {
            dir.ancestors()
                .find(|candidate| candidate.join(".git").exists())
                .map(std::path::Path::to_path_buf)
        })
        .unwrap_or_else(|| std::path::PathBuf::from("."))
}

/// A structured summary of the plan's casting, so the human ratifying sees
/// the whole picture before saying yes: how many streams go to each coder,
/// the data-sensitivity mix, and whether a sensitive stream will convene the
/// review panel. Informed ratification is the governance point - the human
/// decides with the harness's reasoning in view, not blind.
fn casting_brief(
    streams: &[FleetStream],
    ratified_grants: &[mdx_core::ActiveAdaptationGrant],
) -> String {
    use std::collections::BTreeMap;
    let mut by_coder: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_sensitivity: BTreeMap<String, usize> = BTreeMap::new();
    let mut any_sensitive = false;
    for stream in streams {
        let coder = if stream.builder_slot.trim().is_empty() {
            "default".to_string()
        } else {
            stream.builder_slot.trim().to_string()
        };
        *by_coder.entry(coder).or_default() += 1;
        let sensitivity = if stream.data_sensitivity.trim().is_empty() {
            "internal".to_string()
        } else {
            stream.data_sensitivity.trim().to_string()
        };
        if sensitivity.eq_ignore_ascii_case("sensitive") {
            any_sensitive = true;
        }
        *by_sensitivity.entry(sensitivity).or_default() += 1;
    }
    let obj = |map: &BTreeMap<String, usize>| {
        let pairs: Vec<String> = map
            .iter()
            .map(|(k, v)| format!("{}:{v}", json_string_literal(k)))
            .collect();
        format!("{{{}}}", pairs.join(","))
    };
    // The human-ratified lessons that steered this plan, so the person
    // ratifying sees which granted lessons informed the casting before saying
    // yes. Empty when no lesson has been granted casting authority.
    let ratified_lessons: Vec<String> = ratified_grants
        .iter()
        .map(|grant| {
            format!(
                r#"{{"grant_receipt_id":{},"lesson_summary":{},"preferred_builder_slot":{},"review_owner":{}}}"#,
                json_string_literal(&grant.grant_receipt_id),
                json_string_literal(&grant.lesson_summary),
                json_string_literal(&grant.preferred_builder_slot),
                json_string_literal(&grant.review_owner),
            )
        })
        .collect();
    format!(
        r#"{{"streams":{},"by_coder":{},"by_sensitivity":{},"review_panel":{},"ratified_lessons":[{}],"ratified_lesson_count":{}}}"#,
        streams.len(),
        obj(&by_coder),
        obj(&by_sensitivity),
        any_sensitive,
        ratified_lessons.join(","),
        ratified_lessons.len(),
    )
}

fn execution_geometry_json(
    streams: &[FleetStream],
    requested_width: u32,
    integration_owned_paths: &[String],
    full_suite_checks: &[String],
) -> String {
    use std::collections::BTreeMap;
    let mut by_coder: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_sensitivity: BTreeMap<String, usize> = BTreeMap::new();
    let stream_check_count = streams
        .iter()
        .flat_map(|stream| stream.checks.iter())
        .filter(|check| !check.trim().is_empty())
        .count();
    let dependency_edge_count = streams
        .iter()
        .flat_map(|stream| stream.depends_on.iter())
        .filter(|dependency| !dependency.trim().is_empty())
        .count();
    let max_turn_budget: u32 = streams.iter().map(|stream| stream.max_turns).sum();
    for stream in streams {
        let coder = if stream.builder_slot.trim().is_empty() {
            "default".to_string()
        } else {
            stream.builder_slot.trim().to_string()
        };
        *by_coder.entry(coder).or_default() += 1;
        let sensitivity = if stream.data_sensitivity.trim().is_empty() {
            "internal".to_string()
        } else {
            stream.data_sensitivity.trim().to_ascii_lowercase()
        };
        *by_sensitivity.entry(sensitivity).or_default() += 1;
    }
    let max_concurrent = crate::fleet_executor::pool_status().capacity;
    let disjoint_write_scopes_validated =
        mdx_core::validate_fleet_streams(streams, integration_owned_paths).is_ok();
    format!(
        r#"{{"strategy":"parallel_streams_plus_integration","requested_width":{},"stream_count":{},"max_concurrent_workers":{},"disjoint_write_scopes_validated":{},"integration_owned_path_count":{},"stream_check_count":{},"full_suite_check_count":{},"dependency_edge_count":{},"max_turn_budget":{},"by_coder":{},"by_sensitivity":{},"production_write_allowed":false}}"#,
        requested_width,
        streams.len(),
        max_concurrent,
        disjoint_write_scopes_validated,
        integration_owned_paths.len(),
        stream_check_count,
        full_suite_checks
            .iter()
            .filter(|check| !check.trim().is_empty())
            .count(),
        dependency_edge_count,
        max_turn_budget,
        count_object(&by_coder),
        count_object(&by_sensitivity),
    )
}

fn count_object(map: &std::collections::BTreeMap<String, usize>) -> String {
    let pairs: Vec<String> = map
        .iter()
        .map(|(key, value)| format!("{}:{value}", json_string_literal(key)))
        .collect();
    format!("{{{}}}", pairs.join(","))
}

fn lines_field(plan_fields: &std::collections::BTreeMap<String, String>, key: &str) -> Vec<String> {
    plan_fields
        .get(key)
        .map(|value| {
            value
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn handle_ratify(
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
    let parsed: serde_json::Value = serde_json::from_str(body).unwrap_or(serde_json::json!({}));
    let fleet_id = parsed["fleet_id"].as_str().unwrap_or("").trim();
    let reason = parsed["reason"].as_str().unwrap_or("").trim();
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.ratify_fleet_plan_with_identity(
        &resolved.tenant_id,
        &resolved.actor_id,
        fleet_id,
        reason,
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(ratify_refusal(&error.message())),
    };
    let work_items: Vec<String> = report
        .work_item_ids
        .iter()
        .map(|id| json_string_literal(id))
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-fleet-ratify-local-post","status":"RATIFIED","auth_session_status":{},"fleet_id":{},"ratification_receipt_id":{},"policy_decision_id":{},"work_item_ids":[{}],"next":"start the fleet run - build spend begins only now","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.fleet_id),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            work_items.join(","),
        ),
    ))
}

fn handle_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    // Latest draft per fleet, joined with its ratification state.
    use std::collections::BTreeMap;
    let mut latest: BTreeMap<String, (String, BTreeMap<String, String>)> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for receipt in kernel.ledger().query().by_kind("fleet.plan.drafted").iter() {
        let fleet_id = receipt.payload.get("fleet_id").cloned().unwrap_or_default();
        if fleet_id.is_empty() {
            continue;
        }
        if !latest.contains_key(&fleet_id) {
            order.push(fleet_id.clone());
        }
        latest.insert(
            fleet_id,
            (receipt.receipt_id.clone(), receipt.payload.clone()),
        );
    }
    let mut fleets: Vec<String> = fleet_planning_progress_items()
        .into_iter()
        .filter(|progress| !latest.contains_key(&progress.fleet_id))
        .map(|progress| pending_fleet_plan_json(&progress))
        .collect();
    fleets.extend(order
        .iter()
        .map(|fleet_id| {
            let (receipt_id, plan) = &latest[fleet_id];
            let streams = mdx_core::fleet_plan_streams(plan);
            let requested_width = plan
                .get("requested_width")
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(streams.len() as u32);
            let value = |key: &str| {
                json_string_literal(plan.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"fleet_id":{},"status":{},"spec":{},"goal":{},"checks":{},"justification":{},"planner_model":{},"bet_id":{},"repo_id":{},"repo_primary_language":{},"language_pack_id":{},"repo_profile_source":{},"repo_profile_suggested_checks":{},"repo_profile_artifact_patterns":{},"repo_profile_semantic_intelligence":{},"repo_profile_semantic_tool_readiness":{},"repo_profile_toolchain_readiness":{},"repo_profile_proof_plan_status":{},"repo_profile_proof_plan_summary":{},"stream_count":{},"streams":{},"execution_geometry":{},"wide_plan_review":{},"plan_receipt_id":{}}}"#,
                json_string_literal(fleet_id),
                json_string_literal(if kernel.fleet_plan_ratified(fleet_id) {
                    "ratified"
                } else {
                    "drafted"
                }),
                value("spec"),
                value("goal"),
                serde_json::to_string(&lines_field(plan, "checks"))
                    .unwrap_or_else(|_| "[]".to_string()),
                value("justification"),
                value("planner_model"),
                value("bet_id"),
                value("repo_id"),
                value("repo_primary_language"),
                value("language_pack_id"),
                value("repo_profile_source"),
                value("repo_profile_suggested_checks"),
                value("repo_profile_artifact_patterns"),
                value("repo_profile_semantic_intelligence"),
                value("repo_profile_semantic_tool_readiness"),
                value("repo_profile_toolchain_readiness"),
                value("repo_profile_proof_plan_status"),
                value("repo_profile_proof_plan_summary"),
                streams.len(),
                streams_json(&streams),
                execution_geometry_json(
                    &streams,
                    requested_width,
                    &lines_field(plan, "integration_owned_paths"),
                    &lines_field(plan, "full_suite_checks"),
                ),
                wide_plan_review_json_from_fields(plan),
                json_string_literal(receipt_id),
            )
        })
    );
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-fleet-plan-local-projection","receipt_kind":"fleet.plan.drafted","fleet_count":{},"fleets":[{}],"production_write_allowed":false}}"#,
            fleets.len(),
            fleets.join(","),
        ),
    ))
}

fn wide_plan_review_json_from_fields(plan: &std::collections::BTreeMap<String, String>) -> String {
    let required = plan.get("wide_plan_review_required").map(String::as_str) == Some("true");
    let value = |key: &str| json_string_literal(plan.get(key).map(String::as_str).unwrap_or(""));
    format!(
        r#"{{"required":{},"threshold":{},"status":{},"reviewer_model":{},"verdict":{},"confidence":{},"concerns":{},"grants_execution_authority":false}}"#,
        required,
        WIDE_FLEET_REVIEW_REQUIRED_WIDTH,
        value("wide_plan_review_status"),
        value("wide_plan_review_reviewer_model"),
        value("wide_plan_review_verdict"),
        value("wide_plan_review_confidence"),
        value("wide_plan_review_concerns"),
    )
}

/// The prior plan, rendered for a re-plan prompt.
fn prior_plan_for_tenant(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    fleet_id: &str,
) -> Option<std::collections::BTreeMap<String, String>> {
    let Ok(kernel) = kernel.read() else {
        return None;
    };
    let tenant_id = tenant_id.trim();
    let fleet_id = fleet_id.trim();
    if tenant_id.is_empty() || fleet_id.is_empty() {
        return None;
    }
    let mut latest = None;
    for receipt in kernel.ledger().query().by_kind("fleet.plan.drafted") {
        if receipt.tenant_id.as_str() == tenant_id
            && receipt.payload.get("fleet_id").map(String::as_str) == Some(fleet_id)
        {
            latest = Some(receipt.payload.clone());
        }
    }
    latest
}

fn prior_plan_context(kernel: &Arc<RwLock<MdxKernel>>, tenant_id: &str, fleet_id: &str) -> String {
    let Some(plan) = prior_plan_for_tenant(kernel, tenant_id, fleet_id) else {
        return String::new();
    };
    let streams = mdx_core::fleet_plan_streams(&plan);
    format!(
        "\nThe previous plan for this fleet:\n{}\n",
        streams_json(&streams)
    )
}

fn prior_plan_goal(kernel: &Arc<RwLock<MdxKernel>>, tenant_id: &str, fleet_id: &str) -> String {
    prior_plan_for_tenant(kernel, tenant_id, fleet_id)
        .and_then(|plan| plan.get("goal").cloned())
        .unwrap_or_default()
}

fn prior_plan_lines(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    fleet_id: &str,
    field: &str,
) -> Vec<String> {
    prior_plan_for_tenant(kernel, tenant_id, fleet_id)
        .map(|plan| lines_field(&plan, field))
        .unwrap_or_default()
}

fn prior_plan_requested_width(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    fleet_id: &str,
) -> Option<u32> {
    prior_plan_for_tenant(kernel, tenant_id, fleet_id)
        .and_then(|plan| plan.get("requested_width").cloned())
        .and_then(|value| value.parse::<u32>().ok())
}

// A cheap pre-check that mirrors the kernel's core stream invariants (every
// stream declares a non-empty write_scope; scopes are disjoint) so a bad
// planner draft can be caught and re-planned before the authoritative kernel
// validation. The kernel remains the source of truth; this only drives the
// self-correct retry with a specific, actionable reason.
fn prevalidate_planner_streams(streams: &[FleetStream]) -> Result<(), String> {
    if streams.is_empty() {
        return Err("the plan has no streams".to_string());
    }
    let mut claimed: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for stream in streams {
        let id = if stream.stream_id.trim().is_empty() {
            "<unnamed>"
        } else {
            stream.stream_id.trim()
        };
        let scope: Vec<String> = stream
            .write_scope
            .iter()
            .map(|path| {
                path.trim()
                    .trim_start_matches("./")
                    .trim_end_matches('/')
                    .to_string()
            })
            .filter(|path| !path.is_empty())
            .collect();
        if scope.is_empty() {
            return Err(format!("stream {id} declares no write_scope"));
        }
        for path in scope {
            if let Some(other) = claimed.get(&path) {
                return Err(format!(
                    "stream {id} and stream {other} both claim {path}; write scopes must be disjoint"
                ));
            }
            claimed.insert(path, id.to_string());
        }
    }
    Ok(())
}

// Model plans also need a cheap repository-shape check. Disjoint fictional
// paths are still fictional: without this gate a planner can return a
// structurally valid split copied from another Rust repository and the human
// sees eight confident lanes that can never start. Existing-file objectives
// must resolve to files in the connected checkout. New paths remain valid
// when their parent directory exists, which keeps greenfield work possible.
fn prevalidate_planner_streams_for_repo(
    streams: &[FleetStream],
    context: &PlanRepoContext,
) -> Result<(), String> {
    prevalidate_planner_streams(streams)?;
    for stream in streams {
        let requires_existing_file = stream
            .objective
            .to_ascii_lowercase()
            .contains("existing file");
        for raw_scope in &stream.write_scope {
            let normalized = raw_scope
                .trim()
                .trim_start_matches("./")
                .trim_end_matches('/');
            let scope = Path::new(normalized);
            if normalized.is_empty()
                || scope.is_absolute()
                || normalized
                    .chars()
                    .any(|ch| matches!(ch, '*' | '?' | '[' | ']'))
                || scope.components().any(|component| {
                    matches!(
                        component,
                        Component::ParentDir | Component::RootDir | Component::Prefix(_)
                    )
                })
            {
                return Err(format!(
                    "stream {} declares invalid repository scope {raw_scope}; use a concrete relative path inside the selected repo",
                    stream.stream_id
                ));
            }
            let candidate = context.root.join(scope);
            if requires_existing_file && !candidate.is_file() {
                return Err(format!(
                    "stream {} says it will edit the existing file {normalized}, but that file does not exist in repo {}; choose a path from the repository inventory",
                    stream.stream_id, context.repo_id
                ));
            }
            if !candidate.exists() && !candidate.parent().is_some_and(|parent| parent.is_dir()) {
                return Err(format!(
                    "stream {} declares {normalized}, but neither that path nor its parent exists in repo {}; choose a real repository seam or a new path under an existing directory",
                    stream.stream_id, context.repo_id
                ));
            }
        }
    }
    Ok(())
}

fn streams_from_value(value: &serde_json::Value) -> Vec<FleetStream> {
    value
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| FleetStream {
                    stream_id: row["stream_id"].as_str().unwrap_or("").trim().to_string(),
                    objective: row["objective"].as_str().unwrap_or("").trim().to_string(),
                    write_scope: string_list(&row["write_scope"]),
                    interface_contract: row["interface_contract"]
                        .as_str()
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                    depends_on: string_list(&row["depends_on"]),
                    checks: string_list(&row["checks"]),
                    max_turns: row["max_turns"].as_u64().unwrap_or(0).min(120) as u32,
                    builder_slot: row["builder_slot"]
                        .as_str()
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                    data_sensitivity: row["data_sensitivity"]
                        .as_str()
                        .unwrap_or("")
                        .trim()
                        .to_ascii_lowercase(),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn string_list(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn streams_json(streams: &[FleetStream]) -> String {
    let rows: Vec<serde_json::Value> = streams
        .iter()
        .map(|stream| {
            serde_json::json!({
                "stream_id": stream.stream_id,
                "objective": stream.objective,
                "write_scope": stream.write_scope,
                "interface_contract": stream.interface_contract,
                "depends_on": stream.depends_on,
                "checks": stream.checks,
                "max_turns": stream.max_turns,
                "builder_slot": stream.builder_slot,
                "data_sensitivity": stream.data_sensitivity,
            })
        })
        .collect();
    serde_json::Value::Array(rows).to_string()
}

/// The first JSON object in a model reply - tolerant of markdown fences
/// and prose around the answer.
fn extract_json_object(text: &str) -> Option<serde_json::Value> {
    let start = text.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, character) in text[start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return serde_json::from_str(&text[start..start + offset + 1]).ok();
                }
            }
            _ => {}
        }
    }
    None
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-fleet-plan-local-post","status":"REFUSED","reason":{},"plan_receipt_id":"","production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

fn ratify_refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-fleet-ratify-local-post","status":"REFUSED","reason":{},"ratification_receipt_id":"","production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::ForgeRepoConnect;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn the_casting_brief_summarizes_coder_and_sensitivity_mix() {
        let mk = |slot: &str, sens: &str| FleetStream {
            stream_id: "s".into(),
            objective: "o".into(),
            write_scope: vec!["f.rs".into()],
            interface_contract: String::new(),
            depends_on: Vec::new(),
            checks: Vec::new(),
            max_turns: 30,
            builder_slot: slot.into(),
            data_sensitivity: sens.into(),
        };
        let streams = vec![mk("", "internal"), mk("", "open"), mk("OPUS", "sensitive")];
        let brief = casting_brief(&streams, &[]);
        assert!(brief.contains("\"streams\":3"));
        assert!(brief.contains("\"ratified_lesson_count\":0"));
        assert!(brief.contains("\"default\":2"));
        assert!(brief.contains("\"OPUS\":1"));
        assert!(brief.contains("\"sensitive\":1"));
        // A sensitive stream means the review panel will convene.
        assert!(brief.contains("\"review_panel\":true"));
        // An empty sensitivity defaults to "internal", not lost.
        assert!(brief.contains("\"internal\":1"));
    }

    #[test]
    fn prevalidate_planner_streams_catches_missing_and_overlapping_scope() {
        let mk = |id: &str, scope: Vec<&str>| FleetStream {
            stream_id: id.into(),
            objective: "o".into(),
            write_scope: scope.into_iter().map(String::from).collect(),
            interface_contract: String::new(),
            depends_on: Vec::new(),
            checks: Vec::new(),
            max_turns: 30,
            builder_slot: String::new(),
            data_sensitivity: "internal".into(),
        };
        // An empty plan is not usable.
        assert!(prevalidate_planner_streams(&[]).is_err());
        // A stream with no write_scope is rejected by id with an actionable reason.
        let missing = vec![mk("s1", vec!["a.rs"]), mk("s2", vec![])];
        let err = prevalidate_planner_streams(&missing).unwrap_err();
        assert!(err.contains("s2") && err.contains("write_scope"));
        // Overlapping scopes are rejected as non-disjoint.
        let overlap = vec![mk("s1", vec!["a.rs"]), mk("s2", vec!["./a.rs"])];
        assert!(
            prevalidate_planner_streams(&overlap)
                .unwrap_err()
                .contains("disjoint")
        );
        // A clean, disjoint plan passes.
        let ok = vec![mk("s1", vec!["a.rs"]), mk("s2", vec!["b.rs"])];
        assert!(prevalidate_planner_streams(&ok).is_ok());
    }

    #[test]
    fn planner_repo_preflight_rejects_wrong_repo_paths_but_allows_greenfield_files() {
        let repo = temp_repo("fleet_repo_path_preflight");
        std::fs::create_dir_all(repo.join("crates/mdx-rust-core/src")).expect("source dir");
        std::fs::create_dir_all(repo.join("docs")).expect("docs dir");
        std::fs::create_dir_all(repo.join("target/debug")).expect("target dir");
        std::fs::write(
            repo.join("crates/mdx-rust-core/src/config.rs"),
            "// config\n",
        )
        .expect("source file");
        std::fs::write(repo.join("target/debug/noise.rs"), "// ignored\n").expect("artifact file");
        let context = PlanRepoContext {
            repo_id: "mdx-rust".to_string(),
            root: repo.clone(),
            profile: crate::forge_repo_profile::profile_repo(&repo),
            profile_source: "test-profile".to_string(),
            verified_hosted_execution: false,
        };
        let stream = |id: &str, objective: &str, scope: &str| FleetStream {
            stream_id: id.to_string(),
            objective: objective.to_string(),
            write_scope: vec![scope.to_string()],
            interface_contract: String::new(),
            depends_on: Vec::new(),
            checks: vec!["cargo test".to_string()],
            max_turns: 30,
            builder_slot: String::new(),
            data_sensitivity: "internal".to_string(),
        };

        let existing = vec![stream(
            "s1",
            "In the existing file crates/mdx-rust-core/src/config.rs, add a test",
            "crates/mdx-rust-core/src/config.rs",
        )];
        assert!(prevalidate_planner_streams_for_repo(&existing, &context).is_ok());

        let wrong_repo = vec![stream(
            "s1",
            "In the existing file src/configuration.rs, add a test",
            "src/configuration.rs",
        )];
        let error = prevalidate_planner_streams_for_repo(&wrong_repo, &context)
            .expect_err("wrong repository shape must fail");
        assert!(error.contains("does not exist") && error.contains("repository inventory"));

        let greenfield = vec![stream(
            "s1",
            "Create a cloud parity note",
            "docs/cloud-parity.md",
        )];
        assert!(prevalidate_planner_streams_for_repo(&greenfield, &context).is_ok());

        let traversal = vec![stream("s1", "Change a file", "../outside.rs")];
        assert!(prevalidate_planner_streams_for_repo(&traversal, &context).is_err());

        let inventory = repository_path_inventory(&repo);
        assert!(inventory.contains("crates/mdx-rust-core/src/config.rs"));
        assert!(!inventory.contains("target/debug/noise.rs"));
    }

    #[test]
    fn verified_hosted_fleet_plan_uses_sandbox_readiness_not_render_host_tools() {
        let repo = temp_repo("hosted_fleet_plan_readiness");
        std::fs::create_dir_all(repo.join("crates/example/src")).expect("source dir");
        std::fs::write(repo.join("Cargo.toml"), "[workspace]\n").expect("manifest");
        let mut profile = crate::forge_repo_profile::profile_repo(&repo);
        profile.proof_plan_status = "setup_required";
        profile.toolchain_readiness = vec![
            "tool:cargo=missing".to_string(),
            "check:cargo_test=missing".to_string(),
        ];
        let local = PlanRepoContext {
            repo_id: "example".to_string(),
            root: repo.clone(),
            profile: profile.clone(),
            profile_source: "test-profile".to_string(),
            verified_hosted_execution: false,
        };
        let hosted = PlanRepoContext {
            verified_hosted_execution: true,
            ..local.clone()
        };

        assert!(
            plan_toolchain_readiness(&local)
                .iter()
                .any(|item| item.contains("missing"))
        );
        assert_eq!(plan_proof_status(&local), "setup_required");
        assert_eq!(
            plan_toolchain_readiness(&hosted),
            vec![
                "execution:verified_hosted_sandbox".to_string(),
                "proof:selected_checks_run_in_hosted_sandbox".to_string(),
            ]
        );
        assert_eq!(plan_proof_status(&hosted), "ready");
        let prompt = repo_planner_context(&hosted);
        assert!(prompt.contains("execution:verified_hosted_sandbox"));
        assert!(!prompt.contains("tool:cargo=missing"));
    }

    #[test]
    fn json_extraction_survives_fences_prose_and_nested_braces() {
        let fenced =
            "Here is the plan:\n```json\n{\"streams\":[{\"stream_id\":\"s1\"}]}\n```\nDone.";
        let value = extract_json_object(fenced).expect("fenced");
        assert_eq!(value["streams"][0]["stream_id"], "s1");

        let nested = r#"{"a":{"b":"with } brace in string"},"c":1}"#;
        let value = extract_json_object(nested).expect("nested");
        assert_eq!(value["c"], 1);

        assert!(extract_json_object("no json here").is_none());
    }

    #[test]
    fn fallback_plan_after_planner_failure_uses_disjoint_repo_scopes() {
        let repo = temp_repo("fleet_fallback_repo");
        for dir in [
            "apps/mdx-operator-macos",
            "apps/mdx-host/src/routes/forge",
            "apps/mdx-host/src/routes/admin/forge",
            "apps/mdx-host/src/lib",
            "crates/mdx-server/src",
            "crates/mdx-core/src",
            "scripts",
            "docs",
            "generated",
        ] {
            std::fs::create_dir_all(repo.join(dir)).expect("dir");
        }
        std::fs::write(repo.join("crates/mdx-server/src/fleet_plan_route.rs"), "").expect("file");
        std::fs::write(repo.join("crates/mdx-server/src/forge_run_route.rs"), "").expect("file");
        std::fs::write(repo.join("crates/mdx-core/src/forge_run.rs"), "").expect("file");
        let context = PlanRepoContext {
            repo_id: "mdx".to_string(),
            root: repo.clone(),
            profile: crate::forge_repo_profile::profile_repo(&repo),
            profile_source: "test-profile".to_string(),
            verified_hosted_execution: false,
        };

        let (value, planner_model) = fallback_plan_after_planner_failure(
            "Improve native Forge run recovery",
            &context,
            Some(8),
            "the planner did not answer",
        )
        .expect("fallback");
        let streams = streams_from_value(&value["streams"]);
        let integration_owned = string_list(&value["integration_owned_paths"]);

        assert_eq!(planner_model, "mdx-local-fleet-fallback");
        assert_eq!(streams.len(), 8);
        assert_eq!(integration_owned.len(), DEFAULT_INTEGRATION_OWNED.len());
        assert!(prevalidate_planner_streams(&streams).is_ok());
        assert!(mdx_core::validate_fleet_streams(&streams, &integration_owned).is_ok());
        assert!(
            streams
                .iter()
                .all(|stream| !stream.write_scope.iter().any(|scope| scope == "generated/"))
        );
        assert!(
            value["justification"]
                .as_str()
                .unwrap_or("")
                .contains("Local fallback split")
        );
    }

    #[test]
    fn exact_operator_split_preserves_a_numbered_path_range_without_model_planning() {
        let repo = temp_repo("fleet_exact_operator_split_repo");
        std::fs::create_dir_all(repo.join("docs")).expect("docs");
        let context = PlanRepoContext {
            repo_id: "mdx-rust".to_string(),
            root: repo.clone(),
            profile: crate::forge_repo_profile::profile_repo(&repo),
            profile_source: "test-profile".to_string(),
            verified_hosted_execution: true,
        };
        let spec = "Use exactly these new disjoint files with no substitutions: docs/cloud-scale24-v3-lane-01.md through docs/cloud-scale24-v3-lane-24.md. Assign exactly one file to each lane.";

        let (value, planner_model) =
            exact_operator_split_plan(spec, &context, Some(24), &["git diff --check".to_string()])
                .expect("exact split");
        let streams = streams_from_value(&value["streams"]);

        assert_eq!(planner_model, "mdx-exact-operator-split");
        assert_eq!(streams.len(), 24);
        assert_eq!(streams[0].stream_id, "lane_01");
        assert_eq!(
            streams[0].write_scope,
            vec!["docs/cloud-scale24-v3-lane-01.md"]
        );
        assert_eq!(
            streams[23].write_scope,
            vec!["docs/cloud-scale24-v3-lane-24.md"]
        );
        assert!(prevalidate_planner_streams_for_repo(&streams, &context).is_ok());
        assert_eq!(
            string_list(&value["full_suite_checks"]),
            vec!["git diff --check"]
        );
    }

    #[test]
    fn wide_plan_review_deduplicates_shared_checks_across_48_streams() {
        let shared_check = "git diff --check && verify-the-full-acceptance-contract".to_string();
        let streams = (1..=48)
            .map(|index| FleetStream {
                stream_id: format!("lane_{index:02}"),
                objective: format!("Create docs/lane-{index:02}.md"),
                write_scope: vec![format!("docs/lane-{index:02}.md")],
                interface_contract: "One independent file".to_string(),
                depends_on: Vec::new(),
                checks: vec![shared_check.clone()],
                max_turns: 12,
                builder_slot: String::new(),
                data_sensitivity: "internal".to_string(),
            })
            .collect::<Vec<_>>();

        let (summaries, check_sets) = wide_plan_review_streams(&streams);
        assert_eq!(summaries.as_array().map(Vec::len), Some(48));
        assert_eq!(check_sets, serde_json::json!([[shared_check]]));
        assert!(
            summaries
                .to_string()
                .find("verify-the-full-acceptance-contract")
                .is_none()
        );
    }

    #[test]
    fn exact_operator_split_refuses_a_range_that_does_not_match_requested_width() {
        let repo = temp_repo("fleet_exact_operator_split_width_repo");
        std::fs::create_dir_all(repo.join("docs")).expect("docs");
        let context = PlanRepoContext {
            repo_id: "mdx-rust".to_string(),
            root: repo.clone(),
            profile: crate::forge_repo_profile::profile_repo(&repo),
            profile_source: "test-profile".to_string(),
            verified_hosted_execution: true,
        };

        assert!(
            exact_operator_split_plan(
                "docs/lane-01.md through docs/lane-24.md",
                &context,
                Some(48),
                &["git diff --check".to_string()],
            )
            .is_none()
        );
    }

    #[test]
    fn fleet_reasoning_watchdog_returns_before_a_stuck_worker() {
        let started = Instant::now();
        let result = run_fleet_reasoning_with_timeout(Duration::from_millis(5), || {
            std::thread::sleep(Duration::from_millis(100));
            "late"
        });

        assert_eq!(result.unwrap_err(), "model call timed out after 0 seconds");
        assert!(started.elapsed() < Duration::from_millis(80));
    }

    #[test]
    fn external_repo_fallback_does_not_inherit_mdx_integration_paths() {
        let repo = temp_repo("fleet_external_fallback_repo");
        std::fs::create_dir_all(repo.join("docs")).expect("docs");
        std::fs::write(repo.join("README.md"), "# External\n").expect("readme");
        std::fs::write(repo.join("docs/architecture.md"), "# Architecture\n")
            .expect("architecture");
        let context = PlanRepoContext {
            repo_id: "mdx-rust".to_string(),
            root: repo.clone(),
            profile: crate::forge_repo_profile::profile_repo(&repo),
            profile_source: "test-profile".to_string(),
            verified_hosted_execution: false,
        };

        assert!(default_integration_owned_paths(&context).is_empty());
        assert!(
            default_integration_owned_prompt(&context)
                .contains("leave integration_owned_paths empty")
        );

        let (value, _) = fallback_plan_after_planner_failure(
            "Improve README and architecture docs",
            &context,
            Some(2),
            "the planner did not answer",
        )
        .expect("external fallback");
        let streams = streams_from_value(&value["streams"]);
        let integration_owned = string_list(&value["integration_owned_paths"]);

        assert_eq!(streams.len(), 2);
        assert!(integration_owned.is_empty());
        assert!(mdx_core::validate_fleet_streams(&streams, &integration_owned).is_ok());
    }

    #[test]
    fn wide_plan_review_prompt_understands_forge_integration_phase() {
        assert!(WIDE_PLAN_REVIEW_SYSTEM_PROMPT.contains("built-in integration phase"));
        assert!(WIDE_PLAN_REVIEW_SYSTEM_PROMPT.contains("Do not require both"));
        assert!(WIDE_PLAN_REVIEW_SYSTEM_PROMPT.contains("full_suite_checks run exactly once"));
        assert!(WIDE_PLAN_REVIEW_SYSTEM_PROMPT.contains("must not block the plan"));
        assert!(
            WIDE_PLAN_REVIEW_SYSTEM_PROMPT
                .contains("must not require another stream's unmerged implementation")
        );
        assert!(
            PLANNER_SYSTEM_PROMPT
                .contains("stream's own check must pass from the base repo plus that stream")
        );
    }

    #[test]
    fn planner_justification_is_capped_before_kernel_storage() {
        let long = "a".repeat(700);
        let capped = capped_plan_field(&long);
        assert_eq!(capped.chars().count(), 500);
        assert!(capped.ends_with("[truncated]"));
    }

    #[test]
    fn manual_fleet_plan_records_repo_intelligence_for_wide_execution() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        clear_provider_env_for_missing_reviewer_test();
        let repo = temp_repo("fleet_swift_repo");
        std::fs::write(repo.join("Package.swift"), "// swift package\n").expect("package");
        std::fs::create_dir_all(repo.join("Sources/Canary")).expect("sources");
        std::fs::write(
            repo.join("Sources/Canary/Slugger.swift"),
            "struct Slugger {}\n",
        )
        .expect("source");

        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut guard = kernel.write().expect("kernel");
            guard
                .connect_forge_repo(ForgeRepoConnect {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    repo_id: "swift-canary",
                    label: "Swift Canary",
                    root: &repo.to_string_lossy(),
                    kind: "local",
                    origin: "",
                })
                .expect("connect");
        }

        let body = r#"{
            "spec":"modernize slug generation across the package",
            "goal":"Modernize Swift slug generation",
            "checks":["swift test"],
            "tenant_id":"tenant_fleet_missing_reviewer",
            "repo_id":"swift-canary",
            "requested_width":8,
            "streams":[{
                "stream_id":"s1_slugger",
                "objective":"Implement slug generation in the Swift package",
                "write_scope":["Sources/Canary/Slugger.swift"],
                "interface_contract":"Keep Slugger API stable",
                "checks":["swift test"],
                "max_turns":20,
                "builder_slot":"",
                "data_sensitivity":"internal"
            }],
            "full_suite_checks":["swift test"],
            "actor_id":"human:eng"
        }"#;
        let response = handle_draft("POST", body, &kernel).expect("draft response");
        for expected in [
            r#""status":"DRAFTED""#,
            r#""repo_id":"swift-canary""#,
            r#""language_pack_id":"swift-spm""#,
            r#""repo_primary_language":"swift""#,
            r#""execution_geometry":{"strategy":"parallel_streams_plus_integration""#,
            r#""requested_width":8"#,
            r#""stream_count":1"#,
            r#""disjoint_write_scopes_validated":true"#,
            r#""integration_owned_path_count":0"#,
            r#""wide_plan_review":{"required":true"#,
            r#""status":"missing_reviewer""#,
            r#""verdict":"not_recorded""#,
        ] {
            assert!(
                response.body.contains(expected),
                "{expected}: {}",
                response.body
            );
        }

        let projection = handle_projection("GET", &kernel).expect("projection");
        for expected in [
            r#""goal":"Modernize Swift slug generation""#,
            r#""checks":["swift test"]"#,
            r#""repo_id":"swift-canary""#,
            r#""language_pack_id":"swift-spm""#,
            r#""repo_profile_suggested_checks":"swift test""#,
            r#""repo_profile_artifact_patterns":".build/**,DerivedData/**""#,
            r#""repo_profile_source":"live-profile""#,
            r#""execution_geometry":{"strategy":"parallel_streams_plus_integration""#,
            r#""requested_width":8"#,
            r#""stream_check_count":1"#,
            r#""integration_owned_path_count":0"#,
            r#""wide_plan_review":{"required":true"#,
            r#""status":"missing_reviewer""#,
            r#""verdict":"not_recorded""#,
        ] {
            assert!(
                projection.body.contains(expected),
                "{expected}: {}",
                projection.body
            );
        }
    }

    #[test]
    fn pending_fleet_plan_projection_is_visible_without_execution_authority() {
        let mut progress = fleet_planning_progress_from_request(
            "fleet_planning_visible",
            "planner context",
            "Split a wide native build",
            &["make native-macos-operator-check".to_string()],
            "mdx-native",
            Some(8),
        );
        progress.stage = "planner_drafting".to_string();
        progress.detail = "The planner is drafting disjoint lanes for this fleet.".to_string();
        progress.attempt = 1;
        let _progress_guard = begin_fleet_planning_progress(progress);
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let projection = handle_projection("GET", &kernel).expect("projection");

        for expected in [
            r#""fleet_id":"fleet_planning_visible""#,
            r#""status":"planning""#,
            r#""goal":"Split a wide native build""#,
            r#""checks":["make native-macos-operator-check"]"#,
            r#""requested_width":8"#,
            r#""stream_count":0"#,
            r#""strategy":"planning""#,
            r#""planning_progress":{"stage":"planner_drafting""#,
            r#""detail":"The planner is drafting disjoint lanes for this fleet.""#,
            r#""attempt":1"#,
            r#""wide_plan_review":{"required":true"#,
            r#""grants_execution_authority":false"#,
            r#""production_write_allowed":false"#,
        ] {
            assert!(
                projection.body.contains(expected),
                "{expected}: {}",
                projection.body
            );
        }
    }

    #[test]
    fn stale_pending_fleet_plan_projection_shows_recovery_guidance() {
        let mut progress = fleet_planning_progress_from_request(
            "fleet_planning_slow",
            "Split a slow wide build",
            "Split a slow wide build",
            &[],
            "mdx-native",
            Some(8),
        );
        progress.stage = "planner_drafting".to_string();
        progress.detail = "The planner is drafting disjoint lanes for this fleet.".to_string();
        progress.attempt = 1;
        progress.updated_at =
            SystemTime::now() - FLEET_PLANNING_DELAYED_AFTER - Duration::from_secs(1);
        let _progress_guard = begin_fleet_planning_progress(progress);
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let projection = handle_projection("GET", &kernel).expect("projection");

        for expected in [
            r#""fleet_id":"fleet_planning_slow""#,
            r#""status":"planning_delayed""#,
            r#""planning_progress":{"stage":"planner_slow""#,
            "The planner is taking longer than usual",
            "retry with a narrower fleet",
            r#""production_write_allowed":false"#,
        ] {
            assert!(
                projection.body.contains(expected),
                "{expected}: {}",
                projection.body
            );
        }
    }

    #[test]
    fn return_pending_accepts_fleet_plan_before_background_planning_finishes() {
        let progress = fleet_planning_progress_from_request(
            "",
            "Prepare a native fleet plan",
            "Prepare a native fleet plan",
            &[],
            "mdx-native",
            Some(8),
        );
        let response = fleet_planning_accepted_json(&progress, "ACCEPTED_LOCAL_STUB");
        for expected in [
            r#""status":"PLANNING""#,
            r#""fleet_id":"fleet_planning_"#,
            r#""stream_count":0"#,
            r#""requested_width":8"#,
            r#""plan_receipt_id":"""#,
            r#""production_write_allowed":false"#,
        ] {
            assert!(response.contains(expected), "{expected}: {response}");
        }
    }

    #[test]
    fn background_planning_preserves_hosted_cloud_coordinates() {
        let parsed = serde_json::json!({
            "spec": "Prepare a hosted fleet plan",
            "repo_id": "mdx-native",
            "cloud_environment_id": "cloud_environment_aws_agentcore",
            "execution_backend": "hosted_sandbox",
            "return_pending": true,
        });

        let background_body = fleet_plan_body_without_pending(&parsed);
        let background: serde_json::Value =
            serde_json::from_str(&background_body).expect("background fleet plan body");

        assert_eq!(
            background["cloud_environment_id"],
            "cloud_environment_aws_agentcore"
        );
        assert_eq!(background["execution_backend"], "hosted_sandbox");
        assert!(background.get("return_pending").is_none());
    }

    #[test]
    fn background_planning_inherits_the_verified_request_identity() {
        let identity = mdx_core::AdmittedIdentity {
            deployment_mode: "production",
            tenant_id: "tenant_personal_beta".to_string(),
            actor_id: "human:owner".to_string(),
            actor_role: "owner".to_string(),
            actor_kind: "human".to_string(),
            subject_actor_id: "human:owner".to_string(),
            authority_scope: vec!["forge:fleet-plan".to_string()],
            delegation_id: None,
            identity_source: "trusted_session",
            production_write_allowed: false,
        };
        let expected = identity.clone();

        let observed = std::thread::spawn(move || {
            run_with_inherited_fleet_identity(Some(identity), || {
                crate::request_security::current_verified_identity()
            })
        })
        .join()
        .expect("background planner thread");

        assert_eq!(observed, Some(expected));
    }

    #[test]
    fn an_early_background_refusal_cannot_leave_an_eternal_pending_plan() {
        let fleet_id = "fleet_planning_background_refusal";
        insert_fleet_planning_progress(fleet_planning_progress_from_request(
            fleet_id,
            "Prepare a hosted fleet plan",
            "Prepare a hosted fleet plan",
            &[],
            "mdx-rust",
            Some(24),
        ));

        finish_background_fleet_plan(
            fleet_id,
            Ok(refusal("verified hosted environment was not available")),
        );

        let progress = fleet_planning_progress_store()
            .lock()
            .expect("planning progress")
            .get(fleet_id)
            .cloned()
            .expect("retained attention state");
        assert_eq!(progress.status, "planning_needs_attention");
        assert_eq!(progress.stage, "needs_attention");
        assert!(progress.detail.contains("verified hosted environment"));
        clear_fleet_planning_progress(fleet_id);
    }

    fn clear_provider_env_for_missing_reviewer_test() {
        crate::secret_store::global().clear_tenant("local_tenant");
        let keys = [
            "MDX_FLEET_REVIEWER_BASE_URL",
            "MDX_FLEET_REVIEWER_API_KEY",
            "MDX_FLEET_REVIEWER_MODEL",
            "MDX_FLEET_REVIEWER_PROVIDER",
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "XAI_API_KEY",
            "GEMINI_API_KEY",
            "MDX_OPENAI_MODEL",
            "MDX_ANTHROPIC_MODEL",
            "MDX_XAI_MODEL",
            "MDX_XAI_BUILD_MODEL",
            "MDX_GEMINI_MODEL",
            "MDX_FLEET_BUILDER_GPT_BASE_URL",
            "MDX_FLEET_BUILDER_GPT_API_KEY",
            "MDX_FLEET_BUILDER_GPT_MODEL",
            "MDX_FLEET_BUILDER_CODEX_BASE_URL",
            "MDX_FLEET_BUILDER_CODEX_API_KEY",
            "MDX_FLEET_BUILDER_CODEX_MODEL",
            "MDX_FLEET_BUILDER_CODEXMINI_BASE_URL",
            "MDX_FLEET_BUILDER_CODEXMINI_API_KEY",
            "MDX_FLEET_BUILDER_CODEXMINI_MODEL",
            "MDX_FLEET_BUILDER_GEMINI_BASE_URL",
            "MDX_FLEET_BUILDER_GEMINI_API_KEY",
            "MDX_FLEET_BUILDER_GEMINI_MODEL",
            "MDX_FLEET_BUILDER_OPUS_BASE_URL",
            "MDX_FLEET_BUILDER_OPUS_API_KEY",
            "MDX_FLEET_BUILDER_OPUS_MODEL",
            "MDX_FLEET_BUILDER_SONNET_BASE_URL",
            "MDX_FLEET_BUILDER_SONNET_API_KEY",
            "MDX_FLEET_BUILDER_SONNET_MODEL",
            "MDX_FLEET_BUILDER_GROK_BASE_URL",
            "MDX_FLEET_BUILDER_GROK_API_KEY",
            "MDX_FLEET_BUILDER_GROK_MODEL",
            "MDX_FLEET_BUILDER_XAI_BASE_URL",
            "MDX_FLEET_BUILDER_XAI_API_KEY",
            "MDX_FLEET_BUILDER_XAI_MODEL",
        ];
        // SAFETY: this test holds ENV_TEST_LOCK while mutating process env.
        unsafe {
            for key in keys {
                std::env::remove_var(key);
            }
        }
    }

    #[test]
    fn requested_width_is_a_hard_ceiling_for_manual_fleet_plans() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let body = r#"{
            "spec":"split the repo cleanup",
            "goal":"Clean up the repo in parallel",
            "requested_width":1,
            "streams":[{
                "stream_id":"s1_docs",
                "objective":"Clean up docs",
                "write_scope":["docs/"],
                "interface_contract":"Docs only",
                "checks":["make docs-check"],
                "max_turns":10,
                "builder_slot":"",
                "data_sensitivity":"internal"
            },{
                "stream_id":"s2_tests",
                "objective":"Clean up tests",
                "write_scope":["tests/"],
                "interface_contract":"Tests only",
                "checks":["cargo test"],
                "max_turns":10,
                "builder_slot":"",
                "data_sensitivity":"internal"
            }],
            "integration_owned_paths":[],
            "full_suite_checks":[],
            "actor_id":"human:eng"
        }"#;
        let response = handle_draft("POST", body, &kernel).expect("draft response");
        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("exceeds requested_width=1"));
    }

    #[test]
    fn fleet_plan_requires_spec_and_goal_without_cross_filling() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let spec_only = handle_draft(
            "POST",
            r#"{"spec":"Operator requested a 4-worker execution plan"}"#,
            &kernel,
        )
        .expect("spec-only response");
        assert!(spec_only.body.contains(r#""status":"REFUSED""#));
        assert!(spec_only.body.contains("starts from a goal"));

        let goal_only = handle_draft(
            "POST",
            r#"{"goal":"Repair native fleet recovery"}"#,
            &kernel,
        )
        .expect("goal-only response");
        assert!(goal_only.body.contains(r#""status":"REFUSED""#));
        assert!(goal_only.body.contains("starts from a spec"));

        let initial = handle_draft(
            "POST",
            r#"{
                "fleet_id":"fleet_goal_replan",
                "spec":"Split one docs check",
                "goal":"Keep the original operator goal",
                "requested_width":4,
                "checks":["git diff --check"],
                "streams":[{
                    "stream_id":"docs",
                    "objective":"Check docs",
                    "write_scope":["docs/"],
                    "interface_contract":"Docs only",
                    "checks":["git diff --check"],
                    "max_turns":10,
                    "builder_slot":"",
                    "data_sensitivity":"internal"
                }]
            }"#,
            &kernel,
        )
        .expect("initial response");
        assert!(initial.body.contains(r#""status":"DRAFTED""#));

        let replan = handle_draft(
            "POST",
            r#"{
                "fleet_id":"fleet_goal_replan",
                "spec":"Use a narrower docs check",
                "feedback":"Keep the lane narrow",
                "streams":[{
                    "stream_id":"docs",
                    "objective":"Check docs narrowly",
                    "write_scope":["docs/"],
                    "interface_contract":"Docs only",
                    "checks":["git diff --check"],
                    "max_turns":10,
                    "builder_slot":"",
                    "data_sensitivity":"internal"
                }]
            }"#,
            &kernel,
        )
        .expect("replan response");
        assert!(replan.body.contains(r#""status":"DRAFTED""#));
        assert!(
            replan
                .body
                .contains(r#""goal":"Keep the original operator goal""#)
        );
        assert!(replan.body.contains(r#""checks":["git diff --check"]"#));
        assert!(replan.body.contains(r#""requested_width":4"#));

        let guard = kernel.read().expect("kernel");
        assert_eq!(
            guard.ledger().query().by_kind("fleet.plan.drafted").len(),
            2
        );
    }

    #[test]
    fn prior_plan_inheritance_is_scoped_to_the_resolved_tenant() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let initial = handle_draft(
            "POST",
            r#"{
                "tenant_id":"tenant_alpha",
                "actor_id":"human:alpha",
                "fleet_id":"fleet_shared_label",
                "spec":"Split one docs check",
                "goal":"Keep alpha's operator goal",
                "requested_width":24,
                "checks":["git diff --check"],
                "streams":[{
                    "stream_id":"docs",
                    "objective":"Check docs",
                    "write_scope":["docs/"],
                    "interface_contract":"Docs only",
                    "checks":["git diff --check"],
                    "max_turns":10,
                    "builder_slot":"",
                    "data_sensitivity":"internal"
                }]
            }"#,
            &kernel,
        )
        .expect("initial tenant plan");
        assert!(initial.body.contains(r#""status":"DRAFTED""#));

        assert_eq!(
            prior_plan_goal(&kernel, "tenant_alpha", "fleet_shared_label"),
            "Keep alpha's operator goal"
        );
        assert_eq!(
            prior_plan_lines(&kernel, "tenant_alpha", "fleet_shared_label", "checks"),
            vec!["git diff --check"]
        );
        assert_eq!(
            prior_plan_requested_width(&kernel, "tenant_alpha", "fleet_shared_label"),
            Some(24)
        );
        assert!(!prior_plan_context(&kernel, "tenant_alpha", "fleet_shared_label").is_empty());

        assert!(prior_plan_goal(&kernel, "tenant_beta", "fleet_shared_label").is_empty());
        assert!(
            prior_plan_lines(&kernel, "tenant_beta", "fleet_shared_label", "checks").is_empty()
        );
        assert_eq!(
            prior_plan_requested_width(&kernel, "tenant_beta", "fleet_shared_label"),
            None
        );
        assert!(prior_plan_context(&kernel, "tenant_beta", "fleet_shared_label").is_empty());

        let cross_tenant_replan = handle_draft(
            "POST",
            r#"{
                "tenant_id":"tenant_beta",
                "actor_id":"human:beta",
                "fleet_id":"fleet_shared_label",
                "spec":"Attempt to inherit alpha's plan"
            }"#,
            &kernel,
        )
        .expect("cross-tenant replan response");
        assert!(cross_tenant_replan.body.contains(r#""status":"REFUSED""#));
        assert!(cross_tenant_replan.body.contains("starts from a goal"));
        assert!(!cross_tenant_replan.body.contains("alpha's operator goal"));
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
