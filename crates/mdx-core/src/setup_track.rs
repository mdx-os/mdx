//! The activation track a fresh install chose: try, build, or prepare.
//!
//! After an install is claimed and a model is connected, the operator is asked
//! the one thing MDx cannot detect - what they came to do. Their answer is the
//! first intent signal and is recorded on the record like any other governed
//! write. It is NOT auth and NOT a role grant: it routes the one-time activation
//! chooser and seeds later "You" context, nothing more. The choice is
//! revisitable, so the latest receipt wins (re-choosing records a new receipt).
//! The front door derives "has a track been chosen?" from this receipt, so the
//! one-time router shows exactly once without any client flag to drift.
use crate::*;

pub const SETUP_TRACK_CHOSEN_RECEIPT_KIND: &str = "setup.track_chosen";
pub const SETUP_TRACK_REFUSED_RECEIPT_KIND: &str = "setup.track_choice.refused";

/// The three honest tracks. "try" = evaluate MDx, "build" = work in it locally,
/// "prepare" = the production-readiness track. Anything else is refused.
pub const SETUP_TRACKS: [&str; 3] = ["try", "build", "prepare"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChooseSetupTrack<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub track: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupTrackReport {
    pub status: &'static str,
    pub track: String,
    pub receipt_id: String,
    pub policy_decision_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefusedSetupTrackReport {
    pub status: &'static str,
    pub track: String,
    pub receipt_id: String,
    pub policy_decision_id: String,
}

/// The chosen track, derived from the latest track-chosen receipt. `chosen` is
/// false on a fresh install - which is exactly what makes the router one-time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChosenSetupTrack {
    pub chosen: bool,
    pub track: String,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetupTrackError {
    Missing(&'static str),
    UnknownTrack(String),
}

impl SetupTrackError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("choose a setup track missing {field}"),
            Self::UnknownTrack(track) => {
                format!("'{track}' is not a setup track MDx offers")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    /// The track chosen for this install, from the latest track-chosen receipt.
    pub fn chosen_setup_track_local(&self) -> ChosenSetupTrack {
        for receipt in self.storage.ledger().entries().iter().rev() {
            if receipt.kind == SETUP_TRACK_CHOSEN_RECEIPT_KIND {
                return ChosenSetupTrack {
                    chosen: true,
                    track: receipt.payload.get("track").cloned().unwrap_or_default(),
                    receipt_id: receipt.receipt_id.clone(),
                };
            }
        }
        ChosenSetupTrack {
            chosen: false,
            track: String::new(),
            receipt_id: String::new(),
        }
    }

    /// Record the activation track the operator chose. Latest wins, so the
    /// choice can be revisited from settings without overwriting history.
    pub fn choose_setup_track(
        &mut self,
        request: ChooseSetupTrack<'_>,
    ) -> Result<SetupTrackReport, SetupTrackError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("track", request.track),
        ] {
            if value.trim().is_empty() {
                return Err(SetupTrackError::Missing(field));
            }
        }
        let track = request.track.trim();
        if !SETUP_TRACKS.contains(&track) {
            return Err(SetupTrackError::UnknownTrack(track.to_string()));
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "setup_track",
                action: ActionKind::ChooseSetupTrack,
                transition: "CHOOSE_SETUP_TRACK",
                kind: SETUP_TRACK_CHOSEN_RECEIPT_KIND,
            },
            payload(&[
                ("track", track),
                ("chosen_by", request.actor_id),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(SetupTrackReport {
            status: "SETUP_TRACK_CHOSEN",
            track: track.to_string(),
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
        })
    }

    pub fn refuse_setup_track_pre_activation(
        &mut self,
        request: ChooseSetupTrack<'_>,
        reason: &'static str,
        owner_claimed: bool,
        model_connected: bool,
    ) -> Result<RefusedSetupTrackReport, SetupTrackError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("track", request.track),
        ] {
            if value.trim().is_empty() {
                return Err(SetupTrackError::Missing(field));
            }
        }
        let track = request.track.trim();
        if !SETUP_TRACKS.contains(&track) {
            return Err(SetupTrackError::UnknownTrack(track.to_string()));
        }
        let owner_claimed = if owner_claimed { "true" } else { "false" };
        let model_connected = if model_connected { "true" } else { "false" };
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "setup_track",
                action: ActionKind::ChooseSetupTrack,
                transition: "REFUSE_SETUP_TRACK_PRE_ACTIVATION",
                kind: SETUP_TRACK_REFUSED_RECEIPT_KIND,
            },
            payload(&[
                ("track", track),
                ("chosen_by", request.actor_id),
                ("reason", reason),
                ("owner_claimed", owner_claimed),
                ("model_connected", model_connected),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(RefusedSetupTrackReport {
            status: "SETUP_TRACK_REFUSED_PRE_ACTIVATION",
            track: track.to_string(),
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
        })
    }
}
