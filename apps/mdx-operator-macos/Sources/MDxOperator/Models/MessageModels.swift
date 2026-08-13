import Foundation

struct ActivityItem: Identifiable, Equatable {
  let id: String
  let area: String
  let channelID: String
  let actor: String
  let actorType: String
  let headline: String
  let detail: String
  let receiptKind: String
  let receiptID: String
  let seq: Int

  var isAgent: Bool { actorType == "agent" }
}

struct ActivityArea: Identifiable, Equatable {
  let id: String
  let label: String
  let itemCount: Int
}

struct ActionCard: Identifiable, Equatable {
  let id: String
  let requestID: String
  let title: String
  let summary: String
  let proposedAction: String
  let sourceReceiptKind: String
  let channelID: String
  let allowedVerdicts: [String]
  let gateState: String

  var canApprove: Bool { allowedVerdicts.contains("approve") }
  var canReject: Bool { allowedVerdicts.contains("reject") }
}

struct PresencePerson: Identifiable, Equatable {
  let id: String
  let displayName: String
  let actorType: String
  let lastAction: String
  let actionCount: Int
  let active: Bool

  var isAgent: Bool { actorType == "agent" }
}

struct MessageChannel: Identifiable, Equatable {
  let id: String
  let name: String
  let description: String
  let topic: String
  let kind: String
  let visibility: String
  let status: String
  let memberCount: Int

  var displayName: String { name.isEmpty ? humanLabel(id) : name }
  var isDirect: Bool { kind == "dm" }
}

struct ThreadMessage: Identifiable, Equatable {
  let id: String
  let messageID: String
  let threadID: String
  let channelID: String
  let actorID: String
  let actorName: String
  let actorType: String
  let body: String
  let contentType: String
  let contentPayload: String
  let receiptKind: String
  let receiptID: String
  let sourceReceiptID: String
  let sourceReceiptKind: String
  let sequence: Int
  let createdAt: String

  var isAgent: Bool { actorType == "agent" }
  var isHuman: Bool { actorType == "human" }
  var isReply: Bool { threadID.hasPrefix("reply:") }
  var parentReceiptID: String { isReply ? String(threadID.dropFirst("reply:".count)) : "" }
  var isReaction: Bool { threadID.hasPrefix("react:") }
  var reactionParentReceiptID: String { isReaction ? String(threadID.dropFirst("react:".count)) : "" }
  var reactionEmoji: String {
    guard isReaction, body.count > 2 else { return "" }
    return String(body.dropFirst(2))
  }
}

struct MessageReaction: Identifiable, Equatable {
  let emoji: String
  let count: Int
  let mine: Bool

  var id: String { emoji }

  static func fold(
    messages: [ThreadMessage],
    parentReceiptID: String,
    viewerActorID: String
  ) -> [MessageReaction] {
    var state: [String: [String: Bool]] = [:]
    for message in messages.sorted(by: { $0.sequence < $1.sequence })
    where message.reactionParentReceiptID == parentReceiptID {
      guard message.body.hasPrefix("+:") || message.body.hasPrefix("-:") else { continue }
      let emoji = message.reactionEmoji
      guard !emoji.isEmpty else { continue }
      let actor = message.actorID.isEmpty ? message.actorName : message.actorID
      state[emoji, default: [:]][actor] = message.body.hasPrefix("+:")
    }

    return state.compactMap { emoji, actors in
      let active = actors.filter(\.value).map(\.key)
      guard !active.isEmpty else { return nil }
      return MessageReaction(
        emoji: emoji,
        count: active.count,
        mine: active.contains { sameActor($0, viewerActorID) }
      )
    }
    .sorted { $0.emoji < $1.emoji }
  }

  private static func sameActor(_ left: String, _ right: String) -> Bool {
    if left == right { return true }
    let split = { (value: String) -> (kind: String, id: String) in
      let parts = value.split(separator: ":", maxSplits: 1).map(String.init)
      return parts.count == 2 ? (parts[0].lowercased(), parts[1]) : ("", value)
    }
    let leftActor = split(left)
    let rightActor = split(right)
    if !leftActor.kind.isEmpty && !rightActor.kind.isEmpty {
      return leftActor.kind == rightActor.kind && leftActor.id == rightActor.id
    }
    let typed = leftActor.kind.isEmpty ? rightActor : leftActor
    let plain = leftActor.kind.isEmpty ? leftActor : rightActor
    return ["human", "user", "owner"].contains(typed.kind) && typed.id == plain.id
  }
}

struct MessageCatchUp: Equatable {
  let headline: String
  let highlights: [String]
  let participantCount: Int
  let messageCount: Int
  let latestSequence: Int

  static let empty = MessageCatchUp(
    headline: "Nothing new to catch up on",
    highlights: [],
    participantCount: 0,
    messageCount: 0,
    latestSequence: 0
  )
}

struct QueuedThreadMessage: Codable, Identifiable, Equatable {
  let id: String
  let messageID: String?
  let channelID: String
  let body: String
  let queuedAt: Date

  var deliveryMessageID: String {
    messageID ?? "mac_outbox_\(id.replacingOccurrences(of: "-", with: "_"))"
  }
}

/// What the Message rail has selected.
enum MessageLane: Equatable {
  case needsYou
  case channel(String)
  case activity(String?)   // nil == all activity

  var isNeedsYou: Bool { if case .needsYou = self { return true }; return false }
  var area: String? { if case .activity(let a) = self { return a }; return nil }
  var channelID: String? { if case .channel(let id) = self { return id }; return nil }
}

enum ReceiptKindLabel {
  /// Turn "forge.run.event" into "Forge run" for a human-readable chip.
  static func short(_ kind: String) -> String {
    let parts = kind.split(separator: ".")
    guard parts.count >= 2 else { return kind.capitalized }
    let area = parts[0].capitalized
    let what = parts[1].replacingOccurrences(of: "_", with: " ")
    return "\(area) \(what)"
  }
}
