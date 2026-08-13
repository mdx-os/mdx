import Foundation

public enum MobileFormatters {
  public static func shortRevision(_ revision: String) -> String {
    if revision.isEmpty || revision == "pending" || revision == "working-copy" {
      return "Local branch"
    }
    return String(revision.prefix(8))
  }

  public static func relativeUpdate(_ date: Date, now: Date = Date()) -> String {
    let seconds = max(0, Int(now.timeIntervalSince(date)))
    if seconds < 60 { return "Updated now" }
    if seconds < 3_600 { return "Updated \(seconds / 60)m ago" }
    return "Updated \(seconds / 3_600)h ago"
  }
}
