<script>
  // Welcome to MDx: the friendly first hello. It introduces the five surfaces
  // in human terms (what you DO with each, not what they are inside), then tells
  // the honest compounding story - how your work becomes shared context the
  // whole system can cite - with a live, receipt-backed readout (zeros on a new
  // workspace, which is the truth). This is the front of the activation journey.
  let { data } = $props();

  const surfaces = [
    {
      name: "Twin",
      href: "/twin",
      tag: "Your helper",
      line: "Ask it anything and it gets things done with you - reads your docs, drafts, decides, and routes work across MDx. It learns your style and can run privately on your own machine.",
      icon: "M4 5h16v10H8l-4 4z"
    },
    {
      name: "Forge",
      href: "/forge",
      tag: "Ships code",
      line: "Hand it a build or a fix. It reads the repo, proposes a plan, and works under your sign-off - one agent or a fleet in parallel - then brings you a branch to review. Nothing ships without you.",
      icon: "M12 2 4 6v6c0 5 3.5 8.5 8 10 4.5-1.5 8-5 8-10V6z"
    },
    {
      name: "Message",
      href: "/message",
      tag: "Coordinates",
      line: "Where people and agents work together. Agents post, you approve or steer with one tap, and every decision lands with its receipt.",
      icon: "M4 5h16v11H8l-4 4z"
    },
    {
      name: "Pages",
      href: "/pages",
      tag: "Remembers",
      line: "What your company knows - decisions, standards, and docs your agents can draw on. Write your coding guidelines once and every build is grounded in them.",
      icon: "M5 4h9l5 5v11H5z"
    },
    {
      name: "Marketplace",
      href: "/marketplace",
      tag: "Extends",
      line: "Add capabilities your agents can safely lean on - each one scanned and scoped before it can act, recommended for the work you actually do.",
      icon: "M4 7h16l-1 5H5z M5 12v7h14v-7 M9 7V5h6v2"
    }
  ];

  const fw = $derived(data.flywheel ?? { outcomes: 0, lessons: 0, installs: 0, memories: 0, cited: false });
  const fwTotal = $derived(fw.outcomes + fw.lessons + fw.installs + fw.memories);

  let explainerEl = $state(null);
  function seeHow() {
    explainerEl?.scrollIntoView({ behavior: "smooth", block: "start" });
  }
</script>

<svelte:head><title>Welcome to MDx</title></svelte:head>

<section class="meet" data-route-state="ready">
  <div class="hero">
    <div class="hero-inner">
      <div class="hero-logo" aria-label="MDx">MD<span>x</span></div>
      <p class="hero-tag">What do you wanna build <em>next</em>?</p>
      <div class="hero-cta">
        <a class="mdx-btn primary hero-go" href="/welcome/setup">Start your journey &rarr;</a>
      </div>
    </div>
    <button type="button" class="hero-explore" onclick={seeHow} aria-label="See how it works">
      <span class="hero-explore-label">Explore</span>
      <span class="hero-explore-line" aria-hidden="true"></span>
    </button>
  </div>

  <div class="explainer" bind:this={explainerEl}>
  <header class="meet-head">
    <span class="eyebrow">The five surfaces</span>
    <h2 class="meet-h2">One system, five ways in.</h2>
    <p class="sub">Each does a real job on its own - and they're far better together. Here's what each one is for.</p>
  </header>

  <ul class="surfaces">
    {#each surfaces as s (s.name)}
      <li class="surface">
        <span class="surface-icon" aria-hidden="true">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d={s.icon} /></svg>
        </span>
        <div class="surface-body">
          <div class="surface-top">
            <p class="surface-name">{s.name}</p>
            <span class="surface-tag">{s.tag}</span>
          </div>
          <p class="surface-line">{s.line}</p>
        </div>
        <a class="surface-open" href={s.href}>Open</a>
      </li>
    {/each}
  </ul>

  <section class="flywheel" aria-label="How MDx compounds">
    <div class="fw-head">
      <span class="eyebrow">The good part</span>
      <h2>It gets more useful the more you use it.</h2>
      <p class="fw-lead">Every surface feeds the others. A build in Forge leaves a lesson. That lesson becomes context your next build - and Twin - can draw on. A decision or a standard in Pages grounds the next plan. Nothing is lost: your work becomes a shared memory the whole system can cite.</p>
    </div>

    <div class="fw-loop" role="img" aria-label="The five surfaces feed each other in a loop">
      {#each surfaces as s, i (s.name)}
        <span class="fw-node">{s.name}</span>
        {#if i < surfaces.length - 1}<span class="fw-arrow" aria-hidden="true">&rarr;</span>{/if}
      {/each}
      <span class="fw-arrow fw-arrow-loop" aria-hidden="true">&#8631;</span>
    </div>

    <div class="fw-readout" aria-live="polite">
      {#if fwTotal === 0}
        <p class="fw-empty">Your workspace is new - so these are zero, honestly. Your first build will leave the first lesson here, and you'll watch the numbers climb as you work.</p>
      {:else}
        <div class="fw-stats">
          <div class="fw-stat"><strong>{fw.outcomes}</strong><span>build outcomes</span></div>
          <div class="fw-stat"><strong>{fw.lessons}</strong><span>{fw.lessons === 1 ? "lesson" : "lessons"} learned</span></div>
          <div class="fw-stat"><strong>{fw.installs}</strong><span>{fw.installs === 1 ? "capability" : "capabilities"}</span></div>
          <div class="fw-stat"><strong>{fw.memories}</strong><span>active {fw.memories === 1 ? "memory" : "memories"}</span></div>
        </div>
        <p class="fw-note">{fw.cited ? "A later build already cited a lesson an earlier one left - the loop is turning." : "Receipt-backed, and growing with every run. Real work, not a demo."}</p>
      {/if}
    </div>
  </section>

  <div class="meet-cta">
    <a class="mdx-btn primary" href="/welcome/setup">Set up MDx &rarr;</a>
    <a class="meet-skip" href="/twin">or look around first</a>
  </div>
  </div>
</section>

<style>
  .meet { display: block; }
  .hero {
    position: relative;
    min-height: calc(100vh - 88px);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    padding: 32px 24px 96px;
  }
  .hero-inner { max-width: 760px; }
  @keyframes heroRise {
    from { opacity: 0.72; transform: translateY(14px); }
    to { opacity: 1; transform: translateY(0); }
  }
  @keyframes heroFade {
    from { opacity: 0.55; }
    to { opacity: 1; }
  }
  @keyframes heroPulse {
    0%, 100% { opacity: 0.2; }
    50% { opacity: 0.65; }
  }
  .hero-logo {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: clamp(5.5rem, 16vw, 12rem);
    font-weight: 800;
    line-height: 0.9;
    letter-spacing: -0.05em;
    color: var(--mdx-text-primary);
    opacity: 1;
    animation: heroRise 650ms cubic-bezier(0.16, 1, 0.3, 1) 40ms both;
  }
  .hero-logo span { color: var(--mdx-accent-primary); }
  .hero-tag {
    margin: 26px auto 0;
    max-width: 560px;
    color: var(--mdx-text-muted);
    font-size: clamp(16px, 2.2vw, 22px);
    font-weight: 300;
    line-height: 1.45;
    letter-spacing: 0.01em;
    opacity: 1;
    animation: heroRise 560ms ease-out 110ms both;
  }
  .hero-tag em { font-style: normal; font-weight: 400; color: var(--mdx-text-secondary); }
  .hero-cta {
    margin: 36px 0 0;
    display: flex;
    justify-content: center;
    opacity: 1;
    animation: heroRise 560ms ease-out 180ms both;
  }
  .hero-go { height: 46px; padding: 0 28px; font-size: 15px; font-weight: 650; }
  .hero-explore {
    position: absolute;
    bottom: 36px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 11px;
    border: none;
    background: none;
    cursor: pointer;
    padding: 8px;
    opacity: 1;
    animation: heroFade 520ms ease-out 280ms both;
  }
  .hero-explore-label {
    font-family: var(--mdx-font-mono, monospace);
    font-size: 10px;
    letter-spacing: 0.28em;
    text-transform: uppercase;
    color: var(--mdx-text-muted);
    transition: color 0.15s ease;
  }
  .hero-explore-line {
    width: 1px;
    height: 38px;
    background: linear-gradient(to bottom, var(--mdx-text-muted), transparent);
    animation: heroPulse 2.4s ease-in-out infinite;
  }
  .hero-explore:hover .hero-explore-label { color: var(--mdx-text-primary); }
  .explainer {
    max-width: 760px;
    margin: 0 auto;
    padding: 8px 24px 80px;
  }
  .eyebrow {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--mdx-text-muted);
  }
  .meet-h2 {
    margin: 8px 0 0;
    font-family: var(--mdx-font-display);
    font-size: 30px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .meet-head .sub {
    margin: 12px 0 0;
    max-width: 600px;
    color: var(--mdx-text-secondary);
    font-size: 15px;
    line-height: 1.55;
  }
  .surfaces {
    list-style: none;
    margin: 32px 0 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  .surface {
    display: flex;
    align-items: flex-start;
    gap: 14px;
    padding: 16px 18px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }
  .surface-icon {
    flex: none;
    display: inline-flex;
    width: 34px;
    height: 34px;
    align-items: center;
    justify-content: center;
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-primary) 14%, transparent);
    color: var(--mdx-accent-primary);
  }
  .surface-icon svg { width: 18px; height: 18px; }
  .surface-body { flex: 1; min-width: 0; }
  .surface-top {
    display: flex;
    align-items: baseline;
    gap: 9px;
  }
  .surface-name {
    margin: 0;
    font-size: 15px;
    font-weight: 650;
  }
  .surface-tag {
    font-size: 11px;
    font-weight: 600;
    color: var(--mdx-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .surface-line {
    margin: 4px 0 0;
    color: var(--mdx-text-secondary);
    font-size: 13.5px;
    line-height: 1.5;
  }
  .surface-open {
    flex: none;
    align-self: center;
    color: var(--mdx-accent-primary);
    text-decoration: none;
    font-size: 13px;
    font-weight: 600;
  }
  .surface-open:hover { text-decoration: underline; }

  .flywheel {
    margin: 36px 0 0;
    padding: 26px 24px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: color-mix(in srgb, var(--mdx-accent-primary) 5%, var(--mdx-surface-raised));
  }
  .flywheel h2 {
    margin: 8px 0 0;
    font-family: var(--mdx-font-display);
    font-size: 22px;
    font-weight: 680;
  }
  .fw-lead {
    margin: 10px 0 0;
    max-width: 620px;
    color: var(--mdx-text-secondary);
    font-size: 14px;
    line-height: 1.55;
  }
  .fw-loop {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 10px;
    margin: 22px 0 0;
  }
  .fw-node {
    padding: 6px 13px;
    border-radius: var(--mdx-radius-pill, 999px);
    background: var(--mdx-surface-base);
    border: 1px solid var(--mdx-border-subtle);
    font-size: 12.5px;
    font-weight: 600;
  }
  .fw-arrow { color: var(--mdx-text-muted); font-size: 14px; }
  .fw-arrow-loop { color: var(--mdx-accent-primary); font-size: 17px; }
  .fw-readout { margin: 22px 0 0; }
  .fw-empty {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 13px;
    line-height: 1.5;
    max-width: 600px;
  }
  .fw-stats {
    display: flex;
    flex-wrap: wrap;
    gap: 14px 28px;
  }
  .fw-stat { display: grid; gap: 1px; }
  .fw-stat strong {
    font-family: var(--mdx-font-display);
    font-size: 24px;
    font-weight: 700;
    line-height: 1;
  }
  .fw-stat span {
    font-size: 12px;
    color: var(--mdx-text-muted);
  }
  .fw-note {
    margin: 12px 0 0;
    font-size: 12.5px;
    color: var(--mdx-text-muted);
  }

  .meet-cta {
    margin: 36px 0 0;
    display: flex;
    align-items: center;
    gap: 18px;
  }
  .meet-skip {
    color: var(--mdx-text-muted);
    font-size: 13px;
    text-decoration: none;
  }
  .meet-skip:hover { color: var(--mdx-text-primary); }

  @media (max-width: 560px) {
    .surface { flex-direction: column; }
    .surface-open { align-self: flex-start; }
  }

  @media (prefers-reduced-motion: reduce) {
    .hero-logo,
    .hero-tag,
    .hero-cta,
    .hero-explore,
    .hero-explore-line {
      animation: none;
    }
  }
</style>
