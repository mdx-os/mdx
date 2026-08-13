import AppKit
import Foundation
import Observation
import OSLog

@Observable
@MainActor
final class OperatorStore {
  var selectedAppRoute: AppRoute = .home
  var commandPaletteShown = false
  var selectedRouteID: String?
  var actionDraft = GovernedActionDraft()
  var actionResult: GovernedActionResult?
  private(set) var actionInFlight: GovernedActionKind?
  private(set) var hostProjects: [HostProject] = []
  private(set) var phase: LoadPhase = .idle
  private(set) var snapshot: OperatorSnapshot

  // The hot run fields are their own observable properties, not sub-fields of
  // the single `snapshot`. A 4s run-poll (or fleet tick) mutates these alone,
  // so it invalidates run readers only, not every surface that reads any
  // snapshot field. They are seeded from each full snapshot load and updated
  // in place by refreshRuns.
  private(set) var forgeRuns: [ForgeRun] = []
  private(set) var parallelExecutionGroups: [ForgeParallelExecutionGroup] = []

  var selectedRunID: String?
  private(set) var liveEvents: [ForgeEvent] = []
  /// Content signatures of events already in the live trail, so a reconnect
  /// replay does not duplicate them (see appendLiveEvents).
  @ObservationIgnored private var seenEventSignatures: Set<String> = []
  @ObservationIgnored private var seenEventSignatureOrder: [String] = []
  /// True while the live trail lost its stream and is coming back.
  private(set) var streamReconnecting = false
  private(set) var runDiff: [DiffFile] = []
  private(set) var runDiffPhase: LoadPhase = .idle
  @ObservationIgnored private var runDiffGeneration = 0
  private(set) var reviewPacket: ReviewPacket?
  private(set) var reviewPacketPhase: LoadPhase = .idle
  @ObservationIgnored private var reviewPacketGeneration = 0
  private(set) var browserAudit: BrowserAuditResult?
  private(set) var browserAuditInFlight = false
  private(set) var browserEvolutionActive = false
  private(set) var browserEvolutionCycle = 0
  private(set) var browserEvolutionRunID = ""
  var browserAuditError: String?
  var runActionResult: RunActionOutcome?
  // Per-run in-flight verbs (ship/steer/stop/revise). Acting on run A never
  // disables run B's verbs. Build-start and fleet planning use their own flag
  // below so a long fleet-plan POST cannot freeze every run verb in the app.
  private(set) var runActionInFlightIDs: Set<String> = []
  private(set) var buildStartInFlight = false
  private var buildStartTask: Task<RunActionOutcome, Error>?
  var showStartBuild = false
  /// Intent handed from the command palette into the Start a Build sheet.
  var pendingBuildIntent: String?
  private var didAutoOpenBuildShot = false
  private(set) var repos: [ForgeRepo] = []
  var selectedRepoID: String = ""
  private(set) var scout: ScoutResult?
  private(set) var scoutPhase: LoadPhase = .idle
  private(set) var missions: [ForgeMission] = []
  var selectedMissionID: String?
  private(set) var twinMessages: [TwinMessage] = []
  var twinStanceID: String = "auto"
  private(set) var twinSending = false
  private(set) var twinConversations: [TwinConversation] = []
  private(set) var twinLocalActivityTimes: [String: Date] = [:]
  private(set) var twinConversationsLoaded = false
  private(set) var activityItems: [ActivityItem] = []
  private(set) var activityAreas: [ActivityArea] = []
  private(set) var messageChannels: [MessageChannel] = []
  private(set) var threadMessages: [ThreadMessage] = []
  private(set) var actionCards: [ActionCard] = []
  private(set) var presencePeople: [PresencePerson] = []
  private(set) var activeNowCount: Int = 0
  private(set) var messagePhase: LoadPhase = .idle
  var messageLane: MessageLane = .channel(UserDefaults.standard.string(forKey: "mdx.message.channel") ?? "local-ops")
  var messageSearch: String = ""
  var messageCatchUpShown = false
  var openMessageThreadReceiptID: String?
  var messageReplyDraft = ""
  private(set) var messageReplyInFlight = false
  var messageReplyResult: RunActionOutcome?
  private(set) var messageReactionInFlightReceiptID: String?
  private(set) var messageReactionResultParentReceiptID: String?
  var messageReactionResult: RunActionOutcome?
  private(set) var verdictInFlightCardID: String?
  var verdictResult: RunActionOutcome?
  var messageDraft = ""
  private(set) var messageSendInFlight = false
  var messageSendResult: RunActionOutcome?
  private(set) var messageOutbox: [QueuedThreadMessage] = OperatorStore.loadMessageOutbox()
  @ObservationIgnored private var messageOutboxFlushing = false
  @ObservationIgnored private var messageOutboxRetryAfter = Date.distantPast
  var showDiagnostics = false
  // Feedback rail (Cmd-Shift-B): governed capture with a receipt back.
  var showFeedback = false
  var feedbackCategoryID = "bug"
  var feedbackNote = ""
  private(set) var feedbackInFlight = false
  var feedbackResult: RunActionOutcome?
  private(set) var filedReports: [FiledReport] = []
  /// Kernel-side lifecycle per receipt (received/triaged/working/resolved).
  private(set) var reportStatuses: [String: RemoteReportStatus] = [:]
  /// The kernel's self-description, when it has /version.json.
  private(set) var kernelVersion: KernelVersion?
  /// Latest notarized canary, shown only when it is newer than this bundle.
  private(set) var availableMacRelease: MacReleaseManifest?
  private(set) var macReleaseCheckInFlight = false
  /// True when the previous session did not exit cleanly (crash/force quit).
  private(set) var launchedAfterUncleanExit = false
  /// The launch-time crash verdict, frozen for the health rollup. Unlike
  /// `launchedAfterUncleanExit` (which the crash notice and feedback clear), this
  /// stays true for the whole session so the `unclean_relaunch` metric is honest.
  @ObservationIgnored private var uncleanRelaunchThisSession = false
  /// The last app-health state we acted on, so we post ONCE on a healthy ->
  /// not-healthy transition rather than on every recorded event.
  @ObservationIgnored private var lastObservedHealth: AppHealth?
  /// Guards the one-time unclean-relaunch health post.
  @ObservationIgnored private var didPostUncleanRelaunch = false
  /// Stable per-install session id for central app-health, anchored to a
  /// persistent install uuid and this launch, so each app run is a distinct
  /// session in the cross-session projection while staying install-scoped.
  @ObservationIgnored private lazy var healthSessionID: String = Self.makeHealthSessionID()
  /// The cross-session app-health aggregate for the Diagnostics surface.
  private(set) var appHealthProjection: AppHealthProjection?
  private(set) var appHealthProjectionPhase: LoadPhase = .idle
  // Kernel-composed PR handoff for the open run.
  private(set) var prHandoff: PRHandoff?
  private(set) var prHandoffPhase: LoadPhase = .idle
  var showPRHandoff = false
  private(set) var identitySession: IdentitySession?
  private(set) var clearance: Clearance?
  private(set) var memoryWorkspace: MemoryWorkspace?
  private(set) var memoryPhase: LoadPhase = .idle
  var selectedMemoryRail: MemoryRail = .learned
  var memorySearch: String = ""
  private(set) var memoryActionInFlight = false
  var memoryActionResult: RunActionOutcome?
  private(set) var capabilities: [Capability] = []
  private(set) var marketplacePacks: [MarketplacePack] = []
  private(set) var marketplacePhase: LoadPhase = .idle
  var marketplaceSection: MarketplaceSection = .forYou
  var selectedPackID: String?
  var selectedCapabilityID: String?
  var marketplaceFilter: MarketplaceFilter = .all
  var marketplaceSearch: String = ""
  // Twin conversation filter lives in the store so the global toolbar search
  // can drive it too (one search box, always live on the current surface).
  var twinConvoSearch: String = ""
  var twinProjectName: String = UserDefaults.standard.string(forKey: "mdx.twin.project.name") ?? ""
  var twinProjectInstructions: String = UserDefaults.standard.string(forKey: "mdx.twin.project.instructions") ?? ""
  private(set) var capabilityActionInFlight = false
  private(set) var packActionInFlight = false
  var capabilityActionResult: RunActionOutcome?
  var packActionResult: RunActionOutcome?
  private(set) var packTrialPreview: PackTrialPreview?
  private(set) var machineActionInFlight = false
  var machineActionResult: RunActionOutcome?
  private(set) var machinePreflightInFlight = false
  private(set) var machinePreflightSummary: String?
  private(set) var tryPreview: TryPreview?
  private(set) var pages: [Page] = []
  private(set) var pageDrafts: [PageDraftRecord] = []
  private(set) var pageReviewRequests: [PageReviewRequest] = []
  private(set) var pagesPhase: LoadPhase = .idle
  var selectedPageID: String?
  private(set) var selectedPageBody: PageBody?
  private(set) var pageBodyPhase: LoadPhase = .idle
  var pageSearch: String = ""
  private(set) var pageSearchIDs: [String]?
  @ObservationIgnored private var pageSearchTask: Task<Void, Never>?
  var showPagePublish = false
  var pagePublishDraft = PagePublishDraft()
  private(set) var pagePublishInFlight = false
  var pagePublishResult: RunActionOutcome?
  private(set) var twinSessionID = "native_twin_" + NSUserName()
  private(set) var twinAttachments: [TwinAttachment] = []
  private(set) var twinAttaching = false
  var twinAttachError: String?
  var twinSensitive = false
  @ObservationIgnored private var twinStreamTask: Task<Void, Never>?
  var missionActionResult: RunActionOutcome?
  private(set) var missionActionInFlight = false
  var showMissionSetup = false
  var missionSetupDraft = MissionDraft()
  @ObservationIgnored private var streamTask: Task<Void, Never>?
  @ObservationIgnored private var pollTask: Task<Void, Never>?
  @ObservationIgnored private var watchTask: Task<Void, Never>?
  @ObservationIgnored private var reconnectTask: Task<Void, Never>?
  @ObservationIgnored private var fleetPlanningTask: Task<Void, Never>?
  @ObservationIgnored private var refreshTask: Task<Void, Never>?
  @ObservationIgnored private var prHandoffTask: Task<Void, Never>?
  @ObservationIgnored private var browserEvolutionTask: Task<Void, Never>?
  @ObservationIgnored private var browserEvolutionGeneration = 0
  @ObservationIgnored private var didApplyShotRoute = false
  @ObservationIgnored private var didAutoShowDiagnostics = false
  /// Monotonic token dropping out-of-order projection loads.
  @ObservationIgnored private var loadGeneration = 0
  @ObservationIgnored private var startedRunIDs: Set<String> = []
  /// Watched runs that finished and have not been opened yet: the sidebar's
  /// unread state. Observation-tracked so badges update.
  private(set) var unseenRunIDs: Set<String> = []
  @ObservationIgnored private var notifiedRunIDs: Set<String> = []
  @ObservationIgnored private var isForeground = true
  /// Run states persisted from the previous session, used once at launch to
  /// compute "finished while you were away".
  @ObservationIgnored private var previousSessionRunStates: [String: Bool]?
  @ObservationIgnored private var didRecordBetaSessionStart = false
  /// Runs that were live when the app last quit and are finished now.
  private(set) var sinceYouLeftRunIDs: [String] = []

  /// First-run welcome: true while the full-window welcome cover is showing.
  /// Armed once on a fresh install (no `MDxHasOnboarded` flag) and re-openable
  /// later from the Help menu. Never traps the user: skip always closes it.
  var welcomeShown = false
  /// Settings: true while the full-window settings cover takes over the main
  /// window (Codex-desktop style), instead of a detached macOS Settings window.
  /// Shell behind it stays inert; "Back to app" / Esc / Cmd-. close it.
  var settingsShown = false
  /// The kernel's lean setup-router readout, loaded lazily by the welcome flow
  /// so it can confirm the real owner and connected model (never fabricated).
  private(set) var setupRouter: SetupRouterState = .unreachable
  private(set) var setupActionInFlight = false
  var setupActionResult: RunActionOutcome?

  static let apiURLDefaultsKey = "MDxAPIURL"
  static let hasOnboardedDefaultsKey = "MDxHasOnboarded"
  static let appearanceModeDefaultsKey = "MDxAppearanceMode"
  static let workModeDefaultsKey = "MDxWorkMode"
  // V2 key so the panel resets to the new opt-in default (off) for everyone once.
  static let showInspectorDefaultsKey = "MDxContextPanelVisible"
  private static let legacyAPIURLDefaultsKey = "MDxOperatorAPIURL"
  private static let hostProjectsDefaultsKey = "MDxHostProjects"
  private static let lastRouteDefaultsKey = "MDxLastRoute"
  private static let runStatesDefaultsKey = "MDxRunStatesAtExit"
  private static let filedReportsDefaultsKey = "MDxFiledReports"
  static let cleanExitDefaultsKey = "MDxCleanExit"

  private let client: MDxRouteClient
  private let macReleaseHostedOrigin: URL?
  private let logger = Logger(subsystem: "com.mdx.app", category: "Store")

  /// One point in navigation: the route AND the selection state that makes
  /// the screen what it was. Route-only history lands Back on the wrong run.
  private struct NavSnapshot: Equatable {
    let route: AppRoute
    let runID: String?
    let missionID: String?
    let pageID: String?
  }

  private var backStack: [NavSnapshot] = []
  private var forwardStack: [NavSnapshot] = []

  private var currentNavSnapshot: NavSnapshot {
    NavSnapshot(route: selectedAppRoute, runID: selectedRunID, missionID: selectedMissionID, pageID: selectedPageID)
  }

  private func pushHistory() {
    backStack.append(currentNavSnapshot)
    forwardStack.removeAll()
  }

  init(
    client: MDxRouteClient = MDxRouteClient(),
    macReleaseHostedOrigin: URL? = CloudConfiguration.current?.hostedOrigin,
    initialSnapshot: OperatorSnapshot? = nil,
    startBackgroundTasks: Bool = true
  ) {
    self.client = client
    self.macReleaseHostedOrigin = macReleaseHostedOrigin
    let baseURL = Self.configuredBaseURL()
    self.snapshot = initialSnapshot ?? .offline(baseURL: baseURL)
    self.forgeRuns = initialSnapshot?.forgeRuns ?? []
    self.parallelExecutionGroups = initialSnapshot?.parallelExecutionGroups ?? []
    self.hostProjects = Self.loadHostProjects()
    previousSessionRunStates = UserDefaults.standard.dictionary(forKey: Self.runStatesDefaultsKey) as? [String: Bool]
    if let data = UserDefaults.standard.data(forKey: Self.filedReportsDefaultsKey),
       let reports = try? JSONDecoder().decode([FiledReport].self, from: data) {
      filedReports = reports
    }
    // Crash detection: the flag is armed at launch and only cleared by a
    // clean applicationWillTerminate. If it is still false now, the last
    // session ended abruptly.
    if UserDefaults.standard.object(forKey: Self.cleanExitDefaultsKey) != nil {
      launchedAfterUncleanExit = !UserDefaults.standard.bool(forKey: Self.cleanExitDefaultsKey)
    }
    uncleanRelaunchThisSession = launchedAfterUncleanExit
    UserDefaults.standard.set(false, forKey: Self.cleanExitDefaultsKey)
    // Instant resume: reopen on the surface the user left.
    if let savedRoute = UserDefaults.standard.string(forKey: Self.lastRouteDefaultsKey),
       let route = AppRoute(id: savedRoute) {
      self.selectedAppRoute = route
    }
    // First run: a cold install has never onboarded, so open the welcome cover.
    // Every later launch skips it; the Help menu re-opens it on demand.
    self.welcomeShown = !UserDefaults.standard.bool(forKey: Self.hasOnboardedDefaultsKey)
    // Deterministic capture: MDX_SHOT_WELCOME forces the cover visible so the
    // onboarding first-win step can be photographed on any install state.
    if ProcessInfo.processInfo.environment["MDX_SHOT_WELCOME"] != nil {
      self.welcomeShown = true
    }
    // ...and MDX_SHOT_SKIP_WELCOME forces it closed, so the gallery can capture
    // every other state on a fresh (not-yet-onboarded) install without the cover
    // in the way. No UserDefaults mutation, so it never marks the real install
    // onboarded.
    if ProcessInfo.processInfo.environment["MDX_SHOT_SKIP_WELCOME"] != nil {
      self.welcomeShown = false
    }
    // Persist a rolling session summary so Diagnostics survives a crash/relaunch,
    // and on the same cadence batch the metrics-only health rollup to the central
    // ingest (never per-event). A rollup with no calls yet is skipped.
    if startBackgroundTasks {
      Task { [weak self] in
        while true {
          try? await Task.sleep(nanoseconds: 60_000_000_000)
          guard let self else { return }
          DiagnosticsStore.shared.persistSession()
          if DiagnosticsStore.shared.totalCalls > 0 {
            await self.postAppHealthRollup(trigger: .cadence)
          }
        }
      }
      // Post ONCE when app health first slips from healthy, without polling: the
      // observer re-arms itself each time an app-health input changes.
      lastObservedHealth = appHealth
      observeHealthTransitions()
      // Clicking a run-finished notification lands on that run.
      NotificationCenter.default.addObserver(forName: RunNotifier.openRunNotification, object: nil, queue: .main) { [weak self] note in
        guard let runID = note.userInfo?[RunNotifier.runIDKey] as? String else { return }
        Task { @MainActor [weak self] in
          guard let self else { return }
          self.select(.forge(.runs))
          self.openRun(runID)
        }
      }
      startReconnectWatch()
    }
  }

  /// Close every live cloud reader and remove account-scoped projections before
  /// the auth gate can admit another person. The App creates a fresh store after
  /// the next successful admission, so an old tenant can never flash while the
  /// new account's first refresh is still in flight.
  func suspendCloudAccess() {
    loadGeneration += 1
    refreshTask?.cancel()
    refreshTask = nil
    buildStartTask?.cancel()
    buildStartTask = nil
    streamTask?.cancel()
    streamTask = nil
    pollTask?.cancel()
    pollTask = nil
    watchTask?.cancel()
    watchTask = nil
    reconnectTask?.cancel()
    reconnectTask = nil
    fleetPlanningTask?.cancel()
    fleetPlanningTask = nil
    pageSearchTask?.cancel()
    pageSearchTask = nil
    twinStreamTask?.cancel()
    twinStreamTask = nil
    prHandoffTask?.cancel()
    prHandoffTask = nil
    browserEvolutionTask?.cancel()
    browserEvolutionTask = nil

    snapshot = .offline(
      baseURL: Self.configuredBaseURL(),
      reason: "Sign in to MDx to reconnect."
    )
    phase = .idle
    forgeRuns = []
    parallelExecutionGroups = []
    repos = []
    missions = []
    twinMessages = []
    twinConversations = []
    twinLocalActivityTimes = [:]
    twinConversationsLoaded = false
    activityItems = []
    activityAreas = []
    messageChannels = []
    threadMessages = []
    actionCards = []
    presencePeople = []
    activeNowCount = 0
    identitySession = nil
    clearance = nil
    memoryWorkspace = nil
    capabilities = []
    marketplacePacks = []
    pages = []
    pageDrafts = []
    pageReviewRequests = []
    selectedPageBody = nil
    selectedRunID = nil
    selectedMissionID = nil
    selectedPageID = nil
    selectedRepoID = ""
    resetLiveTrail()
    runDiff = []
    reviewPacket = nil
    browserAudit = nil
    prHandoff = nil
    streamReconnecting = false
    NSApp?.dockTile.badgeLabel = nil
  }

  func refresh() async {
    // Coalesce: overlapping refreshes (Cmd-R, watcher, post-action) share one
    // in-flight load instead of racing to clobber the snapshot.
    if let existing = refreshTask {
      await existing.value
      return
    }
    let task = Task { await self.performRefresh() }
    refreshTask = task
    await task.value
    refreshTask = nil
  }

  private func performRefresh() async {
    phase = .loading
    logger.info("Refreshing local MDx routes")
    loadGeneration += 1
    let generation = loadGeneration
    let baseURL = Self.configuredBaseURL()
    let nextSnapshot = await client.loadSnapshot(baseURL: baseURL)
    // A newer full load superseded this one; applying this stale snapshot would
    // roll fresh state back. Never leave the UI stuck on .loading: the newer
    // load owns the resolved phase, but if it has not resolved yet, settle to a
    // loaded state so a manual refresh can never strand the app on a spinner.
    guard generation == loadGeneration else {
      if case .loading = phase {
        phase = .loaded(snapshot.loadedAt ?? Date())
      }
      return
    }
    let selectedWasRunning = selectedRun?.isRunning ?? false
    let selectedID = selectedRunID
    snapshot = nextSnapshot
    // Seed the scoped run properties from the full load. From here a run poll
    // updates these directly without touching (and invalidating) snapshot.
    forgeRuns = nextSnapshot.forgeRuns
    parallelExecutionGroups = nextSnapshot.parallelExecutionGroups
    reloadReviewSurfacesIfSelectedRunFinished(wasRunning: selectedWasRunning, runID: selectedID)
    if selectedRouteID == nil || !nextSnapshot.routeCards.contains(where: { $0.id == selectedRouteID }) {
      selectedRouteID = nextSnapshot.routeCards.first?.id
    }
    if nextSnapshot.connectionStatus == .ok {
      stopReconnectWatch()
      updateRunStateLedger(with: nextSnapshot.forgeRuns)
      if repos.isEmpty { await loadRepos() }
      if kernelVersion == nil {
        kernelVersion = await client.loadKernelVersion(baseURL: baseURL)
      }
      let loadedAt = nextSnapshot.loadedAt ?? Date()
      phase = .loaded(loadedAt)
      Telemetry.markReady("routes loaded")
      startGlobalWatch()
      syncFleetActivityWatch()
      updateDockBadge()
      maybeAutoOpenForScreenshot()
      // A crash/force-quit relaunch is a first-class trouble signal: report it
      // once, now that the kernel is confirmed reachable.
      if uncleanRelaunchThisSession, !didPostUncleanRelaunch {
        didPostUncleanRelaunch = true
        Task { [weak self] in await self?.postAppHealthRollup(trigger: .uncleanRelaunch) }
      }
      logger.info("Local MDx routes loaded")
    } else {
      phase = .failed(nextSnapshot.boundary)
      NSApp.dockTile.badgeLabel = nil
      // The shell is up and honestly showing the offline lifecycle banner: that
      // is "ready". Marking ready here stops a cold launch against a down engine
      // from reporting a launch time of the entire offline wait (~130s); the
      // kernel connection is a separate, visible signal, not launch latency.
      Telemetry.markReady("offline shell")
      logger.info("Local MDx routes unavailable")
      stopFleetActivityWatch()
      startReconnectWatch()
      // Capture hooks that don't need a live kernel (MDX_SHOT_DIAG, _SETTINGS,
      // _ROUTE) also fire offline, so the QA rail can photograph the offline
      // Diagnostics panel. Run-opening hooks are inert here (no runs loaded).
      maybeAutoOpenForScreenshot()
    }
  }

  func select(_ route: AppRoute) {
    guard route != selectedAppRoute else { return }
    pushHistory()
    selectedAppRoute = route
    UserDefaults.standard.set(route.id, forKey: Self.lastRouteDefaultsKey)
    syncRunStreamingToRoute()
    Telemetry.breadcrumb("open \(route.id)", "nav")
    logger.info("Selected app route: \(route.id, privacy: .public)")
  }

  /// The SSE stream and the 4s poll exist to feed the open run detail; when
  /// the user navigates elsewhere they stop, and they resume (with a clean
  /// replay) when the run comes back on screen.
  private func syncRunStreamingToRoute() {
    if selectedAppRoute == .forge(.runs) {
      if streamTask == nil, let run = selectedRun, run.isRunning, !run.streamRoute.isEmpty {
        resetLiveTrail()
        startStreaming(run: run)
        startPolling()
      }
    } else if streamTask != nil || pollTask != nil {
      stopStreaming()
      stopPolling()
    }
  }

  func select(_ section: OperatorSection) {
    select(Self.appRoute(for: section))
  }

  var canNavigateBack: Bool {
    !backStack.isEmpty
  }

  var canNavigateForward: Bool {
    !forwardStack.isEmpty
  }

  func navigateBack() {
    guard let nav = backStack.popLast() else { return }
    forwardStack.append(currentNavSnapshot)
    restore(nav)
    logger.info("Navigated back to app route: \(nav.route.id, privacy: .public)")
  }

  func navigateForward() {
    guard let nav = forwardStack.popLast() else { return }
    backStack.append(currentNavSnapshot)
    restore(nav)
    logger.info("Navigated forward to app route: \(nav.route.id, privacy: .public)")
  }

  private func restore(_ nav: NavSnapshot) {
    selectedAppRoute = nav.route
    if selectedRunID != nav.runID {
      presentRun(nav.runID)
    }
    if selectedMissionID != nav.missionID {
      selectedMissionID = nav.missionID
      missionActionResult = nil
    }
    if let pageID = nav.pageID, pageID != selectedPageID {
      selectPage(pageID)
    }
    syncRunStreamingToRoute()
  }

  func selectRoute(_ route: RouteCard) {
    selectedRouteID = route.id
    logger.info("Selected route: \(route.path, privacy: .public)")
  }

  var selectedRoute: RouteCard? {
    guard let selectedRouteID else {
      return snapshot.routeCards.first
    }
    return snapshot.routeCards.first { $0.id == selectedRouteID } ?? snapshot.routeCards.first
  }

  func updateBaseURL(_ value: String) {
    UserDefaults.standard.set(value, forKey: Self.apiURLDefaultsKey)
    logger.info("Updated local API URL")
  }

  // MARK: - Welcome / first run

  /// Re-open the welcome cover from the Help menu. Does not reset onboarding
  /// state: finishing or skipping again simply closes it.
  func showWelcome() {
    welcomeShown = true
    logger.info("Welcome re-opened")
  }

  /// Load the kernel's setup-router readout for the welcome flow's connect step.
  func loadSetupRouter() async {
    setupRouter = await client.loadSetupRouter(baseURL: Self.configuredBaseURL())
  }

  func claimInstallOwner(_ name: String) async {
    let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty, !setupActionInFlight else { return }
    setupActionInFlight = true
    defer { setupActionInFlight = false }
    do {
      setupActionResult = try await client.claimInstallOwner(baseURL: Self.configuredBaseURL(), name: trimmed, actorID: actorID)
      await loadSetupRouter()
    } catch {
      setupActionResult = RunActionOutcome(title: "Claim this install", status: "FAILED", detail: friendlyLoadError(error), receiptID: "")
    }
  }

  func connectTwinModel(providerID: String, apiKey: String) async {
    let provider = providerID.trimmingCharacters(in: .whitespacesAndNewlines)
    let trimmed = apiKey.trimmingCharacters(in: .whitespacesAndNewlines)
    let credentialless = ["ollama", "vllm"].contains(provider)
    guard !provider.isEmpty, (credentialless || !trimmed.isEmpty), !setupActionInFlight else { return }
    setupActionInFlight = true
    defer { setupActionInFlight = false }
    do {
      setupActionResult = try await client.connectTwinModel(
        baseURL: Self.configuredBaseURL(),
        providerID: provider,
        apiKey: trimmed,
        actorID: actorID
      )
      await loadSetupRouter()
    } catch {
      setupActionResult = RunActionOutcome(title: "Connect a model", status: "FAILED", detail: friendlyLoadError(error), receiptID: "")
    }
  }

  /// Close the welcome cover for good and, if a path was chosen, land on it.
  /// Marks the install onboarded so the cover never auto-appears again.
  func finishWelcome(entering route: AppRoute?) {
    UserDefaults.standard.set(true, forKey: Self.hasOnboardedDefaultsKey)
    welcomeShown = false
    if let route {
      select(route)
    }
    logger.info("Welcome finished; entered \(route?.id ?? "nothing", privacy: .public)")
    Task {
      await recordBetaTelemetry(
        "activation_step",
        route: "/welcome",
        step: "welcome_finished",
        completed: true
      )
    }
  }

  /// The always-working escape. Lands the user in the app exactly where they
  /// already were, and never shows the cover again on its own.
  func skipWelcome() {
    finishWelcome(entering: nil)
    Telemetry.breadcrumb("welcome skipped", "nav")
  }

  /// Onboarding's real win: start a first governed build from the welcome flow,
  /// close the cover, and land in Forge watching it stream. This reuses the
  /// founder-ratified one-shot composer start, so the run is a real receipt on
  /// the kernel, not a demo. Eligibility (connected + model + a repo) is checked
  /// by the caller via `FirstWin.canStartRealBuild`.
  func startFirstBuild(intent: String) async {
    let trimmed = intent.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return }
    Telemetry.breadcrumb("welcome first build", "nav")
    finishWelcome(entering: .forge(.runs))
    await submitComposerBuild(intent: trimmed)
  }

  func submit(_ action: GovernedActionKind) async {
    actionInFlight = action
    logger.info("Submitting governed action: \(action.rawValue, privacy: .public)")
    do {
      let result = try await client.submitAction(
        baseURL: Self.configuredBaseURL(),
        action: action,
        draft: actionDraft
      )
      actionResult = result
      await refresh()
      logger.info("Governed action finished: \(result.status, privacy: .public)")
    } catch {
      actionResult = GovernedActionResult(
        action: action,
        status: "FAILED",
        title: action.title,
        detail: error.localizedDescription,
        route: action.path,
        receiptID: "",
        rawSummary: "Request failed before the kernel recorded a result."
      )
      logger.error("Governed action failed: \(error.localizedDescription, privacy: .public)")
    }
    actionInFlight = nil
  }

  // MARK: - Forge runs

  var activeRuns: [ForgeRun] { forgeRuns.filter { $0.isRunning } }

  var runningRunCount: Int { activeRuns.count }

  var reviewReadyRuns: [ForgeRun] {
    forgeRuns.filter(\.isReviewReady)
  }

  var reviewReadyCount: Int { reviewReadyRuns.count }

  func missionLinked(to runID: String) -> ForgeMission? {
    missions.first { $0.links(runID: runID) }
  }

  /// Running work first, then everything else in projection order.
  var sortedRuns: [ForgeRun] {
    forgeRuns.enumerated().sorted { lhs, rhs in
      if lhs.element.isRunning != rhs.element.isRunning { return lhs.element.isRunning }
      return lhs.offset < rhs.offset
    }.map(\.element)
  }

  var attentionCount: Int { reviewReadyCount }

  func updateDockBadge() {
    let count = attentionCount
    NSApp.dockTile.badgeLabel = count > 0 ? "\(count)" : nil
  }

  var selectedRun: ForgeRun? {
    guard let selectedRunID else { return nil }
    return forgeRuns.first { $0.id == selectedRunID }
  }

  func parallelGroup(containing runID: String) -> ForgeParallelExecutionGroup? {
    parallelExecutionGroups.first { $0.contains(runID: runID) }
  }

  var actorID: String { identitySession?.userID ?? NSUserName() }

  func openRun(_ id: String) {
    guard selectedRunID != id else { return }
    pushHistory()
    presentRun(id)
  }

  func closeRun() {
    guard selectedRunID != nil else { return }
    pushHistory()
    presentRun(nil)
  }

  /// Shared presenter: swap the run on screen without touching history.
  private func presentRun(_ id: String?) {
    stopStreaming()
    stopPolling()
    prHandoffTask?.cancel()
    prHandoffTask = nil
    browserEvolutionTask?.cancel()
    browserEvolutionTask = nil
    browserEvolutionGeneration += 1
    browserEvolutionActive = false
    selectedRunID = id
    if let id { unseenRunIDs.remove(id) }
    runDiff = []
    runDiffGeneration += 1
    runDiffPhase = .idle
    reviewPacket = nil
    reviewPacketGeneration += 1
    reviewPacketPhase = .idle
    browserAudit = nil
    browserAuditError = nil
    resetLiveTrail()
    runActionResult = nil
    guard let id else { return }
    if let run = forgeRuns.first(where: { $0.id == id }), run.isRunning, !run.streamRoute.isEmpty {
      startStreaming(run: run)
      startPolling()
    }
    logger.info("Opened run: \(id, privacy: .public)")
  }

  private func startStreaming(run: ForgeRun, attempt: Int = 0) {
    stopStreaming()
    let base = Self.configuredBaseURL()
    let route = run.streamRoute
    logger.info("Streaming run: \(run.id, privacy: .public)")
    streamTask = Task { [weak self] in
      guard let self else { return }
      let probe = StreamProbe("forge run stream")
      // Replay bursts and busy stages can emit hundreds of events per second.
      // Batch to at most ~4 trail publishes per second: fast enough to read as
      // live, slow enough that the activity list is not re-rendered 12 times a
      // second. Paired with the lazy trail, this is what stops the "text flying
      // in and out" churn during a busy run.
      let coalescer = StreamCoalescer<ForgeEvent>(intervalMs: 250) { [weak self] batch in
        self?.appendLiveEvents(batch)
      }
      var receivedEvents = false
      for await event in self.client.streamEvents(baseURL: base, streamRoute: route) {
        if Task.isCancelled { probe.finish("cancelled"); return }
        probe.tick()
        receivedEvents = true
        if self.streamReconnecting { self.streamReconnecting = false }
        coalescer.add(event)
      }
      coalescer.drain()
      probe.finish("completed")
      guard !Task.isCancelled else { return }
      await self.refreshRuns()
      // If the run is still live, the stream ended early (kernel restart,
      // sleep/wake, network blip). Reconnect with backoff instead of letting
      // the trail silently freeze: no silent work.
      if let current = self.selectedRun, current.id == run.id, current.isRunning, !current.streamRoute.isEmpty {
        self.streamReconnecting = true
        let nextAttempt = receivedEvents ? 0 : attempt + 1
        let delaySeconds = UInt64(min(8, 1 << min(nextAttempt, 3)))
        try? await Task.sleep(nanoseconds: delaySeconds * 1_000_000_000)
        guard !Task.isCancelled else { return }
        // Keep the trail across the reconnect: appendLiveEvents dedupes the
        // replay by signature, so nothing duplicates and the rendered rows are
        // not torn down. Clearing here was an empty-list flash plus a full
        // rebuild (a main-thread stall) on every reconnect.
        self.startStreaming(run: current, attempt: nextAttempt)
      }
    }
  }

  func stopStreaming() {
    streamTask?.cancel()
    streamTask = nil
    streamReconnecting = false
  }

  /// Append a batch of live events, capping the trail so a multi-hour run
  /// cannot grow memory and re-render cost without bound. The receipts and
  /// full history stay durable on the kernel; this is the live window.
  ///
  /// Deduplicates by content signature so a reconnect (whose replay resends the
  /// run from the start) does not re-append events already shown. This is what
  /// lets the reconnect path keep the existing trail instead of clearing it: no
  /// empty-list flash, and the already-rendered rows keep their identity so the
  /// list is not torn down and rebuilt (which stalled the main thread).
  private func appendLiveEvents(_ batch: [ForgeEvent]) {
    let fresh = batch.filter { event in
      let signature = liveSignature(for: event)
      if seenEventSignatures.insert(signature).inserted {
        seenEventSignatureOrder.append(signature)
        return true
      }
      return false
    }
    guard !fresh.isEmpty else { return }
    liveEvents = ForgeEvent.coalescingStreamingTokens(liveEvents, appending: fresh)
    if liveEvents.count > 1200 {
      let removeCount = liveEvents.count - 1000
      liveEvents.removeFirst(removeCount)
    }
    if seenEventSignatureOrder.count > 6000 {
      let stale = seenEventSignatureOrder.prefix(2000)
      stale.forEach { seenEventSignatures.remove($0) }
      seenEventSignatureOrder.removeFirst(2000)
    }
  }

  private func liveSignature(for event: ForgeEvent) -> String {
    "\(event.seq)|\(event.kind)|\(event.stage)|\(event.summary)"
  }

  /// Reset the live trail and its dedupe set together (a new run, or re-opening
  /// one). Always pair these so a stale signature never suppresses a real event.
  private func resetLiveTrail() {
    liveEvents = []
    seenEventSignatures = []
    seenEventSignatureOrder = []
  }

  private func startPolling() {
    stopPolling()
    pollTask = Task { [weak self] in
      while !Task.isCancelled {
        try? await Task.sleep(nanoseconds: 4_000_000_000)
        if Task.isCancelled { return }
        guard let self, let selectedID = self.selectedRunID else { return }
        await self.refreshRuns()
        // Ghost run: the selected run vanished from the projection (a kernel
        // restart, a pruned run). Without this the poll would spin forever on a
        // run that no longer exists. Stop the stream and poll.
        if self.forgeRuns.first(where: { $0.id == selectedID }) == nil {
          self.stopStreaming()
          self.stopPolling()
          return
        }
        if let run = self.selectedRun,
           !run.isRunning,
           (self.parallelGroup(containing: run.id)?.runningCandidateCount ?? 0) == 0 {
          self.stopStreaming()
          self.stopPolling()
          return
        }
      }
    }
  }

  private func stopPolling() {
    pollTask?.cancel()
    pollTask = nil
  }

  private var hasActiveFleetActivity: Bool {
    let planning = snapshot.fleetPlans.contains { plan in
      plan.isPlanning && !plan.status.localizedCaseInsensitiveContains("needs_attention")
    }
    let running = snapshot.fleetRuns.contains { $0.isActive }
    return planning || running
  }

  private func syncFleetActivityWatch() {
    if hasActiveFleetActivity {
      startFleetActivityWatch()
    } else {
      stopFleetActivityWatch()
    }
  }

  private func startFleetActivityWatch() {
    guard fleetPlanningTask == nil else { return }
    fleetPlanningTask = Task { [weak self] in
      while !Task.isCancelled {
        let cadence: UInt64 = await MainActor.run { [weak self] in
          self?.isForeground ?? true
        } ? 4_000_000_000 : 20_000_000_000
        try? await Task.sleep(nanoseconds: cadence)
        if Task.isCancelled { return }
        guard let self else { return }
        guard self.snapshot.connectionStatus == .ok else {
          self.stopFleetActivityWatch()
          return
        }
        guard self.hasActiveFleetActivity else {
          self.stopFleetActivityWatch()
          return
        }
        await self.refreshFleet()
      }
    }
  }

  /// Scoped fleet refresh: loads only the two fleet projections and publishes on
  /// change, instead of the full 17-route snapshot. Keeps the fleet lane live
  /// without a full app-wide reload every 4s while a fleet is active.
  func refreshFleet() async {
    guard refreshTask == nil else { await refreshTask?.value; return }
    let projections = await client.loadFleetProjections(baseURL: Self.configuredBaseURL())
    guard snapshot.connectionStatus == .ok, refreshTask == nil else { return }
    guard projections.plans != snapshot.fleetPlans || projections.runs != snapshot.fleetRuns else { return }
    snapshot.fleetPlans = projections.plans
    snapshot.fleetRuns = projections.runs
  }

  private func stopFleetActivityWatch() {
    fleetPlanningTask?.cancel()
    fleetPlanningTask = nil
  }

  /// Dev/self-audit hook: MDX_SHOT_RUN=first|active|latest (or a run id) opens a run on launch
  /// so screenshots can reach the detail without UI clicks. Inert without the env var.
  private func maybeAutoOpenForScreenshot() {
    let env = ProcessInfo.processInfo.environment
    if let request = ScreenshotBuildRequest.requested(in: env), !didAutoOpenBuildShot {
      // A live-proof autostart belongs to the store, not the sheet lifecycle.
      // Wait for a connected refresh so an offline launch can retry on reconnect
      // instead of consuming its one shot before the governed route is ready.
      if snapshot.connectionStatus == .ok {
        didAutoOpenBuildShot = true
        Task { [weak self] in await self?.autoStartBuildForScreenshot(request) }
      }
    } else if let intent = env["MDX_SHOT_BUILD_INTENT"], !didAutoOpenBuildShot, !showStartBuild {
      didAutoOpenBuildShot = true
      pendingBuildIntent = intent
      startBuildFlow()
    }
    // Self-audit hook for the Home composer one-shot start: exercises the exact
    // store path a plain composer submit takes (classify then start), without
    // opening the guided sheet. Inert without the env var.
    if let intent = env["MDX_SHOT_COMPOSER_INTENT"], !didAutoOpenBuildShot {
      didAutoOpenBuildShot = true
      Task { await submitComposerBuild(intent: intent) }
    }
    if let target = env["MDX_SHOT_RUN"], selectedRunID == nil, !forgeRuns.isEmpty {
      // "parallel" opens a run that is one lane of a multi-lane group so the
      // candidate switcher is on screen; it needs the store's group projection,
      // which ScreenshotRunTarget (runs only) cannot see. Falls through to the
      // normal resolver, so a kernel with no live group just picks nothing here
      // and the capture script records the state as unavailable rather than
      // faking a group.
      let resolved: ForgeRun? = target == "parallel"
        ? forgeRuns.first { (parallelGroup(containing: $0.id)?.candidates.count ?? 0) > 1 }
        : ScreenshotRunTarget.resolve(target: target, runs: forgeRuns)
      if let run = resolved {
        select(.forge(.runs))
        openRun(run.id)
      }
    }
    if let lane = env["MDX_SHOT_ROUTE"], !didApplyShotRoute {
      didApplyShotRoute = true
      let forgeMap: [String: ForgeLane] = [
        "overview": .overview, "runs": .runs, "missions": .missions,
        "review": .review, "fleet": .fleet, "machines": .machines, "evidence": .evidence
      ]
      let topMap: [String: AppRoute] = [
        "home": .home, "twin": .twin, "pages": .pages,
        "message": .message, "memory": .memory, "marketplace": .marketplace, "you": .you
      ]
      if let forgeLane = forgeMap[lane] { select(.forge(forgeLane)) }
      else if let route = topMap[lane] { select(route) }
    }
    if let rail = env["MDX_SHOT_MEMORY_RAIL"], let selected = MemoryRail(rawValue: rail) {
      selectedMemoryRail = selected
    }
    if env["MDX_SHOT_MISSION"] != nil, selectedMissionID == nil {
      Task {
        await loadMissions()
        if let mission = missions.first { select(.forge(.missions)); openMission(mission.id) }
      }
    }
    if env["MDX_SHOT_PALETTE"] != nil {
      Task { [weak self] in
        try? await Task.sleep(nanoseconds: 3_000_000_000)
        self?.showCommandPalette()
      }
    }
    if env["MDX_SHOT_PANEL"] != nil {
      Task { @MainActor in
        try? await Task.sleep(nanoseconds: 3_000_000_000)
        CompanionPanelController.shared.show()
      }
    }
    if env["MDX_SHOT_DIAG"] != nil, !didAutoShowDiagnostics {
      didAutoShowDiagnostics = true
      Task { [weak self] in
        try? await Task.sleep(nanoseconds: 6_000_000_000)
        self?.showDiagnostics = true
      }
    }
    if env["MDX_SHOT_SETTINGS"] != nil {
      Task { [weak self] in
        try? await Task.sleep(nanoseconds: 1_500_000_000)
        self?.showSettings()
      }
    }
    if env["MDX_SHOT_MSG_NEEDS"] != nil {
      select(.message)
      messageLane = .needsYou
    }
    if env["MDX_SHOT_PAGE_PUBLISH"] != nil {
      select(.pages)
      pagePublishDraft = PagePublishDraft(
        title: "Payments migration decision",
        body: "# Payments migration\n\nWe are moving off the legacy processor before Project Kestrel ships.\n\n- Owner: Dana Wu\n- Target: March 14\n- Risk: onboarding still needs a named owner\n\nThis page records the decision so the context stays inspectable.",
        pageType: "decision",
        visibility: "tenant_only"
      )
      showPagePublish = true
    }
    if env["MDX_SHOT_TWIN_ATTACH"] != nil {
      select(.twin)
      twinAttachments = [
        TwinAttachment(id: "shot_pdf", name: "board-brief.pdf", pageCount: 4, charCount: 8200, parseState: "PARSED_LOCAL_PDF_LITEPARSE", extractedText: "")
      ]
      twinSensitive = env["MDX_SHOT_TWIN_SENSITIVE"] != nil
    }
    if env["MDX_SHOT_TWIN_GROUNDED"] != nil {
      select(.twin)
      twinSessionID = "shot_grounded_session"
      var grounded = TwinMessage(
        id: "shot_grounded",
        prompt: "What are the top risks in this board update, and who owns the payments blocker?",
        answer: "Three risks stand out. Retention is the headline: the update flags it as rising, and onboarding still has no clear owner. Second, the payments migration is the gating blocker for Project Kestrel. Dana Wu owns it, with a March 14 ship date that slips if the migration isn't unblocked this month. Third, the board wants a single accountable owner for onboarding, which isn't named yet. My read: make the onboarding owner explicit this week, and put a date on the payments migration before the next review.",
        stance: "advisor",
        memoryScore: 91,
        personaStatus: "matched",
        voiceStatus: "in_bounds",
        summary: "",
        createdAt: "",
        answerReceiptID: "twin_answer_shot_001"
      )
      grounded.attachmentNames = ["board-brief.pdf"]
      grounded.sensitive = true
      twinMessages = [grounded]
    }
  }

  private func autoStartBuildForScreenshot(_ request: ScreenshotBuildRequest) async {
    try? await Task.sleep(nanoseconds: request.delayNanoseconds)
    guard !Task.isCancelled else { return }

    guard let repoID = request.resolvedRepoID(
      availableRepoIDs: Set(repos.map(\.id)),
      fallback: selectedRepoID
    ) else {
      runActionResult = RunActionOutcome(
        title: "Start run",
        status: "REFUSED",
        detail: "The requested screenshot repo is not connected. Autostart stopped instead of targeting a different checkout.",
        receiptID: ""
      )
      logger.info("Native proof build refused because its requested repo is unavailable")
      return
    }
    selectedRepoID = repoID

    var recommendation: WorkRecommendation?
    if request.fleetWidth == nil || request.proofCommands.isEmpty {
      recommendation = await classify(ask: request.intent)
    }
    let width = request.fleetWidth ?? max(1, recommendation?.width ?? 1)
    let commands = request.proofCommands.isEmpty
      ? (recommendation?.suggestedCheckCommands ?? [])
      : request.proofCommands
    logger.info("Autostarting native proof build: width=\(width, privacy: .public)")
    _ = await startRun(
      intent: request.intent,
      repoID: repoID,
      fleetWidth: width,
      allowedCommands: commands
    )
  }

  func refreshRuns() async {
    // Fold into the full-load coalescing scheme: if a full snapshot load is in
    // flight, let it deliver fresh runs rather than racing it. Crucially, a run
    // poll never bumps loadGeneration, so it can never invalidate a full load
    // mid-flight and strand the app on .loading.
    if let existing = refreshTask {
      await existing.value
      return
    }
    let generation = loadGeneration
    do {
      let projection = try await client.loadRunProjection(baseURL: Self.configuredBaseURL())
      // A full refresh started or finished while this poll was loading; its
      // snapshot wins, so drop this poll instead of clobbering fresher state.
      guard generation == loadGeneration, refreshTask == nil else { return }
      // Equality gate: the poll must not force a re-render when nothing changed.
      // The compare walks full event arrays (a streaming projection is ~180KB),
      // so run it off the main actor and only publish on a real change. An
      // empty projection is real (kernel restarted): it clears stale runs.
      let current = forgeRuns
      let currentGroups = parallelExecutionGroups
      let changed = await Task.detached(priority: .utility) {
        projection.runs != current || projection.parallelExecutionGroups != currentGroups
      }.value
      // Re-check after the hop: a full load may have landed while we compared.
      guard changed, generation == loadGeneration, refreshTask == nil else { return }
      let selectedWasRunning = selectedRun?.isRunning ?? false
      let selectedID = selectedRunID
      // Scoped publish: only run readers invalidate, not every snapshot observer.
      forgeRuns = projection.runs
      parallelExecutionGroups = projection.parallelExecutionGroups
      reloadReviewSurfacesIfSelectedRunFinished(wasRunning: selectedWasRunning, runID: selectedID)
      notifyFinishedStartedRuns(in: projection.runs)
      updateRunStateLedger(with: projection.runs)
      updateDockBadge()
    } catch {
      // Transient failure: keep showing last-good runs; the watcher retries.
      logger.info("Run refresh failed: \(error.localizedDescription, privacy: .public)")
    }
  }

  func startBuildFlow() {
    runActionResult = nil
    showStartBuild = true
  }

  // Home composer one-shot start (founder-ratified): a plain submit starts the
  // run immediately using the auto recommendation and the composer's repo chip.
  // Missions and wide fleet plans still need the guided sheet for setup and
  // plan review, so those route into the sheet with the intent prefilled.
  @discardableResult
  func submitComposerBuild(intent: String) async -> RunActionOutcome? {
    let trimmed = intent.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return nil }
    runActionResult = nil
    let recommendation = await classify(ask: trimmed)
    if let recommendation, recommendation.isMission || recommendation.width > 4 {
      pendingBuildIntent = trimmed
      startBuildFlow()
      return nil
    }
    let width = max(1, recommendation?.width ?? 1)
    let commands = recommendation?.suggestedCheckCommands ?? []
    return await startRun(
      intent: trimmed,
      repoID: selectedRepoID,
      fleetWidth: width,
      allowedCommands: commands
    )
  }

  var selectedMission: ForgeMission? {
    guard let selectedMissionID else { return nil }
    return missions.first { $0.id == selectedMissionID }
  }

  func loadMissions() async {
    if let loaded = try? await client.loadMissions(baseURL: Self.configuredBaseURL()) {
      missions = loaded
    }
  }

  // MARK: - You

  // MARK: - Feedback

  func startFeedback(prefillCrash: Bool = false) {
    feedbackResult = nil
    Task { await loadReportStatuses() }
    if prefillCrash {
      feedbackCategoryID = "blocked"
      feedbackNote = "MDx quit unexpectedly last time. What I was doing: "
    }
    showFeedback = true
    Task {
      await recordBetaTelemetry(
        "feedback_opened",
        route: "/\(selectedAppRoute.id)",
        surface: FeedbackSurfaceMap.surface(for: selectedAppRoute)
      )
    }
  }

  // MARK: - External canary continuity

  func beginCanarySession() async {
    guard !didRecordBetaSessionStart else { return }
    didRecordBetaSessionStart = true
    let recorded = await recordBetaTelemetry("session_start", route: "/home")
    if recorded {
      await recordBetaTelemetry(
        "activation_step",
        route: "/welcome",
        step: "install_completed",
        completed: true
      )
    } else {
      didRecordBetaSessionStart = false
    }
    await checkForMacUpdate()
  }

  func recordCanarySurfaceVisit(_ route: AppRoute) async {
    await recordBetaTelemetry(
      "surface_visit",
      route: "/\(route.id)",
      surface: FeedbackSurfaceMap.surface(for: route)
    )
  }

  func checkForMacUpdate() async {
    guard let macReleaseHostedOrigin, !macReleaseCheckInFlight else { return }
    macReleaseCheckInFlight = true
    defer { macReleaseCheckInFlight = false }
    do {
      let manifest = try await client.loadLatestMacRelease(hostedOrigin: macReleaseHostedOrigin)
      if let manifest,
         manifest.isNewer(thanVersion: Bundle.main.appShortVersion, build: Bundle.main.appBuildVersion) {
        availableMacRelease = manifest
      } else {
        availableMacRelease = nil
      }
    } catch {
      logger.info("Mac release check skipped: \(error.localizedDescription, privacy: .public)")
    }
  }

  func openMacUpdate() {
    guard let macReleaseHostedOrigin else { return }
    NSWorkspace.shared.open(macReleaseHostedOrigin.appending(path: "download/macos"))
  }

  func dismissMacUpdate() {
    availableMacRelease = nil
  }

  @discardableResult
  private func recordBetaTelemetry(
    _ eventKind: String,
    route: String,
    step: String? = nil,
    completed: Bool? = nil,
    surface: String? = nil
  ) async -> Bool {
    guard Self.configuredBaseURL().scheme == "https" else { return false }
    do {
      try await client.postBetaTelemetry(
        baseURL: Self.configuredBaseURL(),
        eventKind: eventKind,
        route: route,
        step: step,
        completed: completed,
        surface: surface
      )
      return true
    } catch {
      logger.info("Beta journey telemetry skipped: \(error.localizedDescription, privacy: .public)")
      return false
    }
  }

  func sendFeedback() {
    let note = feedbackNote.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !note.isEmpty, !feedbackInFlight else { return }
    feedbackInFlight = true
    let category = feedbackCategoryID
    let taxonomyClass = category == "blocked" ? "P1" : category == "bug" ? "P2" : "P3"
    let surface = FeedbackSurfaceMap.surface(for: selectedAppRoute)
    let route = selectedAppRoute.id
    Telemetry.breadcrumb("file report (\(category))", "action")
    Task { [weak self] in
      guard let self else { return }
      do {
        // Diagnostics summary rides in the note tail: metrics only, never
        // content (the kernel refuses forbidden context keys by contract).
        let diagnostics = DiagnosticsStore.shared
        let health = "\n\n-- attached by the app --\nversion \(Bundle.main.appVersionLabel) · launch \(diagnostics.launchDurationMs.map { "\(Int($0))ms" } ?? "n/a") · \(diagnostics.totalCalls) calls · \(diagnostics.errorCount) errors · \(diagnostics.hangCount) hangs · p95 \(Int(diagnostics.p95LatencyMs))ms"
        let outcome = try await self.client.postFeedback(
          baseURL: Self.configuredBaseURL(),
          surface: surface,
          route: "/\(route)",
          category: category,
          taxonomyClass: taxonomyClass,
          note: note + health,
          appVersion: Bundle.main.appVersionLabel,
          routeStatus: self.snapshot.connectionStatus == .ok ? "200" : "unreachable"
        )
        self.feedbackResult = outcome
        if outcome.succeeded, !outcome.receiptID.isEmpty {
          let report = FiledReport(
            receiptID: outcome.receiptID,
            category: category,
            note: String(note.prefix(140)),
            surface: surface,
            filedAt: Date(),
            appVersion: Bundle.main.appVersionLabel
          )
          self.filedReports.insert(report, at: 0)
          if self.filedReports.count > 50 { self.filedReports.removeLast(self.filedReports.count - 50) }
          if let data = try? JSONEncoder().encode(self.filedReports) {
            UserDefaults.standard.set(data, forKey: Self.filedReportsDefaultsKey)
          }
          self.feedbackNote = ""
          self.launchedAfterUncleanExit = false
          await self.recordBetaTelemetry(
            "activation_step",
            route: "/\(route)",
            step: "first_feedback_submitted",
            completed: true
          )
        }
      } catch {
        self.feedbackResult = RunActionOutcome(title: "Report an issue", status: "FAILED", detail: friendlyLoadError(error), receiptID: "")
      }
      self.feedbackInFlight = false
    }
  }

  func loadReportStatuses() async {
    guard let statuses = try? await client.loadFeedbackStatuses(baseURL: Self.configuredBaseURL()) else { return }
    reportStatuses = Dictionary(uniqueKeysWithValues: statuses.map { ($0.receiptID, $0) })
  }

  /// Remote repo intake: the kernel clones and manages the checkout.
  func connectRepoFromURL(_ urlString: String) {
    let trimmed = urlString.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty, let url = URL(string: trimmed) else { return }
    var name = url.lastPathComponent
    if name.hasSuffix(".git") { name = String(name.dropLast(4)) }
    let slug = name.lowercased().filter { $0.isLetter || $0.isNumber || $0 == "-" || $0 == "_" }
    Telemetry.breadcrumb("connect repo from URL \(slug)", "action")
    Task { [weak self] in
      guard let self else { return }
      do {
        let outcome = try await self.client.connectRepo(
          baseURL: Self.configuredBaseURL(),
          repoID: slug.isEmpty ? "repo" : slug,
          label: name,
          originURL: trimmed
        )
        self.runActionResult = outcome
        await self.loadRepos()
        if self.repos.contains(where: { $0.id == slug }) {
          self.selectedRepoID = slug
          await self.scoutSelectedRepo()
        }
      } catch {
        self.runActionResult = RunActionOutcome(title: "Connect a repo", status: "FAILED", detail: friendlyLoadError(error), receiptID: "")
      }
    }
  }

  func dismissCrashNotice() {
    launchedAfterUncleanExit = false
  }

  // MARK: - PR handoff

  func preparePRHandoff() {
    guard let run = selectedRun, prHandoffPhase != .loading else { return }
    let runID = run.id
    prHandoffPhase = .loading
    prHandoff = nil
    showPRHandoff = true
    prHandoffTask?.cancel()
    prHandoffTask = Task { [weak self] in
      guard let self else { return }
      do {
        let handoff = try await self.client.preparePRHandoff(baseURL: Self.configuredBaseURL(), runID: runID)
        guard !Task.isCancelled, self.selectedRunID == runID else { return }
        self.prHandoff = handoff
        self.prHandoffPhase = .loaded(Date())
      } catch {
        guard !Task.isCancelled, self.selectedRunID == runID else { return }
        self.prHandoffPhase = .failed(friendlyLoadError(error))
      }
    }
  }

  /// Repo backing the open run (for escape hatches and the compare URL).
  var selectedRunRepo: ForgeRepo? {
    guard let run = selectedRun else { return nil }
    return repos.first { $0.id == run.repo } ?? repos.first { run.repo.contains($0.id) }
  }

  /// Contract drift: connected, but several known routes are missing on this
  /// kernel. Fallback heuristic for kernels that predate /version.json.
  var contractDriftCount: Int {
    guard snapshot.connectionStatus == .ok else { return 0 }
    return snapshot.routeCards.filter { $0.status == .unavailable }.count
  }

  /// Exact handshake verdict when the kernel self-describes: is this app
  /// older than the kernel's minimum?
  var appBelowKernelMinimum: Bool {
    guard let kernelVersion else { return false }
    return kernelVersion.appIsTooOld(appVersion: Bundle.main.appVersionLabel)
  }

  func showDiagnosticsPanel() {
    if settingsShown {
      settingsShown = false
      Task { @MainActor [weak self] in
        await Task.yield()
        self?.showDiagnostics = true
      }
    } else {
      showDiagnostics = true
    }
  }

  /// Combined app health: connection + recent hangs/errors, for the sidebar
  /// indicator. Reads the condensed stored signal (not the raw event log) so
  /// observers only re-render when the health outcome actually changes.
  var appHealth: AppHealth {
    if let forced = Self.shotForcedHealth { return forced }
    let health = DiagnosticsStore.shared.health
    if snapshot.connectionStatus != .ok { return .offline }
    if health.consecutiveFailures >= 3 || health.recentHang || health.recentErrors >= 3 {
      return .degraded
    }
    return .healthy
  }

  /// Capture hook: MDX_SHOT_HEALTH=offline|degraded|healthy forces the lifecycle
  /// banner's state so the QA rail can photograph offline and degraded against a
  /// live kernel. The same inert-unless-set pattern as the other MDX_SHOT_* vars;
  /// never set in the shipping app. Because `connectionDiagnosis` derives from
  /// this property, the whole banner (tint, title, and honest diagnosis line)
  /// stays consistent with the forced state.
  static var shotForcedHealth: AppHealth? {
    switch ProcessInfo.processInfo.environment["MDX_SHOT_HEALTH"] {
    case "offline": return .offline
    case "degraded": return .degraded
    case "healthy": return .healthy
    default: return nil
    }
  }

  // MARK: - App-managed connection lifecycle (Start / Retry / Diagnose)

  /// The base URL the app is actually talking to, for an honest diagnosis line.
  var connectionBaseURL: String {
    snapshot.baseURL.absoluteString
  }

  /// The real last error from the diagnostics trail, human label plus a short
  /// detail. Nil when nothing has failed. Never a cosmetic "all good".
  var lastConnectionError: String? {
    guard let failure = DiagnosticsStore.shared.lastFailure else { return nil }
    let detail = failure.detail.trimmingCharacters(in: .whitespacesAndNewlines)
    if detail.isEmpty { return failure.label }
    return "\(failure.label): \(detail)"
  }

  /// The honest one-line diagnosis for the lifecycle banner.
  var connectionDiagnosis: String {
    let health = DiagnosticsStore.shared.health
    return AppLifecycleDiagnosis.summary(
      health: appHealth,
      baseURL: connectionBaseURL,
      consecutiveFailures: health.consecutiveFailures,
      recentHang: health.recentHang,
      lastError: lastConnectionError
    )
  }

  /// Force a fresh connection attempt against the configured base URL, and keep
  /// the background reconnect watch armed so the shell comes live on its own if
  /// the kernel starts a moment later. This is the operator-driven Retry.
  func retryConnection() async {
    logger.info("Operator retry: reconnecting to \(Self.configuredBaseURL().absoluteString, privacy: .public)")
    Telemetry.breadcrumb("connection retry", "lifecycle")
    await refresh()
    if snapshot.connectionStatus != .ok {
      startReconnectWatch()
    }
  }

  // MARK: - Central app health (batched, metrics-only)

  /// Why a rollup is being sent. Cadence is the steady-state batch; the other two
  /// are one-shot event reports.
  enum AppHealthTrigger {
    case cadence, degraded, uncleanRelaunch
  }

  /// Re-arm on every app-health input change and post ONCE when health slips
  /// from healthy to degraded/offline. `withObservationTracking` fires its
  /// `onChange` when a tracked dependency of `appHealth` is about to change; we
  /// re-read on the main actor (so we see the settled value) and re-install the
  /// tracking for the next change.
  private func observeHealthTransitions() {
    withObservationTracking {
      _ = self.appHealth
    } onChange: {
      Task { @MainActor [weak self] in
        guard let self else { return }
        self.checkHealthTransition()
        self.observeHealthTransitions()
      }
    }
  }

  private func checkHealthTransition() {
    let current = appHealth
    defer { lastObservedHealth = current }
    guard let previous = lastObservedHealth, previous == .healthy, current != .healthy else { return }
    Task { [weak self] in await self?.postAppHealthRollup(trigger: .degraded) }
  }

  /// Build the current metrics-only rollup from the local diagnostics store and
  /// POST it to the central ingest. Best-effort: any failure is swallowed so
  /// telemetry never disturbs the app. The payload is EXACTLY the server's
  /// allowlist (counts, durations, a bounded state, bare route paths); no note,
  /// body, or content ever rides along.
  func postAppHealthRollup(trigger: AppHealthTrigger) async {
    let diagnostics = DiagnosticsStore.shared
    let failing = diagnostics.failingRouteRollups(limit: 50).map { entry in
      AppHealthFailingRoute(route: entry.route, status: entry.status, count: entry.count)
    }
    let rollup = AppHealthRollupPayload(
      appVersion: Bundle.main.appVersionLabel,
      surface: "native_macos",
      sessionID: healthSessionID,
      reportedAt: ISO8601DateFormatter().string(from: Date()),
      healthState: appHealth.wireState,
      totalCalls: diagnostics.totalCalls,
      errorCount: diagnostics.errorCount,
      hangCount: diagnostics.hangCount,
      consecutiveFailures: diagnostics.consecutiveFailures,
      avgLatencyMs: Int(diagnostics.avgLatencyMs.rounded()),
      p95LatencyMs: Int(diagnostics.p95LatencyMs.rounded()),
      launchMs: Int((diagnostics.launchDurationMs ?? 0).rounded()),
      uncleanRelaunch: uncleanRelaunchThisSession,
      failingRoutes: failing
    )
    do {
      try await client.postAppHealth(baseURL: Self.configuredBaseURL(), rollup: rollup)
      logger.info("Posted app-health rollup (\(String(describing: trigger), privacy: .public))")
    } catch {
      // Best-effort: a degraded or offline app must not be disturbed by a failed
      // health post. The next cadence tick retries.
      logger.info("App-health rollup post skipped: \(error.localizedDescription, privacy: .public)")
    }
  }

  /// Load the cross-session app-health aggregate for the Diagnostics surface.
  func loadAppHealthProjection() async {
    if appHealthProjection == nil { appHealthProjectionPhase = .loading }
    do {
      let projection = try await client.loadAppHealthProjection(baseURL: Self.configuredBaseURL())
      appHealthProjection = projection
      appHealthProjectionPhase = projection.totalReports == 0 ? .empty : .loaded(Date())
    } catch {
      appHealthProjectionPhase = appHealthProjection == nil
        ? .failed(friendlyLoadError(error))
        : .stale(friendlyLoadError(error))
    }
  }

  private static func makeHealthSessionID() -> String {
    let key = "MDxInstallID"
    let installID: String
    if let existing = UserDefaults.standard.string(forKey: key) {
      installID = existing
    } else {
      installID = UUID().uuidString
      UserDefaults.standard.set(installID, forKey: key)
    }
    let short = installID.replacingOccurrences(of: "-", with: "").prefix(8)
    return "mac_\(short)_\(Int(Date().timeIntervalSince1970))"
  }

  func loadYou() async {
    let (session, clearance) = await client.loadIdentity(baseURL: Self.configuredBaseURL())
    if let session { identitySession = session }
    if let clearance { self.clearance = clearance }
  }

  func loadMemory() async {
    if memoryWorkspace == nil { memoryPhase = .loading }
    do {
      let workspace = try await client.loadMemoryWorkspace(baseURL: Self.configuredBaseURL())
      memoryWorkspace = workspace
      memoryPhase = workspace.isEmpty ? .empty : .loaded(Date())
    } catch {
      memoryPhase = memoryWorkspace == nil
        ? .failed(friendlyLoadError(error))
        : .stale(friendlyLoadError(error))
    }
  }

  func retireMemoryLesson(_ lesson: MemoryLesson, reason: String, reviewOwner: String) async {
    guard !memoryActionInFlight else { return }
    memoryActionInFlight = true
    defer { memoryActionInFlight = false }
    do {
      let outcome = try await client.retireMemoryLesson(
        baseURL: Self.configuredBaseURL(),
        lesson: lesson,
        reason: reason,
        reviewOwner: reviewOwner,
        actorID: actorID
      )
      memoryActionResult = outcome
      if outcome.succeeded { await loadMemory() }
    } catch {
      memoryActionResult = RunActionOutcome(
        title: "That’s outdated",
        status: "FAILED",
        detail: friendlyLoadError(error),
        receiptID: ""
      )
    }
  }

  // MARK: - Marketplace

  var selectedPack: MarketplacePack? { marketplacePacks.first { $0.id == selectedPackID } }

  var visibleMarketplacePacks: [MarketplacePack] {
    let query = marketplaceSearch.trimmingCharacters(in: .whitespacesAndNewlines)
    return marketplacePacks.filter { pack in
      let sectionMatch: Bool
      switch marketplaceSection {
      case .forYou, .packs: sectionMatch = true
      case .installed: sectionMatch = pack.state.installed
      case .review: sectionMatch = !pack.state.heldItems.isEmpty
      }
      guard sectionMatch, !query.isEmpty else { return sectionMatch }
      return pack.name.localizedCaseInsensitiveContains(query)
        || pack.job.localizedCaseInsensitiveContains(query)
        || pack.outcome.localizedCaseInsensitiveContains(query)
        || pack.supportedApps.joined(separator: " ").localizedCaseInsensitiveContains(query)
    }
  }

  var selectedCapability: Capability? { capabilities.first { $0.id == selectedCapabilityID } }

  var visibleCapabilities: [Capability] {
    let query = marketplaceSearch.trimmingCharacters(in: .whitespacesAndNewlines)
    return capabilities.filter { cap in
      guard marketplaceFilter.matches(cap) else { return false }
      guard !query.isEmpty else { return true }
      return cap.name.localizedCaseInsensitiveContains(query)
        || cap.summary.localizedCaseInsensitiveContains(query)
        || cap.helpsWith.localizedCaseInsensitiveContains(query)
        || cap.type.localizedCaseInsensitiveContains(query)
    }
  }

  var installedCapabilities: [Capability] { capabilities.filter { $0.installed } }

  /// Visible capabilities grouped into ordered category sections (Plugins-style browse).
  var groupedCapabilities: [(category: String, items: [Capability])] {
    let visible = visibleCapabilities
    return Capability.categoryOrder.compactMap { category in
      let items = visible.filter { $0.category == category }
      return items.isEmpty ? nil : (category, items)
    }
  }

  func loadMarketplace() async {
    if capabilities.isEmpty && marketplacePacks.isEmpty { marketplacePhase = .loading }
    do {
      async let capabilityTask = client.loadCapabilities(baseURL: Self.configuredBaseURL())
      async let packTask = client.loadMarketplacePacks(baseURL: Self.configuredBaseURL())
      let (loaded, packs) = try await (capabilityTask, packTask)
      capabilities = loaded
      marketplacePacks = packs
      marketplacePhase = packs.isEmpty ? .empty : .loaded(Date())
      if selectedPackID == nil, let first = packs.first { selectedPackID = first.id }
      if selectedCapabilityID == nil, let first = loaded.first { selectedCapabilityID = first.id }
    } catch {
      // Keep whatever is on screen; only claim failure when there is nothing.
      marketplacePhase = capabilities.isEmpty
        ? .failed(friendlyLoadError(error))
        : .stale(friendlyLoadError(error))
    }
  }

  func selectPack(_ id: String) {
    selectedPackID = id
    packActionResult = nil
    if packTrialPreview?.packID != id { packTrialPreview = nil }
  }

  func installPack(_ pack: MarketplacePack, scope: String, applications: [String]) {
    runPackAction {
      try await self.client.installPack(baseURL: Self.configuredBaseURL(), pack: pack, scope: scope, applications: applications, actorID: self.actorID)
    }
  }

  func actOnPack(_ pack: MarketplacePack, action: String, scope: String, applications: [String], version: String = "") {
    runPackAction {
      try await self.client.actOnPack(baseURL: Self.configuredBaseURL(), pack: pack, action: action, scope: scope, applications: applications, version: version, actorID: self.actorID)
    }
  }

  private func runPackAction(_ action: @escaping () async throws -> RunActionOutcome) {
    guard !packActionInFlight else { return }
    packActionInFlight = true
    packActionResult = nil
    Task { [weak self] in
      guard let self else { return }
      do {
        let outcome = try await action()
        self.packActionResult = outcome
        if outcome.succeeded {
          let keep = self.selectedPackID
          await self.loadMarketplace()
          self.selectedPackID = keep
        }
      } catch {
        self.packActionResult = RunActionOutcome(title: "Pack", status: "FAILED", detail: error.localizedDescription, receiptID: "")
      }
      self.packActionInFlight = false
    }
  }

  func tryPack(_ pack: MarketplacePack) {
    guard !packActionInFlight else { return }
    packActionInFlight = true
    Task { [weak self] in
      guard let self else { return }
      do {
        self.packTrialPreview = try await self.client.tryPack(baseURL: Self.configuredBaseURL(), pack: pack, actorID: self.actorID)
      } catch {
        self.packActionResult = RunActionOutcome(title: "Safe trial", status: "FAILED", detail: error.localizedDescription, receiptID: "")
      }
      self.packActionInFlight = false
    }
  }

  func selectCapability(_ id: String) {
    selectedCapabilityID = id
    if tryPreview?.capabilityID != id { tryPreview = nil }
    capabilityActionResult = nil
  }

  func installCapability(_ capability: Capability, scope: String) {
    Telemetry.breadcrumb("add “\(capability.name)” (\(scope))", "action")
    runCapabilityAction {
      try await self.client.installCapability(baseURL: Self.configuredBaseURL(), capabilityID: capability.id, scope: scope, actorID: self.actorID)
    }
  }

  func removeCapability(_ capability: Capability, scope: String) {
    runCapabilityAction {
      try await self.client.revokeCapability(baseURL: Self.configuredBaseURL(), capabilityID: capability.id, scope: scope, actorID: self.actorID)
    }
  }

  func decideCapability(_ capability: Capability, decision: String, scope: String, readOnly: Bool = false) {
    runCapabilityAction {
      try await self.client.approveCapability(baseURL: Self.configuredBaseURL(), capabilityID: capability.id, decision: decision, scope: scope, readOnly: readOnly, actorID: self.actorID)
    }
  }

  private func runCapabilityAction(_ action: @escaping () async throws -> RunActionOutcome) {
    guard !capabilityActionInFlight else { return }
    capabilityActionInFlight = true
    capabilityActionResult = nil
    Task { [weak self] in
      guard let self else { return }
      do {
        let outcome = try await action()
        self.capabilityActionResult = outcome
        if outcome.succeeded {
          let keep = self.selectedCapabilityID
          await self.loadMarketplace()
          self.selectedCapabilityID = keep
        }
      } catch {
        self.capabilityActionResult = RunActionOutcome(title: "Capability", status: "FAILED", detail: error.localizedDescription, receiptID: "")
      }
      self.capabilityActionInFlight = false
    }
  }

  func tryCapability(_ capability: Capability) {
    guard !capabilityActionInFlight else { return }
    capabilityActionInFlight = true
    Task { [weak self] in
      guard let self else { return }
      do {
        self.tryPreview = try await self.client.tryCapability(baseURL: Self.configuredBaseURL(), capabilityID: capability.id, actorID: self.actorID)
      } catch {
        self.capabilityActionResult = RunActionOutcome(title: "Try safely", status: "FAILED", detail: error.localizedDescription, receiptID: "")
      }
      self.capabilityActionInFlight = false
    }
  }

  // MARK: - Message

  var selectedMessageChannelID: String {
    messageLane.channelID ?? ""
  }

  var selectedMessageChannel: MessageChannel? {
    messageChannels.first { $0.id == selectedMessageChannelID }
  }

  var visibleThreadMessages: [ThreadMessage] {
    let channelID = selectedMessageChannelID
    let query = messageSearch.trimmingCharacters(in: .whitespacesAndNewlines)
    return threadMessages.filter { message in
      guard !message.isReply, !message.isReaction else { return false }
      guard channelID.isEmpty || message.channelID == channelID else { return false }
      return query.isEmpty
        || message.body.localizedCaseInsensitiveContains(query)
        || message.actorName.localizedCaseInsensitiveContains(query)
    }
  }

  func replies(to message: ThreadMessage) -> [ThreadMessage] {
    threadMessages.filter { $0.parentReceiptID == message.receiptID }
  }

  func reactions(to message: ThreadMessage) -> [MessageReaction] {
    MessageReaction.fold(
      messages: threadMessages,
      parentReceiptID: message.receiptID,
      viewerActorID: actorID
    )
  }

  var messageCatchUp: MessageCatchUp {
    let messages = visibleThreadMessages.suffix(12)
    guard !messages.isEmpty else { return .empty }
    let people = Set(messages.map(\.actorName))
    let highlights = messages.suffix(4).map { message in
      let body = message.body.trimmingCharacters(in: .whitespacesAndNewlines)
      return "\(message.actorName): \(String(body.prefix(140)))"
    }
    return MessageCatchUp(
      headline: "\(messages.count) recent updates from \(people.count) participant\(people.count == 1 ? "" : "s")",
      highlights: highlights,
      participantCount: people.count,
      messageCount: messages.count,
      latestSequence: messages.map(\.sequence).max() ?? 0
    )
  }

  func selectMessageChannel(_ id: String) {
    messageLane = .channel(id)
    UserDefaults.standard.set(id, forKey: "mdx.message.channel")
    messageCatchUpShown = false
    openMessageThreadReceiptID = nil
    messageReplyDraft = ""
  }

  var visibleActivity: [ActivityItem] {
    var base: [ActivityItem]
    if let area = messageLane.area {
      base = activityItems.filter { $0.area == area || $0.channelID == area }
    } else {
      base = activityItems
    }
    let query = messageSearch.trimmingCharacters(in: .whitespacesAndNewlines)
    if !query.isEmpty {
      base = base.filter {
        $0.headline.localizedCaseInsensitiveContains(query)
          || $0.actor.localizedCaseInsensitiveContains(query)
          || $0.detail.localizedCaseInsensitiveContains(query)
      }
    }
    return Array(base.prefix(250))
  }

  func loadMessage() async {
    if activityItems.isEmpty && actionCards.isEmpty && threadMessages.isEmpty { messagePhase = .loading }
    let base = Self.configuredBaseURL()
    async let activityResult = client.loadActivity(baseURL: base)
    async let cardsResult = client.loadActionCards(baseURL: base)
    async let presenceResult = client.loadPresence(baseURL: base)
    async let channelsResult = client.loadMessageChannels(baseURL: base)
    async let messagesResult = client.loadThreadMessages(baseURL: base)

    var failure: (any Error)?
    var succeeded = false
    var messagesLoaded = false
    do {
      let (areas, items) = try await activityResult
      activityAreas = areas
      activityItems = items
      succeeded = true
    } catch { failure = error }
    do {
      actionCards = try await cardsResult
      succeeded = true
    } catch { failure = error }
    do {
      messageChannels = try await channelsResult
      succeeded = true
    } catch { failure = error }
    do {
      threadMessages = try await messagesResult
      messagesLoaded = true
      succeeded = true
    } catch { failure = error }
    let knownChannelIDs = Set(messageChannels.map(\.id))
    let derivedChannels = Set(threadMessages.map(\.channelID))
      .subtracting(knownChannelIDs)
      .filter { !$0.isEmpty }
      .sorted()
      .map {
        MessageChannel(
          id: $0,
          name: humanLabel($0),
          description: "",
          topic: "",
          kind: "channel",
          visibility: "public",
          status: "active",
          memberCount: 0
        )
      }
    messageChannels.append(contentsOf: derivedChannels)
    if !messageChannels.contains(where: { $0.id == selectedMessageChannelID }), let first = messageChannels.first {
      selectMessageChannel(first.id)
    }
    if let (activeNow, people) = try? await presenceResult {
      activeNowCount = activeNow
      presencePeople = people
    }

    // One transient timeout must not wipe the feed and the inbox to empty.
    if succeeded {
      messagePhase = (activityItems.isEmpty && actionCards.isEmpty && threadMessages.isEmpty) ? .empty : .loaded(Date())
      let queuedBeforeFlush = messageOutbox.count
      if messagesLoaded { await flushMessageOutbox() }
      if messageOutbox.count < queuedBeforeFlush,
         let refreshed = try? await client.loadThreadMessages(baseURL: base) {
        threadMessages = refreshed
      }
    } else if let failure {
      messagePhase = (activityItems.isEmpty && actionCards.isEmpty && threadMessages.isEmpty)
        ? .failed(friendlyLoadError(failure))
        : .stale(friendlyLoadError(failure))
    }
  }

  func decideActionCard(_ card: ActionCard, verdict: String) {
    guard verdictInFlightCardID == nil else { return }
    Telemetry.breadcrumb("message verdict \(verdict)", "action")
    verdictInFlightCardID = card.id
    verdictResult = nil
    Task { [weak self] in
      guard let self else { return }
      do {
        let outcome = try await self.client.postActionVerdict(
          baseURL: Self.configuredBaseURL(),
          card: card,
          verdict: verdict,
          note: "",
          actorID: self.actorID
        )
        self.verdictResult = outcome
        if outcome.succeeded && !outcome.receiptID.isEmpty {
          self.actionCards.removeAll { $0.id == card.id }
        }
      } catch {
        self.verdictResult = RunActionOutcome(title: "\(verdict.capitalized) action", status: "FAILED", detail: friendlyLoadError(error), receiptID: "")
      }
      self.verdictInFlightCardID = nil
    }
  }

  func sendMessageDraft() {
    let text = messageDraft.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !text.isEmpty, !messageSendInFlight else { return }
    if snapshot.connectionStatus != .ok {
      let queued = QueuedThreadMessage(
        id: "queued_\(UUID().uuidString.lowercased())",
        messageID: "mac_msg_\(UUID().uuidString.prefix(12).lowercased())",
        channelID: selectedMessageChannelID.isEmpty ? "local-ops" : selectedMessageChannelID,
        body: text,
        queuedAt: Date()
      )
      messageOutbox.append(queued)
      persistMessageOutbox()
      messageDraft = ""
      messageSendResult = RunActionOutcome(
        title: "Queue message",
        status: "QUEUED",
        detail: "Saved on this Mac. It will send after MDx reconnects.",
        receiptID: ""
      )
      return
    }
    messageSendInFlight = true
    messageSendResult = nil
    Task { [weak self] in
      guard let self else { return }
      do {
        let outcome = try await self.client.sendThreadMessage(
          baseURL: Self.configuredBaseURL(),
          text: text,
          actorID: self.actorID,
          channelID: self.selectedMessageChannelID.isEmpty ? "local-ops" : self.selectedMessageChannelID
        )
        self.messageSendResult = outcome
        if outcome.succeeded && !outcome.receiptID.isEmpty {
          self.messageDraft = ""
          await self.loadMessage()
        }
      } catch {
        self.messageSendResult = RunActionOutcome(title: "Send message", status: "FAILED", detail: friendlyLoadError(error), receiptID: "")
      }
      self.messageSendInFlight = false
    }
  }

  private static func loadMessageOutbox() -> [QueuedThreadMessage] {
    guard let data = UserDefaults.standard.data(forKey: "mdx.message.native.outbox") else { return [] }
    return (try? JSONDecoder().decode([QueuedThreadMessage].self, from: data)) ?? []
  }

  private func persistMessageOutbox() {
    guard let data = try? JSONEncoder().encode(messageOutbox) else { return }
    UserDefaults.standard.set(data, forKey: "mdx.message.native.outbox")
  }

  private func flushMessageOutbox() async {
    guard snapshot.connectionStatus == .ok,
          !messageOutbox.isEmpty,
          !messageOutboxFlushing,
          Date() >= messageOutboxRetryAfter
    else { return }
    messageOutboxFlushing = true
    defer { messageOutboxFlushing = false }
    for queued in messageOutbox {
      // The projection is read before any retry. If a response was lost after
      // the kernel recorded the write, retire the queue item without resending.
      var alreadyRecorded = threadMessages.contains {
        $0.messageID == queued.deliveryMessageID
      }
      if !alreadyRecorded {
        alreadyRecorded = await reconcileQueuedMessage(queued)
      }
      if alreadyRecorded {
        messageOutbox.removeAll { $0.id == queued.id }
        persistMessageOutbox()
        continue
      }
      do {
        let outcome = try await client.sendThreadMessage(
          baseURL: Self.configuredBaseURL(),
          text: queued.body,
          actorID: actorID,
          channelID: queued.channelID,
          messageID: queued.deliveryMessageID
        )
        if outcome.succeeded && !outcome.receiptID.isEmpty {
          messageOutbox.removeAll { $0.id == queued.id }
          persistMessageOutbox()
          continue
        }
        // A malformed success response is ambiguous. Reconcile by stable id
        // before any future retry, then pause with visible feedback.
        if await reconcileQueuedMessage(queued) {
          messageOutbox.removeAll { $0.id == queued.id }
          persistMessageOutbox()
          continue
        }
        messageOutboxRetryAfter = Date().addingTimeInterval(30)
        messageSendResult = RunActionOutcome(
          title: "Queue paused",
          status: "QUEUED",
          detail: "\(messageOutbox.count) message\(messageOutbox.count == 1 ? "" : "s") remain saved on this Mac. MDx will reconcile before retrying.",
          receiptID: ""
        )
        break
      } catch {
        if await reconcileQueuedMessage(queued) {
          messageOutbox.removeAll { $0.id == queued.id }
          persistMessageOutbox()
          continue
        }
        messageOutboxRetryAfter = Date().addingTimeInterval(30)
        messageSendResult = RunActionOutcome(
          title: "Queue paused",
          status: "QUEUED",
          detail: "Delivery could not be confirmed. \(messageOutbox.count) message\(messageOutbox.count == 1 ? "" : "s") remain saved on this Mac.",
          receiptID: ""
        )
        break
      }
    }
  }

  private func reconcileQueuedMessage(_ queued: QueuedThreadMessage) async -> Bool {
    guard let projected = try? await client.loadThreadMessages(
      baseURL: Self.configuredBaseURL(),
      channelID: queued.channelID,
      limit: 500
    ), projected.contains(where: { $0.messageID == queued.deliveryMessageID }) else {
      return false
    }
    var merged: [String: ThreadMessage] = [:]
    for message in threadMessages { merged[message.id] = message }
    for message in projected { merged[message.id] = message }
    threadMessages = merged.values.sorted { $0.sequence < $1.sequence }
    return true
  }

  func sendMessageReply(to parent: ThreadMessage) {
    let text = messageReplyDraft.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !text.isEmpty, !messageReplyInFlight else { return }
    messageReplyInFlight = true
    messageReplyResult = nil
    Task { [weak self] in
      guard let self else { return }
      do {
        let outcome = try await self.client.sendThreadMessage(
          baseURL: Self.configuredBaseURL(),
          text: text,
          actorID: self.actorID,
          channelID: parent.channelID,
          threadID: "reply:\(parent.receiptID)"
        )
        self.messageReplyResult = outcome
        if outcome.succeeded && !outcome.receiptID.isEmpty {
          self.messageReplyDraft = ""
          await self.loadMessage()
        }
      } catch {
        self.messageReplyResult = RunActionOutcome(title: "Send reply", status: "FAILED", detail: friendlyLoadError(error), receiptID: "")
      }
      self.messageReplyInFlight = false
    }
  }

  func react(to parent: ThreadMessage, emoji: String) {
    guard !emoji.isEmpty, messageReactionInFlightReceiptID == nil else { return }
    let removing = reactions(to: parent).first { $0.emoji == emoji }?.mine == true
    let action = removing ? "Remove reaction" : "Add reaction"
    messageReactionInFlightReceiptID = parent.receiptID
    messageReactionResultParentReceiptID = parent.receiptID
    messageReactionResult = nil
    Task { [weak self] in
      guard let self else { return }
      do {
        let outcome = try await self.client.sendThreadMessage(
          baseURL: Self.configuredBaseURL(),
          text: "\(removing ? "-" : "+"):\(emoji)",
          actorID: self.actorID,
          channelID: parent.channelID,
          threadID: "react:\(parent.receiptID)"
        )
        self.messageReactionResult = RunActionOutcome(
          title: action,
          status: outcome.status,
          detail: outcome.detail,
          receiptID: outcome.receiptID
        )
        if outcome.succeeded && !outcome.receiptID.isEmpty { await self.loadMessage() }
      } catch {
        self.messageReactionResult = RunActionOutcome(
          title: action,
          status: "FAILED",
          detail: friendlyLoadError(error),
          receiptID: ""
        )
      }
      self.messageReactionInFlightReceiptID = nil
    }
  }

  // MARK: - Pages

  var selectedPage: Page? { pages.first { $0.id == selectedPageID } }

  var activePageReview: PageReviewRequest? {
    guard !pagePublishDraft.draftID.isEmpty else { return nil }
    let documentID = pagePublishDraft.resolvedDocumentID()
    return pageReviewRequests.last {
      $0.documentID == documentID && $0.draftID == pagePublishDraft.draftID
    }
  }

  var activePageReviewCoversDraft: Bool {
    guard let review = activePageReview,
          review.isApproved,
          !review.decisionReceiptID.isEmpty,
          let recorded = locallySavedPageDrafts()[pagePublishDraft.draftID]
    else { return false }
    return review.sourceDraftReceiptID == pagePublishDraft.draftReceiptID &&
      recorded.draftReceiptID == pagePublishDraft.draftReceiptID &&
      recorded.documentID == pagePublishDraft.documentID &&
      recorded.title == pagePublishDraft.title &&
      recorded.body == pagePublishDraft.body &&
      recorded.pageType == pagePublishDraft.pageType &&
      recorded.visibility == pagePublishDraft.visibility
  }

  var previousPageReview: PageReviewRequest? {
    let documentID = pagePublishDraft.resolvedDocumentID()
    return pageReviewRequests.last {
      $0.documentID == documentID && $0.draftID != pagePublishDraft.draftID
    }
  }

  var pageDraftCount: Int {
    visiblePageDrafts.count
  }

  var pageReviewCount: Int { pageReviewRequests.filter(\.isPending).count }

  var visiblePageDrafts: [PageDraftRecord] {
    var seen = Set<String>()
    let published = Set(pages.filter { $0.state == "published" }.map(\.id))
    return pageDrafts.reversed().filter {
      !published.contains($0.documentID) && seen.insert($0.documentID).inserted
    }
  }

  /// The rail contents: search results (in server rank order) when a query is
  /// active, otherwise the whole library.
  var visiblePages: [Page] {
    guard let ids = pageSearchIDs else { return pages }
    let byID = Dictionary(uniqueKeysWithValues: pages.map { ($0.id, $0) })
    return ids.compactMap { byID[$0] }
  }

  func loadPages() async {
    if pages.isEmpty { pagesPhase = .loading }
    do {
      async let pagesTask = client.loadPages(baseURL: Self.configuredBaseURL())
      async let draftsTask = try? client.loadPageDrafts(baseURL: Self.configuredBaseURL())
      async let reviewsTask = try? client.loadPageReviewRequests(baseURL: Self.configuredBaseURL())
      let loaded = try await pagesTask
      pages = loaded
      pageDrafts = await draftsTask ?? []
      pageReviewRequests = await reviewsTask ?? []
      pagesPhase = loaded.isEmpty ? .empty : .loaded(Date())
      if let captureTitle = ShotOverrides.pageTitle(),
         let capturePage = loaded.first(where: {
           $0.title.compare(captureTitle, options: [.caseInsensitive, .diacriticInsensitive]) == .orderedSame
         }) {
        selectPage(capturePage.id)
      } else if selectedPageID == nil, let first = loaded.first {
        selectPage(first.id)
      }
    } catch {
      pagesPhase = pages.isEmpty
        ? .failed(friendlyLoadError(error))
        : .stale(friendlyLoadError(error))
    }
  }

  func selectPage(_ id: String) {
    selectedPageID = id
    selectedPageBody = nil
    pageBodyPhase = .loading
    Task { [weak self] in
      guard let self else { return }
      let body = await self.client.loadPageBody(baseURL: Self.configuredBaseURL(), documentID: id)
      guard self.selectedPageID == id else { return }
      self.selectedPageBody = body
      self.pageBodyPhase = body == nil ? .failed("This page has no readable body yet.") : .loaded(Date())
    }
  }

  func startNewPage() {
    pagePublishDraft = PagePublishDraft()
    pagePublishResult = nil
    showPagePublish = true
  }

  func openPageDraft(_ record: PageDraftRecord) {
    let localDraft = locallySavedPageDrafts()[record.id]
    pagePublishDraft = PagePublishDraft(
      documentID: record.documentID,
      title: record.title,
      body: localDraft?.body ?? "",
      pageType: localDraft?.pageType ?? "knowledge",
      visibility: localDraft?.visibility ?? "tenant_only",
      isEdit: pages.contains { $0.id == record.documentID },
      draftID: record.id,
      draftReceiptID: record.receiptID
    )
    pagePublishResult = RunActionOutcome(
      title: "Open draft",
      status: localDraft == nil ? "BODY_UNAVAILABLE" : "SAVED",
      detail: localDraft == nil
        ? "This draft record and receipt are available, but its authored body is not exposed by the current read contract and was saved on another device."
        : "Restored the device-local words linked to this recorded draft.",
      receiptID: record.receiptID
    )
    showPagePublish = true
  }

  func startEditPage() {
    guard let page = selectedPage else { return }
    pagePublishDraft = PagePublishDraft(
      documentID: page.id,
      title: page.title,
      body: selectedPageBody?.body ?? "",
      pageType: "knowledge",
      visibility: page.visibility.isEmpty ? "tenant_only" : page.visibility,
      isEdit: true
    )
    pagePublishResult = nil
    showPagePublish = true
  }

  func publishPage() {
    guard !pagePublishInFlight,
          pagePublishDraft.isValid,
          activePageReviewCoversDraft,
          let approvalDecisionReceiptID = activePageReview?.decisionReceiptID
    else { return }
    let draft = pagePublishDraft
    Telemetry.breadcrumb(draft.isEdit ? "update page" : "publish page", "action")
    pagePublishInFlight = true
    pagePublishResult = nil
    Task { [weak self] in
      guard let self else { return }
      do {
        let outcome = try await self.client.publishPage(
          baseURL: Self.configuredBaseURL(),
          draft: draft,
          approvalDecisionReceiptID: approvalDecisionReceiptID,
          actorID: self.actorID
        )
        self.pagePublishResult = outcome
        if outcome.succeeded && !outcome.receiptID.isEmpty {
          let docID = draft.resolvedDocumentID()
          self.forgetPageDrafts(documentID: docID)
          await self.loadPages()
          self.selectPage(docID)
          self.showPagePublish = false
        }
      } catch {
        self.pagePublishResult = RunActionOutcome(title: "Publish page", status: "FAILED", detail: friendlyLoadError(error), receiptID: "")
      }
      self.pagePublishInFlight = false
    }
  }

  func savePageDraft() {
    guard !pagePublishInFlight, pagePublishDraft.isValid else { return }
    let draft = pagePublishDraft
    pagePublishInFlight = true
    pagePublishResult = nil
    Task { [weak self] in
      guard let self else { return }
      do {
        let (saved, outcome) = try await self.client.savePageDraft(
          baseURL: Self.configuredBaseURL(),
          draft: draft,
          actorID: self.actorID
        )
        self.pagePublishDraft = saved
        self.pagePublishResult = outcome
        if outcome.succeeded { self.rememberPageDraft(saved) }
        self.pageDrafts = (try? await self.client.loadPageDrafts(baseURL: Self.configuredBaseURL())) ?? self.pageDrafts
        self.pageReviewRequests = (try? await self.client.loadPageReviewRequests(baseURL: Self.configuredBaseURL())) ?? self.pageReviewRequests
      } catch {
        self.pagePublishResult = RunActionOutcome(title: "Save draft", status: "FAILED", detail: friendlyLoadError(error), receiptID: "")
      }
      self.pagePublishInFlight = false
    }
  }

  private static let pageDraftDefaultsKey = "mdx.pages.native-drafts.v1"

  private func locallySavedPageDrafts() -> [String: PagePublishDraft] {
    guard let data = UserDefaults.standard.data(forKey: Self.pageDraftDefaultsKey),
          let drafts = try? JSONDecoder().decode([String: PagePublishDraft].self, from: data)
    else { return [:] }
    return drafts
  }

  private func rememberPageDraft(_ draft: PagePublishDraft) {
    guard !draft.draftID.isEmpty else { return }
    var drafts = locallySavedPageDrafts()
    drafts = drafts.filter { $0.value.documentID != draft.documentID }
    drafts[draft.draftID] = draft
    if let data = try? JSONEncoder().encode(drafts) {
      UserDefaults.standard.set(data, forKey: Self.pageDraftDefaultsKey)
    }
  }

  private func forgetPageDrafts(documentID: String) {
    var drafts = locallySavedPageDrafts()
    drafts = drafts.filter { $0.value.documentID != documentID }
    if let data = try? JSONEncoder().encode(drafts) {
      UserDefaults.standard.set(data, forKey: Self.pageDraftDefaultsKey)
    }
  }

  func requestPageReview() {
    guard !pagePublishInFlight,
          pagePublishDraft.isValid,
          !pagePublishDraft.draftReceiptID.isEmpty,
          activePageReview == nil
    else { return }
    let draft = pagePublishDraft
    pagePublishInFlight = true
    pagePublishResult = nil
    Task { [weak self] in
      guard let self else { return }
      do {
        let outcome = try await self.client.requestPageReview(
          baseURL: Self.configuredBaseURL(),
          draft: draft,
          actorID: self.actorID
        )
        self.pagePublishResult = outcome
        self.pageReviewRequests = (try? await self.client.loadPageReviewRequests(baseURL: Self.configuredBaseURL())) ?? self.pageReviewRequests
      } catch {
        self.pagePublishResult = RunActionOutcome(title: "Ask for review", status: "FAILED", detail: friendlyLoadError(error), receiptID: "")
      }
      self.pagePublishInFlight = false
    }
  }

  func decidePageReview(approve: Bool) {
    guard !pagePublishInFlight, let request = activePageReview, request.isPending else { return }
    pagePublishInFlight = true
    pagePublishResult = nil
    Task { [weak self] in
      guard let self else { return }
      do {
        let outcome = try await self.client.decidePageReview(
          baseURL: Self.configuredBaseURL(),
          request: request,
          approve: approve,
          actorID: self.actorID
        )
        self.pagePublishResult = outcome
        self.pageReviewRequests = (try? await self.client.loadPageReviewRequests(baseURL: Self.configuredBaseURL())) ?? self.pageReviewRequests
      } catch {
        self.pagePublishResult = RunActionOutcome(title: approve ? "Approve page" : "Send back", status: "FAILED", detail: friendlyLoadError(error), receiptID: "")
      }
      self.pagePublishInFlight = false
    }
  }

  func runPageSearch(_ query: String) {
    pageSearchTask?.cancel()
    let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else {
      pageSearchIDs = nil
      return
    }
    pageSearchTask = Task { [weak self] in
      guard let self else { return }
      try? await Task.sleep(nanoseconds: 250_000_000)
      if Task.isCancelled { return }
      let hits = await self.client.searchPages(baseURL: Self.configuredBaseURL(), query: trimmed)
      if Task.isCancelled { return }
      self.pageSearchIDs = hits.map(\.documentID)
    }
  }

  // MARK: - Twin

  var twinStance: TwinStance { TwinStance.named(twinStanceID) }

  func loadTwin() async {
    guard let messages = try? await client.loadTwin(baseURL: Self.configuredBaseURL(), sessionID: twinSessionID) else { return }
    twinMessages = messages
  }

  func loadTwinConversations() async {
    if let loaded = try? await client.loadTwinConversations(baseURL: Self.configuredBaseURL()) {
      twinConversations = loaded
    }
    // Mark the first fetch settled even when it fails (offline): the rail must
    // resolve its skeleton to an honest empty state, not skeleton forever.
    twinConversationsLoaded = true
  }

  func newTwinConversation() {
    cancelTwinStream()
    twinSessionID = "native_twin_" + UUID().uuidString.prefix(8)
    twinMessages = []
    twinAttachments = []
    twinAttachError = nil
  }

  func saveTwinProjectContext() {
    UserDefaults.standard.set(twinProjectName.trimmingCharacters(in: .whitespacesAndNewlines), forKey: "mdx.twin.project.name")
    UserDefaults.standard.set(twinProjectInstructions.trimmingCharacters(in: .whitespacesAndNewlines), forKey: "mdx.twin.project.instructions")
  }

  func switchTwinConversation(_ sessionID: String) {
    guard sessionID != twinSessionID else { return }
    cancelTwinStream()
    twinSessionID = sessionID
    twinMessages = []
    twinAttachments = []
    twinAttachError = nil
    Task { await loadTwin() }
  }

  /// Tear down any in-flight stream when leaving a conversation, so the old
  /// stream can neither block sending in the new one nor enrich against it.
  private func cancelTwinStream() {
    twinStreamTask?.cancel()
    twinStreamTask = nil
    twinSending = false
  }

  /// Parse a PDF locally and bind it to this Twin session as private context.
  func attachTwinPDF(url: URL) {
    guard !twinAttaching else { return }
    twinAttachError = nil
    let name = url.lastPathComponent
    let sensitive = twinSensitive
    twinAttaching = true
    Task { [weak self] in
      guard let self else { return }
      let data: Data
      do {
        data = try await Task.detached(priority: .userInitiated) {
          let needsScope = url.startAccessingSecurityScopedResource()
          defer { if needsScope { url.stopAccessingSecurityScopedResource() } }
          return try Data(contentsOf: url)
        }.value
      } catch {
        self.twinAttachError = "Could not read \(name)."
        self.twinAttaching = false
        return
      }
      do {
        var attachment = try await self.client.attachPDF(
          baseURL: Self.configuredBaseURL(),
          sessionID: self.twinSessionID,
          actorID: self.actorID,
          fileName: name,
          data: data,
          sensitive: sensitive
        )
        attachment.localURL = url
        self.twinAttachments.append(attachment)
      } catch {
        self.twinAttachError = error.localizedDescription
      }
      self.twinAttaching = false
    }
  }

  func removeTwinAttachment(_ id: String) {
    twinAttachments.removeAll { $0.id == id }
  }

  func sendTwin(_ text: String) {
    let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
    let attachments = twinAttachments
    let names = attachments.map(\.name)
    guard (!trimmed.isEmpty || !names.isEmpty), !twinSending else { return }
    twinSending = true
    let prompt = trimmed.isEmpty ? "Read the attached file and tell me what matters." : trimmed
    // The extracted text rides inline as document_context so the live answer
    // grounds on the PDF; the durable governed record already landed on attach.
    let documentContext = attachments
      .map { "## \($0.name)\n\(MDxRouteClient.boundedDocumentContext($0.extractedText))" }
      .joined(separator: "\n\n")
    let documentName = names.joined(separator: ", ")
    let pendingID = "pending_\(twinMessages.count)_\(abs(prompt.hashValue))"
    let sensitive = twinSensitive
    let sessionID = twinSessionID
    twinLocalActivityTimes[sessionID] = Date()
    let stanceID = twinStanceID
    twinMessages.append(.pendingMessage(id: pendingID, prompt: prompt, stance: stanceID, attachmentNames: names, sensitive: sensitive))
    Telemetry.breadcrumb("ask Twin (\(twinStanceID))", "action")
    twinAttachments = []
    twinStreamTask = Task { [weak self] in
      await self?.runTwinStream(pendingID: pendingID, sessionID: sessionID, stanceID: stanceID, text: prompt, sensitive: sensitive, documentContext: documentContext, documentName: documentName)
    }
  }

  func stopTwin() {
    twinStreamTask?.cancel()
    twinStreamTask = nil
    if let index = twinMessages.firstIndex(where: { $0.pending }) {
      if twinMessages[index].answer.isEmpty {
        twinMessages[index].answer = "Stopped."
      }
      twinMessages[index].pending = false
    }
    twinSending = false
  }

  func regenerateLastTwin() {
    guard !twinSending, let last = twinMessages.last else { return }
    let prompt = last.prompt
    twinMessages.removeLast()
    sendTwin(prompt)
  }

  private func runTwinStream(pendingID: String, sessionID: String, stanceID: String, text: String, sensitive: Bool, documentContext: String = "", documentName: String = "") async {
    func update(_ mutate: (inout TwinMessage) -> Void) {
      if let index = twinMessages.firstIndex(where: { $0.id == pendingID }) {
        mutate(&twinMessages[index])
      }
    }

    let probe = StreamProbe("twin stream")
    let started = DispatchTime.now()
    var outcome = "completed"
    var streamOK = true
    // Did a terminal event (done/guarded/failed) arrive? If the socket just
    // closes mid-answer, the transcript would otherwise keep a fluent-looking
    // but truncated answer with no sign it was cut off.
    var sawTerminal = false
    var fallbackReason: String?
    let companyContext = await client.loadTwinCompanyContext(baseURL: Self.configuredBaseURL())
    update { $0.companySources = companyContext.sources }
    // Tokens arrive at model speed; publishing each one re-renders the
    // transcript per token. Coalesce to ~20 UI flushes per second.
    let coalescer = StreamCoalescer<String>(intervalMs: 50) { [weak self] tokens in
      guard let self, let index = self.twinMessages.firstIndex(where: { $0.id == pendingID }) else { return }
      self.twinMessages[index].answer += tokens.joined()
    }
    let projectContext = [
      twinProjectName.isEmpty ? "" : "Active project: \(twinProjectName)",
      twinProjectInstructions.isEmpty ? "" : "Project instructions: \(twinProjectInstructions)"
    ].filter { !$0.isEmpty }.joined(separator: "\n")
    let worldContext = [companyContext.text, projectContext].filter { !$0.isEmpty }.joined(separator: "\n\n")
    for await event in client.streamTwin(baseURL: Self.configuredBaseURL(), sessionID: sessionID, stance: stanceID, text: text, sensitive: sensitive, documentContext: documentContext, documentName: documentName, worldContext: worldContext) {
      if Task.isCancelled { break }
      switch event {
      case .delta(let token):
        probe.tick()
        coalescer.add(token)
      case .fallback(let reason):
        fallbackReason = reason
        coalescer.drain()
        break
      case .done(let receiptID, _, let firstTokenMs, let providerDurationMs, let memorySourceCount, let trustedSourceCount):
        sawTerminal = true
        coalescer.drain()
        update {
          $0.answerReceiptID = receiptID
          $0.memorySourceCount = memorySourceCount
          $0.trustedSourceCount = trustedSourceCount
          if providerDurationMs > 0 { $0.durationMs = providerDurationMs }
        }
        if firstTokenMs > 0 {
          Telemetry.breadcrumb("Twin first token \(Int(firstTokenMs)) ms", "latency")
        }
      case .guarded(let message):
        sawTerminal = true
        coalescer.drain()
        outcome = "guarded"
        update { $0.answer = message }
      case .failed(let reason):
        sawTerminal = true
        coalescer.drain()
        outcome = "failed"
        streamOK = false
        update {
          if $0.answer.isEmpty {
            $0.answer = "Twin could not answer: \(reason)"
          } else {
            // Mid-answer failure: keep what streamed, but say it was cut off.
            $0.answer += "\n\nAnswer was cut off: \(reason)"
          }
        }
      }
      if fallbackReason != nil { break }
    }
    coalescer.drain()

    if !Task.isCancelled, let fallbackReason {
      do {
        let fallback = try await client.sendTwin(
          baseURL: Self.configuredBaseURL(),
          sessionID: sessionID,
          stance: stanceID,
          text: text,
          sensitive: sensitive,
          documentContext: documentContext,
          documentName: documentName,
          worldContext: worldContext
        )
        sawTerminal = true
        update { message in
          message.answer = fallback.answer
          message.answerReceiptID = fallback.answerReceiptID
          message.memorySourceCount = fallback.memorySourceCount
          message.trustedSourceCount = fallback.trustedSourceCount
          message.trustedSourceIDs = fallback.trustedSourceIDs
        }
        Telemetry.breadcrumb("Twin used governed fallback (\(fallbackReason))", "route")
      } catch {
        sawTerminal = true
        outcome = "fallback_failed"
        streamOK = false
        update { $0.answer = "Twin could not answer: \(error.localizedDescription)" }
      }
    }

    // Socket closed without any terminal event, mid-answer: mark the cutoff so a
    // truncated answer is never presented as if it finished.
    if !Task.isCancelled, !sawTerminal {
      update {
        if $0.answer.isEmpty {
          $0.answer = "Twin could not answer because the response stream ended before any content arrived."
        } else {
          $0.answer += "\n\nAnswer was cut off before it finished."
        }
      }
    }
    update { $0.pending = false }
    if Task.isCancelled {
      // The canceller (stopTwin / cancelTwinStream) owns the state reset; a
      // stale stream must not clobber a newer send's task or sending flag.
      probe.finish("cancelled")
      return
    }
    probe.finish(outcome, ok: streamOK)
    let elapsedMs = Telemetry.elapsedMs(since: started)
    update { $0.durationMs = elapsedMs }
    // Enrich the finished turn with memory / persona / voice signals from the
    // projection, matched by receipt so a streamed answer is never lost.
    let enriched = (try? await client.loadTwin(baseURL: Self.configuredBaseURL(), sessionID: sessionID)) ?? []
    if let index = twinMessages.firstIndex(where: { $0.id == pendingID }) {
      let receipt = twinMessages[index].answerReceiptID
      let names = twinMessages[index].attachmentNames
      let wasSensitive = twinMessages[index].sensitive
      if !receipt.isEmpty, var match = enriched.first(where: { $0.answerReceiptID == receipt }) {
        match.attachmentNames = names
        match.sensitive = wasSensitive
        match.durationMs = elapsedMs
        twinMessages[index] = match
      }
    }
    await loadTwinConversations()
    twinSending = false
    twinStreamTask = nil
  }

  func prepareTwinMessage(_ message: TwinMessage) {
    guard !message.answer.isEmpty else { return }
    messageDraft = message.messageDraft
    // A Twin handoff is a writable draft. If Message was last left on an
    // activity filter, move back to the person's preferred channel so the
    // composer is visible and the draft can actually be reviewed and sent.
    if messageLane.channelID == nil {
      let preferredChannel = UserDefaults.standard.string(forKey: "mdx.message.channel")
        ?? messageChannels.first?.id
        ?? "local-ops"
      selectMessageChannel(preferredChannel)
    }
    select(.message)
  }

  func prepareTwinPage(_ message: TwinMessage) {
    pagePublishDraft = PagePublishDraft(
      title: String(message.prompt.prefix(72)),
      body: message.answer,
      pageType: "knowledge",
      visibility: "tenant_only"
    )
    showPagePublish = true
    select(.pages)
  }

  func prepareTwinForgeHandoff(_ message: TwinMessage) {
    pendingBuildIntent = "From Twin: \(message.prompt)\n\nProposed outcome:\n\(message.answer)"
    select(.forge(.overview))
    startBuildFlow()
  }

  func openMission(_ id: String) {
    guard selectedMissionID != id else { return }
    pushHistory()
    selectedMissionID = id
    missionActionResult = nil
  }

  func closeMission() {
    guard selectedMissionID != nil else { return }
    pushHistory()
    selectedMissionID = nil
    missionActionResult = nil
  }

  func createMission(draft: MissionDraft) async -> RunActionOutcome? {
    missionActionInFlight = true
    do {
      let outcome = try await client.createMission(baseURL: Self.configuredBaseURL(), draft: draft, actorID: actorID)
      missionActionResult = outcome
      if !outcome.isRefusal {
        await loadMissions()
        select(.forge(.missions))
        if !outcome.receiptID.isEmpty, missions.contains(where: { $0.id == outcome.receiptID }) {
          openMission(outcome.receiptID)
        }
      }
      missionActionInFlight = false
      return outcome
    } catch {
      let outcome = RunActionOutcome(title: "Create mission", status: "FAILED", detail: error.localizedDescription, receiptID: "")
      missionActionResult = outcome
      missionActionInFlight = false
      return outcome
    }
  }

  func prepareMissionFromFleetPlan(_ plan: FleetPlan) {
    var draft = MissionDraft()
    draft.goal = plan.spec
    draft.doneWhen = "The agreed checks pass, the mission records its receipts, and the final handoff explains what changed."
    draft.validationCommands = plan.suggestedChecks.isEmpty
      ? "Add the exact repo proof before launching."
      : plan.suggestedChecks.joined(separator: ", ")
    draft.allowedWriteScope = "Set exact paths before launching."
    draft.blockedPaths = "Production writes, deployment, and ship authority remain closed until receipts and human decisions say otherwise."
    draft.nonGoals = "Do not bypass the delayed fleet planner by starting unmanaged worker spend."
    draft.fleetWidth = max(1, plan.requestedWidth)
    draft.maxCostDollars = max(10, min(500, plan.requestedWidth * 5))
    draft.maxRuntimeHours = plan.requestedWidth >= 8 ? 8 : 4
    draft.checkpointCadenceMinutes = 30
    missionSetupDraft = draft
    missionActionResult = nil
    showMissionSetup = true
    logger.info("Prepared mission from delayed fleet plan \(plan.id, privacy: .public)")
  }

  func launchSelectedMission() async {
    guard let mission = selectedMission else { return }
    missionActionInFlight = true
    do {
      let launch = try await client.launchMission(
        baseURL: Self.configuredBaseURL(),
        mission: mission,
        repoID: selectedRepoID,
        actorID: actorID
      )
      missionActionResult = launch.action
      await loadMissions()
      if launch.opensRun {
        await refreshRuns()
        select(.forge(.runs))
        openRun(launch.delegatedRunID)
      }
    } catch {
      missionActionResult = RunActionOutcome(title: "Launch mission", status: "FAILED", detail: error.localizedDescription, receiptID: "")
    }
    missionActionInFlight = false
  }

  func checkpointSelectedMission(event: String, milestoneID: String = "", note: String = "") async {
    guard let mission = selectedMission else { return }
    await performMissionAction {
      try await self.client.missionCheckpoint(
        baseURL: Self.configuredBaseURL(),
        missionID: mission.id,
        event: event,
        milestoneID: milestoneID,
        note: note,
        actorID: self.actorID
      )
    }
  }

  private func performMissionAction(_ action: @escaping () async throws -> RunActionOutcome) async {
    missionActionInFlight = true
    do {
      missionActionResult = try await action()
      await loadMissions()
    } catch {
      missionActionResult = RunActionOutcome(title: "Mission action", status: "FAILED", detail: error.localizedDescription, receiptID: "")
    }
    missionActionInFlight = false
  }

  /// Watches runs the user launched from this app and pings when they finish,
  /// so an engineer can start a build and walk away. Background loop runs are
  /// intentionally excluded so notifications never turn into noise.
  private func startGlobalWatch() {
    guard watchTask == nil else { return }
    watchTask = Task { [weak self] in
      while !Task.isCancelled {
        let cadence: UInt64 = await MainActor.run { [weak self] in
          self?.isForeground ?? true
        } ? 8_000_000_000 : 30_000_000_000
        try? await Task.sleep(nanoseconds: cadence)
        if Task.isCancelled { return }
        guard let self else { return }
        guard self.snapshot.connectionStatus == .ok else { continue }
        // Don't double-load the ~180KB run projection: while a run is open the
        // scoped 4s poll already refreshes runs, so this background watch would
        // just parse and diff the same projection a second time. It exists to
        // catch finished runs when nothing is open; defer to the poll otherwise.
        if self.pollTask != nil { continue }
        await self.refreshRuns()
      }
    }
  }

  /// Watches for the local route server from every surface, not only the
  /// first-run connect card. A Mac user can start MDx after opening the app and
  /// the shell should quietly become live without requiring a special screen.
  private func startReconnectWatch() {
    guard reconnectTask == nil else { return }
    reconnectTask = Task { [weak self] in
      // Back the reconnect probe off as the offline spell lengthens instead of
      // hammering a down engine every few seconds. Each refresh is now a single
      // liveness probe while offline (loadSnapshot short-circuits), so a slow
      // cadence is enough to come live promptly without a continuous fan-out.
      var backoffMs: UInt64 = 2_000
      while !Task.isCancelled {
        let foreground = await MainActor.run { [weak self] in self?.isForeground ?? true }
        let floorMs: UInt64 = foreground ? 2_000 : 15_000
        let ceilingMs: UInt64 = foreground ? 20_000 : 60_000
        let cadence = min(max(backoffMs, floorMs), ceilingMs)
        try? await Task.sleep(nanoseconds: cadence * 1_000_000)
        if Task.isCancelled { return }
        guard let self else { return }
        guard self.snapshot.connectionStatus != .ok else {
          self.stopReconnectWatch()
          return
        }
        await self.refresh()
        backoffMs = min(cadence + cadence / 2, ceilingMs)
      }
    }
  }

  private func stopReconnectWatch() {
    reconnectTask?.cancel()
    reconnectTask = nil
  }

  /// Scene-phase hook: the app slows its polling when the user is elsewhere
  /// (the menu bar stays glanceable at a 30s cadence) and catches up
  /// immediately on return.
  func setForeground(_ active: Bool) {
    guard active != isForeground else { return }
    isForeground = active
    HangMonitor.shared.setPaused(!active)
    if active {
      Task {
        if snapshot.connectionStatus == .ok {
          await refreshRuns()
        } else {
          startReconnectWatch()
          await refresh()
        }
        // A sleeping Mac drops SSE connections without an error; re-arm the
        // live trail for a run that is still going.
        rearmRunStreamOnForeground()
      }
    }
  }

  /// A sleeping Mac drops the SSE socket without an error, and the request idle
  /// timeout is hours long, so the existing streamTask can be non-nil but dead.
  /// syncRunStreamingToRoute would see a non-nil task and never re-arm, freezing
  /// the trail while the 4s poll keeps advancing the header. On foreground
  /// restore, force a fresh stream for a still-running selected run.
  private func rearmRunStreamOnForeground() {
    guard selectedAppRoute == .forge(.runs),
          let run = selectedRun, run.isRunning, !run.streamRoute.isEmpty
    else {
      syncRunStreamingToRoute()
      return
    }
    // startStreaming cancels any half-dead task first. Keep the trail across
    // the re-arm: appendLiveEvents dedupes the replay, so there is no empty
    // flash and the rendered rows keep their identity.
    startStreaming(run: run)
    startPolling()
  }

  /// Called whenever fresh runs land: first call settles "since you left",
  /// every call keeps the persisted exit-state current (cheap, tiny dict).
  private func updateRunStateLedger(with runs: [ForgeRun]) {
    if let previous = previousSessionRunStates {
      previousSessionRunStates = nil
      let finishedWhileAway = runs.filter { previous[$0.id] == true && !$0.isRunning }.map(\.id)
      if !finishedWhileAway.isEmpty {
        sinceYouLeftRunIDs = finishedWhileAway
      }
    }
    let states = Dictionary(uniqueKeysWithValues: runs.map { ($0.id, $0.isRunning) })
    UserDefaults.standard.set(states, forKey: Self.runStatesDefaultsKey)
  }

  var sinceYouLeftRuns: [ForgeRun] {
    sinceYouLeftRunIDs.compactMap { id in forgeRuns.first { $0.id == id } }
  }

  func dismissSinceYouLeft() {
    sinceYouLeftRunIDs = []
  }

  private func notifyFinishedStartedRuns(in runs: [ForgeRun]) {
    for run in runs where startedRunIDs.contains(run.id) && !run.isRunning && !notifiedRunIDs.contains(run.id) {
      notifiedRunIDs.insert(run.id)
      unseenRunIDs.insert(run.id)
      let failed = run.status.localizedCaseInsensitiveContains("fail") || run.status.localizedCaseInsensitiveContains("error")
      RunNotifier.runFinished(
        title: run.title.isEmpty ? run.id : run.title,
        needsReview: run.hasBranch && !failed,
        failed: failed,
        runID: run.id
      )
    }
  }

  func loadReviewPacket() async {
    guard let run = selectedRun else { return }
    let runID = run.id
    reviewPacketGeneration += 1
    let generation = reviewPacketGeneration
    reviewPacketPhase = .loading
    do {
      let packet = try await client.fetchReviewPacket(baseURL: Self.configuredBaseURL(), runID: runID, actorID: actorID)
      guard selectedRunID == runID, generation == reviewPacketGeneration else { return }
      reviewPacket = packet
      reviewPacketPhase = .loaded(Date())
    } catch {
      guard selectedRunID == runID, generation == reviewPacketGeneration else { return }
      reviewPacket = nil
      reviewPacketPhase = .failed(friendlyLoadError(error))
    }
  }

  /// Reload the open run's diff and review packet so the review surface never
  /// shows stale contents. Only reloads a surface the operator already opened
  /// (anything beyond idle) and only while that run is still the one on screen.
  /// A finish can race an in-flight mid-run request, so a newer generation
  /// supersedes that request instead of letting stale empty evidence win.
  func reloadReviewSurfaces(for runID: String) async {
    guard selectedRunID == runID else { return }
    if runDiffPhase != .idle {
      await loadRunDiff()
    }
    if reviewPacketPhase != .idle {
      await loadReviewPacket()
    }
  }

  /// If the open run just transitioned running -> finished, reload its review
  /// surfaces so a diff loaded mid-run does not keep pre-finish contents.
  private func reloadReviewSurfacesIfSelectedRunFinished(wasRunning: Bool, runID: String?) {
    guard wasRunning, let runID, selectedRunID == runID,
          let updated = forgeRuns.first(where: { $0.id == runID }),
          !updated.isRunning
    else { return }
    Task { [weak self] in await self?.reloadReviewSurfaces(for: runID) }
  }

  func reviseSelectedRun(comment: String) async {
    guard let run = selectedRun else { return }
    await performRunAction(runID: run.id) {
      try await self.client.reviseRun(
        baseURL: Self.configuredBaseURL(),
        runID: run.id,
        comment: comment,
        actorID: self.actorID
      )
    }
  }

  func auditSelectedRun(previewPath: String, viewport: BrowserViewport, autoRevise: Bool) async {
    guard let run = selectedRun, !browserAuditInFlight else { return }
    browserAuditInFlight = true
    browserAuditError = nil
    do {
      let result = try await client.auditRenderedRun(
        baseURL: Self.configuredBaseURL(),
        runID: run.id,
        previewPath: previewPath,
        viewport: viewport,
        autoRevise: autoRevise,
        actorID: actorID
      )
      guard selectedRunID == run.id else {
        browserAuditInFlight = false
        return
      }
      browserAudit = result
      if result.revisionStarted {
        runActionResult = RunActionOutcome(
          title: "Browser repair started",
          status: "RUN_STARTED",
          detail: "Forge opened a governed revision run from the rendered evidence.",
          receiptID: result.revisionRunID
        )
        await refreshRuns()
      }
    } catch where selectedRunID == run.id {
      browserAuditError = friendlyLoadError(error)
    } catch {
      // The operator changed runs while the audit was in flight.
    }
    browserAuditInFlight = false
  }

  func startBrowserEvolution(previewPath: String, viewport: BrowserViewport) {
    guard let run = selectedRun, run.hasBranch, !browserEvolutionActive else { return }
    browserEvolutionActive = true
    browserAuditInFlight = true
    browserAuditError = nil
    browserEvolutionCycle = 0
    browserEvolutionRunID = run.id
    browserEvolutionGeneration += 1
    let generation = browserEvolutionGeneration
    browserEvolutionTask = Task { [weak self] in
      guard let self else { return }
      let maxAudits = 3
      var currentRunID = run.id
      for cycle in 1...maxAudits {
        guard !Task.isCancelled, self.browserEvolutionGeneration == generation else { break }
        self.browserEvolutionCycle = cycle
        self.browserEvolutionRunID = currentRunID
        do {
          let result = try await self.client.auditRenderedRun(
            baseURL: Self.configuredBaseURL(),
            runID: currentRunID,
            previewPath: previewPath,
            viewport: viewport,
            autoRevise: cycle < maxAudits,
            actorID: self.actorID
          )
          guard self.browserEvolutionGeneration == generation else { break }
          self.browserAudit = result
          if result.approved {
            self.runActionResult = RunActionOutcome(
              title: "Render evolved clear",
              status: "APPROVED",
              detail: "Forge re-rendered the branch and found no remaining high or medium browser defects.",
              receiptID: result.auditID
            )
            break
          }
          guard result.revisionStarted, !result.revisionRunID.isEmpty else {
            self.runActionResult = RunActionOutcome(
              title: "Render still needs judgment",
              status: "REVIEW_REQUIRED",
              detail: cycle == maxAudits
                ? "The two-repair browser budget is exhausted. Review the latest screenshot and critique."
                : "Forge captured a revision brief but did not open repair authority.",
              receiptID: result.auditID
            )
            break
          }
          currentRunID = result.revisionRunID
          self.browserEvolutionRunID = currentRunID
          let finished = await self.waitForRunToFinish(currentRunID, timeoutSeconds: 1_200)
          guard finished else {
            if !Task.isCancelled {
              self.browserAuditError = "The browser repair did not finish within 20 minutes. Its run remains available in Forge."
            }
            break
          }
        } catch {
          if !Task.isCancelled, self.browserEvolutionGeneration == generation {
            self.browserAuditError = friendlyLoadError(error)
          }
          break
        }
      }
      if self.browserEvolutionGeneration == generation {
        self.browserAuditInFlight = false
        self.browserEvolutionActive = false
        self.browserEvolutionTask = nil
        await self.refreshRuns()
      }
    }
  }

  func stopBrowserEvolution() {
    browserEvolutionTask?.cancel()
    browserEvolutionTask = nil
    browserEvolutionGeneration += 1
    browserEvolutionActive = false
    browserAuditInFlight = false
  }

  private func waitForRunToFinish(_ runID: String, timeoutSeconds: Int) async -> Bool {
    let deadline = Date().addingTimeInterval(TimeInterval(timeoutSeconds))
    while Date() < deadline, !Task.isCancelled {
      await refreshRuns()
      if let run = forgeRuns.first(where: { $0.id == runID }), !run.isRunning {
        return run.hasBranch
      }
      try? await Task.sleep(for: .seconds(2))
    }
    return false
  }

  func loadRunDiff() async {
    guard let run = selectedRun else { return }
    let runID = run.id
    runDiffGeneration += 1
    let generation = runDiffGeneration
    runDiffPhase = .loading
    do {
      let files = try await client.fetchDiff(baseURL: Self.configuredBaseURL(), runID: runID, actorID: actorID)
      guard selectedRunID == runID, generation == runDiffGeneration else { return }
      runDiff = files
      runDiffPhase = .loaded(Date())
    } catch {
      guard selectedRunID == runID, generation == runDiffGeneration else { return }
      runDiff = []
      runDiffPhase = .failed(friendlyLoadError(error))
    }
  }

  /// Folder picker -> governed connect -> repo appears in every picker.
  func connectRepoFromPanel() {
    let panel = NSOpenPanel()
    panel.canChooseFiles = false
    panel.canChooseDirectories = true
    panel.allowsMultipleSelection = false
    panel.message = "Choose a repository folder to connect to Forge"
    panel.prompt = "Connect"
    guard panel.runModal() == .OK, let url = panel.url else { return }
    let root = url.path
    let slug = url.lastPathComponent.lowercased()
      .replacingOccurrences(of: " ", with: "-")
      .filter { $0.isLetter || $0.isNumber || $0 == "-" || $0 == "_" }
    let label = url.lastPathComponent
    Telemetry.breadcrumb("connect repo \(slug)", "action")
    Task { [weak self] in
      guard let self else { return }
      do {
        let outcome = try await self.client.connectRepo(
          baseURL: Self.configuredBaseURL(),
          repoID: slug.isEmpty ? "repo" : slug,
          label: label,
          root: root
        )
        self.runActionResult = outcome
        await self.loadRepos()
        if self.repos.contains(where: { $0.id == slug }) {
          self.selectedRepoID = slug
          await self.scoutSelectedRepo()
        }
      } catch {
        self.runActionResult = RunActionOutcome(title: "Connect a repo", status: "FAILED", detail: friendlyLoadError(error), receiptID: "")
      }
    }
  }

  func loadRepos() async {
    guard let loaded = try? await client.loadRepos(baseURL: Self.configuredBaseURL()) else { return }
    repos = loaded
    if selectedRepoID.isEmpty || !loaded.contains(where: { $0.id == selectedRepoID }) {
      selectedRepoID = loaded.first(where: { $0.id == "mdx-self" })?.id ?? loaded.first?.id ?? ""
    }
  }

  /// The active repo behind the persistent context switcher, or nil before repos
  /// have loaded.
  var activeRepo: ForgeRepo? {
    repos.first { $0.id == selectedRepoID } ?? repos.first
  }

  /// Switch the repo every governed action is anchored to. The composer, scout,
  /// and start-build flows all read `selectedRepoID`, so this is the one place
  /// context changes.
  func selectRepo(_ id: String) {
    guard !id.isEmpty, id != selectedRepoID, repos.contains(where: { $0.id == id }) else { return }
    selectedRepoID = id
    Telemetry.breadcrumb("switch repo \(id)", "context")
    logger.info("Switched active repo: \(id, privacy: .public)")
  }

  /// The branch line for the context anchor. The kernel projects no persistent
  /// base branch per repo, so this shows the most recent run's branch for the
  /// active repo when one exists; the switcher states the per-run default
  /// otherwise.
  func latestRunBranch(forRepoID id: String) -> String? {
    forgeRuns.first { run in
      run.hasBranch && (run.repo == id || (!id.isEmpty && run.repo.contains(id)))
    }?.branch
  }

  func scoutSelectedRepo() async {
    let repo = selectedRepoID
    guard !repo.isEmpty else { return }
    scoutPhase = .loading
    scout = nil
    do {
      let result = try await client.scoutRepo(baseURL: Self.configuredBaseURL(), repoID: repo)
      // ignore stale results if the user switched repos mid-scan
      guard selectedRepoID == repo else { return }
      scout = result
      scoutPhase = .loaded(Date())
    } catch {
      guard selectedRepoID == repo else { return }
      scoutPhase = .failed(error.localizedDescription)
    }
  }

  func classify(ask: String) async -> WorkRecommendation? {
    let trimmed = ask.trimmingCharacters(in: .whitespacesAndNewlines)
    guard trimmed.count >= 8 else { return nil }
    do {
      let recommendation = try await client.classifyIntent(
        baseURL: Self.configuredBaseURL(),
        ask: trimmed,
        repo: selectedRepoID,
        actorID: actorID
      )
      logger.info("Build classification finished: shape=\(recommendation.shape, privacy: .public) width=\(recommendation.width, privacy: .public)")
      return recommendation
    } catch {
      logger.info("Build classification failed: \(friendlyLoadError(error), privacy: .public)")
      return nil
    }
  }

  /// Cancels an in-flight build start or fleet plan POST. The Start sheet's
  /// Cancel calls this so dismissing actually stops the network work instead of
  /// leaving a 30s (or 180s fleet) request running behind a closed sheet.
  func cancelBuildStart() {
    buildStartTask?.cancel()
  }

  func startRun(intent: String, repoID: String, fleetWidth: Int, allowedCommands: [String]) async -> RunActionOutcome? {
    let trimmed = intent.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return nil }
    // Build start and fleet planning run as a cancellable background task and
    // use buildStartInFlight, never the per-run lock, so a long fleet-plan POST
    // cannot freeze the ship/steer/stop/revise verbs on other runs.
    buildStartTask?.cancel()
    buildStartInFlight = true
    defer {
      buildStartInFlight = false
      buildStartTask = nil
    }
    let baseURL = Self.configuredBaseURL()
    let actor = actorID
    let task = Task<RunActionOutcome, Error> {
      try await self.client.startRun(
        baseURL: baseURL,
        intent: trimmed,
        repoID: repoID,
        fleetWidth: fleetWidth,
        allowedCommands: allowedCommands,
        actorID: actor
      )
    }
    buildStartTask = task
    switch await task.result {
    case .success(let outcome):
      runActionResult = outcome
      if !outcome.isRefusal {
        if !outcome.receiptID.isEmpty {
          await recordBetaTelemetry(
            "activation_step",
            route: "/forge",
            step: "first_forge_request",
            completed: true
          )
          await recordBetaTelemetry(
            "activation_step",
            route: "/forge",
            step: "first_result_seen",
            completed: true
          )
        }
        await refresh()
        if outcome.title == "Plan fleet build" {
          select(.forge(.fleet))
          logger.info("Fleet plan finished: \(outcome.status, privacy: .public)")
          return outcome
        }
        let newID = outcome.receiptID
        if !newID.isEmpty {
          startedRunIDs.insert(newID)
          if !forgeRuns.contains(where: { $0.id == newID }) {
            try? await Task.sleep(nanoseconds: 1_400_000_000)
            await refresh()
          }
          select(.forge(.runs))
          if forgeRuns.contains(where: { $0.id == newID }) {
            openRun(newID)
          }
        }
      }
      logger.info("Start run finished: \(outcome.status, privacy: .public)")
      return outcome
    case .failure(let error):
      // A cancelled start (sheet dismissed) is not a failure to report.
      if error is CancellationError || (error as? URLError)?.code == .cancelled {
        logger.info("Build start cancelled before completion")
        return nil
      }
      let outcome = RunActionOutcome(
        title: fleetWidth > 4 ? "Plan fleet build" : "Start run",
        status: "FAILED",
        detail: startRunFailureDetail(error, fleetWidth: fleetWidth),
        receiptID: ""
      )
      runActionResult = outcome
      return outcome
    }
  }

  private func startRunFailureDetail(_ error: Error, fleetWidth: Int) -> String {
    let detail = friendlyLoadError(error)
    guard fleetWidth > 4 else { return detail }
    if let urlError = error as? URLError, urlError.code == .timedOut {
      return "Fleet planning is still taking too long. Forge may be waiting on planner or advisor reasoning before it can record a plan. Keep the width, try again, or switch to Mission if this is long-horizon work."
    }
    return detail
  }

  func stageMachineFaceOff(runnerID: String) async {
    guard !runnerID.isEmpty else { return }
    machineActionInFlight = true
    machineActionResult = nil
    do {
      let outcome = try await client.stageMachineFaceOff(
        baseURL: Self.configuredBaseURL(),
        runnerID: runnerID,
        actorID: actorID
      )
      machineActionResult = outcome
      if !outcome.isRefusal {
        await refresh()
      }
      logger.info("Machine face-off finished: \(outcome.status, privacy: .public)")
    } catch {
      let detail: String
      if let urlError = error as? URLError, urlError.code == .timedOut {
        detail = "MDx could not confirm the result before the request timed out. Refresh Machine sources before retrying because the held packet may still have been recorded."
      } else {
        detail = friendlyLoadError(error)
      }
      machineActionResult = RunActionOutcome(title: "Compare on a fixture", status: "FAILED", detail: detail, receiptID: "")
    }
    machineActionInFlight = false
  }

  func checkMachineRuntimes() async {
    guard !machinePreflightInFlight else { return }
    machinePreflightInFlight = true
    machinePreflightSummary = "Checking installed harnesses on this Mac. This can take up to a minute."
    do {
      let runners = try await client.loadMachineRunners(baseURL: Self.configuredBaseURL())
      snapshot.runners = runners
      let checked = runners.filter { !$0.isNative && $0.runtimeChecked }
      let installed = checked.filter(\.binaryPresent)
      machinePreflightSummary = "Checked \(checked.count) external harnesses. \(installed.count) are installed and visible on this Mac."
      logger.info("Machine preflight finished: checked=\(checked.count, privacy: .public) installed=\(installed.count, privacy: .public)")
    } catch {
      machinePreflightSummary = "Machine preflight could not finish: \(friendlyLoadError(error))"
      logger.error("Machine preflight failed: \(error.localizedDescription, privacy: .public)")
    }
    machinePreflightInFlight = false
  }

  func shipSelectedRun(reason: String) async {
    guard let run = selectedRun else { return }
    await performRunAction(runID: run.id) {
      try await self.client.shipRun(
        baseURL: Self.configuredBaseURL(),
        runID: run.id,
        commitSha: run.commitSha,
        reason: reason,
        actorID: self.actorID
      )
    }
  }

  /// Ratify-and-start for a reviewed run: record the human ship decision, then
  /// immediately kick the follow-on PR handoff, in one operator move. Mirrors the
  /// fleet-plan ratify -> start pattern (two governed receipts, one gesture): the
  /// ship decision is the ratification, the PR handoff is the follow-on. Only the
  /// second step runs if the first is not refused, so a locked ship never opens a
  /// PR. Returns true when the ship landed, so the caller can advance the flow.
  ///
  /// Kernel gap (flagged for the Codex lane): there is no single route that binds
  /// the ship decision to the follow-on start under one receipt, so this is two
  /// chained governed calls, each independently receipted, rather than one atomic
  /// ratify-and-start receipt.
  @discardableResult
  func shipAndOpenPR(reason: String) async -> Bool {
    guard let run = selectedRun else { return false }
    await shipSelectedRun(reason: reason)
    guard runActionResult?.isRefusal == false, runActionResult?.classification == .succeeded else {
      return false
    }
    guard run.hasBranch else { return true }
    preparePRHandoff()
    return true
  }

  func ratifyFleetPlan(_ plan: FleetPlan) async {
    await performRunAction(runID: nil, title: "Ratify fleet") {
      try await self.client.ratifyFleetPlan(
        baseURL: Self.configuredBaseURL(),
        fleetID: plan.id,
        reason: "Operator reviewed the native fleet split and ratified it from the Mac app.",
        actorID: self.actorID
      )
    }
  }

  func startFleetRun(_ plan: FleetPlan) async {
    await performRunAction(runID: nil, title: "Start fleet") {
      try await self.client.startFleetRun(
        baseURL: Self.configuredBaseURL(),
        fleetID: plan.id,
        repoID: plan.repo,
        actorID: self.actorID
      )
    }
  }

  func controlSelectedRun(_ control: String, note: String) async {
    guard let run = selectedRun else { return }
    await performRunAction(runID: run.id) {
      try await self.client.controlRun(
        baseURL: Self.configuredBaseURL(),
        runID: run.id,
        control: control,
        note: note,
        actorID: self.actorID
      )
    }
  }

  /// True while a governed verb (ship/steer/stop/revise) is in flight for the
  /// given run. Verbs on other runs stay enabled.
  func isRunActionInFlight(_ runID: String) -> Bool {
    runActionInFlightIDs.contains(runID)
  }

  /// runID scopes the in-flight lock to that run; pass nil for fleet-plan
  /// verbs that are not tied to a single run id (they use buildStartInFlight).
  private func performRunAction(runID: String?, title: String = "Governed action", _ action: @escaping () async throws -> RunActionOutcome) async {
    if let runID {
      runActionInFlightIDs.insert(runID)
    } else {
      buildStartInFlight = true
    }
    defer {
      if let runID {
        runActionInFlightIDs.remove(runID)
      } else {
        buildStartInFlight = false
      }
    }
    do {
      let outcome = try await action()
      runActionResult = outcome
      await refresh()
      if outcome.title == "Request changes",
         !outcome.isRefusal,
         outcome.receiptID.hasPrefix("forge_run_") {
        select(.forge(.runs))
        openRun(outcome.receiptID)
      }
      // Keep the open run's diff and review packet fresh after a governed
      // ship or revise so the review surface never shows stale contents.
      if let runID, !outcome.isRefusal {
        await reloadReviewSurfaces(for: runID)
      }
    } catch {
      runActionResult = RunActionOutcome(title: title, status: "FAILED", detail: error.localizedDescription, receiptID: "")
    }
  }

  func addHostProject(path: String) {
    let trimmed = path.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return }
    if hostProjects.contains(where: { $0.path == trimmed }) {
      return
    }
    hostProjects.append(HostProject(id: UUID().uuidString, path: trimmed, addedAt: Date()))
    saveHostProjects()
  }

  func removeHostProject(_ project: HostProject) {
    hostProjects.removeAll { $0.id == project.id }
    saveHostProjects()
  }

  func showSettings() {
    // In-app settings cover: takes over the main window rather than opening a
    // detached macOS Settings scene. Activate first so the window fronts even
    // when invoked from the menu bar extra.
    NSApp.activate(ignoringOtherApps: true)
    settingsShown = true
    logger.info("Opened settings overlay")
  }

  /// Close the settings cover and return to the shell exactly where it was.
  func hideSettings() {
    settingsShown = false
    logger.info("Closed settings overlay")
  }

  func showCommandPalette() {
    commandPaletteShown = true
    logger.info("Opened command palette")
  }

  func hideCommandPalette() {
    commandPaletteShown = false
  }

  static func configuredBaseURL() -> URL {
    if let value = ProcessInfo.processInfo.environment["MDX_API_URL"],
       let url = URL(string: value) {
      return url
    }
    if let value = ProcessInfo.processInfo.environment["MDX_OPERATOR_API_URL"],
       let url = URL(string: value) {
      return url
    }
    if let hostedURL = CloudConfiguration.current?.hostedAPIURL {
      return hostedURL
    }
    if let value = UserDefaults.standard.string(forKey: apiURLDefaultsKey),
       let url = URL(string: value) {
      return url
    }
    if let value = UserDefaults.standard.string(forKey: legacyAPIURLDefaultsKey),
       let url = URL(string: value) {
      return url
    }
    return MDxRouteClient.defaultBaseURL
  }

  private static func loadHostProjects() -> [HostProject] {
    guard let data = UserDefaults.standard.data(forKey: hostProjectsDefaultsKey) else {
      return []
    }
    return (try? JSONDecoder().decode([HostProject].self, from: data)) ?? []
  }

  private func saveHostProjects() {
    if let data = try? JSONEncoder().encode(hostProjects) {
      UserDefaults.standard.set(data, forKey: Self.hostProjectsDefaultsKey)
    }
  }

  private static func appRoute(for section: OperatorSection) -> AppRoute {
    switch section {
    case .overview, .work, .controls, .host:
      return .forge(.overview)
    case .runRoom:
      return .forge(.runs)
    case .review:
      return .forge(.review)
    case .fleets:
      return .forge(.fleet)
    case .machines:
      return .forge(.machines)
    case .evidence:
      return .forge(.evidence)
    }
  }
}
