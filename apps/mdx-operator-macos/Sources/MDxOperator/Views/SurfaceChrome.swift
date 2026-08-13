import SwiftUI

/// Shared layout constants so every surface (Twin, Pages, Message, Marketplace)
/// shares the same rail width and content measure. Changing one value here
/// keeps the whole shell visually consistent.
enum SurfaceMetrics {
  /// Sidebar / list rail width, identical across surfaces.
  static let railWidth: CGFloat = 280
  /// Reading measure for document bodies and detail panes.
  static let readingWidth: CGFloat = 860
  /// Wider measure for transcripts and streaming feeds.
  static let streamWidth: CGFloat = 820
}

/// The one search pill used in every rail.
struct SurfaceSearchField: View {
  let placeholder: String
  @Binding var text: String
  var onChange: ((String) -> Void)? = nil

  var body: some View {
    HStack(spacing: 6) {
      Image(systemName: "magnifyingglass")
        .font(.caption)
        .foregroundStyle(.tertiary)
      TextField(placeholder, text: $text)
        .textFieldStyle(.plain)
        .font(.caption)
        .onChange(of: text) { _, value in onChange?(value) }
      if !text.isEmpty {
        Button {
          text = ""
          onChange?("")
        } label: {
          Image(systemName: "xmark.circle.fill").font(.caption)
        }
        .buttonStyle(.plain)
        .foregroundStyle(.tertiary)
      }
    }
    .padding(.horizontal, 10)
    .padding(.vertical, 6)
    .background(
      RoundedRectangle(cornerRadius: 7, style: .continuous)
        .fill(Color.primary.opacity(0.05))
    )
  }
}

/// The shared top bar for every surface: sidebar toggle, tinted glyph, title,
/// quiet subtitle, and an optional trailing accessory.
struct SurfaceTopBar<Trailing: View>: View {
  let icon: String
  let title: String
  let subtitle: String
  @Binding var showRail: Bool
  @ViewBuilder var trailing: () -> Trailing

  var body: some View {
    HStack(spacing: 10) {
      Button {
        withAnimation(.easeInOut(duration: 0.18)) { showRail.toggle() }
      } label: {
        Image(systemName: "sidebar.left")
      }
      .buttonStyle(.plain)
      .help(showRail ? "Hide the list" : "Show the list")
      .accessibilityLabel(showRail ? "Hide the list" : "Show the list")
      .foregroundStyle(.secondary)

      Image(systemName: icon)
        .font(.callout)
        .foregroundStyle(Color.accentColor)
      Text(title)
        .font(.headline)
      Text(subtitle)
        .font(.caption)
        .foregroundStyle(.secondary)
      Spacer()
      trailing()
    }
    .padding(.horizontal, 20)
    .padding(.vertical, 12)
  }
}

extension SurfaceTopBar where Trailing == EmptyView {
  init(icon: String, title: String, subtitle: String, showRail: Binding<Bool>) {
    self.init(icon: icon, title: title, subtitle: subtitle, showRail: showRail) { EmptyView() }
  }
}


/// Spacing tokens: the app's implicit scale, named. New code should reach for
/// these instead of magic paddings; existing code converges as it is touched.
enum Spacing {
  static let xs: CGFloat = 4
  static let s: CGFloat = 8
  static let m: CGFloat = 12
  static let l: CGFloat = 16
  static let xl: CGFloat = 20
}

/// The app's semantic type scale, named so hierarchy stays deliberate rather
/// than defaulting everything to caption. Primary reading content is body
/// weight; caption is reserved for genuinely secondary metadata (receipts,
/// timestamps, counts, badges). Reach for these instead of ad hoc sizes;
/// existing code converges as it is touched.
enum SurfaceType {
  /// Reader and empty-state headlines.
  static let title = Font.title2.weight(.semibold)
  /// Card and section headings.
  static let heading = Font.headline
  /// Primary reading content: answers, page bodies, run summaries.
  static let body = Font.body
  /// Supporting copy one step below primary reading content.
  static let callout = Font.callout
  /// List-row titles and field labels.
  static let label = Font.subheadline.weight(.medium)
  /// Secondary metadata: receipts, timestamps, counts, badges.
  static let caption = Font.caption
  /// The large glyph in a full-pane empty or placeholder state. Replaces the
  /// scattered `.font(.system(size: 30))` magic numbers.
  static let emptyGlyph = Font.system(size: 34, weight: .regular)
}

/// A calm reading-pane skeleton for a detail or reader load still in flight.
/// Shapes, not a bare spinner, so content arriving causes no layout jump.
struct ReaderSkeleton: View {
  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      RoundedRectangle(cornerRadius: 6).fill(Color.primary.opacity(0.08))
        .frame(width: 260, height: 24)
      HStack(spacing: 8) {
        Capsule().fill(Color.primary.opacity(0.06)).frame(width: 72, height: 14)
        Capsule().fill(Color.primary.opacity(0.06)).frame(width: 96, height: 14)
      }
      Divider().padding(.vertical, 4)
      ForEach(0..<6, id: \.self) { index in
        RoundedRectangle(cornerRadius: 4)
          .fill(Color.primary.opacity(0.05))
          .frame(width: index.isMultiple(of: 3) ? 320 : 460, height: 11)
      }
    }
    .padding(28)
    .frame(maxWidth: SurfaceMetrics.readingWidth, alignment: .leading)
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    .accessibilityElement()
    .accessibilityLabel("Loading")
  }
}

/// The honest state chooser for a detail or reader pane that has no content to
/// show. It never confuses the three cases the audit flagged: a still-loading
/// first fetch (skeleton), an offline or unreachable kernel (connect prompt),
/// and a genuinely empty result (guidance plus the safe next move). When items
/// exist but none is selected, it shows a quiet "pick one" prompt. Built on
/// `ContentUnavailableView` (macOS 14+) so every surface reads the same way.
struct SurfaceDetailPlaceholder: View {
  let connected: Bool
  let phase: LoadPhase
  /// True when the surface's collection has nothing to select.
  let isEmpty: Bool
  let icon: String
  /// Shown when items exist but none is selected.
  let selectPrompt: String
  /// Shown when the collection is genuinely empty and connected.
  let emptyTitle: String
  let emptyMessage: String
  var actionLabel: String? = nil
  var action: (() -> Void)? = nil
  var retry: (() -> Void)? = nil

  var body: some View {
    content.frame(maxWidth: .infinity, maxHeight: .infinity)
  }

  @ViewBuilder private var content: some View {
    if !connected {
      ContentUnavailableView {
        Label("Not connected", systemImage: "bolt.horizontal.circle")
      } description: {
        Text("Start local MDx to see live work. This fills in the moment the connection is up.")
      } actions: {
        if let retry {
          Button("Connect", action: retry).buttonStyle(.bordered)
        }
      }
    } else if isEmpty && isLoading {
      ReaderSkeleton()
    } else if isEmpty, let message = failureMessage {
      ContentUnavailableView {
        Label("Could not load", systemImage: "exclamationmark.triangle")
      } description: {
        Text(message)
      } actions: {
        if let retry {
          Button("Try again", action: retry).buttonStyle(.bordered)
        }
      }
    } else if isEmpty {
      ContentUnavailableView {
        Label(emptyTitle, systemImage: icon)
      } description: {
        Text(emptyMessage)
      } actions: {
        if let action, let actionLabel {
          Button(actionLabel, action: action).buttonStyle(.borderedProminent)
        }
      }
    } else {
      ContentUnavailableView {
        Label(selectPrompt, systemImage: icon)
      } description: {
        EmptyView()
      }
    }
  }

  private var isLoading: Bool {
    switch phase {
    case .idle, .loading: return true
    default: return false
    }
  }

  private var failureMessage: String? {
    switch phase {
    case .failed(let message), .stale(let message): return message
    default: return nil
    }
  }
}

/// The shared rail header: title, optional live count, optional trailing
/// accessory. Four surfaces hand-rolled near-copies of this and had already
/// drifted; one component freezes the anatomy.
struct RailHeader<Trailing: View>: View {
  let title: String
  var count: Int? = nil
  @ViewBuilder var trailing: () -> Trailing

  var body: some View {
    HStack {
      Text(title)
        .font(.caption.weight(.semibold))
        .foregroundStyle(.secondary)
      Spacer()
      if let count {
        Text("\(count)")
          .font(.caption)
          .monospacedDigit()
          .contentTransition(.numericText())
          .animation(.spring(duration: 0.3), value: count)
          .foregroundStyle(.tertiary)
      }
      trailing()
    }
    .padding(.horizontal, Spacing.m)
    .padding(.vertical, 11)
  }
}

extension RailHeader where Trailing == EmptyView {
  init(title: String, count: Int? = nil) {
    self.init(title: title, count: count) { EmptyView() }
  }
}

/// Placeholder rows shown while a rail's first load is in flight: calmer than
/// a spinner, honest about what is coming (never fake content).
struct RailSkeleton: View {
  var rows: Int = 3

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      ForEach(0..<rows, id: \.self) { _ in
        VStack(alignment: .leading, spacing: 5) {
          RoundedRectangle(cornerRadius: 3)
            .fill(Color.primary.opacity(0.07))
            .frame(width: 150, height: 10)
          RoundedRectangle(cornerRadius: 3)
            .fill(Color.primary.opacity(0.045))
            .frame(width: 210, height: 8)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 4)
      }
    }
    .padding(.top, 6)
    .accessibilityLabel("Loading")
  }
}
