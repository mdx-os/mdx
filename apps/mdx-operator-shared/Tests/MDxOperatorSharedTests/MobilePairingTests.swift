import Foundation
import XCTest

@testable import MDxOperatorShared

final class MobilePairingTests: XCTestCase {
  func testCloudClientsAttachBearerAdmissionWithoutPersistingTheToken() async {
    let baseURL = URL(string: "https://mdx.example/api/kernel/")!
    let tokenProvider: MobileAccessTokenProvider = { "cohort-token" }
    let forge = MobileForgeClient(baseURL: baseURL, accessTokenProvider: tokenProvider)
    let pairing = MobilePairingClient(baseURL: baseURL, accessTokenProvider: tokenProvider)

    let forgeRequest = await forge.authorizedRequest(
      url: baseURL.appending(path: "mobile/sessions.json")
    )
    let pairingRequest = await pairing.authorizedRequest(
      url: baseURL.appending(path: "mobile/trust/projection.json")
    )

    XCTAssertEqual(
      forgeRequest.value(forHTTPHeaderField: "Authorization"),
      "Bearer cohort-token"
    )
    XCTAssertEqual(
      pairingRequest.value(forHTTPHeaderField: "Authorization"),
      "Bearer cohort-token"
    )
  }

  func testCloudClientsDoNotSendAnEmptyBearerHeader() async {
    let baseURL = URL(string: "https://mdx.example/api/kernel/")!
    let tokenProvider: MobileAccessTokenProvider = { "" }
    let forge = MobileForgeClient(baseURL: baseURL, accessTokenProvider: tokenProvider)
    let pairing = MobilePairingClient(baseURL: baseURL, accessTokenProvider: tokenProvider)

    let forgeRequest = await forge.authorizedRequest(url: baseURL)
    let pairingRequest = await pairing.authorizedRequest(url: baseURL)

    XCTAssertNil(forgeRequest.value(forHTTPHeaderField: "Authorization"))
    XCTAssertNil(pairingRequest.value(forHTTPHeaderField: "Authorization"))
  }

  func testPairingPayloadAllowsClipboardWhitespaceWithoutChangingChallenge() throws {
    let data = Data(
      #"{"challenge_id":"pairing_1","tenant_id":"tenant","user_id":"user","device_id":"iphone","host_id":"mac","host_public_key_thumbprint":"host-key","challenge_token":"nonce","signature":"server-signature","server_public_key":"server-key","issued_at_epoch":1,"expires_at_epoch":4102444800,"single_use":true}"#.utf8
    )
    let challenge = try JSONDecoder().decode(MobilePairingChallenge.self, from: data)
    let payload = try MobilePairingPayload.encode(challenge)

    XCTAssertEqual(
      try MobilePairingPayload.decode("  \n\(payload)\t "),
      challenge
    )
  }

  func testPairingPayloadRejectsEmptyClipboardText() {
    XCTAssertThrowsError(try MobilePairingPayload.decode(" \n\t ")) { error in
      XCTAssertEqual(error as? MobilePairingClientError, .invalidPairingPayload)
    }
  }

  func testTrustProjectionCarriesVerifiedCommandIdentity() throws {
    let data = Data(
      #"{"tenant_id":"local_tenant","user_id":"local_user","devices":[],"hosts":[],"pairings":[],"host_connector_direction":"outbound_only","inbound_host_listener_allowed":false}"#.utf8
    )

    let projection = try JSONDecoder().decode(MobileTrustProjection.self, from: data)

    XCTAssertEqual(projection.tenantID, "local_tenant")
    XCTAssertEqual(projection.userID, "local_user")
  }

  func testAppAttestChallengeRequiresServerVerificationAndSingleUse() throws {
    let data = Data(
      #"{"status":"APP_ATTEST_CHALLENGE_MINTED","challenge_id":"app_attest_1","device_id":"iphone","challenge":"nonce","environment":"development","issued_at_epoch":1,"expires_at_epoch":4102444800,"single_use":true,"server_verification_required":true}"#.utf8
    )
    let challenge = try JSONDecoder().decode(MobileAppAttestChallenge.self, from: data)
    XCTAssertEqual(challenge.deviceID, "iphone")
    XCTAssertEqual(challenge.environment, "development")
    XCTAssertTrue(challenge.singleUse)
    XCTAssertTrue(challenge.serverVerificationRequired)
    XCTAssertFalse(challenge.isExpired)
  }

  func testAppAttestReceiptCarriesDurableServerProvenance() throws {
    let data = Data(
      #"{"status":"APP_ATTEST_VERIFIED","device_id":"iphone","attestation_risk":"verified_app_attest","attestation_verification":"apple_app_attest_server_cryptographic","environment":"development","validation_category":3,"bundle_version":"1","receipt_id":"receipt_1","state_source":"kernel_receipt_projection","restart_durable":true,"raw_attestation_recorded":false,"raw_apple_receipt_recorded":false,"idempotent_replay":false,"authority_opened":"none"}"#.utf8
    )
    let receipt = try JSONDecoder().decode(MobileAppAttestationReceipt.self, from: data)
    XCTAssertEqual(receipt.attestationRisk, "verified_app_attest")
    XCTAssertEqual(receipt.attestationVerification, "apple_app_attest_server_cryptographic")
    XCTAssertEqual(receipt.validationCategory, 3)
    XCTAssertEqual(receipt.stateSource, "kernel_receipt_projection")
    XCTAssertTrue(receipt.restartDurable)
    XCTAssertFalse(receipt.rawAttestationRecorded)
    XCTAssertFalse(receipt.rawAppleReceiptRecorded)
    XCTAssertEqual(receipt.authorityOpened, "none")
  }
}
