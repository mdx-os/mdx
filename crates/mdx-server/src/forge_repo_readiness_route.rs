// First-read readiness for a connected external repo. This collapses the repo
// profile, cached index state, proof plan, semantic/toolchain readiness, and
// safe next routes into one operator packet. It is read-only: no run starts,
// no provider is called, and no authority opens.
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
    if path != "/forge/repo-readiness.json" {
        return None;
    }
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Some(Ok(response));
    }
    let repo_id = json_string_field(body, "repo_id").unwrap_or_default();
    if repo_id.trim().is_empty() {
        return Some(Ok(refusal("name the connected repo to inspect")));
    }
    Some(handle(&repo_id, kernel))
}

fn handle(repo_id: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let repo_id = repo_id.trim();
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let mut latest: Option<(&mdx_core::Receipt, BTreeMap<String, String>)> = None;
    for receipt in kernel.ledger().query().by_kind("forge.repo.connected") {
        if receipt.payload.get("repo_id").map(String::as_str) == Some(repo_id) {
            latest = Some((receipt, receipt.payload.clone()));
        }
    }
    let Some((repo_receipt, fields)) = latest else {
        return Ok(refusal("no connected repo by that id"));
    };
    let root = fields.get("root").map(String::as_str).unwrap_or("");
    let indexed_profile = kernel.forge_repo_index_profile_json(repo_id);
    let (profile_json, profile_source) = crate::forge_repo_profile::profile_json_with_current_setup(
        std::path::Path::new(root),
        indexed_profile,
    );
    let profile: serde_json::Value = serde_json::from_str(&profile_json)
        .unwrap_or_else(|_| serde_json::json!({"language_pack_id":"generic"}));
    let proof_status = profile["proof_plan"]["status"]
        .as_str()
        .unwrap_or("operator_check_required");
    let readiness_status = match proof_status {
        "ready" => "READY_FOR_MEDIUM_HIGH_WORK",
        "setup_required" => "SETUP_REQUIRED",
        _ => "OPERATOR_CHECK_REQUIRED",
    };
    let next_move = match readiness_status {
        "READY_FOR_MEDIUM_HIGH_WORK" => {
            "Start with the suggested check, then review the generated PR handoff."
        }
        "SETUP_REQUIRED" => {
            "Install or expose the missing proof toolchain before spending provider calls."
        }
        _ => "Choose an explicit proof command before starting the run.",
    };
    let suggested_checks = profile["suggested_checks"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let selected_checks_source = if suggested_checks.is_empty() {
        "none_recorded"
    } else {
        "repo_profile_inferred"
    };
    let language_pack_id = profile["language_pack_id"].as_str().unwrap_or("generic");
    let standards_summaries = string_array(&profile["standards_source_summaries"]);
    let standards_obligations =
        crate::forge_repo_standards_packet_route::standards_obligations(&standards_summaries);
    let origin_url = fields.get("origin_url").map(String::as_str).unwrap_or("");
    let source_host = crate::forge_repo_onboarding_packet_route::infer_source_host(origin_url);
    let credential_sources =
        crate::forge_repo_onboarding_packet_route::credential_sources(source_host);
    let index_receipt_id = kernel
        .forge_repo_index_receipt_id(repo_id)
        .unwrap_or_default();
    let first_run_tasks =
        crate::forge_repo_onboarding_packet_route::first_run_tasks(language_pack_id, &profile);
    let semantic_query_operations = serde_json::json!([
        "capabilities",
        "orientation",
        "workspace_symbol",
        "definition",
        "references",
        "impact_radius",
        "dependency_map",
        "symbol_graph",
        "hover",
        "file_outline",
        "related_tests",
        "diagnostics",
        "lsp_probe"
    ]);
    let source_host_summary = serde_json::json!({
        "source_host": source_host,
        "origin_url_present": !origin_url.trim().is_empty(),
        "origin_url_recorded": false,
        "credential_sources_checked": credential_sources,
        "credential_values_recorded": false,
        "source_host_readiness_route": "/forge/source-host-readiness.json",
        "source_host_pr_draft_route": "/forge/source-host-pr-drafts.json",
        "network_call_allowed": false,
        "remote_push_allowed": false,
        "pull_request_open_allowed": false,
    });

    let mut packet = serde_json::Map::new();
    packet.insert("name".to_string(), string_value("mdx-forge-repo-readiness"));
    packet.insert("status".to_string(), string_value("OK"));
    packet.insert("repo_id".to_string(), string_value(repo_id));
    packet.insert(
        "label".to_string(),
        string_value(fields.get("label").map(String::as_str).unwrap_or("")),
    );
    packet.insert(
        "kind".to_string(),
        string_value(fields.get("kind").map(String::as_str).unwrap_or("")),
    );
    packet.insert("origin_url".to_string(), string_value(origin_url));
    packet.insert(
        "repo_receipt_id".to_string(),
        string_value(&repo_receipt.receipt_id),
    );
    packet.insert(
        "repo_index_receipt_id".to_string(),
        string_value(&index_receipt_id),
    );
    packet.insert("profile_source".to_string(), string_value(profile_source));
    packet.insert(
        "readiness_status".to_string(),
        string_value(readiness_status),
    );
    packet.insert("safe_next_move".to_string(), string_value(next_move));
    packet.insert(
        "medium_high_work_ready".to_string(),
        serde_json::Value::Bool(readiness_status == "READY_FOR_MEDIUM_HIGH_WORK"),
    );
    packet.insert(
        "primary_language".to_string(),
        profile["primary_language"].clone(),
    );
    packet.insert(
        "language_pack_id".to_string(),
        profile["language_pack_id"].clone(),
    );
    packet.insert(
        "detected_language_packs".to_string(),
        profile["detected_language_packs"].clone(),
    );
    packet.insert(
        "quality_signals".to_string(),
        profile["quality_signals"].clone(),
    );
    packet.insert(
        "standards_sources".to_string(),
        profile["standards_sources"].clone(),
    );
    packet.insert(
        "standards_source_summaries".to_string(),
        profile["standards_source_summaries"].clone(),
    );
    packet.insert(
        "semantic_intelligence".to_string(),
        profile["semantic_intelligence"].clone(),
    );
    packet.insert(
        "semantic_tool_readiness".to_string(),
        profile["semantic_tool_readiness"].clone(),
    );
    packet.insert(
        "toolchain_readiness".to_string(),
        profile["toolchain_readiness"].clone(),
    );
    packet.insert("proof_plan".to_string(), profile["proof_plan"].clone());
    packet.insert(
        "proof_command_preflight".to_string(),
        crate::forge_repo_onboarding_packet_route::proof_command_preflight(&profile),
    );
    packet.insert(
        "suggested_checks".to_string(),
        serde_json::Value::Array(suggested_checks),
    );
    packet.insert(
        "selected_checks_source".to_string(),
        string_value(selected_checks_source),
    );
    packet.insert(
        "standards_obligation_count".to_string(),
        serde_json::Value::Number(standards_obligations.len().into()),
    );
    packet.insert(
        "standards_obligations".to_string(),
        serde_json::Value::Array(standards_obligations),
    );
    packet.insert(
        "first_run_tasks".to_string(),
        serde_json::Value::Array(first_run_tasks),
    );
    packet.insert(
        "semantic_query_operations".to_string(),
        semantic_query_operations,
    );
    packet.insert("source_host_summary".to_string(), source_host_summary);
    packet.insert("run_route".to_string(), string_value("/forge/runs.json"));
    packet.insert(
        "repo_index_route".to_string(),
        string_value("/forge/repo-indexes.json"),
    );
    packet.insert(
        "repo_standards_packet_route".to_string(),
        string_value("/forge/repo-standards-packet.json"),
    );
    packet.insert(
        "repo_task_scout_route".to_string(),
        string_value("/forge/repo-task-scout.json"),
    );
    packet.insert(
        "semantic_query_route".to_string(),
        string_value("/forge/semantic-queries.json"),
    );
    packet.insert(
        "review_packet_route".to_string(),
        string_value("/forge/review-packet.json"),
    );
    packet.insert(
        "pr_handoff_route".to_string(),
        string_value("/forge/pr-handoffs.json"),
    );
    packet.insert(
        "provider_calls_allowed".to_string(),
        serde_json::Value::Bool(false),
    );
    packet.insert(
        "adapter_execution_allowed".to_string(),
        serde_json::Value::Bool(false),
    );
    packet.insert("run_started".to_string(), serde_json::Value::Bool(false));
    packet.insert(
        "credential_values_recorded".to_string(),
        serde_json::Value::Bool(false),
    );
    packet.insert(
        "network_call_allowed".to_string(),
        serde_json::Value::Bool(false),
    );
    packet.insert(
        "production_write_allowed".to_string(),
        serde_json::Value::Bool(false),
    );

    Ok(RouteResponse::json(
        "200 OK",
        serde_json::Value::Object(packet).to_string(),
    ))
}

fn string_value(value: &str) -> serde_json::Value {
    serde_json::Value::String(value.to_string())
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-repo-readiness","status":"REFUSED","reason":{},"provider_calls_allowed":false,"adapter_execution_allowed":false,"run_started":false,"credential_values_recorded":false,"network_call_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
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
    use mdx_core::{ForgeRepoConnect, GovernedWriteIdentity};

    #[test]
    fn readiness_refuses_unknown_repo_without_starting_work() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = handle("missing", &kernel).expect("response");
        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains(r#""run_started":false"#));
        assert!(response.body.contains(r#""provider_calls_allowed":false"#));
    }

    #[test]
    fn readiness_reports_connected_repo_safe_next_move() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut kernel = kernel.write().expect("kernel");
            kernel
                .connect_forge_repo_with_identity(
                    ForgeRepoConnect {
                        tenant_id: "t",
                        actor_id: "human:dev",
                        repo_id: "generic_repo",
                        label: "Generic Repo",
                        root: "/no/such/repo",
                        kind: "local",
                        origin: "",
                    },
                    &GovernedWriteIdentity::local_demo("human:dev"),
                )
                .expect("connect");
        }
        let response = handle("generic_repo", &kernel).expect("response");
        assert!(
            response
                .body
                .contains(r#""name":"mdx-forge-repo-readiness""#)
        );
        assert!(response.body.contains(r#""status":"OK""#));
        assert!(
            response
                .body
                .contains(r#""readiness_status":"OPERATOR_CHECK_REQUIRED""#)
        );
        assert!(
            response
                .body
                .contains(r#""semantic_query_route":"/forge/semantic-queries.json""#)
        );
        assert!(
            response
                .body
                .contains(r#""repo_standards_packet_route":"/forge/repo-standards-packet.json""#)
        );
        assert!(
            response
                .body
                .contains(r#""repo_task_scout_route":"/forge/repo-task-scout.json""#)
        );
        assert!(response.body.contains(r#""standards_obligation_count":"#));
        assert!(response.body.contains(r#""first_run_tasks":"#));
        assert!(response.body.contains(r#""proof_command_preflight":"#));
        assert!(
            response
                .body
                .contains(r#""run_admission_would_refuse":true"#)
        );
        assert!(response.body.contains(r#""provider_calls_allowed":false"#));
        assert!(response.body.contains(r#""semantic_query_operations":"#));
        assert!(response.body.contains(r#""source_host_summary":"#));
        assert!(
            response
                .body
                .contains(r#""pr_handoff_route":"/forge/pr-handoffs.json""#)
        );
        assert!(response.body.contains(r#""run_started":false"#));
        assert!(
            response
                .body
                .contains(r#""credential_values_recorded":false"#)
        );
        assert!(response.body.contains(r#""network_call_allowed":false"#));
    }
}
