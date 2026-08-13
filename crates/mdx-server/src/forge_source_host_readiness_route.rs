// Source-host readiness for Forge PR delivery. This is a read-only preflight:
// it inspects Review Packet evidence, branch shape, source-host inference, and
// credential presence by variable name only. It never records credential
// values, calls a network API, pushes a branch, opens a PR, or grants ship
// authority.
use crate::RouteResponse;
use mdx_core::{MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/source-host-readiness.json" {
        return None;
    }
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Some(Ok(response));
    }
    let run_id = json_string_field(body, "run_id").unwrap_or_default();
    if run_id.trim().is_empty() {
        return Some(Ok(refusal(
            "name the run to inspect for source-host readiness",
        )));
    }
    Some(handle(&run_id, body, kernel))
}

fn handle(
    run_id: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    let packet_body = crate::forge_review_packet::assemble_review_packet_json(kernel, run_id)?;
    let packet: serde_json::Value = match serde_json::from_str(&packet_body) {
        Ok(value) => value,
        Err(_) => return Ok(refusal("review packet did not return valid JSON")),
    };
    if packet["status"].as_str() != Some("OK") {
        let reason = packet["reason"]
            .as_str()
            .unwrap_or("review packet is not available for this run");
        return Ok(refusal(reason));
    }

    let repo_id = packet["repo_intelligence"]["repo_id"]
        .as_str()
        .unwrap_or("");
    let origin_url = latest_origin_url(kernel, repo_id)?;
    let requested_host = json_string_field(body, "target_host").unwrap_or_default();
    let requested_host_normalized =
        crate::forge_source_host_pr_draft_route::normalize_source_host(&requested_host);
    let inferred_host = infer_source_host(&origin_url);
    let source_host = if requested_host.trim().is_empty() {
        inferred_host
    } else {
        requested_host_normalized
    };
    let target_base_branch = json_string_field(body, "base_branch")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "main".to_string());
    let source_branch = packet["branch"].as_str().unwrap_or("");
    let review_status = packet["review_status"].as_str().unwrap_or("");
    let delivery_guardrails =
        crate::forge_source_host_pr_draft_route::delivery_guardrails(&packet, review_status);
    let credential_sources_checked = credential_sources(source_host);
    let credentials_present = credentials_present(source_host, &credential_sources_checked);
    let mut blocked_reasons = Vec::new();
    if source_branch.trim().is_empty() {
        blocked_reasons.push("source_branch_missing");
    }
    if source_host == "generic" {
        blocked_reasons.push("source_host_unknown_or_unsupported");
    }
    if !credentials_present {
        blocked_reasons.push("source_host_credentials_missing");
    }
    blocked_reasons.extend(
        crate::forge_source_host_pr_draft_route::delivery_guardrail_blockers(&delivery_guardrails),
    );
    let readiness_status = if source_branch.trim().is_empty() {
        "SOURCE_BRANCH_REQUIRED"
    } else if source_host == "generic" {
        "SOURCE_HOST_REQUIRED"
    } else if !credentials_present {
        "SOURCE_HOST_CREDENTIALS_REQUIRED"
    } else if !crate::forge_source_host_pr_draft_route::delivery_guardrails_satisfied(
        &delivery_guardrails,
    ) {
        "REVIEW_GUARDRAILS_REQUIRED"
    } else {
        "READY_FOR_OPERATOR_PR_ACTION"
    };
    let safe_next_move = match readiness_status {
        "READY_FOR_OPERATOR_PR_ACTION" => {
            "Prepare the source-host PR draft, then require an explicit operator action before any push or PR open."
        }
        "SOURCE_HOST_CREDENTIALS_REQUIRED" => {
            "Add source-host credentials to the local environment or connector vault before live PR delivery."
        }
        "SOURCE_HOST_REQUIRED" => {
            "Choose GitHub or Bitbucket explicitly, or connect the repo with a recognizable origin URL."
        }
        "REVIEW_GUARDRAILS_REQUIRED" => {
            "Resolve the Review Packet proof and semantic guardrails before live source-host delivery."
        }
        _ => "Finish the Forge run so Review Packet records a source branch before PR delivery.",
    };

    Ok(RouteResponse::json(
        "200 OK",
        serde_json::json!({
            "name": "mdx-forge-source-host-readiness",
            "status": "OK",
            "run_id": run_id,
            "repo_id": repo_id,
            "review_status": review_status,
            "readiness_status": readiness_status,
            "ready_for_dry_pr_draft": !source_branch.trim().is_empty(),
            "ready_for_live_source_host_delivery": readiness_status == "READY_FOR_OPERATOR_PR_ACTION",
            "safe_next_move": safe_next_move,
            "source_host": source_host,
            "requested_host": requested_host.trim(),
            "inferred_host": inferred_host,
            "origin_url_present": !origin_url.trim().is_empty(),
            "origin_url_recorded": false,
            "source_branch": source_branch,
            "target_base_branch": target_base_branch.trim(),
            "credential_sources_checked": credential_sources_checked,
            "source_host_credentials_present": credentials_present,
            "credential_values_recorded": false,
            "blocked_reasons": blocked_reasons,
            "source_host_plan": crate::forge_source_host_pr_draft_route::source_host_plan(
                source_host,
                source_branch,
                target_base_branch.trim(),
            ),
            "delivery_guardrails": delivery_guardrails,
            "review_packet_route": "/forge/review-packet.json",
            "pr_handoff_route": "/forge/pr-handoffs.json",
            "source_host_pr_draft_route": "/forge/source-host-pr-drafts.json",
            "provider_calls_allowed": false,
            "network_call_allowed": false,
            "remote_push_allowed": false,
            "pull_request_open_allowed": false,
            "approval_allowed": false,
            "deployment_authority_granted": false,
            "production_write_allowed": false,
        })
        .to_string(),
    ))
}

fn latest_origin_url(kernel: &Arc<RwLock<MdxKernel>>, repo_id: &str) -> Result<String, String> {
    if repo_id.trim().is_empty() {
        return Ok(String::new());
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    Ok(kernel.forge_repo_origin(repo_id).unwrap_or_default())
}

fn infer_source_host(origin_url: &str) -> &'static str {
    let value = origin_url.to_ascii_lowercase();
    if value.contains("github.com") {
        "github"
    } else if value.contains("bitbucket.org") {
        "bitbucket"
    } else {
        "generic"
    }
}

fn credential_sources(source_host: &str) -> Vec<&'static str> {
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

fn credentials_present(source_host: &str, sources: &[&str]) -> bool {
    match source_host {
        "github" => sources
            .iter()
            .any(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty())),
        "bitbucket" => {
            std::env::var_os("BITBUCKET_TOKEN").is_some_and(|value| !value.is_empty())
                || (std::env::var_os("BITBUCKET_USERNAME").is_some_and(|value| !value.is_empty())
                    && std::env::var_os("BITBUCKET_APP_PASSWORD")
                        .is_some_and(|value| !value.is_empty()))
        }
        _ => false,
    }
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-source-host-readiness","status":"REFUSED","reason":{},"credential_values_recorded":false,"provider_calls_allowed":false,"network_call_allowed":false,"remote_push_allowed":false,"pull_request_open_allowed":false,"approval_allowed":false,"deployment_authority_granted":false,"production_write_allowed":false}}"#,
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

    #[test]
    fn refuses_unknown_run_without_source_host_authority() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = handle(
            "forge_run_missing",
            r#"{"run_id":"forge_run_missing","target_host":"github"}"#,
            &kernel,
        )
        .expect("response");
        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains(r#""network_call_allowed":false"#));
        assert!(
            response
                .body
                .contains(r#""pull_request_open_allowed":false"#)
        );
        assert!(
            response
                .body
                .contains(r#""credential_values_recorded":false"#)
        );
    }

    #[test]
    fn host_inference_and_credentials_are_presence_only() {
        assert_eq!(
            infer_source_host("git@github.com:acme/service.git"),
            "github"
        );
        assert_eq!(
            infer_source_host("https://bitbucket.org/acme/service"),
            "bitbucket"
        );
        assert_eq!(infer_source_host("ssh://git.example.com/repo"), "generic");
        assert_eq!(
            credential_sources("github"),
            vec!["GITHUB_TOKEN", "GH_TOKEN"]
        );
        assert_eq!(
            credential_sources("bitbucket"),
            vec![
                "BITBUCKET_TOKEN",
                "BITBUCKET_USERNAME",
                "BITBUCKET_APP_PASSWORD"
            ]
        );
        assert!(credential_sources("generic").is_empty());
    }

    #[test]
    fn readiness_uses_delivery_guardrails_without_granting_authority() {
        let packet = serde_json::json!({
            "proof_coverage": {
                "status": "missing",
                "match_policy": "exact_selected_command"
            },
            "diff": {
                "real_change_count": 1
            },
            "repo_intake": {
                "generated_from": "repo_readiness+repo_task_scout+language_task_alignment",
                "readiness_status": "SETUP_REQUIRED",
                "medium_high_work_ready": false,
                "source_host": "generic",
                "origin_url_present": false,
                "origin_url_recorded": false,
                "provider_calls_allowed": false,
                "network_call_allowed": false,
                "production_write_allowed": false,
                "grants_execution_authority": false
            },
            "repo_intelligence": {
                "principal_orientation_gate": {
                    "required": true,
                    "observed": false
                },
                "related_tests_observed": false
            }
        });
        let guardrails = crate::forge_source_host_pr_draft_route::delivery_guardrails(
            &packet,
            "needs_attention",
        );

        assert_eq!(guardrails["generated_from"], "forge_review_packet");
        assert_eq!(guardrails["repo_intake_present"], true);
        assert_eq!(
            guardrails["repo_intake_generated_from"],
            "repo_readiness+repo_task_scout+language_task_alignment"
        );
        assert_eq!(guardrails["repo_intake_readiness_status"], "SETUP_REQUIRED");
        assert_eq!(guardrails["repo_intake_medium_high_work_ready"], false);
        assert_eq!(guardrails["repo_intake_source_host"], "generic");
        assert_eq!(guardrails["repo_intake_authority_boundary_clear"], true);
        assert_eq!(guardrails["proof_coverage_status"], "missing");
        assert_eq!(guardrails["principal_orientation_gate_required"], true);
        assert_eq!(
            guardrails["successful_semantic_orientation_observed"],
            false
        );
        assert_eq!(guardrails["related_tests_required_for_delivery"], true);
        assert_eq!(guardrails["successful_related_tests_observed"], false);
        assert_eq!(guardrails["review_packet_required"], true);
        assert_eq!(guardrails["grants_delivery_authority"], false);
    }

    #[test]
    fn delivery_guardrail_blockers_fail_closed_before_live_delivery() {
        let guardrails = serde_json::json!({
            "repo_intake_present": true,
            "repo_intake_authority_boundary_clear": true,
            "proof_coverage_status": "missing",
            "principal_orientation_gate_required": true,
            "successful_semantic_orientation_observed": false,
            "related_tests_required_for_delivery": true,
            "successful_related_tests_observed": false
        });
        let blockers =
            crate::forge_source_host_pr_draft_route::delivery_guardrail_blockers(&guardrails);

        assert_eq!(
            blockers,
            vec![
                "proof_coverage_not_satisfied",
                "semantic_orientation_missing",
                "related_tests_missing"
            ]
        );
        assert!(
            !crate::forge_source_host_pr_draft_route::delivery_guardrails_satisfied(&guardrails)
        );

        let satisfied = serde_json::json!({
            "repo_intake_present": true,
            "repo_intake_authority_boundary_clear": true,
            "proof_coverage_status": "satisfied",
            "principal_orientation_gate_required": true,
            "successful_semantic_orientation_observed": true,
            "related_tests_required_for_delivery": true,
            "successful_related_tests_observed": true
        });
        assert!(crate::forge_source_host_pr_draft_route::delivery_guardrails_satisfied(&satisfied));
    }
}
