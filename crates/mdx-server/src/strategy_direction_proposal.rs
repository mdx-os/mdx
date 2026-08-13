// The Strategy surface's authoring verb: POST records a direction in the
// proposer's own words, on the receipt chain, with the verified identity.
// The projection serves the proposal history so the decision room can put
// the latest real proposal on the table. Proposing decides nothing and
// opens no authority - the call still waits for a person.
use crate::RouteResponse;
use mdx_core::{MdxKernel, StrategyDirectionProposal, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/strategy/direction-proposals.json" => Some(handle_post(method, body, kernel)),
        "/strategy/direction-proposals/projection.json" => Some(handle_projection(method, kernel)),
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
    let direction = field("direction");
    let why_now = field("why_now");
    let problem = field("problem");
    let where_we_play = field("where_we_play");
    let how_we_win = field("how_we_win");
    let horizon = field("horizon");
    let sponsor = field("sponsor");
    let success_metric = field("success_metric");
    let kill_condition = field("kill_condition");
    let funding_ask = field("funding_ask");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.save_strategy_direction_proposal_local_with_identity(
        StrategyDirectionProposal {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            direction: &direction,
            why_now: &why_now,
            problem: &problem,
            where_we_play: &where_we_play,
            how_we_win: &how_we_win,
            horizon: &horizon,
            sponsor: &sponsor,
            success_metric: &success_metric,
            kill_condition: &kill_condition,
            funding_ask: &funding_ask,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(RouteResponse::json(
                "200 OK",
                format!(
                    r#"{{"name":"mdx-strategy-direction-proposal-local-post","status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
                    json_string_literal(&error.message())
                ),
            ));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-strategy-direction-proposal-local-post","status":"RECORDED","auth_session_status":{},"proposal_id":{},"direction":{},"proposal_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"ratification_required":true,"ratification_route":"/strategy/ratification-decisions.json","authority_opened":"none","projection_route":"/strategy/direction-proposals/projection.json","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.proposal_id),
            json_string_literal(&direction),
            json_string_literal(&report.proposal_receipt_id),
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
    let proposals: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("strategy.direction.proposed")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"proposal_receipt_id":{},"proposal_id":{},"direction":{},"why_now":{},"problem":{},"where_we_play":{},"how_we_win":{},"horizon":{},"sponsor":{},"success_metric":{},"kill_condition":{},"funding_ask":{},"actor_id":{},"identity_source":{}}}"#,
                json_string_literal(&receipt.receipt_id),
                value("proposal_id"),
                value("direction"),
                value("why_now"),
                value("problem"),
                value("where_we_play"),
                value("how_we_win"),
                value("horizon"),
                value("sponsor"),
                value("success_metric"),
                value("kill_condition"),
                value("funding_ask"),
                json_string_literal(receipt.actor_id.as_str()),
                value("identity_source"),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-strategy-direction-proposal-local-projection","receipt_kind":"strategy.direction.proposed","proposal_count":{},"proposals":[{}],"ratification_route":"/strategy/ratification-decisions.json","authority_opened":"none","production_write_allowed":false}}"#,
            proposals.len(),
            proposals.join(","),
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
