<script>
  import { statusTone, statusLabel } from "../../../lib/consoleStatus.js";
  let { data } = $props();

  const access = $derived(data.access);
</script>

<section class="access" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Access</h1>
    <p class="mdx-page-sub">
      Who is acting, what they can ask for, and which access doors stay locked until live authority exists.
    </p>
  </header>

  <div class="summary" aria-label="Access summary">
    <div>
      <strong>{access.summary.roles}</strong>
      <span>roles</span>
    </div>
    <div>
      <strong>{access.summary.authRoutes}</strong>
      <span>routes</span>
    </div>
    <div>
      <strong>{access.summary.governedWrites}</strong>
      <span>governed writes</span>
    </div>
    <div>
      <strong>{access.summary.blockedAuthority}</strong>
      <span>blocked paths</span>
    </div>
  </div>

  <section class="session-card">
    <div>
      <p class="eyebrow">Current local actor</p>
      <h2>{access.session.user} as {access.session.role}</h2>
      <p>
        Tenant {access.session.tenant}. This is a deterministic local session, expires
        {access.session.expiresAt}, and reads from <code>{access.session.route}</code>.
      </p>
    </div>
    <ul class="role-list" aria-label="Available roles">
      {#each access.session.roles as role}
        <li class:current={role === access.session.role}>{role}</li>
      {/each}
    </ul>
  </section>

  <section class="gate-card">
    <div>
      <p class="eyebrow">Actor admission</p>
      <span class="mdx-badge status-badge" data-tone={statusTone(access.actorAdmission.status)}>{statusLabel(access.actorAdmission.status)}</span>
      <p>
        {access.actorAdmission.governedRouteCount} governed write routes require policy, receipt intent,
        source route, and actor identity before any local mutation.
      </p>
    </div>
    <div class="gate-grid">
      <div>
        <strong>{access.actorAdmission.requiredFields.length}</strong>
        <span>required fields</span>
      </div>
      <div>
        <strong>{access.actorAdmission.requiredControls.length}</strong>
        <span>required controls</span>
      </div>
      <div>
        <strong>{access.actorAdmission.hiddenPrivilegeBypassAllowed ? "yes" : "no"}</strong>
        <span>hidden bypass</span>
      </div>
      <div>
        <strong>{access.actorAdmission.productionWriteAllowed ? "yes" : "no"}</strong>
        <span>production write</span>
      </div>
    </div>
  </section>

  <section class="areas" aria-label="Access areas">
    {#each access.areas as area}
      <article class="area">
        <div class="area-main">
          <div>
            <h2>{area.name}</h2>
            <p>{area.line}</p>
          </div>
          <span>{area.proof}</span>
        </div>

        <dl class="route-pair">
          {#each area.routes as route}
            <div>
              <dt>{route.label}</dt>
              <dd><code>{route.method} {route.path}</code></dd>
            </div>
          {/each}
        </dl>

        <ul class="receipt-list" aria-label={`${area.name} receipt kinds`}>
          {#each area.receipts as receipt}
            <li><code>{receipt}</code></li>
          {/each}
        </ul>
      </article>
    {/each}
  </section>

  <section class="blocked">
    <div>
      <h2>Blocked Authority</h2>
      <p>These stay closed until the Gate can prove live identity and policy authority.</p>
    </div>
    <ul>
      {#each access.blockedAuthority as blocked}
        <li>
          <strong>{blocked.label}</strong>
          <span>{blocked.state}</span>
        </li>
      {/each}
    </ul>
  </section>

  <footer class="quiet-line">
    <details class="quiet-more">
      <summary>Access sources</summary>
      {#each access.sources as source}
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
  .access {
    display: grid;
    gap: 22px;
    max-width: 1040px;
    margin: 0 auto;
    padding: 4vh 0 44px;
  }

  h2,
  p,
  dl,
  dd,
  ul {
    margin: 0;
  }

  h2,
  .summary strong,
  .gate-grid strong {
    font-family: var(--mdx-font-display);
  }

  .status-badge {
    justify-self: start;
    align-self: start;
  }

  h2 {
    font-size: 17px;
    line-height: 1.2;
  }

  .session-card p,
  .gate-card p,
  .area p,
  .area-main span,
  .blocked p,
  .blocked span,
  .quiet-line {
    color: var(--mdx-text-muted);
    font-size: 13px;
    line-height: 1.45;
  }

  .eyebrow,
  .summary span,
  .gate-grid span,
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
  .gate-grid div {
    display: grid;
    gap: 3px;
    padding: 14px;
    background: var(--mdx-surface-raised);
  }

  .summary strong,
  .gate-grid strong {
    font-size: 24px;
  }

  .session-card,
  .gate-card,
  .area,
  .blocked {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .session-card,
  .gate-card {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(260px, 0.55fr);
    gap: 24px;
    padding: 18px;
  }

  .role-list,
  .receipt-list,
  .blocked ul {
    list-style: none;
    padding: 0;
  }

  .role-list,
  .receipt-list,
  .quiet-line {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
  }

  .role-list {
    align-content: start;
    justify-content: end;
  }

  .role-list li,
  .receipt-list li {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    color: var(--mdx-text-secondary);
    font-size: 12px;
    padding: 5px 8px;
  }

  .role-list li.current {
    border-color: var(--mdx-accent-primary);
    color: var(--mdx-text-primary);
  }

  .gate-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 1px;
    overflow: hidden;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-border-subtle);
  }

  .areas {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  .area {
    display: grid;
    gap: 14px;
    padding: 16px;
  }

  .area-main {
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

  .blocked {
    display: grid;
    gap: 14px;
    padding: 16px;
  }

  .blocked ul {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }

  .blocked li {
    display: grid;
    gap: 4px;
    min-height: 76px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    padding: 12px;
  }

  .blocked strong {
    font-size: 13px;
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
    .areas,
    .blocked ul,
    .session-card,
    .gate-card {
      grid-template-columns: 1fr;
    }

    .role-list {
      justify-content: start;
    }
  }
</style>
