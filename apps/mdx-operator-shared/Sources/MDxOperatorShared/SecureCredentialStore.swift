import CryptoKit
import Foundation
import Security

public protocol SecureCredentialStoring: Sendable {
  func save(_ data: Data, account: String, accessibility: CFString) throws
  func load(account: String) throws -> Data?
  func delete(account: String) throws
}

public struct KeychainCredentialStore: SecureCredentialStoring, Sendable {
  private let service: String

  public init(service: String = "com.mdx.forge-anywhere") {
    self.service = service
  }

  public func save(
    _ data: Data,
    account: String,
    accessibility: CFString = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
  ) throws {
    let identity: [CFString: Any] = [
      kSecClass: kSecClassGenericPassword,
      kSecAttrService: service,
      kSecAttrAccount: account,
    ]
    let attributes: [CFString: Any] = [
      kSecAttrAccessible: accessibility,
      kSecValueData: data,
    ]
    let updateStatus = SecItemUpdate(identity as CFDictionary, attributes as CFDictionary)
    if updateStatus == errSecSuccess { return }
    guard updateStatus == errSecItemNotFound else {
      throw KeychainCredentialError.unexpectedStatus(updateStatus)
    }
    var insertion = identity
    insertion.merge(attributes) { _, new in new }
    let status = SecItemAdd(insertion as CFDictionary, nil)
    guard status == errSecSuccess else {
      throw KeychainCredentialError.unexpectedStatus(status)
    }
  }

  public func load(account: String) throws -> Data? {
    let query: [CFString: Any] = [
      kSecClass: kSecClassGenericPassword,
      kSecAttrService: service,
      kSecAttrAccount: account,
      kSecReturnData: true,
      kSecMatchLimit: kSecMatchLimitOne,
    ]
    var result: CFTypeRef?
    let status = SecItemCopyMatching(query as CFDictionary, &result)
    if status == errSecItemNotFound { return nil }
    guard status == errSecSuccess, let data = result as? Data else {
      throw KeychainCredentialError.unexpectedStatus(status)
    }
    return data
  }

  public func delete(account: String) throws {
    let query: [CFString: Any] = [
      kSecClass: kSecClassGenericPassword,
      kSecAttrService: service,
      kSecAttrAccount: account,
    ]
    let status = SecItemDelete(query as CFDictionary)
    guard status == errSecSuccess || status == errSecItemNotFound else {
      throw KeychainCredentialError.unexpectedStatus(status)
    }
  }
}

public enum KeychainCredentialError: Error, Equatable {
  case unexpectedStatus(OSStatus)
}

public struct MobileDeviceIdentity: Codable, Equatable, Sendable {
  public let deviceID: String
  public let publicKey: String
  public let publicKeyThumbprint: String
  public let createdAt: Date
  public let rotatedAt: Date?

  public init(
    deviceID: String,
    publicKey: String,
    publicKeyThumbprint: String,
    createdAt: Date,
    rotatedAt: Date?
  ) {
    self.deviceID = deviceID
    self.publicKey = publicKey
    self.publicKeyThumbprint = publicKeyThumbprint
    self.createdAt = createdAt
    self.rotatedAt = rotatedAt
  }
}

public struct MobileDeviceIdentityManager: Sendable {
  private struct Metadata: Codable {
    let deviceID: String
    let createdAt: Date
    let rotatedAt: Date?
  }

  private let store: any SecureCredentialStoring
  private let namespace: String

  public init(
    store: any SecureCredentialStoring = KeychainCredentialStore(),
    namespace: String
  ) {
    self.store = store
    self.namespace = namespace
  }

  public func loadOrCreate(now: Date = Date()) throws -> MobileDeviceIdentity {
    if let privateKeyData = try store.load(account: keyAccount),
      let metadataData = try store.load(account: metadataAccount),
      let privateKey = try? P256.Signing.PrivateKey(rawRepresentation: privateKeyData),
      let metadata = try? JSONDecoder().decode(Metadata.self, from: metadataData)
    {
      return identity(privateKey: privateKey, metadata: metadata)
    }
    return try create(now: now, rotatedAt: nil, deviceID: nil)
  }

  public func rotateKey(now: Date = Date()) throws -> MobileDeviceIdentity {
    let current = try loadOrCreate(now: now)
    return try create(now: current.createdAt, rotatedAt: now, deviceID: current.deviceID)
  }

  public func signPairingChallenge(_ challengeToken: String) throws -> String {
    try signDeviceBoundPayload(Data(challengeToken.utf8))
  }

  public func signCommandAttestation(_ command: MobileSessionCommand) throws -> String {
    try signDeviceBoundPayload(command.attestationPayload())
  }

  public func signAccountDeletionAttestation(
    _ request: MobileAccountDeletionRequest
  ) throws -> String {
    try signDeviceBoundPayload(request.attestationPayload())
  }

  public func signPushRegistrationAttestation(
    _ request: MobilePushRegistrationRequest
  ) throws -> String {
    try signDeviceBoundPayload(request.attestationPayload())
  }

  private func signDeviceBoundPayload(_ payload: Data) throws -> String {
    guard let privateKeyData = try store.load(account: keyAccount),
      let privateKey = try? P256.Signing.PrivateKey(rawRepresentation: privateKeyData)
    else {
      throw MobileDeviceIdentityError.privateKeyMissingOrInvalid
    }
    let signature = try privateKey.signature(for: payload)
    return base64URLEncoded(signature.derRepresentation)
  }

  public func revokeLocalIdentity() throws {
    try store.delete(account: keyAccount)
    try store.delete(account: metadataAccount)
  }

  private func create(
    now: Date,
    rotatedAt: Date?,
    deviceID: String?
  ) throws -> MobileDeviceIdentity {
    let privateKey = P256.Signing.PrivateKey()
    let metadata = Metadata(
      deviceID: deviceID ?? "device_\(UUID().uuidString.lowercased())",
      createdAt: now,
      rotatedAt: rotatedAt
    )
    try store.save(
      privateKey.rawRepresentation,
      account: keyAccount,
      accessibility: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    )
    try store.save(
      JSONEncoder().encode(metadata),
      account: metadataAccount,
      accessibility: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    )
    return identity(privateKey: privateKey, metadata: metadata)
  }

  private func identity(
    privateKey: P256.Signing.PrivateKey,
    metadata: Metadata
  ) -> MobileDeviceIdentity {
    let digest = SHA256.hash(data: privateKey.publicKey.x963Representation)
    let thumbprint = digest.map { String(format: "%02x", $0) }.joined()
    return MobileDeviceIdentity(
      deviceID: metadata.deviceID,
      publicKey: base64URLEncoded(privateKey.publicKey.x963Representation),
      publicKeyThumbprint: thumbprint,
      createdAt: metadata.createdAt,
      rotatedAt: metadata.rotatedAt
    )
  }

  private var keyAccount: String { "\(namespace).device-private-key" }
  private var metadataAccount: String { "\(namespace).device-metadata" }
}

public enum MobileDeviceIdentityError: Error, Equatable, LocalizedError {
  case privateKeyMissingOrInvalid

  public var errorDescription: String? {
    "This device's pairing key is unavailable. Register it again before pairing."
  }
}

private func base64URLEncoded(_ data: Data) -> String {
  data.base64EncodedString()
    .replacingOccurrences(of: "+", with: "-")
    .replacingOccurrences(of: "/", with: "_")
    .replacingOccurrences(of: "=", with: "")
}
