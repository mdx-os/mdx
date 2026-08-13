import Foundation

/// One failing app route in a health rollup: a bare route path, the HTTP status
/// it returned, and how many times it failed this session. Metrics only, never
/// content. Maps 1:1 onto the kernel's `failing_routes[]` allowlist
/// (`{route, status, count}`), so the central ingest never refuses it.
struct AppHealthFailingRoute: Equatable {
  let route: String
  let status: Int
  let count: Int
}

/// The batched, METRICS-ONLY health rollup the app POSTs to the central ingest
/// (`POST /telemetry/app-health.json`). Every field maps to the server's
/// top-level allowlist; nothing else is ever attached, so a rollup can carry
/// counts, durations, a bounded health state, and non-secret bare route paths,
/// but never a note, body, prompt, secret, or token.
struct AppHealthRollupPayload {
  let appVersion: String
  let surface: String
  let sessionID: String
  let reportedAt: String
  let healthState: String
  let totalCalls: Int
  let errorCount: Int
  let hangCount: Int
  let consecutiveFailures: Int
  let avgLatencyMs: Int
  let p95LatencyMs: Int
  let launchMs: Int
  let uncleanRelaunch: Bool
  let failingRoutes: [AppHealthFailingRoute]
}

/// A failing route aggregated across every session that reported it, read back
/// from the central projection. This is the "where users hit trouble" row.
struct AppHealthRouteHotspot: Identifiable, Equatable {
  var id: String { route }
  let route: String
  let totalFailureCount: Int
  let distinctSessionCount: Int
  let lastStatus: Int
}

/// A recent app-health report in the cross-session projection.
struct AppHealthRecentReport: Identifiable, Equatable {
  var id: String { receiptID }
  let receiptID: String
  let surface: String
  let healthState: String
  let errorCount: Int
  let hangCount: Int
  let uncleanRelaunch: Bool
  let failingRouteCount: Int
  let reportedAt: String
}

/// The cross-session app-health aggregate the Diagnostics surface reads: where
/// users hit trouble, the crash/unclean-relaunch rate, hang hotspots, and how
/// many sessions ran degraded. Read-only; the kernel owns the truth.
struct AppHealthProjection: Equatable {
  let totalReports: Int
  let distinctSessions: Int
  let degradedSessionCount: Int
  let uncleanRelaunchSessionCount: Int
  let uncleanRelaunchRatePct: Int
  let hangHotspotSessionCount: Int
  let topFailingRoutes: [AppHealthRouteHotspot]
  let recentReports: [AppHealthRecentReport]

  static let empty = AppHealthProjection(
    totalReports: 0,
    distinctSessions: 0,
    degradedSessionCount: 0,
    uncleanRelaunchSessionCount: 0,
    uncleanRelaunchRatePct: 0,
    hangHotspotSessionCount: 0,
    topFailingRoutes: [],
    recentReports: []
  )
}

extension AppHealth {
  /// The kernel's `health_state` wire value (one of healthy | degraded | offline).
  var wireState: String {
    switch self {
    case .healthy: return "healthy"
    case .degraded: return "degraded"
    case .offline: return "offline"
    }
  }
}
