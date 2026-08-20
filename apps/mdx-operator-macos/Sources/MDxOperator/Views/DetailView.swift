import AppKit
import SwiftUI

struct DetailView: View {
  @Environment(OperatorStore.self) private var store
  @Environment(CloudAuthStore.self) private var auth
  @Binding var searchText: String
  @State private var selectedAction: GovernedActionKind = .requestBuild
  @State private var highlightedRunID: String?
  @State private var selectedMachineRunner: MachineRunner?
  @State private var showRecordedRuns = false

  var body: some View {
    @Bindable var store = store
    return Group {
      if store.selectedAppRoute == .forge(.runs), let run = store.selectedRun {
        RunDetailView(run: run)
      } else if store.selectedAppRoute == .forge(.missions), let mission = store.selectedMission {
        MissionDetailView(mission: mission)
      } else if store.selectedAppRoute == .forge(.missions) {
        AutomationsView()
      } else if store.selectedAppRoute == .twin {
        TwinView()
      } else if store.selectedAppRoute == .pages {
        PagesView()
      } else if store.selectedAppRoute == .message {
        MessageView()
      } else if store.selectedAppRoute == .memory {
        MemoryView()
      } else if store.selectedAppRoute == .marketplace {
        MarketplaceView()
      } else {
        ScrollView {
          if #available(macOS 26.0, *) {
            GlassEffectContainer(spacing: 12) {
              content
            }
          } else {
            content
          }
        }
      }
    }
    .sheet(isPresented: $store.showStartBuild) {
      StartBuildSheet()
        .environment(store)
    }
    .sheet(isPresented: $store.showMissionSetup) {
      MissionSetupSheet()
        .environment(store)
    }
    .sheet(item: $selectedMachineRunner) { runner in
      MachineRunnerSheet(
        runner: runner,
        result: store.machineActionResult,
        inFlight: store.machineActionInFlight,
        stageFaceOff: {
          Task { await store.stageMachineFaceOff(runnerID: runner.id) }
        }
      )
    }
    // The toolbar search box must be live on every surface: route it into the
    // surface's own filter instead of silently filtering only Forge lists.
    .onChange(of: searchText) { _, value in
      routeSearch(value)
    }
    .onChange(of: store.selectedAppRoute) { _, _ in
      if !searchText.isEmpty { searchText = "" }
    }
  }

  /// Bindings into the observable store for non-body scopes.
  private var bindableStore: Bindable<OperatorStore> { Bindable(store) }

  private var isHosted: Bool {
    store.snapshot.baseURL.scheme?.lowercased() == "https"
  }

  private var connectionName: String {
    isHosted ? "MDx Cloud" : "This Mac"
  }

  private var identityDisplayName: String? {
    let name = auth.displayName.trimmingCharacters(in: .whitespacesAndNewlines)
    return name.isEmpty ? nil : name
  }

  private func routeSearch(_ query: String) {
    switch store.selectedAppRoute {
    case .twin:
      store.twinConvoSearch = query
    case .pages:
      store.pageSearch = query
      store.runPageSearch(query)
    case .marketplace:
      store.marketplaceSearch = query
    case .message:
      store.messageSearch = query
    case .memory:
      store.memorySearch = query
    default:
      break // Home and Forge lists consume searchText directly.
    }
  }

  private var content: some View {
    VStack(alignment: .leading, spacing: 22) {
      MacUpdateBanner()
      AppLifecycleBanner()
      header

      switch store.selectedAppRoute {
      case .home:
        homePanel
      case .forge(let lane):
        forgePanel(for: lane)
      case .twin:
        flagshipPanel(.twin)
      case .pages:
        flagshipPanel(.pages)
      case .message:
        flagshipPanel(.message)
      case .memory:
        flagshipPanel(.memory)
      case .marketplace:
        flagshipPanel(.marketplace)
      case .you:
        youPanel
      }
    }
    .padding(24)
    .frame(maxWidth: .infinity, alignment: .leading)
  }

  private var header: some View {
    VStack(alignment: .leading, spacing: 14) {
      HStack(alignment: .firstTextBaseline) {
        VStack(alignment: .leading, spacing: 7) {
          Text(store.selectedAppRoute.title)
            .font(store.selectedAppRoute == .home ? .largeTitle.weight(.semibold) : .title2.weight(.semibold))
          Text(routeSubtitleText)
            .font(store.selectedAppRoute == .home ? .title3 : .callout)
            .foregroundStyle(.secondary)
            .lineLimit(3)
        }

        Spacer(minLength: 20)

        Text(refreshText)
          .font(.caption)
          .foregroundStyle(.secondary)
      }

      HStack(spacing: 10) {
        StatusPill(status: connectionPillText, tone: store.snapshot.connectionStatus == .ok ? .positive : .neutral)
        if store.selectedAppRoute.isForge, !stagePillText.isEmpty {
          StatusPill(status: stagePillText, tone: .neutral)
        }
        Spacer(minLength: 0)
        if !primaryVerbDisabled {
          Button {
            store.startBuildFlow()
          } label: {
            Label(store.selectedAppRoute.primaryVerb, systemImage: primaryVerbIcon)
          }
          .mdxPrimaryButtonStyle()
          .help(primaryVerbHelp)
        }
      }

      // The safe-next-move banner belongs on Overview and the app surfaces. On a
      // Forge work lane the header subtitle already carries the same signal, so
      // repeating it here (and again in the inspector) is the idle line stated
      // three times. Show it once.
      if showsSafeNextMoveBanner {
        Label(safeNextMoveText, systemImage: "checkmark.seal")
          .font(.callout)
          .foregroundStyle(.primary)
          .padding(12)
          .frame(maxWidth: .infinity, alignment: .leading)
          .mdxGlassSurface()
      }
    }
  }

  private var showsSafeNextMoveBanner: Bool {
    switch store.selectedAppRoute {
    case .forge:
      return false
    default:
      return true
    }
  }

  private var refreshText: String {
    if case .loading = store.phase {
      return "Refreshing"
    }
    if let loadedAt = store.snapshot.loadedAt {
      return "Updated \(OperatorFormatters.time.string(from: loadedAt))"
    }
    return "Not connected"
  }

  private var currentSignalText: String {
    if store.snapshot.currentSignal.localizedCaseInsensitiveContains("No Forge worker job is in flight") {
      return store.forgeRuns.contains(where: { !$0.isRunning })
        ? "Ready for your next build"
        : "Forge is ready for its first build"
    }
    return store.snapshot.currentSignal
  }

  private var connectionPillText: String {
    switch store.snapshot.connectionStatus {
    case .ok: return "Connected"
    case .loading: return "Connecting"
    case .unavailable: return "Offline"
    }
  }

  private var routeSubtitleText: String {
    switch store.selectedAppRoute {
    case .home:
      return isConnected
        ? "Here is what is live, waiting, and safe to ask about."
        : (isHosted
          ? "Reconnect to MDx Cloud to see live work and decisions."
          : "Connect local MDx to see live work and decisions.")
    case .forge:
      return currentSignalText
    case .twin:
      return "Ask from evidence. Action stays governed."
    case .pages:
      return "Typed knowledge, origins, and context will gather here."
    case .message:
      return "Governed coordination across people, agents, and work."
    case .memory:
      return "Approved lessons, team memory, and model evidence stay distinct."
    case .marketplace:
      return "Capabilities are added through audited profiles."
    case .you:
      return "Identity, authority, connections, and preferences."
    }
  }

  private var isConnected: Bool {
    store.snapshot.connectionStatus == .ok
  }

  private var stagePillText: String {
    switch store.selectedAppRoute {
    case .forge:
      return StatusCopy.human(store.snapshot.currentStage)
    case .home:
      return "situation"
    case .twin:
      return "companion"
    case .pages:
      return "knowledge"
    case .message:
      return "coordination"
    case .memory:
      return "recall"
    case .marketplace:
      return "capabilities"
    case .you:
      return "identity"
    }
  }

  private var primaryVerbIcon: String {
    switch store.selectedAppRoute {
    case .home, .twin: "sparkles"
    case .forge: "plus.circle"
    case .pages: "square.and.pencil"
    case .message: "paperplane"
    case .memory: "brain.head.profile"
    case .marketplace: "plus.app"
    case .you: "lock.open"
    }
  }

  private var primaryVerbDisabled: Bool {
    switch store.selectedAppRoute {
    case .forge(let lane):
      return lane != .overview || forgeNextMove.kind != .start
    default:
      return true
    }
  }

  private var forgeNextMove: ForgeNextMove {
    ForgeNextMove.derive(
      decisions: store.snapshot.forgeDecisions,
      runs: store.sortedRuns,
      connected: isConnected
    )
  }

  private func performForgeNextMove() {
    switch forgeNextMove.kind {
    case .decision:
      store.select(.forge(forgeNextMove.destination ?? .overview))
      if let runID = forgeNextMove.runID { store.openRun(runID) }
    case .review:
      store.select(.forge(.review))
      if let runID = forgeNextMove.runID { store.openRun(runID) }
    case .recover, .follow:
      store.select(.forge(.runs))
      if let runID = forgeNextMove.runID { store.openRun(runID) }
    case .start:
      store.startBuildFlow()
    case .connect:
      Task { await store.refresh() }
    }
  }

  private var primaryVerbHelp: String {
    switch store.selectedAppRoute {
    case .forge:
      return "Record a governed build request through the kernel. Execution still follows route authority."
    default:
      return "This surface is scaffolded in the native shell. Governed action waits for its route and receipt rails."
    }
  }

  private var safeNextMoveText: String {
    // The read-only governance banner belongs on Overview only. On other lanes
    // the header shows the kernel's route-appropriate safe next move.
    if store.snapshot.safeNextMove.localizedCaseInsensitiveContains("Open the operator workspace") {
      if store.selectedAppRoute == .forge(.overview) {
        return "Forge works read-only until you approve writes. Review what it can touch before shipping."
      }
      return store.snapshot.currentSignal
    }
    return store.snapshot.safeNextMove
  }

  private var workspaceIntentText: String {
    if !isConnected {
      return isHosted
        ? "Retry the private workspace connection."
        : "Start the local MDx route server, then refresh."
    }
    if store.snapshot.workspaceIntent.localizedCaseInsensitiveContains("future Forge shell") {
      return "MDx is ready to take a governed build request when you are."
    }
    return store.snapshot.workspaceIntent
  }

  private var authorityText: String {
    store.snapshot.authority == "none" ? "No authority granted" : displayLabel(store.snapshot.authority)
  }

  private var productPostureText: String {
    switch store.snapshot.productPosture {
    case "proof_scaffolding_input_not_product_ui":
      return "Still proving the local path"
    default:
      return displayLabel(store.snapshot.productPosture)
    }
  }

  private func displayLabel(_ rawValue: String) -> String {
    rawValue
      .replacingOccurrences(of: "_", with: " ")
      .split(separator: " ")
      .map { word in
        guard let first = word.first else { return "" }
        return first.uppercased() + word.dropFirst().lowercased()
      }
      .joined(separator: " ")
  }

  private var metricGrid: some View {
    LazyVGrid(columns: [GridItem(.adaptive(minimum: 132), spacing: 12)], spacing: 12) {
      ForEach(store.snapshot.metrics) { metric in
        MetricTile(metric: metric)
      }
    }
  }

  @ViewBuilder
  private func forgePanel(for lane: ForgeLane) -> some View {
    switch lane {
    case .overview:
      overviewPanel
    case .runs:
      runRoomPanel
    case .missions:
      // The Missions lane renders the folded surface; DetailView.body routes a
      // selected mission to MissionDetailView before this switch is reached.
      AutomationsView()
    case .review:
      reviewPanel
    case .fleet:
      fleetPanel
    case .machines:
      machinePanel
    case .evidence:
      evidencePanel
    }
  }

  @ViewBuilder
  private var homePanel: some View {
    if isConnected {
      connectedHomePanel
    } else {
      offlineHomePanel
    }
  }

  private var connectedHomePanel: some View {
    VStack(alignment: .leading, spacing: 18) {
      if store.launchedAfterUncleanExit {
        HStack(spacing: 10) {
          Image(systemName: "bandage")
            .foregroundStyle(.orange)
          Text("MDx quit unexpectedly last time. A report helps us fix it.")
            .font(.callout)
          Spacer(minLength: 8)
          Button("Send report") { store.startFeedback(prefillCrash: true) }
            .controlSize(.small)
          Button("Dismiss") { store.dismissCrashNotice() }
            .controlSize(.small)
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
        }
        .padding(12)
        .background(
          RoundedRectangle(cornerRadius: 10, style: .continuous)
            .fill(Color.orange.opacity(0.08))
        )
      }

      if store.appBelowKernelMinimum, let kernel = store.kernelVersion {
        HStack(spacing: 10) {
          Image(systemName: "arrow.triangle.2.circlepath.circle")
            .foregroundStyle(.orange)
          Text("Your MDx kernel (\(kernel.kernelVersion)) needs app \(kernel.minAppVersion) or newer — this build is \(Bundle.main.appVersionLabel). Update the app.")
            .font(.callout)
          Spacer(minLength: 8)
          Button("Details") { store.showDiagnosticsPanel() }
            .controlSize(.small)
        }
        .padding(12)
        .background(
          RoundedRectangle(cornerRadius: 10, style: .continuous)
            .fill(Color.orange.opacity(0.08))
        )
      } else if store.kernelVersion == nil, store.contractDriftCount >= 3 {
        HStack(spacing: 10) {
          Image(systemName: "arrow.triangle.2.circlepath.circle")
            .foregroundStyle(.orange)
          Text("This app and your local MDx look like different versions: \(store.contractDriftCount) routes are missing. Update whichever is older.")
            .font(.callout)
          Spacer(minLength: 8)
          Button("Details") { store.showDiagnosticsPanel() }
            .controlSize(.small)
        }
        .padding(12)
        .background(
          RoundedRectangle(cornerRadius: 10, style: .continuous)
            .fill(Color.orange.opacity(0.08))
        )
      }

      let needsYou = store.reviewReadyRuns
      let running = store.activeRuns
      let sinceYouLeft = store.sinceYouLeftRuns

      // The daily brief: what happened while you were away, first.
      if !sinceYouLeft.isEmpty {
        VStack(alignment: .leading, spacing: 8) {
          HStack(alignment: .firstTextBaseline) {
            SectionHeader(
              title: "Since you left",
              subtitle: "\(sinceYouLeft.count) run\(sinceYouLeft.count == 1 ? "" : "s") finished while you were away."
            )
            Spacer(minLength: 8)
            Button("Dismiss") {
              withAnimation(.easeOut(duration: 0.15)) { store.dismissSinceYouLeft() }
            }
            .controlSize(.small)
          }
          LazyVStack(alignment: .leading, spacing: 4) {
            ForEach(sinceYouLeft.prefix(6)) { run in
              RunRow(run: run) { store.select(.forge(.runs)); store.openRun(run.id) }
            }
          }
        }
        .padding(14)
        .background(
          RoundedRectangle(cornerRadius: 11, style: .continuous)
            .fill(Color.accentColor.opacity(0.06))
        )
        .overlay(
          RoundedRectangle(cornerRadius: 11, style: .continuous)
            .stroke(Color.accentColor.opacity(0.18), lineWidth: 1)
        )
      }

      // What needs a decision leads the page; the composer only takes the top
      // slot when nothing is waiting on the person.
      if needsYou.isEmpty {
        TwinComposerCard()
        SectionHeader(
          title: "Needs you",
          subtitle: "Nothing is waiting on your decision right now."
        )
        CalmStateCard(
          title: "You're all caught up",
          detail: "When a run finishes with a diff to review, it shows up here for your ship decision.",
          systemImage: "checkmark.circle"
        )
      } else {
        SectionHeader(
          title: "Needs you",
          subtitle: "Runs that have proof ready and want your call."
        )
        LazyVStack(alignment: .leading, spacing: 4) {
          ForEach(needsYou.prefix(6)) { run in
            RunRow(run: run) { store.select(.forge(.runs)); store.openRun(run.id) }
          }
        }
        TwinComposerCard()
      }

      if !running.isEmpty {
        SectionHeader(title: "Running now", subtitle: "Work in flight. Open one to watch it live.")
        LazyVStack(alignment: .leading, spacing: 4) {
          ForEach(running.prefix(6)) { run in
            RunRow(run: run) { store.select(.forge(.runs)); store.openRun(run.id) }
          }
        }
      }

      SectionHeader(title: "Jump back in", subtitle: "Open the surface that owns the next decision.")
      jumpBackInGrid
    }
  }

  private var offlineHomePanel: some View {
    VStack(alignment: .leading, spacing: 18) {
      ConnectHeroCard {
        Task { await store.refresh() }
      }

      SectionHeader(
        title: "Explore while you connect",
        subtitle: "These surfaces are part of MDx now. Live work, decisions, and proof fill in once the local route server is reachable."
      )
      jumpBackInGrid
    }
  }

  private var jumpBackInGrid: some View {
    LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 12)], spacing: 12) {
      SurfaceJumpCard(route: .forge(.runs), detail: "Watch work move from intent to proof.")
      SurfaceJumpCard(route: .twin, detail: "Reason over evidence and tradeoffs.")
      SurfaceJumpCard(route: .pages, detail: "Inspect context and published knowledge.")
      SurfaceJumpCard(route: .message, detail: "Coordinate approvals and agent handoffs.")
    }
  }

  private var overviewPanel: some View {
    VStack(alignment: .leading, spacing: 18) {
      // "Start a build" is primary only when Forge has no review, recovery, or
      // live trail that deserves the operator's attention first.
      ForgeNextMoveCard(move: forgeNextMove, perform: performForgeNextMove)

      if isConnected {
        metricGrid
      }

      routeList(title: "Forge context", cards: cards(for: .overview))
    }
  }

  private var workPanel: some View {
    VStack(alignment: .leading, spacing: 18) {
      SectionHeader(title: "Work", subtitle: "Repos, recipes, missions, and loops you can ask Forge to reason about.")
      surfaceList(items: filteredSurfaceItems(store.snapshot.workbenchItems), emptyText: "Connect a repo or recipe and it will appear here.")
      routeList(title: "Work sources", cards: cards(for: .work))
    }
  }

  @ViewBuilder
  private var runRoomPanel: some View {
    if let run = store.selectedRun {
      RunDetailView(run: run)
    } else {
      runListPanel
    }
  }

  private var runListPanel: some View {
    VStack(alignment: .leading, spacing: 18) {
      ForgeTrailMapView(
        scale: "Run",
        title: "One agent, one branch",
        detail: "Open any build for its translated receipts, diff, checks, and review handoff.",
        active: runTrailStage
      )
      let runs = RunListGrouping.representatives(
        visibleRuns: filteredRuns,
        allRuns: store.sortedRuns,
        groups: store.parallelExecutionGroups
      )
      if runs.isEmpty {
        EmptyStateView(text: isConnected ? "No runs match this search." : "Connect local MDx to see runs.")
      } else {
        let running = runs.filter(\.isRunning)
        let reviewIDs = Set(store.reviewReadyRuns.map(\.id))
        let reviewReady = runs.filter { !$0.isRunning && reviewIDs.contains($0.id) }
        let stopped = runs.filter { run in
          !run.isRunning && !reviewIDs.contains(run.id) && forgeRunTone(run.status) == .locked
        }
        let recorded = runs.filter { run in
          !run.isRunning && !reviewIDs.contains(run.id) && forgeRunTone(run.status) != .locked
        }
        let listOrder = running + reviewReady + stopped + (showRecordedRuns ? recorded : [])
        Group {
          if !running.isEmpty {
            runGroup(title: "Working now", runs: running)
          }
          if !reviewReady.isEmpty {
            runGroup(title: "Ready for your call", runs: reviewReady)
          }
          if !stopped.isEmpty {
            runGroup(title: "Stopped", runs: stopped)
          }
          if !recorded.isEmpty {
            recordedGroup(recorded)
          }
        }
        // Arrow keys walk the list; Return opens. Click once (or Tab) to give
        // the list focus, then keep hands on the keyboard.
        .focusable()
        .focusEffectDisabled()
        .onKeyPress(.downArrow) { moveRunHighlight(1, within: listOrder); return .handled }
        .onKeyPress(.upArrow) { moveRunHighlight(-1, within: listOrder); return .handled }
        .onKeyPress(.return) {
          guard let id = highlightedRunID else { return .ignored }
          store.openRun(id)
          return .handled
        }
      }
    }
  }

  private var runTrailStage: ForgeTrailStage {
    if !store.reviewReadyRuns.isEmpty { return .review }
    if !store.activeRuns.isEmpty { return .build }
    if store.sortedRuns.contains(where: { $0.checksPassed > 0 || $0.checksFailed > 0 }) { return .prove }
    return .plan
  }

  private func moveRunHighlight(_ delta: Int, within runs: [ForgeRun]) {
    guard !runs.isEmpty else { return }
    let ids = runs.map(\.id)
    let current = highlightedRunID.flatMap { ids.firstIndex(of: $0) }
    let next = ((current ?? (delta > 0 ? -1 : 0)) + delta + ids.count) % ids.count
    highlightedRunID = ids[next]
  }

  private func runGroup(title: String, runs: [ForgeRun]) -> some View {
    VStack(alignment: .leading, spacing: 6) {
      Text(title)
        .font(.caption.weight(.semibold))
        .foregroundStyle(.secondary)
        .padding(.horizontal, 4)
      LazyVStack(alignment: .leading, spacing: 4) {
        ForEach(runs) { run in
          RunRow(
            run: run,
            parallelGroup: store.parallelGroup(containing: run.id),
            highlighted: run.id == highlightedRunID
          ) {
            store.openRun(run.id)
          }
        }
      }
    }
  }

  // System records (staged profiles, recorded receipts) can outnumber real
  // runs and drown them. Keep them one quiet, expandable summary row.
  private func recordedGroup(_ runs: [ForgeRun]) -> some View {
    VStack(alignment: .leading, spacing: 6) {
      Button {
        withAnimation(.easeOut(duration: 0.15)) { showRecordedRuns.toggle() }
      } label: {
        HStack(spacing: 8) {
          Image(systemName: showRecordedRuns ? "chevron.down" : "chevron.right")
            .font(.caption2)
            .foregroundStyle(.tertiary)
          Text("\(runs.count) system record\(runs.count == 1 ? "" : "s")")
            .font(.caption.weight(.medium))
            .foregroundStyle(.secondary)
          Text("kept out of the live queue")
            .font(.caption)
            .foregroundStyle(.tertiary)
          Spacer(minLength: 0)
        }
        .padding(.vertical, 7)
        .padding(.horizontal, 8)
        .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      if showRecordedRuns {
        LazyVStack(alignment: .leading, spacing: 4) {
          ForEach(runs) { run in
            RunRow(
              run: run,
              parallelGroup: store.parallelGroup(containing: run.id),
              highlighted: run.id == highlightedRunID
            ) {
              store.openRun(run.id)
            }
          }
        }
      }
    }
  }

  private var filteredRuns: [ForgeRun] {
    filter(store.sortedRuns) { run in
      [run.title, run.intent, run.id, run.status, run.repo, run.model, run.branch]
    }
  }

  private var reviewPanel: some View {
    VStack(alignment: .leading, spacing: 18) {
      SectionHeader(title: "Review", subtitle: "Shipping stays a human decision with proof beside it.")
      LazyVGrid(columns: [GridItem(.adaptive(minimum: 240), spacing: 12)], spacing: 12) {
        ForEach(store.snapshot.reviewArtifacts) { item in
          ReviewArtifactTile(item: item)
        }
      }
      // The triage queue: every run waiting on human judgment, cross-run,
      // review-first (review is the bottleneck, so it gets the front slot).
      if !store.reviewReadyRuns.isEmpty {
        SectionHeader(
          title: "Waiting for your call",
          subtitle: "\(store.reviewReadyRuns.count) run\(store.reviewReadyRuns.count == 1 ? "" : "s") with proof ready. Open one, then A ships, R asks for changes."
        )
        LazyVStack(alignment: .leading, spacing: 4) {
          ForEach(store.reviewReadyRuns) { run in
            RunRow(run: run) { store.select(.forge(.runs)); store.openRun(run.id) }
          }
        }
      }

      GovernedActionPanel(
        selectedAction: $selectedAction,
        draft: bindableStore.actionDraft,
        result: store.actionResult,
        inFlight: store.actionInFlight,
        preferredActions: [.humanSignoff, .sourceHostReadiness, .prHandoff],
        title: "Record your call",
        submit: { action in Task { await store.submit(action) } }
      )
      routeList(title: "Review sources", cards: cards(for: .review))
    }
  }

  private var fleetPanel: some View {
    VStack(alignment: .leading, spacing: 18) {
      ForgeTrailMapView(
        scale: "Fleet",
        title: "Parallel lanes, one integrated branch",
        detail: "Each lane keeps its own write scope and proof before integration returns one change.",
        active: store.snapshot.fleetRunCount > 0 ? .build : .plan
      )
      SectionHeader(
        title: "Fleets",
        subtitle: "\(store.snapshot.fleetPlanCount) plans. \(store.snapshot.capacityWorkers) workers available, \(store.snapshot.capacityQueueDepth) queued."
      )

      if filteredFleetPlans.isEmpty {
        EmptyStateView(text: "Describe larger work and Forge will propose a split before any build spend.")
      } else {
        if let outcome = store.runActionResult, outcome.title == "Ratify fleet" || outcome.title == "Start fleet" {
          RunActionBanner(outcome: outcome)
        }
        LazyVStack(alignment: .leading, spacing: 10) {
          ForEach(filteredFleetPlans) { plan in
            FleetPlanRow(
              plan: plan,
              started: store.snapshot.startedFleetIDs.contains(plan.id),
              inFlight: store.buildStartInFlight,
              ratify: { Task { await store.ratifyFleetPlan(plan) } },
              start: { Task { await store.startFleetRun(plan) } },
              recover: { store.prepareMissionFromFleetPlan(plan) }
            )
          }
        }
      }

      if !store.snapshot.fleetRuns.isEmpty {
        VStack(alignment: .leading, spacing: 10) {
          Text("Live fleet work")
            .font(.headline)
          ForEach(store.snapshot.fleetRuns) { run in
            FleetRunRow(
              run: run,
              openRun: { runID in store.openRun(runID) },
              recover: store.snapshot.fleetPlans.first(where: { $0.id == run.id }).map { plan in
                { store.prepareMissionFromFleetPlan(plan) }
              }
            )
          }
        }
      }

      surfaceList(items: filteredSurfaceItems(store.snapshot.fleetItems), emptyText: "No fleet activity is available yet.")
      routeList(title: "Fleet sources", cards: cards(for: .fleets))
    }
  }

  private var machinePanel: some View {
    VStack(alignment: .leading, spacing: 18) {
      SectionHeader(
        title: "Machines",
        subtitle: "\(store.snapshot.selectedRunner) is the current recommendation. \(store.snapshot.fallbackRunner) stays available as fallback evidence."
      )

      HStack(alignment: .center, spacing: 10) {
        if store.machinePreflightInFlight {
          ProgressView()
            .controlSize(.small)
        } else {
          Image(systemName: "desktopcomputer.and.macbook")
            .foregroundStyle(Color.accentColor)
        }
        Text(store.machinePreflightSummary ?? "Check locally installed harnesses when you need current runtime truth. The probe can take up to a minute.")
          .font(.callout)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
        Spacer(minLength: 8)
        Button {
          Task { await store.checkMachineRuntimes() }
        } label: {
          Label(store.machinePreflightInFlight ? "Checking" : "Check this Mac", systemImage: "waveform.path.ecg")
        }
        .buttonStyle(.bordered)
        .controlSize(.small)
        .disabled(store.machinePreflightInFlight)
      }
      .padding(12)
      .background(
        RoundedRectangle(cornerRadius: 10, style: .continuous)
          .fill(Color.accentColor.opacity(0.07))
      )

      if !store.snapshot.modelRoles.isEmpty {
        ModelRoleStrip(roles: store.snapshot.modelRoles)
      }

      let runners = filteredRunners
      if runners.isEmpty {
        EmptyStateView(text: "Runner profiles are not available from the local route.")
      } else {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 260), spacing: 12)], spacing: 12) {
          ForEach(runners) { runner in
            Button {
              store.machineActionResult = nil
              selectedMachineRunner = runner
            } label: {
              RunnerTile(runner: runner)
            }
            .buttonStyle(.plain)
          }
        }
      }

      routeList(title: "Machine sources", cards: cards(for: .machines))
    }
  }

  private var controlsPanel: some View {
    VStack(alignment: .leading, spacing: 18) {
      SectionHeader(title: "Governed actions", subtitle: "Route gates stay visible before anything writes.")
      GovernedActionPanel(
        selectedAction: $selectedAction,
        draft: bindableStore.actionDraft,
        result: store.actionResult,
        inFlight: store.actionInFlight,
        preferredActions: GovernedActionKind.allCases,
        submit: { action in Task { await store.submit(action) } }
      )
      surfaceList(items: filteredSurfaceItems(store.snapshot.controlItems), emptyText: "No controls are reporting yet.")
      routeList(title: "Control sources", cards: cards(for: .controls))
    }
  }

  private var evidencePanel: some View {
    VStack(alignment: .leading, spacing: 18) {
      SectionHeader(
        title: "Evidence",
        subtitle: "\(store.snapshot.receiptCount) receipts and \(store.snapshot.policyDecisionCount) policy decisions reported by Forge routes."
      )

      ForEach(filteredEvidenceItems) { item in
        EvidenceRow(item: item)
      }

      routeList(title: "Evidence sources", cards: cards(for: .evidence))
    }
  }

  private var hostPanel: some View {
    VStack(alignment: .leading, spacing: 18) {
      SectionHeader(title: connectionName, subtitle: store.snapshot.baseURL.absoluteString)

      VStack(alignment: .leading, spacing: 10) {
        InfoRow(label: "Connection", value: store.snapshot.connectionStatus.displayName)
        InfoRow(label: "Boundary", value: store.snapshot.boundary)
        InfoRow(label: "Authority", value: authorityText)
      }
      .padding(14)
      .mdxGlassSurface()

      routeList(title: "All local sources", cards: store.snapshot.routeCards)

      HostProjectPanel(projects: store.hostProjects) { path in
        store.addHostProject(path: path)
      } remove: { project in
        store.removeHostProject(project)
      }

      PackagingPanel()
    }
  }

  private var youPanel: some View {
    VStack(alignment: .leading, spacing: 18) {
      SectionHeader(title: "You", subtitle: "Who you are here, what you are cleared to do, and this app's connections.")

      if let session = store.identitySession {
        IdentityCard(
          session: session,
          displayName: identityDisplayName,
          workspaceName: isHosted ? "Personal beta workspace" : "Local workspace"
        )
      }
      if let clearance = store.clearance {
        ClearanceCard(clearance: clearance)
      }

      Button {
        store.select(.memory)
      } label: {
        HStack(spacing: 12) {
          Image(systemName: "brain.head.profile")
            .font(.title3)
            .foregroundStyle(Color.accentColor)
          VStack(alignment: .leading, spacing: 3) {
            Text("Memory").font(.headline)
            Text("Review what MDx can responsibly recall and retire anything that is outdated.")
              .font(.callout)
              .foregroundStyle(.secondary)
          }
          Spacer()
          Image(systemName: "chevron.right").foregroundStyle(.tertiary)
        }
        .padding(14)
        .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .mdxGlassSurface(interactive: true)

      VStack(alignment: .leading, spacing: 12) {
        if isHosted, !auth.email.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
          InfoRow(label: "Signed in as", value: auth.email)
        }
        InfoRow(label: connectionName, value: store.snapshot.baseURL.absoluteString)
        InfoRow(label: "Connection", value: store.snapshot.connectionStatus.displayName)
        Button {
          store.showSettings()
        } label: {
          Label("Open settings", systemImage: "gearshape")
        }
        .mdxPrimaryButtonStyle()
      }
      .padding(14)
      .mdxGlassSurface(interactive: true)

      HostProjectPanel(projects: store.hostProjects) { path in
        store.addHostProject(path: path)
      } remove: { project in
        store.removeHostProject(project)
      }

      DiagnosticsCard()

      PackagingPanel()
    }
    .task { await store.loadYou() }
  }

  private func flagshipPanel(_ route: AppRoute) -> some View {
    VStack(alignment: .leading, spacing: 18) {
      SectionHeader(title: route.title, subtitle: flagshipSubtitle(for: route))

      FlagshipHeroCard(route: route)

      SectionHeader(title: "What you can do here", subtitle: "This surface is part of the native MDx shell now. Deep routes can fill in without changing the app spine.")
      LazyVGrid(columns: [GridItem(.adaptive(minimum: 230), spacing: 12)], spacing: 12) {
        ForEach(flagshipPreviewItems(for: route)) { item in
          SurfacePreviewTile(item: item)
        }
      }
    }
  }

  private func flagshipSubtitle(for route: AppRoute) -> String {
    switch route {
    case .twin:
      return "A companion that can reason from evidence before it asks for authority."
    case .pages:
      return "Context, documents, and typed knowledge with origins beside them."
    case .message:
      return "Where people and agents coordinate: approvals, activity, and handoffs."
    case .memory:
      return "Approved lessons, reviewable team memory, and model evidence without an opaque brain."
    case .marketplace:
      return "Capabilities, tools, and providers behind audited profiles."
    default:
      return route.subtitle
    }
  }

  private func flagshipPreviewItems(for route: AppRoute) -> [SurfacePreviewItem] {
    switch route {
    case .twin:
      return [
        SurfacePreviewItem(id: "ask", title: "Ask from evidence", detail: "Use the Home composer pattern here first, then promote answers into governed action cards.", status: "Preview", systemImage: "sparkles"),
        SurfacePreviewItem(id: "delegate", title: "Delegate later", detail: "Delegation waits for policy, provider, memory, and receipt rails.", status: "Held", systemImage: "lock")
      ]
    case .pages:
      return [
        SurfacePreviewItem(id: "graph", title: "Context graph", detail: "Pages will show typed objects with derived, asserted, and external origins.", status: "Preview", systemImage: "point.3.connected.trianglepath.dotted"),
        SurfacePreviewItem(id: "publish", title: "Publish or assert", detail: "Publication stays governed until the route can record proof.", status: "Held", systemImage: "square.and.pencil")
      ]
    case .message:
      return [
        SurfacePreviewItem(id: "timeline", title: "Timeline", detail: "Typed envelopes and agent lanes will make coordination readable.", status: "Preview", systemImage: "list.bullet.rectangle"),
        SurfacePreviewItem(id: "approve", title: "Approve in thread", detail: "Approvals become action cards only when receipt rails are wired.", status: "Held", systemImage: "checkmark.seal")
      ]
    case .memory:
      return [
        SurfacePreviewItem(id: "lessons", title: "Approved lessons", detail: "Only human-promoted lessons can guide future work.", status: "Governed", systemImage: "checkmark.seal"),
        SurfacePreviewItem(id: "receipts", title: "Receipt-backed recall", detail: "Team memory and model evidence preserve their source and review boundary.", status: "Traceable", systemImage: "doc.text.magnifyingglass")
      ]
    case .marketplace:
      return [
        SurfacePreviewItem(id: "catalog", title: "Capability catalog", detail: "Skills, tools, and providers appear as governed profiles.", status: "Preview", systemImage: "shippingbox"),
        SurfacePreviewItem(id: "add", title: "Add capability", detail: "Install and provider turn-on stay behind audited records.", status: "Held", systemImage: "plus.app")
      ]
    default:
      return []
    }
  }

  private func cards(for lane: RouteLane) -> [RouteCard] {
    filter(store.snapshot.routeCards.filter { $0.lane == lane }) { card in
      [card.title, card.path, card.detail, card.metric]
    }
  }

  private var filteredWorkItems: [WorkItem] {
    filter(store.snapshot.workItems) { item in
      [item.title, item.subtitle, item.status, item.stage]
    }
  }

  private var filteredRunners: [MachineRunner] {
    filter(store.snapshot.runners) { runner in
      [runner.name, runner.kind, runner.model, runner.status, runner.readinessLine, runner.protocolLabel, runner.compatibilityLine]
    }
  }

  private var filteredFleetPlans: [FleetPlan] {
    filter(store.snapshot.fleetPlans) { plan in
      [plan.spec, plan.phase, plan.status, plan.repo, plan.languagePack, plan.plannerModel, plan.reviewLine, plan.reviewConcerns, plan.builderMix]
    }
  }

  private var filteredEvidenceItems: [EvidenceItem] {
    filter(store.snapshot.evidenceItems) { item in
      [item.title, item.detail, item.route]
    }
  }

  private func filteredSurfaceItems(_ items: [SurfaceItem]) -> [SurfaceItem] {
    filter(items) { item in
      [item.title, item.subtitle, item.status, item.detail, item.route]
    }
  }

  private func filter<T>(_ values: [T], terms: (T) -> [String]) -> [T] {
    let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !query.isEmpty else {
      return values
    }
    return values.filter { value in
      terms(value).contains { term in
        term.localizedCaseInsensitiveContains(query)
      }
    }
  }

  private func routeList(title: String, cards: [RouteCard]) -> some View {
    VStack(alignment: .leading, spacing: 12) {
      SectionHeader(title: title, subtitle: "Select a source when you need the receipt, boundary, or exact route.")

      if cards.isEmpty {
        EmptyStateView(text: "No local route cards are available for this lane.")
      } else {
        LazyVStack(alignment: .leading, spacing: 10) {
          ForEach(cards) { card in
            Button {
              store.selectRoute(card)
            } label: {
              RouteRow(card: card, selected: store.selectedRoute?.id == card.id)
            }
            .buttonStyle(.plain)
          }
        }
      }
    }
  }

  private func surfaceList(items: [SurfaceItem], emptyText: String) -> some View {
    VStack(alignment: .leading, spacing: 10) {
      if items.isEmpty {
        EmptyStateView(text: emptyText)
      } else {
        ForEach(items) { item in
          SurfaceRow(item: item)
        }
      }
    }
  }
}


struct PackagingPanel: View {
  private var profile: MacDistributionProfile { Bundle.main.macDistributionProfile }

  private var items: [PackageReadinessItem] {
    if profile.isCanary {
      return [
        PackageReadinessItem(id: "bundle", title: "App bundle", status: "Ready", detail: "This is the downloadable MDx for Mac canary bundle."),
        PackageReadinessItem(id: "identity", title: "Signing identity", status: "Developer ID", detail: "The release workflow signs this channel with the MDx Developer ID identity and hardened runtime."),
        PackageReadinessItem(id: "notary", title: "Notarization", status: "Apple verified", detail: "Apple acceptance, stapling, and Gatekeeper checks complete before this channel is published."),
        PackageReadinessItem(id: "release", title: "Updates", status: "Private", detail: "MDx checks the private release manifest and opens the secure download for newer builds.")
      ]
    }
    return [
      PackageReadinessItem(id: "bundle", title: "App bundle", status: "Ready", detail: "The run script stages dist/MDx.app from the SwiftPM build."),
      PackageReadinessItem(id: "identity", title: "Signing identity", status: "Local only", detail: "Distribution signing waits for a Developer ID identity and hardened runtime decision."),
      PackageReadinessItem(id: "notary", title: "Notarization", status: "Not started", detail: "Notarization is not claimed for this local development build."),
      PackageReadinessItem(id: "release", title: "Release script", status: "Available", detail: "Use script/package_release.sh to build and inspect the app artifact.")
    ]
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      SectionHeader(title: "Packaging", subtitle: profile.packagingSubtitle)
      LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 12)], spacing: 12) {
        ForEach(items) { item in
          VStack(alignment: .leading, spacing: 8) {
            HStack {
              Text(item.title)
                .font(.headline)
              Spacer()
              StatusPill(
                status: item.status,
                tone: ["Ready", "Developer ID", "Apple verified"].contains(item.status) ? .positive : .neutral
              )
            }
            Text(item.detail)
              .font(.callout)
              .foregroundStyle(.secondary)
              .lineLimit(3)
          }
          .padding(14)
          .mdxGlassSurface()
        }
      }
    }
  }
}
