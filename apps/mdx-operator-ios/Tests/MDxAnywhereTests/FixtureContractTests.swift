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
}
