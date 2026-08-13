import SwiftUI

struct MessageView: View {
  @Environment(OperatorStore.self) private var store
  @State private var showRail = true

  var body: some View {
    HStack(spacing: 0) {
      if showRail {
        MessageRail()
          .frame(width: SurfaceMetrics.railWidth)
          .transition(.move(edge: .leading).combined(with: .opacity))
        Divider()
      }
      mainColumn
    }
    .task {
      if store.threadMessages.isEmpty && store.activityItems.isEmpty {
        await store.loadMessage()
      }
    }
  }

  private var mainColumn: some View {
    VStack(spacing: 0) {
      topBar
      Divider()
      Group {
        if store.snapshot.connectionStatus != .ok
          && store.threadMessages.isEmpty && store.activityItems.isEmpty && store.actionCards.isEmpty {
          offline
        } else if store.messagePhase == .loading
          && store.threadMessages.isEmpty && store.activityItems.isEmpty {
          loading
        } else if store.messageLane.isNeedsYou {
          MessageActionInbox()
        } else if store.messageLane.channelID != nil {
          MessageConversation()
        } else {
          MessageActivityFeed(items: store.visibleActivity)
        }
      }
      .frame(maxWidth: .infinity, maxHeight: .infinity)
      .transition(.opacity)
      .animation(.easeOut(duration: 0.15), value: store.messageLane)

      if store.messageLane.channelID != nil {
        Divider()
        MessageComposer()
      }
    }
  }

  private var topBar: some View {
    SurfaceTopBar(
      icon: store.messageLane.isNeedsYou ? "tray.full" : "bubble.left.and.bubble.right",
      title: laneTitle,
      subtitle: laneSubtitle,
      showRail: $showRail
    ) {
      if store.messageLane.channelID != nil {
        Button {
          withAnimation(.easeOut(duration: 0.15)) { store.messageCatchUpShown.toggle() }
        } label: {
          Label(store.messageCatchUpShown ? "Close catch-up" : "Catch up", systemImage: "sparkles")
        }
        .buttonStyle(.plain)
        .foregroundStyle(store.messageCatchUpShown ? Color.accentColor : .secondary)
        .keyboardShortcut("k", modifiers: [.command, .shift])
        .help("Summarize the recent conversation from the messages on this machine")
      }

      if store.activeNowCount > 0, store.messageLane.channelID != nil {
        Label("\(store.activeNowCount) active now", systemImage: "circle.fill")
          .font(.caption)
          .foregroundStyle(.secondary)
          .labelStyle(MessageActiveLabelStyle())
          .help(store.presencePeople.filter(\.active).prefix(8).map(\.displayName).joined(separator: ", "))
      }
    }
  }

  private var laneTitle: String {
    if store.messageLane.isNeedsYou { return "Needs you" }
    if let channel = store.selectedMessageChannel { return channel.displayName }
    if let area = store.messageLane.area,
       let match = store.activityAreas.first(where: { $0.id == area }) {
      return match.label
    }
    return "All activity"
  }

  private var laneSubtitle: String {
    if store.messageLane.isNeedsYou { return "decisions and requests waiting for you" }
    if let channel = store.selectedMessageChannel {
      if !channel.topic.isEmpty { return channel.topic }
      if channel.isDirect { return "private conversation" }
      return "people and agents in one conversation"
    }
    return "governed work from across MDx"
  }

  private var loading: some View {
    VStack(spacing: 8) {
      ProgressView().controlSize(.small)
      Text("Loading Message…").font(SurfaceType.callout).foregroundStyle(.secondary)
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
  }

  private var offline: some View {
    ContentUnavailableView {
      Label("Message is offline", systemImage: "bolt.horizontal.circle")
    } description: {
      Text("Start local MDx to open your conversations. A message only counts once the kernel records it.")
    } actions: {
      Button("Try again") { Task { await store.refresh() } }
        .buttonStyle(.borderedProminent)
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity)
  }
}

// MARK: - Rail

private struct MessageRail: View {
  @Environment(OperatorStore.self) private var store

  var body: some View {
    @Bindable var store = store
    VStack(spacing: 0) {
      RailHeader(title: "Message") {
        Button {
          Task { await store.loadMessage() }
        } label: {
          Image(systemName: "arrow.clockwise")
        }
        .buttonStyle(.plain)
        .foregroundStyle(.tertiary)
        .help("Refresh")
        .accessibilityLabel("Refresh Message")
      }

      SurfaceSearchField(placeholder: "Search messages", text: $store.messageSearch)
        .padding(.horizontal, 10)
        .padding(.bottom, 8)

      Divider()

      ScrollView {
        LazyVStack(alignment: .leading, spacing: 2) {
          MessageRailRow(
            title: "Needs you",
            systemImage: "tray.full",
            badge: store.actionCards.count,
            selected: store.messageLane.isNeedsYou
          ) { store.messageLane = .needsYou }

          MessageRailHeader(title: "Channels")

          if store.messageChannels.isEmpty {
            Text("Your first channel appears after its first recorded message.")
              .font(.caption)
              .foregroundStyle(.tertiary)
              .padding(.horizontal, 10)
              .padding(.vertical, 6)
          }

          ForEach(store.messageChannels) { channel in
            MessageRailRow(
              title: channel.displayName,
              systemImage: channel.isDirect ? "person.crop.circle" : "number",
              detail: channel.topic.isEmpty ? channel.description : channel.topic,
              badge: nil,
              selected: store.selectedMessageChannelID == channel.id
            ) { store.selectMessageChannel(channel.id) }
          }

          MessageRailHeader(title: "Activity")
          MessageRailRow(
            title: "All activity",
            systemImage: "dot.radiowaves.left.and.right",
            badge: nil,
            selected: store.messageLane == .activity(nil)
          ) { store.messageLane = .activity(nil) }

          ForEach(store.activityAreas) { area in
            MessageRailRow(
              title: area.label,
              systemImage: "waveform.path.ecg",
              badge: area.itemCount > 0 ? area.itemCount : nil,
              selected: store.messageLane == .activity(area.id),
              dimmed: area.itemCount == 0
            ) { store.messageLane = .activity(area.id) }
          }
        }
        .padding(8)
      }

      Divider()
      HStack(spacing: 6) {
        Image(systemName: "lock.shield")
        VStack(alignment: .leading, spacing: 1) {
          Text("Delivered on this Mac")
            .fontWeight(.medium)
          Text("External delivery remains closed")
        }
      }
      .font(.caption2)
      .foregroundStyle(.secondary)
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(10)
      .background(Color.primary.opacity(0.025))
    }
    .background(Color.primary.opacity(0.025))
  }
}

private struct MessageRailHeader: View {
  let title: String

  var body: some View {
    Text(title.uppercased())
      .font(.caption2.weight(.semibold))
      .foregroundStyle(.tertiary)
      .padding(.horizontal, 10)
      .padding(.top, 14)
      .padding(.bottom, 4)
  }
}

private struct MessageRailRow: View {
  let title: String
  let systemImage: String
  var detail: String = ""
  let badge: Int?
  let selected: Bool
  var dimmed: Bool = false
  let open: () -> Void
  @State private var hovering = false

  var body: some View {
    Button(action: open) {
      HStack(spacing: 8) {
        Image(systemName: systemImage)
          .font(.caption)
          .foregroundStyle(selected ? Color.accentColor : .secondary)
          .frame(width: 16)
        VStack(alignment: .leading, spacing: 1) {
          Text(title)
            .font(.subheadline)
            .foregroundStyle(dimmed ? .tertiary : .primary)
            .lineLimit(1)
          if !detail.isEmpty {
            Text(detail)
              .font(.caption2)
              .foregroundStyle(.tertiary)
              .lineLimit(1)
          }
        }
        Spacer(minLength: 4)
        if let badge, badge > 0 {
          Text(badge > 999 ? "999+" : "\(badge)")
            .font(.caption2.weight(.medium))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 6)
            .padding(.vertical, 1)
            .background(Capsule().fill(Color.primary.opacity(0.08)))
        }
      }
      .padding(.horizontal, 10)
      .padding(.vertical, detail.isEmpty ? 6 : 5)
      .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
      .background(
        RoundedRectangle(cornerRadius: 8, style: .continuous)
          .fill(selected ? Color.accentColor.opacity(0.14) : (hovering ? Color.primary.opacity(0.05) : Color.clear))
      )
    }
    .buttonStyle(.plain)
    .onHover { hovering = $0 }
  }
}

// MARK: - Conversation

private struct MessageConversation: View {
  @Environment(OperatorStore.self) private var store

  var body: some View {
    VStack(spacing: 0) {
      if store.messageCatchUpShown {
        MessageCatchUpCard(summary: store.messageCatchUp)
        Divider()
      }

      ScrollView {
        LazyVStack(alignment: .leading, spacing: 0) {
          if store.visibleThreadMessages.isEmpty {
            ContentUnavailableView {
              Label(store.messageSearch.isEmpty ? "Start the conversation" : "No matching messages", systemImage: "bubble.left")
            } description: {
              Text(store.messageSearch.isEmpty
                ? "Share an update, ask a teammate, or bring an agent into the room. Everything recorded here stays inspectable."
                : "Try another person or phrase. Search only reads messages already on this machine.")
            }
            .padding(.top, 70)
          }

          ForEach(store.visibleThreadMessages) { message in
            MessageConversationRow(message: message)
            Divider().padding(.leading, 58)
          }
        }
        .padding(.vertical, 12)
        .frame(maxWidth: SurfaceMetrics.streamWidth, alignment: .leading)
        .frame(maxWidth: .infinity)
      }
    }
  }
}

private struct MessageCatchUpCard: View {
  let summary: MessageCatchUp

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack(spacing: 8) {
        Image(systemName: "sparkles").foregroundStyle(Color.accentColor)
        Text("Catch-up").font(.subheadline.weight(.semibold))
        Spacer()
        Text("\(summary.messageCount) messages")
          .font(.caption)
          .foregroundStyle(.tertiary)
      }
      Text(summary.headline)
        .font(.callout)
        .foregroundStyle(.secondary)
      ForEach(Array(summary.highlights.enumerated()), id: \.offset) { _, line in
        Text(line)
          .font(.caption)
          .foregroundStyle(.secondary)
          .lineLimit(2)
      }
      Text("Composed on this device from the recent recorded messages. No model call or action ran.")
        .font(.caption2)
        .foregroundStyle(.tertiary)
    }
    .padding(14)
    .background(Color.accentColor.opacity(0.06))
  }
}

private struct MessageConversationRow: View {
  @Environment(OperatorStore.self) private var store
  let message: ThreadMessage
  @State private var hovering = false
  @State private var proofExpanded = false

  private var replies: [ThreadMessage] { store.replies(to: message) }
  private var reactions: [MessageReaction] { store.reactions(to: message) }
  private var threadOpen: Bool { store.openMessageThreadReceiptID == message.receiptID }

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      HStack(alignment: .top, spacing: 12) {
        MessageAvatar(name: message.actorName, kind: message.actorType)

        VStack(alignment: .leading, spacing: 5) {
          HStack(spacing: 6) {
            Text(message.actorName.capitalized)
              .font(.subheadline.weight(.semibold))
            if !message.isHuman {
              Text(message.actorType.capitalized)
                .font(.caption2.weight(.medium))
                .foregroundStyle(Color.accentColor)
                .padding(.horizontal, 6)
                .padding(.vertical, 1)
                .background(Capsule().fill(Color.accentColor.opacity(0.10)))
            }
            if !message.createdAt.isEmpty {
              Text(MessageTime.short(message.createdAt))
                .font(.caption2)
                .foregroundStyle(.tertiary)
            }
            Spacer(minLength: 4)
            if hovering {
              MessageRowActions(message: message, proofExpanded: $proofExpanded)
                .transition(.opacity)
            }
          }

          MarkdownText(text: message.body)
            .font(.callout)
            .textSelection(.enabled)

          if !reactions.isEmpty {
            HStack(spacing: 6) {
              ForEach(reactions, id: \.emoji) { reaction in
                Button("\(reaction.emoji) \(reaction.count)") { store.react(to: message, emoji: reaction.emoji) }
                  .buttonStyle(.bordered)
                  .controlSize(.mini)
                  .tint(reaction.mine ? Color.accentColor : nil)
                  .disabled(store.messageReactionInFlightReceiptID != nil)
              }
            }
          }

          if store.messageReactionResultParentReceiptID == message.receiptID,
             let result = store.messageReactionResult {
            Label(
              result.succeeded ? "Reaction recorded" : result.detail,
              systemImage: result.succeeded ? "checkmark.seal" : "exclamationmark.triangle"
            )
            .font(.caption)
            .foregroundStyle(result.succeeded ? Color.secondary : Color.orange)
          }

          if !replies.isEmpty && !threadOpen {
            Button("\(replies.count) \(replies.count == 1 ? "reply" : "replies")") {
              store.openMessageThreadReceiptID = message.receiptID
            }
            .buttonStyle(.plain)
            .font(.caption.weight(.medium))
            .foregroundStyle(Color.accentColor)
          }

          if proofExpanded {
            MessageProof(message: message)
          }
        }
      }

      if threadOpen {
        MessageThread(parent: message, replies: replies)
          .padding(.leading, 40)
      }
    }
    .padding(.horizontal, 16)
    .padding(.vertical, 11)
    .background(hovering ? Color.primary.opacity(0.025) : Color.clear)
    .onHover { hovering = $0 }
    .contextMenu {
      Button("Reply in thread") { store.openMessageThreadReceiptID = message.receiptID }
      Menu("React") {
        ForEach(["👍", "❤️", "✅", "👀"], id: \.self) { emoji in
          Button(emoji) { store.react(to: message, emoji: emoji) }
        }
      }
      Divider()
      Button("Show receipt") { proofExpanded = true }
    }
  }
}

private struct MessageRowActions: View {
  @Environment(OperatorStore.self) private var store
  let message: ThreadMessage
  @Binding var proofExpanded: Bool

  var body: some View {
    HStack(spacing: 4) {
      Button { store.react(to: message, emoji: "👍") } label: { Image(systemName: "hand.thumbsup") }
        .help("React")
      Button { store.openMessageThreadReceiptID = message.receiptID } label: { Image(systemName: "arrowshape.turn.up.left") }
        .help("Reply in thread")
      Button { proofExpanded.toggle() } label: { Image(systemName: "checkmark.seal") }
        .help("Show receipt")
    }
    .buttonStyle(.borderless)
    .font(.caption)
    .foregroundStyle(.secondary)
  }
}

private struct MessageThread: View {
  @Environment(OperatorStore.self) private var store
  let parent: ThreadMessage
  let replies: [ThreadMessage]

  var body: some View {
    @Bindable var store = store
    VStack(alignment: .leading, spacing: 8) {
      HStack {
        Text("Thread").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
        Spacer()
        Button { store.openMessageThreadReceiptID = nil } label: { Image(systemName: "xmark") }
          .buttonStyle(.plain)
          .foregroundStyle(.tertiary)
          .help("Close thread")
      }

      ForEach(replies) { reply in
        HStack(alignment: .top, spacing: 8) {
          MessageAvatar(name: reply.actorName, kind: reply.actorType, size: 24)
          VStack(alignment: .leading, spacing: 2) {
            Text(reply.actorName.capitalized).font(.caption.weight(.semibold))
            Text(reply.body).font(.callout).textSelection(.enabled)
          }
        }
      }

      HStack(alignment: .bottom, spacing: 8) {
        TextField("Reply…", text: $store.messageReplyDraft, axis: .vertical)
          .textFieldStyle(.plain)
          .lineLimit(1...4)
          .padding(8)
          .background(RoundedRectangle(cornerRadius: 8).fill(Color.primary.opacity(0.04)))
          .overlay(RoundedRectangle(cornerRadius: 8).stroke(Color.secondary.opacity(0.14)))
          .onSubmit { store.sendMessageReply(to: parent) }
        Button { store.sendMessageReply(to: parent) } label: { Image(systemName: "arrow.up") }
          .buttonStyle(.borderedProminent)
          .controlSize(.small)
          .disabled(store.messageReplyDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || store.messageReplyInFlight)
          .help("Send reply")
      }

      if let result = store.messageReplyResult, !result.succeeded {
        Label(result.detail, systemImage: "exclamationmark.triangle")
          .font(.caption)
          .foregroundStyle(.orange)
      }
    }
    .padding(12)
    .background(RoundedRectangle(cornerRadius: 10).fill(Color.primary.opacity(0.025)))
    .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color.secondary.opacity(0.12)))
  }
}

private struct MessageProof: View {
  let message: ThreadMessage

  var body: some View {
    VStack(alignment: .leading, spacing: 3) {
      if !message.receiptID.isEmpty {
        Text("Receipt \(message.receiptID)")
      }
      if !message.sourceReceiptID.isEmpty {
        Text("Source \(message.sourceReceiptID)")
      }
      if message.sequence > 0 {
        Text("Position #\(message.sequence)")
      }
    }
    .font(.caption2.monospaced())
    .foregroundStyle(.tertiary)
    .textSelection(.enabled)
  }
}

private struct MessageAvatar: View {
  let name: String
  let kind: String
  var size: CGFloat = 30

  var body: some View {
    ZStack {
      Circle().fill(kind == "agent" ? Color.accentColor.opacity(0.15) : Color.primary.opacity(0.08))
      if kind == "agent" {
        Image(systemName: "sparkle")
      } else {
        Text(String(name.prefix(1)).uppercased()).font(.caption.weight(.semibold))
      }
    }
    .frame(width: size, height: size)
    .foregroundStyle(kind == "agent" ? Color.accentColor : .secondary)
    .accessibilityHidden(true)
  }
}

// MARK: - Composer

private struct MessageComposer: View {
  @Environment(OperatorStore.self) private var store

  var body: some View {
    @Bindable var store = store
    let memoryWasRecorded = store.messageSendResult.map {
      $0.succeeded && $0.status != "QUEUED" && !$0.receiptID.isEmpty
    } ?? false
    VStack(alignment: .leading, spacing: 8) {
      if let result = store.messageSendResult {
        HStack(alignment: .top, spacing: 8) {
          Image(systemName: result.status == "QUEUED" ? "clock" : (result.succeeded ? "checkmark.seal" : "exclamationmark.triangle"))
            .foregroundStyle(result.status == "QUEUED" ? Color.secondary : (result.succeeded ? Color.green : Color.orange))
          VStack(alignment: .leading, spacing: 3) {
            Text(memoryWasRecorded
              ? "MDx noted this for the team. See it in Memory once a teammate clears it."
              : result.detail)
              .font(.caption)
              .foregroundStyle(.secondary)
              .lineLimit(2)
            if result.succeeded, !result.receiptID.isEmpty {
              Text(result.receiptID)
                .font(.caption2.monospaced())
                .foregroundStyle(.tertiary)
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)
            }
          }
          Spacer(minLength: 8)
          if memoryWasRecorded {
            Button("Open Memory") {
              store.select(.memory)
            }
            .buttonStyle(.plain)
            .foregroundStyle(Color.accentColor)
            .help("MDx noted this for the team. It becomes durable memory only after a teammate clears it.")
          }
        }
      }

      if !store.messageOutbox.isEmpty {
        Label("\(store.messageOutbox.count) waiting to send", systemImage: "tray.and.arrow.up")
          .font(.caption)
          .foregroundStyle(.secondary)
      }

      HStack(alignment: .bottom, spacing: 10) {
        TextField("Message \(store.selectedMessageChannel?.displayName ?? "this channel")", text: $store.messageDraft, axis: .vertical)
          .textFieldStyle(.plain)
          .lineLimit(1...5)
          .padding(.horizontal, 12)
          .padding(.vertical, 9)
          .background(RoundedRectangle(cornerRadius: 10).fill(Color.primary.opacity(0.045)))
          .overlay(RoundedRectangle(cornerRadius: 10).stroke(Color.secondary.opacity(0.14)))
          .onSubmit { store.sendMessageDraft() }

        Button { store.sendMessageDraft() } label: {
          Label("Send", systemImage: "paperplane.fill")
        }
        .buttonStyle(.borderedProminent)
        .keyboardShortcut(.return, modifiers: [.command])
        .disabled(store.messageDraft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || store.messageSendInFlight)
        .help("Send to Message")

        if store.messageSendInFlight { ProgressView().controlSize(.small) }
      }

      Text(store.snapshot.connectionStatus == .ok
        ? "Recorded locally with a receipt. External delivery remains closed until production is explicitly opened."
        : "Offline. Your words stay on this Mac and send after MDx reconnects.")
        .font(.caption2)
        .foregroundStyle(.tertiary)
    }
    .padding(.horizontal, 18)
    .padding(.vertical, 12)
    .background(.bar)
  }
}

// MARK: - Needs you and activity

private struct MessageActionInbox: View {
  @Environment(OperatorStore.self) private var store

  var body: some View {
    ScrollView {
      LazyVStack(alignment: .leading, spacing: 12) {
        if store.actionCards.isEmpty {
          ContentUnavailableView {
            Label("You are all caught up", systemImage: "checkmark.circle")
              .foregroundStyle(.green)
          } description: {
            Text("Approvals and handoffs that require a human decision show up here.")
          }
          .padding(.top, 60)
        }
        ForEach(store.actionCards) { card in
          MessageActionCard(card: card, inFlight: store.verdictInFlightCardID == card.id) { verdict in
            store.decideActionCard(card, verdict: verdict)
          }
        }
      }
      .padding(24)
      .frame(maxWidth: SurfaceMetrics.streamWidth, alignment: .leading)
      .frame(maxWidth: .infinity)
    }
  }
}

private struct MessageActionCard: View {
  let card: ActionCard
  let inFlight: Bool
  let decide: (String) -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack(spacing: 8) {
        Image(systemName: "checkmark.shield").font(.caption).foregroundStyle(Color.accentColor)
        Text(card.title).font(.headline)
        Spacer()
        if !card.sourceReceiptKind.isEmpty {
          Text(ReceiptKindLabel.short(card.sourceReceiptKind))
            .font(.caption2)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 7)
            .padding(.vertical, 2)
            .background(Capsule().fill(Color.primary.opacity(0.06)))
        }
      }
      if !card.summary.isEmpty {
        Text(card.summary).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
      }
      if !card.proposedAction.isEmpty {
        Label(card.proposedAction, systemImage: "arrow.turn.down.right")
          .font(.caption).foregroundStyle(.tertiary)
      }
      HStack(spacing: 10) {
        if !card.channelID.isEmpty {
          Label(humanLabel(card.channelID), systemImage: "number").font(.caption2).foregroundStyle(.tertiary)
        }
        Spacer()
        if inFlight {
          ProgressView().controlSize(.small)
        } else {
          if card.canReject { Button("Decline") { decide("reject") }.controlSize(.small) }
          if card.canApprove {
            Button("Approve") { decide("approve") }.controlSize(.small).buttonStyle(.borderedProminent)
          }
        }
      }
      Text("Nothing runs until you choose. Your decision is recorded with a receipt.")
        .font(.caption2).foregroundStyle(.tertiary)
    }
    .padding(16)
    .background(RoundedRectangle(cornerRadius: 12).fill(Color.primary.opacity(0.035)))
    .overlay(RoundedRectangle(cornerRadius: 12).stroke(Color.secondary.opacity(0.12)))
  }
}

private struct MessageActivityFeed: View {
  let items: [ActivityItem]

  var body: some View {
    ScrollView {
      LazyVStack(alignment: .leading, spacing: 0) {
        if items.isEmpty {
          ContentUnavailableView {
            Label("No activity here yet", systemImage: "dot.radiowaves.left.and.right")
          } description: {
            Text("Work from Forge, Twin, Pages and your team lands here with a receipt on every step.")
          }
          .padding(.top, 40)
        }
        ForEach(items) { item in
          HStack(alignment: .top, spacing: 12) {
            MessageAvatar(name: item.actor, kind: item.actorType, size: 28)
            VStack(alignment: .leading, spacing: 3) {
              HStack(spacing: 6) {
                Text(Page.humanActor(item.actor).capitalized).font(.subheadline.weight(.semibold))
                if !item.receiptKind.isEmpty {
                  Text(ReceiptKindLabel.short(item.receiptKind))
                    .font(.caption2).foregroundStyle(.secondary)
                    .padding(.horizontal, 6).padding(.vertical, 1)
                    .background(Capsule().fill(Color.primary.opacity(0.06)))
                }
              }
              if !item.detail.isEmpty {
                Text(item.detail).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
              }
            }
          }
          .padding(.horizontal, 16)
          .padding(.vertical, 9)
          Divider().padding(.leading, 44)
        }
      }
      .padding(.vertical, 8)
      .frame(maxWidth: SurfaceMetrics.streamWidth, alignment: .leading)
      .frame(maxWidth: .infinity)
    }
  }
}

private struct MessageActiveLabelStyle: LabelStyle {
  func makeBody(configuration: Configuration) -> some View {
    HStack(spacing: 5) {
      configuration.icon.font(.system(size: 7)).foregroundStyle(.green)
      configuration.title
    }
  }
}

private enum MessageTime {
  static func short(_ raw: String) -> String {
    guard let date = ISO8601DateFormatter().date(from: raw) else { return "" }
    let formatter = DateFormatter()
    formatter.dateStyle = Calendar.current.isDateInToday(date) ? .none : .short
    formatter.timeStyle = .short
    return formatter.string(from: date)
  }
}
