import Foundation

enum MemoryRail: String, CaseIterable, Identifiable {
  case learned
  case recorded
  case modelProof

  var id: String { rawValue }

  var title: String {
    switch self {
    case .learned: "Lessons"
    case .recorded: "Team memory"
    case .modelProof: "Model evidence"
    }
  }

  var subtitle: String {
    switch self {
    case .learned: "What you approved to guide future work"
    case .recorded: "What MDx kept or is waiting to keep"
    case .modelProof: "How models performed by kind of work"
    }
  }

  var systemImage: String {
    switch self {
    case .learned: "checkmark.seal"
    case .recorded: "tray.full"
    case .modelProof: "chart.bar.xaxis"
    }
  }
}

struct MemoryWorkspace: Equatable {
  let lessons: [MemoryLesson]
  let retiredLessonCount: Int
  let surfaceGroups: [MemorySurfaceGroup]
  let modelProof: [MemoryModelProof]
  let unavailableRails: Set<MemoryRail>

  var pendingReviewCount: Int {
    surfaceGroups.flatMap(\.records).filter { $0.kind == .pending }.count
  }

  var isEmpty: Bool {
    lessons.isEmpty && surfaceGroups.allSatisfy { $0.records.isEmpty } && modelProof.isEmpty
  }

  func count(for rail: MemoryRail) -> Int {
    switch rail {
    case .learned: lessons.count
    case .recorded: surfaceGroups.flatMap(\.records).count
    case .modelProof: modelProof.count
    }
  }
}

struct MemoryLesson: Identifiable, Equatable {
  let id: String
  let activeMemoryID: String
  let summary: String
  let targetType: String
  let reviewOwner: String
  let activationBasis: String
  let rollbackPlan: String
  let promotionReceiptID: String
  let judgmentReceiptID: String
  let steersCasting: Bool
}

enum MemoryRecordKind: String, Equatable {
  case pending
  case kept
}

struct MemorySurfaceRecord: Identifiable, Equatable {
  let id: String
  let kind: MemoryRecordKind
  let content: String
  let proposedBy: String
  let reviewedBy: String
  let note: String
  let sourceReceiptID: String
  let gateReceiptID: String
  let decisionReceiptID: String
}

struct MemorySurfaceGroup: Identifiable, Equatable {
  let surface: String
  let records: [MemorySurfaceRecord]
  var id: String { surface }
}

struct MemoryModelProof: Identifiable, Equatable {
  let model: String
  let workType: String
  let runs: Int
  let done: Int
  let doneRate: Double
  let averageTurns: Double

  var id: String { "\(model)::\(workType)" }
}
