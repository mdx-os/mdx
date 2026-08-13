<script>
  let { data, form } = $props();

  const rows = $derived(Array.isArray(data.projection?.rows) ? data.projection.rows : []);
  const openRows = $derived(rows.filter((row) => !row.closed_loop_replied));

  const lanes = $derived(
    rows.reduce((acc, row) => {
      const lane = row.lane || "unread";
      acc[lane] = (acc[lane] ?? 0) + 1;
      return acc;
    }, {})
  );

  function laneLabel(lane) {
    return String(lane || "unread").replaceAll("_", " ");
  }

  function lifecycleLabel(row) {
    if (row.forge_run_ref) return "Forge started";
    if (row.closed_loop_replied) return "Closed";
    if (row.closeout_published) return "Closeout";
    if (row.forge_requested) return "Forge handoff";
    if (row.work_shaped) return "Work shaped";
    if (row.assessment_published) return "Assessment";
    if (row.acknowledged) return "Acknowledged";
    return "Captured";
  }
</script>

<svelte:head><title>Feedback - MDx</title></svelte:head>

<section class="fa" data-route-state="ready">
  <header class="fa-head">
    <div class="mdx-page-head">
      <a class="mdx-crumb" href="/product-direction">Product</a>
      <h1>Feedback Autonomy</h1>
      <p class="mdx-page-sub">Every beta note gets a visible fate.</p>
    </div>
    <form method="POST" action="?/run">
      <button type="submit" class="mdx-btn primary" disabled={openRows.length === 0}>Run open loops</button>
    </form>
  </header>

  <div class="metrics" aria-label="Feedback autonomy metrics">
    <div>
      <span>{data.projection?.total_feedback ?? 0}</span>
      <p>captured</p>
    </div>
    <div>
      <span>{data.projection?.processed_count ?? 0}</span>
      <p>closed</p>
    </div>
    <div>
      <span>{data.projection?.open_count ?? 0}</span>
      <p>open</p>
    </div>
    <div>
      <span>{Object.keys(lanes).length}</span>
      <p>lanes</p>
    </div>
  </div>

  {#if form}<p class="flash" class:bad={!form.ok}>{form.line}</p>{/if}

  {#if rows.length === 0}
    <div class="empty">
      <p>No beta feedback has entered the autonomy loop yet.</p>
    </div>
  {:else}
    <div class="lane-strip" aria-label="Lane distribution">
      {#each Object.entries(lanes) as [lane, count]}
        <span data-lane={lane}>{laneLabel(lane)} · {count}</span>
      {/each}
    </div>

    <div class="loops" aria-label="Feedback loops">
      {#each rows as row (row.feedback_ref)}
        <article class="loop" data-lane={row.lane}>
          <div class="loop-main">
            <div>
              <span class="source">{row.surface || "surface"} · {row.category || "feedback"}</span>
              <h2>{row.feedback_ref}</h2>
            </div>
            <span class="mdx-badge" data-tone={row.closed_loop_replied ? "ok" : "neutral"}>{lifecycleLabel(row)}</span>
          </div>
          <div class="trail" aria-label="Loop trail">
            <span class:on={row.acknowledged}>Message ack</span>
            <span class:on={row.assessed}>Assessed</span>
            <span class:on={row.assessment_published}>Assessment Page</span>
            <span class:on={row.work_shaped}>Work item</span>
            <span class:on={row.forge_requested}>Forge handoff</span>
            <span class:on={row.evidence_attached}>Evidence</span>
            <span class:on={row.closeout_published}>Closeout Page</span>
            <span class:on={row.closed_loop_replied}>Reply</span>
          </div>
          <dl class="facts">
            <div><dt>Lane</dt><dd>{laneLabel(row.lane)}</dd></div>
            <div><dt>Assessment</dt><dd>{row.assessment_page_ref || "waiting"}</dd></div>
            <div><dt>Work</dt><dd>{row.work_item_ref || "waiting"}</dd></div>
            <div><dt>Forge</dt><dd>{row.forge_run_ref || (row.forge_requested ? "prepared" : "waiting")}</dd></div>
            <div><dt>Closeout</dt><dd>{row.closeout_page_ref || "waiting"}</dd></div>
          </dl>
        </article>
      {/each}
    </div>
  {/if}

  <footer class="quiet-line">
    {data.boundary}
    <details>
      <summary>more</summary>
      <span>{data.safeNext}</span>
    </details>
  </footer>
</section>

<style>
  .fa {
    display: grid;
    gap: 18px;
    max-width: 1120px;
  }

  .fa-head {
    display: flex;
    justify-content: space-between;
    gap: 18px;
    align-items: flex-start;
  }

  .metrics {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    border-top: 1px solid var(--mdx-border);
    border-bottom: 1px solid var(--mdx-border);
  }

  .metrics div {
    padding: 18px 16px;
    border-right: 1px solid var(--mdx-border);
  }

  .metrics div:last-child {
    border-right: 0;
  }

  .metrics span {
    display: block;
    font-family: var(--mdx-font-display);
    font-size: 30px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }

  .metrics p {
    margin: 4px 0 0;
    color: var(--mdx-text-muted);
    font-size: 13px;
  }

  .flash {
    margin: 0;
    color: var(--mdx-tone-ok-text);
    font-weight: 550;
  }

  .flash.bad {
    color: var(--mdx-tone-danger-text);
  }

  .empty {
    padding: 36px 0;
    color: var(--mdx-text-muted);
    font-size: 15px;
  }

  .lane-strip {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .lane-strip span {
    border: 1px solid var(--mdx-border-default);
    border-radius: 999px;
    padding: 5px 10px;
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    font-weight: 550;
    text-transform: capitalize;
  }

  .loops {
    display: grid;
    gap: 12px;
  }

  .loop {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    padding: 18px;
    background: var(--mdx-surface-raised);
  }

  .loop-main {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    align-items: flex-start;
  }

  .source {
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    text-transform: capitalize;
  }

  h2 {
    margin: 6px 0 0;
    font-family: var(--mdx-font-display);
    font-size: 16px;
    font-weight: 650;
    line-height: 1.2;
    overflow-wrap: anywhere;
  }

  .trail {
    display: grid;
    grid-template-columns: repeat(8, minmax(0, 1fr));
    gap: 6px;
    margin-top: 16px;
  }

  .trail span {
    min-height: 34px;
    border: 1px dashed var(--mdx-border-default);
    border-radius: var(--mdx-radius-sm);
    padding: 8px;
    color: var(--mdx-text-muted);
    font-size: 12px;
    text-align: center;
  }

  .trail span.on {
    border-style: solid;
    border-color: rgba(var(--mdx-success-green-rgb), 0.4);
    color: var(--mdx-tone-ok-text);
  }

  .facts {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 12px;
    margin: 16px 0 0;
  }

  .facts div {
    min-width: 0;
  }

  dt {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  dd {
    margin: 4px 0 0;
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  .quiet-line {
    color: var(--mdx-text-muted);
    font-size: 12.5px;
  }

  .quiet-line details {
    display: inline;
    margin-left: 8px;
  }

  @media (max-width: 820px) {
    .fa-head {
      display: grid;
    }

    .metrics {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .metrics div:nth-child(2) {
      border-right: 0;
    }

    .trail,
    .facts {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .loop-main {
      display: grid;
    }

    .loop-main .mdx-badge {
      justify-self: start;
    }
  }
</style>
