import XCTest
@testable import MDxWorkbench

/// S7 macOS parity slice.
///
/// Proves the native macOS operator app reaches parity with the web/API tier on
/// the core operator surfaces. The default run uses a deterministic in-process
/// HTTP fixture. Set MDX_PARITY_BASE_URL to exercise the same live kernel the
/// web tier reads.
///
/// Method: for each surface we (a) fetch the raw kernel route directly as the
/// "web tier" would and count its objects, then (b) drive the app's real
/// `MDxRouteClient` (the exact code the SwiftUI views consume) and count the
/// objects it maps. Parity = the native app receives non-empty, well-formed
/// data that matches what the web tier sees. Product Memory is proven from its
/// governed projections, never from raw storage records.
///
/// A live run emits the machine-readable scorecard next to the markdown
/// scorecard. Fixture runs write to a temporary artifact unless the path is
/// overridden with MDX_PARITY_SCORECARD_PATH.
final class ParitySliceTests: XCTestCase {
  @MainActor
  func testStartBuildUsesLiveCapacityInsteadOfHistoricalRunStates() {
    XCTAssertEqual(
      StartBuildSheet.availableDirectWorkers(capacityWorkers: 24, activeWorkers: 0),
      4
    )
    XCTAssertEqual(
      StartBuildSheet.availableDirectWorkers(capacityWorkers: 24, activeWorkers: 23),
      1
    )
    XCTAssertEqual(
      StartBuildSheet.availableDirectWorkers(capacityWorkers: 24, activeWorkers: 24),
      0
    )
  }

  @MainActor
  func testStartBuildNamesTheProvenExecutionLocationAndHoldsRemoteWrites() {
    XCTAssertEqual(
      StartBuildSheet.executionLocation(for: URL(string: "http://127.0.0.1:18890")!),
      .thisMac
    )
    XCTAssertTrue(
      StartBuildSheet.executionLocation(for: URL(string: "http://localhost:18890")!).canStart
    )
    XCTAssertEqual(
      StartBuildSheet.executionLocation(for: URL(string: "https://mdx.example.com")!),
      .cloudProfileRequired
    )
    XCTAssertFalse(
      StartBuildSheet.executionLocation(for: URL(string: "https://mdx.example.com")!).canStart
    )
  }

  func testRunListDoesNotPresentRecordedEvidenceAsLiveWork() {
    XCTAssertTrue(
      ForgeRun.isLiveOperatorRun(
        id: "forge_run_123",
        origin: "operator",
        systemOrigin: "",
        terminalState: "IN_PROGRESS"
      ))
    XCTAssertFalse(
      ForgeRun.isLiveOperatorRun(
        id: "forge_system_evidence_contract",
        origin: "system",
        systemOrigin: "forge_system",
        terminalState: "IN_PROGRESS"
      ))
    XCTAssertFalse(
      ForgeRun.isLiveOperatorRun(
        id: "forge_dev_seed_run_10",
        origin: "operator",
        systemOrigin: "",
        terminalState: "IN_PROGRESS"
      ))
  }

  private var usesFixtureKernel: Bool {
    ProcessInfo.processInfo.environment["MDX_PARITY_BASE_URL"] == nil
  }

  private var baseURL: URL {
    if let raw = ProcessInfo.processInfo.environment["MDX_PARITY_BASE_URL"],
       let url = URL(string: raw) {
      return url
    }
    return ParityFixtureURLProtocol.baseURL
  }

  private var paritySession: URLSession {
    usesFixtureKernel ? ParityFixtureURLProtocol.makeSession() : URLSession.shared
  }

  private let deadURL = URL(string: "http://127.0.0.1:59997")!

  func testBrowserReviewContractStaysBounded() {
    XCTAssertEqual(BrowserViewport.desktop.size, CGSize(width: 1440, height: 900))
    XCTAssertEqual(BrowserViewport.phone.size, CGSize(width: 390, height: 844))

    let approved = BrowserAuditResult(
      auditID: "audit_1",
      runID: "run_1",
      branch: "forge/run_1",
      previewPath: "apps/mdx-host",
      url: "http://127.0.0.1:45100/",
      screenshot: nil,
      screenshotSHA256: "abc",
      tracePath: ".mdx-local/browser-audits/audit_1/trace.zip",
      verdict: "APPROVE",
      critique: "The rendered result satisfies the review contract.",
      critiqueModel: "deterministic-browser-sensors",
      findings: [],
      consoleErrorCount: 0,
      networkFailureCount: 0,
      revisionStarted: false,
      revisionRunID: ""
    )
    XCTAssertTrue(approved.approved)
    XCTAssertFalse(approved.needsRevision)

    let finding = BrowserFinding(
      id: "horizontal_overflow",
      severity: "high",
      title: "Content spills outside the viewport",
      detail: "40px extends beyond the viewport.",
      selector: "main"
    )
    XCTAssertEqual(finding.id, "horizontal_overflow")
  }

  @MainActor func testMainWindowLaunchSizeAdaptsToTheDisplay() {
    XCTAssertEqual(
      MainWindowLaunchSizing.targetSize(for: CGSize(width: 2560, height: 1400)),
      CGSize(width: 1680, height: 1080)
    )
    XCTAssertEqual(
      MainWindowLaunchSizing.targetSize(for: CGSize(width: 1440, height: 875)),
      CGSize(width: 1180, height: 760)
    )
    XCTAssertEqual(
      MainWindowLaunchSizing.targetSize(for: CGSize(width: 1024, height: 700)),
      CGSize(width: 984, height: 660)
    )
  }

  func testStreamingForgeTokensBecomeOneReadableRow() {
    let make = { (id: String, seq: Int, token: String) in
      ForgeEvent(
        id: id,
        seq: seq,
        kind: "thinking_token",
        stage: "planning",
        summary: token,
        detail: "",
        receiptID: "",
        receiptRoute: "",
        model: "grok-4.5"
      )
    }
    let events = ForgeEvent.coalescingStreamingTokens(
      [make("first", 1, "Repo")],
      appending: [
        make("second", 2, "is"),
        make("third", 3, " ready"),
        make("fourth", 4, " ("),
        make("fifth", 5, "safe"),
        make("sixth", 6, ")")
      ]
    )

    XCTAssertEqual(events.count, 1)
    XCTAssertEqual(events[0].id, "first")
    XCTAssertEqual(events[0].summary, "Repo is ready (safe)")
  }

  func testRouteURLPreservesPaginationQuery() {
    let url = MDxRouteClient.routeURL(
      baseURL: URL(string: "http://127.0.0.1:18890")!,
      path: "/messages/thread-messages/projection.json?limit=250&channel_id=local-ops"
    )

    XCTAssertEqual(url.path, "/messages/thread-messages/projection.json")
    XCTAssertEqual(url.query, "limit=250&channel_id=local-ops")
  }

  func testRouteURLPreservesHostedKernelPrefix() {
    let url = MDxRouteClient.routeURL(
      baseURL: URL(string: "https://beta.mdx-os.com/api/kernel")!,
      path: "/messages/thread-messages/projection.json?limit=250"
    )

    XCTAssertEqual(url.absoluteString, "https://beta.mdx-os.com/api/kernel/messages/thread-messages/projection.json?limit=250")
  }

  @MainActor func testMarkdownParserPreservesBlockStructure() {
    let segments = MarkdownText.parse("""
    # Onboarding

    Start **here**.

    1. Read the guide
    2. Run the checks

    > Keep the proof.
    """)

    XCTAssertEqual(segments.count, 5)
    guard case .heading(let level, let title) = segments[0] else {
      return XCTFail("first block should be a heading")
    }
    XCTAssertEqual(level, 1)
    XCTAssertEqual(String(title.characters), "Onboarding")
    guard case .listItem(let firstMarker, _) = segments[2],
          case .listItem(let secondMarker, _) = segments[3],
          case .quote = segments[4]
    else {
      return XCTFail("ordered list and quote blocks should stay structured")
    }
    XCTAssertEqual(firstMarker, "1.")
    XCTAssertEqual(secondMarker, "2.")
  }

  @MainActor func testMarkdownParserPromotesTablesToStructuredContent() {
    let segments = MarkdownText.parse("""
    | Decision | Owner | State |
    | --- | --- | --- |
    | Ship | Mandeep | Ready |
    | Observe | Team | Next |
    """)

    XCTAssertEqual(segments.count, 1)
    guard case .table(let headers, let rows) = segments[0] else {
      return XCTFail("pipe tables should render as a table instead of raw text")
    }
    XCTAssertEqual(headers.map { String($0.characters) }, ["Decision", "Owner", "State"])
    XCTAssertEqual(rows.count, 2)
    XCTAssertEqual(rows[0].map { String($0.characters) }, ["Ship", "Mandeep", "Ready"])
  }

  @MainActor func testTwinMessageHandoffLeavesActivityForAWritableChannel() {
    let store = OperatorStore()
    store.messageLane = .activity("product")
    let message = TwinMessage(
      id: "answer_1",
      prompt: "What should we share?",
      answer: "The flagship flow is ready for review.",
      stance: "auto",
      memoryScore: 0,
      personaStatus: "",
      voiceStatus: "",
      summary: "",
      createdAt: "",
      answerReceiptID: "answer_receipt_1"
    )

    store.prepareTwinMessage(message)

    XCTAssertNotNil(store.messageLane.channelID)
    XCTAssertFalse(store.messageDraft.isEmpty)
    XCTAssertEqual(store.selectedAppRoute, .message)
  }

  @MainActor
  func testAuthenticationBoundaryDropsMacTenantProjectionBeforeAnotherUserCanEnter() async {
    let client = MDxRouteClient(session: ParityFixtureURLProtocol.makeSession())
    let snapshot = await client.loadSnapshot(baseURL: ParityFixtureURLProtocol.baseURL)
    let store = OperatorStore(
      client: client,
      initialSnapshot: snapshot,
      startBackgroundTasks: false
    )

    XCTAssertFalse(store.forgeRuns.isEmpty)
    store.suspendCloudAccess()

    XCTAssertEqual(store.snapshot.connectionStatus, .unavailable)
    XCTAssertEqual(store.snapshot.boundary, "Sign in to MDx to reconnect.")
    XCTAssertTrue(store.snapshot.forgeRuns.isEmpty)
    XCTAssertTrue(store.forgeRuns.isEmpty)
    XCTAssertNil(store.selectedRunID)
  }

  func testOlderTwinDatesUseAReadableCalendarLabel() {
    let label = RelativeTime.short("2025-01-14T12:00:00Z")
    XCTAssertTrue(label.contains("Jan 14"))
    XCTAssertFalse(label.hasSuffix("mo"))
  }

  func testTwinRecognizesHistoricalNoModelPlaceholder() {
    let message = TwinMessage(
      id: "answer_unavailable",
      prompt: "Can you help?",
      answer: "No model is connected yet, so this is a placeholder, not a real answer.",
      stance: "auto",
      memoryScore: 0,
      personaStatus: "",
      voiceStatus: "",
      summary: "",
      createdAt: "",
      answerReceiptID: "receipt_unavailable"
    )
    XCTAssertTrue(message.isModelUnavailablePlaceholder)
  }

  func testForgeMemoryCitationReadsLikeAnApprovedLesson() {
    let event = ForgeEvent(
      id: "event_memory_fixture",
      seq: 1,
      kind: "evidence_appended",
      stage: "planning",
      summary: "Recorded evidence",
      detail: "active learning memory cited advisory_count=2 selection_basis=relevance_v1",
      receiptID: "receipt_memory_fixture",
      receiptRoute: "/receipts/fixture.json",
      model: "fixture-model"
    )

    XCTAssertEqual(event.approvedLessonCitationCount, 2)
    XCTAssertEqual(event.displaySummary, "Drew on 2 lessons you approved")
  }

  func testMemoryRetirementBindsToTheActivationReceipt() async throws {
    let client = MDxRouteClient(session: ParityFixtureURLProtocol.makeSession())
    let lesson = MemoryLesson(
      id: "activation_receipt_fixture_1",
      activeMemoryID: "active_memory_fixture_1",
      summary: "Keep the first viewport focused.",
      targetType: "work_type",
      reviewOwner: "fixture_operator",
      activationBasis: "Approved after review.",
      rollbackPlan: "Retire when the contract changes.",
      promotionReceiptID: "promotion_receipt_fixture_1",
      judgmentReceiptID: "judgment_receipt_fixture_1",
      steersCasting: true
    )

    let outcome = try await client.retireMemoryLesson(
      baseURL: ParityFixtureURLProtocol.baseURL,
      lesson: lesson,
      reason: "The product contract changed materially.",
      reviewOwner: "fixture_operator",
      actorID: "fixture_operator"
    )

    XCTAssertTrue(outcome.succeeded)
    XCTAssertEqual(outcome.receiptID, "supersede_receipt_fixture_1")
    let body = MDxRouteClient.memorySupersedeBody(
      lesson: lesson,
      reason: "The product contract changed materially.",
      reviewOwner: "fixture_operator",
      actorID: "fixture_operator"
    )
    XCTAssertEqual(body["activation_receipt_id"] as? String, lesson.id)
    XCTAssertEqual(body["reason"] as? String, "The product contract changed materially.")
    XCTAssertEqual(body["review_owner"] as? String, "fixture_operator")
    XCTAssertNil(body["supersede_reason"])
  }

  // MARK: - Raw "web tier" probe

  /// Fetch a route the way the web tier does and count objects at `arrayKey`
  /// (falls back to the whole object being present). Returns nil when the route
  /// is absent (non-2xx), so an absent surface is honestly nil, not zero.
  private func webCount(_ path: String, arrayKey: String?) async -> Int? {
    guard let url = URL(string: baseURL.absoluteString + path) else { return nil }
    var request = URLRequest(url: url)
    request.timeoutInterval = 15
    request.setValue("application/json", forHTTPHeaderField: "Accept")
    guard let (data, response) = try? await paritySession.data(for: request),
          let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode),
          let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    else { return nil }
    if let arrayKey, let arr = obj[arrayKey] as? [Any] { return arr.count }
    return obj.isEmpty ? 0 : 1
  }

  // MARK: - Parity scorecard accumulation

  private struct SurfaceResult {
    let surface: String
    let webPresent: Bool
    let webCount: Int?
    let nativePresent: Bool
    let nativeCount: Int?
    let parity: Bool
    let delta: String
    let owner: String
  }

  private var results: [SurfaceResult] = []

  private func record(
    _ surface: String,
    webCount: Int?,
    nativeCount: Int?,
    parity: Bool,
    delta: String,
    owner: String = "none"
  ) {
    results.append(SurfaceResult(
      surface: surface,
      webPresent: (webCount ?? -1) >= 0 && webCount != nil,
      webCount: webCount,
      nativePresent: nativeCount != nil,
      nativeCount: nativeCount,
      parity: parity,
      delta: delta,
      owner: owner
    ))
    let w = webCount.map(String.init) ?? "absent"
    let n = nativeCount.map(String.init) ?? "absent"
    print("[parity] \(surface): web=\(w) native=\(n) parity=\(parity ? "MATCH" : "GAP") \(delta)")
  }

  // MARK: - The kernel parity slice

  func testMacOSParityAgainstKernelContract() async throws {
    let client = MDxRouteClient(session: paritySession)

    // --- CL-1 / MT-2: the core operator read-loop (Home / operator snapshot) ---
    let snapshot = await client.loadSnapshot(baseURL: baseURL)
    XCTAssertEqual(snapshot.connectionStatus, .ok,
                   "Live snapshot must connect, not fall back offline.")
    XCTAssertNotNil(snapshot.loadedAt, "Live snapshot must carry a load time.")
    XCTAssertNotEqual(snapshot.currentStage, "offline",
                      "Live snapshot must not be the offline fallback.")
    XCTAssertGreaterThan(snapshot.runCount, 0,
                         "The operator snapshot must see Forge runs.")
    XCTAssertFalse(snapshot.routeCards.isEmpty, "Snapshot must carry route cards.")
    let okCards = snapshot.routeCards.filter { $0.status == .ok }.count
    record("operator_snapshot", webCount: 1, nativeCount: okCards, parity: okCards > 0,
           delta: "\(okCards) of \(snapshot.routeCards.count) route cards live; snapshot is not the offline fallback")

    // --- CL-1: Forge runs (the first-real-result surface) ---
    let webRuns = await webCount("/forge/runs/projection.json", arrayKey: "runs")
    let nativeRuns = try await client.loadRuns(baseURL: baseURL)
    XCTAssertGreaterThan(nativeRuns.count, 0, "Native app must render Forge runs.")
    XCTAssertFalse(nativeRuns.first?.id.isEmpty ?? true, "Runs must be well-formed (have ids).")
    record("forge_runs", webCount: webRuns, nativeCount: nativeRuns.count,
           parity: (webRuns ?? 0) == nativeRuns.count && nativeRuns.count > 0,
           delta: parityDelta(webRuns, nativeRuns.count))

    // --- Pages ---
    let webPages = await webCount("/pages.json", arrayKey: "documents")
    let nativePages = try await client.loadPages(baseURL: baseURL)
    XCTAssertGreaterThan(nativePages.count, 0, "Native app must render Pages.")
    XCTAssertFalse(nativePages.first?.id.isEmpty ?? true, "Pages must be well-formed (have ids).")
    record("pages", webCount: webPages, nativeCount: nativePages.count,
           parity: (webPages ?? 0) == nativePages.count && nativePages.count > 0,
           delta: parityDelta(webPages, nativePages.count))

    // --- Messages / activity ---
    let webActivity = await webCount("/messages/activity/projection.json", arrayKey: "items")
    let nativeActivity = try await client.loadActivity(baseURL: baseURL)
    XCTAssertGreaterThan(nativeActivity.items.count, 0, "Native app must render Message activity.")
    record("messages_activity", webCount: webActivity, nativeCount: nativeActivity.items.count,
           parity: (webActivity ?? 0) == nativeActivity.items.count && nativeActivity.items.count > 0,
           delta: parityDelta(webActivity, nativeActivity.items.count))

    let webThreadMessages = await webCount("/messages/thread-messages/projection.json", arrayKey: "messages")
    let nativeThreadMessages = try await client.loadThreadMessages(baseURL: baseURL)
    XCTAssertGreaterThan(nativeThreadMessages.count, 0, "Native Message must render recorded channel messages.")
    record("messages_threads", webCount: webThreadMessages, nativeCount: nativeThreadMessages.count,
           parity: (webThreadMessages ?? 0) == nativeThreadMessages.count && nativeThreadMessages.count > 0,
           delta: parityDelta(webThreadMessages, nativeThreadMessages.count))

    // --- Marketplace capabilities ---
    let webCaps = await webCount("/marketplace/capabilities.json", arrayKey: "capabilities")
    let nativeCaps = try await client.loadCapabilities(baseURL: baseURL)
    XCTAssertGreaterThan(nativeCaps.count, 0, "Native app must render Marketplace capabilities.")
    record("marketplace", webCount: webCaps, nativeCount: nativeCaps.count,
           parity: (webCaps ?? 0) == nativeCaps.count && nativeCaps.count > 0,
           delta: parityDelta(webCaps, nativeCaps.count))

    // --- Marketplace flagship packs ---
    let webPacks = await webCount("/marketplace/packs.json", arrayKey: "packs")
    let nativePacks = try await client.loadMarketplacePacks(baseURL: baseURL)
    XCTAssertGreaterThan(nativePacks.count, 0, "Native app must render Marketplace packs.")
    XCTAssertTrue(nativePacks.allSatisfy { !$0.outcome.isEmpty && !$0.applicationTargets.isEmpty },
                  "Every native pack needs an outcome and an application target.")
    XCTAssertTrue(nativePacks.allSatisfy { !$0.requestedGrants.isEmpty && !$0.blockedGrants.isEmpty },
                  "Every native pack must state both requested and blocked grants.")
    record("marketplace_packs", webCount: webPacks, nativeCount: nativePacks.count,
           parity: (webPacks ?? 0) == nativePacks.count && nativePacks.count > 0,
           delta: parityDelta(webPacks, nativePacks.count))

    // --- LD-2: long-horizon missions (the abandoned-fleet surface) ---
    let webMissions = await webCount("/forge/long-horizon-missions/projection.json", arrayKey: "missions")
    let nativeMissions = try await client.loadMissions(baseURL: baseURL)
    record("missions", webCount: webMissions, nativeCount: nativeMissions.count,
           parity: (webMissions ?? 0) == nativeMissions.count,
           delta: parityDelta(webMissions, nativeMissions.count))

    // --- Memory: three distinct rails, four governed sources ---
    let memorySourceResults = await [
      webCount("/learning/memory-activations/projection.json", arrayKey: nil),
      webCount("/learning/adaptation-grants/projection.json", arrayKey: nil),
      webCount("/memory/consolidation-ratifications/projection.json", arrayKey: nil),
      webCount("/forge/model-scorecard.json", arrayKey: nil),
    ]
    let memoryWorkspace = try await client.loadMemoryWorkspace(baseURL: baseURL)
    XCTAssertEqual(memoryWorkspace.lessons.count, 1)
    XCTAssertTrue(memoryWorkspace.lessons[0].steersCasting)
    XCTAssertEqual(memoryWorkspace.surfaceGroups.flatMap(\.records).count, 2)
    XCTAssertEqual(memoryWorkspace.pendingReviewCount, 1)
    XCTAssertEqual(memoryWorkspace.modelProof.count, 1)
    XCTAssertEqual(memoryWorkspace.modelProof[0].doneRate, 0.875, accuracy: 0.0001)
    XCTAssertTrue(memoryWorkspace.lessons.allSatisfy { !$0.id.isEmpty }, "Approved lessons must carry activation receipts.")
    XCTAssertTrue(memoryWorkspace.surfaceGroups.flatMap(\.records).allSatisfy { !$0.id.isEmpty }, "Team memory must carry review identity.")
    let webMemorySources = memorySourceResults.compactMap { $0 }.count
    record("memory", webCount: webMemorySources, nativeCount: 4,
           parity: webMemorySources == 4,
           delta: "three native rails preserve four governed sources without consuming raw records")

    // Every asserted surface must have populated the native side.
    for r in results {
      XCTAssertNotNil(r.nativeCount, "\(r.surface) should populate on the native side.")
    }

    try emitScorecard(offlineReason: nil)
  }

  // MARK: - Offline fail-closed honesty

  func testOfflineParityHonesty() async throws {
    let client = MDxRouteClient(session: Self.fastFailSession())
    let snapshot = await client.loadSnapshot(baseURL: deadURL)

    XCTAssertEqual(snapshot.connectionStatus, .unavailable,
                   "A dead kernel must fail closed to the offline snapshot.")
    XCTAssertEqual(snapshot.currentStage, "offline")
    XCTAssertNil(snapshot.loadedAt, "Offline snapshot must not claim a load time.")
    XCTAssertEqual(snapshot.runCount, 0, "Offline snapshot must not fabricate counts.")
    XCTAssertFalse(snapshot.boundary.isEmpty, "Offline snapshot must state a clear reason.")
    XCTAssertEqual(snapshot.productPosture, "inspect_only")
    print("[parity] offline: stage=\(snapshot.currentStage) reason=\"\(snapshot.boundary)\"")

    offlineSnapshotReason = snapshot.boundary
  }

  // MARK: - Helpers

  private var offlineSnapshotReason: String = ""

  private func parityDelta(_ web: Int?, _ native: Int) -> String {
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

  private func scorecardURL() -> URL {
    if let override = ProcessInfo.processInfo.environment["MDX_PARITY_SCORECARD_PATH"] {
      return URL(fileURLWithPath: override)
    }
    if usesFixtureKernel {
      return FileManager.default.temporaryDirectory
        .appendingPathComponent("mdx-macos-parity-scorecard.json")
    }
    // .../apps/mdx-operator-macos/Tests/ParitySliceTests.swift -> repo root
    let repoRoot = URL(fileURLWithPath: #filePath)
      .deletingLastPathComponent() // Tests
      .deletingLastPathComponent() // mdx-operator-macos
      .deletingLastPathComponent() // apps
      .deletingLastPathComponent() // repo root
    return repoRoot
      .appendingPathComponent("docs")
      .appendingPathComponent("sim")
      .appendingPathComponent("mdx-sim-macos-parity-scorecard.json")
  }

  private func emitScorecard(offlineReason: String?) throws {
    let allParity = results.allSatisfy(\.parity)
    let gaps = results.filter { !$0.parity }.map(\.surface)

    var surfaces: [[String: Any]] = results.map { r in
      [
        "surface": r.surface,
        "web_present": r.webPresent,
        "web_count": r.webCount as Any? ?? NSNull(),
        "native_present": r.nativePresent,
        "native_count": r.nativeCount as Any? ?? NSNull(),
        "parity": r.parity,
        "delta": r.delta,
        "owner": r.owner
      ]
    }
    // Keep a stable ordering in the artifact.
    surfaces.sort { ($0["surface"] as? String ?? "") < ($1["surface"] as? String ?? "") }

    let doc: [String: Any] = [
      "slice": "S7 macOS parity",
      "generated_at": ISO8601DateFormatter().string(from: Date()),
      "base_url": baseURL.absoluteString,
      "method": "same live kernel, web-tier raw route count vs native MDxRouteClient mapped count",
      "surfaces": surfaces,
      "offline_fail_closed": true,
      "offline_reason": offlineSnapshotReason.isEmpty ? (offlineReason ?? "") : offlineSnapshotReason,
      "named_gaps": gaps,
      "verdict": gaps.isEmpty
        ? "PARITY on all core read-path surfaces, including governed Memory."
        : "PARITY GAPS present: \(gaps.joined(separator: ", "))"
    ]

    let data = try JSONSerialization.data(withJSONObject: doc, options: [.prettyPrinted, .sortedKeys])
    let url = scorecardURL()
    try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    try data.write(to: url)
    print("[parity] scorecard written to \(url.path)")
    print("[parity] core-surface verdict: \(doc["verdict"] as? String ?? "")")
    XCTAssertTrue(allParity, "Every surface must reach parity or be a named, owned gap.")
  }
}
