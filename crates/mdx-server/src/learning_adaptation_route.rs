// Learn adaptation opens the first governed casting gate. A grant lets one
// activated lesson steer future fleet casting - preferring or avoiding the
// runners it names - and nothing else. It verifies the activation receipt in
// the ledger, refuses a superseded activation, refuses a second open grant,
// and records a receipt. The withdraw route closes a grant so the consuming
// seams stop honoring it immediately. Model routing, budgets, check policy,
// and review depth stay closed.
use crate::RouteResponse;
use mdx_core::{
    LearningAdaptationGrant, LearningAdaptationSupersede, MdxKernel, json_string_literal,
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/learning/adaptation-grants.json" => Some(handle_grant(method, body, kernel)),
        "/learning/adaptation-supersedes.json" => Some(handle_supersede(method, body, kernel)),
        "/learning/adaptation-grants/projection.json" => Some(handle_projection(method, kernel)),
        _ => None,
    }
}

fn handle_grant(
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
    let adaptation_type = {
        let value = field("adaptation_type");
        if value.trim().is_empty() {
            "fleet_casting".to_string()
        } else {
            value
        }
    };
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.grant_learning_adaptation_with_identity(
        LearningAdaptationGrant {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            activation_receipt_id: &field("activation_receipt_id"),
            adaptation_type: &adaptation_type,
            target_type: &field("target_type"),
            preferred_builder_slot: &field("preferred_builder_slot"),
            reason: &field("reason"),
            review_owner: &field("review_owner"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(grant_refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-adaptation-grant-local-post","status":"RECORDED","auth_session_status":{},"adaptation_grant_id":{},"activation_receipt_id":{},"active_memory_id":{},"adaptation_type":{},"target_type":{},"preferred_builder_slot":{},"grant_state":{},"adaptation_grant_receipt_id":{},"policy_decision_id":{},"projection_route":"/learning/adaptation-grants/projection.json","adaptation_allowed":true,"adaptation_scope":"fleet_casting","fleet_casting_change_allowed":true,"model_routing_change_allowed":false,"budget_change_allowed":false,"check_policy_change_allowed":false,"review_depth_change_allowed":false,"reversible":true,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.adaptation_grant_id),
            json_string_literal(&report.activation_receipt_id),
            json_string_literal(&report.active_memory_id),
            json_string_literal(&report.adaptation_type),
            json_string_literal(&report.target_type),
            json_string_literal(&report.preferred_builder_slot),
            json_string_literal(&report.grant_state),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
        ),
    ))
}

fn handle_supersede(
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
    let report = match kernel.supersede_learning_adaptation_with_identity(
        LearningAdaptationSupersede {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            grant_receipt_id: &field("grant_receipt_id"),
            reason: &field("reason"),
            review_owner: &field("review_owner"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(supersede_refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-adaptation-supersede-local-post","status":"RECORDED","auth_session_status":{},"adaptation_supersede_id":{},"grant_receipt_id":{},"activation_receipt_id":{},"grant_state":{},"adaptation_supersede_receipt_id":{},"policy_decision_id":{},"projection_route":"/learning/adaptation-grants/projection.json","adaptation_allowed":false,"adaptation_withdrawn":true,"receipts_deleted":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.adaptation_supersede_id),
            json_string_literal(&report.grant_receipt_id),
            json_string_literal(&report.activation_receipt_id),
            json_string_literal(&report.grant_state),
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
    let withdrawn = kernel.withdrawn_grant_receipt_ids();
    // Applied-receipt count per grant, so the surface can show that a grant
    // actually informed a plan or a slot decision.
    let mut applied_by_grant: std::collections::BTreeMap<String, usize> =
        std::collections::BTreeMap::new();
    for receipt in kernel
        .ledger()
        .query()
        .by_kind("learning.adaptation.applied")
        .iter()
    {
        if let Some(grant_id) = receipt.payload.get("grant_receipt_id") {
            *applied_by_grant.entry(grant_id.clone()).or_default() += 1;
        }
    }
    let grants: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("learning.adaptation.granted")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            let open = !withdrawn.contains(&receipt.receipt_id);
            let applied_count = applied_by_grant
                .get(&receipt.receipt_id)
                .copied()
                .unwrap_or(0);
            format!(
                r#"{{"adaptation_grant_receipt_id":{},"adaptation_grant_id":{},"activation_receipt_id":{},"active_memory_id":{},"lesson_summary":{},"adaptation_type":{},"target_type":{},"preferred_builder_slot":{},"reason":{},"review_owner":{},"actor_id":{},"grant_state":{},"open":{},"applied_count":{},"adaptation_allowed":{},"adaptation_scope":"fleet_casting","model_routing_change_allowed":false,"budget_change_allowed":false,"check_policy_change_allowed":false,"review_depth_change_allowed":false}}"#,
                json_string_literal(&receipt.receipt_id),
                value("adaptation_grant_id"),
                value("activation_receipt_id"),
                value("active_memory_id"),
                value("lesson_summary"),
                value("adaptation_type"),
                value("target_type"),
                value("preferred_builder_slot"),
                value("reason"),
                value("review_owner"),
                json_string_literal(receipt.actor_id.as_str()),
                json_string_literal(if open { "granted" } else { "withdrawn" }),
                open,
                applied_count,
                open,
            )
        })
        .collect();
    let open_count = kernel.active_fleet_casting_grants().len();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-adaptation-grant-local-projection","receipt_kind":"learning.adaptation.granted","writes_route":"/learning/adaptation-grants.json","supersede_route":"/learning/adaptation-supersedes.json","grant_count":{},"open_grant_count":{},"grants":[{}],"adaptation_scope":"fleet_casting","model_routing_change_allowed":false,"budget_change_allowed":false,"check_policy_change_allowed":false,"review_depth_change_allowed":false,"production_write_allowed":false}}"#,
            grants.len(),
            open_count,
            grants.join(","),
        ),
    ))
}

fn grant_refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-adaptation-grant-local-post","status":"REFUSED","reason":{},"adaptation_grant_receipt_id":"","adaptation_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

fn supersede_refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-learning-adaptation-supersede-local-post","status":"REFUSED","reason":{},"adaptation_supersede_receipt_id":"","adaptation_withdrawn":false,"production_write_allowed":false}}"#,
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
                target_type: "model_scorecard",
                target_path: "generated/learning/model-worker-scorecard-targets.json",
                lesson_summary: "Prefer the sovereign builder on sensitive work.",
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
                target_type: "model_scorecard",
                target_path: "generated/learning/model-worker-scorecard-targets.json",
                lesson_summary: "Prefer the sovereign builder on sensitive work.",
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
    fn post_grants_and_projection_shows_open_grant() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let activation_receipt_id = activate(&kernel);
        let post = route_response(
            "POST",
            "/learning/adaptation-grants.json",
            &format!(
                r#"{{"activation_receipt_id":"{activation_receipt_id}","adaptation_type":"fleet_casting","target_type":"model_scorecard","preferred_builder_slot":"OPUS","reason":"Prefer the sovereign builder here.","review_owner":"human:eng"}}"#
            ),
            &kernel,
        )
        .expect("grant route")
        .expect("grant response");
        assert!(post.body.contains("\"status\":\"RECORDED\""));
        assert!(post.body.contains("\"adaptation_allowed\":true"));
        assert!(post.body.contains("\"model_routing_change_allowed\":false"));
        assert!(post.body.contains("\"preferred_builder_slot\":\"OPUS\""));

        let projection = route_response(
            "GET",
            "/learning/adaptation-grants/projection.json",
            "",
            &kernel,
        )
        .expect("projection route")
        .expect("projection response");
        assert!(projection.body.contains("\"grant_count\":1"));
        assert!(projection.body.contains("\"open_grant_count\":1"));
        assert!(projection.body.contains("\"grant_state\":\"granted\""));
    }

    #[test]
    fn post_refuses_unknown_activation_and_withdraw_closes_grant() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let unknown = route_response(
            "POST",
            "/learning/adaptation-grants.json",
            r#"{"activation_receipt_id":"receipt_missing","target_type":"model_scorecard","reason":"x","review_owner":"human:eng"}"#,
            &kernel,
        )
        .expect("grant route")
        .expect("grant response");
        assert!(unknown.body.contains("\"status\":\"REFUSED\""));
        assert!(unknown.body.contains("is unknown"));

        let activation_receipt_id = activate(&kernel);
        let grant_body = format!(
            r#"{{"activation_receipt_id":"{activation_receipt_id}","target_type":"model_scorecard","preferred_builder_slot":"OPUS","reason":"Prefer sovereign.","review_owner":"human:eng"}}"#
        );
        let grant = route_response(
            "POST",
            "/learning/adaptation-grants.json",
            &grant_body,
            &kernel,
        )
        .expect("grant route")
        .expect("grant response");
        assert!(grant.body.contains("\"status\":\"RECORDED\""));
        let grant_receipt_id = extract_json_value(&grant.body, "adaptation_grant_receipt_id");

        let withdraw = route_response(
            "POST",
            "/learning/adaptation-supersedes.json",
            &format!(
                r#"{{"grant_receipt_id":"{grant_receipt_id}","reason":"The lesson was wrong.","review_owner":"human:eng"}}"#
            ),
            &kernel,
        )
        .expect("supersede route")
        .expect("supersede response");
        assert!(withdraw.body.contains("\"status\":\"RECORDED\""));
        assert!(withdraw.body.contains("\"grant_state\":\"withdrawn\""));

        let projection = route_response(
            "GET",
            "/learning/adaptation-grants/projection.json",
            "",
            &kernel,
        )
        .expect("projection route")
        .expect("projection response");
        assert!(projection.body.contains("\"open_grant_count\":0"));
    }

    fn extract_json_value(body: &str, key: &str) -> String {
        json_string_field(body, key).unwrap_or_default()
    }
}
