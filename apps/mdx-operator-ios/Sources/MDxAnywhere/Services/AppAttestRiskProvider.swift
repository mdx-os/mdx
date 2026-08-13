import CryptoKit
import DeviceCheck
import Foundation
import MDxOperatorShared
import Security

struct AppAttestRiskSignal: Sendable {
  let posture: String
}

actor AppAttestRiskProvider {
  private let credentialStore: any SecureCredentialStoring

  init(credentialStore: any SecureCredentialStoring = KeychainCredentialStore()) {
    self.credentialStore = credentialStore
  }

  func attest(deviceID: String, client: MobilePairingClient) async -> AppAttestRiskSignal {
    let service = DCAppAttestService.shared
    guard service.isSupported else {
      return AppAttestRiskSignal(posture: "unsupported")
    }
    do {
      return try await enroll(
        deviceID: deviceID,
        client: client,
        service: service,
        mayReplaceKey: true
      )
    } catch let MobilePairingClientError.serverRefused(code, _) {
      if code == "mobile_app_attest_not_configured" {
        return AppAttestRiskSignal(posture: "evidence_pending_server_configuration")
      }
      return AppAttestRiskSignal(posture: "verification_refused")
    } catch {
      return AppAttestRiskSignal(posture: "unavailable")
    }
  }

  private func enroll(
    deviceID: String,
    client: MobilePairingClient,
    service: DCAppAttestService,
    mayReplaceKey: Bool
  ) async throws -> AppAttestRiskSignal {
    let account = keyAccount(deviceID: deviceID)
    let keyID = try await loadOrCreateKey(account: account, service: service)
    let challenge = try await client.mintAppAttestChallenge(deviceID: deviceID, keyID: keyID)
    guard
      challenge.status == "APP_ATTEST_CHALLENGE_MINTED",
      challenge.deviceID == deviceID,
      challenge.singleUse,
      challenge.serverVerificationRequired,
      !challenge.isExpired
    else {
      throw AppAttestRiskError.invalidServerChallenge
    }
    let challengeHash = Data(SHA256.hash(data: Data(challenge.challenge.utf8)))
    let attestationObject: Data
    do {
      attestationObject = try await attestWithBoundedRetry(
        service: service,
        keyID: keyID,
        clientDataHash: challengeHash
      )
    } catch {
      if mayReplaceKey, isInvalidKey(error) {
        try credentialStore.delete(account: account)
        return try await enroll(
          deviceID: deviceID,
          client: client,
          service: service,
          mayReplaceKey: false
        )
      }
      throw error
    }
    let receipt: MobileAppAttestationReceipt
    do {
      receipt = try await client.submitAppAttestation(
        challengeID: challenge.challengeID,
        deviceID: deviceID,
        keyID: keyID,
        attestationObject: attestationObject
      )
    } catch let MobilePairingClientError.serverRefused(code, _)
      where mayReplaceKey && code == "mobile_app_attest_key_identity_conflict"
    {
      try credentialStore.delete(account: account)
      return try await enroll(
        deviceID: deviceID,
        client: client,
        service: service,
        mayReplaceKey: false
      )
    }
    guard
      receipt.status == "APP_ATTEST_VERIFIED",
      receipt.deviceID == deviceID,
      receipt.attestationRisk == "verified_app_attest",
      receipt.attestationVerification == "apple_app_attest_server_cryptographic",
      receipt.stateSource == "kernel_receipt_projection",
      receipt.restartDurable,
      !receipt.rawAttestationRecorded,
      !receipt.rawAppleReceiptRecorded,
      receipt.authorityOpened == "none",
      !receipt.receiptID.isEmpty
    else {
      throw AppAttestRiskError.invalidServerReceipt
    }
    return AppAttestRiskSignal(posture: receipt.attestationRisk)
  }

  private func loadOrCreateKey(
    account: String,
    service: DCAppAttestService
  ) async throws -> String {
    if let data = try credentialStore.load(account: account),
      let keyID = String(data: data, encoding: .utf8),
      !keyID.isEmpty
    {
      return keyID
    }
    let keyID = try await service.generateKey()
    try credentialStore.save(
      Data(keyID.utf8),
      account: account,
      accessibility: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
    )
    return keyID
  }

  private func attestWithBoundedRetry(
    service: DCAppAttestService,
    keyID: String,
    clientDataHash: Data
  ) async throws -> Data {
    var attempt = 0
    while true {
      do {
        return try await service.attestKey(keyID, clientDataHash: clientDataHash)
      } catch {
        attempt += 1
        guard attempt < 3, isServerUnavailable(error) else { throw error }
        try await Task.sleep(for: .milliseconds(250 * attempt))
      }
    }
  }

  private func keyAccount(deviceID: String) -> String {
    "ios.app-attest-key-id.\(deviceID)"
  }

  private func isServerUnavailable(_ error: Error) -> Bool {
    let error = error as NSError
    return error.domain == DCErrorDomain
      && error.code == DCError.Code.serverUnavailable.rawValue
  }

  private func isInvalidKey(_ error: Error) -> Bool {
    let error = error as NSError
    return error.domain == DCErrorDomain && error.code == DCError.Code.invalidKey.rawValue
  }
}

enum AppAttestRiskError: Error {
  case invalidServerChallenge
  case invalidServerReceipt
}
