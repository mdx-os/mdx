// One inbox over everything waiting on a human. The audit found
// waiting-on-you fragmented across five surfaces; this projection folds
// the record-backed reasons into one reason-tagged list. Each reason is
// a fold over receipts - nothing here is stored state:
//
// - page_approval: a pages approval requested with no decision recorded
// - talent_signature: a worker authority request with no authorization
//   linked back to its receipt
// - work_review: a work item whose latest move is in_review
// - triage_open: a triage entry (posted, or a feedback capture) with no
//   verdict
// - message_action: a message action request with no recorded verdict
//
// The stage-derived reasons (a bet ready to back, a direction ready to
// ratify) stay client-derived where their stage logic already lives -
// one derivation per truth, never two.
use crate::RouteResponse;
use mdx_core::{MdxKernel, Receipt, json_string_literal};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

pub(crate) const NEEDS_YOU_REASONS: &[&str] = &[
    "page_approval",
    "talent_signature",
    "work_review",
    "triage_open",
    "message_action",
    "forge_wide_plan_review",
    "forge_ratify_plan",
    "forge_fleet_repair_escalation",
    "forge_ship_call",
];

pub(crate) fn route_response(
    method: &str,
    path: &str,
    _body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/product/needs-you/projection.json" => Some(handle_projection(method, kernel)),
        _ => None,
    }
}

fn payload_value<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn item_json(
    surface: &str,
    reason: &str,
    reference: &str,
    detail: &str,
    route: &str,
    receipt_id: &str,
) -> String {
    format!(
        r#"{{"surface":{},"reason":{},"ref":{},"title":{},"detail":{},"href":{},"route":{},"receipt_id":{}}}"#,
        json_string_literal(surface),
        json_string_literal(reason),
        json_string_literal(reference),
        json_string_literal(detail),
        json_string_literal(detail),
        json_string_literal(route),
        json_string_literal(route),
        json_string_literal(receipt_id),
    )
}

fn add_attention_item(
    items: &mut Vec<String>,
    counts: &mut BTreeMap<String, usize>,
    surface_counts: &mut BTreeMap<String, usize>,
    surface: &str,
    reason: &str,
    json: String,
) {
    *counts.entry(reason.to_string()).or_insert(0) += 1;
    *surface_counts.entry(surface.to_string()).or_insert(0) += 1;
    items.push(json);
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
    let entries = kernel.ledger().entries();
    let mut items: Vec<String> = Vec::new();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut surface_counts: BTreeMap<String, usize> = BTreeMap::new();
    for reason in NEEDS_YOU_REASONS {
        counts.insert((*reason).to_string(), 0);
    }

    // A pages approval request waits until a decision names its receipt.
    for receipt in entries
        .iter()
        .filter(|receipt| receipt.kind == "pages.approval.requested")
    {
        let decided = entries.iter().any(|decision| {
            decision.kind == "pages.approval.decision.recorded"
                && payload_value(decision, "approval_request_receipt_id") == receipt.receipt_id
        });
        if !decided {
            add_attention_item(
                &mut items,
                &mut counts,
                &mut surface_counts,
                "pages",
                "page_approval",
                item_json(
                    "pages",
                    "page_approval",
                    payload_value(receipt, "document_id"),
                    payload_value(receipt, "document_id"),
                    "/pages",
                    &receipt.receipt_id,
                ),
            );
        }
    }

    // A work item whose latest move is in_review waits for acceptance.
    // Fold shapes to latest per item for the title, then take the last
    // move's status.
    let mut latest_shape: BTreeMap<String, (&Receipt, String)> = BTreeMap::new();
    for receipt in entries
        .iter()
        .filter(|receipt| receipt.kind == "work.item.shaped")
    {
        let work_item_id = payload_value(receipt, "work_item_id").to_string();
        if work_item_id.is_empty() {
            continue;
        }
        latest_shape.insert(
            work_item_id,
            (receipt, payload_value(receipt, "title").to_string()),
        );
    }
    for (work_item_id, (shape, title)) in &latest_shape {
        let status = entries
            .iter()
            .rev()
            .find(|receipt| {
                receipt.kind == "work.item.moved"
                    && payload_value(receipt, "work_item_id") == work_item_id
            })
            .map(|receipt| payload_value(receipt, "status"))
            .unwrap_or("intake");
        if status == "in_review" {
            add_attention_item(
                &mut items,
                &mut counts,
                &mut surface_counts,
                "product",
                "work_review",
                item_json(
                    "product",
                    "work_review",
                    work_item_id,
                    title,
                    "/product-direction",
                    &shape.receipt_id,
                ),
            );
        }
    }

    // A triage entry waits until a verdict names it - posted entries and
    // feedback captures alike.
    for receipt in entries.iter() {
        let (entry_ref, detail) = match receipt.kind.as_str() {
            "triage.entry.recorded" => (
                payload_value(receipt, "entry_id").to_string(),
                payload_value(receipt, "text").to_string(),
            ),
            "beta.feedback.captured" => (
                receipt.receipt_id.clone(),
                payload_value(receipt, "note").to_string(),
            ),
            _ => continue,
        };
        if entry_ref.is_empty() {
            continue;
        }
        let judged = entries.iter().any(|verdict| {
            verdict.kind == "triage.verdict.recorded"
                && payload_value(verdict, "entry_ref") == entry_ref
        });
        if !judged {
            add_attention_item(
                &mut items,
                &mut counts,
                &mut surface_counts,
                "product",
                "triage_open",
                item_json(
                    "product",
                    "triage_open",
                    &entry_ref,
                    &detail,
                    "/product-direction",
                    &receipt.receipt_id,
                ),
            );
        }
    }

    for receipt in entries
        .iter()
        .filter(|receipt| receipt.kind == "message.action.requested")
    {
        let decided = entries.iter().any(|verdict| {
            verdict.kind == "message.action.verdict.recorded"
                && payload_value(verdict, "action_request_receipt_id") == receipt.receipt_id
        });
        if !decided {
            add_attention_item(
                &mut items,
                &mut counts,
                &mut surface_counts,
                "message",
                "message_action",
                item_json(
                    "message",
                    "message_action",
                    payload_value(receipt, "action_request_id"),
                    payload_value(receipt, "title"),
                    "/message",
                    &receipt.receipt_id,
                ),
            );
        }
    }

    for forge_item in crate::forge_control_plane_projection::forge_needs_you_items(&kernel) {
        let reason = format!("forge_{}", forge_item.kind);
        add_attention_item(
            &mut items,
            &mut counts,
            &mut surface_counts,
            "forge",
            &reason,
            item_json(
                "forge",
                &reason,
                &forge_item.subject,
                &forge_item.line,
                &forge_item.href,
                "",
            ),
        );
    }

    let reason_counts = counts
        .iter()
        .map(|(reason, count)| format!("{}:{count}", json_string_literal(reason)))
        .collect::<Vec<_>>()
        .join(",");
    let surface_counts = surface_counts
        .iter()
        .map(|(surface, count)| format!("{}:{count}", json_string_literal(surface)))
        .collect::<Vec<_>>()
        .join(",");
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-needs-you-local-projection","total_count":{},"reason_counts":{{{}}},"surface_counts":{{{}}},"items":[{}],"reasons":[{}],"production_write_allowed":false}}"#,
            items.len(),
            reason_counts,
            surface_counts,
            items.join(","),
            NEEDS_YOU_REASONS
                .iter()
                .map(|reason| json_string_literal(reason))
                .collect::<Vec<_>>()
                .join(","),
        ),
    ))
}
