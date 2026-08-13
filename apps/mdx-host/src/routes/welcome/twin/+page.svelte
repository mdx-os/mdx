<script>
  // Make MDx yours: the Twin personalization step. Captures who you are and how
  // you like to be answered, so Twin sounds like it knows you from the first
  // question. Saves through the receipt-backed activation profile contract,
  // with localStorage as a machine-local fallback when the kernel is not
  // reachable.
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import {
    loadProfileWithServer,
    saveServerProfile,
    roles,
    styleDimensions,
    beliefLine
  } from "../../../lib/youProfile.js";

  let profile = $state(null);
  let saved = $state(false);

  onMount(async () => {
    profile = await loadProfileWithServer();
  });

  function setStyle(dimId, optionId) {
    profile.style = { ...profile.style, [dimId]: optionId };
  }
  function toggleQuirk(key) {
    profile.quirks = { ...profile.quirks, [key]: !profile.quirks[key] };
  }

  const belief = $derived(profile ? beliefLine(profile) : "");
  const ready = $derived(!!(profile?.name?.trim() && profile?.role && profile?.style?.register));

  async function saveAndContinue() {
    if (!profile) return;
    await saveServerProfile(profile);
    saved = true;
    goto("/twin");
  }
  function skip() {
    goto("/twin");
  }
</script>

<svelte:head><title>Make MDx yours</title></svelte:head>

<section class="you" data-route-state="ready">
  {#if profile}
    <header class="you-head">
      <span class="eyebrow">Make MDx yours</span>
      <h1>Tell Twin a little about you.</h1>
      <p class="sub">So it answers in your style from the very first question - and gets sharper the more you work together. This stays on your machine.</p>
    </header>

    <div class="field">
      <label class="flabel" for="you-name">What should Twin call you?</label>
      <input id="you-name" class="finput" type="text" bind:value={profile.name} placeholder="Your name" autocomplete="off" />
    </div>

    <div class="field">
      <span class="flabel">What's your work?</span>
      <div class="chips" role="group" aria-label="Your role">
        {#each roles as r (r.id)}
          <button type="button" class="chip" class:active={profile.role === r.id} onclick={() => (profile.role = r.id)}>
            <strong>{r.label}</strong>
            <span>{r.line}</span>
          </button>
        {/each}
      </div>
    </div>

    {#each styleDimensions as dim (dim.id)}
      <div class="field">
        <span class="flabel">{dim.label}</span>
        <div class="chips two" role="group" aria-label={dim.label}>
          {#each dim.options as opt (opt.id)}
            <button type="button" class="chip" class:active={profile.style[dim.id] === opt.id} onclick={() => setStyle(dim.id, opt.id)}>
              <strong>{opt.label}</strong>
              <span>{opt.line}</span>
            </button>
          {/each}
        </div>
      </div>
    {/each}

    <details class="voice">
      <summary>Add your voice <span>optional - how you sound</span></summary>
      <div class="voice-body">
        <div class="field">
          <label class="flabel" for="you-tone">A few words for your tone</label>
          <input id="you-tone" class="finput" type="text" bind:value={profile.toneKeywords} placeholder="e.g. warm, plain, no fluff" autocomplete="off" />
        </div>
        <div class="field">
          <label class="flabel" for="you-avoid">Phrases to never use</label>
          <input id="you-avoid" class="finput" type="text" bind:value={profile.avoidPhrases} placeholder="e.g. leverage, synergy, circle back" autocomplete="off" />
        </div>
        <div class="field">
          <span class="flabel">Little habits to mirror</span>
          <div class="quirks">
            <button type="button" class="quirk" class:active={profile.quirks.casualGreetings} onclick={() => toggleQuirk("casualGreetings")}>Keep greetings casual</button>
            <button type="button" class="quirk" class:active={profile.quirks.ellipsis} onclick={() => toggleQuirk("ellipsis")}>Think out loud with ...</button>
            <button type="button" class="quirk" class:active={profile.quirks.arrows} onclick={() => toggleQuirk("arrows")}>Use -&gt; over bullets</button>
          </div>
        </div>
      </div>
    </details>

    {#if belief}
      <div class="belief" aria-live="polite">
        <span class="belief-eyebrow">Here's what Twin will know about you</span>
        <p>{belief}</p>
      </div>
    {/if}

    <div class="you-cta">
      <button type="button" class="mdx-btn primary" disabled={!ready} onclick={saveAndContinue}>Save and meet Twin &rarr;</button>
      <button type="button" class="you-skip" onclick={skip}>Skip for now</button>
    </div>
  {/if}
</section>

<style>
  .you {
    max-width: 620px;
    margin: 0 auto;
    padding: 48px 24px 80px;
  }
  .eyebrow {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--mdx-text-muted);
  }
  .you-head h1 {
    margin: 8px 0 0;
    font-family: var(--mdx-font-display);
    font-size: 30px;
    font-weight: 700;
  }
  .you-head .sub {
    margin: 10px 0 0;
    color: var(--mdx-text-secondary);
    font-size: 14.5px;
    line-height: 1.55;
    max-width: 520px;
  }
  .field { margin-top: 26px; }
  .flabel {
    display: block;
    margin-bottom: 8px;
    font-size: 13.5px;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }
  .finput {
    width: 100%;
    height: 42px;
    box-sizing: border-box;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    padding: 0 13px;
    font: inherit;
    font-size: 14px;
    outline: none;
  }
  .finput:focus-visible { border-color: var(--mdx-accent-primary); }
  .chips {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }
  .chip {
    display: grid;
    gap: 2px;
    text-align: left;
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: inherit;
    font: inherit;
    cursor: pointer;
    transition: border-color 0.12s ease, background 0.12s ease;
  }
  .chip strong { font-size: 13.5px; font-weight: 650; }
  .chip span { font-size: 12px; color: var(--mdx-text-muted); }
  .chip:hover { border-color: color-mix(in srgb, var(--mdx-accent-primary) 40%, var(--mdx-border-subtle)); }
  .chip.active {
    border-color: var(--mdx-accent-primary);
    background: color-mix(in srgb, var(--mdx-accent-primary) 10%, var(--mdx-surface-raised));
  }
  .voice {
    margin-top: 26px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }
  .voice > summary {
    cursor: pointer;
    padding: 13px 15px;
    font-size: 13.5px;
    font-weight: 600;
    list-style: none;
    display: flex;
    align-items: baseline;
    gap: 9px;
  }
  .voice > summary span { font-size: 12px; font-weight: 400; color: var(--mdx-text-muted); }
  .voice > summary::-webkit-details-marker { display: none; }
  .voice-body { padding: 0 15px 16px; }
  .voice-body .field:first-child { margin-top: 4px; }
  .quirks { display: flex; flex-wrap: wrap; gap: 7px; }
  .quirk {
    padding: 6px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill, 999px);
    background: transparent;
    color: var(--mdx-text-secondary);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }
  .quirk.active {
    border-color: var(--mdx-accent-primary);
    color: var(--mdx-text-primary);
    background: color-mix(in srgb, var(--mdx-accent-primary) 10%, transparent);
  }
  .belief {
    margin-top: 26px;
    padding: 14px 16px;
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-primary) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 22%, transparent);
  }
  .belief-eyebrow {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--mdx-accent-primary);
  }
  .belief p {
    margin: 5px 0 0;
    font-size: 14px;
    color: var(--mdx-text-primary);
    line-height: 1.5;
  }
  .you-cta {
    margin-top: 30px;
    display: flex;
    align-items: center;
    gap: 18px;
  }
  .you-skip {
    border: none;
    background: none;
    color: var(--mdx-text-muted);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .you-skip:hover { color: var(--mdx-text-primary); }
  @media (max-width: 480px) {
    .chips { grid-template-columns: 1fr; }
  }
</style>
