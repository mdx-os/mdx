import Combine
import Foundation
import MDxOperatorShared

@MainActor
final class HostRelayConnector: ObservableObject {
  enum State: Equatable {
    case disconnected
    case connecting
    case connected
    case waitingToReconnect(attempt: Int)
    case refused(String)
  }

  @Published private(set) var state: State = .disconnected
  private var task: URLSessionWebSocketTask?
  private var configuration: HostConnectorConfiguration?
  private let credentialStore: SecureCredentialStoring

  init(credentialStore: SecureCredentialStoring = KeychainCredentialStore()) {
    self.credentialStore = credentialStore
  }

  func connect(using configuration: HostConnectorConfiguration) async {
    disconnect()
    self.configuration = configuration
    guard !configuration.acceptsInboundConnections else {
      state = .refused("Inbound host connections are forbidden")
      return
    }
    guard let credential = try? credentialStore.load(account: configuration.credentialRef),
      let bearer = String(data: credential, encoding: .utf8), !bearer.isEmpty
    else {
      state = .refused("Relay credential is unavailable")
      return
    }

    state = .connecting
    var request = URLRequest(url: configuration.relayURL)
    request.setValue("Bearer \(bearer)", forHTTPHeaderField: "Authorization")
    request.setValue(configuration.hostID, forHTTPHeaderField: "X-MDx-Host-ID")
    let task = URLSession.shared.webSocketTask(with: request)
    self.task = task
    task.resume()
    state = .connected
  }

  func disconnect() {
    task?.cancel(with: .goingAway, reason: nil)
    task = nil
    configuration = nil
    state = .disconnected
  }
}
