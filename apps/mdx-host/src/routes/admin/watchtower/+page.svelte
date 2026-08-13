<script>
  // The day-one operator pulse: one calm read that answers is beta healthy,
  // what is blocked, what changed recently, and what to look at next. Every
  // number is the kernel's own safe count; nothing private is read or shown.
  let { data } = $props();

  const pulse = $derived(data.pulse);
  const metrics = $derived(data.httpMetrics);
  const healthy = $derived(pulse?.health === "STEADY");

  function rows(list) {
    return Array.isArray(list) ? list : [];
  }
</script>

<section class="watchtower" data-route-state={pulse ? "ready" : "quiet"}>
  <header class="wt-head mdx-page-head">
    <h1>Watchtower</h1>
    {#if pulse}
      <p class="mdx-page-sub">{pulse.summary}</p>
      <span class="mdx-badge" data-tone={healthy ? "ok" : "warn"}>{healthy ? "Steady" : "Needs a look"}</span>
    {:else}
      <p class="mdx-page-sub">Start the local server and this page fills with the real numbers - never a guess.</p>
    {/if}
  </header>

  {#if pulse}
    <div class="wt-grid">
      <article class="wt-card">
        <h2>Is beta healthy?</h2>
        <p class="wt-big" class:ok={healthy}>{healthy ? "Steady" : "Attention"}</p>
        <p class="wt-line">{pulse.is_beta_healthy?.escalated_to_human ?? 0} {(pulse.is_beta_healthy?.escalated_to_human ?? 0) === 1 ? "item" : "items"} escalated to a human.</p>
      </article>

      <article class="wt-card">
        <h2>What's blocked <span class="mdx-badge" data-tone="bydesign">by design</span></h2>
        <p class="wt-line">{pulse.what_is_blocked?.refusals_total ?? 0} {(pulse.what_is_blocked?.refusals_total ?? 0) === 1 ? "refusal" : "refusals"} recorded by design.</p>
        {#if rows(pulse.what_is_blocked?.by_kind).length > 0}
          <ul class="wt-list">
            {#each rows(pulse.what_is_blocked.by_kind) as row}
              <li><span class="wt-key">{row.kind}</span><span class="wt-count">{row.count}</span></li>
            {/each}
          </ul>
        {:else}
          <p class="wt-muted">No refusals recorded on this machine.</p>
        {/if}
        <p class="wt-muted">Auth refusals at the edge are not recorded as a runtime signal yet - watch the server logs for them.</p>
      </article>

      <article class="wt-card">
        <h2>Queue pressure</h2>
        <div class="wt-stats">
          <div><strong>{pulse.queue_pressure?.admitted ?? 0}</strong><span>admitted</span></div>
          <div><strong>{pulse.queue_pressure?.queued ?? 0}</strong><span>queued</span></div>
          <div><strong>{pulse.queue_pressure?.shed ?? 0}</strong><span>shed</span></div>
          <div><strong>{pulse.queue_pressure?.escalated ?? 0}</strong><span>escalated</span></div>
        </div>
        <p class="wt-muted">Observed-local; queue wait time is not measured yet.</p>
      </article>

      <article class="wt-card">
        <h2>How fast the service answers</h2>
        {#if metrics?.latency_observed}
          <div class="wt-stats">
            <div><strong>{Math.round((metrics.latency_micros?.p50 ?? 0) / 1000 * 10) / 10}ms</strong><span>typical</span></div>
            <div><strong>{Math.round((metrics.latency_micros?.p95 ?? 0) / 1000 * 10) / 10}ms</strong><span>slowest 5%</span></div>
            <div><strong>{metrics.requests_total ?? 0}</strong><span>requests</span></div>
            <div><strong>{metrics.status?.failed_5xx ?? 0}</strong><span>failed</span></div>
          </div>
          {#if (metrics.status?.rate_limited_429 ?? 0) > 0 || (metrics.status?.unauthorized_401 ?? 0) > 0}
            <p class="wt-line">{metrics.status?.unauthorized_401 ?? 0} unauthorized, {metrics.status?.rate_limited_429 ?? 0} rate-limited - refused by design.</p>
          {/if}
        {:else}
          <p class="wt-muted">No requests measured yet this run - the numbers start with the first one.</p>
        {/if}
      </article>

      <article class="wt-card">
        <h2>Feedback</h2>
        <p class="wt-line">{pulse.feedback?.total ?? 0} {(pulse.feedback?.total ?? 0) === 1 ? "note" : "notes"} in.</p>
        {#if rows(pulse.feedback?.by_surface).length > 0}
          <ul class="wt-list">
            {#each rows(pulse.feedback.by_surface) as row}
              <li><span class="wt-key">{row.surface}</span><span class="wt-count">{row.count}</span></li>
            {/each}
          </ul>
        {/if}
        <p class="wt-muted">Counts only. The note text is never read or shown.</p>
      </article>

      <article class="wt-card">
        <h2>Posture</h2>
        <ul class="wt-flags">
          <li>Providers: {pulse.provider?.posture ?? "gated"} - off until a human turns one on.</li>
          <li>Security: presence-only, no stored keys, sandbox hardened.</li>
          <li>Readiness: proven by <code>{pulse.readiness?.proven_by}</code>.</li>
        </ul>
      </article>

      <article class="wt-card">
        <h2>What changed recently</h2>
        <p class="wt-line">{pulse.what_changed_recently?.total_receipts ?? 0} recorded so far.</p>
        {#if rows(pulse.what_changed_recently?.recent_by_kind).length > 0}
          <ul class="wt-list">
            {#each rows(pulse.what_changed_recently.recent_by_kind) as row}
              <li><span class="wt-key">{row.kind}</span><span class="wt-count">{row.count}</span></li>
            {/each}
          </ul>
        {/if}
      </article>
    </div>
  {/if}

  <nav class="wt-next" aria-label="Look at next">
    <h2>Look at next</h2>
    <ul>
      {#each rows(pulse?.look_at_next) as link}
        <li><a href={link.href}>{link.label}</a></li>
      {:else}
        <li><a href="/admin/platform">Platform - what this install runs on and what turns on next</a></li>
        <li><a href="/admin/aegis">Security - what's watching, what it found, what's blocked by design</a></li>
      {/each}
    </ul>
  </nav>

  <p class="wt-boundary">{data.boundary}</p>
</section>

<style>
  .watchtower {
    max-width: 1040px;
    color: var(--mdx-text-primary);
  }
  .wt-head {
    margin-bottom: 20px;
  }
  .wt-head .mdx-badge {
    align-self: flex-start;
    margin-top: 2px;
  }
  .wt-card h2 .mdx-badge {
    vertical-align: 2px;
    margin-left: 4px;
  }
  .wt-line {
    margin: 4px 0;
    color: var(--mdx-text-secondary);
  }
  .wt-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 14px;
  }
  .wt-card {
    border: 1px solid var(--mdx-border-subtle, rgba(127, 127, 127, 0.2));
    border-radius: var(--mdx-radius-md, 12px);
    background: var(--mdx-surface-raised);
    padding: 16px;
  }
  .wt-card h2 {
    margin: 0 0 8px;
    font-size: 0.95rem;
  }
  .wt-big {
    margin: 0 0 6px;
    font-family: var(--mdx-font-display);
    font-size: 1.4rem;
    color: var(--mdx-text-muted);
  }
  .wt-big.ok {
    color: var(--mdx-tone-ok-text);
  }
  .wt-muted {
    margin: 6px 0 0;
    font-size: var(--mdx-text-xs, 0.74rem);
    color: var(--mdx-text-muted);
  }
  .wt-list {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }
  .wt-list li {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    font-size: 0.8rem;
  }
  .wt-key {
    color: var(--mdx-text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .wt-count {
    color: var(--mdx-text-primary);
    font-variant-numeric: tabular-nums;
  }
  .wt-stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 8px;
  }
  .wt-stats strong {
    display: block;
    font-size: 1.25rem;
    font-variant-numeric: tabular-nums;
  }
  .wt-stats span {
    font-size: var(--mdx-text-xs, 0.7rem);
    color: var(--mdx-text-muted);
  }
  .wt-flags {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
    font-size: 0.82rem;
    color: var(--mdx-text-secondary);
  }
  .wt-flags code {
    font-family: var(--mdx-font-mono, monospace);
    font-size: 0.78rem;
  }
  .wt-next {
    margin-top: 18px;
  }
  .wt-next h2 {
    font-size: 0.95rem;
    margin: 0 0 8px;
  }
  .wt-next ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
  }
  .wt-next a {
    color: var(--mdx-accent-primary, #2563eb);
    text-decoration: none;
  }
  .wt-next a:hover {
    text-decoration: underline;
  }
  .wt-boundary {
    margin-top: 18px;
    font-size: var(--mdx-text-xs, 0.72rem);
    color: var(--mdx-text-muted);
  }
</style>
