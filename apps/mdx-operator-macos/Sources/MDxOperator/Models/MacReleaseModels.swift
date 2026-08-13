import Foundation

struct MacReleaseManifest: Equatable, Sendable {
  let version: String
  let build: String
  let sha256: String
  let sizeBytes: Int
  let notarizedAt: String

  func isNewer(thanVersion installedVersion: String, build installedBuild: String) -> Bool {
    let available = version.split(separator: ".").map { Int($0) ?? 0 }
    let installed = installedVersion.split(separator: ".").map { Int($0) ?? 0 }
    for index in 0..<max(available.count, installed.count) {
      let left = index < available.count ? available[index] : 0
      let right = index < installed.count ? installed[index] : 0
      if left != right { return left > right }
    }
    return (Int(build) ?? 0) > (Int(installedBuild) ?? 0)
  }
}

struct MacDistributionProfile: Equatable, Sendable {
  let channel: String

  init(channel: String?) {
    self.channel = channel?.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() ?? ""
  }

  var isCanary: Bool { channel == "canary" }

  var updateDetail: String {
    if isCanary {
      return "MDx checks the private canary release and sends you to the secure download when a newer build is ready."
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
