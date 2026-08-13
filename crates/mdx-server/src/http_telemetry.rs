//! In-process HTTP runtime telemetry.
//!
//! Until now the serving path measured nothing: the message relay had real
//! counters and percentiles while the kernel server's latency and error mix
//! were invisible, and the watchtower honestly labeled them NOT_OBSERVED.
//! This module records every served request - surface, status class, and
//! wall time - into atomic counters and a fixed ring, and renders them on
//! GET /runtime/http-metrics.json. Counts only, no bodies, no identities.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

const RING_SIZE: usize = 2048;
const MAX_SURFACES: usize = 64;

pub(crate) struct HttpTelemetry {
    started: Instant,
    requests_total: AtomicU64,
    status_2xx: AtomicU64,
    status_3xx: AtomicU64,
    status_4xx: AtomicU64,
    status_5xx: AtomicU64,
    refused_401: AtomicU64,
    refused_403: AtomicU64,
    refused_429: AtomicU64,
    latencies: Mutex<LatencyRing>,
    by_surface: Mutex<BTreeMap<String, u64>>,
}

struct LatencyRing {
    samples_micros: Vec<u64>,
    cursor: usize,
    recorded: u64,
}

fn telemetry() -> &'static HttpTelemetry {
    static TELEMETRY: OnceLock<HttpTelemetry> = OnceLock::new();
    TELEMETRY.get_or_init(|| HttpTelemetry {
        started: Instant::now(),
        requests_total: AtomicU64::new(0),
        status_2xx: AtomicU64::new(0),
        status_3xx: AtomicU64::new(0),
        status_4xx: AtomicU64::new(0),
        status_5xx: AtomicU64::new(0),
        refused_401: AtomicU64::new(0),
        refused_403: AtomicU64::new(0),
        refused_429: AtomicU64::new(0),
        latencies: Mutex::new(LatencyRing {
            samples_micros: Vec::with_capacity(RING_SIZE),
            cursor: 0,
            recorded: 0,
        }),
        by_surface: Mutex::new(BTreeMap::new()),
    })
}

/// The first path segment, bounded in cardinality so a scanner cannot grow
/// the map without limit.
pub(crate) fn surface_of(path: &str) -> String {
    let candidate = path
        .trim_start_matches('/')
        .split(['/', '?'])
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or("root")
        .trim_end_matches(".json");
    if candidate.len() > 48
        || !candidate
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        "other".to_string()
    } else {
        candidate.to_string()
    }
}

/// Record one served request. `status` is the HTTP status line ("200 OK").
pub(crate) fn record(path: &str, status: &str, elapsed_micros: u64) {
    let t = telemetry();
    t.requests_total.fetch_add(1, Ordering::Relaxed);
    let class = status.get(..3).unwrap_or("");
    match class.chars().next() {
        Some('2') => t.status_2xx.fetch_add(1, Ordering::Relaxed),
        Some('3') => t.status_3xx.fetch_add(1, Ordering::Relaxed),
        Some('4') => t.status_4xx.fetch_add(1, Ordering::Relaxed),
        Some('5') => t.status_5xx.fetch_add(1, Ordering::Relaxed),
        _ => 0,
    };
    match class {
        "401" => {
            t.refused_401.fetch_add(1, Ordering::Relaxed);
        }
        "403" => {
            t.refused_403.fetch_add(1, Ordering::Relaxed);
        }
        "429" => {
            t.refused_429.fetch_add(1, Ordering::Relaxed);
        }
        _ => {}
    }
    if let Ok(mut ring) = t.latencies.lock() {
        if ring.samples_micros.len() < RING_SIZE {
            ring.samples_micros.push(elapsed_micros);
        } else {
            let cursor = ring.cursor;
            ring.samples_micros[cursor] = elapsed_micros;
        }
        ring.cursor = (ring.cursor + 1) % RING_SIZE;
        ring.recorded += 1;
    }
    if let Ok(mut surfaces) = t.by_surface.lock() {
        let surface = surface_of(path);
        if surfaces.len() < MAX_SURFACES || surfaces.contains_key(&surface) {
            *surfaces.entry(surface).or_insert(0) += 1;
        } else {
            *surfaces.entry("other".to_string()).or_insert(0) += 1;
        }
    }
}

fn percentile(sorted: &[u64], hundredths: u64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let rank = (sorted.len() as u64 * hundredths).div_ceil(100).max(1) as usize;
    sorted[rank.min(sorted.len()) - 1]
}

/// Render the metrics packet. `latency_observed` is honest: false until at
/// least one request has been timed, never zero-as-green.
pub(crate) fn render_json() -> String {
    let t = telemetry();
    let (p50, p95, p99, window) = match t.latencies.lock() {
        Ok(ring) => {
            let mut sorted = ring.samples_micros.clone();
            sorted.sort_unstable();
            (
                percentile(&sorted, 50),
                percentile(&sorted, 95),
                percentile(&sorted, 99),
                sorted.len(),
            )
        }
        Err(_) => (0, 0, 0, 0),
    };
    let surfaces: BTreeMap<String, u64> = t
        .by_surface
        .lock()
        .map(|map| map.clone())
        .unwrap_or_default();
    let total = t.requests_total.load(Ordering::Relaxed);
    serde_json::json!({
        "name": "mdx-runtime-http-metrics",
        "latency_observed": window > 0,
        "uptime_seconds": t.started.elapsed().as_secs(),
        "requests_total": total,
        "status": {
            "ok_2xx": t.status_2xx.load(Ordering::Relaxed),
            "redirect_3xx": t.status_3xx.load(Ordering::Relaxed),
            "refused_4xx": t.status_4xx.load(Ordering::Relaxed),
            "failed_5xx": t.status_5xx.load(Ordering::Relaxed),
            "unauthorized_401": t.refused_401.load(Ordering::Relaxed),
            "forbidden_403": t.refused_403.load(Ordering::Relaxed),
            "rate_limited_429": t.refused_429.load(Ordering::Relaxed),
        },
        "latency_micros": {
            "p50": p50,
            "p95": p95,
            "p99": p99,
            "window_samples": window,
        },
        "requests_by_surface": surfaces,
        "otlp_export": crate::otel_runtime::status_json(),
        "production_write_allowed": false,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_pick_the_right_ranks() {
        let sorted: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&sorted, 50), 50);
        assert_eq!(percentile(&sorted, 95), 95);
        assert_eq!(percentile(&sorted, 99), 99);
        assert_eq!(percentile(&[], 50), 0);
        assert_eq!(percentile(&[7], 99), 7);
    }

    #[test]
    fn surfaces_are_bounded_and_normalized() {
        assert_eq!(surface_of("/twin/session-drafts.json"), "twin");
        assert_eq!(surface_of("/receipts.json"), "receipts");
        assert_eq!(surface_of("/"), "root");
        assert_eq!(surface_of("/health"), "health");
        assert_eq!(surface_of("/secret=value"), "other");
        assert_eq!(surface_of(&format!("/{}", "a".repeat(49))), "other");
    }

    #[test]
    fn record_and_render_round_trip() {
        record("/twin/session-drafts.json", "200 OK", 1200);
        record("/receipts.json", "401 Unauthorized", 300);
        let rendered = render_json();
        let parsed: serde_json::Value = serde_json::from_str(&rendered).expect("parse");
        assert_eq!(parsed["name"], "mdx-runtime-http-metrics");
        assert_eq!(parsed["latency_observed"], true);
        assert!(parsed["requests_total"].as_u64().unwrap_or(0) >= 2);
        assert!(parsed["status"]["unauthorized_401"].as_u64().unwrap_or(0) >= 1);
        assert!(parsed["latency_micros"]["p50"].as_u64().unwrap_or(0) > 0);
    }
}
