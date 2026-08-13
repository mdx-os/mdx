<script>
  // The Tier 0 first mission, wired to Codex's real spine. Reads the live
  // first-mission projection: Moment A (real curated starters) -> POST start ->
  // Moment B (the REAL brief + persona contributions, revealed one by one) ->
  // C/D/E (Page, streamed Forge run, Message card) -> F (payoff). The run/Message
  // light up as Codex's worker-observer runs the live build; the component polls.
  // `initial` is the projection from the page loader; `live` holds polled updates.
  import { onMount, onDestroy } from "svelte";
  import { startLivePoll } from "./livePoll.js";
  import RunViewer from "./RunViewer.svelte";
  import DiffView from "./DiffView.svelte";
  import DeliberationStream from "./DeliberationStream.svelte";

  let { initial, repos = [] } = $props();
  let live = $state(null);
  const fm = $derived(live ?? initial);
  const mission = $derived(fm?.mission ?? null);
  const starters = $derived(fm?.starters ?? []);
  // Repo-first: lead with starters scouted for THEIR connected repo; MDx-self is the
  // labeled safe sandbox. Starters carry repo_id/repo_target from the spine.
  const isSelf = (s) => !s.repo_id || s.repo_id === "mdx-self";
  const repoLabel = (id) => repos.find((r) => r.repo_id === id)?.label ?? id;
  const yourRepoGroups = $derived(
    Object.values(
      starters.filter((s) => !isSelf(s)).reduce((acc, s) => {
        (acc[s.repo_id] ??= { repo_id: s.repo_id, label: repoLabel(s.repo_id), items: [] }).items.push(s);
        return acc;
      }, {})
    )
  );
  const sandboxStarters = $derived(starters.filter(isSelf));
  const hasYourRepo = $derived(yourRepoGroups.length > 0);
  const chosenStarter = $derived(starters.find((s) => s.starter_id === chosen) ?? null);
  const brief = $derived(mission?.brief ?? null);
  const personas = $derived(mission?.persona_contributions ?? []);
  // Live shaping: the personas really deliberate over the ask (streamed). Curated
  // shaping is the honest offline fallback - prepared lines revealed on a timer.
  const shaping = $derived(mission?.shaping ?? null);
  const liveShaping = $derived(shaping?.mode === "live_personas");
  const shapingReady = $derived(liveShaping ? shaping?.status === "ready" : revealed >= personas.length);
  const run = $derived(mission?.run ?? null);
  const stages = $derived(run?.stages ?? []);
  const stageState = (s) => s?.state ?? s?.status ?? "pending";
  const runDone = $derived(stages.length > 0 && stages.every((s) => stageState(s) === "done"));
  // Honest proof: only claim the proof passed when the checks came back green,
  // not merely because the run reached a terminal stage.
  const runChecksPassed = $derived(Number(run?.checks_passed ?? 0));
  const runChecksFailed = $derived(Number(run?.checks_failed ?? 0));
  const runProofGreen = $derived(
    run?.proof_passed === true || (runChecksFailed === 0 && runChecksPassed > 0)
  );
  const runStarted = $derived(!!(run?.run_id) || stages.some((s) => stageState(s) !== "pending"));
  const msg = $derived(mission?.message ?? null);

  let chosen = $state("");
  let askText = $state("");
  const customAsk = $derived(askText.trim());
  const forgeAskHref = $derived(`/forge?intent=${encodeURIComponent(customAsk)}&source=first-mission`);
  const runHref = $derived(run?.run_id ? `/forge/runs?run=${encodeURIComponent(run.run_id)}` : "/forge/runs");
  let startFlash = $state("");
  let starting = $state(false);
  let proceeded = $state(false);
  let revealed = $state(0);
  let advanced = $state(false);
  let stopPoll = null;
  let revealId = null;

  // The backend admits the run during start, so the run is already running while
  // the user reads the brief (Moment B). Gate the run-viewer on the user pressing
  // "Make it real" (proceeded) - not on runStarted - so Moment B isn't skipped.
  const phase = $derived.by(() => {
    if (!mission) return "ask";
    if (advanced) return "payoff";
    if (proceeded) return "running";
    return "shaping";
  });

  async function poll() {
    try {
      const r = await fetch("/api/kernel/activation/first-mission/projection.json", { signal: AbortSignal.timeout(2500) });
      if (r.ok) live = await r.json();
    } catch (error) { /* keep last */ }
  }
  function startReveal() {
    revealed = 0;
    clearInterval(revealId);
    revealId = setInterval(() => {
      const n = mission?.persona_contributions?.length ?? 0;
      // Keep ticking until personas have loaded (n>0); only stop once all revealed.
      if (n > 0 && revealed >= n) { clearInterval(revealId); return; }
      if (revealed < n) revealed += 1;
    }, 850);
  }
  function openForgeAsk() {
    if (!customAsk || typeof window === "undefined") return;
    window.location.assign(forgeAskHref);
  }
  async function start() {
    if (starting) return;
    if (customAsk) {
      openForgeAsk();
      return;
    }
    if (!chosenStarter) return;
    starting = true;
    // Honor the chosen starter's repo target (their repo or the MDx sandbox). A
    // curated/scout starter carries the repo + authorized work item. Free-text
    // work opens Forge's normal composer instead, so it is not collapsed into
    // this tiny teaching mission's DEV-101 starter scope.
    const st = chosenStarter;
    const body = { actor_id: "human:local_user" };
    body.starter_id = st.starter_id;
    body.repo_target = st.repo_target ?? "mdx_self";
    if (st.repo_id) body.repo_id = st.repo_id;
    try {
      const response = await fetch("/api/kernel/activation/first-mission/start.json", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) });
      const packet = await response.json().catch(() => null);
      if (packet?.status === "REFUSED") {
        // A refusal must be read, not swallowed - the engineer was staring at
        // a silent screen wondering why nothing started.
        startFlash = packet.reason ?? "That mission could not start. Try again.";
      } else {
        startFlash = "";
      }
    } catch (error) {
      startFlash = "The start did not reach MDx. It may be busy - try again in a moment.";
    }
    await poll();
    starting = false;
    startReveal();
    startPolling();
  }
  function startPolling() {
    // One jittered, visibility-gated poll loop (shared startLivePoll): a hidden
    // tab drops to a slow heartbeat instead of a fixed 2.5s timer, restarts
    // stagger instead of synchronizing, and it stops itself at a terminal
    // state. The run's live detail rides SSE in RunViewer; this poll just keeps
    // the mission projection (brief, reveal, terminal) fresh.
    stopPoll?.();
    stopPoll = startLivePoll({
      isActive: () => !runDone,
      refresh: () => poll(),
      activeMs: 2500
    });
  }
  function pick(s) { chosen = s.starter_id; askText = ""; }
  function clearStarterChoice() {
    if (chosen) chosen = "";
  }
  // Enter the real Message surface where the result landed. A full-page
  // navigation (not client routing) so the primary action reliably leaves the
  // setup wall and lands the user in the product, action card and all.
  function enterMessage() {
    if (typeof window !== "undefined") window.location.assign("/message");
  }
  onMount(() => {
    if (mission) {
      // Resume: if the run is already underway/done on load, go straight to it.
      if (runStarted) proceeded = true;
      startReveal();
      startPolling();
    }
  });
  onDestroy(() => { stopPoll?.(); clearInterval(revealId); });

  const chain = $derived([
    { surface: "Twin", did: "shaped the brief with your specialists.", lit: !!brief },
    { surface: "Memory", did: "saved it as a Page your team can reuse.", lit: !!mission?.page },
    { surface: "Forge", did: runDone && !runProofGreen ? "ran the build; the proof isn't green yet." : "ran the build and passed the proof.", lit: runDone },
    { surface: "Message", did: "dropped the result for you to approve.", lit: !!msg }
  ]);
</script>

<section class="rv">
  <div class="rv-inner">
    <div class="rv-progress"><span class="eyebrow">Your first mission</span><span class="rv-step">{phase === "ask" ? "Step 1" : phase === "shaping" ? "Shaping" : phase === "running" ? (runDone ? "Built" : run?.status === "fallback" || run?.status === "needs_attention" ? "Needs you" : run?.status === "admitting" ? "Starting" : "Running") : "Done"}</span></div>

    <div class="card">
      {#if phase === "ask"}
        {#snippet starterBtn(s)}
          <button type="button" class="starter" class:active={chosen === s.starter_id} onclick={() => pick(s)}>
            <span class="starter-icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 11l3 3L22 4 M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" /></svg></span>
            <span class="starter-body"><strong>{s.title}</strong><span>{s.ask}</span></span>
          </button>
        {/snippet}
        <h2>What do you want to get moving?</h2>
        <p class="panel-sub">Pick a quick starter to watch the first loop here, or send your own ask to Forge's normal build screen with the text already filled in.</p>
        {#if starters.length === 0}<p class="fhint">No starters available right now - the kernel may be offline.</p>{/if}
        {#if hasYourRepo}
          {#each yourRepoGroups as g (g.repo_id)}
            <div class="repo-group">
              <span class="repo-head"><span class="repo-dot" aria-hidden="true"></span>{g.label}</span>
              <div class="starters">{#each g.items as s (s.starter_id)}{@render starterBtn(s)}{/each}</div>
            </div>
          {/each}
          {#if sandboxStarters.length}
            <div class="repo-group">
              <span class="repo-head muted">Or try it on MDx itself <span class="repo-tag">safe sandbox</span></span>
              <div class="starters">{#each sandboxStarters as s (s.starter_id)}{@render starterBtn(s)}{/each}</div>
            </div>
          {/if}
        {:else}
          <div class="starters">{#each sandboxStarters as s (s.starter_id)}{@render starterBtn(s)}{/each}</div>
          <a class="connect-hint" href="/forge">Working on your own code? Connect your repo in Forge - this run is a safe MDx sandbox →</a>
        {/if}
        <div class="field">
          <label class="flabel" for="mp-ask">Or describe it</label>
          <input id="mp-ask" class="finput" bind:value={askText} placeholder="e.g. add a test for the slug helper" autocomplete="off" onfocus={clearStarterChoice} oninput={clearStarterChoice} onkeydown={(e) => { if (e.key === "Enter") start(); }} />
          <p class="field-hint">Custom asks open Forge with your text filled in. Broad demos use a small playground so they are not narrowed to the starter cards.</p>
        </div>
        <button type="button" class="mdx-btn primary go" disabled={!(customAsk || chosenStarter) || starting} onclick={() => start()}>{starting ? "Starting..." : customAsk ? "Open in Forge →" : "Shape starter with Twin →"}</button>
        {#if startFlash}<p class="mf-flash" role="alert">{startFlash}</p>{/if}

      {:else if phase === "shaping"}
        <h2>Twin is shaping it</h2>
        <p class="panel-sub">Your specialists are turning the ask into a real brief.</p>
        {#if liveShaping}
          <DeliberationStream {shaping} {personas} />
        {:else}
          <ul class="personas">
            {#each personas as p, i (i)}
              {#if i < revealed}
                <li class="persona">
                  <span class="persona-icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z M16 8l-5 2-2 5 5-2z" /></svg></span>
                  <div class="persona-body"><strong>{p.label}</strong><p>{p.line}</p></div>
                </li>
              {/if}
            {/each}
            {#if revealed < personas.length}<li class="persona thinking"><span class="persona-dots" aria-hidden="true"><i></i><i></i><i></i></span><span>{personas[revealed]?.label ?? "Twin"} is thinking...</span></li>{/if}
          </ul>
        {/if}
        {#if shapingReady && brief}
          <div class="brief">
            <span class="brief-head">The brief</span>
            <div class="brief-row"><span>Goal</span><p>{brief.goal}</p></div>
            <div class="brief-row"><span>Approach</span><p>{brief.approach}</p></div>
            <div class="brief-row"><span>Done when</span><p>{brief.acceptance}</p></div>
          </div>
          <button type="button" class="mdx-btn primary go" onclick={() => (proceeded = true)}>Make it real →</button>
        {/if}

      {:else if phase === "running"}
        {#if mission?.page}<div class="page-saved"><span class="ps-check"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="m5 12 5 5L20 7"/></svg></span>Saved to Memory as a Page · <a href="/pages">{mission.page.title}</a></div>{/if}
        <RunViewer {run} streamRoute={run?.stream_route} />
        {#if runDone}
          <DiffView runId={run?.run_id} ready={runDone} />
          <div class="mf-finish">
            <a class="mdx-btn primary go" href={runHref}>Review this run in Forge →</a>
            {#if msg}<button type="button" class="mf-secondary" onclick={() => enterMessage()}>Open Message</button>{/if}
            <button type="button" class="mf-recap" onclick={() => (advanced = true)}>See recap</button>
          </div>
        {:else}
          <div class="mf-finish">
            <a class="mdx-btn primary go" href={runHref}>Watch this run in Forge →</a>
            <button type="button" class="mf-recap" onclick={() => (advanced = true)}>See recap</button>
          </div>
        {/if}

      {:else}
        <h2>Your first loop is underway</h2>
        <p class="panel-sub">Here is what has landed so far. Anything still grey is still moving, not lost.</p>
        <ol class="chain">
          {#each chain as node (node.surface)}
            <li class="chain-node" data-lit={node.lit ? "true" : "false"}>
              <span class="chain-dot">{#if node.lit}<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="m5 12 5 5L20 7"/></svg>{/if}</span>
              <div class="chain-body"><div class="chain-top"><strong>{node.surface}</strong></div><p>{node.did}</p></div>
            </li>
          {/each}
        </ol>
        <p class="po-note">This was a real Page and a real build path. Forge or Message will show the next decision when the run finishes.</p>
        <div class="po-cta"><a class="mdx-btn primary go" href={msg ? "/message" : "/forge"}>{msg ? "Open Message →" : "Open Forge →"}</a><a class="po-skip" href="/twin">or start in Twin</a></div>
      {/if}
    </div>
  </div>
</section>

<style>
  .rv { flex: 1; display: flex; flex-direction: column; align-items: center; padding: 24px; }
  .rv-inner { width: 100%; max-width: 540px; margin: auto 0; }
  .eyebrow { font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: var(--mdx-text-muted); }
  .rv-progress { display: flex; align-items: baseline; justify-content: space-between; margin-bottom: 14px; }
  .rv-step { font-size: 12px; color: var(--mdx-text-muted); }
  .card { padding: 28px 30px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-lg); background: var(--mdx-surface-raised); box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card); }
  .card h2 { margin: 0; font-family: var(--mdx-font-display); font-size: 23px; font-weight: 700; letter-spacing: -0.01em; }
  .panel-sub { margin: 8px 0 0; color: var(--mdx-text-secondary); font-size: 14px; line-height: 1.55; }
  .repo-group { margin: 20px 0 0; }
  .repo-head { display: flex; align-items: center; gap: 8px; font-size: 12px; font-weight: 650; color: var(--mdx-text-secondary); }
  .repo-head.muted { color: var(--mdx-text-muted); font-weight: 600; }
  .repo-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--mdx-accent-primary); }
  .repo-tag { font-size: 10.5px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase; color: var(--mdx-text-muted); padding: 2px 7px; border-radius: var(--mdx-radius-pill); border: 1px solid var(--mdx-border-subtle); }
  .repo-group .starters { margin-top: 9px; }
  .connect-hint { display: inline-block; margin: 14px 0 0; font-size: 12.5px; color: var(--mdx-text-muted); text-decoration: none; }
  .connect-hint:hover { color: var(--mdx-accent-primary); }
  .starters { display: grid; gap: 8px; margin: 22px 0 0; }
  .starter { display: flex; align-items: center; gap: 13px; text-align: left; padding: 13px 15px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); color: inherit; font: inherit; cursor: pointer; transition: border-color 0.14s ease, background 0.14s ease; }
  .starter:hover { border-color: color-mix(in srgb, var(--mdx-accent-primary) 40%, var(--mdx-border-subtle)); }
  .starter.active { border-color: var(--mdx-accent-primary); background: color-mix(in srgb, var(--mdx-accent-primary) 8%, transparent); }
  .starter-icon { flex: none; display: inline-flex; width: 34px; height: 34px; align-items: center; justify-content: center; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-primary) 12%, transparent); color: var(--mdx-accent-primary); }
  .starter-icon svg { width: 17px; height: 17px; }
  .starter-body { display: grid; gap: 1px; min-width: 0; }
  .starter-body strong { font-size: 13.5px; font-weight: 650; }
  .starter-body span { font-size: 12px; color: var(--mdx-text-muted); overflow: hidden; text-overflow: ellipsis; }
  .field { margin-top: 18px; }
  .flabel { display: block; margin-bottom: 8px; font-size: 12.5px; font-weight: 600; color: var(--mdx-text-secondary); }
  .finput { width: 100%; height: 42px; box-sizing: border-box; border: 1px solid var(--mdx-border-default); border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); color: var(--mdx-text-primary); padding: 0 13px; font: inherit; font-size: 14px; outline: none; }
  .finput:focus-visible { border-color: var(--mdx-accent-primary); }
  .field-hint { margin: 7px 0 0; color: var(--mdx-text-muted); font-size: 12px; line-height: 1.45; }
  .fhint { margin: 12px 0 0; font-size: 12.5px; color: var(--mdx-text-muted); }
  .personas { list-style: none; margin: 22px 0 0; padding: 0; display: grid; gap: 12px; }
  .persona { display: flex; gap: 12px; animation: rise 0.4s ease both; }
  @keyframes rise { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
  .persona-icon { flex: none; display: inline-flex; width: 32px; height: 32px; align-items: center; justify-content: center; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-primary) 12%, transparent); color: var(--mdx-accent-primary); }
  .persona-icon svg { width: 16px; height: 16px; }
  .persona-body strong { font-size: 13px; font-weight: 650; }
  .persona-body p { margin: 2px 0 0; font-size: 13px; color: var(--mdx-text-secondary); line-height: 1.5; }
  .persona.thinking { align-items: center; gap: 9px; color: var(--mdx-text-muted); font-size: 12.5px; }
  .persona-dots { display: inline-flex; gap: 3px; }
  .persona-dots i { width: 5px; height: 5px; border-radius: 50%; background: var(--mdx-text-muted); animation: blink 1.2s infinite; }
  .persona-dots i:nth-child(2) { animation-delay: 0.2s; }
  .persona-dots i:nth-child(3) { animation-delay: 0.4s; }
  .brief { margin: 22px 0 0; padding: 16px 18px; border-radius: var(--mdx-radius-lg); background: color-mix(in srgb, var(--mdx-accent-primary) 6%, var(--mdx-surface-base)); border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 18%, transparent); animation: rise 0.4s ease both; }
  .brief-head { font-size: 11px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: var(--mdx-accent-primary); }
  .brief-row { display: grid; grid-template-columns: 88px 1fr; gap: 10px; margin-top: 11px; }
  .brief-row span { font-size: 12px; color: var(--mdx-text-muted); padding-top: 1px; }
  .brief-row p { margin: 0; font-size: 13px; color: var(--mdx-text-primary); line-height: 1.45; }
  .page-saved { display: flex; align-items: center; gap: 9px; font-size: 12.5px; color: var(--mdx-text-secondary); margin-bottom: 18px; }
  .page-saved a { color: var(--mdx-accent-primary); text-decoration: none; }
  .ps-check { flex: none; display: inline-flex; width: 18px; height: 18px; align-items: center; justify-content: center; border-radius: 50%; background: var(--mdx-accent-success); color: #fff; }
  .ps-check svg { width: 11px; height: 11px; }
  @keyframes blink { 50% { opacity: 0; } }
  .chain { list-style: none; margin: 22px 0 0; padding: 0; }
  .chain-node { position: relative; display: flex; gap: 13px; padding: 0 0 18px; }
  .chain-node:last-child { padding-bottom: 0; }
  .chain-node:not(:last-child) .chain-dot::after { content: ""; position: absolute; top: 24px; left: 11px; width: 2px; height: calc(100% - 24px); background: var(--mdx-border-subtle); }
  .chain-node[data-lit="true"]:not(:last-child) .chain-dot::after { background: color-mix(in srgb, var(--mdx-accent-success) 45%, var(--mdx-border-subtle)); }
  .chain-dot { position: relative; flex: none; width: 24px; height: 24px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; background: var(--mdx-surface-base); border: 1px solid var(--mdx-border-subtle); color: var(--mdx-text-muted); }
  .chain-node[data-lit="true"] .chain-dot { background: var(--mdx-accent-success); border-color: var(--mdx-accent-success); color: #fff; }
  .chain-dot svg { width: 13px; height: 13px; }
  .chain-body { flex: 1; min-width: 0; }
  .chain-node[data-lit="false"] .chain-body { opacity: 0.55; }
  .chain-top strong { font-size: 14px; font-weight: 650; }
  .chain-body p { margin: 3px 0 0; font-size: 13px; color: var(--mdx-text-secondary); line-height: 1.5; }
  .po-note { margin: 18px 0 0; font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.5; }
  .po-cta { margin: 24px 0 0; display: flex; align-items: center; gap: 18px; }
  .po-skip { color: var(--mdx-text-muted); font-size: 13px; text-decoration: none; }
  .po-skip:hover { color: var(--mdx-text-primary); }
  .go { margin-top: 22px; height: 42px; padding: 0 22px; font-weight: 650; }
  .mf-finish { margin-top: 22px; display: flex; align-items: center; gap: 18px; flex-wrap: wrap; }
  .mf-finish .go { margin-top: 0; display: inline-flex; align-items: center; text-decoration: none; }
  .mf-secondary { border: none; background: none; color: var(--mdx-accent-primary); font: inherit; font-size: 13px; cursor: pointer; padding: 0; }
  .mf-secondary:hover { text-decoration: underline; }
  .mf-recap { border: none; background: none; color: var(--mdx-text-muted); font: inherit; font-size: 13px; cursor: pointer; padding: 0; }
  .mf-recap:hover { color: var(--mdx-text-primary); }
  .mf-flash { margin: 8px 0 0; font-size: 13px; color: var(--mdx-accent-warning, #b45309); }
</style>
