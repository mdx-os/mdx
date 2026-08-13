<script>
  // Talent: advisors and workers. The advisors at the top are consulted,
  // never managed - five thinkers with one verb. The workers below are
  // the working roster, master-detail, with presence and outcomes folded
  // from the receipts themselves. Visible labels stay in user words: no
  // internal metaphors (floor, room, salon) ever reach the page.
  import { onMount } from "svelte";
  import { loadProfile } from "../../lib/youProfile.js";
  import { companionColor } from "../../lib/twinCompanions.js";

  let { data } = $props();

  let yourName = $state("");
  onMount(() => {
    yourName = loadProfile().name ?? "";
  });

  // ---- The floor: contract roster merged with chain presence ----
  const floorByAgent = $derived.by(() => {
    const map = new Map();
    for (const worker of data.floor?.workers ?? []) {
      map.set(worker.agent_id, worker);
    }
    return map;
  });

  function momentLine(kind) {
    const words = String(kind ?? "")
      .split(".")
      .slice(-2)
      .join(" ")
      .replace(/_/g, " ");
    return words || "work recorded";
  }

  const floor = $derived(
    data.roster.runners.map((runner) => {
      const presence = floorByAgent.get(runner.id) ?? null;
      return {
        ...runner,
        working: Boolean(presence),
        receiptCount: presence?.receipt_count ?? 0,
        latestMoment: presence ? momentLine(presence.latest_kind) : "",
        latestReceiptId: presence?.latest_receipt_id ?? ""
      };
    })
  );

  let selectedId = $state("");
  const selected = $derived(
    floor.find((worker) => worker.id === selectedId) ?? floor[0] ?? null
  );

  function initial(name) {
    return String(name ?? "?").slice(0, 1).toUpperCase();
  }
</script>

<svelte:head><title>Talent - MDx</title></svelte:head>

<section class="talent" data-route-state="ready">
  <header class="talent-head mdx-page-head">
    <h1>Talent</h1>
    <p class="mdx-page-sub">Everyone who works here — people and agents alike.</p>
  </header>


  <div class="t-section">
    <h2>People</h2>
    <div class="person">
      <span class="avatar human" aria-hidden="true">{initial(yourName || "Y")}</span>
      <div class="person-body">
        <strong>{yourName || "You"}</strong>
        <span class="person-role">Owner — every agent here ultimately answers to a person.</span>
      </div>
    </div>
    <p class="section-cue">
      One human seat today. Invites open when the team rails do — and joining will take a
      recorded approval, like everything else here.
    </p>
  </div>

  <!-- The salon: consulted, never managed. One verb, zero HR fields. -->
  <div class="t-section">
    <h2>Your advisors</h2>
    <p class="section-cue">Five thinkers, grounded in the company's own record. Ask them anything.</p>
    <ul class="council">
      {#each data.roster.companions as companion (companion.id)}
        <li class="seat">
          <span class="avatar agent" style="background: {companionColor(companion.id)}" aria-hidden="true">{initial(companion.name)}</span>
          <strong class="seat-name">{companion.name}</strong>
          <span class="seat-role">{companion.roleTitle}</span>
          <p class="seat-line">{companion.line}</p>
          <a class="seat-ask" href="/?ask={companion.id}">Ask →</a>
        </li>
      {/each}
    </ul>
  </div>

  <!-- The floor: the working roster, master-detail, presence from the
       chain itself - "working" means receipts exist, never a stored flag. -->
  <div class="t-section">
    <h2>Workers</h2>
    <p class="section-cue">
      The agents doing the daily jobs. Every line here comes from work that actually happened.
    </p>
    <div class="floor">
      <ul class="roster" aria-label="The working roster">
        {#each floor as worker (worker.id)}
          <li>
            <button type="button" class="row" class:open={selected?.id === worker.id} onclick={() => (selectedId = worker.id)}>
              <span class="avatar agent small" aria-hidden="true">{initial(worker.name)}</span>
              <span class="row-body">
                <strong class="row-name">{worker.name}</strong>
                <span class="row-line">{worker.line}</span>
              </span>
              <span class="row-presence" data-working={worker.working}>
                <span class="presence-dot" aria-hidden="true"></span>
                {worker.working ? `${worker.receiptCount} on record` : "quiet so far"}
              </span>
            </button>
          </li>
        {/each}
      </ul>
      {#if selected}
        <aside class="panel" aria-label="Worker profile">
          <div class="panel-head">
            <span class="avatar agent" aria-hidden="true">{initial(selected.name)}</span>
            <div>
              <strong class="panel-name">{selected.name}</strong>
              <span class="panel-role">Runs one of the daily jobs</span>
            </div>
          </div>
          <p class="panel-line">{selected.line}</p>
          {#if selected.working}
            <p class="panel-presence">
              Working — {selected.receiptCount} {selected.receiptCount === 1 ? "moment" : "moments"} on
              the record. Latest: {selected.latestMoment}.
            </p>
          {:else}
            <p class="panel-presence quiet">Quiet so far — its first run will show here, with its proof.</p>
          {/if}
          <dl class="panel-facts">
            <div><dt>From</dt><dd>{selected.schoolName} · {selected.background}</dd></div>
            <div><dt>Discipline</dt><dd>Every run passes policy first and leaves its receipt{selected.policyChecked ? "" : " (policy gate pending)"}.</dd></div>
          </dl>
          <details class="quiet-more">
            <summary>view the record</summary>
            <dl class="facts">
              {#if selected.latestReceiptId}<div><dt>Latest</dt><dd><code>{selected.latestReceiptId}</code></dd></div>{/if}
              <div><dt>Route</dt><dd><code>{selected.raw.route}</code></dd></div>
              <div><dt>Contract</dt><dd><code>{selected.raw.contract_path}</code></dd></div>
            </dl>
          </details>
        </aside>
      {/if}
    </div>
  </div>

  <footer class="quiet-line">
    Nothing on this page acts by itself — every line of presence comes from recorded work.
    <details class="quiet-more">
      <summary>more</summary>
      <span>Boundary: {data.boundary}. Safe next move: {data.safeNext}.</span>
    </details>
  </footer>
</section>

<style>
  .talent {
    display: grid;
    gap: 26px;
    align-content: start;
    max-width: 980px;
    margin: 0 auto;
    padding: 2vh 0 40px;
  }

  .talent-head { display: grid; gap: 4px; }


  .t-section { display: grid; gap: 10px; }

  h2 {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 17px;
    font-weight: 650;
  }

  .section-cue {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 13.5px;
    line-height: 1.55;
    max-width: 640px;
  }

  /* ---- Avatars: humans are circles, agents are rounded squares ---- */
  .avatar {
    display: inline-grid;
    place-items: center;
    width: 40px;
    height: 40px;
    font-family: var(--mdx-font-display);
    font-weight: 700;
    font-size: 15px;
    color: #fff;
    background: var(--mdx-accent-primary);
    flex-shrink: 0;
  }

  .avatar.human { border-radius: 50%; }
  .avatar.agent { border-radius: 11px; background: var(--mdx-text-tertiary); }
  .avatar.agent.small { width: 32px; height: 32px; font-size: 12.5px; border-radius: 9px; }

  .person { display: flex; gap: 12px; align-items: center; }
  .person-body { display: grid; gap: 1px; }
  .person-role { color: var(--mdx-text-tertiary); font-size: 12px; }

  /* ---- The salon ---- */
  .council {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(168px, 1fr));
    gap: 12px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .seat {
    display: grid;
    gap: 5px;
    justify-items: start;
    padding: 16px 16px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .seat .avatar { width: 44px; height: 44px; border-radius: 12px; margin-bottom: 3px; }

  .seat-name { font-family: var(--mdx-font-display); font-size: 14.5px; }
  .seat-role { color: var(--mdx-text-muted); font-size: var(--mdx-text-xs); }
  .seat-line { margin: 2px 0 0; color: var(--mdx-text-secondary); font-size: 12px; line-height: 1.5; }

  .seat-ask {
    margin-top: 4px;
    color: var(--mdx-accent-primary);
    font-weight: 650;
    font-size: 12.5px;
    text-decoration: none;
  }

  .seat-ask:hover { text-decoration: underline; }

  /* ---- The floor: master-detail ---- */
  .floor {
    display: grid;
    grid-template-columns: minmax(0, 1.2fr) minmax(260px, 1fr);
    gap: 14px;
    align-items: start;
  }

  .roster { display: grid; gap: 6px; margin: 0; padding: 0; list-style: none; }

  .row {
    display: flex;
    gap: 10px;
    align-items: center;
    width: 100%;
    padding: 9px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    text-align: left;
    font: inherit;
    color: inherit;
    cursor: pointer;
  }

  .row:hover { border-color: var(--mdx-accent-primary); }
  .row.open {
    border-color: var(--mdx-accent-primary);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--mdx-accent-primary) 20%, transparent);
  }

  .row-body { display: grid; gap: 0; min-width: 0; flex: 1; }
  .row-name { font-size: 13px; font-weight: 600; }
  .row-line {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-presence {
    display: inline-flex;
    gap: 5px;
    align-items: center;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
    white-space: nowrap;
  }

  .presence-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--mdx-border-strong);
  }

  .row-presence[data-working="true"] { color: var(--mdx-accent-success); }
  .row-presence[data-working="true"] .presence-dot { background: var(--mdx-accent-success); }

  .panel {
    display: grid;
    gap: 10px;
    padding: 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-sunken);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    position: sticky;
    top: 12px;
  }

  .panel-head { display: flex; gap: 12px; align-items: center; }
  .panel-head > div { display: grid; gap: 1px; }
  .panel-name { font-family: var(--mdx-font-display); font-size: 15px; }
  .panel-role { color: var(--mdx-text-muted); font-size: var(--mdx-text-xs); }
  .panel-line { margin: 0; color: var(--mdx-text-secondary); font-size: 12.5px; line-height: 1.5; }

  .panel-presence { margin: 0; font-size: 12.5px; color: var(--mdx-accent-success); line-height: 1.5; }
  .panel-presence.quiet { color: var(--mdx-text-tertiary); }

  .panel-facts { display: grid; gap: 6px; margin: 0; }
  .panel-facts div { display: grid; grid-template-columns: 80px minmax(0, 1fr); gap: 8px; }
  .panel-facts dt { color: var(--mdx-text-tertiary); font-size: var(--mdx-text-xs); }
  .panel-facts dd { margin: 0; font-size: 12px; color: var(--mdx-text-secondary); line-height: 1.45; }

  .facts { display: grid; gap: 6px; margin: 8px 0 0; }
  .facts div { display: grid; grid-template-columns: 70px minmax(0, 1fr); gap: 8px; }
  .facts dt { color: var(--mdx-text-tertiary); font-size: var(--mdx-text-xs); }
  .facts dd { margin: 0; font-size: var(--mdx-text-xs); color: var(--mdx-text-secondary); overflow-wrap: anywhere; }

  code { font-family: var(--mdx-font-mono); font-size: var(--mdx-text-xs); color: var(--mdx-text-secondary); }

  .quiet-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .quiet-more { display: inline; }

  .quiet-more > summary {
    display: inline;
    color: var(--mdx-text-muted);
    cursor: pointer;
    list-style: none;
    font-size: var(--mdx-text-xs);
  }

  .quiet-more > summary::-webkit-details-marker { display: none; }
  .quiet-line .quiet-more[open] > summary { display: none; }

  @media (max-width: 860px) {
    .floor { grid-template-columns: 1fr; }
  }

  @media (prefers-reduced-motion: reduce) {
    * { transition: none !important; }
  }
</style>
