// The product pipeline's verbs over HTTP: bets shaped and re-shaped,
// their conditions stated and judged with the same evidence discipline
// as the strategy board, and resolutions that close the loop honestly.
// The bet-drafts projection folds re-shapes to the latest per bet so the
// pipeline reads current truth with the full shape history beneath it.
use crate::RouteResponse;
use mdx_core::{
    MdxKernel, PRODUCT_RESOLUTION_OUTCOMES, ProductBetCondition, ProductBetConditionUpdate,
    ProductBetDraft, ProductBetResolution, STRATEGY_CONDITION_STATUSES, json_string_literal,
};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/product/bet-drafts.json" => Some(handle_draft_post(method, body, kernel)),
        "/product/bet-drafts/projection.json" => Some(handle_drafts_projection(method, kernel)),
        "/product/bet-conditions.json" => Some(handle_condition_post(method, body, kernel)),
        "/product/bet-conditions/projection.json" => {
            Some(handle_conditions_projection(method, kernel))
        }
        "/product/bet-condition-updates.json" => Some(handle_update_post(method, body, kernel)),
        "/product/bet-resolutions.json" => Some(handle_resolution_post(method, body, kernel)),
        "/product/bet-resolutions/projection.json" => {
            Some(handle_resolutions_projection(method, kernel))
        }
        _ => None,
    }
}

fn refusal(name: &str, reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":{},"status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
            json_string_literal(name),
            json_string_literal(reason)
        ),
    )
}

fn handle_draft_post(
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
    let report = match kernel.save_product_bet_draft_local_with_identity(
        ProductBetDraft {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            bet_id: &field("bet_id"),
            bet: &field("bet"),
            for_whom: &field("for_whom"),
            success_metric: &field("success_metric"),
            kill_condition: &field("kill_condition"),
            direction_ref: &field("direction_ref"),
            slice: &field("slice"),
            slice_not: &field("slice_not"),
            signal_ref: &field("signal_ref"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(refusal(
                "mdx-product-bet-draft-local-post",
                &error.message(),
            ));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-product-bet-draft-local-post","status":"RECORDED","auth_session_status":{},"bet_id":{},"bet_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"ratification_required":true,"ratification_route":"/product/ratification-decisions.json","projection_route":"/product/bet-drafts/projection.json","authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.record_id),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(report.terminal_state),
        ),
    ))
}

fn handle_condition_post(
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
    let report = match kernel.save_product_bet_condition_local_with_identity(
        ProductBetCondition {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            bet_id: &field("bet_id"),
            claim: &field("claim"),
            make_or_break: field("make_or_break") == "true",
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(refusal(
                "mdx-product-bet-condition-local-post",
                &error.message(),
            ));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-product-bet-condition-local-post","status":"RECORDED","auth_session_status":{},"condition_id":{},"bet_id":{},"condition_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"condition_status":"assumed","update_route":"/product/bet-condition-updates.json","projection_route":"/product/bet-conditions/projection.json","authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.record_id),
            json_string_literal(&field("bet_id")),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(report.terminal_state),
        ),
    ))
}

fn handle_update_post(
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
    let status = field("status");
    let condition_id = field("condition_id");
    let report = match kernel.save_product_bet_condition_update_local_with_identity(
        ProductBetConditionUpdate {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            condition_id: &condition_id,
            status: &status,
            evidence_ref: &field("evidence_ref"),
            note: &field("note"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(refusal(
                "mdx-product-bet-condition-update-local-post",
                &error.message(),
            ));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-product-bet-condition-update-local-post","status":"RECORDED","auth_session_status":{},"update_id":{},"condition_id":{},"condition_status":{},"update_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"allowed_statuses":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.record_id),
            json_string_literal(&condition_id),
            json_string_literal(&status),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(report.terminal_state),
            STRATEGY_CONDITION_STATUSES
                .iter()
                .map(|s| json_string_literal(s))
                .collect::<Vec<_>>()
                .join(","),
        ),
    ))
}

fn handle_resolution_post(
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
    let bet_id = field("bet_id");
    let outcome = field("outcome");
    let report = match kernel.save_product_bet_resolution_local_with_identity(
        ProductBetResolution {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            bet_id: &bet_id,
            outcome: &outcome,
            metric_result: &field("metric_result"),
            measurement_unavailable: field("measurement_unavailable") == "true",
            learned: &field("learned"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(refusal(
                "mdx-product-bet-resolution-local-post",
                &error.message(),
            ));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-product-bet-resolution-local-post","status":"RECORDED","auth_session_status":{},"resolution_id":{},"bet_id":{},"outcome":{},"resolution_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"allowed_outcomes":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.record_id),
            json_string_literal(&bet_id),
            json_string_literal(&outcome),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(report.terminal_state),
            PRODUCT_RESOLUTION_OUTCOMES
                .iter()
                .map(|s| json_string_literal(s))
                .collect::<Vec<_>>()
                .join(","),
        ),
    ))
}

fn handle_drafts_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    // Fold re-shapes to the latest per bet: later receipts overwrite
    // earlier fields wholesale (each shape carries the full anatomy).
    let mut latest: BTreeMap<String, (String, BTreeMap<String, String>, String)> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for receipt in kernel.ledger().query().by_kind("product.bet.shaped").iter() {
        let bet_id = receipt.payload.get("bet_id").cloned().unwrap_or_default();
        if bet_id.is_empty() {
            continue;
        }
        if !latest.contains_key(&bet_id) {
            order.push(bet_id.clone());
        }
        latest.insert(
            bet_id.clone(),
            (
                receipt.receipt_id.clone(),
                receipt.payload.clone(),
                receipt.actor_id.as_str().to_string(),
            ),
        );
    }
    let bets: Vec<String> = order
        .iter()
        .map(|bet_id| {
            let (receipt_id, fields, actor) = &latest[bet_id];
            let value = |key: &str| {
                json_string_literal(fields.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"bet_id":{},"bet_receipt_id":{},"bet":{},"for_whom":{},"success_metric":{},"kill_condition":{},"direction_ref":{},"slice":{},"slice_not":{},"signal_ref":{},"actor_id":{}}}"#,
                json_string_literal(bet_id),
                json_string_literal(receipt_id),
                value("bet"),
                value("for_whom"),
                value("success_metric"),
                value("kill_condition"),
                value("direction_ref"),
                value("slice"),
                value("slice_not"),
                value("signal_ref"),
                json_string_literal(actor),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-product-bet-draft-local-projection","receipt_kind":"product.bet.shaped","bet_count":{},"bets":[{}],"ratification_route":"/product/ratification-decisions.json","authority_opened":"none","production_write_allowed":false}}"#,
            bets.len(),
            bets.join(","),
        ),
    ))
}

fn handle_conditions_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let ledger = kernel.ledger();
    let stated = ledger.query().by_kind("product.condition.stated");
    let updates = ledger.query().by_kind("product.condition.updated");
    let conditions: Vec<String> = stated
        .iter()
        .map(|receipt| {
            let value = |key: &str| receipt.payload.get(key).map(String::as_str).unwrap_or("");
            let condition_id = value("condition_id");
            let mut status = "assumed".to_string();
            let mut latest_evidence = String::new();
            let trail: Vec<String> = updates
                .iter()
                .filter(|update| {
                    update.payload.get("condition_id").map(String::as_str) == Some(condition_id)
                })
                .map(|update| {
                    let uv =
                        |key: &str| update.payload.get(key).map(String::as_str).unwrap_or("");
                    status = uv("status").to_string();
                    if !uv("evidence_ref").is_empty() {
                        latest_evidence = uv("evidence_ref").to_string();
                    }
                    format!(
                        r#"{{"update_receipt_id":{},"status":{},"evidence_ref":{},"note":{},"actor_id":{}}}"#,
                        json_string_literal(&update.receipt_id),
                        json_string_literal(uv("status")),
                        json_string_literal(uv("evidence_ref")),
                        json_string_literal(uv("note")),
                        json_string_literal(update.actor_id.as_str()),
                    )
                })
                .collect();
            format!(
                r#"{{"condition_id":{},"bet_id":{},"claim":{},"make_or_break":{},"status":{},"evidence_ref":{},"stated_receipt_id":{},"actor_id":{},"updates":[{}]}}"#,
                json_string_literal(condition_id),
                json_string_literal(value("bet_id")),
                json_string_literal(value("claim")),
                value("make_or_break") == "true",
                json_string_literal(&status),
                json_string_literal(&latest_evidence),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.actor_id.as_str()),
                trail.join(","),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-product-bet-condition-local-projection","receipt_kind":"product.condition.stated","condition_count":{},"conditions":[{}],"allowed_statuses":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
            conditions.len(),
            conditions.join(","),
            STRATEGY_CONDITION_STATUSES
                .iter()
                .map(|s| json_string_literal(s))
                .collect::<Vec<_>>()
                .join(","),
        ),
    ))
}

fn handle_resolutions_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let resolutions: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("product.bet.resolved")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"resolution_receipt_id":{},"resolution_id":{},"bet_id":{},"outcome":{},"metric_result":{},"measurement_unavailable":{},"learned":{},"actor_id":{}}}"#,
                json_string_literal(&receipt.receipt_id),
                value("resolution_id"),
                value("bet_id"),
                value("outcome"),
                value("metric_result"),
                receipt
                    .payload
                    .get("measurement_unavailable")
                    .map(String::as_str)
                    == Some("true"),
                value("learned"),
                json_string_literal(receipt.actor_id.as_str()),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-product-bet-resolution-local-projection","receipt_kind":"product.bet.resolved","resolution_count":{},"resolutions":[{}],"allowed_outcomes":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
            resolutions.len(),
            resolutions.join(","),
            PRODUCT_RESOLUTION_OUTCOMES
                .iter()
                .map(|s| json_string_literal(s))
                .collect::<Vec<_>>()
                .join(","),
        ),
    ))
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
