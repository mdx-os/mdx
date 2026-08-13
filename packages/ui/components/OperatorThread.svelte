<script>
  import { humanToken } from "../humanCopy.js";

  export let items = [];
</script>

<div class="operator-thread" data-ui-operator-thread>
  {#each items as item}
    <article class="thread-item">
      <span class="thread-node" aria-hidden="true"></span>
      <div class="thread-body">
        <div class="thread-head">
          <span class="thread-actor">{humanToken(item.speaker)}</span>
          <span class="thread-tag">{humanToken(item.kind)}</span>
          {#if item.sequence != null}
            <span class="thread-seq">step {item.sequence}</span>
          {/if}
        </div>
        <p class="thread-summary">{item.summary}</p>
        {#if item.lane}
          <small class="thread-lane">{humanToken(item.lane)}</small>
        {/if}
      </div>
    </article>
  {/each}
</div>

<style>
  .operator-thread {
    display: grid;
    gap: 0;
  }

  .thread-item {
    position: relative;
    display: grid;
    grid-template-columns: 14px minmax(0, 1fr);
    gap: 14px;
    padding: 0 0 16px;
  }

  .thread-item::before {
    content: "";
    position: absolute;
    left: 6px;
    top: 14px;
    bottom: -2px;
    width: 2px;
    background: var(--mdx-border-default);
  }

  .thread-item:last-child::before {
    display: none;
  }

  .thread-node {
    position: relative;
    z-index: 1;
    width: 12px;
    height: 12px;
    margin-top: 4px;
    border: 2px solid var(--mdx-app-accent-forge);
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface);
  }

  .thread-body {
    min-width: 0;
    border: 1px solid var(--mdx-border-soft);
    border-radius: var(--mdx-radius-panel);
    padding: 12px 14px;
    background: rgba(var(--mdx-surface-rgb), 0.6);
  }

  .thread-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 5px;
  }

  .thread-actor {
    color: var(--mdx-text-primary);
    font-size: 13px;
    font-weight: 700;
  }

  .thread-tag {
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-pill);
    padding: 1px 8px;
    color: var(--mdx-text-secondary);
    font-family: var(--mdx-font-mono);
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .thread-seq {
    color: var(--mdx-text-muted);
    font-family: var(--mdx-font-mono);
    font-size: 11px;
  }

  .thread-summary {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.5;
  }

  .thread-lane {
    display: block;
    margin-top: 6px;
    color: var(--mdx-text-muted);
    font-size: 11px;
  }
</style>
