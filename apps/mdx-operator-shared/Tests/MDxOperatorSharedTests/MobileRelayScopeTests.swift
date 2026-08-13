import Foundation
import XCTest

@testable import MDxOperatorShared

final class MobileRelayScopeTests: XCTestCase {
  func testScopeStaysWithinServerLimitAndKeepsActionableSessions() {
    let historical = (0..<40).map { index in
      session(
        id: "forge_run_completed_\(index)",
        state: .completed,
        updatedAt: Date(timeIntervalSince1970: Double(index))
      )
    }
    let actionable = [
      session(id: "forge_run_needs_you", state: .needsUser, updatedAt: .distantPast),
      session(id: "forge_run_review_ready", state: .reviewReady, updatedAt: .distantPast),
      session(id: "forge_run_running", state: .running, updatedAt: .distantPast),
      session(id: "not_a_relay_stream", state: .needsUser, updatedAt: .distantFuture),
    ]

    let result = MobileRelayScope.sessionIDs(from: historical + actionable)

    XCTAssertEqual(result.count, MobileRelayScope.maximumSessionCount)
    XCTAssertEqual(
      Array(result.prefix(3)),
      ["forge_run_needs_you", "forge_run_review_ready", "forge_run_running"]
    )
    XCTAssertFalse(result.contains("not_a_relay_stream"))
  }

  func testScopeUsesNewestSessionsWithinTheSameState() {
    let sessions = (0..<40).map { index in
      session(
        id: "forge_run_running_\(index)",
        state: .running,
        updatedAt: Date(timeIntervalSince1970: Double(index))
      )
    }

    let result = MobileRelayScope.sessionIDs(from: sessions)

    XCTAssertEqual(result.first, "forge_run_running_39")
    XCTAssertEqual(result.last, "forge_run_running_8")
  }

  private func session(
    id: String,
    state: ForgeSessionState,
    updatedAt: Date
  ) -> ForgeWorkSession {
    let target = ExecutionTarget(
      kind: .pairedHost,
      targetID: "host",
      tenantID: "tenant",
      displayName: "Mac",
      capabilityRevision: 1
    )
    return ForgeWorkSession(
      sessionID: id,
      sessionVersion: 1,
      tenantID: "tenant",
      ownerUserID: "user",
      repositoryID: "repo",
      sourceRevision: "main",
      goal: "Test relay scope",
      state: state,
      stage: .build,
      activeTarget: target,
      targetHistory: [target],
      lastEventSequence: 0,
      replayCursor: 0,
      needsUser: state == .needsUser,
      latestCheckpointRef: nil,
      createdAt: updatedAt,
      updatedAt: updatedAt
    )
  }
}
