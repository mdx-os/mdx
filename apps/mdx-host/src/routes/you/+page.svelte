<script>
  // You: tell MDx how you work, once, and feel it everywhere. The profile
  // card always shows exactly what MDx believes and exactly what travels
  // with your questions - a transparent, editable record, never a hidden
  // model. Everything saves to this machine the moment you touch it.
  import { onMount } from "svelte";
  import {
    roles,
    styleDimensions,
    emptyProfile,
    loadProfile,
    saveProfile,
    loadTheme,
    applyTheme,
    personaIdFrom,
    promptShapeFrom,
    beliefLine
  } from "../../lib/youProfile.js";
  import { loadPersonas, savePersonas, personaIdFor } from "../../lib/twinPersonas.js";
  import { isOperatorSession } from "../../lib/session.js";

  let { data } = $props();

  // The console door shows only for the seats that run the system, the same
  // gate the rail uses. Members never see it; the rail carries it for operators.
  const isOperator = $derived(isOperatorSession(data.session));

  let profile = $state(structuredClone(emptyProfile));
  let theme = $state("system");
  let sampleTitle = $state("");
  let sampleText = $state("");
  let savedFlash = $state(false);
  let flashTimer = null;
  let personas = $state([]);
  let personaName = $state("");
  let personaPurpose = $state("");
  let personaVoice = $state("");

  const personaId = $derived(personaIdFrom(profile));
  const belief = $derived(beliefLine(profile));
  const companionMeta = $derived(
    data.companions.find((entry) => entry.id === profile.defaultCompanionId) ?? null
  );
  const exampleShape = $derived(
    promptShapeFrom(profile, "weigh a decision against what the company has actually done")
  );

  function persist() {
    if (saveProfile(profile)) {
      savedFlash = true;
      clearTimeout(flashTimer);
      flashTimer = setTimeout(() => (savedFlash = false), 1600);
    }
  }

  function setRole(id) {
    profile.role = profile.role === id ? "" : id;
    persist();
  }

  function setStyle(dimension, option) {
    profile.style[dimension] = profile.style[dimension] === option ? "" : option;
    persist();
  }

  function setCompanion(event) {
    profile.defaultCompanionId = event.target.value;
    persist();
  }

  function setTheme(mode) {
    theme = mode;
    applyTheme(mode);
  }

  function addSample() {
    const text = sampleText.trim();
    if (!text) {
      return;
    }
    profile.samples = [
      ...profile.samples,
      { title: sampleTitle.trim() || `Sample ${profile.samples.length + 1}`, text }
    ];
    sampleTitle = "";
    sampleText = "";
    persist();
  }

  function removeSample(index) {
    profile.samples = profile.samples.filter((_, i) => i !== index);
    persist();
  }

  function addPersona() {
    const id = personaIdFor(personaName);
    if (!id || personas.some((entry) => entry.id === id)) {
      return;
    }
    personas = [
      ...personas,
      { id, name: personaName.trim(), purpose: personaPurpose.trim(), voice: personaVoice.trim() }
    ];
    savePersonas(personas);
    personaName = "";
    personaPurpose = "";
    personaVoice = "";
    savedFlash = true;
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => (savedFlash = false), 1600);
  }

  function removePersona(id) {
    personas = personas.filter((entry) => entry.id !== id);
    savePersonas(personas);
    if (profile.defaultCompanionId === id) {
      profile.defaultCompanionId = "";
      persist();
    }
  }

  onMount(() => {
    profile = loadProfile();
    theme = loadTheme();
    personas = loadPersonas();
    return () => clearTimeout(flashTimer);
  });
</script>

<svelte:head><title>You - MDx</title></svelte:head>

<section class="you" data-route-state="ready">
  <header class="you-head mdx-page-head">
    <h1>You</h1>
    <p class="mdx-page-sub">Tell MDx how you work, once. Feel it everywhere.</p>
  </header>

  <div class="belief-card">
    {#if belief || profile.name}
      <p class="belief">
        {#if profile.name}<strong>{profile.name}</strong> — {/if}{belief ||
          "tell MDx a little more and this card fills in."}
      </p>
    {:else}
      <p class="belief">
        This card shows exactly what MDx believes about how you work. It starts empty — and it
        only ever knows what you put here.
      </p>
    {/if}
    {#if personaId}
      <p class="travels">
        Travels with every question you ask: <code>{personaId}</code>
      </p>
    {/if}
    <p class="card-note" class:flash={savedFlash}>
      {savedFlash ? "Saved, on this machine." : "Lives on this machine. Edit anything, any time."}
    </p>
    {#if data.session?.authenticated}
      <p class="card-note">
        Sign-in: {data.identity.appleConnected ? "Google and Apple" : "Google"}.
        {#if !data.identity.appleConnected}
          <a class="console-link" href="/auth/link/apple">
            {data.identity.linkFailed ? "Retry Apple" : "Connect Apple"} &rarr;
          </a>
        {/if}
      </p>
    {/if}
  </div>

  <div class="field">
    <h2>Your name</h2>
    <input
      class="name-input"
      type="text"
      placeholder="What should MDx call you?"
      bind:value={profile.name}
      onchange={persist}
      aria-label="Your name"
    />
  </div>

  <div class="field">
    <h2>Your work</h2>
    <div class="role-grid">
      {#each roles as role}
        <button
          type="button"
          class="role-card"
          class:active={profile.role === role.id}
          onclick={() => setRole(role.id)}
        >
          <strong>{role.label}</strong>
          <span>{role.line}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="field">
    <h2>How you like answers</h2>
    <div class="style-rows">
      {#each styleDimensions as dimension}
        <div class="style-row">
          <span class="style-label">{dimension.label}</span>
          <div class="style-options">
            {#each dimension.options as option}
              <button
                type="button"
                class="style-pick"
                class:active={profile.style[dimension.id] === option.id}
                onclick={() => setStyle(dimension.id, option.id)}
              >
                <strong>{option.label}</strong>
                <span>{option.line}</span>
              </button>
            {/each}
          </div>
        </div>
      {/each}
    </div>
    {#if personaId}
      <p class="shape-preview">
        So a question to your Strategic Advisor frames as:
        <em>“{exampleShape}”</em>
      </p>
    {/if}
  </div>

  <div class="field">
    <h2>Your thinking partner</h2>
    <label class="companion-pick">
      <span>Twin opens with</span>
      <select value={profile.defaultCompanionId} onchange={setCompanion} aria-label="Default companion">
        <option value="">No preference</option>
        {#each data.companions as companion}
          <option value={companion.id}>{companion.name} — {companion.stance}</option>
        {/each}
        {#each personas as persona}
          <option value={persona.id}>{persona.name} — yours</option>
        {/each}
      </select>
    </label>
  </div>

  <div class="field">
    <h2>Your companions</h2>
    <p class="field-cue">
      Twin comes with five ways of thinking. Make your own: name it, say what it's for, and give
      it a voice in your words. It shows up next to the others everywhere you ask.
    </p>
    {#if personas.length > 0}
      <ul class="samples">
        {#each personas as persona}
          <li class="sample">
            <div class="sample-body">
              <strong>{persona.name}</strong>
              <span>{persona.purpose || "No purpose written yet."}</span>
            </div>
            <button type="button" class="sample-remove" onclick={() => removePersona(persona.id)} aria-label="Remove {persona.name}">
              remove
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    <div class="sample-add">
      <input type="text" placeholder="Name — Deal Closer, Devil's Advocate, The Editor..." bind:value={personaName} aria-label="Companion name" />
      <input type="text" placeholder="What are they for? One line." bind:value={personaPurpose} aria-label="Companion purpose" />
      <textarea
        rows="4"
        placeholder={"How do they think and sound? Your words.\nAlways asks what we'd cut first. Blunt about scope, generous about people. Ends with one concrete next step..."}
        bind:value={personaVoice}
        aria-label="Companion voice"
      ></textarea>
      <button type="button" class="sample-save" onclick={addPersona} disabled={!personaIdFor(personaName)}>
        Add to your team
      </button>
    </div>
  </div>

  <div class="field">
    <h2>How you write</h2>
    <p class="field-cue">
      Paste things you've written — a memo, a review, a plan. They stay on this machine, and
      they're ready for the day Twin reads personal context. They never ride your questions
      today, and this page will say so the day that changes.
    </p>
    {#if profile.samples.length > 0}
      <ul class="samples">
        {#each profile.samples as sample, index}
          <li class="sample">
            <div class="sample-body">
              <strong>{sample.title}</strong>
              <span>{sample.text.length > 120 ? `${sample.text.slice(0, 120)}…` : sample.text}</span>
            </div>
            <button type="button" class="sample-remove" onclick={() => removeSample(index)} aria-label="Remove {sample.title}">
              remove
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    <div class="sample-add">
      <input type="text" placeholder="What is it? (optional)" bind:value={sampleTitle} aria-label="Sample title" />
      <textarea rows="3" placeholder="Paste a sample of your writing..." bind:value={sampleText} aria-label="Sample text"></textarea>
      <button type="button" class="sample-save" onclick={addSample} disabled={sampleText.trim().length === 0}>
        Keep it here
      </button>
    </div>
  </div>

  <div class="field">
    <h2>Your voice</h2>
    <p class="field-cue">
      This is what makes Twin sound like you, not like an assistant. Everything here travels
      quietly with your questions — Twin honors it without reciting it.
    </p>
    <label class="voice-label">
      How you think — your principles, in your own words
      <textarea
        rows="5"
        placeholder={"I believe shipping beats perfecting...\nTechnical debt is a choice, not an accident...\nHire for curiosity; skills can be taught..."}
        bind:value={profile.principles}
        onchange={persist}
        aria-label="Your principles"
      ></textarea>
    </label>
    <div class="voice-pair">
      <label class="voice-label">
        Your tone, in a few words
        <input
          type="text"
          placeholder="pragmatic, direct, warm, low-ego"
          bind:value={profile.toneKeywords}
          onchange={persist}
          aria-label="Tone keywords"
        />
      </label>
      <label class="voice-label">
        Words you'd never use
        <input
          type="text"
          placeholder="synergy, circle back, move the needle"
          bind:value={profile.avoidPhrases}
          onchange={persist}
          aria-label="Phrases to avoid"
        />
      </label>
    </div>
    <div class="quirk-row" role="group" aria-label="Voice quirks">
      {#each [
        { id: "casualGreetings", label: "Casual greetings", line: "a “hey” gets a “hey hey”, never a paragraph" },
        { id: "ellipsis", label: "Thinking dots", line: "uses “...” for pacing, like thinking out loud" },
        { id: "arrows", label: "Arrows over bullets", line: "prefers “→” for lists" }
      ] as quirk}
        <button
          type="button"
          class="quirk"
          class:active={profile.quirks[quirk.id]}
          onclick={() => { profile.quirks[quirk.id] = !profile.quirks[quirk.id]; persist(); }}
        >
          <strong>{quirk.label}</strong>
          <span>{quirk.line}</span>
        </button>
      {/each}
    </div>
  </div>

  <div class="field">
    <h2>Appearance</h2>
    <div class="theme-row" role="group" aria-label="Theme">
      {#each [{ id: "system", label: "System" }, { id: "light", label: "Light" }, { id: "dark", label: "Dark" }] as mode}
        <button
          type="button"
          class="theme-pick"
          class:active={theme === mode.id}
          onclick={() => setTheme(mode.id)}
        >
          {mode.label}
        </button>
      {/each}
    </div>
  </div>

  {#if isOperator}
    <div class="console-row">
      <div>
        <p class="console-line">Running the system?</p>
        <p class="console-sub">Setup, access, health, and controls live in the console.</p>
      </div>
      <a class="console-link" href="/admin">Open the console &rarr;</a>
    </div>
  {/if}

  <footer class="quiet-line">
    Your preferences stay on this machine. Connected sign-ins stay with your MDx account.
    <details class="quiet-more">
      <summary>more</summary>
      <span>Boundary: {data.boundary}. Safe next move: {data.safeNext}.</span>
    </details>
  </footer>
</section>

<style>
  .console-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 14px 16px;
    border-radius: var(--mdx-radius-md);
    border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 18%, transparent);
  }
  .console-line { margin: 0; font-size: 13.5px; font-weight: 650; font-family: var(--mdx-font-display); }
  .console-sub { margin: 2px 0 0; font-size: 12.5px; color: var(--mdx-text-muted); }
  .console-link { font-size: 13px; white-space: nowrap; }

  .you {
    display: grid;
    gap: 30px;
    align-content: start;
    max-width: 680px;
    margin: 0 auto;
    padding: 4vh 0 40px;
  }

  .you-head {
    display: grid;
    gap: 4px;
  }

  .belief-card {
    display: grid;
    gap: 8px;
    padding: 18px 20px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .belief {
    margin: 0;
    color: var(--mdx-text-primary);
    font-size: 15px;
    line-height: 1.5;
  }

  .travels {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 12px;
  }

  .travels code {
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-secondary);
  }

  .card-note {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .card-note.flash {
    color: var(--mdx-accent-success);
  }

  .field {
    display: grid;
    gap: 10px;
  }

  h2 {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 15px;
    font-weight: 650;
  }

  .field-cue {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    line-height: 1.5;
    max-width: 560px;
  }

  .name-input,
  .sample-add input,
  .sample-add textarea,
  .voice-label textarea,
  .voice-label input {
    padding: 10px 14px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font-family: inherit;
  }

  .name-input {
    max-width: 360px;
    font-size: 14px;
  }

  .name-input:focus-within {
    border-color: var(--mdx-accent-primary);
  }

  .role-grid,
  .style-options,
  .voice-pair {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }

  .role-card,
  .style-pick,
  .quirk {
    display: grid;
    gap: 4px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    text-align: left;
    cursor: pointer;
  }

  .role-card,
  .style-pick {
    padding: 14px;
  }

  .role-card:hover,
  .style-pick:hover,
  .quirk:hover {
    border-color: var(--mdx-border-strong);
  }

  .role-card.active,
  .style-pick.active,
  .quirk.active {
    border-color: var(--mdx-accent-primary);
    background: color-mix(in srgb, var(--mdx-accent-primary) 7%, var(--mdx-surface-raised));
  }

  .role-card strong,
  .style-pick strong {
    font-size: 13.5px;
  }

  .role-card span,
  .style-pick span {
    color: var(--mdx-text-muted);
    font-size: 12px;
    line-height: 1.4;
  }

  .style-rows {
    display: grid;
    gap: 12px;
  }

  .style-row {
    display: grid;
    gap: 6px;
  }

  .style-label {
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
  }

  .shape-preview {
    margin: 4px 0 0;
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    line-height: 1.5;
  }

  .companion-pick {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--mdx-text-secondary);
    font-size: 13px;
  }

  .companion-pick select {
    padding: 8px 10px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font-family: inherit;
    font-size: 13px;
  }

  .samples {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .sample {
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 10px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }

  .sample-body {
    display: grid;
    gap: 2px;
    min-width: 0;
    flex: 1;
  }

  .sample-body strong {
    font-size: 13px;
  }

  .sample-body span {
    color: var(--mdx-text-muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sample-remove {
    border: none;
    background: none;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    cursor: pointer;
  }

  .sample-remove:hover {
    color: var(--mdx-accent-warning);
  }

  .sample-add {
    display: grid;
    gap: 8px;
  }

  .sample-add input,
  .sample-add textarea,
  .voice-label textarea,
  .voice-label input {
    font-size: 13px;
    resize: vertical;
  }

  .sample-save {
    justify-self: start;
    padding: 8px 16px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font-size: 13px;
    cursor: pointer;
  }

  .sample-save:hover:not(:disabled) {
    border-color: var(--mdx-accent-primary);
  }

  .sample-save:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .voice-label {
    display: grid;
    gap: 6px;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
  }

  .quirk-row {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }

  .quirk {
    padding: 12px 14px;
  }

  .quirk strong {
    font-size: 13px;
  }

  .quirk span {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    line-height: 1.4;
  }

  @media (max-width: 640px) {
    .voice-pair,
    .quirk-row {
      grid-template-columns: 1fr;
    }
  }

  .theme-row {
    display: inline-flex;
    gap: 4px;
    padding: 4px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-sunken);
    justify-self: start;
  }

  .theme-pick {
    padding: 7px 16px;
    border: none;
    border-radius: var(--mdx-radius-pill);
    background: transparent;
    color: var(--mdx-text-muted);
    font-size: 13px;
    cursor: pointer;
  }

  .theme-pick.active {
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
  }

  .quiet-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .quiet-more {
    display: inline;
  }

  .quiet-more summary {
    display: inline;
    color: var(--mdx-text-muted);
    cursor: pointer;
    list-style: none;
  }

  .quiet-more summary::-webkit-details-marker {
    display: none;
  }

  .quiet-more[open] summary {
    display: none;
  }

  @media (max-width: 640px) {
    .role-grid,
    .style-options {
      grid-template-columns: 1fr;
    }
  }
</style>
