<script>
  // Incoming: the one stream where signal waits for a human call.
  // Linear's triage rhythm, kept: the queue auto-selects its top entry,
  // every verdict is a single key (1/2/3/H), and recording one advances
  // to the next. Accepting turns signal into work or a Radar bet - the
  // kernel mints it from the entry's own words.
  import { createMdxClient } from "@mdx/client";
  import { invalidateAll } from "$app/navigation";
  import {
    VERDICTS,
    verdictByHotkey,
    normalizeStream,
    openEntries,
    calledEntries,
    calledLine,
    sourceLine
  } from "../../../lib/triageStream.js";

  let { data } = $props();

  const entries = $derived(normalizeStream(data.stream));
  const open = $derived(openEntries(entries));
  const called = $derived(calledEntries(entries));
  const bets = $derived(
    (Array.isArray(data.bets?.bets) ? data.bets.bets : []).map((bet) => ({
      betId: String(bet?.bet_id ?? ""),
      bet: String(bet?.bet ?? "")
    }))
  );

  const client = $derived(createMdxClient({ baseUrl: "/api/kernel", session: data.session }));
  const actor = $derived(data.session?.user_id ?? "local_user");

  let selectedRef = $state("");
  const selected = $derived(
    open.find((entry) => entry.entryRef === selectedRef) ?? open[0] ?? null
  );

  let busy = $state(false);
  let flash = $state(null);
  let expanded = $state(""); // "" | "accepted" | "duplicate"
  let acceptAs = $state("work_item");
  let acceptBetId = $state("");
  let duplicateOf = $state("");
  let noteDraft = $state("");
  let pasteDraft = $state("");

  function resetCall() {
    expanded = "";
    acceptAs = "work_item";
    acceptBetId = "";
    duplicateOf = "";
    noteDraft = "";
  }

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

  // After a call records, the queue advances by itself.
  function advanceFrom(entryRef) {
    const at = open.findIndex((entry) => entry.entryRef === entryRef);
    const next = open[at + 1] ?? open[at - 1] ?? null;
    selectedRef = next?.entryRef ?? "";
    resetCall();
  }

  async function recordVerdict(verdictKey) {
    if (!selected) return;
    if (verdictKey === "accepted" && expanded !== "accepted") {
      expanded = "accepted";
      return;
    }
    if (verdictKey === "duplicate" && expanded !== "duplicate") {
      expanded = "duplicate";
      return;
    }
    const body = { entry_ref: selected.entryRef, verdict: verdictKey, note: noteDraft.trim() };
    if (verdictKey === "accepted") {
      body.accept_as = acceptAs;
      if (acceptAs === "work_item" && acceptBetId) body.bet_id = acceptBetId;
    }
    if (verdictKey === "duplicate") {
      if (!duplicateOf.trim()) return;
      body.duplicate_of = duplicateOf.trim();
    }
    const ref = selected.entryRef;
    const packet = await write(
      "/product/triage-verdicts.json", body, "triage_verdict",
      verdictKey === "accepted"
        ? acceptAs === "bet" ? "Accepted - it is a bet on the Radar now." : "Accepted - it is a piece of work now."
        : verdictKey === "duplicate" ? "Marked as a duplicate."
        : verdictKey === "declined" ? "Declined, kept on the record."
        : "Snoozed - it stays in the stream."
    );
    if (packet) advanceFrom(ref);
  }

  async function pasteSignal() {
    if (!pasteDraft.trim()) return;
    const packet = await write(
      "/product/triage-entries.json",
      { text: pasteDraft.trim(), source: "manual" },
      "triage_entry",
      "In the stream."
    );
    if (packet) pasteDraft = "";
  }

  // One key per call, only when no field has focus.
  function onKeydown(event) {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const tag = event.target?.tagName?.toLowerCase();
    if (tag === "input" || tag === "textarea" || tag === "select") return;
    const verdict = verdictByHotkey(event.key);
    if (verdict && selected) {
      event.preventDefault();
      recordVerdict(verdict.key);
    }
  }
</script>

<svelte:head><title>Incoming - MDx</title></svelte:head>

<svelte:window onkeydown={onKeydown} />

<section class="triage" data-route-state="ready">
  <header class="head">
    <div class="mdx-page-head">
      <a class="mdx-crumb" href="/product-direction">Product</a>
      <h1>Incoming</h1>
      <p class="mdx-page-sub">Feedback, flagged messages, and pasted signal — each waits for one call.</p>
    </div>
  </header>

  <div class="paste">
    <input type="text" bind:value={pasteDraft} placeholder="Paste a piece of signal — a request, an idea, a complaint..." aria-label="New signal" onkeydown={(event) => event.key === "Enter" && pasteSignal()} />
    <button type="button" class="mdx-btn small primary" disabled={busy || !pasteDraft.trim()} onclick={pasteSignal}>Add</button>
  </div>

  {#if flash}<p class="flash" class:bad={!flash.ok}>{flash.line}</p>{/if}

  {#if open.length === 0}
    <div class="empty">
      <p>The stream is clear. New feedback and flagged messages land here on their own.</p>
    </div>
  {:else}
    <div class="split">
      <ul class="queue" aria-label="Waiting for a call">
        {#each open as entry (entry.entryRef)}
          <li>
            <button
              type="button"
              class="q-row"
              class:current={selected?.entryRef === entry.entryRef}
              onclick={() => { selectedRef = entry.entryRef; resetCall(); }}
            >
              <span class="q-source">{sourceLine(entry.source)}</span>
              <span class="q-text">{entry.text || "(no words came with it)"}</span>
              {#if entry.verdict?.verdict === "snoozed"}<span class="q-snooze">snoozed once</span>{/if}
            </button>
          </li>
        {/each}
      </ul>

      {#if selected}
        <div class="detail">
          <p class="d-source">{sourceLine(selected.source)}{selected.sourceRef ? ` · ${selected.sourceRef}` : ""}</p>
          <p class="d-text">{selected.text || "(no words came with it)"}</p>

          <div class="bar" role="group" aria-label="The call">
            {#each VERDICTS as verdict (verdict.key)}
              <button type="button" class="call" class:armed={expanded === verdict.key} disabled={busy} onclick={() => recordVerdict(verdict.key)}>
                <kbd>{verdict.hotkey.toUpperCase()}</kbd>
                <span class="call-label">{verdict.label}</span>
                <span class="call-hint">{verdict.hint}</span>
              </button>
            {/each}
          </div>

          {#if expanded === "accepted"}
            <div class="expand">
              <div class="pick">
                <label class="pick-opt"><input type="radio" bind:group={acceptAs} value="work_item" /> A piece of work</label>
                <label class="pick-opt"><input type="radio" bind:group={acceptAs} value="bet" /> A bet on the Radar</label>
              </div>
              {#if acceptAs === "work_item" && bets.length > 0}
                <select bind:value={acceptBetId} aria-label="The bet it serves">
                  <option value="">No bet yet — it can attach later</option>
                  {#each bets as bet (bet.betId)}
                    <option value={bet.betId}>{bet.bet}</option>
                  {/each}
                </select>
              {/if}
              <input type="text" bind:value={noteDraft} placeholder="One line on why (optional)" aria-label="Note" />
              <button type="button" class="mdx-btn primary" disabled={busy} onclick={() => recordVerdict("accepted")}>
                {acceptAs === "bet" ? "Accept as a bet" : "Accept as work"}
              </button>
            </div>
          {/if}

          {#if expanded === "duplicate"}
            <div class="expand">
              <input type="text" bind:value={duplicateOf} placeholder="The entry it repeats (its id)" aria-label="Duplicate of" />
              <button type="button" class="mdx-btn primary" disabled={busy || !duplicateOf.trim()} onclick={() => recordVerdict("duplicate")}>Mark as duplicate</button>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  {#if called.length > 0}
    <details class="history">
      <summary>Called already · {called.length}</summary>
      <ul>
        {#each called as entry (entry.entryRef)}
          <li>
            <span class="h-text">{entry.text}</span>
            <span class="h-line">{calledLine(entry)}</span>
          </li>
        {/each}
      </ul>
    </details>
  {/if}

  <footer class="quiet-line">
    Four calls and nothing else — and accepting writes the work or the bet on the record.
    <details class="quiet-more">
      <summary>more</summary>
      <span>Boundary: {data.boundary}. Safe next move: {data.safeNext}.</span>
    </details>
  </footer>
</section>

<style>
  .triage { display: grid; gap: 16px; max-width: 980px; }
  .head { display: flex; align-items: flex-end; justify-content: space-between; gap: 12px; }

  .paste { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 8px; }
  .paste input {
    padding: 9px 12px; border-radius: var(--mdx-radius-md); font-size: 13.5px;
    border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 25%, transparent);
    background: var(--mdx-surface-raised); color: inherit;
  }

  .split { display: grid; grid-template-columns: minmax(240px, 360px) minmax(0, 1fr); gap: 16px; align-items: start; }
  .queue { list-style: none; margin: 0; padding: 0; display: grid; gap: 4px; }
  .q-row {
    display: grid; gap: 2px; width: 100%; text-align: left; cursor: pointer;
    padding: 8px 10px; border: none; border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised); color: inherit;
  }
  .q-row.current { outline: 2px solid var(--mdx-accent-primary); outline-offset: -2px; }
  .q-source { font-size: var(--mdx-text-xs); color: var(--mdx-text-muted); }
  .q-text { font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .q-snooze { font-size: var(--mdx-text-xs); color: var(--mdx-text-muted); }

  .detail {
    display: grid; gap: 14px; padding: 16px;
    border-radius: var(--mdx-radius-lg); background: var(--mdx-surface-raised);
    border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 12%, transparent);
  }
  .d-source { margin: 0; font-size: 12px; color: var(--mdx-text-muted); }
  .d-text { margin: 0; font-size: 16px; line-height: 1.5; }

  .bar { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px; }
  .call {
    display: grid; gap: 2px; justify-items: start; text-align: left; cursor: pointer;
    padding: 10px 12px; border-radius: var(--mdx-radius-md); color: inherit;
    border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 18%, transparent);
    background: transparent;
  }
  .call:hover, .call.armed { border-color: var(--mdx-accent-primary); background: color-mix(in srgb, var(--mdx-accent-primary) 6%, transparent); }
  .call kbd {
    font-size: var(--mdx-text-xs); padding: 1px 6px; border-radius: 4px;
    background: color-mix(in srgb, var(--mdx-text-muted) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 20%, transparent);
  }
  .call-label { font-family: var(--mdx-font-display); font-size: 13.5px; font-weight: 650; }
  .call-hint { font-size: var(--mdx-text-xs); color: var(--mdx-text-muted); }

  .expand { display: grid; gap: 10px; }
  .pick { display: flex; gap: 16px; }
  .pick-opt { display: inline-flex; align-items: center; gap: 6px; font-size: 13px; }
  .expand input[type="text"], .expand select {
    padding: 8px 10px; border-radius: var(--mdx-radius-md); font-size: 13px;
    border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 25%, transparent);
    background: var(--mdx-surface-base, transparent); color: inherit;
  }
  .mdx-btn.primary { justify-self: start; }

  .empty { padding: 28px 20px; border-radius: var(--mdx-radius-lg); background: var(--mdx-surface-raised); }
  .empty p { margin: 0; color: var(--mdx-text-muted); font-size: 14px; }

  .history summary { cursor: pointer; font-size: 13px; color: var(--mdx-text-muted); }
  .history ul { list-style: none; margin: 8px 0 0; padding: 0; display: grid; gap: 4px; }
  .history li { display: flex; gap: 10px; align-items: baseline; min-height: 28px; }
  .h-text { font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--mdx-text-muted); }
  .h-line { font-size: 12px; flex: none; }

  .flash { margin: 0; font-size: 13px; }
  .flash.bad { color: var(--mdx-tone-danger-text); }

  .quiet-line { font-size: 12.5px; color: var(--mdx-text-muted); }
  .quiet-more { display: inline; }
  .quiet-more summary { display: inline; cursor: pointer; }

  @media (max-width: 760px) {
    .split { grid-template-columns: 1fr; }
    .bar { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  }
</style>
