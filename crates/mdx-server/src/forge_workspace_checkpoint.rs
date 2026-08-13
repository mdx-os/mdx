//! Crash-recoverable Forge workspace checkpoints.
//!
//! A detached worktree is excellent isolation but poor durable state. This
//! module turns only a run's governed write scope into an immutable Git commit
//! held by a hidden ref. Private metadata names the latest complete ref.
//! Publication order leaves either the prior valid checkpoint or an orphan ref
//! that startup reconciliation can collect, never half-trusted resumable state.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

const CHECKPOINT_SCHEMA: &str = "mdx.forge.workspace-checkpoint.v1";
const CHECKPOINTS_SUBDIR: &str = "mdx/forge-workspace-checkpoints";
pub(crate) const CHECKPOINT_REF_PREFIX: &str = "refs/mdx/forge-checkpoints";
const DEFAULT_MAX_CHECKPOINT_PATHS: usize = 512;
const DEFAULT_MAX_CHECKPOINT_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(unix)]
const DISABLED_GIT_HOOKS_PATH: &str = "/dev/null";
#[cfg(windows)]
const DISABLED_GIT_HOOKS_PATH: &str = "NUL";
#[cfg(not(any(unix, windows)))]
const DISABLED_GIT_HOOKS_PATH: &str = "";
static CHECKPOINT_NONCE: AtomicU64 = AtomicU64::new(0);
static CHECKPOINT_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct CheckpointMetadata {
    schema: String,
    run_id: String,
    repository_root: String,
    checkpoint_ref: String,
    checkpoint_sha: String,
    parent_sha: String,
    origin_ref: Option<String>,
    write_scope: Vec<String>,
    paths: Vec<String>,
    sequence: u64,
    created_unix_seconds: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WorkspaceCheckpoint {
    pub(crate) checkpoint_ref: String,
    pub(crate) checkpoint_sha: String,
    pub(crate) parent_sha: String,
    pub(crate) paths: Vec<String>,
    pub(crate) sequence: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CheckpointReconciliation {
    pub(crate) valid_checkpoints: usize,
    pub(crate) quarantined_metadata: usize,
    pub(crate) orphan_refs_removed: usize,
    pub(crate) stale_temp_files_removed: usize,
    pub(crate) errors: Vec<String>,
}

impl From<&CheckpointMetadata> for WorkspaceCheckpoint {
    fn from(value: &CheckpointMetadata) -> Self {
        Self {
            checkpoint_ref: value.checkpoint_ref.clone(),
            checkpoint_sha: value.checkpoint_sha.clone(),
            parent_sha: value.parent_sha.clone(),
            paths: value.paths.clone(),
            sequence: value.sequence,
        }
    }
}

/// Publish the workspace's current in-scope state. If the in-scope diff is
/// empty, clear any prior checkpoint so resume cannot resurrect reverted work.
pub(crate) fn publish(
    repo_root: &Path,
    workspace: &Path,
    run_id: &str,
    write_scope: &[String],
    origin_ref: Option<&str>,
    sequence: u64,
) -> Result<Option<WorkspaceCheckpoint>, String> {
    let _guard = CHECKPOINT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let run_id = validated_run_id(run_id)?;
    let write_scope = normalize_scope(write_scope)?;
    let repository_root = repository_identity(repo_root)?;
    let parent_sha = git_text(workspace, &["rev-parse", "HEAD"])?;
    let paths = changed_paths(workspace, &write_scope)?;
    if paths.is_empty() {
        clear_locked(repo_root, &run_id)?;
        return Ok(None);
    }
    validate_checkpoint_footprint(
        workspace,
        &paths,
        checkpoint_max_paths(),
        checkpoint_max_bytes(),
    )?;

    let checkpoints_dir = checkpoint_directory(repo_root)?;
    std::fs::create_dir_all(&checkpoints_dir).map_err(|error| error.to_string())?;
    let nonce = CHECKPOINT_NONCE.fetch_add(1, Ordering::Relaxed);
    let index_path =
        checkpoints_dir.join(format!(".{run_id}.{}.{}.index", std::process::id(), nonce));
    let result = (|| {
        git_with_index(workspace, &index_path, &["read-tree", "HEAD"])?;
        let mut add_args = vec!["add".to_string(), "-A".to_string(), "--".to_string()];
        add_args.extend(paths.iter().cloned());
        git_owned_with_index(workspace, &index_path, &add_args)?;
        let tree_sha = git_text_with_index(workspace, &index_path, &["write-tree"])?;
        let parent_tree = git_text(workspace, &["rev-parse", "HEAD^{tree}"])?;
        if tree_sha == parent_tree {
            clear_locked(repo_root, &run_id)?;
            return Ok(None);
        }
        let message = format!("[forge] Workspace checkpoint for {run_id}");
        let checkpoint_sha = git_text(
            workspace,
            &[
                "-c",
                "user.name=Forge Checkpoint",
                "-c",
                "user.email=forge-checkpoint@mdx.local",
                "commit-tree",
                &tree_sha,
                "-p",
                &parent_sha,
                "-m",
                &message,
            ],
        )?;
        let checkpoint_ref = format!(
            "{CHECKPOINT_REF_PREFIX}/{run_id}/{}-{nonce}",
            std::process::id()
        );
        git_ok(repo_root, &["update-ref", &checkpoint_ref, &checkpoint_sha])?;

        let prior = load_metadata_if_present(repo_root, &run_id)?;
        let metadata = CheckpointMetadata {
            schema: CHECKPOINT_SCHEMA.to_string(),
            run_id: run_id.clone(),
            repository_root,
            checkpoint_ref: checkpoint_ref.clone(),
            checkpoint_sha: checkpoint_sha.clone(),
            parent_sha: parent_sha.clone(),
            origin_ref: origin_ref.map(ToString::to_string),
            write_scope,
            paths: paths.clone(),
            sequence,
            created_unix_seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        };
        if let Err(error) = write_metadata_atomically(repo_root, &metadata) {
            let _ = delete_ref(repo_root, &checkpoint_ref, &checkpoint_sha);
            return Err(error);
        }
        if let Some(prior) = prior
            && prior.checkpoint_ref != checkpoint_ref
        {
            let _ = delete_ref(repo_root, &prior.checkpoint_ref, &prior.checkpoint_sha);
        }
        Ok(Some(WorkspaceCheckpoint::from(&metadata)))
    })();
    let _ = std::fs::remove_file(index_path);
    result
}

/// Load and verify the latest complete checkpoint. Metadata grants no
/// authority: repository identity, exact write scope, hidden ref target,
/// commit parent, origin ref, and every changed path must all match.
pub(crate) fn recover(
    repo_root: &Path,
    run_id: &str,
    write_scope: &[String],
    origin_ref: Option<&str>,
) -> Result<Option<WorkspaceCheckpoint>, String> {
    let _guard = CHECKPOINT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let run_id = validated_run_id(run_id)?;
    let expected_scope = normalize_scope(write_scope)?;
    let Some(metadata) = load_metadata_if_present(repo_root, &run_id)? else {
        return Ok(None);
    };
    if metadata.schema != CHECKPOINT_SCHEMA || metadata.run_id != run_id {
        return Err("workspace checkpoint metadata has the wrong schema or run id".to_string());
    }
    if metadata.repository_root != repository_identity(repo_root)? {
        return Err("workspace checkpoint belongs to a different repository".to_string());
    }
    if metadata.write_scope != expected_scope {
        return Err("workspace checkpoint write scope does not match this resume".to_string());
    }
    if metadata.origin_ref.as_deref() != origin_ref {
        return Err("workspace checkpoint origin ref does not match this resume".to_string());
    }
    if !metadata
        .checkpoint_ref
        .starts_with(&format!("{CHECKPOINT_REF_PREFIX}/{run_id}/"))
    {
        return Err("workspace checkpoint ref escaped its run prefix".to_string());
    }
    let resolved = git_text(
        repo_root,
        &["rev-parse", "--verify", &metadata.checkpoint_ref],
    )?;
    if resolved != metadata.checkpoint_sha {
        return Err("workspace checkpoint ref no longer names its recorded commit".to_string());
    }
    let parent = git_text(
        repo_root,
        &["rev-parse", &format!("{}^", metadata.checkpoint_sha)],
    )?;
    if parent != metadata.parent_sha {
        return Err("workspace checkpoint parent does not match its metadata".to_string());
    }
    if let Some(origin_ref) = origin_ref {
        let origin_tip = git_text(repo_root, &["rev-parse", "--verify", origin_ref])?;
        if origin_tip != metadata.parent_sha {
            return Err("the branch being revised advanced after the checkpoint".to_string());
        }
    }
    let actual_paths = diff_paths(repo_root, &metadata.parent_sha, &metadata.checkpoint_sha)?;
    if actual_paths != metadata.paths {
        return Err("workspace checkpoint path manifest does not match its commit".to_string());
    }
    if actual_paths
        .iter()
        .any(|path| !path_in_scope(path, &expected_scope))
    {
        return Err("workspace checkpoint contains an out-of-scope path".to_string());
    }
    Ok(Some(WorkspaceCheckpoint::from(&metadata)))
}

/// Reapply a verified checkpoint as an uncommitted diff on its parent so the
/// eventual review branch does not inherit internal checkpoint commits.
pub(crate) fn apply(workspace: &Path, checkpoint: &WorkspaceCheckpoint) -> Result<(), String> {
    let head = git_text(workspace, &["rev-parse", "HEAD"])?;
    if head != checkpoint.parent_sha {
        return Err("checkpoint recovery worktree is not at the recorded parent".to_string());
    }
    let diff = git_bytes(
        workspace,
        &[
            "diff",
            "--binary",
            &checkpoint.parent_sha,
            &checkpoint.checkpoint_sha,
            "--",
        ],
    )?;
    if diff.is_empty() {
        return Err("checkpoint recovery diff was empty".to_string());
    }
    git_stdin_ok(
        workspace,
        &["apply", "--binary", "--whitespace=nowarn"],
        &diff,
    )?;
    if changed_paths(workspace, &[])? != checkpoint.paths {
        return Err("rehydrated workspace does not match the checkpoint manifest".to_string());
    }
    Ok(())
}

/// Remove metadata before its ref. Interruption leaves only an orphan ref,
/// never discoverable metadata that points to deleted state.
pub(crate) fn clear(repo_root: &Path, run_id: &str) -> Result<(), String> {
    let _guard = CHECKPOINT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let run_id = validated_run_id(run_id)?;
    clear_locked(repo_root, &run_id)
}

pub(crate) fn exists(repo_root: &Path, run_id: &str) -> Result<bool, String> {
    let run_id = validated_run_id(run_id)?;
    Ok(metadata_path(repo_root, &run_id)?.exists())
}

/// Reconcile durable checkpoint state after a process restart. Valid metadata
/// remains available for an explicit resume. Invalid metadata is quarantined,
/// refs not named by any valid metadata are removed, and interrupted temp/index
/// files are removed only when their recorded owner process is no longer live.
pub(crate) fn reconcile_repository(repo_root: &Path) -> CheckpointReconciliation {
    let mut report = CheckpointReconciliation::default();
    if !repo_root.join(".git").exists() {
        report
            .errors
            .push(format!("{} is not a Git repository", repo_root.display()));
        return report;
    }
    let directory = match checkpoint_directory(repo_root) {
        Ok(directory) => directory,
        Err(error) => {
            report.errors.push(error);
            return report;
        }
    };
    let mut valid_refs = BTreeSet::new();
    if directory.exists() {
        match std::fs::read_dir(&directory) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if !path.is_file() {
                        continue;
                    }
                    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                        continue;
                    };
                    if name.ends_with(".tmp") || name.ends_with(".index") {
                        if temp_owner_is_dead(name) {
                            match std::fs::remove_file(&path) {
                                Ok(()) => report.stale_temp_files_removed += 1,
                                Err(error) => report.errors.push(format!(
                                    "could not remove stale checkpoint temp {}: {error}",
                                    path.display()
                                )),
                            }
                        }
                        continue;
                    }
                    let Some(run_id) = name.strip_suffix(".json") else {
                        continue;
                    };
                    let metadata = load_metadata_if_present(repo_root, run_id);
                    let validation = metadata.and_then(|metadata| match metadata {
                        Some(metadata) => recover(
                            repo_root,
                            run_id,
                            &metadata.write_scope,
                            metadata.origin_ref.as_deref(),
                        ),
                        None => Err("checkpoint metadata disappeared during reconciliation".into()),
                    });
                    match validation {
                        Ok(Some(checkpoint)) => {
                            report.valid_checkpoints += 1;
                            valid_refs.insert(checkpoint.checkpoint_ref);
                        }
                        Ok(None) => {}
                        Err(error) => match quarantine_metadata(repo_root, &path) {
                            Ok(()) => {
                                report.quarantined_metadata += 1;
                                report.errors.push(format!(
                                    "quarantined invalid checkpoint metadata {name}: {error}"
                                ));
                            }
                            Err(quarantine_error) => report.errors.push(format!(
                                "invalid checkpoint metadata {name}: {error}; quarantine failed: {quarantine_error}"
                            )),
                        },
                    }
                }
            }
            Err(error) => report.errors.push(format!(
                "could not scan checkpoint directory {}: {error}",
                directory.display()
            )),
        }
    }

    match checkpoint_refs(repo_root) {
        Ok(refs) => {
            for (checkpoint_ref, sha) in refs {
                if valid_refs.contains(&checkpoint_ref) {
                    continue;
                }
                match delete_ref(repo_root, &checkpoint_ref, &sha) {
                    Ok(()) => report.orphan_refs_removed += 1,
                    Err(error) => report.errors.push(format!(
                        "could not remove orphan checkpoint ref {checkpoint_ref}: {error}"
                    )),
                }
            }
        }
        Err(error) => report
            .errors
            .push(format!("could not enumerate checkpoint refs: {error}")),
    }
    if directory.exists() {
        let _ = sync_dir(&directory);
    }
    report
}

fn clear_locked(repo_root: &Path, run_id: &str) -> Result<(), String> {
    let prior = load_metadata_if_present(repo_root, run_id)?;
    let path = metadata_path(repo_root, run_id)?;
    if path.exists() {
        std::fs::remove_file(&path).map_err(|error| error.to_string())?;
        sync_dir(&checkpoint_directory(repo_root)?)?;
    }
    if let Some(prior) = prior {
        delete_ref(repo_root, &prior.checkpoint_ref, &prior.checkpoint_sha)?;
    }
    Ok(())
}

fn checkpoint_refs(repo_root: &Path) -> Result<Vec<(String, String)>, String> {
    let output = git_output(
        repo_root,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            CHECKPOINT_REF_PREFIX,
        ],
        None,
        None,
    )?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|_| "git returned non-UTF-8 checkpoint refs".to_string())?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (reference, sha) = line
                .split_once(' ')
                .ok_or_else(|| "git returned a malformed checkpoint ref".to_string())?;
            Ok((reference.to_string(), sha.to_string()))
        })
        .collect()
}

fn quarantine_metadata(repo_root: &Path, path: &Path) -> Result<(), String> {
    let quarantine = checkpoint_directory(repo_root)?.join("quarantine");
    std::fs::create_dir_all(&quarantine).map_err(|error| error.to_string())?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "checkpoint metadata has no UTF-8 file name".to_string())?;
    let nonce = CHECKPOINT_NONCE.fetch_add(1, Ordering::Relaxed);
    let target = quarantine.join(format!("{name}.{}.{}.invalid", std::process::id(), nonce));
    std::fs::rename(path, target).map_err(|error| error.to_string())?;
    sync_dir(&checkpoint_directory(repo_root)?)
}

fn temp_owner_is_dead(name: &str) -> bool {
    let mut parts = name.trim_start_matches('.').rsplit('.');
    let _suffix = parts.next();
    let _nonce = parts.next();
    let Some(pid) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
        return false;
    };
    !process_is_alive(pid)
}

#[cfg(unix)]
pub(crate) fn process_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let result = unsafe { libc::kill(pid as i32, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
pub(crate) fn process_is_alive(_pid: u32) -> bool {
    // Without a portable process liveness primitive, preserve state. A stale
    // artifact is safer than deleting another server's live workspace.
    true
}

fn changed_paths(workspace: &Path, write_scope: &[String]) -> Result<Vec<String>, String> {
    let mut paths = BTreeSet::new();
    let tracked = git_bytes(
        workspace,
        &["diff", "--no-renames", "--name-only", "-z", "HEAD", "--"],
    )?;
    let staged = git_bytes(
        workspace,
        &[
            "diff",
            "--cached",
            "--no-renames",
            "--name-only",
            "-z",
            "HEAD",
            "--",
        ],
    )?;
    let untracked = git_bytes(
        workspace,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    for raw in [tracked, staged, untracked] {
        for path in split_nul_paths(&raw)? {
            if path_in_scope(&path, write_scope) {
                paths.insert(path);
            }
        }
    }
    Ok(paths.into_iter().collect())
}

fn validate_checkpoint_footprint(
    workspace: &Path,
    paths: &[String],
    max_paths: usize,
    max_bytes: u64,
) -> Result<(), String> {
    if paths.len() > max_paths {
        return Err(format!(
            "workspace checkpoint has {} paths; the limit is {max_paths}",
            paths.len()
        ));
    }
    let mut bytes = 0u64;
    for path in paths {
        let full_path = workspace.join(path);
        let Ok(metadata) = std::fs::symlink_metadata(&full_path) else {
            // A deletion carries no current workspace bytes.
            continue;
        };
        if metadata.is_file() || metadata.file_type().is_symlink() {
            bytes = bytes.saturating_add(metadata.len());
        }
        if bytes > max_bytes {
            return Err(format!(
                "workspace checkpoint has more than {max_bytes} current bytes"
            ));
        }
    }
    Ok(())
}

fn checkpoint_max_paths() -> usize {
    std::env::var("MDX_FORGE_CHECKPOINT_MAX_PATHS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_CHECKPOINT_PATHS)
        .min(4096)
}

fn checkpoint_max_bytes() -> u64 {
    std::env::var("MDX_FORGE_CHECKPOINT_MAX_BYTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_CHECKPOINT_BYTES)
        .min(1024 * 1024 * 1024)
}

fn diff_paths(repo_root: &Path, parent: &str, checkpoint: &str) -> Result<Vec<String>, String> {
    let raw = git_bytes(
        repo_root,
        &["diff", "--name-only", "-z", parent, checkpoint, "--"],
    )?;
    let mut paths = split_nul_paths(&raw)?;
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn split_nul_paths(raw: &[u8]) -> Result<Vec<String>, String> {
    raw.split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            let path = std::str::from_utf8(path)
                .map_err(|_| "workspace contains a non-UTF-8 path".to_string())?;
            validate_relative_path(path)
        })
        .collect()
}

fn normalize_scope(write_scope: &[String]) -> Result<Vec<String>, String> {
    let mut normalized = BTreeSet::new();
    for path in write_scope {
        let path = path.trim_start_matches("./").trim_end_matches('/');
        if path.is_empty() {
            return Err("workspace checkpoint scope contains an empty path".to_string());
        }
        normalized.insert(validate_relative_path(path)?);
    }
    Ok(normalized.into_iter().collect())
}

fn validate_relative_path(path: &str) -> Result<String, String> {
    let candidate = Path::new(path);
    if candidate.is_absolute()
        || candidate.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("workspace checkpoint path is not confined: {path}"));
    }
    Ok(path
        .trim_start_matches("./")
        .trim_end_matches('/')
        .to_string())
}

fn path_in_scope(path: &str, write_scope: &[String]) -> bool {
    write_scope.is_empty()
        || write_scope
            .iter()
            .any(|entry| path == entry || path.starts_with(&format!("{entry}/")))
}

fn validated_run_id(run_id: &str) -> Result<String, String> {
    if run_id.is_empty()
        || run_id.len() > 160
        || !run_id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err("run id cannot name a workspace checkpoint".to_string());
    }
    Ok(run_id.to_string())
}

fn repository_identity(repo_root: &Path) -> Result<String, String> {
    git_common_directory(repo_root).map(|path| path.to_string_lossy().into_owned())
}

fn git_common_directory(repo_root: &Path) -> Result<PathBuf, String> {
    let raw = git_text(repo_root, &["rev-parse", "--git-common-dir"])?;
    let path = PathBuf::from(raw);
    let path = if path.is_absolute() {
        path
    } else {
        repo_root.join(path)
    };
    path.canonicalize().map_err(|error| error.to_string())
}

fn checkpoint_directory(repo_root: &Path) -> Result<PathBuf, String> {
    Ok(git_common_directory(repo_root)?.join(CHECKPOINTS_SUBDIR))
}

fn metadata_path(repo_root: &Path, run_id: &str) -> Result<PathBuf, String> {
    Ok(checkpoint_directory(repo_root)?.join(format!("{run_id}.json")))
}

fn load_metadata_if_present(
    repo_root: &Path,
    run_id: &str,
) -> Result<Option<CheckpointMetadata>, String> {
    let path = metadata_path(repo_root, run_id)?;
    if !path.exists() {
        return Ok(None);
    }
    let body = std::fs::read(path).map_err(|error| error.to_string())?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| format!("invalid workspace checkpoint metadata: {error}"))
}

fn write_metadata_atomically(
    repo_root: &Path,
    metadata: &CheckpointMetadata,
) -> Result<(), String> {
    let directory = checkpoint_directory(repo_root)?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let final_path = metadata_path(repo_root, &metadata.run_id)?;
    let nonce = CHECKPOINT_NONCE.fetch_add(1, Ordering::Relaxed);
    let temp_path = directory.join(format!(
        ".{}.{}.{}.tmp",
        metadata.run_id,
        std::process::id(),
        nonce
    ));
    let body = serde_json::to_vec_pretty(metadata).map_err(|error| error.to_string())?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> std::io::Result<()> {
        let mut file = options.open(&temp_path)?;
        file.write_all(&body)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp_path, &final_path)?;
        sync_dir_io(&directory)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result.map_err(|error| error.to_string())
}

fn sync_dir(path: &Path) -> Result<(), String> {
    sync_dir_io(path).map_err(|error| error.to_string())
}

#[cfg(unix)]
fn sync_dir_io(path: &Path) -> std::io::Result<()> {
    std::fs::File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_dir_io(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

fn delete_ref(repo_root: &Path, checkpoint_ref: &str, sha: &str) -> Result<(), String> {
    git_ok(repo_root, &["update-ref", "-d", checkpoint_ref, sha])
}

fn git_ok(root: &Path, args: &[&str]) -> Result<(), String> {
    let output = git_output(root, args, None, None)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn git_text(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = git_output(root, args, None, None)?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    String::from_utf8(output.stdout)
        .map(|text| text.trim().to_string())
        .map_err(|_| "git returned non-UTF-8 text".to_string())
}

fn git_bytes(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = git_output(root, args, None, None)?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn git_stdin_ok(root: &Path, args: &[&str], stdin: &[u8]) -> Result<(), String> {
    let output = git_output(root, args, Some(stdin), None)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn git_with_index(root: &Path, index: &Path, args: &[&str]) -> Result<(), String> {
    let output = git_output(root, args, None, Some(index))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn git_text_with_index(root: &Path, index: &Path, args: &[&str]) -> Result<String, String> {
    let output = git_output(root, args, None, Some(index))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    String::from_utf8(output.stdout)
        .map(|text| text.trim().to_string())
        .map_err(|_| "git returned non-UTF-8 text".to_string())
}

fn git_owned_with_index(root: &Path, index: &Path, args: &[String]) -> Result<(), String> {
    let output = Command::new("git")
        .args(["-c", &format!("core.hooksPath={DISABLED_GIT_HOOKS_PATH}")])
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_INDEX_FILE", index)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn git_output(
    root: &Path,
    args: &[&str],
    stdin: Option<&[u8]>,
    index: Option<&Path>,
) -> Result<std::process::Output, String> {
    let mut command = Command::new("git");
    command
        .args(["-c", &format!("core.hooksPath={DISABLED_GIT_HOOKS_PATH}")])
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0");
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    if let Some(stdin) = stdin {
        child
            .stdin
            .as_mut()
            .ok_or_else(|| "git stdin was not piped".to_string())?
            .write_all(stdin)
            .map_err(|error| error.to_string())?;
    }
    child.wait_with_output().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_contains_only_the_exact_write_scope() {
        let repo = temp_git_repo("scope");
        write(&repo.join("src/allowed.rs"), b"allowed v2\n");
        write(&repo.join("src/outside.rs"), b"outside v2\n");
        let checkpoint = publish(
            &repo,
            &repo,
            "forge_checkpoint_scope_test",
            &["src/allowed.rs".to_string()],
            None,
            1,
        )
        .expect("published")
        .expect("checkpoint");
        assert_eq!(checkpoint.paths, vec!["src/allowed.rs"]);
        assert_eq!(
            diff_paths(&repo, &checkpoint.parent_sha, &checkpoint.checkpoint_sha)
                .expect("diff paths"),
            vec!["src/allowed.rs"]
        );
        clear(&repo, "forge_checkpoint_scope_test").expect("cleared");
        cleanup(&repo);
    }

    #[test]
    fn checkpoint_rehydrates_modified_deleted_and_binary_files() {
        let repo = temp_git_repo("rehydrate");
        write(&repo.join("src/allowed.rs"), b"allowed v2\n");
        std::fs::remove_file(repo.join("src/outside.rs")).expect("delete tracked file");
        write(&repo.join("src/payload.bin"), &[0, 1, 2, 0xff, 4]);
        let checkpoint = publish(
            &repo,
            &repo,
            "forge_checkpoint_rehydrate_test",
            &["src".to_string()],
            None,
            7,
        )
        .expect("published")
        .expect("checkpoint");
        let recovered = recover(
            &repo,
            "forge_checkpoint_rehydrate_test",
            &["src".to_string()],
            None,
        )
        .expect("valid recovery")
        .expect("checkpoint exists");
        assert_eq!(recovered, checkpoint);
        let workspace = repo.with_extension("recovery-worktree");
        git_ok(
            &repo,
            &[
                "worktree",
                "add",
                "--detach",
                workspace.to_str().expect("utf8 path"),
                &checkpoint.parent_sha,
            ],
        )
        .expect("recovery worktree");
        apply(&workspace, &recovered).expect("rehydrated");
        assert_eq!(
            std::fs::read(workspace.join("src/allowed.rs")).expect("allowed"),
            b"allowed v2\n"
        );
        assert!(!workspace.join("src/outside.rs").exists());
        assert_eq!(
            std::fs::read(workspace.join("src/payload.bin")).expect("binary"),
            [0, 1, 2, 0xff, 4]
        );
        git_ok(
            &repo,
            &[
                "worktree",
                "remove",
                "--force",
                workspace.to_str().expect("utf8 path"),
            ],
        )
        .expect("remove recovery worktree");
        clear(&repo, "forge_checkpoint_rehydrate_test").expect("cleared");
        cleanup(&repo);
    }

    #[test]
    fn recovery_refuses_a_changed_scope() {
        let repo = temp_git_repo("scope_mismatch");
        write(&repo.join("src/allowed.rs"), b"allowed v2\n");
        publish(
            &repo,
            &repo,
            "forge_checkpoint_scope_mismatch_test",
            &["src/allowed.rs".to_string()],
            None,
            1,
        )
        .expect("published");
        let error = recover(
            &repo,
            "forge_checkpoint_scope_mismatch_test",
            &["src".to_string()],
            None,
        )
        .expect_err("scope mismatch must refuse");
        assert!(error.contains("write scope"));
        clear(&repo, "forge_checkpoint_scope_mismatch_test").expect("cleared");
        cleanup(&repo);
    }

    #[test]
    fn replacing_then_reverting_keeps_only_current_state() {
        let repo = temp_git_repo("replace");
        write(&repo.join("src/allowed.rs"), b"allowed v2\n");
        let first = publish(
            &repo,
            &repo,
            "forge_checkpoint_replace_test",
            &["src".to_string()],
            None,
            1,
        )
        .expect("first")
        .expect("first checkpoint");
        write(&repo.join("src/allowed.rs"), b"allowed v3\n");
        let second = publish(
            &repo,
            &repo,
            "forge_checkpoint_replace_test",
            &["src".to_string()],
            None,
            2,
        )
        .expect("second")
        .expect("second checkpoint");
        assert_ne!(first.checkpoint_ref, second.checkpoint_ref);
        assert!(
            !git_output(
                &repo,
                &["rev-parse", "--verify", &first.checkpoint_ref],
                None,
                None,
            )
            .expect("git")
            .status
            .success()
        );
        assert_eq!(
            recover(
                &repo,
                "forge_checkpoint_replace_test",
                &["src".to_string()],
                None,
            )
            .expect("recover"),
            Some(second)
        );
        write(&repo.join("src/allowed.rs"), b"allowed v1\n");
        assert_eq!(
            publish(
                &repo,
                &repo,
                "forge_checkpoint_replace_test",
                &["src".to_string()],
                None,
                3,
            )
            .expect("revert publication"),
            None
        );
        assert!(!exists(&repo, "forge_checkpoint_replace_test").expect("exists"));
        cleanup(&repo);
    }

    #[test]
    fn startup_reconciliation_preserves_valid_state_and_collects_orphans() {
        let repo = temp_git_repo("reconcile");
        write(&repo.join("src/allowed.rs"), b"allowed v2\n");
        let run_id = "forge_checkpoint_reconcile_test";
        let checkpoint = publish(&repo, &repo, run_id, &["src".to_string()], None, 1)
            .expect("published")
            .expect("checkpoint");
        let orphan_ref = format!("{CHECKPOINT_REF_PREFIX}/orphan/1");
        let head = git_text(&repo, &["rev-parse", "HEAD"]).expect("head");
        git_ok(&repo, &["update-ref", &orphan_ref, &head]).expect("orphan ref");
        let directory = checkpoint_directory(&repo).expect("checkpoint directory");
        write(
            &directory.join("forge_invalid_checkpoint.json"),
            b"{not-json",
        );
        let dead_temp = directory.join(".dead.99999999.1.tmp");
        let live_temp = directory.join(format!(".live.{}.2.tmp", std::process::id()));
        write(&dead_temp, b"partial");
        write(&live_temp, b"active");

        let report = reconcile_repository(&repo);
        assert_eq!(report.valid_checkpoints, 1);
        assert_eq!(report.quarantined_metadata, 1);
        assert_eq!(report.orphan_refs_removed, 1);
        assert_eq!(report.stale_temp_files_removed, 1);
        assert!(!dead_temp.exists());
        assert!(live_temp.exists(), "a live writer's temp file is preserved");
        assert_eq!(
            git_text(
                &repo,
                &["rev-parse", "--verify", &checkpoint.checkpoint_ref]
            )
            .expect("valid ref"),
            checkpoint.checkpoint_sha
        );
        assert!(
            !git_output(&repo, &["rev-parse", "--verify", &orphan_ref], None, None)
                .expect("git")
                .status
                .success()
        );
        assert!(directory.join("quarantine").exists());
        let _ = std::fs::remove_file(live_temp);
        clear(&repo, run_id).expect("clear");
        cleanup(&repo);
    }

    #[test]
    fn linked_worktrees_share_one_checkpoint_recovery_domain() {
        let repo = temp_git_repo("linked_domain");
        let linked = repo.with_extension("linked-worktree");
        git_ok(
            &repo,
            &[
                "worktree",
                "add",
                "--detach",
                linked.to_str().expect("utf8 path"),
                "HEAD",
            ],
        )
        .expect("linked worktree");
        write(&repo.join("src/allowed.rs"), b"allowed v2\n");
        let run_id = "forge_checkpoint_linked_domain_test";
        let checkpoint = publish(&repo, &repo, run_id, &["src".to_string()], None, 1)
            .expect("published")
            .expect("checkpoint");
        assert_eq!(
            checkpoint_directory(&repo).expect("primary state directory"),
            checkpoint_directory(&linked).expect("linked state directory")
        );
        let report = reconcile_repository(&linked);
        assert_eq!(report.valid_checkpoints, 1, "{:?}", report.errors);
        assert_eq!(report.orphan_refs_removed, 0);
        assert_eq!(
            recover(&linked, run_id, &["src".to_string()], None).expect("recover"),
            Some(checkpoint)
        );
        clear(&linked, run_id).expect("clear through linked worktree");
        git_ok(
            &repo,
            &[
                "worktree",
                "remove",
                "--force",
                linked.to_str().expect("utf8 path"),
            ],
        )
        .expect("remove linked worktree");
        cleanup(&repo);
    }

    #[test]
    fn checkpoint_footprint_is_bounded_by_path_count_and_current_bytes() {
        let repo = temp_git_repo("footprint");
        write(&repo.join("src/allowed.rs"), b"12345");
        write(&repo.join("src/new.rs"), b"67890");
        let paths = vec!["src/allowed.rs".to_string(), "src/new.rs".to_string()];
        assert!(validate_checkpoint_footprint(&repo, &paths, 2, 10).is_ok());
        assert!(
            validate_checkpoint_footprint(&repo, &paths, 1, 10)
                .expect_err("path cap")
                .contains("2 paths")
        );
        assert!(
            validate_checkpoint_footprint(&repo, &paths, 2, 9)
                .expect_err("byte cap")
                .contains("more than 9")
        );
        cleanup(&repo);
    }

    fn temp_git_repo(label: &str) -> PathBuf {
        let nonce = CHECKPOINT_NONCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "mdx_forge_checkpoint_{label}_{}_{}",
            std::process::id(),
            nonce
        ));
        std::fs::create_dir_all(path.join("src")).expect("temp repo");
        git_ok(&path, &["init", "-q"]).expect("git init");
        write(&path.join("src/allowed.rs"), b"allowed v1\n");
        write(&path.join("src/outside.rs"), b"outside v1\n");
        git_ok(&path, &["add", "-A"]).expect("git add");
        git_ok(
            &path,
            &[
                "-c",
                "user.name=Checkpoint Test",
                "-c",
                "user.email=checkpoint-test@mdx.local",
                "commit",
                "-q",
                "-m",
                "initial",
            ],
        )
        .expect("initial commit");
        path
    }

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent");
        }
        std::fs::write(path, bytes).expect("write fixture");
    }

    fn cleanup(repo: &Path) {
        let _ = std::fs::remove_dir_all(repo);
        let _ = std::fs::remove_dir_all(repo.with_extension("recovery-worktree"));
    }
}
