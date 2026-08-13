use mdx_core::{MdxKernel, json_string_literal};
use serde_json::json;
use std::collections::BTreeMap;
use std::io::Write;
use std::net::TcpStream;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

const MAX_FRAMES_PER_RUN: usize = 500;
const HEARTBEAT_SECS: u64 = 10;

#[derive(Default)]
struct RunStreamState {
    next_seq: u64,
    frames: Vec<String>,
    subscribers: Vec<Sender<String>>,
    model: Option<OwnedRunStreamModel>,
    tenant_id: Option<String>,
}

#[derive(Clone)]
struct OwnedRunStreamModel {
    provider_family: String,
    model_id: String,
    slot: String,
    model_class: String,
}

impl OwnedRunStreamModel {
    fn from_ref(model: &RunStreamModel<'_>) -> Self {
        Self {
            provider_family: model.provider_family.to_string(),
            model_id: model.model_id.to_string(),
            slot: model.slot.to_string(),
            model_class: model.model_class.to_string(),
        }
    }

    fn is_resolved(&self) -> bool {
        !self.provider_family.trim().is_empty() || !self.model_id.trim().is_empty()
    }
}

static BUS: OnceLock<Mutex<BTreeMap<String, RunStreamState>>> = OnceLock::new();

fn bus() -> &'static Mutex<BTreeMap<String, RunStreamState>> {
    BUS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(crate) fn model_class(provider_family: &str, slot: &str, model_id: &str) -> &'static str {
    let provider = provider_family.to_ascii_lowercase();
    let slot = slot.to_ascii_lowercase();
    let model = model_id.to_ascii_lowercase();
    if provider.contains("ollama")
        || provider.contains("local")
        || slot.contains("local")
        || model.contains("qwen")
        || model.contains("llama")
    {
        "local_fallback"
    } else if model.contains("opus")
        || model.contains("gpt-5")
        || model.contains("gpt5")
        || slot.contains("opus")
        || slot.contains("gpt")
        || slot.contains("codex")
    {
        "frontier_strong"
    } else if provider.contains("xai")
        || model.contains("grok")
        || model.contains("sonnet")
        || model.contains("gemini")
        || slot.contains("grok")
        || slot.contains("xai")
    {
        "frontier_fast"
    } else if provider.trim().is_empty() && slot.trim().is_empty() && model.trim().is_empty() {
        "unknown"
    } else {
        "frontier"
    }
}

pub(crate) struct RunStreamModel<'a> {
    pub provider_family: &'a str,
    pub model_id: &'a str,
    pub slot: &'a str,
    pub model_class: &'a str,
}

impl<'a> RunStreamModel<'a> {
    pub(crate) fn from_slot(slot: &'a str, complexity_tier: &'a str) -> Self {
        let class = if slot.trim().is_empty() {
            "unknown"
        } else if complexity_tier.eq_ignore_ascii_case("large")
            || complexity_tier.eq_ignore_ascii_case("xl")
            || complexity_tier.eq_ignore_ascii_case("extreme")
        {
            "frontier_strong"
        } else {
            model_class("", slot, "")
        };
        Self {
            provider_family: "",
            model_id: "",
            slot,
            model_class: class,
        }
    }
}

pub(crate) fn publish_run_started(
    tenant_id: &str,
    run_id: &str,
    receipt_id: &str,
    model: &RunStreamModel<'_>,
    detail: &str,
) {
    remember_run_tenant(run_id, tenant_id);
    publish(
        run_id,
        "stage_start",
        "reading",
        receipt_id,
        model,
        json!({
            "source_event": "run_started",
            "detail": detail,
            "production_write_allowed": false
        }),
    );
}

pub(crate) fn publish_forge_run_event(
    run_id: &str,
    source_event: &str,
    detail: &str,
    turn: u32,
    receipt_id: &str,
    model: &RunStreamModel<'_>,
) {
    let (kind, stage) = event_kind_and_stage(source_event, detail);
    let summary = human_event_summary(source_event, detail);
    let mut payload = json!({
        "source_event": source_event,
        "detail": detail,
        "summary": summary,
        "turn": turn,
        "production_write_allowed": false
    });
    if matches!(kind, "proof_output") {
        payload["text"] = json!(detail);
        payload["output"] = json!(detail);
    }
    if matches!(kind, "diff_ready")
        && let Some(branch) = detail_field(detail, "branch")
    {
        payload["branch"] = json!(branch);
    }
    publish(run_id, kind, stage, receipt_id, model, payload);
}

pub(crate) fn publish_token_delta(
    run_id: &str,
    stage: &str,
    turn: u32,
    text: &str,
    model: &RunStreamModel<'_>,
) {
    if text.is_empty() {
        return;
    }
    let text = text.chars().take(4000).collect::<String>();
    let stage = normalize_stage(stage);
    publish(
        run_id,
        "token_delta",
        &stage,
        "",
        model,
        json!({
            "text": text,
            "turn": turn,
            "source_event": "model_stream",
            "production_write_allowed": false
        }),
    );
}

pub(crate) fn publish_tool_call(
    run_id: &str,
    turn: u32,
    tool_name: &str,
    summary: &str,
    model: &RunStreamModel<'_>,
) {
    publish(
        run_id,
        "tool_call",
        tool_call_stage(tool_name),
        "",
        model,
        json!({
            "name": tool_name,
            "summary": summary,
            "turn": turn,
            "source_event": "tool_call",
            "production_write_allowed": false
        }),
    );
}

pub(crate) fn publish_worker_thought(
    run_id: &str,
    stage: &str,
    turn: u32,
    text: &str,
    model: &RunStreamModel<'_>,
) {
    publish_token_delta(run_id, stage, turn, text, model);
}

pub(crate) fn event_kind_and_stage(
    source_event: &str,
    detail: &str,
) -> (&'static str, &'static str) {
    match source_event {
        "run_started" => ("stage_start", "reading"),
        "model_called" => ("stage_start", "planning"),
        "agent_turn_prose" => ("event", "planning"),
        "turn_executed" => ("event", "planning"),
        "tool_executed" => ("tool_result", tool_stage(detail)),
        "check_started" => ("proof_started", "proof"),
        "check_passed" | "check_failed" => ("proof_output", "proof"),
        "run_recovery_pending" => ("evidence", "done"),
        "run_finished" => ("terminal", "done"),
        "evidence_appended" if detail.contains("branch=") || detail.contains("diff") => {
            ("diff_ready", "done")
        }
        "evidence_appended" => ("evidence", "done"),
        _ => ("event", "planning"),
    }
}

fn tool_stage(detail: &str) -> &'static str {
    let lower = detail.to_ascii_lowercase();
    if lower.starts_with("read")
        || lower.starts_with("list")
        || lower.starts_with("search")
        || lower.contains("semantic")
    {
        "reading"
    } else if lower.contains("patch")
        || lower.contains("edit")
        || lower.contains("write")
        || lower.contains("create")
    {
        "writing"
    } else if lower.contains("run_command") || lower.contains("check_") {
        "proof"
    } else {
        "planning"
    }
}

fn tool_call_stage(tool_name: &str) -> &'static str {
    match tool_name {
        "read_file" | "list_dir" | "search" | "semantic_query" => "reading",
        "edit_file" | "write_file" => "writing",
        "run_command" => "proof",
        "finish" => "done",
        _ => "planning",
    }
}

fn normalize_stage(stage: &str) -> String {
    match stage {
        "admit" | "read" | "reading" => "reading",
        "plan" | "planning" | "work" => "planning",
        "write" | "writing" => "writing",
        "prove" | "proof" => "proof",
        "review" | "done" | "terminal" => "done",
        other => return other.to_string(),
    }
    .to_string()
}

pub(crate) fn human_event_summary(source_event: &str, detail: &str) -> String {
    match source_event {
        "run_started" => "Accepted the work in an isolated copy".to_string(),
        "model_called" => {
            let model =
                detail_field(detail, "model").unwrap_or_else(|| "the selected model".to_string());
            format!("Asked {model} for the next move")
        }
        "agent_turn_prose" => detail.to_string(),
        "turn_executed" => detail.to_string(),
        "tool_executed" => human_tool_summary(detail),
        "check_started" => human_check_started_summary(detail),
        "check_passed" => "Proof check passed".to_string(),
        "check_failed" => "Proof check failed".to_string(),
        "run_recovery_pending" => "Run was interrupted and is ready for recovery".to_string(),
        "run_finished" => human_terminal_summary(detail),
        "evidence_appended" if detail.contains("branch=") || detail.contains("diff") => {
            "Prepared the diff evidence".to_string()
        }
        "evidence_appended" => detail.to_string(),
        _ => detail.to_string(),
    }
}

fn human_check_started_summary(detail: &str) -> String {
    if let Some(command) = detail.strip_prefix("baseline_run_command_started ") {
        return format!("Running baseline proof: {command}");
    }
    if let Some(command) = detail.strip_prefix("run_command_started ") {
        return format!("Running proof: {command}");
    }
    "Running proof".to_string()
}

fn human_terminal_summary(detail: &str) -> String {
    if let Some(status) = detail_field(detail, "status") {
        return format!("Finished with {status}");
    }
    "Reached a terminal run receipt".to_string()
}

fn human_tool_summary(detail: &str) -> String {
    let mut parts = detail.split_whitespace();
    let Some(tool) = parts.next() else {
        return "Used a tool".to_string();
    };
    let target = parts.next().unwrap_or("").trim();
    match tool {
        "read_file" if !target.is_empty() => format!("Read {target}"),
        "list_dir" if !target.is_empty() => format!("Listed {target}"),
        "search" => format!("Searched for {}", target.trim_matches('"')),
        "semantic_query" => "Asked the semantic index for orientation".to_string(),
        "edit_file" if !target.is_empty() => format!("Edited {target}"),
        "write_file" if !target.is_empty() => format!("Wrote {target}"),
        "run_command" => {
            let command = detail
                .strip_prefix("run_command ")
                .unwrap_or(detail)
                .split(" exit=")
                .next()
                .unwrap_or("")
                .trim();
            if command.is_empty() {
                "Ran the selected proof command".to_string()
            } else {
                format!("Ran {command}")
            }
        }
        _ => detail.to_string(),
    }
}

fn detail_field(detail: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    detail.split_whitespace().find_map(|part| {
        part.strip_prefix(&prefix)
            .map(|value| value.trim_matches(',').to_string())
            .filter(|value| !value.is_empty())
    })
}

fn publish(
    run_id: &str,
    kind: &str,
    stage: &str,
    receipt_id: &str,
    model: &RunStreamModel<'_>,
    payload: serde_json::Value,
) {
    if run_id.trim().is_empty() {
        return;
    }
    let Ok(mut guard) = bus().lock() else {
        return;
    };
    let state = guard.entry(run_id.to_string()).or_default();
    state.next_seq = state.next_seq.saturating_add(1);
    let candidate_model = OwnedRunStreamModel::from_ref(model);
    if candidate_model.is_resolved() {
        state.model = Some(candidate_model.clone());
    }
    let frame_model = if candidate_model.is_resolved() {
        candidate_model
    } else {
        state.model.clone().unwrap_or(candidate_model)
    };
    let frame = json!({
        "seq": state.next_seq,
        "kind": kind,
        "run_id": run_id,
        "stage": stage,
        "receipt_id": receipt_id,
        "receipt_route": if receipt_id.trim().is_empty() {
            serde_json::Value::Null
        } else {
            json!(format!("/receipts/{receipt_id}"))
        },
        "model": {
            "provider_family": frame_model.provider_family,
            "model_id": frame_model.model_id,
            "slot": frame_model.slot,
            "model_class": frame_model.model_class
        },
        "payload": payload,
        "authority_opened": "none",
        "production_write_allowed": false
    })
    .to_string();
    if let (Some(tenant_id), Some(mobile_sequence)) = (
        state.tenant_id.as_deref(),
        durable_mobile_sequence(receipt_id),
    ) {
        let summary = payload
            .get("summary")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| safe_mobile_fallback(kind));
        let terminal_status = mobile_terminal_status(&payload);
        let target_kind = std::env::var("MDX_FORGE_EXECUTION_TARGET_KIND")
            .unwrap_or_else(|_| "paired_host".to_string());
        let target_id = std::env::var("MDX_FORGE_EXECUTION_TARGET_ID")
            .unwrap_or_else(|_| "local_host".to_string());
        if let Some(envelope) = mdx_mobile_relay::MobileRelayEnvelope::forge_event(
            tenant_id,
            run_id,
            mobile_sequence,
            kind,
            stage,
            terminal_status.as_deref(),
            summary,
            receipt_id,
            &target_kind,
            &target_id,
        ) {
            let _ = mdx_mobile_relay::publish(&envelope);
        }
    }
    state.frames.push(frame.clone());
    if state.frames.len() > MAX_FRAMES_PER_RUN {
        let excess = state.frames.len() - MAX_FRAMES_PER_RUN;
        state.frames.drain(0..excess);
    }
    state
        .subscribers
        .retain(|subscriber| subscriber.send(frame.clone()).is_ok());
}

fn durable_mobile_sequence(receipt_id: &str) -> Option<u64> {
    receipt_id
        .rsplit('_')
        .next()
        .filter(|suffix| !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit()))
        .and_then(|suffix| suffix.parse().ok())
        .filter(|sequence| *sequence > 0)
}

fn mobile_terminal_status(payload: &serde_json::Value) -> Option<String> {
    if payload
        .get("source_event")
        .and_then(serde_json::Value::as_str)
        != Some("run_finished")
    {
        return None;
    }
    payload
        .get("detail")
        .and_then(serde_json::Value::as_str)
        .and_then(|detail| detail_field(detail, "status"))
}

fn remember_run_tenant(run_id: &str, tenant_id: &str) {
    if run_id.trim().is_empty() || tenant_id.trim().is_empty() {
        return;
    }
    if let Ok(mut guard) = bus().lock() {
        guard.entry(run_id.to_string()).or_default().tenant_id = Some(tenant_id.to_string());
    }
}

fn safe_mobile_fallback(kind: &str) -> &'static str {
    match kind {
        "token_delta" => "Forge is working through the current step",
        "tool_call" => "Forge selected the next governed tool",
        "tool_result" => "Forge completed a governed tool step",
        "proof_started" => "Forge started the selected proof",
        "proof_output" => "Forge updated the proof result",
        "diff_ready" => "The focused diff is ready to review",
        "terminal" => "Forge reached a terminal state",
        _ => "Forge updated this build",
    }
}

fn subscribe(run_id: &str) -> (Vec<String>, Receiver<String>) {
    let (sender, receiver) = std::sync::mpsc::channel();
    let Ok(mut guard) = bus().lock() else {
        return (Vec::new(), receiver);
    };
    let state = guard.entry(run_id.to_string()).or_default();
    let backlog = state.frames.clone();
    state.subscribers.push(sender);
    (backlog, receiver)
}

fn backlog(run_id: &str) -> Vec<String> {
    let Ok(guard) = bus().lock() else {
        return Vec::new();
    };
    guard
        .get(run_id)
        .map(|state| state.frames.clone())
        .unwrap_or_default()
}

pub(crate) fn handle(
    mut stream: TcpStream,
    request_head: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<(), String> {
    let run_id = query_param(request_head, "run_id").unwrap_or_default();
    if run_id.trim().is_empty() {
        return write_json_refusal(
            &mut stream,
            "400 Bad Request",
            "run_id_required",
            "provide /forge/runs/stream?run_id=forge_run_...",
        );
    }
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(HEARTBEAT_SECS)));
    hydrate_backlog_from_receipts(&run_id, kernel);
    write_head(&mut stream)?;
    if query_param(request_head, "once")
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
    {
        for frame in backlog(&run_id) {
            write_event(&mut stream, &frame)?;
        }
        return finish(&mut stream);
    }
    let (backlog, receiver) = subscribe(&run_id);
    for frame in backlog {
        write_event(&mut stream, &frame)?;
    }
    loop {
        match receiver.recv_timeout(Duration::from_secs(HEARTBEAT_SECS)) {
            Ok(frame) => {
                let terminal = frame.contains(r#""kind":"terminal""#);
                write_event(&mut stream, &frame)?;
                if terminal {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                let heartbeat = format!(
                    r#"{{"kind":"heartbeat","run_id":{},"production_write_allowed":false}}"#,
                    json_string_literal(&run_id)
                );
                write_event(&mut stream, &heartbeat)?;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    finish(&mut stream)
}

#[derive(Clone)]
struct ReplayedRunReceipt {
    tenant_id: String,
    receipt_id: String,
    event: String,
    detail: String,
    turn: u32,
    provider_family: String,
    model_id: String,
    slot: String,
    model_class: String,
}

fn hydrate_backlog_from_receipts(run_id: &str, kernel: &Arc<RwLock<MdxKernel>>) {
    if !backlog(run_id).is_empty() {
        return;
    }
    let receipts = {
        let Ok(kernel) = kernel.read() else {
            return;
        };
        kernel
            .ledger()
            .entries()
            .iter()
            .filter(|receipt| {
                receipt.kind == "forge.run.event"
                    && receipt.payload.get("run_id").map(String::as_str) == Some(run_id)
            })
            .map(|receipt| ReplayedRunReceipt {
                tenant_id: receipt.tenant_id.as_str().to_string(),
                receipt_id: receipt.receipt_id.clone(),
                event: receipt_payload(receipt, "event").to_string(),
                detail: receipt_payload(receipt, "detail").to_string(),
                turn: receipt_payload(receipt, "turn").parse().unwrap_or(0),
                provider_family: receipt_payload(
                    receipt,
                    "builder_casting_selected_provider_family",
                )
                .to_string(),
                model_id: receipt_payload(receipt, "builder_casting_selected_model_id").to_string(),
                slot: receipt_payload(receipt, "builder_casting_selected_slot").to_string(),
                model_class: receipt_payload(receipt, "builder_casting_selected_model_class")
                    .to_string(),
            })
            .collect::<Vec<_>>()
    };
    for receipt in receipts {
        remember_run_tenant(run_id, &receipt.tenant_id);
        let provider_family = receipt.provider_family.as_str();
        let model_id = receipt.model_id.as_str();
        let slot = receipt.slot.as_str();
        let model_class = if receipt.model_class.is_empty() {
            model_class(provider_family, slot, model_id)
        } else {
            receipt.model_class.as_str()
        };
        publish_forge_run_event(
            run_id,
            &receipt.event,
            &receipt.detail,
            receipt.turn,
            &receipt.receipt_id,
            &RunStreamModel {
                provider_family,
                model_id,
                slot,
                model_class,
            },
        );
    }
}

fn receipt_payload<'a>(receipt: &'a mdx_core::Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn query_param(request_head: &str, name: &str) -> Option<String> {
    let path = request_head
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)
        .unwrap_or("/");
    let query = path.split_once('?')?.1;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == name {
            return Some(percent_decode(value));
        }
    }
    None
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex) = u8::from_str_radix(&value[i + 1..i + 3], 16)
        {
            out.push(hex);
            i += 3;
            continue;
        }
        out.push(if bytes[i] == b'+' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn write_head(stream: &mut TcpStream) -> Result<(), String> {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache, no-transform\r\nX-Accel-Buffering: no\r\n{}\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n",
        crate::LOCAL_CORS_HEADERS
    );
    stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.flush())
        .map_err(|error| format!("write head: {error}"))
}

fn write_event(stream: &mut TcpStream, json: &str) -> Result<(), String> {
    let frame = format!("data: {json}\n\n");
    let chunk = format!("{:x}\r\n{frame}\r\n", frame.len());
    stream
        .write_all(chunk.as_bytes())
        .and_then(|_| stream.flush())
        .map_err(|error| format!("write event: {error}"))
}

fn finish(stream: &mut TcpStream) -> Result<(), String> {
    stream
        .write_all(b"0\r\n\r\n")
        .and_then(|_| stream.flush())
        .map_err(|error| format!("write end: {error}"))
}

fn write_json_refusal(
    stream: &mut TcpStream,
    status: &str,
    code: &str,
    message: &str,
) -> Result<(), String> {
    let body = format!(
        r#"{{"error":{},"message":{},"production_write_allowed":false}}"#,
        json_string_literal(code),
        json_string_literal(message)
    );
    let wire = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(wire.as_bytes())
        .map_err(|error| format!("write refusal: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publishes_and_replays_run_frames_in_order() {
        let run_id = "forge_run_stream_test";
        let model = RunStreamModel {
            provider_family: "xai",
            model_id: "grok-4.3",
            slot: "GROK",
            model_class: "frontier_fast",
        };
        publish_forge_run_event(run_id, "model_called", "model=grok-4.3", 1, "r1", &model);
        publish_forge_run_event(
            run_id,
            "run_finished",
            "status=RUN_FINISHED_DONE",
            2,
            "r2",
            &model,
        );
        let (backlog, _receiver) = subscribe(run_id);
        assert!(backlog[0].contains(r#""seq":1"#));
        assert!(backlog[0].contains(r#""receipt_route":"/receipts/r1""#));
        assert!(backlog[0].contains(r#""kind":"stage_start""#));
        assert!(backlog[1].contains(r#""kind":"terminal""#));
    }

    #[test]
    fn later_frames_keep_the_resolved_run_model() {
        let run_id = "forge_run_stream_model_memory_test";
        publish_run_started(
            "local_tenant",
            run_id,
            "r0",
            &RunStreamModel {
                provider_family: "xai",
                model_id: "grok-4.3",
                slot: "GROK",
                model_class: "frontier_fast",
            },
            "accepted",
        );
        let unresolved = RunStreamModel::from_slot("", "small");
        publish_forge_run_event(
            run_id,
            "tool_executed",
            "write_file docs/DEV-101.md",
            1,
            "r1",
            &unresolved,
        );

        let (backlog, _receiver) = subscribe(run_id);
        assert!(backlog[1].contains(r#""provider_family":"xai""#));
        assert!(backlog[1].contains(r#""model_id":"grok-4.3""#));
        assert!(backlog[1].contains(r#""model_class":"frontier_fast""#));
    }

    #[test]
    fn emits_ui_stage_keys_and_live_feed_payloads() {
        let run_id = "forge_run_stream_ui_stage_test";
        let model = RunStreamModel {
            provider_family: "anthropic",
            model_id: "claude-opus-4.8",
            slot: "OPUS",
            model_class: "frontier_strong",
        };
        publish_run_started("local_tenant", run_id, "r0", &model, "accepted");
        publish_token_delta(run_id, "plan", 1, "I will inspect the scoped file.", &model);
        publish_tool_call(run_id, 1, "read_file", "Read docs/DEV-101.md", &model);
        publish_forge_run_event(
            run_id,
            "tool_executed",
            "edit_file docs/DEV-101.md",
            1,
            "r1",
            &model,
        );
        publish_forge_run_event(
            run_id,
            "check_passed",
            "run_command make agent-working-contract-check exit=0",
            1,
            "r2",
            &model,
        );
        publish_forge_run_event(
            run_id,
            "run_finished",
            "status=RUN_FINISHED_DONE",
            2,
            "r3",
            &model,
        );

        let frames = backlog(run_id).join("\n");
        assert!(frames.contains(r#""stage":"reading""#));
        assert!(frames.contains(r#""stage":"planning""#));
        assert!(frames.contains(r#""stage":"writing""#));
        assert!(frames.contains(r#""stage":"proof""#));
        assert!(frames.contains(r#""stage":"done""#));
        assert!(frames.contains(r#""kind":"token_delta""#));
        assert!(frames.contains(r#""text":"I will inspect the scoped file.""#));
        assert!(frames.contains(r#""kind":"tool_call""#));
        assert!(frames.contains(r#""summary":"Read docs/DEV-101.md""#));
        assert!(frames.contains(r#""kind":"proof_output""#));
        assert!(
            frames.contains(r#""output":"run_command make agent-working-contract-check exit=0""#)
        );
    }

    #[test]
    fn proof_started_events_are_human_readable() {
        assert_eq!(
            event_kind_and_stage("check_started", "run_command_started cargo test"),
            ("proof_started", "proof")
        );
        assert_eq!(
            human_event_summary("check_started", "baseline_run_command_started cargo test"),
            "Running baseline proof: cargo test"
        );
        assert_eq!(
            human_event_summary("check_started", "run_command_started cargo test"),
            "Running proof: cargo test"
        );
    }

    #[test]
    fn recovery_pending_event_closes_the_live_stage_without_claiming_completion() {
        assert_eq!(
            event_kind_and_stage("run_recovery_pending", "interrupted after tool_executed"),
            ("evidence", "done")
        );
        assert_eq!(
            human_event_summary("run_recovery_pending", "interrupted"),
            "Run was interrupted and is ready for recovery"
        );
    }

    #[test]
    fn model_class_marks_local_and_strong_frontier_models() {
        assert_eq!(
            model_class("ollama", "LOCAL", "qwen2.5-coder"),
            "local_fallback"
        );
        assert_eq!(
            model_class("anthropic", "OPUS", "claude-opus-4.8"),
            "frontier_strong"
        );
        assert_eq!(model_class("xai", "GROK", "grok-4.3"), "frontier_fast");
    }

    #[test]
    fn mobile_sequence_is_stable_from_the_durable_receipt() {
        assert_eq!(
            durable_mobile_sequence("forge_run_event_receipt_000143"),
            Some(143)
        );
        assert_eq!(durable_mobile_sequence(""), None);
        assert_eq!(durable_mobile_sequence("transient-frame"), None);
    }

    #[test]
    fn mobile_terminal_status_comes_only_from_structured_run_completion() {
        assert_eq!(
            mobile_terminal_status(&json!({
                "source_event": "run_finished",
                "detail": "status=RUN_FINISHED_DONE turns=2"
            })),
            Some("RUN_FINISHED_DONE".to_string())
        );
        assert_eq!(
            mobile_terminal_status(&json!({
                "source_event": "agent_turn_prose",
                "detail": "status=RUN_FINISHED_DONE"
            })),
            None
        );
        assert_eq!(
            mobile_terminal_status(&json!({"source_event": "run_finished"})),
            None
        );
    }
}
