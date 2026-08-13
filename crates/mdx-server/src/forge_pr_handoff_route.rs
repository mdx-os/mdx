// A narrow, PR-native handoff generated from the Forge Review Packet. This is
// intentionally a dry-run integration edge: it gives GitHub/Bitbucket-ready
// title/body/checklist text and records a receipt for the handoff artifact, but
// it never pushes a branch, opens a PR, approves work, or writes to production.
use crate::RouteResponse;
use mdx_core::{ForgePrHandoff, MdxKernel, hex, json_string_literal, sha256};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/pr-handoffs.json" {
        return None;
    }
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Some(Ok(response));
    }
    let run_id = json_string_field(body, "run_id").unwrap_or_default();
    if run_id.trim().is_empty() {
        return Some(Ok(refusal("name the run to prepare a PR handoff for")));
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
    let target_host =
        json_string_field(body, "target_host").unwrap_or_else(|| "generic".to_string());
    let target_host = if target_host.trim().is_empty() {
        "generic".to_string()
    } else {
        target_host.trim().to_string()
    };
    let branch = packet["branch"].as_str().unwrap_or("");
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
    let delivery_guardrails =
        crate::forge_source_host_pr_draft_route::delivery_guardrails(&packet, review_status);
    let delivery_blocked_reasons =
        crate::forge_source_host_pr_draft_route::delivery_guardrail_blockers(&delivery_guardrails);
    let delivery_guardrail_status = if delivery_blocked_reasons.is_empty() {
        "SATISFIED"
    } else {
        "BLOCKED"
    };
    let delivery_blocked_reasons_text = delivery_blocked_reasons.join(",");

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
                branch,
                review_status,
                title,
                body_sha256: &body_sha256,
                body_char_count: body_markdown.chars().count(),
                summary_line_count,
                review_checklist_count,
                review_packet_route: "/forge/review-packet.json",
                target_host: &target_host,
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
            "name": "mdx-forge-pr-handoff",
            "status": "RECORDED",
            "auth_session_status": resolved.auth_session_status,
            "run_id": run_id,
            "branch": branch,
            "review_status": review_status,
            "target_host": target_host,
            "generated_from": "forge_review_packet.pr_handoff",
            "pr_handoff_id": report.pr_handoff_id,
            "pr_handoff_receipt_id": report.receipt_id,
            "policy_decision_id": report.policy_decision_id,
            "body_sha256": body_sha256,
            "body_char_count": body_markdown.chars().count(),
            "summary_line_count": summary_line_count,
            "review_checklist_count": review_checklist_count,
            "title": title,
            "body_markdown": body_markdown,
            "summary_lines": handoff["summary_lines"].clone(),
            "review_checklist": handoff["review_checklist"].clone(),
            "pr_open_authority": crate::forge_source_host_pr_draft_route::pr_open_authority_contract(&target_host),
            "delivery_guardrails": delivery_guardrails,
            "delivery_guardrail_status": delivery_guardrail_status,
            "ready_for_live_delivery_guardrails": delivery_blocked_reasons.is_empty(),
            "delivery_blocked_reasons": delivery_blocked_reasons,
            "review_packet_route": "/forge/review-packet.json",
            "dry_run": true,
            "remote_push_allowed": false,
            "pull_request_open_allowed": false,
            "approval_allowed": false,
            "deployment_authority_granted": false,
            "production_write_allowed": false,
        })
        .to_string(),
    ))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-pr-handoff","status":"REFUSED","reason":{},"pr_handoff_receipt_id":"","remote_push_allowed":false,"pull_request_open_allowed":false,"approval_allowed":false,"deployment_authority_granted":false,"production_write_allowed":false}}"#,
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
    fn refuses_unknown_run_without_remote_authority() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = handle(
            "forge_run_missing",
            r#"{"run_id":"forge_run_missing","target_host":"github"}"#,
            &kernel,
        )
        .expect("response");
        assert!(response.body.contains(r#""status":"REFUSED""#));
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
}
