import Foundation

public struct MobileHandoffRepository: Codable, Equatable, Identifiable, Sendable {
  public let repositoryID: String
  public let displayName: String
  public let kind: String
  public let identityFingerprint: String
  public let defaultBranch: String
  public let defaultBranchSource: String
  public let suggestedChecks: [String]?
  public let rootRecorded: Bool
  public var id: String { repositoryID }

  enum CodingKeys: String, CodingKey {
    case kind
    case repositoryID = "repository_id"
    case displayName = "display_name"
    case identityFingerprint = "identity_fingerprint"
    case defaultBranch = "default_branch"
    case defaultBranchSource = "default_branch_source"
    case suggestedChecks = "suggested_checks"
    case rootRecorded = "root_recorded"
  }
}

public struct MobileHandoffCloudEnvironment: Codable, Equatable, Identifiable, Sendable {
  public let environmentID: String
  public let repositoryID: String
  public let displayName: String
  public let identityFingerprint: String
  public let verified: Bool
  public var id: String { environmentID }

  enum CodingKeys: String, CodingKey {
    case verified
    case environmentID = "environment_id"
    case repositoryID = "repository_id"
    case displayName = "display_name"
    case identityFingerprint = "identity_fingerprint"
  }
}

public struct MobileHandoffTargets: Codable, Equatable, Sendable {
  public let status: String
  public let pairedHostRepositories: [MobileHandoffRepository]
  public let cloudEnvironments: [MobileHandoffCloudEnvironment]
  public let rawRootsIncluded: Bool
  public let originValuesIncluded: Bool
  public let containsSecretValues: Bool
  public let grantsExecutionAuthority: Bool
  public let productionWriteAllowed: Bool

  enum CodingKeys: String, CodingKey {
    case status
    case pairedHostRepositories = "paired_host_repositories"
    case cloudEnvironments = "cloud_environments"
    case rawRootsIncluded = "raw_roots_included"
    case originValuesIncluded = "origin_values_included"
    case containsSecretValues = "contains_secret_values"
    case grantsExecutionAuthority = "grants_execution_authority"
    case productionWriteAllowed = "production_write_allowed"
  }
}

public struct MobileSupportDiagnostics: Codable, Equatable, Sendable {
  public let status: String
  public let tenantFingerprint: String
  public let forgeEventCount: Int
  public let reviewableSessionCount: Int
  public let lastReceiptKind: String?
  public let lastReceiptAt: Date?
  public let relayConfigured: Bool
  public let sourceHostLiveEnabled: Bool
  public let hostedSandboxConfigured: Bool
  public let safeExportFields: [String]
  public let excludedFromSupportExport: [String]
  public let rawSourceIncluded: Bool
  public let credentialValuesIncluded: Bool

  enum CodingKeys: String, CodingKey {
    case status
    case tenantFingerprint = "tenant_fingerprint"
    case forgeEventCount = "forge_event_count"
    case reviewableSessionCount = "reviewable_session_count"
    case lastReceiptKind = "last_receipt_kind"
    case lastReceiptAt = "last_receipt_at"
    case relayConfigured = "relay_configured"
    case sourceHostLiveEnabled = "source_host_live_enabled"
    case hostedSandboxConfigured = "hosted_sandbox_configured"
    case safeExportFields = "safe_export_fields"
    case excludedFromSupportExport = "excluded_from_support_export"
    case rawSourceIncluded = "raw_source_included"
    case credentialValuesIncluded = "credential_values_included"
  }
}

public struct MobilePrivacyProjection: Codable, Equatable, Sendable {
  public let status: String
  public let mobileProjectionRetention: String
  public let relayEventRetention: String
  public let hostedWorkspaceRetention: String
  public let offlineDrafts: String
  public let supportExport: String
  public let deleteLocalDataSupported: Bool
  public let accountDeletionExecution: String
  public let silentAccountDeletionClaimAllowed: Bool

  enum CodingKeys: String, CodingKey {
    case status
    case mobileProjectionRetention = "mobile_projection_retention"
    case relayEventRetention = "relay_event_retention"
    case hostedWorkspaceRetention = "hosted_workspace_retention"
    case offlineDrafts = "offline_drafts"
    case supportExport = "support_export"
    case deleteLocalDataSupported = "delete_local_data_supported"
    case accountDeletionExecution = "account_deletion_execution"
    case silentAccountDeletionClaimAllowed = "silent_account_deletion_claim_allowed"
  }
}

public struct MobileAccountDeletionResult: Codable, Equatable, Sendable {
  public let status: String
  public let requestID: String
  public let requestReceiptID: String
  public let nextSafeAction: String
  public let accountReportedDeleted: Bool

  enum CodingKeys: String, CodingKey {
    case status
    case requestID = "request_id"
    case requestReceiptID = "request_receipt_id"
    case nextSafeAction = "next_safe_action"
    case accountReportedDeleted = "account_reported_deleted"
  }
}

public struct MobileAccountDeletionRequest: Codable, Equatable, Sendable {
  public let requestID: String
  public let confirm: Bool
  public let userPresence: String
  public let actorDeviceID: String
  public let issuedAt: Date
  public let expiresAt: Date
  public var deviceSignature: String

  public init(
    requestID: String = "mobile_account_deletion_\(UUID().uuidString.lowercased())",
    actorDeviceID: String,
    userPresence: String = "biometric_verified",
    issuedAt: Date = Date(),
    expiresAt: Date = Date().addingTimeInterval(120),
    deviceSignature: String = ""
  ) {
    self.requestID = requestID
    confirm = true
    self.userPresence = userPresence
    self.actorDeviceID = actorDeviceID
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
    case requestID = "request_id"
    case confirm
    case userPresence = "user_presence"
    case actorDeviceID = "actor_device_id"
    case issuedAt = "issued_at"
    case expiresAt = "expires_at"
    case deviceSignature = "device_signature"
  }
}

extension MobileForgeClient {
  public func handoffTargets() async throws -> MobileHandoffTargets {
    let url = baseURL.appending(path: "mobile/handoff-targets.json")
    let (data, response) = try await authorizedData(from: url)
    return try decode(MobileHandoffTargets.self, data: data, response: response)
  }

  public func supportDiagnostics() async throws -> MobileSupportDiagnostics {
    let url = baseURL.appending(path: "mobile/support-diagnostics.json")
    let (data, response) = try await authorizedData(from: url)
    return try decode(MobileSupportDiagnostics.self, data: data, response: response)
  }

  public func privacy() async throws -> MobilePrivacyProjection {
    let url = baseURL.appending(path: "mobile/privacy.json")
    let (data, response) = try await authorizedData(from: url)
    return try decode(MobilePrivacyProjection.self, data: data, response: response)
  }

  public func requestAccountDeletion(
    _ requestBody: MobileAccountDeletionRequest
  ) async throws -> MobileAccountDeletionResult {
    let url = baseURL.appending(path: "mobile/account-deletion-requests.json")
    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    let encoder = JSONEncoder()
    encoder.dateEncodingStrategy = .iso8601
    request.httpBody = try encoder.encode(requestBody)
    await authorize(&request)
    let (data, response) = try await session.data(for: request)
    return try decode(MobileAccountDeletionResult.self, data: data, response: response)
  }
}
