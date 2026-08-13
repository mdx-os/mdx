import Combine
import Foundation
import MDxOperatorShared
import UIKit

@MainActor
final class MobileTrustStore: ObservableObject {
  enum State: Equatable {
    case loading
    case ready
    case pairing
    case paired(String)
    case failed(String)
  }

  @Published private(set) var state: State = .loading
  @Published private(set) var projection: MobileTrustProjection?
  @Published private(set) var thisDeviceID: String?
  @Published private(set) var attestationPosture = "checking"

  private let identityManager = MobileDeviceIdentityManager(namespace: "ios")
  private let attest = AppAttestRiskProvider()
  private let client: MobilePairingClient

  init(
    baseURL: URL = MobileTrustStore.defaultBaseURL,
    accessTokenProvider: MobileAccessTokenProvider? = nil
  ) {
    client = MobilePairingClient(
      baseURL: baseURL,
      accessTokenProvider: accessTokenProvider
    )
  }

  func prepare() async {
    state = .loading
    do {
      let identity = try identityManager.loadOrCreate()
      thisDeviceID = identity.deviceID
      let device = try await client.registerDevice(
        identity: identity,
        displayName: UIDevice.current.name
      )
      if device.attestationRisk == "verified_app_attest" {
        attestationPosture = device.attestationRisk
      } else {
        let signal = await attest.attest(deviceID: identity.deviceID, client: client)
        attestationPosture = signal.posture
      }
      try await refresh()
    } catch {
      state = .failed(error.localizedDescription)
    }
  }

  func refresh() async throws {
    projection = try await client.projection()
    state = .ready
  }

  func redeem(payload: String) async {
    state = .pairing
    do {
      let challenge = try MobilePairingPayload.decode(payload)
      guard challenge.deviceID == thisDeviceID else {
        throw MobilePairingClientError.serverRefused(
          "mobile_pairing_wrong_device",
          "This code was issued for a different iPhone. Ask the Mac for a new code."
        )
      }
      let deviceSignature = try identityManager.signPairingChallenge(challenge.challengeToken)
      let receipt = try await client.redeem(challenge, deviceSignature: deviceSignature)
      state = .paired(receipt.hostID)
      projection = try await client.projection()
    } catch {
      state = .failed(error.localizedDescription)
    }
  }

  func revokeDevice(_ deviceID: String) async {
    do {
      _ = try await client.revokeDevice(deviceID)
      try await refresh()
    } catch {
      state = .failed(error.localizedDescription)
    }
  }

  func revokeHost(_ hostID: String) async {
    do {
      _ = try await client.revokeHost(hostID)
      try await refresh()
    } catch {
      state = .failed(error.localizedDescription)
    }
  }

  private static var defaultBaseURL: URL {
    if let value = ProcessInfo.processInfo.environment["MDX_API_URL"],
      let url = URL(string: value)
    {
      return url
    }
    return MobileCloudConfiguration.current?.hostedAPIURL
      ?? URL(string: "http://127.0.0.1:18890")!
  }
}
