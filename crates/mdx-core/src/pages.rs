#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PagesOnboardingDocument {
    pub document_id: &'static str,
    pub title: &'static str,
    pub body_ref: &'static str,
    pub summary: &'static str,
    pub category: &'static str,
    pub author_actor_id: &'static str,
    pub source_receipt_ids: &'static [&'static str],
    pub charter_evidence_ids: &'static [&'static str],
    pub revision_id: &'static str,
}

#[rustfmt::skip]
pub static PAGES_ONBOARDING_DOCUMENTS: [PagesOnboardingDocument; 4] = [
    PagesOnboardingDocument { document_id: "page_developer_start_here", title: "Developer Start Here", body_ref: "docs/QUICKSTART.md", summary: "The shortest path from a fresh clone to a running MDx workspace.", category: "developer_onboarding", author_actor_id: "project:mdx", source_receipt_ids: &[], charter_evidence_ids: &[], revision_id: "rev_developer_start_here_001" },
    PagesOnboardingDocument { document_id: "page_architecture", title: "Architecture", body_ref: "docs/ARCHITECTURE.md", summary: "The main system boundaries and the contracts that hold MDx together.", category: "architecture", author_actor_id: "project:mdx", source_receipt_ids: &[], charter_evidence_ids: &[], revision_id: "rev_architecture_001" },
    PagesOnboardingDocument { document_id: "page_security", title: "Security", body_ref: "SECURITY.md", summary: "How to report vulnerabilities and reason about MDx trust boundaries.", category: "security", author_actor_id: "project:mdx", source_receipt_ids: &[], charter_evidence_ids: &[], revision_id: "rev_security_001" },
    PagesOnboardingDocument { document_id: "page_product_direction", title: "Product Direction", body_ref: "docs/UI-PRODUCT-NORTH-STAR.md", summary: "The product experience MDx is working toward across Twin, Forge, Pages, and Message.", category: "product", author_actor_id: "project:mdx", source_receipt_ids: &[], charter_evidence_ids: &[], revision_id: "rev_product_direction_001" },
];

pub fn pages_onboarding_documents() -> &'static [PagesOnboardingDocument] {
    &PAGES_ONBOARDING_DOCUMENTS
}

pub fn pages_onboarding_document_by_id(
    document_id: &str,
) -> Option<&'static PagesOnboardingDocument> {
    PAGES_ONBOARDING_DOCUMENTS
        .iter()
        .find(|document| document.document_id == document_id)
}

pub fn pages_onboarding_document_body_by_id(document_id: &str) -> Option<&'static str> {
    match document_id {
        "page_developer_start_here" => Some(include_str!("../../../docs/QUICKSTART.md")),
        "page_architecture" => Some(include_str!("../../../docs/ARCHITECTURE.md")),
        "page_security" => Some(include_str!("../../../SECURITY.md")),
        "page_product_direction" => Some(include_str!("../../../docs/UI-PRODUCT-NORTH-STAR.md")),
        _ => None,
    }
}
