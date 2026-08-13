import Foundation
import Observation
import OSLog

/// Central observability: os.Logger categories, os_signpost intervals for
/// Instruments, cold-launch timing, a main-thread hang monitor, and a
/// lightweight in-app diagnostics recorder. Recording is cheap and thread-safe
/// so it can wrap every network call, stream, and navigation.
enum Telemetry {
  static let subsystem = "com.mdx.app"
  static let network = Logger(subsystem: subsystem, category: "network")
  static let ui = Logger(subsystem: subsystem, category: "ui")
  static let store = Logger(subsystem: subsystem, category: "store")
  static let signposter = OSSignposter(subsystem: subsystem, category: "perf")

  // MARK: - Route / action timing

  /// Measure an async operation: emit a signpost interval visible in
  /// Instruments and record its latency to the in-app diagnostics ring buffer.
  static func measure<T>(
    _ label: String,
    kind: DiagnosticEvent.Kind = .route,
    _ operation: () async throws -> T
  ) async rethrows -> T {
    let signpostID = signposter.makeSignpostID()
    let state = signposter.beginInterval("op", id: signpostID, "\(label)")
    let start = DispatchTime.now()
    func elapsedMs() -> Double { Self.elapsedMs(since: start) }
    do {
      let result = try await operation()
      let ms = elapsedMs()
      signposter.endInterval("op", state, "ok")
      if ms > 800 { network.notice("slow \(label, privacy: .public): \(Int(ms))ms") }
      DiagnosticsRecorder.record(.init(at: Date(), label: label, kind: kind, durationMs: ms, ok: true, detail: ""))
      return result
    } catch {
      let ms = elapsedMs()
      signposter.endInterval("op", state, "error")
      // Separate "the kernel is down" from "the kernel answered with an error".
      // A connection-refused/timeout while the engine is off is an offline state,
      // not a per-route failure; recording it as an error is what inflated the
      // count to the thousands during an offline launch. It still lands in the
      // trail (honest), just classified so the Errors metric stays real.
      let unreachable = Self.isUnreachable(error)
      if unreachable {
        network.notice("\(label, privacy: .public) unreachable after \(Int(ms))ms: \(error.localizedDescription, privacy: .public)")
      } else {
        network.error("\(label, privacy: .public) failed after \(Int(ms))ms: \(error.localizedDescription, privacy: .public)")
      }
      DiagnosticsRecorder.record(.init(at: Date(), label: label, kind: kind, durationMs: ms, ok: false, detail: error.localizedDescription, unreachable: unreachable))
      throw error
    }
  }

  /// A failure means "the kernel is unreachable" only when the transport could
  /// not reach the host at all (connection refused, host/DNS failure, dropped
  /// connection, timeout). Any answer from the kernel, including an HTTP error
  /// status or a malformed body, is a real route failure, not an offline state.
  static func isUnreachable(_ error: Error) -> Bool {
    guard let urlError = error as? URLError else { return false }
    switch urlError.code {
    case .cannotConnectToHost, .cannotFindHost, .dnsLookupFailed,
         .notConnectedToInternet, .networkConnectionLost, .timedOut:
      return true
    default:
      return false
    }
  }

  // MARK: - Breadcrumbs

  /// A lightweight trail of what the user did, folded into bug reports.
  static func breadcrumb(_ message: String, _ category: String = "nav") {
    ui.log("• \(category, privacy: .public): \(message, privacy: .public)")
    DiagnosticsRecorder.record(.init(at: Date(), label: message, kind: .breadcrumb, durationMs: 0, ok: true, detail: category))
  }

  // MARK: - Cold launch

  private static let launchLock = NSLock()
  nonisolated(unsafe) private static var launchStart: DispatchTime?
  nonisolated(unsafe) private static var launchMarked = false

  static func markLaunch() {
    launchLock.lock(); defer { launchLock.unlock() }
    if launchStart == nil { launchStart = .now() }
  }

  /// Called once when the first data is on screen. Records app cold-start time.
  static func markReady(_ label: String = "first data") {
    launchLock.lock()
    guard let start = launchStart, !launchMarked else { launchLock.unlock(); return }
    launchMarked = true
    launchLock.unlock()
    let ms = elapsedMs(since: start)
    ui.notice("cold launch to \(label, privacy: .public): \(Int(ms))ms")
    DiagnosticsRecorder.record(.init(at: Date(), label: "Cold launch to \(label)", kind: .launch, durationMs: ms, ok: true, detail: ""))
    Task { @MainActor in DiagnosticsStore.shared.launchDurationMs = ms }
  }

  // MARK: - Helpers

  static func elapsedMs(since start: DispatchTime) -> Double {
    Double(DispatchTime.now().uptimeNanoseconds &- start.uptimeNanoseconds) / 1_000_000
  }
}

// MARK: - Stream probe

/// Tracks a streaming session (Twin answer, Forge run feed): time-to-first-token,
/// total duration, delta throughput, and terminal outcome.
final class StreamProbe {
  private let label: String
  private let start = DispatchTime.now()
  private var firstTokenAt: DispatchTime?
  private var deltas = 0
  private let signpostID: OSSignpostID
  private let state: OSSignpostIntervalState

  init(_ label: String) {
    self.label = label
    self.signpostID = Telemetry.signposter.makeSignpostID()
    self.state = Telemetry.signposter.beginInterval("stream", id: signpostID, "\(label)")
  }

  /// Call on each streamed token/event.
  func tick() {
    deltas += 1
    if firstTokenAt == nil {
      firstTokenAt = .now()
      Telemetry.signposter.emitEvent("first-token", id: signpostID)
    }
  }

  /// Call once when the stream ends (completed / cancelled / guarded / failed).
  func finish(_ outcome: String, ok: Bool = true) {
    let totalMs = Telemetry.elapsedMs(since: start)
    let ttftMs = firstTokenAt.map { Telemetry.elapsedMs(since: start) - Telemetry.elapsedMs(since: $0) } ?? 0
    Telemetry.network.notice("stream \(self.label, privacy: .public) outcome=\(outcome, privacy: .public) ttft=\(Int(ttftMs))ms total=\(Int(totalMs))ms deltas=\(self.deltas)")
    Telemetry.signposter.endInterval("stream", state, "\(outcome)")
    let capturedDeltas = deltas
    let event = DiagnosticEvent(
      at: Date(), label: "\(label) · \(outcome)", kind: .stream,
      durationMs: totalMs, ok: ok, detail: "\(capturedDeltas) tokens",
      ttftMs: ttftMs, count: capturedDeltas
    )
    DiagnosticsRecorder.record(event)
  }
}

// MARK: - Hang monitor

/// Background watchdog that measures main-thread responsiveness. If a dispatch
/// to the main queue takes longer than the threshold, the UI hung and we record
/// it (macOS has no MetricKit hang metric, so we roll our own).
final class HangMonitor: @unchecked Sendable {
  static let shared = HangMonitor()

  private let queue = DispatchQueue(label: "com.mdx.app.hangmonitor", qos: .utility)
  private var timer: DispatchSourceTimer?
  private var suspended = false
  private var consecutiveSlowProbes = 0
  private let probeInterval: TimeInterval = 0.5
  // A single delayed frame is not a user-visible hang. AppKit can legitimately
  // hold the main thread while presenting a native panel, restoring a window,
  // or handing focus between apps. Require sustained unresponsiveness before
  // degrading the product shell.
  private let hangThresholdMs: Double = 500

  func start() {
    guard timer == nil else { return }
    let source = DispatchSource.makeTimerSource(queue: queue)
    source.schedule(deadline: .now() + probeInterval, repeating: probeInterval)
    source.setEventHandler { [weak self] in self?.probe() }
    timer = source
    source.resume()
  }

  /// Nothing can visibly hang while the app is inactive; stop burning a
  /// wakeup every half second in the background.
  func setPaused(_ paused: Bool) {
    queue.async { [weak self] in
      guard let self, let timer = self.timer else { return }
      if paused, !self.suspended {
        timer.suspend()
        self.suspended = true
      } else if !paused, self.suspended {
        timer.resume()
        self.suspended = false
      }
    }
  }

  private func probe() {
    let sent = DispatchTime.now()
    let semaphore = DispatchSemaphore(value: 0)
    DispatchQueue.main.async { semaphore.signal() }
    // Cap the wait so a deep hang doesn't wedge the monitor thread.
    _ = semaphore.wait(timeout: .now() + 3.0)
    let ms = Telemetry.elapsedMs(since: sent)
    if ms > hangThresholdMs {
      consecutiveSlowProbes += 1
    } else {
      consecutiveSlowProbes = 0
    }
    if consecutiveSlowProbes >= 2 {
      consecutiveSlowProbes = 0
      Telemetry.ui.error("main thread hang: \(Int(ms))ms")
      Telemetry.signposter.emitEvent("hang", id: Telemetry.signposter.makeSignpostID())
      DiagnosticsRecorder.record(.init(at: Date(), label: "UI hang", kind: .hang, durationMs: ms, ok: false, detail: ""))
    }
  }
}

// MARK: - Event model + store

enum AppHealth: Equatable {
  case healthy, degraded, offline

  var label: String {
    switch self {
    case .healthy: return "Connected"
    case .degraded: return "Degraded"
    case .offline: return "Offline"
    }
  }

  var detail: String {
    switch self {
    case .healthy: return "Local kernel reachable. No recent hangs or errors."
    case .degraded: return "Recent hangs or errors. Open Diagnostics for detail."
    case .offline: return "Local kernel is unreachable. Retrying."
    }
  }
}

struct DiagnosticEvent: Identifiable, Equatable {
  enum Kind: String, CaseIterable { case route, action, stream, launch, hang, breadcrumb, error }

  let id = UUID()
  let at: Date
  let label: String
  let kind: Kind
  let durationMs: Double
  let ok: Bool
  let detail: String
  var ttftMs: Double = 0
  var count: Int = 0
  /// The kernel could not be reached (connection refused, timeout). Recorded so
  /// the trail stays honest, but never counted as a route error.
  var unreachable: Bool = false

  var latencyLabel: String { DiagnosticEvent.durationLabel(durationMs) }

  /// Human duration for the diagnostics trail. Sub-second work reads in
  /// milliseconds; a multi-second stream reads in seconds; a stream that ran for
  /// minutes reads as "2m 37s" instead of a raw "156984ms" that overflows the
  /// column and truncates to a meaningless "156984m".
  static func durationLabel(_ ms: Double) -> String {
    if ms < 1 { return "<1ms" }
    if ms < 1_000 { return "\(Int(ms))ms" }
    let totalSeconds = Int((ms / 1_000).rounded())
    if totalSeconds < 60 { return "\(totalSeconds)s" }
    let minutes = totalSeconds / 60
    let seconds = totalSeconds % 60
    return "\(minutes)m \(seconds)s"
  }
}

/// Thread-safe hand-off from any actor into the main-actor diagnostics store.
enum DiagnosticsRecorder {
  static func record(_ event: DiagnosticEvent) {
    Task { @MainActor in DiagnosticsStore.shared.append(event) }
  }
}

struct SessionSummary: Codable, Equatable {
  var launchMs: Double?
  var calls: Int
  var errors: Int
  var hangs: Int
  var avgMs: Double
  var p95Ms: Double
  var avgTTFTms: Double
  var endedAt: Date
}

@Observable
@MainActor
final class DiagnosticsStore {
  static let shared = DiagnosticsStore()

  private(set) var events: [DiagnosticEvent] = []
  private(set) var totalCalls = 0
  private(set) var errorCount = 0
  private(set) var hangCount = 0
  private(set) var consecutiveFailures = 0
  var launchDurationMs: Double?
  private(set) var lastSession: SessionSummary?

  let sessionStart = Date()
  private let maxEvents = 400
  private let lastSessionKey = "MDxLastSession"

  init() {
    if let data = UserDefaults.standard.data(forKey: lastSessionKey),
       let summary = try? JSONDecoder().decode(SessionSummary.self, from: data) {
      lastSession = summary
    }
  }

  func append(_ event: DiagnosticEvent) {
    events.insert(event, at: 0)
    if events.count > maxEvents { events.removeLast(events.count - maxEvents) }
    if event.kind == .route || event.kind == .action {
      totalCalls += 1
      // A streak of failed connection attempts keeps driving the offline
      // diagnosis ("N checks failed in a row"), whether the kernel refused or
      // errored; a success clears it.
      consecutiveFailures = event.ok ? 0 : consecutiveFailures + 1
    }
    if event.kind == .hang { hangCount += 1 }
    // Errors are real route failures only. Kernel-unreachable events are an
    // offline state, tracked separately, not a per-route error.
    if !event.ok && !event.unreachable { errorCount += 1 }
    refreshHealthSignal()
  }

  // MARK: - Health signals (drive the app health indicator)

  /// The condensed signal the sidebar health pill reads. Stored and updated
  /// only-on-change so the pill does not re-render on every recorded event.
  struct HealthSignal: Equatable {
    var consecutiveFailures = 0
    var recentHang = false
    var recentErrors = 0
  }

  private(set) var health = HealthSignal()

  private func refreshHealthSignal() {
    let next = HealthSignal(
      consecutiveFailures: consecutiveFailures,
      recentHang: recentHang,
      recentErrors: recentErrors
    )
    if next != health { health = next }
  }

  /// A hang recorded in the last two minutes.
  var recentHang: Bool {
    guard let hang = events.first(where: { $0.kind == .hang }) else { return false }
    return Date().timeIntervalSince(hang.at) < 120
  }

  /// Real failures in the recent operating window. Kernel-unreachable events
  /// are excluded so a spell offline never leaves the app reading "degraded"
  /// once it reconnects, and an old recovered route error cannot stay amber for
  /// the rest of a long session.
  var recentErrors: Int {
    let cutoff = Date().addingTimeInterval(-120)
    return routeEvents.prefix(20).filter { $0.at >= cutoff && !$0.ok && !$0.unreachable }.count
  }

  /// The most recent failed route/action, for an honest offline diagnosis. Real
  /// label and detail from the event trail, never a cosmetic placeholder.
  struct FailureNote: Equatable {
    let label: String
    let detail: String
    let at: Date
  }

  var lastFailure: FailureNote? {
    guard let event = events.first(where: { !$0.ok }) else { return nil }
    return FailureNote(label: event.label, detail: event.detail, at: event.at)
  }

  // MARK: - Session persistence

  func snapshotSummary() -> SessionSummary {
    SessionSummary(
      launchMs: launchDurationMs, calls: totalCalls, errors: errorCount, hangs: hangCount,
      avgMs: avgLatencyMs, p95Ms: p95LatencyMs, avgTTFTms: avgTTFTms, endedAt: Date()
    )
  }

  func persistSession() {
    guard totalCalls > 0 else { return }
    if let data = try? JSONEncoder().encode(snapshotSummary()) {
      UserDefaults.standard.set(data, forKey: lastSessionKey)
    }
  }

  // MARK: - Failing-route rollup (central telemetry)

  /// Aggregate failed route/action calls in the ring buffer into
  /// `(route, status, count)`, most-failed first, capped. The route is the bare
  /// path from the event label ("GET /forge/x.json" -> "/forge/x.json"); the
  /// status is parsed from the HTTP error detail ("Route returned HTTP 404." ->
  /// 404), or 0 when the failure was not an HTTP status (a timeout or transport
  /// error). Metrics only: nothing but a bare path and a status leaves here, so
  /// the central ingest never refuses the rollup.
  func failingRouteRollups(limit: Int = 50) -> [(route: String, status: Int, count: Int)] {
    var counts: [String: (route: String, status: Int, count: Int)] = [:]
    // Real route failures only. Offline probes (status 0, kernel unreachable) are
    // not a per-route problem, so they never ride into the central rollup.
    for event in routeEvents where !event.ok && !event.unreachable {
      guard let route = Self.routePath(fromLabel: event.label) else { continue }
      let status = Self.httpStatus(fromDetail: event.detail)
      let key = "\(route)#\(status)"
      if var existing = counts[key] {
        existing.count += 1
        counts[key] = existing
      } else {
        counts[key] = (route, status, 1)
      }
    }
    return counts.values
      .sorted { lhs, rhs in
        lhs.count != rhs.count ? lhs.count > rhs.count : lhs.route < rhs.route
      }
      .prefix(limit)
      .map { $0 }
  }

  /// Extract the bare route path from an event label. Only bare paths qualify;
  /// anything with a scheme, query, fragment, or whitespace is dropped so the
  /// value-level ingest guard is never tripped.
  static func routePath(fromLabel label: String) -> String? {
    let parts = label.split(separator: " ", maxSplits: 1)
    guard parts.count == 2 else { return nil }
    let path = String(parts[1])
    guard path.hasPrefix("/"),
          !path.contains("?"),
          !path.contains("#"),
          !path.contains("://"),
          !path.contains(where: { $0.isWhitespace })
    else { return nil }
    return path
  }

  /// Parse the numeric HTTP status out of an error detail like
  /// "Route returned HTTP 404.". Returns 0 when there is no HTTP status.
  static func httpStatus(fromDetail detail: String) -> Int {
    guard let range = detail.range(of: "HTTP ") else { return 0 }
    let digits = detail[range.upperBound...].prefix { $0.isNumber }
    return Int(digits) ?? 0
  }

  var routeEvents: [DiagnosticEvent] { events.filter { $0.kind == .route || $0.kind == .action } }
  var streamEvents: [DiagnosticEvent] { events.filter { $0.kind == .stream } }
  var breadcrumbs: [DiagnosticEvent] { events.filter { $0.kind == .breadcrumb } }

  func events(of kind: DiagnosticEvent.Kind?) -> [DiagnosticEvent] {
    guard let kind else { return events }
    return events.filter { $0.kind == kind }
  }

  var avgLatencyMs: Double { mean(routeEvents.map(\.durationMs)) }
  var p95LatencyMs: Double { percentile(routeEvents.map(\.durationMs), 0.95) }
  var avgTTFTms: Double { mean(streamEvents.filter { $0.ttftMs > 0 }.map(\.ttftMs)) }
  var slowest: DiagnosticEvent? { routeEvents.max { $0.durationMs < $1.durationMs } }

  var uptimeLabel: String {
    let seconds = Int(Date().timeIntervalSince(sessionStart))
    if seconds < 60 { return "\(seconds)s" }
    return "\(seconds / 60)m \(seconds % 60)s"
  }

  func clear() {
    events = []
    totalCalls = 0
    errorCount = 0
    hangCount = 0
    consecutiveFailures = 0
    refreshHealthSignal()
  }

  /// A copyable text summary for bug reports: session header, breadcrumbs, events.
  func exportText() -> String {
    var lines = ["MDx diagnostics"]
    let launch = launchDurationMs.map { "\(Int($0))ms" } ?? "n/a"
    lines.append("launch=\(launch) uptime=\(uptimeLabel) calls=\(totalCalls) errors=\(errorCount) hangs=\(hangCount)")
    lines.append("latency avg=\(Int(avgLatencyMs))ms p95=\(Int(p95LatencyMs))ms · stream ttft avg=\(Int(avgTTFTms))ms")
    lines.append("")
    let crumbs = breadcrumbs.prefix(12)
    if !crumbs.isEmpty {
      lines.append("recent trail:")
      for crumb in crumbs { lines.append("  \(crumb.detail): \(crumb.label)") }
      lines.append("")
    }
    lines.append("recent events:")
    for event in events.prefix(80) where event.kind != .breadcrumb {
      let flag = event.ok ? "ok " : "ERR"
      let extra = event.kind == .stream ? " ttft=\(Int(event.ttftMs))ms" : ""
      lines.append("\(flag) \(event.latencyLabel.padding(toLength: 8, withPad: " ", startingAt: 0)) [\(event.kind.rawValue)] \(event.label)\(extra) \(event.detail)")
    }
    return lines.joined(separator: "\n")
  }

  private func mean(_ values: [Double]) -> Double {
    guard !values.isEmpty else { return 0 }
    return values.reduce(0, +) / Double(values.count)
  }

  private func percentile(_ values: [Double], _ p: Double) -> Double {
    let sorted = values.sorted()
    guard !sorted.isEmpty else { return 0 }
    return sorted[min(sorted.count - 1, Int(Double(sorted.count) * p))]
  }
}
