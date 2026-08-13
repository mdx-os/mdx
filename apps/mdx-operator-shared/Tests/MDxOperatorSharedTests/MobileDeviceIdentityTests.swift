import CryptoKit
import Foundation
import MDxOperatorShared
import Security
import XCTest

final class MobileDeviceIdentityTests: XCTestCase {
  func testPairingSignatureProvesPossessionOfPublishedKey() throws {
    let store = MemoryCredentialStore()
    let manager = MobileDeviceIdentityManager(store: store, namespace: "possession-test")
    let identity = try manager.loadOrCreate(now: Date(timeIntervalSince1970: 10))
    let challengeToken = "server-issued-single-use-challenge"

    let publicKey = try P256.Signing.PublicKey(
      x963Representation: try XCTUnwrap(Data(base64URL: identity.publicKey)))
    let signature = try P256.Signing.ECDSASignature(
      derRepresentation: try XCTUnwrap(
        Data(base64URL: manager.signPairingChallenge(challengeToken))))

    XCTAssertTrue(publicKey.isValidSignature(signature, for: Data(challengeToken.utf8)))
  }

  func testRotationKeepsDeviceIDAndChangesPossessionKey() throws {
    let store = MemoryCredentialStore()
    let manager = MobileDeviceIdentityManager(store: store, namespace: "rotation-test")
    let original = try manager.loadOrCreate(now: Date(timeIntervalSince1970: 10))
    let rotated = try manager.rotateKey(now: Date(timeIntervalSince1970: 20))

    XCTAssertEqual(rotated.deviceID, original.deviceID)
    XCTAssertNotEqual(rotated.publicKey, original.publicKey)
    XCTAssertNotEqual(rotated.publicKeyThumbprint, original.publicKeyThumbprint)
  }

  func testCommandAttestationCoversCanonicalCommandAndUserPresence() throws {
    let store = MemoryCredentialStore()
    let manager = MobileDeviceIdentityManager(store: store, namespace: "command-test")
    let identity = try manager.loadOrCreate(now: Date(timeIntervalSince1970: 10))
    var command = fixtureCommand(instruction: "Run tests / safely")
    command.deviceSignature = try manager.signCommandAttestation(command)

    let expected =
      #"{"actor_device_id":"iphone_fixture","actor_user_id":"local_user","command_id":"mobile_command_fixture","delegation_envelope_ref":"session_default","expected_session_version":1,"expires_at":"2023-11-14T22:15:20Z","grants_execution_authority":false,"idempotency_key":"mobile_idempotency_fixture","issued_at":"2023-11-14T22:13:20Z","kind":"follow_up","payload":{"instruction":"Run tests / safely"},"schema_version":1,"session_id":"forge_run_fixture","target":{"capability_revision":1,"display_name":"Fixture Mac","kind":"paired_host","target_id":"mac_fixture","tenant_id":"local_tenant"},"tenant_id":"local_tenant","user_presence":"biometric_verified"}"#
    XCTAssertEqual(String(decoding: try command.attestationPayload(), as: UTF8.self), expected)

    let publicKey = try P256.Signing.PublicKey(
      x963Representation: try XCTUnwrap(Data(base64URL: identity.publicKey)))
    let signature = try P256.Signing.ECDSASignature(
      derRepresentation: try XCTUnwrap(Data(base64URL: command.deviceSignature)))
    XCTAssertTrue(publicKey.isValidSignature(signature, for: try command.attestationPayload()))
    XCTAssertFalse(
      publicKey.isValidSignature(
        signature,
        for: try fixtureCommand(instruction: "Tampered after signing").attestationPayload()
      ))
  }

  func testAccountDeletionAttestationBindsDeviceAndConfirmation() throws {
    let store = MemoryCredentialStore()
    let manager = MobileDeviceIdentityManager(store: store, namespace: "deletion-test")
    let identity = try manager.loadOrCreate(now: Date(timeIntervalSince1970: 10))
    var request = MobileAccountDeletionRequest(
      requestID: "mobile_account_deletion_fixture",
      actorDeviceID: "iphone_deletion_attestation",
      issuedAt: Date(timeIntervalSince1970: 1_700_000_000),
      expiresAt: Date(timeIntervalSince1970: 1_700_000_120)
    )
    request.deviceSignature = try manager.signAccountDeletionAttestation(request)

    let expected =
      #"{"actor_device_id":"iphone_deletion_attestation","confirm":true,"expires_at":"2023-11-14T22:15:20Z","issued_at":"2023-11-14T22:13:20Z","request_id":"mobile_account_deletion_fixture","user_presence":"biometric_verified"}"#
    XCTAssertEqual(String(decoding: try request.attestationPayload(), as: UTF8.self), expected)

    let publicKey = try P256.Signing.PublicKey(
      x963Representation: try XCTUnwrap(Data(base64URL: identity.publicKey)))
    let signature = try P256.Signing.ECDSASignature(
      derRepresentation: try XCTUnwrap(Data(base64URL: request.deviceSignature)))
    XCTAssertTrue(publicKey.isValidSignature(signature, for: try request.attestationPayload()))
    XCTAssertFalse(
      publicKey.isValidSignature(
        signature,
        for: try MobileAccountDeletionRequest(
          requestID: "mobile_account_deletion_fixture",
          actorDeviceID: "iphone_other_device",
          issuedAt: Date(timeIntervalSince1970: 1_700_000_000),
          expiresAt: Date(timeIntervalSince1970: 1_700_000_120)
        ).attestationPayload()
      ))
  }

  private func fixtureCommand(instruction: String) -> MobileSessionCommand {
    MobileSessionCommand(
      commandID: "mobile_command_fixture",
      idempotencyKey: "mobile_idempotency_fixture",
      sessionID: "forge_run_fixture",
      expectedSessionVersion: 1,
      tenantID: "local_tenant",
      actorUserID: "local_user",
      actorDeviceID: "iphone_fixture",
      target: ExecutionTarget(
        kind: .pairedHost,
        targetID: "mac_fixture",
        tenantID: "local_tenant",
        displayName: "Fixture Mac",
        capabilityRevision: 1
      ),
      kind: "follow_up",
      payload: MobileCommandPayload(instruction: instruction),
      issuedAt: Date(timeIntervalSince1970: 1_700_000_000),
      expiresAt: Date(timeIntervalSince1970: 1_700_000_120),
      userPresence: "biometric_verified"
    )
  }

  func testPushRegistrationAttestationBindsTokenAndDestination() throws {
    let store = MemoryCredentialStore()
    let manager = MobileDeviceIdentityManager(store: store, namespace: "push-test")
    let identity = try manager.loadOrCreate(now: Date(timeIntervalSince1970: 10))
    var request = MobilePushRegistrationRequest(
      deviceID: identity.deviceID,
      hostID: "mac_push_fixture",
      pushToken: String(repeating: "a", count: 64),
      kind: "apns",
      environment: "sandbox",
      issuedAt: Date(timeIntervalSince1970: 1_700_000_000),
      expiresAt: Date(timeIntervalSince1970: 1_700_000_120)
    )
    request.deviceSignature = try manager.signPushRegistrationAttestation(request)

    let expected =
      #"{"device_id":"\#(identity.deviceID)","environment":"sandbox","expires_at":"2023-11-14T22:15:20Z","host_id":"mac_push_fixture","issued_at":"2023-11-14T22:13:20Z","kind":"apns","push_token":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","schema_version":1}"#
    XCTAssertEqual(String(decoding: try request.attestationPayload(), as: UTF8.self), expected)

    let publicKey = try P256.Signing.PublicKey(
      x963Representation: try XCTUnwrap(Data(base64URL: identity.publicKey)))
    let signature = try P256.Signing.ECDSASignature(
      derRepresentation: try XCTUnwrap(Data(base64URL: request.deviceSignature)))
    XCTAssertTrue(publicKey.isValidSignature(signature, for: try request.attestationPayload()))
    XCTAssertFalse(
      publicKey.isValidSignature(
        signature,
        for: try MobilePushRegistrationRequest(
          deviceID: identity.deviceID,
          hostID: "mdx_cloud",
          pushToken: String(repeating: "b", count: 64),
          kind: "apns",
          environment: "sandbox",
          issuedAt: Date(timeIntervalSince1970: 1_700_000_000),
          expiresAt: Date(timeIntervalSince1970: 1_700_000_120)
        ).attestationPayload()
      ))
  }
}

private final class MemoryCredentialStore: SecureCredentialStoring, @unchecked Sendable {
  private let lock = NSLock()
  private var values: [String: Data] = [:]

  func save(_ data: Data, account: String, accessibility: CFString) throws {
    lock.withLock { values[account] = data }
  }

  func load(account: String) throws -> Data? {
    lock.withLock { values[account] }
  }

  func delete(account: String) throws {
    _ = lock.withLock { values.removeValue(forKey: account) }
  }
}

extension Data {
  fileprivate init?(base64URL value: String) {
    var base64 =
      value
      .replacingOccurrences(of: "-", with: "+")
      .replacingOccurrences(of: "_", with: "/")
    base64.append(String(repeating: "=", count: (4 - base64.count % 4) % 4))
    self.init(base64Encoded: base64)
  }
}
