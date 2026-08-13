// The diff a Forge run produced, for review. A finished run committed its
// work to a branch; this route returns that commit's diff so a person can
// read the change in the product - the keystone of the ship decision,
// since you cannot responsibly ship what you cannot see. The diff is the
// run's own commit (branch tip against its parent), base-independent and
// exactly the work the agent did. Read-only: rendering a diff opens no
// authority and changes nothing.
use crate::RouteResponse;
use mdx_core::{MdxKernel, json_string_literal};
use std::collections::BTreeSet;
use std::sync::{Arc, RwLock};

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/run-diff.json" {
        return None;
    }
    Some(handle(method, body, kernel))
}

fn handle(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    // A read, but POST keeps the run_id off the URL and matches the other
    // forge routes' shape.
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let run_id = json_string_field(body, "run_id").unwrap_or_default();
    if run_id.trim().is_empty() {
        return Ok(refusal("name the run whose change you want to review"));
    }
    // The branch the run produced and the repo it lives in, both from the
    // run's own receipts - a run targeting a connected repo has its diff
    // in that repo, not MDx.
    let (branch, run_root) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        (
            branch_for_run(&kernel, &run_id),
            repo_root_for_run(&kernel, &run_id),
        )
    };
    let Some(branch) = branch else {
        return Ok(refusal(
            "this run has no branch yet - it is still working, or it stopped without committing",
        ));
    };

    let repo_root = run_root
        .map(std::path::PathBuf::from)
        .unwrap_or_else(repo_root);
    // The run's own commit: the branch tip against its parent. One commit,
    // so this is exactly the change the agent made.
    let diff = match git_show(&repo_root, &branch) {
        Some(text) => text,
        None => {
            return Ok(refusal(
                "the branch could not be read - it may have been cleaned up",
            ));
        }
    };
    let all_files = split_into_files(&diff);
    let artifact_files: Vec<&FileDiff> = all_files
        .iter()
        .filter(|file| crate::forge_repo_profile::is_artifact_path(&file.path))
        .collect();
    let files: Vec<&FileDiff> = all_files
        .iter()
        .filter(|file| !crate::forge_repo_profile::is_artifact_path(&file.path))
        .collect();
    let proof_signals = file_proof_signals(kernel, &run_id, &files)?;
    let files_json: Vec<String> = files
        .iter()
        .zip(proof_signals.iter())
        .map(|(file, proof_signal)| {
            format!(
                r#"{{"path":{},"added":{},"removed":{},"patch":{},"hunks":[{}],"trail_step_count":{},"error_step_count":{},"retry_count":{},"checks_touching":[{}],"agent_confidence":{}}}"#,
                json_string_literal(&file.path),
                file.added,
                file.removed,
                json_string_literal(&file.patch),
                hunks_json(&file.hunks),
                proof_signal.trail_step_count,
                proof_signal.error_step_count,
                proof_signal.retry_count,
                json_array(&proof_signal.checks_touching),
                json_string_literal(proof_signal.agent_confidence()),
            )
        })
        .collect();
    let artifact_files_json: Vec<String> = artifact_files
        .iter()
        .take(40)
        .map(|file| {
            format!(
                r#"{{"path":{},"added":{},"removed":{},"reason":{}}}"#,
                json_string_literal(&file.path),
                file.added,
                file.removed,
                json_string_literal(crate::forge_repo_profile::artifact_reason(&file.path)),
            )
        })
        .collect();
    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-run-diff","run_id":{},"branch":{},"file_count":{},"review_file_count":{},"artifact_count":{},"artifact_files_sample":[{}],"files":[{}],"production_write_allowed":false}}"#,
            json_string_literal(&run_id),
            json_string_literal(&branch),
            all_files.len(),
            files_json.len(),
            artifact_files.len(),
            artifact_files_json.join(","),
            files_json.join(","),
        ),
    ))
}

/// One changed file's path and line counts, no patch body. The Review Packet
/// reuses this to classify and order a run's change without re-running git or
/// duplicating the diff parse; the patch text stays with the diff route.
#[derive(Clone)]
pub(crate) struct RunDiffFile {
    pub path: String,
    pub added: u32,
    pub removed: u32,
}

/// One changed file's path and line counts for the run's own commit (branch
/// tip against its parent). Ok(None) when the run has no branch yet. The Review
/// Packet uses this to classify and order the run's change - the diff the agent
/// actually made - without re-running git or duplicating the diff parse.
pub(crate) fn run_diff_files(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
) -> Result<Option<Vec<RunDiffFile>>, String> {
    let (branch, run_root) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        (
            branch_for_run(&kernel, run_id),
            repo_root_for_run(&kernel, run_id),
        )
    };
    let Some(branch) = branch else {
        return Ok(None);
    };
    let repo_root = run_root
        .map(std::path::PathBuf::from)
        .unwrap_or_else(repo_root);
    let Some(diff) = git_show(&repo_root, &branch) else {
        return Ok(None);
    };
    Ok(Some(
        split_into_files(&diff)
            .into_iter()
            .map(|file| RunDiffFile {
                path: file.path,
                added: file.added,
                removed: file.removed,
            })
            .collect(),
    ))
}

/// The files a run's branch changed relative to its merge base. Ok(None) when
/// the run has no branch yet or the base cannot be resolved. Used by the
/// self-modification firewall so a multi-commit branch cannot hide a protected
/// edit in an earlier commit.
pub(crate) fn run_diff_files_against_base(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
    base_branch: &str,
) -> Result<Option<Vec<RunDiffFile>>, String> {
    let (branch, run_root) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        (
            branch_for_run(&kernel, run_id),
            repo_root_for_run(&kernel, run_id),
        )
    };
    let Some(branch) = branch else {
        return Ok(None);
    };
    let repo_root = run_root
        .map(std::path::PathBuf::from)
        .unwrap_or_else(repo_root);
    let Some(diff) = git_diff_against_base(&repo_root, &branch, base_branch) else {
        return Ok(None);
    };
    Ok(Some(
        split_into_files(&diff)
            .into_iter()
            .map(|file| RunDiffFile {
                path: file.path,
                added: file.added,
                removed: file.removed,
            })
            .collect(),
    ))
}

/// The raw unified diff of a run's branch against its merge base. Ok(None)
/// when the run has no branch or the base cannot be resolved.
pub(crate) fn run_diff_text_against_base(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
    base_branch: &str,
) -> Result<Option<String>, String> {
    let (branch, run_root) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        (
            branch_for_run(&kernel, run_id),
            repo_root_for_run(&kernel, run_id),
        )
    };
    let Some(branch) = branch else {
        return Ok(None);
    };
    let repo_root = run_root
        .map(std::path::PathBuf::from)
        .unwrap_or_else(repo_root);
    Ok(git_diff_against_base(&repo_root, &branch, base_branch))
}

/// The raw unified diff a run committed, for a reviewer to read cold. Ok(None)
/// when the run has no branch. Shares the same git path as the file list.
pub(crate) fn run_diff_text(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
) -> Result<Option<String>, String> {
    let (branch, run_root) = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        (
            branch_for_run(&kernel, run_id),
            repo_root_for_run(&kernel, run_id),
        )
    };
    let Some(branch) = branch else {
        return Ok(None);
    };
    let repo_root = run_root
        .map(std::path::PathBuf::from)
        .unwrap_or_else(repo_root);
    Ok(git_show(&repo_root, &branch))
}

// The repo root a run worked in, recorded on its run_started event. None
// means the run predates multi-repo or targeted MDx itself.
pub(crate) fn repo_root_for_run(kernel: &MdxKernel, run_id: &str) -> Option<String> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| receipt.payload.get("run_id").map(String::as_str) == Some(run_id))
        .filter_map(|receipt| {
            if let Some(root) = receipt
                .payload
                .get("repo_root")
                .filter(|root| !root.is_empty())
            {
                return Some(root.clone());
            }
            let detail = receipt.payload.get("detail")?;
            detail
                .split("repo_root=")
                .nth(1)
                .map(|rest| rest.split(' ').next().unwrap_or("").to_string())
        })
        .find(|root| !root.is_empty())
}

pub(crate) fn branch_for_run(kernel: &MdxKernel, run_id: &str) -> Option<String> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| receipt.payload.get("run_id").map(String::as_str) == Some(run_id))
        .filter_map(|receipt| {
            let detail = receipt.payload.get("detail")?;
            detail
                .strip_prefix("branch=")
                .map(|rest| rest.split(' ').next().unwrap_or("").to_string())
        })
        .find(|branch| !branch.is_empty())
}

struct FileDiff {
    path: String,
    patch: String,
    added: u32,
    removed: u32,
    hunks: Vec<FileHunk>,
}

struct FileHunk {
    header: String,
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
    lines: Vec<HunkLine>,
}

struct HunkLine {
    kind: &'static str,
    text: String,
}

#[derive(Default)]
struct FileProofSignal {
    trail_step_count: usize,
    error_step_count: usize,
    retry_count: usize,
    checks_touching: Vec<String>,
    /// Selected proof ran green after the edit, so generic check events (which
    /// rarely name every changed path) still count as proof for the file.
    selected_proof_passed_after_edit: bool,
}

impl FileProofSignal {
    fn agent_confidence(&self) -> &'static str {
        // A green selected proof after the change outranks intermediate trail
        // noise: generic proof events often omit the path, and earlier
        // file-local failures should not stick as needs_attention once the
        // selected check has passed for the edit.
        if self.selected_proof_passed_after_edit && !self.checks_touching.is_empty() {
            "checked"
        } else if self.error_step_count > 0 {
            "needs_attention"
        } else if !self.checks_touching.is_empty() {
            "checked"
        } else if self.trail_step_count > 0 {
            "mentioned"
        } else {
            "unscored"
        }
    }
}

struct RunTrailEvent {
    event: String,
    detail: String,
    /// CSV of selected checks from the run_started payload when present.
    selected_checks: String,
}

/// Selected-check evidence for a run: which checks were chosen, which of those
/// passed, and whether a pass landed after an edit. Used to attach proof to
/// changed files when check events are path-generic.
#[derive(Default)]
struct SelectedProofEvidence {
    selected_checks: Vec<String>,
    passed_selected_checks: Vec<String>,
    proof_passed_after_edit: bool,
}

fn file_proof_signals(
    kernel: &Arc<RwLock<MdxKernel>>,
    run_id: &str,
    files: &[&FileDiff],
) -> Result<Vec<FileProofSignal>, String> {
    let events = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        run_trail_events(&kernel, run_id)
    };
    let selected_proof = selected_proof_evidence(&events);
    Ok(files
        .iter()
        .map(|file| {
            let mut signal = proof_signal_for_file(&file.path, &events);
            attach_selected_proof_to_changed_file(&mut signal, &selected_proof);
            signal
        })
        .collect())
}

fn run_trail_events(kernel: &MdxKernel, run_id: &str) -> Vec<RunTrailEvent> {
    kernel
        .ledger()
        .query()
        .by_kind("forge.run.event")
        .iter()
        .filter(|receipt| receipt.payload.get("run_id").map(String::as_str) == Some(run_id))
        .map(|receipt| RunTrailEvent {
            event: receipt.payload.get("event").cloned().unwrap_or_default(),
            detail: receipt.payload.get("detail").cloned().unwrap_or_default(),
            selected_checks: receipt
                .payload
                .get("selected_checks")
                .cloned()
                .unwrap_or_default(),
        })
        .collect()
}

fn selected_proof_evidence(events: &[RunTrailEvent]) -> SelectedProofEvidence {
    let mut evidence = SelectedProofEvidence::default();
    let mut saw_edit = false;
    for event in events {
        let event_lower = event.event.to_ascii_lowercase();
        let detail_lower = event.detail.to_ascii_lowercase();
        if !event.selected_checks.trim().is_empty() {
            evidence.selected_checks = csv_values(&event.selected_checks);
        }
        if event_looks_like_edit(&event_lower, &detail_lower) {
            saw_edit = true;
        }
        if let Some(command) = check_command_if_passed(&event_lower, &event.detail, &detail_lower) {
            let is_selected = evidence.selected_checks.is_empty()
                || evidence
                    .selected_checks
                    .iter()
                    .any(|selected| check_commands_match(selected, &command));
            if is_selected {
                if !evidence
                    .passed_selected_checks
                    .iter()
                    .any(|seen| seen == &command)
                {
                    evidence.passed_selected_checks.push(command.clone());
                }
                if saw_edit {
                    evidence.proof_passed_after_edit = true;
                }
            }
        }
    }
    // When the run recorded selected checks and a matching pass, prefer the
    // selected labels for attachment so the UI shows the real proof command.
    if evidence.proof_passed_after_edit
        && !evidence.selected_checks.is_empty()
        && evidence.passed_selected_checks.is_empty()
    {
        evidence.passed_selected_checks = evidence.selected_checks.clone();
    }
    if evidence.proof_passed_after_edit && evidence.passed_selected_checks.is_empty() {
        evidence
            .passed_selected_checks
            .push("selected_proof".to_string());
    }
    evidence
}

fn attach_selected_proof_to_changed_file(
    signal: &mut FileProofSignal,
    selected_proof: &SelectedProofEvidence,
) {
    if !selected_proof.proof_passed_after_edit {
        return;
    }
    signal.selected_proof_passed_after_edit = true;
    let mut checks: BTreeSet<String> = signal.checks_touching.iter().cloned().collect();
    for check in &selected_proof.passed_selected_checks {
        checks.insert(check.clone());
    }
    for check in &selected_proof.selected_checks {
        if selected_proof
            .passed_selected_checks
            .iter()
            .any(|passed| check_commands_match(check, passed))
        {
            checks.insert(check.clone());
        }
    }
    signal.checks_touching = checks.into_iter().take(12).collect();
}

fn csv_values(raw: &str) -> Vec<String> {
    raw.split([',', '|', '\n'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| part.to_string())
        .collect()
}

fn event_looks_like_edit(event_lower: &str, detail_lower: &str) -> bool {
    event_lower.contains("edit")
        || event_lower.contains("write")
        || event_lower.contains("patch")
        || event_lower.contains("apply_patch")
        || detail_lower.contains("edit_file")
        || detail_lower.contains("write_file")
        || detail_lower.contains("apply_patch")
        || detail_lower.contains("files changed")
        || detail_lower.contains("wrote ")
        || detail_lower.contains("patched ")
}

fn check_command_if_passed(event_lower: &str, detail: &str, detail_lower: &str) -> Option<String> {
    let looks_like_check = event_lower.contains("check")
        || event_lower.contains("proof")
        || event_lower.contains("test")
        || detail_lower.contains("check")
        || detail_lower.contains("cargo test")
        || detail_lower.contains("npm test")
        || detail_lower.contains("pytest")
        || detail_lower.contains("go test");
    if !looks_like_check {
        return None;
    }
    let failed = event_lower.contains("fail")
        || detail_lower.contains("failed")
        || detail_lower.contains("failure")
        || detail_lower.contains("exit=1")
        || detail_lower.contains("exit_code=1")
        || detail_lower.contains("refused");
    if failed {
        return None;
    }
    let passed = event_lower.contains("pass")
        || event_lower.ends_with("_ok")
        || detail_lower.contains("passed")
        || detail_lower.contains("exit=0")
        || detail_lower.contains("exit_code=0")
        || detail_lower.contains("ok ");
    if !passed && !event_lower.contains("check_passed") {
        // Accept explicit check_passed event names even without "passed" in detail.
        if event_lower != "check_passed" && !event_lower.ends_with(".passed") {
            return None;
        }
    }
    let command = detail
        .split("command=")
        .nth(1)
        .map(|rest| rest.split(" exit").next().unwrap_or(rest).trim())
        .filter(|command| !command.is_empty())
        .map(|command| command.to_string())
        .unwrap_or_else(|| {
            let trimmed = detail.trim();
            if trimmed.is_empty() {
                event_lower.to_string()
            } else {
                trimmed.to_string()
            }
        });
    Some(command)
}

fn check_commands_match(selected: &str, observed: &str) -> bool {
    let selected = selected.trim().to_ascii_lowercase();
    let observed = observed.trim().to_ascii_lowercase();
    if selected.is_empty() || observed.is_empty() {
        return false;
    }
    selected == observed || observed.contains(&selected) || selected.contains(&observed)
}

fn proof_signal_for_file(path: &str, events: &[RunTrailEvent]) -> FileProofSignal {
    let mut signal = FileProofSignal::default();
    let path_lower = path.to_ascii_lowercase();
    let basename = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    let mut checks = BTreeSet::new();
    for event in events {
        let event_lower = event.event.to_ascii_lowercase();
        let detail_lower = event.detail.to_ascii_lowercase();
        let text = format!("{event_lower} {detail_lower}");
        let touches_file = text.contains(&path_lower)
            || (!basename.is_empty() && basename != path_lower && text.contains(&basename));
        if !touches_file {
            continue;
        }
        signal.trail_step_count += 1;
        if text.contains("failed")
            || text.contains("failure")
            || text.contains("error")
            || text.contains("refused")
            || text.contains("panic")
            || text.contains("timed out")
        {
            signal.error_step_count += 1;
        }
        if text.contains("retry") || text.contains("repair") || text.contains("revise") {
            signal.retry_count += 1;
        }
        if event_lower.contains("check") || detail_lower.contains("check") || text.contains("test")
        {
            let label = if event.event.trim().is_empty() {
                "run_check".to_string()
            } else {
                event.event.clone()
            };
            checks.insert(label);
        }
    }
    signal.checks_touching = checks.into_iter().take(12).collect();
    signal
}

// Split a unified diff into per-file sections, counting +/- lines. The
// core change reads first; the raw patch rides along for disclosure.
fn split_into_files(diff: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut current: Option<FileDiff> = None;
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            if let Some(file) = current.take() {
                files.push(file);
            }
            let path = line.split(" b/").nth(1).unwrap_or(line).trim().to_string();
            current = Some(FileDiff {
                path,
                patch: String::new(),
                added: 0,
                removed: 0,
                hunks: Vec::new(),
            });
        }
        if let Some(file) = current.as_mut() {
            if line.starts_with("@@") {
                file.hunks.push(parse_hunk_header(line));
            } else if line.starts_with('+') && !line.starts_with("+++") {
                file.added += 1;
                push_hunk_line(file, "add", line);
            } else if line.starts_with('-') && !line.starts_with("---") {
                file.removed += 1;
                push_hunk_line(file, "remove", line);
            } else if line.starts_with(' ') {
                push_hunk_line(file, "context", line);
            } else if line.starts_with('\\') {
                push_hunk_line(file, "meta", line);
            }
            file.patch.push_str(line);
            file.patch.push('\n');
        }
    }
    if let Some(file) = current.take() {
        files.push(file);
    }
    files
}

fn push_hunk_line(file: &mut FileDiff, kind: &'static str, line: &str) {
    let Some(hunk) = file.hunks.last_mut() else {
        return;
    };
    let text = if matches!(kind, "add" | "remove" | "context") {
        line.get(1..).unwrap_or("").to_string()
    } else {
        line.to_string()
    };
    hunk.lines.push(HunkLine { kind, text });
}

fn parse_hunk_header(line: &str) -> FileHunk {
    let mut old_start = 0;
    let mut old_lines = 0;
    let mut new_start = 0;
    let mut new_lines = 0;
    let mut parts = line.split_whitespace();
    let _at = parts.next();
    if let Some(old) = parts.next().and_then(|part| part.strip_prefix('-')) {
        let (start, lines) = parse_hunk_range(old);
        old_start = start;
        old_lines = lines;
    }
    if let Some(new) = parts.next().and_then(|part| part.strip_prefix('+')) {
        let (start, lines) = parse_hunk_range(new);
        new_start = start;
        new_lines = lines;
    }
    FileHunk {
        header: line.to_string(),
        old_start,
        old_lines,
        new_start,
        new_lines,
        lines: Vec::new(),
    }
}

fn parse_hunk_range(part: &str) -> (u32, u32) {
    let mut pieces = part.split(',');
    let start = pieces.next().unwrap_or("0").parse::<u32>().unwrap_or(0);
    let lines = pieces.next().unwrap_or("1").parse::<u32>().unwrap_or(1);
    (start, lines)
}

fn hunks_json(hunks: &[FileHunk]) -> String {
    hunks
        .iter()
        .map(|hunk| {
            format!(
                r#"{{"header":{},"old_start":{},"old_lines":{},"new_start":{},"new_lines":{},"lines":[{}]}}"#,
                json_string_literal(&hunk.header),
                hunk.old_start,
                hunk.old_lines,
                hunk.new_start,
                hunk.new_lines,
                hunk_lines_json(&hunk.lines),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn hunk_lines_json(lines: &[HunkLine]) -> String {
    lines
        .iter()
        .map(|line| {
            format!(
                r#"{{"kind":{},"text":{}}}"#,
                json_string_literal(line.kind),
                json_string_literal(&line.text),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn json_array(values: &[String]) -> String {
    values
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",")
}

fn git_diff_against_base(
    repo_root: &std::path::Path,
    branch: &str,
    base_branch: &str,
) -> Option<String> {
    // The work branch must be a Forge/fleet branch; the base is ordinary
    // trunk (main by default), so it gets the looser safe-ref check, not the
    // forge/ prefix requirement.
    if !is_reviewable_forge_branch(branch) || !is_safe_base_branch(base_branch) {
        return None;
    }
    let merge_base = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["merge-base", base_branch, branch])
        .output()
        .ok()?;
    if !merge_base.status.success() {
        return None;
    }
    let base = String::from_utf8_lossy(&merge_base.stdout)
        .trim()
        .to_string();
    if base.is_empty() {
        return None;
    }
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["diff", "--no-color", &format!("{base}..{branch}")])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

fn git_show(repo_root: &std::path::Path, branch: &str) -> Option<String> {
    // Validate the ref shape before handing it to git: a Forge worker branch
    // or a governed fleet integration branch only.
    if !is_reviewable_forge_branch(branch) {
        return None;
    }
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args([
            "show",
            "--no-color",
            "--format=",
            &format!("{branch}^..{branch}"),
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        // A single-commit branch has no `^..`; fall back to the commit itself.
        let fallback = std::process::Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["show", "--no-color", "--format=", branch])
            .output()
            .ok()?;
        if !fallback.status.success() {
            return None;
        }
        return Some(String::from_utf8_lossy(&fallback.stdout).to_string());
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

fn is_reviewable_forge_branch(branch: &str) -> bool {
    (branch.starts_with("forge/") || branch.starts_with("fleet/"))
        && !branch.contains([';', '|', '&', ' ', '`'])
}

/// A base branch is ordinary trunk (main, develop, a release line), not a
/// Forge worker branch, so it does NOT require the forge/ prefix. It still has
/// to be a safe git ref before it reaches the shell: nonempty, no whitespace,
/// no shell metacharacters, no leading dash (so it cannot pose as a flag), no
/// `..` traversal, and only the ordinary ref alphabet.
fn is_safe_base_branch(branch: &str) -> bool {
    !branch.is_empty()
        && !branch.starts_with('-')
        && !branch.contains("..")
        && !branch.chars().any(char::is_whitespace)
        && branch
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-'))
}

fn repo_root() -> std::path::PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    cwd.ancestors()
        .find(|path| path.join(".git").exists())
        .map(|path| path.to_path_buf())
        .unwrap_or(cwd)
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-run-diff","status":"REFUSED","reason":{},"production_write_allowed":false}}"#,
            json_string_literal(reason)
        ),
    )
}

fn json_string_field(body: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\":");
    let after = body.split(&marker).nth(1)?;
    let after = after.trim_start();
    let rest = after.strip_prefix('"')?;
    let mut value = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(escaped) = chars.next() {
                    value.push(escaped);
                }
            }
            '"' => return Some(value),
            other => value.push(other),
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{ForgeRunEvent, GovernedWriteIdentity, MdxKernel};

    #[test]
    fn split_diff_marks_build_artifact_paths_as_filterable() {
        let diff = "\
diff --git a/.build/workspace-state.json b/.build/workspace-state.json
index 111..222 100644
--- a/.build/workspace-state.json
+++ b/.build/workspace-state.json
@@ -1 +1 @@
-old
+new
diff --git a/Sources/App/Slug.swift b/Sources/App/Slug.swift
index 111..222 100644
--- a/Sources/App/Slug.swift
+++ b/Sources/App/Slug.swift
@@ -1 +1 @@
-old
+new
";
        let files = split_into_files(diff);
        let artifact_count = files
            .iter()
            .filter(|file| crate::forge_repo_profile::is_artifact_path(&file.path))
            .count();
        let review_count = files.len() - artifact_count;

        assert_eq!(artifact_count, 1);
        assert_eq!(review_count, 1);
    }

    #[test]
    fn split_diff_returns_structured_hunks_for_diff_view() {
        let diff = "\
diff --git a/src/lib.rs b/src/lib.rs
index 111..222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -2,3 +2,4 @@ pub fn answer() -> i32 {
 let before = 40;
-before + 1
+before + 2
+// proven by Forge
 }
";
        let files = split_into_files(diff);

        assert_eq!(files.len(), 1);
        let file = &files[0];
        assert_eq!(file.path, "src/lib.rs");
        assert_eq!(file.added, 2);
        assert_eq!(file.removed, 1);
        assert_eq!(file.hunks.len(), 1);
        assert_eq!(file.hunks[0].old_start, 2);
        assert_eq!(file.hunks[0].old_lines, 3);
        assert_eq!(file.hunks[0].new_start, 2);
        assert_eq!(file.hunks[0].new_lines, 4);
        assert_eq!(file.hunks[0].lines[0].kind, "context");
        assert_eq!(file.hunks[0].lines[1].kind, "remove");
        assert_eq!(file.hunks[0].lines[2].kind, "add");
    }

    #[test]
    fn proof_signal_for_file_reports_checks_and_retry_pressure() {
        let events = vec![
            RunTrailEvent {
                event: "related_test_check".to_string(),
                detail: "cargo test touched Sources/App/Slug.swift".to_string(),
                selected_checks: String::new(),
            },
            RunTrailEvent {
                event: "repair_retry".to_string(),
                detail: "retry after failure in Slug.swift".to_string(),
                selected_checks: String::new(),
            },
            RunTrailEvent {
                event: "unrelated_check".to_string(),
                detail: "cargo test touched Sources/App/Other.swift".to_string(),
                selected_checks: String::new(),
            },
        ];

        let signal = proof_signal_for_file("Sources/App/Slug.swift", &events);

        assert_eq!(signal.trail_step_count, 2);
        assert_eq!(signal.error_step_count, 1);
        assert_eq!(signal.retry_count, 1);
        assert_eq!(signal.checks_touching, vec!["related_test_check"]);
        assert_eq!(signal.agent_confidence(), "needs_attention");
    }

    #[test]
    fn selected_proof_after_edit_marks_changed_file_checked_even_when_events_are_generic() {
        // Proof events often name the command but not every changed path. When
        // the selected proof passes after an edit, the changed file should still
        // score as checked instead of needs_attention from earlier local noise.
        let events = vec![
            RunTrailEvent {
                event: "run_started".to_string(),
                detail: "branch=forge/run-demo".to_string(),
                selected_checks: "cargo test -p mdx-server forge_diff_route".to_string(),
            },
            RunTrailEvent {
                event: "tool_executed".to_string(),
                detail: "edit_file path=crates/mdx-server/src/forge_diff_route.rs".to_string(),
                selected_checks: String::new(),
            },
            RunTrailEvent {
                event: "repair_retry".to_string(),
                detail: "retry after failure in forge_diff_route.rs".to_string(),
                selected_checks: String::new(),
            },
            RunTrailEvent {
                event: "check_passed".to_string(),
                detail: "command=cargo test -p mdx-server forge_diff_route exit=0".to_string(),
                selected_checks: String::new(),
            },
        ];

        let selected = selected_proof_evidence(&events);
        assert!(selected.proof_passed_after_edit);
        assert!(
            selected
                .passed_selected_checks
                .iter()
                .any(|check| check.contains("forge_diff_route"))
        );

        let mut signal =
            proof_signal_for_file("crates/mdx-server/src/forge_diff_route.rs", &events);
        assert_eq!(signal.agent_confidence(), "needs_attention");
        attach_selected_proof_to_changed_file(&mut signal, &selected);
        assert!(signal.selected_proof_passed_after_edit);
        assert!(
            signal
                .checks_touching
                .iter()
                .any(|check| check.contains("forge_diff_route") || check.contains("cargo test"))
        );
        assert_eq!(signal.agent_confidence(), "checked");
    }

    #[test]
    fn selected_proof_does_not_upgrade_when_proof_never_passed_after_edit() {
        let events = vec![
            RunTrailEvent {
                event: "run_started".to_string(),
                detail: "branch=forge/run-demo".to_string(),
                selected_checks: "cargo test -p mdx-server forge_diff_route".to_string(),
            },
            RunTrailEvent {
                event: "tool_executed".to_string(),
                detail: "edit_file path=crates/mdx-server/src/forge_diff_route.rs".to_string(),
                selected_checks: String::new(),
            },
            RunTrailEvent {
                event: "check_failed".to_string(),
                detail: "command=cargo test -p mdx-server forge_diff_route exit=1".to_string(),
                selected_checks: String::new(),
            },
        ];

        let selected = selected_proof_evidence(&events);
        assert!(!selected.proof_passed_after_edit);

        let mut signal =
            proof_signal_for_file("crates/mdx-server/src/forge_diff_route.rs", &events);
        attach_selected_proof_to_changed_file(&mut signal, &selected);
        assert!(!signal.selected_proof_passed_after_edit);
        assert_ne!(signal.agent_confidence(), "checked");
    }

    #[test]
    fn repo_root_for_run_prefers_evidence_payload_before_legacy_detail() {
        let mut kernel = MdxKernel::boot_local();
        kernel
            .record_forge_run_event_with_evidence_fields(
                ForgeRunEvent {
                    tenant_id: "t",
                    actor_id: "a",
                    run_id: "forge_run_external",
                    event: "run_started",
                    work_item_id: "w",
                    detail: "accepted repo_root=/legacy/detail",
                    turn: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                },
                &GovernedWriteIdentity::local_demo("human:dev"),
                &[("repo_root", "/external/repo")],
            )
            .expect("record run");

        assert_eq!(
            repo_root_for_run(&kernel, "forge_run_external"),
            Some("/external/repo".to_string())
        );
    }

    #[test]
    fn fleet_integration_branches_are_reviewable() {
        assert!(is_reviewable_forge_branch("forge/run-forge_run_123"));
        assert!(is_reviewable_forge_branch("fleet/node_wide_8_ledger_007"));
        assert!(!is_reviewable_forge_branch("feature/user-branch"));
        assert!(!is_reviewable_forge_branch("fleet/bad;rm -rf"));
    }

    #[test]
    fn ordinary_trunk_names_are_safe_bases_without_the_forge_prefix() {
        // The base branch is trunk, not a Forge worker branch, so it must not
        // require the forge/ prefix - the bug that denied every normal delivery.
        assert!(is_safe_base_branch("main"));
        assert!(is_safe_base_branch("develop"));
        assert!(is_safe_base_branch("release/1.2.0"));
        assert!(is_safe_base_branch("forge/base"));
        // But it still has to be a safe git ref before it reaches the shell.
        assert!(!is_safe_base_branch(""));
        assert!(!is_safe_base_branch("--upload-pack=evil"));
        assert!(!is_safe_base_branch("main..evil"));
        assert!(!is_safe_base_branch("main branch"));
        assert!(!is_safe_base_branch("main;rm -rf"));
        assert!(!is_safe_base_branch("main$(whoami)"));
    }
}
