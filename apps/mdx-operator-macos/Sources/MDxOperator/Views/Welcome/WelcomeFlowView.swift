import SwiftUI

/// The native first-run welcome: a full-window cover shown once on a fresh
/// install and re-openable from the Help menu. It mirrors the web onboarding in
/// spirit (a warm hello, a short honest setup, a path chooser, a confident
/// "you're set") but is built as a premium native macOS experience, not a port.
///
/// It never traps the user: "Skip for now" (Esc) always works and lands them in
/// the app. Return advances. Every readout is real kernel state - connection,
/// owner, model - or an honest zero, never fabricated.
struct WelcomeFlowView: View {
  @Environment(OperatorStore.self) private var store
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  enum Step: Int, CaseIterable {
    case meet
    case connect
    case path
    case set
  }

  /// Snapshot/testing seam: render the flow at a specific step, optionally with
  /// the entrance motion pre-settled so an off-screen ImageRenderer (which does
  /// not fire onAppear reliably) still shows fully visible content.
  private let initialStep: Step
  private let animateEntrance: Bool

  init(initialStep: Step? = nil, animateEntrance: Bool = true) {
    let resolved = initialStep ?? Self.shotStep ?? .meet
    self.initialStep = resolved
    self.animateEntrance = animateEntrance
    _step = State(initialValue: resolved)
    _appeared = State(initialValue: !animateEntrance)
  }

  /// Deterministic capture: MDX_SHOT_WELCOME=<meet|connect|path|set> opens the
  /// flow at that step so a screenshot can show the first-win step without
  /// clicking through. The store forces the cover visible when this is set.
  static var shotStep: Step? {
    switch ProcessInfo.processInfo.environment["MDX_SHOT_WELCOME"] {
    case "meet": return .meet
    case "connect": return .connect
    case "path": return .path
    case "set": return .set
    default: return nil
    }
  }

  @State private var step: Step
  @State private var chosenPath: WelcomePath?
  @State private var appeared: Bool
  @State private var checking = false
  @State private var startingBuild = false
  @State private var ownerName = ""
  @State private var modelKey = ""
  @State private var modelProvider = "xai"

  private let modelProviders = ["xai", "openai", "anthropic", "google", "mistral", "openrouter", "ollama"]

  var body: some View {
    ZStack {
      backdrop
      VStack(spacing: 0) {
        topBar
        Spacer(minLength: 0)
        content
          .frame(maxWidth: 720)
          .padding(.horizontal, 48)
        Spacer(minLength: 0)
        progressDots
          .padding(.bottom, 30)
      }
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
    .onAppear {
      guard animateEntrance, !reduceMotion, !appeared else { return }
      withAnimation(.easeOut(duration: 0.7).delay(0.08)) { appeared = true }
    }
    .task {
      await store.loadSetupRouter()
    }
  }

  // MARK: - Chrome

  private var backdrop: some View {
    ZStack {
      Rectangle().fill(Color(nsColor: .windowBackgroundColor))
      // The accent moment: a soft wash of brand color at the top, quiet enough
      // to feel premium rather than loud.
      RadialGradient(
        colors: [Color.accentColor.opacity(0.16), Color.accentColor.opacity(0.0)],
        center: .top,
        startRadius: 0,
        endRadius: 620
      )
      .ignoresSafeArea()
    }
  }

  private var topBar: some View {
    HStack {
      HStack(spacing: 7) {
        Image(systemName: "circle.hexagongrid.fill")
          .foregroundStyle(Color.accentColor)
        Text("MDx")
          .font(.headline)
          .foregroundStyle(.secondary)
      }
      .opacity(step == .meet ? 0 : 1)
      Spacer()
      Button("Skip for now") {
        store.skipWelcome()
      }
      .buttonStyle(.plain)
      .font(.callout)
      .foregroundStyle(.secondary)
      .keyboardShortcut(.cancelAction)
      .help("Close the welcome and go straight to MDx (Esc)")
      .accessibilityLabel("Skip the welcome and go to the app")
    }
    .padding(.horizontal, 22)
    .padding(.vertical, 18)
  }

  private var progressDots: some View {
    HStack(spacing: 8) {
      ForEach(Step.allCases, id: \.rawValue) { s in
        Capsule()
          .fill(s == step ? Color.accentColor : Color.secondary.opacity(0.22))
          .frame(width: s == step ? 22 : 7, height: 7)
          .animation(reduceMotion ? nil : .spring(duration: 0.35), value: step)
      }
    }
    .accessibilityLabel("Step \(step.rawValue + 1) of \(Step.allCases.count)")
  }

  // MARK: - Content router

  @ViewBuilder
  private var content: some View {
    Group {
      switch step {
      case .meet: meetStep
      case .connect: connectStep
      case .path: pathStep
      case .set: setStep
      }
    }
    .transition(stepTransition)
    .id(step)
  }

  private var stepTransition: AnyTransition {
    if reduceMotion { return .opacity }
    return .asymmetric(
      insertion: .move(edge: .trailing).combined(with: .opacity),
      removal: .move(edge: .leading).combined(with: .opacity)
    )
  }

  private func advance(to next: Step) {
    if reduceMotion {
      step = next
    } else {
      withAnimation(.easeInOut(duration: 0.32)) { step = next }
    }
  }

  // MARK: - Step 1: Meet

  private var meetStep: some View {
    VStack(alignment: .leading, spacing: 22) {
      HStack(alignment: .firstTextBaseline, spacing: 0) {
        Text("MD")
          .foregroundStyle(.primary)
        Text("x")
          .foregroundStyle(Color.accentColor)
      }
      .font(.system(size: 92, weight: .bold, design: .rounded))
      .kerning(-2)
      .opacity(appeared ? 1 : 0)
      .offset(y: appeared ? 0 : 18)

      VStack(alignment: .leading, spacing: 12) {
        Text("Welcome. This is your MDx.")
          .font(.system(size: 30, weight: .semibold))
        Text("A governed factory for real work. Twin thinks with you, Forge ships code under your sign-off, and every decision lands with its receipt. Let's get you started.")
          .font(.title3)
          .foregroundStyle(.secondary)
          .lineSpacing(3)
          .fixedSize(horizontal: false, vertical: true)
      }
      .opacity(appeared ? 1 : 0)
      .offset(y: appeared ? 0 : 14)

      HStack(spacing: 14) {
        Button {
          advance(to: .connect)
        } label: {
          Text("Get started")
            .font(.headline)
            .padding(.horizontal, 10)
            .padding(.vertical, 3)
        }
        .mdxPrimaryButtonStyle()
        .controlSize(.large)
        .keyboardShortcut(.defaultAction)
        .accessibilityLabel("Get started with setup")

        Text("Takes about a minute")
          .font(.callout)
          .foregroundStyle(.tertiary)
      }
      .padding(.top, 4)
      .opacity(appeared ? 1 : 0)
    }
  }

  // MARK: - Step 2: Connect

  private var connected: Bool { store.snapshot.connectionStatus == .ok }

  private var connectStep: some View {
    VStack(alignment: .leading, spacing: 24) {
      WelcomeHeading(
        eyebrow: "Setup",
        title: connected ? "MDx is running on this Mac." : "Let's connect to MDx.",
        subtitle: connected
          ? "Your data lives in a snapshot file on this computer, nothing in the cloud. Here's what's already on the record."
          : "The app is open, but it can't reach the MDx engine yet. It connects the moment the local server is up."
      )

      if connected {
        connectedReadout
      } else {
        offlineReadout
      }

      HStack(spacing: 14) {
        Button {
          advance(to: .path)
        } label: {
          Text(connected ? "Continue" : "Continue anyway")
            .font(.headline)
            .padding(.horizontal, 10)
            .padding(.vertical, 3)
        }
        .mdxPrimaryButtonStyle()
        .controlSize(.large)
        .keyboardShortcut(.defaultAction)
        .accessibilityLabel("Continue to choose where to start")

        Button {
          Task {
            checking = true
            await store.refresh()
            await store.loadSetupRouter()
            checking = false
          }
        } label: {
          Label(checking ? "Checking" : "Check again", systemImage: "arrow.clockwise")
            .symbolEffect(.pulse, isActive: checking)
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .disabled(checking)
        .help("Look for the local MDx engine again")
      }
      .padding(.top, 2)
    }
  }

  private var connectedReadout: some View {
    VStack(alignment: .leading, spacing: 0) {
      WelcomeCheckRow(
        on: true,
        title: "Connected to your local engine",
        detail: store.snapshot.baseURL.absoluteString
      )
      Divider().padding(.leading, 40)
      WelcomeCheckRow(
        on: store.setupRouter.ownerClaimed,
        title: store.setupRouter.ownerClaimed
          ? "Owned by \(store.setupRouter.ownerName)"
          : "No owner claimed yet",
        detail: store.setupRouter.ownerClaimed
          ? "On the record for this install."
          : "Claim this local install here. The same receipt-backed route serves web setup."
      )
      if !store.setupRouter.ownerClaimed { ownerSetup }
      Divider().padding(.leading, 40)
      WelcomeCheckRow(
        on: store.setupRouter.modelConnected,
        title: store.setupRouter.modelConnected
          ? "\(store.setupRouter.modelID.isEmpty ? "A model" : store.setupRouter.modelID) is connected"
          : "No model connected yet",
        detail: store.setupRouter.modelConnected
          ? "MDx can do real work now."
          : "Choose a provider once. MDx discovers a usable model, verifies it live, and keeps the credential out of receipts."
      )
      if !store.setupRouter.modelConnected { modelSetup }
      Divider().padding(.leading, 40)
      WelcomeCheckRow(
        on: true,
        title: whatsHereLine,
        detail: "Real, receipt-backed counts from this workspace."
      )
    }
    .padding(.vertical, 6)
    .background(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .fill(Color.primary.opacity(0.04))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .stroke(Color.secondary.opacity(0.12), lineWidth: 1)
    )
  }

  private var ownerSetup: some View {
    HStack(spacing: 8) {
      TextField("Your name", text: $ownerName)
        .textFieldStyle(.roundedBorder)
      Button("Claim install") {
        Task { await store.claimInstallOwner(ownerName); if store.setupRouter.ownerClaimed { ownerName = "" } }
      }
      .disabled(ownerName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || store.setupActionInFlight)
    }
    .padding(.horizontal, 14)
    .padding(.bottom, 10)
  }

  private var modelSetup: some View {
    VStack(alignment: .leading, spacing: 7) {
      Picker("Provider", selection: $modelProvider) {
        ForEach(modelProviders, id: \.self) { provider in
          Text(modelProviderLabel(provider)).tag(provider)
        }
      }
      .pickerStyle(.menu)
      HStack(spacing: 8) {
        SecureField(modelProvider == "ollama" ? "No key needed" : "Provider API key", text: $modelKey)
          .textFieldStyle(.roundedBorder)
          .disabled(modelProvider == "ollama")
        Button("Connect model") {
          Task {
            await store.connectTwinModel(providerID: modelProvider, apiKey: modelKey)
            if store.setupRouter.modelConnected { modelKey = "" }
          }
        }
        .disabled(
          (modelProvider != "ollama" && modelKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            || store.setupActionInFlight
        )
      }
      Text("The provider catalog chooses a recommended text model automatically. You can fine-tune routing later in Model Center.")
        .font(.caption)
        .foregroundStyle(Color.secondary)
      if let result = store.setupActionResult, !result.detail.isEmpty {
        Text(result.detail).font(.caption).foregroundStyle(result.succeeded ? Color.secondary : Color.red)
      }
    }
    .padding(.horizontal, 14)
    .padding(.bottom, 10)
  }

  private func modelProviderLabel(_ provider: String) -> String {
    switch provider {
    case "xai": return "xAI"
    case "openai": return "OpenAI"
    case "anthropic": return "Anthropic"
    case "google": return "Google Gemini"
    case "mistral": return "Mistral"
    case "openrouter": return "OpenRouter"
    case "ollama": return "Ollama on this Mac"
    default: return provider
    }
  }

  private var whatsHereLine: String {
    let runs = store.snapshot.runCount
    let repos = store.snapshot.repoCount
    if runs == 0 && repos == 0 {
      return "A fresh workspace, ready for its first build"
    }
    let runWord = runs == 1 ? "run" : "runs"
    let repoWord = repos == 1 ? "repo" : "repos"
    return "\(runs) \(runWord) and \(repos) connected \(repoWord) already here"
  }

  private var offlineReadout: some View {
    VStack(alignment: .leading, spacing: 14) {
      HStack(spacing: 12) {
        ProgressView().controlSize(.small)
        Text("Looking for MDx at \(store.snapshot.baseURL.absoluteString)")
          .font(.callout)
          .foregroundStyle(.secondary)
      }

      VStack(alignment: .leading, spacing: 8) {
        Text("Not running yet? Start it from the MDx repo, then check again:")
          .font(.callout)
          .foregroundStyle(.secondary)
        HStack(spacing: 8) {
          Text("sh scripts/dogfood-stack.sh")
            .font(.body.monospaced())
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background(
              RoundedRectangle(cornerRadius: 7, style: .continuous)
                .fill(Color.primary.opacity(0.06))
            )
          Button {
            Pasteboard.copy("sh scripts/dogfood-stack.sh")
          } label: {
            Image(systemName: "doc.on.doc")
          }
          .buttonStyle(.plain)
          .foregroundStyle(.secondary)
          .help("Copy the start command")
          .accessibilityLabel("Copy the start command")
        }
      }

      DisclosureGroup("Point at a different address") {
        BaseURLField()
          .padding(.top, 8)
      }
      .font(.callout)
      .foregroundStyle(.secondary)
      .tint(Color.accentColor)
    }
    .padding(18)
    .background(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .fill(Color.primary.opacity(0.04))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .stroke(Color.secondary.opacity(0.12), lineWidth: 1)
    )
  }

  // MARK: - Step 3: Path chooser

  private var pathStep: some View {
    VStack(alignment: .leading, spacing: 22) {
      WelcomeHeading(
        eyebrow: store.setupRouter.ownerName.isEmpty ? "One question" : "Welcome, \(store.setupRouter.ownerName)",
        title: "Where do you want to start?",
        subtitle: "Pick a door. You can change your mind any time. Twin is always home."
      )

      VStack(spacing: 10) {
        ForEach(WelcomePath.allCases) { path in
          WelcomePathCard(path: path, selected: chosenPath == path) {
            chosenPath = path
            advance(to: .set)
          }
        }
      }
    }
  }

  // MARK: - Step 4: Your first win

  /// A safe, honest first build: a small README note that produces a reviewable
  /// branch in the connected repo. Kept minimal so it classifies as a one-shot
  /// run and reaches ready-for-review, not a mission.
  static let firstBuildIntent = "Add a short note to the README describing what this project does."

  private var canStartFirstBuild: Bool {
    FirstWin.canStartRealBuild(
      connected: connected,
      modelConnected: store.setupRouter.modelConnected,
      repoCount: store.snapshot.repoCount
    )
  }

  private var firstWinBlockedReason: String? {
    FirstWin.blockedReason(
      connected: connected,
      modelConnected: store.setupRouter.modelConnected,
      repoCount: store.snapshot.repoCount
    )
  }

  private var activeRepoLabel: String {
    store.activeRepo?.label ?? "your repo"
  }

  private var setStep: some View {
    let path = chosenPath ?? .look
    let canStart = canStartFirstBuild
    return VStack(alignment: .leading, spacing: 22) {
      ZStack {
        Circle()
          .fill(Color.accentColor.opacity(0.14))
          .frame(width: 76, height: 76)
        Image(systemName: canStart ? "hammer.fill" : "checkmark")
          .font(.system(size: canStart ? 28 : 32, weight: .bold))
          .foregroundStyle(Color.accentColor)
          .scaleEffect(appeared ? 1 : 0.6)
          .animation(reduceMotion ? nil : .spring(duration: 0.5), value: appeared)
      }
      .accessibilityHidden(true)

      VStack(alignment: .leading, spacing: 12) {
        Text(setTitle(canStart: canStart))
          .font(.system(size: 30, weight: .semibold))
          .fixedSize(horizontal: false, vertical: true)
        Text(setSummary(for: path, canStart: canStart))
          .font(.title3)
          .foregroundStyle(.secondary)
          .lineSpacing(3)
          .fixedSize(horizontal: false, vertical: true)
      }

      if canStart {
        firstBuildCard
      } else if let reason = firstWinBlockedReason {
        Label(reason, systemImage: "info.circle")
          .font(.callout)
          .foregroundStyle(.secondary)
          .padding(12)
          .frame(maxWidth: .infinity, alignment: .leading)
          .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
              .fill(Color.primary.opacity(0.04))
          )
      }

      HStack(spacing: 14) {
        if canStart {
          Button {
            startingBuild = true
            Task { await store.startFirstBuild(intent: Self.firstBuildIntent) }
          } label: {
            Label(startingBuild ? "Starting your build" : "Start my first build", systemImage: "hammer.fill")
              .font(.headline)
              .padding(.horizontal, 10)
              .padding(.vertical, 3)
          }
          .mdxPrimaryButtonStyle()
          .controlSize(.large)
          .keyboardShortcut(.defaultAction)
          .disabled(startingBuild)
          .accessibilityLabel("Start my first governed build")

          Button("Just look around instead") {
            store.finishWelcome(entering: path.route)
          }
          .buttonStyle(.plain)
          .font(.callout)
          .foregroundStyle(.secondary)
        } else {
          Button {
            store.finishWelcome(entering: path.route)
          } label: {
            Text(path.enterVerb)
              .font(.headline)
              .padding(.horizontal, 12)
              .padding(.vertical, 3)
          }
          .mdxPrimaryButtonStyle()
          .controlSize(.large)
          .keyboardShortcut(.defaultAction)
          .accessibilityLabel(path.enterVerb)

          Button("Choose a different start") {
            advance(to: .path)
          }
          .buttonStyle(.plain)
          .font(.callout)
          .foregroundStyle(.secondary)
        }
      }
      .padding(.top, 2)
    }
  }

  /// The honest preview of the real governed outcome: what MDx will do, in which
  /// repo, and the receipt-backed proof that this workspace is live.
  private var firstBuildCard: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(alignment: .top, spacing: 12) {
        Image(systemName: "shippingbox")
          .font(.title3)
          .foregroundStyle(Color.accentColor)
          .frame(width: 28)
        VStack(alignment: .leading, spacing: 3) {
          Text("Build in \(activeRepoLabel)")
            .font(.body.weight(.medium))
          Text("MDx opens a branch, adds a short note to the README, proves it, and hands it back for your review. You sign off before anything ships.")
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
        Spacer(minLength: 0)
      }
      Divider().padding(.leading, 40)
      HStack(alignment: .top, spacing: 12) {
        Image(systemName: "checkmark.seal.fill")
          .font(.title3)
          .foregroundStyle(Color.green)
          .frame(width: 28)
        Text(whatsHereLine)
          .font(.callout)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
        Spacer(minLength: 0)
      }
    }
    .padding(14)
    .background(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .fill(Color.primary.opacity(0.04))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .stroke(Color.secondary.opacity(0.12), lineWidth: 1)
    )
  }

  private func setTitle(canStart: Bool) -> String {
    let name = store.setupRouter.ownerName
    if canStart {
      return name.isEmpty ? "Let's make something real." : "Let's make something real, \(name)."
    }
    return name.isEmpty ? "You're all set." : "You're all set, \(name)."
  }

  private func setSummary(for path: WelcomePath, canStart: Bool) -> String {
    if canStart {
      let model = store.setupRouter.modelID.isEmpty ? "your model" : store.setupRouter.modelID
      return "MDx is connected and \(model) is live. Start a real build now and watch it reach your review, or look around first."
    }
    let base: String
    if connected {
      base = "MDx is connected. "
    } else {
      base = "The app is ready; live work unlocks once the engine is up. "
    }
    switch path {
    case .forge: return base + "Forge is ready to take a build."
    case .twin: return base + "Twin is ready when you are."
    case .look: return base + "Take your time and explore."
    }
  }
}

// MARK: - Shared pieces

/// A generous heading block: eyebrow, large title, quiet subtitle. Anchors each
/// step with real type hierarchy instead of the shell's dense captions.
private struct WelcomeHeading: View {
  let eyebrow: String
  let title: String
  let subtitle: String

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Text(eyebrow.uppercased())
        .font(.caption.weight(.semibold))
        .kerning(1.4)
        .foregroundStyle(.tertiary)
      Text(title)
        .font(.system(size: 30, weight: .semibold))
        .fixedSize(horizontal: false, vertical: true)
      Text(subtitle)
        .font(.title3)
        .foregroundStyle(.secondary)
        .lineSpacing(3)
        .fixedSize(horizontal: false, vertical: true)
    }
  }
}

/// One line in the connected readout: a state dot, a human title, a quiet detail.
private struct WelcomeCheckRow: View {
  let on: Bool
  let title: String
  let detail: String

  var body: some View {
    HStack(alignment: .top, spacing: 12) {
      Image(systemName: on ? "checkmark.circle.fill" : "circle")
        .font(.title3)
        .foregroundStyle(on ? Color.green : Color.secondary.opacity(0.5))
        .frame(width: 28)
      VStack(alignment: .leading, spacing: 2) {
        Text(title)
          .font(.body.weight(.medium))
        Text(detail)
          .font(.callout)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
      Spacer(minLength: 0)
    }
    .padding(.horizontal, 14)
    .padding(.vertical, 12)
    .accessibilityElement(children: .combine)
    .accessibilityLabel("\(on ? "Done" : "Not yet"): \(title). \(detail)")
  }
}

/// A tappable path card: tinted glyph, title, blurb, and a go affordance.
private struct WelcomePathCard: View {
  let path: WelcomePath
  let selected: Bool
  let action: () -> Void

  @State private var hovering = false

  var body: some View {
    Button(action: action) {
      HStack(alignment: .center, spacing: 16) {
        ZStack {
          RoundedRectangle(cornerRadius: 10, style: .continuous)
            .fill(Color.accentColor.opacity(0.14))
            .frame(width: 44, height: 44)
          Image(systemName: path.icon)
            .font(.title3)
            .foregroundStyle(Color.accentColor)
        }
        VStack(alignment: .leading, spacing: 3) {
          Text(path.title)
            .font(.headline)
          Text(path.blurb)
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
        Spacer(minLength: 8)
        Image(systemName: "arrow.right")
          .font(.body.weight(.semibold))
          .foregroundStyle(hovering ? AnyShapeStyle(Color.accentColor) : AnyShapeStyle(.tertiary))
      }
      .padding(16)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(
        RoundedRectangle(cornerRadius: 12, style: .continuous)
          .fill(Color.primary.opacity(hovering ? 0.06 : 0.04))
      )
      .overlay(
        RoundedRectangle(cornerRadius: 12, style: .continuous)
          .stroke(hovering ? Color.accentColor.opacity(0.6) : Color.secondary.opacity(0.12), lineWidth: 1)
      )
    }
    .buttonStyle(.plain)
    .onHover { hovering = $0 }
    .accessibilityLabel("\(path.title). \(path.blurb)")
    .accessibilityAddTraits(.isButton)
  }
}

/// The base-URL editor, reused from the offline connect path. Kept in the flow
/// so a user whose engine lives elsewhere can point at it without opening
/// Settings first.
private struct BaseURLField: View {
  @Environment(OperatorStore.self) private var store
  @State private var text = ""

  var body: some View {
    HStack(spacing: 8) {
      TextField("http://127.0.0.1:18890", text: $text)
        .textFieldStyle(.roundedBorder)
        .font(.body.monospaced())
      Button("Use it") {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        store.updateBaseURL(trimmed)
        Task { await store.refresh(); await store.loadSetupRouter() }
      }
      .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
    }
    .onAppear { text = store.snapshot.baseURL.absoluteString }
  }
}
