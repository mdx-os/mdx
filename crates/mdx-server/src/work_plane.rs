// The work plane's verbs over HTTP: items shaped and re-shaped, moves
// recorded against the fixed status vocabulary. The projection folds
// shapes to the latest per item and derives the current status from the
// latest move - status is never stored, so it can never silently rot
// (v1's never-written status column is the lesson this rail exists to
// fix). Each item carries its move trail, which is its activity feed.
use crate::RouteResponse;
use mdx_core::{
    MdxKernel, WORK_ITEM_APPROVAL_LEVELS, WORK_ITEM_STATUSES, WorkItemMove, WorkItemShape,
    json_string_literal,
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
        "/product/work-items.json" => Some(handle_item_post(method, body, kernel)),
        "/product/work-items/projection.json" => Some(handle_items_projection(method, kernel)),
        "/product/work-item-moves.json" => Some(handle_move_post(method, body, kernel)),
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

fn handle_item_post(
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
    let report = match kernel.save_work_item_local_with_identity(
        WorkItemShape {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            work_item_id: &field("work_item_id"),
            title: &field("title"),
            description: &field("description"),
            bet_id: &field("bet_id"),
            owner_id: &field("owner_id"),
            approval: &field("approval"),
            page_ref: &field("page_ref"),
            build_ref: &field("build_ref"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(refusal("mdx-work-item-local-post", &error.message()));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-work-item-local-post","status":"RECORDED","auth_session_status":{},"work_item_id":{},"item_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"move_route":"/product/work-item-moves.json","projection_route":"/product/work-items/projection.json","allowed_approvals":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.record_id),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(report.terminal_state),
            WORK_ITEM_APPROVAL_LEVELS
                .iter()
                .map(|s| json_string_literal(s))
                .collect::<Vec<_>>()
                .join(","),
        ),
    ))
}

fn handle_move_post(
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
    let work_item_id = field("work_item_id");
    let status = field("status");
    let report = match kernel.save_work_item_move_local_with_identity(
        WorkItemMove {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            work_item_id: &work_item_id,
            status: &status,
            note: &field("note"),
            deliverable_ref: &field("deliverable_ref"),
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(refusal("mdx-work-item-move-local-post", &error.message()));
        }
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-work-item-move-local-post","status":"RECORDED","auth_session_status":{},"move_id":{},"work_item_id":{},"item_status":{},"move_receipt_id":{},"policy_decision_id":{},"terminal_state":{},"allowed_statuses":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.record_id),
            json_string_literal(&work_item_id),
            json_string_literal(&status),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(report.terminal_state),
            WORK_ITEM_STATUSES
                .iter()
                .map(|s| json_string_literal(s))
                .collect::<Vec<_>>()
                .join(","),
        ),
    ))
}

fn handle_items_projection(
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
    // Fold re-shapes to the latest per item: later receipts overwrite
    // earlier fields wholesale (each shape carries the full anatomy).
    let mut latest: BTreeMap<String, (String, BTreeMap<String, String>, String)> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    for receipt in ledger.query().by_kind("work.item.shaped").iter() {
        let work_item_id = receipt
            .payload
            .get("work_item_id")
            .cloned()
            .unwrap_or_default();
        if work_item_id.is_empty() {
            continue;
        }
        if !latest.contains_key(&work_item_id) {
            order.push(work_item_id.clone());
        }
        latest.insert(
            work_item_id.clone(),
            (
                receipt.receipt_id.clone(),
                receipt.payload.clone(),
                receipt.actor_id.as_str().to_string(),
            ),
        );
    }
    let moves = ledger.query().by_kind("work.item.moved");
    let items: Vec<String> = order
        .iter()
        .map(|work_item_id| {
            let (receipt_id, fields, actor) = &latest[work_item_id];
            let value = |key: &str| {
                json_string_literal(fields.get(key).map(String::as_str).unwrap_or(""))
            };
            // The current status is the latest move; an unmoved item is
            // in intake. Derived every read, stored nowhere.
            let mut status = "intake".to_string();
            let trail: Vec<String> = moves
                .iter()
                .filter(|entry| {
                    entry.payload.get("work_item_id").map(String::as_str)
                        == Some(work_item_id.as_str())
                })
                .map(|entry| {
                    let mv = |key: &str| entry.payload.get(key).map(String::as_str).unwrap_or("");
                    status = mv("status").to_string();
                    format!(
                        r#"{{"move_receipt_id":{},"status":{},"note":{},"deliverable_ref":{},"actor_id":{}}}"#,
                        json_string_literal(&entry.receipt_id),
                        json_string_literal(mv("status")),
                        json_string_literal(mv("note")),
                        json_string_literal(mv("deliverable_ref")),
                        json_string_literal(entry.actor_id.as_str()),
                    )
                })
                .collect();
            format!(
                r#"{{"work_item_id":{},"item_receipt_id":{},"title":{},"description":{},"bet_id":{},"owner_id":{},"approval":{},"page_ref":{},"build_ref":{},"status":{},"actor_id":{},"moves":[{}]}}"#,
                json_string_literal(work_item_id),
                json_string_literal(receipt_id),
                value("title"),
                value("description"),
                value("bet_id"),
                value("owner_id"),
                value("approval"),
                value("page_ref"),
                value("build_ref"),
                json_string_literal(&status),
                json_string_literal(actor),
                trail.join(","),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-work-item-local-projection","receipt_kind":"work.item.shaped","item_count":{},"items":[{}],"allowed_statuses":[{}],"allowed_approvals":[{}],"authority_opened":"none","production_write_allowed":false}}"#,
            items.len(),
            items.join(","),
            WORK_ITEM_STATUSES
                .iter()
                .map(|s| json_string_literal(s))
                .collect::<Vec<_>>()
                .join(","),
            WORK_ITEM_APPROVAL_LEVELS
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
