import Foundation

/// A deterministic, development-only build launch request supplied by the
/// native screenshot and live-proof harness. Parsing lives outside SwiftUI so
/// launch intent cannot be consumed by sheet presentation or view task races.
struct ScreenshotBuildRequest: Equatable, Sendable {
  let intent: String
  let repoID: String?
  let fleetWidth: Int?
  let proofCommands: [String]
  let delayNanoseconds: UInt64

  static func requested(in environment: [String: String]) -> ScreenshotBuildRequest? {
    guard environment["MDX_SHOT_AUTOSTART_BUILD"] == "1" else { return nil }
    let intent = (environment["MDX_SHOT_BUILD_INTENT"] ?? "")
      .trimmingCharacters(in: .whitespacesAndNewlines)
    guard !intent.isEmpty else { return nil }

    let repoID = nonEmpty(environment["MDX_SHOT_REPO_ID"])
    let fleetWidth = Int(environment["MDX_SHOT_FLEET_WIDTH"] ?? "")
      .map { min(max($0, 1), 24) }
    let proofCommands = proofCommands(
      from: (environment["MDX_SHOT_PROOF_COMMANDS"] ?? "")
        .replacingOccurrences(of: "\\n", with: "\n")
    )
    let rawDelay = UInt64(environment["MDX_SHOT_AUTOSTART_DELAY_MS"] ?? "") ?? 900
    let delayMilliseconds = min(max(rawDelay, 300), 20_000)

    return ScreenshotBuildRequest(
      intent: intent,
      repoID: repoID,
      fleetWidth: fleetWidth,
      proofCommands: proofCommands,
      delayNanoseconds: delayMilliseconds * 1_000_000
    )
  }

  static func proofCommands(from input: String) -> [String] {
    var seen = Set<String>()
    return input
      .split(whereSeparator: { $0.isNewline })
      .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
      .filter { !$0.isEmpty && seen.insert($0).inserted }
  }

  func resolvedRepoID(availableRepoIDs: Set<String>, fallback: String) -> String? {
    guard let repoID else { return fallback }
    return availableRepoIDs.contains(repoID) ? repoID : nil
  }

  private static func nonEmpty(_ value: String?) -> String? {
    let trimmed = (value ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
  }
}
