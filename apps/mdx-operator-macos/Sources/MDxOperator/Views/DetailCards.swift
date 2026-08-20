import AppKit
import SwiftUI

struct SectionHeader: View {
  let title: String
  let subtitle: String

  var body: some View {
    VStack(alignment: .leading, spacing: 4) {
      Text(title)
        .font(.title3.weight(.semibold))
      Text(subtitle)
        .font(.callout)
        .foregroundStyle(.secondary)
    }
  }
}

struct SurfacePreviewItem: Identifiable {
  let id: String
  let title: String
  let detail: String
  let status: String
  let systemImage: String
}

struct ConnectHeroCard: View {
  @Environment(OperatorStore.self) private var store
  let connect: () -> Void

  private var isHosted: Bool {
    store.snapshot.baseURL.scheme?.lowercased() == "https"
  }

  /// Overridable for beta installs whose start command differs.
  private var startCommand: String {
    UserDefaults.standard.string(forKey: "MDxStartCommand") ?? "sh scripts/dogfood-stack.sh"
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      HStack(alignment: .top, spacing: 14) {
        Image(systemName: "bolt.horizontal.circle.fill")
          .font(.system(size: 30))
          .foregroundStyle(Color.accentColor)
        VStack(alignment: .leading, spacing: 4) {
          Text(isHosted ? "Reconnect to MDx Cloud" : "Connect local MDx to begin")
            .font(.title3.weight(.semibold))
          Text(isHosted
            ? "Your account is signed in, but this app cannot reach your private workspace right now. Retry to restore live work, decisions, and proof."
            : "The native shell is ready. Start the local route server, then connect to see live work, decisions, and proof.")
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
        Spacer(minLength: 12)
      }

      HStack(spacing: 12) {
        Button {
          connect()
        } label: {
          Label(store.phase == .loading ? "Connecting" : (isHosted ? "Retry" : "Connect"), systemImage: "arrow.clockwise")
        }
        .mdxPrimaryButtonStyle()
        .disabled(store.phase == .loading)

        Text(store.snapshot.baseURL.absoluteString)
          .font(.caption.monospaced())
          .foregroundStyle(.secondary)
        Spacer(minLength: 0)
      }

      HStack(spacing: 7) {
        ProgressView().controlSize(.mini)
        Text(isHosted
          ? "Looking for your private MDx Cloud workspace."
          : "Looking for MDx on this Mac. It connects the moment the local server is up.")
          .font(.caption)
          .foregroundStyle(.tertiary)
      }

      if !isHosted {
        VStack(alignment: .leading, spacing: 6) {
          Text("Not running yet? Start it from the MDx repo:")
            .font(.caption)
            .foregroundStyle(.secondary)
          HStack(spacing: 8) {
            Text(startCommand)
              .font(.caption.monospaced())
              .padding(.horizontal, 8)
              .padding(.vertical, 5)
              .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                  .fill(Color.primary.opacity(0.06))
              )
            Button {
              Pasteboard.copy(startCommand)
            } label: {
              Image(systemName: "doc.on.doc")
                .font(.caption)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .help("Copy the start command")
            .accessibilityLabel("Copy the start command")
          }
        }
      }
    }
    .padding(20)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .fill(Color.accentColor.opacity(0.08))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 12, style: .continuous)
        .stroke(Color.accentColor.opacity(0.25), lineWidth: 1)
    )
    .task {
      // Quiet auto-connect poll: first run should not require clicking a
      // button after starting the server. refresh() is coalesced, so this is
      // one cheap in-flight load at a time.
      while !Task.isCancelled, store.snapshot.connectionStatus != .ok {
        try? await Task.sleep(nanoseconds: 3_000_000_000)
        guard !Task.isCancelled else { return }
        await store.refresh()
      }
    }
  }
}

struct TwinComposerCard: View {
  @Environment(OperatorStore.self) private var store
  @State private var prompt = ""
  @FocusState private var promptFocused: Bool

  private var isConnected: Bool {
    store.snapshot.connectionStatus == .ok
  }

  private var isHosted: Bool {
    store.snapshot.baseURL.scheme?.lowercased() == "https"
  }

  private var canBuild: Bool {
    isConnected && repoReady && !prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !store.buildStartInFlight
  }

  private var repoReady: Bool {
    !store.selectedRepoID.isEmpty && store.repos.contains { $0.id == store.selectedRepoID }
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(alignment: .firstTextBaseline) {
        VStack(alignment: .leading, spacing: 4) {
          Text("Ask Forge to build")
            .font(.title3.weight(.semibold))
          Text(isConnected
            ? "Describe a change in plain language. Forge starts the run, you watch it work, and every step carries a receipt."
            : (isHosted
              ? "Reconnect to MDx Cloud to ask Forge to build or fix something."
              : "Connect local MDx to ask Forge to build or fix something."))
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
        Spacer()
        StatusPill(status: "Forge", tone: .neutral)
      }

      // Premium single-container composer (Codex texture): the input, context
      // chips, and a circular send all live inside one soft rounded surface.
      VStack(alignment: .leading, spacing: 12) {
        TextField(
          isConnected
            ? "Describe a change in plain language"
            : (isHosted ? "Waiting for MDx Cloud" : "Waiting for the local route server"),
          text: $prompt,
          axis: .vertical
        )
        .textFieldStyle(.plain)
        .font(.body)
        .lineLimit(2...6)
        .disabled(!isConnected)
        .focused($promptFocused)
        .onSubmit { if canBuild { startBuild() } }
        // Quiet secondary affordance: Option-Return opens the guided sheet
        // prefilled with the typed intent instead of one-shot starting.
        .onKeyPress(keys: [.return]) { press in
          guard press.modifiers.contains(.option), canBuild else { return .ignored }
          openGuidedSheet()
          return .handled
        }

        HStack(spacing: 6) {
          if isConnected {
            Menu {
              ForEach(store.repos) { repo in
                Button(repo.label) { store.selectedRepoID = repo.id }
              }
              if store.repos.isEmpty {
                switch store.repoLoadPhase {
                case .loading:
                  Text("Loading repos…")
                case .failed, .stale:
                  Button("Retry cloud repos") { Task { await store.loadRepos() } }
                default:
                  Text("No repos connected")
                }
              }
              if !isHosted {
                Divider()
                Button("Connect a repo on this Mac…") { store.connectRepoFromPanel() }
              } else if !store.repos.isEmpty {
                Divider()
                Button("Refresh cloud repos") { Task { await store.loadRepos() } }
              }
            } label: {
              ComposerChip(icon: "shippingbox", text: selectedRepoLabel, menu: true)
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .fixedSize()

            ComposerChip(
              icon: isHosted ? "icloud" : "laptopcomputer",
              text: isHosted ? "MDx Cloud" : "Work locally"
            )
            ComposerChip(icon: "arrow.triangle.branch", text: "Isolated branch")
          }
          Spacer(minLength: 8)
          if isConnected {
            Button {
              openGuidedSheet()
            } label: {
              Image(systemName: "slider.horizontal.3")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(.secondary)
                .frame(width: 30, height: 30)
            }
            .buttonStyle(.plain)
            .disabled(!canBuild)
            .help("More options (Option-Return): pick how many workers and proof before starting")
            .accessibilityLabel("More build options")
          }
          sendControl
        }
      }
      .padding(14)
      .background(
        RoundedRectangle(cornerRadius: 16, style: .continuous)
          .fill(Color(nsColor: .textBackgroundColor))
      )
      .overlay(
        RoundedRectangle(cornerRadius: 16, style: .continuous)
          .stroke(promptFocused ? Color.accentColor.opacity(0.55) : Color.secondary.opacity(0.16), lineWidth: 1)
      )
      .shadow(color: .black.opacity(0.05), radius: 10, y: 3)

      if isConnected && prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
        HStack(spacing: 8) {
          Text("Try")
            .font(.caption)
            .foregroundStyle(.tertiary)
          ForEach(Self.examples, id: \.self) { example in
            Button {
              prompt = example
            } label: {
              Text(example)
                .font(.caption)
                .lineLimit(1)
                .padding(.horizontal, 9)
                .padding(.vertical, 4)
                .background(Color.secondary.opacity(0.10))
                .clipShape(Capsule())
            }
            .buttonStyle(.plain)
          }
        }
      }

      if let outcome = store.runActionResult, outcome.title == "Start run", outcome.isRefusal {
        RunActionBanner(outcome: outcome)
      }
    }
    .padding(18)
    .mdxGlassSurface(interactive: true)
  }

  // Circular send (connected) or a Connect action (offline) - lives inside the
  // composer container, right-aligned, the way modern chat kickoffs read.
  @ViewBuilder
  private var sendControl: some View {
    if isConnected {
      Button {
        startBuild()
      } label: {
        Group {
          if store.buildStartInFlight {
            ProgressView().controlSize(.small).tint(.white)
          } else {
            Image(systemName: "arrow.up")
              .font(.system(size: 13, weight: .bold))
              .foregroundStyle(.white)
          }
        }
        .frame(width: 30, height: 30)
        .background(
          Circle().fill(canBuild
            ? AnyShapeStyle(Color.accentColor.gradient)
            : AnyShapeStyle(Color.secondary.opacity(0.22)))
        )
      }
      .buttonStyle(.plain)
        .accessibilityLabel("Start build")
      .disabled(!canBuild)
      .help("Start build")
    } else {
      Button {
        Task { await store.refresh() }
      } label: {
        Label("Connect", systemImage: "arrow.clockwise")
      }
      .mdxPrimaryButtonStyle()
      .controlSize(.small)
    }
  }

  private var selectedRepoLabel: String {
    store.repos.first { $0.id == store.selectedRepoID }?.label
      ?? store.repos.first?.label
      ?? "Pick a repo"
  }

  private static let examples = [
    "Add a health check endpoint",
    "Write tests for an untested module",
    "Tidy up a noisy log line"
  ]

  // Plain submit starts the run in one shot with the auto recommendation.
  private func startBuild() {
    let intent = prompt
    prompt = ""
    Task { await store.submitComposerBuild(intent: intent) }
  }

  // Secondary affordance: open the guided Start a Build sheet prefilled.
  private func openGuidedSheet() {
    store.pendingBuildIntent = prompt
    store.startBuildFlow()
    prompt = ""
  }
}

struct CalmStateCard: View {
  let title: String
  let detail: String
  let systemImage: String

  var body: some View {
    HStack(alignment: .top, spacing: 12) {
      Image(systemName: systemImage)
        .font(.title3)
        .foregroundStyle(.green)
      VStack(alignment: .leading, spacing: 3) {
        Text(title)
          .font(.subheadline.weight(.medium))
        Text(detail)
          .font(.callout)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
      Spacer(minLength: 0)
    }
    .padding(14)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 10, style: .continuous)
        .fill(Color.green.opacity(0.06))
    )
  }
}

struct SituationCard: View {
  let title: String
  let detail: String
  let systemImage: String
  let status: String
  let tone: StatusTone

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack {
        Image(systemName: systemImage)
          .font(.title3)
          .foregroundStyle(.secondary)
        Spacer()
        StatusPill(status: status, tone: tone)
      }
      Text(title)
        .font(.headline)
      Text(detail)
        .font(.callout)
        .foregroundStyle(.secondary)
        .lineLimit(3)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(14)
    .mdxGlassSurface()
  }
}

struct SurfaceJumpCard: View {
  @Environment(OperatorStore.self) private var store
  let route: AppRoute
  let detail: String

  var body: some View {
    Button {
      store.select(route)
    } label: {
      HStack(alignment: .top, spacing: 12) {
        Image(systemName: route.systemImage)
          .font(.title3)
          .foregroundStyle(.secondary)
          .frame(width: 24)
        VStack(alignment: .leading, spacing: 4) {
          Text(route.title)
            .font(.headline)
          Text(detail)
            .font(.callout)
            .foregroundStyle(.secondary)
            .lineLimit(2)
        }
        Spacer()
        Image(systemName: "chevron.right")
          .foregroundStyle(.tertiary)
      }
      .padding(14)
      .frame(maxWidth: .infinity, alignment: .leading)
    }
    .buttonStyle(.plain)
    .mdxGlassSurface(interactive: true)
  }
}

struct FlagshipHeroCard: View {
  let route: AppRoute

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Image(systemName: route.systemImage)
          .font(.title2)
          .foregroundStyle(Color.accentColor)
        Spacer()
        StatusPill(status: "Native shell", tone: .neutral)
      }
      Text(route.primaryVerb)
        .font(.title3.weight(.semibold))
      Text(heroDetail)
        .font(.callout)
        .foregroundStyle(.secondary)
        .lineLimit(4)
    }
    .padding(16)
    .frame(maxWidth: .infinity, alignment: .leading)
    .mdxGlassSurface(interactive: true)
  }

  private var heroDetail: String {
    switch route {
    case .twin:
      return "Twin starts as the reasoning surface beside evidence. It can help interpret local proof now; delegation waits for governed action rails."
    case .pages:
      return "Pages will turn context into typed, inspectable knowledge. Origins stay visible so generated understanding never outruns proof."
    case .message:
      return "Message is where people and agents coordinate: approvals, activity, and handoffs, with every delivery kept on the record."
    case .marketplace:
      return "Marketplace collects capabilities and providers as audited profiles. Adding power requires records, policy, and authority."
    default:
      return route.subtitle
    }
  }
}

struct SurfacePreviewTile: View {
  let item: SurfacePreviewItem

  var body: some View {
    VStack(alignment: .leading, spacing: 9) {
      HStack {
        Image(systemName: item.systemImage)
          .font(.title3)
          .foregroundStyle(.secondary)
        Spacer()
        StatusPill(status: item.status, tone: item.status == "Held" ? .locked : .neutral)
      }
      Text(item.title)
        .font(.headline)
      Text(item.detail)
        .font(.callout)
        .foregroundStyle(.secondary)
        .lineLimit(3)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(14)
    .mdxGlassSurface()
  }
}

struct MetricTile: View {
  let metric: CockpitMetric

  var body: some View {
    VStack(alignment: .leading, spacing: 5) {
      Text(metric.title)
        .font(.caption.weight(.semibold))
        .foregroundStyle(.secondary)
      Text(metric.value)
        .font(.title2.weight(.semibold))
        .monospacedDigit()
        .contentTransition(.numericText())
        .animation(.spring(duration: 0.3), value: metric.value)
      Text(metric.detail)
        .font(.caption)
        .foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(14)
    .mdxGlassSurface()
  }
}

struct WorkRow: View {
  let item: WorkItem

  var body: some View {
    HStack(alignment: .top, spacing: 12) {
      Image(systemName: "hammer.circle")
        .font(.title3)
        .foregroundStyle(.secondary)
      VStack(alignment: .leading, spacing: 4) {
        Text(item.title)
          .font(.headline)
        Text(item.subtitle)
          .foregroundStyle(.secondary)
          .lineLimit(2)
        HStack {
          StatusPill(status: item.status, tone: .neutral)
          StatusPill(status: item.stage, tone: .neutral)
        }
      }
      Spacer()
    }
    .padding(14)
    .mdxGlassSurface(interactive: true)
  }
}
