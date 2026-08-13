import AppKit
import SwiftUI

// MARK: - Shared pieces

struct RunActionBanner: View {
  let outcome: RunActionOutcome

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      HStack {
        Text(outcome.title)
          .font(.subheadline.weight(.semibold))
        Spacer()
        StatusPill(status: outcome.displayStatus, tone: outcome.isRefusal ? .locked : .positive)
      }
      if !outcome.detail.isEmpty {
        Text(outcome.detail)
          .font(.callout)
          .foregroundStyle(.secondary)
          .fixedSize(horizontal: false, vertical: true)
      }
      if !outcome.receiptID.isEmpty || !outcome.status.isEmpty {
        Text([outcome.status, outcome.receiptID].filter { !$0.isEmpty }.joined(separator: "  "))
          .font(.caption.monospaced())
          .foregroundStyle(.tertiary)
          .textSelection(.enabled)
          .lineLimit(1)
      }
    }
    .padding(12)
    .frame(maxWidth: .infinity, alignment: .leading)
    .mdxGlassSurface()
  }
}

struct RunStat: View {
  let label: String
  let value: String

  var body: some View {
    VStack(alignment: .leading, spacing: 2) {
      Text(value)
        .font(.callout.weight(.semibold))
      Text(label)
        .font(.caption2)
        .foregroundStyle(.secondary)
    }
  }
}

struct RunStatusDot: View {
  let status: String

  var body: some View {
    Circle()
      .fill(color)
      .frame(width: 9, height: 9)
      .accessibilityLabel(status)
  }

  private var color: Color {
    switch forgeRunTone(status) {
    case .positive: return .green
    case .locked: return .orange
    case .neutral: return status.localizedCaseInsensitiveContains("running") ? .accentColor : .secondary
    }
  }
}

struct MetaChip: View {
  let text: String

  var body: some View {
    Text(text)
      .font(.caption2)
      .foregroundStyle(.secondary)
      .lineLimit(1)
      .padding(.horizontal, 7)
      .padding(.vertical, 2)
      .background(Color.secondary.opacity(0.10))
      .clipShape(Capsule())
  }
}

struct LiveDot: View {
  // Static, solid dot. It used to blink with a repeatForever opacity animation,
  // and up to three Live dots render at once on a live run (back bar, action line,
  // context card). Each repeatForever timeline independently drives the whole
  // run-detail ViewGraph to re-render at the display refresh rate; together with
  // the stage pulse and the thinking ellipsis that continuous full-tree re-render
  // saturated the main thread (a sample showed ~100% of main in SwiftUI
  // renderDisplayList). A solid green "Live" reads as live without a per-frame
  // animation; the AppKit-backed WorkingSpinner on the current-action line carries
  // the one moving cue. Verified: with these SwiftUI timelines removed a full
  // streaming run logs zero HangMonitor hangs.
  var body: some View {
    HStack(spacing: 6) {
      Circle()
        .fill(Color.green)
        .frame(width: 8, height: 8)
      Text("Live")
        .font(.caption.weight(.medium))
        .foregroundStyle(.secondary)
    }
  }
}

func forgeRunTone(_ status: String) -> StatusTone {
  let s = status.lowercased()
  if s.contains("done") || s.contains("finished") || s.contains("complete") || s.contains("ready") { return .positive }
  if s.contains("error") || s.contains("stopped") || s.contains("cannot") || s.contains("exhausted") || s.contains("fail") { return .locked }
  return .neutral
}


/// A quiet keycap glyph that teaches the single-key verb by being there.
struct Keycap: View {
  let key: String
  var onAccent = false

  init(_ key: String, onAccent: Bool = false) {
    self.key = key
    self.onAccent = onAccent
  }

  var body: some View {
    Text(key)
      .font(.caption2.weight(.semibold).monospaced())
      .foregroundStyle(onAccent ? Color.white.opacity(0.85) : Color.secondary)
      .padding(.horizontal, 4)
      .padding(.vertical, 1)
      .background(
        RoundedRectangle(cornerRadius: 3, style: .continuous)
          .fill(onAccent ? Color.white.opacity(0.18) : Color.secondary.opacity(0.12))
      )
      .accessibilityHidden(true)
  }
}
