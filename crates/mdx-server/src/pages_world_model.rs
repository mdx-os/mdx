// The World Model panel's truth: what a page rests on, what rests on it,
// and where the company is talking about it - every relationship DERIVED
// from recorded bodies and receipts, never asserted. No graph theater:
// the relationships that exist are real, and the ones that need their own
// contracts (Forge run links, contradiction detection) are declared
// not-built slots until those contracts land.
use crate::RouteResponse;
use mdx_core::{
    MdxKernel,
    pages::{pages_onboarding_document_body_by_id, pages_onboarding_documents},
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    _body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/pages/world-model/projection.json" {
        return None;
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
    Some(Ok(RouteResponse::json("200 OK", render_json(&kernel))))
}

struct KnownPage {
    document_id: String,
    title: String,
    body: String,
}

fn known_pages(kernel: &MdxKernel) -> Vec<KnownPage> {
    let mut pages: Vec<KnownPage> = pages_onboarding_documents()
        .iter()
        .filter_map(|document| {
            pages_onboarding_document_body_by_id(document.document_id).map(|body| KnownPage {
                document_id: document.document_id.to_string(),
                title: document.title.to_string(),
                body: body.to_string(),
            })
        })
        .collect();
    for authored in crate::pages_authored::authored_documents(kernel) {
        if let Some(body) = crate::pages_body_store::read_stored_body(&authored.body_ref) {
            pages.push(KnownPage {
                document_id: authored.document_id.clone(),
                title: authored.title.clone(),
                body,
            });
        }
    }
    pages
}

// Receipt-id shaped tokens in a body: lowercase words ending in
// _receipt_<digits> - the shape every kernel receipt id has.
fn receipt_citations(body: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for token in body.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if token.len() < 12 || found.iter().any(|known| known == token) {
            continue;
        }
        if let Some(index) = token.find("_receipt_") {
            let tail = &token[index + "_receipt_".len()..];
            if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
                found.push(token.to_string());
            }
        }
    }
    found
}

fn render_json(kernel: &MdxKernel) -> String {
    let pages = known_pages(kernel);
    // Page-to-page citations: a body naming another page's title or id.
    let mut cites_pages: Vec<(String, Vec<String>)> = Vec::new();
    for page in &pages {
        let mut cited: Vec<String> = Vec::new();
        for other in &pages {
            if other.document_id == page.document_id || other.title.len() < 8 {
                continue;
            }
            if page.body.contains(&other.title) || page.body.contains(&other.document_id) {
                cited.push(other.document_id.clone());
            }
        }
        cites_pages.push((page.document_id.clone(), cited));
    }

    // Conversation mentions: recorded messages naming the page's title.
    let mut documents: Vec<serde_json::Value> = Vec::new();
    for page in &pages {
        let receipts = receipt_citations(&page.body);
        let cited_pages = cites_pages
            .iter()
            .find(|(id, _)| *id == page.document_id)
            .map(|(_, cited)| cited.clone())
            .unwrap_or_default();
        let cited_by: Vec<String> = cites_pages
            .iter()
            .filter(|(id, cited)| *id != page.document_id && cited.contains(&page.document_id))
            .map(|(id, _)| id.clone())
            .collect();
        let mut thread_mentions: Vec<serde_json::Value> = Vec::new();
        let mut decision_mentions: Vec<serde_json::Value> = Vec::new();
        if page.title.len() >= 8 {
            for receipt in kernel.ledger().entries() {
                if receipt.kind != "message.human.posted" {
                    continue;
                }
                let body = receipt
                    .payload
                    .get("body")
                    .map(String::as_str)
                    .unwrap_or_default();
                if !body.contains(&page.title) {
                    continue;
                }
                let entry = serde_json::json!({
                    "receipt_id": receipt.receipt_id,
                    "channel_id": receipt.payload.get("channel_id").cloned().unwrap_or_default(),
                    "snippet": body.chars().take(120).collect::<String>(),
                });
                if body.starts_with("Decision: ") {
                    decision_mentions.push(entry);
                } else {
                    thread_mentions.push(entry);
                }
            }
        }
        documents.push(serde_json::json!({
            "document_id": page.document_id,
            "cites": { "receipts": receipts, "pages": cited_pages },
            "cited_by_pages": cited_by,
            "decision_mentions": decision_mentions,
            "thread_mentions": thread_mentions,
        }));
    }

    serde_json::json!({
        "name": "mdx-pages-world-model-projection",
        "status": "OK",
        "derivation": "every relationship read from recorded bodies and receipts - never asserted",
        "declared_not_built": {
            "forge_run_links": "lands when Forge runs carry page references in their contracts",
            "contradiction_detection": "needs CTX - never guessed lexically"
        },
        "document_count": documents.len(),
        "documents": documents,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::receipt_citations;

    #[test]
    fn receipt_shapes_only() {
        let body = "see twin_session_draft_receipt_000116 and message_thread_message_receipt_000040, not receipt_x or _receipt_ or plain_receipt_word";
        let found = receipt_citations(body);
        assert_eq!(
            found,
            vec![
                "twin_session_draft_receipt_000116".to_string(),
                "message_thread_message_receipt_000040".to_string()
            ]
        );
    }
}
