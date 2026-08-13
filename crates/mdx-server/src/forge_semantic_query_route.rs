// Read-only semantic repo queries for Forge.
//
// This is the first substrate behind the repo intelligence contract. It does
// exposes the language-pack semantic plan, tool readiness, bounded local
// symbol/reference/context lookup, and a gated LSP initialize probe so workers
// and UI can anchor exploration without widening authority.
use crate::RouteResponse;
use mdx_core::{MdxKernel, json_string_literal};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

const MAX_FILES_SCANNED: usize = 500;
const MAX_RESULTS: usize = 50;
const MAX_FILE_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
struct SemanticQueryResult {
    kind: &'static str,
    path: String,
    line: usize,
    column: usize,
    snippet: String,
}

#[derive(Clone, Debug)]
struct SemanticQueryMeta {
    lsp_execution_status: &'static str,
    lsp_execution_allowed: bool,
    lsp_server_command: String,
    lsp_probe_reason: String,
}

#[derive(Clone, Debug)]
struct SemanticSession {
    session_id: String,
    fallback_index_status: &'static str,
    source_file_count: usize,
    indexed_file_count: usize,
    indexed_symbol_count: usize,
    related_test_anchor_count: usize,
    files: Vec<SemanticIndexedFile>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SemanticSessionEvidence {
    pub(crate) session_id: String,
    pub(crate) fallback_index_status: &'static str,
    pub(crate) source_file_count: usize,
    pub(crate) indexed_file_count: usize,
    pub(crate) indexed_symbol_count: usize,
    pub(crate) related_test_anchor_count: usize,
    pub(crate) grants_execution_authority: bool,
}

#[derive(Clone, Debug)]
struct SemanticIndexedFile {
    path: String,
    contents: String,
    is_test: bool,
}

impl Default for SemanticQueryMeta {
    fn default() -> Self {
        Self {
            lsp_execution_status: "not_started",
            lsp_execution_allowed: false,
            lsp_server_command: String::new(),
            lsp_probe_reason: String::new(),
        }
    }
}

pub(crate) fn semantic_session_evidence(
    root: &Path,
    profile: &crate::forge_repo_profile::ForgeRepoProfile,
) -> SemanticSessionEvidence {
    let session = build_semantic_session(root, profile);
    SemanticSessionEvidence {
        session_id: session.session_id,
        fallback_index_status: session.fallback_index_status,
        source_file_count: session.source_file_count,
        indexed_file_count: session.indexed_file_count,
        indexed_symbol_count: session.indexed_symbol_count,
        related_test_anchor_count: session.related_test_anchor_count,
        grants_execution_authority: false,
    }
}

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    if path != "/forge/semantic-queries.json" {
        return None;
    }
    Some(handle(method, body, kernel))
}

pub(crate) fn semantic_query_tool_text(
    root: &Path,
    operation: &str,
    query: &str,
    path: &str,
    line: usize,
) -> String {
    let profile = crate::forge_repo_profile::profile_repo(root);
    let session = build_semantic_session(root, &profile);
    let operation = normalize_operation(operation);
    let mut meta = SemanticQueryMeta::default();
    let (status, reason, results) = run_semantic_operation(
        root, operation, query, path, line, &profile, &session, &mut meta,
    );
    let mut lines = vec![
        format!("semantic_query status={status} operation={operation}"),
        format!(
            "language_pack={} primary_language={}",
            profile.language_pack_id, profile.primary_language
        ),
        format!(
            "semantic_session_id={} fallback_index_status={} indexed_file_count={} indexed_symbol_count={} related_test_anchor_count={}",
            session.session_id,
            session.fallback_index_status,
            session.indexed_file_count,
            session.indexed_symbol_count,
            session.related_test_anchor_count
        ),
        format!(
            "lsp_execution_status={} lsp_execution_allowed={} lsp_server_command={}",
            meta.lsp_execution_status, meta.lsp_execution_allowed, meta.lsp_server_command
        ),
    ];
    if !reason.is_empty() {
        lines.push(format!("reason={reason}"));
    }
    if !meta.lsp_probe_reason.is_empty() {
        lines.push(format!("lsp_probe_reason={}", meta.lsp_probe_reason));
    }
    if operation == "capabilities" {
        lines.push(format!(
            "semantic_intelligence={}",
            profile.semantic_intelligence.join(", ")
        ));
        lines.push(format!(
            "semantic_tool_readiness={}",
            profile.semantic_tool_readiness.join(", ")
        ));
    }
    if results.is_empty() {
        lines.push("results=none".to_string());
    } else {
        for result in results {
            lines.push(format!(
                "{}:{}:{} [{}] {}",
                result.path, result.line, result.column, result.kind, result.snippet
            ));
        }
    }
    lines.join("\n")
}

fn handle(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let operation = json_string_field(body, "operation").unwrap_or_default();
    let query = json_string_field(body, "query").unwrap_or_default();
    let repo_id = json_string_field(body, "repo_id").unwrap_or_default();
    let path = json_string_field(body, "path").unwrap_or_default();
    let line = json_usize_field(body, "line")
        .or_else(|| json_string_field(body, "line").and_then(|value| value.parse::<usize>().ok()))
        .unwrap_or(0);

    let repo_root = {
        let kernel = kernel
            .read()
            .map_err(|_| "kernel lock poisoned".to_string())?;
        if repo_id.trim().is_empty() {
            repo_root()
        } else if let Some(root) = kernel.forge_repo_root(&repo_id) {
            PathBuf::from(root)
        } else {
            return Ok(refusal("connect the repo before querying it semantically"));
        }
    };
    if !repo_root.exists() {
        return Ok(refusal(
            "repo root does not exist; connect a repo before querying it",
        ));
    }

    let profile = crate::forge_repo_profile::profile_repo(&repo_root);
    let session = build_semantic_session(&repo_root, &profile);
    let operation = normalize_operation(&operation);
    let mut meta = SemanticQueryMeta::default();
    let (status, reason, results) = run_semantic_operation(
        &repo_root, operation, &query, &path, line, &profile, &session, &mut meta,
    );
    Ok(RouteResponse::json(
        "200 OK",
        semantic_response_json(
            status, reason, &repo_id, operation, &query, &path, line, &profile, &session, &results,
            &meta,
        ),
    ))
}

#[allow(clippy::too_many_arguments)]
fn run_semantic_operation(
    repo_root: &Path,
    operation: &str,
    query: &str,
    path: &str,
    line: usize,
    profile: &crate::forge_repo_profile::ForgeRepoProfile,
    session: &SemanticSession,
    meta: &mut SemanticQueryMeta,
) -> (&'static str, &'static str, Vec<SemanticQueryResult>) {
    match operation {
        "capabilities" => ("OK", "", Vec::new()),
        "lsp_probe" => {
            let probe =
                crate::forge_semantic_lsp::probe_language_pack(repo_root, profile.language_pack_id);
            meta.lsp_execution_status = probe.status;
            meta.lsp_execution_allowed = crate::forge_semantic_lsp::live_lsp_enabled();
            meta.lsp_server_command = probe.command;
            meta.lsp_probe_reason = probe.reason;
            ("OK", "", Vec::new())
        }
        "orientation" => ("OK", "", orientation(session, path, query)),
        "workspace_symbol" => {
            if query.trim().is_empty() {
                (
                    "REFUSED",
                    "query is required for workspace_symbol",
                    Vec::new(),
                )
            } else {
                ("OK", "", scan_query(session, operation, query))
            }
        }
        "definition" | "references" => {
            if !path.trim().is_empty() && !safe_relative_path(path) {
                (
                    "REFUSED",
                    "path must be a safe relative source path for definition and references",
                    Vec::new(),
                )
            } else if !path.trim().is_empty() && line > 0 {
                let report = crate::forge_semantic_lsp::location_language_pack(
                    repo_root,
                    profile.language_pack_id,
                    operation,
                    path,
                    line,
                    query,
                );
                meta.lsp_execution_status = report.status;
                meta.lsp_execution_allowed = crate::forge_semantic_lsp::live_lsp_enabled();
                meta.lsp_server_command = report.command;
                meta.lsp_probe_reason = report.reason;
                let lsp_results = lsp_location_results(session, operation, &report.locations);
                if report.initialized && !lsp_results.is_empty() {
                    ("OK", "", lsp_results)
                } else if query.trim().is_empty() {
                    (
                        "OK",
                        "",
                        file_anchor_fallback(session, operation, path, line),
                    )
                } else {
                    ("OK", "", scan_query(session, operation, query))
                }
            } else if query.trim().is_empty() {
                (
                    "REFUSED",
                    "query or path plus line is required for definition and references",
                    Vec::new(),
                )
            } else {
                ("OK", "", scan_query(session, operation, query))
            }
        }
        "symbol_graph" => {
            if path.trim().is_empty() && query.trim().is_empty() {
                (
                    "REFUSED",
                    "path or query is required for symbol_graph",
                    Vec::new(),
                )
            } else if !path.trim().is_empty() && !safe_relative_path(path) {
                (
                    "REFUSED",
                    "path must be a safe relative source path for symbol_graph",
                    Vec::new(),
                )
            } else {
                ("OK", "", symbol_graph(session, path, query))
            }
        }
        "impact_radius" => {
            if path.trim().is_empty() && query.trim().is_empty() {
                (
                    "REFUSED",
                    "path or query is required for impact_radius",
                    Vec::new(),
                )
            } else if !path.trim().is_empty() && !safe_relative_path(path) {
                (
                    "REFUSED",
                    "path must be a safe relative source path for impact_radius",
                    Vec::new(),
                )
            } else {
                ("OK", "", impact_radius(session, path, query))
            }
        }
        "dependency_map" => {
            if path.trim().is_empty() && query.trim().is_empty() {
                (
                    "REFUSED",
                    "path or query is required for dependency_map",
                    Vec::new(),
                )
            } else if !path.trim().is_empty() && !safe_relative_path(path) {
                (
                    "REFUSED",
                    "path must be a safe relative source path for dependency_map",
                    Vec::new(),
                )
            } else {
                ("OK", "", dependency_map(session, path, query))
            }
        }
        "hover" => {
            if path.trim().is_empty() || line == 0 {
                (
                    "REFUSED",
                    "path and line are required for hover",
                    Vec::new(),
                )
            } else if !safe_relative_path(path) {
                (
                    "REFUSED",
                    "path must be a safe relative source path for hover",
                    Vec::new(),
                )
            } else {
                let report = crate::forge_semantic_lsp::hover_language_pack(
                    repo_root,
                    profile.language_pack_id,
                    path,
                    line,
                );
                meta.lsp_execution_status = report.status;
                meta.lsp_execution_allowed = crate::forge_semantic_lsp::live_lsp_enabled();
                meta.lsp_server_command = report.command;
                meta.lsp_probe_reason = report.reason;
                if report.initialized && !report.hover.trim().is_empty() {
                    (
                        "OK",
                        "",
                        vec![SemanticQueryResult {
                            kind: "lsp_hover",
                            path: path.to_string(),
                            line,
                            column: 1,
                            snippet: report.hover,
                        }],
                    )
                } else {
                    ("OK", "", hover_context(session, path, line))
                }
            }
        }
        "file_outline" => {
            if path.trim().is_empty() {
                ("REFUSED", "path is required for file_outline", Vec::new())
            } else {
                ("OK", "", file_outline(session, path))
            }
        }
        "related_tests" => {
            if path.trim().is_empty() && query.trim().is_empty() {
                (
                    "REFUSED",
                    "path or query is required for related_tests",
                    Vec::new(),
                )
            } else if !path.trim().is_empty() && !safe_relative_path(path) {
                (
                    "REFUSED",
                    "path must be a safe relative source path for related_tests",
                    Vec::new(),
                )
            } else {
                ("OK", "", related_tests(session, path, query))
            }
        }
        "diagnostics" => match diagnostic_target_path(session, path) {
            Ok(Some(target_path)) => {
                let report = crate::forge_semantic_lsp::diagnostics_language_pack(
                    repo_root,
                    profile.language_pack_id,
                    &target_path,
                );
                meta.lsp_execution_status = report.status;
                meta.lsp_execution_allowed = crate::forge_semantic_lsp::live_lsp_enabled();
                meta.lsp_server_command = report.command;
                meta.lsp_probe_reason = report.reason;
                (
                    "OK",
                    "",
                    diagnostics_results(&target_path, &report.diagnostics),
                )
            }
            Ok(None) => ("OK", "no source file found for diagnostics", Vec::new()),
            Err(reason) => ("REFUSED", reason, Vec::new()),
        },
        _ => (
            "REFUSED",
            "operation must be capabilities, orientation, workspace_symbol, definition, references, impact_radius, dependency_map, symbol_graph, hover, file_outline, related_tests, diagnostics, or lsp_probe",
            Vec::new(),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn semantic_response_json(
    status: &str,
    reason: &str,
    repo_id: &str,
    operation: &str,
    query: &str,
    path: &str,
    line: usize,
    profile: &crate::forge_repo_profile::ForgeRepoProfile,
    session: &SemanticSession,
    results: &[SemanticQueryResult],
    meta: &SemanticQueryMeta,
) -> String {
    let semantic = profile
        .semantic_intelligence
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",");
    let readiness = profile
        .semantic_tool_readiness
        .iter()
        .map(|value| json_string_literal(value))
        .collect::<Vec<_>>()
        .join(",");
    let results_json = results
        .iter()
        .map(|result| {
            format!(
                r#"{{"kind":{},"path":{},"line":{},"column":{},"snippet":{}}}"#,
                json_string_literal(result.kind),
                json_string_literal(&result.path),
                result.line,
                result.column,
                json_string_literal(&result.snippet),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let summary = match status {
        "OK" if operation == "diagnostics" => {
            "bounded read-only LSP diagnostics returned when MDX_FORGE_LSP_EXEC allows a local language server"
        }
        "OK" if operation == "capabilities" => {
            "semantic capabilities returned from the repo language pack"
        }
        "OK" if operation == "orientation" => {
            "bounded semantic orientation returned likely definitions, references, file outline, and related tests"
        }
        "OK" if operation == "lsp_probe" => meta.lsp_probe_reason.as_str(),
        "OK" if operation == "related_tests" => {
            "bounded local semantic fallback returned likely related test anchors"
        }
        "OK" if operation == "impact_radius" => {
            "bounded local semantic fallback returned definitions, references, file outline, and likely tests"
        }
        "OK" if operation == "dependency_map" => {
            "bounded local semantic fallback returned imports, package manifests, neighboring source files, references, and likely tests"
        }
        "OK" if operation == "symbol_graph" => {
            "bounded local semantic fallback returned declarations, definitions, references, dependency edges, and related tests for the requested anchor"
        }
        "OK" => "bounded local semantic fallback returned anchored results",
        _ => reason,
    };
    format!(
        r#"{{"name":"mdx-forge-semantic-query","status":{},"reason":{},"repo_id":{},"operation":{},"query":{},"path":{},"line":{},"primary_language":{},"language_pack_id":{},"semantic_session_id":{},"fallback_index_status":{},"source_file_count":{},"indexed_file_count":{},"indexed_symbol_count":{},"related_test_anchor_count":{},"semantic_intelligence":[{}],"semantic_tool_readiness":[{}],"lsp_execution_status":{},"lsp_execution_allowed":{},"lsp_server_command":{},"lsp_probe_reason":{},"adapter_execution_allowed":false,"provider_calls_allowed":false,"production_write_allowed":false,"result_count":{},"results":[{}],"summary":{}}}"#,
        json_string_literal(status),
        json_string_literal(reason),
        json_string_literal(repo_id),
        json_string_literal(operation),
        json_string_literal(query),
        json_string_literal(path),
        line,
        json_string_literal(profile.primary_language),
        json_string_literal(profile.language_pack_id),
        json_string_literal(&session.session_id),
        json_string_literal(session.fallback_index_status),
        session.source_file_count,
        session.indexed_file_count,
        session.indexed_symbol_count,
        session.related_test_anchor_count,
        semantic,
        readiness,
        json_string_literal(meta.lsp_execution_status),
        meta.lsp_execution_allowed,
        json_string_literal(&meta.lsp_server_command),
        json_string_literal(&meta.lsp_probe_reason),
        results.len(),
        results_json,
        json_string_literal(summary),
    )
}

fn build_semantic_session(
    root: &Path,
    profile: &crate::forge_repo_profile::ForgeRepoProfile,
) -> SemanticSession {
    let mut scanned = 0;
    let mut files = Vec::new();
    index_source_dir(root, root, &mut scanned, &mut files);
    let indexed_symbol_count = files
        .iter()
        .map(|file| declaration_results(file).len())
        .sum();
    let related_test_anchor_count = files.iter().filter(|file| file.is_test).count();
    let fallback_index_status = if files.is_empty() {
        "empty"
    } else if scanned >= MAX_FILES_SCANNED {
        "capped"
    } else {
        "indexed"
    };
    let session_id = semantic_session_id(root, profile.language_pack_id, &files);
    SemanticSession {
        session_id,
        fallback_index_status,
        source_file_count: scanned,
        indexed_file_count: files.len(),
        indexed_symbol_count,
        related_test_anchor_count,
        files,
    }
}

fn index_source_dir(
    root: &Path,
    dir: &Path,
    scanned: &mut usize,
    files: &mut Vec<SemanticIndexedFile>,
) {
    if *scanned >= MAX_FILES_SCANNED || files.len() >= MAX_FILES_SCANNED {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if *scanned >= MAX_FILES_SCANNED || files.len() >= MAX_FILES_SCANNED {
            return;
        }
        let path = entry.path();
        let relative = relative_path(root, &path);
        if crate::forge_repo_profile::is_artifact_path(&relative)
            || relative.starts_with(".git/")
            || relative == ".git"
        {
            continue;
        }
        if path.is_dir() {
            index_source_dir(root, &path, scanned, files);
        } else if is_scan_candidate(&path) {
            *scanned += 1;
            let Ok(metadata) = std::fs::metadata(&path) else {
                continue;
            };
            if metadata.len() > MAX_FILE_BYTES {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            files.push(SemanticIndexedFile {
                is_test: looks_like_test_path(&relative),
                path: relative,
                contents,
            });
        }
    }
}

fn semantic_session_id(
    root: &Path,
    language_pack_id: &str,
    files: &[SemanticIndexedFile],
) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for value in [root.to_string_lossy().as_ref(), language_pack_id] {
        fnv1a_update(&mut hash, value.as_bytes());
    }
    for file in files.iter().take(MAX_FILES_SCANNED) {
        fnv1a_update(&mut hash, file.path.as_bytes());
        fnv1a_update(&mut hash, file.contents.len().to_string().as_bytes());
    }
    format!("semantic_session_fnv1a64:{hash:016x}")
}

fn fnv1a_update(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(0x100000001b3);
    }
}

fn scan_query(session: &SemanticSession, operation: &str, query: &str) -> Vec<SemanticQueryResult> {
    let needle = query.trim();
    let mut results = Vec::new();
    for file in &session.files {
        scan_file(file, needle, operation, &mut results);
        if results.len() >= MAX_RESULTS {
            break;
        }
    }
    results
}

fn scan_file(
    file: &SemanticIndexedFile,
    needle: &str,
    operation: &str,
    results: &mut Vec<SemanticQueryResult>,
) {
    if results.len() >= MAX_RESULTS {
        return;
    }
    for (index, line) in file.contents.lines().enumerate() {
        if results.len() >= MAX_RESULTS {
            return;
        }
        let Some(column) = line.find(needle) else {
            continue;
        };
        if operation == "definition" && !looks_like_definition(line, needle) {
            continue;
        }
        results.push(SemanticQueryResult {
            kind: match operation {
                "definition" => "definition",
                "references" => "reference",
                _ => "workspace_symbol",
            },
            path: file.path.clone(),
            line: index + 1,
            column: column + 1,
            snippet: line.trim().to_string(),
        });
    }
}

fn hover_context(session: &SemanticSession, path: &str, line: usize) -> Vec<SemanticQueryResult> {
    if !safe_relative_path(path) {
        return Vec::new();
    }
    let Some(file) = session.files.iter().find(|file| file.path == path) else {
        return Vec::new();
    };
    let Some(snippet) = file.contents.lines().nth(line.saturating_sub(1)) else {
        return Vec::new();
    };
    vec![SemanticQueryResult {
        kind: "hover_context",
        path: path.to_string(),
        line,
        column: 1,
        snippet: snippet.trim().to_string(),
    }]
}

fn lsp_location_results(
    session: &SemanticSession,
    operation: &str,
    locations: &[crate::forge_semantic_lsp::LspLocation],
) -> Vec<SemanticQueryResult> {
    locations
        .iter()
        .take(MAX_RESULTS)
        .filter(|location| safe_relative_path(&location.path))
        .map(|location| SemanticQueryResult {
            kind: match operation {
                "references" => "reference",
                _ => "definition",
            },
            path: location.path.clone(),
            line: location.line,
            column: location.column,
            snippet: snippet_at(session, &location.path, location.line)
                .unwrap_or_else(|| "LSP location resolved".to_string()),
        })
        .collect()
}

fn file_anchor_fallback(
    session: &SemanticSession,
    operation: &str,
    path: &str,
    line: usize,
) -> Vec<SemanticQueryResult> {
    let Some(snippet) = snippet_at(session, path, line) else {
        return Vec::new();
    };
    vec![SemanticQueryResult {
        kind: match operation {
            "references" => "reference",
            _ => "definition",
        },
        path: path.to_string(),
        line,
        column: 1,
        snippet,
    }]
}

fn snippet_at(session: &SemanticSession, path: &str, line: usize) -> Option<String> {
    session
        .files
        .iter()
        .find(|file| file.path == path)?
        .contents
        .lines()
        .nth(line.saturating_sub(1))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn file_outline(session: &SemanticSession, path: &str) -> Vec<SemanticQueryResult> {
    if !safe_relative_path(path) {
        return Vec::new();
    }
    let Some(file) = session.files.iter().find(|file| file.path == path) else {
        return Vec::new();
    };
    declaration_results(file)
        .into_iter()
        .take(MAX_RESULTS)
        .collect()
}

fn declaration_results(file: &SemanticIndexedFile) -> Vec<SemanticQueryResult> {
    file.contents
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            if !looks_like_declaration_line(line) {
                return None;
            }
            Some(SemanticQueryResult {
                kind: "file_symbol",
                path: file.path.clone(),
                line: index + 1,
                column: line
                    .chars()
                    .position(|ch| !ch.is_whitespace())
                    .map(|value| value + 1)
                    .unwrap_or(1),
                snippet: line.trim().to_string(),
            })
        })
        .collect()
}

fn related_tests(session: &SemanticSession, path: &str, query: &str) -> Vec<SemanticQueryResult> {
    let needles = related_test_needles(path, query);
    if needles.is_empty() {
        return Vec::new();
    }
    let mut ranked = Vec::new();
    for file in &session.files {
        scan_related_test_file(file, &needles, &mut ranked);
        if ranked.len() >= MAX_RESULTS {
            break;
        }
    }
    ranked.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.path.cmp(&right.1.path))
            .then_with(|| left.1.line.cmp(&right.1.line))
    });
    let mut seen = std::collections::BTreeSet::new();
    ranked
        .into_iter()
        .filter_map(|(_, result)| {
            if seen.insert(result.path.clone()) {
                Some(result)
            } else {
                None
            }
        })
        .take(MAX_RESULTS)
        .collect()
}

fn impact_radius(session: &SemanticSession, path: &str, query: &str) -> Vec<SemanticQueryResult> {
    let anchor = impact_anchor(path, query);
    let mut results = Vec::new();
    if !path.trim().is_empty() {
        results.extend(file_outline(session, path));
    }
    if !anchor.is_empty() {
        results.extend(scan_query(session, "definition", &anchor));
    }
    results.extend(related_tests(session, path, &anchor));
    if !anchor.is_empty() {
        results.extend(scan_query(session, "references", &anchor));
    }
    dedupe_results(results)
}

fn dependency_map(session: &SemanticSession, path: &str, query: &str) -> Vec<SemanticQueryResult> {
    let anchor = impact_anchor(path, query);
    let mut results = Vec::new();
    let target_paths = dependency_target_paths(session, path, &anchor);
    for target_path in target_paths.iter().take(8) {
        results.extend(package_manifest_results(session));
        results.extend(import_results(session, target_path));
        results.extend(neighbor_source_results(session, target_path));
        results.extend(related_tests(session, target_path, &anchor));
    }
    if !anchor.is_empty() {
        results.extend(scan_query(session, "definition", &anchor));
        results.extend(scan_query(session, "references", &anchor));
    }
    dedupe_results(results)
}

fn symbol_graph(session: &SemanticSession, path: &str, query: &str) -> Vec<SemanticQueryResult> {
    let anchors = symbol_graph_anchors(session, path, query);
    let mut results = Vec::new();
    if !path.trim().is_empty() {
        results.extend(file_outline(session, path));
        results.extend(import_results(session, path));
        results.extend(neighbor_source_results(session, path));
    }
    for anchor in anchors.iter().take(8) {
        results.extend(scan_query(session, "definition", anchor));
        results.extend(scan_query(session, "references", anchor));
        results.extend(related_tests(session, path, anchor));
    }
    results.extend(dependency_map(
        session,
        path,
        anchors.first().map(String::as_str).unwrap_or(""),
    ));
    dedupe_results(results)
}

fn symbol_graph_anchors(session: &SemanticSession, path: &str, query: &str) -> Vec<String> {
    let mut anchors = Vec::new();
    let query = query.trim();
    if !query.is_empty() {
        push_anchor(&mut anchors, query);
    }
    if !path.trim().is_empty() {
        if let Some(stem) = Path::new(path)
            .file_stem()
            .and_then(|value| value.to_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            push_anchor(&mut anchors, stem);
        }
        if let Some(file) = session.files.iter().find(|file| file.path == path) {
            for declaration in declaration_results(file).into_iter().take(8) {
                if let Some(symbol) = symbol_name_from_declaration(&declaration.snippet) {
                    push_anchor(&mut anchors, &symbol);
                }
            }
        }
    }
    anchors
}

fn push_anchor(anchors: &mut Vec<String>, value: &str) {
    let value = value.trim();
    if value.len() >= 2 && !anchors.iter().any(|anchor| anchor == value) {
        anchors.push(value.to_string());
    }
}

fn dependency_target_paths(session: &SemanticSession, path: &str, anchor: &str) -> Vec<String> {
    let mut paths = Vec::new();
    if !path.trim().is_empty() && session.files.iter().any(|file| file.path == path) {
        paths.push(path.to_string());
    }
    if !anchor.trim().is_empty() {
        for result in scan_query(session, "workspace_symbol", anchor) {
            if !paths.contains(&result.path) {
                paths.push(result.path);
            }
            if paths.len() >= 8 {
                break;
            }
        }
    }
    paths
}

fn package_manifest_results(session: &SemanticSession) -> Vec<SemanticQueryResult> {
    session
        .files
        .iter()
        .filter(|file| is_package_manifest_path(&file.path))
        .take(8)
        .map(|file| SemanticQueryResult {
            kind: "package_manifest",
            path: file.path.clone(),
            line: 1,
            column: 1,
            snippet: file.path.clone(),
        })
        .collect()
}

fn import_results(session: &SemanticSession, path: &str) -> Vec<SemanticQueryResult> {
    let Some(file) = session.files.iter().find(|file| file.path == path) else {
        return Vec::new();
    };
    file.contents
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            if !looks_like_import_line(line) {
                return None;
            }
            Some(SemanticQueryResult {
                kind: "import",
                path: file.path.clone(),
                line: index + 1,
                column: line
                    .chars()
                    .position(|ch| !ch.is_whitespace())
                    .map(|value| value + 1)
                    .unwrap_or(1),
                snippet: line.trim().to_string(),
            })
        })
        .take(20)
        .collect()
}

fn neighbor_source_results(session: &SemanticSession, path: &str) -> Vec<SemanticQueryResult> {
    let parent = Path::new(path)
        .parent()
        .map(|value| value.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    if parent.is_empty() {
        return Vec::new();
    }
    session
        .files
        .iter()
        .filter(|file| file.path != path && !file.is_test)
        .filter(|file| {
            Path::new(&file.path)
                .parent()
                .map(|value| value.to_string_lossy().replace('\\', "/") == parent)
                .unwrap_or(false)
        })
        .take(12)
        .map(|file| SemanticQueryResult {
            kind: "neighbor_source",
            path: file.path.clone(),
            line: 1,
            column: 1,
            snippet: first_declaration_or_path(file),
        })
        .collect()
}

fn orientation(session: &SemanticSession, path: &str, query: &str) -> Vec<SemanticQueryResult> {
    let mut results = Vec::new();
    if !path.trim().is_empty() || !query.trim().is_empty() {
        results.extend(impact_radius(session, path, query));
    }
    if results.is_empty() {
        results.extend(entrypoint_outlines(session));
    }
    dedupe_results(results)
}

fn entrypoint_outlines(session: &SemanticSession) -> Vec<SemanticQueryResult> {
    let mut results = Vec::new();
    for file in &session.files {
        if !file.is_test && is_entrypoint_source_path(&file.path) {
            results.extend(declaration_results(file));
        }
        if results.len() >= MAX_RESULTS {
            break;
        }
    }
    results
}

fn diagnostic_target_path(
    session: &SemanticSession,
    path: &str,
) -> Result<Option<String>, &'static str> {
    if !path.trim().is_empty() {
        if safe_relative_path(path) {
            return Ok(Some(path.to_string()));
        }
        return Err("path must be a safe relative source path for diagnostics");
    }
    Ok(session
        .files
        .iter()
        .find(|file| !file.is_test && is_entrypoint_source_path(&file.path))
        .map(|file| file.path.clone()))
}

fn diagnostics_results(
    path: &str,
    diagnostics: &[crate::forge_semantic_lsp::LspDiagnostic],
) -> Vec<SemanticQueryResult> {
    diagnostics
        .iter()
        .take(MAX_RESULTS)
        .map(|diagnostic| SemanticQueryResult {
            kind: "diagnostic",
            path: path.to_string(),
            line: diagnostic.line,
            column: diagnostic.column,
            snippet: format!(
                "severity={} message={}",
                diagnostic.severity, diagnostic.message
            ),
        })
        .collect()
}

fn is_entrypoint_source_path(relative: &str) -> bool {
    !looks_like_test_path(relative)
        && Path::new(relative)
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(is_source_extension)
}

fn is_source_extension(extension: &str) -> bool {
    matches!(
        extension,
        "swift" | "java" | "kt" | "kts" | "cs" | "ts" | "tsx" | "js" | "jsx" | "rs" | "py" | "go"
    )
}

fn is_package_manifest_path(path: &str) -> bool {
    matches!(
        Path::new(path).file_name().and_then(|value| value.to_str()),
        Some(
            "Package.swift"
                | "pom.xml"
                | "build.gradle"
                | "build.gradle.kts"
                | "settings.gradle"
                | "settings.gradle.kts"
                | "Cargo.toml"
                | "go.mod"
                | "package.json"
                | "pyproject.toml"
        )
    )
}

fn looks_like_import_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    matches!(
        trimmed.split_whitespace().next(),
        Some("import" | "package" | "using" | "use" | "from" | "require")
    ) || trimmed.starts_with("const ") && trimmed.contains("require(")
        || trimmed.starts_with("let ") && trimmed.contains("require(")
        || trimmed.starts_with("var ") && trimmed.contains("require(")
}

fn first_declaration_or_path(file: &SemanticIndexedFile) -> String {
    declaration_results(file)
        .into_iter()
        .next()
        .map(|result| result.snippet)
        .unwrap_or_else(|| file.path.clone())
}

fn symbol_name_from_declaration(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let tokens = trimmed
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    for (index, token) in tokens.iter().enumerate() {
        if matches!(
            *token,
            "class"
                | "struct"
                | "enum"
                | "interface"
                | "protocol"
                | "func"
                | "function"
                | "fn"
                | "def"
                | "record"
                | "object"
                | "type"
                | "const"
                | "let"
                | "var"
        ) {
            return tokens.get(index + 1).map(|value| (*value).to_string());
        }
    }
    None
}

fn impact_anchor(path: &str, query: &str) -> String {
    let query = query.trim();
    if !query.is_empty() {
        return query.to_string();
    }
    Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn dedupe_results(results: Vec<SemanticQueryResult>) -> Vec<SemanticQueryResult> {
    let mut seen = std::collections::BTreeSet::new();
    let mut deduped = Vec::new();
    for result in results {
        let key = format!(
            "{}\0{}\0{}\0{}",
            result.kind, result.path, result.line, result.snippet
        );
        if seen.insert(key) {
            deduped.push(result);
        }
        if deduped.len() >= MAX_RESULTS {
            break;
        }
    }
    deduped
}

fn related_test_needles(path: &str, query: &str) -> Vec<String> {
    let mut needles = Vec::new();
    let query = query.trim();
    if !query.is_empty() {
        push_test_needle(&mut needles, query);
    }
    if !path.trim().is_empty()
        && let Some(stem) = Path::new(path)
            .file_stem()
            .and_then(|value| value.to_str())
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
    {
        push_test_needle(&mut needles, stem);
        for suffix in [
            "ViewModel",
            "Controller",
            "Service",
            "Repository",
            "Manager",
        ] {
            if let Some(stripped) = stem.strip_suffix(suffix) {
                push_test_needle(&mut needles, stripped);
            }
        }
    }
    needles
}

fn push_test_needle(needles: &mut Vec<String>, value: &str) {
    let normalized = normalize_test_anchor(value);
    if normalized.len() >= 2 && !needles.contains(&normalized) {
        needles.push(normalized);
    }
}

fn scan_related_test_file(
    file: &SemanticIndexedFile,
    needles: &[String],
    results: &mut Vec<(usize, SemanticQueryResult)>,
) {
    if results.len() >= MAX_RESULTS {
        return;
    }
    if !file.is_test {
        return;
    };
    let normalized_path = normalize_test_anchor(&file.path);
    let normalized_contents = normalize_test_anchor(&file.contents);
    let Some((rank, matched)) = needles.iter().find_map(|needle| {
        if normalized_path.contains(needle) {
            Some((0, needle.as_str()))
        } else if normalized_contents.contains(needle) {
            Some((1, needle.as_str()))
        } else {
            None
        }
    }) else {
        return;
    };
    let (line, snippet) = related_test_snippet(&file.contents, matched);
    results.push((
        rank,
        SemanticQueryResult {
            kind: "related_test",
            path: file.path.clone(),
            line,
            column: 1,
            snippet,
        },
    ));
}

fn related_test_snippet(contents: &str, matched: &str) -> (usize, String) {
    let normalized_matched = normalize_test_anchor(matched);
    for (index, line) in contents.lines().enumerate() {
        if normalize_test_anchor(line).contains(&normalized_matched) {
            return (index + 1, line.trim().to_string());
        }
    }
    let first = contents
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or("related test file");
    (1, first.to_string())
}

fn looks_like_test_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let name = normalized.rsplit('/').next().unwrap_or(normalized.as_str());
    normalized.starts_with("tests/")
        || normalized.contains("/tests/")
        || normalized.contains(".tests/")
        || normalized.contains("test/")
        || normalized.contains("__tests__/")
        || name.ends_with("test.swift")
        || name.ends_with("tests.swift")
        || name.ends_with("test.java")
        || name.ends_with("tests.java")
        || name.ends_with("test.kt")
        || name.ends_with("tests.kt")
        || name.ends_with("test.cs")
        || name.ends_with("tests.cs")
        || name.ends_with(".test.ts")
        || name.ends_with(".spec.ts")
        || name.ends_with(".test.tsx")
        || name.ends_with(".spec.tsx")
        || name.ends_with(".test.js")
        || name.ends_with(".spec.js")
        || name.ends_with("_test.py")
        || name.starts_with("test_")
        || name.ends_with("_test.go")
        || name.contains("spec")
}

fn normalize_test_anchor(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn safe_relative_path(path: &str) -> bool {
    let safe_path = Path::new(path);
    !path.trim().is_empty()
        && !safe_path.is_absolute()
        && !path.contains("..")
        && !crate::forge_repo_profile::is_artifact_path(path)
}

fn looks_like_definition(line: &str, needle: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with('*')
        || trimmed.starts_with("import ")
        || trimmed.starts_with("using ")
    {
        return false;
    }
    if !trimmed.contains(needle) {
        return false;
    }
    looks_like_declaration_line(line)
        || trimmed.contains(&format!("{needle}("))
        || trimmed.contains(&format!("{needle} ="))
        || trimmed.contains(&format!("{needle}:"))
}

fn looks_like_declaration_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with('*')
        || trimmed.starts_with("import ")
        || trimmed.starts_with("using ")
    {
        return false;
    }
    let declaration_markers = [
        "class ",
        "struct ",
        "enum ",
        "interface ",
        "protocol ",
        "func ",
        "function ",
        "fn ",
        "def ",
        "public ",
        "private ",
        "internal ",
        "protected ",
        "static ",
        "const ",
        "let ",
        "var ",
        "type ",
        "record ",
        "object ",
    ];
    declaration_markers
        .iter()
        .any(|marker| trimmed.contains(marker))
}

fn is_scan_candidate(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    if matches!(
        name,
        "Package.swift"
            | "pom.xml"
            | "build.gradle"
            | "build.gradle.kts"
            | "settings.gradle"
            | "settings.gradle.kts"
            | "Cargo.toml"
            | "go.mod"
            | "package.json"
            | "pyproject.toml"
    ) {
        return true;
    }
    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some(
            "swift"
                | "java"
                | "kt"
                | "kts"
                | "cs"
                | "ts"
                | "tsx"
                | "js"
                | "jsx"
                | "rs"
                | "py"
                | "go"
                | "xml"
                | "json"
                | "yaml"
                | "yml"
                | "toml"
        )
    )
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn normalize_operation(operation: &str) -> &str {
    match operation.trim() {
        "" => "capabilities",
        "orient" | "orientation" | "mission_pack" => "orientation",
        "symbols" => "workspace_symbol",
        "go_to_definition" | "definitions" => "definition",
        "find_references" => "references",
        "impact" | "impact_map" | "impact_radius" => "impact_radius",
        "deps" | "dependencies" | "imports" | "dependency_map" => "dependency_map",
        "graph" | "symbols_graph" | "symbol_graph" => "symbol_graph",
        "outline" => "file_outline",
        "tests" | "test_files" => "related_tests",
        "lsp" => "lsp_probe",
        other => other,
    }
}

fn repo_root() -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut cursor = cwd.as_path();
    while let Some(parent) = cursor.parent() {
        if cursor.join("Cargo.toml").exists() && cursor.join("crates").exists() {
            return cursor.to_path_buf();
        }
        cursor = parent;
    }
    cwd
}

fn refusal(reason: &str) -> RouteResponse {
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-forge-semantic-query","status":"REFUSED","reason":{},"lsp_execution_allowed":false,"adapter_execution_allowed":false,"provider_calls_allowed":false,"production_write_allowed":false,"result_count":0,"results":[]}}"#,
            json_string_literal(reason),
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

fn json_usize_field(body: &str, key: &str) -> Option<usize> {
    let marker = format!("\"{key}\":");
    let after = body.split(&marker).nth(1)?.trim_start();
    let digits = after
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        None
    } else {
        digits.parse::<usize>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdx_core::{ForgeRepoConnect, MdxKernel};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, RwLock};

    static TEMP_REPO_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempRepo {
        path: PathBuf,
    }

    impl TempRepo {
        fn swift() -> Self {
            let counter = TEMP_REPO_COUNTER.fetch_add(1, Ordering::Relaxed);
            let unique = format!(
                "mdx-forge-semantic-query-{}-{}-{}",
                std::process::id(),
                counter,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(path.join("Sources/Canary")).expect("source dir");
            std::fs::create_dir_all(path.join("Tests/CanaryTests")).expect("test dir");
            std::fs::create_dir_all(path.join(".git")).expect("git marker");
            std::fs::write(path.join("Package.swift"), "// swift-tools-version: 5.9\n")
                .expect("package");
            std::fs::write(
                path.join("Sources/Canary/Slugger.swift"),
                "import Foundation\n\npublic struct Slugger {\n    public func slugify(_ value: String) -> String { value.lowercased() }\n}\n",
            )
            .expect("source");
            std::fs::write(
                path.join("Sources/Canary/SlugRules.swift"),
                "public enum SlugRules {\n    public static let separator = \"-\"\n}\n",
            )
            .expect("neighbor");
            std::fs::write(
                path.join("Tests/CanaryTests/SluggerTests.swift"),
                "import XCTest\n@testable import Canary\n\nfinal class SluggerTests: XCTestCase {\n    func testSluggerLowercasesInput() {\n        XCTAssertEqual(Slugger().slugify(\"Hello\"), \"hello\")\n    }\n}\n",
            )
            .expect("tests");
            Self { path }
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn semantic_query_returns_language_pack_capabilities() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"capabilities"}"#,
            &kernel,
        )
        .expect("route");

        assert!(
            response.body.contains(r#""status":"OK""#),
            "{}",
            response.body
        );
        assert!(response.body.contains(r#""language_pack_id":"swift-spm""#));
        assert!(
            response
                .body
                .contains(r#""semantic_session_id":"semantic_session_fnv1a64:"#)
        );
        assert!(
            response
                .body
                .contains(r#""fallback_index_status":"indexed""#)
        );
        assert!(response.body.contains(r#""indexed_file_count":4"#));
        assert!(response.body.contains(r#""related_test_anchor_count":1"#));
        let value: serde_json::Value = serde_json::from_str(&response.body).expect("semantic json");
        assert!(
            value["indexed_symbol_count"].as_u64().unwrap_or_default() > 0,
            "{}",
            response.body
        );
        assert!(response.body.contains("lsp:sourcekit-lsp"));
        assert!(
            response
                .body
                .contains(r#""lsp_execution_status":"not_started""#)
        );
        assert!(
            response
                .body
                .contains(r#""production_write_allowed":false"#)
        );
    }

    #[test]
    fn semantic_query_refuses_unknown_repo_without_falling_back_to_mdx_root() {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));

        let response = handle(
            "POST",
            r#"{"repo_id":"missing-repo","operation":"capabilities"}"#,
            &kernel,
        )
        .expect("route");

        assert!(
            response.body.contains(r#""status":"REFUSED""#),
            "{}",
            response.body
        );
        assert!(
            response
                .body
                .contains("connect the repo before querying it semantically"),
            "{}",
            response.body
        );
        assert!(
            response.body.contains(r#""result_count":0"#),
            "{}",
            response.body
        );
        assert!(
            !response.body.contains("semantic_session_fnv1a64"),
            "{}",
            response.body
        );
    }

    #[test]
    fn semantic_query_finds_workspace_symbol_with_bounded_fallback() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"workspace_symbol","query":"Slugger"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""kind":"workspace_symbol""#));
        assert!(response.body.contains("Sources/Canary/Slugger.swift"));
        assert!(response.body.contains("public struct Slugger"));
    }

    #[test]
    fn semantic_query_finds_likely_definition_without_lsp() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"definition","query":"Slugger"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""operation":"definition""#));
        assert!(response.body.contains(r#""kind":"definition""#));
        assert!(response.body.contains("Sources/Canary/Slugger.swift"));
        assert!(response.body.contains("public struct Slugger"));
    }

    #[test]
    fn semantic_query_definition_can_use_path_line_anchor_with_gated_lsp() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"definition","path":"Sources/Canary/Slugger.swift","line":3,"query":"Slugger"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""operation":"definition""#));
        assert!(
            response
                .body
                .contains(r#""lsp_execution_status":"disabled""#)
        );
        assert!(response.body.contains(r#""lsp_execution_allowed":false"#));
        assert!(response.body.contains(r#""kind":"definition""#));
        assert!(response.body.contains("Sources/Canary/Slugger.swift"));
        assert!(response.body.contains("public struct Slugger"));
        assert!(response.body.contains(r#""provider_calls_allowed":false"#));
        assert!(
            response
                .body
                .contains(r#""production_write_allowed":false"#)
        );
    }

    #[test]
    fn semantic_query_references_can_use_path_line_anchor_with_gated_lsp() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"references","path":"Sources/Canary/Slugger.swift","line":3}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""operation":"references""#));
        assert!(
            response
                .body
                .contains(r#""lsp_execution_status":"disabled""#)
        );
        assert!(response.body.contains(r#""kind":"reference""#));
        assert!(response.body.contains("Sources/Canary/Slugger.swift"));
        assert!(response.body.contains("public struct Slugger"));
    }

    #[test]
    fn semantic_query_definition_refuses_unsafe_lsp_anchor() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"definition","path":"../Slugger.swift","line":3}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("safe relative source path"));
        assert!(
            response
                .body
                .contains(r#""production_write_allowed":false"#)
        );
    }

    #[test]
    fn semantic_query_orientation_bundles_impact_and_related_tests() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"orientation","query":"Slugger"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""operation":"orientation""#));
        assert!(response.body.contains(r#""kind":"definition""#));
        assert!(response.body.contains(r#""kind":"related_test""#));
        assert!(response.body.contains("Sources/Canary/Slugger.swift"));
        assert!(
            response
                .body
                .contains("Tests/CanaryTests/SluggerTests.swift")
        );
    }

    #[test]
    fn semantic_query_returns_symbol_graph_for_source_path() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"symbol_graph","path":"Sources/Canary/Slugger.swift"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""operation":"symbol_graph""#));
        assert!(response.body.contains(r#""kind":"file_symbol""#));
        assert!(response.body.contains(r#""kind":"definition""#));
        assert!(response.body.contains(r#""kind":"reference""#));
        assert!(response.body.contains(r#""kind":"import""#));
        assert!(response.body.contains(r#""kind":"package_manifest""#));
        assert!(response.body.contains(r#""kind":"related_test""#));
        assert!(response.body.contains("public struct Slugger"));
        assert!(
            response
                .body
                .contains("Tests/CanaryTests/SluggerTests.swift")
        );
    }

    #[test]
    fn semantic_query_symbol_graph_refuses_unsafe_source_path() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"symbol_graph","path":"../Slugger.swift"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("safe relative source path"));
        assert!(
            response
                .body
                .contains(r#""production_write_allowed":false"#)
        );
    }

    #[test]
    fn semantic_query_returns_file_outline_without_lsp() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"file_outline","path":"Sources/Canary/Slugger.swift"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""operation":"file_outline""#));
        assert!(response.body.contains(r#""kind":"file_symbol""#));
        assert!(response.body.contains("public struct Slugger"));
        assert!(response.body.contains("public func slugify"));
    }

    #[test]
    fn semantic_query_finds_related_tests_for_source_path() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"related_tests","path":"Sources/Canary/Slugger.swift"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""operation":"related_tests""#));
        assert!(response.body.contains(r#""kind":"related_test""#));
        assert!(
            response
                .body
                .contains("Tests/CanaryTests/SluggerTests.swift")
        );
        assert!(response.body.contains("SluggerTests"));
    }

    #[test]
    fn semantic_query_returns_impact_radius_for_symbol() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"impact_radius","query":"Slugger"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""operation":"impact_radius""#));
        assert!(response.body.contains(r#""kind":"definition""#));
        assert!(response.body.contains(r#""kind":"reference""#));
        assert!(response.body.contains(r#""kind":"related_test""#));
        assert!(response.body.contains("Sources/Canary/Slugger.swift"));
        assert!(
            response
                .body
                .contains("Tests/CanaryTests/SluggerTests.swift")
        );
    }

    #[test]
    fn semantic_query_returns_impact_radius_for_source_path() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"impact_radius","path":"Sources/Canary/Slugger.swift"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""kind":"file_symbol""#));
        assert!(response.body.contains("public func slugify"));
        assert!(
            response
                .body
                .contains("Tests/CanaryTests/SluggerTests.swift")
        );
    }

    #[test]
    fn semantic_query_returns_dependency_map_for_source_path() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"dependency_map","path":"Sources/Canary/Slugger.swift"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""operation":"dependency_map""#));
        assert!(response.body.contains(r#""kind":"package_manifest""#));
        assert!(response.body.contains(r#""kind":"import""#));
        assert!(response.body.contains("import Foundation"));
        assert!(response.body.contains(r#""kind":"neighbor_source""#));
        assert!(response.body.contains("Sources/Canary/SlugRules.swift"));
        assert!(response.body.contains(r#""kind":"related_test""#));
        assert!(
            response
                .body
                .contains("Tests/CanaryTests/SluggerTests.swift")
        );
    }

    #[test]
    fn semantic_query_impact_radius_refuses_unsafe_source_path() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"impact_radius","path":"../Slugger.swift"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("safe relative source path"));
    }

    #[test]
    fn semantic_query_related_tests_refuses_unsafe_source_path() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"related_tests","path":"../Slugger.swift"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("safe relative source path"));
    }

    #[test]
    fn semantic_query_dependency_map_refuses_unsafe_source_path() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"dependency_map","path":"../Slugger.swift"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("safe relative source path"));
    }

    #[test]
    fn semantic_query_lsp_probe_is_disabled_without_env_gate() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"lsp_probe"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""operation":"lsp_probe""#));
        assert!(
            response
                .body
                .contains(r#""lsp_execution_status":"disabled""#)
        );
        assert!(response.body.contains(r#""lsp_execution_allowed":false"#));
        assert!(
            response
                .body
                .contains(r#""lsp_server_command":"sourcekit-lsp""#)
        );
    }

    #[test]
    fn semantic_query_diagnostics_is_gated_and_read_only() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"diagnostics","path":"Sources/Canary/Slugger.swift"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""operation":"diagnostics""#));
        assert!(
            response
                .body
                .contains(r#""lsp_execution_status":"disabled""#)
        );
        assert!(response.body.contains(r#""lsp_execution_allowed":false"#));
        assert!(response.body.contains(r#""provider_calls_allowed":false"#));
        assert!(
            response
                .body
                .contains(r#""production_write_allowed":false"#)
        );
    }

    #[test]
    fn semantic_query_diagnostics_refuses_unsafe_path() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"diagnostics","path":"../Slugger.swift"}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("safe relative source path"));
    }

    #[test]
    fn semantic_query_hover_is_gated_and_read_only() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"hover","path":"Sources/Canary/Slugger.swift","line":3}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"OK""#));
        assert!(response.body.contains(r#""operation":"hover""#));
        assert!(
            response
                .body
                .contains(r#""lsp_execution_status":"disabled""#)
        );
        assert!(response.body.contains(r#""lsp_execution_allowed":false"#));
        assert!(response.body.contains(r#""kind":"hover_context""#));
        assert!(response.body.contains(r#""provider_calls_allowed":false"#));
        assert!(
            response
                .body
                .contains(r#""production_write_allowed":false"#)
        );
    }

    #[test]
    fn semantic_query_hover_refuses_unsafe_path() {
        let repo = TempRepo::swift();
        let kernel = connected_kernel(&repo, "swift-canary");

        let response = handle(
            "POST",
            r#"{"repo_id":"swift-canary","operation":"hover","path":"../secret","line":1}"#,
            &kernel,
        )
        .expect("route");

        assert!(response.body.contains(r#""status":"REFUSED""#));
        assert!(response.body.contains("safe relative source path"));
    }

    fn connected_kernel(repo: &TempRepo, repo_id: &'static str) -> Arc<RwLock<MdxKernel>> {
        let kernel = Arc::new(RwLock::new(MdxKernel::boot_local()));
        kernel
            .write()
            .expect("kernel")
            .connect_forge_repo(ForgeRepoConnect {
                tenant_id: "local_tenant",
                actor_id: "human:test",
                repo_id,
                label: "Swift Canary",
                root: repo.path.to_str().unwrap(),
                kind: "local",
                origin: "",
            })
            .expect("repo connected");
        kernel
    }
}
