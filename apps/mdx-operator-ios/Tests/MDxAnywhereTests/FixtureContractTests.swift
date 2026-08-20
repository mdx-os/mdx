import MDxOperatorShared
import XCTest

@testable import MDxAnywhere

final class FixtureContractTests: XCTestCase {
  func testHandoffRepositoryCarriesHumanVisibleProofCommands() throws {
    let payload = Data(
      #"{"repository_id":"mdx-playground","display_name":"MDx Playground","kind":"paired_host_local","identity_fingerprint":"","default_branch":"main","default_branch_source":"recorded","suggested_checks":["npm test"],"root_recorded":true}"#
        .utf8
    )
    let repository = try JSONDecoder().decode(MobileHandoffRepository.self, from: payload)

    XCTAssertEqual(repository.suggestedChecks, ["npm test"])
  }

  func testEveryRequiredMobileStateExists() {
    XCTAssertEqual(
      Set(ForgeAnywhereFixture.allCases.map(\.rawValue)),
      Set([
        "loading", "empty", "offline", "active", "needs_you", "review_ready", "error",
      ]))
  }

  func testFixturesCannotClaimExecutionAuthority() {
    for fixture in ForgeAnywhereFixture.allCases {
      XCTAssertTrue(fixture.snapshot.sessions.allSatisfy { !$0.executionAuthorityOpen })
      XCTAssertTrue(fixture.snapshot.sessions.allSatisfy { !$0.deploymentAuthorityOpen })
    }
  }

  @MainActor
  func testVerifiedCloudEnvironmentBecomesTheSafeBuildTargetWithoutPairedHostSource() throws {
    let cloudSetup = try JSONDecoder().decode(
      MobileCloudSetupProjection.self,
      from: Data(
        #"{"status":"OK","tenant_id":"tenant_personal_beta","installations":[],"repositories":[{"repository_id":42,"full_name":"mdx-os/mdx-rust","display_name":"MDx Rust","default_branch":"main","private":false,"repo_id":"mdx-rust","source_revision":"abc123","status":"connected","receipt_id":"receipt_repo","token_recorded":true}],"environments":[{"environment_id":"cloud_env_mdx-rust","environment_version":1,"repo_id":"mdx-rust","fingerprint":"sha256:environment","status":"VERIFIED","ready_for_cloud_builds":true,"receipt_id":"receipt_environment","secret_values_recorded":false}],"ready_environment_count":1,"github_app_configured":true,"hosted_sandbox_status":"ready","secret_values_included":false,"production_write_allowed":false}"#
          .utf8
      ))
    let handoffTargets = try JSONDecoder().decode(
      MobileHandoffTargets.self,
      from: Data(
        #"{"status":"OK","paired_host_repositories":[],"cloud_environments":[{"environment_id":"cloud_env_mdx-rust","repository_id":"mdx-rust","display_name":"MDx Cloud","identity_fingerprint":"sha256:repository","verified":true}],"raw_roots_included":false,"origin_values_included":false,"contains_secret_values":false,"grants_execution_authority":false,"production_write_allowed":false}"#
          .utf8
      ))
    let store = ForgeAnywhereStore(
      snapshot: ForgeAnywhereFixture.loading.snapshot,
      apiURL: URL(string: "https://mdx.example"),
      fixtureMode: false,
      cloudSetup: cloudSetup,
      handoffTargets: handoffTargets,
      activeDeviceID: "device_beta",
      activeHostID: "host_beta"
    )

    XCTAssertFalse(store.canStartPairedHostBuild)
    XCTAssertTrue(store.canStartCloudBuild)
    XCTAssertEqual(store.recommendedBuildTarget, .mdxCloud)
    XCTAssertTrue(store.cloudEnvironmentReady(repoID: "mdx-rust"))
    XCTAssertTrue(store.buildTargetReady(repositoryID: "mdx-rust", targetKind: .mdxCloud))
    XCTAssertEqual(
      store.buildRepositories(for: .mdxCloud),
      [
        MobileBuildRepositoryOption(
          id: "mdx-rust",
          displayName: "mdx-os/mdx-rust",
          suggestedChecks: []
        )
      ])
    XCTAssertEqual(
      store.buildProofCommands(repositoryID: "mdx-rust", targetKind: .mdxCloud), [])
  }

  @MainActor
  func testVerifiedHandoffProjectionKeepsCloudBuildDiscoverableWhenSetupIsUnavailable() throws {
    let handoffTargets = try JSONDecoder().decode(
      MobileHandoffTargets.self,
      from: Data(
        #"{"status":"OK","paired_host_repositories":[],"cloud_environments":[{"environment_id":"cloud_env_mdx-rust","repository_id":"mdx-rust","display_name":"MDx Cloud","identity_fingerprint":"sha256:repository","verified":true}],"raw_roots_included":false,"origin_values_included":false,"contains_secret_values":false,"grants_execution_authority":false,"production_write_allowed":false}"#
          .utf8
      ))
    let store = ForgeAnywhereStore(
      snapshot: ForgeAnywhereFixture.loading.snapshot,
      apiURL: URL(string: "https://mdx.example"),
      fixtureMode: false,
      handoffTargets: handoffTargets,
      activeDeviceID: "device_beta",
      activeHostID: "host_beta"
    )

    XCTAssertTrue(store.canStartCloudBuild)
    XCTAssertEqual(store.recommendedBuildTarget, .mdxCloud)
    XCTAssertEqual(store.buildRepositories(for: .mdxCloud).map(\.id), ["mdx-rust"])
  }

  @MainActor
  func testAuthenticationBoundaryDropsTenantProjectionBeforeAnotherUserCanEnter() {
    let store = ForgeAnywhereStore(snapshot: ForgeAnywhereFixture.reviewReady.snapshot)

    store.suspendCloudAccess()

    XCTAssertEqual(store.snapshot.connection, .offline)
    XCTAssertEqual(store.snapshot.safeStatus, "Sign in to reconnect")
    XCTAssertTrue(store.snapshot.sessions.isEmpty)
    XCTAssertTrue(store.snapshot.attention.isEmpty)
    XCTAssertTrue(store.snapshot.reviews.isEmpty)
    XCTAssertNil(store.selectedSessionID)
  }

  @MainActor
  func testConnectedRelayReconcilesCanonicalTerminalState() async throws {
    let target = ExecutionTarget(
      kind: .mdxCloud,
      targetID: "cloud_env_mdx-rust",
      tenantID: "tenant_personal_beta",
      displayName: "MDx Cloud",
      capabilityRevision: 1
    )
    let acceptedAt = Date(timeIntervalSince1970: 1_787_000_000)
    let queued = ForgeWorkSession(
      sessionID: "forge_run_mobile_projection_refresh",
      sessionVersion: 1,
      tenantID: "tenant_personal_beta",
      ownerUserID: "founder",
      repositoryID: "mdx-rust",
      sourceRevision: "pending",
      goal: "Inspect the README and summarize this project. Do not change files.",
      state: .queued,
      stage: .intake,
      activeTarget: target,
      targetHistory: [target],
      lastEventSequence: 0,
      replayCursor: 0,
      needsUser: false,
      latestCheckpointRef: nil,
      createdAt: acceptedAt,
      updatedAt: acceptedAt
    )
    let completedAt = acceptedAt.addingTimeInterval(30)
    let completed = ForgeWorkSession(
      sessionID: queued.sessionID,
      sessionVersion: 42,
      tenantID: queued.tenantID,
      ownerUserID: queued.ownerUserID,
      repositoryID: queued.repositoryID,
      sourceRevision: "working-copy",
      goal: queued.goal,
      state: .completed,
      stage: .done,
      activeTarget: target,
      targetHistory: [target],
      lastEventSequence: 42,
      replayCursor: 42,
      needsUser: false,
      latestCheckpointRef: nil,
      createdAt: acceptedAt,
      updatedAt: completedAt
    )
    let projection = try makeProjection(sessions: [completed])
    let store = ForgeAnywhereStore(
      snapshot: ForgeAnywhereSnapshot(
        connection: .connected,
        safeStatus: "Forge is connected",
        sessions: [queued],
        attention: [],
        reviews: [],
        pairedHostCount: 1,
        readyCloudEnvironmentCount: 1
      )
    )

    let reconciliation = store.canonicalProjectionReconciliationTask(
      refreshInterval: .milliseconds(10)
    ) {
      projection
    }
    defer { reconciliation.cancel() }
    try await Task.sleep(for: .milliseconds(40))

    XCTAssertEqual(store.snapshot.sessions.first?.state, .completed)
    XCTAssertEqual(store.snapshot.sessions.first?.stage, .done)
    XCTAssertEqual(store.snapshot.sessions.first?.replayCursor, 42)
  }

  func testOfflineDraftStoreSurvivesRelaunchAndDeletesCleanly() throws {
    let directory = FileManager.default.temporaryDirectory.appending(
      path: "mdx-offline-draft-test-\(UUID().uuidString)", directoryHint: .isDirectory)
    defer { try? FileManager.default.removeItem(at: directory) }
    let store = OfflineDraftStore(baseURL: directory)
    let createdAt = Date(timeIntervalSince1970: 1_783_922_400)
    let draft = MobileOfflineDraft(
      id: "offline_draft_round_trip",
      kind: .startBuild,
      repositoryID: "mdx-native",
      goal: "Prove a protected draft survives relaunch",
      targetKind: .pairedHost,
      createdAt: createdAt
    )

    try store.save([draft])
    XCTAssertEqual(store.load(), [draft])

    try store.deleteAll()
    XCTAssertTrue(store.load().isEmpty)
  }

  func testOfflineDraftStoreRetainsOnlyTheNewestFiftyDrafts() throws {
    let directory = FileManager.default.temporaryDirectory.appending(
      path: "mdx-offline-draft-limit-test-\(UUID().uuidString)", directoryHint: .isDirectory)
    defer { try? FileManager.default.removeItem(at: directory) }
    let store = OfflineDraftStore(baseURL: directory)
    let drafts = (0..<55).map { index in
      MobileOfflineDraft(
        id: "offline_draft_\(index)",
        kind: .followUp,
        sessionID: "forge_mobile_active",
        instruction: "Direction \(index)",
        targetKind: .pairedHost,
        createdAt: Date(timeIntervalSince1970: Double(index))
      )
    }

    try store.save(drafts)
    let restored = store.load()
    XCTAssertEqual(restored.count, 50)
    XCTAssertEqual(restored.first?.id, "offline_draft_5")
    XCTAssertEqual(restored.last?.id, "offline_draft_54")
  }

  private func makeProjection(sessions: [ForgeWorkSession]) throws -> MobileSessionProjection {
    let sessionValues = try sessions.map { session -> Any in
      let data = try JSONEncoder().encode(session)
      return try JSONSerialization.jsonObject(with: data)
    }
    let data = try JSONSerialization.data(withJSONObject: [
      "status": "OK",
      "schema_version": 1,
      "tenant_id": "tenant_personal_beta",
      "sessions": sessionValues,
      "raw_model_output_included": false,
      "raw_tool_output_included": false,
    ])
    return try JSONDecoder().decode(MobileSessionProjection.self, from: data)
  }
}
