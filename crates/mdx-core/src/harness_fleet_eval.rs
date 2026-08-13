use crate::harness_load_sim::{LoadSimConfig, LoadSimReport, run_load_sim};
use crate::harness_scale_backpressure::{ScaleBackpressurePolicy, WorkerPriority};
use crate::{
    ActionKind, ActorId, CorrelationIds, GovernedWriteIdentity, LoopId, LoopRun, MdxKernel,
    StorageProvider, TenantId, TraceId, WorkflowId, admit_local_route_actor, hex, payload, sha256,
};

pub const PRINCIPAL_REVIEW_GATE_IDS: &str = "behavior:changed_behavior_is_explicit,tests:checks_cover_changed_behavior,stack_idioms:<language-pack-specific>,compatibility:public_contract_or_migration_risk_reviewed,security:no_secret_or_authority_expansion,maintainability:dependency_config_and_generated_churn_justified";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FleetBenchmarkTask {
    pub task_id: &'static str,
    pub class: &'static str,
    pub complexity_tier: &'static str,
    pub engineering_facets: &'static str,
    pub allowed_scope: &'static str,
    pub expected_check: &'static str,
    pub evaluation_oracle: &'static str,
    pub human_timebox_minutes: u32,
    pub contamination_policy: &'static str,
    pub timeout_ms: u64,
    pub acceptance: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FleetScoringDimension {
    pub dimension_id: &'static str,
    pub weight_pct: u32,
    pub evaluator_kind: &'static str,
    pub evidence_required: &'static str,
    pub fail_closed_if_missing: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FleetRunnerProfile {
    pub runner_id: &'static str,
    pub runner_kind: &'static str,
    pub display_name: &'static str,
    pub adapter_kind: &'static str,
    pub model_provider: &'static str,
    pub model: &'static str,
    pub wire_api: &'static str,
    pub model_profile_id: &'static str,
    pub execution_mode: &'static str,
    pub invocation_mode: &'static str,
    pub version_requirement: &'static str,
    pub isolation_policy: &'static str,
    pub transcript_policy: &'static str,
    pub output_policy: &'static str,
    pub evidence_policy: &'static str,
    pub execution_status: &'static str,
    pub visibility_tier: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FleetModelMatrixProfile {
    pub profile_id: &'static str,
    pub provider_family: &'static str,
    pub model_provider: &'static str,
    pub example_model: &'static str,
    pub model_display_name: &'static str,
    pub wire_api: &'static str,
    pub auth_policy: &'static str,
    pub redaction_policy: &'static str,
    pub budget_policy: &'static str,
    pub live_call_allowed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForgeLanguageTaskCorpusEntry {
    pub task_corpus_id: &'static str,
    pub language_pack_id: &'static str,
    pub repo_family: &'static str,
    pub task_class: &'static str,
    pub complexity_tier: &'static str,
    pub visible_check: &'static str,
    pub hidden_check_slot: &'static str,
    pub artifact_noise_expected: &'static str,
    pub required_principal_review_gates: &'static str,
}

pub fn language_task_engineering_facets(task: &ForgeLanguageTaskCorpusEntry) -> &'static str {
    match task.task_class {
        "bug_fix" => "behavioral correctness, regression coverage, minimal diff",
        "feature" => "behavior design, integration fit, regression coverage",
        "refactor" => "architecture fit, public contract preservation, reviewable diff",
        "ci_repair" => "build reproducibility, toolchain correctness, workflow safety",
        "security" => "abuse resistance, authority boundary, negative regression coverage",
        "performance" => "hot-path behavior, budget evidence, maintainability",
        _ => "correctness, tests, maintainability",
    }
}

pub fn language_task_evaluation_oracle(task: &ForgeLanguageTaskCorpusEntry) -> &'static str {
    match task.complexity_tier {
        "small" => "visible native check plus hidden behavioral regression",
        "medium" => "visible native check plus hidden integration or compatibility fixture",
        "large" => {
            "visible native check plus hidden cross-file architecture and compatibility regression"
        }
        _ => "visible native check plus hidden reviewer judgment",
    }
}

pub fn language_task_human_timebox_minutes(task: &ForgeLanguageTaskCorpusEntry) -> u32 {
    match task.complexity_tier {
        "small" => 30,
        "medium" => 90,
        "large" => 180,
        _ => 60,
    }
}

pub fn language_task_contamination_policy(_task: &ForgeLanguageTaskCorpusEntry) -> &'static str {
    "fresh repo fixture or held-out mutation required; no training-set benchmark claim accepted as sole evidence"
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeLanguagePackScorecard {
    pub language_pack_id: &'static str,
    pub repo_family: &'static str,
    pub task_count: u32,
    pub small_task_count: u32,
    pub medium_task_count: u32,
    pub large_task_count: u32,
    pub visible_check_count: u32,
    pub hidden_check_slot_count: u32,
    pub artifact_noise_expectation_count: u32,
    pub principal_review_gate_count: u32,
    pub principal_verdict_required: bool,
    pub ready_for_live_eval: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FleetRunnerScore {
    pub runner_id: String,
    pub runner_kind: String,
    pub model_provider: String,
    pub model: String,
    pub wire_api: String,
    pub tasks_attempted: u32,
    pub accepted: u32,
    pub quarantined: u32,
    pub blocked: u32,
    pub failed_quality_gates: u32,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost_cents: u32,
    pub mean_runtime_ms: u64,
    pub authority_violations_blocked: u32,
    pub pass_rate_pct: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeFleetEvalResultSubmission<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub actor_role: &'a str,
    pub submission_id: &'a str,
    pub benchmark_task_id: &'a str,
    pub language_task_corpus_id: &'a str,
    pub language_pack_id: &'a str,
    pub repo_family: &'a str,
    pub runner_id: &'a str,
    pub model_profile_id: &'a str,
    pub output_artifact_ref: &'a str,
    pub output_artifact_sha256: &'a str,
    pub transcript_ref: &'a str,
    pub claimed_check: &'a str,
    pub hidden_check_slot: &'a str,
    pub artifact_noise_expected: &'a str,
    pub principal_review_gate_results: &'a str,
    pub standards_source_fingerprints: &'a str,
    pub artifact_filter_summary: &'a str,
    pub diff_language_pack_impact: &'a str,
    pub principal_engineer_verdict: &'a str,
    pub pr_handoff_ref: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeFleetEvalResultIngestionReport {
    pub status: &'static str,
    pub submission_id: String,
    pub benchmark_task_id: String,
    pub language_task_corpus_id: String,
    pub language_pack_id: String,
    pub repo_family: String,
    pub runner_id: String,
    pub model_profile_id: String,
    pub principal_review_gate_results: String,
    pub standards_source_fingerprints: String,
    pub artifact_filter_summary: String,
    pub diff_language_pack_impact: String,
    pub language_engineering_facets: String,
    pub language_evaluation_oracle: String,
    pub language_human_timebox_minutes: u32,
    pub language_contamination_policy: String,
    pub pr_handoff_ref: String,
    pub result_receipt_id: String,
    pub policy_decision_id: String,
    pub output_quarantined: bool,
    pub external_output_consumable: bool,
    pub mdx_quality_gates_required: bool,
    pub mdx_quality_gates_passed: bool,
    pub accepted_for_scoreboard: bool,
    pub blocked_reason: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeFleetEvalDryRunReport {
    pub status: &'static str,
    pub dry_run_id: String,
    pub runner_id: String,
    pub benchmark_task_count: u32,
    pub language_task_count: u32,
    pub model_profile_count: u32,
    pub scoring_dimension_count: u32,
    pub dry_run_case_count: u32,
    pub runner_execution_receipt_ids: Vec<String>,
    pub result_receipt_ids: Vec<String>,
    pub score_receipt_ids: Vec<String>,
    pub live_provider_calls_allowed: bool,
    pub live_provider_calls_performed: bool,
    pub provider_credentials_required_for_live: bool,
    pub ready_for_live_credentials: bool,
    pub accepted_for_scoreboard_count: u32,
    pub quarantined_count: u32,
    pub blocked_reason: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeFleetEvalLiveRunApproval<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub actor_role: &'a str,
    pub approval_id: &'a str,
    pub provider_allowlist: &'a str,
    pub max_spend_cents: u32,
    pub max_tasks: u32,
    pub max_parallel_agents: u32,
    pub artifact_retention_policy: &'a str,
    pub redaction_policy: &'a str,
    pub stop_conditions: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeFleetEvalLiveRunApprovalReport {
    pub status: &'static str,
    pub approval_id: String,
    pub approval_receipt_id: String,
    pub policy_decision_id: String,
    pub provider_allowlist: String,
    pub max_spend_cents: u32,
    pub max_tasks: u32,
    pub max_parallel_agents: u32,
    pub artifact_retention_policy: String,
    pub redaction_policy: String,
    pub stop_conditions: String,
    pub provider_credentials_required: bool,
    pub live_provider_calls_allowed: bool,
    pub approval_grants_execution_authority: bool,
    pub adapter_execution_allowed: bool,
    pub production_write_allowed: bool,
    pub blocked_reason: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ForgeFleetEvalResultIngestionError {
    Missing(&'static str),
    InvalidArtifactIdentity(&'static str),
    InvalidStandardsSourceFingerprint(String),
    InvalidPrincipalEngineerVerdict(String),
    UnknownBenchmarkTask(String),
    UnknownLanguageTaskCorpus(String),
    LanguageTaskMismatch(String),
    UnknownRunner(String),
    UnknownModelProfile(String),
    UnknownProviderFamily(String),
    InvalidApprovalField(&'static str),
    ActorAdmission(String),
}

impl ForgeFleetEvalResultIngestionError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("forge fleet eval result missing {field}"),
            Self::InvalidArtifactIdentity(field) => {
                format!("forge fleet eval result has invalid artifact identity in {field}")
            }
            Self::InvalidStandardsSourceFingerprint(message) => message.clone(),
            Self::InvalidPrincipalEngineerVerdict(message) => message.clone(),
            Self::UnknownBenchmarkTask(task_id) => {
                format!("forge fleet eval result task {task_id} is not in benchmark corpus")
            }
            Self::UnknownLanguageTaskCorpus(task_id) => {
                format!("forge fleet eval language task {task_id} is not in language corpus")
            }
            Self::LanguageTaskMismatch(message) => message.clone(),
            Self::UnknownRunner(runner_id) => {
                format!("forge fleet eval result runner {runner_id} is not declared")
            }
            Self::UnknownModelProfile(profile_id) => {
                format!("forge fleet eval result model profile {profile_id} is not declared")
            }
            Self::UnknownProviderFamily(provider) => {
                format!("forge fleet eval provider family {provider} is not declared")
            }
            Self::InvalidApprovalField(field) => {
                format!("forge fleet eval live-run approval invalid {field}")
            }
            Self::ActorAdmission(message) => message.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForgeFleetEvalReport {
    pub status: String,
    pub benchmark_tasks: Vec<FleetBenchmarkTask>,
    pub language_task_corpus: Vec<ForgeLanguageTaskCorpusEntry>,
    pub language_pack_scorecards: Vec<ForgeLanguagePackScorecard>,
    pub scoring_dimensions: Vec<FleetScoringDimension>,
    pub runner_scores: Vec<FleetRunnerScore>,
    pub model_matrix: Vec<FleetModelMatrixProfile>,
    pub winning_runner_id: String,
    pub codex_adapter_status: String,
    pub codex_model_freedom_rule: String,
    pub evidence_policy: String,
    pub parallel_agents_target: u32,
    pub parallel_load: LoadSimReport,
    pub data_decides: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalForgeFleetEvalHarness;

impl LocalForgeFleetEvalHarness {
    pub fn run_scoreboard(&self) -> ForgeFleetEvalReport {
        run_forge_fleet_eval()
    }
}

pub fn forge_fleet_benchmark_tasks() -> Vec<FleetBenchmarkTask> {
    vec![
        FleetBenchmarkTask {
            task_id: "bugfix_small",
            class: "bug_fix",
            complexity_tier: "small",
            engineering_facets: "localization,regression_test,minimal_patch",
            allowed_scope: "crates/mdx-core/src/",
            expected_check: "cargo test -p mdx-core",
            evaluation_oracle: "visible_regression_test_plus_receipt_diff",
            human_timebox_minutes: 20,
            contamination_policy: "local_mutation_not_public_benchmark",
            timeout_ms: 300_000,
            acceptance: "regression test passes and receipt chain remains valid",
        },
        FleetBenchmarkTask {
            task_id: "feature_medium",
            class: "feature",
            complexity_tier: "medium",
            engineering_facets: "contract_design,server_route,schema_evidence",
            allowed_scope: "crates/mdx-server/src/",
            expected_check: "cargo check -p mdx-server",
            evaluation_oracle: "compile_schema_and_route_contract",
            human_timebox_minutes: 45,
            contamination_policy: "local_generated_contract",
            timeout_ms: 600_000,
            acceptance: "route or runtime behavior compiles with schema evidence",
        },
        FleetBenchmarkTask {
            task_id: "refactor_with_tests",
            class: "refactor",
            complexity_tier: "medium",
            engineering_facets: "behavior_preservation,test_quality,readability",
            allowed_scope: "crates/mdx-core/src/",
            expected_check: "cargo test -p mdx-core harness",
            evaluation_oracle: "focused_tests_plus_behavior_invariants",
            human_timebox_minutes: 60,
            contamination_policy: "local_mutation_not_public_benchmark",
            timeout_ms: 600_000,
            acceptance: "behavior preserved and focused tests pass",
        },
        FleetBenchmarkTask {
            task_id: "security_fix",
            class: "security",
            complexity_tier: "medium",
            engineering_facets: "threat_model,deny_by_default,negative_tests",
            allowed_scope: "scripts/",
            expected_check: "make security-check",
            evaluation_oracle: "negative_security_case_and_policy_guard",
            human_timebox_minutes: 45,
            contamination_policy: "local_security_fixture",
            timeout_ms: 300_000,
            acceptance: "unsafe path stays denied and check passes",
        },
        FleetBenchmarkTask {
            task_id: "ci_repair",
            class: "ci_repair",
            complexity_tier: "small",
            engineering_facets: "failure_triage,minimal_ci_change,evidence_observation",
            allowed_scope: ".github/",
            expected_check: "make verification-manifest-check",
            evaluation_oracle: "observed_ci_or_verification_manifest_evidence",
            human_timebox_minutes: 30,
            contamination_policy: "fresh_local_ci_failure_fixture",
            timeout_ms: 300_000,
            acceptance: "declared CI evidence is observed, not asserted",
        },
        FleetBenchmarkTask {
            task_id: "docs_plus_code",
            class: "docs_code",
            complexity_tier: "small",
            engineering_facets: "source_map_alignment,operator_explanation,contract_currentness",
            allowed_scope: "docs/",
            expected_check: "make source-map-check",
            evaluation_oracle: "source_map_and_doc_contract_alignment",
            human_timebox_minutes: 25,
            contamination_policy: "local_doc_contract",
            timeout_ms: 240_000,
            acceptance: "doc, source map, and check stay aligned",
        },
        FleetBenchmarkTask {
            task_id: "multi_file_constrained",
            class: "multi_file",
            complexity_tier: "large",
            engineering_facets: "cross_module_reasoning,blast_radius_control,local_smoke",
            allowed_scope: "crates/mdx-core/src/,crates/mdx-server/src/",
            expected_check: "make local-smoke",
            evaluation_oracle: "local_smoke_plus_receipt_boundary_check",
            human_timebox_minutes: 90,
            contamination_policy: "local_multi_file_mutation",
            timeout_ms: 900_000,
            acceptance: "multi-file change passes local smoke without authority widening",
        },
        FleetBenchmarkTask {
            task_id: "architecture_boundary_change",
            class: "architecture",
            complexity_tier: "large",
            engineering_facets: "boundary_design,source_contract,adr_necessity,generated_artifacts",
            allowed_scope: "crates/mdx-core/src/,crates/mdx-codegen/src/,docs/",
            expected_check: "make source-map-check",
            evaluation_oracle: "contract_schema_source_map_and_handoff_review",
            human_timebox_minutes: 120,
            contamination_policy: "fresh_internal_boundary_task",
            timeout_ms: 1_200_000,
            acceptance: "new boundary is declared, generated, checked, and does not widen authority",
        },
        FleetBenchmarkTask {
            task_id: "api_backward_compat",
            class: "api_compat",
            complexity_tier: "medium",
            engineering_facets: "backward_compatibility,response_schema,consumer_risk",
            allowed_scope: "crates/mdx-server/src/,generated/response-schemas/",
            expected_check: "cargo check -p mdx-server",
            evaluation_oracle: "schema_compatibility_and_route_behavior",
            human_timebox_minutes: 60,
            contamination_policy: "local_route_contract_mutation",
            timeout_ms: 600_000,
            acceptance: "existing route contract remains compatible while new behavior is exposed explicitly",
        },
        FleetBenchmarkTask {
            task_id: "data_migration_safety",
            class: "migration",
            complexity_tier: "large",
            engineering_facets: "idempotency,rollback,backfill_safety,receipt_evidence",
            allowed_scope: "crates/mdx-core/src/,generated/",
            expected_check: "make verification-manifest-check",
            evaluation_oracle: "migration_plan_replay_and_idempotency_receipts",
            human_timebox_minutes: 120,
            contamination_policy: "fresh_internal_migration",
            timeout_ms: 1_200_000,
            acceptance: "migration is idempotent, observable, reversible, and gated before live data",
        },
        FleetBenchmarkTask {
            task_id: "performance_regression",
            class: "performance",
            complexity_tier: "medium",
            engineering_facets: "latency_budget,allocation_control,benchmark_interpretation",
            allowed_scope: "crates/mdx-core/src/,scripts/",
            expected_check: "make local-smoke",
            evaluation_oracle: "before_after_budget_and_regression_threshold",
            human_timebox_minutes: 75,
            contamination_policy: "local_perf_fixture",
            timeout_ms: 900_000,
            acceptance: "performance budget is preserved or improved without weakening correctness",
        },
        FleetBenchmarkTask {
            task_id: "concurrency_race",
            class: "concurrency",
            complexity_tier: "large",
            engineering_facets: "race_reproduction,deterministic_simulation,fairness,backpressure",
            allowed_scope: "crates/mdx-core/src/",
            expected_check: "cargo test -p mdx-core harness_fleet_eval",
            evaluation_oracle: "deterministic_concurrency_repro_and_invariant_check",
            human_timebox_minutes: 120,
            contamination_policy: "local_scheduler_mutation",
            timeout_ms: 1_200_000,
            acceptance: "race is reproduced, fixed, and guarded by deterministic invariants",
        },
        FleetBenchmarkTask {
            task_id: "observability_failure_mode",
            class: "observability",
            complexity_tier: "medium",
            engineering_facets: "traceability,operator_signal,failure_mode_legibility",
            allowed_scope: "crates/mdx-core/src/,crates/mdx-server/src/",
            expected_check: "make verification-manifest-check",
            evaluation_oracle: "trace_receipts_and_operator_projection",
            human_timebox_minutes: 60,
            contamination_policy: "local_failure_fixture",
            timeout_ms: 600_000,
            acceptance: "failure is visible through receipts, route output, and next safe action",
        },
        FleetBenchmarkTask {
            task_id: "frontend_product_workflow",
            class: "product_ux",
            complexity_tier: "medium",
            engineering_facets: "first_viewport,governed_action,evidence_disclosure,copy_quality",
            allowed_scope: "apps/mdx-host/,apps/mdx/",
            expected_check: "make ui-build-check",
            evaluation_oracle: "ui_smoke_and_product_contract_review",
            human_timebox_minutes: 90,
            contamination_policy: "local_product_workflow_fixture",
            timeout_ms: 900_000,
            acceptance: "workflow is usable, contract-aligned, and evidence remains one click away",
        },
        FleetBenchmarkTask {
            task_id: "long_horizon_multi_stage",
            class: "long_horizon",
            complexity_tier: "xl",
            engineering_facets: "planning,implementation,tests,docs,handoff,recovery",
            allowed_scope: "crates/mdx-core/src/,crates/mdx-server/src/,crates/mdx-codegen/src/,docs/,scripts/",
            expected_check: "make local-smoke",
            evaluation_oracle: "multi_stage_receipts_plus_local_smoke_and_handoff",
            human_timebox_minutes: 180,
            contamination_policy: "fresh_internal_long_horizon_task",
            timeout_ms: 1_800_000,
            acceptance: "plan, implementation, validation, generated artifacts, and handoff all cohere",
        },
    ]
}

pub fn forge_fleet_scoring_dimensions() -> Vec<FleetScoringDimension> {
    vec![
        FleetScoringDimension {
            dimension_id: "correctness",
            weight_pct: 20,
            evaluator_kind: "code_reference_based",
            evidence_required: "tests_or_behavioral_oracle",
            fail_closed_if_missing: true,
        },
        FleetScoringDimension {
            dimension_id: "regression_test_quality",
            weight_pct: 12,
            evaluator_kind: "code_and_human_rubric",
            evidence_required: "focused_test_or_explicit_no_test_reason",
            fail_closed_if_missing: true,
        },
        FleetScoringDimension {
            dimension_id: "patch_quality",
            weight_pct: 12,
            evaluator_kind: "diff_rubric",
            evidence_required: "minimal_readable_diff",
            fail_closed_if_missing: true,
        },
        FleetScoringDimension {
            dimension_id: "architecture_fit",
            weight_pct: 10,
            evaluator_kind: "contract_rubric",
            evidence_required: "source_contract_alignment",
            fail_closed_if_missing: true,
        },
        FleetScoringDimension {
            dimension_id: "security_and_policy",
            weight_pct: 10,
            evaluator_kind: "rule_based_gate",
            evidence_required: "deny_boundary_and_secret_policy_receipts",
            fail_closed_if_missing: true,
        },
        FleetScoringDimension {
            dimension_id: "maintainability",
            weight_pct: 8,
            evaluator_kind: "human_or_llm_rubric",
            evidence_required: "readability_and_future_change_review",
            fail_closed_if_missing: false,
        },
        FleetScoringDimension {
            dimension_id: "observability",
            weight_pct: 6,
            evaluator_kind: "receipt_trace_check",
            evidence_required: "receipt_or_route_operator_signal",
            fail_closed_if_missing: false,
        },
        FleetScoringDimension {
            dimension_id: "performance",
            weight_pct: 6,
            evaluator_kind: "budget_check",
            evidence_required: "latency_or_cost_budget",
            fail_closed_if_missing: false,
        },
        FleetScoringDimension {
            dimension_id: "migration_and_compatibility",
            weight_pct: 6,
            evaluator_kind: "compatibility_gate",
            evidence_required: "backward_compat_or_migration_replay",
            fail_closed_if_missing: true,
        },
        FleetScoringDimension {
            dimension_id: "cost_latency_budget",
            weight_pct: 5,
            evaluator_kind: "numeric_budget",
            evidence_required: "tokens_cost_runtime",
            fail_closed_if_missing: false,
        },
        FleetScoringDimension {
            dimension_id: "handoff_quality",
            weight_pct: 5,
            evaluator_kind: "human_rubric",
            evidence_required: "summary_validation_open_risk",
            fail_closed_if_missing: false,
        },
    ]
}

pub fn forge_fleet_runner_profiles() -> Vec<FleetRunnerProfile> {
    vec![
        FleetRunnerProfile {
            runner_id: "mdx_native_harness_runner",
            runner_kind: "mdx_native",
            display_name: "Forge native builder",
            adapter_kind: "mdx_native",
            model_provider: "local_model_gateway",
            model: "deterministic_harness_model",
            wire_api: "mdx_internal",
            model_profile_id: "mdx_native_local_model_profile",
            execution_mode: "local_receipt_gated",
            invocation_mode: "mdx_internal_loop",
            version_requirement: "repo_current",
            isolation_policy: "mdx_managed_worktree",
            transcript_policy: "forge_run_event_receipts",
            output_policy: "accepted_after_mdx_quality_gates",
            evidence_policy: "mdx_receipts_are_runtime_truth",
            execution_status: "LIVE_LOCAL_BASELINE",
            visibility_tier: "product_recommendation",
        },
        FleetRunnerProfile {
            runner_id: "codex_cli_external_worker",
            runner_kind: "external_machine",
            display_name: "Codex CLI",
            adapter_kind: "codex_cli",
            model_provider: "mdx_responses_proxy",
            model: "gpt-5.6-sol-via-mdx",
            wire_api: "responses",
            model_profile_id: "codex_openai_responses_profile",
            execution_mode: "adapter_spike_denied",
            invocation_mode: "exec_json_stream",
            version_requirement: "codex_cli_current_or_newer",
            isolation_policy: "isolated_codex_home_and_worktree",
            transcript_policy: "json_event_and_rollout_transcript_capture",
            output_policy: "quarantine_until_mdx_gates",
            evidence_policy: "quarantined_until_mdx_quality_gates",
            execution_status: "PROFILE_ADMITTED_ADAPTER_EXECUTION_DENIED",
            visibility_tier: "product_recommendation",
        },
        FleetRunnerProfile {
            runner_id: "grok_build_cli_external_worker",
            runner_kind: "external_machine",
            display_name: "Grok Build",
            adapter_kind: "grok_build_cli",
            model_provider: "xai_grok_build_model_profile",
            model: "grok-4.5",
            wire_api: "acp_stdio",
            model_profile_id: "xai_grok_build_model_profile",
            execution_mode: "xai_key_enables_machine_option_execution_gate_denied",
            invocation_mode: "acp_stdio_jsonrpc_pending_runtime_gate",
            version_requirement: "grok_cli_current_no_auto_update",
            isolation_policy: "isolated_worktree_required_no_auto_update",
            transcript_policy: "acp_session_update_and_completion_metadata_capture_required",
            output_policy: "quarantine_until_mdx_gates",
            evidence_policy: "quarantined_until_mdx_quality_gates",
            execution_status: "PROFILE_ADMITTED_XAI_KEY_OPTION_EXECUTION_DENIED",
            visibility_tier: "product_recommendation",
        },
        FleetRunnerProfile {
            runner_id: "claude_code_external_worker",
            runner_kind: "external_machine",
            display_name: "Claude Agent SDK",
            adapter_kind: "claude_code",
            model_provider: "anthropic_claude_agent_sdk",
            model: "claude-agent-sdk-selected-model",
            wire_api: "claude_agent_sdk",
            model_profile_id: "claude_code_model_profile",
            execution_mode: "anthropic_key_enables_machine_option_execution_gate_denied",
            invocation_mode: "agent_sdk_streaming_input_pending_runtime_gate",
            version_requirement: "claude_agent_sdk_current_or_native_binary_recorded",
            isolation_policy: "isolated_worktree_required",
            transcript_policy: "agent_sdk_message_stream_and_tool_event_capture_required",
            output_policy: "quarantine_until_mdx_gates",
            evidence_policy: "quarantined_until_mdx_quality_gates",
            execution_status: "PROFILE_ADMITTED_ANTHROPIC_KEY_OPTION_EXECUTION_DENIED",
            visibility_tier: "product_recommendation",
        },
        FleetRunnerProfile {
            runner_id: "kimi_code_external_worker",
            runner_kind: "external_machine",
            display_name: "Kimi Code CLI",
            adapter_kind: "kimi_code",
            model_provider: "moonshot_kimi_code_cli",
            model: "kimi-k3",
            wire_api: "kimi_code_cli",
            model_profile_id: "kimi_code_k3_model_profile",
            execution_mode: "moonshot_key_enables_machine_option_execution_gate_denied",
            invocation_mode: "non_interactive_stream_json_pending_runtime_gate",
            version_requirement: "kimi_code_cli_current_with_runtime_fingerprint",
            isolation_policy: "isolated_worktree_and_kimi_code_home_required",
            transcript_policy: "stream_json_tool_and_assistant_event_capture_required",
            output_policy: "quarantine_until_mdx_gates",
            evidence_policy: "quarantined_until_mdx_quality_gates",
            execution_status: "PROFILE_ADMITTED_MOONSHOT_KEY_OPTION_EXECUTION_DENIED",
            visibility_tier: "product_recommendation",
        },
        FleetRunnerProfile {
            runner_id: "gemini_cli_external_worker",
            runner_kind: "external_machine",
            display_name: "Gemini CLI",
            adapter_kind: "gemini_cli",
            model_provider: "google_gemini_cli",
            model: "gemini-cli-selected-model",
            wire_api: "gemini_cli",
            model_profile_id: "gemini_cli_model_profile",
            execution_mode: "profile_declared_adapter_execution_denied",
            invocation_mode: "non_interactive_cli_pending_preflight",
            version_requirement: "gemini_cli_version_recorded",
            isolation_policy: "isolated_worktree_required",
            transcript_policy: "json_or_stream_transcript_capture_required",
            output_policy: "quarantine_until_mdx_gates",
            evidence_policy: "quarantined_until_mdx_quality_gates",
            execution_status: "PROFILE_DECLARED_PREFLIGHT_REQUIRED",
            visibility_tier: "operator_raw",
        },
        FleetRunnerProfile {
            runner_id: "opencode_external_worker",
            runner_kind: "external_machine",
            display_name: "OpenCode",
            adapter_kind: "opencode",
            model_provider: "opencode_configured_provider",
            model: "opencode-selected-model",
            wire_api: "opencode_cli",
            model_profile_id: "opencode_model_profile",
            execution_mode: "profile_declared_adapter_execution_denied",
            invocation_mode: "terminal_or_ide_agent_pending_preflight",
            version_requirement: "opencode_version_recorded",
            isolation_policy: "isolated_worktree_required",
            transcript_policy: "agent_transcript_capture_required",
            output_policy: "quarantine_until_mdx_gates",
            evidence_policy: "quarantined_until_mdx_quality_gates",
            execution_status: "PROFILE_DECLARED_PREFLIGHT_REQUIRED",
            visibility_tier: "operator_raw",
        },
        FleetRunnerProfile {
            runner_id: "cline_external_worker",
            runner_kind: "external_machine",
            display_name: "Cline",
            adapter_kind: "cline",
            model_provider: "cline_configured_provider",
            model: "cline-selected-model",
            wire_api: "cline_cli",
            model_profile_id: "cline_model_profile",
            execution_mode: "profile_declared_adapter_execution_denied",
            invocation_mode: "ide_terminal_agent_pending_preflight",
            version_requirement: "cline_version_recorded",
            isolation_policy: "human_supervised_workspace_required",
            transcript_policy: "approval_transcript_capture_required",
            output_policy: "quarantine_until_mdx_gates",
            evidence_policy: "quarantined_until_mdx_quality_gates",
            execution_status: "PROFILE_DECLARED_PREFLIGHT_REQUIRED",
            visibility_tier: "operator_raw",
        },
        FleetRunnerProfile {
            runner_id: "goose_external_worker",
            runner_kind: "external_machine",
            display_name: "Goose",
            adapter_kind: "goose",
            model_provider: "goose_configured_provider",
            model: "goose-selected-model",
            wire_api: "goose_cli",
            model_profile_id: "goose_model_profile",
            execution_mode: "profile_declared_adapter_execution_denied",
            invocation_mode: "headless_cli_pending_preflight",
            version_requirement: "goose_version_recorded",
            isolation_policy: "isolated_worktree_required",
            transcript_policy: "stream_json_transcript_capture_required",
            output_policy: "quarantine_until_mdx_gates",
            evidence_policy: "quarantined_until_mdx_quality_gates",
            execution_status: "PROFILE_DECLARED_PREFLIGHT_REQUIRED",
            visibility_tier: "operator_raw",
        },
    ]
}

pub fn forge_fleet_model_matrix_profiles() -> Vec<FleetModelMatrixProfile> {
    vec![
        FleetModelMatrixProfile {
            profile_id: "mdx_native_local_model_profile",
            provider_family: "mdx",
            model_provider: "mdx_native_harness",
            example_model: "mdx-native-fixture-solver",
            model_display_name: "MDx native fixture solver",
            wire_api: "mdx_internal",
            auth_policy: "no_provider_secret_required",
            redaction_policy: "mdx_context_redaction_required",
            budget_policy: "mdx_local_runtime_budget_required",
            live_call_allowed: false,
        },
        FleetModelMatrixProfile {
            profile_id: "gemini_cli_model_profile",
            provider_family: "gemini",
            model_provider: "google_gemini_cli",
            example_model: "gemini-cli-selected-model",
            model_display_name: "Gemini CLI selected model",
            wire_api: "gemini_cli",
            auth_policy: "local_gemini_cli_auth_presence_only",
            redaction_policy: "mdx_context_redaction_required",
            budget_policy: "mdx_cli_runtime_budget_required",
            live_call_allowed: false,
        },
        FleetModelMatrixProfile {
            profile_id: "codex_openai_cli_profile",
            provider_family: "openai",
            model_provider: "openai_codex_cli",
            example_model: "codex-cli-configured-model",
            model_display_name: "Codex CLI configured model",
            wire_api: "codex_cli",
            auth_policy: "local_codex_cli_auth_presence_only",
            redaction_policy: "mdx_context_redaction_required",
            budget_policy: "mdx_cli_runtime_budget_required",
            live_call_allowed: false,
        },
        FleetModelMatrixProfile {
            profile_id: "opencode_model_profile",
            provider_family: "opencode",
            model_provider: "opencode_configured_provider",
            example_model: "opencode-selected-model",
            model_display_name: "OpenCode selected model",
            wire_api: "opencode_cli",
            auth_policy: "local_opencode_cli_auth_presence_only",
            redaction_policy: "mdx_context_redaction_required",
            budget_policy: "mdx_cli_runtime_budget_required",
            live_call_allowed: false,
        },
        FleetModelMatrixProfile {
            profile_id: "cline_model_profile",
            provider_family: "cline",
            model_provider: "cline_configured_provider",
            example_model: "cline-selected-model",
            model_display_name: "Cline selected model",
            wire_api: "cline_cli",
            auth_policy: "local_cline_cli_auth_presence_only",
            redaction_policy: "mdx_context_redaction_required",
            budget_policy: "mdx_cli_runtime_budget_required",
            live_call_allowed: false,
        },
        FleetModelMatrixProfile {
            profile_id: "goose_model_profile",
            provider_family: "goose",
            model_provider: "goose_configured_provider",
            example_model: "goose-selected-model",
            model_display_name: "Goose selected model",
            wire_api: "goose_cli",
            auth_policy: "local_goose_cli_auth_presence_only",
            redaction_policy: "mdx_context_redaction_required",
            budget_policy: "mdx_cli_runtime_budget_required",
            live_call_allowed: false,
        },
        FleetModelMatrixProfile {
            profile_id: "xai_grok_build_model_profile",
            provider_family: "xai",
            model_provider: "xai_grok_build_cli",
            example_model: "grok-4.5",
            model_display_name: "Grok Build with Grok 4.5",
            wire_api: "acp_stdio",
            auth_policy: "xai_api_key_presence_enables_grok_build_machine_option",
            redaction_policy: "mdx_context_redaction_required",
            budget_policy: "mdx_cli_runtime_budget_required",
            live_call_allowed: false,
        },
        FleetModelMatrixProfile {
            profile_id: "claude_code_model_profile",
            provider_family: "anthropic",
            model_provider: "anthropic_claude_agent_sdk",
            example_model: "claude-agent-sdk-selected-model",
            model_display_name: "Claude Agent SDK selected model",
            wire_api: "claude_agent_sdk",
            auth_policy: "anthropic_api_key_presence_enables_claude_agent_sdk_machine_option",
            redaction_policy: "mdx_context_redaction_required",
            budget_policy: "mdx_cli_runtime_budget_required",
            live_call_allowed: false,
        },
        FleetModelMatrixProfile {
            profile_id: "kimi_code_k3_model_profile",
            provider_family: "moonshot",
            model_provider: "moonshot_kimi_code_cli",
            example_model: "kimi-k3",
            model_display_name: "Kimi Code CLI with Kimi K3",
            wire_api: "kimi_code_cli",
            auth_policy: "tenant_moonshot_api_key_injected_through_kimi_model_env_channel",
            redaction_policy: "mdx_context_and_provider_secret_redaction_required",
            budget_policy: "mdx_cli_runtime_and_live_approval_budget_required",
            live_call_allowed: false,
        },
        FleetModelMatrixProfile {
            profile_id: "codex_openai_responses_profile",
            provider_family: "openai",
            model_provider: "mdx_responses_proxy",
            example_model: "gpt-5.6-sol-via-mdx",
            model_display_name: "GPT-5.6 Sol",
            wire_api: "responses",
            auth_policy: "server_side_provider_secret_presence_only",
            redaction_policy: "mdx_context_redaction_required",
            budget_policy: "mdx_cost_and_token_budget_required",
            live_call_allowed: false,
        },
        FleetModelMatrixProfile {
            profile_id: "codex_gemini_responses_profile",
            provider_family: "gemini",
            model_provider: "mdx_responses_proxy",
            example_model: "gemini-3.1-pro-preview-via-mdx",
            model_display_name: "Gemini 3.1 Pro Preview",
            wire_api: "responses",
            auth_policy: "server_side_provider_secret_presence_only",
            redaction_policy: "mdx_context_redaction_required",
            budget_policy: "mdx_cost_and_token_budget_required",
            live_call_allowed: false,
        },
        FleetModelMatrixProfile {
            profile_id: "codex_anthropic_responses_profile",
            provider_family: "anthropic",
            model_provider: "mdx_responses_proxy",
            example_model: "claude-opus-4-8-via-mdx",
            model_display_name: "Claude Opus 4.8",
            wire_api: "responses",
            auth_policy: "server_side_provider_secret_presence_only",
            redaction_policy: "mdx_context_redaction_required",
            budget_policy: "mdx_cost_and_token_budget_required",
            live_call_allowed: false,
        },
        FleetModelMatrixProfile {
            profile_id: "codex_xai_responses_profile",
            provider_family: "xai",
            model_provider: "mdx_responses_proxy",
            example_model: "grok-4.5-via-mdx",
            model_display_name: "Grok 4.5",
            wire_api: "responses",
            auth_policy: "server_side_provider_secret_presence_only",
            redaction_policy: "mdx_context_redaction_required",
            budget_policy: "mdx_cost_and_token_budget_required",
            live_call_allowed: false,
        },
        FleetModelMatrixProfile {
            profile_id: "codex_bedrock_responses_profile",
            provider_family: "aws_bedrock",
            model_provider: "bedrock_responses_proxy",
            example_model: "bedrock:policy_selected_model_via_mdx",
            model_display_name: "Bedrock policy-selected model",
            wire_api: "responses",
            auth_policy: "server_side_aws_credentials_presence_only",
            redaction_policy: "mdx_context_redaction_required",
            budget_policy: "mdx_cost_and_token_budget_required",
            live_call_allowed: false,
        },
    ]
}

pub fn forge_language_task_corpus() -> Vec<ForgeLanguageTaskCorpusEntry> {
    vec![
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "ios-xcode-bug-fix-small",
            language_pack_id: "ios-xcode",
            repo_family: "iOS Xcode",
            task_class: "bug_fix",
            complexity_tier: "small",
            visible_check: "xcodebuild test",
            hidden_check_slot: "XCTest regression with scheme and destination",
            artifact_noise_expected: "DerivedData/**,.build/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "ios-xcode-feature-medium",
            language_pack_id: "ios-xcode",
            repo_family: "iOS Xcode",
            task_class: "feature",
            complexity_tier: "medium",
            visible_check: "xcodebuild test",
            hidden_check_slot: "view-model or service behavior fixture",
            artifact_noise_expected: "DerivedData/**,.build/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "ios-xcode-refactor-large",
            language_pack_id: "ios-xcode",
            repo_family: "iOS Xcode",
            task_class: "refactor",
            complexity_tier: "large",
            visible_check: "xcodebuild test",
            hidden_check_slot: "target and entitlement compatibility regression",
            artifact_noise_expected: "DerivedData/**,.build/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "swift-spm-bug-fix-small",
            language_pack_id: "swift-spm",
            repo_family: "Swift Package Manager",
            task_class: "bug_fix",
            complexity_tier: "small",
            visible_check: "swift test",
            hidden_check_slot: "edge case unit test",
            artifact_noise_expected: ".build/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "swift-spm-feature-medium",
            language_pack_id: "swift-spm",
            repo_family: "Swift Package Manager",
            task_class: "feature",
            complexity_tier: "medium",
            visible_check: "swift test",
            hidden_check_slot: "API behavior fixture",
            artifact_noise_expected: ".build/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "swift-spm-refactor-large",
            language_pack_id: "swift-spm",
            repo_family: "Swift Package Manager",
            task_class: "refactor",
            complexity_tier: "large",
            visible_check: "swift test",
            hidden_check_slot: "cross-target API compatibility fixture",
            artifact_noise_expected: ".build/**,DerivedData/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "java-maven-bug-fix-small",
            language_pack_id: "java-maven",
            repo_family: "Java Maven",
            task_class: "bug_fix",
            complexity_tier: "small",
            visible_check: "mvn test",
            hidden_check_slot: "JUnit regression",
            artifact_noise_expected: "target/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "java-maven-refactor-medium",
            language_pack_id: "java-maven",
            repo_family: "Java Maven",
            task_class: "refactor",
            complexity_tier: "medium",
            visible_check: "mvn test",
            hidden_check_slot: "public API compatibility check",
            artifact_noise_expected: "target/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "java-maven-feature-large",
            language_pack_id: "java-maven",
            repo_family: "Java Maven",
            task_class: "feature",
            complexity_tier: "large",
            visible_check: "mvn test",
            hidden_check_slot: "module integration plus public API regression",
            artifact_noise_expected: "target/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "gradle-jvm-ci-repair-small",
            language_pack_id: "gradle-jvm",
            repo_family: "Gradle JVM",
            task_class: "ci_repair",
            complexity_tier: "small",
            visible_check: "./gradlew test",
            hidden_check_slot: "Gradle wrapper invocation",
            artifact_noise_expected: "build/**,.gradle/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "gradle-jvm-feature-medium",
            language_pack_id: "gradle-jvm",
            repo_family: "Gradle JVM",
            task_class: "feature",
            complexity_tier: "medium",
            visible_check: "./gradlew test",
            hidden_check_slot: "Kotlin or Java integration test",
            artifact_noise_expected: "build/**,.gradle/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "gradle-jvm-refactor-large",
            language_pack_id: "gradle-jvm",
            repo_family: "Gradle JVM",
            task_class: "refactor",
            complexity_tier: "large",
            visible_check: "./gradlew test",
            hidden_check_slot: "multi-module compatibility regression",
            artifact_noise_expected: "build/**,.gradle/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "android-gradle-bug-fix-small",
            language_pack_id: "android-gradle",
            repo_family: "Android Gradle",
            task_class: "bug_fix",
            complexity_tier: "small",
            visible_check: "./gradlew testDebugUnitTest",
            hidden_check_slot: "Android local unit regression",
            artifact_noise_expected: "build/**,.gradle/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "android-gradle-feature-medium",
            language_pack_id: "android-gradle",
            repo_family: "Android Gradle",
            task_class: "feature",
            complexity_tier: "medium",
            visible_check: "./gradlew testDebugUnitTest",
            hidden_check_slot: "viewmodel or repository behavior fixture",
            artifact_noise_expected: "build/**,.gradle/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "android-gradle-refactor-large",
            language_pack_id: "android-gradle",
            repo_family: "Android Gradle",
            task_class: "refactor",
            complexity_tier: "large",
            visible_check: "./gradlew testDebugUnitTest",
            hidden_check_slot: "module, manifest, and resource compatibility regression",
            artifact_noise_expected: "build/**,.gradle/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "dotnet-bug-fix-small",
            language_pack_id: "dotnet",
            repo_family: ".NET",
            task_class: "bug_fix",
            complexity_tier: "small",
            visible_check: "dotnet test",
            hidden_check_slot: "xUnit or NUnit regression",
            artifact_noise_expected: "bin/**,obj/**,TestResults/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "dotnet-feature-medium",
            language_pack_id: "dotnet",
            repo_family: ".NET",
            task_class: "feature",
            complexity_tier: "medium",
            visible_check: "dotnet test",
            hidden_check_slot: "service or library behavior fixture",
            artifact_noise_expected: "bin/**,obj/**,TestResults/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "dotnet-refactor-large",
            language_pack_id: "dotnet",
            repo_family: ".NET",
            task_class: "refactor",
            complexity_tier: "large",
            visible_check: "dotnet test",
            hidden_check_slot: "solution-level behavior and nullability regression",
            artifact_noise_expected: "bin/**,obj/**,TestResults/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "node-bug-fix-small",
            language_pack_id: "node",
            repo_family: "Node",
            task_class: "bug_fix",
            complexity_tier: "small",
            visible_check: "npm test",
            hidden_check_slot: "unit regression",
            artifact_noise_expected: "node_modules/**,dist/**,coverage/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "node-performance-medium",
            language_pack_id: "node",
            repo_family: "Node",
            task_class: "performance",
            complexity_tier: "medium",
            visible_check: "npm test",
            hidden_check_slot: "behavior plus budget assertion",
            artifact_noise_expected: "node_modules/**,dist/**,coverage/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "node-feature-large",
            language_pack_id: "node",
            repo_family: "Node",
            task_class: "feature",
            complexity_tier: "large",
            visible_check: "npm test",
            hidden_check_slot: "integration behavior and package-script regression",
            artifact_noise_expected: "node_modules/**,dist/**,coverage/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "rust-cargo-bug-fix-small",
            language_pack_id: "rust-cargo",
            repo_family: "Rust Cargo",
            task_class: "bug_fix",
            complexity_tier: "small",
            visible_check: "cargo test",
            hidden_check_slot: "unit regression",
            artifact_noise_expected: "target/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "rust-cargo-security-medium",
            language_pack_id: "rust-cargo",
            repo_family: "Rust Cargo",
            task_class: "security",
            complexity_tier: "medium",
            visible_check: "cargo test",
            hidden_check_slot: "negative abuse case",
            artifact_noise_expected: "target/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "rust-cargo-refactor-large",
            language_pack_id: "rust-cargo",
            repo_family: "Rust Cargo",
            task_class: "refactor",
            complexity_tier: "large",
            visible_check: "cargo test",
            hidden_check_slot: "public API and error-path regression",
            artifact_noise_expected: "target/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "python-bug-fix-small",
            language_pack_id: "python",
            repo_family: "Python",
            task_class: "bug_fix",
            complexity_tier: "small",
            visible_check: "pytest",
            hidden_check_slot: "pytest regression",
            artifact_noise_expected: ".pytest_cache/**,__pycache__/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "python-feature-medium",
            language_pack_id: "python",
            repo_family: "Python",
            task_class: "feature",
            complexity_tier: "medium",
            visible_check: "pytest",
            hidden_check_slot: "fixture or property-style behavior check",
            artifact_noise_expected: ".pytest_cache/**,__pycache__/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "python-refactor-large",
            language_pack_id: "python",
            repo_family: "Python",
            task_class: "refactor",
            complexity_tier: "large",
            visible_check: "pytest",
            hidden_check_slot: "fixture matrix and import-boundary regression",
            artifact_noise_expected: ".pytest_cache/**,__pycache__/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "go-bug-fix-small",
            language_pack_id: "go",
            repo_family: "Go modules",
            task_class: "bug_fix",
            complexity_tier: "small",
            visible_check: "go test ./...",
            hidden_check_slot: "table-driven regression",
            artifact_noise_expected: "bin/**,coverage/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "go-refactor-medium",
            language_pack_id: "go",
            repo_family: "Go modules",
            task_class: "refactor",
            complexity_tier: "medium",
            visible_check: "go test ./...",
            hidden_check_slot: "compatibility behavior test",
            artifact_noise_expected: "bin/**,coverage/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
        ForgeLanguageTaskCorpusEntry {
            task_corpus_id: "go-feature-large",
            language_pack_id: "go",
            repo_family: "Go modules",
            task_class: "feature",
            complexity_tier: "large",
            visible_check: "go test ./...",
            hidden_check_slot: "package integration and table-driven regression",
            artifact_noise_expected: "bin/**,coverage/**",
            required_principal_review_gates: PRINCIPAL_REVIEW_GATE_IDS,
        },
    ]
}

pub fn forge_language_pack_scorecards() -> Vec<ForgeLanguagePackScorecard> {
    let corpus = forge_language_task_corpus();
    let mut language_packs = Vec::<(&'static str, &'static str)>::new();
    for entry in &corpus {
        if !language_packs
            .iter()
            .any(|(language_pack_id, _)| *language_pack_id == entry.language_pack_id)
        {
            language_packs.push((entry.language_pack_id, entry.repo_family));
        }
    }

    language_packs
        .into_iter()
        .map(|(language_pack_id, repo_family)| {
            let entries = corpus
                .iter()
                .filter(|entry| entry.language_pack_id == language_pack_id)
                .collect::<Vec<_>>();
            let task_count = entries.len() as u32;
            let small_task_count = entries
                .iter()
                .filter(|entry| entry.complexity_tier == "small")
                .count() as u32;
            let medium_task_count = entries
                .iter()
                .filter(|entry| entry.complexity_tier == "medium")
                .count() as u32;
            let large_task_count = entries
                .iter()
                .filter(|entry| entry.complexity_tier == "large")
                .count() as u32;
            let visible_check_count =
                unique_static_count(entries.iter().map(|entry| entry.visible_check));
            let hidden_check_slot_count =
                unique_static_count(entries.iter().map(|entry| entry.hidden_check_slot));
            let artifact_noise_expectation_count =
                unique_static_count(entries.iter().map(|entry| entry.artifact_noise_expected));
            let principal_review_gate_count = unique_static_count(
                entries
                    .iter()
                    .flat_map(|entry| entry.required_principal_review_gates.split(',')),
            );
            let principal_verdict_required = entries.iter().all(|entry| {
                !entry.task_corpus_id.trim().is_empty()
                    && !entry.hidden_check_slot.trim().is_empty()
                    && !entry.artifact_noise_expected.trim().is_empty()
                    && !entry.required_principal_review_gates.trim().is_empty()
                    && !language_task_engineering_facets(entry).trim().is_empty()
                    && !language_task_evaluation_oracle(entry).trim().is_empty()
                    && language_task_human_timebox_minutes(entry) >= 30
                    && language_task_contamination_policy(entry).contains("held-out")
            });
            let ready_for_live_eval = task_count >= 2
                && small_task_count > 0
                && medium_task_count > 0
                && large_task_count > 0
                && visible_check_count > 0
                && hidden_check_slot_count > 0
                && artifact_noise_expectation_count > 0
                && principal_review_gate_count >= 6
                && principal_verdict_required;

            ForgeLanguagePackScorecard {
                language_pack_id,
                repo_family,
                task_count,
                small_task_count,
                medium_task_count,
                large_task_count,
                visible_check_count,
                hidden_check_slot_count,
                artifact_noise_expectation_count,
                principal_review_gate_count,
                principal_verdict_required,
                ready_for_live_eval,
            }
        })
        .collect()
}

fn unique_static_count(values: impl Iterator<Item = &'static str>) -> u32 {
    let mut unique = Vec::<&'static str>::new();
    for value in values {
        if !unique.contains(&value) {
            unique.push(value);
        }
    }
    unique.len() as u32
}

fn benchmark_task_for_language_task<'a>(
    language_task: &ForgeLanguageTaskCorpusEntry,
    tasks: &'a [FleetBenchmarkTask],
) -> Option<&'a FleetBenchmarkTask> {
    tasks
        .iter()
        .find(|task| {
            task.class == language_task.task_class
                && task.complexity_tier == language_task.complexity_tier
        })
        .or_else(|| {
            tasks
                .iter()
                .find(|task| task.class == language_task.task_class)
        })
        .or_else(|| {
            tasks
                .iter()
                .find(|task| task.complexity_tier == language_task.complexity_tier)
        })
        .or_else(|| tasks.first())
}

pub fn run_forge_fleet_eval() -> ForgeFleetEvalReport {
    let tasks = forge_fleet_benchmark_tasks();
    let language_task_corpus = forge_language_task_corpus();
    let runners = forge_fleet_runner_profiles();
    let scoring_dimensions = forge_fleet_scoring_dimensions();
    let runner_scores = runners.iter().map(score_runner).collect::<Vec<_>>();
    // No winner is declared from the zero baseline. The scoreboard route
    // recomputes the winner from accepted receipt evidence with a minimum
    // trial floor; an empty id means no runner has earned the slot yet.
    let winning_runner_id = String::new();
    let parallel_load = run_load_sim(&LoadSimConfig {
        policy: fleet_eval_scale_policy(),
        tenants: 8,
        engineers_per_tenant: 16,
        jobs_per_engineer: 4,
        drain_every: 5,
        cancel_every: 11,
        fail_every: 13,
        retry_budget: 2,
    });
    ForgeFleetEvalReport {
        status: "RECEIPT-BACKED-FLEET-EVAL-SCOREBOARD".to_string(),
        benchmark_tasks: tasks,
        language_task_corpus,
        language_pack_scorecards: forge_language_pack_scorecards(),
        scoring_dimensions,
        runner_scores,
        model_matrix: forge_fleet_model_matrix_profiles(),
        winning_runner_id,
        codex_adapter_status: "PROFILE_ADMITTED_ADAPTER_EXECUTION_DENIED".to_string(),
        codex_model_freedom_rule:
            "models_must_be_behind_mdx_approved_responses_compatible_provider_profile".to_string(),
        evidence_policy: "external_outputs_quarantined_until_mdx_quality_gates".to_string(),
        parallel_agents_target: 128,
        parallel_load,
        data_decides: true,
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn run_forge_fleet_eval_dry_run_local(
        &mut self,
        dry_run_id: &str,
    ) -> Result<ForgeFleetEvalDryRunReport, ForgeFleetEvalResultIngestionError> {
        let identity = GovernedWriteIdentity::local_demo("agent:codex");
        self.run_forge_fleet_eval_dry_run_local_with_identity(dry_run_id, &identity)
    }

    pub fn run_forge_fleet_eval_dry_run_local_with_identity(
        &mut self,
        dry_run_id: &str,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeFleetEvalDryRunReport, ForgeFleetEvalResultIngestionError> {
        if dry_run_id.trim().is_empty() {
            return Err(ForgeFleetEvalResultIngestionError::Missing("dry_run_id"));
        }
        let tenant_id = "local_tenant";
        let actor_id = identity.subject_actor_id.as_str();
        let actor_role = "worker";
        let runner = forge_fleet_runner_profiles()
            .into_iter()
            .find(|runner| runner.runner_id == "codex_cli_external_worker")
            .expect("codex runner profile declared");
        let tasks = forge_fleet_benchmark_tasks();
        let model_profiles = forge_fleet_model_matrix_profiles();
        let scoring_dimensions = forge_fleet_scoring_dimensions();
        let actor_admission = admit_local_route_actor(
            tenant_id,
            actor_id,
            actor_role,
            "/forge/fleet-eval-dry-runs.json",
            "forge.fleet_eval_runner.execution.recorded",
            dry_run_id,
        )
        .map_err(|error| ForgeFleetEvalResultIngestionError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(actor_id),
            loop_id: LoopId::new("forge_fleet_eval_dry_run"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let mut runner_execution_receipt_ids = Vec::new();
        let mut result_receipt_ids = Vec::new();
        let mut score_receipt_ids = Vec::new();
        let language_tasks = forge_language_task_corpus();

        for language_task in &language_tasks {
            let task = benchmark_task_for_language_task(language_task, &tasks)
                .expect("benchmark task corpus is non-empty");
            for profile in &model_profiles {
                let submission_id = format!(
                    "{dry_run_id}_{}_{}",
                    language_task.task_corpus_id, profile.profile_id
                );
                let output_artifact_ref = format!(
                    "quarantine://forge/fleet-eval/{dry_run_id}/{}/{}.patch",
                    language_task.task_corpus_id, profile.profile_id
                );
                let output_artifact_sha256 =
                    format!("sha256:{}", hex(&sha256(output_artifact_ref.as_bytes())));
                let transcript_ref = format!(
                    "quarantine://forge/fleet-eval/{dry_run_id}/{}/{}.jsonl",
                    language_task.task_corpus_id, profile.profile_id
                );
                let prompt_bundle_sha256 =
                    format!("sha256:{}", hex(&sha256(b"dry-run-prompt-bundle")));
                let pr_handoff_ref = format!(
                    "quarantine://forge/fleet-eval/{dry_run_id}/{}/{}-pr.md",
                    language_task.task_corpus_id, profile.profile_id
                );
                let runner_decision = self.decide_with_receipt(
                    &correlation,
                    ActionKind::RecordForgeFleetEvalRunnerExecution,
                );
                let runner_receipt = self.transition_receipt(
                    &run_id,
                    "RECORD_FORGE_FLEET_EVAL_RUNNER_EXECUTION",
                    &correlation,
                    &runner_decision,
                    "forge.fleet_eval_runner.execution.recorded",
                    payload(&[
                        ("dry_run_id", dry_run_id),
                        ("submission_id", &submission_id),
                        ("source_route", "/forge/fleet-eval-dry-runs.json"),
                        (
                            "projection_route",
                            "/forge/fleet-eval-dry-runs/projection.json",
                        ),
                        ("result_ingestion_route", "/forge/fleet-eval-results.json"),
                        ("scoreboard_route", "/forge/fleet-eval-scoreboard.json"),
                        ("source_contract", "docs/FORGE-FLEET-EVAL-HARNESS.md"),
                        ("actor_admission_status", actor_admission.status),
                        (
                            "actor_admission_policy_decision_id",
                            &actor_admission.policy_decision_id,
                        ),
                        ("runner_id", runner.runner_id),
                        ("runner_kind", runner.runner_kind),
                        ("benchmark_task_id", task.task_id),
                        ("benchmark_task_class", task.class),
                        ("complexity_tier", task.complexity_tier),
                        ("language_task_corpus_id", language_task.task_corpus_id),
                        ("language_pack_id", language_task.language_pack_id),
                        ("repo_family", language_task.repo_family),
                        ("language_task_class", language_task.task_class),
                        ("language_complexity_tier", language_task.complexity_tier),
                        ("visible_check", language_task.visible_check),
                        ("hidden_check_slot", language_task.hidden_check_slot),
                        (
                            "artifact_noise_expected",
                            language_task.artifact_noise_expected,
                        ),
                        (
                            "principal_engineer_verdict",
                            "pending_principal_engineer_review",
                        ),
                        (
                            "language_engineering_facets",
                            language_task_engineering_facets(language_task),
                        ),
                        (
                            "language_evaluation_oracle",
                            language_task_evaluation_oracle(language_task),
                        ),
                        (
                            "language_human_timebox_minutes",
                            &language_task_human_timebox_minutes(language_task).to_string(),
                        ),
                        (
                            "language_contamination_policy",
                            language_task_contamination_policy(language_task),
                        ),
                        ("engineering_facets", task.engineering_facets),
                        ("expected_check", task.expected_check),
                        ("evaluation_oracle", task.evaluation_oracle),
                        (
                            "human_timebox_minutes",
                            &task.human_timebox_minutes.to_string(),
                        ),
                        ("contamination_policy", task.contamination_policy),
                        ("model_profile_id", profile.profile_id),
                        ("provider_family", profile.provider_family),
                        ("model_provider", profile.model_provider),
                        ("model_id", profile.example_model),
                        ("wire_api", profile.wire_api),
                        ("prompt_bundle_sha256", &prompt_bundle_sha256),
                        ("sandbox_profile", "dry_run_no_network_no_secret"),
                        ("output_artifact_ref", &output_artifact_ref),
                        ("output_artifact_sha256", &output_artifact_sha256),
                        ("transcript_ref", &transcript_ref),
                        ("claimed_check", language_task.visible_check),
                        (
                            "scoring_dimension_count",
                            &scoring_dimensions.len().to_string(),
                        ),
                        ("adapter_execution_allowed", "false"),
                        ("provider_secret_export_allowed", "false"),
                        ("live_provider_call_allowed", "false"),
                        ("live_provider_call_performed", "false"),
                        ("tokens_in", "0"),
                        ("tokens_out", "0"),
                        ("cost_cents", "0"),
                        ("runtime_ms", "0"),
                        ("exit_status", "DRY_RUN_NOT_EXECUTED"),
                    ]),
                );
                runner_execution_receipt_ids.push(runner_receipt.receipt_id);

                let result = self.ingest_forge_fleet_eval_result_local_with_identity(
                    ForgeFleetEvalResultSubmission {
                        tenant_id,
                        actor_id,
                        actor_role,
                        submission_id: &submission_id,
                        benchmark_task_id: task.task_id,
                        language_task_corpus_id: language_task.task_corpus_id,
                        language_pack_id: language_task.language_pack_id,
                        repo_family: language_task.repo_family,
                        runner_id: runner.runner_id,
                        model_profile_id: profile.profile_id,
                        output_artifact_ref: &output_artifact_ref,
                        output_artifact_sha256: &output_artifact_sha256,
                        transcript_ref: &transcript_ref,
                        claimed_check: language_task.visible_check,
                        hidden_check_slot: language_task.hidden_check_slot,
                        artifact_noise_expected: language_task.artifact_noise_expected,
                        principal_review_gate_results: "behavior=dry_run_pending_live_execution;tests=dry_run_pending_live_execution;stack_idioms=dry_run_pending_live_execution;compatibility=dry_run_pending_live_execution;security=dry_run_pending_live_execution;maintainability=dry_run_pending_live_execution",
                        standards_source_fingerprints: "AGENTS.md=fnv1a64:0000000000000000",
                        artifact_filter_summary: "dry_run_artifact_filter_not_applied",
                        diff_language_pack_impact: language_task.language_pack_id,
                        principal_engineer_verdict: "pending_principal_engineer_review",
                        pr_handoff_ref: &pr_handoff_ref,
                    },
                    identity,
                )?;
                result_receipt_ids.push(result.result_receipt_id.clone());

                let score_decision =
                    self.decide_with_receipt(&correlation, ActionKind::ScoreForgeFleetEvalResult);
                let score_receipt = self.transition_receipt(
                    &run_id,
                    "SCORE_FORGE_FLEET_EVAL_RESULT",
                    &correlation,
                    &score_decision,
                    "forge.fleet_eval_result.scored",
                    payload(&[
                        ("dry_run_id", dry_run_id),
                        ("submission_id", &submission_id),
                        ("source_route", "/forge/fleet-eval-dry-runs.json"),
                        (
                            "projection_route",
                            "/forge/fleet-eval-dry-runs/projection.json",
                        ),
                        ("scoreboard_route", "/forge/fleet-eval-scoreboard.json"),
                        ("result_receipt_id", &result.result_receipt_id),
                        ("result_policy_decision_id", &result.policy_decision_id),
                        ("runner_id", runner.runner_id),
                        ("benchmark_task_id", task.task_id),
                        ("language_task_corpus_id", &result.language_task_corpus_id),
                        ("language_pack_id", &result.language_pack_id),
                        ("repo_family", &result.repo_family),
                        ("hidden_check_slot", language_task.hidden_check_slot),
                        (
                            "artifact_noise_expected",
                            language_task.artifact_noise_expected,
                        ),
                        (
                            "diff_language_pack_impact",
                            &result.diff_language_pack_impact,
                        ),
                        (
                            "principal_engineer_verdict",
                            "pending_principal_engineer_review",
                        ),
                        (
                            "language_engineering_facets",
                            language_task_engineering_facets(language_task),
                        ),
                        (
                            "language_evaluation_oracle",
                            language_task_evaluation_oracle(language_task),
                        ),
                        (
                            "language_human_timebox_minutes",
                            &language_task_human_timebox_minutes(language_task).to_string(),
                        ),
                        (
                            "language_contamination_policy",
                            language_task_contamination_policy(language_task),
                        ),
                        ("model_profile_id", profile.profile_id),
                        ("provider_family", profile.provider_family),
                        ("model_provider", profile.model_provider),
                        ("model_id", profile.example_model),
                        ("correctness_score", "0"),
                        ("regression_test_quality_score", "0"),
                        ("patch_quality_score", "0"),
                        ("architecture_fit_score", "0"),
                        ("test_evidence_score", "0"),
                        ("security_and_policy_score", "100"),
                        ("maintainability_score", "0"),
                        ("observability_score", "0"),
                        ("performance_score", "0"),
                        ("migration_and_compatibility_score", "0"),
                        ("policy_score", "100"),
                        ("cost_score", "100"),
                        ("latency_score", "100"),
                        ("cost_latency_budget_score", "100"),
                        ("handoff_quality_score", "0"),
                        ("total_score", "24"),
                        (
                            "scoring_dimension_count",
                            &scoring_dimensions.len().to_string(),
                        ),
                        ("artifact_hash_present", "true"),
                        ("transcript_present", "true"),
                        ("claimed_check_matches_expected", "true"),
                        ("live_provider_output_present", "false"),
                        ("mdx_quality_gates_passed", "false"),
                        ("accepted_for_scoreboard", "false"),
                        ("external_output_consumable", "false"),
                        ("blocked_reason", "dry_run_waiting_for_live_provider_keys"),
                    ]),
                );
                score_receipt_ids.push(score_receipt.receipt_id);
            }
        }
        self.finish_forge_fleet_eval_dry_run(&run_id);
        Ok(ForgeFleetEvalDryRunReport {
            status: "FLEET_EVAL_DRY_RUN_READY_FOR_KEYS",
            dry_run_id: dry_run_id.to_string(),
            runner_id: runner.runner_id.to_string(),
            benchmark_task_count: tasks.len() as u32,
            language_task_count: language_tasks.len() as u32,
            model_profile_count: model_profiles.len() as u32,
            scoring_dimension_count: scoring_dimensions.len() as u32,
            dry_run_case_count: (language_tasks.len() * model_profiles.len()) as u32,
            runner_execution_receipt_ids,
            result_receipt_ids,
            score_receipt_ids,
            live_provider_calls_allowed: false,
            live_provider_calls_performed: false,
            provider_credentials_required_for_live: true,
            ready_for_live_credentials: true,
            accepted_for_scoreboard_count: 0,
            quarantined_count: (language_tasks.len() * model_profiles.len()) as u32,
            blocked_reason: "dry_run_waiting_for_live_provider_keys",
        })
    }

    pub fn approve_forge_fleet_eval_live_run_local(
        &mut self,
        approval: ForgeFleetEvalLiveRunApproval<'_>,
    ) -> Result<ForgeFleetEvalLiveRunApprovalReport, ForgeFleetEvalResultIngestionError> {
        let identity = GovernedWriteIdentity::local_demo(approval.actor_id);
        self.approve_forge_fleet_eval_live_run_local_with_identity(approval, &identity)
    }

    pub fn approve_forge_fleet_eval_live_run_local_with_identity(
        &mut self,
        approval: ForgeFleetEvalLiveRunApproval<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeFleetEvalLiveRunApprovalReport, ForgeFleetEvalResultIngestionError> {
        for (field, value) in [
            ("tenant_id", approval.tenant_id),
            ("actor_id", approval.actor_id),
            ("actor_role", approval.actor_role),
            ("approval_id", approval.approval_id),
            ("provider_allowlist", approval.provider_allowlist),
            (
                "artifact_retention_policy",
                approval.artifact_retention_policy,
            ),
            ("redaction_policy", approval.redaction_policy),
            ("stop_conditions", approval.stop_conditions),
        ] {
            if value.trim().is_empty() {
                return Err(ForgeFleetEvalResultIngestionError::Missing(field));
            }
        }
        if approval.max_spend_cents == 0 {
            return Err(ForgeFleetEvalResultIngestionError::InvalidApprovalField(
                "max_spend_cents",
            ));
        }
        if approval.max_tasks == 0
            || approval.max_tasks > forge_fleet_benchmark_tasks().len() as u32
        {
            return Err(ForgeFleetEvalResultIngestionError::InvalidApprovalField(
                "max_tasks",
            ));
        }
        if approval.max_parallel_agents == 0
            || approval.max_parallel_agents > fleet_eval_scale_policy().max_concurrent_workers
        {
            return Err(ForgeFleetEvalResultIngestionError::InvalidApprovalField(
                "max_parallel_agents",
            ));
        }
        let normalized_allowlist = normalize_provider_allowlist(approval.provider_allowlist)?;
        let actor_admission = admit_local_route_actor(
            approval.tenant_id,
            approval.actor_id,
            approval.actor_role,
            "/forge/fleet-eval-live-run-approvals.json",
            "forge.fleet_eval_live_run.approved",
            approval.approval_id,
        )
        .map_err(|error| ForgeFleetEvalResultIngestionError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(approval.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(approval.actor_id),
            loop_id: LoopId::new("forge_fleet_eval_live_run_approval"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision =
            self.decide_with_receipt(&correlation, ActionKind::ApproveForgeFleetEvalLiveRun);
        let max_spend_cents = approval.max_spend_cents.to_string();
        let max_tasks = approval.max_tasks.to_string();
        let max_parallel_agents = approval.max_parallel_agents.to_string();
        let receipt = self.transition_receipt(
            &run_id,
            "APPROVE_FORGE_FLEET_EVAL_LIVE_RUN",
            &correlation,
            &decision,
            "forge.fleet_eval_live_run.approved",
            payload(&[
                ("approval_id", approval.approval_id),
                ("source_route", "/forge/fleet-eval-live-run-approvals.json"),
                (
                    "projection_route",
                    "/forge/fleet-eval-live-run-approvals/projection.json",
                ),
                (
                    "provider_preflight_route",
                    "/forge/fleet-eval-provider-preflight.json",
                ),
                ("dry_run_route", "/forge/fleet-eval-dry-runs.json"),
                ("scoreboard_route", "/forge/fleet-eval-scoreboard.json"),
                ("source_contract", "docs/FORGE-FLEET-EVAL-HARNESS.md"),
                ("auth_session_route", "/local/auth-session.json"),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("provider_allowlist", &normalized_allowlist),
                ("max_spend_cents", &max_spend_cents),
                ("max_tasks", &max_tasks),
                ("max_parallel_agents", &max_parallel_agents),
                (
                    "artifact_retention_policy",
                    approval.artifact_retention_policy,
                ),
                ("redaction_policy", approval.redaction_policy),
                ("stop_conditions", approval.stop_conditions),
                ("provider_credentials_required", "true"),
                ("provider_secret_values_recorded", "false"),
                ("provider_secret_export_allowed", "false"),
                ("live_provider_calls_allowed", "false"),
                ("approval_grants_execution_authority", "false"),
                ("adapter_execution_allowed", "false"),
                ("production_write_allowed", "false"),
                (
                    "blocked_reason",
                    "approval_recorded_waiting_for_credentials_and_live_adapter",
                ),
            ]),
        );
        self.finish_forge_fleet_eval_live_run_approval(&run_id);
        Ok(ForgeFleetEvalLiveRunApprovalReport {
            status: "FLEET_EVAL_LIVE_RUN_APPROVAL_RECORDED",
            approval_id: approval.approval_id.to_string(),
            approval_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            provider_allowlist: normalized_allowlist,
            max_spend_cents: approval.max_spend_cents,
            max_tasks: approval.max_tasks,
            max_parallel_agents: approval.max_parallel_agents,
            artifact_retention_policy: approval.artifact_retention_policy.to_string(),
            redaction_policy: approval.redaction_policy.to_string(),
            stop_conditions: approval.stop_conditions.to_string(),
            provider_credentials_required: true,
            live_provider_calls_allowed: false,
            approval_grants_execution_authority: false,
            adapter_execution_allowed: false,
            production_write_allowed: false,
            blocked_reason: "approval_recorded_waiting_for_credentials_and_live_adapter",
        })
    }

    pub fn ingest_forge_fleet_eval_result_local(
        &mut self,
        submission: ForgeFleetEvalResultSubmission<'_>,
    ) -> Result<ForgeFleetEvalResultIngestionReport, ForgeFleetEvalResultIngestionError> {
        let identity = GovernedWriteIdentity::local_demo(submission.actor_id);
        self.ingest_forge_fleet_eval_result_local_with_identity(submission, &identity)
    }

    pub fn ingest_forge_fleet_eval_result_local_with_identity(
        &mut self,
        submission: ForgeFleetEvalResultSubmission<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ForgeFleetEvalResultIngestionReport, ForgeFleetEvalResultIngestionError> {
        for (field, value) in [
            ("tenant_id", submission.tenant_id),
            ("actor_id", submission.actor_id),
            ("actor_role", submission.actor_role),
            ("submission_id", submission.submission_id),
            ("benchmark_task_id", submission.benchmark_task_id),
            (
                "language_task_corpus_id",
                submission.language_task_corpus_id,
            ),
            ("language_pack_id", submission.language_pack_id),
            ("repo_family", submission.repo_family),
            ("runner_id", submission.runner_id),
            ("model_profile_id", submission.model_profile_id),
            ("output_artifact_ref", submission.output_artifact_ref),
            ("output_artifact_sha256", submission.output_artifact_sha256),
            ("transcript_ref", submission.transcript_ref),
            ("claimed_check", submission.claimed_check),
            ("hidden_check_slot", submission.hidden_check_slot),
            (
                "artifact_noise_expected",
                submission.artifact_noise_expected,
            ),
            (
                "principal_review_gate_results",
                submission.principal_review_gate_results,
            ),
            (
                "standards_source_fingerprints",
                submission.standards_source_fingerprints,
            ),
            (
                "artifact_filter_summary",
                submission.artifact_filter_summary,
            ),
            (
                "diff_language_pack_impact",
                submission.diff_language_pack_impact,
            ),
            (
                "principal_engineer_verdict",
                submission.principal_engineer_verdict,
            ),
            ("pr_handoff_ref", submission.pr_handoff_ref),
        ] {
            if value.trim().is_empty() {
                return Err(ForgeFleetEvalResultIngestionError::Missing(field));
            }
        }
        let task = forge_fleet_benchmark_tasks()
            .into_iter()
            .find(|task| task.task_id == submission.benchmark_task_id)
            .ok_or_else(|| {
                ForgeFleetEvalResultIngestionError::UnknownBenchmarkTask(
                    submission.benchmark_task_id.to_string(),
                )
            })?;
        let language_task = forge_language_task_corpus()
            .into_iter()
            .find(|task| task.task_corpus_id == submission.language_task_corpus_id)
            .ok_or_else(|| {
                ForgeFleetEvalResultIngestionError::UnknownLanguageTaskCorpus(
                    submission.language_task_corpus_id.to_string(),
                )
            })?;
        validate_language_task_submission(&language_task, &submission)?;
        validate_result_artifact_identity(&submission)?;
        validate_standards_source_fingerprints(&submission)?;
        validate_ingested_principal_engineer_verdict(&submission)?;
        let _runner = forge_fleet_runner_profiles()
            .into_iter()
            .find(|runner| runner.runner_id == submission.runner_id)
            .ok_or_else(|| {
                ForgeFleetEvalResultIngestionError::UnknownRunner(submission.runner_id.to_string())
            })?;
        let _model_profile = forge_fleet_model_matrix_profiles()
            .into_iter()
            .find(|profile| profile.profile_id == submission.model_profile_id)
            .ok_or_else(|| {
                ForgeFleetEvalResultIngestionError::UnknownModelProfile(
                    submission.model_profile_id.to_string(),
                )
            })?;
        let actor_admission = admit_local_route_actor(
            submission.tenant_id,
            submission.actor_id,
            submission.actor_role,
            "/forge/fleet-eval-results.json",
            "forge.fleet_eval_result.ingested",
            submission.submission_id,
        )
        .map_err(|error| ForgeFleetEvalResultIngestionError::ActorAdmission(error.message()))?;
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(submission.tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(submission.actor_id),
            loop_id: LoopId::new("forge_fleet_eval_result"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision =
            self.decide_with_receipt(&correlation, ActionKind::IngestForgeFleetEvalResult);
        let receipt = self.transition_receipt(
            &run_id,
            "INGEST_FORGE_FLEET_EVAL_RESULT",
            &correlation,
            &decision,
            "forge.fleet_eval_result.ingested",
            payload(&[
                ("submission_id", submission.submission_id),
                ("source_route", "/forge/fleet-eval-results.json"),
                (
                    "projection_route",
                    "/forge/fleet-eval-results/projection.json",
                ),
                ("scoreboard_route", "/forge/fleet-eval-scoreboard.json"),
                ("source_contract", "docs/FORGE-FLEET-EVAL-HARNESS.md"),
                ("auth_session_route", "/local/auth-session.json"),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("actor_admission_status", actor_admission.status),
                (
                    "actor_admission_policy_decision_id",
                    &actor_admission.policy_decision_id,
                ),
                ("benchmark_task_id", submission.benchmark_task_id),
                ("benchmark_task_class", task.class),
                ("complexity_tier", task.complexity_tier),
                (
                    "language_task_corpus_id",
                    submission.language_task_corpus_id,
                ),
                ("language_pack_id", submission.language_pack_id),
                ("repo_family", submission.repo_family),
                ("language_task_class", language_task.task_class),
                ("language_complexity_tier", language_task.complexity_tier),
                ("visible_check", language_task.visible_check),
                ("hidden_check_slot", submission.hidden_check_slot),
                (
                    "artifact_noise_expected",
                    submission.artifact_noise_expected,
                ),
                (
                    "required_principal_review_gates",
                    language_task.required_principal_review_gates,
                ),
                (
                    "principal_review_gate_results",
                    submission.principal_review_gate_results,
                ),
                (
                    "standards_source_fingerprints",
                    submission.standards_source_fingerprints,
                ),
                (
                    "artifact_filter_summary",
                    submission.artifact_filter_summary,
                ),
                (
                    "diff_language_pack_impact",
                    submission.diff_language_pack_impact,
                ),
                (
                    "principal_engineer_verdict",
                    submission.principal_engineer_verdict,
                ),
                (
                    "language_engineering_facets",
                    language_task_engineering_facets(&language_task),
                ),
                (
                    "language_evaluation_oracle",
                    language_task_evaluation_oracle(&language_task),
                ),
                (
                    "language_human_timebox_minutes",
                    &language_task_human_timebox_minutes(&language_task).to_string(),
                ),
                (
                    "language_contamination_policy",
                    language_task_contamination_policy(&language_task),
                ),
                ("pr_handoff_ref", submission.pr_handoff_ref),
                ("engineering_facets", task.engineering_facets),
                ("expected_check", task.expected_check),
                ("evaluation_oracle", task.evaluation_oracle),
                (
                    "human_timebox_minutes",
                    &task.human_timebox_minutes.to_string(),
                ),
                ("contamination_policy", task.contamination_policy),
                ("runner_id", submission.runner_id),
                ("model_profile_id", submission.model_profile_id),
                ("output_artifact_ref", submission.output_artifact_ref),
                ("output_artifact_sha256", submission.output_artifact_sha256),
                ("transcript_ref", submission.transcript_ref),
                ("claimed_check", submission.claimed_check),
                ("output_quarantined", "true"),
                ("external_output_consumable", "false"),
                ("mdx_quality_gates_required", "true"),
                ("mdx_quality_gates_passed", "false"),
                ("accepted_for_scoreboard", "false"),
                ("blocked_reason", "pending_mdx_quality_gates"),
                ("production_write_allowed", "false"),
            ]),
        );
        self.finish_forge_fleet_eval_result_run(&run_id);
        Ok(ForgeFleetEvalResultIngestionReport {
            status: "FLEET_EVAL_RESULT_QUARANTINED_PENDING_MDX_GATES",
            submission_id: submission.submission_id.to_string(),
            benchmark_task_id: submission.benchmark_task_id.to_string(),
            language_task_corpus_id: submission.language_task_corpus_id.to_string(),
            language_pack_id: submission.language_pack_id.to_string(),
            repo_family: submission.repo_family.to_string(),
            runner_id: submission.runner_id.to_string(),
            model_profile_id: submission.model_profile_id.to_string(),
            principal_review_gate_results: submission.principal_review_gate_results.to_string(),
            standards_source_fingerprints: submission.standards_source_fingerprints.to_string(),
            artifact_filter_summary: submission.artifact_filter_summary.to_string(),
            diff_language_pack_impact: submission.diff_language_pack_impact.to_string(),
            language_engineering_facets: language_task_engineering_facets(&language_task)
                .to_string(),
            language_evaluation_oracle: language_task_evaluation_oracle(&language_task).to_string(),
            language_human_timebox_minutes: language_task_human_timebox_minutes(&language_task),
            language_contamination_policy: language_task_contamination_policy(&language_task)
                .to_string(),
            pr_handoff_ref: submission.pr_handoff_ref.to_string(),
            result_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            output_quarantined: true,
            external_output_consumable: false,
            mdx_quality_gates_required: true,
            mdx_quality_gates_passed: false,
            accepted_for_scoreboard: false,
            blocked_reason: "pending_mdx_quality_gates",
        })
    }

    fn finish_forge_fleet_eval_result_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "FLEET_EVAL_RESULT_QUARANTINED_PENDING_MDX_GATES".to_string();
        }
    }

    fn finish_forge_fleet_eval_dry_run(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "FLEET_EVAL_DRY_RUN_READY_FOR_KEYS".to_string();
        }
    }

    fn finish_forge_fleet_eval_live_run_approval(&mut self, run_id: &str) {
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "FLEET_EVAL_LIVE_RUN_APPROVAL_RECORDED".to_string();
        }
    }
}

fn normalize_provider_allowlist(
    provider_allowlist: &str,
) -> Result<String, ForgeFleetEvalResultIngestionError> {
    let known = forge_fleet_model_matrix_profiles()
        .into_iter()
        .map(|profile| profile.provider_family)
        .collect::<Vec<_>>();
    let mut normalized = Vec::new();
    for provider in provider_allowlist.split(',').map(str::trim) {
        if provider.is_empty() {
            continue;
        }
        if !known.contains(&provider) {
            return Err(ForgeFleetEvalResultIngestionError::UnknownProviderFamily(
                provider.to_string(),
            ));
        }
        if !normalized.contains(&provider) {
            normalized.push(provider);
        }
    }
    if normalized.is_empty() {
        return Err(ForgeFleetEvalResultIngestionError::Missing(
            "provider_allowlist",
        ));
    }
    Ok(normalized.join(","))
}

fn validate_result_artifact_identity(
    submission: &ForgeFleetEvalResultSubmission<'_>,
) -> Result<(), ForgeFleetEvalResultIngestionError> {
    validate_quarantine_ref("output_artifact_ref", submission.output_artifact_ref)?;
    validate_quarantine_ref("transcript_ref", submission.transcript_ref)?;
    validate_quarantine_ref("pr_handoff_ref", submission.pr_handoff_ref)?;
    validate_sha256_digest("output_artifact_sha256", submission.output_artifact_sha256)?;
    Ok(())
}

fn validate_standards_source_fingerprints(
    submission: &ForgeFleetEvalResultSubmission<'_>,
) -> Result<(), ForgeFleetEvalResultIngestionError> {
    let value = submission.standards_source_fingerprints.trim();
    if value == "dry_run_standards_fingerprints_pending_repo_profile"
        && submission.submission_id.starts_with("dry_run_")
    {
        return Ok(());
    }
    for entry in value.split(',').map(str::trim) {
        let Some((source, fingerprint)) = entry.split_once('=') else {
            return Err(
                ForgeFleetEvalResultIngestionError::InvalidStandardsSourceFingerprint(format!(
                    "forge fleet eval result expected standards_source_fingerprints entries shaped as path=fnv1a64:<16 hex> but got {entry}"
                )),
            );
        };
        if !is_allowed_standards_source(source) || !is_fnv1a64_fingerprint(fingerprint) {
            return Err(
                ForgeFleetEvalResultIngestionError::InvalidStandardsSourceFingerprint(format!(
                    "forge fleet eval result expected allowlisted standards_source_fingerprints but got {entry}"
                )),
            );
        }
    }
    Ok(())
}

fn validate_ingested_principal_engineer_verdict(
    submission: &ForgeFleetEvalResultSubmission<'_>,
) -> Result<(), ForgeFleetEvalResultIngestionError> {
    if submission.principal_engineer_verdict == "pending_principal_engineer_review" {
        Ok(())
    } else {
        Err(
            ForgeFleetEvalResultIngestionError::InvalidPrincipalEngineerVerdict(format!(
                "forge fleet eval result ingestion only records pending_principal_engineer_review; got {}",
                submission.principal_engineer_verdict
            )),
        )
    }
}

fn is_allowed_standards_source(source: &str) -> bool {
    matches!(
        source,
        "AGENTS.md"
            | "CLAUDE.md"
            | "GEMINI.md"
            | "GROK.md"
            | "CONTRIBUTING.md"
            | "CODEOWNERS"
            | ".github/CODEOWNERS"
            | "SECURITY.md"
            | ".editorconfig"
            | "README.md"
            | ".github/workflows/**"
    )
}

fn is_fnv1a64_fingerprint(value: &str) -> bool {
    let digest = value.strip_prefix("fnv1a64:").unwrap_or_default();
    digest.len() == 16 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn validate_quarantine_ref(
    field: &'static str,
    value: &str,
) -> Result<(), ForgeFleetEvalResultIngestionError> {
    if value.starts_with("quarantine://forge/fleet-eval/")
        && !value.chars().any(char::is_whitespace)
    {
        Ok(())
    } else {
        Err(ForgeFleetEvalResultIngestionError::InvalidArtifactIdentity(
            field,
        ))
    }
}

fn validate_sha256_digest(
    field: &'static str,
    value: &str,
) -> Result<(), ForgeFleetEvalResultIngestionError> {
    let digest = value.strip_prefix("sha256:").unwrap_or_default();
    if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(ForgeFleetEvalResultIngestionError::InvalidArtifactIdentity(
            field,
        ))
    }
}

fn validate_language_task_submission(
    expected: &ForgeLanguageTaskCorpusEntry,
    submission: &ForgeFleetEvalResultSubmission<'_>,
) -> Result<(), ForgeFleetEvalResultIngestionError> {
    for (field, actual, expected_value) in [
        (
            "language_pack_id",
            submission.language_pack_id,
            expected.language_pack_id,
        ),
        ("repo_family", submission.repo_family, expected.repo_family),
        (
            "claimed_check",
            submission.claimed_check,
            expected.visible_check,
        ),
        (
            "hidden_check_slot",
            submission.hidden_check_slot,
            expected.hidden_check_slot,
        ),
        (
            "artifact_noise_expected",
            submission.artifact_noise_expected,
            expected.artifact_noise_expected,
        ),
    ] {
        if actual != expected_value {
            return Err(ForgeFleetEvalResultIngestionError::LanguageTaskMismatch(
                format!(
                    "forge fleet eval language task {} expected {field}={} but got {}",
                    expected.task_corpus_id, expected_value, actual
                ),
            ));
        }
    }
    let impacted_packs = submission
        .diff_language_pack_impact
        .split(',')
        .map(str::trim)
        .filter(|pack| !pack.is_empty())
        .collect::<Vec<_>>();
    if !impacted_packs.contains(&expected.language_pack_id) {
        return Err(ForgeFleetEvalResultIngestionError::LanguageTaskMismatch(
            format!(
                "forge fleet eval language task {} expected diff_language_pack_impact to include {} but got {}",
                expected.task_corpus_id,
                expected.language_pack_id,
                submission.diff_language_pack_impact
            ),
        ));
    }
    validate_principal_review_gate_results(
        expected.task_corpus_id,
        expected.required_principal_review_gates,
        submission.principal_review_gate_results,
    )?;
    Ok(())
}

fn validate_principal_review_gate_results(
    task_corpus_id: &str,
    required_gates: &str,
    gate_results: &str,
) -> Result<(), ForgeFleetEvalResultIngestionError> {
    let reported = gate_results
        .split([';', ','])
        .filter_map(|entry| entry.split_once('='))
        .map(|(gate, value)| (gate.trim(), value.trim()))
        .filter(|(gate, value)| !gate.is_empty() && !value.is_empty())
        .collect::<Vec<_>>();
    for required in required_gates.split(',').map(str::trim) {
        if required.is_empty() {
            continue;
        }
        let required_prefix = required.split(':').next().unwrap_or(required);
        let covered = reported
            .iter()
            .any(|(gate, _)| *gate == required || *gate == required_prefix);
        if !covered {
            return Err(ForgeFleetEvalResultIngestionError::LanguageTaskMismatch(
                format!(
                    "forge fleet eval language task {task_corpus_id} expected principal_review_gate_results to include {required}"
                ),
            ));
        }
    }
    Ok(())
}

// A runner's standing accrues ONLY from receipt evidence: trials it ran,
// results that survived quarantine, quality gates, and principal review.
// The declared roster starts at zero. The scoreboard route folds accepted
// receipt counts on top of this zero baseline; nothing here fabricates a
// result the ledger cannot show.
fn score_runner(runner: &FleetRunnerProfile) -> FleetRunnerScore {
    FleetRunnerScore {
        runner_id: runner.runner_id.to_string(),
        runner_kind: runner.runner_kind.to_string(),
        model_provider: runner.model_provider.to_string(),
        model: runner.model.to_string(),
        wire_api: runner.wire_api.to_string(),
        tasks_attempted: 0,
        accepted: 0,
        quarantined: 0,
        blocked: 0,
        failed_quality_gates: 0,
        tokens_in: 0,
        tokens_out: 0,
        cost_cents: 0,
        mean_runtime_ms: 0,
        authority_violations_blocked: 0,
        pass_rate_pct: 0,
    }
}

pub fn fleet_eval_scale_policy() -> ScaleBackpressurePolicy {
    ScaleBackpressurePolicy {
        max_concurrent_workers: 32,
        max_queue_depth: 96,
        per_tenant_max_concurrent: 6,
        high_priority_reserved_slots: 8,
        low_priority_shed_queue_depth: 72,
    }
}

pub fn fleet_eval_priority_for_class(class: &str) -> WorkerPriority {
    match class {
        "security" | "ci_repair" | "architecture" | "migration" | "concurrency" => {
            WorkerPriority::High
        }
        "bug_fix" | "feature" | "multi_file" | "api_compat" | "performance" | "observability"
        | "long_horizon" => WorkerPriority::Normal,
        _ => WorkerPriority::Low,
    }
}
