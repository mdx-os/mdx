use base64::Engine;
use mdx_core::{
    MdxKernel, TwinArtifactBlobStoredRequest, TwinArtifactContextRequest,
    TwinArtifactDeletionRequest, TwinArtifactDownloadRequest, TwinArtifactLiteParseRequest,
    TwinArtifactPagesDraftRequest, json_string_literal,
};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::sync::{Mutex, OnceLock};

use crate::{RouteResponse, reject_unless_method, studio};

pub(crate) fn route_request(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/twin/artifact-runtime.json" => Some(render_runtime_route(method, kernel)),
        "/twin/parser-sandbox-runtime.json" => Some(render_parser_sandbox_runtime_route(method)),
        "/twin/artifact-contexts.json" => Some(render_post_route(method, body, kernel)),
        "/twin/artifact-contexts/projection.json" => Some(render_projection_route(method, kernel)),
        "/twin/artifacts/liteparse-parses.json" => {
            Some(render_liteparse_route(method, body, kernel))
        }
        "/twin/artifacts/pages-drafts.json" => Some(render_pages_draft_route(method, body, kernel)),
        "/twin/artifacts/downloads.json" => Some(render_download_route(method, body, kernel)),
        "/twin/artifacts/deletions.json" => Some(render_delete_route(method, body, kernel)),
        _ => None,
    }
}

pub(crate) fn render_local_command_json() -> Result<String, String> {
    let mut kernel = MdxKernel::boot_local();
    let mut report = save_fixture(&mut kernel)?;
    record_local_blob_storage(
        &mut kernel,
        &mut report,
        "tenant_local",
        "local_user",
        "Board update says retention risk is rising and onboarding needs a clearer owner.",
    )?;
    Ok(report.render_post_json())
}

fn render_runtime_route(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    Ok(RouteResponse::json(
        "200 OK",
        render_artifact_runtime_json(&kernel),
    ))
}

fn render_post_route(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let artifact_text = field_or(
        body,
        "artifact_text",
        "Board update says retention risk is rising and onboarding needs a clearer owner.",
    );
    let tenant_id = field_or(body, "tenant_id", "tenant_local");
    let actor_id = field_or(body, "actor_id", "local_user");
    let report = kernel
        .save_twin_artifact_context_local(TwinArtifactContextRequest {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            session_id: &field_or(body, "session_id", "twin_artifact_session_local_001"),
            artifact_name: &field_or(body, "artifact_name", "board-update.pdf"),
            mime_type: &field_or(body, "mime_type", "application/pdf"),
            artifact_text: &artifact_text,
            privacy_class: &field_or(body, "privacy_class", "user_private_sensitive"),
            company_context: &field_or(body, "company_context", ""),
            company_context_source: &field_or(body, "company_context_source", ""),
        })
        .map_err(|error| error.message())?;
    let mut report = report;
    record_local_blob_storage(
        &mut kernel,
        &mut report,
        &tenant_id,
        &actor_id,
        &artifact_text,
    )?;
    if let Some(interpretation) = local_private_model_interpretation(&report, body) {
        report.interpretation = interpretation;
    }
    store_artifact_interpretation(&report.artifact_id, &report.interpretation);
    Ok(RouteResponse::json("200 OK", report.render_post_json()))
}

fn render_parser_sandbox_runtime_route(method: &str) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "GET") {
        return Ok(response);
    }
    Ok(RouteResponse::json(
        "200 OK",
        render_parser_sandbox_runtime_json(),
    ))
}

fn render_projection_route(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    Ok(RouteResponse::json(
        "200 OK",
        render_artifact_projection_json(&kernel),
    ))
}

fn render_download_route(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let body_artifact_id = field_or(body, "artifact_id", "");
    if body_artifact_id.trim().is_empty() && latest_artifact_id(&kernel).is_empty() {
        let mut report = save_fixture(&mut kernel)?;
        record_local_blob_storage(
            &mut kernel,
            &mut report,
            "tenant_local",
            "local_user",
            "Board update says retention risk is rising and onboarding needs a clearer owner.",
        )?;
    }
    let artifact_id = if body_artifact_id.trim().is_empty() {
        latest_artifact_id(&kernel)
    } else {
        body_artifact_id
    };
    let tenant_id = field_or(body, "tenant_id", "tenant_local");
    let artifact_text =
        crate::twin_artifact_blob_store::read_source_text(&tenant_id, &artifact_id)?
            .ok_or_else(|| "twin artifact local blob could not be read".to_string())?;
    let report = kernel
        .authorize_twin_artifact_download_local(TwinArtifactDownloadRequest {
            tenant_id: &tenant_id,
            actor_id: &field_or(body, "actor_id", "local_user"),
            artifact_id: &artifact_id,
        })
        .map_err(|error| error.message())?;
    Ok(RouteResponse::json(
        "200 OK",
        report.render_download_json(&artifact_text),
    ))
}

fn render_delete_route(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let body_artifact_id = field_or(body, "artifact_id", "");
    if body_artifact_id.trim().is_empty() && latest_artifact_id(&kernel).is_empty() {
        save_fixture(&mut kernel)?;
    }
    let artifact_id = if body_artifact_id.trim().is_empty() {
        latest_artifact_id(&kernel)
    } else {
        body_artifact_id
    };
    let tenant_id = field_or(body, "tenant_id", "tenant_local");
    let local_blob_bytes_deleted =
        crate::twin_artifact_blob_store::delete_artifact(&tenant_id, &artifact_id)?;
    let report = kernel
        .delete_twin_artifact_local(TwinArtifactDeletionRequest {
            tenant_id: &tenant_id,
            actor_id: &field_or(body, "actor_id", "local_user"),
            artifact_id: &artifact_id,
            reason: &field_or(body, "reason", "User asked Twin to forget this upload."),
            local_blob_bytes_deleted,
        })
        .map_err(|error| error.message())?;
    Ok(RouteResponse::json("200 OK", report.render_post_json()))
}

fn render_liteparse_route(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "POST") {
        return Ok(response);
    }
    if field_or(body, "route_smoke", "false") == "true" {
        let tenant_id = field_or(body, "tenant_id", "tenant_route_smoke");
        let artifact_id = field_or(body, "artifact_id", "twin_liteparse_route_smoke");
        let artifact_name = field_or(body, "artifact_name", "route-smoke.pdf");
        return Ok(RouteResponse::json(
            "200 OK",
            format!(
                r#"{{"name":"mdx-twin-artifact-liteparse-local-post","status":"ROUTE_SMOKE_ACCEPTED","route":"/twin/artifacts/liteparse-parses.json","runtime_route":"/twin/parser-sandbox-runtime.json","artifact_context_route":"/twin/artifact-contexts.json","artifact_id":{},"artifact_name":{},"tenant_id":{},"parser_id":"liteparse","parse_state":"ROUTE_SMOKE_NO_PARSE","receipt_backed":true,"receipts":[],"raw_private_content_recorded":false,"provider_call_allowed":false,"frontier_raw_content_allowed":false,"memory_write_performed":false,"production_write_allowed":false}}"#,
                json_string_literal(&artifact_id),
                json_string_literal(&artifact_name),
                json_string_literal(&tenant_id)
            ),
        ));
    }
    let tenant_id = field_or(body, "tenant_id", "tenant_local");
    let actor_id = field_or(body, "actor_id", "local_user");
    let artifact_id = sanitized_json_id(&field_or(
        body,
        "artifact_id",
        "twin_liteparse_pdf_local_001",
    ))
    .ok_or_else(|| "liteparse artifact_id must be lowercase letters, digits, _ or -".to_string())?;
    let artifact_name = field_or(body, "artifact_name", "sample-private-brief.pdf");
    let mime_type = field_or(body, "mime_type", "application/pdf");
    if mime_type != "application/pdf" {
        return Err("liteparse local route only accepts application/pdf".to_string());
    }
    let pdf_bytes = pdf_bytes_from_body(body)?;
    if pdf_bytes.len() > mdx_core::TWIN_ARTIFACT_FILE_SIZE_LIMIT_BYTES {
        return Err("liteparse pdf exceeds local file size limit".to_string());
    }
    let parser_source = crate::twin_artifact_blob_store::stage_pdf_for_parser(
        &tenant_id,
        &artifact_id,
        &pdf_bytes,
    )?;
    let started = std::time::Instant::now();
    let parse_result = parse_pdf_with_liteparse(parser_source.path())?;
    let parser_elapsed_ms = started.elapsed().as_millis();
    let extracted_text = parse_result
        .text
        .trim()
        .chars()
        .take(16_000)
        .collect::<String>();
    if extracted_text.trim().is_empty() {
        return Err("liteparse pdf extracted no readable text; OCR is not enabled yet".to_string());
    }
    let stored = crate::twin_artifact_blob_store::store_binary_artifact_with_extracted_text(
        &tenant_id,
        &artifact_id,
        "pdf",
        &pdf_bytes,
        &extracted_text,
    )?;
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = kernel
        .record_twin_artifact_liteparse_pdf_local(TwinArtifactLiteParseRequest {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            artifact_id: &artifact_id,
            artifact_name: &artifact_name,
            mime_type: &mime_type,
            source_body_ref: &stored.source_body_ref,
            extracted_text_ref: &stored.extracted_text_ref,
            source_body_hash: &stored.source_body_hash,
            extracted_text_hash: &stored.extracted_text_hash,
            byte_size: stored.byte_size,
            page_count: parse_result.pages.len(),
            extracted_text_char_count: stored.extracted_text_char_count,
            parser_elapsed_ms,
            storage_path_posture: stored.storage_path_posture,
            storage_contract_state: stored.storage_contract_state,
        })
        .map_err(|error| error.message())?;
    Ok(RouteResponse::json(
        "200 OK",
        report.render_post_json(&extracted_text),
    ))
}

fn parse_pdf_with_liteparse(path: &std::path::Path) -> Result<liteparse::ParseResult, String> {
    let mut config = liteparse::LiteParseConfig::default();
    config.ocr_enabled = false;
    config.output_format = liteparse::OutputFormat::Markdown;
    config.image_mode = liteparse::config::ImageMode::Off;
    config.max_pages = 25;
    config.quiet = true;
    let parser = liteparse::LiteParse::new(config);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("liteparse runtime: {error}"))?;
    let path = path
        .to_str()
        .ok_or_else(|| "liteparse path is not valid UTF-8".to_string())?;
    runtime
        .block_on(parser.parse(path))
        .map_err(|error| format!("liteparse parse: {error}"))
}

fn pdf_bytes_from_body(body: &str) -> Result<Vec<u8>, String> {
    let encoded = field_or(body, "artifact_pdf_base64", "");
    if encoded.trim().is_empty() {
        return Ok(sample_liteparse_pdf_bytes());
    }
    let normalized = encoded
        .split_once(',')
        .map(|(_, value)| value)
        .unwrap_or(encoded.as_str())
        .trim();
    base64::engine::general_purpose::STANDARD
        .decode(normalized)
        .map_err(|error| format!("artifact_pdf_base64 decode failed: {error}"))
}

fn sanitized_json_id(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > 96 {
        return None;
    }
    if value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        Some(value.to_string())
    } else {
        None
    }
}

fn sample_liteparse_pdf_bytes() -> Vec<u8> {
    let objects = [
        "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n",
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n",
        "4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
        "5 0 obj\n<< /Length 125 >>\nstream\nBT\n/F1 14 Tf\n72 720 Td\n(Twin should read this local PDF privately.) Tj\n0 -24 Td\n(Onboarding add-on conflicts with the Q3 retention decision.) Tj\nET\nendstream\nendobj\n",
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for object in objects {
        offsets.push(pdf.len());
        pdf.push_str(object);
    }
    let xref_offset = pdf.len();
    pdf.push_str("xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n"
    ));
    pdf.into_bytes()
}

fn record_local_blob_storage(
    kernel: &mut MdxKernel,
    report: &mut mdx_core::TwinArtifactContextReport,
    tenant_id: &str,
    actor_id: &str,
    artifact_text: &str,
) -> Result<(), String> {
    let stored = crate::twin_artifact_blob_store::store_text_artifact(
        tenant_id,
        &report.artifact_id,
        artifact_text,
    )?;
    let storage = kernel
        .record_twin_artifact_blob_stored_local(TwinArtifactBlobStoredRequest {
            tenant_id,
            actor_id,
            artifact_id: &report.artifact_id,
            source_body_ref: &stored.source_body_ref,
            extracted_text_ref: &stored.extracted_text_ref,
            source_body_hash: &stored.source_body_hash,
            extracted_text_hash: &stored.extracted_text_hash,
            byte_size: stored.byte_size,
            extracted_text_char_count: stored.extracted_text_char_count,
            storage_path_posture: stored.storage_path_posture,
            storage_contract_state: stored.storage_contract_state,
            encrypted_at_rest_posture: stored.encrypted_at_rest_posture,
            malware_scan_state: stored.malware_scan_state,
            quarantine_state: stored.quarantine_state,
        })
        .map_err(|error| error.message())?;
    report.storage_receipt_id = storage.storage_receipt_id;
    report.policy_decision_id = storage.policy_decision_id;
    report.storage_path = storage.storage_path;
    report.storage_path_posture = storage.storage_path_posture;
    report.storage_contract_state = storage.storage_contract_state;
    report.download_authorization = storage.download_authorization;
    report.download_allowed = storage.download_allowed;
    report.blob_bytes_stored = storage.blob_bytes_stored;
    report.malware_scan_state = storage.malware_scan_state;
    report.quarantine_state = storage.quarantine_state;
    Ok(())
}

fn render_pages_draft_route(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let body_artifact_id = field_or(body, "artifact_id", "");
    if body_artifact_id.trim().is_empty() && latest_artifact_id(&kernel).is_empty() {
        save_fixture(&mut kernel)?;
    }
    let artifact_id = if body_artifact_id.trim().is_empty() {
        latest_artifact_id(&kernel)
    } else {
        body_artifact_id
    };
    let suffix = artifact_suffix(&artifact_id);
    let pages_draft_id = field_or(
        body,
        "pages_draft_id",
        &format!("twin_artifact_pages_draft_{suffix}"),
    );
    let pages_document_id = field_or(
        body,
        "pages_document_id",
        &format!("page_twin_artifact_{suffix}"),
    );
    let pages_title = field_or(
        body,
        "pages_title",
        &format!("Twin Read - {}", artifact_name_for(&kernel, &artifact_id)),
    );
    let pages_revision_id = field_or(
        body,
        "pages_revision_id",
        &format!("rev_twin_artifact_{suffix}"),
    );
    let approval_request_id = field_or(
        body,
        "approval_request_id",
        &format!("approval_twin_artifact_{suffix}"),
    );
    let draft_body = render_pages_draft_body_for_artifact(&kernel, &artifact_id);
    let body_ref = crate::pages_body_store::store_draft_body(&pages_draft_id, &draft_body)
        .ok_or_else(|| "twin artifact Pages draft body could not be stored".to_string())?;
    let report = kernel
        .save_twin_artifact_pages_draft_local(TwinArtifactPagesDraftRequest {
            tenant_id: &field_or(body, "tenant_id", "tenant_local"),
            actor_id: &field_or(body, "actor_id", "local_user"),
            artifact_id: &artifact_id,
            pages_draft_id: &pages_draft_id,
            pages_document_id: &pages_document_id,
            pages_title: &pages_title,
            pages_body_ref: &body_ref,
            pages_revision_id: &pages_revision_id,
            approval_request_id: &approval_request_id,
            requested_visibility: &field_or(body, "requested_visibility", "tenant_review"),
        })
        .map_err(|error| error.message())?;
    Ok(RouteResponse::json("200 OK", report.render_post_json()))
}

fn save_fixture(kernel: &mut MdxKernel) -> Result<mdx_core::TwinArtifactContextReport, String> {
    kernel
        .save_twin_artifact_context_local(TwinArtifactContextRequest {
            tenant_id: "tenant_local",
            actor_id: "local_user",
            session_id: "twin_artifact_session_local_001",
            artifact_name: "board-update.pdf",
            mime_type: "application/pdf",
            artifact_text: "Board update says retention risk is rising and onboarding needs a clearer owner.",
            privacy_class: "user_private_sensitive",
            company_context: "Pages decision states onboarding clarity is the active Q3 customer-health bet.",
            company_context_source: "Pages:Q3 customer-health decision",
        })
        .map_err(|error| error.message())
}

pub(crate) fn render_parser_sandbox_runtime_json() -> String {
    format!(
        r#"{{"name":"mdx-twin-parser-sandbox-runtime","status":"LITEPARSE_PDF_READY_LOCAL","route":"/twin/parser-sandbox-runtime.json","artifact_runtime_route":"/twin/artifact-runtime.json","artifact_context_route":"/twin/artifact-contexts.json","liteparse_route":"/twin/artifacts/liteparse-parses.json","read_only":true,"receipt_backed":true,"parser_sandbox_required":true,"out_of_process_required":true,"binary_parsing_live":true,"pdf_parsing_live":true,"office_parsing_live":false,"image_parsing_live":false,"ocr_parsing_live":false,"text_bearing_parsing_live":true,"safe_reader_refusal_state":"PDF_LOCAL_READY_OFFICE_IMAGE_OCR_NOT_READY","supported_now":["text/plain","text/markdown","text/csv","application/pdf"],"blocked_until_sandbox":["application/vnd.openxmlformats-officedocument.wordprocessingml.document","application/vnd.openxmlformats-officedocument.spreadsheetml.sheet","image/png","image/jpeg","scanned_pdf_ocr"],"caps":{{"file_size_limit_bytes":{},"max_extracted_text_chars":16000,"raw_text_receipts_allowed":false,"frontier_raw_content_allowed":false}},"sandbox_boundary":{{"process_boundary":"required_out_of_process","network_access":"denied_by_default","filesystem_access":"artifact_temp_dir_only","malware_scan":"required_before_office_image_parser_live","redaction":"required_before_frontier_routing_considered","delete_propagation":"source_and_extracted_text_refs_must_delete_together"}},"frontier_raw_content_allowed":false,"production_write_allowed":false,"ui_handoff":{{"target_owner":"claude","safe_to_render":true,"copy_boundary":"Show PDF reading as local and private. Keep Office, image, and OCR not-ready states calm and do not expose parser jargon unless the user opens details."}},"evidence":{{"source_contract":"docs/TWIN-ARTIFACT-CONTEXT-BACKEND-HANDOFF.md","proof_check":"make twin-artifact-context-check","source_routes":["/twin/parser-sandbox-runtime.json","/twin/artifact-runtime.json","/twin/artifact-contexts.json","/twin/artifacts/liteparse-parses.json"]}}}}"#,
        mdx_core::TWIN_ARTIFACT_FILE_SIZE_LIMIT_BYTES
    )
}

pub(crate) fn render_artifact_runtime_json(kernel: &MdxKernel) -> String {
    let private_provider = private_provider_state(kernel);
    let artifact_count = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "twin.artifact.admitted")
        .count();
    let context_count = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "twin.artifact.context_packet.parsed")
        .count();
    let grounding_count = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "twin.artifact.company_context.grounded")
        .count();
    let deleted_count = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "twin.artifact.delete_requested")
        .count();
    let pages_draft_count = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "twin.artifact.pages_draft.created")
        .count();
    format!(
        r#"{{"name":"mdx-twin-artifact-runtime","status":"LIVE-LOCAL-TWIN-ARTIFACT-CONTEXT-FLOOR","route":"/twin/artifact-runtime.json","context_route":"/twin/artifact-contexts.json","projection_route":"/twin/artifact-contexts/projection.json","pages_draft_route":"/twin/artifacts/pages-drafts.json","download_route":"/twin/artifacts/downloads.json","delete_route":"/twin/artifacts/deletions.json","parser_sandbox_route":"/twin/parser-sandbox-runtime.json","liteparse_route":"/twin/artifacts/liteparse-parses.json","read_only":true,"receipt_backed":true,"track_b_slices":["B1_artifact_intake","B2_parser_context_packet","B2A_company_context_grounding","B1_storage_local_blob","B1_delete_propagation","B1_parser_sandbox_runway","B3_sovereign_adapter","B5_pages_draft_from_artifact"],"artifact_count":{},"context_packet_count":{},"grounding_packet_count":{},"deleted_artifact_count":{},"pages_draft_count":{},"privacy_default":"user_private_sensitive","supported_mime_types":["application/pdf","application/vnd.openxmlformats-officedocument.wordprocessingml.document","text/csv","text/plain","text/markdown"],"artifact_card_contract":{{"visible_fields":["artifact_name","parse_state","next_move"],"quiet_disclosure":["privacy_class","receipt_ids","source_artifact_hash","parser_sandbox_required","deletion_state"]}},"latency_budget_ms":250,"sovereign_provider_required":true,"sovereign_provider_connected":{},"sovereign_provider_id":{},"sovereign_model_id":{},"provider_not_connected_reason":{},"provider_call_allowed":{},"frontier_raw_content_allowed":false,"frontier_refusal_reason":"frontier_refused_for_raw_sensitive_artifact","memory_write_performed":false,"storage_contract_state":"LOCAL_BLOB_STORED_BY_REFERENCE","storage_bucket_required":true,"parser_sandbox_required":true,"local_pages_draft_write_allowed":true,"production_write_allowed":false,"ui_handoff":{{"target_owner":"claude","target_surface":"apps/mdx-host Twin module","safe_to_render":true,"copy_boundary":"Show name, state, and next move first. Keep privacy, receipts, storage, deletion, private-model connection, PDF local-reader, and Pages approval details in quiet disclosure."}},"evidence":{{"source_contract":"docs/TWIN-FRONTIER-PRODUCT-RUNTIME-PLAN.md","proof_check":"make twin-artifact-context-check","source_routes":["/twin/artifact-runtime.json","/twin/parser-sandbox-runtime.json","/twin/artifact-contexts.json","/twin/artifact-contexts/projection.json","/twin/artifacts/liteparse-parses.json","/twin/artifacts/pages-drafts.json","/twin/artifacts/downloads.json","/twin/artifacts/deletions.json","/twin/model-gateway-provider-observations.json"]}}}}"#,
        artifact_count,
        context_count,
        grounding_count,
        deleted_count,
        pages_draft_count,
        private_provider.connected,
        json_string_literal(&private_provider.provider_id),
        json_string_literal(&private_provider.model_id),
        json_string_literal(&private_provider.not_connected_reason),
        private_provider.connected
    )
}

pub(crate) fn render_artifact_projection_json(kernel: &MdxKernel) -> String {
    let artifacts = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "twin.artifact.admitted")
        .map(|admission| render_projection_item(kernel, admission))
        .collect::<Vec<_>>();
    format!(
        r#"{{"name":"mdx-twin-artifact-context-local-projection","status":"OK","route":"/twin/artifact-contexts/projection.json","auth_session_route":"/local/auth-session.json","writes_route":"/twin/artifact-contexts.json","download_route":"/twin/artifacts/downloads.json","delete_route":"/twin/artifacts/deletions.json","runtime_route":"/twin/artifact-runtime.json","artifact_count":{},"context_packet_count":{},"grounding_packet_count":{},"deleted_artifact_count":{},"provider_call_allowed":{},"frontier_raw_content_allowed":false,"memory_write_performed":false,"storage_contract_state":"LOCAL_BLOB_STORED_BY_REFERENCE","storage_bucket_required":true,"parser_sandbox_required":true,"production_write_allowed":false,"artifacts":[{}]}}"#,
        artifacts.len(),
        kernel
            .ledger()
            .entries()
            .iter()
            .filter(|receipt| receipt.kind == "twin.artifact.context_packet.parsed")
            .count(),
        kernel
            .ledger()
            .entries()
            .iter()
            .filter(|receipt| receipt.kind == "twin.artifact.company_context.grounded")
            .count(),
        kernel
            .ledger()
            .entries()
            .iter()
            .filter(|receipt| receipt.kind == "twin.artifact.delete_requested")
            .count(),
        private_provider_state(kernel).connected,
        artifacts.join(",")
    )
}

fn render_projection_item(kernel: &MdxKernel, admission: &mdx_core::Receipt) -> String {
    let artifact_id = payload_value(admission, "artifact_id");
    let storage = kernel
        .ledger()
        .entries()
        .iter()
        .find(|receipt| {
            receipt.kind == "twin.artifact.storage.bytes_stored"
                && payload_value(receipt, "artifact_id") == artifact_id
        })
        .or_else(|| {
            kernel.ledger().entries().iter().find(|receipt| {
                receipt.kind == "twin.artifact.storage.declared"
                    && payload_value(receipt, "artifact_id") == artifact_id
            })
        });
    let parse = kernel.ledger().entries().iter().find(|receipt| {
        receipt.kind == "twin.artifact.context_packet.parsed"
            && payload_value(receipt, "artifact_id") == artifact_id
    });
    let grounding = kernel.ledger().entries().iter().find(|receipt| {
        receipt.kind == "twin.artifact.company_context.grounded"
            && payload_value(receipt, "artifact_id") == artifact_id
    });
    let delete = kernel.ledger().entries().iter().rev().find(|receipt| {
        receipt.kind == "twin.artifact.delete_requested"
            && payload_value(receipt, "artifact_id") == artifact_id
    });
    let storage_tombstone = kernel.ledger().entries().iter().rev().find(|receipt| {
        receipt.kind == "twin.artifact.storage_object.tombstoned"
            && payload_value(receipt, "artifact_id") == artifact_id
    });
    let context_invalidated = kernel.ledger().entries().iter().rev().find(|receipt| {
        receipt.kind == "twin.artifact.context_packet.invalidated"
            && payload_value(receipt, "artifact_id") == artifact_id
    });
    let deletion_state = delete
        .map(|receipt| payload_value(receipt, "deletion_state"))
        .unwrap_or_else(|| "ACTIVE".to_string());
    let parse_state = if delete.is_some() {
        "DELETED"
    } else {
        "PARSED_LOCAL_TEXT"
    };
    let next_move = if delete.is_some() {
        "upload_again_or_remove_card"
    } else {
        "review_interpretation_or_create_pages_draft"
    };
    let parse_receipt_id = parse
        .map(|receipt| receipt.receipt_id.as_str())
        .unwrap_or("");
    let grounding_receipt_id = grounding
        .map(|receipt| receipt.receipt_id.as_str())
        .unwrap_or("");
    let storage_receipt_id = storage
        .map(|receipt| receipt.receipt_id.as_str())
        .unwrap_or("");
    let artifact_deleted_receipt_id = delete
        .map(|receipt| receipt.receipt_id.as_str())
        .unwrap_or("");
    let storage_tombstone_receipt_id = storage_tombstone
        .map(|receipt| receipt.receipt_id.as_str())
        .unwrap_or("");
    let context_invalidated_receipt_id = context_invalidated
        .map(|receipt| receipt.receipt_id.as_str())
        .unwrap_or("");
    let context_packet_id = parse
        .map(|receipt| payload_value(receipt, "context_packet_id"))
        .unwrap_or_default();
    let grounding_packet_id = grounding
        .map(|receipt| payload_value(receipt, "grounding_packet_id"))
        .unwrap_or_default();
    let company_context_source = grounding
        .map(|receipt| payload_value(receipt, "company_context_source"))
        .unwrap_or_else(|| "Pages:local company context".to_string());
    let trusted_context_used = grounding
        .map(|receipt| payload_value(receipt, "trusted_context_used") == "true")
        .unwrap_or(false);
    let trusted_context_source_count = grounding
        .map(|receipt| {
            payload_value(receipt, "trusted_context_source_count")
                .parse::<usize>()
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let trusted_context_source_ids = grounding
        .map(|receipt| payload_value(receipt, "trusted_context_source_ids"))
        .unwrap_or_default();
    let trusted_context_receipt_ids = grounding
        .map(|receipt| payload_value(receipt, "trusted_context_receipt_ids"))
        .unwrap_or_default();
    let trusted_context_projection_route = grounding
        .map(|receipt| payload_value(receipt, "trusted_context_projection_route"))
        .unwrap_or_else(|| "/pages/context-sources/projection.json".to_string());
    let provider_call_allowed = grounding
        .map(|receipt| payload_value(receipt, "provider_call_allowed") == "true")
        .unwrap_or(false);
    let provider_class = grounding
        .map(|receipt| payload_value(receipt, "provider_class"))
        .unwrap_or_else(|| "local".to_string());
    let provider_id = grounding
        .map(|receipt| payload_value(receipt, "provider_id"))
        .unwrap_or_default();
    let model_id = grounding
        .map(|receipt| payload_value(receipt, "model_id"))
        .unwrap_or_default();
    let model_gateway_routing = grounding
        .map(|receipt| payload_value(receipt, "model_gateway_routing"))
        .unwrap_or_default();
    let provider_observation_receipt_id = grounding
        .map(|receipt| payload_value(receipt, "provider_observation_receipt_id"))
        .unwrap_or_default();
    let sovereign_provider_required = grounding
        .map(|receipt| payload_value(receipt, "sovereign_provider_required") == "true")
        .unwrap_or(true);
    let sovereign_provider_connected = grounding
        .map(|receipt| payload_value(receipt, "sovereign_provider_connected") == "true")
        .unwrap_or(false);
    let provider_not_connected_reason = grounding
        .map(|receipt| payload_value(receipt, "provider_not_connected_reason"))
        .unwrap_or_else(|| "connect_private_model".to_string());
    let frontier_refusal_reason = grounding
        .map(|receipt| payload_value(receipt, "frontier_refusal_reason"))
        .unwrap_or_else(|| "frontier_refused_for_raw_sensitive_artifact".to_string());
    let storage_path = storage
        .map(|receipt| payload_value(receipt, "storage_path"))
        .unwrap_or_else(|| payload_value(admission, "storage_path"));
    let storage_path_posture = storage
        .map(|receipt| payload_value(receipt, "storage_path_posture"))
        .unwrap_or_else(|| payload_value(admission, "storage_path_posture"));
    let storage_contract_state = storage_tombstone
        .map(|receipt| payload_value(receipt, "storage_contract_state"))
        .or_else(|| storage.map(|receipt| payload_value(receipt, "storage_contract_state")))
        .unwrap_or_else(|| payload_value(admission, "storage_contract_state"));
    let retention_policy = storage
        .map(|receipt| payload_value(receipt, "retention_policy"))
        .unwrap_or_else(|| payload_value(admission, "retention_policy"));
    let download_authorization = storage
        .map(|receipt| payload_value(receipt, "download_authorization"))
        .unwrap_or_else(|| payload_value(admission, "download_authorization"));
    let download_allowed = storage_tombstone
        .map(|receipt| payload_value(receipt, "download_allowed") == "true")
        .or_else(|| storage.map(|receipt| payload_value(receipt, "download_allowed") == "true"))
        .unwrap_or(false);
    let blob_bytes_stored = if storage_tombstone.is_some() {
        false
    } else {
        storage
            .map(|receipt| payload_value(receipt, "blob_bytes_stored") == "true")
            .unwrap_or(false)
    };
    let malware_scan_state = storage
        .map(|receipt| payload_value(receipt, "malware_scan_state"))
        .unwrap_or_else(|| payload_value(admission, "malware_scan_state"));
    let quarantine_state = storage
        .map(|receipt| payload_value(receipt, "quarantine_state"))
        .unwrap_or_else(|| payload_value(admission, "quarantine_state"));
    let interpretation = artifact_interpretation_for(&artifact_id).unwrap_or_else(|| {
        format!(
            "Twin parsed {} and grounded it against {} with separate artifact and company citations.",
            payload_value(admission, "artifact_name"),
            company_context_source
        )
    });
    format!(
        r#"{{"artifact_id":{},"storage_object_id":{},"context_packet_id":{},"grounding_packet_id":{},"session_id":{},"artifact_name":{},"normalized_type":{},"mime_type":{},"privacy_class":{},"parse_state":{},"next_move":{},"artifact_admission_receipt_id":{},"storage_receipt_id":{},"parse_receipt_id":{},"grounding_receipt_id":{},"artifact_deleted_receipt_id":{},"storage_tombstone_receipt_id":{},"context_invalidated_receipt_id":{},"source_artifact_hash":{},"storage_path":{},"storage_path_posture":{},"storage_contract_state":{},"retention_policy":{},"deletion_state":{},"download_authorization":{},"download_allowed":{},"blob_bytes_stored":{},"malware_scan_state":{},"quarantine_state":{},"file_size_limit_bytes":{},"byte_size":{},"extracted_text_char_count":{},"citation_count":2,"artifact_citation_label":"uploaded artifact","company_context_citation_label":"company context","company_context_source":{},"trusted_context_used":{},"trusted_context_source_count":{},"trusted_context_source_ids":{},"trusted_context_receipt_ids":{},"trusted_context_projection_route":{},"interpretation":{},"proposed_next_action":"draft_pages_summary","memory_eligible":false,"memory_write_performed":false,"derived_context_valid":{},"provider_class":{},"provider_id":{},"model_id":{},"model_gateway_routing":{},"provider_observation_receipt_id":{},"sovereign_provider_required":{},"sovereign_provider_connected":{},"provider_not_connected_reason":{},"frontier_refusal_reason":{},"provider_call_allowed":{},"frontier_raw_content_allowed":false,"storage_bucket_required":true,"parser_sandbox_required":true,"production_write_allowed":false}}"#,
        json_string_literal(&artifact_id),
        json_string_literal(&payload_value(admission, "storage_object_id")),
        json_string_literal(&context_packet_id),
        json_string_literal(&grounding_packet_id),
        json_string_literal(&payload_value(admission, "session_id")),
        json_string_literal(&payload_value(admission, "artifact_name")),
        json_string_literal(&payload_value(admission, "normalized_type")),
        json_string_literal(&payload_value(admission, "mime_type")),
        json_string_literal(&payload_value(admission, "privacy_class")),
        json_string_literal(parse_state),
        json_string_literal(next_move),
        json_string_literal(&admission.receipt_id),
        json_string_literal(storage_receipt_id),
        json_string_literal(parse_receipt_id),
        json_string_literal(grounding_receipt_id),
        json_string_literal(artifact_deleted_receipt_id),
        json_string_literal(storage_tombstone_receipt_id),
        json_string_literal(context_invalidated_receipt_id),
        json_string_literal(&payload_value(admission, "source_artifact_hash")),
        json_string_literal(&storage_path),
        json_string_literal(&storage_path_posture),
        json_string_literal(&storage_contract_state),
        json_string_literal(&retention_policy),
        json_string_literal(&deletion_state),
        json_string_literal(&download_authorization),
        download_allowed,
        blob_bytes_stored,
        json_string_literal(&malware_scan_state),
        json_string_literal(&quarantine_state),
        payload_value(admission, "file_size_limit_bytes")
            .parse::<usize>()
            .unwrap_or(0),
        payload_value(admission, "byte_size")
            .parse::<usize>()
            .unwrap_or(0),
        parse
            .map(
                |receipt| payload_value(receipt, "extracted_text_char_count")
                    .parse::<usize>()
                    .unwrap_or(0)
            )
            .unwrap_or(0),
        json_string_literal(&company_context_source),
        trusted_context_used,
        trusted_context_source_count,
        json_string_literal(&trusted_context_source_ids),
        json_string_literal(&trusted_context_receipt_ids),
        json_string_literal(&trusted_context_projection_route),
        json_string_literal(&interpretation),
        delete.is_none(),
        json_string_literal(&provider_class),
        json_string_literal(&provider_id),
        json_string_literal(&model_id),
        json_string_literal(&model_gateway_routing),
        json_string_literal(&provider_observation_receipt_id),
        sovereign_provider_required,
        sovereign_provider_connected,
        json_string_literal(&provider_not_connected_reason),
        json_string_literal(&frontier_refusal_reason),
        provider_call_allowed
    )
}

fn field_or(body: &str, key: &str, default: &str) -> String {
    studio::json_string_field(body, key).unwrap_or_else(|| default.to_string())
}

fn latest_artifact_id(kernel: &MdxKernel) -> String {
    kernel
        .ledger()
        .entries()
        .iter()
        .rev()
        .find(|receipt| receipt.kind == "twin.artifact.admitted")
        .map(|receipt| payload_value(receipt, "artifact_id"))
        .unwrap_or_default()
}

fn artifact_suffix(artifact_id: &str) -> String {
    artifact_id
        .rsplit('_')
        .next()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("local")
        .to_string()
}

fn artifact_name_for(kernel: &MdxKernel, artifact_id: &str) -> String {
    latest_artifact_payload(
        kernel,
        artifact_id,
        "twin.artifact.admitted",
        "artifact_name",
    )
    .unwrap_or_else(|| "Uploaded Artifact".to_string())
}

fn render_pages_draft_body_for_artifact(kernel: &MdxKernel, artifact_id: &str) -> String {
    let artifact_name = artifact_name_for(kernel, artifact_id);
    let normalized_type = latest_artifact_payload(
        kernel,
        artifact_id,
        "twin.artifact.admitted",
        "normalized_type",
    )
    .unwrap_or_else(|| "artifact".to_string());
    let source_artifact_hash = latest_artifact_payload(
        kernel,
        artifact_id,
        "twin.artifact.admitted",
        "source_artifact_hash",
    )
    .unwrap_or_default();
    let extracted_text_char_count = latest_artifact_payload(
        kernel,
        artifact_id,
        "twin.artifact.context_packet.parsed",
        "extracted_text_char_count",
    )
    .unwrap_or_else(|| "0".to_string());
    let company_context_source = latest_artifact_payload(
        kernel,
        artifact_id,
        "twin.artifact.company_context.grounded",
        "company_context_source",
    )
    .unwrap_or_else(|| "Pages:local company context".to_string());
    let interpretation = artifact_interpretation_for(artifact_id).unwrap_or_else(|| {
        format!(
            "Twin read {artifact_name} as {normalized_type} and prepared this Pages draft from the private artifact context packet."
        )
    });
    format!(
        "# Twin Read - {artifact_name}\n\n{interpretation}\n\n## Useful Read\n\nThe artifact points to work that should be reviewed in the context of {company_context_source}. Twin kept artifact evidence and company context separate so approval can inspect each source without exposing raw private content in receipts.\n\n## Citations\n\n- Uploaded artifact: {artifact_id} ({source_artifact_hash})\n- Company context: {company_context_source}\n- Parsed context size: {extracted_text_char_count} characters\n\n## Next Move\n\nReview this draft, edit it if needed, then use the Pages approval rail before publishing."
    )
}

struct PrivateProviderState {
    connected: bool,
    provider_id: String,
    model_id: String,
    not_connected_reason: String,
}

fn private_provider_state(kernel: &MdxKernel) -> PrivateProviderState {
    let accepted = kernel
        .ledger()
        .entries()
        .iter()
        .rev()
        .find(|receipt| private_artifact_provider_receipt_allowed(receipt));
    if let Some(receipt) = accepted {
        return PrivateProviderState {
            connected: true,
            provider_id: payload_value(receipt, "provider_id"),
            model_id: payload_value(receipt, "model_id"),
            not_connected_reason: String::new(),
        };
    }
    PrivateProviderState {
        connected: false,
        provider_id: "ollama".to_string(),
        model_id: String::new(),
        not_connected_reason: "connect_private_model".to_string(),
    }
}

fn private_artifact_provider_receipt_allowed(receipt: &mdx_core::Receipt) -> bool {
    receipt.kind == "twin.model_gateway.provider.observed"
        && payload_value(receipt, "provider_id") == "ollama"
        && payload_value(receipt, "provider_connected_local_allowed") == "true"
        && payload_value(receipt, "output_text_recorded") == "false"
        && payload_value(receipt, "provider_secret_values_recorded") == "false"
}

fn local_private_model_interpretation(
    report: &mdx_core::TwinArtifactContextReport,
    body: &str,
) -> Option<String> {
    if report.provider_id != "ollama" || !report.provider_call_allowed {
        return None;
    }
    let artifact_text = field_or(body, "artifact_text", "");
    if artifact_text.trim().is_empty() {
        return None;
    }
    let company_context = field_or(body, "company_context", "");
    if report.trusted_context_used && company_context.trim().is_empty() {
        return None;
    }
    let tenant_id = field_or(body, "tenant_id", "tenant_local");
    let api_key = crate::secret_store::global()
        .provider_key_for_tenant(&tenant_id, "MDX_OLLAMA_API_KEY")
        .unwrap_or_else(|| "ollama-local".to_string());
    let base_url = std::env::var("OLLAMA_BASE_URL")
        .or_else(|_| std::env::var("MDX_OLLAMA_BASE_URL"))
        .unwrap_or_else(|_| "http://127.0.0.1:11434/v1".to_string());
    call_ollama_artifact_interpretation(
        &base_url,
        &api_key,
        &report.model_id,
        &report.artifact_name,
        &report.company_context_source,
        &artifact_text,
        &company_context,
    )
    .map_err(|error| {
        eprintln!("twin_artifact_context: local Ollama interpretation failed ({error})")
    })
    .ok()
    .filter(|text| !text.trim().is_empty())
}

fn call_ollama_artifact_interpretation(
    base_url: &str,
    api_key: &str,
    model: &str,
    artifact_name: &str,
    company_context_source: &str,
    artifact_text: &str,
    company_context: &str,
) -> Result<String, String> {
    let model = if model.trim().is_empty() {
        std::env::var("MDX_OLLAMA_MODEL").unwrap_or_else(|_| "qwen3.5:27b".to_string())
    } else {
        model.to_string()
    };
    let artifact_excerpt = artifact_text.chars().take(12_000).collect::<String>();
    let company_excerpt = company_context.chars().take(4_000).collect::<String>();
    let user_prompt = format!(
        "Read this uploaded artifact as a sharp colleague who understands our company context.\n\nArtifact name: {artifact_name}\nCompany context source: {company_context_source}\n\nCompany context:\n{company_excerpt}\n\nUploaded artifact text:\n{artifact_excerpt}\n\nGive a concise useful read: what matters, why it matters for this company, and the best next move. Keep artifact evidence and company context conceptually separate. Do not mention receipts, routes, policies, or internal governance."
    );
    let request = serde_json::json!({
        "model": model,
        "think": false,
        "stream": false,
        "messages": [
            {
                "role": "system",
                "content": "You are Twin, a private local work assistant. Be concrete, concise, and helpful. Never claim to inspect sources beyond the supplied artifact and company context."
            },
            { "role": "user", "content": user_prompt }
        ],
        "options": {
            "num_predict": 700
        }
    });
    let response = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(45))
        .build()
        .post(&ollama_native_chat_url(base_url))
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .send_string(&request.to_string())
        .map_err(|error| format!("transport: {error}"))?;
    let parsed: serde_json::Value = response
        .into_json()
        .map_err(|error| format!("parse: {error}"))?;
    parsed["message"]["content"]
        .as_str()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
        .ok_or_else(|| "empty local model interpretation".to_string())
}

fn ollama_native_chat_url(base_url: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if let Some(root) = trimmed.strip_suffix("/v1") {
        format!("{root}/api/chat")
    } else {
        format!("{trimmed}/api/chat")
    }
}

fn artifact_interpretation_store() -> &'static Mutex<HashMap<String, String>> {
    static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn store_artifact_interpretation(artifact_id: &str, interpretation: &str) {
    if let Ok(mut store) = artifact_interpretation_store().lock() {
        store.insert(artifact_id.to_string(), interpretation.to_string());
    }
}

fn artifact_interpretation_for(artifact_id: &str) -> Option<String> {
    artifact_interpretation_store()
        .lock()
        .ok()
        .and_then(|store| store.get(artifact_id).cloned())
}

fn latest_artifact_payload(
    kernel: &MdxKernel,
    artifact_id: &str,
    kind: &str,
    key: &str,
) -> Option<String> {
    kernel
        .ledger()
        .entries()
        .iter()
        .rev()
        .find(|receipt| {
            receipt.kind == kind && payload_value(receipt, "artifact_id") == artifact_id
        })
        .map(|receipt| payload_value(receipt, key).to_string())
}

fn payload_value(receipt: &mdx_core::Receipt, key: &str) -> String {
    receipt.payload.get(key).cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route_request_with_body;

    #[test]
    fn ollama_native_chat_url_normalizes_openai_compatible_base_url() {
        assert_eq!(
            ollama_native_chat_url("http://127.0.0.1:11434/v1"),
            "http://127.0.0.1:11434/api/chat"
        );
        assert_eq!(
            ollama_native_chat_url("http://127.0.0.1:11434"),
            "http://127.0.0.1:11434/api/chat"
        );
    }

    #[test]
    fn twin_artifact_context_route_admits_parses_and_grounds_without_raw_receipts() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_context_route_download","artifact_name":"customer-risk.pdf","mime_type":"application/pdf","artifact_text":"Customer risk increased after onboarding handoff delays.","company_context":"Pages says onboarding clarity is the active Q3 customer-health bet.","company_context_source":"Pages:Q3 customer-health decision"}"#,
            &kernel,
        )
        .expect("route")
        .body;
        assert!(response.contains("\"name\":\"mdx-twin-artifact-context-local-post\""));
        assert!(response.contains("\"status\":\"TWIN_ARTIFACT_CONTEXT_READY_LOCAL\""));
        assert!(response.contains("\"privacy_class\":\"user_private_sensitive\""));
        assert!(response.contains("\"storage_contract_state\":\"LOCAL_BLOB_STORED_BY_REFERENCE\""));
        assert!(response.contains("\"deletion_state\":\"ACTIVE\""));
        assert!(response.contains("\"download_allowed\":true"));
        assert!(response.contains("\"blob_bytes_stored\":true"));
        assert!(response.contains("\"frontier_raw_content_allowed\":false"));
        let artifact_id = studio::json_string_field(&response, "artifact_id").expect("artifact id");

        let download = route_request_with_body(
            "POST",
            "/twin/artifacts/downloads.json",
            &format!(
                r#"{{"tenant_id":"tenant_context_route_download","artifact_id":"{artifact_id}"}}"#
            ),
            &kernel,
        )
        .expect("download")
        .body;
        assert!(download.contains("\"name\":\"mdx-twin-artifact-download-local-post\""));
        assert!(download.contains("\"status\":\"TWIN_ARTIFACT_DOWNLOAD_READY_LOCAL\""));
        assert!(download.contains("\"download_allowed\":true"));
        assert!(download.contains("\"owner_authorized\":true"));
        assert!(download.contains("\"raw_private_content_recorded\":false"));
        assert!(
            download.contains("Customer risk increased after onboarding handoff delays."),
            "download body: {download}"
        );

        let projection = route_request_with_body(
            "GET",
            "/twin/artifact-contexts/projection.json",
            "",
            &kernel,
        )
        .expect("projection")
        .body;
        assert!(projection.contains("\"artifact_count\":1"));
        assert!(projection.contains("\"storage_receipt_id\":\""));
        assert!(projection.contains("\"derived_context_valid\":true"));
        assert!(
            projection.contains("\"company_context_source\":\"Pages:Q3 customer-health decision\"")
        );

        let kernel = kernel.read().expect("kernel");
        assert!(kernel.ledger().entries().iter().any(|receipt| {
            receipt.kind == "twin.artifact.admitted"
                && receipt.payload.get("raw_text_recorded").map(String::as_str) == Some("false")
        }));
        assert!(kernel.ledger().entries().iter().any(|receipt| {
            receipt.kind == "twin.artifact.company_context.grounded"
                && receipt
                    .payload
                    .get("memory_write_performed")
                    .map(String::as_str)
                    == Some("false")
        }));
        assert!(kernel.ledger().entries().iter().any(|receipt| {
            receipt.kind == "twin.artifact.storage.declared"
                && receipt.payload.get("blob_bytes_stored").map(String::as_str) == Some("false")
        }));
        assert!(kernel.ledger().entries().iter().any(|receipt| {
            receipt.kind == "twin.artifact.storage.bytes_stored"
                && receipt
                    .payload
                    .get("source_id")
                    .map(String::as_str)
                    .is_some()
                && receipt.payload.get("blob_bytes_stored").map(String::as_str) == Some("true")
                && receipt.payload.get("raw_text_recorded").map(String::as_str) == Some("false")
        }));
        assert!(kernel.ledger().entries().iter().any(|receipt| {
            receipt.kind == "twin.artifact.download.authorized"
                && receipt.payload.get("download_allowed").map(String::as_str) == Some("true")
                && receipt
                    .payload
                    .get("download_body_recorded")
                    .map(String::as_str)
                    == Some("false")
        }));
        assert!(!kernel.ledger().entries().iter().any(|receipt| {
            receipt
                .payload
                .values()
                .any(|value| value.contains("Customer risk increased"))
        }));
    }

    #[test]
    fn twin_artifact_context_cites_trusted_pages_context_sources() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let source = route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_context_route","artifact_name":"architecture-standard.md","mime_type":"text/markdown","artifact_text":"Architecture standards require a shared Source and Artifact rail.","company_context":"Pages says source provenance must stay receipt backed.","company_context_source":"Pages:Context source rail contract"}"#,
            &kernel,
        )
        .expect("source")
        .body;
        let source_id = studio::json_string_field(&source, "artifact_id").expect("artifact id");
        let trust = route_request_with_body(
            "POST",
            "/pages/context-sources/trust-decisions.json",
            &format!(
                r#"{{"tenant_id":"tenant_context_route","source_id":"{source_id}","decision_note":"Approved as architecture context."}}"#
            ),
            &kernel,
        )
        .expect("trust")
        .body;
        let trust_receipt_id =
            studio::json_string_field(&trust, "trust_decision_receipt_id").expect("trust receipt");

        let read = route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_context_route","artifact_name":"implementation-plan.md","mime_type":"text/markdown","artifact_text":"The implementation plan should follow the architecture source rail."}"#,
            &kernel,
        )
        .expect("read")
        .body;
        for expected in [
            "\"trusted_context_used\":true",
            "\"trusted_context_source_count\":1",
            "\"trusted_context_projection_route\":\"/pages/context-sources/projection.json\"",
            "\"company_context_source\":\"Pages:trusted Context sources\"",
            "\"frontier_raw_content_allowed\":false",
            "\"memory_write_performed\":false",
        ] {
            assert!(read.contains(expected), "{expected}: {read}");
        }
        assert!(read.contains(&source_id), "{read}");
        assert!(read.contains(&trust_receipt_id), "{read}");

        let projection = route_request_with_body(
            "GET",
            "/twin/artifact-contexts/projection.json",
            "",
            &kernel,
        )
        .expect("projection")
        .body;
        assert!(projection.contains("\"trusted_context_used\":true"));
        assert!(projection.contains("\"trusted_context_source_count\":1"));
        assert!(projection.contains(&source_id));
        assert!(projection.contains(&trust_receipt_id));

        route_request_with_body(
            "POST",
            "/twin/artifacts/deletions.json",
            &format!(r#"{{"tenant_id":"tenant_context_route","artifact_id":"{source_id}"}}"#),
            &kernel,
        )
        .expect("delete");
        let after_delete = route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_context_route","artifact_name":"second-plan.md","mime_type":"text/markdown","artifact_text":"The second plan should not cite the deleted source."}"#,
            &kernel,
        )
        .expect("read after delete")
        .body;
        assert!(after_delete.contains("\"trusted_context_used\":false"));
        assert!(after_delete.contains("\"trusted_context_source_count\":0"));
    }

    #[test]
    fn twin_artifact_context_cites_trusted_connector_context_sources() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let source = route_request_with_body(
            "POST",
            "/connectors/items.json",
            r#"{"tenant_id":"tenant_context_route","source_id":"github:mdx-stack","source_kind":"github","external_ref":"https://github.com/mdx-stack/docs/architecture.md","title":"Architecture guide","summary":"Architecture standards require a shared Source and Artifact rail.","grade":"operational","scope":"tenant"}"#,
            &kernel,
        )
        .expect("connector source")
        .body;
        let source_id = studio::json_string_field(&source, "item_receipt_id").expect("receipt id");
        let trust = route_request_with_body(
            "POST",
            "/pages/context-sources/trust-decisions.json",
            &format!(
                r#"{{"tenant_id":"tenant_context_route","source_id":"{source_id}","decision_note":"Approved as connector architecture context."}}"#
            ),
            &kernel,
        )
        .expect("trust")
        .body;
        let trust_receipt_id =
            studio::json_string_field(&trust, "trust_decision_receipt_id").expect("trust receipt");

        let read = route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_context_route","artifact_name":"implementation-plan.md","mime_type":"text/markdown","artifact_text":"The implementation plan should follow company architecture context."}"#,
            &kernel,
        )
        .expect("read")
        .body;
        for expected in [
            "\"trusted_context_used\":true",
            "\"trusted_context_source_count\":1",
            "\"trusted_context_projection_route\":\"/pages/context-sources/projection.json\"",
            "\"company_context_source\":\"Pages:trusted Context sources\"",
            "\"frontier_raw_content_allowed\":false",
            "\"memory_write_performed\":false",
        ] {
            assert!(read.contains(expected), "{expected}: {read}");
        }
        assert!(read.contains(&source_id), "{read}");
        assert!(read.contains(&trust_receipt_id), "{read}");

        route_request_with_body(
            "POST",
            "/connectors/items/deletions.json",
            &format!(
                r#"{{"tenant_id":"tenant_context_route","item_receipt_id":"{source_id}","reason":"Delete trusted connector context."}}"#
            ),
            &kernel,
        )
        .expect("delete connector source");
        let after_delete = route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_context_route","artifact_name":"second-plan.md","mime_type":"text/markdown","artifact_text":"The second plan should not cite deleted connector context."}"#,
            &kernel,
        )
        .expect("read after delete")
        .body;
        assert!(after_delete.contains("\"trusted_context_used\":false"));
        assert!(after_delete.contains("\"trusted_context_source_count\":0"));
    }

    #[test]
    fn twin_artifact_runtime_route_exposes_claude_handoff_boundary() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_request_with_body("GET", "/twin/artifact-runtime.json", "", &kernel)
            .expect("runtime")
            .body;
        assert!(response.contains("\"name\":\"mdx-twin-artifact-runtime\""));
        assert!(response.contains("\"target_owner\":\"claude\""));
        assert!(response.contains("\"parser_sandbox_required\":true"));
        assert!(
            response.contains("\"parser_sandbox_route\":\"/twin/parser-sandbox-runtime.json\"")
        );
        assert!(response.contains("\"liteparse_route\":\"/twin/artifacts/liteparse-parses.json\""));
    }

    #[test]
    fn twin_parser_sandbox_runtime_declares_pdf_ready_and_other_readers_gated() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response =
            route_request_with_body("GET", "/twin/parser-sandbox-runtime.json", "", &kernel)
                .expect("parser sandbox runtime")
                .body;
        assert!(response.contains("\"name\":\"mdx-twin-parser-sandbox-runtime\""));
        assert!(response.contains("\"status\":\"LITEPARSE_PDF_READY_LOCAL\""));
        assert!(response.contains("\"liteparse_route\":\"/twin/artifacts/liteparse-parses.json\""));
        assert!(response.contains("\"binary_parsing_live\":true"));
        assert!(response.contains("\"pdf_parsing_live\":true"));
        assert!(response.contains("\"office_parsing_live\":false"));
        assert!(response.contains("\"ocr_parsing_live\":false"));
        assert!(response.contains("\"out_of_process_required\":true"));
        assert!(response.contains("\"application/pdf\""));
        assert!(response.contains("\"scanned_pdf_ocr\""));
        assert!(response.contains("\"frontier_raw_content_allowed\":false"));
    }

    #[test]
    fn liteparse_route_parses_pdf_locally_without_raw_receipts() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_request_with_body(
            "POST",
            "/twin/artifacts/liteparse-parses.json",
            r#"{"tenant_id":"tenant_liteparse_route","artifact_id":"twin_liteparse_pdf_test","artifact_name":"q3-retention-brief.pdf"}"#,
            &kernel,
        )
        .expect("liteparse")
        .body;
        assert!(response.contains("\"name\":\"mdx-twin-artifact-liteparse-local-post\""));
        assert!(response.contains("\"status\":\"TWIN_ARTIFACT_LITEPARSE_PDF_PARSED_LOCAL\""));
        assert!(response.contains("\"parser_id\":\"liteparse\""));
        assert!(response.contains("\"parser_version\":\"2.1.1\""));
        assert!(response.contains("\"parse_state\":\"PARSED_LOCAL_PDF_LITEPARSE\""));
        assert!(response.contains("\"ocr_enabled\":false"));
        assert!(response.contains("\"source_body_ref\":\".mdx-local/twin-artifacts/tenant_liteparse_route/twin_liteparse_pdf_test/source.pdf\""));
        assert!(response.contains("\"extracted_text_ref\":\".mdx-local/twin-artifacts/tenant_liteparse_route/twin_liteparse_pdf_test/extracted.txt\""));
        assert!(response.contains("\"extracted_text\":\""));
        assert!(response.contains("Onboarding add-on conflicts with the Q3 retention decision"));
        assert!(response.contains("\"raw_private_content_recorded\":false"));
        assert!(response.contains("\"provider_call_allowed\":false"));
        assert!(response.contains("\"frontier_raw_content_allowed\":false"));
        assert!(response.contains("\"memory_write_performed\":false"));
        assert!(response.contains("\"production_write_allowed\":false"));
        let kernel = kernel.read().expect("kernel");
        assert!(kernel.ledger().entries().iter().any(|receipt| {
            receipt.kind == "twin.artifact.parser.liteparse_pdf.parsed"
                && receipt.payload.get("raw_text_recorded").map(String::as_str) == Some("false")
                && receipt.payload.get("raw_blob_recorded").map(String::as_str) == Some("false")
                && receipt
                    .payload
                    .get("frontier_raw_content_allowed")
                    .map(String::as_str)
                    == Some("false")
        }));
        drop(kernel);
        let _ = crate::twin_artifact_blob_store::delete_artifact(
            "tenant_liteparse_route",
            "twin_liteparse_pdf_test",
        );
    }

    #[test]
    fn twin_artifact_delete_route_tombstones_storage_and_invalidates_context() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let admitted = route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_delete_route","artifact_name":"delete-me.pdf","mime_type":"application/pdf","artifact_text":"Delete route should not leak this text."}"#,
            &kernel,
        )
        .expect("admit")
        .body;
        assert!(admitted.contains("\"artifact_id\":\"twin_artifact_"));

        let deleted = route_request_with_body(
            "POST",
            "/twin/artifacts/deletions.json",
            r#"{"tenant_id":"tenant_delete_route","reason":"I no longer need this file."}"#,
            &kernel,
        )
        .expect("delete")
        .body;
        assert!(deleted.contains("\"name\":\"mdx-twin-artifact-deletion-local-post\""));
        assert!(deleted.contains("\"status\":\"TWIN_ARTIFACT_DELETED_LOCAL_PROPAGATED\""));
        assert!(deleted.contains("\"blob_delete_required\":true"));
        assert!(deleted.contains("\"local_blob_bytes_deleted\":true"));
        assert!(deleted.contains("\"context_packets_invalidated\":true"));

        let projection = route_request_with_body(
            "GET",
            "/twin/artifact-contexts/projection.json",
            "",
            &kernel,
        )
        .expect("projection")
        .body;
        assert!(projection.contains("\"deleted_artifact_count\":1"));
        assert!(projection.contains("\"parse_state\":\"DELETED\""));
        assert!(projection.contains("\"deletion_state\":\"DELETED_LOCAL_PROPAGATED\""));
        assert!(projection.contains("\"storage_contract_state\":\"LOCAL_BLOB_DELETED\""));
        assert!(projection.contains("\"blob_bytes_stored\":false"));
        assert!(projection.contains("\"derived_context_valid\":false"));

        let kernel = kernel.read().expect("kernel");
        assert!(kernel.ledger().entries().iter().any(|receipt| {
            receipt.kind == "twin.artifact.storage_object.tombstoned"
                && receipt
                    .payload
                    .get("local_blob_bytes_deleted")
                    .map(String::as_str)
                    == Some("true")
        }));
        assert!(kernel.ledger().entries().iter().any(|receipt| {
            receipt.kind == "twin.artifact.context_packet.invalidated"
                && receipt
                    .payload
                    .get("memory_atoms_invalidated")
                    .map(String::as_str)
                    == Some("false")
        }));
        assert!(!kernel.ledger().entries().iter().any(|receipt| {
            receipt
                .payload
                .values()
                .any(|value| value.contains("Delete route should not leak"))
        }));
    }

    #[test]
    fn twin_artifact_pages_draft_route_creates_pages_draft_and_approval_request() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_pages_draft","artifact_name":"save-me.pdf","mime_type":"application/pdf","artifact_text":"The artifact says onboarding needs a clearer owner.","company_context":"Pages says onboarding clarity is the active Q3 customer-health bet.","company_context_source":"Pages:Q3 customer-health decision"}"#,
            &kernel,
        )
        .expect("admit");

        let response =
            route_request_with_body("POST", "/twin/artifacts/pages-drafts.json", "{}", &kernel)
                .expect("pages draft")
                .body;
        for expected in [
            "\"name\":\"mdx-twin-artifact-pages-draft-local-post\"",
            "\"status\":\"TWIN_ARTIFACT_PAGES_DRAFT_CREATED_LOCAL\"",
            "\"pages_route\":\"/pages\"",
            "\"draft_state\":\"pending_approval\"",
            "\"approval_state\":\"PENDING_DECISION\"",
            "\"production_publish_allowed\":false",
            "\"frontier_raw_content_allowed\":false",
            "\"memory_write_performed\":false",
        ] {
            assert!(response.contains(expected), "{expected}; body: {response}");
        }

        let pages_drafts =
            route_request_with_body("GET", "/pages/edit-drafts/projection.json", "", &kernel)
                .expect("pages drafts")
                .body;
        assert!(pages_drafts.contains("\"draft_count\":1"));
        assert!(pages_drafts.contains("\"document_id\":\"page_twin_artifact_"));
        assert!(pages_drafts.contains("\"title\":\"Twin Read - save-me.pdf\""));

        let approvals = route_request_with_body(
            "GET",
            "/pages/approval-requests/projection.json",
            "",
            &kernel,
        )
        .expect("approval requests")
        .body;
        assert!(approvals.contains("\"approval_request_count\":1"));
        assert!(approvals.contains("\"decision_state\":\"PENDING_DECISION\""));

        let kernel = kernel.read().expect("kernel");
        assert!(kernel.ledger().entries().iter().any(|receipt| {
            receipt.kind == "twin.artifact.pages_draft.created"
                && receipt
                    .payload
                    .get("artifact_citation_label")
                    .map(String::as_str)
                    == Some("uploaded artifact")
                && receipt
                    .payload
                    .get("company_context_citation_label")
                    .map(String::as_str)
                    == Some("company context")
                && receipt
                    .payload
                    .get("production_publish_allowed")
                    .map(String::as_str)
                    == Some("false")
        }));
        assert!(!kernel.ledger().entries().iter().any(|receipt| {
            receipt
                .payload
                .values()
                .any(|value| value.contains("onboarding needs a clearer owner"))
        }));
    }

    #[test]
    fn tensorzero_observation_does_not_unlock_sensitive_artifact_interpretation() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_request_with_body(
            "POST",
            "/twin/model-gateway-provider-observations.json",
            r#"{"tenant_id":"tenant_local","actor_id":"local_operator","provider_id":"tensorzero","adapter":"TensorZeroModelGateway","receipt_kind":"tensorzero.inference.observed","approval_receipt_id":"human_approval_receipt_000002","evidence_file":".mdx-local/provider-turn-on/tensorzero-inference-observed.json","model_id":"mdx-private-reader","response_id":"tz_resp_000001","response_status":"completed","observed":true,"provider_call_attempted":true,"network_call_attempted":true,"credential_presence_only":true,"credential_values_recorded":false,"provider_secret_values_recorded":false,"requested_secret_values_recorded":false,"output_text_recorded":false,"production_write_allowed":false,"total_tokens":19}"#,
            &kernel,
        )
        .expect("provider observation");

        let response = route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_tensorzero_refusal","artifact_name":"risk-note.md","mime_type":"text/markdown","artifact_text":"Retention risk is rising because implementation ownership is unclear. Customers need one named owner by Friday.","company_context":"Pages says customer-health ownership is the active Q3 operating bet.","company_context_source":"Pages:Q3 customer-health decision"}"#,
            &kernel,
        )
        .expect("artifact context")
        .body;
        for expected in [
            "\"provider_id\":\"\"",
            "\"model_id\":\"\"",
            "\"sovereign_provider_connected\":false",
            "\"provider_not_connected_reason\":\"connect_private_model\"",
            "\"provider_call_allowed\":false",
        ] {
            assert!(response.contains(expected), "{expected}; body: {response}");
        }
    }

    #[test]
    fn ollama_observation_unlocks_private_artifact_interpretation_without_receipt_text() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        route_request_with_body(
            "POST",
            "/twin/model-gateway-provider-observations.json",
            r#"{"tenant_id":"tenant_local","actor_id":"local_operator","provider_id":"ollama","adapter":"OllamaLocalModelGateway","receipt_kind":"ollama.chat.observed","approval_receipt_id":"human_approval_receipt_000002","evidence_file":"gui:install-model-connect","model_id":"qwen3.5:27b","response_id":"ollama_resp_000001","response_status":"completed","observed":true,"provider_call_attempted":true,"network_call_attempted":true,"credential_presence_only":true,"credential_values_recorded":false,"provider_secret_values_recorded":false,"requested_secret_values_recorded":false,"output_text_recorded":false,"production_write_allowed":false,"total_tokens":19}"#,
            &kernel,
        )
        .expect("provider observation");

        let response = route_request_with_body(
            "POST",
            "/twin/artifact-contexts.json",
            r#"{"tenant_id":"tenant_ollama_read","artifact_name":"risk-note.md","mime_type":"text/markdown","artifact_text":"Retention risk is rising because implementation ownership is unclear. Customers need one named owner by Friday.","company_context":"Pages says customer-health ownership is the active Q3 operating bet.","company_context_source":"Pages:Q3 customer-health decision"}"#,
            &kernel,
        )
        .expect("artifact context")
        .body;
        for expected in [
            "\"provider_class\":\"local\"",
            "\"provider_id\":\"ollama\"",
            "\"model_id\":\"qwen3.5:27b\"",
            "\"sovereign_provider_required\":true",
            "\"sovereign_provider_connected\":true",
            "\"provider_call_allowed\":true",
            "\"frontier_raw_content_allowed\":false",
        ] {
            assert!(response.contains(expected), "{expected}; body: {response}");
        }

        let pages =
            route_request_with_body("POST", "/twin/artifacts/pages-drafts.json", "{}", &kernel)
                .expect("pages draft")
                .body;
        assert!(pages.contains("\"status\":\"TWIN_ARTIFACT_PAGES_DRAFT_CREATED_LOCAL\""));

        let kernel = kernel.read().expect("kernel");
        assert!(kernel.ledger().entries().iter().any(|receipt| {
            receipt.kind == "twin.artifact.company_context.grounded"
                && receipt.payload.get("provider_id").map(String::as_str) == Some("ollama")
                && receipt.payload.get("model_id").map(String::as_str) == Some("qwen3.5:27b")
                && receipt
                    .payload
                    .get("raw_private_content_routed_to_frontier")
                    .map(String::as_str)
                    == Some("false")
                && receipt
                    .payload
                    .get("provider_output_text_recorded")
                    .map(String::as_str)
                    == Some("false")
        }));
        assert!(!kernel.ledger().entries().iter().any(|receipt| {
            receipt
                .payload
                .values()
                .any(|value| value.contains("implementation ownership is unclear"))
        }));
    }
}
