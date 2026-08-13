import Foundation

struct ForgeMission: Identifiable, Equatable {
  let id: String
  let goal: String
  let nonGoals: String
  let constraints: String
  let doneWhen: String
  let allowedWriteScope: String
  let blockedPaths: String
  let validationCommands: String
  let fleetWidth: Int
  let maxCostCents: Int
  let checkpointCadenceMinutes: Int
  let state: String
  let latestCheckpointSummary: String
  let checkpointCount: Int
  let completedMilestoneCount: Int
  let blockedMilestoneCount: Int
  let steeringNoteCount: Int
  let relatedRunIDs: [String]
  let milestones: [MissionMilestone]

  var milestoneCount: Int { milestones.count }

  var displayName: String {
    let trimmed = goal.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? id : trimmed
  }

  func links(runID: String) -> Bool {
    guard !runID.isEmpty else { return false }
    if relatedRunIDs.contains(runID) { return true }
    return milestones.contains { $0.relatedRunID == runID }
  }

  var isActive: Bool {
    let s = state.lowercased()
    return !s.contains("complete") && !s.contains("done") && !s.contains("closed")
  }

  var stateLabel: String {
    switch state {
    case "ADMITTED_WAITING_FOR_LOCAL_EXECUTION": return "Admitted, waiting to start"
    case "RUNNING", "IN_PROGRESS": return "In progress"
    case "PAUSED": return "Paused"
    case "COMPLETED", "DONE": return "Completed"
    case "BLOCKED": return "Blocked"
    default:
      return state.replacingOccurrences(of: "_", with: " ").capitalized
    }
  }

  var budgetSummary: String {
    let cost = maxCostCents > 0 ? "$\(String(format: "%.2f", Double(maxCostCents) / 100)) budget" : ""
    let cadence = checkpointCadenceMinutes > 0 ? "checkpoint every \(checkpointCadenceMinutes) min" : ""
    let workers = fleetWidth > 0 ? "\(fleetWidth) worker\(fleetWidth == 1 ? "" : "s")" : ""
    return [workers, cadence, cost].filter { !$0.isEmpty }.joined(separator: " · ")
  }

  var launchableMilestone: MissionMilestone? {
    milestones.first { !$0.isDone && !$0.isBlocked } ?? milestones.first
  }
}

struct MissionMilestone: Identifiable, Equatable {
  var id: String { milestoneID }
  let milestoneID: String
  let title: String
  let validationCommand: String
  let status: String
  let acceptanceCriteria: String
  let checkpointReceiptID: String
  let validationStatus: String
  let relatedRunID: String

  var statusLabel: String {
    switch status {
    case "PENDING_LOCAL_EXECUTION", "PENDING": return "Waiting"
    case "IN_PROGRESS", "RUNNING": return "In progress"
    case "COMPLETED", "DONE", "PASSED": return "Done"
    case "BLOCKED", "FAILED": return "Blocked"
    default:
      return status.replacingOccurrences(of: "_", with: " ").capitalized
    }
  }

  var isDone: Bool { statusLabel == "Done" }
  var isBlocked: Bool { statusLabel == "Blocked" }
}

struct MissionLaunchOutcome: Equatable {
  let action: RunActionOutcome
  let delegatedRunID: String
  let delegatedRoute: String
  let milestoneID: String

  var opensRun: Bool {
    action.succeeded && !delegatedRunID.isEmpty && delegatedRoute == "/forge/runs.json"
  }
}
