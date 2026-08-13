import SwiftUI

/// The app-managed connection lifecycle, shown at the top of the workspace when
/// MDx is unreachable or running degraded. It replaces the passive health pill's
/// silence with real controls: Retry (force a reconnect), Diagnose (the actual
/// failure, honestly), and Start MDx (how to bring the local engine up, and
/// where to point if it lives elsewhere).
///
/// Truth first: Diagnose shows the real base URL, failed-check streak, last
/// error, and recent hang, never a cosmetic "all good".
struct AppLifecycleBanner: View {
  @Environment(OperatorStore.self) private var store
  private var diagnostics = DiagnosticsStore.shared

  @State private var showDiagnose = false
  @State private var showStart = false
  @State private var retrying = false
  @State private var didApplyShotState = false

  /// Only present when the connection is not healthy. A screenshot hook can
  /// force it visible for deterministic capture without faking the state below.
  static var forcedVisibleForShot: Bool {
    ProcessInfo.processInfo.environment["MDX_SHOT_OFFLINE"] == "1"
  }

  private var health: AppHealth { store.appHealth }
  private var visible: Bool { health != .healthy || Self.forcedVisibleForShot }

  var body: some View {
    Group {
      if visible {
        card
      }
    }
    .onAppear(perform: applyShotStateOnce)
  }

  private var card: some View {
    VStack(alignment: .leading, spacing: 14) {
      HStack(alignment: .top, spacing: 12) {
        ZStack {
          Circle().fill(tint.opacity(0.22)).frame(width: 22, height: 22)
          Circle().fill(tint).frame(width: 10, height: 10)
        }
        .padding(.top, 1)
        .accessibilityHidden(true)

        VStack(alignment: .leading, spacing: 4) {
          Text(title)
            .font(.headline)
          Text(store.connectionDiagnosis)
            .font(.callout)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
        }
        Spacer(minLength: 0)
      }

      controlRow

      if showDiagnose { diagnosePanel }
      if showStart { startPanel }
    }
    .padding(16)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 14, style: .continuous)
        .fill(tint.opacity(0.06))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 14, style: .continuous)
        .stroke(tint.opacity(0.28), lineWidth: 1)
    )
    .accessibilityElement(children: .contain)
  }

  // MARK: - Controls

  private var controlRow: some View {
    HStack(spacing: 10) {
      Button {
        Task {
          retrying = true
          await store.retryConnection()
          retrying = false
        }
      } label: {
        Label(retrying ? "Retrying" : "Retry", systemImage: "arrow.clockwise")
          .padding(.horizontal, 6)
          .padding(.vertical, 1)
      }
      .mdxPrimaryButtonStyle()
      .disabled(retrying)
      .help("Reconnect to the local MDx engine now")

      Button {
        showDiagnose.toggle()
        if showDiagnose { showStart = false }
      } label: {
        Label("Diagnose", systemImage: "stethoscope")
      }
      .buttonStyle(.bordered)
      .help("Show the actual connection failure")

      if !isHosted {
        Button {
          showStart.toggle()
          if showStart { showDiagnose = false }
        } label: {
          Label("Start MDx", systemImage: "bolt.horizontal.circle")
        }
        .buttonStyle(.bordered)
        .help("How to bring the local engine up, or point at another address")
      }

      Spacer(minLength: 0)
    }
  }

  // MARK: - Diagnose

  private var diagnosePanel: some View {
    VStack(alignment: .leading, spacing: 8) {
      DiagnoseRow(label: "Address", value: store.connectionBaseURL)
      DiagnoseRow(
        label: "Failed in a row",
        value: "\(diagnostics.consecutiveFailures)"
      )
      DiagnoseRow(
        label: "Recent hang",
        value: diagnostics.recentHang ? "Yes, in the last 2 minutes" : "None"
      )
      DiagnoseRow(
        label: "Last error",
        value: store.lastConnectionError ?? "None recorded"
      )
      Button {
        store.showDiagnosticsPanel()
      } label: {
        Label("Open Diagnostics", systemImage: "waveform.path.ecg")
          .font(.callout)
      }
      .buttonStyle(.plain)
      .foregroundStyle(Color.accentColor)
      .padding(.top, 2)
    }
    .padding(12)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 10, style: .continuous)
        .fill(Color.primary.opacity(0.04))
    )
    .transition(.opacity)
  }

  // MARK: - Start MDx

  private var startPanel: some View {
    VStack(alignment: .leading, spacing: 12) {
      Text("Start the local engine from the MDx repo, then Retry:")
        .font(.callout)
        .foregroundStyle(.secondary)
      HStack(spacing: 8) {
        Text("sh scripts/dogfood-stack.sh")
          .font(.body.monospaced())
          .textSelection(.enabled)
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

      // Honest boundary: the app connects to a running engine; it does not spawn
      // one. Launching a kernel process is a separate decision, flagged for the
      // orchestrator, not assumed here.
      Text("MDx runs as its own local engine. The app connects to it; it does not launch it for you.")
        .font(.caption)
        .foregroundStyle(.tertiary)
        .fixedSize(horizontal: false, vertical: true)

      DisclosureGroup("Point at a different address") {
        LifecycleBaseURLField()
          .padding(.top, 8)
      }
      .font(.callout)
      .tint(Color.accentColor)
    }
    .padding(12)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 10, style: .continuous)
        .fill(Color.primary.opacity(0.04))
    )
    .transition(.opacity)
  }

  // MARK: - Copy

  private var title: String {
    switch health {
    case .offline: return isHosted ? "Can't reach MDx Cloud" : "MDx is offline"
    case .degraded: return "Some MDx features need attention"
    case .healthy: return "MDx is connected"
    }
  }

  private var isHosted: Bool {
    store.connectionBaseURL.lowercased().hasPrefix("https://")
  }

  private var tint: Color {
    switch health {
    case .healthy: return .green
    case .degraded: return .orange
    case .offline: return .red
    }
  }

  private func applyShotStateOnce() {
    guard !didApplyShotState else { return }
    didApplyShotState = true
    let env = ProcessInfo.processInfo.environment
    if env["MDX_SHOT_DIAGNOSE"] == "1" { showDiagnose = true }
    if env["MDX_SHOT_START"] == "1" { showStart = true }
  }
}

private struct DiagnoseRow: View {
  let label: String
  let value: String

  var body: some View {
    HStack(alignment: .top, spacing: 10) {
      Text(label)
        .font(.caption.weight(.semibold))
        .foregroundStyle(.secondary)
        .frame(width: 110, alignment: .leading)
      Text(value)
        .font(.caption.monospaced())
        .textSelection(.enabled)
        .fixedSize(horizontal: false, vertical: true)
      Spacer(minLength: 0)
    }
  }
}

/// A compact base-URL editor for the Start panel: point the app at an engine on
/// another host or port without opening Settings first.
private struct LifecycleBaseURLField: View {
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
        Task { await store.retryConnection() }
      }
      .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
    }
    .onAppear { text = store.snapshot.baseURL.absoluteString }
  }
}
