import { normalizeRuns, statusLabel } from "./forgeRuns.js";

export function deriveForgeOutcomes(runsProjection) {
  return normalizeRuns(runsProjection).map((run) => {
    const missingEvidence = missingEvidenceFor(run);
    const checksKnown = run.checksPassed + run.checksFailed > 0;
    const done = run.status === "done";
    const hasFailures = run.checksFailed > 0;
    const promotionStatus = promotionStatusFor({ run, checksKnown, hasFailures, missingEvidence });
    const blockedAdaptations = [
      "model routing",
      "fleet casting",
      "check policy",
      "budget defaults",
      "review depth"
    ];

    return {
      outcome_id: `forge_outcome_${run.runId}`,
      run_id: run.runId,
      work_item_id: run.workItemId,
      status: run.status,
      status_label: statusLabel(run.status),
      outcome_state: outcomeStateFor(run),
      intent_summary: intentSummaryFor(run),
      context_status: contextStatusFor(run),
      execution_summary: executionSummaryFor(run),
      evaluation_summary: evaluationSummaryFor(run),
      human_judgment_status: "waiting for judgment",
      lesson_candidate: lessonCandidateFor(run, checksKnown, hasFailures),
      promotion_status: promotionStatus,
      missing_evidence: missingEvidence,
      blocked_adaptations: blockedAdaptations,
      evidence: {
        route: "/forge/runs/projection.json",
        receipt_kind: "forge.run.event",
        events_observed: run.events.length,
        model: run.model,
        branch: run.branch
      }
    };
  });
}

// Promotion items come from the kernel's drafted-candidate projection, not
// from templated ids over seed JSON. Every id here (candidate_id as
// promotion_id, judgment_id) is kernel-owned; every row cites the source
// receipt that drafted it.
export function deriveLessonPromotions(candidateProjection) {
  return (candidateProjection?.candidates ?? []).map((candidate) => ({
    promotion_id: candidate.promotion_id || candidate.candidate_id,
    judgment_id: candidate.judgment_id || "",
    source_type: candidate.source || "forge_outcome",
    source_id: candidate.source_receipt_id,
    source_receipt_id: candidate.source_receipt_id,
    surface: draftSurfaceLabelFor(candidate.source),
    title: candidate.title || candidate.lesson_summary,
    state: candidate.lane_state || "candidate",
    proposed_memory: candidate.lesson_summary,
    lesson_source: candidate.lesson_source || "deterministic",
    judgment_status: "waiting for judgment",
    required_evidence: [
      "intent link",
      "context packet",
      "execution receipt",
      "evaluation result",
      "accountable judgment",
      "source-of-truth update",
      "local check"
    ],
    missing_evidence: ["accountable judgment", "source-of-truth update", "local check"],
    allowed_next_step: "Read the drafted lesson, edit it, then keep it or set it aside.",
    draft_flagged: Boolean(candidate.draft_flagged),
    draft_flag_reason: candidate.draft_flag_reason || "",
    target_type: candidate.target_type || "decision_record",
    target_path: candidate.target_path || "generated/learning/forge-outcome-memory-targets.json",
    evidence_refs: candidate.evidence_refs || candidate.source_receipt_id || "",
    blocked_changes: [
      "model routing",
      "fleet casting",
      "check policy",
      "budget defaults",
      "review depth"
    ]
  }));
}

function draftSurfaceLabelFor(source) {
  if (source === "beta_feedback") return "Feedback";
  if (source === "fleet_distillation") return "Machines";
  return "Forge";
}

export function deriveJudgmentRecords(lessonPromotions) {
  return lessonPromotions.map((promotion) => ({
    judgment_id: promotion.judgment_id,
    promotion_id: promotion.promotion_id,
    source_type: promotion.source_type,
    surface: promotion.surface,
    title: promotion.title,
    state: judgmentStateFor(promotion),
    decision_options: ["promote", "reject", "supersede", "ask for more evidence"],
    accountable_actor_required: true,
    rationale_required: true,
    receipt_required: true,
    memory_target: promotion.proposed_memory,
    target_type: promotion.target_type,
    target_path: promotion.target_path,
    adaptation_scope: "none in v0",
    missing_evidence: judgmentMissingEvidenceFor(promotion),
    blocked_changes: promotion.blocked_changes,
    safe_next: judgmentNextStepFor(promotion)
  }));
}

export function deriveJudgmentDecisions(decisionProjection, judgmentRecords) {
  const recordsByJudgment = new Map(judgmentRecords.map((record) => [record.judgment_id, record]));
  return (decisionProjection?.decisions ?? []).map((decision) => {
    const record = recordsByJudgment.get(decision.judgment_id);
    return {
      ...decision,
      surface: record?.surface ?? "Learn",
      title: record?.title ?? decision.promotion_id,
      decision_label: decisionLabelFor(decision.decision),
      memory_queue_label: memoryQueueLabelFor(decision.memory_queue_state),
      blocked_changes: record?.blocked_changes ?? ["memory write", "adaptation"],
      safe_next:
        decision.memory_queue_state === "ready_for_memory"
          ? "Review the memory target before any source-of-truth write exists."
          : "Keep the decision on record; no memory or adaptation follows from it."
    };
  });
}

export function deriveMemoryTargets(judgmentRecords, judgmentDecisions = []) {
  const decisionsByJudgment = new Map(
    judgmentDecisions.map((decision) => [decision.judgment_id, decision])
  );
  return judgmentRecords.map((record) => {
    // Prefer the kernel-suggested target from the drafted candidate; fall
    // back to the surface heuristic for records without one.
    const target =
      record.target_type && record.target_path
        ? { type: record.target_type, path: record.target_path }
        : memoryTargetFor(record);
    const decision = decisionsByJudgment.get(record.judgment_id);
    return {
      memory_target_id: `memory_${record.judgment_id}`,
      judgment_id: record.judgment_id,
      promotion_id: record.promotion_id,
      surface: record.surface,
      title: record.title,
      target_type: target.type,
      target_path: target.path,
      source_of_truth_update_required: true,
      local_checks_required: ["make learning-loop-check", "make source-map-check"],
      rollback_rule: "Supersede with a newer memory target instead of editing history silently.",
      supersession_rule: "A later lesson must link back to the memory target it replaces.",
      sovereignty_rule: "Provider prompts, provider transcripts, and external model behavior are not MDx memory.",
      state:
        decision?.memory_queue_state === "ready_for_memory"
          ? "ready_for_memory"
          : record.state === "ready_for_judgment"
            ? "ready_after_judgment"
            : "waiting_for_judgment",
      judgment_decision_receipt_id: decision?.judgment_decision_receipt_id ?? "",
      judgment_decision: decision?.decision ?? "",
      blocked_changes: record.blocked_changes,
      safe_next: "Choose the MDx-owned memory target before any adaptation proposal can be trusted."
    };
  });
}

export function deriveMemoryQueue(judgmentDecisions, memoryTargets) {
  const targetsByJudgment = new Map(memoryTargets.map((target) => [target.judgment_id, target]));
  return judgmentDecisions
    .filter((decision) => decision.memory_queue_state === "ready_for_memory")
    .map((decision) => {
      const target = targetsByJudgment.get(decision.judgment_id);
      return {
        memory_queue_id: `memory_queue_${decision.judgment_decision_id}`,
        judgment_decision_id: decision.judgment_decision_id,
        judgment_id: decision.judgment_id,
        promotion_id: decision.promotion_id,
        surface: decision.surface,
        title: decision.title,
        target_type: target?.target_type ?? "decision_record",
      target_path: target?.target_path ?? "generated/learning/pending-memory-targets.json",
      lesson_summary: target?.title ?? decision.title,
      evidence_refs: decision.evidence_refs,
      memory_write_allowed: false,
      adaptation_allowed: false,
      safe_next: "Write memory through a later governed promotion route, not from this queue."
      };
    });
}

export function deriveMemoryCandidates(memoryPromotionProjection, memoryQueue = []) {
  const queueByDecision = new Map(
    memoryQueue.map((item) => [item.judgment_decision_id, item])
  );
  return (memoryPromotionProjection?.candidates ?? []).map((candidate) => {
    const queueItem = queueByDecision.get(candidate.judgment_decision_id);
    return {
      ...candidate,
      surface: queueItem?.surface ?? "Learn",
      title: queueItem?.title ?? candidate.lesson_summary,
      target_label: memoryTargetLabelFor(candidate.target_type),
      state_label:
        candidate.memory_candidate_state === "candidate"
          ? "Candidate"
          : candidate.memory_candidate_state,
      safe_next:
        "Review this candidate through a later activation gate before any active memory write exists.",
      blocked_changes: [
        "active memory write",
        "generated artifact refresh",
        "scorecard update",
        "adaptation"
      ]
    };
  });
}

export function deriveActiveMemories(memoryActivationProjection, memoryCandidates = []) {
  const candidatesByPromotion = new Map(
    memoryCandidates.map((candidate) => [candidate.memory_promotion_id, candidate])
  );
  return (memoryActivationProjection?.active_memories ?? []).map((active) => {
    const candidate = candidatesByPromotion.get(active.memory_promotion_id);
    const retired = active.active_memory_state === "superseded";
    return {
      ...active,
      surface: candidate?.surface ?? "Learn",
      title: candidate?.title ?? active.lesson_summary,
      target_label: memoryTargetLabelFor(active.target_type),
      state_label:
        active.active_memory_state === "active"
          ? "Active"
          : retired
            ? "Retired"
            : active.active_memory_state,
      safe_next: retired
        ? "This lesson is retired. It stays in the record but no longer guides new work."
        : "Use this memory as evidence for a later adaptation proposal; no behavior changes until adaptation is approved.",
      blocked_changes: [
        "generated artifact refresh",
        "provider memory write",
        "adaptation",
        "runtime behavior change"
      ]
    };
  });
}

export function deriveAdaptationProposals(memoryTargets) {
  return memoryTargets.flatMap((target) =>
    adaptationTypesFor(target).map((adaptationType) => ({
      adaptation_proposal_id: `adapt_${adaptationType.id}_${target.memory_target_id}`,
      memory_target_id: target.memory_target_id,
      judgment_id: target.judgment_id,
      surface: target.surface,
      title: target.title,
      adaptation_type: adaptationType.id,
      adaptation_label: adaptationType.label,
      proposed_change: adaptationType.change,
      consumer_surface: adaptationType.consumer,
      required_memory_state: "memory_written",
      required_judgment_state: "promoted",
      required_checks: ["make learning-loop-check", "make local-smoke"],
      risk_boundary: adaptationType.risk,
      rollback_path: "Revert through a later adaptation receipt and supersede the memory target if the lesson was wrong.",
      execution_state: "blocked",
      blocked_reason: "Memory is not written, judgment is not recorded, and no governed adaptation write route exists."
    }))
  );
}

function outcomeStateFor(run) {
  if (run.status === "running") return "still running";
  if (run.status === "done" && run.checksFailed === 0 && run.checksPassed > 0) return "completed with checks";
  if (run.status === "done") return "completed without evaluation";
  if (run.checksFailed > 0) return "needs review";
  return "not promotable";
}

function intentSummaryFor(run) {
  if (run.workItemId) return `work item ${run.workItemId}`;
  return "intent not exposed by the run projection yet";
}

function contextStatusFor(run) {
  const contextEvent = run.events.find((event) => event.detail.includes("context assembled"));
  const standardsEvent = run.events.find((event) => event.detail.includes("building to your standards"));
  if (contextEvent && standardsEvent) return "context and standards observed";
  if (contextEvent) return "context observed";
  if (standardsEvent) return "standards observed";
  return "context packet missing";
}

function executionSummaryFor(run) {
  return `${run.turns} ${run.turns === 1 ? "turn" : "turns"}, ${run.modelCalls} model ${run.modelCalls === 1 ? "call" : "calls"}, ${run.toolCalls} tool ${run.toolCalls === 1 ? "call" : "calls"}`;
}

function evaluationSummaryFor(run) {
  if (run.checksPassed === 0 && run.checksFailed === 0) {
    return "no checks recorded yet";
  }
  return `${run.checksPassed} passed, ${run.checksFailed} failed`;
}

function lessonCandidateFor(run, checksKnown, hasFailures) {
  if (run.status === "running") {
    return "Watch this run until it produces terminal evidence.";
  }
  if (!checksKnown) {
    return "Outcome cannot teach casting yet because no checks are recorded.";
  }
  if (hasFailures) {
    return "Use this as a failure-pattern candidate, not a reason to reduce review.";
  }
  return "Candidate lesson: this run may inform future Forge casting after judgment.";
}

function missingEvidenceFor(run) {
  const missing = [];
  if (!run.workItemId) missing.push("intent link");
  if (!run.events.some((event) => event.detail.includes("context assembled"))) {
    missing.push("context packet");
  }
  if (run.checksPassed + run.checksFailed === 0) missing.push("evaluation");
  missing.push("human judgment");
  return missing;
}

function adaptationTypesFor(target) {
  if (target.surface === "Forge") {
    return [
      {
        id: "fleet_casting",
        label: "Fleet casting",
        change: "Let promoted Forge outcomes inform the next worker mix.",
        consumer: "Forge",
        risk: "Cannot alter worker mix without explicit approval."
      },
      {
        id: "model_routing",
        label: "Model routing",
        change: "Let promoted Forge outcomes inform model preference.",
        consumer: "Harness",
        risk: "Cannot route to a provider or model without gateway approval."
      }
    ];
  }
  if (target.surface === "Quality") {
    return [
      {
        id: "check_policy",
        label: "Check policy",
        change: "Let promoted quality lessons suggest checks for similar work.",
        consumer: "Quality",
        risk: "Cannot add, remove, or skip checks without approval."
      }
    ];
  }
  if (target.surface === "Harness" || target.surface === "Models" || target.surface === "Machines") {
    return [
      {
        id: "model_routing",
        label: "Model routing",
        change: "Let promoted model lessons update scorecard-informed routing proposals.",
        consumer: "Harness",
        risk: "Cannot change routing or provider behavior in v0."
      }
    ];
  }
  return [
    {
      id: "review_depth",
      label: "Review depth",
      change: "Let promoted lessons suggest whether future work needs more review.",
      consumer: "Learn",
      risk: "Cannot reduce human review without approval."
    }
  ];
}

function memoryTargetFor(record) {
  if (record.surface === "Forge") {
    return {
      type: "decision_record",
      path: "generated/learning/forge-outcome-memory-targets.json"
    };
  }
  if (record.surface === "Harness" || record.surface === "Models") {
    return {
      type: "model_scorecard",
      path: "generated/learning/model-worker-scorecard-targets.json"
    };
  }
  if (record.surface === "Quality") {
    return {
      type: "generated_contract",
      path: "generated/learning/evaluation-weighting-targets.json"
    };
  }
  if (record.surface === "Learn" || record.surface === "MDx") {
    return {
      type: "generated_contract",
      path: "generated/learning/mdx-learning-loop-contract.json"
    };
  }
  return {
    type: "page",
    path: "docs/MDX-LEARNING-LOOP.md"
  };
}

function judgmentStateFor(promotion) {
  // A kernel-drafted candidate cites the source receipt that produced it, so
  // the accountable call can be made as soon as a person has read it.
  if (promotion.source_receipt_id) return "ready_for_judgment";
  return promotion.state === "ready_for_judgment" ? "ready_for_judgment" : "not_ready";
}

function judgmentMissingEvidenceFor(promotion) {
  const missing = [...promotion.missing_evidence];
  missing.push("accountable actor", "judgment receipt");
  return [...new Set(missing)];
}

function judgmentNextStepFor(promotion) {
  if (promotion.state === "ready_for_judgment") {
    return "Prepare the judgment receipt before any memory promotion action exists.";
  }
  return "Collect the missing evidence before judgment can be recorded.";
}

function decisionLabelFor(decision) {
  if (decision === "promote_candidate") return "Ready for memory";
  if (decision === "reject_candidate") return "Rejected";
  if (decision === "supersede_candidate") return "Superseded";
  return decision || "Recorded";
}

function memoryQueueLabelFor(state) {
  if (state === "ready_for_memory") return "Ready for memory";
  if (state === "rejected_no_memory") return "No memory";
  if (state === "superseded_no_memory") return "Superseded";
  return state || "Blocked";
}

function memoryTargetLabelFor(type) {
  if (type === "page") return "Page";
  if (type === "decision_record") return "Decision record";
  if (type === "generated_contract") return "Generated contract";
  if (type === "source_map") return "Source map";
  if (type === "model_scorecard") return "Model scorecard";
  if (type === "worker_scorecard") return "Worker scorecard";
  return type || "Memory target";
}

function promotionStatusFor({ run, checksKnown, hasFailures, missingEvidence }) {
  if (run.status === "running") return "not_promotable";
  if (!checksKnown) return "not_promotable";
  if (hasFailures) return "candidate";
  const stillMissing = missingEvidence.filter((item) => item !== "human judgment");
  return stillMissing.length === 0 ? "ready_for_judgment" : "candidate";
}
