//! Per-source rate limiting at the HTTP edge.
//!
//! The connection cap bounds how many sockets exist at once; this bounds how
//! fast any single source may issue requests. Token bucket per peer IP:
//! refill at MDX_HTTP_RATE_LIMIT_RPS (default 30/s), burst up to
//! MDX_HTTP_RATE_LIMIT_BURST (default 120). Over the limit answers 429 with
//! Retry-After instead of consuming a thread.
//!
//! Posture by mode, same shape as snapshots: trusted-session modes limit by
//! default (that is where strangers can reach the port); local-demo stays
//! unlimited so the deterministic proofs can hammer the server, and opts in
//! with MDX_HTTP_RATE_LIMIT=1. MDX_HTTP_RATE_LIMIT=0 forces it off anywhere.

use mdx_core::DeploymentMode;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

const MAX_TRACKED_SOURCES: usize = 4096;

pub(crate) fn enabled(mode: DeploymentMode) -> bool {
    match std::env::var("MDX_HTTP_RATE_LIMIT").ok().as_deref() {
        Some("1") => true,
        Some("0") => false,
        _ => mode.requires_trusted_session(),
    }
}

fn env_f64(name: &str, default: f64) -> f64 {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.parse::<f64>().ok())
        .filter(|value| *value > 0.0)
        .unwrap_or(default)
}

pub(crate) struct RateLimiter {
    buckets: Mutex<HashMap<String, Bucket>>,
    ip_refill_per_second: f64,
    ip_burst: f64,
    principal_refill_per_second: f64,
    principal_burst: f64,
}

struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

impl RateLimiter {
    pub(crate) fn from_env() -> Self {
        let ip_refill_per_second = env_f64("MDX_HTTP_RATE_LIMIT_RPS", 30.0);
        let ip_burst = env_f64("MDX_HTTP_RATE_LIMIT_BURST", 120.0);
        Self {
            buckets: Mutex::new(HashMap::new()),
            ip_refill_per_second,
            ip_burst,
            principal_refill_per_second: env_f64(
                "MDX_HTTP_PRINCIPAL_RATE_LIMIT_RPS",
                ip_refill_per_second,
            ),
            principal_burst: env_f64("MDX_HTTP_PRINCIPAL_RATE_LIMIT_BURST", ip_burst),
        }
    }

    #[cfg(test)]
    fn with_rates(refill_per_second: f64, burst: f64) -> Self {
        Self {
            buckets: Mutex::new(HashMap::new()),
            ip_refill_per_second: refill_per_second,
            ip_burst: burst,
            principal_refill_per_second: refill_per_second,
            principal_burst: burst,
        }
    }

    /// Take one token for this IP source before the request has been parsed.
    pub(crate) fn admit_ip(&self, source: std::net::IpAddr) -> bool {
        self.admit_key(
            format!("ip:{source}"),
            self.ip_refill_per_second,
            self.ip_burst,
        )
    }

    /// Take one token for this verified principal after the trusted-session
    /// boundary has parsed and verified the bearer token.
    pub(crate) fn admit_principal(&self, key: &str) -> bool {
        self.admit_key(
            format!("principal:{key}"),
            self.principal_refill_per_second,
            self.principal_burst,
        )
    }

    /// Take one token for this source. False means the request is refused.
    /// A poisoned map fails open: rate limiting protects capacity, and
    /// refusing everyone on an internal fault would be the worse failure.
    fn admit_key(&self, source: String, refill_per_second: f64, burst: f64) -> bool {
        let Ok(mut buckets) = self.buckets.lock() else {
            return true;
        };
        if buckets.len() >= MAX_TRACKED_SOURCES && !buckets.contains_key(&source) {
            // Drop the fullest buckets to make room: those sources have been
            // quiet longest relative to their refill.
            buckets.retain(|_, bucket| bucket.tokens < burst * 0.5);
            if buckets.len() >= MAX_TRACKED_SOURCES {
                buckets.clear();
            }
        }
        let now = Instant::now();
        let bucket = buckets.entry(source).or_insert(Bucket {
            tokens: burst,
            last_refill: now,
        });
        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * refill_per_second).min(burst);
        bucket.last_refill = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(last: u8) -> std::net::IpAddr {
        std::net::IpAddr::from([127, 0, 0, last])
    }

    #[test]
    fn burst_admits_then_refuses() {
        let limiter = RateLimiter::with_rates(1.0, 3.0);
        assert!(limiter.admit_ip(ip(1)));
        assert!(limiter.admit_ip(ip(1)));
        assert!(limiter.admit_ip(ip(1)));
        assert!(
            !limiter.admit_ip(ip(1)),
            "the fourth instant request is over burst"
        );
    }

    #[test]
    fn sources_are_limited_independently() {
        let limiter = RateLimiter::with_rates(1.0, 1.0);
        assert!(limiter.admit_ip(ip(1)));
        assert!(!limiter.admit_ip(ip(1)));
        assert!(
            limiter.admit_ip(ip(2)),
            "a different source has its own bucket"
        );
    }

    #[test]
    fn principals_are_limited_independently_even_on_same_ip() {
        let limiter = RateLimiter::with_rates(1.0, 1.0);
        assert!(limiter.admit_principal("tenant:user_a"));
        assert!(!limiter.admit_principal("tenant:user_a"));
        assert!(
            limiter.admit_principal("tenant:user_b"),
            "a different verified actor has its own bucket"
        );
    }

    #[test]
    fn tokens_refill_over_time() {
        let limiter = RateLimiter::with_rates(10.0, 2.0);
        assert!(limiter.admit_ip(ip(1)));
        assert!(limiter.admit_ip(ip(1)));
        assert!(!limiter.admit_ip(ip(1)));
        std::thread::sleep(std::time::Duration::from_millis(120));
        assert!(limiter.admit_ip(ip(1)), "refill restores admission");
    }
}
