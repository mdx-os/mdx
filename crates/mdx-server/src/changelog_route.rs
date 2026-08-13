// Changelog entries over HTTP: POST records a governed changelog entry
// (title, summary, kind) with identity; GET projects the ledger slice
// newest-first. The receipt is the durable record; projections read the
// kernel ledger by kind. Governed: a changelog entry is a person's or
// agent's verb, recorded with their identity.
// Integration note: needs mod + dispatch in main.rs, two HttpRouteDeclaration entries in http_routes.rs w/ array count +2, POST path in actor_admission.rs GOVERNED_WRITE_ROUTES, smoke body exercising the empty-title refusal.
use crate::RouteResponse;
use mdx_core::{ChangelogEntry, MdxKernel, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/changelog/entries.json" => Some(handle_post(method, body, kernel)),
        "/changelog/projection.json" => Some(handle_projection(method, kernel)),
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
    let title = field("title");
    let summary = field("summary");
    let kind = field("kind");
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = match kernel.record_changelog_entry_with_identity(
        ChangelogEntry {
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            title: &title,
            summary: &summary,
            kind: &kind,
        },
        &resolved.identity,
    ) {
        Ok(report) => report,
        Err(error) => return Ok(refusal(&error.message())),
    };
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-changelog-local-post","status":"RECORDED","auth_session_status":{},"title":{},"summary":{},"kind":{},"entry_receipt_id":{},"policy_decision_id":{},"projection_route":"/changelog/projection.json","production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&title),
            json_string_literal(&summary),
            json_string_literal(&kind),
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
    let entries: Vec<String> = kernel
        .ledger()
        .query()
        .by_kind("changelog.entry.recorded")
        .iter()
        .map(|receipt| {
            let value = |key: &str| {
                json_string_literal(receipt.payload.get(key).map(String::as_str).unwrap_or(""))
            };
            format!(
                r#"{{"entry_receipt_id":{},"title":{},"summary":{},"kind":{},"actor_id":{}}}"#,
                json_string_literal(&receipt.receipt_id),
                value("title"),
                value("summary"),
                value("kind"),
                json_string_literal(receipt.actor_id.as_str()),
            )
        })
        .collect();
    let mut entries = entries;
    entries.reverse();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-changelog-local-projection","receipt_kind":"changelog.entry.recorded","entry_count":{},"entries":[{}],"production_write_allowed":false}}"#,
            entries.len(),
            entries.join(","),
        ),
    ))
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-changelog-local-post","status":"REFUSED","reason":{},"entry_receipt_id":"","production_write_allowed":false}}"#,
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
