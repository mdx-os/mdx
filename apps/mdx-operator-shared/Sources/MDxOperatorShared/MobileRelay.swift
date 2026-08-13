import Foundation

public typealias MobileAccessTokenProvider = @Sendable () async -> String?

public enum MobileRelayScope {
  public static let maximumSessionCount = 32

  public static func sessionIDs(from sessions: [ForgeWorkSession]) -> [String] {
    sessions
      .filter { isRelayAddressable($0.sessionID) }
      .sorted { lhs, rhs in
        let lhsPriority = priority(for: lhs.state)
        let rhsPriority = priority(for: rhs.state)
        if lhsPriority != rhsPriority { return lhsPriority < rhsPriority }
        if lhs.updatedAt != rhs.updatedAt { return lhs.updatedAt > rhs.updatedAt }
        return lhs.sessionID < rhs.sessionID
      }
      .prefix(maximumSessionCount)
      .map(\.sessionID)
  }

  private static func isRelayAddressable(_ sessionID: String) -> Bool {
    sessionID.hasPrefix("forge_run_")
      && sessionID.count <= 200
      && sessionID.utf8.allSatisfy {
        (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0)
          || $0 == 45 || $0 == 46 || $0 == 95
      }
  }

  private static func priority(for state: ForgeSessionState) -> Int {
    switch state {
    case .needsUser: 0
    case .reviewReady: 1
    case .running: 2
    case .queued: 3
    case .paused: 4
    case .draft: 5
    case .completed, .failed, .stopped: 6
    }
  }
}

public struct MobileSessionProjection: Codable, Equatable, Sendable {
  public let status: String
  public let schemaVersion: Int
  public let tenantID: String
  public let sessions: [ForgeWorkSession]
  public let rawModelOutputIncluded: Bool
  public let rawToolOutputIncluded: Bool

  enum CodingKeys: String, CodingKey {
    case status, sessions
    case schemaVersion = "schema_version"
    case tenantID = "tenant_id"
    case rawModelOutputIncluded = "raw_model_output_included"
    case rawToolOutputIncluded = "raw_tool_output_included"
  }
}

public struct MobileRelayCredential: Codable, Equatable, Sendable {
  public let status: String
  public let relayURL: URL
  public let relayToken: String
  public let tenantID: String
  public let deviceID: String
  public let hostID: String
  public let allowedStreamIDs: [String]
  public let issuedAtEpoch: UInt64
  public let expiresAtEpoch: UInt64
  public let ttlSeconds: UInt64

  public var isExpired: Bool {
    UInt64(Date().timeIntervalSince1970) >= expiresAtEpoch
  }

  enum CodingKeys: String, CodingKey {
    case status
    case relayURL = "relay_url"
    case relayToken = "relay_token"
    case tenantID = "tenant_id"
    case deviceID = "device_id"
    case hostID = "host_id"
    case allowedStreamIDs = "allowed_stream_ids"
    case issuedAtEpoch = "issued_at_epoch"
    case expiresAtEpoch = "expires_at_epoch"
    case ttlSeconds = "ttl_seconds"
  }
}

public struct MobileRelayEnvelope: Codable, Equatable, Identifiable, Sendable {
  public let schemaVersion: Int
  public let eventID: String
  public let sessionID: String
  public let sessionVersion: Int
  public let sequence: Int
  public let tenantID: String
  public let target: ExecutionTarget
  public let kind: String
  public let commandID: String?
  public let receiptID: String?
  public let safeSummary: String
  public let evidenceRefs: [String]
  public let occurredAtValue: String
  public let redactionStatus: String
  public let containsSecretValues: Bool
  public let grantsAuthority: Bool

  public var id: String { eventID }
  public var summary: String { safeSummary }
  public var occurredAt: Date {
    let fractional = ISO8601DateFormatter()
    fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return fractional.date(from: occurredAtValue)
      ?? ISO8601DateFormatter().date(from: occurredAtValue)
      ?? .distantPast
  }
  public var executionTargetKind: ExecutionTargetKind { target.kind }
  public var executionTargetID: String { target.targetID }
  public var stage: String {
    switch kind {
    case "accepted": "intake"
    case "started": "orientation"
    case "checkpointed": "check"
    case "handed_off": "handoff"
    case "review_ready", "completed", "failed", "stopped": "done"
    default: "build"
    }
  }
  public var state: String {
    switch kind {
    case "accepted": "queued"
    case "needs_user": "needs_user"
    case "handed_off": "paused"
    case "review_ready": "review_ready"
    case "completed": "completed"
    case "failed", "refused": "failed"
    case "stopped": "stopped"
    default: "running"
    }
  }

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case eventID = "event_id"
    case sessionID = "session_id"
    case sessionVersion = "session_version"
    case sequence
    case tenantID = "tenant_id"
    case target, kind
    case commandID = "command_id"
    case receiptID = "receipt_id"
    case safeSummary = "safe_summary"
    case evidenceRefs = "evidence_refs"
    case occurredAtValue = "occurred_at"
    case redactionStatus = "redaction_status"
    case containsSecretValues = "contains_secret_values"
    case grantsAuthority = "grants_authority"
  }
}

public struct MobileRelaySubscribe: Codable, Equatable, Sendable {
  public let type = "subscribe"
  public let tenantID: String
  public let streamIDs: [String]
  public let afterSequence: Int
  public let deviceID: String
  public let connectionGeneration: Int

  public init(
    tenantID: String,
    streamIDs: [String],
    afterSequence: Int,
    deviceID: String,
    connectionGeneration: Int
  ) {
    self.tenantID = tenantID
    self.streamIDs = streamIDs
    self.afterSequence = afterSequence
    self.deviceID = deviceID
    self.connectionGeneration = connectionGeneration
  }

  enum CodingKeys: String, CodingKey {
    case type
    case tenantID = "tenant_id"
    case streamIDs = "stream_ids"
    case afterSequence = "after_sequence"
    case deviceID = "device_id"
    case connectionGeneration = "connection_generation"
  }
}

public struct MobileRelayControlFrame: Codable, Equatable, Sendable {
  public let type: String
  public let topic: String?
  public let state: String?
  public let code: String?
  public let message: String?
}

public struct ModelAccessReadiness: Codable, Equatable, Sendable {
  public let status: String
  public let ready: Bool
  public let providerID: String?
  public let modelID: String?
  public let recommendedNextAction: String
  public let secretValuesExposed: Bool

  enum CodingKeys: String, CodingKey {
    case status, ready
    case providerID = "provider_id"
    case modelID = "model_id"
    case recommendedNextAction = "recommended_next_action"
    case secretValuesExposed = "secret_values_exposed"
  }
}

public struct MobileForgeClient: Sendable {
  private struct ErrorEnvelope: Codable {
    let error: String
    let detail: String
  }

  private struct RelayRequest: Encodable {
    let deviceID: String
    let hostID: String
    let sessionIDs: [String]

    enum CodingKeys: String, CodingKey {
      case deviceID = "device_id"
      case hostID = "host_id"
      case sessionIDs = "session_ids"
    }
  }

  public let baseURL: URL
  let session: URLSession
  let accessTokenProvider: MobileAccessTokenProvider?

  public init(
    baseURL: URL,
    session: URLSession = .shared,
    accessTokenProvider: MobileAccessTokenProvider? = nil
  ) {
    self.baseURL = baseURL
    self.session = session
    self.accessTokenProvider = accessTokenProvider
  }

  func authorizedRequest(url: URL) async -> URLRequest {
    var request = URLRequest(url: url)
    if let token = await accessTokenProvider?(), !token.isEmpty {
      request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    }
    return request
  }

  func authorizedData(from url: URL) async throws -> (Data, URLResponse) {
    try await session.data(for: authorizedRequest(url: url))
  }

  func authorize(_ request: inout URLRequest) async {
    if let token = await accessTokenProvider?(), !token.isEmpty {
      request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    }
  }

  public func sessions() async throws -> MobileSessionProjection {
    let url = baseURL.appending(path: "mobile/sessions.json")
    let (data, response) = try await authorizedData(from: url)
    return try decode(MobileSessionProjection.self, data: data, response: response)
  }

  public func modelReadiness() async throws -> ModelAccessReadiness {
    let url = baseURL.appending(path: "models/readiness.json")
    let (data, response) = try await authorizedData(from: url)
    return try decode(ModelAccessReadiness.self, data: data, response: response)
  }

  public func relayCredential(
    deviceID: String,
    hostID: String,
    sessionIDs: [String]
  ) async throws -> MobileRelayCredential {
    let url = baseURL.appending(path: "mobile/relay-credentials.json")
    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.httpBody = try JSONEncoder().encode(
      RelayRequest(deviceID: deviceID, hostID: hostID, sessionIDs: sessionIDs)
    )
    await authorize(&request)
    let (data, response) = try await session.data(for: request)
    return try decode(MobileRelayCredential.self, data: data, response: response)
  }

  func decode<Response: Decodable>(
    _ type: Response.Type,
    data: Data,
    response: URLResponse
  ) throws -> Response {
    guard let http = response as? HTTPURLResponse, (200...299).contains(http.statusCode) else {
      throw MobileForgeClientError.httpFailure
    }
    if let refusal = try? JSONDecoder().decode(ErrorEnvelope.self, from: data) {
      throw MobileForgeClientError.serverRefused(refusal.error, refusal.detail)
    }
    let decoder = JSONDecoder()
    decoder.dateDecodingStrategy = .custom { decoder in
      let container = try decoder.singleValueContainer()
      let value = try container.decode(String.self)
      let fractional = ISO8601DateFormatter()
      fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
      if let date = fractional.date(from: value) ?? ISO8601DateFormatter().date(from: value) {
        return date
      }
      throw DecodingError.dataCorruptedError(
        in: container, debugDescription: "Invalid receipt timestamp")
    }
    return try decoder.decode(type, from: data)
  }
}

public enum MobileForgeClientError: Error, Equatable, LocalizedError {
  case httpFailure
  case serverRefused(String, String)
  case unsafeRelayEnvelope
  case replayContinuityLost

  public var errorDescription: String? {
    switch self {
    case .httpFailure: "MDx could not reach the Forge control plane."
    case .serverRefused(_, let detail): detail
    case .unsafeRelayEnvelope: "MDx refused an unsafe mobile event."
    case .replayContinuityLost: "MDx is refreshing this build from its durable source."
    }
  }
}
