// Source-host PR draft for Forge. This shapes the Review Packet PR handoff for
// GitHub, Bitbucket, or a generic source host without pushing a branch, opening
// a pull request, calling a network API, or granting ship authority.
use crate::RouteResponse;
use mdx_core::{ForgePrHandoff, MdxKernel, hex, json_string_literal, sha256};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/source-host-pr-drafts.json" {
        return None;
    }
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Some(Ok(response));
    }
    let run_id = json_string_field(body, "run_id").unwrap_or_default();
    if run_id.trim().is_empty() {
        return Some(Ok(refusal(
            "name the run to prepare a source-host PR draft for",
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
    let handoff = &packet["pr_handoff"];
    let title = handoff["title"].as_str().unwrap_or("");
    let body_markdown = handoff["body_markdown"].as_str().unwrap_or("");
    if title.trim().is_empty() || body_markdown.trim().is_empty() {
        return Ok(refusal(
            "review packet did not include a PR handoff title and body",
        ));
    }

    let requested_host =
        json_string_field(body, "target_host").unwrap_or_else(|| "generic".to_string());
    let source_host = normalize_source_host(&requested_host);
    let base_branch = json_string_field(body, "base_branch")
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "main".to_string());
    let source_branch = packet["branch"].as_str().unwrap_or("");
    let review_status = packet["review_status"].as_str().unwrap_or("");
    let summary_line_count = handoff["summary_lines"]
        .as_array()
        .map(|items| items.len())
        .unwrap_or(0);
    let review_checklist_count = handoff["review_checklist"]
        .as_array()
        .map(|items| items.len())
        .unwrap_or(0);
    let body_sha256 = format!("sha256:{}", hex(&sha256(body_markdown.as_bytes())));
    let delivery_guardrails = delivery_guardrails(&packet, review_status);
    let delivery_blocked_reasons = delivery_guardrail_blockers(&delivery_guardrails);
    let delivery_guardrail_status = if delivery_blocked_reasons.is_empty() {
        "SATISFIED"
    } else {
        "BLOCKED"
    };
    let delivery_blocked_reasons_text = delivery_blocked_reasons.join(",");
    let source_host_credentials_present = source_host_credentials_present(source_host);
    let live_delivery_readiness = live_delivery_readiness(
        source_host,
        source_branch,
        base_branch.trim(),
        delivery_guardrail_status,
        &delivery_blocked_reasons,
        source_host_credentials_present,
    );

    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let report = {
        let mut kernel = kernel
            .write()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        match kernel.record_forge_pr_handoff_with_identity(
            ForgePrHandoff {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                run_id,
                branch: source_branch,
                review_status,
                title,
                body_sha256: &body_sha256,
                body_char_count: body_markdown.chars().count(),
                summary_line_count,
                review_checklist_count,
                review_packet_route: "/forge/review-packet.json",
                target_host: source_host,
                delivery_guardrail_status,
                delivery_blocked_reasons: &delivery_blocked_reasons_text,
                repo_intake_present: delivery_guardrails["repo_intake_present"]
                    .as_bool()
                    .unwrap_or(false),
                repo_intake_generated_from: delivery_guardrails["repo_intake_generated_from"]
                    .as_str()
                    .unwrap_or(""),
                repo_intake_readiness_status: delivery_guardrails["repo_intake_readiness_status"]
                    .as_str()
                    .unwrap_or("unknown"),
                repo_intake_medium_high_work_ready:
                    delivery_guardrails["repo_intake_medium_high_work_ready"]
                        .as_bool()
                        .unwrap_or(false),
                repo_intake_source_host: delivery_guardrails["repo_intake_source_host"]
                    .as_str()
                    .unwrap_or("generic"),
                repo_intake_authority_boundary_clear:
                    delivery_guardrails["repo_intake_authority_boundary_clear"]
                        .as_bool()
                        .unwrap_or(false),
                proof_coverage_status: delivery_guardrails["proof_coverage_status"]
                    .as_str()
                    .unwrap_or("unknown"),
                principal_orientation_gate_required:
                    delivery_guardrails["principal_orientation_gate_required"]
                        .as_bool()
                        .unwrap_or(false),
                successful_semantic_orientation_observed:
                    delivery_guardrails["successful_semantic_orientation_observed"]
                        .as_bool()
                        .unwrap_or(false),
                related_tests_required_for_delivery:
                    delivery_guardrails["related_tests_required_for_delivery"]
                        .as_bool()
                        .unwrap_or(false),
                successful_related_tests_observed:
                    delivery_guardrails["successful_related_tests_observed"]
                        .as_bool()
                        .unwrap_or(false),
                candidate_comparison_present: delivery_guardrails["candidate_comparison_present"]
                    .as_bool()
                    .unwrap_or(false),
                candidate_count: delivery_guardrails["candidate_count"].as_u64().unwrap_or(1)
                    as u32,
                candidate_recommended_run_id: delivery_guardrails["candidate_recommended_run_id"]
                    .as_str()
                    .unwrap_or(""),
                candidate_current_rank: delivery_guardrails["candidate_current_rank"]
                    .as_u64()
                    .unwrap_or(1) as u32,
                candidate_current_run_is_recommended:
                    delivery_guardrails["candidate_current_run_is_recommended"]
                        .as_bool()
                        .unwrap_or(true),
                candidate_comparison_grants_authority:
                    delivery_guardrails["candidate_comparison_grants_authority"]
                        .as_bool()
                        .unwrap_or(false),
            },
            &resolved.identity,
        ) {
            Ok(report) => report,
            Err(error) => return Ok(refusal(&error.message())),
        }
    };

    Ok(RouteResponse::json(
        "200 OK",
        serde_json::json!({
            "name": "mdx-forge-source-host-pr-draft",
            "status": "DRAFT_RECORDED",
            "auth_session_status": resolved.auth_session_status,
            "run_id": run_id,
            "review_status": review_status,
            "source_host": source_host,
            "requested_host": requested_host.trim(),
            "target_base_branch": base_branch.trim(),
            "source_branch": source_branch,
            "generated_from": "forge_review_packet.pr_handoff",
            "pr_handoff_id": report.pr_handoff_id,
            "pr_handoff_receipt_id": report.receipt_id,
            "policy_decision_id": report.policy_decision_id,
            "body_sha256": body_sha256,
            "body_char_count": body_markdown.chars().count(),
            "summary_line_count": summary_line_count,
            "review_checklist_count": review_checklist_count,
            "draft_title": title,
            "draft_body_markdown": body_markdown,
            "draft_summary_lines": handoff["summary_lines"].clone(),
            "draft_review_checklist": handoff["review_checklist"].clone(),
            "source_host_plan": source_host_plan(source_host, source_branch, base_branch.trim()),
            "pr_open_authority": pr_open_authority_contract(source_host),
            "live_delivery_readiness": live_delivery_readiness,
            "recommended_candidate_handoff": recommended_candidate_handoff(
                &packet,
                source_host,
                base_branch.trim(),
            ),
            "delivery_guardrails": delivery_guardrails,
            "delivery_guardrail_status": delivery_guardrail_status,
            "ready_for_live_delivery_guardrails": delivery_blocked_reasons.is_empty(),
            "delivery_blocked_reasons": delivery_blocked_reasons,
            "review_packet_route": "/forge/review-packet.json",
            "pr_handoff_route": "/forge/pr-handoffs.json",
            "dry_run": true,
            "credential_values_recorded": false,
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

pub(crate) fn normalize_source_host(value: &str) -> &'static str {
    match value.trim().to_ascii_lowercase().as_str() {
        "github" | "github.com" => "github",
        "bitbucket" | "bitbucket.org" => "bitbucket",
        _ => "generic",
    }
}

pub(crate) fn source_host_plan(
    source_host: &str,
    source_branch: &str,
    target_base_branch: &str,
) -> serde_json::Value {
    let (provider, endpoint_hint, review_surface) = match source_host {
        "github" => (
            "GitHub",
            "POST /repos/{owner}/{repo}/pulls",
            "GitHub pull request",
        ),
        "bitbucket" => (
            "Bitbucket",
            "POST /2.0/repositories/{workspace}/{repo_slug}/pullrequests",
            "Bitbucket pull request",
        ),
        _ => ("generic", "source-host pull request API", "pull request"),
    };
    serde_json::json!({
        "provider": provider,
        "review_surface": review_surface,
        "endpoint_hint": endpoint_hint,
        "source_branch": source_branch,
        "target_base_branch": target_base_branch,
        "operation_order": [
            "verify_review_packet",
            "push_source_branch_after_explicit_operator_action",
            "open_pull_request_after_explicit_operator_action",
            "attach_forge_receipt_summary"
        ],
        "requires_operator_action": true,
        "requires_source_host_credentials": true,
        "dry_run_only": true,
        "remote_push_allowed": false,
        "pull_request_open_allowed": false,
    })
}

pub(crate) fn source_host_credentials_present(source_host: &str) -> bool {
    crate::forge_repo_onboarding_packet_route::credential_sources(source_host)
        .iter()
        .any(|name| {
            std::env::var(name)
                .map(|value| !value.trim().is_empty())
                .unwrap_or(false)
        })
}

pub(crate) fn pr_open_authority_contract(source_host: &str) -> serde_json::Value {
    serde_json::json!({
        "status": "OPERATOR_ACTION_REQUIRED",
        "source_host": source_host,
        "draft_route": "/forge/source-host-pr-drafts.json",
        "live_delivery_route": "/forge/source-host-live-deliveries.json",
        "readiness_route": "/forge/source-host-readiness.json",
        "requires_explicit_operator_action": true,
        "draft_network_call_allowed": false,
        "remote_push_allowed": false,
        "pull_request_open_allowed": false,
        "approval_allowed": false,
        "deployment_authority_granted": false,
        "production_write_allowed": false,
    })
}

pub(crate) fn live_delivery_readiness(
    source_host: &str,
    source_branch: &str,
    target_base_branch: &str,
    delivery_guardrail_status: &str,
    delivery_blocked_reasons: &[&str],
    credentials_present: bool,
) -> serde_json::Value {
    let credential_sources =
        crate::forge_repo_onboarding_packet_route::credential_sources(source_host);
    let mut missing_operator_actions = Vec::new();
    if source_host == "generic" {
        missing_operator_actions.push("choose_github_or_bitbucket_target_host");
    }
    if source_branch.trim().is_empty() {
        missing_operator_actions.push("finish_run_with_source_branch");
    }
    if target_base_branch.trim().is_empty() {
        missing_operator_actions.push("choose_target_base_branch");
    }
    if delivery_guardrail_status != "SATISFIED" {
        missing_operator_actions.push("clear_delivery_guardrails");
    }
    if !credentials_present {
        missing_operator_actions.push("provide_source_host_credentials");
    }
    let status = if missing_operator_actions.is_empty() {
        "READY_FOR_OPERATOR_DELIVERY"
    } else if delivery_guardrail_status == "SATISFIED"
        && !source_branch.trim().is_empty()
        && source_host != "generic"
        && !credentials_present
    {
        "READY_AFTER_CREDENTIAL_SETUP"
    } else {
        "BLOCKED"
    };
    serde_json::json!({
        "generated_from": "source_host_plan+delivery_guardrails+credential_presence",
        "status": status,
        "source_host": source_host,
        "source_branch": source_branch,
        "target_base_branch": target_base_branch,
        "delivery_guardrail_status": delivery_guardrail_status,
        "delivery_blocked_reasons": delivery_blocked_reasons,
        "credential_sources_checked": credential_sources,
        "source_host_credentials_present": credentials_present,
        "credential_values_recorded": false,
        "required_operator_actions": [
            "push_source_branch",
            "open_pull_request",
            "paste_or_attach_forge_receipt_summary"
        ],
        "missing_operator_actions": missing_operator_actions,
        "network_call_allowed": false,
        "remote_push_allowed": false,
        "pull_request_open_allowed": false,
        "production_write_allowed": false,
    })
}

pub(crate) fn recommended_candidate_handoff(
    packet: &serde_json::Value,
    source_host: &str,
    target_base_branch: &str,
) -> serde_json::Value {
    let recommendation = &packet["pr_handoff"]["candidate_recommendation"];
    let comparison = &packet["candidate_comparison"];
    let candidate_count = recommendation["candidate_count"]
        .as_u64()
        .or_else(|| comparison["candidate_count"].as_u64())
        .unwrap_or(1);
    let current_run_id = recommendation["current_run_id"]
        .as_str()
        .or_else(|| comparison["current_run_id"].as_str())
        .unwrap_or("");
    let recommended_run_id = recommendation["recommended_run_id"]
        .as_str()
        .or_else(|| comparison["recommended_run_id"].as_str())
        .unwrap_or("");
    let current_rank = recommendation["current_rank"]
        .as_u64()
        .or_else(|| comparison["current_rank"].as_u64())
        .unwrap_or(1);
    let current_run_is_recommended = recommendation["current_run_is_recommended"]
        .as_bool()
        .unwrap_or_else(|| {
            candidate_count <= 1
                || recommended_run_id.trim().is_empty()
                || current_run_id == recommended_run_id
        });
    let available =
        candidate_count > 1 && !recommended_run_id.trim().is_empty() && !current_run_is_recommended;
    let reason = if available {
        "candidate_comparison_recommends_another_run"
    } else if candidate_count > 1 {
        "current_run_is_recommended"
    } else {
        "single_candidate_run"
    };
    serde_json::json!({
        "available": available,
        "generated_from": "pr_handoff.candidate_recommendation",
        "reason": reason,
        "candidate_count": candidate_count,
        "current_run_id": current_run_id,
        "current_rank": current_rank,
        "recommended_run_id": recommended_run_id,
        "current_run_is_recommended": current_run_is_recommended,
        "route": "/forge/source-host-pr-drafts.json",
        "method": "POST",
        "request_body": {
            "run_id": recommended_run_id,
            "target_host": source_host,
            "base_branch": target_base_branch
        },
        "network_call_allowed": false,
        "remote_push_allowed": false,
        "pull_request_open_allowed": false,
        "production_write_allowed": false,
    })
}

pub(crate) fn delivery_guardrail_blockers(guardrails: &serde_json::Value) -> Vec<&'static str> {
    let mut blocked_reasons = Vec::new();
    if !guardrails["repo_intake_present"].as_bool().unwrap_or(false) {
        blocked_reasons.push("repo_intake_missing");
    }
    if !guardrails["repo_intake_authority_boundary_clear"]
        .as_bool()
        .unwrap_or(false)
    {
        blocked_reasons.push("repo_intake_authority_boundary_breached");
    }
    if guardrails["proof_coverage_status"].as_str() != Some("satisfied") {
        blocked_reasons.push("proof_coverage_not_satisfied");
    }
    if guardrails["behavior_proof_required_for_delivery"]
        .as_bool()
        .unwrap_or(false)
        && guardrails["behavior_proof_status"].as_str() != Some("covered")
    {
        blocked_reasons.push("behavior_proof_not_covered");
    }
    if guardrails["principal_orientation_gate_required"]
        .as_bool()
        .unwrap_or(false)
        && !guardrails["successful_semantic_orientation_observed"]
            .as_bool()
            .unwrap_or(false)
    {
        blocked_reasons.push("semantic_orientation_missing");
    }
    if guardrails["related_tests_required_for_delivery"]
        .as_bool()
        .unwrap_or(false)
        && !guardrails["successful_related_tests_observed"]
            .as_bool()
            .unwrap_or(false)
    {
        blocked_reasons.push("related_tests_missing");
    }
    if guardrails["dependency_map_required_for_delivery"]
        .as_bool()
        .unwrap_or(false)
        && !guardrails["successful_dependency_map_observed"]
            .as_bool()
            .unwrap_or(false)
    {
        blocked_reasons.push("dependency_map_missing");
    }
    if guardrails["candidate_strategy_semantic_operations_required_for_delivery"]
        .as_bool()
        .unwrap_or(false)
        && !guardrails["candidate_strategy_semantic_operations_satisfied"]
            .as_bool()
            .unwrap_or(true)
    {
        blocked_reasons.push("candidate_strategy_semantic_operations_missing");
    }
    if guardrails["fleet_stream_strategy_semantic_operations_required_for_delivery"]
        .as_bool()
        .unwrap_or(false)
        && !guardrails["fleet_stream_strategy_semantic_operations_satisfied"]
            .as_bool()
            .unwrap_or(true)
    {
        blocked_reasons.push("fleet_stream_strategy_semantic_operations_missing");
    }
    if guardrails["fleet_integration_strategy_semantic_operations_required_for_delivery"]
        .as_bool()
        .unwrap_or(false)
        && !guardrails["fleet_integration_strategy_semantic_operations_satisfied"]
            .as_bool()
            .unwrap_or(true)
    {
        blocked_reasons.push("fleet_integration_strategy_semantic_operations_missing");
    }
    if guardrails["fleet_selection_required_for_delivery"]
        .as_bool()
        .unwrap_or(false)
        && !guardrails["fleet_selection_current_run_is_recommended"]
            .as_bool()
            .unwrap_or(true)
    {
        blocked_reasons.push("fleet_stream_not_recommended");
    }
    if guardrails["candidate_selection_required_for_delivery"]
        .as_bool()
        .unwrap_or(false)
        && !guardrails["candidate_selection_satisfies_delivery"]
            .as_bool()
            .unwrap_or(true)
    {
        if !guardrails["candidate_selection_recorded"]
            .as_bool()
            .unwrap_or(false)
        {
            blocked_reasons.push("candidate_selection_missing");
        } else if !guardrails["candidate_selection_current_run_is_selected"]
            .as_bool()
            .unwrap_or(false)
        {
            blocked_reasons.push("candidate_not_selected");
        } else if !guardrails["candidate_current_run_is_recommended"]
            .as_bool()
            .unwrap_or(true)
        {
            blocked_reasons.push("candidate_not_recommended");
        } else {
            blocked_reasons.push("candidate_selection_override_reason_missing");
        }
    }
    blocked_reasons
}

pub(crate) fn delivery_guardrails_satisfied(guardrails: &serde_json::Value) -> bool {
    delivery_guardrail_blockers(guardrails).is_empty()
}

pub(crate) fn delivery_guardrails(
    packet: &serde_json::Value,
    review_status: &str,
) -> serde_json::Value {
    let principal_orientation_gate_required =
        packet["repo_intelligence"]["principal_orientation_gate"]["required"]
            .as_bool()
            .unwrap_or(false);
    let successful_related_tests_observed = packet["repo_intelligence"]["related_tests_observed"]
        .as_bool()
        .unwrap_or(false);
    let repo_intake = &packet["repo_intake"];
    let repo_intake_generated_from = repo_intake["generated_from"].as_str().unwrap_or("");
    let repo_intake_present = !repo_intake_generated_from.trim().is_empty();
    let repo_intake_authority_boundary_clear = !repo_intake["provider_calls_allowed"]
        .as_bool()
        .unwrap_or(false)
        && !repo_intake["origin_url_recorded"]
            .as_bool()
            .unwrap_or(false)
        && !repo_intake["network_call_allowed"]
            .as_bool()
            .unwrap_or(false)
        && !repo_intake["production_write_allowed"]
            .as_bool()
            .unwrap_or(false)
        && !repo_intake["grants_execution_authority"]
            .as_bool()
            .unwrap_or(false);
    let related_tests_required_for_delivery = principal_orientation_gate_required
        && packet["diff"]["real_change_count"].as_u64().unwrap_or(0) > 0;
    let proof_scope_status = packet["proof_scope"]["status"]
        .as_str()
        .unwrap_or("not_applicable");
    let behavior_proof_status = packet["behavior_proof"]["status"]
        .as_str()
        .unwrap_or("unknown");
    let behavior_proof_required_for_delivery =
        packet["diff"]["real_change_count"].as_u64().unwrap_or(0) > 0
            && proof_scope_status != "not_applicable";
    let candidate_comparison = &packet["candidate_comparison"];
    let candidate_count = candidate_comparison["candidate_count"]
        .as_u64()
        .unwrap_or(1);
    let candidate_current_run_id = candidate_comparison["current_run_id"]
        .as_str()
        .unwrap_or("");
    let candidate_recommended_run_id = candidate_comparison["recommended_run_id"]
        .as_str()
        .unwrap_or("");
    let candidate_current_run_is_recommended = candidate_count <= 1
        || candidate_recommended_run_id.trim().is_empty()
        || candidate_current_run_id == candidate_recommended_run_id;
    let candidate_comparison_present = candidate_comparison["generated_from"].as_str()
        == Some("forge.run.event.parallel_candidates");
    let candidate_comparison_required_for_delivery = candidate_count > 1;
    let candidate_selection = &packet["repo_intelligence"]["candidate_selection"];
    let candidate_selection_recorded = candidate_selection["status"].as_str() == Some("recorded");
    let candidate_selection_selected_run_id = candidate_selection["selected_run_id"]
        .as_str()
        .unwrap_or("");
    let candidate_selection_current_run_is_selected = candidate_count <= 1
        || (candidate_selection_recorded
            && candidate_selection["current_run_is_selected"]
                .as_bool()
                .unwrap_or(false)
            && candidate_selection_selected_run_id == candidate_current_run_id);
    let candidate_selection_matches_recommendation = candidate_selection["matches_recommendation"]
        .as_bool()
        .unwrap_or(false);
    let candidate_selection_override_reason_recorded =
        candidate_selection["override_reason_recorded"]
            .as_bool()
            .unwrap_or(false);
    let candidate_selection_required_for_delivery = candidate_comparison_required_for_delivery;
    let candidate_selection_satisfies_delivery = !candidate_selection_required_for_delivery
        || (candidate_selection_recorded
            && candidate_selection_current_run_is_selected
            && (candidate_selection_matches_recommendation
                || candidate_selection_override_reason_recorded));
    let semantic_operations = packet["repo_intelligence"]["semantic_query_operations_observed"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let successful_dependency_map_observed = semantic_operations
        .iter()
        .any(|operation| operation.as_str() == Some("dependency_map"));
    let dependency_map_required_for_delivery = candidate_comparison_required_for_delivery;
    let required_strategy_operations =
        packet["repo_intelligence"]["parallel_candidate"]["required_semantic_operations"]
            .as_array()
            .cloned()
            .unwrap_or_default();
    let missing_strategy_operations = required_strategy_operations
        .iter()
        .filter_map(|operation| operation.as_str())
        .filter(|required| {
            !semantic_operations
                .iter()
                .any(|observed| observed.as_str() == Some(*required))
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    let candidate_strategy_semantic_operations_required_for_delivery =
        candidate_comparison_required_for_delivery && !required_strategy_operations.is_empty();
    let candidate_strategy_semantic_operations_satisfied = missing_strategy_operations.is_empty();
    let fleet_stream = &packet["repo_intelligence"]["fleet_stream"];
    let fleet_stream_required_strategy_operations = fleet_stream["required_semantic_operations"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let missing_fleet_stream_strategy_operations = fleet_stream_required_strategy_operations
        .iter()
        .filter_map(|operation| operation.as_str())
        .filter(|required| {
            !semantic_operations
                .iter()
                .any(|observed| observed.as_str() == Some(*required))
        })
        .map(str::to_string)
        .collect::<Vec<_>>();
    let fleet_stream_strategy_semantic_operations_required_for_delivery =
        fleet_stream["role"].as_str() == Some("stream")
            && !fleet_stream_required_strategy_operations.is_empty();
    let fleet_stream_strategy_semantic_operations_satisfied =
        missing_fleet_stream_strategy_operations.is_empty();
    let fleet_integration = &packet["repo_intelligence"]["fleet_integration"];
    let fleet_integration_required_strategy_operations =
        fleet_integration["required_semantic_operations"]
            .as_array()
            .cloned()
            .unwrap_or_default();
    let missing_fleet_integration_strategy_operations =
        fleet_integration_required_strategy_operations
            .iter()
            .filter_map(|operation| operation.as_str())
            .filter(|required| {
                !semantic_operations
                    .iter()
                    .any(|observed| observed.as_str() == Some(*required))
            })
            .map(str::to_string)
            .collect::<Vec<_>>();
    let fleet_integration_strategy_semantic_operations_required_for_delivery =
        fleet_integration["role"].as_str() == Some("integration")
            && !fleet_integration_required_strategy_operations.is_empty();
    let fleet_integration_strategy_semantic_operations_satisfied =
        missing_fleet_integration_strategy_operations.is_empty();
    let fleet_selection = &packet["repo_intelligence"]["fleet_selection"];
    let fleet_selection_required_for_delivery = fleet_selection["selection_required_for_delivery"]
        .as_bool()
        .unwrap_or(false);
    let fleet_selection_current_run_is_recommended = fleet_selection["current_run_is_recommended"]
        .as_bool()
        .unwrap_or(true);
    let fleet_selection_candidate_count = fleet_selection["candidate_count"].as_u64().unwrap_or(1);
    let fleet_selection_current_rank = fleet_selection["current_rank"].as_u64().unwrap_or(1);
    let fleet_selection_recommended_run_id =
        fleet_selection["recommended_run_id"].as_str().unwrap_or("");
    let fleet_selection_recommended_stream_id = fleet_selection["recommended_stream_id"]
        .as_str()
        .unwrap_or("");
    let mut guardrails = serde_json::Map::new();
    guardrails.insert(
        "generated_from".to_string(),
        serde_json::json!("forge_review_packet"),
    );
    guardrails.insert(
        "review_status".to_string(),
        serde_json::json!(review_status),
    );
    guardrails.insert(
        "repo_intake_present".to_string(),
        serde_json::json!(repo_intake_present),
    );
    guardrails.insert(
        "repo_intake_generated_from".to_string(),
        serde_json::json!(repo_intake_generated_from),
    );
    guardrails.insert(
        "repo_intake_readiness_status".to_string(),
        serde_json::json!(
            repo_intake["readiness_status"]
                .as_str()
                .unwrap_or("unknown")
        ),
    );
    guardrails.insert(
        "repo_intake_medium_high_work_ready".to_string(),
        serde_json::json!(
            repo_intake["medium_high_work_ready"]
                .as_bool()
                .unwrap_or(false)
        ),
    );
    guardrails.insert(
        "repo_intake_source_host".to_string(),
        serde_json::json!(repo_intake["source_host"].as_str().unwrap_or("generic")),
    );
    guardrails.insert(
        "repo_intake_origin_url_present".to_string(),
        serde_json::json!(repo_intake["origin_url_present"].as_bool().unwrap_or(false)),
    );
    guardrails.insert(
        "repo_intake_origin_url_recorded".to_string(),
        serde_json::json!(
            repo_intake["origin_url_recorded"]
                .as_bool()
                .unwrap_or(false)
        ),
    );
    guardrails.insert(
        "repo_intake_provider_calls_allowed".to_string(),
        serde_json::json!(
            repo_intake["provider_calls_allowed"]
                .as_bool()
                .unwrap_or(false)
        ),
    );
    guardrails.insert(
        "repo_intake_network_call_allowed".to_string(),
        serde_json::json!(
            repo_intake["network_call_allowed"]
                .as_bool()
                .unwrap_or(false)
        ),
    );
    guardrails.insert(
        "repo_intake_production_write_allowed".to_string(),
        serde_json::json!(
            repo_intake["production_write_allowed"]
                .as_bool()
                .unwrap_or(false)
        ),
    );
    guardrails.insert(
        "repo_intake_grants_execution_authority".to_string(),
        serde_json::json!(
            repo_intake["grants_execution_authority"]
                .as_bool()
                .unwrap_or(false)
        ),
    );
    guardrails.insert(
        "repo_intake_authority_boundary_clear".to_string(),
        serde_json::json!(repo_intake_authority_boundary_clear),
    );
    guardrails.insert(
        "proof_coverage_status".to_string(),
        serde_json::json!(
            packet["proof_coverage"]["status"]
                .as_str()
                .unwrap_or("unknown")
        ),
    );
    guardrails.insert(
        "proof_match_policy".to_string(),
        serde_json::json!(
            packet["proof_coverage"]["match_policy"]
                .as_str()
                .unwrap_or("unknown")
        ),
    );
    guardrails.insert(
        "proof_scope_status".to_string(),
        serde_json::json!(proof_scope_status),
    );
    guardrails.insert(
        "behavior_proof_status".to_string(),
        serde_json::json!(behavior_proof_status),
    );
    guardrails.insert(
        "behavior_proof_required_for_delivery".to_string(),
        serde_json::json!(behavior_proof_required_for_delivery),
    );
    guardrails.insert(
        "principal_orientation_gate_required".to_string(),
        serde_json::json!(principal_orientation_gate_required),
    );
    guardrails.insert(
        "successful_semantic_orientation_observed".to_string(),
        serde_json::json!(
            packet["repo_intelligence"]["principal_orientation_gate"]["observed"]
                .as_bool()
                .unwrap_or(false)
        ),
    );
    guardrails.insert(
        "related_tests_required_for_delivery".to_string(),
        serde_json::json!(related_tests_required_for_delivery),
    );
    guardrails.insert(
        "successful_related_tests_observed".to_string(),
        serde_json::json!(successful_related_tests_observed),
    );
    guardrails.insert(
        "dependency_map_required_for_delivery".to_string(),
        serde_json::json!(dependency_map_required_for_delivery),
    );
    guardrails.insert(
        "successful_dependency_map_observed".to_string(),
        serde_json::json!(successful_dependency_map_observed),
    );
    guardrails.insert(
        "candidate_strategy_semantic_operations_required_for_delivery".to_string(),
        serde_json::json!(candidate_strategy_semantic_operations_required_for_delivery),
    );
    guardrails.insert(
        "candidate_strategy_semantic_operations_satisfied".to_string(),
        serde_json::json!(candidate_strategy_semantic_operations_satisfied),
    );
    guardrails.insert(
        "missing_candidate_strategy_semantic_operations".to_string(),
        serde_json::json!(missing_strategy_operations),
    );
    guardrails.insert(
        "fleet_stream_strategy_semantic_operations_required_for_delivery".to_string(),
        serde_json::json!(fleet_stream_strategy_semantic_operations_required_for_delivery),
    );
    guardrails.insert(
        "fleet_stream_strategy_semantic_operations_satisfied".to_string(),
        serde_json::json!(fleet_stream_strategy_semantic_operations_satisfied),
    );
    guardrails.insert(
        "missing_fleet_stream_strategy_semantic_operations".to_string(),
        serde_json::json!(missing_fleet_stream_strategy_operations),
    );
    guardrails.insert(
        "fleet_integration_strategy_semantic_operations_required_for_delivery".to_string(),
        serde_json::json!(fleet_integration_strategy_semantic_operations_required_for_delivery),
    );
    guardrails.insert(
        "fleet_integration_strategy_semantic_operations_satisfied".to_string(),
        serde_json::json!(fleet_integration_strategy_semantic_operations_satisfied),
    );
    guardrails.insert(
        "missing_fleet_integration_strategy_semantic_operations".to_string(),
        serde_json::json!(missing_fleet_integration_strategy_operations),
    );
    guardrails.insert(
        "fleet_selection_required_for_delivery".to_string(),
        serde_json::json!(fleet_selection_required_for_delivery),
    );
    guardrails.insert(
        "fleet_selection_current_run_is_recommended".to_string(),
        serde_json::json!(fleet_selection_current_run_is_recommended),
    );
    guardrails.insert(
        "fleet_selection_candidate_count".to_string(),
        serde_json::json!(fleet_selection_candidate_count),
    );
    guardrails.insert(
        "fleet_selection_current_rank".to_string(),
        serde_json::json!(fleet_selection_current_rank),
    );
    guardrails.insert(
        "fleet_selection_recommended_run_id".to_string(),
        serde_json::json!(fleet_selection_recommended_run_id),
    );
    guardrails.insert(
        "fleet_selection_recommended_stream_id".to_string(),
        serde_json::json!(fleet_selection_recommended_stream_id),
    );
    guardrails.insert(
        "candidate_comparison_present".to_string(),
        serde_json::json!(candidate_comparison_present),
    );
    guardrails.insert(
        "candidate_comparison_required_for_delivery".to_string(),
        serde_json::json!(candidate_comparison_required_for_delivery),
    );
    guardrails.insert(
        "candidate_selection_required_for_delivery".to_string(),
        serde_json::json!(candidate_selection_required_for_delivery),
    );
    guardrails.insert(
        "candidate_selection_recorded".to_string(),
        serde_json::json!(candidate_selection_recorded),
    );
    guardrails.insert(
        "candidate_selection_selected_run_id".to_string(),
        serde_json::json!(candidate_selection_selected_run_id),
    );
    guardrails.insert(
        "candidate_selection_current_run_is_selected".to_string(),
        serde_json::json!(candidate_selection_current_run_is_selected),
    );
    guardrails.insert(
        "candidate_selection_matches_recommendation".to_string(),
        serde_json::json!(candidate_selection_matches_recommendation),
    );
    guardrails.insert(
        "candidate_selection_override_reason_recorded".to_string(),
        serde_json::json!(candidate_selection_override_reason_recorded),
    );
    guardrails.insert(
        "candidate_selection_satisfies_delivery".to_string(),
        serde_json::json!(candidate_selection_satisfies_delivery),
    );
    guardrails.insert(
        "candidate_count".to_string(),
        serde_json::json!(candidate_count),
    );
    guardrails.insert(
        "candidate_current_run_id".to_string(),
        serde_json::json!(candidate_current_run_id),
    );
    guardrails.insert(
        "candidate_recommended_run_id".to_string(),
        serde_json::json!(candidate_recommended_run_id),
    );
    guardrails.insert(
        "candidate_current_rank".to_string(),
        serde_json::json!(candidate_comparison["current_rank"].as_u64().unwrap_or(1)),
    );
    guardrails.insert(
        "candidate_current_run_is_recommended".to_string(),
        serde_json::json!(candidate_current_run_is_recommended),
    );
    guardrails.insert(
        "candidate_comparison_diff_quality_included".to_string(),
        serde_json::json!(
            candidate_comparison["diff_quality_included"]
                .as_bool()
                .unwrap_or(false)
        ),
    );
    guardrails.insert(
        "candidate_comparison_model_judgment_included".to_string(),
        serde_json::json!(
            candidate_comparison["model_judgment_included"]
                .as_bool()
                .unwrap_or(false)
        ),
    );
    guardrails.insert(
        "candidate_comparison_grants_authority".to_string(),
        serde_json::json!(
            candidate_comparison["grants_execution_authority"]
                .as_bool()
                .unwrap_or(false)
        ),
    );
    guardrails.insert(
        "remote_push_requires_operator_action".to_string(),
        serde_json::json!(true),
    );
    guardrails.insert(
        "pull_request_open_requires_operator_action".to_string(),
        serde_json::json!(true),
    );
    guardrails.insert(
        "review_packet_required".to_string(),
        serde_json::json!(true),
    );
    guardrails.insert(
        "credential_values_recorded".to_string(),
        serde_json::json!(false),
    );
    guardrails.insert("dry_run_only".to_string(), serde_json::json!(true));
    guardrails.insert(
        "grants_delivery_authority".to_string(),
        serde_json::json!(false),
    );
    serde_json::Value::Object(guardrails)
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-source-host-pr-draft","status":"REFUSED","reason":{},"pr_handoff_receipt_id":"","credential_values_recorded":false,"network_call_allowed":false,"remote_push_allowed":false,"pull_request_open_allowed":false,"approval_allowed":false,"deployment_authority_granted":false,"production_write_allowed":false}}"#,
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
            r#"{"run_id":"forge_run_missing","target_host":"github","base_branch":"main"}"#,
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
                .contains(r#""production_write_allowed":false"#)
        );
    }

    #[test]
    fn source_host_plan_keeps_github_and_bitbucket_dry() {
        let github = source_host_plan("github", "forge/run_1", "main");
        assert_eq!(github["provider"], "GitHub");
        assert_eq!(github["dry_run_only"], true);
        assert_eq!(github["remote_push_allowed"], false);
        let bitbucket = source_host_plan("bitbucket", "forge/run_1", "develop");
        assert_eq!(bitbucket["provider"], "Bitbucket");
        assert_eq!(bitbucket["pull_request_open_allowed"], false);
        assert_eq!(normalize_source_host("github.com"), "github");
        assert_eq!(normalize_source_host("bitbucket.org"), "bitbucket");
        assert_eq!(normalize_source_host("unknown"), "generic");
    }

    #[test]
    fn pr_open_authority_contract_points_to_live_delivery_without_granting_authority() {
        let authority = pr_open_authority_contract("github");

        assert_eq!(authority["status"], "OPERATOR_ACTION_REQUIRED");
        assert_eq!(authority["source_host"], "github");
        assert_eq!(
            authority["draft_route"],
            "/forge/source-host-pr-drafts.json"
        );
        assert_eq!(
            authority["live_delivery_route"],
            "/forge/source-host-live-deliveries.json"
        );
        assert_eq!(authority["requires_explicit_operator_action"], true);
        assert_eq!(authority["remote_push_allowed"], false);
        assert_eq!(authority["pull_request_open_allowed"], false);
        assert_eq!(authority["production_write_allowed"], false);
    }

    #[test]
    fn live_delivery_readiness_is_honest_about_credentials_without_recording_values() {
        let readiness =
            live_delivery_readiness("github", "forge/run_1", "main", "SATISFIED", &[], false);

        assert_eq!(readiness["status"], "READY_AFTER_CREDENTIAL_SETUP");
        assert_eq!(readiness["source_host"], "github");
        assert_eq!(readiness["credential_sources_checked"][0], "GITHUB_TOKEN");
        assert_eq!(readiness["source_host_credentials_present"], false);
        assert_eq!(readiness["credential_values_recorded"], false);
        assert_eq!(readiness["network_call_allowed"], false);
        assert_eq!(readiness["remote_push_allowed"], false);
        assert_eq!(readiness["pull_request_open_allowed"], false);
        assert!(
            readiness["missing_operator_actions"]
                .as_array()
                .expect("missing actions")
                .iter()
                .any(|item| item == "provide_source_host_credentials")
        );
    }

    #[test]
    fn live_delivery_readiness_reports_ready_only_after_guardrails_branch_host_and_credentials() {
        let ready = live_delivery_readiness(
            "bitbucket",
            "forge/run_1",
            "develop",
            "SATISFIED",
            &[],
            true,
        );

        assert_eq!(ready["status"], "READY_FOR_OPERATOR_DELIVERY");
        assert_eq!(ready["source_host"], "bitbucket");
        assert_eq!(ready["source_host_credentials_present"], true);
        assert!(
            ready["missing_operator_actions"]
                .as_array()
                .expect("missing actions")
                .is_empty()
        );

        let blocked = live_delivery_readiness(
            "generic",
            "",
            "main",
            "BLOCKED",
            &["proof_coverage_not_satisfied"],
            false,
        );
        assert_eq!(blocked["status"], "BLOCKED");
        assert_eq!(
            blocked["delivery_blocked_reasons"][0],
            "proof_coverage_not_satisfied"
        );
        assert!(
            blocked["missing_operator_actions"]
                .as_array()
                .expect("missing actions")
                .iter()
                .any(|item| item == "choose_github_or_bitbucket_target_host")
        );
        assert!(
            blocked["missing_operator_actions"]
                .as_array()
                .expect("missing actions")
                .iter()
                .any(|item| item == "finish_run_with_source_branch")
        );
    }

    #[test]
    fn recommended_candidate_handoff_points_non_winning_candidate_to_winner() {
        let packet = serde_json::json!({
            "pr_handoff": {
                "candidate_recommendation": {
                    "candidate_count": 4,
                    "current_run_id": "forge_run_candidate_3",
                    "recommended_run_id": "forge_run_candidate_1",
                    "current_rank": 3,
                    "current_run_is_recommended": false
                }
            },
            "candidate_comparison": {
                "candidate_count": 4,
                "current_run_id": "forge_run_candidate_3",
                "recommended_run_id": "forge_run_candidate_1",
                "current_rank": 3
            }
        });

        let handoff = recommended_candidate_handoff(&packet, "github", "main");

        assert_eq!(handoff["available"], true);
        assert_eq!(
            handoff["reason"],
            "candidate_comparison_recommends_another_run"
        );
        assert_eq!(handoff["current_run_id"], "forge_run_candidate_3");
        assert_eq!(handoff["current_rank"], 3);
        assert_eq!(handoff["recommended_run_id"], "forge_run_candidate_1");
        assert_eq!(handoff["request_body"]["run_id"], "forge_run_candidate_1");
        assert_eq!(handoff["request_body"]["target_host"], "github");
        assert_eq!(handoff["request_body"]["base_branch"], "main");
        assert_eq!(handoff["network_call_allowed"], false);
        assert_eq!(handoff["pull_request_open_allowed"], false);
        assert_eq!(handoff["production_write_allowed"], false);
    }

    #[test]
    fn delivery_guardrails_carry_review_packet_evidence_without_authority() {
        let packet = serde_json::json!({
            "proof_coverage": {
                "status": "satisfied",
                "match_policy": "exact_selected_command"
            },
            "diff": {
                "real_change_count": 1
            },
            "proof_scope": {
                "status": "covered"
            },
            "behavior_proof": {
                "status": "covered"
            },
            "repo_intake": {
                "generated_from": "repo_readiness+repo_task_scout+language_task_alignment",
                "readiness_status": "READY_FOR_MEDIUM_HIGH_WORK",
                "medium_high_work_ready": true,
                "source_host": "github",
                "origin_url_present": true,
                "origin_url_recorded": false,
                "provider_calls_allowed": false,
                "network_call_allowed": false,
                "production_write_allowed": false,
                "grants_execution_authority": false
            },
            "repo_intelligence": {
                "principal_orientation_gate": {
                    "required": true,
                    "observed": true
                },
                "related_tests_observed": true,
                "semantic_query_operations_observed": ["orientation", "related_tests"],
                "candidate_selection": {
                    "status": "recorded",
                    "selected_run_id": "forge_run_1",
                    "current_run_is_selected": true,
                    "matches_recommendation": true,
                    "override_reason_recorded": false
                }
            },
            "candidate_comparison": {
                "generated_from": "forge.run.event.parallel_candidates",
                "current_run_id": "forge_run_1",
                "recommended_run_id": "forge_run_1",
                "candidate_count": 1,
                "current_rank": 1,
                "diff_quality_included": true,
                "model_judgment_included": false,
                "grants_execution_authority": false
            }
        });
        let guardrails = delivery_guardrails(&packet, "ready");

        assert_eq!(guardrails["generated_from"], "forge_review_packet");
        assert_eq!(guardrails["repo_intake_present"], true);
        assert_eq!(
            guardrails["repo_intake_generated_from"],
            "repo_readiness+repo_task_scout+language_task_alignment"
        );
        assert_eq!(
            guardrails["repo_intake_readiness_status"],
            "READY_FOR_MEDIUM_HIGH_WORK"
        );
        assert_eq!(guardrails["repo_intake_medium_high_work_ready"], true);
        assert_eq!(guardrails["repo_intake_source_host"], "github");
        assert_eq!(guardrails["repo_intake_origin_url_present"], true);
        assert_eq!(guardrails["repo_intake_origin_url_recorded"], false);
        assert_eq!(guardrails["repo_intake_provider_calls_allowed"], false);
        assert_eq!(guardrails["repo_intake_network_call_allowed"], false);
        assert_eq!(guardrails["repo_intake_production_write_allowed"], false);
        assert_eq!(guardrails["repo_intake_grants_execution_authority"], false);
        assert_eq!(guardrails["repo_intake_authority_boundary_clear"], true);
        assert_eq!(guardrails["proof_coverage_status"], "satisfied");
        assert_eq!(guardrails["proof_match_policy"], "exact_selected_command");
        assert_eq!(guardrails["proof_scope_status"], "covered");
        assert_eq!(guardrails["behavior_proof_status"], "covered");
        assert_eq!(guardrails["behavior_proof_required_for_delivery"], true);
        assert_eq!(guardrails["principal_orientation_gate_required"], true);
        assert_eq!(guardrails["successful_semantic_orientation_observed"], true);
        assert_eq!(guardrails["related_tests_required_for_delivery"], true);
        assert_eq!(guardrails["successful_related_tests_observed"], true);
        assert_eq!(guardrails["dependency_map_required_for_delivery"], false);
        assert_eq!(guardrails["successful_dependency_map_observed"], false);
        assert_eq!(guardrails["candidate_comparison_present"], true);
        assert_eq!(
            guardrails["candidate_comparison_required_for_delivery"],
            false
        );
        assert_eq!(guardrails["candidate_count"], 1);
        assert_eq!(guardrails["candidate_current_run_id"], "forge_run_1");
        assert_eq!(guardrails["candidate_recommended_run_id"], "forge_run_1");
        assert_eq!(guardrails["candidate_current_rank"], 1);
        assert_eq!(guardrails["candidate_current_run_is_recommended"], true);
        assert_eq!(
            guardrails["candidate_comparison_diff_quality_included"],
            true
        );
        assert_eq!(
            guardrails["candidate_comparison_model_judgment_included"],
            false
        );
        assert_eq!(guardrails["candidate_comparison_grants_authority"], false);
        assert_eq!(guardrails["remote_push_requires_operator_action"], true);
        assert_eq!(
            guardrails["pull_request_open_requires_operator_action"],
            true
        );
        assert_eq!(guardrails["credential_values_recorded"], false);
        assert_eq!(guardrails["grants_delivery_authority"], false);
        assert!(delivery_guardrails_satisfied(&guardrails));
        assert!(delivery_guardrail_blockers(&guardrails).is_empty());
    }

    #[test]
    fn delivery_guardrail_blockers_name_missing_proof_and_orientation() {
        let guardrails = serde_json::json!({
            "repo_intake_present": true,
            "repo_intake_authority_boundary_clear": true,
            "proof_coverage_status": "missing",
            "behavior_proof_required_for_delivery": true,
            "behavior_proof_status": "weak",
            "principal_orientation_gate_required": true,
            "successful_semantic_orientation_observed": false,
            "related_tests_required_for_delivery": true,
            "successful_related_tests_observed": false,
            "dependency_map_required_for_delivery": true,
            "successful_dependency_map_observed": false
        });

        assert_eq!(
            delivery_guardrail_blockers(&guardrails),
            vec![
                "proof_coverage_not_satisfied",
                "behavior_proof_not_covered",
                "semantic_orientation_missing",
                "related_tests_missing",
                "dependency_map_missing"
            ]
        );
        assert!(!delivery_guardrails_satisfied(&guardrails));
    }

    #[test]
    fn delivery_guardrails_block_weak_behavior_proof_for_source_changes() {
        let packet = serde_json::json!({
            "proof_coverage": {
                "status": "satisfied",
                "match_policy": "exact_selected_command"
            },
            "proof_scope": {
                "status": "covered"
            },
            "behavior_proof": {
                "status": "weak"
            },
            "diff": {
                "real_change_count": 1
            },
            "repo_intake": {
                "generated_from": "repo_readiness+repo_task_scout+language_task_alignment",
                "readiness_status": "READY_FOR_MEDIUM_HIGH_WORK",
                "medium_high_work_ready": true,
                "source_host": "github",
                "origin_url_present": true,
                "origin_url_recorded": false,
                "provider_calls_allowed": false,
                "network_call_allowed": false,
                "production_write_allowed": false,
                "grants_execution_authority": false
            },
            "repo_intelligence": {
                "principal_orientation_gate": {
                    "required": true,
                    "observed": true
                },
                "related_tests_observed": true,
                "semantic_query_operations_observed": ["orientation", "related_tests"],
                "candidate_selection": {
                    "status": "recorded",
                    "selected_run_id": "forge_run_1",
                    "current_run_is_selected": true,
                    "matches_recommendation": true,
                    "override_reason_recorded": false
                }
            },
            "candidate_comparison": {
                "generated_from": "forge.run.event.parallel_candidates",
                "current_run_id": "forge_run_1",
                "recommended_run_id": "forge_run_1",
                "candidate_count": 1,
                "current_rank": 1,
                "diff_quality_included": true,
                "model_judgment_included": false,
                "grants_execution_authority": false
            }
        });

        let guardrails = delivery_guardrails(&packet, "attention");

        assert_eq!(guardrails["behavior_proof_required_for_delivery"], true);
        assert_eq!(guardrails["behavior_proof_status"], "weak");
        assert_eq!(
            delivery_guardrail_blockers(&guardrails),
            vec!["behavior_proof_not_covered"]
        );
    }

    #[test]
    fn delivery_guardrail_blockers_require_candidate_selection_for_parallel_delivery() {
        let guardrails = serde_json::json!({
            "repo_intake_present": true,
            "repo_intake_authority_boundary_clear": true,
            "proof_coverage_status": "satisfied",
            "principal_orientation_gate_required": true,
            "successful_semantic_orientation_observed": true,
            "related_tests_required_for_delivery": true,
            "successful_related_tests_observed": true,
            "dependency_map_required_for_delivery": true,
            "successful_dependency_map_observed": true,
            "candidate_comparison_required_for_delivery": true,
            "candidate_selection_required_for_delivery": true,
            "candidate_selection_recorded": false,
            "candidate_selection_satisfies_delivery": false
        });

        assert_eq!(
            delivery_guardrail_blockers(&guardrails),
            vec!["candidate_selection_missing"]
        );
        assert!(!delivery_guardrails_satisfied(&guardrails));
    }

    #[test]
    fn delivery_guardrail_blockers_allow_selected_candidate_override_with_reason() {
        let guardrails = serde_json::json!({
            "repo_intake_present": true,
            "repo_intake_authority_boundary_clear": true,
            "proof_coverage_status": "satisfied",
            "behavior_proof_required_for_delivery": false,
            "principal_orientation_gate_required": false,
            "related_tests_required_for_delivery": false,
            "dependency_map_required_for_delivery": false,
            "candidate_strategy_semantic_operations_required_for_delivery": false,
            "fleet_stream_strategy_semantic_operations_required_for_delivery": false,
            "fleet_integration_strategy_semantic_operations_required_for_delivery": false,
            "fleet_selection_required_for_delivery": false,
            "candidate_selection_required_for_delivery": true,
            "candidate_selection_recorded": true,
            "candidate_selection_current_run_is_selected": true,
            "candidate_selection_matches_recommendation": false,
            "candidate_selection_override_reason_recorded": true,
            "candidate_selection_satisfies_delivery": true
        });

        assert!(delivery_guardrail_blockers(&guardrails).is_empty());
        assert!(delivery_guardrails_satisfied(&guardrails));
    }

    #[test]
    fn delivery_guardrail_blockers_name_selected_non_recommended_candidate() {
        let guardrails = serde_json::json!({
            "repo_intake_present": true,
            "repo_intake_authority_boundary_clear": true,
            "proof_coverage_status": "satisfied",
            "behavior_proof_required_for_delivery": false,
            "principal_orientation_gate_required": false,
            "related_tests_required_for_delivery": false,
            "dependency_map_required_for_delivery": false,
            "candidate_strategy_semantic_operations_required_for_delivery": false,
            "fleet_stream_strategy_semantic_operations_required_for_delivery": false,
            "fleet_integration_strategy_semantic_operations_required_for_delivery": false,
            "fleet_selection_required_for_delivery": false,
            "candidate_selection_required_for_delivery": true,
            "candidate_selection_recorded": true,
            "candidate_selection_current_run_is_selected": true,
            "candidate_current_run_is_recommended": false,
            "candidate_selection_matches_recommendation": false,
            "candidate_selection_override_reason_recorded": false,
            "candidate_selection_satisfies_delivery": false
        });

        assert_eq!(
            delivery_guardrail_blockers(&guardrails),
            vec!["candidate_not_recommended"]
        );
        assert!(!delivery_guardrails_satisfied(&guardrails));
    }

    #[test]
    fn delivery_guardrail_blockers_hold_non_recommended_fleet_stream() {
        let guardrails = serde_json::json!({
            "repo_intake_present": true,
            "repo_intake_authority_boundary_clear": true,
            "proof_coverage_status": "satisfied",
            "behavior_proof_required_for_delivery": false,
            "principal_orientation_gate_required": false,
            "related_tests_required_for_delivery": false,
            "dependency_map_required_for_delivery": false,
            "candidate_strategy_semantic_operations_required_for_delivery": false,
            "fleet_stream_strategy_semantic_operations_required_for_delivery": false,
            "fleet_integration_strategy_semantic_operations_required_for_delivery": false,
            "fleet_selection_required_for_delivery": true,
            "fleet_selection_current_run_is_recommended": false,
            "candidate_comparison_required_for_delivery": false
        });

        assert_eq!(
            delivery_guardrail_blockers(&guardrails),
            vec!["fleet_stream_not_recommended"]
        );
        assert!(!delivery_guardrails_satisfied(&guardrails));
    }

    #[test]
    fn delivery_guardrails_require_dependency_map_for_parallel_candidates() {
        let packet = serde_json::json!({
            "proof_coverage": {
                "status": "satisfied",
                "match_policy": "exact_selected_command"
            },
            "diff": {
                "real_change_count": 1
            },
            "repo_intake": {
                "generated_from": "repo_readiness+repo_task_scout+language_task_alignment",
                "readiness_status": "READY_FOR_MEDIUM_HIGH_WORK",
                "medium_high_work_ready": true,
                "source_host": "github",
                "origin_url_present": true,
                "origin_url_recorded": false,
                "provider_calls_allowed": false,
                "network_call_allowed": false,
                "production_write_allowed": false,
                "grants_execution_authority": false
            },
            "repo_intelligence": {
                "principal_orientation_gate": {
                    "required": true,
                    "observed": true
                },
                "related_tests_observed": true,
                "semantic_query_operations_observed": ["orientation", "related_tests"],
                "candidate_selection": {
                    "status": "recorded",
                    "selected_run_id": "forge_run_1",
                    "current_run_is_selected": true,
                    "matches_recommendation": true,
                    "override_reason_recorded": false
                }
            },
            "candidate_comparison": {
                "generated_from": "forge.run.event.parallel_candidates",
                "current_run_id": "forge_run_1",
                "recommended_run_id": "forge_run_1",
                "candidate_count": 4,
                "current_rank": 1,
                "diff_quality_included": true,
                "model_judgment_included": false,
                "grants_execution_authority": false
            }
        });
        let guardrails = delivery_guardrails(&packet, "ready");

        assert_eq!(guardrails["dependency_map_required_for_delivery"], true);
        assert_eq!(guardrails["successful_dependency_map_observed"], false);
        assert_eq!(
            delivery_guardrail_blockers(&guardrails),
            vec!["dependency_map_missing"]
        );

        let mut packet_with_dependency_map = packet;
        packet_with_dependency_map["repo_intelligence"]["semantic_query_operations_observed"] =
            serde_json::json!(["orientation", "dependency_map", "related_tests"]);
        let satisfied = delivery_guardrails(&packet_with_dependency_map, "ready");
        assert_eq!(satisfied["successful_dependency_map_observed"], true);
        assert!(delivery_guardrail_blockers(&satisfied).is_empty());
    }

    #[test]
    fn delivery_guardrails_require_parallel_candidate_strategy_semantics() {
        let packet = serde_json::json!({
            "proof_coverage": {
                "status": "satisfied",
                "match_policy": "exact_selected_command"
            },
            "proof_scope": {
                "status": "covered"
            },
            "behavior_proof": {
                "status": "covered"
            },
            "diff": {
                "real_change_count": 1
            },
            "repo_intake": {
                "generated_from": "repo_readiness+repo_task_scout+language_task_alignment",
                "readiness_status": "READY_FOR_MEDIUM_HIGH_WORK",
                "medium_high_work_ready": true,
                "source_host": "github",
                "origin_url_present": true,
                "origin_url_recorded": false,
                "provider_calls_allowed": false,
                "network_call_allowed": false,
                "production_write_allowed": false,
                "grants_execution_authority": false
            },
            "repo_intelligence": {
                "principal_orientation_gate": {
                    "required": true,
                    "observed": true
                },
                "related_tests_observed": true,
                "semantic_query_operations_observed": ["related_tests", "dependency_map"],
                "candidate_selection": {
                    "status": "recorded",
                    "selected_run_id": "forge_run_1",
                    "current_run_is_selected": true,
                    "matches_recommendation": true,
                    "override_reason_recorded": false
                },
                "parallel_candidate": {
                    "required_semantic_operations": ["related_tests", "references", "dependency_map", "diagnostics"]
                }
            },
            "candidate_comparison": {
                "generated_from": "forge.run.event.parallel_candidates",
                "current_run_id": "forge_run_1",
                "recommended_run_id": "forge_run_1",
                "candidate_count": 4,
                "current_rank": 1,
                "diff_quality_included": true,
                "model_judgment_included": false,
                "grants_execution_authority": false
            }
        });
        let guardrails = delivery_guardrails(&packet, "ready");

        assert_eq!(
            guardrails["candidate_strategy_semantic_operations_required_for_delivery"],
            true
        );
        assert_eq!(
            guardrails["candidate_strategy_semantic_operations_satisfied"],
            false
        );
        assert_eq!(
            guardrails["missing_candidate_strategy_semantic_operations"][0],
            "references"
        );
        assert_eq!(
            delivery_guardrail_blockers(&guardrails),
            vec!["candidate_strategy_semantic_operations_missing"]
        );

        let mut satisfied_packet = packet;
        satisfied_packet["repo_intelligence"]["semantic_query_operations_observed"] =
            serde_json::json!([
                "related_tests",
                "references",
                "dependency_map",
                "diagnostics"
            ]);
        let satisfied = delivery_guardrails(&satisfied_packet, "ready");
        assert_eq!(
            satisfied["candidate_strategy_semantic_operations_satisfied"],
            true
        );
        assert!(delivery_guardrail_blockers(&satisfied).is_empty());
    }

    #[test]
    fn delivery_guardrails_require_fleet_stream_strategy_semantics() {
        let packet = serde_json::json!({
            "proof_coverage": {
                "status": "satisfied",
                "match_policy": "exact_selected_command"
            },
            "proof_scope": {
                "status": "covered"
            },
            "behavior_proof": {
                "status": "covered"
            },
            "diff": {
                "real_change_count": 1
            },
            "repo_intake": {
                "generated_from": "repo_readiness+repo_task_scout+language_task_alignment",
                "readiness_status": "READY_FOR_MEDIUM_HIGH_WORK",
                "medium_high_work_ready": true,
                "source_host": "github",
                "origin_url_present": true,
                "origin_url_recorded": false,
                "provider_calls_allowed": false,
                "network_call_allowed": false,
                "production_write_allowed": false,
                "grants_execution_authority": false
            },
            "repo_intelligence": {
                "principal_orientation_gate": {
                    "required": true,
                    "observed": true
                },
                "related_tests_observed": true,
                "semantic_query_operations_observed": ["file_outline", "definition", "related_tests"],
                "parallel_candidate": {
                    "required_semantic_operations": []
                },
                "fleet_stream": {
                    "role": "stream",
                    "stream_id": "s1_service",
                    "required_semantic_operations": ["file_outline", "definition", "references", "diagnostics"]
                }
            },
            "candidate_comparison": {
                "generated_from": "single_run",
                "current_run_id": "forge_run_1",
                "recommended_run_id": "forge_run_1",
                "candidate_count": 1,
                "current_rank": 1,
                "diff_quality_included": false,
                "model_judgment_included": false,
                "grants_execution_authority": false
            }
        });
        let guardrails = delivery_guardrails(&packet, "ready");

        assert_eq!(
            guardrails["fleet_stream_strategy_semantic_operations_required_for_delivery"],
            true
        );
        assert_eq!(
            guardrails["fleet_stream_strategy_semantic_operations_satisfied"],
            false
        );
        assert_eq!(
            guardrails["missing_fleet_stream_strategy_semantic_operations"][0],
            "references"
        );
        assert_eq!(
            delivery_guardrail_blockers(&guardrails),
            vec!["fleet_stream_strategy_semantic_operations_missing"]
        );

        let mut satisfied_packet = packet;
        satisfied_packet["repo_intelligence"]["semantic_query_operations_observed"] =
            serde_json::json!(["file_outline", "definition", "references", "diagnostics"]);
        let satisfied = delivery_guardrails(&satisfied_packet, "ready");
        assert_eq!(
            satisfied["fleet_stream_strategy_semantic_operations_satisfied"],
            true
        );
        assert!(delivery_guardrail_blockers(&satisfied).is_empty());
    }

    #[test]
    fn delivery_guardrails_require_fleet_integration_strategy_semantics() {
        let packet = serde_json::json!({
            "proof_coverage": {
                "status": "satisfied",
                "match_policy": "exact_selected_command"
            },
            "proof_scope": {
                "status": "covered"
            },
            "behavior_proof": {
                "status": "covered"
            },
            "diff": {
                "real_change_count": 1
            },
            "repo_intake": {
                "generated_from": "repo_readiness+repo_task_scout+language_task_alignment",
                "readiness_status": "READY_FOR_MEDIUM_HIGH_WORK",
                "medium_high_work_ready": true,
                "source_host": "github",
                "origin_url_present": true,
                "origin_url_recorded": false,
                "provider_calls_allowed": false,
                "network_call_allowed": false,
                "production_write_allowed": false,
                "grants_execution_authority": false
            },
            "repo_intelligence": {
                "principal_orientation_gate": {
                    "required": true,
                    "observed": true
                },
                "related_tests_observed": true,
                "semantic_query_operations_observed": ["file_outline", "dependency_map", "related_tests"],
                "parallel_candidate": {
                    "required_semantic_operations": []
                },
                "fleet_stream": {
                    "role": "not_fleet_stream",
                    "required_semantic_operations": []
                },
                "fleet_integration": {
                    "role": "integration",
                    "strategy_id": "integration_contract_wiring",
                    "required_semantic_operations": ["file_outline", "dependency_map", "references", "diagnostics"]
                }
            },
            "candidate_comparison": {
                "generated_from": "single_run",
                "current_run_id": "forge_run_1",
                "recommended_run_id": "forge_run_1",
                "candidate_count": 1,
                "current_rank": 1,
                "diff_quality_included": false,
                "model_judgment_included": false,
                "grants_execution_authority": false
            }
        });
        let guardrails = delivery_guardrails(&packet, "ready");

        assert_eq!(
            guardrails["fleet_integration_strategy_semantic_operations_required_for_delivery"],
            true
        );
        assert_eq!(
            guardrails["fleet_integration_strategy_semantic_operations_satisfied"],
            false
        );
        assert_eq!(
            guardrails["missing_fleet_integration_strategy_semantic_operations"][0],
            "references"
        );
        assert_eq!(
            delivery_guardrail_blockers(&guardrails),
            vec!["fleet_integration_strategy_semantic_operations_missing"]
        );

        let mut satisfied_packet = packet;
        satisfied_packet["repo_intelligence"]["semantic_query_operations_observed"] =
            serde_json::json!([
                "file_outline",
                "dependency_map",
                "references",
                "diagnostics"
            ]);
        let satisfied = delivery_guardrails(&satisfied_packet, "ready");
        assert_eq!(
            satisfied["fleet_integration_strategy_semantic_operations_satisfied"],
            true
        );
        assert!(delivery_guardrail_blockers(&satisfied).is_empty());
    }

    #[test]
    fn delivery_guardrail_blockers_fail_closed_on_missing_or_breached_intake() {
        let missing = serde_json::json!({
            "repo_intake_present": false,
            "repo_intake_authority_boundary_clear": true,
            "proof_coverage_status": "satisfied",
            "principal_orientation_gate_required": false,
            "successful_semantic_orientation_observed": false,
            "related_tests_required_for_delivery": false,
            "successful_related_tests_observed": false
        });
        assert_eq!(
            delivery_guardrail_blockers(&missing),
            vec!["repo_intake_missing"]
        );

        let breached = serde_json::json!({
            "repo_intake_present": true,
            "repo_intake_authority_boundary_clear": false,
            "proof_coverage_status": "satisfied",
            "principal_orientation_gate_required": false,
            "successful_semantic_orientation_observed": false,
            "related_tests_required_for_delivery": false,
            "successful_related_tests_observed": false
        });
        assert_eq!(
            delivery_guardrail_blockers(&breached),
            vec!["repo_intake_authority_boundary_breached"]
        );
    }
}
