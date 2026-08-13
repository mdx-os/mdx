<script>
  let { data } = $props();

  const operations = $derived(data.operations);

  // A kernel status word is system truth, but it should never read as an
  // alarm when the truth is "set up later" or "nothing happened yet". Map
  // each raw status to a calm tone + a human label; only a genuine fault
  // gets the danger tone. The raw value stays one disclosure away.
  function statusTone(raw) {
    const s = String(raw ?? "").toLowerCase();
    if (/(error|failed|unreachable|down|broken|fault)/.test(s)) return "danger";
    if (/(missing|not instrumented|not set|pending|waiting)/.test(s)) return "bydesign";
    if (/(ok|steady|live|ready|observed|complete|checked|healthy|done|pass)/.test(s)) return "ok";
    return "neutral";
  }
  function statusLabel(raw) {
    const s = String(raw ?? "").toLowerCase().trim();
    const map = {
      missing: "Not set up yet",
      "not instrumented": "Not measured yet",
      "waiting for kernel": "Waiting for the local server",
      ok: "OK",
      steady: "Steady",
      observed: "Observed",
      complete: "Complete",
      checked: "Checked"
    };
    if (map[s]) return map[s];
    return String(raw ?? "").replace(/_/g, " ").replace(/^\w/, (c) => c.toUpperCase());
  }
</script>

<section class="operations" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Operations</h1>
    <p class="mdx-page-sub">
      Runtime pulse, evidence state, live-substrate blockers, and proof routes for this install.
    </p>
  </header>

  <div class="summary" aria-label="Operations summary">
    <div>
      <strong>{operations.summary.liveSubstrates}/{operations.summary.substrates}</strong>
      <span>live local</span>
    </div>
    <div>
      <strong>{operations.summary.pendingLive}</strong>
      <span>pending live</span>
    </div>
    <div>
      <strong>{operations.summary.routes}</strong>
      <span>routes</span>
    </div>
    <div>
      <strong>{operations.summary.receiptBackedRoutes}</strong>
      <span>receipt-backed</span>
    </div>
  </div>

  <section class="pulse">
    <div>
      <p class="eyebrow">Runtime pulse</p>
      <span class="mdx-badge status-badge" data-tone={statusTone(operations.pulse.status)}>{statusLabel(operations.pulse.status)}</span>
      <p>{operations.pulse.line}</p>
    </div>
    <dl>
      <div><dt>Refusals</dt><dd>{operations.pulse.refusals}</dd></div>
      <div><dt>Requests</dt><dd>{operations.metrics.requests}</dd></div>
      <div><dt>p95</dt><dd>{operations.metrics.p95Ms}ms</dd></div>
      <div><dt>Failed</dt><dd>{operations.metrics.failed}</dd></div>
    </dl>
  </section>

  <section class="ops-grid">
    <article>
      <p class="eyebrow">Queue</p>
      <span class="mdx-badge status-badge" data-tone={statusTone(operations.queue.status)}>{statusLabel(operations.queue.status)}</span>
      <dl class="stats">
        <div><dt>Admitted</dt><dd>{operations.queue.admitted}</dd></div>
        <div><dt>Queued</dt><dd>{operations.queue.queued}</dd></div>
        <div><dt>Shed</dt><dd>{operations.queue.shed}</dd></div>
        <div><dt>Escalated</dt><dd>{operations.queue.escalated}</dd></div>
      </dl>
    </article>
    <article>
      <p class="eyebrow">Evidence</p>
      <span class="mdx-badge status-badge" data-tone={statusTone(operations.evidence.status)}>{statusLabel(operations.evidence.status)}</span>
      <dl class="stats">
        <div><dt>Total</dt><dd>{operations.evidence.total}</dd></div>
        <div><dt>Complete</dt><dd>{operations.evidence.complete}</dd></div>
        <div><dt>Stale</dt><dd>{operations.evidence.stale}</dd></div>
      </dl>
    </article>
    <article>
      <p class="eyebrow">Activation</p>
      <span class="mdx-badge status-badge" data-tone={statusTone(operations.activation.status)}>{statusLabel(operations.activation.status)}</span>
      <p>{operations.activation.summary}</p>
    </article>
    <article>
      <p class="eyebrow">Load proof</p>
      <span class="mdx-badge status-badge" data-tone={statusTone(operations.loadProof.status)}>{statusLabel(operations.loadProof.status)}</span>
      <p>{operations.loadProof.summary}</p>
    </article>
  </section>

  <section class="telemetry">
    <header>
      <h2>Telemetry & Budgets</h2>
      <span>{operations.telemetry.budgetStance}</span>
    </header>
    <div class="telemetry-grid">
      <article class="telemetry-card">
        <p class="eyebrow">Pipeline Breakdown</p>
        <div class="stage-list">
          {#each operations.telemetry.latencyBreakdown as stage}
            <div class="stage-row">
              <div>
                <strong>{stage.name}</strong>
                <span>{stage.note}</span>
              </div>
              <code>{stage.value} {stage.unit}</code>
              <small>{stage.status}</small>
            </div>
          {/each}
        </div>
      </article>
      <article class="telemetry-card">
        <p class="eyebrow">Advisory Budgets</p>
        <div class="budget-list">
          {#each operations.telemetry.budgets as budget}
            <div>
              <span>{budget.name}</span>
              <code>{budget.measured}/{budget.budget} {budget.unit}</code>
              <small>{budget.stance}</small>
            </div>
          {/each}
        </div>
      </article>
      <article class="telemetry-card">
        <p class="eyebrow">Cost & Tokens</p>
        <span class="mdx-badge status-badge" data-tone={statusTone(operations.telemetry.cost.status)}>{statusLabel(operations.telemetry.cost.status)}</span>
        <p>{operations.telemetry.cost.line}</p>
        {#if operations.telemetry.cost.missing.length > 0}
          <details class="quiet-more">
            <summary>what's not measured yet</summary>
            {#each operations.telemetry.cost.missing as item}
              <span>{item}</span>
            {/each}
          </details>
        {/if}
      </article>
    </div>
  </section>

  <section class="traces">
    <header>
      <h2>Trace Waterfall</h2>
      <span>{operations.telemetry.traceRows.length} rows</span>
    </header>
    <div class="trace-list">
      {#each operations.telemetry.traceRows as trace}
        <article class="trace-row">
          <div>
            <h3>{trace.id}</h3>
            <p>{trace.summary}</p>
          </div>
          <span>{trace.stage}</span>
          <code>{trace.cost}</code>
          {#if trace.receipt}
            <code>{trace.receipt}</code>
          {/if}
        </article>
      {/each}
    </div>
  </section>

  <section class="eval-heatmap">
    <header>
      <h2>Eval Heatmap</h2>
      <span>{operations.telemetry.evals.status}</span>
    </header>
    <p>{operations.telemetry.evals.line}</p>
    <div class="heatmap-grid">
      {#each operations.telemetry.evals.modes as mode}
        <article class="heat-cell" class:checked={mode.status === "checked"}>
          <div>
            <strong>{mode.name}</strong>
            <span>{mode.line}</span>
          </div>
          <dl>
            <div><dt>Runs</dt><dd>{mode.runCount}</dd></div>
            <div><dt>Cases</dt><dd>{mode.checkCount}</dd></div>
          </dl>
          <small>{mode.status}</small>
          {#if mode.receipt}
            <a href={`/api/kernel/receipts/${mode.receipt}`}>{mode.receipt}</a>
          {/if}
        </article>
      {/each}
    </div>
    <div class="eval-cases">
      <h3>Trace to Eval Case</h3>
      {#each operations.telemetry.evals.traceCases as item}
        <article>
          <div>
            <strong>{item.phrase}</strong>
            <span>{item.suite}</span>
          </div>
          <small>{item.status}</small>
          <a href={item.target}>{item.receipt || item.id}</a>
        </article>
      {/each}
    </div>
  </section>

  <section class="substrates">
    <header>
      <h2>Live Substrates</h2>
      <span>{operations.summary.pendingLive} waiting</span>
    </header>
    <div class="substrate-grid">
      {#each operations.substrates as substrate}
        <article class="substrate-card">
          <div class="substrate-top">
            <h3>{substrate.id}</h3>
            <span class="mdx-badge" data-tone={substrate.live ? "ok" : "neutral"}>{substrate.status}</span>
          </div>
          <p>{substrate.localAdapter} -> {substrate.liveAdapter}</p>
          <p>{substrate.turnOnSignal}</p>
          <details class="quiet-more">
            <summary>blocking rails</summary>
            {#each substrate.blockingRails as rail}
              <span><code>{rail}</code></span>
            {/each}
            <span>First proof: <code>{substrate.firstProof}</code></span>
          </details>
        </article>
      {/each}
    </div>
  </section>

  <section class="blockers">
    <header>
      <h2>Runtime Blockers</h2>
      <span>{operations.summary.blockedApps} apps</span>
    </header>
    <div class="blocker-grid">
      {#each operations.blockers as blocker}
        <article class="blocker-card">
          <h3>{blocker.name}</h3>
          <p>{blocker.status}</p>
          <details class="quiet-more">
            <summary>blocked by</summary>
            {#each blocker.blockedBy as source}
              <span><code>{source}</code></span>
            {/each}
          </details>
          <details class="quiet-more">
            <summary>first proofs</summary>
            {#each blocker.proofs as proof}
              <span><code>{proof}</code></span>
            {/each}
          </details>
        </article>
      {/each}
    </div>
  </section>

  <section class="contracts">
    <header>
      <h2>Operations Contract</h2>
      <span>{operations.contracts.handoffMode}</span>
    </header>
    <div class="contract-grid">
      <article>
        <p class="eyebrow">Rules</p>
        <ul>
          {#each operations.contracts.runtimeRules as rule}
            <li>{rule}</li>
          {/each}
        </ul>
      </article>
      <article>
        <p class="eyebrow">CI Workflow</p>
        <p>{operations.contracts.ciWorkflow}</p>
        <ul>
          {#each operations.contracts.ciJobs as job}
            <li>{job}</li>
          {/each}
        </ul>
      </article>
      <article>
        <p class="eyebrow">Handoff</p>
        <p>{operations.contracts.handoffName}</p>
      </article>
    </div>
  </section>

  <footer class="quiet-line">
    <details class="quiet-more">
      <summary>Operation routes</summary>
      {#each operations.routes as route}
        <span><code>{route.method} {route.path}</code> - {route.operation}</span>
      {/each}
    </details>
    <details class="quiet-more">
      <summary>Operation sources</summary>
      {#each operations.sources as source}
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
  .operations {
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
  dd,
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

  .pulse p,
  .ops-grid p,
  .telemetry-card p,
  .stage-row span,
  .trace-row p,
  .eval-heatmap > p,
  .heat-cell span,
  .eval-cases span,
  .substrate-card p,
  .blocker-card p,
  .contracts p,
  .quiet-line,
  .substrates header span,
  .blockers header span,
  .contracts header span {
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

  .summary div,
  .pulse,
  .ops-grid article,
  .telemetry-card,
  .trace-row,
  .heat-cell,
  .eval-cases article,
  .substrate-card,
  .blocker-card,
  .contracts {
    background: var(--mdx-surface-raised);
  }

  .summary div {
    display: grid;
    gap: 3px;
    padding: 14px;
  }

  .summary strong {
    font-size: 24px;
  }

  /* A status badge sits where a shouted status headline used to - left-
     aligned, its own line, calm. The card eyebrow already names the area. */
  .status-badge {
    justify-self: start;
    align-self: start;
  }

  .pulse,
  .contracts {
    display: grid;
    gap: 14px;
    padding: 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
  }

  .pulse {
    grid-template-columns: minmax(0, 1fr) minmax(260px, 0.5fr);
  }

  .pulse > div,
  .ops-grid article,
  .substrate-card,
  .blocker-card {
    display: grid;
    gap: 10px;
  }

  .pulse dl,
  .stats {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
  }

  dd {
    color: var(--mdx-text-secondary);
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  code {
    color: var(--mdx-text-secondary);
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    overflow-wrap: anywhere;
  }

  .ops-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 12px;
  }

  .ops-grid article,
  .telemetry-card,
  .trace-row,
  .heat-cell,
  .eval-cases article,
  .substrate-card,
  .blocker-card {
    padding: 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
  }

  .ops-grid .stats {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .telemetry,
  .traces,
  .eval-heatmap,
  .substrates,
  .blockers {
    display: grid;
    gap: 12px;
  }

  .telemetry > header,
  .traces > header,
  .eval-heatmap > header,
  .substrates > header,
  .blockers > header,
  .contracts > header {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    padding-top: 14px;
    border-top: 2px solid var(--mdx-border-subtle);
  }

  .telemetry-grid {
    display: grid;
    grid-template-columns: 1.4fr 0.8fr 0.8fr;
    gap: 12px;
  }

  .telemetry-card {
    display: grid;
    gap: 12px;
  }

  .stage-list,
  .budget-list,
  .trace-list {
    display: grid;
    gap: 8px;
  }

  .stage-row,
  .budget-list div,
  .trace-row {
    display: grid;
    gap: 8px;
    align-items: center;
    min-width: 0;
    padding: 10px;
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-bg);
  }

  .stage-row {
    grid-template-columns: minmax(0, 1fr) minmax(72px, auto) minmax(84px, auto);
  }

  .stage-row strong,
  .budget-list span,
  .trace-row span {
    color: var(--mdx-text-primary);
    font-size: 13px;
  }

  .stage-row span,
  .budget-list small,
  .trace-row p {
    display: block;
  }

  .trace-row {
    grid-template-columns: minmax(0, 1fr) minmax(100px, auto) minmax(100px, auto) minmax(0, 0.7fr);
  }

  .heatmap-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }

  .heat-cell {
    display: grid;
    gap: 10px;
    min-width: 0;
  }

  .heat-cell.checked {
    border-color: var(--mdx-accent-success);
  }

  .heat-cell strong,
  .eval-cases strong {
    color: var(--mdx-text-primary);
    font-size: 13px;
  }

  .heat-cell dl {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
    margin: 0;
  }

  .heat-cell dl div {
    display: grid;
    gap: 2px;
    padding: 8px;
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-bg);
  }

  .heat-cell dt,
  .heat-cell dd {
    margin: 0;
  }

  .heat-cell dt {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .heat-cell dd {
    color: var(--mdx-text-primary);
    font-family: var(--mdx-font-display);
    font-size: 18px;
    font-weight: 700;
  }

  .heat-cell a,
  .eval-cases a {
    min-width: 0;
    overflow-wrap: anywhere;
    color: var(--mdx-accent-primary);
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    text-decoration: none;
  }

  .eval-cases {
    display: grid;
    gap: 8px;
    padding-top: 8px;
  }

  .eval-cases h3 {
    margin: 0;
    color: var(--mdx-text-primary);
    font-family: var(--mdx-font-display);
    font-size: 14px;
  }

  .eval-cases article {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(104px, auto) minmax(0, 0.8fr);
    gap: 10px;
    align-items: center;
  }

  .substrate-grid,
  .blocker-grid,
  .contract-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  .substrate-top {
    display: flex;
    justify-content: space-between;
    gap: 12px;
  }

  .substrate-top .mdx-badge {
    flex: none;
    align-self: start;
  }

  .contract-grid article {
    display: grid;
    gap: 8px;
  }

  .contract-grid ul {
    display: grid;
    gap: 6px;
    padding: 0;
    list-style: none;
  }

  .contract-grid li {
    color: var(--mdx-text-secondary);
    font-size: 12px;
    line-height: 1.35;
  }

  .quiet-line {
    display: flex;
    flex-wrap: wrap;
    gap: 7px 14px;
  }

  .quiet-more summary {
    cursor: pointer;
    color: var(--mdx-text-secondary);
  }

  .quiet-more span {
    display: block;
    margin-top: 6px;
    max-width: 780px;
  }

  @media (max-width: 900px) {
    .summary,
    .pulse,
    .ops-grid,
    .telemetry-grid,
    .heatmap-grid,
    .substrate-grid,
    .blocker-grid,
    .contract-grid {
      grid-template-columns: 1fr;
    }

    .stage-row,
    .trace-row,
    .eval-cases article {
      grid-template-columns: 1fr;
    }
  }
</style>
