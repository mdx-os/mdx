// First-run packet for a connected external repo. This turns deterministic repo
// intelligence into a staff-engineer-style starting plan before provider calls:
// language pack, proof command, standards obligations, safe first tasks, and
// source-host delivery readiness hints. Read-only: no provider call, no run
// admission, no shell execution, no credential value recording, and no
// production authority.
use crate::RouteResponse;
use mdx_core::{MdxKernel, json_string_literal};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/repo-onboarding-packet.json" {
        return None;
    }
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Some(Ok(response));
    }
    let repo_id = json_string_field(body, "repo_id").unwrap_or_default();
    if repo_id.trim().is_empty() {
        return Some(Ok(refusal("name the connected repo to prepare")));
    }
    Some(handle(&repo_id, kernel))
}

fn handle(repo_id: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let repo_id = repo_id.trim();
    let kernel_read = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let mut latest: Option<(&mdx_core::Receipt, BTreeMap<String, String>)> = None;
    for receipt in kernel_read.ledger().query().by_kind("forge.repo.connected") {
        if receipt.payload.get("repo_id").map(String::as_str) == Some(repo_id) {
            latest = Some((receipt, receipt.payload.clone()));
        }
    }
    let Some((repo_receipt, fields)) = latest else {
        return Ok(refusal("no connected repo by that id"));
    };

    let root = fields.get("root").map(String::as_str).unwrap_or("");
    let indexed_profile = kernel_read.forge_repo_index_profile_json(repo_id);
    let (profile_json, profile_source) = crate::forge_repo_profile::profile_json_with_current_setup(
        std::path::Path::new(root),
        indexed_profile,
    );
    let profile: serde_json::Value = serde_json::from_str(&profile_json)
        .unwrap_or_else(|_| serde_json::json!({"language_pack_id":"generic"}));

    let suggested_checks = string_array(&profile["suggested_checks"]);
    let selected_checks_source = if suggested_checks.is_empty() {
        "operator_required"
    } else {
        "repo_profile_inferred"
    };
    let proof_status = profile["proof_plan"]["status"]
        .as_str()
        .unwrap_or("operator_check_required");
    let onboarding_status = match proof_status {
        "ready" => "READY_FOR_FIRST_MEDIUM_RUN",
        "setup_required" => "SETUP_REQUIRED",
        _ => "CHECK_REQUIRED",
    };
    let safe_next_move = match onboarding_status {
        "READY_FOR_FIRST_MEDIUM_RUN" => {
            "Pick one suggested task, keep the selected check, and review the PR handoff before any source-host action."
        }
        "SETUP_REQUIRED" => {
            "Install or expose the missing proof toolchain before spending model calls."
        }
        _ => "Choose one explicit proof command, then start with a small or medium task.",
    };

    let standards_summaries = string_array(&profile["standards_source_summaries"]);
    let standards_obligations =
        crate::forge_repo_standards_packet_route::standards_obligations(&standards_summaries);
    let language_pack_id = profile["language_pack_id"].as_str().unwrap_or("generic");
    let source_host = infer_source_host(fields.get("origin_url").map(String::as_str).unwrap_or(""));
    let credential_sources = credential_sources(source_host);

    Ok(RouteResponse::json(
        "200 OK",
        serde_json::json!({
            "name": "mdx-forge-repo-onboarding-packet",
            "status": "OK",
            "repo_id": repo_id,
            "label": fields.get("label").map(String::as_str).unwrap_or(""),
            "kind": fields.get("kind").map(String::as_str).unwrap_or(""),
            "repo_receipt_id": repo_receipt.receipt_id,
            "repo_index_receipt_id": kernel_read.forge_repo_index_receipt_id(repo_id).unwrap_or_default(),
            "profile_source": profile_source,
            "onboarding_status": onboarding_status,
            "safe_next_move": safe_next_move,
            "primary_language": profile["primary_language"].clone(),
            "language_pack_id": language_pack_id,
            "detected_language_packs": profile["detected_language_packs"].clone(),
            "quality_signals": profile["quality_signals"].clone(),
            "proof_plan": profile["proof_plan"].clone(),
            "proof_command_preflight": proof_command_preflight(&profile),
            "suggested_checks": suggested_checks,
            "selected_checks_source": selected_checks_source,
            "semantic_tool_readiness": profile["semantic_tool_readiness"].clone(),
            "toolchain_readiness": profile["toolchain_readiness"].clone(),
            "standards_obligation_count": standards_obligations.len(),
            "standards_obligations": standards_obligations,
            "first_run_tasks": first_run_tasks(language_pack_id, &profile),
            "source_host_summary": {
                "source_host": source_host,
                "origin_url_present": fields.get("origin_url").is_some_and(|origin| !origin.trim().is_empty()),
                "origin_url_recorded": false,
                "credential_sources_checked": credential_sources,
                "credential_values_recorded": false,
                "source_host_readiness_route": "/forge/source-host-readiness.json",
                "source_host_pr_draft_route": "/forge/source-host-pr-drafts.json",
                "network_call_allowed": false,
                "remote_push_allowed": false,
                "pull_request_open_allowed": false,
            },
            "repo_readiness_route": "/forge/repo-readiness.json",
            "repo_standards_packet_route": "/forge/repo-standards-packet.json",
            "semantic_query_route": "/forge/semantic-queries.json",
            "run_route": "/forge/runs.json",
            "review_packet_route": "/forge/review-packet.json",
            "pr_handoff_route": "/forge/pr-handoffs.json",
            "provider_calls_allowed": false,
            "adapter_execution_allowed": false,
            "run_started": false,
            "credential_values_recorded": false,
            "network_call_allowed": false,
            "production_write_allowed": false,
        })
        .to_string(),
    ))
}

pub(crate) fn first_run_tasks(
    language_pack_id: &str,
    profile: &serde_json::Value,
) -> Vec<serde_json::Value> {
    let suggested_checks = proof_sequence(profile);
    let idiom = match language_pack_id {
        "ios-xcode" => "Swift, XCTest, target, scheme, and entitlement boundaries",
        "swift-spm" => "Swift Package Manager, Foundation, and XCTest conventions",
        "java-maven" => "Maven, JUnit, public API, and dependency-surface conventions",
        "gradle-jvm" => "Gradle, Kotlin or Java, public API, and test task conventions",
        "android-gradle" => {
            "Android Gradle, lifecycle, permissions, resources, and local unit tests"
        }
        "dotnet" => ".NET solution, async, nullability, DI, and test-project conventions",
        "node" => "package scripts, TypeScript or module style, and dependency-churn conventions",
        "rust-cargo" => "Cargo, ownership, error style, and public API boundaries",
        "python" => "pytest, import shape, typing, and dependency-surface conventions",
        "go" => "go test, explicit errors, table tests, and public API compatibility",
        _ => "repo-local conventions and the operator-selected proof command",
    };
    vec![
        serde_json::json!({
            "task_id": format!("{language_pack_id}-first-run-behavior-check"),
            "title": "Make one small behavior change with a regression check",
            "recommended_shape": "run",
            "task_class": "bug_fix",
            "complexity_tier": "small",
            "prompt_template": format!("Fix one narrow behavior issue in this repo, preserve {idiom}, add or update the smallest regression coverage, and keep the diff reviewable."),
            "suggested_checks": suggested_checks.clone(),
            "review_focus": ["behavior:changed_behavior_is_explicit", "tests:checks_cover_changed_behavior", "security:no_secret_or_authority_expansion"],
            "why_this_first": "Small behavior work proves repo access, idioms, checks, artifact filtering, and Review Packet handoff with limited blast radius.",
            "grants_execution_authority": false,
        }),
        serde_json::json!({
            "task_id": format!("{language_pack_id}-medium-refactor-scout"),
            "title": "Refactor one contained module behind existing tests",
            "recommended_shape": "run",
            "task_class": "refactor",
            "complexity_tier": "medium",
            "prompt_template": format!("Refactor one contained module to improve clarity while preserving public behavior, following {idiom}; avoid dependency churn unless the repo already requires it."),
            "suggested_checks": suggested_checks,
            "review_focus": ["compatibility:public_contract_or_migration_risk_reviewed", "maintainability:dependency_config_and_generated_churn_justified", "tests:checks_cover_changed_behavior"],
            "why_this_first": "A contained refactor exercises cross-file judgment without turning the first run into a long-horizon mission.",
            "grants_execution_authority": false,
        }),
    ]
}

fn proof_sequence(profile: &serde_json::Value) -> Vec<String> {
    let checks = string_array(&profile["suggested_checks"]);
    if checks.is_empty() {
        vec!["operator-selected check".to_string()]
    } else {
        checks
    }
}

fn proof_sequence_display(checks: &[String]) -> String {
    checks
        .iter()
        .map(|check| check.trim())
        .filter(|check| !check.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}

pub(crate) fn proof_command_preflight(profile: &serde_json::Value) -> serde_json::Value {
    let suggested_checks = string_array(&profile["suggested_checks"]);
    let command = proof_sequence_display(&suggested_checks);
    let command_source = if suggested_checks.is_empty() {
        "operator_required"
    } else {
        "repo_profile_inferred"
    };
    let proof_plan_status = profile["proof_plan"]["status"]
        .as_str()
        .unwrap_or("operator_check_required");
    let toolchain_readiness = string_array(&profile["toolchain_readiness"]);
    let missing_readiness = toolchain_readiness
        .iter()
        .filter(|item| item.ends_with("=missing"))
        .cloned()
        .collect::<Vec<_>>();
    let preflight_status = if command.is_empty() {
        "CHECK_REQUIRED"
    } else if proof_plan_status == "ready" && missing_readiness.is_empty() {
        "READY"
    } else if proof_plan_status == "setup_required" || !missing_readiness.is_empty() {
        "SETUP_REQUIRED"
    } else {
        "CHECK_REQUIRED"
    };
    let safe_next_move = match preflight_status {
        "READY" => "Start the run with this exact check and review the proof coverage afterward.",
        "SETUP_REQUIRED" => {
            "Install or expose the missing proof toolchain before spending provider calls."
        }
        _ => "Choose an explicit proof command before starting the run.",
    };
    serde_json::json!({
        "status": preflight_status,
        "ready": preflight_status == "READY",
        "selected_command": command,
        "selected_commands": suggested_checks,
        "command_source": command_source,
        "proof_plan_status": proof_plan_status,
        "missing_readiness": missing_readiness,
        "toolchain_readiness": toolchain_readiness,
        "safe_next_move": safe_next_move,
        "run_admission_would_refuse": preflight_status != "READY",
        "shell_execution_allowed": false,
        "provider_calls_allowed": false,
        "run_started": false,
        "grants_execution_authority": false,
    })
}

pub(crate) fn infer_source_host(origin_url: &str) -> &'static str {
    let value = origin_url.to_ascii_lowercase();
    if value.contains("github.com") {
        "github"
    } else if value.contains("bitbucket.org") {
        "bitbucket"
    } else {
        "generic"
    }
}

pub(crate) fn credential_sources(source_host: &str) -> Vec<&'static str> {
    match source_host {
        "github" => vec!["GITHUB_TOKEN", "GH_TOKEN"],
        "bitbucket" => vec![
            "BITBUCKET_TOKEN",
            "BITBUCKET_USERNAME",
            "BITBUCKET_APP_PASSWORD",
        ],
        _ => Vec::new(),
    }
}

fn string_array(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-repo-onboarding-packet","status":"REFUSED","reason":{},"provider_calls_allowed":false,"adapter_execution_allowed":false,"run_started":false,"credential_values_recorded":false,"network_call_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(reason)
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
    use mdx_core::{ForgeRepoConnect, ForgeRepoIndex, GovernedWriteIdentity};

    struct TempRepo {
        path: std::path::PathBuf,
    }

    impl TempRepo {
        fn node_with_lock() -> Self {
            let unique = format!(
                "mdx-forge-onboarding-node-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(&path).expect("mkdir repo");
            std::fs::write(
                path.join("package.json"),
                "{\"scripts\":{\"test\":\"node test.js\"}}\n",
            )
            .expect("package");
            std::fs::write(path.join("package-lock.json"), "{}\n").expect("lock");
            Self { path }
        }

        fn python_with_requirements() -> Self {
            let unique = format!(
                "mdx-forge-onboarding-python-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(&path).expect("mkdir repo");
            std::fs::write(path.join("requirements.txt"), "pytest\nrequests\n")
                .expect("requirements");
            Self { path }
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn onboarding_packet_refuses_unknown_repo_without_starting_work() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = handle("missing", &kernel).expect("response");
        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains(r#""run_started":false"#));
        assert!(response.body.contains(r#""provider_calls_allowed":false"#));
        assert!(
            response
                .body
                .contains(r#""production_write_allowed":false"#)
        );
    }

    #[test]
    fn onboarding_packet_shapes_first_run_without_source_host_authority() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut kernel = kernel.write().expect("kernel");
            kernel
                .connect_forge_repo_with_identity(
                    ForgeRepoConnect {
                        tenant_id: "t",
                        actor_id: "human:dev",
                        repo_id: "swift_repo",
                        label: "Swift Repo",
                        root: "/no/such/repo",
                        kind: "local",
                        origin: "git@github.com:acme/swift-repo.git",
                    },
                    &GovernedWriteIdentity::local_demo("human:dev"),
                )
                .expect("connect");
        }
        let response = handle("swift_repo", &kernel).expect("response");
        assert!(
            response
                .body
                .contains(r#""name":"mdx-forge-repo-onboarding-packet""#)
        );
        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""first_run_tasks""#));
        assert!(response.body.contains(r#""proof_command_preflight""#));
        assert!(
            response
                .body
                .contains(r#""run_admission_would_refuse":true"#)
        );
        assert!(response.body.contains(r#""shell_execution_allowed":false"#));
        assert!(response.body.contains(r#""source_host":"github""#));
        assert!(response.body.contains(r#""origin_url_recorded":false"#));
        assert!(
            response
                .body
                .contains(r#""credential_values_recorded":false"#)
        );
        assert!(response.body.contains(r#""network_call_allowed":false"#));
        assert!(
            response
                .body
                .contains(r#""pull_request_open_allowed":false"#)
        );
    }

    #[test]
    fn onboarding_packet_refreshes_stale_node_index_and_keeps_setup_with_proof() {
        let repo = TempRepo::node_with_lock();
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut kernel = kernel.write().expect("kernel");
            let identity = GovernedWriteIdentity::local_demo("human:dev");
            let connected = kernel
                .connect_forge_repo_with_identity(
                    ForgeRepoConnect {
                        tenant_id: "t",
                        actor_id: "human:dev",
                        repo_id: "node_repo",
                        label: "Node Repo",
                        root: repo.path.to_str().unwrap_or(""),
                        kind: "local",
                        origin: "",
                    },
                    &identity,
                )
                .expect("connect");
            kernel
                .index_forge_repo_with_identity(
                    ForgeRepoIndex {
                        tenant_id: "t",
                        actor_id: "human:dev",
                        repo_id: "node_repo",
                        repo_receipt_id: &connected.receipt_id,
                        profile_json: r#"{"language_pack_id":"node","suggested_checks":["npm test"],"proof_plan":{"status":"ready"}}"#,
                        profile_fingerprint: "fnv1a64:stale",
                        primary_language: "javascript-typescript",
                        language_pack_id: "node",
                        detected_language_packs: "node",
                        semantic_tool_readiness: "",
                        toolchain_readiness: "",
                        proof_plan_status: "ready",
                        standards_source_summaries: "",
                    },
                    &identity,
                )
                .expect("stale index");
        }

        let response = handle("node_repo", &kernel).expect("response");
        let value: serde_json::Value = serde_json::from_str(&response.body).expect("json");

        assert_eq!(value["profile_source"], "computed_live_stale_index");
        assert_eq!(value["suggested_checks"][0], "npm ci");
        assert_eq!(value["suggested_checks"][1], "npm test");
        assert_eq!(
            value["proof_command_preflight"]["selected_commands"][0],
            "npm ci"
        );
        assert_eq!(
            value["proof_command_preflight"]["selected_commands"][1],
            "npm test"
        );
        assert_eq!(value["first_run_tasks"][0]["suggested_checks"][0], "npm ci");
        assert_eq!(
            value["first_run_tasks"][0]["suggested_checks"][1],
            "npm test"
        );
    }

    #[test]
    fn onboarding_packet_refreshes_stale_python_requirements_index() {
        let repo = TempRepo::python_with_requirements();
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut kernel = kernel.write().expect("kernel");
            let identity = GovernedWriteIdentity::local_demo("human:dev");
            let connected = kernel
                .connect_forge_repo_with_identity(
                    ForgeRepoConnect {
                        tenant_id: "t",
                        actor_id: "human:dev",
                        repo_id: "python_repo",
                        label: "Python Repo",
                        root: repo.path.to_str().unwrap_or(""),
                        kind: "local",
                        origin: "",
                    },
                    &identity,
                )
                .expect("connect");
            kernel
                .index_forge_repo_with_identity(
                    ForgeRepoIndex {
                        tenant_id: "t",
                        actor_id: "human:dev",
                        repo_id: "python_repo",
                        repo_receipt_id: &connected.receipt_id,
                        profile_json: r#"{"language_pack_id":"python","suggested_checks":["pytest"],"proof_plan":{"status":"ready"}}"#,
                        profile_fingerprint: "fnv1a64:stale",
                        primary_language: "python",
                        language_pack_id: "python",
                        detected_language_packs: "python",
                        semantic_tool_readiness: "",
                        toolchain_readiness: "",
                        proof_plan_status: "ready",
                        standards_source_summaries: "",
                    },
                    &identity,
                )
                .expect("stale index");
        }

        let response = handle("python_repo", &kernel).expect("response");
        let value: serde_json::Value = serde_json::from_str(&response.body).expect("json");

        assert_eq!(value["profile_source"], "computed_live_stale_index");
        let python_setup = "python3 -m venv .venv && . .venv/bin/activate && python -m pip install -r requirements.txt pytest";
        let python_test = ". .venv/bin/activate && pytest";
        assert_eq!(value["suggested_checks"][0], python_setup);
        assert_eq!(value["suggested_checks"][1], python_test);
        assert_eq!(
            value["proof_command_preflight"]["selected_commands"][0],
            python_setup
        );
        assert_eq!(
            value["proof_command_preflight"]["selected_commands"][1],
            python_test
        );
        assert_eq!(
            value["first_run_tasks"][0]["suggested_checks"][0],
            python_setup
        );
        assert_eq!(
            value["first_run_tasks"][0]["suggested_checks"][1],
            python_test
        );
    }
}
