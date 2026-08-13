// Semantic diff classification: a reusable primitive that sorts a changed
// file into the section a reviewer reads it in, and flags generated churn so
// the real behavior change is not buried under it. This is the one genuinely
// new piece the Review Room needs - the diff route already produces a flat
// file list; this gives that list MEANING and ORDER. Pure and deterministic:
// path in, section + generated flag out, no IO.
//
// The review order is the order a human should read a change: the contracts
// that define the shape, then the core behavior, then the server wiring, then
// the UI, then the proof (tests), then the churn (generated, docs), then
// whatever is left. Generated files are flagged so the surface can fold them.

/// The sections a change is read in, in review order. The index is the sort
/// key; lower reads first.
pub(crate) const SECTION_ORDER: [&str; 8] = [
    "contracts",
    "core",
    "server",
    "ui",
    "tests",
    "generated",
    "docs",
    "other",
];

/// A one-line human label per section, for the surface.
pub(crate) fn section_label(section: &str) -> &'static str {
    match section {
        "contracts" => "Contracts and schemas",
        "core" => "Core behavior",
        "server" => "Server and routes",
        "ui" => "Product UI",
        "tests" => "Tests and proof",
        "generated" => "Generated artifacts",
        "docs" => "Docs",
        _ => "Other",
    }
}

pub(crate) fn section_index(section: &str) -> usize {
    SECTION_ORDER
        .iter()
        .position(|candidate| *candidate == section)
        .unwrap_or(SECTION_ORDER.len())
}

/// Classify a changed file path into (section, is_generated). Generated churn
/// is anything the build regenerates rather than hand-writes - it should read
/// last and fold by default, so it never hides the real change. Precedence is
/// deliberate: generated wins first (it is never "real behavior"), then the
/// contract shapes, then tests (so a test file under core is read as a test,
/// not as core), then UI, docs, and finally the crate split.
pub(crate) fn classify_path(path: &str) -> (&'static str, bool) {
    let lower = path.to_ascii_lowercase();
    let base = lower.rsplit('/').next().unwrap_or(&lower);

    // Generated churn: regenerated artifacts and lockfiles. Real, but not the
    // change a reviewer reasons about - so it folds.
    if crate::forge_repo_profile::is_artifact_path(path)
        || lower.starts_with("generated/")
        || base == "cargo.lock"
        || base == "package-lock.json"
        || base == "pnpm-lock.yaml"
    {
        return ("generated", true);
    }

    // Contracts and schemas: the shape a change commits to. Read first.
    if base == "http_routes.rs"
        || lower.ends_with(".schema.json")
        || lower.contains("openapi")
        || lower.contains("/response-schemas/")
        || base.ends_with("_contract.rs")
    {
        return ("contracts", false);
    }

    // Tests and proof - checked before the crate split so a test file inside
    // a crate reads as a test, not as core or server behavior.
    if lower.contains("/tests/")
        || base == "tests.rs"
        || base.ends_with("_test.rs")
        || base.ends_with("_tests.rs")
        || lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.contains("/scripts/")
            && (lower.ends_with("-check.sh") || lower.ends_with("-check.mjs"))
    {
        return ("tests", false);
    }

    if lower.starts_with("apps/") {
        return ("ui", false);
    }

    if lower.ends_with(".md") || lower.starts_with("docs/") {
        return ("docs", false);
    }

    if lower.starts_with("crates/mdx-core/") {
        return ("core", false);
    }
    if lower.starts_with("crates/mdx-server/") || lower.starts_with("crates/") {
        return ("server", false);
    }

    ("other", false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_read_in_a_deterministic_order() {
        // Lower index reads first; contracts before core before generated.
        assert!(section_index("contracts") < section_index("core"));
        assert!(section_index("core") < section_index("server"));
        assert!(section_index("server") < section_index("ui"));
        assert!(section_index("ui") < section_index("tests"));
        assert!(section_index("tests") < section_index("generated"));
    }

    #[test]
    fn classification_separates_real_change_from_generated_churn() {
        // Generated wins first and is flagged so it folds.
        assert_eq!(
            classify_path("generated/routes/mdx-local-routes.json"),
            ("generated", true)
        );
        assert_eq!(
            classify_path(".build/checkouts/Package/Sources/Package.swift"),
            ("generated", true)
        );
        assert_eq!(classify_path("target/debug/deps/app"), ("generated", true));
        assert_eq!(classify_path("Cargo.lock"), ("generated", true));
        // Contracts read first and are NOT generated churn.
        assert_eq!(
            classify_path("crates/mdx-core/src/http_routes.rs"),
            ("contracts", false)
        );
        assert_eq!(
            classify_path("generated/response-schemas/x.schema.json"),
            ("generated", true)
        );
        // A test file inside a crate reads as a test, not as core/server.
        assert_eq!(
            classify_path("crates/mdx-core/src/tests/pages_publication.rs"),
            ("tests", false)
        );
        assert_eq!(
            classify_path("crates/mdx-server/src/tests.rs"),
            ("tests", false)
        );
        // Real behavior by crate, UI by app, docs by extension.
        assert_eq!(
            classify_path("crates/mdx-core/src/connector_ingest.rs"),
            ("core", false)
        );
        assert_eq!(
            classify_path("crates/mdx-server/src/connector_route.rs"),
            ("server", false)
        );
        assert_eq!(
            classify_path("apps/mdx-host/src/routes/forge/+page.svelte"),
            ("ui", false)
        );
        assert_eq!(
            classify_path("docs/UI-PRODUCT-SURFACE-CONTRACT.md"),
            ("docs", false)
        );
        assert_eq!(classify_path("README"), ("other", false));
    }
}
