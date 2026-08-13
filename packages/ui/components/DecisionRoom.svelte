<script>
  // The decision room: one underlying shape shared by Strategy, Product,
  // and (for sign-offs) Talent. An open call, its options as things you
  // press, an optional on-record reason, what stays closed regardless, and
  // the record one tap away. The room never decides; it makes the human
  // moment serious and durable. One accent moment per screen lives here.
  let {
    eyebrow = "The call on the table",
    question = "",
    whyNow = "",
    options = [], // [{ token, title, line }]
    chosen = "",
    deciding = false,
    reason = $bindable(""),
    reasonPlaceholder = "One line on your thinking",
    result = null, // { ok, line, receiptId }
    staysClosed = [], // [{ label, line }] - truthful holds, shown beside the choice
    closedCue = "These stay closed no matter what is chosen - deciding records judgment, it does not turn keys.",
    onDecide = () => {}
  } = $props();
</script>

<section class="room" aria-label={eyebrow}>
  <p class="eyebrow">{eyebrow}</p>
  {#if question}
    <h2 class="question">{question}</h2>
  {/if}
  {#if whyNow}
    <p class="why">Why now: {whyNow}</p>
  {/if}

  {#if options.length > 0}
    <div class="options" role="group" aria-label="The options, each one a real choice">
      {#each options as option}
        <button
          class="option"
          class:chosen={chosen === option.token}
          disabled={deciding}
          onclick={() => onDecide(option.token)}
        >
          <strong>{option.title}</strong>
          {#if option.line}
            <span class="option-line">{option.line}</span>
          {/if}
        </button>
      {/each}
    </div>
    <label class="reason">
      <span>Why (optional, stays on the record)</span>
      <input type="text" maxlength="2000" bind:value={reason} placeholder={reasonPlaceholder} disabled={deciding} />
    </label>
  {/if}

  {#if result}
    <p class="result" data-ok={result.ok}>
      {result.line}
      {#if result.receiptId}
        <details class="quiet">
          <summary>the exact record</summary>
          <code>{result.receiptId}</code>
        </details>
      {/if}
    </p>
  {/if}

  {#if staysClosed.length > 0}
    <div class="holds">
      <h3>Stays closed either way</h3>
      <p class="holds-cue">{closedCue}</p>
      <ul>
        {#each staysClosed as hold}
          <li><strong>{hold.label}</strong>{#if hold.line}<span> — {hold.line}</span>{/if}</li>
        {/each}
      </ul>
    </div>
  {/if}
</section>

<style>
  .room {
    border-left: 3px solid var(--mdx-accent-primary);
    padding: 4px 0 4px 20px;
    margin: 28px 0;
  }
  .eyebrow {
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 0.68rem;
    color: var(--mdx-text-secondary);
    margin: 0 0 6px;
  }
  .question {
    font-size: 1.25rem;
    line-height: 1.35;
    margin: 0 0 6px;
    max-width: 46rem;
  }
  .why {
    color: var(--mdx-text-secondary);
    font-size: 0.9rem;
    margin: 0 0 16px;
    max-width: 46rem;
  }
  .options {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 12px;
    margin: 4px 0 14px;
    max-width: 52rem;
  }
  .option {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    text-align: left;
    padding: 14px 16px;
    border: 1px solid var(--mdx-border-default);
    border-radius: 10px;
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font: inherit;
    cursor: pointer;
  }
  .option:hover:not(:disabled) {
    border-color: var(--mdx-accent-primary);
  }
  .option.chosen {
    border-color: var(--mdx-accent-primary);
    box-shadow: inset 0 0 0 1px var(--mdx-accent-primary);
  }
  .option:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .option-line {
    font-size: 0.82rem;
    color: var(--mdx-text-secondary);
  }
  .reason {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 0.85rem;
    color: var(--mdx-text-secondary);
    max-width: 32rem;
    margin-bottom: 6px;
  }
  .reason input {
    padding: 10px 12px;
    border: 1px solid var(--mdx-border-default);
    border-radius: 8px;
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font: inherit;
  }
  .result {
    font-size: 0.9rem;
    margin: 10px 0 0;
  }
  .result[data-ok="false"] {
    color: var(--mdx-status-attention, #b4541e);
  }
  .holds {
    margin-top: 22px;
  }
  .holds h3 {
    font-size: 0.85rem;
    margin: 0 0 4px;
  }
  .holds-cue {
    font-size: 0.8rem;
    color: var(--mdx-text-secondary);
    margin: 0 0 8px;
    max-width: 46rem;
  }
  .holds ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 0.85rem;
  }
  .holds span {
    color: var(--mdx-text-secondary);
  }
  .quiet summary {
    cursor: pointer;
    opacity: 0.7;
  }
  .quiet code {
    font-size: 0.72rem;
  }
</style>
