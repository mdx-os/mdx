import AppKit
import SwiftUI

// MARK: - Activity log row

struct LogRow: View {
  @Environment(OperatorStore.self) private var store
  let event: ForgeEvent
  @State private var hoveringRow = false

  private var showDetail: Bool {
    !event.displayDetail.isEmpty && event.displayDetail != event.displaySummary
  }

  // Text shaping is O(length), and it runs for every visible row on every ~4/sec
  // trail flush while a run streams. Raw proof output (a pnpm "+177 +++...+++"
  // progress bar, a grep usage dump) can be many hundreds of characters, and even
  // with lineLimit(3) SwiftUI shapes the whole string to find the truncation point.
  // Those handful of huge rows near the streaming tail were the main-thread cost
  // that saturated rendering during a live run (perf timing cleared the app's own
  // mutation code). The cap only bounds what is laid out; the full text stays on
  // the receipt and in the copy actions, and 3 lines at this width is ~430 chars,
  // so nothing visible is lost.
  private static let summaryDisplayCap = 480
  private static let detailDisplayCap = 560

  private var cappedSummary: String { Self.cap(event.displaySummary, Self.summaryDisplayCap) }
  private var cappedDetail: String { Self.cap(event.displayDetail, Self.detailDisplayCap) }

  private static func cap(_ text: String, _ limit: Int) -> String {
    guard text.count > limit else { return text }
    return String(text.prefix(limit)) + "…"
  }

  var body: some View {
    HStack(alignment: .top, spacing: 10) {
      Circle()
        .fill(tint)
        .frame(width: 6, height: 6)
        .padding(.top, 6)
      VStack(alignment: .leading, spacing: 2) {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
          if let tag = kindTag {
            Text(tag)
              .font(.system(size: 10, weight: .semibold, design: .monospaced))
              .foregroundStyle(tint)
              .padding(.horizontal, 4)
              .padding(.vertical, 1)
              .background(tint.opacity(0.12))
              .clipShape(RoundedRectangle(cornerRadius: 3, style: .continuous))
          }
          if !event.model.isEmpty {
            Text(event.model)
              .font(.system(size: 10, weight: .medium, design: .monospaced))
              .foregroundStyle(.secondary)
              .lineLimit(1)
              .padding(.horizontal, 4)
              .padding(.vertical, 1)
              .background(Color.secondary.opacity(0.10))
              .clipShape(RoundedRectangle(cornerRadius: 3, style: .continuous))
          }
        }
        Text(cappedSummary)
          .font(.callout)
          .foregroundStyle(.primary)
          .fixedSize(horizontal: false, vertical: true)
        if showDetail {
          Text(cappedDetail)
            .font(.caption.monospaced())
            .foregroundStyle(.secondary)
            .lineLimit(3)
            .fixedSize(horizontal: false, vertical: true)
        }
        if let count = event.approvedLessonCitationCount, count > 0 {
          Button {
            store.select(.memory)
            store.selectedMemoryRail = .learned
          } label: {
            Label("Review the lessons used", systemImage: "brain.head.profile")
          }
          .buttonStyle(.plain)
          .font(.caption)
          .foregroundStyle(Color.accentColor)
          .help("Open the approved lessons that may guide Forge planning")
        }
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      Spacer(minLength: 6)
      if event.hasReceipt {
        // Warp-block pattern: every receipted action is a stable, copyable
        // identity you can cite in review.
        Button {
          Pasteboard.copy(event.receiptID)
        } label: {
          Image(systemName: "checkmark.seal")
            .font(.caption2)
            .foregroundStyle(.tertiary)
        }
        .buttonStyle(.plain)
        .help("Copy receipt \(event.receiptID)")
        .accessibilityLabel("Copy receipt ID")
      }
    }
    .padding(.vertical, 6)
    .padding(.horizontal, 4)
    .overlay(alignment: .bottom) {
      Rectangle()
        .fill(Color.secondary.opacity(0.10))
        .frame(height: 1)
    }
    .contentShape(Rectangle())
    .contextMenu {
      if event.hasReceipt {
        Button("Copy Receipt ID") { Pasteboard.copy(event.receiptID) }
      }
      if !event.summary.isEmpty {
        Button("Copy Summary") { Pasteboard.copy(event.summary) }
      }
      if showDetail {
        Button("Copy Detail") { Pasteboard.copy(event.detail) }
      }
    }
  }

  private var kindTag: String? {
    let k = event.kind.lowercased()
    if k.contains("model") { return "MODEL" }
    if k.contains("tool_call") { return "TOOL" }
    if k.contains("tool_result") { return "DONE" }
    if k.contains("token") { return "THINK" }
    if k.contains("proof") { return "PROOF" }
    if k.contains("stage") { return "STAGE" }
    if k.contains("diff") { return "DIFF" }
    if k.contains("evidence") { return "NOTE" }
    if k.contains("terminal") || k.contains("complete") { return "END" }
    if k.contains("error") { return "ERROR" }
    return nil
  }

  private var tint: Color {
    let k = event.kind.lowercased()
    if k.contains("error") { return .orange }
    if k.contains("model") { return .purple }
    if k.contains("proof") || k.contains("tool_result") || k.contains("complete") { return .green }
    if k.contains("tool_call") || k.contains("token") { return .accentColor }
    return .secondary
  }
}

// MARK: - Diff

struct DiffFileView: View {
  let file: DiffFile
  var mode: DiffViewMode = .unified
  /// Local checkout root, when known: enables open-in-editor per file.
  var repoRoot: String? = nil
  @Binding var expanded: Bool
  /// Per-file "revise" verb: the diff is where review judgment happens, so
  /// the comment path starts there (Zed/Codex pattern).
  var requestChanges: ((String) -> Void)? = nil
  /// The receipt seam: jump to the trail steps that produced this file.
  var showActivity: ((String) -> Void)? = nil
  /// Live find text: matches are highlighted in the rendered lines.
  var searchTerm: String = ""
  @State private var hoveringHeader = false

  /// Visual-QA only: force the hover-revealed per-file actions visible for a
  /// screenshot (there is no way to hold a synthetic hover across a capture).
  private var forceFileActions: Bool {
    ProcessInfo.processInfo.environment["MDX_SHOT_FILE_ACTIONS"] != nil
  }
  private var showHeaderActions: Bool { hoveringHeader || forceFileActions }

  var body: some View {
    VStack(alignment: .leading, spacing: 0) {
      Button {
        withAnimation(.easeOut(duration: 0.15)) { expanded.toggle() }
      } label: {
        HStack(spacing: 10) {
          Image(systemName: expanded ? "chevron.down" : "chevron.right")
            .font(.caption)
            .foregroundStyle(.secondary)
          Text(file.path)
            .font(.callout.monospaced())
            .lineLimit(1)
            .truncationMode(.middle)
          Spacer(minLength: 8)
          // Per-file review verbs, revealed on hover so a clean diff stays quiet
          // but a reviewer can anchor a change request to exactly this file
          // without scrolling to the run-level action bar.
          if showHeaderActions, let requestChanges {
            Button {
              requestChanges(file.path)
            } label: {
              Label("Request changes", systemImage: "arrow.uturn.backward")
                .font(.caption)
            }
            .buttonStyle(.plain)
            .foregroundStyle(Color.accentColor)
            .help("Send this file back with a note, without failing the whole run")
            .accessibilityLabel("Request changes to \(file.path)")
          }
          if let showActivity {
            Button {
              showActivity(file.path)
            } label: {
              Image(systemName: "clock.arrow.circlepath")
                .font(.caption)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.tertiary)
            .help("Show the steps that produced this change")
            .accessibilityLabel("Show the steps that produced this change")
          }
          Text("+\(file.added)")
            .font(.caption.monospaced())
            .contentTransition(.numericText())
            .foregroundStyle(.green)
          Text("-\(file.removed)")
            .font(.caption.monospaced())
            .contentTransition(.numericText())
            .foregroundStyle(.red)
        }
        .padding(12)
        .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
      .onHover { hoveringHeader = $0 }
      .contextMenu {
        if let requestChanges {
          Button("Request Changes to This File…") { requestChanges(file.path) }
        }
        if let showActivity {
          Button("Show Steps That Touched This File") { showActivity(file.path) }
        }
        if let repoRoot, !repoRoot.isEmpty {
          Divider()
          ForEach(EditorOpener.available()) { destination in
            Button("Open in \(destination.label)") {
              destination.open((repoRoot as NSString).appendingPathComponent(file.path), nil)
            }
          }
        }
        Divider()
        Button("Copy File Path") { Pasteboard.copy(file.path) }
        Button("Copy Patch") { Pasteboard.copy(file.patch) }
      }

      if expanded {
        Divider()
        if mode == .split {
          DiffSplitView(rows: file.splitRows, searchTerm: searchTerm)
        } else {
          DiffPatchView(lines: file.lines, searchTerm: searchTerm)
        }
      }
    }
    .mdxGlassSurface()
  }
}

struct DiffPatchView: View {
  let lines: [DiffLine]
  var searchTerm: String = ""

  var body: some View {
    LazyVStack(alignment: .leading, spacing: 0) {
      ForEach(lines) { line in
        HStack(alignment: .top, spacing: 0) {
          DiffGutter(numbers: [line.oldLine, line.newLine])
          DiffLineText(line: line, searchTerm: searchTerm)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.trailing, 10)
        }
        .padding(.vertical, 1)
        .background(DiffPalette.rowBackground(line.kind))
      }
    }
    .textSelection(.enabled)
  }
}

/// Side-by-side diff: old on the left, new on the right, hunk/meta spanning
/// both. Rows are pre-paired at load time.
struct DiffSplitView: View {
  let rows: [DiffSplitRow]
  var searchTerm: String = ""

  var body: some View {
    LazyVStack(alignment: .leading, spacing: 0) {
      ForEach(rows) { row in
        if let full = row.full {
          HStack(alignment: .top, spacing: 0) {
            DiffGutter(numbers: [full.oldLine, full.newLine])
            DiffLineText(line: full, searchTerm: searchTerm)
              .frame(maxWidth: .infinity, alignment: .leading)
              .padding(.trailing, 10)
          }
          .padding(.vertical, 1)
          .background(DiffPalette.rowBackground(full.kind))
        } else {
          HStack(alignment: .top, spacing: 0) {
            splitCell(row.left, emptyKind: .deletion, number: row.left?.oldLine)
            Divider()
            splitCell(row.right, emptyKind: .addition, number: row.right?.newLine)
          }
        }
      }
    }
    .textSelection(.enabled)
  }

  @ViewBuilder
  private func splitCell(_ line: DiffLine?, emptyKind: DiffLine.Kind, number: Int?) -> some View {
    Group {
      if let line {
        HStack(alignment: .top, spacing: 0) {
          DiffGutter(numbers: [number])
          DiffLineText(line: line, stripMarker: true, searchTerm: searchTerm)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.trailing, 10)
        }
        .padding(.vertical, 1)
        .background(DiffPalette.rowBackground(line.kind))
      } else {
        HStack(alignment: .top, spacing: 0) {
          DiffGutter(numbers: [nil])
          Text(" ")
            .font(.system(.caption, design: .monospaced))
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.trailing, 10)
        }
        .padding(.vertical, 1)
        .background(DiffPalette.rowBackground(emptyKind).opacity(0.4))
      }
    }
  }
}

/// Fixed-width line-number gutter. Non-selectable and hidden from
/// accessibility so a copy of the diff yields code, not line numbers, and
/// VoiceOver reads the line rather than a bare number.
struct DiffGutter: View {
  let numbers: [Int?]

  var body: some View {
    HStack(spacing: 0) {
      ForEach(Array(numbers.enumerated()), id: \.offset) { _, number in
        Text(number.map(String.init) ?? " ")
          .font(.system(size: 10, design: .monospaced))
          .foregroundStyle(.tertiary)
          .frame(width: 32, alignment: .trailing)
          .padding(.trailing, 4)
      }
    }
    .padding(.leading, 8)
    .textSelection(.disabled)
    .accessibilityHidden(true)
  }
}

/// One diff line with intraline emphasis: the changed middle of a paired
/// -/+ line gets a stronger tint so the eye lands on exactly what changed.
/// A live find term, when present, gets a yellow wash on every occurrence.
struct DiffLineText: View {
  let line: DiffLine
  var stripMarker: Bool = false
  var searchTerm: String = ""

  var body: some View {
    Text(attributed)
      .font(.system(.caption, design: .monospaced))
  }

  private var attributed: AttributedString {
    var display = line.text
    var emphasis = line.emphasis
    if stripMarker, line.kind == .addition || line.kind == .deletion, !display.isEmpty {
      display = String(display.dropFirst())
      if let range = emphasis { emphasis = (range.lowerBound - 1)..<(range.upperBound - 1) }
    }
    if display.isEmpty { display = " " }
    var result = AttributedString(display)
    result.foregroundColor = line.isComment ? DiffPalette.commentColor : DiffPalette.textColor(line.kind)
    if let emphasis,
       let start = result.index(result.startIndex, offsetByCharactersSafe: emphasis.lowerBound),
       let end = result.index(result.startIndex, offsetByCharactersSafe: emphasis.upperBound),
       start < end {
      result[start..<end].backgroundColor = DiffPalette.emphasisBackground(line.kind)
    }
    applySearchHighlight(&result, in: display)
    return result
  }

  /// Wash every occurrence of the find term. Case-insensitive, and it runs only
  /// while a search is active so the steady-state render path is untouched.
  private func applySearchHighlight(_ result: inout AttributedString, in display: String) {
    let term = searchTerm.trimmingCharacters(in: .whitespaces)
    guard term.count >= 2 else { return }
    var searchStart = display.startIndex
    while let found = display.range(of: term, options: .caseInsensitive, range: searchStart..<display.endIndex) {
      let lower = display.distance(from: display.startIndex, to: found.lowerBound)
      let upper = display.distance(from: display.startIndex, to: found.upperBound)
      if let start = result.index(result.startIndex, offsetByCharactersSafe: lower),
         let end = result.index(result.startIndex, offsetByCharactersSafe: upper),
         start < end {
        result[start..<end].backgroundColor = Color.yellow.opacity(0.45)
        result[start..<end].foregroundColor = Color.primary
      }
      searchStart = found.upperBound
      if searchStart >= display.endIndex { break }
    }
  }
}

extension AttributedString {
  /// Bounds-safe character offset; intraline offsets were computed in UTF-16
  /// and multi-scalar glyphs could otherwise walk past the end.
  func index(_ base: Index, offsetByCharactersSafe offset: Int) -> Index? {
    var current = base
    for _ in 0..<offset {
      guard current < endIndex else { return nil }
      current = index(afterCharacter: current)
    }
    return current
  }
}

/// One place for the diff's color language.
enum DiffPalette {
  static func textColor(_ kind: DiffLine.Kind) -> Color {
    switch kind {
    case .addition: return .green
    case .deletion: return .red
    case .hunk: return .accentColor
    case .meta: return .secondary
    case .context: return .primary
    }
  }

  static let commentColor: Color = .secondary

  static func rowBackground(_ kind: DiffLine.Kind) -> Color {
    switch kind {
    case .addition: return .green.opacity(0.08)
    case .deletion: return .red.opacity(0.08)
    default: return .clear
    }
  }

  static func emphasisBackground(_ kind: DiffLine.Kind) -> Color {
    switch kind {
    case .addition: return .green.opacity(0.28)
    case .deletion: return .red.opacity(0.28)
    default: return .clear
    }
  }
}

/// Compact map of the changed files: shape of the change at a glance, one
/// click to jump (Kaleidoscope changeset-outline pattern, kept light).
struct DiffFileMap: View {
  struct Signal: Equatable {
    let steps: Int
    let flagged: Int
    var confidence: String = ""
    var checks: Int = 0

    var confidenceLabel: String? {
      switch confidence {
      case "needs_attention": return "look here first"
      case "checked": return "checked"
      case "mentioned": return nil
      case "unscored": return "unverified"
      default: return nil
      }
    }
  }

  let files: [DiffFile]
  var signals: [String: Signal] = [:]
  let jump: (String) -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 2) {
      ForEach(files) { file in
        Button {
          jump(file.path)
        } label: {
          HStack(spacing: 8) {
            Image(systemName: "doc.text")
              .font(.caption2)
              .foregroundStyle(.tertiary)
            Text(file.path)
              .font(.caption.monospaced())
              .lineLimit(1)
              .truncationMode(.middle)
            if let signal = signals[file.path], signal.flagged > 0 {
              Label("\(signal.flagged)", systemImage: "exclamationmark.triangle")
                .font(.caption2)
                .foregroundStyle(.orange)
                .help("\(signal.flagged) recorded step\(signal.flagged == 1 ? "" : "s") hit an error or retry near this file — look here first")
            }
            if let signal = signals[file.path], let label = signal.confidenceLabel {
              Text(label)
                .font(.caption2)
                .foregroundStyle(signal.confidence == "needs_attention" ? Color.orange : (signal.confidence == "checked" ? Color.green : Color.secondary))
                .help(signal.checks > 0 ? "\(signal.checks) check\(signal.checks == 1 ? "" : "s") touched this file" : "No checks touched this file")
            }
            Spacer(minLength: 8)
            if let signal = signals[file.path], signal.steps > 0 {
              Text("\(signal.steps) steps")
                .font(.caption2)
                .foregroundStyle(.tertiary)
                .help("Recorded steps that mention this file")
            }
            ChangeBar(added: file.added, removed: file.removed)
            Text("+\(file.added) -\(file.removed)")
              .font(.caption2.monospaced())
              .foregroundStyle(.tertiary)
          }
          .padding(.horizontal, 8)
          .padding(.vertical, 3)
          .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
      }
    }
    .padding(6)
    .mdxGlassSurface()
  }
}

/// GitHub-style proportional +/- bar, capped so one huge file cannot flatten
/// the rest.
struct ChangeBar: View {
  let added: Int
  let removed: Int

  var body: some View {
    let total = max(added + removed, 1)
    let capped = min(total, 60)
    let addedWidth = CGFloat(capped) * CGFloat(added) / CGFloat(total)
    HStack(spacing: 1) {
      RoundedRectangle(cornerRadius: 1).fill(Color.green.opacity(0.75))
        .frame(width: max(addedWidth, added > 0 ? 2 : 0), height: 5)
      RoundedRectangle(cornerRadius: 1).fill(Color.red.opacity(0.75))
        .frame(width: max(CGFloat(capped) - addedWidth, removed > 0 ? 2 : 0), height: 5)
    }
    .frame(width: 62, alignment: .trailing)
  }
}

struct RunNoteSheet: View {
  @Environment(\.dismiss) private var dismiss
  let title: String
  let message: String
  let placeholder: String
  let submitLabel: String
  @Binding var text: String
  let inFlight: Bool
  /// Quick, honest reasons a reviewer can drop in with one tap. They fill the
  /// note, which lands verbatim on the governed receipt, so they read like a
  /// person wrote them and never overstate what was checked.
  var suggestions: [String] = []
  let submit: () -> Void
  @FocusState private var noteFocused: Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 16) {
      Text(title)
        .font(.title2.weight(.semibold))
      Text(message)
        .font(.callout)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)
      if !suggestions.isEmpty {
        VStack(alignment: .leading, spacing: 6) {
          Text("Quick reasons")
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
          Wrap(spacing: 6, lineSpacing: 6) {
            ForEach(suggestions, id: \.self) { reason in
              Button {
                text = reason
                noteFocused = true
              } label: {
                Text(reason)
                  .font(.caption)
                  .lineLimit(1)
                  .padding(.horizontal, 10)
                  .padding(.vertical, 5)
                  .background(
                    Capsule().fill(text == reason ? Color.accentColor.opacity(0.18) : Color.primary.opacity(0.06))
                  )
                  .overlay(
                    Capsule().stroke(text == reason ? Color.accentColor.opacity(0.5) : Color.clear, lineWidth: 1)
                  )
              }
              .buttonStyle(.plain)
              .help("Use this reason")
            }
          }
        }
      }
      TextField(placeholder, text: $text, axis: .vertical)
        .textFieldStyle(.roundedBorder)
        .lineLimit(2...5)
        .focused($noteFocused)
        .onAppear { noteFocused = true }
      HStack(spacing: 10) {
        Spacer()
        Button("Cancel") { dismiss() }
          .keyboardShortcut(.cancelAction)
        Button {
          submit()
        } label: {
          Label(inFlight ? "Recording" : submitLabel, systemImage: "checkmark.seal")
        }
        .mdxPrimaryButtonStyle()
        .keyboardShortcut(.defaultAction)
        .disabled(inFlight || text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
      }
    }
    .padding(20)
    .frame(width: 520)
  }
}
