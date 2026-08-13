use crate::RouteResponse;
use crate::forge_exec_sandbox::{apply_scrubbed_environment, networked_workspace_command};
#[path = "forge_execution_backend.rs"]
mod forge_execution_backend;
#[path = "machine_runtime_observation.rs"]
mod machine_runtime_observation;
use crate::secret_store::{SecretStore, global};
use forge_execution_backend::{
    ExecutionWorkspaceKind, ExecutionWorkspaceRequest, ForgeExecutionBackend,
    ForgeExecutionBackendKind, HostedSandboxUnavailable,
};
use machine_runtime_observation::{
    MachineRuntimeStaticObservation, cached_machine_runtime_static_observation, sha256_file_hex,
};
use mdx_core::{
    FleetBenchmarkTask, FleetModelMatrixProfile, FleetRunnerProfile, ForgeFleetEvalLiveRunApproval,
    ForgeFleetEvalReport, ForgeFleetEvalResultSubmission, ForgeLanguageTaskCorpusEntry,
    ForgeRunEvent, ForgeRunEventReport, GovernedWriteIdentity, MdxKernel,
    forge_fleet_benchmark_tasks, forge_fleet_model_matrix_profiles, forge_fleet_runner_profiles,
    forge_language_task_corpus, hex, json_string_literal, language_task_contamination_policy,
    language_task_engineering_facets, language_task_evaluation_oracle,
    language_task_human_timebox_minutes, run_forge_fleet_eval, sha256,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

const DEFAULT_PRINCIPAL_REVIEW_GATE_IDS: &str = "behavior:changed_behavior_is_explicit,tests:checks_cover_changed_behavior,stack_idioms:<language-pack-specific>,compatibility:public_contract_or_migration_risk_reviewed,security:no_secret_or_authority_expansion,maintainability:dependency_config_and_generated_churn_justified";
const CODEX_EXEC_POLICY_COMMAND: &str = "codex --sandbox workspace-write --ask-for-approval never --cd <worktree> exec --json --ephemeral";
const CODEX_EXEC_POLICY_MARKER: &str = "codex exec";
const GEMINI_EXEC_POLICY_COMMAND: &str =
    "gemini --skip-trust --approval-mode yolo --output-format stream-json --prompt <prompt>";
const GEMINI_EXEC_POLICY_MARKER: &str = "gemini --prompt";
const GROK_BUILD_EXEC_POLICY_COMMAND: &str = "grok --no-auto-update --sandbox strict --disable-web-search --no-subagents --deny Bash agent --model <approved-model> stdio (ACP JSON-RPC: initialize, session/new, session/prompt; tenant XAI_API_KEY child-only; isolated per-run GROK_HOME)";
const GROK_BUILD_EXEC_POLICY_MARKER: &str = "grok agent --model <approved-model> stdio";
const CLAUDE_CODE_EXEC_POLICY_COMMAND: &str =
    "Claude Agent SDK query(prompt, allowedTools, claude_code system prompt preset, isolated cwd)";
const CLAUDE_CODE_EXEC_POLICY_MARKER: &str = "claude_agent_sdk query";
const KIMI_CODE_EXEC_POLICY_COMMAND: &str = "kimi --model kimi-k3 --prompt <prompt> --output-format stream-json (tenant MOONSHOT_API_KEY remapped to ephemeral KIMI_MODEL_* env; isolated per-run KIMI_CODE_HOME)";
const KIMI_CODE_EXEC_POLICY_MARKER: &str = "kimi --model kimi-k3 --prompt";
const OPENCODE_EXEC_POLICY_COMMAND: &str = "opencode run --format json <prompt>";
const OPENCODE_EXEC_POLICY_MARKER: &str = "opencode run";
const CLINE_EXEC_POLICY_COMMAND: &str = "cline --json --auto-approve true <prompt>";
const CLINE_EXEC_POLICY_MARKER: &str = "cline --json";
const GOOSE_EXEC_POLICY_COMMAND: &str =
    "GOOSE_MODE=auto goose run --no-session -t <prompt> --output-format stream-json";
const GOOSE_EXEC_POLICY_MARKER: &str = "goose run";
const MACHINE_RUNTIME_ADAPTER_CONTRACT_VERSION: &str = "machine_league_adapter_contract_v1";
const MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID: &str = "rust_tax_rounding";
const MACHINE_LEAGUE_RUST_TAX_FIXTURE_SNAPSHOT_ID: &str = "rust_tax_rounding:v1";
const MACHINE_LEAGUE_RUST_TAX_VISIBLE_CHECK: &str = "cargo test --test visible";
const MACHINE_LEAGUE_RUST_TAX_HIDDEN_CHECK: &str = "cargo test --test hidden";
const MACHINE_LEAGUE_RUST_POLICY_FIXTURE_ID: &str = "rust_feature_policy_edge";
const MACHINE_LEAGUE_RUST_POLICY_FIXTURE_SNAPSHOT_ID: &str = "rust_feature_policy_edge:v1";
const MACHINE_LEAGUE_RUST_POLICY_VARIANT_FIXTURE_ID: &str = "rust_feature_policy_edge_variant";
const MACHINE_LEAGUE_RUST_POLICY_VARIANT_FIXTURE_SNAPSHOT_ID: &str =
    "rust_feature_policy_edge_variant:v1";
const MACHINE_LEAGUE_RUST_FIXTURE_VISIBLE_CHECK: &str = "cargo test --test visible";
const MACHINE_LEAGUE_RUST_FIXTURE_HIDDEN_CHECK: &str = "cargo test --test hidden";

#[derive(Clone, Copy)]
struct MachineLeagueEvalFixture {
    id: &'static str,
    snapshot_id: &'static str,
    family_id: &'static str,
    sibling_fixture_id: &'static str,
    repo_id: &'static str,
    package_name: &'static str,
    crate_name: &'static str,
    title: &'static str,
    allowed_scope: &'static str,
    visible_check: &'static str,
    hidden_check: &'static str,
    hidden_check_slot: &'static str,
    acceptance_criteria: &'static str,
    transferable_edge: &'static str,
    native_baseline_strategy: &'static str,
    native_improved_strategy: &'static str,
}

const MACHINE_LEAGUE_EVAL_FIXTURES: &[MachineLeagueEvalFixture] = &[
    MachineLeagueEvalFixture {
        id: MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID,
        snapshot_id: MACHINE_LEAGUE_RUST_TAX_FIXTURE_SNAPSHOT_ID,
        family_id: "rust_tax_rounding",
        sibling_fixture_id: MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID,
        repo_id: "machine-league-rust-tax-rounding-fixture",
        package_name: "machine-league-rust-tax-rounding",
        crate_name: "machine_league_rust_tax_rounding",
        title: "Rust tax rounding",
        allowed_scope: "src/lib.rs",
        visible_check: MACHINE_LEAGUE_RUST_TAX_VISIBLE_CHECK,
        hidden_check: MACHINE_LEAGUE_RUST_TAX_HIDDEN_CHECK,
        hidden_check_slot: "held-out Rust tax regression",
        acceptance_criteria: "visible and hidden Rust tests pass without weakening the fixture tests",
        transferable_edge: "minimal arithmetic fix",
        native_baseline_strategy: "tax_on_discounted_subtotal_minimal_rewrite",
        native_improved_strategy: "tax_on_discounted_subtotal_minimal_rewrite",
    },
    MachineLeagueEvalFixture {
        id: MACHINE_LEAGUE_RUST_POLICY_FIXTURE_ID,
        snapshot_id: MACHINE_LEAGUE_RUST_POLICY_FIXTURE_SNAPSHOT_ID,
        family_id: "rust_feature_policy_edge",
        sibling_fixture_id: MACHINE_LEAGUE_RUST_POLICY_VARIANT_FIXTURE_ID,
        repo_id: "machine-league-rust-feature-policy-fixture",
        package_name: "machine-league-rust-feature-policy",
        crate_name: "machine_league_rust_feature_policy",
        title: "Rust feature policy edge",
        allowed_scope: "src/lib.rs,src/policy.rs",
        visible_check: MACHINE_LEAGUE_RUST_FIXTURE_VISIBLE_CHECK,
        hidden_check: MACHINE_LEAGUE_RUST_FIXTURE_HIDDEN_CHECK,
        hidden_check_slot: "held-out restricted-region policy regression",
        acceptance_criteria: "visible and hidden Rust tests pass by fixing the shared policy boundary, not by bypassing it",
        transferable_edge: "trace the cross-file policy boundary before editing the route",
        native_baseline_strategy: "single_file_visible_case_patch",
        native_improved_strategy: "cross_file_symbol_policy_first",
    },
    MachineLeagueEvalFixture {
        id: MACHINE_LEAGUE_RUST_POLICY_VARIANT_FIXTURE_ID,
        snapshot_id: MACHINE_LEAGUE_RUST_POLICY_VARIANT_FIXTURE_SNAPSHOT_ID,
        family_id: "rust_feature_policy_edge",
        sibling_fixture_id: MACHINE_LEAGUE_RUST_POLICY_FIXTURE_ID,
        repo_id: "machine-league-rust-feature-policy-variant-fixture",
        package_name: "machine-league-rust-feature-policy-variant",
        crate_name: "machine_league_rust_feature_policy_variant",
        title: "Rust feature policy edge variant",
        allowed_scope: "src/lib.rs,src/policy.rs",
        visible_check: MACHINE_LEAGUE_RUST_FIXTURE_VISIBLE_CHECK,
        hidden_check: MACHINE_LEAGUE_RUST_FIXTURE_HIDDEN_CHECK,
        hidden_check_slot: "held-out partner-plan policy regression",
        acceptance_criteria: "visible and hidden Rust tests pass by fixing the shared policy boundary, not by bypassing it",
        transferable_edge: "trace the cross-file policy boundary before editing the route",
        native_baseline_strategy: "single_file_visible_case_patch",
        native_improved_strategy: "cross_file_symbol_policy_first",
    },
];

// The fail-closed fixture allowlist is generated from the versioned registry
// (generated/eval-suites/machine-league-fixture-registry.json) so graduated
// fixtures join the league by being appended to the registry, not by hand-
// editing a const. fixture-registry-check enforces that these ids stay exactly
// equal to MACHINE_LEAGUE_EVAL_FIXTURES above, so the allowlist is as strict as
// before. Defines MACHINE_LEAGUE_FIXTURE_REGISTRY_IDS.
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../generated/rust/machine-league-fixture-registry.rs"
));

pub(crate) struct RunEvalEvidenceReport {
    pub status: String,
    pub reason: String,
    pub result_receipt_id: String,
}

pub(crate) fn record_result_from_run_evidence(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
) -> Result<RunEvalEvidenceReport, String> {
    let body = format!(r#"{{"run_id":{}}}"#, json_string_literal(run_id));
    let response = render_result_from_run_json(&body, kernel)?;
    let value: serde_json::Value =
        serde_json::from_str(&response.body).map_err(|error| error.to_string())?;
    Ok(RunEvalEvidenceReport {
        status: value["status"].as_str().unwrap_or("UNKNOWN").to_string(),
        reason: value["reason"].as_str().unwrap_or("").to_string(),
        result_receipt_id: value["result_receipt_id"]
            .as_str()
            .unwrap_or("")
            .to_string(),
    })
}

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/forge/fleet-eval-scoreboard.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_forge_fleet_eval_scoreboard_json_with_kernel(Some(&kernel))
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/recommendations.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_machine_league_recommendations_json(&kernel),
        )));
    }
    if path == "/forge/machine-league/preflight.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_machine_league_preflight_json(&kernel),
        )));
    }
    if path == "/forge/machine-league/fleet-casts.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_machine_league_fleet_casts_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/fleet-runs.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_machine_league_fleet_runs_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/fleet-runs/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_machine_league_fleet_runs_projection_json(&kernel),
        )));
    }
    if path == "/forge/machine-league/fleet-runs/arbitrations.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_machine_league_fleet_run_arbitration_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/fleet-runs/arbitrations/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_machine_league_fleet_run_arbitrations_projection_json(&kernel),
        )));
    }
    if path == "/forge/machine-league/face-offs.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_machine_league_face_off_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/face-offs/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_machine_league_face_off_projection_json(&kernel),
        )));
    }
    if path == "/forge/machine-league/closed-runner-clearances.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_closed_runner_clearance_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/closed-runner-clearances/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_closed_runner_clearance_projection_json(&kernel),
        )));
    }
    if path == "/forge/machine-league/runtime-smokes.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_runtime_smoke_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/codex-trials.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_codex_trial_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/gemini-trials.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_gemini_trial_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/grok-build-trials.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_open_machine_trial_json(
                body,
                &mut kernel,
                open_machine_trial_spec("grok_build"),
            )
            .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/claude-code-trials.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_open_machine_trial_json(
                body,
                &mut kernel,
                open_machine_trial_spec("claude_code"),
            )
            .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/kimi-code-trials.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_open_machine_trial_json(body, &mut kernel, open_machine_trial_spec("kimi_code"))
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/opencode-trials.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_open_machine_trial_json(body, &mut kernel, open_machine_trial_spec("opencode"))
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/cline-trials.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_open_machine_trial_json(body, &mut kernel, open_machine_trial_spec("cline"))
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/goose-trials.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_open_machine_trial_json(body, &mut kernel, open_machine_trial_spec("goose"))
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/native-trials.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        // No kernel guard here: the harness path runs the real Forge loop,
        // which takes its own short kernel locks and deadlocks under a held
        // guard. render_native_trial_json takes short locks internally.
        return Some(
            render_native_trial_json(body, kernel).map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/pairwise-comparisons.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_pairwise_comparison_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/distillation-notes.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_distillation_note_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/machine-league/native-improvement-loops.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        return Some(render_native_improvement_loop_json(body, kernel));
    }
    if path == "/forge/machine-league/native-improvement-loops/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_native_improvement_loops_projection_json(&kernel),
        )));
    }
    if path == "/forge/machine-league/native-runtime-adaptations.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_native_runtime_adaptations_json(&kernel),
        )));
    }
    if path == "/forge/machine-league/learning-ledger.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_learning_ledger_json(&kernel),
        )));
    }
    if path == "/forge/fleet-eval-provider-preflight.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_provider_preflight_json(global()),
        )));
    }
    if path == "/forge/fleet-eval-live-run-approvals.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_live_run_approval_json(body, &mut kernel, global())
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/fleet-eval-live-run-approvals/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_live_run_approval_projection_json(&kernel),
        )));
    }
    if path == "/forge/fleet-eval-dry-runs.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_dry_run_json(body, &mut kernel).map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/fleet-eval-dry-runs/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_dry_run_projection_json(&kernel),
        )));
    }
    if path == "/forge/fleet-eval-results.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_result_ingestion_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/forge/fleet-eval-results/from-run.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        return Some(render_result_from_run_json(body, kernel));
    }
    if path == "/forge/fleet-eval-results/principal-reviews.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        return Some(render_principal_review_json(body, kernel));
    }
    if path == "/forge/fleet-eval-results/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_result_projection_json(&kernel),
        )));
    }
    None
}

fn render_machine_league_recommendations_json(kernel: &MdxKernel) -> String {
    let runners = forge_fleet_runner_profiles();
    let native = runner_profile("mdx_native_harness_runner", &runners);
    let native_adaptations = NativeRuntimeAdaptationProfile::from_kernel(kernel);
    let native_evidence_count = accepted_score_count_for_runner(kernel, native.runner_id);
    let external_scores = runners
        .iter()
        .filter(|runner| runner.runner_kind != "mdx_native")
        .map(|runner| {
            (
                runner,
                accepted_score_count_for_runner(kernel, runner.runner_id),
            )
        })
        .collect::<Vec<_>>();
    let best_external = external_scores
        .iter()
        .max_by(|(left_runner, left_count), (right_runner, right_count)| {
            left_count.cmp(right_count).then_with(|| {
                machine_league_runner_tiebreak_rank(right_runner.runner_id)
                    .cmp(&machine_league_runner_tiebreak_rank(left_runner.runner_id))
            })
        })
        .map(|(runner, count)| (*runner, *count));
    let selected_runner = match best_external {
        Some((runner, count)) if count > native_evidence_count => runner,
        _ => native,
    };
    let fallback_runner = if selected_runner.runner_id == native.runner_id {
        best_external
            .map(|(runner, _)| runner)
            .unwrap_or_else(|| runner_profile("codex_cli_external_worker", &runners))
    } else {
        native
    };
    let external_evidence_count = external_scores.iter().map(|(_, count)| *count).sum::<u32>();
    let status = if external_evidence_count == 0 {
        "MACHINE_LEAGUE_RECOMMENDATION_GATHERING_EVIDENCE"
    } else {
        "MACHINE_LEAGUE_RECOMMENDATION_READY"
    };
    let rationale = recommendation_rationale(selected_runner, native_evidence_count);
    format!(
        r#"{{"name":"mdx-forge-machine-league-recommendations","status":{},"route":"/forge/machine-league/recommendations.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","native_runtime_adaptation_route":"/forge/machine-league/native-runtime-adaptations.json","trial_route":"/forge/machine-league/codex-trials.json","trial_routes":{{{}}},"visibility_tier":"product_recommendation","selected_runner_id":{},"selected_runner_display_name":{},"fallback_runner_id":{},"fallback_runner_display_name":{},"task_class":"bug_fix","language_pack_id":"rust-cargo","recommendation_rationale":{},"confidence":"gathering_evidence","scorecard_evidence_count":{},"native_runtime_adaptation_status":{},"native_runtime_adaptation_count":{},"native_runtime_adaptation_applied_to_recommendation":{},"native_runtime_adaptation_grant_receipt_ids":{},"quarantine_boundary":"external runner output is held until MDx gates accept it","acceptance_gate":"mdx_quality_gates_then_principal_review","raw_scoreboard_is_front_door":false,"adapter_execution_allowed":false,"external_output_consumable":false,"production_write_allowed":false,"runner_profiles":[{}]}}"#,
        json_string_literal(status),
        machine_league_trial_routes_json(),
        json_string_literal(selected_runner.runner_id),
        json_string_literal(selected_runner.display_name),
        json_string_literal(fallback_runner.runner_id),
        json_string_literal(fallback_runner.display_name),
        json_string_literal(&rationale),
        native_evidence_count + external_evidence_count,
        json_string_literal(native_adaptations.status()),
        native_adaptations.count,
        native_adaptations.count > 0,
        json_string_literal(&native_adaptations.grant_receipt_ids),
        runners
            .iter()
            .map(runner_profile_json)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn machine_league_preflight_cache()
-> &'static std::sync::Mutex<Option<(std::time::Instant, String)>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Option<(std::time::Instant, String)>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(None))
}

fn render_machine_league_preflight_json(kernel: &MdxKernel) -> String {
    // Preflight probes every runner binary (version plus checksum) per request,
    // which is slow. Cache the rendered response briefly so repeated lens loads
    // are snappy. Runtime drift is detected fresh at trial and smoke time, not
    // here, so a short TTL does not mask a binary update or stale trial evidence.
    const PREFLIGHT_TTL_SECS: u64 = 30;
    if let Ok(guard) = machine_league_preflight_cache().lock()
        && let Some((observed_at, body)) = guard.as_ref()
        && observed_at.elapsed().as_secs() < PREFLIGHT_TTL_SECS
    {
        return body.clone();
    }
    let tenant_id = current_tenant_id();
    let secret_store = global();
    let runners = forge_fleet_runner_profiles();
    let any_live_enabled = runners
        .iter()
        .any(|runner| adapter_live_execution_enabled(runner.adapter_kind));
    let runtime_observations = observe_machine_runtime_static_batch(&runners);
    let checks = runners
        .iter()
        .zip(runtime_observations)
        .map(|(runner, observation)| {
            let binary_name = adapter_binary_name(runner.adapter_kind);
            let runtime = machine_runtime_fingerprint_from_static(
                Some(kernel),
                runner.runner_id,
                runner.adapter_kind,
                binary_name,
                adapter_command_contract(runner.adapter_kind),
                observation,
            );
            let binary_present = runtime.binary_present;
            let live_enabled = adapter_live_execution_enabled(runner.adapter_kind);
            let status = if runner.runner_kind == "mdx_native" {
                "preflight_ready"
            } else if binary_present && live_enabled {
                "live_execution_ready"
            } else if binary_present && !adapter_live_env_required(runner.adapter_kind).is_empty() {
                "preflight_binary_present_execution_disabled"
            } else if runner.execution_status.contains("TOS_AUTH") {
                "blocked_by_tos_auth_clearance"
            } else if binary_present {
                "preflight_binary_present_execution_denied"
            } else {
                "preflight_missing"
            };
            let live_env_required = adapter_live_env_required(runner.adapter_kind);
            let provider_key_enabled =
                machine_option_enabled_by_provider_key(secret_store, &tenant_id, runner.adapter_kind);
            let machine_option_enabled = runner.runner_kind == "mdx_native"
                || provider_key_enabled
                || (!closed_runner_requires_clearance(runner.adapter_kind) && binary_present);
            let option_enablement = if runner.adapter_kind == "grok_build_cli" && provider_key_enabled {
                "xai_key_present_acp_option_enabled"
            } else if runner.adapter_kind == "grok_build_cli" {
                "xai_key_required_for_grok_build_option"
            } else if runner.adapter_kind == "claude_code" && provider_key_enabled {
                "anthropic_key_present_agent_sdk_option_enabled"
            } else if runner.adapter_kind == "claude_code" {
                "anthropic_key_required_for_claude_agent_sdk_option"
            } else if runner.adapter_kind == "kimi_code" && provider_key_enabled {
                "moonshot_key_present_kimi_code_option_enabled"
            } else if runner.adapter_kind == "kimi_code" {
                "moonshot_key_required_for_kimi_code_option"
            } else if runner.runner_kind == "mdx_native" {
                "native_runner_always_available"
            } else if binary_present {
                "binary_present"
            } else {
                "binary_missing"
            };
            format!(
                r#"{{"runner_id":{},"display_name":{},"adapter_kind":{},"status":{},"binary_name":{},"binary_present":{},"version_observed":{},"runtime_fingerprint":{},"headless_support":{},"transcript_policy":{},"isolation_policy":{},"mcp_or_acp_supported":{},"acp_stdio_supported":{},"acp_stdio_command":{},"auth_method":{},"auth_env_key":{},"machine_option_enabled":{},"machine_option_enablement":{},"live_execution_env_required":{},"live_execution_enabled":{},"secret_export_allowed":false,"adapter_execution_allowed":{},"task_execution_allowed":{},"production_write_allowed":false}}"#,
                json_string_literal(runner.runner_id),
                json_string_literal(runner.display_name),
                json_string_literal(runner.adapter_kind),
                json_string_literal(if runner.adapter_kind == "grok_build_cli" && provider_key_enabled && binary_present && !live_enabled {
                    "acp_machine_option_ready_execution_disabled"
                } else if runner.adapter_kind == "grok_build_cli" && provider_key_enabled && !binary_present {
                    "acp_machine_option_key_present_binary_missing"
                } else if runner.adapter_kind == "grok_build_cli" && !provider_key_enabled {
                    "blocked_by_xai_key_missing"
                } else if runner.adapter_kind == "claude_code" && provider_key_enabled && binary_present && !live_enabled {
                    "agent_sdk_machine_option_ready_execution_disabled"
                } else if runner.adapter_kind == "claude_code" && provider_key_enabled && !binary_present {
                    "agent_sdk_machine_option_key_present_binary_missing"
                } else if runner.adapter_kind == "claude_code" && !provider_key_enabled {
                    "blocked_by_anthropic_key_missing"
                } else if runner.adapter_kind == "kimi_code" && provider_key_enabled && binary_present && !live_enabled {
                    "kimi_code_machine_option_ready_execution_disabled"
                } else if runner.adapter_kind == "kimi_code" && provider_key_enabled && !binary_present {
                    "kimi_code_machine_option_key_present_binary_missing"
                } else if runner.adapter_kind == "kimi_code" && !provider_key_enabled {
                    "blocked_by_moonshot_key_missing"
                } else {
                    status
                }),
                json_string_literal(binary_name),
                binary_present,
                json_string_literal(&runtime.version_observed),
                machine_runtime_json(&runtime),
                matches!(
                    runner.adapter_kind,
                    "codex_cli"
                        | "grok_build_cli"
                        | "claude_code"
                        | "kimi_code"
                        | "gemini_cli"
                        | "opencode"
                        | "cline"
                        | "goose"
                ),
                json_string_literal(runner.transcript_policy),
                json_string_literal(runner.isolation_policy),
                matches!(
                    runner.adapter_kind,
                    "codex_cli"
                        | "grok_build_cli"
                        | "claude_code"
                        | "kimi_code"
                        | "gemini_cli"
                        | "opencode"
                        | "cline"
                        | "goose"
                ),
                runner.adapter_kind == "grok_build_cli",
                json_string_literal(if runner.adapter_kind == "grok_build_cli" {
                    GROK_BUILD_EXEC_POLICY_COMMAND
                } else if runner.adapter_kind == "claude_code" {
                    CLAUDE_CODE_EXEC_POLICY_COMMAND
                } else if runner.adapter_kind == "kimi_code" {
                    KIMI_CODE_EXEC_POLICY_COMMAND
                } else {
                    ""
                }),
                json_string_literal(if runner.adapter_kind == "grok_build_cli" {
                    "xai.api_key"
                } else if runner.adapter_kind == "claude_code" {
                    "ANTHROPIC_API_KEY"
                } else if runner.adapter_kind == "kimi_code" {
                    "KIMI_MODEL_* ephemeral provider channel"
                } else {
                    ""
                }),
                json_string_literal(if runner.adapter_kind == "grok_build_cli" {
                    "XAI_API_KEY"
                } else if runner.adapter_kind == "claude_code" {
                    "ANTHROPIC_API_KEY"
                } else if runner.adapter_kind == "kimi_code" {
                    "MOONSHOT_API_KEY"
                } else {
                    ""
                }),
                machine_option_enabled,
                json_string_literal(option_enablement),
                json_string_literal(live_env_required),
                live_enabled,
                live_enabled,
                live_enabled,
            )
        })
        .collect::<Vec<_>>();
    let ready_runner_count = 1 + runners
        .iter()
        .filter(|runner| runner.runner_kind != "mdx_native")
        .filter(|runner| {
            binary_present(adapter_binary_name(runner.adapter_kind))
                && adapter_live_execution_enabled(runner.adapter_kind)
        })
        .count();
    let body = format!(
        r#"{{"name":"mdx-forge-machine-league-preflight","status":"MACHINE_LEAGUE_PREFLIGHT_FAIL_CLOSED","route":"/forge/machine-league/preflight.json","recommendation_route":"/forge/machine-league/recommendations.json","runner_count":{},"ready_runner_count":{},"presence_only":true,"provider_secret_values_exposed":false,"provider_secret_export_allowed":false,"adapter_execution_allowed":{},"task_execution_allowed":{},"live_execution_env_required":"MDX_MACHINE_LEAGUE_CODEX_EXEC=1 or MDX_MACHINE_LEAGUE_GROK_BUILD_EXEC=1 or MDX_MACHINE_LEAGUE_CLAUDE_CODE_EXEC=1 or MDX_MACHINE_LEAGUE_KIMI_CODE_EXEC=1 or MDX_MACHINE_LEAGUE_GEMINI_EXEC=1 or MDX_MACHINE_LEAGUE_OPENCODE_EXEC=1 or MDX_MACHINE_LEAGUE_CLINE_EXEC=1 or MDX_MACHINE_LEAGUE_GOOSE_EXEC=1","checks":[{}]}}"#,
        runners.len(),
        ready_runner_count,
        any_live_enabled,
        any_live_enabled,
        checks.join(",")
    );
    if let Ok(mut guard) = machine_league_preflight_cache().lock() {
        *guard = Some((std::time::Instant::now(), body.clone()));
    }
    body
}

#[derive(Clone, Debug)]
struct ClosedRunnerClearance {
    runner_id: String,
    runner_display_name: String,
    adapter_kind: String,
    adapter_protocol_tier: String,
    clearance_mode: String,
    auth_posture: String,
    invocation_contract: String,
    update_policy: String,
    transcript_policy: String,
    isolation_policy: String,
    scorecard_eligible: bool,
    clearance_receipt_id: String,
}

fn render_closed_runner_clearance_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "agent:closed_runner_clearance",
        "operator",
    );
    let runner_id = json_string_field(body, "runner_id").unwrap_or_default();
    let runners = forge_fleet_runner_profiles();
    let Some(runner) = runners
        .iter()
        .find(|runner| runner.runner_id == runner_id)
        .copied()
    else {
        return record_closed_runner_clearance_refusal(
            kernel,
            "unknown closed runner",
            &runner_id,
            "",
        );
    };
    if !live_runner_requires_clearance(runner.adapter_kind) {
        return record_closed_runner_clearance_refusal(
            kernel,
            "runner does not require live execution clearance",
            runner.runner_id,
            runner.adapter_kind,
        );
    }
    let clearance_basis = if runner.adapter_kind == "grok_build_cli" {
        "open_source_execution_attestation"
    } else {
        "closed_runner_terms_and_runtime_attestation"
    };
    if !json_bool_field(body, "operator_attestation").unwrap_or(false) {
        return record_closed_runner_clearance_refusal(
            kernel,
            if runner.adapter_kind == "grok_build_cli" {
                "operator_attestation=true is required before the open-source runner can execute live"
            } else {
                "operator_attestation=true is required before a closed runner can enter Forge"
            },
            runner.runner_id,
            runner.adapter_kind,
        );
    }
    let adapter_protocol_tier = json_string_field(body, "adapter_protocol_tier")
        .unwrap_or_else(|| default_closed_runner_adapter_tier(runner.adapter_kind).to_string());
    if !closed_runner_adapter_tier_allowed(&adapter_protocol_tier) {
        return record_closed_runner_clearance_refusal(
            kernel,
            "unsupported closed-runner adapter protocol tier",
            runner.runner_id,
            runner.adapter_kind,
        );
    }
    let clearance_mode =
        json_string_field(body, "clearance_mode").unwrap_or_else(|| "local_companion".to_string());
    if !matches!(clearance_mode.as_str(), "local_companion" | "fleet_eval") {
        return record_closed_runner_clearance_refusal(
            kernel,
            "clearance_mode must be local_companion or fleet_eval",
            runner.runner_id,
            runner.adapter_kind,
        );
    }
    let auth_posture = json_string_field(body, "auth_posture")
        .unwrap_or_else(|| default_closed_runner_auth_posture(runner.adapter_kind).to_string());
    let update_policy = json_string_field(body, "update_policy")
        .unwrap_or_else(|| "no_auto_update_during_eval".to_string());
    let invocation_contract = adapter_command_contract(runner.adapter_kind);
    let scorecard_eligible = clearance_mode == "fleet_eval";
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                run_id: "forge_machine_league_closed_runner_clearance",
                event: "evidence_appended",
                work_item_id: "machine_league_closed_runner_clearance",
                detail: &format!(
                    "runner_execution_clearance runner_id={} basis={} mode={} tier={} scorecard_eligible={}",
                    runner.runner_id,
                    clearance_basis,
                    clearance_mode,
                    adapter_protocol_tier,
                    scorecard_eligible
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &resolved.identity,
            &[
                ("runner_execution_clearance_status", "cleared"),
                ("runner_execution_clearance_basis", clearance_basis),
                ("closed_runner_clearance_status", "cleared"),
                ("runner_id", runner.runner_id),
                ("closed_runner_adapter_kind", runner.adapter_kind),
                (
                    "closed_runner_adapter_protocol_tier",
                    &adapter_protocol_tier,
                ),
                ("closed_runner_clearance_mode", &clearance_mode),
                ("closed_runner_auth_posture", &auth_posture),
                ("closed_runner_invocation_contract", invocation_contract),
                ("closed_runner_update_policy", &update_policy),
                ("closed_runner_transcript_policy", runner.transcript_policy),
                ("closed_runner_isolation_policy", runner.isolation_policy),
                (
                    "closed_runner_scorecard_eligible",
                    if scorecard_eligible { "true" } else { "false" },
                ),
                ("adapter_execution_allowed", "false"),
                ("task_execution_allowed", "false"),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-closed-runner-clearance-local-post","status":"CLOSED_RUNNER_CLEARANCE_RECORDED","route":"/forge/machine-league/closed-runner-clearances.json","projection_route":"/forge/machine-league/closed-runner-clearances/projection.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","runner_id":{},"runner_display_name":{},"adapter_kind":{},"adapter_protocol_tier":{},"clearance_mode":{},"clearance_basis":{},"closed_runner_terms_clearance_required":{},"live_execution_clearance_required":true,"auth_posture":{},"invocation_contract":{},"update_policy":{},"transcript_policy":{},"isolation_policy":{},"scorecard_eligible":{},"clearance_receipt_id":{},"adapter_execution_allowed":false,"task_execution_allowed":false,"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(runner.runner_id),
        json_string_literal(runner.display_name),
        json_string_literal(runner.adapter_kind),
        json_string_literal(&adapter_protocol_tier),
        json_string_literal(&clearance_mode),
        json_string_literal(clearance_basis),
        runner.adapter_kind != "grok_build_cli",
        json_string_literal(&auth_posture),
        json_string_literal(invocation_contract),
        json_string_literal(&update_policy),
        json_string_literal(runner.transcript_policy),
        json_string_literal(runner.isolation_policy),
        scorecard_eligible,
        json_string_literal(&report.receipt_id),
    ))
}

fn render_closed_runner_clearance_projection_json(kernel: &MdxKernel) -> String {
    let clearances = closed_runner_clearances(kernel);
    let clearance_rows = clearances
        .iter()
        .map(closed_runner_clearance_json)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"name":"mdx-forge-machine-league-closed-runner-clearance-projection","status":"CLOSED_RUNNER_CLEARANCE_PROJECTION_READY","route":"/forge/machine-league/closed-runner-clearances/projection.json","clearance_route":"/forge/machine-league/closed-runner-clearances.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","clearance_count":{},"scorecard_eligible_count":{},"adapter_execution_allowed":false,"task_execution_allowed":false,"external_output_consumable":false,"production_write_allowed":false,"clearances":[{}]}}"#,
        clearances.len(),
        clearances
            .iter()
            .filter(|clearance| clearance.scorecard_eligible)
            .count(),
        clearance_rows,
    )
}

fn record_closed_runner_clearance_refusal(
    kernel: &mut MdxKernel,
    reason: &str,
    runner_id: &str,
    adapter_kind: &str,
) -> Result<String, String> {
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:closed_runner_clearance",
                run_id: "forge_machine_league_closed_runner_clearance_refusal",
                event: "evidence_appended",
                work_item_id: "machine_league_closed_runner_clearance",
                detail: &format!("closed_runner_clearance_refused reason={reason}"),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:closed_runner_clearance"),
            &[
                ("closed_runner_clearance_status", "refused"),
                ("closed_runner_clearance_refusal_reason", reason),
                ("runner_id", runner_id),
                ("closed_runner_adapter_kind", adapter_kind),
                ("closed_runner_scorecard_eligible", "false"),
                ("adapter_execution_allowed", "false"),
                ("task_execution_allowed", "false"),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-closed-runner-clearance-local-post","status":"REFUSED","route":"/forge/machine-league/closed-runner-clearances.json","reason":{},"runner_id":{},"adapter_kind":{},"clearance_refusal_receipt_id":{},"scorecard_eligible":false,"adapter_execution_allowed":false,"task_execution_allowed":false,"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(reason),
        json_string_literal(runner_id),
        json_string_literal(adapter_kind),
        json_string_literal(&report.receipt_id),
    ))
}

fn default_closed_runner_adapter_tier(adapter_kind: &str) -> &'static str {
    match adapter_kind {
        "claude_code" => "sdk_adapter",
        "grok_build_cli" => "protocol_adapter",
        "kimi_code" => "headless_cli_adapter",
        _ => "headless_cli_adapter",
    }
}

fn adapter_protocol_tier_for_runner(
    kernel: Option<&MdxKernel>,
    runner: &FleetRunnerProfile,
) -> String {
    if let Some(clearance) = closed_runner_clearance_for_runner(kernel, runner.runner_id) {
        return clearance.adapter_protocol_tier;
    }
    match runner.adapter_kind {
        "mdx_native" => "native_harness",
        "grok_build_cli" => {
            if machine_option_enabled_by_provider_key(
                global(),
                &current_tenant_id(),
                runner.adapter_kind,
            ) {
                "protocol_adapter"
            } else {
                "declared_only"
            }
        }
        "claude_code" => {
            if machine_option_enabled_by_provider_key(
                global(),
                &current_tenant_id(),
                runner.adapter_kind,
            ) {
                "sdk_adapter"
            } else {
                "declared_only"
            }
        }
        "kimi_code" => {
            if machine_option_enabled_by_provider_key(
                global(),
                &current_tenant_id(),
                runner.adapter_kind,
            ) {
                "headless_cli_adapter"
            } else {
                "declared_only"
            }
        }
        "codex_cli" | "gemini_cli" | "opencode" | "cline" | "goose" => "headless_cli_adapter",
        _ => "declared_only",
    }
    .to_string()
}

fn default_closed_runner_auth_posture(adapter_kind: &str) -> &'static str {
    match adapter_kind {
        "claude_code" => "tenant_anthropic_api_key_presence_only",
        "grok_build_cli" => "tenant_xai_api_key_presence_only",
        "kimi_code" => "tenant_moonshot_api_key_ephemeral_model_env_only",
        _ => "local_user_auth_presence_only",
    }
}

fn closed_runner_adapter_tier_allowed(tier: &str) -> bool {
    matches!(
        tier,
        "sdk_adapter" | "protocol_adapter" | "headless_cli_adapter" | "local_companion_adapter"
    )
}

fn closed_runner_requires_clearance(adapter_kind: &str) -> bool {
    matches!(adapter_kind, "claude_code" | "kimi_code")
}

fn live_runner_requires_clearance(adapter_kind: &str) -> bool {
    matches!(
        adapter_kind,
        "codex_cli" | "grok_build_cli" | "claude_code" | "kimi_code"
    )
}

fn current_tenant_id() -> String {
    crate::request_security::current_verified_identity()
        .map(|identity| identity.tenant_id)
        .unwrap_or_else(|| "local_tenant".to_string())
}

fn machine_option_enabled_by_provider_key(
    secret_store: &SecretStore,
    tenant_id: &str,
    adapter_kind: &str,
) -> bool {
    let Some(env_key) = external_machine_provider_key(adapter_kind) else {
        return false;
    };
    secret_store.has_provider_key_for_tenant(tenant_id, env_key)
}

fn external_machine_provider_key(adapter_kind: &str) -> Option<&'static str> {
    match adapter_kind {
        "grok_build_cli" => Some("XAI_API_KEY"),
        "claude_code" => Some("ANTHROPIC_API_KEY"),
        "kimi_code" => Some("MOONSHOT_API_KEY"),
        _ => None,
    }
}

fn closed_runner_scorecard_enabled(
    kernel: Option<&MdxKernel>,
    runner: &FleetRunnerProfile,
) -> bool {
    if machine_option_enabled_by_provider_key(global(), &current_tenant_id(), runner.adapter_kind) {
        return true;
    }
    closed_runner_clearance_for_runner(kernel, runner.runner_id)
        .map(|clearance| clearance.scorecard_eligible)
        .unwrap_or(false)
}

fn closed_runner_clearances(kernel: &MdxKernel) -> Vec<ClosedRunnerClearance> {
    closed_runner_clearances_for_tenant(kernel, &current_tenant_id())
}

fn closed_runner_clearances_for_tenant(
    kernel: &MdxKernel,
    tenant_id: &str,
) -> Vec<ClosedRunnerClearance> {
    let runners = forge_fleet_runner_profiles();
    let mut clearances = Vec::new();
    for receipt in kernel.ledger().query().by_kind("forge.run.event") {
        if receipt.tenant_id.as_str() != tenant_id
            || payload_value(receipt, "closed_runner_clearance_status") != "cleared"
        {
            continue;
        }
        let runner_id = payload_value(receipt, "runner_id");
        let Some(runner) = runners.iter().find(|runner| runner.runner_id == runner_id) else {
            continue;
        };
        clearances.retain(|clearance: &ClosedRunnerClearance| clearance.runner_id != runner_id);
        clearances.push(ClosedRunnerClearance {
            runner_id: runner.runner_id.to_string(),
            runner_display_name: runner.display_name.to_string(),
            adapter_kind: runner.adapter_kind.to_string(),
            adapter_protocol_tier: payload_value(receipt, "closed_runner_adapter_protocol_tier")
                .to_string(),
            clearance_mode: payload_value(receipt, "closed_runner_clearance_mode").to_string(),
            auth_posture: payload_value(receipt, "closed_runner_auth_posture").to_string(),
            invocation_contract: payload_value(receipt, "closed_runner_invocation_contract")
                .to_string(),
            update_policy: payload_value(receipt, "closed_runner_update_policy").to_string(),
            transcript_policy: payload_value(receipt, "closed_runner_transcript_policy")
                .to_string(),
            isolation_policy: payload_value(receipt, "closed_runner_isolation_policy").to_string(),
            scorecard_eligible: payload_value(receipt, "closed_runner_scorecard_eligible")
                == "true",
            clearance_receipt_id: receipt.receipt_id.clone(),
        });
    }
    clearances
}

fn closed_runner_clearance_for_runner(
    kernel: Option<&MdxKernel>,
    runner_id: &str,
) -> Option<ClosedRunnerClearance> {
    kernel.and_then(|kernel| {
        closed_runner_clearances(kernel)
            .into_iter()
            .find(|clearance| clearance.runner_id == runner_id)
    })
}

fn closed_runner_clearance_for_runner_for_tenant(
    kernel: &MdxKernel,
    runner_id: &str,
    tenant_id: &str,
) -> Option<ClosedRunnerClearance> {
    closed_runner_clearances_for_tenant(kernel, tenant_id)
        .into_iter()
        .find(|clearance| clearance.runner_id == runner_id)
}

#[derive(Clone, Debug)]
pub(crate) struct LiveTrialAuthorization {
    pub(crate) approval_id: String,
    pub(crate) approval_receipt_id: String,
    pub(crate) clearance_receipt_id: String,
    pub(crate) provider: &'static str,
    pub(crate) max_spend_cents: u32,
    pub(crate) max_tasks: u32,
    pub(crate) max_parallel_agents: u32,
    pub(crate) task_ordinal: u32,
}

impl LiveTrialAuthorization {
    fn provider_for_adapter(adapter_kind: &str) -> Option<&'static str> {
        match adapter_kind {
            "codex_cli" => Some("openai"),
            "grok_build_cli" => Some("xai"),
            "claude_code" => Some("anthropic"),
            "kimi_code" => Some("moonshot"),
            _ => None,
        }
    }
}

struct ExternalOutputBoundary<'a> {
    actor_id: &'a str,
    run_id: &'a str,
    work_item_id: &'a str,
    runner_id: &'a str,
    adapter_kind: &'a str,
    output_artifact_ref: &'a str,
}

fn authorize_live_machine_trial(
    body: &str,
    kernel: &MdxKernel,
    runner_id: &str,
    adapter_kind: &str,
) -> Result<LiveTrialAuthorization, String> {
    authorize_live_machine_trial_for_tenant(
        body,
        kernel,
        runner_id,
        adapter_kind,
        &current_tenant_id(),
    )
}

fn authorize_live_machine_trial_for_tenant(
    body: &str,
    kernel: &MdxKernel,
    runner_id: &str,
    adapter_kind: &str,
    tenant_id: &str,
) -> Result<LiveTrialAuthorization, String> {
    let provider = LiveTrialAuthorization::provider_for_adapter(adapter_kind)
        .ok_or_else(|| format!("live authorization is not defined for adapter {adapter_kind}"))?;
    let requested_clearance_receipt_id = if live_runner_requires_clearance(adapter_kind) {
        let requested = json_string_field(body, "clearance_receipt_id")
            .ok_or_else(|| "clearance_receipt_id is required for live execution".to_string())?;
        let clearance = closed_runner_clearance_for_runner_for_tenant(kernel, runner_id, tenant_id)
            .ok_or_else(|| format!("fleet-eval clearance is required for runner {runner_id}"))?;
        if clearance.clearance_mode != "fleet_eval" || !clearance.scorecard_eligible {
            return Err(format!(
                "runner {runner_id} requires a scorecard-eligible fleet_eval execution clearance"
            ));
        }
        if clearance.clearance_receipt_id != requested {
            return Err(format!(
                "clearance_receipt_id does not match the current execution clearance for runner {runner_id}"
            ));
        }
        requested
    } else {
        String::new()
    };

    let approval_id = json_string_field(body, "approval_id")
        .ok_or_else(|| "approval_id is required for live execution".to_string())?;
    let approval_receipt_id = json_string_field(body, "approval_receipt_id")
        .ok_or_else(|| "approval_receipt_id is required for live execution".to_string())?;
    let approval = kernel
        .ledger()
        .entries()
        .iter()
        .rev()
        .find(|receipt| {
            receipt.kind == "forge.fleet_eval_live_run.approved"
                && receipt.tenant_id.as_str() == tenant_id
                && receipt.receipt_id == approval_receipt_id
                && payload_value(receipt, "approval_id") == approval_id
        })
        .ok_or_else(|| {
            "approval_id and approval_receipt_id must identify the same recorded live-run approval"
                .to_string()
        })?;
    let providers = payload_value(approval, "provider_allowlist")
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if !providers.into_iter().any(|value| value == provider) {
        return Err(format!(
            "live-run approval does not allow provider {provider} for adapter {adapter_kind}"
        ));
    }
    let positive_limit = |field: &str| {
        payload_value(approval, field)
            .parse::<u32>()
            .ok()
            .filter(|value| *value > 0)
    };
    let max_spend_cents = positive_limit("max_spend_cents");
    let max_tasks = positive_limit("max_tasks");
    let max_parallel_agents = positive_limit("max_parallel_agents");
    let (Some(max_spend_cents), Some(max_tasks), Some(max_parallel_agents)) =
        (max_spend_cents, max_tasks, max_parallel_agents)
    else {
        return Err(
            "live-run approval requires positive spend, task, and parallel limits".to_string(),
        );
    };
    let requested_spend_cents =
        json_u32_field(body, "trial_max_spend_cents").unwrap_or(max_spend_cents);
    if requested_spend_cents == 0 || requested_spend_cents > max_spend_cents {
        return Err(format!(
            "trial_max_spend_cents must be positive and no greater than the approved {max_spend_cents} cents"
        ));
    }
    let used_tasks = kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| {
            receipt.tenant_id.as_str() == tenant_id
                && payload_value(receipt, "live_run_approval_receipt_id") == approval_receipt_id
                && payload_value(receipt, "adapter_execution_performed") == "true"
        })
        .count() as u32;
    if used_tasks >= max_tasks {
        return Err(format!(
            "live-run approval task limit is exhausted: {used_tasks} of {max_tasks} tasks already executed"
        ));
    }
    if !payload_value(approval, "artifact_retention_policy").starts_with("quarantine") {
        return Err("live-run approval must retain artifacts in quarantine".to_string());
    }
    if !payload_value(approval, "redaction_policy").contains("mdx_context_redaction_required") {
        return Err("live-run approval must require MDx context redaction".to_string());
    }
    if payload_value(approval, "stop_conditions").trim().is_empty() {
        return Err("live-run approval must define stop conditions".to_string());
    }
    if let Some(provider_key) = external_machine_provider_key(adapter_kind)
        && !global().has_provider_key_for_tenant(tenant_id, provider_key)
    {
        return Err(format!(
            "{adapter_kind} live execution requires a tenant-scoped {provider_key} in /forge/model-providers.json; local subscription auth does not grant official adapter API authority"
        ));
    }

    Ok(LiveTrialAuthorization {
        approval_id,
        approval_receipt_id,
        clearance_receipt_id: requested_clearance_receipt_id,
        provider,
        max_spend_cents: requested_spend_cents,
        max_tasks,
        max_parallel_agents,
        task_ordinal: used_tasks + 1,
    })
}

/// Resolve the one-time external-run authority chain for an ordinary Forge
/// run. Operators establish clearance and a bounded live-run approval once;
/// subsequent explicit harness selections bind the latest compatible
/// receipts automatically instead of making engineers copy opaque ids.
pub(crate) fn authorize_forge_external_run(
    body: &str,
    kernel: &MdxKernel,
    tenant_id: &str,
    harness_id: &str,
    requested_max_spend_cents: u32,
    requested_workers: u32,
) -> Result<LiveTrialAuthorization, String> {
    let (runner_id, adapter_kind) = match harness_id {
        "codex_cli" => ("codex_cli_external_worker", "codex_cli"),
        "grok_build" => ("grok_build_cli_external_worker", "grok_build_cli"),
        _ => {
            return Err(format!(
                "live external-run authorization is not defined for harness {harness_id}"
            ));
        }
    };
    if !adapter_live_execution_enabled(adapter_kind) {
        return Err(format!(
            "{} is selected but live execution is still off. Set {} on the Forge server after reviewing the external-run boundary.",
            if harness_id == "grok_build" {
                "Grok Build"
            } else {
                "Codex CLI"
            },
            adapter_live_env_required(adapter_kind)
        ));
    }
    let workers = requested_workers.max(1);
    let harness_label = if harness_id == "grok_build" {
        "Grok Build"
    } else {
        "Codex CLI"
    };
    let clearance = closed_runner_clearance_for_runner_for_tenant(kernel, runner_id, tenant_id)
        .ok_or_else(|| format!(
            "{} needs one-time execution setup. Record a fleet_eval clearance at /forge/machine-league/closed-runner-clearances.json first.",
            harness_label
        ))?;
    if clearance.clearance_mode != "fleet_eval" || !clearance.scorecard_eligible {
        return Err(format!(
            "{} needs a scorecard-eligible fleet_eval execution clearance before it can build.",
            harness_label
        ));
    }
    let provider = LiveTrialAuthorization::provider_for_adapter(adapter_kind)
        .ok_or_else(|| format!("no live provider mapping for {adapter_kind}"))?;
    let requested_total = requested_max_spend_cents
        .max(1)
        .checked_mul(workers)
        .ok_or_else(|| "external-run spend request overflowed its bounded budget".to_string())?;
    let explicit_approval_id = json_string_field(body, "approval_id");
    let explicit_approval_receipt_id = json_string_field(body, "approval_receipt_id");
    let candidates = kernel.ledger().entries().iter().rev().filter(|receipt| {
        receipt.kind == "forge.fleet_eval_live_run.approved"
            && receipt.tenant_id.as_str() == tenant_id
            && explicit_approval_id
                .as_deref()
                .is_none_or(|id| payload_value(receipt, "approval_id") == id)
            && explicit_approval_receipt_id
                .as_deref()
                .is_none_or(|id| receipt.receipt_id == id)
            && payload_value(receipt, "provider_allowlist")
                .split(',')
                .map(str::trim)
                .any(|value| value == provider)
    });
    let mut refusal = None;
    for approval in candidates {
        let max_spend = payload_value(approval, "max_spend_cents")
            .parse::<u32>()
            .unwrap_or(0);
        let max_parallel = payload_value(approval, "max_parallel_agents")
            .parse::<u32>()
            .unwrap_or(0);
        let max_tasks = payload_value(approval, "max_tasks")
            .parse::<u32>()
            .unwrap_or(0);
        let used_tasks = kernel
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .iter()
            .filter(|receipt| {
                receipt.tenant_id.as_str() == tenant_id
                    && payload_value(receipt, "live_run_approval_receipt_id") == approval.receipt_id
                    && payload_value(receipt, "adapter_execution_performed") == "true"
            })
            .count() as u32;
        if requested_total > max_spend {
            refusal = Some(format!(
                "the selected external run needs a {requested_total}-cent aggregate ceiling, above the latest {provider} approval's {max_spend}-cent limit"
            ));
            continue;
        }
        if workers > max_parallel {
            refusal = Some(format!(
                "the selected external run needs {workers} parallel workers, above the latest {provider} approval's {max_parallel}-worker limit"
            ));
            continue;
        }
        if used_tasks.saturating_add(workers) > max_tasks {
            refusal = Some(format!(
                "the latest {provider} approval has only {} task slots remaining, but this run needs {workers}",
                max_tasks.saturating_sub(used_tasks)
            ));
            continue;
        }
        let authorization_body = serde_json::json!({
            "approval_id": payload_value(approval, "approval_id"),
            "approval_receipt_id": approval.receipt_id,
            "clearance_receipt_id": clearance.clearance_receipt_id,
            "trial_max_spend_cents": requested_max_spend_cents.max(1),
        })
        .to_string();
        return authorize_live_machine_trial_for_tenant(
            &authorization_body,
            kernel,
            runner_id,
            adapter_kind,
            tenant_id,
        );
    }
    Err(refusal.unwrap_or_else(|| {
        format!(
            "{} needs a compatible bounded live-run approval at /forge/fleet-eval-live-run-approvals.json.",
            harness_label
        )
    }))
}

fn record_external_output_consumption_boundary(
    kernel: &mut MdxKernel,
    identity: &GovernedWriteIdentity,
    boundary: ExternalOutputBoundary<'_>,
    authorization: Option<&LiveTrialAuthorization>,
) -> Result<String, String> {
    let approval_id = authorization
        .map(|value| value.approval_id.as_str())
        .unwrap_or("");
    let approval_receipt_id = authorization
        .map(|value| value.approval_receipt_id.as_str())
        .unwrap_or("");
    let clearance_receipt_id = authorization
        .map(|value| value.clearance_receipt_id.as_str())
        .unwrap_or("");
    let provider = authorization.map(|value| value.provider).unwrap_or("");
    let max_spend_cents = authorization
        .map(|value| value.max_spend_cents.to_string())
        .unwrap_or_default();
    let max_tasks = authorization
        .map(|value| value.max_tasks.to_string())
        .unwrap_or_default();
    let max_parallel_agents = authorization
        .map(|value| value.max_parallel_agents.to_string())
        .unwrap_or_default();
    let task_ordinal = authorization
        .map(|value| value.task_ordinal.to_string())
        .unwrap_or_default();
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: boundary.actor_id,
                run_id: boundary.run_id,
                event: "evidence_appended",
                work_item_id: boundary.work_item_id,
                detail: "external output remains quarantined and was not consumed by MDx",
                turn: 1,
                input_tokens: 0,
                output_tokens: 0,
            },
            identity,
            &[
                ("runner_id", boundary.runner_id),
                ("adapter_kind", boundary.adapter_kind),
                ("live_execution_provider", provider),
                ("live_run_approval_id", approval_id),
                ("live_run_approval_receipt_id", approval_receipt_id),
                (
                    "runner_execution_clearance_receipt_id",
                    clearance_receipt_id,
                ),
                ("closed_runner_clearance_receipt_id", clearance_receipt_id),
                ("live_run_max_spend_cents", max_spend_cents.as_str()),
                ("live_run_max_tasks", max_tasks.as_str()),
                ("live_run_max_parallel_agents", max_parallel_agents.as_str()),
                ("live_run_task_ordinal", task_ordinal.as_str()),
                ("source_output_artifact_ref", boundary.output_artifact_ref),
                ("external_output_consumption_attempted", "false"),
                (
                    "external_output_consumption_status",
                    "denied_quarantined_pending_distillation",
                ),
                ("external_output_consumable", "false"),
                ("output_quarantined", "true"),
                ("consumption_grants_execution_authority", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(report.receipt_id)
}

fn closed_runner_clearance_json(clearance: &ClosedRunnerClearance) -> String {
    format!(
        r#"{{"runner_id":{},"runner_display_name":{},"adapter_kind":{},"adapter_protocol_tier":{},"clearance_mode":{},"auth_posture":{},"invocation_contract":{},"update_policy":{},"transcript_policy":{},"isolation_policy":{},"scorecard_eligible":{},"clearance_receipt_id":{},"adapter_execution_allowed":false,"task_execution_allowed":false,"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(&clearance.runner_id),
        json_string_literal(&clearance.runner_display_name),
        json_string_literal(&clearance.adapter_kind),
        json_string_literal(&clearance.adapter_protocol_tier),
        json_string_literal(&clearance.clearance_mode),
        json_string_literal(&clearance.auth_posture),
        json_string_literal(&clearance.invocation_contract),
        json_string_literal(&clearance.update_policy),
        json_string_literal(&clearance.transcript_policy),
        json_string_literal(&clearance.isolation_policy),
        clearance.scorecard_eligible,
        json_string_literal(&clearance.clearance_receipt_id),
    )
}

fn render_machine_league_fleet_casts_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    let requested_worker_count = json_u32_field(body, "worker_count")
        .unwrap_or(8)
        .clamp(1, 24);
    let task_class = json_string_field(body, "task_class").unwrap_or_else(|| "bug_fix".to_string());
    let language_pack_id =
        json_string_field(body, "language_pack_id").unwrap_or_else(|| "rust-cargo".to_string());
    let policy_profile = json_string_field(body, "policy_profile")
        .unwrap_or_else(|| "standard_quarantined".to_string());
    let strategy = json_string_field(body, "strategy").unwrap_or_else(|| "compete".to_string());
    let budget_cents = json_u32_field(body, "budget_cents").unwrap_or(800);
    let native_adaptations = NativeRuntimeAdaptationProfile::from_kernel(kernel);
    let plan_seed = format!(
        "{requested_worker_count}:{task_class}:{language_pack_id}:{policy_profile}:{strategy}:{budget_cents}"
    );
    let plan_id = format!(
        "forge_fleet_cast_{}",
        &hex(&sha256(plan_seed.as_bytes()))[..12]
    );
    let compatible = prioritized_fleet_cast_pairs(Some(kernel), &policy_profile);
    let excluded = excluded_fleet_runner_profiles_json(Some(kernel), &policy_profile);
    if compatible.is_empty() {
        let report = record_fleet_cast_plan_receipt(
            kernel,
            FleetCastPlanReceipt {
                plan_id: &plan_id,
                status: "FLEET_CAST_REFUSED",
                requested_worker_count,
                planned_worker_count: 0,
                unique_cast_count: 0,
                policy_profile: &policy_profile,
                task_class: &task_class,
                language_pack_id: &language_pack_id,
                strategy: &strategy,
            },
        )?;
        return Ok(format!(
            r#"{{"name":"mdx-forge-machine-league-fleet-casts-local-post","status":"FLEET_CAST_REFUSED","route":"/forge/machine-league/fleet-casts.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","preflight_route":"/forge/machine-league/preflight.json","plan_id":{},"fleet_cast_receipt_id":{},"reason":"no compatible runner model casts admitted by policy","worker_count_requested":{},"worker_count_planned":0,"unique_cast_count":0,"policy_profile":{},"task_class":{},"language_pack_id":{},"strategy":{},"cast_reuse_policy":"none","can_run_8_wide":false,"can_run_16_wide":false,"can_run_24_wide":false,"adapter_execution_allowed":false,"task_execution_allowed":false,"external_output_consumable":false,"production_write_allowed":false,"worker_casts":[],"excluded_runner_profiles":[{}]}}"#,
            json_string_literal(&plan_id),
            json_string_literal(&report.receipt_id),
            requested_worker_count,
            json_string_literal(&policy_profile),
            json_string_literal(&task_class),
            json_string_literal(&language_pack_id),
            json_string_literal(&strategy),
            excluded,
        ));
    }
    let worker_casts = planned_worker_casts_json(
        Some(kernel),
        &compatible,
        requested_worker_count,
        &policy_profile,
    );
    let unique_cast_count = compatible.len() as u32;
    let report = record_fleet_cast_plan_receipt(
        kernel,
        FleetCastPlanReceipt {
            plan_id: &plan_id,
            status: "FLEET_CAST_PLAN_READY",
            requested_worker_count,
            planned_worker_count: requested_worker_count,
            unique_cast_count,
            policy_profile: &policy_profile,
            task_class: &task_class,
            language_pack_id: &language_pack_id,
            strategy: &strategy,
        },
    )?;
    record_native_runtime_adaptation_applied_receipts(
        kernel,
        &native_adaptations,
        "machine_league_fleet_cast",
        &plan_id,
        "ratified native runtime adaptation informed Machine League fleet cast planning",
    );
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-fleet-casts-local-post","status":"FLEET_CAST_PLAN_READY","route":"/forge/machine-league/fleet-casts.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","preflight_route":"/forge/machine-league/preflight.json","native_runtime_adaptation_route":"/forge/machine-league/native-runtime-adaptations.json","plan_id":{},"fleet_cast_receipt_id":{},"worker_count_requested":{},"worker_count_planned":{},"unique_cast_count":{},"policy_profile":{},"task_class":{},"language_pack_id":{},"strategy":{},"budget_cents":{},"compatibility_matrix_source":"scoreboard.runner_model_compatibility_matrix","native_runtime_adaptation_status":{},"native_runtime_adaptation_count":{},"native_runtime_adaptation_grant_receipt_ids":{},"cast_reuse_policy":"cycle_compatible_pairs_with_distinct_worktree_after_unique_casts","has_8_unique_casts":{},"can_run_8_wide":true,"can_run_16_wide":true,"can_run_24_wide":true,"fleet_widths_proven":[8,16,24],"quarantine_boundary":"every external worker output stays held until MDx quality gates and principal review accept it","adapter_execution_allowed":false,"task_execution_allowed":false,"external_output_consumable":false,"production_write_allowed":false,"worker_casts":[{}],"excluded_runner_profiles":[{}]}}"#,
        json_string_literal(&plan_id),
        json_string_literal(&report.receipt_id),
        requested_worker_count,
        requested_worker_count,
        unique_cast_count,
        json_string_literal(&policy_profile),
        json_string_literal(&task_class),
        json_string_literal(&language_pack_id),
        json_string_literal(&strategy),
        budget_cents,
        json_string_literal(native_adaptations.status()),
        native_adaptations.count,
        json_string_literal(&native_adaptations.grant_receipt_ids),
        unique_cast_count >= 8,
        worker_casts,
        excluded,
    ))
}

fn render_machine_league_fleet_runs_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    let requested_worker_count = json_u32_field(body, "worker_count")
        .unwrap_or(8)
        .clamp(1, 24);
    let task_class = json_string_field(body, "task_class").unwrap_or_else(|| "bug_fix".to_string());
    let language_pack_id =
        json_string_field(body, "language_pack_id").unwrap_or_else(|| "rust-cargo".to_string());
    let policy_profile = json_string_field(body, "policy_profile")
        .unwrap_or_else(|| "standard_quarantined".to_string());
    let eval_fixture = json_string_field(body, "eval_fixture")
        .unwrap_or_else(|| MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID.to_string());
    if !is_machine_league_fixture(&eval_fixture) {
        return record_machine_league_fleet_run_refusal(
            kernel,
            "fleet execution requires a registered Machine League eval fixture",
            &eval_fixture,
        );
    }
    let execute_live = json_bool_field(body, "execute_live").unwrap_or(false);
    let execute_native_live = json_bool_field(body, "execute_native_live").unwrap_or(execute_live);
    let execution_backend = json_string_field(body, "execution_backend")
        .or_else(|| json_string_field(body, "execution_backend_kind"))
        .unwrap_or_else(|| "local".to_string());
    let run_seed = format!(
        "{requested_worker_count}:{task_class}:{language_pack_id}:{policy_profile}:{eval_fixture}:{execute_live}:{execute_native_live}:{execution_backend}"
    );
    let fleet_run_id = json_string_field(body, "fleet_run_id").unwrap_or_else(|| {
        format!(
            "machine_league_fleet_run_{}",
            &hex(&sha256(run_seed.as_bytes()))[..12]
        )
    });
    let cast_body = format!(
        r#"{{"worker_count":{},"task_class":{},"language_pack_id":{},"policy_profile":{},"strategy":"compete"}}"#,
        requested_worker_count,
        json_string_literal(&task_class),
        json_string_literal(&language_pack_id),
        json_string_literal(&policy_profile),
    );
    let cast_packet = render_machine_league_fleet_casts_json(&cast_body, kernel)?;
    let cast_json: serde_json::Value =
        serde_json::from_str(&cast_packet).map_err(|error| error.to_string())?;
    if cast_json["status"].as_str() != Some("FLEET_CAST_PLAN_READY") {
        return record_machine_league_fleet_run_refusal(
            kernel,
            cast_json["reason"]
                .as_str()
                .unwrap_or("fleet cast plan was not ready"),
            &eval_fixture,
        );
    }
    let workers = cast_json["worker_casts"]
        .as_array()
        .ok_or_else(|| "fleet cast packet did not include worker_casts".to_string())?;
    let cast_plan_id = cast_json["plan_id"].as_str().unwrap_or("");
    let cast_receipt_id = cast_json["fleet_cast_receipt_id"].as_str().unwrap_or("");
    let mut lanes = Vec::new();
    for (index, worker) in workers.iter().enumerate() {
        let runner_id = worker["runner_id"].as_str().unwrap_or("");
        let runner_display_name = worker["runner_display_name"].as_str().unwrap_or(runner_id);
        let model_profile_id = worker["model_profile_id"].as_str().unwrap_or("");
        let adapter_protocol_tier = worker["adapter_protocol_tier"].as_str().unwrap_or("");
        let worker_id = worker["worker_id"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| format!("fleet_cast_{:02}", index + 1));
        let lane_index = worker["lane_index"].as_u64().unwrap_or((index + 1) as u64);
        let lane_run_id = format!("{}_lane_{:02}", fleet_run_id, lane_index);
        let lane = if runner_id == "mdx_native_harness_runner" && !execute_native_live {
            MachineLeagueFleetRunLane {
                worker_id,
                lane_index,
                runner_id: runner_id.to_string(),
                runner_display_name: runner_display_name.to_string(),
                model_profile_id: model_profile_id.to_string(),
                adapter_protocol_tier: adapter_protocol_tier.to_string(),
                lane_status: "PLANNED_NATIVE_BASELINE_HELD".to_string(),
                trial_status: "native_execute_live_not_requested".to_string(),
                run_id: String::new(),
                result_receipt_id: String::new(),
                execution_requested: false,
                execution_allowed: false,
                execution_performed: false,
                check_passed: false,
                blocked_reason: "native baseline held until execute_native_live=true".to_string(),
                output_state: "no_output_recorded".to_string(),
            }
        } else {
            let lane_execute_live = if runner_id == "mdx_native_harness_runner" {
                execute_native_live
            } else {
                execute_live
            };
            let trial_body = format!(
                r#"{{"eval_fixture":{},"execute_live":{},"execution_backend":{},"run_id":{},"work_packet_id":{}}}"#,
                json_string_literal(&eval_fixture),
                lane_execute_live,
                json_string_literal(&execution_backend),
                json_string_literal(&lane_run_id),
                json_string_literal(&format!("work_packet_{lane_run_id}")),
            );
            let trial_packet = if runner_id == "mdx_native_harness_runner" {
                // Fleet composition holds the kernel write guard while it
                // stages every lane, so the native lane runs the scripted
                // baseline (honestly labeled, excluded from scoreboard) here
                // rather than the lock-free harness loop.
                render_native_trial_scripted(&trial_body, kernel)
            } else {
                render_trial_for_runner(runner_id, &trial_body, kernel)
            }?;
            machine_league_lane_from_trial_packet(
                worker_id,
                lane_index,
                runner_id,
                runner_display_name,
                model_profile_id,
                adapter_protocol_tier,
                &trial_packet,
            )?
        };
        record_machine_league_fleet_run_lane(kernel, &fleet_run_id, &lane)?;
        lanes.push(lane);
    }
    let executed_count = lanes.iter().filter(|lane| lane.execution_performed).count();
    let requested_count = lanes.iter().filter(|lane| lane.execution_requested).count();
    let ready_for_review_count = lanes.iter().filter(|lane| lane.check_passed).count();
    let held_count = lanes
        .iter()
        .filter(|lane| !lane.execution_performed && lane.lane_status.contains("HELD"))
        .count();
    let blocked_count = lanes
        .iter()
        .filter(|lane| lane.lane_status.contains("BLOCKED") || lane.lane_status.contains("REFUSED"))
        .count();
    let fleet_status = if executed_count > 0 {
        "FLEET_RUN_EXECUTED_PARTIAL_QUARANTINED"
    } else {
        "FLEET_RUN_STAGED_HELD"
    };
    let integration_status = if ready_for_review_count > 0 {
        "waiting_for_principal_review_before_winner_selection"
    } else {
        "waiting_for_live_lane_execution"
    };
    let summary_receipt = record_machine_league_fleet_run_summary(
        kernel,
        MachineLeagueFleetRunSummary {
            fleet_run_id: &fleet_run_id,
            fleet_status,
            cast_plan_id,
            cast_receipt_id,
            requested_worker_count,
            planned_worker_count: lanes.len() as u32,
            execution_requested_count: requested_count as u32,
            executed_count: executed_count as u32,
            held_count: held_count as u32,
            blocked_count: blocked_count as u32,
            ready_for_review_count: ready_for_review_count as u32,
            task_class: &task_class,
            language_pack_id: &language_pack_id,
            eval_fixture: &eval_fixture,
            integration_status,
        },
    )?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-fleet-run-local-post","status":{},"route":"/forge/machine-league/fleet-runs.json","projection_route":"/forge/machine-league/fleet-runs/projection.json","fleet_run_id":{},"fleet_run_receipt_id":{},"cast_plan_id":{},"fleet_cast_receipt_id":{},"worker_count_requested":{},"worker_count_planned":{},"execution_requested_count":{},"executed_lane_count":{},"held_lane_count":{},"blocked_lane_count":{},"ready_for_review_count":{},"integration_status":{},"winner_selection_status":"waiting_for_principal_reviews","accepted_for_scoreboard_count":0,"external_output_consumable":false,"production_write_allowed":false,"lanes":[{}]}}"#,
        json_string_literal(fleet_status),
        json_string_literal(&fleet_run_id),
        json_string_literal(&summary_receipt.receipt_id),
        json_string_literal(cast_plan_id),
        json_string_literal(cast_receipt_id),
        requested_worker_count,
        lanes.len(),
        requested_count,
        executed_count,
        held_count,
        blocked_count,
        ready_for_review_count,
        json_string_literal(integration_status),
        lanes
            .iter()
            .map(machine_league_fleet_run_lane_json)
            .collect::<Vec<_>>()
            .join(","),
    ))
}

fn render_machine_league_fleet_runs_projection_json(kernel: &MdxKernel) -> String {
    let summaries = kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .into_iter()
        .filter(|receipt| payload_value(receipt, "machine_league_fleet_run_summary") == "true")
        .collect::<Vec<_>>();
    let runs = summaries
        .iter()
        .map(|receipt| machine_league_fleet_run_summary_json(kernel, receipt))
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-forge-machine-league-fleet-runs-projection","status":"MACHINE_LEAGUE_FLEET_RUNS_PROJECTION_READY","route":"/forge/machine-league/fleet-runs/projection.json","fleet_run_route":"/forge/machine-league/fleet-runs.json","fleet_cast_route":"/forge/machine-league/fleet-casts.json","principal_review_route":"/forge/fleet-eval-results/principal-reviews.json","learning_ledger_route":"/forge/machine-league/learning-ledger.json","fleet_run_count":{},"external_output_consumable":false,"production_write_allowed":false,"runs":[{}]}}"#,
        runs.len(),
        runs.join(","),
    )
}

#[derive(Clone)]
struct FleetRunAcceptedLane {
    lane_index: u64,
    worker_id: String,
    run_id: String,
    runner_id: String,
    runner_display_name: String,
    model_profile_id: String,
    result_receipt_id: String,
    principal_review_receipt_id: String,
    total_score: u32,
}

fn render_machine_league_fleet_run_arbitration_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    let fleet_run_id = json_string_field(body, "fleet_run_id").unwrap_or_default();
    let Some(summary) = latest_machine_league_fleet_run_summary(kernel, &fleet_run_id) else {
        return record_machine_league_fleet_run_arbitration_refusal(
            kernel,
            "no matching Machine League fleet run was found",
            &fleet_run_id,
        );
    };
    let resolved_fleet_run_id = payload_value(summary, "machine_league_fleet_run_id").to_string();
    let lanes = machine_league_fleet_run_lane_receipts(kernel, &resolved_fleet_run_id);
    let accepted_lanes = accepted_lanes_for_fleet_run(kernel, &lanes);
    let accepted_lane_count = accepted_lanes.len() as u32;
    let planned_lane_count = lanes.len() as u32;
    let waiting_lane_count = planned_lane_count.saturating_sub(accepted_lane_count);
    let winner = accepted_lanes
        .iter()
        .max_by_key(|lane| (lane.total_score, std::cmp::Reverse(lane.lane_index)));
    let status = if winner.is_some() {
        "FLEET_RUN_WINNER_RECOMMENDED_WAITING_FOR_HUMAN_RATIFICATION"
    } else {
        "FLEET_RUN_ARBITRATION_WAITING_FOR_ACCEPTED_LANES"
    };
    let winner_lane_index = winner.map(|lane| lane.lane_index).unwrap_or(0).to_string();
    let winner_worker_id = winner.map(|lane| lane.worker_id.as_str()).unwrap_or("");
    let winner_run_id = winner.map(|lane| lane.run_id.as_str()).unwrap_or("");
    let winner_runner_id = winner.map(|lane| lane.runner_id.as_str()).unwrap_or("");
    let winner_runner_display_name = winner
        .map(|lane| lane.runner_display_name.as_str())
        .unwrap_or("");
    let winner_model_profile_id = winner
        .map(|lane| lane.model_profile_id.as_str())
        .unwrap_or("");
    let winner_result_receipt_id = winner
        .map(|lane| lane.result_receipt_id.as_str())
        .unwrap_or("");
    let winner_review_receipt_id = winner
        .map(|lane| lane.principal_review_receipt_id.as_str())
        .unwrap_or("");
    let winner_total_score = winner.map(|lane| lane.total_score).unwrap_or(0).to_string();
    let report = record_machine_league_fleet_run_arbitration(
        kernel,
        FleetRunArbitrationRecord {
            fleet_run_id: &resolved_fleet_run_id,
            status,
            planned_lane_count,
            accepted_lane_count,
            waiting_lane_count,
            winner_lane_index: &winner_lane_index,
            winner_worker_id,
            winner_run_id,
            winner_runner_id,
            winner_runner_display_name,
            winner_model_profile_id,
            winner_result_receipt_id,
            winner_review_receipt_id,
            winner_total_score: &winner_total_score,
        },
    )?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-fleet-run-arbitration-local-post","status":{},"route":"/forge/machine-league/fleet-runs/arbitrations.json","projection_route":"/forge/machine-league/fleet-runs/arbitrations/projection.json","fleet_run_route":"/forge/machine-league/fleet-runs.json","fleet_run_id":{},"arbitration_receipt_id":{},"planned_lane_count":{},"accepted_lane_count":{},"waiting_lane_count":{},"winner_selection_basis":"accepted_scorecard_lane_receipts_only","winner_lane_index":{},"winner_worker_id":{},"winner_run_id":{},"winner_runner_id":{},"winner_runner_display_name":{},"winner_model_profile_id":{},"winner_result_receipt_id":{},"winner_principal_review_receipt_id":{},"winner_total_score":{},"accepted_for_scoreboard_count":{},"external_output_consumable":false,"production_write_allowed":false,"human_ratification_required":true}}"#,
        json_string_literal(status),
        json_string_literal(&resolved_fleet_run_id),
        json_string_literal(&report.receipt_id),
        planned_lane_count,
        accepted_lane_count,
        waiting_lane_count,
        winner_lane_index,
        json_string_literal(winner_worker_id),
        json_string_literal(winner_run_id),
        json_string_literal(winner_runner_id),
        json_string_literal(winner_runner_display_name),
        json_string_literal(winner_model_profile_id),
        json_string_literal(winner_result_receipt_id),
        json_string_literal(winner_review_receipt_id),
        winner_total_score,
        accepted_lane_count,
    ))
}

fn render_machine_league_fleet_run_arbitrations_projection_json(kernel: &MdxKernel) -> String {
    let arbitrations = kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| payload_value(receipt, "machine_league_fleet_run_arbitration") == "true")
        .map(|receipt| machine_league_fleet_run_arbitration_receipt_json(receipt))
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-forge-machine-league-fleet-run-arbitrations-projection","status":"FLEET_RUN_ARBITRATIONS_PROJECTION_READY","route":"/forge/machine-league/fleet-runs/arbitrations/projection.json","arbitration_route":"/forge/machine-league/fleet-runs/arbitrations.json","fleet_run_route":"/forge/machine-league/fleet-runs.json","arbitration_count":{},"winner_selection_basis":"accepted_scorecard_lane_receipts_only","external_output_consumable":false,"production_write_allowed":false,"arbitrations":[{}]}}"#,
        arbitrations.len(),
        arbitrations.join(",")
    )
}

fn latest_machine_league_fleet_run_summary<'a>(
    kernel: &'a MdxKernel,
    fleet_run_id: &str,
) -> Option<&'a mdx_core::Receipt> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
        .find(|receipt| {
            payload_value(receipt, "machine_league_fleet_run_summary") == "true"
                && (fleet_run_id.trim().is_empty()
                    || payload_value(receipt, "machine_league_fleet_run_id") == fleet_run_id.trim())
        })
        .copied()
}

fn machine_league_fleet_run_lane_receipts<'a>(
    kernel: &'a MdxKernel,
    fleet_run_id: &str,
) -> Vec<&'a mdx_core::Receipt> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| {
            payload_value(receipt, "machine_league_fleet_lane") == "true"
                && payload_value(receipt, "machine_league_fleet_run_id") == fleet_run_id
        })
        .copied()
        .collect()
}

fn accepted_lanes_for_fleet_run(
    kernel: &MdxKernel,
    lanes: &[&mdx_core::Receipt],
) -> Vec<FleetRunAcceptedLane> {
    lanes
        .iter()
        .filter_map(|lane| {
            let lane_run_id = payload_value(lane, "lane_run_id");
            if lane_run_id.trim().is_empty() {
                return None;
            }
            let review = latest_accepted_scorecard_receipt_for_lane(
                kernel,
                lane_run_id,
                payload_value(lane, "result_receipt_id"),
            )?;
            Some(FleetRunAcceptedLane {
                lane_index: payload_value(lane, "lane_index").parse().unwrap_or(0),
                worker_id: payload_value(lane, "worker_id").to_string(),
                run_id: lane_run_id.to_string(),
                runner_id: payload_value(lane, "runner_id").to_string(),
                runner_display_name: payload_value(lane, "runner_display_name").to_string(),
                model_profile_id: payload_value(lane, "model_profile_id").to_string(),
                result_receipt_id: payload_value(review, "eval_result_receipt_id").to_string(),
                principal_review_receipt_id: review.receipt_id.clone(),
                total_score: payload_value(review, "total_score").parse().unwrap_or(0),
            })
        })
        .collect()
}

fn latest_accepted_scorecard_receipt_for_lane<'a>(
    kernel: &'a MdxKernel,
    run_id: &str,
    result_receipt_id: &str,
) -> Option<&'a mdx_core::Receipt> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
        .find(|receipt| {
            (payload_value(receipt, "run_id") == run_id
                || (!result_receipt_id.trim().is_empty()
                    && payload_value(receipt, "eval_result_receipt_id") == result_receipt_id))
                && payload_value(receipt, "accepted_for_scoreboard") == "true"
        })
        .copied()
}

struct FleetRunArbitrationRecord<'a> {
    fleet_run_id: &'a str,
    status: &'a str,
    planned_lane_count: u32,
    accepted_lane_count: u32,
    waiting_lane_count: u32,
    winner_lane_index: &'a str,
    winner_worker_id: &'a str,
    winner_run_id: &'a str,
    winner_runner_id: &'a str,
    winner_runner_display_name: &'a str,
    winner_model_profile_id: &'a str,
    winner_result_receipt_id: &'a str,
    winner_review_receipt_id: &'a str,
    winner_total_score: &'a str,
}

fn record_machine_league_fleet_run_arbitration(
    kernel: &mut MdxKernel,
    record: FleetRunArbitrationRecord<'_>,
) -> Result<ForgeRunEventReport, String> {
    kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:machine_league_fleet_arbitrator",
                run_id: record.fleet_run_id,
                event: "evidence_appended",
                work_item_id: "machine_league_fleet_run_arbitration",
                detail: &format!(
                    "machine_league_fleet_run_arbitrated status={} accepted_lanes={}",
                    record.status, record.accepted_lane_count
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:machine_league_fleet_arbitrator"),
            &[
                ("machine_league_fleet_run_arbitration", "true"),
                ("machine_league_fleet_run_id", record.fleet_run_id),
                ("machine_league_fleet_run_arbitration_status", record.status),
                ("planned_lane_count", &record.planned_lane_count.to_string()),
                (
                    "accepted_lane_count",
                    &record.accepted_lane_count.to_string(),
                ),
                ("waiting_lane_count", &record.waiting_lane_count.to_string()),
                (
                    "winner_selection_basis",
                    "accepted_scorecard_lane_receipts_only",
                ),
                ("winner_lane_index", record.winner_lane_index),
                ("winner_worker_id", record.winner_worker_id),
                ("winner_run_id", record.winner_run_id),
                ("winner_runner_id", record.winner_runner_id),
                (
                    "winner_runner_display_name",
                    record.winner_runner_display_name,
                ),
                ("winner_model_profile_id", record.winner_model_profile_id),
                ("winner_result_receipt_id", record.winner_result_receipt_id),
                (
                    "winner_principal_review_receipt_id",
                    record.winner_review_receipt_id,
                ),
                ("winner_total_score", record.winner_total_score),
                ("human_ratification_required", "true"),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())
}

fn record_machine_league_fleet_run_arbitration_refusal(
    kernel: &mut MdxKernel,
    reason: &str,
    fleet_run_id: &str,
) -> Result<String, String> {
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:machine_league_fleet_arbitrator",
                run_id: if fleet_run_id.trim().is_empty() {
                    "machine_league_fleet_run_arbitration_refusal"
                } else {
                    fleet_run_id.trim()
                },
                event: "evidence_appended",
                work_item_id: "machine_league_fleet_run_arbitration",
                detail: &format!("machine_league_fleet_run_arbitration_refused reason={reason}"),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:machine_league_fleet_arbitrator"),
            &[
                ("machine_league_fleet_run_arbitration_status", "refused"),
                (
                    "machine_league_fleet_run_arbitration_refusal_reason",
                    reason,
                ),
                ("machine_league_fleet_run_id", fleet_run_id.trim()),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-fleet-run-arbitration-local-post","status":"REFUSED","route":"/forge/machine-league/fleet-runs/arbitrations.json","reason":{},"fleet_run_id":{},"arbitration_refusal_receipt_id":{},"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(reason),
        json_string_literal(fleet_run_id.trim()),
        json_string_literal(&report.receipt_id),
    ))
}

fn machine_league_fleet_run_arbitration_receipt_json(receipt: &mdx_core::Receipt) -> String {
    format!(
        r#"{{"fleet_run_id":{},"arbitration_receipt_id":{},"status":{},"planned_lane_count":{},"accepted_lane_count":{},"waiting_lane_count":{},"winner_selection_basis":{},"winner_lane_index":{},"winner_worker_id":{},"winner_run_id":{},"winner_runner_id":{},"winner_runner_display_name":{},"winner_model_profile_id":{},"winner_result_receipt_id":{},"winner_principal_review_receipt_id":{},"winner_total_score":{},"human_ratification_required":{},"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(payload_value(receipt, "machine_league_fleet_run_id")),
        json_string_literal(&receipt.receipt_id),
        json_string_literal(payload_value(
            receipt,
            "machine_league_fleet_run_arbitration_status"
        )),
        payload_value(receipt, "planned_lane_count")
            .parse::<u32>()
            .unwrap_or(0),
        payload_value(receipt, "accepted_lane_count")
            .parse::<u32>()
            .unwrap_or(0),
        payload_value(receipt, "waiting_lane_count")
            .parse::<u32>()
            .unwrap_or(0),
        json_string_literal(payload_value(receipt, "winner_selection_basis")),
        payload_value(receipt, "winner_lane_index")
            .parse::<u64>()
            .unwrap_or(0),
        json_string_literal(payload_value(receipt, "winner_worker_id")),
        json_string_literal(payload_value(receipt, "winner_run_id")),
        json_string_literal(payload_value(receipt, "winner_runner_id")),
        json_string_literal(payload_value(receipt, "winner_runner_display_name")),
        json_string_literal(payload_value(receipt, "winner_model_profile_id")),
        json_string_literal(payload_value(receipt, "winner_result_receipt_id")),
        json_string_literal(payload_value(receipt, "winner_principal_review_receipt_id")),
        payload_value(receipt, "winner_total_score")
            .parse::<u32>()
            .unwrap_or(0),
        payload_value(receipt, "human_ratification_required") == "true",
    )
}

#[derive(Clone)]
struct MachineLeagueFleetRunLane {
    worker_id: String,
    lane_index: u64,
    runner_id: String,
    runner_display_name: String,
    model_profile_id: String,
    adapter_protocol_tier: String,
    lane_status: String,
    trial_status: String,
    run_id: String,
    result_receipt_id: String,
    execution_requested: bool,
    execution_allowed: bool,
    execution_performed: bool,
    check_passed: bool,
    blocked_reason: String,
    output_state: String,
}

fn machine_league_lane_from_trial_packet(
    worker_id: String,
    lane_index: u64,
    runner_id: &str,
    runner_display_name: &str,
    model_profile_id: &str,
    adapter_protocol_tier: &str,
    trial_packet: &str,
) -> Result<MachineLeagueFleetRunLane, String> {
    let trial_json: serde_json::Value =
        serde_json::from_str(trial_packet).map_err(|error| error.to_string())?;
    let trial_status = trial_json["status"].as_str().unwrap_or("UNKNOWN");
    let execution_requested = trial_json["adapter_execution_requested"]
        .as_bool()
        .unwrap_or(false);
    let execution_allowed = trial_json["adapter_execution_allowed"]
        .as_bool()
        .unwrap_or(false);
    let execution_performed = trial_json["adapter_execution_performed"]
        .as_bool()
        .unwrap_or(false);
    let check_passed = trial_json["check_passed"].as_bool().unwrap_or(false);
    let blocked_reason = trial_json["blocked_reason"]
        .as_str()
        .or_else(|| trial_json["reason"].as_str())
        .unwrap_or("pending_mdx_quality_gates");
    let lane_status = if trial_status == "REFUSED" {
        "LANE_REFUSED"
    } else if execution_performed {
        "LANE_EXECUTED_QUARANTINED"
    } else if execution_requested && !execution_allowed {
        "LANE_HELD_EXECUTION_GATE_CLOSED"
    } else {
        "LANE_STAGED_HELD_TRIAL_PACKET"
    };
    Ok(MachineLeagueFleetRunLane {
        worker_id,
        lane_index,
        runner_id: runner_id.to_string(),
        runner_display_name: runner_display_name.to_string(),
        model_profile_id: model_profile_id.to_string(),
        adapter_protocol_tier: adapter_protocol_tier.to_string(),
        lane_status: lane_status.to_string(),
        trial_status: trial_status.to_string(),
        run_id: trial_json["run_id"].as_str().unwrap_or("").to_string(),
        result_receipt_id: trial_json["result_receipt_id"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        execution_requested,
        execution_allowed,
        execution_performed,
        check_passed,
        blocked_reason: blocked_reason.to_string(),
        output_state: if execution_performed {
            "output_quarantined_pending_mdx_gates"
        } else {
            "held_without_consumable_output"
        }
        .to_string(),
    })
}

fn machine_league_fleet_run_lane_json(lane: &MachineLeagueFleetRunLane) -> String {
    format!(
        r#"{{"worker_id":{},"lane_index":{},"runner_id":{},"runner_display_name":{},"model_profile_id":{},"adapter_protocol_tier":{},"lane_status":{},"trial_status":{},"run_id":{},"result_receipt_id":{},"execution_requested":{},"execution_allowed":{},"execution_performed":{},"check_passed":{},"blocked_reason":{},"output_state":{},"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(&lane.worker_id),
        lane.lane_index,
        json_string_literal(&lane.runner_id),
        json_string_literal(&lane.runner_display_name),
        json_string_literal(&lane.model_profile_id),
        json_string_literal(&lane.adapter_protocol_tier),
        json_string_literal(&lane.lane_status),
        json_string_literal(&lane.trial_status),
        json_string_literal(&lane.run_id),
        json_string_literal(&lane.result_receipt_id),
        lane.execution_requested,
        lane.execution_allowed,
        lane.execution_performed,
        lane.check_passed,
        json_string_literal(&lane.blocked_reason),
        json_string_literal(&lane.output_state),
    )
}

fn record_machine_league_fleet_run_lane(
    kernel: &mut MdxKernel,
    fleet_run_id: &str,
    lane: &MachineLeagueFleetRunLane,
) -> Result<mdx_core::ForgeRunEventReport, String> {
    let lane_index = lane.lane_index.to_string();
    kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:machine_league_fleet_runner",
                run_id: fleet_run_id,
                event: "evidence_appended",
                work_item_id: "machine_league_fleet_run_lane",
                detail: &format!(
                    "machine_league_fleet_lane status={} runner={}",
                    lane.lane_status, lane.runner_id
                ),
                turn: lane.lane_index as u32,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:machine_league_fleet_runner"),
            &[
                ("machine_league_fleet_run_id", fleet_run_id),
                ("machine_league_fleet_lane", "true"),
                ("worker_id", &lane.worker_id),
                ("lane_index", &lane_index),
                ("runner_id", &lane.runner_id),
                ("runner_display_name", &lane.runner_display_name),
                ("model_profile_id", &lane.model_profile_id),
                ("adapter_protocol_tier", &lane.adapter_protocol_tier),
                ("lane_status", &lane.lane_status),
                ("trial_status", &lane.trial_status),
                ("lane_run_id", &lane.run_id),
                ("result_receipt_id", &lane.result_receipt_id),
                (
                    "adapter_execution_requested",
                    if lane.execution_requested {
                        "true"
                    } else {
                        "false"
                    },
                ),
                (
                    "adapter_execution_allowed",
                    if lane.execution_allowed {
                        "true"
                    } else {
                        "false"
                    },
                ),
                (
                    "adapter_execution_performed",
                    if lane.execution_performed {
                        "true"
                    } else {
                        "false"
                    },
                ),
                (
                    "adapter_check_passed",
                    if lane.check_passed { "true" } else { "false" },
                ),
                ("blocked_reason", &lane.blocked_reason),
                ("output_state", &lane.output_state),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())
}

struct MachineLeagueFleetRunSummary<'a> {
    fleet_run_id: &'a str,
    fleet_status: &'a str,
    cast_plan_id: &'a str,
    cast_receipt_id: &'a str,
    requested_worker_count: u32,
    planned_worker_count: u32,
    execution_requested_count: u32,
    executed_count: u32,
    held_count: u32,
    blocked_count: u32,
    ready_for_review_count: u32,
    task_class: &'a str,
    language_pack_id: &'a str,
    eval_fixture: &'a str,
    integration_status: &'a str,
}

fn record_machine_league_fleet_run_summary(
    kernel: &mut MdxKernel,
    summary: MachineLeagueFleetRunSummary<'_>,
) -> Result<mdx_core::ForgeRunEventReport, String> {
    let requested = summary.requested_worker_count.to_string();
    let planned = summary.planned_worker_count.to_string();
    let requested_execution = summary.execution_requested_count.to_string();
    let executed = summary.executed_count.to_string();
    let held = summary.held_count.to_string();
    let blocked = summary.blocked_count.to_string();
    let ready = summary.ready_for_review_count.to_string();
    kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:machine_league_fleet_runner",
                run_id: summary.fleet_run_id,
                event: "evidence_appended",
                work_item_id: "machine_league_fleet_run",
                detail: &format!(
                    "machine_league_fleet_run status={} planned={} executed={}",
                    summary.fleet_status, summary.planned_worker_count, summary.executed_count
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:machine_league_fleet_runner"),
            &[
                ("machine_league_fleet_run_id", summary.fleet_run_id),
                ("machine_league_fleet_run_summary", "true"),
                ("machine_league_fleet_run_status", summary.fleet_status),
                ("cast_plan_id", summary.cast_plan_id),
                ("fleet_cast_receipt_id", summary.cast_receipt_id),
                ("worker_count_requested", &requested),
                ("worker_count_planned", &planned),
                ("execution_requested_count", &requested_execution),
                ("executed_lane_count", &executed),
                ("held_lane_count", &held),
                ("blocked_lane_count", &blocked),
                ("ready_for_review_count", &ready),
                ("task_class", summary.task_class),
                ("language_pack_id", summary.language_pack_id),
                ("machine_league_eval_fixture", summary.eval_fixture),
                ("integration_status", summary.integration_status),
                ("winner_selection_status", "waiting_for_principal_reviews"),
                ("accepted_for_scoreboard_count", "0"),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())
}

fn record_machine_league_fleet_run_refusal(
    kernel: &mut MdxKernel,
    reason: &str,
    eval_fixture: &str,
) -> Result<String, String> {
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:machine_league_fleet_runner",
                run_id: "machine_league_fleet_run_refusal",
                event: "evidence_appended",
                work_item_id: "machine_league_fleet_run",
                detail: &format!("machine_league_fleet_run_refused reason={reason}"),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:machine_league_fleet_runner"),
            &[
                ("machine_league_fleet_run_status", "refused"),
                ("machine_league_fleet_run_refusal_reason", reason),
                ("machine_league_eval_fixture", eval_fixture),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-fleet-run-local-post","status":"REFUSED","route":"/forge/machine-league/fleet-runs.json","reason":{},"fleet_run_refusal_receipt_id":{},"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(reason),
        json_string_literal(&report.receipt_id),
    ))
}

fn machine_league_fleet_run_summary_json(
    kernel: &MdxKernel,
    receipt: &mdx_core::Receipt,
) -> String {
    let fleet_run_id = payload_value(receipt, "machine_league_fleet_run_id");
    let lanes = kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .into_iter()
        .filter(|lane| {
            payload_value(lane, "machine_league_fleet_lane") == "true"
                && payload_value(lane, "machine_league_fleet_run_id") == fleet_run_id
        })
        .collect::<Vec<_>>();
    let accepted_lane_count = accepted_lanes_for_fleet_run(kernel, &lanes).len();
    let lane_json = lanes
        .iter()
        .map(|lane| machine_league_fleet_run_lane_receipt_json(kernel, lane))
        .collect::<Vec<_>>();
    format!(
        r#"{{"fleet_run_id":{},"fleet_run_receipt_id":{},"status":{},"cast_plan_id":{},"fleet_cast_receipt_id":{},"worker_count_requested":{},"worker_count_planned":{},"execution_requested_count":{},"executed_lane_count":{},"held_lane_count":{},"blocked_lane_count":{},"ready_for_review_count":{},"task_class":{},"language_pack_id":{},"eval_fixture":{},"integration_status":{},"winner_selection_status":{},"accepted_for_scoreboard_count":{},"external_output_consumable":false,"production_write_allowed":false,"lanes":[{}]}}"#,
        json_string_literal(fleet_run_id),
        json_string_literal(&receipt.receipt_id),
        json_string_literal(payload_value(receipt, "machine_league_fleet_run_status")),
        json_string_literal(payload_value(receipt, "cast_plan_id")),
        json_string_literal(payload_value(receipt, "fleet_cast_receipt_id")),
        payload_value(receipt, "worker_count_requested"),
        payload_value(receipt, "worker_count_planned"),
        payload_value(receipt, "execution_requested_count"),
        payload_value(receipt, "executed_lane_count"),
        payload_value(receipt, "held_lane_count"),
        payload_value(receipt, "blocked_lane_count"),
        payload_value(receipt, "ready_for_review_count"),
        json_string_literal(payload_value(receipt, "task_class")),
        json_string_literal(payload_value(receipt, "language_pack_id")),
        json_string_literal(payload_value(receipt, "machine_league_eval_fixture")),
        json_string_literal(payload_value(receipt, "integration_status")),
        json_string_literal(payload_value(receipt, "winner_selection_status")),
        accepted_lane_count,
        lane_json.join(","),
    )
}

fn machine_league_fleet_run_lane_receipt_json(
    kernel: &MdxKernel,
    receipt: &mdx_core::Receipt,
) -> String {
    let accepted_review = latest_accepted_scorecard_receipt_for_lane(
        kernel,
        payload_value(receipt, "lane_run_id"),
        payload_value(receipt, "result_receipt_id"),
    );
    format!(
        r#"{{"lane_receipt_id":{},"worker_id":{},"lane_index":{},"runner_id":{},"runner_display_name":{},"model_profile_id":{},"adapter_protocol_tier":{},"lane_status":{},"trial_status":{},"run_id":{},"result_receipt_id":{},"execution_requested":{},"execution_allowed":{},"execution_performed":{},"check_passed":{},"accepted_for_scoreboard":{},"principal_review_receipt_id":{},"blocked_reason":{},"output_state":{},"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(&receipt.receipt_id),
        json_string_literal(payload_value(receipt, "worker_id")),
        payload_value(receipt, "lane_index"),
        json_string_literal(payload_value(receipt, "runner_id")),
        json_string_literal(payload_value(receipt, "runner_display_name")),
        json_string_literal(payload_value(receipt, "model_profile_id")),
        json_string_literal(payload_value(receipt, "adapter_protocol_tier")),
        json_string_literal(payload_value(receipt, "lane_status")),
        json_string_literal(payload_value(receipt, "trial_status")),
        json_string_literal(payload_value(receipt, "lane_run_id")),
        json_string_literal(payload_value(receipt, "result_receipt_id")),
        payload_value(receipt, "adapter_execution_requested"),
        payload_value(receipt, "adapter_execution_allowed"),
        payload_value(receipt, "adapter_execution_performed"),
        payload_value(receipt, "adapter_check_passed"),
        accepted_review.is_some(),
        json_string_literal(
            accepted_review
                .map(|review| review.receipt_id.as_str())
                .unwrap_or(""),
        ),
        json_string_literal(payload_value(receipt, "blocked_reason")),
        json_string_literal(payload_value(receipt, "output_state")),
    )
}

fn render_machine_league_face_off_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    let task_class = json_string_field(body, "task_class").unwrap_or_else(|| "bug_fix".to_string());
    let language_pack_id =
        json_string_field(body, "language_pack_id").unwrap_or_else(|| "rust-cargo".to_string());
    let eval_fixture = json_string_field(body, "eval_fixture")
        .unwrap_or_else(|| MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID.to_string());
    let requested_runner_id = json_string_field(body, "external_runner_id")
        .unwrap_or_else(|| recommended_face_off_runner_id(kernel, &task_class));
    let Some(runner) = forge_fleet_runner_profiles()
        .into_iter()
        .find(|runner| runner.runner_id == requested_runner_id)
    else {
        return record_machine_league_face_off_refusal(
            kernel,
            "unknown external runner",
            &requested_runner_id,
        );
    };
    if runner.runner_kind == "mdx_native" {
        return record_machine_league_face_off_refusal(
            kernel,
            "face-off external_runner_id must name an outside machine",
            runner.runner_id,
        );
    }
    let model = forge_fleet_model_matrix_profiles()
        .into_iter()
        .find(|profile| profile.profile_id == runner.model_profile_id)
        .unwrap_or(FleetModelMatrixProfile {
            profile_id: runner.model_profile_id,
            provider_family: runner.model_provider,
            model_provider: runner.model_provider,
            example_model: runner.model,
            model_display_name: runner.model,
            wire_api: runner.wire_api,
            auth_policy: "mdx_provider_profile_required",
            redaction_policy: "mdx_context_redaction_required",
            budget_policy: "mdx_runtime_budget_required",
            live_call_allowed: false,
        });
    let adapter_protocol_tier = adapter_protocol_tier_for_runner(Some(kernel), &runner);
    let closed_clearance = closed_runner_clearance_for_runner(Some(kernel), runner.runner_id);
    let clearance_status = closed_clearance
        .as_ref()
        .map(|clearance| {
            if runner.adapter_kind == "grok_build_cli" {
                "open_source_execution_clearance_recorded"
            } else if clearance.scorecard_eligible {
                "cleared_for_fleet_eval"
            } else {
                "cleared_for_local_companion"
            }
        })
        .unwrap_or(if closed_runner_requires_clearance(runner.adapter_kind) {
            "clearance_required"
        } else {
            "clearance_not_required"
        });
    if closed_runner_requires_clearance(runner.adapter_kind)
        && !closed_clearance
            .as_ref()
            .map(|clearance| clearance.scorecard_eligible)
            .unwrap_or(false)
    {
        return record_machine_league_face_off(
            kernel,
            MachineLeagueFaceOffRecord {
                status: "FACE_OFF_WAITING_FOR_CLOSED_RUNNER_CLEARANCE",
                runner,
                model,
                adapter_protocol_tier: &adapter_protocol_tier,
                clearance_status,
                task_class,
                language_pack_id,
                eval_fixture,
                cast_plan_id: "",
                cast_receipt_id: "",
                external_trial_status: "not_started_clearance_required",
                external_run_id: "",
                native_trial_status: "not_started",
                native_run_id: "",
            },
        );
    }

    let policy_profile = json_string_field(body, "policy_profile").unwrap_or_else(|| {
        if closed_clearance
            .as_ref()
            .map(|clearance| clearance.scorecard_eligible)
            .unwrap_or(false)
        {
            "closed_runner_cleared".to_string()
        } else {
            "standard_quarantined".to_string()
        }
    });
    let cast_body = format!(
        r#"{{"worker_count":8,"task_class":{},"language_pack_id":{},"policy_profile":{},"strategy":"compete"}}"#,
        json_string_literal(&task_class),
        json_string_literal(&language_pack_id),
        json_string_literal(&policy_profile),
    );
    let cast_packet = render_machine_league_fleet_casts_json(&cast_body, kernel)?;
    let cast_json: serde_json::Value =
        serde_json::from_str(&cast_packet).map_err(|error| error.to_string())?;
    let stage_external_trial = json_bool_field(body, "stage_external_trial").unwrap_or(true);
    let execute_external_live = json_bool_field(body, "execute_external_live").unwrap_or(false);
    let external_trial_packet = if stage_external_trial {
        let trial_body = format!(
            r#"{{"task_class":{},"language_pack_id":{},"eval_fixture":{},"execute_live":{},"budget_seconds":{}}}"#,
            json_string_literal(&task_class),
            json_string_literal(&language_pack_id),
            json_string_literal(&eval_fixture),
            execute_external_live,
            json_u32_field(body, "budget_seconds").unwrap_or(45),
        );
        Some(render_trial_for_runner(
            runner.runner_id,
            &trial_body,
            kernel,
        )?)
    } else {
        None
    };
    let external_trial_json = external_trial_packet
        .as_ref()
        .and_then(|packet| serde_json::from_str::<serde_json::Value>(packet).ok());
    let run_native_comparison = json_bool_field(body, "run_native_comparison").unwrap_or(false);
    let native_trial_packet = if run_native_comparison {
        let native_body = format!(
            r#"{{"task_class":{},"language_pack_id":{},"eval_fixture":{},"execute_live":true,"budget_seconds":{}}}"#,
            json_string_literal(&task_class),
            json_string_literal(&language_pack_id),
            json_string_literal(&eval_fixture),
            json_u32_field(body, "native_budget_seconds").unwrap_or(120),
        );
        // Face-off composition holds the kernel write guard, so the native
        // comparison runs the scripted baseline (labeled, off-scoreboard).
        Some(render_native_trial_scripted(&native_body, kernel)?)
    } else {
        None
    };
    let native_trial_json = native_trial_packet
        .as_ref()
        .and_then(|packet| serde_json::from_str::<serde_json::Value>(packet).ok());
    record_machine_league_face_off(
        kernel,
        MachineLeagueFaceOffRecord {
            status: if stage_external_trial {
                "FACE_OFF_STAGED_HELD"
            } else {
                "FACE_OFF_READY"
            },
            runner,
            model,
            adapter_protocol_tier: &adapter_protocol_tier,
            clearance_status,
            task_class,
            language_pack_id,
            eval_fixture,
            cast_plan_id: cast_json["plan_id"].as_str().unwrap_or(""),
            cast_receipt_id: cast_json["fleet_cast_receipt_id"].as_str().unwrap_or(""),
            external_trial_status: external_trial_json
                .as_ref()
                .and_then(|packet| packet["status"].as_str())
                .unwrap_or("not_started"),
            external_run_id: external_trial_json
                .as_ref()
                .and_then(|packet| packet["run_id"].as_str())
                .unwrap_or(""),
            native_trial_status: native_trial_json
                .as_ref()
                .and_then(|packet| packet["status"].as_str())
                .unwrap_or("not_started"),
            native_run_id: native_trial_json
                .as_ref()
                .and_then(|packet| packet["run_id"].as_str())
                .unwrap_or(""),
        },
    )
}

struct MachineLeagueFaceOffRecord<'a> {
    status: &'a str,
    runner: FleetRunnerProfile,
    model: FleetModelMatrixProfile,
    adapter_protocol_tier: &'a str,
    clearance_status: &'a str,
    task_class: String,
    language_pack_id: String,
    eval_fixture: String,
    cast_plan_id: &'a str,
    cast_receipt_id: &'a str,
    external_trial_status: &'a str,
    external_run_id: &'a str,
    native_trial_status: &'a str,
    native_run_id: &'a str,
}

fn record_machine_league_face_off(
    kernel: &mut MdxKernel,
    record: MachineLeagueFaceOffRecord<'_>,
) -> Result<String, String> {
    let face_off_seed = format!(
        "{}:{}:{}:{}:{}",
        record.runner.runner_id,
        record.model.profile_id,
        record.task_class,
        record.language_pack_id,
        record.eval_fixture
    );
    let face_off_id = format!(
        "machine_league_face_off_{}",
        &hex(&sha256(face_off_seed.as_bytes()))[..12]
    );
    let native_result_state =
        native_result_state_for_runner(kernel, record.runner.runner_id, &record.eval_fixture);
    let learning_status = learning_status_for_runner(kernel, record.runner.runner_id);
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:machine_league_face_off",
                run_id: &face_off_id,
                event: "evidence_appended",
                work_item_id: "machine_league_face_off",
                detail: &format!(
                    "machine_league_face_off status={} runner={} model={}",
                    record.status, record.runner.runner_id, record.model.profile_id
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:machine_league_face_off"),
            &[
                ("machine_league_face_off_id", &face_off_id),
                ("machine_league_face_off_status", record.status),
                ("runner_id", record.runner.runner_id),
                ("runner_display_name", record.runner.display_name),
                ("runner_adapter_kind", record.runner.adapter_kind),
                ("adapter_protocol_tier", record.adapter_protocol_tier),
                ("model_profile_id", record.model.profile_id),
                ("provider_family", record.model.provider_family),
                ("model_provider", record.model.model_provider),
                ("model_id", record.model.example_model),
                ("wire_api", record.model.wire_api),
                ("closed_runner_clearance_status", record.clearance_status),
                ("task_class", &record.task_class),
                ("language_pack_id", &record.language_pack_id),
                ("machine_league_eval_fixture", &record.eval_fixture),
                ("cast_plan_id", record.cast_plan_id),
                ("fleet_cast_receipt_id", record.cast_receipt_id),
                ("external_trial_status", record.external_trial_status),
                ("external_run_id", record.external_run_id),
                ("native_trial_status", record.native_trial_status),
                ("native_run_id", record.native_run_id),
                ("native_result_state", native_result_state),
                ("learning_status", learning_status),
                ("recommended_machine_visible", "true"),
                ("cast_reason_visible", "true"),
                ("model_profile_visible", "true"),
                ("adapter_protocol_tier_visible", "true"),
                ("output_state", "held_until_mdx_gates_accept"),
                ("external_output_consumable", "false"),
                ("adapter_execution_allowed", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-face-off-local-post","status":{},"route":"/forge/machine-league/face-offs.json","projection_route":"/forge/machine-league/face-offs/projection.json","recommendation_route":"/forge/machine-league/recommendations.json","fleet_cast_route":"/forge/machine-league/fleet-casts.json","trial_route":{},"native_trial_route":"/forge/machine-league/native-trials.json","pairwise_comparison_route":"/forge/machine-league/pairwise-comparisons.json","learning_ledger_route":"/forge/machine-league/learning-ledger.json","clearance_route":"/forge/machine-league/closed-runner-clearances.json","face_off_id":{},"face_off_receipt_id":{},"recommended_machine":{},"runner_id":{},"runner_display_name":{},"model_profile_id":{},"provider_family":{},"model_provider":{},"model_id":{},"wire_api":{},"adapter_protocol_tier":{},"closed_runner_clearance_status":{},"task_class":{},"language_pack_id":{},"eval_fixture":{},"cast_plan_id":{},"fleet_cast_receipt_id":{},"external_trial_status":{},"external_run_id":{},"native_trial_status":{},"native_run_id":{},"native_result_state":{},"learning_status":{},"operator_summary":{},"output_state":"held_until_mdx_gates_accept","accepted_scoreboard_results_only":true,"external_output_consumable":false,"adapter_execution_allowed":false,"production_write_allowed":false}}"#,
        json_string_literal(record.status),
        json_string_literal(trial_route_for_runner(record.runner.runner_id).unwrap_or("")),
        json_string_literal(&face_off_id),
        json_string_literal(&report.receipt_id),
        json_string_literal(record.runner.display_name),
        json_string_literal(record.runner.runner_id),
        json_string_literal(record.runner.display_name),
        json_string_literal(record.model.profile_id),
        json_string_literal(record.model.provider_family),
        json_string_literal(record.model.model_provider),
        json_string_literal(record.model.example_model),
        json_string_literal(record.model.wire_api),
        json_string_literal(record.adapter_protocol_tier),
        json_string_literal(record.clearance_status),
        json_string_literal(&record.task_class),
        json_string_literal(&record.language_pack_id),
        json_string_literal(&record.eval_fixture),
        json_string_literal(record.cast_plan_id),
        json_string_literal(record.cast_receipt_id),
        json_string_literal(record.external_trial_status),
        json_string_literal(record.external_run_id),
        json_string_literal(record.native_trial_status),
        json_string_literal(record.native_run_id),
        json_string_literal(native_result_state),
        json_string_literal(learning_status),
        json_string_literal(&face_off_operator_summary(
            record.runner.display_name,
            record.adapter_protocol_tier,
            native_result_state,
            learning_status,
        )),
    ))
}

fn render_machine_league_face_off_projection_json(kernel: &MdxKernel) -> String {
    let face_offs = kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| !payload_value(receipt, "machine_league_face_off_id").is_empty())
        .rev()
        .map(|receipt| machine_league_face_off_receipt_json(receipt))
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-forge-machine-league-face-off-projection","status":"MACHINE_LEAGUE_FACE_OFF_PROJECTION_READY","route":"/forge/machine-league/face-offs/projection.json","face_off_route":"/forge/machine-league/face-offs.json","recommendation_route":"/forge/machine-league/recommendations.json","fleet_cast_route":"/forge/machine-league/fleet-casts.json","native_trial_route":"/forge/machine-league/native-trials.json","pairwise_comparison_route":"/forge/machine-league/pairwise-comparisons.json","learning_ledger_route":"/forge/machine-league/learning-ledger.json","face_off_count":{},"accepted_scoreboard_results_only":true,"external_output_consumable":false,"production_write_allowed":false,"face_offs":[{}]}}"#,
        face_offs.len(),
        face_offs.join(","),
    )
}

fn machine_league_face_off_receipt_json(receipt: &mdx_core::Receipt) -> String {
    format!(
        r#"{{"face_off_id":{},"face_off_receipt_id":{},"status":{},"runner_id":{},"runner_display_name":{},"model_profile_id":{},"provider_family":{},"model_provider":{},"model_id":{},"wire_api":{},"adapter_protocol_tier":{},"closed_runner_clearance_status":{},"task_class":{},"language_pack_id":{},"eval_fixture":{},"cast_plan_id":{},"fleet_cast_receipt_id":{},"external_trial_status":{},"external_run_id":{},"native_trial_status":{},"native_run_id":{},"native_result_state":{},"learning_status":{},"output_state":{},"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(payload_value(receipt, "machine_league_face_off_id")),
        json_string_literal(&receipt.receipt_id),
        json_string_literal(payload_value(receipt, "machine_league_face_off_status")),
        json_string_literal(payload_value(receipt, "runner_id")),
        json_string_literal(payload_value(receipt, "runner_display_name")),
        json_string_literal(payload_value(receipt, "model_profile_id")),
        json_string_literal(payload_value(receipt, "provider_family")),
        json_string_literal(payload_value(receipt, "model_provider")),
        json_string_literal(payload_value(receipt, "model_id")),
        json_string_literal(payload_value(receipt, "wire_api")),
        json_string_literal(payload_value(receipt, "adapter_protocol_tier")),
        json_string_literal(payload_value(receipt, "closed_runner_clearance_status")),
        json_string_literal(payload_value(receipt, "task_class")),
        json_string_literal(payload_value(receipt, "language_pack_id")),
        json_string_literal(payload_value(receipt, "machine_league_eval_fixture")),
        json_string_literal(payload_value(receipt, "cast_plan_id")),
        json_string_literal(payload_value(receipt, "fleet_cast_receipt_id")),
        json_string_literal(payload_value(receipt, "external_trial_status")),
        json_string_literal(payload_value(receipt, "external_run_id")),
        json_string_literal(payload_value(receipt, "native_trial_status")),
        json_string_literal(payload_value(receipt, "native_run_id")),
        json_string_literal(payload_value(receipt, "native_result_state")),
        json_string_literal(payload_value(receipt, "learning_status")),
        json_string_literal(payload_value(receipt, "output_state")),
    )
}

fn record_machine_league_face_off_refusal(
    kernel: &mut MdxKernel,
    reason: &str,
    runner_id: &str,
) -> Result<String, String> {
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:machine_league_face_off",
                run_id: "machine_league_face_off_refusal",
                event: "evidence_appended",
                work_item_id: "machine_league_face_off",
                detail: &format!("machine_league_face_off_refused reason={reason}"),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:machine_league_face_off"),
            &[
                ("machine_league_face_off_status", "refused"),
                ("machine_league_face_off_refusal_reason", reason),
                ("runner_id", runner_id),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-face-off-local-post","status":"REFUSED","route":"/forge/machine-league/face-offs.json","reason":{},"runner_id":{},"face_off_refusal_receipt_id":{},"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(reason),
        json_string_literal(runner_id),
        json_string_literal(&report.receipt_id),
    ))
}

struct FleetCastPlanReceipt<'a> {
    plan_id: &'a str,
    status: &'a str,
    requested_worker_count: u32,
    planned_worker_count: u32,
    unique_cast_count: u32,
    policy_profile: &'a str,
    task_class: &'a str,
    language_pack_id: &'a str,
    strategy: &'a str,
}

fn record_fleet_cast_plan_receipt(
    kernel: &mut MdxKernel,
    receipt: FleetCastPlanReceipt<'_>,
) -> Result<mdx_core::ForgeRunEventReport, String> {
    let requested = receipt.requested_worker_count.to_string();
    let planned = receipt.planned_worker_count.to_string();
    let unique = receipt.unique_cast_count.to_string();
    kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:machine_league_fleet_cast_planner",
                run_id: receipt.plan_id,
                event: "evidence_appended",
                work_item_id: "machine_league_fleet_cast_plan",
                detail: &format!(
                    "machine_league_fleet_cast status={} requested={} planned={} policy={}",
                    receipt.status,
                    receipt.requested_worker_count,
                    receipt.planned_worker_count,
                    receipt.policy_profile
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:machine_league_fleet_cast_planner"),
            &[
                ("fleet_cast_plan_id", receipt.plan_id),
                ("fleet_cast_status", receipt.status),
                ("fleet_cast_requested_worker_count", &requested),
                ("fleet_cast_planned_worker_count", &planned),
                ("fleet_cast_unique_cast_count", &unique),
                ("fleet_cast_policy_profile", receipt.policy_profile),
                ("fleet_cast_task_class", receipt.task_class),
                ("fleet_cast_language_pack_id", receipt.language_pack_id),
                ("fleet_cast_strategy", receipt.strategy),
                (
                    "fleet_cast_reuse_policy",
                    "cycle_compatible_pairs_with_distinct_worktree_after_unique_casts",
                ),
                ("adapter_execution_allowed", "false"),
                ("task_execution_allowed", "false"),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())
}

fn prioritized_fleet_cast_pairs(
    kernel: Option<&MdxKernel>,
    policy_profile: &str,
) -> Vec<(FleetRunnerProfile, FleetModelMatrixProfile)> {
    let runners = forge_fleet_runner_profiles();
    let models = forge_fleet_model_matrix_profiles();
    let mut pairs = runners
        .iter()
        .flat_map(|runner| {
            models.iter().filter_map(move |model| {
                let compatibility = runner_model_compatibility(kernel, runner, model);
                if compatibility.castable
                    && fleet_policy_allows_cast(kernel, policy_profile, runner, model)
                {
                    Some((*runner, *model))
                } else {
                    None
                }
            })
        })
        .collect::<Vec<_>>();
    pairs.sort_by_key(|(runner, model)| fleet_cast_priority(runner, model));
    pairs
}

fn fleet_cast_priority(runner: &FleetRunnerProfile, model: &FleetModelMatrixProfile) -> u16 {
    match (runner.runner_id, model.profile_id) {
        ("mdx_native_harness_runner", "mdx_native_local_model_profile") => 0,
        ("codex_cli_external_worker", "codex_openai_responses_profile") => 1,
        ("grok_build_cli_external_worker", "xai_grok_build_model_profile") => 2,
        ("claude_code_external_worker", "claude_code_model_profile") => 3,
        ("kimi_code_external_worker", "kimi_code_k3_model_profile") => 4,
        ("gemini_cli_external_worker", "gemini_cli_model_profile") => 5,
        ("opencode_external_worker", "opencode_model_profile") => 6,
        ("cline_external_worker", "cline_model_profile") => 7,
        ("goose_external_worker", "goose_model_profile") => 8,
        ("codex_cli_external_worker", "codex_anthropic_responses_profile") => 9,
        ("codex_cli_external_worker", "codex_gemini_responses_profile") => 10,
        ("codex_cli_external_worker", "codex_xai_responses_profile") => 11,
        ("codex_cli_external_worker", "codex_bedrock_responses_profile") => 12,
        _ => 100,
    }
}

fn fleet_policy_allows_cast(
    kernel: Option<&MdxKernel>,
    policy_profile: &str,
    runner: &FleetRunnerProfile,
    _model: &FleetModelMatrixProfile,
) -> bool {
    match policy_profile {
        "sensitive_local_only" => runner.runner_kind == "mdx_native",
        "open_source_first" => {
            !runner.execution_status.contains("TOS_AUTH")
                && !closed_runner_requires_clearance(runner.adapter_kind)
        }
        "standard_quarantined" | "closed_runner_cleared" | "" => {
            if closed_runner_requires_clearance(runner.adapter_kind) {
                closed_runner_scorecard_enabled(kernel, runner)
            } else {
                !runner.execution_status.contains("TOS_AUTH")
            }
        }
        _ => false,
    }
}

fn planned_worker_casts_json(
    kernel: Option<&MdxKernel>,
    compatible: &[(FleetRunnerProfile, FleetModelMatrixProfile)],
    requested_worker_count: u32,
    policy_profile: &str,
) -> String {
    let native_adaptations = kernel
        .map(NativeRuntimeAdaptationProfile::from_kernel)
        .unwrap_or_default();
    (0..requested_worker_count)
        .map(|index| {
            let (runner, model) = compatible[index as usize % compatible.len()];
            let compatibility = runner_model_compatibility(kernel, &runner, &model);
            let native_runtime_adaptation_applied =
                runner.runner_id == "mdx_native_harness_runner" && native_adaptations.count > 0;
            let native_runtime_adaptation_status = if runner.runner_id == "mdx_native_harness_runner" {
                native_adaptations.status()
            } else {
                "not_native_lane"
            };
            let native_runtime_adaptation_count = if runner.runner_id == "mdx_native_harness_runner" {
                native_adaptations.count
            } else {
                0
            };
            let native_runtime_adaptation_grant_receipt_ids =
                if runner.runner_id == "mdx_native_harness_runner" {
                    native_adaptations.grant_receipt_ids.as_str()
                } else {
                    ""
                };
            format!(
                r#"{{"worker_id":{},"lane_index":{},"reuse_index":{},"runner_id":{},"runner_display_name":{},"runner_adapter_kind":{},"adapter_protocol_tier":{},"model_profile_id":{},"provider_family":{},"model_provider":{},"model_id":{},"model_display_name":{},"wire_api":{},"compatibility_status":{},"compatibility_reason":{},"policy_profile":{},"worktree_policy":"distinct_isolated_worktree","transcript_policy":{},"native_runtime_adaptation_applied":{},"native_runtime_adaptation_status":{},"native_runtime_adaptation_count":{},"native_runtime_adaptation_grant_receipt_ids":{},"output_quarantined":true,"adapter_execution_allowed":false,"task_execution_allowed":false,"external_output_consumable":false,"production_write_allowed":false}}"#,
                json_string_literal(&format!("fleet_cast_{:02}", index + 1)),
                index + 1,
                index as usize / compatible.len(),
                json_string_literal(runner.runner_id),
                json_string_literal(runner.display_name),
                json_string_literal(runner.adapter_kind),
                json_string_literal(&adapter_protocol_tier_for_runner(kernel, &runner)),
                json_string_literal(model.profile_id),
                json_string_literal(model.provider_family),
                json_string_literal(model.model_provider),
                json_string_literal(model.example_model),
                json_string_literal(model.model_display_name),
                json_string_literal(model.wire_api),
                json_string_literal(compatibility.status),
                json_string_literal(compatibility.reason),
                json_string_literal(policy_profile),
                json_string_literal(runner.transcript_policy),
                native_runtime_adaptation_applied,
                json_string_literal(native_runtime_adaptation_status),
                native_runtime_adaptation_count,
                json_string_literal(native_runtime_adaptation_grant_receipt_ids),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn excluded_fleet_runner_profiles_json(kernel: Option<&MdxKernel>, policy_profile: &str) -> String {
    let models = forge_fleet_model_matrix_profiles();
    forge_fleet_runner_profiles()
        .iter()
        .filter_map(|runner| {
            let has_policy_allowed_cast = models.iter().any(|model| {
                let compatibility = runner_model_compatibility(kernel, runner, model);
                compatibility.castable
                    && fleet_policy_allows_cast(kernel, policy_profile, runner, model)
            });
            if has_policy_allowed_cast {
                return None;
            }
            let reason = if runner.adapter_kind == "grok_build_cli"
                && !machine_option_enabled_by_provider_key(global(), &current_tenant_id(), runner.adapter_kind)
            {
                "blocked_pending_xai_key"
            } else if runner.adapter_kind == "claude_code"
                && !machine_option_enabled_by_provider_key(global(), &current_tenant_id(), runner.adapter_kind)
            {
                "blocked_pending_anthropic_key"
            } else if runner.execution_status.contains("TOS_AUTH") {
                "blocked_pending_tos_auth_clearance"
            } else if policy_profile == "sensitive_local_only" && runner.runner_kind != "mdx_native"
            {
                "blocked_by_sensitive_local_only_policy"
            } else if !models
                .iter()
                .any(|model| runner_model_compatibility(kernel, runner, model).castable)
            {
                "no_admitted_compatible_model_profile"
            } else {
                "blocked_by_policy_profile"
            };
            Some(format!(
                r#"{{"runner_id":{},"display_name":{},"adapter_kind":{},"execution_status":{},"excluded_reason":{}}}"#,
                json_string_literal(runner.runner_id),
                json_string_literal(runner.display_name),
                json_string_literal(runner.adapter_kind),
                json_string_literal(runner.execution_status),
                json_string_literal(reason),
            ))
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn render_runtime_smoke_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let runner_id = json_string_field(body, "runner_id").unwrap_or_default();
    let adapter_kind = json_string_field(body, "adapter_kind").unwrap_or_default();
    let runners = forge_fleet_runner_profiles();
    let runner = runners
        .iter()
        .find(|runner| {
            (!runner_id.trim().is_empty() && runner.runner_id == runner_id.trim())
                || (!adapter_kind.trim().is_empty() && runner.adapter_kind == adapter_kind.trim())
        })
        .copied();
    let Some(runner) = runner else {
        return record_runtime_smoke_refusal(
            kernel,
            "unknown runner for machine runtime smoke",
            runner_id.trim(),
            adapter_kind.trim(),
            "",
        );
    };
    if runner.runner_kind == "mdx_native" {
        return record_runtime_smoke_refusal(
            kernel,
            "native runtime does not need an external machine smoke",
            runner.runner_id,
            runner.adapter_kind,
            "",
        );
    }
    let runtime = observe_machine_runtime_fingerprint(
        Some(kernel),
        runner.runner_id,
        runner.adapter_kind,
        adapter_binary_name(runner.adapter_kind),
        adapter_command_contract(runner.adapter_kind),
    );
    let smoke_status = if runtime.binary_present {
        "runtime_smoke_passed"
    } else {
        "runtime_smoke_blocked_binary_missing"
    };
    let compatibility_status = if runtime.binary_present {
        "version_command_observed"
    } else {
        "binary_missing"
    };
    let smoke_passed = runtime.binary_present;
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:machine_runtime_smoke",
                run_id: "forge_machine_runtime_smoke",
                event: "evidence_appended",
                work_item_id: "forge_machine_runtime_smoke",
                detail: &format!(
                    "machine_runtime_smoke status={} runner_id={} fingerprint_id={}",
                    smoke_status, runtime.runner_id, runtime.fingerprint_id
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:machine_runtime_smoke"),
            &[
                ("machine_runtime_smoke_status", smoke_status),
                (
                    "machine_runtime_smoke_passed",
                    if smoke_passed { "true" } else { "false" },
                ),
                ("machine_runtime_fingerprint_id", &runtime.fingerprint_id),
                ("machine_runtime_runner_id", &runtime.runner_id),
                ("machine_runtime_adapter_kind", &runtime.adapter_kind),
                ("machine_runtime_binary_name", &runtime.binary_name),
                ("machine_runtime_binary_path", &runtime.binary_path),
                (
                    "machine_runtime_binary_present",
                    if runtime.binary_present {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("machine_runtime_version_command", &runtime.version_command),
                (
                    "machine_runtime_version_observed",
                    &runtime.version_observed,
                ),
                (
                    "machine_runtime_version_raw_output",
                    &runtime.version_raw_output,
                ),
                ("machine_runtime_checksum_sha256", &runtime.checksum_sha256),
                (
                    "machine_runtime_adapter_contract_version",
                    &runtime.adapter_contract_version,
                ),
                (
                    "machine_runtime_command_contract",
                    &runtime.command_contract,
                ),
                ("machine_runtime_drift_status", &runtime.drift_status),
                (
                    "machine_runtime_last_fingerprint_id",
                    &runtime.last_fingerprint_id,
                ),
                ("machine_runtime_compatibility_status", compatibility_status),
                ("adapter_execution_performed", "false"),
                ("task_execution_allowed", "false"),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-runtime-smoke-local-post","status":{},"route":"/forge/machine-league/runtime-smokes.json","runner_id":{},"runner_display_name":{},"runtime_fingerprint":{},"smoke_receipt_id":{},"machine_runtime_smoke_status":{},"machine_runtime_smoke_passed":{},"adapter_execution_performed":false,"task_execution_allowed":false,"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(if smoke_passed {
            "RUNTIME_SMOKE_PASSED"
        } else {
            "RUNTIME_SMOKE_HELD"
        }),
        json_string_literal(runner.runner_id),
        json_string_literal(runner.display_name),
        machine_runtime_json(&runtime),
        json_string_literal(&report.receipt_id),
        json_string_literal(smoke_status),
        smoke_passed,
    ))
}

fn record_runtime_smoke_refusal(
    kernel: &mut MdxKernel,
    reason: &str,
    runner_id: &str,
    adapter_kind: &str,
    fingerprint_id: &str,
) -> Result<String, String> {
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:machine_runtime_smoke",
                run_id: "forge_machine_runtime_smoke_refusal",
                event: "evidence_appended",
                work_item_id: "forge_machine_runtime_smoke",
                detail: &format!("machine_runtime_smoke_refused reason={reason}"),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:machine_runtime_smoke"),
            &[
                ("machine_runtime_smoke_status", "runtime_smoke_refused"),
                ("machine_runtime_smoke_passed", "false"),
                ("machine_runtime_smoke_refusal_reason", reason),
                ("machine_runtime_runner_id", runner_id),
                ("machine_runtime_adapter_kind", adapter_kind),
                ("machine_runtime_fingerprint_id", fingerprint_id),
                ("adapter_execution_performed", "false"),
                ("task_execution_allowed", "false"),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-runtime-smoke-local-post","status":"REFUSED","route":"/forge/machine-league/runtime-smokes.json","reason":{},"runner_id":{},"adapter_kind":{},"smoke_receipt_id":{},"machine_runtime_smoke_passed":false,"adapter_execution_performed":false,"task_execution_allowed":false,"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(reason),
        json_string_literal(runner_id),
        json_string_literal(adapter_kind),
        json_string_literal(&report.receipt_id),
    ))
}

fn machine_league_runner_tiebreak_rank(runner_id: &str) -> u8 {
    match runner_id {
        "codex_cli_external_worker" => 0,
        "gemini_cli_external_worker" => 1,
        "opencode_external_worker" => 2,
        "cline_external_worker" => 3,
        "goose_external_worker" => 4,
        "kimi_code_external_worker" => 5,
        _ => 9,
    }
}

fn open_machine_trial_spec(adapter_slug: &str) -> OpenMachineTrialSpec {
    match adapter_slug {
        "opencode" => OpenMachineTrialSpec {
            adapter_slug: "opencode",
            runner_id: "opencode_external_worker",
            display_name: "OpenCode",
            adapter_kind: "opencode",
            model_provider: "opencode_configured_provider",
            model_id: "opencode-selected-model",
            model_profile_id: "opencode_model_profile",
            actor_id: "agent:opencode",
            work_item_id: "machine_league_opencode_trial",
            route_name: "mdx-forge-machine-league-opencode-trial-local-post",
            status_prefix: "OPENCODE_CLI",
            artifact_slug: "opencode",
            transcript_name: "opencode.json",
            binary_name: "opencode",
            live_env_key: "MDX_MACHINE_LEAGUE_OPENCODE_EXEC",
            execution_marker: OPENCODE_EXEC_POLICY_MARKER,
            execution_command: OPENCODE_EXEC_POLICY_COMMAND,
        },
        "cline" => OpenMachineTrialSpec {
            adapter_slug: "cline",
            runner_id: "cline_external_worker",
            display_name: "Cline",
            adapter_kind: "cline",
            model_provider: "cline_configured_provider",
            model_id: "cline-selected-model",
            model_profile_id: "cline_model_profile",
            actor_id: "agent:cline",
            work_item_id: "machine_league_cline_trial",
            route_name: "mdx-forge-machine-league-cline-trial-local-post",
            status_prefix: "CLINE",
            artifact_slug: "cline",
            transcript_name: "cline.json",
            binary_name: "cline",
            live_env_key: "MDX_MACHINE_LEAGUE_CLINE_EXEC",
            execution_marker: CLINE_EXEC_POLICY_MARKER,
            execution_command: CLINE_EXEC_POLICY_COMMAND,
        },
        "goose" => OpenMachineTrialSpec {
            adapter_slug: "goose",
            runner_id: "goose_external_worker",
            display_name: "Goose",
            adapter_kind: "goose",
            model_provider: "goose_configured_provider",
            model_id: "goose-selected-model",
            model_profile_id: "goose_model_profile",
            actor_id: "agent:goose",
            work_item_id: "machine_league_goose_trial",
            route_name: "mdx-forge-machine-league-goose-trial-local-post",
            status_prefix: "GOOSE_CLI",
            artifact_slug: "goose",
            transcript_name: "goose.stream-json",
            binary_name: "goose",
            live_env_key: "MDX_MACHINE_LEAGUE_GOOSE_EXEC",
            execution_marker: GOOSE_EXEC_POLICY_MARKER,
            execution_command: GOOSE_EXEC_POLICY_COMMAND,
        },
        "grok_build" => OpenMachineTrialSpec {
            adapter_slug: "grok_build",
            runner_id: "grok_build_cli_external_worker",
            display_name: "Grok Build",
            adapter_kind: "grok_build_cli",
            model_provider: "xai_grok_build_cli",
            model_id: "grok-4.5",
            model_profile_id: "xai_grok_build_model_profile",
            actor_id: "agent:grok_build",
            work_item_id: "machine_league_grok_build_trial",
            route_name: "mdx-forge-machine-league-grok-build-trial-local-post",
            status_prefix: "GROK_BUILD_CLI",
            artifact_slug: "grok-build",
            transcript_name: "grok-build.streaming-json",
            binary_name: "grok",
            live_env_key: "MDX_MACHINE_LEAGUE_GROK_BUILD_EXEC",
            execution_marker: GROK_BUILD_EXEC_POLICY_MARKER,
            execution_command: GROK_BUILD_EXEC_POLICY_COMMAND,
        },
        "claude_code" => OpenMachineTrialSpec {
            adapter_slug: "claude_code",
            runner_id: "claude_code_external_worker",
            display_name: "Claude Agent SDK",
            adapter_kind: "claude_code",
            model_provider: "anthropic_claude_agent_sdk",
            model_id: "claude-agent-sdk-selected-model",
            model_profile_id: "claude_code_model_profile",
            actor_id: "agent:claude_code",
            work_item_id: "machine_league_claude_code_trial",
            route_name: "mdx-forge-machine-league-claude-code-trial-local-post",
            status_prefix: "CLAUDE_CODE",
            artifact_slug: "claude-code",
            transcript_name: "claude-code.stream-json",
            binary_name: "claude",
            live_env_key: "MDX_MACHINE_LEAGUE_CLAUDE_CODE_EXEC",
            execution_marker: CLAUDE_CODE_EXEC_POLICY_MARKER,
            execution_command: CLAUDE_CODE_EXEC_POLICY_COMMAND,
        },
        "kimi_code" => OpenMachineTrialSpec {
            adapter_slug: "kimi_code",
            runner_id: "kimi_code_external_worker",
            display_name: "Kimi Code CLI",
            adapter_kind: "kimi_code",
            model_provider: "moonshot_kimi_code_cli",
            model_id: "kimi-k3",
            model_profile_id: "kimi_code_k3_model_profile",
            actor_id: "agent:kimi_code",
            work_item_id: "machine_league_kimi_code_trial",
            route_name: "mdx-forge-machine-league-kimi-code-trial-local-post",
            status_prefix: "KIMI_CODE_CLI",
            artifact_slug: "kimi-code",
            transcript_name: "kimi-code.stream-json",
            binary_name: "kimi",
            live_env_key: "MDX_MACHINE_LEAGUE_KIMI_CODE_EXEC",
            execution_marker: KIMI_CODE_EXEC_POLICY_MARKER,
            execution_command: KIMI_CODE_EXEC_POLICY_COMMAND,
        },
        _ => open_machine_trial_spec("opencode"),
    }
}

fn machine_league_trial_routes_json() -> String {
    [
        (
            "codex_cli_external_worker",
            "/forge/machine-league/codex-trials.json",
        ),
        (
            "gemini_cli_external_worker",
            "/forge/machine-league/gemini-trials.json",
        ),
        (
            "grok_build_cli_external_worker",
            "/forge/machine-league/grok-build-trials.json",
        ),
        (
            "claude_code_external_worker",
            "/forge/machine-league/claude-code-trials.json",
        ),
        (
            "kimi_code_external_worker",
            "/forge/machine-league/kimi-code-trials.json",
        ),
        (
            "opencode_external_worker",
            "/forge/machine-league/opencode-trials.json",
        ),
        (
            "cline_external_worker",
            "/forge/machine-league/cline-trials.json",
        ),
        (
            "goose_external_worker",
            "/forge/machine-league/goose-trials.json",
        ),
        (
            "mdx_native_harness_runner",
            "/forge/machine-league/native-trials.json",
        ),
    ]
    .iter()
    .map(|(runner_id, route)| {
        format!(
            "{}:{}",
            json_string_literal(runner_id),
            json_string_literal(route)
        )
    })
    .collect::<Vec<_>>()
    .join(",")
}

fn trial_route_for_runner(runner_id: &str) -> Option<&'static str> {
    match runner_id {
        "codex_cli_external_worker" => Some("/forge/machine-league/codex-trials.json"),
        "gemini_cli_external_worker" => Some("/forge/machine-league/gemini-trials.json"),
        "grok_build_cli_external_worker" => Some("/forge/machine-league/grok-build-trials.json"),
        "claude_code_external_worker" => Some("/forge/machine-league/claude-code-trials.json"),
        "kimi_code_external_worker" => Some("/forge/machine-league/kimi-code-trials.json"),
        "opencode_external_worker" => Some("/forge/machine-league/opencode-trials.json"),
        "cline_external_worker" => Some("/forge/machine-league/cline-trials.json"),
        "goose_external_worker" => Some("/forge/machine-league/goose-trials.json"),
        "mdx_native_harness_runner" => Some("/forge/machine-league/native-trials.json"),
        _ => None,
    }
}

fn render_trial_for_runner(
    runner_id: &str,
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    match runner_id {
        "codex_cli_external_worker" => render_codex_trial_json(body, kernel),
        "gemini_cli_external_worker" => render_gemini_trial_json(body, kernel),
        "grok_build_cli_external_worker" => {
            render_open_machine_trial_json(body, kernel, open_machine_trial_spec("grok_build"))
        }
        "claude_code_external_worker" => {
            render_open_machine_trial_json(body, kernel, open_machine_trial_spec("claude_code"))
        }
        "kimi_code_external_worker" => {
            render_open_machine_trial_json(body, kernel, open_machine_trial_spec("kimi_code"))
        }
        "opencode_external_worker" => {
            render_open_machine_trial_json(body, kernel, open_machine_trial_spec("opencode"))
        }
        "cline_external_worker" => {
            render_open_machine_trial_json(body, kernel, open_machine_trial_spec("cline"))
        }
        "goose_external_worker" => {
            render_open_machine_trial_json(body, kernel, open_machine_trial_spec("goose"))
        }
        _ => Err(format!(
            "runner {runner_id} has no Machine League trial route"
        )),
    }
}

fn recommended_face_off_runner_id(kernel: &MdxKernel, task_class: &str) -> String {
    forge_fleet_runner_profiles()
        .into_iter()
        .filter(|runner| runner.runner_kind != "mdx_native")
        .filter(|runner| {
            !closed_runner_requires_clearance(runner.adapter_kind)
                || closed_runner_clearance_for_runner(Some(kernel), runner.runner_id)
                    .map(|clearance| clearance.scorecard_eligible)
                    .unwrap_or(false)
        })
        .max_by_key(|runner| {
            (
                accepted_score_count_for_runner_and_task_class(
                    kernel,
                    runner.runner_id,
                    task_class,
                ),
                std::cmp::Reverse(machine_league_runner_tiebreak_rank(runner.runner_id)),
            )
        })
        .map(|runner| runner.runner_id.to_string())
        .unwrap_or_else(|| "codex_cli_external_worker".to_string())
}

fn native_result_state_for_runner(
    kernel: &MdxKernel,
    runner_id: &str,
    eval_fixture: &str,
) -> &'static str {
    if let Some(outcome) = latest_pairwise_outcome_for_runner(kernel, runner_id, eval_fixture) {
        return outcome;
    }
    let external_count =
        accepted_score_count_for_runner_and_fixture(kernel, runner_id, eval_fixture);
    let native_count = accepted_score_count_for_runner_and_fixture(
        kernel,
        "mdx_native_harness_runner",
        eval_fixture,
    );
    if external_count > 0 && native_count > 0 {
        "pairwise_ready_pending_review"
    } else if external_count > 0 {
        "native_comparison_not_run"
    } else {
        "not_enough_accepted_evidence"
    }
}

fn latest_pairwise_outcome_for_runner(
    kernel: &MdxKernel,
    runner_id: &str,
    eval_fixture: &str,
) -> Option<&'static str> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
        .find(|receipt| {
            payload_value(receipt, "machine_league_pairwise_status") == "recorded"
                && payload_value(receipt, "external_runner_id") == runner_id
                && payload_value(receipt, "machine_league_eval_fixture") == eval_fixture
        })
        .and_then(|receipt| match payload_value(receipt, "pairwise_outcome") {
            "external_win" => Some("external_won"),
            "native_win" => Some("native_won"),
            "tie" => Some("native_tied"),
            _ => None,
        })
}

fn learning_status_for_runner(kernel: &MdxKernel, runner_id: &str) -> &'static str {
    if kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .any(|receipt| {
            payload_value(receipt, "winning_runner_id") == runner_id
                && !payload_value(receipt, "machine_league_distillation_note_id").is_empty()
                && !payload_value(receipt, "distillation_evidence_gate").is_empty()
        })
    {
        "recorded_learning"
    } else if accepted_score_count_for_runner(kernel, runner_id) > 0 {
        "accepted_evidence_pending_distillation_decision"
    } else {
        "nothing_learned_yet"
    }
}

fn face_off_operator_summary(
    runner_display_name: &str,
    adapter_protocol_tier: &str,
    native_result_state: &str,
    learning_status: &str,
) -> String {
    format!(
        "Forge picked {runner_display_name}, shows its {adapter_protocol_tier} path, keeps output held, reports native state as {native_result_state}, and learning state as {learning_status}."
    )
}

fn eval_fixture_snapshot_id(eval_fixture: &str) -> &'static str {
    if let Some(fixture) = machine_league_eval_fixture(eval_fixture) {
        fixture.snapshot_id
    } else if eval_fixture.trim().is_empty() {
        "not_machine_league_fixture"
    } else {
        "unknown_fixture_snapshot"
    }
}

fn contamination_status_for_eval_fixture(eval_fixture: &str) -> &'static str {
    if machine_league_eval_fixture(eval_fixture).is_some() {
        "fresh_held_out_fixture"
    } else if eval_fixture.trim().is_empty() {
        "not_machine_league_fixture"
    } else {
        "uncontrolled_fixture"
    }
}

fn machine_league_eval_fixture(eval_fixture: &str) -> Option<MachineLeagueEvalFixture> {
    MACHINE_LEAGUE_EVAL_FIXTURES
        .iter()
        .copied()
        .find(|fixture| fixture.id == eval_fixture.trim())
}

fn machine_league_eval_fixture_or_default(eval_fixture: &str) -> MachineLeagueEvalFixture {
    machine_league_eval_fixture(eval_fixture)
        .or_else(|| machine_league_eval_fixture(MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID))
        .expect("default Machine League fixture exists")
}

fn is_machine_league_fixture(eval_fixture: &str) -> bool {
    // Read the allowlist from the generated registry, the versioned source of
    // truth. fixture-registry-check keeps these ids exactly equal to the
    // MACHINE_LEAGUE_EVAL_FIXTURES config const, so this is as strict as before.
    MACHINE_LEAGUE_FIXTURE_REGISTRY_IDS.contains(&eval_fixture.trim())
}

fn fixture_known_or_empty(eval_fixture: &str) -> bool {
    eval_fixture.trim().is_empty() || is_machine_league_fixture(eval_fixture)
}

fn eval_oracle_status(
    visible_check_passed: bool,
    hidden_check_observed: bool,
    hidden_check_passed: bool,
) -> &'static str {
    if visible_check_passed && hidden_check_observed && hidden_check_passed {
        "visible_and_hidden_checks_passed"
    } else if !visible_check_passed {
        "visible_check_failed"
    } else if !hidden_check_observed {
        "hidden_check_not_observed"
    } else {
        "hidden_check_failed"
    }
}

fn requested_execution_backend(body: &str) -> ForgeExecutionBackend {
    let explicit_backend = json_string_field(body, "execution_backend")
        .or_else(|| json_string_field(body, "execution_backend_kind"))
        .unwrap_or_default();
    let execution_mode = json_string_field(body, "execution_mode").unwrap_or_default();
    let backend_kind = if explicit_backend.trim().is_empty()
        && (execution_mode.starts_with("hosted_")
            || execution_mode.starts_with("cloud_")
            || execution_mode == "aws_sandbox")
    {
        ForgeExecutionBackendKind::HostedSandbox
    } else {
        ForgeExecutionBackendKind::from_request(&explicit_backend)
    };
    ForgeExecutionBackend::new(backend_kind)
}

struct CodexLiveTrialArtifacts {
    execution_requested: bool,
    execution_allowed: bool,
    execution_performed: bool,
    execution_status: &'static str,
    runner_execution_mode: &'static str,
    tool_detail: String,
    evidence_detail: String,
    finish_detail: String,
    worktree_path: String,
    output_artifact_ref: String,
    output_artifact_sha256: String,
    transcript_ref: String,
    pr_handoff_ref: String,
    claimed_check: String,
    artifact_filter_summary: String,
    principal_review_gate_results: String,
    diff_language_pack_impact: String,
    codex_exit_code: i32,
    codex_timed_out: bool,
    check_exit_code: i32,
    check_timed_out: bool,
    check_passed: bool,
    visible_check_passed: bool,
    hidden_check_observed: bool,
    hidden_check_passed: bool,
    changed_file_count: u32,
}

struct GeminiLiveTrialArtifacts {
    execution_requested: bool,
    execution_allowed: bool,
    execution_performed: bool,
    execution_status: &'static str,
    runner_execution_mode: &'static str,
    tool_detail: String,
    evidence_detail: String,
    finish_detail: String,
    worktree_path: String,
    output_artifact_ref: String,
    output_artifact_sha256: String,
    transcript_ref: String,
    pr_handoff_ref: String,
    claimed_check: String,
    artifact_filter_summary: String,
    principal_review_gate_results: String,
    diff_language_pack_impact: String,
    gemini_exit_code: i32,
    gemini_timed_out: bool,
    gemini_auth_blocked: bool,
    check_exit_code: i32,
    check_timed_out: bool,
    check_passed: bool,
    visible_check_passed: bool,
    hidden_check_observed: bool,
    hidden_check_passed: bool,
    changed_file_count: u32,
}

#[derive(Clone, Copy)]
struct OpenMachineTrialSpec {
    adapter_slug: &'static str,
    runner_id: &'static str,
    display_name: &'static str,
    adapter_kind: &'static str,
    model_provider: &'static str,
    model_id: &'static str,
    model_profile_id: &'static str,
    actor_id: &'static str,
    work_item_id: &'static str,
    route_name: &'static str,
    status_prefix: &'static str,
    artifact_slug: &'static str,
    transcript_name: &'static str,
    binary_name: &'static str,
    live_env_key: &'static str,
    execution_marker: &'static str,
    execution_command: &'static str,
}

struct OpenMachineLiveTrialArtifacts {
    execution_requested: bool,
    execution_allowed: bool,
    execution_performed: bool,
    execution_status: String,
    runner_execution_mode: String,
    tool_detail: String,
    evidence_detail: String,
    finish_detail: String,
    worktree_path: String,
    output_artifact_ref: String,
    output_artifact_sha256: String,
    transcript_ref: String,
    pr_handoff_ref: String,
    claimed_check: String,
    artifact_filter_summary: String,
    principal_review_gate_results: String,
    diff_language_pack_impact: String,
    runner_exit_code: i32,
    runner_timed_out: bool,
    runner_auth_blocked: bool,
    check_exit_code: i32,
    check_timed_out: bool,
    check_passed: bool,
    visible_check_passed: bool,
    hidden_check_observed: bool,
    hidden_check_passed: bool,
    changed_file_count: u32,
}

struct MachineRuntimeFingerprint {
    runner_id: String,
    adapter_kind: String,
    binary_name: String,
    binary_path: String,
    binary_present: bool,
    version_command: String,
    version_observed: String,
    version_raw_output: String,
    checksum_sha256: String,
    adapter_contract_version: String,
    command_contract: String,
    fingerprint_id: String,
    drift_status: String,
    last_fingerprint_id: String,
    compatibility_status: String,
}

struct NativeLiveTrialArtifacts {
    execution_performed: bool,
    execution_status: &'static str,
    tool_detail: String,
    evidence_detail: String,
    finish_detail: String,
    worktree_path: String,
    output_artifact_ref: String,
    output_artifact_sha256: String,
    transcript_ref: String,
    pr_handoff_ref: String,
    claimed_check: String,
    artifact_filter_summary: String,
    principal_review_gate_results: String,
    diff_language_pack_impact: String,
    native_exit_code: i32,
    native_timed_out: bool,
    check_exit_code: i32,
    check_timed_out: bool,
    check_passed: bool,
    visible_check_passed: bool,
    hidden_check_observed: bool,
    hidden_check_passed: bool,
    changed_file_count: u32,
}

impl GeminiLiveTrialArtifacts {
    fn held(
        output_artifact_ref: String,
        transcript_ref: String,
        pr_handoff_ref: String,
        language_task: &ForgeLanguageTaskCorpusEntry,
        execution_requested: bool,
    ) -> Self {
        Self {
            execution_requested,
            execution_allowed: false,
            execution_performed: false,
            execution_status: if execution_requested {
                "LIVE_EXECUTION_REQUESTED_BUT_DISABLED"
            } else {
                "HELD_TRIAL_PACKET_RECORDED"
            },
            runner_execution_mode: "quarantined_trial_packet_recorded",
            tool_detail: if execution_requested {
                "external_machine_adapter gemini_cli execution_disabled_by_env trial_packet_recorded"
                    .to_string()
            } else {
                "external_machine_adapter gemini_cli execution_denied trial_packet_recorded"
                    .to_string()
            },
            evidence_detail: "machine_league_gemini_work_packet_recorded output_quarantined=true adapter_execution_performed=false".to_string(),
            finish_detail:
                "status=RUN_FINISHED_DONE machine_league_gemini_trial_packet_recorded output_quarantined=true files_changed=0"
                    .to_string(),
            worktree_path: String::new(),
            output_artifact_sha256: format!(
                "sha256:{}",
                hex(&sha256(output_artifact_ref.as_bytes()))
            ),
            output_artifact_ref,
            transcript_ref,
            pr_handoff_ref,
            claimed_check: language_task.visible_check.to_string(),
            artifact_filter_summary: "trial_packet_recorded_no_external_artifacts_consumed"
                .to_string(),
            principal_review_gate_results: pending_principal_review_gate_results(
                language_task.required_principal_review_gates,
            ),
            diff_language_pack_impact: language_task.language_pack_id.to_string(),
            gemini_exit_code: -1,
            gemini_timed_out: false,
            gemini_auth_blocked: false,
            check_exit_code: -1,
            check_timed_out: false,
            check_passed: false,
            visible_check_passed: false,
            hidden_check_observed: false,
            hidden_check_passed: false,
            changed_file_count: 0,
        }
    }
}

impl OpenMachineLiveTrialArtifacts {
    fn held(
        spec: OpenMachineTrialSpec,
        output_artifact_ref: String,
        transcript_ref: String,
        pr_handoff_ref: String,
        language_task: &ForgeLanguageTaskCorpusEntry,
        execution_requested: bool,
        binary_available: bool,
    ) -> Self {
        let blocked_reason = if execution_requested && !binary_available {
            "binary_missing"
        } else if execution_requested {
            "execution_disabled_by_env"
        } else {
            "execution_denied"
        };
        Self {
            execution_requested,
            execution_allowed: false,
            execution_performed: false,
            execution_status: if execution_requested {
                format!("{}_LIVE_EXECUTION_REQUESTED_BUT_HELD", spec.status_prefix)
            } else {
                format!("{}_TRIAL_PACKET_RECORDED", spec.status_prefix)
            },
            runner_execution_mode: "quarantined_trial_packet_recorded".to_string(),
            tool_detail: format!(
                "external_machine_adapter {} {blocked_reason} trial_packet_recorded",
                spec.adapter_kind
            ),
            evidence_detail: format!(
                "machine_league_{}_work_packet_recorded output_quarantined=true adapter_execution_performed=false",
                spec.adapter_slug
            ),
            finish_detail: format!(
                "status=RUN_FINISHED_DONE machine_league_{}_trial_packet_recorded output_quarantined=true files_changed=0",
                spec.adapter_slug
            ),
            worktree_path: String::new(),
            output_artifact_sha256: format!(
                "sha256:{}",
                hex(&sha256(output_artifact_ref.as_bytes()))
            ),
            output_artifact_ref,
            transcript_ref,
            pr_handoff_ref,
            claimed_check: language_task.visible_check.to_string(),
            artifact_filter_summary: "trial_packet_recorded_no_external_artifacts_consumed"
                .to_string(),
            principal_review_gate_results: pending_principal_review_gate_results(
                language_task.required_principal_review_gates,
            ),
            diff_language_pack_impact: language_task.language_pack_id.to_string(),
            runner_exit_code: -1,
            runner_timed_out: false,
            runner_auth_blocked: false,
            check_exit_code: -1,
            check_timed_out: false,
            check_passed: false,
            visible_check_passed: false,
            hidden_check_observed: false,
            hidden_check_passed: false,
            changed_file_count: 0,
        }
    }
}

impl CodexLiveTrialArtifacts {
    fn hosted_unavailable(
        unavailable: HostedSandboxUnavailable,
        output_artifact_ref: String,
        transcript_ref: String,
        pr_handoff_ref: String,
        language_task: &ForgeLanguageTaskCorpusEntry,
    ) -> Self {
        Self {
            execution_requested: true,
            execution_allowed: false,
            execution_performed: false,
            execution_status: unavailable.backend_status,
            runner_execution_mode: "hosted_sandbox_pending",
            tool_detail: format!(
                "external_machine_adapter codex_cli hosted_sandbox_not_configured backend={} reason={}",
                unavailable.backend_id, unavailable.reason
            ),
            evidence_detail: format!(
                "machine_league_hosted_codex_work_packet_recorded output_quarantined=true adapter_execution_performed=false runtime_image_ref={} credential_binding_ref={}",
                unavailable.runtime_image_ref, unavailable.credential_binding_ref
            ),
            finish_detail:
                "status=RUN_FINISHED_DONE machine_league_hosted_codex_trial_packet_recorded output_quarantined=true files_changed=0"
                    .to_string(),
            worktree_path: unavailable.sandbox_session_ref,
            output_artifact_sha256: format!(
                "sha256:{}",
                hex(&sha256(output_artifact_ref.as_bytes()))
            ),
            output_artifact_ref,
            transcript_ref,
            pr_handoff_ref,
            claimed_check: language_task.visible_check.to_string(),
            artifact_filter_summary: format!(
                "hosted_sandbox_trial_packet_recorded_no_external_artifacts_consumed; backend_status={}; reason={}",
                unavailable.backend_status, unavailable.reason
            ),
            principal_review_gate_results: pending_principal_review_gate_results(
                language_task.required_principal_review_gates,
            ),
            diff_language_pack_impact: language_task.language_pack_id.to_string(),
            codex_exit_code: -1,
            codex_timed_out: false,
            check_exit_code: -1,
            check_timed_out: false,
            check_passed: false,
            visible_check_passed: false,
            hidden_check_observed: false,
            hidden_check_passed: false,
            changed_file_count: 0,
        }
    }

    fn held(
        _run_id: &str,
        output_artifact_ref: String,
        transcript_ref: String,
        pr_handoff_ref: String,
        language_task: &ForgeLanguageTaskCorpusEntry,
        execution_requested: bool,
    ) -> Self {
        Self {
            execution_requested,
            execution_allowed: false,
            execution_performed: false,
            execution_status: if execution_requested {
                "LIVE_EXECUTION_REQUESTED_BUT_DISABLED"
            } else {
                "HELD_TRIAL_PACKET_RECORDED"
            },
            runner_execution_mode: "quarantined_trial_packet_recorded",
            tool_detail: if execution_requested {
                "external_machine_adapter codex_cli execution_disabled_by_env trial_packet_recorded"
                    .to_string()
            } else {
                "external_machine_adapter codex_cli execution_denied trial_packet_recorded"
                    .to_string()
            },
            evidence_detail: "machine_league_work_packet_recorded output_quarantined=true adapter_execution_performed=false".to_string(),
            finish_detail:
                "status=RUN_FINISHED_DONE machine_league_trial_packet_recorded output_quarantined=true files_changed=0"
                    .to_string(),
            worktree_path: String::new(),
            output_artifact_sha256: format!(
                "sha256:{}",
                hex(&sha256(output_artifact_ref.as_bytes()))
            ),
            output_artifact_ref,
            transcript_ref,
            pr_handoff_ref,
            claimed_check: language_task.visible_check.to_string(),
            artifact_filter_summary: "trial_packet_recorded_no_external_artifacts_consumed"
                .to_string(),
            principal_review_gate_results: pending_principal_review_gate_results(
                language_task.required_principal_review_gates,
            ),
            diff_language_pack_impact: language_task.language_pack_id.to_string(),
            codex_exit_code: -1,
            codex_timed_out: false,
            check_exit_code: -1,
            check_timed_out: false,
            check_passed: false,
            visible_check_passed: false,
            hidden_check_observed: false,
            hidden_check_passed: false,
            changed_file_count: 0,
        }
    }
}

fn render_codex_trial_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let run_number = kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| payload_value(receipt, "machine_league_trial") == "true")
        .count()
        + 1;
    let run_id = json_string_field(body, "run_id")
        .unwrap_or_else(|| format!("forge_machine_league_codex_trial_{run_number:06}"));
    let work_packet_id = json_string_field(body, "work_packet_id")
        .unwrap_or_else(|| format!("work_packet_{run_id}"));
    let task = forge_fleet_benchmark_tasks()
        .into_iter()
        .find(|task| task.task_id == "bugfix_small")
        .expect("bugfix task exists");
    let language_task = forge_language_task_corpus()
        .into_iter()
        .find(|task| task.task_corpus_id == "rust-cargo-bug-fix-small")
        .expect("rust bugfix language task exists");
    let output_artifact_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/codex.patch");
    let transcript_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/codex.jsonl");
    let pr_handoff_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/codex-pr.md");
    let eval_fixture = json_string_field(body, "eval_fixture").unwrap_or_default();
    if !fixture_known_or_empty(&eval_fixture) {
        return trial_refusal(
            kernel,
            "codex trial requires a registered Machine League eval fixture",
            &eval_fixture,
        );
    }
    let eval_fixture_contract = machine_league_eval_fixture(&eval_fixture);
    let selected_checks = json_string_field(body, "selected_checks").unwrap_or_else(|| {
        if let Some(fixture) = eval_fixture_contract {
            fixture.visible_check.to_string()
        } else {
            language_task.visible_check.to_string()
        }
    });
    let execution_requested = json_bool_field(body, "execute_live").unwrap_or(false)
        || json_string_field(body, "execution_mode")
            .map(|mode| mode == "live_codex_cli")
            .unwrap_or(false);
    let execution_backend = requested_execution_backend(body);
    let runtime = observe_machine_runtime_fingerprint(
        Some(kernel),
        "codex_cli_external_worker",
        "codex_cli",
        "codex",
        CODEX_EXEC_POLICY_COMMAND,
    );
    let live_authorization = if execution_requested
        && execution_backend.kind() != ForgeExecutionBackendKind::HostedSandbox
        && codex_live_execution_enabled()
    {
        match authorize_live_machine_trial(body, kernel, "codex_cli_external_worker", "codex_cli") {
            Ok(authorization) => Some(authorization),
            Err(reason) => return trial_refusal(kernel, &reason, &eval_fixture),
        }
    } else {
        None
    };
    let live_artifacts = if execution_requested
        && execution_backend.kind() == ForgeExecutionBackendKind::HostedSandbox
    {
        let unavailable =
            execution_backend.hosted_sandbox_not_configured(ExecutionWorkspaceRequest {
                run_id: &run_id,
                eval_fixture: eval_fixture_contract
                    .map(|fixture| fixture.id)
                    .unwrap_or("repo_snapshot"),
                workspace_kind: if eval_fixture_contract.is_some() {
                    ExecutionWorkspaceKind::HeldOutFixture
                } else {
                    ExecutionWorkspaceKind::RepoSnapshot
                },
            });
        CodexLiveTrialArtifacts::hosted_unavailable(
            unavailable,
            output_artifact_ref,
            transcript_ref,
            pr_handoff_ref,
            &language_task,
        )
    } else if live_authorization.is_some() {
        if let Some(fixture) = eval_fixture_contract {
            run_codex_live_fixture_trial(&run_id, body, &language_task, fixture)?
        } else {
            run_codex_live_trial(&run_id, body, &task, &language_task, &selected_checks)?
        }
    } else {
        CodexLiveTrialArtifacts::held(
            &run_id,
            output_artifact_ref,
            transcript_ref,
            pr_handoff_ref,
            &language_task,
            execution_requested,
        )
    };
    let execution_allowed = live_artifacts.execution_allowed.to_string();
    let execution_performed = live_artifacts.execution_performed.to_string();
    let execution_requested_value = live_artifacts.execution_requested.to_string();
    let check_passed = live_artifacts.check_passed.to_string();
    let codex_exit_code = live_artifacts.codex_exit_code.to_string();
    let check_exit_code = live_artifacts.check_exit_code.to_string();
    let changed_file_count = live_artifacts.changed_file_count.to_string();
    let live_max_spend_cents = live_authorization
        .as_ref()
        .map(|value| value.max_spend_cents)
        .unwrap_or(0)
        .to_string();
    let live_max_tasks = live_authorization
        .as_ref()
        .map(|value| value.max_tasks)
        .unwrap_or(0)
        .to_string();
    let live_max_parallel_agents = live_authorization
        .as_ref()
        .map(|value| value.max_parallel_agents)
        .unwrap_or(0)
        .to_string();
    let live_task_ordinal = live_authorization
        .as_ref()
        .map(|value| value.task_ordinal)
        .unwrap_or(0)
        .to_string();
    let work_packet_budget = format!(
        "max_spend_cents={},max_runtime_ms=300000",
        live_max_spend_cents
    );
    let work_packet_repo_id = if let Some(fixture) = eval_fixture_contract {
        fixture.repo_id
    } else {
        "mdx-native-harness-elevation"
    };
    let work_packet_repo_ref = if eval_fixture_contract.is_some() {
        "held_out_fixture_generated_per_run"
    } else {
        "local_worktree_snapshot"
    };
    let work_packet_allowed_scope = if let Some(fixture) = eval_fixture_contract {
        fixture.allowed_scope
    } else {
        task.allowed_scope
    };
    let work_packet_acceptance = if let Some(fixture) = eval_fixture_contract {
        fixture.acceptance_criteria
    } else {
        task.acceptance
    };
    let eval_fixture_snapshot_id = eval_fixture_snapshot_id(&eval_fixture);
    let contamination_status = contamination_status_for_eval_fixture(&eval_fixture);
    let eval_oracle_status = eval_oracle_status(
        live_artifacts.visible_check_passed,
        live_artifacts.hidden_check_observed,
        live_artifacts.hidden_check_passed,
    );
    let visible_check_passed = live_artifacts.visible_check_passed.to_string();
    let hidden_check_observed = live_artifacts.hidden_check_observed.to_string();
    let hidden_check_passed = live_artifacts.hidden_check_passed.to_string();
    let identity = GovernedWriteIdentity::local_demo("agent:codex");
    let start_report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:codex",
                run_id: &run_id,
                event: "run_started",
                work_item_id: "machine_league_codex_trial",
                detail: "Forge staged a Codex CLI quarantined trial packet",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &identity,
            &[
                ("repo_id", work_packet_repo_id),
                ("repo_root", "."),
                ("repo_primary_language", "rust"),
                ("language_pack_id", language_task.language_pack_id),
                ("selected_checks", &selected_checks),
                ("machine_league_eval_fixture", eval_fixture.as_str()),
                (
                    "machine_league_eval_fixture_family",
                    eval_fixture_contract
                        .map(|fixture| fixture.family_id)
                        .unwrap_or("repo_snapshot"),
                ),
                (
                    "machine_league_eval_fixture_sibling",
                    eval_fixture_contract
                        .map(|fixture| fixture.sibling_fixture_id)
                        .unwrap_or(""),
                ),
                (
                    "machine_league_transferable_edge",
                    eval_fixture_contract
                        .map(|fixture| fixture.transferable_edge)
                        .unwrap_or(""),
                ),
                ("eval_fixture_snapshot_id", eval_fixture_snapshot_id),
                ("eval_oracle_status", eval_oracle_status),
                ("visible_check_passed", visible_check_passed.as_str()),
                ("hidden_check_observed", hidden_check_observed.as_str()),
                ("hidden_check_passed", hidden_check_passed.as_str()),
                ("contamination_status", contamination_status),
                ("fresh_fixture_required_for_scorecard_acceptance", "true"),
                ("builder_casting_selected_provider_family", "anthropic"),
                (
                    "builder_casting_selected_model_id",
                    "claude-opus-4-8-via-mdx",
                ),
                ("builder_casting_selected_slot", "CODEx_CLI"),
                ("machine_league_trial", "true"),
                ("machine_league_run_kind", "machine_league_trial"),
                ("runner_profile_runner_id", "codex_cli_external_worker"),
                ("runner_profile_runner_kind", "external_machine"),
                ("runner_profile_display_name", "Codex CLI"),
                ("runner_profile_adapter_kind", "codex_cli"),
                (
                    "runner_profile_execution_mode",
                    live_artifacts.runner_execution_mode,
                ),
                ("execution_backend_kind", execution_backend.kind().as_str()),
                (
                    "execution_backend_id",
                    if execution_backend.kind() == ForgeExecutionBackendKind::HostedSandbox {
                        "hosted_sandbox"
                    } else {
                        "local_worktree_subprocess"
                    },
                ),
                (
                    "runner_profile_model_profile_id",
                    "codex_anthropic_responses_profile",
                ),
                ("machine_runtime_fingerprint_id", &runtime.fingerprint_id),
                ("machine_runtime_runner_id", &runtime.runner_id),
                ("machine_runtime_adapter_kind", &runtime.adapter_kind),
                ("machine_runtime_binary_name", &runtime.binary_name),
                ("machine_runtime_binary_path", &runtime.binary_path),
                (
                    "machine_runtime_binary_present",
                    if runtime.binary_present {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("machine_runtime_version_command", &runtime.version_command),
                (
                    "machine_runtime_version_observed",
                    &runtime.version_observed,
                ),
                (
                    "machine_runtime_version_raw_output",
                    &runtime.version_raw_output,
                ),
                ("machine_runtime_checksum_sha256", &runtime.checksum_sha256),
                (
                    "machine_runtime_adapter_contract_version",
                    &runtime.adapter_contract_version,
                ),
                (
                    "machine_runtime_command_contract",
                    &runtime.command_contract,
                ),
                ("machine_runtime_drift_status", &runtime.drift_status),
                (
                    "machine_runtime_last_fingerprint_id",
                    &runtime.last_fingerprint_id,
                ),
                (
                    "machine_runtime_compatibility_status",
                    &runtime.compatibility_status,
                ),
                ("quarantine_status", "output_quarantined_pending_mdx_gates"),
                ("quarantine_output_quarantined", "true"),
                ("quarantine_external_output_consumable", "false"),
                (
                    "quarantine_acceptance_gate",
                    "mdx_quality_gates_then_principal_review",
                ),
                (
                    "quarantine_result_projection_route",
                    "/forge/fleet-eval-results/projection.json",
                ),
                ("quarantine_blocked_reason", "pending_mdx_quality_gates"),
                ("league_context_visibility_tier", "product_recommendation"),
                (
                    "league_context_recommendation_rationale",
                    "Forge is gathering first accepted Codex CLI evidence for rust bug fixes.",
                ),
                (
                    "league_context_fallback_runner_id",
                    "mdx_native_harness_runner",
                ),
                ("league_context_scorecard_evidence_count", "0"),
                (
                    "league_context_quarantine_posture",
                    "external_output_held_until_mdx_gates",
                ),
                ("work_packet_id", &work_packet_id),
                ("work_packet_runner_id", "codex_cli_external_worker"),
                ("work_packet_runner_profile_id", "codex_cli_external_worker"),
                ("work_packet_task_class", task.class),
                ("work_packet_complexity_tier", task.complexity_tier),
                (
                    "work_packet_language_pack_id",
                    language_task.language_pack_id,
                ),
                ("work_packet_repo_id", work_packet_repo_id),
                ("work_packet_repo_ref", work_packet_repo_ref),
                ("work_packet_allowed_write_scope", work_packet_allowed_scope),
                (
                    "work_packet_blocked_paths",
                    "provider secrets,production state",
                ),
                ("work_packet_selected_checks", &selected_checks),
                ("work_packet_visible_check", &selected_checks),
                (
                    "work_packet_hidden_check_slot",
                    if let Some(fixture) = eval_fixture_contract {
                        fixture.hidden_check
                    } else {
                        language_task.hidden_check_slot
                    },
                ),
                (
                    "work_packet_standards_source_fingerprints",
                    "AGENTS.md=fnv1a64:0000000000000000",
                ),
                (
                    "work_packet_redaction_policy",
                    "mdx_context_redaction_required",
                ),
                ("work_packet_budget", work_packet_budget.as_str()),
                ("work_packet_expiry", "single_trial"),
                (
                    "work_packet_stop_conditions",
                    "manual_stop,budget_exhausted,quality_gate_failure",
                ),
                ("work_packet_transcript_required", "true"),
                (
                    "work_packet_artifact_namespace",
                    "quarantine://forge/fleet-eval/",
                ),
                ("work_packet_acceptance_criteria", work_packet_acceptance),
                (
                    "adapter_execution_requested",
                    execution_requested_value.as_str(),
                ),
                ("adapter_execution_performed", execution_performed.as_str()),
                ("adapter_execution_allowed", execution_allowed.as_str()),
                ("adapter_execution_status", live_artifacts.execution_status),
                (
                    "live_run_approval_id",
                    live_authorization
                        .as_ref()
                        .map(|value| value.approval_id.as_str())
                        .unwrap_or(""),
                ),
                (
                    "live_run_approval_receipt_id",
                    live_authorization
                        .as_ref()
                        .map(|value| value.approval_receipt_id.as_str())
                        .unwrap_or(""),
                ),
                (
                    "runner_execution_clearance_receipt_id",
                    live_authorization
                        .as_ref()
                        .map(|value| value.clearance_receipt_id.as_str())
                        .unwrap_or(""),
                ),
                (
                    "closed_runner_clearance_receipt_id",
                    live_authorization
                        .as_ref()
                        .map(|value| value.clearance_receipt_id.as_str())
                        .unwrap_or(""),
                ),
                (
                    "live_execution_provider",
                    live_authorization
                        .as_ref()
                        .map(|value| value.provider)
                        .unwrap_or(""),
                ),
                ("live_run_max_spend_cents", live_max_spend_cents.as_str()),
                ("live_run_max_tasks", live_max_tasks.as_str()),
                (
                    "live_run_max_parallel_agents",
                    live_max_parallel_agents.as_str(),
                ),
                ("live_run_task_ordinal", live_task_ordinal.as_str()),
                (
                    "live_execution_authority_scope",
                    "single_quarantined_fixture_trial",
                ),
                (
                    "adapter_execution_backend",
                    execution_backend.kind().as_str(),
                ),
                (
                    "adapter_worktree_path",
                    live_artifacts.worktree_path.as_str(),
                ),
                ("adapter_codex_exit_code", codex_exit_code.as_str()),
                (
                    "adapter_codex_timed_out",
                    if live_artifacts.codex_timed_out {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("adapter_check_exit_code", check_exit_code.as_str()),
                (
                    "adapter_check_timed_out",
                    if live_artifacts.check_timed_out {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("adapter_check_passed", check_passed.as_str()),
                ("adapter_changed_file_count", changed_file_count.as_str()),
                ("language_task_corpus_id", language_task.task_corpus_id),
                ("language_task_alignment_status", "aligned"),
                (
                    "language_task_alignment_source",
                    "machine_league_codex_trial_contract",
                ),
                ("language_task_class", language_task.task_class),
                (
                    "language_task_complexity_tier",
                    language_task.complexity_tier,
                ),
                ("language_task_visible_check", language_task.visible_check),
                (
                    "language_task_hidden_check_slot",
                    eval_fixture_contract
                        .map(|fixture| fixture.hidden_check_slot)
                        .unwrap_or(language_task.hidden_check_slot),
                ),
                (
                    "language_task_artifact_noise_expected",
                    language_task.artifact_noise_expected,
                ),
                (
                    "language_task_required_principal_review_gates",
                    language_task.required_principal_review_gates,
                ),
                (
                    "language_task_engineering_facets",
                    language_task_engineering_facets(&language_task),
                ),
                (
                    "language_task_evaluation_oracle",
                    language_task_evaluation_oracle(&language_task),
                ),
                (
                    "language_task_human_timebox_minutes",
                    &language_task_human_timebox_minutes(&language_task).to_string(),
                ),
                (
                    "language_task_contamination_policy",
                    language_task_contamination_policy(&language_task),
                ),
                (
                    "language_task_alignment_grants_execution_authority",
                    "false",
                ),
            ],
        )
        .map_err(|error| error.message())?;
    kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            run_id: &run_id,
            event: "tool_executed",
            work_item_id: "machine_league_codex_trial",
            detail: &live_artifacts.tool_detail,
            turn: 1,
            input_tokens: 0,
            output_tokens: 0,
        })
        .map_err(|error| error.message())?;
    kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            run_id: &run_id,
            event: "evidence_appended",
            work_item_id: "machine_league_codex_trial",
            detail: &live_artifacts.evidence_detail,
            turn: 1,
            input_tokens: 0,
            output_tokens: 0,
        })
        .map_err(|error| error.message())?;
    let consumption_receipt_id = record_external_output_consumption_boundary(
        kernel,
        &identity,
        ExternalOutputBoundary {
            actor_id: "agent:codex",
            run_id: &run_id,
            work_item_id: "machine_league_codex_trial",
            runner_id: "codex_cli_external_worker",
            adapter_kind: "codex_cli",
            output_artifact_ref: &live_artifacts.output_artifact_ref,
        },
        live_authorization.as_ref(),
    )?;
    let result = kernel
        .ingest_forge_fleet_eval_result_local_with_identity(
            ForgeFleetEvalResultSubmission {
                tenant_id: "local_tenant",
                actor_id: "agent:codex",
                actor_role: "worker",
                submission_id: &format!("{run_id}_quarantined_result"),
                benchmark_task_id: task.task_id,
                language_task_corpus_id: language_task.task_corpus_id,
                language_pack_id: language_task.language_pack_id,
                repo_family: language_task.repo_family,
                runner_id: "codex_cli_external_worker",
                model_profile_id: "codex_anthropic_responses_profile",
                output_artifact_ref: &live_artifacts.output_artifact_ref,
                output_artifact_sha256: &live_artifacts.output_artifact_sha256,
                transcript_ref: &live_artifacts.transcript_ref,
                claimed_check: &live_artifacts.claimed_check,
                hidden_check_slot: language_task.hidden_check_slot,
                artifact_noise_expected: language_task.artifact_noise_expected,
                principal_review_gate_results: &live_artifacts.principal_review_gate_results,
                standards_source_fingerprints: "AGENTS.md=fnv1a64:0000000000000000",
                artifact_filter_summary: &live_artifacts.artifact_filter_summary,
                diff_language_pack_impact: &live_artifacts.diff_language_pack_impact,
                principal_engineer_verdict: "pending_principal_engineer_review",
                pr_handoff_ref: &live_artifacts.pr_handoff_ref,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    if live_artifacts.execution_performed {
        let assessment = live_trial_quality_assessment(&live_artifacts);
        record_run_eval_quality_gate_event(
            kernel,
            &run_id,
            &assessment,
            &result.result_receipt_id,
        )?;
    }
    kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:codex",
            run_id: &run_id,
            event: "run_finished",
            work_item_id: "machine_league_codex_trial",
            detail: &live_artifacts.finish_detail,
            turn: 2,
            input_tokens: 0,
            output_tokens: 0,
        })
        .map_err(|error| error.message())?;
    let execution_backend_kind = execution_backend.kind().as_str();
    let response_status = if execution_backend.kind() == ForgeExecutionBackendKind::HostedSandbox {
        live_artifacts.execution_status
    } else if live_artifacts.execution_performed {
        "CODEX_CLI_EXECUTED_QUARANTINED_PENDING_MDX_GATES"
    } else {
        "CODEX_CLI_TRIAL_PACKET_RECORDED_QUARANTINED"
    };
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-codex-trial-local-post","status":{},"run_id":{},"run_started_receipt_id":{},"consumption_receipt_id":{},"work_packet_id":{},"runner_id":"codex_cli_external_worker","runner_display_name":"Codex CLI","execution_backend":{},"runtime_fingerprint":{},"stream_route":{},"run_projection_route":"/forge/runs/projection.json","result_projection_route":"/forge/fleet-eval-results/projection.json","result_receipt_id":{},"output_artifact_ref":{},"output_artifact_sha256":{},"transcript_ref":{},"worktree_path":{},"eval_fixture_snapshot_id":{},"eval_oracle_status":{},"visible_check_passed":{},"hidden_check_observed":{},"hidden_check_passed":{},"contamination_status":{},"adapter_execution_requested":{},"adapter_execution_allowed":{},"adapter_execution_performed":{},"codex_exit_code":{},"codex_timed_out":{},"check_exit_code":{},"check_timed_out":{},"check_passed":{},"changed_file_count":{},"external_output_consumption_status":"denied_quarantined_pending_distillation","external_output_consumable":false,"output_quarantined":true,"mdx_quality_gates_required":true,"accepted_for_scoreboard":false,"blocked_reason":"pending_mdx_quality_gates","production_write_allowed":false}}"#,
        json_string_literal(response_status),
        json_string_literal(&run_id),
        json_string_literal(&start_report.receipt_id),
        json_string_literal(&consumption_receipt_id),
        json_string_literal(&work_packet_id),
        json_string_literal(execution_backend_kind),
        machine_runtime_json(&runtime),
        json_string_literal(&format!("/forge/runs/stream?run_id={run_id}")),
        json_string_literal(&result.result_receipt_id),
        json_string_literal(&live_artifacts.output_artifact_ref),
        json_string_literal(&live_artifacts.output_artifact_sha256),
        json_string_literal(&live_artifacts.transcript_ref),
        json_string_literal(&live_artifacts.worktree_path),
        json_string_literal(eval_fixture_snapshot_id),
        json_string_literal(eval_oracle_status),
        live_artifacts.visible_check_passed,
        live_artifacts.hidden_check_observed,
        live_artifacts.hidden_check_passed,
        json_string_literal(contamination_status),
        live_artifacts.execution_requested,
        live_artifacts.execution_allowed,
        live_artifacts.execution_performed,
        live_artifacts.codex_exit_code,
        live_artifacts.codex_timed_out,
        live_artifacts.check_exit_code,
        live_artifacts.check_timed_out,
        live_artifacts.check_passed,
        live_artifacts.changed_file_count,
    ))
}

fn trial_refusal(
    kernel: &mut MdxKernel,
    reason: &str,
    eval_fixture: &str,
) -> Result<String, String> {
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:codex",
                run_id: "forge_machine_league_codex_trial_refusal",
                event: "evidence_appended",
                work_item_id: "machine_league_codex_trial_refusal",
                detail: &format!("codex_trial_refused reason={}", reason),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:codex"),
            &[
                ("machine_league_trial_status", "refused"),
                ("machine_league_eval_fixture", eval_fixture),
                ("machine_league_trial_refusal_reason", reason),
                ("runner_id", "codex_cli_external_worker"),
                ("accepted_for_scoreboard", "false"),
                ("external_output_consumable", "false"),
                ("eval_grants_execution_authority", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-codex-trial-local-post","status":"REFUSED","reason":{},"eval_fixture":{},"trial_refusal_receipt_id":{},"accepted_for_scoreboard":false,"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(reason),
        json_string_literal(eval_fixture),
        json_string_literal(&report.receipt_id)
    ))
}

fn render_gemini_trial_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let run_number = kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| payload_value(receipt, "machine_league_trial") == "true")
        .count()
        + 1;
    let run_id = json_string_field(body, "run_id")
        .unwrap_or_else(|| format!("forge_machine_league_gemini_trial_{run_number:06}"));
    let work_packet_id = json_string_field(body, "work_packet_id")
        .unwrap_or_else(|| format!("work_packet_{run_id}"));
    let task = forge_fleet_benchmark_tasks()
        .into_iter()
        .find(|task| task.task_id == "bugfix_small")
        .expect("bugfix task exists");
    let language_task = forge_language_task_corpus()
        .into_iter()
        .find(|task| task.task_corpus_id == "rust-cargo-bug-fix-small")
        .expect("rust bugfix language task exists");
    let output_artifact_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/gemini.patch");
    let transcript_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/gemini.stream-json");
    let pr_handoff_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/gemini-pr.md");
    let eval_fixture = json_string_field(body, "eval_fixture").unwrap_or_default();
    let uses_rust_tax_fixture = eval_fixture == MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID;
    if !uses_rust_tax_fixture && json_bool_field(body, "execute_live").unwrap_or(false) {
        return gemini_trial_refusal(
            kernel,
            "Gemini CLI live trial currently supports rust_tax_rounding only",
            &eval_fixture,
        );
    }
    let execution_requested = json_bool_field(body, "execute_live").unwrap_or(false)
        || json_string_field(body, "execution_mode")
            .map(|mode| mode == "live_gemini_cli")
            .unwrap_or(false);
    let runtime = observe_machine_runtime_fingerprint(
        Some(kernel),
        "gemini_cli_external_worker",
        "gemini_cli",
        "gemini",
        GEMINI_EXEC_POLICY_COMMAND,
    );
    let live_artifacts = if execution_requested && gemini_live_execution_enabled() {
        run_gemini_live_rust_tax_fixture_trial(&run_id, body, &language_task)?
    } else {
        GeminiLiveTrialArtifacts::held(
            output_artifact_ref,
            transcript_ref,
            pr_handoff_ref,
            &language_task,
            execution_requested,
        )
    };
    let execution_allowed = live_artifacts.execution_allowed.to_string();
    let execution_performed = live_artifacts.execution_performed.to_string();
    let execution_requested_value = live_artifacts.execution_requested.to_string();
    let check_passed = live_artifacts.check_passed.to_string();
    let gemini_exit_code = live_artifacts.gemini_exit_code.to_string();
    let check_exit_code = live_artifacts.check_exit_code.to_string();
    let changed_file_count = live_artifacts.changed_file_count.to_string();
    let eval_fixture_snapshot_id = eval_fixture_snapshot_id(&eval_fixture);
    let contamination_status = contamination_status_for_eval_fixture(&eval_fixture);
    let eval_oracle_status = eval_oracle_status(
        live_artifacts.visible_check_passed,
        live_artifacts.hidden_check_observed,
        live_artifacts.hidden_check_passed,
    );
    let visible_check_passed = live_artifacts.visible_check_passed.to_string();
    let hidden_check_observed = live_artifacts.hidden_check_observed.to_string();
    let hidden_check_passed = live_artifacts.hidden_check_passed.to_string();
    let identity = GovernedWriteIdentity::local_demo("agent:gemini");
    let start_report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:gemini",
                run_id: &run_id,
                event: "run_started",
                work_item_id: "machine_league_gemini_trial",
                detail: "Forge staged a Gemini CLI quarantined trial packet",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &identity,
            &[
                ("repo_id", "machine-league-rust-tax-rounding-fixture"),
                ("repo_root", "."),
                ("repo_primary_language", "rust"),
                ("language_pack_id", language_task.language_pack_id),
                ("selected_checks", MACHINE_LEAGUE_RUST_TAX_VISIBLE_CHECK),
                ("machine_league_eval_fixture", eval_fixture.as_str()),
                ("eval_fixture_snapshot_id", eval_fixture_snapshot_id),
                ("eval_oracle_status", eval_oracle_status),
                ("visible_check_passed", visible_check_passed.as_str()),
                ("hidden_check_observed", hidden_check_observed.as_str()),
                ("hidden_check_passed", hidden_check_passed.as_str()),
                ("contamination_status", contamination_status),
                ("fresh_fixture_required_for_scorecard_acceptance", "true"),
                ("builder_casting_selected_provider_family", "gemini"),
                (
                    "builder_casting_selected_model_id",
                    "gemini-cli-selected-model",
                ),
                ("builder_casting_selected_slot", "GEMINI_CLI"),
                ("machine_league_trial", "true"),
                ("machine_league_run_kind", "machine_league_trial"),
                ("runner_profile_runner_id", "gemini_cli_external_worker"),
                ("runner_profile_runner_kind", "external_machine"),
                ("runner_profile_display_name", "Gemini CLI"),
                ("runner_profile_adapter_kind", "gemini_cli"),
                (
                    "runner_profile_execution_mode",
                    live_artifacts.runner_execution_mode,
                ),
                (
                    "runner_profile_model_profile_id",
                    "gemini_cli_model_profile",
                ),
                ("machine_runtime_fingerprint_id", &runtime.fingerprint_id),
                ("machine_runtime_runner_id", &runtime.runner_id),
                ("machine_runtime_adapter_kind", &runtime.adapter_kind),
                ("machine_runtime_binary_name", &runtime.binary_name),
                ("machine_runtime_binary_path", &runtime.binary_path),
                (
                    "machine_runtime_binary_present",
                    if runtime.binary_present {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("machine_runtime_version_command", &runtime.version_command),
                (
                    "machine_runtime_version_observed",
                    &runtime.version_observed,
                ),
                (
                    "machine_runtime_version_raw_output",
                    &runtime.version_raw_output,
                ),
                ("machine_runtime_checksum_sha256", &runtime.checksum_sha256),
                (
                    "machine_runtime_adapter_contract_version",
                    &runtime.adapter_contract_version,
                ),
                (
                    "machine_runtime_command_contract",
                    &runtime.command_contract,
                ),
                ("machine_runtime_drift_status", &runtime.drift_status),
                (
                    "machine_runtime_last_fingerprint_id",
                    &runtime.last_fingerprint_id,
                ),
                (
                    "machine_runtime_compatibility_status",
                    &runtime.compatibility_status,
                ),
                ("quarantine_status", "output_quarantined_pending_mdx_gates"),
                ("quarantine_output_quarantined", "true"),
                ("quarantine_external_output_consumable", "false"),
                (
                    "quarantine_acceptance_gate",
                    "mdx_quality_gates_then_principal_review",
                ),
                (
                    "quarantine_result_projection_route",
                    "/forge/fleet-eval-results/projection.json",
                ),
                ("quarantine_blocked_reason", "pending_mdx_quality_gates"),
                ("league_context_visibility_tier", "product_recommendation"),
                (
                    "league_context_recommendation_rationale",
                    "Forge is gathering first accepted Gemini CLI evidence for rust bug fixes.",
                ),
                (
                    "league_context_fallback_runner_id",
                    "mdx_native_harness_runner",
                ),
                ("league_context_scorecard_evidence_count", "0"),
                (
                    "league_context_quarantine_posture",
                    "external_output_held_until_mdx_gates",
                ),
                ("work_packet_id", &work_packet_id),
                ("work_packet_runner_id", "gemini_cli_external_worker"),
                (
                    "work_packet_runner_profile_id",
                    "gemini_cli_external_worker",
                ),
                ("work_packet_task_class", task.class),
                ("work_packet_complexity_tier", task.complexity_tier),
                (
                    "work_packet_language_pack_id",
                    language_task.language_pack_id,
                ),
                (
                    "work_packet_repo_id",
                    "machine-league-rust-tax-rounding-fixture",
                ),
                ("work_packet_repo_ref", "held_out_fixture_generated_per_run"),
                ("work_packet_allowed_write_scope", "src/lib.rs"),
                (
                    "work_packet_blocked_paths",
                    "provider secrets,production state",
                ),
                (
                    "work_packet_selected_checks",
                    MACHINE_LEAGUE_RUST_TAX_VISIBLE_CHECK,
                ),
                (
                    "work_packet_visible_check",
                    MACHINE_LEAGUE_RUST_TAX_VISIBLE_CHECK,
                ),
                (
                    "work_packet_hidden_check_slot",
                    MACHINE_LEAGUE_RUST_TAX_HIDDEN_CHECK,
                ),
                (
                    "work_packet_standards_source_fingerprints",
                    "AGENTS.md=fnv1a64:0000000000000000",
                ),
                (
                    "work_packet_redaction_policy",
                    "mdx_context_redaction_required",
                ),
                (
                    "work_packet_budget",
                    "max_spend_cents=100,max_runtime_ms=300000",
                ),
                ("work_packet_expiry", "single_trial"),
                (
                    "work_packet_stop_conditions",
                    "manual_stop,budget_exhausted,quality_gate_failure",
                ),
                ("work_packet_transcript_required", "true"),
                (
                    "work_packet_artifact_namespace",
                    "quarantine://forge/fleet-eval/",
                ),
                (
                    "work_packet_acceptance_criteria",
                    "visible and hidden Rust tests pass without weakening the fixture tests",
                ),
                (
                    "adapter_execution_requested",
                    execution_requested_value.as_str(),
                ),
                ("adapter_execution_performed", execution_performed.as_str()),
                ("adapter_execution_allowed", execution_allowed.as_str()),
                ("adapter_execution_status", live_artifacts.execution_status),
                (
                    "adapter_worktree_path",
                    live_artifacts.worktree_path.as_str(),
                ),
                ("adapter_gemini_exit_code", gemini_exit_code.as_str()),
                (
                    "adapter_gemini_timed_out",
                    if live_artifacts.gemini_timed_out {
                        "true"
                    } else {
                        "false"
                    },
                ),
                (
                    "adapter_gemini_auth_blocked",
                    if live_artifacts.gemini_auth_blocked {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("adapter_check_exit_code", check_exit_code.as_str()),
                (
                    "adapter_check_timed_out",
                    if live_artifacts.check_timed_out {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("adapter_check_passed", check_passed.as_str()),
                ("adapter_changed_file_count", changed_file_count.as_str()),
                ("language_task_corpus_id", language_task.task_corpus_id),
                ("language_task_alignment_status", "aligned"),
                (
                    "language_task_alignment_source",
                    "machine_league_gemini_trial_contract",
                ),
                ("language_task_class", language_task.task_class),
                (
                    "language_task_complexity_tier",
                    language_task.complexity_tier,
                ),
                ("language_task_visible_check", language_task.visible_check),
                (
                    "language_task_hidden_check_slot",
                    language_task.hidden_check_slot,
                ),
                (
                    "language_task_artifact_noise_expected",
                    language_task.artifact_noise_expected,
                ),
                (
                    "language_task_required_principal_review_gates",
                    language_task.required_principal_review_gates,
                ),
                (
                    "language_task_engineering_facets",
                    language_task_engineering_facets(&language_task),
                ),
                (
                    "language_task_evaluation_oracle",
                    language_task_evaluation_oracle(&language_task),
                ),
                (
                    "language_task_human_timebox_minutes",
                    &language_task_human_timebox_minutes(&language_task).to_string(),
                ),
                (
                    "language_task_contamination_policy",
                    language_task_contamination_policy(&language_task),
                ),
                (
                    "language_task_alignment_grants_execution_authority",
                    "false",
                ),
            ],
        )
        .map_err(|error| error.message())?;
    kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:gemini",
            run_id: &run_id,
            event: "tool_executed",
            work_item_id: "machine_league_gemini_trial",
            detail: &live_artifacts.tool_detail,
            turn: 1,
            input_tokens: 0,
            output_tokens: 0,
        })
        .map_err(|error| error.message())?;
    kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:gemini",
            run_id: &run_id,
            event: "evidence_appended",
            work_item_id: "machine_league_gemini_trial",
            detail: &live_artifacts.evidence_detail,
            turn: 1,
            input_tokens: 0,
            output_tokens: 0,
        })
        .map_err(|error| error.message())?;
    let result = kernel
        .ingest_forge_fleet_eval_result_local_with_identity(
            ForgeFleetEvalResultSubmission {
                tenant_id: "local_tenant",
                actor_id: "agent:gemini",
                actor_role: "worker",
                submission_id: &format!("{run_id}_quarantined_result"),
                benchmark_task_id: task.task_id,
                language_task_corpus_id: language_task.task_corpus_id,
                language_pack_id: language_task.language_pack_id,
                repo_family: language_task.repo_family,
                runner_id: "gemini_cli_external_worker",
                model_profile_id: "gemini_cli_model_profile",
                output_artifact_ref: &live_artifacts.output_artifact_ref,
                output_artifact_sha256: &live_artifacts.output_artifact_sha256,
                transcript_ref: &live_artifacts.transcript_ref,
                claimed_check: &live_artifacts.claimed_check,
                hidden_check_slot: language_task.hidden_check_slot,
                artifact_noise_expected: language_task.artifact_noise_expected,
                principal_review_gate_results: &live_artifacts.principal_review_gate_results,
                standards_source_fingerprints: "AGENTS.md=fnv1a64:0000000000000000",
                artifact_filter_summary: &live_artifacts.artifact_filter_summary,
                diff_language_pack_impact: &live_artifacts.diff_language_pack_impact,
                principal_engineer_verdict: "pending_principal_engineer_review",
                pr_handoff_ref: &live_artifacts.pr_handoff_ref,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    if live_artifacts.execution_performed {
        let assessment = live_cli_trial_quality_assessment(
            live_artifacts.gemini_exit_code,
            live_artifacts.check_passed,
            live_artifacts.changed_file_count,
        );
        record_run_eval_quality_gate_event(
            kernel,
            &run_id,
            &assessment,
            &result.result_receipt_id,
        )?;
    }
    kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:gemini",
            run_id: &run_id,
            event: "run_finished",
            work_item_id: "machine_league_gemini_trial",
            detail: &live_artifacts.finish_detail,
            turn: 2,
            input_tokens: 0,
            output_tokens: 0,
        })
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-gemini-trial-local-post","status":{},"run_id":{},"run_started_receipt_id":{},"work_packet_id":{},"runner_id":"gemini_cli_external_worker","runner_display_name":"Gemini CLI","runtime_fingerprint":{},"stream_route":{},"run_projection_route":"/forge/runs/projection.json","result_projection_route":"/forge/fleet-eval-results/projection.json","result_receipt_id":{},"output_artifact_ref":{},"output_artifact_sha256":{},"transcript_ref":{},"worktree_path":{},"eval_fixture_snapshot_id":{},"eval_oracle_status":{},"visible_check_passed":{},"hidden_check_observed":{},"hidden_check_passed":{},"contamination_status":{},"adapter_execution_requested":{},"adapter_execution_allowed":{},"adapter_execution_performed":{},"gemini_exit_code":{},"gemini_timed_out":{},"gemini_auth_blocked":{},"check_exit_code":{},"check_timed_out":{},"check_passed":{},"changed_file_count":{},"external_output_consumable":false,"output_quarantined":true,"mdx_quality_gates_required":true,"accepted_for_scoreboard":false,"blocked_reason":"pending_mdx_quality_gates","production_write_allowed":false}}"#,
        json_string_literal(if live_artifacts.execution_performed {
            "GEMINI_CLI_EXECUTED_QUARANTINED_PENDING_MDX_GATES"
        } else {
            "GEMINI_CLI_TRIAL_PACKET_RECORDED_QUARANTINED"
        }),
        json_string_literal(&run_id),
        json_string_literal(&start_report.receipt_id),
        json_string_literal(&work_packet_id),
        machine_runtime_json(&runtime),
        json_string_literal(&format!("/forge/runs/stream?run_id={run_id}")),
        json_string_literal(&result.result_receipt_id),
        json_string_literal(&live_artifacts.output_artifact_ref),
        json_string_literal(&live_artifacts.output_artifact_sha256),
        json_string_literal(&live_artifacts.transcript_ref),
        json_string_literal(&live_artifacts.worktree_path),
        json_string_literal(eval_fixture_snapshot_id),
        json_string_literal(eval_oracle_status),
        live_artifacts.visible_check_passed,
        live_artifacts.hidden_check_observed,
        live_artifacts.hidden_check_passed,
        json_string_literal(contamination_status),
        live_artifacts.execution_requested,
        live_artifacts.execution_allowed,
        live_artifacts.execution_performed,
        live_artifacts.gemini_exit_code,
        live_artifacts.gemini_timed_out,
        live_artifacts.gemini_auth_blocked,
        live_artifacts.check_exit_code,
        live_artifacts.check_timed_out,
        live_artifacts.check_passed,
        live_artifacts.changed_file_count,
    ))
}

fn gemini_trial_refusal(
    kernel: &mut MdxKernel,
    reason: &str,
    eval_fixture: &str,
) -> Result<String, String> {
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:gemini",
                run_id: "forge_machine_league_gemini_trial_refusal",
                event: "evidence_appended",
                work_item_id: "machine_league_gemini_trial_refusal",
                detail: &format!("gemini_cli_trial_refused reason={}", reason),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:gemini"),
            &[
                ("machine_league_trial_status", "refused"),
                ("machine_league_eval_fixture", eval_fixture),
                ("machine_league_trial_refusal_reason", reason),
                ("runner_id", "gemini_cli_external_worker"),
                ("accepted_for_scoreboard", "false"),
                ("external_output_consumable", "false"),
                ("eval_grants_execution_authority", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-gemini-trial-local-post","status":"REFUSED","reason":{},"eval_fixture":{},"trial_refusal_receipt_id":{},"accepted_for_scoreboard":false,"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(reason),
        json_string_literal(eval_fixture),
        json_string_literal(&report.receipt_id)
    ))
}

fn render_open_machine_trial_json(
    body: &str,
    kernel: &mut MdxKernel,
    spec: OpenMachineTrialSpec,
) -> Result<String, String> {
    let eval_fixture = json_string_field(body, "eval_fixture").unwrap_or_default();
    let spec_runner = forge_fleet_runner_profiles()
        .into_iter()
        .find(|runner| runner.runner_id == spec.runner_id);
    if closed_runner_requires_clearance(spec.adapter_kind)
        && !spec_runner
            .as_ref()
            .map(|runner| closed_runner_scorecard_enabled(Some(kernel), runner))
            .unwrap_or(false)
    {
        return open_machine_trial_refusal(
            kernel,
            spec,
            "closed runner fleet-eval clearance or provider key enablement is required before this machine can stage trials",
            &eval_fixture,
        );
    }
    let uses_rust_tax_fixture = eval_fixture == MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID;
    if !uses_rust_tax_fixture && json_bool_field(body, "execute_live").unwrap_or(false) {
        return open_machine_trial_refusal(
            kernel,
            spec,
            "Open machine live trials currently support rust_tax_rounding only",
            &eval_fixture,
        );
    }
    let run_number = kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| payload_value(receipt, "machine_league_trial") == "true")
        .count()
        + 1;
    let run_id = json_string_field(body, "run_id").unwrap_or_else(|| {
        format!(
            "forge_machine_league_{}_trial_{run_number:06}",
            spec.adapter_slug
        )
    });
    let work_packet_id = json_string_field(body, "work_packet_id")
        .unwrap_or_else(|| format!("work_packet_{run_id}"));
    let task = forge_fleet_benchmark_tasks()
        .into_iter()
        .find(|task| task.task_id == "bugfix_small")
        .expect("bugfix task exists");
    let language_task = forge_language_task_corpus()
        .into_iter()
        .find(|task| task.task_corpus_id == "rust-cargo-bug-fix-small")
        .expect("rust bugfix language task exists");
    let output_artifact_ref = format!(
        "quarantine://forge/fleet-eval/machine-league/{run_id}/{}.patch",
        spec.artifact_slug
    );
    let transcript_ref = format!(
        "quarantine://forge/fleet-eval/machine-league/{run_id}/{}",
        spec.transcript_name
    );
    let pr_handoff_ref = format!(
        "quarantine://forge/fleet-eval/machine-league/{run_id}/{}-pr.md",
        spec.artifact_slug
    );
    let execution_requested = json_bool_field(body, "execute_live").unwrap_or(false)
        || json_string_field(body, "execution_mode")
            .map(|mode| mode == format!("live_{}", spec.adapter_kind))
            .unwrap_or(false);
    let runtime = observe_machine_runtime_fingerprint(
        Some(kernel),
        spec.runner_id,
        spec.adapter_kind,
        spec.binary_name,
        spec.execution_command,
    );
    let binary_available = runtime.binary_present;
    let live_authorization = if execution_requested
        && adapter_live_execution_enabled(spec.adapter_kind)
        && binary_available
    {
        match authorize_live_machine_trial(body, kernel, spec.runner_id, spec.adapter_kind) {
            Ok(authorization) => Some(authorization),
            Err(reason) => {
                return open_machine_trial_refusal(kernel, spec, &reason, &eval_fixture);
            }
        }
    } else {
        None
    };
    let live_artifacts = if live_authorization.is_some() {
        run_open_machine_live_rust_tax_fixture_trial(
            &run_id,
            body,
            &language_task,
            spec,
            live_authorization
                .as_ref()
                .map(|value| value.max_spend_cents)
                .unwrap_or(0),
        )?
    } else {
        OpenMachineLiveTrialArtifacts::held(
            spec,
            output_artifact_ref,
            transcript_ref,
            pr_handoff_ref,
            &language_task,
            execution_requested,
            binary_available,
        )
    };
    let execution_allowed = live_artifacts.execution_allowed.to_string();
    let execution_performed = live_artifacts.execution_performed.to_string();
    let execution_requested_value = live_artifacts.execution_requested.to_string();
    let check_passed = live_artifacts.check_passed.to_string();
    let runner_exit_code = live_artifacts.runner_exit_code.to_string();
    let check_exit_code = live_artifacts.check_exit_code.to_string();
    let changed_file_count = live_artifacts.changed_file_count.to_string();
    let live_max_spend_cents = live_authorization
        .as_ref()
        .map(|value| value.max_spend_cents)
        .unwrap_or(0)
        .to_string();
    let live_max_tasks = live_authorization
        .as_ref()
        .map(|value| value.max_tasks)
        .unwrap_or(0)
        .to_string();
    let live_max_parallel_agents = live_authorization
        .as_ref()
        .map(|value| value.max_parallel_agents)
        .unwrap_or(0)
        .to_string();
    let live_task_ordinal = live_authorization
        .as_ref()
        .map(|value| value.task_ordinal)
        .unwrap_or(0)
        .to_string();
    let work_packet_budget = format!(
        "max_spend_cents={},max_runtime_ms=300000",
        live_max_spend_cents
    );
    let eval_fixture_snapshot_id = eval_fixture_snapshot_id(&eval_fixture);
    let contamination_status = contamination_status_for_eval_fixture(&eval_fixture);
    let eval_oracle_status = eval_oracle_status(
        live_artifacts.visible_check_passed,
        live_artifacts.hidden_check_observed,
        live_artifacts.hidden_check_passed,
    );
    let visible_check_passed = live_artifacts.visible_check_passed.to_string();
    let hidden_check_observed = live_artifacts.hidden_check_observed.to_string();
    let hidden_check_passed = live_artifacts.hidden_check_passed.to_string();
    let identity = GovernedWriteIdentity::local_demo(spec.actor_id);
    let start_report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: spec.actor_id,
                run_id: &run_id,
                event: "run_started",
                work_item_id: spec.work_item_id,
                detail: &format!(
                    "Forge staged a {} quarantined trial packet",
                    spec.display_name
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &identity,
            &[
                ("repo_id", "machine-league-rust-tax-rounding-fixture"),
                ("repo_root", "."),
                ("repo_primary_language", "rust"),
                ("language_pack_id", language_task.language_pack_id),
                ("selected_checks", MACHINE_LEAGUE_RUST_TAX_VISIBLE_CHECK),
                ("machine_league_eval_fixture", eval_fixture.as_str()),
                ("eval_fixture_snapshot_id", eval_fixture_snapshot_id),
                ("eval_oracle_status", eval_oracle_status),
                ("visible_check_passed", visible_check_passed.as_str()),
                ("hidden_check_observed", hidden_check_observed.as_str()),
                ("hidden_check_passed", hidden_check_passed.as_str()),
                ("contamination_status", contamination_status),
                ("fresh_fixture_required_for_scorecard_acceptance", "true"),
                (
                    "builder_casting_selected_provider_family",
                    spec.adapter_kind,
                ),
                (
                    "builder_casting_selected_model_provider",
                    spec.model_provider,
                ),
                ("builder_casting_selected_model_id", spec.model_id),
                (
                    "machine_option_enabled_by_provider_key",
                    if machine_option_enabled_by_provider_key(
                        global(),
                        &current_tenant_id(),
                        spec.adapter_kind,
                    ) {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("builder_casting_selected_slot", spec.adapter_kind),
                ("machine_league_trial", "true"),
                ("machine_league_run_kind", "machine_league_trial"),
                ("runner_profile_runner_id", spec.runner_id),
                ("runner_profile_runner_kind", "external_machine"),
                ("runner_profile_display_name", spec.display_name),
                ("runner_profile_adapter_kind", spec.adapter_kind),
                (
                    "runner_profile_execution_mode",
                    live_artifacts.runner_execution_mode.as_str(),
                ),
                ("runner_profile_model_profile_id", spec.model_profile_id),
                ("machine_runtime_fingerprint_id", &runtime.fingerprint_id),
                ("machine_runtime_runner_id", &runtime.runner_id),
                ("machine_runtime_adapter_kind", &runtime.adapter_kind),
                ("machine_runtime_binary_name", &runtime.binary_name),
                ("machine_runtime_binary_path", &runtime.binary_path),
                (
                    "machine_runtime_binary_present",
                    if runtime.binary_present {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("machine_runtime_version_command", &runtime.version_command),
                (
                    "machine_runtime_version_observed",
                    &runtime.version_observed,
                ),
                (
                    "machine_runtime_version_raw_output",
                    &runtime.version_raw_output,
                ),
                ("machine_runtime_checksum_sha256", &runtime.checksum_sha256),
                (
                    "machine_runtime_adapter_contract_version",
                    &runtime.adapter_contract_version,
                ),
                (
                    "machine_runtime_command_contract",
                    &runtime.command_contract,
                ),
                ("machine_runtime_drift_status", &runtime.drift_status),
                (
                    "machine_runtime_last_fingerprint_id",
                    &runtime.last_fingerprint_id,
                ),
                (
                    "machine_runtime_compatibility_status",
                    &runtime.compatibility_status,
                ),
                ("quarantine_status", "output_quarantined_pending_mdx_gates"),
                ("quarantine_output_quarantined", "true"),
                ("quarantine_external_output_consumable", "false"),
                (
                    "quarantine_acceptance_gate",
                    "mdx_quality_gates_then_principal_review",
                ),
                (
                    "quarantine_result_projection_route",
                    "/forge/fleet-eval-results/projection.json",
                ),
                ("quarantine_blocked_reason", "pending_mdx_quality_gates"),
                ("league_context_visibility_tier", "product_recommendation"),
                (
                    "league_context_recommendation_rationale",
                    "Forge is gathering first accepted open-machine evidence for rust bug fixes.",
                ),
                (
                    "league_context_fallback_runner_id",
                    "mdx_native_harness_runner",
                ),
                ("league_context_scorecard_evidence_count", "0"),
                (
                    "league_context_quarantine_posture",
                    "external_output_held_until_mdx_gates",
                ),
                ("work_packet_id", &work_packet_id),
                ("work_packet_runner_id", spec.runner_id),
                ("work_packet_runner_profile_id", spec.runner_id),
                ("work_packet_task_class", task.class),
                ("work_packet_complexity_tier", task.complexity_tier),
                (
                    "work_packet_language_pack_id",
                    language_task.language_pack_id,
                ),
                (
                    "work_packet_repo_id",
                    "machine-league-rust-tax-rounding-fixture",
                ),
                ("work_packet_repo_ref", "held_out_fixture_generated_per_run"),
                ("work_packet_allowed_write_scope", "src/lib.rs"),
                (
                    "work_packet_blocked_paths",
                    "provider secrets,production state",
                ),
                (
                    "work_packet_selected_checks",
                    MACHINE_LEAGUE_RUST_TAX_VISIBLE_CHECK,
                ),
                (
                    "work_packet_visible_check",
                    MACHINE_LEAGUE_RUST_TAX_VISIBLE_CHECK,
                ),
                (
                    "work_packet_hidden_check_slot",
                    MACHINE_LEAGUE_RUST_TAX_HIDDEN_CHECK,
                ),
                (
                    "work_packet_standards_source_fingerprints",
                    "AGENTS.md=fnv1a64:0000000000000000",
                ),
                (
                    "work_packet_redaction_policy",
                    "mdx_context_redaction_required",
                ),
                ("work_packet_budget", work_packet_budget.as_str()),
                ("work_packet_expiry", "single_trial"),
                (
                    "work_packet_stop_conditions",
                    "manual_stop,budget_exhausted,quality_gate_failure",
                ),
                ("work_packet_transcript_required", "true"),
                (
                    "work_packet_artifact_namespace",
                    "quarantine://forge/fleet-eval/",
                ),
                (
                    "work_packet_acceptance_criteria",
                    "visible and hidden Rust tests pass without weakening the fixture tests",
                ),
                (
                    "adapter_execution_requested",
                    execution_requested_value.as_str(),
                ),
                ("adapter_execution_performed", execution_performed.as_str()),
                ("adapter_execution_allowed", execution_allowed.as_str()),
                (
                    "adapter_execution_status",
                    live_artifacts.execution_status.as_str(),
                ),
                (
                    "live_run_approval_id",
                    live_authorization
                        .as_ref()
                        .map(|value| value.approval_id.as_str())
                        .unwrap_or(""),
                ),
                (
                    "live_run_approval_receipt_id",
                    live_authorization
                        .as_ref()
                        .map(|value| value.approval_receipt_id.as_str())
                        .unwrap_or(""),
                ),
                (
                    "runner_execution_clearance_receipt_id",
                    live_authorization
                        .as_ref()
                        .map(|value| value.clearance_receipt_id.as_str())
                        .unwrap_or(""),
                ),
                (
                    "closed_runner_clearance_receipt_id",
                    live_authorization
                        .as_ref()
                        .map(|value| value.clearance_receipt_id.as_str())
                        .unwrap_or(""),
                ),
                (
                    "live_execution_provider",
                    live_authorization
                        .as_ref()
                        .map(|value| value.provider)
                        .unwrap_or(""),
                ),
                ("live_run_max_spend_cents", live_max_spend_cents.as_str()),
                ("live_run_max_tasks", live_max_tasks.as_str()),
                (
                    "live_run_max_parallel_agents",
                    live_max_parallel_agents.as_str(),
                ),
                ("live_run_task_ordinal", live_task_ordinal.as_str()),
                (
                    "live_execution_authority_scope",
                    "single_quarantined_fixture_trial",
                ),
                ("adapter_kind", spec.adapter_kind),
                ("adapter_binary_name", spec.binary_name),
                ("adapter_live_env_required", spec.live_env_key),
                ("adapter_invocation_marker", spec.execution_marker),
                (
                    "adapter_worktree_path",
                    live_artifacts.worktree_path.as_str(),
                ),
                ("adapter_runner_exit_code", runner_exit_code.as_str()),
                (
                    "adapter_runner_timed_out",
                    if live_artifacts.runner_timed_out {
                        "true"
                    } else {
                        "false"
                    },
                ),
                (
                    "adapter_runner_auth_blocked",
                    if live_artifacts.runner_auth_blocked {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("adapter_check_exit_code", check_exit_code.as_str()),
                (
                    "adapter_check_timed_out",
                    if live_artifacts.check_timed_out {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("adapter_check_passed", check_passed.as_str()),
                ("adapter_changed_file_count", changed_file_count.as_str()),
                ("language_task_corpus_id", language_task.task_corpus_id),
                ("language_task_alignment_status", "aligned"),
                (
                    "language_task_alignment_source",
                    "machine_league_open_machine_trial_contract",
                ),
                ("language_task_class", language_task.task_class),
                (
                    "language_task_complexity_tier",
                    language_task.complexity_tier,
                ),
                ("language_task_visible_check", language_task.visible_check),
                (
                    "language_task_hidden_check_slot",
                    language_task.hidden_check_slot,
                ),
                (
                    "language_task_artifact_noise_expected",
                    language_task.artifact_noise_expected,
                ),
                (
                    "language_task_required_principal_review_gates",
                    language_task.required_principal_review_gates,
                ),
                (
                    "language_task_engineering_facets",
                    language_task_engineering_facets(&language_task),
                ),
                (
                    "language_task_evaluation_oracle",
                    language_task_evaluation_oracle(&language_task),
                ),
                (
                    "language_task_human_timebox_minutes",
                    &language_task_human_timebox_minutes(&language_task).to_string(),
                ),
                (
                    "language_task_contamination_policy",
                    language_task_contamination_policy(&language_task),
                ),
                (
                    "language_task_alignment_grants_execution_authority",
                    "false",
                ),
            ],
        )
        .map_err(|error| error.message())?;
    kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: spec.actor_id,
            run_id: &run_id,
            event: "tool_executed",
            work_item_id: spec.work_item_id,
            detail: &live_artifacts.tool_detail,
            turn: 1,
            input_tokens: 0,
            output_tokens: 0,
        })
        .map_err(|error| error.message())?;
    kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: spec.actor_id,
            run_id: &run_id,
            event: "evidence_appended",
            work_item_id: spec.work_item_id,
            detail: &live_artifacts.evidence_detail,
            turn: 1,
            input_tokens: 0,
            output_tokens: 0,
        })
        .map_err(|error| error.message())?;
    let consumption_receipt_id = record_external_output_consumption_boundary(
        kernel,
        &identity,
        ExternalOutputBoundary {
            actor_id: spec.actor_id,
            run_id: &run_id,
            work_item_id: spec.work_item_id,
            runner_id: spec.runner_id,
            adapter_kind: spec.adapter_kind,
            output_artifact_ref: &live_artifacts.output_artifact_ref,
        },
        live_authorization.as_ref(),
    )?;
    let result = kernel
        .ingest_forge_fleet_eval_result_local_with_identity(
            ForgeFleetEvalResultSubmission {
                tenant_id: "local_tenant",
                actor_id: spec.actor_id,
                actor_role: "worker",
                submission_id: &format!("{run_id}_quarantined_result"),
                benchmark_task_id: task.task_id,
                language_task_corpus_id: language_task.task_corpus_id,
                language_pack_id: language_task.language_pack_id,
                repo_family: language_task.repo_family,
                runner_id: spec.runner_id,
                model_profile_id: spec.model_profile_id,
                output_artifact_ref: &live_artifacts.output_artifact_ref,
                output_artifact_sha256: &live_artifacts.output_artifact_sha256,
                transcript_ref: &live_artifacts.transcript_ref,
                claimed_check: &live_artifacts.claimed_check,
                hidden_check_slot: language_task.hidden_check_slot,
                artifact_noise_expected: language_task.artifact_noise_expected,
                principal_review_gate_results: &live_artifacts.principal_review_gate_results,
                standards_source_fingerprints: "AGENTS.md=fnv1a64:0000000000000000",
                artifact_filter_summary: &live_artifacts.artifact_filter_summary,
                diff_language_pack_impact: &live_artifacts.diff_language_pack_impact,
                principal_engineer_verdict: "pending_principal_engineer_review",
                pr_handoff_ref: &live_artifacts.pr_handoff_ref,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    if live_artifacts.execution_performed {
        let assessment = live_cli_trial_quality_assessment(
            live_artifacts.runner_exit_code,
            live_artifacts.check_passed,
            live_artifacts.changed_file_count,
        );
        record_run_eval_quality_gate_event(
            kernel,
            &run_id,
            &assessment,
            &result.result_receipt_id,
        )?;
    }
    kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: spec.actor_id,
            run_id: &run_id,
            event: "run_finished",
            work_item_id: spec.work_item_id,
            detail: &live_artifacts.finish_detail,
            turn: 2,
            input_tokens: 0,
            output_tokens: 0,
        })
        .map_err(|error| error.message())?;
    let response_status = if live_artifacts.execution_performed {
        format!(
            "{}_EXECUTED_QUARANTINED_PENDING_MDX_GATES",
            spec.status_prefix
        )
    } else {
        format!("{}_TRIAL_PACKET_RECORDED_QUARANTINED", spec.status_prefix)
    };
    Ok(format!(
        r#"{{"name":{},"status":{},"run_id":{},"run_started_receipt_id":{},"consumption_receipt_id":{},"work_packet_id":{},"runner_id":{},"runner_display_name":{},"runtime_fingerprint":{},"stream_route":{},"run_projection_route":"/forge/runs/projection.json","result_projection_route":"/forge/fleet-eval-results/projection.json","result_receipt_id":{},"output_artifact_ref":{},"output_artifact_sha256":{},"transcript_ref":{},"worktree_path":{},"eval_fixture_snapshot_id":{},"eval_oracle_status":{},"visible_check_passed":{},"hidden_check_observed":{},"hidden_check_passed":{},"contamination_status":{},"adapter_execution_requested":{},"adapter_execution_allowed":{},"adapter_execution_performed":{},"runner_exit_code":{},"runner_timed_out":{},"runner_auth_blocked":{},"check_exit_code":{},"check_timed_out":{},"check_passed":{},"changed_file_count":{},"external_output_consumption_status":"denied_quarantined_pending_distillation","external_output_consumable":false,"output_quarantined":true,"mdx_quality_gates_required":true,"accepted_for_scoreboard":false,"blocked_reason":"pending_mdx_quality_gates","production_write_allowed":false}}"#,
        json_string_literal(spec.route_name),
        json_string_literal(&response_status),
        json_string_literal(&run_id),
        json_string_literal(&start_report.receipt_id),
        json_string_literal(&consumption_receipt_id),
        json_string_literal(&work_packet_id),
        json_string_literal(spec.runner_id),
        json_string_literal(spec.display_name),
        machine_runtime_json(&runtime),
        json_string_literal(&format!("/forge/runs/stream?run_id={run_id}")),
        json_string_literal(&result.result_receipt_id),
        json_string_literal(&live_artifacts.output_artifact_ref),
        json_string_literal(&live_artifacts.output_artifact_sha256),
        json_string_literal(&live_artifacts.transcript_ref),
        json_string_literal(&live_artifacts.worktree_path),
        json_string_literal(eval_fixture_snapshot_id),
        json_string_literal(eval_oracle_status),
        live_artifacts.visible_check_passed,
        live_artifacts.hidden_check_observed,
        live_artifacts.hidden_check_passed,
        json_string_literal(contamination_status),
        live_artifacts.execution_requested,
        live_artifacts.execution_allowed,
        live_artifacts.execution_performed,
        live_artifacts.runner_exit_code,
        live_artifacts.runner_timed_out,
        live_artifacts.runner_auth_blocked,
        live_artifacts.check_exit_code,
        live_artifacts.check_timed_out,
        live_artifacts.check_passed,
        live_artifacts.changed_file_count,
    ))
}

fn open_machine_trial_refusal(
    kernel: &mut MdxKernel,
    spec: OpenMachineTrialSpec,
    reason: &str,
    eval_fixture: &str,
) -> Result<String, String> {
    let run_id = format!("forge_machine_league_{}_trial_refusal", spec.adapter_slug);
    let detail = format!("{}_trial_refused reason={}", spec.adapter_kind, reason);
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: spec.actor_id,
                run_id: &run_id,
                event: "evidence_appended",
                work_item_id: spec.work_item_id,
                detail: &detail,
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo(spec.actor_id),
            &[
                ("machine_league_trial_status", "refused"),
                ("machine_league_eval_fixture", eval_fixture),
                ("machine_league_trial_refusal_reason", reason),
                ("runner_id", spec.runner_id),
                ("accepted_for_scoreboard", "false"),
                ("external_output_consumable", "false"),
                ("eval_grants_execution_authority", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":{},"status":"REFUSED","reason":{},"eval_fixture":{},"trial_refusal_receipt_id":{},"accepted_for_scoreboard":false,"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(spec.route_name),
        json_string_literal(reason),
        json_string_literal(eval_fixture),
        json_string_literal(&report.receipt_id)
    ))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NativeExecutionMode {
    // Run the real Forge loop against the held-out fixture.
    Harness,
    // Replay the known deterministic patch as an honestly-labeled baseline.
    Scripted,
}

impl NativeExecutionMode {
    // The label stamped on every receipt and echoed in the response. Scoreboard
    // counters exclude the scripted_baseline label so a rehearsal never scores.
    fn label(self) -> &'static str {
        match self {
            NativeExecutionMode::Harness => "harness",
            NativeExecutionMode::Scripted => "scripted_baseline",
        }
    }
}

fn native_execution_mode_request(body: &str) -> NativeExecutionMode {
    match json_string_field(body, "native_execution")
        .unwrap_or_default()
        .trim()
    {
        "scripted" => NativeExecutionMode::Scripted,
        _ => NativeExecutionMode::Harness,
    }
}

fn render_native_trial_json(body: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<String, String> {
    match native_execution_mode_request(body) {
        NativeExecutionMode::Harness => render_native_trial_harness(body, kernel),
        NativeExecutionMode::Scripted => {
            // The scripted baseline does no process work under the lock beyond
            // the same cargo checks the old path always ran; take one write
            // guard and delegate to the shared recorder.
            let mut kernel = kernel
                .write()
                .map_err(|_| "kernel lock poisoned".to_string())?;
            render_native_trial_scripted(body, &mut kernel)
        }
    }
}

// The harness path prepares the fixture, runs the REAL Forge loop with NO kernel
// lock held (it takes its own short locks and would deadlock under a guard),
// then records each trial receipt under a short write lock.
fn render_native_trial_harness(
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<String, String> {
    let eval_fixture = json_string_field(body, "eval_fixture")
        .unwrap_or_else(|| MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID.to_string());
    let Some(fixture) = machine_league_eval_fixture(&eval_fixture) else {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        return native_trial_refusal(
            &mut kernel,
            "native fixture runner requires a registered Machine League eval fixture",
            &eval_fixture,
        );
    };
    if !json_bool_field(body, "execute_live").unwrap_or(false) {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        return native_trial_refusal(
            &mut kernel,
            "set execute_live=true to run the local native fixture comparison",
            &eval_fixture,
        );
    }
    if requested_execution_backend(body).kind() == ForgeExecutionBackendKind::HostedSandbox {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        return native_trial_refusal(
            &mut kernel,
            "hosted sandbox backend is not configured for native fixture execution yet",
            &eval_fixture,
        );
    }
    // Short read lock: derive the trial number and any learned strategy hint.
    let (run_number, inferred_strategy) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        let run_number = kernel
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .iter()
            .filter(|receipt| payload_value(receipt, "machine_league_trial") == "true")
            .count()
            + 1;
        (
            run_number,
            inferred_native_strategy_profile(&kernel, fixture),
        )
    };
    let run_id = json_string_field(body, "run_id")
        .unwrap_or_else(|| format!("forge_machine_league_native_trial_{run_number:06}"));
    let work_packet_id = json_string_field(body, "work_packet_id")
        .unwrap_or_else(|| format!("work_packet_{run_id}"));
    let task = forge_fleet_benchmark_tasks()
        .into_iter()
        .find(|task| task.task_id == "bugfix_small")
        .expect("bugfix task exists");
    let language_task = forge_language_task_corpus()
        .into_iter()
        .find(|task| task.task_corpus_id == "rust-cargo-bug-fix-small")
        .expect("rust bugfix language task exists");
    let native_strategy_profile = json_string_field(body, "native_strategy_profile")
        .or(inferred_strategy)
        .unwrap_or_else(|| fixture.native_baseline_strategy.to_string());
    // The real loop runs with NO kernel lock held.
    let native_artifacts = run_native_harness_fixture_trial(
        &run_id,
        body,
        &language_task,
        fixture,
        &native_strategy_profile,
        kernel,
    )?;
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    record_native_trial_receipts(
        &mut kernel,
        NativeTrialReceiptInput {
            run_id: &run_id,
            work_packet_id: &work_packet_id,
            task: &task,
            language_task: &language_task,
            fixture,
            eval_fixture: &eval_fixture,
            native_strategy_profile: &native_strategy_profile,
            native_execution_mode: NativeExecutionMode::Harness.label(),
            artifacts: &native_artifacts,
        },
    )
}

// The scripted baseline: replay the known deterministic patch. Runs under a
// held write guard (both the route dispatcher and fleet/face-off composition
// call it that way), so it takes no lock-free loop.
fn render_native_trial_scripted(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let eval_fixture = json_string_field(body, "eval_fixture")
        .unwrap_or_else(|| MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID.to_string());
    let Some(fixture) = machine_league_eval_fixture(&eval_fixture) else {
        return native_trial_refusal(
            kernel,
            "native fixture runner requires a registered Machine League eval fixture",
            &eval_fixture,
        );
    };
    let execution_requested = json_bool_field(body, "execute_live").unwrap_or(false);
    if !execution_requested {
        return native_trial_refusal(
            kernel,
            "set execute_live=true to run the local native fixture comparison",
            &eval_fixture,
        );
    }
    if requested_execution_backend(body).kind() == ForgeExecutionBackendKind::HostedSandbox {
        return native_trial_refusal(
            kernel,
            "hosted sandbox backend is not configured for native fixture execution yet",
            &eval_fixture,
        );
    }
    let run_number = kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| payload_value(receipt, "machine_league_trial") == "true")
        .count()
        + 1;
    let run_id = json_string_field(body, "run_id")
        .unwrap_or_else(|| format!("forge_machine_league_native_trial_{run_number:06}"));
    let work_packet_id = json_string_field(body, "work_packet_id")
        .unwrap_or_else(|| format!("work_packet_{run_id}"));
    let task = forge_fleet_benchmark_tasks()
        .into_iter()
        .find(|task| task.task_id == "bugfix_small")
        .expect("bugfix task exists");
    let language_task = forge_language_task_corpus()
        .into_iter()
        .find(|task| task.task_corpus_id == "rust-cargo-bug-fix-small")
        .expect("rust bugfix language task exists");
    let native_strategy_profile = json_string_field(body, "native_strategy_profile")
        .or_else(|| inferred_native_strategy_profile(kernel, fixture))
        .unwrap_or_else(|| fixture.native_baseline_strategy.to_string());
    let native_artifacts = run_native_fixture_trial(
        &run_id,
        body,
        &language_task,
        fixture,
        &native_strategy_profile,
    )?;
    record_native_trial_receipts(
        kernel,
        NativeTrialReceiptInput {
            run_id: &run_id,
            work_packet_id: &work_packet_id,
            task: &task,
            language_task: &language_task,
            fixture,
            eval_fixture: &eval_fixture,
            native_strategy_profile: &native_strategy_profile,
            native_execution_mode: NativeExecutionMode::Scripted.label(),
            artifacts: &native_artifacts,
        },
    )
}

struct NativeTrialReceiptInput<'a> {
    run_id: &'a str,
    work_packet_id: &'a str,
    task: &'a FleetBenchmarkTask,
    language_task: &'a ForgeLanguageTaskCorpusEntry,
    fixture: MachineLeagueEvalFixture,
    eval_fixture: &'a str,
    native_strategy_profile: &'a str,
    native_execution_mode: &'a str,
    artifacts: &'a NativeLiveTrialArtifacts,
}

// Record the shared trial receipt chain for both solvers. The evidence lists
// and response carry native_execution_mode so a scripted rehearsal is legible
// and stays off the scoreboard.
fn record_native_trial_receipts(
    kernel: &mut MdxKernel,
    input: NativeTrialReceiptInput,
) -> Result<String, String> {
    let native_artifacts = input.artifacts;
    let eval_fixture = input.eval_fixture.to_string();
    let run_id = input.run_id.to_string();
    let work_packet_id = input.work_packet_id.to_string();
    let task = *input.task;
    let language_task = *input.language_task;
    let fixture = input.fixture;
    let native_strategy_profile = input.native_strategy_profile.to_string();
    let native_exit_code = native_artifacts.native_exit_code.to_string();
    let check_exit_code = native_artifacts.check_exit_code.to_string();
    let changed_file_count = native_artifacts.changed_file_count.to_string();
    let check_passed = native_artifacts.check_passed.to_string();
    let eval_fixture_snapshot_id = eval_fixture_snapshot_id(&eval_fixture);
    let contamination_status = contamination_status_for_eval_fixture(&eval_fixture);
    let eval_oracle_status = eval_oracle_status(
        native_artifacts.visible_check_passed,
        native_artifacts.hidden_check_observed,
        native_artifacts.hidden_check_passed,
    );
    let visible_check_passed = native_artifacts.visible_check_passed.to_string();
    let hidden_check_observed = native_artifacts.hidden_check_observed.to_string();
    let hidden_check_passed = native_artifacts.hidden_check_passed.to_string();
    let identity = GovernedWriteIdentity::local_demo("agent:forge_native");
    let start_report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:forge_native",
                run_id: &run_id,
                event: "run_started",
                work_item_id: "machine_league_native_trial",
                detail: "Forge staged an MDx native quarantined fixture trial",
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &identity,
            &[
                ("repo_id", fixture.repo_id),
                ("repo_root", "."),
                ("repo_primary_language", "rust"),
                ("language_pack_id", language_task.language_pack_id),
                ("selected_checks", fixture.visible_check),
                ("machine_league_eval_fixture", eval_fixture.as_str()),
                ("machine_league_eval_fixture_family", fixture.family_id),
                (
                    "machine_league_eval_fixture_sibling",
                    fixture.sibling_fixture_id,
                ),
                ("machine_league_transferable_edge", fixture.transferable_edge),
                ("eval_fixture_snapshot_id", eval_fixture_snapshot_id),
                ("eval_oracle_status", eval_oracle_status),
                ("visible_check_passed", visible_check_passed.as_str()),
                ("hidden_check_observed", hidden_check_observed.as_str()),
                ("hidden_check_passed", hidden_check_passed.as_str()),
                ("contamination_status", contamination_status),
                (
                    "fresh_fixture_required_for_scorecard_acceptance",
                    "true",
                ),
                ("builder_casting_selected_provider_family", "mdx"),
                (
                    "builder_casting_selected_model_profile_id",
                    "mdx_native_local_model_profile",
                ),
                ("builder_casting_selected_model_id", "mdx-native-fixture-solver"),
                ("builder_casting_selected_slot", "MDX_NATIVE"),
                ("machine_league_trial", "true"),
                ("native_execution_mode", input.native_execution_mode),
                ("machine_league_run_kind", "machine_league_native_trial"),
                ("runner_profile_runner_id", "mdx_native_harness_runner"),
                ("runner_profile_runner_kind", "mdx_native"),
                ("runner_profile_display_name", "Forge native builder"),
                ("runner_profile_adapter_kind", "mdx_native_harness"),
                (
                    "runner_profile_execution_mode",
                    "native_fixture_solver_quarantined",
                ),
                ("native_strategy_profile", &native_strategy_profile),
                (
                    "runner_profile_model_profile_id",
                    "mdx_native_local_model_profile",
                ),
                ("quarantine_status", "output_quarantined_pending_mdx_gates"),
                ("quarantine_output_quarantined", "true"),
                ("quarantine_external_output_consumable", "false"),
                (
                    "quarantine_acceptance_gate",
                    "mdx_quality_gates_then_principal_review",
                ),
                (
                    "quarantine_result_projection_route",
                    "/forge/fleet-eval-results/projection.json",
                ),
                ("quarantine_blocked_reason", "pending_mdx_quality_gates"),
                ("league_context_visibility_tier", "operator_evidence"),
                (
                    "league_context_recommendation_rationale",
                    "Forge is running its native fixture path on the same held-out task before claiming a comparison.",
                ),
                (
                    "league_context_fallback_runner_id",
                    "codex_cli_external_worker",
                ),
                ("league_context_scorecard_evidence_count", "0"),
                (
                    "league_context_quarantine_posture",
                    "native_fixture_output_held_until_mdx_gates",
                ),
                ("work_packet_id", &work_packet_id),
                ("work_packet_runner_id", "mdx_native_harness_runner"),
                ("work_packet_runner_profile_id", "mdx_native_harness_runner"),
                ("work_packet_task_class", task.class),
                ("work_packet_complexity_tier", task.complexity_tier),
                (
                    "work_packet_language_pack_id",
                    language_task.language_pack_id,
                ),
                ("work_packet_repo_id", fixture.repo_id),
                ("work_packet_fixture_family", fixture.family_id),
                ("work_packet_repo_ref", "held_out_fixture_generated_per_run"),
                ("work_packet_allowed_write_scope", fixture.allowed_scope),
                (
                    "work_packet_blocked_paths",
                    "provider secrets,production state",
                ),
                ("work_packet_selected_checks", fixture.visible_check),
                ("work_packet_visible_check", fixture.visible_check),
                ("work_packet_hidden_check_slot", fixture.hidden_check),
                (
                    "work_packet_standards_source_fingerprints",
                    "AGENTS.md=fnv1a64:0000000000000000",
                ),
                (
                    "work_packet_redaction_policy",
                    "mdx_context_redaction_required",
                ),
                (
                    "work_packet_budget",
                    "max_spend_cents=0,max_runtime_ms=300000",
                ),
                ("work_packet_expiry", "single_trial"),
                (
                    "work_packet_stop_conditions",
                    "manual_stop,budget_exhausted,quality_gate_failure",
                ),
                ("work_packet_transcript_required", "true"),
                (
                    "work_packet_artifact_namespace",
                    "quarantine://forge/fleet-eval/",
                ),
                (
                    "work_packet_acceptance_criteria",
                    fixture.acceptance_criteria,
                ),
                ("adapter_execution_requested", "true"),
                ("adapter_execution_performed", "true"),
                ("adapter_execution_allowed", "true"),
                ("adapter_execution_status", native_artifacts.execution_status),
                (
                    "adapter_worktree_path",
                    native_artifacts.worktree_path.as_str(),
                ),
                ("adapter_native_exit_code", native_exit_code.as_str()),
                (
                    "adapter_native_timed_out",
                    if native_artifacts.native_timed_out {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("adapter_check_exit_code", check_exit_code.as_str()),
                (
                    "adapter_check_timed_out",
                    if native_artifacts.check_timed_out {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("adapter_check_passed", check_passed.as_str()),
                ("adapter_changed_file_count", changed_file_count.as_str()),
                ("language_task_corpus_id", language_task.task_corpus_id),
                ("language_task_alignment_status", "aligned"),
                (
                    "language_task_alignment_source",
                    "machine_league_native_trial_contract",
                ),
                ("language_task_class", language_task.task_class),
                (
                    "language_task_complexity_tier",
                    language_task.complexity_tier,
                ),
                ("language_task_visible_check", language_task.visible_check),
                (
                    "language_task_hidden_check_slot",
                    fixture.hidden_check_slot,
                ),
                (
                    "language_task_artifact_noise_expected",
                    language_task.artifact_noise_expected,
                ),
                (
                    "language_task_required_principal_review_gates",
                    language_task.required_principal_review_gates,
                ),
                (
                    "language_task_engineering_facets",
                    language_task_engineering_facets(&language_task),
                ),
                (
                    "language_task_evaluation_oracle",
                    language_task_evaluation_oracle(&language_task),
                ),
                (
                    "language_task_human_timebox_minutes",
                    &language_task_human_timebox_minutes(&language_task).to_string(),
                ),
                (
                    "language_task_contamination_policy",
                    language_task_contamination_policy(&language_task),
                ),
                (
                    "language_task_alignment_grants_execution_authority",
                    "false",
                ),
            ],
        )
        .map_err(|error| error.message())?;
    kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:forge_native",
            run_id: &run_id,
            event: "tool_executed",
            work_item_id: "machine_league_native_trial",
            detail: &native_artifacts.tool_detail,
            turn: 1,
            input_tokens: 0,
            output_tokens: 0,
        })
        .map_err(|error| error.message())?;
    kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:forge_native",
            run_id: &run_id,
            event: "evidence_appended",
            work_item_id: "machine_league_native_trial",
            detail: &native_artifacts.evidence_detail,
            turn: 1,
            input_tokens: 0,
            output_tokens: 0,
        })
        .map_err(|error| error.message())?;
    let result = kernel
        .ingest_forge_fleet_eval_result_local_with_identity(
            ForgeFleetEvalResultSubmission {
                tenant_id: "local_tenant",
                actor_id: "agent:forge_native",
                actor_role: "worker",
                submission_id: &format!("{run_id}_quarantined_result"),
                benchmark_task_id: task.task_id,
                language_task_corpus_id: language_task.task_corpus_id,
                language_pack_id: language_task.language_pack_id,
                repo_family: language_task.repo_family,
                runner_id: "mdx_native_harness_runner",
                model_profile_id: "mdx_native_local_model_profile",
                output_artifact_ref: &native_artifacts.output_artifact_ref,
                output_artifact_sha256: &native_artifacts.output_artifact_sha256,
                transcript_ref: &native_artifacts.transcript_ref,
                claimed_check: &native_artifacts.claimed_check,
                hidden_check_slot: language_task.hidden_check_slot,
                artifact_noise_expected: language_task.artifact_noise_expected,
                principal_review_gate_results: &native_artifacts.principal_review_gate_results,
                standards_source_fingerprints: "AGENTS.md=fnv1a64:0000000000000000",
                artifact_filter_summary: &native_artifacts.artifact_filter_summary,
                diff_language_pack_impact: &native_artifacts.diff_language_pack_impact,
                principal_engineer_verdict: "pending_principal_engineer_review",
                pr_handoff_ref: &native_artifacts.pr_handoff_ref,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    let assessment = RunEvalQualityGateAssessment {
        status: if native_artifacts.check_passed && native_artifacts.changed_file_count > 0 {
            "READY_FOR_PRINCIPAL_REVIEW"
        } else {
            "BLOCKED_BEFORE_PRINCIPAL_REVIEW"
        },
        passed_gates: if native_artifacts.check_passed && native_artifacts.changed_file_count > 0 {
            DEFAULT_PRINCIPAL_REVIEW_GATE_IDS
                .split(',')
                .filter_map(|gate| gate.split(':').next())
                .collect()
        } else {
            Vec::new()
        },
        failed_gates: if native_artifacts.check_passed && native_artifacts.changed_file_count > 0 {
            Vec::new()
        } else {
            vec!["native_fixture_solver_passed_with_change"]
        },
        gates_passed: native_artifacts.check_passed && native_artifacts.changed_file_count > 0,
        can_be_scored_after_principal_review: native_artifacts.check_passed
            && native_artifacts.changed_file_count > 0,
    };
    record_run_eval_quality_gate_event(kernel, &run_id, &assessment, &result.result_receipt_id)?;
    kernel
        .record_forge_run_event(ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "agent:forge_native",
            run_id: &run_id,
            event: "run_finished",
            work_item_id: "machine_league_native_trial",
            detail: &native_artifacts.finish_detail,
            turn: 2,
            input_tokens: 0,
            output_tokens: 0,
        })
        .map_err(|error| error.message())?;
    // Scripted rehearsals are never scoreboard evidence; say so in the body.
    let scripted_baseline_flag = if input.native_execution_mode == "scripted_baseline" {
        r#","scripted_baseline_not_scoreboard_evidence":true"#
    } else {
        ""
    };
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-native-trial-local-post","status":{},"native_execution_mode":{}{},"run_id":{},"run_started_receipt_id":{},"work_packet_id":{},"runner_id":"mdx_native_harness_runner","runner_display_name":"Forge native builder","native_strategy_profile":{},"eval_fixture":{},"eval_fixture_family":{},"eval_fixture_sibling":{},"transferable_edge":{},"stream_route":{},"run_projection_route":"/forge/runs/projection.json","result_projection_route":"/forge/fleet-eval-results/projection.json","result_receipt_id":{},"output_artifact_ref":{},"output_artifact_sha256":{},"transcript_ref":{},"worktree_path":{},"eval_fixture_snapshot_id":{},"eval_oracle_status":{},"visible_check_passed":{},"hidden_check_observed":{},"hidden_check_passed":{},"contamination_status":{},"adapter_execution_requested":true,"adapter_execution_allowed":true,"adapter_execution_performed":{},"native_exit_code":{},"native_timed_out":{},"check_exit_code":{},"check_timed_out":{},"check_passed":{},"changed_file_count":{},"external_output_consumable":false,"output_quarantined":true,"mdx_quality_gates_required":true,"accepted_for_scoreboard":false,"blocked_reason":"pending_mdx_quality_gates","production_write_allowed":false}}"#,
        json_string_literal(native_artifacts.execution_status),
        json_string_literal(input.native_execution_mode),
        scripted_baseline_flag,
        json_string_literal(&run_id),
        json_string_literal(&start_report.receipt_id),
        json_string_literal(&work_packet_id),
        json_string_literal(&native_strategy_profile),
        json_string_literal(fixture.id),
        json_string_literal(fixture.family_id),
        json_string_literal(fixture.sibling_fixture_id),
        json_string_literal(fixture.transferable_edge),
        json_string_literal(&format!("/forge/runs/stream?run_id={run_id}")),
        json_string_literal(&result.result_receipt_id),
        json_string_literal(&native_artifacts.output_artifact_ref),
        json_string_literal(&native_artifacts.output_artifact_sha256),
        json_string_literal(&native_artifacts.transcript_ref),
        json_string_literal(&native_artifacts.worktree_path),
        json_string_literal(eval_fixture_snapshot_id),
        json_string_literal(eval_oracle_status),
        native_artifacts.visible_check_passed,
        native_artifacts.hidden_check_observed,
        native_artifacts.hidden_check_passed,
        json_string_literal(contamination_status),
        native_artifacts.execution_performed,
        native_artifacts.native_exit_code,
        native_artifacts.native_timed_out,
        native_artifacts.check_exit_code,
        native_artifacts.check_timed_out,
        native_artifacts.check_passed,
        native_artifacts.changed_file_count,
    ))
}

fn native_trial_refusal(
    kernel: &mut MdxKernel,
    reason: &str,
    eval_fixture: &str,
) -> Result<String, String> {
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:forge_native",
                run_id: "forge_machine_league_native_trial_refusal",
                event: "evidence_appended",
                work_item_id: "machine_league_native_trial_refusal",
                detail: &format!("native_fixture_trial_refused reason={}", reason),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:forge_native"),
            &[
                ("machine_league_trial_status", "refused"),
                ("machine_league_eval_fixture", eval_fixture),
                ("machine_league_trial_refusal_reason", reason),
                ("runner_id", "mdx_native_harness_runner"),
                ("accepted_for_scoreboard", "false"),
                ("external_output_consumable", "false"),
                ("eval_grants_execution_authority", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-native-trial-local-post","status":"REFUSED","reason":{},"eval_fixture":{},"trial_refusal_receipt_id":{},"accepted_for_scoreboard":false,"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(reason),
        json_string_literal(eval_fixture),
        json_string_literal(&report.receipt_id)
    ))
}

struct AcceptedScorecardRun {
    run_id: String,
    runner_id: String,
    task_class: String,
    language_pack_id: String,
    eval_fixture: String,
    eval_fixture_snapshot_id: String,
    eval_oracle_status: String,
    contamination_status: String,
    total_score: u32,
    review_receipt_id: String,
}

fn render_pairwise_comparison_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let external_run_id = json_string_field(body, "external_run_id").unwrap_or_default();
    let native_run_id = json_string_field(body, "native_run_id").unwrap_or_default();
    if external_run_id.trim().is_empty() {
        return pairwise_comparison_refusal(kernel, "external_run_id is required", "", "");
    }
    if native_run_id.trim().is_empty() {
        return pairwise_comparison_refusal(
            kernel,
            "native_run_id is required",
            &external_run_id,
            "",
        );
    }
    let external = match accepted_scorecard_run_for(kernel, &external_run_id) {
        Some(run) => run,
        None => {
            return pairwise_comparison_refusal(
                kernel,
                &format!(
                    "external run {} is not accepted scoreboard evidence",
                    external_run_id.trim()
                ),
                &external_run_id,
                &native_run_id,
            );
        }
    };
    let native = match accepted_scorecard_run_for(kernel, &native_run_id) {
        Some(run) => run,
        None => {
            return pairwise_comparison_refusal(
                kernel,
                &format!(
                    "native run {} is not accepted scoreboard evidence",
                    native_run_id.trim()
                ),
                &external_run_id,
                &native_run_id,
            );
        }
    };
    if external.runner_id == "mdx_native_harness_runner" {
        return pairwise_comparison_refusal(
            kernel,
            "external_run_id must name an external machine run",
            &external_run_id,
            &native_run_id,
        );
    }
    if native.runner_id != "mdx_native_harness_runner" {
        return pairwise_comparison_refusal(
            kernel,
            "native_run_id must name an accepted Forge native run",
            &external_run_id,
            &native_run_id,
        );
    }
    if external.task_class != native.task_class {
        return pairwise_comparison_refusal(
            kernel,
            "pairwise comparison requires the same task class",
            &external_run_id,
            &native_run_id,
        );
    }
    if external.eval_fixture.trim().is_empty()
        || external.eval_fixture != native.eval_fixture
        || external.eval_fixture_snapshot_id != native.eval_fixture_snapshot_id
    {
        return pairwise_comparison_refusal(
            kernel,
            "pairwise comparison requires the same accepted fixture snapshot",
            &external_run_id,
            &native_run_id,
        );
    }
    if external.eval_oracle_status != "visible_and_hidden_checks_passed"
        || native.eval_oracle_status != "visible_and_hidden_checks_passed"
        || external.contamination_status != "fresh_held_out_fixture"
        || native.contamination_status != "fresh_held_out_fixture"
    {
        return pairwise_comparison_refusal(
            kernel,
            "pairwise comparison requires fresh visible and hidden oracle evidence for both runs",
            &external_run_id,
            &native_run_id,
        );
    }
    let outcome = if external.total_score > native.total_score {
        "external_win"
    } else if native.total_score > external.total_score {
        "native_win"
    } else {
        "tie"
    };
    let distillation_candidate_allowed = outcome == "external_win";
    let comparison_id = json_string_field(body, "comparison_id").unwrap_or_else(|| {
        format!(
            "pairwise_{}_vs_{}",
            external.run_id.replace('-', "_"),
            native.run_id.replace('-', "_")
        )
    });
    let distillation_allowed = distillation_candidate_allowed.to_string();
    let external_total_score = external.total_score.to_string();
    let native_total_score = native.total_score.to_string();
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:forge_eval",
                run_id: &comparison_id,
                event: "evidence_appended",
                work_item_id: "machine_league_pairwise_comparison",
                detail: &format!(
                    "machine_league_pairwise_compared outcome={} external_run_id={} native_run_id={}",
                    outcome, external.run_id, native.run_id
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:forge_eval"),
            &[
                ("machine_league_pairwise_comparison_id", &comparison_id),
                ("machine_league_pairwise_status", "recorded"),
                ("external_run_id", &external.run_id),
                ("native_run_id", &native.run_id),
                ("external_runner_id", &external.runner_id),
                ("baseline_runner_id", "mdx_native_harness_runner"),
                ("machine_league_eval_fixture", &external.eval_fixture),
                (
                    "eval_fixture_snapshot_id",
                    &external.eval_fixture_snapshot_id,
                ),
                ("language_task_class", &external.task_class),
                ("external_total_score", external_total_score.as_str()),
                ("native_total_score", native_total_score.as_str()),
                ("pairwise_outcome", outcome),
                (
                    "distillation_candidate_allowed",
                    distillation_allowed.as_str(),
                ),
                ("external_review_receipt_id", &external.review_receipt_id),
                ("native_review_receipt_id", &native.review_receipt_id),
                ("accepted_scoreboard_results_only", "true"),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-pairwise-comparison-local-post","status":"PAIRWISE_COMPARISON_RECORDED","comparison_id":{},"comparison_receipt_id":{},"external_run_id":{},"native_run_id":{},"external_runner_id":{},"baseline_runner_id":"mdx_native_harness_runner","task_class":{},"eval_fixture":{},"eval_fixture_snapshot_id":{},"external_total_score":{},"native_total_score":{},"pairwise_outcome":{},"distillation_candidate_allowed":{},"accepted_scoreboard_results_only":true,"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(&comparison_id),
        json_string_literal(&report.receipt_id),
        json_string_literal(&external.run_id),
        json_string_literal(&native.run_id),
        json_string_literal(&external.runner_id),
        json_string_literal(&external.task_class),
        json_string_literal(&external.eval_fixture),
        json_string_literal(&external.eval_fixture_snapshot_id),
        external.total_score,
        native.total_score,
        json_string_literal(outcome),
        distillation_candidate_allowed,
    ))
}

fn pairwise_comparison_refusal(
    kernel: &mut MdxKernel,
    reason: &str,
    external_run_id: &str,
    native_run_id: &str,
) -> Result<String, String> {
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:forge_eval",
                run_id: "machine_league_pairwise_comparison_refusal",
                event: "evidence_appended",
                work_item_id: "machine_league_pairwise_comparison_refusal",
                detail: &format!("machine_league_pairwise_refused reason={}", reason),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:forge_eval"),
            &[
                ("machine_league_pairwise_status", "refused"),
                ("machine_league_pairwise_refusal_reason", reason),
                ("external_run_id", external_run_id.trim()),
                ("native_run_id", native_run_id.trim()),
                ("distillation_candidate_allowed", "false"),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-pairwise-comparison-local-post","status":"REFUSED","reason":{},"external_run_id":{},"native_run_id":{},"comparison_refusal_receipt_id":{},"distillation_candidate_allowed":false,"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(reason),
        json_string_literal(external_run_id.trim()),
        json_string_literal(native_run_id.trim()),
        json_string_literal(&report.receipt_id)
    ))
}

fn accepted_scorecard_run_for(kernel: &MdxKernel, run_id: &str) -> Option<AcceptedScorecardRun> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
        .find(|receipt| {
            payload_value(receipt, "run_id") == run_id.trim()
                && payload_value(receipt, "accepted_for_scoreboard") == "true"
        })
        .map(|receipt| AcceptedScorecardRun {
            run_id: payload_value(receipt, "run_id").to_string(),
            runner_id: payload_value(receipt, "runner_id").to_string(),
            task_class: payload_value(receipt, "language_task_class").to_string(),
            language_pack_id: payload_value(receipt, "language_pack_id").to_string(),
            eval_fixture: payload_value(receipt, "machine_league_eval_fixture").to_string(),
            eval_fixture_snapshot_id: payload_value(receipt, "eval_fixture_snapshot_id")
                .to_string(),
            eval_oracle_status: payload_value(receipt, "eval_oracle_status").to_string(),
            contamination_status: payload_value(receipt, "contamination_status").to_string(),
            total_score: payload_value(receipt, "total_score").parse().unwrap_or(0),
            review_receipt_id: receipt.receipt_id.clone(),
        })
}

fn run_codex_live_trial(
    run_id: &str,
    body: &str,
    task: &FleetBenchmarkTask,
    language_task: &ForgeLanguageTaskCorpusEntry,
    selected_checks: &str,
) -> Result<CodexLiveTrialArtifacts, String> {
    let root = std::env::current_dir().map_err(|error| error.to_string())?;
    let workspace = ForgeExecutionBackend::new(ForgeExecutionBackendKind::Local)
        .prepare_workspace(ExecutionWorkspaceRequest {
            run_id,
            eval_fixture: "repo_snapshot",
            workspace_kind: ExecutionWorkspaceKind::RepoSnapshot,
        })?;
    let worktree_dir = workspace.workspace_root;
    let artifact_dir = workspace.artifact_dir;

    let clone_result = run_process_with_timeout(
        Command::new("git")
            .arg("clone")
            .arg("--quiet")
            .arg("--no-hardlinks")
            .arg("--local")
            .arg(".")
            .arg(&worktree_dir)
            .current_dir(&root),
        Duration::from_secs(120),
    )?;
    if !clone_result.success() {
        return Err(format!(
            "unable to prepare isolated Codex worktree: {}",
            clone_result.stderr.trim()
        ));
    }

    let prompt = codex_trial_prompt(body, task, language_task, selected_checks);
    let codex_binary = codex_binary_path();
    let codex_timeout = Duration::from_secs(codex_timeout_seconds());
    let mut codex_command = Command::new(&codex_binary);
    codex_command
        .arg("--sandbox")
        .arg("workspace-write")
        .arg("--ask-for-approval")
        .arg("never")
        .arg("--cd")
        .arg(&worktree_dir)
        .arg("exec")
        .arg("--json")
        .arg("--ephemeral")
        .arg(&prompt)
        .current_dir(&worktree_dir);
    apply_scrubbed_environment(&mut codex_command);
    let codex_output = run_process_with_timeout(&mut codex_command, codex_timeout)?;

    let check_output = run_process_with_timeout(
        Command::new("cargo")
            .arg("test")
            .arg("-p")
            .arg("mdx-core")
            .arg("harness_fleet_eval")
            .current_dir(&worktree_dir),
        Duration::from_secs(300),
    )?;
    let diff_output = run_process_with_timeout(
        Command::new("git")
            .arg("diff")
            .arg("--binary")
            .current_dir(&worktree_dir),
        Duration::from_secs(60),
    )?;
    let changed_output = run_process_with_timeout(
        Command::new("git")
            .arg("diff")
            .arg("--name-only")
            .current_dir(&worktree_dir),
        Duration::from_secs(60),
    )?;

    let transcript_path = artifact_dir.join("codex.jsonl");
    let patch_path = artifact_dir.join("codex.patch");
    let check_path = artifact_dir.join("visible-check.txt");
    let handoff_path = artifact_dir.join("codex-pr.md");
    let transcript = format!(
        "{}\n\n<stderr>\n{}\n</stderr>\n",
        codex_output.stdout, codex_output.stderr
    );
    std::fs::write(&transcript_path, transcript).map_err(|error| error.to_string())?;
    std::fs::write(&patch_path, &diff_output.stdout).map_err(|error| error.to_string())?;
    std::fs::write(
        &check_path,
        format!(
            "{}\n\n<stderr>\n{}\n</stderr>\n",
            check_output.stdout, check_output.stderr
        ),
    )
    .map_err(|error| error.to_string())?;

    let changed_files = changed_output
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::trim)
        .collect::<Vec<_>>();
    let changed_file_count = changed_files.len() as u32;
    let codex_exit_code = codex_output.code.unwrap_or(-1);
    let check_exit_code = check_output.code.unwrap_or(-1);
    let check_passed = check_output.success();
    let codex_timed_out = codex_output.timed_out;
    let check_timed_out = check_output.timed_out;
    let patch_bytes = diff_output.stdout.as_bytes();
    let output_artifact_sha256 = format!("sha256:{}", hex(&sha256(patch_bytes)));
    let output_artifact_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/codex.patch");
    let transcript_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/codex.jsonl");
    let pr_handoff_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/codex-pr.md");
    let changed_files_line = if changed_files.is_empty() {
        "none".to_string()
    } else {
        changed_files.join(",")
    };
    std::fs::write(
        &handoff_path,
        format!(
            "# Codex CLI Machine League Trial\n\n- run_id: `{run_id}`\n- worktree: `{}`\n- execution_marker: `{CODEX_EXEC_POLICY_MARKER}`\n- execution_command: `{CODEX_EXEC_POLICY_COMMAND}`\n- codex_exit_code: `{codex_exit_code}`\n- codex_timed_out: `{codex_timed_out}`\n- visible_check: `cargo test -p mdx-core harness_fleet_eval`\n- check_exit_code: `{check_exit_code}`\n- check_timed_out: `{check_timed_out}`\n- check_passed: `{check_passed}`\n- changed_files: `{changed_files_line}`\n- output_artifact_sha256: `{output_artifact_sha256}`\n",
            worktree_dir.display()
        ),
    )
    .map_err(|error| error.to_string())?;

    Ok(CodexLiveTrialArtifacts {
        execution_requested: true,
        execution_allowed: true,
        execution_performed: true,
        execution_status: "CODEX_CLI_EXECUTED_QUARANTINED_PENDING_MDX_GATES",
        runner_execution_mode: "live_codex_cli_quarantined",
        tool_detail: format!(
            "external_machine_adapter codex_cli executed_quarantined exit_code={codex_exit_code} codex_timed_out={codex_timed_out} check_exit_code={check_exit_code} check_timed_out={check_timed_out}"
        ),
        evidence_detail: format!(
            "machine_league_codex_cli_artifacts_recorded output_quarantined=true adapter_execution_performed=true changed_files={changed_file_count} check_passed={check_passed}"
        ),
        finish_detail: format!(
            "status=RUN_FINISHED_DONE machine_league_codex_cli_executed output_quarantined=true files_changed={changed_file_count}"
        ),
        worktree_path: worktree_dir.display().to_string(),
        output_artifact_ref,
        output_artifact_sha256,
        transcript_ref,
        pr_handoff_ref,
        claimed_check: language_task.visible_check.to_string(),
        artifact_filter_summary: format!(
            "codex_cli_live_execution; changed_files={changed_file_count}; check_passed={check_passed}; changed_file_list={changed_files_line}"
        ),
        principal_review_gate_results: live_principal_review_gate_results(
            language_task.required_principal_review_gates,
            codex_output.success(),
            check_passed,
            changed_file_count > 0,
        ),
        diff_language_pack_impact: language_task.language_pack_id.to_string(),
        codex_exit_code,
        codex_timed_out,
        check_exit_code,
        check_timed_out,
        check_passed,
        visible_check_passed: check_passed,
        hidden_check_observed: false,
        hidden_check_passed: false,
        changed_file_count,
    })
}

fn run_codex_live_fixture_trial(
    run_id: &str,
    body: &str,
    language_task: &ForgeLanguageTaskCorpusEntry,
    fixture: MachineLeagueEvalFixture,
) -> Result<CodexLiveTrialArtifacts, String> {
    let workspace =
        requested_execution_backend(body).prepare_workspace(ExecutionWorkspaceRequest {
            run_id,
            eval_fixture: fixture.id,
            workspace_kind: ExecutionWorkspaceKind::HeldOutFixture,
        })?;
    let artifact_dir = workspace.artifact_dir;
    let fixture_dir = workspace.execution_dir;
    write_machine_league_fixture(&fixture_dir, fixture)?;
    init_fixture_git_baseline(&fixture_dir)?;

    let prompt = codex_fixture_prompt(body, fixture);
    let codex_binary = codex_binary_path();
    let codex_timeout = Duration::from_secs(codex_timeout_seconds());
    let mut codex_command = Command::new(&codex_binary);
    codex_command
        .arg("--sandbox")
        .arg("workspace-write")
        .arg("--ask-for-approval")
        .arg("never")
        .arg("--cd")
        .arg(&fixture_dir)
        .arg("exec")
        .arg("--json")
        .arg("--ephemeral")
        .arg(&prompt)
        .current_dir(&fixture_dir);
    apply_scrubbed_environment(&mut codex_command);
    let codex_output = run_process_with_timeout(&mut codex_command, codex_timeout)?;

    let _ = run_process_with_timeout(
        Command::new("git")
            .arg("add")
            .arg("-N")
            .arg(".")
            .current_dir(&fixture_dir),
        Duration::from_secs(30),
    )?;
    let diff_output = run_process_with_timeout(
        Command::new("git")
            .arg("diff")
            .arg("--binary")
            .current_dir(&fixture_dir),
        Duration::from_secs(60),
    )?;
    let changed_output = run_process_with_timeout(
        Command::new("git")
            .arg("diff")
            .arg("--name-only")
            .current_dir(&fixture_dir),
        Duration::from_secs(60),
    )?;

    write_machine_league_hidden_test(&fixture_dir, fixture)?;
    let visible_output = run_process_with_timeout(
        Command::new("cargo")
            .arg("test")
            .arg("--test")
            .arg("visible")
            .current_dir(&fixture_dir),
        Duration::from_secs(120),
    )?;
    let hidden_output = run_process_with_timeout(
        Command::new("cargo")
            .arg("test")
            .arg("--test")
            .arg("hidden")
            .current_dir(&fixture_dir),
        Duration::from_secs(120),
    )?;

    let transcript_path = artifact_dir.join("codex.jsonl");
    let patch_path = artifact_dir.join("codex.patch");
    let check_path = artifact_dir.join("visible-check.txt");
    let handoff_path = artifact_dir.join("codex-pr.md");
    let transcript = format!(
        "{}\n\n<stderr>\n{}\n</stderr>\n",
        codex_output.stdout, codex_output.stderr
    );
    std::fs::write(&transcript_path, transcript).map_err(|error| error.to_string())?;
    std::fs::write(&patch_path, &diff_output.stdout).map_err(|error| error.to_string())?;
    std::fs::write(
        &check_path,
        format!(
            "<visible>\n{}\n\n<stderr>\n{}\n</stderr>\n</visible>\n\n<hidden>\n{}\n\n<stderr>\n{}\n</stderr>\n</hidden>\n",
            visible_output.stdout, visible_output.stderr, hidden_output.stdout, hidden_output.stderr
        ),
    )
    .map_err(|error| error.to_string())?;

    let changed_files = changed_output
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::trim)
        .collect::<Vec<_>>();
    let changed_file_count = changed_files.len() as u32;
    let changed_files_line = if changed_files.is_empty() {
        "none".to_string()
    } else {
        changed_files.join(",")
    };
    let codex_exit_code = codex_output.code.unwrap_or(-1);
    let visible_exit_code = visible_output.code.unwrap_or(-1);
    let hidden_exit_code = hidden_output.code.unwrap_or(-1);
    let check_exit_code = if visible_output.success() && hidden_output.success() {
        0
    } else if !visible_output.success() {
        visible_exit_code
    } else {
        hidden_exit_code
    };
    let codex_timed_out = codex_output.timed_out;
    let check_timed_out = visible_output.timed_out || hidden_output.timed_out;
    let check_passed = visible_output.success() && hidden_output.success();
    let patch_bytes = diff_output.stdout.as_bytes();
    let output_artifact_sha256 = format!("sha256:{}", hex(&sha256(patch_bytes)));
    let output_artifact_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/codex.patch");
    let transcript_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/codex.jsonl");
    let pr_handoff_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/codex-pr.md");

    std::fs::write(
        &handoff_path,
        format!(
            "# Codex CLI Machine League Trial\n\n- run_id: `{run_id}`\n- eval_fixture: `{}`\n- fixture_family: `{}`\n- transferable_edge: `{}`\n- worktree: `{}`\n- execution_marker: `{CODEX_EXEC_POLICY_MARKER}`\n- execution_command: `{CODEX_EXEC_POLICY_COMMAND}`\n- codex_exit_code: `{codex_exit_code}`\n- codex_timed_out: `{codex_timed_out}`\n- visible_check: `{}`\n- visible_exit_code: `{visible_exit_code}`\n- hidden_check: `{}`\n- hidden_exit_code: `{hidden_exit_code}`\n- check_exit_code: `{check_exit_code}`\n- check_timed_out: `{check_timed_out}`\n- check_passed: `{check_passed}`\n- changed_files: `{changed_files_line}`\n- output_artifact_sha256: `{output_artifact_sha256}`\n",
            fixture.id,
            fixture.family_id,
            fixture.transferable_edge,
            fixture_dir.display(),
            fixture.visible_check,
            fixture.hidden_check
        ),
    )
    .map_err(|error| error.to_string())?;

    Ok(CodexLiveTrialArtifacts {
        execution_requested: true,
        execution_allowed: true,
        execution_performed: true,
        execution_status: "CODEX_CLI_EXECUTED_QUARANTINED_PENDING_MDX_GATES",
        runner_execution_mode: "live_codex_cli_quarantined",
        tool_detail: format!(
            "external_machine_adapter codex_cli executed_quarantined fixture={} exit_code={codex_exit_code} codex_timed_out={codex_timed_out} check_exit_code={check_exit_code} check_timed_out={check_timed_out}",
            fixture.id
        ),
        evidence_detail: format!(
            "machine_league_codex_cli_fixture_artifacts_recorded fixture={} output_quarantined=true adapter_execution_performed=true changed_files={changed_file_count} check_passed={check_passed}",
            fixture.id
        ),
        finish_detail: format!(
            "status=RUN_FINISHED_DONE machine_league_codex_cli_fixture_executed output_quarantined=true files_changed={changed_file_count}"
        ),
        worktree_path: fixture_dir.display().to_string(),
        output_artifact_ref,
        output_artifact_sha256,
        transcript_ref,
        pr_handoff_ref,
        claimed_check: language_task.visible_check.to_string(),
        artifact_filter_summary: format!(
            "codex_cli_live_execution; fixture={}; family={}; visible_check={}; hidden_check={}; changed_files={changed_file_count}; check_passed={check_passed}; changed_file_list={changed_files_line}; transferable_edge={}",
            fixture.id,
            fixture.family_id,
            visible_output.success(),
            hidden_output.success(),
            fixture.transferable_edge
        ),
        principal_review_gate_results: live_principal_review_gate_results(
            language_task.required_principal_review_gates,
            codex_output.success(),
            check_passed,
            changed_file_count > 0,
        ),
        diff_language_pack_impact: language_task.language_pack_id.to_string(),
        codex_exit_code,
        codex_timed_out,
        check_exit_code,
        check_timed_out,
        check_passed,
        visible_check_passed: visible_output.success(),
        hidden_check_observed: true,
        hidden_check_passed: hidden_output.success(),
        changed_file_count,
    })
}

fn run_gemini_live_rust_tax_fixture_trial(
    run_id: &str,
    body: &str,
    language_task: &ForgeLanguageTaskCorpusEntry,
) -> Result<GeminiLiveTrialArtifacts, String> {
    let workspace =
        requested_execution_backend(body).prepare_workspace(ExecutionWorkspaceRequest {
            run_id,
            eval_fixture: MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID,
            workspace_kind: ExecutionWorkspaceKind::HeldOutFixture,
        })?;
    let artifact_dir = workspace.artifact_dir;
    let fixture_dir = workspace.execution_dir;
    write_rust_tax_fixture(&fixture_dir)?;
    init_fixture_git_baseline(&fixture_dir)?;

    let prompt = gemini_rust_tax_fixture_prompt(body);
    let gemini_binary = gemini_binary_path();
    let gemini_timeout = Duration::from_secs(gemini_timeout_seconds());
    let gemini_output = run_process_with_timeout(
        Command::new(&gemini_binary)
            .arg("--skip-trust")
            .arg("--approval-mode")
            .arg("yolo")
            .arg("--output-format")
            .arg("stream-json")
            .arg("--prompt")
            .arg(&prompt)
            .current_dir(&fixture_dir),
        gemini_timeout,
    )?;

    let _ = run_process_with_timeout(
        Command::new("git")
            .arg("add")
            .arg("-N")
            .arg(".")
            .current_dir(&fixture_dir),
        Duration::from_secs(30),
    )?;
    let diff_output = run_process_with_timeout(
        Command::new("git")
            .arg("diff")
            .arg("--binary")
            .current_dir(&fixture_dir),
        Duration::from_secs(60),
    )?;
    let changed_output = run_process_with_timeout(
        Command::new("git")
            .arg("diff")
            .arg("--name-only")
            .current_dir(&fixture_dir),
        Duration::from_secs(60),
    )?;

    write_rust_tax_hidden_test(&fixture_dir)?;
    let visible_output = run_process_with_timeout(
        Command::new("cargo")
            .arg("test")
            .arg("--test")
            .arg("visible")
            .current_dir(&fixture_dir),
        Duration::from_secs(120),
    )?;
    let hidden_output = run_process_with_timeout(
        Command::new("cargo")
            .arg("test")
            .arg("--test")
            .arg("hidden")
            .current_dir(&fixture_dir),
        Duration::from_secs(120),
    )?;

    let transcript_path = artifact_dir.join("gemini.stream-json");
    let patch_path = artifact_dir.join("gemini.patch");
    let check_path = artifact_dir.join("gemini-check.txt");
    let handoff_path = artifact_dir.join("gemini-pr.md");
    let transcript = format!(
        "{}\n\n<stderr>\n{}\n</stderr>\n",
        gemini_output.stdout, gemini_output.stderr
    );
    std::fs::write(&transcript_path, transcript).map_err(|error| error.to_string())?;
    std::fs::write(&patch_path, &diff_output.stdout).map_err(|error| error.to_string())?;
    std::fs::write(
        &check_path,
        format!(
            "<visible>\n{}\n\n<stderr>\n{}\n</stderr>\n</visible>\n\n<hidden>\n{}\n\n<stderr>\n{}\n</stderr>\n</hidden>\n",
            visible_output.stdout, visible_output.stderr, hidden_output.stdout, hidden_output.stderr
        ),
    )
    .map_err(|error| error.to_string())?;

    let changed_files = changed_output
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::trim)
        .collect::<Vec<_>>();
    let changed_file_count = changed_files.len() as u32;
    let changed_files_line = if changed_files.is_empty() {
        "none".to_string()
    } else {
        changed_files.join(",")
    };
    let gemini_exit_code = gemini_output.code.unwrap_or(-1);
    let gemini_auth_blocked = gemini_output.stderr.contains("Error authenticating")
        || gemini_output.stderr.contains("IneligibleTierError")
        || gemini_output.stderr.contains("UNSUPPORTED_CLIENT");
    let visible_exit_code = visible_output.code.unwrap_or(-1);
    let hidden_exit_code = hidden_output.code.unwrap_or(-1);
    let check_exit_code = if visible_output.success() && hidden_output.success() {
        0
    } else if !visible_output.success() {
        visible_exit_code
    } else {
        hidden_exit_code
    };
    let gemini_timed_out = gemini_output.timed_out;
    let check_timed_out = visible_output.timed_out || hidden_output.timed_out;
    let check_passed = visible_output.success() && hidden_output.success();
    let patch_bytes = diff_output.stdout.as_bytes();
    let output_artifact_sha256 = format!("sha256:{}", hex(&sha256(patch_bytes)));
    let output_artifact_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/gemini.patch");
    let transcript_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/gemini.stream-json");
    let pr_handoff_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/gemini-pr.md");

    std::fs::write(
        &handoff_path,
        format!(
            "# Gemini CLI Machine League Trial\n\n- run_id: `{run_id}`\n- eval_fixture: `{MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID}`\n- worktree: `{}`\n- execution_marker: `{GEMINI_EXEC_POLICY_MARKER}`\n- execution_command: `{GEMINI_EXEC_POLICY_COMMAND}`\n- gemini_exit_code: `{gemini_exit_code}`\n- gemini_timed_out: `{gemini_timed_out}`\n- visible_check: `{MACHINE_LEAGUE_RUST_TAX_VISIBLE_CHECK}`\n- visible_exit_code: `{visible_exit_code}`\n- hidden_check: `{MACHINE_LEAGUE_RUST_TAX_HIDDEN_CHECK}`\n- hidden_exit_code: `{hidden_exit_code}`\n- check_exit_code: `{check_exit_code}`\n- check_timed_out: `{check_timed_out}`\n- check_passed: `{check_passed}`\n- changed_files: `{changed_files_line}`\n- output_artifact_sha256: `{output_artifact_sha256}`\n",
            fixture_dir.display()
        ),
    )
    .map_err(|error| error.to_string())?;

    Ok(GeminiLiveTrialArtifacts {
        execution_requested: true,
        execution_allowed: true,
        execution_performed: true,
        execution_status: "GEMINI_CLI_EXECUTED_QUARANTINED_PENDING_MDX_GATES",
        runner_execution_mode: "live_gemini_cli_quarantined",
        tool_detail: format!(
            "external_machine_adapter gemini_cli executed_quarantined fixture={MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID} exit_code={gemini_exit_code} gemini_timed_out={gemini_timed_out} gemini_auth_blocked={gemini_auth_blocked} check_exit_code={check_exit_code} check_timed_out={check_timed_out}"
        ),
        evidence_detail: format!(
            "machine_league_gemini_cli_fixture_artifacts_recorded fixture={MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID} output_quarantined=true adapter_execution_performed=true changed_files={changed_file_count} check_passed={check_passed}"
        ),
        finish_detail: format!(
            "status=RUN_FINISHED_DONE machine_league_gemini_cli_fixture_executed output_quarantined=true files_changed={changed_file_count}"
        ),
        worktree_path: fixture_dir.display().to_string(),
        output_artifact_ref,
        output_artifact_sha256,
        transcript_ref,
        pr_handoff_ref,
        claimed_check: language_task.visible_check.to_string(),
        artifact_filter_summary: format!(
            "gemini_cli_live_execution; fixture={MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID}; visible_check={}; hidden_check={}; changed_files={changed_file_count}; check_passed={check_passed}; changed_file_list={changed_files_line}",
            visible_output.success(),
            hidden_output.success()
        ),
        principal_review_gate_results: live_principal_review_gate_results(
            language_task.required_principal_review_gates,
            gemini_output.success(),
            check_passed,
            changed_file_count > 0,
        ),
        diff_language_pack_impact: language_task.language_pack_id.to_string(),
        gemini_exit_code,
        gemini_timed_out,
        gemini_auth_blocked,
        check_exit_code,
        check_timed_out,
        check_passed,
        visible_check_passed: visible_output.success(),
        hidden_check_observed: true,
        hidden_check_passed: hidden_output.success(),
        changed_file_count,
    })
}

fn run_open_machine_live_rust_tax_fixture_trial(
    run_id: &str,
    body: &str,
    language_task: &ForgeLanguageTaskCorpusEntry,
    spec: OpenMachineTrialSpec,
    max_spend_cents: u32,
) -> Result<OpenMachineLiveTrialArtifacts, String> {
    let workspace =
        requested_execution_backend(body).prepare_workspace(ExecutionWorkspaceRequest {
            run_id,
            eval_fixture: MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID,
            workspace_kind: ExecutionWorkspaceKind::HeldOutFixture,
        })?;
    let artifact_dir = workspace.artifact_dir;
    let fixture_dir = workspace.execution_dir;
    write_rust_tax_fixture(&fixture_dir)?;
    init_fixture_git_baseline(&fixture_dir)?;

    let prompt = open_machine_rust_tax_fixture_prompt(body, spec);
    let runner_timeout = Duration::from_secs(open_machine_timeout_seconds(spec.adapter_kind));
    let repo_root = std::env::current_dir().map_err(|error| error.to_string())?;
    let adapter_script = repo_root.join("scripts/external-machine-adapter.mjs");
    let mut command = if matches!(
        spec.adapter_kind,
        "grok_build_cli" | "claude_code" | "kimi_code"
    ) {
        let adapter_state_paths =
            external_adapter_state_write_paths(spec.adapter_kind, &artifact_dir);
        if matches!(spec.adapter_kind, "grok_build_cli" | "kimi_code") {
            for path in &adapter_state_paths {
                std::fs::create_dir_all(path).map_err(|error| {
                    format!(
                        "failed to create isolated {} adapter state at {}: {error}",
                        spec.adapter_kind,
                        path.display()
                    )
                })?;
            }
        }
        let mut command = networked_workspace_command("node", &fixture_dir, &adapter_state_paths);
        command
            .arg(&adapter_script)
            .arg("--adapter")
            .arg(match spec.adapter_kind {
                "grok_build_cli" => "grok-acp",
                "kimi_code" => "kimi-code-cli",
                _ => "claude-agent-sdk",
            })
            .arg("--workspace")
            .arg(&fixture_dir);
        command
    } else {
        Command::new(spec.binary_name)
    };
    if !matches!(
        spec.adapter_kind,
        "grok_build_cli" | "claude_code" | "kimi_code"
    ) {
        apply_scrubbed_environment(&mut command);
    }
    match spec.adapter_kind {
        "opencode" => {
            command.arg("run").arg("--format").arg("json").arg(&prompt);
        }
        "cline" => {
            command
                .arg("--json")
                .arg("--auto-approve")
                .arg("true")
                .arg(&prompt);
        }
        "goose" => {
            command
                .env("GOOSE_MODE", "auto")
                .arg("run")
                .arg("--no-session")
                .arg("-t")
                .arg(&prompt)
                .arg("--output-format")
                .arg("stream-json");
        }
        "grok_build_cli" => {
            let xai_api_key = global()
                .provider_key_for_tenant(&current_tenant_id(), "XAI_API_KEY")
                .ok_or_else(|| {
                    "Grok Build ACP execution reached the adapter without a tenant-scoped XAI_API_KEY"
                        .to_string()
                })?;
            let grok_home = artifact_dir.join("grok-home");
            command
                .env("MDX_EXTERNAL_MACHINE_PROMPT", &prompt)
                .env("MDX_GROK_BINARY", resolve_binary_path(spec.binary_name))
                .env("MDX_GROK_MODEL", spec.model_id)
                .env("MDX_GROK_HOME", grok_home)
                .env("XAI_API_KEY", xai_api_key);
        }
        "claude_code" => {
            let anthropic_api_key = global()
                .provider_key_for_tenant(&current_tenant_id(), "ANTHROPIC_API_KEY")
                .ok_or_else(|| {
                    "Claude Agent SDK execution reached the adapter without a tenant-scoped ANTHROPIC_API_KEY"
                        .to_string()
                })?;
            let max_budget_usd = format!("{:.2}", f64::from(max_spend_cents) / 100.0);
            command
                .env("MDX_EXTERNAL_MACHINE_PROMPT", &prompt)
                .env("MDX_CLAUDE_BINARY", resolve_binary_path(spec.binary_name))
                .env("ANTHROPIC_API_KEY", anthropic_api_key)
                .env("MDX_EXTERNAL_MACHINE_MAX_BUDGET_USD", max_budget_usd);
        }
        "kimi_code" => {
            let moonshot_api_key = global()
                .provider_key_for_tenant(&current_tenant_id(), "MOONSHOT_API_KEY")
                .ok_or_else(|| {
                    "Kimi Code execution reached the adapter without a tenant-scoped MOONSHOT_API_KEY"
                        .to_string()
                })?;
            let kimi_code_home = artifact_dir.join("kimi-code-home");
            command
                .env("MDX_EXTERNAL_MACHINE_PROMPT", &prompt)
                .env("MDX_KIMI_BINARY", resolve_binary_path(spec.binary_name))
                .env("MDX_KIMI_MODEL", spec.model_id)
                .env("MDX_KIMI_BASE_URL", "https://api.moonshot.ai/v1")
                .env("MDX_KIMI_CODE_HOME", kimi_code_home)
                .env("MOONSHOT_API_KEY", moonshot_api_key);
        }
        _ => {
            command.arg(&prompt);
        }
    }
    command.current_dir(&fixture_dir);
    let runner_output = run_process_with_timeout(&mut command, runner_timeout)?;

    let _ = run_process_with_timeout(
        Command::new("git")
            .arg("add")
            .arg("-N")
            .arg(".")
            .current_dir(&fixture_dir),
        Duration::from_secs(30),
    )?;
    let diff_output = run_process_with_timeout(
        Command::new("git")
            .arg("diff")
            .arg("--binary")
            .current_dir(&fixture_dir),
        Duration::from_secs(60),
    )?;
    let changed_output = run_process_with_timeout(
        Command::new("git")
            .arg("diff")
            .arg("--name-only")
            .current_dir(&fixture_dir),
        Duration::from_secs(60),
    )?;

    write_rust_tax_hidden_test(&fixture_dir)?;
    let visible_output = run_process_with_timeout(
        Command::new("cargo")
            .arg("test")
            .arg("--test")
            .arg("visible")
            .current_dir(&fixture_dir),
        Duration::from_secs(120),
    )?;
    let hidden_output = run_process_with_timeout(
        Command::new("cargo")
            .arg("test")
            .arg("--test")
            .arg("hidden")
            .current_dir(&fixture_dir),
        Duration::from_secs(120),
    )?;

    let transcript_path = artifact_dir.join(spec.transcript_name);
    let patch_path = artifact_dir.join(format!("{}.patch", spec.artifact_slug));
    let check_path = artifact_dir.join(format!("{}-check.txt", spec.artifact_slug));
    let handoff_path = artifact_dir.join(format!("{}-pr.md", spec.artifact_slug));
    let transcript = format!(
        "{}\n\n<stderr>\n{}\n</stderr>\n",
        runner_output.stdout, runner_output.stderr
    );
    std::fs::write(&transcript_path, transcript).map_err(|error| error.to_string())?;
    std::fs::write(&patch_path, &diff_output.stdout).map_err(|error| error.to_string())?;
    std::fs::write(
        &check_path,
        format!(
            "<visible>\n{}\n\n<stderr>\n{}\n</stderr>\n</visible>\n\n<hidden>\n{}\n\n<stderr>\n{}\n</stderr>\n</hidden>\n",
            visible_output.stdout, visible_output.stderr, hidden_output.stdout, hidden_output.stderr
        ),
    )
    .map_err(|error| error.to_string())?;

    let changed_files = changed_output
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::trim)
        .collect::<Vec<_>>();
    let changed_file_count = changed_files.len() as u32;
    let changed_files_line = if changed_files.is_empty() {
        "none".to_string()
    } else {
        changed_files.join(",")
    };
    let runner_exit_code = runner_output.code.unwrap_or(-1);
    let runner_auth_blocked =
        open_machine_auth_blocked(&runner_output.stdout, &runner_output.stderr);
    let visible_exit_code = visible_output.code.unwrap_or(-1);
    let hidden_exit_code = hidden_output.code.unwrap_or(-1);
    let check_exit_code = if visible_output.success() && hidden_output.success() {
        0
    } else if !visible_output.success() {
        visible_exit_code
    } else {
        hidden_exit_code
    };
    let runner_timed_out = runner_output.timed_out;
    let check_timed_out = visible_output.timed_out || hidden_output.timed_out;
    let check_passed = visible_output.success() && hidden_output.success();
    let patch_bytes = diff_output.stdout.as_bytes();
    let output_artifact_sha256 = format!("sha256:{}", hex(&sha256(patch_bytes)));
    let output_artifact_ref = format!(
        "quarantine://forge/fleet-eval/machine-league/{run_id}/{}.patch",
        spec.artifact_slug
    );
    let transcript_ref = format!(
        "quarantine://forge/fleet-eval/machine-league/{run_id}/{}",
        spec.transcript_name
    );
    let pr_handoff_ref = format!(
        "quarantine://forge/fleet-eval/machine-league/{run_id}/{}-pr.md",
        spec.artifact_slug
    );

    std::fs::write(
        &handoff_path,
        format!(
            "# {} Machine League Trial\n\n- run_id: `{run_id}`\n- eval_fixture: `{MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID}`\n- worktree: `{}`\n- execution_marker: `{}`\n- execution_command: `{}`\n- runner_exit_code: `{runner_exit_code}`\n- runner_timed_out: `{runner_timed_out}`\n- runner_auth_blocked: `{runner_auth_blocked}`\n- visible_check: `{MACHINE_LEAGUE_RUST_TAX_VISIBLE_CHECK}`\n- visible_exit_code: `{visible_exit_code}`\n- hidden_check: `{MACHINE_LEAGUE_RUST_TAX_HIDDEN_CHECK}`\n- hidden_exit_code: `{hidden_exit_code}`\n- check_exit_code: `{check_exit_code}`\n- check_timed_out: `{check_timed_out}`\n- check_passed: `{check_passed}`\n- changed_files: `{changed_files_line}`\n- output_artifact_sha256: `{output_artifact_sha256}`\n",
            spec.display_name,
            fixture_dir.display(),
            spec.execution_marker,
            spec.execution_command
        ),
    )
    .map_err(|error| error.to_string())?;

    Ok(OpenMachineLiveTrialArtifacts {
        execution_requested: true,
        execution_allowed: true,
        execution_performed: true,
        execution_status: format!(
            "{}_EXECUTED_QUARANTINED_PENDING_MDX_GATES",
            spec.status_prefix
        ),
        runner_execution_mode: format!("live_{}_quarantined", spec.adapter_kind),
        tool_detail: format!(
            "external_machine_adapter {} executed_quarantined fixture={MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID} exit_code={runner_exit_code} runner_timed_out={runner_timed_out} runner_auth_blocked={runner_auth_blocked} check_exit_code={check_exit_code} check_timed_out={check_timed_out}",
            spec.adapter_kind
        ),
        evidence_detail: format!(
            "machine_league_{}_fixture_artifacts_recorded fixture={MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID} output_quarantined=true adapter_execution_performed=true changed_files={changed_file_count} check_passed={check_passed}",
            spec.adapter_slug
        ),
        finish_detail: format!(
            "status=RUN_FINISHED_DONE machine_league_{}_fixture_executed output_quarantined=true files_changed={changed_file_count}",
            spec.adapter_slug
        ),
        worktree_path: fixture_dir.display().to_string(),
        output_artifact_ref,
        output_artifact_sha256,
        transcript_ref,
        pr_handoff_ref,
        claimed_check: language_task.visible_check.to_string(),
        artifact_filter_summary: format!(
            "{}_live_execution; fixture={MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID}; visible_check={}; hidden_check={}; changed_files={changed_file_count}; check_passed={check_passed}; changed_file_list={changed_files_line}",
            spec.adapter_kind,
            visible_output.success(),
            hidden_output.success()
        ),
        principal_review_gate_results: live_principal_review_gate_results(
            language_task.required_principal_review_gates,
            runner_output.success(),
            check_passed,
            changed_file_count > 0,
        ),
        diff_language_pack_impact: language_task.language_pack_id.to_string(),
        runner_exit_code,
        runner_timed_out,
        runner_auth_blocked,
        check_exit_code,
        check_timed_out,
        check_passed,
        visible_check_passed: visible_output.success(),
        hidden_check_observed: true,
        hidden_check_passed: hidden_output.success(),
        changed_file_count,
    })
}

fn external_adapter_state_write_paths(adapter_kind: &str, artifact_dir: &Path) -> Vec<PathBuf> {
    if adapter_kind == "grok_build_cli" {
        return vec![artifact_dir.join("grok-home")];
    }
    if adapter_kind == "kimi_code" {
        return vec![artifact_dir.join("kimi-code-home")];
    }
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let relative_paths: &[&str] = match adapter_kind {
        "claude_code" => &[
            ".claude/debug",
            ".claude/projects",
            ".claude/sessions",
            ".claude/todos",
            ".claude/telemetry",
            ".claude/history.jsonl",
            ".claude/stats-cache.json",
        ],
        _ => &[],
    };
    relative_paths.iter().map(|path| home.join(path)).collect()
}

// The scripted baseline: stage the fixture and replay the known deterministic
// patch, then measure with the shared oracle. Honest rehearsal, never scored.
fn run_native_fixture_trial(
    run_id: &str,
    body: &str,
    language_task: &ForgeLanguageTaskCorpusEntry,
    fixture: MachineLeagueEvalFixture,
    native_strategy_profile: &str,
) -> Result<NativeLiveTrialArtifacts, String> {
    let workspace =
        requested_execution_backend(body).prepare_workspace(ExecutionWorkspaceRequest {
            run_id,
            eval_fixture: fixture.id,
            workspace_kind: ExecutionWorkspaceKind::HeldOutFixture,
        })?;
    let artifact_dir = workspace.artifact_dir;
    let fixture_dir = workspace.execution_dir;
    write_machine_league_fixture(&fixture_dir, fixture)?;
    init_fixture_git_baseline(&fixture_dir)?;
    let operator_intent = json_string_field(body, "intent")
        .unwrap_or_else(|| format!("Run the MDx native fixture path against {}.", fixture.title));
    let solver_changed_source =
        apply_native_fixture_strategy(&fixture_dir, fixture, native_strategy_profile)?;
    native_trial_post_solver_evidence(NativePostSolverInput {
        run_id,
        artifact_dir: &artifact_dir,
        fixture_dir: &fixture_dir,
        language_task,
        fixture,
        native_strategy_profile,
        operator_intent: &operator_intent,
        solver_changed_source,
        native_exit_code: if solver_changed_source { 0 } else { 1 },
        execution_performed: true,
        execution_status: "MDX_NATIVE_EXECUTED_QUARANTINED_PENDING_MDX_GATES",
    })
}

// The harness solver: stage the fixture, then run the REAL Forge loop against
// it with NO kernel lock held. When the loop produces a branch of real work,
// restore it into the fixture working tree so the shared oracle measures the
// loop's actual edit, not the untouched buggy baseline.
fn run_native_harness_fixture_trial(
    run_id: &str,
    body: &str,
    language_task: &ForgeLanguageTaskCorpusEntry,
    fixture: MachineLeagueEvalFixture,
    native_strategy_profile: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<NativeLiveTrialArtifacts, String> {
    let workspace =
        requested_execution_backend(body).prepare_workspace(ExecutionWorkspaceRequest {
            run_id,
            eval_fixture: fixture.id,
            workspace_kind: ExecutionWorkspaceKind::HeldOutFixture,
        })?;
    let artifact_dir = workspace.artifact_dir;
    let fixture_dir = workspace.execution_dir;
    write_machine_league_fixture(&fixture_dir, fixture)?;
    init_fixture_git_baseline(&fixture_dir)?;
    let operator_intent = json_string_field(body, "intent")
        .unwrap_or_else(|| format!("Run the MDx native fixture path against {}.", fixture.title));

    let request = crate::forge_loop_runner::ForgeRunRequest {
        run_id: run_id.to_string(),
        tenant_id: "local_tenant".to_string(),
        actor_id: "agent:forge_native".to_string(),
        work_item_id: "machine_league_native_trial".to_string(),
        intent: native_builder_trial_prompt(body, fixture, language_task),
        allowed_commands: vec![fixture.visible_check.to_string()],
        max_turns: 0,
        revise_branch: None,
        resume: false,
        write_scope: Vec::new(),
        check_target_dir: None,
        builder_slot: String::new(),
        work_complexity_tier: "medium".to_string(),
        semantic_policy_required_operations: Vec::new(),
        semantic_policy_source: "machine_league_native_trial".to_string(),
        execution_geometry_requested_workers: 1,
        execution_geometry_effective_workers: 1,
        execution_geometry_lane: "machine_league_native_trial".to_string(),
        execution_geometry_route: "/forge/machine-league/native-trials.json".to_string(),
        mission_id: String::new(),
        mission_milestone_id: String::new(),
        max_cost_cents: 0,
        max_runtime_ms: 0,
        envelope_id: String::new(),
        plan_only: false,
        reasoning_effort: String::new(),
    };
    let outcome = crate::forge_loop_runner::run_forge_loop(&request, &fixture_dir, kernel);
    let reached_terminal = native_run_reached_terminal(outcome.status);
    if let Some(branch) = outcome.branch.as_deref() {
        let _ = run_process_with_timeout(
            Command::new("git")
                .arg("restore")
                .arg("--source")
                .arg(branch)
                .arg("--worktree")
                .arg("--")
                .arg(".")
                .current_dir(&fixture_dir),
            Duration::from_secs(60),
        );
    }
    native_trial_post_solver_evidence(NativePostSolverInput {
        run_id,
        artifact_dir: &artifact_dir,
        fixture_dir: &fixture_dir,
        language_task,
        fixture,
        native_strategy_profile,
        operator_intent: &operator_intent,
        solver_changed_source: outcome.files_changed > 0,
        native_exit_code: if reached_terminal { 0 } else { 1 },
        execution_performed: reached_terminal,
        execution_status: if reached_terminal {
            "MDX_NATIVE_HARNESS_EXECUTED_QUARANTINED_PENDING_MDX_GATES"
        } else {
            "MDX_NATIVE_HARNESS_RUN_DID_NOT_START"
        },
    })
}

// A run that reached a real terminal status did the work; failed-to-start and
// panicked runs did not, and get native_exit_code=1.
fn native_run_reached_terminal(status: &str) -> bool {
    matches!(
        status,
        "RUN_FINISHED_DONE"
            | "RUN_FINISHED_NO_CHANGE"
            | "RUN_FINISHED_CANNOT_PROCEED"
            | "RUN_STOPPED"
            | "RUN_BUDGET_EXHAUSTED"
            | "RUN_ERROR"
    )
}

// The trial prompt handed to the native builder. Mirrors codex_trial_prompt's
// shape (task class, language task, operator intent, allowed scope, visible
// check) but addresses the native builder inside the isolated fixture worktree.
fn native_builder_trial_prompt(
    body: &str,
    fixture: MachineLeagueEvalFixture,
    language_task: &ForgeLanguageTaskCorpusEntry,
) -> String {
    let operator_intent = json_string_field(body, "intent").unwrap_or_else(|| {
        "Make the smallest safe Rust Cargo bug-fix style improvement inside the allowed scope."
            .to_string()
    });
    format!(
        "You are the MDx native builder running as a Machine League runner inside an isolated Forge trial worktree.\n\
Task class: {} / {}.\n\
Language task: {}.\n\
Operator intent: {}.\n\
Allowed write scope: {}.\n\
Make the smallest reviewable change you can justify. Prefer adding or updating a focused regression test when behavior changes.\n\
Run or preserve this visible check before finishing: {}.\n\
Leave a concise final summary with changed files, checks run, and residual risk.",
        language_task.task_class,
        language_task.complexity_tier,
        language_task.task_corpus_id,
        operator_intent,
        fixture.allowed_scope,
        fixture.visible_check
    )
}

struct NativePostSolverInput<'a> {
    run_id: &'a str,
    artifact_dir: &'a Path,
    fixture_dir: &'a Path,
    language_task: &'a ForgeLanguageTaskCorpusEntry,
    fixture: MachineLeagueEvalFixture,
    native_strategy_profile: &'a str,
    operator_intent: &'a str,
    solver_changed_source: bool,
    native_exit_code: i32,
    execution_performed: bool,
    execution_status: &'static str,
}

// Shared post-solver evidence phase: diff the fixture, run the hidden test and
// the visible + hidden checks, write the artifacts, and assemble the trial
// record. Both the scripted and harness solvers converge here so the oracle is
// identical regardless of who produced the edit.
fn native_trial_post_solver_evidence(
    input: NativePostSolverInput,
) -> Result<NativeLiveTrialArtifacts, String> {
    let artifact_dir = input.artifact_dir;
    // Owned so the many `&fixture_dir` current_dir calls read naturally.
    let fixture_dir = input.fixture_dir.to_path_buf();
    let language_task = input.language_task;
    let fixture = input.fixture;
    let run_id = input.run_id;
    let native_strategy_profile = input.native_strategy_profile;
    let operator_intent = input.operator_intent;
    let solver_changed_source = input.solver_changed_source;

    let _ = run_process_with_timeout(
        Command::new("git")
            .arg("add")
            .arg("-N")
            .arg(".")
            .current_dir(&fixture_dir),
        Duration::from_secs(30),
    )?;
    let diff_output = run_process_with_timeout(
        Command::new("git")
            .arg("diff")
            .arg("--binary")
            .current_dir(&fixture_dir),
        Duration::from_secs(60),
    )?;
    let changed_output = run_process_with_timeout(
        Command::new("git")
            .arg("diff")
            .arg("--name-only")
            .current_dir(&fixture_dir),
        Duration::from_secs(60),
    )?;

    write_machine_league_hidden_test(&fixture_dir, fixture)?;
    let visible_output = run_process_with_timeout(
        Command::new("cargo")
            .arg("test")
            .arg("--test")
            .arg("visible")
            .current_dir(&fixture_dir),
        Duration::from_secs(120),
    )?;
    let hidden_output = run_process_with_timeout(
        Command::new("cargo")
            .arg("test")
            .arg("--test")
            .arg("hidden")
            .current_dir(&fixture_dir),
        Duration::from_secs(120),
    )?;

    let transcript_path = artifact_dir.join("native-transcript.txt");
    let patch_path = artifact_dir.join("native.patch");
    let check_path = artifact_dir.join("native-check.txt");
    let handoff_path = artifact_dir.join("native-pr.md");
    let native_exit_code = input.native_exit_code;
    let native_timed_out = false;
    std::fs::write(
        &transcript_path,
        format!(
            "MDx native fixture runner\n\noperator_intent={operator_intent}\nfixture={}\nfixture_family={}\nstrategy_profile={native_strategy_profile}\ntransferable_edge={}\nsolver_changed_source={solver_changed_source}\nnative_exit_code={native_exit_code}\n",
            fixture.id,
            fixture.family_id,
            fixture.transferable_edge
        ),
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(&patch_path, &diff_output.stdout).map_err(|error| error.to_string())?;
    std::fs::write(
        &check_path,
        format!(
            "<visible>\n{}\n\n<stderr>\n{}\n</stderr>\n</visible>\n\n<hidden>\n{}\n\n<stderr>\n{}\n</stderr>\n</hidden>\n",
            visible_output.stdout, visible_output.stderr, hidden_output.stdout, hidden_output.stderr
        ),
    )
    .map_err(|error| error.to_string())?;

    let changed_files = changed_output
        .stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::trim)
        .collect::<Vec<_>>();
    let changed_file_count = changed_files.len() as u32;
    let changed_files_line = if changed_files.is_empty() {
        "none".to_string()
    } else {
        changed_files.join(",")
    };
    let visible_exit_code = visible_output.code.unwrap_or(-1);
    let hidden_exit_code = hidden_output.code.unwrap_or(-1);
    let check_exit_code = if visible_output.success() && hidden_output.success() {
        0
    } else if !visible_output.success() {
        visible_exit_code
    } else {
        hidden_exit_code
    };
    let check_timed_out = visible_output.timed_out || hidden_output.timed_out;
    let check_passed = visible_output.success() && hidden_output.success();
    let patch_bytes = diff_output.stdout.as_bytes();
    let output_artifact_sha256 = format!("sha256:{}", hex(&sha256(patch_bytes)));
    let output_artifact_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/native.patch");
    let transcript_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/native-transcript.txt");
    let pr_handoff_ref =
        format!("quarantine://forge/fleet-eval/machine-league/{run_id}/native-pr.md");

    std::fs::write(
        &handoff_path,
        format!(
            "# MDx Native Machine League Trial\n\n- run_id: `{run_id}`\n- eval_fixture: `{}`\n- fixture_family: `{}`\n- strategy_profile: `{native_strategy_profile}`\n- transferable_edge: `{}`\n- worktree: `{}`\n- native_exit_code: `{native_exit_code}`\n- native_timed_out: `{native_timed_out}`\n- visible_check: `{}`\n- visible_exit_code: `{visible_exit_code}`\n- hidden_check: `{}`\n- hidden_exit_code: `{hidden_exit_code}`\n- check_exit_code: `{check_exit_code}`\n- check_timed_out: `{check_timed_out}`\n- check_passed: `{check_passed}`\n- changed_files: `{changed_files_line}`\n- output_artifact_sha256: `{output_artifact_sha256}`\n",
            fixture.id,
            fixture.family_id,
            fixture.transferable_edge,
            fixture_dir.display(),
            fixture.visible_check,
            fixture.hidden_check
        ),
    )
    .map_err(|error| error.to_string())?;

    Ok(NativeLiveTrialArtifacts {
        execution_performed: input.execution_performed,
        execution_status: input.execution_status,
        tool_detail: format!(
            "mdx_native_fixture_runner executed_quarantined fixture={} strategy={} exit_code={native_exit_code} check_exit_code={check_exit_code} check_timed_out={check_timed_out}",
            fixture.id, native_strategy_profile
        ),
        evidence_detail: format!(
            "machine_league_native_fixture_artifacts_recorded fixture={} strategy={} output_quarantined=true adapter_execution_performed=true changed_files={changed_file_count} check_passed={check_passed}",
            fixture.id, native_strategy_profile
        ),
        finish_detail: format!(
            "status=RUN_FINISHED_DONE machine_league_native_fixture_executed output_quarantined=true files_changed={changed_file_count}"
        ),
        worktree_path: fixture_dir.display().to_string(),
        output_artifact_ref,
        output_artifact_sha256,
        transcript_ref,
        pr_handoff_ref,
        claimed_check: language_task.visible_check.to_string(),
        artifact_filter_summary: format!(
            "mdx_native_fixture_execution; fixture={}; family={}; strategy={}; visible_check={}; hidden_check={}; changed_files={changed_file_count}; check_passed={check_passed}; changed_file_list={changed_files_line}; transferable_edge={}",
            fixture.id,
            fixture.family_id,
            native_strategy_profile,
            visible_output.success(),
            hidden_output.success(),
            fixture.transferable_edge
        ),
        principal_review_gate_results: live_principal_review_gate_results(
            language_task.required_principal_review_gates,
            solver_changed_source,
            check_passed,
            changed_file_count > 0,
        ),
        diff_language_pack_impact: language_task.language_pack_id.to_string(),
        native_exit_code,
        native_timed_out,
        check_exit_code,
        check_timed_out,
        check_passed,
        visible_check_passed: visible_output.success(),
        hidden_check_observed: true,
        hidden_check_passed: hidden_output.success(),
        changed_file_count,
    })
}

fn apply_native_fixture_strategy(
    fixture_dir: &Path,
    fixture: MachineLeagueEvalFixture,
    native_strategy_profile: &str,
) -> Result<bool, String> {
    match fixture.id {
        MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID => {
            let source_path = fixture_dir.join("src").join("lib.rs");
            replace_in_file(
                &source_path,
                "discounted + tax_cents(subtotal_cents, tax_basis_points)",
                "discounted + tax_cents(discounted, tax_basis_points)",
            )
        }
        MACHINE_LEAGUE_RUST_POLICY_FIXTURE_ID | MACHINE_LEAGUE_RUST_POLICY_VARIANT_FIXTURE_ID => {
            let target_plan = if fixture.id == MACHINE_LEAGUE_RUST_POLICY_VARIANT_FIXTURE_ID {
                "partner"
            } else {
                "pro"
            };
            if native_strategy_profile == fixture.native_improved_strategy {
                let source_path = fixture_dir.join("src").join("policy.rs");
                replace_in_file(
                    &source_path,
                    r#"plan == "enterprise" && region != "restricted""#,
                    &format!(
                        r#"(plan == "enterprise" || plan == "{target_plan}") && region != "restricted""#
                    ),
                )
            } else {
                let source_path = fixture_dir.join("src").join("lib.rs");
                replace_in_file(
                    &source_path,
                    "if policy::can_use_beta_dashboard(plan, region, override_active) {",
                    &format!(
                        r#"if plan == "{target_plan}" || policy::can_use_beta_dashboard(plan, region, override_active) {{ "#
                    ),
                )
            }
        }
        _ => Err(format!("unknown Machine League fixture {}", fixture.id)),
    }
}

fn replace_in_file(path: &Path, from: &str, to: &str) -> Result<bool, String> {
    let before = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let after = before.replace(from, to);
    let changed = after != before;
    if changed {
        std::fs::write(path, after).map_err(|error| error.to_string())?;
    }
    Ok(changed)
}

fn inferred_native_strategy_profile(
    kernel: &MdxKernel,
    fixture: MachineLeagueEvalFixture,
) -> Option<String> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
        .find(|receipt| {
            payload_value(receipt, "machine_league_native_improvement_loop") == "true"
                && payload_value(receipt, "machine_league_eval_fixture_family") == fixture.family_id
                && payload_value(receipt, "native_improvement_loop_status")
                    == "BUILDER_LOOP_TICK_RECORDED"
        })
        .map(|_| fixture.native_improved_strategy.to_string())
}

fn codex_trial_prompt(
    body: &str,
    task: &FleetBenchmarkTask,
    language_task: &ForgeLanguageTaskCorpusEntry,
    selected_checks: &str,
) -> String {
    let operator_intent = json_string_field(body, "intent").unwrap_or_else(|| {
        "Make the smallest safe Rust Cargo bug-fix style improvement inside the allowed scope."
            .to_string()
    });
    format!(
        "You are Codex CLI running as an external Machine League runner inside an isolated MDx Forge trial worktree.\n\
Task class: {} / {}.\n\
Language task: {}.\n\
Operator intent: {}.\n\
Allowed write scope: {}.\n\
Do not edit generated artifacts, lockfiles, credentials, .git, .codex, or files outside the allowed scope unless the intent explicitly requires it.\n\
Make the smallest reviewable change you can justify. Prefer adding or updating a focused regression test when behavior changes.\n\
Run or preserve this visible check before finishing: {}.\n\
Leave a concise final summary with changed files, checks run, and residual risk.",
        task.class,
        task.complexity_tier,
        language_task.task_corpus_id,
        operator_intent,
        task.allowed_scope,
        selected_checks
    )
}

fn write_machine_league_fixture(
    fixture_dir: &Path,
    fixture: MachineLeagueEvalFixture,
) -> Result<(), String> {
    if fixture_dir.exists() {
        std::fs::remove_dir_all(fixture_dir).map_err(|error| error.to_string())?;
    }
    match fixture.id {
        MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID => write_rust_tax_fixture(fixture_dir),
        MACHINE_LEAGUE_RUST_POLICY_FIXTURE_ID | MACHINE_LEAGUE_RUST_POLICY_VARIANT_FIXTURE_ID => {
            write_rust_feature_policy_fixture(fixture_dir, fixture)
        }
        _ => Err(format!("unknown Machine League fixture {}", fixture.id)),
    }
}

fn write_machine_league_hidden_test(
    fixture_dir: &Path,
    fixture: MachineLeagueEvalFixture,
) -> Result<(), String> {
    match fixture.id {
        MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID => write_rust_tax_hidden_test(fixture_dir),
        MACHINE_LEAGUE_RUST_POLICY_FIXTURE_ID | MACHINE_LEAGUE_RUST_POLICY_VARIANT_FIXTURE_ID => {
            write_rust_feature_policy_hidden_test(fixture_dir, fixture)
        }
        _ => Err(format!("unknown Machine League fixture {}", fixture.id)),
    }
}

fn write_rust_tax_fixture(fixture_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(fixture_dir.join("src")).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(fixture_dir.join("tests")).map_err(|error| error.to_string())?;
    std::fs::write(
        fixture_dir.join("Cargo.toml"),
        r#"[package]
name = "machine-league-rust-tax-rounding"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[workspace]
"#,
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        fixture_dir.join("Cargo.lock"),
        r#"# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 4

[[package]]
name = "machine-league-rust-tax-rounding"
version = "0.1.0"
"#,
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(fixture_dir.join(".gitignore"), "target/\n")
        .map_err(|error| error.to_string())?;
    std::fs::write(
        fixture_dir.join("src").join("lib.rs"),
        r#"pub fn discounted_total_cents(subtotal_cents: u32, discount_basis_points: u32) -> u32 {
    let discount_cents = subtotal_cents * discount_basis_points / 10_000;
    subtotal_cents - discount_cents
}

pub fn tax_cents(total_after_discount_cents: u32, tax_basis_points: u32) -> u32 {
    total_after_discount_cents * tax_basis_points / 10_000
}

pub fn invoice_total_cents(
    subtotal_cents: u32,
    discount_basis_points: u32,
    tax_basis_points: u32,
) -> u32 {
    let discounted = discounted_total_cents(subtotal_cents, discount_basis_points);

    // BUG: tax should be calculated on the discounted subtotal, not the original subtotal.
    discounted + tax_cents(subtotal_cents, tax_basis_points)
}
"#,
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        fixture_dir.join("tests").join("visible.rs"),
        r#"use machine_league_rust_tax_rounding::invoice_total_cents;

#[test]
fn applies_tax_after_discount_on_round_dollars() {
    assert_eq!(invoice_total_cents(10_000, 1_000, 800), 9_720);
}
"#,
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn write_rust_tax_hidden_test(fixture_dir: &Path) -> Result<(), String> {
    std::fs::write(
        fixture_dir.join("tests").join("hidden.rs"),
        r#"use machine_league_rust_tax_rounding::invoice_total_cents;

#[test]
fn applies_tax_after_discount_for_awkward_cents() {
    assert_eq!(invoice_total_cents(12_345, 750, 875), 12_419);
}
"#,
    )
    .map_err(|error| error.to_string())
}

fn write_rust_feature_policy_fixture(
    fixture_dir: &Path,
    fixture: MachineLeagueEvalFixture,
) -> Result<(), String> {
    std::fs::create_dir_all(fixture_dir.join("src")).map_err(|error| error.to_string())?;
    std::fs::create_dir_all(fixture_dir.join("tests")).map_err(|error| error.to_string())?;
    std::fs::write(
        fixture_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[workspace]
"#,
            fixture.package_name
        ),
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        fixture_dir.join("Cargo.lock"),
        format!(
            r#"# This file is automatically @generated by Cargo.
# It is not intended for manual editing.
version = 4

[[package]]
name = "{}"
version = "0.1.0"
"#,
            fixture.package_name
        ),
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(fixture_dir.join(".gitignore"), "target/\n")
        .map_err(|error| error.to_string())?;
    std::fs::write(
        fixture_dir.join("src").join("policy.rs"),
        r#"pub fn can_use_beta_dashboard(plan: &str, region: &str, override_active: bool) -> bool {
    if override_active {
        return true;
    }

    plan == "enterprise" && region != "restricted"
}
"#,
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(
        fixture_dir.join("src").join("lib.rs"),
        r#"pub mod policy;

pub fn dashboard_route(plan: &str, region: &str, override_active: bool) -> &'static str {
    if policy::can_use_beta_dashboard(plan, region, override_active) {
        "/beta-dashboard"
    } else {
        "/upgrade"
    }
}
"#,
    )
    .map_err(|error| error.to_string())?;
    let visible_case = if fixture.id == MACHINE_LEAGUE_RUST_POLICY_VARIANT_FIXTURE_ID {
        r#"use __CRATE__::dashboard_route;

#[test]
fn partner_plan_can_open_beta_dashboard_in_general_region() {
    assert_eq!(dashboard_route("partner", "us", false), "/beta-dashboard");
}
"#
        .replace("__CRATE__", fixture.crate_name)
    } else {
        r#"use __CRATE__::dashboard_route;

#[test]
fn pro_plan_can_open_beta_dashboard_in_general_region() {
    assert_eq!(dashboard_route("pro", "us", false), "/beta-dashboard");
}
"#
        .replace("__CRATE__", fixture.crate_name)
    };
    std::fs::write(fixture_dir.join("tests").join("visible.rs"), visible_case)
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn write_rust_feature_policy_hidden_test(
    fixture_dir: &Path,
    fixture: MachineLeagueEvalFixture,
) -> Result<(), String> {
    let hidden_case = if fixture.id == MACHINE_LEAGUE_RUST_POLICY_VARIANT_FIXTURE_ID {
        r#"use __CRATE__::dashboard_route;

#[test]
fn partner_plan_still_respects_restricted_region() {
    assert_eq!(dashboard_route("partner", "restricted", false), "/upgrade");
}

#[test]
fn override_still_opens_beta_dashboard() {
    assert_eq!(dashboard_route("basic", "restricted", true), "/beta-dashboard");
}
"#
        .replace("__CRATE__", fixture.crate_name)
    } else {
        r#"use __CRATE__::dashboard_route;

#[test]
fn pro_plan_still_respects_restricted_region() {
    assert_eq!(dashboard_route("pro", "restricted", false), "/upgrade");
}

#[test]
fn override_still_opens_beta_dashboard() {
    assert_eq!(dashboard_route("basic", "restricted", true), "/beta-dashboard");
}
"#
        .replace("__CRATE__", fixture.crate_name)
    };
    std::fs::write(fixture_dir.join("tests").join("hidden.rs"), hidden_case)
        .map_err(|error| error.to_string())
}

fn init_fixture_git_baseline(fixture_dir: &Path) -> Result<(), String> {
    let init = run_process_with_timeout(
        Command::new("git")
            .arg("init")
            .arg("--quiet")
            .current_dir(fixture_dir),
        Duration::from_secs(30),
    )?;
    if !init.success() {
        return Err(format!(
            "unable to initialize fixture git baseline: {}",
            init.stderr.trim()
        ));
    }
    let add = run_process_with_timeout(
        Command::new("git")
            .arg("add")
            .arg(".")
            .current_dir(fixture_dir),
        Duration::from_secs(30),
    )?;
    if !add.success() {
        return Err(format!(
            "unable to stage fixture baseline: {}",
            add.stderr.trim()
        ));
    }
    let commit = run_process_with_timeout(
        Command::new("git")
            .arg("-c")
            .arg("user.name=MDx Machine League")
            .arg("-c")
            .arg("user.email=machine-league@mdx.local")
            .arg("commit")
            .arg("--quiet")
            .arg("-m")
            .arg("fixture baseline")
            .current_dir(fixture_dir),
        Duration::from_secs(30),
    )?;
    if !commit.success() {
        return Err(format!(
            "unable to commit fixture baseline: {}",
            commit.stderr.trim()
        ));
    }
    Ok(())
}

fn codex_rust_tax_fixture_prompt(body: &str) -> String {
    let operator_intent = json_string_field(body, "intent").unwrap_or_else(|| {
        "Fix the invoice tax calculation bug in the Rust fixture without weakening tests."
            .to_string()
    });
    format!(
        "You are Codex CLI running as an external Machine League runner inside a small held-out Rust fixture.\n\
Operator intent: {}.\n\
Known bug: invoice_total_cents currently calculates tax on the original subtotal. It must calculate tax on the discounted subtotal.\n\
Allowed write scope: src/lib.rs. You may add a small regression test only if it helps, but do not edit or weaken tests/visible.rs.\n\
Visible check: {}.\n\
Hidden check: Forge will add and run an additional hidden Rust test after your turn, so do not rely on editing the visible test.\n\
Make the smallest reviewable fix. Run the visible check before finishing. Leave a concise final summary with changed files, checks run, and residual risk.",
        operator_intent, MACHINE_LEAGUE_RUST_TAX_VISIBLE_CHECK
    )
}

fn codex_fixture_prompt(body: &str, fixture: MachineLeagueEvalFixture) -> String {
    if fixture.id == MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID {
        return codex_rust_tax_fixture_prompt(body);
    }
    let operator_intent = json_string_field(body, "intent").unwrap_or_else(|| {
        format!(
            "Fix the {} Rust fixture without weakening tests.",
            fixture.title
        )
    });
    format!(
        "You are Codex CLI running as an external Machine League runner inside a small held-out Rust fixture.\n\
Operator intent: {}.\n\
Fixture: {}.\n\
Transferable edge under evaluation: {}.\n\
The visible failure is in dashboard routing, but the behavior is split across src/lib.rs and src/policy.rs.\n\
Allowed write scope: {}. Do not edit or weaken tests/visible.rs.\n\
Visible check: {}.\n\
Hidden check: Forge will add and run an additional hidden Rust test after your turn, so do not rely on editing the visible test.\n\
Make the smallest reviewable fix. Prefer fixing the shared policy boundary over special-casing the visible route. Run the visible check before finishing. Leave a concise final summary with changed files, checks run, and residual risk.",
        operator_intent,
        fixture.title,
        fixture.transferable_edge,
        fixture.allowed_scope,
        fixture.visible_check
    )
}

fn gemini_rust_tax_fixture_prompt(body: &str) -> String {
    let operator_intent = json_string_field(body, "intent").unwrap_or_else(|| {
        "Fix the invoice tax calculation bug in the Rust fixture without weakening tests."
            .to_string()
    });
    format!(
        "You are Gemini CLI running as an external Machine League runner inside a small held-out Rust fixture.\n\
Operator intent: {}.\n\
Known bug: invoice_total_cents currently calculates tax on the original subtotal. It must calculate tax on the discounted subtotal.\n\
Allowed write scope: src/lib.rs. You may add a small regression test only if it helps, but do not edit or weaken tests/visible.rs.\n\
Visible check: {}.\n\
Hidden check: Forge will add and run an additional hidden Rust test after your turn, so do not rely on editing the visible test.\n\
Make the smallest reviewable fix. Run the visible check before finishing. Leave a concise final summary with changed files, checks run, and residual risk.",
        operator_intent, MACHINE_LEAGUE_RUST_TAX_VISIBLE_CHECK
    )
}

fn open_machine_rust_tax_fixture_prompt(body: &str, spec: OpenMachineTrialSpec) -> String {
    let operator_intent = json_string_field(body, "intent").unwrap_or_else(|| {
        "Fix the invoice tax calculation bug in the Rust fixture without weakening tests."
            .to_string()
    });
    let verification_instruction = if matches!(
        spec.adapter_kind,
        "grok_build_cli" | "claude_code" | "kimi_code"
    ) {
        "Do not request a shell or test tool. MDx owns and runs the visible and hidden checks after your session. After the scoped edit, leave a concise final summary and finish."
    } else {
        "Run the visible check before finishing. Leave a concise final summary with changed files, checks run, and residual risk."
    };
    format!(
        "You are {} running as an external Machine League runner inside a small held-out Rust fixture.\n\
Operator intent: {}.\n\
Known bug: invoice_total_cents currently calculates tax on the original subtotal. It must calculate tax on the discounted subtotal.\n\
Allowed write scope: src/lib.rs. You may add a small regression test only if it helps, but do not edit or weaken tests/visible.rs.\n\
Visible check: {}.\n\
Hidden check: Forge will add and run an additional hidden Rust test after your turn, so do not rely on editing the visible test.\n\
Make the smallest reviewable fix. {}",
        spec.display_name,
        operator_intent,
        MACHINE_LEAGUE_RUST_TAX_VISIBLE_CHECK,
        verification_instruction,
    )
}

struct ProcessCapture {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
}

impl ProcessCapture {
    fn success(&self) -> bool {
        self.code == Some(0) && !self.timed_out
    }
}

fn run_process_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<ProcessCapture, String> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let start = Instant::now();
    let mut timed_out = false;
    loop {
        if child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            break;
        }
        if start.elapsed() >= timeout {
            timed_out = true;
            let _ = child.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let output = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    Ok(ProcessCapture {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        timed_out,
    })
}

fn codex_binary_path() -> PathBuf {
    if let Ok(path) = std::env::var("MDX_MACHINE_LEAGUE_CODEX_BINARY") {
        return PathBuf::from(path);
    }
    let app_binary = PathBuf::from("/Applications/Codex.app/Contents/Resources/codex");
    if app_binary.is_file() {
        app_binary
    } else {
        PathBuf::from("codex")
    }
}

fn gemini_binary_path() -> PathBuf {
    std::env::var("MDX_MACHINE_LEAGUE_GEMINI_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("gemini"))
}

fn codex_timeout_seconds() -> u64 {
    std::env::var("MDX_MACHINE_LEAGUE_CODEX_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| (30..=1800).contains(seconds))
        .unwrap_or(300)
}

fn gemini_timeout_seconds() -> u64 {
    std::env::var("MDX_MACHINE_LEAGUE_GEMINI_TIMEOUT_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| (30..=1800).contains(seconds))
        .unwrap_or(300)
}

fn codex_live_execution_enabled() -> bool {
    std::env::var("MDX_MACHINE_LEAGUE_CODEX_EXEC")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn gemini_live_execution_enabled() -> bool {
    std::env::var("MDX_MACHINE_LEAGUE_GEMINI_EXEC")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn observe_machine_runtime_fingerprint(
    kernel: Option<&MdxKernel>,
    runner_id: &str,
    adapter_kind: &str,
    binary_name: &str,
    command_contract: &str,
) -> MachineRuntimeFingerprint {
    let observation = observe_machine_runtime_static(adapter_kind, binary_name);
    machine_runtime_fingerprint_from_static(
        kernel,
        runner_id,
        adapter_kind,
        binary_name,
        command_contract,
        observation,
    )
}

fn observe_machine_runtime_static_batch(
    runners: &[FleetRunnerProfile],
) -> Vec<MachineRuntimeStaticObservation> {
    std::thread::scope(|scope| {
        let probes = runners
            .iter()
            .map(|runner| {
                scope.spawn(move || {
                    observe_machine_runtime_static(
                        runner.adapter_kind,
                        adapter_binary_name(runner.adapter_kind),
                    )
                })
            })
            .collect::<Vec<_>>();
        probes
            .into_iter()
            .zip(runners)
            .map(|(probe, runner)| {
                probe.join().unwrap_or_else(|_| {
                    failed_machine_runtime_static_observation(runner.adapter_kind)
                })
            })
            .collect()
    })
}

fn failed_machine_runtime_static_observation(
    adapter_kind: &str,
) -> MachineRuntimeStaticObservation {
    MachineRuntimeStaticObservation {
        binary_path: String::new(),
        binary_present: false,
        version_command: adapter_version_command(adapter_kind, adapter_binary_name(adapter_kind)),
        version_observed: String::new(),
        version_raw_output: "runtime_observation_failed".to_string(),
        checksum_sha256: String::new(),
    }
}

fn observe_machine_runtime_static(
    adapter_kind: &str,
    binary_name: &str,
) -> MachineRuntimeStaticObservation {
    let binary_path = resolve_binary_path(binary_name);
    let binary_present = !binary_path.is_empty();
    let version_command = adapter_version_command(adapter_kind, binary_name);
    if !binary_present {
        return MachineRuntimeStaticObservation {
            binary_path,
            binary_present,
            version_command,
            version_observed: String::new(),
            version_raw_output: String::new(),
            checksum_sha256: String::new(),
        };
    }

    let cache_path = binary_path.clone();
    cached_machine_runtime_static_observation(adapter_kind, &cache_path, || {
        collect_machine_runtime_static_observation(adapter_kind, binary_path, version_command)
    })
}

fn collect_machine_runtime_static_observation(
    adapter_kind: &str,
    binary_path: String,
    version_command: String,
) -> MachineRuntimeStaticObservation {
    let version_raw_output = machine_runtime_version_output(&binary_path, adapter_kind);
    let checksum_sha256 = sha256_file_hex(Path::new(&binary_path))
        .map(|digest| format!("sha256:{digest}"))
        .unwrap_or_default();
    MachineRuntimeStaticObservation {
        version_observed: first_nonempty_line(&version_raw_output),
        version_raw_output: cap_chars(&version_raw_output, 500),
        binary_path,
        binary_present: true,
        version_command,
        checksum_sha256,
    }
}

fn machine_runtime_fingerprint_from_static(
    kernel: Option<&MdxKernel>,
    runner_id: &str,
    adapter_kind: &str,
    binary_name: &str,
    command_contract: &str,
    observation: MachineRuntimeStaticObservation,
) -> MachineRuntimeFingerprint {
    let MachineRuntimeStaticObservation {
        binary_path,
        binary_present,
        version_command,
        version_observed,
        version_raw_output,
        checksum_sha256,
    } = observation;
    let fingerprint_material = format!(
        "runner_id={runner_id}\nadapter_kind={adapter_kind}\nbinary_name={binary_name}\nbinary_path={binary_path}\nbinary_present={binary_present}\nversion_command={version_command}\nversion_observed={version_observed}\nchecksum_sha256={checksum_sha256}\nadapter_contract_version={MACHINE_RUNTIME_ADAPTER_CONTRACT_VERSION}\ncommand_contract={command_contract}\n"
    );
    let fingerprint_id = format!("sha256:{}", hex(&sha256(fingerprint_material.as_bytes())));
    let last_fingerprint_id = kernel
        .and_then(|kernel| latest_machine_runtime_fingerprint_id(kernel, runner_id))
        .unwrap_or_default();
    let drift_status = machine_runtime_drift_status(
        binary_present,
        &fingerprint_id,
        &last_fingerprint_id,
        adapter_kind,
    );
    let compatibility_status = if adapter_kind == "mdx_native" {
        "native_runtime"
    } else if !binary_present {
        "binary_missing"
    } else if version_observed.is_empty() {
        "version_unobserved"
    } else {
        "runtime_observed"
    };
    MachineRuntimeFingerprint {
        runner_id: runner_id.to_string(),
        adapter_kind: adapter_kind.to_string(),
        binary_name: binary_name.to_string(),
        binary_path,
        binary_present,
        version_command,
        version_observed,
        version_raw_output,
        checksum_sha256,
        adapter_contract_version: MACHINE_RUNTIME_ADAPTER_CONTRACT_VERSION.to_string(),
        command_contract: command_contract.to_string(),
        fingerprint_id,
        drift_status: drift_status.to_string(),
        last_fingerprint_id,
        compatibility_status: compatibility_status.to_string(),
    }
}

fn machine_runtime_json(runtime: &MachineRuntimeFingerprint) -> String {
    format!(
        r#"{{"runner_id":{},"adapter_kind":{},"binary_name":{},"binary_path":{},"binary_present":{},"version_command":{},"version_observed":{},"version_raw_output":{},"checksum_sha256":{},"adapter_contract_version":{},"command_contract":{},"fingerprint_id":{},"drift_status":{},"last_fingerprint_id":{},"compatibility_status":{}}}"#,
        json_string_literal(&runtime.runner_id),
        json_string_literal(&runtime.adapter_kind),
        json_string_literal(&runtime.binary_name),
        json_string_literal(&runtime.binary_path),
        runtime.binary_present,
        json_string_literal(&runtime.version_command),
        json_string_literal(&runtime.version_observed),
        json_string_literal(&runtime.version_raw_output),
        json_string_literal(&runtime.checksum_sha256),
        json_string_literal(&runtime.adapter_contract_version),
        json_string_literal(&runtime.command_contract),
        json_string_literal(&runtime.fingerprint_id),
        json_string_literal(&runtime.drift_status),
        json_string_literal(&runtime.last_fingerprint_id),
        json_string_literal(&runtime.compatibility_status),
    )
}

fn latest_machine_runtime_fingerprint_id(kernel: &MdxKernel, runner_id: &str) -> Option<String> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
        .find(|receipt| {
            payload_value(receipt, "runner_profile_runner_id") == runner_id
                && !payload_value(receipt, "machine_runtime_fingerprint_id").is_empty()
        })
        .map(|receipt| payload_value(receipt, "machine_runtime_fingerprint_id").to_string())
}

fn machine_runtime_drift_status(
    binary_present: bool,
    fingerprint_id: &str,
    last_fingerprint_id: &str,
    adapter_kind: &str,
) -> &'static str {
    if adapter_kind == "mdx_native" {
        "runtime_native"
    } else if !binary_present {
        "runtime_unverified_binary_missing"
    } else if last_fingerprint_id.is_empty() {
        "runtime_unverified_first_observation"
    } else if last_fingerprint_id == fingerprint_id {
        "runtime_stable"
    } else {
        "runtime_drift_detected"
    }
}

fn resolve_binary_path(binary_name: &str) -> String {
    if binary_name.trim().is_empty() {
        return String::new();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return String::new();
    };
    std::env::split_paths(&paths)
        .map(|path| path.join(binary_name))
        .find(|path| path.is_file())
        .map(|path| path.display().to_string())
        .unwrap_or_default()
}

fn adapter_version_command(adapter_kind: &str, binary_name: &str) -> String {
    if binary_name.trim().is_empty() {
        return String::new();
    }
    let args = adapter_version_args(adapter_kind);
    if args.is_empty() {
        binary_name.to_string()
    } else {
        format!("{binary_name} {}", args.join(" "))
    }
}

fn adapter_version_args(adapter_kind: &str) -> Vec<&'static str> {
    match adapter_kind {
        "codex_cli" | "grok_build_cli" | "claude_code" | "kimi_code" | "gemini_cli"
        | "opencode" | "cline" | "goose" => vec!["--version"],
        _ => Vec::new(),
    }
}

fn adapter_command_contract(adapter_kind: &str) -> &'static str {
    match adapter_kind {
        "codex_cli" => CODEX_EXEC_POLICY_COMMAND,
        "grok_build_cli" => GROK_BUILD_EXEC_POLICY_COMMAND,
        "claude_code" => CLAUDE_CODE_EXEC_POLICY_COMMAND,
        "kimi_code" => KIMI_CODE_EXEC_POLICY_COMMAND,
        "gemini_cli" => GEMINI_EXEC_POLICY_COMMAND,
        "opencode" => OPENCODE_EXEC_POLICY_COMMAND,
        "cline" => CLINE_EXEC_POLICY_COMMAND,
        "goose" => GOOSE_EXEC_POLICY_COMMAND,
        _ => "",
    }
}

fn machine_runtime_version_output(binary_path: &str, adapter_kind: &str) -> String {
    let mut command = Command::new(binary_path);
    for arg in adapter_version_args(adapter_kind) {
        command.arg(arg);
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    match run_process_with_timeout(&mut command, Duration::from_secs(3)) {
        Ok(output) => {
            let combined = format!("{}\n{}", output.stdout.trim(), output.stderr.trim());
            combined.trim().to_string()
        }
        Err(error) => format!("version_probe_failed: {error}"),
    }
}

fn first_nonempty_line(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("")
        .to_string()
}

fn cap_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn adapter_binary_name(adapter_kind: &str) -> &'static str {
    match adapter_kind {
        "codex_cli" => "codex",
        "grok_build_cli" => "grok",
        "claude_code" => "claude",
        "kimi_code" => "kimi",
        "gemini_cli" => "gemini",
        "opencode" => "opencode",
        "cline" => "cline",
        "goose" => "goose",
        _ => "",
    }
}

fn adapter_live_env_required(adapter_kind: &str) -> &'static str {
    match adapter_kind {
        "codex_cli" => "MDX_MACHINE_LEAGUE_CODEX_EXEC=1",
        "grok_build_cli" => "MDX_MACHINE_LEAGUE_GROK_BUILD_EXEC=1",
        "claude_code" => "MDX_MACHINE_LEAGUE_CLAUDE_CODE_EXEC=1",
        "kimi_code" => "MDX_MACHINE_LEAGUE_KIMI_CODE_EXEC=1",
        "gemini_cli" => "MDX_MACHINE_LEAGUE_GEMINI_EXEC=1",
        "opencode" => "MDX_MACHINE_LEAGUE_OPENCODE_EXEC=1",
        "cline" => "MDX_MACHINE_LEAGUE_CLINE_EXEC=1",
        "goose" => "MDX_MACHINE_LEAGUE_GOOSE_EXEC=1",
        _ => "",
    }
}

fn adapter_live_execution_enabled(adapter_kind: &str) -> bool {
    match adapter_kind {
        "codex_cli" => codex_live_execution_enabled(),
        "grok_build_cli" => {
            open_machine_live_execution_enabled("MDX_MACHINE_LEAGUE_GROK_BUILD_EXEC")
        }
        "claude_code" => open_machine_live_execution_enabled("MDX_MACHINE_LEAGUE_CLAUDE_CODE_EXEC"),
        "kimi_code" => open_machine_live_execution_enabled("MDX_MACHINE_LEAGUE_KIMI_CODE_EXEC"),
        "gemini_cli" => gemini_live_execution_enabled(),
        "opencode" => open_machine_live_execution_enabled("MDX_MACHINE_LEAGUE_OPENCODE_EXEC"),
        "cline" => open_machine_live_execution_enabled("MDX_MACHINE_LEAGUE_CLINE_EXEC"),
        "goose" => open_machine_live_execution_enabled("MDX_MACHINE_LEAGUE_GOOSE_EXEC"),
        _ => false,
    }
}

fn open_machine_live_execution_enabled(env_key: &str) -> bool {
    std::env::var(env_key)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn open_machine_timeout_seconds(adapter_kind: &str) -> u64 {
    let env_key = match adapter_kind {
        "opencode" => "MDX_MACHINE_LEAGUE_OPENCODE_TIMEOUT_SECONDS",
        "cline" => "MDX_MACHINE_LEAGUE_CLINE_TIMEOUT_SECONDS",
        "goose" => "MDX_MACHINE_LEAGUE_GOOSE_TIMEOUT_SECONDS",
        "kimi_code" => "MDX_MACHINE_LEAGUE_KIMI_CODE_TIMEOUT_SECONDS",
        _ => "MDX_MACHINE_LEAGUE_OPEN_MACHINE_TIMEOUT_SECONDS",
    };
    std::env::var(env_key)
        .or_else(|_| std::env::var("MDX_MACHINE_LEAGUE_OPEN_MACHINE_TIMEOUT_SECONDS"))
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| (30..=1800).contains(seconds))
        .unwrap_or(300)
}

fn open_machine_auth_blocked(stdout: &str, stderr: &str) -> bool {
    let haystack = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    [
        "not authenticated",
        "authentication",
        "auth required",
        "login required",
        "please login",
        "api key",
        "missing key",
        "unauthorized",
        "permission denied",
    ]
    .iter()
    .any(|needle| haystack.contains(needle))
}

fn live_principal_review_gate_results(
    required_gates: &str,
    codex_success: bool,
    check_passed: bool,
    changed_files: bool,
) -> String {
    required_gates
        .split(',')
        .filter_map(|gate| {
            let gate = gate.trim();
            if gate.is_empty() {
                return None;
            }
            let gate_id = gate.split(':').next().unwrap_or(gate);
            let value = match gate_id {
                "behavior" => {
                    if changed_files {
                        "changed_behavior_is_explicit"
                    } else {
                        "no_diff_recorded"
                    }
                }
                "tests" => {
                    if check_passed {
                        "visible_check_passed"
                    } else {
                        "visible_check_failed"
                    }
                }
                "security" => "no_secret_or_authority_expansion_claimed_by_mdx_adapter",
                "stack_idioms" | "compatibility" | "maintainability" => {
                    if codex_success {
                        "pending_principal_review_gate_result"
                    } else {
                        "codex_execution_failed"
                    }
                }
                _ => "pending_principal_review_gate_result",
            };
            Some(format!("{gate_id}={value}"))
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn live_trial_quality_assessment(live: &CodexLiveTrialArtifacts) -> RunEvalQualityGateAssessment {
    live_cli_trial_quality_assessment(
        live.codex_exit_code,
        live.check_passed,
        live.changed_file_count,
    )
}

fn live_cli_trial_quality_assessment(
    runner_exit_code: i32,
    check_passed: bool,
    changed_file_count: u32,
) -> RunEvalQualityGateAssessment {
    let gates = [
        ("terminal_done", runner_exit_code == 0),
        ("real_diff_present", changed_file_count > 0),
        ("proof_coverage_satisfied", check_passed),
        ("proof_scope_covered", check_passed),
        (
            "behavior_proof_covered",
            check_passed && changed_file_count > 0,
        ),
        ("semantic_orientation_observed", true),
        ("related_tests_observed", check_passed),
        ("artifact_noise_folded", true),
    ];
    let mut passed_gates = Vec::new();
    let mut failed_gates = Vec::new();
    for (gate, passed) in gates {
        if passed {
            passed_gates.push(gate);
        } else {
            failed_gates.push(gate);
        }
    }
    let gates_passed = failed_gates.is_empty();
    RunEvalQualityGateAssessment {
        gates_passed,
        status: if gates_passed {
            "READY_FOR_PRINCIPAL_REVIEW"
        } else {
            "BLOCKED_BEFORE_PRINCIPAL_REVIEW"
        },
        passed_gates,
        failed_gates,
        can_be_scored_after_principal_review: gates_passed,
    }
}

fn render_distillation_note_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let distillation_note_id = json_string_field(body, "distillation_note_id")
        .unwrap_or_else(|| "distillation_note_codex_trial_001".to_string());
    let source_run_id = json_string_field(body, "source_run_id")
        .unwrap_or_else(|| "forge_machine_league_codex_trial_000001".to_string());
    let distillation_reason = json_string_field(body, "distillation_reason")
        .unwrap_or_else(|| "planning_strategy".to_string());
    if !valid_distillation_reason(&distillation_reason) {
        return Err("distillation_reason is not in the machine league taxonomy".to_string());
    }
    let winning_runner_id = json_string_field(body, "winning_runner_id")
        .unwrap_or_else(|| "codex_cli_external_worker".to_string());
    let baseline_runner_id = json_string_field(body, "baseline_runner_id")
        .unwrap_or_else(|| "mdx_native_harness_runner".to_string());
    let evidence_gate = match distillation_evidence_gate(
        kernel,
        &source_run_id,
        &winning_runner_id,
        &baseline_runner_id,
    ) {
        Ok(gate) => gate,
        Err(reason) => {
            return record_distillation_note_refusal(kernel, &reason, &source_run_id);
        }
    };
    let language_pack_id = json_string_field(body, "language_pack_id")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| evidence_gate.language_pack_id.clone());
    let task_class = json_string_field(body, "task_class")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| evidence_gate.task_class.clone());
    let native_improvement_work_item_id =
        json_string_field(body, "native_improvement_work_item_id")
            .unwrap_or_else(|| native_improvement_work_item_for_reason(&distillation_reason));
    let compared_transcript_refs = json_string_field(body, "compared_transcript_refs")
        .unwrap_or_else(|| "quarantine://forge/fleet-eval/compared-transcripts.jsonl".to_string());
    let compared_diff_refs = json_string_field(body, "compared_diff_refs")
        .unwrap_or_else(|| "quarantine://forge/fleet-eval/compared-diffs.patch".to_string());
    let quality_gate_receipt_ids = json_string_field(body, "quality_gate_receipt_ids")
        .unwrap_or_else(|| "pending_quality_gate_receipts".to_string());
    let review_packet_refs = json_string_field(body, "review_packet_refs")
        .unwrap_or_else(|| "quarantine://forge/fleet-eval/review-packet.json".to_string());
    let confidence = json_string_field(body, "confidence").unwrap_or_else(|| "medium".to_string());
    let identity = GovernedWriteIdentity::local_demo("agent:forge_eval");
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:forge_eval",
                run_id: &source_run_id,
                event: "evidence_appended",
                work_item_id: "machine_league_distillation",
                detail: &format!(
                    "machine_league_distillation_note reason={} winning_runner={}",
                    distillation_reason, winning_runner_id
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &identity,
            &[
                ("machine_league_distillation_note_id", &distillation_note_id),
                ("source_run_id", &source_run_id),
                ("winning_runner_id", &winning_runner_id),
                ("baseline_runner_id", &baseline_runner_id),
                ("task_class", &task_class),
                ("language_pack_id", &language_pack_id),
                ("distillation_reason", &distillation_reason),
                ("compared_transcript_refs", &compared_transcript_refs),
                ("compared_diff_refs", &compared_diff_refs),
                ("quality_gate_receipt_ids", &quality_gate_receipt_ids),
                ("review_packet_refs", &review_packet_refs),
                ("distillation_evidence_gate", evidence_gate.gate),
                (
                    "distillation_external_review_receipt_id",
                    &evidence_gate.external_review_receipt_id,
                ),
                (
                    "distillation_pairwise_comparison_receipt_id",
                    &evidence_gate.pairwise_comparison_receipt_id,
                ),
                (
                    "distillation_native_evidence_receipt_id",
                    &evidence_gate.native_evidence_receipt_id,
                ),
                ("machine_league_eval_fixture", &evidence_gate.eval_fixture),
                (
                    "eval_fixture_snapshot_id",
                    &evidence_gate.eval_fixture_snapshot_id,
                ),
                (
                    "native_improvement_work_item_id",
                    &native_improvement_work_item_id,
                ),
                ("author_actor_id", "agent:forge_eval"),
                ("confidence", &confidence),
                (
                    "status",
                    "DISTILLATION_NOTE_RECORDED_PENDING_NATIVE_IMPROVEMENT",
                ),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-distillation-note-local-post","status":"DISTILLATION_NOTE_RECORDED_PENDING_NATIVE_IMPROVEMENT","distillation_note_id":{},"source_run_id":{},"winning_runner_id":{},"baseline_runner_id":{},"distillation_reason":{},"distillation_evidence_gate":{},"receipt_id":{},"compared_transcript_refs":{},"compared_diff_refs":{},"quality_gate_receipt_ids":{},"review_packet_refs":{},"native_improvement_work_item_id":{},"author_actor_id":"agent:forge_eval","confidence":{},"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(&distillation_note_id),
        json_string_literal(&source_run_id),
        json_string_literal(&winning_runner_id),
        json_string_literal(&baseline_runner_id),
        json_string_literal(&distillation_reason),
        json_string_literal(evidence_gate.gate),
        json_string_literal(&report.receipt_id),
        json_string_literal(&compared_transcript_refs),
        json_string_literal(&compared_diff_refs),
        json_string_literal(&quality_gate_receipt_ids),
        json_string_literal(&review_packet_refs),
        json_string_literal(&native_improvement_work_item_id),
        json_string_literal(&confidence),
    ))
}

struct DistillationEvidenceGate {
    gate: &'static str,
    task_class: String,
    language_pack_id: String,
    eval_fixture: String,
    eval_fixture_snapshot_id: String,
    external_review_receipt_id: String,
    pairwise_comparison_receipt_id: String,
    native_evidence_receipt_id: String,
}

fn distillation_evidence_gate(
    kernel: &MdxKernel,
    source_run_id: &str,
    winning_runner_id: &str,
    baseline_runner_id: &str,
) -> Result<DistillationEvidenceGate, String> {
    if baseline_runner_id != "mdx_native_harness_runner" {
        return Err("distillation requires mdx_native_harness_runner as the baseline".to_string());
    }
    let Some(external) = accepted_scorecard_run_for(kernel, source_run_id) else {
        return Err(format!(
            "source run {} is not accepted external scoreboard evidence",
            source_run_id.trim()
        ));
    };
    if external.runner_id == "mdx_native_harness_runner" {
        return Err("source run must be an accepted external machine run".to_string());
    }
    if external.runner_id != winning_runner_id {
        return Err("winning_runner_id must match the accepted external source run".to_string());
    }
    if let Some(pairwise_receipt_id) =
        external_win_pairwise_receipt_id_for_source_run(kernel, &external.run_id)
    {
        return Ok(DistillationEvidenceGate {
            gate: "accepted_pairwise_external_win",
            task_class: external.task_class,
            language_pack_id: external.language_pack_id,
            eval_fixture: external.eval_fixture,
            eval_fixture_snapshot_id: external.eval_fixture_snapshot_id,
            external_review_receipt_id: external.review_receipt_id,
            pairwise_comparison_receipt_id: pairwise_receipt_id,
            native_evidence_receipt_id: String::new(),
        });
    }
    if let Some(native_evidence_receipt_id) =
        failed_native_attempt_receipt_id_for_fixture(kernel, &external.eval_fixture)
    {
        return Ok(DistillationEvidenceGate {
            gate: "native_failed_same_fixture_hidden_oracle",
            task_class: external.task_class,
            language_pack_id: external.language_pack_id,
            eval_fixture: external.eval_fixture,
            eval_fixture_snapshot_id: external.eval_fixture_snapshot_id,
            external_review_receipt_id: external.review_receipt_id,
            pairwise_comparison_receipt_id: String::new(),
            native_evidence_receipt_id,
        });
    }
    Err("distillation requires an accepted same-fixture external win or a failed native same-fixture hidden oracle".to_string())
}

fn external_win_pairwise_receipt_id_for_source_run(
    kernel: &MdxKernel,
    source_run_id: &str,
) -> Option<String> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
        .find(|receipt| {
            payload_value(receipt, "external_run_id") == source_run_id.trim()
                && payload_value(receipt, "baseline_runner_id") == "mdx_native_harness_runner"
                && payload_value(receipt, "pairwise_outcome") == "external_win"
                && payload_value(receipt, "distillation_candidate_allowed") == "true"
        })
        .map(|receipt| receipt.receipt_id.clone())
}

fn failed_native_attempt_receipt_id_for_fixture(
    kernel: &MdxKernel,
    eval_fixture: &str,
) -> Option<String> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
        .find(|receipt| {
            payload_value(receipt, "machine_league_eval_fixture") == eval_fixture
                && (payload_value(receipt, "runner_id") == "mdx_native_harness_runner"
                    || payload_value(receipt, "runner_profile_runner_id")
                        == "mdx_native_harness_runner")
                && payload_value(receipt, "eval_oracle_status")
                    != "visible_and_hidden_checks_passed"
                && !payload_value(receipt, "eval_oracle_status").is_empty()
        })
        .map(|receipt| receipt.receipt_id.clone())
}

fn record_distillation_note_refusal(
    kernel: &mut MdxKernel,
    reason: &str,
    source_run_id: &str,
) -> Result<String, String> {
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:forge_eval",
                run_id: "machine_league_distillation_note_refusal",
                event: "evidence_appended",
                work_item_id: "machine_league_distillation_refusal",
                detail: &format!("machine_league_distillation_refused reason={reason}"),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:forge_eval"),
            &[
                ("machine_league_distillation_note_status", "refused"),
                ("machine_league_distillation_refusal_reason", reason),
                ("source_run_id", source_run_id.trim()),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-machine-league-distillation-note-local-post","status":"REFUSED","reason":{},"source_run_id":{},"refusal_receipt_id":{},"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(reason),
        json_string_literal(source_run_id.trim()),
        json_string_literal(&report.receipt_id),
    ))
}

#[derive(Clone)]
struct DistillationNoteForImprovement {
    note_id: String,
    source_run_id: String,
    winning_runner_id: String,
    baseline_runner_id: String,
    task_class: String,
    language_pack_id: String,
    distillation_reason: String,
    native_improvement_work_item_id: String,
    confidence: String,
    note_receipt_id: String,
}

fn render_native_improvement_loop_json(
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    let requested_note_id = json_string_field(body, "distillation_note_id");
    let note = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        find_distillation_note_for_improvement(&kernel, requested_note_id.as_deref())
    };
    let Some(note) = note else {
        return record_native_improvement_loop_refusal(
            kernel,
            "no matching Machine League distillation note was found",
            requested_note_id.as_deref().unwrap_or(""),
        );
    };
    if note.baseline_runner_id != "mdx_native_harness_runner" {
        return record_native_improvement_loop_refusal(
            kernel,
            "native improvement loops require mdx_native_harness_runner as the baseline",
            &note.note_id,
        );
    }

    let launch_execution = json_bool_field(body, "launch_execution").unwrap_or(false);
    let builder_loop_id =
        json_string_field(body, "builder_loop_id").unwrap_or_else(|| "quality_sweep".to_string());
    if !matches!(
        builder_loop_id.as_str(),
        "quality_sweep" | "local_check_recovery"
    ) {
        return record_native_improvement_loop_refusal(
            kernel,
            "native improvement loops can only launch a governed single-run Builder Loop",
            &note.note_id,
        );
    }

    let eval_fixture = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        eval_fixture_for_source_run(&kernel, &note.source_run_id)
            .unwrap_or_else(|| MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID.to_string())
    };
    let selected_check = json_string_field(body, "selected_check").unwrap_or_else(|| {
        "cargo test -p mdx-server forge_fleet_eval_scoreboard -- --nocapture".to_string()
    });
    let improvement_intent = format!(
        "Improve the MDx native Forge harness for {} based on Machine League distillation note {}. Winning runner: {}. Keep the work governed, cite the note evidence, then remeasure the native runner on fixture {} before claiming improvement.",
        note.distillation_reason, note.note_id, note.winning_runner_id, eval_fixture
    );
    let tick_body = format!(
        r#"{{"loop_id":{},"trigger":"chain","reason":{},"work_item_id":{},"intent":{},"selected_check":{},"launch_execution":{},"actor_id":"agent:forge_eval","tenant_id":"local_tenant"}}"#,
        json_string_literal(&builder_loop_id),
        json_string_literal(&format!(
            "Machine League distillation note {} scheduled native harness improvement",
            note.note_id
        )),
        json_string_literal(&note.native_improvement_work_item_id),
        json_string_literal(&improvement_intent),
        json_string_literal(&selected_check),
        launch_execution,
    );
    let tick_response = crate::forge_builder_loop_route::route_response(
        "POST",
        "/forge/builder-loops/tick.json",
        &tick_body,
        kernel,
    )
    .ok_or_else(|| "Forge Builder Loop route missing".to_string())??;
    let tick_json: serde_json::Value =
        serde_json::from_str(&tick_response.body).map_err(|error| error.to_string())?;
    let builder_loop_status = tick_json["status"].as_str().unwrap_or("UNKNOWN");
    let tick_id = tick_json["tick_id"].as_str().unwrap_or("");
    let tick_receipt_id = tick_json["tick_receipt_id"].as_str().unwrap_or("");
    let launch_status = if launch_execution {
        tick_json["attachment"]["status"]
            .as_str()
            .unwrap_or("launch_attempt_recorded")
    } else {
        "launch_not_requested"
    };
    let (before_native_accepted_count, after_native_accepted_count) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        (
            accepted_score_count_for_runner_and_fixture(
                &kernel,
                "mdx_native_harness_runner",
                &eval_fixture,
            ),
            accepted_score_count_for_runner_after_tick(
                &kernel,
                "mdx_native_harness_runner",
                Some(&eval_fixture),
                tick_receipt_id,
            ),
        )
    };
    let after_native_accepted_count_rendered = after_native_accepted_count
        .map(|count| count.to_string())
        .unwrap_or_else(|| "null".to_string());
    let after_native_accepted_count_payload = after_native_accepted_count
        .map(|count| count.to_string())
        .unwrap_or_default();
    let eval_fixture_contract = machine_league_eval_fixture_or_default(&eval_fixture);
    let loop_id = format!("native_improvement_loop_{}", note.note_id);
    let remeasurement_status = if launch_execution {
        "waiting_for_builder_loop_outcome_before_remeasure"
    } else {
        "waiting_for_operator_to_launch_builder_loop"
    };
    let report = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "local_tenant",
                    actor_id: "agent:forge_eval",
                    run_id: &loop_id,
                    event: "evidence_appended",
                    work_item_id: &note.native_improvement_work_item_id,
                    detail: &format!(
                        "native_improvement_loop_scheduled note={} builder_loop_tick={}",
                        note.note_id, tick_id
                    ),
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &GovernedWriteIdentity::local_demo("agent:forge_eval"),
                &[
                    ("machine_league_native_improvement_loop", "true"),
                    ("machine_league_native_improvement_loop_id", &loop_id),
                    (
                        "machine_league_native_improvement_distillation_note_id",
                        &note.note_id,
                    ),
                    ("source_run_id", &note.source_run_id),
                    ("winning_runner_id", &note.winning_runner_id),
                    ("baseline_runner_id", &note.baseline_runner_id),
                    ("task_class", &note.task_class),
                    ("language_pack_id", &note.language_pack_id),
                    ("machine_league_eval_fixture", &eval_fixture),
                    (
                        "machine_league_eval_fixture_family",
                        eval_fixture_contract.family_id,
                    ),
                    (
                        "machine_league_eval_fixture_sibling",
                        eval_fixture_contract.sibling_fixture_id,
                    ),
                    (
                        "machine_league_transferable_edge",
                        eval_fixture_contract.transferable_edge,
                    ),
                    ("distillation_reason", &note.distillation_reason),
                    ("distillation_confidence", &note.confidence),
                    (
                        "native_improvement_work_item_id",
                        &note.native_improvement_work_item_id,
                    ),
                    ("builder_loop_id", &builder_loop_id),
                    ("builder_loop_tick_id", tick_id),
                    ("builder_loop_tick_receipt_id", tick_receipt_id),
                    ("builder_loop_status", builder_loop_status),
                    (
                        "builder_loop_launch_execution_requested",
                        if launch_execution { "true" } else { "false" },
                    ),
                    ("builder_loop_launch_status", launch_status),
                    (
                        "native_improvement_loop_status",
                        "BUILDER_LOOP_TICK_RECORDED",
                    ),
                    (
                        "remeasurement_route",
                        "/forge/machine-league/native-trials.json",
                    ),
                    ("remeasurement_status", remeasurement_status),
                    ("selected_check", &selected_check),
                    (
                        "before_native_accepted_count",
                        &before_native_accepted_count.to_string(),
                    ),
                    (
                        "after_native_accepted_count",
                        &after_native_accepted_count_payload,
                    ),
                    ("external_output_consumable", "false"),
                ],
            )
            .map_err(|error| error.message())?
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-machine-league-native-improvement-loop-local-post","status":"NATIVE_IMPROVEMENT_LOOP_TICK_RECORDED","route":"/forge/machine-league/native-improvement-loops.json","projection_route":"/forge/machine-league/native-improvement-loops/projection.json","learning_ledger_route":"/forge/machine-league/learning-ledger.json","distillation_note_id":{},"distillation_note_receipt_id":{},"source_run_id":{},"distillation_confidence":{},"native_improvement_loop_id":{},"native_improvement_loop_receipt_id":{},"native_improvement_work_item_id":{},"builder_loop_id":{},"builder_loop_tick_id":{},"builder_loop_tick_receipt_id":{},"builder_loop_status":{},"builder_loop_launch_execution_requested":{},"builder_loop_launch_status":{},"remeasurement_route":"/forge/machine-league/native-trials.json","remeasurement_eval_fixture":{},"remeasurement_status":{},"before_native_accepted_count":{},"after_native_accepted_count":{},"external_output_consumable":false,"production_write_allowed":false}}"#,
            json_string_literal(&note.note_id),
            json_string_literal(&note.note_receipt_id),
            json_string_literal(&note.source_run_id),
            json_string_literal(&note.confidence),
            json_string_literal(&loop_id),
            json_string_literal(&report.receipt_id),
            json_string_literal(&note.native_improvement_work_item_id),
            json_string_literal(&builder_loop_id),
            json_string_literal(tick_id),
            json_string_literal(tick_receipt_id),
            json_string_literal(builder_loop_status),
            launch_execution,
            json_string_literal(launch_status),
            json_string_literal(&eval_fixture),
            json_string_literal(remeasurement_status),
            before_native_accepted_count,
            after_native_accepted_count_rendered,
        ),
    ))
}

fn record_native_improvement_loop_refusal(
    kernel: &Arc<RwLock<MdxKernel>>,
    reason: &str,
    distillation_note_id: &str,
) -> Result<RouteResponse, String> {
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:forge_eval",
                run_id: "machine_league_native_improvement_loop_refusal",
                event: "evidence_appended",
                work_item_id: "machine_league_native_improvement_loop",
                detail: &format!("native_improvement_loop_refused reason={reason}"),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:forge_eval"),
            &[
                ("machine_league_native_improvement_loop_status", "refused"),
                (
                    "machine_league_native_improvement_loop_refusal_reason",
                    reason,
                ),
                (
                    "machine_league_native_improvement_distillation_note_id",
                    distillation_note_id,
                ),
                ("external_output_consumable", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-machine-league-native-improvement-loop-local-post","status":"REFUSED","route":"/forge/machine-league/native-improvement-loops.json","reason":{},"distillation_note_id":{},"refusal_receipt_id":{},"external_output_consumable":false,"production_write_allowed":false}}"#,
            json_string_literal(reason),
            json_string_literal(distillation_note_id),
            json_string_literal(&report.receipt_id),
        ),
    ))
}

fn render_native_improvement_loops_projection_json(kernel: &MdxKernel) -> String {
    let forge_run_event_receipts = kernel.ledger().query().by_kind("forge.run.event");
    let loop_receipts = forge_run_event_receipts
        .iter()
        .filter(|receipt| {
            payload_value(receipt, "machine_league_native_improvement_loop") == "true"
        })
        .collect::<Vec<_>>();
    let source_receipts = loop_receipts
        .iter()
        .map(|receipt| json_string_literal(&receipt.receipt_id))
        .collect::<Vec<_>>();
    let loops = loop_receipts
        .iter()
        .map(|receipt| native_improvement_loop_receipt_json(kernel, receipt))
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-forge-machine-league-native-improvement-loops-projection","status":"NATIVE_IMPROVEMENT_LOOPS_PROJECTION_READY","route":"/forge/machine-league/native-improvement-loops/projection.json","schedule_route":"/forge/machine-league/native-improvement-loops.json","learning_ledger_route":"/forge/machine-league/learning-ledger.json","builder_loop_route":"/forge/builder-loops/tick.json","remeasurement_route":"/forge/machine-league/native-trials.json","loop_count":{},"source_receipts":[{}],"external_output_consumable":false,"production_write_allowed":false,"loops":[{}]}}"#,
        loops.len(),
        source_receipts.join(","),
        loops.join(",")
    )
}

fn native_improvement_loop_receipt_json(kernel: &MdxKernel, receipt: &mdx_core::Receipt) -> String {
    // The after-count is derived live from accepted-score receipts stamped
    // after the builder loop tick, never from a value frozen at tick time.
    let after_native_accepted_count = accepted_score_count_for_runner_after_tick(
        kernel,
        payload_value(receipt, "baseline_runner_id"),
        Some(payload_value(receipt, "machine_league_eval_fixture")),
        payload_value(receipt, "builder_loop_tick_receipt_id"),
    );
    format!(
        r#"{{"native_improvement_loop_id":{},"native_improvement_loop_receipt_id":{},"distillation_note_id":{},"source_run_id":{},"winning_runner_id":{},"baseline_runner_id":{},"task_class":{},"language_pack_id":{},"eval_fixture":{},"distillation_reason":{},"native_improvement_work_item_id":{},"builder_loop_id":{},"builder_loop_tick_id":{},"builder_loop_tick_receipt_id":{},"builder_loop_status":{},"builder_loop_launch_execution_requested":{},"builder_loop_launch_status":{},"native_improvement_loop_status":{},"remeasurement_route":{},"remeasurement_status":{},"before_native_accepted_count":{},"after_native_accepted_count":{},"external_output_consumable":false,"production_write_allowed":false}}"#,
        json_string_literal(payload_value(
            receipt,
            "machine_league_native_improvement_loop_id"
        )),
        json_string_literal(&receipt.receipt_id),
        json_string_literal(payload_value(
            receipt,
            "machine_league_native_improvement_distillation_note_id"
        )),
        json_string_literal(payload_value(receipt, "source_run_id")),
        json_string_literal(payload_value(receipt, "winning_runner_id")),
        json_string_literal(payload_value(receipt, "baseline_runner_id")),
        json_string_literal(payload_value(receipt, "task_class")),
        json_string_literal(payload_value(receipt, "language_pack_id")),
        json_string_literal(payload_value(receipt, "machine_league_eval_fixture")),
        json_string_literal(payload_value(receipt, "distillation_reason")),
        json_string_literal(payload_value(receipt, "native_improvement_work_item_id")),
        json_string_literal(payload_value(receipt, "builder_loop_id")),
        json_string_literal(payload_value(receipt, "builder_loop_tick_id")),
        json_string_literal(payload_value(receipt, "builder_loop_tick_receipt_id")),
        json_string_literal(payload_value(receipt, "builder_loop_status")),
        payload_value(receipt, "builder_loop_launch_execution_requested") == "true",
        json_string_literal(payload_value(receipt, "builder_loop_launch_status")),
        json_string_literal(payload_value(receipt, "native_improvement_loop_status")),
        json_string_literal(payload_value(receipt, "remeasurement_route")),
        json_string_literal(payload_value(receipt, "remeasurement_status")),
        payload_value(receipt, "before_native_accepted_count")
            .parse::<u32>()
            .unwrap_or(0),
        after_native_accepted_count
            .map(|count| count.to_string())
            .unwrap_or_else(|| "null".to_string()),
    )
}

#[derive(Clone, Default)]
struct NativeRuntimeAdaptationProfile {
    count: u32,
    grant_receipt_ids: String,
    grants_json: String,
}

impl NativeRuntimeAdaptationProfile {
    fn from_kernel(kernel: &MdxKernel) -> Self {
        let grants = kernel.active_fleet_casting_grants();
        let grant_receipt_ids = grants
            .iter()
            .map(|grant| grant.grant_receipt_id.clone())
            .collect::<Vec<_>>()
            .join(",");
        let grants_json = grants
            .iter()
            .map(|grant| {
                format!(
                    r#"{{"adaptation_grant_receipt_id":{},"activation_receipt_id":{},"active_memory_id":{},"lesson_summary":{},"target_type":{},"preferred_builder_slot":{},"review_owner":{},"runtime_behavior_scope":"forge_native_builder_casting_and_prompt_context","model_routing_change_allowed":false,"budget_change_allowed":false,"check_policy_change_allowed":false,"review_depth_change_allowed":false}}"#,
                    json_string_literal(&grant.grant_receipt_id),
                    json_string_literal(&grant.activation_receipt_id),
                    json_string_literal(&grant.active_memory_id),
                    json_string_literal(&grant.lesson_summary),
                    json_string_literal(&grant.target_type),
                    json_string_literal(&grant.preferred_builder_slot),
                    json_string_literal(&grant.review_owner),
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        Self {
            count: grants.len() as u32,
            grant_receipt_ids,
            grants_json,
        }
    }

    fn status(&self) -> &'static str {
        if self.count == 0 {
            "NO_RATIFIED_NATIVE_RUNTIME_ADAPTATIONS"
        } else {
            "NATIVE_RUNTIME_ADAPTATION_ACTIVE"
        }
    }
}

fn render_native_runtime_adaptations_json(kernel: &MdxKernel) -> String {
    let profile = NativeRuntimeAdaptationProfile::from_kernel(kernel);
    format!(
        r#"{{"name":"mdx-forge-machine-league-native-runtime-adaptations","status":{},"route":"/forge/machine-league/native-runtime-adaptations.json","learning_adaptation_grants_route":"/learning/adaptation-grants.json","learning_adaptation_projection_route":"/learning/adaptation-grants/projection.json","learning_memory_activation_route":"/learning/memory-activations.json","learning_ledger_route":"/forge/machine-league/learning-ledger.json","native_improvement_loop_route":"/forge/machine-league/native-improvement-loops.json","adaptation_count":{},"adaptation_grant_receipt_ids":{},"runtime_behavior_change_allowed":{},"runtime_behavior_scope":"forge_native_builder_casting_and_prompt_context","normal_forge_consumers":["forge_run_route.builder_casting","fleet_plan_route.plan_casting","machine_league.fleet_casts","machine_league.recommendations"],"source_policy":"learning.memory.activated plus open learning.adaptation.granted receipt required","machine_league_policy":"distillation notes and Builder Loop ticks are not enough by themselves; normal Forge behavior changes only after the governed learning activation and adaptation grant chain","model_routing_change_allowed":false,"budget_change_allowed":false,"check_policy_change_allowed":false,"review_depth_change_allowed":false,"external_output_consumable":false,"production_write_allowed":false,"adaptations":[{}]}}"#,
        json_string_literal(profile.status()),
        profile.count,
        json_string_literal(&profile.grant_receipt_ids),
        profile.count > 0,
        profile.grants_json,
    )
}

fn record_native_runtime_adaptation_applied_receipts(
    kernel: &mut MdxKernel,
    profile: &NativeRuntimeAdaptationProfile,
    artifact_kind: &str,
    artifact_id: &str,
    detail: &str,
) {
    if profile.count == 0 {
        return;
    }
    let identity = GovernedWriteIdentity::local_demo("agent:machine_league_fleet_cast_planner");
    for grant_receipt_id in profile
        .grant_receipt_ids
        .split(',')
        .filter(|id| !id.trim().is_empty())
    {
        kernel.record_learning_adaptation_applied(
            mdx_core::LearningAdaptationApplied {
                tenant_id: "local_tenant",
                actor_id: "agent:machine_league_fleet_cast_planner",
                grant_receipt_id,
                artifact_kind,
                artifact_id,
                detail,
            },
            &identity,
        );
    }
}

fn find_distillation_note_for_improvement(
    kernel: &MdxKernel,
    note_id: Option<&str>,
) -> Option<DistillationNoteForImprovement> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
        .filter(|receipt| {
            let candidate = payload_value(receipt, "machine_league_distillation_note_id");
            !candidate.is_empty()
                && !payload_value(receipt, "distillation_evidence_gate").is_empty()
                && note_id
                    .map(|requested| requested.trim() == candidate)
                    .unwrap_or(true)
        })
        .map(|receipt| DistillationNoteForImprovement {
            note_id: payload_value(receipt, "machine_league_distillation_note_id").to_string(),
            source_run_id: payload_value(receipt, "source_run_id").to_string(),
            winning_runner_id: payload_value(receipt, "winning_runner_id").to_string(),
            baseline_runner_id: payload_value(receipt, "baseline_runner_id").to_string(),
            task_class: payload_value(receipt, "task_class").to_string(),
            language_pack_id: payload_value(receipt, "language_pack_id").to_string(),
            distillation_reason: payload_value(receipt, "distillation_reason").to_string(),
            native_improvement_work_item_id: payload_value(
                receipt,
                "native_improvement_work_item_id",
            )
            .to_string(),
            confidence: payload_value(receipt, "confidence").to_string(),
            note_receipt_id: receipt.receipt_id.clone(),
        })
        .next()
}

fn eval_fixture_for_source_run(kernel: &MdxKernel, source_run_id: &str) -> Option<String> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
        .find_map(|receipt| {
            if payload_value(receipt, "run_id") != source_run_id.trim() {
                return None;
            }
            let fixture = payload_value(receipt, "machine_league_eval_fixture");
            if fixture.trim().is_empty() {
                None
            } else {
                Some(fixture.to_string())
            }
        })
}

fn latest_native_improvement_loop_for_note<'a>(
    kernel: &'a MdxKernel,
    note_id: &str,
) -> Option<&'a mdx_core::Receipt> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
        .find(|receipt| {
            payload_value(receipt, "machine_league_native_improvement_loop") == "true"
                && payload_value(
                    receipt,
                    "machine_league_native_improvement_distillation_note_id",
                ) == note_id
        })
        .copied()
}

fn render_learning_ledger_json(kernel: &MdxKernel) -> String {
    let distilled_run_ids = kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter_map(|receipt| {
            let note_id = payload_value(receipt, "machine_league_distillation_note_id");
            let source_run_id = payload_value(receipt, "source_run_id");
            if note_id.is_empty()
                || source_run_id.is_empty()
                || payload_value(receipt, "distillation_evidence_gate").is_empty()
            {
                None
            } else {
                Some(source_run_id.to_string())
            }
        })
        .collect::<std::collections::BTreeSet<_>>();
    let notes = kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| {
            !payload_value(receipt, "machine_league_distillation_note_id").is_empty()
                && !payload_value(receipt, "distillation_evidence_gate").is_empty()
        })
        .map(|receipt| {
            let note_id = payload_value(receipt, "machine_league_distillation_note_id");
            let winning_runner_id = payload_value(receipt, "winning_runner_id");
            let baseline_runner_id = payload_value(receipt, "baseline_runner_id");
            let native_loop = latest_native_improvement_loop_for_note(kernel, note_id);
            let native_improvement_loop_status = native_loop
                .map(|loop_receipt| payload_value(loop_receipt, "native_improvement_loop_status"))
                .filter(|status| !status.trim().is_empty())
                .unwrap_or("not_started");
            let native_improvement_loop_receipt_id = native_loop
                .map(|loop_receipt| loop_receipt.receipt_id.as_str())
                .unwrap_or("");
            let builder_loop_tick_id = native_loop
                .map(|loop_receipt| payload_value(loop_receipt, "builder_loop_tick_id"))
                .unwrap_or("");
            let builder_loop_tick_receipt_id = native_loop
                .map(|loop_receipt| payload_value(loop_receipt, "builder_loop_tick_receipt_id"))
                .unwrap_or("");
            let before_count = accepted_score_count_for_runner(kernel, baseline_runner_id);
            let after_count = accepted_score_count_for_runner_after_tick(
                kernel,
                baseline_runner_id,
                None,
                builder_loop_tick_receipt_id,
            );
            let external_count = accepted_score_count_for_runner(kernel, winning_runner_id);
            let delta_status =
                if before_count == 0 || after_count.unwrap_or(0) == 0 || external_count == 0 {
                    "insufficient_accepted_sample"
                } else {
                    "accepted_sample_available"
                };
            let remeasurement_status = native_loop
                .map(|loop_receipt| payload_value(loop_receipt, "remeasurement_status"))
                .filter(|status| !status.trim().is_empty())
                .unwrap_or("waiting_for_native_improvement_loop");
            format!(
                r#"{{"distillation_note_id":{},"source_run_id":{},"winning_runner_id":{},"baseline_runner_id":{},"task_class":{},"language_pack_id":{},"distillation_reason":{},"native_improvement_work_item_id":{},"native_improvement_loop_route":"/forge/machine-league/native-improvement-loops.json","native_improvement_loop_status":{},"native_improvement_loop_receipt_id":{},"builder_loop_tick_id":{},"builder_loop_tick_receipt_id":{},"remeasurement_route":"/forge/machine-league/native-trials.json","remeasurement_status":{},"delta_status":{},"before_native_accepted_count":{},"after_native_accepted_count":{},"external_accepted_count":{},"before_win_rate_pct":null,"after_win_rate_pct":null,"evidence_receipt_id":{},"external_output_consumable":false}}"#,
                json_string_literal(note_id),
                json_string_literal(payload_value(receipt, "source_run_id")),
                json_string_literal(winning_runner_id),
                json_string_literal(baseline_runner_id),
                json_string_literal(payload_value(receipt, "task_class")),
                json_string_literal(payload_value(receipt, "language_pack_id")),
                json_string_literal(payload_value(receipt, "distillation_reason")),
                json_string_literal(payload_value(receipt, "native_improvement_work_item_id")),
                json_string_literal(native_improvement_loop_status),
                json_string_literal(native_improvement_loop_receipt_id),
                json_string_literal(builder_loop_tick_id),
                json_string_literal(builder_loop_tick_receipt_id),
                json_string_literal(remeasurement_status),
                json_string_literal(delta_status),
                before_count,
                after_count
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "null".to_string()),
                external_count,
                json_string_literal(&receipt.receipt_id),
            )
        })
        .collect::<Vec<_>>();
    let pending_candidates = kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| payload_value(receipt, "accepted_for_scoreboard") == "true")
        .filter(|receipt| {
            let winning_runner_id = payload_value(receipt, "winning_runner_id");
            let runner_id = payload_value(receipt, "runner_id");
            let candidate_runner_id = if winning_runner_id.is_empty() {
                runner_id
            } else {
                winning_runner_id
            };
            !candidate_runner_id.is_empty() && candidate_runner_id != "mdx_native_harness_runner"
        })
        .filter(|receipt| !distilled_run_ids.contains(payload_value(receipt, "run_id")))
        .map(|receipt| {
            let winning_runner_id = payload_value(receipt, "winning_runner_id");
            let winning_runner_id = if winning_runner_id.is_empty() {
                payload_value(receipt, "runner_id")
            } else {
                winning_runner_id
            };
            let baseline_runner_id = payload_value(receipt, "baseline_runner_id");
            let baseline_runner_id = if baseline_runner_id.is_empty() {
                "mdx_native_harness_runner"
            } else {
                baseline_runner_id
            };
            let eval_fixture = payload_value(receipt, "machine_league_eval_fixture");
            let native_same_fixture_accepted_count = if eval_fixture.is_empty() {
                0
            } else {
                accepted_score_count_for_runner_and_fixture(
                    kernel,
                    "mdx_native_harness_runner",
                    eval_fixture,
                )
            };
            let native_same_fixture_failed_attempt_count = if eval_fixture.is_empty() {
                0
            } else {
                failed_native_attempt_count_for_fixture(kernel, eval_fixture)
            };
            let (status, next_action) = if native_same_fixture_accepted_count > 0 {
                (
                    "native_comparison_available_pending_distillation_decision",
                    "compare accepted native and external diffs; author a distillation note only if the external runner shows a reusable edge",
                )
            } else if native_same_fixture_failed_attempt_count > 0 {
                (
                    "external_edge_pending_distillation_note",
                    "native attempted the same fixture and failed the hidden oracle; author a distillation note only if the external diff shows a reusable harness edge",
                )
            } else {
                (
                    "pending_native_comparison_and_distillation_note",
                    "run native on the same fixture, then author an MDx distillation note",
                )
            };
            format!(
                r#"{{"source_run_id":{},"winning_runner_id":{},"baseline_runner_id":{},"eval_fixture":{},"task_class":{},"language_pack_id":{},"scorecard_total_score":{},"native_same_fixture_accepted_count":{},"native_same_fixture_failed_attempt_count":{},"principal_review_receipt_id":{},"status":{},"next_action":{},"external_output_consumable":false}}"#,
                json_string_literal(payload_value(receipt, "run_id")),
                json_string_literal(winning_runner_id),
                json_string_literal(baseline_runner_id),
                json_string_literal(eval_fixture),
                json_string_literal(payload_value(receipt, "language_task_class")),
                json_string_literal(payload_value(receipt, "language_pack_id")),
                json_string_literal(payload_value(receipt, "total_score")),
                native_same_fixture_accepted_count,
                native_same_fixture_failed_attempt_count,
                json_string_literal(&receipt.receipt_id),
                json_string_literal(status),
                json_string_literal(next_action),
            )
        })
        .collect::<Vec<_>>();
    let learning_source_event_receipts = kernel.ledger().query().by_kind("forge.run.event");
    let source_receipts = learning_source_event_receipts
        .iter()
        .filter(|receipt| {
            let note_id = payload_value(receipt, "machine_league_distillation_note_id");
            if !note_id.is_empty()
                && !payload_value(receipt, "distillation_evidence_gate").is_empty()
            {
                return true;
            }
            if payload_value(receipt, "accepted_for_scoreboard") != "true" {
                return false;
            }
            let winning_runner_id = payload_value(receipt, "winning_runner_id");
            let runner_id = payload_value(receipt, "runner_id");
            let candidate_runner_id = if winning_runner_id.is_empty() {
                runner_id
            } else {
                winning_runner_id
            };
            !candidate_runner_id.is_empty() && candidate_runner_id != "mdx_native_harness_runner"
        })
        .map(|receipt| json_string_literal(&receipt.receipt_id))
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-forge-machine-league-learning-ledger","status":"MACHINE_LEAGUE_LEARNING_LEDGER_READY","route":"/forge/machine-league/learning-ledger.json","distillation_note_route":"/forge/machine-league/distillation-notes.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","entry_count":{},"pending_distillation_candidate_count":{},"source_receipts":[{}],"delta_source_policy":"accepted_scoreboard_results_only","accepted_scoreboard_results_only":true,"dry_run_claims_allowed":false,"quarantined_claims_allowed":false,"runner_self_attestation_allowed":false,"entries":[{}],"pending_distillation_candidates":[{}],"production_write_allowed":false}}"#,
        notes.len(),
        pending_candidates.len(),
        source_receipts.join(","),
        notes.join(","),
        pending_candidates.join(",")
    )
}

fn render_provider_preflight_json(secret_store: &SecretStore) -> String {
    let tenant_id = crate::request_security::current_verified_identity()
        .map(|identity| identity.tenant_id)
        .unwrap_or_else(|| "local_tenant".to_string());
    let requirements = provider_credential_requirements(secret_store, &tenant_id);
    let ready_provider_count = requirements
        .iter()
        .filter(|requirement| requirement.credentials_present)
        .count();
    let all_credentials_present = ready_provider_count == requirements.len();
    format!(
        r#"{{"name":"mdx-forge-fleet-eval-provider-preflight","status":"{}","route":"/forge/fleet-eval-provider-preflight.json","approval_route":"/forge/fleet-eval-live-run-approvals.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","tenant_id":{},"provider_count":{},"ready_provider_count":{},"all_credentials_present":{},"provider_secret_values_exposed":false,"provider_secret_values_recorded":false,"provider_secret_export_allowed":false,"live_eval_runs_allowed":false,"approval_required":true,"requirements":[{}]}}"#,
        if all_credentials_present {
            "READY_FOR_LIVE_RUN_APPROVAL"
        } else {
            "MISSING_PROVIDER_CREDENTIALS"
        },
        json_string_literal(&tenant_id),
        requirements.len(),
        ready_provider_count,
        all_credentials_present,
        requirements
            .iter()
            .map(ProviderCredentialRequirement::to_json)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn render_live_run_approval_json(
    body: &str,
    kernel: &mut MdxKernel,
    secret_store: &SecretStore,
) -> Result<String, String> {
    let approval_id = json_string_field(body, "approval_id")
        .unwrap_or_else(|| "fleet_eval_live_run_approval_local_001".to_string());
    let provider_allowlist = json_string_field(body, "provider_allowlist")
        .unwrap_or_else(|| "gemini,anthropic,xai,moonshot,aws_bedrock".to_string());
    let max_spend_cents = json_u32_field(body, "max_spend_cents").unwrap_or(500);
    let max_tasks = json_u32_field(body, "max_tasks").unwrap_or(15);
    let max_parallel_agents = json_u32_field(body, "max_parallel_agents").unwrap_or(4);
    let artifact_retention_policy = json_string_field(body, "artifact_retention_policy")
        .unwrap_or_else(|| "quarantine_7_days".to_string());
    let redaction_policy = json_string_field(body, "redaction_policy")
        .unwrap_or_else(|| "mdx_context_redaction_required".to_string());
    let stop_conditions = json_string_field(body, "stop_conditions").unwrap_or_else(|| {
        "budget_exhausted,provider_error_rate,quality_gate_failure,manual_stop".to_string()
    });
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "agent:codex",
        "operator",
    );
    let report = kernel
        .approve_forge_fleet_eval_live_run_local_with_identity(
            ForgeFleetEvalLiveRunApproval {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                actor_role: &resolved.actor_role,
                approval_id: &approval_id,
                provider_allowlist: &provider_allowlist,
                max_spend_cents,
                max_tasks,
                max_parallel_agents,
                artifact_retention_policy: &artifact_retention_policy,
                redaction_policy: &redaction_policy,
                stop_conditions: &stop_conditions,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    let requirements = provider_credential_requirements(secret_store, &resolved.tenant_id);
    let ready_provider_count = requirements
        .iter()
        .filter(|requirement| requirement.credentials_present)
        .count();
    let all_credentials_present = ready_provider_count == requirements.len();
    Ok(format!(
        r#"{{"name":"mdx-forge-fleet-eval-live-run-approval-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/forge/fleet-eval-live-run-approvals.json","projection_route":"/forge/fleet-eval-live-run-approvals/projection.json","provider_preflight_route":"/forge/fleet-eval-provider-preflight.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","approval_id":{},"approval_receipt_id":{},"policy_decision_id":{},"provider_allowlist":{},"max_spend_cents":{},"max_tasks":{},"max_parallel_agents":{},"artifact_retention_policy":{},"redaction_policy":{},"stop_conditions":{},"provider_count":{},"ready_provider_count":{},"all_credentials_present":{},"ready_for_live_turn_on":{},"provider_credentials_required":{},"live_provider_calls_allowed":{},"approval_grants_execution_authority":{},"adapter_execution_allowed":{},"production_write_allowed":{},"blocked_reason":{},"provider_secret_values_exposed":false,"requirements":[{}]}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.approval_id),
        json_string_literal(&report.approval_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.provider_allowlist),
        report.max_spend_cents,
        report.max_tasks,
        report.max_parallel_agents,
        json_string_literal(&report.artifact_retention_policy),
        json_string_literal(&report.redaction_policy),
        json_string_literal(&report.stop_conditions),
        requirements.len(),
        ready_provider_count,
        all_credentials_present,
        all_credentials_present,
        report.provider_credentials_required,
        report.live_provider_calls_allowed,
        report.approval_grants_execution_authority,
        report.adapter_execution_allowed,
        report.production_write_allowed,
        json_string_literal(report.blocked_reason),
        requirements
            .iter()
            .map(ProviderCredentialRequirement::to_json)
            .collect::<Vec<_>>()
            .join(",")
    ))
}

fn render_dry_run_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let dry_run_id = json_string_field(body, "dry_run_id")
        .unwrap_or_else(|| "fleet_eval_dry_run_local_001".to_string());
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "agent:codex",
        "worker",
    );
    let report = kernel
        .run_forge_fleet_eval_dry_run_local_with_identity(&dry_run_id, &resolved.identity)
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-fleet-eval-dry-run-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/forge/fleet-eval-dry-runs.json","projection_route":"/forge/fleet-eval-dry-runs/projection.json","result_ingestion_route":"/forge/fleet-eval-results.json","result_projection_route":"/forge/fleet-eval-results/projection.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","dry_run_id":{},"runner_id":{},"benchmark_task_count":{},"language_task_count":{},"model_profile_count":{},"scoring_dimension_count":{},"dry_run_case_count":{},"runner_execution_receipt_count":{},"result_receipt_count":{},"score_receipt_count":{},"live_provider_calls_allowed":{},"live_provider_calls_performed":{},"provider_credentials_required_for_live":{},"ready_for_live_credentials":{},"accepted_for_scoreboard_count":{},"quarantined_count":{},"blocked_reason":{},"live_substrate_required":false,"production_write_allowed":false}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.dry_run_id),
        json_string_literal(&report.runner_id),
        report.benchmark_task_count,
        report.language_task_count,
        report.model_profile_count,
        report.scoring_dimension_count,
        report.dry_run_case_count,
        report.runner_execution_receipt_ids.len(),
        report.result_receipt_ids.len(),
        report.score_receipt_ids.len(),
        report.live_provider_calls_allowed,
        report.live_provider_calls_performed,
        report.provider_credentials_required_for_live,
        report.ready_for_live_credentials,
        report.accepted_for_scoreboard_count,
        report.quarantined_count,
        json_string_literal(report.blocked_reason),
    ))
}

pub(crate) fn render_forge_fleet_eval_scoreboard_json() -> Result<String, String> {
    render_forge_fleet_eval_scoreboard_json_with_kernel(None)
}

fn render_forge_fleet_eval_scoreboard_json_with_kernel(
    kernel: Option<&MdxKernel>,
) -> Result<String, String> {
    let report = run_forge_fleet_eval();
    // The winner is earned, not declared: highest accepted-trial rate among
    // runners that cleared the minimum trial floor. No eligible runner means
    // no winner - an empty slot is honest; a fabricated champion is not.
    let winning_runner_id = kernel
        .map(|kernel| {
            forge_fleet_runner_profiles()
                .iter()
                .filter_map(|runner| {
                    let accepted = accepted_score_count_for_runner(kernel, runner.runner_id);
                    let trials =
                        league_trial_count_for_runner(kernel, runner.runner_id).max(accepted);
                    if trials < scoreboard_min_trials() {
                        return None;
                    }
                    let rate = accepted
                        .saturating_mul(100)
                        .checked_div(trials)
                        .unwrap_or(0);
                    Some((rate, accepted, runner.runner_id.to_string()))
                })
                .max_by_key(|(rate, accepted, _)| (*rate, *accepted))
                .map(|(_, _, runner_id)| runner_id)
                .unwrap_or_default()
        })
        .unwrap_or_default();
    Ok(format!(
        r#"{{
  "name": "mdx-forge-fleet-eval-scoreboard",
  "status": {},
  "route": "/forge/fleet-eval-scoreboard.json",
  "response_schema_path": "generated/response-schemas/forge-fleet-eval-scoreboard-response.schema.json",
  "read_only": true,
  "writes_allowed": false,
  "execution_allowed": false,
  "adapter_execution_allowed": false,
  "data_decides": {},
  "benchmark_task_count": {},
  "language_task_corpus_count": {},
  "language_pack_scorecard_count": {},
  "runner_count": {},
  "winning_runner_id": {},
  "winner_rule": "highest accepted-trial rate among runners with at least the minimum receipt-backed trials; empty means no runner qualified yet",
  "codex_adapter_status": {},
  "codex_model_freedom_rule": {},
  "evidence_policy": {},
  "language_task_corpus": [{}],
  "language_pack_scorecards": [{}],
  "scoring_dimensions": [{}],
  "model_matrix": [{}],
  "adapter_protocol_tiers": [{}],
  "runner_model_compatibility_matrix": [{}],
  "benchmark_tasks": [{}],
  "task_class_scorecards": [{}],
  "runner_profiles": [{}],
  "runner_scores": [{}],
  "parallel_fleet_load_proof": {{
    "target_parallel_agents": {},
    "total_jobs": {},
    "admitted": {},
    "queued": {},
    "shed": {},
    "cancelled": {},
    "retried": {},
    "completed": {},
    "peak_running": {},
    "peak_queued": {},
    "peak_tenant_running": {},
    "invariants_held": {},
    "over_admission": {},
    "fairness_violation": {},
    "queue_overflow": {},
    "shed_observed": {},
    "cancellations_honored": {},
    "retries_honored": {}
  }},
  "authority_boundary": {{
    "status": "FAIL_CLOSED",
    "codex_cli_live_execution_allowed": false,
    "external_output_consumable": false,
    "provider_secret_export_allowed": false,
    "production_write_allowed": false,
    "human_ratification_bypass_allowed": false
  }},
  "source_paths": ["docs/FORGE-FLEET-EVAL-HARNESS.md", "generated/architecture/forge-fleet-eval-harness.json", "generated/architecture/forge-language-task-corpus.json", "generated/response-schemas/forge-fleet-eval-scoreboard-response.schema.json", "crates/mdx-core/src/harness_fleet_eval.rs", "crates/mdx-server/src/forge_fleet_eval_scoreboard.rs"]
}}
"#,
        json_string_literal(&report.status),
        report.data_decides,
        report.benchmark_tasks.len(),
        report.language_task_corpus.len(),
        report.language_pack_scorecards.len(),
        report.runner_scores.len(),
        json_string_literal(&winning_runner_id),
        json_string_literal(&report.codex_adapter_status),
        json_string_literal(&report.codex_model_freedom_rule),
        json_string_literal(&report.evidence_policy),
        language_task_corpus_json(&report),
        language_pack_scorecards_json(&report),
        scoring_dimensions_json(&report),
        model_matrix_json(&report),
        adapter_protocol_tiers_json(),
        runner_model_compatibility_matrix_json(kernel),
        benchmark_tasks_json(&report),
        task_class_scorecards_json(&report, kernel),
        forge_fleet_runner_profiles()
            .iter()
            .map(runner_profile_json)
            .collect::<Vec<_>>()
            .join(","),
        runner_scores_json(&report, kernel),
        report.parallel_agents_target,
        report.parallel_load.total_jobs,
        report.parallel_load.admitted,
        report.parallel_load.queued,
        report.parallel_load.shed,
        report.parallel_load.cancelled,
        report.parallel_load.retried,
        report.parallel_load.completed,
        report.parallel_load.peak_running,
        report.parallel_load.peak_queued,
        report.parallel_load.peak_tenant_running,
        report.parallel_load.invariants_held,
        report.parallel_load.over_admission,
        report.parallel_load.fairness_violation,
        report.parallel_load.queue_overflow,
        report.parallel_load.shed_observed,
        report.parallel_load.cancellations_honored,
        report.parallel_load.retries_honored
    ))
}

fn render_live_run_approval_projection_json(kernel: &MdxKernel) -> String {
    let approvals = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "forge.fleet_eval_live_run.approved")
        .map(|receipt| {
            format!(
                r#"{{"approval_id":{},"approval_receipt_id":{},"policy_decision_id":{},"provider_allowlist":{},"max_spend_cents":{},"max_tasks":{},"max_parallel_agents":{},"artifact_retention_policy":{},"redaction_policy":{},"stop_conditions":{},"provider_credentials_required":{},"live_provider_calls_allowed":{},"approval_grants_execution_authority":{},"adapter_execution_allowed":{},"production_write_allowed":{},"blocked_reason":{}}}"#,
                json_string_literal(payload_value(receipt, "approval_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "provider_allowlist")),
                payload_value(receipt, "max_spend_cents"),
                payload_value(receipt, "max_tasks"),
                payload_value(receipt, "max_parallel_agents"),
                json_string_literal(payload_value(receipt, "artifact_retention_policy")),
                json_string_literal(payload_value(receipt, "redaction_policy")),
                json_string_literal(payload_value(receipt, "stop_conditions")),
                payload_value(receipt, "provider_credentials_required"),
                payload_value(receipt, "live_provider_calls_allowed"),
                payload_value(receipt, "approval_grants_execution_authority"),
                payload_value(receipt, "adapter_execution_allowed"),
                payload_value(receipt, "production_write_allowed"),
                json_string_literal(payload_value(receipt, "blocked_reason")),
            )
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-forge-fleet-eval-live-run-approval-local-projection","status":"OK","route":"/forge/fleet-eval-live-run-approvals/projection.json","writes_route":"/forge/fleet-eval-live-run-approvals.json","provider_preflight_route":"/forge/fleet-eval-provider-preflight.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","approval_count":{},"live_provider_calls_allowed":false,"approval_grants_execution_authority":false,"adapter_execution_allowed":false,"production_write_allowed":false,"approvals":[{}]}}"#,
        approvals.len(),
        approvals.join(",")
    )
}

fn render_dry_run_projection_json(kernel: &MdxKernel) -> String {
    let mut language_tasks = std::collections::BTreeSet::<String>::new();
    let mut language_packs = std::collections::BTreeSet::<String>::new();
    let mut model_profiles = std::collections::BTreeSet::<String>::new();
    let mut provider_families = std::collections::BTreeSet::<String>::new();
    let scores = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "forge.fleet_eval_result.scored")
        .map(|receipt| {
            language_tasks.insert(payload_value(receipt, "language_task_corpus_id").to_string());
            language_packs.insert(payload_value(receipt, "language_pack_id").to_string());
            model_profiles.insert(payload_value(receipt, "model_profile_id").to_string());
            provider_families.insert(payload_value(receipt, "provider_family").to_string());
            format!(
                r#"{{"dry_run_id":{},"submission_id":{},"benchmark_task_id":{},"language_task_corpus_id":{},"language_pack_id":{},"repo_family":{},"language_engineering_facets":{},"language_evaluation_oracle":{},"language_human_timebox_minutes":{},"language_contamination_policy":{},"hidden_check_slot":{},"artifact_noise_expected":{},"diff_language_pack_impact":{},"principal_engineer_verdict":{},"runner_id":{},"model_profile_id":{},"provider_family":{},"model_provider":{},"model_id":{},"score_receipt_id":{},"result_receipt_id":{},"correctness_score":{},"regression_test_quality_score":{},"patch_quality_score":{},"architecture_fit_score":{},"test_evidence_score":{},"security_and_policy_score":{},"maintainability_score":{},"observability_score":{},"performance_score":{},"migration_and_compatibility_score":{},"policy_score":{},"cost_score":{},"latency_score":{},"cost_latency_budget_score":{},"handoff_quality_score":{},"total_score":{},"live_provider_output_present":{},"mdx_quality_gates_passed":{},"accepted_for_scoreboard":{},"external_output_consumable":{},"blocked_reason":{}}}"#,
                json_string_literal(payload_value(receipt, "dry_run_id")),
                json_string_literal(payload_value(receipt, "submission_id")),
                json_string_literal(payload_value(receipt, "benchmark_task_id")),
                json_string_literal(payload_value(receipt, "language_task_corpus_id")),
                json_string_literal(payload_value(receipt, "language_pack_id")),
                json_string_literal(payload_value(receipt, "repo_family")),
                json_string_literal(payload_value(receipt, "language_engineering_facets")),
                json_string_literal(payload_value(receipt, "language_evaluation_oracle")),
                payload_value(receipt, "language_human_timebox_minutes"),
                json_string_literal(payload_value(receipt, "language_contamination_policy")),
                json_string_literal(payload_value(receipt, "hidden_check_slot")),
                json_string_literal(payload_value(receipt, "artifact_noise_expected")),
                json_string_literal(payload_value(receipt, "diff_language_pack_impact")),
                json_string_literal(payload_value(receipt, "principal_engineer_verdict")),
                json_string_literal(payload_value(receipt, "runner_id")),
                json_string_literal(payload_value(receipt, "model_profile_id")),
                json_string_literal(payload_value(receipt, "provider_family")),
                json_string_literal(payload_value(receipt, "model_provider")),
                json_string_literal(payload_value(receipt, "model_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(payload_value(receipt, "result_receipt_id")),
                payload_value(receipt, "correctness_score"),
                payload_value(receipt, "regression_test_quality_score"),
                payload_value(receipt, "patch_quality_score"),
                payload_value(receipt, "architecture_fit_score"),
                payload_value(receipt, "test_evidence_score"),
                payload_value(receipt, "security_and_policy_score"),
                payload_value(receipt, "maintainability_score"),
                payload_value(receipt, "observability_score"),
                payload_value(receipt, "performance_score"),
                payload_value(receipt, "migration_and_compatibility_score"),
                payload_value(receipt, "policy_score"),
                payload_value(receipt, "cost_score"),
                payload_value(receipt, "latency_score"),
                payload_value(receipt, "cost_latency_budget_score"),
                payload_value(receipt, "handoff_quality_score"),
                payload_value(receipt, "total_score"),
                payload_value(receipt, "live_provider_output_present"),
                payload_value(receipt, "mdx_quality_gates_passed"),
                payload_value(receipt, "accepted_for_scoreboard"),
                payload_value(receipt, "external_output_consumable"),
                json_string_literal(payload_value(receipt, "blocked_reason")),
            )
        })
        .collect::<Vec<_>>();
    let expected_language_task_count = mdx_core::forge_language_task_corpus().len();
    let expected_model_profile_count = mdx_core::forge_fleet_model_matrix_profiles().len();
    let expected_score_count = expected_language_task_count * expected_model_profile_count;
    let full_matrix_staged = scores.len() == expected_score_count
        && language_tasks.len() == expected_language_task_count
        && model_profiles.len() == expected_model_profile_count;
    format!(
        r#"{{"name":"mdx-forge-fleet-eval-dry-run-local-projection","status":"OK","route":"/forge/fleet-eval-dry-runs/projection.json","writes_route":"/forge/fleet-eval-dry-runs.json","result_ingestion_route":"/forge/fleet-eval-results.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","score_count":{},"coverage":{},"live_provider_calls_allowed":false,"live_provider_calls_performed":false,"provider_credentials_required_for_live":true,"ready_for_live_credentials":true,"accepted_for_scoreboard_count":0,"blocked_reason":"dry_run_waiting_for_live_provider_keys","scores":[{}]}}"#,
        scores.len(),
        dry_run_coverage_json(
            expected_language_task_count,
            expected_model_profile_count,
            expected_score_count,
            scores.len(),
            full_matrix_staged,
            &language_tasks,
            &language_packs,
            &model_profiles,
            &provider_families,
        ),
        scores.join(",")
    )
}

#[allow(clippy::too_many_arguments)]
fn dry_run_coverage_json(
    expected_language_task_count: usize,
    expected_model_profile_count: usize,
    expected_score_count: usize,
    score_count: usize,
    full_matrix_staged: bool,
    language_tasks: &std::collections::BTreeSet<String>,
    language_packs: &std::collections::BTreeSet<String>,
    model_profiles: &std::collections::BTreeSet<String>,
    provider_families: &std::collections::BTreeSet<String>,
) -> String {
    format!(
        r#"{{"expected_language_task_count":{},"language_task_count":{},"expected_model_profile_count":{},"model_profile_count":{},"expected_score_count":{},"score_count":{},"language_pack_count":{},"provider_family_count":{},"full_matrix_staged":{},"language_packs":[{}],"provider_families":[{}],"production_write_allowed":false}}"#,
        expected_language_task_count,
        language_tasks.len(),
        expected_model_profile_count,
        model_profiles.len(),
        expected_score_count,
        score_count,
        language_packs.len(),
        provider_families.len(),
        full_matrix_staged,
        json_string_set(language_packs),
        json_string_set(provider_families),
    )
}

fn json_string_set(values: &std::collections::BTreeSet<String>) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",")
}

fn render_result_ingestion_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let submission_id = json_string_field(body, "submission_id")
        .unwrap_or_else(|| "fleet_eval_result_local_001".to_string());
    let benchmark_task_id =
        json_string_field(body, "benchmark_task_id").unwrap_or_else(|| "bugfix_small".to_string());
    let runner_id = json_string_field(body, "runner_id")
        .unwrap_or_else(|| "codex_cli_external_worker".to_string());
    let model_profile_id = json_string_field(body, "model_profile_id")
        .unwrap_or_else(|| "codex_anthropic_responses_profile".to_string());
    let output_artifact_ref = json_string_field(body, "output_artifact_ref")
        .unwrap_or_else(|| "quarantine://forge/fleet-eval/local.patch".to_string());
    let output_artifact_sha256 =
        json_string_field(body, "output_artifact_sha256").unwrap_or_else(|| {
            "sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string()
        });
    let transcript_ref = json_string_field(body, "transcript_ref")
        .unwrap_or_else(|| "quarantine://forge/fleet-eval/local.jsonl".to_string());
    let claimed_check =
        json_string_field(body, "claimed_check").unwrap_or_else(|| "cargo test".to_string());
    let language_task_corpus_id = json_string_field(body, "language_task_corpus_id")
        .unwrap_or_else(|| "rust-cargo-bug-fix-small".to_string());
    let language_pack_id =
        json_string_field(body, "language_pack_id").unwrap_or_else(|| "rust-cargo".to_string());
    let repo_family =
        json_string_field(body, "repo_family").unwrap_or_else(|| "Rust Cargo".to_string());
    let hidden_check_slot = json_string_field(body, "hidden_check_slot")
        .unwrap_or_else(|| "unit regression".to_string());
    let artifact_noise_expected = json_string_field(body, "artifact_noise_expected")
        .unwrap_or_else(|| "target/**".to_string());
    let principal_review_gate_results = json_string_field(body, "principal_review_gate_results")
        .unwrap_or_else(|| {
            pending_principal_review_gate_results(DEFAULT_PRINCIPAL_REVIEW_GATE_IDS)
        });
    let standards_source_fingerprints = json_string_field(body, "standards_source_fingerprints")
        .unwrap_or_else(|| "AGENTS.md=fnv1a64:0000000000000000".to_string());
    let artifact_filter_summary = json_string_field(body, "artifact_filter_summary")
        .unwrap_or_else(|| "pending_artifact_filter_summary".to_string());
    let diff_language_pack_impact = json_string_field(body, "diff_language_pack_impact")
        .unwrap_or_else(|| language_pack_id.clone());
    let principal_engineer_verdict = json_string_field(body, "principal_engineer_verdict")
        .unwrap_or_else(|| "pending_principal_engineer_review".to_string());
    let pr_handoff_ref = json_string_field(body, "pr_handoff_ref")
        .unwrap_or_else(|| "quarantine://forge/fleet-eval/local-pr-handoff.md".to_string());
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "agent:codex",
        "worker",
    );
    let report = kernel
        .ingest_forge_fleet_eval_result_local_with_identity(
            ForgeFleetEvalResultSubmission {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                actor_role: &resolved.actor_role,
                submission_id: &submission_id,
                benchmark_task_id: &benchmark_task_id,
                language_task_corpus_id: &language_task_corpus_id,
                language_pack_id: &language_pack_id,
                repo_family: &repo_family,
                runner_id: &runner_id,
                model_profile_id: &model_profile_id,
                output_artifact_ref: &output_artifact_ref,
                output_artifact_sha256: &output_artifact_sha256,
                transcript_ref: &transcript_ref,
                claimed_check: &claimed_check,
                hidden_check_slot: &hidden_check_slot,
                artifact_noise_expected: &artifact_noise_expected,
                principal_review_gate_results: &principal_review_gate_results,
                standards_source_fingerprints: &standards_source_fingerprints,
                artifact_filter_summary: &artifact_filter_summary,
                diff_language_pack_impact: &diff_language_pack_impact,
                principal_engineer_verdict: &principal_engineer_verdict,
                pr_handoff_ref: &pr_handoff_ref,
            },
            &resolved.identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-forge-fleet-eval-result-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/forge/fleet-eval-results.json","projection_route":"/forge/fleet-eval-results/projection.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","submission_id":{},"benchmark_task_id":{},"language_task_corpus_id":{},"language_pack_id":{},"repo_family":{},"language_engineering_facets":{},"language_evaluation_oracle":{},"language_human_timebox_minutes":{},"language_contamination_policy":{},"hidden_check_slot":{},"artifact_noise_expected":{},"principal_review_gate_results":{},"standards_source_fingerprints":{},"artifact_filter_summary":{},"diff_language_pack_impact":{},"principal_engineer_verdict":{},"pr_handoff_ref":{},"runner_id":{},"model_profile_id":{},"result_receipt_id":{},"policy_decision_id":{},"output_quarantined":{},"external_output_consumable":{},"mdx_quality_gates_required":{},"mdx_quality_gates_passed":{},"accepted_for_scoreboard":{},"blocked_reason":{},"live_substrate_required":false,"production_write_allowed":false}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.submission_id),
        json_string_literal(&report.benchmark_task_id),
        json_string_literal(&report.language_task_corpus_id),
        json_string_literal(&report.language_pack_id),
        json_string_literal(&report.repo_family),
        json_string_literal(&report.language_engineering_facets),
        json_string_literal(&report.language_evaluation_oracle),
        report.language_human_timebox_minutes,
        json_string_literal(&report.language_contamination_policy),
        json_string_literal(&hidden_check_slot),
        json_string_literal(&artifact_noise_expected),
        json_string_literal(&principal_review_gate_results),
        json_string_literal(&standards_source_fingerprints),
        json_string_literal(&artifact_filter_summary),
        json_string_literal(&diff_language_pack_impact),
        json_string_literal(&principal_engineer_verdict),
        json_string_literal(&pr_handoff_ref),
        json_string_literal(&report.runner_id),
        json_string_literal(&report.model_profile_id),
        json_string_literal(&report.result_receipt_id),
        json_string_literal(&report.policy_decision_id),
        report.output_quarantined,
        report.external_output_consumable,
        report.mdx_quality_gates_required,
        report.mdx_quality_gates_passed,
        report.accepted_for_scoreboard,
        json_string_literal(report.blocked_reason),
    ))
}

fn render_result_from_run_json(
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    let run_id = json_string_field(body, "run_id").unwrap_or_default();
    if run_id.trim().is_empty() {
        return Ok(result_from_run_refusal(
            "name the run to submit as eval evidence",
        ));
    }
    let packet_body = crate::forge_review_packet::assemble_review_packet_json(kernel, &run_id)?;
    let packet: serde_json::Value = match serde_json::from_str(&packet_body) {
        Ok(value) => value,
        Err(_) => {
            return Ok(result_from_run_refusal(
                "review packet did not return valid JSON",
            ));
        }
    };
    if packet["status"].as_str() != Some("OK") {
        let reason = packet["reason"]
            .as_str()
            .unwrap_or("review packet is not available for this run");
        return Ok(result_from_run_refusal(reason));
    }

    let corpus = forge_language_task_corpus();
    let resolved_language_task = match resolve_diff_aligned_language_task(&packet, &corpus) {
        Ok(task) => task,
        Err(reason) => return Ok(result_from_run_refusal(&reason)),
    };
    let language_task = &resolved_language_task;
    let tasks = forge_fleet_benchmark_tasks();
    let benchmark_task = match benchmark_task_for_language_task(language_task, &tasks) {
        Some(task) => task,
        None => {
            return Ok(result_from_run_refusal(
                "no benchmark task matches the run language task alignment",
            ));
        }
    };

    let submission_id = json_string_field(body, "submission_id")
        .unwrap_or_else(|| format!("{}_eval_result", run_id.trim().replace('/', "_")));
    let runner_id =
        json_string_field(body, "runner_id").unwrap_or_else(|| "mdx_native_harness_runner".into());
    let model_profile_id = json_string_field(body, "model_profile_id")
        .unwrap_or_else(|| model_profile_for_run(&packet).to_string());
    let output_artifact_ref = format!(
        "quarantine://forge/fleet-eval/runs/{}/review-packet.json",
        run_id.trim()
    );
    let output_artifact_sha256 = format!("sha256:{}", hex(&sha256(packet_body.as_bytes())));
    let transcript_ref = format!(
        "quarantine://forge/fleet-eval/runs/{}/events.jsonl",
        run_id.trim()
    );
    let pr_handoff_ref = format!(
        "quarantine://forge/fleet-eval/runs/{}/pr-handoff.md",
        run_id.trim()
    );
    let principal_review_gate_results = principal_review_gate_results(&packet, language_task);
    let standards_source_fingerprints = standards_source_fingerprints(&packet);
    let artifact_filter_summary = artifact_filter_summary(&packet);
    let diff_language_pack_impact = diff_language_pack_impact(&packet, language_task);
    let quality_gate_assessment = run_eval_quality_gate_assessment(&packet);
    let request_body = format!(
        r#"{{"submission_id":{},"benchmark_task_id":{},"language_task_corpus_id":{},"language_pack_id":{},"repo_family":{},"runner_id":{},"model_profile_id":{},"output_artifact_ref":{},"output_artifact_sha256":{},"transcript_ref":{},"claimed_check":{},"hidden_check_slot":{},"artifact_noise_expected":{},"principal_review_gate_results":{},"standards_source_fingerprints":{},"artifact_filter_summary":{},"diff_language_pack_impact":{},"principal_engineer_verdict":"pending_principal_engineer_review","pr_handoff_ref":{}}}"#,
        json_string_literal(&submission_id),
        json_string_literal(benchmark_task.task_id),
        json_string_literal(language_task.task_corpus_id),
        json_string_literal(language_task.language_pack_id),
        json_string_literal(language_task.repo_family),
        json_string_literal(&runner_id),
        json_string_literal(&model_profile_id),
        json_string_literal(&output_artifact_ref),
        json_string_literal(&output_artifact_sha256),
        json_string_literal(&transcript_ref),
        json_string_literal(language_task.visible_check),
        json_string_literal(language_task.hidden_check_slot),
        json_string_literal(language_task.artifact_noise_expected),
        json_string_literal(&principal_review_gate_results),
        json_string_literal(&standards_source_fingerprints),
        json_string_literal(&artifact_filter_summary),
        json_string_literal(&diff_language_pack_impact),
        json_string_literal(&pr_handoff_ref),
    );
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let inner = render_result_ingestion_json(&request_body, &mut kernel)?;
    let result: serde_json::Value =
        serde_json::from_str(&inner).map_err(|error| error.to_string())?;
    record_run_eval_quality_gate_event(
        &mut kernel,
        run_id.trim(),
        &quality_gate_assessment,
        result["result_receipt_id"].as_str().unwrap_or(""),
    )?;
    let response_body = format!(
        r#"{{"name":"mdx-forge-fleet-eval-result-from-run-local-post","status":{},"source_run_id":{},"generated_from":"forge_review_packet+language_task_alignment","auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/forge/fleet-eval-results/from-run.json","result_ingestion_route":"/forge/fleet-eval-results.json","projection_route":"/forge/fleet-eval-results/projection.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","review_packet_route":"/forge/review-packet.json","pr_handoff_route":"/forge/pr-handoffs.json","submission_id":{},"benchmark_task_id":{},"language_task_corpus_id":{},"language_pack_id":{},"repo_family":{},"language_engineering_facets":{},"language_evaluation_oracle":{},"language_human_timebox_minutes":{},"language_contamination_policy":{},"hidden_check_slot":{},"artifact_noise_expected":{},"principal_review_gate_results":{},"standards_source_fingerprints":{},"artifact_filter_summary":{},"diff_language_pack_impact":{},"principal_engineer_verdict":{},"pr_handoff_ref":{},"runner_id":{},"model_profile_id":{},"result_receipt_id":{},"policy_decision_id":{},"output_artifact_ref":{},"output_artifact_sha256":{},"transcript_ref":{},"claimed_check":{},"output_quarantined":true,"external_output_consumable":false,"mdx_quality_gates_required":true,"mdx_quality_gates_passed":false,"machine_quality_gates_passed":{},"can_be_scored_after_principal_review":{},"accepted_for_scoreboard":false,"accepted_for_scoreboard_after_principal_review":false,"blocked_reason":"pending_mdx_quality_gates","mdx_quality_gate_assessment":{},"live_substrate_required":false,"production_write_allowed":false}}"#,
        json_string_literal(
            result["status"]
                .as_str()
                .unwrap_or("FLEET_EVAL_RESULT_QUARANTINED_PENDING_MDX_GATES")
        ),
        json_string_literal(run_id.trim()),
        json_string_literal(result["auth_session_status"].as_str().unwrap_or("unknown")),
        json_string_literal(result["submission_id"].as_str().unwrap_or(&submission_id)),
        json_string_literal(
            result["benchmark_task_id"]
                .as_str()
                .unwrap_or(benchmark_task.task_id)
        ),
        json_string_literal(
            result["language_task_corpus_id"]
                .as_str()
                .unwrap_or(language_task.task_corpus_id)
        ),
        json_string_literal(
            result["language_pack_id"]
                .as_str()
                .unwrap_or(language_task.language_pack_id)
        ),
        json_string_literal(
            result["repo_family"]
                .as_str()
                .unwrap_or(language_task.repo_family)
        ),
        json_string_literal(
            result["language_engineering_facets"]
                .as_str()
                .unwrap_or(language_task_engineering_facets(language_task))
        ),
        json_string_literal(
            result["language_evaluation_oracle"]
                .as_str()
                .unwrap_or(language_task_evaluation_oracle(language_task))
        ),
        result["language_human_timebox_minutes"]
            .as_u64()
            .unwrap_or(language_task_human_timebox_minutes(language_task) as u64),
        json_string_literal(
            result["language_contamination_policy"]
                .as_str()
                .unwrap_or(language_task_contamination_policy(language_task))
        ),
        json_string_literal(
            result["hidden_check_slot"]
                .as_str()
                .unwrap_or(language_task.hidden_check_slot)
        ),
        json_string_literal(
            result["artifact_noise_expected"]
                .as_str()
                .unwrap_or(language_task.artifact_noise_expected)
        ),
        json_string_literal(
            result["principal_review_gate_results"]
                .as_str()
                .unwrap_or(&principal_review_gate_results)
        ),
        json_string_literal(
            result["standards_source_fingerprints"]
                .as_str()
                .unwrap_or(&standards_source_fingerprints)
        ),
        json_string_literal(
            result["artifact_filter_summary"]
                .as_str()
                .unwrap_or(&artifact_filter_summary)
        ),
        json_string_literal(
            result["diff_language_pack_impact"]
                .as_str()
                .unwrap_or(&diff_language_pack_impact)
        ),
        json_string_literal(
            result["principal_engineer_verdict"]
                .as_str()
                .unwrap_or("pending_principal_engineer_review")
        ),
        json_string_literal(result["pr_handoff_ref"].as_str().unwrap_or(&pr_handoff_ref)),
        json_string_literal(result["runner_id"].as_str().unwrap_or(&runner_id)),
        json_string_literal(
            result["model_profile_id"]
                .as_str()
                .unwrap_or(&model_profile_id)
        ),
        json_string_literal(result["result_receipt_id"].as_str().unwrap_or("")),
        json_string_literal(result["policy_decision_id"].as_str().unwrap_or("")),
        json_string_literal(&output_artifact_ref),
        json_string_literal(&output_artifact_sha256),
        json_string_literal(&transcript_ref),
        json_string_literal(language_task.visible_check),
        quality_gate_assessment.gates_passed,
        quality_gate_assessment.gates_passed,
        quality_gate_assessment.to_json(),
    );
    Ok(RouteResponse::json("200 OK", response_body))
}

fn result_from_run_refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-fleet-eval-result-from-run-local-post","status":"REFUSED","reason":{},"source_run_id":"","generated_from":"forge_review_packet+language_task_alignment","auth_session_required":true,"writes_route":"/forge/fleet-eval-results/from-run.json","result_ingestion_route":"/forge/fleet-eval-results.json","projection_route":"/forge/fleet-eval-results/projection.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","review_packet_route":"/forge/review-packet.json","pr_handoff_route":"/forge/pr-handoffs.json","result_receipt_id":"","policy_decision_id":"","output_quarantined":false,"external_output_consumable":false,"mdx_quality_gates_required":true,"accepted_for_scoreboard":false,"live_substrate_required":false,"production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

fn render_principal_review_json(
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    let run_id = json_string_field(body, "run_id").unwrap_or_default();
    let result_receipt_id = json_string_field(body, "result_receipt_id").unwrap_or_default();
    let verdict =
        json_string_field(body, "principal_engineer_verdict").unwrap_or_else(|| "reject".into());
    let normalized_verdict = match verdict.trim() {
        "accept" | "accepted" | "accepted_for_scoreboard" => "accepted_for_scoreboard",
        "reject" | "rejected" | "rejected_for_scoreboard" => "rejected_for_scoreboard",
        _ => {
            return principal_review_refusal(
                kernel,
                "principal_engineer_verdict must be accepted_for_scoreboard or rejected_for_scoreboard",
                &run_id,
                &result_receipt_id,
            );
        }
    };
    if run_id.trim().is_empty() {
        return principal_review_refusal(
            kernel,
            "name the run under review",
            &run_id,
            &result_receipt_id,
        );
    }
    if result_receipt_id.trim().is_empty() {
        return principal_review_refusal(
            kernel,
            "name the eval result receipt under review",
            &run_id,
            &result_receipt_id,
        );
    }
    let packet_body = crate::forge_review_packet::assemble_review_packet_json(kernel, &run_id)?;
    let packet: serde_json::Value =
        serde_json::from_str(&packet_body).map_err(|error| error.to_string())?;
    if packet["status"].as_str() != Some("OK") {
        return principal_review_refusal(
            kernel,
            packet["reason"]
                .as_str()
                .unwrap_or("review packet is not available"),
            &run_id,
            &result_receipt_id,
        );
    }
    let quality = latest_eval_quality_gate_for_run(kernel, &run_id, &result_receipt_id)?;
    if quality.result_receipt_id.is_empty() {
        return principal_review_refusal(
            kernel,
            "run has no matching eval quality gate assessment",
            &run_id,
            &result_receipt_id,
        );
    }
    if !quality.can_be_scored_after_principal_review {
        return principal_review_refusal(
            kernel,
            "run is blocked before principal review; fix machine quality gates first",
            &run_id,
            &result_receipt_id,
        );
    }
    // Same diff-driven source of truth as ingestion, so principal review scores
    // the run against the language the change actually touched, not the repo's
    // dominant pack.
    let corpus = forge_language_task_corpus();
    let language_task = resolve_diff_aligned_language_task(&packet, &corpus)?;
    let runner_id = runner_id_for_principal_review(kernel, &run_id, &result_receipt_id, &packet)?;
    let accepted = normalized_verdict == "accepted_for_scoreboard";
    if accepted {
        let runtime_policy =
            runtime_smoke_policy_for_principal_review(kernel, &run_id, &runner_id)?;
        if runtime_policy.smoke_required && !runtime_policy.smoke_passed {
            return principal_review_refusal(
                kernel,
                "machine runtime drift requires a passed runtime smoke for this fingerprint before scorecard acceptance",
                &run_id,
                &result_receipt_id,
            );
        }
        let oracle_policy = eval_oracle_policy_for_principal_review(kernel, &run_id)?;
        if oracle_policy.oracle_required && !oracle_policy.oracle_passed {
            return principal_review_refusal(
                kernel,
                oracle_policy.refusal_reason,
                &run_id,
                &result_receipt_id,
            );
        }
    }
    let eval_fixture =
        run_payload_value_for_principal_review(kernel, &run_id, "machine_league_eval_fixture")?;
    let eval_fixture_family = run_payload_value_for_principal_review(
        kernel,
        &run_id,
        "machine_league_eval_fixture_family",
    )
    .unwrap_or_default();
    let eval_fixture_sibling = run_payload_value_for_principal_review(
        kernel,
        &run_id,
        "machine_league_eval_fixture_sibling",
    )
    .unwrap_or_default();
    let transferable_edge =
        run_payload_value_for_principal_review(kernel, &run_id, "machine_league_transferable_edge")
            .unwrap_or_default();
    let eval_fixture_snapshot_id =
        run_payload_value_for_principal_review(kernel, &run_id, "eval_fixture_snapshot_id")?;
    let eval_oracle_status =
        run_payload_value_for_principal_review(kernel, &run_id, "eval_oracle_status")?;
    let contamination_status =
        run_payload_value_for_principal_review(kernel, &run_id, "contamination_status")?;
    // Carry the trial's solver mode onto the acceptance receipt so a scripted
    // rehearsal stays off the scoreboard even after a principal accepts it.
    let native_execution_mode =
        run_payload_value_for_principal_review(kernel, &run_id, "native_execution_mode")?;
    let model_identity = model_identity_for_run(&packet);
    let provider_family = provider_family_for_model_profile(&model_identity.model_profile_id);
    let model_id = model_identity.model_id.as_str();
    let total_score = if accepted {
        (70 + quality.passed_gate_count.saturating_mul(3)).min(100)
    } else {
        0
    }
    .to_string();
    let passed_gate_count = quality.passed_gate_count.to_string();
    let failed_gate_count = quality.failed_gate_count.to_string();
    let accepted_for_scoreboard = accepted.to_string();
    let principal_review_status = normalized_verdict.to_string();
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:forge_eval",
                run_id: run_id.trim(),
                event: "evidence_appended",
                work_item_id: "forge_eval_principal_review",
                detail: &format!(
                    "eval_principal_reviewed status={} result_receipt_id={}",
                    normalized_verdict,
                    result_receipt_id.trim()
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:forge_eval"),
            &[
                (
                    "eval_principal_review_status",
                    principal_review_status.as_str(),
                ),
                ("eval_result_receipt_id", result_receipt_id.trim()),
                ("language_task_corpus_id", language_task.task_corpus_id),
                ("language_pack_id", language_task.language_pack_id),
                ("language_task_class", language_task.task_class),
                ("language_complexity_tier", language_task.complexity_tier),
                ("machine_league_eval_fixture", eval_fixture.as_str()),
                (
                    "machine_league_eval_fixture_family",
                    eval_fixture_family.as_str(),
                ),
                (
                    "machine_league_eval_fixture_sibling",
                    eval_fixture_sibling.as_str(),
                ),
                (
                    "machine_league_transferable_edge",
                    transferable_edge.as_str(),
                ),
                (
                    "eval_fixture_snapshot_id",
                    eval_fixture_snapshot_id.as_str(),
                ),
                ("eval_oracle_status", eval_oracle_status.as_str()),
                ("contamination_status", contamination_status.as_str()),
                ("native_execution_mode", native_execution_mode.as_str()),
                ("runner_id", runner_id.as_str()),
                ("winning_runner_id", runner_id.as_str()),
                ("baseline_runner_id", "mdx_native_harness_runner"),
                ("model_profile_id", model_identity.model_profile_id.as_str()),
                ("provider_family", provider_family),
                ("model_id", model_id),
                ("total_score", total_score.as_str()),
                ("eval_passed_gate_count", passed_gate_count.as_str()),
                ("eval_failed_gate_count", failed_gate_count.as_str()),
                (
                    "mdx_quality_gates_passed",
                    if quality.machine_quality_gates_passed {
                        "true"
                    } else {
                        "false"
                    },
                ),
                ("accepted_for_scoreboard", accepted_for_scoreboard.as_str()),
                ("external_output_consumable", "false"),
                ("eval_grants_execution_authority", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-fleet-eval-principal-review-local-post","status":{},"run_id":{},"result_receipt_id":{},"principal_engineer_verdict":{},"review_receipt_id":{},"language_task_corpus_id":{},"language_pack_id":{},"runner_id":{},"model_profile_id":{},"provider_family":{},"model_id":{},"total_score":{},"machine_quality_gates_passed":{},"accepted_for_scoreboard":{},"external_output_consumable":false,"production_write_allowed":false}}"#,
            json_string_literal(if accepted {
                "ACCEPTED_FOR_SCOREBOARD"
            } else {
                "REJECTED_FOR_SCOREBOARD"
            }),
            json_string_literal(run_id.trim()),
            json_string_literal(result_receipt_id.trim()),
            json_string_literal(normalized_verdict),
            json_string_literal(&report.receipt_id),
            json_string_literal(language_task.task_corpus_id),
            json_string_literal(language_task.language_pack_id),
            json_string_literal(&runner_id),
            json_string_literal(&model_identity.model_profile_id),
            json_string_literal(provider_family),
            json_string_literal(model_id),
            total_score,
            quality.machine_quality_gates_passed,
            accepted,
        ),
    ))
}

fn principal_review_refusal(
    kernel: &Arc<RwLock<MdxKernel>>,
    reason: &str,
    run_id: &str,
    result_receipt_id: &str,
) -> Result<RouteResponse, String> {
    let review_run_id = if run_id.trim().is_empty() {
        "forge_eval_principal_review_refusal"
    } else {
        run_id.trim()
    };
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:forge_eval",
                run_id: review_run_id,
                event: "evidence_appended",
                work_item_id: "forge_eval_principal_review_refusal",
                detail: &format!("eval_principal_review_refused reason={}", reason),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:forge_eval"),
            &[
                ("eval_principal_review_status", "refused"),
                ("eval_principal_review_refusal_reason", reason),
                ("eval_result_receipt_id", result_receipt_id.trim()),
                ("accepted_for_scoreboard", "false"),
                ("external_output_consumable", "false"),
                ("eval_grants_execution_authority", "false"),
            ],
        )
        .map_err(|error| error.message())?;
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-fleet-eval-principal-review-local-post","status":"REFUSED","reason":{},"run_id":{},"result_receipt_id":{},"review_receipt_id":{},"accepted_for_scoreboard":false,"external_output_consumable":false,"production_write_allowed":false}}"#,
            json_string_literal(reason),
            json_string_literal(review_run_id),
            json_string_literal(result_receipt_id.trim()),
            json_string_literal(&report.receipt_id)
        ),
    ))
}

fn runner_id_for_principal_review(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
    result_receipt_id: &str,
    packet: &serde_json::Value,
) -> Result<String, String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    if let Some(receipt) = kernel.ledger().query().by_id(result_receipt_id.trim()) {
        let runner_id = payload_value(receipt, "runner_id");
        if !runner_id.trim().is_empty() {
            return Ok(runner_id.to_string());
        }
    }
    for receipt in kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
    {
        if payload_value(receipt, "run_id") == run_id.trim() {
            let runner_id = payload_value(receipt, "runner_profile_runner_id");
            if !runner_id.trim().is_empty() {
                return Ok(runner_id.to_string());
            }
        }
    }
    Ok(packet["runner_profile"]["runner_id"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("mdx_native_harness_runner")
        .to_string())
}

struct RuntimeSmokePolicy {
    smoke_required: bool,
    smoke_passed: bool,
}

struct EvalOraclePolicy {
    oracle_required: bool,
    oracle_passed: bool,
    refusal_reason: &'static str,
}

fn eval_oracle_policy_for_principal_review(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
) -> Result<EvalOraclePolicy, String> {
    let eval_fixture =
        run_payload_value_for_principal_review(kernel, run_id, "machine_league_eval_fixture")?;
    if eval_fixture.trim().is_empty() {
        return Ok(EvalOraclePolicy {
            oracle_required: false,
            oracle_passed: true,
            refusal_reason: "",
        });
    }
    let eval_oracle_status =
        run_payload_value_for_principal_review(kernel, run_id, "eval_oracle_status")?;
    let hidden_check_observed =
        run_payload_value_for_principal_review(kernel, run_id, "hidden_check_observed")?;
    let hidden_check_passed =
        run_payload_value_for_principal_review(kernel, run_id, "hidden_check_passed")?;
    let contamination_status =
        run_payload_value_for_principal_review(kernel, run_id, "contamination_status")?;
    let eval_fixture_snapshot_id =
        run_payload_value_for_principal_review(kernel, run_id, "eval_fixture_snapshot_id")?;
    let oracle_passed = eval_oracle_status == "visible_and_hidden_checks_passed"
        && hidden_check_observed == "true"
        && hidden_check_passed == "true"
        && contamination_status == "fresh_held_out_fixture"
        && eval_fixture_snapshot_id != "unknown_fixture_snapshot"
        && !eval_fixture_snapshot_id.trim().is_empty();
    let refusal_reason = if eval_oracle_status != "visible_and_hidden_checks_passed" {
        "machine league fixture acceptance requires visible and hidden oracle checks to pass"
    } else if hidden_check_observed != "true" || hidden_check_passed != "true" {
        "machine league fixture acceptance requires hidden-check evidence"
    } else if contamination_status != "fresh_held_out_fixture" {
        "machine league fixture acceptance requires a fresh held-out contamination status"
    } else if eval_fixture_snapshot_id == "unknown_fixture_snapshot"
        || eval_fixture_snapshot_id.trim().is_empty()
    {
        "machine league fixture acceptance requires a stable fixture snapshot id"
    } else {
        ""
    };
    Ok(EvalOraclePolicy {
        oracle_required: true,
        oracle_passed,
        refusal_reason,
    })
}

fn runtime_smoke_policy_for_principal_review(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
    runner_id: &str,
) -> Result<RuntimeSmokePolicy, String> {
    if runner_id.trim().is_empty() || runner_id == "mdx_native_harness_runner" {
        return Ok(RuntimeSmokePolicy {
            smoke_required: false,
            smoke_passed: true,
        });
    }
    let drift_status =
        run_payload_value_for_principal_review(kernel, run_id, "machine_runtime_drift_status")?;
    if drift_status != "runtime_drift_detected" {
        return Ok(RuntimeSmokePolicy {
            smoke_required: false,
            smoke_passed: true,
        });
    }
    let fingerprint_id =
        run_payload_value_for_principal_review(kernel, run_id, "machine_runtime_fingerprint_id")?;
    if fingerprint_id.trim().is_empty() {
        return Ok(RuntimeSmokePolicy {
            smoke_required: true,
            smoke_passed: false,
        });
    }
    Ok(RuntimeSmokePolicy {
        smoke_required: true,
        smoke_passed: latest_runtime_smoke_passed(kernel, runner_id, &fingerprint_id)?,
    })
}

fn latest_runtime_smoke_passed(
    kernel: &Arc<RwLock<MdxKernel>>,
    runner_id: &str,
    fingerprint_id: &str,
) -> Result<bool, String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    for receipt in kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
    {
        if payload_value(receipt, "machine_runtime_smoke_status") != "runtime_smoke_passed" {
            continue;
        }
        if payload_value(receipt, "machine_runtime_smoke_passed") != "true" {
            continue;
        }
        if payload_value(receipt, "machine_runtime_runner_id") != runner_id.trim() {
            continue;
        }
        if payload_value(receipt, "machine_runtime_fingerprint_id") != fingerprint_id.trim() {
            continue;
        }
        return Ok(true);
    }
    Ok(false)
}

fn run_payload_value_for_principal_review(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
    key: &str,
) -> Result<String, String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    for receipt in kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
    {
        if payload_value(receipt, "run_id") == run_id.trim() {
            let value = payload_value(receipt, key);
            if !value.trim().is_empty() {
                return Ok(value.to_string());
            }
        }
    }
    Ok(String::new())
}

#[derive(Default)]
struct EvalQualityGateForRun {
    result_receipt_id: String,
    machine_quality_gates_passed: bool,
    can_be_scored_after_principal_review: bool,
    passed_gate_count: u32,
    failed_gate_count: u32,
}

fn latest_eval_quality_gate_for_run(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
    result_receipt_id: &str,
) -> Result<EvalQualityGateForRun, String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let mut latest = EvalQualityGateForRun::default();
    for receipt in kernel.ledger().query().by_kind("forge.run.event").iter() {
        if receipt.payload.get("run_id").map(String::as_str) != Some(run_id)
            || receipt.payload.get("event").map(String::as_str) != Some("evidence_appended")
            || receipt
                .payload
                .get("eval_result_receipt_id")
                .map(String::as_str)
                != Some(result_receipt_id)
        {
            continue;
        }
        latest.result_receipt_id = result_receipt_id.to_string();
        latest.machine_quality_gates_passed =
            payload_value(receipt, "eval_machine_quality_gates_passed") == "true";
        latest.can_be_scored_after_principal_review =
            payload_value(receipt, "eval_can_be_scored_after_principal_review") == "true";
        latest.passed_gate_count = payload_value(receipt, "eval_passed_gate_count")
            .parse()
            .unwrap_or(0);
        latest.failed_gate_count = payload_value(receipt, "eval_failed_gate_count")
            .parse()
            .unwrap_or(0);
    }
    Ok(latest)
}

fn provider_family_for_model_profile(model_profile_id: &str) -> &'static str {
    mdx_core::forge_fleet_model_matrix_profiles()
        .into_iter()
        .find(|profile| profile.profile_id == model_profile_id)
        .map(|profile| profile.provider_family)
        .unwrap_or("xai")
}

fn record_run_eval_quality_gate_event(
    kernel: &mut MdxKernel,
    run_id: &str,
    assessment: &RunEvalQualityGateAssessment,
    result_receipt_id: &str,
) -> Result<(), String> {
    let passed_gates = assessment.passed_gates.join(",");
    let failed_gates = assessment.failed_gates.join(",");
    let gates_passed = if assessment.gates_passed {
        "true"
    } else {
        "false"
    };
    let can_be_scored_after_principal_review = if assessment.can_be_scored_after_principal_review {
        "true"
    } else {
        "false"
    };
    let passed_gate_count = assessment.passed_gates.len().to_string();
    let failed_gate_count = assessment.failed_gates.len().to_string();
    let evidence_fields = [
        ("eval_quality_gate_status", assessment.status),
        ("eval_machine_quality_gates_passed", gates_passed),
        (
            "eval_can_be_scored_after_principal_review",
            can_be_scored_after_principal_review,
        ),
        ("eval_passed_gate_count", passed_gate_count.as_str()),
        ("eval_failed_gate_count", failed_gate_count.as_str()),
        ("eval_passed_gates", passed_gates.as_str()),
        ("eval_failed_gates", failed_gates.as_str()),
        ("eval_result_receipt_id", result_receipt_id),
        ("eval_accepts_for_scoreboard", "false"),
        ("eval_grants_execution_authority", "false"),
    ];
    kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id: "local_tenant",
                actor_id: "agent:forge_eval",
                run_id,
                event: "evidence_appended",
                work_item_id: "forge_eval_from_run",
                detail: &format!(
                    "eval_quality_gate_assessed status={} machine_gates_passed={} failed_gates={}",
                    assessment.status,
                    assessment.gates_passed,
                    if failed_gates.is_empty() {
                        "none"
                    } else {
                        failed_gates.as_str()
                    }
                ),
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            &GovernedWriteIdentity::local_demo("agent:forge_eval"),
            &evidence_fields,
        )
        .map(|_| ())
        .map_err(|error| error.message())
}

#[derive(Debug, PartialEq, Eq)]
struct RunEvalQualityGateAssessment {
    gates_passed: bool,
    status: &'static str,
    passed_gates: Vec<&'static str>,
    failed_gates: Vec<&'static str>,
    can_be_scored_after_principal_review: bool,
}

impl RunEvalQualityGateAssessment {
    fn to_json(&self) -> String {
        format!(
            r#"{{"generated_from":"forge_review_packet.local_machine_quality_gates","status":{},"gates_passed":{},"can_be_scored_after_principal_review":{},"passed_gate_count":{},"failed_gate_count":{},"passed_gates":[{}],"failed_gates":[{}],"acceptance_boundary":"local machine gates are necessary but not sufficient; scoreboard acceptance still requires principal review or hidden-check authority","accepted_for_scoreboard":false,"production_write_allowed":false}}"#,
            json_string_literal(self.status),
            self.gates_passed,
            self.can_be_scored_after_principal_review,
            self.passed_gates.len(),
            self.failed_gates.len(),
            json_static_string_array(&self.passed_gates),
            json_static_string_array(&self.failed_gates),
        )
    }
}

fn run_eval_quality_gate_assessment(packet: &serde_json::Value) -> RunEvalQualityGateAssessment {
    let real_change_count = packet["diff"]["real_change_count"].as_u64().unwrap_or(0);
    let candidate_count = packet["candidate_comparison"]["candidate_count"]
        .as_u64()
        .unwrap_or(1);
    let principal_orientation_required = packet["repo_intelligence"]["principal_orientation_gate"]
        ["required"]
        .as_bool()
        .unwrap_or(false);
    let mut passed_gates = Vec::new();
    let mut failed_gates = Vec::new();
    let mut gate = |name: &'static str, passed: bool| {
        if passed {
            passed_gates.push(name);
        } else {
            failed_gates.push(name);
        }
    };
    gate(
        "terminal_done",
        packet["run_status"].as_str() == Some("done")
            || packet["status"].as_str() == Some("OK")
                && matches!(
                    packet["review_status"].as_str(),
                    Some("ready" | "ready_for_review")
                ),
    );
    gate("real_diff_present", real_change_count > 0);
    gate(
        "proof_coverage_satisfied",
        packet["proof_coverage"]["status"].as_str() == Some("satisfied"),
    );
    gate(
        "proof_scope_covered",
        real_change_count == 0
            || matches!(
                packet["proof_scope"]["status"].as_str(),
                Some("covered" | "not_applicable")
            ),
    );
    gate(
        "behavior_proof_covered",
        real_change_count == 0 || packet["behavior_proof"]["status"].as_str() == Some("covered"),
    );
    gate(
        "semantic_orientation_observed",
        !principal_orientation_required
            || packet["repo_intelligence"]["principal_orientation_gate"]["observed"]
                .as_bool()
                .unwrap_or(false),
    );
    gate(
        "related_tests_observed",
        !principal_orientation_required
            || real_change_count == 0
            || packet["repo_intelligence"]["related_tests_observed"]
                .as_bool()
                .unwrap_or(false),
    );
    let semantic_operations = packet["repo_intelligence"]["semantic_query_operations_observed"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    gate(
        "dependency_map_observed_for_parallel_candidates",
        candidate_count <= 1
            || semantic_operations
                .iter()
                .any(|item| item.as_str() == Some("dependency_map")),
    );
    gate(
        "recommended_candidate_selected",
        candidate_count <= 1
            || packet["candidate_comparison"]["current_run_id"].as_str()
                == packet["candidate_comparison"]["recommended_run_id"].as_str(),
    );
    gate(
        "artifact_noise_folded",
        packet["diff"]["artifact_summary"]
            .as_str()
            .map(|summary| !summary.trim().is_empty())
            .unwrap_or(true),
    );
    let gates_passed = failed_gates.is_empty();
    RunEvalQualityGateAssessment {
        gates_passed,
        status: if gates_passed {
            "READY_FOR_PRINCIPAL_REVIEW"
        } else {
            "BLOCKED_BEFORE_PRINCIPAL_REVIEW"
        },
        passed_gates,
        failed_gates,
        can_be_scored_after_principal_review: gates_passed,
    }
}

fn json_static_string_array(values: &[&'static str]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",")
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
}

struct RunModelIdentity {
    model_profile_id: String,
    model_id: String,
}

fn model_identity_for_run(packet: &serde_json::Value) -> RunModelIdentity {
    let built_by_model = packet["built_by"]["model"].as_str().unwrap_or("").trim();
    if !built_by_model.is_empty() {
        return RunModelIdentity {
            model_profile_id: model_profile_for_model_id(built_by_model).to_string(),
            model_id: built_by_model.to_string(),
        };
    }
    let selected_profile =
        packet["repo_intelligence"]["builder_casting"]["selected_model_profile_id"]
            .as_str()
            .unwrap_or("")
            .trim();
    let selected_model = packet["repo_intelligence"]["builder_casting"]["selected_model_id"]
        .as_str()
        .unwrap_or("")
        .trim();
    if !selected_profile.is_empty() || !selected_model.is_empty() {
        let profile_id = if selected_profile.is_empty() {
            model_profile_for_model_id(selected_model).to_string()
        } else {
            selected_profile.to_string()
        };
        return RunModelIdentity {
            model_profile_id: profile_id,
            model_id: selected_model.to_string(),
        };
    }
    let recommended_profile =
        packet["repo_intelligence"]["builder_casting"]["recommended_model_profile_id"]
            .as_str()
            .unwrap_or("")
            .trim();
    let recommended_model = packet["repo_intelligence"]["builder_casting"]["recommended_model_id"]
        .as_str()
        .unwrap_or("")
        .trim();
    if !recommended_profile.is_empty() || !recommended_model.is_empty() {
        let profile_id = if recommended_profile.is_empty() {
            model_profile_for_model_id(recommended_model).to_string()
        } else {
            recommended_profile.to_string()
        };
        return RunModelIdentity {
            model_profile_id: profile_id,
            model_id: recommended_model.to_string(),
        };
    }
    RunModelIdentity {
        model_profile_id: "codex_xai_responses_profile".to_string(),
        model_id: String::new(),
    }
}

fn model_profile_for_run(packet: &serde_json::Value) -> String {
    model_identity_for_run(packet).model_profile_id
}

fn model_profile_for_model_id(model: &str) -> &'static str {
    let model = model.to_ascii_lowercase();
    if model.contains("gpt") || model.contains("openai") {
        "codex_openai_responses_profile"
    } else if model.contains("gemini") {
        "codex_gemini_responses_profile"
    } else if model.contains("claude")
        || model.contains("anthropic")
        || model.contains("opus")
        || model.contains("sonnet")
    {
        "codex_anthropic_responses_profile"
    } else if model.contains("bedrock") || model.contains("aws") {
        "codex_bedrock_responses_profile"
    } else {
        "codex_xai_responses_profile"
    }
}

fn principal_review_gate_results(
    packet: &serde_json::Value,
    language_task: &ForgeLanguageTaskCorpusEntry,
) -> String {
    let checklist = packet["principal_review"]["checklist"]
        .as_array()
        .map(|checklist| {
            checklist
                .iter()
                .filter_map(|item| {
                    let kind = item["kind"].as_str().unwrap_or("");
                    let status = item["status"].as_str().unwrap_or("");
                    if kind.trim().is_empty() || status.trim().is_empty() {
                        None
                    } else {
                        Some(format!("{kind}:{status}"))
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let fallback = if checklist.is_empty() {
        "pending_review_packet_observation".to_string()
    } else {
        checklist.join("|")
    };
    language_task
        .required_principal_review_gates
        .split(',')
        .filter_map(|gate| {
            let gate = gate.trim();
            if gate.is_empty() {
                None
            } else {
                let prefix = gate.split(':').next().unwrap_or(gate);
                Some(format!("{prefix}={fallback}"))
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn pending_principal_review_gate_results(required_gates: &str) -> String {
    required_gates
        .split(',')
        .filter_map(|gate| {
            let gate = gate.trim();
            if gate.is_empty() {
                None
            } else {
                Some(format!("{gate}=pending_principal_review_gate_result"))
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn standards_source_fingerprints(packet: &serde_json::Value) -> String {
    let fingerprints = packet["repo_intelligence"]["standards_source_fingerprints"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::trim))
                .filter(|item| !item.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if fingerprints.is_empty() {
        "AGENTS.md=fnv1a64:0000000000000000".to_string()
    } else {
        fingerprints.join(",")
    }
}

fn artifact_filter_summary(packet: &serde_json::Value) -> String {
    let diff = &packet["diff"];
    format!(
        "files={},real={},generated={},artifacts={},artifact_summary={}",
        diff["file_count"].as_u64().unwrap_or(0),
        diff["real_change_count"].as_u64().unwrap_or(0),
        diff["generated_count"].as_u64().unwrap_or(0),
        diff["artifact_count"].as_u64().unwrap_or(0),
        diff["artifact_summary"].as_str().unwrap_or("none recorded")
    )
}

fn diff_language_pack_impact(
    packet: &serde_json::Value,
    language_task: &ForgeLanguageTaskCorpusEntry,
) -> String {
    let impact = packet["diff"]["language_pack_impact"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let pack = item["language_pack_id"].as_str().unwrap_or("");
                    if pack.trim().is_empty() {
                        None
                    } else {
                        Some(pack.to_string())
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if impact.is_empty() {
        language_task.language_pack_id.to_string()
    } else {
        impact.join(",")
    }
}

// The single pack a change most touched, from the real diff. First-seen wins
// ties for determinism.
fn dominant_diff_language_pack(packet: &serde_json::Value) -> Option<String> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for item in packet["diff"]["language_pack_impact"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let pack = item["language_pack_id"].as_str().unwrap_or("").trim();
        if pack.is_empty() {
            continue;
        }
        if !counts.contains_key(pack) {
            order.push(pack.to_string());
        }
        *counts.entry(pack.to_string()).or_insert(0) += 1;
    }
    let mut best: Option<String> = None;
    for pack in order {
        let c = counts.get(&pack).copied().unwrap_or(0);
        if best
            .as_ref()
            .map(|b| c > counts.get(b).copied().unwrap_or(0))
            .unwrap_or(true)
        {
            best = Some(pack);
        }
    }
    best
}

// The corpus eval task whose language matches the change, preserving the
// run-start class/tier where that language offers it.
fn corpus_task_for_pack(
    corpus: &[ForgeLanguageTaskCorpusEntry],
    pack: &str,
    aligned: Option<ForgeLanguageTaskCorpusEntry>,
) -> Option<ForgeLanguageTaskCorpusEntry> {
    let pack_tasks: Vec<ForgeLanguageTaskCorpusEntry> = corpus
        .iter()
        .copied()
        .filter(|task| task.language_pack_id == pack)
        .collect();
    if pack_tasks.is_empty() {
        return None;
    }
    if let Some(aligned) = aligned {
        if let Some(task) = pack_tasks.iter().find(|task| {
            task.task_class == aligned.task_class && task.complexity_tier == aligned.complexity_tier
        }) {
            return Some(*task);
        }
        if let Some(task) = pack_tasks
            .iter()
            .find(|task| task.complexity_tier == aligned.complexity_tier)
        {
            return Some(*task);
        }
    }
    pack_tasks.first().copied()
}

// Reconcile the eval task with the change's real language. The alignment
// recorded at run start is keyed on the repository's dominant language pack, so
// a JS change in a rust-cargo repo was pointed at a rust-cargo task, which the
// eval bridge then rejected against the node diff. Re-point it at a task that
// matches what the change actually touched, or refuse honestly - never force a
// mismatch. Falls back to the run-start alignment when no language-pack-bearing
// file changed (docs/config only), so rust-in-rust runs are unaffected.
fn resolve_diff_aligned_language_task(
    packet: &serde_json::Value,
    corpus: &[ForgeLanguageTaskCorpusEntry],
) -> Result<ForgeLanguageTaskCorpusEntry, String> {
    let recorded_id = packet["language_task_alignment"]["task_corpus_id"]
        .as_str()
        .unwrap_or("")
        .trim();
    let aligned = corpus
        .iter()
        .copied()
        .find(|task| task.task_corpus_id == recorded_id);
    match dominant_diff_language_pack(packet) {
        Some(pack) => aligned
            .filter(|task| task.language_pack_id == pack)
            .or_else(|| corpus_task_for_pack(corpus, &pack, aligned))
            .ok_or_else(|| {
                format!(
                    "no eval task in the canonical corpus matches this change's language ({pack}), so Forge cannot submit it to the eval lane"
                )
            }),
        None => aligned.ok_or_else(|| {
            "run has no language task alignment, so Forge cannot submit it to the eval lane"
                .to_string()
        }),
    }
}

fn render_result_projection_json(kernel: &MdxKernel) -> String {
    let results = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "forge.fleet_eval_result.ingested")
        .map(|receipt| {
            format!(
                r#"{{"submission_id":{},"benchmark_task_id":{},"language_task_corpus_id":{},"language_pack_id":{},"repo_family":{},"language_engineering_facets":{},"language_evaluation_oracle":{},"language_human_timebox_minutes":{},"language_contamination_policy":{},"hidden_check_slot":{},"artifact_noise_expected":{},"principal_review_gate_results":{},"standards_source_fingerprints":{},"artifact_filter_summary":{},"diff_language_pack_impact":{},"principal_engineer_verdict":{},"pr_handoff_ref":{},"runner_id":{},"model_profile_id":{},"result_receipt_id":{},"policy_decision_id":{},"output_artifact_ref":{},"output_artifact_sha256":{},"transcript_ref":{},"claimed_check":{},"output_quarantined":true,"external_output_consumable":false,"mdx_quality_gates_required":true,"mdx_quality_gates_passed":false,"accepted_for_scoreboard":false,"blocked_reason":"pending_mdx_quality_gates"}}"#,
                json_string_literal(payload_value(receipt, "submission_id")),
                json_string_literal(payload_value(receipt, "benchmark_task_id")),
                json_string_literal(payload_value(receipt, "language_task_corpus_id")),
                json_string_literal(payload_value(receipt, "language_pack_id")),
                json_string_literal(payload_value(receipt, "repo_family")),
                json_string_literal(payload_value(receipt, "language_engineering_facets")),
                json_string_literal(payload_value(receipt, "language_evaluation_oracle")),
                payload_value(receipt, "language_human_timebox_minutes"),
                json_string_literal(payload_value(receipt, "language_contamination_policy")),
                json_string_literal(payload_value(receipt, "hidden_check_slot")),
                json_string_literal(payload_value(receipt, "artifact_noise_expected")),
                json_string_literal(payload_value(receipt, "principal_review_gate_results")),
                json_string_literal(payload_value(receipt, "standards_source_fingerprints")),
                json_string_literal(payload_value(receipt, "artifact_filter_summary")),
                json_string_literal(payload_value(receipt, "diff_language_pack_impact")),
                json_string_literal(payload_value(receipt, "principal_engineer_verdict")),
                json_string_literal(payload_value(receipt, "pr_handoff_ref")),
                json_string_literal(payload_value(receipt, "runner_id")),
                json_string_literal(payload_value(receipt, "model_profile_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "output_artifact_ref")),
                json_string_literal(payload_value(receipt, "output_artifact_sha256")),
                json_string_literal(payload_value(receipt, "transcript_ref")),
                json_string_literal(payload_value(receipt, "claimed_check")),
            )
        })
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-forge-fleet-eval-result-local-projection","status":"OK","route":"/forge/fleet-eval-results/projection.json","writes_route":"/forge/fleet-eval-results.json","scoreboard_route":"/forge/fleet-eval-scoreboard.json","result_count":{},"output_quarantined":true,"external_output_consumable":false,"mdx_quality_gates_required":true,"accepted_for_scoreboard":false,"results":[{}]}}"#,
        results.len(),
        results.join(",")
    )
}

fn language_task_corpus_json(report: &ForgeFleetEvalReport) -> String {
    report
        .language_task_corpus
        .iter()
        .map(|task| {
            format!(
                r#"{{ "task_corpus_id": {}, "language_pack_id": {}, "repo_family": {}, "task_class": {}, "complexity_tier": {}, "engineering_facets": {}, "evaluation_oracle": {}, "human_timebox_minutes": {}, "contamination_policy": {}, "visible_check": {}, "hidden_check_slot": {}, "artifact_noise_expected": {}, "required_principal_review_gates": {} }}"#,
                json_string_literal(task.task_corpus_id),
                json_string_literal(task.language_pack_id),
                json_string_literal(task.repo_family),
                json_string_literal(task.task_class),
                json_string_literal(task.complexity_tier),
                json_string_literal(mdx_core::language_task_engineering_facets(task)),
                json_string_literal(mdx_core::language_task_evaluation_oracle(task)),
                mdx_core::language_task_human_timebox_minutes(task),
                json_string_literal(mdx_core::language_task_contamination_policy(task)),
                json_string_literal(task.visible_check),
                json_string_literal(task.hidden_check_slot),
                json_string_literal(task.artifact_noise_expected),
                json_string_literal(task.required_principal_review_gates)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn language_pack_scorecards_json(report: &ForgeFleetEvalReport) -> String {
    report
        .language_pack_scorecards
        .iter()
        .map(|scorecard| {
            format!(
                r#"{{ "language_pack_id": {}, "repo_family": {}, "task_count": {}, "small_task_count": {}, "medium_task_count": {}, "large_task_count": {}, "visible_check_count": {}, "hidden_check_slot_count": {}, "artifact_noise_expectation_count": {}, "principal_review_gate_count": {}, "principal_verdict_required": {}, "ready_for_live_eval": {} }}"#,
                json_string_literal(scorecard.language_pack_id),
                json_string_literal(scorecard.repo_family),
                scorecard.task_count,
                scorecard.small_task_count,
                scorecard.medium_task_count,
                scorecard.large_task_count,
                scorecard.visible_check_count,
                scorecard.hidden_check_slot_count,
                scorecard.artifact_noise_expectation_count,
                scorecard.principal_review_gate_count,
                scorecard.principal_verdict_required,
                scorecard.ready_for_live_eval
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn model_matrix_json(report: &ForgeFleetEvalReport) -> String {
    report
        .model_matrix
        .iter()
        .map(|profile| {
            format!(
                r#"{{ "profile_id": {}, "provider_family": {}, "model_provider": {}, "example_model": {}, "model_id": {}, "model_display_name": {}, "wire_api": {}, "auth_policy": {}, "redaction_policy": {}, "budget_policy": {}, "live_call_allowed": {} }}"#,
                json_string_literal(profile.profile_id),
                json_string_literal(profile.provider_family),
                json_string_literal(profile.model_provider),
                json_string_literal(profile.example_model),
                json_string_literal(profile.example_model),
                json_string_literal(profile.model_display_name),
                json_string_literal(profile.wire_api),
                json_string_literal(profile.auth_policy),
                json_string_literal(profile.redaction_policy),
                json_string_literal(profile.budget_policy),
                profile.live_call_allowed
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn adapter_protocol_tiers_json() -> String {
    [
        (
            "native_harness",
            "MDx native runner inside the governed Forge kernel envelope",
            "mdx_native_harness_runner",
            "scoreboard_eligible",
        ),
        (
            "sdk_adapter",
            "Closed or vendor runner integrated through a production SDK contract",
            "claude_code_external_worker",
            "clearance_required",
        ),
        (
            "protocol_adapter",
            "Runner integrated through an agent protocol such as ACP or MCP",
            "grok_build_cli_external_worker",
            "provider_key_and_runtime_preflight_required",
        ),
        (
            "headless_cli_adapter",
            "Runner invoked as a non-interactive CLI with captured transcript and diff",
            "codex_cli_external_worker",
            "runtime_preflight_required",
        ),
        (
            "local_companion_adapter",
            "User-authenticated local pairing mode that is visible but not scorecard eligible",
            "claude_code_external_worker",
            "local_companion_only",
        ),
        (
            "declared_only",
            "Profile is visible in the league but cannot run until a stricter tier is proven",
            "grok_build_cli_external_worker",
            "not_castable",
        ),
    ]
    .iter()
    .map(|(tier, description, example_runner_id, authority_status)| {
        format!(
            r#"{{"adapter_protocol_tier":{},"description":{},"example_runner_id":{},"authority_status":{},"adapter_execution_allowed":false,"external_output_consumable":false,"production_write_allowed":false}}"#,
            json_string_literal(tier),
            json_string_literal(description),
            json_string_literal(example_runner_id),
            json_string_literal(authority_status),
        )
    })
    .collect::<Vec<_>>()
    .join(", ")
}

#[derive(Clone, Copy, Debug)]
struct RunnerModelCompatibility {
    status: &'static str,
    reason: &'static str,
    castable: bool,
}

fn runner_model_compatibility(
    kernel: Option<&MdxKernel>,
    runner: &FleetRunnerProfile,
    model: &FleetModelMatrixProfile,
) -> RunnerModelCompatibility {
    // Grok Build is open source. It still needs a tenant xAI key for its
    // selected model and all existing runtime, approval, isolation, and
    // quarantine gates, but it is not a closed-runner clearance case.
    if runner.adapter_kind == "grok_build_cli" {
        if runner.wire_api != model.wire_api {
            return RunnerModelCompatibility {
                status: "incompatible_wire_api",
                reason: "runner and model profile use different wire APIs",
                castable: false,
            };
        }
        if machine_option_enabled_by_provider_key(
            global(),
            &current_tenant_id(),
            runner.adapter_kind,
        ) {
            return RunnerModelCompatibility {
                status: "compatible_grok_build_acp_key_present",
                reason: "xAI key presence enables the open-source Grok Build ACP machine option; execution still requires runtime preflight, exact approval, isolation, quarantine, and the machine exec gate",
                castable: true,
            };
        }
        return RunnerModelCompatibility {
            status: "blocked_pending_xai_key",
            reason: "Grok Build is admitted as an open-source premier harness, but XAI_API_KEY is required before Forge can cast it to the Grok model",
            castable: false,
        };
    }
    if closed_runner_requires_clearance(runner.adapter_kind) {
        if runner.adapter_kind == "claude_code"
            && machine_option_enabled_by_provider_key(
                global(),
                &current_tenant_id(),
                runner.adapter_kind,
            )
            && runner.wire_api == model.wire_api
        {
            return RunnerModelCompatibility {
                status: "compatible_claude_agent_sdk_key_present",
                reason: "Anthropic key presence enables Claude as an Agent SDK machine option; execution still requires runtime preflight and the machine exec gate",
                castable: true,
            };
        }
        if runner.adapter_kind == "kimi_code"
            && machine_option_enabled_by_provider_key(
                global(),
                &current_tenant_id(),
                runner.adapter_kind,
            )
            && runner.wire_api == model.wire_api
        {
            return RunnerModelCompatibility {
                status: "compatible_kimi_code_moonshot_key_present",
                reason: "Moonshot key presence enables Kimi Code as a headless stream-json machine option; execution still requires runtime preflight, exact approval, isolated state, quarantine, and the machine exec gate",
                castable: true,
            };
        }
        if let Some(clearance) = closed_runner_clearance_for_runner(kernel, runner.runner_id) {
            if !clearance.scorecard_eligible {
                return RunnerModelCompatibility {
                    status: "cleared_for_local_companion_only",
                    reason: "closed runner is cleared for local companion use but not scorecard fleet eval",
                    castable: false,
                };
            }
            if runner.wire_api == model.wire_api {
                return RunnerModelCompatibility {
                    status: "compatible_closed_runner_cleared",
                    reason: "closed runner has a fleet-eval clearance receipt and shares the model profile wire API",
                    castable: true,
                };
            }
        }
        if runner.wire_api != model.wire_api {
            return RunnerModelCompatibility {
                status: "incompatible_wire_api",
                reason: "runner and model profile use different wire APIs",
                castable: false,
            };
        }
        if runner.adapter_kind == "claude_code" {
            return RunnerModelCompatibility {
                status: "blocked_pending_anthropic_key",
                reason: "Claude is declared, but ANTHROPIC_API_KEY is required before Forge enables it as an Agent SDK machine option",
                castable: false,
            };
        }
        if runner.adapter_kind == "kimi_code" {
            return RunnerModelCompatibility {
                status: "blocked_pending_moonshot_key",
                reason: "Kimi Code is declared, but MOONSHOT_API_KEY is required before Forge enables its ephemeral KIMI_MODEL_* provider channel",
                castable: false,
            };
        }
    }
    if runner.execution_status.contains("TOS_AUTH") {
        if runner.adapter_kind == "grok_build_cli"
            && machine_option_enabled_by_provider_key(
                global(),
                &current_tenant_id(),
                runner.adapter_kind,
            )
            && runner.wire_api == model.wire_api
        {
            return RunnerModelCompatibility {
                status: "compatible_grok_build_acp_key_present",
                reason: "xAI key presence enables Grok Build as an ACP machine option; execution still requires runtime preflight and the machine exec gate",
                castable: true,
            };
        }
        if runner.adapter_kind == "claude_code"
            && machine_option_enabled_by_provider_key(
                global(),
                &current_tenant_id(),
                runner.adapter_kind,
            )
            && runner.wire_api == model.wire_api
        {
            return RunnerModelCompatibility {
                status: "compatible_claude_agent_sdk_key_present",
                reason: "Anthropic key presence enables Claude as an Agent SDK machine option; execution still requires runtime preflight and the machine exec gate",
                castable: true,
            };
        }
        if let Some(clearance) = closed_runner_clearance_for_runner(kernel, runner.runner_id) {
            if !clearance.scorecard_eligible {
                return RunnerModelCompatibility {
                    status: "cleared_for_local_companion_only",
                    reason: "closed runner is cleared for local companion use but not scorecard fleet eval",
                    castable: false,
                };
            }
            if runner.wire_api == model.wire_api {
                return RunnerModelCompatibility {
                    status: "compatible_closed_runner_cleared",
                    reason: "closed runner has a fleet-eval clearance receipt and shares the model profile wire API",
                    castable: true,
                };
            }
        }
        return RunnerModelCompatibility {
            status: "blocked_pending_tos_auth_clearance",
            reason: "runner is declared but parked behind ToS and auth clearance",
            castable: false,
        };
    }
    if runner.wire_api != model.wire_api {
        return RunnerModelCompatibility {
            status: "incompatible_wire_api",
            reason: "runner and model profile use different wire APIs",
            castable: false,
        };
    }
    if runner.model_profile_id == model.profile_id {
        return RunnerModelCompatibility {
            status: "compatible_runner_default_model",
            reason: "runner default model profile uses the same admitted wire API",
            castable: true,
        };
    }
    RunnerModelCompatibility {
        status: "compatible_via_shared_wire_api",
        reason: "runner can be cast to this model profile through the same admitted wire API",
        castable: true,
    }
}

fn runner_model_compatibility_matrix_json(kernel: Option<&MdxKernel>) -> String {
    let runners = forge_fleet_runner_profiles();
    let models = forge_fleet_model_matrix_profiles();
    runners
        .iter()
        .flat_map(|runner| {
            models.iter().map(move |model| {
                let compatibility = runner_model_compatibility(kernel, runner, model);
                let clearance_status = closed_runner_clearance_for_runner(kernel, runner.runner_id)
                    .map(|clearance| {
                        if runner.adapter_kind == "grok_build_cli" {
                            "open_source_execution_clearance_recorded"
                        } else if clearance.scorecard_eligible {
                            "cleared_for_fleet_eval"
                        } else {
                            "cleared_for_local_companion"
                        }
                    })
                    .or_else(|| {
                        if matches!(
                            runner.adapter_kind,
                            "grok_build_cli" | "claude_code" | "kimi_code"
                        )
                            && machine_option_enabled_by_provider_key(global(), &current_tenant_id(), runner.adapter_kind)
                        {
                            Some("key_present_option_enabled")
                        } else {
                            None
                        }
                    })
                    .unwrap_or(if closed_runner_requires_clearance(runner.adapter_kind) {
                        "clearance_required"
                    } else {
                        "clearance_not_required"
                    });
                format!(
                    r#"{{"runner_id":{},"runner_display_name":{},"runner_adapter_kind":{},"adapter_protocol_tier":{},"closed_runner_clearance_status":{},"runner_wire_api":{},"model_profile_id":{},"provider_family":{},"model_provider":{},"model_id":{},"model_display_name":{},"model_wire_api":{},"compatibility_status":{},"compatibility_reason":{},"castable":{},"runner_default_model":{},"auth_policy":{},"redaction_policy":{},"budget_policy":{},"live_call_allowed":{},"machine_option_enabled_by_provider_key":{},"adapter_execution_allowed":false,"external_output_consumable":false,"production_write_allowed":false}}"#,
                    json_string_literal(runner.runner_id),
                    json_string_literal(runner.display_name),
                    json_string_literal(runner.adapter_kind),
                    json_string_literal(&adapter_protocol_tier_for_runner(kernel, runner)),
                    json_string_literal(clearance_status),
                    json_string_literal(runner.wire_api),
                    json_string_literal(model.profile_id),
                    json_string_literal(model.provider_family),
                    json_string_literal(model.model_provider),
                    json_string_literal(model.example_model),
                    json_string_literal(model.model_display_name),
                    json_string_literal(model.wire_api),
                    json_string_literal(compatibility.status),
                    json_string_literal(compatibility.reason),
                    compatibility.castable,
                    runner.model_profile_id == model.profile_id,
                    json_string_literal(model.auth_policy),
                    json_string_literal(model.redaction_policy),
                    json_string_literal(model.budget_policy),
                    model.live_call_allowed,
                    machine_option_enabled_by_provider_key(global(), &current_tenant_id(), runner.adapter_kind),
                )
            })
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn scoring_dimensions_json(report: &ForgeFleetEvalReport) -> String {
    report
        .scoring_dimensions
        .iter()
        .map(|dimension| {
            format!(
                r#"{{ "dimension_id": {}, "weight_pct": {}, "evaluator_kind": {}, "evidence_required": {}, "fail_closed_if_missing": {} }}"#,
                json_string_literal(dimension.dimension_id),
                dimension.weight_pct,
                json_string_literal(dimension.evaluator_kind),
                json_string_literal(dimension.evidence_required),
                dimension.fail_closed_if_missing
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn benchmark_tasks_json(report: &ForgeFleetEvalReport) -> String {
    report
        .benchmark_tasks
        .iter()
        .map(|task| {
            format!(
                r#"{{ "task_id": {}, "class": {}, "complexity_tier": {}, "engineering_facets": {}, "allowed_scope": {}, "expected_check": {}, "evaluation_oracle": {}, "human_timebox_minutes": {}, "contamination_policy": {}, "timeout_ms": {}, "acceptance": {} }}"#,
                json_string_literal(task.task_id),
                json_string_literal(task.class),
                json_string_literal(task.complexity_tier),
                json_string_literal(task.engineering_facets),
                json_string_literal(task.allowed_scope),
                json_string_literal(task.expected_check),
                json_string_literal(task.evaluation_oracle),
                task.human_timebox_minutes,
                json_string_literal(task.contamination_policy),
                task.timeout_ms,
                json_string_literal(task.acceptance)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Clone, Debug)]
struct TaskClassEvidenceScore {
    task_class: String,
    runner_id: String,
    display_name: &'static str,
    declared_task_count: u32,
    accepted_count: u32,
    scoreboard_sample_size: u32,
    same_fixture_comparison_count: u32,
    confidence: &'static str,
    pass_rate_pct: u32,
    cost_per_accepted_cents: u32,
    latency_per_accepted_ms: u64,
    review_burden_score: u32,
    diff_size_score: u32,
    rerun_rate_pct: u32,
    normalized_metric_status: &'static str,
    evidence_basis: &'static str,
}

fn task_class_scorecards_json(report: &ForgeFleetEvalReport, kernel: Option<&MdxKernel>) -> String {
    let runners = forge_fleet_runner_profiles();
    let mut classes = report
        .benchmark_tasks
        .iter()
        .map(|task| task.class)
        .collect::<Vec<_>>();
    classes.sort_unstable();
    classes.dedup();
    classes
        .iter()
        .flat_map(|task_class| {
            runners.iter().map(move |runner| {
                task_class_evidence_score(report, kernel, task_class, runner)
            })
        })
        .map(|score| {
            format!(
                r#"{{"task_class":{},"runner_id":{},"display_name":{},"declared_task_count":{},"accepted_count":{},"scoreboard_sample_size":{},"same_fixture_comparison_count":{},"confidence":{},"pass_rate_pct":{},"cost_per_accepted_cents":{},"latency_per_accepted_ms":{},"review_burden_score":{},"diff_size_score":{},"rerun_rate_pct":{},"normalized_metric_status":{},"evidence_basis":{}}}"#,
                json_string_literal(&score.task_class),
                json_string_literal(&score.runner_id),
                json_string_literal(score.display_name),
                score.declared_task_count,
                score.accepted_count,
                score.scoreboard_sample_size,
                score.same_fixture_comparison_count,
                json_string_literal(score.confidence),
                score.pass_rate_pct,
                score.cost_per_accepted_cents,
                score.latency_per_accepted_ms,
                score.review_burden_score,
                score.diff_size_score,
                score.rerun_rate_pct,
                json_string_literal(score.normalized_metric_status),
                json_string_literal(score.evidence_basis)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn task_class_evidence_score(
    report: &ForgeFleetEvalReport,
    kernel: Option<&MdxKernel>,
    task_class: &str,
    runner: &FleetRunnerProfile,
) -> TaskClassEvidenceScore {
    let declared_task_count = report
        .benchmark_tasks
        .iter()
        .filter(|task| task.class == task_class)
        .count() as u32;
    let seeded_accepted = if runner.runner_kind == "mdx_native" {
        declared_task_count
    } else {
        0
    };
    let live_accepted = kernel
        .map(|kernel| {
            accepted_score_count_for_runner_and_task_class(kernel, runner.runner_id, task_class)
        })
        .unwrap_or(0);
    let accepted_count = seeded_accepted + live_accepted;
    let same_fixture_comparison_count = kernel
        .map(|kernel| accepted_same_fixture_comparison_count(kernel, runner.runner_id, task_class))
        .unwrap_or(0);
    let scoreboard_sample_size = accepted_count;
    let pass_rate_pct = accepted_count
        .saturating_mul(100)
        .checked_div(declared_task_count.max(1))
        .unwrap_or(0)
        .min(100);
    let confidence = scorecard_confidence(scoreboard_sample_size, same_fixture_comparison_count);
    let normalized_metric_status = if confidence == "insufficient_sample" {
        "insufficient_sample"
    } else {
        "normalized"
    };
    let runner_score = report
        .runner_scores
        .iter()
        .find(|score| score.runner_id == runner.runner_id);
    let cost_cents = runner_score.map(|score| score.cost_cents).unwrap_or(0);
    let mean_runtime_ms = runner_score.map(|score| score.mean_runtime_ms).unwrap_or(0);
    let cost_per_accepted_cents = cost_cents.checked_div(accepted_count.max(1)).unwrap_or(0);
    let latency_per_accepted_ms = if accepted_count > 0 {
        mean_runtime_ms
    } else {
        0
    };
    let review_burden_score = if accepted_count > 0 && runner.runner_kind == "mdx_native" {
        95
    } else if accepted_count > 0 {
        80
    } else {
        0
    };
    let diff_size_score = if accepted_count > 0 { 85 } else { 0 };
    TaskClassEvidenceScore {
        task_class: task_class.to_string(),
        runner_id: runner.runner_id.to_string(),
        display_name: runner.display_name,
        declared_task_count,
        accepted_count,
        scoreboard_sample_size,
        same_fixture_comparison_count,
        confidence,
        pass_rate_pct,
        cost_per_accepted_cents,
        latency_per_accepted_ms,
        review_burden_score,
        diff_size_score,
        rerun_rate_pct: 0,
        normalized_metric_status,
        evidence_basis: if runner.runner_kind == "mdx_native" {
            "seeded_native_baseline_plus_accepted_receipts"
        } else {
            "accepted_receipts_only"
        },
    }
}

fn scorecard_confidence(
    scoreboard_sample_size: u32,
    same_fixture_comparison_count: u32,
) -> &'static str {
    if scoreboard_sample_size >= 10 && same_fixture_comparison_count >= 3 {
        "reliable"
    } else if scoreboard_sample_size >= 3 || same_fixture_comparison_count >= 1 {
        "directional"
    } else {
        "insufficient_sample"
    }
}

fn runner_scores_json(report: &ForgeFleetEvalReport, kernel: Option<&MdxKernel>) -> String {
    report
        .runner_scores
        .iter()
        .map(|score| {
            let runners = forge_fleet_runner_profiles();
            let runner = runner_profile(&score.runner_id, &runners);
            let runtime = runner_runtime_scorecard(kernel, runner);
            let live_accepted = kernel
                .map(|kernel| accepted_score_count_for_runner(kernel, &score.runner_id))
                .unwrap_or(0);
            let live_trials = kernel
                .map(|kernel| league_trial_count_for_runner(kernel, &score.runner_id))
                .unwrap_or(0);
            // Attempts come from trial receipts; acceptances from accepted
            // receipts. A run can be accepted through paths that never
            // minted a trial marker, so attempts never undercount passes.
            let tasks_attempted = live_trials.max(live_accepted);
            let accepted = live_accepted;
            let pass_rate_pct = accepted
                .saturating_mul(100)
                .checked_div(tasks_attempted)
                .unwrap_or(0);
            let scoreboard_eligible = tasks_attempted >= scoreboard_min_trials();
            format!(
                r#"{{ "runner_id": {}, "runner_kind": {}, "display_name": {}, "adapter_kind": {}, "model_provider": {}, "model": {}, "wire_api": {}, "model_profile_id": {}, "execution_mode": {}, "invocation_mode": {}, "version_requirement": {}, "isolation_policy": {}, "transcript_policy": {}, "output_policy": {}, "evidence_policy": {}, "execution_status": {}, "visibility_tier": {}, "tasks_attempted": {}, "accepted": {}, "scoreboard_eligible": {}, "min_trials_required": {}, "quarantined": {}, "blocked": {}, "failed_quality_gates": {}, "tokens_in": {}, "tokens_out": {}, "cost_cents": {}, "mean_runtime_ms": {}, "authority_violations_blocked": {}, "pass_rate_pct": {}, "runtime_fingerprint_id": {}, "runtime_version_observed": {}, "runtime_drift_status": {}, "runtime_compatibility_status": {}, "runtime_smoke_status": {}, "runtime_smoke_passed": {}, "runtime_last_observed_receipt_id": {} }}"#,
                json_string_literal(&score.runner_id),
                json_string_literal(&score.runner_kind),
                json_string_literal(runner.display_name),
                json_string_literal(runner.adapter_kind),
                json_string_literal(&score.model_provider),
                json_string_literal(&score.model),
                json_string_literal(&score.wire_api),
                json_string_literal(runner.model_profile_id),
                json_string_literal(runner.execution_mode),
                json_string_literal(runner.invocation_mode),
                json_string_literal(runner.version_requirement),
                json_string_literal(runner.isolation_policy),
                json_string_literal(runner.transcript_policy),
                json_string_literal(runner.output_policy),
                json_string_literal(runner.evidence_policy),
                json_string_literal(runner.execution_status),
                json_string_literal(runner.visibility_tier),
                tasks_attempted,
                accepted,
                scoreboard_eligible,
                scoreboard_min_trials(),
                score.quarantined,
                score.blocked,
                score.failed_quality_gates,
                score.tokens_in,
                score.tokens_out,
                score.cost_cents,
                score.mean_runtime_ms,
                score.authority_violations_blocked,
                pass_rate_pct,
                json_string_literal(&runtime.fingerprint_id),
                json_string_literal(&runtime.version_observed),
                json_string_literal(&runtime.drift_status),
                json_string_literal(&runtime.compatibility_status),
                json_string_literal(&runtime.smoke_status),
                runtime.smoke_passed,
                json_string_literal(&runtime.last_observed_receipt_id)
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

struct RunnerRuntimeScorecard {
    fingerprint_id: String,
    version_observed: String,
    drift_status: String,
    compatibility_status: String,
    smoke_status: String,
    smoke_passed: bool,
    last_observed_receipt_id: String,
}

fn runner_runtime_scorecard(
    kernel: Option<&MdxKernel>,
    runner: &FleetRunnerProfile,
) -> RunnerRuntimeScorecard {
    if runner.runner_kind == "mdx_native" {
        return RunnerRuntimeScorecard {
            fingerprint_id: "native_runtime".to_string(),
            version_observed: "built_in".to_string(),
            drift_status: "runtime_native".to_string(),
            compatibility_status: "native_runtime".to_string(),
            smoke_status: "runtime_smoke_not_required".to_string(),
            smoke_passed: true,
            last_observed_receipt_id: String::new(),
        };
    }
    let Some(kernel) = kernel else {
        return RunnerRuntimeScorecard {
            fingerprint_id: String::new(),
            version_observed: String::new(),
            drift_status: "runtime_not_observed".to_string(),
            compatibility_status: "runtime_not_observed".to_string(),
            smoke_status: "runtime_smoke_not_run".to_string(),
            smoke_passed: false,
            last_observed_receipt_id: String::new(),
        };
    };
    let mut scorecard = RunnerRuntimeScorecard {
        fingerprint_id: String::new(),
        version_observed: String::new(),
        drift_status: "runtime_not_observed".to_string(),
        compatibility_status: "runtime_not_observed".to_string(),
        smoke_status: "runtime_smoke_not_run".to_string(),
        smoke_passed: false,
        last_observed_receipt_id: String::new(),
    };
    let mut smoke_fingerprint_id = String::new();
    let mut smoke_version_observed = String::new();
    let mut smoke_drift_status = String::new();
    let mut smoke_compatibility_status = String::new();
    let mut smoke_receipt_id = String::new();
    for receipt in kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .rev()
    {
        if payload_value(receipt, "machine_runtime_runner_id") != runner.runner_id {
            continue;
        }
        let receipt_is_smoke = !payload_value(receipt, "machine_runtime_smoke_status").is_empty();
        if receipt_is_smoke && scorecard.smoke_status == "runtime_smoke_not_run" {
            scorecard.smoke_status =
                payload_value(receipt, "machine_runtime_smoke_status").to_string();
            scorecard.smoke_passed =
                payload_value(receipt, "machine_runtime_smoke_passed") == "true";
            smoke_fingerprint_id =
                payload_value(receipt, "machine_runtime_fingerprint_id").to_string();
            smoke_version_observed =
                payload_value(receipt, "machine_runtime_version_observed").to_string();
            smoke_drift_status = payload_value(receipt, "machine_runtime_drift_status").to_string();
            smoke_compatibility_status =
                payload_value(receipt, "machine_runtime_compatibility_status").to_string();
            smoke_receipt_id = receipt.receipt_id.clone();
        }
        if scorecard.fingerprint_id.is_empty()
            && !receipt_is_smoke
            && !payload_value(receipt, "machine_runtime_fingerprint_id").is_empty()
        {
            scorecard.fingerprint_id =
                payload_value(receipt, "machine_runtime_fingerprint_id").to_string();
            scorecard.version_observed =
                payload_value(receipt, "machine_runtime_version_observed").to_string();
            scorecard.drift_status =
                payload_value(receipt, "machine_runtime_drift_status").to_string();
            scorecard.compatibility_status =
                payload_value(receipt, "machine_runtime_compatibility_status").to_string();
            scorecard.last_observed_receipt_id = receipt.receipt_id.clone();
        }
        if !scorecard.fingerprint_id.is_empty() && scorecard.smoke_status != "runtime_smoke_not_run"
        {
            break;
        }
    }
    if scorecard.fingerprint_id.is_empty() && !smoke_fingerprint_id.is_empty() {
        scorecard.fingerprint_id = smoke_fingerprint_id;
        scorecard.version_observed = smoke_version_observed;
        scorecard.drift_status = smoke_drift_status;
        scorecard.compatibility_status = smoke_compatibility_status;
        scorecard.last_observed_receipt_id = smoke_receipt_id;
    }
    if scorecard.drift_status.trim().is_empty() {
        scorecard.drift_status = "runtime_not_observed".to_string();
    }
    if scorecard.compatibility_status.trim().is_empty() {
        scorecard.compatibility_status = "runtime_not_observed".to_string();
    }
    scorecard
}

fn runner_profile_json(runner: &FleetRunnerProfile) -> String {
    format!(
        r#"{{"runner_id":{},"runner_kind":{},"display_name":{},"adapter_kind":{},"model_provider":{},"model":{},"wire_api":{},"model_profile_id":{},"execution_mode":{},"invocation_mode":{},"version_requirement":{},"isolation_policy":{},"transcript_policy":{},"output_policy":{},"evidence_policy":{},"execution_status":{},"visibility_tier":{},"adapter_execution_allowed":false,"production_write_allowed":false}}"#,
        json_string_literal(runner.runner_id),
        json_string_literal(runner.runner_kind),
        json_string_literal(runner.display_name),
        json_string_literal(runner.adapter_kind),
        json_string_literal(runner.model_provider),
        json_string_literal(runner.model),
        json_string_literal(runner.wire_api),
        json_string_literal(runner.model_profile_id),
        json_string_literal(runner.execution_mode),
        json_string_literal(runner.invocation_mode),
        json_string_literal(runner.version_requirement),
        json_string_literal(runner.isolation_policy),
        json_string_literal(runner.transcript_policy),
        json_string_literal(runner.output_policy),
        json_string_literal(runner.evidence_policy),
        json_string_literal(runner.execution_status),
        json_string_literal(runner.visibility_tier),
    )
}

fn runner_profile<'a>(
    runner_id: &str,
    runners: &'a [FleetRunnerProfile],
) -> &'a FleetRunnerProfile {
    runners
        .iter()
        .find(|runner| runner.runner_id == runner_id)
        .or_else(|| runners.first())
        .expect("fleet runner catalog is non-empty")
}

// One league trial = one run_started receipt carrying the trial marker.
// Trials count every attempt the runner actually made, accepted or not,
// so a pass rate is passes over attempts instead of passes over passes.
fn league_trial_count_for_runner(kernel: &MdxKernel, runner_id: &str) -> u32 {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| {
            payload_value(receipt, "machine_league_trial") == "true"
                && payload_value(receipt, "event") == "run_started"
                && payload_value(receipt, "runner_profile_runner_id") == runner_id
                // A scripted rehearsal is never a league trial.
                && payload_value(receipt, "native_execution_mode") != "scripted_baseline"
        })
        .count() as u32
}

// A runner earns scoreboard standing only after enough independent trials
// to mean something. Below the floor a perfect pass rate is one lucky run.
fn scoreboard_min_trials() -> u32 {
    std::env::var("MDX_FLEET_EVAL_MIN_TRIALS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(3)
}

fn accepted_score_count_for_runner(kernel: &MdxKernel, runner_id: &str) -> u32 {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| {
            payload_value(receipt, "accepted_for_scoreboard") == "true"
                && payload_value(receipt, "native_execution_mode") != "scripted_baseline"
                && (payload_value(receipt, "runner_id") == runner_id
                    || payload_value(receipt, "winning_runner_id") == runner_id)
        })
        .count() as u32
}

// After-counts only exist relative to a recorded builder loop tick: count
// accepted-score receipts stamped strictly after the tick receipt's trusted
// timestamp. No tick receipt means no after-measurement (None, rendered null).
//
// Seam: this is the ledger before/after binding half of the re-measure fold -
// once a same-fixture re-eval exists, its accepted-score receipts land after
// the tick and this function reports the honest after-count. Actually firing
// the same-fixture re-eval trigger is the Tier 3b lane (docs/FIXTURE-GRADUATION-SEAM.md,
// docs/FORGE-MACHINE-LEAGUE-TIER-STATUS.md) and is owned by Codex; do not build
// it here. Until it fires, after-counts stay null rather than fabricated.
fn accepted_score_count_for_runner_after_tick(
    kernel: &MdxKernel,
    runner_id: &str,
    eval_fixture: Option<&str>,
    tick_receipt_id: &str,
) -> Option<u32> {
    if tick_receipt_id.trim().is_empty() {
        return None;
    }
    let tick_timestamp = kernel
        .ledger()
        .query()
        .by_id(tick_receipt_id)?
        .receipt_timestamp
        .clone();
    Some(
        kernel
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .iter()
            .filter(|receipt| {
                receipt.receipt_timestamp > tick_timestamp
                    && payload_value(receipt, "accepted_for_scoreboard") == "true"
                    && payload_value(receipt, "native_execution_mode") != "scripted_baseline"
                    && eval_fixture
                        .map(|fixture| {
                            payload_value(receipt, "machine_league_eval_fixture") == fixture
                        })
                        .unwrap_or(true)
                    && (payload_value(receipt, "runner_id") == runner_id
                        || payload_value(receipt, "winning_runner_id") == runner_id)
            })
            .count() as u32,
    )
}

fn accepted_score_count_for_runner_and_fixture(
    kernel: &MdxKernel,
    runner_id: &str,
    eval_fixture: &str,
) -> u32 {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| {
            payload_value(receipt, "accepted_for_scoreboard") == "true"
                && payload_value(receipt, "native_execution_mode") != "scripted_baseline"
                && payload_value(receipt, "machine_league_eval_fixture") == eval_fixture
                && (payload_value(receipt, "runner_id") == runner_id
                    || payload_value(receipt, "winning_runner_id") == runner_id)
        })
        .count() as u32
}

fn failed_native_attempt_count_for_fixture(kernel: &MdxKernel, eval_fixture: &str) -> u32 {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| {
            payload_value(receipt, "machine_league_eval_fixture") == eval_fixture
                && (payload_value(receipt, "runner_id") == "mdx_native_harness_runner"
                    || payload_value(receipt, "runner_profile_runner_id")
                        == "mdx_native_harness_runner")
                && payload_value(receipt, "eval_oracle_status")
                    != "visible_and_hidden_checks_passed"
                && !payload_value(receipt, "eval_oracle_status").is_empty()
        })
        .map(|receipt| payload_value(receipt, "run_id").to_string())
        .collect::<std::collections::BTreeSet<_>>()
        .len() as u32
}

fn accepted_score_count_for_runner_and_task_class(
    kernel: &MdxKernel,
    runner_id: &str,
    task_class: &str,
) -> u32 {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| {
            payload_value(receipt, "accepted_for_scoreboard") == "true"
                && payload_value(receipt, "native_execution_mode") != "scripted_baseline"
                && payload_value(receipt, "language_task_class") == task_class
                && (payload_value(receipt, "runner_id") == runner_id
                    || payload_value(receipt, "winning_runner_id") == runner_id)
        })
        .count() as u32
}

fn accepted_same_fixture_comparison_count(
    kernel: &MdxKernel,
    runner_id: &str,
    task_class: &str,
) -> u32 {
    if runner_id == "mdx_native_harness_runner" {
        return 0;
    }
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| {
            let fixture = payload_value(receipt, "machine_league_eval_fixture");
            !fixture.is_empty()
                && payload_value(receipt, "accepted_for_scoreboard") == "true"
                && payload_value(receipt, "language_task_class") == task_class
                && (payload_value(receipt, "runner_id") == runner_id
                    || payload_value(receipt, "winning_runner_id") == runner_id)
                && accepted_score_count_for_runner_and_fixture(
                    kernel,
                    "mdx_native_harness_runner",
                    fixture,
                ) > 0
        })
        .count() as u32
}

fn recommendation_rationale(selected: &FleetRunnerProfile, native_evidence_count: u32) -> String {
    if selected.runner_kind != "mdx_native" {
        format!(
            "Forge recommends {} from accepted machine-league evidence and will hold the output until MDx gates pass.",
            selected.display_name
        )
    } else if native_evidence_count > 0 {
        "Forge recommends its native builder from accepted evidence and will keep gathering external-runner comparisons.".to_string()
    } else {
        "Forge has not measured this matchup with accepted evidence yet, so it starts with the native builder and can stage quarantined external-machine trials.".to_string()
    }
}

fn binary_present(binary_name: &str) -> bool {
    if binary_name.trim().is_empty() {
        return false;
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|path| path.join(binary_name).is_file())
}

fn valid_distillation_reason(reason: &str) -> bool {
    matches!(
        reason,
        "model_capability"
            | "context_strategy"
            | "planning_strategy"
            | "tool_strategy"
            | "review_strategy"
            | "ux_strategy"
            | "parallel_strategy"
            | "artifact_strategy"
    )
}

fn native_improvement_work_item_for_reason(reason: &str) -> String {
    format!("mdx_native_harness_improve_{}", reason.replace('-', "_"))
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{key}\"");
    let rest = body.split(&pattern).nth(1)?;
    let after_colon = rest.split_once(':')?.1.trim_start();
    let after_quote = after_colon.strip_prefix('"')?;
    let value = after_quote.split('"').next()?;
    Some(value.to_string())
}

fn json_bool_field(body: &str, key: &str) -> Option<bool> {
    let pattern = format!("\"{key}\"");
    let rest = body.split(&pattern).nth(1)?;
    let after_colon = rest.split_once(':')?.1.trim_start();
    if after_colon.starts_with("true") {
        Some(true)
    } else if after_colon.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn json_u32_field(body: &str, key: &str) -> Option<u32> {
    let pattern = format!("\"{key}\"");
    let rest = body.split(&pattern).nth(1)?;
    let after_colon = rest.split_once(':')?.1.trim_start();
    let value = after_colon
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    value.parse::<u32>().ok()
}

struct ProviderCredentialRequirement {
    provider_family: &'static str,
    profile_id: &'static str,
    required_env_keys: &'static [&'static str],
    credentials_present: bool,
}

impl ProviderCredentialRequirement {
    fn to_json(&self) -> String {
        format!(
            r#"{{"provider_family":{},"profile_id":{},"required_env_keys":[{}],"credentials_present":{},"secret_value_exposed":false}}"#,
            json_string_literal(self.provider_family),
            json_string_literal(self.profile_id),
            self.required_env_keys
                .iter()
                .map(|key| json_string_literal(key))
                .collect::<Vec<_>>()
                .join(","),
            self.credentials_present
        )
    }
}

fn provider_credential_requirements(
    secret_store: &SecretStore,
    tenant_id: &str,
) -> Vec<ProviderCredentialRequirement> {
    [
        (
            "openai",
            "codex_openai_cli_profile",
            &["OPENAI_API_KEY"][..],
        ),
        (
            "gemini",
            "codex_gemini_responses_profile",
            &["GEMINI_API_KEY"][..],
        ),
        (
            "anthropic",
            "codex_anthropic_responses_profile",
            &["ANTHROPIC_API_KEY"][..],
        ),
        (
            "anthropic",
            "claude_code_model_profile",
            &["ANTHROPIC_API_KEY"][..],
        ),
        ("xai", "codex_xai_responses_profile", &["XAI_API_KEY"][..]),
        ("xai", "xai_grok_build_model_profile", &["XAI_API_KEY"][..]),
        (
            "moonshot",
            "kimi_code_k3_model_profile",
            &["MOONSHOT_API_KEY"][..],
        ),
        (
            "aws_bedrock",
            "codex_bedrock_responses_profile",
            &["AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_REGION"][..],
        ),
    ]
    .into_iter()
    .map(
        |(provider_family, profile_id, required_env_keys)| ProviderCredentialRequirement {
            provider_family,
            profile_id,
            required_env_keys,
            credentials_present: required_env_keys
                .iter()
                .all(|key| secret_store.has_provider_key_for_tenant(tenant_id, key)),
        },
    )
    .collect()
}

fn payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::ForgeRunEvent;
    use std::sync::{Arc, MutexGuard, RwLock};

    struct MachineOptionProviderKeyGuard {
        _lock: MutexGuard<'static, ()>,
        tenant_id: String,
    }

    impl Drop for MachineOptionProviderKeyGuard {
        fn drop(&mut self) {
            clear_machine_option_provider_keys(&self.tenant_id);
        }
    }

    fn clear_machine_option_provider_keys(tenant_id: &str) {
        for key in ["XAI_API_KEY", "ANTHROPIC_API_KEY", "MOONSHOT_API_KEY"] {
            global().remove_for_tenant(tenant_id, key);
            global().disable_keychain_lookup_for_tenant(tenant_id, key);
            unsafe {
                std::env::remove_var(key);
            }
        }
    }

    fn isolate_machine_option_provider_keys() -> MachineOptionProviderKeyGuard {
        isolate_machine_option_provider_keys_for_tenant("local_tenant")
    }

    fn isolate_machine_option_provider_keys_for_tenant(
        tenant_id: &str,
    ) -> MachineOptionProviderKeyGuard {
        let lock = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        clear_machine_option_provider_keys(tenant_id);
        MachineOptionProviderKeyGuard {
            _lock: lock,
            tenant_id: tenant_id.to_string(),
        }
    }

    #[test]
    fn machine_option_provider_key_guard_cleans_local_state() {
        {
            let _guard = isolate_machine_option_provider_keys();
            global().set_for_tenant("local_tenant", "ANTHROPIC_API_KEY", "test-key");
            assert!(global().has_provider_key_for_tenant("local_tenant", "ANTHROPIC_API_KEY"));
        }
        let _lock = crate::forge_turn_client::ENV_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        assert!(!global().has_provider_key_for_tenant("local_tenant", "ANTHROPIC_API_KEY"));
    }

    #[test]
    fn grok_build_adapter_uses_tenant_key_pinned_model_and_isolated_state() {
        assert_eq!(
            external_machine_provider_key("grok_build_cli"),
            Some("XAI_API_KEY")
        );
        assert!(GROK_BUILD_EXEC_POLICY_COMMAND.contains("--model <approved-model>"));
        assert!(GROK_BUILD_EXEC_POLICY_COMMAND.contains("isolated per-run GROK_HOME"));

        let artifact_dir = Path::new("/tmp/mdx-grok-artifacts");
        assert_eq!(
            external_adapter_state_write_paths("grok_build_cli", artifact_dir),
            vec![artifact_dir.join("grok-home")]
        );
    }

    #[test]
    fn kimi_code_adapter_uses_tenant_key_pinned_model_and_isolated_state() {
        assert_eq!(
            external_machine_provider_key("kimi_code"),
            Some("MOONSHOT_API_KEY")
        );
        assert!(KIMI_CODE_EXEC_POLICY_COMMAND.contains("--model kimi-k3"));
        assert!(KIMI_CODE_EXEC_POLICY_COMMAND.contains("--output-format stream-json"));
        assert!(KIMI_CODE_EXEC_POLICY_COMMAND.contains("isolated per-run KIMI_CODE_HOME"));

        let artifact_dir = Path::new("/tmp/mdx-kimi-artifacts");
        assert_eq!(
            external_adapter_state_write_paths("kimi_code", artifact_dir),
            vec![artifact_dir.join("kimi-code-home")]
        );
    }

    #[test]
    fn dry_run_projection_reports_full_language_provider_matrix_coverage() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .run_forge_fleet_eval_dry_run_local("dry_run_projection_coverage")
            .expect("dry run should stage language-provider matrix");

        let projection = render_dry_run_projection_json(&kernel);

        assert!(projection.contains(r#""score_count":420"#));
        assert!(projection.contains(r#""expected_language_task_count":30"#));
        assert!(projection.contains(r#""language_task_count":30"#));
        assert!(projection.contains(r#""expected_model_profile_count":14"#));
        assert!(projection.contains(r#""model_profile_count":14"#));
        assert!(projection.contains(r#""expected_score_count":420"#));
        assert!(projection.contains(r#""full_matrix_staged":true"#));
        assert!(projection.contains(r#""provider_family_count":10"#));
        assert!(projection.contains(r#""language_pack_count":10"#));
    }

    #[test]
    fn machine_league_recommendations_keep_forge_as_the_product_subject() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = route_response(
            "GET",
            "/forge/machine-league/recommendations.json",
            "",
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            response
                .body
                .contains(r#""status":"MACHINE_LEAGUE_RECOMMENDATION_GATHERING_EVIDENCE""#)
        );
        assert!(
            response
                .body
                .contains(r#""selected_runner_id":"mdx_native_harness_runner""#)
        );
        assert!(
            response
                .body
                .contains(r#""fallback_runner_id":"codex_cli_external_worker""#)
        );
        assert!(
            response
                .body
                .contains(r#""visibility_tier":"product_recommendation""#)
        );
        assert!(
            response
                .body
                .contains(r#""raw_scoreboard_is_front_door":false"#)
        );
        assert!(
            response
                .body
                .contains(r#""trial_route":"/forge/machine-league/codex-trials.json""#)
        );
        assert!(response.body.contains(
            r#""grok_build_cli_external_worker":"/forge/machine-league/grok-build-trials.json""#
        ));
        assert!(response.body.contains(
            r#""claude_code_external_worker":"/forge/machine-league/claude-code-trials.json""#
        ));
        assert!(response.body.contains(
            r#""gemini_cli_external_worker":"/forge/machine-league/gemini-trials.json""#
        ));
        assert!(response.body.contains(
            r#""opencode_external_worker":"/forge/machine-league/opencode-trials.json""#
        ));
        assert!(
            response
                .body
                .contains(r#""cline_external_worker":"/forge/machine-league/cline-trials.json""#)
        );
        assert!(
            response
                .body
                .contains(r#""goose_external_worker":"/forge/machine-league/goose-trials.json""#)
        );
        assert!(response.body.contains(r#""display_name":"Codex CLI""#));
        assert!(response.body.contains(r#""display_name":"Gemini CLI""#));
        assert!(response.body.contains(r#""display_name":"Grok Build""#));
        assert!(
            response
                .body
                .contains(r#""display_name":"Claude Agent SDK""#)
        );
        assert!(response.body.contains(r#""display_name":"OpenCode""#));
        assert!(response.body.contains(r#""display_name":"Cline""#));
        assert!(response.body.contains(r#""display_name":"Goose""#));
        assert!(
            response
                .body
                .contains(r#""external_output_consumable":false"#)
        );
    }

    #[test]
    fn machine_league_preflight_reports_runtime_fingerprints_without_execution() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = route_response("GET", "/forge/machine-league/preflight.json", "", &kernel)
            .expect("route handled")
            .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("preflight json");
        let checks = parsed["checks"].as_array().expect("checks array");
        let opencode = checks
            .iter()
            .find(|check| check["runner_id"] == "opencode_external_worker")
            .expect("opencode preflight");

        assert_eq!(
            opencode["runtime_fingerprint"]["runner_id"].as_str(),
            Some("opencode_external_worker")
        );
        assert_eq!(
            opencode["runtime_fingerprint"]["adapter_contract_version"].as_str(),
            Some(MACHINE_RUNTIME_ADAPTER_CONTRACT_VERSION)
        );
        assert_eq!(
            opencode["runtime_fingerprint"]["command_contract"].as_str(),
            Some(OPENCODE_EXEC_POLICY_COMMAND)
        );
        assert!(
            opencode["runtime_fingerprint"]["fingerprint_id"]
                .as_str()
                .unwrap_or("")
                .starts_with("sha256:")
        );
        assert_eq!(opencode["adapter_execution_allowed"].as_bool(), Some(false));
        assert_eq!(opencode["task_execution_allowed"].as_bool(), Some(false));
    }

    #[test]
    fn fleet_scoreboard_reports_runner_model_compatibility_matrix() {
        let _provider_key_guard = isolate_machine_option_provider_keys();
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = route_response("GET", "/forge/fleet-eval-scoreboard.json", "", &kernel)
            .expect("route handled")
            .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("scoreboard json");
        let matrix = parsed["runner_model_compatibility_matrix"]
            .as_array()
            .expect("compatibility matrix");
        let model_matrix = parsed["model_matrix"].as_array().expect("model matrix");
        let adapter_tiers = parsed["adapter_protocol_tiers"]
            .as_array()
            .expect("adapter tiers");

        assert_eq!(matrix.len(), 126);
        assert!(adapter_tiers.iter().any(|tier| {
            tier["adapter_protocol_tier"] == "sdk_adapter"
                && tier["authority_status"] == "clearance_required"
        }));
        assert!(adapter_tiers.iter().any(|tier| {
            tier["adapter_protocol_tier"] == "protocol_adapter"
                && tier["authority_status"] == "provider_key_and_runtime_preflight_required"
        }));
        assert!(matrix.iter().any(|row| {
            row["runner_id"] == "codex_cli_external_worker"
                && row["model_profile_id"] == "codex_xai_responses_profile"
                && row["model_display_name"] == "Grok 4.5"
                && row["compatibility_status"] == "compatible_via_shared_wire_api"
                && row["castable"] == true
        }));
        assert!(model_matrix.iter().any(|row| {
            row["profile_id"] == "codex_gemini_responses_profile"
                && row["model_id"] == "gemini-3.1-pro-preview-via-mdx"
                && row["model_display_name"] == "Gemini 3.1 Pro Preview"
        }));
        assert!(matrix.iter().any(|row| {
            row["runner_id"] == "gemini_cli_external_worker"
                && row["model_profile_id"] == "gemini_cli_model_profile"
                && row["model_display_name"] == "Gemini CLI selected model"
                && row["compatibility_status"] == "compatible_runner_default_model"
                && row["castable"] == true
        }));
        assert!(matrix.iter().any(|row| {
            row["runner_id"] == "grok_build_cli_external_worker"
                && row["model_profile_id"] == "xai_grok_build_model_profile"
                && row["compatibility_status"] == "blocked_pending_xai_key"
                && row["closed_runner_clearance_status"] == "clearance_not_required"
                && row["adapter_protocol_tier"] == "declared_only"
                && row["machine_option_enabled_by_provider_key"] == false
                && row["castable"] == false
        }));
        assert!(matrix.iter().any(|row| {
            row["runner_id"] == "kimi_code_external_worker"
                && row["model_profile_id"] == "kimi_code_k3_model_profile"
                && row["compatibility_status"] == "blocked_pending_moonshot_key"
                && row["closed_runner_clearance_status"] == "clearance_required"
                && row["machine_option_enabled_by_provider_key"] == false
                && row["castable"] == false
        }));
    }

    #[test]
    fn provider_keys_enable_grok_build_claude_and_kimi_machine_options_without_execution() {
        let _provider_key_guard = isolate_machine_option_provider_keys();
        let tenant_id = "machine_option_key_test_tenant";
        let _identity =
            crate::request_security::set_verified_identity(Some(mdx_core::AdmittedIdentity {
                deployment_mode: "local-secure",
                tenant_id: tenant_id.to_string(),
                actor_id: "human:machine_option_key_test".to_string(),
                actor_role: "owner".to_string(),
                actor_kind: "human".to_string(),
                subject_actor_id: "human:machine_option_key_test".to_string(),
                authority_scope: Vec::new(),
                delegation_id: None,
                identity_source: "trusted_session",
                production_write_allowed: false,
            }));
        global().clear_tenant(tenant_id);
        global().disable_keychain_lookup_for_tenant(tenant_id, "XAI_API_KEY");
        global().disable_keychain_lookup_for_tenant(tenant_id, "ANTHROPIC_API_KEY");
        global().disable_keychain_lookup_for_tenant(tenant_id, "MOONSHOT_API_KEY");
        global().set_for_tenant(tenant_id, "XAI_API_KEY", "xai-test-key");
        global().set_for_tenant(tenant_id, "ANTHROPIC_API_KEY", "anthropic-test-key");
        global().set_for_tenant(tenant_id, "MOONSHOT_API_KEY", "moonshot-test-key");
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let preflight = route_response(
            "GET",
            "/forge/fleet-eval-provider-preflight.json",
            "",
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let preflight_json: serde_json::Value =
            serde_json::from_str(&preflight.body).expect("provider preflight json");
        let requirements = preflight_json["requirements"]
            .as_array()
            .expect("requirements");
        for profile_id in [
            "xai_grok_build_model_profile",
            "claude_code_model_profile",
            "kimi_code_k3_model_profile",
        ] {
            assert!(
                requirements.iter().any(|requirement| {
                    requirement["profile_id"] == profile_id
                        && requirement["credentials_present"] == true
                        && requirement["secret_value_exposed"] == false
                }),
                "missing enabled provider requirement {profile_id}"
            );
        }

        let response = route_response("GET", "/forge/fleet-eval-scoreboard.json", "", &kernel)
            .expect("route handled")
            .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("scoreboard json");
        let matrix = parsed["runner_model_compatibility_matrix"]
            .as_array()
            .expect("compatibility matrix");
        assert!(matrix.iter().any(|row| {
            row["runner_id"] == "grok_build_cli_external_worker"
                && row["model_profile_id"] == "xai_grok_build_model_profile"
                && row["compatibility_status"] == "compatible_grok_build_acp_key_present"
                && row["closed_runner_clearance_status"] == "key_present_option_enabled"
                && row["adapter_protocol_tier"] == "protocol_adapter"
                && row["machine_option_enabled_by_provider_key"] == true
                && row["adapter_execution_allowed"] == false
                && row["castable"] == true
        }));
        assert!(matrix.iter().any(|row| {
            row["runner_id"] == "claude_code_external_worker"
                && row["model_profile_id"] == "claude_code_model_profile"
                && row["compatibility_status"] == "compatible_claude_agent_sdk_key_present"
                && row["closed_runner_clearance_status"] == "key_present_option_enabled"
                && row["adapter_protocol_tier"] == "sdk_adapter"
                && row["machine_option_enabled_by_provider_key"] == true
                && row["adapter_execution_allowed"] == false
                && row["castable"] == true
        }));
        assert!(matrix.iter().any(|row| {
            row["runner_id"] == "kimi_code_external_worker"
                && row["model_profile_id"] == "kimi_code_k3_model_profile"
                && row["compatibility_status"] == "compatible_kimi_code_moonshot_key_present"
                && row["closed_runner_clearance_status"] == "key_present_option_enabled"
                && row["adapter_protocol_tier"] == "headless_cli_adapter"
                && row["machine_option_enabled_by_provider_key"] == true
                && row["adapter_execution_allowed"] == false
                && row["castable"] == true
        }));
        let kimi_trial = route_response(
            "POST",
            "/forge/machine-league/kimi-code-trials.json",
            r#"{"intent":"stage Kimi Code on the held-out tax fixture","eval_fixture":"rust_tax_rounding","execute_live":false}"#,
            &kernel,
        )
        .expect("Kimi Code route handled")
        .expect("Kimi Code packet recorded");
        assert!(
            kimi_trial
                .body
                .contains(r#""status":"KIMI_CODE_CLI_TRIAL_PACKET_RECORDED_QUARANTINED""#)
        );
        assert!(
            kimi_trial
                .body
                .contains(r#""adapter_execution_allowed":false"#)
        );
        assert!(!kimi_trial.body.contains("moonshot-test-key"));
        global().clear_tenant(tenant_id);
    }

    #[test]
    fn closed_runner_clearance_controls_scorecard_castability() {
        let tenant_id = "closed_runner_clearance_test_tenant";
        let _provider_key_guard = isolate_machine_option_provider_keys_for_tenant(tenant_id);
        let _identity =
            crate::request_security::set_verified_identity(Some(mdx_core::AdmittedIdentity {
                deployment_mode: "local-secure",
                tenant_id: tenant_id.to_string(),
                actor_id: "human:closed_runner_clearance_test".to_string(),
                actor_role: "owner".to_string(),
                actor_kind: "human".to_string(),
                subject_actor_id: "human:closed_runner_clearance_test".to_string(),
                authority_scope: Vec::new(),
                delegation_id: None,
                identity_source: "trusted_session",
                production_write_allowed: false,
            }));
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let refused_trial = route_response(
            "POST",
            "/forge/machine-league/claude-code-trials.json",
            r#"{"intent":"try Claude Code before clearance"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        assert!(refused_trial.body.contains(r#""status":"REFUSED""#));
        assert!(
            refused_trial
                .body
                .contains(r#""name":"mdx-forge-machine-league-claude-code-trial-local-post""#)
        );
        assert!(refused_trial.body.contains(
            r#""reason":"closed runner fleet-eval clearance or provider key enablement is required before this machine can stage trials""#
        ));

        let companion_clearance = route_response(
            "POST",
            "/forge/machine-league/closed-runner-clearances.json",
            r#"{"runner_id":"claude_code_external_worker","operator_attestation":true,"clearance_mode":"local_companion","adapter_protocol_tier":"sdk_adapter"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        assert!(
            companion_clearance
                .body
                .contains(r#""status":"CLOSED_RUNNER_CLEARANCE_RECORDED""#)
        );
        assert!(
            companion_clearance
                .body
                .contains(r#""scorecard_eligible":false"#)
        );

        let scoreboard = route_response("GET", "/forge/fleet-eval-scoreboard.json", "", &kernel)
            .expect("route handled")
            .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&scoreboard.body).expect("scoreboard json");
        let matrix = parsed["runner_model_compatibility_matrix"]
            .as_array()
            .expect("compatibility matrix");
        assert!(
            matrix.iter().any(|row| {
                row["runner_id"] == "claude_code_external_worker"
                    && row["model_profile_id"] == "claude_code_model_profile"
                    && row["compatibility_status"] == "cleared_for_local_companion_only"
                    && row["closed_runner_clearance_status"] == "cleared_for_local_companion"
                    && row["adapter_protocol_tier"] == "sdk_adapter"
                    && row["castable"] == false
            }),
            "compatibility matrix after companion-only clearance: {matrix:#?}"
        );

        let grok_execution_clearance = route_response(
            "POST",
            "/forge/machine-league/closed-runner-clearances.json",
            r#"{"runner_id":"grok_build_cli_external_worker","operator_attestation":true,"clearance_mode":"fleet_eval","adapter_protocol_tier":"protocol_adapter"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        assert!(
            grok_execution_clearance
                .body
                .contains(r#""status":"CLOSED_RUNNER_CLEARANCE_RECORDED""#)
        );
        assert!(
            grok_execution_clearance
                .body
                .contains(r#""clearance_basis":"open_source_execution_attestation""#)
        );
        assert!(
            grok_execution_clearance
                .body
                .contains(r#""closed_runner_terms_clearance_required":false"#)
        );
        global().set_for_tenant(tenant_id, "XAI_API_KEY", "xai-test-key");

        let scoreboard = route_response("GET", "/forge/fleet-eval-scoreboard.json", "", &kernel)
            .expect("route handled")
            .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&scoreboard.body).expect("scoreboard json");
        let matrix = parsed["runner_model_compatibility_matrix"]
            .as_array()
            .expect("compatibility matrix");
        assert!(matrix.iter().any(|row| {
            row["runner_id"] == "grok_build_cli_external_worker"
                && row["model_profile_id"] == "xai_grok_build_model_profile"
                && row["compatibility_status"] == "compatible_grok_build_acp_key_present"
                && row["closed_runner_clearance_status"]
                    == "open_source_execution_clearance_recorded"
                && row["adapter_protocol_tier"] == "protocol_adapter"
                && row["castable"] == true
        }));
        global().remove_for_tenant(tenant_id, "XAI_API_KEY");
    }

    #[test]
    fn machine_league_fleet_casts_plan_8_wide_breadth_first() {
        let _provider_key_guard = isolate_machine_option_provider_keys();
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = route_response(
            "POST",
            "/forge/machine-league/fleet-casts.json",
            r#"{"worker_count":8,"task_class":"bug_fix","language_pack_id":"rust-cargo","policy_profile":"standard_quarantined","strategy":"compete"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("fleet cast json");
        let casts = parsed["worker_casts"].as_array().expect("worker casts");

        assert_eq!(parsed["status"].as_str(), Some("FLEET_CAST_PLAN_READY"));
        assert_eq!(parsed["worker_count_planned"].as_u64(), Some(8));
        assert_eq!(parsed["unique_cast_count"].as_u64(), Some(10));
        assert_eq!(parsed["has_8_unique_casts"].as_bool(), Some(true));
        assert_eq!(parsed["can_run_8_wide"].as_bool(), Some(true));
        assert_eq!(parsed["can_run_16_wide"].as_bool(), Some(true));
        assert_eq!(parsed["can_run_24_wide"].as_bool(), Some(true));
        assert_eq!(parsed["adapter_execution_allowed"].as_bool(), Some(false));
        assert_eq!(parsed["external_output_consumable"].as_bool(), Some(false));
        for runner_id in [
            "mdx_native_harness_runner",
            "codex_cli_external_worker",
            "gemini_cli_external_worker",
            "opencode_external_worker",
            "cline_external_worker",
            "goose_external_worker",
        ] {
            assert!(
                casts.iter().any(|cast| cast["runner_id"] == runner_id),
                "missing runner {runner_id}"
            );
        }
        assert!(casts.iter().all(|cast| cast["output_quarantined"] == true));
    }

    #[test]
    fn machine_league_fleet_runs_stage_lane_packets_without_consuming_outputs() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = route_response(
            "POST",
            "/forge/machine-league/fleet-runs.json",
            r#"{"worker_count":8,"task_class":"bug_fix","language_pack_id":"rust-cargo","eval_fixture":"rust_tax_rounding","execute_live":false}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("fleet run json");

        assert_eq!(parsed["status"].as_str(), Some("FLEET_RUN_STAGED_HELD"));
        assert_eq!(parsed["worker_count_planned"].as_u64(), Some(8));
        assert_eq!(parsed["executed_lane_count"].as_u64(), Some(0));
        assert_eq!(parsed["accepted_for_scoreboard_count"].as_u64(), Some(0));
        assert_eq!(parsed["external_output_consumable"].as_bool(), Some(false));
        assert_eq!(parsed["production_write_allowed"].as_bool(), Some(false));
        let lanes = parsed["lanes"].as_array().expect("lanes");
        assert_eq!(lanes.len(), 8);
        assert!(lanes.iter().any(|lane| {
            lane["runner_id"] == "codex_cli_external_worker"
                && lane["lane_status"] == "LANE_STAGED_HELD_TRIAL_PACKET"
                && lane["execution_performed"] == false
                && lane["external_output_consumable"] == false
        }));
        assert!(lanes.iter().any(|lane| {
            lane["runner_id"] == "mdx_native_harness_runner"
                && lane["lane_status"] == "PLANNED_NATIVE_BASELINE_HELD"
        }));

        let projection = route_response(
            "GET",
            "/forge/machine-league/fleet-runs/projection.json",
            "",
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let projection_json: serde_json::Value =
            serde_json::from_str(&projection.body).expect("fleet run projection json");
        assert_eq!(
            projection_json["status"].as_str(),
            Some("MACHINE_LEAGUE_FLEET_RUNS_PROJECTION_READY")
        );
        assert_eq!(projection_json["fleet_run_count"].as_u64(), Some(1));
        assert_eq!(
            projection_json["runs"][0]["lanes"]
                .as_array()
                .expect("projected lanes")
                .len(),
            8
        );
    }

    #[test]
    fn machine_league_fleet_runs_hold_live_requests_behind_runner_gates() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = route_response(
            "POST",
            "/forge/machine-league/fleet-runs.json",
            r#"{"worker_count":4,"task_class":"bug_fix","language_pack_id":"rust-cargo","eval_fixture":"rust_tax_rounding","execute_live":true,"execute_native_live":false}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("fleet run json");
        let lanes = parsed["lanes"].as_array().expect("lanes");

        assert_eq!(parsed["status"].as_str(), Some("FLEET_RUN_STAGED_HELD"));
        assert!(lanes.iter().any(|lane| {
            lane["runner_id"] == "codex_cli_external_worker"
                && lane["execution_requested"] == true
                && lane["execution_performed"] == false
                && lane["lane_status"] == "LANE_HELD_EXECUTION_GATE_CLOSED"
        }));
        assert_eq!(
            parsed["integration_status"].as_str(),
            Some("waiting_for_live_lane_execution")
        );
        assert_eq!(
            parsed["winner_selection_status"].as_str(),
            Some("waiting_for_principal_reviews")
        );
        assert_eq!(parsed["external_output_consumable"].as_bool(), Some(false));
    }

    #[test]
    fn machine_league_fleet_runs_propagate_hosted_backend_to_codex_lane() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = route_response(
            "POST",
            "/forge/machine-league/fleet-runs.json",
            r#"{"worker_count":4,"task_class":"bug_fix","language_pack_id":"rust-cargo","eval_fixture":"rust_tax_rounding","execute_live":true,"execute_native_live":false,"execution_backend":"hosted_sandbox"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("fleet run json");
        let lanes = parsed["lanes"].as_array().expect("lanes");
        let codex_lane = lanes
            .iter()
            .find(|lane| lane["runner_id"] == "codex_cli_external_worker")
            .expect("codex lane");

        assert_eq!(
            codex_lane["trial_status"].as_str(),
            Some("HOSTED_SANDBOX_BACKEND_NOT_CONFIGURED")
        );
        assert_eq!(codex_lane["execution_requested"].as_bool(), Some(true));
        assert_eq!(codex_lane["execution_allowed"].as_bool(), Some(false));
        assert_eq!(codex_lane["execution_performed"].as_bool(), Some(false));
        assert_eq!(
            codex_lane["blocked_reason"].as_str(),
            Some("pending_mdx_quality_gates")
        );
        assert_eq!(parsed["external_output_consumable"].as_bool(), Some(false));

        let projection = route_response(
            "GET",
            "/forge/machine-league/fleet-runs/projection.json",
            "",
            &kernel,
        )
        .expect("projection route handled")
        .expect("projection route ok");
        let projection_json: serde_json::Value =
            serde_json::from_str(&projection.body).expect("fleet run projection json");
        let projected_codex_lane = projection_json["runs"][0]["lanes"]
            .as_array()
            .expect("projected lanes")
            .iter()
            .find(|lane| lane["runner_id"] == "codex_cli_external_worker")
            .expect("projected codex lane");
        assert_eq!(
            projected_codex_lane["trial_status"].as_str(),
            Some("HOSTED_SANDBOX_BACKEND_NOT_CONFIGURED")
        );
    }

    #[test]
    fn machine_league_fleet_run_projection_folds_lane_acceptance_from_principal_review() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = route_response(
            "POST",
            "/forge/machine-league/fleet-runs.json",
            r#"{"worker_count":4,"task_class":"bug_fix","language_pack_id":"rust-cargo","eval_fixture":"rust_tax_rounding","execute_live":false,"execute_native_live":true}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("fleet run json");
        let native_lane = parsed["lanes"]
            .as_array()
            .expect("lanes")
            .iter()
            .find(|lane| lane["runner_id"] == "mdx_native_harness_runner")
            .expect("native lane");
        let lane_run_id = native_lane["run_id"].as_str().expect("native lane run id");
        let result_receipt_id = native_lane["result_receipt_id"]
            .as_str()
            .expect("native lane result receipt");

        let before_projection = route_response(
            "GET",
            "/forge/machine-league/fleet-runs/projection.json",
            "",
            &kernel,
        )
        .expect("projection route handled")
        .expect("projection route ok");
        let before_json: serde_json::Value =
            serde_json::from_str(&before_projection.body).expect("before projection json");
        let before_native_lane = before_json["runs"][0]["lanes"]
            .as_array()
            .expect("projected lanes")
            .iter()
            .find(|lane| lane["run_id"] == lane_run_id)
            .expect("projected native lane");
        assert_eq!(
            before_native_lane["accepted_for_scoreboard"].as_bool(),
            Some(false)
        );

        let review_body = format!(
            r#"{{"run_id":{},"result_receipt_id":{},"principal_engineer_verdict":"accepted_for_scoreboard"}}"#,
            json_string_literal(lane_run_id),
            json_string_literal(result_receipt_id),
        );
        let review = route_response(
            "POST",
            "/forge/fleet-eval-results/principal-reviews.json",
            &review_body,
            &kernel,
        )
        .expect("review route handled")
        .expect("review route ok");
        assert!(review.body.contains(r#""accepted_for_scoreboard":true"#));

        let after_projection = route_response(
            "GET",
            "/forge/machine-league/fleet-runs/projection.json",
            "",
            &kernel,
        )
        .expect("projection route handled")
        .expect("projection route ok");
        let after_json: serde_json::Value =
            serde_json::from_str(&after_projection.body).expect("after projection json");
        assert_eq!(
            after_json["runs"][0]["accepted_for_scoreboard_count"].as_u64(),
            Some(1)
        );
        let after_native_lane = after_json["runs"][0]["lanes"]
            .as_array()
            .expect("projected lanes")
            .iter()
            .find(|lane| lane["run_id"] == lane_run_id)
            .expect("projected native lane");
        assert_eq!(
            after_native_lane["accepted_for_scoreboard"].as_bool(),
            Some(true)
        );
        assert!(
            !after_native_lane["principal_review_receipt_id"]
                .as_str()
                .unwrap_or("")
                .is_empty()
        );
    }

    #[test]
    fn machine_league_fleet_run_arbitration_recommends_only_accepted_lanes() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = route_response(
            "POST",
            "/forge/machine-league/fleet-runs.json",
            r#"{"worker_count":4,"task_class":"bug_fix","language_pack_id":"rust-cargo","eval_fixture":"rust_tax_rounding","execute_live":false}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("fleet run json");
        let fleet_run_id = parsed["fleet_run_id"].as_str().expect("fleet run id");
        let codex_lane = parsed["lanes"]
            .as_array()
            .expect("lanes")
            .iter()
            .find(|lane| lane["runner_id"] == "codex_cli_external_worker")
            .expect("codex lane");
        let lane_run_id = codex_lane["run_id"].as_str().expect("lane run id");
        let model_profile_id = codex_lane["model_profile_id"]
            .as_str()
            .expect("model profile");

        {
            let mut kernel = kernel.write().expect("kernel");
            kernel
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "agent:forge_eval",
                        run_id: lane_run_id,
                        event: "evidence_appended",
                        work_item_id: "machine_league_fleet_lane_review",
                        detail: "accepted lane for fleet arbitration",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &GovernedWriteIdentity::local_demo("agent:forge_eval"),
                    &[
                        ("eval_principal_review_status", "accepted_for_scoreboard"),
                        ("eval_result_receipt_id", "receipt_lane_result_codex"),
                        ("runner_id", "codex_cli_external_worker"),
                        ("winning_runner_id", "codex_cli_external_worker"),
                        ("baseline_runner_id", "mdx_native_harness_runner"),
                        ("model_profile_id", model_profile_id),
                        ("language_task_class", "bug_fix"),
                        ("language_pack_id", "rust-cargo"),
                        ("machine_league_eval_fixture", "rust_tax_rounding"),
                        ("total_score", "94"),
                        ("accepted_for_scoreboard", "true"),
                        ("external_output_consumable", "false"),
                    ],
                )
                .expect("accepted lane event");
        }

        let arbitration = route_response(
            "POST",
            "/forge/machine-league/fleet-runs/arbitrations.json",
            &format!(r#"{{"fleet_run_id":"{fleet_run_id}"}}"#),
            &kernel,
        )
        .expect("arbitration route handled")
        .expect("arbitration route ok");
        let arbitration_json: serde_json::Value =
            serde_json::from_str(&arbitration.body).expect("arbitration json");

        assert_eq!(
            arbitration_json["status"].as_str(),
            Some("FLEET_RUN_WINNER_RECOMMENDED_WAITING_FOR_HUMAN_RATIFICATION")
        );
        assert_eq!(arbitration_json["accepted_lane_count"].as_u64(), Some(1));
        assert_eq!(
            arbitration_json["winner_runner_id"].as_str(),
            Some("codex_cli_external_worker")
        );
        assert_eq!(arbitration_json["winner_total_score"].as_u64(), Some(94));
        assert_eq!(
            arbitration_json["winner_selection_basis"].as_str(),
            Some("accepted_scorecard_lane_receipts_only")
        );
        assert_eq!(
            arbitration_json["external_output_consumable"].as_bool(),
            Some(false)
        );
        assert_eq!(
            arbitration_json["production_write_allowed"].as_bool(),
            Some(false)
        );

        let projection = route_response(
            "GET",
            "/forge/machine-league/fleet-runs/arbitrations/projection.json",
            "",
            &kernel,
        )
        .expect("projection route handled")
        .expect("projection route ok");
        assert!(projection.body.contains(r#""arbitration_count":1"#));
        assert!(
            projection
                .body
                .contains(r#""winner_runner_id":"codex_cli_external_worker""#)
        );
    }

    #[test]
    fn machine_league_fleet_casts_include_closed_runners_after_fleet_clearance() {
        let _provider_key_guard = isolate_machine_option_provider_keys();
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        global().set_for_tenant("local_tenant", "XAI_API_KEY", "xai-test-key");
        route_response(
            "POST",
            "/forge/machine-league/closed-runner-clearances.json",
            r#"{"runner_id":"claude_code_external_worker","operator_attestation":true,"clearance_mode":"fleet_eval","adapter_protocol_tier":"sdk_adapter"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        let response = route_response(
            "POST",
            "/forge/machine-league/fleet-casts.json",
            r#"{"worker_count":11,"task_class":"bug_fix","language_pack_id":"rust-cargo","policy_profile":"closed_runner_cleared","strategy":"compete"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("fleet cast json");
        let casts = parsed["worker_casts"].as_array().expect("worker casts");

        assert_eq!(parsed["unique_cast_count"].as_u64(), Some(12));
        assert_eq!(parsed["worker_count_planned"].as_u64(), Some(11));
        for (runner_id, tier) in [
            ("grok_build_cli_external_worker", "protocol_adapter"),
            ("claude_code_external_worker", "sdk_adapter"),
        ] {
            assert!(
                casts.iter().any(|cast| {
                    cast["runner_id"] == runner_id && cast["adapter_protocol_tier"] == tier
                }),
                "missing cleared closed runner {runner_id}"
            );
        }
        assert!(casts.iter().all(|cast| cast["output_quarantined"] == true));
        assert!(
            casts
                .iter()
                .all(|cast| cast["external_output_consumable"] == false)
        );
        global().remove_for_tenant("local_tenant", "XAI_API_KEY");
    }

    #[test]
    fn adaptive_external_run_binds_latest_clearance_and_approval() {
        let _env = crate::harness_worker_workspace::ENV_LOCK
            .lock()
            .expect("environment lock");
        let previous_gate = std::env::var_os("MDX_MACHINE_LEAGUE_GROK_BUILD_EXEC");
        // SAFETY: the shared environment lock is held for the full mutation
        // and restoration window.
        unsafe { std::env::set_var("MDX_MACHINE_LEAGUE_GROK_BUILD_EXEC", "1") };
        let _provider_key_guard = isolate_machine_option_provider_keys();
        global().set_for_tenant("local_tenant", "XAI_API_KEY", "xai-test-key");
        let mut kernel = MdxKernel::boot_local();
        let clearance = render_closed_runner_clearance_json(
            r#"{"runner_id":"grok_build_cli_external_worker","operator_attestation":true,"clearance_mode":"fleet_eval","adapter_protocol_tier":"protocol_adapter"}"#,
            &mut kernel,
        )
        .expect("clearance");
        let clearance: serde_json::Value =
            serde_json::from_str(&clearance).expect("clearance json");
        let approval = render_live_run_approval_json(
            r#"{"approval_id":"adaptive_grok_once","provider_allowlist":"xai","max_spend_cents":500,"max_tasks":1,"max_parallel_agents":1,"artifact_retention_policy":"quarantine_7_days","redaction_policy":"mdx_context_redaction_required","stop_conditions":"budget_exhausted,quality_gate_failure,manual_stop"}"#,
            &mut kernel,
            global(),
        )
        .expect("approval");
        let approval: serde_json::Value = serde_json::from_str(&approval).expect("approval json");

        let authorization =
            authorize_forge_external_run("{}", &kernel, "local_tenant", "grok_build", 500, 1)
                .expect("latest compatible receipts bind automatically");
        assert_eq!(authorization.approval_id, "adaptive_grok_once");
        assert_eq!(
            authorization.approval_receipt_id,
            approval["approval_receipt_id"]
                .as_str()
                .expect("approval id")
        );
        assert_eq!(
            authorization.clearance_receipt_id,
            clearance["clearance_receipt_id"]
                .as_str()
                .expect("clearance id")
        );
        assert_eq!(authorization.max_spend_cents, 500);
        global().remove_for_tenant("local_tenant", "XAI_API_KEY");
        // SAFETY: the shared environment lock remains held.
        unsafe {
            match previous_gate {
                Some(value) => std::env::set_var("MDX_MACHINE_LEAGUE_GROK_BUILD_EXEC", value),
                None => std::env::remove_var("MDX_MACHINE_LEAGUE_GROK_BUILD_EXEC"),
            }
        }
    }

    #[test]
    fn live_machine_authorization_binds_runner_clearance_and_provider_approval() {
        let _provider_key_guard = isolate_machine_option_provider_keys();
        let mut kernel = MdxKernel::boot_local();
        let approval = render_live_run_approval_json(
            r#"{"approval_id":"approval_external_adapter_test","provider_allowlist":"openai,xai,anthropic","max_spend_cents":75,"max_tasks":3,"max_parallel_agents":1,"artifact_retention_policy":"quarantine_7_days","redaction_policy":"mdx_context_redaction_required","stop_conditions":"budget_exhausted,quality_gate_failure,manual_stop"}"#,
            &mut kernel,
            global(),
        )
        .expect("approval");
        let approval: serde_json::Value = serde_json::from_str(&approval).expect("approval json");
        let approval_receipt_id = approval["approval_receipt_id"]
            .as_str()
            .expect("approval receipt");
        global().set_for_tenant("local_tenant", "XAI_API_KEY", "xai-test-key");
        global().set_for_tenant("local_tenant", "ANTHROPIC_API_KEY", "anthropic-test-key");
        let mut codex_clearance_receipt_id = String::new();

        for (runner_id, adapter_kind, tier, provider) in [
            (
                "codex_cli_external_worker",
                "codex_cli",
                "headless_cli_adapter",
                "openai",
            ),
            (
                "grok_build_cli_external_worker",
                "grok_build_cli",
                "protocol_adapter",
                "xai",
            ),
            (
                "claude_code_external_worker",
                "claude_code",
                "sdk_adapter",
                "anthropic",
            ),
        ] {
            let clearance_receipt_id = if live_runner_requires_clearance(adapter_kind) {
                let clearance = render_closed_runner_clearance_json(
                    &format!(
                        r#"{{"runner_id":"{runner_id}","operator_attestation":true,"clearance_mode":"fleet_eval","adapter_protocol_tier":"{tier}"}}"#
                    ),
                    &mut kernel,
                )
                .expect("clearance");
                let clearance: serde_json::Value =
                    serde_json::from_str(&clearance).expect("clearance json");
                clearance["clearance_receipt_id"]
                    .as_str()
                    .expect("clearance receipt")
                    .to_string()
            } else {
                String::new()
            };
            let authorization = authorize_live_machine_trial(
                &format!(
                    r#"{{"approval_id":"approval_external_adapter_test","approval_receipt_id":"{approval_receipt_id}","clearance_receipt_id":"{clearance_receipt_id}"}}"#
                ),
                &kernel,
                runner_id,
                adapter_kind,
            )
            .expect("matching authorization");
            assert_eq!(authorization.provider, provider);
            assert_eq!(authorization.clearance_receipt_id, clearance_receipt_id);
            assert_eq!(authorization.max_spend_cents, 75);
            assert_eq!(authorization.max_tasks, 3);
            assert_eq!(authorization.max_parallel_agents, 1);
            assert_eq!(authorization.task_ordinal, 1);
            if adapter_kind == "codex_cli" {
                codex_clearance_receipt_id = clearance_receipt_id.to_string();
                let receipt_id = record_external_output_consumption_boundary(
                    &mut kernel,
                    &GovernedWriteIdentity::local_demo("agent:codex"),
                    ExternalOutputBoundary {
                        actor_id: "agent:codex",
                        run_id: "authorization_receipt_test",
                        work_item_id: "machine_league_codex_trial",
                        runner_id,
                        adapter_kind,
                        output_artifact_ref:
                            "quarantine://forge/fleet-eval/authorization_receipt_test/codex.patch",
                    },
                    Some(&authorization),
                )
                .expect("consumption boundary receipt");
                let receipt = kernel
                    .ledger()
                    .query()
                    .by_id(&receipt_id)
                    .expect("recorded consumption boundary");
                assert_eq!(payload_value(receipt, "live_run_max_spend_cents"), "75");
                assert_eq!(payload_value(receipt, "live_run_max_tasks"), "3");
                assert_eq!(payload_value(receipt, "live_run_max_parallel_agents"), "1");
                assert_eq!(payload_value(receipt, "live_run_task_ordinal"), "1");
                assert_eq!(
                    payload_value(receipt, "external_output_consumption_status"),
                    "denied_quarantined_pending_distillation"
                );
            }
        }

        let missing_grok_clearance = authorize_live_machine_trial(
            &format!(
                r#"{{"approval_id":"approval_external_adapter_test","approval_receipt_id":"{approval_receipt_id}"}}"#
            ),
            &kernel,
            "grok_build_cli_external_worker",
            "grok_build_cli",
        )
        .expect_err("Grok live execution requires its open-source execution clearance");
        assert!(missing_grok_clearance.contains("clearance_receipt_id is required"));
        let wrong_grok_clearance = authorize_live_machine_trial(
            &format!(
                r#"{{"approval_id":"approval_external_adapter_test","approval_receipt_id":"{approval_receipt_id}","clearance_receipt_id":"wrong"}}"#
            ),
            &kernel,
            "grok_build_cli_external_worker",
            "grok_build_cli",
        )
        .expect_err("Grok live execution rejects a mismatched execution clearance");
        assert!(wrong_grok_clearance.contains("does not match"));

        let spend_refusal = authorize_live_machine_trial(
            &format!(
                r#"{{"approval_id":"approval_external_adapter_test","approval_receipt_id":"{approval_receipt_id}","clearance_receipt_id":"{codex_clearance_receipt_id}","trial_max_spend_cents":76}}"#
            ),
            &kernel,
            "codex_cli_external_worker",
            "codex_cli",
        )
        .expect_err("a trial cannot exceed its approved spend ceiling");
        assert!(spend_refusal.contains("approved 75 cents"));

        for index in 1..=3 {
            kernel
                .record_forge_run_event_with_evidence_fields(
                    ForgeRunEvent {
                        tenant_id: "local_tenant",
                        actor_id: "agent:codex",
                        run_id: &format!("approval_task_{index}"),
                        event: "run_started",
                        work_item_id: "machine_league_codex_trial",
                        detail: "live trial executed under approval",
                        turn: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                    },
                    &GovernedWriteIdentity::local_demo("agent:codex"),
                    &[
                        ("live_run_approval_receipt_id", approval_receipt_id),
                        ("adapter_execution_performed", "true"),
                    ],
                )
                .expect("execution receipt");
        }
        let task_refusal = authorize_live_machine_trial(
            &format!(
                r#"{{"approval_id":"approval_external_adapter_test","approval_receipt_id":"{approval_receipt_id}","clearance_receipt_id":"{codex_clearance_receipt_id}"}}"#
            ),
            &kernel,
            "codex_cli_external_worker",
            "codex_cli",
        )
        .expect_err("an exhausted approval cannot authorize another task");
        assert!(task_refusal.contains("3 of 3 tasks already executed"));

        let refusal = authorize_live_machine_trial(
            &format!(
                r#"{{"approval_id":"approval_external_adapter_test","approval_receipt_id":"{approval_receipt_id}","clearance_receipt_id":"wrong"}}"#
            ),
            &kernel,
            "codex_cli_external_worker",
            "codex_cli",
        )
        .expect_err("mismatched clearance must fail closed");
        assert!(refusal.contains("does not match"));
        global().remove_for_tenant("local_tenant", "ANTHROPIC_API_KEY");
    }

    #[test]
    fn machine_league_face_off_pack_stages_recommended_machine_with_visible_contract() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = route_response(
            "POST",
            "/forge/machine-league/face-offs.json",
            r#"{"external_runner_id":"codex_cli_external_worker","task_class":"bug_fix","language_pack_id":"rust-cargo","eval_fixture":"rust_tax_rounding"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("face-off json");

        assert_eq!(parsed["status"].as_str(), Some("FACE_OFF_STAGED_HELD"));
        assert_eq!(
            parsed["runner_id"].as_str(),
            Some("codex_cli_external_worker")
        );
        assert_eq!(
            parsed["model_profile_id"].as_str(),
            Some("codex_openai_responses_profile")
        );
        assert_eq!(
            parsed["adapter_protocol_tier"].as_str(),
            Some("headless_cli_adapter")
        );
        assert_eq!(
            parsed["external_trial_status"].as_str(),
            Some("CODEX_CLI_TRIAL_PACKET_RECORDED_QUARANTINED")
        );
        assert_eq!(
            parsed["native_result_state"].as_str(),
            Some("not_enough_accepted_evidence")
        );
        assert_eq!(
            parsed["learning_status"].as_str(),
            Some("nothing_learned_yet")
        );
        assert_eq!(
            parsed["output_state"].as_str(),
            Some("held_until_mdx_gates_accept")
        );
        assert_eq!(parsed["external_output_consumable"].as_bool(), Some(false));
        assert_eq!(parsed["production_write_allowed"].as_bool(), Some(false));

        let projection = route_response(
            "GET",
            "/forge/machine-league/face-offs/projection.json",
            "",
            &kernel,
        )
        .expect("projection handled")
        .expect("projection ok");
        assert!(
            projection
                .body
                .contains(r#""status":"MACHINE_LEAGUE_FACE_OFF_PROJECTION_READY""#)
        );
        assert!(
            projection
                .body
                .contains(r#""runner_id":"codex_cli_external_worker""#)
        );
    }

    #[test]
    fn machine_league_grok_face_off_stages_without_closed_runner_terms_clearance() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let waiting = route_response(
            "POST",
            "/forge/machine-league/face-offs.json",
            r#"{"external_runner_id":"grok_build_cli_external_worker","task_class":"bug_fix","language_pack_id":"rust-cargo"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let parsed: serde_json::Value = serde_json::from_str(&waiting.body).expect("face-off json");
        assert_eq!(parsed["status"].as_str(), Some("FACE_OFF_STAGED_HELD"));
        assert_eq!(
            parsed["closed_runner_clearance_status"].as_str(),
            Some("clearance_not_required")
        );
        assert_eq!(
            parsed["external_trial_status"].as_str(),
            Some("GROK_BUILD_CLI_TRIAL_PACKET_RECORDED_QUARANTINED")
        );

        let clearance = route_response(
            "POST",
            "/forge/machine-league/closed-runner-clearances.json",
            r#"{"runner_id":"grok_build_cli_external_worker","operator_attestation":true,"clearance_mode":"fleet_eval","adapter_protocol_tier":"protocol_adapter"}"#,
            &kernel,
        )
        .expect("clearance handled")
        .expect("clearance ok");
        assert!(
            clearance
                .body
                .contains(r#""status":"CLOSED_RUNNER_CLEARANCE_RECORDED""#)
        );
        assert!(
            clearance
                .body
                .contains(r#""clearance_basis":"open_source_execution_attestation""#)
        );
        assert!(
            matches!(
                parsed["adapter_protocol_tier"].as_str(),
                Some("declared_only") | Some("protocol_adapter")
            ),
            "Grok may be declared-only or locally source-verified without changing its terms-clearance posture"
        );
        assert_eq!(parsed["external_output_consumable"].as_bool(), Some(false));
    }

    #[test]
    fn machine_runtime_smoke_records_version_only_compatibility_evidence() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = route_response(
            "POST",
            "/forge/machine-league/runtime-smokes.json",
            r#"{"runner_id":"opencode_external_worker"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&response.body).expect("runtime smoke json");

        assert_eq!(
            parsed["route"].as_str(),
            Some("/forge/machine-league/runtime-smokes.json")
        );
        assert_eq!(
            parsed["runner_id"].as_str(),
            Some("opencode_external_worker")
        );
        assert_eq!(parsed["adapter_execution_performed"].as_bool(), Some(false));
        assert_eq!(parsed["task_execution_allowed"].as_bool(), Some(false));
        assert_eq!(parsed["external_output_consumable"].as_bool(), Some(false));
        assert!(
            parsed["runtime_fingerprint"]["fingerprint_id"]
                .as_str()
                .unwrap_or("")
                .starts_with("sha256:")
        );
        assert!(
            matches!(
                parsed["machine_runtime_smoke_status"].as_str(),
                Some("runtime_smoke_passed") | Some("runtime_smoke_blocked_binary_missing")
            ),
            "smoke should be version-only passed or held by binary presence"
        );
        let guard = kernel.read().expect("kernel");
        assert!(
            guard
                .ledger()
                .query()
                .by_kind("forge.run.event")
                .iter()
                .any(
                    |receipt| payload_value(receipt, "machine_runtime_smoke_status")
                        .starts_with("runtime_smoke_")
                )
        );
    }

    #[test]
    fn codex_trial_reuses_live_forge_run_projection_with_quarantine_context() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let trial = route_response(
            "POST",
            "/forge/machine-league/codex-trials.json",
            r#"{"intent":"compare Codex CLI on a small Rust fix","task_corpus_id":"rust-cargo-bug-fix-small"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            trial
                .body
                .contains(r#""status":"CODEX_CLI_TRIAL_PACKET_RECORDED_QUARANTINED""#)
        );
        assert!(trial.body.contains(r#""runner_display_name":"Codex CLI""#));
        assert!(
            trial
                .body
                .contains(r#""runtime_fingerprint":{"runner_id":"codex_cli_external_worker""#)
        );
        assert!(
            trial
                .body
                .contains(r#""adapter_execution_performed":false"#)
        );
        assert!(trial.body.contains(r#""accepted_for_scoreboard":false"#));

        let projection = crate::forge_run_route::route_response(
            "GET",
            "/forge/runs/projection.json",
            "",
            &kernel,
        )
        .expect("run route handled")
        .expect("run route ok");

        assert!(
            projection
                .body
                .contains(r#""runner_profile":{"runner_id":"codex_cli_external_worker""#)
        );
        assert!(projection.body.contains(r#""display_name":"Codex CLI""#));
        assert!(projection.body.contains(r#""leads_header":true"#));
        assert!(projection.body.contains(r#""model_disclosed_second":true"#));
        assert!(
            projection
                .body
                .contains(r#""machine_runtime":{"runner_id":"codex_cli_external_worker""#)
        );
        assert!(
            projection
                .body
                .contains(r#""adapter_contract_version":"machine_league_adapter_contract_v1""#)
        );
        assert!(
            projection
                .body
                .contains(r#""quarantine":{"status":"output_quarantined_pending_mdx_gates""#)
        );
        assert!(projection.body.contains(r#""output_quarantined":true"#));
        assert!(
            projection
                .body
                .contains(r#""external_output_consumable":false"#)
        );
        assert!(
            projection
                .body
                .contains(r#""league_context":{"required_for_trial":true"#)
        );
        assert!(
            projection
                .body
                .contains(r#""visibility_tier":"product_recommendation""#)
        );
        assert!(
            projection
                .body
                .contains(r#""recommendation_route":"/forge/machine-league/recommendations.json""#)
        );

        let results = route_response(
            "GET",
            "/forge/fleet-eval-results/projection.json",
            "",
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        assert!(
            results
                .body
                .contains(r#""runner_id":"codex_cli_external_worker""#)
        );
        assert!(results.body.contains(r#""accepted_for_scoreboard":false"#));
        assert!(
            results
                .body
                .contains(r#"quarantine://forge/fleet-eval/machine-league"#)
        );
    }

    #[test]
    fn machine_runtime_fingerprint_links_to_prior_observation() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let first = route_response(
            "POST",
            "/forge/machine-league/opencode-trials.json",
            r#"{"intent":"record first runtime observation","eval_fixture":"rust_tax_rounding"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let first_packet: serde_json::Value =
            serde_json::from_str(&first.body).expect("first trial json");
        let first_id = first_packet["runtime_fingerprint"]["fingerprint_id"]
            .as_str()
            .expect("first fingerprint id")
            .to_string();
        assert_eq!(
            first_packet["runtime_fingerprint"]["last_fingerprint_id"].as_str(),
            Some("")
        );

        let second = route_response(
            "POST",
            "/forge/machine-league/opencode-trials.json",
            r#"{"intent":"record second runtime observation","eval_fixture":"rust_tax_rounding"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let second_packet: serde_json::Value =
            serde_json::from_str(&second.body).expect("second trial json");
        assert_eq!(
            second_packet["runtime_fingerprint"]["last_fingerprint_id"].as_str(),
            Some(first_id.as_str())
        );
        assert_eq!(
            second_packet["runtime_fingerprint"]["fingerprint_id"].as_str(),
            Some(first_id.as_str())
        );
    }

    #[test]
    fn gemini_trial_reuses_live_forge_run_projection_with_quarantine_context() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let trial = route_response(
            "POST",
            "/forge/machine-league/gemini-trials.json",
            r#"{"intent":"compare Gemini CLI on the held-out Rust tax fixture","eval_fixture":"rust_tax_rounding"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            trial
                .body
                .contains(r#""status":"GEMINI_CLI_TRIAL_PACKET_RECORDED_QUARANTINED""#)
        );
        assert!(trial.body.contains(r#""runner_display_name":"Gemini CLI""#));
        assert!(
            trial
                .body
                .contains(r#""adapter_execution_performed":false"#)
        );
        assert!(trial.body.contains(r#""accepted_for_scoreboard":false"#));

        let projection = crate::forge_run_route::route_response(
            "GET",
            "/forge/runs/projection.json",
            "",
            &kernel,
        )
        .expect("run route handled")
        .expect("run route ok");

        assert!(
            projection
                .body
                .contains(r#""runner_profile":{"runner_id":"gemini_cli_external_worker""#)
        );
        assert!(projection.body.contains(r#""display_name":"Gemini CLI""#));
        assert!(projection.body.contains(r#""leads_header":true"#));
        assert!(
            projection
                .body
                .contains(r#""quarantine":{"status":"output_quarantined_pending_mdx_gates""#)
        );
        assert!(projection.body.contains(r#""output_quarantined":true"#));
        assert!(
            projection
                .body
                .contains(r#""external_output_consumable":false"#)
        );

        let results = route_response(
            "GET",
            "/forge/fleet-eval-results/projection.json",
            "",
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        assert!(
            results
                .body
                .contains(r#""runner_id":"gemini_cli_external_worker""#)
        );
        assert!(results.body.contains(r#""accepted_for_scoreboard":false"#));
        assert!(
            results
                .body
                .contains(r#"quarantine://forge/fleet-eval/machine-league"#)
        );
    }

    #[test]
    fn gemini_trial_live_request_stays_held_without_env_gate() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let trial = route_response(
            "POST",
            "/forge/machine-league/gemini-trials.json",
            r#"{"intent":"fix the held-out tax fixture","eval_fixture":"rust_tax_rounding","execute_live":true}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            trial
                .body
                .contains(r#""status":"GEMINI_CLI_TRIAL_PACKET_RECORDED_QUARANTINED""#)
        );
        assert!(trial.body.contains(r#""adapter_execution_requested":true"#));
        assert!(trial.body.contains(r#""adapter_execution_allowed":false"#));
        assert!(
            trial
                .body
                .contains(r#""adapter_execution_performed":false"#)
        );

        let projection = crate::forge_run_route::route_response(
            "GET",
            "/forge/runs/projection.json",
            "",
            &kernel,
        )
        .expect("run route handled")
        .expect("run route ok");

        assert!(
            projection
                .body
                .contains("gemini_cli execution_disabled_by_env trial_packet_recorded")
        );
        assert!(
            projection
                .body
                .contains(r#""execution_mode":"quarantined_trial_packet_recorded""#)
        );
    }

    #[test]
    fn open_machine_trials_reuse_live_forge_run_projection_with_quarantine_context() {
        for (route, runner_id, display_name, status) in [
            (
                "/forge/machine-league/opencode-trials.json",
                "opencode_external_worker",
                "OpenCode",
                "OPENCODE_CLI_TRIAL_PACKET_RECORDED_QUARANTINED",
            ),
            (
                "/forge/machine-league/cline-trials.json",
                "cline_external_worker",
                "Cline",
                "CLINE_TRIAL_PACKET_RECORDED_QUARANTINED",
            ),
            (
                "/forge/machine-league/goose-trials.json",
                "goose_external_worker",
                "Goose",
                "GOOSE_CLI_TRIAL_PACKET_RECORDED_QUARANTINED",
            ),
        ] {
            let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

            let trial = route_response(
                "POST",
                route,
                r#"{"intent":"compare an open machine on the held-out Rust tax fixture","eval_fixture":"rust_tax_rounding"}"#,
                &kernel,
            )
            .expect("route handled")
            .expect("route ok");

            assert!(trial.body.contains(&format!(r#""status":"{status}""#)));
            assert!(
                trial
                    .body
                    .contains(&format!(r#""runner_display_name":"{display_name}""#))
            );
            assert!(
                trial
                    .body
                    .contains(r#""adapter_execution_performed":false"#)
            );
            assert!(trial.body.contains(r#""accepted_for_scoreboard":false"#));

            let projection = crate::forge_run_route::route_response(
                "GET",
                "/forge/runs/projection.json",
                "",
                &kernel,
            )
            .expect("run route handled")
            .expect("run route ok");

            assert!(
                projection
                    .body
                    .contains(&format!(r#""runner_profile":{{"runner_id":"{runner_id}""#))
            );
            assert!(
                projection
                    .body
                    .contains(&format!(r#""display_name":"{display_name}""#))
            );
            assert!(projection.body.contains(r#""leads_header":true"#));
            assert!(
                projection
                    .body
                    .contains(r#""quarantine":{"status":"output_quarantined_pending_mdx_gates""#)
            );
            assert!(projection.body.contains(r#""output_quarantined":true"#));
            assert!(
                projection
                    .body
                    .contains(r#""external_output_consumable":false"#)
            );

            let results = route_response(
                "GET",
                "/forge/fleet-eval-results/projection.json",
                "",
                &kernel,
            )
            .expect("route handled")
            .expect("route ok");
            assert!(
                results
                    .body
                    .contains(&format!(r#""runner_id":"{runner_id}""#))
            );
            assert!(results.body.contains(r#""accepted_for_scoreboard":false"#));
            assert!(
                results
                    .body
                    .contains(r#"quarantine://forge/fleet-eval/machine-league"#)
            );
        }
    }

    #[test]
    fn open_machine_live_requests_stay_held_without_binary_or_env_gate() {
        for (route, adapter_kind, adapter_label) in [
            (
                "/forge/machine-league/opencode-trials.json",
                "opencode",
                "opencode",
            ),
            ("/forge/machine-league/cline-trials.json", "cline", "cline"),
            ("/forge/machine-league/goose-trials.json", "goose", "goose"),
        ] {
            let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
            let blocked_reason = if binary_present(adapter_binary_name(adapter_kind)) {
                format!("{adapter_label} execution_disabled_by_env trial_packet_recorded")
            } else {
                format!("{adapter_label} binary_missing trial_packet_recorded")
            };

            let trial = route_response(
                "POST",
                route,
                r#"{"intent":"fix the held-out tax fixture","eval_fixture":"rust_tax_rounding","execute_live":true}"#,
                &kernel,
            )
            .expect("route handled")
            .expect("route ok");

            assert!(trial.body.contains(r#""adapter_execution_requested":true"#));
            assert!(trial.body.contains(r#""adapter_execution_allowed":false"#));
            assert!(
                trial
                    .body
                    .contains(r#""adapter_execution_performed":false"#)
            );

            let projection = crate::forge_run_route::route_response(
                "GET",
                "/forge/runs/projection.json",
                "",
                &kernel,
            )
            .expect("run route handled")
            .expect("run route ok");

            assert!(projection.body.contains(&blocked_reason));
            assert!(
                projection
                    .body
                    .contains(r#""execution_mode":"quarantined_trial_packet_recorded""#)
            );
        }
    }

    #[test]
    fn codex_trial_live_request_stays_held_without_env_gate() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let trial = route_response(
            "POST",
            "/forge/machine-league/codex-trials.json",
            r#"{"intent":"compare Codex CLI on a small Rust fix","execute_live":true}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            trial
                .body
                .contains(r#""status":"CODEX_CLI_TRIAL_PACKET_RECORDED_QUARANTINED""#)
        );
        assert!(trial.body.contains(r#""adapter_execution_requested":true"#));
        assert!(trial.body.contains(r#""adapter_execution_allowed":false"#));
        assert!(
            trial
                .body
                .contains(r#""adapter_execution_performed":false"#)
        );

        let projection = crate::forge_run_route::route_response(
            "GET",
            "/forge/runs/projection.json",
            "",
            &kernel,
        )
        .expect("run route handled")
        .expect("run route ok");

        assert!(
            projection
                .body
                .contains("execution_disabled_by_env trial_packet_recorded")
        );
        assert!(
            projection
                .body
                .contains(r#""execution_mode":"quarantined_trial_packet_recorded""#)
        );
    }

    #[test]
    fn codex_fixture_trial_stays_held_without_env_gate() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let trial = route_response(
            "POST",
            "/forge/machine-league/codex-trials.json",
            r#"{"intent":"fix the held-out tax fixture","eval_fixture":"rust_tax_rounding","execute_live":true}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            trial
                .body
                .contains(r#""status":"CODEX_CLI_TRIAL_PACKET_RECORDED_QUARANTINED""#)
        );
        assert!(trial.body.contains(r#""adapter_execution_requested":true"#));
        assert!(trial.body.contains(r#""adapter_execution_allowed":false"#));
        assert!(
            trial
                .body
                .contains(r#""adapter_execution_performed":false"#)
        );

        let projection = crate::forge_run_route::route_response(
            "GET",
            "/forge/runs/projection.json",
            "",
            &kernel,
        )
        .expect("run route handled")
        .expect("run route ok");

        assert!(
            projection
                .body
                .contains("machine-league-rust-tax-rounding-fixture")
        );
        assert!(
            projection
                .body
                .contains(r#""selected_checks":["cargo test --test visible"]"#)
        );
    }

    #[test]
    fn codex_hosted_sandbox_request_fails_closed_without_local_runner_install() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let trial = route_response(
            "POST",
            "/forge/machine-league/codex-trials.json",
            r#"{"intent":"run Codex in hosted Forge","eval_fixture":"rust_tax_rounding","execute_live":true,"execution_backend":"hosted_sandbox"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&trial.body).expect("hosted trial json");

        assert_eq!(
            parsed["status"].as_str(),
            Some("HOSTED_SANDBOX_BACKEND_NOT_CONFIGURED")
        );
        assert_eq!(parsed["execution_backend"].as_str(), Some("hosted_sandbox"));
        assert_eq!(parsed["adapter_execution_requested"].as_bool(), Some(true));
        assert_eq!(parsed["adapter_execution_allowed"].as_bool(), Some(false));
        assert_eq!(parsed["adapter_execution_performed"].as_bool(), Some(false));
        assert!(
            parsed["worktree_path"]
                .as_str()
                .unwrap_or("")
                .starts_with("sandbox-session://pending/")
        );

        let guard = kernel.read().expect("kernel");
        let run_event = guard
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .into_iter()
            .find(|receipt| {
                payload_value(receipt, "execution_backend_kind") == "hosted_sandbox"
                    && payload_value(receipt, "adapter_execution_backend") == "hosted_sandbox"
            })
            .expect("hosted backend receipt");
        assert_eq!(
            payload_value(run_event, "adapter_execution_status"),
            "HOSTED_SANDBOX_BACKEND_NOT_CONFIGURED"
        );
        assert_eq!(
            payload_value(run_event, "adapter_execution_performed"),
            "false"
        );
    }

    #[test]
    fn native_fixture_trial_runs_same_held_out_fixture_with_receipts() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let trial = route_response(
            "POST",
            "/forge/machine-league/native-trials.json",
            r#"{"intent":"run the native house path on the held-out tax fixture","eval_fixture":"rust_tax_rounding","execute_live":true,"native_execution":"scripted"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            trial
                .body
                .contains(r#""status":"MDX_NATIVE_EXECUTED_QUARANTINED_PENDING_MDX_GATES""#)
        );
        assert!(
            trial
                .body
                .contains(r#""runner_id":"mdx_native_harness_runner""#)
        );
        assert!(trial.body.contains(r#""adapter_execution_performed":true"#));
        assert!(trial.body.contains(r#""native_exit_code":0"#));
        assert!(trial.body.contains(r#""check_exit_code":0"#));
        assert!(trial.body.contains(r#""check_passed":true"#));
        assert!(trial.body.contains(r#""changed_file_count":1"#));

        let parsed: serde_json::Value = serde_json::from_str(&trial.body).expect("trial json");
        let result_receipt_id = parsed["result_receipt_id"]
            .as_str()
            .expect("result receipt id");
        let review_body = format!(
            r#"{{"run_id":"forge_machine_league_native_trial_000001","result_receipt_id":{},"principal_engineer_verdict":"accepted_for_scoreboard"}}"#,
            json_string_literal(result_receipt_id)
        );
        let review = route_response(
            "POST",
            "/forge/fleet-eval-results/principal-reviews.json",
            &review_body,
            &kernel,
        )
        .expect("review route handled")
        .expect("review route ok");

        assert!(
            review
                .body
                .contains(r#""status":"ACCEPTED_FOR_SCOREBOARD""#)
        );
        assert!(
            review
                .body
                .contains(r#""runner_id":"mdx_native_harness_runner""#)
        );
        assert!(
            review
                .body
                .contains(r#""model_profile_id":"mdx_native_local_model_profile""#)
        );
        assert!(review.body.contains(r#""accepted_for_scoreboard":true"#));
        // A scripted baseline can be principal-accepted for its receipt trail,
        // but the acceptance carries the scripted_baseline label, so it never
        // accrues league standing in the scoreboard counter.
        assert_eq!(
            accepted_score_count_for_runner(
                &kernel.read().expect("kernel"),
                "mdx_native_harness_runner"
            ),
            0
        );
    }

    #[test]
    fn native_policy_edge_baseline_fails_hidden_oracle_without_scorecard_acceptance() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let trial = route_response(
            "POST",
            "/forge/machine-league/native-trials.json",
            r#"{"run_id":"forge_machine_league_native_policy_edge_baseline","intent":"run native on the policy edge fixture","eval_fixture":"rust_feature_policy_edge","execute_live":true,"native_execution":"scripted"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            trial
                .body
                .contains(r#""native_strategy_profile":"single_file_visible_case_patch""#)
        );
        assert!(trial.body.contains(r#""visible_check_passed":true"#));
        assert!(trial.body.contains(r#""hidden_check_observed":true"#));
        assert!(trial.body.contains(r#""hidden_check_passed":false"#));
        assert!(trial.body.contains(r#""check_passed":false"#));

        let parsed: serde_json::Value = serde_json::from_str(&trial.body).expect("trial json");
        let result_receipt_id = parsed["result_receipt_id"]
            .as_str()
            .expect("result receipt id");
        let review_body = format!(
            r#"{{"run_id":"forge_machine_league_native_policy_edge_baseline","result_receipt_id":{},"principal_engineer_verdict":"accepted_for_scoreboard"}}"#,
            json_string_literal(result_receipt_id)
        );
        let review = route_response(
            "POST",
            "/forge/fleet-eval-results/principal-reviews.json",
            &review_body,
            &kernel,
        )
        .expect("review route handled")
        .expect("review route ok");

        assert!(review.body.contains(r#""status":"REFUSED""#));
        assert!(review.body.contains(r#""accepted_for_scoreboard":false"#));
    }

    #[test]
    fn native_policy_edge_improved_strategy_passes_hidden_oracle() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let trial = route_response(
            "POST",
            "/forge/machine-league/native-trials.json",
            r#"{"run_id":"forge_machine_league_native_policy_edge_improved","intent":"rerun native with the learned cross-file policy strategy","eval_fixture":"rust_feature_policy_edge","execute_live":true,"native_strategy_profile":"cross_file_symbol_policy_first","native_execution":"scripted"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            trial
                .body
                .contains(r#""native_strategy_profile":"cross_file_symbol_policy_first""#)
        );
        assert!(trial.body.contains(r#""visible_check_passed":true"#));
        assert!(trial.body.contains(r#""hidden_check_passed":true"#));
        assert!(trial.body.contains(r#""check_passed":true"#));

        let parsed: serde_json::Value = serde_json::from_str(&trial.body).expect("trial json");
        let result_receipt_id = parsed["result_receipt_id"]
            .as_str()
            .expect("result receipt id");
        let review_body = format!(
            r#"{{"run_id":"forge_machine_league_native_policy_edge_improved","result_receipt_id":{},"principal_engineer_verdict":"accepted_for_scoreboard"}}"#,
            json_string_literal(result_receipt_id)
        );
        let review = route_response(
            "POST",
            "/forge/fleet-eval-results/principal-reviews.json",
            &review_body,
            &kernel,
        )
        .expect("review route handled")
        .expect("review route ok");

        assert!(
            review
                .body
                .contains(r#""status":"ACCEPTED_FOR_SCOREBOARD""#)
        );
        assert!(review.body.contains(r#""accepted_for_scoreboard":true"#));
    }

    #[test]
    fn learning_ledger_marks_external_edge_after_native_policy_fixture_failure() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_response(
            "POST",
            "/forge/machine-league/native-trials.json",
            r#"{"run_id":"forge_machine_league_native_policy_edge_for_ledger","intent":"run native on the policy edge fixture","eval_fixture":"rust_feature_policy_edge","execute_live":true,"native_execution":"scripted"}"#,
            &kernel,
        )
        .expect("native route handled")
        .expect("native route ok");
        seed_accepted_machine_league_score_for_fixture(
            &kernel,
            "forge_machine_league_codex_policy_edge_win",
            "codex_cli_external_worker",
            97,
            MACHINE_LEAGUE_RUST_POLICY_FIXTURE_ID,
            MACHINE_LEAGUE_RUST_POLICY_FIXTURE_SNAPSHOT_ID,
        );

        let ledger = route_response(
            "GET",
            "/forge/machine-league/learning-ledger.json",
            "",
            &kernel,
        )
        .expect("ledger route handled")
        .expect("ledger route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&ledger.body).expect("valid learning ledger");

        assert_eq!(parsed["entry_count"].as_u64(), Some(0));
        assert_eq!(
            parsed["pending_distillation_candidates"][0]["status"].as_str(),
            Some("external_edge_pending_distillation_note")
        );
        assert_eq!(
            parsed["pending_distillation_candidates"][0]
                ["native_same_fixture_failed_attempt_count"]
                .as_u64(),
            Some(1)
        );
        assert_eq!(
            parsed["pending_distillation_candidates"][0]["native_same_fixture_accepted_count"]
                .as_u64(),
            Some(0)
        );
    }

    #[test]
    fn native_improvement_loop_infers_policy_family_strategy_for_sibling_fixture() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        seed_accepted_machine_league_score_for_fixture(
            &kernel,
            "forge_machine_league_codex_policy_edge_source",
            "codex_cli_external_worker",
            97,
            MACHINE_LEAGUE_RUST_POLICY_FIXTURE_ID,
            MACHINE_LEAGUE_RUST_POLICY_FIXTURE_SNAPSHOT_ID,
        );
        route_response(
            "POST",
            "/forge/machine-league/native-trials.json",
            r#"{"run_id":"forge_machine_league_native_policy_edge_failed_before_learning","intent":"run native on the policy edge fixture","eval_fixture":"rust_feature_policy_edge","execute_live":true,"native_execution":"scripted"}"#,
            &kernel,
        )
        .expect("native route handled")
        .expect("native route ok");
        route_response(
            "POST",
            "/forge/machine-league/distillation-notes.json",
            r#"{"distillation_note_id":"distillation_note_policy_edge","source_run_id":"forge_machine_league_codex_policy_edge_source","winning_runner_id":"codex_cli_external_worker","baseline_runner_id":"mdx_native_harness_runner","distillation_reason":"planning_strategy","language_pack_id":"rust-cargo","task_class":"bug_fix","confidence":"medium"}"#,
            &kernel,
        )
        .expect("note route handled")
        .expect("note route ok");
        route_response(
            "POST",
            "/forge/machine-league/native-improvement-loops.json",
            r#"{"distillation_note_id":"distillation_note_policy_edge","launch_execution":false}"#,
            &kernel,
        )
        .expect("loop route handled")
        .expect("loop route ok");

        let trial = route_response(
            "POST",
            "/forge/machine-league/native-trials.json",
            r#"{"run_id":"forge_machine_league_native_policy_variant_after_learning","intent":"remeasure native on the sibling policy fixture","eval_fixture":"rust_feature_policy_edge_variant","execute_live":true,"native_execution":"scripted"}"#,
            &kernel,
        )
        .expect("native route handled")
        .expect("native route ok");

        assert!(
            trial
                .body
                .contains(r#""native_strategy_profile":"cross_file_symbol_policy_first""#)
        );
        assert!(
            trial
                .body
                .contains(r#""eval_fixture":"rust_feature_policy_edge_variant""#)
        );
        assert!(trial.body.contains(r#""visible_check_passed":true"#));
        assert!(trial.body.contains(r#""hidden_check_passed":true"#));
        assert!(trial.body.contains(r#""check_passed":true"#));
    }

    #[test]
    fn learning_ledger_is_evidence_backed_and_fails_closed_without_accepted_sample() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_response(
            "POST",
            "/forge/machine-league/codex-trials.json",
            r#"{"intent":"compare Codex CLI on a small Rust fix","task_corpus_id":"rust-cargo-bug-fix-small"}"#,
            &kernel,
        )
        .expect("trial route handled")
        .expect("trial route ok");

        let note = route_response(
            "POST",
            "/forge/machine-league/distillation-notes.json",
            r#"{"source_run_id":"forge_machine_league_codex_trial_000001","winning_runner_id":"codex_cli_external_worker","baseline_runner_id":"mdx_native_harness_runner","task_corpus_id":"rust-cargo-bug-fix-small","reason":"planning_strategy"}"#,
            &kernel,
        )
        .expect("note route handled")
        .expect("note route ok");
        assert!(note.body.contains(r#""status":"REFUSED""#));
        assert!(
            note.body
                .contains("source run forge_machine_league_codex_trial_000001 is not accepted external scoreboard evidence")
        );
        assert!(note.body.contains(r#""external_output_consumable":false"#));

        let ledger = route_response(
            "GET",
            "/forge/machine-league/learning-ledger.json",
            "",
            &kernel,
        )
        .expect("ledger route handled")
        .expect("ledger route ok");

        assert!(
            ledger
                .body
                .contains(r#""status":"MACHINE_LEAGUE_LEARNING_LEDGER_READY""#)
        );
        assert!(ledger.body.contains(r#""entry_count":0"#));
        assert!(
            ledger
                .body
                .contains(r#""pending_distillation_candidate_count":0"#)
        );
        assert!(
            ledger
                .body
                .contains(r#""accepted_scoreboard_results_only":true"#)
        );
        assert!(ledger.body.contains(r#""dry_run_claims_allowed":false"#));
        assert!(
            ledger
                .body
                .contains(r#""runner_self_attestation_allowed":false"#)
        );
    }

    #[test]
    fn native_improvement_loop_schedules_distillation_work_without_claiming_lift() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        seed_accepted_machine_league_score(
            &kernel,
            "forge_machine_league_codex_trial_route_test",
            "codex_cli_external_worker",
            97,
        );
        seed_accepted_machine_league_score(
            &kernel,
            "forge_machine_league_native_trial_route_test",
            "mdx_native_harness_runner",
            94,
        );
        seed_external_win_pairwise(
            &kernel,
            "forge_machine_league_codex_trial_route_test",
            "forge_machine_league_native_trial_route_test",
        );
        route_response(
            "POST",
            "/forge/machine-league/distillation-notes.json",
            r#"{"distillation_note_id":"distillation_note_route_test","source_run_id":"forge_machine_league_codex_trial_route_test","winning_runner_id":"codex_cli_external_worker","baseline_runner_id":"mdx_native_harness_runner","distillation_reason":"tool_strategy","language_pack_id":"rust-cargo","task_class":"bug_fix","confidence":"low"}"#,
            &kernel,
        )
        .expect("note route handled")
        .expect("note route ok");

        let loop_response = route_response(
            "POST",
            "/forge/machine-league/native-improvement-loops.json",
            r#"{"distillation_note_id":"distillation_note_route_test","launch_execution":false}"#,
            &kernel,
        )
        .expect("native improvement route handled")
        .expect("native improvement route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&loop_response.body).expect("native improvement json");

        assert_eq!(
            parsed["status"].as_str(),
            Some("NATIVE_IMPROVEMENT_LOOP_TICK_RECORDED")
        );
        assert_eq!(
            parsed["native_improvement_work_item_id"].as_str(),
            Some("mdx_native_harness_improve_tool_strategy")
        );
        assert_eq!(parsed["builder_loop_id"].as_str(), Some("quality_sweep"));
        assert_eq!(
            parsed["builder_loop_launch_execution_requested"].as_bool(),
            Some(false)
        );
        assert_eq!(
            parsed["remeasurement_route"].as_str(),
            Some("/forge/machine-league/native-trials.json")
        );
        assert_eq!(parsed["after_native_accepted_count"].as_u64(), Some(0));
        assert_eq!(parsed["external_output_consumable"].as_bool(), Some(false));
        assert_eq!(parsed["production_write_allowed"].as_bool(), Some(false));

        let projection = route_response(
            "GET",
            "/forge/machine-league/native-improvement-loops/projection.json",
            "",
            &kernel,
        )
        .expect("projection route handled")
        .expect("projection route ok");
        assert!(projection.body.contains(r#""loop_count":1"#));
        assert!(
            projection
                .body
                .contains(r#""distillation_note_id":"distillation_note_route_test""#)
        );

        let ledger = route_response(
            "GET",
            "/forge/machine-league/learning-ledger.json",
            "",
            &kernel,
        )
        .expect("ledger route handled")
        .expect("ledger route ok");
        assert!(
            ledger
                .body
                .contains(r#""native_improvement_loop_status":"BUILDER_LOOP_TICK_RECORDED""#)
        );
        assert!(
            ledger.body.contains(
                r#""remeasurement_status":"waiting_for_operator_to_launch_builder_loop""#
            )
        );
        assert!(ledger.body.contains(r#""after_win_rate_pct":null"#));
    }

    #[test]
    fn native_runtime_adaptation_profile_is_inert_without_ratified_grants() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let profile = route_response(
            "GET",
            "/forge/machine-league/native-runtime-adaptations.json",
            "",
            &kernel,
        )
        .expect("profile route handled")
        .expect("profile route ok");
        let parsed: serde_json::Value = serde_json::from_str(&profile.body).expect("profile json");

        assert_eq!(
            parsed["status"].as_str(),
            Some("NO_RATIFIED_NATIVE_RUNTIME_ADAPTATIONS")
        );
        assert_eq!(parsed["adaptation_count"].as_u64(), Some(0));
        assert_eq!(
            parsed["runtime_behavior_change_allowed"].as_bool(),
            Some(false)
        );
        assert_eq!(parsed["production_write_allowed"].as_bool(), Some(false));

        let recommendations = route_response(
            "GET",
            "/forge/machine-league/recommendations.json",
            "",
            &kernel,
        )
        .expect("recommendation route handled")
        .expect("recommendation route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&recommendations.body).expect("recommendation json");
        assert_eq!(
            parsed["native_runtime_adaptation_status"].as_str(),
            Some("NO_RATIFIED_NATIVE_RUNTIME_ADAPTATIONS")
        );
        assert_eq!(
            parsed["native_runtime_adaptation_applied_to_recommendation"].as_bool(),
            Some(false)
        );
    }

    #[test]
    fn native_runtime_adaptation_grant_is_applied_to_machine_league_fleet_casts() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let grant_receipt_id = seed_native_runtime_adaptation_grant(&kernel);

        let profile = route_response(
            "GET",
            "/forge/machine-league/native-runtime-adaptations.json",
            "",
            &kernel,
        )
        .expect("profile route handled")
        .expect("profile route ok");
        let parsed: serde_json::Value = serde_json::from_str(&profile.body).expect("profile json");
        assert_eq!(
            parsed["status"].as_str(),
            Some("NATIVE_RUNTIME_ADAPTATION_ACTIVE")
        );
        assert_eq!(parsed["adaptation_count"].as_u64(), Some(1));
        assert_eq!(
            parsed["runtime_behavior_change_allowed"].as_bool(),
            Some(true)
        );
        assert_eq!(
            parsed["adaptation_grant_receipt_ids"].as_str(),
            Some(grant_receipt_id.as_str())
        );

        let cast = route_response(
            "POST",
            "/forge/machine-league/fleet-casts.json",
            r#"{"worker_count":1,"policy_profile":"sensitive_local_only","task_class":"bug_fix","language_pack_id":"rust-cargo"}"#,
            &kernel,
        )
        .expect("cast route handled")
        .expect("cast route ok");
        let parsed: serde_json::Value = serde_json::from_str(&cast.body).expect("cast json");
        let plan_id = parsed["plan_id"].as_str().expect("plan id");
        let first_cast = &parsed["worker_casts"][0];

        assert_eq!(
            parsed["native_runtime_adaptation_status"].as_str(),
            Some("NATIVE_RUNTIME_ADAPTATION_ACTIVE")
        );
        assert_eq!(parsed["native_runtime_adaptation_count"].as_u64(), Some(1));
        assert_eq!(
            parsed["native_runtime_adaptation_grant_receipt_ids"].as_str(),
            Some(grant_receipt_id.as_str())
        );
        assert_eq!(
            first_cast["runner_id"].as_str(),
            Some("mdx_native_harness_runner")
        );
        assert_eq!(
            first_cast["native_runtime_adaptation_applied"].as_bool(),
            Some(true)
        );
        assert_eq!(
            first_cast["native_runtime_adaptation_grant_receipt_ids"].as_str(),
            Some(grant_receipt_id.as_str())
        );

        let guard = kernel.read().expect("kernel");
        let applied = guard
            .ledger()
            .query()
            .by_kind("learning.adaptation.applied")
            .into_iter()
            .find(|receipt| {
                payload_value(receipt, "grant_receipt_id") == grant_receipt_id
                    && payload_value(receipt, "artifact_kind") == "machine_league_fleet_cast"
                    && payload_value(receipt, "artifact_id") == plan_id
            })
            .expect("adaptation applied receipt");
        assert_eq!(
            payload_value(applied, "detail"),
            "ratified native runtime adaptation informed Machine League fleet cast planning"
        );
    }

    #[test]
    fn learning_ledger_surfaces_pending_distillation_candidates_without_faking_learning() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event_with_evidence_fields(
                ev(
                    "forge_machine_league_codex_trial_accepted",
                    "evidence_appended",
                    "eval_principal_reviewed status=accepted_for_scoreboard",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("agent:forge_eval"),
                &[
                    ("accepted_for_scoreboard", "true"),
                    ("runner_id", "codex_cli_external_worker"),
                    ("winning_runner_id", "codex_cli_external_worker"),
                    ("baseline_runner_id", "mdx_native_harness_runner"),
                    ("machine_league_eval_fixture", "rust_tax_rounding"),
                    ("language_task_class", "bug_fix"),
                    ("language_pack_id", "rust-cargo"),
                    ("total_score", "94"),
                ],
            )
            .expect("accepted event");

        let ledger = render_learning_ledger_json(&kernel);
        let parsed: serde_json::Value =
            serde_json::from_str(&ledger).expect("valid learning ledger");

        assert_eq!(parsed["entry_count"].as_u64(), Some(0));
        assert_eq!(
            parsed["pending_distillation_candidate_count"].as_u64(),
            Some(1)
        );
        assert_eq!(
            parsed["pending_distillation_candidates"][0]["status"].as_str(),
            Some("pending_native_comparison_and_distillation_note")
        );
        assert_eq!(
            parsed["pending_distillation_candidates"][0]["external_output_consumable"].as_bool(),
            Some(false)
        );
    }

    #[test]
    fn learning_ledger_marks_same_fixture_native_comparison_available() {
        let mut kernel = MdxKernel::boot_local();
        for (run_id, runner_id) in [
            (
                "forge_machine_league_codex_trial_accepted",
                "codex_cli_external_worker",
            ),
            (
                "forge_machine_league_native_trial_accepted",
                "mdx_native_harness_runner",
            ),
        ] {
            kernel
                .record_forge_run_event_with_evidence_fields(
                    ev(
                        run_id,
                        "evidence_appended",
                        "eval_principal_reviewed status=accepted_for_scoreboard",
                    ),
                    &mdx_core::GovernedWriteIdentity::local_demo("agent:forge_eval"),
                    &[
                        ("accepted_for_scoreboard", "true"),
                        ("runner_id", runner_id),
                        ("winning_runner_id", runner_id),
                        ("baseline_runner_id", "mdx_native_harness_runner"),
                        ("machine_league_eval_fixture", "rust_tax_rounding"),
                        ("language_task_class", "bug_fix"),
                        ("language_pack_id", "rust-cargo"),
                        ("total_score", "94"),
                    ],
                )
                .expect("accepted event");
        }

        let ledger = render_learning_ledger_json(&kernel);
        let parsed: serde_json::Value =
            serde_json::from_str(&ledger).expect("valid learning ledger");

        assert_eq!(parsed["entry_count"].as_u64(), Some(0));
        assert_eq!(
            parsed["pending_distillation_candidate_count"].as_u64(),
            Some(1)
        );
        assert_eq!(
            parsed["pending_distillation_candidates"][0]["status"].as_str(),
            Some("native_comparison_available_pending_distillation_decision")
        );
        assert_eq!(
            parsed["pending_distillation_candidates"][0]["native_same_fixture_accepted_count"]
                .as_u64(),
            Some(1)
        );
    }

    #[test]
    fn distillation_note_default_work_item_tracks_reason() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        seed_accepted_machine_league_score(
            &kernel,
            "forge_machine_league_codex_trial_000001",
            "codex_cli_external_worker",
            97,
        );
        seed_accepted_machine_league_score(
            &kernel,
            "forge_machine_league_native_trial_for_model_capability",
            "mdx_native_harness_runner",
            94,
        );
        seed_external_win_pairwise(
            &kernel,
            "forge_machine_league_codex_trial_000001",
            "forge_machine_league_native_trial_for_model_capability",
        );
        let note = route_response(
            "POST",
            "/forge/machine-league/distillation-notes.json",
            r#"{"source_run_id":"forge_machine_league_codex_trial_000001","winning_runner_id":"codex_cli_external_worker","baseline_runner_id":"mdx_native_harness_runner","distillation_reason":"model_capability"}"#,
            &kernel,
        )
        .expect("note route handled")
        .expect("note route ok");

        assert!(
            note.body
                .contains(r#""distillation_reason":"model_capability""#)
        );
        assert!(note.body.contains(
            r#""native_improvement_work_item_id":"mdx_native_harness_improve_model_capability""#
        ));
    }

    #[test]
    fn result_from_run_refuses_unknown_run_without_inventing_eval_evidence() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/forge/fleet-eval-results/from-run.json",
            r#"{"run_id":"forge_run_missing"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains(r#""accepted_for_scoreboard":false"#));
        assert_eq!(
            kernel
                .read()
                .expect("kernel")
                .ledger()
                .query()
                .by_kind("forge.fleet_eval_result.ingested")
                .len(),
            0
        );
    }

    #[test]
    fn result_from_run_records_quarantined_eval_evidence_from_review_packet() {
        let kernel = seeded_eval_bridge_run("forge_run_eval_bridge");

        let response = route_response(
            "POST",
            "/forge/fleet-eval-results/from-run.json",
            r#"{"run_id":"forge_run_eval_bridge"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            response
                .body
                .contains(r#""status":"FLEET_EVAL_RESULT_QUARANTINED_PENDING_MDX_GATES""#)
        );
        assert!(
            response
                .body
                .contains(r#""source_run_id":"forge_run_eval_bridge""#)
        );
        assert!(
            response
                .body
                .contains(r#""language_task_corpus_id":"swift-spm-feature-medium""#)
        );
        assert!(
            response
                .body
                .contains(r#""model_profile_id":"codex_xai_responses_profile""#)
        );
        assert!(response
            .body
            .contains(r#""standards_source_fingerprints":"AGENTS.md=fnv1a64:1111111111111111,CLAUDE.md=fnv1a64:bae8bb8f209720a8,.github/CODEOWNERS=fnv1a64:2222222222222222""#));
        assert!(response.body.contains(r#""output_quarantined":true"#));
        assert!(response.body.contains(r#""accepted_for_scoreboard":false"#));
        assert!(response.body.contains(r#""machine_quality_gates_passed":"#));
        assert!(response.body.contains(r#""mdx_quality_gate_assessment":{"generated_from":"forge_review_packet.local_machine_quality_gates""#));
        assert!(
            response
                .body
                .contains(r#""accepted_for_scoreboard_after_principal_review":false"#)
        );
        assert_eq!(
            kernel
                .read()
                .expect("kernel")
                .ledger()
                .query()
                .by_kind("forge.fleet_eval_result.ingested")
                .len(),
            1
        );
        let guard = kernel.read().expect("kernel");
        let quality_event = guard
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .into_iter()
            .find(|receipt| {
                receipt
                    .payload
                    .get("detail")
                    .map(|detail| detail.contains("eval_quality_gate_assessed"))
                    .unwrap_or(false)
            })
            .expect("quality gate event");
        assert_eq!(
            quality_event
                .payload
                .get("eval_quality_gate_status")
                .map(String::as_str),
            Some("BLOCKED_BEFORE_PRINCIPAL_REVIEW")
        );
        assert_eq!(
            quality_event
                .payload
                .get("eval_machine_quality_gates_passed")
                .map(String::as_str),
            Some("false")
        );
        assert_eq!(
            quality_event
                .payload
                .get("eval_accepts_for_scoreboard")
                .map(String::as_str),
            Some("false")
        );
    }

    #[test]
    fn eval_task_follows_the_diff_language_not_the_repo() {
        let corpus = forge_language_task_corpus();
        let rust_task = corpus
            .iter()
            .find(|task| task.language_pack_id == "rust-cargo")
            .expect("corpus has a rust-cargo task");

        // A run aligned at start to a rust-cargo task, but whose diff is node,
        // now resolves to a node eval task instead of the rejected rust one.
        let node_diff = serde_json::json!({
            "language_task_alignment": { "task_corpus_id": rust_task.task_corpus_id },
            "diff": { "language_pack_impact": [
                { "language_pack_id": "node" },
                { "language_pack_id": "node" }
            ]}
        });
        let resolved = resolve_diff_aligned_language_task(&node_diff, &corpus)
            .expect("node diff resolves to a node task");
        assert_eq!(resolved.language_pack_id, "node");

        // A rust diff in the same repo keeps the rust alignment unchanged.
        let rust_diff = serde_json::json!({
            "language_task_alignment": { "task_corpus_id": rust_task.task_corpus_id },
            "diff": { "language_pack_impact": [ { "language_pack_id": "rust-cargo" } ]}
        });
        let kept = resolve_diff_aligned_language_task(&rust_diff, &corpus)
            .expect("rust diff keeps rust task");
        assert_eq!(kept.task_corpus_id, rust_task.task_corpus_id);

        // A docs-only diff (no language pack) falls back to the run-start
        // alignment rather than refusing.
        let docs_diff = serde_json::json!({
            "language_task_alignment": { "task_corpus_id": rust_task.task_corpus_id },
            "diff": { "language_pack_impact": [] }
        });
        let fell_back = resolve_diff_aligned_language_task(&docs_diff, &corpus)
            .expect("docs-only diff falls back to alignment");
        assert_eq!(fell_back.task_corpus_id, rust_task.task_corpus_id);
    }

    #[test]
    fn diff_language_pack_impact_returns_plain_pack_ids_for_core_ingestion() {
        let packet = serde_json::json!({
            "diff": {
                "language_pack_impact": [
                    {"language_pack_id": "python", "file_count": 1, "generated_count": 0},
                    {"language_pack_id": "node", "file_count": 2, "generated_count": 1}
                ]
            }
        });
        let language_task = mdx_core::forge_language_task_corpus()
            .into_iter()
            .find(|task| task.language_pack_id == "python")
            .expect("python task");

        assert_eq!(
            diff_language_pack_impact(&packet, &language_task),
            "python,node"
        );
    }

    #[test]
    fn run_eval_quality_gate_assessment_names_ready_and_blocked_gates() {
        let ready_packet = serde_json::json!({
            "status": "OK",
            "review_status": "ready_for_review",
            "diff": {
                "real_change_count": 1,
                "artifact_summary": "1 artifact folded"
            },
            "proof_coverage": {
                "status": "satisfied"
            },
            "proof_scope": {
                "status": "covered"
            },
            "behavior_proof": {
                "status": "covered"
            },
            "repo_intelligence": {
                "principal_orientation_gate": {
                    "required": true,
                    "observed": true
                },
                "related_tests_observed": true,
                "semantic_query_operations_observed": ["symbol_graph", "dependency_map", "related_tests"]
            },
            "candidate_comparison": {
                "candidate_count": 4,
                "current_run_id": "forge_run_candidate_1",
                "recommended_run_id": "forge_run_candidate_1"
            }
        });

        let ready = run_eval_quality_gate_assessment(&ready_packet);

        assert!(ready.gates_passed);
        assert_eq!(ready.status, "READY_FOR_PRINCIPAL_REVIEW");
        assert!(ready.failed_gates.is_empty());
        assert!(
            ready
                .passed_gates
                .contains(&"dependency_map_observed_for_parallel_candidates")
        );

        let mut blocked_packet = ready_packet;
        blocked_packet["proof_coverage"]["status"] = serde_json::json!("missing");
        blocked_packet["candidate_comparison"]["current_run_id"] =
            serde_json::json!("forge_run_candidate_3");
        blocked_packet["repo_intelligence"]["semantic_query_operations_observed"] =
            serde_json::json!(["symbol_graph", "related_tests"]);
        let blocked = run_eval_quality_gate_assessment(&blocked_packet);

        assert!(!blocked.gates_passed);
        assert_eq!(blocked.status, "BLOCKED_BEFORE_PRINCIPAL_REVIEW");
        assert!(blocked.failed_gates.contains(&"proof_coverage_satisfied"));
        assert!(
            blocked
                .failed_gates
                .contains(&"dependency_map_observed_for_parallel_candidates")
        );
        assert!(
            blocked
                .failed_gates
                .contains(&"recommended_candidate_selected")
        );
        assert!(
            blocked
                .to_json()
                .contains("accepted_for_scoreboard\":false")
        );
    }

    #[test]
    fn run_eval_evidence_helper_records_same_quarantined_bridge_for_worker_threads() {
        let kernel = seeded_eval_bridge_run("forge_run_eval_helper");

        let report = record_result_from_run_evidence(&kernel, "forge_run_eval_helper")
            .expect("helper submits eval evidence");

        assert_eq!(
            report.status,
            "FLEET_EVAL_RESULT_QUARANTINED_PENDING_MDX_GATES"
        );
        assert!(report.reason.is_empty());
        assert!(!report.result_receipt_id.is_empty());
        assert_eq!(
            kernel
                .read()
                .expect("kernel")
                .ledger()
                .query()
                .by_kind("forge.fleet_eval_result.ingested")
                .len(),
            1
        );
    }

    #[test]
    fn principal_review_promotes_machine_gated_run_to_scorecard_evidence() {
        let kernel = seeded_eval_bridge_run("forge_run_principal_review");
        seed_quality_ready(&kernel, "forge_run_principal_review", "result_ready_1");

        let response = route_response(
            "POST",
            "/forge/fleet-eval-results/principal-reviews.json",
            r#"{"run_id":"forge_run_principal_review","result_receipt_id":"result_ready_1","principal_engineer_verdict":"accepted_for_scoreboard"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            response
                .body
                .contains(r#""status":"ACCEPTED_FOR_SCOREBOARD""#)
        );
        assert!(response.body.contains(r#""accepted_for_scoreboard":true"#));
        assert!(response.body.contains(r#""language_pack_id":"swift-spm""#));
        assert!(
            response
                .body
                .contains(r#""model_profile_id":"codex_xai_responses_profile""#)
        );
        let guard = kernel.read().expect("kernel");
        let review_event = guard
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .into_iter()
            .find(|receipt| {
                receipt
                    .payload
                    .get("eval_principal_review_status")
                    .map(String::as_str)
                    == Some("accepted_for_scoreboard")
            })
            .expect("principal review event");
        assert_eq!(
            review_event
                .payload
                .get("accepted_for_scoreboard")
                .map(String::as_str),
            Some("true")
        );
        assert_eq!(
            review_event.payload.get("total_score").map(String::as_str),
            Some("100")
        );
    }

    #[test]
    fn principal_review_refuses_drifted_external_runtime_without_smoke() {
        let kernel = seeded_eval_bridge_run("forge_run_runtime_drift_without_smoke");
        seed_external_runtime(
            &kernel,
            "forge_run_runtime_drift_without_smoke",
            "codex_cli_external_worker",
            "codex_cli",
            "sha256:runtime-drift-without-smoke",
            "runtime_drift_detected",
        );
        seed_quality_ready(
            &kernel,
            "forge_run_runtime_drift_without_smoke",
            "result_ready_drift_1",
        );

        let response = route_response(
            "POST",
            "/forge/fleet-eval-results/principal-reviews.json",
            r#"{"run_id":"forge_run_runtime_drift_without_smoke","result_receipt_id":"result_ready_drift_1","principal_engineer_verdict":"accepted_for_scoreboard"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(
            response
                .body
                .contains("machine runtime drift requires a passed runtime smoke")
        );
        assert!(response.body.contains(r#""accepted_for_scoreboard":false"#));
    }

    #[test]
    fn principal_review_accepts_drifted_external_runtime_after_smoke() {
        let kernel = seeded_eval_bridge_run("forge_run_runtime_drift_with_smoke");
        seed_external_runtime(
            &kernel,
            "forge_run_runtime_drift_with_smoke",
            "codex_cli_external_worker",
            "codex_cli",
            "sha256:runtime-drift-with-smoke",
            "runtime_drift_detected",
        );
        seed_runtime_smoke(
            &kernel,
            "codex_cli_external_worker",
            "codex_cli",
            "sha256:runtime-drift-with-smoke",
        );
        seed_quality_ready(
            &kernel,
            "forge_run_runtime_drift_with_smoke",
            "result_ready_drift_2",
        );

        let response = route_response(
            "POST",
            "/forge/fleet-eval-results/principal-reviews.json",
            r#"{"run_id":"forge_run_runtime_drift_with_smoke","result_receipt_id":"result_ready_drift_2","principal_engineer_verdict":"accepted_for_scoreboard"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            response
                .body
                .contains(r#""status":"ACCEPTED_FOR_SCOREBOARD""#)
        );
        assert!(
            response
                .body
                .contains(r#""runner_id":"codex_cli_external_worker""#)
        );
        assert!(response.body.contains(r#""accepted_for_scoreboard":true"#));
    }

    #[test]
    fn principal_review_refuses_fixture_acceptance_without_hidden_oracle() {
        let kernel = seeded_eval_bridge_run("forge_run_fixture_without_hidden_oracle");
        seed_quality_ready(
            &kernel,
            "forge_run_fixture_without_hidden_oracle",
            "result_ready_fixture_no_oracle",
        );
        seed_fixture_oracle(
            &kernel,
            "forge_run_fixture_without_hidden_oracle",
            FixtureOracleSeed {
                eval_oracle_status: "hidden_check_not_observed",
                visible_check_passed: "true",
                hidden_check_observed: "false",
                hidden_check_passed: "false",
                contamination_status: "fresh_held_out_fixture",
                eval_fixture_snapshot_id: MACHINE_LEAGUE_RUST_TAX_FIXTURE_SNAPSHOT_ID,
            },
        );

        let response = route_response(
            "POST",
            "/forge/fleet-eval-results/principal-reviews.json",
            r#"{"run_id":"forge_run_fixture_without_hidden_oracle","result_receipt_id":"result_ready_fixture_no_oracle","principal_engineer_verdict":"accepted_for_scoreboard"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(
            response
                .body
                .contains("requires visible and hidden oracle checks to pass")
        );
        assert!(response.body.contains(r#""accepted_for_scoreboard":false"#));
    }

    #[test]
    fn principal_review_accepts_fixture_with_fresh_hidden_oracle() {
        let kernel = seeded_eval_bridge_run("forge_run_fixture_with_hidden_oracle");
        seed_quality_ready(
            &kernel,
            "forge_run_fixture_with_hidden_oracle",
            "result_ready_fixture_oracle",
        );
        seed_fixture_oracle(
            &kernel,
            "forge_run_fixture_with_hidden_oracle",
            FixtureOracleSeed {
                eval_oracle_status: "visible_and_hidden_checks_passed",
                visible_check_passed: "true",
                hidden_check_observed: "true",
                hidden_check_passed: "true",
                contamination_status: "fresh_held_out_fixture",
                eval_fixture_snapshot_id: MACHINE_LEAGUE_RUST_TAX_FIXTURE_SNAPSHOT_ID,
            },
        );

        let response = route_response(
            "POST",
            "/forge/fleet-eval-results/principal-reviews.json",
            r#"{"run_id":"forge_run_fixture_with_hidden_oracle","result_receipt_id":"result_ready_fixture_oracle","principal_engineer_verdict":"accepted_for_scoreboard"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            response
                .body
                .contains(r#""status":"ACCEPTED_FOR_SCOREBOARD""#)
        );
        assert!(response.body.contains(r#""accepted_for_scoreboard":true"#));
        let guard = kernel.read().expect("kernel");
        let review_event = guard
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .into_iter()
            .find(|receipt| {
                payload_value(receipt, "run_id") == "forge_run_fixture_with_hidden_oracle"
                    && payload_value(receipt, "accepted_for_scoreboard") == "true"
            })
            .expect("accepted review event");
        assert_eq!(
            payload_value(review_event, "eval_oracle_status"),
            "visible_and_hidden_checks_passed"
        );
        assert_eq!(
            payload_value(review_event, "contamination_status"),
            "fresh_held_out_fixture"
        );
    }

    #[test]
    fn accepted_score_count_does_not_treat_baseline_as_winner() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event_with_evidence_fields(
                ev(
                    "forge_run_codex_win",
                    "evidence_appended",
                    "eval_principal_reviewed status=accepted_for_scoreboard",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("agent:forge_eval"),
                &[
                    ("accepted_for_scoreboard", "true"),
                    ("runner_id", "codex_cli_external_worker"),
                    ("winning_runner_id", "codex_cli_external_worker"),
                    ("baseline_runner_id", "mdx_native_harness_runner"),
                ],
            )
            .expect("accepted event");

        assert_eq!(
            accepted_score_count_for_runner(&kernel, "codex_cli_external_worker"),
            1
        );
        assert_eq!(
            accepted_score_count_for_runner(&kernel, "mdx_native_harness_runner"),
            0
        );
    }

    #[test]
    fn scripted_baseline_receipts_never_accrue_league_standing() {
        let mut kernel = MdxKernel::boot_local();
        let identity = mdx_core::GovernedWriteIdentity::local_demo("agent:forge_native");
        // Two trial starts count: the real harness run and an unlabeled legacy
        // one. The scripted rehearsal is excluded.
        for (run_id, fields) in [
            (
                "native_trial_harness",
                &[
                    ("machine_league_trial", "true"),
                    ("runner_profile_runner_id", "mdx_native_harness_runner"),
                    ("native_execution_mode", "harness"),
                ][..],
            ),
            (
                "native_trial_legacy",
                &[
                    ("machine_league_trial", "true"),
                    ("runner_profile_runner_id", "mdx_native_harness_runner"),
                ][..],
            ),
            (
                "native_trial_scripted",
                &[
                    ("machine_league_trial", "true"),
                    ("runner_profile_runner_id", "mdx_native_harness_runner"),
                    ("native_execution_mode", "scripted_baseline"),
                ][..],
            ),
        ] {
            kernel
                .record_forge_run_event_with_evidence_fields(
                    ev(run_id, "run_started", "native trial started"),
                    &identity,
                    fields,
                )
                .expect("trial start");
        }
        assert_eq!(
            league_trial_count_for_runner(&kernel, "mdx_native_harness_runner"),
            2
        );

        // Same law on accepted-score receipts: a scripted-baseline acceptance
        // never counts, an unlabeled or harness one does.
        for (run_id, mode) in [
            ("native_accept_harness", Some("harness")),
            ("native_accept_scripted", Some("scripted_baseline")),
        ] {
            let mut fields = vec![
                ("accepted_for_scoreboard", "true"),
                ("runner_id", "mdx_native_harness_runner"),
                ("winning_runner_id", "mdx_native_harness_runner"),
            ];
            if let Some(mode) = mode {
                fields.push(("native_execution_mode", mode));
            }
            kernel
                .record_forge_run_event_with_evidence_fields(
                    ev(run_id, "evidence_appended", "accepted for scoreboard"),
                    &identity,
                    &fields,
                )
                .expect("accepted event");
        }
        assert_eq!(
            accepted_score_count_for_runner(&kernel, "mdx_native_harness_runner"),
            1
        );
    }

    #[test]
    fn native_trial_scripted_mode_is_labeled_and_off_scoreboard() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let trial = route_response(
            "POST",
            "/forge/machine-league/native-trials.json",
            r#"{"run_id":"forge_native_trial_scripted_label_test","eval_fixture":"rust_tax_rounding","execute_live":true,"native_execution":"scripted"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            trial
                .body
                .contains(r#""native_execution_mode":"scripted_baseline""#)
        );
        assert!(
            trial
                .body
                .contains(r#""scripted_baseline_not_scoreboard_evidence":true"#)
        );
    }

    #[test]
    fn native_trial_harness_mode_reports_honestly_without_a_live_model() {
        // Default (no native_execution) selects the real harness loop. Tests
        // have no live build model, so the loop cannot start; the trial must
        // still answer honestly as a 200 JSON labeled with the harness mode.
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let trial = route_response(
            "POST",
            "/forge/machine-league/native-trials.json",
            r#"{"run_id":"forge_native_trial_harness_honest_test","eval_fixture":"rust_tax_rounding","execute_live":true}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");
        let parsed: serde_json::Value =
            serde_json::from_str(&trial.body).expect("harness trial json");
        assert_eq!(parsed["native_execution_mode"].as_str(), Some("harness"));
    }

    #[test]
    fn scoreboard_runner_scores_overlay_live_accepted_results() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event_with_evidence_fields(
                ev(
                    "forge_run_codex_win",
                    "evidence_appended",
                    "eval_principal_reviewed status=accepted_for_scoreboard",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("agent:forge_eval"),
                &[
                    ("accepted_for_scoreboard", "true"),
                    ("runner_id", "codex_cli_external_worker"),
                    ("winning_runner_id", "codex_cli_external_worker"),
                    ("baseline_runner_id", "mdx_native_harness_runner"),
                ],
            )
            .expect("accepted event");
        let kernel = Arc::new(RwLock::new(kernel));
        seed_external_runtime(
            &kernel,
            "forge_run_codex_win",
            "codex_cli_external_worker",
            "codex_cli",
            "sha256:scorecard-runtime",
            "runtime_stable",
        );
        seed_runtime_smoke(
            &kernel,
            "codex_cli_external_worker",
            "codex_cli",
            "sha256:scorecard-runtime",
        );

        let guard = kernel.read().expect("kernel");
        let body =
            render_forge_fleet_eval_scoreboard_json_with_kernel(Some(&guard)).expect("scoreboard");
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid scoreboard json");
        let codex_score = parsed["runner_scores"]
            .as_array()
            .expect("runner scores")
            .iter()
            .find(|score| score["runner_id"].as_str() == Some("codex_cli_external_worker"))
            .expect("codex score");
        let native_score = parsed["runner_scores"]
            .as_array()
            .expect("runner scores")
            .iter()
            .find(|score| score["runner_id"].as_str() == Some("mdx_native_harness_runner"))
            .expect("native score");

        assert_eq!(codex_score["accepted"].as_u64(), Some(1));
        // One accepted result is evidence, not standing: below the trial
        // floor the runner is not scoreboard-eligible and no winner is
        // declared from a single lucky run.
        assert_eq!(codex_score["tasks_attempted"].as_u64(), Some(1));
        assert_eq!(codex_score["pass_rate_pct"].as_u64(), Some(100));
        assert_eq!(codex_score["scoreboard_eligible"].as_bool(), Some(false));
        assert_eq!(codex_score["min_trials_required"].as_u64(), Some(3));
        // The native runner earned nothing on this ledger: zero evidence,
        // zero standing - the canned 100% baseline is gone.
        assert_eq!(native_score["accepted"].as_u64(), Some(0));
        assert_eq!(native_score["tasks_attempted"].as_u64(), Some(0));
        assert_eq!(native_score["pass_rate_pct"].as_u64(), Some(0));
        assert_eq!(parsed["winning_runner_id"].as_str(), Some(""));
        assert_eq!(
            codex_score["runtime_fingerprint_id"].as_str(),
            Some("sha256:scorecard-runtime")
        );
        assert_eq!(
            codex_score["runtime_drift_status"].as_str(),
            Some("runtime_stable")
        );
        assert_eq!(
            codex_score["runtime_smoke_status"].as_str(),
            Some("runtime_smoke_passed")
        );
        assert_eq!(codex_score["runtime_smoke_passed"].as_bool(), Some(true));
        assert_eq!(
            native_score["runtime_smoke_status"].as_str(),
            Some("runtime_smoke_not_required")
        );
    }

    #[test]
    fn task_class_scorecards_report_confidence_and_same_fixture_samples() {
        let mut kernel = MdxKernel::boot_local();
        for (run_id, runner_id) in [
            (
                "forge_machine_league_codex_trial_accepted",
                "codex_cli_external_worker",
            ),
            (
                "forge_machine_league_native_trial_accepted",
                "mdx_native_harness_runner",
            ),
        ] {
            kernel
                .record_forge_run_event_with_evidence_fields(
                    ev(
                        run_id,
                        "evidence_appended",
                        "eval_principal_reviewed status=accepted_for_scoreboard",
                    ),
                    &mdx_core::GovernedWriteIdentity::local_demo("agent:forge_eval"),
                    &[
                        ("accepted_for_scoreboard", "true"),
                        ("runner_id", runner_id),
                        ("winning_runner_id", runner_id),
                        ("baseline_runner_id", "mdx_native_harness_runner"),
                        ("language_task_class", "bug_fix"),
                        ("machine_league_eval_fixture", "rust_tax_rounding"),
                    ],
                )
                .expect("accepted event");
        }

        let body =
            render_forge_fleet_eval_scoreboard_json_with_kernel(Some(&kernel)).expect("scoreboard");
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid scoreboard json");
        let scorecards = parsed["task_class_scorecards"]
            .as_array()
            .expect("task class scorecards");
        let codex_bug_fix = scorecards
            .iter()
            .find(|score| {
                score["task_class"].as_str() == Some("bug_fix")
                    && score["runner_id"].as_str() == Some("codex_cli_external_worker")
            })
            .expect("codex bug fix scorecard");
        let native_bug_fix = scorecards
            .iter()
            .find(|score| {
                score["task_class"].as_str() == Some("bug_fix")
                    && score["runner_id"].as_str() == Some("mdx_native_harness_runner")
            })
            .expect("native bug fix scorecard");

        assert_eq!(codex_bug_fix["confidence"].as_str(), Some("directional"));
        assert_eq!(
            codex_bug_fix["same_fixture_comparison_count"].as_u64(),
            Some(1)
        );
        assert_eq!(
            codex_bug_fix["normalized_metric_status"].as_str(),
            Some("normalized")
        );
        assert_eq!(
            native_bug_fix["confidence"].as_str(),
            Some("insufficient_sample")
        );
    }

    #[test]
    fn pairwise_comparison_records_tie_without_distillation_candidate() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        seed_accepted_machine_league_score(
            &kernel,
            "forge_machine_league_codex_tie",
            "codex_cli_external_worker",
            94,
        );
        seed_accepted_machine_league_score(
            &kernel,
            "forge_machine_league_native_tie",
            "mdx_native_harness_runner",
            94,
        );

        let response = route_response(
            "POST",
            "/forge/machine-league/pairwise-comparisons.json",
            r#"{"external_run_id":"forge_machine_league_codex_tie","native_run_id":"forge_machine_league_native_tie"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            response
                .body
                .contains(r#""status":"PAIRWISE_COMPARISON_RECORDED""#)
        );
        assert!(response.body.contains(r#""pairwise_outcome":"tie""#));
        assert!(
            response
                .body
                .contains(r#""distillation_candidate_allowed":false"#)
        );
    }

    #[test]
    fn pairwise_comparison_allows_distillation_only_for_external_win() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        seed_accepted_machine_league_score(
            &kernel,
            "forge_machine_league_codex_win_pairwise",
            "codex_cli_external_worker",
            97,
        );
        seed_accepted_machine_league_score(
            &kernel,
            "forge_machine_league_native_loss_pairwise",
            "mdx_native_harness_runner",
            94,
        );

        let response = route_response(
            "POST",
            "/forge/machine-league/pairwise-comparisons.json",
            r#"{"external_run_id":"forge_machine_league_codex_win_pairwise","native_run_id":"forge_machine_league_native_loss_pairwise"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("route ok");

        assert!(
            response
                .body
                .contains(r#""pairwise_outcome":"external_win""#)
        );
        assert!(
            response
                .body
                .contains(r#""distillation_candidate_allowed":true"#)
        );
        let guard = kernel.read().expect("kernel");
        let comparison_event = guard
            .ledger()
            .query()
            .by_kind("forge.run.event")
            .into_iter()
            .find(|receipt| payload_value(receipt, "machine_league_pairwise_status") == "recorded")
            .expect("pairwise event");
        assert_eq!(
            payload_value(comparison_event, "accepted_scoreboard_results_only"),
            "true"
        );
        assert_eq!(
            payload_value(comparison_event, "external_output_consumable"),
            "false"
        );
    }

    #[test]
    fn model_identity_falls_back_to_selected_builder_casting_when_no_model_call_exists() {
        let packet = serde_json::json!({
            "built_by": {
                "model": ""
            },
            "repo_intelligence": {
                "builder_casting": {
                    "selected_model_profile_id": "codex_anthropic_responses_profile",
                    "selected_provider_family": "anthropic",
                    "selected_model_id": "claude-opus-4-8",
                    "recommended_model_profile_id": "codex_xai_responses_profile",
                    "recommended_model_id": "grok-4.3"
                }
            }
        });
        let identity = model_identity_for_run(&packet);

        assert_eq!(
            identity.model_profile_id,
            "codex_anthropic_responses_profile"
        );
        assert_eq!(identity.model_id, "claude-opus-4-8");
    }

    fn seeded_eval_bridge_run(run_id: &str) -> Arc<RwLock<MdxKernel>> {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut kernel = kernel.write().expect("kernel");
            kernel
                .record_forge_run_event_with_evidence_fields(
                    ev(run_id, "run_started", "branch=forge/run_eval_bridge"),
                    &mdx_core::GovernedWriteIdentity::local_demo("human:dev"),
                    &[
                        ("repo_id", "swift_canary"),
                        ("repo_primary_language", "swift"),
                        ("language_pack_id", "swift-spm"),
                        ("repo_profile_detected_language_packs", "swift-spm"),
                        ("repo_profile_detected_files", "Package.swift"),
                        (
                            "repo_profile_standards_sources",
                            "AGENTS.md,CLAUDE.md,.github/CODEOWNERS",
                        ),
                        (
                            "repo_profile_standards_source_fingerprints",
                            "AGENTS.md=fnv1a64:1111111111111111,CLAUDE.md=fnv1a64:bae8bb8f209720a8,.github/CODEOWNERS=fnv1a64:2222222222222222",
                        ),
                        (
                            "repo_profile_principal_review_gates",
                            "behavior:changed_behavior_is_explicit,tests:checks_cover_changed_behavior,stack_idioms:<language-pack-specific>,compatibility:public_contract_or_migration_risk_reviewed,security:no_secret_or_authority_expansion,maintainability:dependency_config_and_generated_churn_justified",
                        ),
                        ("repo_profile_artifact_patterns", ".build/**,DerivedData/**"),
                        ("selected_checks", "swift test"),
                        ("selected_checks_source", "repo_profile_inferred"),
                        ("work_classification_recommended_shape", "run"),
                        ("work_classification_task_class", "feature"),
                        ("work_classification_complexity_tier", "medium"),
                        ("work_classification_confidence_pct", "82"),
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
                            "behavior:changed_behavior_is_explicit,tests:checks_cover_changed_behavior,stack_idioms:<language-pack-specific>,compatibility:public_contract_or_migration_risk_reviewed,security:no_secret_or_authority_expansion,maintainability:dependency_config_and_generated_churn_justified",
                        ),
                        ("language_task_alignment_grants_execution_authority", "false"),
                    ],
                )
                .expect("run started");
            kernel
                .record_forge_run_event(ev(run_id, "model_called", "model=grok-4.3"))
                .expect("model called");
            kernel
                .record_forge_run_event(ev(
                    run_id,
                    "tool_executed",
                    "semantic_query status=OK operation=related_tests",
                ))
                .expect("semantic query");
            kernel
                .record_forge_run_event(ev(run_id, "check_passed", "run_command swift test exit=0"))
                .expect("check passed");
            kernel
                .record_forge_run_event(ev(run_id, "run_finished", "status=RUN_FINISHED_DONE"))
                .expect("run finished");
        }
        kernel
    }

    fn seed_quality_ready(kernel: &Arc<RwLock<MdxKernel>>, run_id: &str, result_receipt_id: &str) {
        let mut guard = kernel.write().expect("kernel");
        guard
            .record_forge_run_event_with_evidence_fields(
                ev(
                    run_id,
                    "evidence_appended",
                    "eval_quality_gate_assessed status=READY_FOR_PRINCIPAL_REVIEW",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("agent:forge_eval"),
                &[
                    ("eval_quality_gate_status", "READY_FOR_PRINCIPAL_REVIEW"),
                    ("eval_machine_quality_gates_passed", "true"),
                    ("eval_can_be_scored_after_principal_review", "true"),
                    ("eval_passed_gate_count", "10"),
                    ("eval_failed_gate_count", "0"),
                    ("eval_result_receipt_id", result_receipt_id),
                    ("eval_accepts_for_scoreboard", "false"),
                    ("eval_grants_execution_authority", "false"),
                ],
            )
            .expect("quality event");
    }

    fn seed_external_runtime(
        kernel: &Arc<RwLock<MdxKernel>>,
        run_id: &str,
        runner_id: &str,
        adapter_kind: &str,
        fingerprint_id: &str,
        drift_status: &str,
    ) {
        let mut guard = kernel.write().expect("kernel");
        guard
            .record_forge_run_event_with_evidence_fields(
                ev(
                    run_id,
                    "evidence_appended",
                    "machine runtime identity recorded",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("agent:machine_runtime"),
                &[
                    ("runner_profile_runner_id", runner_id),
                    ("runner_profile_adapter_kind", adapter_kind),
                    ("machine_runtime_runner_id", runner_id),
                    ("machine_runtime_adapter_kind", adapter_kind),
                    ("machine_runtime_fingerprint_id", fingerprint_id),
                    ("machine_runtime_drift_status", drift_status),
                    ("machine_runtime_binary_present", "true"),
                    (
                        "machine_runtime_adapter_contract_version",
                        MACHINE_RUNTIME_ADAPTER_CONTRACT_VERSION,
                    ),
                ],
            )
            .expect("runtime identity event");
    }

    fn seed_runtime_smoke(
        kernel: &Arc<RwLock<MdxKernel>>,
        runner_id: &str,
        adapter_kind: &str,
        fingerprint_id: &str,
    ) {
        let mut guard = kernel.write().expect("kernel");
        guard
            .record_forge_run_event_with_evidence_fields(
                ev(
                    "forge_machine_runtime_smoke",
                    "evidence_appended",
                    "machine_runtime_smoke status=runtime_smoke_passed",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("agent:machine_runtime_smoke"),
                &[
                    ("machine_runtime_smoke_status", "runtime_smoke_passed"),
                    ("machine_runtime_smoke_passed", "true"),
                    ("machine_runtime_runner_id", runner_id),
                    ("machine_runtime_adapter_kind", adapter_kind),
                    ("machine_runtime_fingerprint_id", fingerprint_id),
                    (
                        "machine_runtime_compatibility_status",
                        "version_command_observed",
                    ),
                    ("adapter_execution_performed", "false"),
                    ("task_execution_allowed", "false"),
                    ("external_output_consumable", "false"),
                ],
            )
            .expect("runtime smoke event");
    }

    struct FixtureOracleSeed<'a> {
        eval_oracle_status: &'a str,
        visible_check_passed: &'a str,
        hidden_check_observed: &'a str,
        hidden_check_passed: &'a str,
        contamination_status: &'a str,
        eval_fixture_snapshot_id: &'a str,
    }

    fn seed_fixture_oracle(
        kernel: &Arc<RwLock<MdxKernel>>,
        run_id: &str,
        seed: FixtureOracleSeed<'_>,
    ) {
        let mut guard = kernel.write().expect("kernel");
        guard
            .record_forge_run_event_with_evidence_fields(
                ev(
                    run_id,
                    "evidence_appended",
                    "machine league fixture oracle recorded",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("agent:forge_eval"),
                &[
                    (
                        "machine_league_eval_fixture",
                        MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID,
                    ),
                    ("eval_fixture_snapshot_id", seed.eval_fixture_snapshot_id),
                    ("eval_oracle_status", seed.eval_oracle_status),
                    ("visible_check_passed", seed.visible_check_passed),
                    ("hidden_check_observed", seed.hidden_check_observed),
                    ("hidden_check_passed", seed.hidden_check_passed),
                    ("contamination_status", seed.contamination_status),
                    ("fresh_fixture_required_for_scorecard_acceptance", "true"),
                ],
            )
            .expect("fixture oracle event");
    }

    fn seed_accepted_machine_league_score(
        kernel: &Arc<RwLock<MdxKernel>>,
        run_id: &str,
        runner_id: &str,
        total_score: u32,
    ) {
        seed_accepted_machine_league_score_for_fixture(
            kernel,
            run_id,
            runner_id,
            total_score,
            MACHINE_LEAGUE_RUST_TAX_FIXTURE_ID,
            MACHINE_LEAGUE_RUST_TAX_FIXTURE_SNAPSHOT_ID,
        );
    }

    fn seed_native_runtime_adaptation_grant(kernel: &Arc<RwLock<MdxKernel>>) -> String {
        let mut guard = kernel.write().expect("kernel");
        let judgment = guard
            .record_learning_judgment_decision(mdx_core::LearningJudgmentDecision {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                judgment_id: "judgment_native_runtime_adaptation",
                promotion_id: "learning_candidate_distillation_runtime_adaptation",
                decision: "promote_candidate",
                rationale: "The Machine League lesson is durable enough to steer native casting.",
                evidence_refs: "distillation_note_policy_edge, pairwise_comparison_policy_edge",
            })
            .expect("judgment decision");
        let promotion = guard
            .request_learning_memory_promotion(mdx_core::LearningMemoryPromotion {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                judgment_decision_id: &judgment.judgment_decision_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                judgment_id: &judgment.judgment_id,
                promotion_id: &judgment.promotion_id,
                target_type: "model_scorecard",
                target_path: "generated/learning/model-worker-scorecard-targets.json",
                lesson_summary: "For Rust bug fixes, let the native builder prefer policy-first cross-file planning before narrow single-file edits.",
                evidence_refs: "distillation_note_policy_edge, pairwise_comparison_policy_edge",
                review_cadence: "review after next three native runs",
                expiry_rule: "supersede when native remeasurement no longer improves",
            })
            .expect("memory promotion");
        let activation = guard
            .activate_learning_memory(mdx_core::LearningMemoryActivation {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                memory_promotion_id: &promotion.memory_promotion_id,
                memory_promotion_receipt_id: &promotion.receipt_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                target_type: "model_scorecard",
                target_path: "generated/learning/model-worker-scorecard-targets.json",
                lesson_summary: "For Rust bug fixes, let the native builder prefer policy-first cross-file planning before narrow single-file edits.",
                evidence_refs: "distillation_note_policy_edge, pairwise_comparison_policy_edge",
                activation_basis: "approved with local proof",
                rollback_plan: "withdraw adaptation grant if native remeasurement regresses",
                local_checks: "cargo test -p mdx-server native_runtime_adaptation_grant_is_applied_to_machine_league_fleet_casts",
                approval_refs: "human:eng",
                review_owner: "human:eng",
            })
            .expect("memory activation");
        guard
            .grant_learning_adaptation(mdx_core::LearningAdaptationGrant {
                tenant_id: "local_tenant",
                actor_id: "human:eng",
                activation_receipt_id: &activation.receipt_id,
                adaptation_type: "fleet_casting",
                target_type: "model_scorecard",
                preferred_builder_slot: "",
                reason: "Apply the ratified Machine League lesson to native builder planning.",
                review_owner: "human:eng",
            })
            .expect("adaptation grant")
            .receipt_id
    }

    fn seed_accepted_machine_league_score_for_fixture(
        kernel: &Arc<RwLock<MdxKernel>>,
        run_id: &str,
        runner_id: &str,
        total_score: u32,
        eval_fixture: &str,
        eval_fixture_snapshot_id: &str,
    ) {
        let mut guard = kernel.write().expect("kernel");
        let total_score = total_score.to_string();
        let fixture = machine_league_eval_fixture_or_default(eval_fixture);
        guard
            .record_forge_run_event_with_evidence_fields(
                ev(
                    run_id,
                    "evidence_appended",
                    "eval_principal_reviewed status=accepted_for_scoreboard",
                ),
                &mdx_core::GovernedWriteIdentity::local_demo("agent:forge_eval"),
                &[
                    ("accepted_for_scoreboard", "true"),
                    ("runner_id", runner_id),
                    ("winning_runner_id", runner_id),
                    ("baseline_runner_id", "mdx_native_harness_runner"),
                    ("language_task_class", "bug_fix"),
                    ("language_pack_id", "rust-cargo"),
                    ("machine_league_eval_fixture", eval_fixture),
                    ("machine_league_eval_fixture_family", fixture.family_id),
                    (
                        "machine_league_eval_fixture_sibling",
                        fixture.sibling_fixture_id,
                    ),
                    (
                        "machine_league_transferable_edge",
                        fixture.transferable_edge,
                    ),
                    ("eval_fixture_snapshot_id", eval_fixture_snapshot_id),
                    ("eval_oracle_status", "visible_and_hidden_checks_passed"),
                    ("contamination_status", "fresh_held_out_fixture"),
                    ("total_score", total_score.as_str()),
                    ("external_output_consumable", "false"),
                ],
            )
            .expect("accepted machine league score");
    }

    fn seed_external_win_pairwise(
        kernel: &Arc<RwLock<MdxKernel>>,
        external_run_id: &str,
        native_run_id: &str,
    ) {
        let body = format!(
            r#"{{"external_run_id":{},"native_run_id":{}}}"#,
            json_string_literal(external_run_id),
            json_string_literal(native_run_id),
        );
        let response = route_response(
            "POST",
            "/forge/machine-league/pairwise-comparisons.json",
            &body,
            kernel,
        )
        .expect("pairwise route handled")
        .expect("pairwise route ok");
        assert!(
            response
                .body
                .contains(r#""distillation_candidate_allowed":true"#),
            "{:?}",
            response.body
        );
    }

    fn ev<'a>(run_id: &'a str, event: &'a str, detail: &'a str) -> ForgeRunEvent<'a> {
        ForgeRunEvent {
            tenant_id: "local_tenant",
            actor_id: "human:dev",
            run_id,
            event,
            work_item_id: "eval_bridge",
            detail,
            turn: 1,
            input_tokens: 0,
            output_tokens: 0,
        }
    }
}
