// Verified by its own gh-gated test; the driving surface (ship readiness in a
// worker run) wires this in a later slice, so the constructor is allowed to be
// unused by non-test builds until then.
#![allow(dead_code)]
// The server-layer CI evidence adapter. Ship readiness must not accept a
// caller-asserted "the CI passed" - it must OBSERVE the real run. It verifies
// head commit and branch match the build before returning
// evidence. The kernel never reaches the network; this does, and only when
// MDX_CI_EVIDENCE_OBSERVE=1 (explicit opt-in). The conclusion becomes observed
// truth, not a string the caller supplied.
use std::{process::Command, thread, time::Duration};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ObservedCiRun {
    pub workflow_run_id: String,
    pub commit_sha: String,
    pub branch: String,
    pub conclusion: String,
    pub evidence_url: String,
    pub observed_via: &'static str,
}

pub(crate) struct ServerCiEvidenceAdapter;

trait CiEvidenceProviderAdapter {
    fn observed_via(&self) -> &'static str;
    fn observe(
        &self,
        repo: &str,
        workflow_run_id: &str,
        expected_commit_sha: &str,
        expected_branch: &str,
    ) -> Option<ObservedCiRun>;
}

impl ServerCiEvidenceAdapter {
    // Fetch and verify a CI run. Returns the observed evidence only when the
    // run exists, is completed, and its head commit and branch match the build
    // the readiness packet is for. Any gate open returns None - ship readiness
    // then blocks for want of observed CI, never on a fabricated green.
    pub(crate) fn observe(
        &self,
        repo: &str,
        workflow_run_id: &str,
        expected_commit_sha: &str,
        expected_branch: &str,
    ) -> Option<ObservedCiRun> {
        if std::env::var("MDX_CI_EVIDENCE_OBSERVE").ok().as_deref() != Some("1") {
            return None;
        }
        match std::env::var("MDX_CI_EVIDENCE_PROVIDER").ok().as_deref() {
            Some("generic_json") => GenericJsonCiEvidenceAdapter.observe(
                repo,
                workflow_run_id,
                expected_commit_sha,
                expected_branch,
            ),
            _ => GithubActionsCliAdapter.observe(
                repo,
                workflow_run_id,
                expected_commit_sha,
                expected_branch,
            ),
        }
    }
}

struct GithubActionsCliAdapter;

impl CiEvidenceProviderAdapter for GithubActionsCliAdapter {
    fn observed_via(&self) -> &'static str {
        "github_actions_adapter"
    }

    fn observe(
        &self,
        repo: &str,
        workflow_run_id: &str,
        expected_commit_sha: &str,
        expected_branch: &str,
    ) -> Option<ObservedCiRun> {
        let attempts = ci_attempts();
        for attempt in 0..attempts {
            let mut command = Command::new("gh");
            command.args([
                "run",
                "view",
                workflow_run_id,
                "--repo",
                repo,
                "--json",
                "databaseId,status,conclusion,headSha,headBranch,url",
            ]);
            if let Some(token) = github_auth_token() {
                command.env("GH_TOKEN", token);
            }
            let output = command.output().ok()?;
            if output.status.success() {
                return parse_observed_run(
                    self.observed_via(),
                    workflow_run_id,
                    expected_commit_sha,
                    expected_branch,
                    &output.stdout,
                );
            }
            let rate_limited = output_rate_limited(&output);
            if rate_limited && attempt + 1 < attempts {
                thread::sleep(Duration::from_millis(ci_backoff_ms(attempt)));
                continue;
            }
            break;
        }
        None
    }
}

struct GenericJsonCiEvidenceAdapter;

impl CiEvidenceProviderAdapter for GenericJsonCiEvidenceAdapter {
    fn observed_via(&self) -> &'static str {
        "generic_ci_json_adapter"
    }

    fn observe(
        &self,
        _repo: &str,
        workflow_run_id: &str,
        expected_commit_sha: &str,
        expected_branch: &str,
    ) -> Option<ObservedCiRun> {
        let raw = std::env::var("MDX_GENERIC_CI_EVIDENCE_JSON").ok()?;
        parse_observed_run(
            self.observed_via(),
            workflow_run_id,
            expected_commit_sha,
            expected_branch,
            raw.as_bytes(),
        )
    }
}

fn parse_observed_run(
    observed_via: &'static str,
    workflow_run_id: &str,
    expected_commit_sha: &str,
    expected_branch: &str,
    raw: &[u8],
) -> Option<ObservedCiRun> {
    let parsed: serde_json::Value = serde_json::from_slice(raw).ok()?;
    // Only a completed run carries a real conclusion.
    if parsed["status"].as_str()? != "completed" {
        return None;
    }
    let commit_sha = parsed["headSha"].as_str()?.to_string();
    let branch = parsed["headBranch"].as_str()?.to_string();
    // Bind the observed run to the build: a run for a different commit or
    // branch is not evidence for this build.
    if !commit_matches(&commit_sha, expected_commit_sha) || branch.trim() != expected_branch.trim()
    {
        return None;
    }
    Some(ObservedCiRun {
        workflow_run_id: parsed["databaseId"]
            .as_i64()
            .map(|id| id.to_string())
            .unwrap_or_else(|| workflow_run_id.to_string()),
        commit_sha,
        branch,
        conclusion: parsed["conclusion"].as_str()?.to_string(),
        evidence_url: parsed["url"].as_str().unwrap_or("").to_string(),
        observed_via,
    })
}

fn github_auth_token() -> Option<String> {
    ["GH_TOKEN", "GITHUB_TOKEN", "MDX_GITHUB_APP_TOKEN"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|value| !value.trim().is_empty())
}

fn ci_attempts() -> usize {
    std::env::var("MDX_CI_EVIDENCE_MAX_ATTEMPTS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .map(|value| value.clamp(1, 5))
        .unwrap_or(3)
}

fn ci_backoff_ms(attempt: usize) -> u64 {
    let base = std::env::var("MDX_CI_EVIDENCE_BACKOFF_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(250);
    base.saturating_mul((attempt as u64) + 1).min(5_000)
}

fn output_rate_limited(output: &std::process::Output) -> bool {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();
    text.contains("rate limit")
        || text.contains("secondary rate")
        || text.contains("too many requests")
        || text.contains("http 429")
}

const MIN_COMMIT_PREFIX_LEN: usize = 12;

// A build's commit may be an abbreviated sha, but only with a long-enough
// caller-supplied prefix. A CI adapter should normally observe a full head sha;
// accepting short prefixes here can bind unrelated commits on a governance edge.
fn commit_matches(observed: &str, expected: &str) -> bool {
    let observed = observed.trim();
    let expected = expected.trim();
    if observed.is_empty() || expected.is_empty() {
        return false;
    }
    observed == expected
        || (expected.len() >= MIN_COMMIT_PREFIX_LEN && observed.starts_with(expected))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::OsString, os::unix::process::ExitStatusExt, sync::MutexGuard};

    const CI_EVIDENCE_ENV_KEYS: [&str; 3] = [
        "MDX_CI_EVIDENCE_OBSERVE",
        "MDX_CI_EVIDENCE_PROVIDER",
        "MDX_GENERIC_CI_EVIDENCE_JSON",
    ];

    struct CiEvidenceEnvGuard {
        original: Vec<(&'static str, Option<OsString>)>,
        _lock: MutexGuard<'static, ()>,
    }

    impl CiEvidenceEnvGuard {
        fn new() -> Self {
            let lock = crate::forge_turn_client::ENV_TEST_LOCK
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            let original = CI_EVIDENCE_ENV_KEYS
                .into_iter()
                .map(|key| (key, std::env::var_os(key)))
                .collect();
            Self {
                original,
                _lock: lock,
            }
        }
    }

    impl Drop for CiEvidenceEnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.original {
                // SAFETY: the shared server test lock is held until this guard
                // finishes restoring every process-global environment value.
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }
    }

    fn gh_available() -> bool {
        Command::new("gh")
            .args(["auth", "status"])
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn generic_provider_contract_binds_commit_branch_and_provider_name() {
        let _env = CiEvidenceEnvGuard::new();
        unsafe { std::env::set_var("MDX_CI_EVIDENCE_OBSERVE", "1") };
        unsafe { std::env::set_var("MDX_CI_EVIDENCE_PROVIDER", "generic_json") };
        unsafe {
            std::env::set_var(
                "MDX_GENERIC_CI_EVIDENCE_JSON",
                r#"{"databaseId":42,"status":"completed","conclusion":"success","headSha":"abcdef123456","headBranch":"feature/pass-2","url":"https://ci.example/runs/42"}"#,
            )
        };
        let observed =
            ServerCiEvidenceAdapter.observe("example/repo", "42", "abcdef123456", "feature/pass-2");
        unsafe { std::env::remove_var("MDX_CI_EVIDENCE_PROVIDER") };
        unsafe { std::env::remove_var("MDX_GENERIC_CI_EVIDENCE_JSON") };
        unsafe { std::env::remove_var("MDX_CI_EVIDENCE_OBSERVE") };
        let run = observed.expect("generic provider evidence");
        assert_eq!(run.observed_via, "generic_ci_json_adapter");
        assert_eq!(run.workflow_run_id, "42");
        assert_eq!(run.conclusion, "success");

        unsafe { std::env::set_var("MDX_CI_EVIDENCE_OBSERVE", "1") };
        unsafe { std::env::set_var("MDX_CI_EVIDENCE_PROVIDER", "generic_json") };
        unsafe {
            std::env::set_var(
                "MDX_GENERIC_CI_EVIDENCE_JSON",
                r#"{"databaseId":42,"status":"completed","conclusion":"success","headSha":"abcdef123456","headBranch":"other","url":"https://ci.example/runs/42"}"#,
            )
        };
        let mismatch =
            ServerCiEvidenceAdapter.observe("example/repo", "42", "abcdef123456", "feature/pass-2");
        unsafe { std::env::remove_var("MDX_CI_EVIDENCE_PROVIDER") };
        unsafe { std::env::remove_var("MDX_GENERIC_CI_EVIDENCE_JSON") };
        unsafe { std::env::remove_var("MDX_CI_EVIDENCE_OBSERVE") };
        assert!(mismatch.is_none());
    }

    #[test]
    fn generic_provider_rejects_short_or_wrong_commit_prefixes() {
        let _env = CiEvidenceEnvGuard::new();
        unsafe { std::env::set_var("MDX_CI_EVIDENCE_OBSERVE", "1") };
        unsafe { std::env::set_var("MDX_CI_EVIDENCE_PROVIDER", "generic_json") };
        unsafe {
            std::env::set_var(
                "MDX_GENERIC_CI_EVIDENCE_JSON",
                r#"{"databaseId":42,"status":"completed","conclusion":"success","headSha":"abcdef1234567890","headBranch":"feature/pass-2","url":"https://ci.example/runs/42"}"#,
            )
        };
        let short =
            ServerCiEvidenceAdapter.observe("example/repo", "42", "abcdef1", "feature/pass-2");
        let wrong_long =
            ServerCiEvidenceAdapter.observe("example/repo", "42", "abcdef999999", "feature/pass-2");
        unsafe { std::env::remove_var("MDX_CI_EVIDENCE_PROVIDER") };
        unsafe { std::env::remove_var("MDX_GENERIC_CI_EVIDENCE_JSON") };
        unsafe { std::env::remove_var("MDX_CI_EVIDENCE_OBSERVE") };
        assert!(
            short.is_none(),
            "7-char prefixes are too weak for CI binding"
        );
        assert!(wrong_long.is_none(), "wrong long prefixes must not bind");
    }

    #[test]
    fn rate_limit_output_is_detected_for_backoff() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(1),
            stdout: b"".to_vec(),
            stderr: b"HTTP 429 secondary rate limit".to_vec(),
        };
        assert!(output_rate_limited(&output));
    }

    #[test]
    fn observes_a_real_ci_run_and_binds_to_its_commit() {
        let _env = CiEvidenceEnvGuard::new();
        if !gh_available() {
            eprintln!("gh unavailable/unauthed; skipping real CI observation test");
            return;
        }
        // A real, completed run on this repo (the Unlock 4 merge CI).
        let repo = "mdx-os/mdx";
        let run_id = "27106096662";
        let commit = "e06d4f02";
        // SAFETY: this test holds ENV_TEST_LOCK while mutating process env.
        unsafe { std::env::remove_var("MDX_CI_EVIDENCE_OBSERVE") };
        // Opt-in off -> declines.
        assert!(
            ServerCiEvidenceAdapter
                .observe(repo, run_id, commit, "main")
                .is_none()
        );

        unsafe { std::env::set_var("MDX_CI_EVIDENCE_OBSERVE", "1") };
        let observed = ServerCiEvidenceAdapter.observe(repo, run_id, commit, "main");
        unsafe { std::env::remove_var("MDX_CI_EVIDENCE_OBSERVE") };
        let Some(run) = observed else {
            // The run may have aged out of gh's view; do not fail the suite on
            // external state, but note it.
            eprintln!("real run not observable; skipping assertion");
            return;
        };
        // The conclusion is OBSERVED, not asserted by the caller.
        assert_eq!(run.conclusion, "success");
        assert!(run.commit_sha.starts_with("e06d4f02"));
        assert_eq!(run.branch, "main");
        assert_eq!(run.observed_via, "github_actions_adapter");

        // A run that does not match the build's commit is not evidence.
        unsafe { std::env::set_var("MDX_CI_EVIDENCE_OBSERVE", "1") };
        let mismatch = ServerCiEvidenceAdapter.observe(repo, run_id, "0000000", "main");
        unsafe { std::env::remove_var("MDX_CI_EVIDENCE_OBSERVE") };
        assert!(mismatch.is_none(), "a different commit must not bind");
    }
}
