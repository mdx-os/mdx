<script>
  import { statusTone, statusLabel } from "../../../lib/consoleStatus.js";
  let { data } = $props();

  const knowledge = $derived(data.knowledge);
</script>

<section class="knowledge" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Pages</h1>
    <p class="mdx-page-sub">
      Pages, citations, gaps, and learning hooks for the company memory layer.
    </p>
  </header>

  <div class="summary" aria-label="Knowledge summary">
    <div>
      <strong>{knowledge.summary.documents}</strong>
      <span>documents</span>
    </div>
    <div>
      <strong>{knowledge.summary.citable}</strong>
      <span>citable</span>
    </div>
    <div>
      <strong>{knowledge.summary.gaps}</strong>
      <span>gaps</span>
    </div>
    <div>
      <strong>{knowledge.summary.receiptBackedRoutes}/{knowledge.summary.routes}</strong>
      <span>routes backed</span>
    </div>
  </div>

  <section class="status-grid">
    <article>
      <p class="eyebrow">Runtime</p>
      <span class="mdx-badge status-badge" data-tone={statusTone(knowledge.readiness.status)}>{statusLabel(knowledge.readiness.status)}</span>
      <p>{knowledge.readiness.summary}</p>
      <p><code>{knowledge.readiness.route}</code></p>
    </article>
    <article>
      <p class="eyebrow">Citation rule</p>
      <span class="mdx-badge status-badge" data-tone={statusTone(knowledge.citations.status)}>{statusLabel(knowledge.citations.status)}</span>
      <p>{knowledge.citations.rule}</p>
      <p><code>{knowledge.citations.route}</code></p>
    </article>
    <article>
      <p class="eyebrow">Stewardship</p>
      <h2>{knowledge.stewardship.stale.length} stale, {knowledge.stewardship.unowned.length} unowned</h2>
      <p>{knowledge.stewardship.lifecycle}</p>
    </article>
  </section>

  <section class="documents">
    <header>
      <h2>Documents</h2>
      <span>{knowledge.documents.length} known</span>
    </header>
    <div class="document-grid">
      {#each knowledge.documents.slice(0, 9) as doc}
        <article class="document-card">
          <div class="card-top">
            <h3>{doc.title}</h3>
            <span>{doc.category}</span>
          </div>
          <p>{doc.summary}</p>
          <dl class="mini-stats">
            <div><dt>Receipts</dt><dd>{doc.receipts.length}</dd></div>
            <div><dt>Charter</dt><dd>{doc.charterEvidence.length}</dd></div>
            <div><dt>Citable</dt><dd>{doc.citable ? "yes" : "no"}</dd></div>
          </dl>
          <details class="quiet-more">
            <summary>source</summary>
            <span><code>{doc.bodyRef}</code></span>
            {#each doc.receipts as receipt}
              <span><code>{receipt}</code></span>
            {/each}
          </details>
        </article>
      {/each}
    </div>
  </section>

  <section class="gaps">
    <header>
      <h2>Gaps & Proposals</h2>
      <span>{knowledge.gaps.length} declared</span>
    </header>
    <div class="gap-grid">
      {#each knowledge.gaps.slice(0, 8) as gap}
        <article class="gap-card">
          <div class="card-top">
            <h3>{gap.title}</h3>
            <span>{gap.source}</span>
          </div>
          <p>{gap.line}</p>
          {#if gap.route}
            <p><code>{gap.route}</code></p>
          {/if}
        </article>
      {/each}
    </div>
  </section>

  <section class="learning">
    <header>
      <h2>Learning Engine</h2>
      <span class="mdx-badge" data-tone={statusTone(knowledge.learning.status)}>{statusLabel(knowledge.learning.status)}</span>
    </header>
    <div class="learning-grid">
      <article class="learning-card">
        <p class="eyebrow">World model</p>
        <h3>{knowledge.learning.projectionRoute}</h3>
        <p>{knowledge.learning.line}</p>
        <details class="quiet-more">
          <summary>relationships</summary>
          {#each knowledge.learning.relationshipTypes as relationship}
            <span>{relationship}</span>
          {/each}
        </details>
      </article>
      <article class="learning-card">
        <p class="eyebrow">Answer shapes</p>
        <h3>{knowledge.learning.answerShapes.length} allowed examples</h3>
        {#each knowledge.learning.answerShapes.slice(0, 4) as shape}
          <p>{shape.question} <code>{shape.documentId}</code></p>
        {/each}
      </article>
      <article class="learning-card">
        <p class="eyebrow">Blocked until contracts land</p>
        <h3>{knowledge.learning.blocked.length} blockers</h3>
        {#each knowledge.learning.blocked as blocker}
          <p>{blocker}</p>
        {/each}
      </article>
    </div>
  </section>

  <section class="shapes">
    <header>
      <h2>Page Shapes</h2>
      <span>{knowledge.shapes.length} templates</span>
    </header>
    <div class="shape-grid">
      {#each knowledge.shapes as shape}
        <article class="shape-card">
          <h3>{shape.title}</h3>
          <p>{shape.line}</p>
          <p>Fresh look every {shape.cadence} days.</p>
        </article>
      {/each}
    </div>
  </section>

  <section class="parity">
    <header>
      <h2>V1 Parity</h2>
      <span>{knowledge.parity.shipped} shipped, {knowledge.parity.shippedDifferently} adapted</span>
    </header>
    <article class="parity-card">
      <p>{knowledge.parity.line}</p>
      {#each knowledge.parity.deferred as entry}
        <p><strong>{entry.id.replaceAll("_", " ")}</strong> - {entry.note}</p>
      {/each}
      <details class="quiet-more">
        <summary>v2 surplus</summary>
        {#each knowledge.parity.surpluses as surplus}
          <span>{surplus}</span>
        {/each}
      </details>
    </article>
  </section>

  <footer class="quiet-line">
    <details class="quiet-more">
      <summary>Knowledge routes</summary>
      {#each knowledge.routes as route}
        <span>
          <code>{route.method} {route.path}</code> - {route.operation}
          {#if route.receiptBacked} - receipt-backed{/if}
        </span>
      {/each}
    </details>
    <details class="quiet-more">
      <summary>Knowledge sources</summary>
      {#each knowledge.sources as source}
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
  .knowledge {
    display: grid;
    gap: 22px;
    max-width: 1080px;
    margin: 0 auto;
    padding: 4vh 0 44px;
  }

  h2,
  h3,
  p,
  dl,
  dd {
    margin: 0;
  }

  h2,
  h3,
  .summary strong {
    font-family: var(--mdx-font-display);
  }

  .status-badge {
    justify-self: start;
    align-self: start;
  }

  h2 {
    font-size: 17px;
    line-height: 1.2;
    overflow-wrap: anywhere;
  }

  h3 {
    font-size: 15px;
    line-height: 1.2;
    overflow-wrap: anywhere;
  }

  article p,
  .quiet-line,
  section > header span {
    color: var(--mdx-text-muted);
    font-size: 13px;
    line-height: 1.45;
  }

  .eyebrow,
  .summary span,
  dt {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    letter-spacing: 0;
    text-transform: uppercase;
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
    gap: 4px;
    min-width: 0;
    padding: 16px;
    background: var(--mdx-surface);
  }

  .summary strong {
    color: var(--mdx-text-primary);
    font-size: 26px;
    line-height: 1;
    overflow-wrap: anywhere;
  }

  .status-grid,
  .document-grid,
  .gap-grid,
  .learning-grid,
  .shape-grid {
    display: grid;
    gap: 12px;
  }

  .status-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .document-grid,
  .gap-grid,
  .shape-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .learning-grid {
    grid-template-columns: 1.1fr 1fr 1fr;
  }

  .status-grid article,
  .document-card,
  .gap-card,
  .learning-card,
  .shape-card,
  .parity-card {
    display: grid;
    gap: 12px;
    min-width: 0;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface);
    padding: 16px;
  }

  .documents,
  .gaps,
  .learning,
  .shapes,
  .parity {
    display: grid;
    gap: 12px;
  }

  section > header,
  .card-top {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }

  .card-top span {
    color: var(--mdx-text-muted);
    font-size: 12px;
    overflow-wrap: anywhere;
  }

  .mini-stats {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 1px;
    overflow: hidden;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-border-subtle);
  }

  .mini-stats div {
    display: grid;
    gap: 4px;
    min-width: 0;
    padding: 10px;
    background: var(--mdx-bg);
  }

  .mini-stats dd {
    color: var(--mdx-text-primary);
    font-family: var(--mdx-font-display);
    font-size: 17px;
  }

  code {
    color: var(--mdx-text-primary);
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    overflow-wrap: anywhere;
  }

  .quiet-more {
    display: grid;
    gap: 7px;
    min-width: 0;
  }

  .quiet-more summary {
    color: var(--mdx-text-primary);
    cursor: pointer;
    font-size: 12px;
    font-weight: 650;
  }

  .quiet-more span {
    display: block;
    color: var(--mdx-text-muted);
    font-size: 12px;
    line-height: 1.45;
    overflow-wrap: anywhere;
  }

  .quiet-line {
    display: grid;
    gap: 10px;
    border-top: 1px solid var(--mdx-border-subtle);
    padding-top: 14px;
  }

  @media (max-width: 900px) {
    .knowledge {
      padding-top: 2vh;
    }

    .summary,
    .status-grid,
    .document-grid,
    .gap-grid,
    .learning-grid,
    .shape-grid {
      grid-template-columns: 1fr;
    }

    section > header,
    .card-top {
      align-items: flex-start;
      flex-direction: column;
    }
  }
</style>
