// One isolated workspace per worker run: a detached git worktree that the
// patch applier writes into and the sandbox mounts, so tests run on the
// PATCHED code, not an empty container. The live repo is never touched. The
// workspace owns its own cleanup (Drop removes exactly its own worktree, never
// by a shared name), and its path is scoped to the run plus a process-unique
// counter so concurrent runs at scale never collide or remove each other's
// workspace.
#![allow(dead_code)]
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);
static WORKSPACE_GENERATION: OnceLock<String> = OnceLock::new();
static WORKTREE_GIT_LOCK: Mutex<()> = Mutex::new(());
const WORKTREE_GIT_ATTEMPTS: usize = 5;
#[cfg(unix)]
const DISABLED_GIT_HOOKS_PATH: &str = "/dev/null";
#[cfg(windows)]
const DISABLED_GIT_HOOKS_PATH: &str = "NUL";
#[cfg(not(any(unix, windows)))]
const DISABLED_GIT_HOOKS_PATH: &str = "";

// Tests across the worker drivers toggle process-global opt-in env vars
// (MDX_PLAN_PATCH_APPLY / MDX_PLAN_TEST_EXEC). They must serialize on this lock
// so one test's reset never races another mid-run.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) struct WorkerWorkspace {
    repo_root: PathBuf,
    path: PathBuf,
    id: String,
    live: bool,
    dirty_base_snapshot: Option<DirtyBaseSnapshot>,
    checkpoint_context: Option<CheckpointContext>,
    recovered_checkpoint: Option<crate::forge_workspace_checkpoint::WorkspaceCheckpoint>,
}

#[derive(Clone, Debug)]
struct CheckpointContext {
    run_id: String,
    write_scope: Vec<String>,
    origin_ref: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct DirtyBaseSnapshot {
    pub(crate) file_count: u32,
    pub(crate) untracked_file_count: u32,
    pub(crate) commit_sha: String,
}

impl WorkerWorkspace {
    // Create a detached worktree of the repo at HEAD, at a run-scoped, process-
    // unique path. run_scope should carry tenant/run/workflow identity.
    pub(crate) fn create(repo_root: impl Into<PathBuf>, run_scope: &str) -> Option<Self> {
        let repo_root = repo_root.into();
        let counter = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = format!(
            "mdx_ws_{}_{}_{}_{}",
            sanitize(run_scope),
            std::process::id(),
            workspace_generation(),
            counter
        );
        let path = std::env::temp_dir().join(&id);
        let added = run_worktree_git(
            &repo_root,
            &["worktree", "add", "--detach", path_str(&path), "HEAD"],
        )?;
        if !added.success {
            return None;
        }
        let dirty_base_snapshot = dirty_base_snapshot_for_forge_run(&repo_root, &path, run_scope);
        Some(Self {
            repo_root,
            path,
            id,
            live: true,
            dirty_base_snapshot,
            checkpoint_context: None,
            recovered_checkpoint: None,
        })
    }

    // Create a worktree at an existing ref instead of HEAD - used to revise
    // a run's branch: the agent builds on the prior work, not a clean tree.
    // The ref must resolve in the repo; a detached worktree at it lets the
    // loop add commits that commit_onto_branch then lands back on the ref.
    pub(crate) fn create_at(
        repo_root: impl Into<PathBuf>,
        run_scope: &str,
        git_ref: &str,
    ) -> Option<Self> {
        let repo_root = repo_root.into();
        if !run_git(&repo_root, &["rev-parse", "--verify", git_ref])?.success {
            return None;
        }
        let counter = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let id = format!(
            "mdx_ws_{}_{}_{}_{}",
            sanitize(run_scope),
            std::process::id(),
            workspace_generation(),
            counter
        );
        let path = std::env::temp_dir().join(&id);
        let added = run_worktree_git(
            &repo_root,
            &["worktree", "add", "--detach", path_str(&path), git_ref],
        )?;
        if !added.success {
            return None;
        }
        Some(Self {
            repo_root,
            path,
            id,
            live: true,
            dirty_base_snapshot: None,
            checkpoint_context: None,
            recovered_checkpoint: None,
        })
    }

    /// Open the durable Forge workspace. A fresh run refuses to trample a
    /// recoverable checkpoint with the same run id. A resume verifies and
    /// reapplies that checkpoint at its original parent, preserving a clean
    /// eventual review history.
    pub(crate) fn create_for_forge(
        repo_root: impl Into<PathBuf>,
        run_id: &str,
        revise_branch: Option<&str>,
        resume: bool,
        write_scope: &[String],
    ) -> Result<Self, String> {
        let repo_root = repo_root.into();
        if !resume && crate::forge_workspace_checkpoint::exists(&repo_root, run_id)? {
            return Err(
                "a durable workspace checkpoint already exists; resume or reconcile that run"
                    .to_string(),
            );
        }
        let recovered = if resume {
            crate::forge_workspace_checkpoint::recover(
                &repo_root,
                run_id,
                write_scope,
                revise_branch,
            )?
        } else {
            None
        };
        let mut workspace = if let Some(checkpoint) = recovered.as_ref() {
            let workspace = Self::create_at(&repo_root, run_id, &checkpoint.parent_sha)
                .ok_or_else(|| "could not open the checkpoint parent workspace".to_string())?;
            if let Err(error) =
                crate::forge_workspace_checkpoint::apply(workspace.path(), checkpoint)
            {
                drop(workspace);
                return Err(format!(
                    "could not rehydrate the workspace checkpoint: {error}"
                ));
            }
            workspace
        } else {
            match revise_branch {
                Some(branch) => Self::create_at(&repo_root, run_id, branch),
                None => Self::create(&repo_root, run_id),
            }
            .ok_or_else(|| "could not create the isolated workspace".to_string())?
        };
        workspace.checkpoint_context = Some(CheckpointContext {
            run_id: run_id.to_string(),
            write_scope: write_scope.to_vec(),
            origin_ref: revise_branch.map(ToString::to_string),
        });
        workspace.recovered_checkpoint = recovered;
        Ok(workspace)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    pub(crate) fn dirty_base_snapshot(&self) -> Option<&DirtyBaseSnapshot> {
        self.dirty_base_snapshot.as_ref()
    }

    pub(crate) fn recovered_checkpoint(
        &self,
    ) -> Option<&crate::forge_workspace_checkpoint::WorkspaceCheckpoint> {
        self.recovered_checkpoint.as_ref()
    }

    pub(crate) fn checkpoint(
        &self,
        sequence: u64,
    ) -> Result<Option<crate::forge_workspace_checkpoint::WorkspaceCheckpoint>, String> {
        let Some(context) = self.checkpoint_context.as_ref() else {
            return Ok(None);
        };
        crate::forge_workspace_checkpoint::publish(
            &self.repo_root,
            &self.path,
            &context.run_id,
            &context.write_scope,
            context.origin_ref.as_deref(),
            sequence,
        )
    }

    pub(crate) fn clear_checkpoint(&self) -> Result<(), String> {
        let Some(context) = self.checkpoint_context.as_ref() else {
            return Ok(());
        };
        crate::forge_workspace_checkpoint::clear(&self.repo_root, &context.run_id)
    }

    // The run's exit ramp: turn the workspace's accumulated edits into a
    // branch the live repo can review. The commit lands in the shared
    // object store, so the branch survives this worktree's removal - the
    // run ends, the work remains, and the PR is cut from the branch.
    // Returns the commit sha; None when there is nothing to commit (an
    // empty run produces no branch, honestly) or any git step refuses.
    pub(crate) fn commit_to_branch(&self, branch: &str, message: &str) -> Option<String> {
        prune_known_artifacts(&self.path);
        let base = head_sha(&self.path)?;
        let expected_tip = branch_tip(&self.repo_root, branch)?;
        if let Some(existing) = expected_tip.as_deref()
            && existing != base
        {
            return None;
        }
        let status = run_git(&self.path, &["status", "--porcelain"])?;
        if !status.success || status.stdout.trim().is_empty() {
            return None;
        }
        if !run_git(&self.path, &["add", "-A"])?.success {
            return None;
        }
        let committed = run_git(
            &self.path,
            &[
                "-c",
                "user.name=Forge",
                "-c",
                "user.email=forge-agent@mdx.local",
                "commit",
                "-m",
                message,
            ],
        )?;
        if !committed.success {
            return None;
        }
        let sha = run_git(&self.path, &["rev-parse", "HEAD"])?;
        if !sha.success {
            return None;
        }
        let sha = sha.stdout.trim().to_string();
        // The workspace is detached. Commit there first, then land the
        // review branch ref after the commit exists. This avoids leaving a
        // ghost branch at the original HEAD when commit creation fails.
        // The ref update is compare-and-swap safe: a ghost branch at the
        // base may advance, but a branch with newer or divergent work will
        // refuse instead of being overwritten.
        if !update_branch_ref(&self.repo_root, branch, &sha, expected_tip.as_deref())? {
            return None;
        }
        Some(sha)
    }

    // Distinct files in the committed tip - the count `git diff --stat` and the
    // Review page show. The per-edit `files_changed` tally counts edit
    // operations (a file edited twice, or edited then reverted, still counts),
    // so it overcounts; the run-list card must match the committed diff. Mirrors
    // run_diff_files' `git show <branch>` so revise/multi-commit runs stay
    // consistent.
    pub(crate) fn committed_file_count(&self, sha: &str) -> Option<u32> {
        let show = run_git(&self.path, &["show", "--numstat", "--format=", sha])?;
        if !show.success {
            return None;
        }
        Some(
            show.stdout
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count() as u32,
        )
    }

    pub(crate) fn committed_file_paths(&self, sha: &str) -> Option<Vec<String>> {
        let show = run_git(&self.path, &["show", "--name-only", "--format=", sha])?;
        if !show.success {
            return None;
        }
        Some(
            show.stdout
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToString::to_string)
                .collect(),
        )
    }

    // Add a revision commit ONTO an existing branch. The worktree was
    // opened at the branch tip (create_at), so a commit here sits on top of
    // the prior work; updating the branch ref lands it. Returns the new
    // sha; None when nothing changed or any step refuses.
    pub(crate) fn commit_onto_branch(&self, branch: &str, message: &str) -> Option<String> {
        prune_known_artifacts(&self.path);
        let base = head_sha(&self.path)?;
        let expected_tip = branch_tip(&self.repo_root, branch)?;
        if expected_tip.as_deref() != Some(base.as_str()) {
            return None;
        }
        let status = run_git(&self.path, &["status", "--porcelain"])?;
        if !status.success || status.stdout.trim().is_empty() {
            return None;
        }
        if !run_git(&self.path, &["add", "-A"])?.success {
            return None;
        }
        let committed = run_git(
            &self.path,
            &[
                "-c",
                "user.name=Forge",
                "-c",
                "user.email=forge-agent@mdx.local",
                "commit",
                "-m",
                message,
            ],
        )?;
        if !committed.success {
            return None;
        }
        let sha = run_git(&self.path, &["rev-parse", "HEAD"])?;
        if !sha.success {
            return None;
        }
        let sha = sha.stdout.trim().to_string();
        // Land the new commit on the branch ref (the worktree is detached).
        if !update_branch_ref(&self.repo_root, branch, &sha, expected_tip.as_deref())? {
            return None;
        }
        Some(sha)
    }
}

impl Drop for WorkerWorkspace {
    fn drop(&mut self) {
        if self.live {
            // Remove exactly this workspace's worktree - cleanup ownership is
            // the workspace's, scoped to its own path.
            let _ = run_worktree_git(
                &self.repo_root,
                &["worktree", "remove", "--force", path_str(&self.path)],
            );
        }
    }
}

pub(crate) struct GitRun {
    pub(crate) success: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorktreeReconciliation {
    pub(crate) stale_worktrees_removed: usize,
    pub(crate) live_worktrees_preserved: usize,
    pub(crate) errors: Vec<String>,
}

pub(crate) fn run_git(repo_root: &Path, args: &[&str]) -> Option<GitRun> {
    let output = Command::new("git")
        .args(["-c", &format!("core.hooksPath={DISABLED_GIT_HOOKS_PATH}")])
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .ok()?;
    Some(GitRun {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn worktree_git_guard() -> MutexGuard<'static, ()> {
    WORKTREE_GIT_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

fn run_worktree_git(repo_root: &Path, args: &[&str]) -> Option<GitRun> {
    let _guard = worktree_git_guard();
    let mut last = None;
    for attempt in 0..WORKTREE_GIT_ATTEMPTS {
        let result = run_git(repo_root, args)?;
        if result.success {
            return Some(result);
        }
        last = Some(result);
        if attempt + 1 < WORKTREE_GIT_ATTEMPTS {
            std::thread::sleep(Duration::from_millis(20 * (attempt as u64 + 1)));
        }
    }
    last
}

/// Remove only stale MDx-owned temp worktrees. The generation component is
/// required because container runtimes commonly reuse PID 1 after a restart:
/// a PID-only owner would make the new server preserve the dead server's
/// worktree and then collide with its reset workspace counter. Git's own prune
/// collects registrations whose directories already disappeared. A live owner
/// from another process, a current-generation owner, or an unrecognized path
/// is always preserved.
pub(crate) fn reconcile_stale_worktrees(repo_root: &Path) -> WorktreeReconciliation {
    let mut report = WorktreeReconciliation::default();
    let Some(listed) = run_worktree_git(repo_root, &["worktree", "list", "--porcelain", "-z"])
    else {
        report
            .errors
            .push("could not enumerate Git worktrees".to_string());
        return report;
    };
    if !listed.success {
        report.errors.push(listed.stderr.trim().to_string());
        return report;
    }
    let paths: Vec<PathBuf> = listed
        .stdout
        .split('\0')
        .filter_map(|field| field.strip_prefix("worktree "))
        .map(PathBuf::from)
        .collect();
    for path in paths {
        let Some(owner) = mdx_workspace_owner(&path) else {
            continue;
        };
        let owned_by_current_generation = owner.pid == std::process::id()
            && owner.generation.as_deref() == Some(workspace_generation());
        let owned_by_other_live_process = owner.pid != std::process::id()
            && crate::forge_workspace_checkpoint::process_is_alive(owner.pid);
        if owned_by_current_generation || owned_by_other_live_process {
            report.live_worktrees_preserved += 1;
            continue;
        }
        let Some(result) = run_worktree_git(
            repo_root,
            &["worktree", "remove", "--force", path_str(&path)],
        ) else {
            report.errors.push(format!(
                "could not remove stale worktree {}",
                path.display()
            ));
            continue;
        };
        if result.success {
            report.stale_worktrees_removed += 1;
        } else {
            report.errors.push(format!(
                "could not remove stale worktree {}: {}",
                path.display(),
                result.stderr.trim()
            ));
        }
    }
    if let Some(pruned) = run_worktree_git(repo_root, &["worktree", "prune", "--expire", "now"])
        && !pruned.success
    {
        report.errors.push(format!(
            "could not prune worktree metadata: {}",
            pruned.stderr.trim()
        ));
    }
    report
}

#[derive(Debug, PartialEq, Eq)]
struct WorkspaceOwner {
    pid: u32,
    generation: Option<String>,
}

fn workspace_generation() -> &'static str {
    WORKSPACE_GENERATION
        .get_or_init(|| {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            format!("g{nanos:x}")
        })
        .as_str()
}

fn mdx_workspace_owner(path: &Path) -> Option<WorkspaceOwner> {
    let expected_parent = std::env::temp_dir().canonicalize().ok()?;
    let parent = path.parent()?.canonicalize().ok()?;
    if parent != expected_parent {
        return None;
    }
    let name = path.file_name()?.to_str()?;
    if !name.starts_with("mdx_ws_") {
        return None;
    }
    let mut parts = name.rsplit('_');
    let _counter = parts.next()?.parse::<u64>().ok()?;
    let owner_or_generation = parts.next()?;
    if let Ok(pid) = owner_or_generation.parse::<u32>() {
        return Some(WorkspaceOwner {
            pid,
            generation: None,
        });
    }
    if !owner_or_generation.starts_with('g') {
        return None;
    }
    let pid = parts.next()?.parse::<u32>().ok()?;
    Some(WorkspaceOwner {
        pid,
        generation: Some(owner_or_generation.to_string()),
    })
}

fn run_git_stdin(repo_root: &Path, args: &[&str], stdin: &[u8]) -> Option<GitRun> {
    let mut child = Command::new("git")
        .args(["-c", &format!("core.hooksPath={DISABLED_GIT_HOOKS_PATH}")])
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    {
        use std::io::Write;
        let input = child.stdin.as_mut()?;
        input.write_all(stdin).ok()?;
    }
    let output = child.wait_with_output().ok()?;
    Some(GitRun {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn dirty_base_snapshot_for_forge_run(
    repo_root: &Path,
    workspace_path: &Path,
    run_scope: &str,
) -> Option<DirtyBaseSnapshot> {
    if !run_scope.starts_with("forge_run") || !dirty_base_snapshot_enabled() {
        return None;
    }
    let names = run_git(repo_root, &["diff", "--name-only", "HEAD", "--"])?;
    if !names.success {
        return None;
    }
    let file_count = names
        .stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .count() as u32;
    let tracked_dirty_paths: Vec<PathBuf> = names
        .stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect();
    let untracked = safe_untracked_snapshot_files(repo_root, &tracked_dirty_paths)?;
    if file_count == 0 && untracked.is_empty() {
        return None;
    }
    if file_count > 0 {
        let diff = run_git(repo_root, &["diff", "--binary", "HEAD", "--"])?;
        if !diff.success || diff.stdout.trim().is_empty() {
            return None;
        }
        if !run_git_stdin(
            workspace_path,
            &["apply", "--binary"],
            diff.stdout.as_bytes(),
        )?
        .success
        {
            return None;
        }
    }
    let mut untracked_file_count = 0;
    for relative in &untracked {
        let source = repo_root.join(relative);
        let target = workspace_path.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).ok()?;
        }
        std::fs::copy(&source, &target).ok()?;
        untracked_file_count += 1;
    }
    if !run_git(workspace_path, &["add", "-A"])?.success {
        return None;
    }
    let committed = run_git(
        workspace_path,
        &[
            "-c",
            "user.name=Forge",
            "-c",
            "user.email=forge-agent@mdx.local",
            "commit",
            "-m",
            "[forge] Local working tree snapshot",
        ],
    )?;
    if !committed.success {
        return None;
    }
    let sha = head_sha(workspace_path)?;
    Some(DirtyBaseSnapshot {
        file_count,
        untracked_file_count,
        commit_sha: sha,
    })
}

fn safe_untracked_snapshot_files(
    repo_root: &Path,
    tracked_dirty_paths: &[PathBuf],
) -> Option<Vec<PathBuf>> {
    let listed = run_git(
        repo_root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    if !listed.success {
        return None;
    }
    let mut files = Vec::new();
    for raw in listed.stdout.split('\0') {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        let relative = PathBuf::from(trimmed);
        if !is_safe_untracked_snapshot_path(&relative) {
            continue;
        }
        if !is_adjacent_to_tracked_dirty_path(&relative, tracked_dirty_paths) {
            continue;
        }
        let full_path = repo_root.join(&relative);
        let Ok(metadata) = std::fs::metadata(&full_path) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > 2_000_000 {
            continue;
        }
        files.push(relative);
    }
    Some(files)
}

fn is_adjacent_to_tracked_dirty_path(path: &Path, tracked_dirty_paths: &[PathBuf]) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    tracked_dirty_paths.iter().any(|tracked| {
        tracked
            .parent()
            .map(|tracked_parent| tracked_parent == parent)
            .unwrap_or(false)
    })
}

fn is_safe_untracked_snapshot_path(path: &Path) -> bool {
    const BLOCKED_COMPONENTS: &[&str] = &[
        ".git",
        ".hg",
        ".svn",
        ".venv",
        "node_modules",
        ".pytest_cache",
        ".mypy_cache",
        ".ruff_cache",
        ".gradle",
        ".next",
        ".build",
        "target",
        "dist",
        "build",
        "coverage",
        "DerivedData",
        "TestResults",
        "__pycache__",
    ];
    const SAFE_EXTENSIONS: &[&str] = &[
        "rs", "swift", "js", "jsx", "ts", "tsx", "svelte", "mjs", "cjs", "json", "jsonl", "toml",
        "yaml", "yml", "css", "scss", "html", "md", "sh", "sql", "py", "rb", "go", "java", "kt",
        "gradle", "c", "h", "cpp", "hpp",
    ];
    const SAFE_FILENAMES: &[&str] = &[
        "Makefile",
        "Dockerfile",
        "Package.swift",
        "Cargo.toml",
        "package.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "requirements.txt",
    ];
    const BLOCKED_FILENAMES: &[&str] = &[
        ".env",
        ".env.local",
        ".env.development",
        ".env.production",
        ".npmrc",
        ".pypirc",
        ".netrc",
    ];

    if path.is_absolute() {
        return false;
    }
    for component in path.components() {
        let name = component.as_os_str().to_string_lossy();
        if name == ".." || BLOCKED_COMPONENTS.contains(&name.as_ref()) {
            return false;
        }
        if name.starts_with(".env") {
            return false;
        }
    }
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if BLOCKED_FILENAMES.contains(&file_name)
        || file_name.ends_with(".pem")
        || file_name.ends_with(".key")
    {
        return false;
    }
    if SAFE_FILENAMES.contains(&file_name) {
        return true;
    }
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| SAFE_EXTENSIONS.contains(&extension))
        .unwrap_or(false)
}

fn dirty_base_snapshot_enabled() -> bool {
    std::env::var("MDX_FORGE_DIRTY_BASE_SNAPSHOT")
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            !matches!(value.as_str(), "0" | "false" | "off" | "no")
        })
        .unwrap_or(true)
}

fn head_sha(repo_root: &Path) -> Option<String> {
    let head = run_git(repo_root, &["rev-parse", "HEAD"])?;
    if !head.success {
        return None;
    }
    Some(head.stdout.trim().to_string())
}

fn branch_tip(repo_root: &Path, branch: &str) -> Option<Option<String>> {
    let refname = format!("refs/heads/{branch}");
    let tip = run_git(repo_root, &["rev-parse", "--verify", &refname])?;
    if tip.success {
        Some(Some(tip.stdout.trim().to_string()))
    } else {
        Some(None)
    }
}

fn update_branch_ref(
    repo_root: &Path,
    branch: &str,
    sha: &str,
    expected_tip: Option<&str>,
) -> Option<bool> {
    const MISSING_REF: &str = "0000000000000000000000000000000000000000";
    let refname = format!("refs/heads/{branch}");
    let old = expected_tip.unwrap_or(MISSING_REF);
    Some(run_git(repo_root, &["update-ref", &refname, sha, old])?.success)
}

pub(crate) fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn prune_known_artifacts(root: &Path) {
    const DIRS: &[&str] = &[
        ".venv",
        "node_modules",
        ".pytest_cache",
        ".mypy_cache",
        ".ruff_cache",
        ".gradle",
        ".next",
        ".build",
        "target",
        "dist",
        "build",
        "coverage",
        "DerivedData",
        "TestResults",
    ];
    for dir in DIRS {
        let path = root.join(dir);
        if path.exists() {
            let _ = std::fs::remove_dir_all(path);
        }
    }
    prune_matching_suffix(root, ".egg-info");
    prune_matching_suffix(root, ".dist-info");
    prune_pycache_dirs(root);
}

fn prune_matching_suffix(root: &Path, suffix: &str) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(suffix) {
            let _ = std::fs::remove_dir_all(path);
        } else {
            prune_matching_suffix(&path, suffix);
        }
    }
}

fn prune_pycache_dirs(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if entry.file_name().to_string_lossy() == "__pycache__" {
            let _ = std::fs::remove_dir_all(path);
        } else {
            prune_pycache_dirs(&path);
        }
    }
}

pub(crate) fn path_str(path: &Path) -> &str {
    path.to_str().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_run_produces_no_branch_and_a_real_one_survives_worktree_removal() {
        let repo_root = std::env::current_dir()
            .expect("cwd")
            .ancestors()
            .find(|path| path.join(".git").exists())
            .expect("repo root")
            .to_path_buf();
        let workspace = WorkerWorkspace::create(&repo_root, "commit_exit_test").expect("workspace");
        // Nothing changed: no branch, honestly.
        assert!(
            workspace
                .commit_to_branch("forge/test-empty-run", "nothing happened")
                .is_none()
        );
        // A real edit becomes a branch whose commit outlives the worktree.
        let branch = format!("forge/test-exit-{}", std::process::id());
        std::fs::write(
            workspace.path().join("FORGE_EXIT_TEST.txt"),
            "the run ends, the work remains\n",
        )
        .expect("write into workspace");
        let sha = workspace
            .commit_to_branch(&branch, "forge exit test commit")
            .expect("commit lands");
        assert_eq!(sha.len(), 40);
        drop(workspace);
        let listed = run_git(&repo_root, &["branch", "--list", &branch]).expect("git");
        assert!(listed.stdout.contains(&branch), "branch survives removal");
        let cleaned = run_git(&repo_root, &["branch", "-D", &branch]).expect("git");
        assert!(cleaned.success);
    }

    #[test]
    fn commit_prunes_local_tool_artifacts_before_staging() {
        let repo_root = std::env::current_dir()
            .expect("cwd")
            .ancestors()
            .find(|path| path.join(".git").exists())
            .expect("repo root")
            .to_path_buf();
        let workspace =
            WorkerWorkspace::create(&repo_root, "commit_artifact_test").expect("workspace");
        let branch = format!("forge/test-artifacts-{}", std::process::id());

        std::fs::create_dir_all(workspace.path().join(".venv/bin")).expect("venv");
        std::fs::write(workspace.path().join(".venv/bin/pytest"), "artifact").expect("venv file");
        std::fs::create_dir_all(workspace.path().join("canary.egg-info")).expect("egg info");
        std::fs::write(
            workspace.path().join("canary.egg-info/PKG-INFO"),
            "artifact",
        )
        .expect("egg file");
        std::fs::create_dir_all(workspace.path().join("src/nested_canary.egg-info"))
            .expect("nested egg info");
        std::fs::write(
            workspace.path().join("src/nested_canary.egg-info/PKG-INFO"),
            "artifact",
        )
        .expect("nested egg file");
        std::fs::create_dir_all(workspace.path().join("src/__pycache__")).expect("pycache");
        std::fs::write(workspace.path().join("src/__pycache__/mod.pyc"), "artifact")
            .expect("pycache file");
        std::fs::write(
            workspace.path().join("FORGE_ARTIFACT_TEST.txt"),
            "real reviewable change\n",
        )
        .expect("source change");

        let sha = workspace
            .commit_to_branch(&branch, "forge artifact pruning test")
            .expect("commit lands");
        assert_eq!(sha.len(), 40);
        let show =
            run_git(&repo_root, &["show", "--name-only", "--format=", &branch]).expect("show");
        assert!(show.stdout.contains("FORGE_ARTIFACT_TEST.txt"));
        assert!(!show.stdout.contains(".venv"));
        assert!(!show.stdout.contains(".egg-info"));
        assert!(!show.stdout.contains("__pycache__"));

        drop(workspace);
        let cleaned = run_git(&repo_root, &["branch", "-D", &branch]).expect("git");
        assert!(cleaned.success);
    }

    #[test]
    fn commit_to_branch_advances_a_stale_branch_ref() {
        let repo_root = std::env::current_dir()
            .expect("cwd")
            .ancestors()
            .find(|path| path.join(".git").exists())
            .expect("repo root")
            .to_path_buf();
        let workspace =
            WorkerWorkspace::create(&repo_root, "commit_stale_branch_test").expect("workspace");
        let branch = format!("forge/test-stale-{}", std::process::id());
        let stale = run_git(&repo_root, &["branch", "-f", &branch, "HEAD"]).expect("git");
        assert!(stale.success);

        std::fs::write(
            workspace.path().join("FORGE_STALE_BRANCH_TEST.txt"),
            "the branch ref should advance to the new commit\n",
        )
        .expect("source change");

        let sha = workspace
            .commit_to_branch(&branch, "forge stale branch test")
            .expect("commit lands");
        let resolved = run_git(&repo_root, &["rev-parse", &branch]).expect("git");
        assert_eq!(resolved.stdout.trim(), sha);

        drop(workspace);
        let cleaned = run_git(&repo_root, &["branch", "-D", &branch]).expect("git");
        assert!(cleaned.success);
    }

    #[test]
    fn commit_to_branch_refuses_to_overwrite_diverged_branch_ref() {
        let repo_root = std::env::current_dir()
            .expect("cwd")
            .ancestors()
            .find(|path| path.join(".git").exists())
            .expect("repo root")
            .to_path_buf();
        let branch = format!("forge/test-diverged-{}", std::process::id());
        let first =
            WorkerWorkspace::create(&repo_root, "commit_diverged_branch_first").expect("workspace");
        std::fs::write(
            first.path().join("FORGE_DIVERGED_BRANCH_FIRST.txt"),
            "first reviewable change\n",
        )
        .expect("source change");
        let original_sha = first
            .commit_to_branch(&branch, "forge diverged branch first")
            .expect("first commit lands");
        drop(first);

        let second = WorkerWorkspace::create(&repo_root, "commit_diverged_branch_second")
            .expect("workspace");
        std::fs::write(
            second.path().join("FORGE_DIVERGED_BRANCH_SECOND.txt"),
            "second reviewable change\n",
        )
        .expect("source change");
        assert!(
            second
                .commit_to_branch(&branch, "forge diverged branch second")
                .is_none(),
            "diverged branch should not be overwritten"
        );
        let resolved = run_git(&repo_root, &["rev-parse", &branch]).expect("git");
        assert_eq!(resolved.stdout.trim(), original_sha);

        drop(second);
        let cleaned = run_git(&repo_root, &["branch", "-D", &branch]).expect("git");
        assert!(cleaned.success);
    }

    #[test]
    fn forge_run_workspace_starts_from_tracked_dirty_snapshot() {
        let repo_root = temp_git_repo("dirty_snapshot");
        std::fs::write(repo_root.join("tracked.txt"), "clean\n").expect("tracked");
        assert!(
            run_git(&repo_root, &["add", "tracked.txt"])
                .expect("add")
                .success
        );
        assert!(
            run_git(
                &repo_root,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "commit",
                    "-m",
                    "initial",
                ],
            )
            .expect("commit")
            .success
        );

        std::fs::write(repo_root.join("tracked.txt"), "dirty\n").expect("dirty tracked");
        std::fs::write(repo_root.join("untracked.txt"), "do not include\n").expect("untracked");

        let workspace =
            WorkerWorkspace::create(&repo_root, "forge_run_dirty_snapshot").expect("workspace");
        let snapshot = workspace
            .dirty_base_snapshot()
            .expect("dirty base snapshot");

        assert_eq!(snapshot.file_count, 1);
        assert_eq!(snapshot.untracked_file_count, 0);
        assert_eq!(
            std::fs::read_to_string(workspace.path().join("tracked.txt")).expect("read tracked"),
            "dirty\n"
        );
        assert!(
            !workspace.path().join("untracked.txt").exists(),
            "untracked files must not enter the worker snapshot"
        );
    }

    #[test]
    fn forge_run_workspace_includes_safe_untracked_source_files() {
        let repo_root = temp_git_repo("dirty_snapshot_untracked");
        std::fs::write(repo_root.join("tracked.txt"), "clean\n").expect("tracked");
        assert!(
            run_git(&repo_root, &["add", "tracked.txt"])
                .expect("add")
                .success
        );
        assert!(
            run_git(
                &repo_root,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "commit",
                    "-m",
                    "initial",
                ],
            )
            .expect("commit")
            .success
        );

        std::fs::create_dir_all(repo_root.join("src")).expect("src");
        std::fs::write(repo_root.join("src/existing.rs"), "pub fn existing() {}\n")
            .expect("tracked source");
        assert!(
            run_git(&repo_root, &["add", "src/existing.rs"])
                .expect("add source")
                .success
        );
        assert!(
            run_git(
                &repo_root,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "commit",
                    "-m",
                    "add source",
                ],
            )
            .expect("commit source")
            .success
        );

        std::fs::write(
            repo_root.join("src/existing.rs"),
            "pub fn existing() { new_module(); }\n",
        )
        .expect("dirty adjacent source");
        std::fs::write(
            repo_root.join("src/new_module.rs"),
            "pub fn new_module() {}\n",
        )
        .expect("safe source");
        std::fs::create_dir_all(repo_root.join("docs")).expect("docs");
        std::fs::write(repo_root.join("docs/runbook.md"), "# Runbook\n").expect("safe doc");
        std::fs::write(repo_root.join(".env"), "TOKEN=secret\n").expect("secret");
        std::fs::write(repo_root.join("notes.txt"), "generic note\n").expect("txt");
        std::fs::create_dir_all(repo_root.join("target")).expect("target");
        std::fs::write(
            repo_root.join("target/generated.rs"),
            "pub fn generated() {}\n",
        )
        .expect("artifact");

        let workspace =
            WorkerWorkspace::create(&repo_root, "forge_run_untracked_snapshot").expect("workspace");
        let snapshot = workspace
            .dirty_base_snapshot()
            .expect("dirty base snapshot");

        assert_eq!(snapshot.file_count, 1);
        assert_eq!(snapshot.untracked_file_count, 1);
        assert!(
            workspace.path().join("src/new_module.rs").exists(),
            "safe adjacent untracked Rust source should enter the worker snapshot"
        );
        assert!(
            !workspace.path().join("docs/runbook.md").exists(),
            "unrelated safe-looking docs should stay out of the worker snapshot"
        );
        assert!(
            !workspace.path().join(".env").exists(),
            "secret-like untracked files must stay out of the worker snapshot"
        );
        assert!(
            !workspace.path().join("notes.txt").exists(),
            "generic untracked text files stay out unless they are source-like"
        );
        assert!(
            !workspace.path().join("target/generated.rs").exists(),
            "artifact directories must stay out of the worker snapshot"
        );
    }

    #[test]
    fn forge_dirty_base_snapshot_keeps_nonadjacent_safe_sources_out() {
        let repo_root = temp_git_repo("dirty_snapshot_adjacency");
        std::fs::create_dir_all(repo_root.join("apps/mdx-operator-macos/Sources")).expect("macos");
        std::fs::create_dir_all(repo_root.join("crates/mdx-server/src")).expect("server");
        std::fs::write(
            repo_root.join("apps/mdx-operator-macos/Sources/HomeView.swift"),
            "struct HomeView {}\n",
        )
        .expect("tracked swift");
        std::fs::write(
            repo_root.join("crates/mdx-server/src/lib.rs"),
            "pub fn lib() {}\n",
        )
        .expect("tracked rust");
        assert!(
            run_git(
                &repo_root,
                &[
                    "add",
                    "apps/mdx-operator-macos/Sources/HomeView.swift",
                    "crates/mdx-server/src/lib.rs",
                ],
            )
            .expect("add")
            .success
        );
        assert!(
            run_git(
                &repo_root,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "commit",
                    "-m",
                    "seed packages",
                ],
            )
            .expect("commit")
            .success
        );

        std::fs::write(
            repo_root.join("apps/mdx-operator-macos/Sources/HomeView.swift"),
            "struct HomeView { let banner = true }\n",
        )
        .expect("dirty swift");
        std::fs::write(
            repo_root.join("apps/mdx-operator-macos/Sources/BannerView.swift"),
            "struct BannerView {}\n",
        )
        .expect("adjacent safe swift");
        std::fs::write(
            repo_root.join("crates/mdx-server/src/orphan.rs"),
            "pub fn orphan() {}\n",
        )
        .expect("nonadjacent safe rust");
        std::fs::write(
            repo_root.join("apps/mdx-operator-macos/Sources/secrets.env"),
            "TOKEN=secret\n",
        )
        .expect("adjacent unsafe");

        let workspace =
            WorkerWorkspace::create(&repo_root, "forge_run_adjacency_snapshot").expect("workspace");
        let snapshot = workspace
            .dirty_base_snapshot()
            .expect("dirty base snapshot");

        assert_eq!(snapshot.file_count, 1);
        assert_eq!(snapshot.untracked_file_count, 1);
        assert!(
            workspace
                .path()
                .join("apps/mdx-operator-macos/Sources/BannerView.swift")
                .exists(),
            "adjacent safe Swift source should enter the dirty-base snapshot"
        );
        assert!(
            !workspace
                .path()
                .join("crates/mdx-server/src/orphan.rs")
                .exists(),
            "safe untracked source in a non-adjacent package must stay out"
        );
        assert!(
            !workspace
                .path()
                .join("apps/mdx-operator-macos/Sources/secrets.env")
                .exists(),
            "adjacent but unsafe untracked files must stay out"
        );
    }

    #[test]
    fn non_forge_workspace_does_not_snapshot_dirty_worktree() {
        let repo_root = temp_git_repo("no_dirty_snapshot");
        std::fs::write(repo_root.join("tracked.txt"), "clean\n").expect("tracked");
        assert!(
            run_git(&repo_root, &["add", "tracked.txt"])
                .expect("add")
                .success
        );
        assert!(
            run_git(
                &repo_root,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "commit",
                    "-m",
                    "initial",
                ],
            )
            .expect("commit")
            .success
        );

        std::fs::write(repo_root.join("tracked.txt"), "dirty\n").expect("dirty tracked");
        let workspace =
            WorkerWorkspace::create(&repo_root, "commit_test_scope").expect("workspace");

        assert!(workspace.dirty_base_snapshot().is_none());
        assert_eq!(
            std::fs::read_to_string(workspace.path().join("tracked.txt")).expect("read tracked"),
            "clean\n"
        );
    }

    #[test]
    fn forge_workspace_rehydrates_after_its_original_worktree_is_removed() {
        let repo_root = temp_git_repo("checkpoint_rehydrate");
        std::fs::create_dir_all(repo_root.join("src")).expect("source directory");
        std::fs::write(repo_root.join("src/lib.rs"), "pub fn value() -> u8 { 1 }\n")
            .expect("source");
        assert!(run_git(&repo_root, &["add", "-A"]).expect("add").success);
        assert!(
            run_git(
                &repo_root,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "commit",
                    "-m",
                    "initial",
                ],
            )
            .expect("commit")
            .success
        );
        let base = head_sha(&repo_root).expect("base sha");
        let run_id = "forge_run_workspace_rehydrate_test";
        let scope = vec!["src/lib.rs".to_string()];
        let workspace = WorkerWorkspace::create_for_forge(&repo_root, run_id, None, false, &scope)
            .expect("fresh forge workspace");
        let original_path = workspace.path().to_path_buf();
        std::fs::write(
            workspace.path().join("src/lib.rs"),
            "pub fn value() -> u8 { 2 }\n",
        )
        .expect("edit workspace");
        let checkpoint = workspace
            .checkpoint(1)
            .expect("checkpoint publication")
            .expect("checkpoint");
        assert_eq!(checkpoint.parent_sha, base);
        drop(workspace);
        assert!(!original_path.exists(), "the original worktree was removed");

        let resumed = WorkerWorkspace::create_for_forge(&repo_root, run_id, None, true, &scope)
            .expect("resumed forge workspace");
        assert_eq!(resumed.recovered_checkpoint(), Some(&checkpoint));
        assert_eq!(
            std::fs::read_to_string(resumed.path().join("src/lib.rs")).expect("rehydrated source"),
            "pub fn value() -> u8 { 2 }\n"
        );
        let branch = "forge/checkpoint-rehydrate-test";
        let committed = resumed
            .commit_to_branch(branch, "[forge] Rehydrate checkpoint test")
            .expect("review commit");
        resumed.clear_checkpoint().expect("checkpoint cleared");
        drop(resumed);
        assert!(
            !crate::forge_workspace_checkpoint::exists(&repo_root, run_id).expect("exists"),
            "a durable review branch supersedes the recovery checkpoint"
        );
        assert_eq!(
            run_git(
                &repo_root,
                &["rev-list", "--count", &format!("{base}..{committed}")],
            )
            .expect("commit count")
            .stdout
            .trim(),
            "1",
            "the internal checkpoint commit must not enter review history"
        );
        let _ = std::fs::remove_dir_all(repo_root);
    }

    #[test]
    fn startup_reconciliation_removes_dead_and_prior_generation_worktrees() {
        let repo_root = temp_git_repo("worktree_reconcile");
        std::fs::write(repo_root.join("tracked.txt"), "initial\n").expect("tracked");
        assert!(run_git(&repo_root, &["add", "-A"]).expect("add").success);
        assert!(
            run_git(
                &repo_root,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "commit",
                    "-m",
                    "initial",
                ],
            )
            .expect("commit")
            .success
        );
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let stale_dead =
            std::env::temp_dir().join(format!("mdx_ws_reconcile_{nonce}_99999999_gdead_0"));
        let stale_generation = std::env::temp_dir().join(format!(
            "mdx_ws_reconcile_{nonce}_{}_gprior_1",
            std::process::id()
        ));
        let stale_legacy =
            std::env::temp_dir().join(format!("mdx_ws_reconcile_{nonce}_{}_2", std::process::id()));
        let live = std::env::temp_dir().join(format!(
            "mdx_ws_reconcile_{nonce}_{}_{}_3",
            std::process::id(),
            workspace_generation()
        ));
        for (path, label) in [
            (&stale_dead, "dead worktree"),
            (&stale_generation, "prior-generation worktree"),
            (&stale_legacy, "legacy same-pid worktree"),
            (&live, "live worktree"),
        ] {
            assert!(
                run_worktree_git(
                    &repo_root,
                    &["worktree", "add", "--detach", path_str(path), "HEAD"],
                )
                .unwrap_or_else(|| panic!("{label}"))
                .success,
                "{label} should be created"
            );
        }

        let report = reconcile_stale_worktrees(&repo_root);
        assert_eq!(report.stale_worktrees_removed, 3);
        assert_eq!(report.live_worktrees_preserved, 1);
        assert!(!stale_dead.exists());
        assert!(!stale_generation.exists());
        assert!(!stale_legacy.exists());
        assert!(live.exists());
        assert!(report.errors.is_empty(), "{:?}", report.errors);

        let _ = run_worktree_git(
            &repo_root,
            &["worktree", "remove", "--force", path_str(&live)],
        );
        let _ = std::fs::remove_dir_all(repo_root);
    }

    #[cfg(unix)]
    #[test]
    fn model_influenced_repositories_cannot_run_git_hooks_on_the_host() {
        use std::os::unix::fs::PermissionsExt;

        let repo_root = temp_git_repo("hooks_disabled");
        std::fs::write(repo_root.join("tracked.txt"), "initial\n").expect("tracked");
        assert!(run_git(&repo_root, &["add", "-A"]).expect("add").success);
        assert!(
            run_git(
                &repo_root,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.com",
                    "commit",
                    "-m",
                    "initial",
                ],
            )
            .expect("commit")
            .success
        );
        let marker = repo_root.join("hook-ran");
        let hooks = repo_root.join(".git/hooks");
        for hook_name in ["post-checkout", "pre-commit"] {
            let hook = hooks.join(hook_name);
            std::fs::write(
                &hook,
                format!("#!/bin/sh\ntouch '{}'\nexit 97\n", marker.display()),
            )
            .expect("hook");
            let mut permissions = std::fs::metadata(&hook).expect("metadata").permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&hook, permissions).expect("executable hook");
        }

        let workspace =
            WorkerWorkspace::create(&repo_root, "hooks_disabled_test").expect("workspace");
        assert!(
            !marker.exists(),
            "worktree creation must not run post-checkout"
        );
        std::fs::write(workspace.path().join("tracked.txt"), "changed\n").expect("edit");
        assert!(
            workspace
                .commit_to_branch("forge/hooks-disabled-test", "[forge] Hook suppression test")
                .is_some(),
            "pre-commit must not run or block Forge persistence"
        );
        assert!(
            !marker.exists(),
            "Forge persistence must not run pre-commit"
        );
        drop(workspace);
        let _ = std::fs::remove_dir_all(repo_root);
    }

    fn temp_git_repo(label: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "mdx_worker_workspace_{label}_{}_{}",
            std::process::id(),
            nonce
        ));
        std::fs::create_dir_all(&path).expect("temp repo");
        assert!(run_git(&path, &["init"]).expect("git init").success);
        path
    }
}
