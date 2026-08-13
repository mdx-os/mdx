import SwiftUI

/// The lean, receipt-backed setup state the welcome flow reads from the kernel's
/// `/install/setup-router.json`. Native setup uses the same governed owner and
/// model routes as web setup, then refreshes this projection.
struct SetupRouterState: Equatable {
  var reachable: Bool
  var setupComplete: Bool
  var ownerClaimed: Bool
  var ownerName: String
  var modelConnected: Bool
  var modelID: String
  var trackChosen: Bool
  var chosenTrack: String

  static let unreachable = SetupRouterState(
    reachable: false,
    setupComplete: false,
    ownerClaimed: false,
    ownerName: "",
    modelConnected: false,
    modelID: "",
    trackChosen: false,
    chosenTrack: ""
  )
}

/// The one thing MDx cannot detect: where you want to start. Three honest doors,
/// each landing on a real native surface the moment the welcome closes.
enum WelcomePath: String, CaseIterable, Identifiable {
  case forge
  case twin
  case look

  var id: String { rawValue }

  var title: String {
    switch self {
    case .forge: "Watch Forge build something"
    case .twin: "Think with Twin"
    case .look: "Look around first"
    }
  }

  var blurb: String {
    switch self {
    case .forge: "Hand it a build or a fix and watch it work under your sign-off, from intent to a branch you review."
    case .twin: "Ask it anything. It reads your work, reasons over the tradeoffs, and helps you decide the next move."
    case .look: "No commitment. Open the shell and explore the surfaces at your own pace."
    }
  }

  var icon: String {
    switch self {
    case .forge: "hammer.fill"
    case .twin: "sparkles"
    case .look: "square.grid.2x2"
    }
  }

  var enterVerb: String {
    switch self {
    case .forge: "Open Forge"
    case .twin: "Open Twin"
    case .look: "Open MDx"
    }
  }

  var route: AppRoute {
    switch self {
    case .forge: .forge(.overview)
    case .twin: .twin
    case .look: .home
    }
  }
}
