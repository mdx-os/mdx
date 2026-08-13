@preconcurrency import ActivityKit
import Foundation
import MDxOperatorShared

@MainActor
final class ForgeLiveActivityCoordinator {
  private var activity: Activity<ForgeActivityAttributes>?

  var followedSessionID: String? { activity?.attributes.opaqueSessionRef }

  func follow(_ session: ForgeWorkSession, summary: String) throws {
    guard ActivityAuthorizationInfo().areActivitiesEnabled else {
      throw ForgeLiveActivityError.disabled
    }
    if activity?.attributes.opaqueSessionRef == session.sessionID {
      return
    }
    if let activity {
      Task { await activity.end(nil, dismissalPolicy: .immediate) }
    }
    let attributes = ForgeActivityAttributes(
      opaqueSessionRef: session.sessionID,
      repositoryLabel: session.repositoryID
    )
    let state = ForgeActivityAttributes.ContentState(
      stage: session.stage.rawValue.capitalized,
      safeSummary: summary,
      needsUser: session.needsUser
    )
    activity = try Activity.request(
      attributes: attributes,
      content: ActivityContent(state: state, staleDate: nil),
      pushType: .token
    )
  }

  func update(session: ForgeWorkSession, summary: String) {
    guard let activity, activity.attributes.opaqueSessionRef == session.sessionID else { return }
    let state = ForgeActivityAttributes.ContentState(
      stage: session.stage.rawValue.capitalized,
      safeSummary: summary,
      needsUser: session.needsUser
    )
    Task {
      await activity.update(ActivityContent(state: state, staleDate: nil))
    }
  }

  func end(sessionID: String) {
    guard let activity, activity.attributes.opaqueSessionRef == sessionID else { return }
    self.activity = nil
    Task { await activity.end(nil, dismissalPolicy: .default) }
  }
}

enum ForgeLiveActivityError: Error, LocalizedError {
  case disabled

  var errorDescription: String? {
    "Live Activities are disabled for MDx in iPhone Settings."
  }
}
