<script>
  // How this page connects: titles, channels, and decisions first; the
  // raw source ids one tap away under "sources". Pure presentation -
  // the derivation lives in the world-model projection, the wiring in
  // the route.
  let { world, titleFor, openDocument } = $props();

  const hasAny = $derived(
    world.cites.receipts.length +
      world.cites.pages.length +
      world.cited_by_pages.length +
      world.decision_mentions.length +
      world.thread_mentions.length >
      0
  );
</script>

<div class="world-panel" aria-label="How this page connects">
  <p class="world-title">How this page connects</p>
  {#if world.cites.receipts.length > 0 || world.cites.pages.length > 0}
    <p class="world-line">
      <strong>Rests on</strong>
      {#each world.cites.pages as cited (cited)}
        <button type="button" class="world-page" onclick={() => openDocument(cited)}>{titleFor(cited)}</button>
      {/each}
      {#if world.cites.receipts.length > 0}
        <span class="world-mention">{world.cites.receipts.length} recorded {world.cites.receipts.length === 1 ? "source" : "sources"}</span>
      {/if}
    </p>
  {/if}
  {#if world.cited_by_pages.length > 0}
    <p class="world-line">
      <strong>Cited by</strong>
      {#each world.cited_by_pages as citing (citing)}
        <button type="button" class="world-page" onclick={() => openDocument(citing)}>{titleFor(citing)}</button>
      {/each}
    </p>
  {/if}
  {#if world.decision_mentions.length > 0}
    <p class="world-line">
      <strong>Decisions touching this</strong>
      {#each world.decision_mentions.slice(0, 3) as mention (mention.receipt_id)}
        <span class="world-mention">#{mention.channel_id}</span>
      {/each}
    </p>
  {/if}
  {#if world.thread_mentions.length > 0}
    <p class="world-line">
      <strong>In the conversation</strong>
      {#each world.thread_mentions.slice(0, 3) as mention (mention.receipt_id)}
        <span class="world-mention">#{mention.channel_id}: {mention.snippet.slice(0, 60)}</span>
      {/each}
    </p>
  {/if}
  {#if !hasAny}
    <p class="world-line quiet-world">Nothing cites this page yet, and it cites nothing - connections appear here as the record grows.</p>
  {/if}
  {#if world.cites.receipts.length > 0 || world.decision_mentions.length > 0}
    <details class="quiet-more world-sources">
      <summary>sources</summary>
      <span class="world-source-list">
        {#each world.cites.receipts.slice(0, 6) as receipt (receipt)}
          <span class="world-receipt">{receipt}</span>
        {/each}
        {#each world.decision_mentions.slice(0, 3) as mention (mention.receipt_id)}
          <span class="world-receipt">{mention.receipt_id}</span>
        {/each}
      </span>
    </details>
  {/if}
  <p class="world-slots">Forge run links and contradiction checks arrive with their own contracts - never guessed.</p>
</div>

<style>
  .world-panel {
    margin-top: 10px;
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    display: grid;
    gap: 6px;
  }

  .world-title {
    margin: 0;
    font-size: 13.5px;
    font-weight: 600;
  }

  .world-line {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
    display: flex;
    flex-wrap: wrap;
    gap: 6px 10px;
    align-items: baseline;
  }

  .world-line strong {
    color: var(--mdx-text-primary);
  }

  .world-page {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    padding: 1px 10px;
    font-size: var(--mdx-text-xs);
    cursor: pointer;
  }

  .world-receipt {
    color: var(--mdx-text-muted);
    font-size: 11px;
    font-family: var(--mdx-font-mono, monospace);
  }

  .world-mention {
    font-size: var(--mdx-text-xs);
  }

  .quiet-world {
    color: var(--mdx-text-muted);
  }

  .world-slots {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .world-sources {
    font-size: var(--mdx-text-xs);
  }

  .world-source-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding-top: 4px;
  }

  .quiet-more {
    display: inline;
  }

  .quiet-more summary {
    display: inline;
    cursor: pointer;
    color: var(--mdx-text-muted);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .quiet-more[open] summary {
    display: none;
  }
</style>
