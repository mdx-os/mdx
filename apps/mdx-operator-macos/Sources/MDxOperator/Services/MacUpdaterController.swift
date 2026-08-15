import Foundation
import Observation
import Sparkle

struct MacUpdateAvailability: Equatable, Sendable {
  let version: String
  let build: String
}

@MainActor
@Observable
final class MacUpdaterController: NSObject, SPUUpdaterDelegate {
  typealias AccessTokenProvider = @MainActor () async -> String?

  private(set) var availableUpdate: MacUpdateAvailability?
  private(set) var checkInFlight = false

  private let accessTokenProvider: AccessTokenProvider
  @ObservationIgnored private var standardController: SPUStandardUpdaterController?
  @ObservationIgnored private var hasStarted = false

  init(accessTokenProvider: @escaping AccessTokenProvider) {
    self.accessTokenProvider = accessTokenProvider
    super.init()
    guard Self.isConfigured else { return }
    standardController = SPUStandardUpdaterController(
      startingUpdater: false,
      updaterDelegate: self,
      userDriverDelegate: nil
    )
  }

  var isConfigured: Bool { standardController != nil }

  func checkInBackground() async {
    await check(showingUI: false)
  }

  func checkForUpdates() async {
    await check(showingUI: true)
  }

  func installAvailableUpdate() async {
    await check(showingUI: true)
  }

  func dismissAvailableUpdate() {
    availableUpdate = nil
  }

  private func check(showingUI: Bool) async {
    guard let standardController, !checkInFlight else { return }
    checkInFlight = true
    guard let token = await accessTokenProvider(), !token.isEmpty else {
      checkInFlight = false
      return
    }

    standardController.updater.httpHeaders = ["Authorization": "Bearer \(token)"]
    if !hasStarted {
      standardController.startUpdater()
      standardController.updater.clearFeedURLFromUserDefaults()
      hasStarted = true
    }
    guard standardController.updater.canCheckForUpdates else {
      checkInFlight = false
      return
    }

    if showingUI {
      standardController.checkForUpdates(nil)
    } else {
      standardController.updater.checkForUpdateInformation()
    }
  }

  func updater(_ updater: SPUUpdater, didFindValidUpdate item: SUAppcastItem) {
    availableUpdate = MacUpdateAvailability(
      version: item.displayVersionString,
      build: item.versionString
    )
  }

  func updaterDidNotFindUpdate(_ updater: SPUUpdater, error: Error) {
    availableUpdate = nil
  }

  func updater(
    _ updater: SPUUpdater,
    didFinishUpdateCycleFor updateCheck: SPUUpdateCheck,
    error: Error?
  ) {
    checkInFlight = false
  }

  private static var isConfigured: Bool {
    guard Bundle.main.macDistributionProfile.isCanary,
          let feed = Bundle.main.object(forInfoDictionaryKey: "SUFeedURL") as? String,
          let publicKey = Bundle.main.object(forInfoDictionaryKey: "SUPublicEDKey") as? String,
          let feedURL = URL(string: feed),
          feedURL.scheme == "https"
    else { return false }
    return !publicKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
  }
}
