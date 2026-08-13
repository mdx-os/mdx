use crate::RouteResponse;
use mdx_core::{
    GovernedWriteIdentity, MdxKernel, PagesEmbeddingProviderRefusal,
    PagesPublicationVisibilityCheck, PagesRevisionCitationComparison, PagesSearchPreflight,
    json_string_literal,
    pages::{
        PagesOnboardingDocument, pages_onboarding_document_body_by_id, pages_onboarding_documents,
    },
};
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if let Some(query) = pages_search_query(path) {
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
            render_pages_knowledge_search_json(&kernel, &query),
        )));
    }
    if path == "/pages/search-preflights.json" {
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
    if path == "/pages/search-preflights/projection.json" {
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
    if path == "/pages/revision-citation-comparisons.json" {
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
            render_revision_citation_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/pages/revision-citation-comparisons/projection.json" {
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
            render_revision_citation_projection_json(&kernel),
        )));
    }
    if path == "/pages/publication-visibility-checks.json" {
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
            render_visibility_check_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/pages/publication-visibility-checks/projection.json" {
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
            render_visibility_check_projection_json(&kernel),
        )));
    }
    if path == "/pages/embedding-provider-refusals.json" {
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
            render_embedding_refusal_post_json(body, &mut kernel)
                .map(|body| RouteResponse::json("200 OK", body)),
        );
    }
    if path == "/pages/embedding-provider-refusals/projection.json" {
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
            render_embedding_refusal_projection_json(&kernel),
        )));
    }
    None
}

fn pages_search_query(path: &str) -> Option<String> {
    let suffix = path.strip_prefix("/pages/search.json")?;
    if suffix.is_empty() {
        return Some("harness".to_string());
    }
    let query = suffix.strip_prefix('?')?;
    Some(
        query
            .split('&')
            .find_map(|pair| pair.strip_prefix("q=").map(percent_decode_query_value))
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "harness".to_string()),
    )
}

fn render_pages_knowledge_search_json(kernel: &MdxKernel, query: &str) -> String {
    let normalized_query = normalize_query(query);
    let terms = search_terms(&normalized_query);
    let mut hits = Vec::new();
    maybe_push_static_hit(
        &mut hits,
        &terms,
        StaticPageSearchDocument {
            document_id: "page_evidence_evals_runner",
            title: "Evals Runner Evidence Summary",
            body_ref: "world_model://pages/page_evidence_evals_runner/body/v1",
            summary: "Receipt-backed evidence that read surfaces can ground answers in World Model evidence before richer document storage opens.",
            category: "proof_surface",
            author_actor_id: "loop:evals_runner_agent",
            source_receipt_ids: &[
                "receipt_evals_runner_completed",
                "receipt_credential_minted",
            ],
            charter_evidence_ids: &["charter_evals_runner_attested"],
            revision_id: "rev_001",
            body: "Evals Runner Evidence Summary is a receipt-backed local Pages fixture. It proves that read surfaces can ground answers in World Model evidence before richer document storage opens.",
        },
    );
    for document in pages_onboarding_documents() {
        maybe_push_onboarding_hit(&mut hits, &terms, document);
    }
    // Authored pages search exactly like the charter corpus - from their
    // recorded store refs.
    for authored in crate::pages_authored::authored_documents(kernel) {
        let Some(body) = crate::pages_body_store::read_stored_body(&authored.body_ref) else {
            continue;
        };
        let score = score_document(
            &terms,
            &authored.title,
            "",
            "authored",
            &authored.body_ref,
            &body,
        );
        if score > 0 {
            hits.push(PageSearchHit {
                document_id: authored.document_id.clone(),
                title: authored.title.clone(),
                body_ref: authored.body_ref.clone(),
                summary: String::new(),
                category: "authored".to_string(),
                author_actor_id: authored.author.clone(),
                source_receipt_ids: vec![authored.receipt_id.clone()],
                charter_evidence_ids: Vec::new(),
                revision_id: authored.revision_id.clone(),
                snippet: snippet_for_terms(&terms, "", &body),
                score,
                why: why_for_terms(&terms, &authored.title, "", &body),
            });
        }
    }
    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.document_id.cmp(&right.document_id))
    });
    let results = hits
        .iter()
        .take(8)
        .map(render_pages_search_hit_json)
        .collect::<Vec<_>>()
        .join(",\n    ");
    format!(
        r#"{{
  "name": "mdx-pages-knowledge-search",
  "status": "LOCAL_KNOWLEDGE_SEARCH_READY",
  "route": "/pages/search.json",
  "tenant_id": "tenant_local",
  "query": {},
  "normalized_query": {},
  "search_mode": "local_lexical_allowlisted_pages_corpus",
  "semantic_and_ai_answers": "not built - they arrive with CTX, and every hit already carries the receipts a cited answer needs",
  "result_count": {},
  "max_results": 8,
  "results": [
    {}
  ],
  "read_only": true,
  "writes_allowed": false,
  "receipt_backed": true,
  "body_ref_allowlisted": true,
  "standalone_store_allowed": false,
  "search_indexing_allowed": false,
  "embedding_provider_turn_on_observed": false,
  "embedding_provider_call_allowed": false,
  "vector_store_adapter_allowed": false,
  "public_visibility_allowed": false,
  "live_substrate_required": false,
  "production_write_allowed": false,
  "source_routes": [
    "/pages.json",
    "/pages/{{document_id}}",
    "/pages/{{document_id}}/body",
    "/pages/search-preflights.json",
    "/pages/search-preflights/projection.json"
  ],
  "source_contracts": [
    "generated/routes/pages-readonly-route-contract.json",
    "generated/world-model/pages-projection-fixtures.json",
    "generated/world-model/pages-search-preflight-contract.json"
  ],
  "blocked_until_turn_on": [
    "embedding_provider_call",
    "vector_store_adapter",
    "search_indexing",
    "public_visibility",
    "production_write"
  ],
  "ui_handoff": {{
    "target_owner": "claude",
    "target_surface": "apps/mdx Pages module",
    "safe_to_show_search_results": true,
    "must_show_local_lexical_mode": true,
    "must_show_receipt_citations": true,
    "must_not_claim_vector_search": true,
    "must_not_claim_embedding_provider": true
  }}
}}"#,
        json_string_literal(query),
        json_string_literal(&normalized_query),
        hits.len(),
        results
    )
}

struct StaticPageSearchDocument {
    document_id: &'static str,
    title: &'static str,
    body_ref: &'static str,
    summary: &'static str,
    category: &'static str,
    author_actor_id: &'static str,
    source_receipt_ids: &'static [&'static str],
    charter_evidence_ids: &'static [&'static str],
    revision_id: &'static str,
    body: &'static str,
}

struct PageSearchHit {
    document_id: String,
    title: String,
    body_ref: String,
    summary: String,
    category: String,
    author_actor_id: String,
    source_receipt_ids: Vec<String>,
    charter_evidence_ids: Vec<String>,
    revision_id: String,
    snippet: String,
    score: usize,
    // Why this result: which terms matched, and where - said plainly.
    why: Vec<String>,
}

// Which terms matched where - the why behind every result.
fn why_for_terms(terms: &[String], title: &str, summary: &str, body: &str) -> Vec<String> {
    let title_lower = title.to_ascii_lowercase();
    let summary_lower = summary.to_ascii_lowercase();
    let body_lower = body.to_ascii_lowercase();
    terms
        .iter()
        .filter_map(|term| {
            let location = if title_lower.contains(term.as_str()) {
                "the title"
            } else if summary_lower.contains(term.as_str()) {
                "the summary"
            } else if body_lower.contains(term.as_str()) {
                "the body"
            } else {
                return None;
            };
            Some(format!("'{term}' in {location}"))
        })
        .collect()
}

fn maybe_push_static_hit(
    hits: &mut Vec<PageSearchHit>,
    terms: &[String],
    document: StaticPageSearchDocument,
) {
    let score = score_document(
        terms,
        document.title,
        document.summary,
        document.category,
        document.body_ref,
        document.body,
    );
    if score > 0 {
        hits.push(PageSearchHit {
            document_id: document.document_id.to_string(),
            title: document.title.to_string(),
            body_ref: document.body_ref.to_string(),
            summary: document.summary.to_string(),
            category: document.category.to_string(),
            author_actor_id: document.author_actor_id.to_string(),
            source_receipt_ids: document
                .source_receipt_ids
                .iter()
                .map(|s| s.to_string())
                .collect(),
            charter_evidence_ids: document
                .charter_evidence_ids
                .iter()
                .map(|s| s.to_string())
                .collect(),
            revision_id: document.revision_id.to_string(),
            snippet: snippet_for_terms(terms, document.summary, document.body),
            score,
            why: why_for_terms(terms, document.title, document.summary, document.body),
        });
    }
}

fn maybe_push_onboarding_hit(
    hits: &mut Vec<PageSearchHit>,
    terms: &[String],
    document: &'static PagesOnboardingDocument,
) {
    let body = pages_onboarding_document_body_by_id(document.document_id).unwrap_or("");
    let score = score_document(
        terms,
        document.title,
        document.summary,
        document.category,
        document.body_ref,
        body,
    );
    if score > 0 {
        hits.push(PageSearchHit {
            document_id: document.document_id.to_string(),
            title: document.title.to_string(),
            body_ref: document.body_ref.to_string(),
            summary: document.summary.to_string(),
            category: document.category.to_string(),
            author_actor_id: document.author_actor_id.to_string(),
            source_receipt_ids: document
                .source_receipt_ids
                .iter()
                .map(|s| s.to_string())
                .collect(),
            charter_evidence_ids: document
                .charter_evidence_ids
                .iter()
                .map(|s| s.to_string())
                .collect(),
            revision_id: document.revision_id.to_string(),
            snippet: snippet_for_terms(terms, document.summary, body),
            score,
            why: why_for_terms(terms, document.title, document.summary, body),
        });
    }
}

fn render_pages_search_hit_json(hit: &PageSearchHit) -> String {
    format!(
        r#"{{
      "tenant_id": "tenant_local",
      "document_id": {},
      "title": {},
      "body_ref": {},
      "body_route": {},
      "summary": {},
      "category": {},
      "author_actor_id": {},
      "source_receipt_ids": [{}],
      "charter_evidence_ids": [{}],
      "why": [{}],
      "revision_id": {},
      "snippet": {},
      "score": {},
      "visibility": "tenant_only",
      "receipt_backed": true,
      "body_ref_allowlisted": true,
      "world_model_source": "generated/world-model/pages-projection-fixtures.json"
    }}"#,
        json_string_literal(&hit.document_id),
        json_string_literal(&hit.title),
        json_string_literal(&hit.body_ref),
        json_string_literal(&format!("/pages/{}/body", hit.document_id)),
        json_string_literal(&hit.summary),
        json_string_literal(&hit.category),
        json_string_literal(&hit.author_actor_id),
        json_str_array(
            &hit.source_receipt_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        ),
        json_str_array(
            &hit.charter_evidence_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
        ),
        json_str_array(&hit.why.iter().map(String::as_str).collect::<Vec<_>>()),
        json_string_literal(&hit.revision_id),
        json_string_literal(&hit.snippet),
        hit.score
    )
}

fn score_document(
    terms: &[String],
    title: &str,
    summary: &str,
    category: &str,
    body_ref: &str,
    body: &str,
) -> usize {
    let title = title.to_ascii_lowercase();
    let summary = summary.to_ascii_lowercase();
    let category = category.to_ascii_lowercase();
    let body_ref = body_ref.to_ascii_lowercase();
    let body = body.to_ascii_lowercase();
    terms
        .iter()
        .map(|term| {
            count_occurrences(&title, term) * 8
                + count_occurrences(&summary, term) * 5
                + count_occurrences(&category, term) * 3
                + count_occurrences(&body_ref, term) * 2
                + count_occurrences(&body, term)
        })
        .sum()
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        0
    } else {
        haystack.match_indices(needle).count()
    }
}

fn snippet_for_terms(terms: &[String], summary: &str, body: &str) -> String {
    body.lines()
        .map(str::trim)
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            !line.is_empty() && terms.iter().any(|term| lower.contains(term))
        })
        .map(trim_snippet)
        .unwrap_or_else(|| trim_snippet(summary))
}

fn trim_snippet(value: &str) -> String {
    let value = value.trim();
    if value.chars().count() <= 220 {
        value.to_string()
    } else {
        format!("{}...", value.chars().take(220).collect::<String>())
    }
}

fn normalize_query(query: &str) -> String {
    let normalized = query.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        "harness".to_string()
    } else {
        normalized
    }
}

fn search_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .filter(|term| term.len() >= 2)
        .collect()
}

pub(crate) fn percent_decode_query_value(value: &str) -> String {
    let mut out = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let hex = &value[index + 1..index + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    out.push(byte);
                    index += 3;
                } else {
                    out.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn json_str_array(values: &[&str]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_local_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let preflight_id = json_string_field(body, "preflight_id")
        .unwrap_or_else(|| "pages_search_preflight_local_001".to_string());
    let source_approval_request_receipt_id =
        json_string_field(body, "source_approval_request_receipt_id").unwrap_or_default();
    let document_id = json_string_field(body, "document_id")
        .unwrap_or_else(|| "page_local_operator_note".to_string());
    let revision_id =
        json_string_field(body, "revision_id").unwrap_or_else(|| "rev_local_draft_001".to_string());
    let citation_handle = json_string_field(body, "citation_handle").unwrap_or_else(|| {
        "world_model://pages/page_local_operator_note/body/draft/v1".to_string()
    });
    let attachment_policy_id = json_string_field(body, "attachment_policy_id")
        .unwrap_or_else(|| "attachment-policy-local-readonly".to_string());
    let requested_index_scope = json_string_field(body, "requested_index_scope")
        .unwrap_or_else(|| "tenant operating memory".to_string());
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
        .save_pages_search_preflight_local_with_identity(
            PagesSearchPreflight {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                preflight_id: &preflight_id,
                source_approval_request_receipt_id: &source_approval_request_receipt_id,
                document_id: &document_id,
                revision_id: &revision_id,
                citation_handle: &citation_handle,
                attachment_policy_id: &attachment_policy_id,
                requested_index_scope: &requested_index_scope,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-pages-search-preflight-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","auth_session_tenant_id":{},"auth_session_user_id":{},"auth_session_role":{},"writes_route":"/pages/search-preflights.json","projection_route":"/pages/search-preflights/projection.json","preflight_id":{},"preflight_receipt_id":{},"policy_decision_id":{},"source_approval_request_receipt_id":{},"document_id":{},"revision_id":{},"citation_handle":{},"attachment_policy_id":{},"revision_citation_comparison_proven":{},"attachment_access_policy_proven":{},"search_indexing_admission_recorded":{},"embedding_provider_turn_on_observed":{},"vector_store_adapter_allowed":{},"search_indexing_allowed":{},"public_visibility_allowed":{},"rich_editor_allowed":{},"unsafe_document_access_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&resolved.tenant_id),
        json_string_literal(&resolved.actor_id),
        json_string_literal(&resolved.actor_role),
        json_string_literal(&report.preflight_id),
        json_string_literal(&report.preflight_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.source_approval_request_receipt_id),
        json_string_literal(&report.document_id),
        json_string_literal(&report.revision_id),
        json_string_literal(&report.citation_handle),
        json_string_literal(&report.attachment_policy_id),
        report.revision_citation_comparison_proven,
        report.attachment_access_policy_proven,
        report.search_indexing_admission_recorded,
        report.embedding_provider_turn_on_observed,
        report.vector_store_adapter_allowed,
        report.search_indexing_allowed,
        report.public_visibility_allowed,
        report.rich_editor_allowed,
        report.unsafe_document_access_allowed,
        report.production_write_allowed
    ))
}

fn render_local_projection_json(kernel: &MdxKernel) -> Result<String, String> {
    let preflights = kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == "pages.search.preflighted")
        .map(|receipt| {
            format!(
                r#"{{"preflight_id":{},"preflight_receipt_id":{},"policy_decision_id":{},"source_approval_request_receipt_id":{},"document_id":{},"revision_id":{},"citation_handle":{},"attachment_policy_id":{},"requested_index_scope":{},"terminal_state":{},"revision_citation_comparison_proven":true,"attachment_access_policy_proven":true,"search_indexing_admission_recorded":true,"embedding_provider_turn_on_observed":false,"vector_store_adapter_allowed":false,"search_indexing_allowed":false,"public_visibility_allowed":false,"rich_editor_allowed":false,"unsafe_document_access_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(payload_value(receipt, "preflight_id")),
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(payload_value(receipt, "source_approval_request_receipt_id")),
                json_string_literal(payload_value(receipt, "document_id")),
                json_string_literal(payload_value(receipt, "revision_id")),
                json_string_literal(payload_value(receipt, "citation_handle")),
                json_string_literal(payload_value(receipt, "attachment_policy_id")),
                json_string_literal(payload_value(receipt, "requested_index_scope")),
                json_string_literal(payload_value(receipt, "terminal_state"))
            )
        })
        .collect::<Vec<_>>();
    Ok(format!(
        r#"{{"name":"mdx-pages-search-preflight-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/pages/search-preflights.json","preflight_count":{},"revision_citation_comparison_proven":true,"attachment_access_policy_proven":true,"search_indexing_admission_recorded":true,"embedding_provider_turn_on_observed":false,"vector_store_adapter_allowed":false,"search_indexing_allowed":false,"public_visibility_allowed":false,"rich_editor_allowed":false,"unsafe_document_access_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"preflights":[{}]}}"#,
        preflights.len(),
        preflights.join(",")
    ))
}

fn render_revision_citation_post_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    let comparison_id = json_string_field(body, "comparison_id")
        .unwrap_or_else(|| "pages_revision_citation_comparison_local_001".to_string());
    let source_search_preflight_receipt_id =
        json_string_field(body, "source_search_preflight_receipt_id").unwrap_or_default();
    let document_id = json_string_field(body, "document_id")
        .unwrap_or_else(|| "page_local_operator_note".to_string());
    let revision_id =
        json_string_field(body, "revision_id").unwrap_or_else(|| "rev_local_draft_001".to_string());
    let citation_handle = json_string_field(body, "citation_handle").unwrap_or_else(|| {
        "world_model://pages/page_local_operator_note/body/draft/v1".to_string()
    });
    let v1_revision_key = json_string_field(body, "v1_revision_key")
        .unwrap_or_else(|| "rev_local_draft_001".to_string());
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
        .save_pages_revision_citation_comparison_local_with_identity(
            PagesRevisionCitationComparison {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                comparison_id: &comparison_id,
                source_search_preflight_receipt_id: &source_search_preflight_receipt_id,
                document_id: &document_id,
                revision_id: &revision_id,
                citation_handle: &citation_handle,
                v1_revision_key: &v1_revision_key,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-pages-revision-citation-comparison-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/pages/revision-citation-comparisons.json","projection_route":"/pages/revision-citation-comparisons/projection.json","comparison_id":{},"comparison_receipt_id":{},"policy_decision_id":{},"source_search_preflight_receipt_id":{},"document_id":{},"revision_id":{},"citation_handle":{},"v1_revision_key":{},"revision_citation_comparison_proven":{},"attachment_access_policy_proven":{},"public_visibility_allowed":{},"search_indexing_allowed":{},"embedding_provider_call_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.comparison_id),
        json_string_literal(&report.comparison_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.source_search_preflight_receipt_id),
        json_string_literal(&report.document_id),
        json_string_literal(&report.revision_id),
        json_string_literal(&report.citation_handle),
        json_string_literal(&report.v1_revision_key),
        report.revision_citation_comparison_proven,
        report.attachment_access_policy_proven,
        report.public_visibility_allowed,
        report.search_indexing_allowed,
        report.embedding_provider_call_allowed,
        report.production_write_allowed
    ))
}

fn render_visibility_check_post_json(body: &str, kernel: &mut MdxKernel) -> Result<String, String> {
    let visibility_check_id = json_string_field(body, "visibility_check_id")
        .unwrap_or_else(|| "pages_publication_visibility_check_local_001".to_string());
    let source_revision_citation_receipt_id =
        json_string_field(body, "source_revision_citation_receipt_id").unwrap_or_default();
    let document_id = json_string_field(body, "document_id")
        .unwrap_or_else(|| "page_local_operator_note".to_string());
    let revision_id =
        json_string_field(body, "revision_id").unwrap_or_else(|| "rev_local_draft_001".to_string());
    let requested_visibility = json_string_field(body, "requested_visibility")
        .unwrap_or_else(|| "tenant_review".to_string());
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
        .save_pages_publication_visibility_check_local_with_identity(
            PagesPublicationVisibilityCheck {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                visibility_check_id: &visibility_check_id,
                source_revision_citation_receipt_id: &source_revision_citation_receipt_id,
                document_id: &document_id,
                revision_id: &revision_id,
                requested_visibility: &requested_visibility,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-pages-publication-visibility-check-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/pages/publication-visibility-checks.json","projection_route":"/pages/publication-visibility-checks/projection.json","visibility_check_id":{},"visibility_check_receipt_id":{},"policy_decision_id":{},"source_revision_citation_receipt_id":{},"document_id":{},"revision_id":{},"requested_visibility":{},"publication_visibility_proven":{},"public_visibility_allowed":{},"production_visibility_allowed":{},"search_indexing_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.visibility_check_id),
        json_string_literal(&report.visibility_check_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.source_revision_citation_receipt_id),
        json_string_literal(&report.document_id),
        json_string_literal(&report.revision_id),
        json_string_literal(&report.requested_visibility),
        report.publication_visibility_proven,
        report.public_visibility_allowed,
        report.production_visibility_allowed,
        report.search_indexing_allowed,
        report.production_write_allowed
    ))
}

fn render_embedding_refusal_post_json(
    body: &str,
    kernel: &mut MdxKernel,
) -> Result<String, String> {
    let refusal_id = json_string_field(body, "refusal_id")
        .unwrap_or_else(|| "pages_embedding_provider_refusal_local_001".to_string());
    let source_visibility_check_receipt_id =
        json_string_field(body, "source_visibility_check_receipt_id").unwrap_or_default();
    let document_id = json_string_field(body, "document_id")
        .unwrap_or_else(|| "page_local_operator_note".to_string());
    let revision_id =
        json_string_field(body, "revision_id").unwrap_or_else(|| "rev_local_draft_001".to_string());
    let provider_profile = json_string_field(body, "provider_profile")
        .unwrap_or_else(|| "embedding-provider-local-blocked".to_string());
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
        .save_pages_embedding_provider_refusal_local_with_identity(
            PagesEmbeddingProviderRefusal {
                tenant_id: &tenant_id,
                actor_id: &actor_id,
                refusal_id: &refusal_id,
                source_visibility_check_receipt_id: &source_visibility_check_receipt_id,
                document_id: &document_id,
                revision_id: &revision_id,
                provider_profile: &provider_profile,
            },
            &identity,
        )
        .map_err(|error| error.message())?;
    Ok(format!(
        r#"{{"name":"mdx-pages-embedding-provider-refusal-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"auth_session_route":"/local/auth-session.json","writes_route":"/pages/embedding-provider-refusals.json","projection_route":"/pages/embedding-provider-refusals/projection.json","refusal_id":{},"refusal_receipt_id":{},"policy_decision_id":{},"source_visibility_check_receipt_id":{},"document_id":{},"revision_id":{},"provider_profile":{},"embedding_provider_turn_on_observed":{},"embedding_provider_call_allowed":{},"vector_store_adapter_allowed":{},"search_indexing_allowed":{},"live_substrate_required":false,"production_write_allowed":{}}}"#,
        json_string_literal(report.status),
        json_string_literal(resolved.auth_session_status),
        json_string_literal(&report.refusal_id),
        json_string_literal(&report.refusal_receipt_id),
        json_string_literal(&report.policy_decision_id),
        json_string_literal(&report.source_visibility_check_receipt_id),
        json_string_literal(&report.document_id),
        json_string_literal(&report.revision_id),
        json_string_literal(&report.provider_profile),
        report.embedding_provider_turn_on_observed,
        report.embedding_provider_call_allowed,
        report.vector_store_adapter_allowed,
        report.search_indexing_allowed,
        report.production_write_allowed
    ))
}

fn render_revision_citation_projection_json(kernel: &MdxKernel) -> String {
    let items = evidence_projection_items(
        kernel,
        "pages.revision_citation.compared",
        &[
            "comparison_id",
            "source_search_preflight_receipt_id",
            "document_id",
            "revision_id",
            "citation_handle",
            "v1_revision_key",
            "terminal_state",
        ],
    );
    format!(
        r#"{{"name":"mdx-pages-revision-citation-comparison-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/pages/revision-citation-comparisons.json","comparison_count":{},"revision_citation_comparison_proven":true,"attachment_access_policy_proven":true,"public_visibility_allowed":false,"search_indexing_allowed":false,"embedding_provider_call_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"comparisons":[{}]}}"#,
        items.len(),
        items.join(",")
    )
}

fn render_visibility_check_projection_json(kernel: &MdxKernel) -> String {
    let items = evidence_projection_items(
        kernel,
        "pages.publication.visibility.checked",
        &[
            "visibility_check_id",
            "source_revision_citation_receipt_id",
            "document_id",
            "revision_id",
            "requested_visibility",
            "terminal_state",
        ],
    );
    format!(
        r#"{{"name":"mdx-pages-publication-visibility-check-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/pages/publication-visibility-checks.json","visibility_check_count":{},"publication_visibility_proven":true,"public_visibility_allowed":false,"production_visibility_allowed":false,"search_indexing_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"checks":[{}]}}"#,
        items.len(),
        items.join(",")
    )
}

fn render_embedding_refusal_projection_json(kernel: &MdxKernel) -> String {
    let items = evidence_projection_items(
        kernel,
        "pages.embedding_provider.refused",
        &[
            "refusal_id",
            "source_visibility_check_receipt_id",
            "document_id",
            "revision_id",
            "provider_profile",
            "terminal_state",
        ],
    );
    format!(
        r#"{{"name":"mdx-pages-embedding-provider-refusal-local-projection","status":"OK","auth_session_route":"/local/auth-session.json","writes_route":"/pages/embedding-provider-refusals.json","refusal_count":{},"embedding_provider_turn_on_observed":false,"embedding_provider_call_allowed":false,"vector_store_adapter_allowed":false,"search_indexing_allowed":false,"live_substrate_required":false,"production_write_allowed":false,"refusals":[{}]}}"#,
        items.len(),
        items.join(",")
    )
}

fn evidence_projection_items(kernel: &MdxKernel, kind: &str, fields: &[&str]) -> Vec<String> {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == kind)
        .map(|receipt| {
            let mut values = vec![
                format!(
                    r#""receipt_id":{}"#,
                    json_string_literal(&receipt.receipt_id)
                ),
                format!(
                    r#""policy_decision_id":{}"#,
                    json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or(""))
                ),
            ];
            values.extend(fields.iter().map(|field| {
                format!(
                    r#""{}":{}"#,
                    field,
                    json_string_literal(payload_value(receipt, field))
                )
            }));
            format!("{{{}}}", values.join(","))
        })
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_knowledge_search_route_returns_allowlisted_hits() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response("GET", "/pages/search.json?q=safe+tool+plane", "", &kernel)
            .expect("route handled")
            .expect("search response");
        assert_eq!(response.status, "200 OK");
        for expected in [
            "\"name\": \"mdx-pages-knowledge-search\"",
            "\"status\": \"LOCAL_KNOWLEDGE_SEARCH_READY\"",
            "\"normalized_query\": \"safe tool plane\"",
            "\"search_mode\": \"local_lexical_allowlisted_pages_corpus\"",
            "\"document_id\": \"page_harness_safe_tool_plane\"",
            "\"body_route\": \"/pages/page_harness_safe_tool_plane/body\"",
            "\"read_only\": true",
            "\"writes_allowed\": false",
            "\"search_indexing_allowed\": false",
            "\"embedding_provider_call_allowed\": false",
            "\"vector_store_adapter_allowed\": false",
            "\"production_write_allowed\": false",
            "\"must_not_claim_vector_search\": true",
        ] {
            assert!(response.body.contains(expected), "{expected}");
        }
    }

    #[test]
    fn pages_search_preflight_route_records_preflight_and_projection() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = route_response(
            "POST",
            "/pages/search-preflights.json",
            r#"{"preflight_id":"pages_search_preflight_route_test","document_id":"page-v1-operating-memory","revision_id":"rev-v1-001","citation_handle":"pages/operating-memory#rev-v1-001","attachment_policy_id":"attachment-policy-v1-local","requested_index_scope":"tenant operating memory"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("post response");
        assert_eq!(response.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-pages-search-preflight-local-post\"",
            "\"status\":\"PAGES_SEARCH_PREFLIGHT_RECORDED_INDEXING_BLOCKED\"",
            "\"preflight_id\":\"pages_search_preflight_route_test\"",
            "\"revision_citation_comparison_proven\":true",
            "\"attachment_access_policy_proven\":true",
            "\"search_indexing_admission_recorded\":true",
            "\"embedding_provider_turn_on_observed\":false",
            "\"search_indexing_allowed\":false",
            "\"production_write_allowed\":false",
        ] {
            assert!(response.body.contains(expected), "{expected}");
        }
        let projection = route_response(
            "GET",
            "/pages/search-preflights/projection.json",
            "",
            &kernel,
        )
        .expect("route handled")
        .expect("projection response");
        assert_eq!(projection.status, "200 OK");
        for expected in [
            "\"name\":\"mdx-pages-search-preflight-local-projection\"",
            "\"preflight_count\":1",
            "\"writes_route\":\"/pages/search-preflights.json\"",
            "\"preflight_id\":\"pages_search_preflight_route_test\"",
            "\"terminal_state\":\"PAGES_SEARCH_PREFLIGHT_RECORDED_INDEXING_BLOCKED\"",
        ] {
            assert!(projection.body.contains(expected), "{expected}");
        }
    }

    #[test]
    fn pages_search_evidence_ladder_routes_record_local_proofs() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let search = route_response(
            "POST",
            "/pages/search-preflights.json",
            r#"{"preflight_id":"pages_search_preflight_route_test","document_id":"page-v1-operating-memory","revision_id":"rev-v1-001","citation_handle":"pages/operating-memory#rev-v1-001","attachment_policy_id":"attachment-policy-v1-local","requested_index_scope":"tenant operating memory"}"#,
            &kernel,
        )
        .expect("route handled")
        .expect("search preflight");
        let search_receipt =
            json_string_field(&search.body, "preflight_receipt_id").expect("preflight receipt");
        let comparison = route_response(
            "POST",
            "/pages/revision-citation-comparisons.json",
            &format!(
                r#"{{"comparison_id":"pages_revision_citation_route_test","source_search_preflight_receipt_id":"{}","document_id":"page-v1-operating-memory","revision_id":"rev-v1-001","citation_handle":"pages/operating-memory#rev-v1-001","v1_revision_key":"rev-v1-001"}}"#,
                search_receipt
            ),
            &kernel,
        )
        .expect("route handled")
        .expect("comparison");
        assert!(
            comparison.body.contains(
                "\"status\":\"PAGES_REVISION_CITATION_COMPARISON_RECORDED_SEARCH_BLOCKED\""
            )
        );
        let comparison_receipt = json_string_field(&comparison.body, "comparison_receipt_id")
            .expect("comparison receipt");
        let visibility = route_response(
            "POST",
            "/pages/publication-visibility-checks.json",
            &format!(
                r#"{{"visibility_check_id":"pages_visibility_route_test","source_revision_citation_receipt_id":"{}","document_id":"page-v1-operating-memory","revision_id":"rev-v1-001","requested_visibility":"tenant_review"}}"#,
                comparison_receipt
            ),
            &kernel,
        )
        .expect("route handled")
        .expect("visibility");
        assert!(
            visibility
                .body
                .contains("\"status\":\"PAGES_PUBLICATION_VISIBILITY_CHECKED_PUBLIC_BLOCKED\"")
        );
        let visibility_receipt = json_string_field(&visibility.body, "visibility_check_receipt_id")
            .expect("visibility receipt");
        let refusal = route_response(
            "POST",
            "/pages/embedding-provider-refusals.json",
            &format!(
                r#"{{"refusal_id":"pages_embedding_refusal_route_test","source_visibility_check_receipt_id":"{}","document_id":"page-v1-operating-memory","revision_id":"rev-v1-001","provider_profile":"embedding-provider-local-blocked"}}"#,
                visibility_receipt
            ),
            &kernel,
        )
        .expect("route handled")
        .expect("embedding refusal");
        assert!(
            refusal
                .body
                .contains("\"status\":\"PAGES_EMBEDDING_PROVIDER_REFUSED_LOCAL_ONLY\"")
        );
        assert!(
            refusal
                .body
                .contains("\"embedding_provider_call_allowed\":false")
        );
        for (path, marker) in [
            (
                "/pages/revision-citation-comparisons/projection.json",
                "\"comparison_count\":1",
            ),
            (
                "/pages/publication-visibility-checks/projection.json",
                "\"visibility_check_count\":1",
            ),
            (
                "/pages/embedding-provider-refusals/projection.json",
                "\"refusal_count\":1",
            ),
        ] {
            let projection = route_response("GET", path, "", &kernel)
                .expect("route handled")
                .expect("projection");
            assert_eq!(projection.status, "200 OK");
            assert!(projection.body.contains(marker), "{marker}");
            assert!(
                projection
                    .body
                    .contains("\"production_write_allowed\":false")
            );
        }
    }
}
