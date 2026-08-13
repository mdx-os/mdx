import Foundation

public enum MobileConnectionState: String, Codable, Sendable {
  case loading
  case connected
  case offline
  case failed
}

public struct AttentionItem: Equatable, Identifiable, Sendable {
  public let id: String
  public let sessionID: String
  public let title: String
  public let detail: String
  public let actionLabel: String

  public init(id: String, sessionID: String, title: String, detail: String, actionLabel: String) {
    self.id = id
    self.sessionID = sessionID
    self.title = title
    self.detail = detail
    self.actionLabel = actionLabel
  }
}

public struct ReviewProof: Codable, Equatable, Identifiable, Sendable {
  public let name: String
  public let passed: Bool
  public var id: String { name }
}

public struct ReviewChangedFile: Codable, Equatable, Identifiable, Sendable {
  public let path: String
  public let added: Int
  public let removed: Int
  public let patch: String
  public let patchTruncated: Bool
  public let agentConfidence: String?
  public var id: String { path }

  enum CodingKeys: String, CodingKey {
    case path, added, removed, patch
    case patchTruncated = "patch_truncated"
    case agentConfidence = "agent_confidence"
  }
}

public struct ReviewChangeArea: Codable, Equatable, Identifiable, Sendable {
  public let id: String
  public let label: String
  public let fileCount: Int
  public let added: Int
  public let removed: Int
  public let generated: Bool

  enum CodingKeys: String, CodingKey {
    case id, label, added, removed, generated
    case fileCount = "file_count"
  }
}

public struct ReviewDigest: Codable, Equatable, Identifiable, Sendable {
  public let id: String
  public let sessionID: String
  public let outcome: String
  public let demo: String
  public let reviewStatus: String
  public let nextMove: String
  public let proofSummary: String
  public let proof: [ReviewProof]
  public let risk: String
  public let branch: String
  public let commitSHA: String
  public let changedFiles: Int
  public let added: Int
  public let removed: Int
  public let changeMap: [ReviewChangeArea]
  public let files: [ReviewChangedFile]
  public let filesTruncated: Bool

  public init(
    id: String,
    sessionID: String,
    outcome: String,
    demo: String,
    reviewStatus: String = "review_required",
    nextMove: String = "Review the change",
    proofSummary: String = "Proof is available",
    proof: [ReviewProof],
    risk: String,
    branch: String = "",
    commitSHA: String = "",
    changedFiles: Int,
    added: Int = 0,
    removed: Int = 0,
    changeMap: [ReviewChangeArea] = [],
    files: [ReviewChangedFile] = [],
    filesTruncated: Bool = false
  ) {
    self.id = id
    self.sessionID = sessionID
    self.outcome = outcome
    self.demo = demo
    self.reviewStatus = reviewStatus
    self.nextMove = nextMove
    self.proofSummary = proofSummary
    self.proof = proof
    self.risk = risk
    self.branch = branch
    self.commitSHA = commitSHA
    self.changedFiles = changedFiles
    self.added = added
    self.removed = removed
    self.changeMap = changeMap
    self.files = files
    self.filesTruncated = filesTruncated
  }

  enum CodingKeys: String, CodingKey {
    case id, outcome, demo, proof, risk, branch, added, removed, files
    case sessionID = "session_id"
    case reviewStatus = "review_status"
    case nextMove = "next_move"
    case proofSummary = "proof_summary"
    case commitSHA = "commit_sha"
    case changedFiles = "changed_files"
    case changeMap = "change_map"
    case filesTruncated = "files_truncated"
  }
}

public struct MobileReviewProjection: Codable, Equatable, Sendable {
  public let status: String
  public let reviews: [ReviewDigest]
  public let summaryIsNavigationNotProof: Bool
  public let rawSecretValuesIncluded: Bool
  public let mergeAuthorityGranted: Bool
  public let deploymentAuthorityGranted: Bool
  public let productionWriteAllowed: Bool

  enum CodingKeys: String, CodingKey {
    case status, reviews
    case summaryIsNavigationNotProof = "summary_is_navigation_not_proof"
    case rawSecretValuesIncluded = "raw_secret_values_included"
    case mergeAuthorityGranted = "merge_authority_granted"
    case deploymentAuthorityGranted = "deployment_authority_granted"
    case productionWriteAllowed = "production_write_allowed"
  }
}

public struct ForgeAnywhereSnapshot: Equatable, Sendable {
  public let connection: MobileConnectionState
  public let safeStatus: String
  public let sessions: [ForgeWorkSession]
  public let attention: [AttentionItem]
  public let reviews: [ReviewDigest]
  public let pairedHostCount: Int
  public let readyCloudEnvironmentCount: Int

  public init(
    connection: MobileConnectionState,
    safeStatus: String,
    sessions: [ForgeWorkSession],
    attention: [AttentionItem],
    reviews: [ReviewDigest],
    pairedHostCount: Int,
    readyCloudEnvironmentCount: Int
  ) {
    self.connection = connection
    self.safeStatus = safeStatus
    self.sessions = sessions
    self.attention = attention
    self.reviews = reviews
    self.pairedHostCount = pairedHostCount
    self.readyCloudEnvironmentCount = readyCloudEnvironmentCount
  }
}
