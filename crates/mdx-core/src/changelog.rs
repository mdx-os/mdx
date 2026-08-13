//! Changelog entries: the human- or agent-authored notes that say what
//! changed in the product and why it matters. A changelog entry is a
//! governed verb - a person or agent records it with their identity, and
//! the receipt is the durable record.
//!
//! An entry is a title, a short summary, and a kind (feature, fix, or
//! improvement). The kernel records its shape; it never decides what is
//! worth a changelog - that judgment travels in the words.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

/// What a changelog entry is about. Anything else is refused.
pub const CHANGELOG_KINDS: &[&str] = &["feature", "fix", "improvement"];

const MAX_TITLE_CHARS: usize = 200;
const MAX_SUMMARY_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct ChangelogEntry<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub title: &'a str,
    pub summary: &'a str,
    /// One of CHANGELOG_KINDS.
    pub kind: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangelogReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangelogError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownKind(String),
}

impl ChangelogError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("record a changelog entry: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("changelog {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownKind(other) => format!(
                "changelog kind must be one of {}; got {other}",
                CHANGELOG_KINDS.join(", ")
            ),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_changelog_entry(
        &mut self,
        request: ChangelogEntry<'_>,
    ) -> Result<ChangelogReport, ChangelogError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_changelog_entry_with_identity(request, &identity)
    }

    pub fn record_changelog_entry_with_identity(
        &mut self,
        request: ChangelogEntry<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ChangelogReport, ChangelogError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("title", request.title),
            ("summary", request.summary),
            ("kind", request.kind),
        ] {
            if value.trim().is_empty() {
                return Err(ChangelogError::Missing(field));
            }
        }
        if request.title.chars().count() > MAX_TITLE_CHARS {
            return Err(ChangelogError::TooLong(
                "title",
                request.title.chars().count(),
                MAX_TITLE_CHARS,
            ));
        }
        if request.summary.chars().count() > MAX_SUMMARY_CHARS {
            return Err(ChangelogError::TooLong(
                "summary",
                request.summary.chars().count(),
                MAX_SUMMARY_CHARS,
            ));
        }
        let kind = request.kind.trim();
        if !CHANGELOG_KINDS.contains(&kind) {
            return Err(ChangelogError::UnknownKind(kind.to_string()));
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "changelog",
                action: ActionKind::RecordChangelogEntry,
                transition: "RECORD_CHANGELOG_ENTRY",
                kind: "changelog.entry.recorded",
            },
            payload(&[
                ("title", request.title),
                ("summary", request.summary),
                ("kind", kind),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ChangelogReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_changelog_entry_records_its_shape_and_an_unknown_kind_is_refused() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .record_changelog_entry(ChangelogEntry {
                tenant_id: "t",
                actor_id: "human:eng",
                title: "Forge ships fleet plans",
                summary: "Forge can now run several streams against one repo.",
                kind: "feature",
            })
            .expect("record");
        assert!(!report.receipt_id.is_empty());
        assert!(!report.policy_decision_id.is_empty());
        assert_eq!(
            kernel
                .ledger()
                .query()
                .by_kind("changelog.entry.recorded")
                .len(),
            1
        );

        // A kind outside the vocabulary is refused, recording nothing.
        let bad = kernel.record_changelog_entry(ChangelogEntry {
            tenant_id: "t",
            actor_id: "human:eng",
            title: "x",
            summary: "y",
            kind: "rumor",
        });
        assert!(matches!(bad, Err(ChangelogError::UnknownKind(_))));

        // An empty title is refused before anything is recorded.
        let empty = kernel.record_changelog_entry(ChangelogEntry {
            tenant_id: "t",
            actor_id: "human:eng",
            title: "",
            summary: "y",
            kind: "fix",
        });
        assert!(matches!(empty, Err(ChangelogError::Missing("title"))));
        assert!(kernel.ledger().verify().is_ok());
    }
}
