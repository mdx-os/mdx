//! Who owns THIS install of MDx - recorded once, on the record.
//!
//! A fresh install has no owner; the local session is a deterministic seat, not
//! an established identity. The FIRST claim establishes the owner and is governed
//! like any other write (policy decision + receipt). There is no password and no
//! pretend auth here: this records an owner name and the claiming actor, nothing
//! more. Real per-user sign-in (an external IdP) lands in a later slice; this
//! does not stand in for it. The claim is once-only - a second claim is refused
//! and returns the existing owner, so ownership can never be silently overwritten.
use crate::*;

pub const INSTALL_OWNER_RECEIPT_KIND: &str = "install.owner.claimed";
const MAX_OWNER_NAME_CHARS: usize = 80;

/// A request to claim ownership of this install.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimInstallOwner<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    /// The human name/handle of the owner. No password, no credential - just who.
    pub owner_name: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallOwnerReport {
    pub status: &'static str,
    pub claimed: bool,
    pub owner_name: String,
    pub owner_actor_id: String,
    pub receipt_id: String,
    pub policy_decision_id: String,
    /// True when this call was refused because an owner already existed; the
    /// report then echoes the existing owner and records no new receipt.
    pub already_claimed: bool,
}

/// The derived ownership state of the install.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallOwnerState {
    pub claimed: bool,
    pub owner_name: String,
    pub owner_actor_id: String,
    pub receipt_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallOwnerError {
    Missing(&'static str),
    TooLong(usize, usize),
}

impl InstallOwnerError {
    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("claim this install missing {field}"),
            Self::TooLong(have, max) => {
                format!("owner name is {have} characters; the maximum is {max}")
            }
        }
    }
}

impl<S: StorageProvider> MdxKernel<S> {
    /// The current ownership of the install, derived from the first claim receipt.
    pub fn install_owner_state(&self) -> InstallOwnerState {
        for receipt in self.storage.ledger().entries() {
            if receipt.kind == INSTALL_OWNER_RECEIPT_KIND {
                return InstallOwnerState {
                    claimed: true,
                    owner_name: receipt
                        .payload
                        .get("owner_name")
                        .cloned()
                        .unwrap_or_default(),
                    owner_actor_id: receipt
                        .payload
                        .get("owner_actor_id")
                        .cloned()
                        .unwrap_or_default(),
                    receipt_id: receipt.receipt_id.clone(),
                };
            }
        }
        InstallOwnerState {
            claimed: false,
            owner_name: String::new(),
            owner_actor_id: String::new(),
            receipt_id: String::new(),
        }
    }

    /// Claim ownership of this install. First claim wins; a later claim is
    /// refused and returns the existing owner, recording no second receipt.
    pub fn claim_install_owner(
        &mut self,
        request: ClaimInstallOwner<'_>,
    ) -> Result<InstallOwnerReport, InstallOwnerError> {
        let identity = GovernedWriteIdentity::local_demo(request.actor_id);
        self.claim_install_owner_with_identity(request, &identity)
    }

    /// Claim ownership with the identity already resolved by the serving
    /// boundary. Local demo callers use [`claim_install_owner`]; production and
    /// local-secure routes pass the verified identity so the receipt does not
    /// claim a fixture actor performed the write.
    pub fn claim_install_owner_with_identity(
        &mut self,
        request: ClaimInstallOwner<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<InstallOwnerReport, InstallOwnerError> {
        for (field, value) in [
            ("tenant_id", request.tenant_id),
            ("actor_id", request.actor_id),
            ("owner_name", request.owner_name),
        ] {
            if value.trim().is_empty() {
                return Err(InstallOwnerError::Missing(field));
            }
        }
        let owner_name = request.owner_name.trim();
        if owner_name.chars().count() > MAX_OWNER_NAME_CHARS {
            return Err(InstallOwnerError::TooLong(
                owner_name.chars().count(),
                MAX_OWNER_NAME_CHARS,
            ));
        }

        // First claim wins. A second claim is refused, never overwritten.
        let existing = self.install_owner_state();
        if existing.claimed {
            return Ok(InstallOwnerReport {
                status: "INSTALL_OWNER_ALREADY_CLAIMED",
                claimed: true,
                owner_name: existing.owner_name,
                owner_actor_id: existing.owner_actor_id,
                receipt_id: existing.receipt_id,
                policy_decision_id: String::new(),
                already_claimed: true,
            });
        }

        let receipt = self.record_board_write(
            BoardWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                loop_id: "install_owner",
                action: ActionKind::ClaimInstallOwner,
                transition: "CLAIM_INSTALL_OWNER",
                kind: INSTALL_OWNER_RECEIPT_KIND,
            },
            payload(&[
                ("owner_name", owner_name),
                ("owner_actor_id", request.actor_id),
                ("identity_source", &identity.identity_source),
                ("actor_kind", &identity.actor_kind),
                ("subject_actor_id", &identity.subject_actor_id),
                ("delegation_id", &identity.delegation_id),
                ("auth_method", "claim_no_password"),
                ("authority_opened", "none"),
                ("production_write_allowed", "false"),
            ]),
        );
        Ok(InstallOwnerReport {
            status: "INSTALL_OWNER_CLAIMED",
            claimed: true,
            owner_name: owner_name.to_string(),
            owner_actor_id: request.actor_id.to_string(),
            receipt_id: receipt.0,
            policy_decision_id: receipt.1,
            already_claimed: false,
        })
    }
}
