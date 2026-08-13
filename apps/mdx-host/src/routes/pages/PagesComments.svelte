<script>
  // Comments on a page: pure presentation. The writes (post, resolve,
  // promote to decision) stay in the route - they ride the Message
  // contract and the route owns that wiring.
  import Provenance from "@mdx/ui/components/Provenance.svelte";

  let {
    comments = [],
    draft = $bindable(""),
    busy = false,
    note = "",
    onPost,
    onResolve,
    onDecision
  } = $props();
</script>

<div class="comments" aria-label="Comments">
  <p class="world-title">Comments</p>
  {#each comments as comment (comment.receiptId)}
    <div class="comment" class:resolved={comment.resolved}>
      <p class="comment-body">{comment.body}</p>
      <p class="comment-meta">
        {comment.actor}
        <Provenance label="Details" items={[{ label: "This comment", value: comment.receiptId }]} />
        {#if comment.resolved}
          <span class="comment-resolved">resolved</span>
        {:else}
          <button type="button" class="comment-act" onclick={() => onResolve(comment)}>resolve</button>
          <button type="button" class="comment-act" onclick={() => onDecision(comment)}>record as decision</button>
        {/if}
      </p>
    </div>
  {/each}
  <div class="comment-compose">
    <input
      type="text"
      placeholder="Leave a comment"
      bind:value={draft}
      onkeydown={(event) => event.key === "Enter" && onPost()}
      aria-label="Write a comment"
    />
    <button type="button" onclick={onPost} disabled={busy || !draft.trim()}>Comment</button>
  </div>
  {#if note}<p class="comment-note">{note}</p>{/if}
</div>

<style>
  .comments {
    margin-top: 10px;
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    display: grid;
    gap: 8px;
  }

  .world-title {
    margin: 0;
    font-size: 13.5px;
    font-weight: 600;
  }

  .comment {
    display: grid;
    gap: 2px;
  }

  .comment.resolved {
    opacity: 0.55;
  }

  .comment-body {
    margin: 0;
    font-size: var(--mdx-text-sm);
    line-height: 1.5;
  }

  .comment.resolved .comment-body {
    text-decoration: line-through;
  }

  .comment-meta {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    display: flex;
    gap: 8px;
    align-items: baseline;
    flex-wrap: wrap;
  }

  .comment-resolved {
    color: var(--mdx-accent-success, #2f9e6e);
  }

  .comment-act {
    border: none;
    background: none;
    color: var(--mdx-text-muted);
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
    font-size: var(--mdx-text-xs);
  }

  .comment-compose {
    display: flex;
    gap: 8px;
  }

  .comment-compose input {
    flex: 1;
    padding: 8px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: var(--mdx-text-sm);
  }

  .comment-compose button {
    padding: 8px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    cursor: pointer;
    font-size: var(--mdx-text-sm);
  }

  .comment-note {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }
</style>
