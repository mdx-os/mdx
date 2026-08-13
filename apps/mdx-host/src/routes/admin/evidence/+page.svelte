<script>
  let { data } = $props();

  const evidence = $derived(data.evidence);

  // A status word is system truth, never an alarm when the truth is "set up
  // later" or "nothing yet". Calm tone + human label; only a real fault is
  // danger. Raw value stays one disclosure away.
  function statusTone(raw) {
    const s = String(raw ?? "").toLowerCase();
    if (/(error|failed|unreachable|down|broken|fault)/.test(s)) return "danger";
    if (/(missing|not instrumented|not set|pending|waiting|empty)/.test(s)) return "bydesign";
    if (/(ok|steady|live|ready|observed|complete|checked|healthy|done|pass|signed|verified)/.test(s)) return "ok";
    return "neutral";
  }
  function statusLabel(raw) {
    const s = String(raw ?? "").toLowerCase().trim();
    const map = {
      missing: "Not set up yet",
      "waiting for kernel": "Waiting for the local server",
      empty: "Nothing recorded yet"
    };
    if (map[s]) return map[s];
    return String(raw ?? "").replace(/_/g, " ").replace(/^\w/, (c) => c.toUpperCase());
  }
</script>

<section class="evidence" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Evidence</h1>
    <p class="mdx-page-sub">
      Receipts, checkpoints, evidence chains, and receipt-backed stores in one audit browser.
    </p>
  </header>

  <div class="summary" aria-label="Evidence summary">
    <div>
      <strong>{evidence.summary.runtimeReceipts}</strong>
      <span>runtime receipts</span>
    </div>
    <div>
      <strong>{evidence.summary.checkpoints}</strong>
      <span>checkpoints</span>
    </div>
    <div>
      <strong>{evidence.summary.receiptTables}</strong>
      <span>receipt tables</span>
    </div>
    <div>
      <strong>{evidence.summary.receiptBackedRoutes}/{evidence.summary.evidenceRoutes}</strong>
      <span>evidence routes</span>
    </div>
  </div>

  <section class="runtime">
    <article>
      <p class="eyebrow">Runtime receipts</p>
      <span class="mdx-badge status-badge" data-tone={statusTone(evidence.receipts.status)}>{statusLabel(evidence.receipts.status)}</span>
      {#if evidence.receipts.latest.length > 0}
        <div class="receipt-list">
          {#each evidence.receipts.latest as receipt}
            <span><code>{receipt}</code></span>
          {/each}
        </div>
      {:else}
        <p>Start the local kernel to load recent receipt ids.</p>
      {/if}
    </article>
    <article>
      <p class="eyebrow">Local evidence</p>
      <span class="mdx-badge status-badge" data-tone={statusTone(evidence.localEvidence.status)}>{statusLabel(evidence.localEvidence.status)}</span>
      <dl class="stats">
        <div><dt>Total</dt><dd>{evidence.localEvidence.total}</dd></div>
        <div><dt>Complete</dt><dd>{evidence.localEvidence.complete}</dd></div>
        <div><dt>Stale</dt><dd>{evidence.localEvidence.stale}</dd></div>
      </dl>
    </article>
  </section>

  <section class="checkpoints">
    <header>
      <h2>Evidence Checkpoints</h2>
      <span class="mdx-badge" data-tone={statusTone(evidence.checkpoints.status)}>{statusLabel(evidence.checkpoints.status)}</span>
    </header>
    <div class="checkpoint-grid">
      {#each evidence.checkpoints.checkpoints as checkpoint}
        <article class="checkpoint-card">
          <div class="card-top">
            <h3>{checkpoint.id}</h3>
            <span>{checkpoint.receiptCount} receipts</span>
          </div>
          <p>Receipt: <code>{checkpoint.receipt}</code></p>
          <p>Range: <code>{checkpoint.rangeStart}</code> to <code>{checkpoint.rangeEnd}</code></p>
          <details class="quiet-more">
            <summary>integrity</summary>
            <span>Merkle root: <code>{checkpoint.merkleRoot}</code></span>
            <span>Checkpoint hash: <code>{checkpoint.checkpointHash}</code></span>
            <span>Signed: {checkpoint.signed ? "yes" : "no"}</span>
            <span>Anchored: {checkpoint.anchored ? "yes" : "no"}</span>
          </details>
        </article>
      {/each}
      {#if evidence.checkpoints.checkpoints.length === 0}
        <article class="checkpoint-card">
          <h3>No checkpoint projection loaded</h3>
          <p>Start the local kernel to browse checkpoint records.</p>
        </article>
      {/if}
    </div>
  </section>

  <section class="chains">
    <header>
      <h2>Evidence Chains</h2>
      <span>{evidence.chains.length} contracts</span>
    </header>
    <div class="chain-grid">
      {#each evidence.chains as chain}
        <article class="chain-card">
          <div class="card-top">
            <h3>{chain.name}</h3>
            <span class="mdx-badge" data-tone={statusTone(chain.status)}>{statusLabel(chain.status)}</span>
          </div>
          <p>{chain.source}</p>
          <details class="quiet-more">
            <summary>required receipts</summary>
            {#each chain.requiredReceipts as receipt}
              <span><code>{receipt}</code></span>
            {/each}
          </details>
          <details class="quiet-more">
            <summary>forbidden actions</summary>
            {#each chain.forbiddenActions as action}
              <span><code>{action}</code></span>
            {/each}
          </details>
          <details class="quiet-more">
            <summary>rules</summary>
            {#each chain.rules as rule}
              <span>{rule}</span>
            {/each}
          </details>
        </article>
      {/each}
    </div>
  </section>

  <section class="tables">
    <header>
      <h2>Receipt-backed Stores</h2>
      <span>{evidence.receiptTables.length} tables</span>
    </header>
    <div class="table-grid">
      {#each evidence.receiptTables as table}
        <article class="table-card">
          <h3>{table.table}</h3>
          <p>{table.columns.join(", ")}</p>
          <details class="quiet-more">
            <summary>insert fixture</summary>
            <span><code>{table.fixture}</code></span>
          </details>
        </article>
      {/each}
    </div>
  </section>

  <footer class="quiet-line">
    <details class="quiet-more">
      <summary>Evidence routes</summary>
      {#each evidence.routes as route}
        <span>
          <code>{route.method} {route.path}</code> - {route.operation}
          {#if route.receiptBacked} - receipt-backed{/if}
        </span>
      {/each}
    </details>
    <details class="quiet-more">
      <summary>Evidence sources</summary>
      {#each evidence.sources as source}
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
  .evidence {
    display: grid;
    gap: 22px;
    max-width: 1040px;
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

  h2 {
    font-size: 17px;
    line-height: 1.2;
    overflow-wrap: anywhere;
  }

  /* A status badge sits where a shouted status headline used to: left-
     aligned, its own line. The card eyebrow already names the area. */
  .status-badge {
    justify-self: start;
    align-self: start;
  }

  h3 {
    font-size: 15px;
    line-height: 1.2;
    overflow-wrap: anywhere;
  }

  .runtime p,
  .checkpoint-card p,
  .chain-card p,
  .table-card p,
  .quiet-line,
  .checkpoints header span,
  .chains header span,
  .tables header span {
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

  .runtime,
  .checkpoint-grid,
  .chain-grid,
  .table-grid {
    display: grid;
    gap: 12px;
  }

  .runtime {
    grid-template-columns: 1.2fr 0.8fr;
  }

  .checkpoint-grid,
  .table-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .chain-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .runtime article,
  .checkpoint-card,
  .chain-card,
  .table-card {
    display: grid;
    gap: 12px;
    min-width: 0;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface);
    padding: 16px;
  }

  .checkpoints,
  .chains,
  .tables {
    display: grid;
    gap: 12px;
  }

  .checkpoints header,
  .chains header,
  .tables header,
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

  .stats {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 1px;
    overflow: hidden;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-border-subtle);
  }

  .stats div {
    display: grid;
    gap: 4px;
    min-width: 0;
    padding: 12px;
    background: var(--mdx-bg);
  }

  .stats dd {
    color: var(--mdx-text-primary);
    font-family: var(--mdx-font-display);
    font-size: 20px;
  }

  .receipt-list {
    display: grid;
    gap: 7px;
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
    .evidence {
      padding-top: 2vh;
    }

    .summary,
    .runtime,
    .checkpoint-grid,
    .chain-grid,
    .table-grid {
      grid-template-columns: 1fr;
    }

    .checkpoints header,
    .chains header,
    .tables header,
    .card-top {
      align-items: flex-start;
      flex-direction: column;
    }
  }
</style>
