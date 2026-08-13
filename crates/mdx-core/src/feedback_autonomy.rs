use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, Receipt, StorageProvider, payload};
use std::collections::{BTreeMap, BTreeSet};

pub const FEEDBACK_AUTONOMY_ROUTE: &str = "/feedback/autonomy-runs.json";
pub const FEEDBACK_AUTONOMY_PROJECTION_ROUTE: &str = "/feedback/autonomy-runs/projection.json";
pub const BETA_FEEDBACK_CHANNEL_ID: &str = "beta-feedback";
pub const FEEDBACK_AUTONOMY_ACTOR_ID: &str = "agent:feedback_autonomy";

pub const FEEDBACK_AUTONOMY_RECEIPTS: &[&str] = &[
    "message.feedback.acknowledged",
    "product.feedback.assessed",
    "pages.feedback.assessment_published",
    "work.item.shaped",
    "forge.work.requested",
    "forge.evidence.attached",
    "pages.change.closeout_published",
    "message.feedback.closed_loop_replied",
];

pub const FEEDBACK_AUTONOMY_LANES: &[&str] = &[
    "autopilot_allowed",
    "autopilot_with_review",
    "batch_later",
    "needs_human",
    "declined_with_receipt",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeedbackAutonomyEvent<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub feedback_ref: &'a str,
    pub event: &'a str,
    pub lane: &'a str,
    pub detail: &'a str,
    pub page_ref: &'a str,
    pub work_item_ref: &'a str,
    pub forge_run_ref: &'a str,
    pub source_receipt_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackAutonomyEventReport {
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub terminal_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FeedbackAutonomyError {
    Missing(&'static str),
    UnknownLane(String),
    UnknownEvent(String),
    UnknownSourceReceipt(String),
}

impl FeedbackAutonomyError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("feedback autonomy event missing {field}"),
            Self::UnknownLane(lane) => {
                format!(
                    "feedback autonomy lane must be one of {}; got {lane}",
                    FEEDBACK_AUTONOMY_LANES.join(", ")
                )
            }
            Self::UnknownEvent(event) => {
                format!("feedback autonomy event {event} does not map to a receipt kind")
            }
            Self::UnknownSourceReceipt(id) => {
                format!("feedback autonomy source receipt {id} is unknown")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackAutonomyFeedback {
    pub feedback_ref: String,
    pub tenant_id: String,
    pub surface: String,
    pub route: String,
    pub actor_id: String,
    pub session_ref: String,
    pub category: String,
    pub note: String,
    pub captured_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackAutonomyRow {
    pub feedback_ref: String,
    pub surface: String,
    pub route: String,
    pub category: String,
    pub lane: String,
    pub lifecycle: String,
    pub assessment_page_ref: String,
    pub closeout_page_ref: String,
    pub work_item_ref: String,
    pub forge_run_ref: String,
    pub acknowledged: bool,
    pub assessed: bool,
    pub assessment_published: bool,
    pub work_shaped: bool,
    pub forge_requested: bool,
    pub evidence_attached: bool,
    pub closeout_published: bool,
    pub closed_loop_replied: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackAutonomyProjection {
    pub total_feedback: usize,
    pub processed_count: usize,
    pub open_count: usize,
    pub rows: Vec<FeedbackAutonomyRow>,
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_feedback_autonomy_event_with_identity(
        &mut self,
        event: FeedbackAutonomyEvent<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<FeedbackAutonomyEventReport, FeedbackAutonomyError> {
        for (field, value) in [
            ("tenant_id", event.tenant_id),
            ("actor_id", event.actor_id),
            ("feedback_ref", event.feedback_ref),
            ("event", event.event),
            ("lane", event.lane),
            ("source_receipt_id", event.source_receipt_id),
        ] {
            if value.trim().is_empty() {
                return Err(FeedbackAutonomyError::Missing(field));
            }
        }
        if !FEEDBACK_AUTONOMY_LANES.contains(&event.lane) {
            return Err(FeedbackAutonomyError::UnknownLane(event.lane.to_string()));
        }
        if self
            .storage
            .ledger()
            .query()
            .by_id(event.source_receipt_id)
            .is_none()
        {
            return Err(FeedbackAutonomyError::UnknownSourceReceipt(
                event.source_receipt_id.to_string(),
            ));
        }
        let kind = autonomy_event_kind(event.event)
            .ok_or_else(|| FeedbackAutonomyError::UnknownEvent(event.event.to_string()))?;
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: event.tenant_id,
                actor_id: event.actor_id,
                loop_id: "feedback_autonomy",
                action: ActionKind::RecordFeedbackAutonomyEvent,
                transition: "RECORD_FEEDBACK_AUTONOMY_EVENT",
                kind,
            },
            payload(&[
                ("feedback_ref", event.feedback_ref),
                ("event", event.event),
                ("lane", event.lane),
                ("detail", event.detail),
                ("page_ref", event.page_ref),
                ("work_item_ref", event.work_item_ref),
                ("forge_run_ref", event.forge_run_ref),
                ("source_receipt_id", event.source_receipt_id),
                ("identity_source", &identity.identity_source),
                ("identity_actor_kind", &identity.actor_kind),
                ("identity_subject_actor_id", &identity.subject_actor_id),
                ("identity_delegation_id", &identity.delegation_id),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(FeedbackAutonomyEventReport {
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            terminal_state: "FEEDBACK_AUTONOMY_EVENT_RECORDED",
        })
    }

    pub fn feedback_autonomy_projection(&self) -> FeedbackAutonomyProjection {
        project_feedback_autonomy(self.storage.ledger().entries())
    }

    pub fn feedback_autonomy_already_closed(&self, feedback_ref: &str) -> bool {
        self.storage
            .ledger()
            .query()
            .by_kind("message.feedback.closed_loop_replied")
            .iter()
            .any(|receipt| payload_value(receipt, "feedback_ref") == feedback_ref)
    }
}

pub fn classify_feedback_for_autonomy(
    note: &str,
    category: &str,
    surface: &str,
) -> (&'static str, &'static str) {
    // Fail-closed FIRST, before any note pattern-match. A telemetry-origin capture
    // is synthesized from an UNTRUSTED app-reported route that is embedded verbatim
    // in the note; without this short-circuit a crafted route (e.g. one containing
    // "copy"/"typo"/"label") would steer the note-based rubric into
    // autopilot_allowed and mint an auto-approved Forge run. Telemetry signals are
    // always held for a person. This is additive: real user feedback on other
    // surfaces is unchanged.
    if surface == crate::APP_HEALTH_SELF_HEAL_SURFACE {
        return (
            "needs_human",
            "telemetry-origin signals are held for a person before any execution",
        );
    }
    let lower = note.to_ascii_lowercase();
    if [
        "screenshot",
        "screen shot",
        "video",
        "capture",
        "attachment",
        "recording",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        return (
            "autopilot_with_review",
            "media capture changes touch privacy and attachment boundaries",
        );
    }
    if lower.contains("crash")
        || lower.contains("data loss")
        || lower.contains("security")
        || lower.contains("permission")
    {
        return (
            "needs_human",
            "high-stakes failure modes need a person before execution",
        );
    }
    if category == "confusing"
        || lower.contains("typo")
        || lower.contains("copy")
        || lower.contains("label")
    {
        return (
            "autopilot_allowed",
            "low-risk copy or clarity fix can run with scoped evidence",
        );
    }
    if category == "idea" || category == "blocked" || category == "bug" {
        return (
            "autopilot_with_review",
            "useful product signal, but the first build needs explicit scope",
        );
    }
    (
        "batch_later",
        "not urgent enough for immediate autonomous execution",
    )
}

pub fn feedback_autonomy_feedback(entries: &[Receipt]) -> Vec<FeedbackAutonomyFeedback> {
    entries
        .iter()
        .filter(|receipt| receipt.kind == "beta.feedback.captured")
        .map(|receipt| FeedbackAutonomyFeedback {
            feedback_ref: receipt.receipt_id.clone(),
            tenant_id: receipt.tenant_id.as_str().to_string(),
            surface: payload_value(receipt, "surface").to_string(),
            route: payload_value(receipt, "route").to_string(),
            actor_id: payload_value(receipt, "identity_subject_actor_id").to_string(),
            session_ref: payload_value(receipt, "session_ref").to_string(),
            category: payload_value(receipt, "category").to_string(),
            note: payload_value(receipt, "note").to_string(),
            captured_at: payload_value(receipt, "occurred_at").to_string(),
        })
        .collect()
}

fn project_feedback_autonomy(entries: &[Receipt]) -> FeedbackAutonomyProjection {
    let feedback = feedback_autonomy_feedback(entries);
    let mut rows: BTreeMap<String, FeedbackAutonomyRow> = feedback
        .iter()
        .map(|feedback| {
            let (lane, _) = classify_feedback_for_autonomy(
                &feedback.note,
                &feedback.category,
                &feedback.surface,
            );
            (
                feedback.feedback_ref.clone(),
                FeedbackAutonomyRow {
                    feedback_ref: feedback.feedback_ref.clone(),
                    surface: feedback.surface.clone(),
                    route: feedback.route.clone(),
                    category: feedback.category.clone(),
                    lane: lane.to_string(),
                    lifecycle: "captured".to_string(),
                    assessment_page_ref: String::new(),
                    closeout_page_ref: String::new(),
                    work_item_ref: String::new(),
                    forge_run_ref: String::new(),
                    acknowledged: false,
                    assessed: false,
                    assessment_published: false,
                    work_shaped: false,
                    forge_requested: false,
                    evidence_attached: false,
                    closeout_published: false,
                    closed_loop_replied: false,
                },
            )
        })
        .collect();

    let shaped_work_items = entries
        .iter()
        .filter(|receipt| receipt.kind == "work.item.shaped")
        .map(|receipt| payload_value(receipt, "work_item_id").to_string())
        .collect::<BTreeSet<_>>();

    for receipt in entries {
        let feedback_ref = payload_value(receipt, "feedback_ref");
        if feedback_ref.is_empty() {
            continue;
        }
        if let Some(row) = rows.get_mut(feedback_ref) {
            let lane = payload_value(receipt, "lane");
            if !lane.is_empty() {
                row.lane = lane.to_string();
            }
            let page_ref = payload_value(receipt, "page_ref");
            let work_item_ref = payload_value(receipt, "work_item_ref");
            let forge_run_ref = payload_value(receipt, "forge_run_ref");
            match receipt.kind.as_str() {
                "message.feedback.acknowledged" => row.acknowledged = true,
                "product.feedback.assessed" => row.assessed = true,
                "pages.feedback.assessment_published" => {
                    row.assessment_published = true;
                    row.assessment_page_ref = page_ref.to_string();
                }
                "forge.work.requested" => {
                    row.forge_requested = true;
                    row.work_item_ref = work_item_ref.to_string();
                    row.forge_run_ref = forge_run_ref.to_string();
                }
                "forge.evidence.attached" => row.evidence_attached = true,
                "pages.change.closeout_published" => {
                    row.closeout_published = true;
                    row.closeout_page_ref = page_ref.to_string();
                }
                "message.feedback.closed_loop_replied" => row.closed_loop_replied = true,
                _ => {}
            }
        }
    }

    for row in rows.values_mut() {
        if !row.work_item_ref.is_empty() && shaped_work_items.contains(&row.work_item_ref) {
            row.work_shaped = true;
        }
        row.lifecycle = if row.closed_loop_replied {
            "closed_loop_replied"
        } else if row.closeout_published {
            "closeout_published"
        } else if row.forge_requested {
            "forge_requested"
        } else if row.work_shaped {
            "work_shaped"
        } else if row.assessment_published {
            "assessment_published"
        } else if row.assessed {
            "assessed"
        } else if row.acknowledged {
            "acknowledged"
        } else {
            "captured"
        }
        .to_string();
    }

    let processed: BTreeSet<String> = rows
        .values()
        .filter(|row| row.closed_loop_replied)
        .map(|row| row.feedback_ref.clone())
        .collect();
    let total_feedback = rows.len();
    let rows = rows.into_values().collect::<Vec<_>>();
    FeedbackAutonomyProjection {
        total_feedback,
        processed_count: processed.len(),
        open_count: total_feedback.saturating_sub(processed.len()),
        rows,
    }
}

fn autonomy_event_kind(event: &str) -> Option<&'static str> {
    match event {
        "acknowledged" => Some("message.feedback.acknowledged"),
        "assessed" => Some("product.feedback.assessed"),
        "assessment_published" => Some("pages.feedback.assessment_published"),
        "forge_requested" => Some("forge.work.requested"),
        "evidence_attached" => Some("forge.evidence.attached"),
        "closeout_published" => Some("pages.change.closeout_published"),
        "closed_loop_replied" => Some("message.feedback.closed_loop_replied"),
        _ => None,
    }
}

fn payload_value<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BetaFeedbackCapture, FeedbackContextEntry};

    #[test]
    fn media_feedback_uses_review_lane() {
        let (lane, reason) = classify_feedback_for_autonomy(
            "Please let me include a screenshot with the feedback",
            "idea",
            "forge",
        );
        assert_eq!(lane, "autopilot_with_review");
        assert!(reason.contains("privacy"));
    }

    #[test]
    fn telemetry_surface_is_always_held_regardless_of_note() {
        // The exploit alphabet ("copy"/"typo"/"label") in the note must NOT win on
        // the telemetry surface: a telemetry-origin capture is always held.
        let (lane, _) = classify_feedback_for_autonomy(
            "route /feedback/copy-fix.json returned status 404 in 5 sessions",
            "bug",
            "telemetry",
        );
        assert_eq!(lane, "needs_human");
        // The SAME note on a normal surface keeps its prior behavior (unchanged).
        let (other, _) = classify_feedback_for_autonomy(
            "route /feedback/copy-fix.json returned status 404 in 5 sessions",
            "bug",
            "product",
        );
        assert_eq!(other, "autopilot_allowed");
    }

    #[test]
    fn projection_closes_when_reply_receipt_exists() {
        let mut kernel = MdxKernel::boot_local();
        let feedback = kernel
            .record_beta_feedback_local(&BetaFeedbackCapture {
                surface: "twin",
                route: "/",
                tenant_id: "local_tenant",
                actor_id: "human:local_user",
                session_ref: "session",
                occurred_at: "2026-06-12T00:00:00Z",
                category: "idea",
                context: &[FeedbackContextEntry {
                    key: "feature_area",
                    value: "feedback",
                }],
                note: "please add screenshots to feedback",
            })
            .expect("feedback records");
        let identity = GovernedWriteIdentity::local_demo(FEEDBACK_AUTONOMY_ACTOR_ID);
        kernel
            .record_feedback_autonomy_event_with_identity(
                FeedbackAutonomyEvent {
                    tenant_id: "local_tenant",
                    actor_id: FEEDBACK_AUTONOMY_ACTOR_ID,
                    feedback_ref: &feedback.feedback_receipt_id,
                    event: "closed_loop_replied",
                    lane: "autopilot_with_review",
                    detail: "done",
                    page_ref: "page",
                    work_item_ref: "work",
                    forge_run_ref: "",
                    source_receipt_id: &feedback.feedback_receipt_id,
                },
                &identity,
            )
            .expect("event records");
        let projection = kernel.feedback_autonomy_projection();
        assert_eq!(projection.total_feedback, 1);
        assert_eq!(projection.processed_count, 1);
        assert_eq!(projection.open_count, 0);
        assert_eq!(projection.rows[0].lifecycle, "closed_loop_replied");
    }
}
