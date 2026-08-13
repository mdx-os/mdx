import Foundation
import MDxOperatorShared
import OSLog
import Security
import UIKit
import UserNotifications

struct MobileBuildRepositoryOption: Equatable, Identifiable {
  let id: String
  let displayName: String
  let suggestedChecks: [String]
}

@MainActor
final class ForgeAnywhereStore: ObservableObject {
  @Published private(set) var snapshot: ForgeAnywhereSnapshot
  @Published private(set) var eventsBySession: [String: [MobileRelayEnvelope]] = [:]
  @Published private(set) var needsYou: [MobileNeedsYouItem] = []
  @Published private(set) var commandFeedback: String?
  @Published private(set) var notificationStatus = "Off"
  @Published private(set) var cloudSetup: MobileCloudSetupProjection?
  @Published private(set) var cloudRepositories: [MobileCloudRepository] = []
  @Published private(set) var cloudStatus = "Connect GitHub to build in MDx Cloud"
  @Published private(set) var cloudOperationInFlight = false
  @Published private(set) var followedSessionID: String?
  @Published private(set) var isCommandInFlight = false
  @Published private(set) var offlineDrafts: [MobileOfflineDraft]
  @Published private(set) var lastRefreshAt: Date?
  @Published private(set) var lastActionURL: URL?
  @Published private(set) var supportDiagnostics: MobileSupportDiagnostics?
  @Published private(set) var privacyProjection: MobilePrivacyProjection?
  @Published private(set) var handoffTargets: MobileHandoffTargets?
  @Published private(set) var modelReadiness: ModelAccessReadiness?
  @Published private(set) var lastCommandSucceeded = false
  @Published var selectedSessionID: String?

  private let apiURL: URL?
  private let fixtureMode: Bool
  private let accessTokenProvider: MobileAccessTokenProvider?
  private let identityManager = MobileDeviceIdentityManager(namespace: "ios")
  private let credentialStore: any SecureCredentialStoring = KeychainCredentialStore()
  private var relayTask: Task<Void, Never>?
  private var cursors: [String: Int] = [:]
  private var connectionGeneration = 0
  private var activeTenantID: String?
  private var activeUserID: String?
  private var activeDeviceID: String?
  private var activeHostID: String?
  private var activeHostName: String?
  private let biometricAuthorizer = BiometricDecisionAuthorizer()
  private let liveActivity = ForgeLiveActivityCoordinator()
  private let logger = Logger(subsystem: "com.mdx.anywhere", category: "forge-anywhere")
  private let draftStore = OfflineDraftStore()
  private var lastActionSessionID: String?
  private var startedSessionReconciliationTask: Task<Void, Never>?
  private var acceptedSessions: [String: ForgeWorkSession] = [:]

  var canSendCommands: Bool {
    !fixtureMode && apiURL != nil && activeDeviceID != nil && activeHostID != nil
  }

  var canEnableNotifications: Bool {
    !fixtureMode && apiURL != nil && activeDeviceID != nil
      && (activeHostID != nil || (cloudSetup?.readyEnvironmentCount ?? 0) > 0)
  }

  var canStartCloudBuild: Bool {
    !fixtureMode && apiURL != nil && activeDeviceID != nil
      && (cloudSetup?.readyEnvironmentCount ?? 0) > 0
  }

  func cloudEnvironmentReady(repoID: String) -> Bool {
    cloudSetup?.environments.contains(where: {
      $0.repoID == repoID && $0.readyForCloudBuilds
    }) ?? false
  }

  func buildRepositories(for targetKind: ExecutionTargetKind) -> [MobileBuildRepositoryOption] {
    let options: [MobileBuildRepositoryOption]
    switch targetKind {
    case .pairedHost:
      options =
        handoffTargets?.pairedHostRepositories
        .filter(\.rootRecorded)
        .map {
          MobileBuildRepositoryOption(
            id: $0.repositoryID,
            displayName: $0.displayName.isEmpty ? $0.repositoryID : $0.displayName,
            suggestedChecks: $0.suggestedChecks ?? []
          )
        } ?? []
    case .mdxCloud:
      options =
        cloudSetup?.environments
        .filter(\.readyForCloudBuilds)
        .map { environment in
          let name = cloudSetup?.repositories.first(where: {
            $0.repoID == environment.repoID
          })?.fullName
          return MobileBuildRepositoryOption(
            id: environment.repoID,
            displayName: name ?? environment.repoID,
            suggestedChecks: []
          )
        } ?? []
    case .customerManaged:
      options = []
    }
    return Dictionary(grouping: options, by: \.id)
      .compactMap { $0.value.first }
      .sorted {
        $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending
      }
  }

  var canConfigureCloud: Bool { !fixtureMode && apiURL != nil }

  var mobileAccessTokenProvider: MobileAccessTokenProvider? { accessTokenProvider }

  func actionURL(for sessionID: String) -> URL? {
    lastActionSessionID == sessionID ? lastActionURL : nil
  }

  var safeSupportExport: String? {
    guard let diagnostics = supportDiagnostics else { return nil }
    let appVersion =
      Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString")
      as? String ?? "unknown"
    let buildNumber =
      Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String
      ?? "unknown"
    let refresh = lastRefreshAt.map { ISO8601DateFormatter().string(from: $0) } ?? "never"
    return """
      MDx mobile diagnostics
      app_version: \(appVersion) (\(buildNumber))
      device_model: \(UIDevice.current.model)
      os_version: \(UIDevice.current.systemName) \(UIDevice.current.systemVersion)
      connection_state: \(snapshot.connection.rawValue)
      last_refresh_at: \(refresh)
      tenant_fingerprint: \(diagnostics.tenantFingerprint)
      forge_event_count: \(diagnostics.forgeEventCount)
      reviewable_session_count: \(diagnostics.reviewableSessionCount)
      relay_configured: \(diagnostics.relayConfigured)
      hosted_sandbox_configured: \(diagnostics.hostedSandboxConfigured)
      excluded: source,diffs,raw_logs,prompts,tokens,credentials,secret_values
      """
  }

  func canControl(_ session: ForgeWorkSession) -> Bool {
    switch session.activeTarget.kind {
    case .mdxCloud: !fixtureMode && apiURL != nil && activeDeviceID != nil
    case .pairedHost: canSendCommands
    case .customerManaged: false
    }
  }

  func alternateTarget(for session: ForgeWorkSession) -> ExecutionTarget? {
    switch session.activeTarget.kind {
    case .pairedHost:
      guard
        let source = handoffTargets?.pairedHostRepositories.first(where: {
          $0.repositoryID == session.repositoryID && !$0.identityFingerprint.isEmpty
        }),
        let environment = handoffTargets?.cloudEnvironments.first(where: {
          $0.verified && $0.identityFingerprint == source.identityFingerprint
        })
      else { return nil }
      return ExecutionTarget(
        kind: .mdxCloud,
        targetID: environment.environmentID,
        tenantID: session.tenantID,
        displayName: "MDx Cloud",
        capabilityRevision: 1
      )
    case .mdxCloud:
      guard
        let activeHostID,
        let source = handoffTargets?.cloudEnvironments.first(where: {
          ($0.repositoryID == session.repositoryID
            || $0.environmentID == session.activeTarget.targetID)
            && $0.verified && !$0.identityFingerprint.isEmpty
        }),
        handoffTargets?.pairedHostRepositories.contains(where: {
          $0.identityFingerprint == source.identityFingerprint
        }) == true
      else { return nil }
      return ExecutionTarget(
        kind: .pairedHost,
        targetID: activeHostID,
        tenantID: session.tenantID,
        displayName: activeHostName ?? "Your Mac",
        capabilityRevision: 1
      )
    case .customerManaged:
      return nil
    }
  }

  init(
    snapshot: ForgeAnywhereSnapshot,
    apiURL: URL? = nil,
    fixtureMode: Bool = true,
    accessTokenProvider: MobileAccessTokenProvider? = nil
  ) {
    self.snapshot = snapshot
    self.apiURL = apiURL
    self.fixtureMode = fixtureMode
    self.accessTokenProvider = accessTokenProvider
    offlineDrafts = fixtureMode ? [] : OfflineDraftStore().load()
    selectedSessionID = snapshot.sessions.first?.id
    cursors = Dictionary(uniqueKeysWithValues: snapshot.sessions.map { ($0.id, $0.replayCursor) })
  }

  static func fromLaunchEnvironment(
    _ environment: [String: String] = ProcessInfo.processInfo.environment,
    accessTokenProvider: MobileAccessTokenProvider? = nil
  ) -> ForgeAnywhereStore {
    if let requested = environment["MDX_FIXTURE_SCENARIO"],
      let fixture = ForgeAnywhereFixture(rawValue: requested)
    {
      return ForgeAnywhereStore(snapshot: fixture.snapshot)
    }
    let environmentURL = environment["MDX_API_URL"].flatMap(URL.init(string:))
    let apiURL = environmentURL ?? MobileCloudConfiguration.current?.hostedAPIURL
    let initial =
      apiURL == nil ? ForgeAnywhereFixture.active.snapshot : ForgeAnywhereFixture.loading.snapshot
    return ForgeAnywhereStore(
      snapshot: initial,
      apiURL: apiURL,
      fixtureMode: false,
      accessTokenProvider: accessTokenProvider
    )
  }

  private func forgeClient(baseURL: URL) -> MobileForgeClient {
    MobileForgeClient(
      baseURL: baseURL,
      accessTokenProvider: accessTokenProvider
    )
  }

  private func pairingClient(baseURL: URL) -> MobilePairingClient {
    MobilePairingClient(
      baseURL: baseURL,
      accessTokenProvider: accessTokenProvider
    )
  }

  func connectIfConfigured() async {
    guard !fixtureMode, let apiURL else { return }
    if let identity = try? identityManager.loadOrCreate() {
      activeDeviceID = identity.deviceID
    }
    await refreshCloudSetup()
    await refreshHandoffTargets()
    await refreshModelReadiness()
    relayTask?.cancel()
    relayTask = Task { [weak self] in
      await self?.runRelay(apiURL: apiURL)
    }
  }

  func refreshForReview() async {
    await refreshProjection()
  }

  func refreshCloudSetup() async {
    guard let apiURL, !fixtureMode else { return }
    do {
      let projection = try await forgeClient(baseURL: apiURL).cloudSetup()
      guard !projection.secretValuesIncluded, !projection.productionWriteAllowed else {
        throw MobileForgeClientError.unsafeRelayEnvelope
      }
      cloudSetup = projection
      cloudStatus = cloudStatusText(projection)
      updateReadyCloudEnvironmentCount(projection.readyEnvironmentCount)
      if let installation = projection.installations.first(where: { $0.status == "active" }) {
        await loadCloudRepositories(installationID: installation.installationID)
      }
      await refreshHandoffTargets()
    } catch {
      cloudStatus = error.localizedDescription
    }
  }

  func beginGitHubConnection() async -> URL? {
    guard let apiURL, canConfigureCloud else {
      cloudStatus = "Connect MDx to its control plane first."
      return nil
    }
    cloudOperationInFlight = true
    defer { cloudOperationInFlight = false }
    do {
      let session = try await forgeClient(baseURL: apiURL).beginGitHubInstallation()
      guard !session.personalAccessTokenRequested, !session.productionWriteAllowed else {
        throw MobileForgeClientError.unsafeRelayEnvelope
      }
      try credentialStore.save(
        Data(session.state.utf8),
        account: "github-installation-state",
        accessibility: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly)
      cloudStatus = "Finish selecting repositories in GitHub, then return to MDx."
      return session.installURL
    } catch {
      cloudStatus = error.localizedDescription
      return nil
    }
  }

  func handleCloudCallback(_ url: URL) async {
    guard url.scheme?.lowercased() == "mdx",
      url.host?.lowercased() == "github-installed",
      let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
      let installationText = components.queryItems?.first(where: {
        $0.name == "installation_id"
      })?.value,
      let installationID = UInt64(installationText),
      let apiURL
    else { return }
    let callbackState = components.queryItems?.first(where: { $0.name == "state" })?.value
    let storedState = (try? credentialStore.load(account: "github-installation-state"))
      .flatMap { String(data: $0, encoding: .utf8) }
    guard let state = callbackState ?? storedState, !state.isEmpty else {
      cloudStatus = "This GitHub return link is no longer bound to this iPhone. Start again."
      return
    }
    cloudOperationInFlight = true
    defer { cloudOperationInFlight = false }
    do {
      let result = try await forgeClient(baseURL: apiURL).completeGitHubInstallation(
        state: state, installationID: installationID)
      guard !result.tokenRecorded, !result.productionWriteAllowed else {
        throw MobileForgeClientError.unsafeRelayEnvelope
      }
      try? credentialStore.delete(account: "github-installation-state")
      cloudStatus = "\(result.accountLogin) is connected. Choose a repository."
      await refreshCloudSetup()
    } catch {
      cloudStatus = error.localizedDescription
    }
  }

  func loadCloudRepositories(installationID: UInt64) async {
    guard let apiURL else { return }
    do {
      let result = try await forgeClient(baseURL: apiURL).githubRepositories(
        installationID: installationID)
      guard !result.tokenRecorded, !result.productionWriteAllowed else {
        throw MobileForgeClientError.unsafeRelayEnvelope
      }
      cloudRepositories = result.repositories
    } catch {
      cloudRepositories = []
      cloudStatus = error.localizedDescription
    }
  }

  func connectCloudRepository(_ repository: MobileCloudRepository, installationID: UInt64) async {
    guard let apiURL else { return }
    cloudOperationInFlight = true
    defer { cloudOperationInFlight = false }
    do {
      let connection = try await forgeClient(baseURL: apiURL).connectCloudRepository(
        installationID: installationID, repositoryID: repository.repositoryID)
      guard !connection.tokenRecorded, !connection.productionWriteAllowed else {
        throw MobileForgeClientError.unsafeRelayEnvelope
      }
      cloudStatus = "\(repository.fullName) is connected. Prepare its build environment next."
      await refreshCloudSetup()
    } catch {
      cloudStatus = error.localizedDescription
    }
  }

  func prepareCloudEnvironment(repoID: String) async {
    guard let apiURL else { return }
    cloudOperationInFlight = true
    defer { cloudOperationInFlight = false }
    do {
      let result = try await forgeClient(baseURL: apiURL).prepareCloudEnvironment(
        repoID: repoID)
      guard !result.secretValuesRecorded, !result.productionWriteAllowed
      else {
        throw MobileForgeClientError.unsafeRelayEnvelope
      }
      cloudStatus = result.nextSafeAction
      await refreshCloudSetup()
    } catch {
      cloudStatus = error.localizedDescription
    }
  }

  func verifyCloudEnvironment(repoID: String) async {
    guard let apiURL else { return }
    cloudOperationInFlight = true
    defer { cloudOperationInFlight = false }
    do {
      let result = try await forgeClient(baseURL: apiURL).verifyCloudEnvironment(
        repoID: repoID)
      guard !result.secretValuesRecorded, !result.productionWriteAllowed
      else {
        throw MobileForgeClientError.unsafeRelayEnvelope
      }
      cloudStatus = result.nextSafeAction
      await refreshCloudSetup()
    } catch {
      cloudStatus = error.localizedDescription
    }
  }

  func useFixture(_ fixture: ForgeAnywhereFixture) {
    relayTask?.cancel()
    snapshot = fixture.snapshot
    selectedSessionID = snapshot.sessions.first?.id
    cursors = Dictionary(uniqueKeysWithValues: snapshot.sessions.map { ($0.id, $0.replayCursor) })
    eventsBySession = [:]
    acceptedSessions = [:]
  }

  func events(for sessionID: String) -> [MobileRelayEnvelope] {
    eventsBySession[sessionID] ?? []
  }

  func startBuild(
    goal: String,
    repositoryID: String,
    targetKind: ExecutionTargetKind = .pairedHost
  ) async {
    lastCommandSucceeded = false
    if snapshot.connection == .offline {
      saveOfflineDraft(
        MobileOfflineDraft(
          kind: .startBuild,
          repositoryID: repositoryID,
          goal: goal,
          targetKind: targetKind
        ))
      commandFeedback = "Draft saved on this iPhone. Send it explicitly after reconnecting."
      return
    }
    let sessionID = "forge_run_mobile_\(UUID().uuidString.lowercased())"
    let cloudEnvironmentID = cloudSetup?.environments.first(where: {
      $0.repoID == repositoryID && $0.readyForCloudBuilds
    })?.environmentID
    let allowedCommands = buildProofCommands(
      repositoryID: repositoryID,
      targetKind: targetKind
    )
    await execute(
      kind: "start_build",
      sessionID: sessionID,
      expectedVersion: 1,
      payload: MobileCommandPayload(
        goal: goal,
        repositoryID: repositoryID,
        allowedCommands: allowedCommands,
        fleetWidth: 1
      ),
      requestedTargetKind: targetKind,
      requestedTargetID: cloudEnvironmentID
    )
  }

  func buildProofCommands(
    repositoryID: String,
    targetKind: ExecutionTargetKind
  ) -> [String] {
    buildRepositories(for: targetKind)
      .first(where: { $0.id == repositoryID })?
      .suggestedChecks ?? []
  }

  func followUp(session: ForgeWorkSession, instruction: String) async {
    lastCommandSucceeded = false
    if snapshot.connection == .offline {
      saveOfflineDraft(
        MobileOfflineDraft(
          kind: .followUp,
          sessionID: session.sessionID,
          instruction: instruction,
          targetKind: session.activeTarget.kind
        ))
      commandFeedback = "Follow-up saved on this iPhone. It will not send automatically."
      return
    }
    await execute(
      kind: "follow_up",
      sessionID: session.sessionID,
      expectedVersion: session.sessionVersion,
      payload: MobileCommandPayload(instruction: instruction)
    )
  }

  func requestHandoff(session: ForgeWorkSession, destination: ExecutionTarget) async {
    lastCommandSucceeded = false
    guard let destinationRepositoryID = handoffRepositoryID(for: session, destination: destination)
    else {
      commandFeedback =
        "MDx could not prove that this Mac and cloud target contain the same repository."
      return
    }
    do {
      try await biometricAuthorizer.verify(
        reason: "Move this build through an exact branch and evidence checkpoint."
      )
      await execute(
        kind: "request_handoff",
        sessionID: session.sessionID,
        expectedVersion: session.sessionVersion,
        payload: MobileCommandPayload(
          destinationTarget: destination,
          destinationRepositoryID: destinationRepositoryID
        ),
        userPresence: "biometric_verified"
      )
    } catch {
      commandFeedback = error.localizedDescription
    }
  }

  func requestChanges(review: ReviewDigest, comment: String) async {
    lastCommandSucceeded = false
    guard let session = snapshot.sessions.first(where: { $0.sessionID == review.sessionID }) else {
      commandFeedback = "Refresh this review before requesting changes."
      return
    }
    await execute(
      kind: "request_changes",
      sessionID: session.sessionID,
      expectedVersion: session.sessionVersion,
      payload: MobileCommandPayload(comment: comment)
    )
  }

  func approveAndOpenPullRequest(review: ReviewDigest, reason: String) async {
    lastCommandSucceeded = false
    lastActionURL = nil
    lastActionSessionID = nil
    guard let session = snapshot.sessions.first(where: { $0.sessionID == review.sessionID }) else {
      commandFeedback = "Refresh this review before opening its pull request."
      return
    }
    let reviewerReason = reason.trimmingCharacters(in: .whitespacesAndNewlines)
    guard reviewerReason.count >= 8 else {
      commandFeedback = "Add a short reason explaining why this commit is ready."
      return
    }
    guard let baseBranch = recordedDefaultBranch(for: session) else {
      commandFeedback =
        "Reconnect or refresh this repository so MDx can record its default branch."
      return
    }
    do {
      try await biometricAuthorizer.verify(
        reason: "Approve this exact commit and open its governed pull request."
      )
      await execute(
        kind: "approve_and_open_pr",
        sessionID: session.sessionID,
        expectedVersion: session.sessionVersion,
        payload: MobileCommandPayload(
          reason: reviewerReason,
          commitSHA: review.commitSHA,
          targetHost: "github",
          baseBranch: baseBranch
        ),
        userPresence: "biometric_verified"
      )
      if lastActionURL == nil {
        lastCommandSucceeded = false
      }
    } catch {
      commandFeedback = error.localizedDescription
    }
  }

  private func recordedDefaultBranch(for session: ForgeWorkSession) -> String? {
    let branch: String?
    switch session.activeTarget.kind {
    case .pairedHost:
      let repository = handoffTargets?.pairedHostRepositories.first {
        $0.repositoryID == session.repositoryID
      }
      branch =
        repository?.defaultBranchSource == "current_head_fallback"
        ? nil : repository?.defaultBranch
    case .mdxCloud:
      branch =
        cloudSetup?.repositories.first {
          $0.repoID == session.repositoryID
        }?.defaultBranch
    case .customerManaged:
      branch = nil
    }
    let value = branch?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    return value.isEmpty ? nil : value
  }

  func sendOfflineDraft(_ draft: MobileOfflineDraft) async {
    guard snapshot.connection == .connected else {
      commandFeedback = "Reconnect before sending this draft."
      return
    }
    switch draft.kind {
    case .startBuild:
      await startBuild(
        goal: draft.goal ?? "",
        repositoryID: draft.repositoryID ?? "",
        targetKind: draft.targetKind ?? .pairedHost
      )
    case .followUp:
      guard let sessionID = draft.sessionID,
        let session = snapshot.sessions.first(where: { $0.sessionID == sessionID })
      else {
        commandFeedback = "The session for this draft is no longer available."
        return
      }
      await followUp(session: session, instruction: draft.instruction ?? "")
    }
    if lastCommandSucceeded { deleteOfflineDraft(draft.id) }
  }

  func deleteOfflineDraft(_ id: String) {
    offlineDrafts.removeAll { $0.id == id }
    try? draftStore.save(offlineDrafts)
  }

  func deleteLocalMobileData() {
    closeAuthenticatedSession()
    snapshot = ForgeAnywhereSnapshot(
      connection: .offline,
      safeStatus: "Local mobile data deleted",
      sessions: [],
      attention: [],
      reviews: [],
      pairedHostCount: 0,
      readyCloudEnvironmentCount: 0
    )
    commandFeedback = "Local MDx mobile data was deleted from this iPhone."
  }

  func suspendCloudAccess() {
    relayTask?.cancel()
    relayTask = nil
    startedSessionReconciliationTask?.cancel()
    startedSessionReconciliationTask = nil
    try? credentialStore.delete(account: "mobile-relay-token")
    eventsBySession = [:]
    cursors = [:]
    needsYou = []
    acceptedSessions = [:]
    cloudSetup = nil
    cloudRepositories = []
    cloudStatus = "Sign in to reconnect MDx Cloud"
    cloudOperationInFlight = false
    handoffTargets = nil
    modelReadiness = nil
    supportDiagnostics = nil
    privacyProjection = nil
    followedSessionID = nil
    selectedSessionID = nil
    activeDeviceID = nil
    activeTenantID = nil
    activeUserID = nil
    activeHostID = nil
    activeHostName = nil
    lastActionURL = nil
    lastActionSessionID = nil
    lastRefreshAt = nil
    lastCommandSucceeded = false
    isCommandInFlight = false
    notificationStatus = "Off"
    snapshot = ForgeAnywhereSnapshot(
      connection: .offline,
      safeStatus: "Sign in to reconnect",
      sessions: [],
      attention: [],
      reviews: [],
      pairedHostCount: 0,
      readyCloudEnvironmentCount: 0
    )
    commandFeedback = nil
  }

  func closeAuthenticatedSession() {
    suspendCloudAccess()
    try? draftStore.deleteAll()
    try? credentialStore.delete(account: "github-installation-state")
    try? identityManager.revokeLocalIdentity()
    offlineDrafts = []
  }

  func refreshSupportAndPrivacy() async {
    guard let apiURL else { return }
    do {
      let client = forgeClient(baseURL: apiURL)
      async let diagnostics = client.supportDiagnostics()
      async let privacy = client.privacy()
      async let targets = client.handoffTargets()
      let (diagnosticsValue, privacyValue, targetsValue) = try await (
        diagnostics, privacy, targets
      )
      guard !diagnosticsValue.rawSourceIncluded,
        !diagnosticsValue.credentialValuesIncluded,
        !privacyValue.silentAccountDeletionClaimAllowed,
        !targetsValue.rawRootsIncluded,
        !targetsValue.originValuesIncluded,
        !targetsValue.containsSecretValues,
        !targetsValue.grantsExecutionAuthority,
        !targetsValue.productionWriteAllowed
      else {
        throw MobileForgeClientError.unsafeRelayEnvelope
      }
      supportDiagnostics = diagnosticsValue
      privacyProjection = privacyValue
      handoffTargets = targetsValue
    } catch {
      commandFeedback = error.localizedDescription
    }
  }

  func requestAccountDeletion() async {
    guard let apiURL else { return }
    do {
      try await biometricAuthorizer.verify(
        reason: "Request deletion of your MDx account and retained mobile data."
      )
      let identity = try identityManager.loadOrCreate()
      var request = MobileAccountDeletionRequest(actorDeviceID: identity.deviceID)
      request.deviceSignature = try identityManager.signAccountDeletionAttestation(request)
      let result = try await forgeClient(baseURL: apiURL).requestAccountDeletion(request)
      deleteLocalMobileData()
      try? identityManager.revokeLocalIdentity()
      commandFeedback = result.nextSafeAction
    } catch {
      commandFeedback = error.localizedDescription
    }
  }

  func pause(session: ForgeWorkSession) async {
    await execute(
      kind: "pause",
      sessionID: session.sessionID,
      expectedVersion: session.sessionVersion,
      payload: MobileCommandPayload(reason: "Paused from iPhone")
    )
  }

  func resume(session: ForgeWorkSession) async {
    await execute(
      kind: "resume",
      sessionID: session.sessionID,
      expectedVersion: session.sessionVersion,
      payload: MobileCommandPayload(reason: "Resumed from iPhone")
    )
  }

  func stop(session: ForgeWorkSession) async {
    await execute(
      kind: "stop",
      sessionID: session.sessionID,
      expectedVersion: session.sessionVersion,
      payload: MobileCommandPayload(reason: "Stopped from iPhone")
    )
  }

  func approve(_ item: MobileNeedsYouItem) async {
    guard item.mobileActionSupported,
      let sessionID = item.sessionID,
      let expectedVersion = item.expectedSessionVersion,
      let decisionKind = item.decisionKind,
      let receiptID = item.decisionSubjectReceiptID
    else {
      commandFeedback = "This decision needs the larger review surface on your Mac."
      return
    }
    do {
      try await biometricAuthorizer.verify(
        reason: "Confirm the reviewed Forge plan. This does not start workers or ship code."
      )
      await execute(
        kind: "record_decision",
        sessionID: sessionID,
        expectedVersion: expectedVersion,
        payload: MobileCommandPayload(
          reason: "Reviewed and confirmed with Face ID on iPhone",
          fleetID: item.subjectID,
          decisionKind: decisionKind,
          decisionSubjectReceiptID: receiptID
        ),
        userPresence: "biometric_verified"
      )
    } catch {
      commandFeedback = error.localizedDescription
    }
  }

  func followLiveActivity(session: ForgeWorkSession) {
    do {
      try liveActivity.follow(session, summary: snapshot.safeStatus)
      followedSessionID = session.sessionID
      commandFeedback = "Live Activity started for this build."
    } catch {
      commandFeedback = error.localizedDescription
    }
  }

  func stopLiveActivity(sessionID: String) {
    liveActivity.end(sessionID: sessionID)
    if followedSessionID == sessionID { followedSessionID = nil }
  }

  func enableNotifications() async {
    if activeDeviceID == nil,
      let identity = try? identityManager.loadOrCreate()
    {
      activeDeviceID = identity.deviceID
    }
    if !canEnableNotifications {
      await refreshCloudSetup()
      await refreshHandoffTargets()
    }
    guard canEnableNotifications else {
      notificationStatus = "Waiting for a paired Mac or verified MDx Cloud. Retry stays available."
      return
    }
    do {
      let granted = try await UNUserNotificationCenter.current().requestAuthorization(
        options: [.alert, .badge, .sound]
      )
      guard granted else {
        notificationStatus = "Not allowed"
        return
      }
      notificationStatus = "Registering"
      UIApplication.shared.registerForRemoteNotifications()
    } catch {
      notificationStatus = error.localizedDescription
    }
  }

  func registerPushToken(_ token: String) async {
    guard let apiURL, let deviceID = activeDeviceID else { return }
    let hostID = activeHostID ?? "mdx_cloud"
    do {
      #if DEBUG
        let environment = "sandbox"
      #else
        let environment = "production"
      #endif
      let identity = try identityManager.loadOrCreate()
      guard identity.deviceID == deviceID else {
        throw MobileDeviceIdentityError.privateKeyMissingOrInvalid
      }
      var registration = MobilePushRegistrationRequest(
        deviceID: deviceID,
        hostID: hostID,
        pushToken: token,
        kind: "apns",
        environment: environment
      )
      registration.deviceSignature = try identityManager.signPushRegistrationAttestation(
        registration)
      let result = try await forgeClient(baseURL: apiURL).registerPush(registration)
      guard result.restartDurable,
        result.stateSource == "kernel_receipt_projection",
        result.durableScope == "registration_metadata_only",
        !result.receiptID.isEmpty,
        !result.tokenEchoed,
        !result.rawTokenPersisted,
        result.deviceSignatureVerification == "verified_registered_device_key"
      else {
        throw MobileForgeClientError.unsafeRelayEnvelope
      }
      if result.productionDeliveryAllowed {
        notificationStatus = "On"
      } else {
        notificationStatus = "Registration recorded, delivery closed"
      }
    } catch {
      notificationStatus = error.localizedDescription
    }
  }

  func recordPushFailure(_ error: Error) {
    notificationStatus = error.localizedDescription
  }

  private func execute(
    kind: String,
    sessionID: String,
    expectedVersion: Int,
    payload: MobileCommandPayload,
    userPresence: String = "device_unlocked",
    requestedTargetKind: ExecutionTargetKind? = nil,
    requestedTargetID: String? = nil
  ) async {
    lastCommandSucceeded = false
    commandFeedback = nil
    lastActionURL = nil
    lastActionSessionID = nil
    if activeDeviceID == nil {
      if let identity = try? identityManager.loadOrCreate() {
        activeDeviceID = identity.deviceID
      }
    }
    guard let apiURL, let deviceID = activeDeviceID else {
      commandFeedback = "MDx could not establish this iPhone's device identity."
      return
    }
    guard let tenantID = activeTenantID, let actorID = activeUserID else {
      commandFeedback = "MDx could not verify this iPhone's account identity."
      return
    }
    let currentSession = snapshot.sessions.first { $0.sessionID == sessionID }
    let targetKind = requestedTargetKind ?? currentSession?.activeTarget.kind ?? .pairedHost
    let target: ExecutionTarget
    switch targetKind {
    case .pairedHost:
      guard let hostID = activeHostID else {
        commandFeedback = "Pair this iPhone with MDx on your Mac before using that target."
        return
      }
      target = ExecutionTarget(
        kind: .pairedHost,
        targetID: hostID,
        tenantID: tenantID,
        displayName: activeHostName ?? "Your Mac",
        capabilityRevision: currentSession?.activeTarget.capabilityRevision ?? 1
      )
    case .mdxCloud:
      guard let environmentID = requestedTargetID ?? currentSession?.activeTarget.targetID,
        !environmentID.isEmpty
      else {
        commandFeedback = "Verify this repository's MDx Cloud environment first."
        return
      }
      target = ExecutionTarget(
        kind: .mdxCloud,
        targetID: environmentID,
        tenantID: tenantID,
        displayName: "MDx Cloud",
        capabilityRevision: currentSession?.activeTarget.capabilityRevision ?? 1
      )
    case .customerManaged:
      commandFeedback = "Private cloud targets are not enabled yet."
      return
    }
    var command = MobileSessionCommand(
      sessionID: sessionID,
      expectedSessionVersion: expectedVersion,
      tenantID: tenantID,
      actorUserID: actorID,
      actorDeviceID: deviceID,
      target: target,
      kind: kind,
      payload: payload,
      userPresence: userPresence
    )
    do {
      command.deviceSignature = try identityManager.signCommandAttestation(command)
    } catch {
      commandFeedback = error.localizedDescription
      return
    }
    isCommandInFlight = true
    lastCommandSucceeded = false
    defer { isCommandInFlight = false }
    do {
      let forge = forgeClient(baseURL: apiURL)
      let result = try await forge.command(command)
      guard !result.staleStateAccepted,
        !result.grantsExecutionAuthority,
        !result.productionWriteAllowed
      else {
        throw MobileForgeClientError.unsafeRelayEnvelope
      }
      commandFeedback = result.detail ?? result.safeSummary
      lastActionURL = result.actionURL
      lastActionSessionID = result.actionURL == nil ? nil : sessionID
      lastCommandSucceeded = true
      logger.notice("Mobile command accepted kind=\(kind, privacy: .public)")
      if kind == "start_build" {
        insertAcceptedSession(
          sessionID: sessionID,
          goal: payload.goal ?? "New Forge build",
          repositoryID: payload.repositoryID ?? "MDx workspace",
          target: target,
          tenantID: tenantID,
          actorID: actorID
        )
        startedSessionReconciliationTask?.cancel()
        startedSessionReconciliationTask = Task { [weak self] in
          guard let self else { return }
          await self.reconcileStartedSession(sessionID: sessionID, forge: forge)
          guard !Task.isCancelled else { return }
          self.restartRelay(apiURL: apiURL)
        }
      } else {
        let projection = try await forge.sessions()
        apply(projection: projection, pairedHostCount: snapshot.pairedHostCount)
      }
      await refreshNeedsYou(forge: forge)
      await refreshReviews(forge: forge)
    } catch {
      commandFeedback = error.localizedDescription
      logger.error("Mobile command failed kind=\(kind, privacy: .public)")
      if let clientError = error as? MobileForgeClientError,
        case MobileForgeClientError.serverRefused(let code, _) = clientError,
        code == "mobile_command_stale_session"
      {
        await refreshProjection()
      }
    }
  }

  private func reconcileStartedSession(sessionID: String, forge: MobileForgeClient) async {
    for attempt in 0..<60 where !Task.isCancelled {
      do {
        let projection = try await forge.sessions()
        apply(projection: projection, pairedHostCount: snapshot.pairedHostCount)
        if projection.sessions.contains(where: { $0.sessionID == sessionID }) {
          return
        }
      } catch {
        logger.debug(
          "Waiting for started mobile session attempt=\(attempt + 1, privacy: .public)"
        )
      }
      if attempt < 59 {
        try? await Task.sleep(for: .milliseconds(500))
      }
    }
    commandFeedback = "Forge accepted the build. Reconnecting to follow it."
  }

  private func insertAcceptedSession(
    sessionID: String,
    goal: String,
    repositoryID: String,
    target: ExecutionTarget,
    tenantID: String,
    actorID: String
  ) {
    let now = Date()
    let accepted = ForgeWorkSession(
      sessionID: sessionID,
      sessionVersion: 1,
      tenantID: tenantID,
      ownerUserID: actorID,
      repositoryID: repositoryID,
      sourceRevision: "pending",
      goal: goal,
      state: .queued,
      stage: .intake,
      activeTarget: target,
      targetHistory: [target],
      lastEventSequence: 0,
      replayCursor: 0,
      needsUser: false,
      latestCheckpointRef: nil,
      createdAt: now,
      updatedAt: now
    )
    let existing = snapshot.sessions.filter { $0.sessionID != sessionID }
    acceptedSessions[sessionID] = accepted
    logger.notice(
      "Showing accepted mobile session id=\(sessionID, privacy: .public) pending=\(self.acceptedSessions.count, privacy: .public)"
    )
    snapshot = ForgeAnywhereSnapshot(
      connection: .connected,
      safeStatus: "Forge accepted your build",
      sessions: Array(([accepted] + existing).prefix(32)),
      attention: snapshot.attention,
      reviews: snapshot.reviews,
      pairedHostCount: snapshot.pairedHostCount,
      readyCloudEnvironmentCount: snapshot.readyCloudEnvironmentCount
    )
    selectedSessionID = sessionID
  }

  private func restartRelay(apiURL: URL) {
    relayTask?.cancel()
    relayTask = Task { [weak self] in
      await self?.runRelay(apiURL: apiURL)
    }
  }

  private func refreshProjection() async {
    guard let apiURL else { return }
    do {
      let forge = forgeClient(baseURL: apiURL)
      let projection = try await forge.sessions()
      apply(projection: projection, pairedHostCount: snapshot.pairedHostCount)
      await refreshNeedsYou(forge: forge)
      await refreshReviews(forge: forge)
      await refreshModelReadiness(forge: forge)
    } catch {
      commandFeedback = error.localizedDescription
    }
  }

  private func refreshModelReadiness(forge: MobileForgeClient? = nil) async {
    guard let apiURL, !fixtureMode else { return }
    do {
      let readiness = try await (forge ?? forgeClient(baseURL: apiURL)).modelReadiness()
      guard !readiness.secretValuesExposed else {
        throw MobileForgeClientError.unsafeRelayEnvelope
      }
      modelReadiness = readiness
    } catch {
      modelReadiness = nil
    }
  }

  private func refreshNeedsYou(forge: MobileForgeClient) async {
    do {
      let projection = try await forge.needsYou()
      guard !projection.staleDecisionsAllowed, !projection.productionWriteAllowed else {
        throw MobileForgeClientError.unsafeRelayEnvelope
      }
      needsYou = projection.items
    } catch {
      needsYou = []
    }
  }

  private func refreshReviews(forge: MobileForgeClient) async {
    do {
      let projection = try await forge.reviews()
      guard projection.summaryIsNavigationNotProof,
        !projection.rawSecretValuesIncluded,
        !projection.mergeAuthorityGranted,
        !projection.deploymentAuthorityGranted,
        !projection.productionWriteAllowed
      else {
        throw MobileForgeClientError.unsafeRelayEnvelope
      }
      snapshot = ForgeAnywhereSnapshot(
        connection: snapshot.connection,
        safeStatus: snapshot.safeStatus,
        sessions: snapshot.sessions,
        attention: snapshot.attention,
        reviews: projection.reviews,
        pairedHostCount: snapshot.pairedHostCount,
        readyCloudEnvironmentCount: snapshot.readyCloudEnvironmentCount
      )
      lastRefreshAt = Date()
    } catch {
      // Keep the last safe review projection during transient failures.
    }
  }

  private func refreshHandoffTargets() async {
    guard let apiURL else { return }
    do {
      let targets = try await forgeClient(baseURL: apiURL).handoffTargets()
      guard !targets.rawRootsIncluded,
        !targets.originValuesIncluded,
        !targets.containsSecretValues,
        !targets.grantsExecutionAuthority,
        !targets.productionWriteAllowed
      else {
        throw MobileForgeClientError.unsafeRelayEnvelope
      }
      handoffTargets = targets
    } catch {
      // Handoff stays unavailable while the bounded target projection is stale.
    }
  }

  private func handoffRepositoryID(
    for session: ForgeWorkSession,
    destination: ExecutionTarget
  ) -> String? {
    switch destination.kind {
    case .mdxCloud:
      guard
        let source = handoffTargets?.pairedHostRepositories.first(where: {
          $0.repositoryID == session.repositoryID && !$0.identityFingerprint.isEmpty
        })
      else { return nil }
      return handoffTargets?.cloudEnvironments.first(where: {
        $0.environmentID == destination.targetID
          && $0.verified
          && $0.identityFingerprint == source.identityFingerprint
      })?.repositoryID
    case .pairedHost:
      guard
        let source = handoffTargets?.cloudEnvironments.first(where: {
          ($0.repositoryID == session.repositoryID
            || $0.environmentID == session.activeTarget.targetID)
            && $0.verified && !$0.identityFingerprint.isEmpty
        })
      else { return nil }
      return handoffTargets?.pairedHostRepositories.first(where: {
        $0.identityFingerprint == source.identityFingerprint
      })?.repositoryID
    case .customerManaged:
      return nil
    }
  }

  private func runRelay(apiURL: URL) async {
    let forge = forgeClient(baseURL: apiURL)
    let pairing = pairingClient(baseURL: apiURL)
    var backoffSeconds: UInt64 = 1
    try? credentialStore.delete(account: "mobile-relay-token")
    while !Task.isCancelled {
      do {
        let identity = try identityManager.loadOrCreate()
        let trust = try await pairing.projection()
        activeTenantID = trust.tenantID
        activeUserID = trust.userID
        guard
          let activePairing = trust.pairings.first(where: {
            $0.deviceID == identity.deviceID && $0.pairingStatus == "active"
          })
        else {
          activeDeviceID = identity.deviceID
          activeHostID = nil
          activeHostName = nil
          setConnection(.offline, status: "Waiting for pairing. Retrying automatically")
          try await Task.sleep(for: .seconds(backoffSeconds))
          backoffSeconds = min(backoffSeconds * 2, 30)
          continue
        }
        backoffSeconds = 1
        activeDeviceID = identity.deviceID
        activeHostID = activePairing.hostID
        activeHostName = trust.hosts.first { $0.hostID == activePairing.hostID }?.displayName
        let projection = try await forge.sessions()
        apply(
          projection: projection,
          pairedHostCount: trust.hosts.filter { $0.status == "active" }.count)
        await refreshNeedsYou(forge: forge)
        await refreshReviews(forge: forge)
        let sessionIDs = MobileRelayScope.sessionIDs(from: projection.sessions)
        if sessionIDs.isEmpty {
          setConnection(.connected, status: "Ready for your next build")
          try await Task.sleep(for: .seconds(5))
          continue
        }
        let credential = try await forge.relayCredential(
          deviceID: identity.deviceID,
          hostID: activePairing.hostID,
          sessionIDs: sessionIDs
        )
        try credentialStore.save(
          Data(credential.relayToken.utf8),
          account: "mobile-relay-token",
          accessibility: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        )
        try await consume(
          credential: credential,
          deviceID: identity.deviceID,
          forge: forge
        )
        backoffSeconds = 1
      } catch is CancellationError {
        return
      } catch {
        if activeHostID != nil, let projection = try? await forge.sessions() {
          apply(projection: projection, pairedHostCount: snapshot.pairedHostCount)
          await refreshNeedsYou(forge: forge)
          await refreshReviews(forge: forge)
          setConnection(.connected, status: "Forge is connected. Live updates are reconnecting")
        } else {
          setConnection(.offline, status: "Reconnecting without losing your place")
        }
        try? await Task.sleep(for: .seconds(backoffSeconds))
        backoffSeconds = min(backoffSeconds * 2, 30)
      }
    }
  }

  private func consume(
    credential: MobileRelayCredential,
    deviceID: String,
    forge: MobileForgeClient
  ) async throws {
    guard !credential.isExpired else { throw MobileForgeClientError.httpFailure }
    var request = URLRequest(url: credential.relayURL)
    request.setValue("Bearer \(credential.relayToken)", forHTTPHeaderField: "Authorization")
    let socket = URLSession.shared.webSocketTask(with: request)
    socket.resume()
    connectionGeneration += 1
    let minimumCursor = credential.allowedStreamIDs.compactMap { cursors[$0] }.min() ?? 0
    let subscribe = MobileRelaySubscribe(
      tenantID: credential.tenantID,
      streamIDs: credential.allowedStreamIDs,
      afterSequence: minimumCursor,
      deviceID: deviceID,
      connectionGeneration: connectionGeneration
    )
    let subscribeData = try JSONEncoder().encode(subscribe)
    try await socket.send(.data(subscribeData))
    setConnection(.connected, status: "Forge is connected")
    let heartbeat = Task {
      while !Task.isCancelled {
        try await Task.sleep(for: .seconds(25))
        if credential.isExpired {
          socket.cancel(with: .policyViolation, reason: nil)
          return
        }
        try await socket.send(.string(#"{"type":"heartbeat"}"#))
      }
    }
    defer {
      heartbeat.cancel()
      socket.cancel(with: .goingAway, reason: nil)
      try? credentialStore.delete(account: "mobile-relay-token")
    }
    while !Task.isCancelled {
      let message = try await socket.receive()
      let data: Data
      switch message {
      case .data(let value): data = value
      case .string(let value): data = Data(value.utf8)
      @unknown default: continue
      }
      if let event = try? JSONDecoder().decode(MobileRelayEnvelope.self, from: data) {
        try apply(event: event, credential: credential)
        if ["review_ready", "completed", "failed", "stopped"].contains(event.kind) {
          let projection = try await forge.sessions()
          apply(projection: projection, pairedHostCount: snapshot.pairedHostCount)
        }
        continue
      }
      let control = try JSONDecoder().decode(MobileRelayControlFrame.self, from: data)
      if control.type == "catch_up",
        control.state == "gap_detected" || control.state == "snapshot_required"
      {
        let projection = try await forge.sessions()
        apply(projection: projection, pairedHostCount: snapshot.pairedHostCount)
      } else if control.type == "error" {
        throw MobileForgeClientError.serverRefused(
          control.code ?? "mobile_relay_error",
          control.message ?? "The relay refused this subscription."
        )
      } else if control.type == "drain" {
        throw MobileForgeClientError.httpFailure
      }
    }
  }

  private func apply(event: MobileRelayEnvelope, credential: MobileRelayCredential) throws {
    guard event.schemaVersion == 1,
      credential.allowedStreamIDs.contains(event.sessionID),
      event.tenantID == credential.tenantID,
      event.target.tenantID == credential.tenantID,
      !event.containsSecretValues,
      !event.grantsAuthority
    else {
      throw MobileForgeClientError.unsafeRelayEnvelope
    }
    let current = cursors[event.sessionID] ?? 0
    guard event.sequence > current else { return }
    cursors[event.sessionID] = event.sequence
    var events = eventsBySession[event.sessionID] ?? []
    events.append(event)
    if events.count > 200 { events.removeFirst(events.count - 200) }
    eventsBySession[event.sessionID] = events

    let sessions = snapshot.sessions.map { session in
      session.sessionID == event.sessionID ? session.applying(event) : session
    }
    snapshot = ForgeAnywhereSnapshot(
      connection: .connected,
      safeStatus: event.summary,
      sessions: sessions,
      attention: snapshot.attention,
      reviews: snapshot.reviews,
      pairedHostCount: snapshot.pairedHostCount,
      readyCloudEnvironmentCount: snapshot.readyCloudEnvironmentCount
    )
    if let session = sessions.first(where: { $0.sessionID == event.sessionID }) {
      liveActivity.update(session: session, summary: event.summary)
      if [.completed, .failed, .stopped].contains(session.state) {
        stopLiveActivity(sessionID: session.sessionID)
      }
    }
  }

  private func apply(projection: MobileSessionProjection, pairedHostCount: Int) {
    for session in projection.sessions {
      cursors[session.id] = max(cursors[session.id] ?? 0, session.replayCursor)
      acceptedSessions.removeValue(forKey: session.id)
    }
    let pending = acceptedSessions.values.sorted { $0.updatedAt > $1.updatedAt }
    let sessions = Array((pending + projection.sessions).prefix(32))
    logger.debug(
      "Applied mobile projection canonical=\(projection.sessions.count, privacy: .public) pending=\(pending.count, privacy: .public)"
    )
    snapshot = ForgeAnywhereSnapshot(
      connection: .connected,
      safeStatus: sessions.isEmpty ? "Ready for your next build" : "Forge is connected",
      sessions: sessions,
      attention: snapshot.attention.filter { item in
        sessions.contains { $0.id == item.sessionID && $0.needsUser }
      },
      reviews: snapshot.reviews.filter { review in
        sessions.contains { $0.id == review.sessionID && $0.state == .reviewReady }
      },
      pairedHostCount: pairedHostCount,
      readyCloudEnvironmentCount: cloudSetup?.readyEnvironmentCount
        ?? snapshot.readyCloudEnvironmentCount
    )
    if selectedSessionID == nil { selectedSessionID = projection.sessions.first?.id }
  }

  private func setConnection(_ connection: MobileConnectionState, status: String) {
    logger.notice("Connection state=\(connection.rawValue, privacy: .public)")
    snapshot = ForgeAnywhereSnapshot(
      connection: connection,
      safeStatus: status,
      sessions: snapshot.sessions,
      attention: snapshot.attention,
      reviews: snapshot.reviews,
      pairedHostCount: snapshot.pairedHostCount,
      readyCloudEnvironmentCount: snapshot.readyCloudEnvironmentCount
    )
  }

  private func updateReadyCloudEnvironmentCount(_ count: Int) {
    snapshot = ForgeAnywhereSnapshot(
      connection: snapshot.connection,
      safeStatus: snapshot.safeStatus,
      sessions: snapshot.sessions,
      attention: snapshot.attention,
      reviews: snapshot.reviews,
      pairedHostCount: snapshot.pairedHostCount,
      readyCloudEnvironmentCount: count)
  }

  private func cloudStatusText(_ projection: MobileCloudSetupProjection) -> String {
    if projection.readyEnvironmentCount > 0 {
      return "MDx Cloud is ready for a governed build."
    }
    if projection.environments.contains(where: { $0.status == "VERIFICATION_FAILED" }) {
      return "Hosted verification failed safely. Review the runtime setup and retry."
    }
    if projection.environments.contains(where: { $0.status == "PREPARED_VERIFICATION_REQUIRED" }) {
      return "The environment is prepared and waiting for hosted verification."
    }
    if !projection.repositories.isEmpty {
      return "Choose a connected repository and prepare its environment."
    }
    if projection.installations.contains(where: { $0.status == "active" }) {
      return "Choose exactly which repository MDx may build."
    }
    return projection.githubAppConfigured
      ? "Connect GitHub to build in MDx Cloud."
      : "Your MDx operator must configure the GitHub App first."
  }

  private func saveOfflineDraft(_ draft: MobileOfflineDraft) {
    lastCommandSucceeded = false
    logger.notice("Offline draft saved kind=\(draft.kind.rawValue, privacy: .public)")
    offlineDrafts.append(draft)
    if offlineDrafts.count > 50 { offlineDrafts.removeFirst(offlineDrafts.count - 50) }
    do {
      try draftStore.save(offlineDrafts)
      lastCommandSucceeded = true
    } catch {
      offlineDrafts.removeAll { $0.id == draft.id }
      commandFeedback = "This iPhone could not protect the offline draft."
    }
  }
}

extension ForgeWorkSession {
  fileprivate func applying(_ event: MobileRelayEnvelope) -> ForgeWorkSession {
    let mappedStage: ForgeSessionStage =
      switch event.kind {
      case "accepted": .intake
      case "started": .orientation
      case "checkpointed": .check
      case "handed_off": .handoff
      case "review_ready": .review
      case "completed", "failed", "stopped": .done
      default: stage
      }
    var updatedTargetHistory = targetHistory
    if updatedTargetHistory.last != event.target {
      updatedTargetHistory.append(event.target)
    }
    return ForgeWorkSession(
      schemaVersion: schemaVersion,
      sessionID: sessionID,
      sessionVersion: max(sessionVersion, event.sessionVersion),
      tenantID: tenantID,
      ownerUserID: ownerUserID,
      repositoryID: repositoryID,
      sourceRevision: sourceRevision,
      goal: goal,
      state: ForgeSessionState(rawValue: event.state) ?? state,
      stage: mappedStage,
      activeTarget: event.target,
      targetHistory: updatedTargetHistory,
      lastEventSequence: event.sequence,
      replayCursor: event.sequence,
      needsUser: event.state == "needs_user",
      latestCheckpointRef: latestCheckpointRef,
      createdAt: createdAt,
      updatedAt: event.occurredAt,
      executionAuthorityOpen: false,
      deploymentAuthorityOpen: false
    )
  }
}
