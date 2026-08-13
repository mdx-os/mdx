//! Bounded, non-blocking OTLP export for the serving path.
//!
//! Requests enqueue presence-safe spans into one fixed-capacity worker. A slow
//! or unavailable collector increments an honest failure counter but never
//! blocks product work. Production boot still fails closed when the configured
//! endpoint itself is invalid.

use mdx_core::DeploymentMode;
use mdx_observability::{AttributeValue, OtlpHttpJsonExporter, OtlpSpan};
use ring::rand::{SecureRandom, SystemRandom};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const QUEUE_CAPACITY: usize = 1024;
const BATCH_SIZE: usize = 32;
const OUTCOME_UNOBSERVED: u8 = 0;
const OUTCOME_ACCEPTED: u8 = 1;
const OUTCOME_FAILED: u8 = 2;

#[derive(Debug, PartialEq, Eq)]
struct UpstreamTrace {
    trace_id: [u8; 16],
    parent_span_id: [u8; 8],
    flags: u8,
}

struct TraceSpanIds {
    trace_id: [u8; 16],
    span_id: [u8; 8],
    parent_span_id: Option<[u8; 8]>,
    flags: u8,
}

struct Counters {
    enqueued: AtomicU64,
    exported: AtomicU64,
    failed: AtomicU64,
    dropped: AtomicU64,
    warnings: AtomicU64,
    last_outcome: AtomicU8,
    last_failure_category: Mutex<Option<&'static str>>,
}

impl Default for Counters {
    fn default() -> Self {
        Self {
            enqueued: AtomicU64::new(0),
            exported: AtomicU64::new(0),
            failed: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            warnings: AtomicU64::new(0),
            last_outcome: AtomicU8::new(OUTCOME_UNOBSERVED),
            last_failure_category: Mutex::new(None),
        }
    }
}

struct Runtime {
    sender: SyncSender<OtlpSpan>,
    counters: Arc<Counters>,
}

static RUNTIME: OnceLock<Runtime> = OnceLock::new();

pub(crate) fn initialize(mode: DeploymentMode) -> Result<(), String> {
    let endpoint = std::env::var("MDX_TELEMETRY_ENDPOINT").unwrap_or_default();
    if endpoint.trim().is_empty() {
        if mode == DeploymentMode::Production {
            return Err("production OTLP endpoint is required".to_string());
        }
        return Ok(());
    }
    let headers = std::env::var("OTEL_EXPORTER_OTLP_HEADERS").ok();
    let environment = std::env::var("MDX_ENV").unwrap_or_else(|_| mode.as_str().to_string());
    let exporter = OtlpHttpJsonExporter::connect(
        Some(endpoint.trim()),
        headers.as_deref(),
        "mdx-server",
        &environment,
    )
    .map_err(|error| format!("OTLP configuration refused: {error}"))?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(QUEUE_CAPACITY);
    let counters = Arc::new(Counters::default());
    let worker_counters = Arc::clone(&counters);
    std::thread::Builder::new()
        .name("mdx-otlp-export".to_string())
        .spawn(move || export_worker(receiver, exporter, worker_counters))
        .map_err(|_| "OTLP export worker could not start".to_string())?;
    RUNTIME
        .set(Runtime { sender, counters })
        .map_err(|_| "OTLP export runtime was initialized more than once".to_string())?;
    Ok(())
}

fn export_worker(
    receiver: Receiver<OtlpSpan>,
    exporter: OtlpHttpJsonExporter,
    counters: Arc<Counters>,
) {
    while let Ok(first) = receiver.recv() {
        let mut batch = Vec::with_capacity(BATCH_SIZE);
        batch.push(first);
        while batch.len() < BATCH_SIZE {
            match receiver.try_recv() {
                Ok(span) => batch.push(span),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        match exporter.export(&batch) {
            Ok(report) => {
                counters
                    .exported
                    .fetch_add(report.accepted_spans as u64, Ordering::Relaxed);
                if report.warning_present {
                    counters.warnings.fetch_add(1, Ordering::Relaxed);
                }
                counters
                    .last_outcome
                    .store(OUTCOME_ACCEPTED, Ordering::Relaxed);
                if let Ok(mut category) = counters.last_failure_category.lock() {
                    *category = None;
                }
            }
            Err(error) => {
                let prior_failed = counters
                    .failed
                    .fetch_add(batch.len() as u64, Ordering::Relaxed);
                if let Ok(mut category) = counters.last_failure_category.lock() {
                    *category = Some(error.category());
                }
                counters
                    .last_outcome
                    .store(OUTCOME_FAILED, Ordering::Relaxed);
                if prior_failed == 0
                    || prior_failed / 1024 != prior_failed.saturating_add(batch.len() as u64) / 1024
                {
                    eprintln!("mdx-server OTLP export degraded: {}", error.category());
                }
            }
        }
    }
}

pub(crate) fn record_http(
    method: &str,
    path: &str,
    status: &str,
    elapsed_micros: u64,
    raw_request: &str,
    identity: Option<(&str, &str)>,
) {
    let Some(runtime) = RUNTIME.get() else {
        return;
    };
    let Some(TraceSpanIds {
        trace_id,
        span_id,
        parent_span_id,
        flags,
    }) = trace_and_span_ids(raw_request)
    else {
        runtime.counters.dropped.fetch_add(1, Ordering::Relaxed);
        return;
    };
    let status_code = status
        .get(..3)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    let surface = crate::http_telemetry::surface_of(path);
    let method = normalized_method(method);
    let end_time_unix_nano = now_unix_nanos();
    let start_time_unix_nano =
        end_time_unix_nano.saturating_sub(elapsed_micros.saturating_mul(1000));
    let mut attributes = BTreeMap::from([
        (
            "http.request.method".to_string(),
            AttributeValue::String(method.to_string()),
        ),
        (
            "http.response.status_code".to_string(),
            AttributeValue::Integer(status_code),
        ),
        (
            "mdx.surface".to_string(),
            AttributeValue::String(surface.clone()),
        ),
        (
            "mdx.telemetry.content_recorded".to_string(),
            AttributeValue::Boolean(false),
        ),
    ]);
    if let Some((tenant_id, actor_id)) = identity {
        attributes.insert(
            "mdx.tenant_id".to_string(),
            AttributeValue::String(tenant_id.to_string()),
        );
        attributes.insert(
            "mdx.actor_id".to_string(),
            AttributeValue::String(actor_id.to_string()),
        );
    }
    let span = OtlpSpan {
        trace_id,
        span_id,
        parent_span_id,
        flags: u32::from(flags),
        name: format!("HTTP {method} {surface}"),
        start_time_unix_nano,
        end_time_unix_nano,
        attributes,
        error: status_code >= 500,
    };
    match runtime.sender.try_send(span) {
        Ok(()) => {
            runtime.counters.enqueued.fetch_add(1, Ordering::Relaxed);
        }
        Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
            runtime.counters.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn normalized_method(method: &str) -> &'static str {
    match method {
        "GET" => "GET",
        "HEAD" => "HEAD",
        "POST" => "POST",
        "PUT" => "PUT",
        "PATCH" => "PATCH",
        "DELETE" => "DELETE",
        "OPTIONS" => "OPTIONS",
        "CONNECT" => "CONNECT",
        "TRACE" => "TRACE",
        _ => "OTHER",
    }
}

fn trace_and_span_ids(raw_request: &str) -> Option<TraceSpanIds> {
    let upstream = traceparent_context(raw_request);
    let trace_id = upstream
        .as_ref()
        .map(|context| context.trace_id)
        .or_else(random_bytes::<16>)?;
    let span_id = random_bytes::<8>()?;
    let parent_span_id = upstream.as_ref().map(|context| context.parent_span_id);
    let flags = upstream.map(|context| context.flags).unwrap_or(1);
    Some(TraceSpanIds {
        trace_id,
        span_id,
        parent_span_id,
        flags,
    })
}

fn traceparent_context(raw_request: &str) -> Option<UpstreamTrace> {
    let value = raw_request.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("traceparent")
            .then(|| value.trim())
    })?;
    let mut parts = value.split('-');
    let version = parts.next()?;
    let trace_id = parts.next()?;
    let parent_id = parts.next()?;
    let flags = parts.next()?;
    if parts.next().is_some()
        || version.eq_ignore_ascii_case("ff")
        || version.len() != 2
        || trace_id.len() != 32
        || parent_id.len() != 16
        || flags.len() != 2
        || trace_id.bytes().all(|byte| byte == b'0')
    {
        return None;
    }
    decode_hex::<1>(version)?;
    let parent_span_id = decode_hex::<8>(parent_id)?;
    let flags = decode_hex::<1>(flags)?[0];
    if parent_span_id.iter().all(|byte| *byte == 0) {
        return None;
    }
    Some(UpstreamTrace {
        trace_id: decode_hex(trace_id)?,
        parent_span_id,
        flags,
    })
}

fn decode_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2 {
        return None;
    }
    let mut decoded = [0_u8; N];
    for (index, byte) in decoded.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(&value[start..start + 2], 16).ok()?;
    }
    Some(decoded)
}

fn random_bytes<const N: usize>() -> Option<[u8; N]> {
    let mut bytes = [0_u8; N];
    SystemRandom::new().fill(&mut bytes).ok()?;
    bytes.iter().any(|byte| *byte != 0).then_some(bytes)
}

fn now_unix_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

pub(crate) fn status_json() -> serde_json::Value {
    let Some(runtime) = RUNTIME.get() else {
        return serde_json::json!({
            "adapter": OtlpHttpJsonExporter::adapter_name(),
            "status": "DISABLED",
            "configured": false,
            "transport_export_observed": false,
            "live_status_ratified": false,
            "endpoint_value_recorded": false,
            "header_values_recorded": false,
        });
    };
    let exported = runtime.counters.exported.load(Ordering::Relaxed);
    let failed = runtime.counters.failed.load(Ordering::Relaxed);
    let last_outcome = runtime.counters.last_outcome.load(Ordering::Relaxed);
    let last_failure_category = runtime
        .counters
        .last_failure_category
        .lock()
        .ok()
        .and_then(|category| *category);
    serde_json::json!({
        "adapter": OtlpHttpJsonExporter::adapter_name(),
        "status": match last_outcome { OUTCOME_ACCEPTED => "TRANSPORT_ACCEPTED", OUTCOME_FAILED => "DEGRADED", _ => "CONFIGURED_UNOBSERVED" },
        "configured": true,
        "transport_export_observed": exported > 0,
        "live_status_ratified": false,
        "spans_enqueued": runtime.counters.enqueued.load(Ordering::Relaxed),
        "spans_exported": exported,
        "spans_failed": failed,
        "spans_dropped": runtime.counters.dropped.load(Ordering::Relaxed),
        "collector_warnings": runtime.counters.warnings.load(Ordering::Relaxed),
        "last_failure_category": last_failure_category,
        "queue_capacity": QUEUE_CAPACITY,
        "content_values_recorded": false,
        "endpoint_value_recorded": false,
        "header_values_recorded": false,
        "provider_turn_on_receipt_required": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traceparent_preserves_a_valid_upstream_trace() {
        let request = "GET /health HTTP/1.1\r\ntraceparent: 00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01\r\n\r\n";
        assert_eq!(
            traceparent_context(request),
            Some(UpstreamTrace {
                trace_id: [
                    0x4b, 0xf9, 0x2f, 0x35, 0x77, 0xb3, 0x4d, 0xa6, 0xa3, 0xce, 0x92, 0x9d, 0x0e,
                    0x0e, 0x47, 0x36,
                ],
                parent_span_id: [0x00, 0xf0, 0x67, 0xaa, 0x0b, 0xa9, 0x02, 0xb7],
                flags: 1,
            })
        );
    }

    #[test]
    fn malformed_or_zero_traceparents_are_refused() {
        assert!(traceparent_context("traceparent: nope\r\n").is_none());
        assert!(
            traceparent_context(
                "traceparent: 00-00000000000000000000000000000000-00f067aa0ba902b7-01\r\n"
            )
            .is_none()
        );
    }

    #[test]
    fn disabled_status_never_claims_live_export() {
        let status = status_json();
        assert_eq!(status["configured"], false);
        assert_eq!(status["transport_export_observed"], false);
        assert_eq!(status["live_status_ratified"], false);
        assert_eq!(status["endpoint_value_recorded"], false);
    }

    #[test]
    fn method_cardinality_is_bounded() {
        assert_eq!(normalized_method("GET"), "GET");
        assert_eq!(normalized_method("GET-secret"), "OTHER");
    }
}
