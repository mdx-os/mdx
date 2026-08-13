use mdx_core::json_string_literal;
use serde_json::json;
use std::collections::BTreeMap;
use std::io::Write;
use std::net::TcpStream;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

const MAX_FRAMES_PER_MISSION: usize = 200;
const HEARTBEAT_SECS: u64 = 10;

#[derive(Default)]
struct DeliberationState {
    next_seq: u64,
    frames: Vec<String>,
    subscribers: Vec<Sender<String>>,
}

static BUS: OnceLock<Mutex<BTreeMap<String, DeliberationState>>> = OnceLock::new();

fn bus() -> &'static Mutex<BTreeMap<String, DeliberationState>> {
    BUS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

#[derive(Clone, Copy)]
pub(crate) struct DeliberationModel<'a> {
    pub provider_family: &'a str,
    pub model_id: &'a str,
    pub model_class: &'a str,
}

pub(crate) struct DeliberationFrame<'a> {
    pub mission_id: &'a str,
    pub companion_id: &'a str,
    pub label: &'a str,
    pub text: &'a str,
    pub role: &'a str,
    pub final_chunk: bool,
    pub model: DeliberationModel<'a>,
}

pub(crate) fn publish_deliberation(frame: DeliberationFrame<'_>) {
    publish(
        frame.mission_id,
        "deliberation",
        json!({
            "companion_id": frame.companion_id,
            "label": frame.label,
            "text": frame.text,
            "role": frame.role,
            "final": frame.final_chunk
        }),
        &frame.model,
    );
}

pub(crate) fn publish_terminal(mission_id: &str, status: &str, model: DeliberationModel<'_>) {
    publish(
        mission_id,
        "terminal",
        json!({
            "status": status,
            "phase": "shaping",
            "production_write_allowed": false
        }),
        &model,
    );
}

fn publish(
    mission_id: &str,
    kind: &str,
    payload: serde_json::Value,
    model: &DeliberationModel<'_>,
) {
    if mission_id.trim().is_empty() {
        return;
    }
    let Ok(mut guard) = bus().lock() else {
        return;
    };
    let state = guard.entry(mission_id.to_string()).or_default();
    state.next_seq = state.next_seq.saturating_add(1);
    let frame = json!({
        "seq": state.next_seq,
        "kind": kind,
        "mission_id": mission_id,
        "phase": "shaping",
        "model": {
            "provider_family": model.provider_family,
            "model_id": model.model_id,
            "model_class": model.model_class
        },
        "payload": payload,
        "authority_opened": "none",
        "production_write_allowed": false
    })
    .to_string();
    state.frames.push(frame.clone());
    if state.frames.len() > MAX_FRAMES_PER_MISSION {
        let excess = state.frames.len() - MAX_FRAMES_PER_MISSION;
        state.frames.drain(0..excess);
    }
    state
        .subscribers
        .retain(|subscriber| subscriber.send(frame.clone()).is_ok());
}

fn subscribe(mission_id: &str) -> (Vec<String>, Receiver<String>) {
    let (sender, receiver) = std::sync::mpsc::channel();
    let Ok(mut guard) = bus().lock() else {
        return (Vec::new(), receiver);
    };
    let state = guard.entry(mission_id.to_string()).or_default();
    let backlog = state.frames.clone();
    state.subscribers.push(sender);
    (backlog, receiver)
}

fn backlog(mission_id: &str) -> Vec<String> {
    let Ok(guard) = bus().lock() else {
        return Vec::new();
    };
    guard
        .get(mission_id)
        .map(|state| state.frames.clone())
        .unwrap_or_default()
}

pub(crate) fn handle(
    mut stream: TcpStream,
    request_head: &str,
    _kernel: &Arc<RwLock<mdx_core::MdxKernel>>,
) -> Result<(), String> {
    let mission_id = query_param(request_head, "mission_id").unwrap_or_default();
    if mission_id.trim().is_empty() {
        return write_json_refusal(
            &mut stream,
            "400 Bad Request",
            "mission_id_required",
            "provide /activation/first-mission/shaping/stream?mission_id=activation_first_mission_...",
        );
    }
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(HEARTBEAT_SECS)));
    write_head(&mut stream)?;
    if query_param(request_head, "once")
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
        .unwrap_or(false)
    {
        for frame in backlog(&mission_id) {
            write_event(&mut stream, &frame)?;
        }
        return finish(&mut stream);
    }
    let (backlog, receiver) = subscribe(&mission_id);
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
                    r#"{{"kind":"heartbeat","mission_id":{},"phase":"shaping","production_write_allowed":false}}"#,
                    json_string_literal(&mission_id)
                );
                write_event(&mut stream, &heartbeat)?;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    finish(&mut stream)
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
        .map_err(|error| format!("write stream head: {error}"))
}

fn write_event(stream: &mut TcpStream, json: &str) -> Result<(), String> {
    let payload = format!("data: {json}\n\n");
    let chunk = format!("{:X}\r\n{}\r\n", payload.len(), payload);
    stream
        .write_all(chunk.as_bytes())
        .and_then(|_| stream.flush())
        .map_err(|error| format!("write stream event: {error}"))
}

fn finish(stream: &mut TcpStream) -> Result<(), String> {
    stream
        .write_all(b"0\r\n\r\n")
        .and_then(|_| stream.flush())
        .map_err(|error| format!("finish stream: {error}"))
}

fn write_json_refusal(
    stream: &mut TcpStream,
    status: &str,
    error: &str,
    detail: &str,
) -> Result<(), String> {
    let body = format!(
        r#"{{"error":{},"detail":{},"production_write_allowed":false}}"#,
        json_string_literal(error),
        json_string_literal(detail)
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
    fn deliberation_bus_replays_model_and_companion_frames() {
        let mission_id = "activation_first_mission_stream_unit";
        let model = DeliberationModel {
            provider_family: "anthropic",
            model_id: "claude-opus-4-8",
            model_class: "frontier",
        };
        publish_deliberation(DeliberationFrame {
            mission_id,
            companion_id: "twin_architect",
            label: "Architect",
            text: "Keep the first mission narrow and provable.",
            role: "voice",
            final_chunk: true,
            model,
        });
        publish_terminal(mission_id, "ready", model);

        let frames = backlog(mission_id);
        assert_eq!(frames.len(), 2);
        for expected in [
            r#""kind":"deliberation""#,
            r#""mission_id":"activation_first_mission_stream_unit""#,
            r#""companion_id":"twin_architect""#,
            r#""text":"Keep the first mission narrow and provable.""#,
            r#""model_class":"frontier""#,
        ] {
            assert!(frames[0].contains(expected), "{expected}: {}", frames[0]);
        }
        assert!(frames[1].contains(r#""kind":"terminal""#), "{}", frames[1]);
        assert!(frames[1].contains(r#""status":"ready""#), "{}", frames[1]);
    }
}
