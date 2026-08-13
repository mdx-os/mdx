// Verified by its own isolation test; the driving surface (a worker running a
// multi-step build) arrives with Unlock 4, so the constructor is allowed to be
// unused by non-test builds until then.
#![allow(dead_code)]
// The server-layer patch applier for the plan-execute loop. The kernel never
// touches the filesystem; it delegates an approved, in-scope PatchApplication
// action here. This applier writes the diff into the worker's shared
// WorkerWorkspace - an isolated git worktree the live repo never sees, and the
// SAME workspace the sandbox runner mounts, so tests run on the patched code.
// The live repo is never mutated. The workspace owns its lifecycle (create and
// cleanup); this applier only writes into it. It opens only when
// MDX_PLAN_PATCH_APPLY=1 (an explicit opt-in), and returns presence-only
// evidence: workspace id, how many files and lines changed. Never the
// diff body, never file content.
use mdx_core::{HarnessPatchApplier, HarnessPatchApplyContext, HarnessPatchApplyResult};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) struct ServerPatchApplier {
    // The shared worker workspace path (a git worktree) the diff is applied
    // into - the same one the sandbox mounts.
    workspace_path: PathBuf,
    workspace_id: String,
    // The plan's diffs, keyed by action id. The server builds the applier with
    // these so the kernel holds no patch content.
    diffs: Vec<(String, String)>,
}

impl ServerPatchApplier {
    pub(crate) fn new(
        workspace_path: impl Into<PathBuf>,
        workspace_id: impl Into<String>,
        diffs: Vec<(String, String)>,
    ) -> Self {
        Self {
            workspace_path: workspace_path.into(),
            workspace_id: workspace_id.into(),
            diffs,
        }
    }

    fn diff_for(&self, action_id: &str) -> Option<&str> {
        self.diffs
            .iter()
            .find(|(id, _)| id == action_id)
            .map(|(_, diff)| diff.as_str())
    }
}

impl HarnessPatchApplier for ServerPatchApplier {
    fn apply(&self, context: &HarnessPatchApplyContext<'_>) -> Option<HarnessPatchApplyResult> {
        if std::env::var("MDX_PLAN_PATCH_APPLY").ok().as_deref() != Some("1") {
            return None;
        }
        let diff = self.diff_for(context.action_id)?;
        apply_into_workspace(&self.workspace_path, &self.workspace_id, diff)
    }
}

// Apply the diff inside the shared workspace worktree. The workspace persists
// (the sandbox will mount it next); cleanup belongs to the WorkerWorkspace.
// Any failure returns None and the loop keeps the action denied.
fn apply_into_workspace(
    workspace_path: &Path,
    workspace_id: &str,
    diff: &str,
) -> Option<HarnessPatchApplyResult> {
    let check = run_git_stdin(workspace_path, &["apply", "--check"], diff)?;
    if !check.success {
        return None;
    }
    let numstat = run_git_stdin(workspace_path, &["apply", "--numstat"], diff)?;
    if !numstat.success {
        return None;
    }
    let (files_changed, insertions, deletions) = parse_numstat(&numstat.stdout);
    let applied = run_git_stdin(workspace_path, &["apply"], diff)?;
    if !applied.success {
        return None;
    }
    Some(HarnessPatchApplyResult {
        worktree_id: workspace_id.to_string(),
        files_changed,
        insertions,
        deletions,
        applied: true,
        denied_reason: None,
    })
}

struct GitOutput {
    success: bool,
    stdout: String,
}

fn run_git_stdin(repo_root: &Path, args: &[&str], stdin_text: &str) -> Option<GitOutput> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(stdin_text.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;
    Some(GitOutput {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
    })
}

fn parse_numstat(numstat: &str) -> (u32, u32, u32) {
    let mut files = 0u32;
    let mut insertions = 0u32;
    let mut deletions = 0u32;
    for line in numstat.lines().filter(|line| !line.trim().is_empty()) {
        let mut parts = line.split('\t');
        let added = parts
            .next()
            .unwrap_or("0")
            .trim()
            .parse::<u32>()
            .unwrap_or(0);
        let removed = parts
            .next()
            .unwrap_or("0")
            .trim()
            .parse::<u32>()
            .unwrap_or(0);
        files += 1;
        insertions += added;
        deletions += removed;
    }
    (files, insertions, deletions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness_worker_workspace::WorkerWorkspace;

    fn run(dir: &Path, program: &str, args: &[&str]) {
        let status = Command::new(program)
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("command");
        assert!(status.status.success(), "{program} {args:?} failed");
    }

    #[test]
    fn applies_into_the_shared_workspace_never_the_live_repo() {
        let _env = crate::harness_worker_workspace::ENV_LOCK.lock().unwrap();
        let repo =
            std::env::temp_dir().join(format!("mdx_patch_applier_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).expect("repo dir");
        run(&repo, "git", &["init", "-q"]);
        run(&repo, "git", &["config", "user.email", "t@t"]);
        run(&repo, "git", &["config", "user.name", "t"]);
        std::fs::write(repo.join("file.txt"), "one\ntwo\nthree\n").expect("write file");
        run(&repo, "git", &["add", "."]);
        run(&repo, "git", &["commit", "-q", "-m", "init"]);

        let workspace = WorkerWorkspace::create(&repo, "tenant_run_1").expect("workspace");
        let diff = "diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n one\n-two\n+TWO\n three\n";
        let applier = ServerPatchApplier::new(
            workspace.path(),
            workspace.id(),
            vec![("act_1".to_string(), diff.to_string())],
        );

        // SAFETY: single-threaded test; set and clear the opt-in env var.
        unsafe { std::env::remove_var("MDX_PLAN_PATCH_APPLY") };
        assert!(
            applier
                .apply(&HarnessPatchApplyContext {
                    action_id: "act_1",
                    target: "file.txt",
                    allowed_write_scope: &["file.txt"],
                    approved_plan_hash: "sha256:test",
                })
                .is_none()
        );

        unsafe { std::env::set_var("MDX_PLAN_PATCH_APPLY", "1") };
        let result = applier
            .apply(&HarnessPatchApplyContext {
                action_id: "act_1",
                target: "file.txt",
                allowed_write_scope: &["file.txt"],
                approved_plan_hash: "sha256:test",
            })
            .expect("applied");
        unsafe { std::env::remove_var("MDX_PLAN_PATCH_APPLY") };

        assert!(result.applied);
        assert_eq!(result.files_changed, 1);
        // The patch landed IN the workspace...
        let in_workspace =
            std::fs::read_to_string(workspace.path().join("file.txt")).expect("read workspace");
        assert!(in_workspace.contains("TWO"));
        // ...and the live repo working tree is UNCHANGED.
        let live = std::fs::read_to_string(repo.join("file.txt")).expect("read live");
        assert_eq!(live, "one\ntwo\nthree\n");

        drop(workspace);
        let _ = std::fs::remove_dir_all(&repo);
    }
}
