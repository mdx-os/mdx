import MDxOperatorShared
import SwiftUI

struct ForgeView: View {
  private enum Field: Hashable {
    case intent
    case repository
  }

  @EnvironmentObject private var store: ForgeAnywhereStore
  @Environment(\.dynamicTypeSize) private var dynamicTypeSize
  @FocusState private var focusedField: Field?
  @State private var intent = ""
  @State private var repositoryID = ""
  @State private var selectedTarget: ExecutionTargetKind = .pairedHost

  var body: some View {
    NavigationStack {
      ScrollView {
        LazyVStack(alignment: .leading, spacing: 18) {
          VStack(alignment: .leading, spacing: 6) {
            MobileEyebrow(title: "Governed build", symbol: "hammer.fill")
            Text("Brief the outcome. Forge handles the work and stops where your judgment begins.")
              .font(.subheadline)
              .foregroundStyle(.secondary)
          }

          buildBriefCard
          targetCard
          repositoryCard
          startCard

          if let feedback = store.commandFeedback {
            Label(feedback, systemImage: "checkmark.circle.fill")
              .foregroundStyle(.secondary)
              .mobileCard()
              .accessibilityIdentifier("forge.feedback")
          }
        }
        .mobileScrollContent()
      }
      .scrollDismissesKeyboard(.interactively)
      .navigationTitle("Forge")
      .mobileAdaptiveNavigationTitle(dynamicTypeSize)
      .onAppear(perform: selectDefaultRepository)
      .onChange(of: selectedTarget) { _, _ in
        repositoryID = ""
        selectDefaultRepository()
      }
      .onChange(of: repositoryOptions) { _, _ in selectDefaultRepository() }
      .toolbar {
        ToolbarItemGroup(placement: .keyboard) {
          Spacer()
          Button("Done") { focusedField = nil }
        }
      }
    }
  }

  private var buildBriefCard: some View {
    VStack(alignment: .leading, spacing: 14) {
      MobileSectionHeader(
        "What should Forge build?",
        detail: "Describe a concrete outcome, not a sequence of implementation steps."
      )

      TextField("Build a focused outcome...", text: $intent, axis: .vertical)
        .lineLimit(4...9)
        .focused($focusedField, equals: .intent)
        .padding(12)
        .background(.quaternary, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .accessibilityLabel("Build brief")
        .accessibilityIdentifier("forge.intent")

      Label("Dictation works well for a hands-free brief", systemImage: "mic.fill")
        .font(.caption)
        .foregroundStyle(.secondary)
    }
    .mobileCard()
  }

  private var targetCard: some View {
    VStack(alignment: .leading, spacing: 12) {
      MobileSectionHeader(
        "Run on", detail: "Choose where the governed worker executes."
      )

      VStack(spacing: 10) {
        targetOption(
          .pairedHost, title: "Your Mac", detail: "Continue on a paired Mac",
          symbol: "laptopcomputer")
        targetOption(
          .mdxCloud, title: "MDx Cloud", detail: "Run in a verified environment",
          symbol: "cloud.fill", disabled: !store.canStartCloudBuild)
      }
    }
    .mobileCard()
  }

  @ViewBuilder
  private var repositoryCard: some View {
    VStack(alignment: .leading, spacing: 12) {
      MobileSectionHeader(
        "Repository", detail: "The source boundary for this build."
      )

      if repositoryOptions.isEmpty {
        TextField("Repository ID", text: $repositoryID)
          .textInputAutocapitalization(.never)
          .autocorrectionDisabled()
          .submitLabel(.done)
          .focused($focusedField, equals: .repository)
          .onSubmit { focusedField = nil }
          .padding(12)
          .background(.quaternary, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
          .accessibilityIdentifier("forge.repository")

        if selectedTarget == .mdxCloud {
          NavigationLink {
            CloudSetupView()
              .mobileFocusedDestination()
          } label: {
            Label("Set up MDx Cloud", systemImage: "cloud.badge.plus")
              .frame(maxWidth: .infinity)
          }
          .buttonStyle(.bordered)
        }
      } else {
        Picker("Repository", selection: $repositoryID) {
          ForEach(repositoryOptions) { repository in
            Text(repository.displayName).tag(repository.id)
          }
        }
        .pickerStyle(.menu)
        .accessibilityIdentifier("forge.repository")

        if selectedProofCommands.isEmpty {
          Label(
            "No proof command is configured for this repository.",
            systemImage: "exclamationmark.triangle.fill"
          )
          .font(.caption)
          .foregroundStyle(.orange)
          .accessibilityIdentifier("forge.proof-plan")
        } else {
          Label(
            "Proof: \(selectedProofCommands.joined(separator: " then "))",
            systemImage: "checkmark.shield.fill"
          )
          .font(.caption)
          .foregroundStyle(.secondary)
          .accessibilityIdentifier("forge.proof-plan")
        }
      }
    }
    .mobileCard()
  }

  private var startCard: some View {
    VStack(alignment: .leading, spacing: 12) {
      Button {
        focusedField = nil
        Task {
          await store.startBuild(
            goal: intent.trimmingCharacters(in: .whitespacesAndNewlines),
            repositoryID: repositoryID.trimmingCharacters(in: .whitespacesAndNewlines),
            targetKind: selectedTarget
          )
          if store.lastCommandSucceeded { intent = "" }
        }
      } label: {
        Group {
          if store.isCommandInFlight {
            ProgressView()
          } else {
            Label("Start build", systemImage: "arrow.up.circle.fill")
          }
        }
        .frame(maxWidth: .infinity)
      }
      .buttonStyle(.borderedProminent)
      .controlSize(.large)
      .disabled(startBuildDisabled)
      .accessibilityIdentifier("forge.start-build")

      Label(boundaryCopy, systemImage: boundarySymbol)
        .font(.caption)
        .foregroundStyle(.secondary)
        .accessibilityIdentifier("forge.boundary")

      if let readiness = store.modelReadiness {
        Label(
          readiness.ready
            ? "Model ready on the selected execution environment"
            : readiness.recommendedNextAction,
          systemImage: readiness.ready ? "checkmark.seal.fill" : "exclamationmark.triangle.fill"
        )
        .font(.caption)
        .foregroundStyle(readiness.ready ? .green : .orange)
        .accessibilityIdentifier("forge.model-readiness")
      }
    }
    .mobileCard()
  }

  private func targetOption(
    _ target: ExecutionTargetKind,
    title: String,
    detail: String,
    symbol: String,
    disabled: Bool = false
  ) -> some View {
    Button {
      selectedTarget = target
    } label: {
      HStack(spacing: 12) {
        Image(systemName: symbol)
          .font(.title3)
          .frame(width: 28)
        VStack(alignment: .leading, spacing: 2) {
          Text(title).font(.headline)
          Text(detail).font(.caption).foregroundStyle(.secondary)
        }
        Spacer()
        Image(
          systemName: selectedTarget == target ? "checkmark.circle.fill" : "circle"
        )
        .foregroundStyle(selectedTarget == target ? AnyShapeStyle(.tint) : AnyShapeStyle(.tertiary))
      }
      .padding(12)
      .contentShape(Rectangle())
      .background(
        selectedTarget == target ? Color.accentColor.opacity(0.1) : Color.primary.opacity(0.035),
        in: RoundedRectangle(cornerRadius: 14, style: .continuous)
      )
    }
    .buttonStyle(.plain)
    .disabled(disabled)
    .opacity(disabled ? 0.45 : 1)
    .accessibilityLabel("\(title), \(detail)")
    .accessibilityAddTraits(selectedTarget == target ? .isSelected : [])
    .accessibilityIdentifier("forge.target.\(target.rawValue)")
  }

  private var startBuildDisabled: Bool {
    intent.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      || repositoryID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
      || (selectedTarget == .pairedHost && !store.canSendCommands
        && store.snapshot.connection != .offline)
      || (selectedTarget == .mdxCloud && !cloudRepositoryReady
        && store.snapshot.connection != .offline)
      || selectedTarget == .customerManaged
      || (store.snapshot.connection != .offline && store.modelReadiness?.ready == false)
      || (store.snapshot.connection != .offline && selectedProofCommands.isEmpty)
      || store.isCommandInFlight
  }

  private var boundaryCopy: String {
    if store.snapshot.connection == .offline {
      return "This becomes a protected draft on this iPhone. It never sends automatically."
    }
    if selectedTarget == .mdxCloud {
      return "The verified cloud sandbox runs proof and still stops at the human ship boundary."
    }
    if store.canSendCommands {
      return "Forge starts on your paired Mac and stops at the human ship boundary."
    }
    return "Pair this iPhone with MDx on your Mac before starting remote builds."
  }

  private var boundarySymbol: String {
    store.snapshot.connection == .offline ? "lock.iphone" : "lock.shield"
  }

  private var repositoryOptions: [MobileBuildRepositoryOption] {
    store.buildRepositories(for: selectedTarget)
  }

  private var selectedProofCommands: [String] {
    store.buildProofCommands(repositoryID: repositoryID, targetKind: selectedTarget)
  }

  private func selectDefaultRepository() {
    guard !repositoryOptions.isEmpty else { return }
    if !repositoryOptions.contains(where: { $0.id == repositoryID }) {
      repositoryID =
        store.snapshot.sessions.first(where: { session in
          repositoryOptions.contains(where: {
            $0.id == session.repositoryID && !$0.suggestedChecks.isEmpty
          })
        })?.repositoryID
        ?? repositoryOptions.first(where: { !$0.suggestedChecks.isEmpty })?.id
        ?? repositoryOptions[0].id
    }
  }

  private var cloudRepositoryReady: Bool {
    store.cloudEnvironmentReady(
      repoID: repositoryID.trimmingCharacters(in: .whitespacesAndNewlines))
  }
}
