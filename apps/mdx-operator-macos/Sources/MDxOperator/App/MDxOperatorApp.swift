import AppKit
import SwiftUI
import UserNotifications

final class AppDelegate: NSObject, NSApplicationDelegate, @preconcurrency UNUserNotificationCenterDelegate {
  func applicationDidFinishLaunching(_ notification: Notification) {
    Telemetry.markLaunch()
    HangMonitor.shared.start()
    NSApp.setActivationPolicy(.regular)
    NSApp.activate(ignoringOtherApps: true)
    UNUserNotificationCenter.current().delegate = self
    RunNotifier.requestAuthorization()
    // SwiftUI's default size does not override a frame restored by macOS.
    // Migrate the legacy compact frame once, then leave subsequent user sizing
    // and normal state restoration entirely to the system.
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) {
      MainWindowLaunchSizing.applyLegacyFrameMigrationIfNeeded()
    }
  }

  func applicationWillTerminate(_ notification: Notification) {
    MainActor.assumeIsolated { DiagnosticsStore.shared.persistSession() }
    UserDefaults.standard.set(true, forKey: OperatorStore.cleanExitDefaultsKey)
  }

  /// Clicking a run-finished notification opens that run in the app.
  @MainActor
  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    didReceive response: UNNotificationResponse,
    withCompletionHandler completionHandler: @escaping () -> Void
  ) {
    let userInfo = response.notification.request.content.userInfo
    if let runID = userInfo[RunNotifier.runIDKey] as? String, !runID.isEmpty {
      NSApp.activate(ignoringOtherApps: true)
      NotificationCenter.default.post(name: RunNotifier.openRunNotification, object: nil, userInfo: [RunNotifier.runIDKey: runID])
    }
    completionHandler()
  }

  /// While the app is frontmost the in-app surfaces already show the state
  /// change; only interrupt when the user is elsewhere.
  @MainActor
  func userNotificationCenter(
    _ center: UNUserNotificationCenter,
    willPresent notification: UNNotification,
    withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
  ) {
    completionHandler(NSApp.isActive ? [] : [.banner, .sound])
  }
}

@MainActor
enum MainWindowLaunchSizing {
  static let migrationDefaultsKey = "mdx.main-window.display-aware-size.v1"

  static func targetSize(for visibleSize: CGSize) -> CGSize {
    let availableWidth = max(920, visibleSize.width - 40)
    let availableHeight = max(640, visibleSize.height - 40)
    let preferredWidth = min(1680, max(1180, visibleSize.width * 0.80))
    let preferredHeight = min(1080, max(760, visibleSize.height * 0.84))
    return CGSize(
      width: min(preferredWidth, availableWidth),
      height: min(preferredHeight, availableHeight)
    )
  }

  static func applyLegacyFrameMigrationIfNeeded(defaults: UserDefaults = .standard) {
    guard !defaults.bool(forKey: migrationDefaultsKey),
          let window = NSApp.windows.first(where: { $0.title == "MDx" && !$0.isSheet }),
          let screen = window.screen ?? NSScreen.main
    else { return }

    let visibleFrame = screen.visibleFrame
    let target = targetSize(for: visibleFrame.size)
    let legacyFrameIsCompact = window.frame.width < target.width * 0.90
      || window.frame.height < target.height * 0.88
    if legacyFrameIsCompact {
      let origin = NSPoint(
        x: visibleFrame.midX - target.width / 2,
        y: visibleFrame.midY - target.height / 2
      )
      window.setFrame(NSRect(origin: origin, size: target), display: true)
    }
    defaults.set(true, forKey: migrationDefaultsKey)
  }
}

enum RunNotifier {
  static let runIDKey = "mdxRunID"
  static let openRunNotification = Notification.Name("MDxOpenRunFromNotification")

  static func requestAuthorization() {
    UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { granted, error in
      if let error {
        Telemetry.breadcrumb("notification auth failed: \(error.localizedDescription)", "system")
      } else if !granted {
        Telemetry.breadcrumb("notifications not granted", "system")
      }
    }
  }

  static func runFinished(title: String, needsReview: Bool, failed: Bool, runID: String) {
    let content = UNMutableNotificationContent()
    // Interrupts are for decisions: ready-to-review, or stuck/failed. A run
    // that just finished quietly still notifies once because the user
    // explicitly started and walked away from it.
    if failed {
      content.title = "Run hit a wall"
    } else if needsReview {
      content.title = "Ready to review"
    } else {
      content.title = "Run finished"
    }
    content.body = title
    content.sound = .default
    content.userInfo = [runIDKey: runID]
    let request = UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil)
    UNUserNotificationCenter.current().add(request)
  }
}

@main
struct MDxApp: App {
  @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
  @State private var auth: CloudAuthStore
  @State private var store: OperatorStore

  init() {
    let auth = CloudAuthStore()
    _auth = State(initialValue: auth)
    _store = State(initialValue: OperatorStore(client: MDxRouteClient(accessTokenProvider: {
      await auth.accessToken()
    })))
    // Headless contract checks (`--mapper-checks`): run and exit before any UI.
    MapperChecks.runIfRequested()
    // Headless S7 parity probe (`--parity-probe`): drive the real client against
    // the live kernel, emit the parity scorecard, and exit before any UI.
    ParityProbe.runIfRequested()
    // Dev-only visual snapshot harness (`--snapshot`): render the top surfaces
    // to PNGs under .snapshots/ via ImageRenderer and exit before any UI.
    SnapshotRunner.runIfRequested()
  }
  @AppStorage(OperatorStore.appearanceModeDefaultsKey) private var appearanceModeRaw = AppearanceMode.system.rawValue

  var body: some Scene {
    WindowGroup("MDx") {
      CloudGateView {
        ContentView()
      }
        .environment(auth)
        .environment(store)
        .onChange(of: auth.state) { _, state in
          switch state {
          case .authorized:
            let activeAuth = auth
            store = OperatorStore(client: MDxRouteClient(accessTokenProvider: {
              await activeAuth.accessToken()
            }))
          case .checkingAccess, .signedOut, .pending, .failed:
            store.suspendCloudAccess()
          case .local, .restoring:
            break
          }
        }
        // Capture rail: MDX_SHOT_APPEARANCE forces the scheme for the light/dark
        // matrix; MDX_SHOT_A11Y forces one accessibility setting. Both inert
        // unless set, so the shipping window follows the operator's appearance.
        .preferredColorScheme(ShotOverrides.colorScheme ?? appearanceMode.colorScheme)
        .shotAccessibilityOverrides()
        .frame(minWidth: 920, minHeight: 640)
    }
    .defaultSize(width: 1440, height: 900)
    .commands {
      // File: creation verbs live where every Mac app puts them.
      CommandGroup(replacing: .newItem) {
        Button("Start a Build") { store.startBuildFlow() }
          .keyboardShortcut("n", modifiers: [.command])
        Button("New Page") {
          store.select(.pages)
          store.startNewPage()
        }
        .keyboardShortcut("n", modifiers: [.command, .shift])
        Button("New Conversation") {
          store.select(.twin)
          store.newTwinConversation()
        }
        .keyboardShortcut("n", modifiers: [.command, .option])
      }

      // View: palette, refresh, diagnostics.
      CommandGroup(after: .sidebar) {
        Divider()
        Button("Command Palette") { store.showCommandPalette() }
          .keyboardShortcut("k", modifiers: [.command])
        Button("Refresh Local MDx") { Task { await store.refresh() } }
          .keyboardShortcut("r", modifiers: [.command])
        Button("Diagnostics") { store.showDiagnosticsPanel() }
          .keyboardShortcut("d", modifiers: [.command, .shift])
        Button("Quick Panel") { CompanionPanelController.shared.toggle() }
          .keyboardShortcut(.space, modifiers: [.option])
      }

      // Settings lives in-app now (a full-window cover), not a detached scene.
      // Keep the standard Cmd-comma so muscle memory still opens it.
      CommandGroup(replacing: .appSettings) {
        Button("Settings…") { store.showSettings() }
          .keyboardShortcut(",", modifiers: [.command])
      }

      CommandGroup(replacing: .help) {
        Button("Show Welcome") { store.showWelcome() }
        Button("Check for Updates…") { Task { await store.checkForMacUpdate() } }
        Divider()
        Button("Report an Issue…") { store.startFeedback() }
          .keyboardShortcut("b", modifiers: [.command, .shift])
      }

      // Go: global surface switching and history, Codex/Linear style.
      CommandMenu("Go") {
        Button("Home") { store.select(.home) }
          .keyboardShortcut("1", modifiers: [.command])
        // Forge opens on the governed front door. Trails stay one lane away.
        Button("Forge") { store.select(.forge(.overview)) }
          .keyboardShortcut("2", modifiers: [.command])
        Button("Twin") { store.select(.twin) }
          .keyboardShortcut("3", modifiers: [.command])
        Button("Pages") { store.select(.pages) }
          .keyboardShortcut("4", modifiers: [.command])
        Button("Message") { store.select(.message) }
          .keyboardShortcut("5", modifiers: [.command])
        Button("Marketplace") { store.select(.marketplace) }
          .keyboardShortcut("6", modifiers: [.command])
        Divider()
        Button("Back") { store.navigateBack() }
          .keyboardShortcut("[", modifiers: [.command])
          .disabled(!store.canNavigateBack)
        Button("Forward") { store.navigateForward() }
          .keyboardShortcut("]", modifiers: [.command])
          .disabled(!store.canNavigateForward)
      }
    }

    // Compact tear-off monitor: watch one run beside other work.
    WindowGroup("Run", id: "run-monitor", for: String.self) { $runID in
      if let runID {
        RunMonitorView(runID: runID)
          .environment(store)
          .preferredColorScheme(appearanceMode.colorScheme)
      }
    }
    .windowResizability(.contentSize)
    .defaultSize(width: 420, height: 260)

    // Settings is presented in-app as a full-window cover over the shell (see
    // ContentView + store.settingsShown), so there is no detached Settings scene.

    MenuBarExtra {
      MenuBarView()
        .environment(store)
    } label: {
      MenuBarLabel(store: store)
    }
    .menuBarExtraStyle(.window)
  }

  private var appearanceMode: AppearanceMode {
    AppearanceMode(rawValue: appearanceModeRaw) ?? .system
  }
}
