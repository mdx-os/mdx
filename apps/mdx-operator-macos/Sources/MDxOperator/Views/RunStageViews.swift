import AppKit
import SwiftUI

// MARK: - Live "working" indicator
//
// The "watch it work" indicators used to each run a SwiftUI repeatForever /
// .repeating animation (an iterating ellipsis, a pulsing stage, blinking Live
// dots). While a run was live several animated at once, and each one keeps the
// whole run-detail ViewGraph re-rendering at the display refresh rate (120Hz on
// ProMotion). A main-thread sample during a live run showed ~100% of main in
// SwiftUICore renderDisplayList and almost nothing in app code: that continuous
// full-tree re-render saturated the main thread and produced multi-second
// HangMonitor stalls (the layout itself was stable). The fix keeps one moving cue,
// backed by AppKit so it animates off SwiftUI's render path, and makes the rest
// static; the streaming trail and updating action line still carry the live feel.

/// The one moving "thinking" cue on a live run, backed by AppKit's
/// NSProgressIndicator instead of a SwiftUI symbolEffect. AppKit animates the
/// spinner on its own CoreAnimation layer, entirely off SwiftUI's render path, so
/// it does NOT keep the run-detail ViewGraph regenerating its display list every
/// frame. A SwiftUI .repeating symbol effect or repeatForever animation does: a
/// main-thread sample during a live run showed ~100% of main in SwiftUICore
/// renderDisplayList and near-zero in app code, and removing every continuous
/// SwiftUI animation dropped a full 60s streaming run from ~21 HangMonitor stalls
/// (many at the 3s ceiling) to zero. This keeps the live feel at zero SwiftUI cost.
struct WorkingSpinner: NSViewRepresentable {
  func makeNSView(context: Context) -> NSProgressIndicator {
    let indicator = NSProgressIndicator()
    indicator.style = .spinning
    indicator.controlSize = .small
    indicator.isIndeterminate = true
    indicator.isDisplayedWhenStopped = false
    indicator.startAnimation(nil)
    indicator.setContentHuggingPriority(.required, for: .horizontal)
    indicator.setContentHuggingPriority(.required, for: .vertical)
    return indicator
  }

  func updateNSView(_ nsView: NSProgressIndicator, context: Context) {}
}

// MARK: - Cached fit layout
//
// A drop-in for `ViewThatFits(in: .horizontal)` whose horizontal-vs-vertical
// decision is measured once and cached in @State, and recomputed only when the
// measured widths actually change (a window resize). This is the structural fix
// for the remaining live-run corruption.
//
// `ViewThatFits` re-runs its fit trial on every layout pass. `.equatable()` on the
// candidate gates its *body* re-execution but not this layout-time re-measurement.
// So while a run streamed, every coalesced stream flush (the LiveActionLine tick)
// invalidated the pinned-header VStack layout, and both header ViewThatFits
// containers (StageTracker and RunWorkIdentityStrip) re-measured their candidates
// on that same pass. That racing re-measure inside one NSHostingView is what left
// half-resolved frames on screen mid-stream: the detail column snapped to x0 with
// its content clipped, the h1 re-wrapped and ghosted, and the action-bar buttons
// landed at interim positions. Measuring the fit once and caching the decision
// takes the header off the streaming layout hot path: a stream tick re-lays out
// the header but finds a cached axis and a plain HStack/VStack, with nothing left
// to re-measure.

struct FitAvailWidthKey: PreferenceKey {
  static let defaultValue: CGFloat = 0
  static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
    value = max(value, nextValue())
  }
}

struct FitIdealWidthKey: PreferenceKey {
  static let defaultValue: CGFloat = 0
  static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
    value = max(value, nextValue())
  }
}

struct CachedFitLayout<Horizontal: View, Vertical: View>: View {
  @ViewBuilder var horizontal: () -> Horizontal
  @ViewBuilder var vertical: () -> Vertical

  // nil until the first measurement lands; the vertical (never-clips) layout is
  // the safe first paint.
  @State private var useHorizontal: Bool?
  @State private var available: CGFloat = 0
  @State private var idealHorizontal: CGFloat = 0

  var body: some View {
    Group {
      if useHorizontal == true {
        horizontal()
      } else {
        vertical()
      }
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    // The container's available width. Measured continuously, but onPreferenceChange
    // only fires when it actually changes (a resize), never on a stream flush.
    .background(
      GeometryReader { geo in
        Color.clear.preference(key: FitAvailWidthKey.self, value: geo.size.width)
      }
    )
    // The horizontal candidate's natural width, measured offscreen. As a background
    // it never affects layout, and .fixedSize makes it report its ideal width
    // regardless of the proposed width.
    .background(
      horizontal()
        .fixedSize()
        .hidden()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
        .background(
          GeometryReader { geo in
            Color.clear.preference(key: FitIdealWidthKey.self, value: geo.size.width)
          }
        ),
      alignment: .topLeading
    )
    .onPreferenceChange(FitAvailWidthKey.self) { width in
      if abs(width - available) > 0.5 { available = width; recompute() }
    }
    .onPreferenceChange(FitIdealWidthKey.self) { width in
      if abs(width - idealHorizontal) > 0.5 { idealHorizontal = width; recompute() }
    }
  }

  private func recompute() {
    guard available > 0, idealHorizontal > 0 else { return }
    let next = idealHorizontal <= available + 0.5
    if next != useHorizontal { useHorizontal = next }
  }
}

// MARK: - Parallel candidates
//
// When a run is one lane of a parallel group, the reviewer needs to move
// between the lanes' outputs, not just read one. Both surfaces switch the whole
// run detail (diff and proof included) to a candidate by opening its run, so a
// stream tick never touches them and there is no bespoke per-candidate diff
// fetch: the existing run-open path already reloads the diff and packet.

/// A candidate lane's status color, mirroring `RunStatusDot` so a lane reads the
/// same everywhere.
func candidateDotColor(_ status: String) -> Color {
  switch forgeRunTone(status) {
  case .positive: return .green
  case .locked: return .orange
  case .neutral: return status.localizedCaseInsensitiveContains("running") ? .accentColor : .secondary
  }
}

/// The compact chip row in the pinned header: which lane am I looking at, and
/// what are the siblings. Tapping a chip opens that lane's run.
struct CandidateSwitcher: View {
  @Environment(OperatorStore.self) private var store
  let group: ForgeParallelExecutionGroup
  let currentRunID: String

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      Text("Comparing \(group.candidates.count) candidates")
        .font(.caption.weight(.semibold))
        .foregroundStyle(.secondary)
      Wrap(spacing: 6, lineSpacing: 6) {
        ForEach(group.candidates) { candidate in
          chip(candidate)
        }
      }
    }
    .padding(10)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .fill(Color.primary.opacity(0.035))
    )
  }

  @ViewBuilder
  private func chip(_ candidate: ForgeParallelExecutionCandidate) -> some View {
    let isCurrent = candidate.runID == currentRunID
    let isRecommended = group.recommendedRunID == candidate.runID && !candidate.runID.isEmpty
    let exists = store.forgeRuns.contains { $0.id == candidate.runID }
    Button {
      if !isCurrent, exists { store.openRun(candidate.runID) }
    } label: {
      HStack(spacing: 5) {
        Circle()
          .fill(candidateDotColor(candidate.status))
          .frame(width: 7, height: 7)
        Text(candidate.laneLabel)
          .font(.caption.weight(.medium))
          .lineLimit(1)
        if isRecommended {
          Image(systemName: "star.fill")
            .font(.system(size: 8))
            .foregroundStyle(.orange)
        }
      }
      .padding(.horizontal, 9)
      .padding(.vertical, 5)
      .background(
        Capsule().fill(isCurrent ? Color.accentColor.opacity(0.18) : Color.primary.opacity(0.06))
      )
      .overlay(
        Capsule().stroke(isCurrent ? Color.accentColor.opacity(0.5) : Color.clear, lineWidth: 1)
      )
      .opacity(exists || isCurrent ? 1 : 0.5)
    }
    .buttonStyle(.plain)
    .disabled(isCurrent || !exists)
    .help(isCurrent ? "This is the lane you are reviewing"
          : (exists ? "Switch to this lane" : "This lane's run is not loaded"))
  }
}

/// The Review-lane comparison: every candidate with its outcome and checks, the
/// recommended lane marked, and a one-click switch so you compare, then pick.
struct CandidateReviewGroup: View {
  @Environment(OperatorStore.self) private var store
  let group: ForgeParallelExecutionGroup
  let currentRunID: String

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack(alignment: .firstTextBaseline) {
        Text("Candidates")
          .font(.callout.weight(.semibold))
        Spacer(minLength: 8)
        if !group.selectionStatusLabel.isEmpty {
          StatusPill(status: group.selectionStatusLabel, tone: .neutral)
        }
      }
      ForEach(group.candidates) { candidate in
        row(candidate)
      }
    }
    .padding(12)
    .frame(maxWidth: .infinity, alignment: .leading)
    .mdxGlassSurface()
  }

  @ViewBuilder
  private func row(_ candidate: ForgeParallelExecutionCandidate) -> some View {
    let isCurrent = candidate.runID == currentRunID
    let isRecommended = group.recommendedRunID == candidate.runID && !candidate.runID.isEmpty
    let exists = store.forgeRuns.contains { $0.id == candidate.runID }
    let checkTotal = candidate.checksPassed + candidate.checksFailed
    HStack(alignment: .top, spacing: 10) {
      Image(systemName: isCurrent ? "largecircle.fill.circle" : "circle")
        .font(.callout)
        .foregroundStyle(isCurrent ? Color.accentColor : .secondary)
        .padding(.top, 1)
      VStack(alignment: .leading, spacing: 3) {
        HStack(spacing: 6) {
          Text(candidate.laneLabel)
            .font(.callout.weight(.medium))
          if isRecommended {
            Label("Recommended", systemImage: "star.fill")
              .font(.caption2.weight(.medium))
              .foregroundStyle(.orange)
          }
        }
        if !candidate.strategySummary.isEmpty {
          Text(candidate.strategySummary)
            .font(.caption)
            .foregroundStyle(.secondary)
            .lineLimit(2)
            .fixedSize(horizontal: false, vertical: true)
        }
        HStack(spacing: 8) {
          StatusPill(status: candidate.displayStatus, tone: forgeRunTone(candidate.status))
          if checkTotal > 0 {
            Text("\(candidate.checksPassed)/\(checkTotal) checks")
              .font(.caption)
              .foregroundStyle(candidate.checksFailed == 0 ? Color.green : Color.orange)
          }
          if candidate.turns > 0 {
            Text("\(candidate.turns) turns")
              .font(.caption)
              .foregroundStyle(.tertiary)
          }
        }
      }
      Spacer(minLength: 8)
      if isCurrent {
        Text("Reviewing")
          .font(.caption.weight(.medium))
          .foregroundStyle(Color.accentColor)
          .padding(.top, 2)
      } else {
        Button("Review this") {
          if exists { store.openRun(candidate.runID) }
        }
        .controlSize(.small)
        .disabled(!exists)
        .help(exists ? "Switch the review to this lane" : "This lane's run is not loaded")
      }
    }
    .padding(.vertical, 4)
    .overlay(alignment: .bottom) {
      Rectangle().fill(Color.secondary.opacity(0.08)).frame(height: 1)
    }
  }
}

// MARK: - Stage tracker

/// A run's identity chip. Value-typed and Equatable so the strip can be skipped
/// on a poll that only grew the run's events (these fields are stable mid-run).
struct WorkChip: Equatable, Identifiable {
  let icon: String
  let label: String
  let value: String
  var tone: StatusTone = .neutral
  var id: String { label }
}

struct RunWorkIdentityStrip: View, Equatable {
  let chips: [WorkChip]

  nonisolated static func == (lhs: RunWorkIdentityStrip, rhs: RunWorkIdentityStrip) -> Bool {
    lhs.chips == rhs.chips
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 7) {
      CachedFitLayout {
        HStack(spacing: 7) { chipViews }
      } vertical: {
        VStack(alignment: .leading, spacing: 6) { chipViews }
      }
    }
    .padding(10)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .fill(Color.primary.opacity(0.035))
    )
  }

  @ViewBuilder
  private var chipViews: some View {
    ForEach(chips) { chip in
      RunIdentityChip(icon: chip.icon, label: chip.label, value: chip.value, tone: chip.tone)
    }
  }

  // At most five chips at a glance. The full classification prose lives in the
  // inspector as sentences, and the rest stays reachable there.
  static func chips(for run: ForgeRun) -> [WorkChip] {
    var chips: [WorkChip] = []
    let primaryWorker: String = {
      if !run.harness.isEmpty { return run.harness }
      if !run.runner.displayName.isEmpty { return run.runner.displayName }
      if !run.runner.id.isEmpty { return run.runner.id }
      return ""
    }()
    if !primaryWorker.isEmpty {
      chips.append(WorkChip(icon: "person.text.rectangle", label: "Harness", value: primaryWorker))
    }
    if !run.model.isEmpty {
      chips.append(WorkChip(icon: "sparkles", label: "Model", value: run.model))
    }
    if run.execution.isWide {
      chips.append(WorkChip(icon: "square.grid.3x3", label: "Width", value: run.execution.laneLabel))
    } else if !run.execution.lane.isEmpty {
      chips.append(WorkChip(icon: "arrow.triangle.branch", label: "Role", value: run.execution.roleLabel))
    }
    if !run.workClassification.chipLabel.isEmpty {
      chips.append(WorkChip(icon: "tag", label: "Work", value: run.workClassification.chipLabel))
    }
    if run.quarantine.outputHeld {
      chips.append(WorkChip(icon: "lock", label: "Output", value: "Held", tone: .locked))
    } else if let snapshot = run.localBaseSnapshot {
      chips.append(WorkChip(icon: "tray.and.arrow.down", label: "Local changes", value: snapshot.chipLabel))
    }
    return Array(chips.prefix(5))
  }
}

struct ActiveRunContextCard: View {
  let run: ForgeRun
  let group: ForgeParallelExecutionGroup?
  let latestEvent: ForgeEvent?

  var body: some View {
    VStack(alignment: .leading, spacing: 9) {
      HStack(alignment: .firstTextBaseline, spacing: 8) {
        Text(headline)
          .font(.callout.weight(.semibold))
          .foregroundStyle(.primary)
          .lineLimit(2)
        Spacer(minLength: 8)
        if run.isRunning {
          LiveDot()
        } else {
          StatusPill(status: run.displayStatus, tone: forgeRunTone(run.status))
        }
      }
      if let latest = latestLine {
        Text(latest)
          .font(.callout)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
      if let groupLine {
        Text(groupLine)
          .font(.caption)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
      // The model/lane/checks/classification chips live once, in the header's
      // RunWorkIdentityStrip. This card keeps only the narrative state so the
      // same identity is not printed twice on one screen.
    }
    .padding(12)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .fill(Color.accentColor.opacity(0.055))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .stroke(Color.accentColor.opacity(0.13), lineWidth: 1)
    )
  }

  private var headline: String {
    if let group, group.isSettled, let candidate = group.candidate(runID: run.id) {
      if group.recommendedRunID == run.id {
        return "\(candidate.laneLabel) finished and is recommended"
      }
      return "\(candidate.laneLabel) finished · \(candidate.displayStatus.lowercased())"
    }
    if let group, let candidate = group.candidate(runID: run.id) {
      return "\(candidate.laneLabel) is \(candidate.displayStatus.lowercased())"
    }
    if run.parallelCandidate.isParallel {
      if !run.isRunning {
        return "\(run.parallelCandidate.laneLabel) finished · \(run.displayStatus.lowercased())"
      }
      return "\(run.parallelCandidate.laneLabel) is \(run.isRunning ? "working" : run.displayStatus.lowercased())"
    }
    if run.execution.isWide {
      return "\(run.execution.laneLabel.capitalized) are \(run.isRunning ? "working" : run.displayStatus.lowercased())"
    }
    return run.isRunning ? "Forge is working" : "Forge finished this run"
  }

  private var groupLine: String? {
    guard let group else {
      if run.hasAuthorityGateBlock {
        return "This branch is held before principal review until the machine gates line up."
      }
      return nil
    }
    var parts: [String] = []
    if group.isSettled, !group.completedSummaryLine.isEmpty {
      parts.append(group.completedSummaryLine)
    } else if !group.progressLine.isEmpty {
      parts.append("\(group.laneLabel): \(group.progressLine)")
    }
    if group.isSettled {
      if group.recommendedRunID != run.id, !group.recommendedRunID.isEmpty {
        let label = group.candidate(runID: group.recommendedRunID)?.laneLabel ?? group.recommendedRunID
        parts.append("Recommended lane: \(label).")
      }
    } else if group.recommendedRunID == run.id {
      parts.append("This lane is currently recommended.")
    } else if !group.selectionLine.isEmpty {
      parts.append(group.selectionLine)
    }
    if run.hasAuthorityGateBlock {
      parts.append("This branch is held before principal review.")
    }
    return parts.isEmpty ? nil : parts.joined(separator: " ")
  }

  private var latestLine: String? {
    guard let latestEvent else { return nil }
    let summary = latestEvent.summary.trimmingCharacters(in: .whitespacesAndNewlines)
    if !summary.isEmpty { return summary }
    let detail = latestEvent.detail.trimmingCharacters(in: .whitespacesAndNewlines)
    return detail.isEmpty ? nil : detail
  }

}

struct RunIdentityChip: View {
  let icon: String
  let label: String
  let value: String
  var tone: StatusTone = .neutral

  var body: some View {
    HStack(spacing: 5) {
      Image(systemName: icon)
        .font(.caption2)
      Text(label)
        .font(.caption2.weight(.medium))
        .foregroundStyle(.tertiary)
      Text(value)
        .font(.caption.weight(.medium))
        .lineLimit(1)
        .truncationMode(.middle)
    }
    .foregroundStyle(tone.foreground)
    .padding(.horizontal, 8)
    .padding(.vertical, 4)
    .frame(maxWidth: 220, alignment: .leading)
    .background(tone.fill)
    .clipShape(Capsule())
  }
}

struct StageTracker: View, Equatable {
  let stages: [ForgeStage]
  // Precomputed by the caller so the tracker is Equatable on its real inputs
  // (stages plus which of them need attention) and a poll that only appended
  // events skips it and its ViewThatFits re-measure.
  var attentionStageIDs: Set<String> = []

  nonisolated static func == (lhs: StageTracker, rhs: StageTracker) -> Bool {
    lhs.stages == rhs.stages && lhs.attentionStageIDs == rhs.attentionStageIDs
  }

  /// Convenience for call sites that hold the run: derive the attention set.
  init(stages: [ForgeStage], run: ForgeRun?) {
    self.stages = stages
    if let run {
      self.attentionStageIDs = Set(stages.filter { $0.needsAttention(in: run) }.map(\.id))
    } else {
      self.attentionStageIDs = []
    }
  }

  var body: some View {
    CachedFitLayout {
      stageLine
    } vertical: {
      VStack(alignment: .leading, spacing: 7) {
        ForEach(stages) { stage in
          stageBadge(stage)
        }
      }
    }
    .padding(12)
    .frame(maxWidth: .infinity, alignment: .leading)
    .mdxGlassSurface()
  }

  private func symbol(for stage: ForgeStage) -> String {
    if needsAttention(stage) { return "exclamationmark.triangle.fill" }
    if stage.isDone { return "checkmark.circle.fill" }
    if stage.isActive { return "circle.dotted" }
    return "circle"
  }

  private func tint(for stage: ForgeStage) -> Color {
    if needsAttention(stage) { return .orange }
    if stage.isDone { return .green }
    if stage.isActive { return .accentColor }
    return .secondary
  }

  private func needsAttention(_ stage: ForgeStage) -> Bool {
    attentionStageIDs.contains(stage.id)
  }

  private var stageLine: some View {
    HStack(spacing: 6) {
      ForEach(Array(stages.enumerated()), id: \.element.id) { index, stage in
        stageBadge(stage)
        if index < stages.count - 1 {
          Rectangle()
            .fill(Color.secondary.opacity(0.2))
            .frame(height: 1)
            .frame(width: 18)
        }
      }
    }
  }

  private func stageBadge(_ stage: ForgeStage) -> some View {
    HStack(spacing: 6) {
      Image(systemName: symbol(for: stage))
        .font(.callout)
        .foregroundStyle(tint(for: stage))
        // No pulse: the active stage already reads as active through its accent
        // tint and circle.dotted glyph, and a repeatForever pulse here was one of
        // the continuous animations that re-rendered the whole tree every frame.
      Text(stage.label)
        .font(.caption)
        .foregroundStyle(stage.isActive || needsAttention(stage) ? .primary : .secondary)
        .lineLimit(1)
        .truncationMode(.tail)
        .frame(maxWidth: 130, alignment: .leading)
    }
  }
}
