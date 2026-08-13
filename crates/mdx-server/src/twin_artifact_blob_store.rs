use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_smithy_types::error::metadata::ProvideErrorMetadata;
use aws_smithy_types::timeout::TimeoutConfig;
use mdx_core::DeploymentMode;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

pub(crate) const TWIN_ARTIFACT_BLOBS_DIR: &str = ".mdx-local/twin-artifacts";
const TWIN_ARTIFACT_PARSER_DIR: &str = ".mdx-local/parser-sandbox/twin-artifacts";
const DEFAULT_OPERATION_TIMEOUT_SECONDS: u64 = 15;
const MAX_OBJECT_BYTES: usize = mdx_core::TWIN_ARTIFACT_FILE_SIZE_LIMIT_BYTES;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Backend {
    Local,
    S3,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CredentialMode {
    Ambient,
    Static,
}

struct StorageConfig {
    backend: Backend,
    bucket: Option<String>,
    region: Option<String>,
    endpoint: Option<String>,
    credential_mode: CredentialMode,
    access_key_id: Option<String>,
    secret_access_key: Option<String>,
    force_path_style: bool,
    live_enabled: bool,
    binary_uploads_enabled: bool,
    approval_receipt_present: bool,
    operation_timeout_seconds: u64,
}

impl StorageConfig {
    fn from_env() -> Result<Self, String> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Result<Self, String> {
        let value = |key: &str, lookup: &mut dyn FnMut(&str) -> Option<String>| {
            lookup(key).map(|value| value.trim().to_string())
        };
        let backend_raw = value("MDX_STORAGE_BACKEND", &mut lookup).unwrap_or_default();
        let backend = match backend_raw.as_str() {
            "" | "local" => Backend::Local,
            "s3" => Backend::S3,
            _ => return Err("MDX_STORAGE_BACKEND must be local or s3".to_string()),
        };
        let credential_mode_raw = value("MDX_STORAGE_CREDENTIAL_MODE", &mut lookup)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "ambient".to_string());
        let credential_mode = match credential_mode_raw.as_str() {
            "ambient" => CredentialMode::Ambient,
            "static" => CredentialMode::Static,
            _ => {
                return Err("MDX_STORAGE_CREDENTIAL_MODE must be ambient or static".to_string());
            }
        };
        let bucket = non_empty(value("MDX_STORAGE_BUCKET", &mut lookup));
        let region = non_empty(value("MDX_STORAGE_REGION", &mut lookup));
        let endpoint = non_empty(value("MDX_STORAGE_ENDPOINT", &mut lookup));
        let access_key_id = non_empty(value("MDX_STORAGE_ACCESS_KEY_ID", &mut lookup));
        let secret_access_key = non_empty(value("MDX_STORAGE_SECRET_ACCESS_KEY", &mut lookup));
        let force_path_style = flag(value("MDX_STORAGE_FORCE_PATH_STYLE", &mut lookup));
        let live_enabled = flag(value("MDX_OBJECT_STORAGE_LIVE_ENABLED", &mut lookup));
        let binary_uploads_enabled = flag(value(
            "MDX_OBJECT_STORAGE_BINARY_UPLOADS_ENABLED",
            &mut lookup,
        ));
        let approval_receipt_present = non_empty(value(
            "MDX_STORAGE_TURN_ON_APPROVAL_RECEIPT_ID",
            &mut lookup,
        ))
        .is_some();
        let operation_timeout_seconds = value("MDX_STORAGE_OPERATION_TIMEOUT_SECONDS", &mut lookup)
            .filter(|value| !value.is_empty())
            .map(|value| {
                value
                    .parse::<u64>()
                    .map_err(|_| "MDX_STORAGE_OPERATION_TIMEOUT_SECONDS must be an integer")
            })
            .transpose()?
            .unwrap_or(DEFAULT_OPERATION_TIMEOUT_SECONDS);
        if !(1..=60).contains(&operation_timeout_seconds) {
            return Err(
                "MDX_STORAGE_OPERATION_TIMEOUT_SECONDS must be between 1 and 60".to_string(),
            );
        }

        let config = Self {
            backend,
            bucket,
            region,
            endpoint,
            credential_mode,
            access_key_id,
            secret_access_key,
            force_path_style,
            live_enabled,
            binary_uploads_enabled,
            approval_receipt_present,
            operation_timeout_seconds,
        };
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<(), String> {
        if self.backend == Backend::Local {
            return Ok(());
        }
        let bucket = self
            .bucket
            .as_deref()
            .ok_or_else(|| "MDX_STORAGE_BUCKET is required for s3".to_string())?;
        if !valid_bucket(bucket) {
            return Err("MDX_STORAGE_BUCKET is not a valid S3 bucket name".to_string());
        }
        let region = self
            .region
            .as_deref()
            .ok_or_else(|| "MDX_STORAGE_REGION is required for s3".to_string())?;
        if !valid_region(region) {
            return Err("MDX_STORAGE_REGION is invalid".to_string());
        }
        if let Some(endpoint) = self.endpoint.as_deref() {
            validate_endpoint(endpoint)?;
        }
        if self.credential_mode == CredentialMode::Static
            && (self.access_key_id.is_none() || self.secret_access_key.is_none())
        {
            return Err(
                "static object storage credentials require access key id and secret access key"
                    .to_string(),
            );
        }
        if self.live_enabled && !self.approval_receipt_present {
            return Err(
                "live object storage requires MDX_STORAGE_TURN_ON_APPROVAL_RECEIPT_ID".to_string(),
            );
        }
        if self.binary_uploads_enabled && !self.live_enabled {
            return Err(
                "binary object storage uploads require MDX_OBJECT_STORAGE_LIVE_ENABLED".to_string(),
            );
        }
        Ok(())
    }

    fn ready_for_production(&self) -> bool {
        self.backend == Backend::S3
            && self.bucket.is_some()
            && self.region.is_some()
            && self
                .endpoint
                .as_deref()
                .map(|endpoint| endpoint.starts_with("https://"))
                .unwrap_or(true)
    }
}

struct S3Runtime {
    runtime: Mutex<tokio::runtime::Runtime>,
    client: Client,
    bucket: String,
    operation_timeout: Duration,
}

static S3_RUNTIME: OnceLock<Result<S3Runtime, String>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StoredTwinArtifact {
    pub source_body_ref: String,
    pub extracted_text_ref: String,
    pub source_body_hash: String,
    pub extracted_text_hash: String,
    pub byte_size: usize,
    pub extracted_text_char_count: usize,
    pub storage_path_posture: &'static str,
    pub storage_contract_state: &'static str,
    pub encrypted_at_rest_posture: &'static str,
    pub malware_scan_state: &'static str,
    pub quarantine_state: &'static str,
}

pub(crate) struct StagedParserSource {
    path: PathBuf,
    directory: PathBuf,
}

impl StagedParserSource {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StagedParserSource {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

pub(crate) fn initialize(mode: DeploymentMode) -> Result<(), String> {
    let config = StorageConfig::from_env()?;
    if mode == DeploymentMode::Production && !config.ready_for_production() {
        return Err(
            "production object storage requires a complete S3-compatible configuration".to_string(),
        );
    }
    if config.backend == Backend::S3 {
        s3_runtime(&config)?;
    }
    Ok(())
}

pub(crate) fn production_configuration_ready() -> bool {
    StorageConfig::from_env()
        .map(|config| config.ready_for_production())
        .unwrap_or(false)
}

pub(crate) fn store_text_artifact(
    tenant_id: &str,
    artifact_id: &str,
    artifact_text: &str,
) -> Result<StoredTwinArtifact, String> {
    store_binary_artifact_with_extracted_text(
        tenant_id,
        artifact_id,
        "txt",
        artifact_text.as_bytes(),
        artifact_text,
    )
}

pub(crate) fn store_binary_artifact_with_extracted_text(
    tenant_id: &str,
    artifact_id: &str,
    source_extension: &str,
    source_bytes: &[u8],
    extracted_text: &str,
) -> Result<StoredTwinArtifact, String> {
    let tenant = sanitized_segment(tenant_id)
        .ok_or_else(|| "object storage tenant id is invalid".to_string())?;
    let artifact = sanitized_segment(artifact_id)
        .ok_or_else(|| "object storage artifact id is invalid".to_string())?;
    let source_extension = match source_extension {
        "pdf" | "txt" => source_extension,
        _ => return Err("object storage source extension is unsupported".to_string()),
    };
    if source_bytes.is_empty() || extracted_text.trim().is_empty() {
        return Err("object storage refuses empty artifact content".to_string());
    }
    if source_bytes.len() > MAX_OBJECT_BYTES || extracted_text.len() > MAX_OBJECT_BYTES {
        return Err("object storage artifact exceeds the size limit".to_string());
    }
    let config = StorageConfig::from_env()?;
    match config.backend {
        Backend::Local => store_local(
            &tenant,
            &artifact,
            source_extension,
            source_bytes,
            extracted_text,
        ),
        Backend::S3 => {
            require_live_provider(&config)?;
            if source_extension == "pdf" && !config.binary_uploads_enabled {
                return Err(
                    "binary object storage uploads are held until explicitly approved".to_string(),
                );
            }
            store_s3(
                &config,
                &tenant,
                &artifact,
                source_extension,
                source_bytes,
                extracted_text,
            )
        }
    }
}

pub(crate) fn delete_artifact(tenant_id: &str, artifact_id: &str) -> Result<bool, String> {
    let tenant = sanitized_segment(tenant_id)
        .ok_or_else(|| "object storage tenant id is invalid".to_string())?;
    let artifact = sanitized_segment(artifact_id)
        .ok_or_else(|| "object storage artifact id is invalid".to_string())?;
    let config = StorageConfig::from_env()?;
    match config.backend {
        Backend::Local => delete_local(&tenant, &artifact),
        Backend::S3 => {
            require_live_provider(&config)?;
            delete_s3(&config, &tenant, &artifact)
        }
    }
}

pub(crate) fn read_source_text(
    tenant_id: &str,
    artifact_id: &str,
) -> Result<Option<String>, String> {
    let tenant = sanitized_segment(tenant_id)
        .ok_or_else(|| "object storage tenant id is invalid".to_string())?;
    let artifact = sanitized_segment(artifact_id)
        .ok_or_else(|| "object storage artifact id is invalid".to_string())?;
    let config = StorageConfig::from_env()?;
    match config.backend {
        Backend::Local => read_local_text(&tenant, &artifact),
        Backend::S3 => {
            require_live_provider(&config)?;
            read_s3_text(&config, &tenant, &artifact)
        }
    }
}

pub(crate) fn stage_pdf_for_parser(
    tenant_id: &str,
    artifact_id: &str,
    source_bytes: &[u8],
) -> Result<StagedParserSource, String> {
    let tenant = sanitized_segment(tenant_id)
        .ok_or_else(|| "parser staging tenant id is invalid".to_string())?;
    let artifact = sanitized_segment(artifact_id)
        .ok_or_else(|| "parser staging artifact id is invalid".to_string())?;
    if source_bytes.is_empty() || source_bytes.len() > MAX_OBJECT_BYTES {
        return Err("parser staging artifact is empty or exceeds the size limit".to_string());
    }
    let directory = PathBuf::from(format!("{TWIN_ARTIFACT_PARSER_DIR}/{tenant}/{artifact}"));
    std::fs::create_dir_all(&directory)
        .map_err(|_| "parser staging directory could not be created".to_string())?;
    let path = directory.join("source.pdf");
    if let Err(error) = std::fs::write(&path, source_bytes) {
        let _ = std::fs::remove_dir_all(&directory);
        return Err(format!(
            "parser staging source could not be written: {error}"
        ));
    }
    Ok(StagedParserSource { path, directory })
}

pub(crate) fn run_live_round_trip_proof() -> Result<String, String> {
    let config = StorageConfig::from_env()?;
    if config.backend != Backend::S3 {
        return Err("object storage live proof requires MDX_STORAGE_BACKEND=s3".to_string());
    }
    require_live_provider(&config)?;
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "object storage proof clock is invalid".to_string())?
        .as_nanos();
    let artifact_id = format!("object_storage_proof_{suffix}");
    let expected = "mdx object storage live round trip proof";
    let stored = store_text_artifact("tenant_storage_proof", &artifact_id, expected)?;
    let observed = read_source_text("tenant_storage_proof", &artifact_id)?
        .ok_or_else(|| "object storage proof read returned no object".to_string())?;
    if observed != expected {
        return Err("object storage proof read did not match the written bytes".to_string());
    }
    if !delete_artifact("tenant_storage_proof", &artifact_id)? {
        return Err("object storage proof delete was not observed".to_string());
    }
    Ok(serde_json::json!({
        "name": "mdx-object-storage-live-proof",
        "status": "OBJECT_STORAGE_ROUND_TRIP_OBSERVED",
        "backend": "s3",
        "source_body_hash": stored.source_body_hash,
        "write_observed": true,
        "read_observed": true,
        "delete_observed": true,
        "endpoint_value_recorded": false,
        "bucket_value_recorded": false,
        "credential_values_recorded": false,
        "approval_receipt_present": true,
        "production_write_allowed": false
    })
    .to_string())
}

pub(crate) fn live_proof_command() -> Result<(), String> {
    initialize(DeploymentMode::Production)?;
    println!("{}", run_live_round_trip_proof()?);
    Ok(())
}

fn store_local(
    tenant: &str,
    artifact: &str,
    source_extension: &str,
    source_bytes: &[u8],
    extracted_text: &str,
) -> Result<StoredTwinArtifact, String> {
    let dir = format!("{TWIN_ARTIFACT_BLOBS_DIR}/{tenant}/{artifact}");
    std::fs::create_dir_all(&dir)
        .map_err(|_| "local object storage directory could not be created".to_string())?;
    let source_body_ref = format!("{dir}/source.{source_extension}");
    let extracted_text_ref = format!("{dir}/extracted.txt");
    std::fs::write(&source_body_ref, source_bytes)
        .map_err(|_| "local object storage source write failed".to_string())?;
    if let Err(error) = std::fs::write(&extracted_text_ref, extracted_text.as_bytes()) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!(
            "local object storage extracted text write failed: {error}"
        ));
    }
    Ok(stored_report(
        source_body_ref,
        extracted_text_ref,
        source_bytes,
        extracted_text,
        "live_local",
        "LOCAL_BLOB_STORED_BY_REFERENCE",
        "host_filesystem_encryption_inherited_not_verified",
        if source_extension == "txt" {
            "LOCAL_TEXT_INPUT_NO_BINARY_SCAN_REQUIRED"
        } else {
            "LOCAL_BINARY_SCAN_REQUIRED_NOT_RUN"
        },
        "ACTIVE_LOCAL_PRIVATE_BYTES",
    ))
}

fn store_s3(
    config: &StorageConfig,
    tenant: &str,
    artifact: &str,
    source_extension: &str,
    source_bytes: &[u8],
    extracted_text: &str,
) -> Result<StoredTwinArtifact, String> {
    let runtime = s3_runtime(config)?;
    let source_key = source_key(tenant, artifact, source_extension);
    let extracted_key = extracted_key(tenant, artifact);
    let source_content_type = if source_extension == "pdf" {
        "application/pdf"
    } else {
        "text/plain; charset=utf-8"
    };
    runtime.put(&source_key, source_bytes, source_content_type)?;
    if let Err(error) = runtime.put(
        &extracted_key,
        extracted_text.as_bytes(),
        "text/plain; charset=utf-8",
    ) {
        let _ = runtime.delete_keys(std::slice::from_ref(&source_key));
        return Err(error);
    }
    Ok(stored_report(
        object_ref(tenant, artifact, &format!("source.{source_extension}")),
        object_ref(tenant, artifact, "extracted.txt"),
        source_bytes,
        extracted_text,
        "live_provider_observed",
        "S3_OBJECT_STORED_BY_REFERENCE",
        "provider_managed_encryption_configured_not_verified",
        if source_extension == "txt" {
            "TEXT_INPUT_NO_BINARY_SCAN_REQUIRED"
        } else {
            "BINARY_SCAN_REQUIRED_NOT_RUN"
        },
        if source_extension == "txt" {
            "ACTIVE_PRIVATE_OBJECT"
        } else {
            "QUARANTINED_PENDING_BINARY_SCAN"
        },
    ))
}

fn delete_local(tenant: &str, artifact: &str) -> Result<bool, String> {
    let dir = format!("{TWIN_ARTIFACT_BLOBS_DIR}/{tenant}/{artifact}");
    if !Path::new(&dir).exists() {
        return Ok(false);
    }
    std::fs::remove_dir_all(&dir)
        .map(|_| true)
        .map_err(|_| "local object storage delete failed".to_string())
}

fn delete_s3(config: &StorageConfig, tenant: &str, artifact: &str) -> Result<bool, String> {
    let runtime = s3_runtime(config)?;
    runtime.delete_keys(&[
        source_key(tenant, artifact, "txt"),
        source_key(tenant, artifact, "pdf"),
        extracted_key(tenant, artifact),
    ])?;
    Ok(true)
}

fn read_local_text(tenant: &str, artifact: &str) -> Result<Option<String>, String> {
    let path = format!("{TWIN_ARTIFACT_BLOBS_DIR}/{tenant}/{artifact}/source.txt");
    match std::fs::read_to_string(path) {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("local object storage read failed".to_string()),
    }
}

fn read_s3_text(
    config: &StorageConfig,
    tenant: &str,
    artifact: &str,
) -> Result<Option<String>, String> {
    let runtime = s3_runtime(config)?;
    let bytes = runtime.get(&source_key(tenant, artifact, "txt"))?;
    if bytes.len() > MAX_OBJECT_BYTES {
        return Err("object storage read exceeds the size limit".to_string());
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| "object storage text is not valid UTF-8".to_string())
}

impl S3Runtime {
    fn put(&self, key: &str, bytes: &[u8], content_type: &str) -> Result<(), String> {
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        let key = key.to_string();
        let body = ByteStream::from(bytes.to_vec());
        let content_type = content_type.to_string();
        self.block_on(async move {
            client
                .put_object()
                .bucket(bucket)
                .key(key)
                .content_type(content_type)
                .body(body)
                .send()
                .await
                .map(|_| ())
                .map_err(|error| {
                    let code = error
                        .as_service_error()
                        .and_then(ProvideErrorMetadata::code)
                        .unwrap_or("unknown");
                    let status = error
                        .raw_response()
                        .map(|response| response.status().as_u16())
                        .unwrap_or_default();
                    format!("object storage put failed: http_status={status} service_code={code}")
                })
        })
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, String> {
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        let key = key.to_string();
        self.block_on(async move {
            let response = client
                .get_object()
                .bucket(bucket)
                .key(key)
                .send()
                .await
                .map_err(|_| "object storage get failed".to_string())?;
            let collected = response
                .body
                .collect()
                .await
                .map_err(|_| "object storage body read failed".to_string())?;
            Ok(collected.into_bytes().to_vec())
        })
    }

    fn delete_keys(&self, keys: &[String]) -> Result<(), String> {
        let client = self.client.clone();
        let bucket = self.bucket.clone();
        let keys = keys.to_vec();
        self.block_on(async move {
            for key in keys {
                client
                    .delete_object()
                    .bucket(&bucket)
                    .key(key)
                    .send()
                    .await
                    .map_err(|_| "object storage delete failed".to_string())?;
            }
            Ok(())
        })
    }

    fn block_on<T>(
        &self,
        future: impl std::future::Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        let runtime = self
            .runtime
            .lock()
            .map_err(|_| "object storage runtime lock poisoned".to_string())?;
        runtime
            .block_on(async { tokio::time::timeout(self.operation_timeout, future).await })
            .map_err(|_| "object storage operation timed out".to_string())?
    }
}

fn s3_runtime(config: &StorageConfig) -> Result<&'static S3Runtime, String> {
    S3_RUNTIME
        .get_or_init(|| build_s3_runtime(config))
        .as_ref()
        .map_err(Clone::clone)
}

fn build_s3_runtime(config: &StorageConfig) -> Result<S3Runtime, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "object storage async runtime could not be created".to_string())?;
    let region = config
        .region
        .clone()
        .ok_or_else(|| "object storage region is missing".to_string())?;
    let mut loader =
        aws_config::defaults(aws_config::BehaviorVersion::latest()).region(Region::new(region));
    if config.credential_mode == CredentialMode::Static {
        let credentials = Credentials::new(
            config.access_key_id.clone().unwrap_or_default(),
            config.secret_access_key.clone().unwrap_or_default(),
            None,
            None,
            "mdx-object-storage-static",
        );
        loader = loader.credentials_provider(credentials);
    }
    let shared = runtime.block_on(loader.load());
    let timeout = Duration::from_secs(config.operation_timeout_seconds);
    let timeout_config = TimeoutConfig::builder()
        .operation_timeout(timeout)
        .operation_attempt_timeout(timeout)
        .build();
    let mut builder = aws_sdk_s3::config::Builder::from(&shared)
        .force_path_style(config.force_path_style || config.endpoint.is_some())
        .timeout_config(timeout_config);
    if let Some(endpoint) = config.endpoint.as_deref() {
        let endpoint = if endpoint.ends_with('/') {
            endpoint.to_string()
        } else {
            format!("{endpoint}/")
        };
        builder = builder.endpoint_url(endpoint);
    }
    Ok(S3Runtime {
        runtime: Mutex::new(runtime),
        client: Client::from_conf(builder.build()),
        bucket: config.bucket.clone().unwrap_or_default(),
        operation_timeout: timeout,
    })
}

fn require_live_provider(config: &StorageConfig) -> Result<(), String> {
    if config.live_enabled && config.approval_receipt_present {
        Ok(())
    } else {
        Err(
            "S3 object storage is configured but held pending explicit live turn-on approval"
                .to_string(),
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn stored_report(
    source_body_ref: String,
    extracted_text_ref: String,
    source_bytes: &[u8],
    extracted_text: &str,
    storage_path_posture: &'static str,
    storage_contract_state: &'static str,
    encrypted_at_rest_posture: &'static str,
    malware_scan_state: &'static str,
    quarantine_state: &'static str,
) -> StoredTwinArtifact {
    StoredTwinArtifact {
        source_body_ref,
        extracted_text_ref,
        source_body_hash: local_ref_hash(source_bytes),
        extracted_text_hash: local_ref_hash(extracted_text.as_bytes()),
        byte_size: source_bytes.len(),
        extracted_text_char_count: extracted_text.chars().count().min(16_000),
        storage_path_posture,
        storage_contract_state,
        encrypted_at_rest_posture,
        malware_scan_state,
        quarantine_state,
    }
}

fn source_key(tenant: &str, artifact: &str, extension: &str) -> String {
    format!("tenants/{tenant}/twin-artifacts/{artifact}/source.{extension}")
}

fn extracted_key(tenant: &str, artifact: &str) -> String {
    format!("tenants/{tenant}/twin-artifacts/{artifact}/extracted.txt")
}

fn object_ref(tenant: &str, artifact: &str, name: &str) -> String {
    format!("object://twin-artifacts/{tenant}/{artifact}/{name}")
}

fn sanitized_segment(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > 96 {
        return None;
    }
    if value
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        Some(value.to_string())
    } else {
        None
    }
}

fn valid_bucket(value: &str) -> bool {
    (3..=63).contains(&value.len())
        && !value.starts_with(['.', '-'])
        && !value.ends_with(['.', '-'])
        && value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
}

fn valid_region(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn validate_endpoint(value: &str) -> Result<(), String> {
    let endpoint = url::Url::parse(value)
        .map_err(|_| "MDX_STORAGE_ENDPOINT must be a valid URL".to_string())?;
    let loopback_http = endpoint.scheme() == "http"
        && matches!(endpoint.host_str(), Some("127.0.0.1" | "localhost"));
    if endpoint.scheme() != "https" && !loopback_http {
        return Err("MDX_STORAGE_ENDPOINT must use HTTPS except for loopback proof".to_string());
    }
    if !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(
            "MDX_STORAGE_ENDPOINT must not contain credentials, query, or fragment".to_string(),
        );
    }
    Ok(())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn flag(value: Option<String>) -> bool {
    matches!(value.as_deref(), Some("1" | "true" | "yes" | "on"))
}

fn local_ref_hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn config(values: &[(&str, &str)]) -> Result<StorageConfig, String> {
        let values = values
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect::<HashMap<_, _>>();
        StorageConfig::from_lookup(|key| values.get(key).cloned())
    }

    #[test]
    fn local_is_the_zero_configuration_default() {
        let config = config(&[]).expect("local config");
        assert_eq!(config.backend, Backend::Local);
        assert!(!config.ready_for_production());
    }

    #[test]
    fn supabase_shape_uses_static_credentials_and_path_style() {
        let config = config(&[
            ("MDX_STORAGE_BACKEND", "s3"),
            ("MDX_STORAGE_BUCKET", "mdx-private-artifacts"),
            ("MDX_STORAGE_REGION", "us-east-1"),
            (
                "MDX_STORAGE_ENDPOINT",
                "https://project-ref.storage.supabase.co/storage/v1/s3",
            ),
            ("MDX_STORAGE_CREDENTIAL_MODE", "static"),
            ("MDX_STORAGE_ACCESS_KEY_ID", "present-not-recorded"),
            ("MDX_STORAGE_SECRET_ACCESS_KEY", "present-not-recorded"),
            ("MDX_STORAGE_FORCE_PATH_STYLE", "true"),
        ])
        .expect("Supabase S3 config");
        assert_eq!(config.backend, Backend::S3);
        assert_eq!(config.credential_mode, CredentialMode::Static);
        assert!(config.force_path_style);
        assert!(config.ready_for_production());
        assert!(!config.live_enabled);
    }

    #[test]
    fn aws_shape_uses_ambient_workload_identity() {
        let config = config(&[
            ("MDX_STORAGE_BACKEND", "s3"),
            ("MDX_STORAGE_BUCKET", "mdx-enterprise-artifacts"),
            ("MDX_STORAGE_REGION", "ca-central-1"),
            ("MDX_STORAGE_CREDENTIAL_MODE", "ambient"),
        ])
        .expect("AWS S3 config");
        assert_eq!(config.credential_mode, CredentialMode::Ambient);
        assert!(config.endpoint.is_none());
        assert!(config.ready_for_production());
    }

    #[test]
    fn live_provider_calls_require_an_approval_receipt() {
        let error = config(&[
            ("MDX_STORAGE_BACKEND", "s3"),
            ("MDX_STORAGE_BUCKET", "mdx-private-artifacts"),
            ("MDX_STORAGE_REGION", "us-east-1"),
            ("MDX_OBJECT_STORAGE_LIVE_ENABLED", "true"),
        ])
        .err()
        .expect("missing approval must fail");
        assert!(error.contains("APPROVAL"), "unexpected error: {error}");
    }

    #[test]
    fn endpoint_refuses_embedded_credentials_or_query_values() {
        for endpoint in [
            "https://user:secret@example.com/storage/v1/s3",
            "https://example.com/storage/v1/s3?token=secret",
        ] {
            let error = config(&[
                ("MDX_STORAGE_BACKEND", "s3"),
                ("MDX_STORAGE_BUCKET", "mdx-private-artifacts"),
                ("MDX_STORAGE_REGION", "us-east-1"),
                ("MDX_STORAGE_ENDPOINT", endpoint),
            ])
            .err()
            .expect("credential-bearing endpoint must fail");
            assert!(
                error.contains("must not contain"),
                "unexpected error: {error}"
            );
        }
    }

    #[test]
    fn s3_transport_puts_gets_and_deletes_against_a_loopback_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind S3 stub");
        let address = listener.local_addr().expect("stub address");
        let server = std::thread::spawn(move || {
            for response_body in [b"".as_slice(), b"round-trip".as_slice(), b"".as_slice()] {
                let (mut stream, _) = listener.accept().expect("accept S3 request");
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("request timeout");
                read_http_request(&mut stream);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    response_body.len()
                );
                stream
                    .write_all(response.as_bytes())
                    .expect("response head");
                stream.write_all(response_body).expect("response body");
            }
        });
        let endpoint = format!("http://{address}");
        let config = config(&[
            ("MDX_STORAGE_BACKEND", "s3"),
            ("MDX_STORAGE_BUCKET", "mdx-private-artifacts"),
            ("MDX_STORAGE_REGION", "us-east-1"),
            ("MDX_STORAGE_ENDPOINT", &endpoint),
            ("MDX_STORAGE_CREDENTIAL_MODE", "static"),
            ("MDX_STORAGE_ACCESS_KEY_ID", "local-access-key"),
            ("MDX_STORAGE_SECRET_ACCESS_KEY", "local-secret-key"),
            ("MDX_STORAGE_FORCE_PATH_STYLE", "true"),
            ("MDX_OBJECT_STORAGE_LIVE_ENABLED", "true"),
            ("MDX_STORAGE_TURN_ON_APPROVAL_RECEIPT_ID", "receipt_local"),
        ])
        .expect("loopback S3 config");
        let runtime = build_s3_runtime(&config).expect("S3 runtime");
        runtime
            .put("proof/source.txt", b"round-trip", "text/plain")
            .expect("put");
        assert_eq!(runtime.get("proof/source.txt").expect("get"), b"round-trip");
        runtime
            .delete_keys(&["proof/source.txt".to_string()])
            .expect("delete");
        server.join().expect("S3 stub");
    }

    #[test]
    fn s3_transport_preserves_a_provider_endpoint_path_prefix() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind S3 stub");
        let address = listener.local_addr().expect("stub address");
        let (sender, receiver) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept S3 request");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("request timeout");
            sender
                .send(read_http_request(&mut stream))
                .expect("send captured request");
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .expect("response");
        });
        let endpoint = format!("http://{address}/storage/v1/s3");
        let config = config(&[
            ("MDX_STORAGE_BACKEND", "s3"),
            ("MDX_STORAGE_BUCKET", "mdx-private-artifacts"),
            ("MDX_STORAGE_REGION", "us-west-2"),
            ("MDX_STORAGE_ENDPOINT", &endpoint),
            ("MDX_STORAGE_CREDENTIAL_MODE", "static"),
            ("MDX_STORAGE_ACCESS_KEY_ID", "local-access-key"),
            ("MDX_STORAGE_SECRET_ACCESS_KEY", "local-secret-key"),
            ("MDX_STORAGE_FORCE_PATH_STYLE", "true"),
            ("MDX_OBJECT_STORAGE_LIVE_ENABLED", "true"),
            ("MDX_STORAGE_TURN_ON_APPROVAL_RECEIPT_ID", "receipt_local"),
        ])
        .expect("loopback S3 config");
        let runtime = build_s3_runtime(&config).expect("S3 runtime");
        runtime
            .put("proof/source.txt", b"round-trip", "text/plain")
            .expect("put");
        let request =
            String::from_utf8(receiver.recv().expect("captured request")).expect("request UTF-8");
        assert!(
            request.starts_with(
                "PUT /storage/v1/s3/mdx-private-artifacts/proof/source.txt?x-id=PutObject HTTP/1.1"
            ),
            "unexpected request path: {}",
            request.lines().next().unwrap_or_default()
        );
        server.join().expect("S3 stub");
    }

    #[test]
    fn store_and_delete_text_artifact_by_sanitized_reference() {
        let stored = store_text_artifact(
            "tenant_test",
            "twin_artifact_test_blob",
            "private text stays in the local object store",
        )
        .expect("stored");
        assert!(stored.source_body_ref.ends_with("/source.txt"));
        assert!(Path::new(&stored.source_body_ref).exists());
        assert!(Path::new(&stored.extracted_text_ref).exists());
        assert_eq!(
            read_source_text("tenant_test", "twin_artifact_test_blob")
                .expect("read")
                .as_deref(),
            Some("private text stays in the local object store")
        );
        assert!(delete_artifact("tenant_test", "twin_artifact_test_blob").expect("delete"));
        assert!(!Path::new(&stored.source_body_ref).exists());
    }

    #[test]
    fn store_binary_artifact_keeps_source_and_extracted_refs_separate() {
        let stored = store_binary_artifact_with_extracted_text(
            "tenant_test",
            "twin_artifact_test_pdf_blob",
            "pdf",
            b"%PDF-1.4 private bytes",
            "readable private markdown",
        )
        .expect("stored");
        assert!(stored.source_body_ref.ends_with("/source.pdf"));
        assert!(stored.extracted_text_ref.ends_with("/extracted.txt"));
        assert_ne!(stored.source_body_hash, stored.extracted_text_hash);
        assert!(delete_artifact("tenant_test", "twin_artifact_test_pdf_blob").expect("delete"));
    }

    #[test]
    fn parser_stage_is_deleted_when_guard_drops() {
        let staged = stage_pdf_for_parser(
            "tenant_test",
            "twin_artifact_parser_stage",
            b"%PDF-1.4 private bytes",
        )
        .expect("staged");
        let path = staged.path().to_path_buf();
        assert!(path.exists());
        drop(staged);
        assert!(!path.exists());
    }

    #[test]
    fn unsafe_segments_refuse_to_store_read_or_delete() {
        assert!(store_text_artifact("../tenant", "artifact", "x").is_err());
        assert!(store_text_artifact("tenant", "../artifact", "x").is_err());
        assert!(read_source_text("../tenant", "artifact").is_err());
        assert!(read_source_text("tenant", "../artifact").is_err());
        assert!(delete_artifact("../tenant", "artifact").is_err());
        assert!(delete_artifact("tenant", "../artifact").is_err());
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
        let mut request = Vec::new();
        let mut buffer = [0u8; 4096];
        let mut expected_length = None;
        loop {
            let count = stream.read(&mut buffer).expect("read S3 request");
            if count == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..count]);
            if expected_length.is_none()
                && let Some(header_end) = request.windows(4).position(|part| part == b"\r\n\r\n")
            {
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':').and_then(|(name, value)| {
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().ok())
                                .flatten()
                        })
                    })
                    .unwrap_or(0);
                expected_length = Some(header_end + 4 + content_length);
            }
            if expected_length.is_some_and(|length| request.len() >= length) {
                break;
            }
        }
        assert!(
            request.starts_with(b"PUT ")
                || request.starts_with(b"GET ")
                || request.starts_with(b"DELETE ")
        );
        assert!(
            String::from_utf8_lossy(&request)
                .to_ascii_lowercase()
                .contains("authorization: aws4-hmac-sha256")
        );
        request
    }
}
