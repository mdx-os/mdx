// Sandboxed host execution for Forge checks and the policied shell.
//
// This module inverts the old execution default. Before it existed,
// model-influenced commands ran on the host via `sh -c` with the FULL
// inherited process environment - provider keys, source-host credentials,
// everything - and with unrestricted network. That is the frontier's #1
// disqualifier for running untrusted external repos. The new default:
//
//   1. The environment is SCRUBBED to a small allowlist (toolchain paths
//      and caches only). Provider keys and credentials never reach a
//      model-influenced subprocess.
//   2. On macOS the process additionally runs inside a Seatbelt profile:
//      writes confined to the workspace, build caches, and temp dirs;
//      network denied for proof commands (setup commands keep network so
//      dependency installs still work, matching the internet-on-setup /
//      locked-agent-phase pattern).
//   3. The policied shell tool runs READ-ONLY against the workspace:
//      writes are allowed only to build caches and temp dirs, so the
//      model can explore (ls, git diff, run one narrowed test) but can
//      never mutate source outside the governed edit tools - the
//      write-scope law keeps a single enforcement point.
//
// Raw host execution still exists but is an explicit opt-in
// (MDX_FORGE_HOST_CHECKS=1), and the flip is recorded on the ledger as an
// execution-posture receipt. This module grants nothing by itself; the
// loop decides which commands may run and receipts every execution.
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_CHECK_TIMEOUT_SECS: u64 = 900;
const MAX_CHECK_TIMEOUT_SECS: u64 = 86_400;

/// How model-influenced commands execute on this host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HostExecMode {
    /// Legacy raw host execution: full env, no confinement. Explicit
    /// opt-in via MDX_FORGE_HOST_CHECKS=1 only.
    RawHost,
    /// Scrubbed env + Seatbelt confinement (macOS with sandbox-exec).
    ScrubbedSandboxed,
    /// Scrubbed env, no OS confinement available on this platform. The
    /// secrets hole is closed; filesystem/network confinement is not.
    ScrubbedUnconfined,
}

pub(crate) fn host_exec_mode() -> HostExecMode {
    if std::env::var("MDX_FORGE_HOST_CHECKS").ok().as_deref() == Some("1") {
        return HostExecMode::RawHost;
    }
    if seatbelt_available() {
        HostExecMode::ScrubbedSandboxed
    } else {
        HostExecMode::ScrubbedUnconfined
    }
}

pub(crate) fn posture_label(mode: HostExecMode) -> &'static str {
    match mode {
        HostExecMode::RawHost => "raw_host_opt_in",
        HostExecMode::ScrubbedSandboxed => "scrubbed_sandboxed",
        HostExecMode::ScrubbedUnconfined => "scrubbed_unconfined",
    }
}

/// Classify a failed command's output: does it look like the SANDBOX blocked it
/// (network off, or a write/read outside the workspace) rather than the code
/// being wrong? Returns an actionable hint the model can act on in one turn, or
/// None when the failure looks like a genuine code/test error. Pattern-matched
/// on the output tail, lowercased; conservative so a real error is never
/// mislabeled a sandbox denial.
const NETWORK_SIGNS: [&str; 8] = [
    "could not resolve host",
    "temporary failure in name resolution",
    "no address associated with hostname",
    "network is unreachable",
    "connection refused",
    "connection timed out",
    "failed to establish a new connection",
    "proxyerror",
];

/// True when a failed command's output looks like the sandbox blocked NETWORK
/// (as opposed to a filesystem denial or a real error). Used to decide whether
/// an opt-in retry-with-network could unblock it.
pub(crate) fn is_network_denial(exit_code: i32, output: &str) -> bool {
    if exit_code == 0 {
        return false;
    }
    let lower = output.to_ascii_lowercase();
    NETWORK_SIGNS.iter().any(|sign| lower.contains(sign))
}

pub(crate) fn sandbox_denial_hint(exit_code: i32, output: &str) -> Option<String> {
    // exit_code 0 is not a failure.
    if exit_code == 0 {
        return None;
    }
    let lower = output.to_ascii_lowercase();
    if NETWORK_SIGNS.iter().any(|sign| lower.contains(sign)) {
        return Some(
            "This failed because the sandbox blocks network access for PROOF commands. If it needs to download or install something, run that as a SETUP command (setup commands get network); then re-run the proof command offline."
                .to_string(),
        );
    }
    const FS_DENIAL_SIGNS: [&str; 5] = [
        "operation not permitted",
        "read-only file system",
        "permission denied",
        "sandbox-exec",
        "not permitted by sandbox",
    ];
    if FS_DENIAL_SIGNS.iter().any(|sign| lower.contains(sign)) {
        return Some(
            "This looks like the sandbox blocked a write or access outside the workspace. Keep all writes inside the workspace directory; the workspace is writable, paths outside it are not."
                .to_string(),
        );
    }
    None
}

fn seatbelt_available() -> bool {
    cfg!(target_os = "macos") && Path::new("/usr/bin/sandbox-exec").exists()
}

/// The environment a model-influenced subprocess is allowed to see:
/// toolchain discovery and cache locations, nothing that authenticates.
const ENV_ALLOWLIST: [&str; 16] = [
    "PATH",
    "HOME",
    "TMPDIR",
    "LANG",
    "LC_ALL",
    "TERM",
    "USER",
    "LOGNAME",
    "SHELL",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "GOPATH",
    "GOCACHE",
    "GOMODCACHE",
    "JAVA_HOME",
    "GRADLE_USER_HOME",
];

/// Build a `sh -c` command with the scrubbed environment. Everything not
/// on the allowlist - provider keys, tokens, session state - is dropped.
pub(crate) fn apply_scrubbed_environment(command: &mut std::process::Command) {
    command.env_clear();
    for key in ENV_ALLOWLIST {
        if let Ok(value) = std::env::var(key) {
            command.env(key, value);
        }
    }
}

/// Start a network-capable external agent while keeping every source write
/// inside its isolated workspace. Provider CLIs need network and local-user
/// auth reads, so this is deliberately narrower than the network-denied proof
/// sandbox but still denies writes to the repository, home config, and host.
pub(crate) fn networked_workspace_command(
    program: &str,
    workspace: &Path,
    state_write_paths: &[PathBuf],
) -> std::process::Command {
    let mut command = if seatbelt_available() {
        let mut command = std::process::Command::new("/usr/bin/sandbox-exec");
        let mut profile = format!(
            "(version 1)\n(allow default)\n(deny file-write*)\n(allow file-write* (subpath \"{}\"))\n(allow file-write* (subpath \"/dev\"))\n(allow file-write* (subpath \"/private/tmp\"))\n(allow file-write* (subpath \"/private/var/folders\"))",
            profile_path(workspace),
        );
        for path in state_write_paths {
            profile.push_str(&format!(
                "\n(allow file-write* (literal \"{}\") (subpath \"{}\"))",
                profile_path(path),
                profile_path(path),
            ));
        }
        command.arg("-p").arg(profile).arg(program);
        command
    } else {
        std::process::Command::new(program)
    };
    apply_scrubbed_environment(&mut command);
    command
}

/// Start a browser-preview driver with a scrubbed environment and a Seatbelt
/// profile that can read the isolated workspace, write only the workspace,
/// its local evidence directory, browser caches, and temp, and communicate
/// only over loopback. Browser review fails closed on hosts without Seatbelt;
/// a scrubbed but network-open preview process could still exfiltrate source.
pub(crate) fn loopback_browser_command(
    program: &str,
    workspace: &Path,
    evidence_dir: &Path,
    preview_port: u16,
) -> Option<std::process::Command> {
    if !seatbelt_available() {
        return None;
    }
    let profile = loopback_browser_profile(workspace, evidence_dir, preview_port);
    let mut command = std::process::Command::new("/usr/bin/sandbox-exec");
    apply_scrubbed_environment(&mut command);
    command.arg("-p").arg(profile).arg(program);
    Some(command)
}

fn loopback_browser_profile(workspace: &Path, evidence_dir: &Path, preview_port: u16) -> String {
    let mut rules = vec![
        "(version 1)".to_string(),
        "(allow default)".to_string(),
        "(deny file-write*)".to_string(),
        format!(
            "(allow file-write* (subpath \"{}\"))",
            profile_path(workspace)
        ),
        format!(
            "(allow file-write* (subpath \"{}\"))",
            profile_path(evidence_dir)
        ),
        "(allow file-write* (subpath \"/dev\"))".to_string(),
        "(allow file-write* (subpath \"/private/tmp\"))".to_string(),
        "(allow file-write* (subpath \"/private/var/folders\"))".to_string(),
        "(deny network*)".to_string(),
        format!("(allow network-inbound (local ip \"localhost:{preview_port}\"))"),
        format!("(allow network-outbound (remote ip \"localhost:{preview_port}\"))"),
    ];
    if let Some(home) = home_dir() {
        for cache in [".cache", "Library/Caches"] {
            rules.push(format!(
                "(allow file-write* (subpath \"{}\"))",
                profile_path(&home.join(cache))
            ));
        }
    }
    rules.join("\n")
}

fn scrubbed(program: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new(program);
    apply_scrubbed_environment(&mut cmd);
    cmd
}

fn scrubbed_shell(command: &str, cwd: &Path) -> std::process::Command {
    let mut cmd = scrubbed("sh");
    cmd.arg("-c").arg(command).current_dir(cwd);
    cmd
}

fn scrubbed_seatbelt(profile: String, command: &str, cwd: &Path) -> std::process::Command {
    let mut cmd = scrubbed("/usr/bin/sandbox-exec");
    cmd.arg("-p")
        .arg(profile)
        .arg("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd);
    cmd
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn profile_path(path: &Path) -> String {
    canonical(path).to_string_lossy().replace('"', "\\\"")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

/// The Seatbelt profile for check/setup execution: writes confined to the
/// workspace, the build cache, temp dirs, and toolchain caches; network
/// denied unless the phase allows it.
fn check_profile(workspace: &Path, target_dir: &Path, network_allowed: bool) -> String {
    let mut rules = vec![
        "(version 1)".to_string(),
        "(allow default)".to_string(),
        "(deny file-write*)".to_string(),
        format!(
            "(allow file-write* (subpath \"{}\"))",
            profile_path(workspace)
        ),
        format!(
            "(allow file-write* (subpath \"{}\"))",
            profile_path(target_dir)
        ),
        "(allow file-write* (subpath \"/dev\"))".to_string(),
        "(allow file-write* (subpath \"/private/tmp\"))".to_string(),
        "(allow file-write* (subpath \"/private/var/folders\"))".to_string(),
    ];
    if let Ok(tmp) = std::env::var("TMPDIR") {
        rules.push(format!(
            "(allow file-write* (subpath \"{}\"))",
            profile_path(Path::new(&tmp))
        ));
    }
    if let Some(home) = home_dir() {
        for cache in [
            ".cargo",
            ".rustup",
            ".cache",
            ".npm",
            ".gradle",
            "Library/Caches",
            "go",
        ] {
            rules.push(format!(
                "(allow file-write* (subpath \"{}\"))",
                profile_path(&home.join(cache))
            ));
        }
    }
    if !network_allowed {
        rules.push("(deny network*)".to_string());
    }
    rules.join("\n")
}

/// The Seatbelt profile for the policied shell: like the check profile but
/// the WORKSPACE IS NOT WRITABLE and network is always denied. The shell
/// is an exploration tool; source mutation stays with the governed edit
/// tools where the write-scope law is enforced.
///
/// Rule order is load-bearing: Seatbelt's LAST matching rule wins, and
/// Forge workspaces live under the temp dir, so the workspace deny must
/// come AFTER the temp allowances (to carve the workspace back out of
/// them) and the build-cache allow must come after the workspace deny (so
/// a narrowed test can still compile when the cache sits inside it).
fn shell_profile(workspace: &Path, target_dir: &Path) -> String {
    let mut rules = vec![
        "(version 1)".to_string(),
        "(allow default)".to_string(),
        "(deny network*)".to_string(),
        "(deny file-write*)".to_string(),
        "(allow file-write* (subpath \"/dev\"))".to_string(),
        "(allow file-write* (subpath \"/private/tmp\"))".to_string(),
        "(allow file-write* (subpath \"/private/var/folders\"))".to_string(),
    ];
    if let Ok(tmp) = std::env::var("TMPDIR") {
        rules.push(format!(
            "(allow file-write* (subpath \"{}\"))",
            profile_path(Path::new(&tmp))
        ));
    }
    if let Some(home) = home_dir() {
        for cache in [".cargo", ".cache", "Library/Caches", "go"] {
            rules.push(format!(
                "(allow file-write* (subpath \"{}\"))",
                profile_path(&home.join(cache))
            ));
        }
    }
    rules.push(format!(
        "(deny file-write* (subpath \"{}\"))",
        profile_path(workspace)
    ));
    rules.push(format!(
        "(allow file-write* (subpath \"{}\"))",
        profile_path(target_dir)
    ));
    rules.join("\n")
}

pub(crate) fn run_and_cap_for_run(
    mut cmd: std::process::Command,
    run_id: &str,
) -> (bool, i32, u64, String) {
    let cancellation = crate::process_supervisor::cancellation_for_run(run_id);
    run_and_cap_with_cancellation(&mut cmd, cancellation.as_ref())
}

fn run_and_cap_with_cancellation(
    cmd: &mut std::process::Command,
    cancellation: Option<&crate::process_supervisor::ProcessCancellation>,
) -> (bool, i32, u64, String) {
    let started = std::time::Instant::now();
    const TAIL: usize = 8192;
    let timeout = check_timeout();
    let report = match crate::process_supervisor::run_supervised(
        cmd,
        crate::process_supervisor::ProcessLimits::bounded(timeout, TAIL),
        cancellation,
    ) {
        Ok(report) => report,
        Err(error) => {
            return (
                false,
                -1,
                started.elapsed().as_millis() as u64,
                format!("could not run the command: {error}"),
            );
        }
    };
    let passed = report.passed();
    let exit_code = report.effective_exit_code();
    let mut tail = report.output_tail;
    if report.timed_out {
        tail = format!(
            "command timed out after {} seconds and its process group was terminated\n{tail}",
            timeout.as_secs()
        );
    }
    if report.cancelled {
        tail = format!("command cancelled by operator; its process group was terminated\n{tail}");
    }
    if report.drain_timed_out {
        tail.push_str("\noutput drain reached its bounded deadline");
    }
    (passed, exit_code, report.duration_ms, tail)
}

fn check_timeout() -> Duration {
    Duration::from_secs(
        std::env::var("MDX_FORGE_CHECK_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_CHECK_TIMEOUT_SECS)
            .min(MAX_CHECK_TIMEOUT_SECS),
    )
}

fn resolved_target_dir(repo_root: &Path, target_dir_override: Option<&str>) -> PathBuf {
    match target_dir_override {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => repo_root.join("target"),
    }
}

/// Run one check or setup command with the scrubbed environment and, when
/// available, Seatbelt confinement. Setup commands keep network so
/// dependency installs work; proof commands run with network denied.
pub(crate) fn run_scrubbed_confined(
    command: &str,
    workspace_path: &Path,
    repo_root: &Path,
    target_dir_override: Option<&str>,
    network_allowed: bool,
) -> (bool, i32, u64, String) {
    run_scrubbed_confined_with_cancellation(
        command,
        workspace_path,
        repo_root,
        target_dir_override,
        network_allowed,
        None,
    )
}

pub(crate) fn run_scrubbed_confined_for_run(
    run_id: &str,
    command: &str,
    workspace_path: &Path,
    repo_root: &Path,
    target_dir_override: Option<&str>,
    network_allowed: bool,
) -> (bool, i32, u64, String) {
    let cancellation = crate::process_supervisor::cancellation_for_run(run_id);
    run_scrubbed_confined_with_cancellation(
        command,
        workspace_path,
        repo_root,
        target_dir_override,
        network_allowed,
        cancellation.as_ref(),
    )
}

fn run_scrubbed_confined_with_cancellation(
    command: &str,
    workspace_path: &Path,
    repo_root: &Path,
    target_dir_override: Option<&str>,
    network_allowed: bool,
    cancellation: Option<&crate::process_supervisor::ProcessCancellation>,
) -> (bool, i32, u64, String) {
    let target_dir = resolved_target_dir(repo_root, target_dir_override);
    let mut cmd = if seatbelt_available() {
        let profile = check_profile(workspace_path, &target_dir, network_allowed);
        scrubbed_seatbelt(profile, command, workspace_path)
    } else {
        scrubbed_shell(command, workspace_path)
    };
    cmd.env("CARGO_TARGET_DIR", &target_dir);
    run_and_cap_with_cancellation(&mut cmd, cancellation)
}

/// Run one exploration command in the policied shell: scrubbed env,
/// network denied, workspace read-only. Only available when OS-level
/// confinement exists - without it a shell write could bypass the
/// write-scope law, so the tool fails closed instead.
pub(crate) fn run_shell_readonly_for_run(
    run_id: &str,
    command: &str,
    workspace_path: &Path,
    repo_root: &Path,
    target_dir_override: Option<&str>,
) -> Result<(i32, u64, String), &'static str> {
    let cancellation = crate::process_supervisor::cancellation_for_run(run_id);
    run_shell_readonly_with_cancellation(
        command,
        workspace_path,
        repo_root,
        target_dir_override,
        cancellation.as_ref(),
    )
}

fn run_shell_readonly_with_cancellation(
    command: &str,
    workspace_path: &Path,
    repo_root: &Path,
    target_dir_override: Option<&str>,
    cancellation: Option<&crate::process_supervisor::ProcessCancellation>,
) -> Result<(i32, u64, String), &'static str> {
    if host_exec_mode() != HostExecMode::ScrubbedSandboxed {
        return Err(
            "the shell tool needs OS sandbox confinement, which this host does not provide; use the allowlisted check commands instead",
        );
    }
    let target_dir = resolved_target_dir(repo_root, target_dir_override);
    let profile = shell_profile(workspace_path, &target_dir);
    let mut cmd = scrubbed_seatbelt(profile, command, workspace_path);
    cmd.env("CARGO_TARGET_DIR", &target_dir);
    let (_, exit_code, duration_ms, tail) = run_and_cap_with_cancellation(&mut cmd, cancellation);
    Ok((exit_code, duration_ms, tail))
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn active_run_cancellation_interrupts_a_capped_host_command() {
        let run_id = "forge_exec_active_cancel";
        let _guard = crate::process_supervisor::register_run_cancellation(run_id);
        let canceller = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            assert!(crate::process_supervisor::cancel_run(run_id));
        });
        let mut command = std::process::Command::new("sh");
        command.arg("-c").arg("sleep 30 & wait");

        let (passed, exit_code, duration_ms, tail) = super::run_and_cap_for_run(command, run_id);
        canceller.join().expect("canceller");

        assert!(!passed);
        assert_eq!(exit_code, 130);
        assert!(duration_ms < 2_000);
        assert!(tail.contains("cancelled by operator"));
    }

    #[test]
    fn is_network_denial_only_for_network_failures() {
        assert!(super::is_network_denial(1, "curl: Could not resolve host"));
        assert!(!super::is_network_denial(1, "AssertionError: nope"));
        assert!(!super::is_network_denial(1, "Operation not permitted"));
        assert!(!super::is_network_denial(0, "could not resolve host"));
    }

    #[test]
    fn sandbox_denial_hint_classifies_network_and_fs_not_code_errors() {
        assert!(
            super::sandbox_denial_hint(1, "curl: (6) Could not resolve host: pypi.org")
                .unwrap()
                .contains("SETUP")
        );
        assert!(
            super::sandbox_denial_hint(
                1,
                "OSError: [Errno 1] Operation not permitted: '/etc/hosts'"
            )
            .unwrap()
            .contains("outside the workspace")
        );
        // a real test failure is NOT a sandbox denial
        assert!(
            super::sandbox_denial_hint(1, "AssertionError: expected 90.0, got 100.0").is_none()
        );
        // exit 0 never a denial
        assert!(super::sandbox_denial_hint(0, "could not resolve host").is_none());
    }

    use super::*;

    fn temp_workspace(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mdx_exec_sandbox_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("workspace dir");
        dir
    }

    #[test]
    fn the_scrubbed_environment_never_leaks_secrets_to_the_subprocess() {
        let workspace = temp_workspace("scrub");
        // SAFETY-adjacent test hygiene: a uniquely named var so parallel
        // tests cannot collide on it.
        unsafe { std::env::set_var("MDX_TEST_SECRET_LEAK_PROBE", "sk-super-secret") };
        let (_, exit_code, _, tail) = run_scrubbed_confined(
            "printf '%s' \"${MDX_TEST_SECRET_LEAK_PROBE:-CLEAN}\"",
            &workspace,
            &workspace,
            None,
            true,
        );
        assert_eq!(exit_code, 0);
        assert!(
            tail.contains("CLEAN") && !tail.contains("sk-super-secret"),
            "secret leaked into the scrubbed subprocess: {tail}"
        );
        unsafe { std::env::remove_var("MDX_TEST_SECRET_LEAK_PROBE") };
    }

    #[test]
    fn the_scrubbed_environment_keeps_the_toolchain_path() {
        let workspace = temp_workspace("path");
        let (passed, exit_code, _, tail) =
            run_scrubbed_confined("printf '%s' \"$PATH\"", &workspace, &workspace, None, true);
        assert!(passed, "exit={exit_code} tail={tail}");
        assert!(!tail.trim().is_empty(), "PATH must survive the scrub");
    }

    #[test]
    fn raw_host_remains_an_explicit_opt_in() {
        // Without MDX_FORGE_HOST_CHECKS=1 the mode is never RawHost. This
        // is the inverted default the whole module exists for. (Reading
        // the ambient env in tests is fine: nothing in CI sets the flag.)
        if std::env::var("MDX_FORGE_HOST_CHECKS").ok().as_deref() != Some("1") {
            assert_ne!(host_exec_mode(), HostExecMode::RawHost);
        }
    }

    #[test]
    fn browser_profile_allows_only_the_reserved_preview_port_and_scoped_writes() {
        let workspace = Path::new("/tmp/mdx-browser-workspace");
        let evidence = Path::new("/tmp/mdx-browser-evidence");
        let profile = loopback_browser_profile(workspace, evidence, 43129);
        assert!(profile.contains("(deny network*)"));
        assert!(profile.contains("(allow network-inbound (local ip \"localhost:43129\"))"));
        assert!(profile.contains("(allow network-outbound (remote ip \"localhost:43129\"))"));
        assert!(profile.contains("/tmp/mdx-browser-workspace"));
        assert!(profile.contains("/tmp/mdx-browser-evidence"));
        assert!(!profile.contains("localhost:*"));
        assert!(!profile.contains("network-outbound (remote ip \"*:*\")"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_shell_cannot_write_the_workspace_but_temp_stays_writable() {
        if !Path::new("/usr/bin/sandbox-exec").exists() {
            return;
        }
        let workspace = temp_workspace("shellro");
        let (exit_code, _, _tail) = run_shell_readonly_with_cancellation(
            "touch mutated_by_shell.txt",
            &workspace,
            &workspace,
            None,
            None,
        )
        .expect("shell available under seatbelt");
        assert_ne!(exit_code, 0, "workspace write must be denied in the shell");
        assert!(
            !workspace.join("mutated_by_shell.txt").exists(),
            "the shell mutated the workspace"
        );

        let (exit_code, _, _) = run_shell_readonly_with_cancellation(
            "touch \"$TMPDIR/mdx_shell_tmp_probe\" && rm \"$TMPDIR/mdx_shell_tmp_probe\"",
            &workspace,
            &workspace,
            None,
            None,
        )
        .expect("shell available under seatbelt");
        assert_eq!(exit_code, 0, "temp writes must stay allowed in the shell");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn checks_can_write_the_workspace_under_confinement() {
        if !Path::new("/usr/bin/sandbox-exec").exists() {
            return;
        }
        let workspace = temp_workspace("checkrw");
        let (passed, exit_code, _, tail) = run_scrubbed_confined(
            "touch check_artifact.txt && rm check_artifact.txt",
            &workspace,
            &workspace,
            None,
            false,
        );
        assert!(passed, "exit={exit_code} tail={tail}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_proof_command_cannot_reach_the_network_under_confinement() {
        if !Path::new("/usr/bin/sandbox-exec").exists() {
            return;
        }
        let workspace = temp_workspace("netdeny");
        // Resolving and dialing anything must fail under (deny network*).
        // 1.1.1.1:443 avoids DNS so the failure is the sandbox, not a
        // resolver quirk; curl exits non-zero either way while the same
        // command with network_allowed=true succeeds on hosts with
        // internet. We assert only the denied case to stay hermetic.
        let (_, exit_code, _, _) = run_scrubbed_confined(
            "curl --silent --max-time 4 https://1.1.1.1/ >/dev/null 2>&1",
            &workspace,
            &workspace,
            None,
            false,
        );
        assert_ne!(exit_code, 0, "network must be denied for proof commands");
    }
}
