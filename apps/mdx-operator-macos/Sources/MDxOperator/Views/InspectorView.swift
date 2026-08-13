import SwiftUI

struct InspectorView: View {
  @Environment(OperatorStore.self) private var store

  var body: some View {
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

  private var content: some View {
    VStack(alignment: .leading, spacing: 20) {
      Text("Context")
        .font(.title2.weight(.semibold))
      // (quick actions inserted after the context group below)

      inspectorGroup(title: "Next move", value: nextMoveText)
      // The generic host/boundary context only applies when nothing specific is
      // selected. With a run selected, the inspector is about that run.
      if inspectedRun == nil {
        inspectorGroup(title: "Boundary", value: boundaryText)
        inspectorGroup(title: "Host", value: store.snapshot.baseURL.absoluteString)
        if let loadedAt = store.snapshot.loadedAt {
          inspectorGroup(title: "Last refresh", value: OperatorFormatters.time.string(from: loadedAt))
        }
      }

      Divider()

      quickActionsSection

      Divider()

      if let run = inspectedRun {
        runInspector(run)
      } else if store.selectedAppRoute.isForge, let route = store.selectedRoute {
        routeInspector(route)
      } else {
        surfaceInspector
      }

      Divider()

      Text("Still held")
        .font(.headline)

      Text(authoritySummary)
        .font(.callout)
        .foregroundStyle(.secondary)

      ForEach(Array(blockedAuthorityItems.prefix(3))) { item in
        AuthorityRow(item: item)
      }

      if blockedAuthorityItems.count > 3 {
        DisclosureGroup("Show all locked actions") {
          VStack(alignment: .leading, spacing: 12) {
            ForEach(Array(blockedAuthorityItems.dropFirst(3))) { item in
              AuthorityRow(item: item)
            }
          }
          .padding(.top, 8)
        }
      }

      Divider()

      Text("Proof")
        .font(.headline)

      if let run = inspectedRun {
        runProof(run)
      } else if receiptCards.isEmpty {
        Text("Connect local MDx to show the proof behind this surface.")
          .font(.callout)
          .foregroundStyle(.secondary)
      } else {
        ForEach(Array(receiptCards.prefix(6))) { card in
          VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .top) {
              Image(systemName: card.status == .ok ? "checkmark.seal.fill" : "seal")
                .foregroundStyle(card.status == .ok ? .green : .secondary)
              Text(card.title)
                .font(.subheadline.weight(.medium))
            }
            // The raw route path stays reachable, but tucked behind the same
            // "Route details" disclosure the route inspector uses, not bare.
            DisclosureGroup("Route details") {
              Text(card.path)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
                .lineLimit(1)
                .padding(.top, 4)
            }
            .font(.caption)
          }
        }
      }
    }
    .padding(20)
    .frame(maxWidth: .infinity, alignment: .leading)
    .mdxGlassSurface()
  }

  private var blockedAuthorityItems: [AuthorityItem] {
    store.snapshot.authorityItems.filter { !$0.allowed }
  }

  private var receiptCards: [RouteCard] {
    store.snapshot.routeCards.filter(\.receiptBacked)
  }

  private var inspectedRun: ForgeRun? {
    store.selectedAppRoute == .forge(.runs) ? store.selectedRun : nil
  }

  /// Derive the calm Forge worker context line from the current runs list.
  /// Empty-state idle copy is only used when zero runs are running.
  private func forgeWorkerContextText(from runs: [ForgeRun]) -> String {
    let runningCount = runs.filter(\.isRunning).count
    if runningCount == 0 {
      return "No Forge worker job is in flight"
    }
    if runningCount == 1 {
      return "A Forge worker job is in flight"
    }
    return "\(runningCount) Forge worker jobs are in flight"
  }

  private var nextMoveText: String {
    if let run = inspectedRun {
      if run.isRunning { return "Watch it work. You can steer or stop from the action bar." }
      if run.isReviewReady {
        return "Proof is ready for your call. Open Review, then ship or request changes."
      }
      if run.hasBranch { return "Review the diff, then decide: ship it or ask for a revision." }
      return run.finalLine.isEmpty ? "This run finished without a branch to review." : run.finalLine
    }
    let runningCount = store.forgeRuns.filter(\.isRunning).count
    if runningCount == 1 {
      return "A Forge run is in flight. Open it from Runs to watch or steer."
    }
    if runningCount > 1 {
      return "\(runningCount) Forge runs are in flight. Open one from Runs to watch or steer."
    }
    if store.reviewReadyCount == 1 {
      return "One finished run is ready for your call. Open it from Review or Runs."
    }
    if store.reviewReadyCount > 1 {
      return "\(store.reviewReadyCount) finished runs are ready for your call. Open one from Review or Runs."
    }
    return safeNextMoveText
  }

  private func runInspector(_ run: ForgeRun) -> some View {
    VStack(alignment: .leading, spacing: 10) {
      Text("This run")
        .font(.headline)
      inspectorGroup(title: "Status", value: run.displayStatus)
      if !run.stage.isEmpty { inspectorGroup(title: "Stage", value: run.displayStage) }
      if !run.status.isEmpty { inspectorGroup(title: "Status code", value: run.status) }
      if run.hasBranch { inspectorGroup(title: "Branch", value: run.branch) }
      if !run.harness.isEmpty { inspectorGroup(title: "Harness", value: run.harness) }
      if !run.model.isEmpty { inspectorGroup(title: "Model", value: run.model) }
      if !run.repo.isEmpty { inspectorGroup(title: "Repo", value: run.repo) }
      if run.checksPassed + run.checksFailed > 0 {
        inspectorGroup(title: "Checks", value: "\(run.checksPassed) of \(run.checksPassed + run.checksFailed) passed")
      }
      if run.diffFileCount > 0 {
        inspectorGroup(title: "Files changed", value: "\(run.diffFileCount) file\(run.diffFileCount == 1 ? "" : "s")")
      }
      if run.turns > 0 {
        let calls = run.toolCalls > 0 ? ", \(run.toolCalls) tool call\(run.toolCalls == 1 ? "" : "s")" : ""
        inspectorGroup(title: "Turns", value: "\(run.turns) turn\(run.turns == 1 ? "" : "s")\(calls)")
      }
      if let mission = store.missionLinked(to: run.id) {
        inspectorGroup(title: "Mission", value: mission.displayName)
        if !mission.stateLabel.isEmpty {
          inspectorGroup(title: "Mission state", value: mission.stateLabel)
        }
      }
      if let group = store.parallelGroup(containing: run.id), !parallelSummary(group).isEmpty {
        inspectorGroup(title: "Parallel lanes", value: parallelSummary(group))
      }
      if !run.workClassification.plainSentence.isEmpty {
        inspectorGroup(title: "How Forge read this", value: howForgeReadThis(run))
      }
      inspectorGroup(title: "Review readiness", value: reviewReadinessText(for: run))
      HStack {
        StatusPill(status: run.isRunning ? "Working" : "Finished", tone: run.isRunning ? .neutral : .positive)
        if run.isReviewReady {
          StatusPill(status: "Ready for your call", tone: .positive)
        }
      }
    }
  }

  private func howForgeReadThis(_ run: ForgeRun) -> String {
    var parts: [String] = [run.workClassification.plainSentence]
    let rationale = run.workClassification.rationale.trimmingCharacters(in: .whitespaces)
    if !rationale.isEmpty {
      parts.append(rationale.hasSuffix(".") ? rationale : rationale + ".")
    }
    if !run.languageTaskAlignment.detailLine.isEmpty {
      parts.append("Proof lane: \(run.languageTaskAlignment.detailLine).")
    }
    return parts.joined(separator: " ")
  }

  private func reviewReadinessText(for run: ForgeRun) -> String {
    if run.isRunning {
      return "Still working. Review opens when the run finishes with proof."
    }
    if run.isReviewReady {
      if run.canShip {
        return "Proof is attached and ship or review is open. Shipping stays your recorded decision."
      }
      return "Proof is ready for your call. Open Review, then ship or request changes."
    }
    if run.hasProofTrouble {
      return run.proofTroubleDetail
    }
    if run.hasBranch {
      return "Branch is ready to inspect. Open Diff or Review when you want the full packet."
    }
    return "No review controls are open on this run yet."
  }

  private func parallelSummary(_ group: ForgeParallelExecutionGroup) -> String {
    if group.isSettled {
      return group.completedSummaryLine
    }
    if !group.progressLine.isEmpty {
      return "\(group.laneLabel): \(group.progressLine)"
    }
    return group.selectionLine
  }

  private func runProof(_ run: ForgeRun) -> some View {
    let source = run.isRunning && !store.liveEvents.isEmpty ? store.liveEvents : run.events
    let receipts = source.filter(\.hasReceipt)
    return VStack(alignment: .leading, spacing: 8) {
      if receipts.isEmpty {
        Text("Receipts appear here as the run records each step.")
          .font(.callout)
          .foregroundStyle(.secondary)
      } else {
        Text("\(receipts.count) receipts recorded")
          .font(.caption)
          .foregroundStyle(.secondary)
        ForEach(receipts.suffix(6).reversed()) { event in
          HStack(alignment: .top, spacing: 8) {
            Image(systemName: "checkmark.seal.fill")
              .font(.caption)
              .foregroundStyle(.green)
            VStack(alignment: .leading, spacing: 1) {
              Text(event.summary)
                .font(.caption)
                .lineLimit(1)
              Text(event.receiptID)
                .font(.caption2.monospaced())
                .foregroundStyle(.tertiary)
                .lineLimit(1)
            }
          }
        }
      }
    }
  }

  private var safeNextMoveText: String {
    if store.snapshot.safeNextMove.localizedCaseInsensitiveContains("Open the operator workspace") {
      return "Forge works read-only until you approve writes. Review what it can touch before shipping."
    }
    return store.snapshot.safeNextMove
  }

  private var boundaryText: String {
    let raw = store.snapshot.boundary
    if raw.localizedCaseInsensitiveContains("read-only runtime projection")
      || raw.localizedCaseInsensitiveContains("stay closed here") {
      return "MDx is read-only right now. Starting workers, shipping to CI, and production writes stay locked until you open them."
    }
    return raw
  }

  private var authoritySummary: String {
    let count = blockedAuthorityItems.count
    if count == 0 {
      return "Nothing is locked right now."
    }
    let noun = count == 1 ? "action stays" : "actions stay"
    return "\(count) \(noun) locked until you approve them."
  }

  private var surfaceInspector: some View {
    VStack(alignment: .leading, spacing: 10) {
      Text("Selected surface")
        .font(.headline)
      inspectorGroup(title: "Name", value: store.selectedAppRoute.title)
      inspectorGroup(title: "Primary verb", value: store.selectedAppRoute.primaryVerb)
      inspectorGroup(title: "What it means", value: surfaceContextText)
      HStack {
        StatusPill(status: surfaceStatusLabel, tone: surfaceStatusTone)
      }
    }
  }

  private var surfaceStatusLabel: String {
    switch store.selectedAppRoute {
    case .home: return "Situation"
    case .twin: return "Companion"
    default: return "Preview"
    }
  }

  private var surfaceStatusTone: StatusTone {
    store.selectedAppRoute == .twin ? .positive : .neutral
  }

  private var surfaceContextText: String {
    switch store.selectedAppRoute {
    case .home:
      return "Home brings live work, decisions, recent proof, and the Twin composer into one place."
    case .twin:
      return "Twin can reason from local evidence now. Delegation waits for policy, provider, memory, authority, and receipt rails."
    case .pages:
      return "Pages will show typed knowledge with origins beside it, so context stays inspectable."
    case .message:
      return "Message will coordinate people, agents, approvals, and work envelopes without claiming delivery before proof."
    case .memory:
      return "Memory keeps approved lessons, team-reviewed memory, and model evidence separate so every recalled claim retains its boundary."
    case .marketplace:
      return "Marketplace will add capabilities through governed profiles, not raw provider switches."
    case .you:
      return "You owns identity, authority, connections, appearance, and local app settings."
    case .forge:
      return "Forge work stays tied to local route proof and authority boundaries."
    }
  }

  private func routeInspector(_ route: RouteCard) -> some View {
    VStack(alignment: .leading, spacing: 10) {
      Text("Selected source")
        .font(.headline)
      inspectorGroup(title: "Name", value: route.title)
      inspectorGroup(title: "Status", value: route.status.displayName)
      inspectorGroup(title: "What it says", value: routeDetailText(route))
      HStack {
        StatusPill(status: route.readOnly ? "Read only" : "Can make changes", tone: route.readOnly ? .neutral : .locked)
        if route.receiptBacked {
          StatusPill(status: "Proven", tone: .positive)
        }
      }
      DisclosureGroup("Route details") {
        Text(route.path)
          .font(.caption.monospaced())
          .foregroundStyle(.secondary)
          .textSelection(.enabled)
          .padding(.top, 6)
      }
    }
  }

  private func routeDetailText(_ route: RouteCard) -> String {
    let runningCount = store.forgeRuns.filter(\.isRunning).count
    if runningCount > 0 && route.detail.localizedCaseInsensitiveContains("No Forge worker job is in flight") {
      return forgeWorkerContextText(from: store.forgeRuns)
    }
    return route.detail
  }

  private func inspectorGroup(title: String, value: String) -> some View {
    VStack(alignment: .leading, spacing: 4) {
      Text(title)
        .font(.caption.weight(.semibold))
        .foregroundStyle(.secondary)
      Text(value)
        .font(.callout)
        .textSelection(.enabled)
    }
  }

  // MARK: - Quick actions (Codex Environment-panel style)

  private var quickActionsSection: some View {
    VStack(alignment: .leading, spacing: 8) {
      Text("Quick actions")
        .font(.headline)
      ForEach(quickActions) { action in
        Button(action: action.perform) {
          HStack(spacing: 10) {
            Image(systemName: action.icon)
              .font(.callout)
              .frame(width: 20)
              .foregroundStyle(action.tint)
            Text(action.title)
              .font(.callout)
              .foregroundStyle(.primary)
            Spacer(minLength: 6)
            if let shortcut = action.shortcut {
              Text(shortcut).font(.caption.monospaced()).foregroundStyle(.tertiary)
            } else {
              Image(systemName: "chevron.right").font(.caption2).foregroundStyle(.tertiary)
            }
          }
          .padding(.horizontal, 12)
          .padding(.vertical, 9)
          .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(.plain)
        .mdxGlassSurface(interactive: true)
      }
    }
  }

  private var quickActions: [InspectorAction] {
    var actions: [InspectorAction] = []
    switch store.selectedAppRoute {
    case .home, .forge:
      actions.append(InspectorAction(title: "Start a build", icon: "hammer.fill", tint: .accentColor, shortcut: "⌘N") { store.startBuildFlow() })
    case .twin:
      actions.append(InspectorAction(title: "New conversation", icon: "square.and.pencil", tint: .accentColor) { store.newTwinConversation() })
    case .pages:
      actions.append(InspectorAction(title: "New page", icon: "square.and.pencil", tint: .accentColor) { store.startNewPage() })
    case .message:
      actions.append(InspectorAction(title: "Open Needs you", icon: "tray.full", tint: .accentColor) {
        store.select(.message); store.messageLane = .needsYou
      })
    case .memory:
      actions.append(InspectorAction(title: "Review lessons", icon: "checkmark.seal", tint: .accentColor) {
        store.select(.memory); store.selectedMemoryRail = .learned
      })
    case .marketplace:
      actions.append(InspectorAction(title: "Browse capabilities", icon: "bag", tint: .accentColor) { store.select(.marketplace) })
    case .you:
      actions.append(InspectorAction(title: "Open settings", icon: "gearshape", tint: .accentColor, shortcut: "⌘,") { store.showSettings() })
    }
    actions.append(InspectorAction(title: "Diagnostics", icon: "waveform.path.ecg", tint: .secondary, shortcut: "⌘⇧D") { store.showDiagnosticsPanel() })
    actions.append(InspectorAction(title: "Refresh local MDx", icon: "arrow.clockwise", tint: .secondary, shortcut: "⌘R") { Task { await store.refresh() } })
    return actions
  }
}

struct InspectorAction: Identifiable {
  let id = UUID()
  let title: String
  let icon: String
  var tint: Color = .secondary
  var shortcut: String? = nil
  let perform: () -> Void
}

struct AuthorityRow: View {
  let item: AuthorityItem

  var body: some View {
    HStack(alignment: .top, spacing: 10) {
      Image(systemName: item.allowed ? "checkmark.circle.fill" : "lock.fill")
        .foregroundStyle(item.allowed ? Color.green : Color.secondary)
      VStack(alignment: .leading, spacing: 3) {
        Text(item.title)
          .font(.subheadline.weight(.medium))
        Text(item.detail)
          .font(.caption)
          .foregroundStyle(.secondary)
          .lineLimit(3)
      }
    }
  }
}
