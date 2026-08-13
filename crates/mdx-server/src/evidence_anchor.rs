//! External anchoring for evidence checkpoints: the seam, and exactly one
//! working adapter. Anchoring writes only a checkpoint's hash, root, and
//! signature to an external store - never a receipt, never a payload - so
//! an auditor can later prove the checkpoint existed at anchor time
//! independent of MDx.
//!
//! One real adapter ships: a local append-only journal that refuses to
//! rewrite a line (WORM-style). The trait is the seam for the futures the
//! ADR names (object-lock storage, a transparency log, a chain) - none of
//! which exist yet, so no product copy claims them.

use std::io::Write;

/// What an anchor adapter records. Hashes and references only.
pub(crate) struct AnchorRequest<'a> {
    pub checkpoint_id: &'a str,
    pub checkpoint_hash: &'a str,
    pub merkle_root: &'a str,
    pub signature: &'a str,
}

pub(crate) struct AnchorResult {
    pub adapter: String,
    pub status: String,
    pub anchor_ref: String,
}

pub(crate) trait AuditAnchorAdapter {
    fn name(&self) -> &'static str;
    fn anchor(&self, request: &AnchorRequest<'_>) -> Result<AnchorResult, String>;
}

/// A local append-only journal. Each anchor is one line; the adapter
/// refuses to anchor a checkpoint hash that is already present, so the
/// journal is write-once per checkpoint - the honest local stand-in for
/// WORM storage.
pub(crate) struct LocalFileAnchorAdapter {
    path: String,
}

impl LocalFileAnchorAdapter {
    pub(crate) fn from_env() -> Self {
        let path = std::env::var("MDX_AUDIT_ANCHOR_FILE")
            .ok()
            .filter(|p| !p.trim().is_empty())
            .unwrap_or_else(|| ".mdx-local/audit-anchor-journal.ndjson".to_string());
        Self { path }
    }
}

impl AuditAnchorAdapter for LocalFileAnchorAdapter {
    fn name(&self) -> &'static str {
        "local_file_worm"
    }

    fn anchor(&self, request: &AnchorRequest<'_>) -> Result<AnchorResult, String> {
        if let Ok(existing) = std::fs::read_to_string(&self.path)
            && existing.contains(request.checkpoint_hash)
        {
            return Err(format!(
                "checkpoint {} is already anchored in {}; the journal is write-once",
                request.checkpoint_id, self.path
            ));
        }
        if let Some(parent) = std::path::Path::new(&self.path).parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("create anchor dir: {error}"))?;
        }
        let line = serde_json::json!({
            "checkpoint_id": request.checkpoint_id,
            "checkpoint_hash": request.checkpoint_hash,
            "merkle_root": request.merkle_root,
            "signature": request.signature,
        })
        .to_string();
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| format!("open anchor journal: {error}"))?;
        writeln!(file, "{line}").map_err(|error| format!("append anchor journal: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("sync anchor journal: {error}"))?;
        Ok(AnchorResult {
            adapter: self.name().to_string(),
            status: "anchored".to_string(),
            anchor_ref: format!("{}#{}", self.path, request.checkpoint_hash),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_journal_is_write_once_per_checkpoint() {
        let dir = std::env::temp_dir().join("mdx-anchor-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("journal.ndjson").to_string_lossy().to_string();
        let adapter = LocalFileAnchorAdapter { path: path.clone() };
        let request = AnchorRequest {
            checkpoint_id: "cp_1",
            checkpoint_hash: "abc123",
            merkle_root: "root",
            signature: "sig",
        };
        let first = adapter.anchor(&request).expect("first anchor");
        assert_eq!(first.status, "anchored");
        assert!(first.anchor_ref.contains("abc123"));
        // A second anchor of the same hash refuses - write-once.
        assert!(adapter.anchor(&request).is_err());
        // A different checkpoint appends fine.
        let other = AnchorRequest {
            checkpoint_hash: "def456",
            ..request
        };
        assert!(adapter.anchor(&other).is_ok());
        let body = std::fs::read_to_string(&path).expect("read journal");
        assert_eq!(body.lines().count(), 2);
        // No payload text could be here - only the four hash/ref fields.
        assert!(body.contains("checkpoint_hash"));
    }
}
