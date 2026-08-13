import AppKit
import SwiftUI

extension String {
  /// Truncate to at most `max` characters at a word boundary and append a real
  /// ellipsis, so a long title never clips mid-word without one.
  func ellipsized(_ max: Int) -> String {
    guard count > max else { return self }
    let slice = prefix(max)
    if let space = slice.lastIndex(of: " "), space != slice.startIndex {
      return slice[slice.startIndex..<space].trimmingCharacters(in: .whitespaces) + "\u{2026}"
    }
    return slice.trimmingCharacters(in: .whitespaces) + "\u{2026}"
  }
}

func humanLabel(_ raw: String) -> String {
  if raw.contains(" ") { return raw }
  guard raw.contains("_") || raw.contains("-") else { return raw }
  return raw
    .replacingOccurrences(of: "_", with: " ")
    .replacingOccurrences(of: "-", with: " ")
    .split(separator: " ")
    .map { $0.prefix(1).uppercased() + $0.dropFirst() }
    .joined(separator: " ")
}

struct EvidenceRow: View {
  let item: EvidenceItem

  var body: some View {
    HStack(alignment: .top, spacing: 12) {
      Image(systemName: item.readOnly ? "checkmark.seal" : "exclamationmark.triangle")
        .font(.callout)
        .foregroundStyle(item.readOnly ? Color.green : Color.orange)
        .frame(width: 18)
      VStack(alignment: .leading, spacing: 2) {
        Text(humanLabel(item.title))
          .font(.subheadline.weight(.medium))
        if !item.detail.isEmpty, !item.detail.contains("/") {
          Text(item.detail)
            .font(.caption)
            .foregroundStyle(.secondary)
            .lineLimit(2)
        }
        Text(item.route)
          .font(.caption2.monospaced())
          .foregroundStyle(.tertiary)
          .lineLimit(1)
      }
      Spacer(minLength: 0)
    }
    .padding(.vertical, 9)
    .padding(.horizontal, 12)
    .frame(maxWidth: .infinity, alignment: .leading)
    .background(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .fill(Color.primary.opacity(0.03))
    )
  }
}

struct RouteRow: View {
  let card: RouteCard
  let selected: Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack {
        Text(card.title)
          .font(.headline)
        Spacer()
        Text(card.metric)
          .font(.caption.monospaced())
          .foregroundStyle(.secondary)
        StatusPill(status: card.status.rawValue, tone: card.status == .ok ? .positive : .neutral)
      }

      if !card.detail.isEmpty {
        Text(card.detail)
          .font(.callout)
          .foregroundStyle(.secondary)
          .lineLimit(2)
      }

      HStack(spacing: 8) {
        Text(card.path)
          .font(.caption.monospaced())
          .foregroundStyle(.tertiary)
          .lineLimit(1)
        if card.receiptBacked {
          Label("Proven", systemImage: "checkmark.seal")
            .font(.caption)
            .foregroundStyle(.secondary)
        }
        if card.readOnly {
          Label("Inspecting", systemImage: "eye")
            .font(.caption)
            .foregroundStyle(.secondary)
        }
      }
    }
    .padding(14)
    .mdxGlassSurface(interactive: true)
    .background(selected ? Color.accentColor.opacity(0.10) : Color.clear)
    .overlay(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .stroke(selected ? Color.accentColor.opacity(0.45) : Color.secondary.opacity(0.10), lineWidth: 1)
    )
  }
}

struct InfoRow: View {
  let label: String
  let value: String

  var body: some View {
    HStack(alignment: .firstTextBaseline) {
      Text(label)
        .font(.caption.weight(.semibold))
        .foregroundStyle(.secondary)
        .frame(width: 120, alignment: .leading)
      Text(value)
        .font(.callout)
        .textSelection(.enabled)
      Spacer()
    }
  }
}

struct EmptyStateView: View {
  let text: String

  var body: some View {
    Text(text)
      .foregroundStyle(.secondary)
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(16)
      .mdxGlassSurface()
  }
}

enum StatusTone: Equatable {
  case positive
  case neutral
  case locked

  // One source of truth for capsule tint across the app: tinted text on a soft
  // fill of the same hue. StatusPill, the run identity chips, and the folded
  // Pages/Marketplace/You badges all read their colors from here.
  var foreground: Color {
    switch self {
    case .positive: return .green
    case .neutral: return .secondary
    case .locked: return .orange
    }
  }

  var fill: Color { foreground.opacity(0.14) }
}

struct ComposerChip: View {
  let icon: String
  let text: String
  var menu: Bool = false

  var body: some View {
    HStack(spacing: 5) {
      Image(systemName: icon).font(.caption2)
      Text(text).font(.caption).lineLimit(1)
      if menu {
        Image(systemName: "chevron.down").font(.caption2)
      }
    }
    .foregroundStyle(.secondary)
    .padding(.horizontal, 9)
    .padding(.vertical, 4)
    .background(Capsule().fill(Color.secondary.opacity(0.10)))
  }
}

struct StatusPill: View {
  let status: String
  var tone: StatusTone = .neutral
  /// An explicit hue overrides the tone triad, so surfaces that carry their own
  /// semantic colors (page state, capability status, account role) fold into
  /// this one capsule recipe without losing meaning.
  var color: Color? = nil

  var body: some View {
    // One badge language app-wide: tinted text on a soft tint fill (the
    // Pages/Marketplace treatment), not primary text on a color block.
    Text(status)
      .font(.caption.weight(.semibold))
      .lineLimit(1)
      .padding(.horizontal, 9)
      .padding(.vertical, 5)
      .foregroundStyle(foreground)
      .background(background)
      .clipShape(Capsule())
  }

  private var foreground: Color { color ?? tone.foreground }
  private var background: Color { (color ?? tone.foreground).opacity(0.14) }
}
