// The Forge Review Packet: one read that answers, for a single run, the
// questions a human needs before a ship decision - who built it, why, what
// changed (in semantic order, generated churn folded), what checks ran, which
// standards it cited, where the ship decision stands, and the one next move.
// It COMPOSES what already exists (the run event trail, the diff route's file
// list, the ship decisions) and adds two things: a semantic diff classifier
// and a DERIVED review_status, computed once here instead of inferred in every
// UI. Read-only: assembling a review opens no authority and writes nothing.
use crate::RouteResponse;
use crate::forge_diff_classify::{SECTION_ORDER, classify_path, section_index, section_label};
use mdx_core::{MdxKernel, Receipt};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/review-packet.json" {
        return None;
    }
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Some(Ok(response));
    }
    let run_id = json_string_field(body, "run_id").unwrap_or_default();
    if run_id.trim().is_empty() {
        return Some(Ok(refusal("name the run you want to review")));
    }
    Some(match assemble_review_packet_json(kernel, &run_id) {
        Ok(body) => Ok(RouteResponse::json("200 OK", body)),
        Err(error) => Err(error),
    })
}

pub(crate) fn assemble_review_packet_json(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
) -> Result<String, String> {
    // The diff is read first (it takes a brief read lock and shells git), then
    // the event fold takes its own read lock - never both at once.
    let diff_files = crate::forge_diff_route::run_diff_files(kernel, run_id)?;
    let candidate_ids = {
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Err("kernel lock poisoned".to_string()),
        };
        let fold = fold_run(&kernel, run_id);
        if fold.found {
            let primary_run_id = if fold.parallel_candidate_primary_run_id.trim().is_empty() {
                run_id.to_string()
            } else {
                fold.parallel_candidate_primary_run_id.clone()
            };
            candidate_run_ids(&kernel, &primary_run_id, run_id)
        } else {
            Vec::new()
        }
    };
    let mut candidate_diff_metrics = std::collections::BTreeMap::new();
    for candidate_run_id in candidate_ids {
        let files = if candidate_run_id == run_id {
            diff_files.clone()
        } else {
            crate::forge_diff_route::run_diff_files(kernel, &candidate_run_id)?
        };
        candidate_diff_metrics.insert(candidate_run_id, CandidateDiffMetrics::from_files(files));
    }
    let kernel = match kernel.read() {
        Ok(kernel) => kernel,
        Err(_) => return Err("kernel lock poisoned".to_string()),
    };
    Ok(render(&kernel, run_id, diff_files, &candidate_diff_metrics))
}

// One run's events, folded into the facts the packet reports.
#[derive(Default)]
struct RunFold {
    found: bool,
    status: String,
    finished: bool,
    branch: String,
    branch_sha: String,
    work_item_id: String,
    model: String,
    turns: u64,
    model_calls: u64,
    tool_calls: u64,
    checks_passed: u64,
    checks_failed: u64,
    check_names: Vec<(String, bool)>,
    baseline_checks_passed: u64,
    baseline_checks_failed: u64,
    baseline_check_names: Vec<(String, bool)>,
    standards: Vec<String>,
    repo_id: String,
    repo_primary_language: String,
    language_pack_id: String,
    repo_profile_detected_language_packs: Vec<String>,
    repo_profile_detected_files: Vec<String>,
    repo_profile_quality_signals: Vec<String>,
    repo_profile_standards_sources: Vec<String>,
    repo_profile_standards_source_fingerprints: Vec<String>,
    repo_profile_standards_source_summaries: Vec<String>,
    repo_profile_review_axes: Vec<String>,
    repo_profile_principal_review_gates: Vec<String>,
    repo_profile_language_pack_guidance: Vec<String>,
    repo_profile_semantic_intelligence: Vec<String>,
    repo_profile_semantic_tool_readiness: Vec<String>,
    repo_profile_semantic_session_id: String,
    repo_profile_semantic_fallback_index_status: String,
    repo_profile_semantic_source_file_count: u32,
    repo_profile_semantic_indexed_file_count: u32,
    repo_profile_semantic_indexed_symbol_count: u32,
    repo_profile_semantic_related_test_anchor_count: u32,
    repo_profile_semantic_session_grants_execution_authority: bool,
    repo_profile_toolchain_readiness: Vec<String>,
    repo_profile_suggested_checks: Vec<String>,
    repo_profile_artifact_patterns: Vec<String>,
    repo_profile_proof_plan_status: String,
    repo_profile_proof_plan_next_action: String,
    repo_profile_proof_plan_summary: String,
    selected_checks: Vec<String>,
    selected_checks_source: String,
    work_classification_recommended_shape: String,
    work_classification_task_class: String,
    work_classification_complexity_tier: String,
    work_classification_confidence_pct: u32,
    work_classification_rationale: String,
    work_classification_source: String,
    work_classification_grants_execution_authority: bool,
    language_task_corpus_id: String,
    language_task_alignment_status: String,
    language_task_alignment_source: String,
    language_task_class: String,
    language_task_complexity_tier: String,
    language_task_visible_check: String,
    language_task_hidden_check_slot: String,
    language_task_artifact_noise_expected: Vec<String>,
    language_task_required_principal_review_gates: Vec<String>,
    language_task_engineering_facets: String,
    language_task_evaluation_oracle: String,
    language_task_human_timebox_minutes: u32,
    language_task_contamination_policy: String,
    language_task_alignment_grants_execution_authority: bool,
    repo_profile_source: String,
    repo_profile_grants_execution_authority: bool,
    suggested_checks_are_authority: bool,
    principal_orientation_gate_required: bool,
    principal_orientation_gate_tool: String,
    principal_orientation_gate_observed: bool,
    principal_orientation_required_semantic_operations: Vec<String>,
    principal_orientation_semantic_strategy_required: bool,
    principal_orientation_dependency_map_required: bool,
    principal_orientation_gate_grants_authority: bool,
    semantic_query_operations_observed: Vec<String>,
    semantic_preflight_status: String,
    semantic_preflight_source: String,
    semantic_preflight_session_id: String,
    semantic_preflight_anchor_count: u32,
    semantic_preflight_planned_operations: Vec<String>,
    semantic_preflight_observed_operations: Vec<String>,
    semantic_preflight_indexed_file_count: u32,
    semantic_preflight_indexed_symbol_count: u32,
    semantic_preflight_related_test_anchor_count: u32,
    semantic_preflight_grants_execution_authority: bool,
    repo_intake_generated_from: String,
    repo_intake_readiness_status: String,
    repo_intake_medium_high_work_ready: bool,
    repo_intake_safe_next_move: String,
    repo_intake_semantic_orientation_operations: Vec<String>,
    repo_intake_proof_strategy: String,
    repo_intake_first_run_task_ids: Vec<String>,
    repo_intake_scout_status: String,
    repo_intake_scout_candidate_count: u32,
    repo_intake_scout_candidate_kinds: Vec<String>,
    repo_intake_scout_candidate_paths: Vec<String>,
    repo_intake_write_scope_hint: Vec<String>,
    repo_intake_off_limits_patterns: Vec<String>,
    repo_intake_review_focus: Vec<String>,
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
    mission_id: String,
    mission_milestone_id: String,
    mission_checkpoint_route: String,
    mission_checkpoint_grants_execution_authority: bool,
    parallel_candidate_primary_run_id: String,
    parallel_candidate_index: u32,
    parallel_candidate_count: u32,
    parallel_candidate_write_scope: Vec<String>,
    parallel_candidate_strategy_id: String,
    parallel_candidate_strategy_summary: String,
    parallel_candidate_required_semantic_operations: Vec<String>,
    parallel_candidate_proof_bias: String,
    candidate_selection_id: String,
    candidate_selection_primary_run_id: String,
    candidate_selection_selected_run_id: String,
    candidate_selection_current_run_id: String,
    candidate_selection_current_run_is_selected: bool,
    candidate_selection_recommended_run_id: String,
    candidate_selection_matches_recommendation: bool,
    candidate_selection_override_reason_recorded: bool,
    candidate_selection_reason: String,
    candidate_selection_override_recommendation_reason: String,
    candidate_selection_candidate_count: u32,
    candidate_selection_actor_id: String,
    candidate_selection_grants_delivery_authority: bool,
    candidate_selection_production_write_allowed: bool,
    fleet_stream_id: String,
    fleet_stream_semantic_strategy_id: String,
    fleet_stream_semantic_strategy_summary: String,
    fleet_stream_required_semantic_operations: Vec<String>,
    fleet_stream_proof_bias: String,
    fleet_integration_strategy_id: String,
    fleet_integration_strategy_summary: String,
    fleet_integration_required_semantic_operations: Vec<String>,
    fleet_integration_proof_bias: String,
    builder_casting_status: String,
    builder_casting_requested_slot: String,
    builder_casting_selected_slot: String,
    builder_casting_recommended_slot: String,
    builder_casting_selected_model_profile_id: String,
    builder_casting_selected_provider_family: String,
    builder_casting_selected_model_id: String,
    builder_casting_recommended_model_profile_id: String,
    builder_casting_recommended_provider_family: String,
    builder_casting_recommended_model_id: String,
    builder_casting_basis: String,
    builder_casting_matching_eval_score_count: u32,
    builder_casting_accepted_eval_score_count: u32,
    builder_casting_matching_run_count: u32,
    builder_casting_done_rate_pct: u32,
    builder_casting_requested_slot_matches_evidence: bool,
    builder_casting_grants_execution_authority: bool,
    eval_quality_gate_status: String,
    eval_machine_quality_gates_passed: bool,
    eval_can_be_scored_after_principal_review: bool,
    eval_passed_gate_count: u32,
    eval_failed_gate_count: u32,
    eval_passed_gates: Vec<String>,
    eval_failed_gates: Vec<String>,
    eval_result_receipt_id: String,
    eval_accepts_for_scoreboard: bool,
    eval_grants_execution_authority: bool,
    event_count: usize,
}

#[derive(Default)]
struct CandidateDiffMetrics {
    available: bool,
    file_count: usize,
    real_change_count: usize,
    generated_count: usize,
    artifact_count: usize,
    added_total: u32,
    removed_total: u32,
    source_paths: Vec<String>,
}

impl CandidateDiffMetrics {
    fn from_files(files: Option<Vec<crate::forge_diff_route::RunDiffFile>>) -> Self {
        let Some(files) = files else {
            return Self::default();
        };
        let mut metrics = Self {
            available: true,
            file_count: files.len(),
            ..Self::default()
        };
        for file in files {
            if crate::forge_repo_profile::is_artifact_path(&file.path) {
                metrics.artifact_count += 1;
                continue;
            }
            let (_, generated) = classify_path(&file.path);
            if generated {
                metrics.generated_count += 1;
            } else {
                metrics.real_change_count += 1;
                if metrics.source_paths.len() < 20 {
                    metrics.source_paths.push(file.path.clone());
                }
            }
            metrics.added_total += file.added;
            metrics.removed_total += file.removed;
        }
        metrics
    }
}

struct CandidateDiffQuality {
    score: i64,
    touched_language_pack_count: usize,
    basis: Vec<String>,
}

struct CandidateProofQuality {
    score: i64,
    proof_coverage_status: String,
    proof_scope_status: String,
    behavior_proof_status: String,
    setup_command_count: usize,
    proof_command_count: usize,
    satisfied_proof_command_count: usize,
    basis: Vec<String>,
}

fn fold_run(kernel: &MdxKernel, run_id: &str) -> RunFold {
    let mut fold = RunFold {
        found: false,
        status: "working".to_string(),
        finished: false,
        branch: String::new(),
        branch_sha: String::new(),
        work_item_id: String::new(),
        model: String::new(),
        turns: 0,
        model_calls: 0,
        tool_calls: 0,
        checks_passed: 0,
        checks_failed: 0,
        check_names: Vec::new(),
        baseline_checks_passed: 0,
        baseline_checks_failed: 0,
        baseline_check_names: Vec::new(),
        standards: Vec::new(),
        repo_id: String::new(),
        repo_primary_language: String::new(),
        language_pack_id: String::new(),
        repo_profile_detected_language_packs: Vec::new(),
        repo_profile_detected_files: Vec::new(),
        repo_profile_quality_signals: Vec::new(),
        repo_profile_standards_sources: Vec::new(),
        repo_profile_standards_source_fingerprints: Vec::new(),
        repo_profile_standards_source_summaries: Vec::new(),
        repo_profile_review_axes: Vec::new(),
        repo_profile_principal_review_gates: Vec::new(),
        repo_profile_language_pack_guidance: Vec::new(),
        repo_profile_semantic_intelligence: Vec::new(),
        repo_profile_semantic_tool_readiness: Vec::new(),
        repo_profile_semantic_session_id: String::new(),
        repo_profile_semantic_fallback_index_status: String::new(),
        repo_profile_semantic_source_file_count: 0,
        repo_profile_semantic_indexed_file_count: 0,
        repo_profile_semantic_indexed_symbol_count: 0,
        repo_profile_semantic_related_test_anchor_count: 0,
        repo_profile_semantic_session_grants_execution_authority: false,
        repo_profile_toolchain_readiness: Vec::new(),
        repo_profile_suggested_checks: Vec::new(),
        repo_profile_artifact_patterns: Vec::new(),
        repo_profile_proof_plan_status: String::new(),
        repo_profile_proof_plan_next_action: String::new(),
        repo_profile_proof_plan_summary: String::new(),
        selected_checks: Vec::new(),
        selected_checks_source: String::new(),
        work_classification_recommended_shape: String::new(),
        work_classification_task_class: String::new(),
        work_classification_complexity_tier: String::new(),
        work_classification_confidence_pct: 0,
        work_classification_rationale: String::new(),
        work_classification_source: String::new(),
        work_classification_grants_execution_authority: false,
        language_task_corpus_id: String::new(),
        language_task_alignment_status: String::new(),
        language_task_alignment_source: String::new(),
        language_task_class: String::new(),
        language_task_complexity_tier: String::new(),
        language_task_visible_check: String::new(),
        language_task_hidden_check_slot: String::new(),
        language_task_artifact_noise_expected: Vec::new(),
        language_task_required_principal_review_gates: Vec::new(),
        language_task_engineering_facets: String::new(),
        language_task_evaluation_oracle: String::new(),
        language_task_human_timebox_minutes: 0,
        language_task_contamination_policy: String::new(),
        language_task_alignment_grants_execution_authority: false,
        repo_profile_source: String::new(),
        repo_profile_grants_execution_authority: false,
        suggested_checks_are_authority: false,
        principal_orientation_gate_required: false,
        principal_orientation_gate_tool: String::new(),
        principal_orientation_gate_observed: false,
        principal_orientation_required_semantic_operations: Vec::new(),
        principal_orientation_semantic_strategy_required: false,
        principal_orientation_dependency_map_required: false,
        principal_orientation_gate_grants_authority: false,
        semantic_query_operations_observed: Vec::new(),
        semantic_preflight_status: String::new(),
        semantic_preflight_source: String::new(),
        semantic_preflight_session_id: String::new(),
        semantic_preflight_anchor_count: 0,
        semantic_preflight_planned_operations: Vec::new(),
        semantic_preflight_observed_operations: Vec::new(),
        semantic_preflight_indexed_file_count: 0,
        semantic_preflight_indexed_symbol_count: 0,
        semantic_preflight_related_test_anchor_count: 0,
        semantic_preflight_grants_execution_authority: false,
        repo_intake_generated_from: String::new(),
        repo_intake_readiness_status: String::new(),
        repo_intake_medium_high_work_ready: false,
        repo_intake_safe_next_move: String::new(),
        repo_intake_semantic_orientation_operations: Vec::new(),
        repo_intake_proof_strategy: String::new(),
        repo_intake_first_run_task_ids: Vec::new(),
        repo_intake_scout_status: String::new(),
        repo_intake_scout_candidate_count: 0,
        repo_intake_scout_candidate_kinds: Vec::new(),
        repo_intake_scout_candidate_paths: Vec::new(),
        repo_intake_write_scope_hint: Vec::new(),
        repo_intake_off_limits_patterns: Vec::new(),
        repo_intake_review_focus: Vec::new(),
        repo_intake_source_host: String::new(),
        repo_intake_origin_url_present: false,
        repo_intake_origin_url_recorded: false,
        repo_intake_source_host_readiness_route: String::new(),
        repo_intake_source_host_pr_draft_route: String::new(),
        repo_intake_provider_calls_allowed: false,
        repo_intake_run_started: false,
        repo_intake_network_call_allowed: false,
        repo_intake_production_write_allowed: false,
        repo_intake_grants_execution_authority: false,
        mission_id: String::new(),
        mission_milestone_id: String::new(),
        mission_checkpoint_route: String::new(),
        mission_checkpoint_grants_execution_authority: false,
        parallel_candidate_primary_run_id: String::new(),
        parallel_candidate_index: 1,
        parallel_candidate_count: 1,
        parallel_candidate_write_scope: Vec::new(),
        parallel_candidate_strategy_id: String::new(),
        parallel_candidate_strategy_summary: String::new(),
        parallel_candidate_required_semantic_operations: Vec::new(),
        parallel_candidate_proof_bias: String::new(),
        candidate_selection_id: String::new(),
        candidate_selection_primary_run_id: String::new(),
        candidate_selection_selected_run_id: String::new(),
        candidate_selection_current_run_id: String::new(),
        candidate_selection_current_run_is_selected: false,
        candidate_selection_recommended_run_id: String::new(),
        candidate_selection_matches_recommendation: false,
        candidate_selection_override_reason_recorded: false,
        candidate_selection_reason: String::new(),
        candidate_selection_override_recommendation_reason: String::new(),
        candidate_selection_candidate_count: 0,
        candidate_selection_actor_id: String::new(),
        candidate_selection_grants_delivery_authority: false,
        candidate_selection_production_write_allowed: false,
        fleet_stream_id: String::new(),
        fleet_stream_semantic_strategy_id: String::new(),
        fleet_stream_semantic_strategy_summary: String::new(),
        fleet_stream_required_semantic_operations: Vec::new(),
        fleet_stream_proof_bias: String::new(),
        fleet_integration_strategy_id: String::new(),
        fleet_integration_strategy_summary: String::new(),
        fleet_integration_required_semantic_operations: Vec::new(),
        fleet_integration_proof_bias: String::new(),
        builder_casting_status: String::new(),
        builder_casting_requested_slot: String::new(),
        builder_casting_selected_slot: String::new(),
        builder_casting_recommended_slot: String::new(),
        builder_casting_selected_model_profile_id: String::new(),
        builder_casting_selected_provider_family: String::new(),
        builder_casting_selected_model_id: String::new(),
        builder_casting_recommended_model_profile_id: String::new(),
        builder_casting_recommended_provider_family: String::new(),
        builder_casting_recommended_model_id: String::new(),
        builder_casting_basis: String::new(),
        builder_casting_matching_eval_score_count: 0,
        builder_casting_accepted_eval_score_count: 0,
        builder_casting_matching_run_count: 0,
        builder_casting_done_rate_pct: 0,
        builder_casting_requested_slot_matches_evidence: false,
        builder_casting_grants_execution_authority: false,
        eval_quality_gate_status: String::new(),
        eval_machine_quality_gates_passed: false,
        eval_can_be_scored_after_principal_review: false,
        eval_passed_gate_count: 0,
        eval_failed_gate_count: 0,
        eval_passed_gates: Vec::new(),
        eval_failed_gates: Vec::new(),
        eval_result_receipt_id: String::new(),
        eval_accepts_for_scoreboard: false,
        eval_grants_execution_authority: false,
        event_count: 0,
    };
    for receipt in kernel.ledger().query().by_kind("forge.run.event") {
        if receipt.payload.get("run_id").map(String::as_str) != Some(run_id) {
            continue;
        }
        fold.found = true;
        fold.event_count += 1;
        let event = payload(receipt, "event");
        let detail = payload(receipt, "detail");
        if let Some(turn) = receipt
            .payload
            .get("turn")
            .and_then(|t| t.parse::<u64>().ok())
        {
            fold.turns = fold.turns.max(turn);
        }
        match event {
            "run_started" => {
                fold.work_item_id = payload(receipt, "work_item_id").to_string();
                fold.repo_id = payload(receipt, "repo_id").to_string();
                fold.repo_primary_language = payload(receipt, "repo_primary_language").to_string();
                fold.language_pack_id = payload(receipt, "language_pack_id").to_string();
                fold.repo_profile_detected_language_packs =
                    csv_values(payload(receipt, "repo_profile_detected_language_packs"));
                fold.repo_profile_detected_files =
                    csv_values(payload(receipt, "repo_profile_detected_files"));
                fold.repo_profile_quality_signals =
                    csv_values(payload(receipt, "repo_profile_quality_signals"));
                fold.repo_profile_standards_sources =
                    csv_values(payload(receipt, "repo_profile_standards_sources"));
                fold.repo_profile_standards_source_fingerprints = csv_values(payload(
                    receipt,
                    "repo_profile_standards_source_fingerprints",
                ));
                fold.repo_profile_standards_source_summaries =
                    csv_values(payload(receipt, "repo_profile_standards_source_summaries"));
                fold.repo_profile_review_axes =
                    csv_values(payload(receipt, "repo_profile_review_axes"));
                fold.repo_profile_principal_review_gates =
                    csv_values(payload(receipt, "repo_profile_principal_review_gates"));
                fold.repo_profile_language_pack_guidance =
                    csv_values(payload(receipt, "repo_profile_language_pack_guidance"));
                fold.repo_profile_semantic_intelligence =
                    csv_values(payload(receipt, "repo_profile_semantic_intelligence"));
                fold.repo_profile_semantic_tool_readiness =
                    csv_values(payload(receipt, "repo_profile_semantic_tool_readiness"));
                fold.repo_profile_semantic_session_id =
                    payload(receipt, "repo_profile_semantic_session_id").to_string();
                fold.repo_profile_semantic_fallback_index_status =
                    payload(receipt, "repo_profile_semantic_fallback_index_status").to_string();
                fold.repo_profile_semantic_source_file_count =
                    payload(receipt, "repo_profile_semantic_source_file_count")
                        .parse()
                        .unwrap_or(0);
                fold.repo_profile_semantic_indexed_file_count =
                    payload(receipt, "repo_profile_semantic_indexed_file_count")
                        .parse()
                        .unwrap_or(0);
                fold.repo_profile_semantic_indexed_symbol_count =
                    payload(receipt, "repo_profile_semantic_indexed_symbol_count")
                        .parse()
                        .unwrap_or(0);
                fold.repo_profile_semantic_related_test_anchor_count =
                    payload(receipt, "repo_profile_semantic_related_test_anchor_count")
                        .parse()
                        .unwrap_or(0);
                fold.repo_profile_semantic_session_grants_execution_authority = payload(
                    receipt,
                    "repo_profile_semantic_session_grants_execution_authority",
                ) == "true";
                fold.repo_profile_toolchain_readiness =
                    csv_values(payload(receipt, "repo_profile_toolchain_readiness"));
                fold.repo_profile_suggested_checks =
                    csv_values(payload(receipt, "repo_profile_suggested_checks"));
                fold.repo_profile_artifact_patterns =
                    csv_values(payload(receipt, "repo_profile_artifact_patterns"));
                fold.repo_profile_proof_plan_status =
                    payload(receipt, "repo_profile_proof_plan_status").to_string();
                fold.repo_profile_proof_plan_next_action =
                    payload(receipt, "repo_profile_proof_plan_next_action").to_string();
                fold.repo_profile_proof_plan_summary =
                    payload(receipt, "repo_profile_proof_plan_summary").to_string();
                fold.selected_checks = csv_values(payload(receipt, "selected_checks"));
                fold.selected_checks_source =
                    payload(receipt, "selected_checks_source").to_string();
                fold.work_classification_recommended_shape =
                    payload(receipt, "work_classification_recommended_shape").to_string();
                fold.work_classification_task_class =
                    payload(receipt, "work_classification_task_class").to_string();
                fold.work_classification_complexity_tier =
                    payload(receipt, "work_classification_complexity_tier").to_string();
                fold.work_classification_confidence_pct =
                    payload(receipt, "work_classification_confidence_pct")
                        .parse()
                        .unwrap_or(0);
                fold.work_classification_rationale =
                    payload(receipt, "work_classification_rationale").to_string();
                fold.work_classification_source =
                    payload(receipt, "work_classification_source").to_string();
                fold.work_classification_grants_execution_authority =
                    payload(receipt, "work_classification_grants_execution_authority") == "true";
                fold.language_task_corpus_id =
                    payload(receipt, "language_task_corpus_id").to_string();
                fold.language_task_alignment_status =
                    payload(receipt, "language_task_alignment_status").to_string();
                fold.language_task_alignment_source =
                    payload(receipt, "language_task_alignment_source").to_string();
                fold.language_task_class = payload(receipt, "language_task_class").to_string();
                fold.language_task_complexity_tier =
                    payload(receipt, "language_task_complexity_tier").to_string();
                fold.language_task_visible_check =
                    payload(receipt, "language_task_visible_check").to_string();
                fold.language_task_hidden_check_slot =
                    payload(receipt, "language_task_hidden_check_slot").to_string();
                fold.language_task_artifact_noise_expected =
                    csv_values(payload(receipt, "language_task_artifact_noise_expected"));
                fold.language_task_required_principal_review_gates = csv_values(payload(
                    receipt,
                    "language_task_required_principal_review_gates",
                ));
                fold.language_task_engineering_facets =
                    payload(receipt, "language_task_engineering_facets").to_string();
                fold.language_task_evaluation_oracle =
                    payload(receipt, "language_task_evaluation_oracle").to_string();
                fold.language_task_human_timebox_minutes =
                    payload(receipt, "language_task_human_timebox_minutes")
                        .parse()
                        .unwrap_or(0);
                fold.language_task_contamination_policy =
                    payload(receipt, "language_task_contamination_policy").to_string();
                fold.language_task_alignment_grants_execution_authority = payload(
                    receipt,
                    "language_task_alignment_grants_execution_authority",
                ) == "true";
                fold.repo_profile_source = payload(receipt, "repo_profile_source").to_string();
                fold.repo_profile_grants_execution_authority =
                    payload(receipt, "repo_profile_grants_execution_authority") == "true";
                fold.suggested_checks_are_authority =
                    payload(receipt, "suggested_checks_are_authority") == "true";
                fold.principal_orientation_gate_required =
                    payload(receipt, "principal_orientation_gate_required") == "true";
                fold.principal_orientation_gate_tool =
                    payload(receipt, "principal_orientation_gate_tool").to_string();
                fold.principal_orientation_gate_grants_authority =
                    payload(receipt, "principal_orientation_gate_grants_authority") == "true";
                let required_operations = newline_values(payload(
                    receipt,
                    "principal_orientation_required_semantic_operations",
                ));
                if !required_operations.is_empty() {
                    fold.principal_orientation_required_semantic_operations = required_operations;
                }
                if !payload(receipt, "principal_orientation_semantic_strategy_required").is_empty()
                {
                    fold.principal_orientation_semantic_strategy_required =
                        payload(receipt, "principal_orientation_semantic_strategy_required")
                            == "true";
                }
                if !payload(receipt, "principal_orientation_dependency_map_required").is_empty() {
                    fold.principal_orientation_dependency_map_required =
                        payload(receipt, "principal_orientation_dependency_map_required") == "true";
                }
                fold.repo_intake_generated_from =
                    payload(receipt, "repo_intake_generated_from").to_string();
                fold.repo_intake_readiness_status =
                    payload(receipt, "repo_intake_readiness_status").to_string();
                fold.repo_intake_medium_high_work_ready =
                    payload(receipt, "repo_intake_medium_high_work_ready") == "true";
                fold.repo_intake_safe_next_move =
                    payload(receipt, "repo_intake_safe_next_move").to_string();
                fold.repo_intake_semantic_orientation_operations = csv_values(payload(
                    receipt,
                    "repo_intake_semantic_orientation_operations",
                ));
                fold.repo_intake_proof_strategy =
                    payload(receipt, "repo_intake_proof_strategy").to_string();
                fold.repo_intake_first_run_task_ids =
                    csv_values(payload(receipt, "repo_intake_first_run_task_ids"));
                fold.repo_intake_scout_status =
                    payload(receipt, "repo_intake_scout_status").to_string();
                fold.repo_intake_scout_candidate_count =
                    payload(receipt, "repo_intake_scout_candidate_count")
                        .parse()
                        .unwrap_or(0);
                fold.repo_intake_scout_candidate_kinds =
                    csv_values(payload(receipt, "repo_intake_scout_candidate_kinds"));
                fold.repo_intake_scout_candidate_paths =
                    csv_values(payload(receipt, "repo_intake_scout_candidate_paths"));
                fold.repo_intake_write_scope_hint =
                    csv_values(payload(receipt, "repo_intake_write_scope_hint"));
                fold.repo_intake_off_limits_patterns =
                    csv_values(payload(receipt, "repo_intake_off_limits_patterns"));
                fold.repo_intake_review_focus =
                    csv_values(payload(receipt, "repo_intake_review_focus"));
                fold.repo_intake_source_host =
                    payload(receipt, "repo_intake_source_host").to_string();
                fold.repo_intake_origin_url_present =
                    payload(receipt, "repo_intake_origin_url_present") == "true";
                fold.repo_intake_origin_url_recorded =
                    payload(receipt, "repo_intake_origin_url_recorded") == "true";
                fold.repo_intake_source_host_readiness_route =
                    payload(receipt, "repo_intake_source_host_readiness_route").to_string();
                fold.repo_intake_source_host_pr_draft_route =
                    payload(receipt, "repo_intake_source_host_pr_draft_route").to_string();
                fold.repo_intake_provider_calls_allowed =
                    payload(receipt, "repo_intake_provider_calls_allowed") == "true";
                fold.repo_intake_run_started =
                    payload(receipt, "repo_intake_run_started") == "true";
                fold.repo_intake_network_call_allowed =
                    payload(receipt, "repo_intake_network_call_allowed") == "true";
                fold.repo_intake_production_write_allowed =
                    payload(receipt, "repo_intake_production_write_allowed") == "true";
                fold.repo_intake_grants_execution_authority =
                    payload(receipt, "repo_intake_grants_execution_authority") == "true";
                fold.mission_id = payload(receipt, "mission_id").to_string();
                fold.mission_milestone_id = payload(receipt, "mission_milestone_id").to_string();
                fold.mission_checkpoint_route =
                    payload(receipt, "mission_checkpoint_route").to_string();
                fold.mission_checkpoint_grants_execution_authority =
                    payload(receipt, "mission_checkpoint_grants_execution_authority") == "true";
                let primary_run_id = payload(receipt, "parallel_candidate_primary_run_id");
                if !primary_run_id.trim().is_empty() {
                    fold.parallel_candidate_primary_run_id = primary_run_id.to_string();
                }
                fold.parallel_candidate_index = payload(receipt, "parallel_candidate_index")
                    .parse()
                    .unwrap_or(1);
                fold.parallel_candidate_count = payload(receipt, "parallel_candidate_count")
                    .parse()
                    .unwrap_or(1);
                fold.parallel_candidate_write_scope =
                    newline_values(payload(receipt, "parallel_candidate_write_scope"));
                fold.parallel_candidate_strategy_id =
                    payload(receipt, "parallel_candidate_strategy_id").to_string();
                fold.parallel_candidate_strategy_summary =
                    payload(receipt, "parallel_candidate_strategy_summary").to_string();
                fold.parallel_candidate_required_semantic_operations = csv_values(payload(
                    receipt,
                    "parallel_candidate_required_semantic_operations",
                ));
                fold.parallel_candidate_proof_bias =
                    payload(receipt, "parallel_candidate_proof_bias").to_string();
                fold.fleet_stream_id = payload(receipt, "fleet_stream_id").to_string();
                fold.fleet_stream_semantic_strategy_id =
                    payload(receipt, "fleet_stream_semantic_strategy_id").to_string();
                fold.fleet_stream_semantic_strategy_summary =
                    payload(receipt, "fleet_stream_semantic_strategy_summary").to_string();
                fold.fleet_stream_required_semantic_operations = newline_values(payload(
                    receipt,
                    "fleet_stream_required_semantic_operations",
                ));
                fold.fleet_stream_proof_bias =
                    payload(receipt, "fleet_stream_proof_bias").to_string();
                fold.fleet_integration_strategy_id =
                    payload(receipt, "fleet_integration_strategy_id").to_string();
                fold.fleet_integration_strategy_summary =
                    payload(receipt, "fleet_integration_strategy_summary").to_string();
                fold.fleet_integration_required_semantic_operations = newline_values(payload(
                    receipt,
                    "fleet_integration_required_semantic_operations",
                ));
                fold.fleet_integration_proof_bias =
                    payload(receipt, "fleet_integration_proof_bias").to_string();
                fold.builder_casting_status =
                    payload(receipt, "builder_casting_status").to_string();
                fold.builder_casting_requested_slot =
                    payload(receipt, "builder_casting_requested_slot").to_string();
                fold.builder_casting_selected_slot =
                    payload(receipt, "builder_casting_selected_slot").to_string();
                fold.builder_casting_recommended_slot =
                    payload(receipt, "builder_casting_recommended_slot").to_string();
                fold.builder_casting_selected_model_profile_id =
                    payload(receipt, "builder_casting_selected_model_profile_id").to_string();
                fold.builder_casting_selected_provider_family =
                    payload(receipt, "builder_casting_selected_provider_family").to_string();
                fold.builder_casting_selected_model_id =
                    payload(receipt, "builder_casting_selected_model_id").to_string();
                fold.builder_casting_recommended_model_profile_id =
                    payload(receipt, "builder_casting_recommended_model_profile_id").to_string();
                fold.builder_casting_recommended_provider_family =
                    payload(receipt, "builder_casting_recommended_provider_family").to_string();
                fold.builder_casting_recommended_model_id =
                    payload(receipt, "builder_casting_recommended_model_id").to_string();
                fold.builder_casting_basis = payload(receipt, "builder_casting_basis").to_string();
                fold.builder_casting_matching_eval_score_count =
                    payload(receipt, "builder_casting_matching_eval_score_count")
                        .parse()
                        .unwrap_or(0);
                fold.builder_casting_accepted_eval_score_count =
                    payload(receipt, "builder_casting_accepted_eval_score_count")
                        .parse()
                        .unwrap_or(0);
                fold.builder_casting_matching_run_count =
                    payload(receipt, "builder_casting_matching_run_count")
                        .parse()
                        .unwrap_or(0);
                fold.builder_casting_done_rate_pct =
                    payload(receipt, "builder_casting_done_rate_pct")
                        .parse()
                        .unwrap_or(0);
                fold.builder_casting_requested_slot_matches_evidence =
                    payload(receipt, "builder_casting_requested_slot_matches_evidence") == "true";
                fold.builder_casting_grants_execution_authority =
                    payload(receipt, "builder_casting_grants_execution_authority") == "true";
                if let Some(branch) = detail.strip_prefix("branch=") {
                    fold.branch = branch.split(' ').next().unwrap_or("").to_string();
                    if let Some(sha) = detail
                        .split_whitespace()
                        .find_map(|part| part.strip_prefix("sha="))
                        && !sha.is_empty()
                    {
                        fold.branch_sha = sha.to_string();
                    }
                }
            }
            "model_called" => {
                fold.model_calls += 1;
                // Attribute the builder, not the trailing voice-rewrite/summary
                // pass or a delegated sub-call - those run AFTER the build and
                // would otherwise overwrite the credit (e.g. show the cheap
                // summarizer as the coder). Take the FIRST real builder call,
                // matching the run list's modelFrom and the loop's "model
                // identity rides the first model_called event" contract.
                let builder_call =
                    !detail.starts_with("voice rewrite") && !detail.starts_with("delegated");
                if builder_call
                    && fold.model.is_empty()
                    && let Some(model) = detail.split("model=").nth(1)
                {
                    let name = model.split(' ').next().unwrap_or("").to_string();
                    if !name.is_empty() {
                        fold.model = name;
                    }
                }
            }
            "tool_executed" => {
                fold.tool_calls += 1;
                if semantic_query_detail_succeeded(detail) {
                    fold.principal_orientation_gate_observed = true;
                    if let Some(operation) = semantic_query_operation(detail)
                        && !fold
                            .semantic_query_operations_observed
                            .iter()
                            .any(|seen| seen == &operation)
                    {
                        fold.semantic_query_operations_observed.push(operation);
                    }
                }
            }
            "evidence_appended" if detail.contains("semantic_preflight_observed") => {
                fold.semantic_preflight_status =
                    payload(receipt, "semantic_preflight_status").to_string();
                fold.semantic_preflight_source =
                    payload(receipt, "semantic_preflight_source").to_string();
                fold.semantic_preflight_session_id =
                    payload(receipt, "semantic_preflight_session_id").to_string();
                fold.semantic_preflight_anchor_count =
                    payload(receipt, "semantic_preflight_anchor_count")
                        .parse()
                        .unwrap_or(0);
                fold.semantic_preflight_planned_operations =
                    csv_values(payload(receipt, "semantic_preflight_planned_operations"));
                fold.semantic_preflight_observed_operations =
                    csv_values(payload(receipt, "semantic_preflight_observed_operations"));
                fold.semantic_preflight_indexed_file_count =
                    payload(receipt, "semantic_preflight_indexed_file_count")
                        .parse()
                        .unwrap_or(0);
                fold.semantic_preflight_indexed_symbol_count =
                    payload(receipt, "semantic_preflight_indexed_symbol_count")
                        .parse()
                        .unwrap_or(0);
                fold.semantic_preflight_related_test_anchor_count =
                    payload(receipt, "semantic_preflight_related_test_anchor_count")
                        .parse()
                        .unwrap_or(0);
                fold.semantic_preflight_grants_execution_authority =
                    payload(receipt, "semantic_preflight_grants_execution_authority") == "true";
            }
            "evidence_appended" if detail.contains("candidate_selection_recorded") => {
                fold.candidate_selection_id =
                    payload(receipt, "candidate_selection_id").to_string();
                fold.candidate_selection_primary_run_id =
                    payload(receipt, "candidate_selection_primary_run_id").to_string();
                fold.candidate_selection_selected_run_id =
                    payload(receipt, "candidate_selection_selected_run_id").to_string();
                fold.candidate_selection_current_run_id =
                    payload(receipt, "candidate_selection_current_run_id").to_string();
                fold.candidate_selection_current_run_is_selected =
                    payload(receipt, "candidate_selection_current_run_is_selected") == "true";
                fold.candidate_selection_recommended_run_id =
                    payload(receipt, "candidate_selection_recommended_run_id").to_string();
                fold.candidate_selection_matches_recommendation =
                    payload(receipt, "candidate_selection_matches_recommendation") == "true";
                fold.candidate_selection_override_reason_recorded =
                    payload(receipt, "candidate_selection_override_reason_recorded") == "true";
                fold.candidate_selection_reason =
                    payload(receipt, "candidate_selection_reason").to_string();
                fold.candidate_selection_override_recommendation_reason = payload(
                    receipt,
                    "candidate_selection_override_recommendation_reason",
                )
                .to_string();
                fold.candidate_selection_candidate_count =
                    payload(receipt, "candidate_selection_candidate_count")
                        .parse()
                        .unwrap_or(0);
                fold.candidate_selection_actor_id =
                    payload(receipt, "candidate_selection_actor_id").to_string();
                fold.candidate_selection_grants_delivery_authority =
                    payload(receipt, "candidate_selection_grants_delivery_authority") == "true";
                fold.candidate_selection_production_write_allowed =
                    payload(receipt, "candidate_selection_production_write_allowed") == "true";
            }
            "check_passed" => {
                if detail.trim_start().starts_with("baseline_run_command ") {
                    fold.baseline_checks_passed += 1;
                    fold.baseline_check_names.push((detail.to_string(), true));
                } else {
                    fold.checks_passed += 1;
                    fold.check_names.push((detail.to_string(), true));
                }
            }
            "check_failed" => {
                if detail.trim_start().starts_with("baseline_run_command ") {
                    fold.baseline_checks_failed += 1;
                    fold.baseline_check_names.push((detail.to_string(), false));
                } else {
                    fold.checks_failed += 1;
                    fold.check_names.push((detail.to_string(), false));
                }
            }
            "run_finished" => {
                fold.finished = true;
                fold.status = status_from(detail);
            }
            "evidence_appended" if detail.contains("eval_quality_gate_assessed") => {
                fold.eval_quality_gate_status =
                    payload(receipt, "eval_quality_gate_status").to_string();
                fold.eval_machine_quality_gates_passed =
                    payload(receipt, "eval_machine_quality_gates_passed") == "true";
                fold.eval_can_be_scored_after_principal_review =
                    payload(receipt, "eval_can_be_scored_after_principal_review") == "true";
                fold.eval_passed_gate_count = payload(receipt, "eval_passed_gate_count")
                    .parse()
                    .unwrap_or(0);
                fold.eval_failed_gate_count = payload(receipt, "eval_failed_gate_count")
                    .parse()
                    .unwrap_or(0);
                fold.eval_passed_gates = csv_values(payload(receipt, "eval_passed_gates"));
                fold.eval_failed_gates = csv_values(payload(receipt, "eval_failed_gates"));
                fold.eval_result_receipt_id =
                    payload(receipt, "eval_result_receipt_id").to_string();
                fold.eval_accepts_for_scoreboard =
                    payload(receipt, "eval_accepts_for_scoreboard") == "true";
                fold.eval_grants_execution_authority =
                    payload(receipt, "eval_grants_execution_authority") == "true";
            }
            _ => {}
        }
        if fold.branch.is_empty()
            && let Some(branch) = detail.strip_prefix("branch=")
        {
            fold.branch = branch.split(' ').next().unwrap_or("").to_string();
        }
        if let Some(rest) = detail.strip_prefix("building to your standards: ") {
            fold.standards = rest
                .split(", ")
                .map(|name| name.trim().to_string())
                .filter(|name| !name.is_empty())
                .collect();
        }
    }
    fold
}

// The terminal status a run reached, from its run_finished detail - the same
// vocabulary the run projection derives, read once here.
fn status_from(detail: &str) -> String {
    let upper = detail.to_ascii_uppercase();
    if upper.contains("RUN_FINISHED_DONE") {
        "done".to_string()
    } else if upper.contains("RUN_FINISHED_NO_CHANGE") {
        "no_change".to_string()
    } else if upper.contains("RUN_STOPPED") {
        "stopped".to_string()
    } else if upper.contains("CANNOT_PROCEED") {
        "cannot_proceed".to_string()
    } else if upper.contains("BUDGET_EXHAUSTED") {
        "budget_exhausted".to_string()
    } else if upper.contains("ERROR") || upper.contains("FAILED") {
        "error".to_string()
    } else {
        "finished".to_string()
    }
}

fn semantic_query_operation(detail: &str) -> Option<String> {
    let rest = detail.strip_prefix("semantic_query ")?;
    if !semantic_query_detail_rest_succeeded(rest) {
        return None;
    }
    for token in rest.split_whitespace() {
        if let Some(operation) = token.strip_prefix("operation=") {
            if is_semantic_query_operation(operation) {
                return Some(operation.to_string());
            }
        } else if is_semantic_query_operation(token) {
            return Some(token.to_string());
        }
    }
    None
}

fn semantic_query_detail_succeeded(detail: &str) -> bool {
    detail
        .strip_prefix("semantic_query ")
        .is_some_and(semantic_query_detail_rest_succeeded)
}

fn semantic_query_detail_rest_succeeded(rest: &str) -> bool {
    for token in rest.split_whitespace() {
        if let Some(status) = token.strip_prefix("status=") {
            return status == "OK";
        }
    }
    true
}

fn is_semantic_query_operation(operation: &str) -> bool {
    matches!(
        operation,
        "capabilities"
            | "orientation"
            | "workspace_symbol"
            | "definition"
            | "references"
            | "impact_radius"
            | "dependency_map"
            | "symbol_graph"
            | "hover"
            | "file_outline"
            | "related_tests"
            | "diagnostics"
            | "lsp_probe"
    )
}

// The latest review panel recorded for this run, if one was convened. Folded
// into the packet so the room shows the panel's consensus, dissent, and blind
// spots inline - dissent kept first-class, never collapsed.
fn latest_panel(kernel: &MdxKernel, run_id: &str) -> Option<serde_json::Value> {
    let receipt = kernel
        .ledger()
        .query()
        .by_kind("forge.review.panel.recorded")
        .into_iter()
        .rfind(|receipt| receipt.payload.get("run_id").map(String::as_str) == Some(run_id))?;
    let count: usize = payload(receipt, "member_count").parse().unwrap_or(0);
    let members: Vec<serde_json::Value> = (0..count)
        .map(|i| {
            serde_json::json!({
                "model": payload(receipt, &format!("member_{i}_model")),
                "stance": payload(receipt, &format!("member_{i}_stance")),
                "verdict": payload(receipt, &format!("member_{i}_verdict")),
                "concern": payload(receipt, &format!("member_{i}_concern")),
            })
        })
        .collect();
    let dissent: Vec<&str> = payload(receipt, "dissent")
        .split(", ")
        .filter(|s| !s.is_empty())
        .collect();
    Some(serde_json::json!({
        "consensus": payload(receipt, "consensus"),
        "confidence": payload(receipt, "confidence"),
        "residency": payload(receipt, "residency"),
        "members": members,
        "dissent": dissent,
        "blind_spots": payload(receipt, "blind_spots"),
        "receipt_id": receipt.receipt_id,
    }))
}

// The run-level ship decision, if one was recorded.
struct ShipDecision {
    decided: bool,
    reason: String,
    receipt_id: String,
}

fn ship_decision(kernel: &MdxKernel, run_id: &str) -> ShipDecision {
    for receipt in kernel.ledger().query().by_kind("forge.run.ship.decided") {
        if receipt.payload.get("run_id").map(String::as_str) == Some(run_id) {
            return ShipDecision {
                decided: true,
                reason: payload(receipt, "reason").to_string(),
                receipt_id: receipt.receipt_id.clone(),
            };
        }
    }
    ShipDecision {
        decided: false,
        reason: String::new(),
        receipt_id: String::new(),
    }
}

// The review status, DERIVED once. The whole point: every surface reads this
// instead of re-inferring it from the trail and drifting.
fn review_status(fold: &RunFold, ship: &ShipDecision) -> (&'static str, &'static str) {
    if ship.decided {
        return ("shipped", "Shipped. The change is on the record.");
    }
    if !fold.finished || fold.status == "working" {
        return ("working", "It is still building. Let it run, or steer it.");
    }
    match fold.status.as_str() {
        "done" => (
            "ready_for_review",
            "Review the change, then decide: ship, ask a revision, or stop.",
        ),
        "cannot_proceed" | "error" | "budget_exhausted" => (
            "needs_attention",
            "It could not finish on its own. Read why, then revise or stop.",
        ),
        "stopped" => (
            "stopped",
            "It was stopped. Revise to continue, or leave it.",
        ),
        _ => (
            "ready_for_review",
            "Review the change and decide the next move.",
        ),
    }
}

fn observed_check_command(detail: &str) -> String {
    let trimmed = detail.trim();
    let Some(rest) = trimmed.strip_prefix("run_command ") else {
        return trimmed.to_string();
    };
    if let Some((command, _)) = rest.split_once(" exit=") {
        return command.trim().to_string();
    }
    if let Some(command) = rest.strip_prefix("unavailable ") {
        return command.trim().to_string();
    }
    if let Some(command) = rest.strip_prefix("refused ") {
        return command.trim().to_string();
    }
    rest.trim().to_string()
}

fn proof_coverage(fold: &RunFold) -> serde_json::Value {
    let observed: Vec<(String, bool)> = fold
        .check_names
        .iter()
        .map(|(name, ok)| (observed_check_command(name), *ok))
        .collect();
    let (selected_checks, selected_checks_source) = effective_selected_checks(fold, &observed);
    let setup_commands = selected_checks
        .iter()
        .filter(|check| crate::forge_repo_profile::is_setup_command(check))
        .cloned()
        .collect::<Vec<_>>();
    let proof_commands = selected_checks
        .iter()
        .filter(|check| !crate::forge_repo_profile::is_setup_command(check))
        .cloned()
        .collect::<Vec<_>>();
    let mut satisfied_checks = Vec::new();
    let mut missing_checks = Vec::new();
    let mut failed_checks = Vec::new();
    for selected in &selected_checks {
        let selected = selected.trim();
        if selected.is_empty() {
            continue;
        }
        let mut latest_result = None;
        for (observed_name, ok) in &observed {
            if observed_name == selected {
                latest_result = Some(*ok);
            }
        }
        match latest_result {
            Some(true) => satisfied_checks.push(selected.to_string()),
            Some(false) => failed_checks.push(selected.to_string()),
            None => missing_checks.push(selected.to_string()),
        }
    }
    satisfied_checks.sort();
    satisfied_checks.dedup();
    failed_checks.sort();
    failed_checks.dedup();
    missing_checks.sort();
    missing_checks.dedup();
    let satisfied_setup_commands = satisfied_checks
        .iter()
        .filter(|check| crate::forge_repo_profile::is_setup_command(check))
        .cloned()
        .collect::<Vec<_>>();
    let satisfied_proof_commands = satisfied_checks
        .iter()
        .filter(|check| !crate::forge_repo_profile::is_setup_command(check))
        .cloned()
        .collect::<Vec<_>>();

    let status = if fold.status == "no_change" {
        "no_change"
    } else if selected_checks.is_empty() {
        "not_recorded"
    } else if proof_commands.is_empty() {
        "no_proof_command"
    } else if !failed_checks.is_empty() {
        "failed"
    } else if !missing_checks.is_empty() {
        "missing"
    } else {
        "satisfied"
    };
    let summary = match status {
        "satisfied" => format!(
            "Proof covered: all selected checks were observed passing ({})",
            selected_checks.join(", ")
        ),
        "failed" => format!(
            "Proof failed: selected check(s) failed ({})",
            failed_checks.join(", ")
        ),
        "missing" => format!(
            "Proof missing: selected check(s) were not observed ({})",
            missing_checks.join(", ")
        ),
        "no_proof_command" => {
            "Proof not established: setup commands were selected, but no behavioral proof command was selected.".to_string()
        }
        "no_change" => {
            "Proof did not establish task completion: checks ran against unchanged code, so this is baseline health, not a completed change.".to_string()
        }
        _ => "Proof not recorded: no selected checks were captured for this run".to_string(),
    };
    let observed_checks: Vec<serde_json::Value> = observed
        .iter()
        .map(|(name, ok)| serde_json::json!({ "name": name, "ok": ok }))
        .collect();

    serde_json::json!({
        "generated_from": "repo_intelligence+selected_checks+observed_check_events",
        "match_policy": "exact_selected_command",
        "status": status,
        "summary": summary,
        "selected_checks": selected_checks,
        "selected_checks_source": selected_checks_source,
        "setup_commands": setup_commands,
        "proof_commands": proof_commands,
        "observed_checks": observed_checks,
        "satisfied_checks": satisfied_checks,
        "satisfied_setup_commands": satisfied_setup_commands,
        "satisfied_proof_commands": satisfied_proof_commands,
        "missing_checks": missing_checks,
        "failed_checks": failed_checks,
    })
}

fn effective_selected_checks(fold: &RunFold, observed: &[(String, bool)]) -> (Vec<String>, String) {
    if !fold.selected_checks.is_empty() {
        return (
            fold.selected_checks.clone(),
            selected_checks_source(&fold.selected_checks_source).to_string(),
        );
    }
    if !fold.repo_profile_suggested_checks.is_empty() {
        return (
            fold.repo_profile_suggested_checks.clone(),
            "repo_profile_fallback".to_string(),
        );
    }
    let mut observed_checks = observed
        .iter()
        .filter_map(|(name, _)| {
            let name = name.trim();
            if name.is_empty() {
                None
            } else {
                Some(name.to_string())
            }
        })
        .collect::<Vec<_>>();
    observed_checks.sort();
    observed_checks.dedup();
    if observed_checks.is_empty() {
        (
            Vec::new(),
            selected_checks_source(&fold.selected_checks_source).to_string(),
        )
    } else {
        (observed_checks, "observed_legacy".to_string())
    }
}

fn selected_checks_source(source: &str) -> &str {
    if source.trim().is_empty() {
        "unknown_legacy"
    } else {
        source
    }
}

fn lsp_execution_status_from_readiness(readiness: &[String]) -> &'static str {
    let has_lsp_entry = readiness.iter().any(|item| item.starts_with("lsp:"));
    if !has_lsp_entry {
        return "not_configured";
    }
    if !crate::forge_semantic_lsp::live_lsp_enabled() {
        return "disabled";
    }
    if readiness
        .iter()
        .any(|item| item.starts_with("lsp:") && item.ends_with("=available"))
    {
        "available"
    } else {
        "missing_server"
    }
}

fn proof_language_pack(command: &str) -> Option<&'static str> {
    let normalized = command.split_whitespace().collect::<Vec<_>>().join(" ");
    let command = normalized.as_str();
    let parts = command.split_whitespace().collect::<Vec<_>>();
    if command == "xcodebuild test" || command.starts_with("xcodebuild test ") {
        return Some("ios-xcode");
    }
    if command == "swift test"
        || command.starts_with("swift test ")
        || command.starts_with("swift run ")
    {
        return Some("swift-spm");
    }
    if crate::forge_repo_profile::is_setup_command(command) {
        return None;
    }
    if parts.first() == Some(&"mvn") && parts.contains(&"test") {
        return Some("java-maven");
    }
    if command == "./gradlew testDebugUnitTest"
        || command.starts_with("./gradlew testDebugUnitTest ")
        || command.contains(":testDebugUnitTest")
        || command.contains(" testDebugUnitTest")
    {
        return Some("android-gradle");
    }
    if parts.first() == Some(&"./gradlew")
        && parts
            .iter()
            .skip(1)
            .any(|part| *part == "test" || part.ends_with(":test"))
    {
        return Some("gradle-jvm");
    }
    if command == "dotnet test" || command.starts_with("dotnet test ") {
        return Some("dotnet");
    }
    if command == "npm test"
        || command.starts_with("npm test ")
        || command == "npm run test"
        || command.starts_with("npm run test ")
        || command == "pnpm test"
        || command.starts_with("pnpm test ")
        || command == "pnpm run test"
        || command.starts_with("pnpm run test ")
        || command == "yarn test"
        || command.starts_with("yarn test ")
    {
        return Some("node");
    }
    if command == "cargo test" || command.starts_with("cargo test ") {
        return Some("rust-cargo");
    }
    if command == "pytest"
        || command.starts_with("pytest ")
        || command.starts_with("python -m pytest")
        || command.ends_with("&& pytest")
        || command.contains("&& pytest ")
    {
        return Some("python");
    }
    if command == "go test" || command.starts_with("go test ") {
        return Some("go");
    }
    None
}

fn proof_scope(fold: &RunFold, impacted_language_packs: &[String]) -> serde_json::Value {
    let observed = fold
        .check_names
        .iter()
        .map(|(name, ok)| (observed_check_command(name), *ok))
        .collect::<Vec<_>>();
    let (selected_checks, selected_checks_source) = effective_selected_checks(fold, &observed);
    let mut covered_language_packs = selected_checks
        .iter()
        .filter_map(|check| proof_language_pack(check).map(str::to_string))
        .collect::<Vec<_>>();
    covered_language_packs.sort();
    covered_language_packs.dedup();

    let uncovered_language_packs = impacted_language_packs
        .iter()
        .filter(|pack| {
            !covered_language_packs
                .iter()
                .any(|covered| covered == *pack)
        })
        .cloned()
        .collect::<Vec<_>>();

    let status = if impacted_language_packs.is_empty() {
        "not_applicable"
    } else if covered_language_packs.is_empty() {
        "uncovered"
    } else if uncovered_language_packs.is_empty() {
        "covered"
    } else {
        "partial"
    };
    let summary = match status {
        "covered" => format!(
            "Proof scope covered: selected checks map to the touched stack(s) ({})",
            impacted_language_packs.join(", ")
        ),
        "partial" => format!(
            "Proof scope partial: selected checks cover {}; touched stack(s) still need review: {}",
            covered_language_packs.join(", "),
            uncovered_language_packs.join(", ")
        ),
        "uncovered" => format!(
            "Proof scope uncovered: selected checks do not map to touched stack(s) ({})",
            impacted_language_packs.join(", ")
        ),
        _ => "Proof scope not applicable: no reviewable source language-pack impact was detected"
            .to_string(),
    };

    serde_json::json!({
        "generated_from": "diff.language_pack_impact+selected_checks+language_pack_suggested_checks",
        "status": status,
        "summary": summary,
        "touched_language_packs": impacted_language_packs,
        "covered_language_packs": covered_language_packs,
        "uncovered_language_packs": uncovered_language_packs,
        "selected_checks": selected_checks,
        "selected_checks_source": selected_checks_source,
    })
}

fn is_behavior_proof_path(path: &str) -> bool {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    lower.contains("/test/")
        || lower.starts_with("test/")
        || lower.contains("/tests/")
        || lower.starts_with("tests/")
        || lower.contains("/spec/")
        || lower.starts_with("spec/")
        || lower.contains("/specs/")
        || lower.starts_with("specs/")
        || lower.contains("/__tests__/")
        || lower.starts_with("__tests__/")
        || lower.contains("/fixtures/")
        || lower.starts_with("fixtures/")
        || lower.contains("/fixture/")
        || lower.starts_with("fixture/")
        || lower.contains("/src/test/")
        || lower.contains("/src/androidtest/")
        || lower.contains("/src/uitest/")
        || lower.contains("/src/uitests/")
        || base.contains(".test.")
        || base.contains(".spec.")
        || base.ends_with("test.swift")
        || base.ends_with("tests.swift")
        || base.ends_with("test.kt")
        || base.ends_with("tests.kt")
        || base.ends_with("test.java")
        || base.ends_with("tests.java")
        || base.ends_with("test.cs")
        || base.ends_with("tests.cs")
        || base.ends_with("_test.go")
        || base.starts_with("test_")
        || base.ends_with("_test.py")
}

fn behavior_proof(
    fold: &RunFold,
    proof_coverage: &serde_json::Value,
    real_change_count: usize,
    behavior_proof_paths: &[String],
) -> serde_json::Value {
    let proof_status = proof_coverage["status"].as_str().unwrap_or("not_recorded");
    let related_tests_observed = fold
        .semantic_query_operations_observed
        .iter()
        .any(|operation| operation == "related_tests");
    let source_diff_observed = real_change_count > 0;
    let behavior_diff_observed = !behavior_proof_paths.is_empty();
    let check_coverage_satisfied = proof_status == "satisfied";

    let status = if fold.status == "no_change" || !source_diff_observed {
        "gap"
    } else if check_coverage_satisfied && (behavior_diff_observed || related_tests_observed) {
        "covered"
    } else if check_coverage_satisfied {
        "weak"
    } else {
        "gap"
    };
    let summary = match status {
        "covered" => {
            let mut evidence = Vec::new();
            if behavior_diff_observed {
                evidence.push("changed test or fixture files");
            }
            if related_tests_observed {
                evidence.push("related-test semantic orientation");
            }
            format!(
                "Behavior proof covered: selected checks passed after source changes, with {}.",
                evidence.join(" and ")
            )
        }
        "weak" => {
            "Behavior proof is weak: selected checks passed after source changes, but Review did not see a test/fixture diff or related-test orientation.".to_string()
        }
        _ if fold.status == "no_change" => {
            "Behavior proof gap: the run changed no source files, so passing checks only prove baseline health.".to_string()
        }
        _ if !source_diff_observed => {
            "Behavior proof gap: Review saw no source diff to connect to the requested behavior.".to_string()
        }
        _ => {
            "Behavior proof gap: selected checks did not establish the changed behavior.".to_string()
        }
    };

    serde_json::json!({
        "generated_from": "diff+proof_coverage+semantic_related_tests",
        "status": status,
        "summary": summary,
        "source_diff_observed": source_diff_observed,
        "behavior_diff_observed": behavior_diff_observed,
        "behavior_proof_paths": behavior_proof_paths,
        "related_tests_observed": related_tests_observed,
        "check_coverage_status": proof_status,
        "check_coverage_satisfied": check_coverage_satisfied,
        "grants_execution_authority": false,
    })
}

fn principal_review(context: PrincipalReviewContext<'_>) -> serde_json::Value {
    let PrincipalReviewContext {
        fold,
        real_change_count,
        artifact_count,
        impacted_language_packs,
        proof_coverage,
        proof_scope,
        behavior_proof,
        standards_obligations,
    } = context;
    let mut checklist: Vec<serde_json::Value> = Vec::new();
    let proof_status = proof_coverage["status"].as_str().unwrap_or("not_recorded");
    let check_status = if proof_status == "satisfied" {
        "satisfied"
    } else {
        "attention"
    };
    checklist.push(serde_json::json!({
        "kind": "checks",
        "status": check_status,
        "line": proof_coverage["summary"].as_str().unwrap_or("Proof coverage unavailable"),
        "evidence": proof_coverage,
    }));
    let proof_scope_status = proof_scope["status"].as_str().unwrap_or("not_applicable");
    checklist.push(serde_json::json!({
        "kind": "checks",
        "status": if matches!(proof_scope_status, "covered" | "not_applicable") { "satisfied" } else { "attention" },
        "line": proof_scope["summary"].as_str().unwrap_or("Proof scope unavailable"),
        "evidence": proof_scope,
    }));
    let behavior_status = behavior_proof["status"].as_str().unwrap_or("gap");
    checklist.push(serde_json::json!({
        "kind": "behavior_proof",
        "status": if behavior_status == "covered" { "satisfied" } else { "attention" },
        "line": behavior_proof["summary"].as_str().unwrap_or("Behavior proof unavailable"),
        "evidence": behavior_proof,
    }));

    if !fold.repo_intake_generated_from.is_empty() {
        checklist.push(serde_json::json!({
            "kind": "repo_intake",
            "status": if fold.repo_intake_grants_execution_authority || fold.repo_intake_provider_calls_allowed || fold.repo_intake_network_call_allowed || fold.repo_intake_production_write_allowed { "attention" } else { "open" },
            "line": format!(
                "Repo intake before execution: readiness={}, scout={}, proof_strategy={}",
                empty_as_unknown(&fold.repo_intake_readiness_status),
                empty_as_unknown(&fold.repo_intake_scout_status),
                empty_as_unknown(&fold.repo_intake_proof_strategy)
            ),
            "evidence": repo_intake_json(fold),
        }));
    }

    if !fold.mission_id.trim().is_empty() {
        checklist.push(serde_json::json!({
            "kind": "mission_checkpoint",
            "status": if fold.mission_checkpoint_grants_execution_authority { "attention" } else { "open" },
            "line": format!(
                "Mission checkpoint target: {} / {}.",
                fold.mission_id,
                empty_as_unknown(&fold.mission_milestone_id)
            ),
            "evidence": mission_attachment_json(fold),
        }));
    }

    for axis in &fold.repo_profile_review_axes {
        checklist.push(serde_json::json!({
            "kind": "review_axis",
            "status": "open",
            "line": format!("Review against language-pack axis: {axis}"),
            "evidence": {
                "language_pack_id": fold.language_pack_id,
                "axis": axis,
            },
        }));
    }

    for gate in &fold.repo_profile_principal_review_gates {
        checklist.push(serde_json::json!({
            "kind": "principal_review_gate",
            "status": "open",
            "line": format!("Principal review gate: {gate}"),
            "evidence": {
                "language_pack_id": fold.language_pack_id,
                "gate": gate,
            },
        }));
    }

    for guidance in &fold.repo_profile_language_pack_guidance {
        checklist.push(serde_json::json!({
            "kind": "language_pack_guidance",
            "status": "open",
            "line": format!("Language pack guidance: {guidance}"),
            "evidence": {
                "language_pack_id": fold.language_pack_id,
                "guidance": guidance,
            },
        }));
    }

    if !fold.repo_profile_semantic_intelligence.is_empty() {
        checklist.push(serde_json::json!({
            "kind": "semantic_intelligence",
            "status": "open",
            "line": format!(
                "Semantic context plan: {}",
                fold.repo_profile_semantic_intelligence.join(", ")
            ),
            "evidence": {
                "language_pack_id": fold.language_pack_id,
                "semantic_intelligence": fold.repo_profile_semantic_intelligence.clone(),
            },
        }));
    }

    if !fold.repo_profile_semantic_session_id.trim().is_empty() {
        checklist.push(serde_json::json!({
            "kind": "semantic_session",
            "status": if fold.repo_profile_semantic_indexed_file_count > 0 {
                "satisfied"
            } else {
                "attention"
            },
            "line": format!(
                "Semantic session indexed {} files, {} symbols, and {} related-test anchors before review.",
                fold.repo_profile_semantic_indexed_file_count,
                fold.repo_profile_semantic_indexed_symbol_count,
                fold.repo_profile_semantic_related_test_anchor_count
            ),
            "evidence": {
                "session_id": fold.repo_profile_semantic_session_id,
                "fallback_index_status": fold.repo_profile_semantic_fallback_index_status,
                "source_file_count": fold.repo_profile_semantic_source_file_count,
                "indexed_file_count": fold.repo_profile_semantic_indexed_file_count,
                "indexed_symbol_count": fold.repo_profile_semantic_indexed_symbol_count,
                "related_test_anchor_count": fold.repo_profile_semantic_related_test_anchor_count,
                "grants_execution_authority": fold.repo_profile_semantic_session_grants_execution_authority,
            },
        }));
    }

    if fold.principal_orientation_gate_required {
        let gate_tool = principal_orientation_gate_tool(fold);
        checklist.push(serde_json::json!({
            "kind": "principal_orientation_gate",
            "status": if fold.principal_orientation_gate_observed { "satisfied" } else { "attention" },
            "line": if fold.principal_orientation_gate_observed {
                format!("Principal orientation gate satisfied: worker called {gate_tool} before trusted review.")
            } else {
                format!("Principal orientation gate required: worker must call {gate_tool} before editing, checking, or finishing.")
            },
            "evidence": {
                "language_pack_id": fold.language_pack_id,
                "required": true,
                "tool": gate_tool,
                "observed": fold.principal_orientation_gate_observed,
                "grants_authority": fold.principal_orientation_gate_grants_authority,
            },
        }));
    }

    if fold.semantic_preflight_anchor_count > 0 {
        checklist.push(serde_json::json!({
            "kind": "semantic_preflight",
            "status": "satisfied",
            "line": format!(
                "Forge preflighted {} semantic anchors before worker turns: {}.",
                fold.semantic_preflight_anchor_count,
                display_list(&fold.semantic_preflight_observed_operations)
            ),
            "evidence": {
                "status": fold.semantic_preflight_status,
                "session_id": fold.semantic_preflight_session_id,
                "anchor_count": fold.semantic_preflight_anchor_count,
                "planned_operations": fold.semantic_preflight_planned_operations,
                "observed_operations": fold.semantic_preflight_observed_operations,
                "indexed_file_count": fold.semantic_preflight_indexed_file_count,
                "indexed_symbol_count": fold.semantic_preflight_indexed_symbol_count,
                "related_test_anchor_count": fold.semantic_preflight_related_test_anchor_count,
                "grants_authority": fold.semantic_preflight_grants_execution_authority,
            },
        }));
    }

    if !fold.semantic_query_operations_observed.is_empty() {
        let related_tests_observed = fold
            .semantic_query_operations_observed
            .iter()
            .any(|operation| operation == "related_tests");
        checklist.push(serde_json::json!({
            "kind": "semantic_query_operations",
            "status": if related_tests_observed { "satisfied" } else { "open" },
            "line": if related_tests_observed {
                format!(
                    "Semantic orientation included related-test discovery: {}",
                    fold.semantic_query_operations_observed.join(", ")
                )
            } else {
                format!(
                    "Semantic orientation observed without related-test discovery: {}",
                    fold.semantic_query_operations_observed.join(", ")
                )
            },
            "evidence": {
                "operations": fold.semantic_query_operations_observed.clone(),
                "related_tests_observed": related_tests_observed,
                "grants_authority": false,
            },
        }));
    }

    if !fold.repo_profile_semantic_tool_readiness.is_empty() {
        let missing = fold
            .repo_profile_semantic_tool_readiness
            .iter()
            .any(|item| item.ends_with("=missing"));
        checklist.push(serde_json::json!({
            "kind": "semantic_tool_readiness",
            "status": if missing { "attention" } else { "satisfied" },
            "line": if missing {
                format!(
                    "Semantic tooling has missing prerequisite(s); Forge can fall back to anchored search: {}",
                    fold.repo_profile_semantic_tool_readiness.join(", ")
                )
            } else {
                format!(
                    "Semantic tooling appears ready for this language pack: {}",
                    fold.repo_profile_semantic_tool_readiness.join(", ")
                )
            },
            "evidence": {
                "language_pack_id": fold.language_pack_id,
                "semantic_tool_readiness": fold.repo_profile_semantic_tool_readiness.clone(),
                "grants_execution_authority": false,
            },
        }));
    }

    if !fold.repo_profile_toolchain_readiness.is_empty() {
        let missing = fold
            .repo_profile_toolchain_readiness
            .iter()
            .any(|item| item.ends_with("=missing"));
        checklist.push(serde_json::json!({
            "kind": "toolchain_readiness",
            "status": if missing { "attention" } else { "satisfied" },
            "line": if missing {
                format!(
                    "Local proof toolchain has missing prerequisite(s): {}",
                    fold.repo_profile_toolchain_readiness.join(", ")
                )
            } else {
                format!(
                    "Local proof toolchain appears ready for this language pack: {}",
                    fold.repo_profile_toolchain_readiness.join(", ")
                )
            },
            "evidence": {
                "language_pack_id": fold.language_pack_id,
                "toolchain_readiness": fold.repo_profile_toolchain_readiness.clone(),
            },
        }));
    }

    if !fold.repo_profile_proof_plan_status.is_empty() {
        checklist.push(serde_json::json!({
            "kind": "toolchain_readiness",
            "status": if fold.repo_profile_proof_plan_status == "ready" { "satisfied" } else { "attention" },
            "line": format!(
                "Proof plan is {}: {}",
                fold.repo_profile_proof_plan_status,
                fold.repo_profile_proof_plan_summary
            ),
            "evidence": {
                "status": fold.repo_profile_proof_plan_status,
                "next_action": fold.repo_profile_proof_plan_next_action,
                "summary": fold.repo_profile_proof_plan_summary,
            },
        }));
    }

    if !fold.work_classification_task_class.is_empty() {
        checklist.push(serde_json::json!({
            "kind": "work_classification",
            "status": if fold.work_classification_grants_execution_authority { "attention" } else { "open" },
            "line": format!(
                "Ask classified as {} {} with {}% confidence; use this only as review and eval context.",
                empty_as_unknown(&fold.work_classification_complexity_tier),
                empty_as_unknown(&fold.work_classification_task_class),
                fold.work_classification_confidence_pct
            ),
            "evidence": {
                "recommended_shape": fold.work_classification_recommended_shape,
                "task_class": fold.work_classification_task_class,
                "complexity_tier": fold.work_classification_complexity_tier,
                "confidence_pct": fold.work_classification_confidence_pct,
                "rationale": fold.work_classification_rationale,
                "source": fold.work_classification_source,
                "grants_execution_authority": fold.work_classification_grants_execution_authority,
            },
        }));
    }

    if !fold.language_task_corpus_id.is_empty() || !fold.language_task_alignment_status.is_empty() {
        checklist.push(serde_json::json!({
            "kind": "language_task_alignment",
            "status": if fold.language_task_alignment_grants_execution_authority { "attention" } else { "open" },
            "line": if fold.language_task_corpus_id.is_empty() {
                format!(
                    "No language task corpus row aligned for this run: {}.",
                    empty_as_unknown(&fold.language_task_alignment_status)
                )
            } else {
                format!(
                    "Comparable eval task: {} ({} {}).",
                    fold.language_task_corpus_id,
                    empty_as_unknown(&fold.language_task_complexity_tier),
                    empty_as_unknown(&fold.language_task_class)
                )
            },
            "evidence": {
                "task_corpus_id": fold.language_task_corpus_id,
                "status": fold.language_task_alignment_status,
                "source": fold.language_task_alignment_source,
                "task_class": fold.language_task_class,
                "complexity_tier": fold.language_task_complexity_tier,
                "visible_check": fold.language_task_visible_check,
                "hidden_check_slot": fold.language_task_hidden_check_slot,
                "artifact_noise_expected": fold.language_task_artifact_noise_expected,
                "required_principal_review_gates": fold.language_task_required_principal_review_gates,
                "engineering_facets": fold.language_task_engineering_facets,
                "evaluation_oracle": fold.language_task_evaluation_oracle,
                "human_timebox_minutes": fold.language_task_human_timebox_minutes,
                "contamination_policy": fold.language_task_contamination_policy,
                "grants_execution_authority": fold.language_task_alignment_grants_execution_authority,
            },
        }));
    }

    if !fold.repo_profile_standards_sources.is_empty() {
        checklist.push(serde_json::json!({
            "kind": "standards_sources",
            "status": "open",
            "line": format!(
                "Check repo standards sources before approving: {}",
                fold.repo_profile_standards_sources.join(", ")
            ),
            "evidence": {
                "standards_sources": fold.repo_profile_standards_sources.clone(),
                "standards_source_fingerprints": fold.repo_profile_standards_source_fingerprints.clone(),
                "standards_source_summaries": fold.repo_profile_standards_source_summaries.clone(),
            },
        }));
    }

    if !standards_obligations.is_empty() {
        checklist.push(serde_json::json!({
            "kind": "standards_obligations",
            "status": "open",
            "line": format!(
                "Verify {} standards obligation(s); the worker must read each source before claiming compliance.",
                standards_obligations.len()
            ),
            "evidence": {
                "generated_from": "standards_source_summaries",
                "obligations": standards_obligations,
                "raw_standards_content_recorded": false,
                "grants_authority": false,
            },
        }));
    }

    if fold.repo_profile_detected_language_packs.len() > 1 {
        checklist.push(serde_json::json!({
            "kind": "language_pack_guidance",
            "status": "open",
            "line": format!(
                "Monorepo shape detected: primary pack {} with additional pack evidence {}.",
                empty_as_unknown(&fold.language_pack_id),
                fold.repo_profile_detected_language_packs.join(", ")
            ),
            "evidence": {
                "language_pack_id": fold.language_pack_id,
                "detected_language_packs": fold.repo_profile_detected_language_packs.clone(),
            },
        }));
    }

    if !impacted_language_packs.is_empty() {
        checklist.push(serde_json::json!({
            "kind": "language_pack_guidance",
            "status": "open",
            "line": format!(
                "Diff touches language pack(s): {}.",
                impacted_language_packs.join(", ")
            ),
            "evidence": {
                "language_pack_id": fold.language_pack_id,
                "diff_language_packs": impacted_language_packs,
            },
        }));
    }

    checklist.push(serde_json::json!({
        "kind": "diff_shape",
        "status": if real_change_count > 0 { "open" } else { "attention" },
        "line": format!("Review {real_change_count} source change file(s); generated and artifact noise is separated."),
        "evidence": {
            "real_change_count": real_change_count,
            "artifact_count": artifact_count,
        },
    }));
    checklist.push(serde_json::json!({
        "kind": "artifact_noise",
        "status": "satisfied",
        "line": if artifact_count > 0 {
            format!("{artifact_count} known build artifact file(s) folded out of the first-read diff.")
        } else {
            "No known build artifact files detected in this diff.".to_string()
        },
        "evidence": {
            "artifact_count": artifact_count,
            "artifact_patterns": fold.repo_profile_artifact_patterns.clone(),
        },
    }));
    checklist.push(serde_json::json!({
        "kind": "authority",
        "status": if fold.repo_profile_grants_execution_authority || fold.suggested_checks_are_authority {
            "attention"
        } else {
            "satisfied"
        },
        "line": "Confirm repo profiling and suggested checks did not grant authority or mark the run passed.",
        "evidence": {
            "repo_profile_grants_execution_authority": fold.repo_profile_grants_execution_authority,
            "suggested_checks_are_authority": fold.suggested_checks_are_authority,
        },
    }));

    let handoff_lines = vec![
        format!(
            "Language pack: {}{}",
            empty_as_unknown(&fold.language_pack_id),
            if fold.repo_primary_language.is_empty() {
                String::new()
            } else {
                format!(" ({})", fold.repo_primary_language)
            }
        ),
        format!(
            "Selected checks: {}",
            if fold.selected_checks.is_empty() {
                "none recorded".to_string()
            } else {
                fold.selected_checks.join(", ")
            }
        ),
        format!(
            "Selected checks source: {}",
            selected_checks_source(&fold.selected_checks_source).replace('_', " ")
        ),
        format!(
            "Work classification: {} {} -> {} ({}% confidence)",
            empty_as_unknown(&fold.work_classification_complexity_tier),
            empty_as_unknown(&fold.work_classification_task_class),
            empty_as_unknown(&fold.work_classification_recommended_shape),
            fold.work_classification_confidence_pct
        ),
        format!(
            "Comparable language task: {} ({}, {})",
            empty_as_unknown(&fold.language_task_corpus_id),
            empty_as_unknown(&fold.language_task_class),
            empty_as_unknown(&fold.language_task_complexity_tier)
        ),
        format!(
            "Proof coverage: {}",
            proof_coverage["summary"]
                .as_str()
                .unwrap_or("Proof coverage unavailable")
        ),
        format!(
            "Proof match policy: {}",
            proof_coverage["match_policy"]
                .as_str()
                .unwrap_or("exact_selected_command")
                .replace('_', " ")
        ),
        format!(
            "Satisfied proof: {}",
            display_json_string_list(&proof_coverage["satisfied_checks"])
        ),
        format!(
            "Missing proof: {}",
            display_json_string_list(&proof_coverage["missing_checks"])
        ),
        format!(
            "Failed proof: {}",
            display_json_string_list(&proof_coverage["failed_checks"])
        ),
        format!(
            "Proof scope: {}",
            proof_scope["summary"]
                .as_str()
                .unwrap_or("Proof scope unavailable")
        ),
        format!(
            "Review axes: {}",
            if fold.repo_profile_review_axes.is_empty() {
                "none recorded".to_string()
            } else {
                fold.repo_profile_review_axes.join(", ")
            }
        ),
        format!(
            "Principal review gates: {}",
            if fold.repo_profile_principal_review_gates.is_empty() {
                "none recorded".to_string()
            } else {
                fold.repo_profile_principal_review_gates.join(", ")
            }
        ),
        format!(
            "Language pack guidance: {}",
            if fold.repo_profile_language_pack_guidance.is_empty() {
                "none recorded".to_string()
            } else {
                fold.repo_profile_language_pack_guidance.join(", ")
            }
        ),
        format!(
            "Semantic intelligence: {}",
            if fold.repo_profile_semantic_intelligence.is_empty() {
                "none recorded".to_string()
            } else {
                fold.repo_profile_semantic_intelligence.join(", ")
            }
        ),
        format!(
            "Principal orientation gate: {}",
            display_principal_orientation_gate(fold)
        ),
        format!(
            "Semantic query operations observed: {}",
            display_semantic_query_operations(fold)
        ),
        format!(
            "Semantic tool readiness: {}",
            if fold.repo_profile_semantic_tool_readiness.is_empty() {
                "none recorded".to_string()
            } else {
                fold.repo_profile_semantic_tool_readiness.join(", ")
            }
        ),
        format!(
            "Toolchain readiness: {}",
            if fold.repo_profile_toolchain_readiness.is_empty() {
                "none recorded".to_string()
            } else {
                fold.repo_profile_toolchain_readiness.join(", ")
            }
        ),
        format!(
            "Standards sources: {}",
            if fold.repo_profile_standards_sources.is_empty() {
                "none recorded".to_string()
            } else {
                fold.repo_profile_standards_sources.join(", ")
            }
        ),
        format!(
            "Standards source fingerprints: {}",
            if fold.repo_profile_standards_source_fingerprints.is_empty() {
                "none recorded".to_string()
            } else {
                fold.repo_profile_standards_source_fingerprints.join(", ")
            }
        ),
        format!(
            "Standards summaries: {}",
            if fold.repo_profile_standards_source_summaries.is_empty() {
                "none recorded".to_string()
            } else {
                fold.repo_profile_standards_source_summaries.join(", ")
            }
        ),
        format!(
            "Standards obligations: {}",
            display_standards_obligations(standards_obligations)
        ),
        format!(
            "Diff shape: {real_change_count} source file(s), {artifact_count} artifact file(s) folded"
        ),
        format!(
            "Diff language packs: {}",
            if impacted_language_packs.is_empty() {
                "none inferred".to_string()
            } else {
                impacted_language_packs.join(", ")
            }
        ),
    ];

    serde_json::json!({
        "generated_from": "repo_intelligence+checks+diff",
        "checklist": checklist,
        "handoff_lines": handoff_lines,
        "authority_note": "Repo intelligence improves planning and review only. It does not grant execution authority, replace checks, or mark work passed.",
    })
}

fn display_standards_obligations(obligations: &[serde_json::Value]) -> String {
    if obligations.is_empty() {
        return "none recorded".to_string();
    }
    obligations
        .iter()
        .map(|obligation| {
            let source = obligation["source"].as_str().unwrap_or("unknown source");
            let axis = obligation["review_axis"]
                .as_str()
                .unwrap_or("standards obligation");
            let tag = obligation["tag"].as_str().unwrap_or("unknown_tag");
            format!("{source}: {axis} ({tag})")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn display_principal_orientation_gate(fold: &RunFold) -> String {
    if !fold.principal_orientation_gate_required {
        return "not required".to_string();
    }
    let observed = if fold.principal_orientation_gate_observed {
        "observed"
    } else {
        "not observed"
    };
    format!(
        "required; tool={}; {observed}; grants_authority=false",
        principal_orientation_gate_tool(fold)
    )
}

fn display_semantic_query_operations(fold: &RunFold) -> String {
    if fold.semantic_query_operations_observed.is_empty() {
        "none observed".to_string()
    } else {
        fold.semantic_query_operations_observed.join(", ")
    }
}

#[allow(clippy::too_many_arguments)]
fn pr_handoff(
    run_id: &str,
    fold: &RunFold,
    review_status: &str,
    next_move: &str,
    real_change_count: usize,
    artifact_count: usize,
    impacted_language_packs: &[String],
    artifact_summary: &[serde_json::Value],
    principal_review: &serde_json::Value,
    proof_coverage: &serde_json::Value,
    proof_scope: &serde_json::Value,
    standards_obligations: &[serde_json::Value],
    candidate_comparison: &serde_json::Value,
) -> serde_json::Value {
    let language_pack = empty_as_unknown(&fold.language_pack_id);
    let primary_language = empty_as_unknown(&fold.repo_primary_language);
    let selected_checks = display_list(&fold.selected_checks);
    let selected_checks_source =
        selected_checks_source(&fold.selected_checks_source).replace('_', " ");
    let standards_sources = display_list(&fold.repo_profile_standards_sources);
    let title = format!("Forge run {run_id}: {review_status}");
    let mut review_checklist = principal_review["checklist"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item["line"].as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let candidate_recommendation = candidate_recommendation_json(candidate_comparison);
    if candidate_recommendation["candidate_count"]
        .as_u64()
        .unwrap_or(1)
        > 1
    {
        review_checklist.push(candidate_recommendation_checklist_line(
            &candidate_recommendation,
        ));
    }
    let summary_lines = vec![
        format!("Review status: {review_status}"),
        format!("Next move: {next_move}"),
        candidate_recommendation_summary_line(&candidate_recommendation),
        format!("Language pack: {language_pack} ({primary_language})"),
        format!("Selected checks: {selected_checks}"),
        format!("Selected checks source: {selected_checks_source}"),
        format!(
            "Proof coverage: {}",
            proof_coverage["summary"]
                .as_str()
                .unwrap_or("Proof coverage unavailable")
        ),
        format!(
            "Proof match policy: {}",
            proof_coverage["match_policy"]
                .as_str()
                .unwrap_or("exact_selected_command")
                .replace('_', " ")
        ),
        format!(
            "Satisfied proof: {}",
            display_json_string_list(&proof_coverage["satisfied_checks"])
        ),
        format!(
            "Missing proof: {}",
            display_json_string_list(&proof_coverage["missing_checks"])
        ),
        format!(
            "Failed proof: {}",
            display_json_string_list(&proof_coverage["failed_checks"])
        ),
        format!(
            "Proof scope: {}",
            proof_scope["summary"]
                .as_str()
                .unwrap_or("Proof scope unavailable")
        ),
        format!(
            "Repo intake: readiness={}, scout={}, proof_strategy={}",
            empty_as_unknown(&fold.repo_intake_readiness_status),
            empty_as_unknown(&fold.repo_intake_scout_status),
            empty_as_unknown(&fold.repo_intake_proof_strategy)
        ),
        format!(
            "Repo intake semantic orientation: {}",
            display_list(&fold.repo_intake_semantic_orientation_operations)
        ),
        format!(
            "Repo intake write scope hint: {}",
            display_list(&fold.repo_intake_write_scope_hint)
        ),
        format!(
            "Repo intake source host: {}",
            empty_as_unknown(&fold.repo_intake_source_host)
        ),
        format!(
            "Principal review gates: {}",
            display_list(&fold.repo_profile_principal_review_gates)
        ),
        format!(
            "Language pack guidance: {}",
            display_list(&fold.repo_profile_language_pack_guidance)
        ),
        format!(
            "Semantic intelligence: {}",
            display_list(&fold.repo_profile_semantic_intelligence)
        ),
        format!(
            "Principal orientation gate: {}",
            display_principal_orientation_gate(fold)
        ),
        format!(
            "Semantic query operations observed: {}",
            display_semantic_query_operations(fold)
        ),
        format!(
            "Semantic tool readiness: {}",
            display_list(&fold.repo_profile_semantic_tool_readiness)
        ),
        format!(
            "Toolchain readiness: {}",
            display_list(&fold.repo_profile_toolchain_readiness)
        ),
        format!("Standards sources: {standards_sources}"),
        format!(
            "Standards fingerprints: {}",
            display_list(&fold.repo_profile_standards_source_fingerprints)
        ),
        format!(
            "Standards summaries: {}",
            display_list(&fold.repo_profile_standards_source_summaries)
        ),
        format!(
            "Standards obligations: {}",
            display_standards_obligations(standards_obligations)
        ),
        format!("Diff shape: {real_change_count} source file(s), {artifact_count} artifact file(s) folded"),
        format!(
            "Diff language packs: {}",
            display_list(impacted_language_packs)
        ),
        format!(
            "Artifact noise: {}",
            display_artifact_summary(artifact_summary)
        ),
        "Authority: review handoff only; no production write, push, PR open, or approval authority.".to_string(),
    ];
    let body_markdown = render_pr_handoff_body(
        run_id,
        fold,
        review_status,
        next_move,
        real_change_count,
        artifact_count,
        impacted_language_packs,
        artifact_summary,
        &review_checklist,
        proof_coverage,
        proof_scope,
        standards_obligations,
        &candidate_recommendation,
    );

    serde_json::json!({
        "generated_from": "review_packet+repo_intelligence+diff",
        "title": title,
        "body_markdown": body_markdown,
        "summary_lines": summary_lines,
        "review_checklist": review_checklist,
        "candidate_recommendation": candidate_recommendation,
        "review_packet_route": "/forge/review-packet.json",
        "production_write_allowed": false,
    })
}

fn candidate_recommendation_json(candidate_comparison: &serde_json::Value) -> serde_json::Value {
    let current_run_id = candidate_comparison["current_run_id"]
        .as_str()
        .unwrap_or("");
    let recommended_run_id = candidate_comparison["recommended_run_id"]
        .as_str()
        .unwrap_or("");
    let candidate_count = candidate_comparison["candidate_count"]
        .as_u64()
        .unwrap_or(1);
    let current_rank = candidate_comparison["current_rank"].as_u64().unwrap_or(1);
    let current_run_is_recommended = candidate_count <= 1
        || recommended_run_id.trim().is_empty()
        || current_run_id == recommended_run_id;
    let current_score = candidate_comparison["candidates"]
        .as_array()
        .and_then(|candidates| {
            candidates.iter().find_map(|candidate| {
                if candidate["run_id"].as_str() == Some(current_run_id) {
                    candidate["score"].as_i64()
                } else {
                    None
                }
            })
        });
    let recommended_score = candidate_comparison["candidates"]
        .as_array()
        .and_then(|candidates| {
            candidates.iter().find_map(|candidate| {
                if candidate["run_id"].as_str() == Some(recommended_run_id) {
                    candidate["score"].as_i64()
                } else {
                    None
                }
            })
        });
    serde_json::json!({
        "generated_from": "candidate_comparison",
        "candidate_count": candidate_count,
        "current_run_id": current_run_id,
        "recommended_run_id": recommended_run_id,
        "current_rank": current_rank,
        "current_run_is_recommended": current_run_is_recommended,
        "current_score": current_score,
        "recommended_score": recommended_score,
        "comparison_basis": candidate_comparison["comparison_basis"].clone(),
        "diff_quality_included": candidate_comparison["diff_quality_included"].as_bool().unwrap_or(false),
        "eval_evidence_included": candidate_comparison["eval_evidence_included"].as_bool().unwrap_or(false),
        "model_judgment_included": candidate_comparison["model_judgment_included"].as_bool().unwrap_or(false),
        "grants_execution_authority": false,
    })
}

struct PrincipalReviewContext<'a> {
    fold: &'a RunFold,
    real_change_count: usize,
    artifact_count: usize,
    impacted_language_packs: &'a [String],
    proof_coverage: &'a serde_json::Value,
    proof_scope: &'a serde_json::Value,
    behavior_proof: &'a serde_json::Value,
    standards_obligations: &'a [serde_json::Value],
}

fn candidate_recommendation_summary_line(candidate_recommendation: &serde_json::Value) -> String {
    let candidate_count = candidate_recommendation["candidate_count"]
        .as_u64()
        .unwrap_or(1);
    if candidate_count <= 1 {
        return "Candidate recommendation: single run; no fleet candidate choice required"
            .to_string();
    }
    let current_run_id = candidate_recommendation["current_run_id"]
        .as_str()
        .unwrap_or("unknown");
    let recommended_run_id = candidate_recommendation["recommended_run_id"]
        .as_str()
        .unwrap_or("unknown");
    let current_rank = candidate_recommendation["current_rank"]
        .as_u64()
        .unwrap_or(1);
    if candidate_recommendation["current_run_is_recommended"]
        .as_bool()
        .unwrap_or(false)
    {
        format!(
            "Candidate recommendation: current run {current_run_id} is recommended, rank {current_rank} of {candidate_count}"
        )
    } else {
        format!(
            "Candidate recommendation: use {recommended_run_id}; current run {current_run_id} is rank {current_rank} of {candidate_count}"
        )
    }
}

fn candidate_recommendation_checklist_line(candidate_recommendation: &serde_json::Value) -> String {
    let summary = candidate_recommendation_summary_line(candidate_recommendation);
    format!(
        "{summary}; comparison uses receipt facts only, not diff-quality or model-judgment scoring."
    )
}

#[allow(clippy::too_many_arguments)]
fn render_pr_handoff_body(
    run_id: &str,
    fold: &RunFold,
    review_status: &str,
    next_move: &str,
    real_change_count: usize,
    artifact_count: usize,
    impacted_language_packs: &[String],
    artifact_summary: &[serde_json::Value],
    review_checklist: &[String],
    proof_coverage: &serde_json::Value,
    proof_scope: &serde_json::Value,
    standards_obligations: &[serde_json::Value],
    candidate_recommendation: &serde_json::Value,
) -> String {
    let mut body = vec![
        "## Forge Review Handoff".to_string(),
        String::new(),
        format!("- Run: `{run_id}`"),
        format!("- Review status: `{review_status}`"),
        format!("- Next move: {next_move}"),
        format!(
            "- {}",
            candidate_recommendation_summary_line(candidate_recommendation)
        ),
        format!(
            "- Candidate comparison basis: {}; diff_quality_included={}; model_judgment_included={}; grants_authority=false",
            display_json_string_list(&candidate_recommendation["comparison_basis"]),
            candidate_recommendation["diff_quality_included"]
                .as_bool()
                .unwrap_or(false),
            candidate_recommendation["model_judgment_included"]
                .as_bool()
                .unwrap_or(false)
        ),
        format!("- Branch: `{}`", empty_as_unknown(&fold.branch)),
        format!(
            "- Language pack: `{}` ({})",
            empty_as_unknown(&fold.language_pack_id),
            empty_as_unknown(&fold.repo_primary_language)
        ),
        format!("- Selected checks: {}", display_list(&fold.selected_checks)),
        format!(
            "- Selected checks source: {}",
            selected_checks_source(&fold.selected_checks_source).replace('_', " ")
        ),
        format!(
            "- Proof coverage: {}",
            proof_coverage["summary"]
                .as_str()
                .unwrap_or("Proof coverage unavailable")
        ),
        format!(
            "- Proof match policy: {}",
            proof_coverage["match_policy"]
                .as_str()
                .unwrap_or("exact_selected_command")
                .replace('_', " ")
        ),
        format!(
            "- Satisfied proof: {}",
            display_json_string_list(&proof_coverage["satisfied_checks"])
        ),
        format!(
            "- Missing proof: {}",
            display_json_string_list(&proof_coverage["missing_checks"])
        ),
        format!(
            "- Failed proof: {}",
            display_json_string_list(&proof_coverage["failed_checks"])
        ),
        format!(
            "- Proof scope: {}",
            proof_scope["summary"]
                .as_str()
                .unwrap_or("Proof scope unavailable")
        ),
        format!(
            "- Repo intake: readiness={}, scout={}, proof_strategy={}",
            empty_as_unknown(&fold.repo_intake_readiness_status),
            empty_as_unknown(&fold.repo_intake_scout_status),
            empty_as_unknown(&fold.repo_intake_proof_strategy)
        ),
        format!(
            "- Repo intake semantic orientation: {}",
            display_list(&fold.repo_intake_semantic_orientation_operations)
        ),
        format!(
            "- Repo intake write scope hint: {}",
            display_list(&fold.repo_intake_write_scope_hint)
        ),
        format!(
            "- Repo intake source host: {}",
            empty_as_unknown(&fold.repo_intake_source_host)
        ),
        format!(
            "- Principal review gates: {}",
            display_list(&fold.repo_profile_principal_review_gates)
        ),
        format!(
            "- Language pack guidance: {}",
            display_list(&fold.repo_profile_language_pack_guidance)
        ),
        format!(
            "- Semantic intelligence: {}",
            display_list(&fold.repo_profile_semantic_intelligence)
        ),
        format!(
            "- Principal orientation gate: {}",
            display_principal_orientation_gate(fold)
        ),
        format!(
            "- Semantic query operations observed: {}",
            display_semantic_query_operations(fold)
        ),
        format!(
            "- Semantic tool readiness: {}",
            display_list(&fold.repo_profile_semantic_tool_readiness)
        ),
        format!(
            "- Toolchain readiness: {}",
            display_list(&fold.repo_profile_toolchain_readiness)
        ),
        format!(
            "- Standards sources: {}",
            display_list(&fold.repo_profile_standards_sources)
        ),
        format!(
            "- Standards fingerprints: {}",
            display_list(&fold.repo_profile_standards_source_fingerprints)
        ),
        format!(
            "- Standards summaries: {}",
            display_list(&fold.repo_profile_standards_source_summaries)
        ),
        format!(
            "- Standards obligations: {}",
            display_standards_obligations(standards_obligations)
        ),
        format!(
            "- Diff shape: {real_change_count} source file(s), {artifact_count} artifact file(s) folded"
        ),
        format!(
            "- Diff language packs: {}",
            display_list(impacted_language_packs)
        ),
        format!(
            "- Artifact noise: {}",
            display_artifact_summary(artifact_summary)
        ),
        String::new(),
        "### Review checklist".to_string(),
    ];
    if review_checklist.is_empty() {
        body.push("- [ ] No derived checklist items recorded.".to_string());
    } else {
        body.extend(review_checklist.iter().map(|line| format!("- [ ] {line}")));
    }
    body.extend([
        String::new(),
        "### Boundary".to_string(),
        "- Generated from the Forge Review Packet, repo intelligence, and diff evidence."
            .to_string(),
        "- This handoff does not push a branch, open a PR, approve work, or write to production."
            .to_string(),
        "- Full packet: `/forge/review-packet.json`".to_string(),
    ]);
    body.join("\n")
}

fn display_list(values: &[String]) -> String {
    if values.is_empty() {
        "none recorded".to_string()
    } else {
        values.join(", ")
    }
}

fn display_json_string_list(value: &serde_json::Value) -> String {
    let values = value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    display_list(&values)
}

fn display_artifact_summary(values: &[serde_json::Value]) -> String {
    let lines = values
        .iter()
        .filter_map(|value| {
            let reason = value["reason"].as_str()?;
            let count = value["count"].as_u64().unwrap_or(0);
            let added = value["added"].as_u64().unwrap_or(0);
            let removed = value["removed"].as_u64().unwrap_or(0);
            Some(format!("{reason}: {count} file(s), +{added}/-{removed}"))
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        "none detected".to_string()
    } else {
        lines.join("; ")
    }
}

fn repo_intake_json(fold: &RunFold) -> serde_json::Value {
    serde_json::json!({
        "generated_from": fold.repo_intake_generated_from,
        "readiness_status": fold.repo_intake_readiness_status,
        "medium_high_work_ready": fold.repo_intake_medium_high_work_ready,
        "safe_next_move": fold.repo_intake_safe_next_move,
        "semantic_orientation_operations": fold.repo_intake_semantic_orientation_operations,
        "proof_strategy": fold.repo_intake_proof_strategy,
        "first_run_task_ids": fold.repo_intake_first_run_task_ids,
        "scout_status": fold.repo_intake_scout_status,
        "scout_candidate_count": fold.repo_intake_scout_candidate_count,
        "scout_candidate_kinds": fold.repo_intake_scout_candidate_kinds,
        "scout_candidate_paths": fold.repo_intake_scout_candidate_paths,
        "write_scope_hint": fold.repo_intake_write_scope_hint,
        "off_limits_patterns": fold.repo_intake_off_limits_patterns,
        "review_focus": fold.repo_intake_review_focus,
        "source_host": fold.repo_intake_source_host,
        "origin_url_present": fold.repo_intake_origin_url_present,
        "origin_url_recorded": fold.repo_intake_origin_url_recorded,
        "source_host_readiness_route": fold.repo_intake_source_host_readiness_route,
        "source_host_pr_draft_route": fold.repo_intake_source_host_pr_draft_route,
        "provider_calls_allowed": fold.repo_intake_provider_calls_allowed,
        "run_started": fold.repo_intake_run_started,
        "network_call_allowed": fold.repo_intake_network_call_allowed,
        "production_write_allowed": fold.repo_intake_production_write_allowed,
        "grants_execution_authority": fold.repo_intake_grants_execution_authority,
    })
}

fn empty_as_unknown(value: &str) -> &str {
    if value.is_empty() { "unknown" } else { value }
}

fn principal_orientation_gate_tool(fold: &RunFold) -> &str {
    if fold.principal_orientation_gate_tool.trim().is_empty() {
        "semantic_query"
    } else {
        fold.principal_orientation_gate_tool.as_str()
    }
}

fn affected_language_pack(path: &str, detected_packs: &[String]) -> Option<String> {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    let has_pack = |pack: &str| detected_packs.iter().any(|candidate| candidate == pack);

    if lower.ends_with(".xcodeproj")
        || lower.ends_with(".xcworkspace")
        || lower.contains(".xcodeproj/")
        || lower.contains(".xcworkspace/")
    {
        return Some("ios-xcode".to_string());
    }
    if base == "package.swift" || lower.ends_with(".swift") {
        if has_pack("ios-xcode") && !has_pack("swift-spm") {
            return Some("ios-xcode".to_string());
        }
        return Some("swift-spm".to_string());
    }
    if has_pack("android-gradle")
        && (base == "androidmanifest.xml"
            || lower.contains("/src/main/res/")
            || lower.contains("/src/androidtest/")
            || (lower.contains("/src/test/")
                && (lower.ends_with(".kt") || lower.ends_with(".java"))))
    {
        return Some("android-gradle".to_string());
    }
    if base == "pom.xml" {
        return Some("java-maven".to_string());
    }
    if base == "settings.gradle"
        || base == "settings.gradle.kts"
        || base == "build.gradle"
        || base == "build.gradle.kts"
    {
        if has_pack("android-gradle") {
            return Some("android-gradle".to_string());
        }
        return Some("gradle-jvm".to_string());
    }
    if lower.ends_with(".kt") || lower.ends_with(".java") {
        if has_pack("android-gradle") {
            return Some("android-gradle".to_string());
        }
        if has_pack("gradle-jvm") {
            return Some("gradle-jvm".to_string());
        }
        return Some("java-maven".to_string());
    }
    if lower.ends_with(".cs")
        || lower.ends_with(".csproj")
        || lower.ends_with(".sln")
        || base == "global.json"
    {
        return Some("dotnet".to_string());
    }
    if base == "package.json"
        || lower.ends_with(".js")
        || lower.ends_with(".jsx")
        || lower.ends_with(".ts")
        || lower.ends_with(".tsx")
        || lower.ends_with(".mjs")
        || lower.ends_with(".cjs")
    {
        return Some("node".to_string());
    }
    if base == "cargo.toml" || lower.ends_with(".rs") {
        return Some("rust-cargo".to_string());
    }
    if base == "pyproject.toml" || base == "requirements.txt" || lower.ends_with(".py") {
        return Some("python".to_string());
    }
    if base == "go.mod" || lower.ends_with(".go") {
        return Some("go".to_string());
    }

    None
}

fn repo_intelligence_json(
    kernel: &MdxKernel,
    run_id: &str,
    fold: &RunFold,
    standards_obligations: &[serde_json::Value],
) -> serde_json::Value {
    serde_json::json!({
        "repo_id": fold.repo_id,
        "primary_language": fold.repo_primary_language,
        "language_pack_id": fold.language_pack_id,
        "detected_language_packs": fold.repo_profile_detected_language_packs,
        "detected_files": fold.repo_profile_detected_files,
        "quality_signals": fold.repo_profile_quality_signals,
        "standards_sources": fold.repo_profile_standards_sources,
        "standards_source_fingerprints": fold.repo_profile_standards_source_fingerprints,
        "standards_source_summaries": fold.repo_profile_standards_source_summaries,
        "standards_obligations": standards_obligations,
        "review_axes": fold.repo_profile_review_axes,
        "principal_review_gates": fold.repo_profile_principal_review_gates,
        "language_pack_guidance": fold.repo_profile_language_pack_guidance,
        "semantic_intelligence": fold.repo_profile_semantic_intelligence,
        "semantic_tool_readiness": fold.repo_profile_semantic_tool_readiness,
        "semantic_session": {
            "session_id": fold.repo_profile_semantic_session_id,
            "fallback_index_status": fold.repo_profile_semantic_fallback_index_status,
            "source_file_count": fold.repo_profile_semantic_source_file_count,
            "indexed_file_count": fold.repo_profile_semantic_indexed_file_count,
            "indexed_symbol_count": fold.repo_profile_semantic_indexed_symbol_count,
            "related_test_anchor_count": fold.repo_profile_semantic_related_test_anchor_count,
            "lsp_execution_status": lsp_execution_status_from_readiness(&fold.repo_profile_semantic_tool_readiness),
            "grants_execution_authority": fold.repo_profile_semantic_session_grants_execution_authority,
        },
        "toolchain_readiness": fold.repo_profile_toolchain_readiness,
        "suggested_checks": fold.repo_profile_suggested_checks,
        "artifact_patterns": fold.repo_profile_artifact_patterns,
        "proof_plan": {
            "status": fold.repo_profile_proof_plan_status,
            "next_action": fold.repo_profile_proof_plan_next_action,
            "summary": fold.repo_profile_proof_plan_summary,
        },
        "selected_checks": fold.selected_checks,
        "selected_checks_source": selected_checks_source(&fold.selected_checks_source),
        "profile_source": fold.repo_profile_source,
        "repo_profile_grants_execution_authority": fold.repo_profile_grants_execution_authority,
        "suggested_checks_are_authority": fold.suggested_checks_are_authority,
        "principal_orientation_gate": {
            "required": fold.principal_orientation_gate_required,
            "tool": principal_orientation_gate_tool(fold),
            "observed": fold.principal_orientation_gate_observed,
            "required_semantic_operations": fold.principal_orientation_required_semantic_operations,
            "semantic_strategy_required": fold.principal_orientation_semantic_strategy_required,
            "dependency_map_required": fold.principal_orientation_dependency_map_required,
            "grants_authority": fold.principal_orientation_gate_grants_authority,
        },
        "semantic_preflight": {
            "status": fold.semantic_preflight_status,
            "source": fold.semantic_preflight_source,
            "session_id": fold.semantic_preflight_session_id,
            "anchor_count": fold.semantic_preflight_anchor_count,
            "planned_operations": fold.semantic_preflight_planned_operations,
            "observed_operations": fold.semantic_preflight_observed_operations,
            "indexed_file_count": fold.semantic_preflight_indexed_file_count,
            "indexed_symbol_count": fold.semantic_preflight_indexed_symbol_count,
            "related_test_anchor_count": fold.semantic_preflight_related_test_anchor_count,
            "grants_execution_authority": fold.semantic_preflight_grants_execution_authority,
        },
        "semantic_query_operations_observed": fold.semantic_query_operations_observed.clone(),
        "related_tests_observed": fold
            .semantic_query_operations_observed
            .iter()
            .any(|operation| operation == "related_tests"),
        "builder_casting": builder_casting_json(fold),
        "fleet_stream": fleet_stream_json(fold),
        "fleet_integration": fleet_integration_json(fold),
        "fleet_selection": fleet_selection_json(kernel, run_id),
        "parallel_candidate": parallel_candidate_json(run_id, fold),
        "candidate_selection": candidate_selection_json(fold),
        "mission": mission_attachment_json(fold),
    })
}

fn candidate_selection_json(fold: &RunFold) -> serde_json::Value {
    let recorded = !fold.candidate_selection_id.trim().is_empty();
    serde_json::json!({
        "status": if recorded { "recorded" } else { "not_recorded" },
        "selection_id": fold.candidate_selection_id,
        "primary_run_id": fold.candidate_selection_primary_run_id,
        "selected_run_id": fold.candidate_selection_selected_run_id,
        "current_run_id": fold.candidate_selection_current_run_id,
        "current_run_is_selected": fold.candidate_selection_current_run_is_selected,
        "recommended_run_id": fold.candidate_selection_recommended_run_id,
        "matches_recommendation": fold.candidate_selection_matches_recommendation,
        "override_reason_recorded": fold.candidate_selection_override_reason_recorded,
        "selection_reason": fold.candidate_selection_reason,
        "override_recommendation_reason": fold.candidate_selection_override_recommendation_reason,
        "candidate_count": fold.candidate_selection_candidate_count,
        "actor_id": fold.candidate_selection_actor_id,
        "selection_grants_delivery_authority": fold.candidate_selection_grants_delivery_authority,
        "production_write_allowed": fold.candidate_selection_production_write_allowed,
    })
}

fn mission_attachment_json(fold: &RunFold) -> serde_json::Value {
    serde_json::json!({
        "attached": !fold.mission_id.trim().is_empty(),
        "mission_id": fold.mission_id,
        "milestone_id": fold.mission_milestone_id,
        "checkpoint_route": fold.mission_checkpoint_route,
        "checkpoint_grants_execution_authority": fold.mission_checkpoint_grants_execution_authority,
    })
}

fn builder_casting_json(fold: &RunFold) -> serde_json::Value {
    serde_json::json!({
        "status": fold.builder_casting_status,
        "requested_slot": fold.builder_casting_requested_slot,
        "selected_slot": fold.builder_casting_selected_slot,
        "recommended_slot": fold.builder_casting_recommended_slot,
        "selected_model_profile_id": fold.builder_casting_selected_model_profile_id,
        "selected_provider_family": fold.builder_casting_selected_provider_family,
        "selected_model_id": fold.builder_casting_selected_model_id,
        "recommended_model_profile_id": fold.builder_casting_recommended_model_profile_id,
        "recommended_provider_family": fold.builder_casting_recommended_provider_family,
        "recommended_model_id": fold.builder_casting_recommended_model_id,
        "basis": fold.builder_casting_basis,
        "matching_eval_score_count": fold.builder_casting_matching_eval_score_count,
        "accepted_eval_score_count": fold.builder_casting_accepted_eval_score_count,
        "matching_run_count": fold.builder_casting_matching_run_count,
        "done_rate_pct": fold.builder_casting_done_rate_pct,
        "requested_slot_matches_evidence": fold.builder_casting_requested_slot_matches_evidence,
        "grants_execution_authority": fold.builder_casting_grants_execution_authority,
    })
}

fn fleet_stream_json(fold: &RunFold) -> serde_json::Value {
    let role = if fold.fleet_stream_id.trim().is_empty() {
        "not_fleet_stream"
    } else {
        "stream"
    };
    serde_json::json!({
        "role": role,
        "stream_id": fold.fleet_stream_id,
        "strategy_id": fold.fleet_stream_semantic_strategy_id,
        "strategy_summary": fold.fleet_stream_semantic_strategy_summary,
        "required_semantic_operations": fold.fleet_stream_required_semantic_operations,
        "proof_bias": fold.fleet_stream_proof_bias,
        "strategy_semantic_operations_satisfied": missing_fleet_stream_semantic_operations(fold).is_empty(),
        "missing_required_semantic_operations": missing_fleet_stream_semantic_operations(fold),
        "grants_execution_authority": false,
    })
}

fn fleet_integration_json(fold: &RunFold) -> serde_json::Value {
    let role = if fold.fleet_integration_strategy_id.trim().is_empty() {
        "not_fleet_integration"
    } else {
        "integration"
    };
    serde_json::json!({
        "role": role,
        "strategy_id": fold.fleet_integration_strategy_id,
        "strategy_summary": fold.fleet_integration_strategy_summary,
        "required_semantic_operations": fold.fleet_integration_required_semantic_operations,
        "proof_bias": fold.fleet_integration_proof_bias,
        "strategy_semantic_operations_satisfied": missing_fleet_integration_semantic_operations(fold).is_empty(),
        "missing_required_semantic_operations": missing_fleet_integration_semantic_operations(fold),
        "grants_execution_authority": false,
    })
}

fn parallel_candidate_json(run_id: &str, fold: &RunFold) -> serde_json::Value {
    let primary_run_id = if fold.parallel_candidate_primary_run_id.trim().is_empty() {
        run_id
    } else {
        fold.parallel_candidate_primary_run_id.as_str()
    };
    let role = if primary_run_id != run_id {
        "candidate"
    } else if fold.parallel_candidate_count > 1 {
        "primary"
    } else {
        "single"
    };
    serde_json::json!({
        "role": role,
        "primary_run_id": primary_run_id,
        "index": fold.parallel_candidate_index.max(1),
        "count": fold.parallel_candidate_count.max(1),
        "write_scope": fold.parallel_candidate_write_scope.clone(),
        "strategy_id": candidate_strategy_id(
            &fold.parallel_candidate_strategy_id,
            fold.parallel_candidate_index,
            fold.parallel_candidate_count,
        ),
        "strategy_summary": candidate_strategy_summary(
            &fold.parallel_candidate_strategy_summary,
            fold.parallel_candidate_index,
            fold.parallel_candidate_count,
        ),
        "required_semantic_operations": candidate_required_semantic_operations(
            &fold.parallel_candidate_required_semantic_operations,
            fold.parallel_candidate_index,
            fold.parallel_candidate_count,
        ),
        "proof_bias": candidate_proof_bias(
            &fold.parallel_candidate_proof_bias,
            fold.parallel_candidate_index,
            fold.parallel_candidate_count,
        ),
        "grants_execution_authority": false,
    })
}

#[derive(Clone, Debug, Default)]
struct FleetLaneCandidate {
    fleet_id: String,
    stream_id: String,
    run_id: String,
    state: String,
    detail: String,
}

fn fleet_selection_json(kernel: &MdxKernel, run_id: &str) -> serde_json::Value {
    use std::collections::BTreeMap;
    let mut fleets: BTreeMap<String, Vec<FleetLaneCandidate>> = BTreeMap::new();
    for receipt in kernel.ledger().query().by_kind("fleet.run.event") {
        let event = payload(receipt, "event");
        if !matches!(
            event,
            "stream_started" | "stream_finished" | "stream_needs_attention"
        ) {
            continue;
        }
        let fleet_id = payload(receipt, "fleet_id");
        let stream_id = payload(receipt, "stream_id");
        if fleet_id.trim().is_empty() || stream_id.trim().is_empty() {
            continue;
        }
        let state = match event {
            "stream_started" => "working",
            "stream_finished" => "done",
            _ => "needs_attention",
        };
        let run = payload(receipt, "forge_run_id");
        let lanes = fleets.entry(fleet_id.to_string()).or_default();
        if let Some(lane) = lanes.iter_mut().find(|lane| lane.stream_id == stream_id) {
            lane.state = state.to_string();
            lane.detail = payload(receipt, "detail").to_string();
            if !run.trim().is_empty() {
                lane.run_id = run.to_string();
            }
        } else {
            lanes.push(FleetLaneCandidate {
                fleet_id: fleet_id.to_string(),
                stream_id: stream_id.to_string(),
                run_id: run.to_string(),
                state: state.to_string(),
                detail: payload(receipt, "detail").to_string(),
            });
        }
    }
    let Some((fleet_id, lanes)) = fleets
        .iter()
        .find(|(_, lanes)| lanes.iter().any(|lane| lane.run_id == run_id))
    else {
        return serde_json::json!({
            "role": "not_fleet_stream_candidate",
            "selection_required_for_delivery": false,
            "current_run_is_recommended": true,
            "candidate_count": 1,
            "grants_execution_authority": false,
        });
    };
    let mut ranked: Vec<(i64, String, serde_json::Value)> = lanes
        .iter()
        .map(|lane| {
            let fold = fold_run(kernel, &lane.run_id);
            let missing_required_semantic_operations =
                missing_fleet_stream_semantic_operations(&fold);
            let semantic_strategy_satisfied = missing_required_semantic_operations.is_empty();
            let score = fleet_lane_score(lane, &fold, &missing_required_semantic_operations);
            (
                -score,
                lane.stream_id.clone(),
                serde_json::json!({
                    "fleet_id": lane.fleet_id,
                    "stream_id": lane.stream_id,
                    "run_id": lane.run_id,
                    "state": lane.state,
                    "detail": lane.detail,
                    "score": score,
                    "status": if fold.status.trim().is_empty() { "working" } else { fold.status.as_str() },
                    "branch": fold.branch,
                    "checks_passed": fold.checks_passed,
                    "checks_failed": fold.checks_failed,
                    "semantic_operations_observed": fold.semantic_query_operations_observed,
                    "strategy_id": fold.fleet_stream_semantic_strategy_id,
                    "strategy_semantic_operations_satisfied": semantic_strategy_satisfied,
                    "missing_required_semantic_operations": missing_required_semantic_operations,
                    "builder_casting": builder_casting_json(&fold),
                    "score_basis": fleet_lane_score_basis(lane, &fold, semantic_strategy_satisfied),
                    "grants_execution_authority": false,
                }),
            )
        })
        .collect();
    ranked.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| {
                left.2["run_id"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(right.2["run_id"].as_str().unwrap_or(""))
            })
    });
    let candidates: Vec<serde_json::Value> = ranked
        .into_iter()
        .enumerate()
        .map(|(rank, (_, _, mut candidate))| {
            candidate["rank"] = serde_json::json!(rank + 1);
            candidate
        })
        .collect();
    let recommended_run_id = candidates
        .first()
        .and_then(|candidate| candidate["run_id"].as_str())
        .unwrap_or(run_id);
    let recommended_stream_id = candidates
        .first()
        .and_then(|candidate| candidate["stream_id"].as_str())
        .unwrap_or("");
    let current = candidates
        .iter()
        .find(|candidate| candidate["run_id"].as_str() == Some(run_id));
    let current_stream_id = current
        .and_then(|candidate| candidate["stream_id"].as_str())
        .unwrap_or("");
    let current_rank = current
        .and_then(|candidate| candidate["rank"].as_u64())
        .unwrap_or(1);
    let candidate_count = candidates.len();
    let current_run_is_recommended = candidate_count <= 1
        || recommended_run_id.trim().is_empty()
        || recommended_run_id == run_id;

    serde_json::json!({
        "role": "fleet_stream_candidate",
        "generated_from": "fleet.run.event.stream_lanes",
        "fleet_id": fleet_id,
        "current_run_id": run_id,
        "current_stream_id": current_stream_id,
        "current_rank": current_rank,
        "recommended_run_id": recommended_run_id,
        "recommended_stream_id": recommended_stream_id,
        "current_run_is_recommended": current_run_is_recommended,
        "selection_required_for_delivery": candidate_count > 1,
        "candidate_count": candidate_count,
        "comparison_basis": [
            "fleet_lane_state",
            "run_terminal_status",
            "selected_check_events",
            "fleet_stream_semantic_strategy",
            "builder_casting_evidence",
            "turn_count"
        ],
        "model_judgment_included": false,
        "candidates": candidates,
        "grants_execution_authority": false,
    })
}

fn fleet_lane_score(
    lane: &FleetLaneCandidate,
    fold: &RunFold,
    missing_required_semantic_operations: &[String],
) -> i64 {
    let lane_state = match lane.state.as_str() {
        "done" => 100,
        "working" => 25,
        "needs_attention" => -80,
        _ => 0,
    };
    let run_terminal = match fold.status.as_str() {
        "done" => 80,
        "finished" => 55,
        "working" => 20,
        "cannot_proceed" | "budget_exhausted" | "error" => -70,
        _ => 0,
    };
    let semantic = if missing_required_semantic_operations.is_empty()
        && !fold.fleet_stream_required_semantic_operations.is_empty()
    {
        35
    } else {
        -(missing_required_semantic_operations.len() as i64 * 18)
    };
    let builder_evidence = (fold.builder_casting_accepted_eval_score_count as i64 * 8)
        + (fold.builder_casting_matching_eval_score_count as i64 * 3)
        + (fold.builder_casting_matching_run_count as i64 * 2)
        + (fold.builder_casting_done_rate_pct as i64 / 10);
    lane_state + run_terminal + (fold.checks_passed as i64 * 18) - (fold.checks_failed as i64 * 25)
        + semantic
        + builder_evidence.min(35)
        - (fold.turns as i64 / 25)
}

fn fleet_lane_score_basis(
    lane: &FleetLaneCandidate,
    fold: &RunFold,
    semantic_strategy_satisfied: bool,
) -> Vec<String> {
    let mut basis = vec![
        format!("lane_state={}", lane.state),
        format!("run_status={}", fold.status),
        format!("checks_passed={}", fold.checks_passed),
        format!("checks_failed={}", fold.checks_failed),
        format!("strategy_semantic_operations_satisfied={semantic_strategy_satisfied}"),
        format!(
            "builder_casting_accepted_eval_score_count={}",
            fold.builder_casting_accepted_eval_score_count
        ),
        format!(
            "builder_casting_matching_run_count={}",
            fold.builder_casting_matching_run_count
        ),
    ];
    if !fold.fleet_stream_semantic_strategy_id.trim().is_empty() {
        basis.push(format!(
            "fleet_stream_strategy_id={}",
            fold.fleet_stream_semantic_strategy_id
        ));
    }
    basis
}

fn candidate_strategy_id(value: &str, index: u32, count: u32) -> String {
    if !value.trim().is_empty() {
        return value.to_string();
    }
    if count <= 1 {
        return "single_principal_path".to_string();
    }
    match index.max(1) {
        1 => "minimal_safe_patch",
        2 => "test_first_behavior",
        3 => "idiomatic_refactor",
        _ => "risk_review_repair",
    }
    .to_string()
}

fn candidate_strategy_summary(value: &str, index: u32, count: u32) -> String {
    if !value.trim().is_empty() {
        return value.to_string();
    }
    if count <= 1 {
        return "one focused implementation path".to_string();
    }
    match index.max(1) {
        1 => "smallest correct diff with the lowest integration risk",
        2 => "behavior-proof-led implementation",
        3 => "language-pack idioms and maintainability",
        _ => "edge cases, compatibility, and failure-mode repair",
    }
    .to_string()
}

fn candidate_required_semantic_operations(
    values: &[String],
    index: u32,
    count: u32,
) -> Vec<String> {
    if !values.is_empty() {
        return values.to_vec();
    }
    let operations: &[&str] = if count <= 1 {
        &["symbol_graph", "related_tests"]
    } else {
        match index.max(1) {
            1 => &[
                "symbol_graph",
                "definition",
                "dependency_map",
                "related_tests",
            ],
            2 => &[
                "related_tests",
                "references",
                "dependency_map",
                "diagnostics",
            ],
            3 => &["file_outline", "definition", "references", "diagnostics"],
            _ => &[
                "dependency_map",
                "references",
                "related_tests",
                "diagnostics",
            ],
        }
    };
    operations
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}

fn candidate_proof_bias(value: &str, index: u32, count: u32) -> String {
    if !value.trim().is_empty() {
        return value.to_string();
    }
    if count <= 1 {
        return "balanced_behavioral_proof".to_string();
    }
    match index.max(1) {
        1 => "minimal_diff_with_behavioral_check",
        2 => "red_green_or_related_test_first",
        3 => "language_idiom_and_maintainability_review",
        _ => "edge_case_security_performance_risk_review",
    }
    .to_string()
}

fn missing_required_semantic_operations(fold: &RunFold) -> Vec<String> {
    if fold.parallel_candidate_count <= 1 {
        return Vec::new();
    }
    candidate_required_semantic_operations(
        &fold.parallel_candidate_required_semantic_operations,
        fold.parallel_candidate_index,
        fold.parallel_candidate_count,
    )
    .into_iter()
    .filter(|required| {
        !fold
            .semantic_query_operations_observed
            .iter()
            .any(|observed| observed == required)
    })
    .collect()
}

fn missing_fleet_stream_semantic_operations(fold: &RunFold) -> Vec<String> {
    if fold.fleet_stream_id.trim().is_empty()
        || fold.fleet_stream_required_semantic_operations.is_empty()
    {
        return Vec::new();
    }
    fold.fleet_stream_required_semantic_operations
        .iter()
        .filter(|required| {
            !fold
                .semantic_query_operations_observed
                .iter()
                .any(|observed| observed == *required)
        })
        .cloned()
        .collect()
}

fn missing_fleet_integration_semantic_operations(fold: &RunFold) -> Vec<String> {
    if fold.fleet_integration_strategy_id.trim().is_empty()
        || fold
            .fleet_integration_required_semantic_operations
            .is_empty()
    {
        return Vec::new();
    }
    fold.fleet_integration_required_semantic_operations
        .iter()
        .filter(|required| {
            !fold
                .semantic_query_operations_observed
                .iter()
                .any(|observed| observed == *required)
        })
        .cloned()
        .collect()
}

fn candidate_comparison_json(
    kernel: &MdxKernel,
    run_id: &str,
    fold: &RunFold,
    candidate_diff_metrics: &std::collections::BTreeMap<String, CandidateDiffMetrics>,
) -> serde_json::Value {
    let primary_run_id = if fold.parallel_candidate_primary_run_id.trim().is_empty() {
        run_id.to_string()
    } else {
        fold.parallel_candidate_primary_run_id.clone()
    };
    let candidate_ids = candidate_run_ids(kernel, &primary_run_id, run_id);
    let mut candidates: Vec<(i64, u32, serde_json::Value)> = candidate_ids
        .iter()
        .map(|candidate_run_id| {
            let candidate = fold_run(kernel, candidate_run_id);
            let diff_metrics = candidate_diff_metrics.get(candidate_run_id);
            let diff_quality = candidate_diff_quality(diff_metrics, &candidate);
            let proof_quality = candidate_proof_quality(diff_metrics, &candidate);
            let eval_evidence = candidate_eval_evidence(kernel, candidate_run_id);
            let score = candidate_score(&candidate, &diff_quality, &proof_quality, &eval_evidence);
            let index = candidate.parallel_candidate_index.max(1);
            let status = if candidate.status.trim().is_empty() {
                "working"
            } else {
                candidate.status.as_str()
            };
            let role = if candidate_run_id != &primary_run_id {
                "candidate"
            } else if candidate.parallel_candidate_count > 1 || candidate_ids.len() > 1 {
                "primary"
            } else {
                "single"
            };
            let semantic_orientation_observed = candidate.principal_orientation_gate_observed
                || !candidate.semantic_query_operations_observed.is_empty();
            let related_tests_observed = candidate
                .semantic_query_operations_observed
                .iter()
                .any(|operation| operation == "related_tests");
            let missing_required_semantic_operations =
                missing_required_semantic_operations(&candidate);
            let strategy_semantic_operations_satisfied =
                missing_required_semantic_operations.is_empty();
            (
                -score,
                index,
                serde_json::json!({
                    "run_id": candidate_run_id,
                    "role": role,
                    "index": index,
                    "strategy_id": candidate_strategy_id(
                        &candidate.parallel_candidate_strategy_id,
                        candidate.parallel_candidate_index,
                        candidate.parallel_candidate_count,
                    ),
                    "strategy_summary": candidate_strategy_summary(
                        &candidate.parallel_candidate_strategy_summary,
                        candidate.parallel_candidate_index,
                        candidate.parallel_candidate_count,
                    ),
                    "required_semantic_operations": candidate_required_semantic_operations(
                        &candidate.parallel_candidate_required_semantic_operations,
                        candidate.parallel_candidate_index,
                        candidate.parallel_candidate_count,
                    ),
                    "proof_bias": candidate_proof_bias(
                        &candidate.parallel_candidate_proof_bias,
                        candidate.parallel_candidate_index,
                        candidate.parallel_candidate_count,
                    ),
                    "status": status,
                    "finished": candidate.finished,
                    "branch": candidate.branch,
                    "checks_passed": candidate.checks_passed,
                    "checks_failed": candidate.checks_failed,
                    "builder_casting": builder_casting_json(&candidate),
                    "semantic_orientation_observed": semantic_orientation_observed,
                    "related_tests_observed": related_tests_observed,
                    "strategy_semantic_operations_satisfied": strategy_semantic_operations_satisfied,
                    "missing_required_semantic_operations": missing_required_semantic_operations,
                    "diff_quality": candidate_diff_quality_json(diff_metrics, &diff_quality),
                    "proof_quality": candidate_proof_quality_json(&proof_quality),
                    "eval_evidence": candidate_eval_evidence_json(&eval_evidence),
                    "score": score,
                    "score_basis": candidate_score_basis(&candidate, &diff_quality, &proof_quality, &eval_evidence),
                    "grants_execution_authority": false,
                }),
            )
        })
        .collect();
    candidates.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| {
                left.2["run_id"]
                    .as_str()
                    .unwrap_or("")
                    .cmp(right.2["run_id"].as_str().unwrap_or(""))
            })
    });
    let ranked: Vec<serde_json::Value> = candidates
        .into_iter()
        .enumerate()
        .map(|(rank, (_, _, mut candidate))| {
            candidate["rank"] = serde_json::json!(rank + 1);
            candidate
        })
        .collect();
    let recommended_run_id = ranked
        .first()
        .and_then(|candidate| candidate["run_id"].as_str())
        .unwrap_or(run_id);
    let current_rank = ranked
        .iter()
        .find_map(|candidate| {
            if candidate["run_id"].as_str() == Some(run_id) {
                candidate["rank"].as_u64()
            } else {
                None
            }
        })
        .unwrap_or(1);
    serde_json::json!({
        "generated_from": "forge.run.event.parallel_candidates",
        "primary_run_id": primary_run_id,
        "current_run_id": run_id,
        "current_rank": current_rank,
        "recommended_run_id": recommended_run_id,
        "candidate_count": ranked.len(),
        "comparison_basis": ["terminal_status", "selected_check_events", "proof_coverage", "proof_scope", "behavior_proof", "semantic_orientation", "check_failures", "turn_count", "deterministic_diff_quality", "quarantined_eval_evidence"],
        "diff_quality_included": true,
        "eval_evidence_included": true,
        "model_judgment_included": false,
        "candidates": ranked,
        "grants_execution_authority": false,
    })
}

fn candidate_run_ids(
    kernel: &MdxKernel,
    primary_run_id: &str,
    fallback_run_id: &str,
) -> Vec<String> {
    let mut seen: Vec<(u32, String)> = Vec::new();
    for receipt in kernel.ledger().query().by_kind("forge.run.event") {
        if payload(receipt, "event") != "run_started" {
            continue;
        }
        let run_id = payload(receipt, "run_id");
        if run_id.trim().is_empty() {
            continue;
        }
        let primary = payload(receipt, "parallel_candidate_primary_run_id");
        if primary != primary_run_id {
            continue;
        }
        let index = payload(receipt, "parallel_candidate_index")
            .parse::<u32>()
            .unwrap_or(1);
        if !seen.iter().any(|(_, seen_run_id)| seen_run_id == run_id) {
            seen.push((index, run_id.to_string()));
        }
    }
    if !seen
        .iter()
        .any(|(_, seen_run_id)| seen_run_id == primary_run_id)
    {
        seen.push((1, primary_run_id.to_string()));
    }
    if !seen
        .iter()
        .any(|(_, seen_run_id)| seen_run_id == fallback_run_id)
    {
        seen.push((seen.len() as u32 + 1, fallback_run_id.to_string()));
    }
    seen.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    seen.into_iter().map(|(_, run_id)| run_id).collect()
}

#[derive(Debug, Clone, Default)]
struct CandidateEvalEvidence {
    present: bool,
    result_receipt_id: String,
    policy_decision_id: String,
    language_task_corpus_id: String,
    language_pack_id: String,
    model_profile_id: String,
    output_quarantined: bool,
    accepted_for_scoreboard: bool,
    mdx_quality_gates_required: bool,
    blocked_reason: String,
    output_artifact_ref: String,
}

fn candidate_eval_evidence(kernel: &MdxKernel, run_id: &str) -> CandidateEvalEvidence {
    let needle = format!("/runs/{}/", run_id.trim());
    let mut evidence = CandidateEvalEvidence::default();
    if needle == "/runs//" {
        return evidence;
    }
    for receipt in kernel
        .ledger()
        .query()
        .by_kind("forge.fleet_eval_result.ingested")
    {
        let output_artifact_ref = payload(receipt, "output_artifact_ref");
        if !output_artifact_ref.contains(&needle) {
            continue;
        }
        evidence = CandidateEvalEvidence {
            present: true,
            result_receipt_id: receipt.receipt_id.clone(),
            policy_decision_id: receipt.policy_decision_id.clone().unwrap_or_default(),
            language_task_corpus_id: payload(receipt, "language_task_corpus_id").to_string(),
            language_pack_id: payload(receipt, "language_pack_id").to_string(),
            model_profile_id: payload(receipt, "model_profile_id").to_string(),
            output_quarantined: payload(receipt, "output_quarantined") == "true",
            accepted_for_scoreboard: payload(receipt, "accepted_for_scoreboard") == "true",
            mdx_quality_gates_required: payload(receipt, "mdx_quality_gates_required") == "true",
            blocked_reason: payload(receipt, "blocked_reason").to_string(),
            output_artifact_ref: output_artifact_ref.to_string(),
        };
    }
    evidence
}

fn candidate_eval_evidence_json(evidence: &CandidateEvalEvidence) -> serde_json::Value {
    serde_json::json!({
        "present": evidence.present,
        "result_receipt_id": evidence.result_receipt_id,
        "policy_decision_id": evidence.policy_decision_id,
        "language_task_corpus_id": evidence.language_task_corpus_id,
        "language_pack_id": evidence.language_pack_id,
        "model_profile_id": evidence.model_profile_id,
        "output_quarantined": evidence.output_quarantined,
        "accepted_for_scoreboard": evidence.accepted_for_scoreboard,
        "mdx_quality_gates_required": evidence.mdx_quality_gates_required,
        "blocked_reason": evidence.blocked_reason,
        "output_artifact_ref": evidence.output_artifact_ref,
        "model_judgment_included": false,
        "grants_execution_authority": false,
    })
}

fn candidate_impacted_language_packs(
    metrics: Option<&CandidateDiffMetrics>,
    fold: &RunFold,
) -> Vec<String> {
    let mut touched_packs = std::collections::BTreeSet::new();
    if let Some(metrics) = metrics {
        for path in &metrics.source_paths {
            if let Some(pack) =
                affected_language_pack(path, &fold.repo_profile_detected_language_packs)
            {
                touched_packs.insert(pack);
            }
        }
    }
    touched_packs.into_iter().collect()
}

fn candidate_proof_quality(
    metrics: Option<&CandidateDiffMetrics>,
    fold: &RunFold,
) -> CandidateProofQuality {
    let proof_coverage = proof_coverage(fold);
    let impacted_language_packs = candidate_impacted_language_packs(metrics, fold);
    let proof_scope = proof_scope(fold, &impacted_language_packs);
    let behavior_proof_paths = metrics
        .map(|metrics| {
            metrics
                .source_paths
                .iter()
                .filter(|path| is_behavior_proof_path(path))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let behavior_proof = behavior_proof(
        fold,
        &proof_coverage,
        metrics
            .map(|metrics| metrics.real_change_count)
            .unwrap_or(0),
        &behavior_proof_paths,
    );
    let coverage_status = proof_coverage["status"]
        .as_str()
        .unwrap_or("not_recorded")
        .to_string();
    let scope_status = proof_scope["status"]
        .as_str()
        .unwrap_or("not_applicable")
        .to_string();
    let behavior_status = behavior_proof["status"]
        .as_str()
        .unwrap_or("gap")
        .to_string();
    let setup_command_count = proof_coverage["setup_commands"]
        .as_array()
        .map(|items| items.len())
        .unwrap_or(0);
    let proof_command_count = proof_coverage["proof_commands"]
        .as_array()
        .map(|items| items.len())
        .unwrap_or(0);
    let satisfied_proof_command_count = proof_coverage["satisfied_proof_commands"]
        .as_array()
        .map(|items| items.len())
        .unwrap_or(0);

    let coverage_score = match coverage_status.as_str() {
        "satisfied" => 30,
        "failed" => -60,
        "missing" => -40,
        "no_proof_command" => -35,
        "no_change" => -50,
        "not_recorded" => -25,
        _ => -15,
    };
    let scope_score = match scope_status.as_str() {
        "covered" => 15,
        "not_applicable" => 0,
        "partial" => -10,
        "uncovered" => -25,
        _ => -10,
    };
    let behavior_score = match behavior_status.as_str() {
        "covered" => 25,
        "weak" => -15,
        "gap" => -35,
        _ => -20,
    };
    let proof_command_bonus = (satisfied_proof_command_count as i64 * 5).min(15);
    let setup_only_penalty = if setup_command_count > 0 && proof_command_count == 0 {
        -20
    } else {
        0
    };
    let score =
        coverage_score + scope_score + behavior_score + proof_command_bonus + setup_only_penalty;

    CandidateProofQuality {
        score,
        proof_coverage_status: coverage_status,
        proof_scope_status: scope_status,
        behavior_proof_status: behavior_status,
        setup_command_count,
        proof_command_count,
        satisfied_proof_command_count,
        basis: vec![
            format!(
                "proof_coverage_status={}",
                proof_coverage["status"].as_str().unwrap_or("not_recorded")
            ),
            format!(
                "proof_scope_status={}",
                proof_scope["status"].as_str().unwrap_or("not_applicable")
            ),
            format!(
                "behavior_proof_status={}",
                behavior_proof["status"].as_str().unwrap_or("gap")
            ),
            format!("setup_command_count={setup_command_count}"),
            format!("proof_command_count={proof_command_count}"),
            format!("satisfied_proof_command_count={satisfied_proof_command_count}"),
            format!("proof_quality_score={score}"),
        ],
    }
}

fn candidate_proof_quality_json(quality: &CandidateProofQuality) -> serde_json::Value {
    serde_json::json!({
        "score": quality.score,
        "proof_coverage_status": quality.proof_coverage_status,
        "proof_scope_status": quality.proof_scope_status,
        "behavior_proof_status": quality.behavior_proof_status,
        "setup_command_count": quality.setup_command_count,
        "proof_command_count": quality.proof_command_count,
        "satisfied_proof_command_count": quality.satisfied_proof_command_count,
        "basis": quality.basis,
        "model_judgment_included": false,
        "grants_execution_authority": false,
    })
}

fn candidate_score(
    fold: &RunFold,
    diff_quality: &CandidateDiffQuality,
    proof_quality: &CandidateProofQuality,
    eval_evidence: &CandidateEvalEvidence,
) -> i64 {
    let terminal = match fold.status.as_str() {
        "done" => 100,
        "finished" => 70,
        "working" => 30,
        "stopped" => 15,
        "cannot_proceed" | "budget_exhausted" | "error" => 0,
        _ => 20,
    };
    let semantic = if fold.principal_orientation_gate_observed
        || !fold.semantic_query_operations_observed.is_empty()
    {
        10
    } else {
        0
    };
    let related_tests = if fold
        .semantic_query_operations_observed
        .iter()
        .any(|operation| operation == "related_tests")
    {
        5
    } else {
        0
    };
    let quarantined_eval_evidence = if eval_evidence.present
        && eval_evidence.output_quarantined
        && eval_evidence.mdx_quality_gates_required
        && !eval_evidence.accepted_for_scoreboard
    {
        8
    } else {
        0
    };
    let strategy_semantic_penalty = missing_required_semantic_operations(fold).len() as i64 * 12;
    terminal
        + (fold.checks_passed as i64 * 15)
        + semantic
        + related_tests
        + diff_quality.score
        + proof_quality.score
        + quarantined_eval_evidence
        - strategy_semantic_penalty
        - (fold.checks_failed as i64 * 20)
        - (fold.turns as i64 / 20)
}

fn candidate_score_basis(
    fold: &RunFold,
    diff_quality: &CandidateDiffQuality,
    proof_quality: &CandidateProofQuality,
    eval_evidence: &CandidateEvalEvidence,
) -> Vec<String> {
    let mut basis = vec![format!("status={}", fold.status)];
    basis.push(format!("checks_passed={}", fold.checks_passed));
    basis.push(format!("checks_failed={}", fold.checks_failed));
    if !fold.builder_casting_selected_slot.trim().is_empty() {
        basis.push(format!(
            "builder_casting_selected_slot={}",
            fold.builder_casting_selected_slot
        ));
    }
    if !fold
        .builder_casting_selected_model_profile_id
        .trim()
        .is_empty()
    {
        basis.push(format!(
            "builder_casting_selected_model_profile_id={}",
            fold.builder_casting_selected_model_profile_id
        ));
    }
    if !fold.builder_casting_selected_model_id.trim().is_empty() {
        basis.push(format!(
            "builder_casting_selected_model_id={}",
            fold.builder_casting_selected_model_id
        ));
    }
    if !fold
        .builder_casting_recommended_model_profile_id
        .trim()
        .is_empty()
    {
        basis.push(format!(
            "builder_casting_recommended_model_profile_id={}",
            fold.builder_casting_recommended_model_profile_id
        ));
    }
    if !fold.builder_casting_status.trim().is_empty() {
        basis.push(format!(
            "builder_casting_status={}",
            fold.builder_casting_status
        ));
    }
    if fold.principal_orientation_gate_observed {
        basis.push("semantic_orientation_observed=true".to_string());
    }
    if fold
        .semantic_query_operations_observed
        .iter()
        .any(|operation| operation == "related_tests")
    {
        basis.push("related_tests_observed=true".to_string());
    }
    let missing_required_semantic_operations = missing_required_semantic_operations(fold);
    if missing_required_semantic_operations.is_empty() {
        basis.push("strategy_semantic_operations_satisfied=true".to_string());
    } else {
        basis.push(format!(
            "missing_required_semantic_operations={}",
            missing_required_semantic_operations.join(",")
        ));
    }
    basis.push(format!("turns={}", fold.turns));
    basis.push(format!("diff_quality_score={}", diff_quality.score));
    basis.extend(diff_quality.basis.clone());
    basis.push(format!("proof_quality_score={}", proof_quality.score));
    basis.extend(proof_quality.basis.clone());
    if eval_evidence.present {
        basis.push("quarantined_eval_evidence=true".to_string());
        basis.push(format!(
            "eval_language_task_corpus_id={}",
            eval_evidence.language_task_corpus_id
        ));
        basis.push(format!(
            "eval_model_profile_id={}",
            eval_evidence.model_profile_id
        ));
        basis.push(format!(
            "eval_blocked_reason={}",
            eval_evidence.blocked_reason
        ));
    } else {
        basis.push("quarantined_eval_evidence=false".to_string());
    }
    basis
}

fn candidate_diff_quality(
    metrics: Option<&CandidateDiffMetrics>,
    fold: &RunFold,
) -> CandidateDiffQuality {
    let Some(metrics) = metrics else {
        return CandidateDiffQuality {
            score: 0,
            touched_language_pack_count: 0,
            basis: vec!["diff_metrics_available=false".to_string()],
        };
    };
    if !metrics.available {
        return CandidateDiffQuality {
            score: 0,
            touched_language_pack_count: 0,
            basis: vec!["diff_metrics_available=false".to_string()],
        };
    }

    let mut touched_packs = std::collections::BTreeSet::new();
    for path in &metrics.source_paths {
        if let Some(pack) = affected_language_pack(path, &fold.repo_profile_detected_language_packs)
        {
            touched_packs.insert(pack);
        }
    }

    let mut score = 0i64;
    if metrics.real_change_count > 0 {
        score += 12;
    } else {
        score -= 10;
    }
    if !touched_packs.is_empty() {
        score += 4;
    }
    if metrics.generated_count == 0 {
        score += 3;
    } else if metrics.real_change_count > 0 && metrics.generated_count <= metrics.real_change_count
    {
        score += 1;
    } else {
        score -= 3;
    }
    if metrics.artifact_count == 0 {
        score += 4;
    } else {
        score -= ((metrics.artifact_count as i64) * 2).min(12);
    }
    let churn = metrics.added_total + metrics.removed_total;
    if churn <= 300 {
        score += 4;
    } else if churn > 900 {
        score -= 8;
    }

    CandidateDiffQuality {
        score,
        touched_language_pack_count: touched_packs.len(),
        basis: vec![
            format!("diff_file_count={}", metrics.file_count),
            format!("diff_real_change_count={}", metrics.real_change_count),
            format!("diff_generated_count={}", metrics.generated_count),
            format!("diff_artifact_count={}", metrics.artifact_count),
            format!("diff_churn={churn}"),
            format!("diff_touched_language_packs={}", touched_packs.len()),
        ],
    }
}

fn candidate_diff_quality_json(
    metrics: Option<&CandidateDiffMetrics>,
    quality: &CandidateDiffQuality,
) -> serde_json::Value {
    let available = metrics.map(|metrics| metrics.available).unwrap_or(false);
    serde_json::json!({
        "available": available,
        "score": quality.score,
        "file_count": metrics.map(|metrics| metrics.file_count).unwrap_or(0),
        "real_change_count": metrics.map(|metrics| metrics.real_change_count).unwrap_or(0),
        "generated_count": metrics.map(|metrics| metrics.generated_count).unwrap_or(0),
        "artifact_count": metrics.map(|metrics| metrics.artifact_count).unwrap_or(0),
        "added_total": metrics.map(|metrics| metrics.added_total).unwrap_or(0),
        "removed_total": metrics.map(|metrics| metrics.removed_total).unwrap_or(0),
        "touched_language_pack_count": quality.touched_language_pack_count,
        "basis": quality.basis,
        "model_judgment_included": false,
        "grants_execution_authority": false,
    })
}

fn eval_quality_gate_assessment_json(fold: &RunFold) -> serde_json::Value {
    let recorded = !fold.eval_quality_gate_status.trim().is_empty()
        || !fold.eval_result_receipt_id.trim().is_empty();
    serde_json::json!({
        "generated_from": if recorded {
            "forge.run.event.eval_quality_gate_assessed"
        } else {
            "not_recorded"
        },
        "recorded": recorded,
        "status": if recorded {
            fold.eval_quality_gate_status.as_str()
        } else {
            "not_recorded"
        },
        "machine_quality_gates_passed": fold.eval_machine_quality_gates_passed,
        "can_be_scored_after_principal_review": fold.eval_can_be_scored_after_principal_review,
        "passed_gate_count": fold.eval_passed_gate_count,
        "failed_gate_count": fold.eval_failed_gate_count,
        "passed_gates": fold.eval_passed_gates,
        "failed_gates": fold.eval_failed_gates,
        "result_receipt_id": fold.eval_result_receipt_id,
        "accepted_for_scoreboard": fold.eval_accepts_for_scoreboard,
        "grants_execution_authority": fold.eval_grants_execution_authority,
        "acceptance_boundary": "machine quality gates are evidence only; scoreboard acceptance still requires principal review or hidden-check authority",
        "production_write_allowed": false,
    })
}

fn render(
    kernel: &MdxKernel,
    run_id: &str,
    diff_files: Option<Vec<crate::forge_diff_route::RunDiffFile>>,
    candidate_diff_metrics: &std::collections::BTreeMap<String, CandidateDiffMetrics>,
) -> String {
    let fold = fold_run(kernel, run_id);
    if !fold.found {
        return serde_json::json!({
            "name": "mdx-forge-review-packet",
            "status": "REFUSED",
            "reason": "no run by that id - it may have been cleaned up",
            "production_write_allowed": false,
        })
        .to_string();
    }
    let ship = ship_decision(kernel, run_id);
    let (status, next_move) = review_status(&fold, &ship);

    // Classify the change into sections, in review order, with build output
    // filtered into its own evidence bucket. Generated source artifacts still
    // read as generated; build artifacts do not get to bury the real change.
    let mut by_section: std::collections::BTreeMap<&'static str, Vec<serde_json::Value>> =
        std::collections::BTreeMap::new();
    let mut section_meta: std::collections::BTreeMap<&'static str, (u32, u32, bool)> =
        std::collections::BTreeMap::new();
    let mut added_total = 0u32;
    let mut removed_total = 0u32;
    let mut real_change = 0usize;
    let mut generated_count = 0usize;
    let mut artifact_count = 0usize;
    let mut artifact_files_sample: Vec<serde_json::Value> = Vec::new();
    let mut artifact_summary: std::collections::BTreeMap<
        &'static str,
        (usize, u32, u32, Vec<String>),
    > = std::collections::BTreeMap::new();
    let mut language_pack_impact: std::collections::BTreeMap<
        String,
        (usize, u32, u32, usize, Vec<String>),
    > = std::collections::BTreeMap::new();
    let mut behavior_proof_paths = Vec::new();
    let files = diff_files.unwrap_or_default();
    for file in &files {
        if crate::forge_repo_profile::is_artifact_path(&file.path) {
            artifact_count += 1;
            let reason = crate::forge_repo_profile::artifact_reason(&file.path);
            let entry = artifact_summary.entry(reason).or_default();
            entry.0 += 1;
            entry.1 += file.added;
            entry.2 += file.removed;
            if entry.3.len() < 8 {
                entry.3.push(file.path.clone());
            }
            if artifact_files_sample.len() < 40 {
                artifact_files_sample.push(serde_json::json!({
                    "path": file.path,
                    "added": file.added,
                    "removed": file.removed,
                    "reason": reason,
                }));
            }
            continue;
        }
        let (section, generated) = classify_path(&file.path);
        added_total += file.added;
        removed_total += file.removed;
        if generated {
            generated_count += 1;
        } else {
            real_change += 1;
        }
        if !generated && is_behavior_proof_path(&file.path) {
            behavior_proof_paths.push(file.path.clone());
        }
        if let Some(pack) =
            affected_language_pack(&file.path, &fold.repo_profile_detected_language_packs)
        {
            let entry = language_pack_impact.entry(pack).or_default();
            entry.0 += 1;
            entry.1 += file.added;
            entry.2 += file.removed;
            if generated {
                entry.3 += 1;
            }
            if entry.4.len() < 8 {
                entry.4.push(file.path.clone());
            }
        }
        by_section
            .entry(section)
            .or_default()
            .push(serde_json::json!({
                "path": file.path,
                "added": file.added,
                "removed": file.removed,
            }));
        let meta = section_meta.entry(section).or_insert((0, 0, generated));
        meta.0 += file.added;
        meta.1 += file.removed;
    }
    let mut sections: Vec<serde_json::Value> = by_section
        .iter()
        .map(|(section, files)| {
            let (added, removed, generated) =
                section_meta.get(section).copied().unwrap_or((0, 0, false));
            serde_json::json!({
                "section": section,
                "label": section_label(section),
                "order": section_index(section),
                "file_count": files.len(),
                "added": added,
                "removed": removed,
                "generated": generated,
                "files": files,
            })
        })
        .collect();
    sections.sort_by_key(|value| value["order"].as_u64().unwrap_or(u64::MAX));
    let artifact_summary: Vec<serde_json::Value> = artifact_summary
        .iter()
        .map(|(reason, (count, added, removed, sample_paths))| {
            serde_json::json!({
                "reason": reason,
                "count": count,
                "added": added,
                "removed": removed,
                "sample_paths": sample_paths,
            })
        })
        .collect();
    let language_pack_impact: Vec<serde_json::Value> = language_pack_impact
        .iter()
        .map(
            |(pack, (count, added, removed, generated_count, sample_paths))| {
                serde_json::json!({
                    "language_pack_id": pack,
                    "file_count": count,
                    "added": added,
                    "removed": removed,
                    "generated_count": generated_count,
                    "sample_paths": sample_paths,
                })
            },
        )
        .collect();
    let impacted_language_packs = language_pack_impact
        .iter()
        .filter_map(|value| value["language_pack_id"].as_str().map(str::to_string))
        .collect::<Vec<_>>();

    let checks: Vec<serde_json::Value> = fold
        .check_names
        .iter()
        .map(|(name, ok)| serde_json::json!({ "name": name, "ok": ok }))
        .collect();
    let baseline_checks: Vec<serde_json::Value> = fold
        .baseline_check_names
        .iter()
        .map(|(name, ok)| serde_json::json!({ "name": name, "ok": ok }))
        .collect();
    let proof_coverage = proof_coverage(&fold);
    let proof_scope = proof_scope(&fold, &impacted_language_packs);
    behavior_proof_paths.sort();
    behavior_proof_paths.dedup();
    let behavior_proof = behavior_proof(&fold, &proof_coverage, real_change, &behavior_proof_paths);
    let standards_obligations = crate::forge_repo_standards_packet_route::standards_obligations(
        &fold.repo_profile_standards_source_summaries,
    );
    let principal_review = principal_review(PrincipalReviewContext {
        fold: &fold,
        real_change_count: real_change,
        artifact_count,
        impacted_language_packs: &impacted_language_packs,
        proof_coverage: &proof_coverage,
        proof_scope: &proof_scope,
        behavior_proof: &behavior_proof,
        standards_obligations: &standards_obligations,
    });
    let candidate_comparison =
        candidate_comparison_json(kernel, run_id, &fold, candidate_diff_metrics);
    let pr_handoff = pr_handoff(
        run_id,
        &fold,
        status,
        next_move,
        real_change,
        artifact_count,
        &impacted_language_packs,
        &artifact_summary,
        &principal_review,
        &proof_coverage,
        &proof_scope,
        &standards_obligations,
        &candidate_comparison,
    );

    serde_json::json!({
        "name": "mdx-forge-review-packet",
        "status": "OK",
        "run_id": run_id,
        "review_status": status,
        "next_move": next_move,
        "branch": fold.branch,
        "commit_sha": fold.branch_sha,
        "work_item_id": fold.work_item_id,
        "summary": {
            "run_status": fold.status,
            "finished": fold.finished,
            "turns": fold.turns,
            "model_calls": fold.model_calls,
            "tool_calls": fold.tool_calls,
            "event_count": fold.event_count,
        },
        "built_by": { "model": fold.model },
        "checks": {
            "passed": fold.checks_passed,
            "failed": fold.checks_failed,
            "names": checks,
            "baseline": {
                "passed": fold.baseline_checks_passed,
                "failed": fold.baseline_checks_failed,
                "names": baseline_checks,
                "pre_change_evidence_only": true,
            },
        },
        "proof_coverage": proof_coverage,
        "proof_scope": proof_scope,
        "behavior_proof": behavior_proof,
        "work_classification": {
            "recommended_shape": fold.work_classification_recommended_shape,
            "task_class": fold.work_classification_task_class,
            "complexity_tier": fold.work_classification_complexity_tier,
            "confidence_pct": fold.work_classification_confidence_pct,
            "rationale": fold.work_classification_rationale,
            "source": fold.work_classification_source,
            "grants_execution_authority": fold.work_classification_grants_execution_authority,
        },
        "language_task_alignment": {
            "task_corpus_id": fold.language_task_corpus_id,
            "status": fold.language_task_alignment_status,
            "source": fold.language_task_alignment_source,
            "task_class": fold.language_task_class,
            "complexity_tier": fold.language_task_complexity_tier,
            "visible_check": fold.language_task_visible_check,
            "hidden_check_slot": fold.language_task_hidden_check_slot,
            "artifact_noise_expected": fold.language_task_artifact_noise_expected,
            "required_principal_review_gates": fold.language_task_required_principal_review_gates,
            "engineering_facets": fold.language_task_engineering_facets,
            "evaluation_oracle": fold.language_task_evaluation_oracle,
            "human_timebox_minutes": fold.language_task_human_timebox_minutes,
            "contamination_policy": fold.language_task_contamination_policy,
            "grants_execution_authority": fold.language_task_alignment_grants_execution_authority,
        },
        "mission": mission_attachment_json(&fold),
        "eval_quality_gate_assessment": eval_quality_gate_assessment_json(&fold),
        "standards_cited": fold.standards,
        "repo_intelligence": repo_intelligence_json(kernel, run_id, &fold, &standards_obligations),
        "candidate_comparison": candidate_comparison,
        "repo_intake": repo_intake_json(&fold),
        "principal_review": principal_review,
        "pr_handoff": pr_handoff,
        "diff": {
            "file_count": files.len(),
            "real_change_count": real_change,
            "generated_count": generated_count,
            "artifact_count": artifact_count,
            "artifact_summary": artifact_summary,
            "artifact_files_sample": artifact_files_sample,
            "language_pack_impact": language_pack_impact,
            "added_total": added_total,
            "removed_total": removed_total,
            "order": SECTION_ORDER,
            "sections": sections,
        },
        "ship": {
            "decided": ship.decided,
            "reason": ship.reason,
            "receipt_id": ship.receipt_id,
        },
        "panel": latest_panel(kernel, run_id),
        "diff_route": "/forge/run-diff.json",
        "panel_route": "/forge/review-panel.json",
        "declared_not_built": {
            "predicted_outcome": "a likely-clean / risk-of-budget-exhaust read from model track record lands next",
            "response_schema": "generated/response-schemas/forge-review-packet-response.schema.json",
        },
        "production_write_allowed": false,
    })
    .to_string()
}

fn payload<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn csv_values(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn newline_values(value: &str) -> Vec<String> {
    value
        .lines()
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-review-packet","status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
            mdx_core::json_string_literal(reason)
        ),
    )
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
    fn proof_language_pack_maps_common_stack_native_command_variants() {
        assert_eq!(
            proof_language_pack(
                "xcodebuild test -scheme App -destination 'platform=iOS Simulator'"
            ),
            Some("ios-xcode")
        );
        assert_eq!(
            proof_language_pack("swift test --filter SlugTests"),
            Some("swift-spm")
        );
        assert_eq!(
            proof_language_pack("swift run CanaryCheck"),
            Some("swift-spm")
        );
        assert_eq!(proof_language_pack("mvn -q test"), Some("java-maven"));
        assert_eq!(proof_language_pack("mvn -version"), None);
        assert_eq!(
            proof_language_pack("./gradlew :core:test"),
            Some("gradle-jvm")
        );
        assert_eq!(
            proof_language_pack("./gradlew :app:testDebugUnitTest"),
            Some("android-gradle")
        );
        assert_eq!(
            proof_language_pack("dotnet test src/App.sln"),
            Some("dotnet")
        );
        assert_eq!(
            proof_language_pack("npm run test -- --runInBand"),
            Some("node")
        );
        assert_eq!(
            proof_language_pack("python -m pytest tests/unit"),
            Some("python")
        );
        assert_eq!(
            proof_language_pack(". .venv/bin/activate && pytest"),
            Some("python")
        );
        assert_eq!(proof_language_pack("go test ./pkg/..."), Some("go"));
    }

    #[test]
    fn an_unknown_run_is_refused_not_invented() {
        let kernel = MdxKernel::boot_local();
        let body = render(
            &kernel,
            "forge_run_missing",
            None,
            &std::collections::BTreeMap::new(),
        );
        let value: serde_json::Value = serde_json::from_str(&body).expect("valid json");
        assert_eq!(value["status"], "REFUSED");
    }

    #[test]
    fn baseline_failure_is_separate_from_post_change_proof() {
        let mut kernel = MdxKernel::boot_local();
        let run_id = "forge_run_baseline_then_green";
        kernel
            .record_forge_run_event_with_evidence_fields(
                ev(run_id, "run_started", "branch=forge/baseline-then-green"),
                &mdx_core::GovernedWriteIdentity::local_demo("human:dev"),
                &[
                    ("selected_checks", "swift test"),
                    ("selected_checks_source", "operator_supplied"),
                ],
            )
            .expect("run started");
        kernel
            .record_forge_run_event(ev(
                run_id,
                "check_failed",
                "baseline_run_command swift test exit=1",
            ))
            .expect("baseline failed");
        kernel
            .record_forge_run_event(ev(run_id, "check_passed", "run_command swift test exit=0"))
            .expect("post-change proof passed");
        kernel
            .record_forge_run_event(ev(run_id, "run_finished", "RUN_FINISHED_DONE"))
            .expect("run finished");

        let body = render(
            &kernel,
            run_id,
            Some(Vec::new()),
            &std::collections::BTreeMap::new(),
        );
        let value: serde_json::Value = serde_json::from_str(&body).expect("valid json");
        assert_eq!(value["checks"]["passed"], 1);
        assert_eq!(value["checks"]["failed"], 0);
        assert_eq!(value["checks"]["baseline"]["passed"], 0);
        assert_eq!(value["checks"]["baseline"]["failed"], 1);
        assert_eq!(value["proof_coverage"]["status"], "satisfied");
    }

    #[test]
    fn review_packet_exposes_mission_checkpoint_attachment() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event_with_evidence_fields(
                ev(
                    "forge_run_mission_attached",
                    "run_started",
                    "branch=forge/mission-attached",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("human:dev"),
                &[
                    ("repo_id", "swift_service"),
                    ("repo_primary_language", "swift"),
                    ("language_pack_id", "swift-spm"),
                    ("selected_checks", "swift test"),
                    ("selected_checks_source", "operator_supplied"),
                    ("mission_id", "mission_123"),
                    ("mission_milestone_id", "milestone_acceptance"),
                    (
                        "mission_checkpoint_route",
                        "/forge/long-horizon-mission-checkpoints.json",
                    ),
                    ("mission_checkpoint_grants_execution_authority", "false"),
                ],
            )
            .expect("run started");

        let body = render(
            &kernel,
            "forge_run_mission_attached",
            None,
            &std::collections::BTreeMap::new(),
        );
        let value: serde_json::Value = serde_json::from_str(&body).expect("valid json");
        assert_eq!(value["mission"]["attached"], true);
        assert_eq!(value["mission"]["mission_id"], "mission_123");
        assert_eq!(value["mission"]["milestone_id"], "milestone_acceptance");
        assert_eq!(
            value["repo_intelligence"]["mission"]["checkpoint_route"],
            "/forge/long-horizon-mission-checkpoints.json"
        );
        assert_eq!(
            value["mission"]["checkpoint_grants_execution_authority"],
            false
        );
        assert!(
            value["principal_review"]["checklist"]
                .as_array()
                .expect("checklist")
                .iter()
                .any(|line| line["kind"] == "mission_checkpoint")
        );
    }

    #[test]
    fn refused_semantic_queries_do_not_count_as_review_orientation() {
        assert!(semantic_query_detail_succeeded(
            "semantic_query status=OK operation=related_tests"
        ));
        assert_eq!(
            semantic_query_operation("semantic_query status=OK operation=related_tests"),
            Some("related_tests".to_string())
        );
        assert_eq!(
            semantic_query_operation("semantic_query status=OK operation=orientation"),
            Some("orientation".to_string())
        );
        assert_eq!(
            semantic_query_operation("semantic_query status=OK operation=impact_radius"),
            Some("impact_radius".to_string())
        );
        assert!(!semantic_query_detail_succeeded(
            "semantic_query status=REFUSED operation=related_tests"
        ));
        assert_eq!(
            semantic_query_operation("semantic_query status=REFUSED operation=related_tests"),
            None
        );

        // Legacy receipts from before status-bearing details remain readable.
        assert!(semantic_query_detail_succeeded(
            "semantic_query workspace_symbol"
        ));
        assert_eq!(
            semantic_query_operation("semantic_query workspace_symbol"),
            Some("workspace_symbol".to_string())
        );
    }

    #[test]
    fn review_packet_exposes_fleet_stream_semantic_strategy_gaps() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event_with_evidence_fields(
                ev(
                    "forge_run_fleet_stream",
                    "run_started",
                    "branch=forge/fleet_stream",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("human:dev"),
                &[
                    ("repo_id", "java_service"),
                    ("repo_primary_language", "java"),
                    ("language_pack_id", "java-maven"),
                    ("repo_profile_detected_language_packs", "java-maven"),
                    (
                        "repo_profile_semantic_intelligence",
                        "semantic_query:workspace_symbol,semantic_query:definition",
                    ),
                    (
                        "repo_profile_semantic_tool_readiness",
                        "jdtls=missing,fallback_index=available",
                    ),
                    ("repo_profile_suggested_checks", "mvn test"),
                    ("repo_profile_artifact_patterns", "target/**"),
                    ("selected_checks", "mvn test"),
                    ("selected_checks_source", "fleet_stream_checks"),
                    ("principal_orientation_gate_required", "true"),
                    ("principal_orientation_gate_tool", "semantic_query"),
                    ("principal_orientation_gate_grants_authority", "false"),
                    ("fleet_stream_id", "s1_service"),
                    ("fleet_stream_semantic_strategy_id", "stream_contract_guard"),
                    (
                        "fleet_stream_semantic_strategy_summary",
                        "preserve interfaces while improving the local design",
                    ),
                    (
                        "fleet_stream_required_semantic_operations",
                        "file_outline\ndefinition\nreferences\ndiagnostics",
                    ),
                    (
                        "fleet_stream_proof_bias",
                        "interface_compatibility_and_behavioral_check",
                    ),
                ],
            )
            .expect("run started");
        kernel
            .record_forge_run_event(ev(
                "forge_run_fleet_stream",
                "tool_executed",
                "semantic_query status=OK operation=file_outline",
            ))
            .expect("semantic file outline");
        kernel
            .record_forge_run_event(ev(
                "forge_run_fleet_stream",
                "tool_executed",
                "semantic_query status=OK operation=definition",
            ))
            .expect("semantic definition");

        let body = render(
            &kernel,
            "forge_run_fleet_stream",
            None,
            &std::collections::BTreeMap::new(),
        );
        let value: serde_json::Value = serde_json::from_str(&body).expect("valid json");

        assert_eq!(value["repo_intelligence"]["fleet_stream"]["role"], "stream");
        assert_eq!(
            value["repo_intelligence"]["fleet_stream"]["stream_id"],
            "s1_service"
        );
        assert_eq!(
            value["repo_intelligence"]["fleet_stream"]["strategy_id"],
            "stream_contract_guard"
        );
        assert_eq!(
            value["repo_intelligence"]["fleet_stream"]["required_semantic_operations"][0],
            "file_outline"
        );
        assert_eq!(
            value["repo_intelligence"]["fleet_stream"]["strategy_semantic_operations_satisfied"],
            false
        );
        assert_eq!(
            value["repo_intelligence"]["fleet_stream"]["missing_required_semantic_operations"][0],
            "references"
        );
        assert_eq!(
            value["repo_intelligence"]["fleet_stream"]["missing_required_semantic_operations"][1],
            "diagnostics"
        );
    }

    #[test]
    fn review_packet_ranks_fleet_stream_lanes_and_names_the_recommended_run() {
        let mut kernel = MdxKernel::boot_local();
        for (run_id, stream_id, accepted_scores, matching_runs, observed_ops) in [
            (
                "forge_run_stream_a",
                "s1_api",
                "0",
                "1",
                vec!["definition", "references"],
            ),
            (
                "forge_run_stream_b",
                "s2_tests",
                "2",
                "6",
                vec!["definition", "references", "diagnostics"],
            ),
        ] {
            kernel
                .record_forge_run_event_with_evidence_fields(
                    ev(run_id, "run_started", "branch=forge/fleet_lane"),
                    &mdx_core::GovernedWriteIdentity::local_demo("human:dev"),
                    &[
                        ("repo_id", "java_service"),
                        ("repo_primary_language", "java"),
                        ("language_pack_id", "java-maven"),
                        ("repo_profile_detected_language_packs", "java-maven"),
                        ("selected_checks", "mvn test"),
                        ("selected_checks_source", "fleet_stream_checks"),
                        ("fleet_stream_id", stream_id),
                        ("fleet_stream_semantic_strategy_id", "stream_contract_guard"),
                        (
                            "fleet_stream_required_semantic_operations",
                            "definition\nreferences\ndiagnostics",
                        ),
                        (
                            "fleet_stream_proof_bias",
                            "interface_compatibility_and_behavioral_check",
                        ),
                        ("builder_casting_status", "EVIDENCE_BACKED_SLOT_READY"),
                        ("builder_casting_selected_slot", "OPUS"),
                        ("builder_casting_recommended_slot", "OPUS"),
                        (
                            "builder_casting_recommended_model_profile_id",
                            "codex_anthropic_responses_profile",
                        ),
                        ("builder_casting_recommended_provider_family", "anthropic"),
                        ("builder_casting_recommended_model_id", "claude-opus-4-8"),
                        ("builder_casting_matching_eval_score_count", "2"),
                        ("builder_casting_accepted_eval_score_count", accepted_scores),
                        ("builder_casting_matching_run_count", matching_runs),
                        ("builder_casting_done_rate_pct", "88"),
                        ("builder_casting_grants_execution_authority", "false"),
                    ],
                )
                .expect("run started");
            for operation in observed_ops {
                kernel
                    .record_forge_run_event(ev(
                        run_id,
                        "tool_executed",
                        &format!("semantic_query status=OK operation={operation}"),
                    ))
                    .expect("semantic operation");
            }
            kernel
                .record_forge_run_event(ev(run_id, "check_passed", "mvn test"))
                .expect("check passed");
            kernel
                .record_forge_run_event(ev(
                    run_id,
                    "run_finished",
                    "RUN_FINISHED_DONE branch=forge/fleet_lane",
                ))
                .expect("run finished");
            kernel
                .record_fleet_run_event(mdx_core::FleetRunEvent {
                    tenant_id: "t",
                    actor_id: "human:dev",
                    fleet_id: "fleet_1",
                    event: "stream_finished",
                    stream_id,
                    forge_run_id: run_id,
                    detail: "stream done",
                })
                .expect("stream finished");
        }

        let body = render(
            &kernel,
            "forge_run_stream_a",
            None,
            &std::collections::BTreeMap::new(),
        );
        let value: serde_json::Value = serde_json::from_str(&body).expect("valid json");
        let selection = &value["repo_intelligence"]["fleet_selection"];

        assert_eq!(selection["role"], "fleet_stream_candidate");
        assert_eq!(selection["selection_required_for_delivery"], true);
        assert_eq!(selection["current_run_is_recommended"], false);
        assert_eq!(selection["recommended_stream_id"], "s2_tests");
        assert_eq!(selection["recommended_run_id"], "forge_run_stream_b");
        assert_eq!(selection["candidate_count"], 2);
        assert_eq!(
            selection["candidates"][0]["strategy_semantic_operations_satisfied"],
            true
        );
        assert_eq!(selection["candidates"][1]["stream_id"], "s1_api");
        assert_eq!(
            selection["candidates"][1]["missing_required_semantic_operations"][0],
            "diagnostics"
        );
    }

    #[test]
    fn review_packet_exposes_fleet_integration_semantic_strategy_gaps() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event_with_evidence_fields(
                ev(
                    "forge_run_fleet_integration",
                    "run_started",
                    "fleet=f integration repo_root=/tmp/repo",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("human:dev"),
                &[
                    ("repo_primary_language", "java"),
                    ("language_pack_id", "java-maven"),
                    ("repo_profile_detected_language_packs", "java-maven"),
                    (
                        "repo_profile_semantic_intelligence",
                        "semantic_query:file_outline,semantic_query:dependency_map",
                    ),
                    (
                        "repo_profile_semantic_tool_readiness",
                        "jdtls=missing,fallback_index=available",
                    ),
                    ("selected_checks", "mvn test"),
                    (
                        "selected_checks_source",
                        "fleet_integration_full_suite_checks",
                    ),
                    ("principal_orientation_gate_required", "true"),
                    ("principal_orientation_gate_tool", "semantic_query"),
                    ("principal_orientation_gate_grants_authority", "false"),
                    (
                        "fleet_integration_strategy_id",
                        "integration_contract_wiring",
                    ),
                    (
                        "fleet_integration_strategy_summary",
                        "preserve stream interfaces, apply declared registrations, and prove the merged branch as one system",
                    ),
                    (
                        "fleet_integration_required_semantic_operations",
                        "file_outline\ndependency_map\nreferences\ndiagnostics",
                    ),
                    (
                        "fleet_integration_proof_bias",
                        "full_suite_integration_and_contract_alignment",
                    ),
                ],
            )
            .expect("run started");
        for operation in ["file_outline", "dependency_map"] {
            kernel
                .record_forge_run_event(ev(
                    "forge_run_fleet_integration",
                    "tool_executed",
                    &format!("semantic_query status=OK operation={operation}"),
                ))
                .expect("semantic query");
        }

        let body = render(
            &kernel,
            "forge_run_fleet_integration",
            None,
            &std::collections::BTreeMap::new(),
        );
        let value: serde_json::Value = serde_json::from_str(&body).expect("valid json");

        assert_eq!(
            value["repo_intelligence"]["fleet_integration"]["role"],
            "integration"
        );
        assert_eq!(
            value["repo_intelligence"]["fleet_integration"]["strategy_id"],
            "integration_contract_wiring"
        );
        assert_eq!(
            value["repo_intelligence"]["fleet_integration"]["strategy_semantic_operations_satisfied"],
            false
        );
        assert_eq!(
            value["repo_intelligence"]["fleet_integration"]["missing_required_semantic_operations"]
                [0],
            "references"
        );
        assert_eq!(
            value["repo_intelligence"]["fleet_integration"]["missing_required_semantic_operations"]
                [1],
            "diagnostics"
        );
    }

    #[test]
    fn review_status_is_derived_from_the_trail() {
        // A working run reads as working; a finished-done run as ready; a
        // shipped run as shipped. Derived once, not inferred per surface.
        let working = RunFold {
            found: true,
            status: "working".to_string(),
            finished: false,
            branch: String::new(),
            branch_sha: String::new(),
            work_item_id: String::new(),
            model: String::new(),
            turns: 0,
            model_calls: 0,
            tool_calls: 0,
            checks_passed: 0,
            checks_failed: 0,
            check_names: Vec::new(),
            baseline_checks_passed: 0,
            baseline_checks_failed: 0,
            baseline_check_names: Vec::new(),
            standards: Vec::new(),
            repo_id: String::new(),
            repo_primary_language: String::new(),
            language_pack_id: String::new(),
            repo_profile_detected_language_packs: Vec::new(),
            repo_profile_detected_files: Vec::new(),
            repo_profile_quality_signals: Vec::new(),
            repo_profile_standards_sources: Vec::new(),
            repo_profile_standards_source_fingerprints: Vec::new(),
            repo_profile_standards_source_summaries: Vec::new(),
            repo_profile_review_axes: Vec::new(),
            repo_profile_principal_review_gates: Vec::new(),
            repo_profile_language_pack_guidance: Vec::new(),
            repo_profile_semantic_intelligence: Vec::new(),
            repo_profile_semantic_tool_readiness: Vec::new(),
            repo_profile_toolchain_readiness: Vec::new(),
            repo_profile_suggested_checks: Vec::new(),
            repo_profile_artifact_patterns: Vec::new(),
            repo_profile_proof_plan_status: String::new(),
            repo_profile_proof_plan_next_action: String::new(),
            repo_profile_proof_plan_summary: String::new(),
            selected_checks: Vec::new(),
            selected_checks_source: String::new(),
            work_classification_recommended_shape: String::new(),
            work_classification_task_class: String::new(),
            work_classification_complexity_tier: String::new(),
            work_classification_confidence_pct: 0,
            work_classification_rationale: String::new(),
            work_classification_source: String::new(),
            work_classification_grants_execution_authority: false,
            language_task_corpus_id: String::new(),
            language_task_alignment_status: String::new(),
            language_task_alignment_source: String::new(),
            language_task_class: String::new(),
            language_task_complexity_tier: String::new(),
            language_task_visible_check: String::new(),
            language_task_hidden_check_slot: String::new(),
            language_task_artifact_noise_expected: Vec::new(),
            language_task_required_principal_review_gates: Vec::new(),
            language_task_engineering_facets: String::new(),
            language_task_evaluation_oracle: String::new(),
            language_task_human_timebox_minutes: 0,
            language_task_contamination_policy: String::new(),
            language_task_alignment_grants_execution_authority: false,
            repo_profile_source: String::new(),
            repo_profile_grants_execution_authority: false,
            suggested_checks_are_authority: false,
            principal_orientation_gate_required: false,
            principal_orientation_gate_tool: String::new(),
            principal_orientation_gate_observed: false,
            principal_orientation_gate_grants_authority: false,
            semantic_query_operations_observed: Vec::new(),
            event_count: 1,
            ..RunFold::default()
        };
        let no_ship = ShipDecision {
            decided: false,
            reason: String::new(),
            receipt_id: String::new(),
        };
        assert_eq!(review_status(&working, &no_ship).0, "working");

        let mut done = working;
        done.finished = true;
        done.status = "done".to_string();
        assert_eq!(review_status(&done, &no_ship).0, "ready_for_review");

        let shipped = ShipDecision {
            decided: true,
            reason: "looks right".to_string(),
            receipt_id: "r".to_string(),
        };
        assert_eq!(review_status(&done, &shipped).0, "shipped");
    }

    #[test]
    fn no_change_run_status_is_visible_in_review_packet_status() {
        assert_eq!(
            status_from("status=RUN_FINISHED_NO_CHANGE turns=2 files_changed=0"),
            "no_change"
        );
    }

    #[test]
    fn proof_coverage_uses_repo_suggested_checks_for_legacy_runs() {
        let fold = RunFold {
            repo_profile_suggested_checks: vec!["npm test".to_string()],
            check_names: vec![("run_command npm test exit=0".to_string(), true)],
            ..RunFold::default()
        };

        let coverage = proof_coverage(&fold);

        assert_eq!(coverage["status"], "satisfied");
        assert_eq!(coverage["selected_checks"][0], "npm test");
        assert_eq!(coverage["selected_checks_source"], "repo_profile_fallback");
    }

    #[test]
    fn proof_coverage_separates_setup_from_behavioral_proof() {
        let fold = RunFold {
            selected_checks: vec!["npm ci".to_string(), "npm test".to_string()],
            selected_checks_source: "operator_supplied_plus_repo_setup".to_string(),
            check_names: vec![
                ("run_command npm ci exit=0".to_string(), true),
                ("run_command npm test exit=0".to_string(), true),
            ],
            ..RunFold::default()
        };

        let coverage = proof_coverage(&fold);

        assert_eq!(coverage["status"], "satisfied");
        assert_eq!(coverage["setup_commands"][0], "npm ci");
        assert_eq!(coverage["proof_commands"][0], "npm test");
        assert_eq!(coverage["satisfied_setup_commands"][0], "npm ci");
        assert_eq!(coverage["satisfied_proof_commands"][0], "npm test");
    }

    #[test]
    fn proof_coverage_treats_combined_setup_and_test_as_behavioral_proof() {
        let command = "npm install --no-package-lock && npm test";
        let fold = RunFold {
            selected_checks: vec![command.to_string()],
            selected_checks_source: "operator_supplied".to_string(),
            check_names: vec![(format!("run_command {command} exit=0"), true)],
            ..RunFold::default()
        };

        let coverage = proof_coverage(&fold);

        assert_eq!(coverage["status"], "satisfied");
        assert!(
            coverage["setup_commands"]
                .as_array()
                .expect("setup")
                .is_empty()
        );
        assert_eq!(coverage["proof_commands"][0], command);
        assert_eq!(coverage["satisfied_proof_commands"][0], command);
    }

    #[test]
    fn proof_coverage_never_treats_setup_only_as_behavioral_proof() {
        let fold = RunFold {
            selected_checks: vec!["npm ci".to_string()],
            selected_checks_source: "operator_supplied".to_string(),
            check_names: vec![("run_command npm ci exit=0".to_string(), true)],
            ..RunFold::default()
        };

        let coverage = proof_coverage(&fold);

        assert_eq!(coverage["status"], "no_proof_command");
        assert_eq!(coverage["setup_commands"][0], "npm ci");
        assert!(
            coverage["proof_commands"]
                .as_array()
                .expect("proof")
                .is_empty()
        );
    }

    #[test]
    fn proof_coverage_marks_no_change_as_attention_even_when_checks_passed() {
        let fold = RunFold {
            status: "no_change".to_string(),
            selected_checks: vec!["npm test".to_string()],
            selected_checks_source: "operator_supplied".to_string(),
            check_names: vec![("run_command npm test exit=0".to_string(), true)],
            ..RunFold::default()
        };

        let coverage = proof_coverage(&fold);

        assert_eq!(coverage["status"], "no_change");
        assert_eq!(coverage["satisfied_checks"][0], "npm test");
        assert!(
            coverage["summary"]
                .as_str()
                .expect("summary")
                .contains("baseline health")
        );
    }

    #[test]
    fn behavior_proof_is_weak_without_test_diff_or_related_test_orientation() {
        let fold = RunFold {
            status: "done".to_string(),
            selected_checks: vec!["npm test".to_string()],
            selected_checks_source: "operator_supplied".to_string(),
            check_names: vec![("run_command npm test exit=0".to_string(), true)],
            ..RunFold::default()
        };
        let coverage = proof_coverage(&fold);
        let behavior = behavior_proof(&fold, &coverage, 1, &[]);

        assert_eq!(coverage["status"], "satisfied");
        assert_eq!(behavior["status"], "weak");
        assert!(
            behavior["summary"]
                .as_str()
                .expect("summary")
                .contains("did not see a test/fixture diff")
        );
    }

    #[test]
    fn behavior_proof_is_covered_by_related_test_orientation() {
        let fold = RunFold {
            status: "done".to_string(),
            selected_checks: vec!["swift test".to_string()],
            selected_checks_source: "repo_profile_inferred".to_string(),
            check_names: vec![("run_command swift test exit=0".to_string(), true)],
            semantic_query_operations_observed: vec!["related_tests".to_string()],
            ..RunFold::default()
        };
        let coverage = proof_coverage(&fold);
        let behavior = behavior_proof(&fold, &coverage, 1, &[]);

        assert_eq!(behavior["status"], "covered");
        assert_eq!(behavior["related_tests_observed"], true);
    }

    #[test]
    fn review_packet_suppresses_build_artifacts_without_hiding_their_evidence() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event_with_evidence_fields(
                ev(
                    "forge_run_artifacts",
                    "run_started",
                    "branch=forge/run_artifacts",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("human:dev"),
                &[
                    ("repo_id", "swift_canary"),
                    ("repo_primary_language", "swift"),
                    ("language_pack_id", "swift-spm"),
                    ("repo_profile_detected_language_packs", "swift-spm"),
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
                        "repo_profile_semantic_intelligence",
                        "lsp:sourcekit-lsp,symbols:workspace_symbol_and_definition,references:text_document_references",
                    ),
                    (
                        "repo_profile_semantic_tool_readiness",
                        "anchor:rg=available,lsp:sourcekit-lsp=available,structural:tree_sitter_swift=missing",
                    ),
                    (
                        "repo_profile_semantic_session_id",
                        "semantic_session_fnv1a64:1111111111111111",
                    ),
                    ("repo_profile_semantic_fallback_index_status", "indexed"),
                    ("repo_profile_semantic_source_file_count", "4"),
                    ("repo_profile_semantic_indexed_file_count", "3"),
                    ("repo_profile_semantic_indexed_symbol_count", "7"),
                    ("repo_profile_semantic_related_test_anchor_count", "1"),
                    (
                        "repo_profile_semantic_session_grants_execution_authority",
                        "false",
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
                    ("work_classification_source", "mdx_core::classify_forge_work"),
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
                    ("language_task_alignment_grants_execution_authority", "false"),
                    ("repo_profile_source", "mdx_server::forge_repo_profile"),
                    ("repo_profile_grants_execution_authority", "false"),
                    ("suggested_checks_are_authority", "false"),
                    ("principal_orientation_gate_required", "true"),
                    ("principal_orientation_gate_tool", "semantic_query"),
                    ("principal_orientation_gate_grants_authority", "false"),
                    (
                        "repo_intake_generated_from",
                        "repo_readiness+repo_task_scout+language_task_alignment",
                    ),
                    ("repo_intake_readiness_status", "READY_FOR_MEDIUM_HIGH_WORK"),
                    ("repo_intake_medium_high_work_ready", "true"),
                    (
                        "repo_intake_safe_next_move",
                        "Start with the selected check, orient with semantic_query, then review the generated PR handoff.",
                    ),
                    (
                        "repo_intake_semantic_orientation_operations",
                        "capabilities,file_outline,related_tests",
                    ),
                    (
                        "repo_intake_proof_strategy",
                        "repo_profile_inferred:selected_checks_then_successful_semantic_orientation_then_review_packet",
                    ),
                    (
                        "repo_intake_first_run_task_ids",
                        "swift-spm-first-run-behavior-check,swift-spm-medium-refactor-scout",
                    ),
                    ("repo_intake_scout_status", "TASKS_FOUND"),
                    ("repo_intake_scout_candidate_count", "1"),
                    ("repo_intake_scout_candidate_kinds", "todo_cleanup"),
                    ("repo_intake_scout_candidate_paths", "Sources/App/Slug.swift"),
                    ("repo_intake_write_scope_hint", "Sources/App/Slug.swift"),
                    ("repo_intake_off_limits_patterns", ".build/**,DerivedData/**"),
                    (
                        "repo_intake_review_focus",
                        "the TODO/FIXME intent is preserved,adjacent behavior has focused proof",
                    ),
                    ("repo_intake_source_host", "github"),
                    ("repo_intake_origin_url_present", "true"),
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
                        "parallel_candidate_primary_run_id",
                        "forge_run_primary_parallel",
                    ),
                    ("parallel_candidate_index", "2"),
                    ("parallel_candidate_count", "4"),
                    ("parallel_candidate_write_scope", "Sources/App/Slug.swift"),
                    ("parallel_candidate_strategy_id", "test_first_behavior"),
                    (
                        "parallel_candidate_strategy_summary",
                        "behavior-proof-led implementation",
                    ),
                    (
                        "parallel_candidate_required_semantic_operations",
                        "related_tests,references,dependency_map,diagnostics",
                    ),
                    (
                        "parallel_candidate_proof_bias",
                        "red_green_or_related_test_first",
                    ),
                ],
            )
            .expect("run started");
        kernel
            .record_forge_run_event(ev(
                "forge_run_artifacts",
                "tool_executed",
                "semantic_query status=OK operation=related_tests",
            ))
            .expect("semantic query");
        kernel
            .record_forge_run_event_with_evidence_fields(
                ev(
                    "forge_run_artifacts",
                    "evidence_appended",
                    "semantic_preflight_observed anchors=4 observed_ops=3 planned_ops=4",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("agent:forge"),
                &[
                    ("semantic_preflight_status", "observed"),
                    (
                        "semantic_preflight_source",
                        "forge_loop_runner::semantic_mission_pack",
                    ),
                    (
                        "semantic_preflight_session_id",
                        "semantic_session_fnv1a64:1111111111111111",
                    ),
                    ("semantic_preflight_anchor_count", "4"),
                    ("semantic_preflight_planned_operation_count", "4"),
                    ("semantic_preflight_observed_operation_count", "3"),
                    (
                        "semantic_preflight_planned_operations",
                        "symbol_graph,hover,dependency_map,diagnostics",
                    ),
                    (
                        "semantic_preflight_observed_operations",
                        "symbol_graph,dependency_map,diagnostics",
                    ),
                    ("semantic_preflight_indexed_file_count", "3"),
                    ("semantic_preflight_indexed_symbol_count", "7"),
                    ("semantic_preflight_related_test_anchor_count", "1"),
                    ("semantic_preflight_duration_ms", "12"),
                    ("semantic_preflight_grants_execution_authority", "false"),
                ],
            )
            .expect("semantic preflight evidence");
        kernel
            .record_forge_run_event(ev(
                "forge_run_artifacts",
                "check_failed",
                "run_command swift test exit=1",
            ))
            .expect("first check failed");
        kernel
            .record_forge_run_event(ev(
                "forge_run_artifacts",
                "check_passed",
                "run_command swift test exit=0",
            ))
            .expect("check passed");
        kernel
            .record_forge_run_event(ev(
                "forge_run_artifacts",
                "run_finished",
                "status=RUN_FINISHED_DONE",
            ))
            .expect("run finished");
        let eval_report = kernel
            .ingest_forge_fleet_eval_result_local(mdx_core::ForgeFleetEvalResultSubmission {
                tenant_id: "local_tenant",
                actor_id: "human:dev",
                actor_role: "worker",
                submission_id: "candidate_eval_evidence_for_forge_run_artifacts",
                benchmark_task_id: "feature_medium",
                language_task_corpus_id: "swift-spm-feature-medium",
                language_pack_id: "swift-spm",
                repo_family: "Swift Package Manager",
                runner_id: "codex_cli_external_worker",
                model_profile_id: "codex_xai_responses_profile",
                output_artifact_ref:
                    "quarantine://forge/fleet-eval/runs/forge_run_artifacts/review-packet.json",
                output_artifact_sha256:
                    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                transcript_ref: "quarantine://forge/fleet-eval/runs/forge_run_artifacts/events.jsonl",
                claimed_check: "swift test",
                hidden_check_slot: "API behavior fixture",
                artifact_noise_expected: ".build/**",
                principal_review_gate_results:
                    "behavior=pending;tests=pending;stack_idioms=pending;compatibility=pending;security=pending;maintainability=pending",
                standards_source_fingerprints: "AGENTS.md=fnv1a64:1111111111111111",
                artifact_filter_summary: "artifact_summary=none",
                diff_language_pack_impact: "swift-spm",
                principal_engineer_verdict: "pending_principal_engineer_review",
                pr_handoff_ref:
                    "quarantine://forge/fleet-eval/runs/forge_run_artifacts/pr-handoff.md",
            })
            .expect("eval evidence ingested");
        assert_eq!(
            eval_report.status,
            "FLEET_EVAL_RESULT_QUARANTINED_PENDING_MDX_GATES"
        );
        kernel
            .record_forge_run_event_with_evidence_fields(
                ev(
                    "forge_run_artifacts",
                    "evidence_appended",
                    "eval_quality_gate_assessed status=READY_FOR_PRINCIPAL_REVIEW machine_gates_passed=true failed_gates=none",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("agent:forge_eval"),
                &[
                    (
                        "eval_quality_gate_status",
                        "READY_FOR_PRINCIPAL_REVIEW",
                    ),
                    ("eval_machine_quality_gates_passed", "true"),
                    ("eval_can_be_scored_after_principal_review", "true"),
                    ("eval_passed_gate_count", "10"),
                    ("eval_failed_gate_count", "0"),
                    (
                        "eval_passed_gates",
                        "terminal_done,real_diff_present,proof_coverage_satisfied",
                    ),
                    ("eval_failed_gates", ""),
                    (
                        "eval_result_receipt_id",
                        eval_report.result_receipt_id.as_str(),
                    ),
                    ("eval_accepts_for_scoreboard", "false"),
                    ("eval_grants_execution_authority", "false"),
                ],
            )
            .expect("eval quality gate evidence");
        let diff_files = vec![
            crate::forge_diff_route::RunDiffFile {
                path: ".build/workspace-state.json".to_string(),
                added: 12,
                removed: 2,
            },
            crate::forge_diff_route::RunDiffFile {
                path: "DerivedData/App/Index.noindex".to_string(),
                added: 4,
                removed: 0,
            },
            crate::forge_diff_route::RunDiffFile {
                path: "Sources/App/Slug.swift".to_string(),
                added: 5,
                removed: 1,
            },
        ];
        let mut candidate_diff_metrics = std::collections::BTreeMap::new();
        candidate_diff_metrics.insert(
            "forge_run_artifacts".to_string(),
            CandidateDiffMetrics::from_files(Some(diff_files.clone())),
        );
        let body = render(
            &kernel,
            "forge_run_artifacts",
            Some(diff_files),
            &candidate_diff_metrics,
        );
        let value: serde_json::Value = serde_json::from_str(&body).expect("valid json");

        assert_eq!(value["diff"]["file_count"], 3);
        assert_eq!(value["repo_intelligence"]["repo_id"], "swift_canary");
        assert_eq!(value["repo_intelligence"]["language_pack_id"], "swift-spm");
        assert_eq!(
            value["repo_intelligence"]["detected_language_packs"][0],
            "swift-spm"
        );
        assert_eq!(
            value["repo_intelligence"]["review_axes"][0],
            "behavioral-xctest-coverage"
        );
        assert_eq!(
            value["repo_intelligence"]["principal_review_gates"][0],
            "behavior:changed_behavior_is_explicit"
        );
        assert_eq!(
            value["repo_intelligence"]["language_pack_guidance"][0],
            "source_of_truth:package_swift_targets_tests"
        );
        assert_eq!(
            value["repo_intelligence"]["semantic_intelligence"][0],
            "lsp:sourcekit-lsp"
        );
        assert_eq!(
            value["repo_intelligence"]["semantic_tool_readiness"][1],
            "lsp:sourcekit-lsp=available"
        );
        assert_eq!(
            value["repo_intelligence"]["toolchain_readiness"][0],
            "tool:swift=available"
        );
        assert_eq!(value["repo_intelligence"]["proof_plan"]["status"], "ready");
        assert_eq!(
            value["repo_intelligence"]["proof_plan"]["next_action"],
            "Run swift test."
        );
        assert!(
            value["repo_intelligence"]["proof_plan"]["summary"]
                .as_str()
                .expect("proof plan summary")
                .contains("local Swift toolchain is available")
        );
        assert!(
            value["principal_review"]["checklist"]
                .as_array()
                .expect("checklist")
                .iter()
                .any(|item| item["kind"] == "toolchain_readiness"
                    && item["line"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("Proof plan is ready"))
        );
        assert_eq!(
            value["repo_intelligence"]["quality_signals"][1],
            "tests:swiftpm-tests"
        );
        assert_eq!(
            value["repo_intelligence"]["standards_sources"][0],
            "AGENTS.md"
        );
        assert_eq!(
            value["repo_intelligence"]["standards_sources"][1],
            ".github/CODEOWNERS"
        );
        assert_eq!(
            value["repo_intelligence"]["standards_source_fingerprints"][0],
            "AGENTS.md=fnv1a64:1111111111111111"
        );
        assert_eq!(
            value["repo_intelligence"]["standards_source_summaries"][0],
            "AGENTS.md=agent_instructions+mentions_tests"
        );
        assert_eq!(
            value["repo_intelligence"]["repo_profile_grants_execution_authority"],
            false
        );
        assert_eq!(
            value["repo_intelligence"]["suggested_checks_are_authority"],
            false
        );
        assert_eq!(
            value["repo_intelligence"]["semantic_session"]["session_id"],
            "semantic_session_fnv1a64:1111111111111111"
        );
        assert_eq!(
            value["repo_intelligence"]["semantic_session"]["fallback_index_status"],
            "indexed"
        );
        assert_eq!(
            value["repo_intelligence"]["semantic_session"]["indexed_file_count"],
            3
        );
        assert_eq!(
            value["repo_intelligence"]["semantic_session"]["indexed_symbol_count"],
            7
        );
        assert_eq!(
            value["repo_intelligence"]["semantic_session"]["related_test_anchor_count"],
            1
        );
        assert_eq!(
            value["repo_intelligence"]["semantic_session"]["grants_execution_authority"],
            false
        );
        assert!(
            value["principal_review"]["checklist"]
                .as_array()
                .expect("checklist")
                .iter()
                .any(|item| item["kind"] == "semantic_session"
                    && item["status"] == "satisfied"
                    && item["evidence"]["indexed_file_count"] == 3)
        );
        assert_eq!(
            value["repo_intelligence"]["semantic_preflight"]["status"],
            "observed"
        );
        assert_eq!(
            value["repo_intelligence"]["semantic_preflight"]["anchor_count"],
            4
        );
        assert_eq!(
            value["repo_intelligence"]["semantic_preflight"]["planned_operations"][2],
            "dependency_map"
        );
        assert_eq!(
            value["repo_intelligence"]["semantic_preflight"]["observed_operations"][0],
            "symbol_graph"
        );
        assert_eq!(
            value["repo_intelligence"]["semantic_preflight"]["grants_execution_authority"],
            false
        );
        assert!(
            value["principal_review"]["checklist"]
                .as_array()
                .expect("checklist")
                .iter()
                .any(|item| item["kind"] == "semantic_preflight"
                    && item["status"] == "satisfied"
                    && item["evidence"]["anchor_count"] == 4)
        );
        assert_eq!(
            value["repo_intelligence"]["parallel_candidate"]["role"],
            "candidate"
        );
        assert_eq!(
            value["repo_intelligence"]["parallel_candidate"]["primary_run_id"],
            "forge_run_primary_parallel"
        );
        assert_eq!(value["repo_intelligence"]["parallel_candidate"]["index"], 2);
        assert_eq!(value["repo_intelligence"]["parallel_candidate"]["count"], 4);
        assert_eq!(
            value["repo_intelligence"]["parallel_candidate"]["write_scope"][0],
            "Sources/App/Slug.swift"
        );
        assert_eq!(
            value["repo_intelligence"]["parallel_candidate"]["strategy_id"],
            "test_first_behavior"
        );
        assert_eq!(
            value["repo_intelligence"]["parallel_candidate"]["strategy_summary"],
            "behavior-proof-led implementation"
        );
        assert_eq!(
            value["repo_intelligence"]["parallel_candidate"]["required_semantic_operations"][0],
            "related_tests"
        );
        assert_eq!(
            value["repo_intelligence"]["parallel_candidate"]["proof_bias"],
            "red_green_or_related_test_first"
        );
        assert_eq!(
            value["repo_intelligence"]["parallel_candidate"]["grants_execution_authority"],
            false
        );
        assert_eq!(
            value["eval_quality_gate_assessment"]["generated_from"],
            "forge.run.event.eval_quality_gate_assessed"
        );
        assert_eq!(value["eval_quality_gate_assessment"]["recorded"], true);
        assert_eq!(
            value["eval_quality_gate_assessment"]["status"],
            "READY_FOR_PRINCIPAL_REVIEW"
        );
        assert_eq!(
            value["eval_quality_gate_assessment"]["machine_quality_gates_passed"],
            true
        );
        assert_eq!(
            value["eval_quality_gate_assessment"]["can_be_scored_after_principal_review"],
            true
        );
        assert_eq!(
            value["eval_quality_gate_assessment"]["passed_gates"][0],
            "terminal_done"
        );
        assert_eq!(
            value["eval_quality_gate_assessment"]["accepted_for_scoreboard"],
            false
        );
        assert_eq!(
            value["eval_quality_gate_assessment"]["grants_execution_authority"],
            false
        );
        assert_eq!(
            value["candidate_comparison"]["generated_from"],
            "forge.run.event.parallel_candidates"
        );
        assert_eq!(
            value["candidate_comparison"]["primary_run_id"],
            "forge_run_primary_parallel"
        );
        assert_eq!(
            value["candidate_comparison"]["current_run_id"],
            "forge_run_artifacts"
        );
        assert_eq!(value["candidate_comparison"]["current_rank"], 1);
        assert_eq!(
            value["candidate_comparison"]["recommended_run_id"],
            "forge_run_artifacts"
        );
        assert_eq!(
            value["candidate_comparison"]["comparison_basis"][0],
            "terminal_status"
        );
        assert!(
            value["candidate_comparison"]["comparison_basis"]
                .as_array()
                .expect("comparison basis")
                .iter()
                .any(|item| item == "quarantined_eval_evidence")
        );
        assert_eq!(value["candidate_comparison"]["diff_quality_included"], true);
        assert_eq!(
            value["candidate_comparison"]["eval_evidence_included"],
            true
        );
        assert_eq!(
            value["candidate_comparison"]["model_judgment_included"],
            false
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["run_id"],
            "forge_run_artifacts"
        );
        assert_eq!(value["candidate_comparison"]["candidates"][0]["rank"], 1);
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["strategy_id"],
            "test_first_behavior"
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["strategy_summary"],
            "behavior-proof-led implementation"
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["required_semantic_operations"][0],
            "related_tests"
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["proof_bias"],
            "red_green_or_related_test_first"
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["strategy_semantic_operations_satisfied"],
            false
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["missing_required_semantic_operations"]
                [0],
            "references"
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["related_tests_observed"],
            true
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["diff_quality"]["available"],
            true
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["diff_quality"]["real_change_count"],
            1
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["diff_quality"]["artifact_count"],
            2
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["diff_quality"]["model_judgment_included"],
            false
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["diff_quality"]["grants_execution_authority"],
            false
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["eval_evidence"]["present"],
            true
        );
        assert!(
            !value["candidate_comparison"]["candidates"][0]["eval_evidence"]["result_receipt_id"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .is_empty()
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["eval_evidence"]["language_task_corpus_id"],
            "swift-spm-feature-medium"
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["eval_evidence"]["output_quarantined"],
            true
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["eval_evidence"]["accepted_for_scoreboard"],
            false
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["eval_evidence"]["mdx_quality_gates_required"],
            true
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["eval_evidence"]["blocked_reason"],
            "pending_mdx_quality_gates"
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["eval_evidence"]["model_judgment_included"],
            false
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["eval_evidence"]["grants_execution_authority"],
            false
        );
        assert!(
            value["candidate_comparison"]["candidates"][0]["score_basis"]
                .as_array()
                .expect("score basis")
                .iter()
                .any(|item| item == "quarantined_eval_evidence=true")
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["proof_quality"]["proof_coverage_status"],
            "satisfied"
        );
        assert_eq!(
            value["candidate_comparison"]["candidates"][0]["proof_quality"]["behavior_proof_status"],
            "covered"
        );
        assert!(
            value["candidate_comparison"]["candidates"][0]["score_basis"]
                .as_array()
                .expect("score basis")
                .iter()
                .any(|item| item == "behavior_proof_status=covered")
        );
        assert_eq!(
            value["candidate_comparison"]["grants_execution_authority"],
            false
        );
        assert_eq!(
            value["pr_handoff"]["candidate_recommendation"]["generated_from"],
            "candidate_comparison"
        );
        assert_eq!(
            value["pr_handoff"]["candidate_recommendation"]["recommended_run_id"],
            "forge_run_artifacts"
        );
        assert_eq!(
            value["pr_handoff"]["candidate_recommendation"]["current_run_is_recommended"],
            true
        );
        assert_eq!(
            value["pr_handoff"]["candidate_recommendation"]["diff_quality_included"],
            true
        );
        assert_eq!(
            value["pr_handoff"]["candidate_recommendation"]["eval_evidence_included"],
            true
        );
        assert_eq!(
            value["pr_handoff"]["candidate_recommendation"]["model_judgment_included"],
            false
        );
        assert_eq!(
            value["pr_handoff"]["candidate_recommendation"]["grants_execution_authority"],
            false
        );
        assert_eq!(
            value["repo_intake"]["generated_from"],
            "repo_readiness+repo_task_scout+language_task_alignment"
        );
        assert_eq!(
            value["repo_intake"]["readiness_status"],
            "READY_FOR_MEDIUM_HIGH_WORK"
        );
        assert_eq!(value["repo_intake"]["medium_high_work_ready"], true);
        assert_eq!(
            value["repo_intake"]["semantic_orientation_operations"][2],
            "related_tests"
        );
        assert_eq!(
            value["repo_intake"]["proof_strategy"],
            "repo_profile_inferred:selected_checks_then_successful_semantic_orientation_then_review_packet"
        );
        assert_eq!(
            value["repo_intake"]["write_scope_hint"][0],
            "Sources/App/Slug.swift"
        );
        assert_eq!(value["repo_intake"]["source_host"], "github");
        assert_eq!(value["repo_intake"]["origin_url_recorded"], false);
        assert_eq!(value["repo_intake"]["network_call_allowed"], false);
        assert_eq!(value["repo_intake"]["grants_execution_authority"], false);
        assert!(
            value["principal_review"]["checklist"]
                .as_array()
                .expect("checklist")
                .iter()
                .any(|item| item["kind"] == "repo_intake"
                    && item["line"]
                        .as_str()
                        .unwrap_or_default()
                        .contains("READY_FOR_MEDIUM_HIGH_WORK"))
        );
        assert_eq!(
            value["repo_intelligence"]["principal_orientation_gate"]["required"],
            true
        );
        assert_eq!(
            value["repo_intelligence"]["principal_orientation_gate"]["tool"],
            "semantic_query"
        );
        assert_eq!(
            value["repo_intelligence"]["principal_orientation_gate"]["observed"],
            true
        );
        assert_eq!(
            value["repo_intelligence"]["principal_orientation_gate"]["grants_authority"],
            false
        );
        assert_eq!(
            value["repo_intelligence"]["semantic_query_operations_observed"][0],
            "related_tests"
        );
        assert_eq!(value["repo_intelligence"]["related_tests_observed"], true);
        assert!(
            value["principal_review"]["checklist"]
                .as_array()
                .expect("checklist")
                .iter()
                .any(|item| item["kind"] == "principal_orientation_gate"
                    && item["status"] == "satisfied")
        );
        assert!(
            value["principal_review"]["checklist"]
                .as_array()
                .expect("checklist")
                .iter()
                .any(|item| item["kind"] == "semantic_query_operations"
                    && item["status"] == "satisfied"
                    && item["evidence"]["related_tests_observed"] == true)
        );
        assert_eq!(
            value["principal_review"]["generated_from"],
            "repo_intelligence+checks+diff"
        );
        assert_eq!(value["proof_coverage"]["status"], "satisfied");
        assert_eq!(
            value["proof_coverage"]["match_policy"],
            "exact_selected_command"
        );
        assert_eq!(
            value["proof_coverage"]["generated_from"],
            "repo_intelligence+selected_checks+observed_check_events"
        );
        assert_eq!(value["proof_coverage"]["selected_checks"][0], "swift test");
        assert_eq!(value["proof_coverage"]["satisfied_checks"][0], "swift test");
        assert_eq!(
            value["proof_coverage"]["selected_checks_source"],
            "repo_profile_inferred"
        );
        assert_eq!(
            value["repo_intelligence"]["selected_checks_source"],
            "repo_profile_inferred"
        );
        assert_eq!(value["behavior_proof"]["status"], "covered");
        assert_eq!(value["behavior_proof"]["source_diff_observed"], true);
        assert_eq!(value["behavior_proof"]["behavior_diff_observed"], false);
        assert_eq!(value["behavior_proof"]["related_tests_observed"], true);
        assert!(
            value["principal_review"]["checklist"]
                .as_array()
                .expect("checklist")
                .iter()
                .any(|item| item["kind"] == "behavior_proof"
                    && item["status"] == "satisfied"
                    && item["evidence"]["status"] == "covered")
        );
        assert_eq!(value["work_classification"]["task_class"], "feature");
        assert_eq!(value["work_classification"]["complexity_tier"], "medium");
        assert_eq!(
            value["work_classification"]["grants_execution_authority"],
            false
        );
        assert_eq!(
            value["language_task_alignment"]["task_corpus_id"],
            "swift-spm-feature-medium"
        );
        assert_eq!(
            value["language_task_alignment"]["hidden_check_slot"],
            "API behavior fixture"
        );
        assert_eq!(
            value["language_task_alignment"]["required_principal_review_gates"][0],
            "behavior:changed_behavior_is_explicit"
        );
        assert_eq!(
            value["language_task_alignment"]["grants_execution_authority"],
            false
        );
        assert!(
            value["principal_review"]["checklist"]
                .as_array()
                .expect("checklist")
                .iter()
                .any(|item| item["kind"] == "work_classification")
        );
        assert!(
            value["principal_review"]["checklist"]
                .as_array()
                .expect("checklist")
                .iter()
                .any(|item| item["kind"] == "language_task_alignment")
        );
        assert_eq!(
            value["proof_coverage"]["observed_checks"][0]["name"],
            "swift test"
        );
        assert!(
            value["proof_coverage"]["failed_checks"]
                .as_array()
                .expect("failed checks")
                .is_empty()
        );
        assert!(
            value["proof_coverage"]["summary"]
                .as_str()
                .expect("proof summary")
                .contains("all selected checks were observed passing")
        );
        assert_eq!(value["proof_scope"]["status"], "covered");
        assert_eq!(
            value["proof_scope"]["covered_language_packs"][0],
            "swift-spm"
        );
        assert!(
            value["proof_scope"]["summary"]
                .as_str()
                .expect("proof scope summary")
                .contains("Proof scope covered")
        );
        assert_eq!(value["principal_review"]["checklist"][0]["kind"], "checks");
        assert_eq!(
            value["principal_review"]["checklist"][0]["status"],
            "satisfied"
        );
        assert_eq!(
            value["principal_review"]["checklist"][1]["status"],
            "satisfied"
        );
        assert!(
            value["principal_review"]["checklist"]
                .as_array()
                .expect("checklist")
                .iter()
                .any(|item| item["line"]
                    == "Review against language-pack axis: behavioral-xctest-coverage")
        );
        assert!(
            value["principal_review"]["checklist"]
                .as_array()
                .expect("checklist")
                .iter()
                .any(|item| item["kind"] == "principal_review_gate")
        );
        assert!(
            value["principal_review"]["handoff_lines"][0]
                .as_str()
                .expect("line")
                .contains("swift-spm")
        );
        assert!(
            value["principal_review"]["authority_note"]
                .as_str()
                .expect("authority note")
                .contains("does not grant execution authority")
        );
        assert_eq!(
            value["pr_handoff"]["generated_from"],
            "review_packet+repo_intelligence+diff"
        );
        assert_eq!(value["pr_handoff"]["production_write_allowed"], false);
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("swift-spm")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("AGENTS.md")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("fnv1a64")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Diff language packs: swift-spm")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Principal orientation gate: required; tool=semantic_query; observed; grants_authority=false")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Semantic query operations observed: related_tests")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains(
                    "Candidate recommendation: current run forge_run_artifacts is recommended"
                )
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("diff_quality_included=true; model_judgment_included=false; grants_authority=false")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Repo intake semantic orientation")
        );
        assert!(
            value["pr_handoff"]["summary_lines"]
                .as_array()
                .expect("summary lines")
                .iter()
                .any(|line| line
                    .as_str()
                    .expect("line")
                    .contains("Repo intake: readiness=READY_FOR_MEDIUM_HIGH_WORK"))
        );
        assert!(
            value["pr_handoff"]["summary_lines"]
                .as_array()
                .expect("summary lines")
                .iter()
                .any(|line| line
                    .as_str()
                    .expect("line")
                    .contains("Principal orientation gate: required; tool=semantic_query; observed; grants_authority=false"))
        );
        assert!(
            value["pr_handoff"]["summary_lines"]
                .as_array()
                .expect("summary lines")
                .iter()
                .any(|line| line
                    .as_str()
                    .expect("line")
                    .contains("Semantic query operations observed: related_tests"))
        );
        assert!(
            value["pr_handoff"]["summary_lines"]
                .as_array()
                .expect("summary lines")
                .iter()
                .any(|line| line.as_str().expect("line").contains(
                    "Candidate recommendation: current run forge_run_artifacts is recommended"
                ))
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Proof coverage: Proof covered")
        );
        assert!(
            value["principal_review"]["handoff_lines"]
                .as_array()
                .expect("handoff lines")
                .iter()
                .any(|line| line
                    .as_str()
                    .expect("line")
                    .contains("Proof match policy: exact selected command"))
        );
        assert!(
            value["principal_review"]["handoff_lines"]
                .as_array()
                .expect("handoff lines")
                .iter()
                .any(|line| line
                    .as_str()
                    .expect("line")
                    .contains("Satisfied proof: swift test"))
        );
        assert!(
            value["pr_handoff"]["summary_lines"]
                .as_array()
                .expect("summary lines")
                .iter()
                .any(|line| line
                    .as_str()
                    .expect("line")
                    .contains("Proof match policy: exact selected command"))
        );
        assert!(
            value["pr_handoff"]["summary_lines"]
                .as_array()
                .expect("summary lines")
                .iter()
                .any(|line| line
                    .as_str()
                    .expect("line")
                    .contains("Satisfied proof: swift test"))
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Proof match policy: exact selected command")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Satisfied proof: swift test")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Missing proof: none recorded")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Failed proof: none recorded")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Proof scope: Proof scope covered")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Selected checks source: repo profile inferred")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Artifact noise: swiftpm build output: 1 file(s), +12/-2; xcode derived data: 1 file(s), +4/-0")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("does not push a branch, open a PR")
        );
        assert_eq!(value["diff"]["real_change_count"], 1);
        assert_eq!(value["diff"]["artifact_count"], 2);
        assert_eq!(
            value["diff"]["artifact_summary"][0]["reason"],
            "swiftpm build output"
        );
        assert_eq!(value["diff"]["artifact_summary"][0]["count"], 1);
        assert_eq!(
            value["diff"]["artifact_summary"][1]["reason"],
            "xcode derived data"
        );
        assert_eq!(value["diff"]["artifact_summary"][1]["count"], 1);
        assert_eq!(
            value["diff"]["artifact_files_sample"][0]["path"],
            ".build/workspace-state.json"
        );
        assert_eq!(
            value["diff"]["artifact_files_sample"][0]["reason"],
            "swiftpm build output"
        );
        assert_eq!(
            value["diff"]["language_pack_impact"][0]["language_pack_id"],
            "swift-spm"
        );
        assert_eq!(value["diff"]["language_pack_impact"][0]["file_count"], 1);
        assert_eq!(
            value["diff"]["sections"][0]["files"][0]["path"],
            "Sources/App/Slug.swift"
        );
    }

    #[test]
    fn candidate_scoring_prefers_behavioral_proof_over_setup_only_completion() {
        let weak_fold = RunFold {
            found: true,
            status: "done".to_string(),
            checks_passed: 1,
            selected_checks: vec!["npm ci".to_string()],
            selected_checks_source: "operator_supplied".to_string(),
            check_names: vec![("run_command npm ci exit=0".to_string(), true)],
            repo_profile_detected_language_packs: vec!["node".to_string()],
            turns: 4,
            ..RunFold::default()
        };
        let weak_metrics = CandidateDiffMetrics {
            available: true,
            file_count: 1,
            real_change_count: 1,
            added_total: 6,
            removed_total: 1,
            source_paths: vec!["src/slug.ts".to_string()],
            ..CandidateDiffMetrics::default()
        };
        let strong_fold = RunFold {
            found: true,
            status: "done".to_string(),
            checks_passed: 2,
            selected_checks: vec!["npm ci".to_string(), "npm test".to_string()],
            selected_checks_source: "operator_supplied_plus_repo_setup".to_string(),
            check_names: vec![
                ("run_command npm ci exit=0".to_string(), true),
                ("run_command npm test exit=0".to_string(), true),
            ],
            repo_profile_detected_language_packs: vec!["node".to_string()],
            semantic_query_operations_observed: vec!["related_tests".to_string()],
            turns: 8,
            ..RunFold::default()
        };
        let strong_metrics = CandidateDiffMetrics {
            available: true,
            file_count: 2,
            real_change_count: 2,
            added_total: 18,
            removed_total: 3,
            source_paths: vec!["src/slug.ts".to_string(), "tests/slug.test.ts".to_string()],
            ..CandidateDiffMetrics::default()
        };

        let weak_diff = candidate_diff_quality(Some(&weak_metrics), &weak_fold);
        let weak_proof = candidate_proof_quality(Some(&weak_metrics), &weak_fold);
        let strong_diff = candidate_diff_quality(Some(&strong_metrics), &strong_fold);
        let strong_proof = candidate_proof_quality(Some(&strong_metrics), &strong_fold);
        let weak_score = candidate_score(
            &weak_fold,
            &weak_diff,
            &weak_proof,
            &CandidateEvalEvidence::default(),
        );
        let strong_score = candidate_score(
            &strong_fold,
            &strong_diff,
            &strong_proof,
            &CandidateEvalEvidence::default(),
        );

        assert_eq!(weak_proof.proof_coverage_status, "no_proof_command");
        assert_eq!(weak_proof.behavior_proof_status, "gap");
        assert_eq!(strong_proof.proof_coverage_status, "satisfied");
        assert_eq!(strong_proof.proof_scope_status, "covered");
        assert_eq!(strong_proof.behavior_proof_status, "covered");
        assert!(strong_score > weak_score);
    }

    #[test]
    fn candidate_selection_json_keeps_human_selection_receipt_non_authoritative() {
        let fold = RunFold {
            candidate_selection_id: "forge_candidate_selection_1".to_string(),
            candidate_selection_primary_run_id: "forge_run_primary".to_string(),
            candidate_selection_selected_run_id: "forge_run_candidate_2".to_string(),
            candidate_selection_current_run_id: "forge_run_candidate_2".to_string(),
            candidate_selection_current_run_is_selected: true,
            candidate_selection_recommended_run_id: "forge_run_candidate_1".to_string(),
            candidate_selection_matches_recommendation: false,
            candidate_selection_override_reason_recorded: true,
            candidate_selection_reason: "better behavior proof".to_string(),
            candidate_selection_override_recommendation_reason:
                "candidate 2 has narrower diff and stronger proof".to_string(),
            candidate_selection_candidate_count: 4,
            candidate_selection_actor_id: "human:operator".to_string(),
            candidate_selection_grants_delivery_authority: false,
            candidate_selection_production_write_allowed: false,
            ..RunFold::default()
        };

        let selection = candidate_selection_json(&fold);

        assert_eq!(selection["status"], "recorded");
        assert_eq!(selection["selected_run_id"], "forge_run_candidate_2");
        assert_eq!(selection["current_run_is_selected"], true);
        assert_eq!(selection["matches_recommendation"], false);
        assert_eq!(selection["override_reason_recorded"], true);
        assert_eq!(selection["candidate_count"], 4);
        assert_eq!(selection["selection_grants_delivery_authority"], false);
        assert_eq!(selection["production_write_allowed"], false);
    }

    #[test]
    fn review_packet_marks_selected_checks_missing_until_observed() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event_with_evidence_fields(
                ev(
                    "forge_run_missing_proof",
                    "run_started",
                    "branch=forge/run_missing_proof",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("human:dev"),
                &[
                    ("repo_id", "node_canary"),
                    ("repo_primary_language", "javascript"),
                    ("language_pack_id", "node"),
                    ("repo_profile_detected_language_packs", "node"),
                    ("repo_profile_detected_files", "package.json"),
                    (
                        "repo_profile_toolchain_readiness",
                        "tool:node=available,check:npm_test=available",
                    ),
                    ("repo_profile_suggested_checks", "npm test"),
                    ("repo_profile_artifact_patterns", "node_modules/**,dist/**"),
                    ("repo_profile_proof_plan_status", "ready"),
                    ("repo_profile_proof_plan_next_action", "Run npm test."),
                    (
                        "repo_profile_proof_plan_summary",
                        "Forge inferred npm test and the local Node toolchain is available.",
                    ),
                    ("selected_checks", "npm test"),
                    ("selected_checks_source", "operator_supplied"),
                    ("repo_profile_source", "mdx_server::forge_repo_profile"),
                    ("repo_profile_grants_execution_authority", "false"),
                    ("suggested_checks_are_authority", "false"),
                ],
            )
            .expect("run started");
        kernel
            .record_forge_run_event(ev(
                "forge_run_missing_proof",
                "check_passed",
                "run_command npm run lint exit=0",
            ))
            .expect("different check passed");
        kernel
            .record_forge_run_event(ev(
                "forge_run_missing_proof",
                "run_finished",
                "status=RUN_FINISHED_DONE",
            ))
            .expect("run finished");

        let body = render(
            &kernel,
            "forge_run_missing_proof",
            Some(vec![]),
            &std::collections::BTreeMap::new(),
        );
        let value: serde_json::Value = serde_json::from_str(&body).expect("valid json");

        assert_eq!(value["proof_coverage"]["status"], "missing");
        assert_eq!(
            value["proof_coverage"]["match_policy"],
            "exact_selected_command"
        );
        assert_eq!(value["proof_scope"]["status"], "not_applicable");
        assert_eq!(
            value["proof_coverage"]["selected_checks_source"],
            "operator_supplied"
        );
        assert!(
            value["proof_coverage"]["satisfied_checks"]
                .as_array()
                .expect("satisfied checks")
                .is_empty()
        );
        assert_eq!(value["proof_coverage"]["missing_checks"][0], "npm test");
        assert_eq!(
            value["proof_coverage"]["observed_checks"][0]["name"],
            "npm run lint"
        );
        assert_eq!(
            value["principal_review"]["checklist"][0]["status"],
            "attention"
        );
        assert!(
            value["pr_handoff"]["summary_lines"]
                .as_array()
                .expect("summary lines")
                .iter()
                .any(|line| line
                    .as_str()
                    .expect("line")
                    .contains("Proof coverage: Proof missing"))
        );
        assert!(
            value["pr_handoff"]["summary_lines"]
                .as_array()
                .expect("summary lines")
                .iter()
                .any(|line| line
                    .as_str()
                    .expect("line")
                    .contains("Proof match policy: exact selected command"))
        );
        assert!(
            value["pr_handoff"]["summary_lines"]
                .as_array()
                .expect("summary lines")
                .iter()
                .any(|line| line
                    .as_str()
                    .expect("line")
                    .contains("Satisfied proof: none recorded"))
        );
        assert!(
            value["pr_handoff"]["summary_lines"]
                .as_array()
                .expect("summary lines")
                .iter()
                .any(|line| line
                    .as_str()
                    .expect("line")
                    .contains("Missing proof: npm test"))
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Proof match policy: exact selected command")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Satisfied proof: none recorded")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Missing proof: npm test")
        );
    }

    #[test]
    fn review_packet_groups_real_diff_impact_by_language_pack() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event_with_evidence_fields(
                ev(
                    "forge_run_mixed_stack",
                    "run_started",
                    "branch=forge/run_mixed_stack",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("human:dev"),
                &[
                    ("repo_id", "mixed_stack"),
                    ("repo_primary_language", "javascript"),
                    ("language_pack_id", "node"),
                    (
                        "repo_profile_detected_language_packs",
                        "java-maven,node,rust-cargo",
                    ),
                    ("repo_profile_detected_files", "package.json,pom.xml,Cargo.toml"),
                    (
                        "repo_profile_review_axes",
                        "package-script-test-coverage,typescript-or-module-style-fit",
                    ),
                    (
                        "repo_profile_principal_review_gates",
                        "behavior:changed_behavior_is_explicit,tests:checks_cover_changed_behavior",
                    ),
                    (
                        "repo_profile_language_pack_guidance",
                        "source_of_truth:package_json_and_lockfile,proof:selected_package_test_command",
                    ),
                    (
                        "repo_profile_toolchain_readiness",
                        "tool:node=available,check:npm_test=available",
                    ),
                    ("repo_profile_suggested_checks", "npm test"),
                    (
                        "repo_profile_artifact_patterns",
                        "node_modules/**,target/**,build/**",
                    ),
                    ("repo_profile_proof_plan_status", "ready"),
                    ("repo_profile_proof_plan_next_action", "Run npm test."),
                    (
                        "repo_profile_proof_plan_summary",
                        "Forge inferred npm test and the local Node toolchain is available.",
                    ),
                    ("selected_checks", "npm test"),
                    ("selected_checks_source", "operator_supplied"),
                    ("repo_profile_source", "mdx_server::forge_repo_profile"),
                    ("repo_profile_grants_execution_authority", "false"),
                    ("suggested_checks_are_authority", "false"),
                ],
            )
            .expect("run started");
        kernel
            .record_forge_run_event(ev(
                "forge_run_mixed_stack",
                "run_finished",
                "status=RUN_FINISHED_DONE",
            ))
            .expect("run finished");
        let body = render(
            &kernel,
            "forge_run_mixed_stack",
            Some(vec![
                crate::forge_diff_route::RunDiffFile {
                    path: "package.json".to_string(),
                    added: 2,
                    removed: 1,
                },
                crate::forge_diff_route::RunDiffFile {
                    path: "services/api/src/main/java/com/acme/App.java".to_string(),
                    added: 8,
                    removed: 3,
                },
                crate::forge_diff_route::RunDiffFile {
                    path: "tools/src/main.rs".to_string(),
                    added: 5,
                    removed: 0,
                },
                crate::forge_diff_route::RunDiffFile {
                    path: "node_modules/demo/index.js".to_string(),
                    added: 200,
                    removed: 0,
                },
            ]),
            &std::collections::BTreeMap::new(),
        );
        let value: serde_json::Value = serde_json::from_str(&body).expect("valid json");
        let impact = value["diff"]["language_pack_impact"]
            .as_array()
            .expect("language pack impact");

        assert_eq!(value["diff"]["real_change_count"], 3);
        assert_eq!(value["diff"]["artifact_count"], 1);
        assert_eq!(impact.len(), 3);
        assert_eq!(impact[0]["language_pack_id"], "java-maven");
        assert_eq!(impact[0]["file_count"], 1);
        assert_eq!(impact[1]["language_pack_id"], "node");
        assert_eq!(impact[1]["file_count"], 1);
        assert_eq!(impact[2]["language_pack_id"], "rust-cargo");
        assert_eq!(impact[2]["file_count"], 1);
        assert_eq!(value["proof_scope"]["status"], "partial");
        assert_eq!(value["proof_scope"]["covered_language_packs"][0], "node");
        assert!(
            value["proof_scope"]["uncovered_language_packs"]
                .as_array()
                .expect("uncovered")
                .iter()
                .any(|pack| pack.as_str() == Some("java-maven"))
        );
        assert!(
            value["proof_scope"]["uncovered_language_packs"]
                .as_array()
                .expect("uncovered")
                .iter()
                .any(|pack| pack.as_str() == Some("rust-cargo"))
        );
        assert!(
            !impact
                .iter()
                .flat_map(|item| item["sample_paths"].as_array().expect("sample paths"))
                .any(|path| path.as_str().expect("path").contains("node_modules"))
        );
        assert!(
            value["principal_review"]["checklist"]
                .as_array()
                .expect("checklist")
                .iter()
                .any(|item| item["line"]
                    .as_str()
                    .expect("line")
                    .contains("Proof scope partial"))
        );
        assert!(
            value["principal_review"]["checklist"]
                .as_array()
                .expect("checklist")
                .iter()
                .any(|item| item["line"]
                    .as_str()
                    .expect("line")
                    .contains("Diff touches language pack(s): java-maven, node, rust-cargo."))
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Diff language packs: java-maven, node, rust-cargo")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Proof scope: Proof scope partial")
        );
        assert!(
            value["pr_handoff"]["body_markdown"]
                .as_str()
                .expect("handoff body")
                .contains("Artifact noise: node dependency output: 1 file(s), +200/-0")
        );
    }
}
