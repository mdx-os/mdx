// "Our direction" over HTTP: POST records a new version of the standing
// five-box record; the projection serves every version newest-first plus
// which boxes changed between versions, so the surface renders the diff
// without re-deriving it.
use crate::RouteResponse;
use mdx_core::{MdxKernel, StrategyDirectionRecord, json_string_literal};
use std::sync::{Arc, RwLock};

const BOXES: &[&str] = &[
    "winning",
    "where_we_play",
    "how_we_win",
    "great_at",
    "counting_on",
];

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/strategy/our-direction.json" => Some(handle_post(method, body, kernel)),
        "/strategy/our-direction/projection.json" => Some(handle_projection(method, kernel)),
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
    let report = match kernel.save_strategy_direction_record_local_with_identity(
        StrategyDirectionRecord {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            winning: &field("winning"),
            where_we_play: &field("where_we_play"),
            how_we_win: &field("how_we_win"),
            great_at: &field("great_at"),
            counting_on: &field("counting_on"),
            amended_by: &field("amended_by"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(RouteResponse::json(
                "200 OK",
                format!(
                    r#"{{"name":"mdx-strategy-our-direction-local-post","status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
                    json_string_literal(&error.message())
                ),
            ));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-strategy-our-direction-local-post","status":"RECORDED","auth_session_status":{},"version":{},"record_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"projection_route":"/strategy/our-direction/projection.json","authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            report.version,
            json_string_literal(&report.record_receipt_id),
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
    let records = kernel
        .ledger()
        .query()
        .by_kind("strategy.direction.recorded");
    let mut versions: Vec<String> = Vec::new();
    for (index, receipt) in records.iter().enumerate() {
        let value = |key: &str| receipt.payload.get(key).map(String::as_str).unwrap_or("");
        // Which boxes changed vs the previous version - served, not
        // re-derived by every reader.
        let changed: Vec<String> = if index == 0 {
            BOXES.iter().map(|b| json_string_literal(b)).collect()
        } else {
            let previous = &records[index - 1];
            BOXES
                .iter()
                .filter(|key| {
                    previous
                        .payload
                        .get(**key)
                        .map(String::as_str)
                        .unwrap_or("")
                        != value(key)
                })
                .map(|b| json_string_literal(b))
                .collect()
        };
        versions.push(format!(
            r#"{{"version":{},"record_receipt_id":{},"winning":{},"where_we_play":{},"how_we_win":{},"great_at":{},"counting_on":{},"amended_by":{},"changed":[{}],"actor_id":{}}}"#,
            value("version"),
            json_string_literal(&receipt.receipt_id),
            json_string_literal(value("winning")),
            json_string_literal(value("where_we_play")),
            json_string_literal(value("how_we_win")),
            json_string_literal(value("great_at")),
            json_string_literal(value("counting_on")),
            json_string_literal(value("amended_by")),
            changed.join(","),
            json_string_literal(receipt.actor_id.as_str()),
        ));
    }
    versions.reverse();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-strategy-our-direction-local-projection","receipt_kind":"strategy.direction.recorded","version_count":{},"versions":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
            versions.len(),
            versions.join(","),
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
