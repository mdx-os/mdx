use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::fs::OpenOptions;
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::http::HeaderValue;
use uuid::Uuid;

const HEALTH_CONTENT_TYPE: &str = "application/json; charset=utf-8";
const MAX_SUBSCRIBE_TOPICS_PER_MESSAGE: usize = 32;
const MAX_SUBSCRIPTIONS_PER_CONNECTION: usize = 128;
const DEFAULT_DURABLE_REPLAY_MAX_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone)]
struct Config {
    bind_addr: String,
    relay_url: String,
    relay_id: String,
    channel_buffer_size: usize,
    heartbeat_timeout_secs: u64,
    max_msg_per_sec: u32,
    max_bytes_per_sec: usize,
    max_message_size: usize,
    max_connections_per_actor: usize,
    max_total_connections: usize,
    require_auth: bool,
    jwt_secret: Option<String>,
    typing_expiry_secs: u64,
    hot_cache_per_topic: usize,
    durable_replay_path: Option<String>,
    durable_replay_max_bytes: u64,
}

// Since-boot operational truth, served at /metrics. Counters are atomics;
// the fanout-handoff latency ring keeps the most recent 4096 samples
// (ingest to per-subscriber queue handoff - the socket flush itself adds
// the wire on top, and the field name says exactly that).
#[derive(Default)]
struct RelayMetrics {
    connections_opened: std::sync::atomic::AtomicU64,
    connections_closed: std::sync::atomic::AtomicU64,
    connections_refused: std::sync::atomic::AtomicU64,
    auth_failures: std::sync::atomic::AtomicU64,
    envelopes_ingested: std::sync::atomic::AtomicU64,
    deliveries_handed_off: std::sync::atomic::AtomicU64,
    deliveries_dropped_backpressure: std::sync::atomic::AtomicU64,
    replay_requests: std::sync::atomic::AtomicU64,
    replay_envelopes_served: std::sync::atomic::AtomicU64,
    verdict_caught_up: std::sync::atomic::AtomicU64,
    verdict_gap_detected: std::sync::atomic::AtomicU64,
    verdict_snapshot_required: std::sync::atomic::AtomicU64,
    drains_sent: std::sync::atomic::AtomicU64,
    handoff_micros: std::sync::Mutex<Vec<u64>>,
}

static VALKEY_SUBSCRIBED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn metrics() -> &'static RelayMetrics {
    static CELL: std::sync::OnceLock<RelayMetrics> = std::sync::OnceLock::new();
    CELL.get_or_init(RelayMetrics::default)
}

fn bump(counter: &std::sync::atomic::AtomicU64) {
    counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

fn read_counter(counter: &std::sync::atomic::AtomicU64) -> u64 {
    counter.load(std::sync::atomic::Ordering::Relaxed)
}

fn record_handoff_micros(micros: u64) {
    if let Ok(mut ring) = metrics().handoff_micros.lock() {
        if ring.len() >= 4096 {
            ring.remove(0);
        }
        ring.push(micros);
    }
}

fn handoff_percentile(sorted: &[u64], pct: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let index = ((sorted.len() as f64 - 1.0) * pct).round() as usize;
    sorted[index.min(sorted.len() - 1)]
}

fn render_metrics_json(config: &Config) -> String {
    let m = metrics();
    let mut latencies = m
        .handoff_micros
        .lock()
        .map(|ring| ring.clone())
        .unwrap_or_default();
    latencies.sort_unstable();
    let opened = read_counter(&m.connections_opened);
    let closed = read_counter(&m.connections_closed);
    format!(
        r#"{{"name":"mdx-message-relay-metrics","window":"since boot","relay_id":"{}","valkey_subscribed":{},"connections":{{"opened":{},"closed":{},"active":{},"refused":{},"auth_failures":{}}},"delivery":{{"envelopes_ingested":{},"handed_off":{},"dropped_backpressure":{},"fanout_handoff_micros":{{"p50":{},"p95":{},"p99":{}}},"note":"handoff is ingest to per-subscriber queue; the socket flush adds the wire"}},"replay":{{"requests":{},"envelopes_served":{},"verdicts":{{"caught_up":{},"gap_detected":{},"snapshot_required":{}}}}},"drains_sent":{}}}"#,
        config.relay_id,
        VALKEY_SUBSCRIBED.load(std::sync::atomic::Ordering::Relaxed),
        opened,
        closed,
        opened.saturating_sub(closed),
        read_counter(&m.connections_refused),
        read_counter(&m.auth_failures),
        read_counter(&m.envelopes_ingested),
        read_counter(&m.deliveries_handed_off),
        read_counter(&m.deliveries_dropped_backpressure),
        handoff_percentile(&latencies, 0.5),
        handoff_percentile(&latencies, 0.95),
        handoff_percentile(&latencies, 0.99),
        read_counter(&m.replay_requests),
        read_counter(&m.replay_envelopes_served),
        read_counter(&m.verdict_caught_up),
        read_counter(&m.verdict_gap_detected),
        read_counter(&m.verdict_snapshot_required),
        read_counter(&m.drains_sent),
    )
}

fn render_ready_json() -> (bool, String) {
    let ready = VALKEY_SUBSCRIBED.load(std::sync::atomic::Ordering::Relaxed);
    (
        ready,
        format!(
            r#"{{"name":"mdx-message-relay-readyz","ready":{ready},"requires":"live valkey subscription"}}"#
        ),
    )
}

impl Config {
    fn from_env() -> Self {
        let port = std::env::var("MDX_MESSAGE_RELAY_PORT")
            .or_else(|_| std::env::var("PORT"))
            .unwrap_or_else(|_| "9000".to_string());
        let bind_addr =
            std::env::var("MDX_MESSAGE_RELAY_BIND").unwrap_or_else(|_| format!("127.0.0.1:{port}"));
        let relay_url = std::env::var("MDX_MESSAGE_RELAY_URL")
            .or_else(|_| std::env::var("REDIS_URL"))
            .unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
        let require_auth = env_bool("MDX_MESSAGE_RELAY_REQUIRE_AUTH")
            || std::env::var("MDX_ENV").ok().as_deref() == Some("production")
            || !loopback_bind_addr(&bind_addr);
        Self {
            bind_addr,
            relay_url,
            relay_id: std::env::var("MDX_MESSAGE_RELAY_ID")
                .unwrap_or_else(|_| "local-relay".to_string()),
            channel_buffer_size: env_usize("MDX_MESSAGE_RELAY_CHANNEL_BUFFER_SIZE", 64),
            heartbeat_timeout_secs: env_u64("MDX_MESSAGE_RELAY_HEARTBEAT_TIMEOUT_SECS", 90),
            max_msg_per_sec: env_u32("MDX_MESSAGE_RELAY_MAX_MSG_PER_SEC", 30),
            max_bytes_per_sec: env_usize("MDX_MESSAGE_RELAY_MAX_BYTES_PER_SEC", 262_144),
            max_message_size: env_usize("MDX_MESSAGE_RELAY_MAX_MESSAGE_SIZE", 65_536),
            max_connections_per_actor: env_usize("MDX_MESSAGE_RELAY_MAX_CONNECTIONS_PER_ACTOR", 3),
            max_total_connections: env_usize("MDX_MESSAGE_RELAY_MAX_TOTAL_CONNECTIONS", 10_000),
            require_auth,
            jwt_secret: std::env::var("MDX_MESSAGE_RELAY_JWT_SECRET")
                .or_else(|_| std::env::var("JWT_SECRET"))
                .ok()
                .filter(|secret| secret.len() >= 32),
            typing_expiry_secs: env_u64("MDX_MESSAGE_RELAY_TYPING_EXPIRY_SECS", 8),
            hot_cache_per_topic: env_usize("MDX_MESSAGE_RELAY_HOT_CACHE_PER_TOPIC", 200),
            durable_replay_path: std::env::var("MDX_MESSAGE_RELAY_DURABLE_REPLAY_PATH").ok(),
            durable_replay_max_bytes: env_u64(
                "MDX_MESSAGE_RELAY_DURABLE_REPLAY_MAX_BYTES",
                DEFAULT_DURABLE_REPLAY_MAX_BYTES,
            ),
        }
    }
}

fn loopback_bind_addr(bind_addr: &str) -> bool {
    bind_addr
        .parse::<std::net::SocketAddr>()
        .is_ok_and(|address| address.ip().is_loopback())
}

#[derive(Debug, Clone, Deserialize)]
struct RelayClaims {
    sub: String,
    #[serde(default)]
    tenant_id: Option<String>,
    #[serde(default)]
    actor_id: Option<String>,
    #[serde(default)]
    device_id: Option<String>,
    #[serde(default)]
    purpose: Option<String>,
    #[serde(default)]
    allowed_stream_ids: Option<Vec<String>>,
    #[serde(default)]
    iss: Option<String>,
    #[serde(default)]
    aud: Option<String>,
    #[serde(rename = "exp")]
    _exp: usize,
}

#[derive(Clone)]
struct ConnectionIdentity {
    actor_id: String,
    tenant_id: Option<String>,
    device_id: Option<String>,
    purpose: Option<String>,
    allowed_stream_ids: Option<Vec<String>>,
    auth_status: &'static str,
}

struct ClientContext<'a> {
    fanout: &'a Fanout,
    handle: &'a ConnectionHandle,
    identity: &'a ConnectionIdentity,
    publisher: &'a RelayPublisher,
    typing: &'a TypingState,
}

#[derive(Clone)]
struct ConnectionHandle {
    connection_id: Uuid,
    actor_id: String,
    tx: mpsc::Sender<String>,
}

#[derive(Clone)]
struct RelayPublisher {
    tx: mpsc::Sender<PublishCommand>,
}

struct PublishCommand {
    topic: String,
    payload: String,
}

impl RelayPublisher {
    fn spawn(config: Config, mut shutdown_rx: watch::Receiver<bool>) -> Self {
        let (tx, mut rx) = mpsc::channel::<PublishCommand>(config.channel_buffer_size);
        tokio::spawn(async move {
            let client = match redis::Client::open(config.relay_url.as_str()) {
                Ok(client) => client,
                Err(error) => {
                    tracing::warn!(error = %error, "message relay publisher client unavailable");
                    return;
                }
            };
            let mut connection: Option<redis::aio::MultiplexedConnection> = None;
            loop {
                tokio::select! {
                    command = rx.recv() => {
                        let Some(command) = command else { break };
                        if connection.is_none() {
                            match client.get_multiplexed_async_connection().await {
                                Ok(next_connection) => connection = Some(next_connection),
                                Err(error) => {
                                    tracing::warn!(error = %error, "message relay publisher connection unavailable");
                                    continue;
                                }
                            }
                        }
                        let Some(active_connection) = connection.as_mut() else {
                            tracing::warn!(
                                topic = command.topic,
                                "message relay publisher connection missing after reconnect attempt"
                            );
                            continue;
                        };
                        let publish_result = active_connection
                            .publish::<_, _, ()>(&command.topic, &command.payload)
                            .await;
                        if let Err(error) = publish_result {
                            tracing::warn!(topic = command.topic, error = %error, "message relay publisher publish failed; reconnecting once");
                            connection = None;
                            match client.get_multiplexed_async_connection().await {
                                Ok(mut next_connection) => {
                                    let retry_result = next_connection
                                        .publish::<_, _, ()>(&command.topic, &command.payload)
                                        .await;
                                    if let Err(retry_error) = retry_result {
                                        tracing::warn!(topic = command.topic, error = %retry_error, "message relay publisher retry failed");
                                    }
                                    connection = Some(next_connection);
                                }
                                Err(reconnect_error) => {
                                    tracing::warn!(error = %reconnect_error, "message relay publisher reconnect failed");
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => break,
                }
            }
        });
        Self { tx }
    }

    fn publish(&self, topic: String, payload: String) -> Result<(), String> {
        self.tx
            .try_send(PublishCommand { topic, payload })
            .map_err(|error| format!("message relay publisher queue full: {error}"))
    }
}

#[derive(Default)]
struct Fanout {
    subscribers: DashMap<String, Vec<ConnectionHandle>>,
    hot_cache: DashMap<String, VecDeque<CachedEnvelope>>,
    max_cached_per_topic: usize,
    durable_replay_path: Option<String>,
    durable_replay_max_bytes: u64,
}

#[derive(Clone)]
struct CachedEnvelope {
    sequence: usize,
    payload: String,
}

impl Fanout {
    fn new(
        max_cached_per_topic: usize,
        durable_replay_path: Option<String>,
        durable_replay_max_bytes: u64,
    ) -> Self {
        let fanout = Self {
            subscribers: DashMap::new(),
            hot_cache: DashMap::new(),
            max_cached_per_topic,
            durable_replay_path,
            durable_replay_max_bytes,
        };
        fanout.load_durable_replay();
        fanout
    }

    fn subscribe(&self, topic: &str, handle: ConnectionHandle) {
        self.subscribers
            .entry(topic.to_string())
            .or_default()
            .push(handle);
    }

    fn unsubscribe(&self, topic: &str, connection_id: Uuid) {
        if let Some(mut handles) = self.subscribers.get_mut(topic) {
            handles.retain(|handle| handle.connection_id != connection_id);
        }
    }

    fn deliver(&self, topic: &str, payload: &str) {
        self.cache_if_envelope(topic, payload, true);
        bump(&metrics().envelopes_ingested);
        let ingest_started = Instant::now();
        if let Some(handles) = self.subscribers.get(topic) {
            for handle in handles.iter() {
                if handle.tx.try_send(payload.to_string()).is_err() {
                    bump(&metrics().deliveries_dropped_backpressure);
                    tracing::warn!(
                        topic = topic,
                        actor_id = handle.actor_id,
                        "dropping relay delivery for backpressured subscriber"
                    );
                } else {
                    bump(&metrics().deliveries_handed_off);
                    record_handoff_micros(ingest_started.elapsed().as_micros() as u64);
                }
            }
        }
    }

    fn cache_bounds(&self, topic: &str) -> Option<(usize, usize)> {
        self.hot_cache.get(topic).and_then(|entries| {
            let oldest = entries.iter().map(|envelope| envelope.sequence).min()?;
            let newest = entries.iter().map(|envelope| envelope.sequence).max()?;
            Some((oldest, newest))
        })
    }

    fn cached_after(&self, topic: &str, after_sequence: usize) -> Vec<String> {
        self.hot_cache
            .get(topic)
            .map(|entry| {
                entry
                    .iter()
                    .filter(|envelope| envelope.sequence > after_sequence)
                    .map(|envelope| envelope.payload.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn cache_if_envelope(&self, topic: &str, payload: &str, persist: bool) {
        let Ok(value) = serde_json::from_str::<Value>(payload) else {
            return;
        };
        if !replayable_envelope(topic, &value) {
            return;
        }
        let Some(sequence) = value
            .get("sequence")
            .and_then(Value::as_u64)
            .and_then(|sequence| usize::try_from(sequence).ok())
        else {
            return;
        };
        let mut entry = self.hot_cache.entry(topic.to_string()).or_default();
        entry.push_back(CachedEnvelope {
            sequence,
            payload: payload.to_string(),
        });
        while entry.len() > self.max_cached_per_topic {
            entry.pop_front();
        }
        if persist {
            self.persist_envelope(topic, payload);
        }
    }

    fn persist_envelope(&self, topic: &str, payload: &str) {
        let Some(path) = self.durable_replay_path.as_deref() else {
            return;
        };
        if durable_replay_too_large(path, self.durable_replay_max_bytes) {
            tracing::warn!(
                path = path,
                max_bytes = self.durable_replay_max_bytes,
                "message relay durable replay append skipped because the log is at its size limit"
            );
            return;
        }
        if let Some(parent) = Path::new(path).parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            tracing::warn!(path = path, error = %error, "message relay durable replay directory unavailable");
            return;
        }
        let line = json!({
            "topic": topic,
            "payload": payload,
        })
        .to_string();
        match OpenOptions::new().create(true).append(true).open(path) {
            Ok(mut file) => {
                if let Err(error) = writeln!(file, "{line}") {
                    tracing::warn!(path = path, error = %error, "message relay durable replay append failed");
                }
            }
            Err(error) => {
                tracing::warn!(path = path, error = %error, "message relay durable replay log unavailable");
            }
        }
    }

    fn load_durable_replay(&self) {
        let Some(path) = self.durable_replay_path.as_deref() else {
            return;
        };
        if durable_replay_too_large(path, self.durable_replay_max_bytes) {
            tracing::warn!(
                path = path,
                max_bytes = self.durable_replay_max_bytes,
                "message relay durable replay load skipped because the log exceeds its size limit"
            );
            return;
        }
        let Ok(file) = std::fs::File::open(path) else {
            return;
        };
        let reader = std::io::BufReader::new(file);
        let mut loaded_count = 0_usize;
        for line in reader.lines().map_while(Result::ok) {
            let Ok(entry) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(topic) = entry.get("topic").and_then(Value::as_str) else {
                continue;
            };
            let Some(payload) = entry.get("payload").and_then(Value::as_str) else {
                continue;
            };
            self.cache_if_envelope(topic, payload, false);
            loaded_count += 1;
        }
        tracing::info!(
            path = path,
            loaded_count,
            "message relay durable replay log loaded into hot cache"
        );
    }
}

fn replayable_envelope(topic: &str, value: &Value) -> bool {
    if value.get("type").and_then(Value::as_str) == Some("envelope") {
        return true;
    }
    let Ok(event) = serde_json::from_value::<mdx_mobile_relay::MobileRelayEnvelope>(value.clone())
    else {
        return false;
    };
    topic == event.topic() && event.is_contract_valid()
}

#[derive(Default)]
struct TypingState {
    active: DashMap<(String, String), Instant>,
}

impl TypingState {
    fn start(&self, topic: &str, actor_id: &str) {
        self.active
            .insert((topic.to_string(), actor_id.to_string()), Instant::now());
    }

    fn stop(&self, topic: &str, actor_id: &str) {
        self.active
            .remove(&(topic.to_string(), actor_id.to_string()));
    }

    fn remove_actor(&self, actor_id: &str) {
        self.active
            .retain(|(_, active_actor), _| active_actor != actor_id);
    }

    fn expired(&self, expiry: Duration) -> Vec<(String, String)> {
        let now = Instant::now();
        let mut expired = Vec::new();
        self.active.retain(|key, started| {
            if now.duration_since(*started) >= expiry {
                expired.push(key.clone());
                false
            } else {
                true
            }
        });
        expired
    }
}

#[derive(Default)]
struct ConnectionLimits {
    counts: Mutex<ConnectionCounts>,
}

#[derive(Default)]
struct ConnectionCounts {
    actor_counts: HashMap<String, usize>,
    total: usize,
}

struct ConnectionGuard {
    actor_id: String,
    limits: Arc<ConnectionLimits>,
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Ok(mut counts) = self.limits.counts.lock() {
            if let Some(count) = counts.actor_counts.get_mut(&self.actor_id) {
                *count = count.saturating_sub(1);
                if *count == 0 {
                    counts.actor_counts.remove(&self.actor_id);
                }
            }
            counts.total = counts.total.saturating_sub(1);
        }
    }
}

impl ConnectionLimits {
    fn try_connect(
        self: &Arc<Self>,
        actor_id: &str,
        max_actor: usize,
        max_total: usize,
    ) -> Result<ConnectionGuard, String> {
        let mut counts = self
            .counts
            .lock()
            .map_err(|_| "connection_limit_lock_poisoned".to_string())?;
        if counts.total >= max_total {
            return Err("connection_limit_global".to_string());
        }
        let actor_count = counts.actor_counts.get(actor_id).copied().unwrap_or(0);
        if actor_count >= max_actor {
            return Err("connection_limit_actor".to_string());
        }
        counts.total += 1;
        counts
            .actor_counts
            .insert(actor_id.to_string(), actor_count + 1);
        Ok(ConnectionGuard {
            actor_id: actor_id.to_string(),
            limits: Arc::clone(self),
        })
    }
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMessage {
    Subscribe {
        tenant_id: Option<String>,
        channel_ids: Option<Vec<String>>,
        stream_ids: Option<Vec<String>>,
        after_sequence: Option<usize>,
        // Who is reconnecting, and which attempt: logged for operators,
        // never required - an older client subscribes exactly as before.
        device_id: Option<String>,
        connection_generation: Option<usize>,
    },
    Unsubscribe {
        tenant_id: Option<String>,
        channel_ids: Option<Vec<String>>,
        stream_ids: Option<Vec<String>>,
    },
    Heartbeat {},
    TypingStart {
        tenant_id: Option<String>,
        channel_id: Option<String>,
    },
    TypingStop {
        tenant_id: Option<String>,
        channel_id: Option<String>,
    },
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ServerMessage<'a> {
    Ready {
        relay_id: &'a str,
        driver: &'a str,
        hot_path: &'a str,
        auth_status: &'a str,
        websocket_fanout_allowed: bool,
        production_delivery_allowed: bool,
        session_id: String,
        replay_window: usize,
    },
    // The catch-up verdict, one per subscribed topic after replay: the
    // client either knows it is current or knows it must fill from the
    // projection - never a silent gap pretending to be live.
    CatchUp {
        topic: String,
        state: &'a str,
        replayed_count: usize,
        oldest_cached_sequence: usize,
        newest_cached_sequence: usize,
    },
    Ack {
        action: &'a str,
        topic_count: usize,
    },
    Typing {
        tenant_id: String,
        channel_id: String,
        actor_id: String,
        is_typing: bool,
    },
    Error {
        code: &'a str,
        message: String,
    },
    Drain {
        reason: &'a str,
    },
}

// The catch-up verdict, pure so it can be tested without a socket:
// - empty cache + a real cursor: the relay cannot know what was missed -
//   snapshot_required (full projection reload).
// - empty cache + no cursor: a fresh client that loads the projection
//   anyway - caught_up.
// - cursor at or past the cache's oldest entry minus one: the replay
//   provably covered everything - caught_up.
// - cursor before the cache's reach: messages may have been evicted -
//   gap_detected (projection fill).
fn catch_up_state(cursor: usize, bounds: Option<(usize, usize)>) -> (&'static str, usize, usize) {
    match bounds {
        None => {
            if cursor > 0 {
                ("snapshot_required", 0, 0)
            } else {
                ("caught_up", 0, 0)
            }
        }
        Some((oldest, newest)) => {
            if cursor + 1 >= oldest {
                ("caught_up", oldest, newest)
            } else if cursor == 0 {
                // No cursor: the client fills from the projection by
                // design before going live; the replay just delivered
                // everything the cache holds.
                ("caught_up", oldest, newest)
            } else {
                ("gap_detected", oldest, newest)
            }
        }
    }
}

struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_per_sec: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(rate_per_sec: f64) -> Self {
        let max_tokens = rate_per_sec * 2.0;
        Self {
            tokens: max_tokens,
            max_tokens,
            refill_per_sec: rate_per_sec,
            last_refill: Instant::now(),
        }
    }

    fn try_consume(&mut self, amount: f64) -> bool {
        self.refill();
        if self.tokens >= amount {
            self.tokens -= amount;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let elapsed = self.last_refill.elapsed().as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.max_tokens);
        self.last_refill = Instant::now();
    }
}

struct RateLimiter {
    message_bucket: TokenBucket,
    byte_bucket: TokenBucket,
}

impl RateLimiter {
    fn new(config: &Config) -> Self {
        Self {
            message_bucket: TokenBucket::new(config.max_msg_per_sec as f64),
            byte_bucket: TokenBucket::new(config.max_bytes_per_sec as f64),
        }
    }

    fn check(&mut self, payload_len: usize, config: &Config) -> Result<(), &'static str> {
        let _ = config;
        if !self.message_bucket.try_consume(1.0) {
            return Err("message_rate_limit_exceeded");
        }
        if !self.byte_bucket.try_consume(payload_len as f64) {
            return Err("byte_rate_limit_exceeded");
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mdx_message_relay=info".into()),
        )
        .json()
        .init();

    let config = Config::from_env();
    let fanout = Arc::new(Fanout::new(
        config.hot_cache_per_topic,
        config.durable_replay_path.clone(),
        config.durable_replay_max_bytes,
    ));
    let limits = Arc::new(ConnectionLimits::default());
    let typing = Arc::new(TypingState::default());
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let publisher = RelayPublisher::spawn(config.clone(), shutdown_rx.clone());

    spawn_valkey_subscriber(config.clone(), Arc::clone(&fanout), shutdown_rx.clone()).await?;
    spawn_typing_expiry(
        config.clone(),
        publisher.clone(),
        Arc::clone(&typing),
        shutdown_rx.clone(),
    );

    let listener = TcpListener::bind(&config.bind_addr).await?;
    tracing::info!(
        bind_addr = config.bind_addr,
        relay_url = redacted_relay_url(&config.relay_url),
        relay_credentials_configured = relay_url_has_credentials(&config.relay_url),
        "MDx Message relay listening"
    );

    let accept_config = config.clone();
    let accept_fanout = Arc::clone(&fanout);
    let accept_limits = Arc::clone(&limits);
    let accept_publisher = publisher.clone();
    let accept_typing = Arc::clone(&typing);
    let accept_shutdown = shutdown_rx.clone();
    let accept_task = tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let config = accept_config.clone();
                    let fanout = Arc::clone(&accept_fanout);
                    let limits = Arc::clone(&accept_limits);
                    let publisher = accept_publisher.clone();
                    let typing = Arc::clone(&accept_typing);
                    let shutdown_rx = accept_shutdown.clone();
                    tokio::spawn(async move {
                        if let Err(error) = handle_connection(
                            stream,
                            config,
                            fanout,
                            limits,
                            publisher,
                            typing,
                            shutdown_rx,
                        )
                        .await
                        {
                            tracing::warn!(error = error, "relay connection ended with error");
                        }
                    });
                }
                Err(error) => {
                    tracing::warn!(error = %error, "relay accept failed");
                }
            }
        }
    });

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = sigterm.recv() => {}
        _ = accept_task => {}
    }
    // Drain: every open connection gets a drain frame and a clean close
    // before the process exits.
    let _ = shutdown_tx.send(true);
    tokio::time::sleep(Duration::from_millis(300)).await;
    Ok(())
}

async fn spawn_valkey_subscriber(
    config: Config,
    fanout: Arc<Fanout>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = redis::Client::open(config.relay_url.as_str())?;
    let mut pubsub = client.get_async_pubsub().await?;
    pubsub.psubscribe("mdx:message:*").await?;
    pubsub.psubscribe("mdx:ctx:*").await?;
    pubsub.psubscribe("mdx:dxr:*").await?;
    pubsub.psubscribe("mdx:forge:*").await?;
    VALKEY_SUBSCRIBED.store(true, std::sync::atomic::Ordering::Relaxed);
    tokio::spawn(async move {
        let mut stream = pubsub.on_message();
        loop {
            tokio::select! {
                message = stream.next() => {
                    let Some(message) = message else {
                        VALKEY_SUBSCRIBED.store(false, std::sync::atomic::Ordering::Relaxed);
                        break;
                    };
                    let topic = message.get_channel_name().to_string();
                    match message.get_payload::<String>() {
                        Ok(payload) => fanout.deliver(&topic, &payload),
                        Err(error) => tracing::warn!(topic = topic, error = %error, "invalid relay payload"),
                    }
                }
                _ = shutdown_rx.changed() => break,
            }
        }
    });
    Ok(())
}

async fn handle_connection(
    mut stream: TcpStream,
    config: Config,
    fanout: Arc<Fanout>,
    limits: Arc<ConnectionLimits>,
    publisher: RelayPublisher,
    typing: Arc<TypingState>,
    mut shutdown_rx: watch::Receiver<bool>,
) -> Result<(), String> {
    let mut peek_buf = [0_u8; 2048];
    let n = stream
        .peek(&mut peek_buf)
        .await
        .map_err(|error| error.to_string())?;
    let request = String::from_utf8_lossy(&peek_buf[..n]);
    if !request.to_ascii_lowercase().contains("upgrade: websocket") {
        let request_path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let (status, body) = if request_path.starts_with("/metrics") {
            ("200 OK", render_metrics_json(&config))
        } else if request_path.starts_with("/readyz") {
            let (ready, body) = render_ready_json();
            (
                if ready {
                    "200 OK"
                } else {
                    "503 Service Unavailable"
                },
                body,
            )
        } else {
            ("200 OK", render_health_json(&config))
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {HEALTH_CONTENT_TYPE}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .map_err(|error| error.to_string())?;
        return Ok(());
    }

    let identity = match authenticate_request(&request, &config) {
        Ok(identity) => identity,
        Err(error) => {
            bump(&metrics().auth_failures);
            return Err(error);
        }
    };
    let _guard = match limits.try_connect(
        &identity.actor_id,
        config.max_connections_per_actor,
        config.max_total_connections,
    ) {
        Ok(guard) => guard,
        Err(error) => {
            bump(&metrics().connections_refused);
            return Err(error);
        }
    };
    bump(&metrics().connections_opened);

    let ws_stream = accept_websocket(stream, parse_header(&request, "sec-websocket-protocol"))
        .await
        .map_err(|error| error.to_string())?;
    let (mut ws_write, mut ws_read) = ws_stream.split();
    let (tx, mut rx) = mpsc::channel::<String>(config.channel_buffer_size);
    let connection_id = Uuid::new_v4();
    let handle = ConnectionHandle {
        connection_id,
        actor_id: identity.actor_id.clone(),
        tx,
    };

    send_json(
        &mut ws_write,
        &ServerMessage::Ready {
            relay_id: &config.relay_id,
            driver: "valkey",
            hot_path: "websocket_valkey_pubsub_try_send",
            auth_status: identity.auth_status,
            websocket_fanout_allowed: true,
            production_delivery_allowed: false,
            session_id: connection_id.to_string(),
            replay_window: config.hot_cache_per_topic,
        },
    )
    .await?;

    let mut subscriptions: Vec<String> = Vec::new();
    let mut last_heartbeat = Instant::now();
    let mut rate = RateLimiter::new(&config);
    loop {
        tokio::select! {
            incoming = ws_read.next() => {
                let Some(incoming) = incoming else { break };
                let message = incoming.map_err(|error| error.to_string())?;
                match message {
                    Message::Text(text) => {
                        last_heartbeat = Instant::now();
                        if text.len() > config.max_message_size {
                            send_json(&mut ws_write, &ServerMessage::Error {
                                code: "message_too_large",
                                message: format!("{} exceeds {}", text.len(), config.max_message_size),
                            }).await?;
                            continue;
                        }
                        if let Err(code) = rate.check(text.len(), &config) {
                            send_json(&mut ws_write, &ServerMessage::Error {
                                code,
                                message: "relay input rate limit exceeded".to_string(),
                            }).await?;
                            continue;
                        }
                        handle_client_message(
                            &text,
                            ClientContext {
                                fanout: &fanout,
                                handle: &handle,
                                identity: &identity,
                                publisher: &publisher,
                                typing: &typing,
                            },
                            &mut subscriptions,
                            &mut ws_write,
                        ).await?;
                    }
                    Message::Ping(payload) => {
                        last_heartbeat = Instant::now();
                        ws_write.send(Message::Pong(payload)).await.map_err(|error| error.to_string())?;
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            Some(payload) = rx.recv() => {
                ws_write.send(Message::Text(payload)).await.map_err(|error| error.to_string())?;
            }
            _ = tokio::time::sleep(Duration::from_secs(config.heartbeat_timeout_secs)) => {
                if last_heartbeat.elapsed() > Duration::from_secs(config.heartbeat_timeout_secs) {
                    break;
                }
            }
            _ = shutdown_rx.changed() => {
                // Graceful drain: tell the client to come back, then close.
                // The client reconnects fast and lands on whichever relay
                // answers - the shared valkey topics make that seamless.
                bump(&metrics().drains_sent);
                let _ = send_json(&mut ws_write, &ServerMessage::Drain {
                    reason: "relay restarting - reconnect now",
                }).await;
                break;
            }
        }
    }

    bump(&metrics().connections_closed);
    for topic in subscriptions {
        fanout.unsubscribe(&topic, connection_id);
    }
    typing.remove_actor(&identity.actor_id);
    Ok(())
}

async fn handle_client_message(
    text: &str,
    context: ClientContext<'_>,
    subscriptions: &mut Vec<String>,
    ws_write: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<TcpStream>,
        Message,
    >,
) -> Result<(), String> {
    match serde_json::from_str::<ClientMessage>(text).map_err(|error| error.to_string())? {
        ClientMessage::Subscribe {
            tenant_id,
            channel_ids,
            stream_ids,
            after_sequence,
            device_id,
            connection_generation,
        } => {
            authorize_device(device_id.as_deref(), context.identity)?;
            let topics = topics_from_fields(tenant_id, channel_ids, stream_ids, context.identity)?;
            validate_subscription_budget(&topics, subscriptions)?;
            tracing::info!(
                device_id = device_id.as_deref().unwrap_or(""),
                connection_generation = connection_generation.unwrap_or(0),
                topic_count = topics.len(),
                cursor = after_sequence.unwrap_or(0),
                "relay subscribe"
            );
            for topic in &topics {
                if !subscriptions.contains(topic) {
                    context.fanout.subscribe(topic, context.handle.clone());
                    subscriptions.push(topic.clone());
                }
            }
            send_json(
                ws_write,
                &ServerMessage::Ack {
                    action: "subscribe",
                    topic_count: topics.len(),
                },
            )
            .await?;
            for topic in &topics {
                // Replay only when the client brought a cursor: a fresh
                // subscriber without one fills from the projection by
                // design, and unsolicited cache replay would hand it
                // stale events ahead of the live stream.
                let replayed_count = if let Some(cursor) = after_sequence {
                    bump(&metrics().replay_requests);
                    let cached = context.fanout.cached_after(topic, cursor);
                    let count = cached.len();
                    metrics()
                        .replay_envelopes_served
                        .fetch_add(count as u64, std::sync::atomic::Ordering::Relaxed);
                    for payload in cached {
                        ws_write
                            .send(Message::Text(payload))
                            .await
                            .map_err(|error| error.to_string())?;
                    }
                    count
                } else {
                    0
                };
                // The verdict: replay either provably covered the gap
                // between the client's cursor and now, or it did not -
                // and the client must fill from the projection instead
                // of pretending to be live.
                let bounds = context.fanout.cache_bounds(topic);
                let (state, oldest, newest) = catch_up_state(after_sequence.unwrap_or(0), bounds);
                match state {
                    "caught_up" => bump(&metrics().verdict_caught_up),
                    "gap_detected" => bump(&metrics().verdict_gap_detected),
                    _ => bump(&metrics().verdict_snapshot_required),
                }
                send_json(
                    ws_write,
                    &ServerMessage::CatchUp {
                        topic: topic.clone(),
                        state,
                        replayed_count,
                        oldest_cached_sequence: oldest,
                        newest_cached_sequence: newest,
                    },
                )
                .await?;
            }
            Ok(())
        }
        ClientMessage::Unsubscribe {
            tenant_id,
            channel_ids,
            stream_ids,
        } => {
            let topics = topics_from_fields(tenant_id, channel_ids, stream_ids, context.identity)?;
            for topic in &topics {
                context
                    .fanout
                    .unsubscribe(topic, context.handle.connection_id);
                subscriptions.retain(|subscribed| subscribed != topic);
            }
            send_json(
                ws_write,
                &ServerMessage::Ack {
                    action: "unsubscribe",
                    topic_count: topics.len(),
                },
            )
            .await
        }
        ClientMessage::Heartbeat {} => {
            send_json(
                ws_write,
                &ServerMessage::Ack {
                    action: "heartbeat",
                    topic_count: subscriptions.len(),
                },
            )
            .await
        }
        ClientMessage::TypingStart {
            tenant_id,
            channel_id,
        } => {
            let (tenant_id, channel_id, topic) =
                typing_target(tenant_id, channel_id, context.identity)?;
            context.typing.start(&topic, &context.identity.actor_id);
            publish_typing(
                context.publisher,
                &tenant_id,
                &channel_id,
                &context.identity.actor_id,
                true,
            )
        }
        ClientMessage::TypingStop {
            tenant_id,
            channel_id,
        } => {
            let (tenant_id, channel_id, topic) =
                typing_target(tenant_id, channel_id, context.identity)?;
            context.typing.stop(&topic, &context.identity.actor_id);
            publish_typing(
                context.publisher,
                &tenant_id,
                &channel_id,
                &context.identity.actor_id,
                false,
            )
        }
    }
}

fn publish_typing(
    publisher: &RelayPublisher,
    tenant_id: &str,
    channel_id: &str,
    actor_id: &str,
    is_typing: bool,
) -> Result<(), String> {
    let topic = topic_for_message(tenant_id, channel_id);
    let payload = serde_json::to_string(&ServerMessage::Typing {
        tenant_id: tenant_id.to_string(),
        channel_id: channel_id.to_string(),
        actor_id: actor_id.to_string(),
        is_typing,
    })
    .map_err(|error| error.to_string())?;
    publisher.publish(topic, payload)
}

fn relay_url_has_credentials(raw: &str) -> bool {
    let Some((_, rest)) = raw.split_once("://") else {
        return false;
    };
    let Some((userinfo, _)) = rest.split_once('@') else {
        return false;
    };
    !userinfo.is_empty()
}

fn redacted_relay_url(raw: &str) -> String {
    let Some((scheme, rest)) = raw.split_once("://") else {
        return raw.to_string();
    };
    let tail = rest.rsplit_once('@').map(|(_, tail)| tail).unwrap_or(rest);
    format!("{scheme}://{tail}")
}

fn durable_replay_too_large(path: &str, max_bytes: u64) -> bool {
    max_bytes > 0
        && std::fs::metadata(path)
            .map(|metadata| metadata.len() >= max_bytes)
            .unwrap_or(false)
}

fn validate_subscription_budget(topics: &[String], subscriptions: &[String]) -> Result<(), String> {
    if topics.len() > MAX_SUBSCRIBE_TOPICS_PER_MESSAGE {
        return Err("relay_subscribe_topic_count_exceeded".to_string());
    }
    let new_topic_count = topics
        .iter()
        .filter(|topic| !subscriptions.contains(topic))
        .count();
    if subscriptions.len() + new_topic_count > MAX_SUBSCRIPTIONS_PER_CONNECTION {
        return Err("relay_subscription_limit_exceeded".to_string());
    }
    Ok(())
}

fn topics_from_fields(
    tenant_id: Option<String>,
    channel_ids: Option<Vec<String>>,
    stream_ids: Option<Vec<String>>,
    identity: &ConnectionIdentity,
) -> Result<Vec<String>, String> {
    let tenant_id = tenant_id.unwrap_or_else(|| "local_tenant".to_string());
    authorize_tenant(&tenant_id, identity)?;
    let mut topics = Vec::new();
    if let Some(channel_ids) = channel_ids {
        for channel_id in channel_ids {
            authorize_message_topic(identity)?;
            topics.push(topic_for_message(&tenant_id, &channel_id));
        }
    }
    if let Some(stream_ids) = stream_ids {
        for stream_id in stream_ids {
            authorize_stream(&stream_id, identity)?;
            topics.push(topic_for_stream(&tenant_id, &stream_id));
        }
    }
    if topics.is_empty() {
        authorize_message_topic(identity)?;
        topics.push(topic_for_message(&tenant_id, "local-ops"));
    }
    Ok(topics)
}

fn typing_target(
    tenant_id: Option<String>,
    channel_id: Option<String>,
    identity: &ConnectionIdentity,
) -> Result<(String, String, String), String> {
    let tenant_id = tenant_id.unwrap_or_else(|| "local_tenant".to_string());
    authorize_tenant(&tenant_id, identity)?;
    authorize_message_topic(identity)?;
    let channel_id = channel_id.unwrap_or_else(|| "local-ops".to_string());
    let topic = topic_for_message(&tenant_id, &channel_id);
    Ok((tenant_id, channel_id, topic))
}

fn topic_for_message(tenant_id: &str, channel_id: &str) -> String {
    format!("mdx:message:{tenant_id}:{channel_id}")
}

fn topic_for_stream(tenant_id: &str, stream_id: &str) -> String {
    match stream_id {
        "ctx_event_stream" | "ctx_events" => format!("mdx:ctx:{tenant_id}:events"),
        "dxr_event_stream" | "dxr_events" => format!("mdx:dxr:{tenant_id}:events"),
        forge_run if forge_run.starts_with("forge_run_") => {
            format!("mdx:forge:{tenant_id}:{forge_run}")
        }
        other => topic_for_message(tenant_id, other),
    }
}

fn parse_header(request: &str, header_name: &str) -> Option<String> {
    request
        .lines()
        .take_while(|line| !line.trim().is_empty())
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.trim().eq_ignore_ascii_case(header_name) {
                Some(value.trim().to_string())
            } else {
                None
            }
        })
}

fn authenticate_request(request: &str, config: &Config) -> Result<ConnectionIdentity, String> {
    let token = extract_bearer_token(request);
    if config.require_auth {
        let Some(token) = token else {
            return Err("relay_auth_required".to_string());
        };
        return validate_token(&token, config);
    }
    if let Some(token) = token {
        match validate_token(&token, config) {
            Ok(identity) => return Ok(identity),
            Err(error) => {
                tracing::warn!(error = error, "relay token ignored in local optional mode")
            }
        }
    }
    Ok(ConnectionIdentity {
        actor_id: parse_header(request, "x-mdx-actor-id")
            .unwrap_or_else(|| "human:local_user".to_string()),
        tenant_id: parse_header(request, "x-mdx-tenant-id"),
        device_id: None,
        purpose: None,
        allowed_stream_ids: None,
        auth_status: "LOCAL_HEADER_OPTIONAL",
    })
}

fn validate_token(token: &str, config: &Config) -> Result<ConnectionIdentity, String> {
    let Some(secret) = config.jwt_secret.as_deref() else {
        return Err("relay_jwt_secret_missing".to_string());
    };
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.validate_aud = false;
    let claims = decode::<RelayClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map_err(|error| format!("relay_jwt_invalid: {error}"))?
    .claims;
    if claims.purpose.as_deref() == Some("mobile_forge_relay")
        && (claims.iss.as_deref() != Some("mdx-control-plane")
            || claims.aud.as_deref() != Some("mdx-message-relay"))
    {
        return Err("relay_jwt_mobile_issuer_audience_invalid".to_string());
    }
    let actor_id = claims.actor_id.unwrap_or(claims.sub);
    let tenant_id = claims
        .tenant_id
        .map(|tenant| tenant.trim().to_string())
        .filter(|tenant| !tenant.is_empty())
        .ok_or_else(|| "relay_jwt_tenant_missing".to_string())?;
    Ok(ConnectionIdentity {
        actor_id,
        tenant_id: Some(tenant_id),
        device_id: claims.device_id,
        purpose: claims.purpose,
        allowed_stream_ids: claims.allowed_stream_ids,
        auth_status: "JWT_SUBPROTOCOL_ACCEPTED",
    })
}

fn render_health_json(config: &Config) -> String {
    json!({
        "name": "mdx-message-relay",
        "status": "LIVE-LOCAL-RELAY",
        "driver": "valkey",
        "runtime": "mdx-message-relay",
        "relay_id": config.relay_id,
        "bind_addr": config.bind_addr,
        "topology": {
            "status": "LOCAL-MULTI-RELAY-TOPOLOGY-READY-PRODUCTION-PENDING",
            "shared_pubsub_driver": "valkey",
            "shared_topic_patterns": ["mdx:message:*", "mdx:ctx:*", "mdx:dxr:*", "mdx:forge:*"],
            "horizontal_scale_unit": "relay_process",
            "sticky_sessions_required": false,
            "cross_relay_delivery_path": "shared_valkey_pubsub",
            "production_multi_relay_deployed": false
        },
        "limits": {
            "channel_buffer_size": config.channel_buffer_size,
            "heartbeat_timeout_secs": config.heartbeat_timeout_secs,
            "max_msg_per_sec": config.max_msg_per_sec,
            "max_bytes_per_sec": config.max_bytes_per_sec,
            "max_message_size": config.max_message_size,
            "max_connections_per_actor": config.max_connections_per_actor,
            "max_total_connections": config.max_total_connections,
            "typing_expiry_secs": config.typing_expiry_secs,
            "hot_cache_per_topic": config.hot_cache_per_topic
        },
        "replay": {
            "hot_cache_status": "LIVE-LOCAL-RELAY-HOT-CACHE",
            "reconnect_gap_fill_status": "LIVE-LOCAL-RELAY-RECONNECT-GAP-FILL",
            "durable_replay_status": if config.durable_replay_path.is_some() {
                "LIVE-LOCAL-RELAY-DURABLE-REPLAY-CONFIGURED"
            } else {
                "PENDING-LOCAL-DURABLE-REPLAY-PATH"
            },
            "durable_replay_path_configured": config.durable_replay_path.is_some(),
            "durable_replay_max_bytes": config.durable_replay_max_bytes
        },
        "authority": {
            "require_auth": config.require_auth,
            "jwt_secret_configured": config.jwt_secret.is_some(),
            "local_websocket_fanout_allowed": true,
            "production_delivery_allowed": false,
            "production_write_allowed": false,
            "provider_calls_allowed": false,
            "remote_memory_lookup_allowed": false
        },
        "proof": {
            "static_check": "make message-realtime-relay-check",
            "local_live_proof": "scripts/local-message-realtime-relay-proof.sh",
            "hot_path_budget_ms": 20,
            "measured_p99_required": true
        }
    })
    .to_string()
}

fn extract_bearer_token(request: &str) -> Option<String> {
    if let Some(auth) = parse_header(request, "authorization") {
        let trimmed = auth.trim();
        if let Some(token) = trimmed.strip_prefix("Bearer ") {
            return Some(token.trim().to_string());
        }
        if let Some(token) = trimmed.strip_prefix("bearer ") {
            return Some(token.trim().to_string());
        }
    }
    if let Some(token) = parse_header(request, "x-mdx-relay-token") {
        return Some(token);
    }
    parse_header(request, "sec-websocket-protocol").and_then(|protocols| {
        let mut seen_bearer = false;
        for part in protocols.split(',').map(str::trim) {
            if seen_bearer && !part.is_empty() {
                return Some(part.to_string());
            }
            if part.eq_ignore_ascii_case("bearer") {
                seen_bearer = true;
            }
        }
        None
    })
}

fn protocols_include_bearer(protocols: &str) -> bool {
    protocols
        .split(',')
        .map(str::trim)
        .any(|part| part.eq_ignore_ascii_case("bearer"))
}

fn authorize_tenant(tenant_id: &str, identity: &ConnectionIdentity) -> Result<(), String> {
    if identity.auth_status == "JWT_SUBPROTOCOL_ACCEPTED" && identity.tenant_id.is_none() {
        return Err("relay_tenant_scope_missing".to_string());
    }
    if let Some(allowed) = identity.tenant_id.as_deref()
        && allowed != tenant_id
    {
        return Err("relay_tenant_scope_denied".to_string());
    }
    Ok(())
}

fn authorize_device(
    requested_device_id: Option<&str>,
    identity: &ConnectionIdentity,
) -> Result<(), String> {
    let Some(allowed_device_id) = identity.device_id.as_deref() else {
        return Ok(());
    };
    if requested_device_id != Some(allowed_device_id) {
        return Err("relay_device_scope_denied".to_string());
    }
    Ok(())
}

fn authorize_stream(stream_id: &str, identity: &ConnectionIdentity) -> Result<(), String> {
    if identity.purpose.as_deref() == Some("mobile_forge_relay")
        && !stream_id.starts_with("forge_run_")
    {
        return Err("relay_purpose_scope_denied".to_string());
    }
    if let Some(allowed_stream_ids) = &identity.allowed_stream_ids
        && !allowed_stream_ids
            .iter()
            .any(|allowed| allowed == stream_id)
    {
        return Err("relay_stream_scope_denied".to_string());
    }
    Ok(())
}

fn authorize_message_topic(identity: &ConnectionIdentity) -> Result<(), String> {
    if identity.purpose.as_deref() == Some("mobile_forge_relay") {
        return Err("relay_purpose_scope_denied".to_string());
    }
    Ok(())
}

#[allow(clippy::result_large_err)]
async fn accept_websocket(
    stream: TcpStream,
    protocol_header: Option<String>,
) -> Result<tokio_tungstenite::WebSocketStream<TcpStream>, tokio_tungstenite::tungstenite::Error> {
    tokio_tungstenite::accept_hdr_async(
        stream,
        move |_request: &Request, mut response: Response| {
            if protocol_header
                .as_deref()
                .map(protocols_include_bearer)
                .unwrap_or(false)
            {
                response
                    .headers_mut()
                    .insert("sec-websocket-protocol", HeaderValue::from_static("bearer"));
            }
            Ok(response)
        },
    )
    .await
}

fn spawn_typing_expiry(
    config: Config,
    publisher: RelayPublisher,
    typing: Arc<TypingState>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let expiry = Duration::from_secs(config.typing_expiry_secs);
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    for (topic, actor_id) in typing.expired(expiry) {
                        if let Some((tenant_id, channel_id)) = parse_topic(&topic)
                            && let Err(error) = publish_typing(&publisher, &tenant_id, &channel_id, &actor_id, false)
                        {
                            tracing::warn!(topic = topic, actor_id = actor_id, error = error, "typing expiry publish failed");
                        }
                    }
                }
                _ = shutdown_rx.changed() => break,
            }
        }
    });
}

fn parse_topic(topic: &str) -> Option<(String, String)> {
    let rest = topic.strip_prefix("mdx:message:")?;
    let (tenant_id, channel_id) = rest.split_once(':')?;
    Some((tenant_id.to_string(), channel_id.to_string()))
}

async fn send_json(
    ws_write: &mut futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<TcpStream>,
        Message,
    >,
    message: &ServerMessage<'_>,
) -> Result<(), String> {
    let payload = serde_json::to_string(message).map_err(|error| error.to_string())?;
    ws_write
        .send(Message::Text(payload))
        .await
        .map_err(|error| error.to_string())
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u32(name: &str, default: u32) -> u32 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_bool(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_RELAY_SECRET: &str = "test-only-mobile-relay-secret-32-chars";
    use jsonwebtoken::{EncodingKey, Header, encode};

    #[test]
    fn extracts_bearer_subprotocol_token() {
        let request = "Sec-WebSocket-Protocol: bearer, test.token\n";
        assert_eq!(extract_bearer_token(request).as_deref(), Some("test.token"));
    }

    #[test]
    fn accepts_control_plane_mobile_relay_claims() {
        let token = encode(
            &Header::default(),
            &json!({
                "sub": "human:local_user",
                "tenant_id": "local_tenant",
                "actor_id": "human:local_user",
                "device_id": "iphone_one",
                "purpose": "mobile_forge_relay",
                "allowed_stream_ids": ["forge_run_allowed"],
                "iss": "mdx-control-plane",
                "aud": "mdx-message-relay",
                "exp": 4_102_444_800_u64
            }),
            &EncodingKey::from_secret(TEST_RELAY_SECRET.as_bytes()),
        )
        .expect("mobile token");
        let config = Config {
            bind_addr: "127.0.0.1:9000".to_string(),
            relay_url: "redis://127.0.0.1:6379".to_string(),
            relay_id: "test-relay".to_string(),
            channel_buffer_size: 64,
            heartbeat_timeout_secs: 90,
            max_msg_per_sec: 30,
            max_bytes_per_sec: 262_144,
            max_message_size: 65_536,
            max_connections_per_actor: 3,
            max_total_connections: 10_000,
            require_auth: true,
            jwt_secret: Some(TEST_RELAY_SECRET.to_string()),
            typing_expiry_secs: 8,
            hot_cache_per_topic: 200,
            durable_replay_path: None,
            durable_replay_max_bytes: DEFAULT_DURABLE_REPLAY_MAX_BYTES,
        };
        let identity = validate_token(&token, &config).expect("valid scoped identity");
        assert_eq!(identity.device_id.as_deref(), Some("iphone_one"));
        assert_eq!(identity.purpose.as_deref(), Some("mobile_forge_relay"));
        assert_eq!(
            identity.allowed_stream_ids,
            Some(vec!["forge_run_allowed".to_string()])
        );
    }

    #[test]
    fn non_loopback_bindings_require_authentication() {
        assert!(loopback_bind_addr("127.0.0.1:9000"));
        assert!(loopback_bind_addr("[::1]:9000"));
        assert!(!loopback_bind_addr("0.0.0.0:9000"));
        assert!(!loopback_bind_addr("[::]:9000"));
    }

    #[test]
    fn denies_cross_tenant_subscription_for_token_identity() {
        let identity = ConnectionIdentity {
            actor_id: "human:local_user".to_string(),
            tenant_id: Some("tenant_a".to_string()),
            device_id: None,
            purpose: None,
            allowed_stream_ids: None,
            auth_status: "JWT_SUBPROTOCOL_ACCEPTED",
        };
        assert!(authorize_tenant("tenant_a", &identity).is_ok());
        assert_eq!(
            authorize_tenant("tenant_b", &identity),
            Err("relay_tenant_scope_denied".to_string())
        );
    }

    #[test]
    fn token_bucket_allows_burst_then_refills() {
        let mut bucket = TokenBucket::new(2.0);
        assert!(bucket.try_consume(1.0));
        assert!(bucket.try_consume(1.0));
        assert!(bucket.try_consume(1.0));
        assert!(bucket.try_consume(1.0));
        assert!(!bucket.try_consume(1.0));
    }

    #[test]
    fn parses_message_topic() {
        assert_eq!(
            parse_topic("mdx:message:local_tenant:local-ops"),
            Some(("local_tenant".to_string(), "local-ops".to_string()))
        );
    }

    #[test]
    fn maps_ctx_stream_to_ctx_relay_topic() {
        let identity = ConnectionIdentity {
            actor_id: "human:local_user".to_string(),
            tenant_id: Some("local_tenant".to_string()),
            device_id: None,
            purpose: None,
            allowed_stream_ids: None,
            auth_status: "JWT_SUBPROTOCOL_ACCEPTED",
        };
        assert_eq!(
            topics_from_fields(
                Some("local_tenant".to_string()),
                None,
                Some(vec!["ctx_event_stream".to_string()]),
                &identity
            ),
            Ok(vec!["mdx:ctx:local_tenant:events".to_string()])
        );
    }

    #[test]
    fn maps_dxr_stream_to_dxr_relay_topic() {
        let identity = ConnectionIdentity {
            actor_id: "human:local_user".to_string(),
            tenant_id: Some("local_tenant".to_string()),
            device_id: None,
            purpose: None,
            allowed_stream_ids: None,
            auth_status: "JWT_SUBPROTOCOL_ACCEPTED",
        };
        assert_eq!(
            topics_from_fields(
                Some("local_tenant".to_string()),
                None,
                Some(vec!["dxr_event_stream".to_string()]),
                &identity
            ),
            Ok(vec!["mdx:dxr:local_tenant:events".to_string()])
        );
    }

    #[test]
    fn mobile_token_is_limited_to_its_forge_session_and_device() {
        let identity = ConnectionIdentity {
            actor_id: "human:local_user".to_string(),
            tenant_id: Some("local_tenant".to_string()),
            device_id: Some("iphone_one".to_string()),
            purpose: Some("mobile_forge_relay".to_string()),
            allowed_stream_ids: Some(vec!["forge_run_allowed".to_string()]),
            auth_status: "JWT_SUBPROTOCOL_ACCEPTED",
        };
        assert!(authorize_device(Some("iphone_one"), &identity).is_ok());
        assert_eq!(
            authorize_device(Some("iphone_two"), &identity),
            Err("relay_device_scope_denied".to_string())
        );
        assert_eq!(
            topics_from_fields(
                Some("local_tenant".to_string()),
                None,
                Some(vec!["forge_run_allowed".to_string()]),
                &identity,
            ),
            Ok(vec!["mdx:forge:local_tenant:forge_run_allowed".to_string()])
        );
        assert_eq!(
            topics_from_fields(
                Some("local_tenant".to_string()),
                None,
                Some(vec!["forge_run_other".to_string()]),
                &identity,
            ),
            Err("relay_stream_scope_denied".to_string())
        );
        assert_eq!(
            topics_from_fields(
                Some("local_tenant".to_string()),
                None,
                Some(vec!["ctx_events".to_string()]),
                &identity,
            ),
            Err("relay_purpose_scope_denied".to_string())
        );
    }

    #[test]
    fn mobile_forge_token_cannot_reach_message_topics() {
        let identity = ConnectionIdentity {
            actor_id: "human:local_user".to_string(),
            tenant_id: Some("local_tenant".to_string()),
            device_id: Some("iphone_one".to_string()),
            purpose: Some("mobile_forge_relay".to_string()),
            allowed_stream_ids: Some(vec!["forge_run_allowed".to_string()]),
            auth_status: "JWT_SUBPROTOCOL_ACCEPTED",
        };

        for channel_ids in [
            Some(vec!["local-ops".to_string()]),
            Some(vec!["any-message-channel".to_string()]),
            Some(Vec::new()),
            None,
        ] {
            assert_eq!(
                topics_from_fields(
                    Some("local_tenant".to_string()),
                    channel_ids,
                    None,
                    &identity,
                ),
                Err("relay_purpose_scope_denied".to_string())
            );
        }
        assert_eq!(
            topics_from_fields(
                Some("local_tenant".to_string()),
                None,
                Some(vec!["local-ops".to_string()]),
                &identity,
            ),
            Err("relay_purpose_scope_denied".to_string())
        );
        assert_eq!(
            typing_target(
                Some("local_tenant".to_string()),
                Some("local-ops".to_string()),
                &identity,
            ),
            Err("relay_purpose_scope_denied".to_string())
        );
    }

    #[test]
    fn redacts_relay_url_credentials_for_logs() {
        let raw = "redis://user:password@relay.internal:6379/0";
        assert!(relay_url_has_credentials(raw));
        assert_eq!(
            redacted_relay_url(raw),
            "redis://relay.internal:6379/0".to_string()
        );
    }

    #[test]
    fn refuses_subscription_budget_overflow() {
        let topics = (0..=MAX_SUBSCRIBE_TOPICS_PER_MESSAGE)
            .map(|index| format!("mdx:message:tenant:channel-{index}"))
            .collect::<Vec<_>>();
        assert_eq!(
            validate_subscription_budget(&topics, &[]),
            Err("relay_subscribe_topic_count_exceeded".to_string())
        );
        let existing = (0..MAX_SUBSCRIPTIONS_PER_CONNECTION)
            .map(|index| format!("mdx:message:tenant:existing-{index}"))
            .collect::<Vec<_>>();
        assert_eq!(
            validate_subscription_budget(&["mdx:message:tenant:new".to_string()], &existing),
            Err("relay_subscription_limit_exceeded".to_string())
        );
    }

    #[test]
    fn health_packet_exposes_topology_limits_and_production_boundary() {
        let config = Config {
            bind_addr: "127.0.0.1:9000".to_string(),
            relay_url: "redis://127.0.0.1:6379".to_string(),
            relay_id: "test-relay".to_string(),
            channel_buffer_size: 64,
            heartbeat_timeout_secs: 90,
            max_msg_per_sec: 30,
            max_bytes_per_sec: 262_144,
            max_message_size: 65_536,
            max_connections_per_actor: 3,
            max_total_connections: 10_000,
            require_auth: true,
            jwt_secret: Some("secret".to_string()),
            typing_expiry_secs: 8,
            hot_cache_per_topic: 200,
            durable_replay_path: Some("/tmp/mdx-relay.jsonl".to_string()),
            durable_replay_max_bytes: DEFAULT_DURABLE_REPLAY_MAX_BYTES,
        };
        let health = render_health_json(&config);
        assert!(health.contains("\"status\":\"LIVE-LOCAL-RELAY\""));
        assert!(health.contains("LOCAL-MULTI-RELAY-TOPOLOGY-READY-PRODUCTION-PENDING"));
        assert!(
            health.contains(
                "\"shared_topic_patterns\":[\"mdx:message:*\",\"mdx:ctx:*\",\"mdx:dxr:*\",\"mdx:forge:*\"]"
            )
        );
        assert!(health.contains("\"max_total_connections\":10000"));
        assert!(health.contains("\"production_multi_relay_deployed\":false"));
        assert!(health.contains("\"production_delivery_allowed\":false"));
        assert!(health.contains("LIVE-LOCAL-RELAY-DURABLE-REPLAY-CONFIGURED"));
    }

    #[test]
    fn catch_up_verdicts_are_honest() {
        // Fresh client, empty cache: nothing to miss.
        assert_eq!(catch_up_state(0, None), ("caught_up", 0, 0));
        // Returning client, empty cache: the relay cannot know - say so.
        assert_eq!(catch_up_state(42, None), ("snapshot_required", 0, 0));
        // Cursor within the cache's reach: replay covered it.
        assert_eq!(catch_up_state(10, Some((11, 20))), ("caught_up", 11, 20));
        assert_eq!(catch_up_state(15, Some((11, 20))), ("caught_up", 11, 20));
        // Cursor before the cache's reach: evictions may hide messages.
        assert_eq!(catch_up_state(3, Some((11, 20))), ("gap_detected", 11, 20));
        // Fresh client with a warm cache: replay delivered everything held.
        assert_eq!(catch_up_state(0, Some((11, 20))), ("caught_up", 11, 20));
    }

    #[test]
    fn canonical_forge_events_enter_replay_without_transport_only_fields() {
        let event = json!({
            "schema_version": 1,
            "event_id": "forge_run_one:7",
            "session_id": "forge_run_one",
            "session_version": 7,
            "sequence": 7,
            "tenant_id": "tenant_one",
            "target": {
                "kind": "paired_host",
                "target_id": "host_one",
                "tenant_id": "tenant_one",
                "display_name": "Paired Mac",
                "capability_revision": 1
            },
            "kind": "progress",
            "safe_summary": "Proof is running",
            "evidence_refs": [],
            "occurred_at": "2026-07-13T12:00:00Z",
            "redaction_status": "not_required",
            "contains_secret_values": false,
            "grants_authority": false
        });
        assert!(replayable_envelope(
            "mdx:forge:tenant_one:forge_run_one",
            &event
        ));
        assert!(!replayable_envelope(
            "mdx:forge:tenant_other:forge_run_one",
            &event
        ));
        assert!(!replayable_envelope(
            "mdx:message:tenant_one:local-ops",
            &event
        ));

        let mut incomplete = event.clone();
        incomplete
            .as_object_mut()
            .unwrap()
            .remove("redaction_status");
        assert!(!replayable_envelope(
            "mdx:forge:tenant_one:forge_run_one",
            &incomplete
        ));

        let mut cross_tenant = event.clone();
        cross_tenant["target"]["tenant_id"] = json!("tenant_other");
        assert!(!replayable_envelope(
            "mdx:forge:tenant_one:forge_run_one",
            &cross_tenant
        ));

        let mut unknown = event.clone();
        unknown["kind"] = json!("terminal");
        assert!(!replayable_envelope(
            "mdx:forge:tenant_one:forge_run_one",
            &unknown
        ));

        let mut authority = event.clone();
        authority["grants_authority"] = json!(true);
        assert!(!replayable_envelope(
            "mdx:forge:tenant_one:forge_run_one",
            &authority
        ));

        let mut extra = event;
        extra["production_write_allowed"] = json!(false);
        assert!(!replayable_envelope(
            "mdx:forge:tenant_one:forge_run_one",
            &extra
        ));
    }
}
