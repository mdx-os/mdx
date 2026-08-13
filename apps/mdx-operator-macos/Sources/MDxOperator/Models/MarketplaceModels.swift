import Foundation

enum MarketplaceSection: String, CaseIterable, Identifiable {
  case forYou = "For You"
  case packs = "Packs"
  case installed = "Installed"
  case review = "Review"

  var id: String { rawValue }
  var symbol: String {
    switch self {
    case .forYou: return "sparkles"
    case .packs: return "shippingbox"
    case .installed: return "checkmark.circle"
    case .review: return "person.crop.circle.badge.checkmark"
    }
  }
}

struct MarketplacePackState: Equatable {
  let installed: Bool
  let enabled: Bool
  let scope: String
  let version: String
  let applicationTargets: [String]
  let configured: Bool
  let connected: Bool
  let authorized: Bool
  let applicable: Bool
  let executable: Bool
  let ready: Bool
  let lifecycleState: String
  let heldItems: [String]
  let rollbackVersion: String
  let useCount: Int
  let acceptedCount: Int
  let rejectedCount: Int
  let installReceiptID: String

  static let available = MarketplacePackState(
    installed: false, enabled: false, scope: "", version: "", applicationTargets: [],
    configured: false, connected: false, authorized: false, applicable: false,
    executable: false, ready: false, lifecycleState: "not_installed", heldItems: [],
    rollbackVersion: "", useCount: 0, acceptedCount: 0, rejectedCount: 0, installReceiptID: ""
  )
}

struct MarketplacePack: Identifiable, Equatable {
  let id: String
  let version: String
  let name: String
  let job: String
  let outcome: String
  let sourceLane: String
  let publisher: String
  let publisherVerified: Bool
  let supportedApps: [String]
  let items: [String]
  let requiredItems: [String]
  let optionalItems: [String]
  let applicationTargets: [String]
  let installScopes: [String]
  let dependencies: [String]
  let conflicts: [String]
  let setupSteps: [String]
  let requestedGrants: [String]
  let blockedGrants: [String]
  let trialExample: String
  let firstUseLabel: String
  let firstUsePath: String
  let improvement: String
  let updateVersion: String
  let updateChange: String
  let permissionDiff: String
  let state: MarketplacePackState

  var lifecycleLabel: String {
    switch state.lifecycleState {
    case "ready": return "Ready"
    case "held": return "Needs review"
    case "setup_required": return "Setup verification needed"
    case "update_available": return "Update available"
    case "disabled": return "Disabled"
    default: return "Available"
    }
  }
}

struct PackTrialPreview: Equatable {
  let packID: String
  let example: String
  let previewApps: [String]
  let requestedGrants: [String]
  let blockedGrants: [String]
  let receiptID: String
  let line: String
}

struct Capability: Identifiable, Equatable {
  let id: String
  let name: String
  let type: String
  let summary: String
  let helpsWith: String
  let accessLine: String
  let reviewReason: String
  let effectiveStatus: String
  let blockedActions: [String]
  let installTargets: [String]
  let requiredSecrets: [String]
  let lastScanned: String
  let riskLevel: String
  let riskFindings: [String]
  let sourceOwner: String
  let sourceKind: String
  let sourceVersion: String
  let artifactHash: String
  let hasTry: Bool
  var installed: Bool
  var installedScopes: [String]
  let usedInBuilds: Int

  var typeLabel: String { type.replacingOccurrences(of: "_", with: " ").capitalized }

  var statusLabel: String {
    switch effectiveStatus {
    case "approved": return "Approved"
    case "needs_review": return "Needs review"
    case "blocked": return "Blocked"
    case "rejected": return "Rejected"
    default: return effectiveStatus.replacingOccurrences(of: "_", with: " ").capitalized
    }
  }

  var isBlocked: Bool { effectiveStatus == "blocked" || effectiveStatus == "rejected" }
  var needsReview: Bool { effectiveStatus == "needs_review" }

  var sourceLabel: String {
    switch sourceKind {
    case "curated": return "Curated"
    case "community": return "Community"
    case "workspace": return "Workspace"
    default: return sourceKind.isEmpty ? "" : sourceKind.capitalized
    }
  }

  /// Section a capability belongs to in the browse view.
  var category: String {
    switch type {
    case "skill": return "Skills"
    case "connector": return "Connectors"
    case "plugin": return "Plugins"
    case "template": return "Templates"
    case "hook": return "Hooks"
    case "policy_pack": return "Policy packs"
    case "eval_pack": return "Eval packs"
    case "worker_pack": return "Worker packs"
    default: return "Capabilities"
    }
  }

  /// Glyph for the installed row and cards.
  var glyph: String {
    switch type {
    case "skill": return "sparkles"
    case "connector": return "cable.connector"
    case "plugin": return "puzzlepiece.extension"
    case "template": return "doc.on.doc"
    case "hook": return "link"
    case "policy_pack": return "checkmark.shield"
    case "eval_pack": return "checklist"
    case "worker_pack": return "person.2"
    case "capability_gate": return "lock.square"
    default: return "square.grid.2x2"
    }
  }

  static let categoryOrder = ["Skills", "Connectors", "Plugins", "Templates", "Hooks", "Policy packs", "Eval packs", "Worker packs", "Capabilities"]
}

struct TryPreview: Equatable {
  let capabilityID: String
  let helps: String
  let appearsIn: String
  let example: String
  let line: String
}

enum MarketplaceFilter: String, CaseIterable, Identifiable {
  case all
  case installed
  case needsReview
  case blocked

  var id: String { rawValue }

  var label: String {
    switch self {
    case .all: return "All"
    case .installed: return "On your shelf"
    case .needsReview: return "Needs review"
    case .blocked: return "Blocked"
    }
  }

  func matches(_ capability: Capability) -> Bool {
    switch self {
    case .all: return true
    case .installed: return capability.installed
    case .needsReview: return capability.needsReview
    case .blocked: return capability.isBlocked
    }
  }
}
