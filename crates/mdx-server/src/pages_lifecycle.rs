// The page lifecycle, derived: receipts ARE the lifecycle (the Twin
// multi-turn lesson applied to knowledge). A page's state is the last
// thing that truly happened to it - draft saved, review asked, approved,
// needs work, published - and the timeline the surface renders is the
// receipt chain itself, never a parallel status field that could drift.
// Trust is computed the same way: source known, owner known, evidence
// counted, and what would make the page more trustworthy named plainly.
// Deprecated/archived are DECLARED states awaiting their own governed
// write contract - the projection lists them as such, never fakes them.
use crate::RouteResponse;
use mdx_core::{MdxKernel, pages::pages_onboarding_documents};
use std::sync::{Arc, RwLock};

const LIFECYCLE_KINDS: &[(&str, &str)] = &[
    ("pages.edit.draft.saved", "draft"),
    ("pages.approval.requested", "in_review"),
    ("pages.approval.decision.recorded", "decided"),
    ("pages.document.published", "published"),
];

pub(crate) fn route_response(
    method: &str,
    path: &str,
    _body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/pages/lifecycle/projection.json" && path != "/pages/agent-citations.json" {
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
    if path == "/pages/agent-citations.json" {
        return Some(Ok(RouteResponse::json(
            "200 OK",
            render_citations_json(&kernel),
        )));
    }
    Some(Ok(RouteResponse::json("200 OK", render_json(&kernel))))
}

fn payload_value<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt
        .payload
        .get(key)
        .map(String::as_str)
        .unwrap_or_default()
}

fn render_json(kernel: &MdxKernel) -> String {
    let documents = collect_documents(kernel);
    serde_json::json!({
        "name": "mdx-pages-lifecycle-projection",
        "status": "OK",
        "state_values": ["draft", "in_review", "approved", "needs_work", "published"],
        "declared_not_built_states": {
            "deprecated": "awaits its own governed write contract - never faked",
            "archived": "awaits its own governed write contract - never faked"
        },
        "document_count": documents.len(),
        "documents": documents,
    })
    .to_string()
}

// The agent-facing contract: which pages an agent may cite right now,
// and for everything else, exactly why not. One derivation - the same
// document entries the lifecycle projection renders - partitioned by
// the trusted_for_ai flag. Policy: generated/pages/pages-agent-citation-policy.json.
fn render_citations_json(kernel: &MdxKernel) -> String {
    let documents = collect_documents(kernel);
    let mut citable: Vec<serde_json::Value> = Vec::new();
    let mut excluded: Vec<serde_json::Value> = Vec::new();
    for document in documents {
        let trust = &document["trust"];
        if trust["trusted_for_ai"].as_bool() == Some(true) {
            // The receipt that earned the page in: its latest publish.
            // An agent citing the page cites this receipt with it.
            let published_receipt_id = document["events"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .rev()
                .find(|event| event["state"] == "published")
                .and_then(|event| event["receipt_id"].as_str())
                .unwrap_or_default()
                .to_string();
            citable.push(serde_json::json!({
                "document_id": document["document_id"],
                "state": document["state"],
                "owner": trust["owner"],
                "charter_backed": trust["charter_backed"],
                "evidence_count": trust["evidence_count"],
                "published_receipt_id": published_receipt_id,
            }));
        } else {
            excluded.push(serde_json::json!({
                "document_id": document["document_id"],
                "state": document["state"],
                "why_not": trust["more_trustworthy_with"],
            }));
        }
    }
    serde_json::json!({
        "name": "mdx-pages-agent-citations",
        "status": "OK",
        "policy": "generated/pages/pages-agent-citation-policy.json",
        "rule": "published AND source known AND named owner AND not past its review window",
        "human_line": "What an agent may cite right now - and for every page held back, exactly what would earn it in.",
        "citable_count": citable.len(),
        "citable": citable,
        "excluded_count": excluded.len(),
        "excluded": excluded,
    })
    .to_string()
}

fn collect_documents(kernel: &MdxKernel) -> Vec<serde_json::Value> {
    // Receipt-backed documents first: group lifecycle receipts by document.
    let mut order: Vec<String> = Vec::new();
    let mut events_by_document: std::collections::HashMap<String, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    let mut state_by_document: std::collections::HashMap<String, &'static str> =
        std::collections::HashMap::new();
    for receipt in kernel.ledger().entries() {
        let Some((_, base_state)) = LIFECYCLE_KINDS
            .iter()
            .find(|(kind, _)| *kind == receipt.kind)
        else {
            continue;
        };
        let document_id = payload_value(receipt, "document_id").to_string();
        if document_id.is_empty() {
            continue;
        }
        // A decision receipt resolves to approved or needs_work by its
        // recorded outcome - the receipt itself is the truth.
        let state: &'static str = if *base_state == "decided" {
            if payload_value(receipt, "decision_outcome") == "approved" {
                "approved"
            } else {
                "needs_work"
            }
        } else {
            base_state
        };
        let label = match state {
            "draft" => "Draft saved",
            "in_review" => "Review asked",
            "approved" => "Approved",
            "needs_work" => "Sent back",
            _ => "Published",
        };
        if !events_by_document.contains_key(&document_id) {
            order.push(document_id.clone());
        }
        events_by_document
            .entry(document_id.clone())
            .or_default()
            .push(serde_json::json!({
                "label": label,
                "state": state,
                "receipt_id": receipt.receipt_id,
                "actor_id": receipt.actor_id.as_str(),
                "created_at": payload_value(receipt, "created_at"),
                "note": payload_value(receipt, "decision_note"),
            }));
        state_by_document.insert(document_id, state);
    }

    let mut documents: Vec<serde_json::Value> = Vec::new();
    for document_id in &order {
        let state = state_by_document
            .get(document_id)
            .copied()
            .unwrap_or("draft");
        let events = events_by_document.remove(document_id).unwrap_or_default();
        documents.push(document_entry(document_id, state, events, false, 0));
    }
    // The declared corpus: charter-backed pages are published by
    // declaration, with their evidence counted.
    for document in pages_onboarding_documents() {
        if order.iter().any(|id| id == document.document_id) {
            continue;
        }
        documents.push(document_entry(
            document.document_id,
            "published",
            vec![serde_json::json!({
                "label": "Published by charter",
                "state": "published",
                "receipt_id": document.source_receipt_ids.first().copied().unwrap_or(""),
                "actor_id": document.author_actor_id,
                "created_at": "",
            })],
            true,
            document.charter_evidence_ids.len(),
        ));
    }

    documents
}

fn document_entry(
    document_id: &str,
    state: &str,
    events: Vec<serde_json::Value>,
    charter_backed: bool,
    evidence_count: usize,
) -> serde_json::Value {
    let stewardship = crate::pages_stewardship::Stewardship::load();
    let interval_days = stewardship.review_interval_days(document_id);
    let last_reviewed = stewardship.last_reviewed_epoch(document_id);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let next_due = last_reviewed + interval_days * 86_400;
    // A page never reviewed is not stale - it is unreviewed; staleness
    // begins once a review happened and the interval has passed.
    let stale = last_reviewed > 0 && now > next_due;
    let assigned_owner = stewardship.owner(document_id);
    let source_known = charter_backed
        || events
            .iter()
            .any(|event| event["state"] == "published" || event["state"] == "draft");
    let owner = if assigned_owner.is_empty() {
        events
            .first()
            .and_then(|event| event["actor_id"].as_str())
            .unwrap_or("")
            .to_string()
    } else {
        assigned_owner
    };
    let trusted_for_ai = state == "published" && source_known && !owner.is_empty() && !stale;
    let mut missing: Vec<&str> = Vec::new();
    if state != "published" {
        missing.push("a published revision");
    }
    if owner.is_empty() {
        missing.push("a named owner");
    }
    if stale {
        missing.push("a fresh review");
    }
    if !charter_backed && evidence_count == 0 && state != "published" {
        missing.push("evidence worth citing");
    }
    serde_json::json!({
        "document_id": document_id,
        "state": state,
        "events": events,
        "trust": {
            "source_known": source_known,
            "owner": owner,
            "reviewer": stewardship.entry(document_id)["reviewer"].as_str().unwrap_or(""),
            "charter_backed": charter_backed,
            "evidence_count": evidence_count,
            "trusted_for_ai": trusted_for_ai,
            "more_trustworthy_with": missing,
        },
        "freshness": {
            "review_interval_days": interval_days,
            "last_reviewed_epoch_seconds": last_reviewed,
            "next_due_epoch_seconds": if last_reviewed > 0 { next_due } else { 0 },
            "stale": stale,
        },
    })
}
