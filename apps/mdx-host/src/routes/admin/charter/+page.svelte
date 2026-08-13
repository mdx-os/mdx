<script>
  // Charter: the constitution. The rules this company runs by, and the
  // record that proves they still hold. The rules themselves are always
  // here - they are generated contract truth. The record is read at
  // runtime and stays honest: it shows what has actually been attested on
  // this machine, or invites the first attestation when nothing has.
  let { data } = $props();

  const recordLine = $derived.by(() => {
    if (data.attestationCount == null) {
      return null;
    }
    if (data.attestationCount === 0) {
      return "Nothing has been attested on this machine yet. The first attestation will appear here the moment the charter loop runs.";
    }
    const n = data.attestationCount;
    return `${n} ${n === 1 ? "attestation has" : "attestations have"} been recorded on this machine - each one links back to the rule it proves.`;
  });
</script>

<section class="charter" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Charter</h1>
    <p class="mdx-page-sub">The rules this company runs by — attested on the record.</p>
  </header>

  <p class="pulse">
    A charter is only worth what proves it. Every rule below names the check that keeps it honest,
    and the company attests against them on its own record — never on its word.
  </p>

  <div class="x-section">
    <h2>What this company holds itself to</h2>
    <p class="section-cue">
      Each promise is something a person can hold the company to. Behind each one is a recorded
      check that keeps it true.
    </p>
    <ul class="principles">
      {#each data.principles as principle}
        <li class="principle">
          <strong>{principle.promise}</strong>
          <p class="principle-proof">{principle.proof}</p>
          <span class="principle-tag">Kept honest by the {principle.provingLabel}.</span>
          <details class="quiet-more">
            <summary>view the record</summary>
            <dl class="facts">
              <div><dt>Commitment</dt><dd>{principle.raw.commitment}</dd></div>
              <div><dt>Check</dt><dd><code>{principle.raw.check}</code></dd></div>
              <div><dt>Evidence</dt><dd><code>{principle.raw.evidence}</code></dd></div>
            </dl>
          </details>
        </li>
      {/each}
    </ul>
  </div>

  <div class="x-section">
    <h2>Lines it will not cross</h2>
    <p class="section-cue">
      Some things are never negotiable. These hold whether anyone is watching or not.
    </p>
    <ul class="lines">
      {#each data.lines as line}
        <li class="line">
          <span class="line-mark" aria-hidden="true"></span>
          <div class="line-body">
            <span>{line.line}</span>
            <details class="quiet-more">
              <summary>view the wording</summary>
              <span class="line-raw"><code>{line.raw}</code></span>
            </details>
          </div>
        </li>
      {/each}
    </ul>
  </div>

  <div class="x-section">
    <h2>The record</h2>
    <p class="section-cue">
      Attesting is itself a loop that proves itself. It guarantees the following each time it runs —
      and what it has actually proven on this machine shows below.
    </p>
    <ul class="guarantees">
      {#each data.attestation.guarantees as guarantee}
        <li class="guarantee">
          <span class="guarantee-mark" aria-hidden="true"></span>
          <span>{guarantee.line}</span>
        </li>
      {/each}
    </ul>
    {#if recordLine}
      <p class="record-line" class:quiet={data.attestationCount === 0}>{recordLine}</p>
    {:else}
      <p class="record-line quiet">
        The attestation record reads from this install. Start the local server and it fills in — from
        the record itself, never from hope.
      </p>
    {/if}
    <details class="quiet-more record-more">
      <summary>view the record</summary>
      <dl class="facts">
        <div><dt>Loop</dt><dd><code>{data.attestation.loop}</code></dd></div>
        <div>
          <dt>Attested</dt>
          <dd>{data.attestationCount == null ? "record unread" : data.attestationCount}</dd>
        </div>
        <div>
          <dt>In the chain</dt>
          <dd>{data.recordCount == null ? "record unread" : data.recordCount}</dd>
        </div>
      </dl>
    </details>
  </div>

  <footer class="quiet-line">
    These rules are stated here and proven elsewhere — this page never edits one or signs an attestation.
    <details class="quiet-more">
      <summary>more</summary>
      <span>Boundary: {data.boundary}. Safe next move: {data.safeNext}.</span>
    </details>
  </footer>
</section>

<style>
  .charter {
    display: grid;
    gap: 28px;
    align-content: start;
    max-width: 760px;
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

  .x-section {
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
    font-size: 13.5px;
    line-height: 1.55;
  }

  .principles {
    display: grid;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .principle {
    display: grid;
    gap: 5px;
    padding: 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .principle strong {
    font-family: var(--mdx-font-display);
    font-size: 14.5px;
    font-weight: 650;
    line-height: 1.4;
  }

  .principle-proof {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.5;
  }

  .principle-tag {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .lines,
  .guarantees {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .line {
    display: flex;
    gap: 12px;
  }

  .line-mark {
    flex: none;
    width: 7px;
    height: 7px;
    margin-top: 7px;
    border-radius: 50%;
    background: var(--mdx-accent-primary);
  }

  .line-body {
    display: grid;
    gap: 3px;
    min-width: 0;
  }

  .line-body > span {
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.5;
  }

  .line-raw {
    display: block;
    margin-top: 4px;
    color: var(--mdx-text-muted);
    overflow-wrap: anywhere;
  }

  .guarantee {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }

  .guarantee-mark {
    flex: none;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--mdx-accent-success);
  }

  .guarantee > span {
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.5;
  }

  .record-line {
    margin: 4px 0 0;
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.5;
  }

  .record-line.quiet {
    color: var(--mdx-text-muted);
  }

  .record-more {
    margin-top: 2px;
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
    grid-template-columns: 100px minmax(0, 1fr);
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

  .principle .quiet-more > summary,
  .line .quiet-more > summary,
  .record-more > summary {
    color: var(--mdx-accent-primary);
    opacity: 0.8;
  }

  @media (prefers-reduced-motion: reduce) {
    * {
      transition: none !important;
    }
  }
</style>
