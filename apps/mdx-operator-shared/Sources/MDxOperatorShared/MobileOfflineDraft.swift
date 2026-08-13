import Foundation

public enum MobileOfflineDraftKind: String, Codable, Sendable {
  case startBuild = "start_build"
  case followUp = "follow_up"
}

public struct MobileOfflineDraft: Codable, Equatable, Identifiable, Sendable {
  public let id: String
  public let kind: MobileOfflineDraftKind
  public let sessionID: String?
  public let repositoryID: String?
  public let goal: String?
  public let instruction: String?
  public let targetKind: ExecutionTargetKind?
  public let createdAt: Date

  public init(
    id: String = "offline_draft_\(UUID().uuidString.lowercased())",
    kind: MobileOfflineDraftKind,
    sessionID: String? = nil,
    repositoryID: String? = nil,
    goal: String? = nil,
    instruction: String? = nil,
    targetKind: ExecutionTargetKind? = nil,
    createdAt: Date = Date()
  ) {
    self.id = id
    self.kind = kind
    self.sessionID = sessionID
    self.repositoryID = repositoryID
    self.goal = goal
    self.instruction = instruction
    self.targetKind = targetKind
    self.createdAt = createdAt
  }

  enum CodingKeys: String, CodingKey {
    case id, kind, goal, instruction
    case sessionID = "session_id"
    case repositoryID = "repository_id"
    case targetKind = "target_kind"
    case createdAt = "created_at"
  }
}
