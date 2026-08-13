<script>
  // Quality: the checks the company runs on itself, and how they came out.
  // The suites are always here - they are the bar this place holds itself
  // to. The verdicts are not, until the loops actually run. When there are
  // none, the page says so warmly: the first verdict lands here. It never
  // shows a green report no run earned.
  let { data } = $props();

  const pulse = $derived.by(() => {
    if (!data.verdictsKnown) {
      return "These are the checks this company runs on itself - one gate for each loop it depends on. Connect MDx and the verdicts fill in, from real runs, never from hope.";
    }
    if (data.checkedCount === 0) {
      return "Every gate is registered and waiting. The first verdict lands here the moment a loop runs and leaves its receipt.";
    }
    if (data.checkedCount === data.suites.length) {
      return "Every gate has a run on the record. The company has checked all of its own work, and the receipts back it up.";
    }
    return `${data.checkedCount} of ${data.suites.length} gates have a run on the record. The rest are not failing - they simply have not run yet.`;
  });
</script>

<svelte:head><title>Quality - MDx</title></svelte:head>

<section class="evals" data-route-state="ready">
  <header class="evals-head mdx-page-head">
    <h1>Quality</h1>
    <p class="mdx-page-sub">The checks this company runs on itself — and how they came out.</p>
  </header>

  <p class="pulse">{pulse}</p>

  <div class="e-section">
    <h2>The gates</h2>
    <p class="section-cue">
      One gate for each loop the company runs. A gate with a run on record has been checked; a gate
      without one is honestly not-yet-run, never quietly passed.
    </p>
    <ul class="suites">
      {#each data.suites as suite}
        <li class="suite">
          <div class="suite-head">
            <div class="suite-title">
              <span
                class="verdict-dot"
                class:checked={data.verdictsKnown && suite.runCount > 0}
                aria-hidden="true"
              ></span>
              <strong>{suite.name}</strong>
            </div>
            <span class="verdict" class:checked={data.verdictsKnown && suite.runCount > 0}>
              {#if !data.verdictsKnown}
                awaiting first run
              {:else if suite.runCount > 0}
                checked
              {:else}
                not yet run
              {/if}
            </span>
          </div>
          <p class="suite-line">{suite.line}</p>
          <p class="suite-count">
            {suite.checkCount} {suite.checkCount === 1 ? "check" : "checks"} in this gate
            {#if data.verdictsKnown && suite.runCount > 0}
              · {suite.runCount} {suite.runCount === 1 ? "run" : "runs"} on record
            {/if}
          </p>
          <details class="quiet-more">
            <summary>what this gate checks</summary>
            <ul class="checks">
              {#each suite.checks as check}
                <li>
                  <span class="check-phrase">{check.phrase}</span>
                  <code class="check-token">{check.token}</code>
                </li>
              {/each}
            </ul>
            {#if data.verdictsKnown && suite.lastRuns.length > 0}
              <dl class="facts">
                <div>
                  <dt>Recent runs</dt>
                  <dd>
                    {#each suite.lastRuns as run}
                      <code>{run}</code>
                    {/each}
                  </dd>
                </div>
              </dl>
            {/if}
          </details>
        </li>
      {/each}
    </ul>
  </div>

  <footer class="quiet-line">
    A gate is only green once a real run says so — this page never grades on a curve.
    <details class="quiet-more">
      <summary>more</summary>
      <span>Boundary: {data.boundary}. Safe next move: {data.safeNext}.</span>
    </details>
  </footer>
</section>

<style>
  .evals {
    display: grid;
    gap: 28px;
    align-content: start;
    max-width: 720px;
    margin: 0 auto;
    padding: 4vh 0 40px;
  }

  .evals-head {
    display: grid;
    gap: 4px;
  }

  .pulse {
    max-width: 620px;
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 16px;
    line-height: 1.55;
  }

  .e-section {
    display: grid;
    gap: 12px;
  }

  h2 {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 17px;
    font-weight: 650;
  }

  .section-cue {
    max-width: 620px;
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    line-height: 1.55;
  }

  .suites {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .suite {
    display: grid;
    gap: 6px;
    padding: 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .suite-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }

  .suite-title {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }

  .verdict-dot {
    flex: none;
    align-self: center;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    border: 2px solid var(--mdx-border-strong);
  }

  .verdict-dot.checked {
    border: none;
    background: var(--mdx-accent-success);
  }

  .suite-title strong {
    font-family: var(--mdx-font-display);
    font-size: 14.5px;
    font-weight: 650;
  }

  .verdict {
    color: var(--mdx-text-muted);
    font-size: 12px;
    font-weight: 600;
  }

  .verdict.checked {
    color: var(--mdx-accent-success);
  }

  .suite-line {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.5;
  }

  .suite-count {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .checks {
    display: grid;
    gap: 8px;
    margin: 10px 0 0;
    padding: 0;
    list-style: none;
  }

  .checks li {
    display: grid;
    gap: 2px;
  }

  .check-phrase {
    color: var(--mdx-text-secondary);
    font-size: 12px;
    line-height: 1.45;
  }

  .check-token {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .facts {
    display: grid;
    gap: 6px;
    margin: 12px 0 0;
  }

  .facts div {
    display: grid;
    grid-template-columns: 90px minmax(0, 1fr);
    gap: 8px;
  }

  .facts dt {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .facts dd {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0;
  }

  code {
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-secondary);
    overflow-wrap: anywhere;
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

  .quiet-more > summary {
    display: inline;
    color: var(--mdx-text-muted);
    cursor: pointer;
    list-style: none;
    font-size: var(--mdx-text-xs);
  }

  .quiet-more > summary::-webkit-details-marker {
    display: none;
  }

  .quiet-line .quiet-more[open] > summary {
    display: none;
  }

  .suite .quiet-more > summary {
    color: var(--mdx-accent-primary);
    opacity: 0.8;
  }

  @media (prefers-reduced-motion: reduce) {
    * {
      transition: none !important;
    }
  }
</style>
