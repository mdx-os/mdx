<script>
  import { createMdxClient } from "@mdx/client";
  import { invalidateAll } from "$app/navigation";

  let { data } = $props();

  const activatedLessons = $derived(data.activatedLessons ?? []);
  const retiredLessonCount = $derived(data.retiredLessonCount ?? 0);
  const surfaceRecords = $derived(data.surfaceRecords ?? []);
  const pendingReviewCount = $derived(data.pendingReviewCount ?? 0);
  const modelProof = $derived(data.modelProof ?? []);
  const client = $derived(createMdxClient({ baseUrl: "/api/kernel", session: data.session }));
  const actor = $derived(data.session?.user_id ?? "local_user");

  // Retire a lesson that stopped holding. The same governed write the Learn
  // lane uses, offered here where the lesson is read.
  let retireOpenFor = $state("");
  let retireReason = $state("");
  let retireOwner = $state("");
  let retireBusy = $state("");
  let retireFlash = $state(null);

  function openRetire(lesson) {
    retireOpenFor = lesson.receiptId;
    retireReason = "";
    retireOwner = actor;
    retireFlash = null;
  }

  function closeRetire() {
    retireOpenFor = "";
    retireReason = "";
  }

  async function retireLesson(lesson) {
    if (retireBusy) return;
    const reason = retireReason.trim();
    if (!reason) {
      retireFlash = { ok: false, line: "Say why this lesson should stop guiding work." };
      return;
    }
    retireBusy = lesson.receiptId;
    retireFlash = null;
    try {
      const packet = await client.write(
        "/learning/memory-supersedes.json",
        {
          actor_id: actor,
          activation_receipt_id: lesson.receiptId,
          reason,
          review_owner: retireOwner.trim() || actor
        },
        { receiptIntent: "learning_memory_supersede" }
      );
      if (packet.status === "RECORDED") {
        retireFlash = {
          ok: true,
          line: "Retired. It stays in the record but no longer guides new work."
        };
        closeRetire();
        await invalidateAll();
      } else {
        retireFlash = { ok: false, line: packet.reason ?? "That did not record." };
      }
    } catch (error) {
      retireFlash = { ok: false, line: "Nothing was recorded. Try again." };
    }
    retireBusy = "";
  }

  function ratePct(rate) {
    return `${Math.round(rate * 100)}%`;
  }
</script>

<svelte:head><title>Memory - MDx</title></svelte:head>

{#snippet keptRecord(record)}
  <li class="record">
    <p class="record-text">{record.content || "Kept for the team."}</p>
    <p class="record-meta">
      {record.recordBasis === "ratified"
        ? `Cleared by ${record.ratifiedBy || "a reviewer"}.`
        : "Recorded by its governed memory gate."}
    </p>
    <details>
      <summary>view the record</summary>
      <ul class="source-list">
        {#if record.note}<li>Review note: {record.note}</li>{/if}
        {#if record.sourceReceiptId}<li><code>{record.sourceReceiptId}</code></li>{/if}
        {#if record.gateReceiptId}<li><code>{record.gateReceiptId}</code></li>{/if}
        <li><code>{record.receiptId}</code></li>
      </ul>
    </details>
  </li>
{/snippet}

<section class="memory" data-route-state="ready">
  <header class="mdx-page-head">
    <div>
      <h1>Memory</h1>
      <p class="mdx-page-sub">
        What MDx has learned from your work, what each surface wrote down, and what the runs proved
        about the models. Three honest views, not one - because that is how the work is kept today.
      </p>
    </div>
    <a class="primary-action" href="/learn">Review lessons</a>
  </header>

  <!-- Rail 1: the lessons a person kept, in the flywheel voice. -->
  <section class="rail" aria-label="What MDx has learned">
    <div class="rail-head">
      <h2>What MDx has learned</h2>
      <p>
        Lessons a person read, edited, and kept. Each one can guide the next plan - never on its own,
        and only until you retire it.
      </p>
    </div>
    {#if activatedLessons.length > 0}
      <ul class="lesson-list">
        {#each activatedLessons as lesson (lesson.receiptId)}
          <li class="lesson">
            <div class="lesson-main">
              <p class="lesson-text">{lesson.summary}</p>
              <div class="lesson-chips">
                {#if lesson.steersCasting}
                  <span class="chip steer">Steers who plans this work</span>
                {/if}
                {#if lesson.targetType}
                  <span class="chip quiet">{lesson.targetType.replaceAll("_", " ")}</span>
                {/if}
              </div>
            </div>
            {#if retireOpenFor === lesson.receiptId}
              <div class="retire-form">
                <label>
                  Why should this lesson stop guiding work?
                  <textarea
                    rows="2"
                    bind:value={retireReason}
                    placeholder="What turned out to be wrong or outdated about it"
                  ></textarea>
                </label>
                <label>
                  Who reviewed this call?
                  <input type="text" bind:value={retireOwner} />
                </label>
                <div class="action-row">
                  <button
                    type="button"
                    class="primary-decision"
                    onclick={() => retireLesson(lesson)}
                    disabled={Boolean(retireBusy)}
                  >
                    {retireBusy === lesson.receiptId ? "Recording" : "Retire it"}
                  </button>
                  <button type="button" onclick={closeRetire} disabled={Boolean(retireBusy)}>
                    Keep the lesson
                  </button>
                </div>
              </div>
            {:else}
              <div class="action-row">
                <button type="button" onclick={() => openRetire(lesson)} disabled={Boolean(retireBusy)}>
                  That's outdated
                </button>
              </div>
            {/if}
            <details>
              <summary>view the record</summary>
              <ul class="source-list">
                <li>{lesson.activationBasis}</li>
                <li>{lesson.rollbackPlan}</li>
                <li><code>{lesson.receiptId}</code></li>
                {#if lesson.promotionReceiptId}<li><code>{lesson.promotionReceiptId}</code></li>{/if}
                {#if lesson.judgmentReceiptId}<li><code>{lesson.judgmentReceiptId}</code></li>{/if}
              </ul>
            </details>
          </li>
        {/each}
      </ul>
      {#if retiredLessonCount > 0}
        <p class="rail-foot-note">
          {retiredLessonCount === 1
            ? "1 retired lesson stays in the record and no longer guides work."
            : `${retiredLessonCount} retired lessons stay in the record and no longer guide work.`}
        </p>
      {/if}
      {#if retireFlash}
        <p class:ok={retireFlash.ok} class:error={!retireFlash.ok} class="action-flash">{retireFlash.line}</p>
      {/if}
    {:else}
      <div class="rail-empty">
        <h3>No kept lessons yet</h3>
        <p>
          When a build finishes or feedback lands, the lesson it suggests waits for you in Learn. Read
          one, edit it in your own words, and keep it - then it shows up here.
        </p>
        <a class="rail-empty-link" href="/learn">Open Learn</a>
      </div>
    {/if}
  </section>

  <!-- Rail 2: what each surface wrote down. -->
  <section class="rail" aria-label="What each surface recorded">
    <div class="rail-head">
      <h2>What each surface recorded</h2>
      <p>
        Twin, Message, Pages, and Forge each write down what happened. Notes meant for the whole team
        wait for a second person before MDx recalls them.
      </p>
    </div>
    {#if surfaceRecords.length > 0}
      <div class="surface-groups">
        {#each surfaceRecords as group (group.surface)}
          {@const keptPreviewLimit = Math.max(0, 4 - group.waiting.length)}
          <div class="surface-group">
            <div class="surface-group-head">
              <h3>{group.surface}</h3>
              {#if group.waiting.length > 0}
                <span class="chip waiting">
                  {group.waiting.length === 1 ? "1 waiting on review" : `${group.waiting.length} waiting on review`}
                </span>
              {/if}
            </div>
            <ul class="record-list">
              {#each group.waiting as record (record.memoryId)}
                <li class="record">
                  <p class="record-text">{record.content}</p>
                  <p class="record-meta">Proposed by {record.proposedByLabel} - not used until you clear it.</p>
                  <details>
                    <summary>view the record</summary>
                    <ul class="source-list">
                      <li><code>{record.sourceReceiptId}</code></li>
                      <li><code>{record.gateReceiptId}</code></li>
                    </ul>
                  </details>
                </li>
              {/each}
              {#each group.kept.slice(0, keptPreviewLimit) as record (record.receiptId)}
                {@render keptRecord(record)}
              {/each}
            </ul>
            {#if group.kept.length > keptPreviewLimit}
              <details class="rail-foot-note">
                <summary>
                  Show {group.kept.length - keptPreviewLimit} more
                  {group.kept.length - keptPreviewLimit === 1 ? "record" : "records"}
                </summary>
                <ul class="record-list">
                  {#each group.kept.slice(keptPreviewLimit) as record (record.receiptId)}
                    {@render keptRecord(record)}
                  {/each}
                </ul>
              </details>
            {/if}
          </div>
        {/each}
      </div>
      {#if pendingReviewCount > 0}
        <p class="rail-foot-note">
          Clear or set aside shared notes in <a href="/learn">Learn</a>.
        </p>
      {/if}
    {:else}
      <div class="rail-empty">
        <h3>Nothing recorded yet</h3>
        <p>
          As you publish a page, send a team message, or run a build, MDx writes down what happened.
          Shared notes will appear here first, waiting for a second person to clear them.
        </p>
      </div>
    {/if}
  </section>

  <!-- Rail 3: what the runs proved about the models. -->
  <section class="rail" aria-label="What the work proved about models">
    <div class="rail-head">
      <h2>What the work proved about the models</h2>
      <p>
        When a build finishes, its result folds into a record of how each model does on each kind of
        change. This is the basis the planner uses to pick who works next.
      </p>
    </div>
    {#if modelProof.length > 0}
      <div class="proof-table" role="table" aria-label="Model results by kind of work">
        <div class="proof-row proof-head" role="row">
          <span role="columnheader">Model</span>
          <span role="columnheader">Kind of work</span>
          <span role="columnheader">Finished</span>
          <span role="columnheader">Runs</span>
        </div>
        {#each modelProof as row (row.model + row.workType)}
          <div class="proof-row" role="row">
            <span class="proof-model" role="cell">{row.model}</span>
            <span role="cell">{row.workType.replaceAll("_", " ")}</span>
            <span class="proof-rate" role="cell">{ratePct(row.doneRate)}</span>
            <span class="proof-runs" role="cell">
              {row.runs}{row.runs < 5 ? " - still thin" : ""}
            </span>
          </div>
        {/each}
      </div>
      <p class="rail-foot-note">
        This is the model scorecard, not a promoted lesson. It updates on its own as runs finish.
      </p>
    {:else}
      <div class="rail-empty">
        <h3>No runs to learn from yet</h3>
        <p>
          Once a few builds finish, their results show up here as a per-model, per-work-type record -
          the same read the planner uses to cast the next run.
        </p>
        <a class="rail-empty-link" href="/forge">Open Forge</a>
      </div>
    {/if}
  </section>

  <section class="see-it" aria-label="Where memory shows up">
    <p>
      Twin draws on what it remembers when you ask it something - you'll see it cite a lesson on your
      next turn. The full lane to keep, edit, and retire lessons lives in Learn.
    </p>
    <div class="see-it-links">
      <a href="/twin">Ask Twin</a>
      <a href="/learn">Open Learn</a>
    </div>
  </section>
</section>

<style>
  .memory {
    display: flex;
    flex-direction: column;
    gap: 18px;
    max-width: 900px;
  }
  .mdx-page-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }
  .mdx-page-head h1 {
    margin: 0 0 6px;
    font-size: 22px;
    font-weight: 650;
    color: var(--mdx-text-primary);
  }
  .mdx-page-sub {
    margin: 0;
    max-width: 62ch;
    font-size: 13.5px;
    line-height: 1.55;
    color: var(--mdx-text-secondary);
  }
  .primary-action {
    flex: none;
    padding: 9px 16px;
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-accent-primary);
    color: var(--mdx-text-on-accent, white);
    text-decoration: none;
    font-size: 13px;
    font-weight: 600;
  }
  .rail {
    padding: 18px 20px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-shadow-card);
  }
  .rail-head h2 {
    margin: 0 0 4px;
    font-size: 16px;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }
  .rail-head p {
    margin: 0;
    max-width: 66ch;
    font-size: 13px;
    line-height: 1.55;
    color: var(--mdx-text-secondary);
  }
  .lesson-list,
  .record-list {
    list-style: none;
    margin: 16px 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .lesson {
    padding: 12px 14px;
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .lesson-text {
    margin: 0;
    font-size: 13.5px;
    line-height: 1.5;
    color: var(--mdx-text-primary);
  }
  .lesson-chips,
  .surface-group-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .chip {
    font-size: 11.5px;
    padding: 3px 10px;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-secondary);
  }
  .chip.quiet {
    color: var(--mdx-text-tertiary);
  }
  .chip.steer {
    color: var(--mdx-accent-primary);
    background: color-mix(in srgb, var(--mdx-accent-primary) 10%, transparent);
  }
  .chip.waiting {
    color: var(--mdx-warning-amber, #d9883a);
  }
  .action-row {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .action-row button {
    font: inherit;
    font-size: 12.5px;
    padding: 7px 12px;
    border-radius: var(--mdx-radius-md);
    border: 1px solid var(--mdx-border-subtle);
    background: transparent;
    color: var(--mdx-text-secondary);
    cursor: pointer;
  }
  .action-row button:hover {
    color: var(--mdx-text-primary);
    border-color: var(--mdx-border-default);
  }
  .action-row button.primary-decision {
    border-color: transparent;
    background: var(--mdx-accent-primary);
    color: var(--mdx-text-on-accent, white);
  }
  .retire-form {
    display: grid;
    gap: 8px;
  }
  .retire-form label {
    display: grid;
    gap: 4px;
    font-size: 12px;
    color: var(--mdx-text-secondary);
  }
  .retire-form textarea,
  .retire-form input {
    font: inherit;
    font-size: 13px;
    padding: 7px 9px;
    border-radius: var(--mdx-radius-sm);
    border: 1px solid var(--mdx-border-subtle);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
  }
  details {
    font-size: 12px;
    color: var(--mdx-text-muted);
  }
  summary {
    cursor: pointer;
    color: var(--mdx-text-tertiary);
  }
  .source-list {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    display: grid;
    gap: 4px;
    font-size: 11.5px;
    color: var(--mdx-text-muted);
  }
  .source-list code {
    font-size: 11px;
    word-break: break-all;
  }
  .surface-groups {
    display: grid;
    gap: 16px;
    margin-top: 16px;
  }
  .surface-group-head h3 {
    margin: 0;
    font-size: 13.5px;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }
  .record {
    padding: 10px 12px;
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    display: grid;
    gap: 4px;
  }
  .record-text {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: var(--mdx-text-primary);
  }
  .record-meta {
    margin: 0;
    font-size: 11.5px;
    color: var(--mdx-text-tertiary);
  }
  .proof-table {
    margin-top: 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    overflow: hidden;
  }
  .proof-row {
    display: grid;
    grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr) 90px 110px;
    gap: 10px;
    padding: 9px 14px;
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
    border-top: 1px solid var(--mdx-border-subtle);
  }
  .proof-row:first-child {
    border-top: none;
  }
  .proof-head {
    background: var(--mdx-surface-base);
    color: var(--mdx-text-tertiary);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-weight: 650;
  }
  .proof-model {
    color: var(--mdx-text-primary);
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .proof-rate {
    color: var(--mdx-text-primary);
    font-weight: 600;
  }
  .proof-runs {
    color: var(--mdx-text-muted);
  }
  .rail-foot-note {
    margin: 12px 0 0;
    font-size: 11.5px;
    color: var(--mdx-text-tertiary);
  }
  .rail-foot-note a {
    color: var(--mdx-accent-primary);
    text-decoration: none;
  }
  .rail-empty {
    margin-top: 8px;
    display: grid;
    gap: 8px;
    padding: 16px;
    border: 1px dashed var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }
  .rail-empty h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }
  .rail-empty p {
    margin: 0;
    max-width: 60ch;
    font-size: 13px;
    line-height: 1.55;
    color: var(--mdx-text-secondary);
  }
  .rail-empty-link {
    font-size: 12.5px;
    color: var(--mdx-accent-primary);
    text-decoration: none;
  }
  .action-flash {
    margin: 10px 0 0;
    font-size: 12.5px;
  }
  .action-flash.ok {
    color: var(--mdx-accent-success, var(--mdx-success-green));
  }
  .action-flash.error {
    color: var(--mdx-accent-error, #d66);
  }
  .see-it {
    padding: 16px 20px;
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-sunken);
    display: grid;
    gap: 10px;
  }
  .see-it p {
    margin: 0;
    max-width: 66ch;
    font-size: 13px;
    line-height: 1.55;
    color: var(--mdx-text-secondary);
  }
  .see-it-links {
    display: flex;
    gap: 14px;
  }
  .see-it-links a {
    font-size: 13px;
    font-weight: 600;
    color: var(--mdx-accent-primary);
    text-decoration: none;
  }
  @media (max-width: 640px) {
    .mdx-page-head {
      flex-direction: column;
    }
    .proof-row {
      grid-template-columns: minmax(0, 1.4fr) minmax(0, 1fr) 70px;
    }
    .proof-row span:nth-child(4) {
      display: none;
    }
  }
</style>
