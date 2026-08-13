import Foundation
import SwiftUI

enum AppearanceMode: String, CaseIterable, Identifiable {
  case system
  case light
  case dark

  var id: String { rawValue }

  var title: String {
    switch self {
    case .system: "System"
    case .light: "Light"
    case .dark: "Dark"
    }
  }

  var detail: String {
    switch self {
    case .system: "Follow macOS"
    case .light: "Always light"
    case .dark: "Always dark"
    }
  }

  var systemImage: String {
    switch self {
    case .system: "circle.lefthalf.filled"
    case .light: "sun.max"
    case .dark: "moon"
    }
  }

  var colorScheme: ColorScheme? {
    switch self {
    case .system: nil
    case .light: .light
    case .dark: .dark
    }
  }
}

enum ForgeLane: String, CaseIterable, Identifiable, Hashable {
  case overview
  case runs
  case missions
  case review
  case fleet
  case machines
  case evidence

  var id: String { rawValue }

  var title: String {
    switch self {
    case .overview: "Build"
    case .runs: "Trails"
    case .missions: "Missions"
    case .review: "Review"
    case .fleet: "Fleets"
    case .machines: "Models"
    case .evidence: "Controls"
    }
  }

  var sidebarTitle: String {
    switch self {
    case .overview: "Build"
    default: title
    }
  }

  var subtitle: String {
    switch self {
    case .overview: "Start or resume"
    case .runs: "Work and proof trails"
    case .missions: "Long-horizon work"
    case .review: "Ship decisions"
    case .fleet: "Plans and capacity"
    case .machines: "Runner intelligence"
    case .evidence: "Proof and authority"
    }
  }

  var systemImage: String {
    switch self {
    case .overview: "hammer"
    case .runs: "play.rectangle"
    case .missions: "flag.checkered"
    case .review: "person.crop.circle.badge.checkmark"
    case .fleet: "rectangle.3.group"
    case .machines: "cpu"
    case .evidence: "checkmark.seal"
    }
  }

  var routeLane: RouteLane {
    switch self {
    case .overview: .overview
    case .runs: .runRoom
    case .missions: .work
    case .review: .review
    case .fleet: .fleets
    case .machines: .machines
    case .evidence: .evidence
    }
  }
}

enum AppRoute: Hashable, Identifiable {
  case home
  case forge(ForgeLane)
  case twin
  case pages
  case message
  case memory
  case marketplace
  case you

  var id: String {
    switch self {
    case .home: "home"
    case .forge(let lane): "forge-\(lane.rawValue)"
    case .twin: "twin"
    case .pages: "pages"
    case .message: "message"
    case .memory: "memory"
    case .marketplace: "marketplace"
    case .you: "you"
    }
  }

  /// Inverse of `id`, for restoring the last surface across relaunch.
  init?(id: String) {
    switch id {
    case "home": self = .home
    case "twin": self = .twin
    case "pages": self = .pages
    case "message": self = .message
    case "memory": self = .memory
    case "marketplace": self = .marketplace
    case "you": self = .you
    default:
      guard id.hasPrefix("forge-"), let lane = ForgeLane(rawValue: String(id.dropFirst("forge-".count))) else {
        return nil
      }
      self = .forge(lane)
    }
  }

  var title: String {
    switch self {
    case .home: "Home"
    case .forge(let lane): lane.title
    case .twin: "Twin"
    case .pages: "Pages"
    case .message: "Message"
    case .memory: "Memory"
    case .marketplace: "Marketplace"
    case .you: "You"
    }
  }

  var sidebarTitle: String {
    switch self {
    case .forge(let lane): lane.sidebarTitle
    default: title
    }
  }

  var subtitle: String {
    switch self {
    case .home: "Live now and needs you"
    case .forge(let lane): lane.subtitle
    case .twin: "Ask, deliberate, act"
    case .pages: "Context and knowledge"
    case .message: "Approvals and activity"
    case .memory: "What MDx can responsibly recall"
    case .marketplace: "Governed packs for every MDx app"
    case .you: "Identity and authority"
    }
  }

  var systemImage: String {
    switch self {
    case .home: "house"
    case .forge(let lane): lane.systemImage
    case .twin: "sparkles"
    case .pages: "doc.text"
    case .message: "bubble.left.and.bubble.right"
    case .memory: "brain.head.profile"
    case .marketplace: "shippingbox"
    case .you: "person.crop.circle"
    }
  }

  var primaryVerb: String {
    switch self {
    case .home: "Ask MDx"
    case .forge: "Start a build"
    case .twin: "Ask Twin"
    case .pages: "Publish"
    case .message: "Send"
    case .memory: "Review memory"
    case .marketplace: "Apply pack"
    case .you: "Grant"
    }
  }

  var isForge: Bool {
    if case .forge = self { return true }
    return false
  }

  static let topLevel: [AppRoute] = [.home, .twin, .pages, .message, .memory, .marketplace, .you]
}

enum OperatorSection: String, CaseIterable, Identifiable {
  case overview
  case work
  case runRoom
  case review
  case fleets
  case machines
  case controls
  case evidence
  case host

  var id: String { rawValue }

  var title: String {
    switch self {
    case .overview: "Forge"
    case .work: "Work"
    case .runRoom: "Runs"
    case .review: "Review"
    case .fleets: "Fleets"
    case .machines: "Machines"
    case .controls: "Forge"
    case .evidence: "Evidence"
    case .host: "You"
    }
  }

  var subtitle: String {
    switch self {
    case .overview: "Current signal and next move"
    case .work: "Repos, recipes, missions, and loops"
    case .runRoom: "Runs and live work trails"
    case .review: "Ship decisions and proof gates"
    case .fleets: "Fleet plans, runs, and capacity"
    case .machines: "Runner readiness and fleet posture"
    case .controls: "Build requests and authority chain"
    case .evidence: "Receipts, policy, and source routes"
    case .host: "Connection and local boundary"
    }
  }

  var sidebarSubtitle: String {
    switch self {
    case .overview: "Current signal"
    case .work: "Repos and recipes"
    case .runRoom: "Live work trails"
    case .review: "Ship decisions"
    case .fleets: "Runs and capacity"
    case .machines: "Runner readiness"
    case .controls: "Authority chain"
    case .evidence: "Receipts and policy"
    case .host: "Local boundary"
    }
  }

  var systemImage: String {
    switch self {
    case .overview: "hammer"
    case .work: "tray.full"
    case .runRoom: "play.rectangle"
    case .review: "person.crop.circle.badge.checkmark"
    case .fleets: "rectangle.3.group"
    case .machines: "cpu"
    case .controls: "slider.horizontal.3"
    case .evidence: "checkmark.seal"
    case .host: "desktopcomputer"
    }
  }
}

enum RouteStatus: String {
  case loading = "LOADING"
  case ok = "OK"
  case unavailable = "UNAVAILABLE"

  var displayName: String {
    switch self {
    case .loading: return "Connecting"
    case .ok: return "Connected"
    case .unavailable: return "Offline"
    }
  }
}

enum RouteLane: String {
  case overview
  case work
  case runRoom
  case review
  case fleets
  case machines
  case controls
  case evidence
  case host
}

enum GovernedActionKind: String, CaseIterable, Identifiable {
  case requestBuild
  case startRun
  case runFleet
  case approveBuild
  case provePlan
  case requestWorkerAuthority
  case talentSignoff
  case humanSignoff
  case sourceHostReadiness
  case prHandoff

  var id: String { rawValue }

  var title: String {
    switch self {
    case .requestBuild: "Request a build"
    case .startRun: "Start an admitted run"
    case .runFleet: "Run a fleet"
    case .approveBuild: "Approve the plan"
    case .provePlan: "Record plan proof"
    case .requestWorkerAuthority: "Ask for worker authority"
    case .talentSignoff: "Record Talent signoff"
    case .humanSignoff: "Record human signoff"
    case .sourceHostReadiness: "Check delivery readiness"
    case .prHandoff: "Prepare PR handoff"
    }
  }

  var path: String {
    switch self {
    case .requestBuild: "/forge/build-requests.json"
    case .startRun: "/forge/runs.json"
    case .runFleet: "/forge/machine-league/fleet-runs.json"
    case .approveBuild: "/forge/build-approvals.json"
    case .provePlan: "/forge/workflow-plan-proofs.json"
    case .requestWorkerAuthority: "/forge/worker-authority-requests.json"
    case .talentSignoff: "/forge/talent-authorizations.json"
    case .humanSignoff: "/forge/human-ratification-preflights.json"
    case .sourceHostReadiness: "/forge/source-host-readiness.json"
    case .prHandoff: "/forge/pr-handoffs.json"
    }
  }

  var plainLanguageBoundary: String {
    switch self {
    case .requestBuild:
      "Records the request and stops before worker spawn, provider calls, patching, deployment, or production writes."
    case .startRun:
      "Starts only through the Forge run route. The route still owns repo, model, worktree, command, and authority refusals."
    case .runFleet:
      "Asks Machine League to record a governed fleet run. Live adapter execution stays controlled by the route."
    case .approveBuild:
      "Records a human approval of scope and plan. It does not execute the workflow."
    case .provePlan:
      "Records that a plan is shaped enough to inspect. Workflow execution remains blocked unless later gates open."
    case .requestWorkerAuthority:
      "Asks for worker authority. It does not issue credentials or spawn a worker."
    case .talentSignoff:
      "Records Talent intent without issuing worker credentials or opening live execution."
    case .humanSignoff:
      "Preflights a human decision. Shipping and deployment remain separate governed calls."
    case .sourceHostReadiness:
      "Checks source-host delivery posture without reading credential values, pushing, or opening a PR."
    case .prHandoff:
      "Prepares a dry-run handoff artifact from review proof. It never pushes or opens a PR."
    }
  }
}

struct GovernedActionDraft: Equatable {
  var intent: String = "Build the next governed Forge slice from the native MDx app."
  var runID: String = ""
  var planHash: String = "plan-proof-local-native-mdx"
  var workerProfile: String = "build_agent"
  var decision: String = "ratify"
  var targetHost: String = "github"
  var scope: String = "forge_clean_build_entry"
  var workerCount: Int = 4

  func body(for action: GovernedActionKind) -> [String: Any] {
    let suffix = Self.identifierSuffix()
    switch action {
    case .requestBuild:
      return [
        "request_id": "native_mdx_build_\(suffix)",
        "source_surface": "native_mdx",
        "requested_change": intent,
        "expected_plan_hash": planHash
      ]
    case .startRun:
      return [
        "intent": intent,
        "fleet_width": "1",
        "work_item_id": "native_mdx_work_\(suffix)"
      ]
    case .runFleet:
      return [
        "worker_count": workerCount,
        "task_class": "bug_fix",
        "language_pack_id": "rust-cargo",
        "eval_fixture": "rust_tax_rounding",
        "execute_live": false
      ]
    case .approveBuild:
      return [
        "approval_id": "native_mdx_approval_\(suffix)",
        "approved_scope": scope,
        "plan_hash": planHash
      ]
    case .provePlan:
      return [
        "plan_proof_id": "native_mdx_plan_\(suffix)",
        "approved_plan_hash": planHash,
        "plan_summary": intent
      ]
    case .requestWorkerAuthority:
      return [
        "authority_request_id": "native_mdx_worker_\(suffix)",
        "requested_worker_profile": workerProfile,
        "authority_summary": intent
      ]
    case .talentSignoff:
      return [
        "talent_authorization_id": "native_mdx_talent_\(suffix)",
        "requested_worker_profile": workerProfile,
        "talent_scope": scope,
        "authorization_summary": intent
      ]
    case .humanSignoff:
      return [
        "ratification_preflight_id": "native_mdx_human_\(suffix)",
        "candidate_decision": decision,
        "scope": scope,
        "evidence_summary": intent,
        "preflight_summary": "Native MDx recorded the human signoff shape for review."
      ]
    case .sourceHostReadiness:
      return [
        "run_id": runID,
        "target_host": targetHost,
        "base_branch": "main"
      ]
    case .prHandoff:
      return [
        "run_id": runID,
        "target_host": targetHost
      ]
    }
  }

  private static func identifierSuffix() -> String {
    let value = Int(Date().timeIntervalSince1970)
    return "\(value)"
  }
}

struct GovernedActionResult: Equatable {
  let action: GovernedActionKind
  let status: String
  let title: String
  let detail: String
  let route: String
  let receiptID: String
  let rawSummary: String
}

struct RouteCard: Identifiable, Equatable {
  let id: String
  let title: String
  let path: String
  let status: RouteStatus
  let detail: String
  let lane: RouteLane
  let receiptBacked: Bool
  let readOnly: Bool
  let metric: String
}

struct WorkItem: Identifiable, Equatable {
  let id: String
  let title: String
  let subtitle: String
  let status: String
  let stage: String
}

struct RunStageItem: Identifiable, Equatable {
  let id: String
  let title: String
  let status: String
  let detail: String
}

struct ReviewArtifact: Identifiable, Equatable {
  let id: String
  let title: String
  let status: String
  let detail: String
  let route: String
}

struct HostProject: Identifiable, Equatable, Codable {
  let id: String
  let path: String
  let addedAt: Date
}

struct PackageReadinessItem: Identifiable, Equatable {
  let id: String
  let title: String
  let status: String
  let detail: String
}

extension Bundle {
  /// Human version label for Settings/About. Falls back to "dev build" for
  /// unbundled `swift run` binaries that carry no Info.plist versioning.
  var appVersionLabel: String {
    let short = infoDictionary?["CFBundleShortVersionString"] as? String
    let build = infoDictionary?["CFBundleVersion"] as? String
    switch (short, build) {
    case let (short?, build?): return "\(short) (\(build))"
    case let (short?, nil): return short
    case let (nil, build?): return "build \(build)"
    default: return "dev build"
    }
  }
}

struct MachineRunner: Identifiable, Equatable {
  let id: String
  let name: String
  let kind: String
  let model: String
  let adapterKind: String
  let protocolTier: String
  let protocolLabel: String
  let protocolDetail: String
  let status: String
  let executionAllowed: Bool
  let selected: Bool
  let castable: Bool
  let requiresClearance: Bool
  let clearanceMode: String
  let clearanceLabel: String
  let scorecardEligible: Bool
  let runtimeChecked: Bool
  let binaryPresent: Bool
  let versionObserved: String
  let optionEnabled: Bool
  let optionEnablement: String
  let liveExecutionReady: Bool
  let smokeStatus: String
  let smokePassed: Bool
  let evidenceCount: Int
  let acceptedCount: Int
  let tasksAttempted: Int
  let passRatePct: Int
  let readinessLine: String
  let compatibilityLine: String
  let adminActionLine: String

  var isNative: Bool {
    kind == "mdx_native" || adapterKind == "mdx_native" || id == "mdx_native_harness_runner"
  }

  var canStageFaceOff: Bool {
    !isNative && optionEnabled && (!requiresClearance || !clearanceMode.isEmpty)
  }

  /// Whether this machine is signed in and allowed for Forge to use, in the
  /// words an operator reads on the tile. The raw clearance mode stays in the
  /// machine detail sheet.
  var connectionOpen: Bool {
    isNative || !requiresClearance || !clearanceMode.isEmpty
  }

  var connectionLabel: String {
    if isNative { return "Built in" }
    if connectionOpen { return "Connected" }
    switch adapterKind {
    case "grok_build_cli": return "Needs an xAI key"
    case "claude_code": return "Needs an Anthropic key"
    default: return "Needs a key"
    }
  }

  var runtimeStatusLabel: String {
    if isNative { return "Built in" }
    if liveExecutionReady { return "Verified on this Mac" }
    if smokePassed { return "Verified on this Mac" }
    if !runtimeChecked { return "Not verified" }
    if binaryPresent { return "Installed, not verified" }
    return "Not installed"
  }
}

struct ModelRoleRoute: Identifiable, Equatable {
  let id: String
  let title: String
  let model: String
  let provider: String
  let slot: String
  let ready: Bool
  let assigned: Bool
  let advisoryOnly: Bool

  var displayModel: String {
    if !model.isEmpty { return model }
    return ready ? "Ready" : "Not connected yet"
  }

  var supportLine: String {
    if advisoryOnly {
      return assigned ? "Advisor assigned" : "Advisor auto"
    }
    if assigned { return "Assigned" }
    if !slot.isEmpty { return "Auto routed" }
    return ready ? "Auto routed" : "Connect a model"
  }
}

struct FleetPlan: Identifiable, Equatable {
  let id: String
  let spec: String
  let checks: [String]
  let phase: String
  let status: String
  let repo: String
  let languagePack: String
  let plannerModel: String
  let streamCount: Int
  let requestedWidth: Int
  let workerLimit: Int
  let checkCount: Int
  let suggestedChecks: [String]
  let proofSummary: String
  let reviewLine: String
  let reviewConcerns: String
  let reviewStatus: String
  let planningStage: String
  let planningDetail: String
  let builderMix: String
  let sensitivityLine: String
  let route: String

  /// The first sentence or line of the planner prompt, for the card title.
  /// The full prompt (often several lines of file paths) stays in Details.
  var goalLine: String {
    let trimmed = spec.trimmingCharacters(in: .whitespacesAndNewlines)
    let firstLine = trimmed.split(whereSeparator: \.isNewline).first.map(String.init) ?? trimmed
    if let dot = firstLine.firstIndex(of: ".") {
      let sentence = firstLine[..<dot].trimmingCharacters(in: .whitespaces)
      if sentence.count >= 12 { return sentence }
    }
    return firstLine
  }

  var hasFullPrompt: Bool {
    spec.contains("\n") || spec.trimmingCharacters(in: .whitespacesAndNewlines).count > goalLine.count + 8
  }

  /// "1x CODEXMINI, 7x default" reads as "1 Codex Mini, 7 default".
  var builderMixLabel: String {
    guard !builderMix.isEmpty else { return "" }
    return builderMix
      .split(separator: ",")
      .map { part -> String in
        let token = part.trimmingCharacters(in: .whitespaces)
        let comps = token.split(separator: " ", maxSplits: 1).map(String.init)
        guard comps.count == 2 else { return token }
        let count = comps[0].hasSuffix("x") ? String(comps[0].dropLast()) : comps[0]
        return "\(count) \(FleetPlan.prettyModel(comps[1]))"
      }
      .joined(separator: ", ")
  }

  static func prettyModel(_ raw: String) -> String {
    switch raw.lowercased() {
    case "default": return "default"
    case "codexmini", "codex_mini", "codex-mini": return "Codex Mini"
    case "codex": return "Codex"
    case "grok", "grokfast", "grok_fast": return "Grok"
    case "claude": return "Claude"
    default: return raw.lowercased().prefix(1).uppercased() + raw.lowercased().dropFirst()
    }
  }

  var isRatified: Bool {
    status.localizedCaseInsensitiveContains("ratified") || phase == "ready"
  }

  var isPlanning: Bool {
    !needsPlanningRepair
      && (status.localizedCaseInsensitiveContains("planning") || phase == "planning")
  }

  var isDelayedPlanning: Bool {
    status.localizedCaseInsensitiveContains("delayed") || planningStage == "planner_slow"
  }

  var needsPlanningRepair: Bool {
    status.localizedCaseInsensitiveContains("needs_attention")
      || planningStage == "needs_attention"
  }

  var needsPlanReview: Bool {
    if needsPlanningRepair { return true }
    if isPlanning { return false }
    if !reviewConcerns.isEmpty || reviewLine.localizedCaseInsensitiveContains("asked for changes") { return true }
    return ["missing_reviewer", "review_unavailable"].contains(reviewStatus)
  }

  var displayLine: String {
    if needsPlanningRepair && !planningDetail.isEmpty { return planningDetail }
    if isPlanning && !planningDetail.isEmpty { return planningDetail }
    return reviewLine
  }

  var planningStageLabel: String {
    switch planningStage {
    case "accepted": return "Accepted"
    case "preparing_planner": return "Preparing"
    case "planner_drafting": return "Planner"
    case "planner_slow": return "Still planning"
    case "checking_plan": return "Checking"
    case "planner_retry": return "Retrying"
    case "advisor_review": return "Advisor"
    case "recording_plan": return "Recording"
    case "needs_attention": return "Needs attention"
    default:
      return planningStage.replacingOccurrences(of: "_", with: " ").capitalized
    }
  }
}

struct FleetRun: Identifiable, Equatable {
  let id: String
  let goal: String
  let checks: [String]
  let running: Bool
  let finished: Bool
  let integrationState: String
  let integrationDetail: String
  let reviewVerdict: String
  let status: String
  let summary: String
  let recovery: String
  let lanes: [FleetLane]

  var displayStatus: String { StatusCopy.human(status) }

  var attentionCount: Int {
    lanes.filter { $0.needsAttention }.count + (hasIntegrationFailure ? 1 : 0)
  }

  var hasIntegrationFailure: Bool {
    integrationState == "did_not_land"
      || integrationDetail.localizedCaseInsensitiveContains("no branch")
  }

  var needsRecovery: Bool {
    attentionCount > 0
  }

  var workingCount: Int {
    lanes.filter { $0.isWorking }.count
  }

  var isActive: Bool {
    (running && !finished) || workingCount > 0
  }

  /// The disclosure summary that stands in for the lane list until expanded:
  /// "5 lanes, all done", "5 lanes, 2 need attention", "5 lanes, 3 working".
  var lanesSummaryLine: String {
    let count = lanes.count
    let laneWord = count == 1 ? "lane" : "lanes"
    let attention = lanes.filter(\.needsAttention).count
    if attention > 0 {
      return "\(count) \(laneWord), \(attention) need\(attention == 1 ? "s" : "") attention"
    }
    if workingCount > 0 {
      return "\(count) \(laneWord), \(workingCount) working"
    }
    return "\(count) \(laneWord), all done"
  }

  /// A readable name for the fleet run instead of the raw "fleet_005777" id.
  var displayName: String {
    if !goal.isEmpty { return goal }
    if id.hasPrefix("fleet_") { return "Fleet run " + id.dropFirst("fleet_".count) }
    if !summary.isEmpty { return summary }
    return id.replacingOccurrences(of: "_", with: " ").capitalized
  }
}

struct FleetLane: Identifiable, Equatable {
  let id: String
  let streamID: String
  let state: String
  let runID: String
  let detail: String
  let coder: String
  let model: String
  let castingStatus: String
  let castingReason: String
  let missingStrategy: String

  var needsAttention: Bool {
    state.localizedCaseInsensitiveContains("attention") || detail.localizedCaseInsensitiveContains("failed")
  }

  var isWorking: Bool {
    state.localizedCaseInsensitiveContains("working")
  }

  /// A human lane name from the raw stream id (s1_policy_status).
  var laneName: String {
    guard !streamID.isEmpty else { return "Lane" }
    return streamID
      .replacingOccurrences(of: "_", with: " ")
      .split(separator: " ")
      .map { $0.prefix(1).uppercased() + $0.dropFirst() }
      .joined(separator: " ")
  }

  /// A human state that pulls the "N/M" checks fraction out of a compound raw
  /// status ("RUN FINISHED DONE done 1/5" reads "Done, 1 of 5 checks").
  static func humanState(_ raw: String) -> String {
    var fraction: String?
    for token in raw.split(whereSeparator: { $0 == " " }) {
      let parts = token.split(separator: "/")
      if parts.count == 2, parts.allSatisfy({ Int($0) != nil }) {
        fraction = "\(parts[0]) of \(parts[1]) checks"
        break
      }
    }
    let lower = raw.lowercased()
    let word: String
    if lower.contains("attention") || lower.contains("fail") { word = "Needs attention" }
    else if lower.contains("done") || lower.contains("finished") { word = "Done" }
    else if lower.contains("working") || lower.contains("running") { word = "Working" }
    else { word = StatusCopy.human(raw) }
    if let fraction { return "\(word), \(fraction)" }
    return word
  }

  var stateLabel: String { FleetLane.humanState(state) }

  /// The per-lane detail line, humanized when it is a raw status dump
  /// ("status RUN FINISHED DONE done 1/5") and passed through when it is prose.
  var detailLabel: String {
    let lower = detail.lowercased()
    if lower.hasPrefix("status ") || lower.contains("run finished") || (detail.contains("/") && lower.contains("done")) {
      return FleetLane.humanState(detail)
    }
    return detail
  }

  /// One quiet human line for casting and evidence, instead of the repeated
  /// raw "Insufficient Evidence  no matching language task model evidence yet".
  var castingNote: String {
    let combined = [castingStatus, missingStrategy, castingReason]
      .joined(separator: " ")
      .lowercased()
    if combined.contains("no matching") || combined.contains("insufficient evidence") {
      return "No matching eval evidence yet"
    }
    let parts = [castingStatus, missingStrategy, castingReason].filter { !$0.isEmpty }
    return parts.joined(separator: " · ")
  }
}

struct EvidenceItem: Identifiable, Equatable {
  let id: String
  let title: String
  let detail: String
  let route: String
  let readOnly: Bool
}

struct AuthorityItem: Identifiable, Equatable {
  let id: String
  let title: String
  let detail: String
  let allowed: Bool
}

struct SurfaceItem: Identifiable, Equatable {
  let id: String
  let title: String
  let subtitle: String
  let status: String
  let detail: String
  let route: String
}

struct ForgeDecision: Identifiable, Equatable {
  let id: String
  let kind: String
  let title: String
  let detail: String
  let lane: ForgeLane
  let subjectID: String?
  let priority: Int
  let actionState: String
  let blockerCodes: [String]
  let evidenceReceiptIDs: [String]
}

struct CockpitMetric: Identifiable, Equatable {
  let id: String
  let title: String
  let value: String
  let detail: String
}

struct OperatorSnapshot: Equatable {
  var loadedAt: Date?
  var baseURL: URL
  var connectionStatus: RouteStatus
  var currentSignal: String
  var safeNextMove: String
  var boundary: String
  var workspaceIntent: String
  var currentStage: String
  var authority: String
  var humanDecision: String
  var productPosture: String
  var runCount: Int
  var repoCount: Int
  var recipeCount: Int
  var missionCount: Int
  var fleetPlanCount: Int
  var fleetRunCount: Int
  var arbitrationCount: Int
  var reviewDecisionCount: Int
  var modelProviderCount: Int
  var capacityQueueDepth: Int
  var capacityWorkers: Int
  var capacityActiveWorkers: Int
  var receiptCount: Int
  var policyDecisionCount: Int
  var blockedActionCount: Int
  var safeActionCount: Int
  var selectedRunner: String
  var fallbackRunner: String
  var routeCards: [RouteCard]
  var workItems: [WorkItem]
  var runStages: [RunStageItem]
  var reviewArtifacts: [ReviewArtifact]
  var workbenchItems: [SurfaceItem]
  var runRoomItems: [SurfaceItem]
  var forgeRuns: [ForgeRun]
  var forgeDecisions: [ForgeDecision]
  var parallelExecutionGroups: [ForgeParallelExecutionGroup]
  var reviewItems: [SurfaceItem]
  var fleetItems: [SurfaceItem]
  var fleetPlans: [FleetPlan]
  var fleetRuns: [FleetRun]
  var startedFleetIDs: [String]
  var controlItems: [SurfaceItem]
  var runners: [MachineRunner]
  var modelRoles: [ModelRoleRoute]
  var evidenceItems: [EvidenceItem]
  var authorityItems: [AuthorityItem]

  var reviewReadyCount: Int {
    forgeRuns.filter(\.isReviewReady).count
  }

  var metrics: [CockpitMetric] {
    [
      CockpitMetric(id: "runs", title: "Runs", value: "\(runCount)", detail: "Forge work items"),
      CockpitMetric(id: "missions", title: "Missions", value: "\(missionCount)", detail: "Long-horizon work"),
      CockpitMetric(id: "fleet", title: "Fleet", value: "\(fleetRunCount)", detail: "Runs across \(capacityWorkers) workers"),
      CockpitMetric(id: "review", title: "Review", value: "\(reviewReadyCount)", detail: "Runs ready for your call"),
      CockpitMetric(id: "receipts", title: "Receipts", value: "\(receiptCount)", detail: "Proof records"),
      CockpitMetric(id: "blocked", title: "Locked", value: "\(blockedActionCount)", detail: "Authority held")
    ]
  }

  static func offline(baseURL: URL, reason: String = "Local MDx route server is not connected.") -> OperatorSnapshot {
    OperatorSnapshot(
      loadedAt: nil,
      baseURL: baseURL,
      connectionStatus: .unavailable,
      currentSignal: "Forge is offline locally",
      safeNextMove: "Inspect local setup",
      boundary: reason,
      workspaceIntent: "Connect to a local MDx server to inspect Forge.",
      currentStage: "offline",
      authority: "none",
      humanDecision: "Start local MDx, then refresh.",
      productPosture: "inspect_only",
      runCount: 0,
      repoCount: 0,
      recipeCount: 0,
      missionCount: 0,
      fleetPlanCount: 0,
      fleetRunCount: 0,
      arbitrationCount: 0,
      reviewDecisionCount: 0,
      modelProviderCount: 0,
      capacityQueueDepth: 0,
      capacityWorkers: 0,
      capacityActiveWorkers: 0,
      receiptCount: 0,
      policyDecisionCount: 0,
      blockedActionCount: 0,
      safeActionCount: 0,
      selectedRunner: "Unavailable",
      fallbackRunner: "Unavailable",
      routeCards: [
        RouteCard(
          id: "offline",
          title: "Local route server",
          path: baseURL.absoluteString,
          status: .unavailable,
          detail: reason,
          lane: .host,
          receiptBacked: false,
          readOnly: true,
          metric: "offline"
        )
      ],
      workItems: [],
      runStages: [],
      reviewArtifacts: [],
      workbenchItems: [],
      runRoomItems: [],
      forgeRuns: [],
      forgeDecisions: [],
      parallelExecutionGroups: [],
      reviewItems: [],
      fleetItems: [],
      fleetPlans: [],
      fleetRuns: [],
      startedFleetIDs: [],
      controlItems: [],
      runners: [],
      modelRoles: [],
      evidenceItems: [],
      authorityItems: [
        AuthorityItem(id: "offline", title: "Local connection", detail: reason, allowed: false)
      ]
    )
  }
}

enum LoadPhase: Equatable {
  case idle
  case loading
  case loaded(Date)
  /// Load succeeded and there is genuinely nothing to show. Distinct from
  /// failure: an empty library is a fact, an unreachable kernel is a problem.
  case empty
  /// A refresh failed but earlier data is still on screen.
  case stale(String)
  case failed(String)

  var isSettled: Bool {
    switch self {
    case .idle, .loading: return false
    default: return true
    }
  }
}
