// The connector rail over HTTP. POST ingests one external item as a governed
// receipt with identity; GET projects the ingested items as the company's
// EXTERNAL knowledge - grouped by source, every node marked origin=external
// with its sensitivity, handling, and grade. This is the graph's third origin
// (alongside derived and asserted): content from outside MDx, always traceable
// to the connector that brought it in, never trusted as internal truth.
// Integration note: needs mod + dispatch in main.rs, two HttpRouteDeclaration
// entries in http_routes.rs w/ array count +2, POST path in actor_admission.rs
// GOVERNED_WRITE_ROUTES, smoke body exercising the sovereign refusal.
use crate::RouteResponse;
use mdx_core::{ExternalItem, ExternalItemDeletion, MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/connectors/items.json" => Some(handle_post(method, body, kernel)),
        "/connectors/items/deletions.json" => Some(handle_delete(method, body, kernel)),
        "/connectors/projection.json" => Some(handle_projection(method, kernel)),
        "/connectors/registry.json" => Some(handle_registry(method, kernel)),
        "/connectors/health.json" => Some(handle_health(method, kernel)),
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
    let source_id = field("source_id");
    let source_kind = field("source_kind");
    let external_ref = field("external_ref");
    let title = field("title");
    let summary = field("summary");
    let body_ref = field("body_ref");
    let data_sensitivity = field("data_sensitivity");
    let handling = field("handling");
    let grade = field("grade");
    let scope = field("scope");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.record_external_item_with_identity(
        ExternalItem {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            source_id: &source_id,
            source_kind: &source_kind,
            external_ref: &external_ref,
            title: &title,
            summary: &summary,
            body_ref: &body_ref,
            data_sensitivity: &data_sensitivity,
            handling: &handling,
            grade: &grade,
            scope: &scope,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-connector-item-local-post","status":"INGESTED","auth_session_status":{},"origin":"external","source_id":{},"source_kind":{},"external_ref":{},"title":{},"data_sensitivity":{},"handling":{},"grade":{},"scope":{},"item_receipt_id":{},"policy_decision_id":{},"projection_route":"/connectors/projection.json","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&source_id),
            json_string_literal(&source_kind),
            json_string_literal(&external_ref),
            json_string_literal(&title),
            json_string_literal(report.data_sensitivity),
            json_string_literal(report.handling),
            json_string_literal(report.grade),
            json_string_literal(report.scope),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
        ),
    ))
}

fn handle_delete(
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
    let item_receipt_id = json_string_field(body, "item_receipt_id")
        .or_else(|| json_string_field(body, "source_id"))
        .unwrap_or_default();
    let reason =
        json_string_field(body, "reason").unwrap_or_else(|| "Connector item removed.".to_string());
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.delete_external_item_with_identity(
        ExternalItemDeletion {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            item_receipt_id: &item_receipt_id,
            reason: &reason,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(delete_refusal(&error.message(), &item_receipt_id)),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-connector-item-delete-local-post","status":"DELETED","auth_session_status":{},"item_receipt_id":{},"delete_receipt_id":{},"policy_decision_id":{},"deletion_state":{},"trusted_context_invalidated":true,"projection_route":"/connectors/projection.json","context_projection_route":"/pages/context-sources/projection.json","raw_external_content_recorded":false,"provider_call_allowed":false,"memory_write_performed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.item_receipt_id),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(report.deletion_state),
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
    let all_receipts = kernel.ledger().query().by_kind("connector.item.ingested");
    // The shared feed carries ONLY tenant-scope sources. Personal items belong
    // to their actor's own view (a "You" surface, not built yet) and must never
    // leak into the company-wide projection - so they are filtered out here, and
    // only their COUNT is surfaced, acknowledging they exist without exposing
    // their content. This is the scope promise enforced, not just commented.
    let withheld_personal = all_receipts
        .iter()
        .filter(|receipt| scope_value(receipt) == "personal")
        .count();
    let receipts: Vec<&mdx_core::Receipt> = all_receipts
        .iter()
        .copied()
        .filter(|receipt| scope_value(receipt) != "personal")
        .collect();

    // One node per ingested item, newest first, every one marked external.
    let mut items: Vec<String> = receipts
        .iter()
        .map(|receipt| {
            let delete = latest_delete_receipt(&kernel, &receipt.receipt_id);
            let deletion_state = delete
                .map(|receipt| pv(receipt, "deletion_state"))
                .unwrap_or("ACTIVE");
            format!(
                r#"{{"item_receipt_id":{},"origin":"external","source_id":{},"source_kind":{},"external_ref":{},"title":{},"summary":{},"body_ref":{},"data_sensitivity":{},"handling":{},"grade":{},"scope":{},"deletion_state":{},"delete_receipt_id":{},"ingested_by":{}}}"#,
                json_string_literal(&receipt.receipt_id),
                json_string_literal(pv(receipt, "source_id")),
                json_string_literal(pv(receipt, "source_kind")),
                json_string_literal(pv(receipt, "external_ref")),
                json_string_literal(pv(receipt, "title")),
                json_string_literal(pv(receipt, "summary")),
                json_string_literal(pv(receipt, "body_ref")),
                json_string_literal(pv(receipt, "data_sensitivity")),
                json_string_literal(pv(receipt, "handling")),
                json_string_literal(pv(receipt, "grade")),
                json_string_literal(scope_value(receipt)),
                json_string_literal(deletion_state),
                json_string_literal(delete.map(|receipt| receipt.receipt_id.as_str()).unwrap_or_default()),
                json_string_literal(receipt.actor_id.as_str()),
            )
        })
        .collect();
    items.reverse();

    // Source rollup: per source - kind, scope (whose source it is), item count
    // and how many are sensitive. A legible read of where the company's (and
    // each operator's) external knowledge comes from.
    struct SourceRollup {
        kind: String,
        scope: String,
        total: usize,
        sensitive: usize,
    }
    let mut order: Vec<String> = Vec::new();
    let mut counts: std::collections::HashMap<String, SourceRollup> =
        std::collections::HashMap::new();
    for receipt in &receipts {
        let source_id = pv(receipt, "source_id").to_string();
        let scope = scope_value(receipt).to_string();
        let sensitive = pv(receipt, "data_sensitivity") == "sensitive";
        let kind = pv(receipt, "source_kind").to_string();
        let slot = counts.entry(source_id.clone()).or_insert_with(|| {
            order.push(source_id.clone());
            SourceRollup {
                kind,
                scope,
                total: 0,
                sensitive: 0,
            }
        });
        slot.total += 1;
        if sensitive {
            slot.sensitive += 1;
        }
    }
    let sources: Vec<String> = order
        .iter()
        .map(|source_id| {
            let roll = &counts[source_id];
            format!(
                r#"{{"source_id":{},"source_kind":{},"scope":{},"item_count":{},"sensitive_count":{}}}"#,
                json_string_literal(source_id),
                json_string_literal(&roll.kind),
                json_string_literal(&roll.scope),
                roll.total,
                roll.sensitive,
            )
        })
        .collect();
    // The listed feed is tenant-only; personal items are acknowledged by count.
    let tenant_items = items.len();
    let deleted_item_count = receipts
        .iter()
        .filter(|receipt| latest_delete_receipt(&kernel, &receipt.receipt_id).is_some())
        .count();

    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-connector-local-projection","receipt_kind":"connector.item.ingested","delete_receipt_kind":"connector.item.delete_requested","origin":"external","feed_scope":"tenant","derivation":"every node brought in by a connector and recorded as a receipt - marked external, never trusted as internal truth; this feed is company-wide and lists ONLY tenant sources","scopes":{{"tenant":"company-shared sources, authorized at the admin level - listed here","personal":"one operator's own sources, scoped to them and never listed in this shared feed - count only"}},"declared_not_built":{{"personal_feed":"a per-operator 'You' view of personal sources - this shared projection withholds personal content by design","live_fetch":"the live API pull lands behind the suspend-for-external-call gate; this slice governs the shape of ingest, not the transport","source_authorization":"a registry that authorizes which sources may ingest lands next","external_to_page_edges":"citing an external item from a page joins the world model when pages carry external refs"}},"source_count":{},"item_count":{},"tenant_item_count":{},"deleted_item_count":{},"personal_item_count_withheld":{},"sources":[{}],"items":[{}],"production_write_allowed":false}}"#,
            sources.len(),
            items.len(),
            tenant_items,
            deleted_item_count,
            withheld_personal,
            sources.join(","),
            items.join(","),
        ),
    ))
}

fn handle_registry(method: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let tenant_items = kernel
        .ledger()
        .query()
        .by_kind("connector.item.ingested")
        .iter()
        .filter(|receipt| scope_value(receipt) == "tenant")
        .count();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-connector-registry","status":"OK","route":"/connectors/registry.json","projection_route":"/connectors/projection.json","health_route":"/connectors/health.json","context_projection_route":"/pages/context-sources/projection.json","registry_authority":"local_static_beta_registry","receipt_basis":"connector.item.ingested receipts plus local static beta registry","permission_mirroring_required":true,"broad_enterprise_connectors_blocked_until_proven":true,"tenant_item_count":{},"connectors":[{{"connector_id":"github","label":"GitHub","posture":"link_only","proof_state":"LOCAL_PROOF","source_kinds":["github"],"sync_supported":false,"federated_supported":false,"live_fetch_allowed":false,"permission_model":"tenant_admin_authorized_reference","delete_propagation":"connector.item.delete_requested -> Context trusted view fail closed","visibility":"tenant_only"}},{{"connector_id":"web","label":"Web link","posture":"link_only","proof_state":"LOCAL_PROOF","source_kinds":["web"],"sync_supported":false,"federated_supported":false,"live_fetch_allowed":false,"permission_model":"tenant_admin_authorized_reference","delete_propagation":"connector.item.delete_requested -> Context trusted view fail closed","visibility":"tenant_only"}},{{"connector_id":"sharepoint","label":"SharePoint","posture":"blocked","proof_state":"BLOCKED_UNTIL_PERMISSION_MIRRORING_PROOF","source_kinds":[],"sync_supported":false,"federated_supported":false,"live_fetch_allowed":false,"permission_model":"not_proven","delete_propagation":"not_proven","visibility":"blocked"}},{{"connector_id":"drive","label":"Google Drive","posture":"blocked","proof_state":"BLOCKED_UNTIL_PERMISSION_MIRRORING_PROOF","source_kinds":[],"sync_supported":false,"federated_supported":false,"live_fetch_allowed":false,"permission_model":"not_proven","delete_propagation":"not_proven","visibility":"blocked"}}],"raw_external_content_recorded":false,"provider_call_allowed":false,"memory_write_performed":false,"production_write_allowed":false}}"#,
            tenant_items,
        ),
    ))
}

fn handle_health(method: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let receipts = kernel.ledger().query().by_kind("connector.item.ingested");
    let tenant_items = receipts
        .iter()
        .filter(|receipt| scope_value(receipt) == "tenant")
        .count();
    let personal_items = receipts
        .iter()
        .filter(|receipt| scope_value(receipt) == "personal")
        .count();
    let deleted_items = receipts
        .iter()
        .filter(|receipt| latest_delete_receipt(&kernel, &receipt.receipt_id).is_some())
        .count();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-connector-health","status":"OK","route":"/connectors/health.json","registry_route":"/connectors/registry.json","context_projection_route":"/pages/context-sources/projection.json","health_authority":"connector_item_receipts","tenant_item_count":{},"personal_item_count_withheld":{},"deleted_item_count":{},"freshness_authority":"receipt_timestamp","permission_mirroring_state":"LOCAL_LINK_ONLY","delete_propagation_state":"FAIL_CLOSED_BY_RECEIPT","broad_enterprise_connectors_blocked_until_proven":true,"connectors":[{{"connector_id":"github","health":"READY_FOR_LINK_ONLY_PROOF","last_sync_state":"manual_link_reference_only","sync_mode":"link_only","federated_mode":false}},{{"connector_id":"web","health":"READY_FOR_LINK_ONLY_PROOF","last_sync_state":"manual_link_reference_only","sync_mode":"link_only","federated_mode":false}},{{"connector_id":"sharepoint","health":"BLOCKED","last_sync_state":"permission_mirroring_not_proven","sync_mode":"blocked","federated_mode":false}},{{"connector_id":"drive","health":"BLOCKED","last_sync_state":"permission_mirroring_not_proven","sync_mode":"blocked","federated_mode":false}}],"raw_external_content_recorded":false,"provider_call_allowed":false,"memory_write_performed":false,"production_write_allowed":false}}"#,
            tenant_items, personal_items, deleted_items,
        ),
    ))
}

fn pv<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

// An item's scope, defaulting receipts ingested before scope was first-class
// to `tenant` - the honest read of a source brought in as company-shared.
fn scope_value(receipt: &mdx_core::Receipt) -> &str {
    let scope = receipt
        .payload
        .get("scope")
        .map(String::as_str)
        .unwrap_or("");
    if scope.is_empty() { "tenant" } else { scope }
}

fn latest_delete_receipt<'a>(
    kernel: &'a MdxKernel,
    item_receipt_id: &str,
) -> Option<&'a mdx_core::Receipt> {
    kernel.ledger().entries().iter().rev().find(|receipt| {
        receipt.kind == "connector.item.delete_requested"
            && pv(receipt, "item_receipt_id") == item_receipt_id
    })
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-connector-item-local-post","status":"REFUSED","reason":{},"item_receipt_id":"","production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

fn delete_refusal(reason: &str, item_receipt_id: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-connector-item-delete-local-post","status":"REFUSED","reason":{},"item_receipt_id":{},"delete_receipt_id":"","production_write_allowed":false}}"#,
            json_string_literal(reason),
            json_string_literal(item_receipt_id),
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
