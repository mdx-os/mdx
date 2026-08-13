// Deterministic task scout for a connected external repo. This gives Forge a
// concrete first-run menu without provider calls: repo-specific TODO/FIXME
// cleanup, untested public symbols, deprecated APIs, missing error handling,
// and contained refactor targets. It records paths, symbols, and counts only,
// never raw source snippets or secret values.
use crate::RouteResponse;
use mdx_core::{MdxKernel, json_string_literal};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

const MAX_FILES_SCANNED: usize = 250;
const MAX_CANDIDATES: usize = 3;

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/repo-task-scout.json" {
        return None;
    }
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Some(Ok(response));
    }
    let repo_id = json_string_field(body, "repo_id").unwrap_or_default();
    if repo_id.trim().is_empty() {
        return Some(Ok(refusal("name the connected repo to scout")));
    }
    Some(handle(&repo_id, kernel))
}

fn handle(repo_id: &str, kernel: &Arc<RwLock<MdxKernel>>) -> Result<RouteResponse, String> {
    let repo_id = repo_id.trim();
    let kernel_read = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    let mut latest: Option<(&mdx_core::Receipt, BTreeMap<String, String>)> = None;
    for receipt in kernel_read.ledger().query().by_kind("forge.repo.connected") {
        if receipt.payload.get("repo_id").map(String::as_str) == Some(repo_id) {
            latest = Some((receipt, receipt.payload.clone()));
        }
    }
    let Some((repo_receipt, fields)) = latest else {
        return Ok(refusal("no connected repo by that id"));
    };
    let root = fields.get("root").map(String::as_str).unwrap_or("");
    if root.trim().is_empty() || !Path::new(root).is_dir() {
        return Ok(refusal(
            "connected repo root is not available on this machine",
        ));
    }

    let indexed_profile = kernel_read.forge_repo_index_profile_json(repo_id);
    let (profile_json, _) = crate::forge_repo_profile::profile_json_with_current_setup(
        std::path::Path::new(root),
        indexed_profile,
    );
    let profile: serde_json::Value = serde_json::from_str(&profile_json)
        .unwrap_or_else(|_| serde_json::json!({"language_pack_id":"generic"}));
    let suggested_checks = string_array(&profile["suggested_checks"]);
    let artifact_patterns = string_array(&profile["artifact_patterns"]);
    let scan = scout_repo(Path::new(root), &suggested_checks, &artifact_patterns);
    let status = if scan.candidates.is_empty() {
        "NO_DETERMINISTIC_TASKS_FOUND"
    } else {
        "TASKS_FOUND"
    };
    let safe_next_move = if scan.candidates.is_empty() {
        "Use the onboarding packet templates with an explicit operator goal and check."
    } else {
        "Pick one candidate, keep the suggested check, and review the generated PR handoff before source-host delivery."
    };

    Ok(RouteResponse::json(
        "200 OK",
        serde_json::json!({
            "name": "mdx-forge-repo-task-scout",
            "status": status,
            "repo_id": repo_id,
            "label": fields.get("label").map(String::as_str).unwrap_or(""),
            "repo_receipt_id": repo_receipt.receipt_id,
            "language_pack_id": profile["language_pack_id"].clone(),
            "primary_language": profile["primary_language"].clone(),
            "safe_next_move": safe_next_move,
            "files_scanned": scan.files_scanned,
            "source_files_seen": scan.source_files_seen,
            "test_files_seen": scan.test_files_seen,
            "candidate_count": scan.candidates.len(),
            "candidates": scan.candidates,
            "suggested_checks": suggested_checks,
            "selected_checks_source": if string_array(&profile["suggested_checks"]).is_empty() { "operator_required" } else { "repo_profile_inferred" },
            "scout_limits": {
                "max_files_scanned": MAX_FILES_SCANNED,
                "max_candidates": MAX_CANDIDATES,
                "artifact_filtering_applied": true,
            },
            "repo_onboarding_packet_route": "/forge/repo-onboarding-packet.json",
            "repo_readiness_route": "/forge/repo-readiness.json",
            "run_route": "/forge/runs.json",
            "review_packet_route": "/forge/review-packet.json",
            "raw_source_content_recorded": false,
            "secret_values_recorded": false,
            "provider_calls_allowed": false,
            "adapter_execution_allowed": false,
            "run_started": false,
            "network_call_allowed": false,
            "production_write_allowed": false,
        })
        .to_string(),
    ))
}

pub(crate) struct ScoutResult {
    pub(crate) files_scanned: usize,
    pub(crate) source_files_seen: usize,
    pub(crate) test_files_seen: usize,
    pub(crate) candidates: Vec<serde_json::Value>,
}

pub(crate) fn scout_repo(
    root: &Path,
    suggested_checks: &[String],
    artifact_patterns: &[String],
) -> ScoutResult {
    let mut files = Vec::new();
    collect_files(root, root, &mut files);
    files.sort();
    files.truncate(MAX_FILES_SCANNED);

    let mut drafts = Vec::new();
    let mut test_text_index = String::new();
    let mut source_files_seen = 0usize;
    let mut test_files_seen = 0usize;
    let checks = if suggested_checks.is_empty() {
        vec!["operator-selected check".to_string()]
    } else {
        suggested_checks.to_vec()
    };

    for relative in &files {
        let relative_str = relative.to_string_lossy().replace('\\', "/");
        if is_source_file(&relative_str) {
            source_files_seen += 1;
        }
        if is_test_file(&relative_str) {
            test_files_seen += 1;
            if is_text_file(&relative_str) {
                let absolute = root.join(relative);
                if let Ok(text) = std::fs::read_to_string(&absolute) {
                    test_text_index.push_str(&text.to_ascii_lowercase());
                    test_text_index.push('\n');
                }
            }
        }
    }

    for relative in &files {
        let relative_str = relative.to_string_lossy().replace('\\', "/");
        if !is_text_file(&relative_str) || is_test_file(&relative_str) {
            continue;
        }
        let absolute = root.join(relative);
        let Ok(text) = std::fs::read_to_string(&absolute) else {
            continue;
        };
        collect_todo_candidates(&mut drafts, &relative_str, &text);
        collect_untested_symbol_candidates(&mut drafts, &relative_str, &text, &test_text_index);
        collect_error_boundary_candidates(&mut drafts, &relative_str, &text);
        collect_deprecated_api_candidates(&mut drafts, &relative_str, &text);
        collect_refactor_candidates(&mut drafts, &relative_str, &text, test_files_seen);
    }

    drafts.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line.cmp(&b.line))
            .then_with(|| a.kind.cmp(b.kind))
    });
    drafts.dedup_by(|a, b| a.kind == b.kind && a.path == b.path && a.line == b.line);
    let candidates = drafts
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|draft| candidate(draft, &checks, artifact_patterns))
        .collect::<Vec<_>>();

    ScoutResult {
        files_scanned: files.len(),
        source_files_seen,
        test_files_seen,
        candidates,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CandidateDraft {
    score: u32,
    kind: &'static str,
    title: String,
    path: String,
    line: usize,
    prompt_template: String,
    complexity_tier: &'static str,
    task_class: &'static str,
    why_this_first: String,
    review_focus: Vec<String>,
}

fn collect_files(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) {
    if files.len() >= MAX_FILES_SCANNED {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if files.len() >= MAX_FILES_SCANNED {
            return;
        }
        let path = entry.path();
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let relative_str = relative.to_string_lossy().replace('\\', "/");
        if should_skip_path(&relative_str) {
            continue;
        }
        if path.is_dir() {
            collect_files(root, &path, files);
        } else if path.is_file() {
            files.push(relative.to_path_buf());
        }
    }
}

fn collect_todo_candidates(drafts: &mut Vec<CandidateDraft>, path: &str, text: &str) {
    for (index, line) in text.lines().take(500).enumerate() {
        let upper = line.to_ascii_uppercase();
        if upper.contains("TODO") || upper.contains("FIXME") {
            drafts.push(CandidateDraft {
                score: 72,
                kind: "todo_cleanup",
                title: format!("Resolve the TODO in {}", display_path(path)),
                path: path.to_string(),
                line: index + 1,
                prompt_template: format!("Resolve the TODO or FIXME at {path}:{}, preserve behavior unless the note clearly asks for a behavior change, and add or update focused proof.", index + 1),
                complexity_tier: "small",
                task_class: "bug_fix",
                why_this_first: "There is a specific maintenance note in code, so Forge can make a focused change with a small blast radius.".to_string(),
                review_focus: vec![
                    "the TODO/FIXME intent is actually resolved".to_string(),
                    "adjacent behavior has focused proof".to_string(),
                ],
            });
            return;
        }
    }
}

fn collect_untested_symbol_candidates(
    drafts: &mut Vec<CandidateDraft>,
    path: &str,
    text: &str,
    test_text_index: &str,
) {
    if !is_source_file(path) {
        return;
    }
    for (index, line) in text.lines().take(500).enumerate() {
        let Some(symbol) = public_symbol_name(path, line) else {
            continue;
        };
        if symbol.len() < 3 || test_text_index.contains(&symbol.to_ascii_lowercase()) {
            continue;
        }
        drafts.push(CandidateDraft {
            score: 90,
            kind: "untested_public_symbol",
            title: format!("Add a test for {symbol}() - it has no nearby coverage"),
            path: path.to_string(),
            line: index + 1,
            prompt_template: format!("Add focused coverage for {symbol} in {path}, then keep any production change minimal and behavior-preserving."),
            complexity_tier: "medium",
            task_class: "bug_fix",
            why_this_first: "A public symbol is visible to callers, but the current tests do not reference it by name.".to_string(),
            review_focus: vec![
                "new tests exercise the public behavior".to_string(),
                "production changes stay minimal unless the test exposes a real issue".to_string(),
            ],
        });
        return;
    }
}

fn collect_error_boundary_candidates(drafts: &mut Vec<CandidateDraft>, path: &str, text: &str) {
    if !is_source_file(path) {
        return;
    }
    let lines = text.lines().take(500).collect::<Vec<_>>();
    for (index, line) in lines.iter().enumerate() {
        let lower = line.to_ascii_lowercase();
        let Some(operation) = risky_operation_name(path, &lower) else {
            continue;
        };
        let nearby_try = lines
            .iter()
            .skip(index.saturating_sub(3))
            .take(5)
            .any(|nearby| {
                let nearby = nearby.to_ascii_lowercase();
                nearby.contains("try")
                    || nearby.contains("catch")
                    || nearby.contains("except")
                    || nearby.contains("map_err")
                    || nearby.contains("guard")
            });
        if nearby_try {
            continue;
        }
        drafts.push(CandidateDraft {
            score: 82,
            kind: "error_boundary",
            title: format!("Add explicit failure handling around {operation}"),
            path: path.to_string(),
            line: index + 1,
            prompt_template: format!("Inspect {path}:{}, add explicit failure handling around {operation}, and prove both success and failure behavior where the repo's tests make that practical.", index + 1),
            complexity_tier: "medium",
            task_class: "bug_fix",
            why_this_first: "This code touches IO, parsing, or external boundaries without an obvious nearby error path.".to_string(),
            review_focus: vec![
                "failure behavior is explicit".to_string(),
                "the success path remains unchanged".to_string(),
            ],
        });
        return;
    }
}

fn collect_deprecated_api_candidates(drafts: &mut Vec<CandidateDraft>, path: &str, text: &str) {
    if !is_source_file(path) {
        return;
    }
    for (index, line) in text.lines().take(500).enumerate() {
        let Some(api) = deprecated_api_name(path, line) else {
            continue;
        };
        drafts.push(CandidateDraft {
            score: 78,
            kind: "deprecated_api",
            title: format!("Replace deprecated {api} usage"),
            path: path.to_string(),
            line: index + 1,
            prompt_template: format!("Replace the deprecated {api} usage at {path}:{} with the current idiom, preserving behavior and adding focused proof if the repo has tests.", index + 1),
            complexity_tier: "small",
            task_class: "refactor",
            why_this_first: "The code uses an API with a known modern replacement, so the change is concrete and reviewable.".to_string(),
            review_focus: vec![
                "replacement preserves existing behavior".to_string(),
                "the change avoids broad rewrites".to_string(),
            ],
        });
        return;
    }
}

fn collect_refactor_candidates(
    drafts: &mut Vec<CandidateDraft>,
    path: &str,
    text: &str,
    test_files_seen: usize,
) {
    if !is_source_file(path) || test_files_seen == 0 {
        return;
    }
    let line_count = text.lines().count();
    if line_count < 400 {
        return;
    }
    drafts.push(CandidateDraft {
        score: 58,
        kind: "contained_refactor",
        title: format!("Split the large module in {}", display_path(path)),
        path: path.to_string(),
        line: 1,
        prompt_template: format!("Refactor {path} in a contained way: preserve public behavior, split one obvious helper boundary, and keep the existing tests green."),
        complexity_tier: "medium",
        task_class: "refactor",
        why_this_first: "This file is large enough to slow review, and the repo has tests to keep a contained refactor honest.".to_string(),
        review_focus: vec![
            "public behavior stays stable".to_string(),
            "the extracted boundary is easier to review".to_string(),
        ],
    });
}

fn candidate(
    draft: CandidateDraft,
    suggested_checks: &[String],
    artifact_patterns: &[String],
) -> serde_json::Value {
    let semantic_orientation_queries =
        semantic_orientation_queries(draft.kind, &draft.path, Some(draft.line));
    let write_scope_hint = vec![draft.path.clone()];
    let proof_command_source = if suggested_checks
        .iter()
        .any(|check| check == "operator-selected check")
    {
        "operator_required"
    } else {
        "repo_profile_inferred"
    };
    let related_tests_required = semantic_orientation_queries.iter().any(|query| {
        query["operation"]
            .as_str()
            .is_some_and(|operation| operation == "related_tests")
    });
    serde_json::json!({
        "task_id": format!("{}-{}-{}", draft.kind, draft.path.replace(['/', '.'], "_"), draft.line),
        "kind": draft.kind,
        "title": draft.title,
        "path": draft.path,
        "line": draft.line,
        "prompt_template": draft.prompt_template,
        "recommended_shape": "run",
        "task_class": draft.task_class,
        "complexity_tier": draft.complexity_tier,
        "suggested_checks": suggested_checks,
        "semantic_orientation_queries": semantic_orientation_queries,
        "proof_strategy": {
            "selected_checks": suggested_checks,
            "proof_command_source": proof_command_source,
            "related_tests_required_before_finish": related_tests_required,
            "minimum_evidence": [
                "successful semantic_query orientation",
                "selected check result recorded",
                "review packet PR handoff generated"
            ],
        },
        "write_scope_hint": write_scope_hint,
        "off_limits_patterns": artifact_patterns,
        "review_focus": draft.review_focus,
        "why_this_first": draft.why_this_first,
        "raw_source_content_recorded": false,
        "grants_execution_authority": false,
    })
}

fn semantic_orientation_queries(
    kind: &str,
    path: &str,
    line: Option<usize>,
) -> Vec<serde_json::Value> {
    let mut queries = vec![serde_json::json!({
        "operation": "capabilities",
        "path": "",
        "query": "",
        "why": "Confirm the detected language pack and semantic affordances before planning."
    })];
    if !path.trim().is_empty() && is_source_file(path) {
        if let Some(line) = line.filter(|line| *line > 0) {
            queries.push(serde_json::json!({
                "operation": "definition",
                "path": path,
                "line": line,
                "query": "",
                "why": "Use the language server when available to resolve the symbol under the concrete task line."
            }));
            queries.push(serde_json::json!({
                "operation": "references",
                "path": path,
                "line": line,
                "query": "",
                "why": "Use the language server when available to find call sites or adjacent dependents for the concrete task line."
            }));
        }
        queries.push(serde_json::json!({
            "operation": "file_outline",
            "path": path,
            "query": "",
            "why": "Anchor the edit in the local declarations before changing code."
        }));
        queries.push(serde_json::json!({
            "operation": "related_tests",
            "path": path,
            "query": "",
            "why": "Find adjacent tests before finishing a code-changing run."
        }));
    } else if kind == "test_gap" {
        queries.push(serde_json::json!({
            "operation": "related_tests",
            "path": "",
            "query": "tests",
            "why": "Look for existing test conventions before adding the first focused regression test."
        }));
    }
    queries
}

fn public_symbol_name(path: &str, line: &str) -> Option<String> {
    let trimmed = line.trim();
    let ext = Path::new(path).extension().and_then(|value| value.to_str());
    match ext {
        Some("rs") => symbol_after(trimmed, "pub fn "),
        Some("swift") => {
            symbol_after(trimmed, "public func ").or_else(|| symbol_after(trimmed, "func "))
        }
        Some("go") => {
            let name = symbol_after(trimmed, "func ")?;
            name.chars()
                .next()
                .is_some_and(char::is_uppercase)
                .then_some(name)
        }
        Some("py") => {
            if line.starts_with(' ') || line.starts_with('\t') {
                None
            } else {
                let name = symbol_after(trimmed, "def ")?;
                (!name.starts_with('_')).then_some(name)
            }
        }
        Some("js") | Some("jsx") | Some("ts") | Some("tsx") => {
            symbol_after(trimmed, "export function ")
                .or_else(|| symbol_after(trimmed, "export async function "))
                .or_else(|| symbol_after(trimmed, "export const "))
                .or_else(|| symbol_after(trimmed, "module.exports."))
        }
        Some("kt") | Some("kts") | Some("java") | Some("cs") => {
            if trimmed.contains(" public ") || trimmed.starts_with("public ") {
                symbol_before_paren(trimmed)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn risky_operation_name(path: &str, lower_line: &str) -> Option<&'static str> {
    let ext = Path::new(path).extension().and_then(|value| value.to_str());
    match ext {
        Some("js") | Some("jsx") | Some("ts") | Some("tsx") => {
            if lower_line.contains("json.parse(") {
                Some("JSON.parse")
            } else if lower_line.contains("fetch(") {
                Some("fetch")
            } else if lower_line.contains("readfilesync(") || lower_line.contains("readfile(") {
                Some("file read")
            } else {
                None
            }
        }
        Some("py") => {
            if lower_line.contains("json.loads(") {
                Some("json.loads")
            } else if lower_line.contains("requests.") {
                Some("HTTP request")
            } else if lower_line.contains("open(") {
                Some("file open")
            } else {
                None
            }
        }
        Some("rs") => {
            if lower_line.contains(".unwrap()") || lower_line.contains(".expect(") {
                Some("unwrap/expect")
            } else {
                None
            }
        }
        Some("swift") => {
            if lower_line.contains("try!") || lower_line.contains("try?") {
                Some("Swift try shortcut")
            } else {
                None
            }
        }
        Some("kt") | Some("kts") | Some("java") | Some("cs") => {
            if lower_line.contains("!!") || lower_line.contains("parse(") {
                Some("boundary parse")
            } else {
                None
            }
        }
        _ => None,
    }
}

fn deprecated_api_name(path: &str, line: &str) -> Option<&'static str> {
    let ext = Path::new(path).extension().and_then(|value| value.to_str());
    match ext {
        Some("js") | Some("jsx") | Some("ts") | Some("tsx") => {
            if line.contains("new Buffer(") {
                Some("new Buffer")
            } else if line.contains("fs.exists(") {
                Some("fs.exists")
            } else {
                None
            }
        }
        Some("py") => {
            if line.contains("import imp") || line.contains("imp.") {
                Some("imp")
            } else {
                None
            }
        }
        Some("swift") => {
            if line.contains("NSURLConnection") {
                Some("NSURLConnection")
            } else if line.contains("stringByAddingPercentEscapes") {
                Some("stringByAddingPercentEscapes")
            } else {
                None
            }
        }
        Some("rs") => line.contains("try!(").then_some("try!"),
        _ => None,
    }
}

fn symbol_after(line: &str, marker: &str) -> Option<String> {
    let rest = line.strip_prefix(marker)?;
    let name = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>();
    (!name.is_empty()).then_some(name)
}

fn symbol_before_paren(line: &str) -> Option<String> {
    let before = line.split_once('(')?.0.trim_end();
    let name = before
        .split_whitespace()
        .last()
        .unwrap_or("")
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
        .to_string();
    (!name.is_empty()).then_some(name)
}

fn display_path(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(path)
}

fn should_skip_path(relative: &str) -> bool {
    relative.starts_with(".git/")
        || relative.starts_with(".env")
        || relative.contains("/.env")
        || relative.contains("secret")
        || relative.contains("Secret")
        || relative.contains("node_modules/")
        || crate::forge_repo_profile::is_artifact_path(relative)
}

fn is_text_file(relative: &str) -> bool {
    matches!(
        Path::new(relative)
            .extension()
            .and_then(|value| value.to_str()),
        Some(
            "swift"
                | "kt"
                | "kts"
                | "java"
                | "cs"
                | "js"
                | "jsx"
                | "ts"
                | "tsx"
                | "rs"
                | "py"
                | "go"
                | "md"
                | "yml"
                | "yaml"
                | "xml"
                | "json"
                | "toml"
                | "gradle"
        )
    )
}

fn is_source_file(relative: &str) -> bool {
    !is_test_file(relative)
        && matches!(
            Path::new(relative)
                .extension()
                .and_then(|value| value.to_str()),
            Some(
                "swift"
                    | "kt"
                    | "kts"
                    | "java"
                    | "cs"
                    | "js"
                    | "jsx"
                    | "ts"
                    | "tsx"
                    | "rs"
                    | "py"
                    | "go"
            )
        )
}

fn is_test_file(relative: &str) -> bool {
    let lower = relative.to_ascii_lowercase();
    lower.contains("test") || lower.contains("spec")
}

fn string_array(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-repo-task-scout","status":"REFUSED","reason":{},"raw_source_content_recorded":false,"secret_values_recorded":false,"provider_calls_allowed":false,"adapter_execution_allowed":false,"run_started":false,"network_call_allowed":false,"production_write_allowed":false}}"#,
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
    use mdx_core::{ForgeRepoConnect, ForgeRepoIndex, GovernedWriteIdentity};

    #[test]
    fn task_scout_refuses_unknown_repo_without_authority() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        let response = handle("missing", &kernel).expect("response");
        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(
            response
                .body
                .contains(r#""raw_source_content_recorded":false"#)
        );
        assert!(response.body.contains(r#""run_started":false"#));
        assert!(
            response
                .body
                .contains(r#""production_write_allowed":false"#)
        );
    }

    #[test]
    fn task_scout_finds_todo_without_recording_raw_source() {
        let root = temp_dir("task_scout");
        std::fs::create_dir_all(root.join("Sources/App")).expect("mkdir");
        std::fs::write(root.join("Package.swift"), "// swift package\n").expect("package");
        std::fs::write(
            root.join("Sources/App/Thing.swift"),
            "struct Thing {\n  // TODO: make this better\n}\n",
        )
        .expect("source");

        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut kernel = kernel.write().expect("kernel");
            kernel
                .connect_forge_repo_with_identity(
                    ForgeRepoConnect {
                        tenant_id: "t",
                        actor_id: "human:dev",
                        repo_id: "todo_repo",
                        label: "Todo Repo",
                        root: root.to_str().unwrap_or(""),
                        kind: "local",
                        origin: "",
                    },
                    &GovernedWriteIdentity::local_demo("human:dev"),
                )
                .expect("connect");
        }
        let response = handle("todo_repo", &kernel).expect("response");
        assert!(
            response
                .body
                .contains(r#""name":"mdx-forge-repo-task-scout""#)
        );
        assert!(response.body.contains(r#""status":"TASKS_FOUND""#));
        assert!(response.body.contains(r#""kind":"todo_cleanup""#));
        assert!(
            response
                .body
                .contains(r#""path":"Sources/App/Thing.swift""#)
        );
        assert!(
            response
                .body
                .contains(r#""raw_source_content_recorded":false"#)
        );
        assert!(!response.body.contains("make this better"));
        assert!(response.body.contains(r#""provider_calls_allowed":false"#));
        let value: serde_json::Value = serde_json::from_str(&response.body).expect("json");
        let candidate = &value["candidates"][0];
        assert_eq!(candidate["kind"], "todo_cleanup");
        assert_eq!(
            candidate["semantic_orientation_queries"][1]["operation"],
            "definition"
        );
        assert_eq!(candidate["semantic_orientation_queries"][1]["line"], 2);
        assert_eq!(
            candidate["semantic_orientation_queries"][2]["operation"],
            "references"
        );
        assert_eq!(candidate["semantic_orientation_queries"][2]["line"], 2);
        assert_eq!(
            candidate["semantic_orientation_queries"][3]["operation"],
            "file_outline"
        );
        assert_eq!(
            candidate["semantic_orientation_queries"][4]["operation"],
            "related_tests"
        );
        assert_eq!(
            candidate["proof_strategy"]["related_tests_required_before_finish"],
            true
        );
        assert_eq!(candidate["write_scope_hint"][0], "Sources/App/Thing.swift");
        assert!(
            candidate["review_focus"]
                .as_array()
                .expect("review focus")
                .iter()
                .any(|item| item.as_str().unwrap_or_default().contains("TODO/FIXME"))
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn task_scout_refreshes_stale_node_index_before_suggesting_checks() {
        let root = temp_dir("task_scout_node_stale");
        std::fs::write(
            root.join("package.json"),
            "{\"scripts\":{\"test\":\"node test.js\"}}\n",
        )
        .expect("package");
        std::fs::write(root.join("package-lock.json"), "{}\n").expect("lock");
        std::fs::write(root.join("index.js"), "// TODO: implement peek\n").expect("source");

        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        {
            let mut kernel = kernel.write().expect("kernel");
            let identity = GovernedWriteIdentity::local_demo("human:dev");
            let connected = kernel
                .connect_forge_repo_with_identity(
                    ForgeRepoConnect {
                        tenant_id: "t",
                        actor_id: "human:dev",
                        repo_id: "node_repo",
                        label: "Node Repo",
                        root: root.to_str().unwrap_or(""),
                        kind: "local",
                        origin: "",
                    },
                    &identity,
                )
                .expect("connect");
            kernel
                .index_forge_repo_with_identity(
                    ForgeRepoIndex {
                        tenant_id: "t",
                        actor_id: "human:dev",
                        repo_id: "node_repo",
                        repo_receipt_id: &connected.receipt_id,
                        profile_json: r#"{"language_pack_id":"node","suggested_checks":["npm test"]}"#,
                        profile_fingerprint: "fnv1a64:stale",
                        primary_language: "javascript-typescript",
                        language_pack_id: "node",
                        detected_language_packs: "node",
                        semantic_tool_readiness: "",
                        toolchain_readiness: "",
                        proof_plan_status: "ready",
                        standards_source_summaries: "",
                    },
                    &identity,
                )
                .expect("stale index");
        }

        let response = handle("node_repo", &kernel).expect("response");
        let value: serde_json::Value = serde_json::from_str(&response.body).expect("json");

        assert_eq!(value["suggested_checks"][0], "npm ci");
        assert_eq!(value["suggested_checks"][1], "npm test");
        assert_eq!(value["candidates"][0]["suggested_checks"][0], "npm ci");
        assert_eq!(value["candidates"][0]["suggested_checks"][1], "npm test");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn task_scout_returns_empty_instead_of_generic_padding() {
        let root = temp_dir("task_scout_empty");
        std::fs::create_dir_all(root.join("src")).expect("mkdir");
        std::fs::write(
            root.join("package.json"),
            "{\"scripts\":{\"test\":\"node test.js\"}}\n",
        )
        .expect("package");
        std::fs::write(root.join("src/index.js"), "const value = 1;\n").expect("source");

        let scan = scout_repo(&root, &["npm test".to_string()], &[]);

        assert_eq!(scan.source_files_seen, 1);
        assert_eq!(scan.candidates.len(), 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn task_scout_ranks_untested_public_symbol_with_path() {
        let root = temp_dir("task_scout_untested_symbol");
        std::fs::create_dir_all(root.join("src")).expect("mkdir");
        std::fs::create_dir_all(root.join("test")).expect("mkdir");
        std::fs::write(
            root.join("package.json"),
            "{\"scripts\":{\"test\":\"node test.js\"}}\n",
        )
        .expect("package");
        std::fs::write(
            root.join("src/config.js"),
            "export function parseConfig(input) {\n  return JSON.parse(input);\n}\n",
        )
        .expect("source");
        std::fs::write(
            root.join("test/other.test.js"),
            "test('other', () => {});\n",
        )
        .expect("test");

        let scan = scout_repo(&root, &["npm test".to_string()], &[]);
        let candidate = scan.candidates.first().expect("candidate");

        assert_eq!(candidate["kind"], "untested_public_symbol");
        assert_eq!(candidate["path"], "src/config.js");
        assert_eq!(candidate["line"], 1);
        assert!(
            candidate["title"]
                .as_str()
                .expect("title")
                .contains("parseConfig")
        );
        assert!(!candidate.to_string().contains("return JSON.parse"));
        let _ = std::fs::remove_dir_all(root);
    }

    fn temp_dir(name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "mdx_forge_{name}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }
}
