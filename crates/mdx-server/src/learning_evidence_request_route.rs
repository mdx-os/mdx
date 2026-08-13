// Learn's first governed write: ask for stronger evidence on a lesson
// judgment. This records a receipt and keeps promotion/adaptation closed.
use crate::RouteResponse;
use mdx_core::{LearningEvidenceRequest, MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/learning/evidence-requests.json" => Some(handle_post(method, body, kernel)),
        "/learning/evidence-requests/projection.json" => Some(handle_projection(method, kernel)),
        _ => None,
    }
}

fn handle_post(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let field = |name: &str| json_string_field(body, name).unwrap_or_default();
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.record_learning_evidence_request_with_identity(
        LearningEvidenceRequest {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            judgment_id: &field("judgment_id"),
            promotion_id: &field("promotion_id"),
            reason: &field("reason"),
            requested_evidence: &field("requested_evidence"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-evidence-request-local-post","status":"RECORDED","auth_session_status":{},"evidence_request_id":{},"judgment_id":{},"promotion_id":{},"evidence_request_receipt_id":{},"policy_decision_id":{},"projection_route":"/learning/evidence-requests/projection.json","memory_promotion_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.evidence_request_id),
            json_string_literal(&report.judgment_id),
            json_string_literal(&report.promotion_id),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
        ),
    ))
}

fn handle_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let requests: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("learning.evidence.requested")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"evidence_request_receipt_id":{},"evidence_request_id":{},"judgment_id":{},"promotion_id":{},"reason":{},"requested_evidence":{},"actor_id":{},"memory_promotion_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false}}"#,
                json_string_literal(&receipt.receipt_id),
                value("evidence_request_id"),
                value("judgment_id"),
                value("promotion_id"),
                value("reason"),
                value("requested_evidence"),
                json_string_literal(receipt.actor_id.as_str()),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-evidence-request-local-projection","receipt_kind":"learning.evidence.requested","writes_route":"/learning/evidence-requests.json","evidence_request_count":{},"requests":[{}],"memory_promotion_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            requests.len(),
            requests.join(","),
        ),
    ))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-evidence-request-local-post","status":"REFUSED","reason":{},"evidence_request_receipt_id":"","memory_promotion_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
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
    fn post_records_evidence_request_and_projection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let post = route_response(
            "POST",
            "/learning/evidence-requests.json",
            r#"{"judgment_id":"judgment_1","promotion_id":"promotion_1","reason":"Need stronger proof.","requested_evidence":"Attach local smoke."}"#,
            &kernel,
        )
        .expect("learning route")
        .expect("learning response");
        assert_eq!(post.status, "200 OK");
        assert!(post.body.contains("\"status\":\"RECORDED\""));
        assert!(post.body.contains("\"memory_promotion_allowed\":false"));
        assert!(post.body.contains("\"adaptation_allowed\":false"));

        let projection = route_response(
            "GET",
            "/learning/evidence-requests/projection.json",
            "",
            &kernel,
        )
        .expect("learning projection route")
        .expect("learning projection response");
        assert_eq!(projection.status, "200 OK");
        assert!(projection.body.contains("\"evidence_request_count\":1"));
        assert!(
            projection
                .body
                .contains("\"receipt_kind\":\"learning.evidence.requested\"")
        );
    }
}
