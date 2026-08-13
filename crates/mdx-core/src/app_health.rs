// The central app-health telemetry contract.
//
// The macOS and web apps already measure their own health locally (call counts,
// error/hang counts, latency percentiles, launch time, an unclean-relaunch flag,
// and which app routes are failing). Until now that rollup died with the process.
// This module is the kernel-side ingest that makes it centrally visible, as a
// signed receipt, WITHOUT ever accepting free-text content.
//
// It is a sibling of the beta-feedback contract and copies its safe-payload
// discipline exactly: it reuses the same value-level scan (`value_forbidden_marker`)
// and the same bare-path route guard (`route_path_only_reason`) from
// `beta_feedback`, so a rollup can carry counts, durations, and non-secret app
// route path strings, but never a note, body, prompt, output, secret, or token.
// The one free-text danger on any telemetry surface (a route path) is enforced to
// be a bare path and marker-clean, exactly like the feedback route field.
//
// The self-heal bridge lives on top of this: when a route fails in enough
// DISTINCT sessions, the server synthesizes ONE governed `beta.feedback.captured`
// (metrics-only note) so the EXISTING feedback -> Forge autonomy loop can pick it
// up. This module owns the ingest, the cross-session projection, and the pure
// self-heal candidate/idempotency logic; it never runs Forge and never widens the
// autonomy loop.

use crate::beta_feedback::{route_path_only_reason, value_forbidden_marker};
use crate::{
    ActionKind, ActorId, CorrelationIds, GovernedWriteIdentity, LoopId, LoopRun, MdxKernel,
    Receipt, StorageProvider, TenantId, TraceId, WorkflowId, payload,
};
use std::collections::{BTreeMap, BTreeSet};

/// The governed local POST route that records an app-health rollup.
pub const APP_HEALTH_ROUTE: &str = "/telemetry/app-health.json";
/// The read-back projection route (aggregate across sessions).
pub const APP_HEALTH_PROJECTION_ROUTE: &str = "/telemetry/app-health/projection.json";
/// The governed self-heal scan route (signal -> synthesized capture).
pub const APP_HEALTH_SELF_HEAL_ROUTE: &str = "/telemetry/self-heal-scan.json";

/// Recorded when a rollup is admitted.
pub const APP_HEALTH_RECEIPT_KIND: &str = "app.health.reported";
/// Recorded when a rollup is refused; carries only the refusal code/field/marker,
/// never the offending content (receipts-or-it-didn't-happen, without leaking).
pub const APP_HEALTH_REFUSAL_RECEIPT_KIND: &str = "app.health.refused";
/// Recorded on EVERY self-heal scan (even a no-op), so a governed scan is always
/// auditable: it ran, examined N candidates, and synthesized M captures.
pub const APP_HEALTH_SELF_HEAL_SCAN_RECEIPT_KIND: &str = "app.health.self_heal.scanned";

/// The only accepted health states.
pub const APP_HEALTH_STATES: &[&str] = &["healthy", "degraded", "offline"];

/// The default distinct-session threshold before the self-heal bridge synthesizes
/// a capture for a failing route.
pub const APP_HEALTH_SELF_HEAL_DEFAULT_MIN_SESSIONS: usize = 3;
/// The surface a synthesized self-heal capture carries.
pub const APP_HEALTH_SELF_HEAL_SURFACE: &str = "telemetry";
/// The `feature_area` context marker on a synthesized capture. It is how the
/// bridge recognizes its own prior captures for idempotency.
pub const APP_HEALTH_SELF_HEAL_FEATURE_AREA: &str = "app_health_self_heal";

/// Size bounds. A degraded app must not smuggle content or bloat the ledger.
pub const APP_HEALTH_MAX_FAILING_ROUTES: usize = 50;
pub const APP_HEALTH_MAX_STRING_CHARS: usize = 200;
/// A stricter cap on a route path. A route is a short app path, not a slug; a
/// tighter bound shrinks the low-bandwidth exfil channel a route string could
/// otherwise carry into a synthesized note.
pub const APP_HEALTH_MAX_ROUTE_CHARS: usize = 128;
/// The number of recent reports the projection carries back.
pub const APP_HEALTH_RECENT_REPORT_LIMIT: usize = 20;
/// The most captures one self-heal scan may synthesize, so a flood of rollups
/// cannot batch-grow the ledger through the bridge.
pub const APP_HEALTH_SELF_HEAL_MAX_PER_SCAN: usize = 10;

/// A route on a health rollup must be path-segments only: a leading `/` and then
/// a conservative charset of unreserved path characters (letters, digits, and
/// `/._-`). This is STRICTER than the shared bare-path guard and is applied only
/// to telemetry routes, so it cannot weaken the beta-feedback contract. It rejects
/// spaces, query/fragment/scheme (already caught upstream), and any other
/// punctuation an exfil attempt might lean on.
fn route_charset_reason(route: &str) -> Option<&'static str> {
    if route
        .chars()
        .any(|c| !(c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-')))
    {
        return Some("charset");
    }
    None
}

/// Normalize a route for idempotency and dedup: lowercase and strip a trailing
/// slash (except root). So `/x`, `/x/`, and `/X` collapse to one key and cannot
/// each synthesize a capture.
fn normalize_route(route: &str) -> String {
    let lower = route.to_ascii_lowercase();
    let trimmed = lower.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

/// One failing route inside a rollup: a non-secret app route path, the HTTP
/// status it returned, and how many times in this session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppHealthFailingRoute<'a> {
    pub route: &'a str,
    pub status: u16,
    pub count: u64,
}

/// A batched, metrics-only health rollup from an app session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppHealthRollup<'a> {
    pub app_version: &'a str,
    pub surface: &'a str,
    /// An opaque session reference, never its contents. Scanned like every value.
    pub session_id: &'a str,
    /// ISO-8601 timestamp supplied by the caller (the kernel reads no clock).
    pub reported_at: &'a str,
    pub health_state: &'a str,
    pub total_calls: u64,
    pub error_count: u64,
    pub hang_count: u64,
    pub consecutive_failures: u64,
    pub avg_latency_ms: u64,
    pub p95_latency_ms: u64,
    pub launch_ms: u64,
    pub unclean_relaunch: bool,
    pub failing_routes: &'a [AppHealthFailingRoute<'a>],
}

/// The sanitized, safe-to-record rollup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeAppHealthPayload {
    pub app_version: String,
    pub surface: String,
    pub session_id: String,
    pub reported_at: String,
    pub health_state: String,
    pub total_calls: u64,
    pub error_count: u64,
    pub hang_count: u64,
    pub consecutive_failures: u64,
    pub avg_latency_ms: u64,
    pub p95_latency_ms: u64,
    pub launch_ms: u64,
    pub unclean_relaunch: bool,
    pub failing_routes: Vec<(String, u16, u64)>,
}

/// Why a rollup was refused. Every refusal is explicit; nothing is silently
/// stripped. A refusal never echoes the offending content, only a generic marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppHealthError {
    Missing(&'static str),
    UnknownHealthState(String),
    TooManyFailingRoutes {
        len: usize,
        max: usize,
    },
    StringTooLong {
        field: &'static str,
        len: usize,
        max: usize,
    },
    RouteNotPathOnly {
        field: String,
        reason: &'static str,
    },
    ForbiddenValue {
        field: String,
        marker: String,
    },
}

impl AppHealthError {
    /// A stable refusal code for the response and the refusal receipt.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Missing(_) => "missing_field",
            Self::UnknownHealthState(_) => "unknown_health_state",
            Self::TooManyFailingRoutes { .. } => "too_many_failing_routes",
            Self::StringTooLong { .. } => "string_too_long",
            Self::RouteNotPathOnly { .. } => "invalid_route",
            Self::ForbiddenValue { .. } => "unsafe_content",
        }
    }

    /// Which field was at fault (generic; never content).
    pub fn field(&self) -> String {
        match self {
            Self::Missing(field) => (*field).to_string(),
            Self::UnknownHealthState(_) => "health_state".to_string(),
            Self::TooManyFailingRoutes { .. } => "failing_routes".to_string(),
            Self::StringTooLong { field, .. } => (*field).to_string(),
            Self::RouteNotPathOnly { field, .. } => field.clone(),
            Self::ForbiddenValue { field, .. } => field.clone(),
        }
    }

    /// A generic marker (a fixed-vocabulary token like `secret`, or a reason like
    /// `query_string`); never the raw offending value.
    pub fn marker(&self) -> String {
        match self {
            Self::ForbiddenValue { marker, .. } => marker.clone(),
            Self::RouteNotPathOnly { reason, .. } => (*reason).to_string(),
            _ => String::new(),
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::Missing(field) => format!("app health rollup missing {field}"),
            Self::UnknownHealthState(state) => {
                format!(
                    "app health state must be one of {}; got {state}",
                    APP_HEALTH_STATES.join(", ")
                )
            }
            Self::TooManyFailingRoutes { len, max } => {
                format!("app health rollup carried {len} failing routes; max is {max}")
            }
            Self::StringTooLong { field, len, max } => {
                format!("app health {field} was {len} chars; max is {max}")
            }
            Self::RouteNotPathOnly { field, reason } => {
                format!("app health {field} is not a bare path ({reason})")
            }
            Self::ForbiddenValue { field, .. } => {
                format!("app health {field} carried content MDx will not record")
            }
        }
    }
}

/// The outcome of an admitted rollup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppHealthReport {
    pub status: &'static str,
    pub health_receipt_id: String,
    pub policy_decision_id: String,
    pub auth_session_status: String,
    pub surface: String,
    pub health_state: String,
    pub failing_route_count: usize,
    pub production_write_allowed: bool,
}

/// The outcome of a refused rollup: a recorded refusal receipt (no content).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppHealthRefusalReport {
    pub status: &'static str,
    pub refusal_receipt_id: String,
    pub policy_decision_id: String,
    pub refusal_code: String,
    pub refused_field: String,
    pub refused_marker: String,
    pub production_write_allowed: bool,
}

/// The metrics-only summary of a self-heal scan, recorded on the scan receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppHealthSelfHealScanSummary<'a> {
    pub min_sessions: usize,
    pub candidate_count: usize,
    pub synthesized_count: usize,
    pub capped: bool,
    /// The ids of the beta.feedback.captured receipts this scan synthesized.
    pub synthesized_receipt_ids: &'a [String],
}

/// The outcome of a self-heal scan: the always-recorded scan receipt so a
/// governed scan is auditable even when it synthesizes nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppHealthSelfHealScanReport {
    pub status: &'static str,
    pub scan_receipt_id: String,
    pub policy_decision_id: String,
    pub production_write_allowed: bool,
}

fn bound_string(field: &'static str, value: &str) -> Result<(), AppHealthError> {
    let len = value.chars().count();
    if len > APP_HEALTH_MAX_STRING_CHARS {
        return Err(AppHealthError::StringTooLong {
            field,
            len,
            max: APP_HEALTH_MAX_STRING_CHARS,
        });
    }
    Ok(())
}

fn scan_value(field: &str, value: &str) -> Result<(), AppHealthError> {
    if let Some(marker) = value_forbidden_marker(value) {
        return Err(AppHealthError::ForbiddenValue {
            field: field.to_string(),
            marker,
        });
    }
    Ok(())
}

/// Turn a rollup into a safe payload, or refuse it. The boundary: required fields
/// present, a bounded health state, bounded string lengths, a bounded number of
/// failing routes, every route a bare marker-clean path, and EVERY string value
/// (app_version, surface, session_id, reported_at, every route) free of forbidden
/// content markers and token-like strings. Numbers are pure counts/durations and
/// cannot carry content. Nothing is silently stripped.
pub fn build_safe_app_health_payload(
    rollup: &AppHealthRollup<'_>,
) -> Result<SafeAppHealthPayload, AppHealthError> {
    for (field, value) in [
        ("app_version", rollup.app_version),
        ("surface", rollup.surface),
        ("session_id", rollup.session_id),
        ("reported_at", rollup.reported_at),
        ("health_state", rollup.health_state),
    ] {
        if value.trim().is_empty() {
            return Err(AppHealthError::Missing(field));
        }
    }

    if !APP_HEALTH_STATES.contains(&rollup.health_state) {
        return Err(AppHealthError::UnknownHealthState(
            rollup.health_state.to_string(),
        ));
    }

    // Bound and scan every non-numeric value the app supplied.
    for (field, value) in [
        ("app_version", rollup.app_version),
        ("surface", rollup.surface),
        ("session_id", rollup.session_id),
        ("reported_at", rollup.reported_at),
    ] {
        bound_string(field, value)?;
        scan_value(field, value)?;
    }

    if rollup.failing_routes.len() > APP_HEALTH_MAX_FAILING_ROUTES {
        return Err(AppHealthError::TooManyFailingRoutes {
            len: rollup.failing_routes.len(),
            max: APP_HEALTH_MAX_FAILING_ROUTES,
        });
    }

    let mut failing_routes = Vec::with_capacity(rollup.failing_routes.len());
    for entry in rollup.failing_routes {
        if entry.route.trim().is_empty() {
            return Err(AppHealthError::Missing("failing_route.route"));
        }
        let route_len = entry.route.chars().count();
        if route_len > APP_HEALTH_MAX_ROUTE_CHARS {
            return Err(AppHealthError::StringTooLong {
                field: "failing_route.route",
                len: route_len,
                max: APP_HEALTH_MAX_ROUTE_CHARS,
            });
        }
        if let Some(reason) = route_path_only_reason(entry.route) {
            return Err(AppHealthError::RouteNotPathOnly {
                field: "failing_route.route".to_string(),
                reason,
            });
        }
        if let Some(reason) = route_charset_reason(entry.route) {
            return Err(AppHealthError::RouteNotPathOnly {
                field: "failing_route.route".to_string(),
                reason,
            });
        }
        scan_value("failing_route.route", entry.route)?;
        failing_routes.push((entry.route.to_string(), entry.status, entry.count));
    }

    Ok(SafeAppHealthPayload {
        app_version: rollup.app_version.to_string(),
        surface: rollup.surface.to_string(),
        session_id: rollup.session_id.to_string(),
        reported_at: rollup.reported_at.to_string(),
        health_state: rollup.health_state.to_string(),
        total_calls: rollup.total_calls,
        error_count: rollup.error_count,
        hang_count: rollup.hang_count,
        consecutive_failures: rollup.consecutive_failures,
        avg_latency_ms: rollup.avg_latency_ms,
        p95_latency_ms: rollup.p95_latency_ms,
        launch_ms: rollup.launch_ms,
        unclean_relaunch: rollup.unclean_relaunch,
        failing_routes,
    })
}

/// A parsed app-health receipt.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppHealthRecord {
    pub receipt_id: String,
    pub app_version: String,
    pub surface: String,
    pub session_id: String,
    pub reported_at: String,
    pub health_state: String,
    pub total_calls: u64,
    pub error_count: u64,
    pub hang_count: u64,
    pub consecutive_failures: u64,
    pub avg_latency_ms: u64,
    pub p95_latency_ms: u64,
    pub launch_ms: u64,
    pub unclean_relaunch: bool,
    pub failing_routes: Vec<(String, u16, u64)>,
}

/// A failing route aggregated across every session that reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppHealthFailingRouteRollup {
    pub route: String,
    pub total_failure_count: u64,
    pub distinct_session_count: usize,
    pub last_status: u16,
}

/// A recent report, for the health surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppHealthRecentReport {
    pub receipt_id: String,
    pub surface: String,
    pub session_id: String,
    pub reported_at: String,
    pub health_state: String,
    pub error_count: u64,
    pub hang_count: u64,
    pub unclean_relaunch: bool,
    pub failing_route_count: usize,
}

/// The cross-session aggregation the health surface reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppHealthProjection {
    pub total_reports: usize,
    pub distinct_sessions: usize,
    pub degraded_session_count: usize,
    pub unclean_relaunch_session_count: usize,
    pub unclean_relaunch_rate_pct: u32,
    pub hang_hotspot_session_count: usize,
    pub top_failing_routes: Vec<AppHealthFailingRouteRollup>,
    pub recent_reports: Vec<AppHealthRecentReport>,
}

/// A failing route that has crossed the distinct-session threshold. The bridge
/// synthesizes at most one capture per candidate route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppHealthSelfHealCandidate {
    pub route: String,
    pub last_status: u16,
    pub distinct_session_count: usize,
    pub total_failure_count: u64,
    pub last_reported_at: String,
}

fn u64_field(receipt: &Receipt, key: &str) -> u64 {
    receipt
        .payload
        .get(key)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
}

fn u16_field(receipt: &Receipt, key: &str) -> u16 {
    receipt
        .payload
        .get(key)
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(0)
}

fn str_field<'a>(receipt: &'a Receipt, key: &str) -> &'a str {
    receipt.payload.get(key).map(String::as_str).unwrap_or("")
}

fn bool_str(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

/// Parse every `app.health.reported` receipt into a record, in ledger order.
pub fn app_health_records(entries: &[Receipt]) -> Vec<AppHealthRecord> {
    entries
        .iter()
        .filter(|receipt| receipt.kind == APP_HEALTH_RECEIPT_KIND)
        .map(|receipt| {
            let count = str_field(receipt, "failing_route_count")
                .parse::<usize>()
                .unwrap_or(0);
            let mut failing_routes = Vec::with_capacity(count);
            for index in 0..count {
                let route = str_field(receipt, &format!("failing_route.{index}.route"));
                if route.is_empty() {
                    continue;
                }
                let status = u16_field(receipt, &format!("failing_route.{index}.status"));
                let times = u64_field(receipt, &format!("failing_route.{index}.count"));
                failing_routes.push((route.to_string(), status, times));
            }
            AppHealthRecord {
                receipt_id: receipt.receipt_id.clone(),
                app_version: str_field(receipt, "app_version").to_string(),
                surface: str_field(receipt, "surface").to_string(),
                session_id: str_field(receipt, "session_ref").to_string(),
                reported_at: str_field(receipt, "reported_at").to_string(),
                health_state: str_field(receipt, "health_state").to_string(),
                total_calls: u64_field(receipt, "total_calls"),
                error_count: u64_field(receipt, "error_count"),
                hang_count: u64_field(receipt, "hang_count"),
                consecutive_failures: u64_field(receipt, "consecutive_failures"),
                avg_latency_ms: u64_field(receipt, "avg_latency_ms"),
                p95_latency_ms: u64_field(receipt, "p95_latency_ms"),
                launch_ms: u64_field(receipt, "launch_ms"),
                unclean_relaunch: str_field(receipt, "unclean_relaunch") == "true",
                failing_routes,
            }
        })
        .collect()
}

/// Aggregate a set of records across sessions. Pure; the kernel method wraps it.
pub fn app_health_projection_from(entries: &[Receipt]) -> AppHealthProjection {
    let records = app_health_records(entries);

    let mut sessions: BTreeSet<String> = BTreeSet::new();
    let mut degraded_sessions: BTreeSet<String> = BTreeSet::new();
    let mut unclean_sessions: BTreeSet<String> = BTreeSet::new();
    let mut hang_sessions: BTreeSet<String> = BTreeSet::new();

    // route -> (total failures, distinct sessions, last status in ledger order)
    let mut route_totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut route_sessions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut route_last_status: BTreeMap<String, u16> = BTreeMap::new();

    for record in &records {
        if !record.session_id.is_empty() {
            sessions.insert(record.session_id.clone());
            if record.health_state == "degraded" || record.health_state == "offline" {
                degraded_sessions.insert(record.session_id.clone());
            }
            if record.unclean_relaunch {
                unclean_sessions.insert(record.session_id.clone());
            }
            if record.hang_count > 0 {
                hang_sessions.insert(record.session_id.clone());
            }
        }
        for (route, status, count) in &record.failing_routes {
            *route_totals.entry(route.clone()).or_insert(0) += *count;
            if !record.session_id.is_empty() {
                route_sessions
                    .entry(route.clone())
                    .or_default()
                    .insert(record.session_id.clone());
            }
            route_last_status.insert(route.clone(), *status);
        }
    }

    let mut top_failing_routes: Vec<AppHealthFailingRouteRollup> = route_totals
        .into_iter()
        .map(|(route, total)| {
            let distinct = route_sessions.get(&route).map(BTreeSet::len).unwrap_or(0);
            let last_status = route_last_status.get(&route).copied().unwrap_or(0);
            AppHealthFailingRouteRollup {
                route,
                total_failure_count: total,
                distinct_session_count: distinct,
                last_status,
            }
        })
        .collect();
    // Most-affected first: distinct sessions, then total failures, then route for
    // a stable order.
    top_failing_routes.sort_by(|a, b| {
        b.distinct_session_count
            .cmp(&a.distinct_session_count)
            .then(b.total_failure_count.cmp(&a.total_failure_count))
            .then(a.route.cmp(&b.route))
    });

    let recent_reports: Vec<AppHealthRecentReport> = records
        .iter()
        .rev()
        .take(APP_HEALTH_RECENT_REPORT_LIMIT)
        .map(|record| AppHealthRecentReport {
            receipt_id: record.receipt_id.clone(),
            surface: record.surface.clone(),
            session_id: record.session_id.clone(),
            reported_at: record.reported_at.clone(),
            health_state: record.health_state.clone(),
            error_count: record.error_count,
            hang_count: record.hang_count,
            unclean_relaunch: record.unclean_relaunch,
            failing_route_count: record.failing_routes.len(),
        })
        .collect();

    let distinct_sessions = sessions.len();
    let unclean_relaunch_rate_pct = if distinct_sessions == 0 {
        0
    } else {
        ((unclean_sessions.len() as u64 * 100) / distinct_sessions as u64) as u32
    };

    AppHealthProjection {
        total_reports: records.len(),
        distinct_sessions,
        degraded_session_count: degraded_sessions.len(),
        unclean_relaunch_session_count: unclean_sessions.len(),
        unclean_relaunch_rate_pct,
        hang_hotspot_session_count: hang_sessions.len(),
        top_failing_routes,
        recent_reports,
    }
}

/// The NORMALIZED routes for which the self-heal bridge has already synthesized a
/// capture. A capture is recognized as bridge-origin only when it is on the
/// reserved `telemetry` surface AND carries the self-heal feature-area marker, so
/// a human's own telemetry feedback is never mistaken for a synthesized one. Keys
/// are normalized (`normalize_route`) so `/x`, `/x/`, and `/X` all count as filed.
pub fn app_health_synthesized_routes(entries: &[Receipt]) -> BTreeSet<String> {
    entries
        .iter()
        .filter(|receipt| receipt.kind == crate::FEEDBACK_RECEIPT_KIND)
        .filter(|receipt| str_field(receipt, "surface") == APP_HEALTH_SELF_HEAL_SURFACE)
        .filter(|receipt| {
            str_field(receipt, "context.feature_area") == APP_HEALTH_SELF_HEAL_FEATURE_AREA
        })
        .map(|receipt| str_field(receipt, "route"))
        .filter(|route| !route.is_empty())
        .map(normalize_route)
        .collect()
}

/// The failing routes that cross the distinct-session threshold AND have not
/// already been synthesized. Routes are grouped by NORMALIZED key so case- and
/// trailing-slash variants (`/x`, `/x/`, `/X`) collapse into one candidate and
/// cannot each file a capture. The candidate carries a representative RAW route
/// (the first-seen form) so the synthesized capture cites a real path.
/// Deterministic order (normalized route ascending), so a rerun files nothing new.
/// `last_reported_at` is the timestamp of the most recent report that included the
/// route, used as the synthesized capture's `occurred_at` so it stays clock-free.
pub fn app_health_self_heal_candidates(
    entries: &[Receipt],
    min_sessions: usize,
) -> Vec<AppHealthSelfHealCandidate> {
    let min_sessions = min_sessions.max(1);
    let records = app_health_records(entries);
    let already = app_health_synthesized_routes(entries);

    // All maps keyed by the normalized route.
    let mut route_repr: BTreeMap<String, String> = BTreeMap::new();
    let mut route_totals: BTreeMap<String, u64> = BTreeMap::new();
    let mut route_sessions: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut route_last_status: BTreeMap<String, u16> = BTreeMap::new();
    let mut route_last_reported_at: BTreeMap<String, String> = BTreeMap::new();

    for record in &records {
        for (route, status, count) in &record.failing_routes {
            let key = normalize_route(route);
            route_repr
                .entry(key.clone())
                .or_insert_with(|| route.clone());
            *route_totals.entry(key.clone()).or_insert(0) += *count;
            if !record.session_id.is_empty() {
                route_sessions
                    .entry(key.clone())
                    .or_default()
                    .insert(record.session_id.clone());
            }
            route_last_status.insert(key.clone(), *status);
            if !record.reported_at.is_empty() {
                route_last_reported_at.insert(key.clone(), record.reported_at.clone());
            }
        }
    }

    route_totals
        .into_iter()
        .filter_map(|(key, total)| {
            if already.contains(&key) {
                return None;
            }
            let distinct = route_sessions.get(&key).map(BTreeSet::len).unwrap_or(0);
            if distinct < min_sessions {
                return None;
            }
            Some(AppHealthSelfHealCandidate {
                route: route_repr.get(&key).cloned().unwrap_or(key.clone()),
                last_status: route_last_status.get(&key).copied().unwrap_or(0),
                distinct_session_count: distinct,
                total_failure_count: total,
                last_reported_at: route_last_reported_at
                    .get(&key)
                    .cloned()
                    .unwrap_or_default(),
            })
        })
        .collect()
}

impl<S: StorageProvider> MdxKernel<S> {
    /// Record an app-health rollup. The safety boundary runs FIRST:
    /// `build_safe_app_health_payload` refuses any unsafe rollup before a single
    /// field is written, so a route that is not a bare path, a marker-bearing
    /// value, or a content string never reaches the ledger.
    pub fn record_app_health_local_with_identity(
        &mut self,
        rollup: &AppHealthRollup<'_>,
        identity: &GovernedWriteIdentity,
        tenant_id: &str,
        actor_id: &str,
    ) -> Result<AppHealthReport, AppHealthError> {
        let safe = build_safe_app_health_payload(rollup)?;
        let auth_session_status = if identity.identity_source == "local_demo_fixture" {
            "ACCEPTED_LOCAL_STUB"
        } else {
            "VERIFIED_TRUSTED_SESSION"
        };

        let correlation = CorrelationIds {
            tenant_id: TenantId::new(tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(actor_id),
            loop_id: LoopId::new("app_health"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision = self.decide_with_receipt(&correlation, ActionKind::RecordAppHealth);

        let total_calls = safe.total_calls.to_string();
        let error_count = safe.error_count.to_string();
        let hang_count = safe.hang_count.to_string();
        let consecutive_failures = safe.consecutive_failures.to_string();
        let avg_latency_ms = safe.avg_latency_ms.to_string();
        let p95_latency_ms = safe.p95_latency_ms.to_string();
        let launch_ms = safe.launch_ms.to_string();
        let failing_route_count = safe.failing_routes.len().to_string();

        // Only safe, scanned fields ride onto the receipt: counts, durations, a
        // bounded state, and non-secret bare-path route strings.
        let mut receipt_payload = payload(&[
            ("app_version", safe.app_version.as_str()),
            ("surface", safe.surface.as_str()),
            ("session_ref", safe.session_id.as_str()),
            ("reported_at", safe.reported_at.as_str()),
            ("health_state", safe.health_state.as_str()),
            ("total_calls", total_calls.as_str()),
            ("error_count", error_count.as_str()),
            ("hang_count", hang_count.as_str()),
            ("consecutive_failures", consecutive_failures.as_str()),
            ("avg_latency_ms", avg_latency_ms.as_str()),
            ("p95_latency_ms", p95_latency_ms.as_str()),
            ("launch_ms", launch_ms.as_str()),
            ("unclean_relaunch", bool_str(safe.unclean_relaunch)),
            ("failing_route_count", failing_route_count.as_str()),
            ("auth_session_status", auth_session_status),
            ("identity_source", identity.identity_source.as_str()),
            ("identity_actor_kind", identity.actor_kind.as_str()),
            (
                "identity_subject_actor_id",
                identity.subject_actor_id.as_str(),
            ),
            ("identity_delegation_id", identity.delegation_id.as_str()),
            ("source_route", APP_HEALTH_ROUTE),
            ("projection_route", APP_HEALTH_PROJECTION_ROUTE),
            ("terminal_state", "APP_HEALTH_RECORDED"),
            ("provider_call_allowed", "false"),
            ("worker_spawn_allowed", "false"),
            ("production_write_allowed", "false"),
        ]);
        for (index, (route, status, count)) in safe.failing_routes.iter().enumerate() {
            receipt_payload.insert(format!("failing_route.{index}.route"), route.clone());
            receipt_payload.insert(format!("failing_route.{index}.status"), status.to_string());
            receipt_payload.insert(format!("failing_route.{index}.count"), count.to_string());
        }

        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_APP_HEALTH",
            &correlation,
            &decision,
            APP_HEALTH_RECEIPT_KIND,
            receipt_payload,
        );
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "APP_HEALTH_RECORDED".to_string();
        }

        Ok(AppHealthReport {
            status: "APP_HEALTH_RECORDED",
            health_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            auth_session_status: auth_session_status.to_string(),
            surface: safe.surface,
            health_state: safe.health_state,
            failing_route_count: safe.failing_routes.len(),
            production_write_allowed: false,
        })
    }

    /// Record a refusal receipt for a rollup MDx would not accept. It carries only
    /// the refusal code, the field, and a generic marker; never the offending
    /// content. This keeps the boundary honest (receipts-or-it-didn't-happen)
    /// without leaking what was rejected.
    pub fn record_app_health_refusal_with_identity(
        &mut self,
        code: &str,
        refused_field: &str,
        marker: &str,
        identity: &GovernedWriteIdentity,
        tenant_id: &str,
        actor_id: &str,
    ) -> AppHealthRefusalReport {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(actor_id),
            loop_id: LoopId::new("app_health"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision = self.decide_with_receipt(&correlation, ActionKind::RejectAppHealth);

        let receipt_payload = payload(&[
            ("refusal_code", code),
            ("refused_field", refused_field),
            ("refused_marker", marker),
            ("recorded", "false"),
            ("identity_source", identity.identity_source.as_str()),
            ("identity_actor_kind", identity.actor_kind.as_str()),
            (
                "identity_subject_actor_id",
                identity.subject_actor_id.as_str(),
            ),
            ("identity_delegation_id", identity.delegation_id.as_str()),
            ("source_route", APP_HEALTH_ROUTE),
            ("terminal_state", "APP_HEALTH_REFUSED"),
            ("production_write_allowed", "false"),
        ]);
        let receipt = self.transition_receipt(
            &run_id,
            "REJECT_APP_HEALTH",
            &correlation,
            &decision,
            APP_HEALTH_REFUSAL_RECEIPT_KIND,
            receipt_payload,
        );
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "APP_HEALTH_REFUSED".to_string();
        }

        AppHealthRefusalReport {
            status: "APP_HEALTH_REFUSED",
            refusal_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            refusal_code: code.to_string(),
            refused_field: refused_field.to_string(),
            refused_marker: marker.to_string(),
            production_write_allowed: false,
        }
    }

    /// Record a self-heal scan summary on EVERY invocation, even a no-op. The
    /// receipt captures what the scan examined (min_sessions, candidate_count) and
    /// what it filed (synthesized_count, capped, the synthesized capture ids), so a
    /// governed scan is always auditable and the response can cite a receipt even
    /// when nothing was synthesized. Metrics-only; carries no route content.
    pub fn record_app_health_self_heal_scan_with_identity(
        &mut self,
        summary: AppHealthSelfHealScanSummary<'_>,
        identity: &GovernedWriteIdentity,
        tenant_id: &str,
        actor_id: &str,
    ) -> AppHealthSelfHealScanReport {
        let correlation = CorrelationIds {
            tenant_id: TenantId::new(tenant_id),
            trace_id: TraceId::new(self.ids.next("trace")),
            actor_id: ActorId::new(actor_id),
            loop_id: LoopId::new("app_health_self_heal"),
            workflow_id: WorkflowId::new(self.ids.next("workflow")),
        };
        let run_id = self.ids.next("run");
        self.storage.push_loop_run(LoopRun {
            run_id: run_id.clone(),
            loop_id: correlation.loop_id.clone(),
            agent_id: correlation.actor_id.clone(),
            workflow_id: correlation.workflow_id.clone(),
            status: "RUNNING".to_string(),
        });
        let decision =
            self.decide_with_receipt(&correlation, ActionKind::RecordAppHealthSelfHealScan);

        let min_sessions_s = summary.min_sessions.to_string();
        let candidate_count_s = summary.candidate_count.to_string();
        let synthesized_count_s = summary.synthesized_count.to_string();
        let cap_s = APP_HEALTH_SELF_HEAL_MAX_PER_SCAN.to_string();
        // Only receipt ids (never routes or notes) ride onto the scan receipt.
        let synthesized_ids = summary.synthesized_receipt_ids.join(",");
        let receipt_payload = payload(&[
            ("min_sessions", min_sessions_s.as_str()),
            ("candidate_count", candidate_count_s.as_str()),
            ("synthesized_count", synthesized_count_s.as_str()),
            ("max_per_scan", cap_s.as_str()),
            ("capped", bool_str(summary.capped)),
            ("synthesized_feedback_receipt_ids", synthesized_ids.as_str()),
            ("identity_source", identity.identity_source.as_str()),
            ("identity_actor_kind", identity.actor_kind.as_str()),
            (
                "identity_subject_actor_id",
                identity.subject_actor_id.as_str(),
            ),
            ("identity_delegation_id", identity.delegation_id.as_str()),
            ("source_route", APP_HEALTH_SELF_HEAL_ROUTE),
            ("feedback_route", "/feedback/captures.json"),
            ("autonomy_route", "/feedback/autonomy-runs.json"),
            ("terminal_state", "APP_HEALTH_SELF_HEAL_SCANNED"),
            ("provider_call_allowed", "false"),
            ("worker_spawn_allowed", "false"),
            ("production_write_allowed", "false"),
        ]);
        let receipt = self.transition_receipt(
            &run_id,
            "RECORD_APP_HEALTH_SELF_HEAL_SCAN",
            &correlation,
            &decision,
            APP_HEALTH_SELF_HEAL_SCAN_RECEIPT_KIND,
            receipt_payload,
        );
        if let Some(run) = self
            .storage
            .loop_runs_mut()
            .iter_mut()
            .find(|run| run.run_id == run_id)
        {
            run.status = "APP_HEALTH_SELF_HEAL_SCANNED".to_string();
        }

        AppHealthSelfHealScanReport {
            status: "APP_HEALTH_SELF_HEAL_SCANNED",
            scan_receipt_id: receipt.receipt_id,
            policy_decision_id: decision.policy_decision_id,
            production_write_allowed: false,
        }
    }

    /// The cross-session health projection.
    pub fn app_health_projection(&self) -> AppHealthProjection {
        app_health_projection_from(self.storage.ledger().entries())
    }

    /// The failing routes over the distinct-session threshold that have not been
    /// synthesized yet. Owned so the caller can drop the borrow and record.
    pub fn app_health_self_heal_candidates(
        &self,
        min_sessions: usize,
    ) -> Vec<AppHealthSelfHealCandidate> {
        app_health_self_heal_candidates(self.storage.ledger().entries(), min_sessions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rollup<'a>(
        session_id: &'a str,
        health_state: &'a str,
        failing_routes: &'a [AppHealthFailingRoute<'a>],
    ) -> AppHealthRollup<'a> {
        AppHealthRollup {
            app_version: "2026.7.4",
            surface: "forge",
            session_id,
            reported_at: "2026-07-04T00:00:00Z",
            health_state,
            total_calls: 100,
            error_count: 4,
            hang_count: 1,
            consecutive_failures: 0,
            avg_latency_ms: 42,
            p95_latency_ms: 120,
            launch_ms: 800,
            unclean_relaunch: false,
            failing_routes,
        }
    }

    #[test]
    fn a_valid_metrics_only_rollup_is_admitted() {
        let routes = [AppHealthFailingRoute {
            route: "/forge/build-requests/projection.json",
            status: 404,
            count: 3,
        }];
        let safe = build_safe_app_health_payload(&rollup("sess_a", "degraded", &routes))
            .expect("safe payload");
        assert_eq!(safe.failing_routes.len(), 1);
        assert_eq!(safe.failing_routes[0].1, 404);
        assert_eq!(safe.health_state, "degraded");
    }

    #[test]
    fn a_forbidden_marker_in_a_route_is_refused() {
        let routes = [AppHealthFailingRoute {
            route: "/x/authorization: bearer abc",
            status: 500,
            count: 1,
        }];
        let err = build_safe_app_health_payload(&rollup("sess_a", "healthy", &routes))
            .expect_err("forbidden value refused");
        assert_eq!(err.code(), "invalid_route");
    }

    #[test]
    fn a_content_bearing_session_id_is_refused() {
        let bad = build_safe_app_health_payload(&rollup("system prompt: leak", "healthy", &[]))
            .expect_err("content refused");
        assert_eq!(bad.code(), "unsafe_content");
        assert_eq!(bad.field(), "session_id");
    }

    #[test]
    fn an_unknown_health_state_is_refused() {
        let bad = build_safe_app_health_payload(&rollup("sess_a", "on_fire", &[]))
            .expect_err("unknown state refused");
        assert_eq!(bad.code(), "unknown_health_state");
    }

    #[test]
    fn too_many_failing_routes_is_refused() {
        let many: Vec<AppHealthFailingRoute> = (0..(APP_HEALTH_MAX_FAILING_ROUTES + 1))
            .map(|_| AppHealthFailingRoute {
                route: "/forge/x.json",
                status: 404,
                count: 1,
            })
            .collect();
        let bad = build_safe_app_health_payload(&rollup("sess_a", "healthy", &many))
            .expect_err("too many refused");
        assert_eq!(bad.code(), "too_many_failing_routes");
    }

    #[test]
    fn projection_aggregates_a_route_across_two_sessions() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:engineer");
        let routes = [AppHealthFailingRoute {
            route: "/forge/build-requests/projection.json",
            status: 404,
            count: 2,
        }];
        for session in ["sess_a", "sess_b"] {
            kernel
                .record_app_health_local_with_identity(
                    &rollup(session, "degraded", &routes),
                    &identity,
                    "tenant_local",
                    "human:engineer",
                )
                .expect("recorded");
        }
        let projection = kernel.app_health_projection();
        assert_eq!(projection.total_reports, 2);
        assert_eq!(projection.distinct_sessions, 2);
        assert_eq!(projection.degraded_session_count, 2);
        assert_eq!(projection.top_failing_routes.len(), 1);
        let top = &projection.top_failing_routes[0];
        assert_eq!(top.route, "/forge/build-requests/projection.json");
        assert_eq!(top.distinct_session_count, 2);
        assert_eq!(top.total_failure_count, 4);
        assert_eq!(top.last_status, 404);
    }

    #[test]
    fn candidates_fire_at_threshold_and_are_empty_below() {
        let mut kernel = MdxKernel::boot_local();
        let identity = GovernedWriteIdentity::local_demo("human:engineer");
        let routes = [AppHealthFailingRoute {
            route: "/forge/talent-authorizations/projection.json",
            status: 404,
            count: 1,
        }];
        for session in ["s1", "s2"] {
            kernel
                .record_app_health_local_with_identity(
                    &rollup(session, "degraded", &routes),
                    &identity,
                    "tenant_local",
                    "human:engineer",
                )
                .expect("recorded");
        }
        // Two sessions, threshold 3: no candidate.
        assert!(kernel.app_health_self_heal_candidates(3).is_empty());
        // Add a third distinct session: now it crosses.
        kernel
            .record_app_health_local_with_identity(
                &rollup("s3", "degraded", &routes),
                &identity,
                "tenant_local",
                "human:engineer",
            )
            .expect("recorded");
        let candidates = kernel.app_health_self_heal_candidates(3);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].distinct_session_count, 3);
        assert_eq!(
            candidates[0].route,
            "/forge/talent-authorizations/projection.json"
        );
    }
}
