import Foundation

/// Pure targeting for the MDX_SHOT_RUN screenshot hook.
///
/// Labels:
/// - `first`: first finished run with a branch, else the first projection run
/// - `active`: first running Forge run, else nil when none are live
/// - `latest`: newest projection run, matching the last projection entry
/// - any other value: exact run id match
enum ScreenshotRunTarget {
  static let labels = ["first", "active", "latest"]

  static func resolve(target: String, runs: [ForgeRun]) -> ForgeRun? {
    guard !runs.isEmpty else { return nil }
    switch target {
    case "first":
      return runs.first { !$0.isRunning && $0.hasBranch } ?? runs.first
    case "active":
      return runs.first { $0.isRunning }
    case "latest":
      return runs.last
    default:
      return runs.first { $0.id == target }
    }
  }
}
