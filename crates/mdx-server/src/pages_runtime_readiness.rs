use crate::{JSON_CONTENT_TYPE, RouteResponse};
use mdx_core::{
    MdxKernel, Receipt, json_string_literal,
    pages::{pages_onboarding_document_body_by_id, pages_onboarding_documents},
};
use std::sync::{Arc, RwLock};

const ROUTE: &str = "/pages/runtime-readiness.json";

pub(crate) fn route_response(
    method: &str,
    path: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != ROUTE {
        return None;
    }
    if !method.eq_ignore_ascii_case("GET") {
        return Some(Ok(RouteResponse {
            status: "405 Method Not Allowed",
            content_type: "text/plain; charset=utf-8",
            body: "method not allowed\n".to_string(),
        }));
    }
    let kernel = match kernel.read() {
        Ok(kernel) => kernel,
        Err(_) => return Some(Err("kernel lock poisoned".to_string())),
    };
    Some(Ok(RouteResponse {
        status: "200 OK",
        content_type: JSON_CONTENT_TYPE,
        body: render_pages_runtime_readiness_json(&kernel),
    }))
}

fn render_pages_runtime_readiness_json(kernel: &MdxKernel) -> String {
    let evidence = PagesRuntimeEvidence::from_kernel(kernel);
    let status = evidence.status();
    let documents = 1 + pages_onboarding_documents().len();
    let body_documents = 1 + pages_onboarding_documents()
        .iter()
        .filter(|document| pages_onboarding_document_body_by_id(document.document_id).is_some())
        .count();
    format!(
        r#"{{
  "name": "mdx-pages-runtime-readiness",
  "route": "/pages/runtime-readiness.json",
  "status": {},
  "tenant_id": "tenant_local",
  "mode": "live-local-governed-readiness",
  "read_only": true,
  "writes_allowed": false,
  "receipt_backed": true,
  "document_projection": {{
    "route": "/pages.json",
    "document_count": {},
    "body_route": "/pages/{{document_id}}/body",
    "body_document_count": {},
    "body_ref_allowlisted": true,
    "standalone_store_allowed": false
  }},
  "knowledge_search": {{
    "route": "/pages/search.json",
    "local_search_ready": true,
    "search_mode": "local_lexical_allowlisted_pages_corpus",
    "vector_store_adapter_allowed": false,
    "embedding_provider_turn_on_observed": false,
    "search_indexing_allowed": false
  }},
  "lifecycle": {{
    "publication_count": {},
    "edit_draft_count": {},
    "approval_request_count": {},
    "approval_decision_count": {},
    "lifecycle_ready": {}
  }},
  "knowledge_evidence": {{
    "search_preflight_count": {},
    "revision_citation_comparison_count": {},
    "publication_visibility_check_count": {},
    "embedding_provider_refusal_count": {},
    "knowledge_evidence_ready": {}
  }},
  "required_receipts": {{
    "pages.document.published": {},
    "pages.edit.draft.saved": {},
    "pages.approval.requested": {},
    "pages.approval.decision.recorded": {},
    "pages.search.preflighted": {},
    "pages.revision_citation.compared": {},
    "pages.publication.visibility.checked": {},
    "pages.embedding_provider.refused": {}
  }},
  "source_routes": [
    "/pages.json",
    "/pages/{{document_id}}",
    "/pages/{{document_id}}/body",
    "/pages/search.json",
    "/pages/publications.json",
    "/pages/publications/projection.json",
    "/pages/edit-drafts.json",
    "/pages/edit-drafts/projection.json",
    "/pages/approval-requests.json",
    "/pages/approval-requests/projection.json",
    "/pages/approval-decisions/approve.json",
    "/pages/approval-decisions/reject.json",
    "/pages/approval-decisions/projection.json",
    "/pages/search-preflights.json",
    "/pages/search-preflights/projection.json",
    "/pages/revision-citation-comparisons.json",
    "/pages/revision-citation-comparisons/projection.json",
    "/pages/publication-visibility-checks.json",
    "/pages/publication-visibility-checks/projection.json",
    "/pages/embedding-provider-refusals.json",
    "/pages/embedding-provider-refusals/projection.json"
  ],
  "blocked_authority": {{
    "public_visibility_allowed": false,
    "production_visibility_allowed": false,
    "search_indexing_allowed": false,
    "embedding_provider_call_allowed": false,
    "vector_store_adapter_allowed": false,
    "rich_editor_allowed": false,
    "standalone_store_allowed": false,
    "unsafe_document_access_allowed": false,
    "production_publish_allowed": false,
    "production_write_allowed": false
  }},
  "ui_handoff": {{
    "target_owner": "claude",
    "target_surface": "apps/mdx Pages module",
    "safe_to_show_pages_runtime_ready": {},
    "must_show_body_route": true,
    "must_show_local_lexical_search": true,
    "must_show_human_decision_receipts": true,
    "must_show_embedding_provider_blocked": true,
    "must_not_claim_public_visibility": true,
    "must_not_claim_vector_search": true,
    "must_not_claim_production_publish": true
  }},
  "source_contracts": [
    "docs/PAGES-RUNTIME-READINESS-CONTRACT.md",
    "docs/PAGES-READONLY-ROUTE-CONTRACT.md",
    "docs/PAGES-SEARCH-PREFLIGHT-CONTRACT.md",
    "docs/PAGES-APPROVAL-DECISION-CONTRACT.md",
    "generated/routes/pages-readonly-route-contract.json",
    "generated/world-model/pages-search-preflight-contract.json"
  ]
}}"#,
        json_string_literal(status),
        documents,
        body_documents,
        evidence.publication_count,
        evidence.edit_draft_count,
        evidence.approval_request_count,
        evidence.approval_decision_count,
        evidence.lifecycle_ready(),
        evidence.search_preflight_count,
        evidence.revision_citation_count,
        evidence.visibility_check_count,
        evidence.embedding_refusal_count,
        evidence.knowledge_evidence_ready(),
        json_optional_receipt(evidence.latest_publication),
        json_optional_receipt(evidence.latest_edit_draft),
        json_optional_receipt(evidence.latest_approval_request),
        json_optional_receipt(evidence.latest_approval_decision),
        json_optional_receipt(evidence.latest_search_preflight),
        json_optional_receipt(evidence.latest_revision_citation),
        json_optional_receipt(evidence.latest_visibility_check),
        json_optional_receipt(evidence.latest_embedding_refusal),
        evidence.ready(),
    )
}

struct PagesRuntimeEvidence<'a> {
    publication_count: usize,
    edit_draft_count: usize,
    approval_request_count: usize,
    approval_decision_count: usize,
    search_preflight_count: usize,
    revision_citation_count: usize,
    visibility_check_count: usize,
    embedding_refusal_count: usize,
    latest_publication: Option<&'a Receipt>,
    latest_edit_draft: Option<&'a Receipt>,
    latest_approval_request: Option<&'a Receipt>,
    latest_approval_decision: Option<&'a Receipt>,
    latest_search_preflight: Option<&'a Receipt>,
    latest_revision_citation: Option<&'a Receipt>,
    latest_visibility_check: Option<&'a Receipt>,
    latest_embedding_refusal: Option<&'a Receipt>,
}

impl<'a> PagesRuntimeEvidence<'a> {
    fn from_kernel(kernel: &'a MdxKernel) -> Self {
        Self {
            publication_count: count_kind(kernel, "pages.document.published"),
            edit_draft_count: count_kind(kernel, "pages.edit.draft.saved"),
            approval_request_count: count_kind(kernel, "pages.approval.requested"),
            approval_decision_count: count_kind(kernel, "pages.approval.decision.recorded"),
            search_preflight_count: count_kind(kernel, "pages.search.preflighted"),
            revision_citation_count: count_kind(kernel, "pages.revision_citation.compared"),
            visibility_check_count: count_kind(kernel, "pages.publication.visibility.checked"),
            embedding_refusal_count: count_kind(kernel, "pages.embedding_provider.refused"),
            latest_publication: latest_kind(kernel, "pages.document.published"),
            latest_edit_draft: latest_kind(kernel, "pages.edit.draft.saved"),
            latest_approval_request: latest_kind(kernel, "pages.approval.requested"),
            latest_approval_decision: latest_kind(kernel, "pages.approval.decision.recorded"),
            latest_search_preflight: latest_kind(kernel, "pages.search.preflighted"),
            latest_revision_citation: latest_kind(kernel, "pages.revision_citation.compared"),
            latest_visibility_check: latest_kind(kernel, "pages.publication.visibility.checked"),
            latest_embedding_refusal: latest_kind(kernel, "pages.embedding_provider.refused"),
        }
    }

    fn lifecycle_ready(&self) -> bool {
        self.publication_count > 0
            && self.edit_draft_count > 0
            && self.approval_request_count > 0
            && self.approval_decision_count > 0
    }

    fn knowledge_evidence_ready(&self) -> bool {
        self.search_preflight_count > 0
            && self.revision_citation_count > 0
            && self.visibility_check_count > 0
            && self.embedding_refusal_count > 0
    }

    fn ready(&self) -> bool {
        self.lifecycle_ready() && self.knowledge_evidence_ready()
    }

    fn status(&self) -> &'static str {
        if !self.lifecycle_ready() {
            "LOCAL-PAGES-RUNTIME-MISSING-LIFECYCLE-EVIDENCE"
        } else if !self.knowledge_evidence_ready() {
            "LOCAL-PAGES-RUNTIME-MISSING-KNOWLEDGE-EVIDENCE"
        } else {
            "LIVE-LOCAL-PAGES-RUNTIME-READY-PRODUCTION-BLOCKED"
        }
    }
}

fn count_kind(kernel: &MdxKernel, kind: &str) -> usize {
    kernel
        .ledger()
        .entries()
        .iter()
        .filter(|receipt| receipt.kind == kind)
        .count()
}

fn latest_kind<'a>(kernel: &'a MdxKernel, kind: &str) -> Option<&'a Receipt> {
    kernel
        .ledger()
        .entries()
        .iter()
        .rfind(|receipt| receipt.kind == kind)
}

fn json_optional_receipt(receipt: Option<&Receipt>) -> String {
    receipt
        .map(|receipt| {
            format!(
                r#"{{"receipt_id":{},"policy_decision_id":{},"terminal_state":{}}}"#,
                json_string_literal(&receipt.receipt_id),
                json_string_literal(receipt.policy_decision_id.as_deref().unwrap_or("")),
                json_string_literal(
                    receipt
                        .payload
                        .get("terminal_state")
                        .map(String::as_str)
                        .unwrap_or("")
                )
            )
        })
        .unwrap_or_else(|| "null".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        JSON_CONTENT_TYPE, pages_approval_request, pages_edit_draft, pages_publication,
        pages_search,
    };

    #[test]
    fn pages_runtime_readiness_reports_missing_then_ready_with_production_blocked() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let initial = route_response("GET", ROUTE, &kernel)
            .expect("route response")
            .expect("route body");
        assert_eq!(initial.status, "200 OK");
        assert_eq!(initial.content_type, JSON_CONTENT_TYPE);
        assert!(
            initial
                .body
                .contains("LOCAL-PAGES-RUNTIME-MISSING-LIFECYCLE-EVIDENCE")
        );
        assert!(
            initial
                .body
                .contains("\"safe_to_show_pages_runtime_ready\": false")
        );

        let draft = pages_edit_draft::route_response(
            "POST",
            "/pages/edit-drafts.json",
            r#"{"draft_id":"draft_runtime_ready","document_id":"page_runtime_ready","title":"Runtime Ready","revision_id":"rev_runtime_ready_draft"}"#,
            &kernel,
        )
        .expect("edit route")
        .expect("edit response");
        let source_edit_draft_receipt_id =
            json_string_field_for_test(&draft.body, "edit_draft_receipt_id");
        let request_body = format!(
            r#"{{"approval_request_id":"approval_runtime_ready","source_edit_draft_receipt_id":"{}","document_id":"page_runtime_ready","draft_id":"draft_runtime_ready","requested_visibility":"tenant_review"}}"#,
            source_edit_draft_receipt_id
        );
        let request = pages_approval_request::route_response(
            "POST",
            "/pages/approval-requests.json",
            &request_body,
            &kernel,
        )
        .expect("approval request route")
        .expect("approval request response");
        let approval_request_receipt_id =
            json_string_field_for_test(&request.body, "approval_request_receipt_id");
        let approve_body = format!(
            r#"{{"approval_decision_id":"decision_runtime_ready","approval_request_receipt_id":"{}","decision_note":"Approve local Pages runtime proof."}}"#,
            approval_request_receipt_id
        );
        let decision = pages_approval_request::route_response(
            "POST",
            "/pages/approval-decisions/approve.json",
            &approve_body,
            &kernel,
        )
        .expect("approval decision route")
        .expect("approval decision response");
        let approval_decision_receipt_id =
            json_string_field_for_test(&decision.body, "approval_decision_receipt_id");
        let publication_body = format!(
            r#"{{"document_id":"page_runtime_ready","approval_decision_receipt_id":"{}","page_type":"knowledge"}}"#,
            approval_decision_receipt_id
        );
        pages_publication::route_response(
            "POST",
            "/pages/publications.json",
            &publication_body,
            &kernel,
        )
        .expect("publication route")
        .expect("publication response");

        let missing_knowledge = route_response("GET", ROUTE, &kernel)
            .expect("route response")
            .expect("route body");
        assert!(
            missing_knowledge
                .body
                .contains("LOCAL-PAGES-RUNTIME-MISSING-KNOWLEDGE-EVIDENCE")
        );

        let search = pages_search::route_response(
            "POST",
            "/pages/search-preflights.json",
            r#"{"preflight_id":"search_runtime_ready","document_id":"page_runtime_ready","revision_id":"rev_runtime_ready","citation_handle":"pages/runtime-ready#rev","attachment_policy_id":"attachment-policy-runtime-ready","requested_index_scope":"tenant operating memory"}"#,
            &kernel,
        )
        .expect("search preflight route")
        .expect("search preflight response");
        let search_receipt_id = json_string_field_for_test(&search.body, "preflight_receipt_id");
        let comparison_body = format!(
            r#"{{"comparison_id":"comparison_runtime_ready","source_search_preflight_receipt_id":"{}","document_id":"page_runtime_ready","revision_id":"rev_runtime_ready","citation_handle":"pages/runtime-ready#rev","v1_revision_key":"rev_runtime_ready"}}"#,
            search_receipt_id
        );
        let comparison = pages_search::route_response(
            "POST",
            "/pages/revision-citation-comparisons.json",
            &comparison_body,
            &kernel,
        )
        .expect("comparison route")
        .expect("comparison response");
        let comparison_receipt_id =
            json_string_field_for_test(&comparison.body, "comparison_receipt_id");
        let visibility_body = format!(
            r#"{{"visibility_check_id":"visibility_runtime_ready","source_revision_citation_receipt_id":"{}","document_id":"page_runtime_ready","revision_id":"rev_runtime_ready","requested_visibility":"tenant_review"}}"#,
            comparison_receipt_id
        );
        let visibility = pages_search::route_response(
            "POST",
            "/pages/publication-visibility-checks.json",
            &visibility_body,
            &kernel,
        )
        .expect("visibility route")
        .expect("visibility response");
        let visibility_receipt_id =
            json_string_field_for_test(&visibility.body, "visibility_check_receipt_id");
        let refusal_body = format!(
            r#"{{"refusal_id":"embedding_runtime_ready","source_visibility_check_receipt_id":"{}","document_id":"page_runtime_ready","revision_id":"rev_runtime_ready","provider_profile":"embedding-provider-local-blocked"}}"#,
            visibility_receipt_id
        );
        pages_search::route_response(
            "POST",
            "/pages/embedding-provider-refusals.json",
            &refusal_body,
            &kernel,
        )
        .expect("embedding refusal route")
        .expect("embedding refusal response");

        let ready = route_response("GET", ROUTE, &kernel)
            .expect("route response")
            .expect("route body");
        for expected in [
            "LIVE-LOCAL-PAGES-RUNTIME-READY-PRODUCTION-BLOCKED",
            "\"lifecycle_ready\": true",
            "\"knowledge_evidence_ready\": true",
            "\"safe_to_show_pages_runtime_ready\": true",
            "\"public_visibility_allowed\": false",
            "\"search_indexing_allowed\": false",
            "\"embedding_provider_call_allowed\": false",
            "\"production_write_allowed\": false",
        ] {
            assert!(ready.body.contains(expected), "{expected}");
        }
    }

    fn json_string_field_for_test(body: &str, key: &str) -> String {
        let after_key = body.split(&format!("\"{key}\"")).nth(1).expect("json key");
        let after_colon = after_key
            .split_once(':')
            .expect("json colon")
            .1
            .trim_start();
        let value = after_colon.strip_prefix('"').expect("json string value");
        value
            .split('"')
            .next()
            .expect("json string end")
            .to_string()
    }
}
