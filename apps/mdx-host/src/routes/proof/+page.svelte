<script>
  // Proof: the company's memory of what actually happened. The chain count
  // is the hero; the chapters say what kind of record it holds; recent
  // moments show it in people's own words. Receipt ids are the content of
  // this surface, so they stay - as quiet mono text, never as headlines.
  // Read-only by design: nothing here changes the record.
  let { data } = $props();
</script>

<svelte:head><title>Proof - MDx</title></svelte:head>

<section class="observatory" data-route-state="ready">
  <header class="observatory-head">
    <div class="mdx-page-head">
      <h1>Proof</h1>
      <p class="mdx-page-sub">Everything the company did — with the proof one tap away.</p>
    </div>
    <nav class="head-links" aria-label="Related">
      <a href="/quality">Quality &rarr;</a>
      <a href="/changelog">What shipped &rarr;</a>
    </nav>
  </header>

  {#if data.headline}
    <p class="pulse">{data.headline}</p>
  {:else}
    <p class="pulse">
      This is where the company's memory gathers. Take any action elsewhere and its receipt lands
      here — a record you can trust because it can be proven, not because we say so.
    </p>
  {/if}

  {#if data.runLine}
    <p class="run-line">{data.runLine}</p>
  {/if}

  {#if data.chapters.length > 0}
    <div class="x-section">
      <h2>What the record holds</h2>
      <p class="section-cue">Each line is a kind of proof the chain already carries.</p>
      <ul class="chapters">
        {#each data.chapters as chapter}
          <li class="chapter">
            <span class="chapter-label">{chapter.label}</span>
            <span class="chapter-line">{chapter.line}</span>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if data.activity.length > 0}
    <div class="x-section">
      <h2>Lately</h2>
      <p class="section-cue">The most recent moments, newest first, in their own words.</p>
      <ul class="moments">
        {#each data.activity as moment}
          <li class="moment">
            <p class="moment-text">“{moment.text}”</p>
            <p class="moment-meta">{moment.meta}</p>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  <div class="x-section">
    <h2>The chain</h2>
    {#if data.latest.items.length > 0}
      <p class="section-cue">The newest links, said plainly. The raw links sit one tap away.</p>
      <ul class="receipts">
        {#each data.latest.items as receipt}
          <li class="receipt">{receipt.line}</li>
        {/each}
      </ul>
      {#if data.latest.more > 0}
        <p class="more-line">and {data.latest.more.toLocaleString()} more on the chain.</p>
      {/if}
      <details class="quiet-more block-more">
        <summary>view the raw links</summary>
        <ul class="raw-links">
          {#each data.latest.items as receipt}
            <li><code>{receipt.id}</code></li>
          {/each}
        </ul>
      </details>
    {:else if data.reachable}
      <p class="section-cue">
        The chain is empty on this machine — the honest state of a fresh install. Take any action
        and its receipt becomes the first link.
      </p>
    {:else}
      <p class="section-cue">
        Connect MDx and the chain fills in here, link by link, from this workspace's own
        record.
      </p>
    {/if}
  </div>

  {#if data.integrity}
    <p class="integrity-line">
      Evidence checkpointed through {data.integrity.coveredThrough} — {data.integrity.signed ? "signed" : "integrity-verified"}{#if data.integrity.anchored}, anchored outside MDx{:else}, not yet anchored outside MDx{/if}. The record can be proven, not just trusted.
    </p>
  {/if}

  <footer class="quiet-line">
    Read-only on purpose: this page shows the record and never rewrites it.
    <details class="quiet-more">
      <summary>more</summary>
      <span>Boundary: {data.boundary}. Safe next move: {data.safeNext}.</span>
    </details>
  </footer>
</section>

<style>
  .observatory {
    display: grid;
    gap: 28px;
    align-content: start;
    max-width: 720px;
    margin: 0 auto;
    padding: 4vh 0 40px;
  }

  .head-links { display: flex; gap: 14px; font-size: 13px; white-space: nowrap; }
  .observatory-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }
  .pulse {
    max-width: 620px;
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 16px;
    line-height: 1.55;
  }

  .run-line {
    margin: -16px 0 0;
    color: var(--mdx-text-muted);
    font-size: 13px;
  }

  .x-section {
    display: grid;
    gap: 12px;
    padding: 20px;
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
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

  .integrity-line {
    font-size: 0.85rem;
    color: var(--mdx-text-secondary);
    margin: 18px 0 0;
  }
  .chapters {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .chapter {
    display: grid;
    gap: 3px;
    padding: 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .chapter-label {
    font-family: var(--mdx-font-display);
    font-size: 13px;
    font-weight: 650;
  }

  .chapter-line {
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.5;
  }

  .moments {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .moment {
    display: grid;
    gap: 4px;
    padding: 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .moment-text {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 13.5px;
    line-height: 1.5;
  }

  .moment-meta {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 11.5px;
  }

  .receipts {
    display: grid;
    gap: 3px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .raw-links code {
    display: block;
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .raw-links {
    display: grid;
    gap: 4px;
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
  }

  .receipt {
    min-width: 0;
  }

  .more-line {
    margin: 2px 0 0;
    color: var(--mdx-text-muted);
    font-size: 12px;
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
</style>
