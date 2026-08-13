use crate::RouteResponse;
use mdx_core::{
    ExternalItem, GovernedWriteIdentity, MdxKernel, PagesContextSourceTrustDecision,
    TwinArtifactContextRequest, json_string_literal,
};
use std::sync::{Arc, RwLock};

const CONTEXT_ADMISSION_ROUTE: &str = "/pages/context-sources.json";
const CONTEXT_SOURCES_PROJECTION_ROUTE: &str = "/pages/context-sources/projection.json";
const CONTEXT_TRUST_ROUTE: &str = "/pages/context-sources/trust-decisions.json";
const CONTEXT_ARTIFACTS_PROJECTION_ROUTE: &str = "/pages/context-artifacts/projection.json";
const CONTEXT_FRESHNESS_PROJECTION_ROUTE: &str = "/pages/context-freshness/projection.json";
const CONTEXT_DELETE_PROOF_ROUTE: &str = "/pages/context-delete-propagation/proof.json";
const REVIEW_CADENCE_DAYS: u16 = 90;

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path == CONTEXT_ADMISSION_ROUTE {
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
            render_source_admission_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == CONTEXT_TRUST_ROUTE {
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
            render_trust_decision_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    match path {
        CONTEXT_SOURCES_PROJECTION_ROUTE
        | CONTEXT_ARTIFACTS_PROJECTION_ROUTE
        | CONTEXT_FRESHNESS_PROJECTION_ROUTE
        | CONTEXT_DELETE_PROOF_ROUTE => {}
        _ => return None,
    }
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
    let body = match path {
        CONTEXT_SOURCES_PROJECTION_ROUTE => render_json(&kernel),
        CONTEXT_ARTIFACTS_PROJECTION_ROUTE => render_artifacts_json(&kernel),
        CONTEXT_FRESHNESS_PROJECTION_ROUTE => render_freshness_json(&kernel),
        CONTEXT_DELETE_PROOF_ROUTE => render_delete_proof_json(&kernel),
        _ => unreachable!(),
    };
    Some(Ok(RouteResponse::json("200 OK", body)))
}

fn payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt
        .payload
        .get(key)
        .map(String::as_str)
        .unwrap_or_default()
}

fn latest_receipt<'a>(
    kernel: &'a MdxKernel,
    kind: &str,
    artifact_id: &str,
) -> Option<&'a mdx_core::Receipt> {
    kernel.ledger().entries().iter().rev().find(|receipt| {
        receipt.kind == kind && payload_value(receipt, "artifact_id") == artifact_id
    })
}

fn latest_trust_decision<'a>(
    kernel: &'a MdxKernel,
    source_id: &str,
) -> Option<&'a mdx_core::Receipt> {
    kernel.ledger().entries().iter().rev().find(|receipt| {
        receipt.kind == "pages.context_source.trust_decision.recorded"
            && (payload_value(receipt, "source_id") == source_id
                || payload_value(receipt, "artifact_id") == source_id)
    })
}

fn first_receipt<'a>(
    kernel: &'a MdxKernel,
    kind: &str,
    artifact_id: &str,
) -> Option<&'a mdx_core::Receipt> {
    kernel.ledger().entries().iter().find(|receipt| {
        receipt.kind == kind && payload_value(receipt, "artifact_id") == artifact_id
    })
}

fn render_json(kernel: &MdxKernel) -> String {
    let sources = context_source_rows(kernel);
    let review_queue_count = sources
        .iter()
        .filter(|source| source["review_queue_state"] == "needs_review")
        .count();
    let deleted_count = sources
        .iter()
        .filter(|source| source["deletion_state"] != "ACTIVE")
        .count();
    let trusted_for_ai_count = sources
        .iter()
        .filter(|source| source["review_queue_state"] == "trusted")
        .count();
    serde_json::json!({
        "name": "mdx-pages-context-sources-projection",
        "status": "OK",
        "route": "/pages/context-sources/projection.json",
        "source_contract": "docs/PAGES-CONTEXT-SOURCE-RAIL-CONTRACT.md",
        "source_rail": "shared_context_source_rail",
        "supported_intake_surfaces": ["context", "twin", "connector"],
        "writes_allowed": true,
        "admission_route": CONTEXT_ADMISSION_ROUTE,
        "writes_route": CONTEXT_TRUST_ROUTE,
        "write_scope": "human_trust_decision_only",
        "receipt_backed": true,
        "no_second_source_model": true,
        "visible_states": ["Private", "Reviewing", "Trusted for AI"],
        "artifact_projection_route": CONTEXT_ARTIFACTS_PROJECTION_ROUTE,
        "freshness_projection_route": CONTEXT_FRESHNESS_PROJECTION_ROUTE,
        "delete_propagation_proof_route": CONTEXT_DELETE_PROOF_ROUTE,
        "source_count": sources.len(),
        "review_queue_count": review_queue_count,
        "trusted_for_ai_count": trusted_for_ai_count,
        "deleted_source_count": deleted_count,
        "sources": sources,
    })
    .to_string()
}

fn render_source_admission_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let field = |name: &str| json_string_field(body, name).unwrap_or_default();
    let external_ref = field("external_ref");
    let source_kind = first_non_empty(&[field("source_kind"), "web".to_string()]);
    let title = first_non_empty(&[
        field("title"),
        field("artifact_name"),
        "Untitled context source".to_string(),
    ]);
    let artifact_text = first_non_empty(&[
        field("artifact_text"),
        field("source_text"),
        field("source_body"),
        field("body"),
    ]);
    let mime_type = first_non_empty(&[field("mime_type"), "text/markdown".to_string()]);
    let privacy_class =
        first_non_empty(&[field("privacy_class"), "user_private_sensitive".to_string()]);
    let company_context = field("company_context");
    let company_context_source = first_non_empty(&[
        field("company_context_source"),
        "Context:Add source".to_string(),
    ]);
    let session_id = first_non_empty(&[
        field("session_id"),
        "context_source_admission_local".to_string(),
    ]);

    if !external_ref.trim().is_empty() && artifact_text.trim().is_empty() {
        let external_source_id =
            first_non_empty(&[field("source_id"), format!("{source_kind}:context_link")]);
        let summary = field("summary");
        let body_ref = field("body_ref");
        let data_sensitivity = field("data_sensitivity");
        let handling = field("handling");
        let grade = field("grade");
        let scope = first_non_empty(&[field("scope"), "tenant".to_string()]);
        let report = match kernel.record_external_item_with_identity(
            ExternalItem {
                tenant_id: &resolved.tenant_id,
                actor_id: &resolved.actor_id,
                source_id: &external_source_id,
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
            Err(error) => return Ok(context_admission_refusal(&error.message(), &title, "")),
        };
        return Ok(format!(
            r#"{{"name":"mdx-pages-context-source-local-post","status":"ADMITTED","source_admission_mode":"link_reference","source_rail":"connector_item","auth_session_status":{},"admission_route":{},"projection_route":{},"trust_route":{},"source_id":{},"artifact_id":{},"title":{},"source_kind":{},"external_ref":{},"source_admission_receipt_id":{},"policy_decision_id":{},"visible_trust_state":"Private","review_queue_state":"needs_review","no_second_source_model":true,"raw_private_content_recorded":false,"provider_call_allowed":false,"frontier_raw_content_allowed":false,"embedding_provider_call_allowed":false,"memory_write_performed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(CONTEXT_ADMISSION_ROUTE),
            json_string_literal(CONTEXT_SOURCES_PROJECTION_ROUTE),
            json_string_literal(CONTEXT_TRUST_ROUTE),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.receipt_id),
            json_string_literal(&title),
            json_string_literal(&source_kind),
            json_string_literal(&external_ref),
            json_string_literal(&report.receipt_id),
            json_string_literal(&report.policy_decision_id),
        ));
    }

    if context_mime_blocked(&mime_type) {
        return Ok(context_admission_refusal(
            "Context source admission supports text, markdown, CSV, PDF text, and link references first; Office, OCR, and image parsing stay blocked until the parser sandbox is explicit.",
            &title,
            "BLOCKED_UNTIL_PARSER_SANDBOX",
        ));
    }

    let report = match kernel.save_twin_artifact_context_local(TwinArtifactContextRequest {
        tenant_id: &resolved.tenant_id,
        actor_id: &resolved.actor_id,
        session_id: &session_id,
        artifact_name: &title,
        mime_type: &mime_type,
        artifact_text: &artifact_text,
        privacy_class: &privacy_class,
        company_context: &company_context,
        company_context_source: &company_context_source,
    }) {
        Ok(report) => report,
        Err(error) => {
            return Ok(context_admission_refusal(&error.message(), &title, ""));
        }
    };
    Ok(format!(
        r#"{{"name":"mdx-pages-context-source-local-post","status":"ADMITTED","source_admission_mode":"local_content","source_rail":"twin_artifact_context","auth_session_status":{},"admission_route":{},"projection_route":{},"trust_route":{},"artifact_projection_route":{},"source_id":{},"artifact_id":{},"title":{},"mime_type":{},"normalized_type":{},"parse_state":{},"parser_sandbox_required":{},"source_admission_receipt_id":{},"storage_receipt_id":{},"context_packet_receipt_id":{},"grounding_receipt_id":{},"policy_decision_id":{},"visible_trust_state":"Private","review_queue_state":"needs_review","no_second_source_model":true,"raw_private_content_recorded":{},"provider_call_allowed":{},"frontier_raw_content_allowed":{},"embedding_provider_call_allowed":false,"memory_write_performed":{},"production_write_allowed":{}}}"#,
        json_string_literal(resolved.auth_session_status),
        json_string_literal(CONTEXT_ADMISSION_ROUTE),
        json_string_literal(CONTEXT_SOURCES_PROJECTION_ROUTE),
        json_string_literal(CONTEXT_TRUST_ROUTE),
        json_string_literal(CONTEXT_ARTIFACTS_PROJECTION_ROUTE),
        json_string_literal(&report.artifact_id),
        json_string_literal(&report.artifact_id),
        json_string_literal(&report.artifact_name),
        json_string_literal(&report.mime_type),
        json_string_literal(&report.normalized_type),
        json_string_literal(report.parse_state),
        report.parser_sandbox_required,
        json_string_literal(&report.artifact_admission_receipt_id),
        json_string_literal(&report.storage_receipt_id),
        json_string_literal(&report.parse_receipt_id),
        json_string_literal(&report.grounding_receipt_id),
        json_string_literal(&report.policy_decision_id),
        false,
        report.provider_call_allowed,
        report.frontier_raw_content_allowed,
        report.memory_write_performed,
        report.production_write_allowed,
    ))
}

fn context_admission_refusal(reason: &str, title: &str, parser_state: &str) -> String {
    format!(
        r#"{{"name":"mdx-pages-context-source-local-post","status":"REFUSED","reason":{},"admission_route":{},"projection_route":{},"trust_route":{},"title":{},"parser_state":{},"source_admission_receipt_id":"","policy_decision_id":"","receipt_backed":true,"no_second_source_model":true,"raw_private_content_recorded":false,"provider_call_allowed":false,"frontier_raw_content_allowed":false,"embedding_provider_call_allowed":false,"memory_write_performed":false,"production_write_allowed":false}}"#,
        json_string_literal(reason),
        json_string_literal(CONTEXT_ADMISSION_ROUTE),
        json_string_literal(CONTEXT_SOURCES_PROJECTION_ROUTE),
        json_string_literal(CONTEXT_TRUST_ROUTE),
        json_string_literal(title),
        json_string_literal(parser_state),
    )
}

fn context_mime_blocked(mime_type: &str) -> bool {
    matches!(
        mime_type.trim(),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            | "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            | "application/msword"
            | "application/vnd.ms-powerpoint"
            | "application/vnd.ms-excel"
    ) || mime_type.trim().starts_with("image/")
}

fn render_artifacts_json(kernel: &MdxKernel) -> String {
    let rows = context_source_rows(kernel)
        .into_iter()
        .map(|source| {
            let normalized_type = value_str(&source, "normalized_type");
            let supported_now = matches!(
                normalized_type.as_str(),
                "plain_text" | "csv_table_preview" | "pdf_text" | "external_item"
            );
            serde_json::json!({
                "source_id": value_str(&source, "source_id"),
                "artifact_id": value_str(&source, "artifact_id"),
                "title": value_str(&source, "title"),
                "source_kind": value_str(&source, "source_kind"),
                "intake_surface": value_str(&source, "intake_surface"),
                "normalized_type": normalized_type,
                "parse_state": value_str(&source, "parse_state"),
                "parser_id": value_str(&source, "parser_id"),
                "parser_sandbox_required": value_str(&source, "mime_type") == "application/pdf",
                "supported_now": supported_now,
                "blocked_states": {
                    "office_parsing": "BLOCKED_UNTIL_PARSER_SANDBOX",
                    "ocr": "BLOCKED_UNTIL_OCR_SANDBOX",
                    "image_parsing": "BLOCKED_UNTIL_IMAGE_SANDBOX"
                },
                "context_packet_valid": value_bool(&source, "derived_context_valid"),
                "raw_private_content_recorded": false,
                "provider_call_allowed": value_bool(&source, "provider_call_allowed"),
                "frontier_raw_content_allowed": false,
                "memory_write_performed": false,
                "receipts": source["receipts"].clone()
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "name": "mdx-pages-context-artifacts-projection",
        "status": "OK",
        "route": CONTEXT_ARTIFACTS_PROJECTION_ROUTE,
        "source_projection_route": CONTEXT_SOURCES_PROJECTION_ROUTE,
        "source_rail": "shared_context_source_rail",
        "no_second_source_model": true,
        "parser_sandbox_required_for": ["pdf", "office", "ocr", "image"],
        "artifact_count": rows.len(),
        "artifacts": rows,
    })
    .to_string()
}

fn render_freshness_json(kernel: &MdxKernel) -> String {
    let sources = context_source_rows(kernel)
        .into_iter()
        .map(|source| source_freshness_from_row(&source))
        .collect::<Vec<_>>();
    let stale_count = sources
        .iter()
        .filter(|source| value_bool(source, "stale"))
        .count();
    serde_json::json!({
        "name": "mdx-pages-context-freshness-projection",
        "status": "OK",
        "route": CONTEXT_FRESHNESS_PROJECTION_ROUTE,
        "source_projection_route": CONTEXT_SOURCES_PROJECTION_ROUTE,
        "freshness_authority": "receipt_timestamp_plus_review_cadence",
        "trusted_time_source": "receipt_timestamp",
        "review_cadence_days": REVIEW_CADENCE_DAYS,
        "manual_stale_flag_allowed": false,
        "placeholder_trust_created_at_ignored": true,
        "source_count": sources.len(),
        "stale_source_count": stale_count,
        "sources": sources,
    })
    .to_string()
}

fn render_delete_proof_json(kernel: &MdxKernel) -> String {
    let rows = context_source_rows(kernel)
        .into_iter()
        .map(|source| {
            let receipts = &source["receipts"];
            let deleted = value_str(&source, "deletion_state") != "ACTIVE";
            let connector = value_str(&source, "intake_surface") == "connector";
            serde_json::json!({
                "source_id": value_str(&source, "source_id"),
                "artifact_id": value_str(&source, "artifact_id"),
                "title": value_str(&source, "title"),
                "deletion_state": value_str(&source, "deletion_state"),
                "source_deleted": deleted,
                "artifact_delete_receipt_id": value_str(receipts, "delete_receipt_id"),
                "storage_tombstoned": connector || !value_str(receipts, "storage_tombstone_receipt_id").is_empty(),
                "context_packet_invalidated": connector || !value_str(receipts, "context_invalidated_receipt_id").is_empty(),
                "page_draft_blocked": deleted,
                "memory_atoms_invalidated": deleted,
                "graph_node_active": !deleted,
                "generated_projection_removed_from_active_trusted": deleted,
                "twin_fail_closed": deleted,
                "ctx_fail_closed": deleted,
                "forge_fail_closed": deleted,
                "message_fail_closed": deleted,
                "receipts": receipts.clone()
            })
        })
        .collect::<Vec<_>>();
    let deleted_count = rows
        .iter()
        .filter(|row| value_bool(row, "source_deleted"))
        .count();
    serde_json::json!({
        "name": "mdx-pages-context-delete-propagation-proof",
        "status": "OK",
        "route": CONTEXT_DELETE_PROOF_ROUTE,
        "contract": "docs/PAGES-CONTEXT-DELETE-PROPAGATION-CONTRACT.md",
        "source_projection_route": CONTEXT_SOURCES_PROJECTION_ROUTE,
        "proof_scope": [
            "source",
            "artifact",
            "context_packet",
            "page_draft",
            "memory_atom",
            "graph_node",
            "generated_projection",
            "twin",
            "ctx",
            "forge",
            "message"
        ],
        "delete_fail_closed": true,
        "source_count": rows.len(),
        "deleted_source_count": deleted_count,
        "sources": rows,
    })
    .to_string()
}

fn context_source_rows(kernel: &MdxKernel) -> Vec<serde_json::Value> {
    let mut sources = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "twin.artifact.admitted")
        .map(|admission| source_row(kernel, admission))
        .collect::<Vec<_>>();
    sources.extend(
        kernel
            .ledger()
            .entries()
            .iter()
            .filter(|receipt| receipt.kind == "connector.item.ingested")
            .filter(|receipt| connector_scope(receipt) != "personal")
            .map(|connector| connector_source_row(kernel, connector)),
    );
    sources
}

fn source_row(kernel: &MdxKernel, admission: &mdx_core::Receipt) -> serde_json::Value {
    let artifact_id = payload_value(admission, "artifact_id");
    let storage = latest_receipt(kernel, "twin.artifact.storage.bytes_stored", artifact_id)
        .or_else(|| latest_receipt(kernel, "twin.artifact.storage.declared", artifact_id));
    let parse = first_receipt(kernel, "twin.artifact.context_packet.parsed", artifact_id);
    let liteparse = first_receipt(
        kernel,
        "twin.artifact.parser.liteparse_pdf.parsed",
        artifact_id,
    );
    let grounding = first_receipt(
        kernel,
        "twin.artifact.company_context.grounded",
        artifact_id,
    );
    let pages_draft = latest_receipt(kernel, "twin.artifact.pages_draft.created", artifact_id);
    let delete = latest_receipt(kernel, "twin.artifact.delete_requested", artifact_id);
    let tombstone = latest_receipt(
        kernel,
        "twin.artifact.storage_object.tombstoned",
        artifact_id,
    );
    let invalidated = latest_receipt(
        kernel,
        "twin.artifact.context_packet.invalidated",
        artifact_id,
    );
    let trust_decision = latest_trust_decision(kernel, artifact_id);
    let usage = source_usage(kernel, artifact_id, trust_decision);
    let freshness = source_freshness(
        kernel,
        artifact_id,
        trust_decision,
        admission,
        delete.is_some(),
    );

    let deleted = delete.is_some();
    let deletion_state = delete
        .map(|receipt| payload_value(receipt, "deletion_state"))
        .unwrap_or("ACTIVE");
    let storage_contract_state = tombstone
        .map(|receipt| payload_value(receipt, "storage_contract_state"))
        .or_else(|| storage.map(|receipt| payload_value(receipt, "storage_contract_state")))
        .unwrap_or_else(|| payload_value(admission, "storage_contract_state"));
    let blob_bytes_stored = if tombstone.is_some() {
        false
    } else {
        storage
            .map(|receipt| payload_value(receipt, "blob_bytes_stored") == "true")
            .unwrap_or(false)
    };
    let download_allowed = if tombstone.is_some() {
        false
    } else {
        storage
            .map(|receipt| payload_value(receipt, "download_allowed") == "true")
            .unwrap_or(false)
    };
    let source_body_ref = liteparse
        .map(|receipt| payload_value(receipt, "source_body_ref"))
        .or_else(|| storage.map(|receipt| payload_value(receipt, "source_body_ref")))
        .unwrap_or_default();
    let extracted_text_ref = liteparse
        .map(|receipt| payload_value(receipt, "extracted_text_ref"))
        .or_else(|| storage.map(|receipt| payload_value(receipt, "extracted_text_ref")))
        .unwrap_or_default();
    let parser_id = liteparse
        .map(|receipt| payload_value(receipt, "parser_id"))
        .unwrap_or(if parse.is_some() {
            "twin_text_packet"
        } else {
            ""
        });
    let parse_state = if deleted {
        "DELETED".to_string()
    } else if let Some(receipt) = liteparse {
        payload_value(receipt, "parse_state").to_string()
    } else if parse.is_some() {
        "PARSED_LOCAL_TEXT".to_string()
    } else {
        "ADMITTED".to_string()
    };
    let source_trust_state = liteparse
        .map(|receipt| payload_value(receipt, "source_trust_state"))
        .unwrap_or("read_here");
    let trusted_context_state = if deleted {
        "deleted"
    } else {
        trust_decision
            .map(|receipt| payload_value(receipt, "trusted_context_state"))
            .or_else(|| liteparse.map(|receipt| payload_value(receipt, "trusted_context_state")))
            .unwrap_or("read_here")
    };
    let visible_trust_state = if trusted_context_state == "trusted_for_ai" {
        "Trusted for AI"
    } else if source_trust_state == "read_here" || deleted {
        "Private"
    } else {
        "Reviewing"
    };
    let review_queue_state = if deleted {
        "deleted"
    } else if trusted_context_state == "trusted_for_ai" {
        "trusted"
    } else {
        "needs_review"
    };
    let next_action = match review_queue_state {
        "needs_review" => "review_for_trusted_ai",
        "trusted" => "available_to_twin_and_forge",
        _ => "deleted_not_citable",
    };
    let receipts = serde_json::json!({
        "source_artifact_receipt_id": admission.receipt_id,
        "storage_receipt_id": storage.map(|receipt| receipt.receipt_id.as_str()).unwrap_or_default(),
        "parse_receipt_id": parse.map(|receipt| receipt.receipt_id.as_str()).unwrap_or_default(),
        "liteparse_receipt_id": liteparse.map(|receipt| receipt.receipt_id.as_str()).unwrap_or_default(),
        "grounding_receipt_id": grounding.map(|receipt| receipt.receipt_id.as_str()).unwrap_or_default(),
        "pages_draft_receipt_id": pages_draft.map(|receipt| receipt.receipt_id.as_str()).unwrap_or_default(),
        "trust_decision_receipt_id": trust_decision.map(|receipt| receipt.receipt_id.as_str()).unwrap_or_default(),
        "delete_receipt_id": delete.map(|receipt| receipt.receipt_id.as_str()).unwrap_or_default(),
        "storage_tombstone_receipt_id": tombstone.map(|receipt| receipt.receipt_id.as_str()).unwrap_or_default(),
        "context_invalidated_receipt_id": invalidated.map(|receipt| receipt.receipt_id.as_str()).unwrap_or_default(),
    });
    serde_json::json!({
        "source_id": artifact_id,
        "artifact_id": artifact_id,
        "source_kind": "uploaded_file",
        "intake_surface": "twin",
        "title": payload_value(admission, "artifact_name"),
        "mime_type": payload_value(admission, "mime_type"),
        "normalized_type": payload_value(admission, "normalized_type"),
        "privacy_class": payload_value(admission, "privacy_class"),
        "source_body_ref": source_body_ref,
        "extracted_text_ref": extracted_text_ref,
        "source_artifact_hash": payload_value(admission, "source_artifact_hash"),
        "source_trust_state": source_trust_state,
        "trusted_context_state": trusted_context_state,
        "visible_trust_state": visible_trust_state,
        "review_queue_state": review_queue_state,
        "next_action": next_action,
        "allowed_consumers": trust_decision.map(|receipt| payload_value(receipt, "allowed_consumers")).unwrap_or("twin_current_session,pages_draft_review"),
        "company_context_source": grounding.map(|receipt| payload_value(receipt, "company_context_source")).unwrap_or_default(),
        "parser_id": parser_id,
        "parse_state": parse_state,
        "storage_contract_state": storage_contract_state,
        "deletion_state": deletion_state,
        "download_allowed": download_allowed,
        "blob_bytes_stored": blob_bytes_stored,
        "derived_context_valid": !deleted,
        "pages_draft_id": pages_draft.map(|receipt| payload_value(receipt, "pages_draft_id")).unwrap_or_default(),
        "pages_document_id": pages_draft.map(|receipt| payload_value(receipt, "pages_document_id")).unwrap_or_default(),
        "raw_private_content_recorded": false,
        "provider_call_allowed": grounding.map(|receipt| payload_value(receipt, "provider_call_allowed") == "true").unwrap_or(false),
        "frontier_raw_content_allowed": false,
        "memory_write_performed": false,
        "production_write_allowed": false,
        "usage": usage,
        "freshness": freshness,
        "receipts": receipts
    })
}

fn connector_source_row(kernel: &MdxKernel, connector: &mdx_core::Receipt) -> serde_json::Value {
    let source_id = connector.receipt_id.as_str();
    let trust_decision = latest_trust_decision(kernel, source_id);
    let usage = source_usage(kernel, source_id, trust_decision);
    let delete = latest_connector_delete_receipt(kernel, source_id);
    let deleted = delete.is_some();
    let freshness = source_freshness(kernel, source_id, trust_decision, connector, deleted);
    let trusted_context_state = if deleted {
        "deleted"
    } else {
        trust_decision
            .map(|receipt| payload_value(receipt, "trusted_context_state"))
            .unwrap_or("read_here")
    };
    let visible_trust_state = if trusted_context_state == "trusted_for_ai" {
        "Trusted for AI"
    } else {
        "Private"
    };
    let review_queue_state = if deleted {
        "deleted"
    } else if trusted_context_state == "trusted_for_ai" {
        "trusted"
    } else {
        "needs_review"
    };
    let next_action = match review_queue_state {
        "needs_review" => "review_for_trusted_ai",
        "trusted" => "available_to_twin_and_forge",
        _ => "deleted_not_citable",
    };
    let receipts = serde_json::json!({
        "source_artifact_receipt_id": connector.receipt_id.as_str(),
        "storage_receipt_id": "",
        "parse_receipt_id": "",
        "liteparse_receipt_id": "",
        "grounding_receipt_id": "",
        "pages_draft_receipt_id": "",
        "trust_decision_receipt_id": trust_decision.map(|receipt| receipt.receipt_id.as_str()).unwrap_or_default(),
        "delete_receipt_id": delete.map(|receipt| receipt.receipt_id.as_str()).unwrap_or_default(),
        "storage_tombstone_receipt_id": "",
        "context_invalidated_receipt_id": "",
    });
    serde_json::json!({
        "source_id": source_id,
        "artifact_id": source_id,
        "source_kind": payload_value(connector, "source_kind"),
        "intake_surface": "connector",
        "title": payload_value(connector, "title"),
        "mime_type": "",
        "normalized_type": "external_item",
        "privacy_class": payload_value(connector, "data_sensitivity"),
        "source_body_ref": payload_value(connector, "body_ref"),
        "extracted_text_ref": "",
        "external_ref": payload_value(connector, "external_ref"),
        "external_source_id": payload_value(connector, "source_id"),
        "connector_scope": connector_scope(connector),
        "source_artifact_hash": connector.hash.as_str(),
        "source_trust_state": "read_here",
        "trusted_context_state": trusted_context_state,
        "visible_trust_state": visible_trust_state,
        "review_queue_state": review_queue_state,
        "next_action": next_action,
        "allowed_consumers": trust_decision.map(|receipt| payload_value(receipt, "allowed_consumers")).unwrap_or("pages_draft_review"),
        "company_context_source": format!("Connector:{}", payload_value(connector, "external_ref")),
        "parser_id": "",
        "parse_state": "EXTERNAL_RECEIPT_INDEXED",
        "storage_contract_state": "REFERENCE_ONLY",
        "deletion_state": delete.map(|receipt| payload_value(receipt, "deletion_state")).unwrap_or("ACTIVE"),
        "download_allowed": false,
        "blob_bytes_stored": false,
        "derived_context_valid": !deleted,
        "pages_draft_id": "",
        "pages_document_id": "",
        "raw_private_content_recorded": false,
        "provider_call_allowed": false,
        "frontier_raw_content_allowed": false,
        "memory_write_performed": false,
        "production_write_allowed": false,
        "usage": usage,
        "freshness": freshness,
        "receipts": receipts
    })
}

fn latest_connector_delete_receipt<'a>(
    kernel: &'a MdxKernel,
    item_receipt_id: &str,
) -> Option<&'a mdx_core::Receipt> {
    kernel.ledger().entries().iter().rev().find(|receipt| {
        receipt.kind == "connector.item.delete_requested"
            && payload_value(receipt, "item_receipt_id") == item_receipt_id
    })
}

fn source_freshness(
    _kernel: &MdxKernel,
    source_id: &str,
    trust_decision: Option<&mdx_core::Receipt>,
    source_receipt: &mdx_core::Receipt,
    deleted: bool,
) -> serde_json::Value {
    let trusted = trust_decision
        .map(|receipt| payload_value(receipt, "trusted_context_state") == "trusted_for_ai")
        .unwrap_or(false);
    let authority_receipt = trust_decision.unwrap_or(source_receipt);
    let review_state = if deleted {
        "deleted"
    } else if trusted {
        "within_review_window"
    } else {
        "not_trusted"
    };
    serde_json::json!({
        "source_id": source_id,
        "stale": false,
        "review_state": review_state,
        "ai_use_allowed": trusted && !deleted,
        "freshness_authority": "receipt_timestamp_plus_review_cadence",
        "trusted_time_source": "receipt_timestamp",
        "review_cadence_days": REVIEW_CADENCE_DAYS,
        "manual_stale_flag_allowed": false,
        "placeholder_trust_created_at_ignored": true,
        "currentness_receipt_id": authority_receipt.receipt_id.as_str(),
        "currentness_recorded_at": authority_receipt.receipt_timestamp.as_str(),
    })
}

fn source_freshness_from_row(source: &serde_json::Value) -> serde_json::Value {
    let freshness = source["freshness"].clone();
    serde_json::json!({
        "source_id": value_str(source, "source_id"),
        "title": value_str(source, "title"),
        "visible_trust_state": value_str(source, "visible_trust_state"),
        "review_queue_state": value_str(source, "review_queue_state"),
        "deletion_state": value_str(source, "deletion_state"),
        "stale": value_bool(&freshness, "stale"),
        "review_state": value_str(&freshness, "review_state"),
        "ai_use_allowed": value_bool(&freshness, "ai_use_allowed"),
        "freshness_authority": value_str(&freshness, "freshness_authority"),
        "trusted_time_source": value_str(&freshness, "trusted_time_source"),
        "review_cadence_days": REVIEW_CADENCE_DAYS,
        "currentness_record_receipt_id": value_str(&freshness, "currentness_receipt_id"),
        "currentness_recorded_at": value_str(&freshness, "currentness_recorded_at"),
    })
}

#[derive(Default)]
struct ContextSourceUsage {
    twin_receipt_ids: Vec<String>,
    forge_receipt_ids: Vec<String>,
    message_receipt_ids: Vec<String>,
    last_used_surface: String,
    last_used_receipt_id: String,
    last_used_at: String,
}

fn source_usage(
    kernel: &MdxKernel,
    source_id: &str,
    trust_decision: Option<&mdx_core::Receipt>,
) -> serde_json::Value {
    let trust_receipt_id = trust_decision
        .map(|receipt| receipt.receipt_id.as_str())
        .unwrap_or_default();
    let mut usage = ContextSourceUsage::default();
    for receipt in kernel.ledger().entries() {
        match receipt.kind.as_str() {
            "twin.artifact.company_context.grounded"
                if csv_payload_contains(receipt, "trusted_context_source_ids", source_id) =>
            {
                usage.twin_receipt_ids.push(receipt.receipt_id.clone());
                mark_last_usage(&mut usage, "twin", receipt);
            }
            // The live run admission receipt is the forge usage anchor;
            // the old intake/build lane kinds are deleted. Counts stay 0
            // until runs carry trusted_context_source_ids.
            "forge.run.event"
                if csv_payload_contains(receipt, "trusted_context_source_ids", source_id) =>
            {
                usage.forge_receipt_ids.push(receipt.receipt_id.clone());
                mark_last_usage(&mut usage, "forge", receipt);
            }
            "message.human.posted"
                if payload_value(receipt, "trusted_context_source_id") == source_id
                    || (!trust_receipt_id.is_empty()
                        && payload_value(receipt, "trusted_context_receipt_id")
                            == trust_receipt_id) =>
            {
                usage.message_receipt_ids.push(receipt.receipt_id.clone());
                mark_last_usage(&mut usage, "message", receipt);
            }
            _ => {}
        }
    }
    let twin_usage_count = usage.twin_receipt_ids.len();
    let forge_usage_count = usage.forge_receipt_ids.len();
    let message_citation_count = usage.message_receipt_ids.len();
    let usage_count = twin_usage_count + forge_usage_count + message_citation_count;
    serde_json::json!({
        "usage_count": usage_count,
        "usage_state": if usage_count == 0 { "not_used" } else { "used" },
        "used_by_twin": twin_usage_count > 0,
        "twin_usage_count": twin_usage_count,
        "used_by_forge": forge_usage_count > 0,
        "forge_usage_count": forge_usage_count,
        "cited_in_message": message_citation_count > 0,
        "message_citation_count": message_citation_count,
        "last_used_surface": usage.last_used_surface,
        "last_used_receipt_id": usage.last_used_receipt_id,
        "last_used_at": usage.last_used_at,
        "twin_usage_receipt_ids": usage.twin_receipt_ids.join(","),
        "forge_usage_receipt_ids": usage.forge_receipt_ids.join(","),
        "message_citation_receipt_ids": usage.message_receipt_ids.join(","),
    })
}

fn mark_last_usage(usage: &mut ContextSourceUsage, surface: &str, receipt: &mdx_core::Receipt) {
    usage.last_used_surface = surface.to_string();
    usage.last_used_receipt_id = receipt.receipt_id.clone();
    usage.last_used_at = receipt.receipt_timestamp.clone();
}

fn csv_payload_contains(receipt: &mdx_core::Receipt, key: &str, needle: &str) -> bool {
    payload_value(receipt, key)
        .split(',')
        .any(|candidate| candidate.trim() == needle)
}

fn render_trust_decision_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let source_id = source_id(body, kernel)?;
    let trust_decision_id = json_string_field(body, "trust_decision_id")
        .unwrap_or_else(|| format!("pages_context_trust_{source_id}"));
    let decision_outcome =
        json_string_field(body, "decision_outcome").unwrap_or_else(|| "trusted_for_ai".to_string());
    let decision_note = json_string_field(body, "decision_note").unwrap_or_else(|| {
        "Human marked this source trusted for governed AI use in MDx.".to_string()
    });
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
    let report = match kernel.save_pages_context_source_trust_decision_local_with_identity(
        PagesContextSourceTrustDecision {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            trust_decision_id: &trust_decision_id,
            source_id: &source_id,
            decision_outcome: &decision_outcome,
            decision_note: &decision_note,
            source_route: "/pages/context-sources/trust-decisions.json",
        },
        &identity,
    ) {
        Ok(report) => report,
        Err(error) => {
            return Ok(format!(
                r#"{{"name":"mdx-pages-context-source-trust-decision-local-post","status":"REFUSED","reason":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/pages/context-sources/trust-decisions.json","projection_route":"/pages/context-sources/projection.json","source_id":{},"receipt_backed":true,"source_receipt_id":"","trust_decision_receipt_id":"","no_second_source_model":true,"raw_private_content_recorded":false,"frontier_raw_content_allowed":false,"memory_write_performed":false,"production_write_allowed":false}}"#,
                json_string_literal(&error.message()),
                json_string_literal(resolved.auth_session_status),
                json_string_literal(&resolved.tenant_id),
                json_string_literal(&resolved.actor_id),
                json_string_literal(&resolved.actor_role),
                json_string_literal(&source_id)
            ));
        }
    };
    Ok(format!(
        r#"{{"name":"mdx-pages-context-source-trust-decision-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/pages/context-sources/trust-decisions.json","projection_route":"/pages/context-sources/projection.json","trust_decision_id":{},"trust_decision_receipt_id":{},"policy_decision_id":{},"source_id":{},"artifact_id":{},"source_admission_receipt_id":{},"decision_outcome":{},"decision_note":{},"trusted_context_state":{},"visible_trust_state":{},"review_queue_state":{},"allowed_consumers":{},"created_at":{},"created_at_authority":"local_placeholder_not_trusted_time","freshness_authority":"deferred_to_trusted_time_arc","human_decision_recorded":{},"no_second_source_model":true,"raw_private_content_recorded":{},"frontier_raw_content_allowed":{},"memory_write_performed":{},"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.trust_decision_id),
        json_string_literal(&report.trust_decision_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.source_id),
        json_string_literal(&report.artifact_id),
        json_string_literal(&report.source_admission_receipt_id),
        json_string_literal(&report.decision_outcome),
        json_string_literal(&report.decision_note),
        json_string_literal(&report.trusted_context_state),
        json_string_literal(&report.visible_trust_state),
        json_string_literal(&report.review_queue_state),
        json_string_literal(&report.allowed_consumers),
        json_string_literal(&report.created_at),
        report.human_decision_recorded,
        report.raw_private_content_recorded,
        report.frontier_raw_content_allowed,
        report.memory_write_performed,
        report.production_write_allowed
    ))
}

fn source_id(body: &str, kernel: &MdxKernel) -> Result<String, String> {
    if let Some(source_id) =
        json_string_field(body, "source_id").or_else(|| json_string_field(body, "artifact_id"))
    {
        return Ok(source_id);
    }
    kernel
        .ledger()
        .entries()
        .iter()
        .rev()
        .find(|receipt| receipt.kind == "twin.artifact.admitted")
        .map(|receipt| payload_value(receipt, "artifact_id").to_string())
        .filter(|source_id| !source_id.is_empty())
        .or_else(|| {
            kernel
                .ledger()
                .entries()
                .iter()
                .rev()
                .find(|receipt| {
                    receipt.kind == "connector.item.ingested"
                        && connector_scope(receipt) != "personal"
                })
                .map(|receipt| receipt.receipt_id.clone())
        })
        .ok_or_else(|| "pages context trust decision missing source_id".to_string())
}

fn connector_scope(receipt: &mdx_core::Receipt) -> &str {
    match receipt
        .payload
        .get("scope")
        .map(String::as_str)
        .unwrap_or("")
    {
        "" => "tenant",
        value => value,
    }
}

fn first_non_empty(values: &[String]) -> String {
    values
        .iter()
        .find(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_default()
}

fn value_str(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn value_bool(value: &serde_json::Value, key: &str) -> bool {
    value
        .get(key)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::{json_string_field, route_response};
    use crate::route_request_with_body;
    use mdx_core::MdxKernel;
    use std::sync::{Arc, RwLock};

    #[test]
    fn pages_context_source_projection_starts_empty() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response("GET", "/pages/context-sources/projection.json", "", &kernel)
            .expect("route")
            .expect("response")
            .body;
        assert!(response.contains("\"name\":\"mdx-pages-context-sources-projection\""));
        assert!(response.contains("\"no_second_source_model\":true"));
        assert!(response.contains("\"source_count\":0"));
    }

    #[test]
    fn pages_context_source_admission_uses_shared_artifact_rail() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let admitted = route_request_with_body(
            "POST",
            "/pages/context-sources.json",
            r#"{"tenant_id":"tenant_pages_context_sources","title":"Architecture standard","mime_type":"text/markdown","artifact_text":"Use shared Context source receipts.","company_context":"Company standards live in Context."}"#,
            &kernel,
        )
        .expect("context source admission")
        .body;
        for expected in [
            "\"name\":\"mdx-pages-context-source-local-post\"",
            "\"status\":\"ADMITTED\"",
            "\"source_admission_mode\":\"local_content\"",
            "\"source_rail\":\"twin_artifact_context\"",
            "\"projection_route\":\"/pages/context-sources/projection.json\"",
            "\"artifact_projection_route\":\"/pages/context-artifacts/projection.json\"",
            "\"no_second_source_model\":true",
            "\"raw_private_content_recorded\":false",
            "\"embedding_provider_call_allowed\":false",
            "\"memory_write_performed\":false",
        ] {
            assert!(admitted.contains(expected), "{expected}: {admitted}");
        }

        let projection =
            route_request_with_body("GET", "/pages/context-sources/projection.json", "", &kernel)
                .expect("projection")
                .body;
        for expected in [
            "\"source_count\":1",
            "\"intake_surface\":\"twin\"",
            "\"title\":\"Architecture standard\"",
            "\"visible_trust_state\":\"Private\"",
            "\"review_queue_state\":\"needs_review\"",
        ] {
            assert!(projection.contains(expected), "{expected}: {projection}");
        }
    }

    #[test]
    fn pages_context_source_admission_uses_connector_rail_for_link_reference() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let admitted = route_request_with_body(
            "POST",
            "/pages/context-sources.json",
            r#"{"tenant_id":"tenant_pages_context_sources","title":"Brand guide link","source_kind":"github","source_id":"github:mdx-stack","external_ref":"https://github.com/mdx-stack/docs/brand.md","summary":"Brand rules for beta.","scope":"tenant"}"#,
            &kernel,
        )
        .expect("context link admission")
        .body;
        for expected in [
            "\"source_admission_mode\":\"link_reference\"",
            "\"source_rail\":\"connector_item\"",
            "\"source_kind\":\"github\"",
            "\"external_ref\":\"https://github.com/mdx-stack/docs/brand.md\"",
            "\"no_second_source_model\":true",
            "\"raw_private_content_recorded\":false",
            "\"memory_write_performed\":false",
        ] {
            assert!(admitted.contains(expected), "{expected}: {admitted}");
        }
        let item_receipt_id =
            json_string_field(&admitted, "source_admission_receipt_id").expect("connector receipt");
        let projection =
            route_request_with_body("GET", "/pages/context-sources/projection.json", "", &kernel)
                .expect("projection")
                .body;
        assert!(
            projection.contains(&format!("\"source_id\":\"{item_receipt_id}\"")),
            "{projection}"
        );
        assert!(
            projection.contains("\"intake_surface\":\"connector\""),
            "{projection}"
        );
    }

    #[test]
    fn pages_context_source_admission_blocks_office_and_image_parsing() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let refused = route_request_with_body(
            "POST",
            "/pages/context-sources.json",
            r#"{"title":"Roadmap deck","mime_type":"application/vnd.openxmlformats-officedocument.presentationml.presentation","artifact_text":"slides"}"#,
            &kernel,
        )
        .expect("context source refusal")
        .body;
        for expected in [
            "\"status\":\"REFUSED\"",
            "\"parser_state\":\"BLOCKED_UNTIL_PARSER_SANDBOX\"",
            "\"source_admission_receipt_id\":\"\"",
            "\"no_second_source_model\":true",
            "\"raw_private_content_recorded\":false",
            "\"memory_write_performed\":false",
        ] {
            assert!(refused.contains(expected), "{expected}: {refused}");
        }
    }

    #[test]
    fn pages_context_artifact_freshness_and_delete_proof_routes_project_sources() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_request_with_body(
            "POST",
            "/pages/context-sources.json",
            r#"{"tenant_id":"tenant_pages_context_sources","title":"Current standard","mime_type":"text/plain","artifact_text":"Use current standards."}"#,
            &kernel,
        )
        .expect("context source admission");
        let artifact_projection = route_request_with_body(
            "GET",
            "/pages/context-artifacts/projection.json",
            "",
            &kernel,
        )
        .expect("artifact projection")
        .body;
        for expected in [
            "\"name\":\"mdx-pages-context-artifacts-projection\"",
            "\"artifact_count\":1",
            "\"supported_now\":true",
            "\"context_packet_valid\":true",
            "\"raw_private_content_recorded\":false",
        ] {
            assert!(
                artifact_projection.contains(expected),
                "{expected}: {artifact_projection}"
            );
        }

        let freshness = route_request_with_body(
            "GET",
            "/pages/context-freshness/projection.json",
            "",
            &kernel,
        )
        .expect("freshness projection")
        .body;
        for expected in [
            "\"name\":\"mdx-pages-context-freshness-projection\"",
            "\"freshness_authority\":\"receipt_timestamp_plus_review_cadence\"",
            "\"trusted_time_source\":\"receipt_timestamp\"",
            "\"manual_stale_flag_allowed\":false",
            "\"placeholder_trust_created_at_ignored\":true",
            "\"stale_source_count\":0",
        ] {
            assert!(freshness.contains(expected), "{expected}: {freshness}");
        }

        let delete_proof = route_request_with_body(
            "GET",
            "/pages/context-delete-propagation/proof.json",
            "",
            &kernel,
        )
        .expect("delete proof")
        .body;
        for expected in [
            "\"name\":\"mdx-pages-context-delete-propagation-proof\"",
            "\"delete_fail_closed\":true",
            "\"source_count\":1",
            "\"generated_projection_removed_from_active_trusted\":false",
            "\"twin_fail_closed\":false",
        ] {
            assert!(
                delete_proof.contains(expected),
                "{expected}: {delete_proof}"
            );
        }
    }

    #[test]
    fn pages_context_source_projection_maps_twin_artifact_rail() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_pages_context_sources","artifact_name":"architecture-standard.md","mime_type":"text/markdown","artifact_text":"Use the shared source rail. Do not fork upload models.","company_context":"Pages says source provenance must stay receipt backed.","company_context_source":"Pages:Context source rail contract"}"#,
            &kernel,
        )
        .expect("twin artifact");
        let response =
            route_request_with_body("GET", "/pages/context-sources/projection.json", "", &kernel)
                .expect("pages context sources")
                .body;
        for expected in [
            "\"name\":\"mdx-pages-context-sources-projection\"",
            "\"source_rail\":\"shared_context_source_rail\"",
            "\"supported_intake_surfaces\":[\"context\",\"twin\",\"connector\"]",
            "\"no_second_source_model\":true",
            "\"source_count\":1",
            "\"review_queue_count\":1",
            "\"source_kind\":\"uploaded_file\"",
            "\"intake_surface\":\"twin\"",
            "\"title\":\"architecture-standard.md\"",
            "\"source_trust_state\":\"read_here\"",
            "\"trusted_context_state\":\"read_here\"",
            "\"visible_trust_state\":\"Private\"",
            "\"review_queue_state\":\"needs_review\"",
            "\"next_action\":\"review_for_trusted_ai\"",
            "\"frontier_raw_content_allowed\":false",
            "\"memory_write_performed\":false",
        ] {
            assert!(response.contains(expected), "{expected}: {response}");
        }
    }

    #[test]
    fn pages_context_source_projection_maps_tenant_connector_items() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let ingested = route_request_with_body(
            "POST",
            "/connectors/items.json",
            r#"{"tenant_id":"tenant_pages_context_sources","source_id":"github:mdx-stack","source_kind":"github","external_ref":"https://github.com/mdx-stack/docs/architecture.md","title":"Architecture guide","summary":"Use shared source rails for company context.","grade":"operational","scope":"tenant"}"#,
            &kernel,
        )
        .expect("connector item")
        .body;
        let item_receipt_id = json_string_field(&ingested, "item_receipt_id").expect("receipt id");

        route_request_with_body(
            "POST",
            "/connectors/items.json",
            r#"{"tenant_id":"tenant_pages_context_sources","source_id":"notion:private","source_kind":"notion","external_ref":"https://notion/private","title":"Private note","scope":"personal"}"#,
            &kernel,
        )
        .expect("personal connector item");

        let response =
            route_request_with_body("GET", "/pages/context-sources/projection.json", "", &kernel)
                .expect("pages context sources")
                .body;
        for expected in [
            "\"source_count\":1",
            "\"review_queue_count\":1",
            "\"source_kind\":\"github\"",
            "\"intake_surface\":\"connector\"",
            "\"normalized_type\":\"external_item\"",
            "\"connector_scope\":\"tenant\"",
            "\"external_source_id\":\"github:mdx-stack\"",
            "\"external_ref\":\"https://github.com/mdx-stack/docs/architecture.md\"",
            "\"title\":\"Architecture guide\"",
            "\"source_trust_state\":\"read_here\"",
            "\"trusted_context_state\":\"read_here\"",
            "\"visible_trust_state\":\"Private\"",
            "\"review_queue_state\":\"needs_review\"",
            "\"next_action\":\"review_for_trusted_ai\"",
            "\"raw_private_content_recorded\":false",
            "\"frontier_raw_content_allowed\":false",
        ] {
            assert!(response.contains(expected), "{expected}: {response}");
        }
        assert!(
            response.contains(&format!("\"source_id\":\"{item_receipt_id}\"")),
            "{response}"
        );
        assert!(!response.contains("notion:private"), "{response}");
        assert!(!response.contains("Private note"), "{response}");
    }

    #[test]
    fn pages_context_source_trust_decision_marks_connector_item_trusted_for_ai() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let ingested = route_request_with_body(
            "POST",
            "/connectors/items.json",
            r#"{"tenant_id":"tenant_pages_context_sources","source_id":"github:mdx-stack","source_kind":"github","external_ref":"https://github.com/mdx-stack/docs/brand.md","title":"Brand guide","summary":"Use the MDx product voice.","grade":"operational","scope":"tenant"}"#,
            &kernel,
        )
        .expect("connector item")
        .body;
        let item_receipt_id = json_string_field(&ingested, "item_receipt_id").expect("receipt id");

        let decision = route_request_with_body(
            "POST",
            "/pages/context-sources/trust-decisions.json",
            &format!(
                r#"{{"tenant_id":"tenant_pages_context_sources","source_id":"{item_receipt_id}","decision_note":"Approved as company brand context."}}"#
            ),
            &kernel,
        )
        .expect("pages context trust decision")
        .body;
        for expected in [
            "\"status\":\"PAGES_CONTEXT_SOURCE_TRUSTED_FOR_AI\"",
            "\"source_id\":\"",
            "\"decision_outcome\":\"trusted_for_ai\"",
            "\"trusted_context_state\":\"trusted_for_ai\"",
            "\"visible_trust_state\":\"Trusted for AI\"",
            "\"review_queue_state\":\"trusted\"",
            "\"no_second_source_model\":true",
            "\"frontier_raw_content_allowed\":false",
            "\"memory_write_performed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(decision.contains(expected), "{expected}: {decision}");
        }
        assert!(
            decision.contains(&format!("\"source_id\":\"{item_receipt_id}\"")),
            "{decision}"
        );

        let response =
            route_request_with_body("GET", "/pages/context-sources/projection.json", "", &kernel)
                .expect("pages context sources")
                .body;
        for expected in [
            "\"source_count\":1",
            "\"review_queue_count\":0",
            "\"trusted_for_ai_count\":1",
            "\"intake_surface\":\"connector\"",
            "\"trusted_context_state\":\"trusted_for_ai\"",
            "\"visible_trust_state\":\"Trusted for AI\"",
            "\"review_queue_state\":\"trusted\"",
            "\"next_action\":\"available_to_twin_and_forge\"",
            "\"allowed_consumers\":\"twin_current_session,pages_draft_review,twin_company_context,forge_context_packet,ctx_recall_packet\"",
        ] {
            assert!(response.contains(expected), "{expected}: {response}");
        }
    }

    #[test]
    fn pages_context_source_trust_decision_refuses_personal_connector_item() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let ingested = route_request_with_body(
            "POST",
            "/connectors/items.json",
            r#"{"tenant_id":"tenant_pages_context_sources","source_id":"notion:private","source_kind":"notion","external_ref":"https://notion/private","title":"Private note","scope":"personal"}"#,
            &kernel,
        )
        .expect("personal connector item")
        .body;
        let item_receipt_id = json_string_field(&ingested, "item_receipt_id").expect("receipt id");
        let decision = route_request_with_body(
            "POST",
            "/pages/context-sources/trust-decisions.json",
            &format!(
                r#"{{"tenant_id":"tenant_pages_context_sources","source_id":"{item_receipt_id}","decision_note":"Should stay personal."}}"#
            ),
            &kernel,
        )
        .expect("pages context trust refusal")
        .body;
        for expected in [
            "\"status\":\"REFUSED\"",
            "personal scoped",
            "\"trust_decision_receipt_id\":\"\"",
            "\"frontier_raw_content_allowed\":false",
            "\"memory_write_performed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(decision.contains(expected), "{expected}: {decision}");
        }
    }

    #[test]
    fn pages_context_source_projection_marks_deleted_connector_item_not_citable() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let ingested = route_request_with_body(
            "POST",
            "/connectors/items.json",
            r#"{"tenant_id":"tenant_pages_context_sources","source_id":"github:mdx-stack","source_kind":"github","external_ref":"https://github.com/mdx-stack/docs/delete.md","title":"Deleted guide","summary":"This will be deleted.","grade":"operational","scope":"tenant"}"#,
            &kernel,
        )
        .expect("connector item")
        .body;
        let item_receipt_id = json_string_field(&ingested, "item_receipt_id").expect("receipt id");
        route_request_with_body(
            "POST",
            "/pages/context-sources/trust-decisions.json",
            &format!(
                r#"{{"tenant_id":"tenant_pages_context_sources","source_id":"{item_receipt_id}","decision_note":"Approved before deletion."}}"#
            ),
            &kernel,
        )
        .expect("pages context trust decision");
        let deletion = route_request_with_body(
            "POST",
            "/connectors/items/deletions.json",
            &format!(
                r#"{{"tenant_id":"tenant_pages_context_sources","item_receipt_id":"{item_receipt_id}","reason":"Remove connector item."}}"#
            ),
            &kernel,
        )
        .expect("connector deletion")
        .body;
        for expected in [
            "\"name\":\"mdx-connector-item-delete-local-post\"",
            "\"status\":\"DELETED\"",
            "\"trusted_context_invalidated\":true",
            "\"production_write_allowed\":false",
        ] {
            assert!(deletion.contains(expected), "{expected}: {deletion}");
        }

        let response =
            route_request_with_body("GET", "/pages/context-sources/projection.json", "", &kernel)
                .expect("pages context sources")
                .body;
        for expected in [
            "\"source_count\":1",
            "\"deleted_source_count\":1",
            "\"trusted_for_ai_count\":0",
            "\"trusted_context_state\":\"deleted\"",
            "\"review_queue_state\":\"deleted\"",
            "\"next_action\":\"deleted_not_citable\"",
            "\"deletion_state\":\"DELETED\"",
            "\"derived_context_valid\":false",
            "\"delete_receipt_id\":\"connector_delete_receipt_",
        ] {
            assert!(response.contains(expected), "{expected}: {response}");
        }
    }

    #[test]
    fn pages_context_source_trust_decision_marks_source_trusted_for_ai() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_pages_context_sources","artifact_name":"architecture-standard.md","mime_type":"text/markdown","artifact_text":"Use the shared source rail. Do not fork upload models.","company_context":"Pages says source provenance must stay receipt backed.","company_context_source":"Pages:Context source rail contract"}"#,
            &kernel,
        )
        .expect("twin artifact");
        let decision = route_request_with_body(
            "POST",
            "/pages/context-sources/trust-decisions.json",
            r#"{"tenant_id":"tenant_pages_context_trust_mark","decision_note":"Approved as company architecture context."}"#,
            &kernel,
        )
        .expect("pages context trust decision")
        .body;
        for expected in [
            "\"name\":\"mdx-pages-context-source-trust-decision-local-post\"",
            "\"status\":\"PAGES_CONTEXT_SOURCE_TRUSTED_FOR_AI\"",
            "\"decision_outcome\":\"trusted_for_ai\"",
            "\"trusted_context_state\":\"trusted_for_ai\"",
            "\"visible_trust_state\":\"Trusted for AI\"",
            "\"review_queue_state\":\"trusted\"",
            "\"no_second_source_model\":true",
            "\"frontier_raw_content_allowed\":false",
            "\"memory_write_performed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(decision.contains(expected), "{expected}: {decision}");
        }

        let response =
            route_request_with_body("GET", "/pages/context-sources/projection.json", "", &kernel)
                .expect("pages context sources")
                .body;
        for expected in [
            "\"writes_route\":\"/pages/context-sources/trust-decisions.json\"",
            "\"source_count\":1",
            "\"review_queue_count\":0",
            "\"trusted_for_ai_count\":1",
            "\"trusted_context_state\":\"trusted_for_ai\"",
            "\"visible_trust_state\":\"Trusted for AI\"",
            "\"review_queue_state\":\"trusted\"",
            "\"next_action\":\"available_to_twin_and_forge\"",
            "\"trust_decision_receipt_id\":\"",
            "\"allowed_consumers\":\"twin_current_session,pages_draft_review,twin_company_context,forge_context_packet,ctx_recall_packet\"",
            "\"frontier_raw_content_allowed\":false",
            "\"memory_write_performed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(response.contains(expected), "{expected}: {response}");
        }
    }

    #[test]
    fn pages_context_source_projection_rolls_up_trusted_context_usage() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let tenant_id = "tenant_pages_context_usage_rollup";
        let source = route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            &format!(
                r#"{{"tenant_id":"{tenant_id}","artifact_name":"usage-standard.md","mime_type":"text/markdown","artifact_text":"This standard should be visible when apps use it.","company_context":"Usage should be receipt derived.","company_context_source":"Pages:Context usage trail"}}"#
            ),
            &kernel,
        )
        .expect("twin artifact")
        .body;
        let source_id = json_string_field(&source, "artifact_id").expect("artifact id");
        let decision = route_request_with_body(
            "POST",
            "/pages/context-sources/trust-decisions.json",
            &format!(
                r#"{{"tenant_id":"{tenant_id}","source_id":"{source_id}","decision_note":"Approved for usage trail proof."}}"#
            ),
            &kernel,
        )
        .expect("pages context trust decision")
        .body;
        let trust_receipt_id =
            json_string_field(&decision, "trust_decision_receipt_id").expect("trust receipt");

        route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            &format!(
                r#"{{"tenant_id":"{tenant_id}","artifact_name":"usage-plan.md","mime_type":"text/markdown","artifact_text":"Use the trusted usage standard."}}"#
            ),
            &kernel,
        )
        .expect("twin use");
        route_request_with_body(
            "POST",
            "/messages/thread-messages.json",
            &format!(
                r#"{{"tenant_id":"{tenant_id}","message_id":"context_usage_message","body":"Citing the trusted source.","source_receipt_id":"{trust_receipt_id}"}}"#
            ),
            &kernel,
        )
        .expect("message citation");

        let response =
            route_request_with_body("GET", "/pages/context-sources/projection.json", "", &kernel)
                .expect("pages context sources")
                .body;
        for expected in [
            "\"usage_count\":2",
            "\"usage_state\":\"used\"",
            "\"used_by_twin\":true",
            "\"twin_usage_count\":1",
            "\"used_by_forge\":false",
            "\"forge_usage_count\":0",
            "\"cited_in_message\":true",
            "\"message_citation_count\":1",
            "\"last_used_surface\":\"message\"",
            "\"last_used_receipt_id\":\"message_thread_message_receipt_",
            "\"twin_usage_receipt_ids\":\"twin_artifact_context_receipt_",
            "\"message_citation_receipt_ids\":\"message_thread_message_receipt_",
        ] {
            assert!(response.contains(expected), "{expected}: {response}");
        }
    }

    #[test]
    fn pages_context_source_trust_decision_revoke_supersedes_prior_trust() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let source = route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_pages_context_trust_revoke","artifact_name":"revoked-standard.md","mime_type":"text/markdown","artifact_text":"This standard should stop grounding work when revoked.","company_context":"Revocation must supersede trust.","company_context_source":"Pages:Context trust revoke"}"#,
            &kernel,
        )
        .expect("twin artifact")
        .body;
        let source_id = json_string_field(&source, "artifact_id").expect("artifact id");
        route_request_with_body(
            "POST",
            "/pages/context-sources/trust-decisions.json",
            &format!(
                r#"{{"tenant_id":"tenant_pages_context_trust_revoke","source_id":"{source_id}","decision_note":"Approved before revoke."}}"#
            ),
            &kernel,
        )
        .expect("pages context trust decision");
        let revoke = route_request_with_body(
            "POST",
            "/pages/context-sources/trust-decisions.json",
            &format!(
                r#"{{"tenant_id":"tenant_pages_context_trust_revoke","source_id":"{source_id}","decision_outcome":"revoke_trusted_ai","decision_note":"No longer current enough for AI use."}}"#
            ),
            &kernel,
        )
        .expect("pages context revoke decision")
        .body;
        for expected in [
            "\"status\":\"PAGES_CONTEXT_SOURCE_TRUST_REVOKED\"",
            "\"decision_outcome\":\"revoke_trusted_ai\"",
            "\"trusted_context_state\":\"read_here\"",
            "\"visible_trust_state\":\"Private\"",
            "\"review_queue_state\":\"needs_review\"",
            "\"allowed_consumers\":\"twin_current_session,pages_draft_review\"",
        ] {
            assert!(revoke.contains(expected), "{expected}: {revoke}");
        }

        let response =
            route_request_with_body("GET", "/pages/context-sources/projection.json", "", &kernel)
                .expect("pages context sources")
                .body;
        for expected in [
            "\"trusted_for_ai_count\":0",
            "\"trusted_context_state\":\"read_here\"",
            "\"visible_trust_state\":\"Private\"",
            "\"review_queue_state\":\"needs_review\"",
        ] {
            assert!(response.contains(expected), "{expected}: {response}");
        }

        let twin = route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_pages_context_trust_revoke","artifact_name":"after-revoke.md","mime_type":"text/markdown","artifact_text":"This should not use revoked trusted context."}"#,
            &kernel,
        )
        .expect("twin after revoke")
        .body;
        assert!(twin.contains("\"trusted_context_used\":false"), "{twin}");
        assert!(
            twin.contains("\"trusted_context_source_count\":0"),
            "{twin}"
        );
    }

    #[test]
    fn pages_context_source_trust_decision_refuses_deleted_source_as_json() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_pages_context_sources","artifact_name":"deleted-standard.md","mime_type":"text/markdown","artifact_text":"This source will be deleted."}"#,
            &kernel,
        )
        .expect("twin artifact");
        route_request_with_body(
            "POST",
            "/twin/artifacts/deletions.json",
            r#"{"tenant_id":"tenant_pages_context_sources","reason":"Route smoke delete path."}"#,
            &kernel,
        )
        .expect("twin artifact deletion");

        let decision = route_request_with_body(
            "POST",
            "/pages/context-sources/trust-decisions.json",
            r#"{"tenant_id":"tenant_pages_context_sources","decision_note":"Should refuse because source is deleted."}"#,
            &kernel,
        )
        .expect("pages context trust refusal")
        .body;
        for expected in [
            "\"name\":\"mdx-pages-context-source-trust-decision-local-post\"",
            "\"status\":\"REFUSED\"",
            "\"reason\":\"pages context source",
            "has been deleted",
            "\"receipt_backed\":true",
            "\"trust_decision_receipt_id\":\"\"",
            "\"frontier_raw_content_allowed\":false",
            "\"memory_write_performed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(decision.contains(expected), "{expected}: {decision}");
        }
    }
}
