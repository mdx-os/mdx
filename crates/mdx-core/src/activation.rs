//! Activation receipts for the day-1 MDx welcome spine.
//!
//! These records are deliberately small and proof-shaped. They let onboarding
//! prove that the operator personalized Twin, connected usable model lanes,
//! prepared Forge, seeded a starter workspace, and saw the first cross-surface
//! proof without granting runtime authority or recording secret values.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};

pub const ACTIVATION_PROFILE_SAVED: &str = "activation.profile.saved";
pub const ACTIVATION_MODEL_SETUP_RECORDED: &str = "activation.model_setup.recorded";
pub const ACTIVATION_FORGE_WORKSPACE_SAVED: &str = "activation.forge_workspace.saved";
pub const ACTIVATION_STARTER_WORKSPACE_SEEDED: &str = "activation.starter_workspace.seeded";
pub const ACTIVATION_FIRST_PROOF_RECORDED: &str = "activation.first_proof.recorded";
pub const ACTIVATION_FIRST_MISSION_STARTED: &str = "activation.first_mission.started";
pub const ACTIVATION_FIRST_MISSION_BRIEF_SHAPED: &str = "activation.first_mission.brief_shaped";
pub const ACTIVATION_FIRST_MISSION_RUN_ADMITTED: &str = "activation.first_mission.run_admitted";
pub const ACTIVATION_FIRST_MISSION_COMPLETED: &str = "activation.first_mission.completed";
pub const ACTIVATION_FIRST_MISSION_RESULT_RECORDED: &str =
    "activation.first_mission.result_recorded";

pub const ACTIVATION_EVENT_KINDS: &[&str] = &[
    ACTIVATION_PROFILE_SAVED,
    ACTIVATION_MODEL_SETUP_RECORDED,
    ACTIVATION_FORGE_WORKSPACE_SAVED,
    ACTIVATION_STARTER_WORKSPACE_SEEDED,
    ACTIVATION_FIRST_PROOF_RECORDED,
    ACTIVATION_FIRST_MISSION_STARTED,
    ACTIVATION_FIRST_MISSION_BRIEF_SHAPED,
    ACTIVATION_FIRST_MISSION_RUN_ADMITTED,
    ACTIVATION_FIRST_MISSION_COMPLETED,
    ACTIVATION_FIRST_MISSION_RESULT_RECORDED,
];

const MAX_FIELD_CHARS: usize = 1000;
const MAX_DETAIL_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug)]
pub struct ActivationEvent<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub kind: &'a str,
    pub step: &'a str,
    pub surface: &'a str,
    pub detail: &'a str,
    pub starter_workspace_id: &'a str,
    pub activation_seed: bool,
    pub fields: &'a [(&'a str, &'a str)],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActivationEventReport {
    pub status: &'static str,
    pub step: String,
    pub receipt_id: String,
    pub policy_decision_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActivationEventError {
    Missing(&'static str),
    UnknownKind(String),
    InvalidField(String),
    TooLong(&'static str, usize, usize),
}

impl ActivationEventError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("activation event missing {field}"),
            Self::UnknownKind(kind) => format!(
                "activation event kind must be one of {}; got {kind}",
                ACTIVATION_EVENT_KINDS.join(", ")
            ),
            Self::InvalidField(field) => format!("activation field is not allowed: {field}"),
            Self::TooLong(field, len, max) => {
                format!("activation {field} is {len} characters; the limit is {max}")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn record_activation_event(
        &mut self,
        request: ActivationEvent<'_>,
    ) -> Result<ActivationEventReport, ActivationEventError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.record_activation_event_with_identity(request, &identity)
    }

    pub fn record_activation_event_with_identity(
        &mut self,
        request: ActivationEvent<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<ActivationEventReport, ActivationEventError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("kind", request.kind),
            ("step", request.step),
            ("surface", request.surface),
        ] {
            if value.trim().is_empty() {
                return Err(ActivationEventError::Missing(field));
            }
        }
        if !ACTIVATION_EVENT_KINDS.contains(&request.kind) {
            return Err(ActivationEventError::UnknownKind(request.kind.to_string()));
        }
        let detail_len = request.detail.chars().count();
        if detail_len > MAX_DETAIL_CHARS {
            return Err(ActivationEventError::TooLong(
                "detail",
                detail_len,
                MAX_DETAIL_CHARS,
            ));
        }
        for (key, value) in request.fields {
            if reserved_field(key) || !valid_field_key(key) {
                return Err(ActivationEventError::InvalidField((*key).to_string()));
            }
            let len = value.chars().count();
            if len > MAX_FIELD_CHARS {
                return Err(ActivationEventError::TooLong("field", len, MAX_FIELD_CHARS));
            }
        }
        let activation_event_id = format!(
            "activation_event_{}",
            self.ledger()
                .entries()
                .iter()
                .filter(|receipt| ACTIVATION_EVENT_KINDS.contains(&receipt.kind.as_str()))
                .count()
                + 1
        );
        let activation_seed = if request.activation_seed {
            "true"
        } else {
            "false"
        };
        let mut fields = vec![
            ("activation_event_id", activation_event_id.as_str()),
            ("activation_step", request.step.trim()),
            ("surface", request.surface.trim()),
            ("detail", request.detail.trim()),
            ("starter_workspace_id", request.starter_workspace_id.trim()),
            ("activation_seed", activation_seed),
            ("identity_source", &identity.identity_source),
            ("secret_values_recorded", "false"),
            ("output_text_recorded", "false"),
            ("authority_opened", "none"),
            ("adaptation_allowed", "false"),
            ("production_write_allowed", "false"),
        ];
        for (key, value) in request.fields {
            fields.push((key, value));
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "activation_onboarding",
                action: ActionKind::RecordActivationEvent,
                transition: "RECORD_ACTIVATION_EVENT",
                kind: request.kind,
            },
            payload(&fields),
        );
        Ok(ActivationEventReport {
            status: "ACTIVATION_EVENT_RECORDED",
            step: request.step.trim().to_string(),
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
        })
    }
}

fn valid_field_key(key: &str) -> bool {
    !key.trim().is_empty()
        && key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn reserved_field(key: &str) -> bool {
    matches!(
        key,
        "activation_event_id"
            | "activation_step"
            | "surface"
            | "detail"
            | "activation_seed"
            | "identity_source"
            | "secret_values_recorded"
            | "output_text_recorded"
            | "authority_opened"
            | "adaptation_allowed"
            | "production_write_allowed"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_activation_without_secret_or_behavior_claims() {
        let mut kernel = MdxKernel::boot_local();
        let report = kernel
            .record_activation_event(ActivationEvent {
                tenant_id: "local_tenant",
                actor_id: "human:local_user",
                kind: ACTIVATION_PROFILE_SAVED,
                step: "profile",
                surface: "twin",
                detail: "Saved operator profile for Twin.",
                starter_workspace_id: "",
                activation_seed: false,
                fields: &[("profile_name", "Dana")],
            })
            .expect("activation receipt");
        let receipt = kernel
            .ledger()
            .query()
            .by_id(&report.receipt_id)
            .expect("receipt");
        assert_eq!(receipt.kind, ACTIVATION_PROFILE_SAVED);
        assert_eq!(
            receipt
                .payload
                .get("secret_values_recorded")
                .map(String::as_str),
            Some("false")
        );
        assert_eq!(
            receipt
                .payload
                .get("adaptation_allowed")
                .map(String::as_str),
            Some("false")
        );
    }
}
