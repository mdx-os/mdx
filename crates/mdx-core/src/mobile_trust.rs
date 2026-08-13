//! Receipt-backed mobile device, host, pairing, and revocation state.
//!
//! Trust state is projected only from the kernel ledger. The serving snapshot
//! and Postgres ledger restore paths therefore preserve revocation across a
//! process restart without a second mutable trust store.

use crate::strategy_board::BoardWrite;
use crate::{ActionKind, GovernedWriteIdentity, MdxKernel, StorageProvider, payload};
use std::collections::BTreeMap;

pub const MOBILE_DEVICE_REGISTERED_KIND: &str = "mobile.device.registered";
pub const MOBILE_DEVICE_ATTESTATION_VERIFIED_KIND: &str = "mobile.device.attestation.verified";
pub const MOBILE_HOST_REGISTERED_KIND: &str = "mobile.host.registered";
pub const MOBILE_PAIRING_RECORDED_KIND: &str = "mobile.pairing.recorded";
pub const MOBILE_DEVICE_REVOKED_KIND: &str = "mobile.device.revoked";
pub const MOBILE_HOST_REVOKED_KIND: &str = "mobile.host.revoked";
pub const MOBILE_PUSH_REGISTRATION_RECORDED_KIND: &str = "mobile.push.registration.recorded";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MobileDeviceRegistration<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub device_id: &'a str,
    pub platform: &'a str,
    pub display_name: &'a str,
    pub public_key: &'a str,
    pub public_key_thumbprint: &'a str,
    pub registered_at_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MobileHostRegistration<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub host_id: &'a str,
    pub display_name: &'a str,
    pub public_key_thumbprint: &'a str,
    pub registered_at_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MobileDeviceAttestation<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub device_id: &'a str,
    pub key_id_fingerprint: &'a str,
    pub public_key: &'a str,
    pub environment: &'a str,
    pub validation_category: u32,
    pub bundle_version: &'a str,
    pub receipt_fingerprint: &'a str,
    pub attested_at_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MobilePairingRegistration<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub device_id: &'a str,
    pub host_id: &'a str,
    pub paired_at_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MobilePushRegistration<'a> {
    pub tenant_id: &'a str,
    pub actor_id: &'a str,
    pub registration_id: &'a str,
    pub device_id: &'a str,
    pub host_id: &'a str,
    pub kind: &'a str,
    pub environment: &'a str,
    pub token_fingerprint: &'a str,
    pub device_signature_verification: &'a str,
    pub updated_at_epoch: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MobileDeviceTrust {
    pub device_id: String,
    pub tenant_id: String,
    pub user_id: String,
    pub platform: String,
    pub display_name: String,
    pub public_key: String,
    pub public_key_thumbprint: String,
    pub attestation_risk: String,
    pub app_attest_key_id_fingerprint: Option<String>,
    pub app_attest_public_key: Option<String>,
    pub app_attest_environment: Option<String>,
    pub app_attest_validation_category: Option<u32>,
    pub app_attest_bundle_version: Option<String>,
    pub app_attest_receipt_fingerprint: Option<String>,
    pub attested_at_epoch: Option<u64>,
    pub attestation_receipt_id: Option<String>,
    pub status: String,
    pub registered_at_epoch: u64,
    pub revoked_at_epoch: Option<u64>,
    pub receipt_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MobileHostTrust {
    pub host_id: String,
    pub tenant_id: String,
    pub user_id: String,
    pub display_name: String,
    pub public_key_thumbprint: String,
    pub status: String,
    pub registered_at_epoch: u64,
    pub revoked_at_epoch: Option<u64>,
    pub receipt_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MobilePairingTrust {
    pub pairing_id: String,
    pub tenant_id: String,
    pub user_id: String,
    pub device_id: String,
    pub host_id: String,
    pub paired_at_epoch: u64,
    pub status: String,
    pub receipt_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MobilePushRegistrationTrust {
    pub registration_id: String,
    pub tenant_id: String,
    pub user_id: String,
    pub device_id: String,
    pub host_id: String,
    pub kind: String,
    pub environment: String,
    pub token_fingerprint: String,
    pub device_signature_verification: String,
    pub updated_at_epoch: u64,
    pub status: String,
    pub receipt_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MobileTrustProjection {
    pub devices: Vec<MobileDeviceTrust>,
    pub hosts: Vec<MobileHostTrust>,
    pub pairings: Vec<MobilePairingTrust>,
    pub push_registrations: Vec<MobilePushRegistrationTrust>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MobileTrustWriteReport {
    pub record_id: String,
    pub receipt_id: String,
    pub policy_decision_id: String,
    pub idempotent_replay: bool,
}

struct MobileTrustWrite<'a> {
    tenant_id: &'a str,
    actor_id: &'a str,
    identity: &'a GovernedWriteIdentity,
    kind: &'a str,
    transition: &'a str,
    record_id: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MobileTrustError {
    Missing(&'static str),
    Invalid(&'static str),
    IdentityConflict(&'static str),
    Revoked(&'static str),
    Unknown(&'static str),
    Stale(&'static str),
    ActivePairingRequired,
}

impl MobileTrustError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Missing(field) => field,
            Self::Invalid("kind") => "mobile_push_kind_invalid",
            Self::Invalid("environment") => "mobile_push_environment_invalid",
            Self::Invalid("device_signature_verification") => {
                "mobile_push_device_signature_unverified"
            }
            Self::Invalid("attestation_environment") => "mobile_app_attest_environment_invalid",
            Self::Invalid("attestation_public_key") => "mobile_app_attest_public_key_invalid",
            Self::Invalid("bundle_version") => "mobile_app_attest_bundle_version_invalid",
            Self::Invalid("key_id_fingerprint") => "mobile_app_attest_key_fingerprint_invalid",
            Self::Invalid("receipt_fingerprint") => "mobile_app_attest_receipt_fingerprint_invalid",
            Self::Invalid(_) => "mobile_push_token_fingerprint_invalid",
            Self::IdentityConflict("device") => "mobile_device_identity_conflict",
            Self::IdentityConflict("attestation") => "mobile_app_attest_key_identity_conflict",
            Self::IdentityConflict(_) => "mobile_host_identity_conflict",
            Self::Revoked("device") => "mobile_device_revoked",
            Self::Revoked(_) => "mobile_host_revoked",
            Self::Unknown("device") => "mobile_device_unknown",
            Self::Unknown(_) => "mobile_host_unknown",
            Self::Stale(_) => "mobile_push_registration_stale",
            Self::ActivePairingRequired => "mobile_active_pairing_required",
        }
    }
}

fn epoch(value: Option<&String>) -> u64 {
    value
        .and_then(|value| value.parse().ok())
        .unwrap_or_default()
}

impl<S: StorageProvider> MdxKernel<S> {
    pub fn mobile_trust_projection(&self, tenant_id: &str, user_id: &str) -> MobileTrustProjection {
        let mut devices = BTreeMap::<String, MobileDeviceTrust>::new();
        let mut hosts = BTreeMap::<String, MobileHostTrust>::new();
        let mut pairings = BTreeMap::<String, MobilePairingTrust>::new();
        let mut push_registrations = BTreeMap::<String, MobilePushRegistrationTrust>::new();
        for receipt in self.ledger().entries() {
            if receipt.tenant_id.as_str() != tenant_id || receipt.actor_id.as_str() != user_id {
                continue;
            }
            let get = |key: &str| receipt.payload.get(key).cloned().unwrap_or_default();
            match receipt.kind.as_str() {
                MOBILE_DEVICE_REGISTERED_KIND => {
                    let id = get("device_id");
                    devices.entry(id.clone()).or_insert(MobileDeviceTrust {
                        device_id: id,
                        tenant_id: tenant_id.to_string(),
                        user_id: user_id.to_string(),
                        platform: get("platform"),
                        display_name: get("display_name"),
                        public_key: get("public_key"),
                        public_key_thumbprint: get("public_key_thumbprint"),
                        attestation_risk: get("attestation_risk"),
                        app_attest_key_id_fingerprint: None,
                        app_attest_public_key: None,
                        app_attest_environment: None,
                        app_attest_validation_category: None,
                        app_attest_bundle_version: None,
                        app_attest_receipt_fingerprint: None,
                        attested_at_epoch: None,
                        attestation_receipt_id: None,
                        status: "active".to_string(),
                        registered_at_epoch: epoch(receipt.payload.get("registered_at_epoch")),
                        revoked_at_epoch: None,
                        receipt_id: receipt.receipt_id.clone(),
                    });
                }
                MOBILE_DEVICE_ATTESTATION_VERIFIED_KIND => {
                    let id = get("device_id");
                    if let Some(device) = devices.get_mut(&id)
                        && device.status == "active"
                    {
                        device.attestation_risk = "verified_app_attest".to_string();
                        device.app_attest_key_id_fingerprint = Some(get("key_id_fingerprint"));
                        device.app_attest_public_key = Some(get("public_key"));
                        device.app_attest_environment = Some(get("environment"));
                        device.app_attest_validation_category = receipt
                            .payload
                            .get("validation_category")
                            .and_then(|value| value.parse().ok());
                        device.app_attest_bundle_version = Some(get("bundle_version"));
                        device.app_attest_receipt_fingerprint = Some(get("receipt_fingerprint"));
                        device.attested_at_epoch =
                            Some(epoch(receipt.payload.get("attested_at_epoch")));
                        device.attestation_receipt_id = Some(receipt.receipt_id.clone());
                    }
                }
                MOBILE_HOST_REGISTERED_KIND => {
                    let id = get("host_id");
                    hosts.entry(id.clone()).or_insert(MobileHostTrust {
                        host_id: id,
                        tenant_id: tenant_id.to_string(),
                        user_id: user_id.to_string(),
                        display_name: get("display_name"),
                        public_key_thumbprint: get("public_key_thumbprint"),
                        status: "active".to_string(),
                        registered_at_epoch: epoch(receipt.payload.get("registered_at_epoch")),
                        revoked_at_epoch: None,
                        receipt_id: receipt.receipt_id.clone(),
                    });
                }
                MOBILE_PAIRING_RECORDED_KIND => {
                    let id = get("pairing_id");
                    pairings.entry(id.clone()).or_insert(MobilePairingTrust {
                        pairing_id: id,
                        tenant_id: tenant_id.to_string(),
                        user_id: user_id.to_string(),
                        device_id: get("device_id"),
                        host_id: get("host_id"),
                        paired_at_epoch: epoch(receipt.payload.get("paired_at_epoch")),
                        status: "active".to_string(),
                        receipt_id: receipt.receipt_id.clone(),
                    });
                }
                MOBILE_PUSH_REGISTRATION_RECORDED_KIND => {
                    let id = get("registration_id");
                    push_registrations.insert(
                        id.clone(),
                        MobilePushRegistrationTrust {
                            registration_id: id,
                            tenant_id: tenant_id.to_string(),
                            user_id: user_id.to_string(),
                            device_id: get("device_id"),
                            host_id: get("host_id"),
                            kind: get("push_kind"),
                            environment: get("environment"),
                            token_fingerprint: get("token_fingerprint"),
                            device_signature_verification: get("device_signature_verification"),
                            updated_at_epoch: epoch(receipt.payload.get("updated_at_epoch")),
                            status: "active".to_string(),
                            receipt_id: receipt.receipt_id.clone(),
                        },
                    );
                }
                MOBILE_DEVICE_REVOKED_KIND => {
                    let id = get("device_id");
                    if let Some(device) = devices.get_mut(&id) {
                        device.status = "revoked".to_string();
                        device.revoked_at_epoch =
                            Some(epoch(receipt.payload.get("revoked_at_epoch")));
                    }
                    for pairing in pairings
                        .values_mut()
                        .filter(|pairing| pairing.device_id == id)
                    {
                        pairing.status = "revoked".to_string();
                    }
                    for registration in push_registrations
                        .values_mut()
                        .filter(|registration| registration.device_id == id)
                    {
                        registration.status = "revoked".to_string();
                    }
                }
                MOBILE_HOST_REVOKED_KIND => {
                    let id = get("host_id");
                    if let Some(host) = hosts.get_mut(&id) {
                        host.status = "revoked".to_string();
                        host.revoked_at_epoch =
                            Some(epoch(receipt.payload.get("revoked_at_epoch")));
                    }
                    for pairing in pairings
                        .values_mut()
                        .filter(|pairing| pairing.host_id == id)
                    {
                        pairing.status = "revoked".to_string();
                    }
                    for registration in push_registrations
                        .values_mut()
                        .filter(|registration| registration.host_id == id)
                    {
                        registration.status = "revoked".to_string();
                    }
                }
                _ => {}
            }
        }
        MobileTrustProjection {
            devices: devices.into_values().collect(),
            hosts: hosts.into_values().collect(),
            pairings: pairings.into_values().collect(),
            push_registrations: push_registrations.into_values().collect(),
        }
    }

    pub fn register_mobile_device(
        &mut self,
        request: MobileDeviceRegistration<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MobileTrustWriteReport, MobileTrustError> {
        let state = self.mobile_trust_projection(request.tenant_id, request.actor_id);
        if let Some(existing) = state
            .devices
            .iter()
            .find(|item| item.device_id == request.device_id)
        {
            if existing.status == "revoked" {
                return Err(MobileTrustError::Revoked("device"));
            }
            if existing.public_key != request.public_key {
                return Err(MobileTrustError::IdentityConflict("device"));
            }
            return Ok(MobileTrustWriteReport {
                record_id: request.device_id.to_string(),
                receipt_id: existing.receipt_id.clone(),
                policy_decision_id: String::new(),
                idempotent_replay: true,
            });
        }
        self.mobile_write(
            MobileTrustWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                identity,
                kind: MOBILE_DEVICE_REGISTERED_KIND,
                transition: "REGISTER_MOBILE_DEVICE",
                record_id: request.device_id,
            },
            payload(&[
                ("device_id", request.device_id),
                ("platform", request.platform),
                ("display_name", request.display_name),
                ("public_key", request.public_key),
                ("public_key_thumbprint", request.public_key_thumbprint),
                ("attestation_risk", "unverified"),
                (
                    "registered_at_epoch",
                    &request.registered_at_epoch.to_string(),
                ),
            ]),
        )
    }

    pub fn register_mobile_host(
        &mut self,
        request: MobileHostRegistration<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MobileTrustWriteReport, MobileTrustError> {
        let state = self.mobile_trust_projection(request.tenant_id, request.actor_id);
        if let Some(existing) = state
            .hosts
            .iter()
            .find(|item| item.host_id == request.host_id)
        {
            if existing.status == "revoked" {
                return Err(MobileTrustError::Revoked("host"));
            }
            if existing.public_key_thumbprint != request.public_key_thumbprint {
                return Err(MobileTrustError::IdentityConflict("host"));
            }
            return Ok(MobileTrustWriteReport {
                record_id: request.host_id.to_string(),
                receipt_id: existing.receipt_id.clone(),
                policy_decision_id: String::new(),
                idempotent_replay: true,
            });
        }
        self.mobile_write(
            MobileTrustWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                identity,
                kind: MOBILE_HOST_REGISTERED_KIND,
                transition: "REGISTER_MOBILE_HOST",
                record_id: request.host_id,
            },
            payload(&[
                ("host_id", request.host_id),
                ("display_name", request.display_name),
                ("public_key_thumbprint", request.public_key_thumbprint),
                (
                    "registered_at_epoch",
                    &request.registered_at_epoch.to_string(),
                ),
            ]),
        )
    }

    pub fn record_mobile_device_attestation(
        &mut self,
        request: MobileDeviceAttestation<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MobileTrustWriteReport, MobileTrustError> {
        for (field, value) in [
            ("device_id", request.device_id),
            ("key_id_fingerprint", request.key_id_fingerprint),
            ("attestation_public_key", request.public_key),
            ("attestation_environment", request.environment),
            ("bundle_version", request.bundle_version),
            ("receipt_fingerprint", request.receipt_fingerprint),
        ] {
            if value.trim().is_empty() {
                return Err(MobileTrustError::Missing(field));
            }
        }
        for (field, value) in [
            ("key_id_fingerprint", request.key_id_fingerprint),
            ("receipt_fingerprint", request.receipt_fingerprint),
        ] {
            if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err(MobileTrustError::Invalid(field));
            }
        }
        if request.public_key.len() != 88 {
            return Err(MobileTrustError::Invalid("attestation_public_key"));
        }
        if !matches!(request.environment, "development" | "production") {
            return Err(MobileTrustError::Invalid("attestation_environment"));
        }
        if request.bundle_version.len() > 64 {
            return Err(MobileTrustError::Invalid("bundle_version"));
        }
        let state = self.mobile_trust_projection(request.tenant_id, request.actor_id);
        let device = state
            .devices
            .iter()
            .find(|item| item.device_id == request.device_id)
            .ok_or(MobileTrustError::Unknown("device"))?;
        if device.status != "active" {
            return Err(MobileTrustError::Revoked("device"));
        }
        if let Some(existing_fingerprint) = &device.app_attest_key_id_fingerprint
            && existing_fingerprint == request.key_id_fingerprint
        {
            return Ok(MobileTrustWriteReport {
                record_id: request.device_id.to_string(),
                receipt_id: device
                    .attestation_receipt_id
                    .clone()
                    .unwrap_or_else(|| device.receipt_id.clone()),
                policy_decision_id: String::new(),
                idempotent_replay: true,
            });
        }
        let key_already_bound = self.ledger().entries().iter().any(|receipt| {
            receipt.kind == MOBILE_DEVICE_ATTESTATION_VERIFIED_KIND
                && receipt
                    .payload
                    .get("key_id_fingerprint")
                    .map(String::as_str)
                    == Some(request.key_id_fingerprint)
                && (receipt.tenant_id.as_str() != request.tenant_id
                    || receipt.actor_id.as_str() != request.actor_id
                    || receipt.payload.get("device_id").map(String::as_str)
                        != Some(request.device_id))
        });
        if key_already_bound {
            return Err(MobileTrustError::IdentityConflict("attestation"));
        }
        self.mobile_write(
            MobileTrustWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                identity,
                kind: MOBILE_DEVICE_ATTESTATION_VERIFIED_KIND,
                transition: "VERIFY_MOBILE_DEVICE_APP_ATTESTATION",
                record_id: request.device_id,
            },
            payload(&[
                ("device_id", request.device_id),
                ("key_id_fingerprint", request.key_id_fingerprint),
                ("public_key", request.public_key),
                ("environment", request.environment),
                (
                    "validation_category",
                    &request.validation_category.to_string(),
                ),
                ("bundle_version", request.bundle_version),
                ("receipt_fingerprint", request.receipt_fingerprint),
                ("attested_at_epoch", &request.attested_at_epoch.to_string()),
                ("verification", "apple_app_attest_server_cryptographic"),
                ("raw_attestation_recorded", "false"),
                ("raw_receipt_recorded", "false"),
                ("authority_opened", "none"),
            ]),
        )
    }

    pub fn record_mobile_pairing(
        &mut self,
        request: MobilePairingRegistration<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MobileTrustWriteReport, MobileTrustError> {
        let state = self.mobile_trust_projection(request.tenant_id, request.actor_id);
        let device = state
            .devices
            .iter()
            .find(|item| item.device_id == request.device_id)
            .ok_or(MobileTrustError::Unknown("device"))?;
        let host = state
            .hosts
            .iter()
            .find(|item| item.host_id == request.host_id)
            .ok_or(MobileTrustError::Unknown("host"))?;
        if device.status != "active" {
            return Err(MobileTrustError::Revoked("device"));
        }
        if host.status != "active" {
            return Err(MobileTrustError::Revoked("host"));
        }
        if let Some(existing) = state.pairings.iter().find(|item| {
            item.device_id == request.device_id
                && item.host_id == request.host_id
                && item.status == "active"
        }) {
            return Ok(MobileTrustWriteReport {
                record_id: existing.pairing_id.clone(),
                receipt_id: existing.receipt_id.clone(),
                policy_decision_id: String::new(),
                idempotent_replay: true,
            });
        }
        let pairing_id = self.mint_id("mobile_pairing");
        self.mobile_write(
            MobileTrustWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                identity,
                kind: MOBILE_PAIRING_RECORDED_KIND,
                transition: "RECORD_MOBILE_PAIRING",
                record_id: &pairing_id,
            },
            payload(&[
                ("pairing_id", &pairing_id),
                ("device_id", request.device_id),
                ("host_id", request.host_id),
                ("paired_at_epoch", &request.paired_at_epoch.to_string()),
                ("device_possession", "server_verified_signature"),
            ]),
        )
    }

    pub fn record_mobile_push_registration(
        &mut self,
        request: MobilePushRegistration<'_>,
        identity: &GovernedWriteIdentity,
    ) -> Result<MobileTrustWriteReport, MobileTrustError> {
        for (field, value) in [
            ("registration_id", request.registration_id),
            ("device_id", request.device_id),
            ("host_id", request.host_id),
            ("kind", request.kind),
            ("environment", request.environment),
            ("token_fingerprint", request.token_fingerprint),
        ] {
            if value.trim().is_empty() {
                return Err(MobileTrustError::Missing(field));
            }
        }
        if !matches!(request.kind, "apns" | "live_activity") {
            return Err(MobileTrustError::Invalid("kind"));
        }
        if !matches!(request.environment, "sandbox" | "production") {
            return Err(MobileTrustError::Invalid("environment"));
        }
        if request.token_fingerprint.len() != 64
            || !request
                .token_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(MobileTrustError::Invalid("token_fingerprint"));
        }
        if request.device_signature_verification != "verified_registered_device_key" {
            return Err(MobileTrustError::Invalid("device_signature_verification"));
        }
        let state = self.mobile_trust_projection(request.tenant_id, request.actor_id);
        let device = state
            .devices
            .iter()
            .find(|item| item.device_id == request.device_id)
            .ok_or(MobileTrustError::Unknown("device"))?;
        if device.status != "active" {
            return Err(MobileTrustError::Revoked("device"));
        }
        if request.host_id != "mdx_cloud" {
            self.require_mobile_active_pairing(
                request.tenant_id,
                request.actor_id,
                request.device_id,
                request.host_id,
            )?;
        }
        if let Some(existing) = state
            .push_registrations
            .iter()
            .find(|item| item.registration_id == request.registration_id)
        {
            if existing.status == "active"
                && existing.device_id == request.device_id
                && existing.host_id == request.host_id
                && existing.kind == request.kind
                && existing.environment == request.environment
                && existing.token_fingerprint == request.token_fingerprint
            {
                return Ok(MobileTrustWriteReport {
                    record_id: request.registration_id.to_string(),
                    receipt_id: existing.receipt_id.clone(),
                    policy_decision_id: String::new(),
                    idempotent_replay: true,
                });
            }
            if request.updated_at_epoch <= existing.updated_at_epoch {
                return Err(MobileTrustError::Stale("push_registration"));
            }
        }
        self.mobile_write(
            MobileTrustWrite {
                tenant_id: request.tenant_id,
                actor_id: request.actor_id,
                identity,
                kind: MOBILE_PUSH_REGISTRATION_RECORDED_KIND,
                transition: "RECORD_MOBILE_PUSH_REGISTRATION",
                record_id: request.registration_id,
            },
            payload(&[
                ("registration_id", request.registration_id),
                ("device_id", request.device_id),
                ("host_id", request.host_id),
                ("push_kind", request.kind),
                ("environment", request.environment),
                ("token_fingerprint", request.token_fingerprint),
                (
                    "device_signature_verification",
                    request.device_signature_verification,
                ),
                ("updated_at_epoch", &request.updated_at_epoch.to_string()),
                ("token_value_recorded", "false"),
                ("raw_token_persisted", "false"),
                ("delivery_capability", "closed_missing_secure_token_store"),
                ("production_delivery_allowed", "false"),
            ]),
        )
    }

    pub fn revoke_mobile_device(
        &mut self,
        tenant_id: &str,
        actor_id: &str,
        device_id: &str,
        now: u64,
        identity: &GovernedWriteIdentity,
    ) -> Result<MobileTrustWriteReport, MobileTrustError> {
        let state = self.mobile_trust_projection(tenant_id, actor_id);
        let device = state
            .devices
            .iter()
            .find(|item| item.device_id == device_id)
            .ok_or(MobileTrustError::Unknown("device"))?;
        if device.status == "revoked" {
            return Ok(MobileTrustWriteReport {
                record_id: device_id.to_string(),
                receipt_id: device.receipt_id.clone(),
                policy_decision_id: String::new(),
                idempotent_replay: true,
            });
        }
        self.mobile_write(
            MobileTrustWrite {
                tenant_id,
                actor_id,
                identity,
                kind: MOBILE_DEVICE_REVOKED_KIND,
                transition: "REVOKE_MOBILE_DEVICE",
                record_id: device_id,
            },
            payload(&[
                ("device_id", device_id),
                ("revoked_at_epoch", &now.to_string()),
            ]),
        )
    }

    pub fn revoke_mobile_host(
        &mut self,
        tenant_id: &str,
        actor_id: &str,
        host_id: &str,
        now: u64,
        identity: &GovernedWriteIdentity,
    ) -> Result<MobileTrustWriteReport, MobileTrustError> {
        let state = self.mobile_trust_projection(tenant_id, actor_id);
        let host = state
            .hosts
            .iter()
            .find(|item| item.host_id == host_id)
            .ok_or(MobileTrustError::Unknown("host"))?;
        if host.status == "revoked" {
            return Ok(MobileTrustWriteReport {
                record_id: host_id.to_string(),
                receipt_id: host.receipt_id.clone(),
                policy_decision_id: String::new(),
                idempotent_replay: true,
            });
        }
        self.mobile_write(
            MobileTrustWrite {
                tenant_id,
                actor_id,
                identity,
                kind: MOBILE_HOST_REVOKED_KIND,
                transition: "REVOKE_MOBILE_HOST",
                record_id: host_id,
            },
            payload(&[("host_id", host_id), ("revoked_at_epoch", &now.to_string())]),
        )
    }

    pub fn require_mobile_active_pairing(
        &self,
        tenant_id: &str,
        user_id: &str,
        device_id: &str,
        host_id: &str,
    ) -> Result<(), MobileTrustError> {
        let state = self.mobile_trust_projection(tenant_id, user_id);
        let device = state
            .devices
            .iter()
            .find(|item| item.device_id == device_id)
            .ok_or(MobileTrustError::Unknown("device"))?;
        if device.status != "active" {
            return Err(MobileTrustError::Revoked("device"));
        }
        let host = state
            .hosts
            .iter()
            .find(|item| item.host_id == host_id)
            .ok_or(MobileTrustError::Unknown("host"))?;
        if host.status != "active" {
            return Err(MobileTrustError::Revoked("host"));
        }
        if state.pairings.iter().any(|item| {
            item.device_id == device_id && item.host_id == host_id && item.status == "active"
        }) {
            Ok(())
        } else {
            Err(MobileTrustError::ActivePairingRequired)
        }
    }

    fn mobile_write(
        &mut self,
        write: MobileTrustWrite<'_>,
        mut body: BTreeMap<String, String>,
    ) -> Result<MobileTrustWriteReport, MobileTrustError> {
        body.insert(
            "identity_source".to_string(),
            write.identity.identity_source.clone(),
        );
        body.insert("actor_kind".to_string(), write.identity.actor_kind.clone());
        body.insert(
            "subject_actor_id".to_string(),
            write.identity.subject_actor_id.clone(),
        );
        body.insert("authority_opened".to_string(), "none".to_string());
        body.insert("production_write_allowed".to_string(), "false".to_string());
        let (receipt_id, policy_decision_id) = self.record_board_write(
            BoardWrite {
                tenant_id: write.tenant_id,
                actor_id: write.actor_id,
                loop_id: "mobile_trust",
                action: ActionKind::RecordMobileTrustEvent,
                transition: write.transition,
                kind: write.kind,
            },
            body,
        );
        Ok(MobileTrustWriteReport {
            record_id: write.record_id.to_string(),
            receipt_id,
            policy_decision_id,
            idempotent_replay: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MdxKernel;

    fn identity() -> GovernedWriteIdentity {
        GovernedWriteIdentity::local_demo("founder")
    }

    #[test]
    fn revocation_survives_ledger_restart() {
        let mut kernel = MdxKernel::boot_local();
        let identity = identity();
        kernel
            .register_mobile_device(
                MobileDeviceRegistration {
                    tenant_id: "tenant",
                    actor_id: "founder",
                    device_id: "iphone",
                    platform: "ios",
                    display_name: "iPhone",
                    public_key: "p256-key",
                    public_key_thumbprint: "thumb",
                    registered_at_epoch: 10,
                },
                &identity,
            )
            .unwrap();
        kernel
            .register_mobile_host(
                MobileHostRegistration {
                    tenant_id: "tenant",
                    actor_id: "founder",
                    host_id: "mac",
                    display_name: "Mac",
                    public_key_thumbprint: "host-thumb",
                    registered_at_epoch: 11,
                },
                &identity,
            )
            .unwrap();
        kernel
            .record_mobile_pairing(
                MobilePairingRegistration {
                    tenant_id: "tenant",
                    actor_id: "founder",
                    device_id: "iphone",
                    host_id: "mac",
                    paired_at_epoch: 12,
                },
                &identity,
            )
            .unwrap();
        let fingerprint_one = "1".repeat(64);
        let fingerprint_two = "2".repeat(64);
        assert_eq!(
            kernel
                .record_mobile_push_registration(
                    MobilePushRegistration {
                        tenant_id: "tenant",
                        actor_id: "founder",
                        registration_id: "mobile_push_iphone_apns",
                        device_id: "iphone",
                        host_id: "mac",
                        kind: "apns",
                        environment: "sandbox",
                        token_fingerprint: "raw-token-not-a-sha256-fingerprint",
                        device_signature_verification: "verified_registered_device_key",
                        updated_at_epoch: 12,
                    },
                    &identity,
                )
                .unwrap_err(),
            MobileTrustError::Invalid("token_fingerprint")
        );
        let first_push = kernel
            .record_mobile_push_registration(
                MobilePushRegistration {
                    tenant_id: "tenant",
                    actor_id: "founder",
                    registration_id: "mobile_push_iphone_apns",
                    device_id: "iphone",
                    host_id: "mac",
                    kind: "apns",
                    environment: "sandbox",
                    token_fingerprint: &fingerprint_one,
                    device_signature_verification: "verified_registered_device_key",
                    updated_at_epoch: 12,
                },
                &identity,
            )
            .unwrap();
        let replay = kernel
            .record_mobile_push_registration(
                MobilePushRegistration {
                    tenant_id: "tenant",
                    actor_id: "founder",
                    registration_id: "mobile_push_iphone_apns",
                    device_id: "iphone",
                    host_id: "mac",
                    kind: "apns",
                    environment: "sandbox",
                    token_fingerprint: &fingerprint_one,
                    device_signature_verification: "verified_registered_device_key",
                    updated_at_epoch: 13,
                },
                &identity,
            )
            .unwrap();
        assert!(replay.idempotent_replay);
        assert_eq!(replay.receipt_id, first_push.receipt_id);
        let refreshed = kernel
            .record_mobile_push_registration(
                MobilePushRegistration {
                    tenant_id: "tenant",
                    actor_id: "founder",
                    registration_id: "mobile_push_iphone_apns",
                    device_id: "iphone",
                    host_id: "mac",
                    kind: "apns",
                    environment: "sandbox",
                    token_fingerprint: &fingerprint_two,
                    device_signature_verification: "verified_registered_device_key",
                    updated_at_epoch: 14,
                },
                &identity,
            )
            .unwrap();
        assert!(!refreshed.idempotent_replay);
        assert_ne!(refreshed.receipt_id, first_push.receipt_id);
        assert_eq!(
            kernel
                .record_mobile_push_registration(
                    MobilePushRegistration {
                        tenant_id: "tenant",
                        actor_id: "founder",
                        registration_id: "mobile_push_iphone_apns",
                        device_id: "iphone",
                        host_id: "mac",
                        kind: "apns",
                        environment: "sandbox",
                        token_fingerprint: &fingerprint_one,
                        device_signature_verification: "verified_registered_device_key",
                        updated_at_epoch: 12,
                    },
                    &identity,
                )
                .unwrap_err(),
            MobileTrustError::Stale("push_registration")
        );
        kernel
            .revoke_mobile_host("tenant", "founder", "mac", 15, &identity)
            .unwrap();
        assert_eq!(
            kernel
                .mobile_trust_projection("tenant", "founder")
                .push_registrations[0]
                .status,
            "revoked"
        );
        kernel
            .revoke_mobile_device("tenant", "founder", "iphone", 16, &identity)
            .unwrap();
        let entries = kernel.ledger().entries().to_vec();
        let mut restarted = MdxKernel::boot_local();
        restarted.restore_ledger_entries(entries).unwrap();
        let state = restarted.mobile_trust_projection("tenant", "founder");
        assert_eq!(state.devices[0].status, "revoked");
        assert_eq!(state.devices[0].revoked_at_epoch, Some(16));
        assert_eq!(state.hosts[0].status, "revoked");
        assert_eq!(state.pairings[0].status, "revoked");
        assert_eq!(state.push_registrations.len(), 1);
        assert_eq!(state.push_registrations[0].status, "revoked");
        assert_eq!(
            state.push_registrations[0].token_fingerprint,
            fingerprint_two
        );
        assert_eq!(state.push_registrations[0].receipt_id, refreshed.receipt_id);
        assert_eq!(
            state.push_registrations[0].device_signature_verification,
            "verified_registered_device_key"
        );
        assert!(
            restarted
                .require_mobile_active_pairing("tenant", "founder", "iphone", "mac")
                .is_err()
        );
        assert!(restarted.ledger().verify().is_ok());
    }

    #[test]
    fn verified_app_attestation_is_durable_and_key_bound_to_one_identity() {
        let mut kernel = MdxKernel::boot_local();
        let founder = identity();
        kernel
            .register_mobile_device(
                MobileDeviceRegistration {
                    tenant_id: "tenant",
                    actor_id: "founder",
                    device_id: "iphone",
                    platform: "ios",
                    display_name: "iPhone",
                    public_key: "device-key",
                    public_key_thumbprint: "device-thumbprint",
                    registered_at_epoch: 10,
                },
                &founder,
            )
            .unwrap();
        let key_fingerprint = "a".repeat(64);
        let receipt_fingerprint = "b".repeat(64);
        let public_key = "A".repeat(88);
        let report = kernel
            .record_mobile_device_attestation(
                MobileDeviceAttestation {
                    tenant_id: "tenant",
                    actor_id: "founder",
                    device_id: "iphone",
                    key_id_fingerprint: &key_fingerprint,
                    public_key: &public_key,
                    environment: "development",
                    validation_category: 3,
                    bundle_version: "1",
                    receipt_fingerprint: &receipt_fingerprint,
                    attested_at_epoch: 20,
                },
                &founder,
            )
            .unwrap();
        let replay = kernel
            .record_mobile_device_attestation(
                MobileDeviceAttestation {
                    tenant_id: "tenant",
                    actor_id: "founder",
                    device_id: "iphone",
                    key_id_fingerprint: &key_fingerprint,
                    public_key: &public_key,
                    environment: "development",
                    validation_category: 3,
                    bundle_version: "1",
                    receipt_fingerprint: &receipt_fingerprint,
                    attested_at_epoch: 21,
                },
                &founder,
            )
            .unwrap();
        assert!(replay.idempotent_replay);
        assert_eq!(replay.receipt_id, report.receipt_id);

        let intruder = GovernedWriteIdentity::local_demo("intruder");
        kernel
            .register_mobile_device(
                MobileDeviceRegistration {
                    tenant_id: "tenant",
                    actor_id: "intruder",
                    device_id: "other-iphone",
                    platform: "ios",
                    display_name: "Other iPhone",
                    public_key: "other-device-key",
                    public_key_thumbprint: "other-device-thumbprint",
                    registered_at_epoch: 22,
                },
                &intruder,
            )
            .unwrap();
        assert_eq!(
            kernel
                .record_mobile_device_attestation(
                    MobileDeviceAttestation {
                        tenant_id: "tenant",
                        actor_id: "intruder",
                        device_id: "other-iphone",
                        key_id_fingerprint: &key_fingerprint,
                        public_key: &public_key,
                        environment: "development",
                        validation_category: 3,
                        bundle_version: "1",
                        receipt_fingerprint: &receipt_fingerprint,
                        attested_at_epoch: 23,
                    },
                    &intruder,
                )
                .unwrap_err(),
            MobileTrustError::IdentityConflict("attestation")
        );

        let mut restarted = MdxKernel::boot_local();
        restarted
            .restore_ledger_entries(kernel.ledger().entries().to_vec())
            .unwrap();
        let device = &restarted
            .mobile_trust_projection("tenant", "founder")
            .devices[0];
        assert_eq!(device.attestation_risk, "verified_app_attest");
        assert_eq!(
            device.app_attest_key_id_fingerprint.as_deref(),
            Some(key_fingerprint.as_str())
        );
        assert_eq!(
            device.app_attest_public_key.as_deref(),
            Some(public_key.as_str())
        );
        assert_eq!(
            device.app_attest_environment.as_deref(),
            Some("development")
        );
        assert_eq!(device.app_attest_validation_category, Some(3));
        assert_eq!(device.app_attest_bundle_version.as_deref(), Some("1"));
        assert_eq!(device.attested_at_epoch, Some(20));
        assert_eq!(
            device.attestation_receipt_id.as_deref(),
            Some(report.receipt_id.as_str())
        );
        assert!(restarted.ledger().verify().is_ok());
    }
}
