import AppKit
import SwiftUI

/// A quiet health indicator in the sidebar. Green when connected and clean;
/// amber on recent hangs/errors; red when the kernel is unreachable. Click to
/// open Diagnostics. This is the subtle in-app surfacing of hangs and errors.
struct HealthPill: View {
  @Environment(OperatorStore.self) private var store
  private var diagnostics = DiagnosticsStore.shared
  @State private var hovering = false

  var body: some View {
    let health = store.appHealth
    Button {
      store.showDiagnosticsPanel()
    } label: {
      HStack(spacing: 7) {
        ZStack {
          Circle().fill(color(health).opacity(0.25)).frame(width: 14, height: 14)
          Circle().fill(color(health)).frame(width: 7, height: 7)
        }
        Text(health.label)
          .font(.caption)
          .foregroundStyle(health == .healthy ? .secondary : color(health))
        if diagnostics.hangCount > 0 || diagnostics.errorCount > 0 {
          Spacer(minLength: 4)
          Text(badge)
            .font(.caption2.monospacedDigit())
            .foregroundStyle(.tertiary)
        } else {
          Spacer(minLength: 0)
        }
        Image(systemName: "chevron.right")
          .font(.caption2)
          .foregroundStyle(.tertiary)
          .opacity(hovering ? 1 : 0)
      }
      .padding(.horizontal, 9)
      .padding(.vertical, 5)
      .background(
        RoundedRectangle(cornerRadius: 7, style: .continuous)
          .fill(hovering ? Color.primary.opacity(0.05) : Color.clear)
      )
    }
    .buttonStyle(.plain)
    .onHover { hovering = $0 }
    .help(health.detail)
  }

  private var badge: String {
    var parts: [String] = []
    if diagnostics.errorCount > 0 { parts.append("\(diagnostics.errorCount)⚠") }
    if diagnostics.hangCount > 0 { parts.append("\(diagnostics.hangCount) hang") }
    return parts.joined(separator: " · ")
  }

  private func color(_ health: AppHealth) -> Color {
    switch health {
    case .healthy: return .green
    case .degraded: return .orange
    case .offline: return .red
    }
  }
}

struct DiagnosticsCard: View {
  @Environment(OperatorStore.self) private var store
  private var diagnostics = DiagnosticsStore.shared

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(spacing: 8) {
        Image(systemName: "waveform.path.ecg")
          .foregroundStyle(Color.accentColor)
        Text("Diagnostics")
          .font(.headline)
        if diagnostics.hangCount > 0 {
          Text("\(diagnostics.hangCount) hang\(diagnostics.hangCount == 1 ? "" : "s")")
            .font(.caption2.weight(.medium))
            .foregroundStyle(.red)
            .padding(.horizontal, 6).padding(.vertical, 1)
            .background(Capsule().fill(Color.red.opacity(0.14)))
        }
        Spacer()
        Button("Details") { store.showDiagnosticsPanel() }
          .font(.caption)
          .buttonStyle(.plain)
          .foregroundStyle(Color.accentColor)
      }

      HStack(spacing: 20) {
        DiagMetric(label: "Launch", value: DiagFormat.ms(diagnostics.launchDurationMs), tint: DiagFormat.tint(diagnostics.launchDurationMs))
        DiagMetric(label: "Uptime", value: diagnostics.uptimeLabel, tint: .secondary)
        DiagMetric(label: "Calls", value: "\(diagnostics.totalCalls)", tint: .primary)
        DiagMetric(label: "Errors", value: "\(diagnostics.errorCount)", tint: diagnostics.errorCount > 0 ? .orange : .green)
        DiagMetric(label: "p95", value: "\(Int(diagnostics.p95LatencyMs))ms", tint: DiagFormat.tint(diagnostics.p95LatencyMs))
        if diagnostics.avgTTFTms > 0 {
          DiagMetric(label: "TTFT", value: "\(Int(diagnostics.avgTTFTms))ms", tint: DiagFormat.tint(diagnostics.avgTTFTms))
        }
      }

      Text(diagnostics.events.isEmpty
           ? "Route timings, stream latency, hangs, and your recent trail collect here. Open Details or copy the report into any bug note."
           : "Signposts stream to Instruments; the last \(diagnostics.events.count) events are kept for a copyable report.")
        .font(.caption)
        .foregroundStyle(.tertiary)
        .fixedSize(horizontal: false, vertical: true)
    }
    .padding(16)
    .mdxGlassSurface(interactive: false)
  }
}

struct DiagMetric: View {
  let label: String
  let value: String
  var tint: Color = .primary

  var body: some View {
    VStack(alignment: .leading, spacing: 2) {
      Text(value).font(.title3.weight(.semibold).monospacedDigit()).foregroundStyle(tint)
      Text(label.uppercased()).font(.caption2.weight(.semibold)).foregroundStyle(.tertiary)
    }
  }
}

enum DiagFormat {
  static func ms(_ value: Double?) -> String {
    guard let value else { return "—" }
    return "\(Int(value))ms"
  }

  static func tint(_ ms: Double?) -> Color {
    guard let ms, ms > 0 else { return .secondary }
    if ms < 300 { return .green }
    if ms < 800 { return .orange }
    return .red
  }
}

// MARK: - Full diagnostics sheet

struct DiagnosticsDetailView: View {
  @Environment(OperatorStore.self) private var store
  private var diagnostics = DiagnosticsStore.shared
  @State private var filter: DiagnosticEvent.Kind?
  @State private var copied = false

  private let kinds: [DiagnosticEvent.Kind] = [.route, .action, .stream, .hang, .breadcrumb]

  var body: some View {
    VStack(spacing: 0) {
      HStack {
        Text("Diagnostics")
          .font(.title3.weight(.semibold))
        Spacer()
        Button {
          NSPasteboard.general.clearContents()
          NSPasteboard.general.setString(diagnostics.exportText(), forType: .string)
          copied = true
        } label: {
          Label(copied ? "Copied" : "Copy report", systemImage: copied ? "checkmark" : "doc.on.doc")
        }
        .controlSize(.small)
        Button("Clear") { diagnostics.clear() }
          .controlSize(.small)
        Button("Done") { store.showDiagnostics = false }
          .controlSize(.small)
          .keyboardShortcut(.cancelAction)
      }
      .padding(16)
      Divider()

      summary
      if let last = diagnostics.lastSession {
        Divider()
        HStack(spacing: 8) {
          Image(systemName: "clock.arrow.circlepath").font(.caption).foregroundStyle(.tertiary)
          Text("Last session")
            .font(.caption.weight(.medium))
            .foregroundStyle(.secondary)
          Text("launch \(DiagFormat.ms(last.launchMs)) · \(last.calls) calls · \(last.errors) errors · \(last.hangs) hangs · p95 \(Int(last.p95Ms))ms")
            .font(.caption.monospaced())
            .foregroundStyle(.tertiary)
          Spacer()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
      }
      Divider()
      AcrossSessionsSection()
      Divider()

      HStack(spacing: 6) {
        filterChip("All", nil)
        ForEach(kinds, id: \.self) { kind in
          filterChip(kind.rawValue.capitalized, kind)
        }
        Spacer()
      }
      .padding(.horizontal, 16)
      .padding(.vertical, 10)

      ScrollView {
        LazyVStack(alignment: .leading, spacing: 0) {
          let rows = diagnostics.events(of: filter)
          if rows.isEmpty {
            Text("No events of this kind yet.")
              .font(.callout)
              .foregroundStyle(.tertiary)
              .padding(24)
          }
          ForEach(rows) { event in
            DiagnosticRow(event: event)
            Divider().padding(.leading, 16)
          }
        }
      }
    }
    .frame(width: 640, height: 620)
    .onChange(of: diagnostics.events.count) { _, _ in copied = false }
    .task { await store.loadAppHealthProjection() }
  }

  private var summary: some View {
    HStack(spacing: 26) {
      DiagMetric(label: "Launch", value: DiagFormat.ms(diagnostics.launchDurationMs), tint: DiagFormat.tint(diagnostics.launchDurationMs))
      DiagMetric(label: "Uptime", value: diagnostics.uptimeLabel, tint: .secondary)
      DiagMetric(label: "Calls", value: "\(diagnostics.totalCalls)", tint: .primary)
      DiagMetric(label: "Errors", value: "\(diagnostics.errorCount)", tint: diagnostics.errorCount > 0 ? .orange : .green)
      DiagMetric(label: "Hangs", value: "\(diagnostics.hangCount)", tint: diagnostics.hangCount > 0 ? .red : .green)
      DiagMetric(label: "Avg", value: "\(Int(diagnostics.avgLatencyMs))ms", tint: DiagFormat.tint(diagnostics.avgLatencyMs))
      DiagMetric(label: "p95", value: "\(Int(diagnostics.p95LatencyMs))ms", tint: DiagFormat.tint(diagnostics.p95LatencyMs))
      if diagnostics.avgTTFTms > 0 {
        DiagMetric(label: "TTFT", value: "\(Int(diagnostics.avgTTFTms))ms", tint: DiagFormat.tint(diagnostics.avgTTFTms))
      }
      Spacer()
    }
    .padding(16)
  }

  private func filterChip(_ label: String, _ kind: DiagnosticEvent.Kind?) -> some View {
    let selected = filter == kind
    return Button {
      filter = kind
    } label: {
      Text(label)
        .font(.caption.weight(.medium))
        .foregroundStyle(selected ? Color.white : .secondary)
        .padding(.horizontal, 10)
        .padding(.vertical, 4)
        .background(Capsule().fill(selected ? Color.accentColor : Color.primary.opacity(0.06)))
    }
    .buttonStyle(.plain)
  }
}

struct DiagnosticRow: View {
  let event: DiagnosticEvent

  var body: some View {
    HStack(alignment: .firstTextBaseline, spacing: 10) {
      Circle()
        .fill(event.ok ? Color.green : Color.orange)
        .frame(width: 6, height: 6)
        .padding(.top, 5)
      Text(event.kind.rawValue)
        .font(.caption2.weight(.semibold))
        .foregroundStyle(.tertiary)
        .frame(width: 66, alignment: .leading)
      VStack(alignment: .leading, spacing: 1) {
        Text(event.label)
          .font(.caption.monospaced())
          .foregroundStyle(.primary)
          .lineLimit(1)
          .truncationMode(.middle)
        if !event.detail.isEmpty {
          Text(event.detail)
            .font(.caption2)
            .foregroundStyle(.tertiary)
            .lineLimit(1)
        }
      }
      Spacer(minLength: 8)
      if event.kind == .stream, event.ttftMs > 0 {
        Text("ttft \(Int(event.ttftMs))ms")
          .font(.caption2.monospaced())
          .foregroundStyle(.secondary)
      }
      if event.durationMs > 0 {
        Text(event.latencyLabel)
          .font(.caption.monospaced())
          .foregroundStyle(DiagFormat.tint(event.durationMs))
      }
    }
    .padding(.horizontal, 16)
    .padding(.vertical, 7)
  }
}

// MARK: - Across sessions (central app health)

/// The founder's "continuously see where users have issues" read, inside
/// Diagnostics. It shows where trouble recurs ACROSS sessions and installs: the
/// top failing routes, how often the app relaunched after a crash, hang
/// hotspots, and how many sessions ran degraded. Read-only; the kernel owns the
/// truth, and honest empty/offline states never fake readiness.
struct AcrossSessionsSection: View {
  @Environment(OperatorStore.self) private var store

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack(spacing: 8) {
        Image(systemName: "person.2.wave.2")
          .font(.caption)
          .foregroundStyle(Color.accentColor)
        Text("Where people hit trouble")
          .font(.subheadline.weight(.semibold))
        Spacer()
        Text("across sessions")
          .font(.caption2)
          .foregroundStyle(.tertiary)
      }
      content
    }
    .padding(.horizontal, 16)
    .padding(.vertical, 12)
  }

  @ViewBuilder private var content: some View {
    switch store.appHealthProjectionPhase {
    case .idle, .loading:
      HStack(spacing: 8) {
        ProgressView().controlSize(.small)
        Text("Gathering reports from recent sessions...")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
      .padding(.vertical, 4)
    case .failed(let message):
      ContentUnavailableView {
        Label("Can't reach the shared health record", systemImage: "bolt.horizontal.circle")
      } description: {
        Text(message)
      }
      .frame(height: 120)
    case .empty:
      Text("No trouble reported yet. As sessions run, the routes and crashes people actually hit will collect here so you can see them without asking.")
        .font(.caption)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
    case .loaded, .stale:
      loaded(store.appHealthProjection ?? .empty)
    }
  }

  private func loaded(_ projection: AppHealthProjection) -> some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack(spacing: 22) {
        DiagMetric(label: "Sessions", value: "\(projection.distinctSessions)", tint: .primary)
        DiagMetric(
          label: "Degraded",
          value: "\(projection.degradedSessionCount)",
          tint: projection.degradedSessionCount > 0 ? .orange : .green
        )
        DiagMetric(
          label: "Crash relaunch",
          value: "\(projection.uncleanRelaunchRatePct)%",
          tint: projection.uncleanRelaunchRatePct > 0 ? .red : .green
        )
        DiagMetric(
          label: "Hang sessions",
          value: "\(projection.hangHotspotSessionCount)",
          tint: projection.hangHotspotSessionCount > 0 ? .orange : .green
        )
        Spacer()
      }

      if projection.topFailingRoutes.isEmpty {
        Text("No routes are failing across sessions right now.")
          .font(.caption)
          .foregroundStyle(.secondary)
      } else {
        VStack(alignment: .leading, spacing: 5) {
          ForEach(projection.topFailingRoutes.prefix(4)) { route in
            HotspotRow(route: route)
          }
        }
      }
    }
  }
}

/// One recurring failing route, most-affected first: the path plus how many
/// distinct sessions hit it, the total failures, and the last status seen.
struct HotspotRow: View {
  let route: AppHealthRouteHotspot

  var body: some View {
    HStack(alignment: .firstTextBaseline, spacing: 10) {
      Text("\(route.lastStatus)")
        .font(.caption2.weight(.semibold).monospacedDigit())
        .foregroundStyle(.white)
        .padding(.horizontal, 6)
        .padding(.vertical, 1)
        .background(Capsule().fill(statusColor))
      Text(route.route)
        .font(.caption.monospaced())
        .foregroundStyle(.primary)
        .lineLimit(1)
        .truncationMode(.middle)
      Spacer(minLength: 8)
      Text(sessionsLabel)
        .font(.caption2)
        .foregroundStyle(.secondary)
    }
  }

  private var sessionsLabel: String {
    let sessions = route.distinctSessionCount
    let sessionText = "\(sessions) session\(sessions == 1 ? "" : "s")"
    return "\(sessionText) · \(route.totalFailureCount) fail\(route.totalFailureCount == 1 ? "" : "s")"
  }

  private var statusColor: Color {
    switch route.lastStatus {
    case 500...: return .red
    case 400..<500: return .orange
    case 0: return .secondary
    default: return .blue
    }
  }
}

struct IdentityCard: View {
  let session: IdentitySession
  var displayName: String? = nil
  var workspaceName: String? = nil

  private var resolvedName: String {
    let candidate = displayName?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    return candidate.isEmpty ? session.displayName : candidate
  }

  private var resolvedWorkspace: String {
    let candidate = workspaceName?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    return candidate.isEmpty ? Page.humanActor(session.tenantID) : candidate
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 14) {
      HStack(spacing: 14) {
        ZStack {
          Circle()
            .fill(Color.accentColor.opacity(0.15))
            .frame(width: 46, height: 46)
          Text(initials)
            .font(.title3.weight(.semibold))
            .foregroundStyle(Color.accentColor)
        }
        VStack(alignment: .leading, spacing: 3) {
          Text(resolvedName)
            .font(.title3.weight(.semibold))
          HStack(spacing: 8) {
            RoleBadge(role: session.role)
            Text(resolvedWorkspace)
              .font(.caption)
              .foregroundStyle(.secondary)
          }
        }
        Spacer()
      }

      Divider()

      VStack(alignment: .leading, spacing: 8) {
        youRow("Your role", session.roleLabel)
        if !session.admissionScope.isEmpty {
          youRow("What you can start", Clearance.label(session.admissionScope))
        }
        if !session.expiresAt.isEmpty {
          youRow("Session valid until", friendlyDate(session.expiresAt))
        }
      }
    }
    .padding(16)
    .mdxGlassSurface(interactive: false)
  }

  private var initials: String {
    let parts = resolvedName.split(separator: " ")
    let letters = parts.prefix(2).compactMap { $0.first }
    return String(letters).uppercased()
  }

  private func youRow(_ label: String, _ value: String) -> some View {
    HStack(alignment: .top, spacing: 12) {
      Text(label)
        .font(.caption)
        .foregroundStyle(.tertiary)
        .frame(width: 150, alignment: .leading)
      Text(value)
        .font(.caption)
        .foregroundStyle(.secondary)
        .textSelection(.enabled)
      Spacer(minLength: 0)
    }
  }

  private static let mediumDate: DateFormatter = {
    let out = DateFormatter()
    out.dateStyle = .medium
    return out
  }()

  private func friendlyDate(_ iso: String) -> String {
    guard let date = RelativeTime.date(from: iso) else { return iso }
    return Self.mediumDate.string(from: date)
  }
}

struct RoleBadge: View {
  let role: String

  private var color: Color {
    switch role {
    case "owner": return .purple
    case "operator": return .blue
    case "auditor": return .teal
    default: return .secondary
    }
  }

  var body: some View {
    // Folds into the app-wide StatusPill capsule, keeping account-role hues.
    StatusPill(status: role.capitalized, color: color)
  }
}

struct ClearanceCard: View {
  let clearance: Clearance
  @State private var showHeld = false

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(spacing: 8) {
        Image(systemName: clearance.isFailClosed ? "lock.shield.fill" : "lock.open")
          .foregroundStyle(clearance.isFailClosed ? Color.accentColor : .orange)
        Text("Your access")
          .font(.headline)
        Spacer()
        if clearance.isFailClosed {
          Text("Fail-closed")
            .font(.caption.weight(.medium))
            .foregroundStyle(.green)
            .padding(.horizontal, 8)
            .padding(.vertical, 2)
            .background(Capsule().fill(Color.green.opacity(0.14)))
        }
      }

      Text("You can navigate, read every receipt, and ask Twin freely. Actions that change the world stay held until their governed path opens.")
        .font(.callout)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)

      if !clearance.safeControls.isEmpty {
        FlowChips(
          title: "Open to you",
          items: clearance.safeControls.map { Clearance.label($0) },
          icon: "checkmark.circle.fill",
          tint: .green
        )
      }

      DisclosureGroup(isExpanded: $showHeld) {
        FlowChips(
          title: "",
          items: (clearance.blockedControls + clearance.blockedAuthorities).map { Clearance.label($0) },
          icon: "lock.fill",
          tint: .secondary
        )
        .padding(.top, 6)
      } label: {
        Text("What's held until a governed path opens (\(clearance.blockedControls.count + clearance.blockedAuthorities.count))")
          .font(.caption.weight(.medium))
          .foregroundStyle(.secondary)
      }
    }
    .padding(16)
    .mdxGlassSurface(interactive: false)
  }
}

struct FlowChips: View {
  let title: String
  let items: [String]
  let icon: String
  var tint: Color = .secondary

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      if !title.isEmpty {
        Text(title.uppercased())
          .font(.caption2.weight(.semibold))
          .foregroundStyle(.tertiary)
      }
      FlexibleWrap(items: items) { item in
        HStack(spacing: 4) {
          Image(systemName: icon).font(.system(size: 8)).foregroundStyle(tint)
          Text(item).font(.caption)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 3)
        .background(Capsule().fill(Color.primary.opacity(0.05)))
      }
    }
  }
}

/// Minimal wrapping layout so chips flow onto multiple lines.
struct FlexibleWrap<Item: Hashable, Content: View>: View {
  let items: [Item]
  let content: (Item) -> Content

  var body: some View {
    Wrap(spacing: 6, lineSpacing: 6) {
      ForEach(items, id: \.self) { item in
        content(item)
      }
    }
  }
}

/// A simple flow layout (macOS 13+ Layout API).
struct Wrap: Layout {
  var spacing: CGFloat = 6
  var lineSpacing: CGFloat = 6

  func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
    let maxWidth = proposal.width ?? .infinity
    var x: CGFloat = 0
    var y: CGFloat = 0
    var lineHeight: CGFloat = 0
    for subview in subviews {
      let size = subview.sizeThatFits(.unspecified)
      if x + size.width > maxWidth, x > 0 {
        x = 0
        y += lineHeight + lineSpacing
        lineHeight = 0
      }
      x += size.width + spacing
      lineHeight = max(lineHeight, size.height)
    }
    return CGSize(width: maxWidth == .infinity ? x : maxWidth, height: y + lineHeight)
  }

  func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
    var x = bounds.minX
    var y = bounds.minY
    var lineHeight: CGFloat = 0
    for subview in subviews {
      let size = subview.sizeThatFits(.unspecified)
      if x + size.width > bounds.maxX, x > bounds.minX {
        x = bounds.minX
        y += lineHeight + lineSpacing
        lineHeight = 0
      }
      subview.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
      x += size.width + spacing
      lineHeight = max(lineHeight, size.height)
    }
  }
}
