//! The connector rail, slice 1: bringing content from OUTSIDE MDx into the
//! graph under governance. A connector ingests an external item - an X post, a
//! repo doc, a web page - and the kernel records it as a governed verb with
//! identity. The defining mark is provenance: every ingested item carries
//! `origin = "external"`, the source it came from, and a data-sensitivity
//! class. The kernel never treats external content as truth; it records what
//! was brought in, from where, and how sensitive it is, so the world model can
//! surface it as recon that is always traceable and never asserted.
//!
//! Sensitivity is fail-closed against the sovereign-routing stance: sensitive
//! external content must declare sovereign handling, or it is refused. The
//! live API fetch (and the source-authorization registry) are deliberately
//! NOT here - they land behind the suspend-for-external-call gate. This slice
//! governs the SHAPE of ingest: what an external item is, and the residency
//! line it must clear, independent of how the bytes arrived.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

/// The kind of system an item came from. Anything else is refused - a closed
/// set keeps the source space legible as connectors grow.
pub const CONNECTOR_KINDS: &[&str] = &[
    "x",
    "github",
    "web",
    "confluence",
    "notion",
    "rss",
    "slack",
    "teams",
];

/// The data-sensitivity class of an external item. `normal` is the default;
/// `sensitive` triggers the sovereign-handling requirement.
pub const CONNECTOR_SENSITIVITY: &[&str] = &["normal", "sensitive"];

/// Where the content is allowed to be processed. Sensitive content must be
/// `sovereign`; everything else may ride `frontier`. The class lands on the
/// receipt as provable data residency.
pub const CONNECTOR_HANDLING: &[&str] = &["frontier", "sovereign"];

/// The v1 Focus grading, advisory and never blocking: Signal, Directional
/// insight, Operational. `none` for an ungraded pull.
pub const CONNECTOR_GRADES: &[&str] = &["none", "signal", "directional", "operational"];

/// Who a connector belongs to. `tenant` is a company-shared source (the org
/// repo, the brand account) authorized at the admin level and feeding the
/// shared world model; `personal` is one operator's own source, scoped to
/// them, that must not leak into tenant-wide truth. Default is `tenant`.
pub const CONNECTOR_SCOPES: &[&str] = &["tenant", "personal"];

const MAX_TITLE_CHARS: usize = 300;
const MAX_SUMMARY_CHARS: usize = 4000;

#[derive(Clone, Copy, Debug)]
pub struct ExternalItem<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// The connector source, e.g. "x:realtalkwithmd" or "github:mdx-stack".
    pub source_id: &'a str,
    /// One of CONNECTOR_KINDS.
    pub source_kind: &'a str,
    /// The upstream pointer the item lives at - a URL or external id. This is
    /// the provenance anchor; ingest without it is refused.
    pub external_ref: &'a str,
    pub title: &'a str,
    pub summary: &'a str,
    /// Where the body is stored, if it was. Empty when only the reference and
    /// summary were brought in.
    pub body_ref: &'a str,
    /// One of CONNECTOR_SENSITIVITY. Empty defaults to `normal`.
    pub data_sensitivity: &'a str,
    /// One of CONNECTOR_HANDLING. Empty defaults to `frontier`.
    pub handling: &'a str,
    /// One of CONNECTOR_GRADES. Empty defaults to `none`.
    pub grade: &'a str,
    /// One of CONNECTOR_SCOPES. Empty defaults to `tenant`. A `personal` item
    /// belongs to its actor, not the company.
    pub scope: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalItemReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub data_sensitivity: &'static str,
    pub handling: &'static str,
    pub grade: &'static str,
    pub scope: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct ExternalItemDeletion<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub item_receipt_id: &'a str,
    pub reason: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalItemDeletionReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub item_receipt_id: String,
    pub deletion_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExternalItemError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownKind(String),
    UnknownSensitivity(String),
    UnknownGrade(String),
    UnknownScope(String),
    RequiresSovereign,
    UnknownItem(String),
}

impl ExternalItemError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("ingest an external item: missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("external item {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownKind(other) => format!(
                "connector kind must be one of {}; got {other}",
                CONNECTOR_KINDS.join(", ")
            ),
            Self::UnknownSensitivity(other) => format!(
                "data sensitivity must be one of {}; got {other}",
                CONNECTOR_SENSITIVITY.join(", ")
            ),
            Self::UnknownGrade(other) => format!(
                "grade must be one of {}; got {other}",
                CONNECTOR_GRADES.join(", ")
            ),
            Self::UnknownScope(other) => format!(
                "scope must be one of {}; got {other}",
                CONNECTOR_SCOPES.join(", ")
            ),
            Self::RequiresSovereign => {
                "sensitive external content must declare sovereign handling".to_string()
            }
            Self::UnknownItem(item_receipt_id) => {
                format!("connector item {item_receipt_id} is unknown")
            }
        }
    }
}

fn normalize_one(
    raw: &str,
    allowed: &[&'static str],
    default: &'static str,
    err: impl FnOnce(String) -> ExternalItemError,
) -> Result<&'static str, ExternalItemError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(default);
    }
    let lowered = trimmed.to_ascii_lowercase();
    allowed
        .iter()
        .copied()
        .find(|value| *value == lowered)
        .ok_or_else(|| err(trimmed.to_string()))
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_external_item(
        &mut self,
        request: ExternalItem<'_>,
    ) -> Result<ExternalItemReport, ExternalItemError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_external_item_with_identity(request, &identity)
    }

    pub fn record_external_item_with_identity(
        &mut self,
        request: ExternalItem<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ExternalItemReport, ExternalItemError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("source_id", request.source_id),
            ("source_kind", request.source_kind),
            ("external_ref", request.external_ref),
            ("title", request.title),
        ] {
            if value.trim().is_empty() {
                return Err(ExternalItemError::Missing(field));
            }
        }
        if request.title.chars().count() > MAX_TITLE_CHARS {
            return Err(ExternalItemError::TooLong(
                "title",
                request.title.chars().count(),
                MAX_TITLE_CHARS,
            ));
        }
        if request.summary.chars().count() > MAX_SUMMARY_CHARS {
            return Err(ExternalItemError::TooLong(
                "summary",
                request.summary.chars().count(),
                MAX_SUMMARY_CHARS,
            ));
        }
        let source_kind = request.source_kind.trim().to_ascii_lowercase();
        if !CONNECTOR_KINDS.contains(&source_kind.as_str()) {
            return Err(ExternalItemError::UnknownKind(source_kind));
        }
        let data_sensitivity = normalize_one(
            request.data_sensitivity,
            CONNECTOR_SENSITIVITY,
            "normal",
            ExternalItemError::UnknownSensitivity,
        )?;
        // Handling is fail-closed: only an exact `sovereign` grants sovereign
        // processing. Anything else (empty, a typo) is `frontier`, and a
        // sensitive item then trips the residency gate below - a misspelled
        // `sovereign` never quietly clears sensitive content.
        let handling = if request.handling.trim().eq_ignore_ascii_case("sovereign") {
            "sovereign"
        } else {
            "frontier"
        };
        let grade = normalize_one(
            request.grade,
            CONNECTOR_GRADES,
            "none",
            ExternalItemError::UnknownGrade,
        )?;
        let scope = normalize_one(
            request.scope,
            CONNECTOR_SCOPES,
            "tenant",
            ExternalItemError::UnknownScope,
        )?;
        // Fail-closed residency: sensitive content cannot ride frontier.
        if data_sensitivity == "sensitive" && handling != "sovereign" {
            return Err(ExternalItemError::RequiresSovereign);
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "connector_ingest",
                action: ActionKind::IngestExternalItem,
                transition: "INGEST_EXTERNAL_ITEM",
                kind: "connector.item.ingested",
            },
            payload(&[
                ("origin", "external"),
                ("source_id", request.source_id),
                ("source_kind", &source_kind),
                ("external_ref", request.external_ref),
                ("title", request.title),
                ("summary", request.summary),
                ("body_ref", request.body_ref),
                ("data_sensitivity", data_sensitivity),
                ("handling", handling),
                ("grade", grade),
                ("scope", scope),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ExternalItemReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            data_sensitivity,
            handling,
            grade,
            scope,
        })
    }

    pub fn delete_external_item(
        &mut self,
        request: ExternalItemDeletion<'_>,
    ) -> Result<ExternalItemDeletionReport, ExternalItemError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.delete_external_item_with_identity(request, &identity)
    }

    pub fn delete_external_item_with_identity(
        &mut self,
        request: ExternalItemDeletion<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ExternalItemDeletionReport, ExternalItemError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("item_receipt_id", request.item_receipt_id),
            ("reason", request.reason),
        ] {
            if value.trim().is_empty() {
                return Err(ExternalItemError::Missing(field));
            }
        }
        let item = self
            .ledger()
            .entries()
            .iter()
            .find(|receipt| {
                receipt.kind == "connector.item.ingested"
                    && receipt.receipt_id == request.item_receipt_id
            })
            .cloned()
            .ok_or_else(|| ExternalItemError::UnknownItem(request.item_receipt_id.to_string()))?;
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "connector_delete",
                action: ActionKind::DeleteExternalItem,
                transition: "DELETE_EXTERNAL_ITEM",
                kind: "connector.item.delete_requested",
            },
            payload(&[
                ("item_receipt_id", request.item_receipt_id),
                (
                    "source_id",
                    item.payload
                        .get("source_id")
                        .map(String::as_str)
                        .unwrap_or(""),
                ),
                (
                    "source_kind",
                    item.payload
                        .get("source_kind")
                        .map(String::as_str)
                        .unwrap_or(""),
                ),
                (
                    "external_ref",
                    item.payload
                        .get("external_ref")
                        .map(String::as_str)
                        .unwrap_or(""),
                ),
                (
                    "title",
                    item.payload.get("title").map(String::as_str).unwrap_or(""),
                ),
                (
                    "scope",
                    item.payload
                        .get("scope")
                        .map(String::as_str)
                        .unwrap_or("tenant"),
                ),
                ("reason", request.reason),
                ("deletion_state", "DELETED"),
                ("trusted_context_invalidated", "true"),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("raw_external_content_recorded", "false"),
                ("provider_call_allowed", "false"),
                ("memory_write_performed", "false"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ExternalItemDeletionReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            item_receipt_id: request.item_receipt_id.to_string(),
            deletion_state: "DELETED",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_external_item_records_its_provenance_and_unknown_kind_is_refused() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .record_external_item(ExternalItem {
                tenant_id: "t",
                actor_id: "agent:scout",
                source_id: "x:realtalkwithmd",
                source_kind: "x",
                external_ref: "https://x.com/realtalkwithmd/status/1",
                title: "Founder thread on context graphs",
                summary: "A take on why the why is the moat.",
                body_ref: "",
                data_sensitivity: "",
                handling: "",
                grade: "signal",
                scope: "",
            })
            .expect("record");
        assert!(!report.receipt_id.is_empty());
        assert_eq!(report.data_sensitivity, "normal");
        assert_eq!(report.handling, "frontier");
        assert_eq!(report.grade, "signal");
        assert_eq!(report.scope, "tenant");
        let receipt = kernel
            .ledger()
            .query()
            .by_kind("connector.item.ingested")
            .into_iter()
            .next()
            .expect("ingested receipt");
        assert_eq!(receipt.payload["origin"], "external");
        assert_eq!(receipt.payload["source_id"], "x:realtalkwithmd");

        // A kind outside the vocabulary is refused, recording nothing more.
        let bad = kernel.record_external_item(ExternalItem {
            tenant_id: "t",
            actor_id: "agent:scout",
            source_id: "s",
            source_kind: "carrier_pigeon",
            external_ref: "ref",
            title: "x",
            summary: "",
            body_ref: "",
            data_sensitivity: "",
            handling: "",
            grade: "",
            scope: "personal",
        });
        assert!(matches!(bad, Err(ExternalItemError::UnknownKind(_))));
    }

    #[test]
    fn sensitive_content_without_sovereign_handling_is_refused() {
        let mut kernel = MdxKernel::boot_local();
        let refused = kernel.record_external_item(ExternalItem {
            tenant_id: "t",
            actor_id: "agent:scout",
            source_id: "confluence:eng",
            source_kind: "confluence",
            external_ref: "https://eng/secret",
            title: "Regulated policy excerpt",
            summary: "",
            body_ref: "",
            data_sensitivity: "sensitive",
            handling: "frontier",
            grade: "",
            scope: "tenant",
        });
        assert!(matches!(refused, Err(ExternalItemError::RequiresSovereign)));
        assert_eq!(
            kernel
                .ledger()
                .query()
                .by_kind("connector.item.ingested")
                .len(),
            0
        );

        // The same item rides once it declares sovereign handling.
        let ok = kernel
            .record_external_item(ExternalItem {
                tenant_id: "t",
                actor_id: "agent:scout",
                source_id: "confluence:eng",
                source_kind: "confluence",
                external_ref: "https://eng/secret",
                title: "Regulated policy excerpt",
                summary: "",
                body_ref: "",
                data_sensitivity: "sensitive",
                handling: "sovereign",
                grade: "operational",
                scope: "tenant",
            })
            .expect("sovereign-handled record");
        assert_eq!(ok.data_sensitivity, "sensitive");
        assert_eq!(ok.handling, "sovereign");
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn external_item_deletion_records_invalidation_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let item = kernel
            .record_external_item(ExternalItem {
                tenant_id: "t",
                actor_id: "agent:scout",
                source_id: "github:mdx-stack",
                source_kind: "github",
                external_ref: "https://github.com/mdx-stack/docs/architecture.md",
                title: "Architecture note",
                summary: "",
                body_ref: "",
                data_sensitivity: "",
                handling: "",
                grade: "operational",
                scope: "tenant",
            })
            .expect("record");
        let deleted = kernel
            .delete_external_item(ExternalItemDeletion {
                tenant_id: "t",
                actor_id: "agent:scout",
                item_receipt_id: &item.receipt_id,
                reason: "Remove stale connector item.",
            })
            .expect("delete");
        assert_eq!(deleted.item_receipt_id, item.receipt_id);
        assert_eq!(deleted.deletion_state, "DELETED");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&deleted.receipt_id)
            .expect("delete receipt");
        assert_eq!(receipt.kind, "connector.item.delete_requested");
        assert_eq!(receipt.payload["trusted_context_invalidated"], "true");
        assert_eq!(receipt.payload["raw_external_content_recorded"], "false");
        assert!(kernel.ledger().verify().is_ok());
    }
}
