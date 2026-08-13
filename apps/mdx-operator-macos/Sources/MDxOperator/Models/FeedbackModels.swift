import Foundation

/// The kernel's bounded feedback categories (free-form categories are refused
/// so triage stays sortable). Labels are the human face; ids are the wire.
struct FeedbackCategory: Identifiable, Hashable {
  let id: String
  let label: String
  let icon: String

  static let all: [FeedbackCategory] = [
    FeedbackCategory(id: "bug", label: "Something broke", icon: "ladybug"),
    FeedbackCategory(id: "blocked", label: "I'm blocked", icon: "hand.raised"),
    FeedbackCategory(id: "confusing", label: "This is confusing", icon: "questionmark.circle"),
    FeedbackCategory(id: "idea", label: "I have an idea", icon: "lightbulb"),
    FeedbackCategory(id: "other", label: "Something else", icon: "ellipsis.circle"),
  ]
}

/// A report this Mac has filed, persisted locally so the engineer can always
/// see what they sent and hold the receipt. (Kernel-side status tracking is a
/// pending kernel ask; until then the honest status is "recorded".)
struct FiledReport: Identifiable, Codable, Equatable {
  var id: String { receiptID }
  let receiptID: String
  let category: String
  let note: String
  let surface: String
  let filedAt: Date
  var appVersion: String = ""

  var categoryLabel: String {
    FeedbackCategory.all.first { $0.id == category }?.label ?? category
  }
}

/// Maps the app's routes onto the kernel's accepted feedback surfaces
/// (home and native_macos accepted as of the round-3 kernel slice).
enum FeedbackSurfaceMap {
  static func surface(for route: AppRoute) -> String {
    switch route {
    case .twin: return "twin"
    case .pages: return "pages"
    case .message: return "message"
    case .memory: return "native_macos"
    case .marketplace: return "marketplace"
    case .you: return "native_macos"
    case .home: return "home"
    case .forge: return "forge"
    }
  }
}

/// A report's kernel-side lifecycle from the captures projection.
struct RemoteReportStatus: Equatable {
  let receiptID: String
  let status: String      // received | triaged | working | resolved
  let statusNote: String

  var label: String {
    switch status {
    case "received": return "Received"
    case "triaged": return "Triaged"
    case "working": return "Being worked on"
    case "resolved": return "Resolved"
    default: return status.isEmpty ? "Recorded" : status
    }
  }

  var isResolved: Bool { status == "resolved" }
  var isActive: Bool { status == "working" || status == "triaged" }
}
