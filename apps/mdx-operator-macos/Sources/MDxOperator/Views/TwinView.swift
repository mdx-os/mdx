import AppKit
import QuickLook
import SwiftUI
import UniformTypeIdentifiers

struct TwinView: View {
  @Environment(OperatorStore.self) private var store
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @State private var draft = ""
  @State private var showRail = true
  @State private var dropTargeted = false
  @State private var quickLookURL: URL?
  @State private var showProject = false
  @State private var showDictationHelp = false
  @FocusState private var composerFocused: Bool

  private var canSend: Bool {
    let hasText = !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    return (hasText || !store.twinAttachments.isEmpty) && !store.twinSending
  }

  private var liveAnswerLength: Int {
    store.twinMessages.last?.answer.count ?? 0
  }

  var body: some View {
    HStack(spacing: 0) {
      if showRail {
        conversationRail
          .frame(width: SurfaceMetrics.railWidth)
          .transition(.move(edge: .leading).combined(with: .opacity))
        Divider()
      }
      chatColumn
    }
    .task {
      async let twin: Void = store.loadTwin()
      async let conversations: Void = store.loadTwinConversations()
      _ = await (twin, conversations)
      if let ask = ProcessInfo.processInfo.environment["MDX_SHOT_TWIN_ASK"] {
        // Fresh conversation so the hook always exercises a live stream.
        store.newTwinConversation()
        store.sendTwin(ask)
      }
    }
  }

  private var filteredConversations: [TwinConversation] {
    let query = store.twinConvoSearch.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !query.isEmpty else { return store.twinConversations }
    return store.twinConversations.filter {
      $0.title.localizedCaseInsensitiveContains(query) || $0.snippet.localizedCaseInsensitiveContains(query)
    }
  }

  private var chatColumn: some View {
    VStack(spacing: 0) {
      topBar
      Divider()
      ScrollViewReader { proxy in
        ScrollView {
          LazyVStack(alignment: .leading, spacing: 18) {
            if store.twinMessages.isEmpty {
              TwinEmptyState { draft = $0 }
                .padding(.top, 40)
            }
            ForEach(Array(store.twinMessages.enumerated()), id: \.element.id) { index, message in
              TwinMessageView(
                message: message,
                isLast: index == store.twinMessages.count - 1,
                onRegenerate: { store.regenerateLastTwin() }
              )
            }
            Color.clear.frame(height: 1).id("twin-bottom")
          }
          .padding(24)
          .frame(maxWidth: SurfaceMetrics.streamWidth, alignment: .leading)
          .frame(maxWidth: .infinity)
        }
        .onChange(of: store.twinMessages.count) { _, _ in
          scrollToBottom(proxy)
        }
        .onChange(of: liveAnswerLength) { _, _ in
          scrollToBottom(proxy)
        }
      }
      Divider()
      composer
    }
    .dropDestination(for: URL.self) { urls, _ in
      guard let url = urls.first(where: { $0.pathExtension.lowercased() == "pdf" }) else { return false }
      store.attachTwinPDF(url: url)
      return true
    } isTargeted: { dropTargeted = $0 }
    .overlay {
      if dropTargeted {
        RoundedRectangle(cornerRadius: 12, style: .continuous)
          .strokeBorder(Color.accentColor, style: StrokeStyle(lineWidth: 2, dash: [7]))
          .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
              .fill(Color.accentColor.opacity(0.06))
          )
          .overlay {
            Label("Drop a PDF for Twin to read", systemImage: "doc.text")
              .font(.callout.weight(.medium))
              .foregroundStyle(Color.accentColor)
              .padding(.horizontal, 16)
              .padding(.vertical, 10)
              .background(.regularMaterial, in: Capsule())
          }
          .padding(8)
          .allowsHitTesting(false)
          .transition(.opacity)
      }
    }
    .animation(reduceMotion ? nil : .easeOut(duration: 0.12), value: dropTargeted)
    .quickLookPreview($quickLookURL)
  }

  private func scrollToBottom(_ proxy: ScrollViewProxy) {
    if reduceMotion {
      proxy.scrollTo("twin-bottom", anchor: .bottom)
    } else {
      withAnimation(.easeOut(duration: 0.2)) { proxy.scrollTo("twin-bottom", anchor: .bottom) }
    }
  }

  private var conversationRail: some View {
    VStack(spacing: 0) {
      RailHeader(title: "Conversations") {
        Button {
          store.newTwinConversation()
        } label: {
          Image(systemName: "square.and.pencil")
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .help("New conversation")
        .accessibilityLabel("New conversation")
        .disabled(store.twinSending)
      }

      SurfaceSearchField(placeholder: "Search", text: Bindable(store).twinConvoSearch)
        .padding(.horizontal, 10)
        .padding(.bottom, 8)

      Divider()

      ScrollView {
        LazyVStack(alignment: .leading, spacing: 2, pinnedViews: [.sectionHeaders]) {
          if filteredConversations.isEmpty {
            if !store.twinConversationsLoaded {
              // First load still in flight; don't claim emptiness yet.
              RailSkeleton()
            } else {
              Text(railEmptyText)
                .font(.caption)
                .foregroundStyle(.tertiary)
                .padding(10)
            }
          }
          ForEach(groupedConversations, id: \.bucket) { group in
            Section {
              ForEach(group.items) { convo in
                ConversationRow(
                  conversation: convo,
                  selected: convo.id == store.twinSessionID,
                  localActivityAt: store.twinLocalActivityTimes[convo.id]
                ) {
                  store.switchTwinConversation(convo.id)
                }
              }
            } header: {
              Text(group.bucket.uppercased())
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.tertiary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 10)
                .padding(.vertical, 3)
                .background(.regularMaterial)
            }
          }
        }
        .padding(8)
      }
    }
    .background(Color.primary.opacity(0.025))
  }

  private var railEmptyText: String {
    if !store.twinConvoSearch.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      return "No conversations match."
    }
    if store.snapshot.connectionStatus != .ok {
      return "Not connected. Start local MDx to see your conversations."
    }
    return "Your conversations will collect here."
  }

  private var groupedConversations: [(bucket: String, items: [TwinConversation])] {
    let convos = filteredConversations
    return RelativeTime.bucketOrder.compactMap { bucket in
      let items = convos.filter { RelativeTime.bucket($0.lastTime) == bucket }
      return items.isEmpty ? nil : (bucket, items)
    }
  }

  private var topBar: some View {
    SurfaceTopBar(icon: "sparkles", title: "Twin",
                  subtitle: store.twinProjectName.isEmpty ? "your private work surface" : store.twinProjectName,
                  showRail: $showRail)
      .overlay(alignment: .trailing) {
        Button { showProject.toggle() } label: {
          Label(store.twinProjectName.isEmpty ? "Add project context" : "Project context", systemImage: "folder.badge.gearshape")
        }
          .buttonStyle(.bordered)
          .controlSize(.small)
          .padding(.trailing, 14)
          .popover(isPresented: $showProject) {
            VStack(alignment: .leading, spacing: 10) {
              Text("Project context").font(.headline)
              Text("Included explicitly with every Twin question on this Mac.")
                .font(.caption).foregroundStyle(.secondary)
              TextField("Project name", text: Bindable(store).twinProjectName)
              TextField("Instructions", text: Bindable(store).twinProjectInstructions, axis: .vertical)
                .lineLimit(2...5)
              Button("Save") { store.saveTwinProjectContext(); showProject = false }
                .keyboardShortcut(.defaultAction)
            }
            .padding(16).frame(width: 320)
          }
      }
  }

  private var composer: some View {
    VStack(spacing: 0) {
      if !store.twinAttachments.isEmpty || store.twinAttaching || store.twinAttachError != nil {
        attachmentTray
      }

      HStack(alignment: .bottom, spacing: 10) {
        Menu {
          ForEach(TwinStance.all) { stance in
            Button {
              store.twinStanceID = stance.id
            } label: {
              Text("\(stance.name) — \(stance.line)")
            }
          }
        } label: {
          HStack(spacing: 5) {
            Image(systemName: "person.bubble")
            Text(store.twinStance.name)
            Image(systemName: "chevron.up.chevron.down").font(.caption2)
          }
          .foregroundStyle(.secondary)
        }
        .menuStyle(.borderlessButton)
        .fixedSize()

        Button(action: pickPDF) {
          Image(systemName: "paperclip")
            .font(.body)
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .disabled(store.twinAttaching)
        .help("Attach a PDF for Twin to read")
        .accessibilityLabel("Attach a PDF")

        Button { composerFocused = true; showDictationHelp.toggle() } label: {
          Image(systemName: "mic")
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .help("Use macOS Dictation in the focused composer")
        .accessibilityLabel("Dictate with macOS")
        .popover(isPresented: $showDictationHelp) {
          VStack(alignment: .leading, spacing: 6) {
            Text("macOS Dictation").font(.headline)
            Text("The composer is focused. Use your Mac's Dictation shortcut, usually press the Fn key twice. MDx never records or stores audio.")
              .font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
          }.padding(14).frame(width: 280)
        }

        Button {
          store.twinSensitive.toggle()
        } label: {
          Image(systemName: store.twinSensitive ? "lock.shield.fill" : "lock.shield")
            .font(.body)
        }
        .buttonStyle(.plain)
        .foregroundStyle(store.twinSensitive ? Color.accentColor : .secondary)
        .help(store.twinSensitive
              ? "Private routing is on. This is tagged sensitive on the receipt and kept off frontier models. A dedicated sovereign model is not connected yet, so answers still come from the local model."
              : "Keep this private. Tags it sensitive on the receipt and routes it away from frontier models.")
        // Icon-only toggle: without an explicit label VoiceOver reads only
        // "button". Announce what it does and whether it is on, like the
        // Attach and Send siblings above and below.
        .accessibilityLabel("Keep private")
        .accessibilityValue(store.twinSensitive ? "On" : "Off")
        .accessibilityAddTraits(store.twinSensitive ? [.isSelected] : [])

        TextField("Think something through with Twin", text: $draft, axis: .vertical)
          .textFieldStyle(.plain)
          .font(.body)
          .lineLimit(1...6)
          .focused($composerFocused)
          .onSubmit(send)

        Button {
          if store.twinSending { store.stopTwin() } else { send() }
        } label: {
          Image(systemName: store.twinSending ? "stop.circle.fill" : "arrow.up.circle.fill")
            .font(.title2)
        }
        .buttonStyle(.plain)
        .foregroundStyle(store.twinSending ? .secondary : (canSend ? Color.accentColor : Color.secondary))
        .disabled(!store.twinSending && !canSend)
        .help(store.twinSending ? "Stop" : "Send")
        .accessibilityLabel(store.twinSending ? "Stop answering" : "Send to Twin")
      }
      .padding(.horizontal, 14)
      .padding(.vertical, 10)
    }
    .background(
      RoundedRectangle(cornerRadius: 14, style: .continuous)
        .fill(Color.primary.opacity(0.04))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 14, style: .continuous)
        .stroke(Color.secondary.opacity(0.15), lineWidth: 1)
    )
    .padding(16)
    .frame(maxWidth: SurfaceMetrics.streamWidth)
    .frame(maxWidth: .infinity)
  }

  private var attachmentTray: some View {
    VStack(alignment: .leading, spacing: 6) {
      if let error = store.twinAttachError {
        Label(error, systemImage: "exclamationmark.triangle")
          .font(.caption)
          .foregroundStyle(.orange)
          .lineLimit(2)
      }
      HStack(spacing: 6) {
        ForEach(store.twinAttachments) { attachment in
          AttachmentChip(attachment: attachment) {
            store.removeTwinAttachment(attachment.id)
          }
          .onTapGesture {
            // Quick Look the picked file: the native "did I grab the right
            // one" gesture.
            if let url = attachment.localURL { quickLookURL = url }
          }
          .help(attachment.localURL != nil ? "Click to preview" : attachment.name)
        }
        if store.twinAttaching {
          HStack(spacing: 6) {
            ProgressView().controlSize(.small)
            Text("Reading PDF…")
              .font(.caption)
              .foregroundStyle(.secondary)
          }
          .padding(.horizontal, 10)
          .padding(.vertical, 5)
          .background(
            Capsule().fill(Color.primary.opacity(0.05))
          )
        }
        Spacer(minLength: 0)
      }
    }
    .padding(.horizontal, 14)
    .padding(.top, 10)
    .padding(.bottom, 2)
  }

  private func pickPDF() {
    let panel = NSOpenPanel()
    panel.allowedContentTypes = [.pdf]
    panel.allowsMultipleSelection = false
    panel.canChooseDirectories = false
    panel.prompt = "Attach"
    panel.message = "Twin reads the text locally. Nothing leaves your machine until you send."
    if panel.runModal() == .OK, let url = panel.url {
      store.attachTwinPDF(url: url)
    }
  }

  private func send() {
    guard canSend else { return }
    let text = draft
    draft = ""
    store.sendTwin(text)
  }
}

struct AttachmentChip: View {
  let attachment: TwinAttachment
  let remove: () -> Void
  @State private var hovering = false

  var body: some View {
    HStack(spacing: 7) {
      Image(systemName: "doc.text")
        .font(.caption)
        .foregroundStyle(Color.accentColor)
      VStack(alignment: .leading, spacing: 0) {
        Text(attachment.name)
          .font(.caption.weight(.medium))
          .lineLimit(1)
        Text(attachment.pageLabel)
          .font(.caption2)
          .foregroundStyle(.secondary)
      }
      Button(action: remove) {
        Image(systemName: "xmark.circle.fill")
          .font(.caption)
          .foregroundStyle(.tertiary)
      }
      .buttonStyle(.plain)
      .accessibilityLabel("Remove attachment")
      .opacity(hovering ? 1 : 0.5)
      .help("Remove")
    }
    .padding(.horizontal, 10)
    .padding(.vertical, 5)
    .background(
      Capsule().fill(Color.primary.opacity(0.06))
    )
    .overlay(
      Capsule().stroke(Color.secondary.opacity(0.15), lineWidth: 1)
    )
    .onHover { hovering = $0 }
  }
}

struct ConversationRow: View {
  let conversation: TwinConversation
  let selected: Bool
  var localActivityAt: Date? = nil
  let open: () -> Void
  @State private var hovering = false

  private var snippet: String {
    if conversation.snippet.localizedCaseInsensitiveContains("no model is connected") {
      return "No answer - model unavailable"
    }
    return conversation.snippet
  }

  var body: some View {
    Button(action: open) {
      VStack(alignment: .leading, spacing: 2) {
        HStack(spacing: 6) {
          Text(conversation.title)
            .font(.subheadline.weight(.medium))
            .foregroundStyle(.primary)
            .lineLimit(1)
          Spacer(minLength: 4)
          Text(localActivityAt.map(RelativeTime.short) ?? RelativeTime.short(conversation.lastTime))
            .font(.caption2)
            .foregroundStyle(.tertiary)
        }
        Text(snippet)
          .font(.caption)
          .foregroundStyle(.secondary)
          .lineLimit(1)
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(.horizontal, 10)
      .padding(.vertical, 7)
      .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
      .background(
        RoundedRectangle(cornerRadius: 8, style: .continuous)
          .fill(selected ? Color.accentColor.opacity(0.14) : (hovering ? Color.primary.opacity(0.05) : Color.clear))
      )
    }
    .buttonStyle(.plain)
    .onHover { hovering = $0 }
    .contextMenu {
      Button("Open") { open() }
      Divider()
      Button("Copy Last Answer") { Pasteboard.copy(conversation.snippet) }
      Button("Copy Session ID") { Pasteboard.copy(conversation.id) }
    }
  }
}

struct TwinMessageView: View {
  @Environment(OperatorStore.self) private var store
  let message: TwinMessage
  var isLast: Bool = false
  var onRegenerate: () -> Void = {}
  @State private var hovering = false
  // Which answer action, if any, currently holds keyboard focus. Revealing on
  // focus (not just hover) keeps Copy and Ask again reachable without a mouse.
  @FocusState private var focusedAction: AnswerAction?

  private enum AnswerAction: Hashable { case copy, again }

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      // What you asked
      HStack {
        Spacer(minLength: 60)
        VStack(alignment: .trailing, spacing: 5) {
          if !message.attachmentNames.isEmpty {
            ForEach(message.attachmentNames, id: \.self) { name in
              HStack(spacing: 5) {
                Image(systemName: "doc.text")
                  .font(.caption2)
                Text(name)
                  .font(.caption)
                  .lineLimit(1)
              }
              .foregroundStyle(.secondary)
              .padding(.horizontal, 9)
              .padding(.vertical, 4)
              .background(
                Capsule().fill(Color.primary.opacity(0.06))
              )
            }
          }
          Text(message.prompt)
            .font(.callout)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(
              RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(Color.accentColor.opacity(0.12))
            )
            .textSelection(.enabled)
        }
      }

      // Twin's answer
      VStack(alignment: .leading, spacing: 7) {
        HStack(spacing: 6) {
          Image(systemName: "sparkles")
            .font(.caption)
            .foregroundStyle(Color.accentColor)
          Text("Twin")
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
          if message.pending {
            ProgressView().controlSize(.small)
          }
        }

        if message.pending && message.answer.isEmpty {
          Text("Thinking it through…")
            .font(.body)
            .foregroundStyle(.secondary)
        } else if message.pending {
          // Streaming: plain text + caret (markdown is parsed once complete).
          StreamingText(answer: message.answer)
            .font(.body)
            .foregroundStyle(.primary)
            .fixedSize(horizontal: false, vertical: true)
            .textSelection(.enabled)
        } else if message.isModelUnavailablePlaceholder {
          VStack(alignment: .leading, spacing: 6) {
            Label("No answer - a model was not connected for this turn", systemImage: "bolt.slash")
              .font(.callout.weight(.medium))
              .foregroundStyle(.secondary)
            DisclosureGroup("Details") {
              Text(message.answer)
                .font(.caption)
                .foregroundStyle(.secondary)
                .padding(.top, 4)
            }
            .font(.caption)
            .foregroundStyle(.tertiary)
          }
          .padding(10)
          .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
        } else {
          MarkdownText(text: message.answer)
        }

        if !message.pending && !message.answer.isEmpty {
          deliberationFooter
          sourceDisclosure
          if isLast { proposalActions }
        }
      }
    }
    .onHover { hovering = $0 }
  }

  private var actionButtons: some View {
    HStack(spacing: 8) {
      Button {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(message.answer, forType: .string)
      } label: {
        Image(systemName: "doc.on.doc")
      }
      .buttonStyle(.plain)
      .foregroundStyle(.secondary)
      .help("Copy answer")
      .accessibilityLabel("Copy answer")
      .focused($focusedAction, equals: .copy)

      if isLast {
        Button(action: onRegenerate) {
          Image(systemName: "arrow.clockwise")
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .help("Ask again")
        .accessibilityLabel("Ask again")
        .focused($focusedAction, equals: .again)
      }
    }
    .font(.caption2)
    // Reveal on hover for the mouse, and on keyboard focus so the actions are
    // not gated behind a pointer. The buttons stay in the layout and the
    // accessibility tree at all times, so VoiceOver reaches them regardless of
    // opacity; hover and focus only control the visual reveal.
    .opacity(hovering || focusedAction != nil ? 1 : 0)
  }

  private var deliberationFooter: some View {
    HStack(spacing: 12) {
      if let worked = message.workedLabel {
        Label(worked, systemImage: "clock")
          .font(.caption2)
          .foregroundStyle(.tertiary)
          .help("Time from your message to the finished answer")
      }
      if !message.answerReceiptID.isEmpty {
        Label("Receipt", systemImage: "checkmark.seal")
          .font(.caption2)
          .foregroundStyle(.tertiary)
          .help("Answer receipt: \(message.answerReceiptID)")
      }
      if let grounded = message.attachmentNames.first {
        Label(
          message.attachmentNames.count > 1 ? "Grounded on \(message.attachmentNames.count) files" : "Grounded on \(grounded)",
          systemImage: "doc.text.magnifyingglass"
        )
        .font(.caption2)
        .foregroundStyle(.tertiary)
        .help("This answer read: \(message.attachmentNames.joined(separator: ", "))")
      }
      if message.sensitive {
        Label("Private", systemImage: "lock.shield")
          .font(.caption2)
          .foregroundStyle(.tertiary)
          .help("Routed as sensitive: tagged private on the receipt and kept off frontier models.")
          // This is a status indicator, not a control. The old label
          // "Mark sensitive" announced it to VoiceOver as an action that
          // marks the answer sensitive; it only reports that this answer
          // already was routed privately.
          .accessibilityLabel("Routed as private, kept off frontier models")
      }
      if message.memorySourceCount > 0 {
        Button {
          store.select(.memory)
        } label: {
          Text("Used \(message.memorySourceCount) saved source\(message.memorySourceCount == 1 ? "" : "s")")
        }
          .buttonStyle(.plain)
          .font(.caption2)
          .foregroundStyle(.tertiary)
          .help("Open Memory to inspect the recall boundary")
      }
      // Surface trust signals only when something is OFF, not on every message.
      if !message.personaStatus.isEmpty && !message.personaMatched {
        Label("Off stance", systemImage: "exclamationmark.triangle")
          .font(.caption2)
          .foregroundStyle(.orange)
      }
      if !message.voiceStatus.isEmpty && !message.voiceInBounds {
        Label("Voice drift", systemImage: "waveform.badge.exclamationmark")
          .font(.caption2)
          .foregroundStyle(.orange)
      }
      Spacer(minLength: 8)
      actionButtons
    }
    .padding(.top, 2)
  }

  @ViewBuilder
  private var sourceDisclosure: some View {
    if !message.companySources.isEmpty || message.memorySourceCount > 0 || message.trustedSourceCount > 0 {
      DisclosureGroup("View sources") {
        VStack(alignment: .leading, spacing: 7) {
          if !message.companySources.isEmpty {
            Text("Company right now").font(.caption.weight(.semibold))
            ForEach(message.companySources, id: \.self) { source in
              Text("• \(source)").font(.caption).foregroundStyle(.secondary)
            }
          }
          if message.memorySourceCount > 0 {
            Text("Used \(message.memorySourceCount) saved source\(message.memorySourceCount == 1 ? "" : "s").")
              .font(.caption)
              .foregroundStyle(.secondary)
          }
          if message.trustedSourceCount > 0 {
            Text("Checked \(message.trustedSourceCount) trusted Pages source\(message.trustedSourceCount == 1 ? "" : "s").")
              .font(.caption)
              .foregroundStyle(.secondary)
          }
        }
        .padding(.top, 5)
      }
      .font(.caption)
      .foregroundStyle(.secondary)
    }
  }

  private var proposalActions: some View {
    Menu {
      Button("Send to Message") { store.prepareTwinMessage(message) }
      Button("Save as a Page") { store.prepareTwinPage(message) }
      Button("Turn into a Forge build") { store.prepareTwinForgeHandoff(message) }
    } label: {
      Label("Use this answer", systemImage: "arrowshape.turn.up.right")
    }
    .menuStyle(.borderlessButton)
    .controlSize(.small)
    .fixedSize()
    .help("Choose where to continue. MDx opens a draft and never sends without confirmation.")
    .padding(.top, 3)
  }
}

// MARK: - Markdown rendering

struct MarkdownText: View {
  let text: String

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      ForEach(Array(Self.segments(for: text).enumerated()), id: \.offset) { _, segment in
        switch segment {
        case .code(let language, let code):
          CodeBlock(language: language, code: code)
        case .heading(let level, let value):
          Text(value)
            .font(headingFont(level))
            .foregroundStyle(.primary)
            .fixedSize(horizontal: false, vertical: true)
            .textSelection(.enabled)
            .padding(.top, level == 1 ? 4 : 2)
        case .paragraph(let value):
          Text(value)
            .font(.body)
            .foregroundStyle(.primary)
            .fixedSize(horizontal: false, vertical: true)
            .textSelection(.enabled)
        case .listItem(let marker, let value):
          HStack(alignment: .firstTextBaseline, spacing: 9) {
            Text(marker)
              .font(.body.weight(.semibold))
              .foregroundStyle(.secondary)
              .frame(minWidth: 18, alignment: .trailing)
            Text(value)
              .font(.body)
              .fixedSize(horizontal: false, vertical: true)
              .textSelection(.enabled)
          }
        case .quote(let value):
          HStack(alignment: .top, spacing: 10) {
            RoundedRectangle(cornerRadius: 1)
              .fill(Color.accentColor.opacity(0.65))
              .frame(width: 3)
            Text(value)
              .font(.body)
              .foregroundStyle(.secondary)
              .fixedSize(horizontal: false, vertical: true)
              .textSelection(.enabled)
          }
          .padding(.vertical, 2)
        case .divider:
          Divider().padding(.vertical, 4)
        case .table(let headers, let rows):
          MarkdownTable(headers: headers, rows: rows)
        }
      }
    }
  }

  private func headingFont(_ level: Int) -> Font {
    switch level {
    case 1: return .title.bold()
    case 2: return .title2.bold()
    case 3: return .title3.weight(.semibold)
    default: return .headline
    }
  }

  enum Segment {
    // Inline markdown is rendered to AttributedString at parse time, so a
    // completed message never re-parses during unrelated re-renders.
    case heading(Int, AttributedString)
    case paragraph(AttributedString)
    case listItem(String, AttributedString)
    case quote(AttributedString)
    case divider
    case code(String, String)
    case table([AttributedString], [[AttributedString]])
  }

  // Parse cache keyed by source text. Messages re-render far more often than
  // their content changes (streaming siblings, selection, hover), and
  // AttributedString(markdown:) is too expensive for a body pass.
  @MainActor private static var cache: [String: [Segment]] = [:]

  @MainActor static func segments(for text: String) -> [Segment] {
    if let hit = cache[text] { return hit }
    let parsed = parse(text)
    if cache.count > 256 { cache.removeAll(keepingCapacity: true) }
    cache[text] = parsed
    return parsed
  }

  static func parse(_ text: String) -> [Segment] {
    var result: [Segment] = []
    var paragraph: [String] = []
    var code: [String] = []
    var codeLanguage = ""
    var insideCode = false

    func flushParagraph() {
      guard !paragraph.isEmpty else { return }
      result.append(.paragraph(inline(paragraph.joined(separator: "\n"))))
      paragraph.removeAll(keepingCapacity: true)
    }

    let lines = text.components(separatedBy: .newlines)
    var lineIndex = 0
    while lineIndex < lines.count {
      let rawLine = lines[lineIndex]
      if rawLine.hasPrefix("```") {
        if insideCode {
          result.append(.code(codeLanguage, code.joined(separator: "\n")))
          code.removeAll(keepingCapacity: true)
          codeLanguage = ""
        } else {
          flushParagraph()
          codeLanguage = String(rawLine.dropFirst(3)).trimmingCharacters(in: .whitespaces)
        }
        insideCode.toggle()
        lineIndex += 1
        continue
      }
      if insideCode {
        code.append(rawLine)
        lineIndex += 1
        continue
      }

      if lineIndex + 1 < lines.count,
         let headers = tableCells(from: rawLine),
         let separator = tableCells(from: lines[lineIndex + 1]),
         headers.count == separator.count,
         separator.allSatisfy(isTableSeparator) {
        flushParagraph()
        var rows: [[AttributedString]] = []
        lineIndex += 2
        while lineIndex < lines.count,
              let cells = tableCells(from: lines[lineIndex]),
              cells.count == headers.count {
          rows.append(cells.map(inline))
          lineIndex += 1
        }
        result.append(.table(headers.map(inline), rows))
        continue
      }

      let line = rawLine.trimmingCharacters(in: .whitespaces)
      if line.isEmpty {
        flushParagraph()
        lineIndex += 1
        continue
      }
      if let heading = heading(from: line) {
        flushParagraph()
        result.append(.heading(heading.level, inline(heading.text)))
      } else if ["---", "***", "___"].contains(line) {
        flushParagraph()
        result.append(.divider)
      } else if line.hasPrefix("> ") {
        flushParagraph()
        result.append(.quote(inline(String(line.dropFirst(2)))))
      } else if line.hasPrefix("- ") || line.hasPrefix("* ") || line.hasPrefix("+ ") {
        flushParagraph()
        result.append(.listItem("•", inline(String(line.dropFirst(2)))))
      } else if let ordered = orderedListItem(from: line) {
        flushParagraph()
        result.append(.listItem(ordered.marker, inline(ordered.text)))
      } else {
        paragraph.append(rawLine)
      }
      lineIndex += 1
    }
    if insideCode {
      result.append(.code(codeLanguage, code.joined(separator: "\n")))
    }
    flushParagraph()
    return result.isEmpty ? [.paragraph(inline(text))] : result
  }

  private static func heading(from line: String) -> (level: Int, text: String)? {
    let hashes = line.prefix { $0 == "#" }
    guard (1...6).contains(hashes.count), line.dropFirst(hashes.count).hasPrefix(" ") else { return nil }
    return (hashes.count, String(line.dropFirst(hashes.count + 1)))
  }

  private static func orderedListItem(from line: String) -> (marker: String, text: String)? {
    guard let dot = line.firstIndex(of: ".") else { return nil }
    let prefix = line[..<dot]
    guard !prefix.isEmpty, prefix.allSatisfy(\.isNumber) else { return nil }
    let afterDot = line.index(after: dot)
    guard afterDot < line.endIndex, line[afterDot] == " " else { return nil }
    return ("\(prefix).", String(line[line.index(after: afterDot)...]))
  }

  private static func tableCells(from line: String) -> [String]? {
    guard line.contains("|") else { return nil }
    var value = line.trimmingCharacters(in: .whitespaces)
    if value.hasPrefix("|") { value.removeFirst() }
    if value.hasSuffix("|") { value.removeLast() }
    let cells = value.split(separator: "|", omittingEmptySubsequences: false)
      .map { $0.trimmingCharacters(in: .whitespaces) }
    return cells.count >= 2 ? cells : nil
  }

  private static func isTableSeparator(_ cell: String) -> Bool {
    let value = cell.trimmingCharacters(in: .whitespaces)
      .trimmingCharacters(in: CharacterSet(charactersIn: ":"))
    return value.count >= 3 && value.allSatisfy { $0 == "-" }
  }

  private static func inline(_ value: String) -> AttributedString {
    (try? AttributedString(markdown: value, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(value)
  }
}

private struct MarkdownTable: View {
  let headers: [AttributedString]
  let rows: [[AttributedString]]

  var body: some View {
    VStack(spacing: 0) {
      tableRow(headers, emphasized: true)
      Divider()
      ForEach(rows.indices, id: \.self) { row in
        tableRow(rows[row], emphasized: false)
        Divider()
      }
    }
    .background(Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
    .overlay(RoundedRectangle(cornerRadius: 9, style: .continuous).stroke(Color.secondary.opacity(0.16)))
    .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
  }

  private func tableRow(_ values: [AttributedString], emphasized: Bool) -> some View {
    HStack(alignment: .top, spacing: 0) {
      ForEach(values.indices, id: \.self) { index in
        cell(values[index], emphasized: emphasized)
        if index < values.count - 1 {
          Divider()
        }
      }
    }
    .background(emphasized ? Color.primary.opacity(0.055) : Color.clear)
  }

  private func cell(_ value: AttributedString, emphasized: Bool) -> some View {
    Text(value)
      .font(emphasized ? .callout.weight(.semibold) : .callout)
      .foregroundStyle(emphasized ? .primary : .secondary)
      .fixedSize(horizontal: false, vertical: true)
      .textSelection(.enabled)
      .padding(.horizontal, 12)
      .padding(.vertical, 9)
      .frame(maxWidth: .infinity, alignment: .topLeading)
  }
}

struct CodeBlock: View {
  let language: String
  let code: String

  var body: some View {
    VStack(alignment: .leading, spacing: 0) {
      if !language.isEmpty {
        Text(language)
          .font(.caption2.monospaced())
          .foregroundStyle(.secondary)
          .padding(.horizontal, 12)
          .padding(.top, 8)
      }
      Text(code)
        .font(.system(.callout, design: .monospaced))
        .foregroundStyle(.primary)
        .textSelection(.enabled)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
    }
    .background(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .fill(Color.primary.opacity(0.05))
    )
    .overlay(
      RoundedRectangle(cornerRadius: 8, style: .continuous)
        .stroke(Color.secondary.opacity(0.12), lineWidth: 1)
    )
  }
}

struct TwinEmptyState: View {
  let use: (String) -> Void

  private let examples = [
    "Help me structure a hard decision",
    "Talk through an architecture tradeoff",
    "Draft a tricky message to my team"
  ]

  var body: some View {
    VStack(alignment: .leading, spacing: 14) {
      Image(systemName: "sparkles")
        .font(SurfaceType.emptyGlyph)
        .foregroundStyle(Color.accentColor)
      Text("Hey there.")
        .font(.title.weight(.semibold))
      Text("Twin is your private work surface. Bring a messy problem and think it through with MDx. Everything it uses, it cites, and it only acts after you approve.")
        .font(.title3)
        .foregroundStyle(.secondary)
        .fixedSize(horizontal: false, vertical: true)

      VStack(alignment: .leading, spacing: 8) {
        ForEach(examples, id: \.self) { example in
          Button {
            use(example)
          } label: {
            HStack(spacing: 8) {
              Image(systemName: "arrow.up.right")
                .font(.caption)
                .foregroundStyle(.secondary)
              Text(example)
                .font(.callout)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(
              RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(Color.primary.opacity(0.04))
            )
          }
          .buttonStyle(.plain)
        }
      }
      .padding(.top, 6)
    }
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}


/// The in-flight answer with a quietly blinking caret. TimelineView drives
/// the blink so the concatenated text stays a single wrapping run; Reduce
/// Motion holds the caret steady.
struct StreamingText: View {
  let answer: String
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    if reduceMotion {
      Text(answer) + Text(" \u{258D}").foregroundColor(.secondary)
    } else {
      TimelineView(.periodic(from: .now, by: 0.6)) { context in
        let on = Int(context.date.timeIntervalSinceReferenceDate / 0.6).isMultiple(of: 2)
        Text(answer) + Text(on ? " \u{258D}" : "  ").foregroundColor(.secondary)
      }
    }
  }
}
