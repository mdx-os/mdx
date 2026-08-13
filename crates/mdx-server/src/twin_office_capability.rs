use base64::Engine;
use mdx_core::{
    MdxKernel, TWIN_OFFICE_NATIVE_PACK_ID, TWIN_OFFICE_NATIVE_PACK_VERSION,
    TwinOfficeDownloadRequest, TwinOfficeGeneratedArtifactRequest, hex, json_string_literal,
    sha256,
};
use ppt_rs::{Presentation, SlideContent};
use rust_xlsxwriter::Workbook;
use std::io::{Cursor, Write};
use std::sync::{Arc, RwLock};
use zip::write::FileOptions;

use crate::{RouteResponse, reject_unless_method};

const OFFICE_ARTIFACTS_DIR: &str = ".mdx-local/twin-office-artifacts";

pub(crate) fn route_request(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        "/twin/office-capability-runtime.json" => Some(render_runtime_route(method, kernel)),
        "/twin/office/generated-artifacts.json" => {
            Some(render_generate_route(method, body, kernel))
        }
        "/twin/office/downloads.json" => Some(render_download_route(method, body, kernel)),
        _ => None,
    }
}

pub(crate) fn render_local_command_json() -> Result<String, String> {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let artifact_kind =
        std::env::var("MDX_TWIN_OFFICE_KIND").unwrap_or_else(|_| "docx".to_string());
    let body = format!(r#"{{"artifact_kind":"{artifact_kind}"}}"#);
    render_generate_route("POST", &body, &kernel).map(|response| response.body)
}

pub(crate) fn render_local_download_command_json() -> Result<String, String> {
    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let artifact_kind =
        std::env::var("MDX_TWIN_OFFICE_KIND").unwrap_or_else(|_| "docx".to_string());
    let body = format!(r#"{{"artifact_kind":"{artifact_kind}"}}"#);
    render_generate_route("POST", &body, &kernel)?;
    render_download_route("POST", &body, &kernel).map(|response| response.body)
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
        render_office_capability_runtime_json(&kernel),
    ))
}

fn render_generate_route(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let tenant_id = field_or(body, "tenant_id", "tenant_local");
    let actor_id = field_or(body, "actor_id", "local_user");
    let artifact_kind = field_or(body, "artifact_kind", "docx");
    let title = field_or(body, "title", default_title(&artifact_kind));
    let body_text = field_or(
        body,
        "body_text",
        "Twin prepared this editable Office artifact locally. It is stored by reference and can be reviewed before it becomes company context.",
    );
    let artifact_id = sanitized_id(&field_or(
        body,
        "artifact_id",
        &format!("twin_office_{}_local_001", artifact_kind),
    ))
    .ok_or_else(|| "office artifact_id must be lowercase letters, digits, _ or -".to_string())?;
    let generated = generate_office_artifact(&artifact_kind, &title, &body_text)?;
    let stored =
        store_generated_artifact(&tenant_id, &artifact_id, &artifact_kind, &generated.bytes)
            .ok_or_else(|| "generated Office artifact could not be stored locally".to_string())?;
    let generation_spec_hash = format!(
        "sha256:{}",
        hex(&sha256(
            format!("{artifact_kind}|{title}|{body_text}").as_bytes()
        ))
    );
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = kernel
        .record_twin_office_generated_artifact_local(TwinOfficeGeneratedArtifactRequest {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            artifact_id: &artifact_id,
            artifact_kind: &artifact_kind,
            title: &title,
            output_ref: &stored.output_ref,
            output_hash: &stored.output_hash,
            output_bytes: stored.output_bytes,
            content_type: generated.content_type,
            engine_id: generated.engine_id,
            validation_status: stored.validation_status,
            generation_spec_hash: &generation_spec_hash,
        })
        .map_err(|error| error.message())?;
    Ok(RouteResponse::json("200 OK", report.render_post_json()))
}

pub(crate) fn render_office_capability_runtime_json(kernel: &MdxKernel) -> String {
    let generated_artifact_count = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "twin.office.generated_artifact.created")
        .count();
    format!(
        r#"{{"name":"mdx-twin-office-capability-runtime","status":"LOCAL_OFFICE_NATIVE_PACK_READY","route":"/twin/office-capability-runtime.json","generate_route":"/twin/office/generated-artifacts.json","download_route":"/twin/office/downloads.json","marketplace_pack_route":"/marketplace/packs/{}.json","capability_pack_id":{},"capability_pack_version":{},"capability_pack_flavor":"native","generated_artifact_count":{},"generation_live":true,"office_output_generation_live":true,"office_input_parsing_live":false,"docx_generation":{{"live":true,"engine_id":"docx_openxml_zip","content_type":"application/vnd.openxmlformats-officedocument.wordprocessingml.document"}},"xlsx_generation":{{"live":true,"engine_id":"rust_xlsxwriter","content_type":"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"}},"pptx_generation":{{"live":true,"engine_id":"ppt_rs","content_type":"application/vnd.openxmlformats-officedocument.presentationml.presentation"}},"reading_routes":{{"pdf":"/twin/artifacts/liteparse-parses.json","docx":"blocked_until_parser_sandbox","xlsx":"blocked_until_parser_sandbox","pptx":"blocked_until_parser_sandbox"}},"storage_contract_state":"LOCAL_OFFICE_ARTIFACT_STORED_BY_REFERENCE","download_allowed":true,"receipt_backed":true,"raw_private_content_recorded":false,"provider_call_allowed":false,"frontier_raw_content_allowed":false,"memory_write_performed":false,"production_write_allowed":false,"ui_handoff":{{"target_owner":"claude","safe_to_render":true,"copy_boundary":"Offer Office output as a simple next move. Do not show capability-pack jargon unless the user opens details. Keep uploaded Office reading in the safe-reader-not-ready state until the parser sandbox flips it live."}},"evidence":{{"proof_check":"make twin-office-capability-check","source_routes":["/twin/office-capability-runtime.json","/twin/office/generated-artifacts.json","/twin/office/downloads.json"],"marketplace_pack_id":{}}}}}"#,
        TWIN_OFFICE_NATIVE_PACK_ID,
        json_string_literal(TWIN_OFFICE_NATIVE_PACK_ID),
        json_string_literal(TWIN_OFFICE_NATIVE_PACK_VERSION),
        generated_artifact_count,
        json_string_literal(TWIN_OFFICE_NATIVE_PACK_ID)
    )
}

fn render_download_route(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let tenant_id = field_or(body, "tenant_id", "tenant_local");
    let actor_id = field_or(body, "actor_id", "local_user");
    let artifact_kind = field_or(body, "artifact_kind", "docx");
    let artifact_id = field_or(
        body,
        "artifact_id",
        &format!("twin_office_{}_local_001", artifact_kind),
    );
    let bytes = read_generated_artifact(&tenant_id, &artifact_id, &artifact_kind)
        .ok_or_else(|| "generated Office artifact bytes could not be read".to_string())?;
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let report = kernel
        .authorize_twin_office_download_local(TwinOfficeDownloadRequest {
            tenant_id: &tenant_id,
            actor_id: &actor_id,
            artifact_id: &artifact_id,
            artifact_kind: &artifact_kind,
        })
        .map_err(|error| error.message())?;
    let output_base64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(RouteResponse::json(
        "200 OK",
        report.render_download_json(&output_base64),
    ))
}

struct GeneratedOfficeArtifact {
    bytes: Vec<u8>,
    content_type: &'static str,
    engine_id: &'static str,
}

struct StoredOfficeArtifact {
    output_ref: String,
    output_hash: String,
    output_bytes: usize,
    validation_status: &'static str,
}

fn generate_office_artifact(
    artifact_kind: &str,
    title: &str,
    body_text: &str,
) -> Result<GeneratedOfficeArtifact, String> {
    match artifact_kind {
        "docx" => generate_docx(title, body_text),
        "xlsx" => generate_xlsx(title, body_text),
        "pptx" => generate_pptx(title, body_text),
        other => Err(format!("unsupported Office artifact kind: {other}")),
    }
}

fn generate_docx(title: &str, body_text: &str) -> Result<GeneratedOfficeArtifact, String> {
    let mut bytes = Vec::new();
    {
        let cursor = Cursor::new(&mut bytes);
        let mut zip = zip::ZipWriter::new(cursor);
        let options = FileOptions::default().compression_method(zip::CompressionMethod::Stored);
        write_zip_file(&mut zip, options, "[Content_Types].xml", CONTENT_TYPES_XML)?;
        write_zip_file(&mut zip, options, "_rels/.rels", ROOT_RELS_XML)?;
        write_zip_file(
            &mut zip,
            options,
            "word/document.xml",
            &render_docx_document_xml(title, body_text),
        )?;
        zip.finish()
            .map_err(|error| format!("docx generation failed: {error}"))?;
    }
    Ok(GeneratedOfficeArtifact {
        bytes,
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        engine_id: "docx_openxml_zip",
    })
}

const CONTENT_TYPES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
</Types>"#;

const ROOT_RELS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#;

fn write_zip_file(
    zip: &mut zip::ZipWriter<Cursor<&mut Vec<u8>>>,
    options: FileOptions,
    path: &str,
    content: &str,
) -> Result<(), String> {
    zip.start_file(path, options)
        .map_err(|error| format!("docx generation failed: {error}"))?;
    zip.write_all(content.as_bytes())
        .map_err(|error| format!("docx generation failed: {error}"))
}

fn render_docx_document_xml(title: &str, body_text: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>{}</w:t></w:r></w:p>
    <w:p><w:r><w:t>{}</w:t></w:r></w:p>
    <w:sectPr/>
  </w:body>
</w:document>"#,
        escape_xml_text(title),
        escape_xml_text(body_text)
    )
}

fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn generate_xlsx(title: &str, body_text: &str) -> Result<GeneratedOfficeArtifact, String> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();
    worksheet
        .set_name("Twin Draft")
        .map_err(|error| format!("xlsx sheet name failed: {error}"))?;
    worksheet
        .write_string(0, 0, "Title")
        .map_err(|error| format!("xlsx write failed: {error}"))?;
    worksheet
        .write_string(0, 1, title)
        .map_err(|error| format!("xlsx write failed: {error}"))?;
    worksheet
        .write_string(1, 0, "Draft")
        .map_err(|error| format!("xlsx write failed: {error}"))?;
    worksheet
        .write_string(1, 1, body_text)
        .map_err(|error| format!("xlsx write failed: {error}"))?;
    let bytes = workbook
        .save_to_buffer()
        .map_err(|error| format!("xlsx generation failed: {error}"))?;
    Ok(GeneratedOfficeArtifact {
        bytes,
        content_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        engine_id: "rust_xlsxwriter",
    })
}

fn generate_pptx(title: &str, body_text: &str) -> Result<GeneratedOfficeArtifact, String> {
    let bytes = Presentation::with_title(title)
        .add_slide(SlideContent::new(title).add_bullet(body_text))
        .into_bytes()
        .map_err(|error| format!("pptx generation failed: {error}"))?;
    Ok(GeneratedOfficeArtifact {
        bytes,
        content_type: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        engine_id: "ppt_rs",
    })
}

fn store_generated_artifact(
    tenant_id: &str,
    artifact_id: &str,
    artifact_kind: &str,
    bytes: &[u8],
) -> Option<StoredOfficeArtifact> {
    let tenant = sanitized_id(tenant_id)?;
    let artifact = sanitized_id(artifact_id)?;
    if !["docx", "xlsx", "pptx"].contains(&artifact_kind) || !looks_like_openxml_package(bytes) {
        return None;
    }
    let dir = format!("{OFFICE_ARTIFACTS_DIR}/{tenant}/{artifact}");
    std::fs::create_dir_all(&dir).ok()?;
    let output_ref = format!("{dir}/output.{artifact_kind}");
    std::fs::write(&output_ref, bytes).ok()?;
    Some(StoredOfficeArtifact {
        output_ref,
        output_hash: format!("sha256:{}", hex(&sha256(bytes))),
        output_bytes: bytes.len(),
        validation_status: "OPENXML_PACKAGE_BYTES_PRESENT",
    })
}

fn read_generated_artifact(
    tenant_id: &str,
    artifact_id: &str,
    artifact_kind: &str,
) -> Option<Vec<u8>> {
    let tenant = sanitized_id(tenant_id)?;
    let artifact = sanitized_id(artifact_id)?;
    if !["docx", "xlsx", "pptx"].contains(&artifact_kind) {
        return None;
    }
    let path = format!("{OFFICE_ARTIFACTS_DIR}/{tenant}/{artifact}/output.{artifact_kind}");
    std::fs::read(path).ok()
}

fn looks_like_openxml_package(bytes: &[u8]) -> bool {
    bytes.len() > 16 && bytes.starts_with(b"PK")
}

fn sanitized_id(value: &str) -> Option<String> {
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

fn default_title(artifact_kind: &str) -> &'static str {
    match artifact_kind {
        "docx" => "Twin Word Draft",
        "xlsx" => "Twin Workbook Draft",
        "pptx" => "Twin Deck Draft",
        _ => "Twin Office Draft",
    }
}

fn field_or(body: &str, key: &str, fallback: &str) -> String {
    let Some(after_key) = body.split_once(&format!("\"{key}\"")).map(|(_, rest)| rest) else {
        return fallback.to_string();
    };
    let Some(after_colon) = after_key.split_once(':').map(|(_, rest)| rest.trim_start()) else {
        return fallback.to_string();
    };
    let Some(rest) = after_colon.strip_prefix('"') else {
        return fallback.to_string();
    };
    match rest.split_once('"') {
        Some((value, _)) if !value.trim().is_empty() => value.to_string(),
        _ => fallback.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_artifacts_are_stored_as_openxml_packages() {
        for artifact_kind in ["docx", "xlsx", "pptx"] {
            let generated = generate_office_artifact(artifact_kind, "Title", "Body")
                .unwrap_or_else(|error| panic!("{artifact_kind}: {error}"));
            assert!(
                looks_like_openxml_package(&generated.bytes),
                "{artifact_kind} is OpenXML bytes"
            );
            let stored = store_generated_artifact(
                "tenant_test",
                &format!("office_{artifact_kind}_test"),
                artifact_kind,
                &generated.bytes,
            )
            .expect("stored");
            assert!(
                stored
                    .output_ref
                    .ends_with(&format!("output.{artifact_kind}"))
            );
            assert!(std::path::Path::new(&stored.output_ref).exists());
            assert_eq!(stored.validation_status, "OPENXML_PACKAGE_BYTES_PRESENT");
        }
    }
}
