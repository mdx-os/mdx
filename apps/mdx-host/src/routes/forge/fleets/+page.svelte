<script>
  // Fleets: hand one spec to a team of build agents. The planner proposes
  // how to split the work into parallel streams with decided seams; you
  // ratify the split (nothing is spent before your yes); the streams
  // build simultaneously, each confined to its own files; integration
  // makes them one branch; a fresh reviewer reads the result cold. The
  // branch then waits for your recorded ship call like any other build.
  import { createMdxClient } from "@mdx/client";
  import { invalidateAll, invalidate, goto } from "$app/navigation";
  import { startLivePoll } from "../../../lib/livePoll.js";
  import { onMount } from "svelte";
  import {
    joinFleets,
    phaseLabel,
    laneLabel,
    reviewSummary,
    coderLabel,
    planReviewLine,
    repairSummary,
    fleetNeedsRecovery,
    fleetHasIntegrationFailure,
    prepareMissionFromFleet,
    buildMissionCreatePayload
  } from "../../../lib/fleetRuns.js";
  import ForgeView from "../../../lib/ForgeView.svelte";
  import ForgeTrailMap from "../../../lib/ForgeTrailMap.svelte";

  let { data } = $props();
  const fleets = $derived(joinFleets(data.plans, data.runs));
  const anyBuilding = $derived(fleets.some((fleet) => fleet.running));
  const trailStage = $derived(
    fleets.some((fleet) => fleet.finished || fleet.phase === "finished")
      ? "review"
      : anyBuilding
        ? "build"
        : fleets.some((fleet) => fleet.started)
          ? "prove"
          : "plan"
  );

  // Keep the live poll fail-active, the way the runs page defaults a run to
  // "running". A fleet is live from start to finish; a slow projection tick
  // that aborts to null must not read as "nothing building" and freeze the
  // view. Latch once a started, not-finished fleet is seen; release only when
  // every fleet is terminal.
  const anyLive = $derived(fleets.some((fleet) => fleet.started && !fleet.finished));
  const anyPlanning = $derived(
    fleets.some((fleet) => fleet.phase === "planning" || fleet.phase === "planning_delayed")
  );
  const allTerminal = $derived(fleets.length > 0 && fleets.every((fleet) => fleet.finished));
  let sawLive = $state(false);
  $effect(() => {
    if (anyLive) sawLive = true;
    else if (allTerminal) sawLive = false;
  });

  // Live worker capacity - the floor is bounded (reserve, queue, then refuse),
  // shown only while building so it reads as a pulse, not a dashboard.
  const capacity = $derived.by(() => {
    const c = data.capacity;
    if (!c || !c.worker_capacity) return null;
    return {
      active: Number(c.active_now ?? 0),
      capacity: Number(c.worker_capacity ?? 0),
      queued: Number(c.queue_depth ?? 0)
    };
  });

  // Per-fleet and per-lane result rollups from the control-plane fold, indexed
  // for the cards. A run that has not started has no result yet.
  const fleetResultById = $derived.by(() => {
    const rows = Array.isArray(data.controlPlane?.fleet_results) ? data.controlPlane.fleet_results : [];
    const map = {};
    for (const r of rows) map[String(r?.fleet_id ?? "")] = r;
    return map;
  });
  const laneResultByKey = $derived.by(() => {
    const rows = Array.isArray(data.controlPlane?.lane_results) ? data.controlPlane.lane_results : [];
    const map = {};
    for (const r of rows) map[`${String(r?.fleet_id ?? "")} ${String(r?.stream_id ?? "")}`] = r;
    return map;
  });
  function tok(n) {
    const v = Number(n ?? 0);
    return v >= 1_000_000 ? `${(v / 1_000_000).toFixed(1)}M` : v >= 1000 ? `${Math.round(v / 1000)}k` : String(v);
  }
  const RESULT_STATUS = {
    clean: "Clean",
    running: "Running",
    needs_you: "Needs you",
    finished_with_notes: "Finished with notes"
  };
  const repos = $derived(
    (Array.isArray(data.repos?.repos) ? data.repos.repos : []).map((row) => ({
      repoId: String(row?.repo_id ?? ""),
      label: String(row?.label ?? "")
    }))
  );

  const client = $derived(createMdxClient({ baseUrl: "/api/kernel", session: data.session }));
  const actor = $derived(data.session?.user_id ?? "local_user");
  const focusFleetId = $derived(data.focusFleetId ?? "");
  const preferredRepoId = $derived(data.preferredRepoId ?? "");
  const preferredCloudEnvironmentId = $derived(data.preferredCloudEnvironmentId ?? "");

  let openId = $state("");
  let busy = $state("");
  let cardFlash = $state({});
  function flash(fleetId, ok, line) {
    cardFlash = { ...cardFlash, [fleetId]: { ok, line } };
  }

  function proofLine(proof) {
    if (!proof?.status) return "Proof gates appear after the fleet starts.";
    if (proof.principalEngineerEvidenceReady) return "Ready for principal review - stream and integration proof are recorded.";
    if (proof.status === "needs_principal_attention") return "Stopped for your call - the repair budget is spent.";
    if (proof.status === "revision_ready_for_review" || proof.status === "lane_revision_ready_for_integration") return "Repaired within budget - waiting for your review.";
    if (proof.status === "ready_for_principal_review") return "Ready for principal review.";
    if (proof.status === "needs_attention") return "Needs attention - at least one stream did not land cleanly.";
    if (proof.status === "planned_waiting_for_start") return "Planned - proof gates will update when the fleet starts.";
    if (proof.status === "waiting_for_integration") return "Streams finished. Integration review still needs to land.";
    return "Building or waiting - proof gates are filling in as receipts land.";
  }

  function geometryLine(geometry) {
    const width = geometry?.streamCount || geometry?.requestedWidth || 0;
    if (!width) return "";
    const concurrent = geometry.maxConcurrentWorkers ? `, up to ${geometry.maxConcurrentWorkers} concurrent` : "";
    return `${width} disjoint ${width === 1 ? "stream" : "streams"}${concurrent}`;
  }

  function geometryMixLine(geometry) {
    const coderLine = (geometry?.byCoder ?? [])
      .map((row) => `${row.count}x ${coderLabel(row.label === "default" ? "" : row.label)}`)
      .join(", ");
    const sensitivityLine = (geometry?.bySensitivity ?? [])
      .map((row) => `${row.count} ${row.label}`)
      .join(", ");
    return [coderLine, sensitivityLine].filter(Boolean).join(" - ");
  }

  // A status chip, not a button. The real actions (approve, start, review) live
  // inside the expanded row - this only previews what stage the fleet is at, so
  // it must read as a state, not an imperative CTA.
  function fleetNextLabel(fleet) {
    if (fleet.phase === "planning") return "Planner working";
    if (fleet.phase === "planning_delayed") return "Still planning";
    if (fleet.phase === "planning_needs_attention") return "Needs attention";
    if (fleet.phase === "proposed") return "Split to review";
    if (fleet.phase === "ready") return "Ready to start";
    if (fleet.phase === "building") return "Lanes running";
    if (fleet.finished || fleet.phase === "finished") return "Branch to review";
    return "Open";
  }

  function fleetGlanceLine(fleet) {
    if (fleet.phase.startsWith("planning")) {
      return fleet.planningProgress.detail || "Forge is preparing a governed lane split.";
    }
    const geometry = geometryLine(fleet.executionGeometry) || `${fleet.streams.length} ${fleet.streams.length === 1 ? "stream" : "streams"}`;
    const proof = fleet.proof?.status ? proofLine(fleet.proof) : "Proof gates appear when the fleet starts.";
    return `${geometry}. ${proof}`;
  }

  // Re-plan: your feedback goes back to the planner, a fresh split returns.
  let feedback = $state({});
  async function replan(fleet) {
    const note = (feedback[fleet.fleetId] ?? "").trim();
    if (busy || !note) return;
    busy = fleet.fleetId + "replan";
    const cloudEnvironmentId = fleet.fleetId === focusFleetId ? preferredCloudEnvironmentId : "";
    try {
      const packet = await client.write(
        "/forge/fleet-plans.json",
        {
          spec: fleet.spec,
          fleet_id: fleet.fleetId,
          repo_id: fleet.repoId,
          requested_width: fleet.executionGeometry.requestedWidth,
          feedback: note,
          actor_id: actor,
          cloud_environment_id: cloudEnvironmentId
        },
        { receiptIntent: "fleet_plan" }
      );
      if (packet.status === "DRAFTED") {
        feedback = { ...feedback, [fleet.fleetId]: "" };
        flash(fleet.fleetId, true, "Re-planned with your feedback.");
        await invalidateAll();
      } else {
        flash(fleet.fleetId, false, packet.reason ?? "That did not re-plan.");
      }
    } catch (error) {
      flash(fleet.fleetId, false, "Nothing changed - that did not land.");
    }
    busy = "";
  }

  // The yes: ratifying mints the streams as work items; build spend can start.
  let ratifyReason = $state({});
  async function ratify(fleet) {
    const reason = (ratifyReason[fleet.fleetId] ?? "").trim();
    if (busy || !reason) return;
    busy = fleet.fleetId + "ratify";
    try {
      const packet = await client.write(
        "/forge/fleet-plan-ratifications.json",
        { fleet_id: fleet.fleetId, reason, actor_id: actor },
        { receiptIntent: "fleet_ratify" }
      );
      if (packet.status === "RATIFIED") {
        flash(fleet.fleetId, true, "Ratified. Start the build when ready.");
        await invalidateAll();
      } else {
        flash(fleet.fleetId, false, packet.reason ?? "That did not ratify.");
      }
    } catch (error) {
      flash(fleet.fleetId, false, "Nothing recorded - that did not land.");
    }
    busy = "";
  }

  let runRepo = $state({});
  let initializedHandoff = $state(false);
  $effect(() => {
    if (initializedHandoff || !focusFleetId) return;
    openId = focusFleetId;
    if (preferredRepoId) {
      runRepo = { ...runRepo, [focusFleetId]: preferredRepoId };
    }
    initializedHandoff = true;
  });
  $effect(() => {
    const next = { ...runRepo };
    let changed = false;
    for (const fleet of fleets) {
      if (!next[fleet.fleetId] && fleet.repoId) {
        next[fleet.fleetId] = fleet.repoId;
        changed = true;
      }
    }
    if (changed) runRepo = next;
  });
  async function startFleet(fleet) {
    if (busy) return;
    busy = fleet.fleetId + "start";
    try {
      const packet = await client.write(
        "/forge/fleet-runs.json",
        {
          fleet_id: fleet.fleetId,
          repo_id: runRepo[fleet.fleetId] ?? "",
          cloud_environment_id:
            fleet.fleetId === focusFleetId ? preferredCloudEnvironmentId : "",
          actor_id: actor
        },
        { receiptIntent: "fleet_run" }
      );
      if (packet.status === "FLEET_STARTED") {
        flash(fleet.fleetId, true, "The fleet is building. Watch the lanes.");
        await invalidateAll();
      } else {
        flash(fleet.fleetId, false, packet.reason ?? "That did not start.");
      }
    } catch (error) {
      flash(fleet.fleetId, false, "Nothing started - that did not land.");
    }
    busy = "";
  }

  // Recovery into a mission: when a fleet lands only partially, carry the stuck
  // remainder into a governed mission. The operator sees exactly what will
  // happen - the pre-filled goal, write scope, and checks - before anything is
  // created. Create composes the existing mission route; nothing new on the
  // backend, and the fleet's own receipts stay untouched.
  let recoveryOpenId = $state("");
  let recoveryDraft = $state(null);
  let recoveryBusy = $state(false);
  function prepareRecovery(fleet) {
    recoveryDraft = prepareMissionFromFleet(fleet);
    recoveryOpenId = fleet.fleetId;
    cardFlash = { ...cardFlash, [fleet.fleetId]: null };
  }
  function cancelRecovery() {
    recoveryOpenId = "";
    recoveryDraft = null;
  }
  async function createMissionFromFleet(fleet) {
    if (recoveryBusy || !recoveryDraft) return;
    recoveryBusy = true;
    try {
      const response = await fetch("/api/kernel/forge/long-horizon-missions.json", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(buildMissionCreatePayload(recoveryDraft, actor))
      });
      const packet = await response.json();
      const admitted = String(packet?.status ?? "").includes("ADMITTED") || (packet?.mission_id && packet?.status !== "REFUSED");
      if (admitted) {
        recoveryOpenId = "";
        recoveryDraft = null;
        await goto("/forge/missions");
      } else {
        flash(fleet.fleetId, false, packet?.blocked_reason || packet?.reason || "That did not create a mission.");
      }
    } catch (error) {
      flash(fleet.fleetId, false, "Nothing created - that did not land.");
    }
    recoveryBusy = false;
  }

  // While any fleet builds, the lanes refresh themselves.
  onMount(() => {
    return startLivePoll({
      isActive: () => anyPlanning || anyLive || sawLive,
      // Scoped to the fleet projections so a live tick does not re-run the
      // shell layout waterfall every few seconds.
      refresh: () => invalidate("forge:fleets")
    });
  });
</script>

<svelte:head><title>Fleets - MDx</title></svelte:head>

<ForgeView
  title="Fleets"
  backTo={{ href: "/forge/runs", label: "Trails" }}
  subtitle="Parallel Forge work - plan, lanes, and reviewed branch."
>

  <ForgeTrailMap
    scale="Fleet"
    active={trailStage}
    title="Parallel lanes, one integrated branch"
    detail="Each lane keeps its scope and proof."
  />

  {#if capacity && anyBuilding}
    <p class="capacity-strip">
      <span class="cap-dot" aria-hidden="true"></span>
      {capacity.active} of {capacity.capacity} workers active{#if capacity.queued} &middot; {capacity.queued} queued{/if}. Forge reserves workers, queues the rest, and refuses overflow - it never overruns capacity.
    </p>
  {/if}

  {#if fleets.length === 0}
    <div class="forge-empty">
      <h2>No fleets yet</h2>
      <p>Describe a meaty goal on the Forge home - when the work has naturally separate pieces, Forge proposes running it across a fleet of agents, and the split lands here.</p>
      <a class="mdx-btn primary" href="/forge">Describe work &rarr;</a>
    </div>
  {:else}
    <ul class="fleet-list">
      {#each fleets as fleet (fleet.fleetId)}
        <li class="fleet" data-phase={fleet.phase}>
          <button type="button" class="fleet-head" onclick={() => (openId = openId === fleet.fleetId ? "" : fleet.fleetId)}>
            <span class="fleet-dot" data-phase={fleet.phase} class:spin={fleet.running || fleet.phase === "planning" || fleet.phase === "planning_delayed"} aria-hidden="true"></span>
            <span class="fleet-main">
              <span class="fleet-phase">{phaseLabel(fleet.phase)}</span>
              <span class="fleet-spec">{fleet.spec}</span>
              <span class="fleet-glance">{fleetGlanceLine(fleet)}</span>
              {#if geometryMixLine(fleet.executionGeometry)}
                <span class="fleet-mix">{geometryMixLine(fleet.executionGeometry)}</span>
              {/if}
            </span>
            <span class="fleet-side">
              <span class="fleet-count">
                {#if fleet.phase.startsWith("planning")}
                  {fleet.executionGeometry.requestedWidth} requested
                {:else}
                  {fleet.streams.length} {fleet.streams.length === 1 ? "stream" : "streams"}
                {/if}
              </span>
              <span class="fleet-next">{fleetNextLabel(fleet)}</span>
            </span>
          </button>

          {#if openId === fleet.fleetId}
            <div class="fleet-body">
              {#if fleet.justification}
                <p class="why">Why this split: {fleet.justification}</p>
              {/if}

              {#if fleet.brief.coders.length}
                <div class="brief">
                  <span class="brief-label">Cast to</span>
                  {#each fleet.brief.coders as c (c.coder)}
                    <span class="brief-coder">{c.count}&times; {c.coder}</span>
                  {/each}
                  {#if fleet.brief.sensitive > 0}
                    <span class="brief-sensitive" title="Sensitive streams use a sovereign model or do not run.">
                      {fleet.brief.sensitive} sensitive &middot; sovereign-only
                    </span>
                    <span class="brief-panel" title="A high-stakes fleet is read by a panel of diverse models, not one.">diverse review</span>
                  {/if}
                </div>
              {/if}

              {#if fleet.executionGeometry?.streamCount}
                <div class="geometry-card">
                  <p class="geometry-title">Execution geometry</p>
                  <p class="geometry-line">{geometryLine(fleet.executionGeometry)}</p>
                  <div class="geometry-grid">
                    <span data-ok={fleet.executionGeometry.disjointWriteScopesValidated}>disjoint write scopes</span>
                    <span>{fleet.executionGeometry.streamCheckCount + fleet.executionGeometry.fullSuiteCheckCount} checks</span>
                    <span>{fleet.executionGeometry.integrationOwnedPathCount} integration-owned paths</span>
                    <span>{fleet.executionGeometry.maxTurnBudget} turn budget</span>
                  </div>
                  {#if fleet.executionGeometry.byCoder.length || fleet.executionGeometry.bySensitivity.length}
                    <p class="geometry-meta">
                      {geometryMixLine(fleet.executionGeometry)}
                    </p>
                  {/if}
                </div>
              {/if}

              {#if fleet.languagePackId || fleet.repoProofPlanSummary || fleet.repoSuggestedChecks.length}
                <div class="repo-intelligence">
                  <span class="repo-chip">{fleet.repoId ? fleet.repoId : "MDx"}</span>
                  {#if fleet.languagePackId}<span class="repo-chip">{fleet.languagePackId}</span>{/if}
                  {#if fleet.repoPrimaryLanguage}<span class="repo-chip">{fleet.repoPrimaryLanguage}</span>{/if}
                  {#if fleet.repoSuggestedChecks.length}
                    <span class="repo-chip">checks: {fleet.repoSuggestedChecks.join(", ")}</span>
                  {/if}
                  {#if fleet.repoProofPlanSummary}
                    <p>{fleet.repoProofPlanSummary}</p>
                  {/if}
                  {#if fleet.repoArtifactPatterns.length}
                    <p class="repo-muted">Review filters: {fleet.repoArtifactPatterns.join(", ")}</p>
                  {/if}
                </div>
              {/if}

              {#if fleet.proof?.status}
                {@const repair = repairSummary(fleet.proof)}
                <div class="proof-card" data-ready={fleet.proof.principalEngineerEvidenceReady} data-attention={repair.tone === "attention"}>
                  <p class="proof-title">Principal proof</p>
                  <p class="proof-line">{proofLine(fleet.proof)}</p>
                  {#if repair.show}
                    <p class="repair-line" data-tone={repair.tone}>{repair.line}</p>
                  {/if}
                  <div class="proof-grid">
                    <span data-ok={fleet.proof.repoIntelligenceRecorded}>repo intelligence</span>
                    <span data-ok={fleet.proof.disjointStreamsValidated}>disjoint streams</span>
                    <span data-ok={fleet.proof.proofCommandsDeclared}>checks</span>
                    <span data-ok={fleet.proof.artifactFiltersRecorded}>artifact filters</span>
                    <span data-ok={fleet.proof.semanticOrientationDeclared}>semantic orientation</span>
                    <span data-ok={fleet.proof.integrationReviewed}>integration review</span>
                    {#if fleet.proof.revisionRepaired || fleet.proof.laneRevisionRepaired}
                      <span data-ok="true">repair verified</span>
                    {/if}
                  </div>
                  <p class="proof-meta">
                    {fleet.proof.laneDoneCount}/{fleet.proof.streamCount} streams done
                    {#if fleet.proof.laneAttentionCount} - {fleet.proof.laneAttentionCount} needs attention{/if}
                    - {fleet.proof.declaredCheckCount} checks - {fleet.proof.semanticSignalCount} semantic signals
                  </p>
                  {#if fleet.proof.missingGates.length}
                    <p class="proof-missing">Waiting on {fleet.proof.missingGates.join(", ")}</p>
                  {/if}
                </div>
              {/if}

              {#if fleetResultById[fleet.fleetId] && fleetResultById[fleet.fleetId].status !== "not_started"}
                {@const result = fleetResultById[fleet.fleetId]}
                <div class="fleet-result" data-status={result.status}>
                  <div class="fr-head">
                    <span class="fr-tag">Fleet result</span>
                    <span class="fr-status">{RESULT_STATUS[result.status] ?? result.status}</span>
                  </div>
                  <p class="fr-summary">{result.summary}</p>
                  <p class="fr-cost">{result.turns} turns &middot; {tok(Number(result.tokens_in) + Number(result.tokens_out))} tokens{#if result.integration_state} &middot; integration {result.integration_state}{/if}</p>
                </div>
              {/if}

              <div class="lanes">
                {#each fleet.lanes as lane (lane.streamId)}
                  {@const stream = fleet.streams.find((s) => s.streamId === lane.streamId)}
                  <div class="lane" data-state={lane.state}>
                    <div class="lane-head">
                      <span class="lane-dot" data-state={lane.state} class:spin={lane.state === "working"} aria-hidden="true"></span>
                      <span class="lane-name">{lane.streamId}</span>
                      <span class="lane-state">{laneLabel(lane.state)}</span>
                      {#if lane.forgeRunId}
                        <a class="lane-link" href="/forge/runs">trail &rarr;</a>
                      {/if}
                    </div>
                    {#if stream}
                      <p class="lane-objective">{stream.objective}</p>
                      <p class="lane-cast">
                        <span class="chip">{coderLabel(stream.builderSlot)}</span>
                        {#if String(stream.dataSensitivity).toLowerCase() === "sensitive"}
                          <span class="chip sensitive" title="Sensitive data - runs only on a sovereign model, or not at all.">sensitive</span>
                        {/if}
                      </p>
                      <p class="lane-meta">
                        owns {stream.writeScope.join(", ")}{#if stream.dependsOn.length} &middot; after {stream.dependsOn.join(", ")}{/if}
                      </p>
                      {#if stream.contract}<p class="lane-contract">{stream.contract}</p>{/if}
                    {/if}
                    {#if laneResultByKey[`${fleet.fleetId} ${lane.streamId}`]}
                      {@const lr = laneResultByKey[`${fleet.fleetId} ${lane.streamId}`]}
                      {#if Number(lr.checks_passed) + Number(lr.checks_failed) + Number(lr.turns) > 0}
                        <p class="lane-result">
                          {lr.checks_passed} checks passed{#if Number(lr.checks_failed) > 0}<span class="lr-fail">, {lr.checks_failed} failed</span>{/if}
                          &middot; {lr.turns} turns &middot; {tok(Number(lr.tokens_in) + Number(lr.tokens_out))} tok
                        </p>
                      {/if}
                    {/if}
                    {#if lane.state === "needs_attention" && lane.detail}
                      <p class="lane-flag">{lane.detail}</p>
                    {/if}
                  </div>
                {/each}

                {#if fleet.integrationState}
                  <div class="lane integration" data-state={fleet.integrationState === "done" ? "done" : fleet.integrationState === "working" ? "working" : "needs_attention"}>
                    <div class="lane-head">
                      <span class="lane-dot" data-state={fleet.integrationState === "done" ? "done" : "working"} aria-hidden="true"></span>
                      <span class="lane-name">Bringing it together</span>
                      <span class="lane-state">{fleet.integrationState === "done" ? "Done" : fleet.integrationState === "working" ? "Working" : "Did not land"}</span>
                    </div>
                    <p class="lane-meta">{fleet.integrationDetail}</p>
                  </div>
                {/if}
              </div>

              {#if fleetNeedsRecovery(fleet)}
                <div class="fleet-recovery" data-integration={fleetHasIntegrationFailure(fleet)}>
                  <p class="frc-title">
                    {#if fleetHasIntegrationFailure(fleet)}
                      The branches did not come together
                    {:else}
                      Some lanes stopped short
                    {/if}
                  </p>
                  <p class="frc-line">
                    {#if fleetHasIntegrationFailure(fleet)}
                      The finished lanes are safe, but integration could not produce one branch. Carry the remainder into a mission and finish it there, on a budget and with checks after each step.
                    {:else}
                      The lanes that landed are done. Carry the ones that stopped into a mission - it inherits their write scope and checks, and Forge works them milestone by milestone.
                    {/if}
                  </p>
                  {#if recoveryOpenId === fleet.fleetId && recoveryDraft}
                    <div class="frc-preview">
                      <p class="frc-preview-head">Here is the mission this creates</p>
                      <dl>
                        <dt>Goal</dt><dd>{recoveryDraft.goal}</dd>
                        <dt>Carries</dt><dd>{recoveryDraft.remainderLaneIds.length} {recoveryDraft.remainderLaneIds.length === 1 ? "lane" : "lanes"}{#if recoveryDraft.allowedWriteScope.length}, writing to {recoveryDraft.allowedWriteScope.join(", ")}{/if}</dd>
                        {#if recoveryDraft.validationCommands.length}<dt>Checks</dt><dd>{recoveryDraft.validationCommands.join(", ")}</dd>{/if}
                        <dt>Budget</dt><dd>up to {recoveryDraft.fleetWidth} {recoveryDraft.fleetWidth === 1 ? "worker" : "workers"}, checks after each milestone</dd>
                      </dl>
                      <p class="frc-preview-note">You can refine the goal, write scope, and checks in Missions before you launch. Nothing runs a worker or writes to your repo yet.</p>
                      <div class="frc-actions">
                        <button type="button" class="mdx-btn primary small" disabled={recoveryBusy} onclick={() => createMissionFromFleet(fleet)}>
                          {recoveryBusy ? "Creating..." : "Create the mission"}
                        </button>
                        <button type="button" class="mdx-btn small ghost" disabled={recoveryBusy} onclick={cancelRecovery}>Cancel</button>
                      </div>
                    </div>
                  {:else}
                    <div class="frc-actions">
                      <button type="button" class="mdx-btn small" onclick={() => prepareRecovery(fleet)}>
                        {fleetHasIntegrationFailure(fleet) ? "Repair in a mission" : "Move to a mission"}
                      </button>
                    </div>
                  {/if}
                </div>
              {/if}

              {#if fleet.reviewVerdict}
                {@const review = reviewSummary(fleet.reviewVerdict)}
                <div class="review" class:ready={review.ready}>
                  <p class="review-line">
                    {#if review.panel}
                      A panel of independent reviewers read the whole change:
                    {:else}
                      A fresh set of eyes read the whole change:
                    {/if}
                    <strong>{review.line}</strong>
                  </p>
                  {#if review.panel}
                    {#if review.members.length}
                      <div class="panel-members">
                        {#each review.members as member (member.model)}
                          <span class="panel-member" data-verdict={member.verdict}>
                            <span class="pm-dot" data-verdict={member.verdict} aria-hidden="true"></span>
                            {member.model}: {member.verdict}
                          </span>
                        {/each}
                      </div>
                    {/if}
                    <p class="review-note">Different models, different blind spots - the merge holds for you if any one of them flags a problem.</p>
                  {/if}
                  {#if review.findings.length}
                    <ul class="review-findings">
                      {#each review.findings as finding (finding)}<li>{finding}</li>{/each}
                    </ul>
                  {/if}
                </div>
              {/if}

              {#if fleet.finished && fleet.fleetBranch}
                <p class="next-line">
                  The fleet's work is on <code>{fleet.fleetBranch}</code>. Review and ship it from
                  <a href="/forge/review">Review</a> - shipping stays your recorded call.
                </p>
              {/if}

              {#if fleet.phase === "proposed"}
                {@const review = planReviewLine(fleet.planReview)}
                {#if review.show}
                  <div class="plan-review" data-tone={review.tone}>
                    <span class="pr-tag">Plan review</span>
                    <p class="pr-line">{review.line}</p>
                    {#if review.concerns}<p class="pr-concerns">{review.concerns}</p>{/if}
                  </div>
                {/if}
                <div class="verbs">
                  <div class="verb">
                    <p class="verb-line">Not the right split? Say what to change and the planner tries again.</p>
                    <div class="verb-row">
                      <input type="text" bind:value={feedback[fleet.fleetId]} placeholder="e.g. merge the two backend streams" aria-label="Re-plan feedback" />
                      <button type="button" class="mdx-btn small" disabled={busy === fleet.fleetId + "replan" || !(feedback[fleet.fleetId] ?? "").trim()} onclick={() => replan(fleet)}>
                        {busy === fleet.fleetId + "replan" ? "Re-planning..." : "Re-plan"}
                      </button>
                    </div>
                  </div>
                  <div class="verb">
                    <p class="verb-line">Split looks right? Your yes goes on the record and the streams become real work.</p>
                    <div class="verb-row">
                      <input type="text" bind:value={ratifyReason[fleet.fleetId]} placeholder="Why this split is right (your words)" aria-label="Ratify reason" />
                      <button type="button" class="mdx-btn primary" disabled={busy === fleet.fleetId + "ratify" || !(ratifyReason[fleet.fleetId] ?? "").trim()} onclick={() => ratify(fleet)}>
                        {busy === fleet.fleetId + "ratify" ? "Recording..." : "Approve the split"}
                      </button>
                    </div>
                  </div>
                </div>
              {/if}

              {#if fleet.phase === "ready"}
                <div class="verb">
                  <p class="verb-line">Ratified. Start the build - every stream works at once.</p>
                  <div class="verb-row">
                    {#if repos.length}
                      <select bind:value={runRepo[fleet.fleetId]} aria-label="Which repo">
                        <option value="">MDx (this app)</option>
                        {#each repos as repo (repo.repoId)}<option value={repo.repoId}>{repo.label}</option>{/each}
                      </select>
                    {/if}
                    <button type="button" class="mdx-btn primary" disabled={busy === fleet.fleetId + "start"} onclick={() => startFleet(fleet)}>
                      {busy === fleet.fleetId + "start" ? "Starting..." : "Start the fleet"}
                    </button>
                  </div>
                </div>
              {/if}

              {#if cardFlash[fleet.fleetId]}<p class="flash" class:bad={!cardFlash[fleet.fleetId].ok}>{cardFlash[fleet.fleetId].line}</p>{/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  <p class="boundary">{data.boundary}.</p>
</ForgeView>

<style>
  .flash { margin: 0; font-size: 12.5px; color: var(--mdx-accent-primary); }
  .flash.bad { color: var(--mdx-tone-danger-text); }

  .forge-empty { display: grid; justify-items: center; text-align: center; gap: 12px; max-width: 460px; margin: 44px auto; }
  .forge-empty h2 { margin: 0; font-size: 18px; font-weight: 650; }
  .forge-empty p { margin: 0; color: var(--mdx-text-muted); font-size: 13.5px; line-height: 1.5; }

  .fleet-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 10px; }
  .fleet { border-radius: var(--mdx-radius-lg, 12px); background: var(--mdx-surface-raised); overflow: hidden; border: 1px solid var(--mdx-border-subtle); box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card); }
  .fleet-head { display: flex; align-items: center; gap: 12px; width: 100%; padding: 13px 16px; border: none; background: transparent; color: inherit; cursor: pointer; text-align: left; min-width: 0; }
  .fleet-dot { width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; background: var(--mdx-text-muted); }
  .fleet-dot[data-phase="building"] { background: var(--mdx-accent-primary); }
  .fleet-dot[data-phase="planning"],
  .fleet-dot[data-phase="planning_delayed"] { background: var(--mdx-accent-primary); }
  .fleet-dot[data-phase="planning_needs_attention"] { background: var(--mdx-tone-danger-text); }
  .fleet-dot[data-phase="finished"] { background: var(--mdx-accent-success); }
  .fleet-dot[data-phase="ready"] { background: var(--mdx-accent-warning); }
  .spin { animation: pulse 1.4s ease-in-out infinite; }
  @keyframes pulse { 50% { opacity: 0.35; } }
  .fleet-main { display: grid; gap: 2px; flex: 1; min-width: 0; }
  .fleet-phase { font-family: var(--mdx-font-display); font-size: 13.5px; font-weight: 650; }
  .fleet-spec { font-size: 12.5px; color: var(--mdx-text-secondary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .fleet-glance,
  .fleet-mix {
    font-size: 12px;
    color: var(--mdx-text-muted);
    line-height: 1.45;
    display: -webkit-box;
    -webkit-line-clamp: 1;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .fleet-mix {
    font-family: var(--mdx-font-mono, monospace);
    font-size: 11.5px;
  }
  .fleet-side {
    flex: none;
    display: grid;
    justify-items: end;
    gap: 5px;
  }
  .fleet-count { font-size: 12px; color: var(--mdx-text-muted); white-space: nowrap; }
  .fleet-next {
    min-height: 24px;
    display: inline-flex;
    align-items: center;
    padding: 0 9px;
    border-radius: 999px;
    border: 1px solid var(--mdx-border-subtle);
    background: var(--mdx-surface-sunken, transparent);
    color: var(--mdx-text-muted);
    font-size: 11.5px;
    font-weight: 550;
    white-space: nowrap;
  }

  .fleet-body { display: grid; gap: 12px; padding: 4px 16px 16px; }
  .why { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); }
  .lanes { display: grid; gap: 8px; }
  .lane { display: grid; gap: 4px; padding: 10px 12px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-text-muted) 6%, transparent); }
  .lane.integration { border: 1px dashed color-mix(in srgb, var(--mdx-text-muted) 30%, transparent); background: transparent; }
  .lane-head { display: flex; align-items: center; gap: 8px; }
  .lane-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--mdx-text-muted); }
  .lane-dot[data-state="working"] { background: var(--mdx-accent-primary); }
  .lane-dot[data-state="done"] { background: var(--mdx-accent-success); }
  .lane-dot[data-state="needs_attention"] { background: var(--mdx-accent-error); }
  .lane-name { font-family: var(--mdx-font-display); font-size: 13px; font-weight: 650; }
  .lane-state { font-size: 12px; color: var(--mdx-text-muted); }
  .lane-link { margin-left: auto; font-size: 12px; }
  .lane-objective { margin: 0; font-size: 13px; }
  .lane-meta { margin: 0; font-size: var(--mdx-text-xs); color: var(--mdx-text-muted); }
  .lane-contract { margin: 0; font-size: var(--mdx-text-xs); color: var(--mdx-text-muted); font-family: var(--mdx-font-mono, monospace); }

  .review { padding: 10px 12px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-primary) 7%, transparent); }
  .review.ready { background: color-mix(in srgb, var(--mdx-accent-success) 9%, transparent); }
  .review-line { margin: 0; font-size: 13px; }
  .panel-members { display: flex; flex-wrap: wrap; gap: 7px; margin-top: 6px; }
  .panel-member { display: inline-flex; align-items: center; gap: 5px; font-size: 11.5px; font-family: var(--mdx-font-mono, monospace); padding: 2px 8px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-text-muted) 10%, transparent); }
  .pm-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--mdx-accent-success); }
  .pm-dot[data-verdict="needs work"] { background: var(--mdx-accent-error); }
  .pm-dot[data-verdict="unavailable"] { background: var(--mdx-text-muted); }
  .review-note { margin: 4px 0 0; font-size: 12px; color: var(--mdx-text-muted); }
  .review-findings { margin: 6px 0 0; padding-left: 18px; font-size: 12.5px; color: var(--mdx-text-muted); }
  .next-line { margin: 0; font-size: 13px; }

  /* The casting, made legible: how the harness split the work across coders. */
  .brief { display: flex; flex-wrap: wrap; align-items: center; gap: 7px; font-size: 12px; }
  .brief-label { color: var(--mdx-text-muted); font-weight: 600; }
  .brief-coder { padding: 2px 8px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-text-muted) 12%, transparent); }
  .brief-sensitive { padding: 2px 8px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-accent-warning) 18%, transparent); color: var(--mdx-tone-warning-text, inherit); cursor: help; }
  .brief-panel { padding: 2px 8px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-accent-primary) 14%, transparent); cursor: help; }

  .geometry-card {
    display: grid;
    gap: 8px;
    padding: 11px 12px;
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-primary) 6%, transparent);
    border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 14%, transparent);
  }
  .geometry-title {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 12.5px;
    font-weight: 700;
  }
  .geometry-line,
  .geometry-meta {
    margin: 0;
    font-size: 12.5px;
    color: var(--mdx-text-muted);
  }
  .geometry-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .geometry-grid span {
    min-height: 22px;
    display: inline-flex;
    align-items: center;
    padding: 0 8px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--mdx-text-muted) 10%, transparent);
    font-size: 12px;
  }
  .geometry-grid span[data-ok="true"] {
    color: var(--mdx-accent-success);
    background: color-mix(in srgb, var(--mdx-accent-success) 12%, transparent);
  }

  .repo-intelligence {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
    padding: 10px 12px;
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-primary) 6%, transparent);
    border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 14%, transparent);
  }
  .repo-intelligence p {
    flex-basis: 100%;
    margin: 0;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--mdx-text-muted);
  }
  .repo-intelligence .repo-muted { color: var(--mdx-text-muted); opacity: 0.8; }
  .repo-chip {
    display: inline-flex;
    align-items: center;
    min-height: 22px;
    padding: 0 8px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--mdx-text-muted) 10%, transparent);
    color: var(--mdx-text-secondary);
    font-size: 12px;
    line-height: 1;
    white-space: nowrap;
  }

  .proof-card {
    display: grid;
    gap: 8px;
    padding: 11px 12px;
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-text-muted) 6%, transparent);
    border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 14%, transparent);
  }
  .proof-card[data-ready="true"] {
    background: color-mix(in srgb, var(--mdx-accent-success) 8%, transparent);
    border-color: color-mix(in srgb, var(--mdx-accent-success) 22%, transparent);
  }
  .proof-title {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 12.5px;
    font-weight: 700;
  }
  .proof-line,
  .proof-meta,
  .proof-missing {
    margin: 0;
    font-size: 12.5px;
    color: var(--mdx-text-muted);
    line-height: 1.45;
  }
  .proof-grid { display: flex; flex-wrap: wrap; gap: 6px; }
  .proof-grid span {
    display: inline-flex;
    align-items: center;
    min-height: 22px;
    padding: 0 8px;
    border-radius: 999px;
    font-size: 11.5px;
    background: color-mix(in srgb, var(--mdx-text-muted) 10%, transparent);
    color: var(--mdx-text-muted);
  }
  .proof-grid span[data-ok="true"] {
    background: color-mix(in srgb, var(--mdx-accent-success) 14%, transparent);
    color: var(--mdx-text-primary);
  }

  .lane-cast { margin: 0; display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { font-size: 11.5px; padding: 1px 7px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-text-muted) 12%, transparent); color: var(--mdx-text-muted); }
  .chip.sensitive { background: color-mix(in srgb, var(--mdx-accent-warning) 20%, transparent); color: var(--mdx-tone-warning-text, inherit); cursor: help; }
  .lane-flag { margin: 0; font-size: var(--mdx-text-xs); color: var(--mdx-tone-danger-text, var(--mdx-accent-error)); }

  .verbs { display: grid; gap: 10px; }
  .verb { display: grid; gap: 6px; }
  .verb-line { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); }
  .verb-row { display: flex; gap: 8px; }
  .verb-row input, .verb-row select { flex: 1; min-width: 0; padding: 8px 10px; font-size: 13px; border-radius: var(--mdx-radius-md); border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 25%, transparent); background: var(--mdx-surface-base, transparent); color: inherit; }
  .verb-row select { flex: 0 1 220px; }

  /* Capacity pulse - the floor is bounded; shown only while building. */
  .capacity-strip { display: flex; align-items: center; gap: 8px; margin: 0; padding: 9px 12px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-text-muted) 7%, transparent); font-size: 12.5px; color: var(--mdx-text-muted); }
  .cap-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; background: var(--mdx-accent-primary); animation: pulse 1.4s ease-in-out infinite; }

  /* Plan review - the reviewer's pre-spend read, above the approve verbs so
     ratification is an informed gate. Tone follows the verdict. */
  .plan-review { display: grid; gap: 4px; padding: 11px 12px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-text-muted) 6%, transparent); border-left: 3px solid var(--mdx-text-muted); }
  .plan-review[data-tone="ok"] { border-left-color: var(--mdx-accent-success); background: color-mix(in srgb, var(--mdx-accent-success) 7%, transparent); }
  .plan-review[data-tone="warn"] { border-left-color: var(--mdx-accent-warning); background: color-mix(in srgb, var(--mdx-accent-warning) 9%, transparent); }
  .pr-tag { font-size: 11px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.03em; color: var(--mdx-text-muted); }
  .pr-line { margin: 0; font-size: 13px; }
  .pr-concerns { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.45; }

  /* The budgeted self-heal line; "attention" is the loud escalation case. */
  .repair-line { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.45; }
  .repair-line[data-tone="ok"] { color: var(--mdx-accent-success); }
  .repair-line[data-tone="warn"] { color: var(--mdx-tone-warn-text, var(--mdx-accent-warning)); }
  .repair-line[data-tone="attention"] { color: var(--mdx-tone-danger-text, var(--mdx-accent-error)); font-weight: 600; }
  .proof-card[data-attention="true"] { background: color-mix(in srgb, var(--mdx-accent-error) 8%, transparent); border-color: color-mix(in srgb, var(--mdx-accent-error) 26%, transparent); }

  /* Fleet result - the rollup read of what the fleet produced: lanes clean,
     checks, repairs, cost. The factory-floor number, in sober terms. */
  .fleet-result { display: grid; gap: 5px; padding: 11px 12px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-text-muted) 6%, transparent); border: 1px solid var(--mdx-border-subtle); }
  .fleet-result[data-status="clean"] { background: color-mix(in srgb, var(--mdx-accent-success) 9%, transparent); border-color: color-mix(in srgb, var(--mdx-accent-success) 24%, transparent); }
  .fleet-result[data-status="needs_you"] { background: color-mix(in srgb, var(--mdx-accent-error) 8%, transparent); border-color: color-mix(in srgb, var(--mdx-accent-error) 24%, transparent); }
  .fr-head { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }
  .fr-tag { font-size: 11px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.03em; color: var(--mdx-text-muted); }
  .fr-status { font-size: 12px; font-weight: 600; }
  .fleet-result[data-status="clean"] .fr-status { color: var(--mdx-accent-success); }
  .fleet-result[data-status="needs_you"] .fr-status { color: var(--mdx-tone-danger-text, var(--mdx-accent-error)); }
  .fr-summary { margin: 0; font-size: 13px; }
  .fr-cost { margin: 0; font-size: 12px; color: var(--mdx-text-muted); }
  .lane-result { margin: 0; font-size: var(--mdx-text-xs); color: var(--mdx-text-muted); }
  .lr-fail { color: var(--mdx-tone-danger-text, var(--mdx-accent-error)); }

  /* Fleet recovery into a mission: a calm verb that carries the stuck
     remainder forward, showing the pre-filled mission before it is created. */
  .fleet-recovery { display: grid; gap: 8px; padding: 12px 14px; border-radius: var(--mdx-radius-md); border: 1px solid color-mix(in srgb, var(--mdx-accent-warning, #d9883a) 34%, transparent); border-left: 3px solid var(--mdx-accent-warning, #d9883a); background: color-mix(in srgb, var(--mdx-accent-warning, #d9883a) 8%, transparent); }
  .frc-title { margin: 0; font-size: 13px; font-weight: 650; }
  .frc-line { margin: 0; font-size: 12.5px; line-height: 1.5; color: var(--mdx-text-secondary); }
  .frc-actions { display: flex; flex-wrap: wrap; gap: 8px; }
  .frc-preview { display: grid; gap: 8px; padding: 11px 12px; border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); border: 1px solid var(--mdx-border-subtle); }
  .frc-preview-head { margin: 0; font-size: 11px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.03em; color: var(--mdx-text-muted); }
  .frc-preview dl { display: grid; grid-template-columns: auto 1fr; gap: 4px 12px; margin: 0; }
  .frc-preview dt { font-weight: 650; color: var(--mdx-text-muted); font-size: 12px; }
  .frc-preview dd { margin: 0; font-size: 12.5px; word-break: break-word; }
  .frc-preview-note { margin: 0; font-size: 12px; color: var(--mdx-text-muted); line-height: 1.45; }

  .boundary { margin: 4px 0 0; font-size: 12px; color: var(--mdx-text-muted); }
  @media (max-width: 640px) {
    .fleet-head {
      align-items: flex-start;
    }
    .fleet-side {
      justify-items: start;
    }
    .fleet-head {
      flex-wrap: wrap;
    }
    .fleet-side {
      width: 100%;
      padding-left: 22px;
    }
  }
</style>
