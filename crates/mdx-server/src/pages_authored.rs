use mdx_core::{MdxKernel, json_string_literal, pages::pages_onboarding_documents};

const EVALS_RUNNER_PAGE_ID: &str = "page_evidence_evals_runner";

pub(crate) struct AuthoredDocument {
    pub(crate) document_id: String,
    pub(crate) title: String,
    pub(crate) body_ref: String,
    pub(crate) author: String,
    pub(crate) revision_id: String,
    pub(crate) receipt_id: String,
    pub(crate) source_receipt_id: String,
    pub(crate) origin_receipt_id: String,
    pub(crate) origin_source_receipt_id: String,
}

pub(crate) fn authored_documents(kernel: &MdxKernel) -> Vec<AuthoredDocument> {
    let mut latest: Vec<AuthoredDocument> = Vec::new();
    for receipt in kernel.ledger().entries() {
        if receipt.kind != "pages.document.published" {
            continue;
        }
        let document_id = receipt
            .payload
            .get("document_id")
            .cloned()
            .unwrap_or_default();
        if document_id.is_empty()
            || document_id == EVALS_RUNNER_PAGE_ID
            || pages_onboarding_documents()
                .iter()
                .any(|doc| doc.document_id == document_id)
        {
            continue;
        }
        let origin_receipt_id = receipt
            .payload
            .get("origin_receipt_id")
            .cloned()
            .unwrap_or_default();
        let origin_source_receipt_id = kernel
            .ledger()
            .query()
            .by_id(&origin_receipt_id)
            .and_then(|origin| origin.payload.get("source_receipt_id"))
            .cloned()
            .unwrap_or_default();
        let entry = AuthoredDocument {
            document_id: document_id.clone(),
            title: receipt.payload.get("title").cloned().unwrap_or_default(),
            body_ref: receipt.payload.get("body_ref").cloned().unwrap_or_default(),
            author: receipt.actor_id.as_str().to_string(),
            revision_id: receipt
                .payload
                .get("revision_id")
                .cloned()
                .unwrap_or_default(),
            receipt_id: receipt.receipt_id.clone(),
            source_receipt_id: receipt
                .payload
                .get("source_receipt_id")
                .cloned()
                .unwrap_or_default(),
            origin_receipt_id,
            origin_source_receipt_id,
        };
        match latest.iter_mut().find(|doc| doc.document_id == document_id) {
            Some(existing) => *existing = entry,
            None => latest.push(entry),
        }
    }
    latest
}

pub(crate) fn render_authored_document_object(document: &AuthoredDocument) -> String {
    let source_receipt_ids = authored_source_receipt_ids(document)
        .into_iter()
        .map(json_string_literal)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{ "document_id": {}, "title": {}, "body_ref": {}, "category": "authored", "author_actor_id": {}, "source_receipt_ids": [{}], "charter_evidence_ids": [], "revision_id": {}, "body_route": {} }}"#,
        json_string_literal(&document.document_id),
        json_string_literal(&document.title),
        json_string_literal(&document.body_ref),
        json_string_literal(&document.author),
        source_receipt_ids,
        json_string_literal(&document.revision_id),
        json_string_literal(&format!("/pages/{}/body", document.document_id))
    )
}

pub(crate) fn authored_body_json(kernel: &MdxKernel, document_id: &str) -> Option<String> {
    let authored = authored_documents(kernel)
        .into_iter()
        .find(|doc| doc.document_id == document_id)?;
    let body = crate::pages_body_store::read_stored_body(&authored.body_ref)
        .or_else(|| activation_first_mission_brief_body(kernel, &authored))?;
    let source_receipt_ids = authored_source_receipt_ids(&authored);
    Some(crate::read_surfaces::render_pages_body_object(
        &authored.document_id,
        &authored.title,
        &authored.body_ref,
        &source_receipt_ids,
        &[],
        &authored.revision_id,
        &body,
    ))
}

fn authored_source_receipt_ids(document: &AuthoredDocument) -> Vec<&str> {
    let mut ids = Vec::new();
    for id in [
        document.receipt_id.as_str(),
        document.source_receipt_id.as_str(),
        document.origin_receipt_id.as_str(),
        document.origin_source_receipt_id.as_str(),
    ] {
        if !id.trim().is_empty() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

#[rustfmt::skip]
fn activation_first_mission_brief_body(kernel: &MdxKernel, authored: &AuthoredDocument) -> Option<String> {
    let mission_id = authored.body_ref.strip_prefix("activation://first-mission/")?.strip_suffix("/brief")?;
    let brief = if authored.source_receipt_id.trim().is_empty() { None } else { kernel.ledger().query().by_id(&authored.source_receipt_id) }.or_else(|| {
        kernel.ledger().entries().iter().rev().find(|receipt| receipt.kind == "activation.first_mission.brief_shaped" && receipt.payload.get("mission_id").map(String::as_str).unwrap_or("") == mission_id)
    })?;
    Some(crate::activation_route::first_mission_brief_body(brief.payload.get("goal").map(String::as_str).unwrap_or(""), brief.payload.get("approach").map(String::as_str).unwrap_or(""), brief.payload.get("acceptance").map(String::as_str).unwrap_or(""), brief.payload.get("proof_command").map(String::as_str).unwrap_or("")))
}
