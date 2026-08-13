//! Apple App Attest enrollment for the iPhone companion.
//!
//! The server owns the challenge and the trust decision. The client-provided
//! key identifier and attestation object remain untrusted until every Apple
//! attestation check succeeds. Only derived public material and fingerprints
//! are written to the kernel receipt ledger.
//!
//! The bounded DER certificate-extension walking and WebPKI chain pattern are
//! adapted from `appattest` 0.1.1 by Peter Farr under the MIT License. See
//! `crates/mdx-server/THIRD_PARTY_NOTICES.md`.

use crate::{RouteResponse, request_security};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use mdx_core::{MdxKernel, MobileDeviceAttestation, MobileDeviceTrust, MobileTrustError};
use minicbor::{Decoder, data::Type};
use ring::{
    digest,
    rand::{SecureRandom, SystemRandom},
};
use rustls_pki_types::{CertificateDer, TrustAnchor, UnixTime};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use webpki::{
    EndEntityCert, ExtendedKeyUsageValidator, KeyPurposeIdIter, anchor_from_trusted_cert,
};

const APPLE_APP_ATTEST_ROOT_CA: &[u8] = include_bytes!("apple_app_attestation_root_ca.pem");
const CHALLENGE_LIFETIME_SECONDS: u64 = 300;
const MAX_PENDING_CHALLENGES: usize = 4_096;
const MAX_ATTESTATION_BASE64_BYTES: usize = 196_608;
const MAX_ATTESTATION_CBOR_BYTES: usize = 147_456;
const MAX_CERTIFICATE_BYTES: usize = 16_384;
const MAX_RECEIPT_BYTES: usize = 96 * 1_024;
const MAX_AUTHENTICATOR_DATA_BYTES: usize = 16_384;
const MAX_CBOR_MAP_ENTRIES: u64 = 32;

const FLAG_ATTESTED_CREDENTIAL_DATA: u8 = 0x40;
const PRODUCTION_AAGUID: &[u8; 16] = b"appattest\0\0\0\0\0\0\0";
const DEVELOPMENT_AAGUID: &[u8; 16] = b"appattestdevelop";
const CREDENTIAL_CERTIFICATE_NONCE_OID: &[u8] =
    &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x63, 0x64, 0x08, 0x02];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AppAttestEnvironment {
    Development,
    Production,
}

impl AppAttestEnvironment {
    fn parse(value: &str) -> Result<Self, AppAttestFailure> {
        match value {
            "development" => Ok(Self::Development),
            "production" => Ok(Self::Production),
            _ => Err(AppAttestFailure::configuration(
                "mobile_app_attest_environment_invalid",
                "MDX_APP_ATTEST_ENVIRONMENT must be development or production",
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Production => "production",
        }
    }

    fn aaguid(self) -> &'static [u8; 16] {
        match self {
            Self::Development => DEVELOPMENT_AAGUID,
            Self::Production => PRODUCTION_AAGUID,
        }
    }

    fn default_validation_categories(self) -> BTreeSet<u32> {
        match self {
            Self::Development => BTreeSet::from([3]),
            // MDx's default production lane is TestFlight or the App Store.
            // Enterprise and ad-hoc category 5 remains available only through
            // the explicit allowlist environment variable.
            Self::Production => BTreeSet::from([2, 4]),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AppAttestConfiguration {
    app_id: String,
    environment: AppAttestEnvironment,
    bundle_version: String,
    allowed_validation_categories: BTreeSet<u32>,
}

impl AppAttestConfiguration {
    fn from_environment() -> Result<Self, AppAttestFailure> {
        let app_id = required_environment("MDX_APP_ATTEST_APP_ID")?;
        if app_id.len() > 256 || !app_id.contains('.') {
            return Err(AppAttestFailure::configuration(
                "mobile_app_attest_app_id_invalid",
                "MDX_APP_ATTEST_APP_ID must be the App ID prefix and bundle identifier",
            ));
        }
        let environment =
            AppAttestEnvironment::parse(&required_environment("MDX_APP_ATTEST_ENVIRONMENT")?)?;
        let bundle_version = required_environment("MDX_APP_ATTEST_BUNDLE_VERSION")?;
        if bundle_version.len() > 64 {
            return Err(AppAttestFailure::configuration(
                "mobile_app_attest_bundle_version_invalid",
                "MDX_APP_ATTEST_BUNDLE_VERSION exceeds 64 characters",
            ));
        }
        let allowed_validation_categories =
            match std::env::var("MDX_APP_ATTEST_ALLOWED_VALIDATION_CATEGORIES") {
                Ok(value) if !value.trim().is_empty() => {
                    let categories = value
                        .split(',')
                        .map(str::trim)
                        .map(|value| value.parse::<u32>())
                        .collect::<Result<BTreeSet<_>, _>>()
                        .map_err(|_| {
                            AppAttestFailure::configuration(
                                "mobile_app_attest_validation_categories_invalid",
                                "allowed validation categories must be comma-separated integers",
                            )
                        })?;
                    if categories.is_empty() || categories.contains(&0) {
                        return Err(AppAttestFailure::configuration(
                            "mobile_app_attest_validation_categories_invalid",
                            "allowed validation categories cannot be empty or contain zero",
                        ));
                    }
                    categories
                }
                _ => environment.default_validation_categories(),
            };
        Ok(Self {
            app_id,
            environment,
            bundle_version,
            allowed_validation_categories,
        })
    }
}

fn required_environment(name: &'static str) -> Result<String, AppAttestFailure> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppAttestFailure::configuration(
                "mobile_app_attest_not_configured",
                format!("{name} is required before App Attest enrollment can run"),
            )
        })
}

#[derive(Clone, Debug)]
struct AppAttestChallenge {
    tenant_id: String,
    user_id: String,
    device_id: String,
    key_id: String,
    key_id_fingerprint: String,
    challenge: String,
    expires_at_epoch: u64,
    state: ChallengeState,
    configuration: AppAttestConfiguration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChallengeState {
    Pending,
    Consumed,
    Failed,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct VerifiedAppAttestation {
    public_key: [u8; 65],
    receipt_fingerprint: String,
    validation_category: u32,
    bundle_version: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AppAttestFailure {
    status: &'static str,
    code: &'static str,
    detail: String,
}

impl AppAttestFailure {
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

    fn configuration(code: &'static str, detail: impl Into<String>) -> Self {
        Self::unavailable(code, detail)
    }

    fn verification(code: &'static str, detail: impl Into<String>) -> Self {
        Self::forbidden(code, detail)
    }
}

impl fmt::Display for AppAttestFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.detail)
    }
}

struct ParsedAttestation {
    certificates: Vec<Vec<u8>>,
    receipt: Vec<u8>,
    auth_data: Vec<u8>,
}

#[derive(Debug)]
struct ParsedAuthenticatorData {
    rp_id_hash: [u8; 32],
    counter: u32,
    aaguid: [u8; 16],
    credential_id: Vec<u8>,
    credential_public_key: [u8; 65],
    validation_category: u32,
    bundle_version: String,
}

fn verify_app_attestation(
    attestation_base64: &str,
    challenge: &str,
    key_id: &str,
    configuration: &AppAttestConfiguration,
    root_certificate_pem: &[u8],
    verification_time: UnixTime,
) -> Result<VerifiedAppAttestation, AppAttestFailure> {
    let parsed = parse_attestation(attestation_base64)?;
    verify_certificate_chain(
        &parsed.certificates,
        root_certificate_pem,
        verification_time,
    )?;
    let authenticator = parse_authenticator_data(&parsed.auth_data)?;
    let expected_rp_id_hash = sha256(configuration.app_id.as_bytes());
    if authenticator.rp_id_hash != expected_rp_id_hash {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_app_id_mismatch",
            "attestation RP ID does not match the configured Apple App ID",
        ));
    }
    if authenticator.counter != 0 {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_counter_invalid",
            "initial App Attest counter must equal zero",
        ));
    }
    if &authenticator.aaguid != configuration.environment.aaguid() {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_environment_mismatch",
            "attestation environment does not match server configuration",
        ));
    }
    if !configuration
        .allowed_validation_categories
        .contains(&authenticator.validation_category)
    {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_validation_category_refused",
            "Apple validation category is not allowed for this deployment",
        ));
    }
    if authenticator.bundle_version != configuration.bundle_version {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_bundle_version_mismatch",
            "attested bundle version does not match server configuration",
        ));
    }
    let key_id_bytes = decode_key_id(key_id)?;
    if authenticator.credential_id.as_slice() != key_id_bytes {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_credential_id_mismatch",
            "attestation credential ID does not match the App Attest key ID",
        ));
    }
    let public_key = extract_public_key(&parsed.certificates[0])?;
    if authenticator.credential_public_key != public_key {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_encoded_key_mismatch",
            "authenticator credential key does not match the credential certificate",
        ));
    }
    if sha256(&public_key) != key_id_bytes {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_public_key_mismatch",
            "credential certificate public key does not match the App Attest key ID",
        ));
    }
    let client_data_hash = sha256(challenge.as_bytes());
    let mut nonce_input = Vec::with_capacity(parsed.auth_data.len() + client_data_hash.len());
    nonce_input.extend_from_slice(&parsed.auth_data);
    nonce_input.extend_from_slice(&client_data_hash);
    let expected_nonce = sha256(&nonce_input);
    let certificate_nonce = extract_credential_certificate_nonce(&parsed.certificates[0])?;
    if certificate_nonce != expected_nonce {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_nonce_mismatch",
            "credential certificate nonce does not bind the server challenge",
        ));
    }
    if parsed.receipt.is_empty() {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_receipt_missing",
            "attestation receipt is required",
        ));
    }
    Ok(VerifiedAppAttestation {
        public_key,
        receipt_fingerprint: hex_sha256(&parsed.receipt),
        validation_category: authenticator.validation_category,
        bundle_version: authenticator.bundle_version,
    })
}

fn decode_key_id(key_id: &str) -> Result<[u8; 32], AppAttestFailure> {
    if key_id.len() > 128 {
        return Err(AppAttestFailure::bad_request(
            "mobile_app_attest_key_id_invalid",
            "App Attest key ID is too large",
        ));
    }
    let bytes = STANDARD.decode(key_id).map_err(|_| {
        AppAttestFailure::bad_request(
            "mobile_app_attest_key_id_invalid",
            "App Attest key ID must be standard base64",
        )
    })?;
    bytes.try_into().map_err(|_| {
        AppAttestFailure::bad_request(
            "mobile_app_attest_key_id_invalid",
            "App Attest key ID must decode to 32 bytes",
        )
    })
}

fn parse_attestation(value: &str) -> Result<ParsedAttestation, AppAttestFailure> {
    if value.len() > MAX_ATTESTATION_BASE64_BYTES {
        return Err(AppAttestFailure::bad_request(
            "mobile_app_attest_object_too_large",
            "attestation object exceeds the accepted size",
        ));
    }
    let cbor = STANDARD.decode(value).map_err(|_| {
        AppAttestFailure::bad_request(
            "mobile_app_attest_object_invalid",
            "attestation object must be standard base64",
        )
    })?;
    if cbor.len() > MAX_ATTESTATION_CBOR_BYTES {
        return Err(AppAttestFailure::bad_request(
            "mobile_app_attest_object_too_large",
            "decoded attestation object exceeds the accepted size",
        ));
    }
    let mut decoder = Decoder::new(&cbor);
    let entries = decoder.map().map_err(cbor_error)?;
    let mut format_valid = false;
    let mut statement_seen = false;
    let mut certificates = None;
    let mut receipt = None;
    let mut auth_data = None;
    visit_map(&mut decoder, entries, |decoder, key| {
        match key {
            "fmt" => {
                if format_valid {
                    return Err(duplicate_field("fmt"));
                }
                if decoder.str().map_err(cbor_error)? != "apple-appattest" {
                    return Err(AppAttestFailure::verification(
                        "mobile_app_attest_format_invalid",
                        "attestation format must be apple-appattest",
                    ));
                }
                format_valid = true;
            }
            "attStmt" => {
                if statement_seen {
                    return Err(duplicate_field("attStmt"));
                }
                statement_seen = true;
                let count = decoder.map().map_err(cbor_error)?;
                visit_map(decoder, count, |decoder, key| {
                    match key {
                        "x5c" => {
                            if certificates.is_some() {
                                return Err(duplicate_field("x5c"));
                            }
                            let count = decoder.array().map_err(cbor_error)?.ok_or_else(|| {
                                AppAttestFailure::bad_request(
                                    "mobile_app_attest_object_invalid",
                                    "x5c must be a bounded CBOR array",
                                )
                            })?;
                            if !(2..=3).contains(&count) {
                                return Err(AppAttestFailure::verification(
                                    "mobile_app_attest_certificate_chain_invalid",
                                    "x5c must contain a leaf and one or two intermediate certificates",
                                ));
                            }
                            let mut chain = Vec::with_capacity(count as usize);
                            for _ in 0..count {
                                let certificate = decoder.bytes().map_err(cbor_error)?;
                                if certificate.len() > MAX_CERTIFICATE_BYTES {
                                    return Err(AppAttestFailure::bad_request(
                                        "mobile_app_attest_certificate_too_large",
                                        "attestation certificate exceeds the accepted size",
                                    ));
                                }
                                chain.push(certificate.to_vec());
                            }
                            certificates = Some(chain);
                        }
                        "receipt" => {
                            if receipt.is_some() {
                                return Err(duplicate_field("receipt"));
                            }
                            let bytes = decoder.bytes().map_err(cbor_error)?;
                            if bytes.len() > MAX_RECEIPT_BYTES {
                                return Err(AppAttestFailure::bad_request(
                                    "mobile_app_attest_receipt_too_large",
                                    "attestation receipt exceeds the accepted size",
                                ));
                            }
                            receipt = Some(bytes.to_vec());
                        }
                        _ => decoder.skip().map_err(cbor_error)?,
                    }
                    Ok(())
                })?;
            }
            "authData" => {
                if auth_data.is_some() {
                    return Err(duplicate_field("authData"));
                }
                let bytes = decoder.bytes().map_err(cbor_error)?;
                if bytes.len() > MAX_AUTHENTICATOR_DATA_BYTES {
                    return Err(AppAttestFailure::bad_request(
                        "mobile_app_attest_auth_data_too_large",
                        "attestation authenticator data exceeds the accepted size",
                    ));
                }
                auth_data = Some(bytes.to_vec());
            }
            _ => decoder.skip().map_err(cbor_error)?,
        }
        Ok(())
    })?;
    if decoder.position() != cbor.len() {
        return Err(AppAttestFailure::bad_request(
            "mobile_app_attest_object_invalid",
            "attestation object contains trailing CBOR data",
        ));
    }
    if !format_valid {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_format_invalid",
            "attestation format is missing",
        ));
    }
    Ok(ParsedAttestation {
        certificates: certificates.ok_or_else(|| {
            AppAttestFailure::bad_request(
                "mobile_app_attest_certificate_chain_missing",
                "attestation certificate chain is missing",
            )
        })?,
        receipt: receipt.ok_or_else(|| {
            AppAttestFailure::bad_request(
                "mobile_app_attest_receipt_missing",
                "attestation receipt is missing",
            )
        })?,
        auth_data: auth_data.ok_or_else(|| {
            AppAttestFailure::bad_request(
                "mobile_app_attest_auth_data_missing",
                "attestation authenticator data is missing",
            )
        })?,
    })
}

fn visit_map<'a, F>(
    decoder: &mut Decoder<'a>,
    entries: Option<u64>,
    mut visitor: F,
) -> Result<(), AppAttestFailure>
where
    F: FnMut(&mut Decoder<'a>, &str) -> Result<(), AppAttestFailure>,
{
    match entries {
        Some(count) => {
            if count > MAX_CBOR_MAP_ENTRIES {
                return Err(AppAttestFailure::bad_request(
                    "mobile_app_attest_object_invalid",
                    "attestation CBOR map contains too many entries",
                ));
            }
            for _ in 0..count {
                let key = decoder.str().map_err(cbor_error)?.to_string();
                visitor(decoder, &key)?;
            }
        }
        None => {
            let mut count = 0_u64;
            while decoder.datatype().map_err(cbor_error)? != Type::Break {
                if count >= MAX_CBOR_MAP_ENTRIES {
                    return Err(AppAttestFailure::bad_request(
                        "mobile_app_attest_object_invalid",
                        "attestation CBOR map contains too many entries",
                    ));
                }
                count += 1;
                let key = decoder.str().map_err(cbor_error)?.to_string();
                visitor(decoder, &key)?;
            }
            decoder.skip().map_err(cbor_error)?;
        }
    }
    Ok(())
}

fn parse_authenticator_data(bytes: &[u8]) -> Result<ParsedAuthenticatorData, AppAttestFailure> {
    if bytes.len() < 87 {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_auth_data_invalid",
            "authenticator data is too short",
        ));
    }
    let rp_id_hash = bytes[0..32].try_into().expect("fixed RP ID slice");
    let flags = bytes[32];
    if flags & FLAG_ATTESTED_CREDENTIAL_DATA == 0 {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_auth_data_flags_invalid",
            "authenticator data must contain attested credentials",
        ));
    }
    let counter = u32::from_be_bytes(bytes[33..37].try_into().expect("fixed counter slice"));
    let aaguid = bytes[37..53].try_into().expect("fixed AAGUID slice");
    let credential_length =
        u16::from_be_bytes(bytes[53..55].try_into().expect("fixed length slice")) as usize;
    if credential_length != 32 || bytes.len() < 55 + credential_length {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_credential_id_invalid",
            "credential ID must contain the 32-byte App Attest key hash",
        ));
    }
    let credential_id = bytes[55..55 + credential_length].to_vec();
    let mut decoder = Decoder::new(&bytes[55 + credential_length..]);
    let credential_public_key = parse_credential_public_key(&mut decoder)?;
    if decoder.position() >= bytes.len() - 55 - credential_length {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_extensions_missing",
            "Apple attestation extensions are missing",
        ));
    }
    let entries = decoder.map().map_err(cbor_error)?;
    let mut validation_category = None;
    let mut bundle_version = None;
    visit_map(&mut decoder, entries, |decoder, key| {
        match key {
            "apple_validation_category_01" => {
                if validation_category.is_some() {
                    return Err(duplicate_field("apple_validation_category_01"));
                }
                validation_category = Some(read_validation_category(decoder)?)
            }
            "apple_bundle_version_01" => {
                if bundle_version.is_some() {
                    return Err(duplicate_field("apple_bundle_version_01"));
                }
                bundle_version = Some(decoder.str().map_err(cbor_error)?.to_string())
            }
            _ => decoder.skip().map_err(cbor_error)?,
        }
        Ok(())
    })?;
    if decoder.position() != bytes.len() - 55 - credential_length {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_auth_data_invalid",
            "authenticator data contains trailing bytes",
        ));
    }
    Ok(ParsedAuthenticatorData {
        rp_id_hash,
        counter,
        aaguid,
        credential_id,
        credential_public_key,
        validation_category: validation_category.ok_or_else(|| {
            AppAttestFailure::verification(
                "mobile_app_attest_validation_category_missing",
                "Apple validation category extension is missing",
            )
        })?,
        bundle_version: bundle_version.ok_or_else(|| {
            AppAttestFailure::verification(
                "mobile_app_attest_bundle_version_missing",
                "Apple bundle version extension is missing",
            )
        })?,
    })
}

fn read_validation_category(decoder: &mut Decoder<'_>) -> Result<u32, AppAttestFailure> {
    match decoder.datatype().map_err(cbor_error)? {
        // Apple's production attestation object encodes the UInt32 extension
        // value as a four-byte, little-endian CBOR byte string.
        Type::Bytes => {
            let bytes: [u8; 4] = decoder
                .bytes()
                .map_err(cbor_error)?
                .try_into()
                .map_err(|_| {
                    AppAttestFailure::verification(
                        "mobile_app_attest_validation_category_invalid",
                        "Apple validation category must contain four bytes",
                    )
                })?;
            Ok(u32::from_le_bytes(bytes))
        }
        // Retain the integer form used by the deterministic development
        // fixture so both supported environments exercise the same verifier.
        Type::U8 | Type::U16 | Type::U32 => decoder.u32().map_err(cbor_error),
        _ => Err(AppAttestFailure::verification(
            "mobile_app_attest_validation_category_invalid",
            "Apple validation category has an unsupported CBOR representation",
        )),
    }
}

fn parse_credential_public_key(decoder: &mut Decoder<'_>) -> Result<[u8; 65], AppAttestFailure> {
    let entries = decoder.map().map_err(cbor_error)?;
    let mut key_type = None;
    let mut algorithm = None;
    let mut curve = None;
    let mut x = None;
    let mut y = None;
    let mut visit = |decoder: &mut Decoder<'_>| -> Result<(), AppAttestFailure> {
        let key = decoder.i32().map_err(cbor_error)?;
        match key {
            1 if key_type.is_none() => key_type = Some(decoder.u32().map_err(cbor_error)?),
            3 if algorithm.is_none() => algorithm = Some(decoder.i32().map_err(cbor_error)?),
            -1 if curve.is_none() => curve = Some(decoder.u32().map_err(cbor_error)?),
            -2 if x.is_none() => x = Some(read_coordinate(decoder, "x")?),
            -3 if y.is_none() => y = Some(read_coordinate(decoder, "y")?),
            1 | 3 | -1 | -2 | -3 => return Err(duplicate_field("COSE credential key")),
            _ => decoder.skip().map_err(cbor_error)?,
        }
        Ok(())
    };
    match entries {
        Some(count) => {
            if count > MAX_CBOR_MAP_ENTRIES {
                return Err(AppAttestFailure::bad_request(
                    "mobile_app_attest_encoded_key_invalid",
                    "credential key contains too many entries",
                ));
            }
            for _ in 0..count {
                visit(decoder)?;
            }
        }
        None => {
            let mut count = 0_u64;
            while decoder.datatype().map_err(cbor_error)? != Type::Break {
                if count >= MAX_CBOR_MAP_ENTRIES {
                    return Err(AppAttestFailure::bad_request(
                        "mobile_app_attest_encoded_key_invalid",
                        "credential key contains too many entries",
                    ));
                }
                count += 1;
                visit(decoder)?;
            }
            decoder.skip().map_err(cbor_error)?;
        }
    }
    if key_type != Some(2) || algorithm != Some(-7) || curve != Some(1) {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_encoded_key_invalid",
            "credential key must be an ES256 P-256 COSE key",
        ));
    }
    let mut public_key = [0_u8; 65];
    public_key[0] = 4;
    public_key[1..33].copy_from_slice(&x.ok_or_else(|| {
        AppAttestFailure::verification(
            "mobile_app_attest_encoded_key_invalid",
            "credential key x coordinate is missing",
        )
    })?);
    public_key[33..65].copy_from_slice(&y.ok_or_else(|| {
        AppAttestFailure::verification(
            "mobile_app_attest_encoded_key_invalid",
            "credential key y coordinate is missing",
        )
    })?);
    Ok(public_key)
}

fn duplicate_field(field: &str) -> AppAttestFailure {
    AppAttestFailure::verification(
        "mobile_app_attest_duplicate_field",
        format!("attestation contains duplicate {field} data"),
    )
}

fn read_coordinate(decoder: &mut Decoder<'_>, name: &str) -> Result<[u8; 32], AppAttestFailure> {
    decoder
        .bytes()
        .map_err(cbor_error)?
        .try_into()
        .map_err(|_| {
            AppAttestFailure::verification(
                "mobile_app_attest_encoded_key_invalid",
                format!("credential key {name} coordinate must contain 32 bytes"),
            )
        })
}

fn cbor_error(error: minicbor::decode::Error) -> AppAttestFailure {
    AppAttestFailure::bad_request(
        "mobile_app_attest_object_invalid",
        format!("attestation CBOR is invalid: {error}"),
    )
}

struct AnyExtendedKeyUsage;

impl ExtendedKeyUsageValidator for AnyExtendedKeyUsage {
    fn validate(&self, _iter: KeyPurposeIdIter<'_, '_>) -> Result<(), webpki::Error> {
        Ok(())
    }
}

static APPLE_SIGNATURE_ALGORITHMS: &[&dyn rustls_pki_types::SignatureVerificationAlgorithm] = &[
    webpki::ring::ECDSA_P256_SHA256,
    webpki::ring::ECDSA_P256_SHA384,
    webpki::ring::ECDSA_P384_SHA256,
    webpki::ring::ECDSA_P384_SHA384,
];

fn verify_certificate_chain(
    certificates: &[Vec<u8>],
    root_certificate_pem: &[u8],
    verification_time: UnixTime,
) -> Result<(), AppAttestFailure> {
    if !(2..=3).contains(&certificates.len()) {
        return Err(AppAttestFailure::verification(
            "mobile_app_attest_certificate_chain_invalid",
            "attestation certificate chain has an invalid length",
        ));
    }
    let root_der = pem_certificate_to_der(root_certificate_pem)?;
    let root = CertificateDer::from(root_der.as_slice());
    let trust_anchor: TrustAnchor<'_> = anchor_from_trusted_cert(&root).map_err(|_| {
        AppAttestFailure::unavailable(
            "mobile_app_attest_root_invalid",
            "pinned Apple App Attestation Root CA is invalid",
        )
    })?;
    let leaf = CertificateDer::from(certificates[0].as_slice());
    let leaf = EndEntityCert::try_from(&leaf).map_err(|_| {
        AppAttestFailure::verification(
            "mobile_app_attest_certificate_invalid",
            "credential certificate is invalid",
        )
    })?;
    let intermediates = certificates[1..]
        .iter()
        .map(|certificate| CertificateDer::from(certificate.as_slice()))
        .collect::<Vec<_>>();
    leaf.verify_for_usage(
        APPLE_SIGNATURE_ALGORITHMS,
        &[trust_anchor],
        &intermediates,
        verification_time,
        AnyExtendedKeyUsage,
        None,
        None,
    )
    .map_err(|_| {
        AppAttestFailure::verification(
            "mobile_app_attest_certificate_chain_invalid",
            "credential certificate chain is not anchored to Apple's pinned root",
        )
    })?;
    Ok(())
}

fn pem_certificate_to_der(pem: &[u8]) -> Result<Vec<u8>, AppAttestFailure> {
    let text = std::str::from_utf8(pem).map_err(|_| {
        AppAttestFailure::unavailable(
            "mobile_app_attest_root_invalid",
            "pinned Apple root certificate is not UTF-8 PEM",
        )
    })?;
    let body = text
        .strip_prefix("-----BEGIN CERTIFICATE-----")
        .and_then(|value| value.strip_suffix("-----END CERTIFICATE-----\n"))
        .or_else(|| {
            text.strip_prefix("-----BEGIN CERTIFICATE-----")
                .and_then(|value| value.strip_suffix("-----END CERTIFICATE-----"))
        })
        .ok_or_else(|| {
            AppAttestFailure::unavailable(
                "mobile_app_attest_root_invalid",
                "pinned Apple root certificate has invalid PEM markers",
            )
        })?;
    let compact = body
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    STANDARD.decode(compact).map_err(|_| {
        AppAttestFailure::unavailable(
            "mobile_app_attest_root_invalid",
            "pinned Apple root certificate has invalid PEM base64",
        )
    })
}

fn der_read_length(bytes: &[u8], position: usize) -> Option<(usize, usize)> {
    let first = *bytes.get(position)?;
    if first < 0x80 {
        return Some((first as usize, 1));
    }
    let count = (first & 0x7f) as usize;
    if count == 0 || count > 4 || position + count >= bytes.len() {
        return None;
    }
    let mut length = 0usize;
    for offset in 0..count {
        length = (length << 8) | *bytes.get(position + 1 + offset)? as usize;
    }
    Some((length, 1 + count))
}

fn der_unwrap_tag(bytes: &[u8], expected_tag: u8) -> Option<&[u8]> {
    if *bytes.first()? != expected_tag {
        return None;
    }
    let (length, consumed) = der_read_length(bytes, 1)?;
    let start = 1 + consumed;
    bytes.get(start..start + length)
}

fn der_find_extension<'a>(certificate: &'a [u8], oid: &[u8]) -> Option<&'a [u8]> {
    let certificate = der_unwrap_tag(certificate, 0x30)?;
    let tbs_certificate = der_unwrap_tag(certificate, 0x30)?;
    let mut position = 0;
    while position < tbs_certificate.len() {
        let tag = *tbs_certificate.get(position)?;
        let (length, consumed) = der_read_length(tbs_certificate, position + 1)?;
        let value_start = position + 1 + consumed;
        let value_end = value_start + length;
        if value_end > tbs_certificate.len() {
            return None;
        }
        if tag == 0xa3 {
            let extensions = der_unwrap_tag(&tbs_certificate[value_start..value_end], 0x30)?;
            return der_scan_extensions(extensions, oid);
        }
        position = value_end;
    }
    None
}

fn der_scan_extensions<'a>(extensions: &'a [u8], oid: &[u8]) -> Option<&'a [u8]> {
    let mut position = 0;
    while position < extensions.len() {
        let extension = der_unwrap_tag(&extensions[position..], 0x30)?;
        let (length, consumed) = der_read_length(extensions, position + 1)?;
        let extension_end = position + 1 + consumed + length;
        if extension.first()? == &0x06 {
            let (oid_length, oid_consumed) = der_read_length(extension, 1)?;
            let oid_bytes = extension.get(1 + oid_consumed..1 + oid_consumed + oid_length)?;
            if oid_bytes == oid {
                let remainder = &extension[1 + oid_consumed + oid_length..];
                let value_start = if remainder.first() == Some(&0x01) {
                    let (boolean_length, boolean_consumed) = der_read_length(remainder, 1)?;
                    1 + boolean_consumed + boolean_length
                } else {
                    0
                };
                return der_unwrap_tag(&remainder[value_start..], 0x04);
            }
        }
        position = extension_end;
    }
    None
}

fn extract_credential_certificate_nonce(certificate: &[u8]) -> Result<[u8; 32], AppAttestFailure> {
    let extension =
        der_find_extension(certificate, CREDENTIAL_CERTIFICATE_NONCE_OID).ok_or_else(|| {
            AppAttestFailure::verification(
                "mobile_app_attest_nonce_missing",
                "credential certificate nonce extension is missing",
            )
        })?;
    let sequence = der_unwrap_tag(extension, 0x30).ok_or_else(|| {
        AppAttestFailure::verification(
            "mobile_app_attest_nonce_invalid",
            "credential certificate nonce extension is invalid",
        )
    })?;
    let explicit = der_unwrap_tag(sequence, 0xa1).ok_or_else(|| {
        AppAttestFailure::verification(
            "mobile_app_attest_nonce_invalid",
            "credential certificate nonce extension is invalid",
        )
    })?;
    let nonce = der_unwrap_tag(explicit, 0x04).ok_or_else(|| {
        AppAttestFailure::verification(
            "mobile_app_attest_nonce_invalid",
            "credential certificate nonce extension is invalid",
        )
    })?;
    nonce.try_into().map_err(|_| {
        AppAttestFailure::verification(
            "mobile_app_attest_nonce_invalid",
            "credential certificate nonce must contain 32 bytes",
        )
    })
}

fn extract_public_key(certificate: &[u8]) -> Result<[u8; 65], AppAttestFailure> {
    let certificate = der_unwrap_tag(certificate, 0x30).ok_or_else(|| {
        AppAttestFailure::verification(
            "mobile_app_attest_certificate_invalid",
            "credential certificate is invalid DER",
        )
    })?;
    let tbs_certificate = der_unwrap_tag(certificate, 0x30).ok_or_else(|| {
        AppAttestFailure::verification(
            "mobile_app_attest_certificate_invalid",
            "credential certificate is invalid DER",
        )
    })?;
    let mut position = 0;
    let mut sequence_count = 0;
    while position < tbs_certificate.len() {
        let tag = *tbs_certificate.get(position).ok_or_else(|| {
            AppAttestFailure::verification(
                "mobile_app_attest_certificate_invalid",
                "credential certificate is truncated",
            )
        })?;
        let (length, consumed) =
            der_read_length(tbs_certificate, position + 1).ok_or_else(|| {
                AppAttestFailure::verification(
                    "mobile_app_attest_certificate_invalid",
                    "credential certificate has an invalid DER length",
                )
            })?;
        let value_start = position + 1 + consumed;
        let value_end = value_start + length;
        if value_end > tbs_certificate.len() {
            return Err(AppAttestFailure::verification(
                "mobile_app_attest_certificate_invalid",
                "credential certificate is truncated",
            ));
        }
        if tag == 0x30 {
            sequence_count += 1;
            if sequence_count == 5 {
                let subject_public_key_info = &tbs_certificate[value_start..value_end];
                let (algorithm_length, algorithm_consumed) =
                    der_read_length(subject_public_key_info, 1).ok_or_else(|| {
                        AppAttestFailure::verification(
                            "mobile_app_attest_public_key_invalid",
                            "credential certificate public key is invalid",
                        )
                    })?;
                let algorithm_end = 1 + algorithm_consumed + algorithm_length;
                let bit_string = der_unwrap_tag(
                    subject_public_key_info
                        .get(algorithm_end..)
                        .ok_or_else(|| {
                            AppAttestFailure::verification(
                                "mobile_app_attest_public_key_invalid",
                                "credential certificate public key is missing",
                            )
                        })?,
                    0x03,
                )
                .ok_or_else(|| {
                    AppAttestFailure::verification(
                        "mobile_app_attest_public_key_invalid",
                        "credential certificate public key is invalid",
                    )
                })?;
                if bit_string.first() != Some(&0) {
                    return Err(AppAttestFailure::verification(
                        "mobile_app_attest_public_key_invalid",
                        "credential certificate public key has unused bits",
                    ));
                }
                return bit_string[1..].try_into().map_err(|_| {
                    AppAttestFailure::verification(
                        "mobile_app_attest_public_key_invalid",
                        "credential certificate public key must be an uncompressed P-256 point",
                    )
                });
            }
        }
        position = value_end;
    }
    Err(AppAttestFailure::verification(
        "mobile_app_attest_public_key_invalid",
        "credential certificate public key is missing",
    ))
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    digest::digest(&digest::SHA256, bytes)
        .as_ref()
        .try_into()
        .expect("SHA-256 is always 32 bytes")
}

fn hex_sha256(bytes: &[u8]) -> String {
    sha256(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

struct MobileAppAttestService {
    rng: SystemRandom,
    challenges: BTreeMap<String, AppAttestChallenge>,
}

impl MobileAppAttestService {
    fn new() -> Self {
        Self {
            rng: SystemRandom::new(),
            challenges: BTreeMap::new(),
        }
    }

    fn mint_challenge(
        &mut self,
        identity: &request_security::ResolvedWriteIdentity,
        body: &Value,
        now: u64,
        kernel: &MdxKernel,
        configuration: AppAttestConfiguration,
    ) -> Result<Value, AppAttestFailure> {
        let device_id = required_identifier(body, "device_id")?;
        let key_id = bounded_string(body, "key_id", 128)?;
        let key_id_bytes = decode_key_id(&key_id)?;
        let key_id_fingerprint = hex_sha256(&key_id_bytes);
        self.challenges
            .retain(|_, challenge| now <= challenge.expires_at_epoch);
        if self
            .challenges
            .values()
            .filter(|challenge| challenge.state == ChallengeState::Pending)
            .count()
            >= MAX_PENDING_CHALLENGES
        {
            return Err(AppAttestFailure::unavailable(
                "mobile_app_attest_challenge_capacity_reached",
                "App Attest challenge capacity is temporarily exhausted",
            ));
        }
        let state = kernel.mobile_trust_projection(&identity.tenant_id, &identity.actor_id);
        let device = state
            .devices
            .iter()
            .find(|device| device.device_id == device_id && device.status == "active")
            .ok_or_else(|| {
                AppAttestFailure::forbidden(
                    "mobile_device_revoked",
                    "an active registered device is required before App Attest enrollment",
                )
            })?;
        if device.platform != "ios" {
            return Err(AppAttestFailure::forbidden(
                "mobile_app_attest_platform_invalid",
                "this App Attest enrollment route accepts registered iOS devices only",
            ));
        }
        let mut challenge_bytes = [0_u8; 32];
        self.rng.fill(&mut challenge_bytes).map_err(|_| {
            AppAttestFailure::unavailable(
                "mobile_app_attest_entropy_unavailable",
                "App Attest challenge entropy is unavailable",
            )
        })?;
        let challenge = hex_bytes(&challenge_bytes);
        let challenge_id = format!("app_attest_{}", &challenge[..32]);
        let expires_at_epoch = now + CHALLENGE_LIFETIME_SECONDS;
        self.challenges.insert(
            challenge_id.clone(),
            AppAttestChallenge {
                tenant_id: identity.tenant_id.clone(),
                user_id: identity.actor_id.clone(),
                device_id: device_id.clone(),
                key_id,
                key_id_fingerprint,
                challenge: challenge.clone(),
                expires_at_epoch,
                state: ChallengeState::Pending,
                configuration: configuration.clone(),
            },
        );
        Ok(json!({
            "name": "mdx-mobile-app-attest-challenge",
            "status": "APP_ATTEST_CHALLENGE_MINTED",
            "challenge_id": challenge_id,
            "device_id": device_id,
            "challenge": challenge,
            "environment": configuration.environment.as_str(),
            "issued_at_epoch": now,
            "expires_at_epoch": expires_at_epoch,
            "single_use": true,
            "server_verification_required": true,
            "production_execution_allowed": false
        }))
    }

    fn verify_attestation(
        &mut self,
        identity: &request_security::ResolvedWriteIdentity,
        body: &Value,
        now: u64,
        kernel: &mut MdxKernel,
        root_certificate_pem: &[u8],
    ) -> Result<Value, AppAttestFailure> {
        let challenge_id = required_identifier(body, "challenge_id")?;
        let device_id = required_identifier(body, "device_id")?;
        let key_id = bounded_string(body, "key_id", 128)?;
        let attestation_object =
            bounded_string(body, "attestation_object", MAX_ATTESTATION_BASE64_BYTES)?;
        let challenge = self.challenges.get(&challenge_id).cloned().ok_or_else(|| {
            AppAttestFailure::bad_request(
                "mobile_app_attest_challenge_unknown",
                "App Attest challenge does not exist",
            )
        })?;
        if challenge.tenant_id != identity.tenant_id
            || challenge.user_id != identity.actor_id
            || challenge.device_id != device_id
        {
            return Err(AppAttestFailure::forbidden(
                "mobile_app_attest_identity_mismatch",
                "App Attest challenge belongs to another account or device",
            ));
        }
        if challenge.key_id != key_id {
            return Err(AppAttestFailure::forbidden(
                "mobile_app_attest_key_mismatch",
                "App Attest key does not match the challenged key",
            ));
        }
        match challenge.state {
            ChallengeState::Pending => {}
            ChallengeState::Consumed => {
                return Err(AppAttestFailure::conflict(
                    "mobile_app_attest_challenge_consumed",
                    "App Attest challenge has already been used",
                ));
            }
            ChallengeState::Failed => {
                return Err(AppAttestFailure::conflict(
                    "mobile_app_attest_challenge_failed",
                    "App Attest challenge has already failed verification",
                ));
            }
            ChallengeState::Expired => {
                return Err(AppAttestFailure::gone(
                    "mobile_app_attest_challenge_expired",
                    "App Attest challenge expired",
                ));
            }
        }
        if now > challenge.expires_at_epoch {
            if let Some(stored) = self.challenges.get_mut(&challenge_id) {
                stored.state = ChallengeState::Expired;
            }
            return Err(AppAttestFailure::gone(
                "mobile_app_attest_challenge_expired",
                "App Attest challenge expired",
            ));
        }
        let state = kernel.mobile_trust_projection(&identity.tenant_id, &identity.actor_id);
        if !state.devices.iter().any(|device| {
            device.device_id == device_id && device.platform == "ios" && device.status == "active"
        }) {
            return Err(AppAttestFailure::forbidden(
                "mobile_device_revoked",
                "an active registered iOS device is required",
            ));
        }
        let verified = verify_app_attestation(
            &attestation_object,
            &challenge.challenge,
            &key_id,
            &challenge.configuration,
            root_certificate_pem,
            UnixTime::since_unix_epoch(Duration::from_secs(now)),
        );
        let verified = match verified {
            Ok(verified) => verified,
            Err(error) => {
                if let Some(stored) = self.challenges.get_mut(&challenge_id) {
                    stored.state = ChallengeState::Failed;
                }
                return Err(error);
            }
        };
        let public_key = STANDARD.encode(verified.public_key);
        let report = kernel.record_mobile_device_attestation(
            MobileDeviceAttestation {
                tenant_id: &identity.tenant_id,
                actor_id: &identity.actor_id,
                device_id: &device_id,
                key_id_fingerprint: &challenge.key_id_fingerprint,
                public_key: &public_key,
                environment: challenge.configuration.environment.as_str(),
                validation_category: verified.validation_category,
                bundle_version: &verified.bundle_version,
                receipt_fingerprint: &verified.receipt_fingerprint,
                attested_at_epoch: now,
            },
            &identity.identity,
        );
        let report = match report {
            Ok(report) => report,
            Err(error) => {
                if let Some(stored) = self.challenges.get_mut(&challenge_id) {
                    stored.state = ChallengeState::Failed;
                }
                return Err(core_error(error));
            }
        };
        if let Some(stored) = self.challenges.get_mut(&challenge_id) {
            stored.state = ChallengeState::Consumed;
        }
        let projection = kernel.mobile_trust_projection(&identity.tenant_id, &identity.actor_id);
        let device = projection
            .devices
            .into_iter()
            .find(|device| device.device_id == device_id)
            .ok_or_else(|| {
                AppAttestFailure::unavailable(
                    "mobile_app_attest_projection_missing",
                    "verified device attestation was not projected",
                )
            })?;
        Ok(attestation_receipt_json(
            &device,
            &report.receipt_id,
            report.idempotent_replay,
        ))
    }
}

static SERVICE: OnceLock<Arc<RwLock<MobileAppAttestService>>> = OnceLock::new();

fn service() -> &'static Arc<RwLock<MobileAppAttestService>> {
    SERVICE.get_or_init(|| Arc::new(RwLock::new(MobileAppAttestService::new())))
}

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    let action = match path {
        "/mobile/app-attest-challenges.json" => Some("challenge"),
        "/mobile/app-attestations.json" => Some("attestation"),
        _ => None,
    }?;
    if !method.eq_ignore_ascii_case("POST") {
        return Some(Ok(RouteResponse::text(
            "405 Method Not Allowed",
            "method not allowed\n".to_string(),
        )));
    }
    let parsed: Value = match serde_json::from_str(body) {
        Ok(parsed) => parsed,
        Err(_) => {
            return Some(Ok(error_response(AppAttestFailure::bad_request(
                "mobile_app_attest_body_invalid",
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
    let service = service();
    let result = match (service.write(), kernel.write()) {
        (Ok(mut service), Ok(mut kernel)) => match action {
            "challenge" => match AppAttestConfiguration::from_environment() {
                Ok(configuration) => {
                    service.mint_challenge(&identity, &parsed, now, &kernel, configuration)
                }
                Err(error) => Err(error),
            },
            "attestation" => service.verify_attestation(
                &identity,
                &parsed,
                now,
                &mut kernel,
                APPLE_APP_ATTEST_ROOT_CA,
            ),
            _ => unreachable!(),
        },
        (Err(_), _) => return Some(Err("mobile App Attest service lock poisoned".to_string())),
        (_, Err(_)) => return Some(Err("kernel lock poisoned".to_string())),
    };
    Some(Ok(match result {
        Ok(value) => RouteResponse::json("200 OK", value.to_string()),
        Err(error) => error_response(error),
    }))
}

fn required_identifier(body: &Value, key: &'static str) -> Result<String, AppAttestFailure> {
    let value = bounded_string(body, key, 160)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(AppAttestFailure::bad_request(
            "mobile_app_attest_identifier_invalid",
            format!("{key} contains unsupported characters"),
        ));
    }
    Ok(value)
}

fn bounded_string(
    body: &Value,
    key: &'static str,
    maximum: usize,
) -> Result<String, AppAttestFailure> {
    let value = body
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppAttestFailure::bad_request(
                "mobile_app_attest_field_required",
                format!("{key} is required"),
            )
        })?;
    if value.len() > maximum {
        return Err(AppAttestFailure::bad_request(
            "mobile_app_attest_field_too_large",
            format!("{key} exceeds {maximum} bytes"),
        ));
    }
    Ok(value.to_string())
}

fn core_error(error: MobileTrustError) -> AppAttestFailure {
    AppAttestFailure::forbidden(error.code(), error.code())
}

fn attestation_receipt_json(
    device: &MobileDeviceTrust,
    receipt_id: &str,
    idempotent_replay: bool,
) -> Value {
    json!({
        "name": "mdx-mobile-app-attestation",
        "status": "APP_ATTEST_VERIFIED",
        "device_id": device.device_id,
        "attestation_risk": device.attestation_risk,
        "attestation_verification": "apple_app_attest_server_cryptographic",
        "environment": device.app_attest_environment,
        "validation_category": device.app_attest_validation_category,
        "bundle_version": device.app_attest_bundle_version,
        "receipt_id": receipt_id,
        "state_source": "kernel_receipt_projection",
        "restart_durable": true,
        "raw_attestation_recorded": false,
        "raw_apple_receipt_recorded": false,
        "idempotent_replay": idempotent_replay,
        "authority_opened": "none",
        "production_execution_allowed": false
    })
}

fn error_response(error: AppAttestFailure) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        json!({
            "status": "REFUSED",
            "error": error.code,
            "detail": error.detail,
            "http_semantics": error.status,
            "attestation_verified": false,
            "production_execution_allowed": false
        })
        .to_string(),
    )
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{GovernedWriteIdentity, MobileDeviceRegistration};

    const FIXTURE_CHALLENGE: &str =
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const FIXTURE_KEY_ID: &str = "uKyoruJXif4jMng8NfDd8FT2Dfwx4tE2/6eqmOXf7Bk=";
    const FIXTURE_APP_ID: &str = "TEAMID1234.com.mdx.anywhere";
    const FIXTURE_ATTESTATION: &str =
        include_str!("../tests/fixtures/mobile-app-attest/valid-development-attestation.b64");
    const APPLE_2026_PRODUCTION_ATTESTATION: &str =
        include_str!("../tests/fixtures/mobile-app-attest/apple-2026-production-attestation.b64");
    const APPLE_2026_KEY_ID: &str = "zgSY9YSD+7TaDXssY6WlOPVS1K3Lmk+pFhlcSWE+ZV0=";
    const FIXTURE_ROOT: &[u8] = include_bytes!("../tests/fixtures/mobile-app-attest/test-root.pem");

    fn configuration() -> AppAttestConfiguration {
        AppAttestConfiguration {
            app_id: FIXTURE_APP_ID.to_string(),
            environment: AppAttestEnvironment::Development,
            bundle_version: "1".to_string(),
            allowed_validation_categories: BTreeSet::from([3]),
        }
    }

    fn identity(user: &str) -> request_security::ResolvedWriteIdentity {
        request_security::resolve_governed_write_identity(
            &format!(r#"{{"tenant_id":"tenant_mdx","actor_id":"{user}"}}"#),
            "tenant_mdx",
            user,
            "owner",
        )
    }

    fn registered_kernel(user: &str) -> MdxKernel {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .register_mobile_device(
                MobileDeviceRegistration {
                    tenant_id: "tenant_mdx",
                    actor_id: user,
                    device_id: "iphone_fixture",
                    platform: "ios",
                    display_name: "Fixture iPhone",
                    public_key: "device-possession-public-key",
                    public_key_thumbprint: "device-possession-thumbprint",
                    registered_at_epoch: 1,
                },
                &GovernedWriteIdentity::local_demo(user),
            )
            .unwrap();
        kernel
    }

    #[test]
    fn full_apple_validation_contract_accepts_a_bound_development_fixture() {
        let verified = verify_app_attestation(
            FIXTURE_ATTESTATION.trim(),
            FIXTURE_CHALLENGE,
            FIXTURE_KEY_ID,
            &configuration(),
            FIXTURE_ROOT,
            UnixTime::now(),
        )
        .unwrap();
        assert_eq!(verified.validation_category, 3);
        assert_eq!(verified.bundle_version, "1");
        assert_eq!(
            STANDARD.encode(sha256(&verified.public_key)),
            FIXTURE_KEY_ID
        );
        assert_eq!(verified.receipt_fingerprint.len(), 64);
    }

    #[test]
    fn current_apple_validation_guide_fixture_matches_parser_and_chain() {
        // Primary oracle published by Apple in the 2026 Attestation Object
        // Validation Guide. Apple's sample composite appends its example
        // challenge bytes directly, while the normative prose and MDx client
        // hash the challenge before attestKey. Exercise the sample's exact
        // bytes here and keep MDx's normative hash path covered above.
        let parsed = parse_attestation(APPLE_2026_PRODUCTION_ATTESTATION.trim()).unwrap();
        verify_certificate_chain(
            &parsed.certificates,
            APPLE_APP_ATTEST_ROOT_CA,
            UnixTime::since_unix_epoch(Duration::from_secs(1_776_795_192)),
        )
        .unwrap();

        let authenticator = parse_authenticator_data(&parsed.auth_data).unwrap();
        assert_eq!(
            authenticator.rp_id_hash,
            sha256(b"1234567890.com.example.myapp")
        );
        assert_eq!(authenticator.counter, 0);
        assert_eq!(&authenticator.aaguid, PRODUCTION_AAGUID);
        assert_eq!(authenticator.validation_category, 1);
        assert_eq!(authenticator.bundle_version, "1");
        assert_eq!(
            authenticator.credential_id,
            decode_key_id(APPLE_2026_KEY_ID).unwrap()
        );

        let public_key = extract_public_key(&parsed.certificates[0]).unwrap();
        assert_eq!(authenticator.credential_public_key, public_key);
        assert_eq!(STANDARD.encode(sha256(&public_key)), APPLE_2026_KEY_ID);

        let mut guide_composite = parsed.auth_data.clone();
        guide_composite.extend_from_slice(b"example_server_challenge");
        assert_eq!(
            extract_credential_certificate_nonce(&parsed.certificates[0]).unwrap(),
            sha256(&guide_composite)
        );
        assert!(!parsed.receipt.is_empty());
    }

    #[test]
    fn production_distribution_defaults_exclude_enterprise_and_ad_hoc() {
        assert_eq!(
            AppAttestEnvironment::Production.default_validation_categories(),
            BTreeSet::from([2, 4])
        );
    }

    #[test]
    fn production_root_is_the_pinned_apple_app_attestation_ca() {
        let der = pem_certificate_to_der(APPLE_APP_ATTEST_ROOT_CA).unwrap();
        assert_eq!(
            hex_sha256(&der),
            "1cb9823ba28ba6ad2d33a006941de2ae4f513ef1d4e831b9f7e0fa7b6242c932"
        );
    }

    #[test]
    fn challenge_app_environment_category_and_bundle_are_all_fail_closed() {
        let wrong_challenge = verify_app_attestation(
            FIXTURE_ATTESTATION.trim(),
            "wrong-challenge",
            FIXTURE_KEY_ID,
            &configuration(),
            FIXTURE_ROOT,
            UnixTime::now(),
        )
        .unwrap_err();
        assert_eq!(wrong_challenge.code, "mobile_app_attest_nonce_mismatch");

        let mut wrong_app = configuration();
        wrong_app.app_id = "TEAMID1234.com.attacker.app".to_string();
        assert_eq!(
            verify_app_attestation(
                FIXTURE_ATTESTATION.trim(),
                FIXTURE_CHALLENGE,
                FIXTURE_KEY_ID,
                &wrong_app,
                FIXTURE_ROOT,
                UnixTime::now(),
            )
            .unwrap_err()
            .code,
            "mobile_app_attest_app_id_mismatch"
        );

        let mut wrong_environment = configuration();
        wrong_environment.environment = AppAttestEnvironment::Production;
        assert_eq!(
            verify_app_attestation(
                FIXTURE_ATTESTATION.trim(),
                FIXTURE_CHALLENGE,
                FIXTURE_KEY_ID,
                &wrong_environment,
                FIXTURE_ROOT,
                UnixTime::now(),
            )
            .unwrap_err()
            .code,
            "mobile_app_attest_environment_mismatch"
        );

        let mut wrong_category = configuration();
        wrong_category.allowed_validation_categories = BTreeSet::from([4]);
        assert_eq!(
            verify_app_attestation(
                FIXTURE_ATTESTATION.trim(),
                FIXTURE_CHALLENGE,
                FIXTURE_KEY_ID,
                &wrong_category,
                FIXTURE_ROOT,
                UnixTime::now(),
            )
            .unwrap_err()
            .code,
            "mobile_app_attest_validation_category_refused"
        );

        let mut wrong_bundle = configuration();
        wrong_bundle.bundle_version = "2".to_string();
        assert_eq!(
            verify_app_attestation(
                FIXTURE_ATTESTATION.trim(),
                FIXTURE_CHALLENGE,
                FIXTURE_KEY_ID,
                &wrong_bundle,
                FIXTURE_ROOT,
                UnixTime::now(),
            )
            .unwrap_err()
            .code,
            "mobile_app_attest_bundle_version_mismatch"
        );
    }

    #[test]
    fn verified_challenge_is_single_use_receipted_and_restart_durable() {
        let now = now_epoch();
        let owner = identity("founder");
        let mut kernel = registered_kernel("founder");
        let mut service = MobileAppAttestService::new();
        let key_id_bytes = decode_key_id(FIXTURE_KEY_ID).unwrap();
        service.challenges.insert(
            "app_attest_fixture".to_string(),
            AppAttestChallenge {
                tenant_id: "tenant_mdx".to_string(),
                user_id: "founder".to_string(),
                device_id: "iphone_fixture".to_string(),
                key_id: FIXTURE_KEY_ID.to_string(),
                key_id_fingerprint: hex_sha256(&key_id_bytes),
                challenge: FIXTURE_CHALLENGE.to_string(),
                expires_at_epoch: now + 300,
                state: ChallengeState::Pending,
                configuration: configuration(),
            },
        );
        let body = json!({
            "challenge_id": "app_attest_fixture",
            "device_id": "iphone_fixture",
            "key_id": FIXTURE_KEY_ID,
            "attestation_object": FIXTURE_ATTESTATION.trim()
        });
        let receipt = service
            .verify_attestation(&owner, &body, now, &mut kernel, FIXTURE_ROOT)
            .unwrap();
        assert_eq!(receipt["status"], "APP_ATTEST_VERIFIED");
        assert_eq!(receipt["restart_durable"], true);
        assert_eq!(receipt["raw_attestation_recorded"], false);
        assert_eq!(receipt["raw_apple_receipt_recorded"], false);
        assert_eq!(
            service
                .verify_attestation(&owner, &body, now + 1, &mut kernel, FIXTURE_ROOT)
                .unwrap_err()
                .code,
            "mobile_app_attest_challenge_consumed"
        );

        let entries = kernel.ledger().entries().to_vec();
        let mut restarted = MdxKernel::boot_local();
        restarted.restore_ledger_entries(entries).unwrap();
        let device = &restarted
            .mobile_trust_projection("tenant_mdx", "founder")
            .devices[0];
        assert_eq!(device.attestation_risk, "verified_app_attest");
        assert_eq!(
            device.app_attest_environment.as_deref(),
            Some("development")
        );
        assert_eq!(device.app_attest_validation_category, Some(3));
        assert_eq!(device.app_attest_bundle_version.as_deref(), Some("1"));
        assert!(device.app_attest_public_key.is_some());
        assert!(restarted.ledger().verify().is_ok());
    }

    #[test]
    fn malformed_evidence_never_promotes_and_cannot_be_retried() {
        let now = now_epoch();
        let owner = identity("founder");
        let mut kernel = registered_kernel("founder");
        let mut service = MobileAppAttestService::new();
        let key_id_bytes = decode_key_id(FIXTURE_KEY_ID).unwrap();
        service.challenges.insert(
            "app_attest_bad_fixture".to_string(),
            AppAttestChallenge {
                tenant_id: "tenant_mdx".to_string(),
                user_id: "founder".to_string(),
                device_id: "iphone_fixture".to_string(),
                key_id: FIXTURE_KEY_ID.to_string(),
                key_id_fingerprint: hex_sha256(&key_id_bytes),
                challenge: FIXTURE_CHALLENGE.to_string(),
                expires_at_epoch: now + 300,
                state: ChallengeState::Pending,
                configuration: configuration(),
            },
        );
        let body = json!({
            "challenge_id": "app_attest_bad_fixture",
            "device_id": "iphone_fixture",
            "key_id": FIXTURE_KEY_ID,
            "attestation_object": STANDARD.encode("forged")
        });
        assert_eq!(
            service
                .verify_attestation(&owner, &body, now, &mut kernel, FIXTURE_ROOT)
                .unwrap_err()
                .code,
            "mobile_app_attest_object_invalid"
        );
        assert_eq!(
            service
                .verify_attestation(&owner, &body, now + 1, &mut kernel, FIXTURE_ROOT)
                .unwrap_err()
                .code,
            "mobile_app_attest_challenge_failed"
        );
        assert_eq!(
            kernel
                .mobile_trust_projection("tenant_mdx", "founder")
                .devices[0]
                .attestation_risk,
            "unverified"
        );
    }

    #[test]
    fn one_attested_key_cannot_cross_account_boundaries() {
        let now = now_epoch();
        let verified = verify_app_attestation(
            FIXTURE_ATTESTATION.trim(),
            FIXTURE_CHALLENGE,
            FIXTURE_KEY_ID,
            &configuration(),
            FIXTURE_ROOT,
            UnixTime::now(),
        )
        .unwrap();
        let key_id_bytes = decode_key_id(FIXTURE_KEY_ID).unwrap();
        let key_fingerprint = hex_sha256(&key_id_bytes);
        let public_key = STANDARD.encode(verified.public_key);
        let mut kernel = registered_kernel("founder");
        kernel
            .record_mobile_device_attestation(
                MobileDeviceAttestation {
                    tenant_id: "tenant_mdx",
                    actor_id: "founder",
                    device_id: "iphone_fixture",
                    key_id_fingerprint: &key_fingerprint,
                    public_key: &public_key,
                    environment: "development",
                    validation_category: 3,
                    bundle_version: "1",
                    receipt_fingerprint: &verified.receipt_fingerprint,
                    attested_at_epoch: now,
                },
                &GovernedWriteIdentity::local_demo("founder"),
            )
            .unwrap();
        kernel
            .register_mobile_device(
                MobileDeviceRegistration {
                    tenant_id: "tenant_mdx",
                    actor_id: "intruder",
                    device_id: "iphone_fixture",
                    platform: "ios",
                    display_name: "Same physical iPhone, other account",
                    public_key: "other-device-possession-key",
                    public_key_thumbprint: "other-device-possession-thumbprint",
                    registered_at_epoch: now,
                },
                &GovernedWriteIdentity::local_demo("intruder"),
            )
            .unwrap();
        let mut service = MobileAppAttestService::new();
        service.challenges.insert(
            "app_attest_cross_account".to_string(),
            AppAttestChallenge {
                tenant_id: "tenant_mdx".to_string(),
                user_id: "intruder".to_string(),
                device_id: "iphone_fixture".to_string(),
                key_id: FIXTURE_KEY_ID.to_string(),
                key_id_fingerprint: key_fingerprint,
                challenge: FIXTURE_CHALLENGE.to_string(),
                expires_at_epoch: now + 300,
                state: ChallengeState::Pending,
                configuration: configuration(),
            },
        );
        let body = json!({
            "challenge_id": "app_attest_cross_account",
            "device_id": "iphone_fixture",
            "key_id": FIXTURE_KEY_ID,
            "attestation_object": FIXTURE_ATTESTATION.trim()
        });
        assert_eq!(
            service
                .verify_attestation(&identity("intruder"), &body, now, &mut kernel, FIXTURE_ROOT,)
                .unwrap_err()
                .code,
            "mobile_app_attest_key_identity_conflict"
        );
        assert_eq!(
            service.challenges["app_attest_cross_account"].state,
            ChallengeState::Failed
        );
        assert_eq!(
            kernel
                .mobile_trust_projection("tenant_mdx", "intruder")
                .devices[0]
                .attestation_risk,
            "unverified"
        );
    }
}
