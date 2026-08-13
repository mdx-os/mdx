use crate::RouteResponse;
use mdx_core::{GovernedWriteIdentity, MdxKernel, PagesApprovedPublication, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if let Some(publication_receipt_id) = publication_body_receipt_id(path) {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_publication_body_json(&kernel, publication_receipt_id),
        )));
    }
    if path == "/pages/publications.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        // The publication write happens under the lock; the memory
        // consolidation stage runs after the lock drops so a model-backed
        // extraction can never block or fail the publish.
        let result = {
            let mut kernel = match kernel.write() {
                Ok(kernel) => kernel,
                Err(_) => return Some(Err("kernel lock poisoned".to_string())),
            };
            render_local_post_json(body, &mut kernel)
        };
        return Some(result.map(|(body, memory_intake)| {
            if let Some(intake) = memory_intake {
                crate::memory_extraction::consolidate_surface_event(kernel, intake);
            }
            RouteResponse::json("200 OK", body)
        }));
    }
    if path == "/pages/publications/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let kernel = match kernel.read() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_local_projection_json(&kernel).map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    None
}

fn publication_body_receipt_id(path: &str) -> Option<&str> {
    path.strip_prefix("/pages/publications/")?
        .strip_suffix("/body.json")
        .filter(|receipt_id| !receipt_id.is_empty() && !receipt_id.contains('/'))
}

fn render_publication_body_json(kernel: &MdxKernel, publication_receipt_id: &str) -> String {
    let Some(receipt) = kernel
        .ledger()
        .query()
        .by_id(publication_receipt_id)
        .filter(|receipt| receipt.kind == "pages.document.published")
    else {
        return format!(
            r#"{{"name":"mdx-pages-publication-body","status":"REFUSED","reason":"the named publication receipt does not exist","publication_receipt_id":{}}}"#,
            json_string_literal(publication_receipt_id)
        );
    };
    let body_ref = payload_value(receipt, "body_ref");
    let Some(body) = crate::pages_body_store::read_stored_body(body_ref) else {
        return format!(
            r#"{{"name":"mdx-pages-publication-body","status":"REFUSED","reason":"the exact historical body is not available in the local body store","publication_receipt_id":{},"body_ref":{}}}"#,
            json_string_literal(publication_receipt_id),
            json_string_literal(body_ref)
        );
    };
    format!(
        r#"{{"name":"mdx-pages-publication-body","status":"OK","publication_receipt_id":{},"document_id":{},"title":{},"body_ref":{},"revision_id":{},"body":{},"read_only":true,"publication_allowed":false,"production_write_allowed":false}}"#,
        json_string_literal(publication_receipt_id),
        json_string_literal(payload_value(receipt, "document_id")),
        json_string_literal(payload_value(receipt, "title")),
        json_string_literal(body_ref),
        json_string_literal(payload_value(receipt, "revision_id")),
        json_string_literal(&body)
    )
}

fn render_local_post_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<
    (
        String,
        Option<crate::memory_extraction::SurfaceMemoryIntake>,
    ),
    String,
> {
    let document_id = json_string_field(body, "document_id")
        .unwrap_or_else(|| "page_local_operator_note".to_string());
    let approval_decision_receipt_id =
        json_string_field(body, "approval_decision_receipt_id").unwrap_or_default();
    let page_type = json_string_field(body, "page_type").unwrap_or_default();
    if mdx_core::normalize_page_type(&page_type).is_err() {
        let reason = format!(
            "the page type {page_type} is not one of {}",
            mdx_core::PAGE_TYPES.join(", ")
        );
        return Ok((
            format!(
                r#"{{"name":"mdx-pages-publication-local-post","status":"REFUSED","reason":{}}}"#,
                json_string_literal(&reason)
            ),
            None,
        ));
    }
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let verified = resolved.auth_session_status == "VERIFIED_TRUSTED_SESSION";
    let tenant_id = if verified {
        resolved.tenant_id.clone()
    } else {
        json_string_field(body, "tenant_id").unwrap_or_else(|| "local_tenant".to_string())
    };
    let actor_id = if verified {
        resolved.actor_id.clone()
    } else {
        json_string_field(body, "actor_id").unwrap_or_else(|| "human:local_user".to_string())
    };
    let identity = if verified {
        resolved.identity.clone()
    } else {
        GovernedWriteIdentity::local_demo(&actor_id)
    };
    let report = match kernel.save_pages_approved_publication_local_with_identity(
        PagesApprovedPublication {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            document_id: &document_id,
            approval_decision_receipt_id: &approval_decision_receipt_id,
            page_type: &page_type,
        },
        &identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok((
                format!(
                    r#"{{"name":"mdx-pages-publication-local-post","status":"REFUSED","reason":{},"document_id":{},"approval_decision_receipt_id":{},"approval_binding":"required_approved_draft_content","production_write_allowed":false}}"#,
                    json_string_literal(&error.message()),
                    json_string_literal(&document_id),
                    json_string_literal(&approval_decision_receipt_id),
                ),
                None,
            ));
        }
    };
    // Consolidation happens after the route drops the kernel lock; the
    // extraction stage may only use a boundary-locked model for Pages
    // content and falls back to verbatim if none is configured.
    let memory_intake = crate::memory_extraction::SurfaceMemoryIntake {
        tenant_id: tenant_id.clone(),
        actor_id: actor_id.clone(),
        source_surface: "pages",
        memory_scope: "company_memory",
        source_receipt_id: report.publication_receipt_id.clone(),
        content: format!(
            "Published page {} titled {}",
            report.document_id, report.title
        ),
    };
    let rendered = format!(
        r#"{{"name":"mdx-pages-publication-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/pages/publications.json","projection_route":"/pages/publications/projection.json","document_id":{},"title":{},"body_ref":{},"page_type":{},"publication_receipt_id":{},"policy_decision_id":{},"source_receipt_id":{},"source_edit_draft_receipt_id":{},"origin_receipt_id":{},"origin_surface":{},"origin_grants_authority":false,"approval_decision_receipt_id":{},"approval_binding":{},"reviewer_independence":{},"human_approval_granted":true,"revision_id":{},"visibility":{},"standalone_store_allowed":{},"rich_editor_allowed":{},"live_substrate_required":false,"production_write_allowed":false}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.document_id),
        json_string_literal(&report.title),
        json_string_literal(&report.body_ref),
        json_string_literal(report.page_type),
        json_string_literal(&report.publication_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.source_receipt_id),
        json_string_literal(&report.source_edit_draft_receipt_id),
        json_string_literal(&report.origin_receipt_id),
        json_string_literal(&report.origin_surface),
        json_string_literal(&report.approval_decision_receipt_id),
        json_string_literal(report.approval_binding),
        json_string_literal(report.reviewer_independence),
        json_string_literal(&report.revision_id),
        json_string_literal(report.visibility),
        report.standalone_store_allowed,
        report.rich_editor_allowed
    );
    Ok((rendered, Some(memory_intake)))
}

fn render_local_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let publications = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "pages.document.published")
        .map(|receipt| {
            format!(
                r#"{{"document_id":{},"title":{},"page_type":{},"body_ref":{},"publication_receipt_id":{},"policy_decision_id":{},"author_actor_id":{},"source_receipt_id":{},"source_edit_draft_receipt_id":{},"origin_receipt_id":{},"origin_surface":{},"origin_grants_authority":false,"approval_decision_receipt_id":{},"approval_binding":{},"reviewer_independence":{},"human_approval_granted":{},"revision_id":{},"visibility":{},"world_model_source":{},"standalone_store_allowed":false,"rich_editor_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "document_id")),
                json_string_literal(payload_value(receipt, "title")),
                json_string_literal(page_type_value(receipt)),
                json_string_literal(payload_value(receipt, "body_ref")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(receipt.actor_id.as_str()),
                json_string_literal(payload_value(receipt, "source_receipt_id")),
                json_string_literal(payload_value(receipt, "source_edit_draft_receipt_id")),
                json_string_literal(payload_value(receipt, "origin_receipt_id")),
                json_string_literal(payload_value(receipt, "origin_surface")),
                json_string_literal(payload_value(receipt, "approval_decision_receipt_id")),
                json_string_literal(payload_value(receipt, "approval_binding")),
                json_string_literal(payload_value(receipt, "reviewer_independence")),
                payload_value(receipt, "human_approval_granted") == "true",
                json_string_literal(payload_value(receipt, "revision_id")),
                json_string_literal(payload_value(receipt, "visibility")),
                json_string_literal(payload_value(receipt, "world_model_source"))
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-pages-publication-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/pages/publications.json","publication_count":{},"standalone_store_allowed":false,"rich_editor_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"publications":[{}]}}"#,
        publications.len(),
        publications.join(",")
    ))
}

fn payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

// A publication's declared kind, defaulting pages published before type was
// first-class to `knowledge` - the honest read of an untyped document.
fn page_type_value(receipt: &mdx_core::Receipt) -> &str {
    let kind = payload_value(receipt, "page_type");
    if kind.is_empty() { "knowledge" } else { kind }
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let after_key = body.split(&format!("\"{key}\"")).nth(1)?;
    let after_colon = after_key.split_once(':')?.1.trim_start();
    let value = after_colon.strip_prefix('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for character in value.chars() {
        if escaped {
            out.push(match character {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            return Some(out);
        } else {
            out.push(character);
        }
    }
    None
}
