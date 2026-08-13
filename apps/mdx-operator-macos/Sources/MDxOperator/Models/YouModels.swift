import Foundation

struct IdentitySession: Equatable {
  let userID: String
  let tenantID: String
  let role: String
  let expiresAt: String
  let roles: [String]
  let admissionScope: String
  let admissionStatus: String

  var displayName: String {
    if Self.isOpaqueUserID(userID) { return "You" }
    return Page.humanActor(userID).replacingOccurrences(of: "_", with: " ").capitalized
  }
  var roleLabel: String { role.capitalized }

  static func isOpaqueUserID(_ value: String) -> Bool {
    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return true }
    if UUID(uuidString: trimmed) != nil { return true }
    return trimmed.range(
      of: #"^[0-9a-f]{24,}$"#,
      options: [.regularExpression, .caseInsensitive]
    ) != nil
  }
}

struct Clearance: Equatable {
  let status: String
  let safeControls: [String]
  let blockedControls: [String]
  let blockedAuthorities: [String]

  var isFailClosed: Bool { status == "FAIL_CLOSED" }

  static func label(_ raw: String) -> String {
    switch raw.lowercased() {
    case "worker_spawn", "worker_spawning", "spawn_worker":
      return "Agents starting work on their own"
    case "patch_apply", "patch_application", "apply_patch":
      return "Applying code changes"
    case "deploy", "deployment", "deploy_production":
      return "Deploying to production"
    case "production_write", "production_writes":
      return "Writing to production"
    case "ship", "ship_decision":
      return "Shipping"
    case "read_only", "inspect", "inspect_only":
      return "Reading and inspecting"
    case "":
      return ""
    default:
      return raw.replacingOccurrences(of: "_", with: " ").capitalized
    }
  }
}
