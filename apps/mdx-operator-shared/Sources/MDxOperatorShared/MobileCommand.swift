import Foundation

public struct MobileCommandPayload: Codable, Equatable, Sendable {
  public var goal: String?
  public var repositoryID: String?
  public var allowedCommands: [String]?
  public var instruction: String?
  public var reason: String?
  public var answer: String?
  public var fleetID: String?
  public var decisionKind: String?
  public var decisionSubjectReceiptID: String?
  public var fleetWidth: UInt64?
  public var maxCostCents: UInt64?
  public var maxRuntimeMilliseconds: UInt64?
  public var destinationTarget: ExecutionTarget?
  public var destinationRepositoryID: String?
  public var comment: String?
  public var commitSHA: String?
  public var targetHost: String?
  public var baseBranch: String?

  public init(
    goal: String? = nil,
    repositoryID: String? = nil,
    allowedCommands: [String]? = nil,
    instruction: String? = nil,
    reason: String? = nil,
    answer: String? = nil,
    fleetID: String? = nil,
    decisionKind: String? = nil,
    decisionSubjectReceiptID: String? = nil,
    fleetWidth: UInt64? = nil,
    maxCostCents: UInt64? = nil,
    maxRuntimeMilliseconds: UInt64? = nil,
    destinationTarget: ExecutionTarget? = nil,
    destinationRepositoryID: String? = nil,
    comment: String? = nil,
    commitSHA: String? = nil,
    targetHost: String? = nil,
    baseBranch: String? = nil
  ) {
    self.goal = goal
    self.repositoryID = repositoryID
    self.allowedCommands = allowedCommands
    self.instruction = instruction
    self.reason = reason
    self.answer = answer
    self.fleetID = fleetID
    self.decisionKind = decisionKind
    self.decisionSubjectReceiptID = decisionSubjectReceiptID
    self.fleetWidth = fleetWidth
    self.maxCostCents = maxCostCents
    self.maxRuntimeMilliseconds = maxRuntimeMilliseconds
    self.destinationTarget = destinationTarget
    self.destinationRepositoryID = destinationRepositoryID
    self.comment = comment
    self.commitSHA = commitSHA
    self.targetHost = targetHost
    self.baseBranch = baseBranch
  }

  enum CodingKeys: String, CodingKey {
    case goal, instruction, reason, answer, comment
    case repositoryID = "repository_id"
    case allowedCommands = "allowed_commands"
    case fleetID = "fleet_id"
    case decisionKind = "decision_kind"
    case decisionSubjectReceiptID = "decision_subject_receipt_id"
    case fleetWidth = "fleet_width"
    case maxCostCents = "max_cost_cents"
    case maxRuntimeMilliseconds = "max_runtime_ms"
    case destinationTarget = "destination_target"
    case destinationRepositoryID = "destination_repository_id"
    case commitSHA = "commit_sha"
    case targetHost = "target_host"
    case baseBranch = "base_branch"
  }
}

public struct MobileSessionCommand: Codable, Equatable, Sendable {
  public let schemaVersion: Int
  public let commandID: String
  public let idempotencyKey: String
  public let sessionID: String
  public let expectedSessionVersion: Int
  public let tenantID: String
  public let actorUserID: String
  public let actorDeviceID: String
  public let target: ExecutionTarget
  public let kind: String
  public let payload: MobileCommandPayload
  public let issuedAt: Date
  public let expiresAt: Date
  public let userPresence: String
  public let delegationEnvelopeRef: String
  public let grantsExecutionAuthority: Bool
  public var deviceSignature: String

  public init(
    commandID: String = "mobile_command_\(UUID().uuidString.lowercased())",
    idempotencyKey: String = "mobile_idempotency_\(UUID().uuidString.lowercased())",
    sessionID: String,
    expectedSessionVersion: Int,
    tenantID: String,
    actorUserID: String,
    actorDeviceID: String,
    target: ExecutionTarget,
    kind: String,
    payload: MobileCommandPayload,
    issuedAt: Date = Date(),
    expiresAt: Date = Date().addingTimeInterval(120),
    userPresence: String = "device_unlocked",
    delegationEnvelopeRef: String = "session_default",
    deviceSignature: String = ""
  ) {
    schemaVersion = 1
    self.commandID = commandID
    self.idempotencyKey = idempotencyKey
    self.sessionID = sessionID
    self.expectedSessionVersion = expectedSessionVersion
    self.tenantID = tenantID
    self.actorUserID = actorUserID
    self.actorDeviceID = actorDeviceID
    self.target = target
    self.kind = kind
    self.payload = payload
    self.issuedAt = issuedAt
    self.expiresAt = expiresAt
    self.userPresence = userPresence
    self.delegationEnvelopeRef = delegationEnvelopeRef
    grantsExecutionAuthority = false
    self.deviceSignature = deviceSignature
  }

  public func attestationPayload() throws -> Data {
    let encoder = JSONEncoder()
    encoder.dateEncodingStrategy = .iso8601
    encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
    let encoded = try encoder.encode(self)
    guard var object = try JSONSerialization.jsonObject(with: encoded) as? [String: Any] else {
      throw MobileCommandAttestationError.invalidCommandShape
    }
    object.removeValue(forKey: "device_signature")
    return try JSONSerialization.data(
      withJSONObject: object,
      options: [.sortedKeys, .withoutEscapingSlashes]
    )
  }

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case commandID = "command_id"
    case idempotencyKey = "idempotency_key"
    case sessionID = "session_id"
    case expectedSessionVersion = "expected_session_version"
    case tenantID = "tenant_id"
    case actorUserID = "actor_user_id"
    case actorDeviceID = "actor_device_id"
    case target, kind, payload
    case issuedAt = "issued_at"
    case expiresAt = "expires_at"
    case userPresence = "user_presence"
    case delegationEnvelopeRef = "delegation_envelope_ref"
    case grantsExecutionAuthority = "grants_execution_authority"
    case deviceSignature = "device_signature"
  }
}

public enum MobileCommandAttestationError: Error, Equatable {
  case invalidCommandShape
}

public struct MobileSessionCommandResult: Codable, Equatable, Sendable {
  public let status: String
  public let commandID: String
  public let idempotencyKey: String
  public let idempotentReplay: Bool
  public let sessionID: String
  public let sessionVersion: Int
  public let kind: String
  public let controlReceiptID: String
  public let commandReceiptID: String
  public let safeSummary: String
  public let detail: String?
  public let staleStateAccepted: Bool
  public let grantsExecutionAuthority: Bool
  public let productionWriteAllowed: Bool
  public let actionURL: URL?
  public let checkpoint: HandoffCheckpoint?
  public let resultingTarget: ExecutionTarget?

  enum CodingKeys: String, CodingKey {
    case status, kind, detail
    case commandID = "command_id"
    case idempotencyKey = "idempotency_key"
    case idempotentReplay = "idempotent_replay"
    case sessionID = "session_id"
    case sessionVersion = "session_version"
    case controlReceiptID = "control_receipt_id"
    case commandReceiptID = "command_receipt_id"
    case safeSummary = "safe_summary"
    case staleStateAccepted = "stale_state_accepted"
    case grantsExecutionAuthority = "grants_execution_authority"
    case productionWriteAllowed = "production_write_allowed"
    case actionURL = "action_url"
    case checkpoint
    case resultingTarget = "resulting_target"
  }
}

public struct MobileNeedsYouProjection: Codable, Equatable, Sendable {
  public let status: String
  public let tenantID: String
  public let items: [MobileNeedsYouItem]
  public let staleDecisionsAllowed: Bool
  public let productionWriteAllowed: Bool

  enum CodingKeys: String, CodingKey {
    case status, items
    case tenantID = "tenant_id"
    case staleDecisionsAllowed = "stale_decisions_allowed"
    case productionWriteAllowed = "production_write_allowed"
  }
}

public struct MobileNeedsYouItem: Codable, Equatable, Identifiable, Sendable {
  public let id: String
  public let sessionID: String?
  public let expectedSessionVersion: Int?
  public let kind: String
  public let title: String
  public let detail: String
  public let recommendedChoice: String
  public let alternatives: [String]
  public let subjectType: String
  public let subjectID: String
  public let subject: String
  public let actionState: String
  public let blockerCodes: [String]
  public let evidenceReceiptIDs: [String]
  public let mobileActionSupported: Bool
  public let decisionKind: String?
  public let decisionSubjectReceiptID: String?
  public let requiresBiometric: Bool
  public let exactActionAllowed: String
  public let grantsWorkerStartAuthority: Bool
  public let productionWriteAllowed: Bool

  enum CodingKeys: String, CodingKey {
    case id, kind, title, detail, alternatives, subject
    case sessionID = "session_id"
    case expectedSessionVersion = "expected_session_version"
    case recommendedChoice = "recommended_choice"
    case subjectType = "subject_type"
    case subjectID = "subject_id"
    case actionState = "action_state"
    case blockerCodes = "blocker_codes"
    case evidenceReceiptIDs = "evidence_receipt_ids"
    case mobileActionSupported = "mobile_action_supported"
    case decisionKind = "decision_kind"
    case decisionSubjectReceiptID = "decision_subject_receipt_id"
    case requiresBiometric = "requires_biometric"
    case exactActionAllowed = "exact_action_allowed"
    case grantsWorkerStartAuthority = "grants_worker_start_authority"
    case productionWriteAllowed = "production_write_allowed"
  }
}

public struct MobilePushRegistrationResult: Codable, Equatable, Sendable {
  public let status: String
  public let registrationID: String
  public let receiptID: String
  public let idempotentReplay: Bool
  public let stateSource: String
  public let restartDurable: Bool
  public let durableScope: String
  public let tokenEchoed: Bool
  public let rawTokenPersisted: Bool
  public let deviceSignatureVerification: String
  public let notificationCategories: [String]
  public let routineProgressNotificationsAllowed: Bool
  public let productionDeliveryAllowed: Bool

  enum CodingKeys: String, CodingKey {
    case status
    case registrationID = "registration_id"
    case receiptID = "receipt_id"
    case idempotentReplay = "idempotent_replay"
    case stateSource = "state_source"
    case restartDurable = "restart_durable"
    case durableScope = "durable_scope"
    case tokenEchoed = "token_echoed"
    case rawTokenPersisted = "raw_token_persisted"
    case deviceSignatureVerification = "device_signature_verification"
    case notificationCategories = "notification_categories"
    case routineProgressNotificationsAllowed = "routine_progress_notifications_allowed"
    case productionDeliveryAllowed = "production_delivery_allowed"
  }
}

public struct MobilePushRegistrationRequest: Codable, Equatable, Sendable {
  public let schemaVersion: Int
  public let deviceID: String
  public let hostID: String
  public let pushToken: String
  public let kind: String
  public let environment: String
  public let issuedAt: Date
  public let expiresAt: Date
  public var deviceSignature: String

  public init(
    deviceID: String,
    hostID: String,
    pushToken: String,
    kind: String,
    environment: String,
    issuedAt: Date = Date(),
    expiresAt: Date = Date().addingTimeInterval(120),
    deviceSignature: String = ""
  ) {
    schemaVersion = 1
    self.deviceID = deviceID
    self.hostID = hostID
    self.pushToken = pushToken
    self.kind = kind
    self.environment = environment
    self.issuedAt = issuedAt
    self.expiresAt = expiresAt
    self.deviceSignature = deviceSignature
  }

  public func attestationPayload() throws -> Data {
    let encoder = JSONEncoder()
    encoder.dateEncodingStrategy = .iso8601
    encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
    let encoded = try encoder.encode(self)
    guard var object = try JSONSerialization.jsonObject(with: encoded) as? [String: Any] else {
      throw MobileCommandAttestationError.invalidCommandShape
    }
    object.removeValue(forKey: "device_signature")
    return try JSONSerialization.data(
      withJSONObject: object,
      options: [.sortedKeys, .withoutEscapingSlashes]
    )
  }

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case deviceID = "device_id"
    case hostID = "host_id"
    case pushToken = "push_token"
    case kind, environment
    case issuedAt = "issued_at"
    case expiresAt = "expires_at"
    case deviceSignature = "device_signature"
  }
}

extension MobileForgeClient {
  public func command(_ command: MobileSessionCommand) async throws -> MobileSessionCommandResult {
    let url = baseURL.appending(path: "mobile/session-commands.json")
    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    let encoder = JSONEncoder()
    encoder.dateEncodingStrategy = .iso8601
    request.httpBody = try encoder.encode(command)
    await authorize(&request)
    let (data, response) = try await session.data(for: request)
    return try decode(MobileSessionCommandResult.self, data: data, response: response)
  }

  public func needsYou() async throws -> MobileNeedsYouProjection {
    let url = baseURL.appending(path: "mobile/needs-you.json")
    let (data, response) = try await authorizedData(from: url)
    return try decode(MobileNeedsYouProjection.self, data: data, response: response)
  }

  public func reviews() async throws -> MobileReviewProjection {
    let url = baseURL.appending(path: "mobile/reviews.json")
    let (data, response) = try await authorizedData(from: url)
    return try decode(MobileReviewProjection.self, data: data, response: response)
  }

  public func registerPush(
    _ registration: MobilePushRegistrationRequest
  ) async throws -> MobilePushRegistrationResult {
    let url = baseURL.appending(path: "mobile/push-registrations.json")
    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    let encoder = JSONEncoder()
    encoder.dateEncodingStrategy = .iso8601
    request.httpBody = try encoder.encode(registration)
    await authorize(&request)
    let (data, response) = try await session.data(for: request)
    return try decode(MobilePushRegistrationResult.self, data: data, response: response)
  }
}
