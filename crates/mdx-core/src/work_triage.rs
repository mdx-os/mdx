//! The triage stream's verbs. One company-wide inbox of incoming
//! signal: manual entries, flagged messages, and the feedback rail's
//! captures (their note text already passed the safety boundary at
//! capture time; triage is where reading it belongs - the watchtower
//! stays metadata-only). Two verbs:
//!
//! - RECORD an entry: a piece of signal lands on the stream with its
//!   text and where it came from.
//! - RECORD a verdict: every entry leaves the stream through one of
//!   four calls - accepted, duplicate, declined, snoozed. Accepting
//!   turns signal into work: it mints a work item or seeds a Radar bet
//!   with a derived id, so re-accepting re-shapes instead of
//!   duplicating (the same handoff pattern as ratify-seeds-bet).
//!
//! Neither verb opens authority.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

/// How an entry leaves the stream. Linear's four, unchanged: they are
/// the complete set of honest answers to "what is this signal worth".
pub const TRIAGE_VERDICTS: &[&str] = &["accepted", "duplicate", "declined", "snoozed"];

/// Where an entry can come from when posted directly. Feedback captures
/// join the stream from their own receipts and never post here.
pub const TRIAGE_ENTRY_SOURCES: &[&str] = &["manual", "message", "signal"];

/// What an accepted entry becomes.
pub const TRIAGE_ACCEPT_TARGETS: &[&str] = &["work_item", "bet"];

const MAX_TEXT_CHARS: usize = 1000;
const MAX_FIELD_CHARS: usize = 500;
const MAX_TITLE_CHARS: usize = 300;

#[derive(Clone, Copy, Debug)]
pub struct TriageEntry<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// The signal itself, in the words it arrived in.
    pub text: &'a str,
    /// One of TRIAGE_ENTRY_SOURCES; empty means manual.
    pub source: &'a str,
    /// Where it came from: a message id, a page, a receipt.
    pub source_ref: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct TriageVerdict<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// A triage entry id, or a feedback capture's receipt id.
    pub entry_ref: &'a str,
    pub verdict: &'a str,
    pub note: &'a str,
    /// Required when the verdict is accepted: work_item or bet.
    pub accept_as: &'a str,
    /// The minted record's title; falls back to the entry's own text.
    pub accept_title: &'a str,
    /// The bet an accepted work item is filed under. Optional.
    pub bet_id: &'a str,
    /// Required when the verdict is duplicate: what it duplicates.
    pub duplicate_of: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriageWriteReport {
    pub record_id: String,
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub terminal_state: &'static str,
    /// Set when an accepted verdict minted a record.
    pub accepted_record_id: Option<String>,
    pub accepted_receipt_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriageError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownVerdict(String),
    UnknownSource(String),
    UnknownAcceptTarget(String),
    DuplicateRefRequired,
    AcceptTitleRequired,
}

impl TriageError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("triage write missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("triage {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownVerdict(other) => format!(
                "triage verdict must be one of {}; got {other}",
                TRIAGE_VERDICTS.join(", ")
            ),
            Self::UnknownSource(other) => format!(
                "triage entry source must be one of {}; got {other}",
                TRIAGE_ENTRY_SOURCES.join(", ")
            ),
            Self::UnknownAcceptTarget(other) => format!(
                "an accepted entry becomes one of {}; got {other}",
                TRIAGE_ACCEPT_TARGETS.join(", ")
            ),
            Self::DuplicateRefRequired => {
                "a duplicate verdict names what it duplicates in duplicate_of".to_string()
            }
            Self::AcceptTitleRequired => "accepting needs a title - the entry was not found on \
                 the stream, so pass accept_title with the words to mint"
                .to_string(),
        }
    }
}

fn check_len(field: &'static str, value: &str, max: usize) -> Result<(), TriageError> {
    let len = value.chars().count();
    if len > max {
        return Err(TriageError::TooLong(field, len, max));
    }
    Ok(())
}

fn truncate_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_triage_entry_local_with_identity(
        &mut self,
        request: TriageEntry<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<TriageWriteReport, TriageError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("text", request.text),
        ] {
            if value.trim().is_empty() {
                return Err(TriageError::Missing(field));
            }
        }
        check_len("text", request.text, MAX_TEXT_CHARS)?;
        check_len("source_ref", request.source_ref, MAX_FIELD_CHARS)?;
        let source = if request.source.trim().is_empty() {
            "manual"
        } else {
            request.source.trim()
        };
        if !TRIAGE_ENTRY_SOURCES.contains(&source) {
            return Err(TriageError::UnknownSource(source.to_string()));
        }
        let entry_id = self.ids.next("triage_entry");
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "product_shaping",
                action: ActionKind::RecordTriageEntry,
                transition: "RECORD_TRIAGE_ENTRY",
                kind: "triage.entry.recorded",
            },
            payload(&[
                ("entry_id", &entry_id),
                ("text", request.text),
                ("source", source),
                ("source_ref", request.source_ref),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(TriageWriteReport {
            record_id: entry_id,
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            terminal_state: "TRIAGE_ENTRY_RECORDED",
            accepted_record_id: None,
            accepted_receipt_id: None,
        })
    }

    pub fn save_triage_verdict_local_with_identity(
        &mut self,
        request: TriageVerdict<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<TriageWriteReport, TriageError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("entry_ref", request.entry_ref),
            ("verdict", request.verdict),
        ] {
            if value.trim().is_empty() {
                return Err(TriageError::Missing(field));
            }
        }
        if !TRIAGE_VERDICTS.contains(&request.verdict) {
            return Err(TriageError::UnknownVerdict(request.verdict.to_string()));
        }
        check_len("note", request.note, MAX_FIELD_CHARS)?;
        check_len("accept_title", request.accept_title, MAX_TITLE_CHARS)?;
        check_len("bet_id", request.bet_id, MAX_FIELD_CHARS)?;
        check_len("duplicate_of", request.duplicate_of, MAX_FIELD_CHARS)?;
        if request.verdict == "duplicate" && request.duplicate_of.trim().is_empty() {
            return Err(TriageError::DuplicateRefRequired);
        }
        let mut accepted_record_id = None;
        let mut accepted_receipt_id = None;
        let mut accepted_as = "";
        if request.verdict == "accepted" {
            let accept_as = request.accept_as.trim();
            if accept_as.is_empty() {
                return Err(TriageError::Missing("accept_as"));
            }
            if !TRIAGE_ACCEPT_TARGETS.contains(&accept_as) {
                return Err(TriageError::UnknownAcceptTarget(accept_as.to_string()));
            }
            accepted_as = if accept_as == "bet" {
                "bet"
            } else {
                "work_item"
            };
            // The minted record speaks in the entry's own words: the
            // title is the caller's, or the entry's text from the
            // stream (a triage entry's text, or a feedback capture's
            // already-scanned note).
            let entry_text = self.triage_entry_text(request.entry_ref);
            let title = if request.accept_title.trim().is_empty() {
                truncate_chars(entry_text.trim(), MAX_TITLE_CHARS)
            } else {
                request.accept_title.trim().to_string()
            };
            if title.is_empty() {
                return Err(TriageError::AcceptTitleRequired);
            }
            if accepted_as == "bet" {
                let seeded_id = format!("product_bet_for_triage_{}", request.entry_ref.trim());
                if let Ok(bet) = self.save_product_bet_draft_local_with_identity(
                    crate::ProductBetDraft {
                        tenant_id: request.tenant_id,
                        actor_id: request.actor_id,
                        bet_id: &seeded_id,
                        bet: &title,
                        for_whom: "",
                        success_metric: "",
                        kill_condition: "",
                        direction_ref: "",
                        slice: "",
                        slice_not: "",
                        signal_ref: request.entry_ref,
                    },
                    identity,
                ) {
                    accepted_record_id = Some(bet.record_id);
                    accepted_receipt_id = Some(bet.receipt_id);
                }
            } else {
                let seeded_id = format!("work_item_for_{}", request.entry_ref.trim());
                if let Ok(item) = self.save_work_item_local_with_identity(
                    crate::WorkItemShape {
                        tenant_id: request.tenant_id,
                        actor_id: request.actor_id,
                        work_item_id: &seeded_id,
                        title: &title,
                        description: &entry_text,
                        bet_id: request.bet_id,
                        owner_id: "",
                        approval: "",
                        page_ref: "",
                        build_ref: "",
                    },
                    identity,
                ) {
                    accepted_record_id = Some(item.record_id);
                    accepted_receipt_id = Some(item.receipt_id);
                }
            }
        }
        let verdict_id = self.ids.next("triage_verdict");
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "product_shaping",
                action: ActionKind::RecordTriageVerdict,
                transition: "RECORD_TRIAGE_VERDICT",
                kind: "triage.verdict.recorded",
            },
            payload(&[
                ("verdict_id", &verdict_id),
                ("entry_ref", request.entry_ref),
                ("verdict", request.verdict),
                ("note", request.note),
                ("duplicate_of", request.duplicate_of),
                ("accepted_as", accepted_as),
                (
                    "accepted_record_id",
                    accepted_record_id.as_deref().unwrap_or(""),
                ),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(TriageWriteReport {
            record_id: verdict_id,
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            terminal_state: "TRIAGE_VERDICT_RECORDED",
            accepted_record_id,
            accepted_receipt_id,
        })
    }

    /// The entry's own words, from the stream: a triage entry's text,
    /// or a feedback capture's note (already scanned at the capture
    /// boundary). Empty when the reference is not on the stream.
    fn triage_entry_text(&self, entry_ref: &str) -> String {
        let entry_ref = entry_ref.trim();
        if let Some(text) = self
            .ledger()
            .query()
            .by_kind("triage.entry.recorded")
            .iter()
            .find(|entry| entry.payload.get("entry_id").map(String::as_str) == Some(entry_ref))
            .and_then(|entry| entry.payload.get("text").cloned())
        {
            return text;
        }
        self.ledger()
            .query()
            .by_kind("beta.feedback.captured")
            .iter()
            .find(|entry| entry.receipt_id == entry_ref)
            .and_then(|entry| entry.payload.get("note").cloned())
            .unwrap_or_default()
    }
}
