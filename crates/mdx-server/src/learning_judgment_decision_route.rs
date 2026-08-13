// Learn judgment decisions record accountability and queue memory review.
// They do not write memory, apply adaptation, or change runtime behavior.
use crate::RouteResponse;
use mdx_core::{LearningJudgmentDecision, MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/learning/judgment-decisions.json" => Some(handle_post(method, body, kernel)),
        "/learning/judgment-decisions/projection.json" => Some(handle_projection(method, kernel)),
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
    let evidence_refs =
        json_string_or_string_array_field(body, "evidence_refs").unwrap_or_default();
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.record_learning_judgment_decision_with_identity(
        LearningJudgmentDecision {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            judgment_id: &field("judgment_id"),
            promotion_id: &field("promotion_id"),
            decision: &field("decision"),
            rationale: &field("rationale"),
            evidence_refs: &evidence_refs,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-judgment-decision-local-post","status":"RECORDED","auth_session_status":{},"judgment_decision_id":{},"judgment_id":{},"promotion_id":{},"decision":{},"memory_queue_state":{},"judgment_decision_receipt_id":{},"policy_decision_id":{},"projection_route":"/learning/judgment-decisions/projection.json","memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.judgment_decision_id),
            json_string_literal(&report.judgment_id),
            json_string_literal(&report.promotion_id),
            json_string_literal(&report.decision),
            json_string_literal(&report.memory_queue_state),
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
    let decisions: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("learning.judgment.decided")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"judgment_decision_receipt_id":{},"judgment_decision_id":{},"judgment_id":{},"promotion_id":{},"decision":{},"rationale":{},"evidence_refs":{},"actor_id":{},"memory_queue_state":{},"memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false}}"#,
                json_string_literal(&receipt.receipt_id),
                value("judgment_decision_id"),
                value("judgment_id"),
                value("promotion_id"),
                value("decision"),
                value("rationale"),
                value("evidence_refs"),
                json_string_literal(receipt.actor_id.as_str()),
                value("memory_queue_state"),
            )
        })
        .collect();
    let ready: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("learning.judgment.decided")
        .iter()
        .filter(|receipt| {
            receipt.payload.get("memory_queue_state").map(String::as_str)
                == Some("ready_for_memory")
        })
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"judgment_decision_receipt_id":{},"judgment_id":{},"promotion_id":{},"decision":{},"memory_queue_state":{},"memory_write_allowed":false,"adaptation_allowed":false}}"#,
                json_string_literal(&receipt.receipt_id),
                value("judgment_id"),
                value("promotion_id"),
                value("decision"),
                value("memory_queue_state"),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-judgment-decision-local-projection","receipt_kind":"learning.judgment.decided","writes_route":"/learning/judgment-decisions.json","judgment_decision_count":{},"ready_for_memory_count":{},"decisions":[{}],"ready_for_memory":[{}],"memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            decisions.len(),
            ready.len(),
            decisions.join(","),
            ready.join(","),
        ),
    ))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-judgment-decision-local-post","status":"REFUSED","reason":{},"judgment_decision_receipt_id":"","memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
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

fn json_string_or_string_array_field(body: &str, key: &str) -> Option<String> {
    if let Some(value) = json_string_field(body, key) {
        return Some(value);
    }
    let marker = format!("\"{key}\":");
    let after = body.split(&marker).nth(1)?;
    let after = after.trim_start();
    let rest = after.strip_prefix('[')?;
    let mut values = Vec::new();
    let mut chars = rest.chars().peekable();
    loop {
        while matches!(chars.peek(), Some(c) if c.is_whitespace() || *c == ',') {
            chars.next();
        }
        match chars.peek() {
            Some(']') => return Some(values.join(", ")),
            Some('"') => {
                chars.next();
                let mut value = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(escaped) = chars.next() {
                                value.push(escaped);
                            }
                        }
                        '"' => {
                            values.push(value);
                            break;
                        }
                        other => value.push(other),
                    }
                }
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_records_judgment_decision_and_memory_queue_projection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let post = route_response(
            "POST",
            "/learning/judgment-decisions.json",
            r#"{"judgment_id":"judgment_1","promotion_id":"promotion_1","decision":"promote_candidate","rationale":"Evidence is enough for memory review.","evidence_refs":"make learning-loop-check"}"#,
            &kernel,
        )
        .expect("learning route")
        .expect("learning response");
        assert_eq!(post.status, "200 OK");
        assert!(post.body.contains("\"status\":\"RECORDED\""));
        assert!(
            post.body
                .contains("\"memory_queue_state\":\"ready_for_memory\"")
        );
        assert!(post.body.contains("\"memory_write_allowed\":false"));
        assert!(post.body.contains("\"adaptation_allowed\":false"));

        let projection = route_response(
            "GET",
            "/learning/judgment-decisions/projection.json",
            "",
            &kernel,
        )
        .expect("learning projection route")
        .expect("learning projection response");
        assert_eq!(projection.status, "200 OK");
        assert!(projection.body.contains("\"judgment_decision_count\":1"));
        assert!(projection.body.contains("\"ready_for_memory_count\":1"));
        assert!(
            projection
                .body
                .contains("\"receipt_kind\":\"learning.judgment.decided\"")
        );
    }

    #[test]
    fn post_accepts_evidence_refs_as_array() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let post = route_response(
            "POST",
            "/learning/judgment-decisions.json",
            r#"{"judgment_id":"judgment_1","promotion_id":"promotion_1","decision":"promote_candidate","rationale":"Evidence is enough for memory review.","evidence_refs":["make learning-loop-check","receipt:abc"]}"#,
            &kernel,
        )
        .expect("learning route")
        .expect("learning response");
        assert_eq!(post.status, "200 OK");
        assert!(post.body.contains("\"status\":\"RECORDED\""));

        let projection = route_response(
            "GET",
            "/learning/judgment-decisions/projection.json",
            "",
            &kernel,
        )
        .expect("learning projection route")
        .expect("learning projection response");
        assert!(
            projection
                .body
                .contains("\"evidence_refs\":\"make learning-loop-check, receipt:abc\"")
        );
    }
}
