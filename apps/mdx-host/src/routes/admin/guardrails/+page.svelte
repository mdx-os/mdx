<script>
  import { statusTone, statusLabel } from "../../../lib/consoleStatus.js";
  let { data } = $props();

  const guardrails = $derived(data.guardrails);
</script>

<section class="guardrails" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Guardrails</h1>
    <p class="mdx-page-sub">
      The access, output, and privacy boundaries that refuse unsafe authority before it can act.
    </p>
  </header>

  <div class="summary" aria-label="Guardrails summary">
    <div>
      <strong>{guardrails.summary.guardrails}</strong>
      <span>guardrails</span>
    </div>
    <div>
      <strong>{guardrails.summary.receiptBackedWrites}</strong>
      <span>receipt-backed writes</span>
    </div>
    <div>
      <strong>{guardrails.summary.blockedCases}</strong>
      <span>blocked cases</span>
    </div>
    <div>
      <strong>{guardrails.summary.liveRefusals ?? "unknown"}</strong>
      <span>live refusals</span>
    </div>
  </div>

  <section class="policy-strip">
    <article>
      <p class="eyebrow">Actor admission</p>
      <span class="mdx-badge status-badge" data-tone={statusTone(guardrails.policies.actorAdmission.status)}>{statusLabel(guardrails.policies.actorAdmission.status)}</span>
      <p>
        {guardrails.policies.actorAdmission.requiredFields.length} fields and
        {guardrails.policies.actorAdmission.requiredControls.length} controls are required before a
        governed mutation.
      </p>
    </article>
    <article>
      <p class="eyebrow">Message noise</p>
      <h2>{guardrails.policies.messageNoise.threshold}+ non-human posts fold</h2>
      <p>{guardrails.policies.messageNoise.line}</p>
    </article>
    <article>
      <p class="eyebrow">Page citation</p>
      <h2>{guardrails.policies.pageCitation.citableListRoute}</h2>
      <p>{guardrails.policies.pageCitation.line}</p>
    </article>
  </section>

  {#each guardrails.groups as group}
    <section class="guardrail-group">
      <header>
        <h2>{group.name}</h2>
        <span>{group.guardrails.length} rails</span>
      </header>
      <div class="rail-grid">
        {#each group.guardrails as rail}
          <article class="rail">
            <div class="rail-main">
              <div>
                <h3>{rail.name}</h3>
                <p>{rail.line}</p>
              </div>
              <span class="rail-count">
                {rail.runtimeCount === null ? "waiting for kernel" : `${rail.runtimeCount} recorded`}
              </span>
            </div>

            <dl class="route-pair">
              {#each rail.routes as route}
                <div>
                  <dt>{route.label}</dt>
                  <dd><code>{route.method} {route.path}</code></dd>
                </div>
              {/each}
            </dl>

            <ul class="chips" aria-label={`${rail.name} receipts and blocked cases`}>
              {#each rail.receiptKinds as receipt}
                <li><code>{receipt}</code></li>
              {/each}
              {#each rail.blocked as blocked}
                <li>{blocked}</li>
              {/each}
            </ul>

            <details class="quiet-more">
              <summary>evidence</summary>
              <span>Check: <code>{rail.check ?? "declared in generated contract"}</code></span>
              <span>Source: <code>{rail.evidence}</code></span>
            </details>
          </article>
        {/each}
      </div>
    </section>
  {/each}

  <footer class="quiet-line">
    <details class="quiet-more">
      <summary>Guardrail sources</summary>
      {#each guardrails.sources as source}
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
  .guardrails {
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
  }

  .policy-strip p,
  .rail p,
  .rail-count,
  .quiet-line,
  .guardrail-group header span {
    color: var(--mdx-text-muted);
    font-size: 13px;
    line-height: 1.45;
  }

  .eyebrow,
  .summary span,
  .route-pair dt {
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
  .policy-strip article,
  .rail {
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

  .policy-strip {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
  }

  .policy-strip article,
  .rail {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
  }

  .policy-strip article {
    display: grid;
    gap: 8px;
    padding: 16px;
  }

  .guardrail-group {
    display: grid;
    gap: 12px;
  }

  .guardrail-group header {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    padding-top: 14px;
    border-top: 2px solid var(--mdx-border-subtle);
  }

  .rail-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  .rail {
    display: grid;
    gap: 14px;
    padding: 16px;
  }

  .rail-main {
    display: grid;
    gap: 10px;
  }

  .route-pair {
    display: grid;
    gap: 8px;
    padding-top: 12px;
    border-top: 1px solid var(--mdx-border-subtle);
  }

  code {
    color: var(--mdx-text-secondary);
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    overflow-wrap: anywhere;
  }

  .chips,
  .quiet-line {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
  }

  .chips {
    list-style: none;
    padding: 0;
  }

  .chips li {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    color: var(--mdx-text-secondary);
    font-size: 12px;
    padding: 5px 8px;
  }

  .quiet-more summary {
    cursor: pointer;
    color: var(--mdx-text-secondary);
  }

  .quiet-more span {
    display: block;
    margin-top: 6px;
    max-width: 760px;
  }

  @media (max-width: 760px) {
    .summary,
    .policy-strip,
    .rail-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
