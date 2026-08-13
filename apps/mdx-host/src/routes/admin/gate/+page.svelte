<script>
  import { statusTone, statusLabel } from "../../../lib/consoleStatus.js";
  let { data } = $props();

  const gate = $derived(data.gate);
</script>

<section class="gate" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Gate</h1>
    <p class="mdx-page-sub">
      One map for governed writes, receipt coverage, change gates, and the authority still closed.
    </p>
  </header>

  <div class="summary" aria-label="Gate summary">
    <div>
      <strong>{gate.summary.receiptBackedWrites}/{gate.summary.writes}</strong>
      <span>receipt-backed writes</span>
    </div>
    <div>
      <strong>{gate.summary.exceptions}</strong>
      <span>local exceptions</span>
    </div>
    <div>
      <strong>{gate.summary.changeGates}</strong>
      <span>change gates</span>
    </div>
    <div>
      <strong>{gate.summary.blockedAuthorities}</strong>
      <span>blocked paths</span>
    </div>
  </div>

  <section class="stance">
    <div>
      <p class="eyebrow">Gate stance</p>
      <h2>{gate.stance.name}</h2>
      <p>{gate.stance.source} records the checks before a class of change can move.</p>
    </div>
    <ul>
      {#each gate.stance.rules as rule}
        <li>{rule}</li>
      {/each}
    </ul>
  </section>

  <section class="domains">
    <header>
      <h2>Write Domains</h2>
      <span>{gate.summary.routes} total routes</span>
    </header>
    <div class="domain-grid">
      {#each gate.writeDomains as domain}
        <article class="write-domain-card">
          <div class="card-top">
            <h3>{domain.domain}</h3>
            <span>{domain.receiptBacked}/{domain.total}</span>
          </div>
          <p>{domain.exceptions} local exception routes</p>
          <details class="quiet-more">
            <summary>sample routes</summary>
            {#each domain.routes as route}
              <span><code>{route}</code></span>
            {/each}
          </details>
        </article>
      {/each}
    </div>
  </section>

  <section class="exceptions">
    <header>
      <h2>Local Exceptions</h2>
      <span>{gate.exceptions.length} routes</span>
    </header>
    <div class="exception-grid">
      {#each gate.exceptions as exception}
        <article class="exception-card">
          <p class="eyebrow">{exception.domain}</p>
          <h3>{exception.path}</h3>
          <p>{exception.note}</p>
          <span><code>{exception.operation}</code></span>
        </article>
      {/each}
    </div>
  </section>

  <section class="authority">
    <header>
      <h2>Authority Chain</h2>
      <span>{gate.authorityCards.length} contracts</span>
    </header>
    <div class="authority-grid">
      {#each gate.authorityCards as card}
        <article class="authority-card">
          <div class="card-top">
            <h3>{card.name}</h3>
            <span class="mdx-badge" data-tone={statusTone(card.status)}>{statusLabel(card.status)}</span>
          </div>
          <p>Receipt: <code>{card.receiptKind}</code></p>
          <p>Route: <code>{card.route}</code></p>
          <details class="quiet-more">
            <summary>required before open</summary>
            {#each card.required as item}
              <span><code>{item}</code></span>
            {/each}
          </details>
          <details class="quiet-more">
            <summary>blocked authority</summary>
            {#each card.forbidden as item}
              <span><code>{item}</code></span>
            {/each}
          </details>
          <details class="quiet-more">
            <summary>rules</summary>
            {#each card.rules as rule}
              <span>{rule}</span>
            {/each}
          </details>
        </article>
      {/each}
    </div>
  </section>

  <section class="checks">
    <header>
      <h2>Change Gates</h2>
      <span>{gate.changeGates.length} shown</span>
    </header>
    <div class="check-grid">
      {#each gate.changeGates as check}
        <article class="change-gate-card">
          <h3>{check.id}</h3>
          <p>{check.changeType}</p>
          <details class="quiet-more">
            <summary>required checks</summary>
            {#each check.requiredChecks as required}
              <span><code>{required}</code></span>
            {/each}
          </details>
        </article>
      {/each}
    </div>
  </section>

  <footer class="quiet-line">
    <details class="quiet-more">
      <summary>Gate routes</summary>
      {#each gate.routes as route}
        <span>
          <code>{route.method} {route.path}</code> - {route.operation}
          {#if route.receiptBacked} - receipt-backed{/if}
        </span>
      {/each}
    </details>
    <details class="quiet-more">
      <summary>Gate sources</summary>
      {#each gate.sources as source}
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
  .gate {
    display: grid;
    gap: 22px;
    max-width: 1040px;
    margin: 0 auto;
    padding: 4vh 0 44px;
  }

  h2,
  h3,
  p,
  ul {
    margin: 0;
  }

  h2,
  h3,
  .summary strong {
    font-family: var(--mdx-font-display);
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

  .stance p,
  .stance li,
  .write-domain-card p,
  .exception-card p,
  .authority-card p,
  .change-gate-card p,
  .quiet-line,
  .domains header span,
  .exceptions header span,
  .authority header span,
  .checks header span {
    color: var(--mdx-text-muted);
    font-size: 13px;
    line-height: 1.45;
  }

  .eyebrow,
  .summary span {
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

  .stance,
  .write-domain-card,
  .exception-card,
  .authority-card,
  .change-gate-card {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface);
  }

  .stance {
    display: grid;
    grid-template-columns: minmax(0, 1.2fr) minmax(240px, 0.8fr);
    gap: 18px;
    padding: 20px;
  }

  .stance > div {
    display: grid;
    gap: 8px;
    min-width: 0;
  }

  .stance ul {
    display: grid;
    gap: 8px;
    padding-left: 18px;
  }

  .domains,
  .exceptions,
  .authority,
  .checks {
    display: grid;
    gap: 12px;
  }

  .domains header,
  .exceptions header,
  .authority header,
  .checks header,
  .card-top {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }

  .domain-grid,
  .exception-grid,
  .authority-grid,
  .check-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
  }

  .write-domain-card,
  .exception-card,
  .authority-card,
  .change-gate-card {
    display: grid;
    gap: 12px;
    min-width: 0;
    padding: 16px;
  }

  .authority-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .card-top span,
  .exception-card > span {
    color: var(--mdx-text-muted);
    font-size: 12px;
    overflow-wrap: anywhere;
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

  @media (max-width: 860px) {
    .gate {
      padding-top: 2vh;
    }

    .summary,
    .domain-grid,
    .exception-grid,
    .authority-grid,
    .check-grid,
    .stance {
      grid-template-columns: 1fr;
    }

    .domains header,
    .exceptions header,
    .authority header,
    .checks header,
    .card-top {
      align-items: flex-start;
      flex-direction: column;
    }
  }
</style>
