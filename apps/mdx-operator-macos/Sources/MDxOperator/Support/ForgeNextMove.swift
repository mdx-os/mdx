import Foundation

/// The one operator-facing Forge move, derived only from projected run state.
/// It chooses a doorway. Authority remains on the route behind that doorway.
struct ForgeNextMove: Equatable {
  enum Kind: Equatable {
    case decision
    case review
    case recover
    case follow
    case start
    case connect
  }

  let kind: Kind
  let eyebrow: String
  let title: String
  let detail: String
  let buttonTitle: String
  let systemImage: String
  let runID: String?
  let destination: ForgeLane?
  let additionalWaiting: Int

  static func derive(decisions: [ForgeDecision] = [], runs: [ForgeRun], connected: Bool) -> ForgeNextMove {
    let rankedDecisions = decisions.sorted {
      $0.priority == $1.priority ? decisionRank($0.kind) < decisionRank($1.kind) : $0.priority < $1.priority
    }
    if let decision = rankedDecisions.first {
      let matchingCount = rankedDecisions.filter { $0.kind == decision.kind }.count
      return ForgeNextMove(
        kind: .decision,
        eyebrow: "Needs you",
        title: groupedTitle(for: decision, count: matchingCount),
        detail: decision.detail,
        buttonTitle: "Open decision",
        systemImage: "person.crop.circle.badge.exclamationmark",
        runID: decision.kind == "ship_call" ? decision.subjectID : nil,
        destination: decision.lane,
        additionalWaiting: max(0, rankedDecisions.count - matchingCount)
      )
    }

    // Finished work gets the bounded human judgment before ordinary recovery.
    // A future projected urgency can still promote a critical intervention.
    if let run = runs.first(where: \.isReviewReady) {
      return ForgeNextMove(
        kind: .review,
        eyebrow: "Ready to review",
        title: run.displayName,
        detail: "Read the change and its proof, then decide whether it ships or goes back for revision.",
        buttonTitle: "Review branch",
        systemImage: "checkmark.seal",
        runID: run.id,
        destination: .review,
        additionalWaiting: 0
      )
    }

    if let run = runs.first(where: { !$0.isRunning && !$0.isSystemEvidence && $0.hasProofTrouble }) {
      return ForgeNextMove(
        kind: .recover,
        eyebrow: "Needs direction",
        title: run.displayName,
        detail: "See where it stopped, then narrow the work or ask for a focused revision.",
        buttonTitle: "Pick it back up",
        systemImage: "arrow.uturn.forward.circle",
        runID: run.id,
        destination: .runs,
        additionalWaiting: 0
      )
    }

    if let run = runs.first(where: \.isRunning) {
      return ForgeNextMove(
        kind: .follow,
        eyebrow: "Working now",
        title: run.displayName,
        detail: "Follow the translated trail while the branch and proof take shape.",
        buttonTitle: "Follow the work",
        systemImage: "waveform.path.ecg",
        runID: run.id,
        destination: .runs,
        additionalWaiting: 0
      )
    }

    if connected {
      return ForgeNextMove(
        kind: .start,
        eyebrow: "Ready",
        title: "Hand Forge a scoped change",
        detail: "Describe the outcome and the check that proves it. Forge works in isolation and brings back a branch.",
        buttonTitle: "Start a build",
        systemImage: "plus.circle",
        runID: nil,
        destination: .overview,
        additionalWaiting: 0
      )
    }

    return ForgeNextMove(
      kind: .connect,
      eyebrow: "Offline",
      title: "Connect local MDx",
      detail: "The native workspace is ready. Connect the local route server to load work and record requests.",
      buttonTitle: "Try again",
      systemImage: "arrow.clockwise",
      runID: nil,
      destination: nil,
      additionalWaiting: 0
    )
  }

  private static func decisionRank(_ kind: String) -> Int {
    switch kind {
    case "fleet_repair_escalation": 0
    case "ratify_plan": 1
    case "wide_plan_review": 2
    case "ship_call": 3
    default: 9
    }
  }

  private static func groupedTitle(for decision: ForgeDecision, count: Int) -> String {
    guard count > 1 else { return decision.title }
    switch decision.kind {
    case "fleet_repair_escalation": return "\(count) fleets to unblock"
    case "ratify_plan": return "\(count) splits to approve"
    case "wide_plan_review": return "\(count) plans to review before they scale"
    case "ship_call": return "\(count) changes ready to review"
    default: return "\(count) decisions waiting"
    }
  }
}

enum ForgeTrailStage: Int, CaseIterable {
  case plan
  case build
  case prove
  case review

  var title: String {
    switch self {
    case .plan: "Plan"
    case .build: "Build"
    case .prove: "Prove"
    case .review: "Review"
    }
  }
}
