use crate::{RouteResponse, request_security};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use mdx_core::{
    MdxKernel, MobileDeviceRegistration, MobileDeviceTrust, MobileHostRegistration,
    MobileHostTrust, MobilePairingRegistration, MobilePairingTrust, MobileTrustError,
    MobileTrustProjection,
};
use ring::digest::{SHA256, digest};
use ring::rand::{SecureRandom, SystemRandom};
use ring::signature::{
    ECDSA_P256_SHA256_ASN1, ED25519, Ed25519KeyPair, KeyPair, UnparsedPublicKey,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

const CHALLENGE_LIFETIME_SECONDS: u64 = 60;
const MAX_PENDING_CHALLENGES: usize = 4_096;

#[derive(Clone, Debug)]
struct ChallengeRecord {
    tenant_id: String,
    user_id: String,
    device_id: String,
    host_id: String,
    host_public_key_thumbprint: String,
    token: String,
    signature: String,
    expires_at_epoch: u64,
    state: String,
}

#[derive(Debug)]
struct PairingError {
    status: &'static str,
    code: &'static str,
    detail: String,
}

impl PairingError {
    fn bad_request(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: "400 Bad Request",
            code,
            detail: detail.into(),
        }
    }

    fn forbidden(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: "403 Forbidden",
            code,
            detail: detail.into(),
        }
    }

    fn conflict(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: "409 Conflict",
            code,
            detail: detail.into(),
        }
    }

    fn gone(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: "410 Gone",
            code,
            detail: detail.into(),
        }
    }

    fn unavailable(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            status: "503 Service Unavailable",
            code,
            detail: detail.into(),
        }
    }
}

struct MobilePairingService {
    rng: SystemRandom,
    signer: Ed25519KeyPair,
    sequence: u64,
    challenges: BTreeMap<String, ChallengeRecord>,
}

impl MobilePairingService {
    fn new() -> Result<Self, String> {
        let rng = SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng)
            .map_err(|_| "mobile pairing signing key generation failed".to_string())?;
        let signer = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
            .map_err(|_| "mobile pairing signing key parse failed".to_string())?;
        Ok(Self {
            rng,
            signer,
            sequence: 0,
            challenges: BTreeMap::new(),
        })
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.sequence += 1;
        format!("{prefix}_{:08}", self.sequence)
    }

    fn register_device(
        &mut self,
        identity: &request_security::ResolvedWriteIdentity,
        body: &Value,
        now: u64,
        kernel: &mut MdxKernel,
    ) -> Result<Value, PairingError> {
        let device_id = required_identifier(body, "device_id")?;
        let platform = required_string(body, "platform")?;
        if !matches!(platform.as_str(), "ios" | "macos") {
            return Err(PairingError::bad_request(
                "mobile_device_platform_invalid",
                "platform must be ios or macos",
            ));
        }
        let display_name = bounded_string(body, "display_name", 120)?;
        let public_key = bounded_string(body, "public_key", 512)?;
        let public_key_thumbprint = bounded_string(body, "public_key_thumbprint", 256)?;
        validate_device_public_key(&public_key, &public_key_thumbprint)?;
        if public_key_thumbprint.len() != 64 {
            return Err(PairingError::bad_request(
                "mobile_device_key_thumbprint_invalid",
                "public key thumbprint must be at least 16 characters",
            ));
        }
        kernel
            .register_mobile_device(
                MobileDeviceRegistration {
                    tenant_id: &identity.tenant_id,
                    actor_id: &identity.actor_id,
                    device_id: &device_id,
                    platform: &platform,
                    display_name: &display_name,
                    public_key: &public_key,
                    public_key_thumbprint: &public_key_thumbprint,
                    registered_at_epoch: now,
                },
                &identity.identity,
            )
            .map_err(pairing_core_error)?;
        let state = kernel.mobile_trust_projection(&identity.tenant_id, &identity.actor_id);
        let record = state
            .devices
            .into_iter()
            .find(|record| record.device_id == device_id)
            .ok_or_else(|| {
                PairingError::unavailable(
                    "mobile_device_projection_missing",
                    "registered device was not projected",
                )
            })?;
        Ok(device_json(&record))
    }

    fn register_host(
        &mut self,
        identity: &request_security::ResolvedWriteIdentity,
        body: &Value,
        now: u64,
        kernel: &mut MdxKernel,
    ) -> Result<Value, PairingError> {
        if body
            .get("accepts_inbound_connections")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Err(PairingError::forbidden(
                "mobile_host_inbound_listener_forbidden",
                "Forge Anywhere hosts must use an outbound connector",
            ));
        }
        let host_id = required_identifier(body, "host_id")?;
        let display_name = bounded_string(body, "display_name", 120)?;
        let public_key_thumbprint = bounded_string(body, "public_key_thumbprint", 256)?;
        if public_key_thumbprint.len() < 16 {
            return Err(PairingError::bad_request(
                "mobile_host_key_thumbprint_invalid",
                "public key thumbprint must be at least 16 characters",
            ));
        }
        kernel
            .register_mobile_host(
                MobileHostRegistration {
                    tenant_id: &identity.tenant_id,
                    actor_id: &identity.actor_id,
                    host_id: &host_id,
                    display_name: &display_name,
                    public_key_thumbprint: &public_key_thumbprint,
                    registered_at_epoch: now,
                },
                &identity.identity,
            )
            .map_err(pairing_core_error)?;
        let state = kernel.mobile_trust_projection(&identity.tenant_id, &identity.actor_id);
        let record = state
            .hosts
            .into_iter()
            .find(|record| record.host_id == host_id)
            .ok_or_else(|| {
                PairingError::unavailable(
                    "mobile_host_projection_missing",
                    "registered host was not projected",
                )
            })?;
        Ok(host_json(&record))
    }

    fn mint_challenge(
        &mut self,
        identity: &request_security::ResolvedWriteIdentity,
        body: &Value,
        now: u64,
        kernel: &MdxKernel,
    ) -> Result<Value, PairingError> {
        let device_id = required_identifier(body, "device_id")?;
        let host_id = required_identifier(body, "host_id")?;
        self.challenges.retain(|_, challenge| {
            challenge.state == "pending" && now <= challenge.expires_at_epoch
        });
        if self.challenges.len() >= MAX_PENDING_CHALLENGES {
            return Err(PairingError::unavailable(
                "mobile_pairing_challenge_capacity_reached",
                "pairing challenge capacity is temporarily exhausted",
            ));
        }
        let state = kernel.mobile_trust_projection(&identity.tenant_id, &identity.actor_id);
        let device = state
            .devices
            .iter()
            .find(|record| record.device_id == device_id && record.status == "active")
            .ok_or_else(|| {
                PairingError::forbidden(
                    "mobile_device_revoked",
                    "an active registered device is required",
                )
            })?;
        let host = state
            .hosts
            .iter()
            .find(|record| record.host_id == host_id && record.status == "active")
            .ok_or_else(|| {
                PairingError::forbidden(
                    "mobile_host_revoked",
                    "an active registered host is required",
                )
            })?;
        let challenge_id = self.next_id("pairing_challenge");
        let mut nonce_bytes = [0_u8; 32];
        self.rng.fill(&mut nonce_bytes).map_err(|_| {
            PairingError::unavailable(
                "mobile_pairing_entropy_unavailable",
                "pairing challenge entropy is unavailable",
            )
        })?;
        let nonce = URL_SAFE_NO_PAD.encode(nonce_bytes);
        let expires_at_epoch = now + CHALLENGE_LIFETIME_SECONDS;
        let payload = format!(
            "v1|{challenge_id}|{}|{}|{device_id}|{host_id}|{}|{now}|{expires_at_epoch}|{nonce}",
            identity.tenant_id, identity.actor_id, host.public_key_thumbprint,
        );
        let token = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(self.signer.sign(token.as_bytes()).as_ref());
        let challenge = ChallengeRecord {
            tenant_id: identity.tenant_id.clone(),
            user_id: identity.actor_id.clone(),
            device_id,
            host_id,
            host_public_key_thumbprint: host.public_key_thumbprint.clone(),
            token: token.clone(),
            signature: signature.clone(),
            expires_at_epoch,
            state: "pending".to_string(),
        };
        self.challenges
            .insert(challenge_id.clone(), challenge.clone());
        Ok(json!({
            "name": "mdx-mobile-pairing-challenge",
            "status": "PAIRING_CHALLENGE_MINTED",
            "challenge_id": challenge_id,
            "tenant_id": challenge.tenant_id,
            "user_id": challenge.user_id,
            "device_id": device.device_id,
            "host_id": challenge.host_id,
            "host_public_key_thumbprint": challenge.host_public_key_thumbprint,
            "challenge_token": token,
            "signature": signature,
            "server_public_key": URL_SAFE_NO_PAD.encode(self.signer.public_key().as_ref()),
            "issued_at_epoch": now,
            "expires_at_epoch": expires_at_epoch,
            "single_use": true,
            "production_write_allowed": false
        }))
    }

    fn redeem_challenge(
        &mut self,
        identity: &request_security::ResolvedWriteIdentity,
        body: &Value,
        now: u64,
        kernel: &mut MdxKernel,
    ) -> Result<Value, PairingError> {
        let challenge_id = required_identifier(body, "challenge_id")?;
        let token = required_string(body, "challenge_token")?;
        let signature = required_string(body, "signature")?;
        let device_signature = required_string(body, "device_signature")?;
        let public_key = self.signer.public_key().as_ref().to_vec();
        let signature_bytes = URL_SAFE_NO_PAD.decode(&signature).map_err(|_| {
            PairingError::bad_request(
                "mobile_pairing_signature_invalid",
                "signature is not base64url",
            )
        })?;
        UnparsedPublicKey::new(&ED25519, public_key)
            .verify(token.as_bytes(), &signature_bytes)
            .map_err(|_| {
                PairingError::forbidden(
                    "mobile_pairing_signature_invalid",
                    "challenge signature verification failed",
                )
            })?;
        let challenge = self.challenges.get(&challenge_id).ok_or_else(|| {
            PairingError::bad_request(
                "mobile_pairing_challenge_unknown",
                "pairing challenge does not exist",
            )
        })?;
        if challenge.token != token || challenge.signature != signature {
            return Err(PairingError::forbidden(
                "mobile_pairing_challenge_mismatch",
                "challenge token does not match the stored challenge",
            ));
        }
        if challenge.tenant_id != identity.tenant_id || challenge.user_id != identity.actor_id {
            return Err(PairingError::forbidden(
                "mobile_pairing_identity_mismatch",
                "pairing challenge belongs to another account or tenant",
            ));
        }
        if challenge.state != "pending" {
            return Err(PairingError::conflict(
                "mobile_pairing_challenge_consumed",
                "pairing challenge has already been used",
            ));
        }
        if now > challenge.expires_at_epoch {
            if let Some(challenge) = self.challenges.get_mut(&challenge_id) {
                challenge.state = "expired".to_string();
            }
            return Err(PairingError::gone(
                "mobile_pairing_challenge_expired",
                "pairing challenge expired",
            ));
        }
        let device_id = challenge.device_id.clone();
        let host_id = challenge.host_id.clone();
        let state = kernel.mobile_trust_projection(&identity.tenant_id, &identity.actor_id);
        let device = state
            .devices
            .iter()
            .find(|record| record.device_id == device_id && record.status == "active")
            .ok_or_else(|| {
                PairingError::forbidden(
                    "mobile_device_revoked",
                    "an active registered device is required",
                )
            })?;
        if !state
            .hosts
            .iter()
            .any(|record| record.host_id == host_id && record.status == "active")
        {
            return Err(PairingError::forbidden(
                "mobile_host_revoked",
                "an active registered host is required",
            ));
        }
        verify_device_signature(&device.public_key, token.as_bytes(), &device_signature)?;
        let report = kernel
            .record_mobile_pairing(
                MobilePairingRegistration {
                    tenant_id: &identity.tenant_id,
                    actor_id: &identity.actor_id,
                    device_id: &device_id,
                    host_id: &host_id,
                    paired_at_epoch: now,
                },
                &identity.identity,
            )
            .map_err(pairing_core_error)?;
        if let Some(challenge) = self.challenges.get_mut(&challenge_id) {
            challenge.state = "consumed".to_string();
        }
        let state = kernel.mobile_trust_projection(&identity.tenant_id, &identity.actor_id);
        let pairing = state
            .pairings
            .into_iter()
            .find(|record| record.pairing_id == report.record_id)
            .ok_or_else(|| {
                PairingError::unavailable(
                    "mobile_pairing_projection_missing",
                    "recorded pairing was not projected",
                )
            })?;
        Ok(pairing_json(&pairing))
    }

    fn revoke_device(
        &mut self,
        identity: &request_security::ResolvedWriteIdentity,
        body: &Value,
        now: u64,
        kernel: &mut MdxKernel,
    ) -> Result<Value, PairingError> {
        let device_id = required_identifier(body, "device_id")?;
        kernel
            .revoke_mobile_device(
                &identity.tenant_id,
                &identity.actor_id,
                &device_id,
                now,
                &identity.identity,
            )
            .map_err(pairing_core_error)?;
        let state = kernel.mobile_trust_projection(&identity.tenant_id, &identity.actor_id);
        let record = state
            .devices
            .into_iter()
            .find(|record| record.device_id == device_id)
            .ok_or_else(|| {
                PairingError::bad_request("mobile_device_unknown", "device does not exist")
            })?;
        Ok(device_json(&record))
    }

    fn revoke_host(
        &mut self,
        identity: &request_security::ResolvedWriteIdentity,
        body: &Value,
        now: u64,
        kernel: &mut MdxKernel,
    ) -> Result<Value, PairingError> {
        let host_id = required_identifier(body, "host_id")?;
        kernel
            .revoke_mobile_host(
                &identity.tenant_id,
                &identity.actor_id,
                &host_id,
                now,
                &identity.identity,
            )
            .map_err(pairing_core_error)?;
        let state = kernel.mobile_trust_projection(&identity.tenant_id, &identity.actor_id);
        let record = state
            .hosts
            .into_iter()
            .find(|record| record.host_id == host_id)
            .ok_or_else(|| {
                PairingError::bad_request("mobile_host_unknown", "host does not exist")
            })?;
        Ok(host_json(&record))
    }
}

static SERVICE: OnceLock<Arc<RwLock<MobilePairingService>>> = OnceLock::new();

fn service() -> Result<&'static Arc<RwLock<MobilePairingService>>, String> {
    if let Some(service) = SERVICE.get() {
        return Ok(service);
    }
    let candidate = Arc::new(RwLock::new(MobilePairingService::new()?));
    let _ = SERVICE.set(candidate);
    SERVICE
        .get()
        .ok_or_else(|| "mobile pairing service initialization failed".to_string())
}

pub(crate) fn require_active_pairing(
    identity: &request_security::ResolvedWriteIdentity,
    device_id: &str,
    host_id: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<(), String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    kernel
        .require_mobile_active_pairing(&identity.tenant_id, &identity.actor_id, device_id, host_id)
        .map_err(|error| error.code().to_string())
}

pub(crate) fn verify_registered_device_signature(
    identity: &request_security::ResolvedWriteIdentity,
    device_id: &str,
    message: &[u8],
    signature: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<(), String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let state = kernel.mobile_trust_projection(&identity.tenant_id, &identity.actor_id);
    let device = state
        .devices
        .iter()
        .find(|record| record.device_id == device_id && record.status == "active")
        .ok_or_else(|| "mobile_device_revoked".to_string())?;
    verify_device_signature(&device.public_key, message, signature)
        .map_err(|error| error.code.to_string())
}

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    let write = match path {
        "/mobile/devices.json" => Some("device"),
        "/mobile/hosts.json" => Some("host"),
        "/mobile/pairing-challenges.json" => Some("challenge"),
        "/mobile/pairings.json" => Some("pairing"),
        "/mobile/device-revocations.json" => Some("device_revoke"),
        "/mobile/host-revocations.json" => Some("host_revoke"),
        _ => None,
    };
    if let Some(kind) = write {
        if !method.eq_ignore_ascii_case("POST") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let parsed: Value = match serde_json::from_str(body) {
            Ok(parsed) => parsed,
            Err(_) => {
                return Some(Ok(error_response(PairingError::bad_request(
                    "mobile_pairing_body_invalid",
                    "request body must be valid JSON",
                ))));
            }
        };
        let identity = request_security::resolve_governed_write_identity(
            body,
            "local_tenant",
            "local_user",
            "owner",
        );
        let now = now_epoch();
        let service = match service() {
            Ok(service) => service,
            Err(error) => return Some(Err(error)),
        };
        let result = match (service.write(), kernel.write()) {
            (Ok(mut service), Ok(mut kernel)) => match kind {
                "device" => service.register_device(&identity, &parsed, now, &mut kernel),
                "host" => service.register_host(&identity, &parsed, now, &mut kernel),
                "challenge" => service.mint_challenge(&identity, &parsed, now, &kernel),
                "pairing" => service.redeem_challenge(&identity, &parsed, now, &mut kernel),
                "device_revoke" => service.revoke_device(&identity, &parsed, now, &mut kernel),
                "host_revoke" => service.revoke_host(&identity, &parsed, now, &mut kernel),
                _ => unreachable!(),
            },
            (Err(_), _) => return Some(Err("mobile pairing service lock poisoned".to_string())),
            (_, Err(_)) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(match result {
            Ok(value) => RouteResponse::json("200 OK", value.to_string()),
            Err(error) => error_response(error),
        }));
    }

    if path == "/mobile/trust/projection.json" {
        if !method.eq_ignore_ascii_case("GET") {
            return Some(Ok(RouteResponse::text(
                "405 Method Not Allowed",
                "method not allowed\n".to_string(),
            )));
        }
        let identity = request_security::current_verified_identity();
        let (tenant_id, user_id) = identity
            .map(|identity| (identity.tenant_id, identity.actor_id))
            .unwrap_or_else(|| ("local_tenant".to_string(), "local_user".to_string()));
        let projection = match kernel.read() {
            Ok(kernel) => trust_projection_json(
                &kernel.mobile_trust_projection(&tenant_id, &user_id),
                &tenant_id,
                &user_id,
            ),
            Err(_) => return Some(Err("kernel lock poisoned".to_string())),
        };
        return Some(Ok(RouteResponse::json("200 OK", projection.to_string())));
    }
    None
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn required_identifier(body: &Value, key: &'static str) -> Result<String, PairingError> {
    let value = required_string(body, key)?;
    if value.len() > 160
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(PairingError::bad_request(
            "mobile_pairing_identifier_invalid",
            format!("{key} contains unsupported characters"),
        ));
    }
    Ok(value)
}

fn required_string(body: &Value, key: &'static str) -> Result<String, PairingError> {
    body.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            PairingError::bad_request(
                "mobile_pairing_field_required",
                format!("{key} is required"),
            )
        })
}

fn bounded_string(body: &Value, key: &'static str, max: usize) -> Result<String, PairingError> {
    let value = required_string(body, key)?;
    if value.chars().count() > max {
        return Err(PairingError::bad_request(
            "mobile_pairing_field_too_long",
            format!("{key} exceeds {max} characters"),
        ));
    }
    Ok(value)
}

fn pairing_core_error(error: MobileTrustError) -> PairingError {
    PairingError::forbidden(error.code(), error.code())
}

fn decode_device_public_key(public_key: &str) -> Result<Vec<u8>, PairingError> {
    let bytes = URL_SAFE_NO_PAD.decode(public_key).map_err(|_| {
        PairingError::bad_request(
            "mobile_device_public_key_invalid",
            "public_key must be base64url",
        )
    })?;
    if bytes.len() != 65 || bytes.first() != Some(&4) {
        return Err(PairingError::bad_request(
            "mobile_device_public_key_invalid",
            "public_key must be a P-256 X9.63 public key",
        ));
    }
    Ok(bytes)
}

fn validate_device_public_key(public_key: &str, thumbprint: &str) -> Result<Vec<u8>, PairingError> {
    let bytes = decode_device_public_key(public_key)?;
    let observed = digest(&SHA256, &bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if observed != thumbprint {
        return Err(PairingError::forbidden(
            "mobile_device_key_thumbprint_mismatch",
            "public_key does not match public_key_thumbprint",
        ));
    }
    Ok(bytes)
}

fn verify_device_signature(
    public_key: &str,
    message: &[u8],
    signature: &str,
) -> Result<(), PairingError> {
    let public_key = decode_device_public_key(public_key)?;
    let signature = URL_SAFE_NO_PAD.decode(signature).map_err(|_| {
        PairingError::bad_request(
            "mobile_device_signature_invalid",
            "device_signature must be base64url",
        )
    })?;
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, public_key)
        .verify(message, &signature)
        .map_err(|_| {
            PairingError::forbidden(
                "mobile_device_signature_invalid",
                "device possession signature verification failed",
            )
        })
}

fn device_json(record: &MobileDeviceTrust) -> Value {
    json!({
        "device_id": record.device_id,
        "tenant_id": record.tenant_id,
        "user_id": record.user_id,
        "platform": record.platform,
        "display_name": record.display_name,
        "public_key_thumbprint": record.public_key_thumbprint,
        "device_possession_verification": "required_for_pairing",
        "attestation_risk": record.attestation_risk,
        "attestation_verification": if record.attestation_risk == "verified_app_attest" {
            "apple_app_attest_server_cryptographic"
        } else {
            "not_verified"
        },
        "app_attest_environment": record.app_attest_environment,
        "app_attest_validation_category": record.app_attest_validation_category,
        "app_attest_bundle_version": record.app_attest_bundle_version,
        "attested_at_epoch": record.attested_at_epoch,
        "attestation_receipt_id": record.attestation_receipt_id,
        "attestation_state_source": "kernel_receipt_projection",
        "attestation_restart_durable": true,
        "status": record.status,
        "registered_at_epoch": record.registered_at_epoch,
        "revoked_at_epoch": record.revoked_at_epoch,
        "production_execution_allowed": false
    })
}

fn host_json(record: &MobileHostTrust) -> Value {
    json!({
        "host_id": record.host_id,
        "tenant_id": record.tenant_id,
        "user_id": record.user_id,
        "display_name": record.display_name,
        "public_key_thumbprint": record.public_key_thumbprint,
        "status": record.status,
        "connector_status": if record.status == "revoked" { "revoked" } else { "offline" },
        "registered_at_epoch": record.registered_at_epoch,
        "revoked_at_epoch": record.revoked_at_epoch,
        "accepts_inbound_connections": false,
        "production_execution_allowed": false
    })
}

fn pairing_json(record: &MobilePairingTrust) -> Value {
    json!({
        "name": "mdx-mobile-pairing",
        "status": if record.status == "active" { "PAIRED" } else { "REVOKED" },
        "pairing_id": record.pairing_id,
        "tenant_id": record.tenant_id,
        "user_id": record.user_id,
        "device_id": record.device_id,
        "host_id": record.host_id,
        "paired_at_epoch": record.paired_at_epoch,
        "pairing_status": record.status,
        "device_possession": "server_verified_signature",
        "receipt_id": record.receipt_id,
        "production_execution_allowed": false
    })
}

fn trust_projection_json(state: &MobileTrustProjection, tenant_id: &str, user_id: &str) -> Value {
    json!({
        "name": "mdx-mobile-trust-projection",
        "status": "OK",
        "tenant_id": tenant_id,
        "user_id": user_id,
        "devices": state.devices.iter().map(device_json).collect::<Vec<_>>(),
        "hosts": state.hosts.iter().map(host_json).collect::<Vec<_>>(),
        "pairings": state.pairings.iter().map(pairing_json).collect::<Vec<_>>(),
        "state_source": "kernel_receipt_projection",
        "restart_durable": true,
        "host_connector_direction": "outbound_only",
        "inbound_host_listener_allowed": false,
        "production_execution_allowed": false
    })
}

fn error_response(error: PairingError) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        json!({
            "status": "REFUSED",
            "error": error.code,
            "detail": error.detail,
            "http_semantics": error.status,
            "production_execution_allowed": false
        })
        .to_string(),
    )
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    use ring::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair};

    fn identity(tenant: &str, user: &str) -> request_security::ResolvedWriteIdentity {
        request_security::resolve_governed_write_identity(
            &format!(r#"{{"tenant_id":"{tenant}","actor_id":"{user}"}}"#),
            tenant,
            user,
            "owner",
        )
    }

    fn key_pair() -> EcdsaKeyPair {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng).unwrap()
    }

    fn device_body(key: &EcdsaKeyPair) -> Value {
        let public_key = key.public_key().as_ref();
        let thumbprint = digest(&SHA256, public_key)
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        json!({
            "device_id": "iphone_founder",
            "platform": "ios",
            "display_name": "Founder's iPhone",
            "public_key": URL_SAFE_NO_PAD.encode(public_key),
            "public_key_thumbprint": thumbprint
        })
    }

    fn host_body() -> Value {
        json!({
            "host_id": "mac_founder",
            "display_name": "Founder's MacBook Pro",
            "public_key_thumbprint": "host_thumbprint_123456",
            "accepts_inbound_connections": false
        })
    }

    fn ready_service() -> (
        MobilePairingService,
        request_security::ResolvedWriteIdentity,
        MdxKernel,
        EcdsaKeyPair,
    ) {
        let mut service = MobilePairingService::new().unwrap();
        let identity = identity("tenant_mdx", "founder");
        let mut kernel = MdxKernel::boot_local();
        let key = key_pair();
        service
            .register_device(&identity, &device_body(&key), 1_000, &mut kernel)
            .unwrap();
        service
            .register_host(&identity, &host_body(), 1_000, &mut kernel)
            .unwrap();
        (service, identity, kernel, key)
    }

    fn signed_redemption(challenge: &Value, key: &EcdsaKeyPair) -> Value {
        let token = challenge["challenge_token"].as_str().unwrap();
        let device_signature = key.sign(&SystemRandom::new(), token.as_bytes()).unwrap();
        json!({
            "challenge_id": challenge["challenge_id"],
            "challenge_token": token,
            "signature": challenge["signature"],
            "device_signature": URL_SAFE_NO_PAD.encode(device_signature.as_ref())
        })
    }

    #[test]
    fn signed_pairing_is_single_use_and_under_sixty_seconds() {
        let (mut service, owner_identity, mut kernel, key) = ready_service();
        let challenge = service
            .mint_challenge(
                &owner_identity,
                &json!({"device_id":"iphone_founder","host_id":"mac_founder"}),
                1_010,
                &kernel,
            )
            .unwrap();
        assert_eq!(challenge["expires_at_epoch"].as_u64(), Some(1_070));
        let redemption = signed_redemption(&challenge, &key);
        let paired = service
            .redeem_challenge(&owner_identity, &redemption, 1_020, &mut kernel)
            .unwrap();
        assert_eq!(paired["status"], "PAIRED");
        let replay = service
            .redeem_challenge(&owner_identity, &redemption, 1_021, &mut kernel)
            .unwrap_err();
        assert_eq!(replay.code, "mobile_pairing_challenge_consumed");
    }

    #[test]
    fn expired_and_tampered_challenges_fail_closed() {
        let (mut service, identity, mut kernel, key) = ready_service();
        let challenge = service
            .mint_challenge(
                &identity,
                &json!({"device_id":"iphone_founder","host_id":"mac_founder"}),
                1_010,
                &kernel,
            )
            .unwrap();
        let tampered = json!({
            "challenge_id": challenge["challenge_id"],
            "challenge_token": "tampered",
            "signature": challenge["signature"],
            "device_signature": URL_SAFE_NO_PAD.encode([0_u8; 64])
        });
        assert_eq!(
            service
                .redeem_challenge(&identity, &tampered, 1_020, &mut kernel)
                .unwrap_err()
                .code,
            "mobile_pairing_signature_invalid"
        );
        let original = signed_redemption(&challenge, &key);
        assert_eq!(
            service
                .redeem_challenge(&identity, &original, 1_071, &mut kernel)
                .unwrap_err()
                .code,
            "mobile_pairing_challenge_expired"
        );
    }

    #[test]
    fn wrong_identity_and_revoked_resources_fail_closed() {
        let (mut service, owner_identity, mut kernel, key) = ready_service();
        let challenge = service
            .mint_challenge(
                &owner_identity,
                &json!({"device_id":"iphone_founder","host_id":"mac_founder"}),
                1_010,
                &kernel,
            )
            .unwrap();
        let redemption = signed_redemption(&challenge, &key);
        let other = identity("tenant_other", "intruder");
        assert_eq!(
            service
                .redeem_challenge(&other, &redemption, 1_020, &mut kernel)
                .unwrap_err()
                .code,
            "mobile_pairing_identity_mismatch"
        );
        service
            .revoke_device(
                &owner_identity,
                &json!({"device_id":"iphone_founder"}),
                1_021,
                &mut kernel,
            )
            .unwrap();
        assert_eq!(
            service
                .register_device(&owner_identity, &device_body(&key), 1_022, &mut kernel)
                .unwrap_err()
                .code,
            "mobile_device_revoked"
        );
        assert_eq!(
            service
                .redeem_challenge(&owner_identity, &redemption, 1_023, &mut kernel)
                .unwrap_err()
                .code,
            "mobile_device_revoked"
        );
    }

    #[test]
    fn inbound_host_listener_is_never_admitted() {
        let mut service = MobilePairingService::new().unwrap();
        let identity = identity("tenant_mdx", "founder");
        let mut kernel = MdxKernel::boot_local();
        let mut body = host_body();
        body["accepts_inbound_connections"] = Value::Bool(true);
        let error = service
            .register_host(&identity, &body, 1_000, &mut kernel)
            .unwrap_err();
        assert_eq!(error.code, "mobile_host_inbound_listener_forbidden");
    }

    #[test]
    fn caller_supplied_attestation_reference_never_promotes_device_trust() {
        let mut service = MobilePairingService::new().unwrap();
        let identity = identity("tenant_mdx", "founder");
        let mut kernel = MdxKernel::boot_local();
        let key = key_pair();
        let mut body = device_body(&key);
        body["attestation_evidence_ref"] = json!("app_attest_key:caller-controlled");
        let device = service
            .register_device(&identity, &body, 1_000, &mut kernel)
            .unwrap();
        assert_eq!(device["attestation_risk"], "unverified");
        assert_eq!(device["attestation_verification"], "not_verified");
        assert!(
            kernel
                .mobile_trust_projection("tenant_mdx", "founder")
                .devices[0]
                .app_attest_key_id_fingerprint
                .is_none()
        );
    }

    #[test]
    fn http_routes_complete_the_pairing_vertical() {
        let kernel = Arc::new(RwLock::new(mdx_core::MdxKernel::boot_local()));
        let key = key_pair();
        let public_key = key.public_key().as_ref();
        let thumbprint = digest(&SHA256, public_key)
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let device = json!({
            "device_id": "iphone_http_vertical",
            "platform": "ios",
            "display_name": "HTTP iPhone",
            "public_key": URL_SAFE_NO_PAD.encode(public_key),
            "public_key_thumbprint": thumbprint
        });
        let device = route_response("POST", "/mobile/devices.json", &device.to_string(), &kernel)
            .unwrap()
            .unwrap();
        assert_eq!(device.status_for_test(), "200 OK");
        let host = route_response(
            "POST",
            "/mobile/hosts.json",
            r#"{"host_id":"mac_http_vertical","display_name":"HTTP Mac","public_key_thumbprint":"host_http_thumbprint_123456","accepts_inbound_connections":false}"#,
            &kernel,
        )
        .unwrap()
        .unwrap();
        assert_eq!(host.status_for_test(), "200 OK");
        let challenge = route_response(
            "POST",
            "/mobile/pairing-challenges.json",
            r#"{"device_id":"iphone_http_vertical","host_id":"mac_http_vertical"}"#,
            &kernel,
        )
        .unwrap()
        .unwrap();
        let challenge: Value = serde_json::from_str(challenge.body_text()).unwrap();
        let redemption = signed_redemption(&challenge, &key);
        let paired = route_response(
            "POST",
            "/mobile/pairings.json",
            &redemption.to_string(),
            &kernel,
        )
        .unwrap()
        .unwrap();
        let paired: Value = serde_json::from_str(paired.body_text()).unwrap();
        assert_eq!(paired["status"], "PAIRED");
        assert_eq!(paired["production_execution_allowed"], false);
    }
}

#[cfg(test)]
mod possession_tests {
    use super::*;
    use ring::signature::{ECDSA_P256_SHA256_ASN1_SIGNING, EcdsaKeyPair};

    fn identity() -> request_security::ResolvedWriteIdentity {
        request_security::resolve_governed_write_identity(
            r#"{"tenant_id":"tenant_mdx","actor_id":"founder"}"#,
            "tenant_mdx",
            "founder",
            "owner",
        )
    }

    fn key_pair() -> EcdsaKeyPair {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng).unwrap()
    }

    fn device_body(key: &EcdsaKeyPair) -> Value {
        let public_key = key.public_key().as_ref();
        let thumbprint = digest(&SHA256, public_key)
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        json!({
            "device_id": "iphone_founder",
            "platform": "ios",
            "display_name": "Founder's iPhone",
            "public_key": URL_SAFE_NO_PAD.encode(public_key),
            "public_key_thumbprint": thumbprint
        })
    }

    fn host_body() -> Value {
        json!({
            "host_id": "mac_founder",
            "display_name": "Founder's MacBook Pro",
            "public_key_thumbprint": "host_thumbprint_123456",
            "accepts_inbound_connections": false
        })
    }

    fn signed_redemption(challenge: &Value, key: &EcdsaKeyPair) -> Value {
        let token = challenge["challenge_token"].as_str().unwrap();
        let signature = key.sign(&SystemRandom::new(), token.as_bytes()).unwrap();
        json!({
            "challenge_id": challenge["challenge_id"],
            "challenge_token": token,
            "signature": challenge["signature"],
            "device_signature": URL_SAFE_NO_PAD.encode(signature.as_ref())
        })
    }

    fn ready() -> (
        MobilePairingService,
        request_security::ResolvedWriteIdentity,
        MdxKernel,
        EcdsaKeyPair,
    ) {
        let mut service = MobilePairingService::new().unwrap();
        let identity = identity();
        let mut kernel = MdxKernel::boot_local();
        let key = key_pair();
        service
            .register_device(&identity, &device_body(&key), 1_000, &mut kernel)
            .unwrap();
        service
            .register_host(&identity, &host_body(), 1_000, &mut kernel)
            .unwrap();
        (service, identity, kernel, key)
    }

    #[test]
    fn pairing_requires_the_registered_device_key_and_is_single_use() {
        let (mut service, identity, mut kernel, key) = ready();
        let challenge = service
            .mint_challenge(
                &identity,
                &json!({"device_id":"iphone_founder","host_id":"mac_founder"}),
                1_010,
                &kernel,
            )
            .unwrap();
        assert_eq!(challenge["expires_at_epoch"], 1_070);

        let forged = signed_redemption(&challenge, &key_pair());
        assert_eq!(
            service
                .redeem_challenge(&identity, &forged, 1_020, &mut kernel)
                .unwrap_err()
                .code,
            "mobile_device_signature_invalid"
        );

        let redemption = signed_redemption(&challenge, &key);
        let paired = service
            .redeem_challenge(&identity, &redemption, 1_020, &mut kernel)
            .unwrap();
        assert_eq!(paired["device_possession"], "server_verified_signature");
        assert_eq!(
            service
                .redeem_challenge(&identity, &redemption, 1_021, &mut kernel)
                .unwrap_err()
                .code,
            "mobile_pairing_challenge_consumed"
        );
    }

    #[test]
    fn revoked_pairing_stays_revoked_after_kernel_restart() {
        let (mut service, identity, mut kernel, key) = ready();
        let challenge = service
            .mint_challenge(
                &identity,
                &json!({"device_id":"iphone_founder","host_id":"mac_founder"}),
                1_010,
                &kernel,
            )
            .unwrap();
        service
            .redeem_challenge(
                &identity,
                &signed_redemption(&challenge, &key),
                1_020,
                &mut kernel,
            )
            .unwrap();
        service
            .revoke_device(
                &identity,
                &json!({"device_id":"iphone_founder"}),
                1_030,
                &mut kernel,
            )
            .unwrap();

        let mut restarted = MdxKernel::boot_local();
        restarted
            .restore_ledger_entries(kernel.ledger().entries().to_vec())
            .unwrap();
        assert_eq!(
            restarted
                .mobile_trust_projection("tenant_mdx", "founder")
                .devices[0]
                .status,
            "revoked"
        );
        assert!(
            restarted
                .require_mobile_active_pairing(
                    "tenant_mdx",
                    "founder",
                    "iphone_founder",
                    "mac_founder"
                )
                .is_err()
        );
    }

    #[test]
    fn registration_rejects_a_caller_supplied_thumbprint() {
        let mut service = MobilePairingService::new().unwrap();
        let mut kernel = MdxKernel::boot_local();
        let mut body = device_body(&key_pair());
        body["public_key_thumbprint"] = json!("0".repeat(64));
        assert_eq!(
            service
                .register_device(&identity(), &body, 1_000, &mut kernel)
                .unwrap_err()
                .code,
            "mobile_device_key_thumbprint_mismatch"
        );
        assert!(
            kernel
                .mobile_trust_projection("tenant_mdx", "founder")
                .devices
                .is_empty()
        );
    }
}
