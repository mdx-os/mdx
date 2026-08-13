import Foundation

struct TwinStance: Identifiable, Hashable {
  let id: String
  let name: String
  let line: String

  static let all: [TwinStance] = [
    TwinStance(id: "auto", name: "Auto", line: "Routes to the right specialist"),
    TwinStance(id: "advisor", name: "Advisor", line: "Org design, domain modeling, strategy"),
    TwinStance(id: "coach", name: "Coach", line: "1:1s, feedback, difficult conversations"),
    TwinStance(id: "architect", name: "Architect", line: "Tech strategy, build vs buy, architecture"),
    TwinStance(id: "problem_solver", name: "Problem Solver", line: "First-principles breakdown of hard problems"),
    TwinStance(id: "compliance", name: "Compliance", line: "Regulatory guidance, policy, HIPAA")
  ]

  static func named(_ id: String) -> TwinStance {
    all.first { $0.id == id } ?? all[0]
  }
}

enum TwinStreamEvent {
  case delta(String)
  /// The streaming route intentionally delegates to the ordinary governed
  /// route when live provider gates are not open. The client must follow that
  /// contract instead of leaving an empty assistant turn behind.
  case fallback(String)
  case done(receiptID: String, model: String, firstTokenMs: Double, durationMs: Double, memorySourceCount: Int, trustedSourceCount: Int)
  case guarded(String)
  case failed(String)
}

/// Human-friendly relative time from an ISO-8601 string, for conversation rows.
enum RelativeTime {
  // ISO8601DateFormatter is expensive to construct and documented thread-safe,
  // so shared immutable instances are correct; nonisolated(unsafe) because the
  // type predates Sendable. These run inside view bodies and must be cached.
  private nonisolated(unsafe) static let fractionFormatter: ISO8601DateFormatter = {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return formatter
  }()
  private nonisolated(unsafe) static let plainFormatter = ISO8601DateFormatter()
  private static let monthDayFormatter: DateFormatter = {
    let formatter = DateFormatter()
    formatter.dateFormat = "MMM d"
    return formatter
  }()
  private static let yearFormatter: DateFormatter = {
    let formatter = DateFormatter()
    formatter.dateFormat = "MMM d, yyyy"
    return formatter
  }()

  static func date(from iso: String) -> Date? {
    fractionFormatter.date(from: iso) ?? plainFormatter.date(from: iso)
  }

  static func short(_ iso: String) -> String {
    guard let date = date(from: iso) else { return "" }
    return short(date)
  }

  static func short(_ date: Date) -> String {
    let seconds = Date().timeIntervalSince(date)
    if seconds < 60 { return "now" }
    if seconds < 3600 { return "\(Int(seconds / 60))m" }
    if seconds < 86_400 { return "\(Int(seconds / 3600))h" }
    if seconds < 604_800 { return "\(Int(seconds / 86_400))d" }
    if seconds < 2_629_800 { return "\(Int(seconds / 604_800))w" }
    return Calendar.current.isDate(date, equalTo: Date(), toGranularity: .year)
      ? monthDayFormatter.string(from: date)
      : yearFormatter.string(from: date)
  }

  /// Section bucket for grouping conversations by recency.
  static func bucket(_ iso: String) -> String {
    guard let date = date(from: iso) else { return "Earlier" }
    let seconds = Date().timeIntervalSince(date)
    if seconds < 86_400 { return "Today" }
    if seconds < 604_800 { return "Previous 7 days" }
    if seconds < 2_629_800 { return "Previous 30 days" }
    return "Earlier"
  }

  static let bucketOrder = ["Today", "Previous 7 days", "Previous 30 days", "Earlier"]
}

struct TwinAttachment: Identifiable, Equatable {
  let id: String
  let name: String
  let pageCount: Int
  let charCount: Int
  let parseState: String
  let extractedText: String
  /// The picked file on disk, kept so the chip can Quick Look it.
  var localURL: URL? = nil

  var pageLabel: String {
    switch pageCount {
    case 0: return "\(charCount) chars"
    case 1: return "1 page"
    default: return "\(pageCount) pages"
    }
  }
}

struct TwinConversation: Identifiable, Equatable {
  let id: String
  let title: String
  let snippet: String
  let lastTime: String
  let count: Int
}

struct TwinMessage: Identifiable, Equatable {
  let id: String
  let prompt: String
  var answer: String
  let stance: String
  let memoryScore: Int
  var memorySourceCount: Int = 0
  var trustedSourceCount: Int = 0
  var trustedSourceIDs: String = ""
  var companySources: [String] = []
  let personaStatus: String
  let voiceStatus: String
  let summary: String
  let createdAt: String
  var answerReceiptID: String
  var pending: Bool = false
  var attachmentNames: [String] = []
  var sensitive: Bool = false
  var durationMs: Double = 0

  /// "Worked for Ns" style trust chip (Codex pattern).
  var workedLabel: String? {
    guard durationMs > 0 else { return nil }
    let seconds = durationMs / 1000
    if seconds < 1 { return "Answered instantly" }
    if seconds < 60 { return String(format: "Answered in %.1fs", seconds) }
    return "Answered in \(Int(seconds / 60))m \(Int(seconds.truncatingRemainder(dividingBy: 60)))s"
  }

  var personaMatched: Bool { personaStatus.localizedCaseInsensitiveContains("matched") }
  var voiceInBounds: Bool { voiceStatus.localizedCaseInsensitiveContains("in_bounds") || voiceStatus.localizedCaseInsensitiveContains("in bounds") }
  var isModelUnavailablePlaceholder: Bool {
    answer.localizedCaseInsensitiveContains("no model is connected")
      && answer.localizedCaseInsensitiveContains("placeholder")
  }
  var messageDraft: String { "Twin on \"\(prompt.prefix(100))\":\n\n\(answer)" }

  static func pendingMessage(id: String, prompt: String, stance: String, attachmentNames: [String] = [], sensitive: Bool = false) -> TwinMessage {
    TwinMessage(
      id: id,
      prompt: prompt,
      answer: "",
      stance: stance,
      memoryScore: 0,
      personaStatus: "",
      voiceStatus: "",
      summary: "",
      createdAt: "",
      answerReceiptID: "",
      pending: true,
      attachmentNames: attachmentNames,
      sensitive: sensitive
    )
  }
}
