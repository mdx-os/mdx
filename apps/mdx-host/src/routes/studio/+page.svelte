<script>
  // Studio: hand it a goal, and it opens a governed run you can steer before it
  // lands. One primary verb starts the run (Start the run); from there each run
  // shows only the governed controls the kernel allows from its current state -
  // pause or land while it is drafting, add steering or resume while it is
  // paused. State and the allowed controls come from the projection, derived
  // from receipts, so the surface never claims a move the kernel would refuse.
  // Landing still reuses the Pages publication, so a document is one body of
  // truth. Steering and the record stay in quiet disclosure.
  import { createMdxClient } from "@mdx/client";
  import { invalidateAll } from "$app/navigation";

  let { data } = $props();

  const client = $derived(createMdxClient({ baseUrl: "/api/kernel", session: data.session }));
  const actor = $derived(data.session?.user_id ?? "local_user");
  const runs = $derived(data.runs ?? []);
  const sources = $derived(data.sources ?? []);

  // The work kinds Studio can make. Code is deliberately not here: code work
  // opens in Forge. Each kind lands as one typed Pages object.
  const KINDS = [
    { id: "document", label: "Document", hint: "Write or paste the document. Studio lands exactly this, after any steering." },
    { id: "decision", label: "Decision", hint: "State the decision: the context, the options, what you chose, and why." },
    { id: "sheet", label: "Sheet", hint: "Paste the rows (CSV or tab-separated). Studio lands the sheet as a Page." },
    { id: "dataset", label: "Dataset", hint: "Paste the dataset: the columns and the rows it holds." },
    { id: "deck", label: "Deck", hint: "One slide per section. Lead each with its headline, then the points." }
  ];

  let goal = $state("");
  let kind = $state("document");
  let sourceId = $state("");
  let busy = $state(false);
  let flash = $state(null);

  const draftHint = $derived(
    (KINDS.find((k) => k.id === kind) ?? KINDS[0]).hint
  );

  // Per-run surface state, all keyed by run_id: the draft you bring at start so
  // landing has a title and body, which inline panel is open, and the steering
  // instruction being written. None of this is system truth; the receipts are.
  let drafts = $state({});
  let openRecord = $state("");
  let landOpen = $state("");
  let steerOpen = $state("");
  let steerText = $state("");

  let title = $state("");
  let draft = $state("");

  const ready = $derived(goal.trim().length > 0);

  function stamp() {
    return Date.now().toString(36);
  }

  function slug(text, fallback) {
    const cleaned = text
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "_")
      .replace(/^_+|_+$/g, "");
    return cleaned || fallback;
  }

  function stateLabel(state) {
    if (state === "drafting") return "Drafting";
    if (state === "paused") return "Paused";
    if (state === "landed") return "Landed";
    return "Open";
  }

  function sourceLabel(sourceKind) {
    if (sourceKind === "pages_object") return "Page";
    if (sourceKind === "connector_item") return "External source";
    return "Local grounding";
  }

  function kindLabel(artifactKind) {
    return (KINDS.find((k) => k.id === artifactKind) ?? {}).label ?? "Document";
  }

  function allows(run, control) {
    return Array.isArray(run.allowed_controls) && run.allowed_controls.includes(control);
  }

  async function post(path, body, intent) {
    return client.write(path, { ...body, actor_id: actor }, { receiptIntent: intent });
  }

  // Start a held run. We open with the goal; the title and draft you bring are
  // kept on the surface and authored into the document at land time.
  async function start() {
    if (busy || !ready) return;
    busy = true;
    flash = null;
    const runId = `studio_run_${stamp()}`;
    const chosen = sources.find((s) => s.receipt_id === sourceId);
    const grounding = chosen
      ? { source_receipt_id: chosen.receipt_id, source_kind: chosen.kind, source_title: chosen.title }
      : {};
    try {
      const packet = await post(
        `/studio/runs/${runId}/open.json`,
        { goal: goal.trim(), artifact_kind: kind, ...grounding },
        "studio_run_open"
      );
      if (packet?.status === "STUDIO_RUN_DRAFTING") {
        drafts = { ...drafts, [runId]: { title: title.trim(), draft: draft.trim() } };
        flash = { ok: true, line: "Run started. Steer it if you need to, then land it." };
        goal = "";
        title = "";
        draft = "";
        sourceId = "";
        kind = "document";
        await invalidateAll();
      } else {
        flash = { ok: false, line: packet?.refusal_reason ?? packet?.reason ?? "That did not start. Try again." };
      }
    } catch (error) {
      flash = { ok: false, line: "Nothing was recorded - that did not start. Try again." };
    }
    busy = false;
  }

  async function control(run, leaf, status, intent, line) {
    if (busy) return;
    busy = true;
    flash = null;
    try {
      const packet = await post(
        `/studio/runs/${run.run_id}/${leaf}.json`,
        { idempotency_key: `${leaf}_${run.run_id}_${stamp()}` },
        intent
      );
      if (packet?.status === status && packet?.refused !== true) {
        flash = { ok: true, line };
        await invalidateAll();
      } else {
        flash = { ok: false, line: packet?.refusal_reason ?? "That move was refused. The record says why." };
      }
    } catch (error) {
      flash = { ok: false, line: "Nothing was recorded - try again." };
    }
    busy = false;
  }

  function pause(run) {
    return control(run, "pause", "STUDIO_RUN_PAUSED", "studio_run_pause", "Paused. Add steering, then resume.");
  }

  function resume(run) {
    return control(run, "resume", "STUDIO_RUN_RESUMED", "studio_run_resume", "Back to drafting.");
  }

  async function steer(run) {
    if (busy || steerText.trim().length === 0) return;
    busy = true;
    flash = null;
    try {
      const packet = await post(
        `/studio/runs/${run.run_id}/steering.json`,
        {
          idempotency_key: `steering_${run.run_id}_${stamp()}`,
          instruction: steerText.trim(),
          target_scope: "document",
          reason: "operator steering"
        },
        "studio_run_steering"
      );
      if (packet?.status === "STUDIO_STEERING_RECORDED" && packet?.refused !== true) {
        flash = { ok: true, line: "Steering is on the record. Resume when you are ready." };
        steerText = "";
        steerOpen = "";
        await invalidateAll();
      } else {
        flash = { ok: false, line: packet?.refusal_reason ?? "Steering was refused. The record says why." };
      }
    } catch (error) {
      flash = { ok: false, line: "Nothing was recorded - try again." };
    }
    busy = false;
  }

  async function land(run) {
    if (busy) return;
    const brought = drafts[run.run_id] ?? { title: "", draft: "" };
    const landTitle = (brought.title || "").trim();
    const landDraft = (brought.draft || "").trim();
    if (landTitle.length === 0 || landDraft.length === 0) return;
    busy = true;
    flash = null;
    const rev = `rev_${stamp()}`;
    try {
      const packet = await post(
        `/studio/runs/${run.run_id}/land.json`,
        {
          idempotency_key: `land_${run.run_id}`,
          document_id: `${slug(landTitle, "studio_document")}_${stamp()}`,
          title: landTitle,
          body_text: landDraft,
          revision_id: rev
        },
        "studio_run_land"
      );
      if (packet?.status === "STUDIO_DOCUMENT_RUN_LANDED" && packet?.refused !== true) {
        flash = { ok: true, line: "Landed. It is a Page now, with the whole run on the record." };
        landOpen = "";
        await invalidateAll();
      } else {
        flash = { ok: false, line: packet?.refusal_reason ?? "That did not land. Try again." };
      }
    } catch (error) {
      flash = { ok: false, line: "Nothing was recorded - that did not land. Try again." };
    }
    busy = false;
  }

  function bind(run, field, value) {
    const current = drafts[run.run_id] ?? { title: "", draft: "" };
    drafts = { ...drafts, [run.run_id]: { ...current, [field]: value } };
  }

  // Signal that you are on a run. Presence is who is working it, derived from
  // the run's receipts; this adds you without having to act on the run yet.
  async function joinRun(run) {
    if (busy) return;
    busy = true;
    flash = null;
    try {
      const packet = await post(
        `/studio/runs/${run.run_id}/presence.json`,
        { presence_scope: "on the run" },
        "studio_run_presence"
      );
      if (packet?.status === "STUDIO_PRESENCE_MARKED") {
        flash = { ok: true, line: "You are on this run now." };
        await invalidateAll();
      } else {
        flash = { ok: false, line: packet?.reason ?? "That did not record. Try again." };
      }
    } catch (error) {
      flash = { ok: false, line: "Nothing was recorded - try again." };
    }
    busy = false;
  }

  function actorName(actorId) {
    // Show a human-readable name: drop the "human:" / "agent:" prefix.
    const cut = actorId.indexOf(":");
    return cut >= 0 ? actorId.slice(cut + 1) : actorId;
  }

  function presenceSummary(run) {
    const present = Array.isArray(run.present) ? run.present : [];
    if (present.length === 0) return "";
    const active = present.filter((p) => p.active).map((p) => actorName(p.actor_id));
    const around = present.length - active.length;
    const lead = active.length ? active.join(", ") : actorName(present[0].actor_id);
    if (around > 0) return `${lead} active now, ${around} more around`;
    return `${lead} active now`;
  }
</script>

<section class="studio">
  <header>
    <h1>Studio</h1>
    <p class="cue">
      Hand Studio a goal and a draft. It opens a governed run you can pause, steer, and resume before
      it lands the document as a Page you can stand behind.
    </p>
  </header>

  <form class="composer" onsubmit={(event) => { event.preventDefault(); start(); }}>
    <label>
      <span>What are you making?</span>
      <input type="text" bind:value={goal} placeholder="A short brief for the Q3 board" />
    </label>
    <label>
      <span>Kind</span>
      <select bind:value={kind}>
        {#each KINDS as option}
          <option value={option.id}>{option.label}</option>
        {/each}
      </select>
    </label>
    <label>
      <span>Title</span>
      <input type="text" bind:value={title} placeholder="Q3 board memo" />
    </label>
    <label>
      <span>Draft</span>
      <textarea bind:value={draft} rows="6" placeholder={draftHint}></textarea>
    </label>
    {#if sources.length}
      <label>
        <span>Ground it in</span>
        <select bind:value={sourceId}>
          <option value="">Nothing yet, just my draft</option>
          {#each sources as source}
            <option value={source.receipt_id}>
              {sourceLabel(source.kind)}: {source.title}{source.sensitivity === "sensitive" ? " (sensitive)" : ""}
            </option>
          {/each}
        </select>
      </label>
    {/if}
    <div class="composer-foot">
      <button type="submit" class="primary" disabled={!ready || busy}>
        {busy ? "Starting..." : "Start the run"}
      </button>
      {#if flash}<span class="flash" class:bad={!flash.ok}>{flash.line}</span>{/if}
    </div>
  </form>

  <div class="runs">
    {#if runs.length === 0}
      <p class="empty">
        No runs yet. Give Studio a goal and a draft, and it opens a run you can steer - then lands a
        document you can put in front of anyone, with the full trail behind it.
      </p>
    {:else}
      <h2>Recent runs</h2>
      <ul>
        {#each runs as run}
          <li class="run">
            <div class="run-main">
              <strong>{run.title || run.goal || run.run_id}</strong>
              <span class="tags">
                <span class="kind-chip">{kindLabel(run.artifact_kind)}</span>
                <span class="badge" data-state={run.state}>{stateLabel(run.state)}</span>
              </span>
            </div>

            {#if presenceSummary(run)}
              <p class="presence">{presenceSummary(run)}</p>
            {/if}

            <div class="controls">
              {#if allows(run, "pause")}
                <button type="button" class="ctrl" onclick={() => pause(run)} disabled={busy}>Pause</button>
              {/if}
              {#if allows(run, "land")}
                <button
                  type="button"
                  class="ctrl primary-ctrl"
                  onclick={() => (landOpen = landOpen === run.run_id ? "" : run.run_id)}
                  disabled={busy}
                >Land it</button>
              {/if}
              {#if allows(run, "steering")}
                <button
                  type="button"
                  class="ctrl"
                  onclick={() => { steerOpen = steerOpen === run.run_id ? "" : run.run_id; steerText = ""; }}
                  disabled={busy}
                >Add steering</button>
              {/if}
              {#if allows(run, "resume")}
                <button type="button" class="ctrl primary-ctrl" onclick={() => resume(run)} disabled={busy}>Resume</button>
              {/if}
              {#if run.state !== "landed" && run.state !== "unknown"}
                <button type="button" class="ctrl" onclick={() => joinRun(run)} disabled={busy}>I'm on this run</button>
              {/if}
            </div>

            {#if landOpen === run.run_id && allows(run, "land")}
              <div class="panel">
                <label>
                  <span>Title</span>
                  <input
                    type="text"
                    value={(drafts[run.run_id] ?? {}).title ?? ""}
                    oninput={(event) => bind(run, "title", event.currentTarget.value)}
                    placeholder="Q3 board memo"
                  />
                </label>
                <label>
                  <span>Draft</span>
                  <textarea
                    rows="5"
                    value={(drafts[run.run_id] ?? {}).draft ?? ""}
                    oninput={(event) => bind(run, "draft", event.currentTarget.value)}
                    placeholder="Studio lands exactly this."
                  ></textarea>
                </label>
                <button type="button" class="ctrl primary-ctrl" onclick={() => land(run)} disabled={busy}>
                  Land this document
                </button>
              </div>
            {/if}

            {#if steerOpen === run.run_id && allows(run, "steering")}
              <div class="panel">
                <label>
                  <span>What should change?</span>
                  <textarea
                    rows="3"
                    bind:value={steerText}
                    placeholder="Tighten the risks section and cite the Q2 outcome."
                  ></textarea>
                </label>
                <button type="button" class="ctrl primary-ctrl" onclick={() => steer(run)} disabled={busy || steerText.trim().length === 0}>
                  Record steering
                </button>
              </div>
            {/if}

            <button
              type="button"
              class="record-toggle"
              onclick={() => (openRecord = openRecord === run.run_id ? "" : run.run_id)}
            >
              {openRecord === run.run_id ? "Hide the record" : "View the record"}
            </button>
            {#if openRecord === run.run_id}
              <dl class="record">
                <div><dt>Goal</dt><dd>{run.goal}</dd></div>
                <div><dt>Kind</dt><dd>{kindLabel(run.artifact_kind)}</dd></div>
                <div><dt>State</dt><dd>{stateLabel(run.state)}</dd></div>
                {#if run.source_title}
                  <div><dt>Grounded in</dt><dd>{sourceLabel(run.source_kind)}: {run.source_title}</dd></div>
                {/if}
                {#if run.steering_receipt_ids?.length}
                  <div><dt>Steering</dt><dd>{run.steering_receipt_ids.length} on the record</dd></div>
                {/if}
                {#if run.document_id}
                  <div><dt>Document</dt><dd>{run.document_id}</dd></div>
                {/if}
                {#if run.publication_receipt_id}
                  <div><dt>Also in Pages</dt><dd>{run.publication_receipt_id}</dd></div>
                {/if}
              </dl>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <p class="boundary">{data.boundary}</p>
</section>

<style>
  .studio {
    max-width: 56rem;
    margin: 0 auto;
    padding: 2rem 1.5rem 4rem;
    display: flex;
    flex-direction: column;
    gap: 1.75rem;
  }
  header {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  h1 {
    font-family: var(--mdx-font-display);
    color: var(--mdx-text-primary);
    margin: 0;
  }
  .cue {
    color: var(--mdx-text-secondary);
    margin: 0;
    max-width: 42rem;
  }
  .composer {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1.5rem;
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg);
    box-shadow: var(--mdx-shadow-card);
  }
  .composer label,
  .panel label {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }
  .composer span,
  .panel span {
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-secondary);
  }
  .composer input,
  .composer textarea,
  .composer select,
  .panel input,
  .panel textarea {
    width: 100%;
    box-sizing: border-box;
    padding: 0.6rem 0.75rem;
    background: var(--mdx-surface-base);
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    color: var(--mdx-text-primary);
    font: inherit;
    resize: vertical;
  }
  .composer input:focus,
  .composer textarea:focus,
  .panel input:focus,
  .panel textarea:focus {
    outline: none;
    border-color: var(--mdx-accent-primary);
  }
  .composer-foot {
    display: flex;
    align-items: center;
    gap: 1rem;
    flex-wrap: wrap;
  }
  .primary {
    padding: 0.6rem 1.25rem;
    background: var(--mdx-accent-primary);
    color: var(--mdx-surface-base);
    border: none;
    border-radius: var(--mdx-radius-pill);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .flash {
    font-size: var(--mdx-text-sm);
    color: var(--mdx-tone-ok-text);
  }
  .flash.bad {
    color: var(--mdx-tone-danger-text);
  }
  .runs h2 {
    font-size: var(--mdx-text-sm);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--mdx-text-tertiary);
    margin: 0 0 0.75rem;
  }
  .empty {
    color: var(--mdx-text-secondary);
    padding: 1.5rem;
    background: var(--mdx-surface-sunken);
    border: 1px dashed var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg);
    max-width: 42rem;
  }
  .runs ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .run {
    padding: 1rem 1.25rem;
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .run-main {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 1rem;
  }
  .run-main strong {
    color: var(--mdx-text-primary);
  }
  .tags {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    flex-shrink: 0;
  }
  .kind-chip {
    font-size: var(--mdx-text-xs);
    padding: 0.15rem 0.55rem;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-base);
    border: 1px solid var(--mdx-border-subtle);
    color: var(--mdx-text-tertiary);
    white-space: nowrap;
  }
  .badge {
    font-size: var(--mdx-text-xs);
    padding: 0.15rem 0.55rem;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-secondary);
    white-space: nowrap;
  }
  .badge[data-state="paused"] {
    color: var(--mdx-tone-warn-text, var(--mdx-text-secondary));
  }
  .badge[data-state="landed"] {
    color: var(--mdx-tone-ok-text);
  }
  .presence {
    margin: 0;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-tertiary);
  }
  .controls {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .ctrl {
    padding: 0.4rem 0.9rem;
    background: var(--mdx-surface-base);
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-pill);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: var(--mdx-text-sm);
    cursor: pointer;
  }
  .ctrl:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .primary-ctrl {
    background: var(--mdx-accent-primary);
    color: var(--mdx-surface-base);
    border-color: transparent;
    font-weight: 600;
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 0.9rem;
    background: var(--mdx-surface-sunken);
    border-radius: var(--mdx-radius-md);
  }
  .panel button {
    align-self: flex-start;
  }
  .record-toggle {
    align-self: flex-start;
    background: none;
    border: none;
    padding: 0;
    color: var(--mdx-accent-primary);
    font: inherit;
    font-size: var(--mdx-text-sm);
    cursor: pointer;
  }
  .record {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin: 0.25rem 0 0;
    padding: 0.75rem;
    background: var(--mdx-surface-sunken);
    border-radius: var(--mdx-radius-sm);
  }
  .record div {
    display: flex;
    gap: 0.75rem;
  }
  .record dt {
    flex: 0 0 8rem;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-tertiary);
  }
  .record dd {
    margin: 0;
    font-family: var(--mdx-font-mono);
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-secondary);
    word-break: break-all;
  }
  .boundary {
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-tertiary);
    margin: 0;
    padding-top: 0.5rem;
    border-top: 1px solid var(--mdx-border-subtle);
  }
</style>
