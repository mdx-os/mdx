<script>
  import { onMount } from "svelte";
  import { invalidate } from "$app/navigation";
  import { startLivePoll } from "../../../lib/livePoll.js";
  import { refusalLine } from "../../../lib/refusals.js";
  import { createMdxClient } from "@mdx/client";
  import { diffLines } from "../../../lib/codeHighlight.js";
  import ForgeView from "../../../lib/ForgeView.svelte";
  import FirstUseHint from "../../../lib/FirstUseHint.svelte";

  let { data } = $props();

  // The same governed-write client the Runs ship door uses: every verb here
  // posts to the routes that already carry the gate and the receipt. The
  // Review Room is just a second, better-shaped doorway to the same decisions.
  const client = $derived(createMdxClient({ baseUrl: "/api/kernel", session: data.session }));
  const actor = $derived(data.session?.user_id ?? "local_user");

  // The review queue: finished runs worth a human's eyes, freshest first.
  // A still-running run has nothing to decide yet, so it stays out.
  const REVIEWABLE = new Set([
    "done",
    "finished",
    "cannot_proceed",
    "error",
    "budget_exhausted",
    "stopped"
  ]);
  const queue = $derived(
    (Array.isArray(data.runs?.runs) ? data.runs.runs : [])
      .filter((run) => REVIEWABLE.has(String(run?.status ?? "")))
      // Demo/seed runs are not real work waiting on a decision - keep them out of
      // the review queue so it reads as your actual changes, not stale fixtures.
      // The seed marker lives on the work item id as often as the run id.
      .filter((run) => {
        const tag = `${run?.run_id ?? ""} ${run?.work_item_id ?? ""}`;
        return !/(^|\s)(tier0_dev|forge_dev_seed)/.test(tag);
      })
      .map((run) => {
        const rawTitle = String(run?.run_title || run?.operator_intent || run?.intent || "");
        const isMeta = /^accepted:|selected_checks|language_pack=|execution_geometry=/.test(rawTitle);
        const summary = String(run?.operator_run_summary || run?.run_summary || "").trim();
        const machineTrial =
          String(run?.origin ?? "") === "system" ||
          /quarantined (trial|fixture)|trial packet|fixture trial/i.test(String(run?.run_title ?? "") + String(run?.operator_intent ?? ""));
        const runId = String(run?.run_id ?? "");
        const tail = runId.match(/(\d+)\s*$/)?.[1];
        return {
          runId,
          status: String(run?.status ?? ""),
          workItemId: String(run?.work_item_id ?? ""),
          branch: String(run?.branch ?? ""),
          machineTrial,
          // Never a raw id in the queue: operator words, then the summary's
          // first sentence, then a short human handle.
          title:
            (isMeta ? "" : rawTitle) ||
            (summary ? (summary.split(/(?<=[.!?])\s/)[0] ?? summary).slice(0, 96) : "") ||
            (tail ? `Run ${tail}` : runId)
        };
      })
  );

  // Queue at scale: surface the runs that need the most care first, and let the
  // operator narrow by status or search instead of scanning a flat list.
  const ATTENTION = new Set(["needs_attention", "error", "cannot_proceed", "budget_exhausted"]);
  let statusFilter = $state("all");
  let searchText = $state("");
  const attentionCount = $derived(queue.filter((run) => ATTENTION.has(run.status)).length);
  const filteredQueue = $derived.by(() => {
    const q = searchText.trim().toLowerCase();
    return queue
      .filter((run) => {
        if (statusFilter === "attention" && !ATTENTION.has(run.status)) return false;
        if (statusFilter === "ready" && ATTENTION.has(run.status)) return false;
        if (q && !`${run.title} ${run.workItemId} ${run.runId} ${run.branch}`.toLowerCase().includes(q)) return false;
        return true;
      })
      .slice()
      .sort((a, b) => (ATTENTION.has(a.status) ? 0 : 1) - (ATTENTION.has(b.status) ? 0 : 1));
  });

  let selectedRunId = $state("");
  // Lead the review with the operator's own ask, not the model id. Pulled from
  // the queue (which already prefers run_title / operator_intent over metadata).
  const selectedTitle = $derived(String(queue.find((r) => r.runId === selectedRunId)?.title || ""));
  let packet = $state(null);
  let patchByPath = $state({});
  let signalsByPath = $state({});
  let loading = $state(false);
  let openFiles = $state({});
  let openGenerated = $state(false);
  // Diff reading controls: unified or side-by-side, and a find scoped to the
  // open review that force-expands files with matches and highlights them.
  let diffMode = $state("unified"); // "unified" | "split"
  let findText = $state("");
  let findIndex = $state(0);
  // Switching candidates should be instant: cache each run's packet + diff by
  // run id so flipping between candidates does not refetch everything.
  const packetCache = new Map();
  let prDraftHost = $state("github");
  // Lit after a ship decision: the PR draft is the road out, not a footnote.
  let prDraftNudge = $state(false);
  let prDraftBase = $state("main");
  let prDraft = $state(null);
  let prDrafting = $state(false);
  let prDraftFlash = $state(null);
  let sourceHostReadiness = $state(null);
  let sourceHostBusy = $state(false);

  const STATUS_LABEL = {
    ready_for_review: "Ready for review",
    needs_attention: "Needs you",
    stopped: "Stopped",
    shipped: "Shipped"
  };
  // Short, human run-status for the narrow queue chip (the wide Runs screen uses
  // the longer statusLabel). One vocabulary, never a raw enum.
  const QUEUE_STATUS = {
    done: "Done",
    finished: "Done",
    error: "Errored",
    cannot_proceed: "Cannot finish",
    budget_exhausted: "Out of turns",
    stopped: "Stopped"
  };
  const queueStatusLabel = (s) => QUEUE_STATUS[s] ?? String(s ?? "").replace(/_/g, " ");

  // Honest quick-pick reasons for the ship decision - a starting point the
  // person can accept or edit, so the record is never a blank rubber stamp.
  // They fill the same governed reason field a free-text note would.
  const SHIP_REASONS = [
    "Checks pass, the diff matches the ask",
    "Reviewed the diff, scope stays contained",
    "Read the change end to end, ready to merge"
  ];

  // Diff rendering: parse each open file's patch once (line numbers included),
  // and scope a find across the whole open review.
  const renderedByPath = $derived.by(() => {
    const out = {};
    for (const [path, patch] of Object.entries(patchByPath)) out[path] = diffLines(patch);
    return out;
  });
  const lineText = (line) => line.tokens.map((t) => t.v).join("");
  const findQuery = $derived(findText.trim().toLowerCase());
  // Every matching row across the open review, in file-then-line order, so
  // "3 of 17" and prev/next can walk them and scroll each into view.
  const findHits = $derived.by(() => {
    const q = findQuery;
    if (!q) return [];
    const hits = [];
    for (const [path, lines] of Object.entries(renderedByPath)) {
      lines.forEach((line, li) => {
        if (line.kind === "hunk" || line.kind === "meta") return;
        if (lineText(line).toLowerCase().includes(q)) hits.push({ path, li });
      });
    }
    return hits;
  });
  const activeHit = $derived(findHits.length ? findHits[((findIndex % findHits.length) + findHits.length) % findHits.length] : null);
  function lineIsHit(path, li) {
    return findQuery && renderedByPath[path]?.[li] && lineText(renderedByPath[path][li]).toLowerCase().includes(findQuery);
  }
  function stepFind(delta) {
    if (!findHits.length) return;
    findIndex = (((findIndex + delta) % findHits.length) + findHits.length) % findHits.length;
  }
  // A find should open the files that contain matches and bring the active one
  // into view - nothing worse than "3 of 17" pointing at a collapsed file.
  $effect(() => {
    if (!findHits.length) return;
    const opens = { ...openFiles };
    for (const hit of findHits) opens[hit.path] = true;
    let changed = false;
    for (const k of Object.keys(opens)) if (opens[k] !== openFiles[k]) changed = true;
    if (changed) openFiles = opens;
    const hit = activeHit;
    if (hit && typeof document !== "undefined") {
      requestAnimationFrame(() => {
        document.getElementById(`dl-${hit.path}-${hit.li}`)?.scrollIntoView({ block: "center", behavior: "smooth" });
      });
    }
  });

  async function postJson(path, bodyObj) {
    const response = await fetch(`/api/kernel${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(bodyObj)
    });
    if (!response.ok) throw new Error(`${path} failed`);
    return response.json();
  }

  function resetReviewState(runId) {
    selectedRunId = runId;
    openFiles = {};
    actionFlash = null;
    panelFlash = null;
    prDraft = null;
    prDraftFlash = null;
    sourceHostReadiness = null;
    shipReason = "";
    reviseComment = "";
    findText = "";
    findIndex = 0;
  }

  // Keep the open candidate in the URL so a refresh or a second tab restores
  // it and the link is shareable. A user-initiated open pushes a history entry
  // (back navigates between candidates); the initial mount replaces.
  function syncRunUrl(runId, push) {
    if (typeof window === "undefined" || !runId) return;
    const url = new URL(window.location.href);
    if (url.searchParams.get("run") === runId) return;
    url.searchParams.set("run", runId);
    url.searchParams.delete("run_id");
    window.history[push ? "pushState" : "replaceState"]({}, "", url);
  }

  async function openReview(runId, { force = false, push = true } = {}) {
    if (!runId) return;
    syncRunUrl(runId, push);
    // Instant candidate switching: a cached packet paints immediately; we only
    // refresh source-host readiness in the background. A ship/revise passes
    // force to bypass the cache and re-read the fresh verdict.
    if (!force && packetCache.has(runId)) {
      const cached = packetCache.get(runId);
      resetReviewState(runId);
      packet = cached.packet;
      patchByPath = cached.patchByPath;
      signalsByPath = cached.signalsByPath;
      loading = false;
      if (packet?.status === "OK") loadSourceHostReadiness(runId);
      return;
    }
    resetReviewState(runId);
    loading = true;
    packet = null;
    patchByPath = {};
    try {
      const [pkt, diff] = await Promise.all([
        postJson("/forge/review-packet.json", { run_id: runId }),
        postJson("/forge/run-diff.json", { run_id: runId }).catch(() => null)
      ]);
      packet = pkt && pkt.status === "OK" ? pkt : pkt;
      const map = {};
      const signals = {};
      for (const file of diff?.files ?? []) {
        if (!file?.path) continue;
        map[file.path] = file.patch ?? "";
        // Kernel-computed proof signals: which files the trail actually
        // worked (and where it erred), so review starts at the edge cases.
        signals[file.path] = {
          confidence: String(file.agent_confidence ?? ""),
          errorSteps: Number(file.error_step_count ?? 0)
        };
      }
      patchByPath = map;
      signalsByPath = signals;
      // Cache the fetched packet for instant re-open. Skipped for a not-OK
      // read so a transient refusal never sticks.
      if (packet?.status === "OK") packetCache.set(runId, { packet, patchByPath: map, signalsByPath: signals });
      if (packet?.status === "OK") await loadSourceHostReadiness(runId);
    } catch (error) {
      packet = { status: "REFUSED", reason: "the review could not be read right now" };
    }
    loading = false;
  }

  function toggleFile(path) {
    openFiles = { ...openFiles, [path]: !openFiles[path] };
  }


  // The governed verbs, brought into the room. Each posts to the route the
  // Runs ship door already uses, then re-reads the packet so the verdict moves
  // with the decision. A reason is the human's; the room never invents one.
  let shipReason = $state("");
  let reviseComment = $state("");
  let acting = $state("");
  let actionFlash = $state(null);
  let shipInput = $state(null);
  let reviseInput = $state(null);

  // Money-action shortcuts, only when no field is focused: A ships (or focuses
  // the reason), R asks for a revision, Esc clears an active find. The keys are
  // shown in each button's title, the way the Mac app does.
  function onReviewKeydown(event) {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    if (event.key === "Escape") {
      if (findText) { event.preventDefault(); findText = ""; }
      return;
    }
    const tag = event.target?.tagName?.toLowerCase();
    if (tag === "input" || tag === "textarea" || tag === "select") return;
    if (!selectedRunId || !packet || packet.review_status === "shipped") return;
    const key = event.key.toLowerCase();
    if (key === "a" && packet.review_status === "ready_for_review") {
      event.preventDefault();
      if (shipReason.trim()) ship();
      else shipInput?.focus();
    } else if (key === "r") {
      event.preventDefault();
      if (reviseComment.trim()) askRevision();
      else reviseInput?.focus();
    }
  }

  // Check rows carry the whole observation ("run_command pnpm test exit=0
  // tail=/private/var/folders/..."). The command is the headline; the tail
  // is evidence behind a disclosure, with machine temp paths shortened to
  // what a person can read (walkthrough finding).
  function checkCommand(name) {
    const text = String(name ?? "");
    const command = text.replace(/^run_command\s+/, "").split(/\s+exit=|\s+tail=/)[0].trim();
    return command || text;
  }
  function checkTail(name) {
    const text = String(name ?? "");
    const tailIndex = text.indexOf("tail=");
    if (tailIndex === -1) return "";
    return text
      .slice(tailIndex + 5)
      .replace(/\/(?:private\/)?var\/folders\/[^\s]+\//g, ".../")
      .replace(/\/Users\/[^/\s]+\//g, "~/")
      .trim();
  }

  async function ship() {
    const reason = shipReason.trim();
    if (!reason || acting) return;
    acting = "ship";
    actionFlash = null;
    try {
      const result = await client.write(
        "/forge/run-ship-decisions.json",
        { run_id: selectedRunId, commit_sha: packet?.commit_sha ?? "", reason, actor_id: actor },
        { receiptIntent: "forge_run_ship" }
      );
      if (result.status === "RECORDED") {
        shipReason = "";
        actionFlash = {
          ok: true,
          line: "Shipped. The decision is on the record - next move: prepare the PR draft below and take the branch to your host.",
          changelog: true
        };
        packetCache.delete(selectedRunId);
        await openReview(selectedRunId, { force: true });
        prDraftNudge = true;
        // The ship becomes a citable record: a changelog entry carrying the
        // run id and branch. Fire-and-forget; the ship decision receipt is
        // already durable either way. (Kernel-side auto-write stays an ask.)
        const entryTitle = (packet?.run_title ?? packet?.title ?? `Forge run ${selectedRunId}`).slice(0, 120);
        postJson("/changelog/entries.json", {
          title: `Shipped: ${entryTitle}`,
          summary: `Human ship decision recorded for ${selectedRunId} on ${packet?.branch ?? "its branch"}. Reason: ${reason.slice(0, 200)}`,
          kind: "improvement",
          actor_id: actor
        }).catch(() => {});
      } else {
        actionFlash = { ok: false, line: refusalLine(result, { fallback: "That did not record." }) };
      }
    } catch (error) {
      actionFlash = { ok: false, line: "Nothing recorded - that did not land." };
    }
    acting = "";
  }

  async function askRevision() {
    const comment = reviseComment.trim();
    if (!comment || acting) return;
    acting = "revise";
    actionFlash = null;
    try {
      const response = await fetch("/api/kernel/forge/run-revisions.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ run_id: selectedRunId, comment, allowed_commands: [], actor_id: actor })
      });
      const result = await response.json();
      if (result.status === "RUN_STARTED") {
        reviseComment = "";
        actionFlash = { ok: true, line: "Revising. A new run picks up this branch - watch it in Runs." };
      } else {
        actionFlash = { ok: false, line: result.reason ?? "That did not start." };
      }
    } catch (error) {
      actionFlash = { ok: false, line: "Nothing started - that did not land." };
    }
    acting = "";
  }

  // Convene a diverse review panel on the change. Real model calls (cost), so
  // it is explicit. Each model reads from a distinct lens; the room keeps their
  // dissent first-class. After it records, the packet re-reads to show it.
  let convening = $state(false);
  let panelFlash = $state(null);
  async function convenePanel() {
    if (convening || !selectedRunId) return;
    convening = true;
    panelFlash = null;
    try {
      const result = await postJson("/forge/review-panel.json", { run_id: selectedRunId });
      if (result.status === "RECORDED") {
        packetCache.delete(selectedRunId);
        await openReview(selectedRunId, { force: true });
      } else {
        panelFlash = { ok: false, line: result.reason ?? "The panel did not run." };
      }
    } catch (error) {
      panelFlash = { ok: false, line: "The panel did not run - that did not land." };
    }
    convening = false;
  }

  const VERDICT_LABEL = { ready: "ready", "needs work": "needs work", unavailable: "unavailable" };

  function formatCandidateScore(value) {
    if (typeof value !== "number" || Number.isNaN(value)) return "not scored";
    return `${Math.round(value)} pts`;
  }

  function candidateCurrentIsRecommended(recommendation, selected) {
    if (recommendation?.current_run_is_recommended === true) return true;
    if (recommendation?.current_run_is_recommended === false) return false;
    const current = recommendation?.current_run_id || selected;
    return Boolean(recommendation?.recommended_run_id && recommendation.recommended_run_id === current);
  }

  function candidateLine(recommendation, selected) {
    const count = Number(recommendation?.candidate_count ?? 0);
    if (count <= 1) return "Single run. No fleet choice is required.";
    const current = recommendation?.current_run_id ?? "this run";
    const recommended = recommendation?.recommended_run_id ?? "";
    const rank = Number(recommendation?.current_rank ?? 0);
    if (candidateCurrentIsRecommended(recommendation, selected)) {
      return `This run is the recommended candidate, ranked ${rank || 1} of ${count}.`;
    }
    return `Forge recommends ${recommended || "another candidate"}; ${current} is ranked ${rank || "outside the top"} of ${count}.`;
  }

  function candidateDiffLine(candidate) {
    const diff = candidate?.diff_quality ?? {};
    if (!diff.available) return "diff not available yet";
    const churn = Number(diff.added_total ?? 0) + Number(diff.removed_total ?? 0);
    return `${diff.real_change_count ?? 0} source, ${diff.generated_count ?? 0} generated, ${diff.artifact_count ?? 0} artifact, ${churn} lines`;
  }

  async function loadSourceHostReadiness(runId = selectedRunId) {
    if (!runId) return;
    sourceHostBusy = true;
    try {
      const result = await postJson("/forge/source-host-readiness.json", {
        run_id: runId,
        target_host: prDraftHost,
        base_branch: prDraftBase
      });
      sourceHostReadiness = result?.status === "OK" ? result : result;
    } catch (error) {
      sourceHostReadiness = { status: "REFUSED", reason: "source-host readiness could not be read" };
    }
    sourceHostBusy = false;
  }

  let copiedNote = $state("");
  let copiedTimer = null;
  function copyDraft(text) {
    if (!text) return;
    navigator.clipboard?.writeText(text).then(() => {
      copiedNote = "Copied";
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copiedNote = ""), 2000);
    });
  }

  // The branch is on the kernel's clone; the compare page is where a human
  // opens the PR. Built from the draft's own plan when present, else from
  // the repo origin + branch.
  function compareUrl(draft) {
    const direct = String(draft?.source_host_plan?.compare_url ?? draft?.compare_url ?? "");
    if (direct.startsWith("http")) return direct;
    const branch = String(draft?.source_branch ?? draft?.branch ?? review?.branch ?? "");
    let origin = String(draft?.source_host_plan?.origin_url ?? draft?.origin_url ?? "");
    if (!origin || !branch) return "";
    if (origin.endsWith(".git")) origin = origin.slice(0, -4);
    if (origin.startsWith("git@")) origin = "https://" + origin.slice(4).replace(":", "/");
    if (!origin.startsWith("http")) return "";
    return `${origin}/compare/${encodeURIComponent(branch)}?expand=1`;
  }

  async function preparePrDraft() {
    if (prDrafting || !selectedRunId) return;
    prDrafting = true;
    prDraftFlash = null;
    try {
      await loadSourceHostReadiness(selectedRunId);
      const result = await postJson("/forge/source-host-pr-drafts.json", {
        run_id: selectedRunId,
        target_host: prDraftHost,
        base_branch: prDraftBase,
        actor_id: actor
      });
      if (result.status === "DRAFT_RECORDED") {
        prDraft = result;
        prDraftFlash = {
          ok: true,
          line: `PR draft recorded for ${result.source_host_plan?.review_surface ?? "pull request"}. No branch was pushed and no PR was opened.`
        };
      } else {
        prDraft = null;
        prDraftFlash = { ok: false, line: refusalLine(result, { fallback: "That PR draft did not record." }) };
      }
    } catch (error) {
      prDraft = null;
      prDraftFlash = { ok: false, line: "Nothing recorded - the PR draft could not be prepared." };
    }
    prDrafting = false;
  }

  async function stopRun() {
    if (acting) return;
    acting = "stop";
    actionFlash = null;
    try {
      const result = await client.write(
        "/forge/run-controls.json",
        { run_id: selectedRunId, control: "stop", note: "", actor_id: actor },
        { receiptIntent: "forge_run_control" }
      );
      actionFlash =
        result.status === "RECORDED"
          ? { ok: true, line: "Stopping at the next turn." }
          : { ok: false, line: result.reason ?? "That did not land." };
    } catch (error) {
      actionFlash = { ok: false, line: "Nothing sent - that did not land." };
    }
    acting = "";
  }

  onMount(() => {
    const urlParams = new URLSearchParams(location.search);
    const wanted = urlParams.get("run") ?? urlParams.get("run_id");
    openReview(wanted || queue[0]?.runId || "", { push: false });
    // Live queue: a run finishing while you sit in Review should appear on its
    // own, the way Runs already does. Scoped to the runs projection, so this
    // never re-runs the layout waterfall, and visibility-gated by startLivePoll.
    return startLivePoll({
      isActive: () => true,
      refresh: () => invalidate("forge:runs")
    });
  });
</script>

<svelte:head><title>{selectedTitle ? `${selectedTitle.slice(0, 60)} - Review - MDx` : "Review - MDx"}</title></svelte:head>

<svelte:window onkeydown={onReviewKeydown} />

<ForgeView
  title="Review"
  subtitle="Changes waiting for your call - what each one built, in the order that makes sense, and the one decision it needs from you."
>
  {#if queue.length > 0}
    <FirstUseHint
      surface="forge-review"
      title="Your call"
      body="Pick a change to see what it built and the proof behind it, then ship it, ask for a revision, or stop it. Nothing ships without you."
    />
  {/if}
  {#if queue.length === 0}
    <div class="review-empty">
      <h2>Nothing to review yet</h2>
      <p>When a build finishes, the change waits here with its checks, diff, and the one decision it needs from you - ship it, ask for a revision, or stop it.</p>
      <a class="mdx-btn primary" href="/forge">Describe work</a>
    </div>
  {:else}
  <div class="review-body">
    <aside class="review-queue" aria-label="Runs to review">
        {#if queue.length > 4}
          <div class="rq-controls">
            <input class="rq-search" type="search" placeholder="Search runs" bind:value={searchText} aria-label="Search runs to review" />
            <div class="rq-filters" role="group" aria-label="Filter by status">
              <button type="button" class:active={statusFilter === "all"} aria-pressed={statusFilter === "all"} onclick={() => (statusFilter = "all")}>All {queue.length}</button>
              <button type="button" class:active={statusFilter === "attention"} aria-pressed={statusFilter === "attention"} onclick={() => (statusFilter = "attention")}>Needs you {attentionCount}</button>
              <button type="button" class:active={statusFilter === "ready"} aria-pressed={statusFilter === "ready"} onclick={() => (statusFilter = "ready")}>Ready {queue.length - attentionCount}</button>
            </div>
          </div>
        {/if}
        {#each filteredQueue.filter((r) => !r.machineTrial) as run (run.runId)}
          <button
            type="button"
            class="queue-row"
            class:active={run.runId === selectedRunId}
            onclick={() => openReview(run.runId)}
          >
            <span class="queue-title">{run.title || run.workItemId || run.runId}</span>
            <span class="queue-status" data-status={run.status}>{queueStatusLabel(run.status)}</span>
          </button>
        {/each}
        {#if filteredQueue.some((r) => r.machineTrial)}
          <!-- System trials (builder fixtures, quarantined packets) fold below
               the real work instead of drowning it (walkthrough finding: ~15
               machine rows crowded the queue). -->
          <details class="rq-trials">
            <summary>{filteredQueue.filter((r) => r.machineTrial).length} system trials</summary>
            {#each filteredQueue.filter((r) => r.machineTrial) as run (run.runId)}
              <button
                type="button"
                class="queue-row trial"
                class:active={run.runId === selectedRunId}
                onclick={() => openReview(run.runId)}
              >
                <span class="queue-title">{run.title || run.workItemId || run.runId}</span>
                <span class="queue-status" data-status={run.status}>{queueStatusLabel(run.status)}</span>
              </button>
            {/each}
          </details>
        {/if}
        {#if filteredQueue.length === 0}
          <p class="rq-none">Nothing matches that filter - clear it to see all {queue.length}.</p>
        {/if}
    </aside>

    <article class="review-main" aria-label="The change under review">
      {#if loading}
        <p class="quiet">Reading the change...</p>
      {:else if !packet}
        <p class="quiet">Pick a change on the left to make the call.</p>
      {:else if packet.status === "REFUSED"}
        <p class="quiet">{packet.reason}</p>
      {:else}
        {@const diff = packet.diff ?? {}}
        {@const reviewBrief = packet.principal_review ?? {}}
        {@const repoIntel = packet.repo_intelligence ?? {}}
        {@const observedSemanticOps = Array.isArray(repoIntel.semantic_query_operations_observed) ? repoIntel.semantic_query_operations_observed : []}
        {@const proofCoverage = packet.proof_coverage ?? {}}
        {@const proofScope = packet.proof_scope ?? {}}
        {@const workShape = packet.work_classification ?? {}}
        {@const languageTask = packet.language_task_alignment ?? {}}
        {@const candidateRecommendation = packet.pr_handoff?.candidate_recommendation ?? {}}
        {@const candidateComparison = packet.candidate_comparison ?? {}}
        {@const candidateCount = Number(candidateRecommendation.candidate_count ?? candidateComparison.candidate_count ?? 0)}
        {@const currentRunIsRecommended = candidateCurrentIsRecommended(candidateRecommendation, selectedRunId)}
        {@const rankedCandidates = Array.isArray(candidateComparison.candidates) ? candidateComparison.candidates : []}
        {@const recommendedRunId = candidateRecommendation.recommended_run_id || candidateComparison.recommended_run_id}
        {@const reviewChanged = diff.real_change_count ?? 0}
        {@const reviewPassed = packet.checks?.passed ?? 0}
        {@const reviewFailed = packet.checks?.failed ?? 0}
        {@const baseline = packet.checks?.baseline}
        {@const reviewModel = packet.built_by?.model || "A coder"}
        {#if selectedTitle}<h2 class="review-ask">{selectedTitle}</h2>{/if}
        <div class="verdict" data-status={packet.review_status}>
          <span class="verdict-tag">{STATUS_LABEL[packet.review_status] ?? packet.review_status}</span>
          <p class="verdict-headline" data-tone={reviewFailed > 0 || reviewChanged === 0 ? "attention" : "good"}>
            {#if reviewChanged === 0}This run produced no file change - there is nothing to ship.{:else if reviewFailed > 0}{reviewModel} changed {reviewChanged} file{reviewChanged === 1 ? "" : "s"}, but {reviewFailed} check{reviewFailed === 1 ? "" : "s"} did not pass - review carefully before shipping.{:else}{reviewModel} changed {reviewChanged} file{reviewChanged === 1 ? "" : "s"} and {reviewPassed} check{reviewPassed === 1 ? "" : "s"} passed.{/if}
          </p>
        </div>

        <div class="facts">
          <span>Built by <strong>{packet.built_by?.model || "a coder"}</strong></span>
          <span>{packet.summary?.turns ?? 0} turns</span>
          <span>{packet.checks?.passed ?? 0} checks passed{(packet.checks?.failed ?? 0) > 0 ? `, ${packet.checks.failed} failed` : ""}</span>
          {#if baseline?.passed > 0 || baseline?.failed > 0}<span>Before the change: {baseline.passed ?? 0} passed{baseline.failed > 0 ? `, ${baseline.failed} failed` : ""}</span>{/if}
          {#if workShape.task_class}<span>{String(workShape.complexity_tier || "unknown").replace(/_/g, " ")} {String(workShape.task_class).replace(/_/g, " ")}</span>{/if}
          {#if languageTask.task_corpus_id}<span title="Eval corpus: {languageTask.task_corpus_id}">Checked against a known eval set</span>{/if}
          {#if (languageTask.required_principal_review_gates ?? []).length > 0}<span>{languageTask.required_principal_review_gates.length} senior-review checks</span>{/if}
          <span>{diff.real_change_count ?? 0} files changed{(diff.generated_count ?? 0) > 0 ? ` + ${diff.generated_count} generated` : ""}</span>
          {#if (diff.language_pack_impact ?? []).length > 0}<span>{diff.language_pack_impact.length} stack{diff.language_pack_impact.length === 1 ? "" : "s"} touched</span>{/if}
          {#if (diff.artifact_count ?? 0) > 0}<span>{diff.artifact_count} build artifact{diff.artifact_count === 1 ? "" : "s"} folded</span>{/if}
          {#if packet.branch}<span class="facts-branch">{packet.branch}</span>{/if}
        </div>

        {#if candidateCount > 1}
          <section class="candidate-recommendation" data-state={currentRunIsRecommended ? "recommended" : "attention"} aria-label="Fleet candidate recommendation">
            <div class="candidate-head">
              <div>
                <strong>{currentRunIsRecommended ? "Fleet recommends this candidate" : "Fleet recommends a different candidate"}</strong>
                <p>{candidateLine(candidateRecommendation, selectedRunId)}</p>
              </div>
              <span>{candidateCount} candidates</span>
            </div>
            <div class="candidate-facts">
              <span>Current: {candidateRecommendation.current_run_id || selectedRunId}</span>
              <span>Score: {formatCandidateScore(candidateRecommendation.current_score)}</span>
              <span>Recommended: {candidateRecommendation.recommended_run_id || "not recorded"}</span>
              <span>Best score: {formatCandidateScore(candidateRecommendation.recommended_score)}</span>
            </div>
            {#if (candidateRecommendation.comparison_basis ?? []).length > 0}
              <p class="candidate-basis">Basis: {candidateRecommendation.comparison_basis.join(", ")}.</p>
            {/if}
            <p class="candidate-boundary">
              The ranking compares plain facts from the receipts and the diff. It has no opinion of its own, and it never ships anything - the call is yours.
            </p>
            {#if !currentRunIsRecommended && candidateRecommendation.recommended_run_id}
              <button type="button" class="mdx-btn small" onclick={() => openReview(candidateRecommendation.recommended_run_id)}>
                Open recommended run
              </button>
            {/if}
          </section>

          {#if rankedCandidates.length > 1}
            <section class="candidate-ranking" aria-label="Fleet candidate ranking">
              <div class="candidate-ranking-head">
                <strong>Candidate ranking</strong>
                <span>Deterministic comparison</span>
              </div>
              <ul class="candidate-list">
                {#each rankedCandidates as candidate (candidate.run_id)}
                  {@const candidateIsCurrent = candidate.run_id === selectedRunId}
                  {@const candidateIsRecommended = candidate.run_id === recommendedRunId}
                  <li class="candidate-row" data-current={candidateIsCurrent} data-recommended={candidateIsRecommended}>
                    <div class="candidate-row-main">
                      <span class="candidate-rank">#{candidate.rank}</span>
                      <span class="candidate-run">{candidate.run_id}</span>
                      {#if candidateIsRecommended}<span class="candidate-pill">recommended</span>{/if}
                      {#if candidateIsCurrent}<span class="candidate-pill muted">current</span>{/if}
                    </div>
                    <div class="candidate-row-facts">
                      <span>{candidate.status}</span>
                      <span>{formatCandidateScore(candidate.score)}</span>
                      <span>{candidate.checks_passed ?? 0} passed / {candidate.checks_failed ?? 0} failed</span>
                      <span>{candidate.related_tests_observed ? "related tests" : "no related-test receipt"}</span>
                      <span>{candidateDiffLine(candidate)}</span>
                    </div>
                    {#if !candidateIsCurrent}
                      <button type="button" class="section-fold" onclick={() => openReview(candidate.run_id)}>
                        Open
                      </button>
                    {/if}
                  </li>
                {/each}
              </ul>
              <p>Ranks compare what Forge can prove from receipts and the branch diff. Human review still decides whether the work is good enough.</p>
            </section>
          {/if}
        {/if}

        {#if proofCoverage.summary}
          <section class="proof-coverage" data-status={proofCoverage.status} aria-label="Selected proof coverage">
            <div class="proof-coverage-head">
              <strong>Proof coverage</strong>
              <span>{String(proofCoverage.status ?? "unknown").replace(/_/g, " ")}</span>
            </div>
            <p>{proofCoverage.summary}</p>
            {#if proofScope.summary}
              <p>{proofScope.summary}</p>
            {/if}
            {#if (proofCoverage.failed_checks ?? []).length > 0 || (proofCoverage.missing_checks ?? []).length > 0 || (proofScope.uncovered_language_packs ?? []).length > 0}
              <div class="proof-coverage-grid">
                {#if (proofCoverage.failed_checks ?? []).length > 0}
                  <span class="pc-bad">Failed: {proofCoverage.failed_checks.join(", ")}</span>
                {/if}
                {#if (proofCoverage.missing_checks ?? []).length > 0}
                  <span class="pc-bad">Missing: {proofCoverage.missing_checks.join(", ")}</span>
                {/if}
                {#if (proofScope.uncovered_language_packs ?? []).length > 0}
                  <span class="pc-warn">Uncovered stack: {proofScope.uncovered_language_packs.join(", ")}</span>
                {/if}
              </div>
            {/if}
            <details class="evidence-detail">
              <summary>Proof details</summary>
              <div class="proof-coverage-grid">
                <span>Source: {String(proofCoverage.selected_checks_source ?? "unknown_legacy").replace(/_/g, " ")}</span>
                <span>Match: {String(proofCoverage.match_policy ?? "exact_selected_command").replace(/_/g, " ")}</span>
                <span>Selected: {(proofCoverage.selected_checks ?? []).length > 0 ? proofCoverage.selected_checks.join(", ") : "none recorded"}</span>
                {#if (proofCoverage.satisfied_checks ?? []).length > 0}
                  <span>Satisfied: {proofCoverage.satisfied_checks.join(", ")}</span>
                {/if}
                {#if proofScope.status}
                  <span>Scope: {String(proofScope.status).replace(/_/g, " ")}</span>
                {/if}
              </div>
            </details>
          </section>
        {/if}

        {#if (packet.standards_cited ?? []).length > 0}
          <p class="standards">Grounded in your standards: {packet.standards_cited.join(", ")}</p>
        {/if}

        {#if (packet.checks?.names ?? []).length > 0}
          <ul class="checks">
            {#each packet.checks.names as check, i (i)}
              <li class="check" class:bad={!check.ok}>
                <span class="check-dot" aria-hidden="true"></span>
                <span class="check-body">
                  <code class="check-cmd">{checkCommand(check.name)}</code>
                  {#if checkTail(check.name)}
                    <details class="check-tail"><summary>output</summary><code>{checkTail(check.name)}</code></details>
                  {/if}
                </span>
              </li>
            {/each}
          </ul>
        {/if}

        {#if (reviewBrief.checklist ?? []).length > 0}
          <div class="review-brief" aria-label="Principal review checklist">
            <strong>What to check before you ship</strong>
            <ul>
              {#each reviewBrief.checklist.slice(0, 3) as item, i (i)}
                <li>{item.line}</li>
              {/each}
            </ul>
            {#if reviewBrief.checklist.length > 3}
              <p class="boundary-note">{reviewBrief.checklist.length - 3} more in the handoff.</p>
            {/if}
          </div>
          {#if reviewBrief.authority_note}
            <p class="boundary-note">{reviewBrief.authority_note}</p>
          {/if}
        {/if}

        {#if observedSemanticOps.length > 0 || repoIntel.principal_orientation_gate}
          <section class="semantic-evidence" aria-label="Where the agent looked">
            <div class="semantic-evidence-head">
              <strong>Where the agent looked</strong>
              <span>{repoIntel.related_tests_observed ? "related tests observed" : "related tests not recorded"}</span>
            </div>
            <div class="semantic-chips">
              {#if repoIntel.principal_orientation_gate}
                <span data-state={repoIntel.principal_orientation_gate.observed ? "satisfied" : "attention"}>
                  Gate {repoIntel.principal_orientation_gate.observed ? "observed" : "missing"}
                </span>
              {/if}
              {#each observedSemanticOps as op (op)}
                <span data-state={op === "related_tests" ? "satisfied" : "open"}>{String(op).replace(/_/g, " ")}</span>
              {/each}
            </div>
            <p class="boundary-note">This is what the agent looked at while it worked. It helps you review; it does not decide anything or mark the checks passed.</p>
          </section>
        {/if}

        {#if (diff.language_pack_impact ?? []).length > 0}
          <section class="artifact-fold" aria-label="What areas this touches">
            <div class="artifact-fold-head">
              <strong>What areas this touches</strong>
              <span>Worked out from the files that changed - it helps you review, nothing more.</span>
            </div>
            <ul class="artifact-summary">
              {#each diff.language_pack_impact as item, i (item.language_pack_id ?? i)}
                <li class="artifact-row">
                  <span class="artifact-reason">{item.language_pack_id}</span>
                  <span class="artifact-count">{item.file_count} file{item.file_count === 1 ? "" : "s"} · +{item.added ?? 0} / -{item.removed ?? 0}{(item.generated_count ?? 0) > 0 ? ` · ${item.generated_count} generated` : ""}</span>
                  {#if (item.sample_paths ?? []).length > 0}
                    <span class="artifact-sample">{item.sample_paths.slice(0, 2).join(", ")}</span>
                  {/if}
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        {#if (diff.artifact_summary ?? []).length > 0}
          <section class="artifact-fold" aria-label="Generated files, tucked away">
            <div class="artifact-fold-head">
              <strong>Generated files, tucked away</strong>
              <span>{diff.artifact_count ?? 0} artifact file{(diff.artifact_count ?? 0) === 1 ? "" : "s"} kept out of the first-read diff</span>
            </div>
            <ul class="artifact-summary">
              {#each diff.artifact_summary as item, i (item.reason ?? i)}
                <li class="artifact-row">
                  <span class="artifact-reason">{item.reason}</span>
                  <span class="artifact-count">{item.count} file{item.count === 1 ? "" : "s"} · +{item.added ?? 0} / -{item.removed ?? 0}</span>
                  {#if (item.sample_paths ?? []).length > 0}
                    <span class="artifact-sample">{item.sample_paths.slice(0, 2).join(", ")}</span>
                  {/if}
                </li>
              {/each}
            </ul>
          </section>
        {/if}

        {#if (diff.sections ?? []).length === 0}
          <p class="quiet">This run committed no change to read - it stopped before producing a diff.</p>
        {:else}
          <div class="diff-toolbar">
            <div class="diff-find">
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="11" cy="11" r="7"/><path d="m21 21-4.3-4.3"/></svg>
              <input
                type="search"
                placeholder="Find in this change"
                bind:value={findText}
                aria-label="Find in this change"
                onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); stepFind(e.shiftKey ? -1 : 1); } }}
              />
              {#if findQuery}
                <span class="diff-find-count">{findHits.length ? `${(((findIndex % findHits.length) + findHits.length) % findHits.length) + 1} of ${findHits.length}` : "no matches"}</span>
                <button type="button" class="diff-find-nav" onclick={() => stepFind(-1)} disabled={!findHits.length} aria-label="Previous match">&uarr;</button>
                <button type="button" class="diff-find-nav" onclick={() => stepFind(1)} disabled={!findHits.length} aria-label="Next match">&darr;</button>
              {/if}
            </div>
            <div class="diff-mode" role="group" aria-label="Diff layout">
              <button type="button" class:on={diffMode === "unified"} onclick={() => (diffMode = "unified")}>Unified</button>
              <button type="button" class:on={diffMode === "split"} onclick={() => (diffMode = "split")}>Split</button>
            </div>
          </div>
          <div class="sections">
            {#each diff.sections as section (section.section)}
              {@const folded = section.generated && !openGenerated}
              <section class="diff-section" class:generated={section.generated}>
                <div class="section-head">
                  <strong>{section.label}</strong>
                  <span class="section-count">{section.file_count} {section.file_count === 1 ? "file" : "files"} · +{section.added} / -{section.removed}</span>
                  {#if section.generated}
                    <button type="button" class="section-fold" onclick={() => (openGenerated = !openGenerated)}>
                      {openGenerated ? "hide churn" : "show generated churn"}
                    </button>
                  {/if}
                </div>
                {#if !folded}
                  <ul class="file-list">
                    {#each section.files as file (file.path)}
                      <li class="file">
                        <button type="button" class="file-head" onclick={() => toggleFile(file.path)}>
                          <span class="file-path">{file.path}</span>
                          {#if signalsByPath[file.path]?.confidence === "needs_attention"}
                            <span class="file-sig warn" title="{signalsByPath[file.path].errorSteps} recorded step{signalsByPath[file.path].errorSteps === 1 ? "" : "s"} hit an error near this file">look here first</span>
                          {:else if signalsByPath[file.path]?.confidence === "checked"}
                            <span class="file-sig ok" title="Checks touched this file while it was being written">checked</span>
                          {/if}
                          <span class="file-stat"><span class="add">+{file.added}</span> <span class="del">-{file.removed}</span></span>
                        </button>
                        {#if openFiles[file.path] && patchByPath[file.path]}
                          {#if diffMode === "split"}
                            <pre class="patch split">{#each renderedByPath[file.path] ?? [] as line, li (li)}{#if line.kind === "hunk" || line.kind === "meta"}<span class="pl" data-k={line.kind} id={`dl-${file.path}-${li}`}><span class="pl-side pl-side-full">{#each line.tokens as tok, ti (ti)}<span class="t-{tok.t}">{tok.v}</span>{/each}</span></span>{:else}<span class="pl" data-k={line.kind} class:hit={lineIsHit(file.path, li)} class:active={activeHit && activeHit.path === file.path && activeHit.li === li} id={`dl-${file.path}-${li}`}><span class="pl-side pl-side-old" data-empty={line.oldLine == null}><span class="pl-ln">{line.oldLine ?? ""}</span><span class="pl-code">{#if line.kind !== "add"}{#each line.tokens as tok, ti (ti)}<span class="t-{tok.t}">{tok.v}</span>{/each}{/if}</span></span><span class="pl-side pl-side-new" data-empty={line.newLine == null}><span class="pl-ln">{line.newLine ?? ""}</span><span class="pl-code">{#if line.kind !== "del"}{#each line.tokens as tok, ti (ti)}<span class="t-{tok.t}">{tok.v}</span>{/each}{/if}</span></span></span>{/if}{/each}</pre>
                          {:else}
                            <pre class="patch">{#each renderedByPath[file.path] ?? [] as line, li (li)}<span class="pl" data-k={line.kind} class:hit={lineIsHit(file.path, li)} class:active={activeHit && activeHit.path === file.path && activeHit.li === li} id={`dl-${file.path}-${li}`}><span class="pl-ln pl-ln-old">{line.oldLine ?? ""}</span><span class="pl-ln pl-ln-new">{line.newLine ?? ""}</span><span class="pl-marker">{line.marker || " "}</span>{#each line.tokens as tok, ti (ti)}<span class="t-{tok.t}">{tok.v}</span>{/each}</span>{/each}</pre>
                          {/if}
                        {/if}
                      </li>
                    {/each}
                  </ul>
                {/if}
              </section>
            {/each}
          </div>
        {/if}

        <div class="panel">
          {#if packet.panel}
            {@const pn = packet.panel}
            <div class="panel-head">
              <strong>Review panel</strong>
              <span class="panel-consensus" data-c={pn.consensus}>{pn.consensus.replace(/_/g, " ")}</span>
              <span class="panel-confidence">{pn.confidence} confidence</span>
              <button type="button" class="section-fold" onclick={convenePanel} disabled={convening}>
                {convening ? "convening..." : "run again"}
              </button>
            </div>
            <ul class="panel-members">
              {#each pn.members as m, i (i)}
                <li class="panel-member">
                  <span class="member-lens">{m.stance}</span>
                  <span class="member-model">{m.model}</span>
                  <span class="member-verdict" data-v={m.verdict}>{VERDICT_LABEL[m.verdict] ?? m.verdict}</span>
                  {#if m.concern}<p class="member-concern">{m.concern}</p>{/if}
                </li>
              {/each}
            </ul>
            {#if (pn.dissent ?? []).length > 0}
              <p class="panel-dissent">Dissent: {pn.dissent.join(", ")} did not agree with the consensus - their view is kept, not overruled.</p>
            {/if}
            {#if pn.blind_spots}
              <div class="panel-blind">
                <span class="blind-tag">Blind spots one lens caught</span>
                <pre>{pn.blind_spots}</pre>
              </div>
            {/if}
          {:else}
            <button type="button" class="mdx-btn small" onclick={convenePanel} disabled={convening}>
              {convening ? "Convening the panel..." : "Convene a review panel"}
            </button>
            <span class="panel-hint">Diverse models read the change cold from different lenses; their agreement and dissent are recorded.</span>
            {#if panelFlash}<p class="action-flash bad">{panelFlash.line}</p>{/if}
          {/if}
        </div>

        <section class="pr-draft" aria-label="Pull request handoff">
          <div class="pr-draft-head">
            <div>
              <strong>PR handoff</strong>
              <p>Prepare the title, body, checklist, proof summary, and receipt trail for the source host your team already uses.</p>
            </div>
            <span>Dry run only</span>
          </div>
          <div class="pr-draft-controls">
            <label>
              Host
              <select bind:value={prDraftHost}>
                <option value="github">GitHub</option>
                <option value="bitbucket">Bitbucket</option>
                <option value="generic">Generic</option>
              </select>
            </label>
            <label>
              Base branch
              <input type="text" bind:value={prDraftBase} placeholder="main" />
            </label>
            <button type="button" class="mdx-btn small" onclick={preparePrDraft} disabled={prDrafting || !selectedRunId}>
              {prDrafting ? "Preparing..." : "Prepare PR draft"}
            </button>
            <button type="button" class="mdx-btn small ghost" onclick={() => loadSourceHostReadiness()} disabled={sourceHostBusy || !selectedRunId}>
              {sourceHostBusy ? "Checking..." : "Check readiness"}
            </button>
          </div>
          <p class="pr-draft-boundary">This records a handoff artifact. Forge does not push the branch, open a PR, approve work, deploy, or write to production.</p>
          {#if sourceHostReadiness}
            <div class="source-host-readiness" data-status={sourceHostReadiness.readiness_status ?? sourceHostReadiness.status}>
              <div class="source-host-line">
                <strong>{sourceHostReadiness.readiness_status ? String(sourceHostReadiness.readiness_status).replace(/_/g, " ") : "Source-host readiness unavailable"}</strong>
                {#if sourceHostReadiness.source_host}<span>{sourceHostReadiness.source_host}</span>{/if}
              </div>
              <p>{sourceHostReadiness.safe_next_move ?? sourceHostReadiness.reason}</p>
              {#if (sourceHostReadiness.blocked_reasons ?? []).length > 0}
                <p class="pc-bad pr-held">Held: {sourceHostReadiness.blocked_reasons.join(", ")}</p>
              {/if}
              <details class="evidence-detail">
                <summary>Readiness details</summary>
                <div class="source-host-facts">
                  <span>Dry draft: {sourceHostReadiness.ready_for_dry_pr_draft ? "ready" : "blocked"}</span>
                  <span>Live delivery: {sourceHostReadiness.ready_for_live_source_host_delivery ? "ready" : "held"}</span>
                  <span>Connection to your host: {sourceHostReadiness.source_host_credentials_present ? "ready" : "not set up yet"}</span>
                  {#if (sourceHostReadiness.credential_sources_checked ?? []).length > 0}
                    <span>Checked: {sourceHostReadiness.credential_sources_checked.join(", ")}</span>
                  {/if}
                </div>
              </details>
            </div>
          {/if}
          {#if prDraftFlash}
            <p class="action-flash" class:bad={!prDraftFlash.ok}>{prDraftFlash.line}</p>
          {/if}
          {#if prDraft}
            <div class="pr-draft-result">
              <div class="pr-draft-actions">
                <button type="button" class="mdx-btn small" onclick={() => copyDraft(prDraft.draft_title ?? "")}>Copy title</button>
                <button type="button" class="mdx-btn small" onclick={() => copyDraft(prDraft.draft_body_markdown ?? "")}>Copy description</button>
                {#if compareUrl(prDraft)}
                  <a class="mdx-btn small primary" href={compareUrl(prDraft)} target="_blank" rel="noreferrer noopener">Open compare on {prDraftHost === "bitbucket" ? "Bitbucket" : "GitHub"}</a>
                {/if}
                {#if copiedNote}<span class="copied-note" role="status">{copiedNote}</span>{/if}
              </div>
              <label>
                Title
                <input type="text" readonly value={prDraft.draft_title ?? ""} />
              </label>
              <label>
                Body
                <textarea readonly rows="12" value={prDraft.draft_body_markdown ?? ""}></textarea>
              </label>
              <details class="evidence-detail">
                <summary>Receipt trail</summary>
                <div class="pr-draft-facts">
                  <span>{prDraft.source_host_plan?.provider ?? prDraft.source_host} plan</span>
                  <span>Receipt: {prDraft.pr_handoff_receipt_id}</span>
                  <span>Policy: {prDraft.policy_decision_id}</span>
                </div>
              </details>
            </div>
          {/if}
        </section>

        <div class="decide">
          <p class="next-line">{packet.next_move}</p>
          {#if packet.review_status === "shipped"}
            <p class="shipped-note">This change shipped{packet.ship?.reason ? `: "${packet.ship.reason}"` : "."}</p>
          {:else}
            {#if packet.review_status === "ready_for_review"}
              <div class="ship-reasons" role="group" aria-label="Quick ship reasons">
                {#each SHIP_REASONS as reason}
                  <button type="button" class="ship-reason-chip" onclick={() => (shipReason = reason)}>{reason}</button>
                {/each}
              </div>
              <div class="verb">
                <input
                  type="text"
                  bind:this={shipInput}
                  bind:value={shipReason}
                  placeholder="Why this ships (your words), or pick one above"
                  aria-label="Ship reason"
                />
                <button type="button" class="mdx-btn primary small" onclick={ship} disabled={acting !== "" || !shipReason.trim()} title="Ship it (A)">
                  {acting === "ship" ? "Shipping..." : "Ship it"}
                </button>
              </div>
            {/if}
            <div class="verb">
              <input
                type="text"
                bind:this={reviseInput}
                bind:value={reviseComment}
                placeholder="What should change (a new run picks up this branch)"
                aria-label="Revision request"
              />
              <button type="button" class="mdx-btn small" onclick={askRevision} disabled={acting !== "" || !reviseComment.trim()} title="Ask a revision (R)">
                {acting === "revise" ? "Starting..." : "Ask a revision"}
              </button>
            </div>
            <button type="button" class="mdx-btn small ghost" onclick={stopRun} disabled={acting !== ""}>
              {acting === "stop" ? "Stopping..." : "Stop this run"}
            </button>
          {/if}
          {#if actionFlash}
            <p class="action-flash" class:bad={!actionFlash.ok}>{actionFlash.line}{#if actionFlash.changelog}&nbsp;<a href="/changelog">On the record →</a>{/if}{#if !actionFlash.ok}&nbsp;<button type="button" class="report-this" onclick={() => window.dispatchEvent(new CustomEvent("mdx:report", { detail: { note: `While deciding on ${selectedRunId}: ${actionFlash.line}` } }))}>Report this</button>{/if}</p>
          {/if}
        </div>
      {/if}
    </article>
  </div>
  {/if}
</ForgeView>

<style>
  .review-body {
    display: grid;
    grid-template-columns: 240px minmax(0, 1fr);
    gap: 18px;
    min-height: 0;
  }
  /* Empty state: one calm centered card, not the two-pane split that read as
     a contradiction (nothing here / pick on the left). */
  .review-empty {
    display: grid;
    justify-items: center;
    text-align: center;
    gap: 12px;
    max-width: 520px;
    margin: 48px auto;
    padding: 36px 28px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg, 12px);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }
  .review-empty h2 {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 16px;
    font-weight: 650;
  }
  .review-empty p {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 13px;
    line-height: 1.5;
  }
  .review-empty .mdx-btn {
    margin-top: 4px;
  }
  .review-queue {
    display: grid;
    gap: 6px;
    align-content: start;
  }
  /* Queue controls appear once the queue is worth filtering. */
  .rq-controls {
    display: grid;
    gap: 8px;
    margin-bottom: 4px;
  }
  .rq-search {
    height: 32px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    padding: 0 10px;
    font: inherit;
    font-size: 12.5px;
  }
  .rq-search:focus-visible {
    outline: none;
    border-color: var(--mdx-accent-primary);
  }
  .rq-filters {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .rq-filters button {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill, 999px);
    background: transparent;
    color: var(--mdx-text-muted);
    padding: 2px 9px;
    font: inherit;
    font-size: 11.5px;
    cursor: pointer;
  }
  .rq-filters button.active {
    border-color: var(--mdx-accent-primary);
    color: var(--mdx-text-primary);
    background: color-mix(in srgb, var(--mdx-accent-primary) 12%, transparent);
  }
  .rq-none {
    margin: 8px 2px;
    color: var(--mdx-text-muted);
    font-size: 12.5px;
  }
  .queue-row {
    display: flex;
    flex-direction: column;
    gap: 3px;
    text-align: left;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    padding: 8px 10px;
    cursor: pointer;
  }
  .queue-row.active {
    border-color: var(--mdx-accent-primary);
    background: color-mix(in srgb, var(--mdx-accent-primary) 8%, var(--mdx-surface-base));
  }
  .queue-title {
    font-size: 13px;
    font-weight: 550;
    color: var(--mdx-text-primary);
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .review-ask {
    margin: 0 0 12px;
    font-size: 18px;
    font-weight: 600;
    line-height: 1.3;
    color: var(--mdx-text);
  }
  .queue-status {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--mdx-text-secondary);
  }
  .queue-status[data-status="done"],
  .queue-status[data-status="finished"] { color: var(--mdx-accent-success); }
  .queue-status[data-status="cannot_proceed"],
  .queue-status[data-status="error"],
  .queue-status[data-status="budget_exhausted"] { color: var(--mdx-accent-error); }
  .queue-status[data-status="stopped"] { color: var(--mdx-text-muted); }

  .review-main {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    padding: 18px 20px;
    display: grid;
    gap: 14px;
    align-content: start;
    min-height: 0;
  }
  .verdict {
    display: grid;
    gap: 4px;
  }
  .verdict-tag {
    justify-self: start;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 600;
    color: var(--mdx-text-secondary);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    padding: 1px 8px;
  }
  .verdict[data-status="ready_for_review"] .verdict-tag { color: var(--mdx-accent-primary); border-color: color-mix(in srgb, var(--mdx-accent-primary) 40%, var(--mdx-border-subtle)); }
  .verdict[data-status="needs_attention"] .verdict-tag { color: var(--mdx-accent-warning); border-color: color-mix(in srgb, var(--mdx-accent-warning) 42%, var(--mdx-border-subtle)); }
  .verdict-headline {
    margin: 8px 0 2px;
    font-size: 16px;
    font-weight: 650;
    line-height: 1.4;
    color: var(--mdx-text-primary);
  }
  .verdict-headline[data-tone="attention"] {
    color: var(--mdx-tone-warn-text, var(--mdx-accent-warning));
  }
  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
  }
  .facts-branch {
    font-family: var(--mdx-font-mono, monospace);
    font-size: 11.5px;
  }
  .standards {
    margin: 0;
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
  }
  .candidate-recommendation {
    display: grid;
    gap: 8px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-surface-base) 78%, transparent);
    padding: 11px 12px;
  }
  .candidate-recommendation[data-state="recommended"] {
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 35%, var(--mdx-border-subtle));
  }
  .candidate-recommendation[data-state="attention"] {
    border-color: color-mix(in srgb, var(--mdx-accent-warning, #c0392b) 44%, var(--mdx-border-subtle));
  }
  .candidate-head {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: 12px;
  }
  .candidate-head strong {
    font-size: 13px;
    color: var(--mdx-text-primary);
  }
  .candidate-head p,
  .candidate-basis,
  .candidate-boundary {
    margin: 3px 0 0;
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
  }
  .candidate-head > span {
    flex: 0 0 auto;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    padding: 2px 7px;
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--mdx-text-tertiary);
  }
  .candidate-facts {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 14px;
    font-family: var(--mdx-font-mono, monospace);
    font-size: 11.5px;
    color: var(--mdx-text-tertiary);
  }
  .candidate-boundary {
    color: var(--mdx-text-tertiary);
  }
  .candidate-ranking {
    display: grid;
    gap: 8px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-surface-base) 70%, transparent);
    padding: 10px 12px;
  }
  .candidate-ranking-head {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    align-items: baseline;
  }
  .candidate-ranking-head strong {
    font-size: 13px;
    color: var(--mdx-text-primary);
  }
  .candidate-ranking-head span {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--mdx-text-tertiary);
  }
  .candidate-list {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .candidate-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 6px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    padding: 8px 10px;
  }
  .candidate-row[data-recommended="true"] {
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 34%, var(--mdx-border-subtle));
  }
  .candidate-row[data-current="true"] {
    background: color-mix(in srgb, var(--mdx-accent-primary) 7%, var(--mdx-surface-raised));
  }
  .candidate-row-main,
  .candidate-row-facts {
    display: flex;
    flex-wrap: wrap;
    gap: 7px 10px;
    align-items: baseline;
    min-width: 0;
  }
  .candidate-rank {
    font-family: var(--mdx-font-mono, monospace);
    color: var(--mdx-text-primary);
    font-size: 12px;
    font-weight: 600;
  }
  .candidate-run {
    font-family: var(--mdx-font-mono, monospace);
    color: var(--mdx-text-primary);
    font-size: 11.5px;
    overflow-wrap: anywhere;
  }
  .candidate-pill {
    border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 40%, var(--mdx-border-subtle));
    border-radius: var(--mdx-radius-sm);
    color: var(--mdx-accent-primary);
    font-size: 10.5px;
    padding: 1px 6px;
  }
  .candidate-pill.muted {
    color: var(--mdx-text-tertiary);
    border-color: var(--mdx-border-subtle);
  }
  .candidate-row-facts {
    grid-column: 1 / -1;
    font-family: var(--mdx-font-mono, monospace);
    font-size: 11.5px;
    color: var(--mdx-text-tertiary);
  }
  .candidate-ranking p {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
  }
  .semantic-evidence {
    display: grid;
    gap: 8px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-surface-base) 74%, transparent);
    padding: 10px 12px;
  }
  .semantic-evidence-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }
  .semantic-evidence-head strong {
    font-size: 13px;
    color: var(--mdx-text-primary);
  }
  .semantic-evidence-head span {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--mdx-text-tertiary);
    text-align: right;
  }
  .semantic-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
  }
  .semantic-chips span {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    padding: 2px 7px;
    font-size: 11.5px;
    color: var(--mdx-text-secondary);
    background: var(--mdx-surface-raised);
  }
  .semantic-chips span[data-state="satisfied"] {
    color: var(--mdx-accent-primary);
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 36%, var(--mdx-border-subtle));
  }
  .semantic-chips span[data-state="attention"] {
    color: var(--mdx-accent-warning, #c0392b);
    border-color: color-mix(in srgb, var(--mdx-accent-warning, #c0392b) 40%, var(--mdx-border-subtle));
  }
  .semantic-evidence p {
    margin: 0;
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
  }
  .proof-coverage {
    display: grid;
    gap: 6px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-surface-base) 74%, transparent);
    padding: 10px 12px;
  }
  .proof-coverage[data-status="satisfied"] {
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 35%, var(--mdx-border-subtle));
  }
  .proof-coverage[data-status="missing"],
  .proof-coverage[data-status="failed"],
  .proof-coverage[data-status="not_recorded"] {
    border-color: color-mix(in srgb, var(--mdx-accent-warning, #c0392b) 40%, var(--mdx-border-subtle));
  }
  .proof-coverage-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }
  .proof-coverage-head strong {
    font-size: 13px;
    color: var(--mdx-text-primary);
  }
  .proof-coverage-head span {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--mdx-text-tertiary);
  }
  .proof-coverage p {
    margin: 0;
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
  }
  .proof-coverage-grid {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 14px;
    font-family: var(--mdx-font-mono, monospace);
    font-size: 11.5px;
    color: var(--mdx-text-tertiary);
  }
  /* Problem signals stay visible and legible; reference metadata goes quiet. */
  .proof-coverage-grid .pc-bad {
    font-family: inherit;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--mdx-tone-danger-text, var(--mdx-accent-error));
  }
  .proof-coverage-grid .pc-warn {
    font-family: inherit;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--mdx-accent-warning, var(--mdx-text-secondary));
  }
  /* Raw evidence one tap away, not in the operator's face. */
  .evidence-detail {
    margin-top: 2px;
  }
  .evidence-detail > summary {
    cursor: pointer;
    color: var(--mdx-text-muted);
    font-size: 11.5px;
  }
  .evidence-detail[open] > summary {
    margin-bottom: 7px;
  }
  .boundary-note {
    margin: 4px 0 0;
    color: var(--mdx-text-tertiary);
    font-size: 11px;
    line-height: 1.4;
  }
  .review-brief {
    display: grid;
    gap: 5px;
  }
  .review-brief > strong {
    color: var(--mdx-text-primary);
    font-size: 12.5px;
  }
  .review-brief ul {
    margin: 0;
    padding-left: 16px;
    display: grid;
    gap: 3px;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.45;
  }
  .review-brief .boundary-note {
    margin: 1px 0 0;
  }
  .checks {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--mdx-text-secondary);
  }
  .check-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--mdx-accent-primary);
  }
  .check.bad .check-dot { background: var(--mdx-accent-error, #c0392b); }

  .artifact-fold {
    display: grid;
    gap: 8px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-surface-base) 74%, transparent);
    padding: 10px 12px;
  }
  .artifact-fold-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }
  .artifact-fold-head strong {
    font-size: 13px;
    color: var(--mdx-text-primary);
  }
  .artifact-fold-head span {
    font-size: 11.5px;
    color: var(--mdx-text-tertiary);
    text-align: right;
  }
  .artifact-summary {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .artifact-row {
    display: grid;
    grid-template-columns: minmax(140px, 1.2fr) auto minmax(0, 1.4fr);
    gap: 10px;
    align-items: baseline;
    font-size: 12px;
    color: var(--mdx-text-secondary);
  }
  .artifact-reason {
    color: var(--mdx-text-primary);
  }
  .artifact-count {
    white-space: nowrap;
    font-family: var(--mdx-font-mono, monospace);
    font-size: 11.5px;
    color: var(--mdx-text-tertiary);
  }
  .artifact-sample {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mdx-font-mono, monospace);
    font-size: 11.5px;
    color: var(--mdx-text-tertiary);
  }

  .sections { display: grid; gap: 10px; }
  .diff-section {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    overflow: hidden;
  }
  .diff-section.generated { opacity: 0.8; }
  .section-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 8px 12px;
    background: var(--mdx-surface-base);
  }
  .section-head strong { font-size: 13px; }
  .section-count { font-size: 11.5px; color: var(--mdx-text-tertiary); }
  .section-fold {
    margin-left: auto;
    border: none;
    background: transparent;
    color: var(--mdx-accent-primary);
    font-size: 11.5px;
    cursor: pointer;
  }
  .file-list { margin: 0; padding: 0; list-style: none; }
  .file { border-top: 1px solid var(--mdx-border-subtle); }
  .file-head {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    border: none;
    background: transparent;
    padding: 7px 12px;
    cursor: pointer;
    text-align: left;
  }
  .file-path {
    font-family: var(--mdx-font-mono, monospace);
    font-size: 12px;
    color: var(--mdx-text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .file-stat { font-size: 11.5px; white-space: nowrap; }
  .add { color: var(--mdx-accent-success); }
  .del { color: var(--mdx-accent-error); }
  .ship-reasons { display: flex; flex-wrap: wrap; gap: 6px; margin: 0 0 8px; }
  .ship-reason-chip { font: inherit; font-size: 12px; padding: 4px 11px; border-radius: var(--mdx-radius-full, 999px); border: 1px solid var(--mdx-border-subtle); background: var(--mdx-surface-base); color: var(--mdx-text-secondary); cursor: pointer; }
  .ship-reason-chip:hover { border-color: var(--mdx-border-default); color: var(--mdx-text-primary); background: var(--mdx-surface-raised); }
  .diff-toolbar { display: flex; flex-wrap: wrap; gap: 10px; align-items: center; justify-content: space-between; margin: 0 0 12px; }
  .diff-find { display: flex; align-items: center; gap: 6px; flex: 1; min-width: 220px; padding: 4px 10px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-full, 999px); background: var(--mdx-surface-base); }
  .diff-find svg { width: 15px; height: 15px; flex: none; color: var(--mdx-text-muted); }
  .diff-find input { flex: 1; min-width: 0; border: none; background: none; font: inherit; font-size: 13px; color: var(--mdx-text-primary); outline: none; }
  .diff-find-count { flex: none; font-size: 12px; color: var(--mdx-text-muted); white-space: nowrap; }
  .diff-find-nav { flex: none; border: none; background: none; color: var(--mdx-text-secondary); cursor: pointer; font-size: 13px; padding: 2px 5px; border-radius: 6px; }
  .diff-find-nav:hover:not(:disabled) { background: var(--mdx-surface-raised); }
  .diff-find-nav:disabled { opacity: 0.4; cursor: default; }
  .diff-mode { display: inline-flex; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-full, 999px); overflow: hidden; flex: none; }
  .diff-mode button { border: none; background: none; font: inherit; font-size: 12.5px; padding: 5px 14px; color: var(--mdx-text-muted); cursor: pointer; }
  .diff-mode button.on { background: var(--mdx-accent-primary); color: var(--mdx-on-accent, #fff); }
  .patch {
    margin: 0;
    padding: 10px 12px;
    overflow-x: auto;
    font-family: var(--mdx-font-mono, monospace);
    font-size: 12.5px;
    line-height: 1.6;
    letter-spacing: -0.01em;
    background: var(--mdx-surface-base);
    border-top: 1px solid var(--mdx-border-subtle);
  }
  .pl { display: block; white-space: pre; padding-left: 0; border-left: 2px solid transparent; }
  .pl-ln { display: inline-block; width: 4ch; padding: 0 6px 0 0; text-align: right; color: var(--mdx-text-faint); user-select: none; }
  .pl-ln-old { border-right: 1px solid var(--mdx-border-subtle); }
  .pl-ln-new { padding-left: 6px; }
  .pl-marker { display: inline-block; width: 1ch; padding-left: 8px; color: var(--mdx-text-faint); user-select: none; }
  .pl[data-k="add"] { background: var(--mdx-diff-add-bg); border-left-color: var(--mdx-diff-add-gutter); }
  .pl[data-k="del"] { background: var(--mdx-diff-del-bg); border-left-color: var(--mdx-diff-del-gutter); }
  .pl[data-k="add"] .pl-marker { color: var(--mdx-diff-add-gutter); }
  .pl[data-k="del"] .pl-marker { color: var(--mdx-diff-del-gutter); }
  .pl[data-k="hunk"] { color: var(--mdx-accent-primary); }
  .pl[data-k="hunk"] .t-hunk { color: var(--mdx-accent-primary); }
  .pl[data-k="hunk"] .pl-ln, .pl[data-k="meta"] .pl-ln { border-right-color: transparent; }
  .pl[data-k="meta"] .t-meta { color: var(--mdx-text-tertiary); }
  /* Find: every matching row gets a quiet highlight; the active one is stronger. */
  .pl.hit { background: color-mix(in srgb, var(--mdx-accent-warning, #b45309) 16%, transparent); }
  .pl.active { background: color-mix(in srgb, var(--mdx-accent-warning, #b45309) 34%, transparent); outline: 1px solid color-mix(in srgb, var(--mdx-accent-warning, #b45309) 55%, transparent); }
  /* Split view: two aligned columns, old on the left, new on the right. */
  .patch.split .pl { display: flex; }
  .patch.split .pl-side { flex: 1 1 50%; min-width: 0; display: flex; }
  .patch.split .pl-side-old { border-right: 1px solid var(--mdx-border-subtle); }
  .patch.split .pl-side-full { flex-basis: 100%; }
  .patch.split .pl-side[data-empty="true"] { background: color-mix(in srgb, var(--mdx-text-muted) 6%, transparent); }
  .patch.split .pl-ln { flex: none; }
  .patch.split .pl-code { flex: 1; min-width: 0; padding-left: 8px; white-space: pre; }
  /* Syntax tokens - the premium, legible code coloring. */
  .t-keyword { color: var(--mdx-code-keyword); }
  .t-string { color: var(--mdx-code-string); }
  .t-comment { color: var(--mdx-code-comment); font-style: italic; }
  .t-number, .t-value { color: var(--mdx-code-number); }
  .t-type { color: var(--mdx-code-type); }
  .t-fn { color: var(--mdx-code-fn); }
  .t-text, .t-punct { color: var(--mdx-text-secondary); }

  .panel {
    display: grid;
    gap: 8px;
    border-top: 1px solid var(--mdx-border-subtle);
    padding-top: 12px;
  }
  .panel-head { display: flex; align-items: baseline; gap: 10px; }
  .panel-head strong { font-size: 13px; }
  .panel-consensus {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    padding: 1px 6px;
  }
  .panel-consensus[data-c="ready"] { color: var(--mdx-accent-primary); border-color: color-mix(in srgb, var(--mdx-accent-primary) 40%, var(--mdx-border-subtle)); }
  .panel-consensus[data-c="needs_work"] { color: var(--mdx-accent-warning, #c0392b); }
  .panel-confidence { font-size: 11.5px; color: var(--mdx-text-tertiary); }
  .panel-members { margin: 0; padding: 0; list-style: none; display: grid; gap: 8px; }
  .panel-member {
    display: grid;
    grid-template-columns: auto auto auto;
    justify-content: start;
    gap: 8px;
    align-items: baseline;
  }
  .member-lens { font-size: 10px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--mdx-text-secondary); }
  .member-model { font-family: var(--mdx-font-mono, monospace); font-size: 11.5px; color: var(--mdx-text-primary); }
  .member-verdict { font-size: 11px; }
  .member-verdict[data-v="ready"] { color: var(--mdx-accent-primary); }
  .member-verdict[data-v="needs work"] { color: var(--mdx-accent-warning, #c0392b); }
  .member-concern { grid-column: 1 / -1; margin: 2px 0 0; font-size: 12px; color: var(--mdx-text-secondary); }
  .panel-dissent { margin: 0; font-size: 12.5px; color: var(--mdx-text-primary); }
  .panel-blind { display: grid; gap: 4px; }
  .blind-tag { font-size: 11px; color: var(--mdx-text-secondary); }
  .panel-blind pre { margin: 0; padding: 8px 10px; background: var(--mdx-surface-base); border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-sm); font-size: 12px; white-space: pre-wrap; }
  .panel-hint { font-size: 12px; color: var(--mdx-text-secondary); }

  .pr-draft {
    display: grid;
    gap: 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-surface-base) 74%, transparent);
    padding: 12px;
  }
  .pr-draft-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
  }
  .pr-draft-head strong {
    font-size: 13px;
    color: var(--mdx-text-primary);
  }
  .pr-draft-head p,
  .pr-draft-boundary {
    margin: 3px 0 0;
    color: var(--mdx-text-tertiary);
    font-size: 11px;
    line-height: 1.4;
  }
  .pr-draft-head > span {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    padding: 2px 7px;
    color: var(--mdx-text-tertiary);
    font-size: 10.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    white-space: nowrap;
  }
  .pr-draft-controls {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: end;
  }
  .pr-draft label,
  .pr-draft-result label {
    display: grid;
    gap: 4px;
    color: var(--mdx-text-secondary);
    font-size: 11.5px;
  }
  .pr-draft select,
  .pr-draft input,
  .pr-draft textarea {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 12.5px;
  }
  .pr-draft select,
  .pr-draft input {
    height: 32px;
    padding: 0 9px;
  }
  .pr-draft-result {
    display: grid;
    gap: 8px;
  }
  .pr-draft-actions { display: flex; align-items: center; gap: 8px; margin: 0 0 10px; flex-wrap: wrap; }
  .copied-note { font-size: 12px; color: var(--mdx-accent-success); }
  .source-host-readiness {
    display: grid;
    gap: 6px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    padding: 9px 10px;
  }
  .source-host-readiness[data-status="READY_FOR_OPERATOR_PR_ACTION"] {
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 40%, var(--mdx-border-subtle));
  }
  .source-host-line,
  .source-host-facts {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 14px;
    align-items: baseline;
  }
  .source-host-line strong {
    color: var(--mdx-text-primary);
    font-size: 12.5px;
  }
  .source-host-line span {
    color: var(--mdx-text-tertiary);
    font-family: var(--mdx-font-mono, monospace);
    font-size: 11.5px;
  }
  .source-host-readiness p {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.45;
  }
  .source-host-facts {
    color: var(--mdx-text-tertiary);
    font-family: var(--mdx-font-mono, monospace);
    font-size: 11.5px;
  }
  .pr-draft-result textarea {
    width: 100%;
    padding: 9px 10px;
    font-family: var(--mdx-font-mono, monospace);
    line-height: 1.45;
    resize: vertical;
  }
  .pr-draft-facts {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 14px;
    color: var(--mdx-text-tertiary);
    font-family: var(--mdx-font-mono, monospace);
    font-size: 11.5px;
  }

  .decide {
    display: grid;
    gap: 10px;
    border-top: 1px solid var(--mdx-border-subtle);
    padding-top: 14px;
  }
  .next-line { margin: 0; font-size: 14px; color: var(--mdx-text-primary); }
  .verb {
    display: flex;
    gap: 8px;
    align-items: center;
    flex-wrap: wrap;
  }
  .verb input {
    flex: 1;
    min-width: 220px;
    padding: 7px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    font-size: 13px;
  }
  .mdx-btn.ghost {
    justify-self: start;
    color: var(--mdx-text-secondary);
  }
  .shipped-note { margin: 0; font-size: 13px; color: var(--mdx-accent-primary); }
  .action-flash { margin: 0; font-size: 12.5px; color: var(--mdx-accent-primary); }
  .action-flash.bad { color: var(--mdx-accent-error, #c0392b); }
  .quiet { color: var(--mdx-text-secondary); font-size: 13px; }

  @media (max-width: 820px) {
    .review-body {
      grid-template-columns: minmax(0, 1fr);
    }
    .review-queue {
      grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    }
    .candidate-head {
      display: grid;
    }
    .candidate-row {
      grid-template-columns: minmax(0, 1fr);
    }
    .candidate-row .section-fold {
      margin-left: 0;
      justify-self: start;
    }
    .artifact-fold-head {
      display: grid;
      justify-content: stretch;
    }
    .artifact-fold-head span {
      text-align: left;
    }
    .artifact-row {
      grid-template-columns: minmax(0, 1fr);
      gap: 4px;
    }
    .artifact-sample {
      white-space: normal;
      overflow-wrap: anywhere;
    }
  }
  .file-sig { flex: none; font-size: 11px; font-weight: 650; padding: 1px 7px; border-radius: 999px; white-space: nowrap; }
  .file-sig.warn { color: var(--mdx-accent-warning, #b45309); background: color-mix(in srgb, var(--mdx-accent-warning, #b45309) 12%, transparent); }
  .file-sig.ok { color: var(--mdx-accent-success); background: color-mix(in srgb, var(--mdx-accent-success) 12%, transparent); }
  /* Interaction motion for the review room: the queue and verbs eased like
     the rest of the app instead of snapping. */
  .queue-row:hover, .file-head:hover, .verb button:hover { background: color-mix(in srgb, var(--mdx-accent-primary) 6%, transparent); }
  .report-this { border: none; background: none; font: inherit; font-size: 12px; color: var(--mdx-text-secondary); text-decoration: underline; cursor: pointer; padding: 0; }
  .check-body { display: grid; gap: 3px; min-width: 0; }
  .check-cmd { font-family: var(--mdx-font-mono); font-size: 12px; }
  .check-tail summary { cursor: pointer; font-size: 11px; color: var(--mdx-text-muted); }
  .check-tail code { display: block; margin-top: 4px; padding: 8px 10px; border-radius: 7px; background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle); font-size: 11px; line-height: 1.5; white-space: pre-wrap; word-break: break-word; color: var(--mdx-text-secondary); }
  .rq-trials { margin-top: 8px; }
  .rq-trials summary { cursor: pointer; font-size: 12px; color: var(--mdx-text-muted); padding: 4px 2px; }
  .queue-row.trial { opacity: 0.75; }
</style>
