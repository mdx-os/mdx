<script>
  // Shared Forge run viewer (Slick Rails primitive). Renders a run live:
  // - Baseline = the run projection's stages (reliable; late viewers reconstruct here).
  // - Enhancement = the SSE stream (Codex contract): stage_start / tool_result /
  //   proof_output / diff_ready / terminal advance the view live. token_delta is
  //   handled too, for when the adapter-level token slice lands.
  // Consumed by the first mission, Forge home, and the Message action card.
  import { onDestroy } from "svelte";
  import { subscribeStream } from "./sse.js";

  let { run = null, streamRoute = "" } = $props();

  const stageState = (s) => s?.state ?? s?.status ?? "pending";
  // Honest pre-run state: the run is reserved but Forge has not accepted it yet.
  const admitting = $derived(run?.status === "admitting");
  const fallbackReason = $derived(run?.fallback_reason ?? "");
  // Honest stalled state - but trust real activity over a stale status: the admission
  // watchdog can flag "fallback" while the run is actually streaming and finishing, so
  // only show the couldn't-start state when there is genuinely no sign of work.
  const fellBack = $derived(
    (run?.status === "fallback" || run?.status === "needs_attention") &&
      !view.done && !view.diffReady && view.liveFeed.length === 0
  );
  // Honest proof gating: "the proof passed" is a real claim, so make it only
  // when the checks actually came back green - not merely because the run
  // reached a terminal stage. A finished-but-not-green run defers to the
  // recovery copy instead of a fake green line. Accepts snake or camel counts
  // (raw projection vs mapped run) and an explicit proofPassed flag.
  const checksPassed = $derived(Number(run?.checksPassed ?? run?.checks_passed ?? 0));
  const checksFailed = $derived(Number(run?.checksFailed ?? run?.checks_failed ?? 0));
  const proofFlag = $derived(run?.proofPassed ?? run?.proof_passed);
  const proofKnown = $derived(typeof proofFlag === "boolean" || checksPassed + checksFailed > 0);
  const proofGreen = $derived(proofFlag === true || (checksFailed === 0 && checksPassed > 0));
  const STAGE_ALIAS = { read: "reading", reading: "reading", plan: "planning", planning: "planning", write: "writing", writing: "writing", proof: "proof", prove: "proof", done: "done" };
  const mapStage = (s) => STAGE_ALIAS[s] ?? s;
  const modelLabel = (cls) => ({ frontier_strong: "frontier", frontier_fast: "fast frontier", frontier_strict: "frontier (strict)", local_fallback: "local (dev)", sovereign_local: "private (local)" }[cls] ?? cls);
  const SPINE_KINDS = new Set(["stage_start", "tool_call", "tool_result", "proof_output", "diff_ready", "terminal"]);
  // How much of the live trail and proof output to retain. The stream folds
  // incrementally into these bounds instead of re-sorting and re-folding the
  // entire event history on every frame (quadratic once token deltas land) and
  // instead of letting the retained arrays grow without limit.
  const FEED_CAP = 40;
  const PROOF_CAP = 4000;

  // Incremental stream accumulator: each SSE frame is applied once, in arrival
  // order (a single EventSource delivers in order), advancing a monotonic
  // spine, a bounded live feed, and bounded proof output. No per-frame re-sort.
  function emptyStream() {
    return { hasStream: false, reached: -1, terminated: false, proofOutput: "", branch: "", model: null, error: null, feed: [] };
  }
  let stream = $state(emptyStream());

  // Live-trail health: a dead stream must never look like an eternal
  // "thinking...". After a couple of failed reconnect attempts we say so
  // plainly and lean on the projection baseline (the stages render from
  // run.stages regardless), then quietly recover when the stream comes back.
  let streamHealth = $state("live"); // "live" | "reconnecting" | "closed"
  let failedAttempts = $state(0);
  let unsub = null;
  let lastRoute = "";

  function stageRank(stageKey) {
    const stages = run?.stages ?? [];
    for (let i = 0; i < stages.length; i += 1) if (stages[i].key === stageKey) return i;
    return null;
  }

  function applyEvent(ev) {
    const s = stream;
    s.hasStream = true;
    // Prefer a real model class; a placeholder ("unknown") before routing
    // resolves must not overwrite the projection's real cast.
    if (ev.model?.model_class && ev.model.model_class !== "unknown") s.model = ev.model;
    const sk = mapStage(ev.stage);
    const pl = ev.payload ?? {};
    const line = pl.summary ?? pl.text ?? pl.detail ?? pl.name ?? "";
    if (SPINE_KINDS.has(ev.kind)) {
      const rank = stageRank(sk);
      if (rank != null && rank > s.reached) s.reached = rank;
    }
    switch (ev.kind) {
      case "stage_start":
        pushAction(line);
        break;
      case "token_delta":
        pushThought(pl.text ?? "");
        break;
      case "tool_call":
      case "tool_result":
      case "evidence":
        pushAction(line);
        break;
      case "proof_output":
        s.proofOutput = (s.proofOutput + (pl.text ?? pl.output ?? pl.detail ?? "")).slice(-PROOF_CAP);
        pushAction(line);
        break;
      case "diff_ready":
        s.branch = pl.branch ?? s.branch;
        pushAction(line || "Prepared the diff to review");
        break;
      case "terminal":
        s.terminated = true;
        break;
      case "error":
        s.error = pl.message ?? pl.text ?? pl.detail ?? "Run error";
        break;
      // "event" frames are turn boundaries - skipped as noise.
    }
  }

  function pushAction(text) {
    if (!text) return;
    const feed = stream.feed;
    feed.push({ kind: "do", text });
    if (feed.length > FEED_CAP) feed.splice(0, feed.length - FEED_CAP);
  }
  function pushThought(text) {
    if (!text) return;
    const feed = stream.feed;
    const last = feed[feed.length - 1];
    if (last && last.kind === "think") {
      last.text = (last.text + text).slice(-400);
    } else {
      feed.push({ kind: "think", text });
      if (feed.length > FEED_CAP) feed.splice(0, feed.length - FEED_CAP);
    }
  }

  function subscribe(route) {
    unsub?.();
    stream = emptyStream();
    streamHealth = "live";
    failedAttempts = 0;
    unsub = subscribeStream(route, {
      onEvent: (ev) => {
        applyEvent(ev);
        failedAttempts = 0;
        streamHealth = "live";
      },
      onReconnect: () => {
        failedAttempts = 0;
        streamHealth = "live";
      },
      onError: () => {
        failedAttempts += 1;
        // One blip is normal; only after a couple do we say the trail dropped.
        if (streamHealth !== "closed" && failedAttempts >= 2) streamHealth = "reconnecting";
      },
      onClose: () => {
        streamHealth = "closed";
      }
    });
  }

  $effect(() => {
    const route = streamRoute;
    if (!route || route === lastRoute) return;
    lastRoute = route;
    subscribe(route);
  });
  onDestroy(() => unsub?.());

  function retryStream() {
    const route = streamRoute;
    if (!route) return;
    subscribe(route);
  }

  const baseStages = $derived((run?.stages ?? []).map((s) => ({ key: s.key, label: s.label, state: stageState(s), detail: s.detail })));

  const view = $derived.by(() => {
    const stages = baseStages.map((s) => ({ ...s }));
    const hasStream = stream.hasStream;
    let done;
    if (hasStream) {
      let reached = stream.reached;
      if (stream.terminated) reached = stages.length - 1;
      done = stream.terminated || (stages.length > 0 && reached >= stages.length - 1);
      for (let i = 0; i < stages.length; i++) {
        stages[i].state = done ? "done" : i < reached ? "done" : i === reached ? "active" : "pending";
      }
    } else {
      done = stages.length > 0 && stages.every((s) => stageState(s) === "done");
      for (const s of stages) s.state = stageState(s);
    }
    const diffReady = !!run?.branch || !!stream.branch;
    const branch = stream.branch || run?.branch || "";
    const model = stream.model ?? run?.model ?? null;
    // Show the most recent slice so the feed reads as "now", and clamp a long
    // thought to its trailing text so it streams rather than walls.
    const liveFeed = stream.feed.slice(-7).map((f) => ({ ...f, text: f.kind === "think" ? f.text.slice(-180) : f.text }));
    return { stages, proofOutput: stream.proofOutput.slice(-1200), diffReady, branch, model, done, error: stream.error, liveFeed };
  });
</script>

<div class="run">
  <div class="rv-top">
    <span class="rv-live" class:on={!view.done && !view.error && !fellBack} aria-hidden="true"></span>
    <p>{view.error ? "Forge hit a snag - details below." : view.done ? (proofGreen ? "Forge built it - and the proof passed." : proofKnown ? "Forge finished - the proof isn't green yet. The recovery notes below have the next move." : "Forge finished, working in a safe copy.") : fellBack ? "Forge couldn't start this run in time - your brief is saved, and it's waiting for you in Message." : admitting ? "Getting Forge ready - starting the run in a safe copy." : "Forge is working in a safe copy. Nothing ships without you."}</p>
    {#if view.model?.model_class && !fellBack}<span class="model-badge" title="model used for this run">{modelLabel(view.model.model_class)}</span>{/if}
  </div>
  {#if fellBack}
    {#if fallbackReason}<p class="rv-fallback">{fallbackReason}</p>{/if}
  {:else}
  <ol class="stages">
    {#each view.stages as s, i (s.key)}
      <li class="stage" data-state={s.state}>
        <span class="stage-dot">{#if s.state === "done"}<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="m5 12 5 5L20 7"/></svg>{:else}{i + 1}{/if}</span>
        <div class="stage-body">
          <span class="stage-label">{s.label}</span>
        </div>
      </li>
    {/each}
  </ol>
  {/if}
  {#if !view.done && !view.error && view.liveFeed.length}
    <div class="rv-feed" aria-live="polite">
      {#each view.liveFeed as f, i (i)}
        <p class="feed-line" data-kind={f.kind}>{#if f.kind === "do"}<span class="feed-mark">⎿</span>{/if}<span class="feed-text">{f.text}</span>{#if i === view.liveFeed.length - 1}<span class="caret" aria-hidden="true"></span>{/if}</p>
      {/each}
    </div>
  {/if}
  {#if streamHealth === "reconnecting" && !view.done && !view.error && !fellBack}
    <div class="rv-trail" role="status">
      <span>Lost the live trail - reconnecting. The stages above still update from the run's own record.</span>
      <button type="button" class="rv-trail-retry" onclick={retryStream}>Retry now</button>
    </div>
  {/if}
  {#if view.proofOutput}<pre class="proof-out">{view.proofOutput}</pre>{/if}
  {#if view.done && !view.error}
    <div class="rv-meta">{#if proofGreen}<span class="rv-check">proof passed</span>{:else if proofKnown}<span class="rv-check not-green">{checksFailed} of {checksPassed + checksFailed} checks not green</span>{/if}{#if view.branch}<span class="rv-branch">{view.branch}</span>{/if}{#if view.diffReady}<span class="rv-diff">diff ready</span>{/if}</div>
  {/if}
  {#if view.error}<p class="rv-error">{view.error}</p>{/if}
</div>

<style>
  .rv-top { display: flex; gap: 11px; align-items: center; }
  .rv-top p { margin: 0; flex: 1; font-size: 14px; color: var(--mdx-text-secondary); line-height: 1.5; }
  .rv-live { flex: none; width: 9px; height: 9px; border-radius: 50%; background: var(--mdx-text-muted); }
  .rv-live.on { background: var(--mdx-accent-success); animation: rvpulse 1.4s ease-in-out infinite; }
  @keyframes rvpulse { 0%,100% { opacity: 0.35; } 50% { opacity: 1; } }
  .model-badge { flex: none; font-size: 10.5px; font-weight: 700; letter-spacing: 0.03em; text-transform: uppercase; color: var(--mdx-accent-primary); padding: 2px 8px; border-radius: var(--mdx-radius-pill); border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 30%, transparent); }
  .stages { list-style: none; margin: 22px 0 0; padding: 0; }
  .stage { position: relative; display: flex; gap: 14px; padding: 0 0 18px; }
  .stage:last-child { padding-bottom: 0; }
  .stage:not(:last-child) .stage-dot::after { content: ""; position: absolute; top: 26px; left: 12px; width: 2px; height: calc(100% - 26px); background: var(--mdx-border-subtle); }
  .stage[data-state="done"]:not(:last-child) .stage-dot::after { background: color-mix(in srgb, var(--mdx-accent-success) 45%, var(--mdx-border-subtle)); }
  .stage-dot { position: relative; flex: none; width: 26px; height: 26px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; font-size: 12px; font-weight: 700; background: var(--mdx-surface-base); border: 1px solid var(--mdx-border-subtle); color: var(--mdx-text-muted); }
  .stage-dot svg { width: 14px; height: 14px; }
  .stage[data-state="done"] .stage-dot { background: var(--mdx-accent-success); border-color: var(--mdx-accent-success); color: #fff; }
  .stage[data-state="active"] .stage-dot { border-color: var(--mdx-accent-primary); color: var(--mdx-accent-primary); box-shadow: 0 0 0 4px color-mix(in srgb, var(--mdx-accent-primary) 12%, transparent); }
  .stage-body { flex: 1; min-width: 0; padding-top: 3px; }
  .stage-label { font-size: 14px; font-weight: 600; }
  .stage[data-state="pending"] .stage-label { color: var(--mdx-text-muted); }
  .caret { display: inline-block; width: 7px; height: 13px; margin-left: 2px; vertical-align: text-bottom; background: var(--mdx-accent-primary); animation: blink 1s step-end infinite; }
  @keyframes blink { 50% { opacity: 0; } }
  .rv-feed { margin: 16px 0 0; padding: 12px 14px; border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); border: 1px solid var(--mdx-border-subtle); display: grid; gap: 5px; }
  .feed-line { margin: 0; display: flex; gap: 7px; font-family: var(--mdx-font-mono, monospace); font-size: 12px; line-height: 1.5; color: var(--mdx-text-secondary); overflow: hidden; }
  .feed-line[data-kind="think"] { color: var(--mdx-text-muted); font-style: italic; }
  .feed-mark { flex: none; color: var(--mdx-accent-primary); }
  .feed-text { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .feed-line[data-kind="think"] .feed-text { white-space: normal; }
  .rv-feed .feed-line:not(:last-child) { opacity: 0.62; }
  .proof-out { margin: 14px 0 0; padding: 11px 13px; max-height: 160px; overflow: auto; border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); border: 1px solid var(--mdx-border-subtle); font-family: var(--mdx-font-mono, monospace); font-size: 11.5px; line-height: 1.5; color: var(--mdx-text-secondary); white-space: pre-wrap; }
  .rv-meta { margin: 18px 0 0; display: flex; flex-wrap: wrap; gap: 9px; align-items: center; }
  .rv-check { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.04em; color: var(--mdx-accent-success); }
  .rv-check.not-green { color: var(--mdx-accent-warning, #b45309); }
  .rv-branch { font-family: var(--mdx-font-mono, monospace); font-size: 12.5px; color: var(--mdx-text-primary); padding: 3px 9px; border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); border: 1px solid var(--mdx-border-subtle); }
  .rv-diff { font-size: 12.5px; color: var(--mdx-text-muted); }
  .rv-error { margin: 14px 0 0; font-size: 13px; color: var(--mdx-accent-error); line-height: 1.5; }
  .rv-fallback { margin: 12px 0 0; font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.5; }
  .rv-trail { margin: 14px 0 0; display: flex; flex-wrap: wrap; gap: 10px; align-items: center; padding: 9px 13px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-warning, #b45309) 10%, var(--mdx-surface-base)); border: 1px solid color-mix(in srgb, var(--mdx-accent-warning, #b45309) 30%, transparent); font-size: 12.5px; color: var(--mdx-text-secondary); line-height: 1.5; }
  .rv-trail span { flex: 1; min-width: 200px; }
  .rv-trail-retry { flex: none; font: inherit; font-size: 12px; padding: 4px 11px; border-radius: var(--mdx-radius-pill); border: 1px solid var(--mdx-border-default); background: var(--mdx-surface-base); color: var(--mdx-text-primary); cursor: pointer; }
</style>
