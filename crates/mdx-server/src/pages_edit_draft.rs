use crate::RouteResponse;
use mdx_core::{GovernedWriteIdentity, MdxKernel, PagesEditDraft, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/pages/edit-drafts.json" {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let mut kernel = match kernel.write() {
            Ok(kernel) => kernel,
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(
            render_local_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/pages/edit-drafts/projection.json" {
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

fn render_local_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let draft_id = json_string_field(body, "draft_id")
        .unwrap_or_else(|| "pages_edit_draft_local_001".to_string());
    let document_id = json_string_field(body, "document_id")
        .unwrap_or_else(|| "page_local_operator_note".to_string());
    let title =
        json_string_field(body, "title").unwrap_or_else(|| "Local Operator Note".to_string());
    // An authored body lands write-once in the local store and the
    // kernel records the reference - the file is the recorded source.
    let body_text = json_string_field(body, "body_text");
    let body_ref = if let Some(text) = body_text.as_deref() {
        match crate::pages_body_store::store_draft_body(&draft_id, text) {
            Some(stored_ref) => stored_ref,
            None => {
                return Ok(r#"{"name":"mdx-pages-edit-draft-local-post","status":"REFUSED","reason":"the draft body could not be stored - draft ids are lowercase letters, digits, underscores, and hyphens"}"#.to_string());
            }
        }
    } else {
        json_string_field(body, "body_ref").unwrap_or_else(|| {
            "world_model://pages/page_local_operator_note/body/draft/v1".to_string()
        })
    };
    let source_publication_receipt_id =
        json_string_field(body, "source_publication_receipt_id").unwrap_or_default();
    let origin_receipt_id = json_string_field(body, "origin_receipt_id").unwrap_or_default();
    let origin_surface = json_string_field(body, "origin_surface").unwrap_or_default();
    let revision_id =
        json_string_field(body, "revision_id").unwrap_or_else(|| "rev_local_draft_001".to_string());
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
    let report = kernel
        .save_pages_edit_draft_local_with_identity(
            PagesEditDraft {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                draft_id: &draft_id,
                document_id: &document_id,
                title: &title,
                body_ref: &body_ref,
                source_publication_receipt_id: &source_publication_receipt_id,
                origin_receipt_id: &origin_receipt_id,
                origin_surface: &origin_surface,
                revision_id: &revision_id,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-pages-edit-draft-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/pages/edit-drafts.json","projection_route":"/pages/edit-drafts/projection.json","draft_id":{},"document_id":{},"edit_draft_receipt_id":{},"policy_decision_id":{},"source_publication_receipt_id":{},"origin_receipt_id":{},"origin_surface":{},"origin_grants_authority":false,"revision_id":{},"rich_editor_allowed":{},"approval_rail_allowed":{},"standalone_store_allowed":{},"live_substrate_required":false,"production_publish_allowed":{},"production_write_allowed":false}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.draft_id),
        json_string_literal(&report.document_id),
        json_string_literal(&report.edit_draft_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.source_publication_receipt_id),
        json_string_literal(&report.origin_receipt_id),
        json_string_literal(&report.origin_surface),
        json_string_literal(&revision_id),
        report.rich_editor_allowed,
        report.approval_rail_allowed,
        report.standalone_store_allowed,
        report.production_publish_allowed
    ))
}

fn render_local_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let drafts = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "pages.edit.draft.saved")
        .map(|receipt| {
            format!(
                r#"{{"draft_id":{},"document_id":{},"title":{},"body_ref":{},"edit_draft_receipt_id":{},"policy_decision_id":{},"source_publication_receipt_id":{},"origin_receipt_id":{},"origin_surface":{},"origin_grants_authority":false,"revision_id":{},"terminal_state":{},"rich_editor_allowed":false,"approval_rail_allowed":false,"standalone_store_allowed":false,"production_publish_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "draft_id")),
                json_string_literal(payload_value(receipt, "document_id")),
                json_string_literal(payload_value(receipt, "title")),
                json_string_literal(payload_value(receipt, "body_ref")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "source_publication_receipt_id")),
                json_string_literal(payload_value(receipt, "origin_receipt_id")),
                json_string_literal(payload_value(receipt, "origin_surface")),
                json_string_literal(payload_value(receipt, "revision_id")),
                json_string_literal(payload_value(receipt, "terminal_state"))
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-pages-edit-draft-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/pages/edit-drafts.json","draft_count":{},"rich_editor_allowed":false,"approval_rail_allowed":false,"standalone_store_allowed":false,"live_substrate_required":false,"production_publish_allowed":false,"production_write_allowed":false,"drafts":[{}]}}"#,
        drafts.len(),
        drafts.join(",")
    ))
}

fn payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
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
