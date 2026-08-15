import Foundation

struct MacDistributionProfile: Equatable, Sendable {
  let channel: String

  init(channel: String?) {
    self.channel = channel?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() ?? ""
  }

  var isCanary: Bool { channel == "canary" }

  var updateDetail: String {
    if isCanary {
      return "MDx checks the private canary channel and can install a newer signed build in place."
    }
    return "Development builds are updated from the repository."
  }

  var packagingSubtitle: String {
    if isCanary {
      return "This downloadable canary is signed, notarized, and checked before publication."
    }
    return "Local bundle proof now, signing and notarization when distribution credentials are ready."
  }
}

extension Bundle {
  var appShortVersion: String {
    infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.0.0"
  }

  var appBuildVersion: String {
    infoDictionary?["CFBundleVersion"] as? String ?? "0"
  }

  var macDistributionProfile: MacDistributionProfile {
    MacDistributionProfile(channel: infoDictionary?["MDXDistributionChannel"] as? String)
  }
}
