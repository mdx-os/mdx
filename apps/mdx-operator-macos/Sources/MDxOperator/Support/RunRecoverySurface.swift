import Foundation

/// Pure recovery-surface mapping for RunDetailView's recovery banner.
/// Keeps proof caveat, branch identity, and revision controls readable for
/// failed Forge runs without depending on SwiftUI rendering.
struct RunRecoverySurface: Equatable {
  let title: String
  let recoveryLine: String
  let branchIdentity: String
  let revisionControlLabel: String
  let revisionControlSymbol: String
  let statusPillLabel: String
  let showsOpenDiff: Bool
  let showsRevisionControl: Bool
  let isProofCaveat: Bool
  let isBaselineRed: Bool
  let proofTurnedGreen: Bool
  let suggestedRevisionNote: String

  init(run: ForgeRun) {
    title = run.proofTroubleTitle
    isProofCaveat = run.isReviewableWithProofCaveat
    isBaselineRed = run.selectedProofRedOnArrival
    proofTurnedGreen = run.selectedProofTurnedGreen
    branchIdentity = run.hasBranch ? run.branch.trimmingCharacters(in: .whitespacesAndNewlines) : ""
    showsOpenDiff = run.hasBranch
    showsRevisionControl = !run.isRunning && (run.control("revise") != nil || run.hasBranch)
    statusPillLabel = run.isRunning ? "Still working" : run.displayStatus

    let detail = run.proofTroubleDetail
    if run.isRunning && run.selectedProofTurnedGreen {
      recoveryLine = "The selected check failed before the change and now passes. Forge is finishing the evidence packet."
    } else if run.isRunning {
      recoveryLine = "\(detail) Forge is still trying to work through it; steer only if the direction is wrong."
    } else if run.isReviewableWithProofCaveat && run.selectedProofTurnedGreen {
      recoveryLine = "\(detail) Review the diff and confirm the before-and-after proof matches the intended behavior. Request a revision only if it does not."
    } else if run.isReviewableWithProofCaveat {
      recoveryLine = "\(detail) Review the diff, then request a focused revision only if the branch caused or worsened the failure."
    } else if run.selectedProofRedOnArrival {
      recoveryLine = "\(detail) Treat this as baseline health first: isolate or repair the failing check, then return to the original ask."
    } else if run.hasBranch {
      recoveryLine = "\(detail) Review what it left on the branch, then ask for a focused revision."
    } else {
      recoveryLine = "\(detail) Start a narrower run so the next attempt has a smaller proof target."
    }

    if run.isReviewableWithProofCaveat && run.selectedProofTurnedGreen {
      revisionControlLabel = "Request revision"
      revisionControlSymbol = "arrow.triangle.branch"
      suggestedRevisionNote = "The selected proof turned green on this branch. Review whether the diff and before-and-after evidence match the intended behavior, then revise only what remains wrong: "
    } else if run.isReviewableWithProofCaveat {
      revisionControlLabel = "Request revision"
      revisionControlSymbol = "arrow.triangle.branch"
      suggestedRevisionNote = "Review this branch with the proof caveat in mind. If the failure is caused by this change, fix it; otherwise keep the change focused and note the existing red proof: "
    } else if run.selectedProofRedOnArrival {
      revisionControlLabel = "Repair baseline"
      revisionControlSymbol = "wrench.adjustable"
      suggestedRevisionNote = "Pick this back up by isolating the baseline failure first: "
    } else if run.checksFailed > 0 {
      revisionControlLabel = "Pick it back up"
      revisionControlSymbol = "arrow.triangle.branch"
      suggestedRevisionNote = "Pick this back up by focusing on the failing proof first: "
    } else {
      revisionControlLabel = "Pick it back up"
      revisionControlSymbol = "arrow.triangle.branch"
      suggestedRevisionNote = "Pick this back up with a narrower next step: "
    }
  }

  var keepsFailedBranchRecoveryReadable: Bool {
    guard !branchIdentity.isEmpty else { return false }
    guard showsRevisionControl, !revisionControlLabel.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
      return false
    }
    guard !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return false }
    guard !recoveryLine.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return false }
    if isProofCaveat {
      return (title.localizedCaseInsensitiveContains("proof caveat")
        || title.localizedCaseInsensitiveContains("turned green"))
        && recoveryLine.localizedCaseInsensitiveContains("revision")
        && revisionControlLabel == "Request revision"
    }
    return recoveryLine.localizedCaseInsensitiveContains("branch")
      || recoveryLine.localizedCaseInsensitiveContains("revision")
  }
}
