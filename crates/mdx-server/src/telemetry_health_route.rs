// The central app-health telemetry ingest + self-heal bridge.
//
// This is the server half of the app-health contract in
// `mdx_core::app_health`. It accepts a batched, METRICS-ONLY health rollup from
// an app session, records it as a signed `app.health.reported` receipt, reads it
// back aggregated across sessions, and bridges a recurring failing route into the
// EXISTING feedback -> Forge autonomy loop by synthesizing ONE governed
// `beta.feedback.captured`.
//
// It copies the beta-feedback safe-payload discipline exactly: a top-level
// allowlist + forbidden-key scan runs FIRST (refusing note/body/prompt/secret/
// token or any unknown key), then the kernel's `build_safe_app_health_payload`
// enforces the value-level boundary (bare-path routes, marker-clean values, size
// bounds). A refused rollup records nothing but a content-free refusal receipt.
//
// FAIL-CLOSED: the self-heal scan only CREATES a capture. It never runs Forge,
// never widens `autopilot_allowed`, and never merges. The synthesized note is
// metrics-only (a route path + status + session count). The existing lane
// classifier holds a `bug` capture for review; self-delivery stays DARK.

use crate::RouteResponse;
use mdx_core::{
    APP_HEALTH_SELF_HEAL_DEFAULT_MIN_SESSIONS, APP_HEALTH_SELF_HEAL_FEATURE_AREA,
    APP_HEALTH_SELF_HEAL_MAX_PER_SCAN, APP_HEALTH_SELF_HEAL_SURFACE, AppHealthFailingRoute,
    AppHealthRollup, BetaFeedbackCapture, FORBIDDEN_CONTEXT_KEYS, FeedbackContextEntry, MdxKernel,
    json_string_literal,
};
use serde_json::Value;
use std::sync::{Arc, RwLock};

/// The routes this module serves.
const APP_HEALTH_ROUTE: &str = "/telemetry/app-health.json";
const APP_HEALTH_PROJECTION_ROUTE: &str = "/telemetry/app-health/projection.json";
const APP_HEALTH_SELF_HEAL_ROUTE: &str = "/telemetry/self-heal-scan.json";
const FEEDBACK_ROUTE: &str = "/feedback/captures.json";
const FEEDBACK_AUTONOMY_ROUTE: &str = "/feedback/autonomy-runs.json";

/// A degraded app must not smuggle content or bloat the kernel. Reject an
/// oversized body before any parse or record.
const MAX_BODY_BYTES: usize = 16 * 1024;

/// The ONLY top-level keys an app-health rollup may carry. Anything else, or any
/// key naming forbidden content, is refused and records nothing but a refusal.
const ACCEPTED_TOP_LEVEL: &[&str] = &[
    "app_version",
    "surface",
    "session_id",
    "reported_at",
    "health_state",
    "total_calls",
    "error_count",
    "hang_count",
    "consecutive_failures",
    "avg_latency_ms",
    "p95_latency_ms",
    "launch_ms",
    "unclean_relaunch",
    "failing_routes",
    "tenant_id",
    "actor_id",
    "actor_role",
];

/// The ONLY keys a `failing_routes[]` object may carry.
const ACCEPTED_FAILING_ROUTE_KEYS: &[&str] = &["route", "status", "count"];

pub(crate) fn route_response(
    method: &str,
    path: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Option<Result<RouteResponse, String>> {
    match path {
        APP_HEALTH_ROUTE => Some(handle_ingest(method, body, kernel)),
        APP_HEALTH_PROJECTION_ROUTE => Some(handle_projection(method, kernel)),
        APP_HEALTH_SELF_HEAL_ROUTE => Some(handle_self_heal_scan(method, body, kernel)),
        _ => None,
    }
}

/// A pre-parse refusal (`code`, generic `marker`) before we trust the body shape.
fn top_level_refusal(body: &str) -> Option<(&'static str, String)> {
    let value = match serde_json::from_str::<Value>(body) {
        Ok(value) => value,
        Err(_) => return Some(("bad_body", String::new())),
    };
    let object = match value.as_object() {
        Some(object) => object,
        None => return Some(("bad_body", String::new())),
    };
    for key in object.keys() {
        let lower = key.to_ascii_lowercase();
        if let Some(marker) = FORBIDDEN_CONTEXT_KEYS
            .iter()
            .find(|forbidden| lower.contains(**forbidden))
        {
            return Some(("unsafe_content", (*marker).to_string()));
        }
        if !ACCEPTED_TOP_LEVEL.contains(&key.as_str()) {
            // The key name is client-controlled; never echo it back.
            return Some(("unknown_field", String::new()));
        }
    }
    // Every failing-route object may only carry the accepted keys, and never a
    // forbidden one. This catches a note/body/secret smuggled one level down.
    if let Some(Value::Array(routes)) = object.get("failing_routes") {
        for entry in routes {
            let Some(entry) = entry.as_object() else {
                return Some(("bad_body", String::new()));
            };
            for key in entry.keys() {
                let lower = key.to_ascii_lowercase();
                if let Some(marker) = FORBIDDEN_CONTEXT_KEYS
                    .iter()
                    .find(|forbidden| lower.contains(**forbidden))
                {
                    return Some(("unsafe_content", (*marker).to_string()));
                }
                if !ACCEPTED_FAILING_ROUTE_KEYS.contains(&key.as_str()) {
                    return Some(("unknown_field", String::new()));
                }
            }
        }
    }
    None
}

fn handle_ingest(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "tenant_local",
        "local_user",
        "viewer",
    );
    let mut guard = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;

    if body.len() > MAX_BODY_BYTES {
        return Ok(record_refusal(
            &mut guard,
            &resolved,
            "body_too_large",
            "body",
            "",
        ));
    }
    if let Some((code, marker)) = top_level_refusal(body) {
        return Ok(record_refusal(&mut guard, &resolved, code, "body", &marker));
    }

    // The top-level shape is trusted now; parse the metrics.
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let app_version = str_field(&value, "app_version");
    let surface = str_field(&value, "surface");
    let session_id = str_field(&value, "session_id");
    let reported_at = str_field(&value, "reported_at");
    let health_state = str_field(&value, "health_state");
    let failing = parse_failing_routes(&value);
    let failing_routes: Vec<AppHealthFailingRoute<'_>> = failing
        .iter()
        .map(|(route, status, count)| AppHealthFailingRoute {
            route: route.as_str(),
            status: *status,
            count: *count,
        })
        .collect();

    let rollup = AppHealthRollup {
        app_version: &app_version,
        surface: &surface,
        session_id: &session_id,
        reported_at: &reported_at,
        health_state: &health_state,
        total_calls: u64_field(&value, "total_calls"),
        error_count: u64_field(&value, "error_count"),
        hang_count: u64_field(&value, "hang_count"),
        consecutive_failures: u64_field(&value, "consecutive_failures"),
        avg_latency_ms: u64_field(&value, "avg_latency_ms"),
        p95_latency_ms: u64_field(&value, "p95_latency_ms"),
        launch_ms: u64_field(&value, "launch_ms"),
        unclean_relaunch: value
            .get("unclean_relaunch")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        failing_routes: &failing_routes,
    };

    match guard.record_app_health_local_with_identity(
        &rollup,
        &resolved.identity,
        &resolved.tenant_id,
        &resolved.actor_id,
    ) {
        Ok(report) => Ok(RouteResponse::json(
            "200 OK",
            format!(
                r#"{{"name":"mdx-app-health-report-local-post","status":{},"auth_session_required":true,"auth_session_status":{},"surface":{},"health_state":{},"failing_route_count":{},"health_receipt_id":{},"policy_decision_id":{},"projection_route":{},"self_heal_route":{},"provider_call_allowed":false,"worker_spawn_allowed":false,"production_write_allowed":false}}"#,
                json_string_literal(report.status),
                json_string_literal(&report.auth_session_status),
                json_string_literal(&report.surface),
                json_string_literal(&report.health_state),
                report.failing_route_count,
                json_string_literal(&report.health_receipt_id),
                json_string_literal(&report.policy_decision_id),
                json_string_literal(APP_HEALTH_PROJECTION_ROUTE),
                json_string_literal(APP_HEALTH_SELF_HEAL_ROUTE),
            ),
        )),
        Err(error) => Ok(record_refusal(
            &mut guard,
            &resolved,
            error.code(),
            &error.field(),
            &error.marker(),
        )),
    }
}

/// Record a content-free refusal receipt and render the refusal response.
fn record_refusal(
    kernel: &mut MdxKernel,
    resolved: &crate::request_security::ResolvedWriteIdentity,
    code: &str,
    field: &str,
    marker: &str,
) -> RouteResponse {
    let report = kernel.record_app_health_refusal_with_identity(
        code,
        field,
        marker,
        &resolved.identity,
        &resolved.tenant_id,
        &resolved.actor_id,
    );
    RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-app-health-report-local-post","status":"APP_HEALTH_REFUSED","auth_session_required":true,"auth_session_status":{},"recorded":false,"refusal_code":{},"refused_field":{},"refused_marker":{},"refusal_receipt_id":{},"policy_decision_id":{},"message":{},"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            json_string_literal(&report.refusal_code),
            json_string_literal(&report.refused_field),
            json_string_literal(&report.refused_marker),
            json_string_literal(&report.refusal_receipt_id),
            json_string_literal(&report.policy_decision_id),
            json_string_literal(refusal_message(code)),
        ),
    )
}

fn refusal_message(code: &str) -> &'static str {
    match code {
        "unsafe_content" => {
            "That health report included content MDx will not record. Telemetry carries counts and route paths only."
        }
        "invalid_route" => "A failing route was not a plain path. MDx records route paths only.",
        "unknown_field" => "That health report included fields MDx does not accept.",
        "unknown_health_state" => "The health state was not one MDx recognizes.",
        "too_many_failing_routes" => "That health report listed too many failing routes.",
        "string_too_long" => "A health report value was too long.",
        "missing_field" => "Some required health fields were missing.",
        "body_too_large" => "That health report was too large.",
        "bad_body" => "That health report could not be read.",
        _ => "That health report could not be recorded.",
    }
}

fn handle_projection(
    method: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "GET") {
        return Ok(response);
    }
    let kernel = kernel
        .read()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    Ok(RouteResponse::json(
        "200 OK",
        render_projection_json(&kernel),
    ))
}

fn render_projection_json(kernel: &MdxKernel) -> String {
    let projection = kernel.app_health_projection();
    let top = projection
        .top_failing_routes
        .iter()
        .map(|row| {
            format!(
                r#"{{"route":{},"total_failure_count":{},"distinct_session_count":{},"last_status":{}}}"#,
                json_string_literal(&row.route),
                row.total_failure_count,
                row.distinct_session_count,
                row.last_status,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let recent = projection
        .recent_reports
        .iter()
        .map(|row| {
            format!(
                r#"{{"health_receipt_id":{},"surface":{},"session_ref":{},"reported_at":{},"health_state":{},"error_count":{},"hang_count":{},"unclean_relaunch":{},"failing_route_count":{}}}"#,
                json_string_literal(&row.receipt_id),
                json_string_literal(&row.surface),
                json_string_literal(&row.session_id),
                json_string_literal(&row.reported_at),
                json_string_literal(&row.health_state),
                row.error_count,
                row.hang_count,
                row.unclean_relaunch,
                row.failing_route_count,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"name":"mdx-app-health-local-projection","status":"OK","route":{},"writes_route":{},"self_heal_route":{},"total_reports":{},"distinct_sessions":{},"degraded_session_count":{},"unclean_relaunch_session_count":{},"unclean_relaunch_rate_pct":{},"hang_hotspot_session_count":{},"top_failing_routes":[{}],"recent_reports":[{}],"production_write_allowed":false}}"#,
        json_string_literal(APP_HEALTH_PROJECTION_ROUTE),
        json_string_literal(APP_HEALTH_ROUTE),
        json_string_literal(APP_HEALTH_SELF_HEAL_ROUTE),
        projection.total_reports,
        projection.distinct_sessions,
        projection.degraded_session_count,
        projection.unclean_relaunch_session_count,
        projection.unclean_relaunch_rate_pct,
        projection.hang_hotspot_session_count,
        top,
        recent,
    )
}

fn handle_self_heal_scan(
    method: &str,
    body: &str,
    kernel: &Arc<RwLock<MdxKernel>>,
) -> Result<RouteResponse, String> {
    if let Some(response) = crate::reject_unless_method(method, "POST") {
        return Ok(response);
    }
    let resolved = crate::request_security::resolve_governed_write_identity(
        body,
        "tenant_local",
        "local_user",
        "viewer",
    );
    let value: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let min_sessions = value
        .get("min_sessions")
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(APP_HEALTH_SELF_HEAL_DEFAULT_MIN_SESSIONS)
        .max(1);

    let mut guard = kernel
        .write()
        .map_err(|_| "kernel lock poisoned".to_string())?;
    // Candidates already EXCLUDE any route already synthesized, so the scan is
    // idempotent: a rerun over the same signal files nothing new.
    let all_candidates = guard.app_health_self_heal_candidates(min_sessions);
    // Per-scan cap: a flood of rollups (many distinct failing routes) must not
    // batch-grow the ledger through the bridge. The remainder is filed on a later
    // scan once these are cleared, so nothing is lost, only rate-limited.
    let candidate_count = all_candidates.len();
    let capped = candidate_count > APP_HEALTH_SELF_HEAL_MAX_PER_SCAN;
    let candidates: Vec<_> = all_candidates
        .into_iter()
        .take(APP_HEALTH_SELF_HEAL_MAX_PER_SCAN)
        .collect();

    let mut synthesized = Vec::new();
    for candidate in &candidates {
        let note = format!(
            "route {} returned status {} in {} sessions",
            candidate.route, candidate.last_status, candidate.distinct_session_count
        );
        let status_string = candidate.last_status.to_string();
        let occurred_at = if candidate.last_reported_at.trim().is_empty() {
            "app_health_self_heal_scan"
        } else {
            candidate.last_reported_at.as_str()
        };
        let context = [
            FeedbackContextEntry {
                key: "feature_area",
                value: APP_HEALTH_SELF_HEAL_FEATURE_AREA,
            },
            FeedbackContextEntry {
                key: "route_status",
                value: status_string.as_str(),
            },
        ];
        let capture = BetaFeedbackCapture {
            surface: APP_HEALTH_SELF_HEAL_SURFACE,
            route: candidate.route.as_str(),
            tenant_id: &resolved.tenant_id,
            actor_id: &resolved.actor_id,
            session_ref: "app_health_self_heal_scan",
            occurred_at,
            category: "bug",
            context: &context,
            note: &note,
        };
        // A synthesized capture is metrics-only; the existing safe-feedback
        // boundary still scans it. A failure here means a bug in candidate
        // derivation, so it surfaces rather than silently dropping.
        let report = guard
            .record_beta_feedback_local_with_identity(&capture, &resolved.identity)
            .map_err(|_| {
                "synthesized self-heal capture was refused by the feedback boundary".to_string()
            })?;
        synthesized.push((candidate.clone(), report.feedback_receipt_id));
    }

    // Record a scan receipt on EVERY invocation (even a no-op), so a governed
    // scan is always auditable and the response cites a receipt even when nothing
    // was synthesized. This is in addition to the per-capture receipts.
    let synthesized_ids: Vec<String> = synthesized
        .iter()
        .map(|(_, receipt_id)| receipt_id.clone())
        .collect();
    let scan = guard.record_app_health_self_heal_scan_with_identity(
        mdx_core::AppHealthSelfHealScanSummary {
            min_sessions,
            candidate_count,
            synthesized_count: synthesized.len(),
            capped,
            synthesized_receipt_ids: &synthesized_ids,
        },
        &resolved.identity,
        &resolved.tenant_id,
        &resolved.actor_id,
    );
    drop(guard);

    let synthesized_json = synthesized
        .iter()
        .map(|(candidate, receipt_id)| {
            format!(
                r#"{{"route":{},"last_status":{},"distinct_session_count":{},"total_failure_count":{},"feedback_receipt_id":{},"lane_expectation":"held_for_review"}}"#,
                json_string_literal(&candidate.route),
                candidate.last_status,
                candidate.distinct_session_count,
                candidate.total_failure_count,
                json_string_literal(receipt_id),
            )
        })
        .collect::<Vec<_>>()
        .join(",");

    Ok(RouteResponse::json(
        "200 OK",
        format!(
            r#"{{"name":"mdx-app-health-self-heal-scan-local-post","status":"SCANNED","auth_session_status":{},"min_sessions":{},"candidate_count":{},"max_per_scan":{},"capped":{},"synthesized_count":{},"synthesized":[{}],"scan_receipt_id":{},"policy_decision_id":{},"feedback_route":{},"autonomy_route":{},"note":"Synthesized captures are held for human review by the existing lane classifier; no Forge run starts here.","provider_call_allowed":false,"worker_spawn_allowed":false,"production_write_allowed":false}}"#,
            json_string_literal(resolved.auth_session_status),
            min_sessions,
            candidate_count,
            APP_HEALTH_SELF_HEAL_MAX_PER_SCAN,
            capped,
            synthesized.len(),
            synthesized_json,
            json_string_literal(&scan.scan_receipt_id),
            json_string_literal(&scan.policy_decision_id),
            json_string_literal(FEEDBACK_ROUTE),
            json_string_literal(FEEDBACK_AUTONOMY_ROUTE),
        ),
    ))
}

fn parse_failing_routes(value: &Value) -> Vec<(String, u16, u64)> {
    let Some(Value::Array(entries)) = value.get("failing_routes") else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            let object = entry.as_object()?;
            let route = object.get("route")?.as_str()?.to_string();
            let status = object
                .get("status")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                .min(u16::MAX as u64) as u16;
            let count = object.get("count").and_then(Value::as_u64).unwrap_or(0);
            Some((route, status, count))
        })
        .collect()
}

fn str_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn u64_field(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_status(response: &RouteResponse) -> serde_json::Value {
        serde_json::from_str(response.body_text()).expect("json body")
    }

    fn ingest(kernel: &Arc<RwLock<MdxKernel>>, body: &str) -> RouteResponse {
        handle_ingest("POST", body, kernel).expect("ingest")
    }

    fn boot() -> Arc<RwLock<MdxKernel>> {
        Arc::new(RwLock::new(MdxKernel::boot_local()))
    }

    const VALID: &str = r#"{"app_version":"2026.7.4","surface":"forge","session_id":"sess_a","reported_at":"2026-07-04T00:00:00Z","health_state":"degraded","total_calls":100,"error_count":5,"hang_count":1,"consecutive_failures":0,"avg_latency_ms":40,"p95_latency_ms":120,"launch_ms":800,"unclean_relaunch":false,"failing_routes":[{"route":"/forge/build-requests/projection.json","status":404,"count":3}]}"#;

    #[test]
    fn ingest_accepts_valid_metrics_only_rollup() {
        let kernel = boot();
        let response = ingest(&kernel, VALID);
        let json = body_status(&response);
        assert_eq!(json["status"], "APP_HEALTH_RECORDED");
        assert!(
            json["health_receipt_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
        // The receipt is in the ledger.
        let guard = kernel.read().unwrap();
        let count = guard
            .ledger()
            .entries()
            .iter()
            .filter(|r| r.kind == "app.health.reported")
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn ingest_refuses_forbidden_content_key_with_refusal_receipt() {
        let kernel = boot();
        // A forbidden content key (`secret`) is caught by the top-level scan
        // before any parse; it records a content-free refusal, never a report.
        let bad = r#"{"surface":"forge","session_id":"s","reported_at":"t","health_state":"healthy","secret":"leaked"}"#;
        let response = ingest(&kernel, bad);
        let json = body_status(&response);
        assert_eq!(json["status"], "APP_HEALTH_REFUSED");
        assert_eq!(json["refusal_code"], "unsafe_content");
        assert!(
            json["refusal_receipt_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
        let guard = kernel.read().unwrap();
        // Recorded a refusal, but NOT a health report.
        assert_eq!(
            guard
                .ledger()
                .entries()
                .iter()
                .filter(|r| r.kind == "app.health.reported")
                .count(),
            0
        );
        assert_eq!(
            guard
                .ledger()
                .entries()
                .iter()
                .filter(|r| r.kind == "app.health.refused")
                .count(),
            1
        );
    }

    #[test]
    fn ingest_refuses_unknown_top_level_key() {
        let kernel = boot();
        let bad = r#"{"surface":"forge","session_id":"s","reported_at":"t","health_state":"healthy","surprise":"x"}"#;
        let json = body_status(&ingest(&kernel, bad));
        assert_eq!(json["status"], "APP_HEALTH_REFUSED");
        assert_eq!(json["refusal_code"], "unknown_field");
    }

    #[test]
    fn ingest_bounds_oversized_body() {
        let kernel = boot();
        let big_route = "/x".repeat(20_000);
        let bad = format!(
            r#"{{"surface":"forge","session_id":"s","reported_at":"t","health_state":"healthy","failing_routes":[{{"route":"{big_route}","status":404,"count":1}}]}}"#
        );
        let json = body_status(&ingest(&kernel, &bad));
        assert_eq!(json["status"], "APP_HEALTH_REFUSED");
        assert_eq!(json["refusal_code"], "body_too_large");
    }

    #[test]
    fn self_heal_synthesizes_one_capture_and_is_idempotent() {
        let kernel = boot();
        // Three distinct sessions all failing the same route.
        for session in ["s1", "s2", "s3"] {
            let body = format!(
                r#"{{"app_version":"v","surface":"forge","session_id":"{session}","reported_at":"2026-07-04T00:00:00Z","health_state":"degraded","failing_routes":[{{"route":"/forge/talent-authorizations/projection.json","status":404,"count":1}}]}}"#
            );
            ingest(&kernel, &body);
        }
        // First scan synthesizes exactly one capture.
        let first = body_status(&handle_self_heal_scan("POST", "{}", &kernel).unwrap());
        assert_eq!(first["synthesized_count"], 1);
        assert_eq!(
            first["synthesized"][0]["route"],
            "/forge/talent-authorizations/projection.json"
        );
        // Rerun is idempotent: nothing new.
        let second = body_status(&handle_self_heal_scan("POST", "{}", &kernel).unwrap());
        assert_eq!(second["synthesized_count"], 0);

        // Exactly one telemetry-origin capture exists.
        let guard = kernel.read().unwrap();
        let telemetry_captures = guard
            .ledger()
            .entries()
            .iter()
            .filter(|r| r.kind == "beta.feedback.captured")
            .filter(|r| r.payload.get("surface").map(String::as_str) == Some("telemetry"))
            .count();
        assert_eq!(telemetry_captures, 1);
        // The hash-chain still verifies after ingest, refusal-free records, and
        // the synthesized capture.
        guard.ledger().verify().expect("ledger verifies");
    }

    #[test]
    fn self_heal_does_not_fire_below_threshold() {
        let kernel = boot();
        for session in ["s1", "s2"] {
            let body = format!(
                r#"{{"app_version":"v","surface":"forge","session_id":"{session}","reported_at":"t","health_state":"degraded","failing_routes":[{{"route":"/forge/x.json","status":404,"count":1}}]}}"#
            );
            ingest(&kernel, &body);
        }
        // Default threshold is 3; two sessions must not synthesize.
        let json = body_status(&handle_self_heal_scan("POST", "{}", &kernel).unwrap());
        assert_eq!(json["synthesized_count"], 0);
        // Even a no-op scan is a governed write: it MUST cite a scan receipt so the
        // route is auditable (receipts-or-it-didn't-happen).
        assert!(
            json["scan_receipt_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
        let guard = kernel.read().unwrap();
        assert_eq!(
            guard
                .ledger()
                .entries()
                .iter()
                .filter(|r| r.kind == "app.health.self_heal.scanned")
                .count(),
            1
        );
    }

    #[test]
    fn empty_kernel_scan_still_cites_a_scan_receipt() {
        // The runtime-surface smoke path: a scan on a fresh kernel synthesizes
        // nothing yet must still cite a receipt.
        let kernel = boot();
        let json = body_status(&handle_self_heal_scan("POST", "{}", &kernel).unwrap());
        assert_eq!(json["candidate_count"], 0);
        assert_eq!(json["synthesized_count"], 0);
        assert!(
            json["scan_receipt_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
    }

    #[test]
    fn exploit_route_is_held_end_to_end_not_auto_run() {
        // THE exploit the adversarial review found: a crafted route whose synthesized
        // note contains the auto-run alphabet ("copy") would, without the surface
        // short-circuit, classify autopilot_allowed + resolve a real write scope +
        // mint an auto-approved Forge run. Prove the whole loop now HOLDS it.
        let kernel = boot();
        let exploit = "/feedback/copy-fix.json";
        for session in ["s1", "s2", "s3"] {
            let body = format!(
                r#"{{"app_version":"v","surface":"forge","session_id":"{session}","reported_at":"2026-07-04T00:00:00Z","health_state":"degraded","failing_routes":[{{"route":"{exploit}","status":404,"count":1}}]}}"#
            );
            ingest(&kernel, &body);
        }
        let scan = body_status(&handle_self_heal_scan("POST", "{}", &kernel).unwrap());
        assert_eq!(scan["synthesized_count"], 1);
        assert_eq!(scan["synthesized"][0]["route"], exploit);

        // The synthesized note carries the crafted route verbatim, yet the
        // classifier holds it because the surface is telemetry.
        let note = format!("route {exploit} returned status 404 in 3 sessions");
        let (lane, _) = mdx_core::classify_feedback_for_autonomy(&note, "bug", "telemetry");
        assert_eq!(lane, "needs_human");
        assert_ne!(lane, "autopilot_allowed");

        // Run the ACTUAL autonomy loop over the synthesized capture: it must NOT
        // mint a Forge run (empty forge_run_ref) and must land in needs_human.
        let response = crate::feedback_autonomy_route::route_response(
            "POST",
            "/feedback/autonomy-runs.json",
            "{}",
            &kernel,
        )
        .expect("route matched")
        .expect("autonomy run");
        let json: serde_json::Value =
            serde_json::from_str(response.body_text()).expect("autonomy json");
        let processed = json["processed"].as_array().expect("processed array");
        let row = processed
            .iter()
            .find(|row| row["lane"].is_string())
            .expect("a processed row");
        assert_eq!(row["lane"], "needs_human");
        assert_eq!(row["forge_run_ref"], "");
        assert!(
            row["write_scope"].as_array().map(Vec::len).unwrap_or(0) == 0,
            "telemetry-origin capture must resolve no write scope"
        );
    }

    #[test]
    fn benign_synthesized_note_is_also_held() {
        // A benign metrics note on the telemetry surface is held too.
        let note = "route /forge/build-requests/projection.json returned status 404 in 5 sessions";
        let (lane, _) = mdx_core::classify_feedback_for_autonomy(note, "bug", "telemetry");
        assert_eq!(lane, "needs_human");
    }

    #[test]
    fn dedup_normalizes_route_case_and_trailing_slash() {
        // /x, /x/, and /X across three distinct sessions are ONE route, so the
        // scan synthesizes exactly one capture, not three.
        let kernel = boot();
        let variants = [
            ("s1", "/forge/x.json"),
            ("s2", "/forge/x.json/"),
            ("s3", "/forge/X.json"),
        ];
        for (session, route) in variants {
            let body = format!(
                r#"{{"app_version":"v","surface":"forge","session_id":"{session}","reported_at":"t","health_state":"degraded","failing_routes":[{{"route":"{route}","status":404,"count":1}}]}}"#
            );
            ingest(&kernel, &body);
        }
        let json = body_status(&handle_self_heal_scan("POST", "{}", &kernel).unwrap());
        assert_eq!(json["synthesized_count"], 1);
        let guard = kernel.read().unwrap();
        let telemetry_captures = guard
            .ledger()
            .entries()
            .iter()
            .filter(|r| r.kind == "beta.feedback.captured")
            .filter(|r| r.payload.get("surface").map(String::as_str) == Some("telemetry"))
            .count();
        assert_eq!(telemetry_captures, 1);
    }

    #[test]
    fn per_scan_cap_bounds_synthesis() {
        // Many distinct failing routes, each over threshold: one scan files at most
        // the cap, and reports the overflow honestly.
        let kernel = boot();
        let route_count = APP_HEALTH_SELF_HEAL_MAX_PER_SCAN + 3;
        for session in ["s1", "s2", "s3"] {
            let routes: Vec<String> = (0..route_count)
                .map(|i| format!(r#"{{"route":"/forge/r{i}.json","status":404,"count":1}}"#))
                .collect();
            let body = format!(
                r#"{{"app_version":"v","surface":"forge","session_id":"{session}","reported_at":"t","health_state":"degraded","failing_routes":[{}]}}"#,
                routes.join(",")
            );
            ingest(&kernel, &body);
        }
        let json = body_status(&handle_self_heal_scan("POST", "{}", &kernel).unwrap());
        assert_eq!(json["candidate_count"], route_count);
        assert_eq!(json["synthesized_count"], APP_HEALTH_SELF_HEAL_MAX_PER_SCAN);
        assert_eq!(json["capped"], true);
    }

    #[test]
    fn ingest_rejects_a_route_outside_the_strict_charset() {
        // A hyphenated slug is fine, but a route with disallowed punctuation (an
        // exfil channel) is refused.
        let kernel = boot();
        let bad = r#"{"app_version":"v","surface":"forge","session_id":"s","reported_at":"t","health_state":"healthy","failing_routes":[{"route":"/forge/a+b=c%20d.json","status":404,"count":1}]}"#;
        let json = body_status(&ingest(&kernel, bad));
        assert_eq!(json["status"], "APP_HEALTH_REFUSED");
        assert_eq!(json["refusal_code"], "invalid_route");
    }

    #[test]
    fn ingest_rejects_an_overlong_route() {
        let kernel = boot();
        let long_route = format!("/forge/{}.json", "a".repeat(200));
        let bad = format!(
            r#"{{"app_version":"v","surface":"forge","session_id":"s","reported_at":"t","health_state":"healthy","failing_routes":[{{"route":"{long_route}","status":404,"count":1}}]}}"#
        );
        let json = body_status(&ingest(&kernel, &bad));
        assert_eq!(json["status"], "APP_HEALTH_REFUSED");
        assert_eq!(json["refusal_code"], "string_too_long");
    }

    #[test]
    fn projection_aggregates_across_two_sessions() {
        let kernel = boot();
        for session in ["sess_a", "sess_b"] {
            let body = format!(
                r#"{{"app_version":"v","surface":"forge","session_id":"{session}","reported_at":"t","health_state":"degraded","failing_routes":[{{"route":"/forge/build-requests/projection.json","status":404,"count":2}}]}}"#
            );
            ingest(&kernel, &body);
        }
        let json = body_status(&handle_projection("GET", &kernel).unwrap());
        assert_eq!(json["status"], "OK");
        assert_eq!(json["total_reports"], 2);
        assert_eq!(json["distinct_sessions"], 2);
        assert_eq!(json["degraded_session_count"], 2);
        assert_eq!(
            json["top_failing_routes"][0]["route"],
            "/forge/build-requests/projection.json"
        );
        assert_eq!(json["top_failing_routes"][0]["distinct_session_count"], 2);
        assert_eq!(json["top_failing_routes"][0]["total_failure_count"], 4);
    }
}
