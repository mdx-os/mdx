import SwiftUI
import AppKit

// MARK: - Run list

struct RunRow: View {
  let run: ForgeRun
  var parallelGroup: ForgeParallelExecutionGroup? = nil
  var highlighted = false
  let open: () -> Void
  @State private var hovering = false
  @Environment(\.openWindow) private var openWindow

  var body: some View {
    Button(action: open) {
      HStack(spacing: 12) {
        RunStatusDot(status: run.status)
        VStack(alignment: .leading, spacing: 2) {
          // A word-boundary cap with a real ellipsis: the enclosing row clips
          // this text mid-word without one, so bound the string itself.
          Text(run.displayName.ellipsized(112))
            .font(.subheadline.weight(.medium))
            .foregroundStyle(.primary)
            .lineLimit(1)
            .truncationMode(.tail)
          if !subtitle.isEmpty {
            Text(subtitle.ellipsized(120))
              .font(.caption)
              .foregroundStyle(.secondary)
              .lineLimit(1)
              .truncationMode(.tail)
          }
        }
        Spacer(minLength: 8)
        if let parallelGroup, parallelGroup.isWide {
          StatusPill(status: parallelGroup.laneLabel, tone: .neutral)
        }
        StatusPill(status: run.displayStatus, tone: forgeRunTone(run.status))
        Image(systemName: "chevron.right")
          .font(.caption2)
          .foregroundStyle(.tertiary)
      }
      .padding(.vertical, 9)
      .padding(.horizontal, 12)
      .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
      .background(
        RoundedRectangle(cornerRadius: 8, style: .continuous)
          .fill(highlighted ? Color.accentColor.opacity(0.10) : Color.primary.opacity(hovering ? 0.06 : 0.03))
      )
      .overlay(
        RoundedRectangle(cornerRadius: 8, style: .continuous)
          .stroke(highlighted ? Color.accentColor.opacity(0.35) : Color.clear, lineWidth: 1)
      )
    }
    .buttonStyle(.plain)
    .onHover { hovering = $0 }
    .contextMenu {
      Button("Open") { open() }
      Button("Open in Monitor Window") { openWindow(id: "run-monitor", value: run.id) }
      Divider()
      Button("Copy Run ID") { Pasteboard.copy(run.id) }
      if !run.branch.isEmpty {
        Button("Copy Branch") { Pasteboard.copy(run.branch) }
      }
      if !run.intent.isEmpty {
        Button("Copy Intent") { Pasteboard.copy(run.intent) }
      }
    }
  }

  private var subtitle: String {
    if let parallelGroup, parallelGroup.isWide {
      var parts = [parallelGroup.progressLine]
      if parallelGroup.isSettled, !parallelGroup.selectionLine.isEmpty {
        parts.append(parallelGroup.selectionLine)
      }
      return parts.filter { !$0.isEmpty }.joined(separator: "  ·  ")
    }
    var parts: [String] = []
    if !run.repo.isEmpty { parts.append(run.repo) }
    if !run.harness.isEmpty { parts.append(run.harness) }
    if !run.model.isEmpty { parts.append(run.model) }
    if run.isRunning, !run.stage.isEmpty { parts.append(run.stage) }
    if run.diffFileCount > 0 { parts.append("\(run.diffFileCount) file\(run.diffFileCount == 1 ? "" : "s")") }
    return parts.joined(separator: "  ·  ")
  }
}

// MARK: - Run detail

enum RunTab: Hashable {
  case activity, preview, diff, review
}

struct RunDetailView: View {
  @Environment(OperatorStore.self) private var store
  let run: ForgeRun

  @State private var tab: RunTab = .activity
  @State private var showShip = false
  @State private var showSteer = false
  @State private var showRevise = false
  @State private var showRatifyStart = false
  @State private var shipReason = ""
  @State private var ratifyReason = ""
  @State private var steerNote = ""
  @State private var reviseComment = ""
  @AppStorage("MDxDiffMode") private var diffModeRaw = DiffViewMode.unified.rawValue
  @State private var collapsedDiffFiles: Set<String> = []
  /// When set, the Activity tab shows only steps that mention this file.
  @State private var activityFileFilter: String?
  /// Live find over the diff, plus which matching file the stepper is on.
  @State private var diffSearch = ""
  @State private var diffMatchCursor = 0
  @FocusState private var detailFocused: Bool
  @FocusState private var diffSearchFocused: Bool

  private var diffMode: DiffViewMode {
    get { DiffViewMode(rawValue: diffModeRaw) ?? .unified }
  }

  private var trailEvents: [ForgeEvent] {
    if run.isRunning {
      return store.liveEvents.isEmpty ? run.events : store.liveEvents
    }
    return run.events
  }

  private let detailColumnWidth: CGFloat = 1080

  /// Honest one-tap ship reasons. Each is a claim a reviewer can stand behind
  /// after reading the diff and the proof; they land verbatim on the ship
  /// receipt, so none of them overstate what was verified.
  static let shipReasonSuggestions = [
    "Proof is green and the behavior matches the intent.",
    "Reviewed the diff and the scope stays contained.",
    "Checks pass and the change is easy to revert.",
    "Read every changed file and the risk is low.",
  ]

  var body: some View {
    detailBody
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
  }

  private var detailBody: some View {
    VStack(spacing: 0) {
      pinnedHeader
      Divider()
      ScrollViewReader { proxy in
        ScrollView {
          tabContent(proxy)
            .padding(20)
            .frame(maxWidth: detailColumnWidth, alignment: .leading)
            .frame(maxWidth: .infinity, alignment: .leading)
            .transition(.opacity)
            .animation(.easeOut(duration: 0.15), value: tab)
        }
        // Native tail-follow for a streaming activity trail: the scroll view
        // stays pinned to the bottom as rows arrive, with no per-flush
        // proxy.scrollTo (which forced the lazy list to realize rows to the tail
        // on every burst, the last main-thread stall). Other tabs read top-first;
        // the proxy is still here for the diff file-map jump.
        .defaultScrollAnchor(tab == .activity && run.isRunning ? .bottom : .top)
      }
    }
    // Single-key verbs while the run has focus (sheets and text fields keep
    // their own key handling): A ship, R request changes, S steer, Esc back.
    .focusable()
    .focusEffectDisabled()
    .focused($detailFocused)
    .onKeyPress(characters: CharacterSet(charactersIn: "arsARS"), phases: .down) { press in
      guard !store.isRunActionInFlight(run.id) else { return .ignored }
      switch press.characters.lowercased() {
      case "a":
        if run.control("ship")?.allowed == true { showShip = true; return .handled }
      case "r":
        if run.control("revise") != nil || run.hasBranch { showRevise = true; return .handled }
      case "s":
        if run.control("steer")?.allowed == true { showSteer = true; return .handled }
      default: break
      }
      return .ignored
    }
    .onKeyPress(.escape) {
      store.closeRun()
      return .handled
    }
    .onAppear { detailFocused = true }
    .onAppear {
      if !run.isRunning && run.hasBranch { tab = .diff }
      switch ProcessInfo.processInfo.environment["MDX_SHOT_TAB"]?.lowercased() {
      case "activity":
        tab = .activity
      case "preview":
        tab = .preview
      case "diff":
        tab = .diff
      case "review":
        tab = .review
      default:
        break
      }
      loadForTab(tab)
      applyReviewShotState()
    }
    .onChange(of: tab) { _, newValue in loadForTab(newValue) }
    .onChange(of: run.id) { _, _ in activityFileFilter = nil }
    .onChange(of: run.hasBranch) { _, hasBranch in
      guard hasBranch, tab == .diff, case .idle = store.runDiffPhase else { return }
      // A newly started revision has no branch when the Diff tab first opens.
      // Load as soon as its governed branch arrives instead of leaving the
      // operator on an idle loading row until they press Reload.
      Task { await store.loadRunDiff() }
    }
    .onChange(of: store.selectedRunID) { previousRunID, selectedRunID in
      guard previousRunID != selectedRunID, showRevise else { return }
      // A successful revision opens its new governed run immediately. Close
      // the originating sheet as the selection changes so its stale note does
      // not cover the new run while it starts.
      showRevise = false
      reviseComment = ""
    }
    .sheet(isPresented: $showRevise) {
      RunNoteSheet(
        title: "Request changes",
        message: "Sends the run back with a revision note instead of shipping. The work stays governed.",
        placeholder: "What needs to change before this can ship?",
        submitLabel: "Request changes",
        text: $reviseComment,
        inFlight: store.isRunActionInFlight(run.id)
      ) {
        Task {
          await store.reviseSelectedRun(comment: reviseComment)
          if store.runActionResult?.isRefusal == false { showRevise = false; reviseComment = "" }
        }
      }
    }
    .sheet(isPresented: $showShip) {
      RunNoteSheet(
        title: "Ship this run",
        message: "Records a human ship decision with proof beside it. Deployment and production writes stay separate governed calls.",
        placeholder: "Why is this safe to ship?",
        submitLabel: "Record ship decision",
        text: $shipReason,
        inFlight: store.isRunActionInFlight(run.id),
        suggestions: Self.shipReasonSuggestions
      ) {
        Task {
          await store.shipSelectedRun(reason: shipReason)
          if store.runActionResult?.isRefusal == false { showShip = false; shipReason = "" }
        }
      }
    }
    .sheet(isPresented: $showRatifyStart) {
      RunNoteSheet(
        title: "Ship and open a PR",
        message: "Records the ship decision, then opens the PR handoff for this branch. Two governed steps, one move. Deployment stays a separate call.",
        placeholder: "Why is this safe to ship?",
        submitLabel: "Ship, then open PR",
        text: $ratifyReason,
        inFlight: store.isRunActionInFlight(run.id),
        suggestions: Self.shipReasonSuggestions
      ) {
        Task {
          let shipped = await store.shipAndOpenPR(reason: ratifyReason)
          if shipped { showRatifyStart = false; ratifyReason = "" }
        }
      }
    }
    .sheet(isPresented: $showSteer) {
      RunNoteSheet(
        title: "Steer this run",
        message: "Sends a steering note to the running work without stopping it.",
        placeholder: "What should the run do differently?",
        submitLabel: "Send steer",
        text: $steerNote,
        inFlight: store.isRunActionInFlight(run.id)
      ) {
        Task {
          await store.controlSelectedRun("steer", note: steerNote)
          if store.runActionResult?.isRefusal == false { showSteer = false; steerNote = "" }
        }
      }
    }
  }

  private var backBar: some View {
    HStack(spacing: 10) {
      Button {
        store.closeRun()
      } label: {
        Label("All runs", systemImage: "chevron.left")
      }
      .buttonStyle(.plain)
      .foregroundStyle(.secondary)
      Spacer()
      if run.isRunning {
        LiveDot()
      }
    }
  }

  private var pinnedHeader: some View {
    VStack(alignment: .leading, spacing: 12) {
      backBar
      compactHeader
      if let group = store.parallelGroup(containing: run.id), group.candidates.count > 1 {
        CandidateSwitcher(group: group, currentRunID: run.id)
      }
      if !run.stages.isEmpty {
        StageTracker(stages: run.stages, run: run)
          .equatable()
      }
      // The one live-updating strip in the pinned header. It reads the event
      // stream inside its own body, so a stream flush invalidates only this
      // line, never the stage tracker, identity strip, or action bar.
      if run.isRunning {
        LiveActionLine()
      }
      if run.hasProofTrouble {
        recoveryBanner
      }
      controlsBar
      Picker("View", selection: $tab) {
        Text("Activity").tag(RunTab.activity)
        Text("Preview").tag(RunTab.preview)
        Text(run.diffFileCount > 0 ? "Diff (\(run.diffFileCount))" : "Diff").tag(RunTab.diff)
        Text("Review").tag(RunTab.review)
      }
      .pickerStyle(.segmented)
      .labelsHidden()
      if let outcome = store.runActionResult {
        RunActionBanner(outcome: outcome)
      }
    }
    .padding(20)
    .frame(maxWidth: detailColumnWidth, alignment: .leading)
    .frame(maxWidth: .infinity, alignment: .leading)
  }

  private var compactHeader: some View {
    VStack(alignment: .leading, spacing: 7) {
      HStack(alignment: .firstTextBaseline, spacing: 10) {
        Text(run.displayName)
          .font(.title2.weight(.semibold))
          .lineLimit(2)
          // A real ellipsis at the two-line boundary: without this the h1 clips
          // mid-word (the "...helpe" the orchestrator flagged), and during a
          // mid-stream re-layout the clip lands anywhere in the word.
          .truncationMode(.tail)
        Spacer(minLength: 8)
        StatusPill(status: run.displayStatus, tone: forgeRunTone(run.status))
      }
      if !run.intent.isEmpty, run.intent != run.displayName {
        Text(run.intent)
          .font(.callout)
          .foregroundStyle(.secondary)
          .lineLimit(2)
      }
      Text(statLine)
        .font(.caption)
        .foregroundStyle(.tertiary)
        .lineLimit(1)
        .truncationMode(.middle)
        .textSelection(.enabled)
      if run.hasWorkIdentity {
        // .equatable() so a 4s poll that only appended events skips the strip
        // and its ViewThatFits re-measure (the mid-layout flicker source).
        RunWorkIdentityStrip(chips: RunWorkIdentityStrip.chips(for: run))
          .equatable()
          .padding(.top, 2)
      }
      if showFinalLine {
        Text(run.finalLine)
          .font(.callout)
          .foregroundStyle(.primary)
      }
    }
  }

  private var showFinalLine: Bool {
    !run.isRunning && !run.finalLine.isEmpty && !run.finalLine.contains("=") && run.finalLine.contains(" ")
  }

  private var statLine: String {
    var parts: [String] = []
    if !run.repo.isEmpty { parts.append(run.repo) }
    if !run.model.isEmpty { parts.append(run.model) }
    if run.turns > 0 { parts.append("\(run.turns) turns") }
    if run.toolCalls > 0 { parts.append("\(run.toolCalls) tool calls") }
    let checkTotal = run.checksPassed + run.checksFailed
    if run.selectedProofTurnedGreen {
      parts.append("selected proof green")
    } else if checkTotal > 0 {
      parts.append("\(run.checksPassed)/\(checkTotal) checks")
    }
    if run.hasBranch { parts.append(run.branch) }
    return parts.joined(separator: "  ·  ")
  }

  private var recoveryBanner: some View {
    let surface = RunRecoverySurface(run: run)
    return RunRecoveryBanner(
      surface: surface,
      reviewProof: {
        tab = .review
        loadForTab(.review)
      },
      openDiff: surface.showsOpenDiff ? {
        tab = .diff
        loadForTab(.diff)
      } : nil,
      steer: run.control("steer")?.allowed == true ? {
        showSteer = true
      } : nil,
      pickBackUp: surface.showsRevisionControl ? {
        reviseComment = surface.suggestedRevisionNote
        showRevise = true
      } : nil,
      startSmaller: (!run.isRunning && !surface.showsRevisionControl) ? {
        store.pendingBuildIntent = suggestedSmallerRunIntent
        store.startBuildFlow()
      } : nil
    )
  }

  private var suggestedSmallerRunIntent: String {
    let ask = run.intent.isEmpty ? run.displayName : run.intent
    if run.selectedProofRedOnArrival {
      return "Repair or isolate the selected proof that was already red before this Forge run. Keep the scope tight, explain whether it is baseline health or task-related, and then return to the original ask: \(ask)"
    }
    return "Pick up this Forge run with a narrower next step. Original ask: \(ask)"
  }

  /// The one verb this state promotes to a filled button. Everything else in
  /// the action bar stays quiet. When the recovery surface is up it owns the
  /// primary decision, so the action bar promotes nothing and does not compete.
  private enum PrimaryVerb { case steer, ship, requestChanges }

  private var primaryVerb: PrimaryVerb? {
    if run.hasProofTrouble { return nil }
    if run.isRunning { return run.control("steer") != nil ? .steer : nil }
    if let ship = run.control("ship"), ship.allowed { return .ship }
    if run.control("revise") != nil || run.hasBranch { return .requestChanges }
    return nil
  }

  @ViewBuilder
  private var controlsBar: some View {
    let steer = run.control("steer")
    let stop = run.control("stop")
    let ship = run.control("ship")
    if steer != nil || stop != nil || ship != nil {
      HStack(spacing: 10) {
        if let steer {
          Button {
            showSteer = true
          } label: {
            HStack(spacing: 5) {
              Label("Steer", systemImage: "location.north.line")
              Keycap("S")
            }
          }
          .mdxActionEmphasis(primaryVerb == .steer)
          .disabled(!steer.allowed || store.isRunActionInFlight(run.id))
        }
        if stop != nil {
          Button {
            Task { await store.controlSelectedRun("stop", note: "Stopped from native MDx") }
          } label: {
            Label("Stop", systemImage: "stop.circle")
          }
          .buttonStyle(.bordered)
          .disabled(!(stop?.allowed ?? false) || store.isRunActionInFlight(run.id))
        }
        if !run.hasProofTrouble && (run.control("revise") != nil || run.hasBranch) {
          Button {
            showRevise = true
          } label: {
            HStack(spacing: 5) {
              Label("Request changes", systemImage: "arrow.uturn.backward")
              Keycap("R")
            }
          }
          .mdxActionEmphasis(primaryVerb == .requestChanges)
          .disabled(store.isRunActionInFlight(run.id))
        }
        Spacer(minLength: 0)

        // Escape hatches: the work is never trapped in MDx.
        if let repo = store.selectedRunRepo, !repo.root.isEmpty {
          Menu {
            ForEach(EditorOpener.available()) { destination in
              Button(destination.label) { destination.open(repo.root, nil) }
            }
            if let origin = repo.webOrigin, let url = URL(string: origin) {
              Divider()
              Button("Repository on GitHub") { NSWorkspace.shared.open(url) }
            }
          } label: {
            Label("Open In", systemImage: "arrow.up.forward.app")
          }
          .fixedSize()
        }

        if run.hasBranch, !run.isRunning {
          Button {
            store.preparePRHandoff()
          } label: {
            Label("PR handoff", systemImage: "arrow.triangle.pull")
          }
          .buttonStyle(.bordered)
          .help("Compose a receipt-backed PR description from this run")
        }

        if let ship {
          Button {
            showShip = true
          } label: {
            HStack(spacing: 5) {
              Label("Ship", systemImage: "checkmark.seal")
              Keycap("A", onAccent: primaryVerb == .ship)
            }
          }
          .mdxActionEmphasis(primaryVerb == .ship)
          .disabled(!ship.allowed || store.isRunActionInFlight(run.id))
          .help(ship.allowed ? "Record a human ship decision" : "Ship stays locked until review proof and authority line up.")
        }
      }
    }
  }

  @ViewBuilder
  private func tabContent(_ proxy: ScrollViewProxy) -> some View {
    switch tab {
    // ActivityTab reads store.liveEvents inside its own body, so stream flushes
    // re-render the trail alone and never re-execute RunDetailView.body (which
    // would re-measure the two ViewThatFits containers and re-diff the header).
    case .activity: ActivityTab(run: run, fileFilter: $activityFileFilter)
    case .preview: BrowserPreviewView(run: run)
    case .diff: diffContent(proxy)
    case .review: reviewContent
    }
  }

  /// Per-file signals from the recorded trail: how many steps mentioned the
  /// file and whether any were errors/retries. Honest counts from receipts,
  /// not invented risk scores. (Richer kernel-side uncertainty is a pending
  /// kernel ask.)
  private var fileSignals: [String: DiffFileMap.Signal] {
    var signals: [String: DiffFileMap.Signal] = [:]
    // Prefer the kernel's per-file proof signals (round-3 kernel); fall back
    // to app-side trail matching on older kernels.
    let kernelScored = store.runDiff.contains { $0.trailStepCount > 0 || !$0.agentConfidence.isEmpty }
    for file in store.runDiff {
      if kernelScored {
        signals[file.path] = DiffFileMap.Signal(
          steps: file.trailStepCount,
          flagged: file.errorStepCount + file.retryCount,
          confidence: file.agentConfidence,
          checks: file.checksTouching.count
        )
      } else {
        let basename = (file.path as NSString).lastPathComponent
        var steps = 0
        var flagged = 0
        for event in trailEvents {
          if event.summary.localizedCaseInsensitiveContains(basename) || event.detail.localizedCaseInsensitiveContains(basename) {
            steps += 1
            if event.kind.localizedCaseInsensitiveContains("error") { flagged += 1 }
          }
        }
        signals[file.path] = DiffFileMap.Signal(steps: steps, flagged: flagged)
      }
    }
    return signals
  }


  private var changedFilesLine: String {
    let count = store.runDiff.isEmpty ? run.diffFileCount : store.runDiff.count
    return "\(count) changed file\(count == 1 ? "" : "s")"
  }

  @ViewBuilder
  private func diffContent(_ proxy: ScrollViewProxy) -> some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(alignment: .firstTextBaseline, spacing: 10) {
        Text(run.hasBranch ? changedFilesLine : "No branch yet, so there is nothing to diff.")
          .font(.callout)
          .foregroundStyle(.secondary)
        Spacer(minLength: 8)
        if run.hasBranch, !store.runDiff.isEmpty {
          Picker("", selection: $diffModeRaw) {
            ForEach(DiffViewMode.allCases) { mode in
              Text(mode.label).tag(mode.rawValue)
            }
          }
          .pickerStyle(.segmented)
          .labelsHidden()
          .fixedSize()
          .help("Unified or side-by-side")

          Button {
            withAnimation(.easeOut(duration: 0.15)) {
              if collapsedDiffFiles.count == store.runDiff.count {
                collapsedDiffFiles = []
              } else {
                collapsedDiffFiles = Set(store.runDiff.map(\.path))
              }
            }
          } label: {
            Label(collapsedDiffFiles.count == store.runDiff.count ? "Expand All" : "Collapse All",
                  systemImage: collapsedDiffFiles.count == store.runDiff.count ? "chevron.down" : "chevron.right")
          }
          .controlSize(.small)
        }
        if run.hasBranch {
          Button {
            Task { await store.loadRunDiff() }
          } label: {
            Label("Reload", systemImage: "arrow.clockwise")
          }
          .controlSize(.small)
          .disabled(isDiffLoading)
        }
      }

      // Find across the changed files: type to highlight every match, step
      // through matching files with the chevrons or Return. Needed the moment a
      // diff is more than a screen tall.
      if run.hasBranch, !store.runDiff.isEmpty {
        diffFindBar(proxy)
      }

      // Proof beside the code: what actually ran, right where you review.
      if run.checksPassed + run.checksFailed > 0 || run.toolCalls > 0 {
        HStack(spacing: 8) {
          if run.checksPassed + run.checksFailed > 0 {
            StatusPill(
              status: run.selectedProofTurnedGreen
                ? "Selected proof green"
                : "\(run.checksPassed)/\(run.checksPassed + run.checksFailed) checks passed",
              tone: run.selectedProofTurnedGreen || run.checksFailed == 0 ? .positive : .locked
            )
          }
          if run.toolCalls > 0 {
            StatusPill(status: "\(run.toolCalls) tool calls", tone: .neutral)
          }
          if run.turns > 0 {
            StatusPill(status: "\(run.turns) turns", tone: .neutral)
          }
          Spacer(minLength: 0)
          Button {
            tab = .review
          } label: {
            Label("Full proof", systemImage: "checkmark.seal")
          }
          .controlSize(.small)
          .help("Open the review packet: checks, judges, and evidence")
        }
      }

      // File map: one glance at the shape of the change, one click to jump.
      if store.runDiff.count > 2 {
        DiffFileMap(files: store.runDiff, signals: fileSignals) { path in
          collapsedDiffFiles.remove(path)
          withAnimation(.easeOut(duration: 0.2)) { proxy.scrollTo("diff-\(path)", anchor: .top) }
        }
      }

      switch store.runDiffPhase {
      case .loading:
        loadingRow("Loading the diff…")
      case .failed(let message), .stale(let message):
        EmptyStateView(text: message)
      case .loaded, .empty:
        if store.runDiff.isEmpty {
          EmptyStateView(text: "No file changes were returned for this run.")
        } else {
          ForEach(store.runDiff) { file in
            DiffFileView(
              file: file,
              mode: diffMode,
              repoRoot: store.selectedRunRepo?.root,
              expanded: Binding(
                get: { !collapsedDiffFiles.contains(file.path) },
                set: { open in
                  if open { collapsedDiffFiles.remove(file.path) } else { collapsedDiffFiles.insert(file.path) }
                }
              ),
              requestChanges: { path in
                reviseComment = "In \(path): "
                showRevise = true
              },
              showActivity: { path in
                activityFileFilter = path
                tab = .activity
              },
              searchTerm: diffSearch
            )
            .id("diff-\(file.path)")
          }
        }
      case .idle:
        if run.hasBranch { loadingRow("Loading the diff…") } else {
          EmptyStateView(text: "This run has no diff to review yet.")
        }
      }
    }
  }

  @ViewBuilder
  private func diffFindBar(_ proxy: ScrollViewProxy) -> some View {
    let hits = diffSearchHits
    HStack(spacing: 8) {
      Image(systemName: "magnifyingglass")
        .font(.caption)
        .foregroundStyle(.secondary)
      TextField("Find in diff", text: $diffSearch)
        .textFieldStyle(.plain)
        .font(.callout)
        .focused($diffSearchFocused)
        .onSubmit { stepDiffMatch(proxy, by: 1) }
        .onChange(of: diffSearch) { _, term in
          diffMatchCursor = 0
          let paths = matchingPaths(term)
          for path in paths { collapsedDiffFiles.remove(path) }
          if let first = paths.first { jumpToFile(first, proxy) }
        }
      if !diffSearch.trimmingCharacters(in: .whitespaces).isEmpty {
        Text(matchLabel(hits))
          .font(.caption)
          .foregroundStyle(hits.lines == 0 ? Color.orange : Color.secondary)
        Button { stepDiffMatch(proxy, by: -1) } label: {
          Image(systemName: "chevron.up")
        }
        .controlSize(.small)
        .disabled(hits.files.isEmpty)
        .help("Previous file with a match")
        Button { stepDiffMatch(proxy, by: 1) } label: {
          Image(systemName: "chevron.down")
        }
        .controlSize(.small)
        .disabled(hits.files.isEmpty)
        .help("Next file with a match")
        Button { diffSearch = "" } label: {
          Image(systemName: "xmark.circle.fill")
        }
        .buttonStyle(.plain)
        .foregroundStyle(.tertiary)
        .help("Clear find")
      }
    }
    .padding(.horizontal, 10)
    .padding(.vertical, 6)
    .background(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .fill(Color.primary.opacity(0.04))
    )
  }

  private func matchLabel(_ hits: (files: [String], lines: Int)) -> String {
    if diffSearch.trimmingCharacters(in: .whitespaces).count < 2 { return "Keep typing" }
    if hits.lines == 0 { return "No matches" }
    let lines = "\(hits.lines) line\(hits.lines == 1 ? "" : "s")"
    let files = "\(hits.files.count) file\(hits.files.count == 1 ? "" : "s")"
    if hits.files.count <= 1 { return lines }
    return "\(lines) · \(min(diffMatchCursor + 1, hits.files.count))/\(files)"
  }

  /// Files whose rendered lines contain the term, in file order. Matching on
  /// `lines` (not the raw patch) keeps the count aligned with what the reader
  /// sees highlighted.
  private func matchingPaths(_ term: String) -> [String] {
    let trimmed = term.trimmingCharacters(in: .whitespaces)
    guard trimmed.count >= 2 else { return [] }
    return store.runDiff.compactMap { file in
      file.lines.contains { $0.text.range(of: trimmed, options: .caseInsensitive) != nil } ? file.path : nil
    }
  }

  private var diffSearchHits: (files: [String], lines: Int) {
    let trimmed = diffSearch.trimmingCharacters(in: .whitespaces)
    guard trimmed.count >= 2 else { return ([], 0) }
    var files: [String] = []
    var lineCount = 0
    for file in store.runDiff {
      let count = file.lines.filter { $0.text.range(of: trimmed, options: .caseInsensitive) != nil }.count
      if count > 0 { files.append(file.path); lineCount += count }
    }
    return (files, lineCount)
  }

  private func stepDiffMatch(_ proxy: ScrollViewProxy, by delta: Int) {
    let files = diffSearchHits.files
    guard !files.isEmpty else { return }
    diffMatchCursor = ((diffMatchCursor + delta) % files.count + files.count) % files.count
    jumpToFile(files[diffMatchCursor], proxy)
  }

  private func jumpToFile(_ path: String, _ proxy: ScrollViewProxy) {
    collapsedDiffFiles.remove(path)
    withAnimation(.easeOut(duration: 0.2)) { proxy.scrollTo("diff-\(path)", anchor: .top) }
  }

  @ViewBuilder
  private var reviewContent: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(alignment: .firstTextBaseline) {
        Text("The proof bundle Forge assembles so you can decide with evidence, not vibes.")
          .font(.callout)
          .foregroundStyle(.secondary)
        Spacer(minLength: 8)
        Button {
          Task { await store.loadReviewPacket() }
        } label: {
          Label("Rebuild", systemImage: "arrow.clockwise")
        }
        .controlSize(.small)
        .disabled(isPacketLoading)
      }

      // Compare the parallel lanes, then pick. Each row switches the whole
      // review surface to that candidate's run so the diff and proof below
      // follow your choice.
      if let group = store.parallelGroup(containing: run.id), group.candidates.count > 1 {
        CandidateReviewGroup(group: group, currentRunID: run.id)
      }

      reviewDecisionBar

      switch store.reviewPacketPhase {
      case .loading, .idle:
        loadingRow("Building the review packet…")
      case .failed(let message), .stale(let message):
        EmptyStateView(text: message)
      case .loaded, .empty:
        if let packet = store.reviewPacket {
          ReviewPacketView(packet: packet, proofRecovered: run.proofRecoveredAfterFailure)
        } else {
          EmptyStateView(text: "No review packet was returned for this run.")
        }
      }
    }
  }

  /// The review surface's one primary decision: ship this reviewed run and open
  /// its PR in a single move. Only appears once the run is finished, has a
  /// branch, and ship is unlocked; otherwise the surface stays evidence-only and
  /// the controls bar keeps the granular verbs.
  @ViewBuilder
  private var reviewDecisionBar: some View {
    if !run.isRunning, run.hasBranch, run.control("ship")?.allowed == true {
      HStack(spacing: 10) {
        Button {
          ratifyReason = ""
          showRatifyStart = true
        } label: {
          Label("Ship and open a PR", systemImage: "checkmark.seal")
        }
        .mdxActionEmphasis(true)
        .disabled(store.isRunActionInFlight(run.id))
        .help("Records the ship decision, then opens the PR handoff. Two receipts, one move.")

        Button {
          shipReason = ""
          showShip = true
        } label: {
          Text("Record ship decision only")
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .disabled(store.isRunActionInFlight(run.id))

        Spacer(minLength: 0)

        if run.control("revise") != nil || run.hasBranch {
          Button {
            reviseComment = ""
            showRevise = true
          } label: {
            Label("Request changes", systemImage: "arrow.uturn.backward")
          }
          .controlSize(.small)
          .disabled(store.isRunActionInFlight(run.id))
        }
      }
      .padding(12)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(
        RoundedRectangle(cornerRadius: 10, style: .continuous)
          .fill(Color.accentColor.opacity(0.06))
      )
    }
  }

  private func loadingRow(_ text: String) -> some View {
    HStack(spacing: 8) {
      ProgressView().controlSize(.small)
      Text(text).font(.callout).foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(.vertical, 8)
  }

  /// Deterministic screenshot state for the review desk, in the same family as
  /// MDX_SHOT_TAB/PALETTE/SETTINGS. Inert unless the env var is set (only the
  /// visual-QA launcher sets them), and each one just opens a real control, so
  /// nothing is faked. A tiny delay lets the diff/packet load first.
  private func applyReviewShotState() {
    let env = ProcessInfo.processInfo.environment
    if let term = env["MDX_SHOT_DIFF_FIND"], !term.isEmpty {
      diffSearch = term
    }
    if env["MDX_SHOT_SHIP"] != nil {
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { showShip = true }
    }
    if env["MDX_SHOT_RATIFY"] != nil {
      DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { showRatifyStart = true }
    }
  }

  private func loadForTab(_ tab: RunTab) {
    switch tab {
    case .diff:
      if case .idle = store.runDiffPhase, run.hasBranch {
        Task { await store.loadRunDiff() }
      }
    case .review:
      if case .idle = store.reviewPacketPhase {
        Task { await store.loadReviewPacket() }
      }
    case .activity:
      break
    case .preview:
      break
    }
  }

  private var isDiffLoading: Bool {
    if case .loading = store.runDiffPhase { return true }
    return false
  }

  private var isPacketLoading: Bool {
    if case .loading = store.reviewPacketPhase { return true }
    return false
  }
}
