use crate::RouteResponse;
use mdx_core::{
    GovernedWriteIdentity, MdxKernel, PagesApprovalDecision, PagesApprovalRequest,
    json_string_literal,
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/pages/approval-requests.json" {
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
            render_local_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/pages/approval-decisions/approve.json" {
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
            render_local_decision_post_json(
                body,
                "approved",
                "/pages/approval-decisions/approve.json",
                &mut kernel,
            )
            .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/pages/approval-decisions/reject.json" {
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
            render_local_decision_post_json(
                body,
                "rejected",
                "/pages/approval-decisions/reject.json",
                &mut kernel,
            )
            .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/pages/approval-decisions/projection.json" {
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
            render_local_decision_projection_json(&kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/pages/approval-requests/projection.json" {
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
            render_local_projection_json(&kernel).map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    None
}

fn render_local_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let approval_request_id = json_string_field(body, "approval_request_id")
        .unwrap_or_else(|| "pages_approval_request_local_001".to_string());
    let source_edit_draft_receipt_id =
        json_string_field(body, "source_edit_draft_receipt_id").unwrap_or_default();
    let document_id = json_string_field(body, "document_id")
        .unwrap_or_else(|| "page_local_operator_note".to_string());
    let draft_id = json_string_field(body, "draft_id")
        .unwrap_or_else(|| "draft_for_approval_request".to_string());
    let requested_visibility = json_string_field(body, "requested_visibility")
        .unwrap_or_else(|| "tenant_review".to_string());
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let verified = resolved.auth_session_status == "VERIFIED_TRUSTED_SESSION";
    let tenant_id = if verified {
        resolved.tenant_id.clone()
    } else {
        json_string_field(body, "tenant_id").unwrap_or_else(|| "local_tenant".to_string())
    };
    let actor_id = if verified {
        resolved.actor_id.clone()
    } else {
        json_string_field(body, "actor_id").unwrap_or_else(|| "human:local_user".to_string())
    };
    let identity = if verified {
        resolved.identity.clone()
    } else {
        GovernedWriteIdentity::local_demo(&actor_id)
    };
    let report = match kernel.save_pages_approval_request_local_with_identity(
        PagesApprovalRequest {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            approval_request_id: &approval_request_id,
            source_edit_draft_receipt_id: &source_edit_draft_receipt_id,
            document_id: &document_id,
            draft_id: &draft_id,
            requested_visibility: &requested_visibility,
        },
        &identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(format!(
                r#"{{"name":"mdx-pages-approval-request-local-post","status":"REFUSED","reason":{},"document_id":{},"draft_id":{},"source_edit_draft_receipt_id":{},"production_write_allowed":false}}"#,
                json_string_literal(&error.message()),
                json_string_literal(&document_id),
                json_string_literal(&draft_id),
                json_string_literal(&source_edit_draft_receipt_id),
            ));
        }
    };
    Ok(format!(
        r#"{{"name":"mdx-pages-approval-request-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/pages/approval-requests.json","projection_route":"/pages/approval-requests/projection.json","approval_request_id":{},"approval_request_receipt_id":{},"policy_decision_id":{},"source_edit_draft_receipt_id":{},"document_id":{},"draft_id":{},"public_visibility_allowed":{},"search_indexing_allowed":{},"embedding_provider_allowed":{},"rich_editor_allowed":{},"standalone_store_allowed":{},"live_substrate_required":false,"production_publish_allowed":{},"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.approval_request_id),
        json_string_literal(&report.approval_request_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.source_edit_draft_receipt_id),
        json_string_literal(&report.document_id),
        json_string_literal(&report.draft_id),
        report.public_visibility_allowed,
        report.search_indexing_allowed,
        report.embedding_provider_allowed,
        report.rich_editor_allowed,
        report.standalone_store_allowed,
        report.production_publish_allowed,
        report.production_write_allowed
    ))
}

fn render_local_decision_post_json(
    body: &str,
    decision_outcome: &str,
    source_route: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    let approval_request_receipt_id = match approval_request_receipt_id(body) {
        Ok(receipt_id) => receipt_id,
        Err(error) => {
            return Ok(format!(
                r#"{{"name":"mdx-pages-approval-decision-local-post","status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
                json_string_literal(&error),
            ));
        }
    };
    let approval_decision_id =
        json_string_field(body, "approval_decision_id").unwrap_or_else(|| {
            if decision_outcome == "approved" {
                "pages_approval_decision_approve_local_001".to_string()
            } else {
                "pages_approval_decision_reject_local_001".to_string()
            }
        });
    let decision_note = json_string_field(body, "decision_note").unwrap_or_else(|| {
        if decision_outcome == "approved" {
            "Local human approved the draft for the next governed Pages step.".to_string()
        } else {
            "Local human rejected the draft before publication authority opened.".to_string()
        }
    });
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let verified = resolved.auth_session_status == "VERIFIED_TRUSTED_SESSION";
    let tenant_id = if verified {
        resolved.tenant_id.clone()
    } else {
        json_string_field(body, "tenant_id").unwrap_or_else(|| "local_tenant".to_string())
    };
    let actor_id = if verified {
        resolved.actor_id.clone()
    } else {
        json_string_field(body, "actor_id").unwrap_or_else(|| "human:local_user".to_string())
    };
    let identity = if verified {
        resolved.identity.clone()
    } else {
        GovernedWriteIdentity::local_demo(&actor_id)
    };
    let report = match kernel.save_pages_approval_decision_local_with_identity(
        PagesApprovalDecision {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            approval_decision_id: &approval_decision_id,
            approval_request_receipt_id: &approval_request_receipt_id,
            decision_outcome,
            decision_note: &decision_note,
            source_route,
        },
        &identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(format!(
                r#"{{"name":"mdx-pages-approval-decision-local-post","status":"REFUSED","reason":{},"approval_request_receipt_id":{},"decision_outcome":{},"production_write_allowed":false}}"#,
                json_string_literal(&error.message()),
                json_string_literal(&approval_request_receipt_id),
                json_string_literal(decision_outcome),
            ));
        }
    };
    Ok(format!(
        r#"{{"name":"mdx-pages-approval-decision-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/pages/approval-decisions.json","approve_route":"/pages/approval-decisions/approve.json","reject_route":"/pages/approval-decisions/reject.json","projection_route":"/pages/approval-decisions/projection.json","approval_decision_id":{},"approval_decision_receipt_id":{},"policy_decision_id":{},"approval_request_receipt_id":{},"approval_request_id":{},"document_id":{},"draft_id":{},"decision_outcome":{},"decision_note":{},"created_at":{},"human_decision_recorded":{},"human_approval_granted":{},"reviewer_independence":{},"self_review_permitted":{},"public_visibility_allowed":{},"search_indexing_allowed":{},"embedding_provider_allowed":{},"rich_editor_allowed":{},"standalone_store_allowed":{},"live_substrate_required":false,"production_publish_allowed":{},"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.approval_decision_id),
        json_string_literal(&report.approval_decision_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.approval_request_receipt_id),
        json_string_literal(&report.approval_request_id),
        json_string_literal(&report.document_id),
        json_string_literal(&report.draft_id),
        json_string_literal(&report.decision_outcome),
        json_string_literal(&report.decision_note),
        json_string_literal(&report.created_at),
        report.human_decision_recorded,
        report.human_approval_granted,
        json_string_literal(report.reviewer_independence),
        report.self_review_permitted,
        report.public_visibility_allowed,
        report.search_indexing_allowed,
        report.embedding_provider_allowed,
        report.rich_editor_allowed,
        report.standalone_store_allowed,
        report.production_publish_allowed,
        report.production_write_allowed
    ))
}

fn render_local_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let requests = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "pages.approval.requested")
        .map(|receipt| {
            let decision = latest_decision_for_request(kernel, &receipt.receipt_id);
            format!(
                r#"{{"approval_request_id":{},"approval_request_receipt_id":{},"policy_decision_id":{},"source_edit_draft_receipt_id":{},"document_id":{},"draft_id":{},"requested_visibility":{},"terminal_state":{},"created_at":{},"decision_state":{},"approval_decision_receipt_id":{},"decision_outcome":{},"decision_created_at":{},"human_decision_recorded":{},"human_approval_granted":{},"reviewer_independence":{},"self_review_permitted":{},"public_visibility_allowed":false,"search_indexing_allowed":false,"embedding_provider_allowed":false,"rich_editor_allowed":false,"standalone_store_allowed":false,"production_publish_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "approval_request_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "source_edit_draft_receipt_id")),
                json_string_literal(payload_value(receipt, "document_id")),
                json_string_literal(payload_value(receipt, "draft_id")),
                json_string_literal(payload_value(receipt, "requested_visibility")),
                json_string_literal(payload_value(receipt, "terminal_state")),
                json_string_literal(payload_value(receipt, "created_at")),
                json_string_literal(decision.map(decision_state).unwrap_or("PENDING_DECISION")),
                json_string_literal(decision.map(|receipt| receipt.receipt_id.as_str()).unwrap_or("")),
                json_string_literal(decision.map(|receipt| payload_value(receipt, "decision_outcome")).unwrap_or("")),
                json_string_literal(decision.map(|receipt| payload_value(receipt, "created_at")).unwrap_or("")),
                decision.is_some(),
                decision.map(decision_human_approval_granted).unwrap_or(false),
                json_string_literal(decision.map(|receipt| payload_value(receipt, "reviewer_independence")).unwrap_or("")),
                decision.map(|receipt| payload_value(receipt, "self_review_permitted") == "true").unwrap_or(true)
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-pages-approval-request-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/pages/approval-requests.json","approval_request_count":{},"public_visibility_allowed":false,"search_indexing_allowed":false,"embedding_provider_allowed":false,"rich_editor_allowed":false,"standalone_store_allowed":false,"live_substrate_required":false,"production_publish_allowed":false,"production_write_allowed":false,"requests":[{}]}}"#,
        requests.len(),
        requests.join(",")
    ))
}

fn render_local_decision_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let decisions = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "pages.approval.decision.recorded")
        .map(render_decision_object)
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-pages-approval-decision-local-projection","status":"OK","receipt_kind":"pages.approval.decision.recorded","auth_session_route":"/local/auth-session.json","writes_route":"/pages/approval-decisions.json","approve_route":"/pages/approval-decisions/approve.json","reject_route":"/pages/approval-decisions/reject.json","decision_count":{},"human_decision_recorded":{},"public_visibility_allowed":false,"search_indexing_allowed":false,"embedding_provider_allowed":false,"rich_editor_allowed":false,"standalone_store_allowed":false,"live_substrate_required":false,"production_publish_allowed":false,"production_write_allowed":false,"decisions":[{}]}}"#,
        decisions.len(),
        !decisions.is_empty(),
        decisions.join(",")
    ))
}

fn render_decision_object(receipt: &mdx_core::Receipt) -> String {
    format!(
        r#"{{"approval_decision_id":{},"approval_decision_receipt_id":{},"policy_decision_id":{},"approval_request_receipt_id":{},"approval_request_id":{},"document_id":{},"draft_id":{},"decision_outcome":{},"decision_note":{},"terminal_state":{},"created_at":{},"human_decision_recorded":true,"human_approval_granted":{},"reviewer_independence":{},"self_review_permitted":{},"public_visibility_allowed":false,"search_indexing_allowed":false,"embedding_provider_allowed":false,"rich_editor_allowed":false,"standalone_store_allowed":false,"production_publish_allowed":false,"production_write_allowed":false}}"#,
        json_string_literal(payload_value(receipt, "approval_decision_id")),
        json_string_literal(&receipt.receipt_id),
        json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
        json_string_literal(payload_value(receipt, "approval_request_receipt_id")),
        json_string_literal(payload_value(receipt, "approval_request_id")),
        json_string_literal(payload_value(receipt, "document_id")),
        json_string_literal(payload_value(receipt, "draft_id")),
        json_string_literal(payload_value(receipt, "decision_outcome")),
        json_string_literal(payload_value(receipt, "decision_note")),
        json_string_literal(payload_value(receipt, "terminal_state")),
        json_string_literal(payload_value(receipt, "created_at")),
        decision_human_approval_granted(receipt),
        json_string_literal(payload_value(receipt, "reviewer_independence")),
        payload_value(receipt, "self_review_permitted") == "true"
    )
}

fn latest_decision_for_request<'a>(
    kernel: &'a MdxKernel,
    approval_request_receipt_id: &str,
) -> Option<&'a mdx_core::Receipt> {
    kernel.ledger().entries().iter().rfind(|receipt| {
        receipt.kind == "pages.approval.decision.recorded"
            && payload_value(receipt, "approval_request_receipt_id") == approval_request_receipt_id
    })
}

fn approval_request_receipt_id(body: &str) -> Result<String, String> {
    if let Some(receipt_id) = json_string_field(body, "approval_request_receipt_id")
        .filter(|receipt_id| !receipt_id.trim().is_empty())
    {
        return Ok(receipt_id);
    }
    Err("pages approval decision missing approval_request_receipt_id".to_string())
}

fn decision_state(receipt: &mdx_core::Receipt) -> &'static str {
    match payload_value(receipt, "decision_outcome") {
        "approved" => "APPROVED_PUBLICATION_BLOCKED",
        "rejected" => "REJECTED_PUBLICATION_BLOCKED",
        _ => "DECISION_RECORDED_PUBLICATION_BLOCKED",
    }
}

fn decision_human_approval_granted(receipt: &mdx_core::Receipt) -> bool {
    payload_value(receipt, "human_approval_granted") == "true"
}

fn payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let after_key = body.split(&format!("\"{key}\"")).nth(1)?;
    let after_colon = after_key.split_once(':')?.1.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            out.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Some(out);
        } else {
            out.push(character);
        }
    }
    None
}
