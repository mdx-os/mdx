<script>
  let { data } = $props();

  const registry = $derived(data.registry);
</script>

<section class="registry" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Registry</h1>
    <p class="mdx-page-sub">The admin plane as components, routes, flows, checks, and evidence.</p>
  </header>

  <div class="summary" aria-label="Registry summary">
    <div>
      <strong>{registry.componentCount}</strong>
      <span>components</span>
    </div>
    <div>
      <strong>{registry.routeCount}</strong>
      <span>routes</span>
    </div>
    <div>
      <strong>{registry.governedWriteCount}</strong>
      <span>governed writes</span>
    </div>
    <div>
      <strong>{registry.marketplace.count}</strong>
      <span>capabilities</span>
    </div>
  </div>

  <div class="layers">
    {@html data.registryHtml}
  </div>

  <section class="marketplace-line">
    <h2>Capability Standing</h2>
    <p>
      {registry.marketplace.approved} approved, {registry.marketplace.review} awaiting review,
      {registry.marketplace.blocked} blocked.
    </p>
  </section>

  <footer class="quiet-line">
    <details class="quiet-more">
      <summary>Registry sources</summary>
      {#each registry.sources as source}
        <span><strong>{source.label}:</strong> {source.value}</span>
      {/each}
    </details>
    <details class="quiet-more">
      <summary>boundary</summary>
      <span>Boundary: {data.boundary}. Safe next move: {data.safeNext}.</span>
    </details>
  </footer>
</section>

<style>
  .registry {
    display: grid;
    gap: 22px;
    max-width: 1040px;
    margin: 0 auto;
    padding: 4vh 0 44px;
  }

  p {
    margin: 0;
  }

  .marketplace-line p,
  .quiet-line {
    color: var(--mdx-text-muted);
    font-size: 13px;
    line-height: 1.45;
  }

  .summary {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 1px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    overflow: hidden;
    background: var(--mdx-border-subtle);
  }

  .summary div {
    display: grid;
    gap: 3px;
    padding: 14px;
    background: var(--mdx-surface-raised);
  }

  .summary strong {
    font-family: var(--mdx-font-display);
    font-size: 24px;
  }

  .summary span {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
  }

  .layers,
  :global(.registry-layer),
  :global(.registry-component),
  .marketplace-line {
    display: grid;
    gap: 14px;
  }

  :global(.registry-layer-head) {
    display: flex;
    justify-content: space-between;
    gap: 18px;
    padding-top: 14px;
    border-top: 2px solid var(--mdx-border-subtle);
  }

  :global(.registry-layer.blue .registry-layer-head) {
    border-color: var(--mdx-accent-primary);
  }

  :global(.registry-layer.gold .registry-layer-head) {
    border-color: var(--mdx-accent-warning);
  }

  :global(.registry-layer.green .registry-layer-head) {
    border-color: var(--mdx-accent-success);
  }

  :global(.registry-components) {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
  }

  :global(.registry-component) {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    padding: 14px;
  }

  .registry :global(h2),
  .registry :global(h3),
  .registry :global(p),
  .registry :global(dl) {
    margin: 0;
  }

  .registry :global(h2),
  .registry :global(h3),
  .registry :global(dd) {
    font-family: var(--mdx-font-display);
  }

  .registry :global(h2) {
    font-size: 18px;
  }

  .registry :global(h3) {
    font-size: 15px;
  }

  .registry :global(p),
  .registry :global(li),
  .registry :global(.registry-layer-head span) {
    color: var(--mdx-text-muted);
    font-size: 12px;
    line-height: 1.45;
  }

  .registry :global(dl) {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }

  .registry :global(dt) {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
  }

  .registry :global(dd) {
    font-size: 18px;
  }

  .registry :global(code),
  .registry :global(.registry-flow) {
    color: var(--mdx-text-secondary);
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    overflow-wrap: anywhere;
  }

  :global(.registry-chips),
  :global(.registry-routes li),
  :global(.registry-sources li),
  :global(.registry-capabilities li),
  .quiet-line {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
  }

  :global(.registry-chips code),
  :global(.registry-receipt) {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    padding: 3px 7px;
    background: var(--mdx-surface-base);
  }

  .registry :global(details) {
    border-top: 1px solid var(--mdx-border-subtle);
    padding-top: 10px;
  }

  .registry :global(summary) {
    color: var(--mdx-text-secondary);
    cursor: pointer;
    font-size: 12px;
  }

  :global(.registry-routes),
  :global(.registry-sources),
  :global(.registry-capabilities) {
    display: grid;
    gap: 7px;
    margin: 10px 0 0;
    padding: 0;
    list-style: none;
  }

  :global(.registry-method.post),
  :global(.registry-receipt) {
    color: var(--mdx-tone-ok-text);
  }

  .marketplace-line {
    padding-top: 18px;
    border-top: 1px solid var(--mdx-border-subtle);
  }

  .quiet-more {
    display: grid;
    gap: 5px;
  }

  @media (max-width: 860px) {
    :global(.registry-components),
    .summary {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @media (max-width: 620px) {
    :global(.registry-components),
    .summary {
      grid-template-columns: 1fr;
    }

    :global(.registry-layer-head) {
      display: grid;
    }
  }
</style>
