<script>
  // Strategy: the board. Six columns derived from records - cards move on
  // evidence (conditions and their verdicts), human decisions, and
  // resolutions; nothing is dragged and nothing stores a column. The
  // story view is the same record as a narrative. Every verb here is a
  // governed write that opens no authority by itself.
  import { createMdxClient } from "@mdx/client";
  import { invalidateAll } from "$app/navigation";
  import DecisionRoom from "@mdx/ui/components/DecisionRoom.svelte";
  import { BOARD_COLUMNS, boardFrom, timelineFrom } from "../../lib/strategyBoard.js";

  let { data } = $props();

  const board = $derived(boardFrom(data.board));
  const timeline = $derived(timelineFrom(data.board));

  // The legacy fixture question stands in as one decidable card only
  // while nobody has authored a card yet - a fresh install still opens
  // with a real call waiting.
  const fixtureCard = $derived.by(() => {
    if (board.cards.length > 0) return null;
    const fixture = data.fixture;
    if (!fixture?.question || fixture.decided) return null;
    return {
      fixture: true,
      proposalId: fixture.proposalId ?? "strategy_local_ratification_001",
      title: fixture.question,
      whyNow: fixture.whyNow ?? "",
      stage: "ready",
      conditions: [],
      scoreboard: { total: 0, makeOrBreak: 0, holds: 0, broken: 0, testing: 0 },
      horizon: "",
      ratified: false,
      resolution: null
    };
  });

  const columns = $derived(
    BOARD_COLUMNS.map((column) => ({
      ...column,
      cards: [
        ...(fixtureCard && column.key === "ready" ? [fixtureCard] : []),
        ...board.columns.find((entry) => entry.key === column.key).cards
      ]
    }))
  );

  const totalCards = $derived(columns.reduce((sum, column) => sum + column.cards.length, 0));
  const waitingOnYou = $derived(columns.find((c) => c.key === "ready")?.cards.length ?? 0);

  let view = $state("board"); // board | story
  let selectedId = $state("");
  const selected = $derived(
    selectedId === fixtureCard?.proposalId
      ? fixtureCard
      : board.cards.find((card) => card.proposalId === selectedId) ?? null
  );

  const client = $derived(createMdxClient({ baseUrl: "/api/kernel", session: data.session }));
  const actor = $derived(data.session?.user_id ?? "local_user");

  // ---- New card ----
  let newOpen = $state(false);
  let busy = $state(false);
  let flash = $state(null); // { ok, line } - one quiet result line, reused by every verb
  let draft = $state({
    direction: "", why_now: "", problem: "", where_we_play: "", how_we_win: "",
    horizon: "", sponsor: "", success_metric: "", kill_condition: "", funding_ask: ""
  });

  async function write(path, body, intent, okLine) {
    if (busy) return null;
    busy = true;
    flash = null;
    try {
      const packet = await client.write(path, { ...body, actor_id: actor }, { receiptIntent: intent });
      if (packet.status === "RECORDED") {
        flash = { ok: true, line: okLine };
        await invalidateAll();
        busy = false;
        return packet;
      }
      flash = { ok: false, line: packet.reason ?? "That did not record. Try again." };
    } catch (error) {
      flash = { ok: false, line: "Nothing was recorded - that did not land. Try again." };
    }
    busy = false;
    return null;
  }

  async function createCard() {
    if (!draft.direction.trim()) return;
    const body = Object.fromEntries(
      Object.entries(draft).map(([key, value]) => [key, value.trim()]).filter(([, value]) => value)
    );
    const packet = await write(
      "/strategy/direction-proposals.json", body,
      "strategy_direction_proposal",
      "On the board. Now: what would have to be true?"
    );
    if (packet) {
      draft = { direction: "", why_now: "", problem: "", where_we_play: "", how_we_win: "",
        horizon: "", sponsor: "", success_metric: "", kill_condition: "", funding_ask: "" };
      newOpen = false;
      selectedId = packet.proposal_id ?? "";
    }
  }

  // ---- Conditions ----
  let claimDraft = $state("");
  let claimMob = $state(true);
  async function addCondition() {
    if (!selected || !claimDraft.trim()) return;
    const packet = await write(
      "/strategy/conditions.json",
      { proposal_id: selected.proposalId, claim: claimDraft.trim(), make_or_break: claimMob ? "true" : "false" },
      "strategy_condition",
      "Claim stated. It starts as assumed - testing it is fair work for an agent."
    );
    if (packet) claimDraft = "";
  }

  let verdict = $state(null); // { conditionId, status, evidence_ref, note }
  function openVerdict(condition, status) {
    verdict = { conditionId: condition.conditionId, status, evidence_ref: "", note: "" };
  }
  async function startTesting(condition) {
    verdict = null;
    await write(
      "/strategy/condition-updates.json",
      { condition_id: condition.conditionId, status: "being_tested", evidence_ref: "", note: "" },
      "strategy_condition_update",
      "Under test."
    );
  }
  async function saveVerdict() {
    if (!verdict) return;
    const packet = await write(
      "/strategy/condition-updates.json",
      { condition_id: verdict.conditionId, status: verdict.status,
        evidence_ref: verdict.evidence_ref.trim(), note: verdict.note.trim() },
      "strategy_condition_update",
      "Verdict on the record, with its evidence."
    );
    if (packet) verdict = null;
  }

  // ---- The call ----
  const CALL_OPTIONS = [
    { token: "hold_current_direction", title: "Hold the current direction", line: "Keep going as we are. Nothing new is committed.", weight: "Steady" },
    { token: "request_more_evidence", title: "Ask for more evidence first", line: "Hold the call open and gather more proof before anyone commits.", weight: "Patient" },
    { token: "ratify_next_local_strategy_option", title: "Decide on this direction", line: "Put it on the record so the bet, build, and team paths can open behind it.", weight: "Committing" }
  ];
  let chosen = $state("");
  let reasonDraft = $state("");
  let callResult = $state(null);
  async function makeTheCall(option) {
    if (!selected) return;
    const packet = await write(
      "/strategy/ratification-decisions.json",
      { proposal_id: selected.proposalId, decision: option, reason: reasonDraft.trim() },
      "strategy_ratification_decision",
      "Your call is on the record. Nothing opened because of it - that is the design."
    );
    if (packet) {
      chosen = option;
      callResult = { ok: true, line: "Your call is on the record.", receiptId: packet.decision_receipt_id ?? "" };
      reasonDraft = "";
    } else if (flash && !flash.ok) {
      callResult = { ok: false, line: flash.line, receiptId: "" };
    }
  }

  // ---- Resolution ----
  let resolveOpen = $state(false);
  let learnedDraft = $state("");
  async function resolveCard(outcome) {
    if (!selected) return;
    const packet = await write(
      "/strategy/proposal-resolutions.json",
      { proposal_id: selected.proposalId, outcome, learned: learnedDraft.trim() },
      "strategy_proposal_resolution",
      outcome === "killed"
        ? "Killed, lesson kept. That is a success outcome here."
        : outcome === "archived"
          ? "Shelved with its reasons. It can come back."
          : "Completed, on the record."
    );
    if (packet) {
      learnedDraft = "";
      resolveOpen = false;
    }
  }

  function selectCard(card) {
    selectedId = card.proposalId;
    chosen = "";
    callResult = null;
    verdict = null;
    resolveOpen = false;
    flash = null;
  }

  // ---- Our direction: the standing record ----
  const BOX_LABELS = {
    winning: "What winning looks like",
    where_we_play: "Where we play",
    how_we_win: "How we win",
    great_at: "What we have to be great at",
    counting_on: "What we are counting on"
  };
  const directionVersions = $derived(
    Array.isArray(data.ourDirection?.versions) ? data.ourDirection.versions : []
  );
  const currentDirection = $derived(directionVersions[0] ?? null);
  let directionOpen = $state(false);
  let amendOpen = $state(false);
  let amend = $state(null);
  function startAmend() {
    amend = {
      winning: currentDirection?.winning ?? "",
      where_we_play: currentDirection?.where_we_play ?? "",
      how_we_win: currentDirection?.how_we_win ?? "",
      great_at: currentDirection?.great_at ?? "",
      counting_on: currentDirection?.counting_on ?? ""
    };
    amendOpen = true;
    directionOpen = true;
  }
  async function saveDirection() {
    if (!amend?.winning?.trim()) return;
    const body = {};
    for (const [key, value] of Object.entries(amend)) {
      if (String(value).trim()) body[key] = String(value).trim();
    }
    const packet = await write(
      "/strategy/our-direction.json", body,
      "strategy_our_direction",
      "The direction is written down - versioned, with what changed."
    );
    if (packet) amendOpen = false;
  }

  const HORIZON_LABEL = { now: "Now", next: "Next", someday: "Someday" };
  const STATUS_LABEL = {
    assumed: "assumed",
    being_tested: "testing",
    holds: "holds",
    broken: "broke"
  };
</script>

<svelte:head><title>Strategy - MDx</title></svelte:head>

<section class="strategy" data-route-state="ready">
  <header class="board-head">
    <div class="mdx-page-head">
      <h1>Strategy</h1>
      <p class="mdx-page-sub">Where the company is heading — and what is still being decided.</p>
    </div>
    <div class="head-actions">
      <div class="mdx-segmented" role="tablist" aria-label="View">
        <button type="button" role="tab" aria-selected={view === "board"} class:active={view === "board"} onclick={() => (view = "board")}>Board</button>
        <button type="button" role="tab" aria-selected={view === "story"} class:active={view === "story"} onclick={() => (view = "story")}>Story</button>
      </div>
      <button type="button" class="mdx-btn primary" onclick={() => { newOpen = true; selectedId = ""; }}>
        + New direction
      </button>
    </div>
  </header>

  {#if waitingOnYou > 0}
    <p class="pulse">
      {waitingOnYou} {waitingOnYou === 1 ? "call is" : "calls are"} ready for you — the evidence is in.
    </p>
  {:else if totalCards === 0}
    <p class="pulse">Nothing on the board yet. Put the first direction on the table — one sentence is enough to start.</p>
  {:else}
    <p class="pulse">The board is working. Nothing needs your call right now.</p>
  {/if}

  <div class="direction-card" data-open={directionOpen}>
    <button type="button" class="direction-strip" onclick={() => (directionOpen = !directionOpen)}>
      <span class="direction-title">Our direction{currentDirection ? ` · v${currentDirection.version}` : ""}</span>
      <span class="direction-peek">
        {currentDirection ? currentDirection.winning : "Not written down yet — say what winning looks like."}
      </span>
      <span class="direction-caret" aria-hidden="true">{directionOpen ? "▾" : "▸"}</span>
    </button>
    {#if directionOpen}
      {#if currentDirection && !amendOpen}
        <dl class="direction-boxes">
          {#each Object.entries(BOX_LABELS) as [key, label]}
            {#if currentDirection[key]}
              <div><dt>{label}</dt><dd>{currentDirection[key]}</dd></div>
            {/if}
          {/each}
        </dl>
      {/if}
      {#if amendOpen}
        <div class="direction-form">
          {#each Object.entries(BOX_LABELS) as [key, label]}
            <label class="f-label" for={"od-" + key}>{label}{key === "winning" ? "" : " (optional)"}</label>
            <textarea id={"od-" + key} rows="2" bind:value={amend[key]}></textarea>
          {/each}
          <div class="drawer-actions">
            <button type="button" class="mdx-btn primary" disabled={busy || !amend.winning.trim()} onclick={saveDirection}>
              {busy ? "Recording..." : currentDirection ? "Record the new version" : "Write it down"}
            </button>
            <button type="button" class="mdx-btn small" onclick={() => (amendOpen = false)}>Cancel</button>
          </div>
        </div>
      {:else}
        <div class="direction-acts">
          <button type="button" class="mdx-btn small" onclick={startAmend}>{currentDirection ? "Amend it" : "Write it down"}</button>
          {#if directionVersions.length > 1}
            <details class="quiet-more">
              <summary>what changed, version by version</summary>
              <ul class="direction-history">
                {#each directionVersions as version (version.record_receipt_id)}
                  <li>v{version.version} — changed: {version.changed.map((key) => BOX_LABELS[key] ?? key).join(", ").toLowerCase()}{version.amended_by ? ` · from ${version.amended_by}` : ""}</li>
                {/each}
              </ul>
            </details>
          {/if}
        </div>
      {/if}
      {#if flash && directionOpen && !selected && !newOpen}<p class="flash" class:bad={!flash.ok}>{flash.line}</p>{/if}
    {/if}
  </div>

  {#if view === "board"}
    <div class="board" role="list" aria-label="The strategy board">
      {#each columns as column}
        <div class="column" role="listitem" data-column={column.key}>
          <div class="col-head">
            <span class="col-label">{column.label}</span>
            {#if column.cards.length > 0}<span class="col-count">{column.cards.length}</span>{/if}
          </div>
          <p class="col-cue">{column.cue}</p>
          {#each column.cards as card (card.proposalId)}
            <button type="button" class="card" class:open={selectedId === card.proposalId} onclick={() => selectCard(card)}>
              <span class="card-title">{card.title}</span>
              <span class="card-meta">
                {#if card.horizon}<span class="chip horizon">{HORIZON_LABEL[card.horizon] ?? card.horizon}</span>{/if}
                {#if card.scoreboard.makeOrBreak > 0}
                  <span class="chip score" class:good={card.scoreboard.holds === card.scoreboard.makeOrBreak} class:bad={card.scoreboard.broken > 0}>
                    {card.scoreboard.holds}/{card.scoreboard.makeOrBreak} hold{card.scoreboard.broken > 0 ? ` · ${card.scoreboard.broken} broke` : ""}
                  </span>
                {:else if card.conditions.length === 0 && !card.fixture && card.stage !== "resolved" && card.stage !== "in_motion"}
                  <span class="chip todo">needs its claims</span>
                {/if}
                {#if card.resolution}<span class="chip done">{card.resolution.outcome}</span>{/if}
              </span>
            </button>
          {/each}
          {#if column.cards.length === 0}
            <div class="col-empty" aria-hidden="true"></div>
          {/if}
        </div>
      {/each}
    </div>
  {:else}
    <ol class="story" aria-label="The story so far">
      {#each timeline as event (event.receiptId)}
        <li class="story-line" data-kind={event.kind}>
          <span class="story-dot" aria-hidden="true"></span>
          <span class="story-text">{event.line}</span>
        </li>
      {:else}
        <li class="story-line"><span class="story-text">No story yet — it starts with the first direction on the table.</span></li>
      {/each}
    </ol>
  {/if}

  {#if flash && !selected && !newOpen}
    <p class="flash" class:bad={!flash.ok}>{flash.line}</p>
  {/if}

  <footer class="quiet-line">
    Cards move on records, never by hand — and deciding opens nothing by itself.
    <details class="quiet-more">
      <summary>more</summary>
      <span>Boundary: {data.boundary}. Safe next move: {data.safeNext}.</span>
    </details>
  </footer>
</section>

{#if newOpen}
  <button type="button" class="backdrop" aria-label="Close" onclick={() => (newOpen = false)}></button>
  <aside class="drawer" aria-label="New direction">
    <div class="drawer-head">
      <h2>Put a direction on the table</h2>
      <button type="button" class="drawer-close" onclick={() => (newOpen = false)} aria-label="Close">×</button>
    </div>
    <label class="f-label" for="nd-direction">The direction, in one sentence</label>
    <textarea id="nd-direction" rows="2" bind:value={draft.direction} placeholder="Where should the company head next?"></textarea>
    <label class="f-label" for="nd-problem">The problem it answers <span class="opt">optional</span></label>
    <textarea id="nd-problem" rows="2" bind:value={draft.problem} placeholder="What is actually wrong or unclaimed..."></textarea>
    <div class="f-row">
      <div>
        <label class="f-label" for="nd-where">Where we play <span class="opt">optional</span></label>
        <input id="nd-where" type="text" bind:value={draft.where_we_play} placeholder="The customers and field" />
      </div>
      <div>
        <label class="f-label" for="nd-how">How we win <span class="opt">optional</span></label>
        <input id="nd-how" type="text" bind:value={draft.how_we_win} placeholder="An edge a rival could choose the opposite of" />
      </div>
    </div>
    <label class="f-label" for="nd-why">Why now <span class="opt">optional</span></label>
    <input id="nd-why" type="text" bind:value={draft.why_now} placeholder="What you are seeing that makes this the moment" />
    <div class="f-row">
      <div>
        <label class="f-label" for="nd-horizon">Horizon <span class="opt">optional</span></label>
        <select id="nd-horizon" bind:value={draft.horizon}>
          <option value="">—</option>
          <option value="now">Now</option>
          <option value="next">Next</option>
          <option value="someday">Someday</option>
        </select>
      </div>
      <div>
        <label class="f-label" for="nd-metric">Success looks like <span class="opt">optional</span></label>
        <input id="nd-metric" type="text" bind:value={draft.success_metric} placeholder="The one number or moment" />
      </div>
    </div>
    <label class="f-label" for="nd-kill">We stop if <span class="opt">optional</span></label>
    <input id="nd-kill" type="text" bind:value={draft.kill_condition} placeholder="The kill condition, named up front" />
    <div class="drawer-actions">
      <button type="button" class="mdx-btn primary" disabled={busy || !draft.direction.trim()} onclick={createCard}>
        {busy ? "Recording..." : "Put it on the board"}
      </button>
      <button type="button" class="mdx-btn small" onclick={() => (newOpen = false)}>Never mind</button>
    </div>
    {#if flash}<p class="flash" class:bad={!flash.ok}>{flash.line}</p>{/if}
  </aside>
{/if}

{#if selected}
  <button type="button" class="backdrop" aria-label="Close" onclick={() => (selectedId = "")}></button>
  <aside class="drawer" aria-label="Card detail">
    <div class="drawer-head">
      <h2>{selected.title}</h2>
      <button type="button" class="drawer-close" onclick={() => (selectedId = "")} aria-label="Close">×</button>
    </div>
    {#if selected.whyNow}<p class="d-why">Why now: {selected.whyNow}</p>{/if}

    {#if !selected.fixture}
      {#if selected.problem || selected.whereWePlay || selected.howWeWin || selected.successMetric || selected.killCondition || selected.fundingAsk}
        <dl class="d-facts">
          {#if selected.problem}<div><dt>The problem</dt><dd>{selected.problem}</dd></div>{/if}
          {#if selected.whereWePlay}<div><dt>Where we play</dt><dd>{selected.whereWePlay}</dd></div>{/if}
          {#if selected.howWeWin}<div><dt>How we win</dt><dd>{selected.howWeWin}</dd></div>{/if}
          {#if selected.successMetric}<div><dt>Success looks like</dt><dd>{selected.successMetric}</dd></div>{/if}
          {#if selected.killCondition}<div><dt>We stop if</dt><dd>{selected.killCondition}</dd></div>{/if}
          {#if selected.fundingAsk}<div><dt>The ask</dt><dd>{selected.fundingAsk}</dd></div>{/if}
        </dl>
      {/if}

      <div class="d-section">
        <h3>What would have to be true</h3>
        {#if selected.conditions.length === 0}
          <p class="d-cue">No claims yet. State the make-or-break ones first — the card moves when they get verdicts.</p>
        {/if}
        <ul class="conds">
          {#each selected.conditions as condition (condition.conditionId)}
            <li class="cond" data-status={condition.status}>
              <div class="cond-row">
                <span class="cond-claim">
                  {condition.claim}
                  {#if condition.makeOrBreak}<span class="mob" title="Make or break">make-or-break</span>{/if}
                </span>
                <span class="cond-status">{STATUS_LABEL[condition.status] ?? condition.status}</span>
              </div>
              {#if (condition.status === "assumed" || condition.status === "being_tested") && !selected.resolution}
                <div class="cond-acts">
                  {#if condition.status === "assumed"}
                    <button type="button" class="mdx-btn small" disabled={busy} onclick={() => startTesting(condition)}>Start testing</button>
                  {/if}
                  <button type="button" class="mdx-btn small" onclick={() => openVerdict(condition, "holds")}>It holds</button>
                  <button type="button" class="mdx-btn small" onclick={() => openVerdict(condition, "broken")}>It broke</button>
                </div>
              {/if}
              {#if condition.evidenceRef}
                <details class="quiet-more"><summary>view the evidence</summary><code>{condition.evidenceRef}</code></details>
              {/if}
              {#if verdict?.conditionId === condition.conditionId}
                <div class="verdict">
                  <input type="text" bind:value={verdict.evidence_ref} placeholder="The evidence: a page, a signal, a receipt" aria-label="Evidence" />
                  <input type="text" bind:value={verdict.note} placeholder="One line on what you saw (optional)" aria-label="Note" />
                  <div class="cond-acts">
                    <button type="button" class="mdx-btn small primary" disabled={busy || !verdict.evidence_ref.trim()} onclick={saveVerdict}>
                      {verdict.status === "holds" ? "It holds — record it" : "It broke — record it"}
                    </button>
                    <button type="button" class="mdx-btn small" onclick={() => (verdict = null)}>Cancel</button>
                  </div>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
        {#if !selected.resolution}
          <div class="add-claim">
            <input type="text" bind:value={claimDraft} placeholder="A short testable claim..." aria-label="New claim" />
            <label class="mob-toggle"><input type="checkbox" bind:checked={claimMob} /> make-or-break</label>
            <button type="button" class="mdx-btn small primary" disabled={busy || !claimDraft.trim()} onclick={addCondition}>Add</button>
          </div>
        {/if}
      </div>
    {/if}

    {#if !selected.resolution && !selected.ratified}
      <div class="d-section">
        <DecisionRoom
          eyebrow="Your call"
          question={selected.fixture ? selected.title : "Where do you stand on this direction?"}
          whyNow={selected.scoreboard.makeOrBreak > 0
            ? `${selected.scoreboard.holds} of ${selected.scoreboard.makeOrBreak} make-or-break claims hold${selected.scoreboard.broken > 0 ? `, ${selected.scoreboard.broken} broke` : ""}`
            : ""}
          options={CALL_OPTIONS}
          {chosen}
          deciding={busy}
          bind:reason={reasonDraft}
          result={callResult}
          onDecide={makeTheCall}
        />
      </div>
    {/if}

    {#if selected.ratified && !selected.resolution}
      <p class="d-standing">Decided and in motion. When it lands or stalls, resolve it below with what was learned.</p>
    {/if}

    {#if !selected.fixture && !selected.resolution}
      <div class="d-section">
        {#if !resolveOpen}
          <button type="button" class="mdx-btn small" onclick={() => (resolveOpen = true)}>Resolve this card...</button>
        {:else}
          <h3>Resolve it</h3>
          <input type="text" bind:value={learnedDraft} placeholder="What we learned (kept forever)" aria-label="What we learned" />
          <div class="cond-acts">
            <button type="button" class="mdx-btn small" disabled={busy} onclick={() => resolveCard("completed")}>Completed</button>
            <button type="button" class="mdx-btn small" disabled={busy} onclick={() => resolveCard("killed")}>Kill it</button>
            <button type="button" class="mdx-btn small" disabled={busy} onclick={() => resolveCard("archived")}>Shelve it</button>
            <button type="button" class="mdx-btn small" onclick={() => (resolveOpen = false)}>Cancel</button>
          </div>
        {/if}
      </div>
    {/if}

    {#if selected.resolution}
      <p class="d-standing">
        {selected.resolution.outcome === "killed" ? "Killed" : selected.resolution.outcome === "archived" ? "Shelved" : "Completed"}{selected.resolution.learned ? ` — ${selected.resolution.learned}` : "."}
      </p>
    {/if}

    {#if flash}<p class="flash" class:bad={!flash.ok}>{flash.line}</p>{/if}

    {#if !selected.fixture}
      <details class="quiet-more block-more">
        <summary>view the record</summary>
        <dl class="d-facts small">
          <div><dt>Card</dt><dd><code>{selected.proposalId}</code></dd></div>
          <div><dt>Receipt</dt><dd><code>{selected.receiptId}</code></dd></div>
        </dl>
      </details>
    {/if}
  </aside>
{/if}

<style>
  .strategy {
    display: grid;
    gap: 20px;
    align-content: start;
    padding: 2vh 0 40px;
  }

  .board-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }

  .head-actions {
    display: flex;
    gap: 10px;
    align-items: center;
  }



  .pulse {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 15px;
  }

  .direction-card {
    display: grid;
    gap: 10px;
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .direction-strip {
    display: flex;
    align-items: baseline;
    gap: 12px;
    border: none;
    background: none;
    padding: 0;
    text-align: left;
    cursor: pointer;
    font: inherit;
    color: var(--mdx-text-primary);
    min-width: 0;
  }

  .direction-title {
    font-family: var(--mdx-font-display);
    font-size: 13px;
    font-weight: 700;
    white-space: nowrap;
  }

  .direction-peek {
    color: var(--mdx-text-tertiary);
    font-size: 12.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    flex: 1;
  }

  .direction-caret {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .direction-boxes {
    display: grid;
    gap: 8px;
    margin: 0;
    max-width: 720px;
  }

  .direction-boxes div {
    display: grid;
    grid-template-columns: 200px minmax(0, 1fr);
    gap: 10px;
  }

  .direction-boxes dt {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .direction-boxes dd {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: var(--mdx-text-secondary);
  }

  .direction-form {
    display: grid;
    gap: 6px;
    max-width: 720px;
  }

  .direction-form textarea {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-sm);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 12.5px;
  }

  .direction-acts {
    display: flex;
    gap: 14px;
    align-items: baseline;
  }

  .direction-history {
    display: grid;
    gap: 4px;
    margin: 8px 0 0;
    padding: 0 0 0 4px;
    list-style: none;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-tertiary);
  }

  /* Fixed-width columns that scroll horizontally on purpose (Trello/Linear),
     instead of stretch-to-fit columns that clipped the tail lane off-screen
     at 1440. A thin always-visible scrollbar signals there is more board. */
  .board {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 280px;
    gap: 12px;
    overflow-x: auto;
    padding-bottom: 10px;
    scrollbar-width: thin;
  }

  .column {
    display: grid;
    gap: 8px;
    align-content: start;
    min-height: 180px;
    padding: 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-sunken);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .col-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }

  .col-label {
    font-family: var(--mdx-font-display);
    font-size: 12.5px;
    font-weight: 700;
    letter-spacing: 0.02em;
    color: var(--mdx-text-secondary);
  }

  .col-count {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
  }

  .col-cue {
    margin: 0 0 2px;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    line-height: 1.4;
  }

  .card {
    display: grid;
    gap: 6px;
    padding: 10px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    text-align: left;
    cursor: pointer;
    font: inherit;
    color: var(--mdx-text-primary);
  }

  .card:hover { border-color: var(--mdx-accent-primary); }
  .card.open {
    border-color: var(--mdx-accent-primary);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--mdx-accent-primary) 25%, transparent);
  }

  .card-title {
    font-size: 13px;
    font-weight: 600;
    line-height: 1.35;
  }

  .card-meta { display: flex; gap: 6px; flex-wrap: wrap; }

  .chip {
    padding: 2px 7px;
    border-radius: 999px;
    font-size: var(--mdx-text-xs);
    font-weight: 600;
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-tertiary);
    border: 1px solid var(--mdx-border-subtle);
  }

  .chip.score.good {
    color: var(--mdx-accent-success);
    border-color: color-mix(in srgb, var(--mdx-accent-success) 40%, transparent);
  }

  .chip.score.bad {
    color: var(--mdx-accent-danger, #c0392b);
    border-color: color-mix(in srgb, var(--mdx-accent-danger, #c0392b) 40%, transparent);
  }

  .chip.done { text-transform: capitalize; }

  .col-empty {
    min-height: 40px;
    border: 1.5px dashed var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    opacity: 0.6;
  }

  .story {
    display: grid;
    gap: 10px;
    margin: 0;
    padding: 0 0 0 4px;
    list-style: none;
    max-width: 720px;
  }

  .story-line {
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr);
    gap: 10px;
    align-items: baseline;
  }

  .story-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--mdx-border-strong);
    margin-top: 4px;
  }

  .story-line[data-kind="decision"] .story-dot { background: var(--mdx-accent-primary); }
  .story-line[data-kind="condition_holds"] .story-dot { background: var(--mdx-accent-success); }
  .story-line[data-kind="condition_broken"] .story-dot { background: var(--mdx-accent-danger, #c0392b); }

  .story-text {
    font-size: 14px;
    line-height: 1.5;
    color: var(--mdx-text-secondary);
  }

  .backdrop {
    position: fixed;
    inset: 0;
    border: none;
    background: rgba(15, 18, 24, 0.18);
    cursor: pointer;
    z-index: 29;
  }

  /* The decision kit lays its options side by side for a full page;
     inside the drawer they stack so nothing leaves the panel. */
  .drawer :global(.options) {
    display: grid !important;
    grid-template-columns: 1fr !important;
    gap: 8px;
  }

  .drawer {
    position: fixed;
    top: 0;
    right: 0;
    bottom: 0;
    width: min(460px, 92vw);
    overflow-y: auto;
    display: grid;
    gap: 14px;
    align-content: start;
    padding: 22px 22px 40px;
    background: var(--mdx-surface-raised);
    border-left: 1px solid var(--mdx-border-subtle);
    box-shadow: -12px 0 32px rgba(0, 0, 0, 0.08);
    z-index: 30;
  }

  .drawer-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
  }

  .drawer-head h2 {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 18px;
    line-height: 1.3;
  }

  .drawer-close {
    border: none;
    background: none;
    font-size: 22px;
    line-height: 1;
    color: var(--mdx-text-muted);
    cursor: pointer;
  }

  .d-why {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: 12.5px;
  }

  .d-facts {
    display: grid;
    gap: 8px;
    margin: 0;
  }

  .d-facts div {
    display: grid;
    grid-template-columns: 120px minmax(0, 1fr);
    gap: 8px;
  }

  .d-facts.small div { grid-template-columns: 70px minmax(0, 1fr); }

  .d-facts dt {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-xs);
  }

  .d-facts dd {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--mdx-text-secondary);
    overflow-wrap: anywhere;
  }

  .d-section { display: grid; gap: 10px; }

  .d-section h3 {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 14px;
    font-weight: 650;
  }

  .d-cue {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: 12.5px;
  }

  .d-standing {
    margin: 0;
    font-size: 13px;
    color: var(--mdx-text-secondary);
  }

  .conds {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .cond {
    display: grid;
    gap: 6px;
    padding: 9px 11px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-sunken);
  }

  .cond[data-status="holds"] { border-left: 3px solid var(--mdx-accent-success); }
  .cond[data-status="broken"] { border-left: 3px solid var(--mdx-accent-danger, #c0392b); }
  .cond[data-status="being_tested"] { border-left: 3px solid var(--mdx-accent-primary); }

  .cond-row {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    align-items: baseline;
  }

  .cond-claim { font-size: 12.5px; line-height: 1.4; }

  .mob {
    margin-left: 6px;
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 9.5px;
    font-weight: 700;
    color: var(--mdx-accent-primary);
    border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 40%, transparent);
    white-space: nowrap;
  }

  .cond-status {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
    white-space: nowrap;
  }

  .cond-acts { display: flex; gap: 6px; flex-wrap: wrap; }


  .verdict { display: grid; gap: 6px; }

  .verdict input,
  .add-claim input[type="text"],
  .drawer textarea,
  .drawer input[type="text"],
  .drawer select {
    width: 100%;
    padding: 8px 10px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-sm);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 12.5px;
  }

  .add-claim {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    gap: 8px;
    align-items: center;
  }

  .mob-toggle {
    display: inline-flex;
    gap: 5px;
    align-items: center;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-tertiary);
    white-space: nowrap;
  }

  .f-label {
    font-family: var(--mdx-font-display);
    font-size: 12px;
    font-weight: 600;
    color: var(--mdx-text-secondary);
  }

  .f-label .opt { color: var(--mdx-text-muted); font-weight: 400; font-size: var(--mdx-text-xs); }

  .f-row { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .f-row > div { display: grid; gap: 4px; }

  .drawer-actions { display: flex; gap: 10px; align-items: center; }


  .flash { margin: 0; font-size: 12.5px; color: var(--mdx-tone-ok-text); }
  .flash.bad { color: var(--mdx-tone-danger-text); }

  code {
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-secondary);
  }

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
  .block-more > summary { color: var(--mdx-accent-primary); opacity: 0.8; }

  @media (max-width: 900px) {
    .board { grid-auto-columns: 248px; }
  }

  @media (prefers-reduced-motion: reduce) {
    * { transition: none !important; }
  }
</style>
