import Foundation

public enum ForgeAnywhereFixture: String, CaseIterable, Sendable {
  case loading
  case empty
  case offline
  case active
  case needsYou = "needs_you"
  case reviewReady = "review_ready"
  case error

  public var snapshot: ForgeAnywhereSnapshot {
    switch self {
    case .loading:
      return .init(
        connection: .loading, safeStatus: "Loading your work", sessions: [], attention: [],
        reviews: [], pairedHostCount: 0, readyCloudEnvironmentCount: 0)
    case .empty:
      return .init(
        connection: .connected, safeStatus: "Ready for your first build", sessions: [],
        attention: [], reviews: [], pairedHostCount: 1, readyCloudEnvironmentCount: 1)
    case .offline:
      return .init(
        connection: .offline,
        safeStatus: "Offline. Drafts stay on this iPhone until you reconnect.",
        sessions: [Self.activeSession], attention: [], reviews: [], pairedHostCount: 1,
        readyCloudEnvironmentCount: 1)
    case .active:
      return .init(
        connection: .connected, safeStatus: "Forge is building on your Mac",
        sessions: [Self.activeSession], attention: [], reviews: [], pairedHostCount: 1,
        readyCloudEnvironmentCount: 1)
    case .needsYou:
      return .init(
        connection: .connected, safeStatus: "One build needs your judgment",
        sessions: [Self.needsYouSession], attention: [Self.attention], reviews: [],
        pairedHostCount: 1, readyCloudEnvironmentCount: 1)
    case .reviewReady:
      return .init(
        connection: .connected, safeStatus: "One build is ready to review",
        sessions: [Self.reviewSession], attention: [], reviews: [Self.review], pairedHostCount: 1,
        readyCloudEnvironmentCount: 1)
    case .error:
      return .init(
        connection: .failed,
        safeStatus: "MDx could not refresh. Your running build was not changed.", sessions: [],
        attention: [], reviews: [], pairedHostCount: 1, readyCloudEnvironmentCount: 1)
    }
  }

  private static let mac = ExecutionTarget(
    kind: .pairedHost, targetID: "host_founder_mac", tenantID: "tenant_mdx",
    displayName: "Darren's MacBook Pro", capabilityRevision: 3)
  private static let now = Date(timeIntervalSince1970: 1_783_922_400)

  private static let activeSession = ForgeWorkSession(
    sessionID: "forge_mobile_active",
    sessionVersion: 12,
    tenantID: "tenant_mdx",
    ownerUserID: "founder",
    repositoryID: "mdx-native",
    sourceRevision: "ca71b88d",
    goal: "Build the iPhone command surface for Forge Anywhere",
    state: .running,
    stage: .build,
    activeTarget: mac,
    targetHistory: [mac],
    lastEventSequence: 41,
    replayCursor: 41,
    needsUser: false,
    latestCheckpointRef: nil,
    createdAt: now.addingTimeInterval(-1_800),
    updatedAt: now.addingTimeInterval(-12)
  )

  private static let needsYouSession = ForgeWorkSession(
    sessionID: "forge_mobile_attention",
    sessionVersion: 8,
    tenantID: "tenant_mdx",
    ownerUserID: "founder",
    repositoryID: "mdx-native",
    sourceRevision: "ca71b88d",
    goal: "Connect the mobile relay without widening authority",
    state: .needsUser,
    stage: .check,
    activeTarget: mac,
    targetHistory: [mac],
    lastEventSequence: 22,
    replayCursor: 22,
    needsUser: true,
    latestCheckpointRef: nil,
    createdAt: now.addingTimeInterval(-2_400),
    updatedAt: now.addingTimeInterval(-30)
  )

  private static let reviewSession = ForgeWorkSession(
    sessionID: "forge_mobile_review",
    sessionVersion: 19,
    tenantID: "tenant_mdx",
    ownerUserID: "founder",
    repositoryID: "mdx-native",
    sourceRevision: "ca71b88d",
    goal: "Ship a reconnect-safe mobile session timeline",
    state: .reviewReady,
    stage: .review,
    activeTarget: mac,
    targetHistory: [mac],
    lastEventSequence: 67,
    replayCursor: 67,
    needsUser: false,
    latestCheckpointRef: "checkpoint_review_1",
    createdAt: now.addingTimeInterval(-5_400),
    updatedAt: now.addingTimeInterval(-90)
  )

  private static let attention = AttentionItem(
    id: "attention_relay_scope",
    sessionID: needsYouSession.sessionID,
    title: "Choose the relay scope",
    detail:
      "Forge found two safe implementations. One keeps artifacts local; the other uploads redacted evidence for mobile review.",
    actionLabel: "Review options"
  )

  private static let review = ReviewDigest(
    id: "review_reconnect",
    sessionID: reviewSession.sessionID,
    outcome:
      "Reconnect now resumes from the last durable event without duplicating terminal state.",
    demo: "A 15-minute disconnect replays 38 events and lands on the same review-ready result.",
    nextMove: "Inspect the focused diff, then approve this exact commit or request a revision.",
    proofSummary: "All selected checks passed on the reviewed commit.",
    proof: [
      ReviewProof(name: "Relay replay suite: 24 passed", passed: true),
      ReviewProof(name: "Native mapper checks passed", passed: true),
      ReviewProof(name: "No authority boundary changed", passed: true),
    ],
    risk: "The production relay still requires observed provider evidence before activation.",
    branch: "codex/ios-relay-replay",
    commitSHA: "ca71b88d5b3125b0dbd5f7f4bbec2cb3e66b42aa",
    changedFiles: 11,
    added: 284,
    removed: 61,
    changeMap: [
      ReviewChangeArea(
        id: "relay", label: "Relay replay", fileCount: 4, added: 126, removed: 25,
        generated: false),
      ReviewChangeArea(
        id: "mobile", label: "iPhone session state", fileCount: 5, added: 139, removed: 31,
        generated: false),
      ReviewChangeArea(
        id: "proof", label: "Proof and fixtures", fileCount: 2, added: 19, removed: 5,
        generated: false),
    ],
    files: [
      ReviewChangedFile(
        path: "apps/mdx-operator-ios/Sources/MDxAnywhere/Stores/ForgeAnywhereStore.swift",
        added: 67,
        removed: 18,
        patch: """
          @@ -1088,6 +1088,12 @@
           guard envelope.sequence > replayCursor else { return }
          +guard envelope.sessionVersion >= session.sessionVersion else {
          +  recordSafeReplaySkip(envelope)
          +  return
          +}
           apply(envelope)
           replayCursor = envelope.sequence
          """,
        patchTruncated: false,
        agentConfidence: "high"),
      ReviewChangedFile(
        path:
          "apps/mdx-operator-shared/Tests/MDxOperatorSharedTests/MobileRelayContractTests.swift",
        added: 42,
        removed: 3,
        patch: """
          @@ -54,6 +54,11 @@
          +#expect(replayed.map(\\.sequence) == [41, 42, 43])
          +#expect(replayed.last?.kind == "review_ready")
          +#expect(replayed.last?.grantsAuthority == false)
          """,
        patchTruncated: false,
        agentConfidence: "high"),
      ReviewChangedFile(
        path: "services/mdx-kernel/src/mobile-relay.ts",
        added: 58,
        removed: 21,
        patch: """
          @@ -211,7 +211,12 @@
          -return events.filter(event => event.sequence > cursor)
          +return events
          +  .filter(event => event.sequence > cursor)
          +  .sort((left, right) => left.sequence - right.sequence)
          +  .filter(isMobileSafeEnvelope)
          """,
        patchTruncated: true,
        agentConfidence: "medium"),
    ],
    filesTruncated: true
  )
}
