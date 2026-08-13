// Learn memory promotion requests create memory candidates only.
// They do not write active memory, refresh generated artifacts, or adapt behavior.
use crate::RouteResponse;
use mdx_core::{
    LearningMemoryApplicability, LearningMemoryPromotion, MdxKernel, json_string_literal,
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/learning/memory-promotions.json" => Some(handle_post(method, body, kernel)),
        "/learning/memory-promotions/projection.json" => Some(handle_projection(method, kernel)),
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
    // Optional applicability scoping: empty fields mean the lesson applies
    // universally, which is also what every pre-existing caller sends.
    let applicability_work_tiers =
        json_string_or_string_array_field(body, "applicability_work_tiers").unwrap_or_default();
    let applicability_language_packs =
        json_string_or_string_array_field(body, "applicability_language_packs").unwrap_or_default();
    let applicability_notes = field("applicability_notes");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.request_learning_memory_promotion_with_applicability(
        LearningMemoryPromotion {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            judgment_decision_id: &field("judgment_decision_id"),
            judgment_decision_receipt_id: &field("judgment_decision_receipt_id"),
            judgment_id: &field("judgment_id"),
            promotion_id: &field("promotion_id"),
            target_type: &field("target_type"),
            target_path: &field("target_path"),
            lesson_summary: &field("lesson_summary"),
            evidence_refs: &evidence_refs,
            review_cadence: &field("review_cadence"),
            expiry_rule: &field("expiry_rule"),
        },
        LearningMemoryApplicability {
            work_tiers: &applicability_work_tiers,
            language_packs: &applicability_language_packs,
            notes: &applicability_notes,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-memory-promotion-local-post","status":"RECORDED","auth_session_status":{},"memory_promotion_id":{},"memory_candidate_state":{},"target_type":{},"target_path":{},"memory_promotion_receipt_id":{},"policy_decision_id":{},"projection_route":"/learning/memory-promotions/projection.json","active_memory_write_allowed":false,"generated_refresh_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.memory_promotion_id),
            json_string_literal(&report.memory_candidate_state),
            json_string_literal(&report.target_type),
            json_string_literal(&report.target_path),
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
    let candidates: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("learning.memory.promotion.requested")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"memory_promotion_receipt_id":{},"memory_promotion_id":{},"judgment_decision_id":{},"judgment_decision_receipt_id":{},"judgment_id":{},"promotion_id":{},"target_type":{},"target_path":{},"lesson_summary":{},"evidence_refs":{},"review_cadence":{},"expiry_rule":{},"applicability_work_tiers":{},"applicability_language_packs":{},"applicability_notes":{},"actor_id":{},"memory_candidate_state":{},"active_memory_write_allowed":false,"generated_refresh_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false}}"#,
                json_string_literal(&receipt.receipt_id),
                value("memory_promotion_id"),
                value("judgment_decision_id"),
                value("judgment_decision_receipt_id"),
                value("judgment_id"),
                value("promotion_id"),
                value("target_type"),
                value("target_path"),
                value("lesson_summary"),
                value("evidence_refs"),
                value("review_cadence"),
                value("expiry_rule"),
                value("applicability_work_tiers"),
                value("applicability_language_packs"),
                value("applicability_notes"),
                json_string_literal(receipt.actor_id.as_str()),
                value("memory_candidate_state"),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-memory-promotion-local-projection","receipt_kind":"learning.memory.promotion.requested","writes_route":"/learning/memory-promotions.json","memory_candidate_count":{},"candidates":[{}],"active_memory_write_allowed":false,"generated_refresh_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            candidates.len(),
            candidates.join(","),
        ),
    ))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-memory-promotion-local-post","status":"REFUSED","reason":{},"memory_promotion_receipt_id":"","active_memory_write_allowed":false,"generated_refresh_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
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
    use mdx_core::LearningJudgmentDecision;

    #[test]
    fn post_records_memory_candidate_and_projection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let judgment = kernel
            .write()
            .expect("kernel")
            .record_learning_judgment_decision(LearningJudgmentDecision {
                tenant_id: "t",
                actor_id: "human:eng",
                judgment_id: "judgment_1",
                promotion_id: "promotion_1",
                decision: "promote_candidate",
                rationale: "The evidence is enough to queue memory review.",
                evidence_refs: "make learning-loop-check",
            })
            .expect("judgment decision");
        let body = format!(
            r#"{{"judgment_decision_id":"decision_1","judgment_decision_receipt_id":"{}","judgment_id":"judgment_1","promotion_id":"promotion_1","target_type":"decision_record","target_path":"generated/learning/forge-outcome-memory-targets.json","lesson_summary":"Queue this as a memory candidate.","evidence_refs":["make learning-loop-check"],"review_cadence":"review before activation","expiry_rule":"supersede when stale"}}"#,
            judgment.receipt_id
        );
        let post = route_response("POST", "/learning/memory-promotions.json", &body, &kernel)
            .expect("memory promotion route")
            .expect("memory promotion response");
        assert_eq!(post.status, "200 OK");
        assert!(post.body.contains("\"status\":\"RECORDED\""));
        assert!(
            post.body
                .contains("\"memory_candidate_state\":\"candidate\"")
        );
        assert!(post.body.contains("\"active_memory_write_allowed\":false"));
        assert!(post.body.contains("\"adaptation_allowed\":false"));

        let projection = route_response(
            "GET",
            "/learning/memory-promotions/projection.json",
            "",
            &kernel,
        )
        .expect("memory projection route")
        .expect("memory projection response");
        assert_eq!(projection.status, "200 OK");
        assert!(projection.body.contains("\"memory_candidate_count\":1"));
        assert!(
            projection
                .body
                .contains("\"receipt_kind\":\"learning.memory.promotion.requested\"")
        );
        assert!(
            projection
                .body
                .contains("\"active_memory_write_allowed\":false")
        );
    }

    #[test]
    fn post_refuses_judgment_receipt_missing_from_ledger() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let post = route_response(
            "POST",
            "/learning/memory-promotions.json",
            r#"{"judgment_decision_id":"decision_1","judgment_decision_receipt_id":"receipt_missing","judgment_id":"judgment_1","promotion_id":"promotion_1","target_type":"decision_record","target_path":"generated/learning/forge-outcome-memory-targets.json","lesson_summary":"Queue this as a memory candidate.","evidence_refs":["make learning-loop-check"],"review_cadence":"review before activation","expiry_rule":"supersede when stale"}"#,
            &kernel,
        )
        .expect("memory promotion route")
        .expect("memory promotion response");
        assert_eq!(post.status, "200 OK");
        assert!(post.body.contains("\"status\":\"REFUSED\""));
        assert!(
            post.body
                .contains("judgment decision receipt receipt_missing is unknown")
        );
    }
}
