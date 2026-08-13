<script>
  // A companion's offer to act: plain words first, the mechanics one tap
  // away. Pure presentation - the approval write and the decline record
  // stay in the route, which owns the session.
  let { proposal, companionName, channel, body, skillFact, onApprove, onDecline } = $props();
</script>

<div class="skill-card" data-state={proposal.state}>
  {#if proposal.state === "done"}
    <p class="skill-done">
      {proposal.tool === "record_decision" ? "Recorded" : "Posted"} in
      <a href="/message">#{channel}</a>.
    </p>
  {:else}
    <p class="skill-offer">
      {companionName} can
      {proposal.tool === "record_decision"
        ? "record this decision for the company"
        : "post this to the team"} - your call.
    </p>
    <p class="skill-preview">{body}</p>
    {#if proposal.state === "error"}
      <p class="send-error" role="alert">Not saved - try again.</p>
    {/if}
    <div class="skill-actions">
      <button
        type="button"
        class="skill-approve"
        onclick={onApprove}
        disabled={proposal.state === "sending"}
      >
        {proposal.state === "sending"
          ? "Saving..."
          : proposal.tool === "record_decision"
            ? "Record it"
            : "Post it"}
      </button>
      <button type="button" class="skill-decline" onclick={onDecline}>
        Not now
      </button>
    </div>
    <details class="skill-details">
      <summary>exactly what happens</summary>
      <span>
        Posts the text above to #{channel} as you, after you
        approve - the same as typing it yourself. Nothing is sent until you choose,
        and your decision is saved either way - the text itself only if you approve.
        {#if skillFact?.route}
          <span class="skill-mechanics">Under the hood: POST {skillFact.route}, saved as a {skillFact.receiptKind} record.</span>
        {/if}
      </span>
    </details>
  {/if}
</div>

<style>
  .skill-card {
    display: grid;
    gap: 8px;
    max-width: 560px;
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-default);
    border-left: 3px solid var(--mdx-accent-primary);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }

  .skill-offer {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 13px;
  }

  .skill-preview {
    margin: 0;
    color: var(--mdx-text-primary);
    font-size: 13.5px;
    line-height: 1.5;
    white-space: pre-wrap;
  }

  .skill-actions {
    display: flex;
    gap: 8px;
  }

  .skill-approve {
    padding: 7px 16px;
    border: none;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-accent-primary);
    color: #fff;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }

  .skill-approve:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .skill-decline {
    padding: 7px 14px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-pill);
    background: transparent;
    color: var(--mdx-text-secondary);
    font-size: 13px;
    cursor: pointer;
  }

  .skill-done {
    margin: 0;
    color: var(--mdx-accent-success);
    font-size: 13px;
  }

  .skill-done a {
    color: inherit;
  }

  .skill-details {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-tertiary);
  }

  .skill-details summary {
    cursor: pointer;
    color: var(--mdx-text-muted);
  }

  .skill-mechanics {
    display: block;
    margin-top: 4px;
    color: var(--mdx-text-muted);
    font-family: var(--mdx-font-mono, monospace);
    font-size: 11px;
  }

  .send-error {
    margin: 0;
    color: var(--mdx-accent-warning);
    font-size: var(--mdx-text-sm);
  }
</style>
