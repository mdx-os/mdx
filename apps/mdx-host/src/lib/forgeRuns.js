// The Forge run viewer, read-side. A run is the build agent working: it
// reads, edits, runs the checks, and either lands a branch or stops
// honestly. The kernel's projection carries the event trail in its own
// grammar; these helpers translate that grammar into lines a person
// reads at a glance, and keep the raw event for the disclosure drawer.
import { diffLines } from "./codeHighlight.js";

const STATUS_LABEL = {
  running: "Working",
  plan_proposed: "Plan proposed - your call",
  done: "Done - branch ready",
  stopped: "Stopped - you ended it",
  cannot_proceed: "Stopped - it could not finish",
  budget_exhausted: "Stopped - ran out of turns",
  error: "Errored",
  finished: "Finished"
};

export function statusLabel(status) {
  return STATUS_LABEL[status] ?? "Working";
}

export function contextFill(run) {
  const telemetry = run?.contextTelemetry;
  const latest = telemetry?.latest;
  const peak = telemetry?.peak;
  const tokens = Number(latest?.input_tokens ?? 0);
  const window = Number(latest?.context_window ?? 0);
  if (!tokens || !window) return null;
  const peakTokens = Number(peak?.input_tokens ?? tokens);
  const peakWindow = Number(peak?.context_window ?? window);
  const multiModel = Number(telemetry?.total?.model_count ?? 0) > 1;
  const displayTokens = multiModel ? peakTokens : tokens;
  const displayWindow = multiModel ? peakWindow : window;
  return {
    tokens,
    window,
    pct: contextPct(displayTokens, displayWindow),
    label: `${multiModel ? "peak " : ""}${contextLabel(displayTokens, displayWindow)}`,
    title: `${multiModel ? "Peak" : "Latest"} context: ${displayTokens.toLocaleString()} / ${displayWindow.toLocaleString()} input tokens`
  };
}

function contextPct(tokens, window) {
  if (!tokens || !window) return 0;
  return Math.min(100, Math.max(1, Math.round((100 * tokens) / window)));
}

function contextLabel(tokens, window) {
  if (!tokens || !window) return "";
  return (100 * tokens) / window < 1 ? "<1%" : `${contextPct(tokens, window)}%`;
}

export function isRunning(status) {
  return status === "running";
}

export function normalizeRuns(projection) {
  const rows = Array.isArray(projection?.runs) ? projection.runs : [];
  return rows.map((row) => ({
    runId: String(row?.run_id ?? ""),
    workItemId: String(row?.work_item_id ?? ""),
    status: String(row?.status ?? "running"),
    turns: Number(row?.turns ?? 0),
    modelCalls: Number(row?.model_calls ?? 0),
    latestInputTokens: Number(row?.latest_input_tokens ?? 0),
    contextTelemetry: row?.context_telemetry ?? null,
    toolCalls: Number(row?.tool_calls ?? 0),
    checksPassed: Number(row?.checks_passed ?? 0),
    checksFailed: Number(row?.checks_failed ?? 0),
    branch: String(row?.branch ?? ""),
    // The branch tip the run committed, for ship ratification: the human
    // ratifies a commit, never a branch name.
    commitSha: String(row?.diff?.commit_sha ?? ""),
    finalLine: String(row?.final_line ?? ""),
    filesChanged: Number((String(row?.final_line ?? "").match(/files_changed=(\d+)/) || [])[1] ?? 0),
    model: modelFrom(row?.events),
    // What the finished run left behind for the next one: its outcome
    // disposition and the lesson it proposes (citation-only, never applied
    // on its own). Empty when the run has not produced an outcome yet.
    outcomeDisposition: String(row?.outcome_disposition ?? ""),
    lessonCandidate: String(row?.lesson_candidate ?? ""),
    repoIntake: normalizeRepoIntake(row?.repo_intake),
    executionGeometry: normalizeExecutionGeometry(row?.execution_geometry),
    parallelCandidate: normalizeParallelCandidate(row?.parallel_candidate),
    intent: String(row?.intent ?? ""),
    // The human voice, captured at the source: the operator's own ask (the run
    // title) and the agent's first-person summary of what it built. Empty for
    // runs recorded before capture existed; we fall back gracefully.
    runTitle: humanRunTitle(row),
    // Prefer the operator-facing voice rewrite; fall back to the builder's raw
    // factual summary when the voice pass did not run or was held by the claim
    // guard. Either way it is honest - the rewrite is tone-only.
    runSummary: String(row?.operator_run_summary || row?.run_summary || ""),
    voiceRewriteStatus: String(row?.voice_rewrite_status ?? ""),
    streamRoute: String(row?.stream_route ?? ""),
    operatorStatus: String(row?.operator_status ?? ""),
    stages: normalizeStages(row?.stages),
    controls: normalizeControls(row?.controls),
    // The Machine League additions (additive, absent or default for an
    // ordinary native run): which machine ran this, whether its output is held
    // until Forge's checks pass, and the league context that makes a trial run
    // legible. league_context is null for a native non-league run.
    runnerProfile: normalizeRunnerProfile(row?.runner_profile),
    quarantine: normalizeQuarantine(row?.quarantine),
    leagueContext: normalizeLeagueContext(row?.league_context),
    events: Array.isArray(row?.events) ? row.events.map(normalizeEvent) : []
  }));
}

function normalizeRunnerProfile(value) {
  if (!value || typeof value !== "object") return null;
  const id = String(value.runner_id ?? "");
  if (!id) return null;
  return {
    runnerId: id,
    runnerKind: String(value.runner_kind ?? ""),
    displayName: String(value.display_name ?? ""),
    adapterKind: String(value.adapter_kind ?? ""),
    executionMode: String(value.execution_mode ?? ""),
    modelProfileId: String(value.model_profile_id ?? ""),
    leadsHeader: value.leads_header === true,
    modelDisclosedSecond: value.model_disclosed_second === true
  };
}

function normalizeQuarantine(value) {
  if (!value || typeof value !== "object") return null;
  return {
    status: String(value.status ?? ""),
    outputQuarantined: value.output_quarantined === true,
    externalOutputConsumable: value.external_output_consumable === true,
    acceptanceGate: String(value.acceptance_gate ?? ""),
    resultProjectionRoute: String(value.result_projection_route ?? ""),
    blockedReason: String(value.blocked_reason ?? "")
  };
}

function normalizeLeagueContext(value) {
  if (!value || typeof value !== "object") return null;
  return {
    visibilityTier: String(value.visibility_tier ?? ""),
    recommendationRationale: String(value.recommendation_rationale ?? ""),
    fallbackRunnerId: String(value.fallback_runner_id ?? ""),
    scorecardEvidenceCount: String(value.scorecard_evidence_count ?? ""),
    quarantinePosture: String(value.quarantine_posture ?? "")
  };
}

// A league/trial run is one Forge ran on a machine other than the house
// builder, or any run carrying league context. For these the header leads with
// the runner and the model sits one disclosure deeper; native runs are
// untouched (runnerHeader returns "").
export function isLeagueRun(run) {
  return !!run?.leagueContext || (!!run?.runnerProfile && run.runnerProfile.runnerKind !== "mdx_native");
}
export function runnerHeader(run) {
  const rp = run?.runnerProfile;
  if (!rp || !isLeagueRun(run)) return "";
  return rp.displayName || rp.runnerId;
}
// The held-output boundary line for a trial run, human first. Empty when the
// run is native or its output is not held.
export function quarantineLine(run) {
  const q = run?.quarantine;
  if (!q || !q.outputQuarantined) return "";
  return "Held in an isolated copy. Nothing it produced counts until Forge's checks accept it.";
}
// Whether a trial actually ran the external machine (live execution) versus
// only staging a held packet. The activity trail carries the exit codes; this
// is the at-a-glance ran-vs-staged distinction.
export function trialExecuted(run) {
  const m = run?.runnerProfile?.executionMode || "";
  return m.includes("live") || m.includes("executed");
}
// Whether a trial passed visible and hidden checks and a person accepted it
// into the scorecard. The kernel now folds acceptance into the run's quarantine
// status (the clean signal); the receipt events are the fallback. Acceptance
// never makes the output consumable - it counts as evidence, held from
// production.
export function trialAccepted(run) {
  if (/accepted/.test(run?.quarantine?.status || "")) return true;
  return (run?.events || []).some(
    (e) => /accepted_for_scoreboard/.test(e.detail || "") || /accepted_for_scoreboard/.test(e.kind || "")
  );
}

const OPERATOR_STATUS_LABEL = {
  working: "Working",
  needs_you: "Needs you",
  ready_for_review: "Ready to review",
  paused: "Paused",
  stopped: "Stopped"
};
export function operatorStatusLabel(run) {
  return OPERATOR_STATUS_LABEL[run?.operatorStatus] ?? statusLabel(run?.status);
}

function normalizeStages(value) {
  if (!Array.isArray(value)) return [];
  return value
    .map((s) => ({
      key: String(s?.key ?? ""),
      label: String(s?.label ?? ""),
      state: String(s?.state ?? "pending")
    }))
    .filter((s) => s.key);
}

function normalizeControls(value) {
  if (!Array.isArray(value)) return {};
  const map = {};
  for (const c of value) {
    const action = String(c?.action ?? "");
    if (!action) continue;
    map[action] = { allowed: Boolean(c?.allowed), route: String(c?.route ?? "") };
  }
  return map;
}

export function controlAllowed(run, action) {
  const controls = run?.controls;
  // Fail closed: a row without kernel-declared controls (snapshot-restored
  // history) must not render live verbs that would only refuse when clicked.
  if (!controls || Object.keys(controls).length === 0) return false;
  return controls[action]?.allowed === true;
}

// A run must never render nameless: prefer the operator's own words, then
// the run's summary voice, then an honest short handle from its id.
function humanRunTitle(row) {
  const direct = String(row?.run_title ?? row?.title ?? row?.operator_intent ?? row?.intent ?? "").trim();
  if (direct) return direct;
  const summary = String(row?.operator_run_summary || row?.run_summary || "").trim();
  if (summary) {
    const firstSentence = summary.split(/(?<=[.!?])\s/)[0] ?? summary;
    return firstSentence.length > 96 ? `${firstSentence.slice(0, 93)}...` : firstSentence;
  }
  const id = String(row?.run_id ?? "");
  const tail = id.match(/(\d+)\s*$/)?.[1];
  return tail ? `Run ${tail}` : id || "Forge run";
}

function normalizeRepoIntake(value) {
  return {
    generatedFrom: String(value?.generated_from ?? ""),
    readinessStatus: String(value?.readiness_status ?? ""),
    safeNextMove: String(value?.safe_next_move ?? ""),
    semanticOrientationOperations: arrayOfStrings(value?.semantic_orientation_operations),
    scoutCandidateCount: Number(value?.scout_candidate_count ?? 0),
    writeScopeHint: arrayOfStrings(value?.write_scope_hint),
    sourceHost: String(value?.source_host ?? "")
  };
}

function normalizeExecutionGeometry(value) {
  return {
    requestedWorkers: Number(value?.requested_workers ?? 1),
    effectiveWorkers: Number(value?.effective_workers ?? 1),
    lane: String(value?.lane ?? ""),
    route: String(value?.route ?? ""),
    reason: String(value?.reason ?? ""),
    fleetRequired: value?.fleet_required === true,
    grantsExecutionAuthority: value?.grants_execution_authority === true
  };
}

function normalizeParallelCandidate(value) {
  return {
    role: String(value?.role ?? ""),
    primaryRunId: String(value?.primary_run_id ?? ""),
    index: Number(value?.index ?? 1),
    count: Number(value?.count ?? 1),
    writeScope: arrayOfStrings(value?.write_scope),
    grantsExecutionAuthority: value?.grants_execution_authority === true
  };
}

function arrayOfStrings(value) {
  return Array.isArray(value) ? value.map(String).filter(Boolean) : [];
}

// Which coder drove this run - parsed from its first model_called event, so
// the trace shows WHO built it. This is the thread that ties a run back to
// the casting decision (Fleets) and the model's track record (Models).
function modelFrom(events) {
  for (const event of Array.isArray(events) ? events : []) {
    if (String(event?.event ?? "") !== "model_called") continue;
    const match = /model=(\S+)/.exec(String(event?.detail ?? ""));
    if (match) return match[1];
  }
  return "";
}

// One event, with a human line. The grammar stays in the raw fields for
// the disclosure drawer; the line is what a person reads.
function normalizeEvent(event) {
  const kind = String(event?.event ?? "");
  const detail = String(event?.detail ?? "");
  return {
    turn: Number(event?.turn ?? 0),
    kind,
    detail,
    line: humanLine(kind, detail),
    tone: toneFor(kind)
  };
}

// Translate the DXR event grammar into plain language. The detail already
// carries the tool name or command; this frames it.
function humanLine(kind, detail) {
  switch (kind) {
    case "run_started":
      return "Started the run";
    case "model_called":
      return "Thinking about the next step";
    case "turn_executed":
      return "";
    case "tool_executed":
      return toolLine(detail);
    case "check_passed":
      return `Check passed - ${commandFrom(detail)}`;
    case "check_failed":
      return `Check failed - ${commandFrom(detail)}`;
    case "evidence_appended":
      if (detail.startsWith("branch=")) {
        return `Committed the work to a branch`;
      }
      if (detail.startsWith("finish=")) {
        return detail.includes("done") ? "Declared the work done" : "Said it could not proceed";
      }
      if (detail.startsWith("building to your standards:")) {
        return `Building to your standards: ${detail.replace("building to your standards:", "").trim()}`;
      }
      // The preparation beats (walkthrough finding: these rendered as a wall
      // of identical "Recorded evidence" lines). Each family gets its own
      // words; anything unknown shows its own detail humanized, never a
      // fixed placeholder.
      if (detail.startsWith("semantic_strategy_assigned")) return "Chose how to read this change";
      if (detail.startsWith("phase=intake")) return "Read the repo and sized up the work";
      if (detail.startsWith("context assembled")) return `Assembled context from the repo${detail.match(/\((.+)\)/)?.[1] ? ` (${detail.match(/\((.+)\)/)[1]})` : ""}`;
      if (detail.startsWith("flywheel context assembled")) return "Pulled in past outcomes and memories";
      // The already-receipted citation: this run drew on lessons a person
      // promoted. Say it in human terms; the matched ids stay in the raw
      // detail for the disclosure drawer.
      if (detail.startsWith("active learning memory cited")) {
        const n = Number(detail.match(/advisory_count=(\d+)/)?.[1] ?? 0);
        return n === 1
          ? "Drew on 1 lesson you approved"
          : `Drew on ${n} lessons you approved`;
      }
      if (detail.startsWith("language pack guidance applied")) return `Applied ${detail.split(":").pop().trim()} conventions`;
      if (detail.startsWith("repo quality signals applied")) return "Applied this repo's quality signals";
      if (detail.startsWith("review axes")) return "Set the review lenses for this change";
      if (detail.startsWith("principles")) return "Loaded the working principles";
      if (detail.startsWith("selected_checks") || detail.startsWith("checks=")) return "Locked in the checks that must pass";
      {
        const head = detail.split(/[=:]/)[0].replaceAll("_", " ").trim();
        if (head && head.length <= 48) return `Noted ${head}`;
      }
      return "Recorded evidence";
    case "run_finished":
      return "Run finished";
    default:
      return detail;
  }
}

function toolLine(detail) {
  if (detail.startsWith("read_file")) return `Read ${detail.replace("read_file", "").trim()}`;
  if (detail.startsWith("write_file")) return `Edited ${detail.replace("write_file", "").trim()}`;
  if (detail.startsWith("search")) return `Searched for ${detail.replace("search", "").trim()}`;
  if (detail.startsWith("list_dir")) return `Looked in ${detail.replace("list_dir", "").trim() || "the root"}`;
  if (detail.includes("refused")) return `Tool refused: ${detail}`;
  return detail;
}

function commandFrom(detail) {
  const match = /run_command (.+?) exit=/.exec(detail);
  return match ? match[1] : detail;
}

function toneFor(kind) {
  if (kind === "check_passed" || (kind === "evidence_appended")) return "good";
  if (kind === "check_failed") return "bad";
  if (kind === "tool_executed") return "action";
  return "muted";
}

// The headline line for a run row, before it is opened.
export function summaryLine(run) {
  if (run.status === "done") {
    const parts = [`Done in ${run.turns} turns`];
    if (run.filesChanged) parts.push(`${run.filesChanged} file${run.filesChanged === 1 ? "" : "s"} changed`);
    parts.push(`${run.checksPassed} check${run.checksPassed === 1 ? "" : "s"} passed`);
    return `${parts.join(" · ")}, branch ready`;
  }
  if (run.status === "running") {
    if (run.turns === 0 && run.toolCalls === 0) return "Starting - waiting for the first recorded step";
    const parts = [];
    if (run.turns > 0) parts.push(`${run.turns} ${run.turns === 1 ? "turn" : "turns"}`);
    if (run.toolCalls > 0) parts.push(`${run.toolCalls} tool ${run.toolCalls === 1 ? "call" : "calls"}`);
    return `Working - ${parts.join(", ")}`;
  }
  return humanizeBlockedSummary(run);
}

// A run that stopped or could not finish must never show its raw telemetry
// tail (turns=0 files_changed=0 check_runs=1 check_duration_ms=177
// cost_cents=2) as its headline the way a finished run shows a clean summary.
// Say what happened in one plain sentence, preferring the backend's own reason
// with any key=value telemetry stripped off, and fall back to a human line per
// status.
export function humanizeBlockedSummary(run) {
  const raw = String(run?.blockedReason ?? run?.finalLine ?? "").trim();
  const clean = raw
    .replace(/^status=\w+\s*/, "")
    .replace(
      /\b(turns|files_changed|check_runs|check_duration_ms|cost_cents|input_tokens|output_tokens|spent_cents|max_cost_cents|elapsed_ms|max_runtime_ms)=\S+/g,
      ""
    )
    .replace(/\s{2,}/g, " ")
    .replace(/\s+([.,])/g, "$1")
    .trim();
  const looksHuman = clean && /\s/.test(clean) && !/^[\w-]+=/.test(clean);
  if (looksHuman) return clean;
  switch (run?.status) {
    case "cannot_proceed":
      return "Forge stopped before changing anything - its checks were already failing, so there was nothing safe to build on.";
    case "budget_exhausted":
      return "Forge ran out of its turn or cost budget before finishing - try a tighter scope or a higher budget.";
    case "stopped":
      return "You stopped this run.";
    case "no_change":
      return "Forge finished without making a change.";
    default:
      return statusLabel(run?.status);
  }
}

// What to show as a run's headline, before it is opened. Prefer the operator's
// own words (their ask). The backend currently puts run config metadata in the
// intent field (accepted: N selected_checks language_pack=...), which is not a
// human ask - so until the real ask is projected, fall back to a clean human
// summary instead of leaking that grammar onto the card.
export function runHeadline(run) {
  const title = String(run.runTitle ?? run.intent ?? "").trim();
  const looksLikeMetadata = /^accepted:|selected_checks|language_pack=|execution_geometry=|workers=/.test(title);
  if (title && !looksLikeMetadata) return title;
  return summaryLine(run);
}

// True when a run carries the operator's own ask as a title (so the card can
// show the ask as the headline and the outcome as a second line). False for
// pre-capture runs that only have metadata - those just show the summary.
export function hasHumanTitle(run) {
  const title = String(run.runTitle ?? run.intent ?? "").trim();
  return Boolean(title) && !/^accepted:|selected_checks|language_pack=|execution_geometry=|workers=/.test(title);
}

// Guided recovery for a run that stopped short or landed with a proof caveat.
// This is the web port of the macOS RunRecoverySurface: a red or stalled run
// should read as "here is what happened and here are your sane moves", never a
// dead end. Every branch is derived from the run's own receipts (events,
// status, checks, branch), so the copy stays honest - the kernel is fail-closed
// and its refusals surface as refusals.

// One run event as a single lowercased haystack, the way the Mac ForgeEvent
// predicates join kind + detail before matching.
function eventText(event) {
  return `${event?.kind ?? ""} ${event?.detail ?? ""}`.toLowerCase();
}

function isBaselineProofFailure(event) {
  const text = eventText(event);
  return (
    text.includes("baseline_run_command") &&
    (text.includes("check_failed") || text.includes("proof check failed") || text.includes("exit=1"))
  );
}

function isPostChangeProofPass(event) {
  const text = eventText(event);
  if (text.includes("baseline_run_command")) return false;
  return text.includes("proof check passed") || (text.includes("run_command") && text.includes("exit=0"));
}

function proofCommand(event) {
  const detail = String(event?.detail ?? "");
  return /(?:baseline_)?run_command\s+(.+?)\s+exit=/.exec(detail)?.[1]?.trim() ?? "";
}

function isProofFailureEvent(event) {
  const text = eventText(event);
  return (
    text.includes("check_failed") ||
    text.includes("proof check failed") ||
    text.includes("post-change proof") ||
    (text.includes("run_command") && text.includes("exit=1"))
  );
}

// A run's selected proof was already red before this run touched anything -
// so a failing check is a baseline health problem, not necessarily this
// change's fault. Read from the events and from the run's own final line.
function selectedProofRedOnArrival(run) {
  const marker = /red on arrival|already red before|pre-existing selected proof/i;
  if (marker.test(String(run?.blockedReason ?? "")) || marker.test(String(run?.finalLine ?? ""))) return true;
  return (run?.events ?? []).some((event) => isBaselineProofFailure(event) || marker.test(eventText(event)));
}

// The baseline was red, and a check passed after the change - the selected
// proof turned green on this branch.
function selectedProofTurnedGreen(run) {
  if (!selectedProofRedOnArrival(run)) return false;
  const failedCommands = new Set(
    (run?.events ?? [])
      .filter(isBaselineProofFailure)
      .map(proofCommand)
      .filter(Boolean)
  );
  return (run?.events ?? []).some(
    (event) => isPostChangeProofPass(event) && failedCommands.has(proofCommand(event))
  );
}

// The branch is still worth reviewing even though a check failed, because the
// selected proof was already red before this change landed.
function isReviewableWithProofCaveat(run) {
  const status = String(run?.status ?? "").toLowerCase();
  return (
    !isRunning(run?.status) &&
    Boolean(run?.branch) &&
    Number(run?.checksFailed ?? 0) > 0 &&
    selectedProofRedOnArrival(run) &&
    !status.includes("cannot") &&
    (status.includes("done") || status.includes("finished") || run?.operatorStatus === "ready_for_review")
  );
}

// The most recent failing-proof event's line, trimmed to something a person
// reads without scrolling.
function latestProofFailureLine(run) {
  const events = run?.events ?? [];
  for (let i = events.length - 1; i >= 0; i -= 1) {
    if (isProofFailureEvent(events[i])) {
      const source = String(events[i].detail || events[i].line || "").replace(/\s+/g, " ").trim();
      const words = source.split(" ");
      return words.length > 34 ? `${words.slice(0, 34).join(" ")}...` : source;
    }
  }
  return "";
}

// Runs that read as "needs a person": stopped short, or done with a proof that
// is not green. A clean finished run returns false (it flows to the normal ship
// path, not the recovery banner).
export function needsRecovery(run) {
  if (isRunning(run?.status) || run?.status === "plan_proposed") return false;
  if (isReviewableWithProofCaveat(run)) return true;
  const status = String(run?.status ?? "").toLowerCase();
  if (["cannot_proceed", "budget_exhausted", "error", "stopped", "interrupted"].includes(status)) return true;
  return Number(run?.checksFailed ?? 0) > 0;
}

// The recovery surface for one run: a calm title, one specific line about what
// happened and the sane next move, a pre-filled revision note, and the flags a
// banner uses to offer only the moves that make sense. Returns null when the
// run does not need recovery. Port of RunRecoverySurface.swift.
export function runRecovery(run) {
  if (!run || !needsRecovery(run)) return null;
  const interrupted = run.status === "interrupted";
  const running = isRunning(run.status);
  const status = String(run.status ?? "").toLowerCase();
  const proofCaveat = isReviewableWithProofCaveat(run);
  const baselineRed = selectedProofRedOnArrival(run);
  const turnedGreen = selectedProofTurnedGreen(run);
  const hasBranch = Boolean(run.branch);
  const checksFailed = Number(run.checksFailed ?? 0);
  const checksPassed = Number(run.checksPassed ?? 0);

  const title = (() => {
    if (interrupted) return "Forge paused after a restart";
    if (proofCaveat && turnedGreen) return "Ready for review: proof turned green";
    if (proofCaveat) return "Ready for review with a proof caveat";
    if (baselineRed) return "Proof started red";
    if (status.includes("cannot")) return "Forge needs a narrower next step";
    if (status.includes("exhausted")) return "Forge ran out of turns";
    if (checksFailed > 0) return "Proof is not green yet";
    return "Forge needs attention";
  })();

  const detail = (() => {
    if (checksFailed > 0) {
      const total = checksPassed + checksFailed;
      const checkLine = total > 0 ? `${checksFailed} of ${total} checks failed.` : `${checksFailed} checks failed.`;
      if (proofCaveat && turnedGreen) return "The selected check failed before the change and passes on this branch.";
      if (proofCaveat) return `${checkLine} The branch is still reviewable because the selected proof was already red before this change.`;
      if (baselineRed) return `${checkLine} The selected proof was already red before this run.`;
      const failLine = latestProofFailureLine(run);
      return failLine ? `${checkLine} ${failLine}` : checkLine;
    }
    const failLine = latestProofFailureLine(run);
    if (failLine) return failLine;
    return humanizeBlockedSummary(run);
  })();

  const recoveryLine = (() => {
    if (interrupted) {
      return "Your isolated workspace and transcript are safe. Resume from the last recorded step; Forge will rerun the interrupted check before it continues.";
    }
    if (running && turnedGreen) {
      return "The selected check failed before the change and now passes. Forge is finishing the evidence packet.";
    }
    if (running) return `${detail} Forge is still trying to work through it; steer only if the direction is wrong.`;
    if (proofCaveat && turnedGreen) {
      return `${detail} Review the diff and confirm the before-and-after proof matches the intended behavior. Request a revision only if it does not.`;
    }
    if (proofCaveat) {
      return `${detail} Review the diff, then request a focused revision only if the branch caused or worsened the failure.`;
    }
    if (baselineRed) {
      return `${detail} Treat this as baseline health first: isolate or repair the failing check, then return to the original ask.`;
    }
    if (hasBranch) return `${detail} Review what it left on the branch, then ask for a focused revision.`;
    return `${detail} Start a narrower run so the next attempt has a smaller proof target.`;
  })();

  const [revisionControlLabel, suggestedRevisionNote] = (() => {
    if (proofCaveat && turnedGreen) {
      return [
        "Request revision",
        "The selected proof turned green on this branch. Review whether the diff and before-and-after evidence match the intended behavior, then revise only what remains wrong: "
      ];
    }
    if (proofCaveat) {
      return [
        "Request revision",
        "Review this branch with the proof caveat in mind. If the failure is caused by this change, fix it; otherwise keep the change focused and note the existing red proof: "
      ];
    }
    if (baselineRed) {
      return ["Repair baseline", "Pick this back up by isolating the baseline failure first: "];
    }
    if (checksFailed > 0) {
      return ["Pick it back up", "Pick this back up by focusing on the failing proof first: "];
    }
    return ["Pick it back up", "Pick this back up with a narrower next step: "];
  })();

  return {
    title,
    recoveryLine,
    branchIdentity: hasBranch ? String(run.branch).trim() : "",
    revisionControlLabel,
    suggestedRevisionNote,
    statusPillLabel: running ? "Still working" : recoveryDisplayStatus(run),
    showsOpenDiff: hasBranch,
    showsRevisionControl: !running && (controlAllowed(run, "revise") || hasBranch),
    showsResume: interrupted,
    // "Start smaller" is only the right lead when there is no branch to review;
    // otherwise the review-and-revise path is the calmer first move.
    showsStartSmaller: !running && !hasBranch,
    isProofCaveat: proofCaveat,
    isBaselineRed: baselineRed,
    proofTurnedGreen: turnedGreen
  };
}

function recoveryDisplayStatus(run) {
  const status = String(run?.status ?? "").toLowerCase();
  if (status === "running") return "Working";
  if (status.includes("cannot")) return "Needs a narrower next step";
  if (status.includes("exhausted")) return "Out of turns";
  if (status.includes("done") || status.includes("finished")) return "Done";
  if (run?.operatorStatus === "ready_for_review") return "Ready";
  return statusLabel(run?.status);
}

// The invariant the macOS side keeps (keepsFailedBranchRecoveryReadable): a
// failed run that left a branch must always render a readable recovery with a
// revision control, and a proof-caveat run must name the caveat and offer a
// revision. Kept as a web equivalent so recovery can never silently collapse
// into a dead end.
export function keepsFailedBranchRecoveryReadable(surface) {
  if (!surface) return false;
  if (!surface.branchIdentity) return false;
  if (!surface.showsRevisionControl || !surface.revisionControlLabel.trim()) return false;
  if (!surface.title.trim() || !surface.recoveryLine.trim()) return false;
  if (surface.isProofCaveat) {
    return (
      (/proof caveat/i.test(surface.title) || /turned green/i.test(surface.title)) &&
      /revision/i.test(surface.recoveryLine) &&
      surface.revisionControlLabel === "Request revision"
    );
  }
  return /branch/i.test(surface.recoveryLine) || /revision/i.test(surface.recoveryLine);
}

// Parallel execution groups from the runs projection. A wide build runs many
// candidates in parallel; the projection keeps one row per candidate, but the
// operator list should show one build-level row per group so a width-24 fleet
// does not flood the list. Normalized here; the representative logic lives in
// runListRepresentatives.
export function normalizeParallelGroups(projection) {
  const rows = Array.isArray(projection?.parallel_execution_groups) ? projection.parallel_execution_groups : [];
  return rows
    .map((row) => ({
      primaryRunId: String(row?.primary_run_id ?? ""),
      requestedWorkers: Number(row?.requested_workers ?? 1),
      effectiveWorkers: Number(row?.effective_workers ?? 1),
      lane: String(row?.lane ?? ""),
      plannedCandidateCount: Number(row?.planned_candidate_count ?? 0),
      observedCandidateCount: Number(row?.observed_candidate_count ?? 0),
      finishedCandidateCount: Number(row?.finished_candidate_count ?? 0),
      runningCandidateCount: Number(row?.running_candidate_count ?? 0),
      doneCandidateCount: Number(row?.done_candidate_count ?? 0),
      noChangeCandidateCount: Number(row?.no_change_candidate_count ?? 0),
      cannotProceedCandidateCount: Number(row?.cannot_proceed_candidate_count ?? 0),
      failedCandidateCount: Number(row?.failed_candidate_count ?? 0),
      checksPassedTotal: Number(row?.checks_passed_total ?? 0),
      checksFailedTotal: Number(row?.checks_failed_total ?? 0),
      selectionStatus: String(row?.selection_status ?? ""),
      recommendedRunId: String(row?.recommended_run_id ?? ""),
      candidateIds: (Array.isArray(row?.candidates) ? row.candidates : []).map((c) => String(c?.run_id ?? "")).filter(Boolean)
    }))
    .filter((group) => group.primaryRunId);
}

// A group is "wide" when it genuinely ran more than one lane. Single-worker
// runs never collapse.
export function isWideGroup(group) {
  return (
    Number(group?.effectiveWorkers ?? 1) > 1 ||
    Number(group?.requestedWorkers ?? 1) > 1 ||
    Number(group?.observedCandidateCount ?? 0) > 1
  );
}

function groupContainsRun(group, runId) {
  return group?.primaryRunId === runId || (group?.candidateIds ?? []).includes(runId);
}

// One row per build: single runs pass through untouched; a wide parallel group
// collapses to a single representative. The representative is the group's
// primary, unless the operator is focused on a specific candidate (a deep link
// or the run they just started), in which case that candidate leads so the
// deep link lands exactly where it pointed. Port of RunListGrouping.swift, with
// the focused-candidate promotion the web deep-link idiom needs.
export function runListRepresentatives(visibleRuns, allRuns, groups, focusedIds = []) {
  const wideGroups = (groups ?? []).filter(isWideGroup);
  const emitted = new Set();
  const focus = focusedIds.filter(Boolean);
  const byId = new Map((allRuns ?? []).map((run) => [run.runId, run]));
  const output = [];
  for (const run of visibleRuns ?? []) {
    const group = wideGroups.find((g) => groupContainsRun(g, run.runId));
    if (!group) {
      output.push(run);
      continue;
    }
    if (emitted.has(group.primaryRunId)) continue;
    emitted.add(group.primaryRunId);
    // Focus order is intentional: the explicitly opened lane comes before the
    // just-started primary. Candidate order must not steal that precedence.
    const focusedId = focus.find((id) => groupContainsRun(group, id)) ?? "";
    const leadId = focusedId || group.primaryRunId;
    output.push(byId.get(leadId) ?? byId.get(group.primaryRunId) ?? run);
  }
  return output;
}

// The candidate runs of a group, in lane order, resolved to the normalized run
// rows so the expander can show each one's status and deep-link to it.
export function groupCandidates(group, allRuns) {
  const byId = new Map((allRuns ?? []).map((run) => [run.runId, run]));
  return (group?.candidateIds ?? []).map((id) => byId.get(id)).filter(Boolean);
}

// The group's one-line roll-up for the representative row: how many lanes and
// where they stand, in plain words.
export function groupSummaryLine(group) {
  if (!group) return "";
  const lanes = Math.max(group.observedCandidateCount, group.plannedCandidateCount, group.effectiveWorkers, group.candidateIds.length);
  const parts = [];
  if (group.doneCandidateCount > 0) parts.push(`${group.doneCandidateCount} of ${lanes} lanes done`);
  else parts.push(`${lanes} ${lanes === 1 ? "lane" : "lanes"}`);
  if (group.runningCandidateCount > 0) parts.push(`${group.runningCandidateCount} still working`);
  if (group.failedCandidateCount > 0) parts.push(`${group.failedCandidateCount} failed`);
  if (group.cannotProceedCandidateCount > 0) parts.push(`${group.cannotProceedCandidateCount} need a narrower step`);
  return parts.join(" · ");
}

// A collapsed candidate group is still building until every lane is
// terminal. Do not let an early successful candidate make the build-level row
// look review-ready while the ranked comparison is incomplete.
// The diff a run produced: per-file patches with their add/remove counts.
// Normalized for the review surface; the raw patch rides along for the
// line-by-line render.
export function normalizeDiff(payload) {
  const files = Array.isArray(payload?.files) ? payload.files : [];
  return {
    branch: String(payload?.branch ?? ""),
    fileCount: Number(payload?.review_file_count ?? payload?.file_count ?? files.length),
    files: files.map((file) => ({
      path: String(file?.path ?? ""),
      added: Number(file?.added ?? 0),
      removed: Number(file?.removed ?? 0),
      // Kernel-computed proof signals (zero/empty on kernels that predate
      // them): how much of the trail touched this file and whether any of
      // those steps erred. "needs_attention" is the look-here-first flag.
      trailSteps: Number(file?.trail_step_count ?? 0),
      errorSteps: Number(file?.error_step_count ?? 0),
      retries: Number(file?.retry_count ?? 0),
      checksTouching: Array.isArray(file?.checks_touching) ? file.checks_touching.map(String) : [],
      confidence: String(file?.agent_confidence ?? ""),
      // Shared tokenizer: each line carries its kind, its +/- marker, and
      // syntax-colored tokens, so the Runs diff reads the same premium way as
      // the Review Room.
      lines: diffLines(String(file?.patch ?? ""))
    }))
  };
}

// The repos an engineer has connected Forge to. MDx itself is always the
// implicit default (an empty repo target); these are the others.
export function normalizeRepos(projection) {
  const rows = Array.isArray(projection?.repos) ? projection.repos : [];
  return rows.map((row) => ({
    repoId: String(row?.repo_id ?? ""),
    label: String(row?.label ?? ""),
    root: String(row?.root ?? ""),
    kind: String(row?.kind ?? "local"),
    originUrl: String(row?.origin_url ?? ""),
    profile: row?.profile ?? {}
  }));
}

// The curated maintenance recipes - one click prefills a run. Cadence is a
// hint (which are meant to run nightly or weekly); launching is on demand.
export function normalizeRecipes(payload) {
  const rows = Array.isArray(payload?.recipes) ? payload.recipes : [];
  return rows.map((row) => ({
    id: String(row?.id ?? ""),
    title: String(row?.title ?? ""),
    category: String(row?.category ?? ""),
    cadence: String(row?.cadence ?? "on_demand"),
    summary: String(row?.summary ?? ""),
    intent: String(row?.intent ?? ""),
    commands: Array.isArray(row?.default_commands) ? row.default_commands.map(String) : []
  }));
}
