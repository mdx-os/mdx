<script>
  // First-run orientation for the Build path. An engineer dropping in cold does
  // not know Forge is a governed software factory, not a chatbot - so we lead
  // with that one contrast, make the gates visible, and hand off INTO a real
  // first run (the intake, pre-filled) rather than a cold dashboard.
  let { data } = $props();
  const greeting = $derived(data.ownerName ? `${data.ownerName}, ` : "");

  // The governed run lifecycle, with the two points where a human decides.
  const pipeline = [
    { step: "Intent", note: "You describe the change." },
    { step: "Plan", note: "Forge proposes the work." },
    { step: "Approve", gate: true, note: "You open build authority." },
    { step: "Build", note: "It works, recording every step." },
    { step: "Review", note: "You read the diff and receipts." },
    { step: "Ship", gate: true, note: "Nothing ships until you say so." }
  ];

  // A safe, bounded first run: a review that changes nothing without sign-off,
  // so the engineer sees the whole governed loop without risk. Pre-filled into
  // the intake; they can edit it before approving the plan.
  const firstRunIntent =
    "Review this area for bugs and risky edge cases. For each finding, give a minimal repro and a small fix proposal. Change nothing without sign-off.";
  // The forge front door owns the composer; hand the intent to the surface
  // that reads it (the runs list never did - that link was a dead handoff).
  const startHref = `/forge?intent=${encodeURIComponent(firstRunIntent)}`;
</script>

<section class="onboard">
  <header>
    <p class="eyebrow">Build with MDx</p>
    <h1>Forge isn't a chatbot.</h1>
    <p class="lede">
      {greeting}you don't get an answer here - you get a reviewed, shipped change, with a receipt
      for every step. Forge is a governed software factory. {data.modelName || "Your model"} is
      connected, so your first run can be real.
    </p>
  </header>

  <figure class="pipeline" aria-label="How a Forge run works">
    <ol>
      {#each pipeline as node, i}
        <li class="node" class:gate={node.gate}>
          <span class="dot">{#if node.gate}&#9995;{:else}{i + 1}{/if}</span>
          <span class="step">{node.step}</span>
          <span class="note">{node.note}</span>
        </li>
      {/each}
    </ol>
    <figcaption>The two hands are yours: build authority opens only when you approve, and nothing ships until you decide.</figcaption>
  </figure>

  <div class="why">
    <article>
      <h2>Every step leaves a receipt</h2>
      <p>Intent, plan, checks, denials, and the result are all on the record. You can always see what happened and why.</p>
    </article>
    <article>
      <h2>It works under gates</h2>
      <p>Forge can't quietly build or ship. Authority opens at the points you approve, and stays closed everywhere else.</p>
    </article>
    <article>
      <h2>You review before it lands</h2>
      <p>A run ends with a diff and its evidence for you to accept or send back - not a black-box commit.</p>
    </article>
  </div>

  <div class="start">
    <div class="start-copy">
      <p class="eyebrow">Start here</p>
      <h2>Run your first one.</h2>
      <p>We've drafted a safe, read-only run for you. Approve the plan to watch a governed run end to end - you can edit the intent first.</p>
    </div>
    <a class="primary" href={startHref}>Start your first run &rarr;</a>
  </div>

  <div class="nav">
    <a class="ghost" href="/welcome/path">All paths</a>
    <a class="ghost" href="/forge">Skip to Forge</a>
  </div>
</section>

<style>
  .onboard {
    max-width: 52rem;
    margin: 0 auto;
    padding: 3.5rem 1.5rem 5rem;
    display: grid;
    gap: 1.75rem;
  }
  header {
    display: grid;
    gap: 0.75rem;
  }
  .eyebrow {
    margin: 0;
    font-size: var(--mdx-text-sm);
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--mdx-text-tertiary);
  }
  h1 {
    margin: 0;
    font-family: var(--mdx-font-display);
    color: var(--mdx-text-primary);
    font-size: clamp(2.4rem, 7vw, 4.25rem);
    line-height: 1;
  }
  h2 {
    margin: 0;
    font-family: var(--mdx-font-display);
    color: var(--mdx-text-primary);
    font-size: var(--mdx-text-lg);
  }
  .lede {
    margin: 0;
    max-width: 40rem;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-lg);
    line-height: 1.45;
  }

  /* The run lifecycle, gates made visible. */
  .pipeline {
    margin: 0;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    padding: 1.25rem 1.25rem 1rem;
    display: grid;
    gap: 0.85rem;
  }
  .pipeline ol {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
    gap: 0.5rem;
  }
  .node {
    display: grid;
    gap: 0.3rem;
    align-content: start;
    justify-items: start;
    position: relative;
    padding-top: 0.35rem;
  }
  .dot {
    display: grid;
    place-items: center;
    width: 1.85rem;
    height: 1.85rem;
    border-radius: 50%;
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    font-weight: 700;
  }
  .node.gate .dot {
    background: var(--mdx-accent-soft, var(--mdx-surface-sunken));
    color: var(--mdx-accent-primary);
    box-shadow: inset 0 0 0 1.5px var(--mdx-accent-primary);
  }
  .step {
    font-weight: 700;
    color: var(--mdx-text-primary);
    font-size: var(--mdx-text-sm);
  }
  .node.gate .step {
    color: var(--mdx-accent-primary);
  }
  .note {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    line-height: 1.35;
  }
  .pipeline figcaption {
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    line-height: 1.45;
    border-top: 1px solid var(--mdx-border-subtle);
    padding-top: 0.75rem;
  }

  .why {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.75rem;
  }
  .why article {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    padding: 1rem;
    display: grid;
    gap: 0.4rem;
    align-content: start;
  }
  .why p {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    line-height: 1.5;
  }

  /* The seeded first-run handoff. */
  .start {
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-base);
    padding: 1.4rem;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1.25rem;
    flex-wrap: wrap;
  }
  .start-copy {
    display: grid;
    gap: 0.4rem;
    max-width: 32rem;
  }
  .start-copy p:last-child {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    line-height: 1.45;
  }
  .nav {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
  }
  .primary,
  .ghost {
    border-radius: var(--mdx-radius-pill);
    padding: 0.85rem 1.35rem;
    text-decoration: none;
    font-weight: 700;
    white-space: nowrap;
  }
  .primary {
    color: white;
    background: var(--mdx-accent-primary);
  }
  .ghost {
    color: var(--mdx-text-secondary);
  }

  @media (max-width: 760px) {
    .pipeline ol {
      grid-template-columns: 1fr 1fr;
    }
    .why {
      grid-template-columns: 1fr;
    }
    .start {
      flex-direction: column;
      align-items: stretch;
    }
    .start .primary {
      text-align: center;
    }
    .nav {
      flex-direction: column-reverse;
      align-items: stretch;
    }
    .nav .ghost {
      text-align: center;
    }
  }
</style>
