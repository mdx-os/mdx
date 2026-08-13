// The authored-body store: when a person writes a page on this machine,
// the words land as a write-once file under .mdx-local and the kernel
// records the REFERENCE - so the file IS the recorded source and "Pages
// keeps no copy" stays true for authored pages exactly as it is for the
// charter corpus. Reads are confined to the two store prefixes with
// sanitized names - a body_ref can never walk the filesystem.
use std::path::Path;

pub(crate) const DRAFT_BODIES_DIR: &str = ".mdx-local/pages-draft-bodies";
pub(crate) const PUBLISHED_BODIES_DIR: &str = ".mdx-local/pages-published-bodies";

fn sanitized(id: &str) -> Option<String> {
    if id.is_empty() || id.len() > 80 {
        return None;
    }
    if id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        Some(id.to_string())
    } else {
        None
    }
}

pub(crate) fn store_draft_body(draft_id: &str, body_text: &str) -> Option<String> {
    let name = sanitized(draft_id)?;
    std::fs::create_dir_all(DRAFT_BODIES_DIR).ok()?;
    let path = format!("{DRAFT_BODIES_DIR}/{name}.md");
    std::fs::write(&path, body_text).ok()?;
    Some(path)
}

pub(crate) fn store_published_body(
    document_id: &str,
    revision_id: &str,
    body_text: &str,
) -> Option<String> {
    let document = sanitized(document_id)?;
    let revision = sanitized(revision_id)?;
    std::fs::create_dir_all(PUBLISHED_BODIES_DIR).ok()?;
    let path = format!("{PUBLISHED_BODIES_DIR}/{document}__{revision}.md");
    // Write-once: a published revision's body never changes underneath it.
    if !Path::new(&path).exists() {
        std::fs::write(&path, body_text).ok()?;
    }
    Some(path)
}

// Serve a stored body ONLY when the recorded reference sits inside one of
// the two store directories and names a sanitized file.
pub(crate) fn read_stored_body(body_ref: &str) -> Option<String> {
    let rest = body_ref
        .strip_prefix(&format!("{DRAFT_BODIES_DIR}/"))
        .or_else(|| body_ref.strip_prefix(&format!("{PUBLISHED_BODIES_DIR}/")))?;
    let stem = rest.strip_suffix(".md")?;
    let valid = stem
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
    if !valid {
        return None;
    }
    std::fs::read_to_string(body_ref).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_outside_the_store_never_read() {
        assert!(read_stored_body("docs/ONBOARDING.md").is_none());
        assert!(read_stored_body(".mdx-local/pages-draft-bodies/../secrets.md").is_none());
        assert!(read_stored_body(".mdx-local/pages-draft-bodies/UPPER.md").is_none());
    }

    #[test]
    fn store_and_read_round_trip_with_write_once_publish() {
        let draft_ref = store_draft_body("draft_p2_test", "hello draft").expect("draft stored");
        assert_eq!(read_stored_body(&draft_ref).as_deref(), Some("hello draft"));
        let pub_ref = store_published_body("page_p2_test", "rev_001", "hello published")
            .expect("published stored");
        assert_eq!(
            read_stored_body(&pub_ref).as_deref(),
            Some("hello published")
        );
        // a second publish to the same revision never rewrites the body
        store_published_body("page_p2_test", "rev_001", "tampered").expect("idempotent");
        assert_eq!(
            read_stored_body(&pub_ref).as_deref(),
            Some("hello published")
        );
        let _ = std::fs::remove_file(draft_ref);
        let _ = std::fs::remove_file(pub_ref);
    }

    #[test]
    fn bad_ids_refuse_to_store() {
        assert!(store_draft_body("../evil", "x").is_none());
        assert!(store_draft_body("", "x").is_none());
    }
}
