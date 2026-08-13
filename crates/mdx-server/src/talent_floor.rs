// The floor's presence, from the chain itself: every loop receipt
// carries its agent's actor id, so "ran today, all green" is a fold over
// records - never a stored status, never decoration. The projection
// groups the ledger by agent actor and serves each worker's latest
// moment and lifetime count.
use crate::RouteResponse;
use mdx_core::{MdxKernel, json_string_literal};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    _body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/talent/floor/projection.json" => Some(handle_projection(method, kernel)),
        _ => None,
    }
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
    // Fold: latest receipt and lifetime count per agent actor. Agent
    // actors are the loop runners' own ids; human and system actors are
    // not the floor's business.
    let mut by_agent: BTreeMap<String, (u64, String, String)> = BTreeMap::new();
    for receipt in kernel.ledger().entries() {
        let actor = receipt.actor_id.as_str();
        if !actor.ends_with("_agent") {
            continue;
        }
        let entry = by_agent
            .entry(actor.to_string())
            .or_insert((0, String::new(), String::new()));
        entry.0 += 1;
        entry.1 = receipt.receipt_id.clone();
        entry.2 = receipt.kind.clone();
    }
    let workers: Vec<String> = by_agent
        .iter()
        .map(|(actor, (count, latest_receipt, latest_kind))| {
            format!(
                r#"{{"agent_id":{},"receipt_count":{},"latest_receipt_id":{},"latest_kind":{}}}"#,
                json_string_literal(actor),
                count,
                json_string_literal(latest_receipt),
                json_string_literal(latest_kind),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-talent-floor-local-projection","worker_count":{},"workers":[{}],"production_write_allowed":false}}"#,
            workers.len(),
            workers.join(","),
        ),
    ))
}
