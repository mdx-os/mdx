use crate::RouteResponse;
use mdx_core::{GovernedWriteIdentity, MdxKernel, StudioDocumentRun, json_string_literal};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == "/studio/runs.json" {
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
    if path == "/studio/runs/projection.json" {
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
    let source_receipt_id = source_receipt_id(kernel)?;
    let run_id =
        json_string_field(body, "run_id").unwrap_or_else(|| "studio_run_local_001".to_string());
    let goal = json_string_field(body, "goal")
        .unwrap_or_else(|| "Draft a document from grounded evidence".to_string());
    let document_id = json_string_field(body, "document_id")
        .unwrap_or_else(|| "page_studio_local_draft".to_string());
    let title =
        json_string_field(body, "title").unwrap_or_else(|| "Studio Local Draft".to_string());
    let revision_id = json_string_field(body, "revision_id")
        .unwrap_or_else(|| "rev_studio_local_001".to_string());
    let artifact_kind = json_string_field(body, "artifact_kind").unwrap_or_default();

    // The artifact kind is validated before any write. An unknown kind is refused
    // outright; the code kind is real but opens in Forge, not Studio, so it gets
    // a distinct, honest redirect rather than a generic not-implemented message.
    let normalized_kind = match mdx_core::normalize_studio_artifact_kind(&artifact_kind) {
        Ok(kind) => kind,
        Err(error) => {
            return Ok(format!(
                r#"{{"name":"mdx-studio-run-local-post","status":"REFUSED","reason":{}}}"#,
                json_string_literal(&error.message())
            ));
        }
    };
    if normalized_kind == "code" {
        return Ok(r#"{"name":"mdx-studio-run-local-post","status":"REFUSED","reason":"code work opens in Forge, not Studio","open_in":"forge"}"#.to_string());
    }

    let body_text = json_string_field(body, "body_text");
    let body_ref = if let Some(text) = body_text.as_deref() {
        match crate::pages_body_store::store_published_body(&document_id, &revision_id, text) {
            Some(stored_ref) => stored_ref,
            None => {
                return Ok(r#"{"name":"mdx-studio-run-local-post","status":"REFUSED","reason":"the body could not be stored - document and revision ids are lowercase letters, digits, underscores, and hyphens"}"#.to_string());
            }
        }
    } else {
        json_string_field(body, "body_ref")
            .unwrap_or_else(|| "world_model://pages/page_studio_local_draft/body/v1".to_string())
    };

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
        .run_studio_document_local_with_identity(
            StudioDocumentRun {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                run_id: &run_id,
                goal: &goal,
                artifact_kind: &artifact_kind,
                document_id: &document_id,
                title: &title,
                body_ref: &body_ref,
                source_receipt_id: &source_receipt_id,
                revision_id: &revision_id,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-studio-run-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/studio/runs.json","projection_route":"/studio/runs/projection.json","run_id":{},"goal":{},"artifact_kind":{},"document_id":{},"title":{},"kind_selection_mode":{},"run_opened_receipt_id":{},"kind_selected_receipt_id":{},"artifact_drafted_receipt_id":{},"publication_receipt_id":{},"run_landed_receipt_id":{},"policy_decision_id":{},"source_receipt_id":{},"revision_id":{},"standalone_document_store_allowed":false,"live_co_edit_allowed":false,"production_write_allowed":false}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.run_id),
        json_string_literal(&report.goal),
        json_string_literal(report.artifact_kind),
        json_string_literal(&report.document_id),
        json_string_literal(&report.title),
        json_string_literal(report.kind_selection_mode),
        json_string_literal(&report.run_opened_receipt_id),
        json_string_literal(&report.kind_selected_receipt_id),
        json_string_literal(&report.artifact_drafted_receipt_id),
        json_string_literal(&report.publication_receipt_id),
        json_string_literal(&report.run_landed_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.source_receipt_id),
        json_string_literal(&report.revision_id)
    ))
}

fn allowed_controls(state: mdx_core::StudioRunState) -> &'static str {
    // The next governed controls the operator may take from this state, as a
    // JSON array body. Drafting can pause or land; paused can steer or resume;
    // a landed run is terminal.
    match state {
        mdx_core::StudioRunState::Drafting => r#""pause","land""#,
        mdx_core::StudioRunState::Paused => r#""steering","resume""#,
        mdx_core::StudioRunState::Landed | mdx_core::StudioRunState::Unknown => "",
    }
}

fn render_local_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let entries = kernel.ledger().entries();
    // One row per run, keyed by its opening receipt. State is derived from the
    // ledger; a landed run reads its title and body through its body-of-truth
    // publication receipt, never a stored copy.
    let runs = entries
        .iter()
        .filter(|receipt| receipt.kind == "studio.run.opened")
        .map(|opened| {
            let run_id = payload_value(opened, "studio_run_id");
            let state = kernel.studio_run_state(run_id);
            let landed = entries.iter().find(|receipt| {
                receipt.kind == "studio.run.landed"
                    && payload_value(receipt, "studio_run_id") == run_id
            });
            let publication_receipt_id =
                landed.map(|receipt| payload_value(receipt, "publication_receipt_id")).unwrap_or("");
            let publication = entries
                .iter()
                .find(|receipt| receipt.receipt_id == publication_receipt_id);
            let title = publication
                .map(|receipt| payload_value(receipt, "title"))
                .unwrap_or("");
            let document_id = landed
                .map(|receipt| payload_value(receipt, "document_id"))
                .unwrap_or("");
            let steering_ids = entries
                .iter()
                .filter(|receipt| {
                    receipt.kind == "studio.run.steering.added"
                        && payload_value(receipt, "studio_run_id") == run_id
                })
                .map(|receipt| json_string_literal(&receipt.receipt_id))
                .collect::<Vec<_>>()
                .join(",");
            // Who is on this run, derived from its receipts in ledger order.
            let present = kernel
                .studio_run_presence(run_id)
                .iter()
                .map(|entry| {
                    format!(
                        r#"{{"actor_id":{},"active":{}}}"#,
                        json_string_literal(&entry.actor_id),
                        entry.active
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                r#"{{"run_id":{},"state":{},"goal":{},"artifact_kind":{},"source_kind":{},"source_title":{},"allowed_controls":[{}],"document_id":{},"title":{},"publication_receipt_id":{},"steering_receipt_ids":[{}],"present":[{}],"origin":"studio"}}"#,
                json_string_literal(run_id),
                json_string_literal(state.as_str()),
                json_string_literal(payload_value(opened, "goal")),
                json_string_literal(payload_value(opened, "artifact_kind")),
                json_string_literal(payload_value(opened, "source_kind")),
                json_string_literal(payload_value(opened, "source_title")),
                allowed_controls(state),
                json_string_literal(document_id),
                json_string_literal(title),
                json_string_literal(publication_receipt_id),
                steering_ids,
                present,
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-studio-run-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/studio/runs.json","pages_projection_route":"/pages/publications/projection.json","run_count":{},"standalone_document_store_allowed":false,"live_co_edit_allowed":false,"production_write_allowed":false,"runs":[{}]}}"#,
        runs.len(),
        runs.join(",")
    ))
}

pub(crate) fn source_receipt_id(kernel: &mut MdxKernel) -> Result<String, String> {
    if let Some(receipt_id) = kernel
        .ledger()
        .entries()
        .first()
        .map(|receipt| receipt.receipt_id.clone())
    {
        return Ok(receipt_id);
    }
    kernel.run_evals_runner_agent()?;
    kernel
        .ledger()
        .entries()
        .first()
        .map(|receipt| receipt.receipt_id.clone())
        .ok_or_else(|| "local Studio run route missing source receipt".to_string())
}

fn payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

pub(crate) fn json_string_field(body: &str, key: &str) -> Option<String> {
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
