use crate::RouteResponse;
use crate::forge_turn_client::TurnClient;
use mdx_core::{MdxKernel, Receipt};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/flywheel-proof.json" {
        return None;
    }
    Some(handle_get(method, kernel))
}

fn handle_get(method: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let outcome_signals = kernel
        .ledger()
        .query()
        .by_kind("forge.outcome.signal.recorded");
    let candidate_lessons = outcome_signals
        .iter()
        .filter(|receipt| !payload_value(receipt, "lesson_candidate").trim().is_empty())
        .count();
    // Superseded lessons are retired: they no longer count as active memory,
    // though their receipts stay in the ledger and the source receipt set.
    let superseded_activations =
        crate::learning_routes::memory_supersede::superseded_activation_receipt_ids(&kernel);
    let active_memory_count = kernel
        .ledger()
        .query()
        .by_kind("learning.memory.activated")
        .iter()
        .filter(|receipt| {
            payload_value(receipt, "active_memory_state") == "active"
                && !superseded_activations.contains(&receipt.receipt_id)
        })
        .count();
    let installed_capability_count = installed_capability_count(&kernel);
    let citation_events = active_learning_citation_events(&kernel);
    let flywheel_context_events = flywheel_context_events(&kernel);
    let observed_two_lap_loop = !outcome_signals.is_empty()
        && candidate_lessons > 0
        && active_memory_count > 0
        && !citation_events.is_empty();
    let measured_improvement = measured_improvement_proof(&kernel);
    let adaptation_proof = adaptation_proof(&kernel, &measured_improvement);
    let ablation_proof = ablation_proof(&kernel);
    let source_receipt_ids = source_receipt_ids(&kernel);
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-marketplace-flywheel-proof","status":"LOCAL_REAL_FLYWHEEL_PROOF_READY","route":"/forge/flywheel-proof.json","source_routes":["/forge/runs/projection.json","/forge/outcome-signals/projection.json","/marketplace/installed-capabilities/projection.json","/learning/forge-outcome-candidates/projection.json","/learning/memory-activations/projection.json"],"source_receipt_ids":{},"outcome_signal_count":{},"candidate_lesson_count":{},"installed_capability_count":{},"active_memory_count":{},"compounding_proof":{{"status":{},"minimum_laps_required":2,"outcome_lap_count":{},"lesson_citation_event_count":{},"flywheel_context_event_count":{},"lesson_cited_by_later_lap":{},"evidence_receipt_ids":{},"claim":"local flywheel is wired and citation-observed when a later Forge lap cites an activated lesson from an earlier outcome","measured_improvement_required_next":{},"adaptation_allowed":false,"runtime_behavior_change_allowed":false}},"measured_improvement_proof":{},"adaptation_proof":{},"ablation_proof":{},"local_real":{{"loop":"forge_loop_runner","worktree_isolated":true,"live_repo_mutated":false,"model_configured":{},"host_checks_enabled":{},"sandbox_tests_enabled":{},"patch_apply_enabled":{},"deployment_allowed":false}},"learning_posture":{{"candidate_lessons_allowed":true,"active_memory_citation_allowed":true,"adaptation_allowed":false,"runtime_behavior_change_allowed":false}},"capability_posture":{{"capability_execution_allowed":false,"secret_access_allowed":false,"inherited_agent_permissions_allowed":false,"untrusted_capability_execution_requires_stronger_isolation":true}},"named_beta_gaps":["durable_queue_restart_loses_in_flight_work_until_aws_queue_handoff","community_imported_capability_execution_requires_gvisor_or_firecracker_first"],"claude_ui_handoff":{{"safe_to_render":true,"target_surface":"Forge, Marketplace, Twin","copy_boundary":"Show the flywheel as local-real and evidence-backed. Do not imply deployment authority, autonomous production writes, durable cloud queue recovery, or untrusted community capability execution."}},"production_write_allowed":false}}"#,
            json_string_array(&source_receipt_ids),
            outcome_signals.len(),
            candidate_lessons,
            installed_capability_count,
            active_memory_count,
            if observed_two_lap_loop {
                r#""OBSERVED_TWO_LAP_CITATION_LOOP""#
            } else {
                r#""READY_NOT_YET_OBSERVED""#
            },
            outcome_signals.len(),
            citation_events.len(),
            flywheel_context_events.len(),
            observed_two_lap_loop,
            json_string_array(&citation_events),
            measured_improvement.status != "OBSERVED_MEASURED_IMPROVEMENT",
            measured_improvement.to_json(),
            adaptation_proof,
            ablation_proof,
            forge_model_configured(&kernel),
            forge_host_checks_enabled(),
            env_enabled("MDX_PLAN_TEST_EXEC"),
            env_enabled("MDX_PLAN_PATCH_APPLY"),
        ),
    ))
}

// Shared with the outcome-signal distiller: prior laps on the same work item
// are the ReasoningBank delta a lesson can cite.
#[derive(Clone, Debug, Default)]
pub(crate) struct LapMetrics {
    pub(crate) run_id: String,
    pub(crate) work_item_id: String,
    pub(crate) finish_receipt_id: String,
    pub(crate) finish_index: usize,
    pub(crate) status: String,
    pub(crate) turns: u64,
    pub(crate) files_changed: u64,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) check_passed_count: u64,
    pub(crate) check_failed_count: u64,
    pub(crate) citation_receipt_ids: Vec<String>,
}

#[derive(Clone, Debug)]
struct ImprovementPair {
    prior: LapMetrics,
    later: LapMetrics,
    outcome_receipt_id: String,
    any_metric_improved: bool,
}

#[derive(Clone, Debug)]
struct ImprovementProof {
    status: &'static str,
    insufficient_reason: &'static str,
    pairs: Vec<ImprovementPair>,
    measured_lap_count: usize,
    cited_lap_count: usize,
}

impl ImprovementProof {
    fn to_json(&self) -> String {
        let pair_json = self
            .pairs
            .iter()
            .map(ImprovementPair::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            r#"{{"status":"{}","minimum_comparable_laps_required":2,"measured_lap_count":{},"cited_lap_count":{},"comparable_lap_pair_count":{},"insufficient_reason":{},"metrics_observed":{{"latency_ms":true,"tokens":true,"checks":true,"review_friction":true,"cost":false}},"cost_comparison_status":"NOT_INSTRUMENTED","comparison_basis":"same_work_item_and_later_lap_cited_active_lesson","claim":"measured improvement is reported only when comparable Forge laps carry real metrics and the later lap cites active learning memory","compared_pairs":[{}],"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            self.status,
            self.measured_lap_count,
            self.cited_lap_count,
            self.pairs.len(),
            json_string(self.insufficient_reason),
            pair_json
        )
    }
}

impl ImprovementPair {
    fn to_json(&self) -> String {
        let latency_delta = delta(self.prior.duration_ms, self.later.duration_ms);
        let input_delta = self.later.input_tokens as i64 - self.prior.input_tokens as i64;
        let output_delta = self.later.output_tokens as i64 - self.prior.output_tokens as i64;
        let failed_delta =
            self.later.check_failed_count as i64 - self.prior.check_failed_count as i64;
        let passed_delta =
            self.later.check_passed_count as i64 - self.prior.check_passed_count as i64;
        format!(
            r#"{{"prior_run_id":{},"later_run_id":{},"same_work_key":{},"outcome_receipt_id":{},"lesson_citation_receipt_ids":{},"later_lap_cited_lesson":true,"any_metric_improved":{},"prior":{},"later":{},"deltas":{{"duration_ms":{},"input_tokens":{},"output_tokens":{},"check_failed_count":{},"check_passed_count":{}}}}}"#,
            json_string(&self.prior.run_id),
            json_string(&self.later.run_id),
            json_string(&self.prior.work_item_id),
            json_string(&self.outcome_receipt_id),
            json_string_array(&self.later.citation_receipt_ids),
            self.any_metric_improved,
            self.prior.to_json(),
            self.later.to_json(),
            json_i64_option(latency_delta),
            input_delta,
            output_delta,
            failed_delta,
            passed_delta
        )
    }
}

impl LapMetrics {
    fn to_json(&self) -> String {
        format!(
            r#"{{"run_id":{},"work_item_id":{},"finish_receipt_id":{},"status":{},"turns":{},"files_changed":{},"duration_ms":{},"input_tokens":{},"output_tokens":{},"check_passed_count":{},"check_failed_count":{}}}"#,
            json_string(&self.run_id),
            json_string(&self.work_item_id),
            json_string(&self.finish_receipt_id),
            json_string(&self.status),
            self.turns,
            self.files_changed,
            json_u64_option(self.duration_ms),
            self.input_tokens,
            self.output_tokens,
            self.check_passed_count,
            self.check_failed_count
        )
    }
}

fn measured_improvement_proof(kernel: &MdxKernel) -> ImprovementProof {
    let mut laps = fold_laps(kernel);
    let outcome_runs = outcome_runs(kernel);
    for receipt in kernel.ledger().query().by_kind("forge.run.event") {
        if payload_value(receipt, "event") == "evidence_appended"
            && payload_value(receipt, "detail").contains("active learning memory cited")
        {
            let run_id = payload_value(receipt, "run_id");
            if let Some(lap) = laps.get_mut(run_id) {
                lap.citation_receipt_ids.push(receipt.receipt_id.clone());
            }
        }
    }
    let measured_lap_count = laps
        .values()
        .filter(|lap| lap.duration_ms.is_some())
        .count();
    let cited_lap_count = laps
        .values()
        .filter(|lap| !lap.citation_receipt_ids.is_empty())
        .count();
    let mut pairs = Vec::new();
    for prior in laps.values() {
        let Some(outcome_receipt_id) = outcome_runs.get(&prior.run_id) else {
            continue;
        };
        if prior.work_item_id.is_empty() || prior.duration_ms.is_none() {
            continue;
        }
        for later in laps.values() {
            if later.finish_index <= prior.finish_index
                || later.work_item_id != prior.work_item_id
                || later.duration_ms.is_none()
                || later.citation_receipt_ids.is_empty()
            {
                continue;
            }
            pairs.push(ImprovementPair {
                prior: prior.clone(),
                later: later.clone(),
                outcome_receipt_id: outcome_receipt_id.clone(),
                any_metric_improved: lap_any_metric_improved(prior, later),
            });
        }
    }
    let status = if pairs.iter().any(|pair| pair.any_metric_improved) {
        "OBSERVED_MEASURED_IMPROVEMENT"
    } else if pairs.is_empty() {
        "INSUFFICIENT_SAMPLES"
    } else {
        "OBSERVED_NO_IMPROVEMENT"
    };
    let insufficient_reason = if pairs.is_empty() {
        "need_two_finished_measured_laps_on_the_same_work_item_with_a_later_active_lesson_citation"
    } else {
        ""
    };
    ImprovementProof {
        status,
        insufficient_reason,
        pairs,
        measured_lap_count,
        cited_lap_count,
    }
}

pub(crate) fn fold_laps(kernel: &MdxKernel) -> BTreeMap<String, LapMetrics> {
    let mut laps = BTreeMap::<String, LapMetrics>::new();
    for (index, receipt) in kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "forge.run.event")
        .enumerate()
    {
        let run_id = payload_value(receipt, "run_id");
        if run_id.is_empty() {
            continue;
        }
        let event = payload_value(receipt, "event");
        let detail = payload_value(receipt, "detail");
        let lap = laps
            .entry(run_id.to_string())
            .or_insert_with(|| LapMetrics {
                run_id: run_id.to_string(),
                ..LapMetrics::default()
            });
        if lap.work_item_id.is_empty() {
            lap.work_item_id = payload_value(receipt, "work_item_id").to_string();
        }
        lap.input_tokens += payload_value(receipt, "tokens_in")
            .parse::<u64>()
            .unwrap_or(0);
        lap.output_tokens += payload_value(receipt, "tokens_out")
            .parse::<u64>()
            .unwrap_or(0);
        match event {
            "check_passed" => lap.check_passed_count += 1,
            "check_failed" => lap.check_failed_count += 1,
            "run_finished" => {
                lap.finish_receipt_id = receipt.receipt_id.clone();
                lap.finish_index = index;
                lap.status =
                    field_in(detail, "status=").unwrap_or_else(|| "RUN_ERRORED".to_string());
                lap.turns = field_in(detail, "turns=")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                lap.files_changed = field_in(detail, "files_changed=")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                lap.duration_ms = payload_value(receipt, "duration_ms").parse::<u64>().ok();
            }
            _ => {}
        }
    }
    laps.retain(|_, lap| !lap.finish_receipt_id.is_empty());
    laps
}

/// The first governed adaptation gate, observed honestly. A ratified lesson
/// may open ONE fleet_casting grant; the consuming seams record
/// learning.adaptation.applied when it steers a plan or a builder slot; a later
/// same-work-item lap that improves is the compounding proof. Every stage is
/// reported with the same honest-null grammar the rest of this proof uses, so
/// no stage claims more than the receipts show.
fn adaptation_proof(kernel: &MdxKernel, improvement: &ImprovementProof) -> String {
    let open_grants = kernel.active_fleet_casting_grants();
    let granted_count = kernel
        .ledger()
        .query()
        .by_kind("learning.adaptation.granted")
        .len();
    #[derive(Default)]
    struct Applied {
        artifact_kind: String,
        artifact_id: String,
        grant_receipt_id: String,
        receipt_id: String,
    }
    let applied: Vec<Applied> = kernel
        .ledger()
        .query()
        .by_kind("learning.adaptation.applied")
        .iter()
        .map(|receipt| Applied {
            artifact_kind: payload_value(receipt, "artifact_kind").to_string(),
            artifact_id: payload_value(receipt, "artifact_id").to_string(),
            grant_receipt_id: payload_value(receipt, "grant_receipt_id").to_string(),
            receipt_id: receipt.receipt_id.clone(),
        })
        .collect();
    // The runs a later-lap improvement was measured on. A builder_slot grant
    // that steered one of those runs is adaptation observed end to end.
    let improved_runs: BTreeSet<String> = improvement
        .pairs
        .iter()
        .filter(|pair| pair.any_metric_improved)
        .map(|pair| pair.later.run_id.clone())
        .collect();
    let observed_delta = applied.iter().any(|entry| {
        entry.artifact_kind == "builder_slot" && improved_runs.contains(&entry.artifact_id)
    });
    let status = if open_grants.is_empty() {
        "READY_NOT_YET_GRANTED"
    } else if applied.is_empty() {
        "GRANTED_NOT_YET_APPLIED"
    } else if observed_delta {
        "OBSERVED_ADAPTATION_IMPROVEMENT"
    } else {
        "INSUFFICIENT_SAMPLES"
    };
    let insufficient_reason = match status {
        "READY_NOT_YET_GRANTED" => "no lesson has been granted fleet casting authority yet",
        "GRANTED_NOT_YET_APPLIED" => {
            "a grant is open but has not steered a plan or a builder slot yet"
        }
        "INSUFFICIENT_SAMPLES" => {
            "a grant steered casting but no later same-work-item lap has improved yet"
        }
        _ => "",
    };
    let applied_receipt_ids: Vec<String> = applied
        .iter()
        .map(|entry| entry.receipt_id.clone())
        .collect();
    let grant_receipt_ids: Vec<String> = applied
        .iter()
        .map(|entry| entry.grant_receipt_id.clone())
        .filter(|id| !id.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    format!(
        r#"{{"status":"{status}","adaptation_type":"fleet_casting","open_grant_count":{},"granted_receipt_count":{},"applied_receipt_count":{},"applied_grant_receipt_ids":{},"applied_receipt_ids":{},"insufficient_reason":{},"observed_chain":"lesson -> grant -> applied -> later-lap delta","claim":"a granted lesson is reported as compounding only when a learning.adaptation.applied receipt steered a builder slot and a later same-work-item lap improved","model_routing_change_allowed":false,"budget_change_allowed":false,"check_policy_change_allowed":false,"review_depth_change_allowed":false,"reversible":true,"production_write_allowed":false}}"#,
        open_grants.len(),
        granted_count,
        applied.len(),
        json_string_array(&grant_receipt_ids),
        json_string_array(&applied_receipt_ids),
        json_string(insufficient_reason),
    )
}

/// A run must carry at least this many finished laps in an arm before that arm
/// is treated as measured. Below the floor the ablation reports
/// INSUFFICIENT_SAMPLES and claims no delta.
const MINIMUM_ABLATION_ARM_SAMPLES: usize = 3;

#[derive(Default)]
struct AblationArm {
    sample_count: usize,
    done_count: usize,
    sum_turns: u64,
    sum_input_tokens: u64,
    sum_output_tokens: u64,
    sum_check_failed: u64,
    sum_check_passed: u64,
    sum_duration_ms: u64,
    duration_sample_count: usize,
    run_ids: Vec<String>,
}

impl AblationArm {
    fn observe(&mut self, lap: &LapMetrics) {
        self.sample_count += 1;
        if lap.status == "RUN_FINISHED_DONE" {
            self.done_count += 1;
        }
        self.sum_turns += lap.turns;
        self.sum_input_tokens += lap.input_tokens;
        self.sum_output_tokens += lap.output_tokens;
        self.sum_check_failed += lap.check_failed_count;
        self.sum_check_passed += lap.check_passed_count;
        if let Some(duration) = lap.duration_ms {
            self.sum_duration_ms += duration;
            self.duration_sample_count += 1;
        }
        self.run_ids.push(lap.run_id.clone());
    }

    fn mean(sum: u64, count: usize) -> Option<f64> {
        (count > 0).then(|| sum as f64 / count as f64)
    }

    fn mean_turns(&self) -> Option<f64> {
        Self::mean(self.sum_turns, self.sample_count)
    }

    fn mean_input_tokens(&self) -> Option<f64> {
        Self::mean(self.sum_input_tokens, self.sample_count)
    }

    fn mean_output_tokens(&self) -> Option<f64> {
        Self::mean(self.sum_output_tokens, self.sample_count)
    }

    fn mean_check_failed(&self) -> Option<f64> {
        Self::mean(self.sum_check_failed, self.sample_count)
    }

    fn mean_check_passed(&self) -> Option<f64> {
        Self::mean(self.sum_check_passed, self.sample_count)
    }

    fn mean_duration_ms(&self) -> Option<f64> {
        Self::mean(self.sum_duration_ms, self.duration_sample_count)
    }

    fn to_json(&self) -> String {
        format!(
            r#"{{"sample_count":{},"done_count":{},"mean_turns":{},"mean_input_tokens":{},"mean_output_tokens":{},"mean_check_failed_count":{},"mean_check_passed_count":{},"mean_duration_ms":{},"run_ids":{}}}"#,
            self.sample_count,
            self.done_count,
            json_f64_option(self.mean_turns()),
            json_f64_option(self.mean_input_tokens()),
            json_f64_option(self.mean_output_tokens()),
            json_f64_option(self.mean_check_failed()),
            json_f64_option(self.mean_check_passed()),
            json_f64_option(self.mean_duration_ms()),
            json_string_array(&self.run_ids),
        )
    }
}

/// Memory-on/off ablation, reported with the same honest-null grammar as the
/// rest of this proof. The harness (scripts/memory-ablation-harness.sh) drives
/// matched fixture task families through Forge twice: once with the two
/// flywheel injection blocks (memory-on) and once with
/// MDX_FORGE_FLYWHEEL_CONTEXT=off (the memory-off baseline, which emits a
/// receipted suppression event). Each run is classified into an arm by its
/// receipts. Under the deterministic local gateway the on and off arms feed
/// different prompt bytes to a deterministic function, so the delta is
/// definitionally zero: without a live model the experiment reports
/// NOT_MEASURABLE. With a model but fewer than the per-arm sample floor it is
/// INSUFFICIENT_SAMPLES. No arm claims significance below the floor, and cost
/// stays NOT_INSTRUMENTED.
fn ablation_proof(kernel: &MdxKernel) -> String {
    ablation_proof_with(kernel, forge_model_configured(kernel))
}

fn ablation_proof_with(kernel: &MdxKernel, model_configured: bool) -> String {
    let laps = fold_laps(kernel);

    let mut suppressed_runs = BTreeSet::<String>::new();
    let mut context_runs = BTreeSet::<String>::new();
    let mut suppression_receipt_ids = Vec::new();
    for receipt in kernel.ledger().query().by_kind("forge.run.event") {
        if payload_value(receipt, "event") != "evidence_appended" {
            continue;
        }
        let detail = payload_value(receipt, "detail");
        let run_id = payload_value(receipt, "run_id");
        if detail.contains("flywheel context suppressed for ablation") {
            suppressed_runs.insert(run_id.to_string());
            suppression_receipt_ids.push(receipt.receipt_id.clone());
        } else if detail.contains("flywheel context assembled") {
            context_runs.insert(run_id.to_string());
        }
    }

    // A run is the memory-off arm when it carries the suppression receipt, the
    // memory-on arm when flywheel context actually rode into it. A run where
    // context was permitted but nothing was available to inject sits in neither
    // arm, so an empty memory store never inflates the on arm.
    let mut on_arm = AblationArm::default();
    let mut off_arm = AblationArm::default();
    for lap in laps.values() {
        if suppressed_runs.contains(&lap.run_id) {
            off_arm.observe(lap);
        } else if context_runs.contains(&lap.run_id) {
            on_arm.observe(lap);
        }
    }

    let status = if !model_configured {
        "NOT_MEASURABLE"
    } else if on_arm.sample_count < MINIMUM_ABLATION_ARM_SAMPLES
        || off_arm.sample_count < MINIMUM_ABLATION_ARM_SAMPLES
    {
        "INSUFFICIENT_SAMPLES"
    } else {
        "MEASURED_ABLATION_DELTA"
    };
    let insufficient_reason = match status {
        "NOT_MEASURABLE" => {
            "no live builder model is configured; under the deterministic local gateway the on and off arms feed a deterministic function so the delta is definitionally zero"
        }
        "INSUFFICIENT_SAMPLES" => {
            "each arm needs at least the per-arm sample floor of finished laps before any delta is reported"
        }
        _ => "",
    };
    // Delta is on minus off, reported for descriptive review only and never as
    // a significance test, and only when both arms cleared the floor.
    let measured = status == "MEASURED_ABLATION_DELTA";
    let delta = |on: Option<f64>, off: Option<f64>| -> Option<f64> {
        match (measured, on, off) {
            (true, Some(on), Some(off)) => Some(on - off),
            _ => None,
        }
    };
    let delta_json = format!(
        r#"{{"mean_turns":{},"mean_input_tokens":{},"mean_output_tokens":{},"mean_check_failed_count":{},"mean_check_passed_count":{},"mean_duration_ms":{}}}"#,
        json_f64_option(delta(on_arm.mean_turns(), off_arm.mean_turns())),
        json_f64_option(delta(
            on_arm.mean_input_tokens(),
            off_arm.mean_input_tokens()
        )),
        json_f64_option(delta(
            on_arm.mean_output_tokens(),
            off_arm.mean_output_tokens()
        )),
        json_f64_option(delta(
            on_arm.mean_check_failed(),
            off_arm.mean_check_failed()
        )),
        json_f64_option(delta(
            on_arm.mean_check_passed(),
            off_arm.mean_check_passed()
        )),
        json_f64_option(delta(on_arm.mean_duration_ms(), off_arm.mean_duration_ms())),
    );
    format!(
        r#"{{"status":"{status}","env_gate":"MDX_FORGE_FLYWHEEL_CONTEXT","model_configured":{},"minimum_arm_samples":{},"pre_registered_primary_metrics":["check_passed_count","check_failed_count","turns_used","input_tokens","output_tokens","duration_ms"],"cost_comparison_status":"NOT_INSTRUMENTED","significance_tested":false,"context_on_arm":{},"context_off_arm":{},"delta_on_minus_off":{},"suppression_event_receipt_ids":{},"insufficient_reason":{},"comparison_basis":"same_fixture_task_families_run_twice_context_on_and_context_off_under_sim_isolation","claim":"memory-on is reported as beating the memory-off baseline only when a live model is configured and both arms clear the per-arm sample floor; below the floor or without a model no delta is claimed","adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
        model_configured,
        MINIMUM_ABLATION_ARM_SAMPLES,
        on_arm.to_json(),
        off_arm.to_json(),
        delta_json,
        json_string_array(&suppression_receipt_ids),
        json_string(insufficient_reason),
    )
}

fn outcome_runs(kernel: &MdxKernel) -> BTreeMap<String, String> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.outcome.signal.recorded")
        .iter()
        .filter_map(|receipt| {
            let run_id = payload_value(receipt, "run_id");
            (!run_id.is_empty()).then(|| (run_id.to_string(), receipt.receipt_id.clone()))
        })
        .collect()
}

fn lap_any_metric_improved(prior: &LapMetrics, later: &LapMetrics) -> bool {
    (later.status == "RUN_FINISHED_DONE" && prior.status != "RUN_FINISHED_DONE")
        || later.duration_ms < prior.duration_ms
        || (prior.input_tokens > 0 && later.input_tokens < prior.input_tokens)
        || (prior.output_tokens > 0 && later.output_tokens < prior.output_tokens)
        || later.check_failed_count < prior.check_failed_count
        || later.check_passed_count > prior.check_passed_count
}

fn field_in(detail: &str, key: &str) -> Option<String> {
    let start = detail.find(key)? + key.len();
    let rest = &detail[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let value = rest[..end].trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn delta(prior: Option<u64>, later: Option<u64>) -> Option<i64> {
    Some(later? as i64 - prior? as i64)
}

fn installed_capability_count(kernel: &MdxKernel) -> usize {
    let mut installed = BTreeSet::<String>::new();
    for receipt in kernel.ledger().query().by_kind("marketplace.act.recorded") {
        let capability_id = payload_value(receipt, "capability_id");
        let scope = payload_value(receipt, "scope");
        if capability_id.is_empty() || scope.is_empty() {
            continue;
        }
        let key = format!("{capability_id}@{scope}");
        match payload_value(receipt, "act") {
            "install" | "pack_install" | "approval" => {
                installed.insert(key);
            }
            "revocation" => {
                installed.remove(&key);
            }
            _ => {}
        }
    }
    installed.len()
}

fn source_receipt_ids(kernel: &MdxKernel) -> Vec<String> {
    let mut ids = BTreeSet::<String>::new();
    for kind in [
        "forge.outcome.signal.recorded",
        "learning.memory.activated",
        "marketplace.act.recorded",
    ] {
        for receipt in kernel.ledger().query().by_kind(kind) {
            ids.insert(receipt.receipt_id.clone());
        }
    }
    ids.into_iter().collect()
}

fn active_learning_citation_events(kernel: &MdxKernel) -> Vec<String> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| {
            payload_value(receipt, "event") == "evidence_appended"
                && payload_value(receipt, "detail").contains("active learning memory cited")
        })
        .map(|receipt| receipt.receipt_id.clone())
        .collect()
}

fn flywheel_context_events(kernel: &MdxKernel) -> Vec<String> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| {
            payload_value(receipt, "event") == "evidence_appended"
                && payload_value(receipt, "detail").contains("flywheel context assembled")
        })
        .map(|receipt| receipt.receipt_id.clone())
        .collect()
}

fn json_string_array(values: &[String]) -> String {
    let items = values
        .iter()
        .map(|value| format!(r#""{}""#, escape_json(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{items}]")
}

fn json_string(value: &str) -> String {
    format!(r#""{}""#, escape_json(value))
}

fn json_u64_option(value: Option<u64>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn json_i64_option(value: Option<i64>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn json_f64_option(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:.2}"))
        .unwrap_or_else(|| "null".to_string())
}

fn escape_json(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
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

fn payload_value<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{
        ForgeOutcomeSignal, ForgeRunEvent, LearningJudgmentDecision, LearningMemoryActivation,
        LearningMemoryPromotion, MarketplaceAct,
    };

    /// Records the judgment decision and memory promotion receipts the
    /// activation write now verifies in the ledger. Returns
    /// (judgment_decision_receipt_id, memory_promotion_receipt_id).
    fn promotion_chain_receipt_ids(
        kernel: &mut MdxKernel,
        promotion_id: &str,
        lesson_summary: &str,
    ) -> (String, String) {
        let judgment = kernel
            .record_learning_judgment_decision(LearningJudgmentDecision {
                tenant_id: "t",
                actor_id: "human:eng",
                judgment_id: &format!("judgment_{promotion_id}"),
                promotion_id,
                decision: "promote_candidate",
                rationale: "The evidence is enough to queue memory review.",
                evidence_refs: "make learning-loop-check",
            })
            .expect("judgment decision");
        let promotion = kernel
            .request_learning_memory_promotion(LearningMemoryPromotion {
                tenant_id: "t",
                actor_id: "human:eng",
                judgment_decision_id: &judgment.judgment_decision_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                judgment_id: &format!("judgment_{promotion_id}"),
                promotion_id,
                target_type: "decision_record",
                target_path: "generated/learning/forge-outcome-memory-targets.json",
                lesson_summary,
                evidence_refs: "make learning-loop-check",
                review_cadence: "review before activation",
                expiry_rule: "supersede when stale",
            })
            .expect("memory promotion");
        (judgment.receipt_id, promotion.receipt_id)
    }

    #[test]
    fn proof_route_names_counts_boundaries_and_beta_gaps() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut k = kernel.write().expect("kernel");
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
                run_id: "forge_run_1",
                source_receipt_id: &source_receipt_id,
                source_receipt_kind: "forge.run.event",
                disposition: "completed",
                summary: "Run completed.",
                capability_ids: "rust_backend_skill",
                model_or_worker: "local_forge_worker",
                lesson_candidate: "Use the local proof route before beta handoff.",
                lesson_source: "",
                message_channel_id: "forge",
            })
            .expect("outcome");
            let (judgment_receipt_id, promotion_receipt_id) = promotion_chain_receipt_ids(
                &mut k,
                "promotion_1",
                "Use the local proof route before beta handoff.",
            );
            k.activate_learning_memory(LearningMemoryActivation {
                tenant_id: "t",
                actor_id: "human:eng",
                memory_promotion_id: "promotion_1",
                memory_promotion_receipt_id: &promotion_receipt_id,
                judgment_decision_receipt_id: &judgment_receipt_id,
                target_type: "decision_record",
                target_path: "generated/learning/forge-outcome-memory-targets.json",
                lesson_summary: "Use the local proof route before beta handoff.",
                evidence_refs: "receipt:forge_run_1",
                activation_basis: "approved with local proof",
                rollback_plan: "supersede with later receipt",
                local_checks: "make local-smoke",
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
                capability_id: "rust_backend_skill",
                scope: "repo",
                decision: "install",
                read_only: true,
                ..MarketplaceAct::default()
            })
            .expect("install");
        }
        let response = route_response("GET", "/forge/flywheel-proof.json", &kernel)
            .expect("route")
            .expect("response");
        assert_eq!(response.status, "200 OK");
        assert!(response.body.contains("\"outcome_signal_count\":1"));
        assert!(response.body.contains("\"candidate_lesson_count\":1"));
        assert!(response.body.contains("\"installed_capability_count\":1"));
        assert!(response.body.contains("\"active_memory_count\":1"));
        assert!(response.body.contains("\"source_receipt_ids\":["));
        assert!(
            response
                .body
                .contains("\"status\":\"READY_NOT_YET_OBSERVED\"")
        );
        assert!(
            response
                .body
                .contains("\"lesson_cited_by_later_lap\":false")
        );
        assert!(
            response
                .body
                .contains("\"measured_improvement_required_next\":true")
        );
        assert!(
            response
                .body
                .contains("\"measured_improvement_proof\":{\"status\":\"INSUFFICIENT_SAMPLES\"")
        );
        assert!(response.body.contains("receipt_"));
        assert!(response.body.contains("\"worktree_isolated\":true"));
        assert!(response.body.contains("\"adaptation_allowed\":false"));
        assert!(
            response
                .body
                .contains("durable_queue_restart_loses_in_flight_work")
        );
        assert!(response.body.contains(
            "community_imported_capability_execution_requires_gvisor_or_firecracker_first"
        ));
    }

    #[test]
    fn proof_route_reports_observed_two_lap_citation_loop() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut k = kernel.write().expect("kernel");
            let lap_one_finished = k
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "agent:forge",
                    run_id: "forge_lap_1",
                    event: "run_finished",
                    work_item_id: "beta_flywheel_lap_1",
                    detail: "status=RUN_FINISHED_DONE turns=4 files_changed=1",
                    turn: 4,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("lap one finished");
            let outcome = k
                .record_forge_outcome_signal(ForgeOutcomeSignal {
                    tenant_id: "t",
                    actor_id: "agent:forge",
                    run_id: "forge_lap_1",
                    source_receipt_id: &lap_one_finished.receipt_id,
                    source_receipt_kind: "forge.run.event",
                    disposition: "completed",
                    summary: "Lap one found that UI work needs screenshot proof before handoff.",
                    capability_ids: "svelte_ui_skill",
                    model_or_worker: "local_forge_worker",
                    lesson_candidate: "Run screenshot proof before handing off visual route work.",
                    lesson_source: "",
                    message_channel_id: "forge",
                })
                .expect("outcome");
            let (judgment_receipt_id, promotion_receipt_id) = promotion_chain_receipt_ids(
                &mut k,
                "promotion_beta_lap_1",
                "Run screenshot proof before handing off visual route work.",
            );
            let active_memory = k
                .activate_learning_memory(LearningMemoryActivation {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    memory_promotion_id: "promotion_beta_lap_1",
                    memory_promotion_receipt_id: &promotion_receipt_id,
                    judgment_decision_receipt_id: &judgment_receipt_id,
                    target_type: "decision_record",
                    target_path: "generated/learning/forge-outcome-memory-targets.json",
                    lesson_summary: "Run screenshot proof before handing off visual route work.",
                    evidence_refs: &outcome.receipt_id,
                    activation_basis: "approved with local lap proof",
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
            .expect("install");
            k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_lap_2",
                event: "evidence_appended",
                work_item_id: "beta_flywheel_lap_2",
                detail: "flywheel context assembled: outcomes=1 active_memories=1 installed_capabilities=1",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("flywheel context event");
            k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_lap_2",
                event: "evidence_appended",
                work_item_id: "beta_flywheel_lap_2",
                detail: "active learning memory cited advisory_count=1",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("citation event");
            k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_lap_2",
                event: "check_passed",
                work_item_id: "beta_flywheel_lap_2",
                detail: &format!(
                    "lap two followed active lesson from {} and ran screenshot proof",
                    active_memory.receipt_id
                ),
                turn: 3,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("lap two check");
        }

        let response = route_response("GET", "/forge/flywheel-proof.json", &kernel)
            .expect("route")
            .expect("response");
        assert_eq!(response.status, "200 OK");
        assert!(
            response
                .body
                .contains("\"status\":\"OBSERVED_TWO_LAP_CITATION_LOOP\"")
        );
        assert!(response.body.contains("\"outcome_lap_count\":1"));
        assert!(response.body.contains("\"lesson_citation_event_count\":1"));
        assert!(response.body.contains("\"flywheel_context_event_count\":1"));
        assert!(response.body.contains("\"lesson_cited_by_later_lap\":true"));
        assert!(response.body.contains("\"adaptation_allowed\":false"));
        assert!(
            response
                .body
                .contains("\"runtime_behavior_change_allowed\":false")
        );
        assert!(
            response
                .body
                .contains("\"measured_improvement_proof\":{\"status\":\"INSUFFICIENT_SAMPLES\"")
        );
    }

    #[test]
    fn proof_route_reports_measured_improvement_only_from_comparable_cited_laps() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut k = kernel.write().expect("kernel");
            let lap_one_finished = k
                .record_forge_run_event_with_duration(
                    ForgeRunEvent {
                        tenant_id: "t",
                        actor_id: "agent:forge",
                        run_id: "forge_same_work_lap_1",
                        event: "run_finished",
                        work_item_id: "same_work",
                        detail: "status=RUN_BUDGET_EXHAUSTED turns=8 files_changed=1",
                        turn: 8,
                        input_tokens: 900,
                        output_tokens: 400,
                    },
                    1200,
                )
                .expect("lap one finished");
            let outcome = k
                .record_forge_outcome_signal(ForgeOutcomeSignal {
                    tenant_id: "t",
                    actor_id: "agent:forge",
                    run_id: "forge_same_work_lap_1",
                    source_receipt_id: &lap_one_finished.receipt_id,
                    source_receipt_kind: "forge.run.event",
                    disposition: "budget_exhausted",
                    summary: "Lap one spent too many turns before checks.",
                    capability_ids: "rust_backend_skill",
                    model_or_worker: "local_forge_worker",
                    lesson_candidate: "Run the focused check earlier on similar work.",
                    lesson_source: "",
                    message_channel_id: "forge",
                })
                .expect("outcome");
            let (judgment_receipt_id, promotion_receipt_id) = promotion_chain_receipt_ids(
                &mut k,
                "promotion_same_work",
                "Run the focused check earlier on similar work.",
            );
            k.activate_learning_memory(LearningMemoryActivation {
                tenant_id: "t",
                actor_id: "human:eng",
                memory_promotion_id: "promotion_same_work",
                memory_promotion_receipt_id: &promotion_receipt_id,
                judgment_decision_receipt_id: &judgment_receipt_id,
                target_type: "decision_record",
                target_path: "generated/learning/forge-outcome-memory-targets.json",
                lesson_summary: "Run the focused check earlier on similar work.",
                evidence_refs: &outcome.receipt_id,
                activation_basis: "approved measured proof seed",
                rollback_plan: "supersede through a later memory receipt",
                local_checks: "cargo test -p mdx-server forge_flywheel_proof_route",
                approval_refs: "approval:local",
                review_owner: "human:eng",
            })
            .expect("active memory");
            k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_same_work_lap_2",
                event: "evidence_appended",
                work_item_id: "same_work",
                detail: "active learning memory cited advisory_count=1",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("citation");
            k.record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_same_work_lap_2",
                event: "check_passed",
                work_item_id: "same_work",
                detail: "run_command cargo test -p mdx-server forge_flywheel_proof_route exit=0",
                turn: 3,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("check passed");
            k.record_forge_run_event_with_duration(
                ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "agent:forge",
                    run_id: "forge_same_work_lap_2",
                    event: "run_finished",
                    work_item_id: "same_work",
                    detail: "status=RUN_FINISHED_DONE turns=3 files_changed=1",
                    turn: 3,
                    input_tokens: 500,
                    output_tokens: 250,
                },
                700,
            )
            .expect("lap two finished");
        }

        let response = route_response("GET", "/forge/flywheel-proof.json", &kernel)
            .expect("route")
            .expect("response");
        assert!(response.body.contains(
            "\"measured_improvement_proof\":{\"status\":\"OBSERVED_MEASURED_IMPROVEMENT\""
        ));
        assert!(response.body.contains("\"comparable_lap_pair_count\":1"));
        assert!(
            response
                .body
                .contains("\"prior_run_id\":\"forge_same_work_lap_1\"")
        );
        assert!(
            response
                .body
                .contains("\"later_run_id\":\"forge_same_work_lap_2\"")
        );
        assert!(response.body.contains("\"duration_ms\":-500"));
        assert!(response.body.contains("\"input_tokens\":-400"));
        assert!(response.body.contains("\"check_passed_count\":1"));
        assert!(
            response
                .body
                .contains("\"cost_comparison_status\":\"NOT_INSTRUMENTED\"")
        );
        assert!(response.body.contains("\"adaptation_allowed\":false"));
    }

    #[test]
    fn proof_route_counts_relevance_v1_citation_events() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        kernel
            .write()
            .expect("kernel")
            .record_forge_run_event(ForgeRunEvent {
                tenant_id: "t",
                actor_id: "agent:forge",
                run_id: "forge_relevance_lap",
                event: "evidence_appended",
                work_item_id: "relevance_lap",
                detail: "active learning memory cited advisory_count=1 selection_basis=relevance_v1 matched_memory_ids=learning_active_memory_1 passed_over_count=1 passed_over_memory_ids=learning_active_memory_2",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            })
            .expect("relevance citation event");
        let response = route_response("GET", "/forge/flywheel-proof.json", &kernel)
            .expect("route")
            .expect("response");
        assert!(response.body.contains("\"lesson_citation_event_count\":1"));
    }

    #[test]
    fn proof_route_reports_adaptation_gate_with_honest_null_grammar() {
        use mdx_core::LearningAdaptationGrant;
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        // No grant yet: the adaptation gate is honest-null, ready but not granted.
        let before = route_response("GET", "/forge/flywheel-proof.json", &kernel)
            .expect("route")
            .expect("response");
        assert!(
            before
                .body
                .contains("\"adaptation_proof\":{\"status\":\"READY_NOT_YET_GRANTED\"")
        );
        assert!(before.body.contains("\"open_grant_count\":0"));

        // Open a grant over an activated lesson: it is granted but not applied.
        let activation_receipt_id = {
            let mut k = kernel.write().expect("kernel");
            let (judgment_receipt_id, promotion_receipt_id) = promotion_chain_receipt_ids(
                &mut k,
                "promotion_adapt",
                "Prefer the sovereign builder.",
            );
            let activation = k
                .activate_learning_memory(LearningMemoryActivation {
                    tenant_id: "t",
                    actor_id: "human:eng",
                    memory_promotion_id: "promotion_adapt",
                    memory_promotion_receipt_id: &promotion_receipt_id,
                    judgment_decision_receipt_id: &judgment_receipt_id,
                    target_type: "model_scorecard",
                    target_path: "generated/learning/model-worker-scorecard-targets.json",
                    lesson_summary: "Prefer the sovereign builder.",
                    evidence_refs: "make learning-loop-check",
                    activation_basis: "human approval with local proof",
                    rollback_plan: "supersede through a later memory receipt",
                    local_checks: "make learning-loop-check",
                    approval_refs: "approval:local",
                    review_owner: "human:eng",
                })
                .expect("activation");
            k.grant_learning_adaptation(LearningAdaptationGrant {
                tenant_id: "t",
                actor_id: "human:eng",
                activation_receipt_id: &activation.receipt_id,
                adaptation_type: "fleet_casting",
                target_type: "model_scorecard",
                preferred_builder_slot: "OPUS",
                reason: "Prefer the sovereign builder here.",
                review_owner: "human:eng",
            })
            .expect("grant");
            activation.receipt_id
        };
        let _ = activation_receipt_id;
        let granted = route_response("GET", "/forge/flywheel-proof.json", &kernel)
            .expect("route")
            .expect("response");
        assert!(
            granted
                .body
                .contains("\"adaptation_proof\":{\"status\":\"GRANTED_NOT_YET_APPLIED\"")
        );
        assert!(granted.body.contains("\"open_grant_count\":1"));
        assert!(
            granted
                .body
                .contains("\"model_routing_change_allowed\":false")
        );
    }

    #[test]
    fn proof_route_drops_superseded_lessons_from_active_memory() {
        use mdx_core::LearningMemorySupersede;
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let activation_receipt_id = {
            let mut k = kernel.write().expect("kernel");
            let (judgment_receipt_id, promotion_receipt_id) = promotion_chain_receipt_ids(
                &mut k,
                "promotion_retire",
                "A lesson that later turns out to be wrong.",
            );
            k.activate_learning_memory(LearningMemoryActivation {
                tenant_id: "t",
                actor_id: "human:eng",
                memory_promotion_id: "promotion_retire",
                memory_promotion_receipt_id: &promotion_receipt_id,
                judgment_decision_receipt_id: &judgment_receipt_id,
                target_type: "decision_record",
                target_path: "generated/learning/forge-outcome-memory-targets.json",
                lesson_summary: "A lesson that later turns out to be wrong.",
                evidence_refs: "make learning-loop-check",
                activation_basis: "approved with local proof",
                rollback_plan: "supersede through a later memory receipt",
                local_checks: "make learning-loop-check",
                approval_refs: "approval:local",
                review_owner: "human:eng",
            })
            .expect("active memory")
            .receipt_id
        };
        let before = route_response("GET", "/forge/flywheel-proof.json", &kernel)
            .expect("route")
            .expect("response");
        assert!(before.body.contains("\"active_memory_count\":1"));

        kernel
            .write()
            .expect("kernel")
            .supersede_learning_memory(LearningMemorySupersede {
                tenant_id: "t",
                actor_id: "human:eng",
                activation_receipt_id: &activation_receipt_id,
                reason: "The lesson misdiagnosed the intent and steered later runs wrong.",
                review_owner: "human:eng",
            })
            .expect("memory supersede");
        let after = route_response("GET", "/forge/flywheel-proof.json", &kernel)
            .expect("route")
            .expect("response");
        assert!(after.body.contains("\"active_memory_count\":0"));
    }

    #[test]
    fn ablation_proof_reports_not_measurable_without_a_model() {
        // Under the deterministic local gateway the on and off arms feed a
        // deterministic function, so the honest verdict is NOT_MEASURABLE, not
        // a null and not a fabricated number.
        let kernel = MdxKernel::boot_local();
        let json = ablation_proof_with(&kernel, false);
        assert!(json.contains(r#""status":"NOT_MEASURABLE""#), "{json}");
        assert!(json.contains(r#""model_configured":false"#));
        assert!(json.contains(r#""cost_comparison_status":"NOT_INSTRUMENTED""#));
        assert!(json.contains(r#""significance_tested":false"#));
        assert!(json.contains(r#""env_gate":"MDX_FORGE_FLYWHEEL_CONTEXT""#));
        // The pre-registered metrics and the per-arm floor are always disclosed.
        assert!(json.contains(r#""minimum_arm_samples":3"#));
        assert!(json.contains(r#""pre_registered_primary_metrics":["#));
        assert!(json.contains(r#""context_on_arm""#));
        assert!(json.contains(r#""context_off_arm""#));
        assert!(json.contains(r#""delta_on_minus_off""#));
    }

    #[test]
    fn ablation_proof_reports_insufficient_samples_below_the_floor() {
        // A live model is configured but each arm has one finished lap, below
        // the per-arm floor: report INSUFFICIENT_SAMPLES and claim no delta.
        let mut kernel = MdxKernel::boot_local();
        for (run_id, detail) in [
            (
                "ablation_on_run",
                "flywheel context assembled: outcomes=1 active_memories=1 installed_capabilities=0",
            ),
            (
                "ablation_off_run",
                "flywheel context suppressed for ablation (MDX_FORGE_FLYWHEEL_CONTEXT=off): outcome-signal and active-learning-memory blocks withheld from this run",
            ),
        ] {
            kernel
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "agent:forge",
                    run_id,
                    event: "evidence_appended",
                    work_item_id: "ablation_fixture",
                    detail,
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("arm marker event");
            kernel
                .record_forge_run_event(ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "agent:forge",
                    run_id,
                    event: "run_finished",
                    work_item_id: "ablation_fixture",
                    detail: "status=RUN_FINISHED_DONE turns=3 files_changed=1",
                    turn: 3,
                    input_tokens: 0,
                    output_tokens: 0,
                })
                .expect("finish event");
        }
        let json = ablation_proof_with(&kernel, true);
        assert!(
            json.contains(r#""status":"INSUFFICIENT_SAMPLES""#),
            "{json}"
        );
        assert!(json.contains(r#""model_configured":true"#));
        // Each arm collected exactly one classified lap; no delta is claimed.
        assert!(json.contains(r#""sample_count":1"#), "{json}");
        assert!(
            json.contains(
                r#""delta_on_minus_off":{"mean_turns":null,"mean_input_tokens":null,"mean_output_tokens":null,"mean_check_failed_count":null,"mean_check_passed_count":null,"mean_duration_ms":null}"#
            ),
            "{json}"
        );
        assert!(
            !json.contains(r#""suppression_event_receipt_ids":[]"#),
            "{json}"
        );
    }
}
