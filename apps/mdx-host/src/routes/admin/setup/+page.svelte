<script>
  // Setup: local is useful today; production opens only when each authority has
  // proof. The page is target-first - pick where it runs, then see exactly what
  // THAT target needs. Everything heavy is behind disclosure so the page reads
  // as a checklist, not a wall. It never adds readiness the kernel did not
  // report, and env VALUES never appear (presence only).
  import Provenance from "@mdx/ui/components/Provenance.svelte";

  let { data } = $props();

  function toneFor(state) {
    if (state === "local working") return "ok";
    if (state === "production-shaped proof") return "proof";
    if (state === "blocked") return "blocked";
    return "unknown";
  }

  function readableId(id) {
    return String(id ?? "")
      .replace(/_/g, " ")
      .replace(/\b\w/g, (char) => char.toUpperCase());
  }

  function readyPct(p) {
    if (!p || p.requiredEnvCount <= 0) return 100;
    return Math.round(((p.requiredEnvCount - p.missingEnvCount) / p.requiredEnvCount) * 100);
  }

  const proofItems = $derived(
    data.proof
      ? [
          { label: "Packet", value: data.proof.route },
          { label: "Status", value: data.proof.status },
          { label: "Evidence", value: data.proof.evidenceFile },
          { label: "Refresh with", value: data.proof.refreshCommand }
        ]
      : []
  );
</script>

<section class="setup" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Prepare for Production</h1>
    <p class="mdx-page-sub">
      Your install runs locally today. Production opens one authority at a time, each with proof.
      Start by picking where it will run.
    </p>
  </header>

  {#if data.pulse}
    <p class="pulse">
      {data.pulse}
      {#if data.proof}
        <Provenance items={proofItems} label="Read from this install's own evidence" />
      {/if}
    </p>
  {:else}
    <p class="pulse">
      This page reads what your install says about itself. Start the local server and it fills in
      honestly, from saved evidence, never from hope.
    </p>
  {/if}

  <!-- Step 1: the spine. Everything below is a consequence of this choice. -->
  {#if data.profileOptions.length > 0}
    <section class="targets" aria-labelledby="pick-target">
      <div class="section-head">
        <div>
          <p class="kicker">Step 1</p>
          <h2 id="pick-target">Where will it run?</h2>
        </div>
        <p>Pick a target to see exactly what it needs. A scaffold is never deploy authority.</p>
      </div>
      <nav class="target-grid" aria-label="Production targets">
        {#each data.profileOptions as option}
          <a
            class="target"
            class:active={option.selected}
            href={option.href}
            aria-current={option.selected ? "true" : undefined}
          >
            <strong>{option.label}</strong>
            <span class="state {toneFor(option.state)}">{option.state}</span>
          </a>
        {/each}
      </nav>
    </section>
  {/if}

  <!-- Step 2: one focused panel for the chosen target. Heavy detail discloses. -->
  {#if data.selectedProfilePreflight}
    {@const p = data.selectedProfilePreflight}
    <section class="panel" aria-labelledby="target-detail">
      <div class="panel-head">
        <div>
          <p class="kicker">Your target</p>
          <h2 id="target-detail">{p.label}</h2>
        </div>
        <span class="state {toneFor(p.state)}">{p.state}</span>
      </div>

      <div class="readiness-bar">
        <div class="readiness-count">
          <strong>{p.requiredEnvCount - p.missingEnvCount}</strong>
          <span>of {p.requiredEnvCount} required values in place</span>
        </div>
        <div class="bar" role="presentation"><span style="width:{readyPct(p)}%"></span></div>
      </div>

      <dl class="posture">
        <div><dt>Services</dt><dd>{p.servicePlane.join(", ") || "-"}</dd></div>
        <div><dt>Secrets</dt><dd>{p.secretStrategy || "-"}</dd></div>
        <div><dt>Auth</dt><dd>{p.authStrategy || "-"}</dd></div>
        <div><dt>Data</dt><dd>{p.dataPlane || "-"}</dd></div>
        <div><dt>Network</dt><dd>{p.networkStrategy || "-"}</dd></div>
        <div><dt>Scale</dt><dd>{p.scaleStrategy || "-"}</dd></div>
      </dl>

      <div class="do-next">
        <h3>Do this next</h3>
        <ol>
          {#each p.nextActions as action}
            <li>{action}</li>
          {/each}
        </ol>
      </div>

      <div class="discs">
        <details class="disc">
          <summary>
            <span>Required environment names</span>
            <span class="tag {p.missingEnvCount > 0 ? 'warn' : 'ok'}">
              {p.missingEnvCount} missing of {p.requiredEnvCount}
            </span>
          </summary>
          <div class="disc-body">
            {#if p.worksheet}
              <p class="worksheet">Worksheet: <code>{p.worksheet}</code></p>
            {/if}
            {#if p.envRows.length > 0}
              <ul class="env-list">
                {#each p.envRows as env}
                  <li class:missing={!env.present}>
                    <span>{env.name}</span>
                    <small>{env.present ? "present" : "missing"} &middot; {env.kind} &middot; {env.category}</small>
                  </li>
                {/each}
              </ul>
            {:else}
              <p class="quiet-copy">This profile declares no env names in the current packet.</p>
            {/if}
          </div>
        </details>

        <details class="disc">
          <summary>
            <span>Deployment package</span>
            <span class="tag">{p.packageAssumptions.title}</span>
          </summary>
          <div class="disc-body">
            <div class="provide">
              <div>
                <h5>MDx provides</h5>
                <ul class="plain-list">
                  {#each p.packageAssumptions.standard as item}<li>{item}</li>{/each}
                </ul>
              </div>
              <div>
                <h5>You provide</h5>
                <ul class="plain-list">
                  {#each p.packageAssumptions.customerSpecific as item}<li>{item}</li>{/each}
                </ul>
              </div>
            </div>
            {#if p.packageArtifacts.length > 0}
              <ul class="artifact-list">
                {#each p.packageArtifacts as artifact}
                  <li>
                    <div class="artifact-head">
                      <strong>{readableId(artifact.id)}</strong>
                      <span class="tag">{artifact.status}</span>
                    </div>
                    {#each artifact.paths as path}<code>{path}</code>{/each}
                    <small>Proof: {artifact.proof} &middot; deploy allowed: {artifact.deploymentAllowedNow ? "yes" : "no"}</small>
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        </details>

        <details class="disc">
          <summary>
            <span>Proof commands</span>
            <span class="tag">{p.proofCommands.length}</span>
          </summary>
          <div class="disc-body">
            <ul class="plain-list">
              {#each p.proofCommands as command}<li><code>{command}</code></li>{/each}
            </ul>
          </div>
        </details>
      </div>

      {#if p.blockers.length > 0}
        <div class="closed-for-target">
          <h3>Still closed for this target</h3>
          <ul>
            {#each p.blockers as blocker}<li>{blocker}</li>{/each}
          </ul>
        </div>
      {/if}
    </section>
  {/if}

  <!-- Step 3: the full checklist, scannable - one line per area, detail on tap. -->
  <section class="readiness" aria-labelledby="production-checklist">
    <div class="section-head">
      <div>
        <p class="kicker">Step 3</p>
        <h2 id="production-checklist">The full readiness checklist</h2>
      </div>
      <p>Every area that must be true before real users or cloud traffic. Tap a row for detail.</p>
    </div>
    <div class="check-list">
      {#each data.readinessRows as row}
        <details class="check">
          <summary>
            <span class="check-area">{row.area}</span>
            <span class="state {toneFor(row.state)}">{row.state}</span>
          </summary>
          <dl>
            <div><dt>Now</dt><dd>{row.current}</dd></div>
            <div><dt>Why</dt><dd>{row.why}</dd></div>
            <div><dt>Next</dt><dd><code>{row.next}</code></dd></div>
            <div><dt>GUI today</dt><dd>{row.gui}</dd></div>
          </dl>
        </details>
      {/each}
    </div>
  </section>

  <!-- The boundary, kept calm: one strip, always honest, never shouting. -->
  <section class="boundary" aria-labelledby="blocked-authority">
    <div class="section-head compact">
      <div>
        <p class="kicker">The line</p>
        <h2 id="blocked-authority">What setup will not do</h2>
      </div>
    </div>
    <ul class="closed-strip">
      {#each data.blockedAuthorities as item}
        <li>
          <span class="state {item.allowed ? 'ok' : 'blocked'}">{item.allowed ? "open" : "blocked"}</span>
          <div>
            <strong>{item.label}</strong>
            <p>{item.reason}</p>
          </div>
        </li>
      {/each}
    </ul>
  </section>

  <!-- Secondary context, demoted to disclosures. -->
  {#if data.tracks.length > 0}
    <details class="section-disc">
      <summary>All paths from local to production ({data.tracks.length})</summary>
      <ul class="tracks">
        {#each data.tracks as track}
          <li class="track">
            <span class="track-dot {track.status.kind}" aria-hidden="true"></span>
            <div class="track-body">
              <div class="track-title">
                <strong>{track.name}</strong>
                {#if track.vendor}<span class="track-vendor">{track.vendor}</span>{/if}
                <span class="track-status {track.status.kind}">{track.status.label}</span>
              </div>
              <p class="track-line">{track.line}</p>
            </div>
          </li>
        {/each}
      </ul>
    </details>
  {/if}

  {#if data.publicGates.length > 0}
    <details class="section-disc">
      <summary>Open-source release gates ({data.publicGates.length})</summary>
      <div class="check-list">
        {#each data.publicGates as gate}
          <div class="gate-row">
            <span class="gate-name">{readableId(gate.id)}</span>
            <span class="state {toneFor(gate.status === 'COMPLETE' ? 'local working' : 'blocked')}">{gate.status}</span>
          </div>
        {/each}
      </div>
    </details>
  {/if}

  <footer class="quiet-line">
    Everything starts on this machine, proven before it moves. Keys and secrets live in your shell
    or your cloud's secret manager, never inside MDx.
    <details class="quiet-more">
      <summary>more</summary>
      <span>
        Boundary: {data.boundary}. Safe next move: {data.safeNext}. One command proves the local
        story end to end: <code>{data.oneCommandLocal}</code>. Packets: {data.packetRoutes.join(", ")}.
      </span>
    </details>
  </footer>
</section>

<style>
  .setup {
    display: grid;
    gap: 26px;
    align-content: start;
    max-width: 1040px;
    margin: 0 auto;
    padding-top: 6vh;
  }
  .pulse {
    display: inline;
    max-width: 620px;
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 16px;
    line-height: 1.55;
  }
  .pulse :global(.provenance) {
    margin-left: 6px;
    vertical-align: baseline;
  }

  .section-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: 1.25rem;
    flex-wrap: wrap;
    margin-bottom: 0.9rem;
  }
  .section-head.compact {
    margin-bottom: 0.5rem;
  }
  .section-head p {
    margin: 0;
    max-width: 30rem;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
    line-height: 1.45;
  }
  .kicker {
    margin: 0 0 0.2rem;
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--mdx-text-tertiary);
  }
  h2 {
    margin: 0;
    font-family: var(--mdx-font-display);
    color: var(--mdx-text-primary);
    font-size: var(--mdx-text-xl);
  }

  /* State chips, shared. */
  .state {
    font-size: var(--mdx-text-xs);
    font-weight: 700;
    padding: 0.15rem 0.6rem;
    border-radius: var(--mdx-radius-pill);
    white-space: nowrap;
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-secondary);
  }
  .state.ok {
    background: var(--mdx-tone-ok-surface, var(--mdx-surface-sunken));
    color: var(--mdx-tone-ok-text);
  }
  .state.proof {
    background: var(--mdx-surface-sunken);
    color: var(--mdx-accent-primary);
  }
  .state.blocked {
    background: var(--mdx-tone-danger-surface, var(--mdx-surface-sunken));
    color: var(--mdx-tone-danger-text);
  }

  /* Step 1: target grid - the spine. */
  .target-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.6rem;
  }
  .target {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.6rem;
    padding: 0.9rem 1rem;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    text-decoration: none;
    color: var(--mdx-text-primary);
    font-weight: 600;
  }
  .target:hover {
    border-color: var(--mdx-border-default);
  }
  .target.active {
    border-color: var(--mdx-accent-primary);
    box-shadow: inset 0 0 0 1px var(--mdx-accent-primary);
  }

  /* Step 2: the focused target panel. */
  .panel {
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    padding: 1.4rem;
    display: grid;
    gap: 1.1rem;
  }
  .panel-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }
  .readiness-bar {
    display: grid;
    gap: 0.5rem;
  }
  .readiness-count {
    display: flex;
    align-items: baseline;
    gap: 0.4rem;
  }
  .readiness-count strong {
    font-size: var(--mdx-text-xl);
    color: var(--mdx-text-primary);
    font-family: var(--mdx-font-display);
  }
  .readiness-count span {
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
  }
  .bar {
    height: 0.4rem;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-sunken);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--mdx-accent-primary);
  }
  .posture {
    margin: 0;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.75rem;
  }
  .posture > div {
    display: grid;
    gap: 0.15rem;
  }
  .posture dt {
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--mdx-text-tertiary);
  }
  .posture dd {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    line-height: 1.4;
  }
  .do-next h3 {
    margin: 0 0 0.5rem;
    font-size: var(--mdx-text-base);
    color: var(--mdx-text-primary);
  }
  .do-next ol {
    margin: 0;
    padding-left: 1.2rem;
    display: grid;
    gap: 0.4rem;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    line-height: 1.45;
  }

  /* Progressive disclosure inside the panel. */
  .discs {
    display: grid;
    gap: 0.5rem;
  }
  .disc {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }
  .disc > summary {
    cursor: pointer;
    list-style: none;
    padding: 0.75rem 0.9rem;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }
  .disc > summary::-webkit-details-marker {
    display: none;
  }
  .disc[open] > summary {
    border-bottom: 1px solid var(--mdx-border-subtle);
  }
  .disc-body {
    padding: 0.9rem;
    display: grid;
    gap: 0.75rem;
  }
  .tag {
    font-size: var(--mdx-text-xs);
    font-weight: 600;
    padding: 0.1rem 0.5rem;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-tertiary);
  }
  .tag.warn {
    color: var(--mdx-tone-danger-text);
  }
  .tag.ok {
    color: var(--mdx-tone-ok-text);
  }
  .worksheet {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
  }
  .env-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.4rem;
  }
  .env-list li {
    display: grid;
    gap: 0.1rem;
    padding: 0.4rem 0.6rem;
    border-radius: var(--mdx-radius-sm);
    background: var(--mdx-surface-sunken);
  }
  .env-list li.missing {
    box-shadow: inset 0 0 0 1px var(--mdx-tone-danger-border, var(--mdx-border-default));
  }
  .env-list span {
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-primary);
  }
  .env-list small {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }
  .provide {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 1rem;
  }
  .provide h5 {
    margin: 0 0 0.4rem;
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-primary);
  }
  .plain-list {
    margin: 0;
    padding-left: 1.1rem;
    display: grid;
    gap: 0.3rem;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    line-height: 1.4;
  }
  .artifact-list {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    gap: 0.5rem;
  }
  .artifact-list li {
    display: grid;
    gap: 0.25rem;
    padding: 0.6rem 0.75rem;
    border-radius: var(--mdx-radius-sm);
    background: var(--mdx-surface-sunken);
  }
  .artifact-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .artifact-list code {
    font-size: var(--mdx-text-xs);
  }
  .artifact-list small {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }
  .closed-for-target {
    border-top: 1px solid var(--mdx-border-subtle);
    padding-top: 0.9rem;
  }
  .closed-for-target h3 {
    margin: 0 0 0.4rem;
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-secondary);
  }
  .closed-for-target ul {
    margin: 0;
    padding-left: 1.1rem;
    display: grid;
    gap: 0.25rem;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
  }

  /* Step 3: the scannable checklist. */
  .check-list {
    display: grid;
    gap: 0.4rem;
  }
  .check {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }
  .check > summary {
    cursor: pointer;
    list-style: none;
    padding: 0.7rem 0.95rem;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
  }
  .check > summary::-webkit-details-marker {
    display: none;
  }
  .check-area {
    font-weight: 600;
    color: var(--mdx-text-primary);
  }
  .check[open] > summary {
    border-bottom: 1px solid var(--mdx-border-subtle);
  }
  .check dl {
    margin: 0;
    padding: 0.85rem 0.95rem;
    display: grid;
    gap: 0.6rem;
  }
  .check dl > div {
    display: grid;
    grid-template-columns: 5rem 1fr;
    gap: 0.75rem;
    align-items: start;
  }
  .check dt {
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--mdx-text-tertiary);
  }
  .check dd {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    line-height: 1.45;
  }
  code {
    font-family: var(--mdx-font-mono);
    font-size: 0.85em;
    background: var(--mdx-surface-sunken);
    padding: 0.05rem 0.3rem;
    border-radius: var(--mdx-radius-sm);
  }

  /* The boundary strip. */
  .closed-strip {
    margin: 0;
    padding: 0;
    list-style: none;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.6rem;
  }
  .closed-strip li {
    display: flex;
    gap: 0.6rem;
    align-items: flex-start;
    padding: 0.75rem 0.9rem;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }
  .closed-strip strong {
    display: block;
    color: var(--mdx-text-primary);
    font-size: var(--mdx-text-sm);
  }
  .closed-strip p {
    margin: 0.15rem 0 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    line-height: 1.4;
  }

  /* Demoted secondary sections. */
  .section-disc {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }
  .section-disc > summary {
    cursor: pointer;
    padding: 0.8rem 1rem;
    font-weight: 600;
    color: var(--mdx-text-secondary);
  }
  .section-disc[open] > summary {
    border-bottom: 1px solid var(--mdx-border-subtle);
  }
  .tracks {
    margin: 0;
    padding: 0.9rem 1rem;
    list-style: none;
    display: grid;
    gap: 0.75rem;
  }
  .track {
    display: flex;
    gap: 0.6rem;
    align-items: flex-start;
  }
  .track-dot {
    margin-top: 0.4rem;
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--mdx-text-tertiary);
    flex: none;
  }
  .track-dot.local,
  .track-status.local {
    color: var(--mdx-tone-ok-text);
  }
  .track-dot.local {
    background: var(--mdx-tone-ok-text);
  }
  .track-title {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .track-title strong {
    color: var(--mdx-text-primary);
  }
  .track-vendor,
  .track-status {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-tertiary);
  }
  .track-line {
    margin: 0.2rem 0 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    line-height: 1.45;
  }
  .gate-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.6rem 1rem;
  }
  .gate-name {
    color: var(--mdx-text-primary);
    font-size: var(--mdx-text-sm);
    font-weight: 600;
  }

  .quiet-copy {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
  }
  .quiet-line {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
    line-height: 1.5;
  }
  .quiet-more {
    display: inline;
  }
  .quiet-more summary {
    display: inline;
    cursor: pointer;
    color: var(--mdx-accent-primary);
  }

  @media (max-width: 760px) {
    .target-grid,
    .posture,
    .env-list,
    .provide,
    .closed-strip {
      grid-template-columns: 1fr;
    }
    .section-head {
      align-items: flex-start;
    }
  }
</style>
