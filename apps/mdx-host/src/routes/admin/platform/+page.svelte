<script>
  // Platform: what this install runs on, and what turns on next. Statuses
  // drive everything - when a rail's live proof is observed, this page
  // upgrades itself. Key NAMES and presence only; values never exist here.
  let { data } = $props();
</script>

<section class="platform" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Platform</h1>
    <p class="mdx-page-sub">What this install runs on — and what turns on next.</p>
  </header>

  {#if data.pulse}
    <p class="pulse">{data.pulse}</p>
  {:else}
    <p class="pulse">
      This page reads what your install runs on. Start the local server and it fills in — from
      the install's own answer, never from hope.
    </p>
  {/if}

  {#if data.substrates.length > 0}
    <div class="p-section">
      <h2>The rails</h2>
      <ul class="rails">
        {#each data.substrates as substrate}
          <li class="rail">
            <span class="rail-dot" class:live={substrate.live} aria-hidden="true"></span>
            <div class="rail-body">
              <div class="rail-title">
                <strong>{substrate.name}</strong>
                <span class="rail-status" class:live={substrate.live}>{substrate.statusLabel}</span>
              </div>
              <p class="rail-line">{substrate.line}</p>
              {#if !substrate.live && substrate.goesLiveWhen}
                <p class="rail-when">Goes live when {substrate.goesLiveWhen}.</p>
              {/if}
              <details class="quiet-more">
                <summary>view the state</summary>
                <dl class="facts">
                  <div><dt>State</dt><dd><code>{substrate.raw.status}</code></dd></div>
                  {#if substrate.raw.blocking}
                    <div><dt>Before live</dt><dd>{substrate.raw.blocking}</dd></div>
                  {/if}
                  {#if substrate.raw.firstProof}
                    <div><dt>First proof</dt><dd><code>{substrate.raw.firstProof}</code></dd></div>
                  {/if}
                </dl>
              </details>
            </div>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if data.candidates.length > 0}
    <div class="p-section">
      <h2>Turn-ons waiting</h2>
      <p class="section-cue">
        Each candidate below lists what it needs by name — presence only, never values. Keys live
        in your shell or your cloud's secret manager. Turning on happens at the terminal, with a
        recorded approval, and counts only when the live proof is observed.
      </p>
      <ul class="candidates">
        {#each data.candidates as candidate}
          <li class="candidate">
            <div class="candidate-head">
              <strong>{candidate.name}</strong>
              <span class="candidate-status">{candidate.statusLabel}</span>
            </div>
            <ul class="needs">
              {#each candidate.requirements as requirement}
                <li class="need">
                  <span class="need-dot" class:present={requirement.present} aria-hidden="true"></span>
                  <code>{requirement.name}</code>
                </li>
              {/each}
            </ul>
            <details class="quiet-more">
              <summary>what counts as on</summary>
              <dl class="facts">
                <div><dt>Counts when</dt><dd>{candidate.raw.countsWhen}</dd></div>
                {#each candidate.requirements as requirement}
                  <div><dt><code>{requirement.name}</code></dt><dd>{requirement.purpose}</dd></div>
                {/each}
                <div><dt>Receipt</dt><dd><code>{candidate.raw.receiptKind}</code></dd></div>
                <div><dt>Adapter</dt><dd><code>{candidate.raw.adapter}</code></dd></div>
                <div><dt>State</dt><dd><code>{candidate.raw.status}</code></dd></div>
              </dl>
            </details>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  <footer class="quiet-line">
    Nothing turns on from this page — and when something turns on for real, this page shows it.
    <details class="quiet-more">
      <summary>more</summary>
      <span>Boundary: {data.boundary}. Safe next move: {data.safeNext}.</span>
    </details>
  </footer>
</section>

<style>
  .platform {
    display: grid;
    gap: 28px;
    align-content: start;
    max-width: 720px;
    margin: 0 auto;
    padding: 4vh 0 40px;
  }

  .pulse {
    max-width: 620px;
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 16px;
    line-height: 1.55;
  }

  .p-section {
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
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    line-height: 1.55;
  }

  .rails,
  .candidates {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .rail {
    display: flex;
    gap: 14px;
    padding: 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .rail-dot {
    flex: none;
    width: 9px;
    height: 9px;
    margin-top: 6px;
    border-radius: 50%;
    border: 2px solid var(--mdx-border-strong);
  }

  .rail-dot.live {
    border: none;
    background: var(--mdx-accent-success);
  }

  .rail-body {
    display: grid;
    gap: 4px;
    min-width: 0;
  }

  .rail-title {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 10px;
  }

  .rail-title strong {
    font-family: var(--mdx-font-display);
    font-size: 14.5px;
    font-weight: 650;
  }

  .rail-status {
    color: var(--mdx-text-muted);
    font-size: 12px;
    font-weight: 600;
  }

  .rail-status.live {
    color: var(--mdx-tone-ok-text);
  }

  .rail-line {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
  }

  .rail-when {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 12px;
    line-height: 1.45;
  }

  .candidate {
    display: grid;
    gap: 8px;
    padding: 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .candidate-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }

  .candidate-head strong {
    font-family: var(--mdx-font-display);
    font-size: 14px;
    font-weight: 650;
  }

  .candidate-status {
    color: var(--mdx-accent-primary);
    font-size: 12px;
    font-weight: 600;
  }

  .needs {
    display: grid;
    gap: 5px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .need {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  .need-dot {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    border: 1.5px solid var(--mdx-border-strong);
  }

  .need-dot.present {
    border: none;
    background: var(--mdx-accent-success);
  }

  code {
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-secondary);
  }

  .facts {
    display: grid;
    gap: 6px;
    margin: 10px 0 0;
  }

  .facts div {
    display: grid;
    grid-template-columns: 90px minmax(0, 1fr);
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

  .rail .quiet-more > summary,
  .candidate .quiet-more > summary {
    color: var(--mdx-accent-primary);
    opacity: 0.8;
  }
</style>
