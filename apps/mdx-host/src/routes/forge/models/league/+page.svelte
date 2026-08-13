<script>
  // The machines lens. Forge is the subject of every line here: it chooses the
  // builder, it can stage an outside machine in a held trial, and it learns
  // when another machine out-builds the house. This is not a leaderboard with
  // Forge in the ranking. It is the place an operator sees which machine runs
  // their work, what else Forge can govern, and how the house keeps improving.
  import ForgeView from "../../../../lib/ForgeView.svelte";
  import { invalidateAll } from "$app/navigation";
  import { onMount } from "svelte";

  let { data } = $props();

  // Preflight (binary presence + version + exec-readiness) is slow - it probes
  // every installed runner, so it can take several seconds. We load it
  // client-side after the page renders rather than blocking the page load, then
  // the live-ready states fill in. Until it returns, the roster shows the
  // recommendation's static posture.
  let clientPreflight = $state(null);
  onMount(async () => {
    try {
      const r = await fetch("/api/kernel/forge/machine-league/preflight.json", { signal: AbortSignal.timeout(20000) });
      if (r.ok) clientPreflight = await r.json();
    } catch (error) {
      /* leave null; roster falls back to recommendation posture */
    }
  });
  const preflightChecks = $derived(
    Array.isArray(clientPreflight?.checks)
      ? clientPreflight.checks
      : Array.isArray(data.preflight?.checks)
        ? data.preflight.checks
        : []
  );

  const rec = $derived(data.recommendation ?? null);
  const ledger = $derived(data.ledger ?? null);
  const faceOffProjection = $derived(data.faceOffs ?? null);
  const faceOffRows = $derived(Array.isArray(faceOffProjection?.face_offs) ? faceOffProjection.face_offs : []);
  const latestFaceOff = $derived(faceOffRows[0] ?? null);

  // The roster, grouped by how ready Forge is to run each machine. The posture
  // comes straight from the kernel's execution_status; the grouping and the
  // human line are the only translation. Open machines are eligible; closed or
  // sign-in-gated machines are held until their terms are cleared.
  const POSTURE = {
    LIVE_LOCAL_BASELINE: {
      group: "Running now",
      line: "The house builder. Always on, and every step it takes leaves a receipt."
    },
    PROFILE_ADMITTED_ADAPTER_EXECUTION_DENIED: {
      group: "Ready for a held trial",
      line: "Open source. Forge can run it in an isolated copy as a trial. Nothing it produces counts until Forge's checks pass."
    },
    PROFILE_DECLARED_PREFLIGHT_REQUIRED: {
      group: "Open, pending a local check",
      line: "Open source. Available once Forge confirms it runs cleanly on this machine."
    },
    PROFILE_DECLARED_TOS_AUTH_CLEARANCE_REQUIRED: {
      group: "Held for clearance",
      line: "Declared so the roster is honest, but Forge will not run it until its terms and sign-in are cleared."
    }
  };
  const GROUP_ORDER = [
    "Running now",
    "Ready to run live",
    "Ready for a held trial",
    "Open, pending a local check",
    "Held for clearance"
  ];

  // Which machines an operator has explicitly cleared for execution, and in
  // which mode. Open-source Grok uses this as an execution attestation, not a
  // terms clearance. Clearing never grants execution by itself.
  const clearanceByRunner = $derived.by(() => {
    const rows = Array.isArray(data.clearances?.clearances) ? data.clearances.clearances : [];
    const map = {};
    for (const c of rows) {
      map[c.runner_id] = { mode: String(c.clearance_mode || ""), scorecardEligible: c.scorecard_eligible === true };
    }
    return map;
  });

  let clearOpen = $state("");
  let clearBusy = $state(false);
  let clearResult = $state(null);
  // Clearance is a conscious operator act. It still does not run anything:
  // execution stays key-, approval-, budget-, preflight-, and env-gated.
  async function clearRunner(runnerId, mode) {
    if (clearBusy) return;
    clearBusy = true;
    clearResult = null;
    try {
      const response = await fetch("/api/kernel/forge/machine-league/closed-runner-clearances.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ runner_id: runnerId, clearance_mode: mode, operator_attestation: true })
      });
      const packet = await response.json();
      if (packet.status === "CLOSED_RUNNER_CLEARANCE_RECORDED") {
        clearResult = {
          ok: true,
          line: mode === "fleet_eval"
            ? "Cleared to compete. Forge can run it in held trials and let accepted results count - still held until the checks pass."
            : "Cleared as a companion. You can use it here, but it stays out of the scorecard."
        };
        clearOpen = "";
        await invalidateAll();
      } else {
        clearResult = { ok: false, line: packet.reason ? String(packet.reason).replace(/_/g, " ") : "That did not clear." };
      }
    } catch (error) {
      clearResult = { ok: false, line: "Nothing cleared - that did not land." };
    }
    clearBusy = false;
  }

  // Field a fleet: plan an N-wide run cast across machines and models. This is a
  // quarantined planning layer - it proves which machines can be cast together
  // and why others are held; it runs nothing.
  let fleetWidth = $state(8);
  let fleetBusy = $state(false);
  let fleetPlan = $state(null);
  function excludedReason(r) {
    const map = {
      blocked_pending_tos_auth_clearance: "held until you clear its terms and sign-in",
      binary_missing: "its binary is not installed here",
      profile_declared_preflight_required: "waiting on a local check"
    };
    return map[String(r || "")] || String(r || "").replace(/_/g, " ");
  }
  async function planFleet(width) {
    if (fleetBusy) return;
    fleetWidth = width;
    fleetBusy = true;
    try {
      const response = await fetch("/api/kernel/forge/machine-league/fleet-casts.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ task_class: "bug_fix", language_pack_id: "rust-cargo", worker_count: width })
      });
      const p = await response.json();
      if (p.status === "FLEET_CAST_PLAN_READY") {
        fleetPlan = {
          width: Number(p.worker_count_planned ?? width),
          uniqueCasts: Number(p.unique_cast_count ?? 0),
          widthsProven: Array.isArray(p.fleet_widths_proven) ? p.fleet_widths_proven : [],
          workers: (p.worker_casts ?? []).map((w) => ({
            runner: String(w.runner_display_name || runnerName(w.runner_id)),
            model: humanModel(w.model_profile_id),
            native: w.runner_adapter_kind === "mdx_native"
          })),
          excluded: (p.excluded_runner_profiles ?? []).map((e) => ({
            name: String(e.display_name || e.runner_id || "a machine"),
            reason: excludedReason(e.excluded_reason)
          }))
        };
      } else {
        fleetPlan = { error: p.reason ? String(p.reason).replace(/_/g, " ") : "Forge could not plan the fleet." };
      }
    } catch (error) {
      fleetPlan = { error: "Forge could not plan the fleet." };
    }
    fleetBusy = false;
  }

  // Running a fleet for real: a mix of machines execute one task in parallel,
  // each lane quarantined. Reads are projection-driven so the view survives a
  // reload and reflects the kernel's truth, not optimistic local state.
  function lhStatus(s) {
    const map = {
      LANE_EXECUTED_QUARANTINED: "ran, held",
      LANE_STAGED_HELD_TRIAL_PACKET: "staged, held",
      PLANNED_NATIVE_BASELINE_HELD: "ready to run",
      LANE_BLOCKED: "blocked"
    };
    return map[String(s || "")] || String(s || "").replace(/_/g, " ").toLowerCase();
  }
  function normalizeFleetRun(r) {
    if (!r) return null;
    return {
      id: String(r.fleet_run_id || ""),
      status: String(r.status || ""),
      planned: Number(r.worker_count_planned ?? 0),
      executed: Number(r.executed_lane_count ?? 0),
      held: Number(r.held_lane_count ?? 0),
      readyForReview: Number(r.ready_for_review_count ?? 0),
      accepted: Number(r.accepted_for_scoreboard_count ?? 0),
      integration: String(r.integration_status || ""),
      lanes: (r.lanes ?? []).map((l) => ({
        index: Number(l.lane_index ?? 0),
        runner: String(l.runner_display_name || runnerName(l.runner_id)),
        model: humanModel(l.model_profile_id),
        status: lhStatus(l.lane_status),
        executed: l.execution_performed === true,
        checkPassed: l.check_passed === true,
        runId: String(l.run_id || ""),
        resultReceiptId: String(l.result_receipt_id || ""),
        accepted: l.accepted_for_scoreboard === true,
        native: l.runner_id === "mdx_native_harness_runner"
      }))
    };
  }
  const latestProjectionRun = $derived.by(() => {
    const runs = Array.isArray(data.fleetRuns?.runs) ? data.fleetRuns.runs : [];
    return runs.length ? normalizeFleetRun(runs[runs.length - 1]) : null;
  });
  let fleetRunOverride = $state(null);
  const fleetRun = $derived(fleetRunOverride ?? latestProjectionRun);
  const arbitrationByRun = $derived.by(() => {
    const rows = Array.isArray(data.arbitrations?.arbitrations) ? data.arbitrations.arbitrations : [];
    const map = {};
    for (const a of rows) map[a.fleet_run_id] = a;
    return map;
  });
  let arbitrationOverride = $state(null);
  const arbitration = $derived(arbitrationOverride ?? (fleetRun ? arbitrationByRun[fleetRun.id] : null));

  let fleetRunBusy = $state(false);
  let fleetRunErr = $state("");
  async function runFleet(width) {
    if (fleetRunBusy) return;
    fleetRunBusy = true;
    fleetRunErr = "";
    arbitrationOverride = null;
    try {
      const response = await fetch("/api/kernel/forge/machine-league/fleet-runs.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          worker_count: width,
          task_class: "bug_fix",
          language_pack_id: "rust-cargo",
          eval_fixture: "rust_tax_rounding",
          execute_live: true,
          execute_native_live: true
        })
      });
      const p = await response.json();
      if (p.status === "REFUSED") fleetRunErr = p.reason ? String(p.reason).replace(/_/g, " ") : "Forge could not run the fleet.";
      else fleetRunOverride = normalizeFleetRun(p);
    } catch (error) {
      fleetRunErr = "Forge could not run the fleet.";
    }
    fleetRunBusy = false;
  }

  async function refreshFleetRun(id) {
    try {
      const r = await fetch("/api/kernel/forge/machine-league/fleet-runs/projection.json");
      const d = await r.json();
      const run = (d.runs ?? []).find((x) => x.fleet_run_id === id);
      if (run) fleetRunOverride = normalizeFleetRun(run);
    } catch (error) {
      /* keep current */
    }
  }

  let laneAcceptBusy = $state("");
  async function acceptLane(lane, fleetRunId) {
    if (laneAcceptBusy || !lane.runId || !lane.resultReceiptId) return;
    laneAcceptBusy = lane.runId;
    try {
      await fetch("/api/kernel/forge/fleet-eval-results/principal-reviews.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ run_id: lane.runId, result_receipt_id: lane.resultReceiptId, principal_engineer_verdict: "accept" })
      });
      await refreshFleetRun(fleetRunId);
    } catch (error) {
      /* keep current */
    }
    laneAcceptBusy = "";
  }

  let arbitrateBusy = $state(false);
  async function arbitrateFleet(fleetRunId) {
    if (arbitrateBusy || !fleetRunId) return;
    arbitrateBusy = true;
    try {
      const response = await fetch("/api/kernel/forge/machine-league/fleet-runs/arbitrations.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ fleet_run_id: fleetRunId })
      });
      arbitrationOverride = await response.json();
    } catch (error) {
      arbitrationOverride = { status: "error" };
    }
    arbitrateBusy = false;
  }

  // Which machine + model pairings Forge can cast, grouped by machine. Answers
  // "any harness with any model?" honestly - the kernel's compatibility matrix
  // says which combinations are castable and why others are not.
  const compatByRunner = $derived.by(() => {
    const rows = Array.isArray(scoreboard?.runner_model_compatibility_matrix)
      ? scoreboard.runner_model_compatibility_matrix
      : [];
    const by = new Map();
    for (const r of rows) {
      const name = String(r.runner_display_name || r.runner_id || "");
      if (!by.has(name)) by.set(name, { name, castable: new Set(), total: 0 });
      const entry = by.get(name);
      entry.total += 1;
      if (r.castable === true) entry.castable.add(humanModel(r.model_profile_id));
    }
    return [...by.values()].map((e) => ({ name: e.name, models: [...e.castable], total: e.total }));
  });

  const runners = $derived(Array.isArray(rec?.runner_profiles) ? rec.runner_profiles : []);
  const PREMIER_RUNNER_IDS = [
    "mdx_native_harness_runner",
    "codex_cli_external_worker",
    "grok_build_cli_external_worker"
  ];
  const premierMachines = $derived.by(() => {
    const pf = {};
    for (const check of preflightChecks) pf[check.runner_id] = check.status;
    return runners
      .filter((runner) => PREMIER_RUNNER_IDS.includes(String(runner.runner_id ?? "")))
      .map((runner) => {
        const id = String(runner.runner_id);
        const native = runner.runner_kind === "mdx_native";
        const ready = native || pf[id] === "live_execution_ready";
        return {
          id,
          name: runner.display_name,
          model: native ? "Any frontier model" : id === "grok_build_cli_external_worker" ? "Grok-4.5" : "GPT-5.6",
          status: native ? "Recommended default" : ready ? "Ready to run" : "Premier, setup needed",
          line: native
            ? "The default that improves MDx itself. Swap models by role."
            : id === "grok_build_cli_external_worker"
              ? "Open-source ACP. Connect xAI and authorize governed execution."
              : "Open-source CLI. Outputs stay held until MDx accepts them."
        };
      });
  });
  const grouped = $derived.by(() => {
    // Preflight is the live operational truth (binary present + exec turned on).
    // When a runner is live-ready, show it as ready to run, even if the static
    // recommendation status still reads as pending - otherwise the group label
    // and the "Run a live trial" button would disagree.
    const pf = {};
    for (const c of preflightChecks) pf[c.runner_id] = c.status;
    const by = {};
    for (const r of runners) {
      if (PREMIER_RUNNER_IDS.includes(String(r.runner_id ?? ""))) continue;
      const liveReady = pf[String(r.runner_id ?? "")] === "live_execution_ready";
      const posture = liveReady && r.runner_kind !== "mdx_native"
        ? { group: "Ready to run live", line: "Installed and turned on here. Forge can run it live in an isolated copy - nothing it produces counts until your checks pass." }
        : POSTURE[r.execution_status] ?? { group: "Held for clearance", line: "Declared. Forge will not run it yet." };
      (by[posture.group] ??= []).push({
        id: String(r.runner_id ?? ""),
        name: String(r.display_name ?? r.runner_id ?? "this machine"),
        external: r.runner_kind !== "mdx_native",
        line: posture.line
      });
    }
    return GROUP_ORDER.filter((g) => by[g]?.length).map((g) => ({ group: g, runners: by[g] }));
  });

  // Routing evidence: kept behind a disclosure on purpose, not a front-door
  // leaderboard. Native is the baseline; an outside machine earns routing only
  // on accepted evidence.
  const scoreboard = $derived(data.scoreboard ?? null);

  // Scorecard V2: per kind of work, grounded only in accepted runs. This is the
  // honest routing view - it carries sample size and a confidence read, so a
  // single accepted run never reads as a settled verdict. Native is the
  // baseline; an outside machine earns routing only when its accepted evidence
  // on that task class is strong enough.
  const taskClassRows = $derived(
    Array.isArray(scoreboard?.task_class_scorecards) ? scoreboard.task_class_scorecards : []
  );
  const taskClasses = $derived([...new Set(taskClassRows.map((r) => String(r.task_class ?? "")))].filter(Boolean).sort());
  let selectedTaskClass = $state("");
  const activeTaskClass = $derived(taskClasses.includes(selectedTaskClass) ? selectedTaskClass : taskClasses[0] ?? "");
  const scorecardForClass = $derived.by(() => {
    return taskClassRows
      .filter((r) => String(r.task_class ?? "") === activeTaskClass)
      .map((r) => ({
        name: String(r.display_name ?? r.runner_id ?? ""),
        native: String(r.runner_id ?? "").includes("native"),
        accepted: Number(r.accepted_count ?? 0),
        sample: Number(r.scoreboard_sample_size ?? 0),
        passRate: Number(r.pass_rate_pct ?? 0),
        confidence: String(r.confidence ?? "insufficient_sample")
      }))
      .sort((a, b) => Number(b.native) - Number(a.native) || b.accepted - a.accepted);
  });
  function humanTaskClass(tc) {
    const s = String(tc ?? "").replace(/_/g, " ");
    return s ? s.charAt(0).toUpperCase() + s.slice(1) : "this work";
  }
  function humanConfidence(c) {
    switch (String(c ?? "")) {
      case "insufficient_sample": return "Not enough runs yet";
      case "low": return "Low confidence";
      case "medium": return "Building confidence";
      case "high": return "High confidence";
      default: return String(c ?? "").replace(/_/g, " ");
    }
  }

  // Tier 3b: turning a recorded learning into a real native-harness improvement.
  // The loop schedules a Builder Loop (so Forge attempts the change under its own
  // governed run/review/ship path) and plans a re-measurement on the same task.
  // It does not auto-rewrite the harness; before/after appears only on a real
  // re-measured sample.
  const improvementLoopByNote = $derived.by(() => {
    const rows = Array.isArray(data.improvementLoops?.loops) ? data.improvementLoops.loops : [];
    const map = {};
    for (const lp of rows) if (lp.distillation_note_id) map[lp.distillation_note_id] = lp;
    return map;
  });
  let improveBusy = $state("");
  let improveOverride = $state({});
  function loopForNote(noteId) {
    return improveOverride[noteId] ?? improvementLoopByNote[noteId] ?? null;
  }
  function loopLine(lp) {
    const before = Number(lp.before_native_accepted_count ?? 0);
    const after = Number(lp.after_native_accepted_count ?? 0);
    if (after > before && after > 0) {
      return `Forge re-measured its own builder after the change: accepted runs on this task went from ${before} to ${after}.`;
    }
    return "Forge scheduled an improvement run to teach its builder this pattern, under the same governed run, review, and ship path. It will re-run the same task to measure the change. Not re-measured yet, so no before-and-after is claimed.";
  }
  async function improveFromNote(noteId) {
    if (improveBusy || !noteId) return;
    improveBusy = noteId;
    try {
      const response = await fetch("/api/kernel/forge/machine-league/native-improvement-loops.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ distillation_note_id: noteId, builder_loop_launch_execution: false })
      });
      const lp = await response.json();
      if (lp.status === "NATIVE_IMPROVEMENT_LOOP_TICK_RECORDED") improveOverride = { ...improveOverride, [noteId]: lp };
    } catch (error) {
      /* keep current */
    }
    improveBusy = "";
  }

  const entries = $derived(Array.isArray(ledger?.entries) ? ledger.entries : []);
  // A pending learning: an outside machine has an accepted win, but the
  // comparison is not complete until the house runner is run on the same task
  // and a person records what to distill. Shown as in-progress, never as a
  // settled "this machine is better" claim.
  const pendingCandidates = $derived(
    Array.isArray(ledger?.pending_distillation_candidates) ? ledger.pending_distillation_candidates : []
  );
  const runnerName = (id) =>
    runners.find((r) => r.runner_id === id)?.display_name ||
    (id === "mdx_native_harness_runner" ? "the native builder" : id || "a machine");

  // How a runner won, in plain words. model_capability is routing evidence (use
  // this machine for this class); the strategy reasons are reusable patterns
  // the house runner can learn.
  const REASON_PHRASE = {
    model_capability: "a stronger model for this kind of task",
    context_strategy: "better context gathering",
    planning_strategy: "better planning of the change",
    tool_strategy: "better use of its tools",
    review_strategy: "catching its own mistakes better",
    ux_strategy: "clearer updates while it worked",
    parallel_strategy: "better parallel work",
    artifact_strategy: "cleaner diffs and handoffs"
  };
  // The honest delta line. Only states a win-rate change when accepted before
  // and after samples actually exist; otherwise it says, plainly, that nothing
  // has been re-measured yet - one accepted win is evidence, not a verdict.
  function ledgerDeltaLine(e) {
    const before = e.before_win_rate_pct;
    const after = e.after_win_rate_pct;
    if (before != null && after != null) {
      return `After the improvement landed, the native builder's accepted rate on this task class moved from ${before}% to ${after}%.`;
    }
    const n = Number(e.external_accepted_count ?? 1);
    return `Recorded from ${n} accepted ${n === 1 ? "win" : "wins"}. A native improvement is queued (${String(e.native_improvement_work_item_id ?? "tracked").replace(/_/g, " ")}); Forge has not re-measured the house runner yet, so it claims no win-rate change - evidence, not a verdict.`;
  }
  const selectedName = $derived(rec?.selected_runner_display_name || "its own builder");
  const fallbackName = $derived(rec?.fallback_runner_display_name || "");
  const stillLearning = $derived(rec?.confidence === "gathering_evidence");

  // Is live execution turned on here? Preflight reports it per runner (binary
  // present and the execution env set). When off, the lens stages a held packet
  // - admitted and recorded, no external run. When on, it runs the CLI for real
  // in an isolated copy and holds the output either way.
  const liveByRunner = $derived.by(() => {
    return Object.fromEntries(preflightChecks.map((x) => [x.runner_id, x.status === "live_execution_ready"]));
  });
  const trialRoutes = $derived(rec?.trial_routes ?? {});

  // Translate a real trial outcome into a useful reason, not an exit code. A
  // live run that does not pass a check stays held - Forge never reports a win
  // the machine did not earn.
  function trialOutcomeLine(p) {
    if (p.status === "CODEX_CLI_TRIAL_PACKET_RECORDED_QUARANTINED") {
      return "Held trial recorded. No external run - the packet is admitted and held until Forge's checks accept it.";
    }
    if (p.status === "GEMINI_CLI_TRIAL_PACKET_RECORDED_QUARANTINED") {
      return "Held Gemini trial recorded. No external run - the packet is admitted and held until Forge's checks accept it.";
    }
    const machineName = p.runner_display_name || runnerName(p.runner_id);
    if (p.gemini_auth_blocked === true) {
      return "Gemini CLI ran for real, but local auth is blocked. Held - nothing it produced counts.";
    }
    if (p.runner_auth_blocked === true) {
      return `${machineName} ran for real, but local auth is blocked. Held - nothing it produced counts.`;
    }
    if (String(p.status ?? "").endsWith("_TRIAL_PACKET_RECORDED_QUARANTINED")) {
      return `Held ${machineName} trial recorded. No external run - the packet is admitted and held until Forge's checks accept it.`;
    }
    const machineTimedOut = p.codex_timed_out === true || p.gemini_timed_out === true || p.runner_timed_out === true;
    const checkTimedOut = p.check_timed_out === true;
    const machineExit = Number(p.codex_exit_code ?? p.gemini_exit_code ?? p.runner_exit_code ?? 0);
    const machineFailed = machineExit !== 0;
    const checkFailed = Number(p.check_exit_code ?? 0) !== 0;
    if (machineTimedOut) return `Ran for real, then ${machineName} ran out of time. Held - nothing it produced counts.`;
    if (machineFailed) return `Ran for real, but ${machineName} hit an error. Held - nothing it produced counts.`;
    if (checkTimedOut) return "Ran for real; the check ran out of time. Held until a check passes.";
    if (checkFailed) return "Ran for real; the check did not pass. Held - Forge did not count it.";
    return "Ran for real and the check passed. Held pending Forge's gates and a person's review.";
  }

  // Whether a candidate's native comparison has already run on the same fixture
  // (the house runner attempted the identical held-out task). When it has, the
  // remaining step is a judgment: distill only if the outside machine showed a
  // reusable edge - a tie distills nothing.
  const nativeComparisonRan = (c) =>
    Number(c?.native_same_fixture_accepted_count ?? 0) > 0 ||
    c?.status === "native_comparison_available_pending_distillation_decision";

  let nativeCompareBusy = $state(false);
  let nativeCompareResult = $state(null);
  // Run the native builder on the same held-out fixture, so the comparison is a
  // real head-to-head and not an assertion. No external machine here - this is
  // the house runner, governed the same way.
  async function runNativeComparison() {
    if (nativeCompareBusy) return;
    nativeCompareBusy = true;
    nativeCompareResult = null;
    trialResult = null;
    try {
      const response = await fetch("/api/kernel/forge/machine-league/native-trials.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          task_class: "bug_fix",
          language_pack_id: "rust-cargo",
          eval_fixture: "rust_tax_rounding",
          execute_live: true,
          budget_seconds: 120
        })
      });
      const packet = await response.json();
      if (packet.status === "MDX_NATIVE_EXECUTED_QUARANTINED_PENDING_MDX_GATES") {
        nativeCompareResult = {
          ok: true,
          line: packet.check_passed
            ? "The native builder also solved it. Forge now compares the diffs and records a learning only if an outside machine showed a reusable edge."
            : "The native builder ran but did not pass the checks. That itself is the comparison Forge needed."
        };
      } else {
        nativeCompareResult = { ok: false, line: packet.reason ? String(packet.reason) : "That did not run." };
      }
    } catch (error) {
      nativeCompareResult = { ok: false, line: "Nothing ran - that did not land." };
    }
    nativeCompareBusy = false;
  }

  let trialBusy = $state(false);
  let trialResult = $state(null);
  let faceOffBusy = $state(false);
  let faceOffResult = $state(null);
  let smokeBusy = $state("");
  let smokeResult = $state(null);
  async function smokeRuntime(runnerId) {
    if (smokeBusy) return;
    smokeBusy = runnerId;
    smokeResult = null;
    trialResult = null;
    nativeCompareResult = null;
    try {
      const response = await fetch("/api/kernel/forge/machine-league/runtime-smokes.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ runner_id: runnerId })
      });
      const packet = await response.json();
      const name = packet.runner_display_name || runnerName(runnerId);
      if (packet.machine_runtime_smoke_passed === true) {
        smokeResult = { ok: true, line: `${name} runtime smoke passed. Forge can accept drifted results for this exact fingerprint after normal gates pass.` };
      } else if (packet.status === "REFUSED") {
        smokeResult = { ok: false, line: packet.reason ? String(packet.reason) : "Runtime smoke refused." };
      } else {
        smokeResult = { ok: false, line: `${name} runtime smoke is held. The binary is missing or not compatible enough to count drifted results.` };
      }
    } catch (error) {
      smokeResult = { ok: false, line: "Runtime smoke did not land." };
    }
    smokeBusy = "";
  }

  async function stageTrial(runnerId) {
    if (trialBusy) return;
    trialBusy = true;
    trialResult = null;
    nativeCompareResult = null;
    const kernelRoute = trialRoutes[runnerId];
    const route = kernelRoute ? `/api/kernel${kernelRoute}` : "";
    if (!route) {
      trialResult = { ok: false, line: "Forge does not have a trial route for that machine yet." };
      trialBusy = false;
      return;
    }
    const live = liveByRunner[runnerId] === true;
    try {
      const response = await fetch(route, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          task_class: "bug_fix",
          language_pack_id: "rust-cargo",
          eval_fixture: "rust_tax_rounding",
          execute_live: live,
          budget_seconds: 45
        })
      });
      const packet = await response.json();
      const ok = packet.status === "CODEX_CLI_TRIAL_PACKET_RECORDED_QUARANTINED"
        || packet.status === "CODEX_CLI_EXECUTED_QUARANTINED_PENDING_MDX_GATES"
        || packet.status === "GEMINI_CLI_TRIAL_PACKET_RECORDED_QUARANTINED"
        || packet.status === "GEMINI_CLI_EXECUTED_QUARANTINED_PENDING_MDX_GATES"
        || String(packet.status ?? "").endsWith("_TRIAL_PACKET_RECORDED_QUARANTINED")
        || String(packet.status ?? "").endsWith("_EXECUTED_QUARANTINED_PENDING_MDX_GATES");
      if (ok) {
        trialResult = { ok: true, runId: String(packet.run_id ?? ""), line: trialOutcomeLine(packet) };
      } else {
        trialResult = { ok: false, line: packet.blocked_reason ? String(packet.blocked_reason).replace(/_/g, " ") : "That did not run." };
      }
    } catch (error) {
      trialResult = { ok: false, line: "Nothing ran - that did not land." };
    }
    trialBusy = false;
  }

  const preferredExternalRunner = $derived(rec?.fallback_runner_id || "codex_cli_external_worker");
  function statePhrase(value) {
    return String(value || "not recorded").replace(/_/g, " ");
  }

  // Human names for the model behind a runner. The kernel carries systemy
  // profile ids (codex_anthropic_responses_profile); an operator should read the
  // model, not the id.
  // The backend now carries a display name per model profile. Prefer it, so the
  // UI always shows exactly what is configured to run and never a stale hardcoded
  // version. The local map is only a fallback for an older projection.
  const modelDisplayByProfile = $derived.by(() => {
    const rows = Array.isArray(scoreboard?.runner_model_compatibility_matrix)
      ? scoreboard.runner_model_compatibility_matrix
      : [];
    const map = {};
    for (const r of rows) {
      if (r.model_profile_id && r.model_display_name) map[r.model_profile_id] = r.model_display_name;
    }
    return map;
  });
  const MODEL_NAME = {
    mdx_native_local_model_profile: "Forge's own model",
    codex_openai_responses_profile: "GPT-5.6 Sol",
    codex_anthropic_responses_profile: "Claude Opus",
    codex_gemini_responses_profile: "Gemini 2.5 Pro",
    codex_xai_responses_profile: "Grok 4.5",
    codex_bedrock_responses_profile: "a Bedrock model",
    claude_code_model_profile: "Claude",
    gemini_cli_model_profile: "Gemini",
    xai_grok_build_model_profile: "Grok Build 4.5",
    // CLI runners that use whatever model the operator has configured in them.
    cline_model_profile: "its configured model",
    goose_model_profile: "its configured model",
    opencode_model_profile: "its configured model"
  };
  function humanModel(id) {
    const key = String(id || "");
    if (!key || key === "model profile pending") return "its model";
    if (modelDisplayByProfile[key]) return modelDisplayByProfile[key];
    if (MODEL_NAME[key]) return MODEL_NAME[key];
    return key.replace(/_profile$/, "").replace(/_responses?$/, "").replace(/_/g, " ");
  }
  // How the machine ran, in plain words (not the adapter tier id).
  const ADAPTER_PHRASE = {
    headless_cli_adapter: "headless, in a safe copy",
    responses_api_adapter: "through the model's API",
    stream_json_adapter: "with a streamed transcript",
    local_companion_adapter: "as a local companion"
  };
  function adapterPhrase(t) {
    const key = String(t || "");
    return ADAPTER_PHRASE[key] || key.replace(/_adapter$/, "").replace(/_/g, " ") || "in a safe copy";
  }
  // The native-comparison and learning states, in plain human words.
  const NATIVE_PHRASE = {
    not_enough_accepted_evidence: "your own builder has not been measured against it yet",
    native_not_run: "your own builder has not run this one yet",
    native_also_passed: "your own builder solved it too",
    native_comparison_tie: "your own builder solved it too",
    external_beat_native: "it out-built your own builder",
    native_beat_external: "your own builder did it better"
  };
  function nativePhrase(s) {
    return NATIVE_PHRASE[String(s || "")] || "the comparison with your builder is still open";
  }
  const LEARNING_PHRASE = {
    nothing_learned_yet: "there is nothing to learn this round",
    nothing_to_distill_tie: "it was a tie, so there is nothing to learn",
    pending_native_comparison_and_distillation_note: "a learning is still in progress",
    native_comparison_available_pending_distillation_decision: "Forge is deciding whether there is anything to learn",
    distillation_note_recorded: "Forge learned something from it"
  };
  function learningPhrase(s) {
    return LEARNING_PHRASE[String(s || "")] || "there is nothing to learn yet";
  }
  // The face-off as one confident, human sentence - Forge is the subject, the
  // model reads as a model, and the held boundary is plain.
  function faceOffNarrative(runner, model, native, learning) {
    return `Forge sent this to ${runner}, running on ${humanModel(model)}. It worked in a safe copy and the output stays held - ${nativePhrase(native)}, so ${learningPhrase(learning)}.`;
  }

  async function runFaceOff(runnerId = preferredExternalRunner) {
    if (faceOffBusy) return;
    faceOffBusy = true;
    faceOffResult = null;
    trialResult = null;
    nativeCompareResult = null;
    smokeResult = null;
    try {
      const response = await fetch("/api/kernel/forge/machine-league/face-offs.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          external_runner_id: runnerId,
          task_class: "bug_fix",
          language_pack_id: "rust-cargo",
          eval_fixture: "rust_tax_rounding",
          stage_external_trial: true,
          execute_external_live: liveByRunner[runnerId] === true
        })
      });
      const packet = await response.json();
      if (packet.status === "REFUSED") {
        faceOffResult = { ok: false, line: packet.reason ? String(packet.reason) : "Forge could not create the face-off." };
      } else {
        faceOffResult = {
          ok: packet.external_output_consumable === false,
          runner: String(packet.runner_display_name ?? runnerName(packet.runner_id)),
          model: String(packet.model_profile_id ?? "model profile pending"),
          adapter: String(packet.adapter_protocol_tier ?? "adapter pending"),
          native: String(packet.native_result_state ?? "not_enough_accepted_evidence"),
          learning: String(packet.learning_status ?? "nothing_learned_yet"),
          line: String(packet.operator_summary ?? "Forge staged the face-off and kept the output held.")
        };
      }
    } catch (error) {
      faceOffResult = { ok: false, line: "Forge could not create the face-off." };
    }
    faceOffBusy = false;
  }
</script>

<svelte:head><title>Machines - MDx</title></svelte:head>

<ForgeView
  title="Machines"
  backTo={{ href: "/forge/models", label: "Models" }}
  subtitle="Three premier harnesses, independently paired with models. MDx Native is the default; Codex CLI and Grok Build are first-class choices."
>
  {#if !rec}
    <div class="lens-empty">
      <h2>Not reachable</h2>
      <p>Forge could not read the machine roster just now. It opens here as soon as the kernel answers.</p>
    </div>
  {:else}
    <section class="premier" aria-label="Premier harnesses">
      <div class="premier-head">
        <div>
          <span class="stance-eyebrow">Premier harnesses</span>
          <h2>Choose the harness and model separately</h2>
        </div>
        <a href="/forge/models">Configure models &rarr;</a>
      </div>
      <div class="premier-grid">
        {#each premierMachines as machine (machine.id)}
          <article class="premier-card">
            <div class="premier-title">
              <h3>{machine.name}</h3>
              <strong>{machine.status}</strong>
            </div>
            <p>{machine.line}</p>
            <small>{machine.model}</small>
            {#if machine.id === "grok_build_cli_external_worker" && !clearanceByRunner[machine.id]}
              <button type="button" class="m-trial" disabled={clearBusy} onclick={() => clearRunner(machine.id, "fleet_eval")}>Authorize live trials</button>
            {/if}
          </article>
        {/each}
      </div>
    </section>

    <!-- How Forge runs your work: the conductor stance. Forge is the subject. -->
    <section class="stance">
      <span class="stance-eyebrow">How Forge runs your work</span>
      <p class="stance-lead">Forge defaults to <strong>MDx Native</strong> so every run teaches the house harness, while senior engineers can switch harness or model without changing the governance boundary.</p>
      <p class="stance-why">Current pick: <strong>{selectedName}</strong>{rec.recommendation_rationale ? ` - ${rec.recommendation_rationale}` : ""}</p>
      {#if fallbackName}
        <p class="stance-fallback">Evidence challenger: <strong>{fallbackName}</strong>. Forge keeps outputs held and feeds reusable wins back into native.</p>
      {/if}
      <details class="stance-more">
        <summary>How Forge decides</summary>
        <ul class="stance-detail">
          <li>Confidence: {stillLearning ? "still gathering evidence on this matchup" : (rec.confidence ?? "recorded")}</li>
          <li>Accepted evidence so far: {Number(rec.scorecard_evidence_count ?? 0)}</li>
          {#if rec.quarantine_boundary}<li>Outside work: {rec.quarantine_boundary}.</li>{/if}
          {#if rec.acceptance_gate}<li>Acceptance: {String(rec.acceptance_gate).replace(/_/g, " ")}.</li>{/if}
        </ul>
      </details>
      <div class="faceoff-action">
        <button type="button" class="faceoff-button" disabled={faceOffBusy} onclick={() => runFaceOff()}>
          {faceOffBusy ? "Setting it up..." : "Send it head to head"}
        </button>
        <span>Forge runs the machine it recommends in a safe copy, against your own builder, and holds whatever it produces until your checks pass.</span>
      </div>
    </section>

    <section class="faceoff">
      <h2 class="faceoff-head">Current face-off</h2>
      {#if faceOffResult}
        <div class="faceoff-card" data-ok={faceOffResult.ok}>
          {#if faceOffResult.runner}
            <p class="faceoff-line">{faceOffNarrative(faceOffResult.runner, faceOffResult.model, faceOffResult.native, faceOffResult.learning)}</p>
            <details class="faceoff-more">
              <summary>Under the hood</summary>
              <dl class="faceoff-facts">
                <div><dt>Machine</dt><dd>{faceOffResult.runner}</dd></div>
                <div><dt>Model</dt><dd>{humanModel(faceOffResult.model)}</dd></div>
                <div><dt>How it ran</dt><dd>{adapterPhrase(faceOffResult.adapter)}</dd></div>
              </dl>
            </details>
          {:else}
            <p class="faceoff-line">{faceOffResult.line}</p>
          {/if}
        </div>
      {:else if latestFaceOff}
        <div class="faceoff-card" data-ok="true">
          <p class="faceoff-line">{faceOffNarrative(latestFaceOff.runner_display_name, latestFaceOff.model_profile_id, latestFaceOff.native_result_state, latestFaceOff.learning_status)}</p>
          <details class="faceoff-more">
            <summary>Under the hood</summary>
            <dl class="faceoff-facts">
              <div><dt>Machine</dt><dd>{latestFaceOff.runner_display_name}</dd></div>
              <div><dt>Model</dt><dd>{humanModel(latestFaceOff.model_profile_id)}</dd></div>
              <div><dt>How it ran</dt><dd>{adapterPhrase(latestFaceOff.adapter_protocol_tier)}</dd></div>
            </dl>
          </details>
        </div>
      {:else}
        <div class="faceoff-card">
          <p class="faceoff-line">No machine has gone head to head yet. Forge can send your recommended machine into a face-off and hold whatever it produces.</p>
        </div>
      {/if}
    </section>

    <!-- Additional machines Forge can govern, grouped by readiness. -->
    <section class="roster">
      <h2 class="roster-head">Additional specialist harnesses</h2>
      {#each grouped as bucket (bucket.group)}
        <div class="bucket">
          <span class="bucket-label">{bucket.group}</span>
          <ul class="machines">
            {#each bucket.runners as m (m.id)}
              {@const heldForClearance = bucket.group === "Held for clearance"}
              {@const cleared = clearanceByRunner[m.id]}
              <li class="machine" data-external={m.external}>
                <div class="m-row">
                  <span class="m-dot" data-group={bucket.group} aria-hidden="true"></span>
                  <span class="m-body">
                    <span class="m-name">{m.name}</span>
                    <span class="m-line">{cleared ? "Cleared by you. Forge can govern it now - still held and env-gated until you turn execution on." : m.line}</span>
                  </span>
                  <span class="m-actions">
                    {#if heldForClearance}
                      {#if cleared}
                        <span class="m-cleared" data-eligible={cleared.scorecardEligible}>{cleared.scorecardEligible ? "Cleared to compete" : "Cleared as companion"}</span>
                      {:else}
                        <button type="button" class="m-trial" onclick={() => (clearOpen = clearOpen === m.id ? "" : m.id)}>
                          {clearOpen === m.id ? "Cancel" : "Clear to use"}
                        </button>
                      {/if}
                    {:else}
                      {#if m.external && trialRoutes[m.id]}
                        {@const liveRunner = liveByRunner[m.id] === true}
                        <button type="button" class="m-trial" disabled={trialBusy} onclick={() => stageTrial(m.id)} title={liveRunner ? `Runs ${m.name} for real in an isolated copy; output is held until Forge's checks accept it` : "Records a held trial packet; no external run happens"}>
                          {#if trialBusy}{liveRunner ? "Running..." : "Staging..."}{:else}{liveRunner ? "Run a live trial" : "Stage a held trial"}{/if}
                        </button>
                      {/if}
                      {#if m.external}
                        <button type="button" class="m-check" disabled={smokeBusy !== ""} onclick={() => smokeRuntime(m.id)} title="Checks this machine's installed version is still compatible - version only, no task runs">
                          {smokeBusy === m.id ? "Checking..." : "Check version"}
                        </button>
                      {/if}
                    {/if}
                  </span>
                </div>
                {#if heldForClearance && clearOpen === m.id && !cleared}
                  <div class="m-consent">
                    <p class="mc-attest">By clearing <strong>{m.name}</strong>, you confirm you are allowed to use it here under its own terms and sign-in. Clearing runs nothing - Forge still holds every result until your checks pass.</p>
                    <div class="mc-modes">
                      <button type="button" class="mc-mode" disabled={clearBusy} onclick={() => clearRunner(m.id, "fleet_eval")}>
                        <strong>Admit it to compete</strong>
                        <span>Forge can run it in held trials, and accepted results can count on the scorecard.</span>
                      </button>
                      <button type="button" class="mc-mode" disabled={clearBusy} onclick={() => clearRunner(m.id, "local_companion")}>
                        <strong>Use as a companion</strong>
                        <span>You can use it here, but it stays out of the scorecard.</span>
                      </button>
                    </div>
                  </div>
                {/if}
              </li>
            {/each}
          </ul>
        </div>
      {/each}
      {#if smokeResult}
        <div class="trial-result" data-ok={smokeResult.ok}>
          <span>{smokeResult.line}</span>
        </div>
      {/if}
      {#if trialResult}
        <div class="trial-result" data-ok={trialResult.ok}>
          <span>{trialResult.line}</span>
          {#if trialResult.ok && trialResult.runId}<a href="/forge/runs">Follow its trail &rarr;</a>{/if}
        </div>
      {/if}
      {#if clearResult}
        <div class="trial-result" data-ok={clearResult.ok}><span>{clearResult.line}</span></div>
      {/if}
    </section>

    <!-- Field a fleet: cast many machines x models onto one job. Planning only. -->
    <section class="fleet">
      <h2 class="fleet-head">Field a fleet</h2>
      <p class="fleet-frame">Hand Forge one job and it can put many machines on it at once - your own builder and every outside machine you have connected, each on its best model. This plans the team and holds it. Nothing runs until you start it.</p>
      <div class="fleet-widths">
        {#each [8, 16, 24] as w (w)}
          <button type="button" class="fleet-w" class:active={fleetPlan && !fleetPlan.error && fleetPlan.width === w} disabled={fleetBusy} onclick={() => planFleet(w)}>
            {fleetBusy && fleetWidth === w ? "Planning..." : `${w} wide`}
          </button>
        {/each}
      </div>
      {#if fleetPlan?.error}
        <p class="fleet-err">{fleetPlan.error}</p>
      {:else if fleetPlan}
        <div class="fleet-plan">
          <p class="fleet-lead">Forge would field <strong>{fleetPlan.width} builders</strong> at once{#if fleetPlan.uniqueCasts}, across {fleetPlan.uniqueCasts} machine-and-model combinations{/if}.</p>
          <ul class="fleet-team">
            {#each fleetPlan.workers as w, i (i)}
              <li class="fw" data-native={w.native}>
                <span class="fw-n">{i + 1}</span>
                <span class="fw-runner">{w.runner}</span>
                <span class="fw-on">on {w.model}</span>
              </li>
            {/each}
          </ul>
          {#if fleetPlan.excluded.length}
            <p class="fleet-excluded">Held back: {#each fleetPlan.excluded as e, i (i)}{e.name} ({e.reason}){i < fleetPlan.excluded.length - 1 ? "; " : ""}{/each}.</p>
          {/if}
          {#if fleetPlan.widthsProven.length}
            <p class="fleet-proven">Forge has proven it can plan {fleetPlan.widthsProven.join(", ")} wide. This is the plan only - nothing runs until you start it.</p>
          {/if}
        </div>
      {/if}

      <!-- Run the fleet for real: lanes execute (quarantined), then a human-ratified winner. -->
      <div class="fleet-run-action">
        <button type="button" class="fleet-run-btn" disabled={fleetRunBusy} onclick={() => runFleet(fleetPlan && !fleetPlan.error ? fleetPlan.width : 4)}>
          {fleetRunBusy ? "Running the fleet..." : "Run this fleet live"}
        </button>
        <span>Each machine runs the task in its own isolated copy. Every result is held until you accept it.</span>
      </div>
      {#if fleetRunErr}<p class="fleet-err">{fleetRunErr}</p>{/if}

      {#if fleetRun}
        {@const acceptedLanes = fleetRun.lanes.filter((l) => l.accepted).length}
        <div class="fleet-runpanel">
          <p class="fr-lead">Forge ran <strong>{fleetRun.executed || fleetRun.planned} machines</strong> on this task in parallel. {fleetRun.executed ? "Each one's work is held in an isolated copy until you accept it." : "The lanes are staged and held - run them live to execute."}</p>
          <ul class="fr-lanes">
            {#each fleetRun.lanes as l (l.index)}
              <li class="frl" data-native={l.native}>
                <span class="frl-n">{l.index}</span>
                <span class="frl-body">
                  <span class="frl-runner">{l.runner}<span class="frl-on"> on {l.model}</span></span>
                  <span class="frl-status">{l.status}{#if l.executed} - {l.checkPassed ? "checks passed" : "checks did not pass"}{/if}</span>
                </span>
                <span class="frl-actions">
                  {#if l.accepted}
                    <span class="frl-accepted">accepted</span>
                  {:else if l.executed && l.checkPassed && l.resultReceiptId}
                    <button type="button" class="m-check" disabled={laneAcceptBusy === l.runId} onclick={() => acceptLane(l, fleetRun.id)}>{laneAcceptBusy === l.runId ? "Accepting..." : "Accept"}</button>
                  {/if}
                  {#if l.runId}<a class="frl-watch" href="/forge/runs">Watch</a>{/if}
                </span>
              </li>
            {/each}
          </ul>

          {#if arbitration?.winner_runner_display_name}
            <div class="fr-winner">
              <p class="frw-lead">Forge's pick: <strong>{arbitration.winner_runner_display_name}</strong> on {humanModel(arbitration.winner_model_profile_id)}{#if arbitration.winner_total_score}, scored {arbitration.winner_total_score}{/if}.</p>
              <p class="frw-gate">It chooses only from the lanes you accepted ({arbitration.accepted_lane_count}). This is a recommendation - nothing ships until you ratify it.</p>
            </div>
          {:else if acceptedLanes > 0}
            <button type="button" class="fleet-run-btn secondary" disabled={arbitrateBusy} onclick={() => arbitrateFleet(fleetRun.id)}>{arbitrateBusy ? "Choosing..." : "Pick the winner"}</button>
          {:else if fleetRun.executed}
            <p class="fleet-proven">Each lane is held and waiting for your review. Accept the ones that earned it, then Forge picks a winner from those - never from a lane you did not accept.</p>
          {/if}
        </div>
      {/if}

      {#if compatByRunner.length}
        <details class="evidence">
          <summary>Which machines can pair with which models</summary>
          <ul class="compat">
            {#each compatByRunner as c (c.name)}
              <li><strong>{c.name}</strong> - {c.models.length ? c.models.join(", ") : "no compatible model connected yet"}</li>
            {/each}
          </ul>
        </details>
      {/if}
    </section>

    <!-- What Forge has learned: the compounding story, honest when empty. -->
    <section class="learned">
      <h2 class="learned-head">What Forge has learned</h2>
      {#if entries.length > 0}
        <ul class="lessons">
          {#each entries as e, i (e.distillation_note_id ?? i)}
            <li class="lesson">
              <div class="lesson-head">
                <span class="lesson-what"><strong>{runnerName(e.winning_runner_id)}</strong> solved a {String(e.task_class ?? "task").replace(/_/g, " ")} task with {REASON_PHRASE[e.distillation_reason] ?? "a measured edge"}.</span>
                {#if e.distillation_reason}<span class="lesson-class">{e.distillation_reason.replace(/_/g, " ")}</span>{/if}
              </div>
              <p class="lesson-status">{ledgerDeltaLine(e)}</p>
              {#if loopForNote(e.distillation_note_id)}
                <p class="lesson-loop">{loopLine(loopForNote(e.distillation_note_id))}</p>
              {:else}
                <button type="button" class="m-check lesson-improve" disabled={improveBusy === e.distillation_note_id} onclick={() => improveFromNote(e.distillation_note_id)}>
                  {improveBusy === e.distillation_note_id ? "Scheduling..." : "Improve the native builder from this"}
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      {:else if pendingCandidates.length > 0}
        <div class="learning-pending">
          {#each pendingCandidates as c, i (c.source_run_id ?? i)}
            {#if nativeComparisonRan(c)}
              <div class="lp-row">
                <p class="lp-head">Forge ran both on the same task.</p>
                <p class="lp-line"><strong>{runnerName(c.winning_runner_id)}</strong> passed the held-out {String(c.task_class ?? "task").replace(/_/g, " ")} task, and so did the native builder.</p>
                <p class="lp-next">Forge records a learning only when an outside machine shows a reusable edge over the house runner. If the fix is the same, that is a tie, and nothing is distilled - no fake learning.</p>
              </div>
            {:else}
              <div class="lp-row">
                <p class="lp-head">A learning is in progress.</p>
                <p class="lp-line"><strong>{runnerName(c.winning_runner_id)}</strong> solved a held-out {String(c.task_class ?? "task").replace(/_/g, " ")} task that the house runner is measured on.</p>
                {#if c.scorecard_total_score}<p class="lp-line">A person reviewed it and scored it {c.scorecard_total_score} out of 100.</p>{/if}
                <p class="lp-next">Next, Forge runs its own builder on the same task. Nothing is settled until that head-to-head is done - one accepted win is evidence, not a verdict.</p>
                <button type="button" class="lp-run" disabled={nativeCompareBusy} onclick={runNativeComparison}>{nativeCompareBusy ? "Running native..." : "Run native comparison"}</button>
              </div>
            {/if}
          {/each}
          {#if nativeCompareResult}<p class="lp-result" data-ok={nativeCompareResult.ok}>{nativeCompareResult.line}</p>{/if}
        </div>
      {:else}
        <div class="learned-empty">
          <p>Forge has not learned from another machine yet.</p>
          <p class="learned-quiet">When an outside machine out-builds the house on a task, Forge studies how, turns it into an improvement to its own builder, and shows it here with the run and the before-and-after behind it. No invented numbers: an entry appears only once accepted evidence supports it.</p>
        </div>
      {/if}
    </section>

    <!-- Routing evidence: behind a disclosure on purpose, not a front-door
         leaderboard. The numbers are real run receipts; native is the baseline. -->
    {#if taskClasses.length}
      <details class="evidence">
        <summary>Routing evidence</summary>
        <p class="evidence-frame">
          How each machine has done on this kind of work, counting only runs Forge's checks accepted. Most kinds are still gathering evidence, so Forge keeps the house builder until an outside machine clears the bar on enough accepted runs.
        </p>
        <div class="sc-picker">
          <label for="sc-class">Kind of work</label>
          <select id="sc-class" value={activeTaskClass} onchange={(e) => (selectedTaskClass = e.currentTarget.value)}>
            {#each taskClasses as tc}
              <option value={tc}>{humanTaskClass(tc)}</option>
            {/each}
          </select>
        </div>
        <table class="evidence-table">
          <thead>
            <tr><th>Machine</th><th>Accepted</th><th>Of runs</th><th>Pass rate</th><th>Confidence</th></tr>
          </thead>
          <tbody>
            {#each scorecardForClass as s (s.name)}
              <tr data-external={!s.native}>
                <td>{s.name}{#if s.native} <span class="baseline">baseline</span>{/if}</td>
                <td>{s.accepted}</td>
                <td>{s.sample}</td>
                <td>{s.sample > 0 ? `${s.passRate}%` : "-"}</td>
                <td class="sc-conf" data-weak={s.confidence === "insufficient_sample"}>{humanConfidence(s.confidence)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </details>
    {/if}

    <footer class="lens-foot">
      {data.boundary}.
    </footer>
  {/if}
</ForgeView>

<style>
  .lens-empty { display: grid; gap: 8px; max-width: 460px; margin: 40px auto; text-align: center; }
  .lens-empty h2 { margin: 0; font-size: 18px; font-weight: 650; }
  .lens-empty p { margin: 0; color: var(--mdx-text-muted); font-size: 13.5px; }

  .premier { margin-bottom: 22px; }
  .premier-head { display: flex; justify-content: space-between; margin-bottom: 12px; }
  .premier-head h2 { margin: 3px 0 0; font-size: 18px; font-weight: 650; }
  .premier-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
  .premier-card { padding: 15px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-lg); background: var(--mdx-surface-raised); }
  .premier-title { display: flex; justify-content: space-between; }
  .premier-title h3 { margin: 0; font-size: 15px; }
  .premier-title strong { font-size: 10.5px; }
  .premier-card p { color: var(--mdx-text-secondary); font-size: 12px; }

  .stance {
    display: grid; gap: 7px; padding: 18px 20px; margin-bottom: 22px;
    border-radius: var(--mdx-radius-lg); background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }
  .stance-eyebrow { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em; color: var(--mdx-text-muted); }
  .stance-lead { margin: 0; font-size: 18px; font-weight: 600; line-height: 1.4; }
  .stance-why { margin: 0; font-size: 13.5px; color: var(--mdx-text-secondary); line-height: 1.55; }
  .stance-fallback { margin: 4px 0 0; font-size: 13px; color: var(--mdx-text-muted); line-height: 1.55; }
  .stance-more { margin-top: 6px; }
  .stance-more summary { cursor: pointer; font-size: 12.5px; color: var(--mdx-text-muted); }
  .stance-detail { margin: 8px 0 0; padding-left: 18px; display: grid; gap: 3px; font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.5; }
  .faceoff-action { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-top: 8px; font-size: 12.5px; color: var(--mdx-text-muted); }
  .faceoff-button {
    font: inherit; font-size: 12.5px; font-weight: 650;
    color: var(--mdx-surface); cursor: pointer; padding: 7px 13px;
    border-radius: var(--mdx-radius-md); border: 1px solid var(--mdx-accent-primary);
    background: var(--mdx-accent-primary);
  }
  .faceoff-button:hover:not(:disabled) { filter: brightness(1.04); }
  .faceoff-button:disabled { opacity: 0.6; cursor: default; }

  .faceoff { margin-bottom: 24px; }
  .faceoff-head { margin: 0 0 10px; font-size: 15px; font-weight: 650; }
  .faceoff-card {
    display: grid; gap: 12px; padding: 14px 16px;
    border-radius: var(--mdx-radius-md); background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
  }
  .faceoff-card[data-ok="true"] { border-color: color-mix(in srgb, var(--mdx-accent-success) 25%, var(--mdx-border-subtle)); }
  .faceoff-card[data-ok="false"] { border-color: color-mix(in srgb, var(--mdx-accent-error, #c0392b) 25%, var(--mdx-border-subtle)); }
  .faceoff-line { margin: 0; font-size: 14px; line-height: 1.55; color: var(--mdx-text-primary); }
  .faceoff-more { margin-top: 10px; }
  .faceoff-more summary { cursor: pointer; font-size: 12px; color: var(--mdx-text-muted); }
  .faceoff-facts { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 8px; margin: 9px 0 0; }
  .faceoff-facts div { min-width: 0; }
  .faceoff-facts dt { font-size: 10.5px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--mdx-text-muted); }
  .faceoff-facts dd { margin: 3px 0 0; font-size: 12.5px; color: var(--mdx-text-primary); overflow-wrap: anywhere; }

  .roster { margin-bottom: 26px; }
  .roster-head, .learned-head { margin: 0 0 12px; font-size: 15px; font-weight: 650; }
  .bucket { margin-bottom: 16px; }
  .bucket-label { display: inline-block; margin-bottom: 7px; font-size: 11.5px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.04em; color: var(--mdx-text-muted); }
  .machines { list-style: none; margin: 0; padding: 0; display: grid; gap: 7px; }
  .machine {
    display: grid; gap: 0; padding: 12px 14px;
    border-radius: var(--mdx-radius-md); background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
  }
  .m-row { display: flex; gap: 12px; align-items: flex-start; }
  .m-cleared { flex: none; align-self: center; font-size: 11.5px; font-weight: 650; padding: 3px 10px; border-radius: 999px; color: var(--mdx-text-muted); border: 1px solid var(--mdx-border-subtle); }
  .m-cleared[data-eligible="true"] { color: var(--mdx-accent-success); border-color: color-mix(in srgb, var(--mdx-accent-success) 35%, transparent); }
  .m-consent { margin: 12px 0 2px; padding: 12px 14px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-warning, #d9883a) 7%, transparent); border: 1px solid color-mix(in srgb, var(--mdx-accent-warning, #d9883a) 22%, transparent); display: grid; gap: 10px; }
  .mc-attest { margin: 0; font-size: 12.5px; color: var(--mdx-text-secondary); line-height: 1.55; }
  .mc-modes { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .mc-mode { text-align: left; display: grid; gap: 3px; padding: 10px 12px; border-radius: var(--mdx-radius-md); border: 1px solid var(--mdx-border-subtle); background: var(--mdx-surface-base, transparent); cursor: pointer; }
  .mc-mode:hover:not(:disabled) { border-color: var(--mdx-accent-primary); }
  .mc-mode:disabled { opacity: 0.5; cursor: default; }
  .mc-mode strong { font-size: 13px; }
  .mc-mode span { font-size: 12px; color: var(--mdx-text-muted); line-height: 1.45; }
  @media (max-width: 620px) { .mc-modes { grid-template-columns: 1fr; } }
  .m-dot { flex: none; width: 9px; height: 9px; margin-top: 5px; border-radius: 50%; background: var(--mdx-text-muted); }
  .m-dot[data-group="Running now"] { background: var(--mdx-accent-success); }
  .m-dot[data-group="Ready to run live"] { background: var(--mdx-accent-success); }
  .m-dot[data-group="Ready for a held trial"] { background: var(--mdx-accent-primary); }
  .m-dot[data-group="Open, pending a local check"] { background: var(--mdx-accent-warning, #d9883a); }
  .m-body { display: grid; gap: 2px; min-width: 0; flex: 1; }
  .m-name { font-size: 14px; font-weight: 600; }
  .m-line { font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.5; }
  .m-actions { flex: none; display: flex; gap: 8px; align-items: center; align-self: center; flex-wrap: wrap; justify-content: flex-end; }
  .m-trial {
    font: inherit; font-size: 12.5px; font-weight: 600;
    color: var(--mdx-accent-primary); cursor: pointer; padding: 6px 12px;
    border-radius: var(--mdx-radius-md); border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 35%, transparent);
    background: transparent;
  }
  .m-trial:hover:not(:disabled) { background: color-mix(in srgb, var(--mdx-accent-primary) 8%, transparent); }
  .m-trial:disabled { opacity: 0.5; cursor: default; }
  /* A quiet, secondary diagnostic - never competes with the trial verb. */
  .m-check { font: inherit; font-size: 12px; color: var(--mdx-text-muted); cursor: pointer; padding: 4px 4px; border: none; background: transparent; }
  .m-check:hover:not(:disabled) { color: var(--mdx-text-secondary); text-decoration: underline; }
  .m-check:disabled { opacity: 0.5; cursor: default; }
  .trial-result { margin-top: 10px; display: flex; gap: 10px; align-items: baseline; flex-wrap: wrap; padding: 10px 13px; border-radius: var(--mdx-radius-md); font-size: 12.5px; background: color-mix(in srgb, var(--mdx-accent-primary) 7%, transparent); }
  .trial-result[data-ok="false"] { background: color-mix(in srgb, var(--mdx-accent-error, #c0392b) 8%, transparent); }
  .trial-result a { color: var(--mdx-accent-primary); text-decoration: none; font-weight: 600; }
  .trial-result a:hover { text-decoration: underline; }

  .learned { margin-bottom: 24px; }
  .learning-pending { padding: 16px 18px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-success) 7%, transparent); border: 1px solid color-mix(in srgb, var(--mdx-accent-success) 22%, transparent); display: grid; gap: 10px; }
  .lp-head { margin: 0; font-size: 13.5px; font-weight: 650; }
  .lp-row { display: grid; gap: 3px; }
  .lp-line { margin: 0; font-size: 13.5px; }
  .lp-next { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.55; }
  .lp-run { justify-self: start; margin-top: 4px; font: inherit; font-size: 12.5px; font-weight: 600; color: var(--mdx-accent-primary); cursor: pointer; padding: 6px 12px; border-radius: var(--mdx-radius-md); border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 35%, transparent); background: transparent; }
  .lp-run:hover:not(:disabled) { background: color-mix(in srgb, var(--mdx-accent-primary) 8%, transparent); }
  .lp-run:disabled { opacity: 0.5; cursor: default; }
  .lp-result { margin: 4px 0 0; font-size: 12.5px; color: var(--mdx-text-secondary); }
  .lp-result[data-ok="false"] { color: var(--mdx-accent-error, #c0392b); }
  .learned-empty { padding: 16px 18px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-primary) 6%, transparent); border: 1px solid var(--mdx-border-subtle); }
  .learned-empty p { margin: 0; font-size: 13.5px; }
  .learned-quiet { margin-top: 6px !important; font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.55; }
  .lessons { list-style: none; margin: 0; padding: 0; display: grid; gap: 7px; }
  .lesson { display: grid; gap: 5px; padding: 12px 14px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-success) 6%, transparent); border: 1px solid color-mix(in srgb, var(--mdx-accent-success) 20%, transparent); }
  .lesson-head { display: flex; gap: 10px; align-items: baseline; justify-content: space-between; }
  .lesson-what { font-size: 13.5px; }
  .lesson-status { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.55; }
  .lesson-loop { margin: 4px 0 0; font-size: 12.5px; color: var(--mdx-accent-success); line-height: 1.55; }
  .lesson-improve { justify-self: start; padding-left: 0; }
  .lesson-class { flex: none; font-size: 11.5px; font-family: var(--mdx-font-mono, monospace); color: var(--mdx-text-muted); }

  .evidence { margin-top: 18px; margin-bottom: 22px; }
  .evidence summary { cursor: pointer; font-size: 13px; font-weight: 600; color: var(--mdx-text-secondary); }
  .evidence-frame { margin: 10px 0; font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.55; }
  .sc-picker { display: flex; align-items: center; gap: 8px; margin: 4px 0 12px; font-size: 12.5px; }
  .sc-picker label { color: var(--mdx-text-muted); }
  .sc-picker select { background: var(--mdx-surface-raised, rgba(255,255,255,0.04)); color: var(--mdx-text); border: 1px solid var(--mdx-border-subtle); border-radius: 7px; padding: 5px 9px; font-size: 12.5px; font-family: inherit; }
  .sc-conf[data-weak="true"] { color: var(--mdx-text-muted); }
  .evidence-table { width: 100%; border-collapse: collapse; font-size: 12.5px; }
  .evidence-table th { text-align: left; font-weight: 600; color: var(--mdx-text-muted); padding: 6px 10px; border-bottom: 1px solid var(--mdx-border-subtle); }
  .evidence-table td { padding: 7px 10px; border-bottom: 1px solid color-mix(in srgb, var(--mdx-text-muted) 8%, transparent); }
  .evidence-table th:nth-child(2), .evidence-table td:nth-child(2),
  .evidence-table th:nth-child(3), .evidence-table td:nth-child(3),
  .evidence-table th:nth-child(4), .evidence-table td:nth-child(4) { text-align: right; font-variant-numeric: tabular-nums; }
  .evidence-table th:nth-child(5), .evidence-table td:nth-child(5) { text-align: right; }
  .baseline { font-size: 10.5px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--mdx-accent-success); margin-left: 4px; }

  .fleet { margin-bottom: 26px; }
  .fleet-head { margin: 0 0 8px; font-size: 15px; font-weight: 650; }
  .fleet-frame { margin: 0 0 12px; font-size: 13px; color: var(--mdx-text-secondary); line-height: 1.55; max-width: 680px; }
  .fleet-widths { display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 14px; }
  .fleet-w { font: inherit; font-size: 12.5px; font-weight: 600; color: var(--mdx-text-secondary); cursor: pointer; padding: 7px 16px; border-radius: var(--mdx-radius-pill, 999px); border: 1px solid var(--mdx-border-subtle); background: var(--mdx-surface-raised); }
  .fleet-w:hover:not(:disabled) { border-color: var(--mdx-accent-primary); }
  .fleet-w.active { color: #fff; background: var(--mdx-accent-primary); border-color: var(--mdx-accent-primary); }
  .fleet-w:disabled { opacity: 0.55; cursor: default; }
  .fleet-plan { display: grid; gap: 10px; padding: 16px 18px; border-radius: var(--mdx-radius-lg); background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle); box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card); }
  .fleet-lead { margin: 0; font-size: 14px; }
  .fleet-team { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 7px; }
  .fw { display: flex; align-items: baseline; gap: 9px; padding: 8px 11px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-primary) 6%, transparent); border: 1px solid var(--mdx-border-subtle); }
  .fw[data-native="true"] { background: color-mix(in srgb, var(--mdx-accent-success) 7%, transparent); }
  .fw-n { flex: none; width: 18px; font-size: 11px; font-weight: 700; color: var(--mdx-text-muted); font-variant-numeric: tabular-nums; }
  .fw-runner { font-size: 13px; font-weight: 600; }
  .fw-on { font-size: 12.5px; color: var(--mdx-text-muted); }
  .fleet-excluded { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.5; }
  .fleet-proven { margin: 0; font-size: 12px; color: var(--mdx-text-muted); }
  .fleet-err { margin: 0; font-size: 13px; color: var(--mdx-accent-error, #c0392b); }
  .fleet-run-action { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-top: 14px; font-size: 12.5px; color: var(--mdx-text-muted); }
  .fleet-run-btn { font: inherit; font-size: 13px; font-weight: 650; color: #fff; cursor: pointer; padding: 8px 16px; border-radius: var(--mdx-radius-md); border: none; background: var(--mdx-accent-primary); }
  .fleet-run-btn.secondary { color: var(--mdx-accent-primary); background: transparent; border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 35%, transparent); }
  .fleet-run-btn:hover:not(:disabled) { filter: brightness(1.05); }
  .fleet-run-btn:disabled { opacity: 0.6; cursor: default; }
  .fleet-runpanel { margin-top: 14px; display: grid; gap: 12px; padding: 16px 18px; border-radius: var(--mdx-radius-lg); background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle); box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card); }
  .fr-lead { margin: 0; font-size: 14px; line-height: 1.5; }
  .fr-lanes { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  .frl { display: flex; align-items: center; gap: 11px; padding: 9px 12px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-primary) 5%, transparent); border: 1px solid var(--mdx-border-subtle); }
  .frl[data-native="true"] { background: color-mix(in srgb, var(--mdx-accent-success) 6%, transparent); }
  .frl-n { flex: none; width: 18px; font-size: 11px; font-weight: 700; color: var(--mdx-text-muted); font-variant-numeric: tabular-nums; }
  .frl-body { flex: 1; min-width: 0; display: grid; gap: 1px; }
  .frl-runner { font-size: 13px; font-weight: 600; }
  .frl-on { font-weight: 400; color: var(--mdx-text-muted); }
  .frl-status { font-size: 12px; color: var(--mdx-text-muted); }
  .frl-actions { flex: none; display: flex; align-items: center; gap: 10px; }
  .frl-accepted { font-size: 11.5px; font-weight: 650; color: var(--mdx-accent-success); }
  .frl-watch { font-size: 12px; color: var(--mdx-accent-primary); text-decoration: none; }
  .frl-watch:hover { text-decoration: underline; }
  .fr-winner { padding: 12px 14px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-success) 8%, transparent); border: 1px solid color-mix(in srgb, var(--mdx-accent-success) 24%, transparent); display: grid; gap: 4px; }
  .frw-lead { margin: 0; font-size: 14px; }
  .frw-gate { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.5; }
  .compat { list-style: none; margin: 13px 0 0; padding: 0; display: grid; gap: 10px; font-size: 12.5px; color: var(--mdx-text-secondary); line-height: 1.6; }
  .compat li { padding-bottom: 10px; border-bottom: 1px solid color-mix(in srgb, var(--mdx-text-muted) 7%, transparent); }
  .compat li:last-child { border-bottom: none; padding-bottom: 0; }
  @media (max-width: 620px) { .fleet-team { grid-template-columns: 1fr; } }

  .lens-foot { font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.5; }

  @media (max-width: 760px) {
    .premier-grid { grid-template-columns: 1fr; }
    .premier-head { align-items: start; flex-direction: column; }
    .faceoff-facts { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  }
</style>
