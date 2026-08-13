<script>
  // Aegis (admin): security for the operator, in three plain questions -
  // what's watching, what it's looking for, and what's locked shut by
  // design. The declared posture always renders; the locked doors and scan
  // evidence come from the running install and degrade to a warm invitation
  // when it's not up. The locked doors are the strength here, not a warning.
  let { data } = $props();

  const pulse = $derived.by(() => {
    const watching = data.watchers.length;
    const looking = data.threats.length;
    if (!watching && !looking) {
      return null;
    }
    return `${watching} checks stand watch, each one looking for one specific way things could go wrong. Nothing acts from here - this surface only reads, and a missing answer counts as unknown, never as safe.`;
  });
</script>

<section class="aegis" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Security</h1>
    <p class="mdx-page-sub">What's watching, what it found, and what's locked shut by design.</p>
  </header>

  {#if pulse}
    <p class="pulse">{pulse}</p>
  {:else}
    <p class="pulse">
      This is where the install's own security posture shows up - the checks standing watch and the
      ways things could go wrong that they guard against. Start the local server and it fills in.
    </p>
  {/if}

  {#if data.watchers.length > 0}
    <div class="a-section">
      <h2>What's watching</h2>
      <p class="section-cue">
        Each line is a standing check. None of them is a promise - every one names the exact command
        that proves it still holds.
      </p>
      <ul class="cards">
        {#each data.watchers as watcher}
          <li class="card">
            <strong>{watcher.title}</strong>
            <p class="card-line">{watcher.line}</p>
            <details class="quiet-more">
              <summary>view the record</summary>
              <dl class="facts">
                <div><dt>Check</dt><dd><code>{watcher.raw.check}</code></dd></div>
                <div><dt>Proven by</dt><dd><code>{watcher.raw.evidence}</code></dd></div>
                <div><dt>Token</dt><dd><code>{watcher.raw.id}</code></dd></div>
              </dl>
            </details>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if data.threats.length > 0}
    <div class="a-section">
      <h2>What it's looking for</h2>
      <p class="section-cue">
        The ways things could go wrong, named openly and ordered by how much they'd cost. Naming a
        risk is how it gets a guard - each one points at the check that catches it.
      </p>
      <ul class="threats">
        {#each data.threats as threat}
          <li class="threat">
            <span class="threat-dot" class:serious={threat.serious} aria-hidden="true"></span>
            <div class="threat-body">
              <div class="threat-title">
                <strong>{threat.title}</strong>
                <span class="threat-tag" class:serious={threat.serious}>{threat.impactLabel}</span>
              </div>
              <p class="threat-line">{threat.line}</p>
              <details class="quiet-more">
                <summary>view the record</summary>
                <dl class="facts">
                  <div><dt>Risk</dt><dd>{threat.raw.description}</dd></div>
                  <div><dt>Caught by</dt><dd><code>{threat.raw.mitigation_check}</code></dd></div>
                  <div><dt>Evidence</dt><dd><code>{threat.raw.evidence_artifact}</code></dd></div>
                  <div><dt>Token</dt><dd><code>{threat.raw.id}</code> · <code>{threat.raw.impact}</code></dd></div>
                </dl>
              </details>
            </div>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  <div class="a-section">
    <h2>Locked by design</h2>
    {#if data.doors && data.doors.length > 0}
      <p class="section-cue">
        These doors are shut on purpose. Not because something went wrong - because this surface was
        built so they can't open from here. Each one is the install's own answer, read live.
      </p>
      <ul class="doors">
        {#each data.doors as door}
          <li class="door">
            <span class="lock" aria-hidden="true">&#10003;</span>
            <span class="door-line">{door.line}</span>
            <details class="quiet-more">
              <summary>view the flag</summary>
              <span class="door-flag"><code>{door.raw}</code> is held at <code>false</code></span>
            </details>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="section-cue">
        The list of doors this install keeps shut by design will show here once the local server is
        running. Until then, take the strongest one as given: nothing on this surface can act.
      </p>
    {/if}
    {#if data.hardStops.length > 0}
      <details class="quiet-more hard-stops-disclosure">
        <summary>the lines that are never crossed quietly</summary>
        <ul class="hard-stops">
          {#each data.hardStops as stop}
            <li><span>{stop.line}</span> <code>{stop.raw}</code></li>
          {/each}
        </ul>
      </details>
    {/if}
  </div>

  <div class="a-section">
    <h2>What it found</h2>
    {#if data.scans === null}
      <p class="section-cue">
        Scan receipts land here once the local server is up. The sweep runs as the security loop, on
        its own schedule - which is why there's nothing to press.
      </p>
    {:else if data.scans.length === 0}
      <p class="section-cue">
        No scan receipts on this machine yet, and that's an honest empty - not a clean bill. The
        sweep runs as the security loop, under the same checks above, and what it finds will land here.
      </p>
    {:else}
      <p class="section-cue">The most recent sweeps, newest first. Each one is a receipt you can trace.</p>
      <ul class="receipts">
        {#each data.scans as scan}
          <li><code>{scan}</code></li>
        {/each}
      </ul>
    {/if}
  </div>

  <footer class="quiet-line">
    Missing evidence reads as unknown here, never as safe - and nothing on this page can act.
    <details class="quiet-more">
      <summary>more</summary>
      <span>Boundary: {data.boundary}. Safe next move: {data.safeNext}.</span>
    </details>
  </footer>
</section>

<style>
  .aegis {
    display: grid;
    gap: 28px;
    align-content: start;
    max-width: 740px;
    margin: 0 auto;
    padding: 4vh 0 40px;
  }

  .pulse {
    max-width: 640px;
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 16px;
    line-height: 1.55;
  }

  .a-section {
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
    max-width: 640px;
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.55;
  }

  .cards {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .card strong {
    font-family: var(--mdx-font-display);
    font-size: 14px;
    font-weight: 650;
  }

  .card-line {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.5;
  }

  .threats,
  .doors {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .threat {
    display: flex;
    gap: 14px;
    padding: 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .threat-dot {
    flex: none;
    width: 9px;
    height: 9px;
    margin-top: 6px;
    border-radius: 50%;
    border: 2px solid var(--mdx-border-strong);
  }

  .threat-dot.serious {
    border: none;
    background: var(--mdx-accent-primary);
  }

  .threat-body {
    display: grid;
    gap: 4px;
    min-width: 0;
  }

  .threat-title {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 10px;
  }

  .threat-title strong {
    font-family: var(--mdx-font-display);
    font-size: 14px;
    font-weight: 650;
  }

  .threat-tag {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    font-weight: 600;
  }

  .threat-tag.serious {
    color: var(--mdx-accent-primary);
  }

  .threat-line {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.5;
  }

  .door {
    display: flex;
    align-items: baseline;
    gap: 12px;
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .lock {
    flex: none;
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--mdx-accent-success);
    color: #fff;
    font-size: var(--mdx-text-xs);
    line-height: 1;
    transform: translateY(2px);
  }

  .door-line {
    flex: 1;
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.5;
  }

  .door-flag {
    display: block;
    margin-top: 6px;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .hard-stops-disclosure {
    margin-top: 2px;
  }

  .hard-stops {
    display: grid;
    gap: 6px;
    margin: 10px 0 0;
    padding: 0;
    list-style: none;
  }

  .hard-stops li {
    display: grid;
    gap: 2px;
    color: var(--mdx-text-secondary);
    font-size: 12px;
    line-height: 1.45;
  }

  .receipts {
    display: grid;
    gap: 5px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .receipts li {
    padding: 8px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .facts {
    display: grid;
    gap: 6px;
    margin: 10px 0 0;
  }

  .facts div {
    display: grid;
    grid-template-columns: 80px minmax(0, 1fr);
    gap: 8px;
  }

  .facts dt {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .facts dd {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-xs);
    line-height: 1.45;
    overflow-wrap: anywhere;
  }

  code {
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-secondary);
    overflow-wrap: anywhere;
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

  .card .quiet-more > summary,
  .threat .quiet-more > summary,
  .door .quiet-more > summary,
  .hard-stops-disclosure > summary {
    color: var(--mdx-accent-primary);
    opacity: 0.8;
  }

  @media (max-width: 640px) {
    .cards {
      grid-template-columns: 1fr;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    * {
      transition: none !important;
    }
  }
</style>
