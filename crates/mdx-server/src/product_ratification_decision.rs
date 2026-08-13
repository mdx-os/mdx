// The Product surface's governed verb: the bet's sibling of the Strategy
// decision. POST records whether the human backed the shaped bet, held it,
// or sent it back - on the chain, with the verified identity. Recording
// opens no authority - every blocked action stays blocked.
use crate::RouteResponse;
use mdx_core::{
    MdxKernel, PRODUCT_RATIFICATION_DECISIONS, ProductRatificationDecision, json_string_literal,
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/product/ratification-decisions.json" => Some(handle_post(method, body, kernel)),
        "/product/ratification-decisions/projection.json" => {
            Some(handle_projection(method, kernel))
        }
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
    let bet_id = field("bet_id");
    let decision = field("decision");
    let reason = field("reason");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.save_product_ratification_decision_local_with_identity(
        ProductRatificationDecision {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            bet_id: &bet_id,
            decision: &decision,
            reason: &reason,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(RouteResponse::json(
                "200 OK",
                format!(
                    r#"{{"name":"mdx-product-ratification-decision-local-post","status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
                    json_string_literal(&error.message())
                ),
            ));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-product-ratification-decision-local-post","status":"RECORDED","auth_session_status":{},"bet_id":{},"decision":{},"decision_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"authority_opened":"none","projection_route":"/product/ratification-decisions/projection.json","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&bet_id),
            json_string_literal(&report.decision),
            json_string_literal(&report.decision_receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(report.terminal_state),
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
        .by_kind("product.ratification.recorded")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"decision_receipt_id":{},"bet_id":{},"decision":{},"reason":{},"actor_id":{},"identity_source":{}}}"#,
                json_string_literal(&receipt.receipt_id),
                value("bet_id"),
                value("decision"),
                value("reason"),
                json_string_literal(receipt.actor_id.as_str()),
                value("identity_source"),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-product-ratification-decision-local-projection","receipt_kind":"product.ratification.recorded","decision_count":{},"allowed_decisions":[{}],"decisions":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
            decisions.len(),
            PRODUCT_RATIFICATION_DECISIONS
                .iter()
                .map(|option| json_string_literal(option))
                .collect::<Vec<_>>()
                .join(","),
            decisions.join(","),
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
