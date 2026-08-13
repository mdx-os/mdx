use mdx_core::{
    MdxKernel, Receipt, json_string_literal,
    pages::{
        PagesOnboardingDocument, pages_onboarding_document_body_by_id,
        pages_onboarding_document_by_id, pages_onboarding_documents,
    },
};
use std::collections::BTreeSet;

struct PageDocument {
    document_id: &'static str,
    title: &'static str,
    body_ref: &'static str,
    author_actor_id: &'static str,
    source_receipt_ids: &'static [&'static str],
    charter_evidence_ids: &'static [&'static str],
    revision_id: &'static str,
}

const EVALS_RUNNER_PAGE: PageDocument = PageDocument {
    document_id: "page_evidence_evals_runner",
    title: "Evals Runner Evidence Summary",
    body_ref: "world_model://pages/page_evidence_evals_runner/body/v1",
    author_actor_id: "loop:evals_runner_agent",
    source_receipt_ids: &[
        "receipt_evals_runner_completed",
        "receipt_credential_minted",
    ],
    charter_evidence_ids: &["charter_evals_runner_attested"],
    revision_id: "rev_001",
};

pub(crate) fn render_pages_list_json(kernel: &MdxKernel) -> String {
    let mut documents = vec![render_pages_document_object(&EVALS_RUNNER_PAGE)];
    documents.extend(
        pages_onboarding_documents()
            .iter()
            .map(render_onboarding_pages_document_object),
    );
    documents.extend(
        crate::pages_authored::authored_documents(kernel)
            .iter()
            .map(crate::pages_authored::render_authored_document_object),
    );
    let count = documents.len();
    let documents = documents.join(",\n    ");
    format!(
        r#"{{
  "surface": "pages",
  "projection": "document_list",
  "tenant_id": "tenant_local",
  "count": {},
  "documents": [
    {}
  ],
  "writes_allowed": false,
  "receipt_backed": true,
  "source_contracts": [
    "generated/world-model/pages-projection-contract.json",
    "generated/world-model/pages-projection-fixtures.json",
    "generated/routes/pages-readonly-route-contract.json"
  ]
}}"#,
        count, documents
    )
}

pub(crate) fn render_pages_document_json(_kernel: &MdxKernel, document_id: &str) -> String {
    if document_id == EVALS_RUNNER_PAGE.document_id {
        return render_pages_document_object(&EVALS_RUNNER_PAGE);
    }
    pages_onboarding_document_by_id(document_id)
        .map(render_onboarding_pages_document_object)
        .unwrap_or_else(|| "{}".to_string())
}

pub(crate) fn render_pages_document_body_json(kernel: &MdxKernel, document_id: &str) -> String {
    if document_id == EVALS_RUNNER_PAGE.document_id {
        return render_pages_body_object(
            EVALS_RUNNER_PAGE.document_id,
            EVALS_RUNNER_PAGE.title,
            EVALS_RUNNER_PAGE.body_ref,
            EVALS_RUNNER_PAGE.source_receipt_ids,
            EVALS_RUNNER_PAGE.charter_evidence_ids,
            EVALS_RUNNER_PAGE.revision_id,
            "Evals Runner Evidence Summary is a receipt-backed local Pages fixture. It exists to prove that read surfaces can ground answers in World Model evidence before richer document storage opens.",
        );
    }
    if let Some(json) = crate::pages_authored::authored_body_json(kernel, document_id) {
        return json;
    }
    pages_onboarding_document_by_id(document_id)
        .zip(pages_onboarding_document_body_by_id(document_id))
        .map(|(document, body)| {
            render_pages_body_object(
                document.document_id,
                document.title,
                document.body_ref,
                document.source_receipt_ids,
                document.charter_evidence_ids,
                document.revision_id,
                body,
            )
        })
        .unwrap_or_else(|| "{}".to_string())
}

pub(crate) fn is_known_pages_document(document_id: &str) -> bool {
    document_id == EVALS_RUNNER_PAGE.document_id
        || pages_onboarding_document_by_id(document_id).is_some()
}

pub(crate) fn has_known_pages_document_body(document_id: &str) -> bool {
    document_id == EVALS_RUNNER_PAGE.document_id
        || pages_onboarding_document_body_by_id(document_id).is_some()
}

pub(crate) fn render_message_threads_json(kernel: &MdxKernel) -> String {
    let receipt_ids = projection_receipts(kernel);
    let latest_receipt_id = receipt_ids.last().map(String::as_str);
    let source_event_ids = source_event_ids(&receipt_ids);
    let messages = render_message_projection_items(kernel, &source_event_ids);
    let threads = if receipt_ids.is_empty() {
        String::new()
    } else {
        render_thread_summary_object(
            latest_receipt_id,
            receipt_ids.len(),
            receipt_ids.len(),
            participant_count(kernel),
            &messages,
        )
    };
    format!(
        r#"{{
  "surface": "message",
  "projection": "thread_summary",
  "tenant_id": "tenant_local",
  "thread_id": "thread_local_receipts",
  "channel_id": "local-ops",
  "latest_message_id": {},
  "latest_receipt_id": {},
  "message_count": {},
  "sequence_high_watermark": {},
  "thread_count": {},
  "threads": [
    {}
  ],
  "messages": [
    {}
  ],
  "source_event_ids": [{}],
  "receipt_ids": [{}],
  "writes_allowed": false,
  "receipt_backed": true,
  "llm_allowed_on_hot_path": false,
  "source_contracts": [
    "generated/world-model/message-event-plane-contract.json",
    "generated/world-model/message-thread-projection-contract.json",
    "generated/routes/message-readonly-route-contract.json"
  ]
}}"#,
        json_optional_message_id(latest_receipt_id),
        json_optional_string(latest_receipt_id),
        receipt_ids.len(),
        receipt_ids.len(),
        usize::from(!receipt_ids.is_empty()),
        threads,
        messages,
        json_owned_array(&source_event_ids),
        json_owned_array(&receipt_ids)
    )
}

pub(crate) fn render_message_channel_json(kernel: &MdxKernel, channel_id: &str) -> String {
    let receipt_ids = projection_receipts(kernel);
    let thread_ids = if receipt_ids.is_empty() {
        Vec::new()
    } else {
        vec!["thread_local_receipts".to_string()]
    };
    let source_event_ids = source_event_ids(&receipt_ids);
    let messages = render_message_projection_items(kernel, &source_event_ids);
    let channel_name = if channel_id == "local-ops" {
        "Local Ops"
    } else {
        channel_id
    };
    format!(
        r#"{{
  "surface": "message",
  "projection": "channel_timeline",
  "tenant_id": "tenant_local",
  "thread_id": "thread_local_receipts",
  "channel_id": {},
  "name": {},
  "thread_ids": [{}],
  "thread_count": {},
  "message_count": {},
  "participant_count": {},
  "latest_receipt_id": {},
  "sequence_high_watermark": {},
  "updated_at": "local-deterministic",
  "messages": [
    {}
  ],
  "source_event_ids": [{}],
  "receipt_ids": [{}],
  "writes_allowed": false,
  "receipt_backed": true,
  "llm_allowed_on_hot_path": false
}}"#,
        json_string_literal(channel_id),
        json_string_literal(channel_name),
        json_owned_array(&thread_ids),
        thread_ids.len(),
        receipt_ids.len(),
        participant_count(kernel),
        json_optional_string(receipt_ids.last().map(String::as_str)),
        receipt_ids.len(),
        messages,
        json_owned_array(&source_event_ids),
        json_owned_array(&receipt_ids)
    )
}

fn render_thread_summary_object(
    latest_receipt_id: Option<&str>,
    message_count: usize,
    sequence_high_watermark: usize,
    participant_count: usize,
    messages: &str,
) -> String {
    format!(
        r#"{{
      "thread_id": "thread_local_receipts",
      "channel_id": "local-ops",
      "title": "Local receipt trail",
      "latest_message_id": {},
      "latest_receipt_id": {},
      "message_count": {},
      "sequence_high_watermark": {},
      "participant_count": {},
      "messages": [
        {}
      ]
    }}"#,
        json_optional_message_id(latest_receipt_id),
        json_optional_string(latest_receipt_id),
        message_count,
        sequence_high_watermark,
        participant_count,
        messages
    )
}

fn render_message_projection_items(kernel: &MdxKernel, source_event_ids: &[String]) -> String {
    kernel
        .ledger()
        .entries()
        .iter()
        .enumerate()
        .map(|(index, receipt)| {
            let source_event_id = source_event_ids
                .get(index)
                .map(String::as_str)
                .unwrap_or("message_event_unknown");
            render_message_projection_item(receipt, index + 1, source_event_id)
        })
        .collect::<Vec<_>>()
        .join(",\n    ")
}

fn render_message_projection_item(
    receipt: &Receipt,
    sequence: usize,
    source_event_id: &str,
) -> String {
    let actor_id = receipt.actor_id.as_str();
    format!(
        r#"{{
      "message_id": {},
      "thread_id": "thread_local_receipts",
      "channel_id": "local-ops",
      "actor_id": {},
      "actor_type": {},
      "actor_display_name": {},
      "actor_role": {},
      "body": {},
      "receipt_id": {},
      "source_receipt_kind": {},
      "loop_id": {},
      "trace_id": {},
      "source_event_id": {},
      "sequence": {},
      "created_at": {},
      "writes_allowed": false,
      "receipt_backed": true
    }}"#,
        json_string_literal(&format!("message_for_{}", receipt.receipt_id)),
        json_string_literal(actor_id),
        json_string_literal(message_actor_type(actor_id)),
        json_string_literal(&message_actor_display_name(actor_id)),
        json_string_literal(message_actor_role(actor_id)),
        json_string_literal(&message_body(receipt)),
        json_string_literal(&receipt.receipt_id),
        json_string_literal(&receipt.kind),
        json_string_literal(receipt.loop_id.as_str()),
        json_string_literal(receipt.trace_id.as_str()),
        json_string_literal(source_event_id),
        sequence,
        json_string_literal(&format!("local-sequence-{sequence:06}"))
    )
}

fn message_body(receipt: &Receipt) -> String {
    format!(
        "{} recorded {} for {}. Receipt {} remains the source of truth.",
        receipt.actor_id.as_str(),
        receipt.kind,
        receipt.loop_id.as_str(),
        receipt.receipt_id
    )
}

fn message_actor_type(actor_id: &str) -> &'static str {
    if actor_id.starts_with("human:") {
        "human"
    } else if actor_id.starts_with("agent:") || actor_id.contains("_agent") {
        "agent"
    } else {
        "system"
    }
}

fn message_actor_display_name(actor_id: &str) -> String {
    actor_id
        .trim_start_matches("agent:")
        .trim_start_matches("human:")
        .replace(['_', ':'], " ")
}

fn message_actor_role(actor_id: &str) -> &'static str {
    match message_actor_type(actor_id) {
        "human" => "operator",
        "agent" => "agent",
        _ => "system",
    }
}

fn participant_count(kernel: &MdxKernel) -> usize {
    kernel
        .ledger()
        .entries()
        .iter()
        .map(|receipt| receipt.actor_id.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

fn render_pages_document_object(document: &PageDocument) -> String {
    render_page_document_object(
        document.document_id,
        document.title,
        document.body_ref,
        document.author_actor_id,
        document.source_receipt_ids,
        document.charter_evidence_ids,
        document.revision_id,
    )
}

fn render_onboarding_pages_document_object(document: &PagesOnboardingDocument) -> String {
    render_page_document_object(
        document.document_id,
        document.title,
        document.body_ref,
        document.author_actor_id,
        document.source_receipt_ids,
        document.charter_evidence_ids,
        document.revision_id,
    )
}

fn render_page_document_object(
    document_id: &str,
    title: &str,
    body_ref: &str,
    author_actor_id: &str,
    source_receipt_ids: &[&str],
    charter_evidence_ids: &[&str],
    revision_id: &str,
) -> String {
    format!(
        r#"{{
  "tenant_id": "tenant_local",
  "document_id": {},
  "title": {},
  "body_ref": {},
  "author_actor_id": {},
  "source_receipt_ids": [{}],
  "charter_evidence_ids": [{}],
  "revision_id": {},
  "visibility": "tenant_only",
  "receipt_backed": true,
  "world_model_source": "generated/world-model/pages-projection-fixtures.json"
}}"#,
        json_string_literal(document_id),
        json_string_literal(title),
        json_string_literal(body_ref),
        json_string_literal(author_actor_id),
        json_str_array(source_receipt_ids),
        json_str_array(charter_evidence_ids),
        json_string_literal(revision_id)
    )
}

pub(crate) fn render_pages_body_object(
    document_id: &str,
    title: &str,
    body_ref: &str,
    source_receipt_ids: &[&str],
    charter_evidence_ids: &[&str],
    revision_id: &str,
    body: &str,
) -> String {
    format!(
        r#"{{
  "tenant_id": "tenant_local",
  "document_id": {},
  "title": {},
  "body_ref": {},
  "source_receipt_ids": [{}],
  "charter_evidence_ids": [{}],
  "revision_id": {},
  "body": {},
  "content_type": "text/markdown; charset=utf-8",
  "visibility": "tenant_only",
  "writes_allowed": false,
  "receipt_backed": true,
  "body_ref_allowlisted": true,
  "standalone_store_allowed": false,
  "world_model_source": "generated/world-model/pages-projection-fixtures.json"
}}"#,
        json_string_literal(document_id),
        json_string_literal(title),
        json_string_literal(body_ref),
        json_str_array(source_receipt_ids),
        json_str_array(charter_evidence_ids),
        json_string_literal(revision_id),
        json_string_literal(body)
    )
}

fn projection_receipts(kernel: &MdxKernel) -> Vec<String> {
    kernel
        .ledger()
        .query()
        .receipt_ids()
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn source_event_ids(receipt_ids: &[String]) -> Vec<String> {
    receipt_ids
        .iter()
        .enumerate()
        .map(|(index, _)| format!("message_event_{:06}", index + 1))
        .collect()
}

fn json_optional_message_id(receipt_id: Option<&str>) -> String {
    receipt_id
        .map(|receipt_id| json_string_literal(&format!("message_for_{receipt_id}")))
        .unwrap_or_else(|| "null".to_string())
}

fn json_optional_string(value: Option<&str>) -> String {
    value
        .map(json_string_literal)
        .unwrap_or_else(|| "null".to_string())
}

fn json_owned_array(values: &[String]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(", ")
}

fn json_str_array(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(", ")
}
