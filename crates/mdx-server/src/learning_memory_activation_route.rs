// Learn memory activation records active MDx-owned memory as a receipt only.
// It does not write provider memory, refresh generated artifacts, or adapt behavior.
use crate::RouteResponse;
use mdx_core::{
    LearningMemoryActivation, LearningMemoryApplicability, MdxKernel, json_string_literal,
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/learning/memory-activations.json" => Some(handle_post(method, body, kernel)),
        "/learning/memory-activations/projection.json" => Some(handle_projection(method, kernel)),
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
    let local_checks = json_string_or_string_array_field(body, "local_checks").unwrap_or_default();
    let approval_refs =
        json_string_or_string_array_field(body, "approval_refs").unwrap_or_default();
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
    let report = match kernel.activate_learning_memory_with_applicability(
        LearningMemoryActivation {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            memory_promotion_id: &field("memory_promotion_id"),
            memory_promotion_receipt_id: &field("memory_promotion_receipt_id"),
            judgment_decision_receipt_id: &field("judgment_decision_receipt_id"),
            target_type: &field("target_type"),
            target_path: &field("target_path"),
            lesson_summary: &field("lesson_summary"),
            evidence_refs: &evidence_refs,
            activation_basis: &field("activation_basis"),
            rollback_plan: &field("rollback_plan"),
            local_checks: &local_checks,
            approval_refs: &approval_refs,
            review_owner: &field("review_owner"),
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
            r#"{{"name":"mdx-learning-memory-activation-local-post","status":"RECORDED","auth_session_status":{},"active_memory_id":{},"active_memory_state":{},"target_type":{},"target_path":{},"active_memory_receipt_id":{},"policy_decision_id":{},"projection_route":"/learning/memory-activations/projection.json","active_memory_written":true,"generated_refresh_allowed":false,"provider_memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.active_memory_id),
            json_string_literal(&report.active_memory_state),
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
    // Superseded lessons stay visible in this projection; only their state
    // changes, so the record never hides a retirement.
    let superseded =
        crate::learning_routes::memory_supersede::superseded_activation_receipt_ids(&kernel);
    let active_memories: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("learning.memory.activated")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            let active_memory_state = if superseded.contains(&receipt.receipt_id) {
                json_string_literal("superseded")
            } else {
                value("active_memory_state")
            };
            format!(
                r#"{{"active_memory_receipt_id":{},"active_memory_id":{},"memory_promotion_id":{},"memory_promotion_receipt_id":{},"judgment_decision_receipt_id":{},"target_type":{},"target_path":{},"lesson_summary":{},"evidence_refs":{},"activation_basis":{},"rollback_plan":{},"local_checks":{},"approval_refs":{},"review_owner":{},"applicability_work_tiers":{},"applicability_language_packs":{},"applicability_notes":{},"actor_id":{},"active_memory_state":{},"active_memory_written":true,"generated_refresh_allowed":false,"provider_memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false}}"#,
                json_string_literal(&receipt.receipt_id),
                value("active_memory_id"),
                value("memory_promotion_id"),
                value("memory_promotion_receipt_id"),
                value("judgment_decision_receipt_id"),
                value("target_type"),
                value("target_path"),
                value("lesson_summary"),
                value("evidence_refs"),
                value("activation_basis"),
                value("rollback_plan"),
                value("local_checks"),
                value("approval_refs"),
                value("review_owner"),
                value("applicability_work_tiers"),
                value("applicability_language_packs"),
                value("applicability_notes"),
                json_string_literal(receipt.actor_id.as_str()),
                active_memory_state,
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-memory-activation-local-projection","receipt_kind":"learning.memory.activated","writes_route":"/learning/memory-activations.json","active_memory_count":{},"active_memories":[{}],"active_memory_written":true,"generated_refresh_allowed":false,"provider_memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            active_memories.len(),
            active_memories.join(","),
        ),
    ))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-memory-activation-local-post","status":"REFUSED","reason":{},"active_memory_receipt_id":"","active_memory_written":false,"generated_refresh_allowed":false,"provider_memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
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
    use mdx_core::{LearningJudgmentDecision, LearningMemoryPromotion};

    fn seed_promotion_chain(kernel: &Arc<RwLock<MdxKernel>>) -> (String, String) {
        let mut kernel = kernel.write().expect("kernel");
        let judgment = kernel
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
        let promotion = kernel
            .request_learning_memory_promotion(LearningMemoryPromotion {
                tenant_id: "t",
                actor_id: "human:eng",
                judgment_decision_id: &judgment.judgment_decision_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                judgment_id: "judgment_1",
                promotion_id: "promotion_1",
                target_type: "decision_record",
                target_path: "generated/learning/forge-outcome-memory-targets.json",
                lesson_summary: "Queue this as a memory candidate.",
                evidence_refs: "make learning-loop-check",
                review_cadence: "review before activation",
                expiry_rule: "supersede when stale",
            })
            .expect("memory promotion");
        (judgment.receipt_id, promotion.receipt_id)
    }

    #[test]
    fn post_activates_memory_and_projection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let (judgment_receipt_id, promotion_receipt_id) = seed_promotion_chain(&kernel);
        let body = format!(
            r#"{{"memory_promotion_id":"learning_memory_promotion_1","memory_promotion_receipt_id":"{promotion_receipt_id}","judgment_decision_receipt_id":"{judgment_receipt_id}","target_type":"decision_record","target_path":"generated/learning/forge-outcome-memory-targets.json","lesson_summary":"Activate this as MDx-owned memory.","evidence_refs":["{promotion_receipt_id}","make learning-loop-check"],"activation_basis":"approved with local proof","rollback_plan":"supersede through a later receipt","local_checks":["make learning-loop-check"],"approval_refs":["approval:local"],"review_owner":"human:eng"}}"#
        );
        let post = route_response("POST", "/learning/memory-activations.json", &body, &kernel)
            .expect("memory activation route")
            .expect("memory activation response");
        assert_eq!(post.status, "200 OK");
        assert!(post.body.contains("\"status\":\"RECORDED\""));
        assert!(post.body.contains("\"active_memory_state\":\"active\""));
        assert!(post.body.contains("\"active_memory_written\":true"));
        assert!(
            post.body
                .contains("\"provider_memory_write_allowed\":false")
        );
        assert!(
            post.body
                .contains("\"runtime_behavior_change_allowed\":false")
        );

        let projection = route_response(
            "GET",
            "/learning/memory-activations/projection.json",
            "",
            &kernel,
        )
        .expect("memory activation projection route")
        .expect("memory activation projection response");
        assert_eq!(projection.status, "200 OK");
        assert!(projection.body.contains("\"active_memory_count\":1"));
        assert!(
            projection
                .body
                .contains("\"receipt_kind\":\"learning.memory.activated\"")
        );
        assert!(
            projection
                .body
                .contains("\"active_memory_state\":\"active\"")
        );
        assert!(projection.body.contains("\"adaptation_allowed\":false"));
    }

    #[test]
    fn projection_marks_superseded_lessons_without_hiding_them() {
        use mdx_core::LearningMemorySupersede;
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let (judgment_receipt_id, promotion_receipt_id) = seed_promotion_chain(&kernel);
        let body = format!(
            r#"{{"memory_promotion_id":"learning_memory_promotion_1","memory_promotion_receipt_id":"{promotion_receipt_id}","judgment_decision_receipt_id":"{judgment_receipt_id}","target_type":"decision_record","target_path":"generated/learning/forge-outcome-memory-targets.json","lesson_summary":"Activate this as MDx-owned memory.","evidence_refs":["make learning-loop-check"],"activation_basis":"approved with local proof","rollback_plan":"supersede through a later receipt","local_checks":["make learning-loop-check"],"approval_refs":["approval:local"],"review_owner":"human:eng"}}"#
        );
        route_response("POST", "/learning/memory-activations.json", &body, &kernel)
            .expect("memory activation route")
            .expect("memory activation response");
        let activation_receipt_id = {
            let kernel = kernel.read().expect("kernel");
            kernel
                .ledger()
                .query()
                .by_kind("learning.memory.activated")
                .first()
                .expect("activation receipt")
                .receipt_id
                .clone()
        };
        kernel
            .write()
            .expect("kernel")
            .supersede_learning_memory(LearningMemorySupersede {
                tenant_id: "t",
                actor_id: "human:eng",
                activation_receipt_id: &activation_receipt_id,
                reason: "The lesson no longer holds.",
                review_owner: "human:eng",
            })
            .expect("memory supersede");

        let projection = route_response(
            "GET",
            "/learning/memory-activations/projection.json",
            "",
            &kernel,
        )
        .expect("memory activation projection route")
        .expect("memory activation projection response");
        // The record stays visible; only its state changes.
        assert!(projection.body.contains("\"active_memory_count\":1"));
        assert!(
            projection
                .body
                .contains("\"active_memory_state\":\"superseded\"")
        );
        assert!(
            !projection
                .body
                .contains("\"active_memory_state\":\"active\"")
        );
    }

    #[test]
    fn post_refuses_promotion_receipt_missing_from_ledger() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let post = route_response(
            "POST",
            "/learning/memory-activations.json",
            r#"{"memory_promotion_id":"learning_memory_promotion_1","memory_promotion_receipt_id":"receipt_missing","judgment_decision_receipt_id":"receipt_0","target_type":"decision_record","target_path":"generated/learning/forge-outcome-memory-targets.json","lesson_summary":"Activate this as MDx-owned memory.","evidence_refs":["make learning-loop-check"],"activation_basis":"approved with local proof","rollback_plan":"supersede through a later receipt","local_checks":["make learning-loop-check"],"approval_refs":["approval:local"],"review_owner":"human:eng"}"#,
            &kernel,
        )
        .expect("memory activation route")
        .expect("memory activation response");
        assert_eq!(post.status, "200 OK");
        assert!(post.body.contains("\"status\":\"REFUSED\""));
        assert!(
            post.body
                .contains("memory promotion receipt receipt_missing is unknown")
        );
        assert!(post.body.contains("\"active_memory_written\":false"));
    }
}
