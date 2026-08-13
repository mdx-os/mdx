// The model scorecard: which coder is actually performing, derived from
// the chain itself - no separate metrics store, no self-reported scores.
// Every run's model_called receipts carry model= identity and token
// usage; every run_finished carries the terminal status. Folded per
// model, the scorecard answers the founder's question after 10, 20, 50
// real runs: who converges, who burns budget, who quits.
//
// This is the read side of the A/B/C/D lever (builder slots): same task,
// different coder, scored on the same chain. Read-only by construction.
use crate::RouteResponse;
use mdx_core::{MdxKernel, forge_fleet_model_matrix_profiles, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    _body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/model-scorecard.json" {
        return None;
    }
    Some(handle(method, kernel))
}

#[derive(Default)]
pub(crate) struct ModelScore {
    pub(crate) runs: std::collections::BTreeSet<String>,
    pub(crate) model_calls: u64,
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) done: u64,
    pub(crate) cannot_proceed: u64,
    pub(crate) budget_exhausted: u64,
    pub(crate) stopped: u64,
    pub(crate) errored: u64,
    pub(crate) turns_total: u64,
}

impl ModelScore {
    /// Runs that reached a terminal status (the fair denominator - a run
    /// that vanished before finishing is not counted as a success).
    pub(crate) fn finished(&self) -> u64 {
        self.done + self.cannot_proceed + self.budget_exhausted + self.stopped + self.errored
    }
    /// Fraction of finished runs that reached done, in [0,1].
    pub(crate) fn done_rate(&self) -> f64 {
        let finished = self.finished();
        if finished > 0 {
            self.done as f64 / finished as f64
        } else {
            0.0
        }
    }
    /// Average turns across finished runs.
    pub(crate) fn avg_turns(&self) -> f64 {
        let finished = self.finished();
        if finished > 0 {
            self.turns_total as f64 / finished as f64
        } else {
            0.0
        }
    }
}

/// Fold every forge run on the chain into a per-model track record, keyed
/// by the model identity its calls carried. The single source of truth for
/// both the scorecard surface and the planner's casting feedback - no
/// separate metrics store, no self-reported scores.
///
/// `min_turns_for_credit` is the fairness floor: a run that ended in fewer
/// turns than this WITHOUT reaching done was starved (a tiny budget, a
/// throwaway probe) and is not a fair test of the model, so its outcome is
/// not credited. A fast SUCCESS always counts. The scorecard surface passes
/// 0 (the honest full tally); the planner's casting feedback passes a real
/// floor so load-test noise cannot make it distrust a capable model.
pub(crate) fn model_scores(
    kernel: &MdxKernel,
    min_turns_for_credit: u64,
) -> std::collections::BTreeMap<String, ModelScore> {
    use std::collections::BTreeMap;
    // First pass: which model drove each run (the model= on its calls),
    // plus per-model call and token tallies.
    let mut run_model: BTreeMap<String, String> = BTreeMap::new();
    let mut scores: BTreeMap<String, ModelScore> = BTreeMap::new();
    let mut terminals: Vec<(String, String, u64)> = Vec::new(); // run, status, turns
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        let get = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
        let run_id = get("run_id");
        let event = get("event");
        let detail = get("detail");
        if event == "model_called" {
            if let Some(model) = field_in(&detail, "model=") {
                let score = scores.entry(model.clone()).or_default();
                score.runs.insert(run_id.clone());
                score.model_calls += 1;
                score.input_tokens += get("tokens_in").parse::<u64>().unwrap_or(0);
                score.output_tokens += get("tokens_out").parse::<u64>().unwrap_or(0);
                run_model.insert(run_id, model);
            }
        } else if event == "run_finished" {
            // A run that died before its status line (a model call that
            // failed after retries, a workspace that would not open) still
            // ENDED - it scores as errored instead of vanishing from the
            // tally. Twelve parallel benchmark runs taught this live: the
            // vanished deaths inflated every survivor's done rate.
            let status = field_in(&detail, "status=").unwrap_or_else(|| "RUN_ERRORED".to_string());
            let turns = field_in(&detail, "turns=")
                .and_then(|t| t.parse::<u64>().ok())
                .unwrap_or(0);
            terminals.push((run_id, status, turns));
        }
    }
    // Second pass: outcomes attribute to the model that drove the run.
    for (run_id, status, turns) in terminals {
        let Some(model) = run_model.get(&run_id) else {
            continue; // a run that never reached a model call scores nowhere
        };
        // The fairness floor: a starved short failure is not a fair test of
        // the model and must not drag its track record down. A success
        // always counts, however fast.
        if turns < min_turns_for_credit && status != "RUN_FINISHED_DONE" {
            continue;
        }
        let score = scores.entry(model.clone()).or_default();
        score.turns_total += turns;
        match status.as_str() {
            "RUN_FINISHED_DONE" => score.done += 1,
            "RUN_FINISHED_CANNOT_PROCEED" => score.cannot_proceed += 1,
            "RUN_BUDGET_EXHAUSTED" => score.budget_exhausted += 1,
            "RUN_STOPPED" => score.stopped += 1,
            _ => score.errored += 1,
        }
    }
    scores
}

/// A coarse work-type for a run, from how many DISTINCT files it edited -
/// the breadth of the change is the signal. Leaf work touches one module
/// (with or without its one-line registration); cross-cutting work spans
/// many. Segmenting by this lets a coder's record on the work it is GOOD at
/// stand apart from the work it is not, so the planner matches a stream to
/// the relevant evidence instead of one muddy average.
pub(crate) fn classify_work_type(distinct_files_edited: usize) -> &'static str {
    match distinct_files_edited {
        0 => "no_edit",
        1..=2 => "focused",
        3..=5 => "multi_file",
        _ => "cross_cutting",
    }
}

/// The edited path inside a tool_executed detail ("edit_file <path>" /
/// "write_file <path>"), or None for a read, a list, or a REFUSED edit (a
/// scope refusal changed nothing, so it is not a touched file).
fn edited_path(detail: &str) -> Option<String> {
    for prefix in ["edit_file ", "write_file "] {
        if let Some(rest) = detail.strip_prefix(prefix) {
            let rest = rest.trim();
            if rest.is_empty() || rest.starts_with("refused") {
                return None;
            }
            return Some(rest.to_string());
        }
    }
    None
}

/// The same fold as `model_scores`, but keyed by (model, work_type) so the
/// planner can read a coder's done-rate FOR THE KIND OF WORK a stream is,
/// not blended across kinds. Work-type comes from the distinct files each
/// run edited. Kept separate from `model_scores` so the scorecard surface
/// stays byte-identical.
pub(crate) fn model_scores_by_work_type(
    kernel: &MdxKernel,
    min_turns_for_credit: u64,
) -> std::collections::BTreeMap<(String, String), ModelScore> {
    use std::collections::{BTreeMap, BTreeSet};
    #[derive(Default)]
    struct RunAcc {
        model: String,
        calls: u64,
        input_tokens: u64,
        output_tokens: u64,
        files: BTreeSet<String>,
    }
    let mut runs: BTreeMap<String, RunAcc> = BTreeMap::new();
    let mut terminals: Vec<(String, String, u64)> = Vec::new();
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        let get = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
        let run_id = get("run_id");
        let detail = get("detail");
        match get("event").as_str() {
            "model_called" => {
                if let Some(model) = field_in(&detail, "model=") {
                    let acc = runs.entry(run_id).or_default();
                    acc.model = model;
                    acc.calls += 1;
                    acc.input_tokens += get("tokens_in").parse::<u64>().unwrap_or(0);
                    acc.output_tokens += get("tokens_out").parse::<u64>().unwrap_or(0);
                }
            }
            "tool_executed" => {
                if let Some(path) = edited_path(&detail) {
                    runs.entry(run_id).or_default().files.insert(path);
                }
            }
            "run_finished" => {
                let status =
                    field_in(&detail, "status=").unwrap_or_else(|| "RUN_ERRORED".to_string());
                let turns = field_in(&detail, "turns=")
                    .and_then(|t| t.parse::<u64>().ok())
                    .unwrap_or(0);
                terminals.push((run_id, status, turns));
            }
            _ => {}
        }
    }
    let mut scores: BTreeMap<(String, String), ModelScore> = BTreeMap::new();
    for (run_id, status, turns) in terminals {
        let Some(acc) = runs.get(&run_id) else {
            continue;
        };
        if acc.model.is_empty() {
            continue; // never reached a model call - scores nowhere
        }
        if turns < min_turns_for_credit && status != "RUN_FINISHED_DONE" {
            continue; // the fairness floor: a starved short failure is unfair
        }
        let work_type = classify_work_type(acc.files.len()).to_string();
        let score = scores.entry((acc.model.clone(), work_type)).or_default();
        score.runs.insert(run_id.clone());
        score.model_calls += acc.calls;
        score.input_tokens += acc.input_tokens;
        score.output_tokens += acc.output_tokens;
        score.turns_total += turns;
        match status.as_str() {
            "RUN_FINISHED_DONE" => score.done += 1,
            "RUN_FINISHED_CANNOT_PROCEED" => score.cannot_proceed += 1,
            "RUN_BUDGET_EXHAUSTED" => score.budget_exhausted += 1,
            "RUN_STOPPED" => score.stopped += 1,
            _ => score.errored += 1,
        }
    }
    scores
}

/// One coder's run of a shared task, for the A/B read.
pub(crate) struct AbRun {
    pub model: String,
    pub status: String,
    pub turns: u64,
    pub tokens: u64,
}

/// A same-task comparison: the SAME work item run under two or more distinct
/// coders (the A/B/C/D builder-slot lever). These are the fairest read of who
/// is better - identical task, different model, scored on the same chain.
pub(crate) struct AbComparison {
    pub work_item: String,
    pub runs: Vec<AbRun>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BuilderCastingEvidence {
    pub(crate) status: String,
    pub(crate) requested_builder_slot: String,
    pub(crate) selected_builder_slot: String,
    pub(crate) recommended_builder_slot: String,
    pub(crate) selected_model_profile_id: String,
    pub(crate) selected_provider_family: String,
    pub(crate) selected_model_id: String,
    pub(crate) recommended_model_profile_id: String,
    pub(crate) recommended_provider_family: String,
    pub(crate) recommended_model_id: String,
    pub(crate) basis: String,
    pub(crate) matching_eval_score_count: u32,
    pub(crate) accepted_eval_score_count: u32,
    pub(crate) matching_run_count: u32,
    pub(crate) done_rate_pct: u32,
    pub(crate) requested_slot_matches_evidence: bool,
    /// The grant receipt of the ratified lesson that steered this casting, if
    /// any. Non-empty only when a human-ratified fleet_casting grant (not an
    /// explicit per-run slot) chose the builder. The run start seam cites it in
    /// a learning.adaptation.applied receipt.
    pub(crate) ratified_grant_receipt_id: String,
}

impl BuilderCastingEvidence {
    pub(crate) fn selected_slot_for_execution(&self) -> &str {
        self.selected_builder_slot.as_str()
    }
}

/// Find every work item that has been built by two or more distinct coders,
/// and lay their outcomes side by side. Folded from the run receipts: each
/// run carries its work item, the model its calls named, and its terminal
/// status and turns.
pub(crate) fn ab_comparisons(kernel: &MdxKernel) -> Vec<AbComparison> {
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Acc {
        work_item: String,
        model: String,
        status: String,
        turns: u64,
        tokens: u64,
    }
    let mut runs: BTreeMap<String, Acc> = BTreeMap::new();
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        let get = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
        let run_id = get("run_id");
        if run_id.is_empty() {
            continue;
        }
        let acc = runs.entry(run_id).or_default();
        let work_item = get("work_item_id");
        if !work_item.is_empty() {
            acc.work_item = work_item;
        }
        let detail = get("detail");
        match get("event").as_str() {
            "model_called" => {
                if let Some(model) = field_in(&detail, "model=") {
                    acc.model = model;
                }
                acc.tokens += get("tokens_in").parse::<u64>().unwrap_or(0)
                    + get("tokens_out").parse::<u64>().unwrap_or(0);
            }
            "run_finished" => {
                // A run_finished with no parseable status= ended abnormally (a
                // model error, a workspace that would not open) - it errored,
                // it did not just keep "working". An unfinished run keeps the
                // empty default and reads as still in flight.
                acc.status =
                    field_in(&detail, "status=").unwrap_or_else(|| "RUN_ERROR".to_string());
                acc.turns = field_in(&detail, "turns=")
                    .and_then(|t| t.parse::<u64>().ok())
                    .unwrap_or(0);
            }
            _ => {}
        }
    }
    let mut by_item: BTreeMap<String, Vec<Acc>> = BTreeMap::new();
    for (_, acc) in runs {
        if acc.work_item.is_empty() || acc.model.is_empty() {
            continue;
        }
        by_item.entry(acc.work_item.clone()).or_default().push(acc);
    }
    let mut out = Vec::new();
    for (work_item, accs) in by_item {
        let mut models: Vec<&String> = accs.iter().map(|a| &a.model).collect();
        models.sort();
        models.dedup();
        if models.len() < 2 {
            continue; // not a comparison: only one coder built it
        }
        out.push(AbComparison {
            work_item,
            runs: accs
                .into_iter()
                .map(|a| AbRun {
                    model: a.model,
                    status: a.status,
                    turns: a.turns,
                    tokens: a.tokens,
                })
                .collect(),
        });
    }
    out
}

/// Decide which builder lane the evidence supports for a language/task.
///
/// This is deliberately advisory and receipt-derived. It may auto-select a
/// configured slot only when accepted eval evidence or completed local run
/// evidence points at a provider family and no explicit slot was requested.
/// Otherwise the requested/default slot wins and the evidence records why.
pub(crate) fn builder_casting_evidence(
    kernel: &MdxKernel,
    language_pack_id: &str,
    task_class: &str,
    complexity_tier: &str,
    requested_builder_slot: &str,
) -> BuilderCastingEvidence {
    let requested_builder_slot = requested_builder_slot.trim().to_string();
    let eval = best_eval_profile(kernel, language_pack_id, task_class, complexity_tier);
    let historical = best_historical_model(kernel, language_pack_id, task_class, complexity_tier);
    let (profile_id, provider_family, model_id, basis, status_seed) =
        if let Some(eval) = eval.recommended.clone() {
            (
                eval.model_profile_id,
                eval.provider_family,
                eval.model_id,
                "accepted_eval_scoreboard_result".to_string(),
                "EVIDENCE_BACKED".to_string(),
            )
        } else if let Some(model) = historical.recommended_model.clone() {
            let (profile_id, provider_family) = model_profile_for_model(&model);
            (
                profile_id.to_string(),
                provider_family.to_string(),
                model,
                "local_run_track_record".to_string(),
                "LOCAL_RUN_EVIDENCE".to_string(),
            )
        } else {
            (
                String::new(),
                String::new(),
                String::new(),
                "no_matching_language_task_model_evidence_yet".to_string(),
                "INSUFFICIENT_EVIDENCE".to_string(),
            )
        };

    let recommended_builder_slot = configured_builder_slot_for_provider(&provider_family);
    let requested_slot_matches_evidence = requested_builder_slot.is_empty()
        || recommended_builder_slot.is_empty()
        || normalize_slot(&requested_builder_slot) == normalize_slot(&recommended_builder_slot);
    // Precedence: an explicit per-run human slot wins first; then a ratified
    // fleet_casting lesson; then the automatic eval/run evidence. A ratified
    // grant only steers when no explicit slot was asked for, and only when the
    // slot it names is actually configured to run.
    let ratified = if requested_builder_slot.is_empty() {
        ratified_fleet_casting_slot(kernel)
    } else {
        None
    };
    let (selected_builder_slot, ratified_grant_receipt_id, basis) =
        if !requested_builder_slot.is_empty() {
            (requested_builder_slot.clone(), String::new(), basis)
        } else if let Some(grant) = ratified.as_ref() {
            (
                grant.slot.clone(),
                grant.grant_receipt_id.clone(),
                "ratified_fleet_casting_grant".to_string(),
            )
        } else {
            (recommended_builder_slot.clone(), String::new(), basis)
        };
    let (selected_model_profile_id, selected_provider_family, selected_model_id) =
        selected_builder_identity(
            &selected_builder_slot,
            &profile_id,
            &provider_family,
            &model_id,
        );
    let status = if !ratified_grant_receipt_id.is_empty() {
        "RATIFIED_ADAPTATION_SLOT_READY".to_string()
    } else if status_seed == "INSUFFICIENT_EVIDENCE" {
        status_seed
    } else if !requested_builder_slot.is_empty() && !requested_slot_matches_evidence {
        format!("{status_seed}_REQUESTED_SLOT_OVERRIDES_EVIDENCE")
    } else if recommended_builder_slot.is_empty() {
        format!("{status_seed}_PROFILE_UNCONFIGURED")
    } else {
        format!("{status_seed}_SLOT_READY")
    };

    BuilderCastingEvidence {
        status,
        requested_builder_slot,
        selected_builder_slot,
        recommended_builder_slot,
        selected_model_profile_id,
        selected_provider_family,
        selected_model_id,
        recommended_model_profile_id: profile_id,
        recommended_provider_family: provider_family,
        recommended_model_id: model_id,
        basis,
        matching_eval_score_count: eval.matching_score_count,
        accepted_eval_score_count: eval.accepted_score_count,
        matching_run_count: historical.matching_run_count,
        done_rate_pct: historical.done_rate_pct,
        requested_slot_matches_evidence,
        ratified_grant_receipt_id,
    }
}

/// A ratified fleet_casting grant that names a configured builder slot, read at
/// decision time. Skips grants that were withdrawn or whose activation was
/// superseded (both handled by `active_fleet_casting_grants`), and skips a
/// named slot that is not actually configured to run. The first such grant in
/// ledger order wins - deterministic and receipt-derived.
struct RatifiedCastingSlot {
    grant_receipt_id: String,
    slot: String,
}

fn ratified_fleet_casting_slot(kernel: &MdxKernel) -> Option<RatifiedCastingSlot> {
    for grant in kernel.active_fleet_casting_grants() {
        let slot = grant.preferred_builder_slot.trim();
        if slot.is_empty() {
            continue;
        }
        if !builder_slot_ready(slot) {
            continue;
        }
        return Some(RatifiedCastingSlot {
            grant_receipt_id: grant.grant_receipt_id,
            slot: slot.to_string(),
        });
    }
    None
}

fn selected_builder_identity(
    selected_builder_slot: &str,
    recommended_model_profile_id: &str,
    recommended_provider_family: &str,
    recommended_model_id: &str,
) -> (String, String, String) {
    let selected_model_id = builder_slot_model_id(selected_builder_slot)
        .or_else(default_xai_builder_model_id)
        .unwrap_or_else(|| recommended_model_id.trim().to_string());
    if !selected_model_id.trim().is_empty() {
        let (profile_id, provider_family) = model_profile_for_model(&selected_model_id);
        return (
            profile_id.to_string(),
            provider_family.to_string(),
            selected_model_id,
        );
    }
    (
        recommended_model_profile_id.trim().to_string(),
        recommended_provider_family.trim().to_string(),
        String::new(),
    )
}

fn builder_slot_model_id(slot: &str) -> Option<String> {
    let prefix = if slot.trim().is_empty() {
        "MDX_FLEET_BUILDER".to_string()
    } else {
        format!("MDX_FLEET_BUILDER_{}", slot.trim().to_ascii_uppercase())
    };
    std::env::var(format!("{prefix}_MODEL"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn default_xai_builder_model_id() -> Option<String> {
    std::env::var("MDX_XAI_BUILD_MODEL")
        .ok()
        .or_else(|| std::env::var("MDX_XAI_MODEL").ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[derive(Clone, Debug, Default)]
struct EvalProfileCandidate {
    model_profile_id: String,
    provider_family: String,
    model_id: String,
    score: u32,
}

#[derive(Clone, Debug, Default)]
struct EvalProfileEvidence {
    recommended: Option<EvalProfileCandidate>,
    matching_score_count: u32,
    accepted_score_count: u32,
}

fn best_eval_profile(
    kernel: &MdxKernel,
    language_pack_id: &str,
    task_class: &str,
    complexity_tier: &str,
) -> EvalProfileEvidence {
    let mut evidence = EvalProfileEvidence::default();
    for receipt in kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "forge.fleet_eval_result.scored")
    {
        if !field_matches(payload(receipt, "language_pack_id"), language_pack_id)
            || !class_matches(
                first_present(&[
                    payload(receipt, "language_task_class"),
                    payload(receipt, "benchmark_task_class"),
                ]),
                task_class,
            )
            || !tier_matches(
                first_present(&[
                    payload(receipt, "language_complexity_tier"),
                    payload(receipt, "complexity_tier"),
                ]),
                complexity_tier,
            )
        {
            continue;
        }
        evidence.matching_score_count += 1;
        let accepted = payload(receipt, "accepted_for_scoreboard") == "true"
            && payload(receipt, "mdx_quality_gates_passed") == "true";
        if !accepted {
            continue;
        }
        evidence.accepted_score_count += 1;
        let score = payload(receipt, "total_score").parse::<u32>().unwrap_or(0);
        let candidate = EvalProfileCandidate {
            model_profile_id: payload(receipt, "model_profile_id").to_string(),
            provider_family: payload(receipt, "provider_family").to_string(),
            model_id: payload(receipt, "model_id").to_string(),
            score,
        };
        if evidence
            .recommended
            .as_ref()
            .map(|best| candidate.score > best.score)
            .unwrap_or(true)
        {
            evidence.recommended = Some(candidate);
        }
    }
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        if receipt.payload.get("event").map(String::as_str) != Some("evidence_appended")
            || receipt
                .payload
                .get("eval_principal_review_status")
                .map(String::as_str)
                != Some("accepted_for_scoreboard")
            || !field_matches(payload(receipt, "language_pack_id"), language_pack_id)
            || !class_matches(payload(receipt, "language_task_class"), task_class)
            || !tier_matches(
                payload(receipt, "language_complexity_tier"),
                complexity_tier,
            )
        {
            continue;
        }
        evidence.matching_score_count += 1;
        evidence.accepted_score_count += 1;
        let score = payload(receipt, "total_score").parse::<u32>().unwrap_or(0);
        let candidate = EvalProfileCandidate {
            model_profile_id: payload(receipt, "model_profile_id").to_string(),
            provider_family: payload(receipt, "provider_family").to_string(),
            model_id: payload(receipt, "model_id").to_string(),
            score,
        };
        if evidence
            .recommended
            .as_ref()
            .map(|best| candidate.score > best.score)
            .unwrap_or(true)
        {
            evidence.recommended = Some(candidate);
        }
    }
    evidence
}

#[derive(Clone, Debug, Default)]
struct HistoricalModelEvidence {
    recommended_model: Option<String>,
    matching_run_count: u32,
    done_rate_pct: u32,
}

fn best_historical_model(
    kernel: &MdxKernel,
    language_pack_id: &str,
    task_class: &str,
    complexity_tier: &str,
) -> HistoricalModelEvidence {
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct RunAcc {
        language_pack_id: String,
        task_class: String,
        complexity_tier: String,
        model: String,
        terminal_status: String,
        turns: u64,
    }
    #[derive(Default)]
    struct ModelAcc {
        finished: u32,
        done: u32,
        turns_total: u64,
    }
    let mut runs: BTreeMap<String, RunAcc> = BTreeMap::new();
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        let run_id = payload(receipt, "run_id").to_string();
        if run_id.is_empty() {
            continue;
        }
        let acc = runs.entry(run_id).or_default();
        let detail = payload(receipt, "detail");
        match payload(receipt, "event") {
            "run_started" => {
                acc.language_pack_id = payload(receipt, "language_pack_id").to_string();
                acc.task_class = first_present(&[
                    payload(receipt, "language_task_class"),
                    payload(receipt, "work_classification_task_class"),
                ])
                .to_string();
                acc.complexity_tier = first_present(&[
                    payload(receipt, "language_task_complexity_tier"),
                    payload(receipt, "work_classification_complexity_tier"),
                ])
                .to_string();
            }
            "model_called" => {
                if let Some(model) = field_in(detail, "model=") {
                    acc.model = model;
                }
            }
            "run_finished" => {
                acc.terminal_status =
                    field_in(detail, "status=").unwrap_or_else(|| "RUN_ERROR".to_string());
                acc.turns = field_in(detail, "turns=")
                    .and_then(|turns| turns.parse::<u64>().ok())
                    .unwrap_or(0);
            }
            _ => {}
        }
    }
    let mut by_model: BTreeMap<String, ModelAcc> = BTreeMap::new();
    for (_, run) in runs {
        if run.model.is_empty()
            || run.terminal_status.is_empty()
            || !field_matches(&run.language_pack_id, language_pack_id)
            || !class_matches(&run.task_class, task_class)
            || !tier_matches(&run.complexity_tier, complexity_tier)
        {
            continue;
        }
        if run.turns < 5 && run.terminal_status != "RUN_FINISHED_DONE" {
            continue;
        }
        let acc = by_model.entry(run.model).or_default();
        acc.finished += 1;
        acc.turns_total += run.turns;
        if run.terminal_status == "RUN_FINISHED_DONE" {
            acc.done += 1;
        }
    }
    let mut best_model = None::<(String, u32, u64, u32)>;
    let mut matching_run_count = 0_u32;
    for (model, acc) in by_model {
        matching_run_count += acc.finished;
        let done_rate_pct = acc
            .done
            .checked_mul(100)
            .and_then(|value| value.checked_div(acc.finished))
            .unwrap_or(0)
            .min(100);
        let avg_turns = if acc.finished > 0 {
            acc.turns_total / u64::from(acc.finished)
        } else {
            0
        };
        if best_model
            .as_ref()
            .map(|(_, best_rate, best_turns, best_finished)| {
                done_rate_pct > *best_rate
                    || done_rate_pct == *best_rate
                        && (acc.finished > *best_finished
                            || acc.finished == *best_finished && avg_turns < *best_turns)
            })
            .unwrap_or(true)
        {
            best_model = Some((model, done_rate_pct, avg_turns, acc.finished));
        }
    }
    HistoricalModelEvidence {
        recommended_model: best_model.as_ref().map(|(model, _, _, _)| model.clone()),
        matching_run_count,
        done_rate_pct: best_model.map(|(_, rate, _, _)| rate).unwrap_or(0),
    }
}

fn configured_builder_slot_for_provider(provider_family: &str) -> String {
    if provider_family.trim().is_empty() {
        return String::new();
    }
    for slot in [
        "OPUS",
        "SONNET",
        "GEMINI",
        "GROK",
        "XAI",
        "BEDROCK",
        "CODEX",
        "CODEXMINI",
        "",
    ] {
        if builder_slot_ready(slot) && slot_matches_provider(slot, provider_family) {
            return slot.to_string();
        }
    }
    String::new()
}

fn builder_slot_ready(slot: &str) -> bool {
    let prefix = if slot.trim().is_empty() {
        "MDX_FLEET_BUILDER".to_string()
    } else {
        format!("MDX_FLEET_BUILDER_{}", slot.trim().to_ascii_uppercase())
    };
    for suffix in ["BASE_URL", "API_KEY", "MODEL"] {
        if std::env::var(format!("{prefix}_{suffix}"))
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
        {
            return false;
        }
    }
    true
}

fn slot_matches_provider(slot: &str, provider_family: &str) -> bool {
    let slot_upper = slot.trim().to_ascii_uppercase();
    let prefix = if slot_upper.is_empty() {
        "MDX_FLEET_BUILDER".to_string()
    } else {
        format!("MDX_FLEET_BUILDER_{slot_upper}")
    };
    let model = std::env::var(format!("{prefix}_MODEL")).unwrap_or_default();
    let base_url = std::env::var(format!("{prefix}_BASE_URL")).unwrap_or_default();
    let haystack = format!("{slot_upper} {model} {base_url}").to_ascii_lowercase();
    match provider_family {
        "anthropic" => {
            haystack.contains("anthropic")
                || haystack.contains("claude")
                || haystack.contains("opus")
                || haystack.contains("sonnet")
        }
        "gemini" => haystack.contains("gemini") || haystack.contains("google"),
        "xai" => haystack.contains("xai") || haystack.contains("grok"),
        "aws_bedrock" => haystack.contains("bedrock") || haystack.contains("aws"),
        _ => false,
    }
}

fn model_profile_for_model(model: &str) -> (&'static str, &'static str) {
    let lower = model.to_ascii_lowercase();
    let profile_id = if lower.contains("gemini") {
        "codex_gemini_responses_profile"
    } else if lower.contains("gpt") || lower.contains("openai") || lower.contains("codex") {
        "codex_openai_responses_profile"
    } else if lower.contains("claude")
        || lower.contains("anthropic")
        || lower.contains("opus")
        || lower.contains("sonnet")
    {
        "codex_anthropic_responses_profile"
    } else if lower.contains("bedrock") || lower.contains("aws") {
        "codex_bedrock_responses_profile"
    } else if lower.contains("grok") || lower.contains("xai") {
        "codex_xai_responses_profile"
    } else {
        "codex_openai_responses_profile"
    };
    let provider_family = forge_fleet_model_matrix_profiles()
        .into_iter()
        .find(|profile| profile.profile_id == profile_id)
        .map(|profile| profile.provider_family)
        .unwrap_or("openai");
    (profile_id, provider_family)
}

fn field_matches(left: &str, right: &str) -> bool {
    let right = right.trim();
    right.is_empty() || left.trim() == right
}

fn class_matches(left: &str, right: &str) -> bool {
    field_matches(left, &normalize_task_class(right))
}

fn tier_matches(left: &str, right: &str) -> bool {
    field_matches(left, &normalize_complexity_tier(right))
}

fn normalize_task_class(class: &str) -> String {
    match class.trim() {
        "docs_code" | "product_ux" => "feature",
        "multi_file" | "architecture" | "api_compat" | "migration" | "concurrency"
        | "observability" | "long_horizon" => "refactor",
        value => value,
    }
    .to_string()
}

fn normalize_complexity_tier(tier: &str) -> String {
    match tier.trim() {
        "xl" | "extreme" => "large",
        value => value,
    }
    .to_string()
}

fn normalize_slot(slot: &str) -> String {
    slot.trim().to_ascii_uppercase()
}

fn first_present<'a>(values: &[&'a str]) -> &'a str {
    values
        .iter()
        .copied()
        .find(|value| !value.trim().is_empty())
        .unwrap_or("")
}

fn payload<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn handle(method: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let scores = model_scores(&kernel, 0);
    let rows: Vec<String> = scores
        .iter()
        .map(|(model, s)| {
            format!(
                r#"{{"model":{},"runs":{},"finished_runs":{},"done":{},"cannot_proceed":{},"budget_exhausted":{},"stopped":{},"errored":{},"done_rate":{:.3},"avg_turns":{:.1},"model_calls":{},"input_tokens":{},"output_tokens":{}}}"#,
                json_string_literal(model),
                s.runs.len(),
                s.finished(),
                s.done,
                s.cannot_proceed,
                s.budget_exhausted,
                s.stopped,
                s.errored,
                s.done_rate(),
                s.avg_turns(),
                s.model_calls,
                s.input_tokens,
                s.output_tokens,
            )
        })
        .collect();
    // The fair-attempt view, segmented by kind of work: the same evidence the
    // planner casts on, so the operator sees WHICH model is good at WHAT, not
    // a single blended rate. Starved short failures are excluded (the fairness
    // floor) so load-test noise does not bury a capable coder.
    let segments = model_scores_by_work_type(&kernel, 5);
    let segment_rows: Vec<String> = segments
        .iter()
        .map(|((model, work_type), s)| {
            format!(
                r#"{{"model":{},"work_type":{},"runs":{},"done":{},"done_rate":{:.3},"avg_turns":{:.1},"input_tokens":{},"output_tokens":{}}}"#,
                json_string_literal(model),
                json_string_literal(work_type),
                s.finished(),
                s.done,
                s.done_rate(),
                s.avg_turns(),
                s.input_tokens,
                s.output_tokens,
            )
        })
        .collect();
    // Same-task A/B reads: the same work item built by two or more coders -
    // the fairest comparison of who is better, identical task notwithstanding.
    let ab = ab_comparisons(&kernel);
    let ab_rows: Vec<String> = ab
        .iter()
        .map(|c| {
            let runs: Vec<String> = c
                .runs
                .iter()
                .map(|r| {
                    format!(
                        r#"{{"model":{},"status":{},"turns":{},"tokens":{}}}"#,
                        json_string_literal(&r.model),
                        json_string_literal(&r.status),
                        r.turns,
                        r.tokens,
                    )
                })
                .collect();
            format!(
                r#"{{"work_item":{},"runs":[{}]}}"#,
                json_string_literal(&c.work_item),
                runs.join(","),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-model-scorecard","receipt_kind":"forge.run.event","model_count":{},"models":[{}],"by_work_type":[{}],"ab_comparisons":[{}],"note":"derived entirely from run receipts - same-task slot comparisons are the fair reads; by_work_type is the fair-attempt view the planner casts on","production_write_allowed":false}}"#,
            rows.len(),
            rows.join(","),
            segment_rows.join(","),
            ab_rows.join(","),
        ),
    ))
}

/// The value of `key=` inside a presence-detail string, up to whitespace.
fn field_in(detail: &str, key: &str) -> Option<String> {
    let start = detail.find(key)? + key.len();
    let rest = &detail[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let value = rest[..end].trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev<'a>(run: &'a str, event: &'a str, detail: &'a str) -> mdx_core::ForgeRunEvent<'a> {
        mdx_core::ForgeRunEvent {
            tenant_id: "t",
            actor_id: "a",
            run_id: run,
            event,
            work_item_id: "w",
            detail,
            turn: 0,
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    #[test]
    fn model_profile_mapping_never_labels_gpt_as_xai() {
        assert_eq!(
            model_profile_for_model("gpt-5.6-sol"),
            ("codex_openai_responses_profile", "openai")
        );
        assert_eq!(
            model_profile_for_model("grok-4.5"),
            ("codex_xai_responses_profile", "xai")
        );
    }

    #[test]
    fn the_fairness_floor_drops_starved_failures_but_keeps_fast_wins() {
        let mut kernel = MdxKernel::boot_local();
        // run A: a starved 2-turn FAILURE (a throwaway probe) - must not count.
        let _ = kernel.record_forge_run_event(ev("rA", "model_called", "model=cheap"));
        let _ = kernel.record_forge_run_event(ev(
            "rA",
            "run_finished",
            "status=RUN_BUDGET_EXHAUSTED turns=2",
        ));
        // run B: a fast 2-turn SUCCESS - always counts.
        let _ = kernel.record_forge_run_event(ev("rB", "model_called", "model=cheap"));
        let _ = kernel.record_forge_run_event(ev(
            "rB",
            "run_finished",
            "status=RUN_FINISHED_DONE turns=2",
        ));
        // run C: a fair 20-turn FAILURE - the model got its shot, so it counts.
        let _ = kernel.record_forge_run_event(ev("rC", "model_called", "model=cheap"));
        let _ = kernel.record_forge_run_event(ev(
            "rC",
            "run_finished",
            "status=RUN_BUDGET_EXHAUSTED turns=20",
        ));

        // Honest full tally (floor 0): all three count -> 1 of 3 done.
        let full = model_scores(&kernel, 0);
        assert_eq!(full["cheap"].finished(), 3);
        assert!((full["cheap"].done_rate() - 1.0 / 3.0).abs() < 1e-6);

        // The planner's fair view (floor 5): the starved failure is dropped,
        // so the cheap coder reads 1 of 2 done - its real rate, not noise.
        let fair = model_scores(&kernel, 5);
        assert_eq!(fair["cheap"].finished(), 2);
        assert!((fair["cheap"].done_rate() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn work_type_buckets_by_breadth_and_edited_path_skips_refusals() {
        assert_eq!(classify_work_type(0), "no_edit");
        assert_eq!(classify_work_type(1), "focused");
        assert_eq!(classify_work_type(2), "focused");
        assert_eq!(classify_work_type(4), "multi_file");
        assert_eq!(classify_work_type(9), "cross_cutting");
        assert_eq!(
            edited_path("edit_file crates/x.rs"),
            Some("crates/x.rs".into())
        );
        assert_eq!(edited_path("write_file a/b.rs"), Some("a/b.rs".into()));
        assert_eq!(edited_path("edit_file refused crates/x.rs"), None);
        assert_eq!(edited_path("read_file crates/x.rs"), None);
    }

    #[test]
    fn the_track_record_separates_a_coders_focused_work_from_its_cross_cutting_work() {
        let mut kernel = MdxKernel::boot_local();
        // run F: cheap coder edits ONE module and finishes done -> focused win.
        let _ = kernel.record_forge_run_event(ev("rF", "model_called", "model=cheap"));
        let _ = kernel.record_forge_run_event(ev("rF", "tool_executed", "edit_file crates/m.rs"));
        let _ = kernel.record_forge_run_event(ev(
            "rF",
            "run_finished",
            "status=RUN_FINISHED_DONE turns=20",
        ));
        // run X: cheap coder edits SIX files and exhausts budget -> cross-cutting miss.
        let _ = kernel.record_forge_run_event(ev("rX", "model_called", "model=cheap"));
        for p in [
            "edit_file a.rs",
            "edit_file b.rs",
            "edit_file c.rs",
            "write_file d.rs",
            "edit_file e.rs",
            "edit_file f.rs",
        ] {
            let _ = kernel.record_forge_run_event(ev("rX", "tool_executed", p));
        }
        let _ = kernel.record_forge_run_event(ev(
            "rX",
            "run_finished",
            "status=RUN_BUDGET_EXHAUSTED turns=30",
        ));
        let scores = model_scores_by_work_type(&kernel, 5);
        let focused = &scores[&("cheap".to_string(), "focused".to_string())];
        let cross = &scores[&("cheap".to_string(), "cross_cutting".to_string())];
        // The coder's focused record (100%) stands apart from its cross-cutting
        // record (0%) - the blend that made the planner over-cautious is gone.
        assert_eq!(focused.done, 1);
        assert!((focused.done_rate() - 1.0).abs() < 1e-6);
        assert_eq!(cross.done, 0);
        assert!((cross.done_rate() - 0.0).abs() < 1e-6);
    }

    fn ev_w<'a>(
        run: &'a str,
        work: &'a str,
        event: &'a str,
        detail: &'a str,
    ) -> mdx_core::ForgeRunEvent<'a> {
        mdx_core::ForgeRunEvent {
            tenant_id: "t",
            actor_id: "a",
            run_id: run,
            event,
            work_item_id: work,
            detail,
            turn: 0,
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    #[test]
    fn ab_compares_only_work_items_built_by_two_or_more_coders() {
        let mut kernel = MdxKernel::boot_local();
        // work item W: built by Opus (done) AND grok (budget) -> a comparison.
        let _ = kernel.record_forge_run_event(ev_w("r1", "W", "model_called", "model=opus"));
        let _ = kernel.record_forge_run_event(ev_w(
            "r1",
            "W",
            "run_finished",
            "status=RUN_FINISHED_DONE turns=9",
        ));
        let _ = kernel.record_forge_run_event(ev_w("r2", "W", "model_called", "model=grok"));
        let _ = kernel.record_forge_run_event(ev_w(
            "r2",
            "W",
            "run_finished",
            "status=RUN_BUDGET_EXHAUSTED turns=25",
        ));
        // work item Z: only grok built it -> not a comparison.
        let _ = kernel.record_forge_run_event(ev_w("r3", "Z", "model_called", "model=grok"));
        let _ = kernel.record_forge_run_event(ev_w(
            "r3",
            "Z",
            "run_finished",
            "status=RUN_FINISHED_DONE turns=12",
        ));
        let ab = ab_comparisons(&kernel);
        assert_eq!(ab.len(), 1, "only W has two coders");
        assert_eq!(ab[0].work_item, "W");
        assert_eq!(ab[0].runs.len(), 2);
    }

    #[test]
    fn detail_fields_parse_from_presence_strings() {
        assert_eq!(
            field_in(
                "model=grok-build-0.1 finish_reason=stop tool_calls=2",
                "model="
            ),
            Some("grok-build-0.1".to_string())
        );
        assert_eq!(
            field_in("status=RUN_FINISHED_DONE turns=9 files_changed=3", "turns="),
            Some("9".to_string())
        );
        assert_eq!(field_in("no model here", "model="), None);
    }

    #[test]
    fn builder_casting_recommends_configured_slot_from_local_language_evidence() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        set_slot_env(
            "OPUS",
            "https://api.anthropic.test/v1",
            "test-key",
            "claude-opus-4-8",
        );
        clear_slot_env("XAI");
        clear_slot_env("GROK");
        let mut kernel = MdxKernel::boot_local();
        seed_language_run(
            &mut kernel,
            SeedLanguageRun {
                run_id: "r_swift_opus",
                language_pack_id: "swift-spm",
                task_class: "feature",
                complexity_tier: "medium",
                model: "claude-opus-4-8",
                status: "RUN_FINISHED_DONE",
                turns: 14,
            },
        );

        let evidence = builder_casting_evidence(&kernel, "swift-spm", "feature", "medium", "");

        assert_eq!(evidence.status, "LOCAL_RUN_EVIDENCE_SLOT_READY");
        assert_eq!(evidence.recommended_builder_slot, "OPUS");
        assert_eq!(evidence.selected_builder_slot, "OPUS");
        assert_eq!(
            evidence.recommended_model_profile_id,
            "codex_anthropic_responses_profile"
        );
        assert_eq!(evidence.recommended_provider_family, "anthropic");
        assert_eq!(evidence.matching_run_count, 1);
        assert_eq!(evidence.done_rate_pct, 100);
        assert!(evidence.requested_slot_matches_evidence);
        clear_slot_env("OPUS");
    }

    #[test]
    fn builder_casting_preserves_explicit_human_slot_override() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        set_slot_env(
            "OPUS",
            "https://api.anthropic.test/v1",
            "test-key",
            "claude-opus-4-8",
        );
        set_slot_env("XAI", "https://api.x.ai/v1", "test-key", "grok-4.3");
        let mut kernel = MdxKernel::boot_local();
        seed_language_run(
            &mut kernel,
            SeedLanguageRun {
                run_id: "r_swift_opus",
                language_pack_id: "swift-spm",
                task_class: "feature",
                complexity_tier: "medium",
                model: "claude-opus-4-8",
                status: "RUN_FINISHED_DONE",
                turns: 14,
            },
        );

        let evidence = builder_casting_evidence(&kernel, "swift-spm", "feature", "medium", "XAI");

        assert_eq!(
            evidence.status,
            "LOCAL_RUN_EVIDENCE_REQUESTED_SLOT_OVERRIDES_EVIDENCE"
        );
        assert_eq!(evidence.recommended_builder_slot, "OPUS");
        assert_eq!(evidence.selected_builder_slot, "XAI");
        assert!(!evidence.requested_slot_matches_evidence);
        clear_slot_env("OPUS");
        clear_slot_env("XAI");
    }

    #[test]
    fn builder_casting_prefers_accepted_principal_review_eval_evidence() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        set_slot_env(
            "GEMINI",
            "https://generativelanguage.googleapis.test/v1",
            "test-key",
            "gemini-2.5-pro",
        );
        let mut kernel = MdxKernel::boot_local();
        seed_language_run(
            &mut kernel,
            SeedLanguageRun {
                run_id: "r_swift_opus",
                language_pack_id: "swift-spm",
                task_class: "feature",
                complexity_tier: "medium",
                model: "claude-opus-4-8",
                status: "RUN_FINISHED_DONE",
                turns: 14,
            },
        );
        kernel
            .record_forge_run_event_with_evidence_fields(
                ev(
                    "r_swift_gemini_review",
                    "evidence_appended",
                    "eval_principal_reviewed status=accepted_for_scoreboard",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("human:test"),
                &[
                    ("eval_principal_review_status", "accepted_for_scoreboard"),
                    ("language_pack_id", "swift-spm"),
                    ("language_task_class", "feature"),
                    ("language_complexity_tier", "medium"),
                    ("model_profile_id", "codex_gemini_responses_profile"),
                    ("provider_family", "gemini"),
                    ("model_id", "gemini-2.5-pro"),
                    ("total_score", "97"),
                ],
            )
            .expect("principal review accepted");

        let evidence = builder_casting_evidence(&kernel, "swift-spm", "feature", "medium", "");

        assert_eq!(evidence.status, "EVIDENCE_BACKED_SLOT_READY");
        assert_eq!(evidence.recommended_builder_slot, "GEMINI");
        assert_eq!(evidence.selected_builder_slot, "GEMINI");
        assert_eq!(
            evidence.recommended_model_profile_id,
            "codex_gemini_responses_profile"
        );
        assert_eq!(evidence.accepted_eval_score_count, 1);
        assert_eq!(evidence.matching_eval_score_count, 1);
        clear_slot_env("GEMINI");
    }

    struct SeedLanguageRun<'a> {
        run_id: &'a str,
        language_pack_id: &'a str,
        task_class: &'a str,
        complexity_tier: &'a str,
        model: &'a str,
        status: &'a str,
        turns: u64,
    }

    fn seed_language_run(kernel: &mut MdxKernel, run: SeedLanguageRun<'_>) {
        kernel
            .record_forge_run_event_with_evidence_fields(
                ev(run.run_id, "run_started", "accepted"),
                &mdx_core::GovernedWriteIdentity::local_demo("human:test"),
                &[
                    ("language_pack_id", run.language_pack_id),
                    ("language_task_class", run.task_class),
                    ("language_task_complexity_tier", run.complexity_tier),
                ],
            )
            .expect("run started");
        kernel
            .record_forge_run_event(ev(
                run.run_id,
                "model_called",
                &format!("model={}", run.model),
            ))
            .expect("model called");
        kernel
            .record_forge_run_event(ev(
                run.run_id,
                "run_finished",
                &format!("status={} turns={}", run.status, run.turns),
            ))
            .expect("run finished");
    }

    fn set_slot_env(slot: &str, base_url: &str, api_key: &str, model: &str) {
        let prefix = format!("MDX_FLEET_BUILDER_{slot}");
        // SAFETY: tests hold ENV_TEST_LOCK while mutating process env.
        unsafe {
            std::env::set_var(format!("{prefix}_BASE_URL"), base_url);
            std::env::set_var(format!("{prefix}_API_KEY"), api_key);
            std::env::set_var(format!("{prefix}_MODEL"), model);
        }
    }

    fn clear_slot_env(slot: &str) {
        let prefix = format!("MDX_FLEET_BUILDER_{slot}");
        // SAFETY: tests hold ENV_TEST_LOCK while mutating process env.
        unsafe {
            std::env::remove_var(format!("{prefix}_BASE_URL"));
            std::env::remove_var(format!("{prefix}_API_KEY"));
            std::env::remove_var(format!("{prefix}_MODEL"));
        }
    }

    /// Activate a lesson and open a fleet_casting grant that prefers `slot`.
    /// Returns the grant receipt id.
    fn seed_casting_grant(kernel: &mut MdxKernel, slot: &str) -> String {
        let judgment = kernel
            .record_learning_judgment_decision(mdx_core::LearningJudgmentDecision {
                tenant_id: "t",
                actor_id: "human:eng",
                judgment_id: "judgment_1",
                promotion_id: "promotion_1",
                decision: "promote_candidate",
                rationale: "The evidence is enough to queue memory review.",
                evidence_refs: "make learning-loop-check",
            })
            .expect("judgment");
        let promotion = kernel
            .request_learning_memory_promotion(mdx_core::LearningMemoryPromotion {
                tenant_id: "t",
                actor_id: "human:eng",
                judgment_decision_id: &judgment.judgment_decision_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                judgment_id: "judgment_1",
                promotion_id: "promotion_1",
                target_type: "model_scorecard",
                target_path: "generated/learning/model-worker-scorecard-targets.json",
                lesson_summary: "Prefer the sovereign builder on this work.",
                evidence_refs: "make learning-loop-check",
                review_cadence: "review before activation",
                expiry_rule: "supersede when stale",
            })
            .expect("promotion");
        let activation = kernel
            .activate_learning_memory(mdx_core::LearningMemoryActivation {
                tenant_id: "t",
                actor_id: "human:eng",
                memory_promotion_id: "memory_promotion_1",
                memory_promotion_receipt_id: &promotion.receipt_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                target_type: "model_scorecard",
                target_path: "generated/learning/model-worker-scorecard-targets.json",
                lesson_summary: "Prefer the sovereign builder on this work.",
                evidence_refs: "make learning-loop-check",
                activation_basis: "human approval with local proof",
                rollback_plan: "supersede through a later memory receipt",
                local_checks: "make learning-loop-check",
                approval_refs: "approval:local",
                review_owner: "human:eng",
            })
            .expect("activation");
        kernel
            .grant_learning_adaptation(mdx_core::LearningAdaptationGrant {
                tenant_id: "t",
                actor_id: "human:eng",
                activation_receipt_id: &activation.receipt_id,
                adaptation_type: "fleet_casting",
                target_type: "model_scorecard",
                preferred_builder_slot: slot,
                reason: "Prefer the sovereign builder here.",
                review_owner: "human:eng",
            })
            .expect("grant")
            .receipt_id
    }

    #[test]
    fn ratified_grant_steers_casting_over_automatic_evidence() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        set_slot_env(
            "OPUS",
            "https://api.anthropic.test/v1",
            "test-key",
            "claude-opus-4-8",
        );
        clear_slot_env("XAI");
        clear_slot_env("GROK");
        let mut kernel = MdxKernel::boot_local();
        let grant_receipt_id = seed_casting_grant(&mut kernel, "OPUS");

        // No explicit slot requested: the ratified grant wins over automatic
        // evidence and names the grant it stands on.
        let evidence = builder_casting_evidence(&kernel, "rust-cargo", "feature", "medium", "");
        assert_eq!(evidence.status, "RATIFIED_ADAPTATION_SLOT_READY");
        assert_eq!(evidence.selected_builder_slot, "OPUS");
        assert_eq!(evidence.ratified_grant_receipt_id, grant_receipt_id);
        assert_eq!(evidence.basis, "ratified_fleet_casting_grant");
        clear_slot_env("OPUS");
    }

    #[test]
    fn explicit_human_slot_beats_a_ratified_grant() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        set_slot_env(
            "OPUS",
            "https://api.anthropic.test/v1",
            "test-key",
            "claude-opus-4-8",
        );
        set_slot_env("XAI", "https://api.x.ai/v1", "test-key", "grok-4.3");
        let mut kernel = MdxKernel::boot_local();
        seed_casting_grant(&mut kernel, "OPUS");

        // An explicit per-run slot is the human's direct call: it wins over the
        // ratified grant, and no grant is cited as having steered the decision.
        let evidence = builder_casting_evidence(&kernel, "rust-cargo", "feature", "medium", "XAI");
        assert_eq!(evidence.selected_builder_slot, "XAI");
        assert!(evidence.ratified_grant_receipt_id.is_empty());
        assert_ne!(evidence.status, "RATIFIED_ADAPTATION_SLOT_READY");
        clear_slot_env("OPUS");
        clear_slot_env("XAI");
    }

    #[test]
    fn withdrawn_grant_stops_steering_casting() {
        let _env = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        set_slot_env(
            "OPUS",
            "https://api.anthropic.test/v1",
            "test-key",
            "claude-opus-4-8",
        );
        clear_slot_env("XAI");
        let mut kernel = MdxKernel::boot_local();
        let grant_receipt_id = seed_casting_grant(&mut kernel, "OPUS");
        kernel
            .supersede_learning_adaptation(mdx_core::LearningAdaptationSupersede {
                tenant_id: "t",
                actor_id: "human:eng",
                grant_receipt_id: &grant_receipt_id,
                reason: "The lesson was wrong.",
                review_owner: "human:eng",
            })
            .expect("withdraw");
        let evidence = builder_casting_evidence(&kernel, "rust-cargo", "feature", "medium", "");
        assert!(evidence.ratified_grant_receipt_id.is_empty());
        assert_ne!(evidence.status, "RATIFIED_ADAPTATION_SLOT_READY");
        clear_slot_env("OPUS");
    }
}
