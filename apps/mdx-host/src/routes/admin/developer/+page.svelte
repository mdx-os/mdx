<script>
  // Developer: how MDx is built, and how to build on it. Every number and
  // name on this page comes from a generated contract - the repo
  // describing itself. If it disagrees with a doc, the contract wins.
  let { data } = $props();

  const map = $derived(data.map);
  const adm = $derived(
    map.session
      ? JSON.stringify({
          tenant_id: map.session.tenant_id,
          actor_id: map.session.user_id,
          actor_role: map.session.role,
          policy_decision_id: "policy-local-actor-admission",
          receipt_intent: "message_thread_message",
          source_route: "/developer",
          request_id: "actor-your-first-write"
        })
      : ""
  );
</script>

<section class="developer" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Developer</h1>
    <p class="mdx-page-sub">How MDx is built — and how to build on it.</p>
  </header>

  <p class="lead">
    One Rust kernel, {map.crates.length} crates, one rule that explains everything else:
    <strong>nothing happens without a receipt.</strong> Every write is admitted against policy,
    recorded with proof, and refused fail-closed when its rails aren't open. The UI you're in
    renders what the kernel can prove — never what anyone hopes.
  </p>

  <div class="dev-section">
    <h2>The shape</h2>
    <p class="section-cue">
      A boring monolith on purpose: {map.crates.length} crates, {map.loops.length} closed loops,
      and generated contracts as the only source of truth.
    </p>
    <div class="crate-row">
      {#each map.crates as crate}
        <code class="crate">{crate}</code>
      {/each}
    </div>
    <ul class="loops">
      {#each map.loops as loop}
        <li>
          <code>{loop.id}</code>
          <span>{loop.line}{loop.policyRequired ? " — every run policy-checked" : ""}</span>
        </li>
      {/each}
    </ul>
    <details class="quiet-more">
      <summary>the architecture rules, verbatim</summary>
      <ul class="rules">
        {#each map.rules as rule}
          <li>{rule}</li>
        {/each}
      </ul>
    </details>
  </div>

  <div class="dev-section">
    <h2>The API</h2>
    <p class="section-cue">
      {map.routeCount} declared routes, {map.governedWriteCount} of them governed writes — every
      one in the generated catalog, every write receipt-backed. This list <em>is</em> the
      contract; anything not here answers 404.
    </p>
    <div class="api-groups">
      {#each map.apiGroups as group}
        <details class="api-group">
          <summary>
            <span class="group-name">{group.label}</span>
            <span class="group-count">{group.count} {group.count === 1 ? "route" : "routes"}{group.writes > 0 ? ` · ${group.writes} write${group.writes === 1 ? "" : "s"}` : ""}</span>
          </summary>
          <ul class="route-list">
            {#each group.routes as route}
              <li>
                <span class="method {route.method.toLowerCase()}">{route.method}</span>
                <code>{route.path}</code>
                {#if route.receiptBacked}<span class="receipted" title="Saves a receipt">✓</span>{/if}
              </li>
            {/each}
          </ul>
        </details>
      {/each}
    </div>
  </div>

  <div class="dev-section">
    <h2>Your first write</h2>
    <p class="section-cue">
      Writes need an admission header — who you are, why you're writing, which receipt you
      intend. The local kernel ships a deterministic session, so this works the minute
      <code>make local-smoke</code> passes:
    </p>
    <pre class="example"><code>curl -X POST http://127.0.0.1:18787/messages/thread-messages.json \
  -H 'Content-Type: application/json' \
  -H 'X-MDX-Actor-Admission: {adm}' \
  -d '&#123;"body":"my first receipt-backed write"&#125;'</code></pre>
    <p class="section-cue">
      The response names the receipts it saved. Open Message and your line is in the channel;
      open the answer's proof and the receipt is there. That round trip is the whole system
      in miniature.
    </p>
  </div>

  <div class="dev-section">
    <h2>Contribute</h2>
    <p class="section-cue">
      The work queue is generated and scored — {map.workItems.length} items, each with an owner
      type, a write scope, and a definition of done. Pick one; don't widen it.
    </p>
    <ul class="work-items">
      {#each map.workItems.slice(0, 8) as item}
        <li>
          <span class="work-priority">P{item.priority}</span>
          <div class="work-body">
            <code>{item.id}</code>
            <span>{item.summary}</span>
          </div>
        </li>
      {/each}
    </ul>
    {#if map.workItems.length > 8}
      <details class="quiet-more">
        <summary>all {map.workItems.length} items</summary>
        <ul class="work-items">
          {#each map.workItems.slice(8) as item}
            <li>
              <span class="work-priority">P{item.priority}</span>
              <div class="work-body">
                <code>{item.id}</code>
                <span>{item.summary}</span>
              </div>
            </li>
          {/each}
        </ul>
      </details>
    {/if}
    <p class="section-cue">
      Full queue with scopes and checks: <code>{map.workQueueSource}</code> · agent ground rules:
      <code>AGENTS.md</code> · install rails live on <a href="/admin/setup">Setup</a>.
    </p>
  </div>

  <div class="dev-section">
    <h2>Change lanes</h2>
    <p class="section-cue">
      Pick the lane that matches the work before you edit. The commands marked declared are in
      the generated verification manifest; local commands are repo gates that still belong in the
      PR note when they apply.
    </p>
    <div class="lane-grid">
      {#each map.checkGroups as group}
        <article class="lane-card">
          <h3>{group.title}</h3>
          <p>{group.line}</p>
          <ul>
            {#each group.commands as check}
              <li>
                <code>{check.command}</code>
                <span>{check.state}</span>
              </li>
            {/each}
          </ul>
        </article>
      {/each}
    </div>
  </div>

  <div class="dev-section">
    <h2>Handoff docs</h2>
    <p class="section-cue">
      DEV-101 is the quick start. DEV-201 is the richer maintainer guide for changing the repo,
      running a scoped agent, and understanding what is still blocked before beta.
    </p>
    <ul class="doc-list">
      {#each map.developerDocs as doc}
        <li>
          <code>{doc.path}</code>
          <div>
            <strong>{doc.title}</strong>
            <span>{doc.line}</span>
          </div>
        </li>
      {/each}
    </ul>
  </div>

  <footer class="quiet-line">
    Everything on this page is the repo describing itself.
    <details class="quiet-more">
      <summary>more</summary>
      <span>Boundary: {data.boundary}. Safe next move: {data.safeNext}.</span>
    </details>
  </footer>
</section>

<style>
  .developer {
    display: grid;
    gap: 30px;
    align-content: start;
    max-width: 760px;
    margin: 0 auto;
    padding: 4vh 0 40px;
  }

  .lead {
    max-width: 640px;
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 16px;
    line-height: 1.55;
  }

  .lead strong {
    color: var(--mdx-text-primary);
  }

  .dev-section {
    display: grid;
    gap: 12px;
  }

  h2 {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 17px;
    font-weight: 650;
  }

  .section-cue {
    max-width: 620px;
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 13.5px;
    line-height: 1.55;
  }

  code {
    font-family: var(--mdx-font-mono);
    font-size: 12px;
    color: var(--mdx-text-secondary);
  }

  .crate-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .crate {
    padding: 4px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-raised);
    font-size: var(--mdx-text-xs);
  }

  .loops {
    display: grid;
    gap: 6px;
    margin: 4px 0 0;
    padding: 0;
    list-style: none;
  }

  .loops li {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 10px;
    font-size: 12.5px;
  }

  .loops span {
    color: var(--mdx-text-muted);
  }

  .rules {
    display: grid;
    gap: 5px;
    margin: 10px 0 0;
    padding-left: 18px;
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    line-height: 1.5;
  }

  .api-groups {
    display: grid;
    gap: 4px;
  }

  .api-group {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }

  .api-group summary {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
    padding: 10px 14px;
    cursor: pointer;
    list-style: none;
  }

  .api-group summary::-webkit-details-marker {
    display: none;
  }

  .group-name {
    font-size: 13px;
    font-weight: 600;
    text-transform: capitalize;
  }

  .group-count {
    color: var(--mdx-text-muted);
    font-size: 12px;
  }

  .route-list {
    display: grid;
    gap: 4px;
    margin: 0;
    padding: 4px 14px 12px;
    list-style: none;
  }

  .route-list li {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .route-list code {
    overflow-wrap: anywhere;
    font-size: var(--mdx-text-xs);
  }

  .method {
    flex: none;
    width: 38px;
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    font-weight: 700;
  }

  .method.get {
    color: var(--mdx-text-muted);
  }

  .method.post {
    color: var(--mdx-accent-primary);
  }

  .receipted {
    color: var(--mdx-accent-success);
    font-size: var(--mdx-text-xs);
  }

  .example {
    margin: 0;
    padding: 14px 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-sunken);
    overflow-x: auto;
  }

  .example code {
    font-size: var(--mdx-text-xs);
    line-height: 1.6;
    white-space: pre;
  }

  .work-items {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .work-items li {
    display: flex;
    gap: 12px;
    align-items: baseline;
  }

  .work-priority {
    flex: none;
    color: var(--mdx-text-muted);
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
  }

  .work-body {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .work-body span {
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    line-height: 1.45;
  }

  .lane-grid {
    display: grid;
    gap: 10px;
  }

  .lane-card {
    display: grid;
    gap: 8px;
    padding: 14px 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .lane-card h3 {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 14px;
    font-weight: 650;
  }

  .lane-card p {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    line-height: 1.45;
  }

  .lane-card ul,
  .doc-list {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .lane-card li,
  .doc-list li {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }

  .lane-card li span {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .doc-list li {
    padding: 10px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }

  .doc-list li > code {
    flex: 0 1 210px;
    overflow-wrap: anywhere;
  }

  .doc-list div {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .doc-list strong {
    font-size: 13px;
  }

  .doc-list span {
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    line-height: 1.45;
  }

  a {
    color: var(--mdx-accent-primary);
    text-decoration: none;
  }

  .quiet-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .quiet-more {
    display: inline;
  }

  .quiet-more > summary {
    display: inline;
    color: var(--mdx-text-muted);
    cursor: pointer;
    list-style: none;
    font-size: var(--mdx-text-xs);
  }

  .quiet-more > summary::-webkit-details-marker {
    display: none;
  }

  .quiet-line .quiet-more[open] > summary {
    display: none;
  }

  .dev-section .quiet-more > summary {
    color: var(--mdx-accent-primary);
    opacity: 0.8;
  }
</style>
