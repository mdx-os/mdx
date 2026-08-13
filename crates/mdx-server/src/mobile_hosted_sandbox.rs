use aws_config::BehaviorVersion;
use aws_sdk_bedrockagentcore::Client;
use aws_sdk_bedrockagentcore::types::{
    InvokeAgentRuntimeCommandRequestBody, InvokeAgentRuntimeCommandStreamOutput,
};
use aws_smithy_types::Blob;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use ring::digest::{SHA256, digest};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs;
use std::path::{Component, Path};
use std::process::Command;
use std::time::Instant;

const MAX_SOURCE_FILE_COUNT: usize = 20_000;
const MAX_SOURCE_FILE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_SOURCE_TOTAL_BYTES: u64 = 70 * 1024 * 1024;
const MAX_INVOCATION_RESPONSE_BYTES: usize = 128 * 1024;
const MAX_COMMAND_OUTPUT_BYTES: usize = 512 * 1024;
const DEFAULT_COMMAND_TIMEOUT_SECONDS: i32 = 900;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostedEnvironmentDefinition {
    pub(crate) schema_version: u64,
    pub(crate) environment_id: String,
    pub(crate) environment_version: u64,
    pub(crate) repository_id: String,
    pub(crate) base_image: String,
    pub(crate) architecture: String,
    pub(crate) setup_commands: Vec<String>,
    pub(crate) proof_commands: Vec<String>,
    pub(crate) service_dependencies: Vec<String>,
    pub(crate) cache_paths: Vec<String>,
    pub(crate) resource_class: String,
    pub(crate) network_allowlist: Vec<String>,
    pub(crate) secret_binding_refs: Vec<String>,
    pub(crate) preview_ports: Vec<u16>,
    pub(crate) retention_hours: u16,
    pub(crate) snapshot_policy: String,
    pub(crate) grants_execution_authority: bool,
}

#[derive(Clone, Debug, Serialize)]
struct SourceFile {
    path: String,
    kind: &'static str,
    mode: u32,
    sha256: String,
    content_base64: String,
}

#[derive(Clone, Debug, Serialize)]
struct WorkspaceSyncPayload<'a> {
    schema_version: u64,
    operation: &'static str,
    source_fingerprint: &'a str,
    environment_fingerprint: &'a str,
    files: &'a [SourceFile],
    mutable_paths: &'a [String],
    secret_values_included: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct HostedCommandResult {
    pub(crate) passed: bool,
    pub(crate) exit_code: i32,
    pub(crate) duration_ms: u64,
    pub(crate) output_tail: String,
    pub(crate) output_sha256: String,
    pub(crate) source_fingerprint: String,
    pub(crate) session_ref: String,
    pub(crate) provider: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct HostedVerification {
    pub(crate) environment_fingerprint: String,
    pub(crate) source_fingerprint: String,
    pub(crate) session_ref: String,
    pub(crate) setup_count: usize,
    pub(crate) proof_count: usize,
    pub(crate) command_evidence_sha256: String,
    pub(crate) provider: &'static str,
}

#[derive(Clone, Debug)]
struct ProviderCommandResult {
    exit_code: i32,
    completed: bool,
    output: String,
}

trait HostedSandboxProvider {
    fn sync_workspace(
        &self,
        session_id: &str,
        payload: Vec<u8>,
        expected_source_fingerprint: &str,
    ) -> Result<(), String>;

    fn execute_command(
        &self,
        session_id: &str,
        command: &str,
        timeout_seconds: i32,
    ) -> Result<ProviderCommandResult, String>;
}

struct LiveAgentCoreProvider {
    client: Client,
    runtime_arn: String,
    qualifier: String,
}

impl LiveAgentCoreProvider {
    fn from_environment() -> Result<Self, String> {
        let runtime_arn = required_environment("MDX_AGENTCORE_RUNTIME_ARN")?;
        if !runtime_arn.starts_with("arn:aws:bedrock-agentcore:") {
            return Err("MDX_AGENTCORE_RUNTIME_ARN must be an AgentCore runtime ARN".to_string());
        }
        let qualifier = std::env::var("MDX_AGENTCORE_RUNTIME_QUALIFIER")
            .unwrap_or_else(|_| "DEFAULT".to_string());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("AgentCore async runtime failed: {error}"))?;
        let config = runtime.block_on(aws_config::defaults(BehaviorVersion::latest()).load());
        Ok(Self {
            client: Client::new(&config),
            runtime_arn,
            qualifier,
        })
    }
}

impl HostedSandboxProvider for LiveAgentCoreProvider {
    fn sync_workspace(
        &self,
        session_id: &str,
        payload: Vec<u8>,
        expected_source_fingerprint: &str,
    ) -> Result<(), String> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("AgentCore async runtime failed: {error}"))?;
        let output = runtime
            .block_on(
                self.client
                    .invoke_agent_runtime()
                    .agent_runtime_arn(&self.runtime_arn)
                    .runtime_session_id(session_id)
                    .qualifier(&self.qualifier)
                    .content_type("application/json")
                    .accept("application/json")
                    .payload(Blob::new(payload))
                    .send(),
            )
            .map_err(|error| format!("AgentCore workspace sync failed: {error}"))?;
        if output.status_code() != Some(200) {
            return Err(format!(
                "AgentCore workspace sync returned HTTP {:?}",
                output.status_code()
            ));
        }
        let bytes = runtime
            .block_on(output.response.collect())
            .map_err(|error| format!("AgentCore workspace response failed: {error}"))?
            .into_bytes();
        if bytes.len() > MAX_INVOCATION_RESPONSE_BYTES {
            return Err("AgentCore workspace response exceeded its bounded limit".to_string());
        }
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| "AgentCore workspace response was not JSON".to_string())?;
        if value["status"].as_str() != Some("READY")
            || value["source_fingerprint"].as_str() != Some(expected_source_fingerprint)
            || value["secret_values_received"].as_bool() != Some(false)
        {
            return Err("AgentCore workspace did not attest the synchronized source".to_string());
        }
        Ok(())
    }

    fn execute_command(
        &self,
        session_id: &str,
        command: &str,
        timeout_seconds: i32,
    ) -> Result<ProviderCommandResult, String> {
        let encoded = STANDARD.encode(command.as_bytes());
        let bounded_command = format!("/opt/mdx/bin/mdx-run-bounded {encoded}");
        let body = InvokeAgentRuntimeCommandRequestBody::builder()
            .command(bounded_command)
            .timeout(timeout_seconds.clamp(1, 3600))
            .build()
            .map_err(|error| format!("AgentCore command request failed: {error}"))?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("AgentCore async runtime failed: {error}"))?;
        let mut output = runtime
            .block_on(
                self.client
                    .invoke_agent_runtime_command()
                    .agent_runtime_arn(&self.runtime_arn)
                    .runtime_session_id(session_id)
                    .qualifier(&self.qualifier)
                    .content_type("application/json")
                    .accept("application/vnd.amazon.eventstream")
                    .body(body)
                    .send(),
            )
            .map_err(|error| format!("AgentCore command invocation failed: {error}"))?;
        if output.status_code().is_some_and(|status| status != 200) {
            return Err(format!(
                "AgentCore command returned HTTP {}",
                output.status_code().unwrap_or_default()
            ));
        }
        let mut combined = String::new();
        let mut exit_code = None;
        let mut completed = false;
        loop {
            let event = runtime
                .block_on(output.stream.recv())
                .map_err(|error| format!("AgentCore command stream failed: {error}"))?;
            let Some(event) = event else { break };
            let InvokeAgentRuntimeCommandStreamOutput::Chunk(chunk) = event else {
                continue;
            };
            if let Some(delta) = chunk.content_delta() {
                append_bounded(&mut combined, delta.stdout().unwrap_or(""));
                append_bounded(&mut combined, delta.stderr().unwrap_or(""));
            }
            if let Some(stop) = chunk.content_stop() {
                exit_code = Some(stop.exit_code());
                completed = stop.status().as_str() == "COMPLETED";
            }
        }
        Ok(ProviderCommandResult {
            exit_code: exit_code
                .ok_or_else(|| "AgentCore command stream ended without contentStop".to_string())?,
            completed,
            output: combined,
        })
    }
}

pub(crate) fn hosted_backend_configured() -> bool {
    [
        "MDX_AGENTCORE_RUNTIME_ARN",
        "MDX_AGENTCORE_RUNTIME_IMAGE",
        "MDX_AGENTCORE_NETWORK_ALLOWLIST",
        "MDX_AGENTCORE_NETWORK_POLICY_FINGERPRINT",
    ]
    .iter()
    .all(|name| std::env::var(name).is_ok_and(|value| !value.trim().is_empty()))
}

pub(crate) fn load_environment(root: &Path) -> Result<HostedEnvironmentDefinition, String> {
    let path = root.join(".mdx/environment.json");
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| "a prepared .mdx/environment.json is required".to_string())?;
    if !metadata.file_type().is_file() || metadata.len() > 128 * 1024 {
        return Err(".mdx/environment.json must be a bounded regular file".to_string());
    }
    let definition: HostedEnvironmentDefinition = serde_json::from_slice(
        &fs::read(&path).map_err(|error| format!("environment read failed: {error}"))?,
    )
    .map_err(|error| format!("environment definition is invalid: {error}"))?;
    validate_runtime_contract(&definition)?;
    Ok(definition)
}

pub(crate) fn environment_fingerprint(
    definition: &HostedEnvironmentDefinition,
) -> Result<String, String> {
    let value = json!({
        "schema_version": definition.schema_version,
        "environment_id": definition.environment_id,
        "environment_version": definition.environment_version,
        "repository_id": definition.repository_id,
        "base_image": definition.base_image,
        "architecture": definition.architecture,
        "setup_commands": definition.setup_commands,
        "proof_commands": definition.proof_commands,
        "service_dependencies": definition.service_dependencies,
        "cache_paths": definition.cache_paths,
        "resource_class": definition.resource_class,
        "network_allowlist": definition.network_allowlist,
        "secret_binding_refs": definition.secret_binding_refs,
        "preview_ports": definition.preview_ports,
        "retention_hours": definition.retention_hours,
        "snapshot_policy": definition.snapshot_policy,
        "grants_execution_authority": definition.grants_execution_authority,
    });
    serde_json::to_vec(&value)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| format!("environment fingerprint failed: {error}"))
}

pub(crate) fn verify_environment(root: &Path) -> Result<HostedVerification, String> {
    let definition = load_environment(root)?;
    let provider = LiveAgentCoreProvider::from_environment()?;
    verify_environment_with_provider(root, &definition, &provider)
}

pub(crate) fn run_hosted_command(
    run_id: &str,
    workspace: &Path,
    environment_root: &Path,
    command: &str,
) -> HostedCommandResult {
    let started = Instant::now();
    match run_hosted_command_inner(run_id, workspace, environment_root, command) {
        Ok(mut result) => {
            result.duration_ms = started.elapsed().as_millis() as u64;
            result
        }
        Err(reason) => HostedCommandResult {
            passed: false,
            exit_code: -1,
            duration_ms: started.elapsed().as_millis() as u64,
            output_sha256: sha256_hex(reason.as_bytes()),
            output_tail: compact_tail(&reason),
            source_fingerprint: String::new(),
            session_ref: "agentcore-session://unavailable".to_string(),
            provider: "aws_agentcore",
        },
    }
}

fn run_hosted_command_inner(
    run_id: &str,
    workspace: &Path,
    environment_root: &Path,
    command: &str,
) -> Result<HostedCommandResult, String> {
    if command.is_empty()
        || command.len() > 500
        || command.bytes().any(|b| matches!(b, b'\0' | b'\n' | b'\r'))
    {
        return Err("hosted command violates the bounded command contract".to_string());
    }
    let definition = load_environment(environment_root)?;
    let provider = LiveAgentCoreProvider::from_environment()?;
    run_hosted_command_with_provider(run_id, workspace, command, &definition, &provider)
}

fn run_hosted_command_with_provider(
    run_id: &str,
    workspace: &Path,
    command: &str,
    definition: &HostedEnvironmentDefinition,
    provider: &dyn HostedSandboxProvider,
) -> Result<HostedCommandResult, String> {
    let environment_fingerprint = environment_fingerprint(definition)?;
    let session_id = runtime_session_id(run_id);
    let source_fingerprint = sync_workspace(
        provider,
        &session_id,
        workspace,
        definition,
        &environment_fingerprint,
    )?;
    ensure_setup(provider, &session_id, definition, &environment_fingerprint)?;
    let result = provider.execute_command(&session_id, command, DEFAULT_COMMAND_TIMEOUT_SECONDS)?;
    let output_sha256 = sha256_hex(result.output.as_bytes());
    Ok(HostedCommandResult {
        passed: result.completed && result.exit_code == 0,
        exit_code: result.exit_code,
        duration_ms: 0,
        output_tail: compact_tail(&result.output),
        output_sha256,
        source_fingerprint,
        session_ref: session_reference(&session_id),
        provider: "aws_agentcore",
    })
}

fn verify_environment_with_provider(
    root: &Path,
    definition: &HostedEnvironmentDefinition,
    provider: &dyn HostedSandboxProvider,
) -> Result<HostedVerification, String> {
    let environment_fingerprint = environment_fingerprint(definition)?;
    let session_id = runtime_session_id(&format!(
        "verify-{}-{environment_fingerprint}",
        definition.environment_id
    ));
    let source_fingerprint = sync_workspace(
        provider,
        &session_id,
        root,
        definition,
        &environment_fingerprint,
    )?;
    let mut command_evidence = Vec::new();
    for command in definition
        .setup_commands
        .iter()
        .chain(definition.proof_commands.iter())
    {
        let result =
            provider.execute_command(&session_id, command, DEFAULT_COMMAND_TIMEOUT_SECONDS)?;
        command_evidence.push(json!({
            "command_sha256": sha256_hex(command.as_bytes()),
            "exit_code": result.exit_code,
            "completed": result.completed,
            "output_sha256": sha256_hex(result.output.as_bytes()),
        }));
        if !result.completed || result.exit_code != 0 {
            return Err(format!(
                "hosted verification command failed: {}",
                compact_tail(&result.output)
            ));
        }
    }
    Ok(HostedVerification {
        environment_fingerprint,
        source_fingerprint,
        session_ref: session_reference(&session_id),
        setup_count: definition.setup_commands.len(),
        proof_count: definition.proof_commands.len(),
        command_evidence_sha256: sha256_hex(
            &serde_json::to_vec(&command_evidence)
                .map_err(|error| format!("command evidence encode failed: {error}"))?,
        ),
        provider: "aws_agentcore",
    })
}

fn ensure_setup(
    provider: &dyn HostedSandboxProvider,
    session_id: &str,
    definition: &HostedEnvironmentDefinition,
    environment_fingerprint: &str,
) -> Result<(), String> {
    let marker = format!("/workspace/.mdx-runtime/setup-{environment_fingerprint}");
    let probe = provider.execute_command(session_id, &format!("test -f {marker}"), 30)?;
    if probe.completed && probe.exit_code == 0 {
        return Ok(());
    }
    for command in &definition.setup_commands {
        let result =
            provider.execute_command(session_id, command, DEFAULT_COMMAND_TIMEOUT_SECONDS)?;
        if !result.completed || result.exit_code != 0 {
            return Err(format!(
                "hosted setup command failed: {}",
                compact_tail(&result.output)
            ));
        }
    }
    let mark = provider.execute_command(
        session_id,
        &format!("mkdir -p /workspace/.mdx-runtime && touch {marker}"),
        30,
    )?;
    if !mark.completed || mark.exit_code != 0 {
        return Err("hosted setup completion marker could not be written".to_string());
    }
    Ok(())
}

fn sync_workspace(
    provider: &dyn HostedSandboxProvider,
    session_id: &str,
    root: &Path,
    definition: &HostedEnvironmentDefinition,
    environment_fingerprint: &str,
) -> Result<String, String> {
    let files = source_manifest(root, &definition.cache_paths)?;
    let source_fingerprint = manifest_fingerprint(&files)?;
    let payload = WorkspaceSyncPayload {
        schema_version: 1,
        operation: "replace_source",
        source_fingerprint: &source_fingerprint,
        environment_fingerprint,
        files: &files,
        mutable_paths: &definition.cache_paths,
        secret_values_included: false,
    };
    let bytes = serde_json::to_vec(&payload)
        .map_err(|error| format!("workspace payload encode failed: {error}"))?;
    if bytes.len() > 99_000_000 {
        return Err("workspace payload exceeds AgentCore's 100 MB invocation limit".to_string());
    }
    provider.sync_workspace(session_id, bytes, &source_fingerprint)?;
    Ok(source_fingerprint)
}

fn source_manifest(root: &Path, mutable_paths: &[String]) -> Result<Vec<SourceFile>, String> {
    ensure_mutable_paths_do_not_overlap_tracked_source(root, mutable_paths)?;
    let root = root
        .canonicalize()
        .map_err(|error| format!("workspace root is unavailable: {error}"))?;
    let mut files = Vec::new();
    let mut total_bytes = 0_u64;
    collect_source_files(&root, &root, mutable_paths, &mut files, &mut total_bytes)?;
    files.sort_by(|left, right| left.path.cmp(&right.path));
    if files.is_empty() {
        return Err("hosted workspace source is empty".to_string());
    }
    Ok(files)
}

fn collect_source_files(
    root: &Path,
    dir: &Path,
    mutable_paths: &[String],
    files: &mut Vec<SourceFile>,
    total_bytes: &mut u64,
) -> Result<(), String> {
    let mut entries = fs::read_dir(dir)
        .map_err(|error| format!("workspace directory read failed: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("workspace directory entry failed: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "workspace path escaped its root".to_string())?;
        if sensitive_source_path(relative) {
            return Err(format!(
                "hosted source snapshot refused credential-shaped path {}",
                relative.display()
            ));
        }
        if ignored_source_path(relative, mutable_paths) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("workspace metadata failed: {error}"))?;
        if metadata.file_type().is_dir() {
            collect_source_files(root, &path, mutable_paths, files, total_bytes)?;
            continue;
        }
        if files.len() >= MAX_SOURCE_FILE_COUNT {
            return Err(format!(
                "workspace exceeds the {MAX_SOURCE_FILE_COUNT} source file limit"
            ));
        }
        let relative = portable_relative_path(relative)?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path)
                .map_err(|error| format!("workspace symlink read failed: {error}"))?;
            validate_symlink_target(Path::new(&relative), &target)?;
            let content = target.to_string_lossy().into_owned();
            files.push(SourceFile {
                path: relative,
                kind: "symlink",
                mode: 0,
                sha256: sha256_hex(content.as_bytes()),
                content_base64: STANDARD.encode(content.as_bytes()),
            });
            continue;
        }
        if !metadata.file_type().is_file() {
            return Err(format!(
                "workspace contains unsupported source entry {relative}"
            ));
        }
        if metadata.len() > MAX_SOURCE_FILE_BYTES {
            return Err(format!(
                "source file {relative} exceeds the 16 MB file limit"
            ));
        }
        *total_bytes = total_bytes.saturating_add(metadata.len());
        if *total_bytes > MAX_SOURCE_TOTAL_BYTES {
            return Err("workspace exceeds the 70 MB source snapshot limit".to_string());
        }
        let bytes = fs::read(&path)
            .map_err(|error| format!("source file {relative} could not be read: {error}"))?;
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode() & 0o111
        };
        #[cfg(not(unix))]
        let mode = 0;
        files.push(SourceFile {
            path: relative,
            kind: "file",
            mode,
            sha256: sha256_hex(&bytes),
            content_base64: STANDARD.encode(bytes),
        });
    }
    Ok(())
}

fn sensitive_source_path(path: &Path) -> bool {
    path.components().any(|component| {
        let Component::Normal(value) = component else {
            return false;
        };
        let value = value.to_string_lossy();
        if matches!(
            value.as_ref(),
            ".env.example" | ".env.sample" | ".env.template"
        ) {
            return false;
        }
        value == ".ssh"
            || value == ".aws"
            || value == ".netrc"
            || value == ".npmrc"
            || value == ".pypirc"
            || value == "id_rsa"
            || value == "id_ed25519"
            || value == ".env"
            || value.starts_with(".env.")
    })
}

fn ignored_source_path(path: &Path, mutable_paths: &[String]) -> bool {
    let portable = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/");
    if mutable_paths
        .iter()
        .any(|pattern| mutable_path_matches(&portable, pattern))
    {
        return true;
    }
    let mut components = path.components();
    let Some(Component::Normal(first)) = components.next() else {
        return false;
    };
    let first = first.to_string_lossy();
    if matches!(
        first.as_ref(),
        ".git" | ".mdx-local" | ".mdx-runtime" | "dist" | "build"
    ) {
        return true;
    }
    path.components().any(|component| {
        let Component::Normal(value) = component else {
            return true;
        };
        matches!(
            value.to_string_lossy().as_ref(),
            ".git" | ".worktrees" | "target" | "node_modules" | ".build" | ".swiftpm" | ".venv"
        )
    })
}

fn mutable_path_matches(relative: &str, pattern: &str) -> bool {
    if let Some(component) = pattern.strip_prefix("**/") {
        return relative.split('/').any(|candidate| candidate == component);
    }
    relative == pattern || relative.starts_with(&format!("{pattern}/"))
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
                Component::Normal(value) => value
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

fn default_mutable_path(relative: &str) -> bool {
    let first = relative.split('/').next().unwrap_or_default();
    matches!(first, "build" | "dist")
        || relative.split('/').any(|component| {
            matches!(
                component,
                ".mdx-runtime"
                    | ".worktrees"
                    | "target"
                    | "node_modules"
                    | ".build"
                    | ".swiftpm"
                    | ".venv"
            )
        })
}

fn ensure_mutable_paths_do_not_overlap_tracked_source(
    root: &Path,
    mutable_paths: &[String],
) -> Result<(), String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output()
        .map_err(|error| format!("tracked source inventory failed: {error}"))?;
    if !output.status.success() {
        return Err("managed checkout is not a readable Git repository".to_string());
    }
    for bytes in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let relative = std::str::from_utf8(bytes)
            .map_err(|_| "tracked source path must be UTF-8".to_string())?;
        if default_mutable_path(relative)
            || mutable_paths
                .iter()
                .any(|pattern| mutable_path_matches(relative, pattern))
        {
            return Err(format!(
                "mutable path contract overlaps tracked source {relative}"
            ));
        }
    }
    Ok(())
}

fn portable_relative_path(path: &Path) -> Result<String, String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => {
                let value = value
                    .to_str()
                    .ok_or_else(|| "workspace path must be UTF-8".to_string())?;
                if value.is_empty() || value == "." || value == ".." {
                    return Err("workspace path contains an unsafe component".to_string());
                }
                parts.push(value);
            }
            _ => return Err("workspace path must remain relative".to_string()),
        }
    }
    Ok(parts.join("/"))
}

fn validate_symlink_target(link_path: &Path, target: &Path) -> Result<(), String> {
    if target.is_absolute() {
        return Err(format!(
            "source symlink {} has an absolute target",
            link_path.display()
        ));
    }
    let joined = link_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(target);
    let mut depth = 0_i32;
    for component in joined.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::ParentDir if depth > 0 => depth -= 1,
            Component::CurDir => {}
            _ => {
                return Err(format!(
                    "source symlink {} escapes the workspace",
                    link_path.display()
                ));
            }
        }
    }
    Ok(())
}

fn manifest_fingerprint(files: &[SourceFile]) -> Result<String, String> {
    let manifest = files
        .iter()
        .map(|file| {
            json!({
                "path": file.path,
                "kind": file.kind,
                "mode": file.mode,
                "sha256": file.sha256,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_vec(&manifest)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| format!("source fingerprint failed: {error}"))
}

fn validate_runtime_contract(definition: &HostedEnvironmentDefinition) -> Result<(), String> {
    if definition.schema_version != 1
        || definition.environment_version == 0
        || definition.grants_execution_authority
        || definition.architecture != "linux/arm64"
        || definition.proof_commands.is_empty()
    {
        return Err("environment is incompatible with the hosted sandbox contract".to_string());
    }
    if !definition.secret_binding_refs.is_empty() {
        return Err(
            "hosted secret bindings remain closed until the managed broker is configured"
                .to_string(),
        );
    }
    if definition
        .cache_paths
        .iter()
        .any(|path| !valid_mutable_path_pattern(path))
    {
        return Err("environment contains an invalid mutable path pattern".to_string());
    }
    let configured_image = required_environment("MDX_AGENTCORE_RUNTIME_IMAGE")?;
    if configured_image != definition.base_image {
        return Err(
            "AgentCore runtime image does not match the pinned environment image".to_string(),
        );
    }
    let deployed_allowlist = deployed_network_allowlist()?;
    let expected_network_fingerprint = network_policy_fingerprint(&deployed_allowlist)?;
    if required_environment("MDX_AGENTCORE_NETWORK_POLICY_FINGERPRINT")?
        != expected_network_fingerprint
    {
        return Err(
            "AgentCore network policy fingerprint does not match the deployed allowlist"
                .to_string(),
        );
    }
    if definition
        .network_allowlist
        .iter()
        .any(|domain| !deployed_allowlist.contains(domain))
    {
        return Err(
            "environment network requirements exceed the deployed AgentCore allowlist".to_string(),
        );
    }
    Ok(())
}

fn deployed_network_allowlist() -> Result<Vec<String>, String> {
    let raw = required_environment("MDX_AGENTCORE_NETWORK_ALLOWLIST")?;
    parse_deployed_network_allowlist(&raw)
}

fn parse_deployed_network_allowlist(raw: &str) -> Result<Vec<String>, String> {
    let mut values = raw
        .split(',')
        .map(str::trim)
        .map(str::to_string)
        .collect::<Vec<_>>();
    if values.is_empty() || values.iter().any(|value| !valid_network_domain(value)) {
        return Err("MDX_AGENTCORE_NETWORK_ALLOWLIST is invalid".to_string());
    }
    values.sort();
    values.dedup();
    Ok(values)
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

pub(crate) fn network_policy_fingerprint(allowlist: &[String]) -> Result<String, String> {
    let mut values = allowlist.to_vec();
    values.sort();
    values.dedup();
    serde_json::to_vec(&json!({
        "schema_version": 1,
        "default": "deny",
        "allow": values,
    }))
    .map(|bytes| sha256_hex(&bytes))
    .map_err(|error| format!("network policy fingerprint failed: {error}"))
}

fn runtime_session_id(seed: &str) -> String {
    format!("mdx-agentcore-{}", &sha256_hex(seed.as_bytes())[..48])
}

fn session_reference(session_id: &str) -> String {
    format!(
        "agentcore-session://{}",
        &sha256_hex(session_id.as_bytes())[..32]
    )
}

fn required_environment(name: &str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("{name} is required for hosted execution"))
}

fn append_bounded(buffer: &mut String, value: &str) {
    if buffer.len() >= MAX_COMMAND_OUTPUT_BYTES {
        return;
    }
    let remaining = MAX_COMMAND_OUTPUT_BYTES - buffer.len();
    if value.len() <= remaining {
        buffer.push_str(value);
        return;
    }
    let mut end = remaining;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    buffer.push_str(&value[..end]);
    buffer.push_str("\n[output truncated by MDx]\n");
}

fn compact_tail(value: &str) -> String {
    const LIMIT: usize = 8 * 1024;
    if value.len() <= LIMIT {
        return value.to_string();
    }
    let mut start = value.len() - LIMIT;
    while !value.is_char_boundary(start) {
        start += 1;
    }
    format!("[truncated to last {LIMIT} bytes]\n{}", &value[start..])
}

fn sha256_hex(bytes: &[u8]) -> String {
    digest(&SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeProvider {
        commands: Mutex<Vec<String>>,
    }

    impl HostedSandboxProvider for FakeProvider {
        fn sync_workspace(
            &self,
            _session_id: &str,
            payload: Vec<u8>,
            expected_source_fingerprint: &str,
        ) -> Result<(), String> {
            let value: Value = serde_json::from_slice(&payload).unwrap();
            assert_eq!(value["source_fingerprint"], expected_source_fingerprint);
            assert_eq!(value["secret_values_included"], false);
            Ok(())
        }

        fn execute_command(
            &self,
            _session_id: &str,
            command: &str,
            _timeout_seconds: i32,
        ) -> Result<ProviderCommandResult, String> {
            self.commands.lock().unwrap().push(command.to_string());
            Ok(ProviderCommandResult {
                exit_code: 0,
                completed: true,
                output: format!("ok:{command}"),
            })
        }
    }

    fn definition() -> HostedEnvironmentDefinition {
        HostedEnvironmentDefinition {
            schema_version: 1,
            environment_id: "cloud_env_repo".to_string(),
            environment_version: 1,
            repository_id: "repo".to_string(),
            base_image: format!("example.invalid/mdx@sha256:{}", "a".repeat(64)),
            architecture: "linux/arm64".to_string(),
            setup_commands: vec!["cargo fetch --locked".to_string()],
            proof_commands: vec!["cargo test --locked".to_string()],
            service_dependencies: Vec::new(),
            cache_paths: vec!["target".to_string()],
            resource_class: "medium".to_string(),
            network_allowlist: vec!["crates.io".to_string()],
            secret_binding_refs: Vec::new(),
            preview_ports: Vec::new(),
            retention_hours: 24,
            snapshot_policy: "after_verified_setup".to_string(),
            grants_execution_authority: false,
        }
    }

    #[test]
    fn source_manifest_excludes_build_state_and_fingerprints_content() {
        let root = std::env::temp_dir().join(format!("mdx-hosted-manifest-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::create_dir_all(root.join("crates/example/.repo-generated")).unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn value() -> u8 { 7 }\n").unwrap();
        fs::write(root.join("target/secret-build-state"), "excluded").unwrap();
        fs::write(
            root.join("crates/example/.repo-generated/generated.json"),
            "excluded",
        )
        .unwrap();
        let initialized = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["init", "--quiet"])
            .status()
            .unwrap();
        assert!(initialized.success());
        let added = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["add", "src/lib.rs"])
            .status()
            .unwrap();
        assert!(added.success());
        let files = source_manifest(&root, &["**/.repo-generated".to_string()]).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/lib.rs");
        assert_eq!(manifest_fingerprint(&files).unwrap().len(), 64);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mutable_path_contract_cannot_hide_tracked_source() {
        let root =
            std::env::temp_dir().join(format!("mdx-hosted-mutable-overlap-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("packages/tool/.repo-generated")).unwrap();
        fs::write(
            root.join("packages/tool/.repo-generated/committed.json"),
            "tracked",
        )
        .unwrap();
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["init", "--quiet"])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["add", "packages/tool/.repo-generated/committed.json"])
                .status()
                .unwrap()
                .success()
        );
        let error = source_manifest(&root, &["**/.repo-generated".to_string()])
            .expect_err("tracked source overlap must fail closed");
        assert!(error.contains("overlaps tracked source"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn verification_runs_setup_then_proof_without_secret_values() {
        let root = std::env::temp_dir().join(format!("mdx-hosted-verify-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["init", "--quiet"])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["add", "Cargo.toml"])
                .status()
                .unwrap()
                .success()
        );
        let provider = FakeProvider::default();
        let result = verify_environment_with_provider(&root, &definition(), &provider).unwrap();
        assert_eq!(result.setup_count, 1);
        assert_eq!(result.proof_count, 1);
        assert_eq!(
            provider.commands.lock().unwrap().as_slice(),
            ["cargo fetch --locked", "cargo test --locked"]
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn distinct_forge_lanes_receive_distinct_agentcore_sessions() {
        let first = runtime_session_id("forge_run_fleet_stream_1");
        let second = runtime_session_id("forge_run_fleet_stream_2");

        assert_ne!(first, second);
        assert_eq!(first.len(), "mdx-agentcore-".len() + 48);
        assert_ne!(session_reference(&first), session_reference(&second));
    }

    #[test]
    fn hosted_run_syncs_the_isolated_worktree_with_an_external_definition() {
        let workspace = std::env::temp_dir().join(format!(
            "mdx-hosted-worktree-definition-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&workspace);
        fs::create_dir_all(&workspace).unwrap();
        fs::write(workspace.join("README.md"), "isolated change\n").unwrap();
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&workspace)
                .args(["init", "--quiet"])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&workspace)
                .args(["add", "README.md"])
                .status()
                .unwrap()
                .success()
        );
        assert!(!workspace.join(".mdx/environment.json").exists());

        let provider = FakeProvider::default();
        let result = run_hosted_command_with_provider(
            "run-distinct-definition-root",
            &workspace,
            "cargo test --locked",
            &definition(),
            &provider,
        )
        .expect("hosted worktree command");

        assert!(result.passed);
        assert_eq!(result.source_fingerprint.len(), 64);
        assert_eq!(
            provider.commands.lock().unwrap().last().map(String::as_str),
            Some("cargo test --locked")
        );
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn network_policy_fingerprint_is_order_independent_and_default_deny() {
        let left = network_policy_fingerprint(&[
            "github.com".into(),
            "static.crates.io".into(),
            "crates.io".into(),
        ])
        .unwrap();
        let right = network_policy_fingerprint(&[
            "crates.io".into(),
            "github.com".into(),
            "static.crates.io".into(),
        ])
        .unwrap();
        assert_eq!(left, right);
        assert_eq!(
            left,
            "213a43f5b7ef245ed06c6cd6df630307543e494a37feffa3765d89a9199706d5"
        );
    }

    #[test]
    fn runtime_network_policy_is_distinct_from_repository_requirements() {
        let deployed = vec![
            "crates.io".to_string(),
            "github.com".to_string(),
            "registry.npmjs.org".to_string(),
        ];
        let definition = definition();
        assert!(
            definition
                .network_allowlist
                .iter()
                .all(|domain| deployed.contains(domain))
        );
        assert_ne!(
            network_policy_fingerprint(&deployed).unwrap(),
            network_policy_fingerprint(&definition.network_allowlist).unwrap()
        );
    }

    #[test]
    fn deployed_network_allowlist_is_bounded_sorted_and_exact() {
        assert_eq!(
            parse_deployed_network_allowlist(" github.com,crates.io,github.com ").unwrap(),
            ["crates.io", "github.com"]
        );
        for invalid in [
            "",
            "*",
            "https://crates.io",
            "crates.io:443",
            "crates.io,",
            "CRATES.IO",
            "-crates.io",
            "crates..io",
            "169.254.169.254",
        ] {
            assert!(parse_deployed_network_allowlist(invalid).is_err());
        }
    }

    #[test]
    fn escaping_symlink_is_refused() {
        assert!(
            validate_symlink_target(Path::new("src/link"), Path::new("../../outside")).is_err()
        );
        assert!(validate_symlink_target(Path::new("src/link"), Path::new("../shared.rs")).is_ok());
    }

    #[test]
    fn credential_shaped_source_paths_are_refused() {
        assert!(sensitive_source_path(Path::new(".env.local")));
        assert!(sensitive_source_path(Path::new("config/.aws/credentials")));
        assert!(!sensitive_source_path(Path::new("src/environment.rs")));
    }

    #[test]
    #[ignore = "requires a live MDX_AGENTCORE_RUNTIME_ARN and AWS session"]
    fn live_agentcore_repository_acceptance() {
        assert_eq!(
            std::env::var("MDX_AGENTCORE_LIVE_REPOSITORY_ACCEPTANCE").as_deref(),
            Ok("1"),
            "set MDX_AGENTCORE_LIVE_REPOSITORY_ACCEPTANCE=1 to acknowledge the paid live acceptance run"
        );
        let root = std::env::var("MDX_AGENTCORE_ACCEPTANCE_ROOT")
            .map(std::path::PathBuf::from)
            .expect("MDX_AGENTCORE_ACCEPTANCE_ROOT must name a prepared Git checkout");
        let definition = load_environment(&root).expect("prepared environment definition");
        let provider = LiveAgentCoreProvider::from_environment().expect("live AgentCore provider");
        let result = verify_environment_with_provider(&root, &definition, &provider)
            .expect("live repository environment verification");
        assert_eq!(result.provider, "aws_agentcore");
        assert_eq!(result.source_fingerprint.len(), 64);
        assert_eq!(result.environment_fingerprint.len(), 64);
        assert_eq!(result.setup_count, definition.setup_commands.len());
        assert_eq!(result.proof_count, definition.proof_commands.len());
    }

    #[test]
    #[ignore = "requires a live MDX_AGENTCORE_RUNTIME_ARN and AWS session"]
    fn live_agentcore_multi_lane_canary() {
        assert_eq!(
            std::env::var("MDX_AGENTCORE_LIVE_MULTI_LANE_CANARY").as_deref(),
            Ok("1"),
            "set MDX_AGENTCORE_LIVE_MULTI_LANE_CANARY=1 to acknowledge the paid live canary"
        );
        let width = std::env::var("MDX_AGENTCORE_LIVE_MULTI_LANE_WIDTH")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(4);
        assert!(
            (2..=8).contains(&width),
            "live canary width must be 2 through 8"
        );
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "mdx-agentcore-live-multi-lane-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/main.rs"),
            "fn main() { println!(\"live multi-lane canary\"); }\n",
        )
        .unwrap();
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["init", "--quiet"])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["add", "src/main.rs"])
                .status()
                .unwrap()
                .success()
        );

        let mut live_definition = definition();
        live_definition.base_image = std::env::var("MDX_AGENTCORE_RUNTIME_IMAGE").unwrap();
        live_definition.network_allowlist.clear();
        let environment_fingerprint = environment_fingerprint(&live_definition).unwrap();
        let sessions = (0..width)
            .map(|lane| runtime_session_id(&format!("live-fleet-canary-{nonce}-lane-{lane}")))
            .collect::<Vec<_>>();
        assert_eq!(
            sessions
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            width
        );
        let providers = (0..width)
            .map(|_| LiveAgentCoreProvider::from_environment().unwrap())
            .collect::<Vec<_>>();
        for (provider, session_id) in providers.iter().zip(&sessions) {
            sync_workspace(
                provider,
                session_id,
                &root,
                &live_definition,
                &environment_fingerprint,
            )
            .unwrap();
        }

        std::thread::scope(|scope| {
            let handles = sessions
                .iter()
                .zip(&providers)
                .enumerate()
                .map(|(lane, (session_id, provider))| {
                    scope.spawn(move || {
                        let command = format!(
                            "test -f src/main.rs && rustc --version && printf 'lane-{lane}\\n'"
                        );
                        provider.execute_command(session_id, &command, 180)
                    })
                })
                .collect::<Vec<_>>();
            for (lane, handle) in handles.into_iter().enumerate() {
                let result = handle.join().expect("live lane thread").unwrap();
                assert!(result.completed && result.exit_code == 0);
                assert!(result.output.contains(&format!("lane-{lane}")));
            }
        });
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    #[ignore = "requires a live MDX_AGENTCORE_RUNTIME_ARN and AWS session"]
    fn live_agentcore_runtime_canary() {
        assert_eq!(
            std::env::var("MDX_AGENTCORE_LIVE_CANARY").as_deref(),
            Ok("1"),
            "set MDX_AGENTCORE_LIVE_CANARY=1 to acknowledge the paid live canary"
        );
        let root = std::env::temp_dir().join(format!(
            "mdx-agentcore-live-canary-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/main.rs"),
            "fn main() { println!(\"live AgentCore canary\"); }\n",
        )
        .unwrap();
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["init", "--quiet"])
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(["add", "src/main.rs"])
                .status()
                .unwrap()
                .success()
        );
        let mut live_definition = definition();
        live_definition.base_image = std::env::var("MDX_AGENTCORE_RUNTIME_IMAGE").unwrap();
        live_definition.network_allowlist = vec![
            "crates.io".into(),
            "github.com".into(),
            "registry.npmjs.org".into(),
        ];
        let provider = LiveAgentCoreProvider::from_environment().unwrap();
        let environment_fingerprint = environment_fingerprint(&live_definition).unwrap();
        let session_id = runtime_session_id(&format!(
            "live-canary-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source_fingerprint = sync_workspace(
            &provider,
            &session_id,
            &root,
            &live_definition,
            &environment_fingerprint,
        )
        .unwrap();
        assert_eq!(source_fingerprint.len(), 64);

        for command in [
            "test -f src/main.rs && rustc --version && cargo --version && node --version && npm --version && python3 --version && python3 -m pip --version && git --version",
            "mkdir -p target && rustc src/main.rs -o target/mdx-agentcore-canary && test \"$(./target/mdx-agentcore-canary)\" = 'live AgentCore canary'",
            "test -n \"${HTTPS_PROXY:-}\" && cargo search serde --limit 1",
            "npm view react version",
            "mkdir -p .mdx-runtime/pip-canary && python3 -m pip download --no-deps --dest .mdx-runtime/pip-canary idna==3.10",
            "git ls-remote https://github.com/rust-lang/cargo HEAD",
            "if git ls-remote https://example.com/not-allowed.git; then exit 90; else echo network-deny-confirmed; fi",
            "test -z \"${OPENAI_API_KEY:-}${ANTHROPIC_API_KEY:-}${XAI_API_KEY:-}${GITHUB_TOKEN:-}${AWS_ACCESS_KEY_ID:-}${AWS_SECRET_ACCESS_KEY:-}${AWS_SESSION_TOKEN:-}${AWS_WEB_IDENTITY_TOKEN_FILE:-}${AWS_CONTAINER_CREDENTIALS_RELATIVE_URI:-}${AWS_CONTAINER_CREDENTIALS_FULL_URI:-}\"",
            "if python3 -c \"import urllib.request; urllib.request.urlopen('http://169.254.169.254/latest/meta-data/iam/security-credentials/', timeout=3)\"; then exit 91; else echo metadata-deny-confirmed; fi",
        ] {
            let result = provider.execute_command(&session_id, command, 300).unwrap();
            assert!(
                result.completed && result.exit_code == 0,
                "live canary command failed: {command}: {}",
                result.output
            );
        }

        let mutation = provider
            .execute_command(&session_id, "printf unsafe > src/new.txt", 60)
            .unwrap();
        assert_eq!(mutation.exit_code, 86);
        assert!(mutation.output.contains("MDX_UNOBSERVED_SOURCE_MUTATION"));
        sync_workspace(
            &provider,
            &session_id,
            &root,
            &live_definition,
            &environment_fingerprint,
        )
        .unwrap();
        let cleanup = provider
            .execute_command(&session_id, "test ! -e src/new.txt", 60)
            .unwrap();
        assert!(cleanup.completed && cleanup.exit_code == 0);
        let _ = fs::remove_dir_all(root);
    }
}
