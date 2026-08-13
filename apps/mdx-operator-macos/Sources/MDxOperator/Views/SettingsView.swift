import AppKit
import SwiftUI

enum SettingsSection: String, CaseIterable, Identifiable {
  case account
  case general
  case appearance
  case projects
  case connection
  case models
  case diagnostics
  case authority
  case updates
  case about

  var id: String { rawValue }

  static func screenshotSection(
    in environment: [String: String] = ProcessInfo.processInfo.environment
  ) -> SettingsSection? {
    guard let raw = environment["MDX_SHOT_SETTINGS"]?
      .trimmingCharacters(in: .whitespacesAndNewlines)
      .lowercased(),
      !raw.isEmpty
    else { return nil }
    return SettingsSection(rawValue: raw)
  }

  var title: String {
    switch self {
    case .account: "Account"
    case .general: "General"
    case .appearance: "Appearance"
    case .projects: "Projects"
    case .connection: "Connection"
    case .models: "Models"
    case .diagnostics: "Diagnostics"
    case .authority: "Authority"
    case .updates: "Updates"
    case .about: "About"
    }
  }

  var subtitle: String {
    switch self {
    case .account: "Identity and access"
    case .general: "Panels and shortcuts"
    case .appearance: "Theme and display"
    case .projects: "Local folders on this Mac"
    case .connection: "Local MDx host"
    case .models: "Shared model access"
    case .diagnostics: "Performance and health"
    case .authority: "Boundaries and gates"
    case .updates: "Build and release"
    case .about: "Version and credits"
    }
  }

  var systemImage: String {
    switch self {
    case .account: "person.crop.circle"
    case .general: "gearshape"
    case .appearance: "sun.max"
    case .projects: "folder"
    case .connection: "network"
    case .models: "cpu"
    case .diagnostics: "waveform.path.ecg"
    case .authority: "lock.shield"
    case .updates: "arrow.triangle.2.circlepath"
    case .about: "info.circle"
    }
  }

  var group: SettingsGroupKind {
    switch self {
    case .account, .general, .appearance: .personal
    case .projects, .connection, .models, .diagnostics: .local
    case .authority, .updates, .about: .system
    }
  }
}

enum SettingsGroupKind: String, CaseIterable, Identifiable {
  case personal
  case local
  case system

  var id: String { rawValue }

  var label: String {
    switch self {
    case .personal: "Personal"
    case .local: "This Mac"
    case .system: "System"
    }
  }
}

/// The full-window in-app settings cover (Codex-desktop style): a top bar with a
/// "Back to app" affordance over the existing SettingsView. Presented above the
/// shell in ContentView and gated on `store.settingsShown`, it replaces the old
/// detached macOS Settings window. Esc and Cmd-. also close it.
struct SettingsCoverView: View {
  @Environment(OperatorStore.self) private var store
  let initialSection: SettingsSection

  init(initialSection: SettingsSection = .account) {
    self.initialSection = initialSection
  }

  var body: some View {
    VStack(spacing: 0) {
      SettingsCoverTopBar { store.hideSettings() }
      Divider()
      SettingsView(initialSection: initialSection)
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background(Color(nsColor: .windowBackgroundColor))
    // Cmd-. closes too, matching the Esc / Back-to-app affordances. Hidden but
    // still wired; a real button so the shortcut is discoverable to VoiceOver.
    .background(
      Button("Close settings") { store.hideSettings() }
        .keyboardShortcut(".", modifiers: .command)
        .hidden()
    )
  }
}

/// The cover's chrome: "Back to app" top-left (Esc), a quiet centered title, and
/// balancing space so the title stays centered. Chrome materials only.
private struct SettingsCoverTopBar: View {
  let back: () -> Void

  var body: some View {
    ZStack {
      Text("Settings")
        .font(.headline)
        .foregroundStyle(.secondary)

      HStack {
        Button(action: back) {
          HStack(spacing: 6) {
            Image(systemName: "chevron.left")
              .font(.body.weight(.semibold))
            Text("Back to app")
          }
          .font(.callout.weight(.medium))
          .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .keyboardShortcut(.cancelAction)
        .help("Return to MDx (Esc)")
        .accessibilityLabel("Back to app")

        Spacer()
      }
    }
    .padding(.horizontal, 18)
    .padding(.vertical, 12)
    .background(.bar)
  }
}

struct SettingsView: View {
  @Environment(OperatorStore.self) private var store
  @Environment(CloudAuthStore.self) private var auth
  @AppStorage(OperatorStore.apiURLDefaultsKey) private var apiURL = MDxRouteClient.defaultBaseURL.absoluteString
  @AppStorage(OperatorStore.appearanceModeDefaultsKey) private var appearanceModeRaw = AppearanceMode.system.rawValue
  @AppStorage(OperatorStore.showInspectorDefaultsKey) private var showInspector = false
  @State private var selectedSection: SettingsSection = .account
  @State private var searchText = ""
  @State private var modelProvider = "xai"
  @State private var modelCredential = ""

  init(initialSection: SettingsSection = .account) {
    _selectedSection = State(initialValue: initialSection)
  }

  var body: some View {
    HStack(spacing: 0) {
      settingsSidebar
        .frame(width: 272)

      Divider()

      settingsDetail
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .background(Color(nsColor: .windowBackgroundColor))
    .task {
      await store.loadYou()
      await store.loadSetupRouter()
    }
  }

  @ViewBuilder
  private var settingsDetail: some View {
    ScrollView {
      if #available(macOS 26.0, *) {
        GlassEffectContainer(spacing: 14) {
          settingsContent
        }
      } else {
        settingsContent
      }
    }
    .frame(maxWidth: .infinity)
  }

  private var settingsSidebar: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(spacing: 6) {
        Image(systemName: "magnifyingglass")
          .font(.caption)
          .foregroundStyle(.tertiary)
        TextField("Search settings", text: $searchText)
          .textFieldStyle(.plain)
      }
      .padding(.horizontal, 9)
      .padding(.vertical, 6)
      .background(
        RoundedRectangle(cornerRadius: 8, style: .continuous)
          .fill(Color.primary.opacity(0.06))
      )
      .padding(.horizontal, 10)
      .padding(.top, 16)

      // The sections fit above the footer in the app's minimum window height,
      // so this stays a plain list and keeps every destination immediately
      // available without nesting another scroll region.
      VStack(alignment: .leading, spacing: 4) {
        ForEach(SettingsGroupKind.allCases) { group in
          let sections = filteredSections.filter { $0.group == group }
          if !sections.isEmpty {
            Text(group.label.uppercased())
              .font(.caption2.weight(.semibold))
              .foregroundStyle(.tertiary)
              .padding(.horizontal, 12)
              .padding(.top, 10)
              .padding(.bottom, 2)
            ForEach(sections) { section in
              SettingsSidebarRow(
                section: section,
                selected: section == selectedSection
              ) {
                selectedSection = section
              }
            }
          }
        }
      }
      .padding(.horizontal, 8)

      Spacer(minLength: 0)

      SidebarAccountFooter(name: userDisplayName)
        .padding(10)
    }
    .background(SidebarBackground())
  }

  private var settingsContent: some View {
    VStack(alignment: .leading, spacing: 28) {
      Text(selectedSection.title)
        .font(.title.weight(.semibold))
        .padding(.top, 46)

      switch selectedSection {
      case .account:
        accountSettings
      case .general:
        generalSettings
      case .appearance:
        appearanceSettings
      case .projects:
        projectsSettings
      case .connection:
        connectionSettings
      case .models:
        modelSettings
      case .diagnostics:
        diagnosticsSettings
      case .authority:
        authoritySettings
      case .updates:
        updateSettings
      case .about:
        aboutSettings
      }
    }
    .padding(.horizontal, 56)
    .padding(.bottom, 50)
    .frame(maxWidth: 760, alignment: .leading)
  }

  // MARK: - Account (folded-in "You")

  private var accountSettings: some View {
    VStack(alignment: .leading, spacing: 22) {
      SettingsSectionHeader(
        title: "Who you are here",
        subtitle: "Your identity, current role, and what stays governed."
      )

      if let session = store.identitySession {
        IdentityCard(
          session: session,
          displayName: userDisplayName,
          workspaceName: isHosted ? "Personal beta workspace" : "Local workspace"
        )
      } else {
        SettingsGroup {
          SettingsInfoRow(
            title: "Local operator",
            subtitle: "Identity loads from the local auth session when MDx is connected.",
            value: store.snapshot.connectionStatus == .ok ? "Loading" : "Offline"
          )
        }
      }

      if let clearance = store.clearance {
        ClearanceCard(clearance: clearance)
      }
    }
  }

  // MARK: - General

  private var generalSettings: some View {
    VStack(alignment: .leading, spacing: 26) {
      SettingsSectionHeader(
        title: "Panels",
        subtitle: "How MDx opens and where it shows detail."
      )

      SettingsGroup {
        SettingsToggleRow(
          title: "Show context panel by default",
          subtitle: "The right-side context and proof panel. Toggle anytime with Command Option I.",
          isOn: $showInspector
        )
      }

      Text("Command K opens the palette to jump to any surface or action.")
        .font(.caption)
        .foregroundStyle(.secondary)
    }
  }

  // MARK: - Appearance

  private var appearanceSettings: some View {
    VStack(alignment: .leading, spacing: 26) {
      SettingsSectionHeader(
        title: "Theme",
        subtitle: "MDx follows macOS, or you can pin light or dark."
      )

      HStack(spacing: 12) {
        ForEach(AppearanceMode.allCases) { mode in
          AppearanceSwatch(
            mode: mode,
            selected: appearanceMode == mode
          ) {
            appearanceModeRaw = mode.rawValue
          }
        }
      }

      Text("MDx uses system glass on macOS 26 (with a material fallback on older macOS), keeps a compact sidebar, and follows your macOS accent color.")
        .font(.caption)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
    }
  }

  // MARK: - Projects (local-first advantage)

  private var projectsSettings: some View {
    VStack(alignment: .leading, spacing: 20) {
      SettingsSectionHeader(
        title: "Projects on this Mac",
        subtitle: "MDx works against local git checkouts. Add the folders you want it to reason about, right where they live on disk."
      )

      Button {
        chooseFolder()
      } label: {
        Label("Add project folder", systemImage: "folder.badge.plus")
      }

      if store.hostProjects.isEmpty {
        SettingsGroup {
          SettingsInfoRow(
            title: "No project folders yet",
            subtitle: "Point MDx at a repo checkout so builds and reviews can reference it locally.",
            value: ""
          )
        }
      } else {
        SettingsGroup {
          ForEach(Array(store.hostProjects.enumerated()), id: \.element.id) { index, project in
            if index > 0 { Divider() }
            ProjectRow(
              project: project,
              reveal: { reveal(project) },
              remove: { store.removeHostProject(project) }
            )
          }
        }
      }
    }
  }

  // MARK: - Connection

  private var connectionSettings: some View {
    VStack(alignment: .leading, spacing: 20) {
      SettingsSectionHeader(
        title: "Where MDx is running",
        subtitle: isHosted
          ? "This app is connected to your private MDx Cloud workspace."
          : "This app is connected to an MDx service on this Mac."
      )

      SettingsGroup {
        SettingsInfoRow(
          title: isHosted ? "MDx Cloud" : "This Mac",
          subtitle: isHosted
            ? "Your account, builds, reviews, and cloud model access stay together here."
            : "Projects and model access can remain on this Mac.",
          value: store.snapshot.connectionStatus == .ok ? "Connected" : "Needs attention"
        )

        if !Bundle.main.macDistributionProfile.isCanary || !isHosted {
          Divider()
          DisclosureGroup("Advanced connection address") {
            VStack(alignment: .leading, spacing: 8) {
              TextField("http://127.0.0.1:18890", text: $apiURL)
                .textFieldStyle(.roundedBorder)
                .onSubmit { applyConnection() }
              Text(store.snapshot.baseURL.absoluteString)
                .font(.caption.monospaced())
                .foregroundStyle(.tertiary)
                .textSelection(.enabled)
            }
            .padding(.top, 8)
          }
        }
      }

      HostPairingCard(
        baseURL: store.snapshot.baseURL,
        accessTokenProvider: { await auth.accessToken() },
        // Pairing can present system UI and mutate a real host relationship.
        // The deterministic gallery is read-only; normal app launches prepare.
        autoPrepare: SettingsSection.screenshotSection() == nil
      )

      Button {
        applyConnection()
      } label: {
        Label(isHosted ? "Refresh connection" : "Save and refresh", systemImage: "arrow.clockwise")
      }
      .mdxPrimaryButtonStyle()
    }
  }

  // MARK: - Models

  private var modelSettings: some View {
    VStack(alignment: .leading, spacing: 20) {
      SettingsSectionHeader(
        title: "Models available to MDx",
        subtitle: isHosted
          ? "Cloud work uses models provided by your workspace. Local work can use a model that stays on this Mac."
          : "Choose the models this Mac can use for Twin and Forge."
      )

      SettingsGroup {
        SettingsInfoRow(
          title: isHosted ? "MDx Cloud models" : "Models on this Mac",
          subtitle: store.setupRouter.modelConnected
            ? "Ready for Twin, Forge, web, and iPhone work in this workspace."
            : "MDx will show setup here when this workspace still needs a model.",
          value: store.setupRouter.modelConnected ? "Available" : "Needs setup"
        )

        if isHosted {
          Divider()
          SettingsInfoRow(
            title: "Models on this Mac",
            subtitle: "Optional for private, offline, or low-latency work. Using local and cloud models together is held until a local MDx host can stay connected beside the cloud workspace.",
            value: "Optional"
          )
        }

        if !isHosted || !store.setupRouter.modelConnected {
          Divider()
          modelConnectionFields
        }
      }

      if !isHosted || !store.setupRouter.modelConnected {
        Button {
          Task {
            await store.connectTwinModel(providerID: modelProvider, apiKey: modelCredential)
            if store.setupActionResult?.status == "MODEL_CONNECTED" {
              modelCredential = ""
            }
          }
        } label: {
          if store.setupActionInFlight {
            ProgressView().controlSize(.small)
          } else {
            Label("Connect and verify", systemImage: "checkmark.seal")
          }
        }
        .disabled(
          store.setupActionInFlight
            || (modelProvider != "ollama"
              && modelCredential.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        )
        .mdxPrimaryButtonStyle()
      }

      if let result = store.setupActionResult {
        SettingsGroup {
          SettingsInfoRow(
            title: result.title,
            subtitle: result.detail,
            value: result.status == "MODEL_CONNECTED" ? "Ready" : "Needs attention"
          )
        }
      }
    }
  }

  private var modelConnectionFields: some View {
    VStack(alignment: .leading, spacing: 10) {
      Picker("Provider", selection: $modelProvider) {
        Text("xAI").tag("xai")
        Text("OpenAI").tag("openai")
        Text("Anthropic").tag("anthropic")
        Text("Google Gemini").tag("google")
        Text("Mistral").tag("mistral")
        Text("OpenRouter").tag("openrouter")
        Text("Ollama on this Mac").tag("ollama")
      }
      .pickerStyle(.menu)

      if modelProvider != "ollama" {
        SecureField("Provider API key", text: $modelCredential)
          .textFieldStyle(.roundedBorder)
      }

      Text("MDx verifies the connection before it becomes available. The key is never written to receipts or app state.")
        .font(.caption)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
    }
    .padding(.vertical, 8)
  }

  // MARK: - Diagnostics

  private var diagnosticsSettings: some View {
    VStack(alignment: .leading, spacing: 20) {
      SettingsSectionHeader(
        title: "Performance and health",
        subtitle: "Launch time, route latency, stream first-token, hangs, and your recent trail, all local."
      )

      DiagnosticsCard()

      Button {
        store.showDiagnosticsPanel()
      } label: {
        Label("Open full report", systemImage: "waveform.path.ecg")
      }
      .mdxPrimaryButtonStyle()
    }
  }

  // MARK: - Authority

  private var authoritySettings: some View {
    VStack(alignment: .leading, spacing: 20) {
      SettingsSectionHeader(
        title: "What MDx can do without asking",
        subtitle: "See when MDx can keep moving and when it must stop for you."
      )

      SettingsGroup {
        SettingsInfoRow(
          title: "Current autonomy",
          subtitle: authorityModeText == "Read only"
            ? "MDx can inspect and explain work, but it cannot start or change it on its own."
            : "MDx can act only inside authority already recorded for this workspace.",
          value: authorityModeText
        )
        Divider()
        SettingsInfoRow(
          title: "What needs you now",
          subtitle: store.snapshot.humanDecision,
          value: "Your decision"
        )
        ForEach(lockedAuthorityItems) { item in
          Divider()
          SettingsInfoRow(title: item.title, subtitle: item.detail, value: "MDx will ask")
        }
      }

      Button("Open Forge controls") {
        store.hideSettings()
        store.select(AppRoute.forge(.evidence))
      }
      .mdxPrimaryButtonStyle()
    }
  }

  private var authorityModeText: String {
    let raw = store.snapshot.authority
    if raw.isEmpty || raw == "none" { return "Read only" }
    return StatusCopy.human(raw)
  }

  private var lockedAuthorityItems: [AuthorityItem] {
    store.snapshot.authorityItems.filter { !$0.allowed }
  }

  // MARK: - Updates

  private var updateSettings: some View {
    VStack(alignment: .leading, spacing: 20) {
      SettingsSectionHeader(
        title: "Keep MDx up to date",
        subtitle: "See the version on this Mac and download a newer signed release when one is ready."
      )

      if let kernel = store.kernelVersion {
        SettingsGroup {
          SettingsInfoRow(
            title: isHosted ? "MDx service" : "Local MDx service",
            subtitle: isHosted
              ? "The service and this app check that they can work together."
              : "This Mac is running a compatible MDx service.",
            value: kernel.kernelVersion
          )
        }
      }

      SettingsGroup {
        SettingsInfoRow(
          title: "This build",
          subtitle: Bundle.main.macDistributionProfile.updateDetail,
          value: Bundle.main.appVersionLabel
        )
      }

      if let release = store.availableMacRelease {
        SettingsGroup {
          SettingsInfoRow(
            title: "Update available",
            subtitle: "A newer signed canary is ready on the private download page.",
            value: "\(release.version) (\(release.build))"
          )
        }
      }

      HStack(spacing: 10) {
        Button("Check for updates") {
          Task { await store.checkForMacUpdate() }
        }
        .disabled(store.macReleaseCheckInFlight)

        if store.availableMacRelease != nil {
          Button("Open secure download") { store.openMacUpdate() }
            .buttonStyle(.borderedProminent)
        }
      }

      if !Bundle.main.macDistributionProfile.isCanary {
        PackagingPanel()
      } else {
        Text("This canary was signed with Developer ID, notarized by Apple, and checked before it was published.")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
    }
  }

  // MARK: - About

  private var aboutSettings: some View {
    VStack(alignment: .leading, spacing: 20) {
      HStack(spacing: 14) {
        RoundedRectangle(cornerRadius: 14, style: .continuous)
          .fill(Color.accentColor.gradient)
          .frame(width: 54, height: 54)
          .overlay(
            Image(systemName: "hammer.fill")
              .font(.title2)
              .foregroundStyle(.white)
          )
        VStack(alignment: .leading, spacing: 3) {
          Text("MDx")
            .font(.title2.weight(.semibold))
          Text("Version \(appVersion)")
            .font(.callout)
            .foregroundStyle(.secondary)
          Text(Bundle.main.macDistributionProfile.isCanary ? "Private macOS canary" : "Local-first operator workbench")
            .font(.caption)
            .foregroundStyle(.tertiary)
        }
        Spacer()
      }

      SettingsGroup {
        SettingsInfoRow(
          title: "Runs against",
          subtitle: store.snapshot.baseURL.scheme == "https"
            ? "Your verified hosted MDx session is the source of truth."
            : "The local MDx route server is the source of truth.",
          value: store.snapshot.baseURL.absoluteString
        )
        Divider()
        SettingsInfoRow(
          title: "Bundle",
          subtitle: Bundle.main.macDistributionProfile.isCanary
            ? "Developer ID signed and Apple notarized before private publication."
            : "Packaged locally as dist/MDx.app.",
          value: Bundle.main.macDistributionProfile.isCanary ? "Canary" : "Local beta"
        )
        Divider()
        SettingsInfoRow(
          title: "Boundary",
          subtitle: "Reads local runtime proof. Writes stay governed.",
          value: "Fail-closed"
        )
      }
    }
  }

  private var appVersion: String {
    let info = Bundle.main.infoDictionary
    let short = info?["CFBundleShortVersionString"] as? String
    let build = info?["CFBundleVersion"] as? String
    switch (short, build) {
    case let (v?, b?): return "\(v) (\(b))"
    case let (v?, nil): return v
    default: return "local"
    }
  }

  private var filteredSections: [SettingsSection] {
    let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !query.isEmpty else {
      return SettingsSection.allCases
    }
    return SettingsSection.allCases.filter { section in
      section.title.localizedCaseInsensitiveContains(query)
        || section.subtitle.localizedCaseInsensitiveContains(query)
    }
  }

  private var appearanceMode: AppearanceMode {
    AppearanceMode(rawValue: appearanceModeRaw) ?? .system
  }

  private var userDisplayName: String {
    let cloudName = auth.displayName.trimmingCharacters(in: .whitespacesAndNewlines)
    if !cloudName.isEmpty { return cloudName }
    let fullName = NSFullUserName().trimmingCharacters(in: .whitespacesAndNewlines)
    if !fullName.isEmpty {
      return fullName
    }
    return NSUserName()
  }

  private var isHosted: Bool {
    store.snapshot.baseURL.scheme?.lowercased() == "https"
  }

  private func applyConnection() {
    store.updateBaseURL(apiURL)
    Task { await store.refresh() }
  }

  private func chooseFolder() {
    let panel = NSOpenPanel()
    panel.canChooseFiles = false
    panel.canChooseDirectories = true
    panel.allowsMultipleSelection = false
    panel.prompt = "Add"
    if panel.runModal() == .OK, let url = panel.url {
      store.addHostProject(path: url.path)
    }
  }

  private func reveal(_ project: HostProject) {
    let url = URL(fileURLWithPath: project.path)
    NSWorkspace.shared.activateFileViewerSelecting([url])
  }
}

// MARK: - Rows and swatches

struct ProjectRow: View {
  let project: HostProject
  let reveal: () -> Void
  let remove: () -> Void
  @State private var hovering = false

  private var folderName: String {
    URL(fileURLWithPath: project.path).lastPathComponent
  }

  var body: some View {
    HStack(spacing: 12) {
      Image(systemName: "folder.fill")
        .foregroundStyle(Color.accentColor)
      VStack(alignment: .leading, spacing: 2) {
        Text(folderName)
          .font(.subheadline.weight(.semibold))
          .lineLimit(1)
        Text(project.path)
          .font(.caption)
          .foregroundStyle(.secondary)
          .lineLimit(1)
          .truncationMode(.middle)
          .textSelection(.enabled)
      }
      Spacer(minLength: 8)

      Button(action: reveal) {
        Image(systemName: "arrow.up.forward.app")
      }
      .buttonStyle(.plain)
      .foregroundStyle(.secondary)
      .help("Reveal in Finder")
      .opacity(hovering ? 1 : 0.55)

      Button(action: remove) {
        Image(systemName: "minus.circle")
      }
      .buttonStyle(.plain)
      .foregroundStyle(.secondary)
      .help("Remove from MDx")
      .opacity(hovering ? 1 : 0.55)
    }
    .padding(.vertical, 10)
    .onHover { hovering = $0 }
  }
}

/// A light/dark/system swatch that previews the choice, matching Codex's
/// appearance picker feel.
struct AppearanceSwatch: View {
  let mode: AppearanceMode
  let selected: Bool
  let select: () -> Void

  var body: some View {
    Button(action: select) {
      VStack(spacing: 10) {
        preview
          .frame(height: 74)
          .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
          .overlay(
            RoundedRectangle(cornerRadius: 10, style: .continuous)
              .stroke(selected ? Color.accentColor : Color.secondary.opacity(0.2), lineWidth: selected ? 2 : 1)
          )
        HStack(spacing: 5) {
          Image(systemName: selected ? "checkmark.circle.fill" : mode.systemImage)
            .font(.caption)
            .foregroundStyle(selected ? Color.accentColor : .secondary)
          Text(mode.title)
            .font(.caption.weight(.medium))
            .foregroundStyle(selected ? .primary : .secondary)
        }
      }
      .padding(6)
      .contentShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
    .buttonStyle(.plain)
    .frame(maxWidth: .infinity)
  }

  @ViewBuilder
  private var preview: some View {
    switch mode {
    case .light:
      swatchBody(bg: Color(white: 0.97), bar: Color(white: 0.88), text: Color(white: 0.6))
    case .dark:
      swatchBody(bg: Color(white: 0.13), bar: Color(white: 0.25), text: Color(white: 0.5))
    case .system:
      HStack(spacing: 0) {
        swatchBody(bg: Color(white: 0.97), bar: Color(white: 0.88), text: Color(white: 0.6))
        swatchBody(bg: Color(white: 0.13), bar: Color(white: 0.25), text: Color(white: 0.5))
      }
    }
  }

  private func swatchBody(bg: Color, bar: Color, text: Color) -> some View {
    ZStack(alignment: .topLeading) {
      bg
      VStack(alignment: .leading, spacing: 4) {
        RoundedRectangle(cornerRadius: 2).fill(Color.accentColor).frame(width: 22, height: 4)
        RoundedRectangle(cornerRadius: 2).fill(text.opacity(0.6)).frame(width: 34, height: 3)
        RoundedRectangle(cornerRadius: 2).fill(text.opacity(0.4)).frame(width: 28, height: 3)
      }
      .padding(10)
    }
  }
}

struct SettingsSidebarRow: View {
  let section: SettingsSection
  let selected: Bool
  let select: () -> Void

  var body: some View {
    Button {
      select()
    } label: {
      HStack(spacing: 10) {
        Image(systemName: section.systemImage)
          .font(.system(size: 14, weight: .medium))
          .frame(width: 18)
          .foregroundStyle(selected ? Color.accentColor : Color.secondary)
        Text(section.title)
          .font(.subheadline.weight(selected ? .semibold : .regular))
        Spacer(minLength: 0)
      }
      .padding(.horizontal, 10)
      .padding(.vertical, 7)
      .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }
    .buttonStyle(.plain)
    .background(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .fill(selected ? Color.accentColor.opacity(0.12) : Color.clear)
    )
  }
}

struct SettingsSectionHeader: View {
  let title: String
  let subtitle: String

  var body: some View {
    VStack(alignment: .leading, spacing: 4) {
      Text(title)
        .font(.headline)
      Text(subtitle)
        .font(.callout)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
    }
  }
}

struct SettingsGroup<Content: View>: View {
  @ViewBuilder let content: Content

  var body: some View {
    VStack(spacing: 0) {
      content
    }
    .padding(.horizontal, 14)
    .padding(.vertical, 6)
    .background(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .fill(Color.primary.opacity(0.035))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .stroke(Color.secondary.opacity(0.12), lineWidth: 1)
    )
  }
}

struct PreferenceChoiceCard: View {
  let title: String
  let subtitle: String
  let systemImage: String
  let selected: Bool
  let select: () -> Void
  @State private var hovering = false

  var body: some View {
    Button {
      select()
    } label: {
      HStack(alignment: .center, spacing: 12) {
        Image(systemName: systemImage)
          .font(.title3)
          .frame(width: 24)
          .foregroundStyle(selected ? Color.accentColor : Color.secondary)

        VStack(alignment: .leading, spacing: 3) {
          Text(title)
            .font(.subheadline.weight(.semibold))
          Text(subtitle)
            .font(.callout)
            .foregroundStyle(.secondary)
            .lineLimit(2)
        }

        Spacer(minLength: 8)

        Image(systemName: selected ? "checkmark.circle.fill" : "circle")
          .foregroundStyle(selected ? Color.accentColor : Color.secondary.opacity(0.45))
      }
      .padding(14)
      .frame(maxWidth: .infinity, minHeight: 76, alignment: .leading)
      .contentShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
    .buttonStyle(.plain)
    .background(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .fill(selected ? Color.accentColor.opacity(0.08) : (hovering ? Color.primary.opacity(0.05) : Color.primary.opacity(0.02)))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .stroke(selected ? Color.accentColor.opacity(0.5) : Color.secondary.opacity(0.12), lineWidth: 1)
    )
    .onHover { hovering = $0 }
  }
}

struct SettingsToggleRow: View {
  let title: String
  let subtitle: String
  @Binding var isOn: Bool

  var body: some View {
    HStack(alignment: .center, spacing: 12) {
      VStack(alignment: .leading, spacing: 3) {
        Text(title)
          .font(.subheadline.weight(.semibold))
        Text(subtitle)
          .font(.callout)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
      Spacer(minLength: 16)
      Toggle("", isOn: $isOn)
        .labelsHidden()
        .toggleStyle(.switch)
    }
    .padding(.vertical, 11)
  }
}

struct SettingsInfoRow: View {
  let title: String
  let subtitle: String
  let value: String

  var body: some View {
    HStack(alignment: .center, spacing: 12) {
      VStack(alignment: .leading, spacing: 3) {
        Text(title)
          .font(.subheadline.weight(.semibold))
        Text(subtitle)
          .font(.callout)
          .foregroundStyle(.secondary)
          .lineLimit(3)
          .fixedSize(horizontal: false, vertical: true)
      }
      Spacer(minLength: 16)
      if !value.isEmpty {
        Text(value)
          .font(.callout.weight(.medium))
          .foregroundStyle(.secondary)
          .lineLimit(1)
      }
    }
    .padding(.vertical, 11)
  }
}
