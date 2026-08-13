<script>
  // Product: the bets pipeline. Five stages derived from records - a bet
  // earns its anatomy in Shaping, its claims get judged with evidence,
  // a person backs it in the decision room, and landing records the
  // metric and the lesson. The hero at the top is v1's best pattern
  // revived: the one next move, named.
  import { createMdxClient } from "@mdx/client";
  import { invalidateAll } from "$app/navigation";
  import DecisionRoom from "@mdx/ui/components/DecisionRoom.svelte";
  import { PIPELINE_STAGES, pipelineFrom, heroFor, sectionsFor } from "../../lib/productPipeline.js";
  import {
    normalizeItems,
    itemsForBet,
    rolloutLineFor,
    myWork,
    nextStatusFor,
    ownedByAgent,
    statusLabel
  } from "../../lib/workPlane.js";

  let { data } = $props();

  const pipeline = $derived(pipelineFrom(data.pipeline));
  const workItems = $derived(normalizeItems(data.work));
  const hero = $derived(heroFor(pipeline));
  const waitingOnYou = $derived(
    pipeline.stages.find((stage) => stage.key === "ready")?.cards.length ?? 0
  );

  let selectedId = $state("");
  const selected = $derived(pipeline.cards.find((card) => card.betId === selectedId) ?? null);
  const selectedWork = $derived(itemsForBet(workItems, selectedId));

  const client = $derived(createMdxClient({ baseUrl: "/api/kernel", session: data.session }));
  const actor = $derived(data.session?.user_id ?? "local_user");
  const yourWork = $derived(myWork(workItems, actor));

  let newOpen = $state(false);
  let busy = $state(false);
  let flash = $state(null);
  let newBet = $state("");

  async function write(path, body, intent, okLine, okStatusField = "status") {
    if (busy) return null;
    busy = true;
    flash = null;
    try {
      const packet = await client.write(path, { ...body, actor_id: actor }, { receiptIntent: intent });
      const ok = packet[okStatusField] === "RECORDED";
      if (ok) {
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

  async function createBet() {
    if (!newBet.trim()) return;
    const packet = await write(
      "/product/bet-drafts.json",
      { bet: newBet.trim() },
      "product_bet_draft",
      "On the radar. Open it to shape the rest."
    );
    if (packet) {
      newBet = "";
      newOpen = false;
      selectedId = packet.bet_id ?? "";
      startShape();
    }
  }

  // ---- Shaping ----
  let shapeOpen = $state(false);
  let shape = $state(null);
  function startShape() {
    if (!selected && !shape) return;
    const card = selected;
    shape = {
      bet: card?.bet ?? "",
      for_whom: card?.forWhom ?? "",
      success_metric: card?.successMetric ?? "",
      kill_condition: card?.killCondition ?? "",
      slice: card?.slice ?? "",
      slice_not: card?.sliceNot ?? "",
      direction_ref: card?.directionRef ?? "",
      signal_ref: card?.signalRef ?? ""
    };
    shapeOpen = true;
  }
  async function saveShape() {
    if (!selected || !shape?.bet?.trim()) return;
    const body = { bet_id: selected.betId };
    for (const [key, value] of Object.entries(shape)) {
      if (String(value).trim()) body[key] = String(value).trim();
    }
    const packet = await write(
      "/product/bet-drafts.json", body,
      "product_bet_draft",
      "Shape saved. Every save is on the record."
    );
    if (packet) shapeOpen = false;
  }

  // ---- Conditions ----
  let claimDraft = $state("");
  let claimMob = $state(true);
  async function addCondition() {
    if (!selected || !claimDraft.trim()) return;
    const packet = await write(
      "/product/bet-conditions.json",
      { bet_id: selected.betId, claim: claimDraft.trim(), make_or_break: claimMob ? "true" : "false" },
      "product_bet_condition",
      "Claim stated - testing it is fair work for an agent."
    );
    if (packet) claimDraft = "";
  }

  let verdict = $state(null);
  function openVerdict(condition, status) {
    verdict = { conditionId: condition.conditionId, status, evidence_ref: "", note: "" };
  }
  async function startTesting(condition) {
    verdict = null;
    await write(
      "/product/bet-condition-updates.json",
      { condition_id: condition.conditionId, status: "being_tested", evidence_ref: "", note: "" },
      "product_bet_condition_update",
      "Under test."
    );
  }
  async function saveVerdict() {
    if (!verdict) return;
    const packet = await write(
      "/product/bet-condition-updates.json",
      { condition_id: verdict.conditionId, status: verdict.status,
        evidence_ref: verdict.evidence_ref.trim(), note: verdict.note.trim() },
      "product_bet_condition_update",
      "Verdict on the record, with its evidence."
    );
    if (packet) verdict = null;
  }

  // ---- Backing ----
  const CALL_OPTIONS = [
    { token: "back_the_bet", title: "Back the bet", line: "Your yes, written down with your name on it.", weight: "Committing" },
    { token: "hold_for_more_signal", title: "Hold for more signal", line: "Not yet - let the claims keep gathering proof.", weight: "Patient" },
    { token: "send_back_for_reshaping", title: "Send it back", line: "The bet needs reshaping before it deserves a yes.", weight: "Steady" }
  ];
  let chosen = $state("");
  let reasonDraft = $state("");
  let callResult = $state(null);
  async function makeTheCall(option) {
    if (!selected) return;
    const packet = await write(
      "/product/ratification-decisions.json",
      { bet_id: selected.betId, decision: option, reason: reasonDraft.trim() },
      "product_ratification_decision",
      "Your call is on the record. It starts no work by itself - that is the design."
    );
    if (packet) {
      chosen = option;
      callResult = { ok: true, line: "Your call is on the record.", receiptId: packet.decision_receipt_id ?? "" };
      reasonDraft = "";
    } else if (flash && !flash.ok) {
      callResult = { ok: false, line: flash.line, receiptId: "" };
    }
  }

  // ---- Landing ----
  let resolveOpen = $state(false);
  let metricDraft = $state("");
  let noMeasure = $state(false);
  let learnedDraft = $state("");
  async function resolveBet(outcome) {
    if (!selected) return;
    const body = {
      bet_id: selected.betId,
      outcome,
      learned: learnedDraft.trim()
    };
    if (outcome === "landed") {
      body.metric_result = metricDraft.trim();
      body.measurement_unavailable = noMeasure ? "true" : "false";
    }
    const packet = await write(
      "/product/bet-resolutions.json", body,
      "product_bet_resolution",
      outcome === "landed"
        ? "Landed, with the metric on the record."
        : outcome === "killed"
          ? "Killed, lesson kept. That is a success outcome here."
          : "Dismissed with its reasons."
    );
    if (packet) {
      metricDraft = "";
      learnedDraft = "";
      noMeasure = false;
      resolveOpen = false;
    }
  }

  let itemDraft = $state("");
  async function addWorkItem() {
    if (!selected || !itemDraft.trim()) return;
    const packet = await write(
      "/product/work-items.json",
      { title: itemDraft.trim(), bet_id: selected.betId },
      "work_item",
      "Added. It starts in intake."
    );
    if (packet) itemDraft = "";
  }

  const MOVE_VERB = {
    intake: "Queue it",
    up_next: "Start it",
    in_motion: "Send to review",
    in_review: "Accept — done"
  };
  async function moveItem(item) {
    const status = nextStatusFor(item.status);
    if (!status) return;
    await write(
      "/product/work-item-moves.json",
      { work_item_id: item.itemId, status },
      "work_item_move",
      status === "done" ? "Accepted. It is done on the record." : `Moved to ${statusLabel(status).toLowerCase()}.`
    );
  }
  async function dropItem(item) {
    await write(
      "/product/work-item-moves.json",
      { work_item_id: item.itemId, status: "dropped" },
      "work_item_move",
      "Dropped. It leaves the count — that is honest, not failure."
    );
  }
  // Taking a piece of work re-shapes it wholesale: every shape carries
  // the full anatomy, so the record stays one fold.
  async function takeItem(item) {
    await write(
      "/product/work-items.json",
      {
        work_item_id: item.itemId,
        title: item.title,
        description: item.description,
        bet_id: item.betId,
        owner_id: actor,
        approval: item.approval,
        page_ref: item.pageRef,
        build_ref: item.buildRef
      },
      "work_item",
      "It is yours."
    );
  }

  function selectCard(card) {
    selectedId = card.betId;
    chosen = "";
    callResult = null;
    verdict = null;
    resolveOpen = false;
    shapeOpen = false;
    flash = null;
  }

  function heroAct() {
    if (hero.kind === "new") {
      newOpen = true;
      selectedId = "";
      return;
    }
    const card = pipeline.cards.find((entry) => entry.betId === hero.betId);
    if (card) selectCard(card);
    if (hero.kind === "shape") startShape();
  }

  const STATUS_LABEL = { assumed: "assumed", being_tested: "testing", holds: "holds", broken: "broke" };
</script>

<svelte:head><title>Product - MDx</title></svelte:head>

<section class="product" data-route-state="ready">
  <header class="pipe-head">
    <div class="mdx-page-head">
      <h1>Product</h1>
      <p class="dogfood-line">Directing MDx with MDx: the bets below steer what Forge builds next.</p>
      <p class="mdx-page-sub">What we are building — and what it must prove.</p>
    </div>
    <div class="head-acts">
      <a class="incoming-link" href="/product-direction/feedback-autonomy">Autonomy</a>
      <a class="incoming-link" href="/product-direction/triage">Incoming{data.openSignal > 0 ? ` · ${data.openSignal}` : ""}</a>
      <button type="button" class="mdx-btn primary" onclick={() => { newOpen = true; selectedId = ""; }}>+ New bet</button>
    </div>
  </header>

  <div class="hero">
    <div class="hero-text">
      <span class="hero-eyebrow">Next decision</span>
      <p class="hero-line">{hero.line}</p>
    </div>
    <button type="button" class="hero-go" onclick={heroAct}>{hero.label}</button>
  </div>

  {#if waitingOnYou > 1}
    <p class="pulse">{waitingOnYou} bets are ready to back — the claims are judged.</p>
  {/if}

  {#if yourWork.length > 0}
    <div class="my-work" aria-label="Your work">
      <span class="my-work-label">Your work</span>
      <ul class="my-work-list">
        {#each yourWork as item (item.itemId)}
          <li>
            <button type="button" class="my-work-item" onclick={() => (selectedId = item.betId || selectedId)}>
              <span class="work-dot" data-status={item.status} aria-hidden="true"></span>
              {item.title}
              <span class="work-status">{statusLabel(item.status)}</span>
            </button>
          </li>
        {/each}
      </ul>
    </div>
  {/if}

  <div class="pipe" role="list" aria-label="The bets pipeline">
    {#each pipeline.stages as stage}
      <div class="column" role="listitem" data-stage={stage.key}>
        <div class="col-head">
          <span class="col-label">{stage.label}</span>
          {#if stage.cards.length > 0}<span class="col-count">{stage.cards.length}</span>{/if}
        </div>
        <p class="col-cue">{stage.cue}</p>
        {#each stage.cards as card (card.betId)}
          <button type="button" class="card" class:open={selectedId === card.betId} onclick={() => selectCard(card)}>
            <span class="card-title">{card.bet}</span>
            <span class="card-meta">
              {#if card.stage === "shaping" || card.stage === "radar"}
                <span class="chip">{card.meter.done}/{card.meter.total} spelled out</span>
              {/if}
              {#if card.scoreboard.makeOrBreak > 0}
                <span class="chip score" class:good={card.scoreboard.holds === card.scoreboard.makeOrBreak} class:bad={card.scoreboard.broken > 0}>
                  {card.scoreboard.holds}/{card.scoreboard.makeOrBreak} hold{card.scoreboard.broken > 0 ? ` · ${card.scoreboard.broken} broke` : ""}
                </span>
              {/if}
              {#if card.stage === "in_build" && rolloutLineFor(workItems, card.betId)}<span class="chip">{rolloutLineFor(workItems, card.betId)}</span>{/if}
              {#if card.resolution}<span class="chip done">{card.resolution.outcome}</span>{/if}
            </span>
          </button>
        {/each}
        {#if stage.cards.length === 0}<div class="col-empty" aria-hidden="true"></div>{/if}
      </div>
    {/each}
  </div>

  {#if flash && !selected && !newOpen}<p class="flash" class:bad={!flash.ok}>{flash.line}</p>{/if}

  <footer class="quiet-line">
    Bets move on records, never by hand — and backing one opens nothing by itself.
    <details class="quiet-more">
      <summary>more</summary>
      <span>Boundary: {data.boundary}. Safe next move: {data.safeNext}.</span>
    </details>
  </footer>
</section>

{#if newOpen}
  <button type="button" class="backdrop" aria-label="Close" onclick={() => (newOpen = false)}></button>
  <aside class="drawer" aria-label="New bet">
    <div class="drawer-head">
      <h2>Put a bet on the radar</h2>
      <button type="button" class="drawer-close" onclick={() => (newOpen = false)} aria-label="Close">×</button>
    </div>
    <label class="f-label" for="nb-bet">The bet, in one sentence</label>
    <textarea id="nb-bet" rows="2" bind:value={newBet} placeholder="What should we build next, and for whom?"></textarea>
    <div class="drawer-actions">
      <button type="button" class="mdx-btn primary" disabled={busy || !newBet.trim()} onclick={createBet}>
        {busy ? "Recording..." : "Put it on the radar"}
      </button>
      <button type="button" class="mdx-btn small" onclick={() => (newOpen = false)}>Never mind</button>
    </div>
    {#if flash}<p class="flash" class:bad={!flash.ok}>{flash.line}</p>{/if}
  </aside>
{/if}

{#if selected}
  <button type="button" class="backdrop" aria-label="Close" onclick={() => (selectedId = "")}></button>
  <aside class="drawer" aria-label="Bet detail">
    <div class="drawer-head">
      <h2>{selected.bet}</h2>
      <button type="button" class="drawer-close" onclick={() => (selectedId = "")} aria-label="Close">×</button>
    </div>

    {#if !shapeOpen}
      <div class="meter-row">
        <span class="meter-text">{selected.meter.done}/{selected.meter.total} spelled out{selected.meter.missing.length > 0 ? ` — still needs: ${selected.meter.missing.join(", ").toLowerCase()}` : ""}</span>
        {#if !selected.resolution}
          <button type="button" class="mdx-btn small" onclick={startShape}>Shape it</button>
        {/if}
      </div>
      <dl class="d-facts">
        {#if selected.forWhom}<div><dt>For whom</dt><dd>{selected.forWhom}</dd></div>{/if}
        {#if selected.successMetric}<div><dt>Success looks like</dt><dd>{selected.successMetric}</dd></div>{/if}
        {#if selected.killCondition}<div><dt>We stop if</dt><dd>{selected.killCondition}</dd></div>{/if}
        {#if selected.slice}<div><dt>What ships first</dt><dd>{selected.slice}</dd></div>{/if}
        {#if selected.sliceNot}<div><dt>Explicitly not</dt><dd>{selected.sliceNot}</dd></div>{/if}
        {#if selected.directionRef}<div><dt>Serves</dt><dd><a class="ref" href="/strategy">{selected.directionRef}</a></dd></div>{/if}
        {#if selected.signalRef}<div><dt>Came from</dt><dd><code>{selected.signalRef}</code></dd></div>{/if}
      </dl>
      <!-- The bet-to-build seam: direction becomes work in one click. Forge's
           admission gates apply there; this only carries the words over.
           Product is dogfooding first - MDx directing MDx (program arc S5). -->
      <a
        class="mdx-btn primary small bet-to-forge"
        href={`/forge?intent=${encodeURIComponent(`${selected.bet ?? selected.title ?? ""}${selected.slice ? ` - ship first: ${selected.slice}` : ""}`.slice(0, 500))}`}
      >Hand to Forge &rarr;</a>
    {:else}
      <div class="d-section">
        <h3>Shape the bet</h3>
        <label class="f-label" for="sh-bet">The bet</label>
        <textarea id="sh-bet" rows="2" bind:value={shape.bet}></textarea>
        <div class="f-row">
          <div><label class="f-label" for="sh-who">For whom</label><input id="sh-who" type="text" bind:value={shape.for_whom} /></div>
          <div><label class="f-label" for="sh-metric">Success looks like</label><input id="sh-metric" type="text" bind:value={shape.success_metric} /></div>
        </div>
        <div class="f-row">
          <div><label class="f-label" for="sh-kill">We stop if</label><input id="sh-kill" type="text" bind:value={shape.kill_condition} /></div>
          <div><label class="f-label" for="sh-dir">Direction it serves</label><input id="sh-dir" type="text" bind:value={shape.direction_ref} placeholder="A strategy card id" /></div>
        </div>
        <label class="f-label" for="sh-slice">What ships first</label>
        <textarea id="sh-slice" rows="2" bind:value={shape.slice}></textarea>
        <label class="f-label" for="sh-not">Explicitly not</label>
        <input id="sh-not" type="text" bind:value={shape.slice_not} />
        <div class="drawer-actions">
          <button type="button" class="mdx-btn primary" disabled={busy || !shape.bet.trim()} onclick={saveShape}>{busy ? "Saving..." : "Save the shape"}</button>
          <button type="button" class="mdx-btn small" onclick={() => (shapeOpen = false)}>Cancel</button>
        </div>
      </div>
    {/if}

    {#if !shapeOpen}
      <div class="d-section">
        <h3>What would have to be true</h3>
        {#if selected.conditions.length === 0}
          <p class="d-cue">No claims yet. State the make-or-break ones — the bet moves when they get verdicts.</p>
        {/if}
        <ul class="conds">
          {#each selected.conditions as condition (condition.conditionId)}
            <li class="cond" data-status={condition.status}>
              <div class="cond-row">
                <span class="cond-claim">
                  {condition.claim}
                  {#if condition.makeOrBreak}<span class="mob">make-or-break</span>{/if}
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

      <div class="d-section">
        <h3>The work</h3>
        {#if selectedWork.length === 0}
          <p class="d-cue">Nothing broken out yet. Add the first piece — something one owner can pick up and finish.</p>
        {/if}
        <ul class="work-rows">
          {#each selectedWork as item (item.itemId)}
            <li class="work-row" data-status={item.status}>
              <span class="work-dot" data-status={item.status} aria-hidden="true"></span>
              <span class="work-title">{item.title}</span>
              <span class="work-status">{statusLabel(item.status)}</span>
              {#if item.ownerId}
                <span class="work-owner" class:agent={ownedByAgent(item)} title={item.ownerId}>{item.ownerId.split(":").at(-1)}</span>
              {:else if !selected.resolution}
                <button type="button" class="mdx-btn small" disabled={busy} onclick={() => takeItem(item)}>I'll take it</button>
              {/if}
              {#if nextStatusFor(item.status) && !selected.resolution}
                <button type="button" class="mdx-btn small" disabled={busy} onclick={() => moveItem(item)}>{MOVE_VERB[item.status]}</button>
              {/if}
              {#if item.status !== "done" && item.status !== "dropped" && !selected.resolution}
                <a class="mini ghost work-forge" href="/forge/runs?work_item={item.itemId}" title="Build this with Forge">Build with Forge</a>
                <button type="button" class="mini ghost work-drop" disabled={busy} onclick={() => dropItem(item)} aria-label="Drop this piece of work">drop</button>
              {/if}
            </li>
          {/each}
        </ul>
        {#if !selected.resolution}
          <div class="add-claim">
            <input type="text" bind:value={itemDraft} placeholder="A piece of work one owner can pick up..." aria-label="New piece of work" />
            <button type="button" class="mdx-btn small primary" disabled={busy || !itemDraft.trim()} onclick={addWorkItem}>Add</button>
          </div>
        {/if}
      </div>

      {#if !selected.resolution && !selected.backed}
        <div class="d-section">
          <DecisionRoom
            eyebrow="Your call on this bet"
            question="Where do you stand?"
            whyNow={selected.scoreboard.makeOrBreak > 0
              ? `${selected.scoreboard.holds} of ${selected.scoreboard.makeOrBreak} make-or-break claims hold${selected.scoreboard.broken > 0 ? `, ${selected.scoreboard.broken} broke` : ""}`
              : `${selected.meter.done} of ${selected.meter.total} sections spelled out`}
            options={CALL_OPTIONS}
            {chosen}
            deciding={busy}
            bind:reason={reasonDraft}
            result={callResult}
            onDecide={makeTheCall}
          />
        </div>
      {/if}

      {#if selected.backed && !selected.resolution}
        <p class="d-standing">Backed — break the work out below, or build it with Forge.</p>
      {/if}

      {#if !selected.resolution}
        <div class="d-section">
          {#if !resolveOpen}
            <button type="button" class="mdx-btn small" onclick={() => (resolveOpen = true)}>Resolve this bet...</button>
          {:else}
            <h3>Resolve it</h3>
            <input type="text" bind:value={metricDraft} placeholder="The metric result (for a landing)" aria-label="Metric result" />
            <label class="mob-toggle"><input type="checkbox" bind:checked={noMeasure} /> measurement was unavailable — say so plainly</label>
            <input type="text" bind:value={learnedDraft} placeholder="What we learned (kept forever)" aria-label="What we learned" />
            <div class="cond-acts">
              <button type="button" class="mdx-btn small primary" disabled={busy} onclick={() => resolveBet("landed")}>Landed</button>
              <button type="button" class="mdx-btn small" disabled={busy} onclick={() => resolveBet("killed")}>Kill it</button>
              <button type="button" class="mdx-btn small" disabled={busy} onclick={() => resolveBet("dismissed")}>Dismiss</button>
              <button type="button" class="mdx-btn small" onclick={() => (resolveOpen = false)}>Cancel</button>
            </div>
          {/if}
        </div>
      {/if}

      {#if selected.resolution}
        <p class="d-standing">
          {selected.resolution.outcome === "landed" ? "Landed" : selected.resolution.outcome === "killed" ? "Killed" : "Dismissed"}{selected.resolution.metric_result ? ` — ${selected.resolution.metric_result}` : selected.resolution.measurement_unavailable ? " — measurement was unavailable" : ""}{selected.resolution.learned ? `. ${selected.resolution.learned}` : "."}
        </p>
      {/if}

      {#if flash}<p class="flash" class:bad={!flash.ok}>{flash.line}</p>{/if}

      <details class="quiet-more block-more">
        <summary>view the record</summary>
        <dl class="d-facts small">
          <div><dt>Bet</dt><dd><code>{selected.betId}</code></dd></div>
          <div><dt>Receipt</dt><dd><code>{selected.receiptId}</code></dd></div>
        </dl>
      </details>
    {/if}
  </aside>
{/if}

<style>
  .product { display: grid; gap: 20px; align-content: start; padding: 2vh 0 40px; }

  .pipe-head { display: flex; align-items: flex-end; justify-content: space-between; gap: 16px; flex-wrap: wrap; }

  .head-acts { display: flex; align-items: center; gap: 10px; }
  .incoming-link {
    font-size: 13px; color: inherit; text-decoration: none;
    padding: 8px 14px; border-radius: var(--mdx-radius-md);
    border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 22%, transparent);
  }
  .incoming-link:hover { border-color: var(--mdx-accent-primary); }

  .hero {
    display: flex; align-items: center; justify-content: space-between; gap: 16px;
    padding: 16px 20px; border-radius: var(--mdx-radius-lg);
    background: linear-gradient(120deg, color-mix(in srgb, var(--mdx-accent-primary) 10%, var(--mdx-surface-raised)), var(--mdx-surface-raised));
    border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 25%, transparent);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }
  .hero-text { display: grid; gap: 3px; }
  .hero-eyebrow { font-size: var(--mdx-text-xs); font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: var(--mdx-accent-primary); }
  .hero-line { margin: 0; font-family: var(--mdx-font-display); font-size: 15.5px; font-weight: 600; line-height: 1.4; }
  .hero-go {
    flex-shrink: 0; padding: 10px 18px; border: none; border-radius: var(--mdx-radius-md);
    background: var(--mdx-accent-primary); color: #fff;
    font-family: var(--mdx-font-display); font-size: 13.5px; font-weight: 650; cursor: pointer;
  }
  .hero-go:hover { filter: brightness(1.06); }

  .pulse { margin: 0; color: var(--mdx-text-secondary); font-size: 15px; }

  /* Fixed-width columns + intentional horizontal scroll (see Strategy board):
     stretch-to-fit clipped the tail lane off-screen at 1440. */
  .pipe { display: grid; grid-auto-flow: column; grid-auto-columns: 280px; gap: 12px; overflow-x: auto; padding-bottom: 10px; scrollbar-width: thin; }

  .column {
    display: grid; gap: 8px; align-content: start; min-height: 180px; padding: 10px;
    border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-sunken);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }
  .col-head { display: flex; align-items: baseline; justify-content: space-between; }
  .col-label { font-family: var(--mdx-font-display); font-size: 12.5px; font-weight: 700; letter-spacing: 0.02em; color: var(--mdx-text-secondary); }
  .col-count { font-size: var(--mdx-text-xs); color: var(--mdx-text-muted); }
  .col-cue { margin: 0 0 2px; color: var(--mdx-text-muted); font-size: var(--mdx-text-xs); line-height: 1.4; }

  .card {
    display: grid; gap: 6px; padding: 10px 12px;
    border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised); text-align: left; cursor: pointer; font: inherit; color: var(--mdx-text-primary);
  }
  .card:hover { border-color: var(--mdx-accent-primary); }
  .card.open { border-color: var(--mdx-accent-primary); box-shadow: 0 0 0 2px color-mix(in srgb, var(--mdx-accent-primary) 25%, transparent); }
  .card-title { font-size: 13px; font-weight: 600; line-height: 1.35; }
  .card-meta { display: flex; gap: 6px; flex-wrap: wrap; }

  .chip {
    padding: 2px 7px; border-radius: 999px; font-size: var(--mdx-text-xs); font-weight: 600;
    background: var(--mdx-surface-sunken); color: var(--mdx-text-tertiary); border: 1px solid var(--mdx-border-subtle);
  }
  .chip.score.good { color: var(--mdx-accent-success); border-color: color-mix(in srgb, var(--mdx-accent-success) 40%, transparent); }
  .chip.score.bad { color: var(--mdx-accent-danger, #c0392b); border-color: color-mix(in srgb, var(--mdx-accent-danger, #c0392b) 40%, transparent); }
  .chip.done { text-transform: capitalize; }

  .col-empty { min-height: 40px; border: 1.5px dashed var(--mdx-border-subtle); border-radius: var(--mdx-radius-md); opacity: 0.6; }

  .backdrop { position: fixed; inset: 0; border: none; background: rgba(15, 18, 24, 0.18); cursor: pointer; z-index: 29; }

  .drawer :global(.options) { display: grid !important; grid-template-columns: 1fr !important; gap: 8px; }

  .drawer {
    position: fixed; top: 0; right: 0; bottom: 0; width: min(460px, 92vw); overflow-y: auto;
    display: grid; gap: 14px; align-content: start; padding: 22px 22px 40px;
    background: var(--mdx-surface-raised); border-left: 1px solid var(--mdx-border-subtle);
    box-shadow: -12px 0 32px rgba(0, 0, 0, 0.08); z-index: 30;
  }
  .drawer-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 10px; }
  .drawer-head h2 { margin: 0; font-family: var(--mdx-font-display); font-size: 18px; line-height: 1.3; }
  .drawer-close { border: none; background: none; font-size: 22px; line-height: 1; color: var(--mdx-text-muted); cursor: pointer; }

  .meter-row { display: flex; align-items: center; justify-content: space-between; gap: 10px; }
  .meter-text { font-size: 12px; color: var(--mdx-text-tertiary); line-height: 1.45; }

  .d-facts { display: grid; gap: 8px; margin: 0; }
  .d-facts div { display: grid; grid-template-columns: 120px minmax(0, 1fr); gap: 8px; }
  .d-facts.small div { grid-template-columns: 70px minmax(0, 1fr); }
  .d-facts dt { color: var(--mdx-text-tertiary); font-size: var(--mdx-text-xs); }
  .d-facts dd { margin: 0; font-size: 12.5px; line-height: 1.45; color: var(--mdx-text-secondary); overflow-wrap: anywhere; }

  .ref { color: var(--mdx-accent-primary); font-weight: 600; text-decoration: none; }
  .ref:hover { text-decoration: underline; }

  .d-section { display: grid; gap: 10px; }
  .d-section h3 { margin: 0; font-family: var(--mdx-font-display); font-size: 14px; font-weight: 650; }
  .d-cue { margin: 0; color: var(--mdx-text-tertiary); font-size: 12.5px; }
  .d-standing { margin: 0; font-size: 13px; color: var(--mdx-text-secondary); }

  .conds { display: grid; gap: 8px; margin: 0; padding: 0; list-style: none; }
  .cond {
    display: grid; gap: 6px; padding: 9px 11px;
    border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-sunken);
  }
  .cond[data-status="holds"] { border-left: 3px solid var(--mdx-accent-success); }
  .cond[data-status="broken"] { border-left: 3px solid var(--mdx-accent-danger, #c0392b); }
  .cond[data-status="being_tested"] { border-left: 3px solid var(--mdx-accent-primary); }
  .cond-row { display: flex; justify-content: space-between; gap: 10px; align-items: baseline; }
  .cond-claim { font-size: 12.5px; line-height: 1.4; }
  .mob {
    margin-left: 6px; padding: 1px 6px; border-radius: 999px; font-size: 9.5px; font-weight: 700;
    color: var(--mdx-accent-primary); border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 40%, transparent); white-space: nowrap;
  }
  .cond-status { font-size: var(--mdx-text-xs); color: var(--mdx-text-muted); white-space: nowrap; }
  .cond-acts { display: flex; gap: 6px; flex-wrap: wrap; }


  .verdict { display: grid; gap: 6px; }

  .verdict input,
  .add-claim input[type="text"],
  .drawer textarea,
  .drawer input[type="text"] {
    width: 100%; padding: 8px 10px;
    border: 1px solid var(--mdx-border-default); border-radius: var(--mdx-radius-sm);
    background: var(--mdx-surface-raised); color: var(--mdx-text-primary); font: inherit; font-size: 12.5px;
  }

  .add-claim { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; gap: 8px; align-items: center; }

  /* The work rows: one glance per row - dot (status geometry), title
     flexing, status word, owner last. 4px grid, calm rest state. */
  .work-rows { list-style: none; margin: 0 0 10px; padding: 0; display: grid; gap: 4px; }
  .work-row {
    display: flex; align-items: center; gap: 8px; min-height: 36px;
    padding: 4px 8px; border-radius: var(--mdx-radius-md);
  }
  .work-row:hover { background: color-mix(in srgb, var(--mdx-accent-primary) 5%, transparent); }
  .work-forge { text-decoration: none; color: var(--mdx-accent-primary); }
  .work-row .mdx-btn, .work-row .work-drop { visibility: hidden; }
  .work-row:hover .mdx-btn, .work-row:hover .work-drop, .work-row:focus-within .mdx-btn { visibility: visible; }
  .work-dot {
    width: 10px; height: 10px; border-radius: 50%; flex: none;
    border: 1.5px solid var(--mdx-text-muted); background: transparent;
  }
  .work-dot[data-status="up_next"] { border-style: dashed; }
  .work-dot[data-status="in_motion"] { background: linear-gradient(90deg, var(--mdx-accent-primary) 50%, transparent 50%); border-color: var(--mdx-accent-primary); }
  .work-dot[data-status="in_review"] { background: color-mix(in srgb, var(--mdx-accent-primary) 35%, transparent); border-color: var(--mdx-accent-primary); }
  .work-dot[data-status="done"] { background: var(--mdx-accent-primary); border-color: var(--mdx-accent-primary); }
  .work-dot[data-status="dropped"] { border-color: color-mix(in srgb, var(--mdx-text-muted) 50%, transparent); }
  .work-row[data-status="done"] .work-title, .work-row[data-status="dropped"] .work-title { color: var(--mdx-text-muted); }
  .work-row[data-status="dropped"] .work-title { text-decoration: line-through; }
  .work-title { flex: 1; min-width: 0; font-size: 13.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .work-status { font-size: 12px; color: var(--mdx-text-muted); flex: none; }
  .work-owner {
    flex: none; font-size: 12px; color: var(--mdx-text-muted);
    padding: 2px 8px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-text-muted) 10%, transparent);
  }
  .work-owner.agent { border-radius: 6px; }
  .work-drop { font-size: var(--mdx-text-xs); }

  .my-work {
    display: flex; align-items: baseline; gap: 12px; flex-wrap: wrap;
    margin: 0 0 4px; padding: 10px 14px; border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }
  .my-work-label { font-family: var(--mdx-font-display); font-size: 13px; font-weight: 650; }
  .my-work-list { list-style: none; display: flex; gap: 8px; flex-wrap: wrap; margin: 0; padding: 0; }
  .my-work-item {
    display: inline-flex; align-items: center; gap: 6px; border: none; cursor: pointer;
    font-size: 12.5px; padding: 4px 10px; border-radius: 999px;
    background: color-mix(in srgb, var(--mdx-accent-primary) 8%, transparent); color: inherit;
  }
  .my-work-item .work-status { font-size: var(--mdx-text-xs); }
  .mob-toggle { display: inline-flex; gap: 5px; align-items: center; font-size: var(--mdx-text-xs); color: var(--mdx-text-tertiary); white-space: nowrap; }

  .f-label { font-family: var(--mdx-font-display); font-size: 12px; font-weight: 600; color: var(--mdx-text-secondary); }
  .f-row { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; }
  .f-row > div { display: grid; gap: 4px; }

  .drawer-actions { display: flex; gap: 10px; align-items: center; }

  .mdx-btn.primary { justify-self: start; }

  .flash { margin: 0; font-size: 12.5px; color: var(--mdx-tone-ok-text); }
  .flash.bad { color: var(--mdx-tone-danger-text); }

  code { font-family: var(--mdx-font-mono); font-size: var(--mdx-text-xs); color: var(--mdx-text-secondary); }

  .quiet-line { display: flex; align-items: baseline; gap: 8px; color: var(--mdx-text-muted); font-size: var(--mdx-text-xs); }
  .quiet-more { display: inline; }
  .quiet-more > summary { display: inline; color: var(--mdx-text-muted); cursor: pointer; list-style: none; font-size: var(--mdx-text-xs); }
  .quiet-more > summary::-webkit-details-marker { display: none; }
  .quiet-line .quiet-more[open] > summary { display: none; }
  .block-more > summary { color: var(--mdx-accent-primary); opacity: 0.8; }

  @media (max-width: 900px) { .pipe { grid-auto-columns: 248px; } }
  @media (prefers-reduced-motion: reduce) { * { transition: none !important; } }
  .bet-to-forge { margin-top: 12px; display: inline-block; text-decoration: none; }
  .dogfood-line { margin: 2px 0 0; font-size: 12.5px; color: var(--mdx-text-muted); }
</style>
