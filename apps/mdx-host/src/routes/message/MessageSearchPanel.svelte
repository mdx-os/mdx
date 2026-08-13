<script>
  // Search across every room: pure presentation. The query runs and the
  // jump-to-result scroll live in the route, which owns the timeline.
  let { query = $bindable(""), searching = false, results = null, onSearch, onJump } = $props();
</script>

<div class="search-panel">
  <div class="search-row">
    <input
      type="search"
      placeholder="Search all channels..."
      bind:value={query}
      onkeydown={(event) => event.key === "Enter" && onSearch()}
      aria-label="Search messages"
    />
    <button type="button" class="search-go" onclick={onSearch} disabled={searching || !query.trim()}>
      {searching ? "Searching..." : "Search"}
    </button>
  </div>
  {#if results != null}
    {#if results.length === 0}
      <p class="search-empty">Nothing said that matches - yet.</p>
    {:else}
      <ul class="search-results">
        {#each results.slice(0, 12) as result (result.receiptId)}
          <li>
            <button type="button" class="search-hit" onclick={() => onJump(result)}>
              <span class="hit-channel">#{result.channelId}</span>
              <span class="hit-body">{result.body.slice(0, 120)}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</div>

<style>
  .search-panel {
    position: relative;
    z-index: 30;
    border-bottom: 1px solid var(--mdx-border-subtle);
    padding: 10px 16px;
    display: grid;
    gap: 8px;
    background: var(--mdx-surface-raised);
  }

  .search-row {
    display: flex;
    gap: 8px;
  }

  .search-row input {
    flex: 1;
    padding: 8px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base, transparent);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: var(--mdx-text-sm);
  }

  .search-go {
    padding: 8px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    cursor: pointer;
    font-size: var(--mdx-text-sm);
  }

  .search-empty {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-sm);
  }

  .search-results {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
    max-height: 260px;
    overflow-y: auto;
  }

  .search-hit {
    display: flex;
    gap: 10px;
    align-items: baseline;
    width: 100%;
    text-align: left;
    border: none;
    background: none;
    color: var(--mdx-text-primary);
    padding: 6px 8px;
    border-radius: var(--mdx-radius-md);
    cursor: pointer;
  }

  .search-hit:hover {
    background: var(--mdx-surface-base, rgba(127, 127, 127, 0.08));
  }

  .hit-channel {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    flex: none;
  }

  .hit-body {
    font-size: var(--mdx-text-sm);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
