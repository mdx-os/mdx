// The strategy board's verbs over HTTP: conditions stated and updated,
// proposals resolved, and the projections the board reads. Stage is
// derived by the reader from these records - the kernel stores evidence,
// never column positions.
use crate::RouteResponse;
use mdx_core::{
    MdxKernel, STRATEGY_CONDITION_STATUSES, STRATEGY_RESOLUTION_OUTCOMES, StrategyCondition,
    StrategyConditionUpdate, StrategyProposalResolution, json_string_literal,
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/strategy/conditions.json" => Some(handle_condition_post(method, body, kernel)),
        "/strategy/conditions/projection.json" => {
            Some(handle_conditions_projection(method, kernel))
        }
        "/strategy/condition-updates.json" => Some(handle_update_post(method, body, kernel)),
        "/strategy/proposal-resolutions.json" => Some(handle_resolution_post(method, body, kernel)),
        "/strategy/proposal-resolutions/projection.json" => {
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
    let proposal_id = field("proposal_id");
    let claim = field("claim");
    let make_or_break = field("make_or_break") == "true";
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.save_strategy_condition_local_with_identity(
        StrategyCondition {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            proposal_id: &proposal_id,
            claim: &claim,
            make_or_break,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(refusal(
                "mdx-strategy-condition-local-post",
                &error.message(),
            ));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-strategy-condition-local-post","status":"RECORDED","auth_session_status":{},"condition_id":{},"proposal_id":{},"condition_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"condition_status":"assumed","update_route":"/strategy/condition-updates.json","projection_route":"/strategy/conditions/projection.json","authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.record_id),
            json_string_literal(&proposal_id),
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
    let condition_id = field("condition_id");
    let status = field("status");
    let evidence_ref = field("evidence_ref");
    let note = field("note");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.save_strategy_condition_update_local_with_identity(
        StrategyConditionUpdate {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            condition_id: &condition_id,
            status: &status,
            evidence_ref: &evidence_ref,
            note: &note,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(refusal(
                "mdx-strategy-condition-update-local-post",
                &error.message(),
            ));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-strategy-condition-update-local-post","status":"RECORDED","auth_session_status":{},"update_id":{},"condition_id":{},"condition_status":{},"update_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"allowed_statuses":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
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
    let proposal_id = field("proposal_id");
    let outcome = field("outcome");
    let learned = field("learned");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.save_strategy_proposal_resolution_local_with_identity(
        StrategyProposalResolution {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            proposal_id: &proposal_id,
            outcome: &outcome,
            learned: &learned,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(refusal(
                "mdx-strategy-proposal-resolution-local-post",
                &error.message(),
            ));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-strategy-proposal-resolution-local-post","status":"RECORDED","auth_session_status":{},"resolution_id":{},"proposal_id":{},"outcome":{},"resolution_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"allowed_outcomes":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.record_id),
            json_string_literal(&proposal_id),
            json_string_literal(&outcome),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(report.terminal_state),
            STRATEGY_RESOLUTION_OUTCOMES
                .iter()
                .map(|s| json_string_literal(s))
                .collect::<Vec<_>>()
                .join(","),
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
    // Latest-status fold: stated receipts define the condition; updates in
    // chain order move its status. The projection serves the fold AND the
    // update trail so the board can show both the verdict and the work.
    let ledger = kernel.ledger();
    let stated = ledger.query().by_kind("strategy.condition.stated");
    let updates = ledger.query().by_kind("strategy.condition.updated");
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
                r#"{{"condition_id":{},"proposal_id":{},"claim":{},"make_or_break":{},"status":{},"evidence_ref":{},"stated_receipt_id":{},"actor_id":{},"updates":[{}]}}"#,
                json_string_literal(condition_id),
                json_string_literal(value("proposal_id")),
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
            r#"{{"name":"mdx-strategy-condition-local-projection","receipt_kind":"strategy.condition.stated","condition_count":{},"conditions":[{}],"allowed_statuses":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
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
        .by_kind("strategy.proposal.resolved")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"resolution_receipt_id":{},"resolution_id":{},"proposal_id":{},"outcome":{},"learned":{},"actor_id":{}}}"#,
                json_string_literal(&receipt.receipt_id),
                value("resolution_id"),
                value("proposal_id"),
                value("outcome"),
                value("learned"),
                json_string_literal(receipt.actor_id.as_str()),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-strategy-proposal-resolution-local-projection","receipt_kind":"strategy.proposal.resolved","resolution_count":{},"resolutions":[{}],"allowed_outcomes":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
            resolutions.len(),
            resolutions.join(","),
            STRATEGY_RESOLUTION_OUTCOMES
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
