// Gated language-server probing for Forge semantic intelligence.
//
// This is deliberately a probe, not a long-lived session pool. It proves that
// Forge can initialize an allowed language server over stdio JSON-RPC without
// giving the route write authority, provider access, or proof authority.
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const PROBE_TIMEOUT_MS: u64 = 3_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LspProbeReport {
    pub status: &'static str,
    pub command: String,
    pub reason: String,
    pub initialized: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LspDiagnostic {
    pub line: usize,
    pub column: usize,
    pub severity: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LspDiagnosticsReport {
    pub status: &'static str,
    pub command: String,
    pub reason: String,
    pub initialized: bool,
    pub diagnostics: Vec<LspDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LspHoverReport {
    pub status: &'static str,
    pub command: String,
    pub reason: String,
    pub initialized: bool,
    pub hover: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LspLocation {
    pub path: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LspLocationReport {
    pub status: &'static str,
    pub command: String,
    pub reason: String,
    pub initialized: bool,
    pub locations: Vec<LspLocation>,
}

pub(crate) fn live_lsp_enabled() -> bool {
    std::env::var("MDX_FORGE_LSP_EXEC")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

pub(crate) fn diagnostics_language_pack(
    root: &Path,
    language_pack_id: &str,
    relative_path: &str,
) -> LspDiagnosticsReport {
    let Some(spec) = spec_for_language_pack(language_pack_id) else {
        return LspDiagnosticsReport {
            status: "missing_server",
            command: String::new(),
            reason: format!("no LSP server is mapped for language pack {language_pack_id}"),
            initialized: false,
            diagnostics: Vec::new(),
        };
    };
    if !live_lsp_enabled() {
        return LspDiagnosticsReport {
            status: "disabled",
            command: spec.command.to_string(),
            reason: "set MDX_FORGE_LSP_EXEC=1 to allow bounded read-only LSP diagnostics"
                .to_string(),
            initialized: false,
            diagnostics: Vec::new(),
        };
    }
    if find_command_in_path(spec.command).is_none() {
        return LspDiagnosticsReport {
            status: "missing_server",
            command: spec.command.to_string(),
            reason: format!("{} is not available on PATH", spec.command),
            initialized: false,
            diagnostics: Vec::new(),
        };
    }
    diagnostics_command(root, spec.command, spec.args, relative_path)
}

pub(crate) fn hover_language_pack(
    root: &Path,
    language_pack_id: &str,
    relative_path: &str,
    line: usize,
) -> LspHoverReport {
    let Some(spec) = spec_for_language_pack(language_pack_id) else {
        return LspHoverReport {
            status: "missing_server",
            command: String::new(),
            reason: format!("no LSP server is mapped for language pack {language_pack_id}"),
            initialized: false,
            hover: String::new(),
        };
    };
    if !live_lsp_enabled() {
        return LspHoverReport {
            status: "disabled",
            command: spec.command.to_string(),
            reason: "set MDX_FORGE_LSP_EXEC=1 to allow bounded read-only LSP hover".to_string(),
            initialized: false,
            hover: String::new(),
        };
    }
    if find_command_in_path(spec.command).is_none() {
        return LspHoverReport {
            status: "missing_server",
            command: spec.command.to_string(),
            reason: format!("{} is not available on PATH", spec.command),
            initialized: false,
            hover: String::new(),
        };
    }
    hover_command(root, spec.command, spec.args, relative_path, line)
}

pub(crate) fn location_language_pack(
    root: &Path,
    language_pack_id: &str,
    operation: &str,
    relative_path: &str,
    line: usize,
    query: &str,
) -> LspLocationReport {
    let Some(spec) = spec_for_language_pack(language_pack_id) else {
        return LspLocationReport {
            status: "missing_server",
            command: String::new(),
            reason: format!("no LSP server is mapped for language pack {language_pack_id}"),
            initialized: false,
            locations: Vec::new(),
        };
    };
    if !live_lsp_enabled() {
        return LspLocationReport {
            status: "disabled",
            command: spec.command.to_string(),
            reason: format!("set MDX_FORGE_LSP_EXEC=1 to allow bounded read-only LSP {operation}"),
            initialized: false,
            locations: Vec::new(),
        };
    }
    if find_command_in_path(spec.command).is_none() {
        return LspLocationReport {
            status: "missing_server",
            command: spec.command.to_string(),
            reason: format!("{} is not available on PATH", spec.command),
            initialized: false,
            locations: Vec::new(),
        };
    }
    location_command(
        root,
        spec.command,
        spec.args,
        operation,
        relative_path,
        line,
        query,
    )
}

pub(crate) fn probe_language_pack(root: &Path, language_pack_id: &str) -> LspProbeReport {
    let Some(spec) = spec_for_language_pack(language_pack_id) else {
        return LspProbeReport {
            status: "missing_server",
            command: String::new(),
            reason: format!("no LSP server is mapped for language pack {language_pack_id}"),
            initialized: false,
        };
    };
    if !live_lsp_enabled() {
        return LspProbeReport {
            status: "disabled",
            command: spec.command.to_string(),
            reason: "set MDX_FORGE_LSP_EXEC=1 to allow a bounded language-server probe".to_string(),
            initialized: false,
        };
    }
    if find_command_in_path(spec.command).is_none() {
        return LspProbeReport {
            status: "missing_server",
            command: spec.command.to_string(),
            reason: format!("{} is not available on PATH", spec.command),
            initialized: false,
        };
    }
    probe_command(root, spec.command, spec.args)
}

#[derive(Clone, Copy)]
struct LspSpec {
    command: &'static str,
    args: &'static [&'static str],
}

fn spec_for_language_pack(language_pack_id: &str) -> Option<LspSpec> {
    match language_pack_id {
        "ios-xcode" | "swift-spm" => Some(LspSpec {
            command: "sourcekit-lsp",
            args: &[],
        }),
        "java-maven" => Some(LspSpec {
            command: "jdtls",
            args: &[],
        }),
        "gradle-jvm" | "android-gradle" => Some(LspSpec {
            command: "kotlin-language-server",
            args: &[],
        }),
        "dotnet" => Some(LspSpec {
            command: "omnisharp",
            args: &["--languageserver"],
        }),
        "node" => Some(LspSpec {
            command: "typescript-language-server",
            args: &["--stdio"],
        }),
        "rust-cargo" => Some(LspSpec {
            command: "rust-analyzer",
            args: &[],
        }),
        "python" => Some(LspSpec {
            command: "pyright-langserver",
            args: &["--stdio"],
        }),
        "go" => Some(LspSpec {
            command: "gopls",
            args: &[],
        }),
        _ => None,
    }
}

fn probe_command(root: &Path, command: &str, args: &[&str]) -> LspProbeReport {
    let mut child = match Command::new(command)
        .args(args)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return LspProbeReport {
                status: "failed",
                command: command.to_string(),
                reason: format!("could not start {command}: {error}"),
                initialized: false,
            };
        }
    };

    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        return LspProbeReport {
            status: "failed",
            command: command.to_string(),
            reason: "language server stdin was unavailable".to_string(),
            initialized: false,
        };
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        return LspProbeReport {
            status: "failed",
            command: command.to_string(),
            reason: "language server stdout was unavailable".to_string(),
            initialized: false,
        };
    };

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let response = read_lsp_response(stdout);
        let _ = sender.send(response);
    });

    let init = initialize_request(root);
    if let Err(error) = stdin.write_all(init.as_bytes()) {
        let _ = child.kill();
        return LspProbeReport {
            status: "failed",
            command: command.to_string(),
            reason: format!("could not send initialize request: {error}"),
            initialized: false,
        };
    }
    let _ = stdin.flush();

    match receiver.recv_timeout(Duration::from_millis(PROBE_TIMEOUT_MS)) {
        Ok(Some(response))
            if response.contains(r#""id":1"#) && response.contains(r#""result""#) =>
        {
            let _ = stdin.write_all(shutdown_request().as_bytes());
            let _ = stdin.write_all(exit_notification().as_bytes());
            let _ = stdin.flush();
            let _ = child.kill();
            let _ = child.wait();
            LspProbeReport {
                status: "initialized",
                command: command.to_string(),
                reason: "language server initialized over stdio JSON-RPC".to_string(),
                initialized: true,
            }
        }
        Ok(Some(response)) => {
            let _ = child.kill();
            let _ = child.wait();
            LspProbeReport {
                status: "failed",
                command: command.to_string(),
                reason: format!("initialize response did not include a result: {response}"),
                initialized: false,
            }
        }
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            LspProbeReport {
                status: "failed",
                command: command.to_string(),
                reason: "language server response could not be parsed".to_string(),
                initialized: false,
            }
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            LspProbeReport {
                status: "failed",
                command: command.to_string(),
                reason: "language server initialize timed out".to_string(),
                initialized: false,
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn probe_command_for_test(root: &Path, command: &str) -> LspProbeReport {
    probe_command(root, command, &[])
}

#[cfg(test)]
pub(crate) fn diagnostics_command_for_test(
    root: &Path,
    command: &str,
    relative_path: &str,
) -> LspDiagnosticsReport {
    diagnostics_command(root, command, &[], relative_path)
}

#[cfg(test)]
pub(crate) fn hover_command_for_test(
    root: &Path,
    command: &str,
    relative_path: &str,
    line: usize,
) -> LspHoverReport {
    hover_command(root, command, &[], relative_path, line)
}

#[cfg(test)]
pub(crate) fn location_command_for_test(
    root: &Path,
    command: &str,
    operation: &str,
    relative_path: &str,
    line: usize,
    query: &str,
) -> LspLocationReport {
    location_command(root, command, &[], operation, relative_path, line, query)
}

fn diagnostics_command(
    root: &Path,
    command: &str,
    args: &[&str],
    relative_path: &str,
) -> LspDiagnosticsReport {
    let file = root.join(relative_path);
    let contents = match std::fs::read_to_string(&file) {
        Ok(contents) => contents,
        Err(error) => {
            return LspDiagnosticsReport {
                status: "failed",
                command: command.to_string(),
                reason: format!("could not read diagnostic target {relative_path}: {error}"),
                initialized: false,
                diagnostics: Vec::new(),
            };
        }
    };
    let mut child = match Command::new(command)
        .args(args)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return LspDiagnosticsReport {
                status: "failed",
                command: command.to_string(),
                reason: format!("could not start {command}: {error}"),
                initialized: false,
                diagnostics: Vec::new(),
            };
        }
    };

    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        return LspDiagnosticsReport {
            status: "failed",
            command: command.to_string(),
            reason: "language server stdin was unavailable".to_string(),
            initialized: false,
            diagnostics: Vec::new(),
        };
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        return LspDiagnosticsReport {
            status: "failed",
            command: command.to_string(),
            reason: "language server stdout was unavailable".to_string(),
            initialized: false,
            diagnostics: Vec::new(),
        };
    };

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut stdout = stdout;
        while let Some(response) = read_lsp_response(&mut stdout) {
            if sender.send(response).is_err() {
                break;
            }
        }
    });

    let init = initialize_request(root);
    if let Err(error) = stdin.write_all(init.as_bytes()) {
        let _ = child.kill();
        return LspDiagnosticsReport {
            status: "failed",
            command: command.to_string(),
            reason: format!("could not send initialize request: {error}"),
            initialized: false,
            diagnostics: Vec::new(),
        };
    }
    let _ = stdin.flush();

    match receive_initialize_response(&receiver) {
        Ok(()) => {}
        Err(reason) => {
            let _ = child.kill();
            let _ = child.wait();
            return LspDiagnosticsReport {
                status: "failed",
                command: command.to_string(),
                reason,
                initialized: false,
                diagnostics: Vec::new(),
            };
        }
    }

    let uri = root.join(relative_path);
    let uri = root_uri(&uri);
    let language_id = language_id_for_path(relative_path);
    for message in [
        initialized_notification(),
        did_open_notification(&uri, language_id, &contents),
    ] {
        if let Err(error) = stdin.write_all(message.as_bytes()) {
            let _ = child.kill();
            let _ = child.wait();
            return LspDiagnosticsReport {
                status: "failed",
                command: command.to_string(),
                reason: format!("could not send diagnostics request: {error}"),
                initialized: true,
                diagnostics: Vec::new(),
            };
        }
    }
    let _ = stdin.flush();

    let diagnostics = receive_diagnostics(&receiver);
    let _ = stdin.write_all(shutdown_request().as_bytes());
    let _ = stdin.write_all(exit_notification().as_bytes());
    let _ = stdin.flush();
    let _ = child.kill();
    let _ = child.wait();

    LspDiagnosticsReport {
        status: "initialized",
        command: command.to_string(),
        reason: "language server initialized and returned diagnostics for an opened document"
            .to_string(),
        initialized: true,
        diagnostics,
    }
}

fn hover_command(
    root: &Path,
    command: &str,
    args: &[&str],
    relative_path: &str,
    line: usize,
) -> LspHoverReport {
    let file = root.join(relative_path);
    let contents = match std::fs::read_to_string(&file) {
        Ok(contents) => contents,
        Err(error) => {
            return LspHoverReport {
                status: "failed",
                command: command.to_string(),
                reason: format!("could not read hover target {relative_path}: {error}"),
                initialized: false,
                hover: String::new(),
            };
        }
    };
    let mut child = match Command::new(command)
        .args(args)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return LspHoverReport {
                status: "failed",
                command: command.to_string(),
                reason: format!("could not start {command}: {error}"),
                initialized: false,
                hover: String::new(),
            };
        }
    };

    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        return LspHoverReport {
            status: "failed",
            command: command.to_string(),
            reason: "language server stdin was unavailable".to_string(),
            initialized: false,
            hover: String::new(),
        };
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        return LspHoverReport {
            status: "failed",
            command: command.to_string(),
            reason: "language server stdout was unavailable".to_string(),
            initialized: false,
            hover: String::new(),
        };
    };

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut stdout = stdout;
        while let Some(response) = read_lsp_response(&mut stdout) {
            if sender.send(response).is_err() {
                break;
            }
        }
    });

    let init = initialize_request(root);
    if let Err(error) = stdin.write_all(init.as_bytes()) {
        let _ = child.kill();
        return LspHoverReport {
            status: "failed",
            command: command.to_string(),
            reason: format!("could not send initialize request: {error}"),
            initialized: false,
            hover: String::new(),
        };
    }
    let _ = stdin.flush();

    match receive_initialize_response(&receiver) {
        Ok(()) => {}
        Err(reason) => {
            let _ = child.kill();
            let _ = child.wait();
            return LspHoverReport {
                status: "failed",
                command: command.to_string(),
                reason,
                initialized: false,
                hover: String::new(),
            };
        }
    }

    let uri = root_uri(&root.join(relative_path));
    let language_id = language_id_for_path(relative_path);
    let hover_line = line.saturating_sub(1);
    let hover_character = hover_character_for_line(&contents, line);
    for message in [
        initialized_notification(),
        did_open_notification(&uri, language_id, &contents),
        hover_request(&uri, hover_line, hover_character),
    ] {
        if let Err(error) = stdin.write_all(message.as_bytes()) {
            let _ = child.kill();
            let _ = child.wait();
            return LspHoverReport {
                status: "failed",
                command: command.to_string(),
                reason: format!("could not send hover request: {error}"),
                initialized: true,
                hover: String::new(),
            };
        }
    }
    let _ = stdin.flush();

    let hover = receive_hover(&receiver);
    let _ = stdin.write_all(shutdown_request().as_bytes());
    let _ = stdin.write_all(exit_notification().as_bytes());
    let _ = stdin.flush();
    let _ = child.kill();
    let _ = child.wait();

    LspHoverReport {
        status: "initialized",
        command: command.to_string(),
        reason: "language server initialized and returned hover for an opened document".to_string(),
        initialized: true,
        hover,
    }
}

fn location_command(
    root: &Path,
    command: &str,
    args: &[&str],
    operation: &str,
    relative_path: &str,
    line: usize,
    query: &str,
) -> LspLocationReport {
    let file = root.join(relative_path);
    let contents = match std::fs::read_to_string(&file) {
        Ok(contents) => contents,
        Err(error) => {
            return LspLocationReport {
                status: "failed",
                command: command.to_string(),
                reason: format!("could not read {operation} target {relative_path}: {error}"),
                initialized: false,
                locations: Vec::new(),
            };
        }
    };
    let mut child = match Command::new(command)
        .args(args)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return LspLocationReport {
                status: "failed",
                command: command.to_string(),
                reason: format!("could not start {command}: {error}"),
                initialized: false,
                locations: Vec::new(),
            };
        }
    };

    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        return LspLocationReport {
            status: "failed",
            command: command.to_string(),
            reason: "language server stdin was unavailable".to_string(),
            initialized: false,
            locations: Vec::new(),
        };
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        return LspLocationReport {
            status: "failed",
            command: command.to_string(),
            reason: "language server stdout was unavailable".to_string(),
            initialized: false,
            locations: Vec::new(),
        };
    };

    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut stdout = stdout;
        while let Some(response) = read_lsp_response(&mut stdout) {
            if sender.send(response).is_err() {
                break;
            }
        }
    });

    let init = initialize_request(root);
    if let Err(error) = stdin.write_all(init.as_bytes()) {
        let _ = child.kill();
        return LspLocationReport {
            status: "failed",
            command: command.to_string(),
            reason: format!("could not send initialize request: {error}"),
            initialized: false,
            locations: Vec::new(),
        };
    }
    let _ = stdin.flush();

    match receive_initialize_response(&receiver) {
        Ok(()) => {}
        Err(reason) => {
            let _ = child.kill();
            let _ = child.wait();
            return LspLocationReport {
                status: "failed",
                command: command.to_string(),
                reason,
                initialized: false,
                locations: Vec::new(),
            };
        }
    }

    let uri = root_uri(&root.join(relative_path));
    let language_id = language_id_for_path(relative_path);
    let lsp_line = line.saturating_sub(1);
    let character = character_for_line_and_query(&contents, line, query);
    for message in [
        initialized_notification(),
        did_open_notification(&uri, language_id, &contents),
        location_request(operation, &uri, lsp_line, character),
    ] {
        if let Err(error) = stdin.write_all(message.as_bytes()) {
            let _ = child.kill();
            let _ = child.wait();
            return LspLocationReport {
                status: "failed",
                command: command.to_string(),
                reason: format!("could not send {operation} request: {error}"),
                initialized: true,
                locations: Vec::new(),
            };
        }
    }
    let _ = stdin.flush();

    let locations = receive_locations(&receiver, root);
    let _ = stdin.write_all(shutdown_request().as_bytes());
    let _ = stdin.write_all(exit_notification().as_bytes());
    let _ = stdin.flush();
    let _ = child.kill();
    let _ = child.wait();

    LspLocationReport {
        status: "initialized",
        command: command.to_string(),
        reason: format!("language server initialized and returned {operation} locations"),
        initialized: true,
        locations,
    }
}

fn receive_initialize_response(receiver: &mpsc::Receiver<String>) -> Result<(), String> {
    loop {
        let response = receiver
            .recv_timeout(Duration::from_millis(PROBE_TIMEOUT_MS))
            .map_err(|_| "language server initialize timed out".to_string())?;
        if response.contains(r#""id":1"#) && response.contains(r#""result""#) {
            return Ok(());
        }
        if response.contains(r#""id":1"#) {
            return Err(format!(
                "initialize response did not include a result: {response}"
            ));
        }
    }
}

fn receive_hover(receiver: &mpsc::Receiver<String>) -> String {
    loop {
        let Ok(response) = receiver.recv_timeout(Duration::from_millis(PROBE_TIMEOUT_MS)) else {
            return String::new();
        };
        if !response.contains(r#""id":3"#) {
            continue;
        }
        return parse_hover(&response);
    }
}

fn receive_diagnostics(receiver: &mpsc::Receiver<String>) -> Vec<LspDiagnostic> {
    loop {
        let Ok(response) = receiver.recv_timeout(Duration::from_millis(PROBE_TIMEOUT_MS)) else {
            return Vec::new();
        };
        if !response.contains("textDocument/publishDiagnostics") {
            continue;
        }
        return parse_diagnostics(&response);
    }
}

fn receive_locations(receiver: &mpsc::Receiver<String>, root: &Path) -> Vec<LspLocation> {
    loop {
        let Ok(response) = receiver.recv_timeout(Duration::from_millis(PROBE_TIMEOUT_MS)) else {
            return Vec::new();
        };
        if !response.contains(r#""id":4"#) {
            continue;
        }
        return parse_locations(&response, root);
    }
}

fn parse_hover(response: &str) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(response) else {
        return String::new();
    };
    let contents = &value["result"]["contents"];
    if let Some(text) = contents.as_str() {
        return text.trim().to_string();
    }
    if let Some(text) = contents["value"].as_str() {
        return text.trim().to_string();
    }
    if let Some(items) = contents.as_array() {
        return items
            .iter()
            .filter_map(|item| {
                item.as_str()
                    .or_else(|| item["value"].as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    String::new()
}

fn parse_locations(response: &str, root: &Path) -> Vec<LspLocation> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(response) else {
        return Vec::new();
    };
    let result = &value["result"];
    if result.is_null() {
        return Vec::new();
    }
    let root_uri = root_uri(root);
    if let Some(items) = result.as_array() {
        return items
            .iter()
            .filter_map(|item| parse_location_item(item, &root_uri))
            .take(50)
            .collect();
    }
    parse_location_item(result, &root_uri)
        .into_iter()
        .take(50)
        .collect()
}

fn parse_location_item(item: &serde_json::Value, root_uri: &str) -> Option<LspLocation> {
    let uri = item["uri"]
        .as_str()
        .or_else(|| item["targetUri"].as_str())?;
    let range = if item["range"].is_object() {
        &item["range"]
    } else {
        &item["targetRange"]
    };
    let relative_path = file_uri_to_relative_path(uri, root_uri)?;
    Some(LspLocation {
        path: relative_path,
        line: range["start"]["line"].as_u64().unwrap_or(0) as usize + 1,
        column: range["start"]["character"].as_u64().unwrap_or(0) as usize + 1,
    })
}

fn file_uri_to_relative_path(uri: &str, root_uri: &str) -> Option<String> {
    let without_root = uri.strip_prefix(root_uri)?.trim_start_matches('/');
    if without_root.is_empty() {
        return None;
    }
    Some(without_root.replace("%20", " "))
}

fn parse_diagnostics(response: &str) -> Vec<LspDiagnostic> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(response) else {
        return Vec::new();
    };
    value["params"]["diagnostics"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .take(50)
                .map(|item| LspDiagnostic {
                    line: item["range"]["start"]["line"].as_u64().unwrap_or(0) as usize + 1,
                    column: item["range"]["start"]["character"].as_u64().unwrap_or(0) as usize + 1,
                    severity: item["severity"]
                        .as_u64()
                        .map(|severity| severity.to_string())
                        .unwrap_or_else(|| "unknown".to_string()),
                    message: item["message"].as_str().unwrap_or("").to_string(),
                })
                .filter(|item| !item.message.trim().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn initialize_request(root: &Path) -> String {
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"processId":null,"rootUri":{},"capabilities":{{}}}}}}"#,
        json_string(&root_uri(root)),
    );
    framed(&body)
}

fn initialized_notification() -> String {
    framed(r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#)
}

fn did_open_notification(uri: &str, language_id: &str, text: &str) -> String {
    let body = format!(
        r#"{{"jsonrpc":"2.0","method":"textDocument/didOpen","params":{{"textDocument":{{"uri":{},"languageId":{},"version":1,"text":{}}}}}}}"#,
        json_string(uri),
        json_string(language_id),
        json_string(text),
    );
    framed(&body)
}

fn hover_request(uri: &str, line: usize, character: usize) -> String {
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":3,"method":"textDocument/hover","params":{{"textDocument":{{"uri":{}}},"position":{{"line":{},"character":{}}}}}}}"#,
        json_string(uri),
        line,
        character,
    );
    framed(&body)
}

fn location_request(operation: &str, uri: &str, line: usize, character: usize) -> String {
    let method = match operation {
        "references" => "textDocument/references",
        _ => "textDocument/definition",
    };
    let params = if operation == "references" {
        format!(
            r#"{{"textDocument":{{"uri":{}}},"position":{{"line":{},"character":{}}},"context":{{"includeDeclaration":true}}}}"#,
            json_string(uri),
            line,
            character,
        )
    } else {
        format!(
            r#"{{"textDocument":{{"uri":{}}},"position":{{"line":{},"character":{}}}}}"#,
            json_string(uri),
            line,
            character,
        )
    };
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":4,"method":{},"params":{}}}"#,
        json_string(method),
        params,
    );
    framed(&body)
}

fn shutdown_request() -> String {
    framed(r#"{"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}"#)
}

fn exit_notification() -> String {
    framed(r#"{"jsonrpc":"2.0","method":"exit","params":null}"#)
}

fn framed(body: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{}", body.len(), body)
}

fn read_lsp_response(mut stdout: impl Read) -> Option<String> {
    let mut header = Vec::new();
    let mut byte = [0_u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        if stdout.read(&mut byte).ok()? == 0 {
            return None;
        }
        header.push(byte[0]);
        if header.len() > 4096 {
            return None;
        }
    }
    let header_text = String::from_utf8_lossy(&header);
    let content_length = header_text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.eq_ignore_ascii_case("content-length") {
            value.trim().parse::<usize>().ok()
        } else {
            None
        }
    })?;
    if content_length > 256 * 1024 {
        return None;
    }
    let mut body = vec![0_u8; content_length];
    stdout.read_exact(&mut body).ok()?;
    String::from_utf8(body).ok()
}

fn root_uri(root: &Path) -> String {
    format!(
        "file://{}",
        root.to_string_lossy()
            .replace('\\', "/")
            .replace(' ', "%20")
    )
}

fn hover_character_for_line(contents: &str, line: usize) -> usize {
    contents
        .lines()
        .nth(line.saturating_sub(1))
        .and_then(|value| value.chars().position(|ch| !ch.is_whitespace()))
        .unwrap_or(0)
}

fn character_for_line_and_query(contents: &str, line: usize, query: &str) -> usize {
    let Some(value) = contents.lines().nth(line.saturating_sub(1)) else {
        return 0;
    };
    let needle = query.trim();
    if !needle.is_empty()
        && let Some(index) = value.find(needle)
    {
        return index;
    }
    value
        .chars()
        .position(|ch| !ch.is_whitespace())
        .unwrap_or(0)
}

fn language_id_for_path(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|value| value.to_str()) {
        Some("swift") => "swift",
        Some("java") => "java",
        Some("kt") | Some("kts") => "kotlin",
        Some("cs") => "csharp",
        Some("ts") | Some("tsx") => "typescript",
        Some("js") | Some("jsx") => "javascript",
        Some("rs") => "rust",
        Some("py") => "python",
        Some("go") => "go",
        _ => "plaintext",
    }
}

fn json_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn find_command_in_path(command: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn lsp_test_guard() -> std::sync::MutexGuard<'static, ()> {
        static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
        GUARD
            .get_or_init(|| Mutex::new(()))
            .lock()
            .expect("lsp test guard")
    }

    #[test]
    fn lsp_probe_initializes_fake_stdio_server() {
        let _guard = lsp_test_guard();
        let temp = TempDir::new();
        let server = temp.path.join("fake-lsp");
        std::fs::write(
            &server,
            r#"#!/bin/sh
body='{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#body}" "$body"
while read line; do
  [ "$line" = "" ] && break
done
"#,
        )
        .expect("fake server");
        let mut permissions = std::fs::metadata(&server).expect("metadata").permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o755);
            std::fs::set_permissions(&server, permissions).expect("permissions");
        }

        let report = probe_command_for_test(&temp.path, server.to_str().unwrap());

        assert_eq!(report.status, "initialized", "{}", report.reason);
        assert!(report.initialized);
    }

    #[test]
    fn lsp_diagnostics_reads_publish_diagnostics_from_fake_server() {
        let _guard = lsp_test_guard();
        let temp = TempDir::new();
        std::fs::write(temp.path.join("Thing.swift"), "let value = 1\n").expect("source");
        let server = temp.path.join("fake-diagnostic-lsp");
        std::fs::write(
            &server,
            r#"#!/bin/sh
body='{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"textDocumentSync":1}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#body}" "$body"
body='{"jsonrpc":"2.0","method":"textDocument/publishDiagnostics","params":{"uri":"file:///tmp/Thing.swift","diagnostics":[{"range":{"start":{"line":0,"character":4},"end":{"line":0,"character":9}},"severity":2,"message":"unused value"}]}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#body}" "$body"
sleep 1
"#,
        )
        .expect("fake server");
        let mut permissions = std::fs::metadata(&server).expect("metadata").permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o755);
            std::fs::set_permissions(&server, permissions).expect("permissions");
        }

        let report =
            diagnostics_command_for_test(&temp.path, server.to_str().unwrap(), "Thing.swift");

        assert_eq!(report.status, "initialized", "{}", report.reason);
        assert!(report.initialized);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].line, 1);
        assert_eq!(report.diagnostics[0].column, 5);
        assert_eq!(report.diagnostics[0].message, "unused value");
    }

    #[test]
    fn lsp_hover_reads_hover_response_from_fake_server() {
        let _guard = lsp_test_guard();
        let temp = TempDir::new();
        std::fs::write(temp.path.join("Thing.swift"), "let value = 1\n").expect("source");
        let server = temp.path.join("fake-hover-lsp");
        std::fs::write(
            &server,
            r#"#!/bin/sh
body='{"jsonrpc":"2.0","id":1,"result":{"capabilities":{"hoverProvider":true,"textDocumentSync":1}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#body}" "$body"
body='{"jsonrpc":"2.0","id":3,"result":{"contents":{"kind":"markdown","value":"`value: Int`"}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#body}" "$body"
sleep 1
"#,
        )
        .expect("fake server");
        let mut permissions = std::fs::metadata(&server).expect("metadata").permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o755);
            std::fs::set_permissions(&server, permissions).expect("permissions");
        }

        let report = hover_command_for_test(&temp.path, server.to_str().unwrap(), "Thing.swift", 1);

        assert_eq!(report.status, "initialized", "{}", report.reason);
        assert!(report.initialized);
        assert_eq!(report.hover, "`value: Int`");
    }

    #[test]
    fn lsp_location_reads_definition_from_fake_server() {
        let _guard = lsp_test_guard();
        let temp = TempDir::new();
        std::fs::create_dir_all(temp.path.join("Sources")).expect("source dir");
        std::fs::write(temp.path.join("Sources/Thing.swift"), "let value = 1\n").expect("source");
        let server = temp.path.join("fake-location-lsp");
        let target_uri = format!(
            "file://{}/Sources/Thing.swift",
            temp.path.to_string_lossy().replace('\\', "/")
        );
        std::fs::write(
            &server,
            format!(
                r#"#!/bin/sh
body='{{"jsonrpc":"2.0","id":1,"result":{{"capabilities":{{"definitionProvider":true,"referencesProvider":true,"textDocumentSync":1}}}}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${{#body}}" "$body"
body='{{"jsonrpc":"2.0","id":4,"result":[{{"uri":"{}","range":{{"start":{{"line":0,"character":4}},"end":{{"line":0,"character":9}}}}}}]}}'
printf 'Content-Length: %s\r\n\r\n%s' "${{#body}}" "$body"
sleep 1
"#,
                target_uri
            ),
        )
        .expect("fake server");
        let mut permissions = std::fs::metadata(&server).expect("metadata").permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o755);
            std::fs::set_permissions(&server, permissions).expect("permissions");
        }

        let definition = location_command_for_test(
            &temp.path,
            server.to_str().unwrap(),
            "definition",
            "Sources/Thing.swift",
            1,
            "value",
        );

        assert_eq!(definition.status, "initialized", "{}", definition.reason);
        assert!(definition.initialized);
        assert_eq!(definition.locations.len(), 1);
        assert_eq!(definition.locations[0].path, "Sources/Thing.swift");
        assert_eq!(definition.locations[0].line, 1);
        assert_eq!(definition.locations[0].column, 5);
    }

    struct TempDir {
        path: std::path::PathBuf,
    }

    impl TempDir {
        fn new() -> Self {
            let unique = format!(
                "mdx-forge-lsp-probe-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unique);
            std::fs::create_dir_all(&path).expect("temp dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
