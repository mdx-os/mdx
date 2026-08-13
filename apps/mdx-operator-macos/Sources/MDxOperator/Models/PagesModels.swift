import Foundation

struct Page: Identifiable, Equatable {
  let id: String
  let title: String
  let bodyRef: String
  let author: String
  let visibility: String
  let revisionID: String
  let sourceReceiptIDs: [String]
  let charterEvidenceIDs: [String]
  // Folded in from the lifecycle projection.
  var state: String
  var owner: String
  var reviewer: String
  var trustedForAI: Bool
  var charterBacked: Bool
  var evidenceCount: Int
  var stale: Bool
  var reviewIntervalDays: Int
  var events: [PageLifecycleEvent]
  // Present only when this page came back through search.
  var summary: String = ""
  var category: String = ""

  var receiptCount: Int { sourceReceiptIDs.count }

  /// Human-facing author label: drop the actor namespace prefix.
  var authorLabel: String { Self.humanActor(author) }
  var ownerLabel: String { Self.humanActor(owner.isEmpty ? author : owner) }

  var visibilityLabel: String {
    switch visibility {
    case "tenant_only", "tenant_review": return "Team"
    case "public": return "Public"
    case "private", "owner_only": return "Private"
    default: return visibility.replacingOccurrences(of: "_", with: " ").capitalized
    }
  }

  static func humanActor(_ actor: String) -> String {
    guard let colon = actor.firstIndex(of: ":") else { return actor }
    let name = actor[actor.index(after: colon)...]
    return name.replacingOccurrences(of: "_", with: " ")
  }
}

struct PageLifecycleEvent: Identifiable, Equatable {
  let id: String
  let label: String
  let state: String
  let actorID: String
  let note: String
  let receiptID: String
}

struct PageBody: Equatable {
  let documentID: String
  let body: String
  let contentType: String
}

struct PageDraftRecord: Identifiable, Equatable {
  let id: String
  let documentID: String
  let title: String
  let bodyRef: String
  let receiptID: String
  let revisionID: String
}

struct PageReviewRequest: Identifiable, Equatable {
  let id: String
  let receiptID: String
  let documentID: String
  let draftID: String
  let sourceDraftReceiptID: String
  let decisionState: String
  let decisionReceiptID: String
  let decisionOutcome: String

  var isPending: Bool { decisionState == "PENDING_DECISION" }
  var isApproved: Bool { decisionOutcome == "approved" }
  var isRejected: Bool { decisionOutcome == "rejected" }
}

struct PagePublishDraft: Equatable, Codable {
  var documentID: String = ""
  var title: String = ""
  var body: String = ""
  var pageType: String = "knowledge"
  var visibility: String = "tenant_only"
  var isEdit: Bool = false
  var draftID: String = ""
  var draftReceiptID: String = ""

  static let pageTypes = ["knowledge", "spec", "decision", "standard", "signal", "changelog"]
  static let visibilities = ["tenant_only", "private", "public"]

  var isValid: Bool {
    !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty &&
    !body.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
  }

  /// A kernel-safe document id (lowercase letters, digits, `_`, `-`) from the title.
  func resolvedDocumentID() -> String {
    if isEdit, !documentID.isEmpty { return documentID }
    let slug = title.lowercased().map { char -> Character in
      (char.isLetter || char.isNumber) ? char : "_"
    }
    let cleaned = String(slug).prefix(48)
    let trimmed = cleaned.trimmingCharacters(in: CharacterSet(charactersIn: "_"))
    return "page_\(trimmed.isEmpty ? "note" : trimmed)"
  }

  static func typeLabel(_ type: String) -> String {
    type.replacingOccurrences(of: "_", with: " ").capitalized
  }

  static func visibilityLabel(_ visibility: String) -> String {
    switch visibility {
    case "tenant_only": return "Team"
    case "public": return "Public"
    case "private": return "Private"
    default: return visibility.replacingOccurrences(of: "_", with: " ").capitalized
    }
  }
}

enum PageState {
  static func label(_ state: String) -> String {
    switch state {
    case "published": return "Published"
    case "approved": return "Approved"
    case "in_review": return "In review"
    case "draft": return "Draft"
    default: return state.isEmpty ? "Unknown" : state.replacingOccurrences(of: "_", with: " ").capitalized
    }
  }
}
