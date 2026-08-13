// The fleet surface, read-side. A fleet is one spec built by parallel
// agents: a plan (the streams and their seams) joined with its run (the
// live lanes, the integration result, the reviewer's verdict). These
// helpers fold the two kernel projections into one card per fleet.

const LANE_LABEL = {
  working: "Working",
  done: "Done",
  needs_attention: "Needs attention"
};

export function laneLabel(state) {
  return LANE_LABEL[state] ?? "Waiting";
}

// One card per fleet: the latest plan, joined with its run when started.
export function joinFleets(plans, runs) {
  const planRows = Array.isArray(plans?.fleets) ? plans.fleets : [];
  const runRows = Array.isArray(runs?.fleet_runs) ? runs.fleet_runs : [];
  const runByFleet = new Map(runRows.map((row) => [String(row?.fleet_id ?? ""), row]));
  // Projections can contain more than one lifecycle row for a fleet while a
  // poll crosses a draft/ratify boundary. Fold them before rendering so the
  // operator sees the latest state as one fleet, never duplicate cards.
  const planByFleet = new Map();
  for (const row of planRows) {
    const fleetId = String(row?.fleet_id ?? "");
    if (fleetId) planByFleet.set(fleetId, row);
  }
  return [...planByFleet.values()].map((plan) => {
    const fleetId = String(plan?.fleet_id ?? "");
    const run = runByFleet.get(fleetId) ?? null;
    const streams = Array.isArray(plan?.streams) ? plan.streams.map(normalizeStream) : [];
    const lanes = Array.isArray(run?.lanes) ? run.lanes.map(normalizeLane) : [];
    // Streams that have not started yet still deserve a lane row.
    for (const stream of streams) {
      if (!lanes.some((lane) => lane.streamId === stream.streamId)) {
        lanes.push({ streamId: stream.streamId, state: "waiting", forgeRunId: "", detail: "" });
      }
    }
    return {
      fleetId,
      spec: String(plan?.spec ?? ""),
      justification: String(plan?.justification ?? ""),
      plannerModel: String(plan?.planner_model ?? ""),
      repoId: String(plan?.repo_id ?? ""),
      repoPrimaryLanguage: String(plan?.repo_primary_language ?? ""),
      languagePackId: String(plan?.language_pack_id ?? ""),
      repoProfileSource: String(plan?.repo_profile_source ?? ""),
      repoSuggestedChecks: splitCsv(plan?.repo_profile_suggested_checks),
      repoArtifactPatterns: splitCsv(plan?.repo_profile_artifact_patterns),
      repoSemanticIntelligence: splitCsv(plan?.repo_profile_semantic_intelligence),
      repoSemanticToolReadiness: splitCsv(plan?.repo_profile_semantic_tool_readiness),
      repoToolchainReadiness: splitCsv(plan?.repo_profile_toolchain_readiness),
      repoProofPlanStatus: String(plan?.repo_profile_proof_plan_status ?? ""),
      repoProofPlanSummary: String(plan?.repo_profile_proof_plan_summary ?? ""),
      ratified: String(plan?.status ?? "") === "ratified",
      streams,
      executionGeometry: normalizeExecutionGeometry(plan?.execution_geometry ?? run?.proof?.execution_geometry),
      started: Boolean(run),
      running: Boolean(run?.running),
      finished: Boolean(run?.finished),
      finalDetail: String(run?.final_detail ?? ""),
      integrationState: String(run?.integration_state ?? ""),
      integrationDetail: String(run?.integration_detail ?? ""),
      reviewVerdict: String(run?.review_verdict ?? ""),
      proof: normalizeProof(run?.proof),
      // The reviewer's read of the PLAN before any build spend (wide plans only).
      planReview: normalizePlanReview(plan?.wide_plan_review),
      planningProgress: normalizePlanningProgress(plan?.planning_progress),
      fleetBranch: branchFrom(String(run?.final_detail ?? "")),
      lanes,
      brief: castingBrief(streams),
      phase: phaseFor(plan, run)
    };
  });
}

function normalizeProof(proof) {
  const row = proof && typeof proof === "object" ? proof : {};
  return {
    status: String(row?.status ?? ""),
    repoId: String(row?.repo_id ?? ""),
    repoPrimaryLanguage: String(row?.repo_primary_language ?? ""),
    languagePackId: String(row?.language_pack_id ?? ""),
    streamCount: Number(row?.stream_count ?? 0),
    laneDoneCount: Number(row?.lane_done_count ?? 0),
    laneAttentionCount: Number(row?.lane_attention_count ?? 0),
    declaredCheckCount: Number(row?.declared_check_count ?? 0),
    artifactFilterCount: Number(row?.artifact_filter_count ?? 0),
    semanticSignalCount: Number(row?.semantic_signal_count ?? 0),
    executionGeometry: normalizeExecutionGeometry(row?.execution_geometry),
    repoIntelligenceRecorded: Boolean(row?.repo_intelligence_recorded),
    disjointStreamsValidated: Boolean(row?.disjoint_streams_validated),
    proofCommandsDeclared: Boolean(row?.proof_commands_declared),
    artifactFiltersRecorded: Boolean(row?.artifact_filters_recorded),
    semanticOrientationDeclared: Boolean(row?.semantic_orientation_declared),
    integrationReviewed: Boolean(row?.integration_reviewed),
    revisionRepaired: Boolean(row?.revision_repaired),
    laneRevisionRepaired: Boolean(row?.lane_revision_repaired),
    principalEngineerEvidenceReady: Boolean(row?.principal_engineer_evidence_ready),
    missingGates: Array.isArray(row?.missing_gates) ? row.missing_gates.map(String) : [],
    runtimeRecovery: normalizeRecovery(row?.runtime_recovery)
  };
}

// The plan reviewer's pre-spend read (wide plans, width >= 8). Recorded on the
// plan receipt; surfaced so human ratification is an informed gate, not a
// rubber stamp. Grants no execution authority - the human still decides.
function normalizePlanReview(row) {
  const review = row && typeof row === "object" ? row : {};
  return {
    required: Boolean(review?.required),
    status: String(review?.status ?? ""),
    reviewerModel: String(review?.reviewer_model ?? ""),
    verdict: String(review?.verdict ?? ""),
    confidence: String(review?.confidence ?? ""),
    concerns: String(review?.concerns ?? "")
  };
}

function normalizePlanningProgress(row) {
  const progress = row && typeof row === "object" ? row : {};
  return {
    stage: String(progress?.stage ?? ""),
    detail: String(progress?.detail ?? ""),
    plannerModel: String(progress?.planner_model ?? ""),
    attempt: Number(progress?.attempt ?? 0)
  };
}

// The budgeted self-heal state: how many repairs Forge spent, what's left, and
// whether it stopped and handed the decision back to a human.
function normalizeRecovery(row) {
  const recovery = row && typeof row === "object" ? row : {};
  return {
    recoveryClass: String(recovery?.recovery_class ?? ""),
    nextAction: String(recovery?.next_action ?? ""),
    recoveryHealth: String(recovery?.recovery_health ?? ""),
    interruptRequired: Boolean(recovery?.interrupt_required),
    repairAttemptCount: Number(recovery?.repair_attempt_count ?? 0),
    maxRepairAttempts: Number(recovery?.max_repair_attempts ?? 0),
    repairBudgetRemaining: Number(recovery?.repair_budget_remaining ?? 0)
  };
}

function normalizeExecutionGeometry(row) {
  const geometry = row && typeof row === "object" ? row : {};
  return {
    strategy: String(geometry?.strategy ?? ""),
    requestedWidth: Number(geometry?.requested_width ?? 0),
    streamCount: Number(geometry?.stream_count ?? 0),
    startedLaneCount: Number(geometry?.started_lane_count ?? 0),
    maxConcurrentWorkers: Number(geometry?.max_concurrent_workers ?? 0),
    disjointWriteScopesValidated: Boolean(geometry?.disjoint_write_scopes_validated),
    integrationOwnedPathCount: Number(geometry?.integration_owned_path_count ?? 0),
    streamCheckCount: Number(geometry?.stream_check_count ?? 0),
    fullSuiteCheckCount: Number(geometry?.full_suite_check_count ?? 0),
    dependencyEdgeCount: Number(geometry?.dependency_edge_count ?? 0),
    maxTurnBudget: Number(geometry?.max_turn_budget ?? 0),
    byCoder: objectCounts(geometry?.by_coder),
    bySensitivity: objectCounts(geometry?.by_sensitivity)
  };
}

function objectCounts(row) {
  if (!row || typeof row !== "object") return [];
  return Object.entries(row).map(([label, count]) => ({ label, count: Number(count ?? 0) }));
}

function splitCsv(value) {
  return String(value ?? "")
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function normalizeStream(row) {
  return {
    streamId: String(row?.stream_id ?? ""),
    objective: String(row?.objective ?? ""),
    writeScope: Array.isArray(row?.write_scope) ? row.write_scope.map(String) : [],
    contract: String(row?.interface_contract ?? ""),
    dependsOn: Array.isArray(row?.depends_on) ? row.depends_on.map(String) : [],
    checks: Array.isArray(row?.checks) ? row.checks.map(String) : [],
    // The harness's casting decision for this stream: which coder, and how
    // sensitive its data is. Surfaced so the choice is legible, not hidden.
    builderSlot: String(row?.builder_slot ?? ""),
    dataSensitivity: String(row?.data_sensitivity ?? "")
  };
}

// A friendly name for a coder slot. Empty is the cheap default coder; a slot
// token names a stronger model. Engineers want to see who built what.
export function coderLabel(slot) {
  const name = String(slot ?? "").trim();
  if (!name) return "default coder";
  return { OPUS: "Opus", SONNET: "Sonnet", CODEXMINI: "GPT-5 mini" }[name] ?? name;
}

// The plan's casting at a glance, derived from the streams - the same shape
// the human ratifies against: who builds what, how many touch sensitive
// data, and whether that triggers a diverse review panel.
export function castingBrief(streams) {
  const byCoder = new Map();
  let sensitive = 0;
  for (const stream of streams) {
    const coder = coderLabel(stream.builderSlot);
    byCoder.set(coder, (byCoder.get(coder) ?? 0) + 1);
    if (String(stream.dataSensitivity).toLowerCase() === "sensitive") sensitive += 1;
  }
  return {
    coders: [...byCoder.entries()].map(([coder, count]) => ({ coder, count })),
    sensitive,
    panel: sensitive > 0
  };
}

// The plan reviewer's verdict, as one human line. Empty when no review ran
// (plans narrower than the wide-plan threshold skip it).
export function planReviewLine(review) {
  if (!review?.required) return { show: false, tone: "", line: "", concerns: "" };
  const reviewer = review.reviewerModel ? ` (${review.reviewerModel})` : "";
  const concerns = review.concerns?.trim() ?? "";
  if (review.status !== "recorded") {
    return { show: true, tone: "muted", line: `A pre-build plan review was required${reviewer} but did not complete.`, concerns: "" };
  }
  const verdict = String(review.verdict || "").toLowerCase();
  const confidence = review.confidence && review.confidence !== "none" ? `, ${review.confidence} confidence` : "";
  if (verdict === "ready") {
    return { show: true, tone: "ok", line: `A reviewer read this plan before any build spend and found it ready to run${confidence}.`, concerns };
  }
  if (verdict === "revise") {
    return { show: true, tone: "warn", line: `A reviewer read this plan before any build spend and suggested changes${confidence}.`, concerns };
  }
  return { show: true, tone: "muted", line: `A reviewer read this plan before any build spend${confidence}.`, concerns };
}

// The budgeted self-heal, as one human line. The loud case is escalation:
// Forge spent its repair budget, then stopped and handed the call to a person.
export function repairSummary(proof) {
  const recovery = proof?.runtimeRecovery;
  if (!recovery) return { show: false, tone: "", line: "" };
  const used = recovery.repairAttemptCount;
  const max = recovery.maxRepairAttempts;
  const escalated =
    proof.status === "needs_principal_attention" ||
    recovery.recoveryClass.endsWith("_exhausted") ||
    recovery.recoveryHealth === "exhausted";
  if (escalated) {
    const tried = max ? `tried ${used} of ${max} repairs within budget, then ` : "";
    return {
      show: true,
      tone: "attention",
      line: `Forge ${tried}stopped and handed this to you.${recovery.nextAction ? ` Next: ${recovery.nextAction}.` : ""}`
    };
  }
  if (proof.revisionRepaired || proof.laneRevisionRepaired) {
    return { show: true, tone: "ok", line: `Forge repaired this within budget (${used} of ${max} attempts) - ready for your review.` };
  }
  if (used > 0) {
    return { show: true, tone: "warn", line: `Repairing - attempt ${used} of ${max}.` };
  }
  return { show: false, tone: "", line: "" };
}

function normalizeLane(row) {
  return {
    streamId: String(row?.stream_id ?? ""),
    state: String(row?.state ?? "waiting"),
    forgeRunId: String(row?.forge_run_id ?? ""),
    detail: String(row?.detail ?? "")
  };
}

// When a fleet lands only partially, the integration step can fail to produce
// one branch. Mirrors the macOS FleetRun.hasIntegrationFailure.
export function fleetHasIntegrationFailure(fleet) {
  const state = String(fleet?.integrationState ?? "");
  return state === "did_not_land" || /no branch/i.test(String(fleet?.integrationDetail ?? ""));
}

// A fleet needs recovery when a lane stopped short or the branches did not come
// together. Only meaningful once the fleet has started. Mirrors
// FleetRun.needsRecovery.
export function fleetNeedsRecovery(fleet) {
  if (!fleet?.started) return false;
  const stuck = (fleet.lanes ?? []).some((lane) => lane.state === "needs_attention");
  return stuck || fleetHasIntegrationFailure(fleet);
}

// The lanes a mission would carry forward: everything that has not finished
// cleanly (stuck lanes and lanes that never landed), plus their streams so the
// mission inherits the right write scope and checks.
export function fleetRemainderLanes(fleet) {
  const streamById = new Map((fleet?.streams ?? []).map((stream) => [stream.streamId, stream]));
  return (fleet?.lanes ?? [])
    .filter((lane) => lane.state !== "done")
    .map((lane) => ({ ...lane, stream: streamById.get(lane.streamId) ?? null }));
}

// Turn a partially-landed fleet into a pre-filled mission draft: the stuck
// remainder becomes the goal, the remaining lanes' write scopes and checks are
// carried over, and the budget scales with the plan's width. Shown to the
// operator before anything is created, so they see exactly what will happen.
// Web port of macOS prepareMissionFromFleetPlan, scoped to the remainder.
export function prepareMissionFromFleet(fleet) {
  const remainder = fleetRemainderLanes(fleet);
  const integrationFailed = fleetHasIntegrationFailure(fleet);
  const width = Math.max(1, Number(fleet?.executionGeometry?.requestedWidth || fleet?.executionGeometry?.streamCount || (fleet?.streams ?? []).length || 1));

  const writeScope = [];
  for (const lane of remainder) {
    for (const path of lane.stream?.writeScope ?? []) {
      if (path && !writeScope.includes(path)) writeScope.push(path);
    }
  }
  const checks = [];
  for (const check of fleet?.repoSuggestedChecks ?? []) {
    if (check && !checks.includes(check)) checks.push(check);
  }
  for (const lane of remainder) {
    for (const check of lane.stream?.checks ?? []) {
      if (check && !checks.includes(check)) checks.push(check);
    }
  }

  const goal = integrationFailed
    ? `Bring the fleet's work together and finish what did not land: ${fleet?.spec ?? ""}`.trim()
    : `Finish the lanes that stopped short: ${fleet?.spec ?? ""}`.trim();

  return {
    fleetId: String(fleet?.fleetId ?? ""),
    repoId: String(fleet?.repoId ?? ""),
    integrationFailed,
    remainderLaneIds: remainder.map((lane) => lane.streamId),
    goal,
    doneWhen: "The carried-over lanes pass their checks, integration produces one branch, and the mission records its receipts.",
    validationCommands: checks,
    allowedWriteScope: writeScope,
    blockedPaths: ["production writes", "deploy", "ship authority"],
    nonGoals: "Do not restart the whole fleet - carry only the lanes that did not land cleanly.",
    fleetWidth: Math.max(1, remainder.length || width),
    maxCostCents: Math.max(1000, Math.min(50000, width * 500)),
    maxRuntimeMs: (width >= 8 ? 8 : 4) * 3_600_000,
    checkpointCadenceMinutes: 30
  };
}

// The POST body for /forge/long-horizon-missions.json from a prepared draft.
// Kept pure and testable; the write scope and checks join into the comma form
// the route reads.
export function buildMissionCreatePayload(draft, actor = "local_user") {
  return {
    goal: draft.goal,
    done_when: draft.doneWhen,
    validation_commands: (draft.validationCommands ?? []).join(", "),
    allowed_write_scope: (draft.allowedWriteScope ?? []).join(", "),
    blocked_paths: (draft.blockedPaths ?? []).join(", "),
    non_goals: draft.nonGoals,
    fleet_width: Math.max(1, Number(draft.fleetWidth ?? 1)),
    max_cost_cents: Math.max(1, Number(draft.maxCostCents ?? 1000)),
    max_runtime_ms: Math.max(60_000, Number(draft.maxRuntimeMs ?? 3_600_000)),
    checkpoint_cadence_minutes: Math.max(1, Number(draft.checkpointCadenceMinutes ?? 30)),
    actor_id: actor
  };
}

// Where this fleet stands, as one word the page renders a card around.
function phaseFor(plan, run) {
  if (run?.finished) return "finished";
  if (run?.running) return "building";
  const status = String(plan?.status ?? "");
  if (status === "ratified") return "ready";
  if (status === "planning_needs_attention") return "planning_needs_attention";
  if (status === "planning_delayed") return "planning_delayed";
  if (status === "planning") return "planning";
  return "proposed";
}

export function phaseLabel(phase) {
  return (
    {
      proposed: "Plan proposed - your call",
      planning: "Planning the fleet",
      planning_delayed: "Planner is taking longer",
      planning_needs_attention: "Plan needs attention",
      ready: "Ratified - ready to build",
      building: "Building",
      finished: "Finished"
    }[phase] ?? phase
  );
}

// The fleet branch rides the run's final rollup line.
function branchFrom(finalDetail) {
  const match = /fleet_branch=(\S+)/.exec(finalDetail);
  return match ? match[1] : "";
}

// The reviewer's verdict, split from its findings. A high-stakes fleet is
// read by a PANEL of diverse models; we detect that and break out each
// member's own verdict ("grok-4.3=ready; gpt-5-mini=ready" -> chips) so the
// diverse review is legible: you see WHO read it and what each one thought.
export function reviewSummary(verdict) {
  const text = String(verdict ?? "").trim();
  if (!text) return { line: "", findings: [], panel: false, ready: false, members: [] };
  const lines = text.split("\n").filter((line) => line.trim());
  const head = lines[0] ?? "";
  const panel = /^panel verdict:/i.test(head);
  const ready = /verdict:\s*ready/i.test(head);
  let members = [];
  const bracket = /\[([^\]]+)\]/.exec(head);
  if (panel && bracket) {
    members = bracket[1]
      .split(";")
      .map((part) => {
        const [model, ...rest] = part.split("=");
        return { model: model.trim(), verdict: rest.join("=").trim() };
      })
      .filter((m) => m.model);
  }
  // The clean panel headline without the bracket noise.
  const line = panel ? head.replace(/\s*\[[^\]]*\]\s*$/, "") : head;
  return { line, findings: lines.slice(1), panel, ready, members };
}
