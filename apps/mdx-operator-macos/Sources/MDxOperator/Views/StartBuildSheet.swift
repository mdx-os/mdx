import SwiftUI

struct StartBuildSheet: View {
  @Environment(OperatorStore.self) private var store
  @Environment(\.dismiss) private var dismiss

  @State private var intent = ""
  @State private var fleetWidth = 1
  @State private var proofCommands = ""
  @State private var proofCommandsTouched = false
  @State private var recommendation: WorkRecommendation?
  @State private var classificationError: String?
  @State private var classifying = false
  @State private var activeClassificationKey = ""
  @State private var lastClassificationKey = ""
  @State private var adaptingLaunch = false
  @State private var showAdvanced = false
  @FocusState private var intentFocused: Bool
  @State private var showURLPrompt = false
  @State private var repoURLInput = ""

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 18) {
        header
        repoPicker
        executionLocationDisclosure
        suggestions
        Divider()
        intentField
        recommendationCard
        classificationFailureCard
        proofControl
        widthControl
        if !repoReady {
          runtimeWorkspaceCallout
        }
        if directWorkerLimitExceeded {
          directWorkerLimitCallout
        }
        if launchInFlight {
          launchInFlightCallout
        }
        if let outcome = store.runActionResult, (outcome.title == "Start run" || outcome.title == "Plan fleet build"), outcome.isRefusal {
          RunActionBanner(outcome: outcome)
        }
        footer
      }
      .padding(22)
    }
    .frame(minWidth: 560, idealWidth: 660, minHeight: 540, idealHeight: 720)
    .alert("Connect a repo from a Git URL", isPresented: $showURLPrompt) {
      TextField("https://github.com/org/repo", text: $repoURLInput)
      Button("Connect") {
        store.connectRepoFromURL(repoURLInput)
        repoURLInput = ""
      }
      Button("Cancel", role: .cancel) { repoURLInput = "" }
    } message: {
      Text("MDx clones and manages the checkout locally, then profiles it for Forge. GitHub and Bitbucket URLs work.")
    }
    .onAppear {
      // The palette can dispatch straight into this sheet with the ask typed.
      if let pending = store.pendingBuildIntent {
        intent = pending
        store.pendingBuildIntent = nil
      }
      applyShotProofCommandsIfRequested()
      applyShotFleetWidthIfRequested()
      intentFocused = true
    }
    .task {
      await store.loadRepos()
      if let repoID = ProcessInfo.processInfo.environment["MDX_SHOT_REPO_ID"],
         store.repos.contains(where: { $0.id == repoID }) {
        store.selectedRepoID = repoID
      }
      seedProofCommandsFromSelectedRepo()
      applyShotProofCommandsIfRequested()
      await classifyCurrentIntent()
      await scoutIfUseful()
    }
    .onChange(of: store.selectedRepoID) { _, _ in
      recommendation = nil
      classificationError = nil
      proofCommands = ""
      proofCommandsTouched = false
      seedProofCommandsFromSelectedRepo()
      applyShotProofCommandsIfRequested()
      Task {
        await classifyCurrentIntent()
        await scoutIfUseful()
      }
    }
    .task(id: intent) {
      try? await Task.sleep(nanoseconds: 700_000_000)
      if Task.isCancelled { return }
      await classifyCurrentIntent()
    }
  }

  private func classifyCurrentIntent() async {
    let ask = intent.trimmingCharacters(in: .whitespacesAndNewlines)
    guard ask.count >= 8 else {
      recommendation = nil
      classificationError = nil
      classifying = false
      return
    }
    guard repoReady else {
      recommendation = nil
      classificationError = nil
      classifying = false
      return
    }
    let key = "\(store.selectedRepoID)|\(ask)"
    if classifying && activeClassificationKey == key { return }
    if lastClassificationKey == key, recommendation != nil { return }
    activeClassificationKey = key
    classifying = true
    let rec = await store.classify(ask: ask)
    guard intent.trimmingCharacters(in: .whitespacesAndNewlines) == ask, activeClassificationKey == key else { return }
    classifying = false
    activeClassificationKey = ""
    lastClassificationKey = key
    recommendation = rec
    classificationError = rec == nil ? "Forge could not classify this ask before the local request timed out." : nil
    if let rec, !proofCommandsTouched, proofCommands.isEmpty, !isExternalRepo {
      proofCommands = rec.suggestedCheckCommands.joined(separator: "\n")
    }
    if let rec, !showAdvanced { fleetWidth = max(1, rec.width) }
    applyShotFleetWidthIfRequested()
  }

  private func scoutIfUseful() async {
    guard intent.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
    await store.scoutSelectedRepo()
  }

  private var header: some View {
    VStack(alignment: .leading, spacing: 5) {
      Text("Start a build")
        .font(.title2.weight(.semibold))
      Text("Tell Forge what you want in plain language. Forge starts the run and you watch it work, with proof at every step.")
        .font(.callout)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
    }
  }

  private var repoPicker: some View {
    HStack(spacing: 10) {
      Text("Repo")
        .font(.caption.weight(.medium))
        .foregroundStyle(.secondary)
      Menu {
        ForEach(store.repos) { repo in
          Button(repo.label) { store.selectedRepoID = repo.id }
        }
        Divider()
        Button("Connect a repo on this Mac…") { store.connectRepoFromPanel() }
        Button("Connect from a Git URL…") { showURLPrompt = true }
      } label: {
        HStack(spacing: 6) {
          Image(systemName: "folder")
          Text(selectedRepoLabel)
            .lineLimit(1)
          Image(systemName: "chevron.up.chevron.down")
            .font(.caption2)
            .foregroundStyle(.secondary)
        }
      }
      .menuStyle(.borderlessButton)
      .fixedSize()
      Spacer()
      if let scout = store.scout, scout.filesScanned > 0 {
        Text(repoContext(scout))
          .font(.caption)
          .foregroundStyle(.tertiary)
      }
    }
  }

  private var repoReady: Bool {
    !store.selectedRepoID.isEmpty && store.repos.contains { $0.id == store.selectedRepoID }
  }

  private var executionLocation: ForgeExecutionLocation {
    Self.executionLocation(for: OperatorStore.configuredBaseURL())
  }

  static func executionLocation(for baseURL: URL) -> ForgeExecutionLocation {
    MDxRouteClient.isTrustedLocalBaseURL(baseURL) ? .thisMac : .cloudProfileRequired
  }

  private var executionLocationDisclosure: some View {
    HStack(alignment: .top, spacing: 10) {
      Image(systemName: executionLocation.systemImage)
        .foregroundStyle(executionLocation.canStart ? Color.accentColor : .orange)
        .frame(width: 18)
      VStack(alignment: .leading, spacing: 3) {
        Text(executionLocation.title)
          .font(.callout.weight(.semibold))
        Text(executionLocation.detail)
          .font(.caption)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
      Spacer(minLength: 0)
    }
    .padding(12)
    .background(
      RoundedRectangle(cornerRadius: 10, style: .continuous)
        .fill((executionLocation.canStart ? Color.accentColor : Color.orange).opacity(0.08))
    )
  }

  private var directWorkersAvailable: Int {
    Self.availableDirectWorkers(
      capacityWorkers: store.snapshot.capacityWorkers,
      activeWorkers: store.snapshot.capacityActiveWorkers
    )
  }

  static func availableDirectWorkers(capacityWorkers: Int, activeWorkers: Int) -> Int {
    min(4, max(0, capacityWorkers - activeWorkers))
  }

  private var directWorkerLimitExceeded: Bool {
    directWorkerLimitExceeded(for: fleetWidth)
  }

  private func directWorkerLimitExceeded(for width: Int) -> Bool {
    width <= 4 && width > directWorkersAvailable
  }

  private var runtimeWorkspaceCallout: some View {
    HStack(alignment: .center, spacing: 10) {
      Image(systemName: "macwindow")
        .foregroundStyle(Color.accentColor)
      Text("Forge will use the local MDx runtime workspace. Connect a repo when you want this run aimed at another checkout.")
        .font(.callout)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
      Spacer(minLength: 8)
      Button("Connect") { store.connectRepoFromPanel() }
        .controlSize(.small)
    }
    .padding(12)
    .background(
      RoundedRectangle(cornerRadius: 10, style: .continuous)
        .fill(Color.accentColor.opacity(0.08))
    )
  }

  private var directWorkerLimitCallout: some View {
    HStack(alignment: .center, spacing: 10) {
      Image(systemName: "person.2.badge.gearshape")
        .foregroundStyle(.orange)
      Text("Direct runs have \(directWorkersAvailable) worker\(directWorkersAvailable == 1 ? "" : "s") free. Choose a smaller direct run or move above 4 workers to prepare a governed fleet plan.")
        .font(.callout)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
      Spacer(minLength: 0)
    }
    .padding(12)
    .background(
      RoundedRectangle(cornerRadius: 10, style: .continuous)
        .fill(Color.orange.opacity(0.08))
    )
  }

  private func repoContext(_ scout: ScoutResult) -> String {
    let language = scout.primaryLanguage.isEmpty ? "" : scout.primaryLanguage
    let files = "\(scout.filesScanned) files scanned"
    return [language, files].filter { !$0.isEmpty }.joined(separator: " · ")
  }

  @ViewBuilder
  private var suggestions: some View {
    switch store.scoutPhase {
    case .loading:
      HStack(spacing: 8) {
        ProgressView().controlSize(.small)
        Text("Looking for good first tasks in \(selectedRepoLabel)…")
          .font(.callout)
          .foregroundStyle(.secondary)
      }
    case .loaded:
      if let scout = store.scout, scout.foundTasks {
        VStack(alignment: .leading, spacing: 8) {
          Text("Ideas Forge found in this repo")
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
          ForEach(scout.candidates.prefix(3)) { candidate in
            ScoutCandidateCard(candidate: candidate) { pick(candidate) }
          }
        }
      } else {
        Text("No ready-made suggestions here yet. Describe what you want below.")
          .font(.callout)
          .foregroundStyle(.secondary)
      }
    default:
      EmptyView()
    }
  }

  private var intentField: some View {
    VStack(alignment: .leading, spacing: 6) {
      Text("What should Forge build or fix?")
        .font(.caption.weight(.medium))
        .foregroundStyle(.secondary)
      TextField(placeholder, text: $intent, axis: .vertical)
        .textFieldStyle(.roundedBorder)
        .lineLimit(3...6)
        .focused($intentFocused)
    }
  }

  @ViewBuilder
  private var recommendationCard: some View {
    if classifying {
      HStack(spacing: 8) {
        ProgressView().controlSize(.small)
        Text("Reading your ask…")
          .font(.callout)
          .foregroundStyle(.secondary)
      }
    } else if let rec = recommendation {
      VStack(alignment: .leading, spacing: 8) {
        HStack(alignment: .firstTextBaseline) {
          Label("Forge suggests: \(rec.label)", systemImage: "wand.and.stars")
            .font(.callout.weight(.semibold))
          Spacer()
          if rec.confidencePct > 0 {
            Text("\(rec.confidencePct)% sure")
              .font(.caption)
              .foregroundStyle(.secondary)
          }
        }
        if !rec.reason.isEmpty {
          Text(rec.reason)
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
        if !rec.readsAs.isEmpty {
          Text(rec.readsAs)
            .font(.caption)
            .foregroundStyle(.tertiary)
        }
      }
      .padding(14)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(
        RoundedRectangle(cornerRadius: 10, style: .continuous)
          .fill(Color.accentColor.opacity(0.08))
      )
      .overlay(
        RoundedRectangle(cornerRadius: 10, style: .continuous)
          .stroke(Color.accentColor.opacity(0.22), lineWidth: 1)
      )
    }
  }

  @ViewBuilder
  private var classificationFailureCard: some View {
    if let classificationError, recommendation == nil {
      HStack(alignment: .top, spacing: 10) {
        Image(systemName: "exclamationmark.triangle")
          .foregroundStyle(.orange)
        VStack(alignment: .leading, spacing: 6) {
          Text("Forge did not answer yet")
            .font(.callout.weight(.semibold))
          Text("\(classificationError) Try again, or pick how many workers below and start the run.")
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
          Button {
            Task { await classifyCurrentIntent() }
          } label: {
            Label(classifying ? "Trying again" : "Try again", systemImage: "arrow.clockwise")
          }
          .controlSize(.small)
          .disabled(classifying)
        }
        Spacer(minLength: 0)
      }
      .padding(12)
      .background(
        RoundedRectangle(cornerRadius: 10, style: .continuous)
          .fill(Color.orange.opacity(0.08))
      )
    }
  }

  @ViewBuilder
  private var widthControl: some View {
    if let recommendation = recommendation ?? fallbackRecommendation {
      VStack(alignment: .leading, spacing: 8) {
        HStack(alignment: .firstTextBaseline) {
          Text("Workers")
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
          Spacer()
          Text(widthSummary(for: fleetWidth, recommendation: recommendation))
            .font(.caption)
            .foregroundStyle(.tertiary)
        }
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 122), spacing: 8)], spacing: 8) {
          ForEach(widthChoices(for: recommendation)) { choice in
            WidthChoiceButton(
              choice: choice,
              selected: choice.isAuto ? !showAdvanced : (showAdvanced && fleetWidth == choice.width)
            ) {
              fleetWidth = choice.width
              showAdvanced = !choice.isAuto
            }
          }
        }
      }
    }
  }

  private var proofControl: some View {
    VStack(alignment: .leading, spacing: 7) {
      HStack(alignment: .firstTextBaseline) {
        Text("Checks that prove it")
          .font(.caption.weight(.semibold))
          .foregroundStyle(.secondary)
        Spacer()
        Text(proofCommandList.isEmpty ? "Needed when Forge cannot infer one" : "\(proofCommandList.count) selected")
          .font(.caption)
          .foregroundStyle(proofCommandList.isEmpty ? Color.orange : Color.secondary)
      }
      TextField(
        "One command per line, for example: test -f README.md",
        text: Binding(
          get: { proofCommands },
          set: {
            proofCommands = $0
            proofCommandsTouched = true
          }
        ),
        axis: .vertical
      )
      .textFieldStyle(.roundedBorder)
      .lineLimit(1...3)
      Text(proofCommandList.isEmpty
        ? proofGuidance
        : "Forge runs these inside the isolated worktree and keeps the result with the review evidence.")
        .font(.caption)
        .foregroundStyle(.tertiary)
        .fixedSize(horizontal: false, vertical: true)
    }
  }

  private var footer: some View {
    HStack(spacing: 10) {
      Spacer()
      Button("Cancel") {
        store.cancelBuildStart()
        dismiss()
      }
        .keyboardShortcut(.cancelAction)
      Button {
        startBuild()
      } label: {
        if launchInFlight {
          HStack(spacing: 6) {
            ProgressView()
              .controlSize(.small)
            Text(startLabel)
          }
        } else {
          Label(startLabel, systemImage: startIcon)
        }
      }
      .mdxPrimaryButtonStyle()
      .keyboardShortcut(.defaultAction)
      .disabled(store.buildStartInFlight || adaptingLaunch || intent.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || directWorkerLimitExceeded || !executionLocation.canStart)
    }
  }

  private func startBuild() {
    Task {
      let ask = intent.trimmingCharacters(in: .whitespacesAndNewlines)
      guard !ask.isEmpty else { return }
      applyShotFleetWidthIfRequested()
      // START MEANS START: launch with the recommendation the user already saw.
      // Only classify at launch when none exists yet, so a late classification
      // can never flip the run shape or fleet width after the click.
      var launchRecommendation = recommendation
      if launchRecommendation == nil {
        adaptingLaunch = true
        launchRecommendation = await store.classify(ask: ask)
        adaptingLaunch = false
      }
      if let launchRecommendation {
        recommendation = launchRecommendation
        if launchRecommendation.isMission && !showAdvanced {
          setupMission()
          return
        }
        if !showAdvanced {
          fleetWidth = max(1, launchRecommendation.width)
        }
      }
      guard executionLocation.canStart else { return }
      guard !directWorkerLimitExceeded(for: fleetWidth) else { return }
      let outcome = await store.startRun(
        intent: ask,
        repoID: store.selectedRepoID,
        fleetWidth: fleetWidth,
        allowedCommands: proofCommandList
      )
      if outcome?.isRefusal == false { dismiss() }
    }
  }

  private func applyShotFleetWidthIfRequested() {
    guard let width = shotFleetWidth else { return }
    fleetWidth = width
    showAdvanced = true
  }

  private func applyShotProofCommandsIfRequested() {
    guard let commands = ProcessInfo.processInfo.environment["MDX_SHOT_PROOF_COMMANDS"],
          !commands.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    else { return }
    proofCommands = commands.replacingOccurrences(of: "\\n", with: "\n")
    proofCommandsTouched = true
  }

  private func seedProofCommandsFromSelectedRepo() {
    guard !proofCommandsTouched, proofCommands.isEmpty,
          let repo = store.repos.first(where: { $0.id == store.selectedRepoID }),
          !repo.suggestedCheckCommands.isEmpty
    else { return }
    proofCommands = repo.suggestedCheckCommands.joined(separator: "\n")
  }

  private var proofGuidance: String {
    if let repo = store.repos.first(where: { $0.id == store.selectedRepoID }),
       !repo.proofPlanNextAction.isEmpty {
      return repo.proofPlanNextAction
    }
    return "Forge may infer a stack-native check. If it cannot, add the command you trust here instead of leaving the build at a dead end."
  }

  private var shotFleetWidth: Int? {
    let raw = ProcessInfo.processInfo.environment["MDX_SHOT_FLEET_WIDTH"] ?? ""
    guard let value = Int(raw), value > 0 else { return nil }
    return min(max(value, 1), 24)
  }

  private var proofCommandList: [String] {
    Self.proofCommands(from: proofCommands)
  }

  static func proofCommands(from input: String) -> [String] {
    ScreenshotBuildRequest.proofCommands(from: input)
  }

  private func setupMission() {
    guard let rec = recommendation else { return }
    var draft = MissionDraft()
    draft.goal = intent
    draft.doneWhen = "The checks pass and every step has its receipt."
    draft.validationCommands = rec.suggestedChecks.isEmpty ? "make local-smoke" : rec.suggestedChecks
    draft.allowedWriteScope = rec.allowedWriteScope
    draft.blockedPaths = rec.blockedPaths
    draft.recommendationLabel = rec.label
    draft.recommendationReason = rec.reason
    draft.recommendationReadsAs = rec.readsAs
    draft.fleetWidth = max(1, showAdvanced ? fleetWidth : rec.width)
    store.missionSetupDraft = draft
    store.missionActionResult = nil
    dismiss()
    store.showMissionSetup = true
  }

  private var startLabel: String {
    if launchInFlight { return fleetWidth > 4 ? "Planning" : "Starting" }
    if recommendation?.isMission == true && !showAdvanced { return "Looks right, set it up" }
    return fleetWidth > 4 ? "Looks right, prepare the plan" : recommendation?.cta ?? "Start build"
  }

  private var startIcon: String {
    if recommendation?.isMission == true && !showAdvanced { return "flag.checkered" }
    if fleetWidth > 4 { return "rectangle.3.group" }
    return "hammer.fill"
  }

  private var launchInFlight: Bool {
    store.buildStartInFlight || adaptingLaunch
  }

  private var launchInFlightCallout: some View {
    HStack(alignment: .top, spacing: 10) {
      ProgressView()
        .controlSize(.small)
        .padding(.top, 2)
      VStack(alignment: .leading, spacing: 3) {
        Text(fleetWidth > 4 ? "Preparing the fleet plan" : "Starting the run")
          .font(.callout.weight(.semibold))
        Text(fleetWidth > 4
          ? "Forge is drafting lanes, checking repo proof, and asking for wide-plan review before any workers start."
          : "Forge is admitting the run and preparing the first proof step.")
          .font(.callout)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
      Spacer(minLength: 0)
    }
    .padding(12)
    .background(
      RoundedRectangle(cornerRadius: 10, style: .continuous)
        .fill(Color.accentColor.opacity(0.08))
    )
  }

  private var fallbackRecommendation: WorkRecommendation? {
    guard classificationError != nil || !repoReady else { return nil }
    return WorkRecommendation(
      shape: fleetWidth > 4 ? "fleet" : "run",
      width: max(1, fleetWidth),
      label: "Choose the path",
      reason: repoReady ? "" : "Forge can start from the local MDx runtime workspace.",
      taskClass: "",
      complexityTier: "",
      confidencePct: 0,
      rationale: "",
      suggestedCheckCommands: [],
      allowedWriteScope: "",
      blockedPaths: ""
    )
  }

  private var selectedRepoLabel: String {
    store.repos.first { $0.id == store.selectedRepoID }?.label ?? "Runtime workspace"
  }

  private var isExternalRepo: Bool {
    !store.selectedRepoID.isEmpty && store.selectedRepoID != "mdx-self"
  }

  private var placeholder: String {
    isExternalRepo
      ? "e.g. Add a retry with backoff to the API client, and a test that covers it."
      : "e.g. Add a health check endpoint to the API, with a test."
  }

  private func pick(_ candidate: ScoutCandidate) {
    intent = candidate.intent
    fleetWidth = Self.widthForTier(candidate.complexity, taskClass: candidate.taskClass)
    showAdvanced = false
  }

  static func widthForTier(_ tier: String, taskClass: String) -> Int {
    let t = tier.lowercased()
    if t.contains("extreme") { return 8 }
    if t.contains("medium") || taskClass.lowercased().contains("refactor") { return 2 }
    return 1
  }

  private func widthChoices(for recommendation: WorkRecommendation) -> [WidthChoice] {
    let recommended = max(1, recommendation.width)
    var choices = [
      WidthChoice(
        id: "auto",
        title: "Auto",
        detail: widthSummary(for: recommended, recommendation: recommendation),
        width: recommended,
        systemImage: "wand.and.stars",
        isAuto: true
      ),
      WidthChoice(id: "solo", title: "Solo", detail: "1 worker", width: 1, systemImage: "person"),
      WidthChoice(id: "pair", title: "Pair", detail: "2 workers", width: 2, systemImage: "person.2"),
      WidthChoice(id: "direct4", title: "Direct 4", detail: "4 workers", width: 4, systemImage: "person.3")
    ]
    choices.append(contentsOf: [
      WidthChoice(id: "fleet8", title: "Fleet 8", detail: "8 workers, planned split", width: 8, systemImage: "rectangle.3.group"),
      WidthChoice(id: "fleet16", title: "Fleet 16", detail: "16 workers, planned split", width: 16, systemImage: "square.grid.3x3"),
      WidthChoice(id: "fleet24", title: "Fleet 24", detail: "24 workers, planned split", width: 24, systemImage: "square.grid.4x3.fill")
    ])
    return choices
  }

  private func widthSummary(for width: Int, recommendation: WorkRecommendation) -> String {
    if recommendation.isMission && !showAdvanced {
      return "\(width) worker mission"
    }
    if width > 4 {
      return "\(width)-worker fleet plan"
    }
    return "\(width)-worker direct run"
  }
}

enum ForgeExecutionLocation: Equatable {
  case thisMac
  case cloudProfileRequired

  var title: String {
    switch self {
    case .thisMac: return "Runs on This Mac"
    case .cloudProfileRequired: return "Cloud sign-in needed"
    }
  }

  var detail: String {
    switch self {
    case .thisMac:
      return "Forge uses the local MDx runtime, this checkout, and model access from Keychain. The run keeps this location with its record."
    case .cloudProfileRequired:
      return "A server address alone cannot start governed work. Cloud opens after MDx has a signed-in profile and stores its refresh access in Keychain."
    }
  }

  var systemImage: String {
    switch self {
    case .thisMac: return "macbook"
    case .cloudProfileRequired: return "icloud.slash"
    }
  }

  var canStart: Bool { self == .thisMac }
}

private struct WidthChoice: Identifiable, Equatable {
  let id: String
  let title: String
  let detail: String
  let width: Int
  let systemImage: String
  var isAuto = false
}

private struct WidthChoiceButton: View {
  let choice: WidthChoice
  let selected: Bool
  let action: () -> Void

  var body: some View {
    Button(action: action) {
      HStack(alignment: .top, spacing: 8) {
        Image(systemName: choice.systemImage)
          .font(.caption.weight(.semibold))
          .frame(width: 18)
          .foregroundStyle(selected ? Color.accentColor : .secondary)
        VStack(alignment: .leading, spacing: 2) {
          Text(choice.title)
            .font(.caption.weight(.semibold))
            .foregroundStyle(.primary)
          Text(choice.detail)
            .font(.caption2)
            .foregroundStyle(.secondary)
            .lineLimit(2)
            .fixedSize(horizontal: false, vertical: true)
        }
        Spacer(minLength: 0)
      }
      .padding(.vertical, 8)
      .padding(.horizontal, 9)
      .frame(maxWidth: .infinity, minHeight: 52, alignment: .topLeading)
      .background(
        RoundedRectangle(cornerRadius: 8, style: .continuous)
          .fill(selected ? Color.accentColor.opacity(0.10) : Color.secondary.opacity(0.06))
      )
      .overlay(
        RoundedRectangle(cornerRadius: 8, style: .continuous)
          .stroke(selected ? Color.accentColor.opacity(0.34) : Color.secondary.opacity(0.12), lineWidth: 1)
      )
    }
    .buttonStyle(.plain)
  }
}

struct ScoutCandidateCard: View {
  let candidate: ScoutCandidate
  let use: () -> Void

  var body: some View {
    Button(action: use) {
      VStack(alignment: .leading, spacing: 6) {
        HStack(alignment: .firstTextBaseline) {
          Text(candidate.title)
            .font(.subheadline.weight(.medium))
            .lineLimit(2)
          Spacer(minLength: 8)
          Text("Use this")
            .font(.caption.weight(.medium))
            .foregroundStyle(Color.accentColor)
        }
        if !candidate.why.isEmpty {
          Text(candidate.why)
            .font(.caption)
            .foregroundStyle(.secondary)
            .lineLimit(2)
            .fixedSize(horizontal: false, vertical: true)
        }
        HStack(spacing: 6) {
          if !candidate.location.isEmpty {
            MetaChip(text: candidate.location)
          }
          if !candidate.complexity.isEmpty {
            MetaChip(text: candidate.complexity)
          }
        }
      }
      .padding(12)
      .frame(maxWidth: .infinity, alignment: .leading)
      .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
      .mdxGlassSurface(interactive: true)
    }
    .buttonStyle(.plain)
  }
}
