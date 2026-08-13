import AppKit
import SwiftUI

struct FleetPlanRow: View {
  let plan: FleetPlan
  let started: Bool
  let inFlight: Bool
  let ratify: () -> Void
  let start: () -> Void
  let recover: () -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack(alignment: .top, spacing: 12) {
        Image(systemName: planIcon)
          .font(.title3)
          .foregroundStyle(plan.isRatified ? Color.green : Color.accentColor)
          .frame(width: 22)
        VStack(alignment: .leading, spacing: 4) {
          HStack(alignment: .firstTextBaseline) {
            Text(plan.goalLine)
              .font(.headline)
              .lineLimit(2)
            Spacer(minLength: 8)
            StatusPill(status: statusText, tone: statusTone)
            fleetAction
          }
          Text(plan.displayLine)
            .font(.callout)
            .foregroundStyle(.secondary)
            .lineLimit(2)
          if !plan.reviewConcerns.isEmpty {
            Text(plan.reviewConcerns)
              .font(.caption)
              .foregroundStyle(.secondary)
              .lineLimit(2)
          }
        }
      }

      // Title, one status pill, and one action lead. The split's shape
      // (streams, checks, builder mix, repo, proof, full prompt) waits in the
      // expanded body so the plan reads as an overview first.
      DisclosureGroup("Plan details") {
        VStack(alignment: .leading, spacing: 8) {
          HStack(spacing: 8) {
            if plan.isPlanning && !plan.planningStageLabel.isEmpty { MetaChip(text: plan.planningStageLabel) }
            MetaChip(text: "\(plan.streamCount) streams")
            if plan.checkCount > 0 { MetaChip(text: "\(plan.checkCount) checks") }
            if plan.requestedWidth > 0 { MetaChip(text: "\(plan.requestedWidth) requested lanes") }
            if !plan.builderMixLabel.isEmpty { MetaChip(text: plan.builderMixLabel) }
          }
          .lineLimit(1)

          HStack(alignment: .firstTextBaseline, spacing: 8) {
            if !plan.repo.isEmpty { Text(plan.repo) }
            if !plan.languagePack.isEmpty { Text(plan.languagePack) }
            if !plan.proofSummary.isEmpty { Text(plan.proofSummary) }
          }
          .font(.caption)
          .foregroundStyle(.tertiary)
          .lineLimit(2)

          if plan.hasFullPrompt {
            DisclosureGroup("Full plan prompt") {
              Text(plan.spec)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, 4)
            }
            .font(.caption)
          }
        }
        .padding(.top, 4)
      }
      .font(.caption)
    }
    .padding(14)
    .frame(maxWidth: .infinity, alignment: .leading)
    .mdxGlassSurface(interactive: true)
  }

  @ViewBuilder
  private var fleetAction: some View {
    if plan.needsPlanningRepair {
      Button(action: recover) {
        Label("Repair in Mission", systemImage: "wrench.and.screwdriver")
      }
      .controlSize(.small)
      .disabled(inFlight)
    } else if plan.isDelayedPlanning {
      Button(action: recover) {
        Label("Move to Mission", systemImage: "flag.checkered")
      }
      .controlSize(.small)
      .disabled(inFlight)
    } else if plan.isPlanning {
      HStack(spacing: 6) {
        ProgressView()
          .controlSize(.small)
        Text(plan.planningStageLabel.isEmpty ? "Planning" : plan.planningStageLabel)
          .font(.caption.weight(.medium))
      }
      .foregroundStyle(.secondary)
    } else if started || plan.needsPlanReview {
      EmptyView()
    } else if plan.isRatified {
      Button(action: start) {
        Label("Start fleet", systemImage: "play.fill")
      }
      .controlSize(.small)
      .disabled(inFlight)
    } else {
      Button(action: ratify) {
        Label("Ratify split", systemImage: "checkmark.seal")
      }
      .controlSize(.small)
      .disabled(inFlight)
    }
  }

  private var statusText: String {
    if plan.needsPlanningRepair { return "Needs repair" }
    if plan.isPlanning { return "Planning" }
    if started { return "Started" }
    if ["missing_reviewer", "review_unavailable"].contains(plan.reviewStatus) { return "Review held" }
    if plan.needsPlanReview { return "Needs revision" }
    if plan.isRatified { return "Ready to start" }
    return "Review the split"
  }

  private var statusTone: StatusTone {
    if plan.needsPlanningRepair { return .locked }
    if plan.isPlanning { return .neutral }
    if started { return .neutral }
    if plan.needsPlanReview { return .locked }
    if plan.isRatified { return .positive }
    return .neutral
  }

  private var planIcon: String {
    if plan.needsPlanningRepair { return "exclamationmark.triangle.fill" }
    if plan.isPlanning { return "clock.arrow.circlepath" }
    if plan.isRatified { return "checkmark.seal.fill" }
    return "text.badge.checkmark"
  }
}

struct FleetRunRow: View {
  let run: FleetRun
  let openRun: (String) -> Void
  let recover: (() -> Void)?
  // Problems open by default; a clean run stays collapsed to its one-line
  // summary so the fleet reads as an integration overview, not a wall of lanes.
  @State private var lanesExpanded: Bool

  init(run: FleetRun, openRun: @escaping (String) -> Void, recover: (() -> Void)?) {
    self.run = run
    self.openRun = openRun
    self.recover = recover
    _lanesExpanded = State(initialValue: run.needsRecovery)
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(alignment: .firstTextBaseline, spacing: 10) {
        Image(systemName: statusIcon)
          .foregroundStyle(statusToneColor)
          .frame(width: 20)
        VStack(alignment: .leading, spacing: 3) {
          Text(run.displayName)
            .font(.subheadline.weight(.semibold))
            .lineLimit(1)
          if !run.summary.isEmpty, run.summary != run.displayName {
            Text(run.summary)
              .font(.callout)
              .foregroundStyle(.secondary)
          }
        }
        Spacer(minLength: 8)
        StatusPill(status: run.displayStatus, tone: statusTone)
        // One action per card: repair when a lane needs it, otherwise the lanes
        // hold their own open-run entries inside the disclosure.
        if run.needsRecovery, let recover {
          Button(action: recover) {
            Label(run.hasIntegrationFailure ? "Repair in Mission" : "Recover in Mission", systemImage: "wrench.and.screwdriver")
          }
          .controlSize(.small)
        }
      }

      if !run.recovery.isEmpty {
        Text(run.recovery)
          .font(.caption)
          .foregroundStyle(.secondary)
          .lineLimit(2)
      }

      if !run.lanes.isEmpty {
        DisclosureGroup(isExpanded: $lanesExpanded) {
          VStack(spacing: 6) {
            ForEach(run.lanes) { lane in
              FleetLaneRow(lane: lane, openRun: openRun)
            }
          }
          .padding(.top, 6)
        } label: {
          Text(run.lanesSummaryLine)
            .font(.caption.weight(.medium))
            .foregroundStyle(.secondary)
        }
      }
    }
    .padding(14)
    .frame(maxWidth: .infinity, alignment: .leading)
    .mdxGlassSurface(interactive: true)
  }

  private var statusTone: StatusTone {
    if run.attentionCount > 0 { return .locked }
    if run.finished { return .positive }
    return .neutral
  }

  private var statusIcon: String {
    if run.attentionCount > 0 { return "exclamationmark.triangle.fill" }
    if run.finished { return "checkmark.seal.fill" }
    return "arrow.triangle.2.circlepath"
  }

  private var statusToneColor: Color {
    if run.attentionCount > 0 { return .orange }
    if run.finished { return .green }
    return .accentColor
  }
}

struct FleetLaneRow: View {
  let lane: FleetLane
  let openRun: (String) -> Void

  var body: some View {
    HStack(alignment: .top, spacing: 10) {
      Image(systemName: lane.needsAttention ? "exclamationmark.circle" : lane.isWorking ? "clock.arrow.circlepath" : "checkmark.circle")
        .foregroundStyle(lane.needsAttention ? .orange : lane.isWorking ? Color.accentColor : .green)
        .frame(width: 18)
      VStack(alignment: .leading, spacing: 4) {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
          Text(lane.laneName)
            .font(.caption.weight(.semibold))
            .lineLimit(1)
          StatusPill(status: lane.stateLabel, tone: lane.needsAttention ? .locked : .neutral)
          if !lane.coder.isEmpty {
            MetaChip(text: lane.coder)
          }
          if !lane.model.isEmpty {
            MetaChip(text: lane.model)
          }
        }
        if !lane.detailLabel.isEmpty {
          Text(lane.detailLabel)
            .font(.caption)
            .foregroundStyle(.secondary)
            .lineLimit(2)
        }
        if !lane.castingNote.isEmpty {
          Text(lane.castingNote)
            .font(.caption2)
            .foregroundStyle(.tertiary)
            .lineLimit(1)
        }
      }
      Spacer(minLength: 0)
      if !lane.runID.isEmpty {
        Button {
          openRun(lane.runID)
        } label: {
          Label("Open run", systemImage: "arrow.forward.circle")
        }
        .controlSize(.small)
      }
    }
    .padding(.vertical, 8)
    .padding(.horizontal, 10)
    .background(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .fill(Color.secondary.opacity(0.06))
    )
  }

}

struct ModelRoleStrip: View {
  let roles: [ModelRoleRoute]

  private var readyCount: Int {
    roles.filter { $0.ready && !$0.advisoryOnly }.count
  }

  private var runnableCount: Int {
    roles.filter { !$0.advisoryOnly }.count
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack(alignment: .firstTextBaseline) {
        VStack(alignment: .leading, spacing: 3) {
          Text("Model roles")
            .font(.headline)
          Text("\(readyCount) of \(runnableCount) roles connected. The advisor can help on hard calls, but never runs work itself.")
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        Spacer()
      }

      LazyVGrid(columns: [GridItem(.adaptive(minimum: 156), spacing: 8)], spacing: 8) {
        ForEach(roles) { role in
          ModelRoleCard(role: role)
        }
      }
    }
    .padding(14)
    .mdxGlassSurface()
  }
}

struct ModelRoleCard: View {
  let role: ModelRoleRoute

  private var icon: String {
    if role.advisoryOnly { return "person.crop.circle.badge.questionmark" }
    switch role.id {
    case "plan": return "point.3.connected.trianglepath.dotted"
    case "build": return "hammer"
    case "integrate": return "arrow.triangle.merge"
    case "review": return "checkmark.seal"
    case "evaluate": return "chart.line.uptrend.xyaxis"
    default: return "sparkles"
    }
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      HStack(spacing: 6) {
        Image(systemName: icon)
          .foregroundStyle(role.ready ? Color.accentColor : Color.secondary)
        Text(role.title)
          .font(.caption.weight(.semibold))
          .lineLimit(1)
        Spacer(minLength: 0)
      }
      Text(role.ready ? role.provider : "Not connected yet")
        .font(.callout.weight(.medium))
        .lineLimit(1)
      Text(role.displayModel)
        .font(.caption)
        .foregroundStyle(.secondary)
        .lineLimit(1)
      StatusPill(status: role.supportLine, tone: role.ready ? .positive : .locked)
    }
    .padding(10)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .fill(role.ready ? Color.accentColor.opacity(0.07) : Color.primary.opacity(0.035))
    )
  }
}

struct MachineRunnerSheet: View {
  let runner: MachineRunner
  let result: RunActionOutcome?
  let inFlight: Bool
  let stageFaceOff: () -> Void
  @Environment(\.dismiss) private var dismiss

  private var canStageFaceOff: Bool {
    runner.canStageFaceOff
  }

  private var runtimePillText: String {
    runner.runtimeStatusLabel
  }

  private var runtimePillTone: StatusTone {
    runner.isNative || runner.liveExecutionReady || runner.smokePassed ? .positive : (!runner.runtimeChecked || runner.binaryPresent) ? .neutral : .locked
  }

  private var clearancePillTone: StatusTone {
    if runner.isNative || !runner.requiresClearance || !runner.clearanceMode.isEmpty { return .positive }
    return .locked
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      HStack(alignment: .top, spacing: 12) {
        Image(systemName: runner.selected ? "checkmark.seal.fill" : "cpu")
          .font(.title2)
          .foregroundStyle(runner.selected ? Color.green : Color.accentColor)
          .frame(width: 26)
        VStack(alignment: .leading, spacing: 4) {
          Text(runner.name)
            .font(.title3.weight(.semibold))
          Text(runner.model)
            .font(.callout)
            .foregroundStyle(.secondary)
          HStack(spacing: 6) {
            StatusPill(status: runner.selected ? "Selected" : runner.protocolLabel, tone: runner.selected ? .positive : .neutral)
            StatusPill(status: runner.connectionLabel, tone: clearancePillTone)
            StatusPill(status: runtimePillText, tone: runtimePillTone)
          }
        }
        Spacer()
        Button("Done") { dismiss() }
      }

      VStack(alignment: .leading, spacing: 8) {
        Text("How this machine works")
          .font(.headline)
        MachineDetailLine(title: "How it connects", value: runner.protocolDetail)
        MachineDetailLine(title: "Where it stands", value: runner.readinessLine)
      }

      VStack(alignment: .leading, spacing: 8) {
        Text("Admin readiness")
          .font(.headline)
        MachineGateRow(
          title: "Sign-in",
          open: runner.connectionOpen,
          detail: runner.requiresClearance ? runner.connectionLabel : "No sign-in is required for this machine."
        )
        MachineGateRow(
          title: "Runtime",
          open: runner.isNative || runner.smokePassed || runner.liveExecutionReady,
          detail: runner.isNative ? "The native builder is built in." : runner.adminActionLine
        )
        if !runner.versionObserved.isEmpty {
          MachineDetailLine(title: "Observed", value: runner.versionObserved)
        }
      }

      VStack(alignment: .leading, spacing: 8) {
        Text("What this machine can't do yet")
          .font(.headline)
        MachineGateRow(title: "Run work", open: runner.liveExecutionReady, detail: runner.liveExecutionReady ? "It can run work on this Mac. Its output is held until Forge checks accept it." : "It can't run work from this Mac yet.")
        MachineGateRow(title: "Ship its output", open: false, detail: "Its output is held until Forge checks accept it.")
        MachineGateRow(title: "Write to production", open: false, detail: "Production writes stay closed from this surface.")
      }

      DisclosureGroup("Details") {
        VStack(alignment: .leading, spacing: 8) {
          MachineDetailLine(title: "Protocol", value: runner.protocolLabel)
          if !runner.compatibilityLine.isEmpty {
            MachineDetailLine(title: "Compatibility", value: runner.compatibilityLine)
          }
          MachineDetailLine(title: "Status code", value: runner.status)
        }
        .padding(.top, 4)
      }
      .font(.subheadline.weight(.medium))

      if let result {
        RunActionBanner(outcome: result)
      }

      HStack {
        Button {
          stageFaceOff()
        } label: {
          Label(inFlight ? "Staging" : "Compare on a fixture", systemImage: "flag.checkered")
        }
        .mdxPrimaryButtonStyle()
        .disabled(inFlight || !canStageFaceOff)

        Text(canStageFaceOff ? "Runs this machine against a fixture so you can compare it. It does not allow production writes." : runner.adminActionLine)
          .font(.caption)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
    }
    .padding(20)
    .frame(width: 560)
  }
}

struct MachineDetailLine: View {
  let title: String
  let value: String

  var body: some View {
    VStack(alignment: .leading, spacing: 2) {
      Text(title)
        .font(.caption.weight(.semibold))
        .foregroundStyle(.secondary)
      Text(value)
        .font(.callout)
        .foregroundStyle(.primary)
        .fixedSize(horizontal: false, vertical: true)
    }
  }
}

struct MachineGateRow: View {
  let title: String
  let open: Bool
  let detail: String

  var body: some View {
    HStack(alignment: .top, spacing: 10) {
      Image(systemName: open ? "checkmark.circle.fill" : "lock.circle")
        .foregroundStyle(open ? Color.green : Color.secondary)
        .frame(width: 18)
      VStack(alignment: .leading, spacing: 2) {
        Text(title)
          .font(.subheadline.weight(.medium))
        Text(detail)
          .font(.caption)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
    }
  }
}

struct RunnerTile: View {
  let runner: MachineRunner

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack {
        Text(runner.name)
          .font(.headline)
        Spacer()
        Image(systemName: runner.selected ? "checkmark.seal.fill" : runner.liveExecutionReady ? "play.circle.fill" : runner.optionEnabled ? "slider.horizontal.3" : "lock.circle")
          .foregroundStyle(runner.selected ? Color.green : runner.liveExecutionReady ? Color.green : runner.optionEnabled ? Color.accentColor : Color.secondary)
      }
      Text(runner.model)
        .font(.callout)
        .foregroundStyle(.secondary)
        .lineLimit(1)
      Text(runner.readinessLine)
        .font(.caption)
        .foregroundStyle(.secondary)
        .lineLimit(3)
      // At most three pills, human words, sized to fit. The protocol, raw
      // status code, and compatibility detail live in the machine sheet.
      HStack(spacing: 6) {
        if runner.selected {
          StatusPill(status: "Selected", tone: .positive)
        }
        StatusPill(status: runner.connectionLabel, tone: runner.connectionOpen ? .positive : .locked)
        StatusPill(status: runner.runtimeStatusLabel, tone: runner.liveExecutionReady || runner.smokePassed || runner.isNative ? .positive : (!runner.runtimeChecked || runner.binaryPresent) ? .neutral : .locked)
      }
    }
    .padding(14)
    .frame(maxWidth: .infinity, alignment: .leading)
    .mdxGlassSurface()
  }
}
