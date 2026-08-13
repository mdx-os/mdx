import Foundation

/// Pure, testable formatting for the app-managed connection lifecycle and the
/// two context affordances that ride on it: the honest diagnosis shown when the
/// local kernel is unreachable or the app is running degraded, the repo/branch
/// anchor line, and whether onboarding can end in a real governed build.
///
/// Kept as plain functions so the view layer stays thin and the "never a
/// cosmetic all-good" rule is pinned by mapper checks instead of a screenshot.
enum AppLifecycleDiagnosis {
  /// A plain-language diagnosis built from the real failure signals. Offline and
  /// degraded name the base URL, the streak of failed checks, a recent hang, and
  /// the actual last error; healthy states the truth without inventing trouble.
  static func summary(
    health: AppHealth,
    baseURL: String,
    consecutiveFailures: Int,
    recentHang: Bool,
    lastError: String?
  ) -> String {
    let cleanError = lastError?.trimmingCharacters(in: .whitespacesAndNewlines)
    switch health {
    case .healthy:
      return "Connected to \(baseURL). No recent hangs or errors."
    case .offline:
      var parts = ["Can't reach MDx at \(baseURL)."]
      if consecutiveFailures > 0 {
        parts.append("\(consecutiveFailures) \(consecutiveFailures == 1 ? "check" : "checks") failed in a row.")
      }
      if let cleanError, !cleanError.isEmpty {
        parts.append("Last error: \(cleanError).")
      }
      return parts.joined(separator: " ")
    case .degraded:
      var parts = ["Connected to \(baseURL), but something is off."]
      if recentHang { parts.append("A recent request hung.") }
      if let cleanError, !cleanError.isEmpty {
        parts.append("Last error: \(cleanError).")
      }
      if !recentHang, cleanError?.isEmpty ?? true {
        parts.append("Recent calls are failing.")
      }
      return parts.joined(separator: " ")
    }
  }
}

/// The repo/branch context anchor. The kernel projects no persistent base branch
/// per repo (Forge cuts a fresh branch for each run), so the branch line shows
/// the latest run's branch when there is one and otherwise states the true
/// behavior instead of inventing a "main".
enum RepoContext {
  static func branchLine(latestRunBranch: String?) -> String {
    let trimmed = latestRunBranch?.trimmingCharacters(in: .whitespacesAndNewlines)
    if let trimmed, !trimmed.isEmpty {
      return trimmed
    }
    return "New branch per run"
  }

  /// Whether the branch line is a real run branch (worth a git glyph) or the
  /// honest default-behavior note.
  static func hasRealBranch(latestRunBranch: String?) -> Bool {
    !(latestRunBranch?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true)
  }
}

/// Whether onboarding can end in a real governed outcome. It can only start a
/// first build when MDx is connected, a model is live to do the work, and at
/// least one repo is on the record; otherwise the flow lands honestly on
/// read-only orientation instead of faking a win.
enum FirstWin {
  static func canStartRealBuild(connected: Bool, modelConnected: Bool, repoCount: Int) -> Bool {
    connected && modelConnected && repoCount > 0
  }

  /// The one-line reason the real-win button is unavailable, so the final step
  /// can tell the operator what unlocks it rather than hiding the gap.
  static func blockedReason(connected: Bool, modelConnected: Bool, repoCount: Int) -> String? {
    if !connected { return "Connect MDx to start your first build here." }
    if !modelConnected { return "Connect a model to let MDx do real work." }
    if repoCount == 0 { return "Add a repo and MDx can build in it." }
    return nil
  }
}
