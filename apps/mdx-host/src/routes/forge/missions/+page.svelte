<script>
  // Missions: the cockpit for long-horizon, meaty work. A big goal becomes a
  // governed mission - milestones, validation cadence, budget, fleet width,
  // model policy - and this page is the operator's goal-level view over it.
  //
  // Every state line here is a function of what the routes return. Nothing is
  // hardcoded as "running" or "ready" until the receipts that prove it exist,
  // so the same page honestly shows today (shaped, not executable), the day
  // providers and approvals land, and the live run loop - it just lights up
  // different states. Reads fail soft, so a kernel without these routes opens
  // on the honest "can shape, cannot run live yet" state.
  import { goto, invalidateAll } from "$app/navigation";
  import ForgeView from "../../../lib/ForgeView.svelte";
  import ForgeTrailMap from "../../../lib/ForgeTrailMap.svelte";
  import {
    buildCheckpointPayload,
    checkpointLanded,
    missionState as missionStateLabel,
    isMissionPaused,
    latestCheckpointLine
  } from "../../../lib/missionCheckpoints.js";

  let { data } = $props();
  const actor = $derived(data.session?.user_id ?? "local_user");

  const dashboard = $derived(data.dashboard);
  const preflight = $derived(data.providerPreflight);
  const approvals = $derived(data.liveRunApprovals);
  const scoreboard = $derived(data.scoreboard);

  // Per-mission detail comes from the projection; the dashboard carries the
  // counts and the closed-authority flags. Prefer the projection's missions.
  const missions = $derived(
    (Array.isArray(data.projection?.missions)
      ? data.projection.missions
      : Array.isArray(dashboard?.missions)
        ? dashboard.missions
        : []
    ).map((m, missionIndex) => ({
      missionKey: `${String(m?.mission_id ?? "")}:${String(m?.mission_receipt_id ?? m?.receipt_id ?? missionIndex)}`,
      missionId: String(m?.mission_id ?? ""),
      goal: String(m?.goal ?? ""),
      doneWhen: String(m?.done_when ?? ""),
      providerAllowlist: String(m?.provider_allowlist ?? ""),
      fleetWidth: Number(m?.fleet_width ?? 0),
      maxCostCents: Number(m?.max_cost_cents ?? 0),
      maxRuntimeMs: Number(m?.max_runtime_ms ?? 0),
      checkpointCadenceMinutes: Number(m?.checkpoint_cadence_minutes ?? 0),
      missionReceiptId: String(m?.mission_receipt_id ?? ""),
      policyDecisionId: String(m?.policy_decision_id ?? ""),
      // The checkpoint history the operator drives against: the mission's
      // overall state, the latest note, and how many times it has been steered,
      // paused, or resumed. Absent on kernels before checkpoint controls landed.
      missionState: String(m?.mission_state ?? ""),
      latestCheckpointEvent: String(m?.latest_checkpoint_event ?? ""),
      latestCheckpointSummary: String(m?.latest_checkpoint_summary ?? ""),
      checkpointCount: Number(m?.checkpoint_count ?? 0),
      steeringNoteCount: Number(m?.steering_note_count ?? 0),
      pauseCount: Number(m?.pause_count ?? 0),
      resumeCount: Number(m?.resume_count ?? 0),
      relatedRunIds: Array.isArray(m?.related_run_ids) ? m.related_run_ids.map(String).filter(Boolean) : [],
      relatedFleetIds: Array.isArray(m?.related_fleet_ids) ? m.related_fleet_ids.map(String).filter(Boolean) : [],
      milestones: (Array.isArray(m?.milestones) ? m.milestones : []).map((ms, milestoneIndex) => ({
        milestoneKey: `${String(ms?.milestone_id ?? "")}:${String(ms?.validation_command ?? "")}:${milestoneIndex}`,
        milestoneId: String(ms?.milestone_id ?? ""),
        title: String(ms?.title ?? ""),
        validationCommand: String(ms?.validation_command ?? ""),
        status: String(ms?.status ?? ""),
        acceptanceCriteria: String(ms?.acceptance_criteria ?? ""),
        checkpointReceiptId: String(ms?.checkpoint_receipt_id ?? ""),
        validationStatus: String(ms?.validation_status ?? ""),
        relatedRunId: String(ms?.related_run_id ?? "")
      }))
    }))
  );

  // Axis 1 - the readiness ladder, derived from receipts. Each rung is true
  // only when the route proves it. Live execution is off until the adapter
  // turn-on receipt exists; we never imply otherwise.
  const providersReady = $derived(Number(preflight?.ready_provider_count ?? 0));
  const providerTotal = $derived(Number(preflight?.provider_count ?? dashboard?.provider_matrix_count ?? 4));
  const allCredentialsPresent = $derived(preflight?.all_credentials_present === true);
  const approvalRecorded = $derived(Number(approvals?.approval_count ?? 0) > 0);
  const executionOn = $derived(dashboard?.adapter_execution_allowed === true);
  const hasMissions = $derived(missions.length > 0);
  const missionTrailStage = $derived(
    missions.some((mission) => mission.milestones?.length && mission.milestones.every((item) => ["PASSED", "DONE", "completed"].includes(String(item?.status ?? ""))))
      ? "review"
      : missions.some((mission) => mission.milestones?.some((item) => ["STARTED", "RUNNING", "working"].includes(String(item?.status ?? ""))))
        ? "build"
        : missions.some((mission) => mission.milestones?.some((item) => ["PASSED", "DONE", "completed", "BLOCKED", "FAILED"].includes(String(item?.status ?? ""))))
          ? "prove"
          : "plan"
  );

  // The single state line + safe next move for the hero. Order matters: the
  // first true rung from the top wins, so the line always names the real
  // frontier, not an aspirational one.
  const state = $derived.by(() => {
    if (executionOn) {
      return {
        tone: "ok",
        line: "Live worker execution is on. Missions can run.",
        move: "Open a mission below to follow its milestones."
      };
    }
    if (allCredentialsPresent && approvalRecorded) {
      return {
        tone: "ready",
        line: "Providers are configured and a live-run approval is recorded.",
        move: "Live worker execution turns on next - the last gate before missions run for real."
      };
    }
    if (allCredentialsPresent && !approvalRecorded) {
      return {
        tone: "ready",
        line: "Providers are configured. A live-run approval is the next step.",
        move: "Record the approval envelope (budget, task and agent caps, stop conditions) to clear the next gate."
      };
    }
    if (hasMissions) {
      return {
        tone: "shaping",
        line: "Mission packets are ready. Launch a milestone through the normal run or fleet rails.",
        move: "Forge will refuse with the missing model, repo, or authority gate if a milestone cannot run yet."
      };
    }
    return {
      tone: "shaping",
      line: "Forge can shape a long-horizon mission, but can't run live workers yet.",
      move: "Describe a meaty goal on a build, then Forge plans milestones, budget, and fleet width here."
    };
  });

  // Milestone status -> human label. The keys are the kernel's milestone states
  // (see milestone_status_for_checkpoint_event in harness_long_horizon_mission.rs):
  // a milestone starts PENDING, a launched run flips it to RUNNING, and the run's
  // outcome records a completed or blocked checkpoint. The simple aliases below
  // cover any leaner status a future path might emit.
  const MILESTONE = {
    PENDING_LOCAL_EXECUTION: { label: "Waiting to run", tone: "held" },
    RUNNING_LOCAL_CHECKPOINT: { label: "Running", tone: "active" },
    COMPLETED_LOCAL_CHECKPOINT: { label: "Passed", tone: "ok" },
    BLOCKED_NEEDS_OPERATOR: { label: "Needs you", tone: "warn" },
    RUNNING: { label: "Running", tone: "active" },
    PASSED: { label: "Passed", tone: "ok" },
    DONE: { label: "Passed", tone: "ok" },
    FAILED: { label: "Failed", tone: "warn" },
    BLOCKED: { label: "Needs you", tone: "warn" }
  };
  function milestone(status) {
    return MILESTONE[status] ?? { label: status.toLowerCase().replace(/_/g, " ") || "planned", tone: "" };
  }

  // The mission's outcome, derived only from milestone receipts. Today every
  // milestone is "waiting to run", so the honest verdict is "not run yet" -
  // the same block lights up to passed/failed the moment validation receipts
  // land, with no redesign. Phase drives the pill tone.
  function outcomeOf(m) {
    const total = m.milestones.length;
    const tones = m.milestones.map((ms) => milestone(ms.status).tone);
    const passed = tones.filter((t) => t === "ok").length;
    const failed = tones.filter((t) => t === "warn").length;
    const running = tones.filter((t) => t === "active").length;
    if (total === 0) {
      return { phase: "admitted", tone: "shaping", line: "Mission admitted. Milestones are being planned." };
    }
    if (failed > 0) {
      return { phase: "failed", tone: "warn", line: `${failed} of ${total} checks did not pass.` };
    }
    if (passed === total) {
      return { phase: "passed", tone: "ok", line: `Passed all ${total === 1 ? "1 check" : `${total} checks`}.` };
    }
    if (running > 0 || passed > 0) {
      return { phase: "running", tone: "active", line: `Running. ${passed} of ${total} checks passed so far.` };
    }
    return {
      phase: "waiting",
      tone: "shaping",
      line: "Milestones are planned and waiting. Checks run once live execution is on - no result yet."
    };
  }

  const PHASE_LABEL = {
    admitted: "Admitted",
    waiting: "Not run yet",
    running: "Running",
    passed: "Passed",
    failed: "Failed"
  };

  function plural(n, word) {
    return `${n} ${word}${n === 1 ? "" : "s"}`;
  }
  function money(cents) {
    if (!cents) return "";
    const dollars = cents / 100;
    return dollars >= 1 ? `$${dollars.toFixed(dollars % 1 === 0 ? 0 : 2)}` : `${cents}c`;
  }
  function duration(ms) {
    if (!ms) return "";
    const hours = ms / 3_600_000;
    if (hours >= 1) return `${hours % 1 === 0 ? hours : hours.toFixed(1)} ${hours === 1 ? "hour" : "hours"}`;
    const minutes = Math.round(ms / 60_000);
    return plural(minutes, "minute");
  }

  let launchBusy = $state("");
  let launchFlash = $state({});

  function launchKey(m, ms, shape) {
    return `${m.missionId}:${ms.milestoneId}:${shape}`;
  }
  function missionFlash(missionId, ok, line) {
    launchFlash = { ...launchFlash, [missionId]: { ok, line } };
  }
  function delegatedRunId(packet) {
    return String(packet?.delegated_response?.run_id ?? "");
  }
  function delegatedFleetId(packet) {
    return String(packet?.delegated_response?.fleet_id ?? packet?.auto_fleet_plan?.fleet_id ?? "");
  }
  async function launchMission(m, ms, shape) {
    const key = launchKey(m, ms, shape);
    if (launchBusy) return;
    launchBusy = key;
    missionFlash(m.missionId, true, "Launching the milestone...");
    try {
      const response = await fetch("/api/kernel/forge/long-horizon-mission-launches.json", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          mission_id: m.missionId,
          mission_milestone_id: ms.milestoneId,
          shape,
          requested_workers: shape === "fleet" ? Math.max(2, Number(m.fleetWidth) || 4) : 1,
          max_turns: shape === "fleet" ? 30 : 20,
          actor_id: actor
        })
      });
      const packet = await response.json();
      if (packet.status === "MISSION_LAUNCH_DELEGATED") {
        missionFlash(m.missionId, true, "Milestone launched. Opening the work trail.");
        if (packet.delegated_route === "/forge/runs.json") {
          const runId = delegatedRunId(packet);
          await goto(runId ? `/forge/runs?run=${encodeURIComponent(runId)}` : "/forge/runs");
        } else if (packet.delegated_route === "/forge/fleet-runs.json") {
          const fleetId = delegatedFleetId(packet);
          await goto(fleetId ? `/forge/fleets?fleet_id=${encodeURIComponent(fleetId)}` : "/forge/fleets");
        } else {
          await goto("/forge/runs");
        }
        return;
      }
      missionFlash(
        m.missionId,
        false,
        packet.reason || packet.delegated_response?.reason || "That milestone did not launch. Tighten the scope or try a normal run."
      );
      await invalidateAll();
    } catch (error) {
      missionFlash(m.missionId, false, "That milestone did not launch. Try again.");
    } finally {
      launchBusy = "";
    }
  }

  // Driving a mission: the operator can pause it, resume it, steer the crew
  // with a note, and mark a milestone started, passed, or blocked. Each verb
  // posts one checkpoint. The kernel is fail-closed, so a refusal comes back as
  // a refusal - we never fake success.
  let steerNote = $state({});
  let checkpointBusy = $state("");
  function checkpointKey(missionId, event, milestoneId = "") {
    return `${missionId}:${event}:${milestoneId}`;
  }
  async function checkpoint(mission, event, { milestoneId = "", note = "" } = {}) {
    const key = checkpointKey(mission.missionId, event, milestoneId);
    if (checkpointBusy) return;
    const { body, error } = buildCheckpointPayload({
      missionId: mission.missionId,
      event,
      milestoneId,
      note,
      actor
    });
    if (error) {
      missionFlash(mission.missionId, false, error);
      return;
    }
    checkpointBusy = key;
    try {
      const response = await fetch("/api/kernel/forge/long-horizon-mission-checkpoints.json", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body)
      });
      const packet = await response.json();
      if (checkpointLanded(packet)) {
        if (event === "mission_steered") steerNote = { ...steerNote, [mission.missionId]: "" };
        missionFlash(mission.missionId, true, checkpointConfirmation(event));
        await invalidateAll();
      } else {
        missionFlash(mission.missionId, false, packet?.reason || "That did not record. The kernel held it.");
      }
    } catch (error) {
      missionFlash(mission.missionId, false, "Nothing recorded - that did not land.");
    } finally {
      checkpointBusy = "";
    }
  }
  function checkpointConfirmation(event) {
    return {
      mission_paused: "Paused. The crew stops at the next safe point.",
      mission_resumed: "Resumed. The crew picks the work back up.",
      mission_steered: "Sent. The crew reads your note on its next turn.",
      milestone_started: "Marked as started.",
      milestone_completed: "Marked as passed.",
      milestone_blocked: "Flagged for you. It waits for a decision."
    }[event] ?? "Recorded.";
  }
  async function steerMission(mission) {
    await checkpoint(mission, "mission_steered", { note: (steerNote[mission.missionId] ?? "").trim() });
  }

  // Axis 2 - machine readiness, read from the eval lane. Quiet and honest:
  // it reads as the local proof it is today, never "scores well", because the
  // live runner receipts that would earn that claim do not exist yet.
  const evalLane = $derived.by(() => {
    if (!scoreboard) return null;
    return {
      tasks: Number(scoreboard.benchmark_task_count ?? 0),
      dimensions: Number(scoreboard.scoring_dimensions?.length ?? scoreboard.scoring_dimension_count ?? 0),
      providers: Number(scoreboard.model_matrix?.length ?? 0),
      measuredLive: scoreboard.authority_boundary?.codex_cli_live_execution_allowed === true
    };
  });
</script>

<svelte:head><title>Missions - MDx</title></svelte:head>

<ForgeView
  title="Missions"
  backTo={{ href: "/forge/runs", label: "Trails" }}
  subtitle="A big goal Forge can carry on its own - across many steps, with checks after each milestone, a budget, and a fleet of workers. This is where you hand it something meaty and follow what it is doing."
>

  <ForgeTrailMap
    scale="Mission"
    active={missionTrailStage}
    title="Milestones, each ending in proof"
    detail="A mission carries one goal across bounded milestones, with checks and human decisions at every boundary."
  />

  <!-- The one thing to know + the safe next move, derived from receipts. -->
  <div class="state" data-tone={state.tone}>
    <p class="state-line">{state.line}</p>
    <p class="state-move">{state.move}</p>
    <ul class="ladder" aria-label="What turns a mission from shaped into live">
      <li data-done={allCredentialsPresent}>
        <span class="rung-dot" aria-hidden="true"></span>
        <span class="rung-text">Models ready
          <span class="rung-meta">{providersReady} of {providerTotal} provider profiles configured</span>
        </span>
      </li>
      <li data-done={approvalRecorded}>
        <span class="rung-dot" aria-hidden="true"></span>
        <span class="rung-text">Live-run approval recorded
          <span class="rung-meta">budget, task and agent caps, stop conditions</span>
        </span>
      </li>
      <li data-done={executionOn}>
        <span class="rung-dot" aria-hidden="true"></span>
        <span class="rung-text">Live worker execution on
          <span class="rung-meta">the last gate before a mission runs for real</span>
        </span>
      </li>
    </ul>
  </div>

  {#if !hasMissions}
    <div class="forge-empty">
      <h2>No mission yet</h2>
      <p>A quick fix is a run. A bigger goal - a refactor with tests, a meaty feature, or a brand-new build - becomes a mission: Forge breaks it into milestones, runs your checks after each one, holds to a budget and a fleet of workers, and stops for you at the decisions that need a person.</p>
      <a class="mdx-btn primary" href="/forge">Describe work &rarr;</a>
    </div>
  {:else}
    <ul class="mission-list">
      {#each missions as m (m.missionKey)}
        {@const passed = m.milestones.filter((ms) => milestone(ms.status).tone === "ok").length}
        {@const oc = outcomeOf(m)}
        {@const paused = isMissionPaused(m)}
        {@const stateInfo = missionStateLabel(m.missionState)}
        <li class="mission">
          <div class="mission-head">
            <p class="mission-goal">{m.goal}</p>
            <span class="mission-progress">{passed} of {plural(m.milestones.length, "check")} passed</span>
          </div>
          {#if m.doneWhen}<p class="mission-done"><span class="k">Done when</span> {m.doneWhen}</p>{/if}

          <!-- Drive the mission: pause, resume, and leave the crew a note. The
               state pill and latest note are read straight from the checkpoint
               receipts, so they never claim more than what was recorded. -->
          <div class="control-bar">
            <div class="control-state">
              {#if stateInfo.label}<span class="state-pill" data-tone={stateInfo.tone}>{stateInfo.label}</span>{/if}
              {#if latestCheckpointLine(m)}<span class="control-latest">{latestCheckpointLine(m)}</span>{/if}
            </div>
            <div class="control-verbs">
              {#if paused}
                <button type="button" class="mdx-btn small primary" disabled={!!checkpointBusy} onclick={() => checkpoint(m, "mission_resumed")}>
                  {checkpointBusy === checkpointKey(m.missionId, "mission_resumed") ? "Resuming..." : "Resume"}
                </button>
              {:else if m.missionState === "IN_PROGRESS_LOCAL_CHECKPOINTS" || m.missionState === "BLOCKED_NEEDS_OPERATOR"}
                <button type="button" class="mdx-btn small" disabled={!!checkpointBusy} onclick={() => checkpoint(m, "mission_paused")}>
                  {checkpointBusy === checkpointKey(m.missionId, "mission_paused") ? "Pausing..." : "Pause"}
                </button>
              {/if}
            </div>
          </div>
          <div class="steer-crew">
            <input type="text" bind:value={steerNote[m.missionId]} placeholder="Leave a note for the crew - e.g. keep the first milestone tight" aria-label="Note for the crew" />
            <button type="button" class="mdx-btn small" disabled={!!checkpointBusy || !(steerNote[m.missionId] ?? "").trim()} onclick={() => steerMission(m)}>
              {checkpointBusy === checkpointKey(m.missionId, "mission_steered") ? "Sending..." : "Steer"}
            </button>
          </div>

          {#if m.milestones.length}
            <ol class="milestones">
              {#each m.milestones as ms (ms.milestoneKey)}
                <li class="milestone" data-tone={milestone(ms.status).tone}>
                  <span class="ms-dot" aria-hidden="true"></span>
                  <span class="ms-title">{ms.title}</span>
                  <code class="ms-check">{ms.validationCommand}</code>
                  <span class="ms-status">{milestone(ms.status).label}</span>
                  {#if milestone(ms.status).tone !== "ok"}
                    <span class="ms-actions">
                      <button
                        type="button"
                        disabled={!!launchBusy}
                        onclick={() => launchMission(m, ms, "run")}
                      >
                        {launchBusy === launchKey(m, ms, "run") ? "Starting..." : "Run"}
                      </button>
                      {#if m.fleetWidth > 1}
                        <button
                          type="button"
                          disabled={!!launchBusy}
                          onclick={() => launchMission(m, ms, "fleet")}
                        >
                          {launchBusy === launchKey(m, ms, "fleet") ? "Starting..." : "Fleet"}
                        </button>
                      {/if}
                      <button
                        type="button"
                        class="ms-mark"
                        disabled={!!checkpointBusy}
                        title="Record this milestone as passed without a launch"
                        onclick={() => checkpoint(m, "milestone_completed", { milestoneId: ms.milestoneId })}
                      >
                        {checkpointBusy === checkpointKey(m.missionId, "milestone_completed", ms.milestoneId) ? "..." : "Mark passed"}
                      </button>
                      <button
                        type="button"
                        class="ms-mark"
                        disabled={!!checkpointBusy}
                        title="Flag this milestone as needing your call"
                        onclick={() => checkpoint(m, "milestone_blocked", { milestoneId: ms.milestoneId })}
                      >
                        {checkpointBusy === checkpointKey(m.missionId, "milestone_blocked", ms.milestoneId) ? "..." : "Flag"}
                      </button>
                    </span>
                  {/if}
                </li>
              {/each}
            </ol>
            {#if launchFlash[m.missionId]?.line}
              <p class:good={launchFlash[m.missionId].ok} class:bad={!launchFlash[m.missionId].ok} class="mission-flash">
                {launchFlash[m.missionId].line}
              </p>
            {/if}
            <p class="cadence">Checks run after each milestone.</p>
          {/if}

          <div class="facts">
            {#if m.fleetWidth}<span class="fact"><strong>{plural(m.fleetWidth, "worker")}</strong></span>{/if}
            {#if m.maxCostCents}<span class="fact">up to <strong>{money(m.maxCostCents)}</strong></span>{/if}
            {#if m.maxRuntimeMs}<span class="fact">up to <strong>{duration(m.maxRuntimeMs)}</strong></span>{/if}
            {#if m.checkpointCadenceMinutes}<span class="fact quiet">checks every {plural(m.checkpointCadenceMinutes, "minute")}</span>{/if}
            {#if m.providerAllowlist}<span class="fact quiet">{m.providerAllowlist}</span>{/if}
          </div>

          <div class="outcome" data-tone={oc.tone}>
            <span class="oc-label">Outcome</span>
            <span class="oc-pill" data-tone={oc.tone}>{PHASE_LABEL[oc.phase]}</span>
            <span class="oc-line">{oc.line}</span>
          </div>

          <p class="mission-boundary">Forge won't write to your live repo, ship without you, or trust a
          worker's output until your checks pass.</p>

          <details class="evidence">
            <summary>Evidence</summary>
            <div class="ev-group">
              <p class="ev-head">Recorded now</p>
              <dl>
                {#if m.missionReceiptId}<dt>Mission receipt</dt><dd>{m.missionReceiptId}</dd>{/if}
                {#if m.policyDecisionId}<dt>Policy decision</dt><dd>{m.policyDecisionId}</dd>{/if}
                {#if m.milestones.length}<dt>Milestone plan</dt><dd>{plural(m.milestones.length, "checkpoint")}, a check after each one</dd>{/if}
              </dl>
            </div>
            {#if m.milestones.length}
              <div class="ev-group">
                <p class="ev-head">What each check must prove</p>
                <dl>
                  {#each m.milestones as ms (ms.milestoneKey)}
                    <dt>{ms.title}</dt><dd>{ms.acceptanceCriteria}</dd>
                  {/each}
                </dl>
              </div>
            {/if}
            <div class="ev-group">
              <p class="ev-head">Launch rail</p>
              <p class="ev-note">The Run and Fleet buttons call the mission launch route, which delegates to the governed run or fleet rails with this milestone attached. Nothing is marked passed until the delegated check actually runs.</p>
            </div>
          </details>
        </li>
      {/each}
    </ul>
  {/if}

  {#if evalLane}
    <details class="eval">
      <summary>How ready is Forge for work like this</summary>
      <p>
        {#if evalLane.measuredLive}
          Measured live across {plural(evalLane.tasks, "senior-engineer task")} on {plural(evalLane.dimensions, "dimension")}.
        {:else}
          This class of work is covered by the eval lane: {plural(evalLane.tasks, "senior-engineer task")} scored on
          {plural(evalLane.dimensions, "dimension")} across {plural(evalLane.providers, "provider profile")}. These are local
          proof runs - the live measured scores arrive once live execution is on.
        {/if}
      </p>
      <a class="eval-link" href="/forge/models">See the full readiness story in Models &rarr;</a>
    </details>
  {/if}

  <p class="boundary">Mission admission is recorded and governed. It does not call providers, run an
  outside worker, or write production state - those stay closed until their own gates and your approval.</p>
</ForgeView>

<style>

  /* The state hero: one line, one move, then the readiness ladder. */
  .state { display: grid; gap: 6px; padding: 18px 20px; border-radius: var(--mdx-radius-lg, 12px); background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle); box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card); border-left: 3px solid var(--mdx-text-muted); }
  .state[data-tone="ok"] { border-left-color: var(--mdx-accent-success); }
  .state[data-tone="ready"] { border-left-color: var(--mdx-accent-primary); }
  .state[data-tone="shaping"] { border-left-color: var(--mdx-accent-warning, var(--mdx-text-muted)); }
  .state-line { margin: 0; font-size: 15.5px; font-weight: 650; }
  .state-move { margin: 0; font-size: 13.5px; color: var(--mdx-text-muted); max-width: 600px; }
  .ladder { list-style: none; margin: 8px 0 0; padding: 0; display: grid; gap: 8px; }
  .ladder li { display: flex; align-items: flex-start; gap: 10px; font-size: 13px; }
  .rung-dot { margin-top: 3px; width: 11px; height: 11px; border-radius: 999px; border: 2px solid color-mix(in srgb, var(--mdx-text-muted) 45%, transparent); flex: none; }
  .ladder li[data-done="true"] .rung-dot { background: var(--mdx-accent-success); border-color: var(--mdx-accent-success); }
  .rung-text { display: grid; gap: 1px; }
  .ladder li[data-done="false"] .rung-text { color: var(--mdx-text-muted); }
  .rung-meta { font-size: 11.5px; color: var(--mdx-text-muted); }

  .forge-empty { display: grid; justify-items: center; text-align: center; gap: 12px; max-width: 460px; margin: 44px auto; }
  .forge-empty h2 { margin: 0; font-size: 18px; font-weight: 650; }
  .forge-empty p { margin: 0; color: var(--mdx-text-muted); font-size: 13.5px; line-height: 1.5; }

  .mission-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 12px; }
  .mission { display: grid; gap: 12px; padding: 18px 20px; border-radius: var(--mdx-radius-lg, 12px); background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle); box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card); }
  .mission-head { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }
  .mission-goal { margin: 0; font-size: 15px; font-weight: 650; }
  .mission-progress { font-size: 12.5px; color: var(--mdx-text-muted); white-space: nowrap; }
  .mission-done { margin: 0; font-size: 13px; color: var(--mdx-text-muted); }
  .mission-done .k { font-weight: 650; color: var(--mdx-text); }

  .milestones { list-style: none; margin: 0; padding: 0; display: grid; gap: 6px; }
  .milestone { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 10px; font-size: 12.5px; padding: 7px 10px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-text-muted) 6%, transparent); }
  .ms-dot { width: 9px; height: 9px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-text-muted) 40%, transparent); }
  .milestone[data-tone="ok"] .ms-dot { background: var(--mdx-accent-success); }
  .milestone[data-tone="active"] .ms-dot { background: var(--mdx-accent-primary); }
  .milestone[data-tone="warn"] .ms-dot { background: var(--mdx-tone-warn-text, var(--mdx-accent-warning)); }
  .ms-title { font-weight: 600; }
  .ms-check { grid-column: 2; font-family: var(--mdx-font-mono, monospace); font-size: 11.5px; color: var(--mdx-text-muted); justify-self: start; }
  .ms-status { color: var(--mdx-text-muted); white-space: nowrap; }
  .ms-actions { grid-column: 2 / -1; display: flex; flex-wrap: wrap; gap: 6px; }
  .ms-actions button { border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-pill, 999px); background: var(--mdx-surface-base); color: var(--mdx-text-secondary); font: inherit; font-size: 11.5px; font-weight: 650; padding: 4px 10px; cursor: pointer; }
  .ms-actions button:hover:not(:disabled) { color: var(--mdx-text-primary); border-color: color-mix(in srgb, var(--mdx-accent-primary) 45%, var(--mdx-border-subtle)); }
  .ms-actions button:disabled { opacity: 0.55; cursor: not-allowed; }
  .ms-mark { color: var(--mdx-text-muted); }
  .milestone[data-tone="ok"] .ms-status { color: var(--mdx-accent-success); }

  /* Mission controls: drive it (pause/resume/steer), with the state and latest
     note read straight from the checkpoint receipts. */
  .control-bar { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
  .control-state { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; min-width: 0; }
  .state-pill { font-size: 11.5px; font-weight: 650; padding: 2px 10px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-text-muted) 14%, transparent); color: var(--mdx-text-muted); white-space: nowrap; }
  .state-pill[data-tone="ok"] { background: color-mix(in srgb, var(--mdx-accent-success) 18%, transparent); color: var(--mdx-accent-success); }
  .state-pill[data-tone="active"] { background: color-mix(in srgb, var(--mdx-accent-primary) 16%, transparent); color: var(--mdx-accent-primary); }
  .state-pill[data-tone="warn"] { background: color-mix(in srgb, var(--mdx-accent-warning, var(--mdx-text-muted)) 18%, transparent); color: var(--mdx-tone-warn-text, var(--mdx-accent-warning)); }
  .control-latest { font-size: 12.5px; color: var(--mdx-text-muted); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 420px; }
  .control-verbs { display: flex; gap: 7px; flex: none; }
  .steer-crew { display: flex; gap: 8px; }
  .steer-crew input { flex: 1; min-width: 0; padding: 8px 11px; font-size: 13px; border-radius: var(--mdx-radius-md); border: 1px solid var(--mdx-border-default); background: var(--mdx-surface-base); color: inherit; }
  @media (max-width: 560px) { .milestone { grid-template-columns: auto 1fr; } .ms-check { display: none; } }

  .cadence { margin: 0; font-size: 12px; color: var(--mdx-text-muted); }
  .mission-flash { margin: -2px 0 0; font-size: 12.5px; }
  .mission-flash.good { color: var(--mdx-accent-success); }
  .mission-flash.bad { color: var(--mdx-tone-danger-text, #b42318); }
  .facts { display: flex; flex-wrap: wrap; gap: 8px 16px; font-size: 12.5px; }
  .fact strong { font-weight: 650; }
  .fact.quiet { color: var(--mdx-text-muted); }
  .mission-boundary { margin: 0; font-size: 12px; color: var(--mdx-text-muted); }

  /* Outcome - the receipt-driven verdict. Honest "not run yet" today, lights
     up to passed/failed when validation receipts land. */
  .outcome { display: flex; align-items: baseline; flex-wrap: wrap; gap: 6px 10px; padding-top: 8px; border-top: 1px solid color-mix(in srgb, var(--mdx-text-muted) 12%, transparent); }
  .oc-label { font-size: 11.5px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.03em; color: var(--mdx-text-muted); }
  .oc-pill { font-size: 11.5px; font-weight: 650; padding: 1px 8px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-text-muted) 14%, transparent); color: var(--mdx-text-muted); }
  .oc-pill[data-tone="ok"] { background: color-mix(in srgb, var(--mdx-accent-success) 18%, transparent); color: var(--mdx-accent-success); }
  .oc-pill[data-tone="active"] { background: color-mix(in srgb, var(--mdx-accent-primary) 16%, transparent); color: var(--mdx-accent-primary); }
  .oc-pill[data-tone="warn"] { background: color-mix(in srgb, var(--mdx-accent-warning, var(--mdx-text-muted)) 18%, transparent); color: var(--mdx-tone-warn-text, var(--mdx-accent-warning)); }
  .oc-line { font-size: 13px; color: var(--mdx-text-muted); }

  .evidence, .eval { font-size: 12.5px; }
  .evidence summary, .eval summary { cursor: pointer; color: var(--mdx-text-muted); font-size: 12.5px; }
  .ev-group { margin-top: 10px; }
  .ev-head { margin: 0 0 4px; font-size: 11px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.03em; color: var(--mdx-text-muted); }
  .evidence dl { display: grid; grid-template-columns: auto 1fr; gap: 4px 12px; margin: 0; }
  .evidence dt { font-weight: 650; color: var(--mdx-text-muted); }
  .evidence dd { margin: 0; font-size: 11.5px; word-break: break-word; }
  .ev-note { margin: 0; font-size: 12px; color: var(--mdx-text-muted); max-width: 560px; line-height: 1.5; }
  .eval p { margin: 8px 0 0; color: var(--mdx-text-muted); max-width: 600px; line-height: 1.5; }
  .eval-link { display: inline-block; margin-top: 8px; font-size: 12.5px; font-weight: 600; color: var(--mdx-accent-primary); text-decoration: none; }

  .boundary { margin: 4px 0 0; font-size: 12px; color: var(--mdx-text-muted); max-width: 640px; }
</style>
