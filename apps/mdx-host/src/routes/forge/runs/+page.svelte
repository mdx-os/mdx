<script>
  // Forge runs: watch the build agent work. Start a run by describing the
  // change and naming the checks that prove it; the agent reads, edits,
  // runs those checks, and either lands a branch or stops honestly. Each
  // run opens into its turn-by-turn activity - the agent's own trail,
  // translated from receipts into plain language, with the raw grammar a
  // disclosure away. While a run works, the view refreshes itself.
  import { createMdxClient } from "@mdx/client";
  import { invalidateAll, invalidate, goto } from "$app/navigation";
  import { startLivePoll } from "../../../lib/livePoll.js";
  import RepoConnect from "../../../lib/RepoConnect.svelte";
  import { enableNotifications, notificationState, notifyRunTransitions, notificationsSupported } from "../../../lib/runNotifications.js";
  import { onMount } from "svelte";
  import ForgeView from "../../../lib/ForgeView.svelte";
  import ForgeTrailMap from "../../../lib/ForgeTrailMap.svelte";
  import RunViewer from "../../../lib/RunViewer.svelte";
  import {
    normalizeRuns,
    summaryLine,
    runHeadline,
    hasHumanTitle,
    statusLabel,
    isRunning,
    normalizeDiff,
    normalizeRepos,
    operatorStatusLabel,
    controlAllowed,
    isLeagueRun,
    runnerHeader,
    quarantineLine,
    trialExecuted,
    trialAccepted,
    contextFill,
    runRecovery,
    needsRecovery,
    normalizeParallelGroups,
    runListRepresentatives,
    isWideGroup,
    groupCandidates,
    groupSummaryLine
  } from "../../../lib/forgeRuns.js";

  let { data, form } = $props();

  // Runs the operator set aside to quiet the list. This is a view preference
  // only - the run and its receipts stay on the ledger untouched. Set-aside is
  // never a way to hide stuck work: a run that needs a person cannot be set
  // aside, and whatever is set aside stays one visible tap from coming back.
  const HIDDEN_RUNS_KEY = "forge.hiddenRuns";
  let hiddenRunIds = $state([]);
  let showSetAside = $state(false);
  function persistSetAside() {
    try {
      localStorage.setItem(HIDDEN_RUNS_KEY, JSON.stringify(hiddenRunIds));
    } catch (error) {
      // best-effort view preference
    }
  }
  function setAsideRun(run) {
    // Stuck or still-working runs are exactly what must not disappear.
    if (isRunning(run.status) || needsRecovery(run)) return;
    hiddenRunIds = [...hiddenRunIds, run.runId];
    if (openId === run.runId) openRun(run.runId);
    persistSetAside();
  }
  function bringBackRun(runId) {
    hiddenRunIds = hiddenRunIds.filter((id) => id !== runId);
    persistSetAside();
  }
  const allRuns = $derived(normalizeRuns(data.runs));
  const setAsideRuns = $derived(allRuns.filter((run) => hiddenRunIds.includes(run.runId)));
  // Notify on running -> terminal transitions while the tab is hidden.
  let notifyState = $state("unsupported");
  let prevRunStates = new Map();
  $effect(() => {
    notifyRunTransitions(prevRunStates, allRuns, { isRunning: (s) => isRunning(s) });
    prevRunStates = new Map(allRuns.map((run) => [run.runId, { status: run.status }]));
  });
  async function turnOnNotifications() {
    notifyState = await enableNotifications();
  }
  const runs = $derived(allRuns.filter((run) => !hiddenRunIds.includes(run.runId)));
  const anyRunning = $derived(runs.some((run) => isRunning(run.status)));
  // A restart can make the fail-soft loader return no run projection before
  // the kernel has finished restoring it. Keep polling that honest recovery
  // state even though there is not yet a visible running row to drive the
  // normal live poll.
  const reconnectingRunHistory = $derived(data.runs == null);
  const trailStage = $derived(
    runs.some((run) => run.operatorStatus === "ready_for_review" || (["done", "finished"].includes(run.status) && run.branch))
      ? "review"
      : anyRunning
        ? "build"
        : runs.some((run) => run.checksPassed > 0 || run.checksFailed > 0)
          ? "prove"
          : "plan"
  );
  const repos = $derived(normalizeRepos(data.repos));

  // Wide parallel builds keep one row per candidate in the projection. Collapse
  // each group to one build-level row so a width-N fleet does not flood the
  // list, but keep every candidate reachable: a search matches any of them, and
  // a deep link (or the run you just started) promotes that candidate to lead
  // its group.
  const groups = $derived(normalizeParallelGroups(data.runs));
  const wideGroups = $derived(groups.filter(isWideGroup));
  function groupFor(run) {
    return wideGroups.find((g) => g.primaryRunId === run.runId || g.candidateIds.includes(run.runId));
  }
  let runSearch = $state("");
  function runMatchesSearch(run, q) {
    return `${runHeadline(run)} ${run.runSummary ?? ""} ${run.branch ?? ""} ${run.runId} ${run.workItemId ?? ""}`
      .toLowerCase()
      .includes(q);
  }

  const client = $derived(createMdxClient({ baseUrl: "/api/kernel", session: data.session }));
  const actor = $derived(data.session?.user_id ?? "local_user");
  const revisionAllowedCommands = ["cargo build -p mdx-core"];

  let openId = $state("");
  // The open run's title rides the tab title so multi-tab work is legible.
  const openRunTitle = $derived(String(allRuns.find((r) => r.runId === openId)?.title || ""));
  // Opening a run writes the URL, so every open run is a shareable deep link
  // and refresh lands back on the same run.
  function openRun(runId) {
    openId = openId === runId ? "" : runId;
    try {
      const url = new URL(window.location.href);
      if (openId) {
        url.searchParams.set("run", openId);
      } else {
        url.searchParams.delete("run");
      }
      url.searchParams.delete("run_id");
      history.replaceState(history.state, "", url);
    } catch (error) {
      // URL write is best-effort; the run is open either way
    }
  }
  // When you arrive from "Start a run now", the run id rides in the URL. We open
  // that run, scroll to it, and mark it - so you land on the run you just
  // started instead of hunting it in the list.
  let justStartedId = $state("");

  // The list rows: one per build. A wide group collapses to a single
  // representative (the open or just-started candidate leads it when the
  // operator is focused there), and search still reaches any hidden candidate.
  const representatives = $derived(
    runListRepresentatives(runs, allRuns, groups, [openId, justStartedId])
  );
  const shownRuns = $derived.by(() => {
    const q = runSearch.trim().toLowerCase();
    if (!q) return representatives;
    return representatives.filter((run) => {
      if (runMatchesSearch(run, q)) return true;
      const group = groupFor(run);
      return group ? groupCandidates(group, allRuns).some((c) => runMatchesSearch(c, q)) : false;
    });
  });

  // Watch it work. While a run is running, the open run leads with the live
  // stream - the same primitive the first mission proved (RunViewer): a stage
  // rail and a rolling "thinking and doing" feed, driven by real stream frames.
  // The stage spine and the stream route both come from the kernel's Live Forge
  // Run contract (Codex backend seam). The canonical spine below is only a
  // fallback for an older projection; the run's position on it always comes from
  // a real stage_start frame, never invented. If SSE is down, RunViewer falls
  // back to the spine baseline and the 3s trail poll still advances the history.
  const RUN_SPINE = [
    { key: "reading", label: "Reading" },
    { key: "planning", label: "Planning" },
    { key: "writing", label: "Writing" },
    { key: "proof", label: "Proving" },
    { key: "done", label: "Done" }
  ];
  function liveRun(run) {
    const stages = run.stages?.length
      ? run.stages.map((s) => ({ ...s }))
      : RUN_SPINE.map((s) => ({ ...s, state: "pending" }));
    // Carry the real check counts so RunViewer can gate its "proof passed"
    // line on actual green, not merely a terminal stage.
    return {
      status: run.status,
      branch: run.branch,
      stages,
      model: null,
      checksPassed: run.checksPassed,
      checksFailed: run.checksFailed
    };
  }
  function streamRouteFor(run) {
    return run.streamRoute || `/forge/runs/stream?run_id=${encodeURIComponent(run.runId)}`;
  }

  // Connecting a repo: an engineer sets up the repos they support, right
  // where they start the work. MDx itself is always the implicit default.
  let connectOpen = $state(false);
  let connecting = $state(false);
  let connectFlash = $state(null);
  // Steering a run that is already working: a stop ends it cleanly at the
  // next turn, a steer hands the agent a note it picks up on its next turn.
  // Both are recorded; the running loop reads them between turns.
  let steerNote = $state({});
  let controlling = $state("");
  let controlFlash = $state({});
  async function sendControl(run, control) {
    if (controlling) return;
    const note = (steerNote[run.runId] ?? "").trim();
    if (control === "steer" && !note) return;
    controlling = run.runId + control;
    controlFlash = { ...controlFlash, [run.runId]: null };
    try {
      const packet = await client.write(
        "/forge/run-controls.json",
        { run_id: run.runId, control, note, actor_id: actor },
        { receiptIntent: "forge_run_control" }
      );
      if (packet.status === "RECORDED") {
        controlFlash = {
          ...controlFlash,
          [run.runId]: { ok: true, line: control === "stop" ? "Stopping at the next turn." : "Sent. The agent picks it up next turn." }
        };
        if (control === "steer") steerNote = { ...steerNote, [run.runId]: "" };
        await invalidateAll();
      } else {
        controlFlash = { ...controlFlash, [run.runId]: { ok: false, line: packet.reason ?? "That did not land." } };
      }
    } catch (error) {
      controlFlash = { ...controlFlash, [run.runId]: { ok: false, line: "Nothing sent - that did not land." } };
    }
    controlling = "";
  }

  // While a run is working, refresh the trail. Stops itself when nothing runs.
  onMount(() => {
    try {
      const stored = JSON.parse(localStorage.getItem(HIDDEN_RUNS_KEY) ?? "[]");
      if (Array.isArray(stored)) hiddenRunIds = stored.map(String);
    } catch (error) {
      // no archive yet
    }
    notifyState = notificationState();
    const params = new URLSearchParams(window.location.search);
    // Accept both spellings: /forge/work links ?run_id=, older links ?run=.
    const target = params.get("run") ?? params.get("run_id");
    if (target) {
      openId = target;
      justStartedId = target;
      requestAnimationFrame(() => {
        document.getElementById(`run-${target}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
      });
    }
    return startLivePoll({
      isActive: () => anyRunning || reconnectingRunHistory,
      // Scoped: re-read only the runs projection, not the whole layout
      // waterfall, on each tick. SSE carries the open run's live detail.
      refresh: () => invalidate("forge:runs")
    });
  });

  // The diff a run produced, loaded on demand - you cannot responsibly ship
  // what you cannot see, so the review happens right here.
  let diffByRun = $state({});
  let diffLoading = $state("");
  async function reviewChange(run) {
    if (diffByRun[run.runId]) {
      diffByRun = { ...diffByRun, [run.runId]: null };
      return;
    }
    diffLoading = run.runId;
    try {
      // A hard timeout so a severed connection (a kernel restart mid-
      // request, say) shows an honest retry state instead of loading
      // forever - the founder hit the forever case on a live run.
      const response = await fetch("/api/kernel/forge/run-diff.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ run_id: run.runId, actor_id: actor }),
        signal: AbortSignal.timeout(15000)
      });
      const payload = await response.json();
      diffByRun = { ...diffByRun, [run.runId]: normalizeDiff(payload) };
    } catch (error) {
      diffByRun = { ...diffByRun, [run.runId]: { branch: "", fileCount: 0, files: [], error: true } };
    }
    diffLoading = "";
  }

  // The run's two lenses and the depth dial. Activity is the agent's trail at
  // the visibility you pick - Little (just the milestones), Medium (each turn),
  // Full (every step with its raw detail: the tool calls and commands). Diff
  // is the change it produced. You see as much or as little as you want.
  let lens = $state("activity");
  let depth = $state("medium");
  function eventsAtDepth(events, level) {
    if (level === "full") return events;
    if (level === "little") {
      // The status beats: started, any checks/finish markers. The outcome
      // row is synthesized from the run itself in the markup, so Little always
      // ends on how it landed even when there is no explicit finish event.
      return events.filter((event) =>
        ["run_started", "check_passed", "check_failed", "run_finished", "run_stopped", "ship_recorded"].includes(event.kind)
      );
    }
    // Medium: each translated step (a line per turn), the default read.
    return events.filter((e) => e.line);
  }
  // Load (not toggle) the diff so switching to the Diff lens always lands on
  // the change; mirrors reviewChange's fetch.
  async function loadDiff(run) {
    if (!run.branch || diffLoading === run.runId) return;
    diffLoading = run.runId;
    try {
      const response = await fetch("/api/kernel/forge/run-diff.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ run_id: run.runId, actor_id: actor }),
        signal: AbortSignal.timeout(15000)
      });
      const payload = await response.json();
      diffByRun = { ...diffByRun, [run.runId]: normalizeDiff(payload) };
    } catch (error) {
      diffByRun = { ...diffByRun, [run.runId]: { branch: "", fileCount: 0, files: [], error: true } };
    }
    diffLoading = "";
    if (openId === run.runId && lens === "diff") scrollDiffIntoView(run.runId);
  }
  function scrollDiffIntoView(runId) {
    requestAnimationFrame(() => {
      const root = document.getElementById(`run-${runId}`);
      const target = root?.querySelector(".diff-action") || root?.querySelector(".next");
      target?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
  }
  function openDiff(run) {
    lens = "diff";
    const current = diffByRun[run.runId];
    if (run.branch && (!current || current.error)) loadDiff(run);
    scrollDiffIntoView(run.runId);
  }

  function candidateGeometryLine(run) {
    const geometry = run.executionGeometry ?? {};
    const candidate = run.parallelCandidate ?? {};
    const count = Number(candidate.count || geometry.effectiveWorkers || 1);
    if (count <= 1) return "";
    const index = Number(candidate.index || 1);
    const role = candidate.role === "primary" ? "primary candidate" : "candidate";
    if (geometry.lane === "bounded_parallel_exploration") {
      return `Worker ${index} of ${count}, ${role}. Review ranks the finished branches before anyone ships.`;
    }
    return `Worker ${index} of ${count}, ${role}. The full setup is on the run if you want the detail.`;
  }

  function candidateReviewHref(run) {
    const primary = run.parallelCandidate?.primaryRunId || run.runId;
    return `/forge/review?run_id=${encodeURIComponent(primary)}`;
  }

  function runWorkbenchLine(run) {
    if (run.status === "interrupted") return "Paused after a restart. Your workspace is safe and ready to resume from the last recorded step.";
    if (isRunning(run.status)) return "Live now. Watch the stages, steer it if the direction changes, or stop cleanly.";
    if (run.status === "plan_proposed") return "Plan proposed. Nothing was written yet; approve it to start the build.";
    if (run.operatorStatus === "ready_for_review" && run.branch) return "Branch ready. Review the diff, then record ship or ask for a revision.";
    if (run.branch) return "Branch exists. Open the diff to see what changed or revise the branch from here.";
    if (run.operatorStatus === "needs_you") return "Needs your call. The trail shows where it stopped and the guidance box picks it back up.";
    return "Trail recorded. Activity stays here so you can audit what happened later.";
  }

  // The ship decision: a human's verb, available only after the diff has
  // been reviewed. The kernel gate refuses anything that did not finish
  // green - the button is the call, not the gate.
  let shipReason = $state({});
  let shipping = $state("");
  let shippedByRun = $state({});
  async function ship(run) {
    const reason = (shipReason[run.runId] ?? "").trim();
    if (!reason || shipping === run.runId) return;
    shipping = run.runId;
    try {
      const packet = await client.write(
        "/forge/run-ship-decisions.json",
        { run_id: run.runId, commit_sha: run.commitSha ?? "", reason, actor_id: actor },
        { receiptIntent: "forge_run_ship" }
      );
      if (packet.status === "RECORDED") {
        shippedByRun = { ...shippedByRun, [run.runId]: packet };
      } else {
        shippedByRun = { ...shippedByRun, [run.runId]: { error: packet.reason ?? "That did not record." } };
      }
    } catch (error) {
      shippedByRun = { ...shippedByRun, [run.runId]: { error: "Nothing recorded - that did not land." } };
    }
    shipping = "";
  }

  // Review and revise: not happy with the change? Say what you want and a
  // new run picks up this branch and addresses it - the prior work stays,
  // the branch grows a commit, and it still waits for your ship decision.
  let reviseComment = $state({});
  let revising = $state("");
  let reviseFlash = $state({});

  // Opening a run that needs recovery pre-fills the revision box with the note
  // the recovery surface suggests, so the operator starts from "here is the
  // sane next step", not a blank field. Never clobbers a note they are typing.
  $effect(() => {
    if (!openId) return;
    const run = allRuns.find((r) => r.runId === openId);
    if (!run) return;
    const recovery = runRecovery(run);
    if (recovery && !(reviseComment[openId] ?? "").trim()) {
      reviseComment = { ...reviseComment, [openId]: recovery.suggestedRevisionNote };
    }
  });

  // Start smaller: carry the run's ask into the composer, narrowed, so the next
  // attempt has a smaller proof target. Routes to the single front door.
  function startSmaller(run) {
    const ask = String(run.runTitle || run.intent || "").trim();
    const carried = ask ? `Narrow this down and try a smaller first step: ${ask}` : "Start with a smaller, well-scoped first step.";
    goto(`/forge?intent=${encodeURIComponent(carried)}`);
  }

  // Review the proof: jump to the activity trail at step depth and scroll to it.
  function reviewProof(run) {
    lens = "activity";
    depth = "medium";
    requestAnimationFrame(() => {
      document.getElementById(`run-${run.runId}`)?.querySelector(".lens-bar")?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
  }

  async function reviseRun(run) {
    const comment = (reviseComment[run.runId] ?? "").trim();
    if (!comment || revising === run.runId) return;
    revising = run.runId;
    try {
      const response = await fetch("/api/kernel/forge/run-revisions.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ run_id: run.runId, comment, allowed_commands: revisionAllowedCommands, actor_id: actor })
      });
      const packet = await response.json();
      if (packet.status === "RUN_STARTED") {
        reviseFlash = { ...reviseFlash, [run.runId]: { ok: true, line: "Revising. Watch the new run below; the branch will update." } };
        reviseComment = { ...reviseComment, [run.runId]: "" };
        diffByRun = { ...diffByRun, [run.runId]: null };
        openId = packet.run_id ?? openId;
        await invalidateAll();
      } else {
        reviseFlash = { ...reviseFlash, [run.runId]: { ok: false, line: packet.reason ?? "That did not start." } };
      }
    } catch (error) {
      reviseFlash = { ...reviseFlash, [run.runId]: { ok: false, line: "Nothing started - that did not land." } };
    }
    revising = "";
  }

  let buildingPlan = $state("");
  let buildPlanFlash = $state({});
  // Ratify a proposed plan: start a real run seeded with the plan the coder
  // proposed. Nothing was written until this - the plan (and the casting it
  // recommended) is now your call.
  async function buildPlan(run) {
    if (buildingPlan === run.runId) return;
    buildingPlan = run.runId;
    buildPlanFlash = { ...buildPlanFlash, [run.runId]: null };
    try {
      const intent = `${run.intent}\n\nApproved plan to carry out:\n${run.runSummary}`;
      const response = await fetch("/api/kernel/forge/runs.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ intent, actor_id: actor, plan_only: false })
      });
      const packet = await response.json();
      if (packet.status === "RUN_STARTED") {
        buildPlanFlash = { ...buildPlanFlash, [run.runId]: { ok: true, line: "Building the plan. Watch the new run below." } };
        openId = packet.run_id ?? openId;
        await invalidateAll();
      } else {
        buildPlanFlash = { ...buildPlanFlash, [run.runId]: { ok: false, line: packet.reason ?? "That did not start." } };
      }
    } catch (error) {
      buildPlanFlash = { ...buildPlanFlash, [run.runId]: { ok: false, line: "Nothing started - that did not land." } };
    }
    buildingPlan = "";
  }
</script>

<svelte:head><title>{openRunTitle ? `${openRunTitle.slice(0, 60)} - Runs - MDx` : "Runs - MDx"}</title></svelte:head>

<ForgeView
  title="Runs"
  backTo={{ href: "/forge", label: "Build" }}
  subtitle="Every build Forge has run, with its translated trail, change, and proof."
>
  <ForgeTrailMap
    scale="Run"
    active={trailStage}
    title="One agent, one branch"
    detail="Open any build for its translated receipt trail, diff, checks, and review handoff."
  />

  {#if notificationsSupported() && notifyState === "default" && anyRunning}
    <button type="button" class="notify-chip" onclick={turnOnNotifications}>
      Notify me when runs finish
    </button>
  {:else if notifyState === "granted" && anyRunning}
    <span class="notify-chip on" title="You'll be notified when a hidden-tab run finishes, needs your call, or fails">Notifications on</span>
  {/if}
  <div class="repos-bar">
    <span class="repos-label">Repos</span>
    <span class="repos-list">
      <span class="repo-chip self">MDx (this app){#if repos.length === 0} - the only one connected{/if}</span>
      {#each repos as repo (repo.repoId)}
        <span class="repo-chip" title={repo.kind === "remote" ? repo.originUrl : repo.root}>{repo.label}{#if repo.kind === "remote"} <span class="repo-tag">remote</span>{/if}</span>
      {/each}
    </span>
    <button type="button" class="mdx-btn small" onclick={() => (connectOpen = !connectOpen)}>
      {connectOpen ? "Close" : "Connect a repo"}
    </button>
  </div>
  {#if connectOpen}
    <div class="connect">
      <p class="connect-cue">Connect code you support, then point a run at it. Hosted MDx uses repository access you choose in GitHub; local MDx can also use folders on this Mac.</p>
      <RepoConnect
        actor={data.session?.user_id ?? "local_user"}
        hosted={data.session?.authenticated === true}
        onConnected={async (repoId, label) => {
          connectFlash = { ok: true, line: `Connected ${label}. Pick it when you start a run.` };
          await invalidateAll();
        }}
      />
      {#if connectFlash?.ok}<p class="flash">{connectFlash.line}</p>{/if}
    </div>
  {/if}

  {#if reconnectingRunHistory}
    <div class="forge-empty reconnecting" role="status">
      <h2>Reconnecting to your run history</h2>
      <p>Your runs and isolated workspaces stay recorded while MDx restores the latest trail.</p>
    </div>
  {:else if runs.length === 0}
    <div class="forge-empty">
      <h2>Nothing built yet</h2>
      <p>Describe a change on the Forge home and one agent reads the code, makes the change, and runs your checks - each run lands here with its full trail.</p>
      <a class="mdx-btn primary" href="/forge">Describe work &rarr;</a>
    </div>
  {:else}
    {#if representatives.length > 6}
      <div class="runs-toolbar">
        <input class="runs-search" type="search" placeholder="Search runs" bind:value={runSearch} aria-label="Search runs" />
      </div>
    {/if}
    {#if setAsideRuns.length > 0}
      <div class="set-aside-bar">
        <span>{setAsideRuns.length} set aside</span>
        <button type="button" class="link-btn" onclick={() => (showSetAside = !showSetAside)}>
          {showSetAside ? "Hide them" : "Show them"}
        </button>
      </div>
      {#if showSetAside}
        <ul class="set-aside-list">
          {#each setAsideRuns as run (run.runId)}
            <li>
              <span class="sa-title">{runHeadline(run)}</span>
              <span class="sa-status">{operatorStatusLabel(run)}</span>
              <button type="button" class="link-btn" onclick={() => bringBackRun(run.runId)}>Bring back</button>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
    {#if shownRuns.length === 0}
      <p class="runs-none">No run matches that search. Clear it to see all {runs.length}.</p>
    {/if}
    <ul class="run-list">
      {#each shownRuns as run (run.runId)}
        {@const group = groupFor(run)}
        {@const groupRunning = group?.runningCandidateCount > 0}
        <li class="run" id={`run-${run.runId}`} data-status={groupRunning ? "running" : run.status} class:just-started={run.runId === justStartedId}>
          <button type="button" class="run-head" onclick={() => openRun(run.runId)}>
            <span class="run-dot" data-status={groupRunning ? "running" : run.status} class:spin={groupRunning || isRunning(run.status)} aria-hidden="true"></span>
            <span class="run-main">
              <span class="run-status" data-op={groupRunning ? "working" : run.operatorStatus}>{groupRunning ? "Working" : operatorStatusLabel(run)}{#if run.runId === justStartedId} <span class="run-yours">run you just started</span>{/if}</span>
              {#if hasHumanTitle(run)}
                <span class="run-title">{runHeadline(run)}</span>
                {#if !(openId === run.runId && run.runSummary)}
                  <span class="run-summary">{run.runSummary || summaryLine(run)}</span>
                {/if}
              {:else}
                <span class="run-summary">{run.runSummary || runHeadline(run)}</span>
              {/if}
            </span>
            {#if group}
              <span class="run-group-chip" title="One parallel build. Open it to see every lane.">{Math.max(group.observedCandidateCount, group.plannedCandidateCount, group.candidateIds.length)} lanes</span>
            {/if}
            {#if isLeagueRun(run)}<span class="run-machine" class:accepted={trialAccepted(run)} title={trialAccepted(run) ? "Outside machine accepted into the scorecard." : "Outside machine held until checks accept it."}>{runnerHeader(run)}{#if trialAccepted(run)} · accepted{:else if run.quarantine?.outputQuarantined} · {trialExecuted(run) ? "ran, held" : "held trial"}{/if}</span>{/if}
            {#if run.model}<span class="run-coder" title="The coder that built this run - see its track record under Models">{run.model}</span>{/if}
            {#if contextFill(run)}
              {@const context = contextFill(run)}
              <span class="run-context" title={context.title} aria-label={context.title}><span class="run-context-bar"><span class="run-context-fill" style={`width:${context.pct}%`}></span></span>{context.label} context</span>
            {/if}
            {#if run.branch}<span class="run-branch" title={run.branch}>{run.branch}</span>{/if}
          </button>

          {#if openId === run.runId}
            {#if !isRunning(run.status) && !needsRecovery(run)}
              <div class="run-archive-row">
                <button type="button" class="run-archive" onclick={() => setAsideRun(run)} title="Quiet this run in the list. It stays on the ledger and one tap brings it back. Runs that need you cannot be set aside.">
                  Set aside
                </button>
              </div>
            {/if}
            <div class="trail">
              {#if group}
                <div class="run-group">
                  <p class="rg-summary">{groupSummaryLine(group)}</p>
                  <div class="rg-candidates">
                    {#each groupCandidates(group, allRuns) as cand, ci (cand.runId)}
                      <button type="button" class="rg-cand" class:active={cand.runId === run.runId} data-op={cand.operatorStatus} onclick={() => openRun(cand.runId)}>
                        <span class="rg-cand-lane">Lane {ci + 1}</span>
                        <span class="rg-cand-status">{operatorStatusLabel(cand)}</span>
                      </button>
                    {/each}
                  </div>
                  <a class="rg-review" href={candidateReviewHref(run)}>Open ranked review &rarr;</a>
                </div>
              {/if}
              <div class="run-workbench" data-op={run.operatorStatus}>
                <div class="rwb-copy">
                                    <p>{runWorkbenchLine(run)}</p>
                </div>
                <div class="rwb-actions">
                  <button type="button" class="mdx-btn small" class:primary={lens === "activity"} onclick={() => (lens = "activity")}>
                    Activity
                  </button>
                  {#if run.branch}
                    <button type="button" class="mdx-btn small" class:primary={lens === "diff"} onclick={() => openDiff(run)}>
                      Diff
                    </button>
                    <a class="mdx-btn small" href={candidateReviewHref(run)}>Open Review</a>
                  {/if}
                </div>
              </div>
              {#if run.runSummary}
                <div class="run-said">
                  <span class="rs-label">What Forge did</span>
                  <p class="rs-text">{run.runSummary}</p>
                </div>
              {/if}
              {#if isLeagueRun(run)}
                <div class="machine-boundary" class:accepted={trialAccepted(run)}>
                  <span class="mb-runner">{runnerHeader(run)}<span class="mb-tag">{trialAccepted(run) ? "accepted into the scorecard" : trialExecuted(run) ? "ran in an isolated copy" : "staged, not run"}</span></span>
                  {#if trialAccepted(run)}
                    <span class="mb-line">Passed the visible and hidden checks, and a person accepted it into the scorecard. Recorded as evidence - never auto-consumed into production.</span>
                  {:else if quarantineLine(run)}<span class="mb-line">{quarantineLine(run)}</span>{/if}
                  {#if run.leagueContext?.recommendationRationale}<span class="mb-why">{run.leagueContext.recommendationRationale}</span>{/if}
                </div>
              {/if}
              {#if isRunning(run.status)}
                <div class="live-now">
                  <RunViewer run={liveRun(run)} streamRoute={streamRouteFor(run)} />
                </div>
              {/if}
              <div class="lens-bar">
                <div class="mdx-segmented lens-tabs" aria-label="What to show">
                  <button type="button" class:active={lens === "activity"} onclick={() => (lens = "activity")}>Activity</button>
                  <button type="button" class:active={lens === "diff"} disabled={!run.branch} onclick={() => openDiff(run)}>Diff</button>
                </div>
                {#if lens === "activity"}
                  <div class="mdx-segmented depth-tabs" aria-label="How much to show">
                    <button type="button" class:active={depth === "little"} onclick={() => (depth = "little")}>Summary</button>
                    <button type="button" class:active={depth === "medium"} onclick={() => (depth = "medium")}>Steps</button>
                    <button type="button" class:active={depth === "full"} onclick={() => (depth = "full")}>Everything</button>
                  </div>
                {/if}
              </div>
              {#if lens === "activity"}
              {#if run.repoIntake?.generatedFrom}
                <div class="repo-intake">
                  <div>
                    <span class="intake-label">Getting to know this repo</span>
                    <span class="intake-line">{run.repoIntake.readinessStatus || "Looked it over - ready to work."}{#if run.repoIntake.sourceHost} · {run.repoIntake.sourceHost}{/if}{#if run.repoIntake.scoutCandidateCount} · {run.repoIntake.scoutCandidateCount} scout candidate{run.repoIntake.scoutCandidateCount === 1 ? "" : "s"}{/if}</span>
                  </div>
                  <div class="intake-tags">
                    {#each run.repoIntake.semanticOrientationOperations.slice(0, 4) as op (op)}
                      <span>{op}</span>
                    {/each}
                    {#each run.repoIntake.writeScopeHint.slice(0, 2) as path (path)}
                      <span>{path}</span>
                    {/each}
                  </div>
                  {#if run.repoIntake.safeNextMove}
                    <p>{run.repoIntake.safeNextMove}</p>
                  {/if}
                </div>
              {/if}
              {#if candidateGeometryLine(run)}
                <div class="candidate-geometry">
                  <div>
                    <span class="intake-label">Candidate set</span>
                    <span class="intake-line">{candidateGeometryLine(run)}</span>
                  </div>
                  {#if run.parallelCandidate?.writeScope?.length > 0}
                    <div class="intake-tags">
                      {#each run.parallelCandidate.writeScope.slice(0, 3) as path (path)}
                        <span>{path}</span>
                      {/each}
                    </div>
                  {/if}
                  <a href={candidateReviewHref(run)}>Open ranked review &rarr;</a>
                </div>
              {/if}
              {#each eventsAtDepth(run.events, depth) as event, eventIndex (eventIndex)}
                <div class="event" data-tone={event.tone}>
                  <span class="event-turn">{event.turn > 0 ? `#${event.turn}` : ""}</span>
                  <span class="event-body">
                    <span class="event-line">{event.line || event.kind.replace(/_/g, " ")}</span>
                    {#if depth === "full" && event.detail}<code class="event-detail">{event.detail}</code>{/if}
                  </span>
                </div>
              {/each}
              {#if depth === "little"}
                <div class="event" data-tone={run.status === "stopped" || run.status === "error" ? "bad" : isRunning(run.status) ? "muted" : "good"}>
                  <span class="event-turn"></span>
                  <span class="event-body"><span class="event-line">{statusLabel(run.status)} - {summaryLine(run)}</span></span>
                </div>
              {/if}
              {#if isRunning(run.status)}
                <div class="steer">
                  <p class="steer-line">It is working. You can redirect it, or stop it cleanly.</p>
                  <div class="steer-row">
                    <input type="text" bind:value={steerNote[run.runId]} placeholder="Redirect it - e.g. also handle the empty case" aria-label="Steer note" />
                    {#if controlAllowed(run, "steer")}
                      <button type="button" class="mdx-btn small" disabled={controlling === run.runId + "steer" || !(steerNote[run.runId] ?? "").trim()} onclick={() => sendControl(run, "steer")}>
                        {controlling === run.runId + "steer" ? "Sending..." : "Steer"}
                      </button>
                    {/if}
                    {#if controlAllowed(run, "stop")}
                      <button type="button" class="mdx-btn small" disabled={controlling === run.runId + "stop"} onclick={() => sendControl(run, "stop")}>
                        {controlling === run.runId + "stop" ? "Stopping..." : "Stop"}
                      </button>
                    {/if}
                  </div>
                  {#if controlFlash[run.runId]}<p class="flash" class:bad={!controlFlash[run.runId].ok}>{controlFlash[run.runId].line}</p>{/if}
                </div>
              {/if}
              {#if run.status === "plan_proposed"}
                <div class="intervene" data-status="plan_proposed">
                  <p class="intervene-head">Plan proposed - your call</p>
                  <p class="intervene-line">Nothing was written yet. Here is the plan and the machines it suggests. Approve to build it, or start over with a new ask.</p>
                  {#if run.runSummary}<p class="plan-proposed-body">{run.runSummary}</p>{/if}
                  <div class="intervene-row">
                    <button type="button" class="mdx-btn small primary" disabled={buildingPlan === run.runId} onclick={() => buildPlan(run)}>
                      {buildingPlan === run.runId ? "Starting..." : "Build this plan"}
                    </button>
                  </div>
                  {#if buildPlanFlash[run.runId]}<p class="flash" class:bad={!buildPlanFlash[run.runId].ok}>{buildPlanFlash[run.runId].line}</p>{/if}
                </div>
              {/if}
              {#if needsRecovery(run) && !isRunning(run.status) && run.status !== "plan_proposed"}
                {@const recovery = runRecovery(run)}
                <div class="recovery" data-caveat={recovery.isProofCaveat} data-baseline={recovery.isBaselineRed}>
                  <p class="recovery-title">{recovery.title}</p>
                  <p class="recovery-line">{recovery.recoveryLine}</p>
                  <div class="recovery-moves">
                    {#if recovery.showsResume}
                      <form method="POST" action="?/resume">
                        <input type="hidden" name="run_id" value={run.runId} />
                        <button type="submit" class="mdx-btn small primary">Resume from last step</button>
                      </form>
                    {/if}
                    <button type="button" class="mdx-btn small" onclick={() => reviewProof(run)}>Review proof</button>
                    {#if recovery.showsOpenDiff}
                      <button type="button" class="mdx-btn small" onclick={() => openDiff(run)}>Open diff</button>
                    {/if}
                    {#if recovery.showsStartSmaller}
                      <button type="button" class="mdx-btn small" onclick={() => startSmaller(run)}>Start smaller</button>
                    {/if}
                  </div>
                  {#if recovery.showsRevisionControl}
                    <div class="recovery-revise">
                      <input type="text" bind:value={reviseComment[run.runId]} placeholder="What should the next attempt focus on?" aria-label="Revision request" />
                      <button type="button" class="mdx-btn small primary" disabled={revising === run.runId || !(reviseComment[run.runId] ?? "").trim()} onclick={() => reviseRun(run)}>
                        {revising === run.runId ? "Starting..." : recovery.revisionControlLabel}
                      </button>
                    </div>
                  {/if}
                  {#if form?.runId === run.runId && form?.resumeError}<p class="flash bad">{form.resumeError}</p>{/if}
                  {#if reviseFlash[run.runId]}<p class="flash" class:bad={!reviseFlash[run.runId].ok}>{reviseFlash[run.runId].line}</p>{/if}
                </div>
              {/if}
              {:else}
                {#if !run.branch}
                  <p class="next-quiet">No branch yet - the change shows here once the agent commits its work.</p>
                {:else if diffLoading === run.runId}
                  <p class="next-quiet">Pulling up what changed...</p>
                {:else if diffByRun[run.runId] && !diffByRun[run.runId].error}
                  {@const diff = diffByRun[run.runId]}
                  <div class="next">
                    <p class="next-line">The agent committed its work to <code>{run.branch}</code>.</p>
                    <p class="next-quiet">Shipping is a recorded decision - the branch waits until you make it.</p>
                    <div class="diff">
                      <div class="diff-action">
                        <div>
                          <span class="diff-action-kicker">Review this branch</span>
                          <p>{diff.fileCount} {diff.fileCount === 1 ? "file changed" : "files changed"} on <code>{run.branch}</code>. Inspect the diff here, or use Review for the decision flow.</p>
                        </div>
                        <a class="mdx-btn small primary" href={candidateReviewHref(run)}>Open Review</a>
                      </div>
                      <p class="diff-summary">{diff.fileCount} {diff.fileCount === 1 ? "file" : "files"}</p>
                      {#each diff.files as file (file.path)}
                        <div class="diff-file">
                          <div class="diff-file-head">
                            <span class="diff-path">{file.path}</span>
                            <span class="diff-counts"><span class="add">+{file.added}</span> <span class="rem">-{file.removed}</span></span>
                          </div>
                          <pre class="diff-body">{#each file.lines as line, lineIndex (lineIndex)}<span class="dl" data-kind={line.kind}><span class="dl-ln dl-ln-old">{line.oldLine ?? ""}</span><span class="dl-ln dl-ln-new">{line.newLine ?? ""}</span><span class="dl-marker">{line.marker || " "}</span>{#each line.tokens as tok, ti (ti)}<span class="t-{tok.t}">{tok.v}</span>{/each}
</span>{/each}</pre>
                        </div>
                      {/each}

                      {#if shippedByRun[run.runId]?.status === "RECORDED"}
                        <p class="shipped">Shipped on the record. <code>{run.branch}</code> is ready to merge - the deploy step reads this decision.</p>
                      {:else}
                        {#if controlAllowed(run, "ship")}
                          <div class="ship">
                            <p class="ship-line">Reviewed it? Record your call to ship - it is yours, and it goes on the record.</p>
                            <input type="text" bind:value={shipReason[run.runId]} placeholder="Why this ships (your words)" aria-label="Ship reason" />
                            <button type="button" class="mdx-btn primary ship-go" disabled={shipping === run.runId || !(shipReason[run.runId] ?? "").trim()} onclick={() => ship(run)}>
                              {shipping === run.runId ? "Recording..." : "Ship it"}
                            </button>
                            {#if shippedByRun[run.runId]?.error}<p class="flash bad">{shippedByRun[run.runId].error}</p>{/if}
                          </div>
                        {/if}
                        {#if controlAllowed(run, "revise")}
                          <div class="revise">
                            <p class="revise-line">Not quite right? Say what to change and the agent revises this branch.</p>
                            <input type="text" bind:value={reviseComment[run.runId]} placeholder="What should change?" aria-label="Revision request" />
                            <button type="button" class="mdx-btn small" disabled={revising === run.runId || !(reviseComment[run.runId] ?? "").trim()} onclick={() => reviseRun(run)}>
                              {revising === run.runId ? "Starting..." : "Revise"}
                            </button>
                            {#if reviseFlash[run.runId]}<p class="flash" class:bad={!reviseFlash[run.runId].ok}>{reviseFlash[run.runId].line}</p>{/if}
                          </div>
                        {/if}
                      {/if}
                    </div>
                  </div>
                {:else}
                  <div class="next">
                    <p class="next-quiet">The change did not load - the branch may have been cleaned up.</p>
                    <button type="button" class="mdx-btn small" onclick={() => loadDiff(run)}>Load the change</button>
                  </div>
                {/if}
              {/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  <footer class="quiet-line">
    A run works in an isolated copy of the repo and runs only the checks you allow - it produces a branch, never a deploy.
    <details class="quiet-more">
      <summary>more</summary>
      <span>Boundary: {data.boundary}. Safe next move: {data.safeNext}.</span>
    </details>
  </footer>
</ForgeView>

<style>
  .repos-bar { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; min-width: 0; max-width: 100%; }
  .repos-label { font-family: var(--mdx-font-display); font-size: 13px; font-weight: 650; }
  .repos-list { display: flex; gap: 6px; flex-wrap: wrap; flex: 1; min-width: 0; }
  .repo-chip {
    font-size: 12px; padding: 3px 10px; border-radius: 999px;
    background: color-mix(in srgb, var(--mdx-text-muted) 10%, transparent);
    min-width: 0;
    max-width: min(100%, 280px);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .repo-chip.self { background: color-mix(in srgb, var(--mdx-accent-primary) 12%, transparent); }
  .repo-tag { font-size: var(--mdx-text-xs); text-transform: uppercase; letter-spacing: 0.04em; opacity: 0.65; }
  .connect { display: grid; gap: 8px; padding: 12px 14px; border-radius: var(--mdx-radius-md); background: var(--mdx-surface-raised); }
  .connect-cue { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); }
  .connect-note { margin: 0; font-size: var(--mdx-text-xs); color: var(--mdx-text-muted); }
  .connect-kind { display: flex; gap: 16px; font-size: 12.5px; }
  .connect-row { display: grid; grid-template-columns: minmax(0,1fr) minmax(0,1fr) minmax(0,2fr) auto; gap: 8px; }

  .forge-empty { display: grid; justify-items: center; text-align: center; gap: 12px; max-width: 460px; margin: 44px auto; }
  .forge-empty h2 { margin: 0; font-size: 18px; font-weight: 650; }
  .forge-empty p { margin: 0; color: var(--mdx-text-muted); font-size: 13.5px; line-height: 1.5; }

  .run-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; min-width: 0; max-width: 100%; }
  .run {
    border-radius: var(--mdx-radius-lg); background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle); overflow: hidden;
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    transition: border-color var(--mdx-motion-fast) var(--mdx-ease);
    min-width: 0;
    max-width: 100%;
  }
  .run:hover { border-color: var(--mdx-border-default); }
  .run.just-started { border-color: var(--mdx-accent-primary); box-shadow: var(--mdx-edge-highlight), 0 0 0 1px color-mix(in srgb, var(--mdx-accent-primary) 45%, transparent), var(--mdx-shadow-card); }
  .run-yours { font-family: var(--mdx-font); font-size: 11px; font-weight: 600; color: var(--mdx-accent-primary); padding: 1px 8px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-accent-primary) 14%, transparent); margin-left: 8px; vertical-align: middle; white-space: nowrap; }
  .run-yours::before { content: "· "; }
  .run-head {
    display: flex; align-items: center; gap: 12px; width: 100%; text-align: left;
    padding: 12px 14px; border: none; background: none; cursor: pointer; color: inherit;
    min-width: 0;
    max-width: 100%;
    flex-wrap: wrap;
  }
  .run-dot { width: 11px; height: 11px; border-radius: 50%; flex: none; background: var(--mdx-text-muted); }
  .run-dot[data-status="done"] { background: var(--mdx-accent-success); }
  .run-dot[data-status="cannot_proceed"], .run-dot[data-status="budget_exhausted"], .run-dot[data-status="error"] { background: var(--mdx-accent-error); }
  .run-dot.spin { animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 0%,100% { opacity: 1; } 50% { opacity: 0.3; } }
  .run-main { flex: 1; min-width: 0; display: grid; gap: 1px; }
  .run-status { font-family: var(--mdx-font-display); font-size: 13.5px; font-weight: 650; }
  /* The human status leads: a glance says wait, decide, or move on. */
  .run-status[data-op="needs_you"] { color: var(--mdx-accent-warning, #d9883a); }
  .run-status[data-op="ready_for_review"] { color: var(--mdx-accent-success); }
  .run-title { font-size: 13px; font-weight: 550; color: var(--mdx-text); line-height: 1.35; margin: 1px 0; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .run-said { margin: 0 14px 14px; padding: 12px 14px; border-radius: var(--mdx-radius-md, 10px); background: color-mix(in srgb, var(--mdx-accent-primary) 7%, transparent); border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 18%, transparent); }
  .rs-label { font-size: 11px; font-weight: 600; letter-spacing: 0.04em; text-transform: uppercase; color: var(--mdx-accent-primary); }
  .rs-text { margin: 5px 0 0; font-size: 13px; line-height: 1.5; color: var(--mdx-text); }
  .run-summary { font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.45; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .run-branch { flex: 0 1 auto; min-width: 0; max-width: 220px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--mdx-text-xs); font-family: var(--mdx-font-mono, monospace); color: var(--mdx-accent-primary); }
  .run-coder { flex: 0 1 auto; min-width: 0; max-width: 140px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--mdx-text-xs); font-family: var(--mdx-font-mono, monospace); color: var(--mdx-text-muted); padding: 1px 7px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-text-muted) 11%, transparent); }
  /* A league/trial run names the outside machine - the runner leads, the held
     posture is plain. Native runs never render this. */
  .run-machine { flex: 0 1 auto; min-width: 0; max-width: 220px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--mdx-text-xs); font-weight: 650; color: var(--mdx-accent-primary); padding: 2px 9px; border-radius: 999px; border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 30%, transparent); }
  .run-machine.accepted { color: var(--mdx-accent-success); border-color: color-mix(in srgb, var(--mdx-accent-success) 38%, transparent); }
  .machine-boundary { display: grid; gap: 3px; margin: 10px 0 4px; padding: 10px 12px; border-radius: var(--mdx-radius-md); border-left: 2px solid var(--mdx-accent-primary); background: color-mix(in srgb, var(--mdx-accent-primary) 7%, transparent); }
  .machine-boundary.accepted { border-left-color: var(--mdx-accent-success); background: color-mix(in srgb, var(--mdx-accent-success) 7%, transparent); }
  .mb-runner { font-size: 13px; font-weight: 650; display: flex; align-items: baseline; gap: 8px; }
  .mb-tag { font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.03em; color: var(--mdx-text-muted); }
  .mb-line { font-size: 12.5px; color: var(--mdx-text-secondary); line-height: 1.5; }
  .mb-why { font-size: 12px; color: var(--mdx-text-muted); line-height: 1.5; }

  .trail { padding: 4px 14px 14px; display: grid; gap: 3px; border-top: 1px solid color-mix(in srgb, var(--mdx-text-muted) 10%, transparent); }
  .run-workbench {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    align-items: center;
    margin: 8px 0 12px;
    padding: 12px 14px;
    border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 18%, var(--mdx-border-subtle));
    border-left: 3px solid var(--mdx-accent-primary);
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-primary) 6%, transparent);
    min-width: 0;
  }
  .run-workbench[data-op="ready_for_review"] {
    border-left-color: var(--mdx-accent-success);
    background: color-mix(in srgb, var(--mdx-accent-success) 7%, transparent);
  }
  .run-workbench[data-op="needs_you"] {
    border-left-color: var(--mdx-accent-warning, var(--mdx-accent-primary));
    background: color-mix(in srgb, var(--mdx-accent-warning, var(--mdx-accent-primary)) 8%, transparent);
  }
  .rwb-copy {
    display: grid;
    gap: 3px;
    min-width: 0;
  }
  .rwb-eyebrow {
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--mdx-text-muted);
  }
  .rwb-copy p {
    margin: 0;
    font-size: 13px;
    line-height: 1.45;
    color: var(--mdx-text-secondary);
  }
  .rwb-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 7px;
    flex: none;
  }
  /* The live stream leads an open running run; the full history sits below it. */
  .live-now { padding: 14px 0 16px; margin-bottom: 6px; border-bottom: 1px solid color-mix(in srgb, var(--mdx-text-muted) 10%, transparent); }
  /* The lens bar: Activity vs Diff, and the visibility dial when on Activity. */
  .lens-bar { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; margin: 8px 0 12px; }
  .lens-tabs, .depth-tabs { display: inline-flex; gap: 2px; padding: 3px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-md); background: var(--mdx-surface-sunken); }
  .lens-tabs button, .depth-tabs button { border: none; background: transparent; color: var(--mdx-text-muted); font: inherit; font-size: 12.5px; padding: 5px 12px; border-radius: var(--mdx-radius-sm, 6px); cursor: pointer; }
  .lens-tabs button:hover:not(:disabled), .depth-tabs button:hover { color: var(--mdx-text-primary); }
  .lens-tabs button.active, .depth-tabs button.active { background: var(--mdx-surface-raised); color: var(--mdx-text-primary); }
  .lens-tabs button:disabled { opacity: 0.4; cursor: default; }
  .repo-intake { display: grid; gap: 6px; margin: 0 0 10px 38px; padding: 9px 11px; border-left: 2px solid var(--mdx-accent-primary); background: color-mix(in srgb, var(--mdx-accent-primary) 6%, transparent); }
  .candidate-geometry { display: grid; gap: 6px; margin: 0 0 10px 38px; padding: 9px 11px; border-left: 2px solid var(--mdx-accent-success, var(--mdx-accent-primary)); background: color-mix(in srgb, var(--mdx-accent-success, var(--mdx-accent-primary)) 7%, transparent); }
  .candidate-geometry a { width: fit-content; font-size: 12.5px; color: var(--mdx-accent-primary); text-decoration: none; }
  .candidate-geometry a:hover { text-decoration: underline; }
  .repo-intake p, .intake-line { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); }
  .intake-label { font-weight: 650; margin-right: 8px; }
  .intake-tags { display: flex; flex-wrap: wrap; gap: 5px; font: 11.5px var(--mdx-font-mono, monospace); color: var(--mdx-text-secondary); }
  .event { display: flex; gap: 10px; align-items: baseline; min-height: 24px; padding: 2px 0; }
  .event-turn { flex: none; width: 28px; font-size: var(--mdx-text-xs); color: var(--mdx-text-muted); font-family: var(--mdx-font-mono, monospace); }
  .event-body { display: grid; gap: 2px; min-width: 0; }
  .event-detail { font-family: var(--mdx-font-mono, monospace); font-size: 11.5px; color: var(--mdx-text-tertiary); white-space: pre-wrap; word-break: break-word; }
  .event-line { font-size: 13px; }
  /* When a run stops short or needs a person, it never dead-ends: an inline
     "your move" with the guidance box right where you are watching. */
  .intervene { margin-top: 12px; padding: 12px 14px; border: 1px solid color-mix(in srgb, var(--mdx-accent-warning, #d9883a) 40%, transparent); border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-warning, #d9883a) 8%, transparent); }
  .intervene-head { margin: 0 0 4px; font-size: 13px; font-weight: 650; color: var(--mdx-text-primary); }
  .intervene-line { margin: 0 0 10px; font-size: 12.5px; color: var(--mdx-text-secondary); line-height: 1.5; }
  .plan-proposed-body { margin: 0 0 10px; padding: 10px 12px; font-size: 12.5px; color: var(--mdx-text-primary); line-height: 1.55; white-space: pre-wrap; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-md); background: var(--mdx-surface-raised); }
  .run-context { display: inline-flex; align-items: center; gap: 6px; font-size: 11px; color: var(--mdx-text-muted); white-space: nowrap; min-width: 0; max-width: 190px; overflow: hidden; text-overflow: ellipsis; }
  .run-context-bar { display: inline-block; width: 34px; height: 5px; border-radius: 3px; background: var(--mdx-border-subtle); overflow: hidden; }
  .run-context-fill { display: block; height: 100%; background: var(--mdx-text-muted); }
  .intervene-row { display: flex; gap: 8px; }
  .event[data-tone="good"] .event-line { color: var(--mdx-tone-ok-text); }
  .event[data-tone="bad"] .event-line { color: var(--mdx-tone-danger-text); }
  .event[data-tone="muted"] .event-line { color: var(--mdx-text-muted); }

  .next { margin-top: 8px; padding: 10px 12px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-primary) 7%, transparent); }
  .next-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
  .next-line { margin: 0; font-size: 13px; }

  .diff { margin-top: 10px; display: grid; gap: 10px; min-width: 0; max-width: 100%; }
  .diff-action {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    align-items: center;
    padding: 11px 12px;
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-success) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--mdx-accent-success) 22%, transparent);
  }
  .diff-action div {
    display: grid;
    gap: 3px;
    min-width: 0;
  }
  .diff-action-kicker {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--mdx-accent-success);
  }
  .diff-action p {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--mdx-text-secondary);
  }
  .diff-summary { margin: 0; font-size: 12px; color: var(--mdx-text-muted); }
  .ship { margin-top: 12px; display: grid; gap: 8px; padding: 12px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-primary) 8%, transparent); }
  .ship-line { margin: 0; font-size: 13px; }
  .ship input { width: 100%; box-sizing: border-box; padding: 8px 10px; font-size: 13px; border-radius: var(--mdx-radius-md); border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 25%, transparent); background: var(--mdx-surface-base, transparent); color: inherit; }
  .ship-go { justify-self: start; }
  .shipped { margin: 12px 0 0; font-size: 13px; color: var(--mdx-tone-ok-text); }
  .revise { margin-top: 8px; display: grid; gap: 6px; }
  .revise-line { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); }
  .revise input { width: 100%; box-sizing: border-box; padding: 8px 10px; font-size: 13px; border-radius: var(--mdx-radius-md); border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 25%, transparent); background: var(--mdx-surface-base, transparent); color: inherit; }
  .revise .mdx-btn { justify-self: start; }
  .steer { margin-top: 8px; display: grid; gap: 6px; padding: 10px 12px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-text-muted) 7%, transparent); }
  .steer-line { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); }
  .steer-row { display: flex; gap: 8px; }
  .steer-row input { flex: 1; min-width: 0; padding: 8px 10px; font-size: 13px; border-radius: var(--mdx-radius-md); border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 25%, transparent); background: var(--mdx-surface-base, transparent); color: inherit; }
  .diff-file { border-radius: var(--mdx-radius-md); overflow: hidden; border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 14%, transparent); min-width: 0; max-width: 100%; }
  .diff-file-head {
    display: flex; align-items: center; justify-content: space-between; gap: 10px;
    padding: 6px 10px; background: color-mix(in srgb, var(--mdx-text-muted) 8%, transparent);
  }
  .diff-path { font-family: var(--mdx-font-mono, monospace); font-size: 12px; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .diff-counts { font-family: var(--mdx-font-mono, monospace); font-size: var(--mdx-text-xs); }
  .diff-counts .add { color: var(--mdx-tone-ok-text); }
  .diff-counts .rem { color: var(--mdx-tone-danger-text); }
  .diff-body { margin: 0; padding: 10px 0; overflow-x: auto; max-width: 100%; font-family: var(--mdx-font-mono, monospace); font-size: 12.5px; line-height: 1.6; letter-spacing: -0.01em; background: var(--mdx-surface-base, transparent); }
  .dl { display: block; padding: 0 10px 0 0; white-space: pre; border-left: 2px solid transparent; }
  .dl-ln { display: inline-block; width: 3.5ch; padding: 0 6px 0 0; text-align: right; color: var(--mdx-text-faint); user-select: none; }
  .dl-ln-old { border-right: 1px solid var(--mdx-border-subtle); }
  .dl-ln-new { padding-left: 6px; }
  .dl-marker { display: inline-block; width: 1ch; padding-left: 8px; color: var(--mdx-text-faint); user-select: none; }
  .dl[data-kind="add"] { background: var(--mdx-diff-add-bg); border-left-color: var(--mdx-diff-add-gutter); }
  .dl[data-kind="del"] { background: var(--mdx-diff-del-bg); border-left-color: var(--mdx-diff-del-gutter); }
  .dl[data-kind="add"] .dl-marker { color: var(--mdx-diff-add-gutter); }
  .dl[data-kind="del"] .dl-marker { color: var(--mdx-diff-del-gutter); }
  .dl[data-kind="hunk"] .t-hunk { color: var(--mdx-accent-primary); }
  .dl[data-kind="meta"] .t-meta { color: var(--mdx-text-muted); }
  .dl .t-keyword { color: var(--mdx-code-keyword); }
  .dl .t-string { color: var(--mdx-code-string); }
  .dl .t-comment { color: var(--mdx-code-comment); font-style: italic; }
  .dl .t-number, .dl .t-value { color: var(--mdx-code-number); }
  .dl .t-type { color: var(--mdx-code-type); }
  .dl .t-fn { color: var(--mdx-code-fn); }
  .dl .t-text, .dl .t-punct { color: var(--mdx-text-secondary); }
  .next-quiet { margin: 4px 0 0; font-size: 12px; color: var(--mdx-text-muted); }

  .quiet-more { margin-top: 8px; }
  .quiet-more summary { cursor: pointer; font-size: 12.5px; color: var(--mdx-text-muted); }
  .raw { list-style: none; margin: 6px 0 0; padding: 0; display: grid; gap: 2px; }

  .flash { margin: 0; font-size: 13px; }
  .flash.bad { color: var(--mdx-tone-danger-text); }
  .quiet-line { font-size: 12.5px; color: var(--mdx-text-muted); }
  .quiet-line .quiet-more { display: inline; }
  .quiet-line summary { display: inline; }
  .run-archive-row { display: flex; justify-content: flex-end; padding: 2px 14px 0; }
  .run-archive { border: none; background: none; font: inherit; font-size: 12px; color: var(--mdx-text-muted); cursor: pointer; padding: 2px 4px; }
  .run-archive:hover { color: var(--mdx-text-secondary); text-decoration: underline; }

  /* Search + set-aside: a wide list stays scannable, and set-aside runs are
     always one visible tap from coming back - never silently gone. */
  .runs-toolbar { margin: 0 0 10px; }
  .runs-search { width: 100%; max-width: 320px; box-sizing: border-box; height: 34px; padding: 0 11px; font: inherit; font-size: 13px; border-radius: var(--mdx-radius-md); border: 1px solid var(--mdx-border-subtle); background: var(--mdx-surface-base); color: var(--mdx-text-primary); }
  .runs-search:focus-visible { outline: none; border-color: var(--mdx-accent-primary); }
  .runs-none { margin: 8px 2px; font-size: 12.5px; color: var(--mdx-text-muted); }
  .set-aside-bar { display: flex; align-items: center; gap: 10px; margin: 0 0 8px; font-size: 12.5px; color: var(--mdx-text-muted); }
  .set-aside-list { list-style: none; margin: 0 0 10px; padding: 8px 12px; display: grid; gap: 6px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-text-muted) 6%, transparent); }
  .set-aside-list li { display: flex; align-items: center; gap: 10px; font-size: 12.5px; }
  .sa-title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sa-status { color: var(--mdx-text-muted); white-space: nowrap; }
  .link-btn { border: none; background: none; font: inherit; font-size: 12.5px; color: var(--mdx-accent-primary); cursor: pointer; padding: 0; }
  .link-btn:hover { text-decoration: underline; }

  /* Parallel group: one row for the whole build, with every lane one tap away
     and a summary that reads "4 of 8 lanes done". */
  .run-group-chip { flex: none; font-size: var(--mdx-text-xs); font-weight: 650; color: var(--mdx-accent-primary); padding: 2px 9px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-accent-primary) 12%, transparent); white-space: nowrap; }
  .run-group { display: grid; gap: 8px; margin: 8px 0 12px; padding: 11px 14px; border-radius: var(--mdx-radius-md); border-left: 3px solid var(--mdx-accent-primary); background: color-mix(in srgb, var(--mdx-accent-primary) 6%, transparent); }
  .rg-summary { margin: 0; font-size: 13px; font-weight: 600; }
  .rg-candidates { display: flex; flex-wrap: wrap; gap: 6px; }
  .rg-cand { display: inline-flex; align-items: baseline; gap: 6px; border: 1px solid var(--mdx-border-subtle); border-radius: 999px; background: var(--mdx-surface-base); color: var(--mdx-text-secondary); font: inherit; font-size: 11.5px; padding: 3px 10px; cursor: pointer; }
  .rg-cand:hover { border-color: color-mix(in srgb, var(--mdx-accent-primary) 45%, var(--mdx-border-subtle)); color: var(--mdx-text-primary); }
  .rg-cand.active { border-color: var(--mdx-accent-primary); background: color-mix(in srgb, var(--mdx-accent-primary) 10%, var(--mdx-surface-base)); color: var(--mdx-text-primary); }
  .rg-cand-lane { font-weight: 650; }
  .rg-cand-status { color: var(--mdx-text-muted); }
  .rg-cand[data-op="needs_you"] .rg-cand-status { color: var(--mdx-accent-warning, #d9883a); }
  .rg-cand[data-op="ready_for_review"] .rg-cand-status { color: var(--mdx-accent-success); }
  .rg-review { width: fit-content; font-size: 12.5px; color: var(--mdx-accent-primary); text-decoration: none; }
  .rg-review:hover { text-decoration: underline; }

  /* Guided recovery: a red or stalled run reads as "here is what happened and
     here are your sane moves", calm and specific, never a dead end. */
  .recovery { margin-top: 12px; padding: 13px 15px; border-radius: var(--mdx-radius-md); border: 1px solid color-mix(in srgb, var(--mdx-accent-warning, #d9883a) 38%, transparent); border-left: 3px solid var(--mdx-accent-warning, #d9883a); background: color-mix(in srgb, var(--mdx-accent-warning, #d9883a) 8%, transparent); display: grid; gap: 9px; }
  .recovery[data-caveat="true"] { border-color: color-mix(in srgb, var(--mdx-accent-primary) 34%, transparent); border-left-color: var(--mdx-accent-primary); background: color-mix(in srgb, var(--mdx-accent-primary) 7%, transparent); }
  .recovery-title { margin: 0; font-size: 13.5px; font-weight: 650; color: var(--mdx-text-primary); }
  .recovery-line { margin: 0; font-size: 12.5px; line-height: 1.5; color: var(--mdx-text-secondary); }
  .recovery-moves { display: flex; flex-wrap: wrap; gap: 7px; }
  .recovery-revise { display: flex; gap: 8px; }
  .recovery-revise input { flex: 1; min-width: 0; padding: 8px 11px; font-size: 13px; border-radius: var(--mdx-radius-md); border: 1px solid var(--mdx-border-default); background: var(--mdx-surface-base); color: inherit; }
  .notify-chip { font: inherit; font-size: 12px; padding: 4px 11px; border-radius: 999px; border: 1px solid var(--mdx-border-subtle); background: var(--mdx-surface-raised); color: var(--mdx-text-secondary); cursor: pointer; margin: 0 0 10px; }
  .notify-chip.on { cursor: default; color: var(--mdx-accent-success); }
  @media (max-width: 640px) {
    .run-workbench,
    .diff-action {
      align-items: stretch;
      flex-direction: column;
    }
    .rwb-actions {
      justify-content: flex-start;
    }
    .diff-action .mdx-btn {
      width: fit-content;
    }
  }
</style>
