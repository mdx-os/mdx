import AppKit
import Combine
import Foundation
import MDxOperatorShared

@MainActor
final class HostPairingStore: ObservableObject {
  enum State: Equatable {
    case loading
    case ready
    case code(String, expiresAt: Date)
    case paired(String)
    case failed(String)
  }

  @Published private(set) var state: State = .loading
  @Published private(set) var availableDevices: [MobileDeviceDescriptor] = []
  @Published var selectedDeviceID: String?

  private let identityManager = MobileDeviceIdentityManager(namespace: "macos")
  private let client: MobilePairingClient
  private var hostID: String?
  private var watchTask: Task<Void, Never>?

  init(baseURL: URL, accessTokenProvider: MobileAccessTokenProvider? = nil) {
    client = MobilePairingClient(
      baseURL: baseURL,
      accessTokenProvider: accessTokenProvider
    )
  }

  deinit {
    watchTask?.cancel()
  }

  func prepare() async {
    state = .loading
    do {
      let identity = try identityManager.loadOrCreate()
      let host = try await client.registerHost(
        identity: identity,
        displayName: Host.current().localizedName ?? "This Mac"
      )
      hostID = host.hostID
      let projection = try await client.projection()
      availableDevices = projection.devices.filter {
        $0.platform == "ios" && $0.status == "active"
      }
      if selectedDeviceID == nil { selectedDeviceID = availableDevices.first?.deviceID }
      if let pairing = projection.pairings.first(where: {
        $0.hostID == host.hostID && $0.pairingStatus == "active"
      }) {
        state = .paired(pairing.deviceID)
      } else {
        state = .ready
      }
    } catch {
      state = .failed(error.localizedDescription)
    }
  }

  func makePairingCode() async {
    guard let deviceID = selectedDeviceID, let hostID else {
      state = .failed("Sign in to MDx on your iPhone first, then refresh this list.")
      return
    }
    do {
      let challenge = try await client.mintChallenge(deviceID: deviceID, hostID: hostID)
      let payload = try MobilePairingPayload.encode(challenge)
      state = .code(
        payload,
        expiresAt: Date(timeIntervalSince1970: TimeInterval(challenge.expiresAtEpoch))
      )
      watchForRedemption(deviceID: deviceID, hostID: hostID)
    } catch {
      state = .failed(error.localizedDescription)
    }
  }

  func rotateHostKey() async {
    do {
      _ = try identityManager.rotateKey()
      await prepare()
    } catch {
      state = .failed(error.localizedDescription)
    }
  }

  private func watchForRedemption(deviceID: String, hostID: String) {
    watchTask?.cancel()
    watchTask = Task { [weak self] in
      for _ in 0..<30 {
        guard !Task.isCancelled else { return }
        try? await Task.sleep(for: .seconds(2))
        guard let self else { return }
        if let projection = try? await self.client.projection(),
          projection.pairings.contains(where: {
            $0.deviceID == deviceID && $0.hostID == hostID && $0.pairingStatus == "active"
          })
        {
          self.state = .paired(deviceID)
          return
        }
      }
      if case .code = self?.state {
        self?.state = .ready
      }
    }
  }
}
