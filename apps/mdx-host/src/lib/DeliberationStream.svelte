<script>
  // Shared deliberation viewer (Slick Rails primitive). Renders the first mission's
  // brief shaping as the real multi-persona debate it is: subscribes to the shaping
  // stream (Codex contract), folds `deliberation` frames by companion, streams each
  // voice's text (token deltas appended in seq order), and shows the Architect's
  // synthesis turn distinctly as the three voices converge on the brief. Late viewers
  // and the curated fallback reconstruct from the projection's persona_contributions.
  import { onDestroy } from "svelte";
  import { subscribeStream } from "./sse.js";

  let { shaping = null, personas = [] } = $props();

  const ORDER = ["twin_architect", "twin_coder", "twin_advisor"];
  const modelLabel = (cls) => ({ frontier_strong: "frontier", frontier_fast: "fast frontier", frontier_strict: "frontier (strict)", local_fallback: "local (dev)", sovereign_local: "private (local)" }[cls] ?? cls);
  const rank = (id) => { const i = ORDER.indexOf(id); return i === -1 ? ORDER.length : i; };

  // A dead shaping stream must not leave a voice "thinking..." forever. Track
  // the live-trail health and say so plainly after a couple of failed
  // reconnects; the projection fallback below still reconstructs the debate.
  let streamHealth = $state("live"); // "live" | "reconnecting" | "closed"
  let failedAttempts = 0;
  let unsub = null;
  let lastRoute = "";

  // Incremental fold: each shaping frame appends to a voice once, in arrival
  // order, instead of re-sorting and re-folding the whole frame history on
  // every token delta.
  function emptyAcc() {
    return { voices: {}, synthesis: null, model: null, done: false, speaking: null, received: 0 };
  }
  let acc = $state(emptyAcc());

  function applyFrame(ev) {
    const a = acc;
    a.received += 1;
    if (ev.model?.model_class && ev.model.model_class !== "unknown") a.model = ev.model;
    if (ev.kind === "terminal") { a.done = true; return; }
    if (ev.kind !== "deliberation") return;
    const pl = ev.payload ?? {};
    const id = pl.companion_id;
    if (!id) return;
    if (pl.role === "synthesis") {
      a.synthesis ??= { companion_id: id, label: pl.label ?? "Architect", text: "", done: false };
      a.synthesis.text += pl.text ?? "";
      if (pl.final) a.synthesis.done = true;
      a.speaking = a.synthesis.done ? null : "synthesis";
    } else {
      const v = (a.voices[id] ??= { companion_id: id, label: pl.label ?? id, text: "", done: false });
      v.text += pl.text ?? "";
      if (pl.final) v.done = true;
      a.speaking = v.done ? null : id;
    }
  }

  $effect(() => {
    const route = shaping?.stream_route;
    if (!route || route === lastRoute) return;
    lastRoute = route;
    acc = emptyAcc();
    streamHealth = "live";
    failedAttempts = 0;
    unsub?.();
    unsub = subscribeStream(route, {
      onEvent: (ev) => { applyFrame(ev); failedAttempts = 0; streamHealth = "live"; },
      onReconnect: () => { failedAttempts = 0; streamHealth = "live"; },
      onError: () => { failedAttempts += 1; if (streamHealth !== "closed" && failedAttempts >= 2) streamHealth = "reconnecting"; },
      onClose: () => { streamHealth = "closed"; }
    });
  });
  onDestroy(() => unsub?.());

  const view = $derived.by(() => {
    const voices = { ...acc.voices };
    let synthesis = acc.synthesis;
    let model = acc.model;
    let done = acc.done;
    let speaking = acc.speaking;
    // Baseline when nothing streamed (a late viewer after the bus cleared, or the
    // honest curated fallback): reconstruct the voices from the projection.
    if (acc.received === 0 && personas.length) {
      for (const p of personas) voices[p.companion_id] = { companion_id: p.companion_id, label: p.label, text: p.line ?? "", done: true };
      done = true;
    }
    const orderedVoices = Object.values(voices).sort((a, b) => rank(a.companion_id) - rank(b.companion_id));
    // The next voice yet to speak, while the debate is still live.
    const nextWaiting = !done && !synthesis ? ORDER.find((id) => !voices[id]) : null;
    return { voices: orderedVoices, synthesis, model, done, speaking, nextWaiting };
  });

  const personaLabel = (id) => ({ twin_architect: "Architect", twin_coder: "Coder", twin_advisor: "Advisor" }[id] ?? "Twin");
</script>

<div class="dl">
  {#if view.model?.model_class}<span class="dl-model" title="model shaping this brief">{modelLabel(view.model.model_class)}</span>{/if}
  <ul class="dl-voices">
    {#each view.voices as v (v.companion_id)}
      <li class="dl-voice">
        <span class="dl-icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z M16 8l-5 2-2 5 5-2z" /></svg></span>
        <div class="dl-body"><strong>{v.label}</strong><p>{v.text}{#if !v.done}<span class="dl-caret" aria-hidden="true"></span>{/if}</p></div>
      </li>
    {/each}
    {#if view.nextWaiting}
      <li class="dl-voice thinking"><span class="dl-dots" aria-hidden="true"><i></i><i></i><i></i></span><span>{personaLabel(view.nextWaiting)} is thinking...</span></li>
    {/if}
  </ul>
  {#if streamHealth === "reconnecting" && !view.done}
    <p class="dl-trail" role="status">Lost the live trail - reconnecting. The shaping so far is still shown above.</p>
  {/if}
  {#if view.synthesis}
    <div class="dl-synth">
      <span class="dl-synth-head">{view.synthesis.label} is bringing it together</span>
      <p>{view.synthesis.text}{#if !view.synthesis.done}<span class="dl-caret" aria-hidden="true"></span>{/if}</p>
    </div>
  {/if}
</div>

<style>
  .dl { margin: 22px 0 0; }
  .dl-model { display: inline-block; margin-bottom: 14px; font-size: 10.5px; font-weight: 700; letter-spacing: 0.03em; text-transform: uppercase; color: var(--mdx-accent-primary); padding: 2px 8px; border-radius: var(--mdx-radius-pill); border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 30%, transparent); }
  .dl-voices { list-style: none; margin: 0; padding: 0; display: grid; gap: 12px; }
  .dl-voice { display: flex; gap: 12px; animation: rise 0.4s ease both; }
  .dl-icon { flex: none; display: inline-flex; width: 32px; height: 32px; align-items: center; justify-content: center; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-primary) 12%, transparent); color: var(--mdx-accent-primary); }
  .dl-icon svg { width: 16px; height: 16px; }
  .dl-body strong { font-size: 13px; font-weight: 650; }
  .dl-body p { margin: 2px 0 0; font-size: 13px; color: var(--mdx-text-secondary); line-height: 1.5; }
  .dl-voice.thinking { align-items: center; gap: 9px; color: var(--mdx-text-muted); font-size: 12.5px; }
  .dl-dots { display: inline-flex; gap: 3px; }
  .dl-dots i { width: 5px; height: 5px; border-radius: 50%; background: var(--mdx-text-muted); animation: blink 1.2s infinite; }
  .dl-dots i:nth-child(2) { animation-delay: 0.2s; }
  .dl-dots i:nth-child(3) { animation-delay: 0.4s; }
  .dl-synth { margin: 16px 0 0; padding: 13px 16px; border-radius: var(--mdx-radius-lg); background: color-mix(in srgb, var(--mdx-accent-primary) 6%, var(--mdx-surface-base)); border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 18%, transparent); animation: rise 0.4s ease both; }
  .dl-synth-head { font-size: 11px; font-weight: 700; letter-spacing: 0.05em; text-transform: uppercase; color: var(--mdx-accent-primary); }
  .dl-synth p { margin: 6px 0 0; font-size: 13px; color: var(--mdx-text-primary); line-height: 1.5; }
  .dl-trail { margin: 12px 0 0; font-size: 12px; color: var(--mdx-text-muted); line-height: 1.5; }
  .dl-caret { display: inline-block; width: 6px; height: 13px; margin-left: 2px; vertical-align: text-bottom; background: var(--mdx-accent-primary); animation: blink 1s step-end infinite; }
  @keyframes blink { 50% { opacity: 0; } }
  @keyframes rise { from { opacity: 0; transform: translateY(6px); } to { opacity: 1; transform: none; } }
</style>
