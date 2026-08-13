<script>
  import { statusTone, statusLabel } from "../../../lib/consoleStatus.js";

  let { data, form } = $props();

  const forge = $derived(data.forge);
</script>

<svelte:head><title>Forge Admin - MDx Console</title></svelte:head>

<section class="forge-admin" data-route-state={forge.status === "ready" ? "ready" : "quiet"}>
  <header class="mdx-page-head">
    <h1>Forge Admin</h1>
    <p class="mdx-page-sub">
      Clear supported machines for local use, record runtime compatibility, and keep execution authority separate from availability.
    </p>
  </header>

  <div class="admin-bar" aria-label="Forge admin summary">
    <div>
      <strong>{forge.summary.machines}</strong>
      <span>machines</span>
    </div>
    <div>
      <strong>{forge.summary.optionEnabled}</strong>
      <span>options</span>
    </div>
    <div>
      <strong>{forge.summary.cleared}</strong>
      <span>cleared</span>
    </div>
    <div>
      <strong>{forge.summary.liveReady}</strong>
      <span>live-ready</span>
    </div>
    <form method="POST">
      <input type="hidden" name="intent" value="preflight" />
      <button type="submit">Run preflight</button>
    </form>
  </div>

  {#if form}
    <p class="action-result" data-ok={form.ok}>{form.line}</p>
  {/if}

  <section class="boundary">
    <div>
      <p class="eyebrow">Admin boundary</p>
      <h2>Availability is not authority</h2>
      <p>{forge.boundary.line}</p>
    </div>
    <dl>
      <div><dt>External output</dt><dd>held</dd></div>
      <div><dt>Production writes</dt><dd>blocked</dd></div>
      <div><dt>Secret export</dt><dd>blocked</dd></div>
    </dl>
  </section>

  {#if forge.machines.length === 0}
    <section class="empty-state">
      <h2>Waiting for Forge</h2>
      <p>Start the local kernel and this room will show the supported machines for this install.</p>
    </section>
  {:else}
    <section class="machines">
      <header>
        <h2>Supported Machines</h2>
        <span>{forge.summary.smokePassed} runtime smoke passed</span>
      </header>
      <div class="machine-list">
        {#each forge.machines as machine}
          <article class:native={machine.native}>
            <div class="machine-main">
              <div>
                <p class="eyebrow">{machine.protocol}</p>
                <h3>{machine.name}</h3>
                <p>{machine.model}</p>
              </div>
              <span class="mdx-badge" data-tone={statusTone(machine.liveExecutionReady || machine.smokePassed ? "ready" : machine.status)}>
                {machine.native ? "Native" : machine.liveExecutionReady ? "Live-ready" : machine.optionEnabled ? "Option" : "Held"}
              </span>
            </div>

            <p class="machine-line">{machine.readinessLine}</p>

            <dl class="machine-facts">
              <div><dt>Clearance</dt><dd>{machine.clearanceLabel}</dd></div>
              <div><dt>Runtime</dt><dd>{machine.binaryPresent ? machine.binaryName || "present" : "missing"}</dd></div>
              <div><dt>Smoke</dt><dd>{statusLabel(machine.smokeStatus)}</dd></div>
              <div><dt>Accepted</dt><dd>{machine.accepted}/{machine.tasksAttempted}</dd></div>
            </dl>

            {#if machine.versionObserved}
              <p class="version-line">Observed version: <code>{machine.versionObserved}</code></p>
            {/if}

            <div class="machine-actions">
              {#if machine.clearable}
                <form method="POST">
                  <input type="hidden" name="intent" value="clear" />
                  <input type="hidden" name="runner_id" value={machine.id} />
                  <input type="hidden" name="clearance_mode" value="local_companion" />
                  <button type="submit" disabled={machine.clearanceMode === "local_companion"}>Clear companion</button>
                </form>
                <form method="POST">
                  <input type="hidden" name="intent" value="clear" />
                  <input type="hidden" name="runner_id" value={machine.id} />
                  <input type="hidden" name="clearance_mode" value="fleet_eval" />
                  <button type="submit" class="primary" disabled={machine.clearanceMode === "fleet_eval"}>Clear fleet eval</button>
                </form>
              {/if}
              {#if !machine.native}
                <form method="POST">
                  <input type="hidden" name="intent" value="smoke" />
                  <input type="hidden" name="runner_id" value={machine.id} />
                  <button type="submit">Run smoke</button>
                </form>
              {/if}
            </div>

            <p class="action-line">{machine.actionLine}</p>

            {#if machine.receipt}
              <details class="quiet-more">
                <summary>evidence</summary>
                <span><code>{machine.receipt}</code></span>
              </details>
            {/if}
          </article>
        {/each}
      </div>
    </section>
  {/if}

  <footer class="quiet-line">
    <details class="quiet-more">
      <summary>Forge admin routes</summary>
      {#each forge.routes as route}
        <span><code>{route.method} {route.path}</code> - {route.operation}</span>
      {/each}
    </details>
    <details class="quiet-more">
      <summary>Forge sources</summary>
      {#each forge.sources as source}
        <span><strong>{source.label}:</strong> {source.value}</span>
      {/each}
    </details>
    <details class="quiet-more">
      <summary>safe next</summary>
      <span>{data.safeNext}. {forge.boundary.policy}. {forge.boundary.runtimePolicy}</span>
    </details>
  </footer>
</section>

<style>
  .forge-admin {
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
  .admin-bar strong {
    font-family: var(--mdx-font-display);
  }

  h2 {
    font-size: 17px;
    line-height: 1.2;
  }

  h3 {
    font-size: 16px;
    line-height: 1.2;
  }

  button {
    min-height: 34px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }

  button:hover:not(:disabled),
  button.primary {
    border-color: color-mix(in srgb, var(--mdx-text-primary) 26%, var(--mdx-border-subtle));
    background: var(--mdx-text-primary);
    color: var(--mdx-surface);
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  code {
    color: var(--mdx-text-secondary);
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    overflow-wrap: anywhere;
  }

  .eyebrow,
  .admin-bar span,
  dt {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    letter-spacing: 0;
    text-transform: uppercase;
  }

  .admin-bar {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr)) minmax(150px, 0.7fr);
    gap: 1px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    overflow: hidden;
    background: var(--mdx-border-subtle);
  }

  .admin-bar div,
  .admin-bar form {
    display: grid;
    gap: 3px;
    padding: 14px;
    border: 0;
    border-radius: 0;
    background: var(--mdx-surface-raised);
    text-align: left;
  }

  .admin-bar form button {
    width: 100%;
    height: 100%;
    min-height: 44px;
    padding: 0;
    border: 0;
    background: transparent;
    text-align: left;
  }

  .admin-bar form button:hover:not(:disabled) {
    background: transparent;
    color: var(--mdx-text-primary);
  }

  .admin-bar strong {
    font-size: 24px;
  }

  .action-result {
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-secondary);
    font-size: 13px;
  }

  .action-result[data-ok="true"] {
    color: var(--mdx-tone-ok-text);
  }

  .boundary,
  .empty-state {
    display: grid;
    gap: 14px;
    padding: 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .boundary {
    grid-template-columns: minmax(0, 1fr) minmax(260px, 0.45fr);
  }

  .boundary > div {
    display: grid;
    gap: 8px;
  }

  .boundary p,
  .empty-state p,
  .machine-main p,
  .machine-line,
  .action-line,
  .version-line,
  .machines header span,
  .quiet-line {
    color: var(--mdx-text-muted);
    font-size: 13px;
    line-height: 1.45;
  }

  .boundary dl,
  .machine-facts {
    display: grid;
    gap: 8px;
  }

  dd {
    overflow-wrap: anywhere;
  }

  .machines {
    display: grid;
    gap: 12px;
  }

  .machines > header {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    padding-top: 14px;
    border-top: 2px solid var(--mdx-border-subtle);
  }

  .machine-list {
    display: grid;
    gap: 10px;
  }

  .machine-list article {
    display: grid;
    gap: 13px;
    padding: 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .machine-list article.native {
    background: color-mix(in srgb, var(--mdx-surface-raised) 92%, var(--mdx-text-primary));
  }

  .machine-main {
    display: flex;
    justify-content: space-between;
    gap: 16px;
  }

  .machine-main > div {
    display: grid;
    gap: 5px;
  }

  .machine-main .mdx-badge {
    flex: none;
    align-self: start;
  }

  .machine-facts {
    grid-template-columns: repeat(4, minmax(0, 1fr));
    padding-top: 12px;
    border-top: 1px solid var(--mdx-border-subtle);
  }

  .machine-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .machine-actions form {
    margin: 0;
  }

  .machine-actions button {
    padding: 7px 11px;
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
    max-width: 820px;
  }

  @media (max-width: 860px) {
    .admin-bar,
    .boundary,
    .machine-facts {
      grid-template-columns: 1fr;
    }

    .machine-main {
      display: grid;
    }
  }
</style>
