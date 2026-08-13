import Foundation

public struct MobilePairingChallenge: Codable, Equatable, Sendable {
  public let challengeID: String
  public let tenantID: String
  public let userID: String
  public let deviceID: String
  public let hostID: String
  public let hostPublicKeyThumbprint: String
  public let challengeToken: String
  public let signature: String
  public let serverPublicKey: String
  public let issuedAtEpoch: UInt64
  public let expiresAtEpoch: UInt64
  public let singleUse: Bool

  public var isExpired: Bool {
    UInt64(Date().timeIntervalSince1970) > expiresAtEpoch
  }

  enum CodingKeys: String, CodingKey {
    case challengeID = "challenge_id"
    case tenantID = "tenant_id"
    case userID = "user_id"
    case deviceID = "device_id"
    case hostID = "host_id"
    case hostPublicKeyThumbprint = "host_public_key_thumbprint"
    case challengeToken = "challenge_token"
    case signature
    case serverPublicKey = "server_public_key"
    case issuedAtEpoch = "issued_at_epoch"
    case expiresAtEpoch = "expires_at_epoch"
    case singleUse = "single_use"
  }
}

public struct MobileDeviceDescriptor: Codable, Equatable, Identifiable, Sendable {
  public let deviceID: String
  public let platform: String
  public let displayName: String
  public let publicKeyThumbprint: String
  public let attestationRisk: String
  public let attestationVerification: String?
  public let appAttestEnvironment: String?
  public let appAttestValidationCategory: UInt32?
  public let appAttestBundleVersion: String?
  public let attestedAtEpoch: UInt64?
  public let attestationReceiptID: String?
  public let attestationRestartDurable: Bool?
  public let status: String
  public let registeredAtEpoch: UInt64
  public let revokedAtEpoch: UInt64?

  public var id: String { deviceID }

  enum CodingKeys: String, CodingKey {
    case deviceID = "device_id"
    case platform
    case displayName = "display_name"
    case publicKeyThumbprint = "public_key_thumbprint"
    case attestationRisk = "attestation_risk"
    case attestationVerification = "attestation_verification"
    case appAttestEnvironment = "app_attest_environment"
    case appAttestValidationCategory = "app_attest_validation_category"
    case appAttestBundleVersion = "app_attest_bundle_version"
    case attestedAtEpoch = "attested_at_epoch"
    case attestationReceiptID = "attestation_receipt_id"
    case attestationRestartDurable = "attestation_restart_durable"
    case status
    case registeredAtEpoch = "registered_at_epoch"
    case revokedAtEpoch = "revoked_at_epoch"
  }
}

public struct MobileAppAttestChallenge: Codable, Equatable, Sendable {
  public let status: String
  public let challengeID: String
  public let deviceID: String
  public let challenge: String
  public let environment: String
  public let issuedAtEpoch: UInt64
  public let expiresAtEpoch: UInt64
  public let singleUse: Bool
  public let serverVerificationRequired: Bool

  public var isExpired: Bool {
    UInt64(Date().timeIntervalSince1970) > expiresAtEpoch
  }

  enum CodingKeys: String, CodingKey {
    case status
    case challengeID = "challenge_id"
    case deviceID = "device_id"
    case challenge
    case environment
    case issuedAtEpoch = "issued_at_epoch"
    case expiresAtEpoch = "expires_at_epoch"
    case singleUse = "single_use"
    case serverVerificationRequired = "server_verification_required"
  }
}

public struct MobileAppAttestationReceipt: Codable, Equatable, Sendable {
  public let status: String
  public let deviceID: String
  public let attestationRisk: String
  public let attestationVerification: String
  public let environment: String
  public let validationCategory: UInt32
  public let bundleVersion: String
  public let receiptID: String
  public let stateSource: String
  public let restartDurable: Bool
  public let rawAttestationRecorded: Bool
  public let rawAppleReceiptRecorded: Bool
  public let idempotentReplay: Bool
  public let authorityOpened: String

  enum CodingKeys: String, CodingKey {
    case status
    case deviceID = "device_id"
    case attestationRisk = "attestation_risk"
    case attestationVerification = "attestation_verification"
    case environment
    case validationCategory = "validation_category"
    case bundleVersion = "bundle_version"
    case receiptID = "receipt_id"
    case stateSource = "state_source"
    case restartDurable = "restart_durable"
    case rawAttestationRecorded = "raw_attestation_recorded"
    case rawAppleReceiptRecorded = "raw_apple_receipt_recorded"
    case idempotentReplay = "idempotent_replay"
    case authorityOpened = "authority_opened"
  }
}

public struct MobileHostDescriptor: Codable, Equatable, Identifiable, Sendable {
  public let hostID: String
  public let displayName: String
  public let publicKeyThumbprint: String
  public let status: String
  public let connectorStatus: String
  public let registeredAtEpoch: UInt64
  public let revokedAtEpoch: UInt64?
  public let acceptsInboundConnections: Bool

  public var id: String { hostID }

  enum CodingKeys: String, CodingKey {
    case hostID = "host_id"
    case displayName = "display_name"
    case publicKeyThumbprint = "public_key_thumbprint"
    case status
    case connectorStatus = "connector_status"
    case registeredAtEpoch = "registered_at_epoch"
    case revokedAtEpoch = "revoked_at_epoch"
    case acceptsInboundConnections = "accepts_inbound_connections"
  }
}

public struct MobilePairingDescriptor: Codable, Equatable, Identifiable, Sendable {
  public let pairingID: String
  public let deviceID: String
  public let hostID: String
  public let pairedAtEpoch: UInt64
  public let pairingStatus: String

  public var id: String { pairingID }

  enum CodingKeys: String, CodingKey {
    case pairingID = "pairing_id"
    case deviceID = "device_id"
    case hostID = "host_id"
    case pairedAtEpoch = "paired_at_epoch"
    case pairingStatus = "pairing_status"
  }
}

public struct MobileTrustProjection: Codable, Equatable, Sendable {
  public let tenantID: String
  public let userID: String
  public let devices: [MobileDeviceDescriptor]
  public let hosts: [MobileHostDescriptor]
  public let pairings: [MobilePairingDescriptor]
  public let hostConnectorDirection: String
  public let inboundHostListenerAllowed: Bool

  enum CodingKeys: String, CodingKey {
    case devices, hosts, pairings
    case tenantID = "tenant_id"
    case userID = "user_id"
    case hostConnectorDirection = "host_connector_direction"
    case inboundHostListenerAllowed = "inbound_host_listener_allowed"
  }
}

public struct MobilePairingReceipt: Codable, Equatable, Sendable {
  public let status: String
  public let pairingID: String
  public let deviceID: String
  public let hostID: String
  public let pairedAtEpoch: UInt64
  public let pairingStatus: String

  enum CodingKeys: String, CodingKey {
    case status
    case pairingID = "pairing_id"
    case deviceID = "device_id"
    case hostID = "host_id"
    case pairedAtEpoch = "paired_at_epoch"
    case pairingStatus = "pairing_status"
  }
}

public enum MobilePairingPayload {
  public static func encode(_ challenge: MobilePairingChallenge) throws -> String {
    let data = try JSONEncoder().encode(challenge)
    return data.base64EncodedString()
  }

  public static func decode(_ value: String) throws -> MobilePairingChallenge {
    let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !normalized.isEmpty,
      let data = Data(base64Encoded: normalized),
      let challenge = try? JSONDecoder().decode(MobilePairingChallenge.self, from: data)
    else {
      throw MobilePairingClientError.invalidPairingPayload
    }
    return challenge
  }
}

public struct MobilePairingClient: Sendable {
  private struct ErrorEnvelope: Codable {
    let error: String
    let detail: String
  }

  private let baseURL: URL
  private let session: URLSession
  private let accessTokenProvider: MobileAccessTokenProvider?

  public init(
    baseURL: URL,
    session: URLSession = .shared,
    accessTokenProvider: MobileAccessTokenProvider? = nil
  ) {
    self.baseURL = baseURL
    self.session = session
    self.accessTokenProvider = accessTokenProvider
  }

  private func authorize(_ request: inout URLRequest) async {
    if let token = await accessTokenProvider?(), !token.isEmpty {
      request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
    }
  }

  func authorizedRequest(url: URL) async -> URLRequest {
    var request = URLRequest(url: url)
    await authorize(&request)
    return request
  }

  public func registerDevice(
    identity: MobileDeviceIdentity,
    displayName: String
  ) async throws -> MobileDeviceDescriptor {
    let body: [String: String] = [
      "device_id": identity.deviceID,
      "platform": "ios",
      "display_name": displayName,
      "public_key": identity.publicKey,
      "public_key_thumbprint": identity.publicKeyThumbprint,
    ]
    return try await post("mobile/devices.json", body: body)
  }

  public func mintAppAttestChallenge(deviceID: String, keyID: String) async throws
    -> MobileAppAttestChallenge
  {
    try await post(
      "mobile/app-attest-challenges.json",
      body: ["device_id": deviceID, "key_id": keyID]
    )
  }

  public func submitAppAttestation(
    challengeID: String,
    deviceID: String,
    keyID: String,
    attestationObject: Data
  ) async throws -> MobileAppAttestationReceipt {
    try await post(
      "mobile/app-attestations.json",
      body: [
        "challenge_id": challengeID,
        "device_id": deviceID,
        "key_id": keyID,
        "attestation_object": attestationObject.base64EncodedString(),
      ]
    )
  }

  public func registerHost(
    identity: MobileDeviceIdentity,
    displayName: String
  ) async throws -> MobileHostDescriptor {
    let body: [String: JSONValue] = [
      "host_id": .string(identity.deviceID.replacingOccurrences(of: "device_", with: "host_")),
      "display_name": .string(displayName),
      "public_key_thumbprint": .string(identity.publicKeyThumbprint),
      "accepts_inbound_connections": .bool(false),
    ]
    return try await post("mobile/hosts.json", body: body)
  }

  public func mintChallenge(deviceID: String, hostID: String) async throws
    -> MobilePairingChallenge
  {
    try await post(
      "mobile/pairing-challenges.json",
      body: ["device_id": deviceID, "host_id": hostID]
    )
  }

  public func redeem(
    _ challenge: MobilePairingChallenge,
    deviceSignature: String
  ) async throws -> MobilePairingReceipt {
    guard !challenge.isExpired, challenge.singleUse else {
      throw MobilePairingClientError.expiredPairingPayload
    }
    return try await post(
      "mobile/pairings.json",
      body: [
        "challenge_id": challenge.challengeID,
        "challenge_token": challenge.challengeToken,
        "signature": challenge.signature,
        "device_signature": deviceSignature,
      ]
    )
  }

  public func projection() async throws -> MobileTrustProjection {
    let url = baseURL.appending(path: "mobile/trust/projection.json")
    let request = await authorizedRequest(url: url)
    let (data, response) = try await session.data(for: request)
    return try decode(data: data, response: response)
  }

  public func revokeDevice(_ deviceID: String) async throws -> MobileDeviceDescriptor {
    try await post("mobile/device-revocations.json", body: ["device_id": deviceID])
  }

  public func revokeHost(_ hostID: String) async throws -> MobileHostDescriptor {
    try await post("mobile/host-revocations.json", body: ["host_id": hostID])
  }

  private func post<Response: Decodable, Body: Encodable>(
    _ path: String,
    body: Body
  ) async throws -> Response {
    let url = baseURL.appending(path: path)
    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.httpBody = try JSONEncoder().encode(body)
    await authorize(&request)
    let (data, response) = try await session.data(for: request)
    return try decode(data: data, response: response)
  }

  private func decode<Response: Decodable>(data: Data, response: URLResponse) throws -> Response {
    guard let http = response as? HTTPURLResponse, (200...299).contains(http.statusCode) else {
      let envelope = try? JSONDecoder().decode(ErrorEnvelope.self, from: data)
      throw MobilePairingClientError.serverRefused(
        envelope?.error ?? "mobile_pairing_http_failure",
        envelope?.detail ?? "The pairing service refused the request."
      )
    }
    if let envelope = try? JSONDecoder().decode(ErrorEnvelope.self, from: data) {
      throw MobilePairingClientError.serverRefused(envelope.error, envelope.detail)
    }
    return try JSONDecoder().decode(Response.self, from: data)
  }
}

public enum MobilePairingClientError: Error, Equatable, LocalizedError {
  case invalidPairingPayload
  case expiredPairingPayload
  case serverRefused(String, String)

  public var errorDescription: String? {
    switch self {
    case .invalidPairingPayload:
      "This is not an MDx pairing code."
    case .expiredPairingPayload:
      "This pairing code expired. Ask the Mac for a new one."
    case .serverRefused(_, let detail):
      detail
    }
  }
}

public enum JSONValue: Encodable, Sendable {
  case string(String)
  case bool(Bool)

  public func encode(to encoder: Encoder) throws {
    var container = encoder.singleValueContainer()
    switch self {
    case .string(let value):
      try container.encode(value)
    case .bool(let value):
      try container.encode(value)
    }
  }
}

public struct HostConnectorConfiguration: Equatable, Sendable {
  public let relayURL: URL
  public let hostID: String
  public let credentialRef: String
  public let credentialExpiresAt: Date
  public let reconnectLimit: Int

  public var acceptsInboundConnections: Bool { false }

  public init(
    relayURL: URL,
    hostID: String,
    credentialRef: String,
    credentialExpiresAt: Date,
    reconnectLimit: Int = 8
  ) throws {
    guard relayURL.scheme?.lowercased() == "wss", relayURL.host != nil else {
      throw HostConnectorConfigurationError.secureRelayRequired
    }
    guard !hostID.isEmpty, !credentialRef.isEmpty else {
      throw HostConnectorConfigurationError.identityRequired
    }
    guard credentialExpiresAt > Date() else {
      throw HostConnectorConfigurationError.expiredCredential
    }
    guard (1...20).contains(reconnectLimit) else {
      throw HostConnectorConfigurationError.reconnectLimitInvalid
    }
    self.relayURL = relayURL
    self.hostID = hostID
    self.credentialRef = credentialRef
    self.credentialExpiresAt = credentialExpiresAt
    self.reconnectLimit = reconnectLimit
  }
}

public enum HostConnectorConfigurationError: Error, Equatable {
  case secureRelayRequired
  case identityRequired
  case expiredCredential
  case reconnectLimitInvalid
}
