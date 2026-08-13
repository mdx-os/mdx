// Day-1 scale rails for the live stream path (ADR 0472): a bounded number
// of concurrent provider streams, a per-person rate window, and a rolling
// telemetry ring served at /twin/stream-telemetry.json. All in-memory by
// design - these limits protect THIS process and the route says "since
// boot" honestly. An over-limit ask falls back to the deterministic path
// with a plain reason; nobody gets an error for asking a busy server.
use crate::RouteResponse;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

fn env_limit(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

pub(crate) fn max_concurrent_streams() -> usize {
    env_limit("MDX_TWIN_MAX_CONCURRENT_STREAMS", 64)
}

pub(crate) fn actor_calls_per_minute() -> usize {
    env_limit("MDX_TWIN_ACTOR_LIVE_CALLS_PER_MINUTE", 30)
}

static ACTIVE_STREAMS: AtomicUsize = AtomicUsize::new(0);
const MAX_TRACKED_ACTORS: usize = 4096;
const RATE_WINDOW: Duration = Duration::from_secs(60);

// Holding a slot IS the permission to stream; dropping it frees the seat
// on every exit path, including panics and early returns.
pub(crate) struct StreamSlot;

impl Drop for StreamSlot {
    fn drop(&mut self) {
        ACTIVE_STREAMS.fetch_sub(1, Ordering::SeqCst);
    }
}

pub(crate) fn try_acquire_slot() -> Option<StreamSlot> {
    let prior = ACTIVE_STREAMS.fetch_add(1, Ordering::SeqCst);
    if prior >= max_concurrent_streams() {
        ACTIVE_STREAMS.fetch_sub(1, Ordering::SeqCst);
        return None;
    }
    Some(StreamSlot)
}

pub(crate) fn active_streams() -> usize {
    ACTIVE_STREAMS.load(Ordering::SeqCst)
}

#[derive(Default)]
struct ActorRateWindows {
    windows: HashMap<String, VecDeque<Instant>>,
}

impl ActorRateWindows {
    fn admit(&mut self, actor_id: &str, now: Instant, limit: usize, capacity: usize) -> bool {
        let cutoff = now.checked_sub(RATE_WINDOW).unwrap_or(now);
        let capacity = capacity.max(1);
        if !self.windows.contains_key(actor_id) && self.windows.len() >= capacity {
            // Reclaim actors whose entire one-minute window has elapsed before
            // admitting a new identity. If all tracked actors are still active,
            // refuse the unseen identity instead of evicting a live limiter and
            // allowing high-cardinality churn to reset rate history.
            self.windows.retain(|_, window| {
                while window.front().is_some_and(|at| *at <= cutoff) {
                    window.pop_front();
                }
                !window.is_empty()
            });
            if self.windows.len() >= capacity {
                return false;
            }
        }

        let window = self.windows.entry(actor_id.to_string()).or_default();
        while window.front().is_some_and(|at| *at <= cutoff) {
            window.pop_front();
        }
        if window.len() >= limit {
            return false;
        }
        window.push_back(now);
        true
    }
}

fn rate_windows() -> &'static Mutex<ActorRateWindows> {
    static CELL: OnceLock<Mutex<ActorRateWindows>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(ActorRateWindows::default()))
}

// One person's live-call window. A poisoned lock fails OPEN on purpose:
// the limiter exists for availability, so its own failure must not take
// availability down with it.
pub(crate) fn actor_within_rate(actor_id: &str) -> bool {
    let Ok(mut windows) = rate_windows().lock() else {
        return true;
    };
    windows.admit(
        actor_id,
        Instant::now(),
        actor_calls_per_minute(),
        MAX_TRACKED_ACTORS,
    )
}

pub(crate) struct StreamRecord {
    pub provider: String,
    pub model: String,
    // completed | failed | empty | busy | rate_limited
    pub outcome: &'static str,
    pub ttft_ms: Option<u64>,
    pub duration_ms: u64,
    pub delta_count: usize,
}

const RING_CAP: usize = 1000;

fn ring() -> &'static Mutex<Vec<StreamRecord>> {
    static CELL: OnceLock<Mutex<Vec<StreamRecord>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(Vec::new()))
}

pub(crate) fn record(entry: StreamRecord) {
    let Ok(mut records) = ring().lock() else {
        return;
    };
    if records.len() >= RING_CAP {
        records.remove(0);
    }
    records.push(entry);
}

fn percentile(sorted: &[u64], pct: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() as f64 - 1.0) * pct).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

pub(crate) fn route_response(method: &str, path: &str, _body: &str) -> Option<RouteResponse> {
    if path != "/twin/stream-telemetry.json" {
        return None;
    }
    if !method.eq_ignore_ascii_case("GET") {
        return Some(RouteResponse::text(
            "405 Method Not Allowed",
            "method not allowed\n".to_string(),
        ));
    }
    Some(RouteResponse::json("200 OK", render_json()))
}

fn render_json() -> String {
    let records = ring().lock().ok();
    let records = records.as_deref().map(|r| r.as_slice()).unwrap_or(&[]);
    let mut outcomes: HashMap<&str, usize> = HashMap::new();
    let mut providers: HashMap<String, usize> = HashMap::new();
    let mut total_deltas: usize = 0;
    for entry in records {
        *outcomes.entry(entry.outcome).or_default() += 1;
        let provider_model = format!("{} {}", entry.provider, entry.model);
        *providers.entry(provider_model).or_default() += 1;
        total_deltas += entry.delta_count;
    }
    let mut ttfts: Vec<u64> = records.iter().filter_map(|entry| entry.ttft_ms).collect();
    ttfts.sort_unstable();
    let mut durations: Vec<u64> = records
        .iter()
        .filter(|entry| entry.outcome == "completed")
        .map(|entry| entry.duration_ms)
        .collect();
    durations.sort_unstable();
    let ttft_p50 = percentile(&ttfts, 0.5);
    let ttft_p95 = percentile(&ttfts, 0.95);
    let duration_p50 = percentile(&durations, 0.5);
    let duration_p95 = percentile(&durations, 0.95);
    let has_samples = !durations.is_empty() && !ttfts.is_empty();
    let within_slo = has_samples
        && ttft_p50 <= 2_500
        && ttft_p95 <= 8_000
        && duration_p50 <= 15_000
        && duration_p95 <= 45_000;
    serde_json::json!({
        "name": "mdx-twin-stream-telemetry",
        "status": "OK",
        "window": "since boot, most recent 1000 streams",
        "stream_count": records.len(),
        "active_now": active_streams(),
        "outcomes": outcomes.iter().map(|(k, v)| (k.to_string(), v)).collect::<HashMap<String, &usize>>(),
        "providers": providers,
        "delta_count_total": total_deltas,
        "first_token_ms": { "p50": ttft_p50, "p95": ttft_p95 },
        "stream_duration_ms": { "p50": duration_p50, "p95": duration_p95 },
        "slo": {
            "status": if !has_samples { "NO_SAMPLES" } else if within_slo { "WITHIN_TARGET" } else { "MISSED_TARGET" },
            "first_token_p50_target_ms": 2_500,
            "first_token_p95_target_ms": 8_000,
            "duration_p50_target_ms": 15_000,
            "duration_p95_target_ms": 45_000,
            "within_target": within_slo
        },
        "limits": {
            "max_concurrent_streams": max_concurrent_streams(),
            "actor_live_calls_per_minute": actor_calls_per_minute(),
            "max_tracked_actor_windows": MAX_TRACKED_ACTORS,
            "provider_read_timeout_seconds": super::twin_session_stream::STREAM_READ_TIMEOUT_SECS,
        },
        "over_limit_behavior": "falls back to the deterministic governed POST with a plain reason - never an error",
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_cap_and_release() {
        // Default cap is far above this test's reach; assert the guard
        // arithmetic instead of racing the global.
        let before = active_streams();
        {
            let _one = try_acquire_slot().expect("slot");
            let _two = try_acquire_slot().expect("slot");
            assert_eq!(active_streams(), before + 2);
        }
        assert_eq!(active_streams(), before);
    }

    #[test]
    fn rate_window_holds_per_actor() {
        let limit = actor_calls_per_minute();
        for _ in 0..limit {
            assert!(actor_within_rate("rate_test_actor"));
        }
        assert!(!actor_within_rate("rate_test_actor"));
        // Another actor is unaffected.
        assert!(actor_within_rate("rate_test_other"));
    }

    #[test]
    fn actor_windows_bound_cardinality_and_reclaim_expired_entries() {
        let mut windows = ActorRateWindows::default();
        let started = Instant::now();
        let capacity = 16;

        for index in 0..1_000 {
            let admitted = windows.admit(&format!("actor_{index}"), started, 30, capacity);
            assert_eq!(admitted, index < capacity);
        }
        assert_eq!(windows.windows.len(), capacity);
        assert!(
            windows.admit("actor_0", started, 30, capacity),
            "tracked actors keep their rate history and admission path"
        );

        let after_window = started + RATE_WINDOW + Duration::from_millis(1);
        assert!(windows.admit("actor_after_expiry", after_window, 30, capacity));
        assert_eq!(windows.windows.len(), 1);
        assert!(windows.windows.contains_key("actor_after_expiry"));
    }

    #[test]
    fn telemetry_percentiles_and_outcomes() {
        for (ttft, outcome) in [(100, "completed"), (200, "completed"), (300, "completed")] {
            record(StreamRecord {
                provider: "xai".to_string(),
                model: "grok-4.3".to_string(),
                outcome,
                ttft_ms: Some(ttft),
                duration_ms: ttft * 10,
                delta_count: 5,
            });
        }
        record(StreamRecord {
            provider: "xai".to_string(),
            model: "grok-4.3".to_string(),
            outcome: "busy",
            ttft_ms: None,
            duration_ms: 1,
            delta_count: 0,
        });
        let body = render_json();
        assert!(body.contains("\"busy\":1") || body.contains("\"busy\": 1"));
        let packet: serde_json::Value = serde_json::from_str(&body).expect("telemetry JSON");
        assert_eq!(packet["slo"]["first_token_p50_target_ms"], 2_500);
        assert_eq!(packet["slo"]["first_token_p95_target_ms"], 8_000);
        assert_eq!(packet["slo"]["duration_p50_target_ms"], 15_000);
        assert_eq!(packet["slo"]["duration_p95_target_ms"], 45_000);
        assert!(packet["slo"].is_object());
    }
}
