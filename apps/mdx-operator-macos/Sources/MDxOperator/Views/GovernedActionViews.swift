import AppKit
import SwiftUI

enum GovernedField: Hashable {
  case intent, planHash, scope, workerProfile, decision, runID, host, workerCount
}

struct GovernedActionPanel: View {
  @Binding var selectedAction: GovernedActionKind
  @Binding var draft: GovernedActionDraft
  let result: GovernedActionResult?
  let inFlight: GovernedActionKind?
  let preferredActions: [GovernedActionKind]
  var title: String = "Governed action"
  let submit: (GovernedActionKind) -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      HStack(alignment: .firstTextBaseline, spacing: 12) {
        VStack(alignment: .leading, spacing: 4) {
          Text(title)
            .font(.title3.weight(.semibold))
          Text(selectedAction.plainLanguageBoundary)
            .font(.callout)
            .foregroundStyle(.secondary)
        }
        Spacer(minLength: 12)
        Picker("Action", selection: $selectedAction) {
          ForEach(preferredActions) { action in
            Text(action.title).tag(action)
          }
        }
        .labelsHidden()
        .pickerStyle(.menu)
        .frame(maxWidth: 220)
      }

      if fields.contains(.intent) {
        labeled("What should MDx do?") {
          TextField("Describe the change in plain language", text: $draft.intent, axis: .vertical)
            .textFieldStyle(.roundedBorder)
            .lineLimit(2...4)
        }
      }

      let scalars = fields.filter { $0 != .intent }
      if !scalars.isEmpty {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 200), spacing: 12)], alignment: .leading, spacing: 12) {
          ForEach(scalars, id: \.self) { field in
            scalarField(field)
          }
        }
      }

      Divider()

      HStack(spacing: 10) {
        if requiredFieldMissing {
          Label("Name a run first", systemImage: "exclamationmark.circle")
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        Spacer(minLength: 0)
        Button {
          submit(selectedAction)
        } label: {
          Label(inFlight == selectedAction ? "Recording" : selectedAction.title, systemImage: inFlight == selectedAction ? "hourglass" : "checkmark.seal")
        }
        .mdxPrimaryButtonStyle()
        .disabled(inFlight != nil || requiredFieldMissing)
      }

      if let result {
        ActionResultView(result: result)
      }
    }
    .padding(16)
    .mdxGlassSurface(interactive: true)
    .onAppear(perform: normalizeSelection)
    .onChange(of: preferredActions) { _, _ in normalizeSelection() }
  }

  private var fields: [GovernedField] {
    switch selectedAction {
    case .requestBuild: return [.intent, .planHash]
    case .startRun: return [.intent]
    case .runFleet: return [.workerCount]
    case .approveBuild: return [.scope, .planHash]
    case .provePlan: return [.intent, .planHash]
    case .requestWorkerAuthority: return [.workerProfile, .intent]
    case .talentSignoff: return [.workerProfile, .scope, .intent]
    case .humanSignoff: return [.decision, .scope, .intent]
    case .sourceHostReadiness: return [.runID, .host]
    case .prHandoff: return [.runID, .host]
    }
  }

  @ViewBuilder
  private func scalarField(_ field: GovernedField) -> some View {
    switch field {
    case .intent:
      EmptyView()
    case .planHash:
      labeled("Plan hash") {
        TextField("plan-proof-...", text: $draft.planHash)
          .textFieldStyle(.roundedBorder)
      }
    case .scope:
      labeled("What you're approving") {
        VStack(alignment: .leading, spacing: 4) {
          TextField("scope", text: $draft.scope)
            .textFieldStyle(.roundedBorder)
          if !draft.scope.isEmpty {
            // The raw scope token is the receipt-level truth; keep it, but quiet.
            Text("Recorded as \(draft.scope)")
              .font(.caption2.monospaced())
              .foregroundStyle(.tertiary)
              .textSelection(.enabled)
          }
        }
      }
    case .workerProfile:
      labeled("Worker profile") {
        TextField("build_agent", text: $draft.workerProfile)
          .textFieldStyle(.roundedBorder)
      }
    case .runID:
      labeled("Run id") {
        TextField("Name the run to check", text: $draft.runID)
          .textFieldStyle(.roundedBorder)
      }
    case .host:
      labeled("Target host") {
        TextField("github", text: $draft.targetHost)
          .textFieldStyle(.roundedBorder)
      }
    case .decision:
      labeled("Decision") {
        Picker("Decision", selection: $draft.decision) {
          Text("Ratify").tag("ratify")
          Text("Reject").tag("reject")
          Text("Ask for revision").tag("request_revision")
          Text("Rerun tests").tag("request_tests_rerun")
          Text("Escalate").tag("escalate")
        }
        .labelsHidden()
        .pickerStyle(.menu)
      }
    case .workerCount:
      labeled("Workers") {
        Stepper("\(draft.workerCount)", value: $draft.workerCount, in: 1...8)
      }
    }
  }

  @ViewBuilder
  private func labeled<Control: View>(_ title: String, @ViewBuilder _ control: () -> Control) -> some View {
    VStack(alignment: .leading, spacing: 5) {
      Text(title)
        .font(.caption.weight(.medium))
        .foregroundStyle(.secondary)
      control()
    }
  }

  private func normalizeSelection() {
    if !preferredActions.contains(selectedAction), let first = preferredActions.first {
      selectedAction = first
    }
  }

  private var requiredFieldMissing: Bool {
    [.sourceHostReadiness, .prHandoff].contains(selectedAction)
      && draft.runID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
  }
}

struct ActionResultView: View {
  let result: GovernedActionResult

  var body: some View {
    VStack(alignment: .leading, spacing: 7) {
      HStack {
        Text(result.title)
          .font(.headline)
        Spacer()
        StatusPill(status: result.status, tone: result.status.localizedCaseInsensitiveContains("REFUSED") || result.status.localizedCaseInsensitiveContains("FAILED") ? .locked : .positive)
      }
      Text(result.detail)
        .font(.callout)
        .foregroundStyle(.secondary)
      HStack {
        Text(result.route)
          .font(.caption.monospaced())
          .foregroundStyle(.tertiary)
        if !result.receiptID.isEmpty {
          Text(result.receiptID)
            .font(.caption.monospaced())
            .foregroundStyle(.tertiary)
            .lineLimit(1)
        }
      }
    }
    .padding(12)
    .mdxGlassSurface()
  }
}

struct RunStageTile: View {
  let stage: RunStageItem

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack {
        Text(stage.title)
          .font(.headline)
        Spacer()
        StatusPill(status: stage.status, tone: stage.status.localizedCaseInsensitiveContains("held") ? .locked : .neutral)
      }
      Text(stage.detail)
        .font(.callout)
        .foregroundStyle(.secondary)
        .lineLimit(3)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(14)
    .mdxGlassSurface()
  }
}

struct ReviewArtifactTile: View {
  let item: ReviewArtifact

  private var isLocked: Bool {
    item.status.localizedCaseInsensitiveContains("locked")
  }

  private var pillLabel: String {
    isLocked ? "Locked until proof" : "Open"
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack {
        Text(item.title)
          .font(.headline)
        Spacer()
        StatusPill(status: pillLabel, tone: isLocked ? .locked : .positive)
      }
      Text(item.detail)
        .font(.callout)
        .foregroundStyle(.secondary)
        .lineLimit(3)
      if !item.route.isEmpty {
        DisclosureGroup("Details") {
          Text(item.route)
            .font(.caption.monospaced())
            .foregroundStyle(.tertiary)
            .textSelection(.enabled)
            .lineLimit(1)
            .padding(.top, 4)
        }
        .font(.caption)
      }
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(14)
    .mdxGlassSurface()
  }
}

struct HostProjectPanel: View {
  let projects: [HostProject]
  let add: (String) -> Void
  let remove: (HostProject) -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      SectionHeader(title: "Projects on this Mac", subtitle: "Keep the local work roots visible before MDx is asked to work in them.")
      Button {
        chooseFolder()
      } label: {
        Label("Add project folder", systemImage: "folder.badge.plus")
      }

      if projects.isEmpty {
        EmptyStateView(text: "No local project folder has been added yet.")
      } else {
        ForEach(projects) { project in
          HStack {
            Image(systemName: "folder")
              .foregroundStyle(.secondary)
            Text(project.path)
              .textSelection(.enabled)
              .lineLimit(1)
            Spacer()
            Button {
              remove(project)
            } label: {
              Image(systemName: "minus.circle")
            }
            .buttonStyle(.plain)
            .help("Remove this project from the native app list.")
            .accessibilityLabel("Remove this project")
          }
          .padding(12)
          .mdxGlassSurface(interactive: true)
        }
      }
    }
  }

  private func chooseFolder() {
    let panel = NSOpenPanel()
    panel.canChooseFiles = false
    panel.canChooseDirectories = true
    panel.allowsMultipleSelection = false
    panel.prompt = "Add"
    if panel.runModal() == .OK, let url = panel.url {
      add(url.path)
    }
  }
}

struct SurfaceRow: View {
  let item: SurfaceItem

  var body: some View {
    HStack(alignment: .top, spacing: 12) {
      Image(systemName: item.status.localizedCaseInsensitiveContains("locked") ? "lock.circle" : "circle.grid.2x2")
        .font(.title3)
        .foregroundStyle(item.status.localizedCaseInsensitiveContains("locked") ? Color.orange : Color.secondary)
      VStack(alignment: .leading, spacing: 5) {
        HStack(alignment: .firstTextBaseline) {
          Text(humanLabel(item.title))
            .font(.headline)
          Spacer()
          StatusPill(status: item.status, tone: item.status.localizedCaseInsensitiveContains("locked") ? .locked : .neutral)
        }
        Text(item.subtitle)
          .font(.callout)
          .foregroundStyle(.secondary)
          .lineLimit(2)
        Text(item.detail)
          .font(.caption)
          .foregroundStyle(.secondary)
          .lineLimit(3)
        Text(item.route)
          .font(.caption.monospaced())
          .foregroundStyle(.tertiary)
          .lineLimit(1)
      }
    }
    .padding(14)
    .mdxGlassSurface()
  }
}

struct ActionPreview: View {
  let title: String
  let detail: String
  let locked: Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack {
        Text(title)
          .font(.headline)
        Spacer()
        Image(systemName: locked ? "lock" : "checkmark.circle")
          .foregroundStyle(locked ? Color.orange : Color.green)
      }
      Text(detail)
        .font(.callout)
        .foregroundStyle(.secondary)
        .lineLimit(3)
      StatusPill(status: locked ? "Connect first" : "Ready", tone: locked ? .locked : .positive)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(14)
    .mdxGlassSurface(interactive: true)
  }
}
