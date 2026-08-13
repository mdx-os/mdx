use crate::{RouteResponse, forge_repo_route, request_security};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use mdx_core::{ForgeRunEvent, ForgeRunEventReport, GovernedWriteIdentity, MdxKernel, Receipt};
use ring::rand::{SecureRandom, SystemRandom};
use ring::{digest, hmac};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

const INSTALL_SESSION_LIFETIME_SECONDS: u64 = 10 * 60;
const MAX_INSTALL_SESSIONS: usize = 4_096;
const MAX_REPOSITORIES: usize = 100;
const GITHUB_API_VERSION: &str = "2022-11-28";

#[derive(Clone, Deserialize, Serialize)]
struct InstallationSession {
    tenant_id: String,
    actor_id: String,
    expires_at_epoch: u64,
}

static INSTALL_SESSIONS: OnceLock<Mutex<BTreeMap<String, InstallationSession>>> = OnceLock::new();

fn install_sessions() -> &'static Mutex<BTreeMap<String, InstallationSession>> {
    INSTALL_SESSIONS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GitHubInstallation {
    installation_id: u64,
    account_login: String,
    account_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GitHubRepository {
    repository_id: u64,
    full_name: String,
    display_name: String,
    default_branch: String,
    private: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PreparedCheckout {
    root: PathBuf,
    source_revision: String,
}

trait GitHubAppProvider {
    fn verify_installation(&self, installation_id: u64) -> Result<GitHubInstallation, String>;
    fn repositories(&self, installation_id: u64) -> Result<Vec<GitHubRepository>, String>;
    fn checkout(
        &self,
        tenant_id: &str,
        installation_id: u64,
        repository: &GitHubRepository,
    ) -> Result<PreparedCheckout, String>;
}

struct LiveGitHubAppProvider;

#[derive(Serialize)]
struct GitHubAppClaims {
    iat: usize,
    exp: usize,
    iss: String,
}

impl LiveGitHubAppProvider {
    fn app_jwt(&self) -> Result<String, String> {
        let app_id = std::env::var("MDX_GITHUB_APP_ID")
            .map_err(|_| "MDX_GITHUB_APP_ID is required".to_string())?;
        let raw_key = std::env::var("MDX_GITHUB_APP_PRIVATE_KEY_PEM")
            .map_err(|_| "MDX_GITHUB_APP_PRIVATE_KEY_PEM is required".to_string())?;
        let key = raw_key.replace("\\n", "\n");
        let now = now_epoch();
        let claims = GitHubAppClaims {
            iat: now.saturating_sub(30) as usize,
            exp: (now + 9 * 60) as usize,
            iss: app_id,
        };
        encode(
            &Header::new(Algorithm::RS256),
            &claims,
            &EncodingKey::from_rsa_pem(key.as_bytes())
                .map_err(|error| format!("GitHub App private key is invalid: {error}"))?,
        )
        .map_err(|error| format!("GitHub App JWT mint failed: {error}"))
    }

    fn installation_token(
        &self,
        installation_id: u64,
        repository_id: Option<u64>,
    ) -> Result<String, String> {
        let jwt = self.app_jwt()?;
        let url =
            format!("https://api.github.com/app/installations/{installation_id}/access_tokens");
        let mut body = json!({
            "permissions": {
                "metadata": "read",
                "contents": "read",
                "pull_requests": "write",
                "checks": "read"
            }
        });
        if let Some(repository_id) = repository_id {
            body["repository_ids"] = json!([repository_id]);
        }
        let response = github_request(ureq::post(&url), &jwt)
            .send_json(body)
            .map_err(github_error)?;
        let value: Value = response
            .into_json()
            .map_err(|error| format!("GitHub token response was invalid: {error}"))?;
        value["token"]
            .as_str()
            .filter(|token| !token.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| "GitHub installation token response omitted token".to_string())
    }
}

impl GitHubAppProvider for LiveGitHubAppProvider {
    fn verify_installation(&self, installation_id: u64) -> Result<GitHubInstallation, String> {
        let jwt = self.app_jwt()?;
        let url = format!("https://api.github.com/app/installations/{installation_id}");
        let response = github_request(ureq::get(&url), &jwt)
            .call()
            .map_err(github_error)?;
        let value: Value = response
            .into_json()
            .map_err(|error| format!("GitHub installation response was invalid: {error}"))?;
        let account_login = value["account"]["login"]
            .as_str()
            .unwrap_or("")
            .trim()
            .to_string();
        if account_login.is_empty() {
            return Err("GitHub installation has no account login".to_string());
        }
        Ok(GitHubInstallation {
            installation_id,
            account_login,
            account_type: value["account"]["type"]
                .as_str()
                .unwrap_or("Unknown")
                .to_string(),
        })
    }

    fn repositories(&self, installation_id: u64) -> Result<Vec<GitHubRepository>, String> {
        let token = self.installation_token(installation_id, None)?;
        let response = github_request(
            ureq::get("https://api.github.com/installation/repositories?per_page=100"),
            &token,
        )
        .call()
        .map_err(github_error)?;
        let value: Value = response
            .into_json()
            .map_err(|error| format!("GitHub repository response was invalid: {error}"))?;
        let repositories = value["repositories"]
            .as_array()
            .ok_or_else(|| "GitHub repository response omitted repositories".to_string())?;
        repositories
            .iter()
            .take(MAX_REPOSITORIES)
            .map(parse_github_repository)
            .collect()
    }

    fn checkout(
        &self,
        tenant_id: &str,
        installation_id: u64,
        repository: &GitHubRepository,
    ) -> Result<PreparedCheckout, String> {
        let token = self.installation_token(installation_id, Some(repository.repository_id))?;
        checkout_with_installation_token(tenant_id, repository, &token)
    }
}

fn github_request(request: ureq::Request, credential: &str) -> ureq::Request {
    request
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", GITHUB_API_VERSION)
        .set("User-Agent", "MDx-Forge-Anywhere")
        .set("Authorization", &format!("Bearer {credential}"))
}

fn github_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(status, _) => format!("GitHub App request returned HTTP {status}"),
        ureq::Error::Transport(error) => format!("GitHub App request failed: {error}"),
    }
}

fn parse_github_repository(value: &Value) -> Result<GitHubRepository, String> {
    let repository_id = value["id"]
        .as_u64()
        .ok_or_else(|| "GitHub repository omitted id".to_string())?;
    let full_name = value["full_name"].as_str().unwrap_or("").trim().to_string();
    if !valid_repository_name(&full_name) {
        return Err("GitHub repository full_name is invalid".to_string());
    }
    let default_branch = value["default_branch"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    if !forge_repo_route::is_safe_default_branch(&default_branch) {
        return Err("GitHub repository default_branch is missing or invalid".to_string());
    }
    Ok(GitHubRepository {
        repository_id,
        display_name: value["name"].as_str().unwrap_or(&full_name).to_string(),
        default_branch,
        private: value["private"].as_bool().unwrap_or(true),
        full_name,
    })
}

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    let provider = LiveGitHubAppProvider;
    match path_base(path) {
        "/mobile/cloud/github-installation-sessions.json" => Some(begin_installation(method, body)),
        "/mobile/cloud/github-installations.json" => {
            Some(complete_installation(method, body, kernel, &provider))
        }
        "/mobile/cloud/repositories.json" => {
            Some(list_repositories(method, path, kernel, &provider))
        }
        "/mobile/cloud/repository-connections.json" => {
            Some(connect_repository(method, body, kernel, &provider))
        }
        "/mobile/cloud/environments.json" => Some(prepare_environment(method, body, kernel)),
        "/mobile/cloud/setup.json" => Some(setup_projection(method, kernel)),
        "/mobile/cloud/github-webhooks.json" => Some(webhook_response(method, body, "", kernel)),
        _ => None,
    }
}

fn begin_installation(method: &str, body: &str) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("POST") {
        return Ok(method_not_allowed());
    }
    let resolved = request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let install_base = match std::env::var("MDX_GITHUB_APP_INSTALL_URL") {
        Ok(value) if value.starts_with("https://github.com/apps/") => value,
        _ => {
            return Ok(refusal(
                "github_app_not_configured",
                "MDX_GITHUB_APP_INSTALL_URL must name the GitHub App installation page",
            ));
        }
    };
    let state = random_state()?;
    let now = now_epoch();
    let session = InstallationSession {
        tenant_id: resolved.tenant_id,
        actor_id: resolved.actor_id,
        expires_at_epoch: now + INSTALL_SESSION_LIFETIME_SECONDS,
    };
    if let Err(error) = store_installation_session(&state, &session) {
        return Ok(refusal(
            "github_installation_state_store_unavailable",
            &error,
        ));
    }
    let separator = if install_base.contains('?') { '&' } else { '?' };
    let install_url = format!(
        "{install_base}{separator}state={}",
        url::form_urlencoded::byte_serialize(state.as_bytes()).collect::<String>()
    );
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-cloud-github-installation-session",
            "status": "INSTALLATION_REQUIRED",
            "install_url": install_url,
            "state": state,
            "expires_at_epoch": now + INSTALL_SESSION_LIFETIME_SECONDS,
            "requested_permissions": ["metadata:read", "contents:read", "pull_requests:write", "checks:read"],
            "organization_administration_requested": false,
            "workflow_write_requested": false,
            "personal_access_token_requested": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn complete_installation(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
    provider: &dyn GitHubAppProvider,
) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("POST") {
        return Ok(method_not_allowed());
    }
    let parsed: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => {
            return Ok(refusal(
                "github_installation_shape_invalid",
                "body must be JSON",
            ));
        }
    };
    let state = string_value(&parsed, "state");
    let installation_id = parsed["installation_id"].as_u64().unwrap_or(0);
    if state.len() < 32 || installation_id == 0 {
        return Ok(refusal(
            "github_installation_shape_invalid",
            "state and installation_id are required",
        ));
    }
    let Some(session) = consume_installation_session(&state)? else {
        return Ok(refusal(
            "github_installation_state_invalid",
            "installation state is unknown or already consumed",
        ));
    };
    let now = now_epoch();
    if session.expires_at_epoch <= now {
        return Ok(refusal(
            "github_installation_state_expired",
            "installation state expired; begin again",
        ));
    }
    let resolved = request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    if session.tenant_id != resolved.tenant_id || session.actor_id != resolved.actor_id {
        return Ok(refusal(
            "github_installation_identity_mismatch",
            "installation state belongs to another user or tenant",
        ));
    }
    let installation = match provider.verify_installation(installation_id) {
        Ok(value) => value,
        Err(error) => {
            let _ = store_installation_session(&state, &session);
            return Ok(refusal("github_installation_verification_failed", &error));
        }
    };
    let installation_id_text = installation.installation_id.to_string();
    let report = record_cloud_event(
        kernel,
        (&resolved.tenant_id, &resolved.actor_id, &resolved.identity),
        &format!("forge_run_cloud_installation_{installation_id}"),
        "mobile_cloud_installation",
        "GitHub App installation verified",
        &[
            ("cloud_record_kind", "github_installation"),
            ("github_installation_id", installation_id_text.as_str()),
            ("github_account_login", installation.account_login.as_str()),
            ("github_account_type", installation.account_type.as_str()),
            ("github_installation_status", "active"),
            ("github_token_recorded", "false"),
        ],
    )?;
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-cloud-github-installation",
            "status": "CONNECTED",
            "installation_id": installation.installation_id,
            "account_login": installation.account_login,
            "account_type": installation.account_type,
            "installation_receipt_id": report.receipt_id,
            "token_recorded": false,
            "repository_selection_scope": "github_app_installation",
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn list_repositories(
    method: &str,
    path: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
    provider: &dyn GitHubAppProvider,
) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("GET") {
        return Ok(method_not_allowed());
    }
    let installation_id = query_value(path, "installation_id")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let verified = request_security::current_verified_identity();
    let tenant_id = verified
        .as_ref()
        .map(|identity| identity.tenant_id.as_str())
        .unwrap_or("local_tenant");
    if installation_id == 0 || !installation_belongs_to(kernel, tenant_id, installation_id)? {
        return Ok(refusal(
            "github_installation_not_available",
            "select a verified GitHub App installation",
        ));
    }
    let repositories = match provider.repositories(installation_id) {
        Ok(values) => values,
        Err(error) => return Ok(refusal("github_repository_list_failed", &error)),
    };
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-cloud-repositories",
            "status": "OK",
            "installation_id": installation_id,
            "repositories": repositories.iter().map(repository_json).collect::<Vec<_>>(),
            "repository_count": repositories.len(),
            "token_recorded": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn connect_repository(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
    provider: &dyn GitHubAppProvider,
) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("POST") {
        return Ok(method_not_allowed());
    }
    if body.len() > 64 * 1024 {
        return Ok(refusal(
            "github_webhook_payload_too_large",
            "installation lifecycle payload exceeds the bounded webhook limit",
        ));
    }
    let parsed: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => {
            return Ok(refusal(
                "cloud_repository_shape_invalid",
                "body must be JSON",
            ));
        }
    };
    let installation_id = parsed["installation_id"].as_u64().unwrap_or(0);
    let repository_id = parsed["repository_id"].as_u64().unwrap_or(0);
    let resolved = request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    if installation_id == 0
        || repository_id == 0
        || !installation_belongs_to(kernel, &resolved.tenant_id, installation_id)?
    {
        return Ok(refusal(
            "cloud_repository_not_available",
            "repository must belong to a verified installation in this tenant",
        ));
    }
    let repositories = match provider.repositories(installation_id) {
        Ok(values) => values,
        Err(error) => return Ok(refusal("github_repository_list_failed", &error)),
    };
    let Some(repository) = repositories
        .into_iter()
        .find(|repository| repository.repository_id == repository_id)
    else {
        return Ok(refusal(
            "cloud_repository_scope_denied",
            "repository is not selected in this GitHub App installation",
        ));
    };
    let checkout = match provider.checkout(&resolved.tenant_id, installation_id, &repository) {
        Ok(value) => value,
        Err(error) => return Ok(refusal("cloud_repository_checkout_failed", &error)),
    };
    let repo_id = format!("github_{}", repository.repository_id);
    let origin = format!("https://github.com/{}.git", repository.full_name);
    let connected = forge_repo_route::connect_prepared_repo(
        &resolved,
        forge_repo_route::PreparedRepo {
            repo_id: &repo_id,
            label: &repository.display_name,
            root: &checkout.root.to_string_lossy(),
            kind: "remote",
            origin: &origin,
            default_branch_hint: &repository.default_branch,
        },
        kernel,
    )?;
    let connected_value: Value = serde_json::from_str(&connected.body).unwrap_or_default();
    if connected_value["status"].as_str() != Some("CONNECTED") {
        return Ok(connected);
    }
    let installation_id_text = installation_id.to_string();
    let repository_id_text = repository.repository_id.to_string();
    let report = record_cloud_event(
        kernel,
        (&resolved.tenant_id, &resolved.actor_id, &resolved.identity),
        &format!("forge_run_cloud_repo_{}", repository.repository_id),
        "mobile_cloud_repository",
        "Selected GitHub repository connected to a managed checkout",
        &[
            ("cloud_record_kind", "cloud_repository"),
            ("github_installation_id", installation_id_text.as_str()),
            ("github_repository_id", repository_id_text.as_str()),
            ("github_repository_full_name", repository.full_name.as_str()),
            ("github_default_branch", repository.default_branch.as_str()),
            ("cloud_repo_id", repo_id.as_str()),
            ("cloud_source_revision", checkout.source_revision.as_str()),
            ("github_token_recorded", "false"),
        ],
    )?;
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-cloud-repository-connection",
            "status": "CONNECTED",
            "repo_id": repo_id,
            "repository": repository_json(&repository),
            "source_revision": checkout.source_revision,
            "repo_receipt_id": connected_value["repo_receipt_id"],
            "repo_index_receipt_id": connected_value["repo_index_receipt_id"],
            "cloud_connection_receipt_id": report.receipt_id,
            "token_recorded": false,
            "next_safe_action": "Prepare and review the cloud environment definition",
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct CloudEnvironmentDefinition {
    schema_version: u64,
    environment_id: String,
    environment_version: u64,
    repository_id: String,
    base_image: String,
    architecture: String,
    setup_commands: Vec<String>,
    proof_commands: Vec<String>,
    service_dependencies: Vec<String>,
    cache_paths: Vec<String>,
    resource_class: String,
    network_allowlist: Vec<String>,
    secret_binding_refs: Vec<String>,
    preview_ports: Vec<u16>,
    retention_hours: u16,
    snapshot_policy: String,
    grants_execution_authority: bool,
}

fn prepare_environment(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("POST") {
        return Ok(method_not_allowed());
    }
    let parsed: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => {
            return Ok(refusal(
                "cloud_environment_shape_invalid",
                "body must be JSON",
            ));
        }
    };
    let action = string_value(&parsed, "action");
    let repo_id = string_value(&parsed, "repo_id");
    if !matches!(action.as_str(), "propose" | "prepare" | "verify") || !valid_identifier(&repo_id) {
        return Ok(refusal(
            "cloud_environment_shape_invalid",
            "action must be propose, prepare, or verify and repo_id is required",
        ));
    }
    let resolved = request_security::resolve_governed_write_identity(
        body,
        "local_tenant",
        "local_user",
        "owner",
    );
    let root = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned while reading cloud repo".to_string())?;
        match kernel.forge_repo_root(&repo_id) {
            Some(root) => PathBuf::from(root),
            None => {
                return Ok(refusal(
                    "cloud_environment_repo_not_connected",
                    "connect the selected GitHub repository first",
                ));
            }
        }
    };
    let root = if managed_cloud_root(&root) {
        root
    } else {
        match ensure_managed_cloud_checkout(kernel, &resolved.tenant_id, &repo_id) {
            Ok(Some(recovered)) => recovered,
            Ok(None) => match forge_repo_route::ensure_managed_remote_checkout(kernel, &repo_id) {
                Ok(Some(recovered)) => recovered,
                Ok(None) => root,
                Err(error) => {
                    return Ok(refusal(
                        "cloud_environment_repo_recovery_failed",
                        &format!("connected repository checkout could not be recovered: {error}"),
                    ));
                }
            },
            Err(error) => {
                return Ok(refusal(
                    "cloud_environment_repo_recovery_failed",
                    &format!("connected repository checkout could not be recovered: {error}"),
                ));
            }
        }
    };
    if !managed_cloud_root(&root) {
        return Ok(refusal(
            "cloud_environment_repo_not_managed",
            "cloud environments can only be prepared from an MDx-managed checkout",
        ));
    }
    let secret_binding_refs = match parsed.get("secret_binding_refs") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(values)) if values.len() <= 32 && values.iter().all(Value::is_string) => {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        }
        _ => {
            return Ok(refusal(
                "cloud_environment_secret_ref_invalid",
                "secret_binding_refs must be a bounded array of opaque references",
            ));
        }
    };
    if secret_binding_refs
        .iter()
        .any(|value| !valid_secret_ref(value))
    {
        return Ok(refusal(
            "cloud_environment_secret_ref_invalid",
            "secrets must be opaque secret:// references, never values",
        ));
    }
    let definition = match environment_definition(&root, &repo_id, secret_binding_refs) {
        Ok(value) => value,
        Err(error) => return Ok(refusal("cloud_environment_definition_invalid", &error)),
    };
    let definition_value = serde_json::to_value(&definition)
        .map_err(|error| format!("environment serialization failed: {error}"))?;
    let definition_json = serde_json::to_string(&definition_value)
        .map_err(|error| format!("environment encode failed: {error}"))?;
    let fingerprint = digest_hex(
        &serde_json::to_vec(&definition_value)
            .map_err(|error| format!("environment fingerprint failed: {error}"))?,
    );
    let base_image_ready =
        definition
            .base_image
            .rsplit_once("@sha256:")
            .is_some_and(|(name, value)| {
                !name.is_empty()
                    && value.len() == 64
                    && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            });
    let mut verification = None;
    let mut verification_failure = None;
    let status = if action == "verify" {
        if !base_image_ready || definition.architecture != "linux/arm64" {
            return Ok(refusal(
                "cloud_environment_runtime_incompatible",
                "hosted verification requires a digest-pinned linux/arm64 runtime image",
            ));
        }
        match crate::mobile_hosted_sandbox::verify_environment(&root) {
            Ok(result) if result.environment_fingerprint == fingerprint => {
                verification = Some(result);
                "VERIFIED"
            }
            Ok(_) => {
                verification_failure = Some(
                    "hosted environment fingerprint did not match the prepared definition"
                        .to_string(),
                );
                "VERIFICATION_FAILED"
            }
            Err(error) => {
                verification_failure = Some(error);
                "VERIFICATION_FAILED"
            }
        }
    } else if action == "prepare" && base_image_ready {
        if let Err(error) = persist_environment_definition(&root, &definition) {
            return Ok(refusal("cloud_environment_path_unsafe", &error));
        }
        "PREPARED_VERIFICATION_REQUIRED"
    } else if action == "prepare" {
        "CONFIGURATION_REQUIRED"
    } else {
        "PROPOSED"
    };
    let version_text = definition.environment_version.to_string();
    let verification_source_fingerprint = verification
        .as_ref()
        .map(|result| result.source_fingerprint.as_str())
        .unwrap_or("");
    let verification_environment_fingerprint = verification
        .as_ref()
        .map(|result| result.environment_fingerprint.as_str())
        .unwrap_or("");
    let verification_session_ref = verification
        .as_ref()
        .map(|result| result.session_ref.as_str())
        .unwrap_or("");
    let verification_provider = verification
        .as_ref()
        .map(|result| result.provider)
        .unwrap_or("");
    let verification_evidence_sha256 = verification
        .as_ref()
        .map(|result| result.command_evidence_sha256.as_str())
        .unwrap_or("");
    let verification_setup_count = verification
        .as_ref()
        .map(|result| result.setup_count.to_string())
        .unwrap_or_default();
    let verification_proof_count = verification
        .as_ref()
        .map(|result| result.proof_count.to_string())
        .unwrap_or_default();
    let verification_failure_sha256 = verification_failure
        .as_ref()
        .map(|reason| digest_hex(reason.as_bytes()))
        .unwrap_or_default();
    let report = record_cloud_event(
        kernel,
        (&resolved.tenant_id, &resolved.actor_id, &resolved.identity),
        &format!("forge_run_cloud_environment_{}", &fingerprint[..24]),
        "mobile_cloud_environment",
        "Cloud environment definition recorded",
        &[
            ("cloud_record_kind", "cloud_environment"),
            ("cloud_environment_id", definition.environment_id.as_str()),
            ("cloud_environment_version", version_text.as_str()),
            ("cloud_environment_repo_id", repo_id.as_str()),
            ("cloud_environment_fingerprint", fingerprint.as_str()),
            (
                "cloud_environment_definition_json",
                definition_json.as_str(),
            ),
            ("cloud_environment_status", status),
            ("cloud_environment_secret_values_recorded", "false"),
            ("cloud_environment_execution_authority", "closed"),
            ("cloud_sandbox_provider", verification_provider),
            (
                "cloud_verification_source_fingerprint",
                verification_source_fingerprint,
            ),
            (
                "cloud_verification_environment_fingerprint",
                verification_environment_fingerprint,
            ),
            ("cloud_verification_session_ref", verification_session_ref),
            (
                "cloud_verification_command_evidence_sha256",
                verification_evidence_sha256,
            ),
            ("cloud_verification_setup_count", &verification_setup_count),
            ("cloud_verification_proof_count", &verification_proof_count),
            (
                "cloud_verification_failure_sha256",
                &verification_failure_sha256,
            ),
            ("cloud_verification_secret_values_recorded", "false"),
        ],
    )?;
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-cloud-environment",
            "status": status,
            "environment": definition_value,
            "environment_fingerprint": fingerprint,
            "environment_receipt_id": report.receipt_id,
            "definition_path": ".mdx/environment.json",
            "base_image_digest_required": !base_image_ready,
            "verification_required": status != "VERIFIED",
            "ready_for_cloud_builds": status == "VERIFIED",
            "secret_values_recorded": false,
            "hosted_sandbox_provider": verification_provider,
            "source_fingerprint": verification_source_fingerprint,
            "command_evidence_sha256": verification_evidence_sha256,
            "next_safe_action": if status == "VERIFIED" { "Start a governed build on MDx Cloud" } else if status == "VERIFICATION_FAILED" { "Review the hosted runtime configuration and retry verification" } else if base_image_ready { "Verify setup and proof commands in the hosted sandbox" } else { "Configure a pinned MDX_CLOUD_BASE_IMAGE before preparing" },
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn environment_definition(
    root: &Path,
    repo_id: &str,
    secret_binding_refs: Vec<String>,
) -> Result<CloudEnvironmentDefinition, String> {
    let existing = root.join(".mdx/environment.json");
    if existing.exists() {
        let metadata = std::fs::symlink_metadata(&existing)
            .map_err(|error| format!("existing environment metadata failed: {error}"))?;
        if metadata.file_type().is_symlink() || metadata.len() > 128 * 1024 {
            return Err(
                "existing .mdx/environment.json must be a bounded regular file".to_string(),
            );
        }
        let bytes = std::fs::read(&existing)
            .map_err(|error| format!("existing environment read failed: {error}"))?;
        let mut definition: CloudEnvironmentDefinition = serde_json::from_slice(&bytes)
            .map_err(|error| format!("existing .mdx/environment.json is invalid: {error}"))?;
        validate_environment_definition(&definition, repo_id)?;
        refresh_managed_base_image(root, &mut definition, &configured_cloud_base_image())?;
        return Ok(definition);
    }
    let base_image = configured_cloud_base_image();
    let (setup_commands, proof_commands, cache_paths, network_allowlist) =
        infer_environment_commands(root);
    let definition = CloudEnvironmentDefinition {
        schema_version: 1,
        environment_id: format!("cloud_env_{repo_id}"),
        environment_version: 1,
        repository_id: repo_id.to_string(),
        base_image,
        architecture: "linux/arm64".to_string(),
        setup_commands,
        proof_commands,
        service_dependencies: Vec::new(),
        cache_paths,
        resource_class: "medium".to_string(),
        network_allowlist,
        secret_binding_refs,
        preview_ports: Vec::new(),
        retention_hours: 24,
        snapshot_policy: "after_verified_setup".to_string(),
        grants_execution_authority: false,
    };
    validate_environment_definition(&definition, repo_id)?;
    Ok(definition)
}

fn configured_cloud_base_image() -> String {
    std::env::var("MDX_CLOUD_BASE_IMAGE")
        .unwrap_or_else(|_| "public.ecr.aws/amazonlinux/amazonlinux:2023".to_string())
}

fn persist_environment_definition(
    root: &Path,
    definition: &CloudEnvironmentDefinition,
) -> Result<(), String> {
    let environment_dir = root.join(".mdx");
    if std::fs::symlink_metadata(&environment_dir)
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(".mdx must be a real directory inside the managed checkout".to_string());
    }
    std::fs::create_dir_all(&environment_dir)
        .map_err(|error| format!("environment directory create failed: {error}"))?;
    let definition_path = environment_dir.join("environment.json");
    if std::fs::symlink_metadata(&definition_path)
        .is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(".mdx/environment.json must not be a symbolic link".to_string());
    }
    let mut encoded = serde_json::to_string_pretty(definition)
        .map_err(|error| format!("environment encode failed: {error}"))?;
    encoded.push('\n');
    std::fs::write(definition_path, encoded)
        .map_err(|error| format!("environment write failed: {error}"))
}

fn refresh_managed_base_image(
    root: &Path,
    definition: &mut CloudEnvironmentDefinition,
    configured_base_image: &str,
) -> Result<(), String> {
    if definition.base_image == configured_base_image {
        return Ok(());
    }
    if tracked_environment_definition(root)? {
        return Err(
            "committed .mdx/environment.json must be updated explicitly for the deployed runtime image"
                .to_string(),
        );
    }
    definition.base_image = configured_base_image.to_string();
    definition.environment_version = definition
        .environment_version
        .checked_add(1)
        .ok_or_else(|| "cloud environment version cannot advance".to_string())?;
    validate_environment_definition(definition, &definition.repository_id)
}

fn tracked_environment_definition(root: &Path) -> Result<bool, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "--error-unmatch", "--", ".mdx/environment.json"])
        .output()
        .map_err(|error| format!("environment ownership check failed: {error}"))?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err("environment ownership check did not complete".to_string()),
    }
}

fn validate_environment_definition(
    definition: &CloudEnvironmentDefinition,
    repo_id: &str,
) -> Result<(), String> {
    if definition.schema_version != 1
        || definition.environment_version == 0
        || definition.repository_id != repo_id
        || !valid_identifier(&definition.environment_id)
        || definition.grants_execution_authority
        || !matches!(
            definition.architecture.as_str(),
            "linux/amd64" | "linux/arm64"
        )
        || !matches!(
            definition.resource_class.as_str(),
            "small" | "medium" | "large"
        )
        || !matches!(
            definition.snapshot_policy.as_str(),
            "never" | "after_verified_setup" | "manual"
        )
        || definition.retention_hours == 0
        || definition.retention_hours > 168
        || definition.base_image.is_empty()
        || definition.base_image.len() > 300
        || definition
            .base_image
            .bytes()
            .any(|byte| byte.is_ascii_whitespace())
        || definition.setup_commands.len() > 20
        || definition.proof_commands.is_empty()
        || definition.proof_commands.len() > 20
        || definition.service_dependencies.len() > 16
        || definition.cache_paths.len() > 20
        || definition.network_allowlist.len() > 32
        || definition.secret_binding_refs.len() > 32
        || definition
            .setup_commands
            .iter()
            .chain(definition.proof_commands.iter())
            .any(|command| {
                command.is_empty()
                    || command.len() > 500
                    || command
                        .bytes()
                        .any(|byte| matches!(byte, b'\n' | b'\r' | b'\0'))
            })
        || definition
            .service_dependencies
            .iter()
            .any(|value| !valid_identifier(value))
        || definition
            .cache_paths
            .iter()
            .any(|value| !valid_mutable_path_pattern(value))
        || definition.preview_ports.len() > 16
        || definition.preview_ports.contains(&0)
        || definition
            .secret_binding_refs
            .iter()
            .any(|value| !valid_secret_ref(value))
        || definition
            .network_allowlist
            .iter()
            .any(|value| !valid_network_domain(value))
    {
        return Err("environment definition violates the bounded cloud contract".to_string());
    }
    Ok(())
}

fn valid_network_domain(value: &str) -> bool {
    if value.is_empty()
        || value.len() > 253
        || value.parse::<std::net::IpAddr>().is_ok()
        || value.bytes().any(|byte| {
            !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.'))
        })
    {
        return false;
    }
    let labels = value.split('.').collect::<Vec<_>>();
    labels.len() >= 2
        && labels.iter().all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

fn valid_mutable_path_pattern(value: &str) -> bool {
    if value.is_empty() || value.len() > 200 || value.contains('\0') || value.contains('\\') {
        return false;
    }
    if let Some(component) = value.strip_prefix("**/") {
        return !component.is_empty()
            && component != "."
            && component != ".."
            && !component.contains('/')
            && !component.contains('*')
            && !reserved_mutable_component(component);
    }
    !Path::new(value).is_absolute()
        && Path::new(value)
            .components()
            .all(|component| match component {
                std::path::Component::Normal(value) => value
                    .to_str()
                    .is_some_and(|value| !reserved_mutable_component(value)),
                _ => false,
            })
}

fn reserved_mutable_component(value: &str) -> bool {
    matches!(
        value,
        ".git"
            | ".mdx"
            | ".ssh"
            | ".aws"
            | ".netrc"
            | ".npmrc"
            | ".pypirc"
            | "id_rsa"
            | "id_ed25519"
            | ".env"
    ) || value.starts_with(".env.")
}

fn inferred_recursive_mutable_paths(root: &Path) -> Vec<String> {
    let Ok(metadata) = std::fs::metadata(root.join(".gitignore")) else {
        return Vec::new();
    };
    if !metadata.is_file() || metadata.len() > 128 * 1024 {
        return Vec::new();
    }
    let Ok(contents) = std::fs::read_to_string(root.join(".gitignore")) else {
        return Vec::new();
    };
    let mut inferred = contents
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("**/") && line.ends_with('/'))
        .filter_map(|line| {
            line.strip_prefix("**/")
                .and_then(|line| line.strip_suffix('/'))
        })
        .filter(|component| {
            !matches!(
                *component,
                ".git"
                    | ".mdx-runtime"
                    | ".worktrees"
                    | "target"
                    | "node_modules"
                    | ".build"
                    | ".swiftpm"
                    | ".venv"
                    | ".ssh"
                    | ".aws"
                    | "secrets"
            ) && !component.starts_with(".env")
        })
        .map(|component| format!("**/{component}"))
        .filter(|value| valid_mutable_path_pattern(value))
        .collect::<Vec<_>>();
    inferred.sort();
    inferred.dedup();
    inferred.truncate(8);
    inferred
}

fn infer_environment_commands(root: &Path) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    let (setup, proof, mut cache_paths, hosts) = if root.join("Cargo.toml").exists() {
        (
            vec!["cargo fetch --locked".to_string()],
            vec!["cargo test --workspace --locked".to_string()],
            vec!["~/.cargo/registry".to_string(), "target".to_string()],
            vec!["crates.io".to_string(), "github.com".to_string()],
        )
    } else if root.join("pnpm-lock.yaml").exists() {
        (
            vec!["pnpm install --frozen-lockfile".to_string()],
            vec!["pnpm test".to_string()],
            vec!["~/.pnpm-store".to_string(), "node_modules".to_string()],
            vec!["registry.npmjs.org".to_string(), "github.com".to_string()],
        )
    } else if root.join("package-lock.json").exists() || root.join("package.json").exists() {
        (
            vec!["npm ci".to_string()],
            vec!["npm test".to_string()],
            vec!["~/.npm".to_string(), "node_modules".to_string()],
            vec!["registry.npmjs.org".to_string(), "github.com".to_string()],
        )
    } else if root.join("uv.lock").exists() {
        (
            vec!["uv sync --frozen".to_string()],
            vec!["uv run pytest".to_string()],
            vec!["~/.cache/uv".to_string(), ".venv".to_string()],
            vec!["pypi.org".to_string(), "files.pythonhosted.org".to_string()],
        )
    } else if root.join("requirements.lock").exists() || root.join("requirements.txt").exists() {
        (
            vec!["python -m pip install -r requirements.txt".to_string()],
            vec!["python -m pytest".to_string()],
            vec!["~/.cache/pip".to_string(), ".venv".to_string()],
            vec!["pypi.org".to_string(), "files.pythonhosted.org".to_string()],
        )
    } else {
        (
            Vec::new(),
            vec!["git status --short".to_string()],
            Vec::new(),
            vec!["github.com".to_string()],
        )
    };
    cache_paths.extend(inferred_recursive_mutable_paths(root));
    cache_paths.sort();
    cache_paths.dedup();
    cache_paths.truncate(20);
    (setup, proof, cache_paths, hosts)
}

fn setup_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("GET") {
        return Ok(method_not_allowed());
    }
    let verified = request_security::current_verified_identity();
    let tenant_id = verified
        .as_ref()
        .map(|identity| identity.tenant_id.as_str())
        .unwrap_or("local_tenant");
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned while projecting cloud setup".to_string())?;
    let mut installations = BTreeMap::<String, Value>::new();
    let mut repositories = BTreeMap::<String, Value>::new();
    let mut environments = BTreeMap::<String, Value>::new();
    let mut source_receipt_ids = BTreeSet::<String>::new();
    for receipt in kernel.ledger().entries().iter().filter(|receipt| {
        receipt.kind == "forge.run.event" && receipt.tenant_id.as_str() == tenant_id
    }) {
        match payload(receipt, "cloud_record_kind") {
            "github_installation" | "github_installation_webhook" => {
                let id = payload(receipt, "github_installation_id");
                if !id.is_empty() {
                    source_receipt_ids.insert(receipt.receipt_id.clone());
                    installations.insert(
                        id.to_string(),
                        json!({
                            "installation_id": id.parse::<u64>().unwrap_or(0),
                            "account_login": payload(receipt, "github_account_login"),
                            "account_type": payload(receipt, "github_account_type"),
                            "status": payload(receipt, "github_installation_status"),
                            "receipt_id": receipt.receipt_id,
                            "token_recorded": false
                        }),
                    );
                }
            }
            "cloud_repository" => {
                let id = payload(receipt, "github_repository_id");
                if !id.is_empty() {
                    source_receipt_ids.insert(receipt.receipt_id.clone());
                    repositories.insert(
                        id.to_string(),
                        json!({
                            "repository_id": id.parse::<u64>().unwrap_or(0),
                            "full_name": payload(receipt, "github_repository_full_name"),
                            "default_branch": payload(receipt, "github_default_branch"),
                            "repo_id": payload(receipt, "cloud_repo_id"),
                            "source_revision": payload(receipt, "cloud_source_revision"),
                            "status": "connected",
                            "receipt_id": receipt.receipt_id,
                            "token_recorded": false
                        }),
                    );
                }
            }
            "cloud_environment" => {
                let id = payload(receipt, "cloud_environment_id");
                if !id.is_empty() {
                    source_receipt_ids.insert(receipt.receipt_id.clone());
                    let status = payload(receipt, "cloud_environment_status");
                    let repo_id = payload(receipt, "cloud_environment_repo_id");
                    let definition_recoverable =
                        verified_environment_definition_from_receipt(receipt, id, repo_id).is_ok();
                    environments.insert(
                        id.to_string(),
                        json!({
                            "environment_id": id,
                            "environment_version": payload(receipt, "cloud_environment_version").parse::<u64>().unwrap_or(0),
                            "repo_id": repo_id,
                            "fingerprint": payload(receipt, "cloud_environment_fingerprint"),
                            "status": status,
                            "ready_for_cloud_builds": status == "VERIFIED" && definition_recoverable,
                            "definition_recoverable": definition_recoverable,
                            "requires_reprepare": status == "VERIFIED" && !definition_recoverable,
                            "receipt_id": receipt.receipt_id,
                            "secret_values_recorded": false
                        }),
                    );
                }
            }
            _ => {}
        }
    }
    let environments = environments.into_values().collect::<Vec<_>>();
    let ready_count = environments
        .iter()
        .filter(|environment| environment["ready_for_cloud_builds"] == true)
        .count();
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-cloud-setup",
            "status": "OK",
            "tenant_id": tenant_id,
            "source_receipt_ids": source_receipt_ids,
            "installations": installations.into_values().collect::<Vec<_>>(),
            "repositories": repositories.into_values().collect::<Vec<_>>(),
            "environments": environments,
            "ready_environment_count": ready_count,
            "github_app_configured": github_app_configured(),
            "hosted_sandbox_status": if crate::mobile_hosted_sandbox::hosted_backend_configured() { "CONFIGURED_PENDING_VERIFICATION" } else { "CONFIGURATION_REQUIRED" },
            "secret_values_included": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

pub(crate) fn require_verified_environment(
    kernel: &MdxKernel,
    tenant_id: &str,
    environment_id: &str,
    repo_id: &str,
) -> Result<(), String> {
    let latest = kernel.ledger().entries().iter().rev().find(|receipt| {
        receipt.kind == "forge.run.event"
            && receipt.tenant_id.as_str() == tenant_id
            && payload(receipt, "cloud_record_kind") == "cloud_environment"
            && payload(receipt, "cloud_environment_id") == environment_id
    });
    let Some(receipt) = latest else {
        return Err(
            "the selected MDx Cloud environment does not exist for this tenant".to_string(),
        );
    };
    verified_environment_definition_from_receipt(receipt, environment_id, repo_id).map(|_| ())
}

fn verified_environment_definition_from_receipt(
    receipt: &Receipt,
    environment_id: &str,
    repo_id: &str,
) -> Result<CloudEnvironmentDefinition, String> {
    if payload(receipt, "cloud_environment_status") != "VERIFIED"
        || payload(receipt, "cloud_environment_repo_id") != repo_id
        || payload(receipt, "cloud_verification_secret_values_recorded") != "false"
    {
        return Err(
            "the selected MDx Cloud environment is not verified for this repository".to_string(),
        );
    }
    let encoded = payload(receipt, "cloud_environment_definition_json");
    if encoded.is_empty() {
        return Err(
            "the verified cloud environment predates durable definition recovery; prepare it once more"
                .to_string(),
        );
    }
    let definition: CloudEnvironmentDefinition = serde_json::from_str(encoded)
        .map_err(|error| format!("durable cloud environment definition is invalid: {error}"))?;
    validate_environment_definition(&definition, repo_id)?;
    if definition.environment_id != environment_id {
        return Err("durable cloud environment identity does not match the receipt".to_string());
    }
    if definition.base_image != configured_cloud_base_image() {
        return Err(
            "durable cloud environment image is stale; prepare and verify the environment again"
                .to_string(),
        );
    }
    let value = serde_json::to_value(&definition)
        .map_err(|error| format!("durable environment serialization failed: {error}"))?;
    let fingerprint = digest_hex(
        &serde_json::to_vec(&value)
            .map_err(|error| format!("durable environment fingerprint failed: {error}"))?,
    );
    if fingerprint != payload(receipt, "cloud_environment_fingerprint") {
        return Err("durable cloud environment fingerprint does not match the receipt".to_string());
    }
    Ok(definition)
}

pub(crate) fn restore_verified_environment_definition(
    kernel: &MdxKernel,
    tenant_id: &str,
    environment_id: &str,
    repo_id: &str,
    root: &Path,
) -> Result<(), String> {
    let definition_path = root.join(".mdx/environment.json");
    if std::fs::symlink_metadata(&definition_path).is_ok() {
        return Err(
            "the existing cloud environment definition could not be loaded and was not replaced"
                .to_string(),
        );
    }
    let receipt = kernel
        .ledger()
        .entries()
        .iter()
        .rev()
        .find(|receipt| {
            receipt.kind == "forge.run.event"
                && receipt.tenant_id.as_str() == tenant_id
                && payload(receipt, "cloud_record_kind") == "cloud_environment"
                && payload(receipt, "cloud_environment_id") == environment_id
        })
        .ok_or_else(|| "the selected MDx Cloud environment does not exist".to_string())?;
    let definition =
        verified_environment_definition_from_receipt(receipt, environment_id, repo_id)?;
    persist_environment_definition(root, &definition)
}

pub(crate) fn webhook_response(
    method: &str,
    body: &str,
    raw_request: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if !method.eq_ignore_ascii_case("POST") {
        return Ok(method_not_allowed());
    }
    let secret = match std::env::var("MDX_GITHUB_WEBHOOK_SECRET") {
        Ok(value) if value.len() >= 32 => value,
        _ => {
            return Ok(refusal(
                "github_webhook_secret_missing",
                "GitHub webhook processing is closed until a webhook secret is configured",
            ));
        }
    };
    let signature = request_header(raw_request, "x-hub-signature-256").unwrap_or("");
    let delivery_id = request_header(raw_request, "x-github-delivery").unwrap_or("");
    let event = request_header(raw_request, "x-github-event").unwrap_or("");
    if !verify_github_signature(&secret, body.as_bytes(), signature)
        || !valid_identifier(delivery_id)
        || event != "installation"
    {
        return Ok(refusal(
            "github_webhook_signature_invalid",
            "webhook signature, delivery id, or event is invalid",
        ));
    }
    let parsed: Value = match serde_json::from_str(body) {
        Ok(value) => value,
        Err(_) => {
            return Ok(refusal(
                "github_webhook_payload_invalid",
                "payload must be JSON",
            ));
        }
    };
    let installation_id = parsed["installation"]["id"].as_u64().unwrap_or(0);
    let action = parsed["action"].as_str().unwrap_or("");
    if installation_id == 0
        || !matches!(
            action,
            "created" | "new_permissions_accepted" | "suspend" | "unsuspend" | "deleted"
        )
    {
        return Ok(refusal(
            "github_webhook_payload_invalid",
            "installation action or id is unsupported",
        ));
    }
    let (tenant_id, actor_id, account_login, account_type) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned while resolving webhook".to_string())?;
        if kernel
            .ledger()
            .entries()
            .iter()
            .any(|receipt| payload(receipt, "github_webhook_delivery_id") == delivery_id)
        {
            return Ok(RouteResponse::json(
                "200 OK",
                json!({
                    "name": "mdx-mobile-cloud-github-webhook",
                    "status": "IDEMPOTENT_REPLAY",
                    "delivery_id": delivery_id,
                    "payload_recorded": false,
                    "production_write_allowed": false
                })
                .to_string(),
            ));
        }
        let installation_id_text = installation_id.to_string();
        let Some(receipt) = kernel.ledger().entries().iter().rev().find(|receipt| {
            payload(receipt, "cloud_record_kind") == "github_installation"
                && payload(receipt, "github_installation_id") == installation_id_text
        }) else {
            return Ok(refusal(
                "github_webhook_installation_unknown",
                "webhook installation is not bound to an MDx tenant",
            ));
        };
        (
            receipt.tenant_id.clone(),
            receipt.actor_id.clone(),
            payload(receipt, "github_account_login").to_string(),
            payload(receipt, "github_account_type").to_string(),
        )
    };
    let status = match action {
        "suspend" => "suspended",
        "deleted" => "deleted",
        _ => "active",
    };
    let installation_id_text = installation_id.to_string();
    let identity = GovernedWriteIdentity {
        identity_source: "verified_github_webhook".to_string(),
        actor_kind: "service".to_string(),
        subject_actor_id: actor_id.to_string(),
        delegation_id: String::new(),
        authority_scope: vec!["mobile:cloud:installation-lifecycle".to_string()],
    };
    let report = record_cloud_event(
        kernel,
        (tenant_id.as_str(), actor_id.as_str(), &identity),
        &format!("forge_run_cloud_installation_{installation_id}"),
        "mobile_cloud_installation",
        "GitHub App installation lifecycle updated",
        &[
            ("cloud_record_kind", "github_installation_webhook"),
            ("github_installation_id", installation_id_text.as_str()),
            ("github_account_login", account_login.as_str()),
            ("github_account_type", account_type.as_str()),
            ("github_installation_status", status),
            ("github_webhook_action", action),
            ("github_webhook_delivery_id", delivery_id),
            ("github_webhook_payload_recorded", "false"),
        ],
    )?;
    Ok(RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-cloud-github-webhook",
            "status": "ACCEPTED",
            "installation_id": installation_id,
            "installation_status": status,
            "delivery_id": delivery_id,
            "receipt_id": report.receipt_id,
            "payload_recorded": false,
            "production_write_allowed": false
        })
        .to_string(),
    ))
}

fn checkout_with_installation_token(
    tenant_id: &str,
    repository: &GitHubRepository,
    token: &str,
) -> Result<PreparedCheckout, String> {
    if token.trim().is_empty() || !valid_identifier(tenant_id) {
        return Err("managed checkout identity is invalid".to_string());
    }
    let base = cloud_repo_base();
    std::fs::create_dir_all(&base)
        .map_err(|error| format!("managed checkout root create failed: {error}"))?;
    let askpass = ensure_askpass_script(&base)?;
    let tenant_root = base.join(tenant_id);
    std::fs::create_dir_all(&tenant_root)
        .map_err(|error| format!("managed tenant checkout root create failed: {error}"))?;
    let destination = tenant_root.join(format!("github_{}", repository.repository_id));
    if !path_is_within(&destination, &base) {
        return Err("managed checkout escaped its bounded root".to_string());
    }
    let nonce = random_state()?;
    let temporary = tenant_root.join(format!(
        ".github_{}_{}",
        repository.repository_id,
        &nonce[..12]
    ));
    let remote = format!("https://github.com/{}.git", repository.full_name);
    let output = Command::new("git")
        .arg("clone")
        .arg("--no-tags")
        .arg("--depth")
        .arg("1")
        .arg("--branch")
        .arg(&repository.default_branch)
        .arg("--")
        .arg(&remote)
        .arg(&temporary)
        .env("GIT_ASKPASS", &askpass)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("MDX_GITHUB_INSTALLATION_TOKEN", token)
        .output();
    let _ = std::fs::remove_file(&askpass);
    let output = output.map_err(|error| format!("managed git clone could not start: {error}"))?;
    if !output.status.success() {
        let _ = std::fs::remove_dir_all(&temporary);
        let detail = String::from_utf8_lossy(&output.stderr).replace(token, "[redacted]");
        return Err(format!(
            "managed git clone failed: {}",
            detail.trim().chars().take(500).collect::<String>()
        ));
    }
    let revision = Command::new("git")
        .arg("-C")
        .arg(&temporary)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .map_err(|error| format!("managed checkout revision could not be read: {error}"))?;
    if !revision.status.success() {
        let _ = std::fs::remove_dir_all(&temporary);
        return Err("managed checkout did not expose a source revision".to_string());
    }
    let source_revision = String::from_utf8_lossy(&revision.stdout).trim().to_string();
    if source_revision.len() != 40 || !source_revision.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        let _ = std::fs::remove_dir_all(&temporary);
        return Err("managed checkout source revision was invalid".to_string());
    }
    let _ = Command::new("git")
        .arg("-C")
        .arg(&temporary)
        .arg("remote")
        .arg("set-url")
        .arg("origin")
        .arg(&remote)
        .output();
    if destination.exists() {
        std::fs::remove_dir_all(&destination)
            .map_err(|error| format!("previous managed checkout cleanup failed: {error}"))?;
    }
    std::fs::rename(&temporary, &destination)
        .map_err(|error| format!("managed checkout publish failed: {error}"))?;
    Ok(PreparedCheckout {
        root: destination,
        source_revision,
    })
}

pub(crate) fn ensure_managed_cloud_checkout(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    repo_id: &str,
) -> Result<Option<PathBuf>, String> {
    ensure_managed_cloud_checkout_with(kernel, tenant_id, repo_id, &LiveGitHubAppProvider)
}

fn ensure_managed_cloud_checkout_with<P: GitHubAppProvider>(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    repo_id: &str,
    provider: &P,
) -> Result<Option<PathBuf>, String> {
    let (recorded_root, cloud_repository) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned while recovering cloud checkout".to_string())?;
        let Some(recorded_root) = kernel.forge_repo_root(repo_id).map(PathBuf::from) else {
            return Ok(None);
        };
        if recorded_root.join(".git").is_dir() {
            return Ok(Some(recorded_root));
        }
        let cloud_repository = kernel
            .ledger()
            .entries()
            .iter()
            .rev()
            .find(|receipt| {
                receipt.kind == "forge.run.event"
                    && receipt.tenant_id.as_str() == tenant_id
                    && payload(receipt, "cloud_record_kind") == "cloud_repository"
                    && payload(receipt, "cloud_repo_id") == repo_id
            })
            .map(|receipt| {
                (
                    payload(receipt, "github_installation_id").to_string(),
                    payload(receipt, "github_repository_id").to_string(),
                )
            });
        (recorded_root, cloud_repository)
    };
    let Some((installation_id, repository_id)) = cloud_repository else {
        return Ok(None);
    };
    let installation_id = installation_id
        .parse::<u64>()
        .map_err(|_| "durable GitHub installation id is invalid".to_string())?;
    let repository_id = repository_id
        .parse::<u64>()
        .map_err(|_| "durable GitHub repository id is invalid".to_string())?;
    if !installation_belongs_to(kernel, tenant_id, installation_id)? {
        return Err("durable GitHub installation is no longer active for this tenant".to_string());
    }
    let repository = provider
        .repositories(installation_id)?
        .into_iter()
        .find(|repository| repository.repository_id == repository_id)
        .ok_or_else(|| {
            "durable GitHub repository is no longer selected in this installation".to_string()
        })?;
    let checkout = provider.checkout(tenant_id, installation_id, &repository)?;
    if checkout.root != recorded_root {
        return Err("recovered GitHub App checkout did not match its recorded root".to_string());
    }
    Ok(Some(checkout.root))
}

fn store_installation_session(state: &str, session: &InstallationSession) -> Result<(), String> {
    if let Ok(redis_url) = std::env::var("MDX_GITHUB_INSTALL_STATE_REDIS_URL") {
        let client = redis::Client::open(redis_url)
            .map_err(|_| "durable installation state configuration is invalid".to_string())?;
        let mut connection = client
            .get_connection()
            .map_err(|_| "durable installation state is unavailable".to_string())?;
        let key = installation_state_key(state);
        let value = serde_json::to_string(session)
            .map_err(|_| "installation state could not be encoded".to_string())?;
        let ttl = session.expires_at_epoch.saturating_sub(now_epoch()).max(1);
        let stored: Option<String> = redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("EX")
            .arg(ttl)
            .arg("NX")
            .query(&mut connection)
            .map_err(|_| "durable installation state write failed".to_string())?;
        return stored
            .is_some()
            .then_some(())
            .ok_or_else(|| "installation state collision was refused".to_string());
    }
    if production_deployment() {
        return Err(
            "production requires MDX_GITHUB_INSTALL_STATE_REDIS_URL for single-use installation state"
                .to_string(),
        );
    }
    let now = now_epoch();
    let mut sessions = install_sessions()
        .lock()
        .map_err(|_| "GitHub installation session lock poisoned".to_string())?;
    sessions.retain(|_, value| value.expires_at_epoch > now);
    if sessions.len() >= MAX_INSTALL_SESSIONS {
        return Err("GitHub installation session capacity is temporarily exhausted".to_string());
    }
    sessions.insert(state.to_string(), session.clone());
    Ok(())
}

fn consume_installation_session(state: &str) -> Result<Option<InstallationSession>, String> {
    if let Ok(redis_url) = std::env::var("MDX_GITHUB_INSTALL_STATE_REDIS_URL") {
        let client = redis::Client::open(redis_url)
            .map_err(|_| "durable installation state configuration is invalid".to_string())?;
        let mut connection = client
            .get_connection()
            .map_err(|_| "durable installation state is unavailable".to_string())?;
        let value: Option<String> = redis::cmd("GETDEL")
            .arg(installation_state_key(state))
            .query(&mut connection)
            .map_err(|_| "durable installation state consume failed".to_string())?;
        return value
            .map(|value| {
                serde_json::from_str(&value)
                    .map_err(|_| "durable installation state was invalid".to_string())
            })
            .transpose();
    }
    if production_deployment() {
        return Err("production durable installation state is not configured".to_string());
    }
    install_sessions()
        .lock()
        .map_err(|_| "GitHub installation session lock poisoned".to_string())
        .map(|mut sessions| sessions.remove(state))
}

fn installation_state_key(state: &str) -> String {
    format!(
        "mdx:mobile-cloud:github-state:{}",
        digest_hex(state.as_bytes())
    )
}

fn production_deployment() -> bool {
    std::env::var("MDX_DEPLOYMENT_MODE").ok().as_deref() == Some("production")
        || std::env::var("MDX_ENV").ok().as_deref() == Some("production")
}

fn ensure_askpass_script(base: &Path) -> Result<PathBuf, String> {
    let script = base.join(format!("git-askpass-{}.sh", &random_state()?[..16]));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&script)
        .map_err(|error| format!("managed git credential helper create failed: {error}"))?;
    file.write_all(
        concat!(
            "#!/bin/sh\n",
            "case \"$1\" in\n",
            "  *Username*) printf '%s\\n' 'x-access-token' ;;\n",
            "  *) printf '%s\\n' \"$MDX_GITHUB_INSTALLATION_TOKEN\" ;;\n",
            "esac\n"
        )
        .as_bytes(),
    )
    .map_err(|error| format!("managed git credential helper write failed: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("managed git credential helper chmod failed: {error}"))?;
    }
    Ok(script)
}

fn cloud_repo_base() -> PathBuf {
    std::env::var("MDX_CLOUD_REPO_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(".mdx-local/cloud-repos"))
}

fn managed_cloud_root(root: &Path) -> bool {
    let Ok(root) = std::fs::canonicalize(root) else {
        return false;
    };
    let Ok(base) = std::fs::canonicalize(cloud_repo_base()) else {
        return false;
    };
    root.starts_with(base) && root.join(".git").is_dir()
}

fn path_is_within(path: &Path, base: &Path) -> bool {
    let absolute = |value: &Path| {
        if value.is_absolute() {
            value.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(value)
        }
    };
    absolute(path).starts_with(absolute(base))
}

fn installation_belongs_to(
    kernel: &Arc<RwLock<MdxKernel>>,
    tenant_id: &str,
    installation_id: u64,
) -> Result<bool, String> {
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned while authorizing installation".to_string())?;
    let installation_id = installation_id.to_string();
    Ok(kernel
        .ledger()
        .entries()
        .iter()
        .rev()
        .find(|receipt| {
            receipt.kind == "forge.run.event"
                && receipt.tenant_id.as_str() == tenant_id
                && matches!(
                    payload(receipt, "cloud_record_kind"),
                    "github_installation" | "github_installation_webhook"
                )
                && payload(receipt, "github_installation_id") == installation_id
        })
        .is_some_and(|receipt| payload(receipt, "github_installation_status") == "active"))
}

fn record_cloud_event(
    kernel: &Arc<RwLock<MdxKernel>>,
    principal: (&str, &str, &GovernedWriteIdentity),
    run_id: &str,
    work_item_id: &str,
    detail: &str,
    evidence: &[(&str, &str)],
) -> Result<ForgeRunEventReport, String> {
    let (tenant_id, actor_id, identity) = principal;
    let mut kernel = kernel
        .write()
        .map_err(|_| "kernel lock poisoned while recording cloud receipt".to_string())?;
    kernel
        .record_forge_run_event_with_evidence_fields(
            ForgeRunEvent {
                tenant_id,
                actor_id,
                run_id,
                event: "evidence_appended",
                work_item_id,
                detail,
                turn: 0,
                input_tokens: 0,
                output_tokens: 0,
            },
            identity,
            evidence,
        )
        .map_err(|error| error.message())
}

fn repository_json(repository: &GitHubRepository) -> Value {
    json!({
        "repository_id": repository.repository_id,
        "full_name": repository.full_name,
        "display_name": repository.display_name,
        "default_branch": repository.default_branch,
        "private": repository.private,
        "selection_scope": "github_app_installation"
    })
}

fn valid_repository_name(value: &str) -> bool {
    if value.is_empty() || value.len() > 201 || value.matches('/').count() != 1 {
        return false;
    }
    value.split('/').all(|component| {
        !component.is_empty()
            && component != "."
            && component != ".."
            && component.len() <= 100
            && component
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    })
}

fn valid_secret_ref(value: &str) -> bool {
    value
        .strip_prefix("secret://")
        .is_some_and(|name| valid_identifier(name) && !name.contains('.'))
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn string_value(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_string()
}

fn payload<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn path_base(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

fn query_value(path: &str, key: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
}

fn request_header<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request.lines().skip(1).find_map(|line| {
        let (candidate, value) = line.split_once(':')?;
        candidate.eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

fn verify_github_signature(secret: &str, body: &[u8], signature: &str) -> bool {
    let Some(encoded) = signature.strip_prefix("sha256=") else {
        return false;
    };
    let Some(bytes) = decode_hex(encoded) else {
        return false;
    };
    hmac::verify(
        &hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes()),
        body,
        &bytes,
    )
    .is_ok()
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16)?;
            let low = (pair[1] as char).to_digit(16)?;
            Some(((high << 4) | low) as u8)
        })
        .collect()
}

fn digest_hex(bytes: &[u8]) -> String {
    digest::digest(&digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn random_state() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "secure GitHub installation state generation failed".to_string())?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn github_app_configured() -> bool {
    [
        "MDX_GITHUB_APP_ID",
        "MDX_GITHUB_APP_PRIVATE_KEY_PEM",
        "MDX_GITHUB_APP_INSTALL_URL",
        "MDX_GITHUB_WEBHOOK_SECRET",
    ]
    .iter()
    .all(|name| std::env::var(name).is_ok_and(|value| !value.trim().is_empty()))
}

fn refusal(code: &str, detail: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        json!({
            "name": "mdx-mobile-cloud",
            "status": "REFUSED",
            "error": code,
            "detail": detail,
            "receipt_recorded": false,
            "token_recorded": false,
            "secret_values_recorded": false,
            "production_write_allowed": false
        })
        .to_string(),
    )
}

fn method_not_allowed() -> RouteResponse {
    RouteResponse::text("405 Method Not Allowed", "method not allowed\n".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_refusals_disclose_that_no_receipt_was_recorded() {
        let response = refusal("cloud_test_refusal", "test refusal");
        let body: Value = serde_json::from_str(response.body_text()).expect("refusal JSON");

        assert_eq!(body["status"], "REFUSED");
        assert_eq!(body["receipt_recorded"], false);
    }

    #[test]
    fn empty_cloud_setup_keeps_explicit_receipt_provenance() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = setup_projection("GET", &kernel).expect("setup projection");
        let body: Value = serde_json::from_str(response.body_text()).expect("setup JSON");

        assert_eq!(body["source_receipt_ids"], json!([]));
    }

    struct MockGitHubProvider {
        root: PathBuf,
    }

    impl GitHubAppProvider for MockGitHubProvider {
        fn verify_installation(&self, installation_id: u64) -> Result<GitHubInstallation, String> {
            Ok(GitHubInstallation {
                installation_id,
                account_login: "mdx-labs".to_string(),
                account_type: "Organization".to_string(),
            })
        }

        fn repositories(&self, _installation_id: u64) -> Result<Vec<GitHubRepository>, String> {
            Ok(vec![GitHubRepository {
                repository_id: 77,
                full_name: "mdx-labs/native".to_string(),
                display_name: "native".to_string(),
                default_branch: "main".to_string(),
                private: true,
            }])
        }

        fn checkout(
            &self,
            _tenant_id: &str,
            _installation_id: u64,
            _repository: &GitHubRepository,
        ) -> Result<PreparedCheckout, String> {
            if !self.root.join(".git").is_dir() {
                std::fs::create_dir_all(&self.root)
                    .map_err(|error| format!("mock checkout create failed: {error}"))?;
                let status = Command::new("git")
                    .arg("init")
                    .arg("--quiet")
                    .arg(&self.root)
                    .status()
                    .map_err(|error| format!("mock checkout git init failed: {error}"))?;
                if !status.success() {
                    return Err("mock checkout git init failed".to_string());
                }
                std::fs::write(self.root.join("Cargo.toml"), "[workspace]\n")
                    .map_err(|error| format!("mock checkout manifest failed: {error}"))?;
            }
            Ok(PreparedCheckout {
                root: self.root.clone(),
                source_revision: "0123456789abcdef0123456789abcdef01234567".to_string(),
            })
        }
    }

    fn test_repo() -> PathBuf {
        let root = std::env::temp_dir().join(format!("mdx-cloud-repo-{}", random_state().unwrap()));
        std::fs::create_dir_all(&root).expect("test repo root");
        let status = Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(&root)
            .status()
            .expect("git init");
        assert!(status.success());
        std::fs::write(root.join("Cargo.toml"), "[workspace]\n").expect("manifest");
        root
    }

    #[test]
    fn repository_names_and_secret_refs_are_bounded() {
        assert!(valid_repository_name("openai/codex"));
        assert!(!valid_repository_name("openai/codex/escape"));
        assert!(!valid_repository_name("../escape"));
        assert!(valid_secret_ref("secret://github_app_key"));
        assert!(!valid_secret_ref("plain-text-token"));
        assert!(!valid_secret_ref("secret://../token"));
        assert!(
            parse_github_repository(&json!({
                "id": 77,
                "full_name": "openai/codex",
                "name": "codex",
                "private": true
            }))
            .is_err()
        );
        assert_eq!(
            parse_github_repository(&json!({
                "id": 77,
                "full_name": "openai/codex",
                "name": "codex",
                "default_branch": "trunk",
                "private": true
            }))
            .expect("repository")
            .default_branch,
            "trunk"
        );
    }

    #[test]
    fn webhook_signature_verification_is_exact() {
        let key = hmac::Key::new(hmac::HMAC_SHA256, b"01234567890123456789012345678901");
        let signature = hmac::sign(&key, br#"{\"action\":\"suspend\"}"#);
        let encoded = signature
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert!(verify_github_signature(
            "01234567890123456789012345678901",
            br#"{\"action\":\"suspend\"}"#,
            &format!("sha256={encoded}")
        ));
        assert!(!verify_github_signature(
            "01234567890123456789012345678901",
            br#"{\"action\":\"deleted\"}"#,
            &format!("sha256={encoded}")
        ));
    }

    #[test]
    fn inferred_environment_is_conservative() {
        let root = std::env::temp_dir().join(format!("mdx-cloud-test-{}", now_epoch()));
        std::fs::create_dir_all(&root).expect("test root");
        std::fs::write(root.join("Cargo.toml"), "[workspace]\n").expect("manifest");
        std::fs::write(
            root.join(".gitignore"),
            "**/.tool-output/\n**/.worktrees/\n**/.env-cache/\n",
        )
        .expect("ignore contract");
        let (setup, proof, cache_paths, hosts) = infer_environment_commands(&root);
        assert_eq!(setup, vec!["cargo fetch --locked"]);
        assert_eq!(proof, vec!["cargo test --workspace --locked"]);
        assert!(cache_paths.contains(&"**/.tool-output".to_string()));
        assert!(!cache_paths.contains(&"**/.worktrees".to_string()));
        assert!(!cache_paths.contains(&"**/.env-cache".to_string()));
        assert!(!valid_mutable_path_pattern("**/.ssh"));
        assert!(!valid_mutable_path_pattern(".mdx/environment.json"));
        assert_eq!(hosts, vec!["crates.io", "github.com"]);
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn generated_environment_tracks_runtime_upgrades_but_committed_contracts_do_not() {
        let root = test_repo();
        let mut definition = CloudEnvironmentDefinition {
            schema_version: 1,
            environment_id: "cloud_env_repo-alpha".to_string(),
            environment_version: 1,
            repository_id: "repo-alpha".to_string(),
            base_image: "registry.example/runtime@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            architecture: "linux/arm64".to_string(),
            setup_commands: vec!["cargo fetch --locked".to_string()],
            proof_commands: vec!["cargo test --workspace --locked".to_string()],
            service_dependencies: Vec::new(),
            cache_paths: vec!["target".to_string()],
            resource_class: "medium".to_string(),
            network_allowlist: vec!["crates.io".to_string()],
            secret_binding_refs: Vec::new(),
            preview_ports: Vec::new(),
            retention_hours: 24,
            snapshot_policy: "after_verified_setup".to_string(),
            grants_execution_authority: false,
        };
        let next_image = "registry.example/runtime@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

        refresh_managed_base_image(&root, &mut definition, next_image)
            .expect("untracked generated definition may follow the runtime");
        assert_eq!(definition.base_image, next_image);
        assert_eq!(definition.environment_version, 2);

        let environment_dir = root.join(".mdx");
        std::fs::create_dir_all(&environment_dir).expect("environment directory");
        std::fs::write(environment_dir.join("environment.json"), "{}\n")
            .expect("tracked environment file");
        let added = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["add", ".mdx/environment.json"])
            .status()
            .expect("git add environment");
        assert!(added.success());
        definition.base_image = "registry.example/runtime@sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".to_string();
        let error = refresh_managed_base_image(&root, &mut definition, next_image)
            .expect_err("tracked definition requires an explicit repository change");
        assert!(error.contains("must be updated explicitly"));
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn verified_environment_definition_recovers_after_checkout_restart() {
        let root = test_repo();
        let definition = CloudEnvironmentDefinition {
            schema_version: 1,
            environment_id: "cloud_env_repo-alpha".to_string(),
            environment_version: 1,
            repository_id: "repo-alpha".to_string(),
            base_image: configured_cloud_base_image(),
            architecture: "linux/arm64".to_string(),
            setup_commands: vec!["cargo fetch --locked".to_string()],
            proof_commands: vec!["cargo test --workspace --locked".to_string()],
            service_dependencies: Vec::new(),
            cache_paths: vec!["target".to_string()],
            resource_class: "medium".to_string(),
            network_allowlist: vec!["crates.io".to_string()],
            secret_binding_refs: Vec::new(),
            preview_ports: Vec::new(),
            retention_hours: 24,
            snapshot_policy: "after_verified_setup".to_string(),
            grants_execution_authority: false,
        };
        let definition_value = serde_json::to_value(&definition).expect("definition value");
        let definition_json = serde_json::to_string(&definition_value).expect("definition JSON");
        let fingerprint = digest_hex(
            &serde_json::to_vec(&definition_value).expect("definition fingerprint input"),
        );
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let identity = GovernedWriteIdentity {
            identity_source: "test".to_string(),
            actor_kind: "human".to_string(),
            subject_actor_id: "local_user".to_string(),
            delegation_id: String::new(),
            authority_scope: vec!["mobile:cloud:environment".to_string()],
        };
        record_cloud_event(
            &kernel,
            ("local_tenant", "local_user", &identity),
            "forge_run_cloud_environment_test",
            "mobile_cloud_environment",
            "Cloud environment definition recorded",
            &[
                ("cloud_record_kind", "cloud_environment"),
                ("cloud_environment_id", "cloud_env_repo-alpha"),
                ("cloud_environment_repo_id", "repo-alpha"),
                ("cloud_environment_status", "VERIFIED"),
                ("cloud_environment_fingerprint", fingerprint.as_str()),
                (
                    "cloud_environment_definition_json",
                    definition_json.as_str(),
                ),
                ("cloud_verification_secret_values_recorded", "false"),
            ],
        )
        .expect("verified environment receipt");

        {
            let guard = kernel.read().expect("kernel");
            require_verified_environment(
                &guard,
                "local_tenant",
                "cloud_env_repo-alpha",
                "repo-alpha",
            )
            .expect("durable verified environment required");
        }
        let setup = setup_projection("GET", &kernel).expect("setup projection");
        let setup_body: Value =
            serde_json::from_str(setup.body_text()).expect("setup projection JSON");
        assert_eq!(setup_body["ready_environment_count"], 1);
        assert_eq!(
            setup_body["environments"][0]["definition_recoverable"],
            true
        );

        {
            let guard = kernel.read().expect("kernel");
            restore_verified_environment_definition(
                &guard,
                "local_tenant",
                "cloud_env_repo-alpha",
                "repo-alpha",
                &root,
            )
            .expect("environment restored");
        }
        let restored: CloudEnvironmentDefinition = serde_json::from_slice(
            &std::fs::read(root.join(".mdx/environment.json")).expect("restored definition"),
        )
        .expect("restored definition JSON");
        assert_eq!(restored, definition);
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn legacy_verified_environment_requires_one_reprepare() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let identity = GovernedWriteIdentity {
            identity_source: "test".to_string(),
            actor_kind: "human".to_string(),
            subject_actor_id: "local_user".to_string(),
            delegation_id: String::new(),
            authority_scope: vec!["mobile:cloud:environment".to_string()],
        };
        record_cloud_event(
            &kernel,
            ("local_tenant", "local_user", &identity),
            "forge_run_cloud_environment_legacy",
            "mobile_cloud_environment",
            "Legacy cloud environment receipt",
            &[
                ("cloud_record_kind", "cloud_environment"),
                ("cloud_environment_id", "cloud_env_repo-legacy"),
                ("cloud_environment_repo_id", "repo-legacy"),
                ("cloud_environment_status", "VERIFIED"),
                ("cloud_verification_secret_values_recorded", "false"),
            ],
        )
        .expect("legacy environment receipt");

        let guard = kernel.read().expect("kernel");
        let error = require_verified_environment(
            &guard,
            "local_tenant",
            "cloud_env_repo-legacy",
            "repo-legacy",
        )
        .expect_err("legacy receipt must require reprepare");
        assert!(error.contains("prepare it once more"));
        drop(guard);

        let setup = setup_projection("GET", &kernel).expect("setup projection");
        let setup_body: Value =
            serde_json::from_str(setup.body_text()).expect("setup projection JSON");
        assert_eq!(setup_body["ready_environment_count"], 0);
        assert_eq!(
            setup_body["environments"][0]["ready_for_cloud_builds"],
            false
        );
        assert_eq!(setup_body["environments"][0]["requires_reprepare"], true);
    }

    #[test]
    fn network_domains_are_canonical_hostnames() {
        for allowed in ["crates.io", "index.crates.io", "registry.npmjs.org"] {
            assert!(valid_network_domain(allowed));
        }
        for refused in [
            "*",
            "CRATES.IO",
            "-crates.io",
            "crates..io",
            "crates.io:443",
            "https://crates.io",
            "169.254.169.254",
        ] {
            assert!(!valid_network_domain(refused));
        }
    }

    #[test]
    fn installation_repository_and_checkout_form_one_receipted_flow() {
        let root = test_repo();
        let provider = MockGitHubProvider { root: root.clone() };
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let state = random_state().expect("state");
        install_sessions().lock().expect("session lock").insert(
            state.clone(),
            InstallationSession {
                tenant_id: "local_tenant".to_string(),
                actor_id: "local_user".to_string(),
                expires_at_epoch: now_epoch() + 60,
            },
        );
        let completion = complete_installation(
            "POST",
            &json!({ "state": state, "installation_id": 42 }).to_string(),
            &kernel,
            &provider,
        )
        .expect("installation completion");
        assert!(completion.body.contains("\"status\":\"CONNECTED\""));
        assert!(completion.body.contains("\"token_recorded\":false"));

        let repositories = list_repositories(
            "GET",
            "/mobile/cloud/repositories.json?installation_id=42",
            &kernel,
            &provider,
        )
        .expect("repository projection");
        assert!(repositories.body.contains("mdx-labs/native"));

        let connected = connect_repository(
            "POST",
            &json!({ "installation_id": 42, "repository_id": 77 }).to_string(),
            &kernel,
            &provider,
        )
        .expect("repository connection");
        assert!(connected.body.contains("\"status\":\"CONNECTED\""));
        assert!(
            connected
                .body
                .contains("\"source_revision\":\"0123456789abcdef")
        );

        let setup = setup_projection("GET", &kernel).expect("cloud setup projection");
        assert!(setup.body.contains("\"installation_id\":42"));
        assert!(setup.body.contains("\"repository_id\":77"));
        assert!(setup.body.contains("\"default_branch\":\"main\""));
        assert!(!setup.body.contains("github_app_private_key"));

        std::fs::remove_dir_all(&root).expect("simulate checkout loss");
        let recovered =
            ensure_managed_cloud_checkout_with(&kernel, "local_tenant", "github_77", &provider)
                .expect("GitHub App checkout recovery")
                .expect("managed checkout");
        assert_eq!(recovered, root);
        assert!(recovered.join(".git").is_dir());
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
