//! The work plane's verbs. A work item is the atom one rung below a
//! bet: a piece of work one owner can pick up. Two verbs make the plane
//! real:
//!
//! - SHAPE a work item: create it with a title, then re-shape it by
//!   work_item_id as it fills in. Every shape is a receipt; the
//!   projection folds to the latest. The owner is exactly one id, a
//!   person or an agent, and every item carries its own approval level
//!   (the autonomy dial lives on the work, not in a global setting).
//! - MOVE a work item: status changes are their own receipts against a
//!   fixed vocabulary, so the item's history reads as an activity trail
//!   and the current status is always derived, never stored. v1's
//!   lesson: a status nothing writes back is a lie waiting to be
//!   noticed - here every move is a record.
//!
//! Neither verb opens authority. Assigning an item to an agent does not
//! run anything; execution still goes through the Talent chain, scoped
//! by the item's own approval level.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

/// Where a work item can stand. Fixed on purpose: a constrained
/// vocabulary is what lets agents work the plane reliably.
pub const WORK_ITEM_STATUSES: &[&str] = &[
    "intake",
    "up_next",
    "in_motion",
    "in_review",
    "done",
    "dropped",
];

/// How much ceremony an agent owner needs before running this item.
/// Defaults to the full sign-off chain; dialed down per item as trust
/// builds.
pub const WORK_ITEM_APPROVAL_LEVELS: &[&str] =
    &["full_signoff", "plan_first", "brief_first", "auto"];

const MAX_TITLE_CHARS: usize = 300;
const MAX_FIELD_CHARS: usize = 500;
const MAX_LONG_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct WorkItemShape<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// Empty means mint a new item; a known id means re-shape it.
    pub work_item_id: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    /// The bet this work serves. Empty is legal - triaged work can
    /// exist before its bet does - but stays visibly unattached.
    pub bet_id: &'a str,
    /// Exactly one owner: a person or an agent. Empty means unowned.
    pub owner_id: &'a str,
    /// One of WORK_ITEM_APPROVAL_LEVELS; empty means full_signoff.
    pub approval: &'a str,
    /// The spec or deliverable page this item points at.
    pub page_ref: &'a str,
    /// The forge build this item is linked to.
    pub build_ref: &'a str,
}

#[derive(Clone, Copy, Debug)]
pub struct WorkItemMove<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub work_item_id: &'a str,
    pub status: &'a str,
    pub note: &'a str,
    /// What the work produced, when it is done: a page, a build, a
    /// receipt. Optional - the move receipt itself is already proof.
    pub deliverable_ref: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkPlaneWriteReport {
    pub record_id: String,
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub terminal_state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkPlaneError {
    Missing(&'static str),
    TooLong(&'static str, usize, usize),
    UnknownStatus(String),
    UnknownApproval(String),
}

impl WorkPlaneError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("work plane write missing {field}"),
            Self::TooLong(field, len, max) => {
                format!("work item {field} is {len} characters; the limit is {max}")
            }
            Self::UnknownStatus(other) => format!(
                "work item status must be one of {}; got {other}",
                WORK_ITEM_STATUSES.join(", ")
            ),
            Self::UnknownApproval(other) => format!(
                "work item approval must be one of {}; got {other}",
                WORK_ITEM_APPROVAL_LEVELS.join(", ")
            ),
        }
    }
}

fn check_len(field: &'static str, value: &str, max: usize) -> Result<(), WorkPlaneError> {
    let len = value.chars().count();
    if len > max {
        return Err(WorkPlaneError::TooLong(field, len, max));
    }
    Ok(())
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn save_work_item_local_with_identity(
        &mut self,
        request: WorkItemShape<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<WorkPlaneWriteReport, WorkPlaneError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("title", request.title),
        ] {
            if value.trim().is_empty() {
                return Err(WorkPlaneError::Missing(field));
            }
        }
        check_len("title", request.title, MAX_TITLE_CHARS)?;
        check_len("description", request.description, MAX_LONG_CHARS)?;
        for (field, value) in [
            ("bet_id", request.bet_id),
            ("owner_id", request.owner_id),
            ("page_ref", request.page_ref),
            ("build_ref", request.build_ref),
        ] {
            check_len(field, value, MAX_FIELD_CHARS)?;
        }
        let approval = if request.approval.trim().is_empty() {
            "full_signoff"
        } else {
            request.approval.trim()
        };
        if !WORK_ITEM_APPROVAL_LEVELS.contains(&approval) {
            return Err(WorkPlaneError::UnknownApproval(approval.to_string()));
        }
        let work_item_id = if request.work_item_id.trim().is_empty() {
            self.ids.next("work_item")
        } else {
            request.work_item_id.trim().to_string()
        };
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "product_shaping",
                action: ActionKind::RecordWorkItemShape,
                transition: "RECORD_WORK_ITEM_SHAPE",
                kind: "work.item.shaped",
            },
            payload(&[
                ("work_item_id", &work_item_id),
                ("title", request.title),
                ("description", request.description),
                ("bet_id", request.bet_id),
                ("owner_id", request.owner_id),
                ("approval", approval),
                ("page_ref", request.page_ref),
                ("build_ref", request.build_ref),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(WorkPlaneWriteReport {
            record_id: work_item_id,
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            terminal_state: "WORK_ITEM_SHAPED",
        })
    }

    pub fn save_work_item_move_local_with_identity(
        &mut self,
        request: WorkItemMove<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<WorkPlaneWriteReport, WorkPlaneError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("work_item_id", request.work_item_id),
            ("status", request.status),
        ] {
            if value.trim().is_empty() {
                return Err(WorkPlaneError::Missing(field));
            }
        }
        if !WORK_ITEM_STATUSES.contains(&request.status) {
            return Err(WorkPlaneError::UnknownStatus(request.status.to_string()));
        }
        check_len("note", request.note, MAX_FIELD_CHARS)?;
        check_len("deliverable_ref", request.deliverable_ref, MAX_FIELD_CHARS)?;
        let move_id = self.ids.next("work_item_move");
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "product_shaping",
                action: ActionKind::RecordWorkItemMove,
                transition: "RECORD_WORK_ITEM_MOVE",
                kind: "work.item.moved",
            },
            payload(&[
                ("move_id", &move_id),
                ("work_item_id", request.work_item_id),
                ("status", request.status),
                ("note", request.note),
                ("deliverable_ref", request.deliverable_ref),
                ("identity_source", &identity.identity_source),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(WorkPlaneWriteReport {
            record_id: move_id,
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            terminal_state: "WORK_ITEM_MOVED",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape<'a>(work_item_id: &'a str, title: &'a str) -> WorkItemShape<'a> {
        WorkItemShape {
            tenant_id: "t",
            actor_id: "human:founder",
            work_item_id,
            title,
            description: "",
            bet_id: "",
            owner_id: "",
            approval: "",
            page_ref: "",
            build_ref: "",
        }
    }

    fn item_move<'a>(work_item_id: &'a str, status: &'a str) -> WorkItemMove<'a> {
        WorkItemMove {
            tenant_id: "t",
            actor_id: "human:founder",
            work_item_id,
            status,
            note: "",
            deliverable_ref: "",
        }
    }

    #[test]
    fn shaping_a_work_item_mints_an_id_and_a_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:founder");
        let report = kernel
            .save_work_item_local_with_identity(shape("", "Draft the foo spec"), &identity)
            .expect("shape");
        assert!(report.record_id.starts_with("work_item"));
        assert_eq!(report.terminal_state, "WORK_ITEM_SHAPED");
        assert!(!report.receipt_id.is_empty());

        // Re-shaping by id reuses that id rather than minting a new one.
        let again = kernel
            .save_work_item_local_with_identity(
                shape(&report.record_id, "Draft the foo spec, v2"),
                &identity,
            )
            .expect("reshape");
        assert_eq!(again.record_id, report.record_id);
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn moving_a_work_item_records_a_move_receipt() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:founder");
        let report = kernel
            .save_work_item_move_local_with_identity(item_move("work_item_1", "up_next"), &identity)
            .expect("move");
        assert!(report.record_id.starts_with("work_item_move"));
        assert_eq!(report.terminal_state, "WORK_ITEM_MOVED");
        assert!(!report.receipt_id.is_empty());
        assert!(kernel.ledger().verify().is_ok());
    }

    #[test]
    fn shaping_refuses_missing_required_fields() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:founder");
        for (field, request) in [
            ("tenant_id", {
                let mut r = shape("", "title");
                r.tenant_id = "  ";
                r
            }),
            ("actor_id", {
                let mut r = shape("", "title");
                r.actor_id = "";
                r
            }),
            ("title", shape("", " ")),
        ] {
            let refused = kernel.save_work_item_local_with_identity(request, &identity);
            assert_eq!(refused, Err(WorkPlaneError::Missing(field)));
        }
    }

    #[test]
    fn shaping_refuses_overlong_fields() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:founder");
        let long_title = "x".repeat(MAX_TITLE_CHARS + 1);
        let long_long = "y".repeat(MAX_LONG_CHARS + 1);
        let long_field = "z".repeat(MAX_FIELD_CHARS + 1);

        let refused = kernel.save_work_item_local_with_identity(shape("", &long_title), &identity);
        assert_eq!(
            refused,
            Err(WorkPlaneError::TooLong(
                "title",
                MAX_TITLE_CHARS + 1,
                MAX_TITLE_CHARS
            ))
        );

        let mut with_long_description = shape("", "title");
        with_long_description.description = &long_long;
        let refused = kernel.save_work_item_local_with_identity(with_long_description, &identity);
        assert_eq!(
            refused,
            Err(WorkPlaneError::TooLong(
                "description",
                MAX_LONG_CHARS + 1,
                MAX_LONG_CHARS
            ))
        );

        for field in ["bet_id", "owner_id", "page_ref", "build_ref"] {
            let mut request = shape("", "title");
            match field {
                "bet_id" => request.bet_id = &long_field,
                "owner_id" => request.owner_id = &long_field,
                "page_ref" => request.page_ref = &long_field,
                "build_ref" => request.build_ref = &long_field,
                _ => unreachable!(),
            }
            let refused = kernel.save_work_item_local_with_identity(request, &identity);
            assert_eq!(
                refused,
                Err(WorkPlaneError::TooLong(
                    field,
                    MAX_FIELD_CHARS + 1,
                    MAX_FIELD_CHARS
                ))
            );
        }
    }

    #[test]
    fn shaping_refuses_an_unknown_approval_level() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:founder");
        let mut request = shape("", "title");
        request.approval = "vibes";
        let refused = kernel.save_work_item_local_with_identity(request, &identity);
        assert_eq!(
            refused,
            Err(WorkPlaneError::UnknownApproval("vibes".to_string()))
        );
    }

    #[test]
    fn moving_refuses_missing_required_fields() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:founder");
        for (field, request) in [
            ("tenant_id", {
                let mut r = item_move("work_item_1", "up_next");
                r.tenant_id = "  ";
                r
            }),
            ("actor_id", {
                let mut r = item_move("work_item_1", "up_next");
                r.actor_id = "";
                r
            }),
            ("work_item_id", item_move(" ", "up_next")),
            ("status", item_move("work_item_1", "")),
        ] {
            let refused = kernel.save_work_item_move_local_with_identity(request, &identity);
            assert_eq!(refused, Err(WorkPlaneError::Missing(field)));
        }
    }

    #[test]
    fn moving_refuses_an_unknown_status() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:founder");
        let refused = kernel.save_work_item_move_local_with_identity(
            item_move("work_item_1", "blocked"),
            &identity,
        );
        assert_eq!(
            refused,
            Err(WorkPlaneError::UnknownStatus("blocked".to_string()))
        );
    }

    #[test]
    fn moving_refuses_overlong_note_and_deliverable() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:founder");
        let long_field = "z".repeat(MAX_FIELD_CHARS + 1);

        let mut with_long_note = item_move("work_item_1", "in_review");
        with_long_note.note = &long_field;
        let refused = kernel.save_work_item_move_local_with_identity(with_long_note, &identity);
        assert_eq!(
            refused,
            Err(WorkPlaneError::TooLong(
                "note",
                MAX_FIELD_CHARS + 1,
                MAX_FIELD_CHARS
            ))
        );

        let mut with_long_deliverable = item_move("work_item_1", "done");
        with_long_deliverable.deliverable_ref = &long_field;
        let refused =
            kernel.save_work_item_move_local_with_identity(with_long_deliverable, &identity);
        assert_eq!(
            refused,
            Err(WorkPlaneError::TooLong(
                "deliverable_ref",
                MAX_FIELD_CHARS + 1,
                MAX_FIELD_CHARS
            ))
        );
    }

    #[test]
    fn error_messages_name_the_offending_field_and_vocabulary() {
        assert_eq!(
            WorkPlaneError::Missing("title").message(),
            "work plane write missing title"
        );
        assert_eq!(
            WorkPlaneError::TooLong("title", 301, 300).message(),
            "work item title is 301 characters; the limit is 300"
        );
        let status = WorkPlaneError::UnknownStatus("blocked".to_string()).message();
        assert!(status.contains("blocked"));
        assert!(status.contains("intake"));
        let approval = WorkPlaneError::UnknownApproval("vibes".to_string()).message();
        assert!(approval.contains("vibes"));
        assert!(approval.contains("full_signoff"));
    }
}
