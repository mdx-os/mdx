//! Headless calibration entry: `mdx-server forge-bench`.
//!
//! Runs the REAL native Forge loop - the same run_forge_loop the server
//! and fleet execute, no benchmark-special solver - against a task in a
//! plain directory, then restores the loop's branch into the working tree
//! so an external verifier (Terminal-Bench's, or a human) measures the
//! actual work. This exists so the harness gets an honest external number
//! instead of grading itself on its own fixtures.
//!
//! It grants nothing: the kernel is ephemeral and local, ship stays a
//! human verb, and the only posture change a benchmark needs - opening
//! the run_command allowlist to arbitrary terminal work - is a separate
//! explicit env flag (MDX_FORGE_BENCH_OPEN_COMMANDS=1) receipted on the
//! run. Model configuration rides the same MDX_FLEET_BUILDER_* env the
//! server uses.

use mdx_core::MdxKernel;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse_args(args)?;
    let repo_root = options
        .repo_root
        .canonicalize()
        .map_err(|error| format!("repo root {}: {error}", options.repo_root.display()))?;
    ensure_git_baseline(&repo_root)?;

    let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
    let request = crate::forge_loop_runner::ForgeRunRequest {
        run_id: options.run_id.clone(),
        tenant_id: "local_tenant".to_string(),
        actor_id: "agent:forge_bench".to_string(),
        work_item_id: "native_harness_calibration".to_string(),
        intent: options.task.clone(),
        allowed_commands: options.allowed_commands.clone(),
        max_turns: options.max_turns,
        revise_branch: None,
        resume: false,
        write_scope: Vec::new(),
        check_target_dir: None,
        builder_slot: options.builder_slot.clone(),
        work_complexity_tier: "medium".to_string(),
        semantic_policy_required_operations: Vec::new(),
        semantic_policy_source: "forge_bench".to_string(),
        execution_geometry_requested_workers: 1,
        execution_geometry_effective_workers: 1,
        execution_geometry_lane: "forge_bench".to_string(),
        execution_geometry_route: "cli:forge-bench".to_string(),
        mission_id: String::new(),
        mission_milestone_id: String::new(),
        max_cost_cents: options.max_cost_cents,
        max_runtime_ms: options.max_runtime_ms,
        envelope_id: String::new(),
        plan_only: options.plan_only,
        reasoning_effort: options.reasoning_effort.clone(),
    };
    let outcome = crate::forge_loop_runner::run_forge_loop(&request, &repo_root, &kernel);

    // The loop lands its work on a branch in a throwaway worktree. A
    // benchmark verifier reads the task directory itself, so restore the
    // branch content into the working tree. Note: restore overlays branch
    // content; a file the run DELETED stays in the tree (rare in benchmark
    // tasks, honest to name).
    let mut restored = false;
    if let Some(branch) = outcome.branch.as_deref() {
        restored = Command::new("git")
            .arg("restore")
            .arg("--source")
            .arg(branch)
            .arg("--worktree")
            .arg("--")
            .arg(".")
            .current_dir(&repo_root)
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
    }

    // JSONL exec mode: emit every forge.run.event for this run as one JSON
    // line, for CI and observability (Codex's --json / exec mode). Written to a
    // path, or to stdout when "-". Machine-readable trace alongside the human
    // outcome line below.
    if let Some(target) = options.json_events.as_deref()
        && let Ok(kernel) = kernel.read()
    {
        {
            let mut lines = String::new();
            for receipt in kernel.ledger().query().by_kind("forge.run.event") {
                if receipt.payload.get("run_id").map(String::as_str)
                    == Some(request.run_id.as_str())
                {
                    let obj: std::collections::BTreeMap<&String, &String> =
                        receipt.payload.iter().collect();
                    if let Ok(line) = serde_json::to_string(&obj) {
                        lines.push_str(&line);
                        lines.push('\n');
                    }
                }
            }
            if target == "-" {
                print!("{lines}");
            } else if let Err(error) = std::fs::write(target, &lines) {
                eprintln!("forge-bench: could not write --json-events to {target}: {error}");
            }
        }
    }

    println!(
        "{}",
        serde_json::json!({
            "run_id": request.run_id,
            "status": outcome.status,
            "turns_used": outcome.turns_used,
            "files_changed": outcome.files_changed,
            "check_runs": outcome.check_runs,
            "last_check_passed": outcome.last_check_passed,
            "branch": outcome.branch,
            "branch_restored_to_worktree": restored,
            "finish_summary": outcome.finish_summary,
        })
    );
    Ok(())
}

struct BenchOptions {
    repo_root: PathBuf,
    task: String,
    run_id: String,
    builder_slot: String,
    allowed_commands: Vec<String>,
    max_turns: u32,
    max_cost_cents: u32,
    max_runtime_ms: u64,
    json_events: Option<String>,
    plan_only: bool,
    reasoning_effort: String,
}

fn parse_args(args: &[String]) -> Result<BenchOptions, String> {
    let mut repo_root: Option<PathBuf> = None;
    let mut task: Option<String> = None;
    let mut run_id = "forge_bench_run".to_string();
    let mut builder_slot = String::new();
    let mut allowed_commands: Vec<String> = Vec::new();
    let mut max_turns: u32 = 0;
    let mut max_cost_cents: u32 = 0;
    let mut max_runtime_ms: u64 = 0;
    let mut json_events: Option<String> = None;
    let mut plan_only = false;
    let mut reasoning_effort = String::new();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let mut value = |name: &str| -> Result<String, String> {
            index += 1;
            args.get(index)
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag {
            "--repo-root" => repo_root = Some(PathBuf::from(value("--repo-root")?)),
            "--task" => task = Some(value("--task")?),
            "--task-file" => {
                let path = value("--task-file")?;
                task = Some(
                    std::fs::read_to_string(&path)
                        .map_err(|error| format!("task file {path}: {error}"))?
                        .trim()
                        .to_string(),
                );
            }
            "--run-id" => run_id = value("--run-id")?,
            "--json-events" => json_events = Some(value("--json-events")?),
            "--plan-only" => plan_only = true,
            "--reasoning-effort" => reasoning_effort = value("--reasoning-effort")?,
            "--builder-slot" => builder_slot = value("--builder-slot")?,
            "--allow" => allowed_commands.push(value("--allow")?),
            "--max-turns" => {
                max_turns = value("--max-turns")?
                    .parse()
                    .map_err(|_| "--max-turns needs a number".to_string())?;
            }
            "--max-cost-cents" => {
                max_cost_cents = value("--max-cost-cents")?
                    .parse()
                    .map_err(|_| "--max-cost-cents needs a number".to_string())?;
            }
            "--max-runtime-ms" => {
                max_runtime_ms = value("--max-runtime-ms")?
                    .parse()
                    .map_err(|_| "--max-runtime-ms needs a number".to_string())?;
            }
            other => return Err(format!("unknown flag: {other}")),
        }
        index += 1;
    }
    Ok(BenchOptions {
        repo_root: repo_root.ok_or("--repo-root is required")?,
        task: task.ok_or("--task or --task-file is required")?,
        run_id,
        builder_slot,
        allowed_commands,
        max_turns,
        max_cost_cents,
        max_runtime_ms,
        json_events,
        plan_only,
        reasoning_effort,
    })
}

/// The loop's workspace model is a git worktree cut from HEAD, so the task
/// directory must be a git repo with at least one commit. A benchmark
/// container usually is not one - make the baseline here, identity pinned
/// locally so nothing depends on global git config.
fn ensure_git_baseline(repo_root: &Path) -> Result<(), String> {
    let inside = Command::new("git")
        .arg("rev-parse")
        .arg("--is-inside-work-tree")
        .current_dir(repo_root)
        .output();
    let is_repo = inside
        .as_ref()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !is_repo {
        run_git(repo_root, &["init"])?;
    }
    let has_head = Command::new("git")
        .arg("rev-parse")
        .arg("HEAD")
        .current_dir(repo_root)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !has_head {
        run_git(repo_root, &["add", "-A"])?;
        run_git(
            repo_root,
            &[
                "-c",
                "user.email=forge-bench@mdx.local",
                "-c",
                "user.name=forge-bench",
                "commit",
                "--allow-empty",
                "-m",
                "forge-bench baseline",
            ],
        )?;
    }
    Ok(())
}

fn run_git(repo_root: &Path, args: &[&str]) -> Result<(), String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .map_err(|error| format!("git {}: {error}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arg_vec(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn parses_the_full_flag_set() {
        let options = parse_args(&arg_vec(&[
            "--repo-root",
            "/tmp/task",
            "--task",
            "fix the bug",
            "--run-id",
            "bench_1",
            "--builder-slot",
            "BENCH",
            "--allow",
            "make test",
            "--allow",
            "make lint",
            "--max-turns",
            "40",
            "--max-cost-cents",
            "500",
            "--max-runtime-ms",
            "900000",
            "--json-events",
            "/tmp/events.jsonl",
        ]))
        .expect("parses");
        assert_eq!(options.repo_root, PathBuf::from("/tmp/task"));
        assert_eq!(options.task, "fix the bug");
        assert_eq!(options.run_id, "bench_1");
        assert_eq!(options.builder_slot, "BENCH");
        assert_eq!(options.allowed_commands, vec!["make test", "make lint"]);
        assert_eq!(options.max_turns, 40);
        assert_eq!(options.max_cost_cents, 500);
        assert_eq!(options.max_runtime_ms, 900_000);
        assert_eq!(options.json_events.as_deref(), Some("/tmp/events.jsonl"));
    }

    #[test]
    fn refuses_missing_required_flags_and_unknown_flags() {
        assert!(parse_args(&arg_vec(&["--task", "x"])).is_err());
        assert!(parse_args(&arg_vec(&["--repo-root", "/tmp"])).is_err());
        assert!(parse_args(&arg_vec(&["--frobnicate"])).is_err());
    }

    #[test]
    fn git_baseline_initializes_a_bare_directory() {
        let dir =
            std::env::temp_dir().join(format!("forge-bench-baseline-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("hello.txt"), "hi\n").expect("write");
        ensure_git_baseline(&dir).expect("baseline");
        let head = Command::new("git")
            .arg("rev-parse")
            .arg("HEAD")
            .current_dir(&dir)
            .output()
            .expect("git");
        assert!(head.status.success(), "baseline commit must exist");
        // Idempotent: a second call must not fail or re-commit.
        ensure_git_baseline(&dir).expect("idempotent");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
