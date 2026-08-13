import SwiftUI

func missionTone(_ mission: ForgeMission) -> StatusTone {
  if mission.blockedMilestoneCount > 0 || mission.state.localizedCaseInsensitiveContains("blocked") { return .locked }
  if !mission.isActive { return .positive }
  return .neutral
}

struct MissionRow: View {
  let mission: ForgeMission
  let open: () -> Void

  var body: some View {
    Button(action: open) {
      HStack(spacing: 12) {
        Image(systemName: "flag.checkered")
          .font(.title3)
          .foregroundStyle(.secondary)
          .frame(width: 22)
        VStack(alignment: .leading, spacing: 3) {
          Text(mission.goal.isEmpty ? mission.id : mission.goal)
            .font(.headline)
            .lineLimit(2)
          Text("\(mission.completedMilestoneCount)/\(mission.milestoneCount) milestones · \(mission.budgetSummary)")
            .font(.caption)
            .foregroundStyle(.secondary)
            .lineLimit(1)
        }
        Spacer(minLength: 8)
        StatusPill(status: mission.stateLabel, tone: missionTone(mission))
        Image(systemName: "chevron.right")
          .font(.caption)
          .foregroundStyle(.tertiary)
      }
      .padding(14)
      .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
      .mdxGlassSurface(interactive: true)
    }
    .buttonStyle(.plain)
  }
}

struct MissionDetailView: View {
  @Environment(OperatorStore.self) private var store
  let mission: ForgeMission

  @State private var showSteer = false
  @State private var steerNote = ""
  @State private var didAutoLaunchShot = false

  var body: some View {
    VStack(spacing: 0) {
      VStack(alignment: .leading, spacing: 12) {
        backBar
        header
        if let outcome = store.missionActionResult {
          RunActionBanner(outcome: outcome)
        }
        controls
      }
      .padding(20)

      Divider()

      ScrollView {
        VStack(alignment: .leading, spacing: 18) {
          milestonesSection
          if !mission.relatedRunIDs.isEmpty {
            relatedRunsSection
          }
          detailsSection
        }
        .padding(20)
        .frame(maxWidth: .infinity, alignment: .leading)
      }
    }
    .sheet(isPresented: $showSteer) {
      RunNoteSheet(
        title: "Steer this mission",
        message: "Records a steering note as a checkpoint. The mission keeps its scope, budget, and checkpoints.",
        placeholder: "What should the mission adjust?",
        submitLabel: "Send steer",
        text: $steerNote,
        inFlight: store.missionActionInFlight
      ) {
        Task {
          await store.checkpointSelectedMission(event: "mission_steered", note: steerNote)
          if store.missionActionResult?.isRefusal == false { showSteer = false; steerNote = "" }
        }
      }
    }
    .task(id: mission.id) {
      await autoLaunchForScreenshotIfRequested()
    }
  }

  private var backBar: some View {
    HStack(spacing: 10) {
      Button {
        store.closeMission()
      } label: {
        Label("All missions", systemImage: "chevron.left")
      }
      .buttonStyle(.plain)
      .foregroundStyle(.secondary)
      Spacer()
    }
  }

  private var header: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack(alignment: .firstTextBaseline, spacing: 10) {
        Text(mission.goal.isEmpty ? mission.id : mission.goal)
          .font(.title2.weight(.semibold))
          .fixedSize(horizontal: false, vertical: true)
        Spacer(minLength: 8)
        StatusPill(status: mission.stateLabel, tone: missionTone(mission))
      }
      Text(mission.budgetSummary)
        .font(.callout)
        .foregroundStyle(.secondary)
      if !mission.latestCheckpointSummary.isEmpty {
        Label(mission.latestCheckpointSummary, systemImage: "clock.arrow.circlepath")
          .font(.callout)
          .foregroundStyle(.primary)
          .padding(12)
          .frame(maxWidth: .infinity, alignment: .leading)
          .mdxGlassSurface()
      }
    }
  }

  @ViewBuilder
  private var controls: some View {
    HStack(spacing: 10) {
      if mission.state.localizedCaseInsensitiveContains("waiting") {
        Button {
          Task { await store.launchSelectedMission() }
        } label: {
          Label("Launch", systemImage: "paperplane.fill")
        }
        .mdxPrimaryButtonStyle()
        .disabled(store.missionActionInFlight)
      }
      if mission.state.localizedCaseInsensitiveContains("paused") {
        Button {
          Task { await store.checkpointSelectedMission(event: "mission_resumed") }
        } label: {
          Label("Resume", systemImage: "play.circle")
        }
        .disabled(store.missionActionInFlight)
      } else if mission.isActive {
        Button {
          Task { await store.checkpointSelectedMission(event: "mission_paused") }
        } label: {
          Label("Pause", systemImage: "pause.circle")
        }
        .disabled(store.missionActionInFlight)
      }
      Button {
        showSteer = true
      } label: {
        Label("Steer", systemImage: "location.north.line")
      }
      .disabled(store.missionActionInFlight)
      Spacer(minLength: 0)
    }
  }

  private func autoLaunchForScreenshotIfRequested() async {
    guard ProcessInfo.processInfo.environment["MDX_SHOT_AUTOLAUNCH_MISSION"] == "1" else { return }
    guard !didAutoLaunchShot else { return }
    guard mission.state.localizedCaseInsensitiveContains("waiting") else { return }
    didAutoLaunchShot = true
    try? await Task.sleep(nanoseconds: shotMissionDelayNanoseconds)
    if Task.isCancelled { return }
    guard !store.missionActionInFlight else { return }
    await store.launchSelectedMission()
  }

  private var shotMissionDelayNanoseconds: UInt64 {
    let raw = ProcessInfo.processInfo.environment["MDX_SHOT_AUTOSTART_DELAY_MS"] ?? ""
    let milliseconds = UInt64(raw) ?? 1_200
    return min(max(milliseconds, 300), 20_000) * 1_000_000
  }

  private var milestonesSection: some View {
    VStack(alignment: .leading, spacing: 10) {
      SectionHeader(
        title: "Milestones",
        subtitle: "Forge checkpoints after each one, so long work stays inspectable instead of one giant leap."
      )
      if mission.milestones.isEmpty {
        EmptyStateView(text: "No milestones are recorded for this mission yet.")
      } else {
        ForEach(mission.milestones) { milestone in
          MissionMilestoneRow(milestone: milestone) { event in
            Task { await store.checkpointSelectedMission(event: event, milestoneID: milestone.milestoneID) }
          }
        }
      }
    }
  }

  private var relatedRunsSection: some View {
    VStack(alignment: .leading, spacing: 10) {
      SectionHeader(title: "Runs in this mission", subtitle: "Open a run to watch it work.")
      LazyVGrid(columns: [GridItem(.adaptive(minimum: 200), spacing: 10)], alignment: .leading, spacing: 10) {
        ForEach(mission.relatedRunIDs, id: \.self) { runID in
          Button {
            store.select(.forge(.runs))
            store.openRun(runID)
          } label: {
            HStack(spacing: 8) {
              Image(systemName: "play.rectangle")
                .foregroundStyle(.secondary)
              Text(runID)
                .font(.caption.monospaced())
                .lineLimit(1)
                .truncationMode(.middle)
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .mdxGlassSurface(interactive: true)
          }
          .buttonStyle(.plain)
        }
      }
    }
  }

  private var detailsSection: some View {
    VStack(alignment: .leading, spacing: 10) {
      SectionHeader(title: "Shape and boundary", subtitle: "The scope, guardrails, and finish line Forge agreed to before it started.")
      VStack(alignment: .leading, spacing: 10) {
        if !mission.doneWhen.isEmpty { InfoRow(label: "Done when", value: mission.doneWhen) }
        if !mission.allowedWriteScope.isEmpty { InfoRow(label: "Can write", value: mission.allowedWriteScope) }
        if !mission.blockedPaths.isEmpty { InfoRow(label: "Off limits", value: mission.blockedPaths) }
        if !mission.nonGoals.isEmpty { InfoRow(label: "Not doing", value: mission.nonGoals) }
        if !mission.validationCommands.isEmpty { InfoRow(label: "Proof", value: mission.validationCommands) }
      }
      .padding(14)
      .mdxGlassSurface()
    }
  }
}

struct MissionMilestoneRow: View {
  let milestone: MissionMilestone
  let checkpoint: (String) -> Void

  var body: some View {
    HStack(alignment: .top, spacing: 12) {
      Image(systemName: symbol)
        .font(.callout)
        .foregroundStyle(tint)
        .frame(width: 20)
      VStack(alignment: .leading, spacing: 4) {
        HStack(alignment: .firstTextBaseline) {
          Text(milestone.title)
            .font(.subheadline.weight(.medium))
          Spacer(minLength: 8)
          StatusPill(status: milestone.statusLabel, tone: milestoneTone)
        }
        if !milestone.acceptanceCriteria.isEmpty {
          Text(milestone.acceptanceCriteria)
            .font(.caption)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
        if !milestone.validationCommand.isEmpty {
          Text(milestone.validationCommand)
            .font(.caption.monospaced())
            .foregroundStyle(.tertiary)
            .lineLimit(1)
        }
      }
      if !milestone.isDone {
        Menu {
          Button("Mark started") { checkpoint("milestone_started") }
          Button("Mark complete") { checkpoint("milestone_completed") }
          Button("Mark blocked") { checkpoint("milestone_blocked") }
        } label: {
          Image(systemName: "ellipsis.circle")
            .foregroundStyle(.secondary)
        }
        .menuStyle(.borderlessButton)
        .fixedSize()
      }
    }
    .padding(12)
    .frame(maxWidth: .infinity, alignment: .leading)
    .mdxGlassSurface()
  }

  private var symbol: String {
    if milestone.isDone { return "checkmark.circle.fill" }
    if milestone.isBlocked { return "exclamationmark.triangle" }
    return "circle"
  }

  private var tint: Color {
    if milestone.isDone { return .green }
    if milestone.isBlocked { return .orange }
    return .secondary
  }

  private var milestoneTone: StatusTone {
    if milestone.isDone { return .positive }
    if milestone.isBlocked { return .locked }
    return .neutral
  }
}

struct MissionSetupSheet: View {
  @Environment(OperatorStore.self) private var store
  @Environment(\.dismiss) private var dismiss
  @State private var didAutoCreateShot = false

  var body: some View {
    @Bindable var store = store
    return ScrollView {
      VStack(alignment: .leading, spacing: 16) {
        VStack(alignment: .leading, spacing: 5) {
          Text("Set up a mission")
            .font(.title2.weight(.semibold))
          Text("Long-horizon work runs as a mission: a goal, milestones with a checkpoint after each, a budget, and a fleet of workers. Shape it once, then let it run.")
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }

        recommendationSummary

        field("Goal") {
          TextField("What should this mission accomplish?", text: $store.missionSetupDraft.goal, axis: .vertical)
            .textFieldStyle(.roundedBorder)
            .lineLimit(2...4)
        }
        field("Done when") {
          TextField("The checks pass and every step has its receipt.", text: $store.missionSetupDraft.doneWhen)
            .textFieldStyle(.roundedBorder)
        }
        field("Checks to prove each milestone (comma separated)") {
          TextField("make local-smoke", text: $store.missionSetupDraft.validationCommands)
            .textFieldStyle(.roundedBorder)
        }
        field("Can write") {
          TextField("Paths the mission may change", text: $store.missionSetupDraft.allowedWriteScope)
            .textFieldStyle(.roundedBorder)
        }

        HStack(spacing: 16) {
          Stepper("Workers: \(store.missionSetupDraft.fleetWidth)", value: $store.missionSetupDraft.fleetWidth, in: 1...24)
          Stepper("Checkpoint every \(store.missionSetupDraft.checkpointCadenceMinutes) min", value: $store.missionSetupDraft.checkpointCadenceMinutes, in: 5...120, step: 5)
        }
        HStack(spacing: 16) {
          Stepper("Budget: $\(store.missionSetupDraft.maxCostDollars)", value: $store.missionSetupDraft.maxCostDollars, in: 1...500)
          Stepper("Time cap: \(store.missionSetupDraft.maxRuntimeHours)h", value: $store.missionSetupDraft.maxRuntimeHours, in: 1...24)
        }

        if let outcome = store.missionActionResult, outcome.title == "Create mission", outcome.isRefusal {
          RunActionBanner(outcome: outcome)
        }

        HStack(spacing: 10) {
          Spacer()
          Button("Cancel") { dismiss() }
            .keyboardShortcut(.cancelAction)
          Button {
            Task {
              let outcome = await store.createMission(draft: store.missionSetupDraft)
              if outcome?.isRefusal == false { dismiss() }
            }
          } label: {
            Label(store.missionActionInFlight ? "Setting up" : "Set up mission", systemImage: "flag.checkered")
          }
          .mdxPrimaryButtonStyle()
          .disabled(store.missionActionInFlight || store.missionSetupDraft.goal.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
      }
      .padding(22)
    }
    .frame(width: 640, height: 640)
    .task {
      autoCreateForScreenshotIfRequested()
    }
  }

  private func field<Control: View>(_ title: String, @ViewBuilder _ control: () -> Control) -> some View {
    VStack(alignment: .leading, spacing: 5) {
      Text(title)
        .font(.caption.weight(.medium))
        .foregroundStyle(.secondary)
      control()
    }
  }

  @ViewBuilder
  private var recommendationSummary: some View {
    let draft = store.missionSetupDraft
    if !draft.recommendationLabel.isEmpty || !draft.recommendationReason.isEmpty {
      HStack(alignment: .top, spacing: 10) {
        Image(systemName: "wand.and.stars")
          .foregroundStyle(Color.accentColor)
          .padding(.top, 1)
        VStack(alignment: .leading, spacing: 5) {
          if !draft.recommendationLabel.isEmpty {
            Text("Forge suggests: \(draft.recommendationLabel)")
              .font(.callout.weight(.semibold))
          }
          if !draft.recommendationReason.isEmpty {
            Text(draft.recommendationReason)
              .font(.callout)
              .foregroundStyle(.secondary)
              .fixedSize(horizontal: false, vertical: true)
          }
          if !draft.recommendationReadsAs.isEmpty {
            Text(draft.recommendationReadsAs)
              .font(.caption)
              .foregroundStyle(.tertiary)
          }
        }
        Spacer(minLength: 0)
      }
      .padding(12)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(
        RoundedRectangle(cornerRadius: 10, style: .continuous)
          .fill(Color.accentColor.opacity(0.08))
      )
    }
  }

  private func autoCreateForScreenshotIfRequested() {
    guard ProcessInfo.processInfo.environment["MDX_SHOT_AUTOSTART_MISSION"] == "1" else { return }
    guard !didAutoCreateShot else { return }
    guard !store.missionSetupDraft.goal.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
    didAutoCreateShot = true
    Task {
      try? await Task.sleep(nanoseconds: shotMissionDelayNanoseconds)
      if Task.isCancelled { return }
      guard !store.missionActionInFlight else { return }
      let outcome = await store.createMission(draft: store.missionSetupDraft)
      if outcome?.isRefusal == false { dismiss() }
    }
  }

  private var shotMissionDelayNanoseconds: UInt64 {
    let raw = ProcessInfo.processInfo.environment["MDX_SHOT_AUTOSTART_DELAY_MS"] ?? ""
    let milliseconds = UInt64(raw) ?? 1_200
    return min(max(milliseconds, 300), 20_000) * 1_000_000
  }
}
