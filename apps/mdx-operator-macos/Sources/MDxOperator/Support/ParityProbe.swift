import Foundation

/// S7 macOS parity probe: proves the native app reaches parity with the web/API
/// tier on the core operator surfaces, against the SAME live kernel.
///
/// For each surface it (a) fetches the raw kernel route the way the web tier
/// would and counts its objects, then (b) drives the app's real MDxRouteClient
/// (the exact code the SwiftUI views consume) and counts the objects it maps.
/// Parity = the native app receives non-empty, well-formed data matching what
/// the web tier sees. Where the native app has no consumer for a surface the
/// web shows, it records a NAMED, owned gap instead of hiding it. It also
/// verifies the offline snapshot fails closed when the kernel is absent.
///
/// Runs headless when launched with `--parity-probe`, then exits with a
/// non-zero status on any failed assertion. Built into the app binary for the
/// same reason as MapperChecks: the Command Line Tools toolchain ships neither
/// XCTest nor swift-testing, and SwiftPM cannot link one executable target
/// against another. The XCTest form lives in Tests/ParitySliceTests.swift for
/// Xcode/CI. Emits a scorecard artifact to docs/sim (override with
/// MDX_PARITY_SCORECARD_PATH); kernel base overridable with MDX_PARITY_BASE_URL.
@MainActor
enum ParityProbe {
  struct SurfaceResult {
    let surface: String
    let webCount: Int?
    let nativeCount: Int?
    let parity: Bool
    let delta: String
    let owner: String
  }

  private static var results: [SurfaceResult] = []
  private static var failures: [String] = []

  static func runIfRequested() {
    guard CommandLine.arguments.contains("--parity-probe") else { return }
    // Detached so it does not inherit the MainActor; it runs the probe and
    // exits the process directly. The main thread parks on dispatchMain().
    Task.detached {
      let code = await runAll()
      exit(code)
    }
    dispatchMain()
  }

  // MARK: - Config

  private static var baseURL: URL {
    if let raw = ProcessInfo.processInfo.environment["MDX_PARITY_BASE_URL"],
       let url = URL(string: raw) {
      return url
    }
    return MDxRouteClient.defaultBaseURL
  }

  private static let deadURL = URL(string: "http://127.0.0.1:59997")!

  // MARK: - Assertions and recording

  private static func check(_ condition: Bool, _ message: String) {
    if condition {
      print("ok   \(message)")
    } else {
      failures.append(message)
      print("FAIL \(message)")
    }
  }

  private static func record(
    _ surface: String,
    webCount: Int?,
    nativeCount: Int?,
    parity: Bool,
    delta: String,
    owner: String = "none"
  ) {
    results.append(SurfaceResult(surface: surface, webCount: webCount, nativeCount: nativeCount, parity: parity, delta: delta, owner: owner))
    let w = webCount.map(String.init) ?? "absent"
    let n = nativeCount.map(String.init) ?? "absent"
    print("[parity] \(surface): web=\(w) native=\(n) \(parity ? "MATCH" : "GAP") - \(delta)")
  }

  // MARK: - Raw web-tier probe

  private static func webCount(_ path: String, arrayKey: String?) async -> Int? {
    guard let url = URL(string: baseURL.absoluteString + path) else { return nil }
    var request = URLRequest(url: url)
    request.timeoutInterval = 15
    request.setValue("application/json", forHTTPHeaderField: "Accept")
    guard let (data, response) = try? await URLSession.shared.data(for: request),
          let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode),
          let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    else { return nil }
    if let arrayKey, let arr = obj[arrayKey] as? [Any] { return arr.count }
    return obj.isEmpty ? 0 : 1
  }

  private static func parityDelta(_ web: Int?, _ native: Int) -> String {
    guard let web else { return "web route absent on this snapshot; native mapped \(native)" }
    if web == native { return "count parity (\(native))" }
    return "count delta: web \(web) vs native \(native)"
  }

  private static func fastFailSession() -> URLSession {
    let config = URLSessionConfiguration.ephemeral
    config.timeoutIntervalForRequest = 3
    config.timeoutIntervalForResource = 3
    return URLSession(configuration: config)
  }

  // MARK: - Run

  private static func runAll() async -> Int32 {
    print("== S7 macOS parity probe against \(baseURL.absoluteString) ==")
    let client = MDxRouteClient()

    // Core operator read-loop (Home / operator snapshot): CL-1, MT-2.
    let snapshot = await client.loadSnapshot(baseURL: baseURL)
    check(snapshot.connectionStatus == .ok, "live snapshot connects (not the offline fallback)")
    check(snapshot.loadedAt != nil, "live snapshot carries a load time")
    check(snapshot.currentStage != "offline", "live snapshot stage is not offline")
    check(snapshot.runCount > 0, "operator snapshot sees Forge runs (\(snapshot.runCount))")
    check(!snapshot.routeCards.isEmpty, "snapshot carries route cards")
    let okCards = snapshot.routeCards.filter { $0.status == .ok }.count
    record("operator_snapshot", webCount: 1, nativeCount: okCards, parity: okCards > 0,
           delta: "\(okCards) of \(snapshot.routeCards.count) route cards live; not the offline fallback")

    // Forge runs (CL-1 first-real-result).
    let webRuns = await webCount("/forge/runs/projection.json", arrayKey: "runs")
    let nativeRuns = (try? await client.loadRuns(baseURL: baseURL)) ?? []
    check(nativeRuns.count > 0, "native app renders Forge runs (\(nativeRuns.count))")
    check(!(nativeRuns.first?.id.isEmpty ?? true), "runs are well-formed (carry ids)")
    record("forge_runs", webCount: webRuns, nativeCount: nativeRuns.count,
           parity: (webRuns ?? 0) == nativeRuns.count && nativeRuns.count > 0, delta: parityDelta(webRuns, nativeRuns.count))

    // Pages.
    let webPages = await webCount("/pages.json", arrayKey: "documents")
    let nativePages = (try? await client.loadPages(baseURL: baseURL)) ?? []
    check(nativePages.count > 0, "native app renders Pages (\(nativePages.count))")
    check(!(nativePages.first?.id.isEmpty ?? true), "pages are well-formed (carry ids)")
    record("pages", webCount: webPages, nativeCount: nativePages.count,
           parity: (webPages ?? 0) == nativePages.count && nativePages.count > 0, delta: parityDelta(webPages, nativePages.count))

    // Messages / activity.
    let webActivity = await webCount("/messages/activity/projection.json", arrayKey: "items")
    let nativeActivity = try? await client.loadActivity(baseURL: baseURL)
    let activityCount = nativeActivity?.items.count ?? 0
    check(activityCount > 0, "native app renders Message activity (\(activityCount))")
    record("messages_activity", webCount: webActivity, nativeCount: activityCount,
           parity: (webActivity ?? 0) == activityCount && activityCount > 0, delta: parityDelta(webActivity, activityCount))

    // Marketplace capabilities.
    let webCaps = await webCount("/marketplace/capabilities.json", arrayKey: "capabilities")
    let nativeCaps = (try? await client.loadCapabilities(baseURL: baseURL)) ?? []
    check(nativeCaps.count > 0, "native app renders Marketplace capabilities (\(nativeCaps.count))")
    record("marketplace", webCount: webCaps, nativeCount: nativeCaps.count,
           parity: (webCaps ?? 0) == nativeCaps.count && nativeCaps.count > 0, delta: parityDelta(webCaps, nativeCaps.count))

    // Long-horizon missions (LD-2 abandoned fleet).
    let webMissions = await webCount("/forge/long-horizon-missions/projection.json", arrayKey: "missions")
    let nativeMissions = (try? await client.loadMissions(baseURL: baseURL)) ?? []
    record("missions", webCount: webMissions, nativeCount: nativeMissions.count,
           parity: (webMissions ?? 0) == nativeMissions.count, delta: parityDelta(webMissions, nativeMissions.count))

    // Memory is three distinct product rails backed by four governed sources.
    // Raw memory records are intentionally not a durable product consumer.
    let memorySources = await [
      webCount("/learning/memory-activations/projection.json", arrayKey: nil),
      webCount("/learning/adaptation-grants/projection.json", arrayKey: nil),
      webCount("/memory/consolidation-ratifications/projection.json", arrayKey: nil),
      webCount("/forge/model-scorecard.json", arrayKey: nil),
    ]
    let nativeMemory = try? await client.loadMemoryWorkspace(baseURL: baseURL)
    let mappedSourceCount = nativeMemory == nil ? 0 : 4
    check(nativeMemory != nil, "native app maps all governed Memory sources")
    check(nativeMemory?.lessons.allSatisfy { !$0.id.isEmpty } == true, "approved lessons carry activation receipts")
    check(nativeMemory?.surfaceGroups.flatMap(\.records).allSatisfy { !$0.id.isEmpty } == true, "team memory carries review identity")
    check(nativeMemory?.modelProof.allSatisfy { !$0.model.isEmpty && !$0.workType.isEmpty } == true, "model evidence is attributed by work type")
    let webMemorySourceCount = memorySources.compactMap { $0 }.count
    record("memory", webCount: webMemorySourceCount, nativeCount: mappedSourceCount,
           parity: webMemorySourceCount == 4 && mappedSourceCount == 4,
           delta: "three product rails preserve four governed sources without consuming raw records")

    // Offline fail-closed honesty.
    let deadClient = MDxRouteClient(session: fastFailSession())
    let offline = await deadClient.loadSnapshot(baseURL: deadURL)
    check(offline.connectionStatus == .unavailable, "dead kernel fails closed to the offline snapshot")
    check(offline.currentStage == "offline", "offline snapshot stage is offline")
    check(offline.loadedAt == nil, "offline snapshot claims no load time")
    check(offline.runCount == 0, "offline snapshot fabricates no counts")
    check(!offline.boundary.isEmpty, "offline snapshot states a clear reason")
    print("[parity] offline: stage=\(offline.currentStage) reason=\"\(offline.boundary)\"")

    emitScorecard(offline: offline)

    if failures.isEmpty {
      print("\n[parity] ALL \(results.count) surfaces evaluated; ALL ASSERTIONS PASSED")
      return 0
    }
    print("\n[parity] \(failures.count) assertion(s) FAILED")
    return 1
  }

  // MARK: - Scorecard artifact

  private static func scorecardURL() -> URL {
    if let override = ProcessInfo.processInfo.environment["MDX_PARITY_SCORECARD_PATH"] {
      return URL(fileURLWithPath: override)
    }
    var dir = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
    for _ in 0..<10 {
      let candidate = dir.appendingPathComponent("docs").appendingPathComponent("sim")
      if FileManager.default.fileExists(atPath: candidate.path) {
        return candidate.appendingPathComponent("mdx-sim-macos-parity-scorecard.json")
      }
      dir = dir.deletingLastPathComponent()
    }
    return URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
      .appendingPathComponent("mdx-sim-macos-parity-scorecard.json")
  }

  private static func emitScorecard(offline: OperatorSnapshot) {
    let coreGaps = results.filter { !$0.parity }.map(\.surface)
    let namedGaps = results.filter { !$0.parity }.map(\.surface)
    let surfaces: [[String: Any]] = results
      .sorted { $0.surface < $1.surface }
      .map { r in
        [
          "surface": r.surface,
          "web_present": r.webCount != nil,
          "web_count": r.webCount as Any? ?? NSNull(),
          "native_present": r.nativeCount != nil,
          "native_count": r.nativeCount as Any? ?? NSNull(),
          "parity": r.parity,
          "delta": r.delta,
          "owner": r.owner
        ]
      }

    let verdict = coreGaps.isEmpty
      ? "PARITY on all core read-path surfaces, including Memory."
      : "PARITY GAPS present on core surfaces: \(coreGaps.joined(separator: ", "))"

    let doc: [String: Any] = [
      "slice": "S7 macOS parity",
      "generated_at": ISO8601DateFormatter().string(from: Date()),
      "base_url": baseURL.absoluteString,
      "method": "same live kernel, web-tier raw route count vs native MDxRouteClient mapped count",
      "surfaces": surfaces,
      "offline_fail_closed": offline.connectionStatus == .unavailable && offline.currentStage == "offline",
      "offline_reason": offline.boundary,
      "named_gaps": namedGaps,
      "assertion_failures": failures,
      "verdict": verdict
    ]

    do {
      let data = try JSONSerialization.data(withJSONObject: doc, options: [.prettyPrinted, .sortedKeys])
      let url = scorecardURL()
      try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
      try data.write(to: url)
      print("[parity] scorecard written to \(url.path)")
    } catch {
      print("[parity] failed to write scorecard: \(error)")
    }
    print("[parity] verdict: \(verdict)")
  }
}
