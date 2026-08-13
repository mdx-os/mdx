// The Message bridge to Slack and Teams over HTTP. POST /messages/bridge-posts
// queues an outbound post as a governed receipt (recorded, never delivered -
// live transport is gated). GET /messages/bridge/projection.json reports the
// bridge status: which outside systems are connected (inbound items arriving
// through the connector rail + outbound posts queued), the counts, and the
// blocked authority that keeps delivery off until the provider gate opens.
// Inbound is the connector rail: a Slack/Teams message is ingested through
// /connectors/items.json with source_kind slack|teams (origin=external).
// Integration: mod + dispatch in main.rs, two HttpRouteDeclaration entries in
// mdx-core http_routes.rs (+2 count), one POST in GOVERNED_WRITE_ROUTES, one
// ActionKind, a smoke body, regenerate route/openapi manifests.
use crate::RouteResponse;
use mdx_core::{BridgePost, MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/messages/bridge-posts.json" => Some(handle_post(method, body, kernel)),
        "/messages/bridge/projection.json" => Some(handle_projection(method, kernel)),
        _ => None,
    }
}

const BRIDGE_KINDS: &[&str] = &["slack", "teams"];

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
    let target_kind = field("target_kind");
    let target_channel = field("target_channel");
    let post_body = field("body");
    let source_message_receipt_id = field("source_message_receipt_id");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.record_message_bridge_post_with_identity(
        BridgePost {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            target_kind: &target_kind,
            target_channel: &target_channel,
            body: &post_body,
            source_message_receipt_id: &source_message_receipt_id,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-message-bridge-post-local-post","status":"QUEUED","auth_session_status":{},"target_kind":{},"target_channel":{},"bridge_receipt_id":{},"policy_decision_id":{},"production_delivery_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(report.target_kind),
            json_string_literal(&target_channel),
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

    // Inbound: Slack/Teams content ingested through the connector rail.
    let inbound: Vec<&mdx_core::Receipt> = kernel
        .ledger()
        .query()
        .by_kind("connector.item.ingested")
        .into_iter()
        .filter(|receipt| BRIDGE_KINDS.contains(&pv(receipt, "source_kind")))
        .collect();
    // Outbound: posts queued toward Slack/Teams (recorded, not delivered).
    let outbound = kernel
        .ledger()
        .query()
        .by_kind(mdx_core::MESSAGE_BRIDGE_POST_KIND);

    // Per-system rollup across both directions.
    let mut systems: Vec<String> = Vec::new();
    for kind in BRIDGE_KINDS {
        let inbound_count = inbound
            .iter()
            .filter(|receipt| pv(receipt, "source_kind") == *kind)
            .count();
        let outbound_count = outbound
            .iter()
            .filter(|receipt| pv(receipt, "target_kind") == *kind)
            .count();
        systems.push(format!(
            r#"{{"system":{},"connected":{},"inbound_count":{},"outbound_queued_count":{}}}"#,
            json_string_literal(kind),
            inbound_count + outbound_count > 0,
            inbound_count,
            outbound_count,
        ));
    }

    let recent_outbound: Vec<String> = outbound
        .iter()
        .rev()
        .take(20)
        .map(|receipt| {
            format!(
                r#"{{"target_kind":{},"target_channel":{},"body":{},"source_message_receipt_id":{},"bridge_receipt_id":{},"delivered":false}}"#,
                json_string_literal(pv(receipt, "target_kind")),
                json_string_literal(pv(receipt, "target_channel")),
                json_string_literal(pv(receipt, "body")),
                json_string_literal(pv(receipt, "source_message_receipt_id")),
                json_string_literal(&receipt.receipt_id),
            )
        })
        .collect();

    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-message-bridge-local-projection","status":"OK","derivation":"the Slack/Teams bridge: inbound items arrive through the connector rail (origin=external), outbound posts are queued as governed receipts; nothing is delivered - live transport is gated","inbound_route":"/connectors/items.json","outbound_route":"/messages/bridge-posts.json","systems":[{}],"inbound_total":{},"outbound_total":{},"recent_outbound":[{}],"blocked_authority":{{"production_delivery_allowed":false,"live_provider_call_allowed":false,"production_cutover_allowed":false}},"live_transport_note":"turning on real delivery needs the suspend-for-external-call provider gate","production_write_allowed":false}}"#,
            systems.join(","),
            inbound.len(),
            outbound.len(),
            recent_outbound.join(","),
        ),
    ))
}

fn pv<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-message-bridge-post-local-post","status":"REFUSED","reason":{},"bridge_receipt_id":"","production_write_allowed":false}}"#,
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
