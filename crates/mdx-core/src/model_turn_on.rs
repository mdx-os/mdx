//! Approving a model turn-on: the human act that authorizes connecting a model,
//! recorded on the record before any live call. The observation receipt that
//! flips the model to connected cites this approval. The approval records WHO
//! authorized the turn-on and for WHICH provider - never a key, never a secret.
//! The live call and the observation are recorded by the server/gateway gate;
//! this is only the approval half of the governed ceremony.
use crate::*;

pub const MODEL_TURN_ON_APPROVAL_RECEIPT_KIND: &str = "install.model.turn_on.approved";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApproveModelTurnOn<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub provider_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelTurnOnApprovalReport {
    pub status: &'static str,
    pub provider_id: String,
    pub approval_receipt_id: String,
    pub policy_decision_id: String,
}

/// The connected model, derived from the accepted provider observation receipt.
/// This is the single source of truth for "a model is connected" - no side file
/// and no `make` step required; recording the observation flips it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectedModel {
    pub connected: bool,
    pub provider_id: String,
    pub model_id: String,
    pub observation_receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModelTurnOnError {
    Missing(&'static str),
}

impl ModelTurnOnError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("model turn-on approval missing {field}"),
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    /// Record the human approval to turn on a provider. Returns the approval
    /// receipt id the observation will cite. No key or secret is recorded.
    pub fn approve_model_turn_on(
        &mut self,
        request: ApproveModelTurnOn<'_>,
    ) -> Result<ModelTurnOnApprovalReport, ModelTurnOnError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("provider_id", request.provider_id),
        ] {
            if value.trim().is_empty() {
                return Err(ModelTurnOnError::Missing(field));
            }
        }
        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "model_turn_on",
                action: ActionKind::ApproveModelTurnOn,
                transition: "APPROVE_MODEL_TURN_ON",
                kind: MODEL_TURN_ON_APPROVAL_RECEIPT_KIND,
            },
            payload(&[
                ("provider_id", request.provider_id),
                ("approved_by", request.actor_id),
                ("credential_recorded", "false"),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(ModelTurnOnApprovalReport {
            status: "MODEL_TURN_ON_APPROVED",
            provider_id: request.provider_id.to_string(),
            approval_receipt_id: receipt.0,
            policy_decision_id: receipt.1,
        })
    }

    /// The connected model, derived from the latest accepted provider
    /// observation receipt (twin.model_gateway.provider.observed with
    /// provider_connected_local_allowed=true). Source of truth for the GUI.
    pub fn connected_model_local(&self) -> ConnectedModel {
        for receipt in self.storage.ledger().entries().iter().rev() {
            if receipt.kind == "twin.model_gateway.provider.observed"
                && receipt
                    .payload
                    .get("provider_connected_local_allowed")
                    .map(String::as_str)
                    == Some("true")
            {
                return ConnectedModel {
                    connected: true,
                    provider_id: receipt
                        .payload
                        .get("provider_id")
                        .cloned()
                        .unwrap_or_default(),
                    model_id: receipt.payload.get("model_id").cloned().unwrap_or_default(),
                    observation_receipt_id: receipt.receipt_id.clone(),
                };
            }
        }
        ConnectedModel {
            connected: false,
            provider_id: String::new(),
            model_id: String::new(),
            observation_receipt_id: String::new(),
        }
    }
}
