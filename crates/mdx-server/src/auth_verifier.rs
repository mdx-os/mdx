//! Concrete trusted-session verifiers for the HTTP serving path.
//!
//! mdx-core defines the verifier contract (`trusted_session_verifier`) and stays
//! dependency-free. The crypto lives here.
//!
//! - local-secure: a signed HS256 JWT carried in the `Authorization: Bearer`
//!   header, verified against a local signing key (a deterministic test key,
//!   never a real credential).
//! - production: an OIDC/JWT RS256 token verified against a configured JWKS,
//!   selected by the token's `kid` through a fail-closed `JwksResolver` - pinned
//!   inline material, or a process-shared rate-limited rotation cache over an
//!   HTTPS endpoint - with configurable claim mappings.
//!
//! A signed bearer token is not a spoofable identity header: anyone can send a
//! header, but only a token validly signed by the configured key/JWKS verifies,
//! and the request body is never identity. Every non-signature check (issuer,
//! audience, expiry, tenant and subject mappings, actor kind, delegation for an
//! agent) runs through the shared mdx-core admission, so the refusal taxonomy is
//! identical across verifiers. local-demo never reaches this path; a mode with no
//! configured verifier fails closed.

use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use mdx_core::{
    AuthProfile, DeploymentMode, TrustedSession, TrustedSessionVerifier, VerifiedClaims,
    VerifierRefusal, admit_verified_claims, precheck_token_presence,
};
use std::collections::HashSet;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Which JWT claim names map to the MDx identity fields. Defaults to the MDx claim
/// names; a production IdP that emits different names is mapped through the
/// `MDX_AUTH_*_CLAIM` environment overrides.
#[derive(Clone, Debug)]
pub(crate) struct ClaimMapping {
    pub tenant: String,
    pub actor: String,
    pub role: String,
    pub kind: String,
    pub subject: String,
    pub delegation: String,
    pub scope: String,
}

impl Default for ClaimMapping {
    fn default() -> Self {
        Self {
            tenant: "tenant".to_string(),
            actor: "actor".to_string(),
            role: "role".to_string(),
            kind: "kind".to_string(),
            subject: "subject".to_string(),
            delegation: "delegation_id".to_string(),
            scope: "scope".to_string(),
        }
    }
}

impl ClaimMapping {
    fn from_env() -> Self {
        let d = Self::default();
        let or = |key: &str, default: String| std::env::var(key).ok().unwrap_or(default);
        Self {
            tenant: or("MDX_AUTH_TENANT_CLAIM", d.tenant),
            actor: or("MDX_AUTH_ACTOR_CLAIM", d.actor),
            role: or("MDX_AUTH_ROLE_CLAIM", d.role),
            kind: or("MDX_AUTH_KIND_CLAIM", d.kind),
            subject: or("MDX_AUTH_SUBJECT_CLAIM", d.subject),
            delegation: or("MDX_AUTH_DELEGATION_CLAIM", d.delegation),
            scope: or("MDX_AUTH_SCOPE_CLAIM", d.scope),
        }
    }
}

/// Everything the verifiers need, gathered once. Built from the environment for
/// the live path, or injected for deterministic tests.
#[derive(Clone)]
pub(crate) struct VerifierConfig {
    pub profile: AuthProfile,
    pub local_secure_key: Option<Vec<u8>>,
    pub jwks: Option<JwkSet>,
    /// A configured JWKS endpoint for live rotation. Used only when inline `jwks`
    /// is absent (inline material is pinned and takes precedence); absent in the
    /// inline and fixture paths.
    pub jwks_url: Option<String>,
    pub mapping: ClaimMapping,
}

impl VerifierConfig {
    pub(crate) fn from_env() -> Self {
        Self {
            profile: auth_profile_from_env(),
            local_secure_key: local_secure_key_from_env(),
            jwks: jwks_from_env(),
            jwks_url: jwks_url_from_env(),
            mapping: ClaimMapping::from_env(),
        }
    }
}

/// The auth profile from the environment. `MDX_AUTH_PROFILE` being set marks the
/// profile configured. The canonical issuer and audience names are
/// `MDX_AUTH_ISSUER` and `MDX_AUTH_AUDIENCE`; the deployed alpha profile used
/// `MDX_AUTH_JWT_ISSUER` and `MDX_AUTH_JWT_AUDIENCE`, so those remain accepted
/// aliases to avoid refusing every otherwise valid production token.
fn auth_profile_from_env() -> AuthProfile {
    let configured = std::env::var("MDX_AUTH_PROFILE")
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if !configured {
        return AuthProfile::default();
    }
    AuthProfile {
        configured: true,
        issuer: env_with_alias("MDX_AUTH_ISSUER", "MDX_AUTH_JWT_ISSUER"),
        audience: env_with_alias("MDX_AUTH_AUDIENCE", "MDX_AUTH_JWT_AUDIENCE"),
    }
}

fn env_with_alias(primary: &str, alias: &str) -> String {
    std::env::var(primary)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var(alias)
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_default()
}

fn env_with_alias_or(primary: &str, alias: &str, default: &str) -> String {
    let value = env_with_alias(primary, alias);
    if value.trim().is_empty() {
        default.to_string()
    } else {
        value
    }
}

/// The local-secure HS256 signing key (a deterministic test key, never a real
/// credential), from `MDX_LOCAL_SECURE_TOKEN_KEY`.
fn local_secure_key_from_env() -> Option<Vec<u8>> {
    std::env::var("MDX_LOCAL_SECURE_TOKEN_KEY")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.into_bytes())
}

/// The production JWKS material, parsed from inline JSON in `MDX_AUTH_JWKS`. This
/// is public key material (no secret), verified locally with no network fetch.
fn jwks_from_env() -> Option<JwkSet> {
    let raw = std::env::var("MDX_AUTH_JWKS").ok()?;
    serde_json::from_str::<JwkSet>(&raw).ok()
}

/// The JWKS endpoint URL for live rotation, from `MDX_AUTH_JWKS_URL`. A JWKS
/// document is public key material, so this carries no credential; it is off
/// unless explicitly configured, and inline `MDX_AUTH_JWKS` takes precedence.
fn jwks_url_from_env() -> Option<String> {
    std::env::var("MDX_AUTH_JWKS_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// A failure to obtain JWKS material. The reason is intentionally opaque: a
/// fetch failure must fail closed without surfacing endpoint internals.
pub(crate) struct JwksFetchError;

/// Where a JWKS document comes from. The cache asks a source for a fresh key set
/// when it needs to rotate; the source is injectable so the rotation and
/// fail-closed behavior are proven against fixtures with no live network.
pub(crate) trait JwksSource: Send + Sync {
    fn fetch(&self) -> Result<JwkSet, JwksFetchError>;
}

/// The decoding key for a `kid` in a key set, if present and usable.
fn key_from(jwks: &JwkSet, kid: &str) -> Option<DecodingKey> {
    jwks.find(kid)
        .and_then(|jwk| DecodingKey::from_jwk(jwk).ok())
}

/// An empty JWKS, the starting point for an endpoint cache before its first fetch.
fn empty_jwks() -> JwkSet {
    serde_json::from_str(r#"{"keys":[]}"#).expect("empty jwks")
}

/// `true` for an `https://` URL with a host. A plaintext JWKS is MITM-forgeable
/// (an attacker who can serve it can mint trusted tokens), so only HTTPS is
/// accepted; a non-HTTPS endpoint is refused and production fails closed.
fn is_https_url(url: &str) -> bool {
    url.starts_with("https://") && url.len() > "https://".len()
}

/// A live JWKS endpoint (`MDX_AUTH_JWKS_URL`). It fetches a public JWKS document
/// over HTTPS and parses it; any transport, body, or parse error fails closed. It
/// is only constructed for a configured HTTPS URL and is never exercised by the
/// fixture tests (no live endpoint). It holds and sends no credential.
pub(crate) struct HttpJwksSource {
    pub url: String,
}

impl JwksSource for HttpJwksSource {
    fn fetch(&self) -> Result<JwkSet, JwksFetchError> {
        let agent = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(5))
            .build();
        let body = agent
            .get(&self.url)
            .call()
            .map_err(|_| JwksFetchError)?
            .into_string()
            .map_err(|_| JwksFetchError)?;
        serde_json::from_str::<JwkSet>(&body).map_err(|_| JwksFetchError)
    }
}

/// At most one JWKS fetch per endpoint per this interval, across all requests.
/// This bounds the rotation refresh so a stream of tokens with unknown kids
/// cannot drive a fetch storm against the IdP.
const JWKS_MIN_REFRESH: std::time::Duration = std::time::Duration::from_secs(60);

/// The rate-limited rotation state for one JWKS endpoint. It is held
/// process-wide and shared across requests (see `SHARED_JWKS`), so the rate
/// limit is honest: a per-request cache could only bound fetches within a single
/// verification. The clock and the fetch are injected into [`Self::signing_key`]
/// so rotation, the rate limit, and the fail-closed refusals are proven
/// deterministically against fixtures.
pub(crate) struct RateLimitedJwks {
    keys: JwkSet,
    last_attempt: Option<Instant>,
}

impl RateLimitedJwks {
    pub(crate) fn empty() -> Self {
        Self {
            keys: empty_jwks(),
            last_attempt: None,
        }
    }

    /// Resolve a `kid`. A key already held never fetches. An unknown `kid`
    /// refreshes from `fetch`, but only if at least `min_interval` has elapsed
    /// since the last attempt - otherwise it fails closed (`SigningKeyUnknown`)
    /// without a fetch. A fetch failure is `JwksFetchFailed`; the attempt is
    /// timestamped either way, so a failing endpoint backs off too.
    pub(crate) fn signing_key(
        &mut self,
        kid: &str,
        now: Instant,
        min_interval: std::time::Duration,
        fetch: impl FnOnce() -> Result<JwkSet, JwksFetchError>,
    ) -> Result<DecodingKey, VerifierRefusal> {
        if let Some(key) = key_from(&self.keys, kid) {
            return Ok(key);
        }
        let due = self
            .last_attempt
            .is_none_or(|t| now.duration_since(t) >= min_interval);
        if !due {
            return Err(VerifierRefusal::SigningKeyUnknown);
        }
        self.last_attempt = Some(now);
        let rotated = fetch().map_err(|_| VerifierRefusal::JwksFetchFailed)?;
        self.keys = rotated;
        key_from(&self.keys, kid).ok_or(VerifierRefusal::SigningKeyUnknown)
    }
}

/// Process-shared rotation caches keyed by endpoint URL, so the rate limit holds
/// across every request and connection - the only honest way to bound fetches.
static SHARED_JWKS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, RateLimitedJwks>>,
> = std::sync::OnceLock::new();

/// Resolve a `kid` through the process-shared, rate-limited cache for `url`,
/// fetching live over HTTPS when a refresh is due. A poisoned lock fails closed.
fn shared_endpoint_signing_key(url: &str, kid: &str) -> Result<DecodingKey, VerifierRefusal> {
    let registry =
        SHARED_JWKS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let mut guard = registry
        .lock()
        .map_err(|_| VerifierRefusal::JwksFetchFailed)?;
    let entry = guard
        .entry(url.to_string())
        .or_insert_with(RateLimitedJwks::empty);
    let source = HttpJwksSource {
        url: url.to_string(),
    };
    entry.signing_key(kid, Instant::now(), JWKS_MIN_REFRESH, || source.fetch())
}

/// How the production verifier resolves a signing key for a token's `kid`.
pub(crate) enum JwksResolver {
    /// Inline material pinned by the operator: select by `kid`, no fetch, no
    /// rotation. Deterministic emergency pinning; an unknown `kid` fails closed.
    Inline(JwkSet),
    /// A configured HTTPS endpoint, resolved through the shared rate-limited cache.
    Endpoint(String),
}

impl JwksResolver {
    pub(crate) fn signing_key(&self, kid: &str) -> Result<DecodingKey, VerifierRefusal> {
        match self {
            Self::Inline(jwks) => key_from(jwks, kid).ok_or(VerifierRefusal::SigningKeyUnknown),
            Self::Endpoint(url) => shared_endpoint_signing_key(url, kid),
        }
    }
}

/// Build the verifier's JWKS resolver. Inline `MDX_AUTH_JWKS` takes precedence (a
/// pinned static set), so an operator can pin a known-good key set even with an
/// endpoint configured. Otherwise a configured HTTPS endpoint rotates; a
/// non-HTTPS endpoint is refused. Otherwise None, and production fails closed.
pub(crate) fn build_jwks_resolver(config: &VerifierConfig) -> Option<JwksResolver> {
    if let Some(jwks) = &config.jwks {
        return Some(JwksResolver::Inline(jwks.clone()));
    }
    // Only an HTTPS endpoint rotates; a non-HTTPS URL is refused, falling through
    // to fail closed (a plaintext JWKS is MITM-forgeable).
    if let Some(url) = config.jwks_url.as_ref().filter(|url| is_https_url(url)) {
        return Some(JwksResolver::Endpoint(url.clone()));
    }
    None
}

/// Current time as an RFC 3339 UTC string, for the live request path. Tests pass a
/// fixed `now` instead so expiry is deterministic.
pub(crate) fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    epoch_to_rfc3339(secs)
}

/// Format epoch seconds as RFC 3339 UTC, dependency-free (Howard Hinnant's
/// civil-from-days). A numeric JWT `exp` is compared to `now` as RFC 3339, the
/// same fixed-width zulu form the rest of the codebase compares lexicographically.
fn epoch_to_rfc3339(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hour, minute, second) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (year + if month <= 2 { 1 } else { 0 }, month, day)
}

/// Map a decoded JWT claim object into the shared `VerifiedClaims`, using the
/// configured claim names. A numeric `exp` becomes an RFC 3339 string. `iss`,
/// `aud`, and `exp` are the standard JWT claim names.
fn claims_from_json(value: &serde_json::Value, mapping: &ClaimMapping) -> VerifiedClaims {
    let s = |key: &str| {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    };
    let exp = value
        .get("exp")
        .and_then(|v| v.as_i64())
        .map(epoch_to_rfc3339)
        .unwrap_or_default();
    let delegation_id = value
        .get(&mapping.delegation)
        .and_then(|v| v.as_str())
        .filter(|v| !v.trim().is_empty())
        .map(|v| v.to_string());
    let authority_scope = value
        .get(&mapping.scope)
        .and_then(|v| v.as_str())
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    // Agent delegation claims (agent tokens only). issued_at accepts a numeric
    // JWT `iat` or a string claim; sponsor_scope is the sponsor's attested scope.
    let issued_at = value
        .get("iat")
        .and_then(|v| v.as_i64())
        .map(epoch_to_rfc3339)
        .or_else(|| {
            value
                .get("issued_at")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default();
    let sponsor_scope = value
        .get("sponsor_scope")
        .and_then(|v| v.as_str())
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    VerifiedClaims {
        issuer: s("iss"),
        audience: s("aud"),
        expires_at: exp,
        tenant_id: s(&mapping.tenant),
        actor_id: s(&mapping.actor),
        actor_role: s(&mapping.role),
        actor_kind: s(&mapping.kind),
        subject_actor_id: s(&mapping.subject),
        delegation_id,
        authority_scope,
        sponsor_scope,
        issued_at,
        policy_decision_id: s("policy_decision_id"),
        revoked: value
            .get("revoked")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

/// A local-secure verifier for signed HS256 test tokens.
pub(crate) struct SignedTestTokenVerifier {
    profile: AuthProfile,
    key: Vec<u8>,
    mapping: ClaimMapping,
}

impl TrustedSessionVerifier for SignedTestTokenVerifier {
    fn verify(&self, token: Option<&str>, now: &str) -> Result<TrustedSession, VerifierRefusal> {
        let token = precheck_token_presence(&self.profile, token)?;
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = false;
        validation.validate_aud = false;
        validation.required_spec_claims = HashSet::new();
        let data =
            decode::<serde_json::Value>(token, &DecodingKey::from_secret(&self.key), &validation)
                .map_err(|_| VerifierRefusal::MalformedToken)?;
        let claims = claims_from_json(&data.claims, &self.mapping);
        admit_verified_claims(&self.profile, &claims, now)
    }
}

/// A production verifier for asymmetric OIDC tokens verified against a configured
/// JWKS. RS256 and ES256 are accepted; symmetric and unsecured algorithms are
/// refused before decoding.
/// The signing key is selected by the token's `kid` through a rotation cache: a
/// `kid` the cache does not hold triggers one refresh from the JWKS source. An
/// unparseable header or missing kid is a malformed token; a fetch failure or a
/// `kid` unknown after refresh is its own fail-closed refusal.
pub(crate) struct ProductionOidcVerifier {
    profile: AuthProfile,
    jwks: JwksResolver,
    mapping: ClaimMapping,
}

impl TrustedSessionVerifier for ProductionOidcVerifier {
    fn verify(&self, token: Option<&str>, now: &str) -> Result<TrustedSession, VerifierRefusal> {
        let token = precheck_token_presence(&self.profile, token)?;
        let header = decode_header(token).map_err(|_| VerifierRefusal::MalformedToken)?;
        let kid = header.kid.ok_or(VerifierRefusal::MalformedToken)?;
        let algorithm = match header.alg {
            Algorithm::RS256 => Algorithm::RS256,
            Algorithm::ES256 => Algorithm::ES256,
            _ => return Err(VerifierRefusal::MalformedToken),
        };
        // The signing key, with one rotation refresh on a miss. Fail-closed on a
        // fetch failure or an unknown key.
        let key = self.jwks.signing_key(&kid)?;
        let mut validation = Validation::new(algorithm);
        validation.validate_exp = false;
        validation.validate_aud = false;
        validation.required_spec_claims = HashSet::new();
        let data = decode::<serde_json::Value>(token, &key, &validation)
            .map_err(|_| VerifierRefusal::MalformedToken)?;
        let claims = claims_from_json(&data.claims, &self.mapping);
        admit_verified_claims(&self.profile, &claims, now)
    }
}

/// The `Authorization: Bearer <token>` value from the raw request headers, if
/// present. Only the header block before the blank line is considered; a body
/// line named Authorization is not token material. Duplicate Authorization
/// headers are malformed and fail closed.
pub(crate) fn extract_bearer_token(request: &str) -> Result<Option<&str>, VerifierRefusal> {
    let mut found = None;
    for line in request.lines().take_while(|line| !line.trim().is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if !name.trim().eq_ignore_ascii_case("authorization") {
            continue;
        }
        if found.is_some() {
            return Err(VerifierRefusal::MalformedToken);
        }
        let value = value.trim();
        found = value
            .strip_prefix("Bearer ")
            .or_else(|| value.strip_prefix("bearer "))
            .map(str::trim);
        if found.is_none() {
            return Err(VerifierRefusal::MalformedToken);
        }
    }
    Ok(found)
}

/// The verification outcome for a connection: a verified session, or the refusal
/// code for the 401 body. local-demo returns neither (it does not use this path).
pub(crate) struct ConnectionVerification {
    pub session: Option<TrustedSession>,
    pub refusal_code: Option<&'static str>,
}

/// Verify a connection's bearer token under the deployment mode, reading the
/// verifier configuration from the environment.
pub(crate) fn verify_for_connection(
    mode: DeploymentMode,
    request: &str,
    now: &str,
) -> ConnectionVerification {
    verify_for_connection_with(mode, request, now, &VerifierConfig::from_env())
}

/// The pure core of [`verify_for_connection`] with the configuration injected, so
/// tests exercise the full path with a minted token and no process environment.
/// A mode with no configured verifier fails closed; local-demo never reaches here.
pub(crate) fn verify_for_connection_with(
    mode: DeploymentMode,
    request: &str,
    now: &str,
    config: &VerifierConfig,
) -> ConnectionVerification {
    let none = ConnectionVerification {
        session: None,
        refusal_code: None,
    };
    if !mode.requires_trusted_session() {
        return none;
    }
    let token = match extract_bearer_token(request) {
        Ok(token) => token,
        Err(refusal) => {
            return ConnectionVerification {
                session: None,
                refusal_code: Some(refusal.code()),
            };
        }
    };
    let result = match mode {
        DeploymentMode::LocalSecure => match &config.local_secure_key {
            Some(key) => SignedTestTokenVerifier {
                profile: config.profile.clone(),
                key: key.clone(),
                mapping: config.mapping.clone(),
            }
            .verify(token, now),
            // No signing key configured: fail closed, do not fall open.
            None => Err(VerifierRefusal::UnconfiguredAuthProfile),
        },
        DeploymentMode::Production => match build_jwks_resolver(config) {
            Some(jwks) if config.profile.configured => ProductionOidcVerifier {
                profile: config.profile.clone(),
                jwks,
                mapping: config.mapping.clone(),
            }
            .verify(token, now),
            // No JWKS material or HTTPS endpoint, or no profile: production fails
            // closed. The explicit blocker is configured JWKS material (inline or
            // an HTTPS URL) plus an auth profile.
            _ => Err(VerifierRefusal::UnconfiguredAuthProfile),
        },
        DeploymentMode::LocalDemo => return none,
    };
    match result {
        Ok(session) => ConnectionVerification {
            session: Some(session),
            refusal_code: None,
        },
        Err(refusal) => ConnectionVerification {
            session: None,
            refusal_code: Some(refusal.code()),
        },
    }
}

/// Mint a signed HS256 local-secure token. This is dev/test tooling: it requires
/// the symmetric local-secure signing key, so it cannot mint production tokens
/// (production verifies RS256 against a public JWKS, and the server never holds
/// the private key). Used by tests and the `mint-local-secure-token` CLI.
pub(crate) fn mint_local_secure_token(key: &[u8], claims: &serde_json::Value) -> String {
    use jsonwebtoken::{EncodingKey, Header};
    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(key),
    )
    .expect("mint token")
}

/// The CLI `mint-local-secure-token` command: prints a signed local-secure token
/// for an operator to exercise the verifier path. Reads the signing key, issuer,
/// and audience from the environment and the identity claims from the
/// `MDX_TOKEN_*` variables (with sensible local defaults). The token expires one
/// hour from now.
pub(crate) fn mint_local_secure_token_cli() -> Result<String, String> {
    let key = local_secure_key_from_env()
        .ok_or_else(|| "MDX_LOCAL_SECURE_TOKEN_KEY is required to mint a token".to_string())?;
    let env_or =
        |key: &str, default: &str| std::env::var(key).unwrap_or_else(|_| default.to_string());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let claims = serde_json::json!({
        "iss": env_with_alias_or("MDX_AUTH_ISSUER", "MDX_AUTH_JWT_ISSUER", "mdx-local-issuer"),
        "aud": env_with_alias_or("MDX_AUTH_AUDIENCE", "MDX_AUTH_JWT_AUDIENCE", "mdx"),
        "exp": now + 3600,
        "iat": now,
        "tenant": env_or("MDX_TOKEN_TENANT", "local_tenant"),
        "actor": env_or("MDX_TOKEN_ACTOR", "local_user"),
        "role": env_or("MDX_TOKEN_ROLE", "owner"),
        "kind": env_or("MDX_TOKEN_KIND", "human"),
        "subject": env_or("MDX_TOKEN_SUBJECT", &env_or("MDX_TOKEN_ACTOR", "local_user")),
    });
    Ok(mint_local_secure_token(&key, &claims))
}
