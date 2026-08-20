import Foundation

struct ForgeRunProjection: Equatable {
  let runs: [ForgeRun]
  let parallelExecutionGroups: [ForgeParallelExecutionGroup]
}

struct ForgeFleetProjections: Equatable {
  let plans: [FleetPlan]
  let runs: [FleetRun]
}

struct ForgeRun: Identifiable, Equatable {
  let id: String
  let title: String
  let intent: String
  let origin: String
  let systemOrigin: String
  let status: String
  let terminalState: String
  let stage: String
  let repo: String
  let harness: String
  let model: String
  let branch: String
  let commitSha: String
  let turns: Int
  let modelCalls: Int
  let toolCalls: Int
  let checksPassed: Int
  let checksFailed: Int
  let finalLine: String
  let streamRoute: String
  let diffReady: Bool
  let diffFileCount: Int
  let workerCount: Int
  let runner: ForgeRunnerProfile
  let execution: ForgeExecutionGeometry
  let parallelCandidate: ForgeParallelCandidate
  let workClassification: ForgeWorkClassification
  let languageTaskAlignment: ForgeLanguageTaskAlignment
  let quarantine: ForgeQuarantine
  let contextTelemetry: ForgeContextTelemetry
  let stages: [ForgeStage]
  let events: [ForgeEvent]
  let controls: [ForgeControl]

  var isRunning: Bool {
    Self.isLiveOperatorRun(
      id: id,
      origin: origin,
      systemOrigin: systemOrigin,
      terminalState: terminalState
    )
  }
  var isSystemEvidence: Bool { origin == "system" || systemOrigin == "forge_system" }
  var hasBranch: Bool { !branch.trimmingCharacters(in: .whitespaces).isEmpty }
  var isReviewReady: Bool {
    !isSystemEvidence
      && controls.contains { ($0.action == "ship" || $0.action == "review") && $0.allowed }
  }
  /// The single kernel field the Ship button's enabled state also reads, so the
  /// inspector's ship claim and the Ship control never disagree.
  var canShip: Bool { control("ship")?.allowed == true }

  static func isLiveOperatorRun(
    id: String,
    origin: String,
    systemOrigin: String,
    terminalState: String
  ) -> Bool {
    terminalState == "IN_PROGRESS"
      && origin != "system"
      && systemOrigin != "forge_system"
      && id.hasPrefix("forge_run_")
  }
  var isLeagueRun: Bool {
    !runner.id.isEmpty && runner.kind != "mdx_native" || !quarantine.status.isEmpty || !quarantine.leagueRecommendation.isEmpty
  }

  var hasWorkIdentity: Bool {
    !harness.isEmpty
      || !model.isEmpty
      || workerCount > 1
      || !execution.lane.isEmpty
      || !workClassification.isEmpty
      || !languageTaskAlignment.isEmpty
      || contextTelemetry.hasContext
      || quarantine.outputHeld
      || localBaseSnapshot != nil
  }

  var localBaseSnapshot: ForgeLocalBaseSnapshot? {
    events.compactMap(ForgeLocalBaseSnapshot.init(event:)).last
  }

  var isReviewableWithProofCaveat: Bool {
    !isRunning
      && hasBranch
      && checksFailed > 0
      && selectedProofRedOnArrival
      && !status.lowercased().contains("cannot")
      && (status.lowercased().contains("done") || stage.lowercased().contains("ready"))
  }

  var selectedProofTurnedGreen: Bool {
    selectedProofRedOnArrival && proofRecoveredAfterFailure
  }

  /// The selected proof can fail while Forge is still working, then pass after
  /// a correction. The aggregate failed-check count preserves those attempts,
  /// so the final operator-facing state must follow the latest proof outcome
  /// instead of treating any historical failure as the current result.
  var proofRecoveredAfterFailure: Bool {
    if checksFailed > 0, projectedSelectedProofPassed { return true }
    // Older projections do not carry selected_proof_status. Keep the existing
    // baseline turnaround contract as the narrow compatibility fallback, and
    // let the latest relevant event decide the final proof outcome.
    guard selectedProofRedOnArrival else { return false }
    for event in events.reversed() {
      if event.isPostChangeProofPass { return true }
      if event.isProofFailure { return false }
    }
    return false
  }

  private var projectedSelectedProofPassed: Bool {
    let marker = "selected_proof_status="
    // Prefer the terminal summary, then walk backward through events. Extract
    // only the status token so punctuation added for display cannot turn a
    // passed proof back into a false negative.
    let lines = [finalLine] + events.reversed().map { "\($0.summary) \($0.detail)" }
    for line in lines {
      guard let range = line.lowercased().range(of: marker) else { continue }
      let value = line[range.upperBound...]
        .prefix { $0.isLetter || $0 == "_" || $0 == "-" }
      return ["passed", "green", "succeeded"].contains(value.lowercased())
    }
    return false
  }

  var selectedProofRedOnArrival: Bool {
    events.contains { event in
      event.isBaselineProofFailure
        || event.summary.localizedCaseInsensitiveContains("red on arrival")
        || event.detail.localizedCaseInsensitiveContains("red on arrival")
        || event.summary.localizedCaseInsensitiveContains("already red before")
        || event.detail.localizedCaseInsensitiveContains("already red before")
        || event.summary.localizedCaseInsensitiveContains("pre-existing selected proof")
        || event.detail.localizedCaseInsensitiveContains("pre-existing selected proof")
    }
  }

  var hasProofTrouble: Bool {
    checksFailed > 0
      || status.lowercased().contains("cannot")
      || status.lowercased().contains("exhausted")
      || status.lowercased().contains("fail")
      || latestProofFailure != nil
  }

  /// A finished run whose proof ended green: no failing checks, not stopped for
  /// turns or scope, and either review-ready or recovered to green. Its
  /// Proving/Done stages are completed-green, not warnings, even if a proof
  /// failure sits earlier in history.
  var terminalStateIsPositive: Bool {
    guard !isRunning else { return false }
    // A recovered baseline (red on arrival, green on this branch) keeps a
    // failed-check count, but the run itself ended green.
    if proofRecoveredAfterFailure { return true }
    if checksFailed > 0 { return false }
    return terminalState == "SUCCEEDED" || terminalState == "NO_CHANGE"
  }

  var hasAuthorityGateBlock: Bool {
    events.contains { event in
      event.summary.localizedCaseInsensitiveContains("blocked_before_principal_review")
        || event.detail.localizedCaseInsensitiveContains("blocked_before_principal_review")
        || event.summary.localizedCaseInsensitiveContains("machine_gates_passed=false")
        || event.detail.localizedCaseInsensitiveContains("machine_gates_passed=false")
    }
  }

  var proofTroubleTitle: String {
    let s = status.lowercased()
    if !isRunning && proofRecoveredAfterFailure {
      return "Ready for review: proof turned green"
    }
    if isReviewableWithProofCaveat && selectedProofTurnedGreen {
      return "Ready for review: proof turned green"
    }
    if isReviewableWithProofCaveat { return "Ready for review with a proof caveat" }
    if selectedProofRedOnArrival { return "Proof started red" }
    if s.contains("cannot") { return "Forge needs a narrower next step" }
    if s.contains("exhausted") { return "Forge ran out of turns" }
    if checksFailed > 0 { return "Proof is not green yet" }
    return "Forge needs attention"
  }

  var proofTroubleDetail: String {
    if checksFailed > 0 {
      let total = checksPassed + checksFailed
      let checkLine = total > 0
        ? "\(checksFailed) of \(total) checks failed."
        : "\(checksFailed) checks failed."
      if isReviewableWithProofCaveat && selectedProofTurnedGreen {
        return "The selected check failed before the change and passes on this branch."
      }
      if !isRunning && proofRecoveredAfterFailure {
        return "The selected check failed during the run and now passes on this branch. Earlier attempts remain in proof history."
      }
      if isReviewableWithProofCaveat {
        return "\(checkLine) The branch is still reviewable because the selected proof was already red before this change."
      }
      if selectedProofRedOnArrival {
        if let event = latestProofFailure {
          return "\(checkLine) The selected proof was already red before this run: \(event.failingCommandLabel)."
        }
        return "\(checkLine) The selected proof was already red before this run."
      }
      if let event = latestProofFailure {
        return "\(checkLine) \(event.conciseFailureText)"
      }
      return checkLine
    }
    if let event = latestProofFailure {
      return event.conciseFailureText
    }
    if let event = latestBlockerEvent {
      return event.conciseFailureText
    }
    if !finalLine.isEmpty {
      return finalLine
    }
    return "Review the proof and decide whether to steer, revise, or stop."
  }

  var latestProofFailure: ForgeEvent? {
    events.last { event in
      event.isProofFailure
    }
  }

  var latestBlockerEvent: ForgeEvent? {
    events.last { event in
      event.summary.localizedCaseInsensitiveContains("cannot_proceed")
        || event.detail.localizedCaseInsensitiveContains("cannot_proceed")
    }
  }

  var displayStatus: String { StatusCopy.human(status) }

  /// The current stage in human words. Raw stage stays in `stage`.
  var displayStage: String { StatusCopy.stage(stage) }

  /// A human-readable name, prettifying machine ids that leak through as titles.
  var displayName: String {
    let base = title.isEmpty ? id : title
    if base.contains(" ") { return base }
    if base.contains("_") || base.contains("-") {
      let words = base
        .replacingOccurrences(of: "_", with: " ")
        .replacingOccurrences(of: "-", with: " ")
        .split(separator: " ")
        .map { $0.prefix(1).uppercased() + $0.dropFirst() }
      return words.joined(separator: " ")
    }
    return base
  }

  func control(_ action: String) -> ForgeControl? {
    controls.first { $0.action == action }
  }
}

struct ForgeWorkClassification: Equatable {
  let recommendedShape: String
  let taskClass: String
  let complexityTier: String
  let confidencePct: Int
  let rationale: String

  static let empty = ForgeWorkClassification(
    recommendedShape: "",
    taskClass: "",
    complexityTier: "",
    confidencePct: 0,
    rationale: ""
  )

  var isEmpty: Bool {
    recommendedShape.isEmpty && taskClass.isEmpty && complexityTier.isEmpty
  }

  var chipLabel: String {
    let classLabel = polishedWorkLabel(taskClass)
    let tierLabel = polishedWorkLabel(complexityTier)
    if !classLabel.isEmpty && !tierLabel.isEmpty { return "\(classLabel), \(tierLabel)" }
    if !classLabel.isEmpty { return classLabel }
    return tierLabel
  }

  var shapeLabel: String {
    polishedWorkLabel(recommendedShape)
  }

  var detailLine: String {
    var parts: [String] = []
    if !shapeLabel.isEmpty { parts.append("shape \(shapeLabel.lowercased())") }
    if confidencePct > 0 { parts.append("\(confidencePct)% confidence") }
    if !rationale.isEmpty { parts.append(rationale) }
    return parts.joined(separator: ". ")
  }

  /// The same read, written as a sentence for the inspector instead of the
  /// terse "shape run. 82% confidence." chip prose.
  var plainSentence: String {
    guard !isEmpty else { return "" }
    let work = chipLabel.isEmpty ? shapeLabel : chipLabel
    var sentence = "Forge read this as "
    sentence += work.isEmpty ? "work it could run" : work.lowercased()
    if confidencePct > 0 {
      sentence += ", \(confidencePct)% sure"
    }
    sentence += "."
    return sentence
  }
}

struct ForgeLanguageTaskAlignment: Equatable {
  let taskCorpusID: String
  let status: String
  let taskClass: String
  let complexityTier: String
  let visibleCheck: String
  let hiddenCheckSlot: String

  static let empty = ForgeLanguageTaskAlignment(
    taskCorpusID: "",
    status: "",
    taskClass: "",
    complexityTier: "",
    visibleCheck: "",
    hiddenCheckSlot: ""
  )

  var isEmpty: Bool {
    taskCorpusID.isEmpty && status.isEmpty && taskClass.isEmpty && complexityTier.isEmpty
  }

  /// The kernel reports a generic or missing corpus when a repo has no test
  /// suite Forge recognizes. Say that in plain words instead of the raw pack id.
  var hasNoTestSuite: Bool {
    let all = [taskCorpusID, status, taskClass].map { $0.lowercased() }
    return all.contains { $0.contains("no_known") || $0.contains("generic") || $0 == "none" || $0 == "unknown" }
  }

  var chipLabel: String {
    if hasNoTestSuite { return "No test suite detected" }
    let classLabel = polishedWorkLabel(taskClass)
    let tierLabel = polishedWorkLabel(complexityTier)
    if !classLabel.isEmpty && !tierLabel.isEmpty { return "\(classLabel), \(tierLabel)" }
    if !taskCorpusID.isEmpty { return polishedWorkLabel(taskCorpusID) }
    if !classLabel.isEmpty { return classLabel }
    if !tierLabel.isEmpty { return tierLabel }
    return polishedWorkLabel(status)
  }

  var detailLine: String {
    if hasNoTestSuite { return "No test suite detected" }
    var parts: [String] = []
    if !visibleCheck.isEmpty { parts.append("visible check \(visibleCheck)") }
    if !hiddenCheckSlot.isEmpty { parts.append("hidden slot \(hiddenCheckSlot)") }
    if !status.isEmpty { parts.append(polishedWorkLabel(status)) }
    return parts.joined(separator: ". ")
  }
}

private func polishedWorkLabel(_ raw: String) -> String {
  let base = humanLabel(raw)
  let cased = base == base.lowercased()
    ? base.prefix(1).uppercased() + base.dropFirst()
    : base
  return cased
    .replacingOccurrences(of: " Ux", with: " UX")
    .replacingOccurrences(of: " Ui", with: " UI")
    .replacingOccurrences(of: " Api", with: " API")
    .replacingOccurrences(of: " Ci", with: " CI")
    .replacingOccurrences(of: " Spm", with: " SPM")
}

struct ForgeRunnerProfile: Equatable {
  let id: String
  let kind: String
  let displayName: String
  let adapterKind: String
  let executionMode: String
  let modelProfileID: String

  static let empty = ForgeRunnerProfile(
    id: "",
    kind: "",
    displayName: "",
    adapterKind: "",
    executionMode: "",
    modelProfileID: ""
  )

  var protocolLabel: String {
    switch adapterKind {
    case "grok_build_cli": return "ACP"
    case "claude_code": return "Agents SDK"
    case "codex_cli": return "CLI"
    case "mdx_native": return "Native"
    default:
      if kind == "mdx_native" { return "Native" }
      return adapterKind.isEmpty ? "" : humanLabel(adapterKind)
    }
  }
}

struct ForgeExecutionGeometry: Equatable {
  let requestedWorkers: Int
  let effectiveWorkers: Int
  let lane: String
  let route: String
  let reason: String
  let fleetRequired: Bool

  static let single = ForgeExecutionGeometry(
    requestedWorkers: 1,
    effectiveWorkers: 1,
    lane: "",
    route: "",
    reason: "",
    fleetRequired: false
  )

  var isWide: Bool { effectiveWorkers > 1 || requestedWorkers > 1 }

  var laneLabel: String {
    if isWide { return "\(effectiveWorkers) workers" }
    return roleLabel
  }

  var roleLabel: String {
    if lane.isEmpty { return "Single run" }
    return humanLabel(lane)
  }
}

struct ForgeParallelCandidate: Equatable {
  let role: String
  let primaryRunID: String
  let index: Int
  let count: Int
  let writeScope: [String]
  let strategyID: String
  let strategySummary: String
  let proofBias: String

  static let single = ForgeParallelCandidate(
    role: "",
    primaryRunID: "",
    index: 1,
    count: 1,
    writeScope: [],
    strategyID: "",
    strategySummary: "",
    proofBias: ""
  )

  var isParallel: Bool { count > 1 }

  var laneLabel: String {
    guard isParallel else { return "" }
    let base = role.isEmpty ? "candidate" : role
    return "\(humanLabel(base).capitalized) \(index)/\(count)"
  }
}

struct ForgeParallelExecutionGroup: Identifiable, Equatable {
  let primaryRunID: String
  let requestedWorkers: Int
  let effectiveWorkers: Int
  let lane: String
  let plannedCandidateCount: Int
  let observedCandidateCount: Int
  let finishedCandidateCount: Int
  let runningCandidateCount: Int
  let doneCandidateCount: Int
  let noChangeCandidateCount: Int
  let cannotProceedCandidateCount: Int
  let failedCandidateCount: Int
  let checksPassedTotal: Int
  let checksFailedTotal: Int
  let selectionStatus: String
  let recommendedRunID: String
  let recommendedStrategySummary: String
  let candidates: [ForgeParallelExecutionCandidate]

  var id: String { primaryRunID.isEmpty ? "parallel_group" : primaryRunID }

  var isWide: Bool { effectiveWorkers > 1 || requestedWorkers > 1 || observedCandidateCount > 1 }

  var isSettled: Bool {
    runningCandidateCount == 0
      && finishedCandidateCount > 0
      && finishedCandidateCount >= max(observedCandidateCount, plannedCandidateCount)
  }

  var laneLabel: String {
    if effectiveWorkers > 1 { return "\(effectiveWorkers) lanes" }
    if observedCandidateCount > 1 { return "\(observedCandidateCount) lanes" }
    return lane.isEmpty ? "Single lane" : humanLabel(lane)
  }

  var progressLine: String {
    if isSettled {
      return outcomeProgressLine
    }
    return liveProgressLine
  }

  private var liveProgressLine: String {
    var parts: [String] = []
    if finishedCandidateCount > 0 { parts.append("\(finishedCandidateCount) finished") }
    if runningCandidateCount > 0 { parts.append("\(runningCandidateCount) still working") }
    if failedCandidateCount > 0 { parts.append("\(failedCandidateCount) failed") }
    if cannotProceedCandidateCount > 0 { parts.append("\(cannotProceedCandidateCount) need a narrower step") }
    if noChangeCandidateCount > 0 { parts.append("\(noChangeCandidateCount) made no change") }
    if parts.isEmpty {
      let observed = max(observedCandidateCount, plannedCandidateCount)
      if observed > 0 { parts.append("\(observed) planned") }
    }
    return parts.joined(separator: ", ")
  }

  private var outcomeProgressLine: String {
    var parts: [String] = []
    if doneCandidateCount > 0 { parts.append("\(doneCandidateCount) done") }
    if failedCandidateCount > 0 { parts.append("\(failedCandidateCount) failed") }
    if cannotProceedCandidateCount > 0 { parts.append("\(cannotProceedCandidateCount) need a narrower step") }
    if noChangeCandidateCount > 0 { parts.append("\(noChangeCandidateCount) made no change") }
    if parts.isEmpty {
      parts.append("\(finishedCandidateCount) finished")
    }
    return parts.joined(separator: ", ")
  }

  var selectionLine: String {
    var parts: [String] = []
    if !selectionStatus.isEmpty {
      parts.append(selectionStatusLabel)
    }
    if !recommendedRunID.isEmpty {
      let label = candidates.first { $0.runID == recommendedRunID }?.laneLabel ?? recommendedRunID
      parts.append("recommended: \(label)")
    }
    if !recommendedStrategySummary.isEmpty {
      parts.append(recommendedStrategySummary)
    }
    return parts.joined(separator: " · ")
  }

  var selectionStatusLabel: String {
    switch selectionStatus.lowercased() {
    case "ready_for_review": return "Ready for review"
    case "waiting_for_candidates": return "Waiting for other lanes"
    case "selection_recorded": return "Selection recorded"
    case "no_recommendation": return "No recommendation yet"
    case "": return ""
    default: return humanLabel(selectionStatus)
    }
  }

  var completedSummaryLine: String {
    guard isSettled else { return "" }
    var parts: [String] = []
    if !progressLine.isEmpty {
      parts.append("\(laneLabel): \(progressLine)")
    }
    if !selectionLine.isEmpty {
      parts.append(selectionLine)
    }
    return parts.joined(separator: ". ")
  }

  func contains(runID: String) -> Bool {
    primaryRunID == runID || candidates.contains { $0.runID == runID }
  }

  func candidate(runID: String) -> ForgeParallelExecutionCandidate? {
    candidates.first { $0.runID == runID }
  }
}

struct ForgeParallelExecutionCandidate: Identifiable, Equatable {
  let runID: String
  let role: String
  let index: Int
  let count: Int
  let status: String
  let finished: Bool
  let strategySummary: String
  let checksPassed: Int
  let checksFailed: Int
  let turns: Int
  let branch: String

  var id: String { runID }

  var laneLabel: String {
    guard count > 1 else { return role.isEmpty ? "Lane" : humanLabel(role).capitalized }
    let base = role.isEmpty ? "candidate" : role
    return "\(humanLabel(base).capitalized) \(index)/\(count)"
  }

  var displayStatus: String {
    let s = status.lowercased()
    if s == "running" { return "Working" }
    if s.contains("cannot") { return "Needs a narrower step" }
    if s.contains("fail") { return "Failed" }
    if s.contains("no_change") || s.contains("no change") { return "No change" }
    if s.contains("done") { return "Done" }
    if finished { return "Finished" }
    return humanLabel(status)
  }
}

struct ForgeQuarantine: Equatable {
  let status: String
  let outputHeld: Bool
  let outputConsumable: Bool
  let acceptanceGate: String
  let blockedReason: String
  let acceptedForScoreboard: Bool
  let leagueRecommendation: String
  let quarantinePosture: String

  static let empty = ForgeQuarantine(
    status: "",
    outputHeld: false,
    outputConsumable: false,
    acceptanceGate: "",
    blockedReason: "",
    acceptedForScoreboard: false,
    leagueRecommendation: "",
    quarantinePosture: ""
  )

  var humanLine: String {
    if outputHeld {
      return "Held in an isolated copy until Forge checks accept it."
    }
    if acceptedForScoreboard {
      return "Accepted for the scorecard; output still stays held."
    }
    return ""
  }
}

struct ForgeContextTelemetry: Equatable {
  let latestInputTokens: Int
  let latestOutputTokens: Int
  let peakInputTokens: Int
  let contextWindow: Int
  let peakPct: Int
  let modelCount: Int

  static let empty = ForgeContextTelemetry(
    latestInputTokens: 0,
    latestOutputTokens: 0,
    peakInputTokens: 0,
    contextWindow: 0,
    peakPct: 0,
    modelCount: 0
  )

  var hasContext: Bool { contextWindow > 0 && (latestInputTokens > 0 || peakInputTokens > 0 || peakPct > 0) }

  var displayLabel: String {
    guard hasContext else { return "" }
    let pct = peakPct > 0 ? peakPct : max(1, min(100, Int((Double(max(latestInputTokens, peakInputTokens)) / Double(contextWindow)) * 100.0)))
    return modelCount > 1 ? "peak \(pct)%" : "\(pct)% context"
  }
}

struct ForgeLocalBaseSnapshot: Equatable {
  let trackedFileCount: Int
  let untrackedFileCount: Int
  let commitSha: String
  let includesUntracked: Bool
  let liveRepoMutated: Bool

  init?(event: ForgeEvent) {
    let text = [event.kind, event.summary, event.detail].joined(separator: " ")
    guard text.localizedCaseInsensitiveContains("local_dirty_base_snapshot") else { return nil }
    trackedFileCount = Self.intValue(after: "tracked_files=", in: text) ?? 0
    untrackedFileCount = Self.intValue(after: "untracked_files=", in: text) ?? 0
    commitSha = Self.stringValue(after: "commit_sha=", in: text) ?? ""
    includesUntracked = Self.boolValue(after: "untracked_included=", in: text) ?? false
    liveRepoMutated = Self.boolValue(after: "live_repo_mutated=", in: text) ?? false
  }

  var chipLabel: String {
    let total = trackedFileCount + untrackedFileCount
    guard total > 0 else { return "clean base" }
    return total == 1 ? "1 local file" : "\(total) local files"
  }

  var shortLine: String {
    if trackedFileCount + untrackedFileCount > 0 {
      return "Using \(chipLabel) from this Mac"
    }
    return "Using a clean local base"
  }

  var detailLine: String {
    var parts = [shortLine]
    if untrackedFileCount > 0 {
      let label = untrackedFileCount == 1 ? "1 untracked file included" : "\(untrackedFileCount) untracked files included"
      parts.append(label)
    } else {
      parts.append(includesUntracked ? "untracked files included" : "untracked files left out")
    }
    parts.append(liveRepoMutated ? "live checkout was touched" : "live checkout left alone")
    if !commitSha.isEmpty {
      parts.append("snapshot \(String(commitSha.prefix(7)))")
    }
    return parts.joined(separator: "; ") + "."
  }

  private static func intValue(after marker: String, in text: String) -> Int? {
    stringValue(after: marker, in: text).flatMap(Int.init)
  }

  private static func boolValue(after marker: String, in text: String) -> Bool? {
    guard let value = stringValue(after: marker, in: text)?.lowercased() else { return nil }
    if ["1", "true", "yes", "on"].contains(value) { return true }
    if ["0", "false", "no", "off"].contains(value) { return false }
    return nil
  }

  private static func stringValue(after marker: String, in text: String) -> String? {
    guard let range = text.range(of: marker) else { return nil }
    let tail = text[range.upperBound...]
    let value = tail.split { $0 == " " || $0 == "\n" || $0 == "\t" }.first.map(String.init) ?? ""
    return value.isEmpty ? nil : value
  }
}

struct ForgeStage: Identifiable, Equatable {
  let id: String
  let label: String
  let state: String // done / active / pending

  var isDone: Bool { state.localizedCaseInsensitiveContains("done") }
  var isActive: Bool { state.localizedCaseInsensitiveContains("active") || state.localizedCaseInsensitiveContains("running") }

  var isProofStage: Bool {
    let text = "\(id) \(label)".lowercased()
    return text.contains("proof") || text.contains("proving")
  }

  var isCompletionStage: Bool {
    let text = "\(id) \(label)".lowercased()
    return text.contains("done") || text.contains("complete") || text.contains("finish")
  }

  func needsAttention(in run: ForgeRun) -> Bool {
    guard run.hasProofTrouble else { return false }
    // A run that ended green does not flag its Proving/Done stages.
    if run.terminalStateIsPositive { return false }
    if isProofStage { return true }
    return !run.isRunning && isCompletionStage
  }
}

struct ForgeEvent: Identifiable, Equatable {
  let id: String
  let seq: Int
  let kind: String
  let stage: String
  let summary: String
  let detail: String
  let receiptID: String
  let receiptRoute: String
  let model: String

  var hasReceipt: Bool { !receiptID.isEmpty }

  var isStreamingToken: Bool {
    let normalized = kind.lowercased()
    return normalized == "token" || normalized.contains("_token") || normalized.contains("token_")
  }

  /// Joins provider token events into one stable activity row. The first id is
  /// preserved so SwiftUI updates the row instead of replacing it per token.
  static func coalescingStreamingTokens(_ existing: [ForgeEvent], appending fresh: [ForgeEvent]) -> [ForgeEvent] {
    var result = existing
    for event in fresh {
      if event.isStreamingToken, let last = result.last, last.isStreamingToken,
         last.stage == event.stage, last.model == event.model {
        result[result.count - 1] = last.mergingStreamingToken(event)
      } else {
        result.append(event)
      }
    }
    return result
  }

  private func mergingStreamingToken(_ next: ForgeEvent) -> ForgeEvent {
    let joinedSummary = Self.joinToken(summary, next.summary)
    let joinedDetail = Self.joinToken(detail, next.detail)
    return ForgeEvent(
      id: id,
      seq: seq,
      kind: kind,
      stage: stage,
      summary: joinedSummary,
      detail: joinedDetail,
      receiptID: receiptID,
      receiptRoute: receiptRoute,
      model: model
    )
  }

  private static func joinToken(_ left: String, _ right: String) -> String {
    guard !left.isEmpty else { return right }
    guard !right.isEmpty else { return left }
    if left.last?.isWhitespace == true || right.first?.isWhitespace == true { return left + right }
    let punctuation = CharacterSet.punctuationCharacters
    if right.unicodeScalars.first.map(punctuation.contains) == true { return left + right }
    if let last = left.last, "([{/'\"".contains(last) { return left + right }
    return left + " " + right
  }

  /// True when the summary reads as a raw key=value dump
  /// (accepted: 1 selected_checks language_pack=generic, phase=intake
  /// context_chars=5198) rather than a sentence.
  var summaryIsRaw: Bool {
    guard summary.contains("=") || summary.contains("_") else { return false }
    // A written sentence ends a clause with ". "; a dump does not.
    return !summary.contains(". ")
  }

  /// A bare terminal word ("Done") the kernel repeats across trailing NOTE
  /// rows; on its own it is noise, so we lead with the detail instead.
  private var summaryIsBareStatus: Bool {
    ["done", "finished", "complete", "ok", "note", "recorded"]
      .contains(summary.trimmingCharacters(in: .whitespaces).lowercased())
  }

  /// The phrase before the first ":" or "key=value" in a raw line, humanized,
  /// so consecutive rows read distinctly ("Repo quality signals applied").
  private static func leadPhrase(from raw: String) -> String {
    var head = raw
    if let colon = head.firstIndex(of: ":") { head = String(head[..<colon]) }
    if head.contains("=") {
      var kept: [Substring] = []
      for token in head.split(separator: " ") {
        if token.contains("=") { break }
        kept.append(token)
      }
      head = kept.joined(separator: " ")
    }
    head = head.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: "_", with: " ")
    guard head.count >= 3 else { return "" }
    // A single stray lowercase word ("evidence" grabbed from mid-line) is not a
    // sentence; let the caller fall back to a plain "Note" leader instead.
    let isSingleWord = !head.contains(" ")
    let startsLowercase = head.first?.isLowercase ?? false
    if isSingleWord && startsLowercase { return "" }
    return head.prefix(1).uppercased() + head.dropFirst()
  }

  /// The human line the trail leads with. The raw dump moves into the mono
  /// detail row (displayDetail), so nothing is lost.
  var displaySummary: String {
    if let count = approvedLessonCitationCount {
      return "Drew on \(count) lesson\(count == 1 ? "" : "s") you approved"
    }
    let needsHumanizing = summaryIsRaw || (summaryIsBareStatus && !detail.isEmpty)
    guard needsHumanizing else { return summary }

    let text = [kind, stage, summary, detail].joined(separator: " ").lowercased()
    if text.contains("proof") || text.contains("run_command")
      || text.contains("check_failed") || text.contains("check passed") {
      if text.contains("fail") || text.contains("exit=1") { return "Proof check failed" }
      if text.contains("accepted") || text.contains("passed") || text.contains("exit=0") { return "Proof check passed" }
      return "Proof check"
    }
    // A distinct, non-terminal stage is a good leader; a terminal "Done" is not,
    // because it would repeat on every trailing NOTE row.
    if !stage.isEmpty {
      let staged = StatusCopy.stage(stage)
      if !staged.isEmpty, staged != "Done" { return staged }
    }
    // Lead with the phrase before the first key=value, from whichever of the
    // summary or the detail carries the raw content.
    let lead = ForgeEvent.leadPhrase(from: summaryIsRaw ? summary : detail)
    if !lead.isEmpty { return lead }
    switch kind.uppercased() {
    case "STAGE": return "Stage update"
    case "NOTE": return "Note"
    case "PROOF": return "Proof check"
    default:
      let label = humanLabel(kind)
      return label.isEmpty ? "Update" : label
    }
  }

  var approvedLessonCitationCount: Int? {
    let source = [summary, detail].joined(separator: " ")
    guard source.localizedCaseInsensitiveContains("active learning memory cited") else { return nil }
    guard let range = source.range(of: #"advisory_count=(\d+)"#, options: .regularExpression) else {
      return 0
    }
    return Int(source[range].split(separator: "=").last ?? "0") ?? 0
  }

  var displayDetail: String {
    if summaryIsRaw {
      if detail.isEmpty || detail == summary { return summary }
      return summary + "\n" + detail
    }
    // Bare-status rows keep their raw detail below the humanized leader.
    return detail
  }

  var isProofFailure: Bool {
    let text = [kind, stage, summary, detail].joined(separator: " ").lowercased()
    return text.contains("check_failed")
      || text.contains("proof check failed")
      || text.contains("post-change proof")
      || text.contains("run_command") && text.contains("exit=1")
  }

  var isBaselineProofFailure: Bool {
    let text = [kind, stage, summary, detail].joined(separator: " ").lowercased()
    return text.contains("baseline_run_command")
      && (text.contains("check_failed") || text.contains("proof check failed") || text.contains("exit=1"))
  }

  var isPostChangeProofPass: Bool {
    let text = [kind, stage, summary, detail].joined(separator: " ").lowercased()
    guard !text.contains("baseline_run_command") else { return false }
    return text.contains("proof check passed")
      || text.contains("run_command") && text.contains("exit=0")
  }

  var conciseFailureText: String {
    let source = detail.isEmpty ? summary : detail
    let normalized = source
      .replacingOccurrences(of: "\n", with: " ")
      .replacingOccurrences(of: "\t", with: " ")
    let words = normalized.split(separator: " ")
    guard words.count > 34 else { return normalized }
    return words.prefix(34).joined(separator: " ") + "..."
  }

  var failingCommandLabel: String {
    let source = detail.isEmpty ? summary : detail
    for marker in ["baseline_run_command ", "run_command "] {
      if let range = source.range(of: marker) {
        let tail = source[range.upperBound...]
        let command = String(tail)
          .components(separatedBy: " exit=")
          .first
          .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) } ?? ""
        if !command.isEmpty { return "\(command) failed" }
      }
    }
    return conciseFailureText
  }
}

struct ForgeControl: Identifiable, Equatable {
  var id: String { action }
  let action: String // steer / stop / revise / review / ship
  let allowed: Bool
  let route: String
}

/// How the diff renders: single column with +/- markers, or side-by-side.
enum DiffViewMode: String, CaseIterable, Identifiable {
  case unified
  case split

  var id: String { rawValue }

  var label: String {
    switch self {
    case .unified: return "Unified"
    case .split: return "Split"
    }
  }
}

/// One rendered diff line, classified once at load time so the viewer never
/// re-parses the patch during scrolling or unrelated re-renders.
struct DiffLine: Identifiable, Equatable {
  enum Kind: Equatable {
    case addition, deletion, hunk, meta, context
  }

  let id: Int
  let text: String
  let kind: Kind
  /// Intraline emphasis: the character range (UTF-16 offsets into `text`)
  /// that actually changed within a paired -/+ line. Computed once at parse.
  var emphasis: Range<Int>? = nil
  /// Whole-line comment (context lines only): rendered quieter.
  var isComment: Bool = false
  /// Source line numbers, tracked from the hunk headers at parse time so the
  /// gutter can show a real old/new reference. A deletion has only an old
  /// number, an addition only a new number, context both, hunk/meta neither.
  var oldLine: Int? = nil
  var newLine: Int? = nil

  static func parse(_ patch: String) -> [DiffLine] {
    var oldCursor = 0
    var newCursor = 0
    var lines: [DiffLine] = patch.split(separator: "\n", omittingEmptySubsequences: false).enumerated().map { index, raw in
      let line = String(raw)
      let kind: Kind
      if line.hasPrefix("+++") || line.hasPrefix("---") || line.hasPrefix("diff ") || line.hasPrefix("index ") {
        kind = .meta
      } else if line.hasPrefix("+") {
        kind = .addition
      } else if line.hasPrefix("-") {
        kind = .deletion
      } else if line.hasPrefix("@@") {
        kind = .hunk
      } else {
        kind = .context
      }
      let trimmed = line.dropFirst(kind == .context && !line.isEmpty ? 1 : 0).trimmingCharacters(in: .whitespaces)
      let isComment = kind == .context && (trimmed.hasPrefix("//") || trimmed.hasPrefix("#") || trimmed.hasPrefix("/*") || trimmed.hasPrefix("*"))
      var old: Int? = nil
      var new: Int? = nil
      switch kind {
      case .hunk:
        if let starts = hunkStarts(line) { oldCursor = starts.old; newCursor = starts.new }
      case .context:
        old = oldCursor; new = newCursor
        oldCursor += 1; newCursor += 1
      case .deletion:
        old = oldCursor; oldCursor += 1
      case .addition:
        new = newCursor; newCursor += 1
      case .meta:
        break
      }
      return DiffLine(id: index, text: line, kind: kind, isComment: isComment, oldLine: old, newLine: new)
    }
    applyIntralineEmphasis(&lines)
    return lines
  }

  /// Parse the starting line numbers out of a `@@ -old,count +new,count @@`
  /// hunk header. Tolerant of the single-line `@@ -old +new @@` form.
  static func hunkStarts(_ header: String) -> (old: Int, new: Int)? {
    let scanner = header.drop { $0 != "-" }
    guard scanner.first == "-" else { return nil }
    let body = scanner.dropFirst()
    let parts = body.split(separator: " ")
    guard parts.count >= 2 else { return nil }
    func start(_ token: Substring) -> Int? {
      let stripped = token.hasPrefix("+") ? token.dropFirst() : token[...]
      let number = stripped.prefix { $0.isNumber }
      return Int(number)
    }
    guard let old = start(parts[0]), let new = start(parts[1]) else { return nil }
    return (old, new)
  }

  /// For every 1:1 paired deletion/addition run, mark the differing middle
  /// (common prefix and suffix stripped) so the eye lands on what changed.
  static func applyIntralineEmphasis(_ lines: inout [DiffLine]) {
    var index = 0
    while index < lines.count {
      guard lines[index].kind == .deletion else { index += 1; continue }
      var deletions: [Int] = []
      var cursor = index
      while cursor < lines.count, lines[cursor].kind == .deletion {
        deletions.append(cursor)
        cursor += 1
      }
      var additions: [Int] = []
      while cursor < lines.count, lines[cursor].kind == .addition {
        additions.append(cursor)
        cursor += 1
      }
      for pair in 0..<min(deletions.count, additions.count) {
        let delIndex = deletions[pair]
        let addIndex = additions[pair]
        // Strip the -/+ marker for comparison; offsets are into the full text.
        let old = Array(lines[delIndex].text.utf16.dropFirst())
        let new = Array(lines[addIndex].text.utf16.dropFirst())
        var prefix = 0
        while prefix < old.count, prefix < new.count, old[prefix] == new[prefix] { prefix += 1 }
        var suffix = 0
        while suffix < old.count - prefix, suffix < new.count - prefix,
              old[old.count - 1 - suffix] == new[new.count - 1 - suffix] { suffix += 1 }
        if prefix > 0 || suffix > 0 {
          if old.count - suffix > prefix {
            lines[delIndex].emphasis = (prefix + 1)..<(old.count - suffix + 1)
          }
          if new.count - suffix > prefix {
            lines[addIndex].emphasis = (prefix + 1)..<(new.count - suffix + 1)
          }
        }
      }
      index = max(cursor, index + 1)
    }
  }
}

/// One row of the side-by-side view. `full` carries hunk/meta lines that span
/// both columns; otherwise left is old, right is new.
struct DiffSplitRow: Identifiable, Equatable {
  let id: Int
  var left: DiffLine? = nil
  var right: DiffLine? = nil
  var full: DiffLine? = nil

  /// Pair deletions with the additions that replaced them; context flows to
  /// both sides. Computed once when the diff loads.
  static func build(from lines: [DiffLine]) -> [DiffSplitRow] {
    var rows: [DiffSplitRow] = []
    var index = 0
    var rowID = 0
    while index < lines.count {
      let line = lines[index]
      switch line.kind {
      case .meta, .hunk:
        rows.append(DiffSplitRow(id: rowID, full: line)); rowID += 1
        index += 1
      case .context:
        rows.append(DiffSplitRow(id: rowID, left: line, right: line)); rowID += 1
        index += 1
      case .deletion:
        var deletions: [DiffLine] = []
        while index < lines.count, lines[index].kind == .deletion {
          deletions.append(lines[index]); index += 1
        }
        var additions: [DiffLine] = []
        while index < lines.count, lines[index].kind == .addition {
          additions.append(lines[index]); index += 1
        }
        for pair in 0..<max(deletions.count, additions.count) {
          rows.append(DiffSplitRow(
            id: rowID,
            left: pair < deletions.count ? deletions[pair] : nil,
            right: pair < additions.count ? additions[pair] : nil
          ))
          rowID += 1
        }
      case .addition:
        rows.append(DiffSplitRow(id: rowID, right: line)); rowID += 1
        index += 1
      }
    }
    return rows
  }
}

struct DiffFile: Identifiable, Equatable {
  var id: String { path }
  let path: String
  let added: Int
  let removed: Int
  let patch: String
  let lines: [DiffLine]
  let splitRows: [DiffSplitRow]
  // Kernel-computed proof signals (zero/empty when the kernel predates them).
  var trailStepCount: Int = 0
  var errorStepCount: Int = 0
  var retryCount: Int = 0
  var checksTouching: [String] = []
  /// needs_attention | checked | mentioned | unscored
  var agentConfidence: String = ""

  init(path: String, added: Int, removed: Int, patch: String) {
    self.path = path
    self.added = added
    self.removed = removed
    self.patch = patch
    self.lines = DiffLine.parse(patch)
    self.splitRows = DiffSplitRow.build(from: self.lines)
  }
}

struct ForgeRepo: Identifiable, Equatable {
  let id: String
  let label: String
  let kind: String
  let originURL: String
  var primaryLanguage: String = ""
  var suggestedCheckCommands: [String] = []
  var proofPlanStatus: String = ""
  var proofPlanNextAction: String = ""
  /// Local checkout path, when the kernel exposes it: the door to Finder,
  /// Terminal, and editors.
  var root: String = ""

  /// origin_url normalized to a browsable https URL (git@/.git stripped).
  var webOrigin: String? {
    guard !originURL.isEmpty else { return nil }
    var value = originURL
    if value.hasSuffix(".git") { value = String(value.dropLast(4)) }
    if value.hasPrefix("git@") {
      value = value.replacingOccurrences(of: ":", with: "/")
      value = value.replacingOccurrences(of: "git@", with: "https://")
    }
    return value.hasPrefix("http") ? value : nil
  }
}

struct ScoutCandidate: Identifiable, Equatable {
  var id: String { taskID }
  let taskID: String
  let title: String
  let why: String
  let path: String
  let line: Int
  let complexity: String
  let taskClass: String
  let recommendedShape: String
  let promptTemplate: String

  var location: String {
    line > 0 ? "\(path):\(line)" : path
  }

  var intent: String {
    promptTemplate.isEmpty ? title : promptTemplate
  }
}

struct ScoutResult: Equatable {
  let repoID: String
  let status: String
  let safeNextMove: String
  let filesScanned: Int
  let primaryLanguage: String
  let candidates: [ScoutCandidate]

  var foundTasks: Bool { !candidates.isEmpty }
}

struct WorkRecommendation: Equatable {
  let shape: String
  let width: Int
  let label: String
  let reason: String
  let taskClass: String
  let complexityTier: String
  let confidencePct: Int
  let rationale: String
  let suggestedCheckCommands: [String]
  let allowedWriteScope: String
  let blockedPaths: String

  var isMission: Bool { shape.lowercased() == "mission" }

  var suggestedChecks: String {
    suggestedCheckCommands.joined(separator: ", ")
  }

  var cta: String {
    switch shape.lowercased() {
    case "mission": return "Looks right, set it up"
    default: return width > 4 ? "Looks right, prepare the plan" : "Looks right, start"
    }
  }

  var readsAs: String {
    let tier = complexityTier.isEmpty ? "" : complexityTier
    let cls = taskClass.replacingOccurrences(of: "_", with: " ")
    let parts = [tier, cls].filter { !$0.isEmpty }
    return parts.isEmpty ? "" : "Reads as a " + parts.joined(separator: " ")
  }
}

struct ReviewPacket: Equatable {
  let runID: String
  let reviewStatus: String
  let nextMove: String
  let checksPassed: Int
  let checksFailed: Int
  let checkNames: [String]
  let behaviorStatus: String
  let behaviorSummary: String
  let satisfiedChecks: [String]
  let missingChecks: [String]
  let principalChecklist: [String]
  let handoffLines: [String]
  let shipDecided: Bool
  let shipReason: String
}

struct MissionDraft: Equatable {
  var goal = ""
  var doneWhen = "The checks pass and every step has its receipt."
  var validationCommands = "make local-smoke"
  var allowedWriteScope = ""
  var blockedPaths = ""
  var nonGoals = "Do not call live providers, run outside tools, or write to production."
  var recommendationLabel = ""
  var recommendationReason = ""
  var recommendationReadsAs = ""
  var fleetWidth = 3
  var maxCostDollars = 10
  var maxRuntimeHours = 4
  var checkpointCadenceMinutes = 30
}

struct RunActionOutcome: Equatable {
  let title: String
  let status: String
  let detail: String
  let receiptID: String

  /// The ONE place kernel status strings become semantics. Kernel statuses
  /// are compound (PAGE_PUBLISHED_EDITOR_BLOCKED means published), so this
  /// classifies rather than string-matches at every call site.
  enum Classification: Equatable {
    case succeeded
    case refused
    case failed
  }

  var classification: Classification {
    if status.localizedCaseInsensitiveContains("REFUSED") || status.localizedCaseInsensitiveContains("DENIED") {
      return .refused
    }
    if status.localizedCaseInsensitiveContains("FAILED") {
      return .failed
    }
    return .succeeded
  }

  var succeeded: Bool { classification == .succeeded }

  var isRefusal: Bool { classification != .succeeded }

  /// The human words for this outcome's pill. The raw kernel code stays
  /// reachable in the banner's copyable receipt row.
  var displayStatus: String { StatusCopy.human(status) }
}

/// The one place raw kernel status codes turn into words a person reads.
/// Every visible status pill, header, and banner flows through here so no
/// SCREAMING_SNAKE or route-shaped enum reaches a first read. The raw code
/// stays reachable in a Details or receipt row next to wherever this is shown.
enum StatusCopy {
  static func human(_ raw: String) -> String {
    let s = raw.lowercased()
    if s.isEmpty { return "" }
    // Reconciled admission-class writes (Arc 1): the action landed and the
    // record confirms it after a slow-but-successful kernel reply.
    if s.contains("run_started") && s.contains("reconciled") {
      return "Started and confirmed on the record"
    }
    if s.contains("ship_decision_recorded") && s.contains("reconciled") {
      return "Ship decision recorded and confirmed"
    }
    if s.contains("ship_decision_recorded") { return "Ship decision recorded" }
    if s.contains("face_off") && s.contains("staged") { return "Comparison staged and held" }
    // Refusals and denials: warm, honest, never the raw enum.
    if s.contains("refused") || s.contains("denied") { return "Held" }
    // Missing-receipt failures fail closed: the action may not have landed.
    if s.contains("missing_receipt") || (s.contains("failed") && s.contains("receipt")) {
      return "Not confirmed"
    }
    if s.contains("failed") { return "Did not go through" }
    if s.contains("run_started") || s.contains("started") { return "Started" }
    if s == "running" || s.contains("in_progress") { return "Working" }
    if s.contains("cannot") { return "Needs a narrower next step" }
    if s.contains("exhausted") { return "Out of turns" }
    if s.contains("no_change") || s.contains("no change") { return "No change" }
    if s.contains("done") || s.contains("finished") || s.contains("complete") { return "Done" }
    if s.contains("recorded") { return "Recorded" }
    if s.contains("ready") { return "Ready" }
    if s == "idle" { return "Idle" }
    if s == "unknown" { return "" }
    // Title-case a lone SCREAMING token so no raw caps enum reaches a read.
    if raw == raw.uppercased() {
      return raw.lowercased()
        .split(whereSeparator: { $0 == "_" || $0 == "-" || $0 == " " })
        .map { $0.prefix(1).uppercased() + $0.dropFirst() }
        .joined(separator: " ")
    }
    return humanLabel(raw)
  }

  /// Known run stages in the words the run detail and inspector should show.
  static func stage(_ raw: String) -> String {
    switch raw.lowercased() {
    case "": return ""
    case "ready_for_review": return "Ready for review"
    case "intake": return "Reading the ask"
    case "planning": return "Planning"
    case "proving", "proof": return "Proving"
    case "shipping": return "Shipping"
    case "done", "finished", "complete": return "Done"
    default: return humanLabel(raw)
    }
  }
}


/// The kernel-composed PR handoff: receipt-backed narrative, review
/// checklist, and honest delivery authority flags.
struct PRHandoff: Equatable {
  let runID: String
  let title: String
  let bodyMarkdown: String
  let branch: String
  let receiptID: String
  let reviewStatus: String
  let checklist: [String]
  let remotePushAllowed: Bool
  let pullRequestOpenAllowed: Bool
  let blockedReasons: [String]
  // The kernel's PR-open authority contract: where live delivery happens
  // once an operator explicitly acts, and what stays closed until then.
  var authorityStatus: String = ""
  var liveDeliveryRoute: String = ""
  var readinessRoute: String = ""
  var requiresOperatorAction: Bool = true
}

/// GET /version.json: the kernel introduces itself, so drift stops being a
/// guess.
struct KernelVersion: Equatable {
  let kernelVersion: String
  let contractFingerprint: String
  let minAppVersion: String
  let routeCount: Int

  /// Simple numeric-dotted comparison; both sides control their formats.
  func appIsTooOld(appVersion: String) -> Bool {
    func parts(_ value: String) -> [Int] {
      value.split(separator: " ").first.map(String.init).unwrapOrEmpty()
        .split(separator: ".").compactMap { Int($0) }
    }
    let app = parts(appVersion)
    let min = parts(minAppVersion)
    for index in 0..<max(app.count, min.count) {
      let a = index < app.count ? app[index] : 0
      let m = index < min.count ? min[index] : 0
      if a != m { return a < m }
    }
    return false
  }
}

private extension Optional where Wrapped == String {
  func unwrapOrEmpty() -> String { self ?? "" }
}
