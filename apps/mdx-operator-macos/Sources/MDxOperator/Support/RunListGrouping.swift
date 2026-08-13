import Foundation

enum RunListGrouping {
  /// The run projection keeps one row per parallel candidate. The operator
  /// list keeps one build-level row and opens the primary into the grouped
  /// detail, while search can still match any candidate.
  static func representatives(
    visibleRuns: [ForgeRun],
    allRuns: [ForgeRun],
    groups: [ForgeParallelExecutionGroup]
  ) -> [ForgeRun] {
    var emittedGroupIDs = Set<String>()
    var output: [ForgeRun] = []
    for run in visibleRuns {
      guard let group = groups.first(where: { $0.isWide && $0.contains(runID: run.id) }) else {
        output.append(run)
        continue
      }
      guard !emittedGroupIDs.contains(group.id) else { continue }
      emittedGroupIDs.insert(group.id)
      output.append(allRuns.first(where: { $0.id == group.primaryRunID }) ?? run)
    }
    return output
  }
}
