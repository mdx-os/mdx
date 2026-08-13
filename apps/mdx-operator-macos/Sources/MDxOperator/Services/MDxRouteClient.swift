import Foundation

struct MDxRouteClient {
  static let defaultBaseURL = URL(string: "http://127.0.0.1:18890")!
  static let maxInlineDocumentContextCharacters = 24_000
  static let maxPDFBytesForInlineParse = 12_000_000
  static let maxDiffPatchCharacters = 180_000
  static let maxDiffPatchLines = 2_500

  private let session: URLSession
  private let accessTokenProvider: @Sendable () async -> String?

  /// The snapshot load fans out ~26 GETs to localhost at once; the shared
  /// session's 6-per-host limit would serialize them into timeout risk.
  private static func makeLocalSession() -> URLSession {
    let config = URLSessionConfiguration.ephemeral
    config.httpMaximumConnectionsPerHost = 16
    config.timeoutIntervalForRequest = 60
    return URLSession(configuration: config)
  }

  init(
    session: URLSession? = nil,
    accessTokenProvider: @escaping @Sendable () async -> String? = { nil }
  ) {
    self.session = session ?? Self.makeLocalSession()
    self.accessTokenProvider = accessTokenProvider
  }

  static func isTrustedLocalBaseURL(_ url: URL) -> Bool {
    guard let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https" else { return false }
    guard let host = url.host?.lowercased() else { return false }
    return host == "localhost" || host == "127.0.0.1" || host == "::1"
  }

  /// A remote kernel derives identity exclusively from its verified bearer
  /// session. Local demo routes still accept their deterministic body fixture,
  /// but a hosted write must never echo tenant, actor, or role as authority.
  static func governedRequestBody(_ body: [String: Any], baseURL: URL) -> [String: Any] {
    guard !isTrustedLocalBaseURL(baseURL) else { return body }
    var sanitized = body
    for key in ["tenant_id", "actor_id", "actor_role"] {
      sanitized.removeValue(forKey: key)
    }
    return sanitized
  }

  static func boundedDocumentContext(_ text: String) -> String {
    guard text.count > maxInlineDocumentContextCharacters else { return text }
    return String(text.prefix(maxInlineDocumentContextCharacters)) + "\n\n[MDx clipped this document context before streaming. Open the attached artifact for the full text.]"
  }

  static func cappedDiffPatch(_ patch: String) -> String {
    guard patch.count > maxDiffPatchCharacters || patch.split(separator: "\n", omittingEmptySubsequences: false).count > maxDiffPatchLines else {
      return patch
    }
    var lines: [Substring] = []
    lines.reserveCapacity(maxDiffPatchLines + 1)
    var consumedCharacters = 0
    for line in patch.split(separator: "\n", omittingEmptySubsequences: false) {
      if lines.count >= maxDiffPatchLines || consumedCharacters + line.count > maxDiffPatchCharacters { break }
      lines.append(line)
      consumedCharacters += line.count + 1
    }
    return lines.joined(separator: "\n") + "\n\n[MDx clipped this diff preview. Use the review packet or source checkout for the full patch.]"
  }

  private static func requireTrustedWriteHost(_ baseURL: URL, path: String, authenticated: Bool = false) throws {
    let trustedRemote = baseURL.scheme?.lowercased() == "https" && authenticated
    guard isTrustedLocalBaseURL(baseURL) || trustedRemote else {
      throw RouteClientError.untrustedHost(path)
    }
  }

  private func authorize(_ request: URLRequest) async -> URLRequest {
    var request = request
    if let token = await accessTokenProvider()?.trimmingCharacters(in: .whitespacesAndNewlines), !token.isEmpty {
      request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    }
    return request
  }

  /// A single cheap liveness check against the overview route. Reachable means
  /// the kernel answered at all - even a non-2xx status or malformed body counts,
  /// because the engine is up. Only a transport failure (connection refused,
  /// timeout) means unreachable. Records one honest offline event when down,
  /// never the seventeen-route error burst.
  func isKernelReachable(baseURL: URL) async -> Bool {
    do {
      _ = try await fetchJSON(baseURL: baseURL, path: "/forge/runtime-projection.json", timeout: 4)
      return true
    } catch {
      return !Telemetry.isUnreachable(error)
    }
  }

  func loadSnapshot(baseURL: URL) async -> OperatorSnapshot {
    // Probe first: if the engine is off, return offline on ONE failed request
    // instead of fanning out all seventeen routes and recording seventeen
    // "errors" every refresh. This is what turned a ~130s offline launch into
    // ~1300 errors. A reachable kernel proceeds to the full load below.
    guard await isKernelReachable(baseURL: baseURL) else {
      return .offline(baseURL: baseURL)
    }
    let routeSpecs = [
      RouteSpec(title: "Runtime", path: "/forge/runtime-projection.json", lane: .overview, receiptBacked: false),
      RouteSpec(title: "Forge runs", path: "/forge/runs/projection.json", lane: .runRoom, receiptBacked: true),
      RouteSpec(title: "Repositories", path: "/forge/repos/projection.json", lane: .work, receiptBacked: true),
      RouteSpec(title: "Recipes", path: "/forge/recipes.json", lane: .work, receiptBacked: false),
      RouteSpec(title: "Builder loops", path: "/forge/builder-loops.json", lane: .work, receiptBacked: true),
      RouteSpec(title: "Missions", path: "/forge/long-horizon-missions/projection.json", lane: .work, receiptBacked: true),
      RouteSpec(title: "Fleet plans", path: "/forge/fleet-plans/projection.json", lane: .fleets, receiptBacked: true),
      RouteSpec(title: "Machine League recommendation", path: "/forge/machine-league/recommendations.json", lane: .machines, receiptBacked: true),
      RouteSpec(title: "Fleet runs", path: "/forge/fleet-runs/projection.json", lane: .fleets, receiptBacked: true),
      RouteSpec(title: "Arbitration", path: "/forge/machine-league/fleet-runs/arbitrations/projection.json", lane: .fleets, receiptBacked: true),
      RouteSpec(title: "Scoreboard", path: "/forge/fleet-eval-scoreboard.json", lane: .machines, receiptBacked: true),
      RouteSpec(title: "Machine sign-ins", path: "/forge/machine-league/closed-runner-clearances/projection.json", lane: .machines, receiptBacked: true),
      RouteSpec(title: "Fleet capacity", path: "/forge/fleet-capacity.json", lane: .fleets, receiptBacked: false),
      RouteSpec(title: "Control plane", path: "/forge/control-plane/projection.json", lane: .controls, receiptBacked: true),
      RouteSpec(title: "Model providers", path: "/forge/model-providers.json", lane: .machines, receiptBacked: true),
      RouteSpec(title: "Ship decisions", path: "/forge/ship-decisions/projection.json", lane: .review, receiptBacked: true),
      RouteSpec(title: "Evidence index", path: "/local/evidence-index.json", lane: .evidence, receiptBacked: true)
    ]
    // Fan out: the 17 route loads run concurrently instead of one after another,
    // so a cold snapshot is bounded by the slowest single route (each has its own
    // timeout), not by their sum. Results are re-ordered to the spec order so the
    // route cards and per-lane phases stay deterministic.
    let loadedRoutes: [LoadedRoute] = await withTaskGroup(of: (Int, LoadedRoute).self) { group in
      for (index, spec) in routeSpecs.enumerated() {
        group.addTask {
          let route = await self.loadRoute(
            baseURL: baseURL,
            title: spec.title,
            path: spec.path,
            lane: spec.lane,
            receiptBacked: spec.receiptBacked,
            timeout: spec.timeout
          )
          return (index, route)
        }
      }
      var collected: [(Int, LoadedRoute)] = []
      collected.reserveCapacity(routeSpecs.count)
      for await result in group { collected.append(result) }
      return collected.sorted { $0.0 < $1.0 }.map(\.1)
    }
    let cards = loadedRoutes.map(\.card)
    let successfulRoutes = loadedRoutes.filter { $0.card.status == .ok }

    guard !successfulRoutes.isEmpty else {
      return .offline(baseURL: baseURL)
    }

    let briefingJSON: JSONDictionary = [:]
    let workspaceJSON: JSONDictionary = [:]
    let runtimeJSON = loadedRoutes.json(for: "/forge/runtime-projection.json")
    let runsJSON = loadedRoutes.json(for: "/forge/runs/projection.json")
    let reposJSON = loadedRoutes.json(for: "/forge/repos/projection.json")
    let recipesJSON = loadedRoutes.json(for: "/forge/recipes.json")
    let builderLoopsJSON = loadedRoutes.json(for: "/forge/builder-loops.json")
    let missionsJSON = loadedRoutes.json(for: "/forge/long-horizon-missions/projection.json")
    let fleetPlansJSON = loadedRoutes.json(for: "/forge/fleet-plans/projection.json")
    let leagueJSON = loadedRoutes.json(for: "/forge/machine-league/recommendations.json")
    let fleetJSON = loadedRoutes.json(for: "/forge/fleet-runs/projection.json")
    let arbitrationJSON = loadedRoutes.json(for: "/forge/machine-league/fleet-runs/arbitrations/projection.json")
    let scoreboardJSON = loadedRoutes.json(for: "/forge/fleet-eval-scoreboard.json")
    let machinePreflightJSON = loadedRoutes.json(for: "/forge/machine-league/preflight.json")
    let machineClearancesJSON = loadedRoutes.json(for: "/forge/machine-league/closed-runner-clearances/projection.json")
    let capacityJSON = loadedRoutes.json(for: "/forge/fleet-capacity.json")
    let controlPlaneJSON = loadedRoutes.json(for: "/forge/control-plane/projection.json")
    let modelProvidersJSON = loadedRoutes.json(for: "/forge/model-providers.json")
    let shipDecisionsJSON = loadedRoutes.json(for: "/forge/ship-decisions/projection.json")
    let buildRequestsJSON = loadedRoutes.json(for: "/forge/build-requests/projection.json")
    let buildApprovalsJSON = loadedRoutes.json(for: "/forge/build-approvals/projection.json")
    let planProofsJSON = loadedRoutes.json(for: "/forge/workflow-plan-proofs/projection.json")
    let workerAuthorityJSON = loadedRoutes.json(for: "/forge/worker-authority-requests/projection.json")
    let talentAuthorizationsJSON = loadedRoutes.json(for: "/forge/talent-authorizations/projection.json")
    let ciPreflightJSON = loadedRoutes.json(for: "/forge/ci-evidence-preflights/projection.json")
    let humanPreflightJSON = loadedRoutes.json(for: "/forge/human-ratification-preflights/projection.json")
    let deploymentPreflightJSON = loadedRoutes.json(for: "/forge/deployment-preflights/projection.json")

    return OperatorSnapshot(
      loadedAt: Date(),
      baseURL: baseURL,
      connectionStatus: .ok,
      currentSignal: Self.firstString(
        in: [
          runtimeJSON.value(at: "forge_runtime.current_signal")
        ],
        fallback: "Local Forge routes are available"
      ),
      safeNextMove: Self.firstString(
        in: [
          runtimeJSON.value(at: "forge_runtime.safe_next_move")
        ],
        fallback: "Inspect evidence"
      ),
      boundary: Self.firstString(
        in: [
          runtimeJSON.value(at: "forge_runtime.boundary")
        ],
        fallback: "Inspect-only macOS shell. Writes, worker spawn, patch application, and deployment stay locked."
      ),
      workspaceIntent: runtimeJSON.string(at: "forge_runtime.operator_workspace_route", fallback: "Inspect Forge state from local MDx routes."),
      currentStage: Self.firstString(
        in: [
          runtimeJSON.value(at: "forge_runtime.queue_pressure_status")
        ],
        fallback: "unknown"
      ),
      authority: Self.firstString(
        in: [
          runtimeJSON.value(at: "forge_runtime.authority")
        ],
        fallback: "none"
      ),
      humanDecision: Self.firstString(
        in: [
          runtimeJSON.value(at: "forge_runtime.safe_next_move")
        ],
        fallback: "Review evidence before granting authority."
      ),
      productPosture: Self.firstString(
        in: [
          runtimeJSON.bool(at: "read_only", fallback: true) ? "inspect_only" : ""
        ],
        fallback: "inspect_only"
      ),
      runCount: runsJSON.int(at: "run_count", fallback: runsJSON.array(at: "runs").count),
      repoCount: reposJSON.int(at: "repo_count", fallback: reposJSON.array(at: "repos").count),
      recipeCount: recipesJSON.int(at: "recipe_count", fallback: recipesJSON.array(at: "recipes").count),
      missionCount: missionsJSON.int(at: "mission_count", fallback: missionsJSON.array(at: "missions").count),
      fleetPlanCount: fleetPlansJSON.int(at: "fleet_count", fallback: fleetPlansJSON.array(at: "fleets").count),
      fleetRunCount: Self.fleetRunCount(from: fleetJSON),
      arbitrationCount: arbitrationJSON.int(at: "arbitration_count", fallback: arbitrationJSON.array(at: "arbitrations").count),
      reviewDecisionCount: shipDecisionsJSON.int(at: "ship_decision_count", fallback: shipDecisionsJSON.array(at: "decisions").count),
      modelProviderCount: modelProvidersJSON.int(at: "provider_count", fallback: modelProvidersJSON.array(at: "providers").count),
      capacityQueueDepth: capacityJSON.int(at: "queue_depth", fallback: 0),
      capacityWorkers: capacityJSON.int(at: "worker_capacity", fallback: 0),
      capacityActiveWorkers: capacityJSON.int(at: "active_now", fallback: 0),
      receiptCount: briefingJSON.int(at: "operator_signal.receipt_count", fallback: 0),
      policyDecisionCount: briefingJSON.int(at: "operator_signal.policy_decision_count", fallback: 0),
      blockedActionCount: briefingJSON.int(at: "operator_signal.blocked_action_report_count", fallback: 0),
      safeActionCount: briefingJSON.array(at: "safe_actions").count,
      selectedRunner: leagueJSON.string(at: "selected_runner_display_name", fallback: "Forge native builder"),
      fallbackRunner: leagueJSON.string(at: "fallback_runner_display_name", fallback: "Codex CLI"),
      routeCards: cards,
      workItems: Self.workItems(from: runsJSON),
      runStages: Self.runStages(from: runsJSON),
      reviewArtifacts: Self.reviewArtifacts(
        shipDecisionsJSON: shipDecisionsJSON,
        ciPreflightJSON: ciPreflightJSON,
        humanPreflightJSON: humanPreflightJSON,
        deploymentPreflightJSON: deploymentPreflightJSON
      ),
      workbenchItems: Self.workbenchItems(
        reposJSON: reposJSON,
        recipesJSON: recipesJSON,
        builderLoopsJSON: builderLoopsJSON,
        missionsJSON: missionsJSON
      ),
      runRoomItems: Self.runRoomItems(from: runsJSON),
      forgeRuns: Self.forgeRuns(from: runsJSON),
      forgeDecisions: Self.forgeDecisions(from: controlPlaneJSON),
      parallelExecutionGroups: Self.parallelExecutionGroups(from: runsJSON),
      reviewItems: Self.reviewItems(
        shipDecisionsJSON: shipDecisionsJSON,
        ciPreflightJSON: ciPreflightJSON,
        humanPreflightJSON: humanPreflightJSON,
        deploymentPreflightJSON: deploymentPreflightJSON
      ),
      fleetItems: Self.fleetItems(
        fleetPlansJSON: fleetPlansJSON,
        fleetJSON: fleetJSON,
        arbitrationJSON: arbitrationJSON,
        capacityJSON: capacityJSON
      ),
      fleetPlans: Self.fleetPlans(from: fleetPlansJSON),
      fleetRuns: Self.fleetRuns(from: fleetJSON),
      startedFleetIDs: Self.startedFleetIDs(from: fleetJSON),
      controlItems: Self.controlItems(
        controlPlaneJSON: controlPlaneJSON,
        buildRequestsJSON: buildRequestsJSON,
        buildApprovalsJSON: buildApprovalsJSON,
        planProofsJSON: planProofsJSON,
        workerAuthorityJSON: workerAuthorityJSON,
        talentAuthorizationsJSON: talentAuthorizationsJSON
      ),
      runners: Self.runners(
        from: leagueJSON,
        scoreboardJSON: scoreboardJSON,
        preflightJSON: machinePreflightJSON,
        clearancesJSON: machineClearancesJSON
      ),
      modelRoles: Self.modelRoleRoutes(from: modelProvidersJSON),
      evidenceItems: Self.evidenceItems(from: briefingJSON, workspaceJSON: workspaceJSON, cards: cards),
      authorityItems: Self.authorityItems(from: briefingJSON, workspaceJSON: workspaceJSON)
    )
  }

  /// Machine preflight shells out to each supported local harness and can take
  /// close to a minute. Keep it out of the ordinary snapshot refresh and run it
  /// only when the operator asks to check this Mac.
  func loadMachineRunners(baseURL: URL) async throws -> [MachineRunner] {
    async let league = fetchJSON(
      baseURL: baseURL,
      path: "/forge/machine-league/recommendations.json"
    )
    async let scoreboard = fetchJSON(
      baseURL: baseURL,
      path: "/forge/fleet-eval-scoreboard.json"
    )
    async let preflight = fetchJSON(
      baseURL: baseURL,
      path: "/forge/machine-league/preflight.json",
      timeout: 75
    )
    async let clearances = fetchJSON(
      baseURL: baseURL,
      path: "/forge/machine-league/closed-runner-clearances/projection.json"
    )

    return Self.runners(
      from: try await league,
      scoreboardJSON: try await scoreboard,
      preflightJSON: try await preflight,
      clearancesJSON: try await clearances
    )
  }

  /// GET /version.json (round-3 kernel). Returns nil on older kernels.
  func loadKernelVersion(baseURL: URL) async -> KernelVersion? {
    guard let json = try? await fetchJSON(baseURL: baseURL, path: "/version.json") else { return nil }
    return KernelVersion(
      kernelVersion: json.string(at: "kernel_version", fallback: ""),
      contractFingerprint: json.string(at: "contract_fingerprint", fallback: ""),
      minAppVersion: json.string(at: "min_app_version", fallback: ""),
      routeCount: json.int(at: "route_count", fallback: 0)
    )
  }

  /// Kernel-side lifecycle for filed reports, filtered to the current actor.
  func loadFeedbackStatuses(baseURL: URL) async throws -> [RemoteReportStatus] {
    let json = try await fetchJSON(baseURL: baseURL, path: "/feedback/captures/projection.json")
    return json.array(at: "reports").map { report in
      RemoteReportStatus(
        receiptID: report.string(at: "feedback_receipt_id", fallback: ""),
        status: report.string(at: "status", fallback: ""),
        statusNote: report.string(at: "status_note", fallback: "")
      )
    }
    .filter { !$0.receiptID.isEmpty }
  }

  /// Connect a local checkout to Forge: {repo_id, label, root} -> CONNECTED
  /// with a repo receipt; the kernel profiles and indexes it. (Cloning from a
  /// remote URL is a pending kernel ask; local folders work today.)
  func connectRepo(baseURL: URL, repoID: String, label: String, root: String = "", originURL: String = "") async throws -> RunActionOutcome {
    var body: [String: Any] = ["repo_id": repoID, "label": label]
    // Local folder OR remote intake: with origin_url the kernel clones and
    // manages the checkout under .mdx-local/forge-repos (round-3 kernel).
    if !root.isEmpty { body["root"] = root }
    if !originURL.isEmpty { body["origin_url"] = originURL }
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/forge/repos.json",
      body: body
    )
    if json.string(at: "status", fallback: "") == "REFUSED" {
      throw RouteClientError.refused(json.string(at: "reason", fallback: "The kernel refused this repo."))
    }
    let receiptID = json.string(at: "repo_receipt_id", fallback: "")
    return RunActionOutcome(
      title: "Connect a repo",
      status: Self.requireReceiptStatus(json.string(at: "status", fallback: ""), receiptID: receiptID),
      detail: "",
      receiptID: receiptID
    )
  }

  /// Governed beta feedback capture. Context carries ONLY the allowed
  /// non-content keys (app_version, feature_area, route_status); private
  /// content never rides along, per the beta feedback contract.
  func postFeedback(
    baseURL: URL,
    surface: String,
    route: String,
    category: String,
    taxonomyClass: String,
    note: String,
    appVersion: String,
    routeStatus: String
  ) async throws -> RunActionOutcome {
    let body: [String: Any] = [
      "surface": surface,
      "route": route,
      "occurred_at": ISO8601DateFormatter().string(from: Date()),
      "category": category,
      "taxonomy_class": taxonomyClass,
      "note": note,
      "app_version": appVersion,
      "route_status": routeStatus,
      "feature_area": "native_macos",
    ]
    let json = try await postJSON(baseURL: baseURL, path: "/feedback/captures.json", body: body)
    let receiptID = json.string(at: "feedback_receipt_id", fallback: "")
    return RunActionOutcome(
      title: "Report an issue",
      status: Self.requireReceiptStatus(json.string(at: "status", fallback: ""), receiptID: receiptID),
      detail: json.string(at: "message", fallback: ""),
      receiptID: receiptID
    )
  }

  /// Safe Source-B beta telemetry. Identity is never sent in the body; the
  /// hosted kernel derives it from the same verified bearer session as every
  /// other governed write.
  func postBetaTelemetry(
    baseURL: URL,
    eventKind: String,
    route: String,
    step: String? = nil,
    completed: Bool? = nil,
    surface: String? = nil
  ) async throws {
    var body: [String: Any] = [
      "type": eventKind,
      "route": route,
      "ts": ISO8601DateFormatter().string(from: Date()),
      "app_version": Bundle.main.appVersionLabel,
    ]
    if let step { body["step"] = step }
    if let completed { body["completed"] = completed }
    if let surface { body["surface"] = surface }
    _ = try await postJSON(baseURL: baseURL, path: "/beta/telemetry-events.json", body: body)
  }

  func loadLatestMacRelease(hostedOrigin: URL) async throws -> MacReleaseManifest? {
    let json = try await fetchJSON(
      baseURL: hostedOrigin,
      path: "/download/macos/manifest.json",
      timeout: 10
    )
    guard json.bool(at: "available", fallback: false) else { return nil }
    let manifest = json.dictionary(at: "manifest")
    let version = manifest.string(at: "version", fallback: "")
    let build = manifest.string(at: "build", fallback: "")
    let sha256 = manifest.string(at: "sha256", fallback: "")
    let sizeBytes = manifest.int(at: "size_bytes", fallback: 0)
    guard !version.isEmpty, !build.isEmpty, sha256.count == 64, sizeBytes > 0 else { return nil }
    return MacReleaseManifest(
      version: version,
      build: build,
      sha256: sha256,
      sizeBytes: sizeBytes,
      notarizedAt: manifest.string(at: "notarized_at", fallback: "")
    )
  }

  /// Governed, METRICS-ONLY app-health ingest. Sends EXACTLY the server's
  /// allowlisted rollup (counts, durations, a bounded health state, and bare
  /// route-path strings), governed like `postFeedback`, so the kernel never
  /// refuses it. Never carries a note, body, or any content. Best-effort: the
  /// caller swallows failures so telemetry never disturbs the app.
  func postAppHealth(baseURL: URL, rollup: AppHealthRollupPayload) async throws {
    let failingRoutes: [[String: Any]] = rollup.failingRoutes.map { entry in
      ["route": entry.route, "status": entry.status, "count": entry.count]
    }
    let body: [String: Any] = [
      "app_version": rollup.appVersion,
      "surface": rollup.surface,
      "session_id": rollup.sessionID,
      "reported_at": rollup.reportedAt,
      "health_state": rollup.healthState,
      "total_calls": rollup.totalCalls,
      "error_count": rollup.errorCount,
      "hang_count": rollup.hangCount,
      "consecutive_failures": rollup.consecutiveFailures,
      "avg_latency_ms": rollup.avgLatencyMs,
      "p95_latency_ms": rollup.p95LatencyMs,
      "launch_ms": rollup.launchMs,
      "unclean_relaunch": rollup.uncleanRelaunch,
      "failing_routes": failingRoutes,
    ]
    // The ingest answers 200 for both admitted and refused rollups; a thrown
    // error here means a transport failure, which the caller treats as best-effort.
    _ = try await postJSON(baseURL: baseURL, path: "/telemetry/app-health.json", body: body)
  }

  /// The cross-session app-health aggregate: top failing routes across sessions,
  /// the unclean-relaunch rate, hang hotspots, and degraded-session count. This
  /// is the founder's "continuously see where users have issues" read.
  func loadAppHealthProjection(baseURL: URL) async throws -> AppHealthProjection {
    let json = try await fetchJSON(baseURL: baseURL, path: "/telemetry/app-health/projection.json")
    let topFailingRoutes = json.array(at: "top_failing_routes").map { row in
      AppHealthRouteHotspot(
        route: row.string(at: "route", fallback: ""),
        totalFailureCount: row.int(at: "total_failure_count", fallback: 0),
        distinctSessionCount: row.int(at: "distinct_session_count", fallback: 0),
        lastStatus: row.int(at: "last_status", fallback: 0)
      )
    }
    .filter { !$0.route.isEmpty }
    let recentReports = json.array(at: "recent_reports").map { row in
      AppHealthRecentReport(
        receiptID: row.string(at: "health_receipt_id", fallback: ""),
        surface: row.string(at: "surface", fallback: ""),
        healthState: row.string(at: "health_state", fallback: ""),
        errorCount: row.int(at: "error_count", fallback: 0),
        hangCount: row.int(at: "hang_count", fallback: 0),
        uncleanRelaunch: row.bool(at: "unclean_relaunch", fallback: false),
        failingRouteCount: row.int(at: "failing_route_count", fallback: 0),
        reportedAt: row.string(at: "reported_at", fallback: "")
      )
    }
    return AppHealthProjection(
      totalReports: json.int(at: "total_reports", fallback: 0),
      distinctSessions: json.int(at: "distinct_sessions", fallback: 0),
      degradedSessionCount: json.int(at: "degraded_session_count", fallback: 0),
      uncleanRelaunchSessionCount: json.int(at: "unclean_relaunch_session_count", fallback: 0),
      uncleanRelaunchRatePct: json.int(at: "unclean_relaunch_rate_pct", fallback: 0),
      hangHotspotSessionCount: json.int(at: "hang_hotspot_session_count", fallback: 0),
      topFailingRoutes: topFailingRoutes,
      recentReports: recentReports
    )
  }

  /// The kernel composes the PR handoff FROM the run's receipts: title,
  /// receipt-backed body markdown, review checklist, branch, and guardrail
  /// truth (remote push / PR-open stay false until authority is granted).
  func preparePRHandoff(baseURL: URL, runID: String) async throws -> PRHandoff {
    let json = try await postJSON(baseURL: baseURL, path: "/forge/pr-handoffs.json", body: ["run_id": runID])
    if (json.string(at: "status", fallback: "") == "REFUSED") {
      throw RouteClientError.refused(json.string(at: "reason", fallback: "The kernel refused the handoff."))
    }
    var handoff = PRHandoff(
      runID: runID,
      title: json.string(at: "title", fallback: "Forge run \(runID)"),
      bodyMarkdown: json.string(at: "body_markdown", fallback: ""),
      branch: json.string(at: "branch", fallback: ""),
      receiptID: json.string(at: "pr_handoff_receipt_id", fallback: ""),
      reviewStatus: json.string(at: "review_status", fallback: ""),
      checklist: (json["review_checklist"] as? [String]) ?? [],
      remotePushAllowed: json.bool(at: "remote_push_allowed", fallback: false),
      pullRequestOpenAllowed: json.bool(at: "pull_request_open_allowed", fallback: false),
      blockedReasons: (json["delivery_blocked_reasons"] as? [String]) ?? []
    )
    let authority = json.dictionary(at: "pr_open_authority")
    handoff.authorityStatus = authority.string(at: "status", fallback: "")
    handoff.liveDeliveryRoute = authority.string(at: "live_delivery_route", fallback: "")
    handoff.readinessRoute = authority.string(at: "readiness_route", fallback: "")
    handoff.requiresOperatorAction = authority.bool(at: "requires_explicit_operator_action", fallback: true)
    return handoff
  }

  func submitAction(
    baseURL: URL,
    action: GovernedActionKind,
    draft: GovernedActionDraft
  ) async throws -> GovernedActionResult {
    let json = try await postJSON(baseURL: baseURL, path: action.path, body: draft.body(for: action))
    let status = json.string(at: "status", fallback: "")
    let receiptID = Self.firstString(
      in: [
        json.value(at: "build_request_receipt_id"),
        json.value(at: "build_approval_receipt_id"),
        json.value(at: "workflow_plan_proof_receipt_id"),
        json.value(at: "worker_authority_receipt_id"),
        json.value(at: "talent_authorization_receipt_id"),
        json.value(at: "ratification_preflight_receipt_id"),
        json.value(at: "decision_receipt_id"),
        json.value(at: "pr_handoff_receipt_id")
      ],
      fallback: ""
    )
    let detail = Self.firstString(
      in: [
        json.value(at: "terminal_state"),
        json.value(at: "next_safe_action"),
        json.value(at: "safe_next_move"),
        json.value(at: "reason"),
        json.value(at: "delivery_guardrail_status")
      ],
      fallback: action.plainLanguageBoundary
    )
    return GovernedActionResult(
      action: action,
      status: Self.requireReceiptStatus(status, receiptID: receiptID),
      title: action.title,
      detail: detail,
      route: action.path,
      receiptID: receiptID,
      rawSummary: Self.summary(from: json)
    )
  }

  // MARK: - Forge run lifecycle

  func fetchDiff(baseURL: URL, runID: String, actorID: String) async throws -> [DiffFile] {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/forge/run-diff.json",
      body: ["run_id": runID, "actor_id": actorID]
    )
    if let reason = json["reason"] as? String, (json["status"] as? String) == "REFUSED" {
      throw RouteClientError.refused(reason)
    }
    return json.array(at: "files").map { file in
      var diffFile = DiffFile(
        path: file.string(at: "path", fallback: "file"),
        added: file.int(at: "added", fallback: 0),
        removed: file.int(at: "removed", fallback: 0),
        patch: Self.cappedDiffPatch(file.string(at: "patch", fallback: ""))
      )
      diffFile.trailStepCount = file.int(at: "trail_step_count", fallback: 0)
      diffFile.errorStepCount = file.int(at: "error_step_count", fallback: 0)
      diffFile.retryCount = file.int(at: "retry_count", fallback: 0)
      diffFile.checksTouching = (file["checks_touching"] as? [String]) ?? []
      diffFile.agentConfidence = file.string(at: "agent_confidence", fallback: "")
      return diffFile
    }
  }

  func loadRuns(baseURL: URL) async throws -> [ForgeRun] {
    try await loadRunProjection(baseURL: baseURL).runs
  }

  func loadRunProjection(baseURL: URL) async throws -> ForgeRunProjection {
    let json = try await fetchJSON(baseURL: baseURL, path: "/forge/runs/projection.json")
    return ForgeRunProjection(
      runs: Self.forgeRuns(from: json),
      parallelExecutionGroups: Self.parallelExecutionGroups(from: json)
    )
  }

  /// The two fleet projections only, for the fleet-activity watch. Loads them
  /// concurrently instead of re-running the full 17-route snapshot every 4s
  /// while a fleet is planning or running.
  func loadFleetProjections(baseURL: URL) async -> ForgeFleetProjections {
    async let plansJSON = try? fetchJSON(baseURL: baseURL, path: "/forge/fleet-plans/projection.json")
    async let runsJSON = try? fetchJSON(baseURL: baseURL, path: "/forge/fleet-runs/projection.json")
    return ForgeFleetProjections(
      plans: Self.fleetPlans(from: (await plansJSON) ?? [:]),
      runs: Self.fleetRuns(from: (await runsJSON) ?? [:])
    )
  }

  func loadRepos(baseURL: URL) async throws -> [ForgeRepo] {
    let json = try await fetchJSON(baseURL: baseURL, path: "/forge/repos/projection.json")
    return Self.forgeRepos(from: json)
  }

  /// Reads the kernel's lean setup-router projection so the welcome flow can
  /// confirm the real install state (owner, connected model) without composing
  /// a full readiness report. Returns `.unreachable` if the kernel is down,
  /// which the flow surfaces honestly rather than faking readiness.
  func loadSetupRouter(baseURL: URL) async -> SetupRouterState {
    do {
      let json = try await fetchJSON(baseURL: baseURL, path: "/install/setup-router.json")
      return SetupRouterState(
        reachable: true,
        setupComplete: json.bool(at: "setup_complete", fallback: false),
        ownerClaimed: json.bool(at: "owner_claimed", fallback: false),
        ownerName: json.string(at: "owner_name", fallback: ""),
        modelConnected: json.bool(at: "model_connected", fallback: false),
        modelID: json.string(at: "model_id", fallback: ""),
        trackChosen: json.bool(at: "track_chosen", fallback: false),
        chosenTrack: json.string(at: "chosen_track", fallback: "")
      )
    } catch {
      return .unreachable
    }
  }

  func claimInstallOwner(baseURL: URL, name: String, actorID: String) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/install/owner-claims.json",
      body: ["owner_name": name, "actor_id": actorID]
    )
    let receipt = json.string(at: "receipt_id", fallback: "")
    return RunActionOutcome(
      title: "Claim this install",
      status: json.bool(at: "claimed", fallback: false) ? "RECORDED" : "REFUSED",
      detail: json.string(at: "reason", fallback: json.string(at: "owner_name", fallback: name)),
      receiptID: receipt
    )
  }

  func connectTwinModel(baseURL: URL, providerID: String, apiKey: String, actorID: String) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/models/connect.json",
      body: [
        "provider_id": providerID,
        "api_key": apiKey,
        "persist_key": true,
        "actor_id": actorID
      ],
      timeout: 60
    )
    let receipt = json.string(at: "connection_receipt_id", fallback: "")
    return RunActionOutcome(
      title: "Connect a model",
      status: json.string(at: "status", fallback: "") == "MODEL_CONNECTED" ? "CONNECTED" : "REFUSED",
      detail: json.string(at: "reason", fallback: json.string(at: "model_id", fallback: "")),
      receiptID: receipt
    )
  }

  func loadMissions(baseURL: URL) async throws -> [ForgeMission] {
    let json = try await fetchJSON(baseURL: baseURL, path: "/forge/long-horizon-missions/projection.json")
    return Self.forgeMissions(from: json)
  }

  func loadMemoryWorkspace(baseURL: URL) async throws -> MemoryWorkspace {
    async let activationsTask: JSONDictionary? = try? fetchJSON(
      baseURL: baseURL,
      path: "/learning/memory-activations/projection.json"
    )
    async let grantsTask: JSONDictionary? = try? fetchJSON(
      baseURL: baseURL,
      path: "/learning/adaptation-grants/projection.json"
    )
    async let recordsTask: JSONDictionary? = try? fetchJSON(
      baseURL: baseURL,
      path: "/memory/consolidation-ratifications/projection.json"
    )
    async let scorecardTask: JSONDictionary? = try? fetchJSON(
      baseURL: baseURL,
      path: "/forge/model-scorecard.json"
    )

    let packets = await (
      activationsTask,
      grantsTask,
      recordsTask,
      scorecardTask
    )
    guard packets.0 != nil || packets.1 != nil || packets.2 != nil || packets.3 != nil else {
      throw URLError(.cannotConnectToHost)
    }
    let activations = packets.0 ?? [:]
    let grants = packets.1 ?? [:]
    let records = packets.2 ?? [:]
    let scorecard = packets.3 ?? [:]
    let activationRows = activations.array(at: "active_memories").isEmpty
      ? activations.array(at: "items")
      : activations.array(at: "active_memories")
    let grantRows = grants.array(at: "grants").isEmpty
      ? grants.array(at: "items")
      : grants.array(at: "grants")
    let pendingRows = records.array(at: "pending").isEmpty
      ? records.array(at: "pending_items")
      : records.array(at: "pending")
    let openGrantActivationIDs = Set(
      grantRows
        .filter { $0.bool(at: "open", fallback: false) }
        .map { $0.string(at: "activation_receipt_id", fallback: "") }
        .filter { !$0.isEmpty }
    )
    let retiredCount = activationRows.filter {
      $0.string(at: "active_memory_state", fallback: "ACTIVE").uppercased() != "ACTIVE"
    }.count
    let lessons = activationRows
      .filter { $0.string(at: "active_memory_state", fallback: "ACTIVE").uppercased() == "ACTIVE" }
      .compactMap { item -> MemoryLesson? in
        let receiptID = item.string(at: "active_memory_receipt_id", fallback: "")
        guard !receiptID.isEmpty else { return nil }
        return MemoryLesson(
          id: receiptID,
          activeMemoryID: item.string(at: "active_memory_id", fallback: ""),
          summary: Self.memoryLessonSummary(item.string(at: "lesson_summary", fallback: "Approved lesson")),
          targetType: item.string(at: "target_type", fallback: "work"),
          reviewOwner: item.string(at: "review_owner", fallback: "Owner"),
          activationBasis: item.string(at: "activation_basis", fallback: ""),
          rollbackPlan: item.string(at: "rollback_plan", fallback: ""),
          promotionReceiptID: item.string(at: "memory_promotion_receipt_id", fallback: ""),
          judgmentReceiptID: item.string(at: "judgment_decision_receipt_id", fallback: ""),
          steersCasting: openGrantActivationIDs.contains(receiptID)
        )
      }

    var groupedRecords: [String: [MemorySurfaceRecord]] = [:]
    for item in pendingRows {
      let memoryID = item.string(at: "memory_id", fallback: "")
      guard !memoryID.isEmpty else { continue }
      let surface = Self.memorySurfaceName(item.string(at: "memory_scope", fallback: "shared"))
      groupedRecords[surface, default: []].append(
        MemorySurfaceRecord(
          id: "pending::\(memoryID)",
          kind: .pending,
          content: item.string(at: "content", fallback: "Memory proposed for review"),
          proposedBy: item.string(at: "proposed_by", fallback: "MDx"),
          reviewedBy: "",
          note: "",
          sourceReceiptID: item.string(at: "source_receipt_id", fallback: ""),
          gateReceiptID: item.string(at: "gate_receipt_id", fallback: ""),
          decisionReceiptID: ""
        )
      )
    }
    for item in records.array(at: "ratifications") where item.string(at: "ratification_decision", fallback: "").uppercased() == "APPROVE" {
      let memoryID = item.string(at: "memory_id", fallback: "")
      guard !memoryID.isEmpty else { continue }
      groupedRecords["Kept for the team", default: []].append(
        MemorySurfaceRecord(
          id: "kept::\(memoryID)",
          kind: .kept,
          content: item.string(at: "note", fallback: "Approved team memory"),
          proposedBy: item.string(at: "proposed_by", fallback: "MDx"),
          reviewedBy: item.string(at: "ratified_by", fallback: "Owner"),
          note: item.string(at: "note", fallback: ""),
          sourceReceiptID: "",
          gateReceiptID: "",
          decisionReceiptID: item.string(at: "ratification_receipt_id", fallback: "")
        )
      )
    }
    let surfaceGroups = groupedRecords.keys.sorted().map {
      MemorySurfaceGroup(surface: $0, records: groupedRecords[$0] ?? [])
    }
    let modelProof = scorecard.array(at: "by_work_type").compactMap { item -> MemoryModelProof? in
      let model = item.string(at: "model", fallback: "")
      let workType = item.string(at: "work_type", fallback: "")
      guard !model.isEmpty, !workType.isEmpty else { return nil }
      return MemoryModelProof(
        model: model,
        workType: workType,
        runs: item.int(at: "runs", fallback: 0),
        done: item.int(at: "done", fallback: 0),
        doneRate: item.double(at: "done_rate", fallback: 0),
        averageTurns: item.double(at: "avg_turns", fallback: 0)
      )
    }
    .sorted { lhs, rhs in
      if lhs.doneRate != rhs.doneRate { return lhs.doneRate > rhs.doneRate }
      if lhs.runs != rhs.runs { return lhs.runs > rhs.runs }
      return lhs.model < rhs.model
    }

    return MemoryWorkspace(
      lessons: lessons,
      retiredLessonCount: retiredCount,
      surfaceGroups: surfaceGroups,
      modelProof: Array(modelProof.prefix(12)),
      unavailableRails: Set([
        packets.0 == nil ? MemoryRail.learned : nil,
        packets.2 == nil ? MemoryRail.recorded : nil,
        packets.3 == nil ? MemoryRail.modelProof : nil,
      ].compactMap { $0 })
    )
  }

  func retireMemoryLesson(
    baseURL: URL,
    lesson: MemoryLesson,
    reason: String,
    reviewOwner: String,
    actorID: String
  ) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/learning/memory-supersedes.json",
      body: Self.memorySupersedeBody(
        lesson: lesson,
        reason: reason,
        reviewOwner: reviewOwner,
        actorID: actorID
      )
    )
    let receipt = json.string(at: "memory_supersede_receipt_id", fallback: "")
    return RunActionOutcome(
      title: "That’s outdated",
      status: json.bool(at: "memory_superseded", fallback: false) ? "RECORDED" : "REFUSED",
      detail: json.string(at: "reason", fallback: reason),
      receiptID: receipt
    )
  }

  static func memorySupersedeBody(
    lesson: MemoryLesson,
    reason: String,
    reviewOwner: String,
    actorID: String
  ) -> [String: Any] {
    [
      "activation_receipt_id": lesson.id,
      "active_memory_id": lesson.activeMemoryID,
      "reason": reason,
      "review_owner": reviewOwner,
      "actor_id": actorID,
    ]
  }

  private static func memorySurfaceName(_ scope: String) -> String {
    switch scope.lowercased() {
    case "team_memory": "Message"
    case "company_memory": "Pages"
    case "project_memory": "Forge"
    case "agent_operational_memory": "Operations"
    case "private_user": "Twin"
    default: "Shared"
    }
  }

  private static func memoryLessonSummary(_ raw: String) -> String {
    let prefix = "candidate lesson:"
    let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
    guard trimmed.lowercased().hasPrefix(prefix) else { return trimmed }
    return String(trimmed.dropFirst(prefix.count)).trimmingCharacters(in: .whitespacesAndNewlines)
  }

  func loadTwin(baseURL: URL, sessionID: String) async throws -> [TwinMessage] {
    let json = try await fetchJSON(baseURL: baseURL, path: "/twin/session-drafts/projection.json")
    return json.array(at: "drafts")
      .filter { $0.string(at: "session_id", fallback: "") == sessionID }
      .map { Self.twinMessage(from: $0) }
      .filter { !$0.prompt.isEmpty }
      .sorted { $0.createdAt < $1.createdAt }
  }

  /// Current company context for every native Twin send. Each source fails
  /// independently, matching the web digest's fail-soft boundary.
  func loadTwinCompanyContext(baseURL: URL) async -> (text: String, sources: [String]) {
    async let needs = try? fetchJSON(baseURL: baseURL, path: "/product/needs-you/projection.json", timeout: 3)
    async let forge = try? fetchJSON(baseURL: baseURL, path: "/forge/runs/projection.json", timeout: 3)
    async let messages = try? fetchJSON(baseURL: baseURL, path: "/messages/thread-messages/projection.json", timeout: 3)
    async let pages = try? fetchJSON(baseURL: baseURL, path: "/pages/lifecycle/projection.json", timeout: 3)
    async let strategy = try? fetchJSON(baseURL: baseURL, path: "/strategy/direction-proposals/projection.json", timeout: 3)
    async let twinRuntime = try? fetchJSON(baseURL: baseURL, path: "/twin/agent-runtime.json", timeout: 3)
    async let twinReadiness = try? fetchJSON(baseURL: baseURL, path: "/twin/intelligence-readiness.json", timeout: 3)
    async let twinArtifacts = try? fetchJSON(baseURL: baseURL, path: "/twin/artifact-runtime.json", timeout: 3)
    let packets = await (needs, forge, messages, pages, strategy, twinRuntime, twinReadiness, twinArtifacts)
    var lines: [String] = []
    if let runtime = packets.5, let readiness = packets.6 {
      let conversation = readiness.bool(at: "conversation_ready", fallback: false) ? "conversation is ready" : "conversation is still getting ready"
      let model = readiness.bool(at: "provider_connected_local_allowed", fallback: false) ? "a live model is connected" : "no live model is connected"
      let artifacts = packets.7?.int(at: "artifact_count", fallback: 0) ?? 0
      let tools = runtime.dictionary(at: "blocked_authority").bool(at: "tool_execution_allowed", fallback: false) ? "tools are open" : "tools still require governed approval"
      lines.append("Twin: \(conversation), \(model), \(artifacts) artifacts are on record, and \(tools).")
    }
    if let packet = packets.0, packet.int(at: "total_count", fallback: 0) > 0 {
      lines.append("Needs you: \(packet.int(at: "total_count", fallback: 0)) things are waiting on your call.")
    }
    if let packet = packets.1, packet.int(at: "run_count", fallback: 0) > 0 {
      lines.append("Forge: \(packet.int(at: "run_count", fallback: 0)) builds are on record.")
    }
    if let packet = packets.2, packet.int(at: "message_count", fallback: 0) > 0 {
      lines.append("Message: \(packet.int(at: "message_count", fallback: 0)) messages are in the current projection.")
    }
    if let packet = packets.3, packet.int(at: "document_count", fallback: 0) > 0 {
      lines.append("Pages: \(packet.int(at: "document_count", fallback: 0)) pages are in the library.")
    }
    if let packet = packets.4, packet.int(at: "proposal_count", fallback: 0) > 0 {
      lines.append("Strategy: \(packet.int(at: "proposal_count", fallback: 0)) direction proposals are open.")
    }
    return (lines.isEmpty ? "" : "Across MDx right now:\n- " + lines.joined(separator: "\n- "), lines)
  }

  func loadTwinConversations(baseURL: URL) async throws -> [TwinConversation] {
    let json = try await fetchJSON(baseURL: baseURL, path: "/twin/session-drafts/projection.json")
    let drafts = json.array(at: "drafts").filter { !$0.string(at: "draft_text", fallback: "").isEmpty }
    let groups = Dictionary(grouping: drafts) { $0.string(at: "session_id", fallback: "") }
    return groups.compactMap { sessionID, items -> TwinConversation? in
      guard !sessionID.isEmpty else { return nil }
      let sorted = items.sorted { $0.string(at: "created_at", fallback: "") < $1.string(at: "created_at", fallback: "") }
      guard let first = sorted.first, let last = sorted.last else { return nil }
      return TwinConversation(
        id: sessionID,
        title: first.string(at: "draft_text", fallback: "Conversation"),
        snippet: last.string(at: "grounded_answer", fallback: last.string(at: "draft_text", fallback: "")),
        lastTime: last.string(at: "created_at", fallback: ""),
        count: sorted.count
      )
    }
    .sorted { $0.lastTime > $1.lastTime }
  }

  func sendTwin(
    baseURL: URL,
    sessionID: String,
    stance: String,
    text: String,
    sensitive: Bool = false,
    documentContext: String = "",
    documentName: String = "",
    worldContext: String = ""
  ) async throws -> TwinMessage {
    var body: [String: Any] = [
      "session_id": sessionID,
      "companion_id": "twin",
      "companion_stance": stance,
      "draft_text": text,
      "prompt_shape": "A person is thinking through their work with you. Be direct and useful.",
      "sensitivity_tier": sensitive ? "sensitive" : "internal"
    ]
    if !documentContext.isEmpty {
      body["document_context"] = documentContext
      body["document_name"] = documentName
    }
    if !worldContext.isEmpty { body["world_context"] = worldContext }
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/twin/session-drafts.json",
      body: body,
      timeout: 45
    )
    return Self.twinMessage(from: json, promptFallback: text)
  }

  func streamTwin(baseURL: URL, sessionID: String, stance: String, text: String, sensitive: Bool = false, documentContext: String = "", documentName: String = "", worldContext: String = "") -> AsyncStream<TwinStreamEvent> {
    AsyncStream(bufferingPolicy: .bufferingNewest(200)) { continuation in
      let task = Task {
        let url = Self.routeURL(baseURL: baseURL, path: "/twin/session-stream")
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.timeoutInterval = 120
        var payload: [String: Any] = [
          "session_id": sessionID,
          "companion_id": "twin",
          "companion_stance": stance,
          "draft_text": text,
          "prompt_shape": "A person is thinking through their work with you. Be direct and useful.",
          "sensitivity_tier": sensitive ? "sensitive" : "internal"
        ]
        if !documentContext.isEmpty {
          payload["document_context"] = documentContext
          payload["document_name"] = documentName
        }
        if !worldContext.isEmpty { payload["world_context"] = worldContext }
        request.httpBody = try? JSONSerialization.data(withJSONObject: payload)
        do {
          request = await authorize(request)
          try Self.requireTrustedWriteHost(
            baseURL,
            path: "/twin/session-stream",
            authenticated: request.value(forHTTPHeaderField: "Authorization") != nil
          )
          let (bytes, response) = try await session.bytes(for: request)
          guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            continuation.yield(.failed("Twin stream is unavailable right now."))
            continuation.finish()
            return
          }
          for try await line in bytes.lines {
            if Task.isCancelled { break }
            guard line.hasPrefix("data:") else { continue }
            let payload = line.dropFirst(5).trimmingCharacters(in: .whitespaces)
            guard let data = payload.data(using: .utf8),
                  let obj = try? JSONSerialization.jsonObject(with: data) as? JSONDictionary else { continue }
            switch obj.string(at: "type", fallback: "") {
            case "delta":
              continuation.yield(.delta(obj.string(at: "text", fallback: "")))
            case "done":
              continuation.yield(.done(
                receiptID: obj.string(at: "answer_receipt_id", fallback: ""),
                model: obj.string(at: "model_gateway_model_id", fallback: ""),
                firstTokenMs: Double(obj.int(at: "first_token_ms", fallback: 0)),
                durationMs: Double(obj.int(at: "duration_ms", fallback: 0)),
                memorySourceCount: obj.int(at: "brain_recall_source_count", fallback: 0),
                trustedSourceCount: obj.int(at: "trusted_context_source_count", fallback: 0)
              ))
            case "fallback":
              continuation.yield(.fallback(obj.string(at: "reason", fallback: "ordinary_route")))
            case "guarded":
              continuation.yield(.guarded(obj.string(at: "message", fallback: "Twin held this input for review.")))
            case "error":
              continuation.yield(.failed(obj.string(at: "message", fallback: "Twin could not answer.")))
            default:
              break
            }
          }
        } catch {
          continuation.yield(.failed(error.localizedDescription))
        }
        continuation.finish()
      }
      continuation.onTermination = { _ in task.cancel() }
    }
  }

  /// Stable identity for a feed row: ledger_seq when present (unique and
  /// stable across refreshes), else the receipt id, else a positional key.
  static func activityItemID(_ item: JSONDictionary, index: Int) -> String {
    let seq = item.int(at: "ledger_seq", fallback: -1)
    if seq >= 0 { return "seq_\(seq)" }
    let receipt = item.string(at: "receipt_id", fallback: "")
    return receipt.isEmpty ? "item_\(index)" : receipt
  }

  static func twinMessage(from json: JSONDictionary, promptFallback: String = "") -> TwinMessage {
    // Fallback id must be derived, not random: a UUID() here gives every
    // projection reload a fresh identity and breaks row diffing.
    let derived = "draft_\(json.string(at: "session_id", fallback: "s"))_\(json.string(at: "created_at", fallback: "t"))"
    let receipt = firstString(in: [json.value(at: "answer_receipt_id"), json.value(at: "draft_receipt_id")], fallback: derived)
    return TwinMessage(
      id: receipt,
      prompt: firstString(in: [json.value(at: "draft_text")], fallback: promptFallback),
      answer: json.string(at: "grounded_answer", fallback: ""),
      stance: json.string(at: "companion_stance", fallback: ""),
      memoryScore: min(100, max(0, json.int(at: "memory_relevance_score", fallback: 0))),
      memorySourceCount: json.int(at: "brain_recall_source_count", fallback: 0),
      trustedSourceCount: json.int(at: "trusted_context_source_count", fallback: 0),
      trustedSourceIDs: json.string(at: "trusted_context_source_ids", fallback: ""),
      personaStatus: json.string(at: "persona_contract_status", fallback: ""),
      voiceStatus: json.string(at: "voice_drift_status", fallback: ""),
      summary: json.string(at: "conversation_summary", fallback: ""),
      createdAt: json.string(at: "created_at", fallback: ""),
      answerReceiptID: json.string(at: "answer_receipt_id", fallback: "")
    )
  }

  static func firstInt(in values: [Int?], fallback: Int) -> Int {
    for value in values { if let value, value > 0 { return value } }
    return fallback
  }

  // MARK: - You (identity + clearance)

  func loadIdentity(baseURL: URL) async -> (session: IdentitySession?, clearance: Clearance?) {
    let auth = (try? await fetchJSON(baseURL: baseURL, path: "/local/auth-session.json")) ?? [:]
    // Harness clearance is a local-development projection. A hosted canary
    // derives authority from its verified session and must not manufacture a
    // Diagnostics error by probing an optional local-only route.
    let harness = Self.isTrustedLocalBaseURL(baseURL)
      ? ((try? await fetchJSON(baseURL: baseURL, path: "/harness/session.json")) ?? [:])
      : [:]

    var session: IdentitySession?
    if !auth.isEmpty {
      let verified = auth.dictionary(at: "verified_session")
      let local = verified.isEmpty ? auth.dictionary(at: "local_session") : verified
      let admission = auth.dictionary(at: "actor_admission")
      session = IdentitySession(
        userID: local.string(at: "user_id", fallback: "local_user"),
        tenantID: local.string(at: "tenant_id", fallback: "local_tenant"),
        role: local.string(at: "role", fallback: "viewer"),
        expiresAt: local.string(at: "expires_at", fallback: ""),
        roles: Self.stringList(auth.value(at: "roles")),
        admissionScope: admission.string(at: "scope", fallback: ""),
        admissionStatus: admission.string(at: "status", fallback: "")
      )
    }

    var clearance: Clearance?
    if !harness.isEmpty {
      let operatorSession = harness.dictionary(at: "operator_session")
      let boundaries = harness.dictionary(at: "authority_boundaries")
      clearance = Clearance(
        status: boundaries.string(at: "status", fallback: ""),
        safeControls: Self.stringList(operatorSession.value(at: "safe_controls")),
        blockedControls: Self.stringList(operatorSession.value(at: "blocked_controls")),
        blockedAuthorities: Self.stringList(boundaries.value(at: "blocked_authorities"))
      )
    }
    return (session, clearance)
  }

  // MARK: - Marketplace

  func loadCapabilities(baseURL: URL) async throws -> [Capability] {
    let json = try await fetchJSON(baseURL: baseURL, path: "/marketplace/capabilities.json")
    return json.array(at: "capabilities").map { cap in
      let source = cap.dictionary(at: "source")
      let risk = cap.dictionary(at: "risk")
      let state = cap.dictionary(at: "state")
      let tryValue = cap.value(at: "try")
      return Capability(
        id: cap.string(at: "id", fallback: ""),
        name: cap.string(at: "name", fallback: "Capability"),
        type: cap.string(at: "type", fallback: ""),
        summary: cap.string(at: "summary", fallback: ""),
        helpsWith: cap.string(at: "helps_with", fallback: ""),
        accessLine: cap.string(at: "access_line", fallback: ""),
        reviewReason: cap.string(at: "review_reason", fallback: ""),
        effectiveStatus: cap.string(at: "effective_status", fallback: cap.string(at: "status", fallback: "")),
        blockedActions: Self.stringList(cap.value(at: "blocked_actions")),
        installTargets: Self.stringList(cap.value(at: "install_targets")),
        requiredSecrets: Self.stringList(cap.value(at: "required_secrets")),
        lastScanned: cap.string(at: "last_scanned", fallback: ""),
        riskLevel: risk.string(at: "level", fallback: ""),
        riskFindings: Self.stringList(risk.value(at: "findings")),
        sourceOwner: source.string(at: "owner", fallback: ""),
        sourceKind: source.string(at: "kind", fallback: ""),
        sourceVersion: "\(source.value(at: "version").map { "\($0)" } ?? "")",
        artifactHash: source.string(at: "artifact_hash", fallback: ""),
        hasTry: !(tryValue is NSNull) && tryValue != nil,
        installed: state.bool(at: "installed", fallback: false),
        installedScopes: Self.stringList(state.value(at: "installed_scopes")),
        usedInBuilds: cap.int(at: "used_in_builds", fallback: 0)
      )
    }
    .filter { !$0.id.isEmpty }
  }

  func loadMarketplacePacks(baseURL: URL) async throws -> [MarketplacePack] {
    let json = try await fetchJSON(baseURL: baseURL, path: "/marketplace/packs.json")
    return json.array(at: "packs").map(Self.marketplacePack(from:)).filter { !$0.id.isEmpty }
  }

  static func marketplacePack(from pack: JSONDictionary) -> MarketplacePack {
    let publisher = pack.dictionary(at: "publisher")
    let state = pack.dictionary(at: "state")
    let trial = pack.dictionary(at: "safe_trial")
    let firstUse = pack.dictionary(at: "first_use")
    let update = pack.dictionary(at: "update")
    return MarketplacePack(
      id: pack.string(at: "id", fallback: ""),
      version: pack.string(at: "version", fallback: ""),
      name: pack.string(at: "name", fallback: "Pack"),
      job: pack.string(at: "job", fallback: ""),
      outcome: pack.string(at: "outcome", fallback: ""),
      sourceLane: pack.string(at: "source_lane", fallback: "mdx"),
      publisher: publisher.string(at: "name", fallback: "MDx"),
      publisherVerified: publisher.bool(at: "verified", fallback: false),
      supportedApps: Self.stringList(pack.value(at: "supported_apps")),
      items: Self.stringList(pack.value(at: "items")),
      requiredItems: Self.stringList(pack.value(at: "required_items")),
      optionalItems: Self.stringList(pack.value(at: "optional_items")),
      applicationTargets: Self.stringList(pack.value(at: "application_targets")),
      installScopes: Self.stringList(pack.value(at: "install_scopes")),
      dependencies: Self.stringList(pack.value(at: "dependencies")),
      conflicts: Self.stringList(pack.value(at: "conflicts")),
      setupSteps: Self.stringList(pack.value(at: "setup_steps")),
      requestedGrants: Self.stringList(pack.value(at: "requested_grants")),
      blockedGrants: Self.stringList(pack.value(at: "blocked_grants")),
      trialExample: trial.string(at: "example", fallback: ""),
      firstUseLabel: firstUse.string(at: "label", fallback: "Use it now"),
      firstUsePath: firstUse.string(at: "href", fallback: ""),
      improvement: pack.string(at: "improves_forge", fallback: ""),
      updateVersion: update.string(at: "version", fallback: ""),
      updateChange: update.string(at: "change", fallback: ""),
      permissionDiff: update.string(at: "permission_diff", fallback: ""),
      state: MarketplacePackState(
        installed: state.bool(at: "installed", fallback: false),
        enabled: state.bool(at: "enabled", fallback: false),
        scope: state.string(at: "scope", fallback: ""),
        version: state.string(at: "version", fallback: pack.string(at: "version", fallback: "")),
        applicationTargets: Self.stringList(state.value(at: "application_targets")),
        configured: state.bool(at: "configured", fallback: false),
        connected: state.bool(at: "connected", fallback: false),
        authorized: state.bool(at: "authorized", fallback: false),
        applicable: state.bool(at: "applicable", fallback: false),
        executable: state.bool(at: "executable", fallback: false),
        ready: state.bool(at: "ready", fallback: false),
        lifecycleState: state.string(at: "lifecycle_state", fallback: "not_installed"),
        heldItems: Self.stringList(state.value(at: "held_items")),
        rollbackVersion: state.string(at: "rollback_version", fallback: ""),
        useCount: state.int(at: "use_count", fallback: 0),
        acceptedCount: state.int(at: "accepted_count", fallback: 0),
        rejectedCount: state.int(at: "rejected_count", fallback: 0),
        installReceiptID: state.string(at: "install_receipt_id", fallback: "")
      )
    )
  }

  func installPack(baseURL: URL, pack: MarketplacePack, scope: String, applications: [String], actorID: String) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/marketplace/installs.json",
      body: ["actor_id": actorID, "pack_id": pack.id, "scope": scope, "application_targets": applications, "origin_surface": "macos"],
      timeout: 15
    )
    return Self.marketplaceOutcome(from: json, title: "Apply pack")
  }

  func actOnPack(baseURL: URL, pack: MarketplacePack, action: String, scope: String, applications: [String], version: String = "", actorID: String) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/marketplace/pack-actions.json",
      body: ["actor_id": actorID, "pack_id": pack.id, "action": action, "scope": scope, "application_targets": applications, "version": version, "origin_surface": "macos"],
      timeout: 15
    )
    return Self.marketplaceOutcome(from: json, title: "\(action.capitalized) pack")
  }

  func tryPack(baseURL: URL, pack: MarketplacePack, actorID: String) async throws -> PackTrialPreview {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/marketplace/pack-trials.json",
      body: ["actor_id": actorID, "pack_id": pack.id, "origin_surface": "macos"],
      timeout: 15
    )
    if json.string(at: "status", fallback: "") == "REFUSED" {
      throw RouteClientError.refused(json.string(at: "reason", fallback: "The kernel refused the trial."))
    }
    let trial = json.dictionary(at: "trial")
    let after = json.dictionary(at: "after")
    return PackTrialPreview(
      packID: pack.id,
      example: trial.string(at: "example", fallback: pack.trialExample),
      previewApps: Self.stringList(after.value(at: "preview_available_in")),
      requestedGrants: Self.stringList(json.value(at: "requested_grants")),
      blockedGrants: Self.stringList(json.value(at: "blocked_grants")),
      receiptID: json.string(at: "record_id", fallback: ""),
      line: json.string(at: "line", fallback: "Safe trial complete. Nothing external changed.")
    )
  }

  func installCapability(baseURL: URL, capabilityID: String, scope: String, actorID: String) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/marketplace/installs.json",
      body: ["actor_id": actorID, "capability_id": capabilityID, "scope": scope],
      timeout: 15
    )
    return Self.marketplaceOutcome(from: json, title: "Add capability")
  }

  func revokeCapability(baseURL: URL, capabilityID: String, scope: String, actorID: String) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/marketplace/revocations.json",
      body: ["actor_id": actorID, "capability_id": capabilityID, "scope": scope, "reason": "Removed from the native app."],
      timeout: 15
    )
    return Self.marketplaceOutcome(from: json, title: "Remove capability")
  }

  func approveCapability(baseURL: URL, capabilityID: String, decision: String, scope: String, readOnly: Bool = false, actorID: String) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/marketplace/approvals.json",
      body: ["actor_id": actorID, "capability_id": capabilityID, "decision": decision, "scope": scope, "read_only": readOnly],
      timeout: 15
    )
    return Self.marketplaceOutcome(from: json, title: "\(decision.capitalized) capability")
  }

  func tryCapability(baseURL: URL, capabilityID: String, actorID: String) async throws -> TryPreview {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/marketplace/try-safely.json",
      body: ["actor_id": actorID, "capability_id": capabilityID],
      timeout: 15
    )
    let preview = json.dictionary(at: "preview")
    return TryPreview(
      capabilityID: capabilityID,
      helps: preview.string(at: "helps", fallback: ""),
      appearsIn: preview.string(at: "appears_in", fallback: ""),
      example: preview.string(at: "example", fallback: ""),
      line: json.string(at: "line", fallback: "A read-only preview. Nothing ran and nothing changed.")
    )
  }

  static func marketplaceOutcome(from json: JSONDictionary, title: String) -> RunActionOutcome {
    let status = json.string(at: "status", fallback: "")
    let receiptID = json.string(at: "record_id", fallback: "")
    let ok = status == "OK" && !receiptID.isEmpty
    return RunActionOutcome(
      title: title,
      status: ok ? "OK" : (status.isEmpty ? "REFUSED" : status),
      detail: Self.firstString(in: [json.value(at: "line"), json.value(at: "reason")], fallback: status),
      receiptID: receiptID
    )
  }

  // MARK: - Message

  func loadMessageChannels(baseURL: URL) async throws -> [MessageChannel] {
    let json = try await fetchJSON(baseURL: baseURL, path: "/messages/channels/projection.json")
    return json.array(at: "channels").map { channel in
      MessageChannel(
        id: channel.string(at: "channel_id", fallback: "local-ops"),
        name: channel.string(at: "name", fallback: ""),
        description: channel.string(at: "description", fallback: ""),
        topic: channel.string(at: "topic", fallback: ""),
        kind: channel.string(at: "channel_kind", fallback: "channel"),
        visibility: channel.string(at: "visibility", fallback: "public"),
        status: channel.string(at: "status", fallback: "active"),
        memberCount: channel.int(at: "member_count", fallback: channel.array(at: "members").count)
      )
    }
    .filter { $0.status != "archived" }
  }

  func loadThreadMessages(baseURL: URL, channelID: String = "", limit: Int = 250) async throws -> [ThreadMessage] {
    var path = "/messages/thread-messages/projection.json?limit=\(max(1, min(limit, 500)))"
    if !channelID.isEmpty {
      let encoded = channelID.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? channelID
      path += "&channel_id=\(encoded)"
    }
    let json = try await fetchJSON(baseURL: baseURL, path: path)
    return json.array(at: "messages").map { message in
      let receiptID = message.string(at: "message_receipt_id", fallback: message.string(at: "receipt_id", fallback: ""))
      return ThreadMessage(
        id: receiptID.isEmpty ? message.string(at: "message_id", fallback: UUID().uuidString) : receiptID,
        messageID: message.string(at: "message_id", fallback: ""),
        threadID: message.string(at: "thread_id", fallback: ""),
        channelID: message.string(at: "channel_id", fallback: "local-ops"),
        actorID: message.string(at: "actor_id", fallback: ""),
        actorName: message.string(at: "actor_display_name", fallback: humanLabel(message.string(at: "actor_id", fallback: "someone"))),
        actorType: message.string(at: "actor_type", fallback: "system"),
        body: message.string(at: "body", fallback: ""),
        contentType: message.string(at: "content_type", fallback: "text"),
        contentPayload: message.string(at: "content_payload", fallback: ""),
        receiptKind: message.string(at: "message_receipt_kind", fallback: ""),
        receiptID: receiptID,
        sourceReceiptID: message.string(at: "source_receipt_id", fallback: ""),
        sourceReceiptKind: message.string(at: "source_receipt_kind", fallback: ""),
        sequence: message.int(at: "sequence", fallback: 0),
        createdAt: message.string(at: "created_at", fallback: "")
      )
    }
    .sorted { $0.sequence < $1.sequence }
  }

  func loadActivity(baseURL: URL) async throws -> (areas: [ActivityArea], items: [ActivityItem]) {
    let json = try await fetchJSON(baseURL: baseURL, path: "/messages/activity/projection.json")
    let areas = json.array(at: "areas").map { area in
      ActivityArea(
        id: area.string(at: "channel_id", fallback: ""),
        label: area.string(at: "label", fallback: "Area"),
        itemCount: area.int(at: "item_count", fallback: 0)
      )
    }
    let items = json.array(at: "items").enumerated().map { index, item in
      ActivityItem(
        id: Self.activityItemID(item, index: index),
        area: item.string(at: "area", fallback: item.string(at: "channel_id", fallback: "")),
        channelID: item.string(at: "channel_id", fallback: ""),
        actor: item.string(at: "actor", fallback: "MDx"),
        actorType: item.string(at: "actor_type", fallback: ""),
        headline: item.string(at: "headline", fallback: item.string(at: "actor", fallback: "Activity")),
        detail: item.string(at: "detail", fallback: ""),
        receiptKind: item.string(at: "receipt_kind", fallback: ""),
        receiptID: item.string(at: "receipt_id", fallback: ""),
        seq: item.int(at: "ledger_seq", fallback: index)
      )
    }
    .sorted { $0.seq > $1.seq }
    return (areas, items)
  }

  func loadActionCards(baseURL: URL) async throws -> [ActionCard] {
    let json = try await fetchJSON(baseURL: baseURL, path: "/messages/action-requests/projection.json")
    return json.array(at: "requests").map { req in
      ActionCard(
        id: req.string(at: "action_request_receipt_id", fallback: req.string(at: "action_request_id", fallback: "card")),
        requestID: req.string(at: "action_request_id", fallback: ""),
        title: req.string(at: "title", fallback: "Action"),
        summary: req.string(at: "summary", fallback: ""),
        proposedAction: req.string(at: "proposed_action", fallback: ""),
        sourceReceiptKind: req.string(at: "source_receipt_kind", fallback: ""),
        channelID: req.string(at: "channel_id", fallback: ""),
        allowedVerdicts: req.string(at: "allowed_verdicts", fallback: "").split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) },
        gateState: req.string(at: "gate_state", fallback: "")
      )
    }
    .filter { $0.gateState == "WAITING_FOR_HUMAN_VERDICT" }
  }

  func sendThreadMessage(
    baseURL: URL,
    text: String,
    actorID: String,
    channelID: String = "local-ops",
    threadID: String = "",
    messageID: String = ""
  ) async throws -> RunActionOutcome {
    var body: [String: Any] = [
      "thread_id": threadID.isEmpty ? "thread_native" : threadID,
      "message_id": messageID.isEmpty ? "mac_msg_\(UUID().uuidString.prefix(8).lowercased())" : messageID,
      "body": text,
      "actor_id": actorID,
      "channel_id": channelID
    ]
    if threadID.isEmpty { body.removeValue(forKey: "thread_id") }
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/messages/thread-messages.json",
      body: body,
      timeout: 15
    )
    return Self.runOutcome(from: json, title: "Send message", idKeys: ["message_receipt_id", "thread_message_receipt_id", "message_thread_message_receipt_id", "receipt_id"])
  }

  func loadPresence(baseURL: URL) async throws -> (activeNow: Int, people: [PresencePerson]) {
    let json = try await fetchJSON(baseURL: baseURL, path: "/messages/presence/projection.json")
    let roster = json.array(at: "roster").map { person in
      PresencePerson(
        id: person.string(at: "actor_id", fallback: person.string(at: "display_name", fallback: "person")),
        displayName: person.string(at: "display_name", fallback: "Someone"),
        actorType: person.string(at: "actor_type", fallback: ""),
        lastAction: person.string(at: "last_action", fallback: ""),
        actionCount: person.int(at: "action_count", fallback: 0),
        active: person.bool(at: "active", fallback: false)
      )
    }
    return (json.int(at: "active_now_count", fallback: 0), roster)
  }

  func postActionVerdict(baseURL: URL, card: ActionCard, verdict: String, note: String, actorID: String) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/messages/action-verdicts.json",
      body: [
        "action_verdict_id": "mac_\(verdict)_\(card.requestID)",
        "verdict": verdict,
        "action_request_receipt_id": card.id,
        "decision_note": note.isEmpty ? "\(verdict.capitalized) from the native app." : note,
        "actor_id": actorID
      ],
      timeout: 15
    )
    let status = json.string(at: "status", fallback: "")
    let receiptID = json.string(at: "action_verdict_receipt_id", fallback: "")
    let recorded = (status.localizedCaseInsensitiveContains("RECORDED") || status.localizedCaseInsensitiveContains("VERDICT")) && !receiptID.isEmpty
    return RunActionOutcome(
      title: "\(verdict.capitalized) action",
      status: recorded ? "RECORDED" : (status.isEmpty ? "FAILED_MISSING_RECEIPT" : "FAILED_MISSING_RECEIPT_\(status)"),
      detail: Self.firstString(in: [json.value(at: "gate_state"), json.value(at: "applied_receipt_kind"), json.value(at: "reason")], fallback: status),
      receiptID: receiptID
    )
  }

  // MARK: - Pages

  /// The library, joined with the lifecycle projection so each page carries its
  /// state, trust, owner, and freshness in one object.
  func loadPages(baseURL: URL) async throws -> [Page] {
    async let listTask = fetchJSON(baseURL: baseURL, path: "/pages.json")
    async let lifeTask = try? fetchJSON(baseURL: baseURL, path: "/pages/lifecycle/projection.json")
    // The library list is the source of truth; lifecycle enrichment is optional.
    let list = try await listTask
    let life = await lifeTask ?? [:]

    var lifeByID: [String: JSONDictionary] = [:]
    for doc in life.array(at: "documents") {
      lifeByID[doc.string(at: "document_id", fallback: "")] = doc
    }

    return list.array(at: "documents").map { doc in
      let id = doc.string(at: "document_id", fallback: "")
      let lc = lifeByID[id] ?? [:]
      let trust = lc.dictionary(at: "trust")
      let freshness = lc.dictionary(at: "freshness")
      return Page(
        id: id,
        title: doc.string(at: "title", fallback: "Untitled page"),
        bodyRef: doc.string(at: "body_ref", fallback: ""),
        author: doc.string(at: "author_actor_id", fallback: ""),
        visibility: doc.string(at: "visibility", fallback: ""),
        revisionID: doc.string(at: "revision_id", fallback: ""),
        sourceReceiptIDs: Self.stringList(doc.value(at: "source_receipt_ids")),
        charterEvidenceIDs: Self.stringList(doc.value(at: "charter_evidence_ids")),
        state: lc.string(at: "state", fallback: ""),
        owner: trust.string(at: "owner", fallback: ""),
        reviewer: trust.string(at: "reviewer", fallback: ""),
        trustedForAI: trust.bool(at: "trusted_for_ai", fallback: false),
        charterBacked: trust.bool(at: "charter_backed", fallback: false),
        evidenceCount: trust.int(at: "evidence_count", fallback: 0),
        stale: freshness.bool(at: "stale", fallback: false),
        reviewIntervalDays: freshness.int(at: "review_interval_days", fallback: 0),
        events: lc.array(at: "events").enumerated().map { index, event in
          PageLifecycleEvent(
            id: event.string(at: "receipt_id", fallback: "event_\(index)"),
            label: event.string(at: "label", fallback: "Event"),
            state: event.string(at: "state", fallback: ""),
            actorID: event.string(at: "actor_id", fallback: ""),
            note: event.string(at: "note", fallback: ""),
            receiptID: event.string(at: "receipt_id", fallback: "")
          )
        }
      )
    }
    .filter { !$0.id.isEmpty }
  }

  func publishPage(baseURL: URL, draft: PagePublishDraft, approvalDecisionReceiptID: String, actorID: String) async throws -> RunActionOutcome {
    let body: [String: Any] = [
      "document_id": draft.resolvedDocumentID(),
      "page_type": draft.pageType,
      "approval_decision_receipt_id": approvalDecisionReceiptID,
      "actor_id": actorID
    ]
    let json = try await postJSON(baseURL: baseURL, path: "/pages/publications.json", body: body, timeout: 15)
    let status = json.string(at: "status", fallback: "")
    let receiptID = json.string(at: "publication_receipt_id", fallback: "")
    let published = status.localizedCaseInsensitiveContains("PUBLISHED") && !receiptID.isEmpty
    let detail = published
      ? "Published as \(PagePublishDraft.typeLabel(draft.pageType))."
      : Self.firstString(in: [json.value(at: "reason"), json.value(at: "status")], fallback: "Publish was not accepted.")
    return RunActionOutcome(
      title: draft.isEdit ? "Update page" : "Publish page",
      status: published ? "PUBLISHED" : "REFUSED",
      detail: detail,
      receiptID: receiptID
    )
  }

  func loadPageDrafts(baseURL: URL) async throws -> [PageDraftRecord] {
    let json = try await fetchJSON(baseURL: baseURL, path: "/pages/edit-drafts/projection.json")
    return json.array(at: "drafts").compactMap { row in
      let id = row.string(at: "draft_id", fallback: "")
      guard !id.isEmpty else { return nil }
      return PageDraftRecord(
        id: id,
        documentID: row.string(at: "document_id", fallback: ""),
        title: row.string(at: "title", fallback: "Untitled page"),
        bodyRef: row.string(at: "body_ref", fallback: ""),
        receiptID: row.string(at: "edit_draft_receipt_id", fallback: ""),
        revisionID: row.string(at: "revision_id", fallback: "")
      )
    }
  }

  func loadPageReviewRequests(baseURL: URL) async throws -> [PageReviewRequest] {
    let json = try await fetchJSON(baseURL: baseURL, path: "/pages/approval-requests/projection.json")
    return json.array(at: "requests").compactMap { row in
      let id = row.string(at: "approval_request_id", fallback: "")
      guard !id.isEmpty else { return nil }
      return PageReviewRequest(
        id: id,
        receiptID: row.string(at: "approval_request_receipt_id", fallback: ""),
        documentID: row.string(at: "document_id", fallback: ""),
        draftID: row.string(at: "draft_id", fallback: ""),
        sourceDraftReceiptID: row.string(at: "source_edit_draft_receipt_id", fallback: ""),
        decisionState: row.string(at: "decision_state", fallback: "PENDING_DECISION"),
        decisionReceiptID: row.string(at: "approval_decision_receipt_id", fallback: ""),
        decisionOutcome: row.string(at: "decision_outcome", fallback: "")
      )
    }
  }

  func savePageDraft(baseURL: URL, draft: PagePublishDraft, actorID: String) async throws -> (PagePublishDraft, RunActionOutcome) {
    var saved = draft
    let documentID = draft.resolvedDocumentID()
    // Every save is a new immutable revision. A prior human approval must not
    // silently carry across changed words.
    let draftID = "draft_\(UUID().uuidString.prefix(8).lowercased())"
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/pages/edit-drafts.json",
      body: [
        "document_id": documentID,
        "draft_id": draftID,
        "title": draft.title.trimmingCharacters(in: .whitespacesAndNewlines),
        "body_text": draft.body,
        "revision_id": "rev_\(UUID().uuidString.prefix(8).lowercased())",
        "actor_id": actorID
      ],
      timeout: 15
    )
    let status = json.string(at: "status", fallback: "")
    let receiptID = json.string(at: "edit_draft_receipt_id", fallback: "")
    let recorded = status.localizedCaseInsensitiveContains("SAVED") && !receiptID.isEmpty
    saved.documentID = documentID
    saved.draftID = draftID
    saved.draftReceiptID = receiptID
    return (
      saved,
      RunActionOutcome(
        title: "Save draft",
        status: recorded ? "SAVED" : "REFUSED",
        detail: recorded ? "Draft recorded. Publication is still blocked until review." : Self.firstString(in: [json.value(at: "reason"), json.value(at: "status")], fallback: "Draft was not recorded."),
        receiptID: receiptID
      )
    )
  }

  func requestPageReview(baseURL: URL, draft: PagePublishDraft, actorID: String) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/pages/approval-requests.json",
      body: [
        "approval_request_id": "approval_\(UUID().uuidString.prefix(8).lowercased())",
        "source_edit_draft_receipt_id": draft.draftReceiptID,
        "document_id": draft.resolvedDocumentID(),
        "draft_id": draft.draftID,
        "requested_visibility": draft.visibility,
        "actor_id": actorID
      ],
      timeout: 15
    )
    let status = json.string(at: "status", fallback: "")
    let receiptID = json.string(at: "approval_request_receipt_id", fallback: "")
    let recorded = status.localizedCaseInsensitiveContains("RECORDED") && !receiptID.isEmpty
    return RunActionOutcome(
      title: "Ask for review",
      status: recorded ? "IN_REVIEW" : "REFUSED",
      detail: recorded ? "Review requested. A human decision is required before publish." : Self.firstString(in: [json.value(at: "reason"), json.value(at: "status")], fallback: "Review request was not recorded."),
      receiptID: receiptID
    )
  }

  func decidePageReview(baseURL: URL, request: PageReviewRequest, approve: Bool, actorID: String) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: approve ? "/pages/approval-decisions/approve.json" : "/pages/approval-decisions/reject.json",
      body: [
        "approval_decision_id": "decision_\(UUID().uuidString.prefix(8).lowercased())",
        "approval_request_receipt_id": request.receiptID,
        "decision_note": approve ? "Approved in the native Pages review workspace." : "Sent back from the native Pages review workspace.",
        "actor_id": actorID
      ],
      timeout: 15
    )
    let status = json.string(at: "status", fallback: "")
    let receiptID = json.string(at: "approval_decision_receipt_id", fallback: "")
    let recorded = status.localizedCaseInsensitiveContains(approve ? "APPROVED" : "REJECTED") && !receiptID.isEmpty
    return RunActionOutcome(
      title: approve ? "Approve page" : "Send back",
      status: recorded ? (approve ? "APPROVED" : "NEEDS_WORK") : "REFUSED",
      detail: recorded ? (approve ? "Approved for the next governed publish step." : "Sent back. Update the draft before asking again.") : Self.firstString(in: [json.value(at: "reason"), json.value(at: "status")], fallback: "Decision was not recorded."),
      receiptID: receiptID
    )
  }

  func loadPageBody(baseURL: URL, documentID: String) async -> PageBody? {
    guard let json = try? await fetchJSON(baseURL: baseURL, path: "/pages/\(documentID)/body") else { return nil }
    return PageBody(
      documentID: documentID,
      body: json.string(at: "body", fallback: ""),
      contentType: json.string(at: "content_type", fallback: "text/markdown")
    )
  }

  /// Real server search (hits bodies, not just titles). Returns ordered
  /// document ids plus the server's summary/why so the rail can show them.
  func searchPages(baseURL: URL, query: String) async -> [(documentID: String, summary: String)] {
    let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty,
          var components = URLComponents(url: baseURL.appending(path: "/pages/search.json"), resolvingAgainstBaseURL: false)
    else { return [] }
    components.queryItems = [URLQueryItem(name: "q", value: trimmed)]
    guard let url = components.url else { return [] }
    var request = URLRequest(url: url)
    request.timeoutInterval = 4
    request.setValue("application/json", forHTTPHeaderField: "Accept")
    request = await authorize(request)
    guard let (data, response) = try? await session.data(for: request),
          let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode),
          let json = try? JSONSerialization.jsonObject(with: data) as? JSONDictionary
    else { return [] }
    return json.array(at: "results").map {
      (documentID: $0.string(at: "document_id", fallback: ""), summary: $0.string(at: "summary", fallback: ""))
    }
    .filter { !$0.documentID.isEmpty }
  }

  // MARK: - Twin attachments

  /// Parse a local PDF with LiteParse, then bind the extracted text to the Twin
  /// session as a private context packet. Twin grounds on it for the next turn.
  func attachPDF(
    baseURL: URL,
    sessionID: String,
    actorID: String,
    fileName: String,
    data: Data,
    sensitive: Bool
  ) async throws -> TwinAttachment {
    guard data.count <= Self.maxPDFBytesForInlineParse else {
      throw RouteClientError.refused("This PDF is too large for native inline parsing. Use a smaller file or add route-side chunking first.")
    }
    let base64 = data.base64EncodedString()
    let artifactID = Self.sanitizedArtifactID(from: fileName)
    let parse = try await postJSON(
      baseURL: baseURL,
      path: "/twin/artifacts/liteparse-parses.json",
      body: [
        "artifact_pdf_base64": base64,
        "artifact_id": artifactID,
        "artifact_name": fileName,
        "mime_type": "application/pdf",
        "session_id": sessionID,
        "actor_id": actorID
      ],
      timeout: 45
    )
    let extractedText = parse.string(at: "extracted_text", fallback: "")
    guard !extractedText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
      throw RouteClientError.refused("No readable text found in this PDF. Scanned or image-only files are not supported yet.")
    }
    _ = try await postJSON(
      baseURL: baseURL,
      path: "/twin/artifact-contexts.json",
      body: [
        "session_id": sessionID,
        "artifact_name": fileName,
        "mime_type": "application/pdf",
        "artifact_text": extractedText,
        "actor_id": actorID,
        "privacy_class": sensitive ? "user_private_sensitive" : "user_private"
      ],
      timeout: 30
    )
    return TwinAttachment(
      id: artifactID,
      name: fileName,
      pageCount: parse.int(at: "page_count", fallback: 0),
      charCount: parse.int(at: "extracted_text_char_count", fallback: extractedText.count),
      parseState: parse.string(at: "parse_state", fallback: ""),
      extractedText: Self.boundedDocumentContext(extractedText)
    )
  }

  /// Kernel requires lowercase letters, digits, `_`, or `-` (max 96 chars).
  static func sanitizedArtifactID(from fileName: String) -> String {
    let mapped = fileName.lowercased().map { char -> Character in
      (char.isLetter || char.isNumber) ? char : "_"
    }
    let cleaned = String(mapped).prefix(64)
    let trimmed = cleaned.trimmingCharacters(in: CharacterSet(charactersIn: "_"))
    let base = trimmed.isEmpty ? "pdf" : trimmed
    return "twin_pdf_\(base)"
  }

  func scoutRepo(baseURL: URL, repoID: String) async throws -> ScoutResult {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/forge/repo-task-scout.json",
      body: ["repo_id": repoID],
      timeout: 8
    )
    return Self.scoutResult(from: json)
  }

  func classifyIntent(baseURL: URL, ask: String, repo: String, actorID: String) async throws -> WorkRecommendation {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/forge/work-classifications.json",
      body: ["ask": ask, "repo": repo.isEmpty ? "MDx" : repo, "actor_id": actorID],
      timeout: 8
    )
    return WorkRecommendation(
      shape: json.string(at: "recommended_shape", fallback: "run"),
      width: json.int(at: "recommended_width", fallback: 1),
      label: json.string(at: "recommendation_label", fallback: "A single run"),
      reason: json.string(at: "recommendation_reason", fallback: ""),
      taskClass: json.string(at: "task_class", fallback: ""),
      complexityTier: json.string(at: "complexity_tier", fallback: ""),
      confidencePct: json.int(at: "confidence_pct", fallback: 0),
      rationale: json.string(at: "rationale", fallback: ""),
      suggestedCheckCommands: Self.stringList(json.value(at: "suggested_checks")),
      allowedWriteScope: json.string(at: "allowed_write_scope", fallback: ""),
      blockedPaths: json.string(at: "blocked_paths", fallback: "")
    )
  }

  func createMission(baseURL: URL, draft: MissionDraft, actorID: String) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/forge/long-horizon-missions.json",
      body: [
        "goal": draft.goal,
        "done_when": draft.doneWhen,
        "validation_commands": draft.validationCommands,
        "allowed_write_scope": draft.allowedWriteScope,
        "blocked_paths": draft.blockedPaths,
        "non_goals": draft.nonGoals,
        "fleet_width": max(1, draft.fleetWidth),
        "max_cost_cents": max(1, draft.maxCostDollars * 100),
        "max_runtime_ms": max(60_000, draft.maxRuntimeHours * 3_600_000),
        "checkpoint_cadence_minutes": max(1, draft.checkpointCadenceMinutes),
        "actor_id": actorID
      ],
      timeout: 20
    )
    return Self.runOutcome(from: json, title: "Create mission", idKeys: ["mission_id", "mission_receipt_id"])
  }

  func missionCheckpoint(baseURL: URL, missionID: String, event: String, milestoneID: String, note: String, actorID: String) async throws -> RunActionOutcome {
    var body: [String: Any] = ["mission_id": missionID, "checkpoint_event": event, "actor_id": actorID]
    if !milestoneID.isEmpty { body["milestone_id"] = milestoneID }
    if !note.isEmpty {
      body["summary"] = note
      if event == "mission_steered" { body["steering_note"] = note }
    }
    let json = try await postJSON(baseURL: baseURL, path: "/forge/long-horizon-mission-checkpoints.json", body: body)
    return Self.runOutcome(from: json, title: event.replacingOccurrences(of: "_", with: " ").capitalized, idKeys: ["checkpoint_receipt_id", "receipt_id"])
  }

  func launchMission(baseURL: URL, mission: ForgeMission, repoID: String, actorID: String) async throws -> MissionLaunchOutcome {
    let milestone = mission.launchableMilestone
    var body: [String: Any] = [
      "mission_id": mission.id,
      "actor_id": actorID,
      "requested_workers": max(1, mission.fleetWidth)
    ]
    if let milestone {
      body["milestone_id"] = milestone.milestoneID
    }
    if !repoID.isEmpty && repoID != "mdx-self" {
      body["repo_id"] = repoID
    }
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/forge/long-horizon-mission-launches.json",
      body: body,
      timeout: 30
    )
    let action = Self.runOutcome(
      from: json,
      title: "Launch mission",
      idKeys: ["delegated_response.run_id", "mission_id", "receipt_id"]
    )
    return MissionLaunchOutcome(
      action: action,
      delegatedRunID: json.string(at: "delegated_response.run_id", fallback: ""),
      delegatedRoute: json.string(at: "delegated_route", fallback: ""),
      milestoneID: json.string(at: "milestone_id", fallback: milestone?.milestoneID ?? "")
    )
  }

  func reviseRun(baseURL: URL, runID: String, comment: String, actorID: String) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/forge/run-revisions.json",
      body: ["run_id": runID, "comment": comment, "allowed_commands": [], "actor_id": actorID],
      timeout: 30
    )
    return Self.runOutcome(from: json, title: "Request changes", idKeys: ["run_id", "run_revision_receipt_id", "revision_receipt_id", "receipt_id"])
  }

  func auditRenderedRun(
    baseURL: URL,
    runID: String,
    previewPath: String,
    viewport: BrowserViewport,
    autoRevise: Bool,
    actorID: String
  ) async throws -> BrowserAuditResult {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/forge/browser-audits.json",
      body: [
        "run_id": runID,
        "preview_path": previewPath,
        "viewport_width": Int(viewport.size.width),
        "viewport_height": Int(viewport.size.height),
        "color_scheme": "dark",
        "auto_revise": autoRevise,
        "actor_id": actorID
      ],
      timeout: 120
    )
    if json.string(at: "status", fallback: "").localizedCaseInsensitiveContains("REFUSED") {
      throw RouteClientError.refused(json.string(at: "reason", fallback: "Forge refused the browser audit."))
    }
    let critique = json.dictionary(at: "critique")
    let revision = json.dictionary(at: "revision")
    let findings = json.array(at: "findings").enumerated().map { index, finding in
      BrowserFinding(
        id: finding.string(at: "id", fallback: "finding_\(index)"),
        severity: finding.string(at: "severity", fallback: "advisory"),
        title: finding.string(at: "title", fallback: "Render finding"),
        detail: finding.string(at: "detail", fallback: "Inspect the captured browser evidence."),
        selector: finding.string(at: "selector", fallback: "")
      )
    }
    let screenshotData = Data(base64Encoded: json.string(at: "screenshotBase64", fallback: ""))
    return BrowserAuditResult(
      auditID: json.string(at: "auditId", fallback: ""),
      runID: json.string(at: "runId", fallback: runID),
      branch: json.string(at: "branch", fallback: ""),
      previewPath: json.string(at: "previewPath", fallback: previewPath),
      url: json.string(at: "url", fallback: ""),
      screenshot: screenshotData,
      screenshotSHA256: json.string(at: "screenshotSha256", fallback: ""),
      tracePath: json.string(at: "tracePath", fallback: ""),
      verdict: critique.string(at: "verdict", fallback: findings.isEmpty ? "APPROVE" : "REVISE"),
      critique: critique.string(at: "summary", fallback: "The browser audit completed."),
      critiqueModel: critique.string(at: "model_id", fallback: "deterministic-browser-sensors"),
      findings: findings,
      consoleErrorCount: json.array(at: "consoleErrors").count + json.array(at: "pageErrors").count,
      networkFailureCount: json.array(at: "failedRequests").count + json.array(at: "badResponses").count,
      revisionStarted: revision.bool(at: "started", fallback: false),
      revisionRunID: revision.string(at: "run_id", fallback: "")
    )
  }

  func fetchReviewPacket(baseURL: URL, runID: String, actorID: String) async throws -> ReviewPacket {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/forge/review-packet.json",
      body: ["run_id": runID, "actor_id": actorID]
    )
    return Self.reviewPacket(from: json)
  }

  func startRun(baseURL: URL, intent: String, repoID: String, fleetWidth: Int, allowedCommands: [String], actorID: String) async throws -> RunActionOutcome {
    if fleetWidth > 4 {
      let json = try await postJSON(
        baseURL: baseURL,
        path: "/forge/fleet-plans.json",
        body: [
          "goal": intent,
          "checks": allowedCommands,
          "repo_id": repoID,
          "actor_id": actorID,
          "requested_width": fleetWidth,
          "return_pending": true
        ],
        timeout: 180
      )
      return Self.runOutcome(from: json, title: "Plan fleet build", idKeys: ["fleet_id", "plan_receipt_id"])
    }
    let priorRunIDs = await priorRunIDs(baseURL: baseURL)
    do {
      let json = try await postJSON(
        baseURL: baseURL,
        path: "/forge/runs.json",
        body: [
          "intent": intent,
          "repo_id": repoID,
          "allowed_commands": allowedCommands,
          "actor_id": actorID,
          "fleet_width": fleetWidth
        ],
        timeout: 30
      )
      return Self.runOutcome(from: json, title: "Start run", idKeys: ["run_id"])
    } catch let error as URLError where error.code == .timedOut {
      if let projection = try? await fetchJSON(baseURL: baseURL, path: "/forge/runs/projection.json"),
         let recovered = Self.recoveredStartedRun(
           from: projection,
           intent: intent,
           repoID: repoID,
           excluding: priorRunIDs
         )
      {
        return recovered
      }
      throw error
    }
  }

  func ratifyFleetPlan(baseURL: URL, fleetID: String, reason: String, actorID: String) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/forge/fleet-plan-ratifications.json",
      body: ["fleet_id": fleetID, "reason": reason, "actor_id": actorID],
      timeout: 20
    )
    return Self.runOutcome(from: json, title: "Ratify fleet", idKeys: ["ratification_receipt_id", "receipt_id"])
  }

  func startFleetRun(baseURL: URL, fleetID: String, repoID: String, actorID: String) async throws -> RunActionOutcome {
    var body: [String: Any] = ["fleet_id": fleetID, "actor_id": actorID]
    if !repoID.isEmpty { body["repo_id"] = repoID }
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/forge/fleet-runs.json",
      body: body,
      timeout: 20
    )
    return Self.runOutcome(from: json, title: "Start fleet", idKeys: ["run_started_receipt_id", "receipt_id"])
  }

  func shipRun(baseURL: URL, runID: String, commitSha: String, reason: String, actorID: String) async throws -> RunActionOutcome {
    let priorReceipts = await priorShipReceipts(baseURL: baseURL)
    do {
      let json = try await postJSON(
        baseURL: baseURL,
        path: "/forge/run-ship-decisions.json",
        body: ["run_id": runID, "commit_sha": commitSha, "reason": reason, "actor_id": actorID],
        timeout: 30
      )
      return Self.runOutcome(from: json, title: "Ship decision", idKeys: ["run_ship_decision_receipt_id", "decision_receipt_id"])
    } catch let error as URLError where error.code == .timedOut {
      if let projection = try? await fetchJSON(baseURL: baseURL, path: "/forge/ship-decisions/projection.json"),
         let recovered = Self.recoveredShipDecision(
           from: projection,
           runID: runID,
           excluding: priorReceipts
         )
      {
        return recovered
      }
      throw error
    }
  }

  func controlRun(baseURL: URL, runID: String, control: String, note: String, actorID: String) async throws -> RunActionOutcome {
    let json = try await postJSON(
      baseURL: baseURL,
      path: "/forge/run-controls.json",
      body: ["run_id": runID, "control": control, "note": note, "actor_id": actorID],
      timeout: 30
    )
    return Self.runOutcome(from: json, title: control.capitalized, idKeys: ["run_control_receipt_id", "control_receipt_id", "receipt_id"])
  }

  func stageMachineFaceOff(baseURL: URL, runnerID: String, actorID: String) async throws -> RunActionOutcome {
    let projectionPath = "/forge/machine-league/face-offs/projection.json"
    let priorProjection = try? await fetchJSON(baseURL: baseURL, path: projectionPath)
    let priorReceipts = Set(
      priorProjection?.array(at: "face_offs").map {
        $0.string(at: "face_off_receipt_id", fallback: "")
      } ?? []
    )
    do {
      let json = try await postJSON(
        baseURL: baseURL,
        path: "/forge/machine-league/face-offs.json",
        body: [
          "external_runner_id": runnerID,
          "task_class": "bug_fix",
          "language_pack_id": "rust-cargo",
          "eval_fixture": "rust_tax_rounding",
          "stage_external_trial": true,
          "execute_external_live": false,
          "actor_id": actorID
        ],
        timeout: 30
      )
      return Self.runOutcome(from: json, title: "Compare on a fixture", idKeys: ["face_off_receipt_id"])
    } catch let error as URLError where error.code == .timedOut {
      if let projection = try? await fetchJSON(baseURL: baseURL, path: projectionPath),
         let recovered = Self.recoveredFaceOffOutcome(
           from: projection,
           runnerID: runnerID,
           excluding: priorReceipts
         )
      {
        return recovered
      }
      throw error
    }
  }

  static func recoveredFaceOffOutcome(
    from projection: JSONDictionary,
    runnerID: String,
    excluding priorReceipts: Set<String>
  ) -> RunActionOutcome? {
    guard let faceOff = projection.array(at: "face_offs").first(where: { item in
      let receiptID = item.string(at: "face_off_receipt_id", fallback: "")
      return item.string(at: "runner_id", fallback: "") == runnerID
        && !receiptID.isEmpty
        && !priorReceipts.contains(receiptID)
    }) else {
      return nil
    }
    return RunActionOutcome(
      title: "Compare on a fixture",
      status: "FACE_OFF_STAGED_HELD",
      detail: "The held comparison packet was recorded after the request timed out.",
      receiptID: faceOff.string(at: "face_off_receipt_id", fallback: "")
    )
  }

  /// Admission-class writes take an appropriate window (30s) and, when the
  /// local request still times out, reconcile against the projection before
  /// reporting failure. This keeps a slow-but-successful kernel from returning
  /// "failed" and inviting a retry that mints a duplicate run or ship decision.
  /// Receipts-or-it-did-not-happen holds: recovery succeeds only when the
  /// projection actually shows the receipt for this action.

  /// A run with matching intent/repo that appeared after the POST began counts
  /// as started. `excluding` is the set of run ids seen before the POST.
  static func recoveredStartedRun(
    from projection: JSONDictionary,
    intent: String,
    repoID: String,
    excluding priorRunIDs: Set<String>
  ) -> RunActionOutcome? {
    let target = intent.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !target.isEmpty else { return nil }
    guard let match = forgeRuns(from: projection).first(where: { run in
      !run.id.isEmpty
        && !priorRunIDs.contains(run.id)
        && intentReconciles(run.intent, target: target)
        && (repoID.isEmpty || run.repo.isEmpty || run.repo == repoID)
    }) else {
      return nil
    }
    return RunActionOutcome(
      title: "Start run",
      status: requireReceiptStatus("RUN_STARTED_RECONCILED", receiptID: match.id),
      detail: "The local request timed out, but the run was admitted. Reconciled against the run projection.",
      receiptID: match.id
    )
  }

  /// A ship decision recorded for this run after the POST began counts as
  /// shipped. `excluding` is the set of decision receipts seen before the POST.
  static func recoveredShipDecision(
    from projection: JSONDictionary,
    runID: String,
    excluding priorReceipts: Set<String>
  ) -> RunActionOutcome? {
    guard !runID.isEmpty else { return nil }
    for decision in projection.array(at: "decisions") {
      guard decision.string(at: "run_id", fallback: "") == runID else { continue }
      let receipt = shipDecisionReceipt(decision)
      guard !receipt.isEmpty, !priorReceipts.contains(receipt) else { continue }
      return RunActionOutcome(
        title: "Ship decision",
        status: requireReceiptStatus("SHIP_DECISION_RECORDED_RECONCILED", receiptID: receipt),
        detail: "The local request timed out, but the ship decision was recorded. Reconciled against the ship-decision projection.",
        receiptID: receipt
      )
    }
    return nil
  }

  static func shipDecisionReceipt(_ decision: JSONDictionary) -> String {
    firstString(
      in: ["run_ship_decision_receipt_id", "decision_receipt_id", "receipt_id", "id"].map { decision.value(at: $0) },
      fallback: ""
    )
  }

  /// Kernels sometimes store a trimmed or normalized echo of the intent, so a
  /// clear prefix match in either direction still reconciles the same ask.
  static func intentReconciles(_ stored: String, target: String) -> Bool {
    let a = stored.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !a.isEmpty else { return false }
    if a == target { return true }
    let lowerA = a.lowercased()
    let lowerT = target.lowercased()
    let minLength = 8
    guard min(lowerA.count, lowerT.count) >= minLength else { return false }
    return lowerA.hasPrefix(lowerT) || lowerT.hasPrefix(lowerA)
  }

  private func priorRunIDs(baseURL: URL) async -> Set<String> {
    guard let json = try? await fetchJSON(baseURL: baseURL, path: "/forge/runs/projection.json") else { return [] }
    return Set(Self.forgeRuns(from: json).map(\.id))
  }

  private func priorShipReceipts(baseURL: URL) async -> Set<String> {
    guard let json = try? await fetchJSON(baseURL: baseURL, path: "/forge/ship-decisions/projection.json") else { return [] }
    return Set(json.array(at: "decisions").map { Self.shipDecisionReceipt($0) }.filter { !$0.isEmpty })
  }

  func streamEvents(baseURL: URL, streamRoute: String) -> AsyncStream<ForgeEvent> {
    AsyncStream(bufferingPolicy: .bufferingNewest(500)) { continuation in
      let task = Task {
        let url = Self.routeURL(baseURL: baseURL, path: streamRoute)
        var request = URLRequest(url: url)
        request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
        request.timeoutInterval = 8600
        do {
          request = await authorize(request)
          let (bytes, response) = try await session.bytes(for: request)
          guard let http = response as? HTTPURLResponse, (200..<300).contains(http.statusCode) else {
            continuation.finish()
            return
          }
          var index = 0
          for try await line in bytes.lines {
            if Task.isCancelled { break }
            guard line.hasPrefix("data:") else { continue }
            let payload = line.dropFirst(5).trimmingCharacters(in: .whitespaces)
            guard let data = payload.data(using: .utf8),
                  let obj = try? JSONSerialization.jsonObject(with: data) as? JSONDictionary else { continue }
            if obj.string(at: "kind", fallback: "") == "heartbeat" { continue }
            index += 1
            continuation.yield(Self.streamEvent(from: obj, index: index))
          }
        } catch {
          // stream closed or errored; fall through to finish
        }
        continuation.finish()
      }
      continuation.onTermination = { _ in task.cancel() }
    }
  }

  static func forgeRuns(from json: JSONDictionary) -> [ForgeRun] {
    json.array(at: "runs").map { run in
      let diff = run.dictionary(at: "diff")
      let repoObject = run.dictionary(at: "repo")
      let modelObject = run.dictionary(at: "model_or_worker")
      let runnerObject = run.dictionary(at: "runner_profile")
      let events = run.array(at: "events")
      let status = run.string(at: "status", fallback: "unknown")
      let geometry = run.dictionary(at: "execution_geometry")
      let parallel = run.dictionary(at: "parallel_candidate")
      let workClassification = run.dictionary(at: "work_classification")
      let languageTaskAlignment = run.dictionary(at: "language_task_alignment")
      let quarantine = run.dictionary(at: "quarantine")
      let league = run.dictionary(at: "league_context")
      let contextTelemetry = run.dictionary(at: "context_telemetry")
      let mappedEvents = events.enumerated().map { index, event in forgeEvent(from: event, index: index) }
      return ForgeRun(
        id: run.string(at: "run_id", fallback: ""),
        title: firstString(
          in: [run.value(at: "run_title"), run.value(at: "title"), run.value(at: "operator_run_summary"), run.value(at: "run_summary")],
          fallback: run.string(at: "intent", fallback: "Forge run")
        ),
        intent: run.string(at: "intent", fallback: ""),
        origin: run.string(at: "origin", fallback: ""),
        systemOrigin: run.string(at: "system_origin", fallback: ""),
        status: status,
        terminalState: run.string(
          at: "terminal_state",
          fallback: status == "running" ? "IN_PROGRESS" : "FINISHED"
        ),
        stage: firstString(in: [run.value(at: "operator_status"), run.value(at: "latest_event")], fallback: ""),
        repo: firstString(in: [repoObject.value(at: "repo_id"), repoObject.value(at: "id"), run.value(at: "repo")], fallback: ""),
        harness: firstString(
          in: [
            runnerObject.value(at: "display_name"),
            runnerObject.value(at: "runner_id"),
            runnerObject.value(at: "runner_kind"),
            run.value(at: "runner_profile")
          ],
          fallback: ""
        ),
        model: forgeRunDisplayModel(
          modelObject: modelObject,
          runnerObject: runnerObject,
          contextTelemetry: contextTelemetry,
          events: mappedEvents,
          run: run
        ),
        branch: run.string(at: "branch", fallback: ""),
        commitSha: diff.string(at: "commit_sha", fallback: ""),
        turns: run.int(at: "turns", fallback: 0),
        modelCalls: run.int(at: "model_calls", fallback: 0),
        toolCalls: run.int(at: "tool_calls", fallback: 0),
        checksPassed: run.int(at: "checks_passed", fallback: 0),
        checksFailed: run.int(at: "checks_failed", fallback: 0),
        finalLine: run.string(at: "final_line", fallback: ""),
        streamRoute: run.string(at: "stream_route", fallback: ""),
        diffReady: diff.bool(at: "ready", fallback: false),
        diffFileCount: diff.int(at: "file_count", fallback: 0),
        workerCount: max(1, geometry.int(at: "effective_workers", fallback: geometry.int(at: "requested_workers", fallback: run.int(at: "fleet_width", fallback: 1)))),
        runner: forgeRunner(from: runnerObject),
        execution: forgeExecution(from: geometry),
        parallelCandidate: forgeParallelCandidate(from: parallel),
        workClassification: forgeWorkClassification(from: workClassification),
        languageTaskAlignment: forgeLanguageTaskAlignment(from: languageTaskAlignment),
        quarantine: forgeQuarantine(from: quarantine, league: league),
        contextTelemetry: forgeContextTelemetry(from: contextTelemetry, latestInputTokens: run.int(at: "latest_input_tokens", fallback: 0)),
        stages: run.array(at: "stages").enumerated().map { index, stage in
          ForgeStage(
            id: stage.string(at: "key", fallback: "stage_\(index)"),
            label: stage.string(at: "label", fallback: "Stage"),
            state: stage.string(at: "state", fallback: "pending")
          )
        },
        events: mappedEvents,
        controls: run.array(at: "controls").map { control in
          ForgeControl(
            action: control.string(at: "action", fallback: ""),
            allowed: control.bool(at: "allowed", fallback: false),
            route: control.string(at: "route", fallback: "")
          )
        }
      )
    }
  }

  private static func forgeWorkClassification(from json: JSONDictionary) -> ForgeWorkClassification {
    ForgeWorkClassification(
      recommendedShape: json.string(at: "recommended_shape", fallback: ""),
      taskClass: json.string(at: "task_class", fallback: ""),
      complexityTier: json.string(at: "complexity_tier", fallback: ""),
      confidencePct: json.int(at: "confidence_pct", fallback: 0),
      rationale: json.string(at: "rationale", fallback: "")
    )
  }

  private static func forgeLanguageTaskAlignment(from json: JSONDictionary) -> ForgeLanguageTaskAlignment {
    ForgeLanguageTaskAlignment(
      taskCorpusID: json.string(at: "task_corpus_id", fallback: ""),
      status: json.string(at: "status", fallback: ""),
      taskClass: json.string(at: "task_class", fallback: ""),
      complexityTier: json.string(at: "complexity_tier", fallback: ""),
      visibleCheck: json.string(at: "visible_check", fallback: ""),
      hiddenCheckSlot: json.string(at: "hidden_check_slot", fallback: "")
    )
  }

  static func parallelExecutionGroups(from json: JSONDictionary) -> [ForgeParallelExecutionGroup] {
    json.array(at: "parallel_execution_groups").map { group in
      let candidates = group.array(at: "candidates").map { candidate in
        ForgeParallelExecutionCandidate(
          runID: candidate.string(at: "run_id", fallback: ""),
          role: candidate.string(at: "role", fallback: ""),
          index: max(1, candidate.int(at: "index", fallback: 1)),
          count: max(1, candidate.int(at: "count", fallback: 1)),
          status: candidate.string(at: "status", fallback: ""),
          finished: candidate.bool(at: "finished", fallback: false),
          strategySummary: candidate.string(at: "strategy_summary", fallback: ""),
          checksPassed: candidate.int(at: "checks_passed", fallback: 0),
          checksFailed: candidate.int(at: "checks_failed", fallback: 0),
          turns: candidate.int(at: "turns", fallback: 0),
          branch: candidate.string(at: "branch", fallback: "")
        )
      }
      return ForgeParallelExecutionGroup(
        primaryRunID: group.string(at: "primary_run_id", fallback: ""),
        requestedWorkers: max(1, group.int(at: "requested_workers", fallback: 1)),
        effectiveWorkers: max(1, group.int(at: "effective_workers", fallback: 1)),
        lane: group.string(at: "lane", fallback: ""),
        plannedCandidateCount: group.int(at: "planned_candidate_count", fallback: candidates.count),
        observedCandidateCount: group.int(at: "observed_candidate_count", fallback: candidates.count),
        finishedCandidateCount: group.int(at: "finished_candidate_count", fallback: candidates.filter(\.finished).count),
        runningCandidateCount: group.int(at: "running_candidate_count", fallback: candidates.filter { $0.status == "running" }.count),
        doneCandidateCount: group.int(at: "done_candidate_count", fallback: 0),
        noChangeCandidateCount: group.int(at: "no_change_candidate_count", fallback: 0),
        cannotProceedCandidateCount: group.int(at: "cannot_proceed_candidate_count", fallback: 0),
        failedCandidateCount: group.int(at: "failed_candidate_count", fallback: 0),
        checksPassedTotal: group.int(at: "checks_passed_total", fallback: candidates.map(\.checksPassed).reduce(0, +)),
        checksFailedTotal: group.int(at: "checks_failed_total", fallback: candidates.map(\.checksFailed).reduce(0, +)),
        selectionStatus: group.string(at: "selection_status", fallback: ""),
        recommendedRunID: group.string(at: "recommended_run_id", fallback: ""),
        recommendedStrategySummary: group.string(at: "recommended_strategy_summary", fallback: ""),
        candidates: candidates
      )
    }
  }

  private static func forgeRunDisplayModel(
    modelObject: JSONDictionary,
    runnerObject: JSONDictionary,
    contextTelemetry: JSONDictionary,
    events: [ForgeEvent],
    run: JSONDictionary
  ) -> String {
    let configuredModel = firstDisplayModel(
      in: [
        modelObject.value(at: "model_id"),
        modelObject.value(at: "worker_id"),
        run.value(at: "model_id"),
        run.value(at: "worker_id")
      ]
    )
    let observedModels = uniqueStrings(
      observedContextModels(from: contextTelemetry) + events.map(\.model)
    )
    guard let primary = observedModels.first, !primary.isEmpty else {
      return configuredModel
    }
    if observedModels.count > 1 {
      return "\(primary) +\(observedModels.count - 1)"
    }
    return primary
  }

  private static func firstDisplayModel(in candidates: [Any?]) -> String {
    for candidate in candidates {
      guard let value = candidate as? String,
            let display = displayableModelID(value)
      else { continue }
      return display
    }
    return ""
  }

  private static func displayableModelID(_ value: String) -> String? {
    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return nil }
    let normalized = trimmed.lowercased()
    if normalized == "unknown" { return nil }
    if normalized.contains("local_model_profile") { return nil }
    if normalized.hasSuffix("_model_profile") || normalized.hasSuffix("-model-profile") { return nil }
    if normalized.hasSuffix("_responses_profile") || normalized.hasSuffix("-responses-profile") { return nil }
    return trimmed
  }

  private static func observedContextModels(from json: JSONDictionary) -> [String] {
    guard !json.isEmpty else { return [] }
    var models: [String] = []
    models.append(json.dictionary(at: "peak").string(at: "model_id", fallback: ""))
    models.append(json.dictionary(at: "latest").string(at: "model_id", fallback: ""))
    for model in json.array(at: "models") {
      models.append(model.string(at: "model_id", fallback: ""))
    }
    return models
  }

  private static func uniqueStrings(_ values: [String]) -> [String] {
    var seen = Set<String>()
    var output: [String] = []
    for value in values {
      let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
      guard !trimmed.isEmpty, !seen.contains(trimmed) else { continue }
      seen.insert(trimmed)
      output.append(trimmed)
    }
    return output
  }

  static func forgeEvent(from event: JSONDictionary, index: Int) -> ForgeEvent {
    ForgeEvent(
      id: event.string(at: "id", fallback: "event_\(index)"),
      seq: event.int(at: "seq", fallback: event.int(at: "turn", fallback: index)),
      kind: firstString(in: [event.value(at: "kind"), event.value(at: "event")], fallback: "event"),
      stage: event.string(at: "stage", fallback: ""),
      summary: firstString(in: [event.value(at: "summary"), event.value(at: "detail"), event.value(at: "event")], fallback: "Activity"),
      detail: event.string(at: "detail", fallback: ""),
      receiptID: event.string(at: "receipt_id", fallback: ""),
      receiptRoute: event.string(at: "receipt_route", fallback: ""),
      model: eventModel(from: event)
    )
  }

  static func streamEvent(from obj: JSONDictionary, index: Int) -> ForgeEvent {
    let payload = obj.dictionary(at: "payload")
    return ForgeEvent(
      id: "live_\(obj.int(at: "seq", fallback: index))_\(index)",
      seq: obj.int(at: "seq", fallback: index),
      kind: obj.string(at: "kind", fallback: "event"),
      stage: obj.string(at: "stage", fallback: ""),
      summary: firstString(in: [payload.value(at: "summary"), payload.value(at: "text"), payload.value(at: "detail"), payload.value(at: "name")], fallback: obj.string(at: "kind", fallback: "Activity")),
      detail: payload.string(at: "detail", fallback: ""),
      receiptID: obj.string(at: "receipt_id", fallback: ""),
      receiptRoute: obj.string(at: "receipt_route", fallback: ""),
      model: obj.dictionary(at: "model").string(at: "model_id", fallback: "")
    )
  }

  private static func forgeRunner(from json: JSONDictionary) -> ForgeRunnerProfile {
    guard !json.isEmpty else { return .empty }
    return ForgeRunnerProfile(
      id: json.string(at: "runner_id", fallback: ""),
      kind: json.string(at: "runner_kind", fallback: ""),
      displayName: json.string(at: "display_name", fallback: ""),
      adapterKind: json.string(at: "adapter_kind", fallback: ""),
      executionMode: json.string(at: "execution_mode", fallback: ""),
      modelProfileID: json.string(at: "model_profile_id", fallback: "")
    )
  }

  private static func forgeExecution(from json: JSONDictionary) -> ForgeExecutionGeometry {
    guard !json.isEmpty else { return .single }
    return ForgeExecutionGeometry(
      requestedWorkers: max(1, json.int(at: "requested_workers", fallback: 1)),
      effectiveWorkers: max(1, json.int(at: "effective_workers", fallback: 1)),
      lane: json.string(at: "lane", fallback: ""),
      route: json.string(at: "route", fallback: ""),
      reason: json.string(at: "reason", fallback: ""),
      fleetRequired: json.bool(at: "fleet_required", fallback: false)
    )
  }

  private static func forgeParallelCandidate(from json: JSONDictionary) -> ForgeParallelCandidate {
    guard !json.isEmpty else { return .single }
    return ForgeParallelCandidate(
      role: json.string(at: "role", fallback: ""),
      primaryRunID: json.string(at: "primary_run_id", fallback: ""),
      index: max(1, json.int(at: "index", fallback: 1)),
      count: max(1, json.int(at: "count", fallback: 1)),
      writeScope: stringArray(json.value(at: "write_scope")),
      strategyID: json.string(at: "strategy_id", fallback: ""),
      strategySummary: json.string(at: "strategy_summary", fallback: ""),
      proofBias: json.string(at: "proof_bias", fallback: "")
    )
  }

  private static func forgeQuarantine(from json: JSONDictionary, league: JSONDictionary) -> ForgeQuarantine {
    guard !json.isEmpty || !league.isEmpty else { return .empty }
    return ForgeQuarantine(
      status: json.string(at: "status", fallback: ""),
      outputHeld: json.bool(at: "output_quarantined", fallback: false),
      outputConsumable: json.bool(at: "external_output_consumable", fallback: false),
      acceptanceGate: json.string(at: "acceptance_gate", fallback: ""),
      blockedReason: json.string(at: "blocked_reason", fallback: ""),
      acceptedForScoreboard: json.bool(at: "accepted_for_scoreboard", fallback: false),
      leagueRecommendation: league.string(at: "recommendation_rationale", fallback: ""),
      quarantinePosture: league.string(at: "quarantine_posture", fallback: "")
    )
  }

  private static func forgeContextTelemetry(from json: JSONDictionary, latestInputTokens: Int) -> ForgeContextTelemetry {
    guard !json.isEmpty || latestInputTokens > 0 else { return .empty }
    let latest = json.dictionary(at: "latest")
    let peak = json.dictionary(at: "peak")
    let total = json.dictionary(at: "total")
    return ForgeContextTelemetry(
      latestInputTokens: latest.int(at: "input_tokens", fallback: latestInputTokens),
      latestOutputTokens: latest.int(at: "output_tokens", fallback: 0),
      peakInputTokens: peak.int(at: "input_tokens", fallback: latestInputTokens),
      contextWindow: peak.int(at: "context_window", fallback: latest.int(at: "context_window", fallback: 0)),
      peakPct: peak.int(at: "pct", fallback: latest.int(at: "pct", fallback: 0)),
      modelCount: total.int(at: "model_count", fallback: 0)
    )
  }

  private static func eventModel(from event: JSONDictionary) -> String {
    if let model = event.value(at: "model.model_id") as? String, !model.isEmpty {
      return model
    }
    let detail = event.string(at: "detail", fallback: "")
    for part in detail.split(separator: " ") {
      if part.hasPrefix("model=") {
        return String(part.dropFirst("model=".count))
      }
    }
    return ""
  }

  static func forgeRepos(from json: JSONDictionary) -> [ForgeRepo] {
    json.array(at: "repos").map { repo in
      let profile = repo.dictionary(at: "profile")
      let proofPlan = profile.dictionary(at: "proof_plan")
      return ForgeRepo(
        id: repo.string(at: "repo_id", fallback: ""),
        label: firstString(in: [repo.value(at: "label"), repo.value(at: "repo_id")], fallback: "Repo"),
        kind: repo.string(at: "kind", fallback: "local"),
        originURL: repo.string(at: "origin_url", fallback: ""),
        primaryLanguage: profile.string(at: "primary_language", fallback: ""),
        suggestedCheckCommands: stringList(profile.value(at: "suggested_checks")),
        proofPlanStatus: proofPlan.string(at: "status", fallback: ""),
        proofPlanNextAction: proofPlan.string(at: "next_action", fallback: ""),
        root: repo.string(at: "root", fallback: "")
      )
    }
    .filter { !$0.id.isEmpty }
  }

  static func forgeMissions(from json: JSONDictionary) -> [ForgeMission] {
    json.array(at: "missions").map { mission in
      ForgeMission(
        id: mission.string(at: "mission_id", fallback: ""),
        goal: mission.string(at: "goal", fallback: ""),
        nonGoals: mission.string(at: "non_goals", fallback: ""),
        constraints: mission.string(at: "constraints", fallback: ""),
        doneWhen: mission.string(at: "done_when", fallback: ""),
        allowedWriteScope: mission.string(at: "allowed_write_scope", fallback: ""),
        blockedPaths: mission.string(at: "blocked_paths", fallback: ""),
        validationCommands: mission.string(at: "validation_commands", fallback: ""),
        fleetWidth: mission.int(at: "fleet_width", fallback: 1),
        maxCostCents: mission.int(at: "max_cost_cents", fallback: 0),
        checkpointCadenceMinutes: mission.int(at: "checkpoint_cadence_minutes", fallback: 0),
        state: mission.string(at: "mission_state", fallback: ""),
        latestCheckpointSummary: mission.string(at: "latest_checkpoint_summary", fallback: ""),
        checkpointCount: mission.int(at: "checkpoint_count", fallback: 0),
        completedMilestoneCount: mission.int(at: "completed_milestone_count", fallback: 0),
        blockedMilestoneCount: mission.int(at: "blocked_milestone_count", fallback: 0),
        steeringNoteCount: mission.int(at: "steering_note_count", fallback: 0),
        relatedRunIDs: stringList(mission.value(at: "related_run_ids")),
        milestones: mission.array(at: "milestones").map { milestone in
          MissionMilestone(
            milestoneID: milestone.string(at: "milestone_id", fallback: ""),
            title: milestone.string(at: "title", fallback: "Milestone"),
            validationCommand: milestone.string(at: "validation_command", fallback: ""),
            status: milestone.string(at: "status", fallback: ""),
            acceptanceCriteria: milestone.string(at: "acceptance_criteria", fallback: ""),
            checkpointReceiptID: milestone.string(at: "checkpoint_receipt_id", fallback: ""),
            validationStatus: milestone.string(at: "validation_status", fallback: ""),
            relatedRunID: milestone.string(at: "related_run_id", fallback: "")
          )
        }
      )
    }
    .filter { !$0.id.isEmpty }
  }

  static func scoutResult(from json: JSONDictionary) -> ScoutResult {
    ScoutResult(
      repoID: json.string(at: "repo_id", fallback: ""),
      status: json.string(at: "status", fallback: ""),
      safeNextMove: json.string(at: "safe_next_move", fallback: ""),
      filesScanned: json.int(at: "files_scanned", fallback: 0),
      primaryLanguage: json.string(at: "primary_language", fallback: ""),
      candidates: json.array(at: "candidates").map { candidate in
        let path = candidate.string(at: "path", fallback: "")
        let line = candidate.int(at: "line", fallback: 0)
        return ScoutCandidate(
          taskID: candidate.string(at: "task_id", fallback: path.isEmpty ? "task" : "\(path):\(line)"),
          title: candidate.string(at: "title", fallback: "Suggested task"),
          why: candidate.string(at: "why_this_first", fallback: ""),
          path: path,
          line: line,
          complexity: candidate.string(at: "complexity_tier", fallback: ""),
          taskClass: candidate.string(at: "task_class", fallback: ""),
          recommendedShape: candidate.string(at: "recommended_shape", fallback: "run"),
          promptTemplate: candidate.string(at: "prompt_template", fallback: "")
        )
      }
    )
  }

  static func reviewPacket(from json: JSONDictionary) -> ReviewPacket {
    let checks = json.dictionary(at: "checks")
    let behavior = json.dictionary(at: "behavior_proof")
    let coverage = json.dictionary(at: "proof_coverage")
    let principal = json.dictionary(at: "principal_review")
    let ship = json.dictionary(at: "ship")
    return ReviewPacket(
      runID: json.string(at: "run_id", fallback: ""),
      reviewStatus: firstString(in: [json.value(at: "review_status"), json.value(at: "status")], fallback: ""),
      nextMove: json.string(at: "next_move", fallback: ""),
      checksPassed: countOf(checks.value(at: "passed"), fallback: 0),
      checksFailed: countOf(checks.value(at: "failed"), fallback: 0),
      checkNames: stringList(checks.value(at: "names")),
      behaviorStatus: behavior.string(at: "status", fallback: ""),
      behaviorSummary: behavior.string(at: "summary", fallback: ""),
      satisfiedChecks: stringList(coverage.value(at: "satisfied_checks")),
      missingChecks: stringList(coverage.value(at: "missing_checks")),
      principalChecklist: stringList(principal.value(at: "checklist")),
      handoffLines: stringList(principal.value(at: "handoff_lines")),
      shipDecided: ship.bool(at: "decided", fallback: false),
      shipReason: ship.string(at: "reason", fallback: "")
    )
  }

  static func stringList(_ value: Any?) -> [String] {
    if let arr = value as? [String] { return arr }
    if let arr = value as? [Any] { return arr.map { "\($0)" } }
    if let string = value as? String, !string.isEmpty { return [string] }
    return []
  }

  static func countOf(_ value: Any?, fallback: Int) -> Int {
    if let number = value as? Int { return number }
    if let number = value as? NSNumber { return number.intValue }
    if let arr = value as? [Any] { return arr.count }
    return fallback
  }

  static func runOutcome(from json: JSONDictionary, title: String, idKeys: [String]) -> RunActionOutcome {
    let status = json.string(at: "status", fallback: "")
    let receipt = firstString(in: idKeys.map { json.value(at: $0) }, fallback: "")
    let detail = firstString(
      in: [json.value(at: "reason"), json.value(at: "next_safe_action"), json.value(at: "safe_next_move"), json.value(at: "terminal_state")],
      fallback: summary(from: json)
    )
    return RunActionOutcome(title: title, status: requireReceiptStatus(status, receiptID: receipt), detail: detail, receiptID: receipt)
  }

  static func requireReceiptStatus(_ status: String, receiptID: String) -> String {
    let normalized = status.trimmingCharacters(in: .whitespacesAndNewlines)
    if normalized.isEmpty { return "FAILED_MISSING_STATUS" }
    if normalized.localizedCaseInsensitiveContains("REFUSED")
      || normalized.localizedCaseInsensitiveContains("DENIED")
      || normalized.localizedCaseInsensitiveContains("FAILED") {
      return normalized
    }
    return receiptID.isEmpty ? "FAILED_MISSING_RECEIPT" : normalized
  }

  private func loadRoute(
    baseURL: URL,
    title: String,
    path: String,
    lane: RouteLane,
    receiptBacked: Bool,
    timeout: TimeInterval = 8
  ) async -> LoadedRoute {
    do {
      let json = try await fetchJSON(baseURL: baseURL, path: path, timeout: timeout)
      let summary = Self.summary(from: json)
      return LoadedRoute(
        card: RouteCard(
          id: path,
          title: title,
          path: path,
          status: .ok,
          detail: summary.isEmpty ? Self.curatedRouteDetail(for: path) : summary,
          lane: lane,
          receiptBacked: receiptBacked,
          readOnly: json.readOnlyValue() ?? true,
          metric: Self.metric(from: json)
        ),
        json: json
      )
    } catch {
      return LoadedRoute(
        card: RouteCard(
          id: path,
          title: title,
          path: path,
          status: .unavailable,
          detail: error.localizedDescription,
          lane: lane,
          receiptBacked: receiptBacked,
          readOnly: true,
          metric: "unavailable"
        ),
        json: [:]
      )
    }
  }

  private func fetchJSON(baseURL: URL, path: String, timeout: TimeInterval = 8) async throws -> JSONDictionary {
    try await Telemetry.measure("GET \(path)", kind: .route) {
      let url = Self.routeURL(baseURL: baseURL, path: path)
      var request = URLRequest(url: url)
      request.timeoutInterval = timeout
      request.setValue("application/json", forHTTPHeaderField: "Accept")
      request = await authorize(request)

      let (data, response) = try await session.data(for: request)
      guard let httpResponse = response as? HTTPURLResponse else {
        throw RouteClientError.invalidResponse
      }
      guard (200..<300).contains(httpResponse.statusCode) else {
        throw RouteClientError.httpStatus(httpResponse.statusCode)
      }
      guard let object = try JSONSerialization.jsonObject(with: data) as? JSONDictionary else {
        throw RouteClientError.invalidJSON
      }
      return object
    }
  }

  private func postJSON(baseURL: URL, path: String, body: [String: Any], timeout: TimeInterval = 8) async throws -> JSONDictionary {
    try await Telemetry.measure("POST \(path)", kind: .action) {
      let url = Self.routeURL(baseURL: baseURL, path: path)
      var request = URLRequest(url: url)
      request.httpMethod = "POST"
      request.timeoutInterval = timeout
      request.setValue("application/json", forHTTPHeaderField: "Accept")
      request.setValue("application/json", forHTTPHeaderField: "Content-Type")
      request.httpBody = try JSONSerialization.data(
        withJSONObject: Self.governedRequestBody(body, baseURL: baseURL)
      )
      request = await authorize(request)
      try Self.requireTrustedWriteHost(
        baseURL,
        path: path,
        authenticated: request.value(forHTTPHeaderField: "Authorization") != nil
      )

      let (data, response) = try await session.data(for: request)
      guard let httpResponse = response as? HTTPURLResponse else {
        throw RouteClientError.invalidResponse
      }
      guard (200..<300).contains(httpResponse.statusCode) else {
        throw RouteClientError.httpStatus(httpResponse.statusCode)
      }
      guard let object = try JSONSerialization.jsonObject(with: data) as? JSONDictionary else {
        throw RouteClientError.invalidJSON
      }
      return object
    }
  }

  /// Resolve a kernel route without treating its query string as a literal
  /// path segment. `URL.appending(path:)` escapes `?limit=...` to `%3F...`,
  /// which makes every paginated route look like a 404 to the native app.
  static func routeURL(baseURL: URL, path: String) -> URL {
    guard var base = URLComponents(url: baseURL, resolvingAgainstBaseURL: false),
          let route = URLComponents(string: path) else {
      return baseURL.appending(path: path)
    }
    let prefix = base.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    let suffix = route.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    base.path = "/" + [prefix, suffix].filter { !$0.isEmpty }.joined(separator: "/")
    base.queryItems = route.queryItems
    base.fragment = route.fragment
    return base.url ?? baseURL.appending(path: path)
  }

  private static func summary(from json: JSONDictionary) -> String {
    for path in [
      "forge_runtime.current_signal",
      "workspace.intent",
      "briefing.operator_message",
      "operator_boundary",
      "authority_boundary",
      "backpressure_policy",
      "status",
      "mode",
      "name"
    ] {
      let value = json.string(at: path, fallback: "")
      if !value.isEmpty {
        // A status or mode enum is not a description; drop it so the card does
        // not echo a Title-Cased enum as its subtitle. The caller supplies a
        // curated sentence for the routes that need one.
        if path == "status" || path == "mode" { return "" }
        return value
      }
    }
    return ""
  }

  /// A real sentence for routes whose projection carries only a status enum, so
  /// the card reads instead of echoing the enum. Empty means show no subtitle;
  /// the route line already carries the exact source.
  private static func curatedRouteDetail(for path: String) -> String {
    switch path {
    case "/forge/machine-league/recommendations.json":
      return "The league's current pick and the evidence it is gathering."
    case "/forge/fleet-eval-scoreboard.json":
      return "Results of held machine comparisons."
    case "/forge/machine-league/closed-runner-clearances/projection.json":
      return "Which machines are signed in and allowed here."
    default:
      return ""
    }
  }

  private static func metric(from json: JSONDictionary) -> String {
    for path in [
      "run_count",
      "repo_count",
      "recipe_count",
      "mission_count",
      "fleet_count",
      "fleet_run_count",
      "arbitration_count",
      "ship_decision_count",
      "request_count",
      "approval_count",
      "plan_proof_count",
      "authority_request_count",
      "talent_authorization_count",
      "ci_preflight_count",
      "ratification_preflight_count",
      "deployment_preflight_count",
      "provider_count",
      "runner_count",
      "queue_depth",
      "operator_signal.receipt_count",
      "operator_context_bundle.route_count"
    ] {
      if let value = json.int(at: path) {
        return "\(value)"
      }
    }
    // No count to show. The humanized status already reads in the card detail,
    // so keep the metric quiet rather than repeating a long status here.
    let status = json.string(at: "status", fallback: "OK")
    return status == "OK" ? "OK" : ""
  }

  private static func workbenchItems(
    reposJSON: JSONDictionary,
    recipesJSON: JSONDictionary,
    builderLoopsJSON: JSONDictionary,
    missionsJSON: JSONDictionary
  ) -> [SurfaceItem] {
    var items = [
      SurfaceItem(
        id: "repos",
        title: "Connected repositories",
        subtitle: "\(reposJSON.int(at: "repo_count", fallback: reposJSON.array(at: "repos").count)) repos ready for Forge",
        status: reposJSON.bool(at: "production_write_allowed", fallback: false) ? "Writes open" : "Writes locked",
        detail: "Connect and index repos before asking Forge to change code.",
        route: "/forge/repos/projection.json"
      ),
      SurfaceItem(
        id: "recipes",
        title: "Work recipes",
        subtitle: "\(recipesJSON.int(at: "recipe_count", fallback: recipesJSON.array(at: "recipes").count)) reusable jobs",
        status: "Ready",
        detail: firstRecipeSummary(from: recipesJSON),
        route: "/forge/recipes.json"
      ),
      SurfaceItem(
        id: "builder-loops",
        title: "Builder loops",
        subtitle: builderLoopsJSON.string(at: "status", fallback: "Loop state unavailable"),
        status: builderLoopsJSON.bool(at: "launch_execution_allowed", fallback: false) ? "Can launch" : "Launch locked",
        detail: builderLoopsJSON.bool(at: "checker_gate_required", fallback: true) ? "Checker proof is required before a loop can accept itself." : "Checker proof is not required by this local route.",
        route: "/forge/builder-loops.json"
      ),
      SurfaceItem(
        id: "missions",
        title: "Long-horizon missions",
        subtitle: "\(missionsJSON.int(at: "active_mission_count", fallback: 0)) active of \(missionsJSON.int(at: "mission_count", fallback: missionsJSON.array(at: "missions").count))",
        status: missionsJSON.bool(at: "adapter_execution_allowed", fallback: false) ? "Adapter open" : "Adapter locked",
        detail: "Missions keep larger work visible without pretending the operator can skip proof.",
        route: "/forge/long-horizon-missions/projection.json"
      )
    ]

    items.append(contentsOf: recipesJSON.array(at: "recipes").prefix(4).map { recipe in
      SurfaceItem(
        id: "recipe-\(recipe.string(at: "id", fallback: UUID().uuidString))",
        title: recipe.string(at: "title", fallback: "Forge recipe"),
        subtitle: recipe.string(at: "summary", fallback: "Reusable Forge work"),
        status: recipe.string(at: "cadence", fallback: recipe.string(at: "category", fallback: "Ready")),
        detail: recipe.string(at: "intent", fallback: "Review the recipe before requesting work."),
        route: "/forge/recipes.json"
      )
    })

    return items
  }

  private static func workItems(from json: JSONDictionary) -> [WorkItem] {
    json.array(at: "runs").prefix(8).enumerated().map { index, item in
      let id = item.string(at: "run_id", fallback: item.string(at: "id", fallback: "run-\(index)"))
      return WorkItem(
        id: id,
        title: item.string(at: "intent", fallback: id),
        subtitle: item.string(at: "summary", fallback: item.string(at: "source_surface", fallback: "Forge run")),
        status: item.string(at: "status", fallback: "UNKNOWN"),
        stage: item.string(at: "current_stage", fallback: item.string(at: "stage", fallback: "unknown"))
      )
    }
  }

  private static func runStages(from json: JSONDictionary) -> [RunStageItem] {
    let firstRun = json.array(at: "runs").first
    let stage = firstRun?.string(at: "current_stage", fallback: firstRun?.string(at: "stage", fallback: "") ?? "") ?? ""
    let status = firstRun?.string(at: "status", fallback: json.string(at: "status", fallback: "Waiting")) ?? "Waiting"
    let localReal = json.dictionary(at: "local_real")
    return [
      RunStageItem(id: "intent", title: "Intent", status: json.int(at: "run_count", fallback: 0) > 0 ? "Present" : "Waiting", detail: firstRun?.string(at: "intent", fallback: "No run intent has been admitted yet.") ?? "No run intent has been admitted yet."),
      RunStageItem(id: "plan", title: "Plan", status: stage.isEmpty ? "Waiting" : stage, detail: "Forge keeps the plan visible before execution claims."),
      RunStageItem(id: "worktree", title: "Workspace", status: localReal.bool(at: "worktree_isolated", fallback: false) ? "Isolated" : "Held", detail: localReal.bool(at: "live_repo_mutated", fallback: false) ? "Live repo mutation was reported." : "Live repo mutation is not reported."),
      RunStageItem(id: "model", title: "Model", status: localReal.bool(at: "model_configured", fallback: false) ? "Connected" : "Needs setup", detail: "A model must be configured before Forge can run."),
      RunStageItem(id: "checks", title: "Checks", status: localReal.bool(at: "host_checks_enabled", fallback: false) ? "Enabled" : "Held", detail: localReal.bool(at: "sandbox_tests_enabled", fallback: false) ? "Sandbox tests are enabled." : "Sandbox tests remain held."),
      RunStageItem(id: "review", title: "Review", status: status, detail: "Review, handoff, and ship readiness stay human-governed.")
    ]
  }

  private static func reviewArtifacts(
    shipDecisionsJSON: JSONDictionary,
    ciPreflightJSON: JSONDictionary,
    humanPreflightJSON: JSONDictionary,
    deploymentPreflightJSON: JSONDictionary
  ) -> [ReviewArtifact] {
    [
      ReviewArtifact(id: "ship", title: "Ship call", status: "\(shipDecisionsJSON.int(at: "ship_decision_count", fallback: 0)) recorded", detail: shipDecisionsJSON.bool(at: "deployment_allowed", fallback: false) ? "Deployment authority is open." : "Deployment remains closed.", route: "/forge/ship-decisions/projection.json"),
      ReviewArtifact(id: "ci", title: "CI proof", status: ciPreflightJSON.bool(at: "ci_claim_allowed", fallback: false) ? "Claim allowed" : "Claim locked", detail: ciPreflightJSON.bool(at: "ci_verification_observed", fallback: false) ? "CI verification was observed." : "CI verification still needs proof.", route: "/forge/ci-evidence-preflights/projection.json"),
      ReviewArtifact(id: "human", title: "Human signoff", status: humanPreflightJSON.bool(at: "ship_authority_allowed", fallback: false) ? "Authority open" : "Authority locked", detail: "\(humanPreflightJSON.int(at: "ratification_preflight_count", fallback: 0)) signoff preflights recorded.", route: "/forge/human-ratification-preflights/projection.json"),
      ReviewArtifact(id: "deployment", title: "Deployment proof", status: deploymentPreflightJSON.bool(at: "deployment_authority_granted", fallback: false) ? "Authority open" : "Authority locked", detail: "\(deploymentPreflightJSON.int(at: "deployment_preflight_count", fallback: 0)) deployment preflights recorded.", route: "/forge/deployment-preflights/projection.json")
    ]
  }

  private static func runRoomItems(from json: JSONDictionary) -> [SurfaceItem] {
    let runs = json.array(at: "runs").prefix(12).enumerated().map { index, item in
      let id = item.string(at: "run_id", fallback: item.string(at: "id", fallback: "run-\(index)"))
      return SurfaceItem(
        id: id,
        title: item.string(at: "intent", fallback: id),
        subtitle: item.string(at: "summary", fallback: item.string(at: "source_surface", fallback: "Forge run")),
        status: item.string(at: "status", fallback: "UNKNOWN"),
        detail: item.string(at: "current_stage", fallback: item.string(at: "stage", fallback: "No stage reported")),
        route: "/forge/runs/projection.json"
      )
    }

    if !runs.isEmpty {
      return runs
    }

    return [
      SurfaceItem(
        id: "run-empty",
        title: "No run is active",
        subtitle: "Forge can show trails here as soon as a governed build request exists.",
        status: "Waiting",
        detail: "Requesting work stays locked until the local authority chain opens the write route.",
        route: "/forge/runs/projection.json"
      )
    ]
  }

  private static func reviewItems(
    shipDecisionsJSON: JSONDictionary,
    ciPreflightJSON: JSONDictionary,
    humanPreflightJSON: JSONDictionary,
    deploymentPreflightJSON: JSONDictionary
  ) -> [SurfaceItem] {
    var items = shipDecisionsJSON.array(at: "decisions").prefix(8).enumerated().map { index, item in
      SurfaceItem(
        id: item.string(at: "id", fallback: "ship-\(index)"),
        title: item.string(at: "title", fallback: item.string(at: "run_id", fallback: "Ship decision")),
        subtitle: item.string(at: "summary", fallback: "A human ship call is waiting."),
        status: item.string(at: "status", fallback: "Needs review"),
        detail: item.string(at: "decision", fallback: "Shipping stays a recorded human call."),
        route: "/forge/ship-decisions/projection.json"
      )
    }

    items.append(contentsOf: [
      preflightItem(id: "ci", title: "CI proof", json: ciPreflightJSON, countPath: "ci_preflight_count", allowedPath: "ci_claim_allowed", detail: "CI evidence must be shaped and observed before a claim can move forward.", route: "/forge/ci-evidence-preflights/projection.json"),
      preflightItem(id: "human-ratification", title: "Human signoff", json: humanPreflightJSON, countPath: "ratification_preflight_count", allowedPath: "ship_authority_allowed", detail: "The product records the human call instead of inferring it from green checks.", route: "/forge/human-ratification-preflights/projection.json"),
      preflightItem(id: "deployment", title: "Deployment proof", json: deploymentPreflightJSON, countPath: "deployment_preflight_count", allowedPath: "deployment_authority_granted", detail: "Deployment waits for explicit authority and a valid deploy shape.", route: "/forge/deployment-preflights/projection.json"),
      SurfaceItem(id: "source-host-readiness", title: "Source-host readiness", subtitle: "Ready only after credentials, branch, review packet, and delivery guardrails line up.", status: "Locked", detail: "This is a governed write route. Native shows the door and the key, but does not deliver code without authority.", route: "/forge/source-host-readiness.json"),
      SurfaceItem(id: "pr-handoff", title: "PR handoff", subtitle: "Drafting or opening a pull request remains a recorded human delivery call.", status: "Locked", detail: "The route exists for handoff, but native keeps it closed until remote push, PR open, approval, deployment, and production-write gates say yes.", route: "/forge/pr-handoffs.json")
    ])

    return items
  }

  private static func fleetItems(
    fleetPlansJSON: JSONDictionary,
    fleetJSON: JSONDictionary,
    arbitrationJSON: JSONDictionary,
    capacityJSON: JSONDictionary
  ) -> [SurfaceItem] {
    let fleetRunSummary = fleetRunSummary(from: fleetJSON)
    return [
      SurfaceItem(id: "fleet-plans", title: "Fleet plans", subtitle: "\(fleetPlansJSON.int(at: "fleet_count", fallback: fleetPlansJSON.array(at: "fleets").count)) drafted", status: fleetPlansJSON.bool(at: "production_write_allowed", fallback: false) ? "Writes open" : "Writes locked", detail: "Plans describe how work is split across workers before any of them start.", route: "/forge/fleet-plans/projection.json"),
      SurfaceItem(id: "fleet-runs", title: "Fleet runs", subtitle: fleetRunSummary.subtitle, status: fleetRunSummary.status, detail: fleetRunSummary.detail, route: "/forge/fleet-runs/projection.json"),
      SurfaceItem(id: "arbitration", title: "Arbitration", subtitle: "\(arbitrationJSON.int(at: "arbitration_count", fallback: arbitrationJSON.array(at: "arbitrations").count)) calls", status: arbitrationJSON.isEmpty ? "Unavailable" : "Ready", detail: "Close calls stay visible for review before a winner is trusted.", route: "/forge/machine-league/fleet-runs/arbitrations/projection.json"),
      SurfaceItem(id: "capacity", title: "Capacity", subtitle: "\(capacityJSON.int(at: "active_now", fallback: 0)) active, \(capacityJSON.int(at: "queue_depth", fallback: 0)) queued", status: "\(capacityJSON.int(at: "worker_capacity", fallback: 0)) workers", detail: capacityJSON.string(at: "backpressure_policy", fallback: "Capacity route is available."), route: "/forge/fleet-capacity.json")
    ]
  }

  private static func fleetRunCount(from json: JSONDictionary) -> Int {
    json.int(
      at: "fleet_count",
      fallback: json.int(
        at: "fleet_run_count",
        fallback: max(json.array(at: "fleet_runs").count, json.array(at: "runs").count)
      )
    )
  }

  private static func startedFleetIDs(from json: JSONDictionary) -> [String] {
    json.array(at: "fleet_runs").compactMap { run in
      let id = run.string(at: "fleet_id", fallback: "")
      return id.isEmpty ? nil : id
    }
  }

  private static func fleetRunSummary(from json: JSONDictionary) -> (subtitle: String, status: String, detail: String) {
    let runs = json.array(at: "fleet_runs")
    let count = fleetRunCount(from: json)
    guard !json.isEmpty else {
      return ("0 fleet runs", "Unavailable", "Fleet run state is not available from the local route.")
    }
    guard count > 0 else {
      return ("0 fleet runs", "Ready", "Native keeps the fleet trail visible so engineers can compare work without changing context.")
    }
    let runningCount = runs.filter { $0.bool(at: "running", fallback: false) }.count
    let attentionCount = runs.reduce(0) { total, run in
      total
        + run.dictionary(at: "proof").int(at: "lane_attention_count", fallback: 0)
        + (run.string(at: "integration_state", fallback: "") == "did_not_land" ? 1 : 0)
    }
    let recoveryActive = runs.contains { run in
      run.dictionary(at: "proof").dictionary(at: "runtime_recovery").bool(at: "can_auto_continue", fallback: false)
    }
    let laneCount = runs.reduce(0) { total, run in
      total + run.dictionary(at: "proof").int(at: "stream_count", fallback: run.array(at: "lanes").count)
    }
    let subtitle = "\(count) fleet \(count == 1 ? "run" : "runs"), \(laneCount) lane\(laneCount == 1 ? "" : "s")"
    if attentionCount > 0 {
      let stillRecovering = recoveryActive && runningCount > 0
      return (
        subtitle,
        stillRecovering ? "Recovering" : "Needs attention",
        "\(attentionCount) lane\(attentionCount == 1 ? "" : "s") need attention. \(stillRecovering ? "Forge is using its bounded repair path." : "Review the lane evidence before starting more work.")"
      )
    }
    if runningCount > 0 {
      return (subtitle, "Working", "\(runningCount) fleet \(runningCount == 1 ? "is" : "are") running.")
    }
    return (subtitle, "Ready", "Fleet work is recorded and ready for review.")
  }

  static func forgeDecisions(from json: JSONDictionary) -> [ForgeDecision] {
    json.array(at: "needs_you").enumerated().map { index, item in
      let kind = item.string(at: "kind", fallback: "decision")
      let subject = item.string(at: "subject", fallback: "")
      let subjectRef = item.dictionary(at: "subject_ref")
      let action = item.dictionary(at: "primary_action")
      let href = item.string(at: "href", fallback: "/forge")
      let lane: ForgeLane
      if href.contains("/review") {
        lane = .review
      } else if href.contains("/fleets") {
        lane = .fleet
      } else if href.contains("/missions") {
        lane = .missions
      } else {
        lane = .overview
      }
      let projectedSubjectID = subjectRef.string(
        at: "id",
        fallback: subject.split(separator: "=").last.map(String.init) ?? ""
      )
      return ForgeDecision(
        id: item.string(at: "id", fallback: subject.isEmpty ? "\(kind)-\(index)" : "\(kind)-\(subject)"),
        kind: kind,
        title: item.string(at: "label", fallback: "Make the waiting decision"),
        detail: item.string(at: "line", fallback: "Forge is holding at a boundary only you can cross."),
        lane: lane,
        subjectID: projectedSubjectID.isEmpty ? nil : projectedSubjectID,
        priority: item.int(at: "priority", fallback: 999),
        actionState: action.string(at: "state", fallback: "review_required"),
        blockerCodes: stringList(action.value(at: "blocker_codes")),
        evidenceReceiptIDs: stringList(action.value(at: "evidence_receipt_ids"))
      )
    }
  }

  private static func controlItems(
    controlPlaneJSON: JSONDictionary,
    buildRequestsJSON: JSONDictionary,
    buildApprovalsJSON: JSONDictionary,
    planProofsJSON: JSONDictionary,
    workerAuthorityJSON: JSONDictionary,
    talentAuthorizationsJSON: JSONDictionary
  ) -> [SurfaceItem] {
    [
      SurfaceItem(id: "control-plane", title: "Control plane", subtitle: "\(controlPlaneJSON.int(at: "needs_you_count", fallback: 0)) items need you", status: controlPlaneJSON.bool(at: "authority_granted", fallback: false) ? "Authority open" : "Authority locked", detail: controlPlaneJSON.string(at: "operator_boundary", fallback: "Forge will not operate outside the local authority boundary."), route: "/forge/control-plane/projection.json"),
      gatedItem(id: "build-requests", title: "Request a build", json: buildRequestsJSON, countPath: "request_count", allowedPaths: ["worker_spawn_allowed", "shell_execution_allowed", "provider_call_allowed"], route: "/forge/build-requests/projection.json"),
      gatedItem(id: "build-approvals", title: "Approve a build", json: buildApprovalsJSON, countPath: "approval_count", allowedPaths: ["workflow_execution_allowed", "worker_spawn_allowed", "patch_application_allowed"], route: "/forge/build-approvals/projection.json"),
      gatedItem(id: "plan-proofs", title: "Prove the plan", json: planProofsJSON, countPath: "plan_proof_count", allowedPaths: ["workflow_execution_allowed", "ci_claim_allowed", "deployment_allowed"], route: "/forge/workflow-plan-proofs/projection.json"),
      gatedItem(id: "worker-authority", title: "Grant worker authority", json: workerAuthorityJSON, countPath: "authority_request_count", allowedPaths: ["talent_authority_granted", "worker_credential_issued", "live_worker_execution_allowed"], route: "/forge/worker-authority-requests/projection.json"),
      gatedItem(id: "talent-authorizations", title: "Talent signoff", json: talentAuthorizationsJSON, countPath: "talent_authorization_count", allowedPaths: ["worker_spawn_allowed", "human_ratification_allowed", "deployment_allowed"], route: "/forge/talent-authorizations/projection.json")
    ]
  }

  static func fleetPlans(from json: JSONDictionary) -> [FleetPlan] {
    json.array(at: "fleets").prefix(12).map { item in
      let status = item.string(at: "status", fallback: "drafted")
      let geometry = item.dictionary(at: "execution_geometry")
      let review = item.dictionary(at: "wide_plan_review")
      let progress = item.dictionary(at: "planning_progress")
      let goal = item.string(at: "goal", fallback: "")
      let streamCount = item.int(at: "stream_count", fallback: item.array(at: "streams").count)
      let streamChecks = uniqueStrings(item.array(at: "streams").flatMap { stream in
        stringArray(stream.value(at: "checks"))
      })
      let requestedChecks = stringArray(item.value(at: "checks"))
      let suggestedChecks = uniqueStrings(
        requestedChecks
          + stringList(item.value(at: "repo_profile_suggested_checks"))
          + streamChecks
      )
      let checkCount = geometry.int(at: "stream_check_count", fallback: suggestedChecks.count)
      return FleetPlan(
        id: item.string(at: "fleet_id", fallback: UUID().uuidString),
        spec: goal,
        checks: requestedChecks,
        phase: status == "ratified" ? "ready" : (status == "planning" ? "planning" : "proposed"),
        status: status,
        repo: item.string(at: "repo_id", fallback: ""),
        languagePack: item.string(at: "language_pack_id", fallback: ""),
        plannerModel: item.string(at: "planner_model", fallback: ""),
        streamCount: streamCount,
        requestedWidth: geometry.int(at: "requested_width", fallback: streamCount),
        workerLimit: geometry.int(at: "max_concurrent_workers", fallback: 0),
        checkCount: checkCount,
        suggestedChecks: suggestedChecks,
        proofSummary: item.string(at: "repo_profile_proof_plan_summary", fallback: "Proof gates appear when the fleet starts."),
        reviewLine: fleetReviewLine(from: review),
        reviewConcerns: fleetReviewConcern(from: review),
        reviewStatus: review.string(at: "status", fallback: ""),
        planningStage: progress.string(at: "stage", fallback: ""),
        planningDetail: progress.string(at: "detail", fallback: ""),
        builderMix: objectCountLine(from: geometry.dictionary(at: "by_coder"), fallback: "\(streamCount) streams"),
        sensitivityLine: objectCountLine(from: geometry.dictionary(at: "by_sensitivity"), fallback: "No sensitivity split recorded"),
        route: "/forge/fleet-plans/projection.json"
      )
    }
  }

  static func fleetRuns(from json: JSONDictionary) -> [FleetRun] {
    json.array(at: "fleet_runs").prefix(8).map { run in
      let proof = run.dictionary(at: "proof")
      let recovery = proof.dictionary(at: "runtime_recovery")
      let lanes = run.array(at: "lanes").prefix(8).map { lane -> FleetLane in
        let casting = lane.dictionary(at: "builder_casting")
        let model = firstString(
          in: [
            casting.value(at: "selected_model_id"),
            casting.value(at: "recommended_model_id"),
            lane.value(at: "model")
          ],
          fallback: ""
        )
        let missing = stringArray(lane.value(at: "missing_semantic_strategy_operations"))
        return FleetLane(
          id: lane.string(at: "stream_id", fallback: UUID().uuidString),
          streamID: lane.string(at: "stream_id", fallback: "stream"),
          state: lane.string(at: "state", fallback: "unknown"),
          runID: lane.string(at: "forge_run_id", fallback: ""),
          detail: cleanFleetDetail(lane.string(at: "detail", fallback: "")),
          coder: lane.string(at: "coder", fallback: lane.string(at: "planner_coder", fallback: "")),
          model: model,
          castingStatus: casting.string(at: "status", fallback: ""),
          castingReason: cleanFleetDetail(casting.string(at: "basis", fallback: "")),
          missingStrategy: missing.isEmpty ? "" : "\(missing.count) semantic signals missing"
        )
      }
      return FleetRun(
        id: run.string(at: "fleet_id", fallback: UUID().uuidString),
        goal: run.string(at: "goal", fallback: ""),
        checks: stringArray(run.value(at: "checks")),
        running: run.bool(at: "running", fallback: false),
        finished: run.bool(at: "finished", fallback: false),
        integrationState: run.string(at: "integration_state", fallback: ""),
        integrationDetail: cleanFleetDetail(run.string(at: "integration_detail", fallback: "")),
        reviewVerdict: cleanFleetDetail(run.string(at: "review_verdict", fallback: "")),
        status: fleetRunStatus(from: run, lanes: Array(lanes)),
        summary: fleetRunDetail(from: run),
        recovery: fleetRecoveryLine(from: recovery, run: run),
        lanes: Array(lanes)
      )
    }
  }

  private static func fleetRunStatus(from run: JSONDictionary, lanes: [FleetLane]) -> String {
    let integrationState = run.string(at: "integration_state", fallback: "")
    if integrationState == "did_not_land" { return "Integration failed" }
    if integrationState == "working" { return "Integrating" }
    if run.bool(at: "running", fallback: false) {
      return lanes.contains { $0.needsAttention } ? "Recovering" : "Working"
    }
    if lanes.contains(where: \.needsAttention) { return "Needs attention" }
    if run.bool(at: "finished", fallback: false) { return "Finished" }
    return "Started"
  }

  private static func fleetRunDetail(from run: JSONDictionary) -> String {
    let proof = run.dictionary(at: "proof")
    let laneCount = proof.int(at: "stream_count", fallback: run.array(at: "lanes").count)
    let integrationState = run.string(at: "integration_state", fallback: "")
    let integrationDetail = cleanFleetDetail(run.string(at: "integration_detail", fallback: ""))
    if integrationState == "did_not_land" {
      return integrationDetail.isEmpty
        ? "All \(laneCount) lanes finished, but integration did not land."
        : integrationDetail
    }
    let attention = proof.int(at: "lane_attention_count", fallback: 0)
    if attention > 0 {
      return "\(attention) of \(laneCount) lanes need attention"
    }
    if run.bool(at: "running", fallback: false) {
      return "\(laneCount) lanes are working"
    }
    let final = cleanFleetDetail(run.string(at: "final_detail", fallback: ""))
    return final.isEmpty ? "\(laneCount) lanes recorded" : final
  }

  private static func fleetRecoveryLine(from recovery: JSONDictionary, run: JSONDictionary) -> String {
    if run.string(at: "integration_state", fallback: "") == "did_not_land" {
      return "Integration could not land the lane branches. Review the merge evidence and recover with corrected scopes."
    }
    let next = cleanFleetDetail(recovery.string(at: "next_action", fallback: ""))
    let health = cleanFleetDetail(recovery.string(at: "recovery_health", fallback: ""))
    let attempts = recovery.int(at: "repair_attempt_count", fallback: 0)
    let maxAttempts = recovery.int(at: "max_repair_attempts", fallback: 0)
    if !next.isEmpty && maxAttempts > 0 {
      if next == "repair attention lanes before failing fleet" {
        return "Retry the lanes that need attention from their recorded branches. Repair \(attempts)/\(maxAttempts)."
      }
      return "\(next). Repair \(attempts)/\(maxAttempts)."
    }
    if !health.isEmpty { return health }
    return ""
  }

  private static func cleanFleetDetail(_ value: String) -> String {
    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return "" }
    return trimmed
      .replacingOccurrences(of: "_", with: " ")
      .replacingOccurrences(of: "=", with: " ")
      .replacingOccurrences(of: "  ", with: " ")
  }

  static func runners(
    from json: JSONDictionary,
    scoreboardJSON: JSONDictionary,
    preflightJSON: JSONDictionary,
    clearancesJSON: JSONDictionary
  ) -> [MachineRunner] {
    let selectedID = json.string(at: "selected_runner_id", fallback: "")
    let quarantine = json.string(at: "quarantine_boundary", fallback: "This machine's output is held until Forge checks accept it.")
    let scoresByID = rowsByRunner(scoreboardJSON.array(at: "runner_scores"))
    let checksByID = rowsByRunner(preflightJSON.array(at: "checks"))
    let clearancesByID = rowsByRunner(clearancesJSON.array(at: "clearances"))
    let compatibilityRows = scoreboardJSON.array(at: "runner_model_compatibility_matrix")
    let protocolDescriptions = Dictionary(
      uniqueKeysWithValues: scoreboardJSON.array(at: "adapter_protocol_tiers").map { tier in
        (
          tier.string(at: "adapter_protocol_tier", fallback: ""),
          tier.string(at: "description", fallback: "")
        )
      }
    )
    return json.array(at: "runner_profiles").prefix(10).map { item in
      let id = item.string(at: "runner_id", fallback: item.string(at: "id", fallback: UUID().uuidString))
      let score = scoresByID[id] ?? [:]
      let preflight = checksByID[id] ?? [:]
      let clearance = clearancesByID[id] ?? [:]
      let executionAllowed = item.bool(at: "adapter_execution_allowed", fallback: false)
      let accepted = score.int(at: "accepted", fallback: 0)
      let attempted = score.int(at: "tasks_attempted", fallback: 0)
      let adapterKind = firstString(
        in: [
          item.value(at: "adapter_kind"),
          preflight.value(at: "adapter_kind"),
          score.value(at: "adapter_kind")
        ],
        fallback: ""
      )
      let modelProfileID = item.string(at: "model_profile_id", fallback: item.string(at: "model_provider", fallback: ""))
      let compatibility = preferredCompatibilityRow(for: id, modelProfileID: modelProfileID, rows: compatibilityRows)
      let protocolTier = preferredProtocolTier(for: item, compatibility: compatibility)
      let castable = compatibility.bool(at: "castable", fallback: false)
      let runnerKind = item.string(at: "runner_kind", fallback: "runner")
      let isNative = runnerKind == "mdx_native" || adapterKind == "mdx_native" || id == "mdx_native_harness_runner"
      let requiresClearance = adapterRequiresClearance(adapterKind)
      let runnerStatus = item.string(at: "execution_status", fallback: "")
      let preflightStatus = preflight.string(at: "status", fallback: runnerStatus.isEmpty ? "waiting_for_preflight" : runnerStatus)
      let clearanceMode = clearance.string(at: "clearance_mode", fallback: "")
      let runtimeChecked = isNative || !preflight.isEmpty
      let binaryPresent = isNative || preflight.bool(at: "binary_present", fallback: false)
      let openRunnerOption =
        !isNative &&
        !requiresClearance &&
        (runnerStatus.hasPrefix("PROFILE_ADMITTED") || runnerStatus.hasPrefix("PROFILE_DECLARED"))
      let optionEnabled =
        isNative ||
        preflight.bool(at: "machine_option_enabled", fallback: false) ||
        compatibility.bool(at: "machine_option_enabled_by_provider_key", fallback: false) ||
        preflightStatus.localizedCaseInsensitiveContains("option_ready") ||
        openRunnerOption
      let liveExecutionReady = preflightStatus == "live_execution_ready"
      let smokeStatus = isNative ? "runtime_smoke_not_required" : score.string(at: "runtime_smoke_status", fallback: "runtime_smoke_not_run")
      let smokePassed = score.bool(at: "runtime_smoke_passed", fallback: false) || smokeStatus == "runtime_smoke_passed"
      let line: String
      if isNative {
        line = "Built in. Always available as the house builder."
      } else if liveExecutionReady {
        line = "Installed and turned on here. Its output is still held until Forge checks accept it."
      } else if optionEnabled && !runtimeChecked {
        line = "Available as an option. It has not been verified on this Mac yet."
      } else if optionEnabled && binaryPresent {
        line = "Available as an option. It stays off until you turn it on for this Mac."
      } else if optionEnabled {
        line = "The key is present, so it shows as an option, but it is not installed on this Mac yet."
      } else if requiresClearance && !clearanceMode.isEmpty {
        line = "Signed in. It still needs to be verified on this Mac before Forge can use it."
      } else if requiresClearance {
        line = "Supported, but held until you sign in and allow this tool here."
      } else if !binaryPresent {
        line = "Supported, but it is not installed on this Mac."
      } else if id == selectedID {
        line = "Chosen for this task from what the record shows."
      } else if castable {
        line = "Available as an option. It still needs to be verified before Forge can use it."
      } else if executionAllowed {
        line = "Available. Its output is still checked by Forge before it counts."
      } else if runnerKind.localizedCaseInsensitiveContains("external") {
        line = quarantine
      } else {
        line = item.string(at: "output_policy", fallback: "Output is accepted only after Forge checks accept it.")
      }
      return MachineRunner(
        id: id,
        name: machineDisplayName(id: id, fallback: item.string(at: "display_name", fallback: id)),
        kind: runnerKind,
        model: modelDisplayName(for: item, compatibility: compatibility),
        adapterKind: adapterKind,
        protocolTier: protocolTier,
        protocolLabel: protocolLabel(for: protocolTier, adapterKind: adapterKind),
        protocolDetail: protocolDetail(
          for: protocolTier,
          adapterKind: adapterKind,
          routeDescription: protocolDescriptions[protocolTier]
        ),
        status: item.string(at: "execution_status", fallback: "UNKNOWN"),
        executionAllowed: executionAllowed,
        selected: id == selectedID,
        castable: castable,
        requiresClearance: requiresClearance,
        clearanceMode: clearanceMode,
        clearanceLabel: isNative ? "Built in" : clearanceModeLabel(clearanceMode),
        scorecardEligible: clearance.bool(at: "scorecard_eligible", fallback: false),
        runtimeChecked: runtimeChecked,
        binaryPresent: binaryPresent,
        versionObserved: preflight.string(at: "version_observed", fallback: score.string(at: "runtime_version_observed", fallback: "")),
        optionEnabled: optionEnabled,
        optionEnablement: preflight.string(at: "machine_option_enablement", fallback: ""),
        liveExecutionReady: liveExecutionReady,
        smokeStatus: smokeStatus,
        smokePassed: smokePassed,
        evidenceCount: json.int(at: "scorecard_evidence_count", fallback: 0),
        acceptedCount: accepted,
        tasksAttempted: attempted,
        passRatePct: score.int(at: "pass_rate_pct", fallback: 0),
        readinessLine: line,
        compatibilityLine: compatibilityLine(from: compatibility, adapterKind: adapterKind),
        adminActionLine: machineAdminActionLine(
          isNative: isNative,
          requiresClearance: requiresClearance,
          clearanceMode: clearanceMode,
          smokePassed: smokePassed,
          runtimeChecked: runtimeChecked,
          binaryPresent: binaryPresent
        )
      )
    }
  }

  private static func rowsByRunner(_ rows: [JSONDictionary]) -> [String: JSONDictionary] {
    rows.reduce(into: [:]) { result, row in
      let runnerID = row.string(at: "runner_id", fallback: "")
      if !runnerID.isEmpty {
        result[runnerID] = row
      }
    }
  }

  static func modelRoleRoutes(from json: JSONDictionary) -> [ModelRoleRoute] {
    let roles = json.array(at: "roles")
    let byRole = Dictionary(uniqueKeysWithValues: roles.map { role in
      (role.string(at: "role", fallback: ""), role)
    })
    let groups: [(id: String, title: String, roles: [String], advisoryOnly: Bool)] = [
      ("plan", "Plan & orchestrate", ["planner", "conductor"], false),
      ("build", "Build", ["executor"], false),
      ("integrate", "Integrate", ["integrator"], false),
      ("review", "Review", ["reviewer"], false),
      ("evaluate", "Evaluate", ["evaluator"], false),
      ("advisor", "Senior advisor", ["advisor"], true)
    ]
    return groups.map { group in
      let resolved = group.roles.compactMap { byRole[$0] }.first { role in
        role.bool(at: "credential_available", fallback: false)
      } ?? group.roles.compactMap { byRole[$0] }.first ?? [:]
      let slot = resolved.string(at: "slot", fallback: "")
      let assignedSlot = resolved.string(at: "assigned_slot", fallback: "")
      let provider = providerDisplayName(resolved.string(at: "provider_family", fallback: ""))
      return ModelRoleRoute(
        id: group.id,
        title: group.title,
        model: resolved.string(at: "model_id", fallback: ""),
        provider: provider,
        slot: slot,
        ready: resolved.bool(at: "credential_available", fallback: false),
        assigned: !assignedSlot.isEmpty && assignedSlot != "AUTO",
        advisoryOnly: group.advisoryOnly
      )
    }
  }

  private static func adapterRequiresClearance(_ adapterKind: String) -> Bool {
    adapterKind == "grok_build_cli" || adapterKind == "claude_code"
  }

  private static func providerDisplayName(_ provider: String) -> String {
    switch provider {
    case "anthropic": return "Claude"
    case "openai": return "GPT"
    case "xai": return "Grok"
    case "gemini": return "Gemini"
    case "bedrock": return "Bedrock"
    case "ollama": return "Local model"
    case "": return ""
    default: return humanLabelForRoute(provider)
    }
  }

  private static func clearanceModeLabel(_ mode: String) -> String {
    switch mode {
    case "fleet_eval": return "Signed in for comparisons"
    case "local_companion": return "Signed in on this Mac"
    case "": return "Not signed in"
    default: return humanLabelForRoute(mode)
    }
  }

  private static func machineAdminActionLine(
    isNative: Bool,
    requiresClearance: Bool,
    clearanceMode: String,
    smokePassed: Bool,
    runtimeChecked: Bool,
    binaryPresent: Bool
  ) -> String {
    if isNative {
      return "No turn-on control is needed for the native builder."
    }
    if requiresClearance && clearanceMode.isEmpty {
      return "Sign in and allow this machine here before you compare it."
    }
    if !runtimeChecked {
      return "Check this Mac to verify it. It can take up to a minute."
    }
    if smokePassed {
      return "Verified on this Mac for the recorded version."
    }
    if binaryPresent {
      return "Check this Mac to verify it and record the version."
    }
    return "Install it on this Mac, then check again."
  }

  private static func evidenceItems(
    from briefingJSON: JSONDictionary,
    workspaceJSON: JSONDictionary,
    cards: [RouteCard]
  ) -> [EvidenceItem] {
    var items = briefingJSON.array(at: "route_inputs").map { input in
      EvidenceItem(
        id: input.string(at: "id", fallback: input.string(at: "route", fallback: UUID().uuidString)),
        title: input.string(at: "role", fallback: input.string(at: "id", fallback: "Route input")),
        detail: input.string(at: "response_schema_path", fallback: "Route-backed evidence"),
        route: input.string(at: "route", fallback: ""),
        readOnly: input.bool(at: "read_only", fallback: true)
      )
    }

    if items.isEmpty {
      items = workspaceJSON.array(at: "related_routes").map { input in
        EvidenceItem(
          id: input.string(at: "route", fallback: UUID().uuidString),
          title: input.string(at: "label", fallback: "Related route"),
          detail: input.string(at: "role", fallback: "Route-backed evidence"),
          route: input.string(at: "route", fallback: ""),
          readOnly: input.bool(at: "read_only", fallback: true)
        )
      }
    }

    if items.isEmpty {
      items = cards.filter(\.receiptBacked).map { card in
        EvidenceItem(id: card.id, title: card.title, detail: card.detail, route: card.path, readOnly: card.readOnly)
      }
    }

    return Array(items.prefix(14))
  }

  private static func authorityItems(
    from briefingJSON: JSONDictionary,
    workspaceJSON: JSONDictionary
  ) -> [AuthorityItem] {
    var items = briefingJSON.array(at: "blocked_actions").prefix(8).enumerated().map { index, item in
      AuthorityItem(
        id: item.string(at: "id", fallback: "blocked-\(index)"),
        title: item.string(at: "label", fallback: item.string(at: "action", fallback: "Blocked action")),
        detail: item.string(at: "reason", fallback: item.string(at: "status", fallback: "Held by authority boundary")),
        allowed: false
      )
    }

    if items.isEmpty {
      items = workspaceJSON.array(at: "blocked_controls").prefix(8).enumerated().map { index, item in
        AuthorityItem(
          id: item.string(at: "id", fallback: "blocked-control-\(index)"),
          title: item.string(at: "label", fallback: "Blocked control"),
          detail: item.string(at: "reason", fallback: "Held by authority boundary"),
          allowed: false
        )
      }
    }

    if items.isEmpty {
      items = [
        AuthorityItem(id: "worker", title: "Worker spawn", detail: "Closed until authority rails are green", allowed: false),
        AuthorityItem(id: "patch", title: "Patch application", detail: "Closed until scoped write authority is proven", allowed: false),
        AuthorityItem(id: "deploy", title: "Deployment", detail: "Human ratification and deployment proof required", allowed: false)
      ]
    }

    return items
  }

  private static func firstString(in candidates: [Any?], fallback: String) -> String {
    for candidate in candidates {
      if let value = candidate as? String, !value.isEmpty {
        return value
      }
    }
    return fallback
  }

  private static func fleetReviewLine(from review: JSONDictionary) -> String {
    guard review.bool(at: "required", fallback: false) else {
      return "No pre-build plan review required."
    }
    let reviewer = review.string(at: "reviewer_model", fallback: "")
    let confidence = review.string(at: "confidence", fallback: "")
    let verdict = review.string(at: "verdict", fallback: "")
    let status = review.string(at: "status", fallback: "")
    let reviewerText = reviewer.isEmpty ? "" : " by \(reviewer)"
    let confidenceText = confidence.isEmpty || confidence == "none" ? "" : ", \(confidence) confidence"
    if status == "missing_reviewer" {
      return "Connect a reviewer before workers start."
    }
    if status == "review_unavailable" {
      return "Advisor review\(reviewerText) could not run. Review is still required before workers start."
    }
    if verdict.localizedCaseInsensitiveContains("revise") {
      return "Advisor review\(reviewerText) asked for changes\(confidenceText)."
    }
    if verdict.localizedCaseInsensitiveContains("approve") || verdict.localizedCaseInsensitiveContains("ready") {
      return "Advisor review\(reviewerText) found this ready\(confidenceText)."
    }
    if status != "recorded" && status != "ready" {
      return "Advisor review\(reviewerText) is required before workers start."
    }
    return "Advisor review\(reviewerText) read this plan\(confidenceText)."
  }

  private static func fleetReviewConcern(from review: JSONDictionary) -> String {
    guard review.bool(at: "required", fallback: false) else { return "" }
    guard review.string(at: "verdict", fallback: "").localizedCaseInsensitiveContains("revise") else {
      return ""
    }
    let raw = review.string(at: "concerns", fallback: "")
    guard !raw.isEmpty else { return "" }
    for line in raw.components(separatedBy: .newlines) {
      let cleaned = cleanFleetReviewConcernLine(line)
      if !cleaned.isEmpty {
        return cappedInlineText(cleaned, limit: 220)
      }
    }
    return ""
  }

  private static func cleanFleetReviewConcernLine(_ line: String) -> String {
    var text = line.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !text.isEmpty else { return "" }
    let upper = text.uppercased()
    if upper.hasPrefix("VERDICT:") || upper.hasPrefix("CONFIDENCE:") { return "" }
    if text.hasPrefix("- ") { text.removeFirst(2) }
    if let marker = text.firstIndex(of: ")") {
      let prefix = text[..<marker]
      if !prefix.isEmpty && prefix.allSatisfy(\.isNumber) {
        text = String(text[text.index(after: marker)...]).trimmingCharacters(in: .whitespacesAndNewlines)
      }
    }
    return text
  }

  private static func cappedInlineText(_ text: String, limit: Int) -> String {
    guard text.count > limit else { return text }
    return String(text.prefix(max(0, limit - 3))).trimmingCharacters(in: .whitespacesAndNewlines) + "..."
  }

  private static func preferredCompatibilityRow(
    for runnerID: String,
    modelProfileID: String,
    rows: [JSONDictionary]
  ) -> JSONDictionary {
    let runnerRows = rows.filter { $0.string(at: "runner_id", fallback: "") == runnerID }
    if let exact = runnerRows.first(where: { $0.string(at: "model_profile_id", fallback: "") == modelProfileID }) {
      return exact
    }
    if let castable = runnerRows.first(where: { $0.bool(at: "castable", fallback: false) }) {
      return castable
    }
    return runnerRows.first ?? [:]
  }

  private static func preferredProtocolTier(for runner: JSONDictionary, compatibility: JSONDictionary) -> String {
    let adapterKind = runner.string(at: "adapter_kind", fallback: "")
    let compatibilityTier = compatibility.string(at: "adapter_protocol_tier", fallback: "")
    if compatibilityTier != "declared_only", !compatibilityTier.isEmpty {
      return compatibilityTier
    }
    switch adapterKind {
    case "grok_build_cli": return "protocol_adapter"
    case "claude_code": return "sdk_adapter"
    case "codex_cli": return "headless_cli_adapter"
    case "mdx_native": return "native_harness"
    default: return compatibilityTier.isEmpty ? runner.string(at: "wire_api", fallback: "") : compatibilityTier
    }
  }

  private static func protocolLabel(for tier: String, adapterKind: String) -> String {
    switch tier {
    case "protocol_adapter":
      return adapterKind == "grok_build_cli" ? "ACP" : "Protocol adapter"
    case "sdk_adapter":
      return adapterKind == "claude_code" ? "Agents SDK" : "SDK adapter"
    case "headless_cli_adapter":
      return "CLI"
    case "native_harness":
      return "Native"
    case "local_companion_adapter":
      return "Local companion"
    case "declared_only":
      return "Declared"
    default:
      return tier.isEmpty ? "Adapter pending" : humanLabelForRoute(tier)
    }
  }

  private static func protocolDetail(for tier: String, adapterKind: String, routeDescription: String?) -> String {
    switch (tier, adapterKind) {
    case ("protocol_adapter", "grok_build_cli"):
      return "Grok Build is integrated through ACP stdio and stays behind the machine execution gate."
    case ("sdk_adapter", "claude_code"):
      return "Claude Code is integrated through the Claude Agents SDK and stays behind the machine execution gate."
    case ("native_harness", _):
      return "Forge's native builder runs inside the governed kernel envelope."
    default:
      if let routeDescription, !routeDescription.isEmpty {
        return routeDescription
      }
      return "Machine output stays held until MDx gates and review accept it."
    }
  }

  private static func machineDisplayName(id: String, fallback: String) -> String {
    switch id {
    case "grok_build_cli_external_worker": return "Grok Build"
    case "claude_code_external_worker": return "Claude Code"
    default: return fallback
    }
  }

  private static func modelDisplayName(for runner: JSONDictionary, compatibility: JSONDictionary) -> String {
    let profileID = runner.string(at: "model_profile_id", fallback: runner.string(at: "model_provider", fallback: ""))
    if let label = modelProfileDisplayName(profileID) {
      return label
    }
    let compatibilityDisplay = compatibility.string(at: "model_display_name", fallback: "")
    if !compatibilityDisplay.isEmpty {
      return compatibilityDisplay
    }
    return runner.string(at: "model", fallback: runner.string(at: "model_provider", fallback: "model pending"))
  }

  private static func modelProfileDisplayName(_ profileID: String) -> String? {
    switch profileID {
    case "xai_grok_build_model_profile", "grok-build-cli-selected-model":
      return "Grok Build with Grok 4.5"
    case "claude_code_model_profile", "claude-code-selected-model":
      return "Claude Agent SDK selected model"
    case "mdx_native_local_model_profile", "mdx-native-builder":
      return "Forge Native Builder"
    case "codex_xai_responses_profile":
      return "Grok 4.5"
    default:
      return nil
    }
  }

  private static func compatibilityLine(from compatibility: JSONDictionary, adapterKind: String) -> String {
    let status = compatibility.string(at: "compatibility_status", fallback: "")
    if status == "compatible_grok_build_acp_key_present" {
      return "The xAI key is present, so Forge can use this machine once it is verified on this Mac."
    }
    if status == "compatible_claude_agent_sdk_key_present" {
      return "The Anthropic key is present, so Forge can use this machine once it is verified on this Mac."
    }
    if status == "blocked_pending_xai_key" {
      return "Needs an xAI key before Forge can use Grok Build."
    }
    if status == "blocked_pending_anthropic_key" {
      return "Needs an Anthropic key before Forge can use Claude Code."
    }
    if status == "blocked_pending_tos_auth_clearance" {
      return "Sign-in and verifying on this Mac still hold this machine closed."
    }
    let reason = compatibility.string(at: "compatibility_reason", fallback: "")
    return reason.isEmpty ? "Still gathering evidence." : reason
  }

  private static func humanLabelForRoute(_ raw: String) -> String {
    raw
      .replacingOccurrences(of: "_", with: " ")
      .replacingOccurrences(of: "-", with: " ")
      .split(separator: " ")
      .map { $0.prefix(1).uppercased() + $0.dropFirst() }
      .joined(separator: " ")
  }

  private static func objectCountLine(from dictionary: JSONDictionary, fallback: String) -> String {
    let parts = dictionary
      .map { key, value -> String in
        let count = (value as? NSNumber)?.intValue ?? value as? Int ?? 0
        return "\(count)x \(key)"
      }
      .sorted()
    return parts.isEmpty ? fallback : parts.joined(separator: ", ")
  }

  private static func stringArray(_ value: Any?) -> [String] {
    if let values = value as? [String] {
      return values
    }
    if let values = value as? [Any] {
      return values.compactMap { $0 as? String }
    }
    return []
  }

  private static func firstRecipeSummary(from json: JSONDictionary) -> String {
    json.array(at: "recipes").first?.string(at: "summary", fallback: "Recipes are ready for governed build requests.") ?? "Recipes are ready for governed build requests."
  }

  private static func preflightItem(
    id: String,
    title: String,
    json: JSONDictionary,
    countPath: String,
    allowedPath: String,
    detail: String,
    route: String
  ) -> SurfaceItem {
    SurfaceItem(
      id: id,
      title: title,
      subtitle: "\(json.int(at: countPath, fallback: json.array(at: "preflights").count)) records",
      status: json.bool(at: allowedPath, fallback: false) ? "Allowed" : "Locked",
      detail: detail,
      route: route
    )
  }

  private static func gatedItem(
    id: String,
    title: String,
    json: JSONDictionary,
    countPath: String,
    allowedPaths: [String],
    route: String
  ) -> SurfaceItem {
    let openCount = allowedPaths.filter { json.bool(at: $0, fallback: false) }.count
    return SurfaceItem(
      id: id,
      title: title,
      subtitle: "\(json.int(at: countPath, fallback: 0)) records",
      status: openCount == allowedPaths.count ? "Ready" : "Locked",
      detail: "\(openCount) of \(allowedPaths.count) authority gates are open.",
      route: route
    )
  }
}

// @unchecked Sendable: `json` is a decoded [String: Any] (not statically
// Sendable), but each LoadedRoute is produced by one task, never mutated after
// load, and only read. Safe to hand back from the snapshot load's TaskGroup.
private struct LoadedRoute: @unchecked Sendable {
  let card: RouteCard
  let json: JSONDictionary
}

private struct RouteSpec {
  let title: String
  let path: String
  let lane: RouteLane
  let receiptBacked: Bool
  var timeout: TimeInterval = 8
}

private extension Array where Element == LoadedRoute {
  func json(for path: String) -> JSONDictionary {
    first(where: { $0.card.path == path })?.json ?? [:]
  }
}

typealias JSONDictionary = [String: Any]

private extension JSONDictionary {
  func value(at path: String) -> Any? {
    let parts = path.split(separator: ".").map(String.init)
    var current: Any? = self
    for part in parts {
      guard let dictionary = current as? JSONDictionary else {
        return nil
      }
      current = dictionary[part]
    }
    return current
  }

  func string(at path: String, fallback: String) -> String {
    if let value = value(at: path) as? String {
      return value
    }
    if let value = value(at: path) as? Int {
      return "\(value)"
    }
    if let value = value(at: path) as? Bool {
      return value ? "true" : "false"
    }
    return fallback
  }

  func int(at path: String, fallback: Int) -> Int {
    int(at: path) ?? fallback
  }

  func int(at path: String) -> Int? {
    if let value = value(at: path) as? Int {
      return value
    }
    if let value = value(at: path) as? NSNumber {
      return value.intValue
    }
    return nil
  }

  func double(at path: String, fallback: Double) -> Double {
    if let value = value(at: path) as? NSNumber {
      return value.doubleValue
    }
    if let value = value(at: path) as? Double {
      return value
    }
    return fallback
  }

  func bool(at path: String, fallback: Bool) -> Bool {
    if let value = value(at: path) as? Bool {
      return value
    }
    return fallback
  }

  func array(at path: String) -> [JSONDictionary] {
    value(at: path) as? [JSONDictionary] ?? []
  }

  func dictionary(at path: String) -> JSONDictionary {
    value(at: path) as? JSONDictionary ?? [:]
  }

  func readOnlyValue() -> Bool? {
    if let value = self["read_only"] as? Bool {
      return value
    }
    if let value = self["writes_allowed"] as? Bool {
      return !value
    }
    if let value = self["production_write_allowed"] as? Bool {
      return !value
    }
    return nil
  }
}

/// Human copy for a failed projection load: distinguishes "MDx unreachable"
/// from "route missing on this kernel" from "unexpected shape". Views show
/// this text; the raw error stays in diagnostics.
func friendlyLoadError(_ error: Error) -> String {
  if let routeError = error as? RouteClientError {
    switch routeError {
    case .httpStatus(404):
      return "This part of MDx isn't available on this kernel yet."
    case .httpStatus(let status):
      return "MDx answered with an error (HTTP \(status))."
    case .refused(let reason):
      return reason
    case .untrustedHost:
      return routeError.localizedDescription
    case .invalidResponse, .invalidJSON:
      return "MDx sent back something unexpected."
    }
  }
  if let urlError = error as? URLError {
    switch urlError.code {
    case .timedOut:
      return "MDx is taking too long to answer."
    default:
      return "MDx isn't reachable right now."
    }
  }
  return error.localizedDescription
}

enum RouteClientError: LocalizedError {
  case invalidResponse
  case invalidJSON
  case httpStatus(Int)
  case refused(String)
  case untrustedHost(String)

  var errorDescription: String? {
    switch self {
    case .invalidResponse:
      return "Route returned an invalid response."
    case .invalidJSON:
      return "Route returned JSON in an unexpected shape."
    case .httpStatus(let status):
      return "Route returned HTTP \(status)."
    case .refused(let reason):
      return reason
    case .untrustedHost(let path):
      return "MDx blocked \(path) because the configured API host is not local. Use the local route server or add an authenticated trusted remote profile first."
    }
  }
}
