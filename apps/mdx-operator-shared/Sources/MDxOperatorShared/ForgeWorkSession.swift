import Foundation

public enum ExecutionTargetKind: String, Codable, CaseIterable, Sendable {
  case pairedHost = "paired_host"
  case mdxCloud = "mdx_cloud"
  case customerManaged = "customer_managed"

  public var shortLabel: String {
    switch self {
    case .pairedHost: "Mac"
    case .mdxCloud: "MDx Cloud"
    case .customerManaged: "Private Cloud"
    }
  }
}

public struct ExecutionTarget: Codable, Equatable, Sendable {
  public let kind: ExecutionTargetKind
  public let targetID: String
  public let tenantID: String
  public let displayName: String
  public let capabilityRevision: Int

  public init(
    kind: ExecutionTargetKind,
    targetID: String,
    tenantID: String,
    displayName: String,
    capabilityRevision: Int
  ) {
    self.kind = kind
    self.targetID = targetID
    self.tenantID = tenantID
    self.displayName = displayName
    self.capabilityRevision = capabilityRevision
  }

  enum CodingKeys: String, CodingKey {
    case kind
    case targetID = "target_id"
    case tenantID = "tenant_id"
    case displayName = "display_name"
    case capabilityRevision = "capability_revision"
  }
}

public struct HandoffCheckpoint: Codable, Equatable, Identifiable, Sendable {
  public let checkpointID: String
  public let sessionID: String
  public let sessionVersion: Int
  public let sourceTarget: ExecutionTarget
  public let destinationTarget: ExecutionTarget
  public let sourceRevision: String
  public let branch: String
  public let workspaceSnapshotRef: String?
  public let eventCursor: Int
  public let evidenceRefs: [String]
  public let createdByDeviceID: String
  public let createdAt: Date
  public let expiresAt: Date
  public let containsSecretValues: Bool
  public let grantsExecutionAuthority: Bool

  public var id: String { checkpointID }

  enum CodingKeys: String, CodingKey {
    case checkpointID = "checkpoint_id"
    case sessionID = "session_id"
    case sessionVersion = "session_version"
    case sourceTarget = "source_target"
    case destinationTarget = "destination_target"
    case sourceRevision = "source_revision"
    case branch
    case workspaceSnapshotRef = "workspace_snapshot_ref"
    case eventCursor = "event_cursor"
    case evidenceRefs = "evidence_refs"
    case createdByDeviceID = "created_by_device_id"
    case createdAt = "created_at"
    case expiresAt = "expires_at"
    case containsSecretValues = "contains_secret_values"
    case grantsExecutionAuthority = "grants_execution_authority"
  }
}

public enum ForgeSessionState: String, Codable, Sendable {
  case draft
  case queued
  case running
  case paused
  case needsUser = "needs_user"
  case reviewReady = "review_ready"
  case completed
  case failed
  case stopped

  public var label: String {
    switch self {
    case .needsUser: "Needs you"
    case .reviewReady: "Ready to review"
    default: rawValue.replacingOccurrences(of: "_", with: " ").capitalized
    }
  }
}

public enum ForgeSessionStage: String, Codable, CaseIterable, Sendable {
  case intake
  case orientation
  case build
  case check
  case review
  case handoff
  case done
}

public struct ForgeWorkSession: Codable, Equatable, Identifiable, Sendable {
  public let schemaVersion: Int
  public let sessionID: String
  public let sessionVersion: Int
  public let tenantID: String
  public let ownerUserID: String
  public let repositoryID: String
  public let sourceRevision: String
  public let goal: String
  public let state: ForgeSessionState
  public let stage: ForgeSessionStage
  public let activeTarget: ExecutionTarget
  public let targetHistory: [ExecutionTarget]
  public let lastEventSequence: Int
  public let replayCursor: Int
  public let needsUser: Bool
  public let latestCheckpointRef: String?
  public let createdAt: Date
  public let updatedAt: Date
  public let executionAuthorityOpen: Bool
  public let deploymentAuthorityOpen: Bool

  public var id: String { sessionID }

  public init(
    schemaVersion: Int = 1,
    sessionID: String,
    sessionVersion: Int,
    tenantID: String,
    ownerUserID: String,
    repositoryID: String,
    sourceRevision: String,
    goal: String,
    state: ForgeSessionState,
    stage: ForgeSessionStage,
    activeTarget: ExecutionTarget,
    targetHistory: [ExecutionTarget],
    lastEventSequence: Int,
    replayCursor: Int,
    needsUser: Bool,
    latestCheckpointRef: String?,
    createdAt: Date,
    updatedAt: Date,
    executionAuthorityOpen: Bool = false,
    deploymentAuthorityOpen: Bool = false
  ) {
    self.schemaVersion = schemaVersion
    self.sessionID = sessionID
    self.sessionVersion = sessionVersion
    self.tenantID = tenantID
    self.ownerUserID = ownerUserID
    self.repositoryID = repositoryID
    self.sourceRevision = sourceRevision
    self.goal = goal
    self.state = state
    self.stage = stage
    self.activeTarget = activeTarget
    self.targetHistory = targetHistory
    self.lastEventSequence = lastEventSequence
    self.replayCursor = replayCursor
    self.needsUser = needsUser
    self.latestCheckpointRef = latestCheckpointRef
    self.createdAt = createdAt
    self.updatedAt = updatedAt
    self.executionAuthorityOpen = executionAuthorityOpen
    self.deploymentAuthorityOpen = deploymentAuthorityOpen
  }

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case sessionID = "session_id"
    case sessionVersion = "session_version"
    case tenantID = "tenant_id"
    case ownerUserID = "owner_user_id"
    case repositoryID = "repository_id"
    case sourceRevision = "source_revision"
    case goal, state, stage
    case activeTarget = "active_target"
    case targetHistory = "target_history"
    case lastEventSequence = "last_event_sequence"
    case replayCursor = "replay_cursor"
    case needsUser = "needs_user"
    case latestCheckpointRef = "latest_checkpoint_ref"
    case createdAt = "created_at"
    case updatedAt = "updated_at"
    case executionAuthorityOpen = "execution_authority_open"
    case deploymentAuthorityOpen = "deployment_authority_open"
  }
}
