import Foundation

/// Contract checks for the JSON-to-model mappers, pinned against kernel
/// projections captured in Tests/Fixtures. The app's real contract is the
/// kernel wire shape; a mapping regression should fail here instead of
/// rendering as a silently empty surface.
///
/// Runs headless when the binary is launched with `--mapper-checks` and the
/// fixtures directory is provided via MDX_MAPPER_FIXTURES, then exits with a
/// non-zero status on any failure. Built into the app binary because the
/// Command Line Tools toolchain ships neither XCTest nor swift-testing, and
/// SwiftPM cannot link one executable target against another.
@MainActor
enum MapperChecks {
  private static var failures = 0

  static func runIfRequested() {
    guard CommandLine.arguments.contains("--mapper-checks") else { return }
    runAll()
  }

  private static func expect(_ condition: Bool, _ label: String) {
    if condition {
      print("ok   \(label)")
    } else {
      failures += 1
      print("FAIL \(label)")
    }
  }

  private static func fixture(_ name: String) -> JSONDictionary {
    guard let dir = ProcessInfo.processInfo.environment["MDX_MAPPER_FIXTURES"] else {
      failures += 1
      print("FAIL MDX_MAPPER_FIXTURES is not set")
      return [:]
    }
    let url = URL(fileURLWithPath: dir).appendingPathComponent("\(name).json")
    guard let data = try? Data(contentsOf: url),
          let object = try? JSONSerialization.jsonObject(with: data),
          let json = object as? JSONDictionary
    else {
      failures += 1
      print("FAIL fixture \(name).json missing or malformed at \(url.path)")
      return [:]
    }
    return json
  }

  private static func runAll() {
    checkForgeRuns()
    checkFleetPlans()
    checkStreamEvent()
    checkOutcomeClassification()
    checkTwin()
    checkActivityIdentity()
    checkMessageReactions()
    checkMachineRunners()
    checkAdmissionReconciliation()
    checkDiffParsing()
    checkHardeningHelpers()
    checkHelpers()
    checkScreenshotBuildRequest()
    checkScreenshotRunTarget()
    checkAppLifecycle()
    checkDiagnosticsAccounting()
    checkVisualQA()

    if failures > 0 {
      print("\n\(failures) mapper check(s) FAILED")
      exit(1)
    }
    print("\nAll mapper checks passed.")
    exit(0)
  }

  private static func checkMachineRunners() {
    let profiles: JSONDictionary = [
      "runner_profiles": [[
        "runner_id": "grok_build_cli_external_worker",
        "runner_kind": "external_worker",
        "display_name": "Grok Build",
        "adapter_kind": "grok_build_cli",
        "execution_status": "PROFILE_DECLARED",
        "model_profile_id": "grok_build_profile"
      ]]
    ]
    let scoreboard: JSONDictionary = [
      "runner_model_compatibility_matrix": [[
        "runner_id": "grok_build_cli_external_worker",
        "model_profile_id": "grok_build_profile",
        "model_display_name": "Grok 4.5",
        "adapter_protocol_tier": "protocol_adapter",
        "castable": true,
        "machine_option_enabled_by_provider_key": true
      ]],
      "runner_scores": [],
      "adapter_protocol_tiers": []
    ]

    let unchecked = MDxRouteClient.runners(
      from: profiles,
      scoreboardJSON: scoreboard,
      preflightJSON: [:],
      clearancesJSON: [:]
    ).first
    expect(unchecked?.runtimeChecked == false, "missing machine preflight maps to unchecked runtime")
    expect(unchecked?.runtimeStatusLabel == "Not verified", "unchecked machine does not claim its runtime is missing")
    expect(unchecked?.optionEnabled == true, "provider-key compatibility keeps a machine visible as an option")
    expect(unchecked?.readinessLine.contains("not been verified") == true, "unchecked machine explains it is not verified yet")

    let checked = MDxRouteClient.runners(
      from: profiles,
      scoreboardJSON: scoreboard,
      preflightJSON: [
        "checks": [[
          "runner_id": "grok_build_cli_external_worker",
          "binary_present": true,
          "status": "machine_option_ready",
          "version_observed": "grok 0.2.91"
        ]]
      ],
      clearancesJSON: [:]
    ).first
    expect(checked?.runtimeChecked == true, "returned machine preflight maps to checked runtime")
    expect(checked?.binaryPresent == true, "machine preflight preserves binary presence")
    expect(checked?.runtimeStatusLabel == "Installed, not verified", "installed machine distinguishes smoke readiness")

    let recoveredFaceOff = MDxRouteClient.recoveredFaceOffOutcome(
      from: [
        "face_offs": [[
          "runner_id": "claude_code_external_worker",
          "face_off_receipt_id": "face_off_receipt_new"
        ]]
      ],
      runnerID: "claude_code_external_worker",
      excluding: ["face_off_receipt_old"]
    )
    expect(recoveredFaceOff?.status == "FACE_OFF_STAGED_HELD", "timed-out face-off recovers a newly recorded held packet")
    expect(recoveredFaceOff?.receiptID == "face_off_receipt_new", "timed-out face-off preserves the recovered receipt")
    expect(
      MDxRouteClient.recoveredFaceOffOutcome(
        from: [
          "face_offs": [[
            "runner_id": "claude_code_external_worker",
            "face_off_receipt_id": "face_off_receipt_old"
          ]]
        ],
        runnerID: "claude_code_external_worker",
        excluding: ["face_off_receipt_old"]
      ) == nil,
      "timed-out face-off does not mistake old evidence for the current action"
    )
  }

  /// Pins the admission-class timeout recovery: a slow-but-successful kernel
  /// must reconcile against the projection instead of reporting failure and
  /// inviting a duplicate-minting retry. Receipts-or-it-did-not-happen holds,
  /// so recovery succeeds only when the projection actually shows the receipt.
  private static func checkAdmissionReconciliation() {
    // A newly admitted run with matching intent/repo counts as started.
    let startedRuns: JSONDictionary = [
      "runs": [
        ["run_id": "forge_run_old", "intent": "Add a health check endpoint", "repo": ["repo_id": "demo_weekly_digest"]],
        ["run_id": "forge_run_new", "intent": "Add a health check endpoint", "repo": ["repo_id": "demo_weekly_digest"]]
      ]
    ]
    let started = MDxRouteClient.recoveredStartedRun(
      from: startedRuns,
      intent: "Add a health check endpoint",
      repoID: "demo_weekly_digest",
      excluding: ["forge_run_old"]
    )
    expect(started?.receiptID == "forge_run_new", "timed-out start reconciles the newly admitted run")
    expect(started?.succeeded == true, "reconciled start with a run id counts as success")

    expect(
      MDxRouteClient.recoveredStartedRun(
        from: startedRuns,
        intent: "Add a health check endpoint",
        repoID: "demo_weekly_digest",
        excluding: ["forge_run_old", "forge_run_new"]
      ) == nil,
      "timed-out start does not mistake a pre-existing run for the new one"
    )
    expect(
      MDxRouteClient.recoveredStartedRun(
        from: startedRuns,
        intent: "A completely different ask",
        repoID: "demo_weekly_digest",
        excluding: ["forge_run_old"]
      ) == nil,
      "timed-out start does not reconcile an unrelated intent"
    )
    expect(
      MDxRouteClient.recoveredStartedRun(
        from: startedRuns,
        intent: "Add a health check endpoint",
        repoID: "some_other_repo",
        excluding: ["forge_run_old"]
      ) == nil,
      "timed-out start does not reconcile across a different repo"
    )

    // A ship decision recorded for this run counts as shipped.
    let shipDecisions: JSONDictionary = [
      "decisions": [
        ["run_id": "forge_run_new", "run_ship_decision_receipt_id": "ship_receipt_old"],
        ["run_id": "forge_run_new", "run_ship_decision_receipt_id": "ship_receipt_new"]
      ]
    ]
    let shipped = MDxRouteClient.recoveredShipDecision(
      from: shipDecisions,
      runID: "forge_run_new",
      excluding: ["ship_receipt_old"]
    )
    expect(shipped?.receiptID == "ship_receipt_new", "timed-out ship reconciles the newly recorded decision")
    expect(shipped?.succeeded == true, "reconciled ship with a receipt counts as success")
    expect(
      MDxRouteClient.recoveredShipDecision(
        from: shipDecisions,
        runID: "forge_run_new",
        excluding: ["ship_receipt_old", "ship_receipt_new"]
      ) == nil,
      "timed-out ship does not mistake a prior decision for the current one"
    )
    expect(
      MDxRouteClient.recoveredShipDecision(
        from: shipDecisions,
        runID: "forge_run_other",
        excluding: ["ship_receipt_old"]
      ) == nil,
      "timed-out ship does not reconcile a decision for a different run"
    )

    // Intent matching tolerates a trimmed echo but not a coincidence.
    expect(MDxRouteClient.intentReconciles("Add a health check endpoint", target: "Add a health check endpoint"), "exact intent reconciles")
    expect(MDxRouteClient.intentReconciles("Add a health check", target: "Add a health check endpoint"), "trimmed intent prefix reconciles")
    expect(!MDxRouteClient.intentReconciles("Fix", target: "Fix the bug"), "too-short intent does not reconcile on a coincidental prefix")
    expect(!MDxRouteClient.intentReconciles("Tidy a noisy log line", target: "Add a health check endpoint"), "unrelated intent does not reconcile")
  }

  private static func checkForgeRuns() {
    let runs = MDxRouteClient.forgeRuns(from: fixture("forge-runs"))
    expect(!runs.isEmpty, "forge-runs fixture parses to runs")
    expect(runs.allSatisfy { !$0.id.isEmpty }, "every run carries a run_id")
    expect(Set(runs.map(\.id)).count == runs.count, "run ids are unique")
    expect(runs.contains { !$0.events.isEmpty }, "at least one run embeds its event trail")
    expect(runs.contains { !$0.stages.isEmpty }, "at least one run embeds stages")

    let structured = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "forge_run_structured",
        "run_title": "Ship typed Forge projections",
        "intent": "structured run",
        "status": "running",
        "terminal_state": "IN_PROGRESS",
        "repo": ["repo_id": "mdx-native"],
        "model_or_worker": ["model_id": "gpt-5-codex"],
        "runner_profile": [
          "runner_id": "codex_cli_external_worker",
          "runner_kind": "codex_cli",
          "display_name": "Codex",
          "adapter_kind": "codex_cli",
          "execution_mode": "headless_cli_adapter",
          "model_profile_id": "codex_xai_responses_profile"
        ],
        "execution_geometry": [
          "requested_workers": 4,
          "effective_workers": 4,
          "lane": "bounded_parallel_exploration",
          "reason": "parallel candidates fit this work"
        ],
        "parallel_candidate": [
          "role": "primary",
          "index": 1,
          "count": 4,
          "strategy_summary": "smallest correct diff first"
        ],
        "work_classification": [
          "recommended_shape": "run",
          "task_class": "product_ux",
          "complexity_tier": "extreme",
          "confidence_pct": 82,
          "rationale": "matched a bounded native product work class"
        ],
        "language_task_alignment": [
          "task_corpus_id": "swift-spm-refactor-large",
          "status": "tier_fallback",
          "task_class": "refactor",
          "complexity_tier": "large",
          "visible_check": "make native-macos-operator-check",
          "hidden_check_slot": "native UX regression fixture"
        ],
        "quarantine": [
          "output_quarantined": true,
          "status": "output_quarantined_pending_mdx_gates",
          "blocked_reason": "pending_mdx_quality_gates"
        ],
        "context_telemetry": [
          "latest": ["input_tokens": 12_000, "output_tokens": 600, "context_window": 200_000, "pct": 6],
          "peak": ["input_tokens": 24_000, "context_window": 200_000, "pct": 12],
          "total": ["model_count": 2]
        ],
        "events": [[
          "event": "model_called",
          "detail": "model=gpt-5-codex input_tokens=12000"
        ]]
      ]]
    ])
    expect(structured.first?.repo == "mdx-native", "structured repo object maps to repo id")
    expect(structured.first?.title == "Ship typed Forge projections", "mission and run title maps from run_title")
    expect(structured.first?.terminalState == "IN_PROGRESS", "run terminal state maps from the typed field")
    expect(structured.first?.isRunning == true, "run activity reads terminal_state instead of event prose")
    expect(structured.first?.harness == "Codex", "structured runner profile maps to harness")
    expect(structured.first?.model == "gpt-5-codex", "structured model object maps to model id")
    expect(structured.first?.runner.protocolLabel == "CLI", "structured runner protocol maps to harness path")
    expect(structured.first?.execution.effectiveWorkers == 4, "structured execution geometry maps worker width")
    expect(structured.first?.parallelCandidate.laneLabel == "Primary 1/4", "parallel candidate maps lane label")
    expect(structured.first?.workClassification.chipLabel == "Product UX, Extreme", "work classification maps native chip label")
    expect(structured.first?.workClassification.detailLine.contains("82% confidence") == true, "work classification maps confidence")
    expect(structured.first?.languageTaskAlignment.chipLabel == "Refactor, Large", "language task alignment maps class and tier")
    expect(structured.first?.languageTaskAlignment.detailLine.contains("make native-macos-operator-check") == true, "language task alignment maps visible proof")
    expect(structured.first?.hasWorkIdentity == true, "classification and alignment keep the native work identity strip visible")
    expect(structured.first?.quarantine.outputHeld == true, "quarantine maps held output")
    expect(structured.first?.contextTelemetry.displayLabel == "peak 12%", "context telemetry maps peak label")
    expect(structured.first?.events.first?.model == "gpt-5-codex", "event model is parsed from event detail")
    expect(
      ForgeNextMove.derive(runs: structured, connected: true).kind == .follow,
      "native Forge next move follows a live run before offering new work"
    )
    expect(
      ForgeNextMove.derive(runs: [], connected: true).kind == .start,
      "native Forge next move keeps start primary when no run needs attention"
    )

    let reviewable = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "run_ready_for_review",
        "run_title": "Repair session refresh",
        "status": "done",
        "terminal_state": "SUCCEEDED",
        "branch": "forge/session-refresh",
        "controls": [["action": "review", "allowed": true, "route": "/forge/review"]]
      ]]
    ])
    let reviewMove = ForgeNextMove.derive(runs: reviewable + structured, connected: true)
    expect(reviewMove.kind == .review, "native Forge next move puts review before active work")
    expect(reviewMove.runID == "run_ready_for_review", "native Forge next move preserves the review run identity")

    let stopped = MDxRouteClient.forgeRuns(from: [
      "runs": [["run_id": "run_stopped", "status": "cannot_proceed"]]
    ])
    expect(
      ForgeNextMove.derive(runs: reviewable + stopped, connected: true).kind == .review,
      "native Forge next move puts review before ordinary recovery"
    )

    let projectedDecisions = MDxRouteClient.forgeDecisions(from: [
      "needs_you": [
        ["id": "ship_call:run:run_ready_for_review", "kind": "ship_call", "label": "Review finished run", "line": "A run waits for your call.", "href": "/forge/review", "subject": "run_id=run_ready_for_review", "subject_ref": ["type": "run", "id": "run_ready_for_review"], "priority": 40, "primary_action": ["state": "review_required", "blocker_codes": [], "evidence_receipt_ids": ["receipt_check", "receipt_finish"]]],
        ["id": "ratify_plan:fleet:fleet_one", "kind": "ratify_plan", "label": "Approve fleet split", "line": "Workers wait for approval.", "href": "/forge/fleets", "subject": "fleet_id=fleet_one", "subject_ref": ["type": "fleet", "id": "fleet_one"], "priority": 30, "primary_action": ["state": "blocked", "blocker_codes": ["missing_proof"], "evidence_receipt_ids": ["receipt_plan"]]]
      ]
    ])
    let decisionMove = ForgeNextMove.derive(decisions: projectedDecisions, runs: reviewable, connected: true)
    expect(decisionMove.kind == .decision, "native Forge next move puts a projected decision before review")
    expect(decisionMove.destination == .fleet, "native Forge decision preserves its owning lane")
    expect(decisionMove.additionalWaiting == 1, "native Forge decision keeps the remaining queue visible")
    expect(projectedDecisions.last?.priority == 30, "native Forge decision maps server-owned priority")
    expect(projectedDecisions.last?.actionState == "blocked", "native Forge decision maps projected action state")
    expect(projectedDecisions.last?.blockerCodes == ["missing_proof"], "native Forge decision maps projected blockers")
    expect(projectedDecisions.first?.evidenceReceiptIDs.count == 2, "native Forge decision preserves evidence receipts")

    let fleetRepairLaneRun = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "run_fleet_orchestrator_repair",
        "intent": "fleet repair lane",
        "execution_geometry": [
          "requested_workers": 1,
          "effective_workers": 1,
          "lane": "fleet_orchestrator_repair",
          "reason": "repair attention lanes before failing fleet"
        ]
      ]]
    ])
    expect(
      fleetRepairLaneRun.first?.execution.laneLabel == "Fleet Orchestrator Repair",
      "fleet_orchestrator_repair maps to a human-readable native lane label"
    )

    let localSnapshotRun = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "run_local_snapshot",
        "intent": "local snapshot truth",
        "events": [[
          "event": "evidence",
          "detail": "local_dirty_base_snapshot applied tracked_files=36 untracked_files=2 commit_sha=96427a101a5 untracked_included=true live_repo_mutated=false"
        ]]
      ]]
    ])
    expect(localSnapshotRun.first?.localBaseSnapshot?.trackedFileCount == 36, "local dirty snapshot maps tracked file count")
    expect(localSnapshotRun.first?.localBaseSnapshot?.untrackedFileCount == 2, "local dirty snapshot maps untracked file count")
    expect(localSnapshotRun.first?.localBaseSnapshot?.includesUntracked == true, "local dirty snapshot keeps untracked boundary")
    expect(localSnapshotRun.first?.localBaseSnapshot?.liveRepoMutated == false, "local dirty snapshot shows live checkout was not mutated")
    expect(localSnapshotRun.first?.localBaseSnapshot?.chipLabel == "38 local files", "local dirty snapshot chip includes tracked and untracked files")
    expect(localSnapshotRun.first?.localBaseSnapshot?.detailLine.contains("2 untracked files included") == true, "local dirty snapshot detail names included untracked files")
    expect(localSnapshotRun.first?.localBaseSnapshot?.detailLine.contains("snapshot 96427a1") == true, "local dirty snapshot shows short commit")

    let oneTrackedFileSnapshot = ForgeLocalBaseSnapshot(event: ForgeEvent(
      id: "local_one_file",
      seq: 1,
      kind: "evidence",
      stage: "work",
      summary: "",
      detail: "local_dirty_base_snapshot applied tracked_files=1 untracked_files=0 commit_sha=abc1234 untracked_included=false live_repo_mutated=false",
      receiptID: "",
      receiptRoute: "",
      model: ""
    ))
    expect(
      oneTrackedFileSnapshot?.shortLine == "Using 1 local file from this Mac",
      "local dirty snapshot shortLine pluralizes one local file"
    )

    let observedModelRun = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "run_observed_model",
        "intent": "model truth",
        "model_or_worker": ["model_id": "gpt-5-mini"],
        "context_telemetry": [
          "latest": ["model_id": "gpt-5-mini", "input_tokens": 500, "context_window": 272_000, "pct": 1],
          "peak": ["model_id": "grok-4.5", "input_tokens": 24_000, "context_window": 256_000, "pct": 10],
          "total": ["model_count": 2],
          "models": [
            ["model_id": "gpt-5-mini"],
            ["model_id": "grok-4.5"]
          ]
        ],
        "events": [[
          "event": "model_called",
          "detail": "model=grok-4.5 finish_reason=tool_calls"
        ]]
      ]]
    ])
    expect(observedModelRun.first?.model == "grok-4.5 +1", "observed execution model beats configured profile in run chip")

    let pendingModelRun = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "run_pending_model",
        "intent": "model pending",
        "model_or_worker": ["provider_family": "", "model_id": ""],
        "runner_profile": [
          "display_name": "Forge native builder",
          "model_profile_id": "mdx_native_local_model_profile"
        ]
      ]]
    ])
    expect(pendingModelRun.first?.model == "", "internal model profile ids do not render as the running model")

    let redProofRun = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "forge_red_proof",
        "status": "running",
        "checks_passed": 1,
        "checks_failed": 1,
        "events": [[
          "event": "check_failed",
          "stage": "proof",
          "detail": "run_command pnpm test exit=1 tail=expected failure"
        ]]
      ]]
    ])
    expect(redProofRun.first?.hasProofTrouble == true, "red proof run shows recovery state")
    expect(redProofRun.first?.proofTroubleDetail.contains("pnpm test") == true, "red proof detail names failing command")
    expect(redProofRun.first?.selectedProofRedOnArrival == false, "ordinary red proof is not treated as baseline red")

    let baselineRedRun = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "forge_baseline_red",
        "status": "no_change",
        "checks_passed": 2,
        "checks_failed": 2,
        "stages": [
          ["key": "intake", "label": "Reading the repo", "state": "done"],
          ["key": "proof", "label": "Proving", "state": "done"],
          ["key": "done", "label": "Done", "state": "done"]
        ],
        "events": [
          [
            "event": "check_failed",
            "stage": "proof",
            "detail": "baseline_run_command pnpm test exit=1 tail=empty"
          ],
          [
            "event": "evidence_appended",
            "detail": "selected proof is red on arrival before model turns: `pnpm test` exited 1 after 100 ms. This baseline was already red before your ask, not caused by it."
          ]
        ]
      ]]
    ])
    expect(baselineRedRun.first?.selectedProofRedOnArrival == true, "baseline red proof is detected from run events")
    expect(baselineRedRun.first?.proofTroubleTitle.contains("Proof started red") == true, "baseline red proof gets a neutral recovery title")
    expect(baselineRedRun.first?.proofTroubleDetail.contains("already red before this run") == true, "baseline red detail names pre-existing proof")
    expect(baselineRedRun.first?.proofTroubleDetail.contains("pnpm test failed") == true, "baseline red detail uses a clean command label")
    if let baselineRed = baselineRedRun.first {
      expect(baselineRed.stages.first { $0.id == "proof" }?.needsAttention(in: baselineRed) == true, "baseline red proof marks proof stage for attention")
      expect(baselineRed.stages.first { $0.id == "done" }?.needsAttention(in: baselineRed) == true, "baseline red proof marks terminal stage for attention")
    }

    let reviewableRedProofRun = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "forge_reviewable_red_proof",
        "status": "done",
        "stage": "ready_for_review",
        "branch": "forge/run-forge_reviewable_red_proof-mdx_ws_123",
        "checks_passed": 0,
        "checks_failed": 3,
        "events": [
          [
            "event": "check_failed",
            "stage": "proof",
            "detail": "baseline_run_command pnpm test exit=1 tail=empty"
          ],
          [
            "event": "evidence_appended",
            "stage": "done",
            "summary": "pre-existing selected proof stayed red after a reviewable branch landed"
          ]
        ]
      ]]
    ])
    expect(reviewableRedProofRun.first?.isReviewableWithProofCaveat == true, "reviewable red proof branch maps to proof caveat state")
    expect(reviewableRedProofRun.first?.proofTroubleTitle.contains("proof caveat") == true, "reviewable red proof branch gets caveat title")
    expect(reviewableRedProofRun.first?.proofTroubleDetail.contains("cannot") == false, "reviewable red proof branch does not echo hard stop copy")
    if let reviewableRun = reviewableRedProofRun.first {
      let surface = RunRecoverySurface(run: reviewableRun)
      expect(surface.isProofCaveat == true, "recovery surface marks proof caveat for reviewable red branch")
      expect(surface.branchIdentity == "forge/run-forge_reviewable_red_proof-mdx_ws_123", "recovery surface keeps branch identity readable")
      expect(surface.showsOpenDiff == true, "recovery surface keeps open-diff control for branched failure")
      expect(surface.showsRevisionControl == true, "recovery surface keeps revision control for branched failure")
      expect(surface.revisionControlLabel == "Request revision", "recovery surface keeps revision control label readable")
      expect(surface.title.localizedCaseInsensitiveContains("proof caveat"), "recovery surface title keeps proof caveat readable")
      expect(surface.recoveryLine.localizedCaseInsensitiveContains("revision"), "recovery surface recovery line keeps revision guidance readable")
      expect(surface.suggestedRevisionNote.localizedCaseInsensitiveContains("proof caveat"), "recovery surface revision note keeps proof caveat guidance")
      expect(surface.keepsFailedBranchRecoveryReadable == true, "failed Forge run with branch keeps caveat, branch, and revision controls readable")
    }

    let proofTurnedGreenRun = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "forge_proof_turned_green",
        "status": "done",
        "stage": "ready_for_review",
        "branch": "forge/run-forge_proof_turned_green-mdx_ws_789",
        "checks_passed": 1,
        "checks_failed": 1,
        "events": [
          [
            "event": "check_failed",
            "stage": "proof",
            "detail": "baseline_run_command grep -q Troubleshooting README.md exit=1 tail=empty"
          ],
          [
            "event": "proof_output",
            "stage": "proof",
            "summary": "Proof check passed",
            "detail": "run_command grep -q Troubleshooting README.md exit=0 tail=empty"
          ]
        ]
      ]]
    ])
    expect(proofTurnedGreenRun.first?.selectedProofTurnedGreen == true, "task proof can start red and turn green")
    expect(proofTurnedGreenRun.first?.proofTroubleTitle.contains("turned green") == true, "green turnaround is not mislabeled as broken baseline health")
    if let turnedGreen = proofTurnedGreenRun.first {
      let surface = RunRecoverySurface(run: turnedGreen)
      expect(surface.proofTurnedGreen == true, "recovery surface preserves proof turnaround")
      expect(surface.recoveryLine.contains("before-and-after proof"), "proof turnaround asks for evidence review")
      expect(surface.keepsFailedBranchRecoveryReadable == true, "proof turnaround keeps review controls readable")
    }

    let failedBranchRun = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "forge_failed_with_branch",
        "status": "cannot_proceed",
        "stage": "done",
        "branch": "forge/run-forge_failed_with_branch-mdx_ws_456",
        "checks_passed": 1,
        "checks_failed": 2,
        "events": [[
          "event": "check_failed",
          "stage": "proof",
          "detail": "run_command make native-macos-operator-check exit=1 tail=expected failure"
        ]]
      ]]
    ])
    if let failedBranch = failedBranchRun.first {
      let surface = RunRecoverySurface(run: failedBranch)
      expect(surface.branchIdentity == "forge/run-forge_failed_with_branch-mdx_ws_456", "failed branch recovery keeps branch identity")
      expect(surface.showsRevisionControl == true, "failed branch recovery keeps revision control")
      expect(!surface.revisionControlLabel.isEmpty, "failed branch recovery keeps revision label readable")
      expect(surface.recoveryLine.localizedCaseInsensitiveContains("branch") || surface.recoveryLine.localizedCaseInsensitiveContains("revision"), "failed branch recovery line keeps branch or revision guidance")
      expect(surface.keepsFailedBranchRecoveryReadable == true, "failed Forge run with branch keeps recovery surface readable")
    }

    let width2Runs = MDxRouteClient.forgeRuns(from: [
      "runs": [
        [
          "run_id": "forge_width2_primary",
          "intent": "width-2 fleet dogfood primary",
          "status": "running",
          "branch": "forge/run-forge_width2_primary-mdx_ws_w2",
          "model_or_worker": ["model_id": "grok-4.5"],
          "execution_geometry": [
            "requested_workers": 2,
            "effective_workers": 2,
            "lane": "bounded_parallel_exploration",
            "reason": "width-2 parallel candidates fit this work"
          ],
          "parallel_candidate": [
            "role": "primary",
            "index": 1,
            "count": 2,
            "strategy_summary": "minimal_safe_patch",
            "proof_bias": "minimal_diff_with_behavioral_check"
          ]
        ],
        [
          "run_id": "forge_width2_candidate",
          "intent": "width-2 fleet dogfood candidate",
          "status": "cannot_proceed",
          "stage": "done",
          "branch": "forge/run-forge_width2_candidate-mdx_ws_w2",
          "checks_passed": 0,
          "checks_failed": 1,
          "execution_geometry": [
            "requested_workers": 2,
            "effective_workers": 2,
            "lane": "bounded_parallel_exploration",
            "reason": "width-2 parallel candidates fit this work"
          ],
          "parallel_candidate": [
            "role": "candidate",
            "index": 2,
            "count": 2,
            "strategy_summary": "broader exploration",
            "proof_bias": "semantic_orientation_first"
          ],
          "events": [[
            "event": "check_failed",
            "stage": "proof",
            "detail": "run_command make native-macos-operator-check exit=1 tail=expected width-2 failure"
          ]]
        ]
      ]
    ])
    expect(width2Runs.count == 2, "width-2 Forge run maps both parallel workers")
    if let primary = width2Runs.first(where: { $0.id == "forge_width2_primary" }),
       let candidate = width2Runs.first(where: { $0.id == "forge_width2_candidate" }) {
      expect(primary.execution.requestedWorkers == 2, "width-2 primary keeps requested worker width")
      expect(primary.execution.effectiveWorkers == 2, "width-2 primary keeps effective worker width")
      expect(primary.execution.isWide == true, "width-2 primary is wide execution")
      expect(primary.execution.laneLabel == "2 workers", "width-2 primary keeps worker width readable")
      expect(primary.parallelCandidate.isParallel == true, "width-2 primary is a parallel candidate")
      expect(primary.parallelCandidate.laneLabel == "Primary 1/2", "width-2 primary role identity stays readable")
      expect(candidate.execution.effectiveWorkers == 2, "width-2 candidate keeps effective worker width")
      expect(candidate.execution.laneLabel == "2 workers", "width-2 candidate keeps worker width readable")
      expect(candidate.parallelCandidate.laneLabel == "Candidate 2/2", "width-2 candidate role identity stays readable")
      expect(candidate.hasBranch == true, "width-2 failed candidate keeps branch identity")
      expect(candidate.hasProofTrouble == true, "width-2 failed candidate surfaces proof trouble")

      let surface = RunRecoverySurface(run: candidate)
      expect(surface.branchIdentity == "forge/run-forge_width2_candidate-mdx_ws_w2", "width-2 failed branch keeps branch identity readable")
      expect(surface.showsRevisionControl == true, "width-2 failed branch keeps revision control")
      expect(!surface.revisionControlLabel.isEmpty, "width-2 failed branch keeps revision label readable")
      expect(surface.recoveryLine.localizedCaseInsensitiveContains("branch") || surface.recoveryLine.localizedCaseInsensitiveContains("revision"), "width-2 failed recovery line keeps branch or revision guidance")
      expect(surface.keepsFailedBranchRecoveryReadable == true, "width-2 failed branch recovery stays readable")
    }

    let width2GroupEnvelope: JSONDictionary = [
      "parallel_execution_groups": [[
        "primary_run_id": "forge_width2_primary",
        "requested_workers": 2,
        "effective_workers": 2,
        "lane": "bounded_parallel_exploration",
        "planned_candidate_count": 2,
        "observed_candidate_count": 2,
        "finished_candidate_count": 1,
        "running_candidate_count": 1,
        "done_candidate_count": 1,
        "failed_candidate_count": 0,
        "checks_passed_total": 2,
        "checks_failed_total": 0,
        "selection_status": "waiting_for_candidates",
        "recommended_run_id": "forge_width2_primary",
        "recommended_strategy_summary": "minimal safe patch",
        "candidates": [
          [
            "run_id": "forge_width2_primary",
            "role": "primary",
            "index": 1,
            "count": 2,
            "status": "done",
            "finished": true,
            "strategy_summary": "minimal safe patch",
            "checks_passed": 2,
            "checks_failed": 0,
            "turns": 11,
            "branch": "forge/run-forge_width2_primary"
          ],
          [
            "run_id": "forge_width2_candidate",
            "role": "candidate",
            "index": 2,
            "count": 2,
            "status": "running",
            "finished": false,
            "strategy_summary": "behavior proof",
            "checks_passed": 1,
            "checks_failed": 0,
            "turns": 26,
            "branch": ""
          ]
        ]
      ]]
    ]
    let width2Groups = MDxRouteClient.parallelExecutionGroups(from: width2GroupEnvelope)
    expect(width2Groups.count == 1, "width-2 parallel group maps from run projection")
    if let group = width2Groups.first {
      expect(group.effectiveWorkers == 2, "width-2 parallel group keeps effective worker count")
      expect(group.laneLabel == "2 lanes", "width-2 parallel group keeps native lane label")
      expect(group.progressLine == "1 finished, 1 still working", "width-2 mixed lane progress stays readable")
      expect(group.contains(runID: "forge_width2_candidate") == true, "width-2 parallel group can find candidate lane")
      expect(group.candidate(runID: "forge_width2_primary")?.laneLabel == "Primary 1/2", "width-2 group keeps primary lane label")
      expect(group.candidate(runID: "forge_width2_candidate")?.displayStatus == "Working", "width-2 group keeps running lane status")
      let representatives = RunListGrouping.representatives(
        visibleRuns: width2Runs,
        allRuns: width2Runs,
        groups: width2Groups
      )
      expect(representatives.map(\.id) == ["forge_width2_primary"], "parallel candidates collapse to one build-level run row")
      let candidateSearchRepresentatives = RunListGrouping.representatives(
        visibleRuns: width2Runs.filter { $0.id == "forge_width2_candidate" },
        allRuns: width2Runs,
        groups: width2Groups
      )
      expect(candidateSearchRepresentatives.map(\.id) == ["forge_width2_primary"], "candidate search still opens the grouped primary run")
      expect(group.selectionLine.localizedCaseInsensitiveContains("recommended: Primary 1/2"), "width-2 group names recommended lane")
    }

    let settledGroupEnvelope: JSONDictionary = [
      "parallel_execution_groups": [[
        "primary_run_id": "forge_width4_primary",
        "requested_workers": 4,
        "effective_workers": 4,
        "lane": "bounded_parallel_exploration",
        "planned_candidate_count": 4,
        "observed_candidate_count": 4,
        "finished_candidate_count": 4,
        "running_candidate_count": 0,
        "done_candidate_count": 3,
        "no_change_candidate_count": 1,
        "cannot_proceed_candidate_count": 0,
        "failed_candidate_count": 0,
        "checks_passed_total": 9,
        "checks_failed_total": 1,
        "selection_status": "ready_for_review",
        "recommended_run_id": "forge_width4_primary",
        "recommended_strategy_summary": "smallest correct diff with the lowest integration risk",
        "candidates": [
          [
            "run_id": "forge_width4_primary",
            "role": "primary",
            "index": 1,
            "count": 4,
            "status": "done",
            "finished": true,
            "strategy_summary": "smallest diff",
            "checks_passed": 2,
            "checks_failed": 0,
            "turns": 15,
            "branch": "forge/run-forge_width4_primary"
          ],
          [
            "run_id": "forge_width4_no_change",
            "role": "candidate",
            "index": 4,
            "count": 4,
            "status": "no_change",
            "finished": true,
            "strategy_summary": "verify first",
            "checks_passed": 2,
            "checks_failed": 0,
            "turns": 53,
            "branch": ""
          ]
        ]
      ]]
    ]
    let settledGroups = MDxRouteClient.parallelExecutionGroups(from: settledGroupEnvelope)
    if let group = settledGroups.first {
      expect(group.isSettled, "settled width-4 group maps as settled")
      expect(group.progressLine == "3 done, 1 made no change", "settled group summarizes outcomes, not generic finished")
      expect(group.selectionStatusLabel == "Ready for review", "settled group selection status is human-readable")
      expect(group.completedSummaryLine.contains("4 lanes: 3 done, 1 made no change"), "settled group summary names lane outcomes")
      expect(group.completedSummaryLine.contains("recommended: Primary 1/4"), "settled group summary names recommended lane")
      expect(group.candidate(runID: "forge_width4_no_change")?.displayStatus == "No change", "finished no-change lane keeps terminal outcome")
    }

    let authorityBlockedRun = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "forge_authority_gate_blocked",
        "status": "done",
        "branch": "forge/run-forge_authority_gate_blocked",
        "checks_passed": 2,
        "checks_failed": 0,
        "events": [[
          "event": "evidence_appended",
          "detail": "eval_quality_gate_assessed status=BLOCKED_BEFORE_PRINCIPAL_REVIEW machine_gates_passed=false failed_gates=proof_scope_covered"
        ]]
      ]]
    ])
    expect(authorityBlockedRun.first?.hasAuthorityGateBlock == true, "principal review gate block stays visible to native run detail")

    let reviewReadyRun = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "forge_review_ready",
        "status": "done",
        "stage": "ready_for_review",
        "branch": "forge/run-forge_review_ready",
        "checks_passed": 3,
        "checks_failed": 0,
        "controls": [
          ["action": "review", "allowed": true, "route": "/forge/runs/review"],
          ["action": "ship", "allowed": true, "route": "/forge/runs/ship"]
        ]
      ]]
    ])
    expect(reviewReadyRun.first?.isReviewReady == true, "open ship or review control marks the run review-ready")
    expect(reviewReadyRun.first?.hasBranch == true, "review-ready run keeps branch identity")

    let notReviewReadyRun = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "forge_not_review_ready",
        "status": "running",
        "controls": [
          ["action": "steer", "allowed": true, "route": "/forge/runs/steer"],
          ["action": "ship", "allowed": false, "route": "/forge/runs/ship"]
        ]
      ]]
    ])
    expect(notReviewReadyRun.first?.isReviewReady == false, "locked ship control does not mark the run review-ready")

    let linkedMission = ForgeMission(
      id: "mission_linked",
      goal: "Improve native Forge review readiness",
      nonGoals: "",
      constraints: "",
      doneWhen: "",
      allowedWriteScope: "apps/mdx-operator-macos/",
      blockedPaths: "",
      validationCommands: "make native-macos-operator-check",
      fleetWidth: 2,
      maxCostCents: 500,
      checkpointCadenceMinutes: 30,
      state: "RUNNING",
      latestCheckpointSummary: "checkpoint 1 landed",
      checkpointCount: 1,
      completedMilestoneCount: 0,
      blockedMilestoneCount: 0,
      steeringNoteCount: 0,
      relatedRunIDs: ["forge_review_ready"],
      milestones: [
        MissionMilestone(
          milestoneID: "mission_milestone_01",
          title: "Checkpoint 1",
          validationCommand: "make native-macos-operator-check",
          status: "IN_PROGRESS",
          acceptanceCriteria: "Inspector shows review readiness and mission linkage",
          checkpointReceiptID: "",
          validationStatus: "",
          relatedRunID: "forge_width2_candidate"
        )
      ]
    )
    expect(linkedMission.displayName == "Improve native Forge review readiness", "mission displayName prefers the goal")
    expect(linkedMission.links(runID: "forge_review_ready") == true, "mission links via related_run_ids")
    expect(linkedMission.links(runID: "forge_width2_candidate") == true, "mission links via milestone related_run_id")
    expect(linkedMission.links(runID: "unrelated_run") == false, "mission does not invent run links")
    expect(linkedMission.stateLabel == "In progress", "mission stateLabel stays human-readable")

    let heldSystemRun = MDxRouteClient.forgeRuns(from: [
      "runs": [[
        "run_id": "forge_fleet_cast_test",
        "title": "System Forge run",
        "status": "recorded",
        "terminal_state": "RECORDED",
        "origin": "system",
        "system_origin": "forge_system",
        "stream_route": "/forge/runs/stream?run_id=forge_fleet_cast_test",
        "turns": 0,
        "model_calls": 0,
        "tool_calls": 0,
        "stages": [[
          "key": "done",
          "label": "Done",
          "state": "active"
        ]],
        "controls": [[
          "action": "review",
          "allowed": true,
          "route": "/forge/review-packet.json"
        ]],
        "events": [[
          "event": "evidence_appended",
          "stage": "done",
          "detail": "machine_league_fleet_cast status=FLEET_CAST_PLAN_READY requested=8 planned=8"
        ]]
      ]]
    ])
    expect(heldSystemRun.first?.isRunning == false, "held system evidence rows do not render as running")
    expect(heldSystemRun.first?.terminalState == "RECORDED", "system evidence reads the kernel terminal state")
    expect(heldSystemRun.first?.isReviewReady == false, "held system evidence does not enter the human review queue")

    let roles = MDxRouteClient.modelRoleRoutes(from: [
      "roles": [
        [
          "role": "planner",
          "slot": "OPUS",
          "provider_family": "anthropic",
          "model_id": "claude-opus-4-8",
          "credential_available": true,
          "assigned_slot": "AUTO"
        ],
        [
          "role": "executor",
          "slot": "XAI",
          "provider_family": "xai",
          "model_id": "grok-4.5",
          "credential_available": true,
          "assigned_slot": "XAI"
        ],
        [
          "role": "advisor",
          "slot": "GPT",
          "provider_family": "openai",
          "model_id": "gpt-5",
          "credential_available": true,
          "assigned_slot": ""
        ]
      ]
    ])
    expect(roles.first { $0.id == "plan" }?.title == "Plan & orchestrate", "planner and conductor collapse to the native plan role")
    expect(roles.first { $0.id == "build" }?.provider == "Grok", "executor role maps provider display name")
    expect(roles.first { $0.id == "build" }?.assigned == true, "explicit role assignment is preserved")
    expect(roles.first { $0.id == "advisor" }?.advisoryOnly == true, "advisor role stays advisory-only")
    expect(roles.first { $0.id == "advisor" }?.supportLine == "Advisor auto", "unassigned advisor reads as auto advisor")
  }

  private static func checkStreamEvent() {
    let envelope: JSONDictionary = [
      "seq": 41,
      "kind": "tool_call",
      "stage": "work",
      "receipt_id": "receipt_abc",
      "model": "grok-4.3",
      "payload": ["summary": "ran make lint", "text": "", "detail": "exit 0"],
    ]
    let event = MDxRouteClient.streamEvent(from: envelope, index: 7)
    expect(event.seq == 41, "stream event seq")
    expect(event.kind == "tool_call", "stream event kind")
    expect(event.stage == "work", "stream event stage")
    expect(event.receiptID == "receipt_abc", "stream event receipt")
  }

  private static func checkFleetPlans() {
    let plans = MDxRouteClient.fleetPlans(from: [
      "fleets": [[
        "fleet_id": "fleet_review",
        "status": "drafted",
        "spec": "Plan a native fleet",
        "stream_count": 1,
        "execution_geometry": [
          "requested_width": 8,
          "max_concurrent_workers": 24,
          "stream_check_count": 1,
          "by_coder": ["default": 1],
          "by_sensitivity": ["internal": 1]
        ],
        "wide_plan_review": [
          "required": true,
          "status": "recorded",
          "reviewer_model": "gpt-5",
          "verdict": "revise",
          "confidence": "high",
          "concerns": "VERDICT: revise\nCONFIDENCE: high\n1) s1 write_scope collides with integration-owned paths. Action: narrow the scope."
        ]
      ]]
    ])
    expect(plans.first?.reviewLine == "Advisor review by gpt-5 asked for changes, high confidence.", "fleet advisor review line is human-readable")
    expect(plans.first?.reviewConcerns.hasPrefix("s1 write_scope collides") == true, "fleet advisor concern drops protocol labels")
    expect(plans.first?.needsPlanReview == true, "fleet advisor concern marks plan for revision")

    let ratified = MDxRouteClient.fleetPlans(from: [
      "fleets": [[
        "fleet_id": "fleet_ratified",
        "status": "ratified",
        "spec": "Ratified plan",
        "stream_count": 1,
        "execution_geometry": ["stream_check_count": 1],
        "wide_plan_review": ["required": false]
      ]]
    ])
    expect(ratified.first?.isRatified == true, "ratified fleet plan maps to startable state")
    expect(ratified.first?.needsPlanReview == false, "not-required fleet review does not block native actions")

    let reviewUnavailable = MDxRouteClient.fleetPlans(from: [
      "fleets": [[
        "fleet_id": "fleet_review_held",
        "status": "drafted",
        "spec": "Fallback plan",
        "stream_count": 8,
        "execution_geometry": ["requested_width": 8],
        "wide_plan_review": [
          "required": true,
          "status": "review_unavailable",
          "reviewer_model": "gpt-5",
          "verdict": "not_recorded",
          "confidence": "none",
          "concerns": "provider calls are paused by operator runtime control"
        ]
      ]]
    ])
    expect(reviewUnavailable.first?.reviewStatus == "review_unavailable", "fleet review status is structured")
    expect(reviewUnavailable.first?.needsPlanReview == true, "unavailable wide review blocks ratification")
    expect(reviewUnavailable.first?.reviewLine.contains("could not run") == true, "unavailable wide review is human-readable")

    let planning = MDxRouteClient.fleetPlans(from: [
      "fleets": [[
        "fleet_id": "fleet_planning",
        "status": "planning",
        "spec": "Planner-only decomposition context",
        "goal": "Planning fleet",
        "checks": ["npm test"],
        "stream_count": 0,
        "execution_geometry": [
          "requested_width": 8,
          "max_concurrent_workers": 24
        ],
        "wide_plan_review": [
          "required": true,
          "status": "planning",
          "verdict": "not_recorded",
          "confidence": "none"
        ],
        "planning_progress": [
          "stage": "planner_drafting",
          "detail": "The planner is drafting disjoint lanes for this fleet.",
          "attempt": 1
        ]
      ]]
    ])
    expect(planning.first?.isPlanning == true, "planning fleet plan maps to pending state")
    expect(planning.first?.needsPlanReview == false, "planning fleet plan does not expose ratification as review")
    expect(planning.first?.requestedWidth == 8, "planning fleet plan preserves requested width")
    expect(planning.first?.spec == "Planning fleet", "planning fleet hides planner-only instructions from the operator goal")
    expect(planning.first?.checks == ["npm test"], "planning fleet maps first-class requested checks")
    expect(planning.first?.suggestedChecks == ["npm test"], "planning fleet preserves the operator-supplied proof")
    expect(planning.first?.displayLine == "The planner is drafting disjoint lanes for this fleet.", "planning fleet plan uses progress detail")
    expect(planning.first?.planningStageLabel == "Planner", "planning fleet plan maps stage label")

    let legacySpecOnly = MDxRouteClient.fleetPlans(from: [
      "fleets": [[
        "fleet_id": "fleet_legacy_spec_only",
        "status": "drafted",
        "spec": "Operator requested a 4-worker execution plan"
      ]]
    ])
    expect(legacySpecOnly.first?.spec == "", "fleet goal stays empty when only planner spec is projected")

    let slowPlanning = MDxRouteClient.fleetPlans(from: [
      "fleets": [[
        "fleet_id": "fleet_slow",
        "status": "planning_delayed",
        "spec": "Slow planning fleet",
        "stream_count": 0,
        "execution_geometry": ["requested_width": 8],
        "wide_plan_review": ["required": true, "status": "planning"],
        "planning_progress": [
          "stage": "planner_slow",
          "detail": "The planner is taking longer than usual. Forge is still waiting; you can keep waiting, retry with a narrower fleet, or switch to Mission if this is long-horizon work."
        ]
      ]]
    ])
    expect(slowPlanning.first?.isPlanning == true, "slow planning fleet remains pending")
    expect(slowPlanning.first?.isDelayedPlanning == true, "slow planning fleet maps delayed state")
    expect(slowPlanning.first?.planningStageLabel == "Still planning", "slow planning fleet maps recovery stage")
    expect(slowPlanning.first?.displayLine.contains("retry with a narrower fleet") == true, "slow planning fleet surfaces recovery guidance")

    let planningNeedsRepair = MDxRouteClient.fleetPlans(from: [
      "fleets": [[
        "fleet_id": "fleet_needs_repair",
        "status": "planning_needs_attention",
        "spec": "Conflicting fleet",
        "stream_count": 0,
        "execution_geometry": [
          "requested_width": 8,
          "max_concurrent_workers": 24
        ],
        "wide_plan_review": ["required": true, "status": "planning"],
        "planning_progress": [
          "stage": "needs_attention",
          "detail": "two streams would write the same files - fix the split"
        ]
      ]]
    ])
    expect(planningNeedsRepair.first?.needsPlanningRepair == true, "unsafe planning split maps to explicit repair state")
    expect(planningNeedsRepair.first?.isPlanning == false, "unsafe planning split stops presenting as active planning")
    expect(planningNeedsRepair.first?.needsPlanReview == true, "unsafe planning split cannot expose ratification")
    expect(planningNeedsRepair.first?.requestedWidth == 8, "repair state preserves the operator-requested lane width")
    expect(planningNeedsRepair.first?.displayLine.contains("fix the split") == true, "repair state preserves actionable planner evidence")

    let runs = MDxRouteClient.fleetRuns(from: [
      "fleet_runs": [[
        "fleet_id": "fleet_live",
        "goal": "Repair native fleet recovery",
        "checks": ["make native-macos-operator-check"],
        "running": false,
        "finished": true,
        "final_detail": "streams=2 green=0 needs_attention=2",
        "proof": [
          "stream_count": 2,
          "lane_attention_count": 1,
          "runtime_recovery": [
            "next_action": "repair_attention_lanes_before_failing_fleet",
            "repair_attempt_count": 2,
            "max_repair_attempts": 2
          ]
        ],
        "lanes": [[
          "stream_id": "native_fleet_store_review",
          "state": "needs_attention",
          "forge_run_id": "forge_run_000100",
          "detail": "repair exhausted: attempts=2 prior_status=RUN_FAILED_TO_START",
          "coder": "default",
          "builder_casting": [
            "status": "INSUFFICIENT_EVIDENCE",
            "basis": "no_matching_language_task_model_evidence_yet"
          ]
        ]]
      ]]
    ])
    expect(runs.first?.status == "Needs attention", "fleet run maps terminal attention state")
    expect(runs.first?.goal == "Repair native fleet recovery", "fleet run maps its first-class goal")
    expect(runs.first?.checks == ["make native-macos-operator-check"], "fleet run maps its first-class checks")
    expect(runs.first?.lanes.first?.needsAttention == true, "fleet lane maps attention state")
    expect(runs.first?.lanes.first?.castingStatus == "INSUFFICIENT_EVIDENCE", "fleet lane preserves builder casting evidence")

    let partialFleet = MDxRouteClient.fleetRuns(from: [
      "fleet_runs": [[
        "fleet_id": "fleet_partial_deadline",
        "running": false,
        "finished": true,
        "integration_state": "skipped",
        "proof": [
          "stream_count": 2,
          "lane_done_count": 1,
          "lane_attention_count": 1,
          "runtime_recovery": [
            "next_action": "repair_attention_lanes_before_failing_fleet",
            "repair_attempt_count": 0,
            "max_repair_attempts": 2
          ]
        ],
        "lanes": [
          ["stream_id": "s1_green", "state": "done", "forge_run_id": "forge_run_green"],
          [
            "stream_id": "s2_stuck",
            "state": "needs_attention",
            "forge_run_id": "forge_run_stuck",
            "detail": "lane did not report before the 5000ms fleet deadline; routed to attention; done=2/2"
          ]
        ]
      ]]
    ])
    expect(partialFleet.first?.status == "Needs attention", "partial fleet maps to terminal attention")
    expect(partialFleet.first?.summary == "1 of 2 lanes need attention", "partial fleet keeps exact lane counts readable")
    expect(partialFleet.first?.needsRecovery == true, "partial fleet exposes recovery")
    expect(partialFleet.first?.recovery.hasPrefix("Retry the lanes that need attention") == true, "partial fleet names the safe retry")
    expect(partialFleet.first?.lanes.last?.detail.contains("5000ms fleet deadline") == true, "stuck lane preserves its enforced deadline")

    let integrationFailure = MDxRouteClient.fleetRuns(from: [
      "fleet_runs": [[
        "fleet_id": "fleet_integration_failed",
        "running": false,
        "finished": true,
        "integration_state": "did_not_land",
        "integration_detail": "no branch: merge of s1 did not apply cleanly",
        "proof": [
          "stream_count": 2,
          "lane_done_count": 2,
          "lane_attention_count": 0,
          "runtime_recovery": [
            "next_action": "integrate_terminal_lanes",
            "recovery_health": "healthy",
            "can_auto_continue": true
          ]
        ],
        "lanes": [
          ["stream_id": "s1", "state": "done"],
          ["stream_id": "s2", "state": "done"]
        ]
      ]]
    ])
    expect(integrationFailure.first?.status == "Integration failed", "failed fleet merge overrides generic finished status")
    expect(integrationFailure.first?.hasIntegrationFailure == true, "failed fleet merge counts as integration attention")
    expect(integrationFailure.first?.attentionCount == 1, "integration failure contributes to fleet attention")
    expect(integrationFailure.first?.summary.contains("did not apply cleanly") == true, "integration failure preserves merge evidence")
    expect(integrationFailure.first?.recovery.contains("corrected scopes") == true, "integration failure gives a safe recovery move")
    expect(integrationFailure.first?.recovery.contains("healthy") == false, "integration failure does not echo stale healthy recovery")

    let activeRuns = MDxRouteClient.fleetRuns(from: [
      "fleet_runs": [[
        "fleet_id": "fleet_active",
        "running": true,
        "finished": false,
        "lanes": [[
          "stream_id": "native_fleet_live_lane",
          "state": "working",
          "forge_run_id": "forge_run_live_lane",
          "detail": "scope_paths=1"
        ]]
      ]]
    ])
    expect(activeRuns.first?.isActive == true, "active fleet run keeps native fleet watcher alive")
  }

  private static func checkOutcomeClassification() {
    func outcome(_ status: String) -> RunActionOutcome {
      RunActionOutcome(title: "t", status: status, detail: "", receiptID: "")
    }
    expect(outcome("RUN_STARTED").succeeded, "RUN_STARTED is success")
    expect(outcome("PAGE_PUBLISHED_EDITOR_BLOCKED").succeeded, "blocked-suffix publish is success")
    expect(outcome("MESSAGE_ACTION_VERDICT_RECORDED_EXECUTION_BLOCKED").succeeded, "recorded verdict is success")
    expect(outcome("OK").succeeded, "OK is success")
    expect(outcome("REFUSED").classification == .refused, "REFUSED classifies refused")
    expect(outcome("REFUSED_NO_BRANCH").classification == .refused, "compound refusal classifies refused")
    expect(outcome("DENIED").classification == .refused, "DENIED classifies refused")
    expect(outcome("FAILED").classification == .failed, "FAILED classifies failed")
    expect(!outcome("RUN_STARTED").isRefusal, "success is not a refusal")

    let mapped = MDxRouteClient.runOutcome(
      from: ["status": "RUN_STARTED", "run_id": "run_000123"],
      title: "Start run",
      idKeys: ["run_id"]
    )
    expect(
      mapped.status == "RUN_STARTED" && mapped.receiptID == "run_000123" && mapped.succeeded,
      "runOutcome maps status + id keys"
    )
    let missingReceipt = MDxRouteClient.runOutcome(
      from: ["status": "RUN_STARTED"],
      title: "Start run",
      idKeys: ["run_id"]
    )
    expect(missingReceipt.status == "FAILED_MISSING_RECEIPT" && !missingReceipt.succeeded, "runOutcome refuses success without receipt")
  }

  private static func checkTwin() {
    let twinJSON = fixture("twin-drafts")
    if let firstDraft = (twinJSON["drafts"] as? [[String: Any]])?.first {
      let message = MDxRouteClient.twinMessage(from: firstDraft)
      expect(!message.id.isEmpty, "twin draft maps with identity")
      expect(!message.prompt.isEmpty || !message.answer.isEmpty, "twin draft carries prompt or answer")
      expect(
        message.messageDraft.contains(message.answer) && message.messageDraft.hasPrefix("Twin on \""),
        "Twin Message handoff prepares a reviewable draft instead of sending"
      )
    }
    let derivedRow: JSONDictionary = ["session_id": "s1", "created_at": "2026-07-01T10:00:00Z", "draft_text": "hi"]
    expect(
      MDxRouteClient.twinMessage(from: derivedRow).id == MDxRouteClient.twinMessage(from: derivedRow).id,
      "twin fallback id is derived, not random"
    )
    let rawRankingOnly: JSONDictionary = [
      "session_id": "s2",
      "created_at": "2026-07-01T10:00:01Z",
      "memory_recall_final_score": 630,
      "brain_recall_source_count": 3,
      "trusted_context_source_count": 2
    ]
    let honestMemory = MDxRouteClient.twinMessage(from: rawRankingOnly)
    expect(honestMemory.memoryScore == 0, "raw recall ranking is never rendered as a percentage")
    expect(honestMemory.memorySourceCount == 3, "Twin maps the observed saved-context source count")
    expect(honestMemory.trustedSourceCount == 2, "Twin maps trusted Pages source count")
  }

  private static func checkActivityIdentity() {
    let seqItem: JSONDictionary = ["receipt_id": "r1", "ledger_seq": 4046]
    expect(
      MDxRouteClient.activityItemID(seqItem, index: 0) == MDxRouteClient.activityItemID(seqItem, index: 250),
      "activity id stable across reorder"
    )
    let activityItems = (fixture("activity")["items"] as? [[String: Any]]) ?? []
    if !activityItems.isEmpty {
      let ids = activityItems.enumerated().map { MDxRouteClient.activityItemID($1, index: $0) }
      expect(Set(ids).count == ids.count, "activity fixture ids unique (\(ids.count) items)")
    }
  }

  private static func checkMessageReactions() {
    func event(_ actor: String, _ body: String, _ sequence: Int) -> ThreadMessage {
      ThreadMessage(
        id: "reaction_\(sequence)",
        messageID: "reaction_\(sequence)",
        threadID: "react:parent_receipt",
        channelID: "general",
        actorID: actor,
        actorName: actor,
        actorType: actor.hasPrefix("agent:") ? "agent" : "human",
        body: body,
        contentType: "text",
        contentPayload: "",
        receiptKind: "message.thread_message.recorded",
        receiptID: "reaction_receipt_\(sequence)",
        sourceReceiptID: "",
        sourceReceiptKind: "",
        sequence: sequence,
        createdAt: "2026-07-13T12:00:00Z"
      )
    }
    let reactions = MessageReaction.fold(
      messages: [
        event("user:mina", "+:👍", 1),
        event("agent:scout", "+:👍", 2),
        event("user:mina", "-:👍", 3),
        event("user:mina", "+:✅", 4),
        event("agent:mina", "+:👀", 5)
      ],
      parentReceiptID: "parent_receipt",
      viewerActorID: "mina"
    )
    expect(
      reactions == [
        MessageReaction(emoji: "✅", count: 1, mine: true),
        MessageReaction(emoji: "👀", count: 1, mine: false),
        MessageReaction(emoji: "👍", count: 1, mine: false)
      ],
      "Message reactions fold the latest state per actor and preserve viewer ownership"
    )
  }

  private static func checkDiffParsing() {
    let patch = """
    diff --git a/x.swift b/x.swift
    index abc..def 100644
    --- a/x.swift
    +++ b/x.swift
    @@ -1,3 +1,4 @@
     context line
    -removed line
    +added line

    """
    let lines = DiffLine.parse(patch)
    expect(
      lines.count == patch.split(separator: "\n", omittingEmptySubsequences: false).count,
      "diff parse preserves every line"
    )
    expect(lines[0].kind == .meta && lines[2].kind == .meta && lines[3].kind == .meta, "diff headers are meta, not +/- lines")
    expect(lines[4].kind == .hunk, "@@ line is hunk")
    expect(lines[5].kind == .context, "context line")
    expect(lines[6].kind == .deletion, "deletion line")
    expect(lines[7].kind == .addition, "addition line")
    expect(Set(lines.map(\.id)).count == lines.count, "diff line ids unique")

    // Line-number gutter: numbers track from the @@ -1,3 +1,4 @@ header. The
    // context line is old 1 / new 1, the deletion has only an old number, the
    // addition only a new number, and headers carry neither.
    expect(lines[4].oldLine == nil && lines[4].newLine == nil, "hunk header has no line numbers")
    expect(lines[5].oldLine == 1 && lines[5].newLine == 1, "context line numbers both sides")
    expect(lines[6].oldLine == 2 && lines[6].newLine == nil, "deletion numbers the old side only")
    expect(lines[7].oldLine == nil && lines[7].newLine == 2, "addition numbers the new side only")
  }

  private static func checkHardeningHelpers() {
    expect(MDxRouteClient.isTrustedLocalBaseURL(URL(string: "http://127.0.0.1:18890")!), "127.0.0.1 is a trusted write host")
    expect(MDxRouteClient.isTrustedLocalBaseURL(URL(string: "http://localhost:18890")!), "localhost is a trusted write host")
    expect(!MDxRouteClient.isTrustedLocalBaseURL(URL(string: "https://example.com")!), "remote hosts are not trusted write hosts")

    let claimedIdentity: JSONDictionary = [
      "tenant_id": "local_tenant",
      "actor_id": "local_user",
      "actor_role": "owner",
      "intent": "Inspect the repository"
    ]
    let remoteBody = MDxRouteClient.governedRequestBody(
      claimedIdentity,
      baseURL: URL(string: "https://mdx.example.com/api/kernel")!
    )
    expect(
      remoteBody["tenant_id"] == nil && remoteBody["actor_id"] == nil && remoteBody["actor_role"] == nil,
      "remote governed writes derive identity from the verified bearer session"
    )
    expect(remoteBody["intent"] as? String == "Inspect the repository", "remote governed writes preserve action fields")
    let localBody = MDxRouteClient.governedRequestBody(
      claimedIdentity,
      baseURL: URL(string: "http://127.0.0.1:18890")!
    )
    expect(localBody["actor_id"] as? String == "local_user", "local demo writes preserve the deterministic actor fixture")

    let longContext = String(repeating: "a", count: MDxRouteClient.maxInlineDocumentContextCharacters + 100)
    let boundedContext = MDxRouteClient.boundedDocumentContext(longContext)
    expect(
      boundedContext.hasPrefix(String(repeating: "a", count: MDxRouteClient.maxInlineDocumentContextCharacters))
        && boundedContext.contains("clipped this document context"),
      "document context is bounded"
    )

    let longPatch = Array(repeating: "+x", count: MDxRouteClient.maxDiffPatchLines + 100).joined(separator: "\n")
    let capped = MDxRouteClient.cappedDiffPatch(longPatch)
    expect(capped.count < longPatch.count && capped.contains("clipped this diff preview"), "large diff previews are clipped")
  }

  private static func checkHelpers() {
    expect(RelativeTime.date(from: "2026-07-01T10:00:00.123Z") != nil, "ISO with fraction parses")
    expect(RelativeTime.date(from: "2026-07-01T10:00:00Z") != nil, "ISO without fraction parses")
    expect(RelativeTime.date(from: "not a date") == nil, "garbage does not parse")

    let artifactID = MDxRouteClient.sanitizedArtifactID(from: "Board Brief (v2).pdf")
    expect(!artifactID.isEmpty && !artifactID.contains(" "), "artifact ids are route-safe")
    expect(
      MDxRouteClient.stringList("make native-macos-operator-check") == ["make native-macos-operator-check"],
      "string helper preserves scalar check values"
    )
    expect(
      StartBuildSheet.proofCommands(from: " test -f README.md\n\nmake smoke\ntest -f README.md ")
        == ["test -f README.md", "make smoke"],
      "native build composer keeps explicit proof commands ordered and unique"
    )
    let repos = MDxRouteClient.forgeRepos(from: [
      "repos": [[
        "repo_id": "node-app",
        "label": "Node app",
        "root": "/tmp/node-app",
        "profile": [
          "primary_language": "javascript-typescript",
          "suggested_checks": ["npm test"],
          "proof_plan": [
            "status": "ready",
            "next_action": "Use `npm test` as the proof command."
          ]
        ]
      ]]
    ])
    expect(repos.first?.suggestedCheckCommands == ["npm test"], "native repo mapping preserves stack-native proof commands")
    expect(repos.first?.proofPlanStatus == "ready", "native repo mapping preserves proof readiness")
    expect(repos.first?.proofPlanNextAction.contains("npm test") == true, "native repo mapping preserves proof guidance")
    expect(!MissionDraft().doneWhen.isEmpty, "mission draft carries a non-empty done_when default")
  }

  private static func checkScreenshotBuildRequest() {
    expect(
      ScreenshotBuildRequest.requested(in: ["MDX_SHOT_BUILD_INTENT": "build it"]) == nil,
      "native proof build does not autostart without the explicit switch"
    )
    expect(
      ScreenshotBuildRequest.requested(in: ["MDX_SHOT_AUTOSTART_BUILD": "1"]) == nil,
      "native proof build does not consume an empty intent"
    )

    let request = ScreenshotBuildRequest.requested(in: [
      "MDX_SHOT_AUTOSTART_BUILD": "1",
      "MDX_SHOT_BUILD_INTENT": "  repair the hard fixture  ",
      "MDX_SHOT_REPO_ID": "hard-fixture",
      "MDX_SHOT_FLEET_WIDTH": "2",
      "MDX_SHOT_PROOF_COMMANDS": "npm test\\nnpm run lint\\nnpm test",
      "MDX_SHOT_AUTOSTART_DELAY_MS": "1"
    ])
    expect(request?.intent == "repair the hard fixture", "native proof build trims its intent")
    expect(request?.repoID == "hard-fixture", "native proof build preserves its repo target")
    expect(
      request?.resolvedRepoID(availableRepoIDs: ["hard-fixture"], fallback: "fallback") == "hard-fixture",
      "native proof build resolves an available requested repo"
    )
    expect(
      request?.resolvedRepoID(availableRepoIDs: ["another-repo"], fallback: "fallback") == nil,
      "native proof build refuses an unavailable requested repo instead of changing checkout"
    )
    expect(request?.fleetWidth == 2, "native proof build preserves an explicit dual-run width")
    expect(request?.proofCommands == ["npm test", "npm run lint"], "native proof build preserves unique proof commands")
    expect(request?.delayNanoseconds == 300_000_000, "native proof build clamps a too-short launch delay")

    let bounded = ScreenshotBuildRequest.requested(in: [
      "MDX_SHOT_AUTOSTART_BUILD": "1",
      "MDX_SHOT_BUILD_INTENT": "build the fleet",
      "MDX_SHOT_FLEET_WIDTH": "99",
      "MDX_SHOT_AUTOSTART_DELAY_MS": "99999"
    ])
    expect(bounded?.fleetWidth == 24, "native proof build clamps fleet width to the composer contract")
    expect(bounded?.delayNanoseconds == 20_000_000_000, "native proof build clamps an excessive launch delay")
  }

  /// Pins MDX_SHOT_RUN targeting labels so screenshot auto-open cannot silently
  /// drop active/latest while keeping the existing first semantics.
  private static func checkScreenshotRunTarget() {
    expect(
      ScreenshotRunTarget.labels == ["first", "active", "latest"],
      "screenshot run labels are first/active/latest"
    )

    let fixtureRuns = MDxRouteClient.forgeRuns(from: fixture("forge-runs"))
    expect(!fixtureRuns.isEmpty, "screenshot targeting has forge-runs fixture data")

    let first = ScreenshotRunTarget.resolve(target: "first", runs: fixtureRuns)
    let expectedFirst = fixtureRuns.first { !$0.isRunning && $0.hasBranch } ?? fixtureRuns.first
    expect(first?.id == expectedFirst?.id, "MDX_SHOT_RUN=first keeps finished-with-branch preference")

    func run(id: String, status: String, branch: String) -> ForgeRun {
      ForgeRun(
        id: id,
        title: id,
        intent: id,
        origin: "",
        systemOrigin: "",
        status: status,
        terminalState: status == "running" ? "IN_PROGRESS" : "SUCCEEDED",
        stage: status == "running" ? "writing" : "done",
        repo: "mdx",
        harness: "native",
        model: "m",
        branch: branch,
        commitSha: "",
        turns: 0,
        modelCalls: 0,
        toolCalls: 0,
        checksPassed: 0,
        checksFailed: 0,
        finalLine: "",
        streamRoute: status == "running" ? "/forge/runs/stream?run_id=\(id)" : "",
        diffReady: false,
        diffFileCount: 0,
        workerCount: 0,
        runner: .empty,
        execution: .single,
        parallelCandidate: .single,
        workClassification: .empty,
        languageTaskAlignment: .empty,
        quarantine: .empty,
        contextTelemetry: .empty,
        stages: [],
        events: [],
        controls: []
      )
    }

    let synthetic = [
      run(id: "forge_run_finished_old", status: "done", branch: "forge/run-finished-old"),
      run(id: "forge_run_active", status: "running", branch: "forge/run-active"),
      run(id: "forge_run_finished_new", status: "done", branch: "forge/run-finished-new")
    ]

    let active = ScreenshotRunTarget.resolve(target: "active", runs: synthetic)
    expect(active?.id == "forge_run_active", "MDX_SHOT_RUN=active opens a running run")
    expect(
      ScreenshotRunTarget.resolve(target: "active", runs: synthetic.filter { !$0.isRunning }) == nil,
      "MDX_SHOT_RUN=active is nil when no run is live"
    )

    let latest = ScreenshotRunTarget.resolve(target: "latest", runs: synthetic)
    expect(latest?.id == "forge_run_finished_new", "MDX_SHOT_RUN=latest opens the newest projection run")

    let byID = ScreenshotRunTarget.resolve(target: "forge_run_active", runs: synthetic)
    expect(byID?.id == "forge_run_active", "MDX_SHOT_RUN=<id> still matches exact run ids")

    expect(
      ScreenshotRunTarget.resolve(target: "missing", runs: synthetic) == nil,
      "unknown MDX_SHOT_RUN id fails closed"
    )
  }

  /// Pins the app-managed lifecycle copy: the diagnosis never hides a real
  /// failure, the repo/branch anchor states the honest per-run default, and the
  /// onboarding win only offers a real build when the workspace can actually
  /// produce one.
  private static func checkAppLifecycle() {
    let healthy = AppLifecycleDiagnosis.summary(
      health: .healthy,
      baseURL: "http://127.0.0.1:18890",
      consecutiveFailures: 0,
      recentHang: false,
      lastError: nil
    )
    expect(healthy.contains("No recent hangs or errors"), "healthy diagnosis states the truth")

    let offline = AppLifecycleDiagnosis.summary(
      health: .offline,
      baseURL: "http://127.0.0.1:18891",
      consecutiveFailures: 4,
      recentHang: false,
      lastError: "connection refused"
    )
    expect(offline.contains("http://127.0.0.1:18891"), "offline diagnosis names the base URL")
    expect(offline.contains("4 checks failed in a row"), "offline diagnosis names the failure streak")
    expect(offline.contains("Last error: connection refused"), "offline diagnosis surfaces the real last error")

    let degraded = AppLifecycleDiagnosis.summary(
      health: .degraded,
      baseURL: "http://127.0.0.1:18890",
      consecutiveFailures: 0,
      recentHang: true,
      lastError: nil
    )
    expect(degraded.contains("something is off") && degraded.contains("hung"), "degraded diagnosis is honest, not cosmetic")

    expect(RepoContext.branchLine(latestRunBranch: "forge/run-abc") == "forge/run-abc", "branch line shows a real run branch")
    expect(RepoContext.branchLine(latestRunBranch: "  ") == "New branch per run", "empty branch falls back to the honest per-run default")
    expect(RepoContext.hasRealBranch(latestRunBranch: "forge/run-abc"), "a real branch is flagged as real")
    expect(!RepoContext.hasRealBranch(latestRunBranch: nil), "no branch is not flagged as real")

    expect(
      FirstWin.canStartRealBuild(connected: true, modelConnected: true, repoCount: 1),
      "connected + model + repo can start a real first build"
    )
    expect(
      !FirstWin.canStartRealBuild(connected: true, modelConnected: false, repoCount: 1),
      "no model cannot fake a first-build win"
    )
    expect(
      FirstWin.blockedReason(connected: false, modelConnected: false, repoCount: 0) == "Connect MDx to start your first build here.",
      "offline first-win reason names the connection gap first"
    )
    expect(
      FirstWin.blockedReason(connected: true, modelConnected: false, repoCount: 1) == "Connect a model to let MDx do real work.",
      "connected-but-no-model first-win reason names the model gap"
    )
    expect(
      FirstWin.blockedReason(connected: true, modelConnected: true, repoCount: 1) == nil,
      "an eligible workspace has no blocked reason"
    )
  }

  /// Honest diagnostics accounting: a kernel-unreachable transport failure is an
  /// offline state, not a per-route error, and a long-lived stream reads as a
  /// human duration instead of an overflowing millisecond count.
  private static func checkDiagnosticsAccounting() {
    // Connection refused / timeout while the engine is off => unreachable.
    expect(Telemetry.isUnreachable(URLError(.cannotConnectToHost)), "connection refused is an unreachable state")
    expect(Telemetry.isUnreachable(URLError(.timedOut)), "a transport timeout is an unreachable state")
    expect(Telemetry.isUnreachable(URLError(.networkConnectionLost)), "a dropped connection is an unreachable state")
    // The kernel answering at all (any HTTP status, malformed body) is a real
    // route failure, never an offline state.
    expect(!Telemetry.isUnreachable(RouteClientError.httpStatus(500)), "an HTTP 500 is a real error, not offline")
    expect(!Telemetry.isUnreachable(RouteClientError.httpStatus(404)), "an HTTP 404 is a real error, not offline")
    expect(!Telemetry.isUnreachable(RouteClientError.invalidJSON), "a malformed body is a real error, not offline")

    // An unreachable event stays in the trail but must not count as an error.
    let unreachableEvent = DiagnosticEvent(
      at: Date(), label: "GET /forge/runtime-projection.json", kind: .route,
      durationMs: 12, ok: false, detail: "cannot connect", unreachable: true
    )
    expect(!unreachableEvent.ok && unreachableEvent.unreachable, "an offline probe is recorded but flagged unreachable")

    // Duration format: sub-second in ms, seconds for a few seconds, m/s for a
    // long stream (the 156984ms stream that used to overflow to "156984m").
    expect(DiagnosticEvent.durationLabel(0.4) == "<1ms", "sub-millisecond work reads as <1ms")
    expect(DiagnosticEvent.durationLabel(240) == "240ms", "sub-second work reads in milliseconds")
    expect(DiagnosticEvent.durationLabel(4200) == "4s", "a few seconds reads in seconds")
    expect(DiagnosticEvent.durationLabel(156_984) == "2m 37s", "a minutes-long stream reads as minutes and seconds")
  }

  /// The visual-QA capture rail's parsing contract: the gallery script and the
  /// app must agree on what MDX_SHOT_APPEARANCE / MDX_SHOT_A11Y mean and how a
  /// variant is named, or captures land in the wrong file or scheme.
  private static func checkVisualQA() {
    expect(ShotOverrides.colorScheme(for: "light") == .light, "appearance=light forces light")
    expect(ShotOverrides.colorScheme(for: "DARK") == .dark, "appearance is case-insensitive")
    expect(ShotOverrides.colorScheme(for: nil) == nil, "no appearance override leaves the saved scheme")
    expect(ShotOverrides.colorScheme(for: "sepia") == nil, "an unknown appearance is ignored, not guessed")

    expect(ShotOverrides.accessibility(for: "large_text") == .largeText, "a11y=large_text forces larger type")
    expect(ShotOverrides.accessibility(for: nil) == nil, "no a11y override applies nothing")
    // Reduce Motion and Increase Contrast cannot be forced in a still frame
    // (see ShotOverrides); the rail must not pretend it captured them.
    expect(ShotOverrides.accessibility(for: "reduce_motion") == nil, "reduce_motion is not a forced still-frame variant")
    expect(ShotOverrides.accessibility(for: "increase_contrast") == nil, "increase_contrast has no per-process override")

    expect(ShotOverrides.variantSlug(appearance: "light", a11y: nil) == "light", "light variant slug")
    expect(ShotOverrides.variantSlug(appearance: "dark", a11y: nil) == "dark", "dark variant slug")
    expect(
      ShotOverrides.variantSlug(appearance: "dark", a11y: "large_text") == "dark-large_text",
      "a11y variant slug appends the setting"
    )

    expect(
      SettingsSection.screenshotSection(in: ["MDX_SHOT_SETTINGS": " authority "]) == .authority,
      "settings capture selects a named section"
    )
    expect(
      SettingsSection.screenshotSection(in: ["MDX_SHOT_SETTINGS": "UPDATES"]) == .updates,
      "settings capture selection is case-insensitive"
    )
    expect(
      SettingsSection.screenshotSection(in: ["MDX_SHOT_SETTINGS": "unknown"]) == nil,
      "unknown settings capture selection fails closed"
    )
    expect(
      SettingsSection.screenshotSection(in: [:]) == nil,
      "missing settings capture selection leaves the app unchanged"
    )
    expect(
      ShotOverrides.pageTitle(in: ["MDX_SHOT_PAGE_TITLE": "Activation_Runway"]) == "Activation Runway",
      "page capture target decodes a TSV-safe title"
    )
    expect(
      ShotOverrides.pageTitle(in: [:]) == nil,
      "missing page capture target leaves page selection unchanged"
    )
  }
}
