use super::*;
use crate::harness_fleet_eval::{
    language_task_contamination_policy, language_task_engineering_facets,
    language_task_evaluation_oracle, language_task_human_timebox_minutes,
};

const FULL_PRINCIPAL_REVIEW_GATE_RESULTS: &str = "behavior=present;tests=present;stack_idioms=present;compatibility=present;security=present;maintainability=present";
const VALID_OUTPUT_SHA256_A: &str =
    "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const VALID_OUTPUT_SHA256_B: &str =
    "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const VALID_OUTPUT_SHA256_C: &str =
    "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

#[test]
fn forge_fleet_eval_report_starts_every_runner_at_zero_evidence() {
    let report = LocalForgeFleetEvalHarness.run_scoreboard();

    assert_eq!(report.status, "RECEIPT-BACKED-FLEET-EVAL-SCOREBOARD");
    assert_eq!(report.benchmark_tasks.len(), 15);
    assert!(report.data_decides);
    assert_eq!(report.parallel_agents_target, 128);
    assert_eq!(
        report.codex_model_freedom_rule,
        "models_must_be_behind_mdx_approved_responses_compatible_provider_profile"
    );
    assert_eq!(report.model_matrix.len(), 14);
    assert!(forge_fleet_runner_profiles().iter().any(|runner| {
        runner.runner_id == "kimi_code_external_worker"
            && runner.model_profile_id == "kimi_code_k3_model_profile"
            && runner.wire_api == "kimi_code_cli"
    }));
    assert!(report.model_matrix.iter().any(|profile| {
        profile.profile_id == "kimi_code_k3_model_profile"
            && profile.provider_family == "moonshot"
            && profile.example_model == "kimi-k3"
    }));
    assert_eq!(report.scoring_dimensions.len(), 11);
    assert_eq!(
        report
            .scoring_dimensions
            .iter()
            .map(|dimension| dimension.weight_pct)
            .sum::<u32>(),
        100
    );

    // No runner starts with standing: the pure report carries the declared
    // roster at zero and no winner. Scores accrue only from receipt
    // evidence folded in by the scoreboard route.
    assert_eq!(report.winning_runner_id, "");
    for score in &report.runner_scores {
        assert_eq!(score.tasks_attempted, 0, "{}", score.runner_id);
        assert_eq!(score.accepted, 0, "{}", score.runner_id);
        assert_eq!(score.pass_rate_pct, 0, "{}", score.runner_id);
        assert_eq!(score.tokens_in, 0, "{}", score.runner_id);
        assert_eq!(score.mean_runtime_ms, 0, "{}", score.runner_id);
    }

    let codex = report
        .runner_scores
        .iter()
        .find(|score| score.runner_id == "codex_cli_external_worker")
        .expect("codex runner score");
    assert_eq!(codex.runner_kind, "external_machine");
    assert_eq!(codex.model_provider, "mdx_responses_proxy");
    assert_eq!(codex.wire_api, "responses");
}

#[test]
fn forge_fleet_eval_corpus_covers_senior_engineering_facets() {
    let tasks = forge_fleet_benchmark_tasks();
    for class in [
        "bug_fix",
        "feature",
        "refactor",
        "security",
        "ci_repair",
        "docs_code",
        "multi_file",
        "architecture",
        "api_compat",
        "migration",
        "performance",
        "concurrency",
        "observability",
        "product_ux",
        "long_horizon",
    ] {
        assert!(
            tasks.iter().any(|task| task.class == class),
            "missing task class {class}"
        );
    }
    assert!(
        tasks
            .iter()
            .any(|task| task.complexity_tier == "xl" && task.human_timebox_minutes >= 180)
    );
    assert!(
        tasks
            .iter()
            .all(|task| task.contamination_policy.contains("local")
                || task.contamination_policy.contains("fresh"))
    );
}

#[test]
fn forge_fleet_eval_scoring_dimensions_match_industry_quality_bar() {
    let dimensions = forge_fleet_scoring_dimensions();
    for dimension in [
        "correctness",
        "regression_test_quality",
        "patch_quality",
        "architecture_fit",
        "security_and_policy",
        "maintainability",
        "observability",
        "performance",
        "migration_and_compatibility",
        "cost_latency_budget",
        "handoff_quality",
    ] {
        assert!(
            dimensions
                .iter()
                .any(|candidate| candidate.dimension_id == dimension),
            "missing scoring dimension {dimension}"
        );
    }
    assert_eq!(
        dimensions
            .iter()
            .map(|dimension| dimension.weight_pct)
            .sum::<u32>(),
        100
    );
    assert!(
        dimensions
            .iter()
            .filter(|dimension| dimension.fail_closed_if_missing)
            .count()
            >= 5
    );
}

#[test]
fn forge_fleet_eval_model_matrix_covers_required_provider_families() {
    let profiles = forge_fleet_model_matrix_profiles();
    for family in ["anthropic", "xai", "aws_bedrock"] {
        let profile = profiles
            .iter()
            .find(|profile| profile.provider_family == family && profile.wire_api == "responses")
            .expect("required responses-compatible model family profile");
        assert_eq!(profile.wire_api, "responses");
        assert!(!profile.live_call_allowed);
        assert_eq!(profile.redaction_policy, "mdx_context_redaction_required");
        assert_eq!(profile.budget_policy, "mdx_cost_and_token_budget_required");
    }
    assert!(profiles.iter().any(|profile| {
        profile.profile_id == "codex_gemini_responses_profile"
            && profile.provider_family == "gemini"
            && profile.example_model == "gemini-3.1-pro-preview-via-mdx"
            && profile.model_display_name == "Gemini 3.1 Pro Preview"
            && profile.wire_api == "responses"
    }));
    assert!(profiles.iter().any(|profile| {
        profile.profile_id == "codex_anthropic_responses_profile"
            && profile.example_model == "claude-opus-4-8-via-mdx"
            && profile.model_display_name == "Claude Opus 4.8"
    }));
    assert!(profiles.iter().any(|profile| {
        profile.profile_id == "codex_xai_responses_profile"
            && profile.example_model == "grok-4.5-via-mdx"
            && profile.model_display_name == "Grok 4.5"
    }));
    assert!(profiles.iter().any(|profile| {
        profile.profile_id == "gemini_cli_model_profile"
            && profile.provider_family == "gemini"
            && profile.wire_api == "gemini_cli"
            && profile.budget_policy == "mdx_cli_runtime_budget_required"
            && !profile.live_call_allowed
    }));
    for (profile_id, family, wire_api) in [
        ("opencode_model_profile", "opencode", "opencode_cli"),
        ("cline_model_profile", "cline", "cline_cli"),
        ("goose_model_profile", "goose", "goose_cli"),
        ("xai_grok_build_model_profile", "xai", "acp_stdio"),
        ("claude_code_model_profile", "anthropic", "claude_agent_sdk"),
    ] {
        let profile = profiles
            .iter()
            .find(|profile| profile.profile_id == profile_id)
            .expect("machine CLI model profile");
        assert_eq!(profile.provider_family, family);
        assert_eq!(profile.wire_api, wire_api);
        assert_eq!(profile.budget_policy, "mdx_cli_runtime_budget_required");
        assert!(!profile.live_call_allowed);
    }
    assert!(profiles.iter().any(
        |profile| profile.profile_id == "codex_bedrock_responses_profile"
            && profile.model_provider == "bedrock_responses_proxy"
    ));
}

#[test]
fn forge_fleet_eval_ingests_external_result_as_quarantined_evidence() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .ingest_forge_fleet_eval_result_local(ForgeFleetEvalResultSubmission {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            actor_role: "worker",
            submission_id: "fleet_eval_result_codex_001",
            benchmark_task_id: "bugfix_small",
            language_task_corpus_id: "rust-cargo-bug-fix-small",
            language_pack_id: "rust-cargo",
            repo_family: "Rust Cargo",
            runner_id: "codex_cli_external_worker",
            model_profile_id: "codex_anthropic_responses_profile",
            output_artifact_ref: "quarantine://forge/fleet-eval/codex-001.patch",
            output_artifact_sha256: VALID_OUTPUT_SHA256_A,
            transcript_ref: "quarantine://forge/fleet-eval/codex-001.jsonl",
            claimed_check: "cargo test",
            hidden_check_slot: "unit regression",
            artifact_noise_expected: "target/**",
            principal_review_gate_results: FULL_PRINCIPAL_REVIEW_GATE_RESULTS,
            standards_source_fingerprints: "AGENTS.md=fnv1a64:0123456789abcdef",
            artifact_filter_summary: "target/** folded from review",
            diff_language_pack_impact: "rust-cargo",
            principal_engineer_verdict: "pending_principal_engineer_review",
            pr_handoff_ref: "quarantine://forge/fleet-eval/codex-001-pr.md",
        })
        .expect("fleet eval result ingestion");

    assert_eq!(
        report.status,
        "FLEET_EVAL_RESULT_QUARANTINED_PENDING_MDX_GATES"
    );
    assert!(report.output_quarantined);
    assert!(!report.external_output_consumable);
    assert!(report.mdx_quality_gates_required);
    assert!(!report.mdx_quality_gates_passed);
    assert!(!report.accepted_for_scoreboard);
    assert_eq!(report.blocked_reason, "pending_mdx_quality_gates");

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.result_receipt_id)
        .expect("ingestion receipt");
    assert_eq!(receipt.kind, "forge.fleet_eval_result.ingested");
    assert_eq!(receipt.payload["runner_id"], "codex_cli_external_worker");
    assert_eq!(
        receipt.payload["language_task_corpus_id"],
        "rust-cargo-bug-fix-small"
    );
    assert_eq!(receipt.payload["language_pack_id"], "rust-cargo");
    assert_eq!(receipt.payload["repo_family"], "Rust Cargo");
    assert_eq!(receipt.payload["hidden_check_slot"], "unit regression");
    assert_eq!(receipt.payload["artifact_noise_expected"], "target/**");
    assert_eq!(
        receipt.payload["principal_review_gate_results"],
        FULL_PRINCIPAL_REVIEW_GATE_RESULTS
    );
    assert_eq!(
        receipt.payload["standards_source_fingerprints"],
        "AGENTS.md=fnv1a64:0123456789abcdef"
    );
    assert_eq!(
        receipt.payload["artifact_filter_summary"],
        "target/** folded from review"
    );
    assert_eq!(receipt.payload["diff_language_pack_impact"], "rust-cargo");
    assert_eq!(
        receipt.payload["principal_engineer_verdict"],
        "pending_principal_engineer_review"
    );
    assert_eq!(
        report.language_engineering_facets,
        "behavioral correctness, regression coverage, minimal diff"
    );
    assert_eq!(
        report.language_evaluation_oracle,
        "visible native check plus hidden behavioral regression"
    );
    assert_eq!(report.language_human_timebox_minutes, 30);
    assert!(
        report
            .language_contamination_policy
            .contains("held-out mutation")
    );
    assert_eq!(
        receipt.payload["language_engineering_facets"],
        "behavioral correctness, regression coverage, minimal diff"
    );
    assert_eq!(
        receipt.payload["language_evaluation_oracle"],
        "visible native check plus hidden behavioral regression"
    );
    assert_eq!(receipt.payload["language_human_timebox_minutes"], "30");
    assert!(receipt.payload["language_contamination_policy"].contains("held-out mutation"));
    assert_eq!(
        receipt.payload["pr_handoff_ref"],
        "quarantine://forge/fleet-eval/codex-001-pr.md"
    );
    assert_eq!(
        receipt.payload["model_profile_id"],
        "codex_anthropic_responses_profile"
    );
    assert_eq!(receipt.payload["output_quarantined"], "true");
    assert_eq!(receipt.payload["external_output_consumable"], "false");
}

#[test]
fn forge_fleet_eval_accepts_cross_agent_instruction_standards_source() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .ingest_forge_fleet_eval_result_local(ForgeFleetEvalResultSubmission {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            actor_role: "worker",
            submission_id: "fleet_eval_result_codex_claude_standard",
            benchmark_task_id: "bugfix_small",
            language_task_corpus_id: "rust-cargo-bug-fix-small",
            language_pack_id: "rust-cargo",
            repo_family: "Rust Cargo",
            runner_id: "codex_cli_external_worker",
            model_profile_id: "codex_anthropic_responses_profile",
            output_artifact_ref: "quarantine://forge/fleet-eval/codex-claude-standard.patch",
            output_artifact_sha256: VALID_OUTPUT_SHA256_A,
            transcript_ref: "quarantine://forge/fleet-eval/codex-claude-standard.jsonl",
            claimed_check: "cargo test",
            hidden_check_slot: "unit regression",
            artifact_noise_expected: "target/**",
            principal_review_gate_results: FULL_PRINCIPAL_REVIEW_GATE_RESULTS,
            standards_source_fingerprints: "CLAUDE.md=fnv1a64:0123456789abcdef",
            artifact_filter_summary: "target/** folded from review",
            diff_language_pack_impact: "rust-cargo",
            principal_engineer_verdict: "pending_principal_engineer_review",
            pr_handoff_ref: "quarantine://forge/fleet-eval/codex-claude-standard-pr.md",
        })
        .expect("cross-agent standards source accepted");

    assert_eq!(
        report.status,
        "FLEET_EVAL_RESULT_QUARANTINED_PENDING_MDX_GATES"
    );
    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.result_receipt_id)
        .expect("ingestion receipt");
    assert_eq!(
        receipt.payload["standards_source_fingerprints"],
        "CLAUDE.md=fnv1a64:0123456789abcdef"
    );
}

#[test]
fn forge_fleet_eval_refuses_unknown_model_profile() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .ingest_forge_fleet_eval_result_local(ForgeFleetEvalResultSubmission {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            actor_role: "worker",
            submission_id: "fleet_eval_result_codex_002",
            benchmark_task_id: "bugfix_small",
            language_task_corpus_id: "rust-cargo-bug-fix-small",
            language_pack_id: "rust-cargo",
            repo_family: "Rust Cargo",
            runner_id: "codex_cli_external_worker",
            model_profile_id: "raw_vendor_profile",
            output_artifact_ref: "quarantine://forge/fleet-eval/codex-002.patch",
            output_artifact_sha256: VALID_OUTPUT_SHA256_B,
            transcript_ref: "quarantine://forge/fleet-eval/codex-002.jsonl",
            claimed_check: "cargo test",
            hidden_check_slot: "unit regression",
            artifact_noise_expected: "target/**",
            principal_review_gate_results: FULL_PRINCIPAL_REVIEW_GATE_RESULTS,
            standards_source_fingerprints: "AGENTS.md=fnv1a64:0123456789abcdef",
            artifact_filter_summary: "target/** folded from review",
            diff_language_pack_impact: "rust-cargo",
            principal_engineer_verdict: "pending_principal_engineer_review",
            pr_handoff_ref: "quarantine://forge/fleet-eval/codex-002-pr.md",
        })
        .expect_err("unknown model profile refused");

    assert!(matches!(
        error,
        ForgeFleetEvalResultIngestionError::UnknownModelProfile(_)
    ));
    assert!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.fleet_eval_result.ingested")
            .is_empty()
    );
}

#[test]
fn forge_fleet_eval_refuses_language_task_mismatch() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .ingest_forge_fleet_eval_result_local(ForgeFleetEvalResultSubmission {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            actor_role: "worker",
            submission_id: "fleet_eval_result_bad_language",
            benchmark_task_id: "bugfix_small",
            language_task_corpus_id: "swift-spm-bug-fix-small",
            language_pack_id: "swift-spm",
            repo_family: "Swift Package Manager",
            runner_id: "codex_cli_external_worker",
            model_profile_id: "codex_anthropic_responses_profile",
            output_artifact_ref: "quarantine://forge/fleet-eval/codex-language.patch",
            output_artifact_sha256: VALID_OUTPUT_SHA256_C,
            transcript_ref: "quarantine://forge/fleet-eval/codex-language.jsonl",
            claimed_check: "cargo test",
            hidden_check_slot: "edge case unit test",
            artifact_noise_expected: ".build/**",
            principal_review_gate_results: FULL_PRINCIPAL_REVIEW_GATE_RESULTS,
            standards_source_fingerprints: "AGENTS.md=fnv1a64:0123456789abcdef",
            artifact_filter_summary: ".build/** folded from review",
            diff_language_pack_impact: "swift-spm",
            principal_engineer_verdict: "pending_principal_engineer_review",
            pr_handoff_ref: "quarantine://forge/fleet-eval/codex-language-pr.md",
        })
        .expect_err("language task mismatch refused");

    assert!(matches!(
        error,
        ForgeFleetEvalResultIngestionError::LanguageTaskMismatch(_)
    ));
    assert!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.fleet_eval_result.ingested")
            .is_empty()
    );
}

#[test]
fn forge_fleet_eval_refuses_result_without_matching_diff_language_pack_impact() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .ingest_forge_fleet_eval_result_local(ForgeFleetEvalResultSubmission {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            actor_role: "worker",
            submission_id: "fleet_eval_result_bad_impact",
            benchmark_task_id: "bugfix_small",
            language_task_corpus_id: "rust-cargo-bug-fix-small",
            language_pack_id: "rust-cargo",
            repo_family: "Rust Cargo",
            runner_id: "codex_cli_external_worker",
            model_profile_id: "codex_anthropic_responses_profile",
            output_artifact_ref: "quarantine://forge/fleet-eval/codex-impact.patch",
            output_artifact_sha256: VALID_OUTPUT_SHA256_A,
            transcript_ref: "quarantine://forge/fleet-eval/codex-impact.jsonl",
            claimed_check: "cargo test",
            hidden_check_slot: "unit regression",
            artifact_noise_expected: "target/**",
            principal_review_gate_results: FULL_PRINCIPAL_REVIEW_GATE_RESULTS,
            standards_source_fingerprints: "AGENTS.md=fnv1a64:0123456789abcdef",
            artifact_filter_summary: "target/** folded from review",
            diff_language_pack_impact: "node,java-maven",
            principal_engineer_verdict: "pending_principal_engineer_review",
            pr_handoff_ref: "quarantine://forge/fleet-eval/codex-impact-pr.md",
        })
        .expect_err("diff language-pack impact mismatch refused");

    assert!(matches!(
        error,
        ForgeFleetEvalResultIngestionError::LanguageTaskMismatch(_)
    ));
    assert!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.fleet_eval_result.ingested")
            .is_empty()
    );
}

#[test]
fn forge_fleet_eval_refuses_result_without_all_principal_review_gates() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .ingest_forge_fleet_eval_result_local(ForgeFleetEvalResultSubmission {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            actor_role: "worker",
            submission_id: "fleet_eval_result_missing_gates",
            benchmark_task_id: "bugfix_small",
            language_task_corpus_id: "rust-cargo-bug-fix-small",
            language_pack_id: "rust-cargo",
            repo_family: "Rust Cargo",
            runner_id: "codex_cli_external_worker",
            model_profile_id: "codex_anthropic_responses_profile",
            output_artifact_ref: "quarantine://forge/fleet-eval/codex-gates.patch",
            output_artifact_sha256: VALID_OUTPUT_SHA256_A,
            transcript_ref: "quarantine://forge/fleet-eval/codex-gates.jsonl",
            claimed_check: "cargo test",
            hidden_check_slot: "unit regression",
            artifact_noise_expected: "target/**",
            principal_review_gate_results: "behavior=present;tests=present;stack_idioms=present",
            standards_source_fingerprints: "AGENTS.md=fnv1a64:0123456789abcdef",
            artifact_filter_summary: "target/** folded from review",
            diff_language_pack_impact: "rust-cargo",
            principal_engineer_verdict: "pending_principal_engineer_review",
            pr_handoff_ref: "quarantine://forge/fleet-eval/codex-gates-pr.md",
        })
        .expect_err("missing principal-review gates refused");

    assert!(matches!(
        error,
        ForgeFleetEvalResultIngestionError::LanguageTaskMismatch(message)
            if message.contains("principal_review_gate_results")
                && message.contains("compatibility:public_contract_or_migration_risk_reviewed")
    ));
    assert!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.fleet_eval_result.ingested")
            .is_empty()
    );
}

#[test]
fn forge_fleet_eval_refuses_malformed_artifact_identity() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .ingest_forge_fleet_eval_result_local(ForgeFleetEvalResultSubmission {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            actor_role: "worker",
            submission_id: "fleet_eval_result_bad_artifact_identity",
            benchmark_task_id: "bugfix_small",
            language_task_corpus_id: "rust-cargo-bug-fix-small",
            language_pack_id: "rust-cargo",
            repo_family: "Rust Cargo",
            runner_id: "codex_cli_external_worker",
            model_profile_id: "codex_anthropic_responses_profile",
            output_artifact_ref: "file:///tmp/codex.patch",
            output_artifact_sha256: "sha256:not-a-real-digest",
            transcript_ref: "quarantine://forge/fleet-eval/codex-artifact.jsonl",
            claimed_check: "cargo test",
            hidden_check_slot: "unit regression",
            artifact_noise_expected: "target/**",
            principal_review_gate_results: FULL_PRINCIPAL_REVIEW_GATE_RESULTS,
            standards_source_fingerprints: "AGENTS.md=fnv1a64:0123456789abcdef",
            artifact_filter_summary: "target/** folded from review",
            diff_language_pack_impact: "rust-cargo",
            principal_engineer_verdict: "pending_principal_engineer_review",
            pr_handoff_ref: "quarantine://forge/fleet-eval/codex-artifact-pr.md",
        })
        .expect_err("malformed artifact identity refused");

    assert!(matches!(
        error,
        ForgeFleetEvalResultIngestionError::InvalidArtifactIdentity("output_artifact_ref")
    ));
    assert!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.fleet_eval_result.ingested")
            .is_empty()
    );
}

#[test]
fn forge_fleet_eval_refuses_malformed_standards_source_fingerprint() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .ingest_forge_fleet_eval_result_local(ForgeFleetEvalResultSubmission {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            actor_role: "worker",
            submission_id: "fleet_eval_result_bad_standard",
            benchmark_task_id: "bugfix_small",
            language_task_corpus_id: "rust-cargo-bug-fix-small",
            language_pack_id: "rust-cargo",
            repo_family: "Rust Cargo",
            runner_id: "codex_cli_external_worker",
            model_profile_id: "codex_anthropic_responses_profile",
            output_artifact_ref: "quarantine://forge/fleet-eval/codex-standard.patch",
            output_artifact_sha256: VALID_OUTPUT_SHA256_A,
            transcript_ref: "quarantine://forge/fleet-eval/codex-standard.jsonl",
            claimed_check: "cargo test",
            hidden_check_slot: "unit regression",
            artifact_noise_expected: "target/**",
            principal_review_gate_results: FULL_PRINCIPAL_REVIEW_GATE_RESULTS,
            standards_source_fingerprints: "Cargo.toml=fnv1a64:0123456789abcdef",
            artifact_filter_summary: "target/** folded from review",
            diff_language_pack_impact: "rust-cargo",
            principal_engineer_verdict: "pending_principal_engineer_review",
            pr_handoff_ref: "quarantine://forge/fleet-eval/codex-standard-pr.md",
        })
        .expect_err("malformed standards source fingerprint refused");

    assert!(matches!(
        error,
        ForgeFleetEvalResultIngestionError::InvalidStandardsSourceFingerprint(message)
            if message.contains("standards_source_fingerprints")
                && message.contains("Cargo.toml=fnv1a64:0123456789abcdef")
    ));
    assert!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.fleet_eval_result.ingested")
            .is_empty()
    );
}

#[test]
fn forge_fleet_eval_refuses_external_principal_verdict_self_attestation() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .ingest_forge_fleet_eval_result_local(ForgeFleetEvalResultSubmission {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            actor_role: "worker",
            submission_id: "fleet_eval_result_self_attested_verdict",
            benchmark_task_id: "bugfix_small",
            language_task_corpus_id: "rust-cargo-bug-fix-small",
            language_pack_id: "rust-cargo",
            repo_family: "Rust Cargo",
            runner_id: "codex_cli_external_worker",
            model_profile_id: "codex_anthropic_responses_profile",
            output_artifact_ref: "quarantine://forge/fleet-eval/codex-verdict.patch",
            output_artifact_sha256: VALID_OUTPUT_SHA256_A,
            transcript_ref: "quarantine://forge/fleet-eval/codex-verdict.jsonl",
            claimed_check: "cargo test",
            hidden_check_slot: "unit regression",
            artifact_noise_expected: "target/**",
            principal_review_gate_results: FULL_PRINCIPAL_REVIEW_GATE_RESULTS,
            standards_source_fingerprints: "AGENTS.md=fnv1a64:0123456789abcdef",
            artifact_filter_summary: "target/** folded from review",
            diff_language_pack_impact: "rust-cargo",
            principal_engineer_verdict: "approved_by_external_worker",
            pr_handoff_ref: "quarantine://forge/fleet-eval/codex-verdict-pr.md",
        })
        .expect_err("external principal verdict self-attestation refused");

    assert!(matches!(
        error,
        ForgeFleetEvalResultIngestionError::InvalidPrincipalEngineerVerdict(message)
            if message.contains("pending_principal_engineer_review")
                && message.contains("approved_by_external_worker")
    ));
    assert!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.fleet_eval_result.ingested")
            .is_empty()
    );
}

#[test]
fn forge_fleet_eval_dry_run_records_full_model_task_matrix() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .run_forge_fleet_eval_dry_run_local("dry_run_ready_for_keys")
        .expect("fleet eval dry run");

    assert_eq!(report.status, "FLEET_EVAL_DRY_RUN_READY_FOR_KEYS");
    assert_eq!(report.benchmark_task_count, 15);
    assert_eq!(report.language_task_count, 30);
    assert_eq!(report.model_profile_count, 14);
    assert_eq!(report.scoring_dimension_count, 11);
    assert_eq!(report.dry_run_case_count, 420);
    assert_eq!(report.runner_execution_receipt_ids.len(), 420);
    assert_eq!(report.result_receipt_ids.len(), 420);
    assert_eq!(report.score_receipt_ids.len(), 420);
    assert!(!report.live_provider_calls_allowed);
    assert!(!report.live_provider_calls_performed);
    assert!(report.provider_credentials_required_for_live);
    assert!(report.ready_for_live_credentials);
    assert_eq!(report.accepted_for_scoreboard_count, 0);
    assert_eq!(report.quarantined_count, 420);
    assert_eq!(
        report.blocked_reason,
        "dry_run_waiting_for_live_provider_keys"
    );

    assert_eq!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.fleet_eval_runner.execution.recorded")
            .len(),
        420
    );
    assert_eq!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.fleet_eval_result.ingested")
            .len(),
        420
    );
    assert_eq!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.fleet_eval_result.scored")
            .len(),
        420
    );

    for task in forge_language_task_corpus() {
        assert!(
            kernel
                .ledger()
                .query()
                .by_kind("forge.fleet_eval_result.scored")
                .iter()
                .any(|receipt| receipt
                    .payload
                    .get("language_task_corpus_id")
                    .map(String::as_str)
                    == Some(task.task_corpus_id)),
            "dry run did not score language task {}",
            task.task_corpus_id
        );
    }
}

#[test]
fn forge_fleet_eval_dry_run_scores_fail_closed_until_live_keys() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .run_forge_fleet_eval_dry_run_local("dry_run_scoring")
        .expect("fleet eval dry run");
    let score_receipt = kernel
        .ledger()
        .query()
        .by_id(&report.score_receipt_ids[0])
        .expect("score receipt");

    assert_eq!(score_receipt.kind, "forge.fleet_eval_result.scored");
    assert_eq!(
        score_receipt.payload["blocked_reason"],
        "dry_run_waiting_for_live_provider_keys"
    );
    assert_eq!(
        score_receipt.payload["live_provider_output_present"],
        "false"
    );
    assert_eq!(score_receipt.payload["mdx_quality_gates_passed"], "false");
    assert_eq!(score_receipt.payload["accepted_for_scoreboard"], "false");
    assert_eq!(score_receipt.payload["external_output_consumable"], "false");
    assert_eq!(score_receipt.payload["security_and_policy_score"], "100");
    assert_eq!(score_receipt.payload["total_score"], "24");
    assert_eq!(
        score_receipt.payload["claimed_check_matches_expected"],
        "true"
    );
    assert!(!score_receipt.payload["language_task_corpus_id"].is_empty());
    assert!(!score_receipt.payload["language_pack_id"].is_empty());
    assert!(!score_receipt.payload["repo_family"].is_empty());
    assert!(!score_receipt.payload["language_engineering_facets"].is_empty());
    assert!(!score_receipt.payload["language_evaluation_oracle"].is_empty());
    assert!(!score_receipt.payload["language_human_timebox_minutes"].is_empty());
    assert!(score_receipt.payload["language_contamination_policy"].contains("held-out mutation"));
    assert_eq!(
        score_receipt.payload["principal_engineer_verdict"],
        "pending_principal_engineer_review"
    );
}

#[test]
fn forge_fleet_eval_live_run_approval_records_no_execution_authority() {
    let mut kernel = MdxKernel::boot_local();
    let report = kernel
        .approve_forge_fleet_eval_live_run_local(ForgeFleetEvalLiveRunApproval {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            actor_role: "operator",
            approval_id: "fleet_eval_approval_001",
            provider_allowlist: "gemini,anthropic,xai,aws_bedrock",
            max_spend_cents: 500,
            max_tasks: 15,
            max_parallel_agents: 4,
            artifact_retention_policy: "quarantine_7_days",
            redaction_policy: "mdx_context_redaction_required",
            stop_conditions: "budget_exhausted,provider_error_rate,quality_gate_failure,manual_stop",
        })
        .expect("live-run approval");

    assert_eq!(report.status, "FLEET_EVAL_LIVE_RUN_APPROVAL_RECORDED");
    assert_eq!(
        report.blocked_reason,
        "approval_recorded_waiting_for_credentials_and_live_adapter"
    );
    assert!(report.provider_credentials_required);
    assert!(!report.live_provider_calls_allowed);
    assert!(!report.approval_grants_execution_authority);
    assert!(!report.adapter_execution_allowed);
    assert!(!report.production_write_allowed);

    let receipt = kernel
        .ledger()
        .query()
        .by_id(&report.approval_receipt_id)
        .expect("approval receipt");
    assert_eq!(receipt.kind, "forge.fleet_eval_live_run.approved");
    assert_eq!(
        receipt.payload["provider_allowlist"],
        "gemini,anthropic,xai,aws_bedrock"
    );
    assert_eq!(receipt.payload["provider_secret_values_recorded"], "false");
    assert_eq!(receipt.payload["provider_secret_export_allowed"], "false");
    assert_eq!(receipt.payload["live_provider_calls_allowed"], "false");
    assert_eq!(
        receipt.payload["approval_grants_execution_authority"],
        "false"
    );
}

#[test]
fn forge_fleet_eval_live_run_approval_refuses_unknown_provider() {
    let mut kernel = MdxKernel::boot_local();
    let error = kernel
        .approve_forge_fleet_eval_live_run_local(ForgeFleetEvalLiveRunApproval {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            actor_role: "operator",
            approval_id: "fleet_eval_approval_bad_provider",
            provider_allowlist: "anthropic,raw_vendor",
            max_spend_cents: 500,
            max_tasks: 15,
            max_parallel_agents: 4,
            artifact_retention_policy: "quarantine_7_days",
            redaction_policy: "mdx_context_redaction_required",
            stop_conditions: "budget_exhausted,manual_stop",
        })
        .expect_err("unknown provider refused");

    assert!(matches!(
        error,
        ForgeFleetEvalResultIngestionError::UnknownProviderFamily(_)
    ));
    assert!(
        kernel
            .ledger()
            .query()
            .by_kind("forge.fleet_eval_live_run.approved")
            .is_empty()
    );
}

#[test]
fn forge_fleet_eval_load_proof_supports_hundreds_without_over_admission() {
    let report = run_forge_fleet_eval();
    let load = report.parallel_load;

    assert_eq!(load.total_jobs, 512);
    assert!(load.invariants_held);
    assert!(!load.over_admission);
    assert!(!load.fairness_violation);
    assert!(!load.queue_overflow);
    assert!(load.shed_observed);
    assert!(load.cancellations_honored);
    assert!(load.retries_honored);
    assert!(load.peak_running <= 32);
    assert!(load.peak_queued <= 96);
}

#[test]
fn forge_fleet_eval_language_scorecards_cover_task_corpus() {
    let report = run_forge_fleet_eval();

    assert_eq!(report.language_task_corpus.len(), 30);
    assert_eq!(report.language_pack_scorecards.len(), 10);

    for scorecard in &report.language_pack_scorecards {
        let matching_tasks = report
            .language_task_corpus
            .iter()
            .filter(|task| task.language_pack_id == scorecard.language_pack_id)
            .collect::<Vec<_>>();

        assert_eq!(scorecard.task_count as usize, matching_tasks.len());
        assert!(scorecard.small_task_count > 0);
        assert!(scorecard.medium_task_count > 0);
        assert!(scorecard.large_task_count > 0);
        assert!(scorecard.visible_check_count > 0);
        assert!(scorecard.hidden_check_slot_count > 0);
        assert!(scorecard.artifact_noise_expectation_count > 0);
        assert!(scorecard.principal_verdict_required);
        assert!(scorecard.ready_for_live_eval);
        assert!(
            matching_tasks
                .iter()
                .all(|task| task.repo_family == scorecard.repo_family)
        );
        assert!(matching_tasks.iter().all(|task| {
            !language_task_engineering_facets(task).trim().is_empty()
                && !language_task_evaluation_oracle(task).trim().is_empty()
                && language_task_human_timebox_minutes(task) >= 30
                && language_task_contamination_policy(task).contains("held-out")
        }));
        assert!(
            matching_tasks
                .iter()
                .filter(|task| task.complexity_tier == "large")
                .all(|task| language_task_human_timebox_minutes(task) >= 180)
        );
    }
}

#[test]
fn forge_fleet_eval_prioritizes_security_and_ci_repairs() {
    assert_eq!(
        fleet_eval_priority_for_class("security"),
        WorkerPriority::High
    );
    assert_eq!(
        fleet_eval_priority_for_class("ci_repair"),
        WorkerPriority::High
    );
    assert_eq!(
        fleet_eval_priority_for_class("bug_fix"),
        WorkerPriority::Normal
    );
    assert_eq!(
        fleet_eval_priority_for_class("docs_code"),
        WorkerPriority::Low
    );
}
