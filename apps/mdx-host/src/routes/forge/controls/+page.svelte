<script>
  import ForgeView from "../../../lib/ForgeView.svelte";

  let { data } = $props();

  const capacity = $derived(data.controlPlane?.capacity_pulse ?? data.capacity ?? {});
  const pipeline = $derived(data.controlPlane?.pipeline_counts ?? {});
  const sourceRoutes = $derived(Array.isArray(data.controlPlane?.source_routes) ? data.controlPlane.source_routes : []);
  const providers = $derived(Array.isArray(data.providerRouting?.providers) ? data.providerRouting.providers : []);
  const repos = $derived(Array.isArray(data.repos?.repos) ? data.repos.repos : []);

  function count(value) {
    return Number(value ?? 0);
  }

  function plural(countValue, noun) {
    return `${countValue} ${noun}${countValue === 1 ? "" : "s"}`;
  }

  function formatNum(value) {
    return Number(value ?? 0).toLocaleString("en-US");
  }

  // Plain copy for the known backpressure policies; a de-underscored fallback
  // keeps an unknown enum readable rather than raw.
  const POLICY_COPY = {
    reserve_workers_then_queue_then_refuse_overflow:
      "Forge reserves workers, then queues new work, then refuses anything over the limit.",
    queue_then_refuse_overflow: "Forge queues new work, then refuses anything over the limit.",
    refuse_overflow: "Forge refuses new work over the limit."
  };
  function humanizePolicy(policy) {
    const key = String(policy ?? "");
    return POLICY_COPY[key] ?? (key ? `Forge ${key.replace(/_/g, " ")}.` : "not set.");
  }
</script>

<svelte:head><title>Controls - MDx</title></svelte:head>

<ForgeView
  title="Controls"
  subtitle="Rules, limits, and readiness for the work Forge is allowed to run."
>
  <p class="controls-lead">
    These read-only panels fill in as work runs - the capacity in use, the decisions waiting on you, model and repo readiness, and the record behind it all. Nothing here grants authority; it shows the bounds Forge operates under.
  </p>

  <section class="controls-grid" aria-label="Control summary">
    <article class="control-card">
      <span>Worker capacity</span>
      <strong>{count(capacity.worker_capacity) === 0 ? "Idle" : `${count(capacity.active_now)} / ${count(capacity.worker_capacity)}`}</strong>
      <p>{count(capacity.worker_capacity) === 0 ? "No workers running yet" : `${plural(count(capacity.queue_depth), "queued item")} · peak ${count(capacity.peak_active)}`}</p>
    </article>
    <a class="control-card control-card-link" href="/forge/review">
      <span>Human gates</span>
      <strong>{count(data.controlPlane?.needs_you_count)}</strong>
      <p>Plans, reviews, and ships waiting - make the call in Review &rarr;</p>
    </a>
    <article class="control-card">
      <span>Authority</span>
      <strong>{data.controlPlane?.authority_granted ? "Granted" : "Human-gated"}</strong>
      <p>Production writes {data.controlPlane?.production_write_allowed ? "allowed" : "off"} - every ship stays your call.</p>
    </article>
  </section>

  <section class="control-panel">
    <div class="section-head">
      <h2>Operating limits</h2>
      <p>How much Forge takes on at once, and what it does when there is more work than room.</p>
    </div>
    <dl class="fact-grid">
      <div><dt>Work submitted</dt><dd>{formatNum(capacity.submitted_total)}</dd></div>
      <div><dt>Work completed</dt><dd>{formatNum(capacity.completed_total)}</dd></div>
      <div><dt>Queue limit</dt><dd>{formatNum(capacity.queue_depth_limit)}</dd></div>
    </dl>
    {#if capacity.backpressure_policy}
      <p class="limit-policy"><span>When the queue is full</span> {humanizePolicy(capacity.backpressure_policy)}</p>
    {/if}
  </section>

  <section class="control-panel">
    <div class="section-head">
      <h2>Model readiness</h2>
      <p>Connected models can compete for roles; eval results decide more over time.</p>
    </div>
    {#if providers.length}
      <div class="provider-list">
        {#each providers as provider}
          <div class="provider-row" data-ready={provider.credential_available === true}>
            <strong>{provider.label ?? provider.provider_family ?? "Provider"}</strong>
            <span>{provider.credential_available ? "connected for this session" : "not connected"}</span>
          </div>
        {/each}
      </div>
    {:else}
      <p class="empty-line">No models connected yet. Connect one from <a href="/forge/models">Models</a> and you'll see how Forge casts each role here.</p>
    {/if}
  </section>

  <section class="control-panel">
    <div class="section-head">
      <h2>Repo boundaries</h2>
      <p>Repos define where Forge may work and what proof should count.</p>
    </div>
    {#if repos.length}
      <div class="provider-list">
        {#each repos.slice(0, 6) as repo}
          <div class="provider-row">
            <strong>{repo.label ?? repo.repo_id ?? repo.id ?? "Repo"}</strong>
            <span>{repo.primary_language ?? repo.language ?? "language pending"}</span>
          </div>
        {/each}
      </div>
    {:else}
      <p class="empty-line">No repo connected yet. Connect one from <a href="/forge">Build</a> and its language, checks, and proof boundaries show here.</p>
    {/if}
  </section>

  <section class="control-panel quiet">
    <div>
      <h2>Record sources</h2>
      <p>{data.boundary}</p>
    </div>
    {#if sourceRoutes.length}
      <details>
        <summary>View sources</summary>
        <ul>
          {#each sourceRoutes as route}
            <li><code>{route}</code></li>
          {/each}
        </ul>
      </details>
    {/if}
  </section>
</ForgeView>

<style>
  .controls-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }
  .control-card,
  .control-panel {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg, 12px);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    padding: 16px 18px;
    min-width: 0;
    max-width: 100%;
  }
  .control-card {
    display: grid;
    gap: 2px;
  }
  .control-card-link {
    text-decoration: none;
    color: inherit;
    transition: border-color 0.12s ease;
  }
  .control-card-link:hover {
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 45%, var(--mdx-border-subtle));
  }
  .control-card-link p {
    color: var(--mdx-accent-primary);
  }
  .control-card span {
    font-size: 11px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--mdx-text-muted);
  }
  .control-card strong {
    font-family: var(--mdx-font-display);
    font-size: 22px;
    font-weight: 700;
    line-height: 1.1;
  }
  .control-card p,
  .section-head p,
  .empty-line,
  .quiet p {
    margin: 4px 0 0;
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    line-height: 1.45;
  }
  .section-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 12px;
  }
  .section-head h2,
  .quiet h2 {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 15px;
    font-weight: 650;
  }
  .fact-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
    margin: 0;
  }
  .fact-grid div,
  .provider-row {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    padding: 11px 12px;
    background: color-mix(in srgb, var(--mdx-surface-base) 78%, transparent);
  }
  .provider-row[data-ready="true"] {
    border-color: color-mix(in srgb, var(--mdx-accent-success) 30%, var(--mdx-border-subtle));
  }
  dt {
    color: var(--mdx-text-muted);
    font-size: 11.5px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  dd {
    margin: 5px 0 0;
    color: var(--mdx-text-primary);
    font-size: 20px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }
  .limit-policy {
    margin: 12px 0 0;
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.5;
  }
  .limit-policy span {
    display: block;
    margin-bottom: 2px;
    color: var(--mdx-text-muted);
    font-size: 11.5px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .provider-list {
    display: grid;
    gap: 8px;
  }
  .provider-row {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    min-width: 0;
  }
  .provider-row strong {
    color: var(--mdx-text-primary);
    font-size: 13px;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .provider-row span {
    color: var(--mdx-text-muted);
    font-size: 12px;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .provider-row[data-ready="true"] span {
    color: var(--mdx-accent-success);
  }
  .controls-lead {
    margin: 0;
    max-width: 760px;
    color: var(--mdx-text-muted);
    font-size: 13px;
    line-height: 1.5;
  }
  .empty-line {
    color: var(--mdx-text-muted);
  }
  .empty-line a {
    color: var(--mdx-accent-primary);
    text-decoration: none;
  }
  .empty-line a:hover {
    text-decoration: underline;
  }
  .quiet {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    min-width: 0;
  }
  details {
    margin-top: 4px;
  }
  summary {
    cursor: pointer;
    color: var(--mdx-accent-primary);
    font-size: 12.5px;
  }
  code {
    color: var(--mdx-text-muted);
    font-size: 11.5px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    word-break: break-word;
  }
  @media (max-width: 760px) {
    .controls-grid,
    .fact-grid {
      grid-template-columns: 1fr;
    }
    .section-head,
    .provider-row,
    .quiet {
      flex-direction: column;
    }
  }
</style>
