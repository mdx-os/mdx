//! The first-run profile: the few facts the guided "Try MDx" start collects so
//! the very first Twin conversation is about the operator's actual work.
//!
//! This is NOT account creation and NOT auth - it records role, goal, working
//! mode, and an explicit local-posture acknowledgement on the record, nothing
//! more. The rich personalization (voice, principles, style) stays in the "You"
//! surface; this is just enough to seed a useful first conversation and to prove
//! the guided start happened. The local-posture acknowledgement is required: MDx
//! will not record a first-run profile for someone who did not confirm they
//! understand it runs locally.
use crate::*;

pub const FIRST_RUN_PROFILE_RECEIPT_KIND: &str = "setup.first_run_profile.recorded";

/// The roles offered on the guided start - aligned with the "You" surface so a
/// chosen role seeds persona context directly.
pub const FIRST_RUN_ROLES: [&str; 4] = ["leadership", "product", "design", "engineering"];

/// How the operator wants to start. Honest, small, not a persona test.
pub const FIRST_RUN_WORKING_MODES: [&str; 3] = ["exploring", "deciding", "hands_on"];

const MAX_GOAL_CHARS: usize = 400;
const MAX_ORG_CHARS: usize = 200;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordFirstRunProfile<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub role: &'a str,
    pub goal: &'a str,
    pub org_context: &'a str,
    pub working_mode: &'a str,
    /// Must be true: the operator confirmed they understand MDx runs locally and
    /// their data stays on this machine. Without it nothing is recorded.
    pub privacy_ack: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirstRunProfileReport {
    pub status: &'static str,
    pub role: String,
    pub receipt_id: String,
    pub policy_decision_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirstRunProfileState {
    pub recorded: bool,
    pub role: String,
    pub goal: String,
    pub working_mode: String,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FirstRunProfileError {
    Missing(&'static str),
    UnknownRole(String),
    UnknownWorkingMode(String),
    PrivacyNotAcknowledged,
    TooLong(&'static str, usize, usize),
}

impl FirstRunProfileError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("guided start missing {field}"),
            Self::UnknownRole(role) => format!("'{role}' is not a role MDx offers here"),
            Self::UnknownWorkingMode(mode) => format!("'{mode}' is not a working mode MDx offers"),
            Self::PrivacyNotAcknowledged => {
                "the local-posture acknowledgement is required to continue".to_string()
            }
            Self::TooLong(field, have, max) => {
                format!("{field} is {have} characters; the maximum is {max}")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    /// The latest recorded first-run profile, or an empty state if none.
    pub fn first_run_profile_local(&self) -> FirstRunProfileState {
        for receipt in self.storage.ledger().entries().iter().rev() {
            if receipt.kind == FIRST_RUN_PROFILE_RECEIPT_KIND {
                return FirstRunProfileState {
                    recorded: true,
                    role: receipt.payload.get("role").cloned().unwrap_or_default(),
                    goal: receipt.payload.get("goal").cloned().unwrap_or_default(),
                    working_mode: receipt
                        .payload
                        .get("working_mode")
                        .cloned()
                        .unwrap_or_default(),
                    receipt_id: receipt.receipt_id.clone(),
                };
            }
        }
        FirstRunProfileState {
            recorded: false,
            role: String::new(),
            goal: String::new(),
            working_mode: String::new(),
            receipt_id: String::new(),
        }
    }

    /// Record the guided-start facts. Latest wins (the start can be redone).
    pub fn record_first_run_profile(
        &mut self,
        request: RecordFirstRunProfile<'_>,
    ) -> Result<FirstRunProfileReport, FirstRunProfileError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("role", request.role),
            ("working_mode", request.working_mode),
        ] {
            if value.trim().is_empty() {
                return Err(FirstRunProfileError::Missing(field));
            }
        }
        if !request.privacy_ack {
            return Err(FirstRunProfileError::PrivacyNotAcknowledged);
        }
        let role = request.role.trim();
        if !FIRST_RUN_ROLES.contains(&role) {
            return Err(FirstRunProfileError::UnknownRole(role.to_string()));
        }
        let working_mode = request.working_mode.trim();
        if !FIRST_RUN_WORKING_MODES.contains(&working_mode) {
            return Err(FirstRunProfileError::UnknownWorkingMode(
                working_mode.to_string(),
            ));
        }
        let goal = request.goal.trim();
        if goal.chars().count() > MAX_GOAL_CHARS {
            return Err(FirstRunProfileError::TooLong(
                "goal",
                goal.chars().count(),
                MAX_GOAL_CHARS,
            ));
        }
        let org_context = request.org_context.trim();
        if org_context.chars().count() > MAX_ORG_CHARS {
            return Err(FirstRunProfileError::TooLong(
                "org_context",
                org_context.chars().count(),
                MAX_ORG_CHARS,
            ));
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "first_run_profile",
                action: ActionKind::RecordFirstRunProfile,
                transition: "RECORD_FIRST_RUN_PROFILE",
                kind: FIRST_RUN_PROFILE_RECEIPT_KIND,
            },
            payload(&[
                ("role", role),
                ("goal", goal),
                ("org_context", org_context),
                ("working_mode", working_mode),
                ("privacy_acknowledged", "true"),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(FirstRunProfileReport {
            status: "FIRST_RUN_PROFILE_RECORDED",
            role: role.to_string(),
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
        })
    }
}
