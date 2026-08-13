import MDxOperatorShared
import SwiftUI

struct YouView: View {
  @EnvironmentObject private var store: ForgeAnywhereStore
  @EnvironmentObject private var auth: MobileCloudAuthStore
  @Environment(\.dynamicTypeSize) private var dynamicTypeSize
  @State private var showingLocalDelete = false
  @State private var showingAccountDelete = false

  var body: some View {
    NavigationStack {
      List {
        if auth.requiresCloudSignIn {
          Section("Account") {
            LabeledContent(
              "Signed in as",
              value: auth.displayName.isEmpty ? "You" : auth.displayName
            )
            if !auth.email.isEmpty {
              Text(auth.email)
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            Button("Sign out") { Task { await auth.signOut() } }
          }
        }

        Section {
          VStack(alignment: .leading, spacing: 8) {
            MobileEyebrow(title: "Mobile command", symbol: "iphone.gen3")
            Text(store.snapshot.safeStatus)
              .font(.headline)
            Text(
              "Your execution targets, trust boundaries, interruptions, and private mobile data."
            )
            .font(.subheadline)
            .foregroundStyle(.secondary)
          }
          .padding(.vertical, 5)
        }

        Section("Execution") {
          Label(
            countLabel(
              store.snapshot.pairedHostCount, singular: "paired Mac", plural: "paired Macs"),
            systemImage: "laptopcomputer.and.iphone"
          )
          Label(
            countLabel(
              store.snapshot.readyCloudEnvironmentCount,
              singular: "cloud environment ready",
              plural: "cloud environments ready"
            ),
            systemImage: "cloud.fill")
          NavigationLink {
            CloudSetupView()
              .mobileFocusedDestination()
          } label: {
            Label("MDx Cloud setup", systemImage: "cloud.badge.plus")
          }
        }

        Section("Trust") {
          NavigationLink {
            TrustedDevicesView(accessTokenProvider: store.mobileAccessTokenProvider)
              .mobileFocusedDestination()
          } label: {
            Label("Trusted devices", systemImage: "laptopcomputer.and.iphone")
          }
          Label("Face ID for consequential actions", systemImage: "faceid")
          Label("Long-lived provider secrets stay off this device", systemImage: "key.slash")
        }

        Section("Interruptions") {
          Button {
            Task { await store.enableNotifications() }
          } label: {
            Label("Enable high-signal notifications", systemImage: "bell.badge")
          }
          Text(store.notificationStatus)
            .font(.caption)
            .foregroundStyle(.secondary)
          Text("Only needs-you, review-ready, and failed-after-recovery events are eligible.")
            .font(.caption)
            .foregroundStyle(.secondary)
        }

        Section("Support") {
          NavigationLink(
            "Connection diagnostics",
            destination: MobileDiagnosticsView().mobileFocusedDestination())
          NavigationLink(
            "Privacy and retention", destination: MobilePrivacyView().mobileFocusedDestination())
        }

        if !store.offlineDrafts.isEmpty {
          Section {
            ForEach(store.offlineDrafts) { draft in
              VStack(alignment: .leading, spacing: 8) {
                Text(draftTitle(draft)).font(.headline)
                Text(draft.createdAt, style: .relative)
                  .font(.caption)
                  .foregroundStyle(.secondary)
                ViewThatFits(in: .horizontal) {
                  HStack {
                    draftActions(draft)
                  }
                  VStack(alignment: .leading) {
                    draftActions(draft)
                  }
                }
              }
            }
          } header: {
            Text("Offline drafts")
          } footer: {
            Text("Drafts stay on this iPhone and never send automatically.")
          }
        }

        Section("Data controls") {
          Button("Delete local mobile data", role: .destructive) {
            showingLocalDelete = true
          }
          Button("Request account deletion", role: .destructive) {
            showingAccountDelete = true
          }
        }
      }
      .navigationTitle("You")
      .mobileAdaptiveNavigationTitle(dynamicTypeSize)
      .safeAreaPadding(.bottom, 56)
      .accessibilityIdentifier("you.list")
      .confirmationDialog(
        "Delete local MDx data from this iPhone?",
        isPresented: $showingLocalDelete,
        titleVisibility: .visible
      ) {
        Button("Delete local data", role: .destructive) { store.deleteLocalMobileData() }
      } message: {
        Text(
          "This deletes offline drafts, cached events, and temporary credentials on this iPhone.")
      }
      .confirmationDialog(
        "Request account deletion?",
        isPresented: $showingAccountDelete,
        titleVisibility: .visible
      ) {
        Button("Confirm with Face ID", role: .destructive) {
          Task { await store.requestAccountDeletion() }
        }
      } message: {
        Text(
          "MDx records the request, deletes local mobile data, and reports the account as pending until identity and retained-storage erasure are fulfilled."
        )
      }
    }
  }

  private func draftTitle(_ draft: MobileOfflineDraft) -> String {
    switch draft.kind {
    case .startBuild: draft.goal ?? "New build draft"
    case .followUp: draft.instruction ?? "Follow-up draft"
    }
  }

  @ViewBuilder
  private func draftActions(_ draft: MobileOfflineDraft) -> some View {
    Button("Send") { Task { await store.sendOfflineDraft(draft) } }
      .buttonStyle(.borderedProminent)
      .disabled(store.snapshot.connection != .connected)
    Button("Delete", role: .destructive) { store.deleteOfflineDraft(draft.id) }
      .buttonStyle(.bordered)
  }

  private func countLabel(_ count: Int, singular: String, plural: String) -> String {
    "\(count) \(count == 1 ? singular : plural)"
  }
}

private struct MobileDiagnosticsView: View {
  @EnvironmentObject private var store: ForgeAnywhereStore

  var body: some View {
    List {
      if let diagnostics = store.supportDiagnostics {
        Section("Connection") {
          LabeledContent(
            "Relay", value: diagnostics.relayConfigured ? "Configured" : "Not configured")
          LabeledContent(
            "Hosted sandbox",
            value: diagnostics.hostedSandboxConfigured ? "Configured" : "Not configured")
          LabeledContent("Reviewable sessions", value: "\(diagnostics.reviewableSessionCount)")
          if let lastReceiptAt = diagnostics.lastReceiptAt {
            LabeledContent("Last server event") { Text(lastReceiptAt, style: .relative) }
          }
        }
        Section("Safe support export") {
          ForEach(diagnostics.safeExportFields, id: \.self) { Label($0, systemImage: "checkmark") }
          Text("Source, diffs, raw logs, prompts, tokens, and credentials are excluded.")
            .font(.caption)
            .foregroundStyle(.secondary)
          if let export = store.safeSupportExport {
            ShareLink(item: export) {
              Label("Share safe diagnostics", systemImage: "square.and.arrow.up")
            }
          }
        }
      } else {
        ProgressView("Loading diagnostics")
      }
    }
    .navigationTitle("Diagnostics")
    .navigationBarTitleDisplayMode(.inline)
    .mobileFocusedDestination()
    .task { await store.refreshSupportAndPrivacy() }
    .refreshable { await store.refreshSupportAndPrivacy() }
  }
}

private struct MobilePrivacyView: View {
  @EnvironmentObject private var store: ForgeAnywhereStore

  var body: some View {
    List {
      if let privacy = store.privacyProjection {
        Section("On this iPhone") {
          Text(privacy.offlineDrafts)
          Text(privacy.mobileProjectionRetention)
        }
        Section("MDx services") {
          Text(privacy.relayEventRetention)
          Text(privacy.hostedWorkspaceRetention)
        }
        Section("Deletion") {
          Text(
            "Account deletion remains pending until the identity provider and retained-storage erasure workers confirm fulfillment. MDx does not silently claim deletion."
          )
        }
      } else {
        ProgressView("Loading privacy details")
      }
    }
    .navigationTitle("Privacy")
    .navigationBarTitleDisplayMode(.inline)
    .mobileFocusedDestination()
    .task { await store.refreshSupportAndPrivacy() }
  }
}

struct CloudSetupView: View {
  @EnvironmentObject private var store: ForgeAnywhereStore
  @Environment(\.openURL) private var openURL

  var body: some View {
    List {
      Section {
        Label(store.cloudStatus, systemImage: statusSymbol)
          .foregroundStyle(.secondary)
        if store.cloudOperationInFlight {
          ProgressView("Securing your cloud setup")
        }
      } header: {
        Text("Build from anywhere")
      } footer: {
        Text(
          "MDx requests access only to repositories you select. Installation tokens and secret values never live on this iPhone."
        )
      }

      if let setup = store.cloudSetup,
        let installation = setup.installations.first(where: { $0.status == "active" })
      {
        Section("GitHub") {
          LabeledContent("Account", value: installation.accountLogin)
          Label("GitHub App connected", systemImage: "checkmark.seal.fill")
            .foregroundStyle(.green)
        }

        Section {
          if store.cloudRepositories.isEmpty {
            ContentUnavailableView(
              "No repositories selected",
              systemImage: "folder.badge.questionmark",
              description: Text("Choose repositories in the GitHub App installation, then refresh.")
            )
          } else {
            ForEach(store.cloudRepositories) { repository in
              VStack(alignment: .leading, spacing: 8) {
                Text(repository.fullName).font(.headline)
                Text(repository.defaultBranch ?? "Default branch")
                  .font(.caption)
                  .foregroundStyle(.secondary)
                Button("Connect repository") {
                  Task {
                    await store.connectCloudRepository(
                      repository, installationID: installation.installationID)
                  }
                }
                .buttonStyle(.bordered)
                .disabled(store.cloudOperationInFlight)
              }
              .padding(.vertical, 4)
            }
          }
          Button("Refresh repositories") {
            Task { await store.loadCloudRepositories(installationID: installation.installationID) }
          }
        } header: {
          Text("Available repositories")
        } footer: {
          Text("MDx cannot see repositories outside this GitHub App installation.")
        }
      } else {
        Section {
          Button {
            Task {
              guard let url = await store.beginGitHubConnection() else { return }
              openURL(url)
            }
          } label: {
            Label("Connect GitHub", systemImage: "arrow.up.right.square")
          }
          .disabled(!store.canConfigureCloud || store.cloudOperationInFlight)
        } footer: {
          Text("No personal access token. You choose the repositories in GitHub.")
        }
      }

      if let setup = store.cloudSetup, !setup.repositories.isEmpty {
        Section("Connected") {
          ForEach(setup.repositories) { repository in
            VStack(alignment: .leading, spacing: 8) {
              Label(repository.fullName, systemImage: "externaldrive.connected.to.line.below")
              if let revision = repository.sourceRevision {
                Text(MobileFormatters.shortRevision(revision))
                  .font(.caption.monospaced())
                  .foregroundStyle(.secondary)
              }
              if let repoID = repository.repoID {
                Button("Prepare environment") {
                  Task { await store.prepareCloudEnvironment(repoID: repoID) }
                }
                .buttonStyle(.borderedProminent)
                .disabled(store.cloudOperationInFlight)
              }
            }
            .padding(.vertical, 4)
          }
        }
      }

      if let environments = store.cloudSetup?.environments, !environments.isEmpty {
        Section("Environments") {
          ForEach(environments) { environment in
            VStack(alignment: .leading, spacing: 6) {
              HStack {
                Text(environment.repoID).font(.headline)
                Spacer()
                Image(
                  systemName: environment.readyForCloudBuilds
                    ? "checkmark.circle.fill" : "clock.badge.checkmark"
                )
                .foregroundStyle(environment.readyForCloudBuilds ? .green : .orange)
              }
              Text(environmentStatus(environment.status))
                .font(.subheadline)
                .foregroundStyle(.secondary)
              Text(MobileFormatters.shortRevision(environment.fingerprint))
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
              if matchesVerificationRetry(environment.status) {
                Button("Verify in MDx Cloud") {
                  Task { await store.verifyCloudEnvironment(repoID: environment.repoID) }
                }
                .buttonStyle(.borderedProminent)
                .disabled(store.cloudOperationInFlight)
              }
            }
          }
        }
      }
    }
    .navigationTitle("MDx Cloud")
    .navigationBarTitleDisplayMode(.inline)
    .mobileFocusedDestination()
    .task { await store.refreshCloudSetup() }
    .refreshable { await store.refreshCloudSetup() }
    .accessibilityIdentifier("cloud.setup")
  }

  private var statusSymbol: String {
    if store.snapshot.readyCloudEnvironmentCount > 0 { return "checkmark.circle.fill" }
    if store.cloudOperationInFlight { return "arrow.triangle.2.circlepath" }
    return "cloud"
  }

  private func environmentStatus(_ status: String) -> String {
    switch status {
    case "VERIFIED": "Ready for cloud builds"
    case "PREPARED_VERIFICATION_REQUIRED": "Prepared, verification required"
    case "VERIFICATION_FAILED": "Verification failed, safe to retry"
    case "CONFIGURATION_REQUIRED": "Pinned base image required"
    default: status.replacingOccurrences(of: "_", with: " ").capitalized
    }
  }

  private func matchesVerificationRetry(_ status: String) -> Bool {
    status == "PREPARED_VERIFICATION_REQUIRED" || status == "VERIFICATION_FAILED"
  }
}
