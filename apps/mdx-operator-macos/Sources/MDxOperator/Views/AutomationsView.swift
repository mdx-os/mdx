import SwiftUI

struct AutomationTemplate: Identifiable {
  let id: String
  let title: String
  let icon: String
  let subtitle: String
  let draft: MissionDraft

  static let presets: [AutomationTemplate] = [
    AutomationTemplate(
      id: "daily-brief", title: "Daily brief", icon: "sun.max",
      subtitle: "Each morning, gather what changed across Forge runs, Pages, and decisions into one read.",
      draft: MissionDraft(
        goal: "Produce a daily brief of what changed across Forge runs, Pages, and decisions.",
        doneWhen: "A dated brief page is published summarizing the last day's activity.",
        validationCommands: "make local-smoke", fleetWidth: 1, maxCostDollars: 3,
        maxRuntimeHours: 1, checkpointCadenceMinutes: 15)
    ),
    AutomationTemplate(
      id: "weekly-review", title: "Weekly review", icon: "calendar",
      subtitle: "Weekly, review shipped work and surface the open questions worth a human call.",
      draft: MissionDraft(
        goal: "Review shipped work over the last week and list the open questions that need a decision.",
        doneWhen: "A weekly review page is published with shipped items and open questions.",
        validationCommands: "make local-smoke", fleetWidth: 1, maxCostDollars: 5,
        maxRuntimeHours: 2, checkpointCadenceMinutes: 30)
    ),
    AutomationTemplate(
      id: "repo-monitor", title: "Repo monitor", icon: "waveform.path.ecg",
      subtitle: "Watch the repo for failing checks and flaky tests, and flag what needs attention.",
      draft: MissionDraft(
        goal: "Monitor the repo for failing checks and flaky tests and flag regressions.",
        doneWhen: "A monitor report lists failing checks and suspected flaky tests.",
        validationCommands: "make local-smoke", fleetWidth: 1, maxCostDollars: 5,
        maxRuntimeHours: 2, checkpointCadenceMinutes: 20)
    )
  ]
}

struct AutomationsView: View {
  @Environment(OperatorStore.self) private var store

  var body: some View {
    VStack(spacing: 0) {
      header
      Divider()
      ScrollView {
        VStack(alignment: .leading, spacing: 22) {
          ForgeTrailMapView(
            scale: "Mission",
            title: "Milestones, each ending in proof",
            detail: "One goal moves through bounded milestones, checks, and human decisions.",
            active: missionTrailStage
          )
          if store.missions.isEmpty {
            emptyState
          } else {
            activeMissions
          }
          // Quick-starts survive as a section inside the Missions lane, not a
          // separate surface.
          templatesSection
        }
        .padding(28)
        .frame(maxWidth: SurfaceMetrics.streamWidth, alignment: .leading)
        .frame(maxWidth: .infinity, alignment: .leading)
      }
    }
    .task { await store.loadMissions() }
  }

  private var missionTrailStage: ForgeTrailStage {
    if store.missions.contains(where: { !$0.isActive }) { return .review }
    if store.missions.contains(where: { $0.isActive && $0.milestones.contains(where: { $0.statusLabel == "In progress" }) }) { return .build }
    if store.missions.contains(where: { $0.completedMilestoneCount > 0 || $0.blockedMilestoneCount > 0 }) { return .prove }
    return .plan
  }

  private var header: some View {
    HStack(spacing: 16) {
      Image(systemName: "flag.checkered")
        .font(.callout)
        .foregroundStyle(Color.accentColor)
      VStack(alignment: .leading, spacing: 1) {
        Text("Missions")
          .font(.headline)
        Text("Long-horizon work Forge runs with a checkpoint after each milestone and a budget.")
          .font(.caption)
          .foregroundStyle(.secondary)
          .lineLimit(1)
      }

      Spacer()

      Button {
        store.select(.twin)
      } label: {
        Label("Start from chat", systemImage: "sparkles")
          .font(.callout)
      }
      .buttonStyle(.plain)
      .foregroundStyle(.secondary)

      Button {
        store.missionSetupDraft = MissionDraft()
        store.showMissionSetup = true
      } label: {
        Label("New mission", systemImage: "plus")
          .font(.callout.weight(.medium))
      }
      .buttonStyle(.borderedProminent)
      .controlSize(.small)
    }
    .padding(.horizontal, 20)
    .padding(.vertical, 12)
  }

  @ViewBuilder
  private var activeMissions: some View {
    VStack(alignment: .leading, spacing: 12) {
      Text("Running and scheduled")
        .font(.subheadline.weight(.semibold))
        .foregroundStyle(.secondary)
      ForEach(store.missions) { mission in
        MissionCard(mission: mission) { store.openMission(mission.id) }
      }
    }
  }

  private var emptyState: some View {
    VStack(spacing: 16) {
      Image(systemName: "flag.checkered")
        .font(SurfaceType.emptyGlyph)
        .foregroundStyle(.tertiary)
      Text("Start your first mission")
        .font(SurfaceType.title)
      Text("A mission is longer work Forge runs across milestones, checking in after each one and staying inside a budget you set.")
        .font(.callout)
        .foregroundStyle(.secondary)
        .multilineTextAlignment(.center)
        .frame(maxWidth: 460)
    }
    .frame(maxWidth: .infinity)
    .padding(.top, 40)
  }

  private var templatesSection: some View {
    VStack(alignment: .leading, spacing: 12) {
      Text("Start from a template")
        .font(.subheadline.weight(.semibold))
        .foregroundStyle(.secondary)
      Text("Pick a starting point. You review and approve the mission before anything runs.")
        .font(.callout)
        .foregroundStyle(.tertiary)
      LazyVGrid(columns: [GridItem(.adaptive(minimum: 240), spacing: 12)], spacing: 12) {
        ForEach(AutomationTemplate.presets) { preset in
          TemplateCard(template: preset) { start(preset) }
        }
      }
    }
  }

  private func start(_ template: AutomationTemplate) {
    store.missionSetupDraft = template.draft
    store.showMissionSetup = true
  }
}

struct MissionCard: View {
  let mission: ForgeMission
  let open: () -> Void
  @State private var hovering = false

  var body: some View {
    Button(action: open) {
      VStack(alignment: .leading, spacing: 8) {
        HStack(spacing: 8) {
          Image(systemName: "clock.arrow.2.circlepath")
            .font(.caption)
            .foregroundStyle(Color.accentColor)
          Text(mission.goal.isEmpty ? mission.id : mission.goal)
            .font(.headline)
            .lineLimit(1)
          Spacer()
          if !mission.state.isEmpty {
            Text(mission.state.replacingOccurrences(of: "_", with: " ").capitalized)
              .font(.caption2.weight(.medium))
              .foregroundStyle(.secondary)
              .padding(.horizontal, 7).padding(.vertical, 2)
              .background(Capsule().fill(Color.primary.opacity(0.06)))
          }
        }
        if !mission.latestCheckpointSummary.isEmpty {
          Text(mission.latestCheckpointSummary)
            .font(.callout)
            .foregroundStyle(.secondary)
            .lineLimit(2)
            .fixedSize(horizontal: false, vertical: true)
        }
        HStack(spacing: 14) {
          if mission.checkpointCount > 0 {
            Label("\(mission.checkpointCount) checkpoints", systemImage: "checkmark.seal")
              .font(.caption2).foregroundStyle(.tertiary)
          }
          if mission.completedMilestoneCount > 0 {
            Label("\(mission.completedMilestoneCount) done", systemImage: "flag.checkered")
              .font(.caption2).foregroundStyle(.tertiary)
          }
          if mission.blockedMilestoneCount > 0 {
            Label("\(mission.blockedMilestoneCount) blocked", systemImage: "exclamationmark.triangle")
              .font(.caption2).foregroundStyle(.orange)
          }
        }
      }
      .padding(16)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(
        RoundedRectangle(cornerRadius: 12, style: .continuous)
          .fill(hovering ? Color.primary.opacity(0.05) : Color.primary.opacity(0.03))
      )
      .overlay(
        RoundedRectangle(cornerRadius: 12, style: .continuous)
          .stroke(Color.secondary.opacity(0.12), lineWidth: 1)
      )
    }
    .buttonStyle(.plain)
    .onHover { hovering = $0 }
  }
}

struct TemplateCard: View {
  let template: AutomationTemplate
  let use: () -> Void
  @State private var hovering = false

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack(spacing: 8) {
        Image(systemName: template.icon)
          .font(.callout)
          .foregroundStyle(Color.accentColor)
        Text(template.title)
          .font(.headline)
        Spacer()
      }
      Text(template.subtitle)
        .font(.caption)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
      Button(action: use) {
        Text("Use template")
          .font(.caption.weight(.medium))
      }
      .buttonStyle(.borderedProminent)
      .controlSize(.small)
      .padding(.top, 2)
    }
    .padding(16)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .fill(hovering ? Color.primary.opacity(0.05) : Color.primary.opacity(0.03))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .stroke(Color.secondary.opacity(0.12), lineWidth: 1)
    )
    .onHover { hovering = $0 }
  }
}
