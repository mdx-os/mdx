import SwiftUI

struct CommandPaletteView: View {
  @Environment(OperatorStore.self) private var store
  @Environment(\.dismiss) private var dismiss
  @State private var query = ""
  @State private var selected = 0
  @FocusState private var queryFocused: Bool

  var body: some View {
    let currentGroups = groups
    let flat = currentGroups.flatMap { $0.items }
    let active = flat.isEmpty ? -1 : min(max(selected, 0), flat.count - 1)

    return VStack(alignment: .leading, spacing: 12) {
      HStack(spacing: 10) {
        Image(systemName: "magnifyingglass")
          .foregroundStyle(.secondary)
        TextField("Search MDx or run a command", text: $query)
          .textFieldStyle(.plain)
          .font(.title3)
          .focused($queryFocused)
      }
      .padding(12)
      .mdxGlassSurface(interactive: true)

      ScrollViewReader { proxy in
        ScrollView {
          VStack(alignment: .leading, spacing: 12) {
            ForEach(currentGroups, id: \.name) { group in
              VStack(alignment: .leading, spacing: 4) {
                Text(group.name.uppercased())
                  .font(.caption2.weight(.semibold))
                  .foregroundStyle(.tertiary)
                  .padding(.horizontal, 6)
                ForEach(group.items) { item in
                  row(item, isSelected: active >= 0 && flat[active].id == item.id)
                    .id(item.id)
                }
              }
            }
            if currentGroups.isEmpty {
              Text("No matches for \(query)")
                .font(.callout)
                .foregroundStyle(.tertiary)
                .padding(12)
            }
          }
        }
        .frame(maxHeight: 440)
        .onChange(of: active) { _, index in
          guard index >= 0, flat.indices.contains(index) else { return }
          withAnimation(.easeOut(duration: 0.12)) { proxy.scrollTo(flat[index].id, anchor: .center) }
        }
      }
    }
    .padding(18)
    .frame(width: 640)
    .background(keyboardControls(flat: flat, active: active))
    .onChange(of: query) { _, _ in selected = 0 }
    .onAppear { queryFocused = true }
  }

  // Hidden buttons give the palette an arrow-key + Return model without
  // fighting the text field for focus.
  private func keyboardControls(flat: [CommandPaletteItem], active: Int) -> some View {
    VStack(spacing: 0) {
      Button("") { move(-1, count: flat.count) }
        .keyboardShortcut(.upArrow, modifiers: [])
      Button("") { move(1, count: flat.count) }
        .keyboardShortcut(.downArrow, modifiers: [])
      Button("") { if flat.indices.contains(active) { run(flat[active]) } }
        .keyboardShortcut(.defaultAction)
      Button("") { store.hideCommandPalette(); dismiss() }
        .keyboardShortcut(.cancelAction)
    }
    .opacity(0)
    .frame(width: 0, height: 0)
    .accessibilityHidden(true)
  }

  private func move(_ delta: Int, count: Int) {
    guard count > 0 else { return }
    selected = (min(max(selected, 0), count - 1) + delta + count) % count
  }

  private func row(_ item: CommandPaletteItem, isSelected: Bool) -> some View {
    Button {
      run(item)
    } label: {
      HStack(spacing: 12) {
        Image(systemName: item.systemImage)
          .font(.body)
          .frame(width: 22)
          .foregroundStyle(item.tint)
        VStack(alignment: .leading, spacing: 1) {
          Text(item.title)
            .font(.callout.weight(.medium))
            .lineLimit(1)
          Text(item.detail)
            .font(.caption)
            .foregroundStyle(.secondary)
            .lineLimit(1)
        }
        Spacer()
        if let shortcut = item.shortcut {
          Text(shortcut)
            .font(.caption.monospaced())
            .foregroundStyle(isSelected ? .secondary : .tertiary)
        }
      }
      .padding(.horizontal, 12)
      .padding(.vertical, 9)
      .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
    }
    .buttonStyle(.plain)
    .background(
      RoundedRectangle(cornerRadius: 9, style: .continuous)
        .fill(isSelected ? Color.accentColor.opacity(0.16) : Color.clear)
    )
    .overlay(
      RoundedRectangle(cornerRadius: 9, style: .continuous)
        .stroke(isSelected ? Color.accentColor.opacity(0.4) : Color.clear, lineWidth: 1)
    )
    .onHover { if $0, let idx = indexOf(item) { selected = idx } }
    .modifier(QuickJump(index: item.quickIndex) { run(item) })
  }

  private func indexOf(_ item: CommandPaletteItem) -> Int? {
    groups.flatMap { $0.items }.firstIndex { $0.id == item.id }
  }

  private func run(_ item: CommandPaletteItem) {
    item.perform()
    store.hideCommandPalette()
    dismiss()
  }

  // MARK: - Items

  private struct Group { let name: String; let items: [CommandPaletteItem] }

  private var groups: [Group] {
    let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
    func filter(_ items: [CommandPaletteItem]) -> [CommandPaletteItem] {
      guard !trimmed.isEmpty else { return items }
      return items.filter {
        $0.title.localizedCaseInsensitiveContains(trimmed) || $0.detail.localizedCaseInsensitiveContains(trimmed)
      }
    }
    var result: [Group] = []
    // Raycast/Codex move: the palette doesn't just navigate, it DISPATCHES.
    // Any real sentence becomes a build or a Twin ask in one keystroke.
    if trimmed.count >= 12, trimmed.contains(" ") {
      result.append(Group(name: "Dispatch", items: [
        CommandPaletteItem(
          id: "dispatch-build", title: "Start a build: \u{201c}\(trimmed)\u{201d}",
          detail: "Hand this ask to Forge with a shape recommendation",
          systemImage: "hammer.fill", tint: .accentColor, shortcut: nil
        ) {
          store.pendingBuildIntent = trimmed
          store.startBuildFlow()
        },
        CommandPaletteItem(
          id: "dispatch-twin", title: "Ask Twin: \u{201c}\(trimmed)\u{201d}",
          detail: "Think it through with your companion",
          systemImage: "sparkles", tint: .accentColor, shortcut: nil
        ) {
          store.select(.twin)
          store.sendTwin(trimmed)
        }
      ]))
    }
    let named: [(String, [CommandPaletteItem])] = [
      ("Go to", goToItems), ("Do", actionItems), ("Recent", recentItems)
    ]
    // Assign ⌘1–5 across the flattened, filtered result set (Codex-style quick jump).
    var quick = 0
    for (name, raw) in named {
      var filtered = filter(raw)
      if filtered.isEmpty { continue }
      filtered = filtered.map { item in
        var copy = item
        if quick < 5 { quick += 1; copy.quickIndex = quick }
        return copy
      }
      result.append(Group(name: name, items: filtered))
    }
    return result
  }

  private var goToItems: [CommandPaletteItem] {
    [
      routeItem(.home, "Situation, Twin composer, and jump back in"),
      routeItem(.forge(.overview), "Start or resume the work Forge puts first"),
      routeItem(.forge(.runs), "Follow work from intent to proof"),
      routeItem(.forge(.review), "Review ship decisions and proof gates"),
      routeItem(.twin, "Ask and reason from evidence"),
      routeItem(.pages, "Open context and knowledge"),
      routeItem(.message, "Governed coordination and approvals"),
      routeItem(.memory, "Approved lessons and receipt-backed recall"),
      routeItem(.marketplace, "Capabilities and skills"),
      routeItem(.you, "Identity, authority, and connections")
    ]
  }

  private var actionItems: [CommandPaletteItem] {
    [
      CommandPaletteItem(id: "start-build", title: "Start a build", detail: "Ask Forge to build or fix something",
                         systemImage: "hammer.fill", tint: .accentColor, shortcut: "⌘N") { store.startBuildFlow() },
      CommandPaletteItem(id: "new-page", title: "New page", detail: "Write and publish to your library",
                         systemImage: "square.and.pencil", tint: .accentColor, shortcut: nil) { store.startNewPage() },
      CommandPaletteItem(id: "diagnostics", title: "Diagnostics", detail: "Latency, streams, hangs, and your trail",
                         systemImage: "waveform.path.ecg", tint: .secondary, shortcut: "⌘⇧D") { store.showDiagnosticsPanel() },
      CommandPaletteItem(id: "refresh", title: "Refresh local MDx", detail: "Reload route state and readiness",
                         systemImage: "arrow.clockwise", tint: .secondary, shortcut: "⌘R") { Task { await store.refresh() } },
      CommandPaletteItem(id: "report", title: "Report an issue", detail: "Send a bug, blocker, or idea with a receipt back",
                         systemImage: "ladybug", tint: .secondary, shortcut: "⌘⇧B") { store.startFeedback() },
      CommandPaletteItem(id: "settings", title: "Settings", detail: "Appearance, connection, authority, updates",
                         systemImage: "gearshape", tint: .secondary, shortcut: "⌘,") { store.showSettings() }
    ]
  }

  private var recentItems: [CommandPaletteItem] {
    var items: [CommandPaletteItem] = []
    for run in store.forgeRuns.prefix(5) {
      items.append(CommandPaletteItem(
        id: "run-\(run.id)", title: run.title.isEmpty ? run.id : run.title,
        detail: run.isRunning ? "Running now — open to watch" : "Open this run",
        systemImage: "play.rectangle", tint: run.isRunning ? .green : .secondary, shortcut: nil
      ) {
        store.select(.forge(.runs)); store.openRun(run.id)
      })
    }
    for convo in store.twinConversations.prefix(4) {
      items.append(CommandPaletteItem(
        id: "twin-\(convo.id)", title: convo.title, detail: "Twin · \(convo.snippet)",
        systemImage: "sparkles", tint: .secondary, shortcut: nil
      ) {
        store.select(.twin); store.switchTwinConversation(convo.id)
      })
    }
    return items
  }

  private func routeItem(_ route: AppRoute, _ detail: String) -> CommandPaletteItem {
    CommandPaletteItem(id: route.id, title: route.title, detail: detail,
                       systemImage: route.systemImage, tint: .secondary, shortcut: nil) {
      store.select(route)
    }
  }
}

struct CommandPaletteItem: Identifiable {
  let id: String
  let title: String
  let detail: String
  let systemImage: String
  var tint: Color = .secondary
  let shortcut: String?
  var quickIndex: Int? = nil
  let perform: () -> Void
}

/// Attaches ⌘1–⌘5 to the top results so a search-and-jump is one keystroke.
private struct QuickJump: ViewModifier {
  let index: Int?
  let action: () -> Void

  func body(content: Content) -> some View {
    if let index, let key = KeyEquivalent(exactly: index) {
      content.overlay(
        Button("", action: action).keyboardShortcut(key, modifiers: .option).opacity(0).frame(width: 0, height: 0)
      )
    } else {
      content
    }
  }
}

private extension KeyEquivalent {
  init?(exactly index: Int) {
    guard let scalar = "\(index)".unicodeScalars.first else { return nil }
    self.init(Character(scalar))
  }
}
