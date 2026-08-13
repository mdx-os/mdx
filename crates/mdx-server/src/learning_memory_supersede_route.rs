// Learn memory supersede retires an activated MDx memory as a receipt only.
// It verifies the activation receipt in the ledger, refuses a double
// supersede, and does not delete history, write provider memory, refresh
// generated artifacts, or adapt behavior.
use crate::RouteResponse;
use mdx_core::{LearningMemorySupersede, MdxKernel, json_string_literal};
use std::collections::BTreeSet;
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/learning/memory-supersedes.json" => Some(handle_post(method, body, kernel)),
        "/learning/memory-supersedes/projection.json" => Some(handle_projection(method, kernel)),
        _ => None,
    }
}

/// The activation receipt ids retired by a learning.memory.superseded receipt.
/// A lesson is superseded when any supersede receipt cites its activation
/// receipt id; readers skip those lessons without hiding the receipts.
pub(crate) fn superseded_activation_receipt_ids(kernel: &MdxKernel) -> BTreeSet<String> {
    kernel
        .ledger()
        .query()
        .by_kind("learning.memory.superseded")
        .iter()
        .filter_map(|receipt| receipt.payload.get("activation_receipt_id").cloned())
        .filter(|receipt_id| !receipt_id.is_empty())
        .collect()
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
    let report = match kernel.supersede_learning_memory_with_identity(
        LearningMemorySupersede {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            activation_receipt_id: &field("activation_receipt_id"),
            reason: &field("reason"),
            review_owner: &field("review_owner"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-memory-supersede-local-post","status":"RECORDED","auth_session_status":{},"memory_supersede_id":{},"activation_receipt_id":{},"active_memory_id":{},"active_memory_state":{},"memory_supersede_receipt_id":{},"policy_decision_id":{},"projection_route":"/learning/memory-supersedes/projection.json","memory_superseded":true,"receipts_deleted":false,"generated_refresh_allowed":false,"provider_memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.memory_supersede_id),
            json_string_literal(&report.activation_receipt_id),
            json_string_literal(&report.active_memory_id),
            json_string_literal(&report.active_memory_state),
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
    let superseded_memories: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("learning.memory.superseded")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"memory_supersede_receipt_id":{},"memory_supersede_id":{},"activation_receipt_id":{},"active_memory_id":{},"lesson_summary":{},"reason":{},"review_owner":{},"actor_id":{},"active_memory_state":{},"memory_superseded":true,"receipts_deleted":false,"generated_refresh_allowed":false,"provider_memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false}}"#,
                json_string_literal(&receipt.receipt_id),
                value("memory_supersede_id"),
                value("activation_receipt_id"),
                value("active_memory_id"),
                value("lesson_summary"),
                value("reason"),
                value("review_owner"),
                json_string_literal(receipt.actor_id.as_str()),
                value("active_memory_state"),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-memory-supersede-local-projection","receipt_kind":"learning.memory.superseded","writes_route":"/learning/memory-supersedes.json","superseded_memory_count":{},"superseded_memories":[{}],"memory_superseded":true,"receipts_deleted":false,"generated_refresh_allowed":false,"provider_memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
            superseded_memories.len(),
            superseded_memories.join(","),
        ),
    ))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-memory-supersede-local-post","status":"REFUSED","reason":{},"memory_supersede_receipt_id":"","memory_superseded":false,"receipts_deleted":false,"generated_refresh_allowed":false,"provider_memory_write_allowed":false,"adaptation_allowed":false,"runtime_behavior_change_allowed":false,"production_write_allowed":false}}"#,
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
    use mdx_core::{LearningJudgmentDecision, LearningMemoryActivation, LearningMemoryPromotion};

    fn activate(kernel: &Arc<RwLock<MdxKernel>>) -> String {
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
                lesson_summary: "Queue this lesson as a decision-record memory candidate.",
                evidence_refs: "make learning-loop-check",
                review_cadence: "review before activation",
                expiry_rule: "supersede when stale",
            })
            .expect("memory promotion");
        kernel
            .activate_learning_memory(LearningMemoryActivation {
                tenant_id: "t",
                actor_id: "human:eng",
                memory_promotion_id: "memory_promotion_1",
                memory_promotion_receipt_id: &promotion.receipt_id,
                judgment_decision_receipt_id: &judgment.receipt_id,
                target_type: "decision_record",
                target_path: "generated/learning/forge-outcome-memory-targets.json",
                lesson_summary: "Activate this as MDx-owned memory.",
                evidence_refs: "make learning-loop-check",
                activation_basis: "human approval with local proof",
                rollback_plan: "supersede through a later memory receipt",
                local_checks: "make learning-loop-check",
                approval_refs: "approval:local",
                review_owner: "human:eng",
            })
            .expect("memory activation")
            .receipt_id
    }

    #[test]
    fn post_supersedes_memory_and_projection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let activation_receipt_id = activate(&kernel);
        let post = route_response(
            "POST",
            "/learning/memory-supersedes.json",
            &format!(
                r#"{{"activation_receipt_id":"{activation_receipt_id}","reason":"The lesson misdiagnosed the intent and steered later runs wrong.","review_owner":"human:eng"}}"#
            ),
            &kernel,
        )
        .expect("memory supersede route")
        .expect("memory supersede response");
        assert_eq!(post.status, "200 OK");
        assert!(post.body.contains("\"status\":\"RECORDED\""));
        assert!(post.body.contains("\"active_memory_state\":\"superseded\""));
        assert!(post.body.contains("\"memory_superseded\":true"));
        assert!(post.body.contains("\"receipts_deleted\":false"));
        assert!(
            post.body
                .contains("\"runtime_behavior_change_allowed\":false")
        );

        let projection = route_response(
            "GET",
            "/learning/memory-supersedes/projection.json",
            "",
            &kernel,
        )
        .expect("memory supersede projection route")
        .expect("memory supersede projection response");
        assert_eq!(projection.status, "200 OK");
        assert!(projection.body.contains("\"superseded_memory_count\":1"));
        assert!(
            projection
                .body
                .contains("\"receipt_kind\":\"learning.memory.superseded\"")
        );
        assert!(projection.body.contains(&format!(
            "\"activation_receipt_id\":\"{activation_receipt_id}\""
        )));
        assert!(projection.body.contains("\"adaptation_allowed\":false"));

        let superseded = {
            let kernel = kernel.read().expect("kernel");
            superseded_activation_receipt_ids(&kernel)
        };
        assert!(superseded.contains(&activation_receipt_id));
    }

    #[test]
    fn post_refuses_unknown_activation_receipt_and_double_supersede() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let unknown = route_response(
            "POST",
            "/learning/memory-supersedes.json",
            r#"{"activation_receipt_id":"receipt_that_does_not_exist","reason":"Stale lesson.","review_owner":"human:eng"}"#,
            &kernel,
        )
        .expect("memory supersede route")
        .expect("memory supersede response");
        assert!(unknown.body.contains("\"status\":\"REFUSED\""));
        assert!(unknown.body.contains("is unknown"));

        let activation_receipt_id = activate(&kernel);
        let body = format!(
            r#"{{"activation_receipt_id":"{activation_receipt_id}","reason":"Stale lesson.","review_owner":"human:eng"}}"#
        );
        let first = route_response("POST", "/learning/memory-supersedes.json", &body, &kernel)
            .expect("memory supersede route")
            .expect("memory supersede response");
        assert!(first.body.contains("\"status\":\"RECORDED\""));
        let second = route_response("POST", "/learning/memory-supersedes.json", &body, &kernel)
            .expect("memory supersede route")
            .expect("memory supersede response");
        assert!(second.body.contains("\"status\":\"REFUSED\""));
        assert!(second.body.contains("already superseded"));
    }
}
