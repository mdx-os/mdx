// Verified by its own Docker isolation + patched-code test; the driving
// surface (a worker running a multi-step build) arrives with Unlock 4, so the
// constructor is allowed to be unused by non-test builds until then.
#![allow(dead_code)]
// The server-layer sandbox runner for the plan-execute loop. The kernel never
// runs commands; it delegates an approved TestExecution action here. This
// runner executes the command inside an EPHEMERAL container with no network
// and resource caps (memory, cpu, pids), then the container is gone (--rm). It
// mounts the worker's shared WorkerWorkspace at /work, so the tests run on the
// PATCHED code. It opens only when MDX_PLAN_TEST_EXEC=1 (explicit opt-in), and
// returns presence-only evidence: exit code, duration, output byte count, the
// no-network fact, and whether it timed out. Never the command output text.
//
// Hardening (Slice B): the container is pinned by image digest (immutable, not
// a moving tag), runs as a non-root user, drops ALL Linux capabilities, and
// sets no-new-privileges so the test process cannot escalate. The command's
// combined output is byte-capped so a runaway test cannot exhaust host memory,
// and a wall-clock timeout kills it. The mounted workspace is the only writable
// host path.
use crate::harness_worker_workspace::path_str;
use mdx_core::{HarnessSandboxRunContext, HarnessSandboxRunResult, HarnessSandboxRunner};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

// An immutable digest pin (alpine:3.20), not a moving tag and not :latest.
// Override with MDX_SANDBOX_IMAGE for a different pinned image or local digest.
const DEFAULT_IMAGE: &str =
    "alpine@sha256:d9e853e87e55526f6b2917df91a2115c36dd7c696a35be12163d44e6e2a4b6bc";
const MEMORY_CAP: &str = "512m";
const CPU_CAP: &str = "1";
const PIDS_CAP: &str = "256";
const DEFAULT_TIMEOUT_SECS: &str = "900";
// Cap the combined output we read from the container so a runaway test cannot
// exhaust host memory. The container truncates its own output to this bound; we
// only record the (capped) byte count, never the text.
const MAX_OUTPUT_BYTES: usize = 1_000_000;
// A fixed non-root fallback (nobody) when the host process itself is root - the
// sandbox must never run as uid 0.
const NONROOT_FALLBACK: &str = "65534:65534";

pub(crate) struct ServerSandboxRunner {
    // The shared workspace mounted at /work so tests run on the patched code.
    workspace_path: Option<PathBuf>,
}

impl ServerSandboxRunner {
    pub(crate) fn new(workspace_path: Option<PathBuf>) -> Self {
        Self { workspace_path }
    }
}

impl HarnessSandboxRunner for ServerSandboxRunner {
    fn run(&self, context: &HarnessSandboxRunContext<'_>) -> Option<HarnessSandboxRunResult> {
        if std::env::var("MDX_PLAN_TEST_EXEC").ok().as_deref() != Some("1") {
            return None;
        }
        run_in_container(
            context.action_id,
            context.command,
            self.workspace_path.as_deref(),
        )
    }
}

fn run_in_container(
    action_id: &str,
    command: &str,
    workspace_path: Option<&std::path::Path>,
) -> Option<HarnessSandboxRunResult> {
    let image = std::env::var("MDX_SANDBOX_IMAGE")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_IMAGE.to_string());
    let sandbox_id = format!("mdx_sbx_{}_{}", std::process::id(), sanitize(action_id));
    let mount = workspace_path.map(|path| format!("{}:/work", path_str(path)));
    let docker_args = build_docker_args(
        &sandbox_id,
        &image,
        mount.as_deref(),
        command,
        &container_user(),
    );

    let cancellation = crate::process_supervisor::cancellation_for_run(action_id);
    let report = run_docker_supervised(&docker_args, 0, cancellation.as_ref())?;
    if report.timed_out || report.cancelled || report.drain_timed_out {
        cleanup_ephemeral_container(&sandbox_id);
    }
    // The in-container wrapper already capped what reached us; record the count
    // and whether it hit the cap. Never the text.
    let output_bytes = report.output_bytes();
    let output_truncated = output_bytes >= MAX_OUTPUT_BYTES as u64;
    Some(HarnessSandboxRunResult {
        sandbox_id,
        exit_code: report.effective_exit_code(),
        duration_ms: report.duration_ms,
        output_bytes,
        passed: report.passed(),
        network_disabled: true,
        timed_out: report.timed_out,
        output_truncated,
    })
}

/// The session-only output channel for the harness loop. The coding model
/// cannot fix a failure it cannot read, so the loop gets a capped TAIL of
/// the combined output back - and ONLY the loop: this string goes into the
/// model conversation and is never recorded. Receipts keep the presence-only
/// result exactly as before.
const LOOP_OUTPUT_TAIL_BYTES: usize = 8192;

pub(crate) fn run_in_container_with_output(
    action_id: &str,
    command: &str,
    workspace_path: Option<&std::path::Path>,
) -> Option<(HarnessSandboxRunResult, String)> {
    if std::env::var("MDX_PLAN_TEST_EXEC").ok().as_deref() != Some("1") {
        return None;
    }
    let image = std::env::var("MDX_SANDBOX_IMAGE")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_IMAGE.to_string());
    let sandbox_id = format!("mdx_sbx_{}_{}", std::process::id(), sanitize(action_id));
    let mount = workspace_path.map(|path| format!("{}:/work", path_str(path)));
    let docker_args = build_docker_args(
        &sandbox_id,
        &image,
        mount.as_deref(),
        command,
        &container_user(),
    );
    let cancellation = crate::process_supervisor::cancellation_for_run(action_id);
    let report =
        run_docker_supervised(&docker_args, LOOP_OUTPUT_TAIL_BYTES, cancellation.as_ref())?;
    if report.timed_out || report.cancelled || report.drain_timed_out {
        cleanup_ephemeral_container(&sandbox_id);
    }
    let output_bytes = report.output_bytes();
    let output_truncated = output_bytes >= MAX_OUTPUT_BYTES as u64;
    let tail = report.output_tail.clone();
    Some((
        HarnessSandboxRunResult {
            sandbox_id,
            exit_code: report.effective_exit_code(),
            duration_ms: report.duration_ms,
            output_bytes,
            passed: report.passed(),
            network_disabled: true,
            timed_out: report.timed_out,
            output_truncated,
        },
        tail,
    ))
}

// Build the docker run argument vector. Separated so the hardening posture
// (no network, dropped capabilities, no-new-privileges, non-root user, an
// immutable image, resource caps, a scoped mount, and self-cleanup) is unit
// testable without a running daemon.
fn build_docker_args(
    sandbox_id: &str,
    image: &str,
    mount: Option<&str>,
    command: &str,
    user: &str,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "run".into(),
        "--rm".into(),
        "--network=none".into(),
        // Drop every Linux capability and forbid regaining privilege; the test
        // process runs with the least authority the kernel can give it.
        "--cap-drop=ALL".into(),
        "--security-opt=no-new-privileges".into(),
        format!("--user={user}"),
        format!("--name={sandbox_id}"),
        format!("--memory={MEMORY_CAP}"),
        format!("--cpus={CPU_CAP}"),
        format!("--pids-limit={PIDS_CAP}"),
        "--workdir=/work".into(),
    ];
    if let Some(mount) = mount {
        args.push("--volume".into());
        args.push(mount.to_string());
    }
    args.push(image.to_string());
    args.push("sh".into());
    args.push("-c".into());
    args.push(output_capped_command(command));
    args
}

fn cleanup_ephemeral_container(sandbox_id: &str) {
    let mut command = Command::new("docker");
    command.args(["rm", "-f", sandbox_id]);
    let _ = crate::process_supervisor::run_supervised(
        &mut command,
        crate::process_supervisor::ProcessLimits::bounded(Duration::from_secs(10), 0),
        None,
    );
}

fn run_docker_supervised(
    docker_args: &[String],
    output_tail_bytes: usize,
    cancellation: Option<&crate::process_supervisor::ProcessCancellation>,
) -> Option<crate::process_supervisor::ProcessReport> {
    let mut command = Command::new("docker");
    command.args(docker_args);
    crate::process_supervisor::run_supervised(
        &mut command,
        crate::process_supervisor::ProcessLimits::bounded(sandbox_timeout(), output_tail_bytes),
        cancellation,
    )
    .ok()
}

fn sandbox_timeout() -> Duration {
    let seconds = std::env::var("MDX_SANDBOX_TIMEOUT_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or_else(|| DEFAULT_TIMEOUT_SECS.parse().expect("static timeout"));
    Duration::from_secs(seconds.min(86_400))
}

// Wrap the command so its combined output is byte-capped inside the container
// (head -c) while the original exit code is preserved. A runaway test cannot
// then flood the host pipe; the exit code stays honest.
fn output_capped_command(command: &str) -> String {
    format!(
        "( {command} ) >/tmp/mdx_sbx_out 2>&1; __mdx_code=$?; head -c {MAX_OUTPUT_BYTES} /tmp/mdx_sbx_out 2>/dev/null; exit $__mdx_code"
    )
}

// The sandbox must never run as root. Use the host process uid:gid so it owns
// the mounted workspace, but if the host itself is root fall back to nobody.
fn container_user() -> String {
    let uid = id_value("-u");
    let gid = id_value("-g");
    match (uid, gid) {
        (Some(u), Some(g)) if u != "0" && g != "0" => format!("{u}:{g}"),
        _ => NONROOT_FALLBACK.to_string(),
    }
}

fn id_value(flag: &str) -> Option<String> {
    let out = Command::new("id").arg(flag).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if value.is_empty() { None } else { Some(value) }
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness_patch_applier::ServerPatchApplier;
    use crate::harness_worker_workspace::WorkerWorkspace;
    use mdx_core::{HarnessPatchApplier, HarnessPatchApplyContext, HarnessSandboxRunContext};
    use std::path::Path;

    fn docker_available() -> bool {
        Command::new("docker")
            .arg("version")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn the_hardening_posture_is_present_in_the_docker_args() {
        // No daemon needed: the argument vector itself must carry the full
        // least-authority posture.
        let args = build_docker_args(
            "mdx_sbx_test",
            DEFAULT_IMAGE,
            Some("/tmp/ws:/work"),
            "echo hi",
            "65534:65534",
        );
        let joined = args.join(" ");
        assert!(joined.contains("--network=none"), "no network");
        assert!(joined.contains("--cap-drop=ALL"), "all caps dropped");
        assert!(
            joined.contains("--security-opt=no-new-privileges"),
            "no privilege escalation"
        );
        assert!(joined.contains("--user=65534:65534"), "non-root user");
        assert!(joined.contains("--rm"), "ephemeral and self-cleaning");
        assert!(joined.contains("--memory=512m"), "memory cap");
        assert!(joined.contains("--pids-limit=256"), "pids cap");
        assert!(joined.contains("alpine@sha256:"), "immutable digest pin");
        assert!(joined.contains("/tmp/ws:/work"), "scoped workspace mount");
        // The command is byte-capped while preserving its exit code.
        assert!(joined.contains("head -c 1000000"), "output is byte-capped");
        assert!(joined.contains("exit $__mdx_code"), "exit code preserved");
    }

    #[test]
    fn container_user_is_never_root() {
        let user = container_user();
        let uid = user.split(':').next().unwrap();
        assert_ne!(uid, "0", "the sandbox must never run as uid 0");
    }

    #[test]
    fn the_sandbox_runs_as_a_non_root_user() {
        if !docker_available() {
            eprintln!("docker unavailable; skipping non-root sandbox test");
            return;
        }
        let _env = crate::harness_worker_workspace::ENV_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let runner = ServerSandboxRunner::new(None);
        unsafe { std::env::set_var("MDX_PLAN_TEST_EXEC", "1") };
        // Passes only if the in-container uid is not 0.
        let result = runner
            .run(&HarnessSandboxRunContext {
                action_id: "nonroot",
                command: "test \"$(id -u)\" != \"0\"",
                allowed_write_scope: &["crates/"],
                approved_plan_hash: "sha256:test",
            })
            .expect("ran");
        unsafe { std::env::remove_var("MDX_PLAN_TEST_EXEC") };
        assert_eq!(result.exit_code, 0, "the container must not run as root");
    }

    #[test]
    fn output_is_byte_capped() {
        if !docker_available() {
            eprintln!("docker unavailable; skipping output-cap sandbox test");
            return;
        }
        let _env = crate::harness_worker_workspace::ENV_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let runner = ServerSandboxRunner::new(None);
        unsafe { std::env::set_var("MDX_PLAN_TEST_EXEC", "1") };
        // yes floods stdout; the cap must bound what reaches the host.
        let result = runner
            .run(&HarnessSandboxRunContext {
                action_id: "flood",
                command: "yes mdxmdxmdx | head -c 5000000",
                allowed_write_scope: &["crates/"],
                approved_plan_hash: "sha256:test",
            })
            .expect("ran");
        unsafe { std::env::remove_var("MDX_PLAN_TEST_EXEC") };
        assert!(
            result.output_bytes <= MAX_OUTPUT_BYTES as u64,
            "output must be capped, got {}",
            result.output_bytes
        );
        assert!(result.output_truncated, "the cap must be flagged");
    }

    #[test]
    fn a_runaway_command_times_out_and_the_container_is_gone() {
        if !docker_available() {
            eprintln!("docker unavailable; skipping timeout sandbox test");
            return;
        }
        let _env = crate::harness_worker_workspace::ENV_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let runner = ServerSandboxRunner::new(None);
        unsafe { std::env::set_var("MDX_PLAN_TEST_EXEC", "1") };
        unsafe { std::env::set_var("MDX_SANDBOX_TIMEOUT_SECS", "1") };
        let result = runner
            .run(&HarnessSandboxRunContext {
                action_id: "runaway",
                command: "sleep 30",
                allowed_write_scope: &["crates/"],
                approved_plan_hash: "sha256:test",
            })
            .expect("ran");
        unsafe { std::env::remove_var("MDX_SANDBOX_TIMEOUT_SECS") };
        unsafe { std::env::remove_var("MDX_PLAN_TEST_EXEC") };
        assert!(result.timed_out, "the runaway command must time out");
        // --rm means no leftover container with this run's name.
        let ps = Command::new("docker")
            .args(["ps", "-a", "--format", "{{.Names}}"])
            .output()
            .expect("docker ps");
        let names = String::from_utf8_lossy(&ps.stdout);
        assert!(
            !names.contains(&result.sandbox_id),
            "the ephemeral container must be cleaned up"
        );
    }

    fn run(dir: &Path, program: &str, args: &[&str]) {
        let out = Command::new(program)
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("command");
        assert!(out.status.success(), "{program} {args:?} failed");
    }

    #[test]
    fn no_network_and_exit_codes_are_captured() {
        if !docker_available() {
            eprintln!("docker unavailable; skipping sandbox isolation test");
            return;
        }
        let _env = crate::harness_worker_workspace::ENV_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let runner = ServerSandboxRunner::new(None);
        // SAFETY: single-threaded test.
        unsafe { std::env::set_var("MDX_PLAN_TEST_EXEC", "1") };
        let ctx = |cmd: &'static str| HarnessSandboxRunContext {
            action_id: "t1",
            command: cmd,
            allowed_write_scope: &["crates/"],
            approved_plan_hash: "sha256:test",
        };
        let ok = runner.run(&ctx("echo hello")).expect("ran");
        assert_eq!(ok.exit_code, 0);
        assert!(ok.passed && ok.network_disabled);
        let bad = runner.run(&ctx("exit 7")).expect("ran");
        assert_eq!(bad.exit_code, 7);
        let net = runner
            .run(&ctx("wget -q -T 2 -O /dev/null http://1.1.1.1"))
            .expect("ran");
        assert_ne!(net.exit_code, 0, "network must be unreachable");
        unsafe { std::env::remove_var("MDX_PLAN_TEST_EXEC") };
    }

    #[test]
    fn the_sandbox_runs_tests_on_the_patched_code() {
        if !docker_available() {
            eprintln!("docker unavailable; skipping patched-code sandbox test");
            return;
        }
        let _env = crate::harness_worker_workspace::ENV_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // A throwaway repo, a workspace, a patch that adds a marker, then a
        // sandbox command that only passes if it sees the PATCHED file. This is
        // the proof that patch and test share one workspace.
        let repo = std::env::temp_dir().join(format!("mdx_ws_int_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).expect("repo");
        run(&repo, "git", &["init", "-q"]);
        run(&repo, "git", &["config", "user.email", "t@t"]);
        run(&repo, "git", &["config", "user.name", "t"]);
        std::fs::write(repo.join("file.txt"), "before\n").expect("write");
        run(&repo, "git", &["add", "."]);
        run(&repo, "git", &["commit", "-q", "-m", "init"]);

        let workspace = WorkerWorkspace::create(&repo, "tenant_run_int").expect("workspace");
        let diff = "diff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-before\n+PATCHED_MARKER\n";
        let applier = ServerPatchApplier::new(
            workspace.path(),
            workspace.id(),
            vec![("p1".to_string(), diff.to_string())],
        );
        unsafe { std::env::set_var("MDX_PLAN_PATCH_APPLY", "1") };
        unsafe { std::env::set_var("MDX_PLAN_TEST_EXEC", "1") };
        let applied = applier
            .apply(&HarnessPatchApplyContext {
                action_id: "p1",
                target: "file.txt",
                allowed_write_scope: &["file.txt"],
                approved_plan_hash: "sha256:test",
            })
            .expect("applied");
        assert!(applied.applied);

        let runner = ServerSandboxRunner::new(Some(workspace.path().to_path_buf()));
        // This command exits 0 ONLY if the mounted workspace has the patch.
        let result = runner
            .run(&HarnessSandboxRunContext {
                action_id: "test_patched",
                command: "grep -q PATCHED_MARKER /work/file.txt",
                allowed_write_scope: &["file.txt"],
                approved_plan_hash: "sha256:test",
            })
            .expect("ran");
        assert_eq!(
            result.exit_code, 0,
            "the sandbox must see the patched code in the shared workspace"
        );
        assert!(result.passed);

        unsafe { std::env::remove_var("MDX_PLAN_PATCH_APPLY") };
        unsafe { std::env::remove_var("MDX_PLAN_TEST_EXEC") };
        drop(workspace);
        let _ = std::fs::remove_dir_all(&repo);
    }
}
