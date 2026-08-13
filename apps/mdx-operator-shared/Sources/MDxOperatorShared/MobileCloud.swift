import Foundation

public struct MobileCloudInstallation: Codable, Equatable, Identifiable, Sendable {
  public let installationID: UInt64
  public let accountLogin: String
  public let accountType: String
  public let status: String
  public let receiptID: String
  public let tokenRecorded: Bool

  public var id: UInt64 { installationID }

  enum CodingKeys: String, CodingKey {
    case status
    case installationID = "installation_id"
    case accountLogin = "account_login"
    case accountType = "account_type"
    case receiptID = "receipt_id"
    case tokenRecorded = "token_recorded"
  }
}

public struct MobileCloudRepository: Codable, Equatable, Identifiable, Sendable {
  public let repositoryID: UInt64
  public let fullName: String
  public let displayName: String?
  public let defaultBranch: String?
  public let isPrivate: Bool?
  public let repoID: String?
  public let sourceRevision: String?
  public let status: String?
  public let receiptID: String?
  public let tokenRecorded: Bool?

  public var id: UInt64 { repositoryID }

  enum CodingKeys: String, CodingKey {
    case status
    case repositoryID = "repository_id"
    case fullName = "full_name"
    case displayName = "display_name"
    case defaultBranch = "default_branch"
    case isPrivate = "private"
    case repoID = "repo_id"
    case sourceRevision = "source_revision"
    case receiptID = "receipt_id"
    case tokenRecorded = "token_recorded"
  }
}

public struct MobileCloudEnvironmentSummary: Codable, Equatable, Identifiable, Sendable {
  public let environmentID: String
  public let environmentVersion: UInt64
  public let repoID: String
  public let fingerprint: String
  public let status: String
  public let readyForCloudBuilds: Bool
  public let receiptID: String
  public let secretValuesRecorded: Bool

  public var id: String { environmentID }

  enum CodingKeys: String, CodingKey {
    case status, fingerprint
    case environmentID = "environment_id"
    case environmentVersion = "environment_version"
    case repoID = "repo_id"
    case readyForCloudBuilds = "ready_for_cloud_builds"
    case receiptID = "receipt_id"
    case secretValuesRecorded = "secret_values_recorded"
  }
}

public struct MobileCloudSetupProjection: Codable, Equatable, Sendable {
  public let status: String
  public let tenantID: String
  public let installations: [MobileCloudInstallation]
  public let repositories: [MobileCloudRepository]
  public let environments: [MobileCloudEnvironmentSummary]
  public let readyEnvironmentCount: Int
  public let githubAppConfigured: Bool
  public let hostedSandboxStatus: String
  public let secretValuesIncluded: Bool
  public let productionWriteAllowed: Bool

  enum CodingKeys: String, CodingKey {
    case status, installations, repositories, environments
    case tenantID = "tenant_id"
    case readyEnvironmentCount = "ready_environment_count"
    case githubAppConfigured = "github_app_configured"
    case hostedSandboxStatus = "hosted_sandbox_status"
    case secretValuesIncluded = "secret_values_included"
    case productionWriteAllowed = "production_write_allowed"
  }
}

public struct MobileGitHubInstallationSession: Codable, Equatable, Sendable {
  public let status: String
  public let installURL: URL
  public let state: String
  public let expiresAtEpoch: UInt64
  public let requestedPermissions: [String]
  public let personalAccessTokenRequested: Bool
  public let productionWriteAllowed: Bool

  enum CodingKeys: String, CodingKey {
    case status, state
    case installURL = "install_url"
    case expiresAtEpoch = "expires_at_epoch"
    case requestedPermissions = "requested_permissions"
    case personalAccessTokenRequested = "personal_access_token_requested"
    case productionWriteAllowed = "production_write_allowed"
  }
}

public struct MobileGitHubInstallationResult: Codable, Equatable, Sendable {
  public let status: String
  public let installationID: UInt64
  public let accountLogin: String
  public let accountType: String
  public let installationReceiptID: String
  public let tokenRecorded: Bool
  public let productionWriteAllowed: Bool

  enum CodingKeys: String, CodingKey {
    case status
    case installationID = "installation_id"
    case accountLogin = "account_login"
    case accountType = "account_type"
    case installationReceiptID = "installation_receipt_id"
    case tokenRecorded = "token_recorded"
    case productionWriteAllowed = "production_write_allowed"
  }
}

public struct MobileGitHubRepositoryList: Codable, Equatable, Sendable {
  public let status: String
  public let installationID: UInt64
  public let repositories: [MobileCloudRepository]
  public let repositoryCount: Int
  public let tokenRecorded: Bool
  public let productionWriteAllowed: Bool

  enum CodingKeys: String, CodingKey {
    case status, repositories
    case installationID = "installation_id"
    case repositoryCount = "repository_count"
    case tokenRecorded = "token_recorded"
    case productionWriteAllowed = "production_write_allowed"
  }
}

public struct MobileCloudRepositoryConnection: Codable, Equatable, Sendable {
  public let status: String
  public let repoID: String
  public let repository: MobileCloudRepository
  public let sourceRevision: String
  public let cloudConnectionReceiptID: String
  public let tokenRecorded: Bool
  public let nextSafeAction: String
  public let productionWriteAllowed: Bool

  enum CodingKeys: String, CodingKey {
    case status, repository
    case repoID = "repo_id"
    case sourceRevision = "source_revision"
    case cloudConnectionReceiptID = "cloud_connection_receipt_id"
    case tokenRecorded = "token_recorded"
    case nextSafeAction = "next_safe_action"
    case productionWriteAllowed = "production_write_allowed"
  }
}

public struct MobileCloudEnvironmentResult: Codable, Equatable, Sendable {
  public let status: String
  public let environmentFingerprint: String
  public let environmentReceiptID: String
  public let baseImageDigestRequired: Bool
  public let verificationRequired: Bool
  public let readyForCloudBuilds: Bool
  public let secretValuesRecorded: Bool
  public let nextSafeAction: String
  public let productionWriteAllowed: Bool

  enum CodingKeys: String, CodingKey {
    case status
    case environmentFingerprint = "environment_fingerprint"
    case environmentReceiptID = "environment_receipt_id"
    case baseImageDigestRequired = "base_image_digest_required"
    case verificationRequired = "verification_required"
    case readyForCloudBuilds = "ready_for_cloud_builds"
    case secretValuesRecorded = "secret_values_recorded"
    case nextSafeAction = "next_safe_action"
    case productionWriteAllowed = "production_write_allowed"
  }
}

extension MobileForgeClient {
  public func cloudSetup() async throws -> MobileCloudSetupProjection {
    let url = baseURL.appending(path: "mobile/cloud/setup.json")
    let (data, response) = try await authorizedData(from: url)
    return try decode(MobileCloudSetupProjection.self, data: data, response: response)
  }

  public func beginGitHubInstallation() async throws -> MobileGitHubInstallationSession {
    try await cloudPOST(
      MobileGitHubInstallationSession.self,
      path: "mobile/cloud/github-installation-sessions.json",
      body: BeginInstallationRequest())
  }

  public func completeGitHubInstallation(
    state: String,
    installationID: UInt64
  ) async throws -> MobileGitHubInstallationResult {
    try await cloudPOST(
      MobileGitHubInstallationResult.self,
      path: "mobile/cloud/github-installations.json",
      body: CompleteInstallationRequest(state: state, installationID: installationID))
  }

  public func githubRepositories(installationID: UInt64) async throws
    -> MobileGitHubRepositoryList
  {
    var components = URLComponents(
      url: baseURL.appending(path: "mobile/cloud/repositories.json"),
      resolvingAgainstBaseURL: false)
    components?.queryItems = [URLQueryItem(name: "installation_id", value: String(installationID))]
    guard let url = components?.url else { throw MobileForgeClientError.httpFailure }
    let (data, response) = try await authorizedData(from: url)
    return try decode(MobileGitHubRepositoryList.self, data: data, response: response)
  }

  public func connectCloudRepository(
    installationID: UInt64,
    repositoryID: UInt64
  ) async throws -> MobileCloudRepositoryConnection {
    try await cloudPOST(
      MobileCloudRepositoryConnection.self,
      path: "mobile/cloud/repository-connections.json",
      body: ConnectRepositoryRequest(
        installationID: installationID, repositoryID: repositoryID))
  }

  public func prepareCloudEnvironment(repoID: String) async throws
    -> MobileCloudEnvironmentResult
  {
    try await cloudPOST(
      MobileCloudEnvironmentResult.self,
      path: "mobile/cloud/environments.json",
      body: PrepareEnvironmentRequest(repoID: repoID))
  }

  public func verifyCloudEnvironment(repoID: String) async throws
    -> MobileCloudEnvironmentResult
  {
    try await cloudPOST(
      MobileCloudEnvironmentResult.self,
      path: "mobile/cloud/environments.json",
      body: VerifyEnvironmentRequest(repoID: repoID))
  }

  private func cloudPOST<Request: Encodable & Sendable, Response: Decodable>(
    _ type: Response.Type,
    path: String,
    body: Request
  ) async throws -> Response {
    let url = baseURL.appending(path: path)
    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.httpBody = try JSONEncoder().encode(body)
    await authorize(&request)
    let (data, response) = try await session.data(for: request)
    return try decode(type, data: data, response: response)
  }
}

private struct BeginInstallationRequest: Encodable, Sendable {
  let intent = "connect_selected_repositories"
}

private struct CompleteInstallationRequest: Encodable, Sendable {
  let state: String
  let installationID: UInt64

  enum CodingKeys: String, CodingKey {
    case state
    case installationID = "installation_id"
  }
}

private struct ConnectRepositoryRequest: Encodable, Sendable {
  let installationID: UInt64
  let repositoryID: UInt64

  enum CodingKeys: String, CodingKey {
    case installationID = "installation_id"
    case repositoryID = "repository_id"
  }
}

private struct PrepareEnvironmentRequest: Encodable, Sendable {
  let action = "prepare"
  let repoID: String

  enum CodingKeys: String, CodingKey {
    case action
    case repoID = "repo_id"
  }
}

private struct VerifyEnvironmentRequest: Encodable, Sendable {
  let action = "verify"
  let repoID: String

  enum CodingKeys: String, CodingKey {
    case action
    case repoID = "repo_id"
  }
}
