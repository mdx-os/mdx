<script>
  // Needs you: one page for everything waiting on a person, grouped by
  // why it waits (the Superhuman lesson: split the inbox by reason, not
  // by age). Every row is a record; clicking it opens the door where
  // acting happens. Nothing is actioned from this list - judgment lives
  // behind each door, this page just makes sure nothing waits unseen.
  import { normalizeNeedsYou, readyBetRows, groupNeedsYou, totalWaiting } from "../../lib/needsYou.js";
  import { pipelineFrom } from "../../lib/productPipeline.js";

  let { data } = $props();

  const items = $derived([
    ...readyBetRows(pipelineFrom(data.pipeline)),
    ...normalizeNeedsYou(data.needsYou)
  ]);
  const groups = $derived(groupNeedsYou(items));
  const total = $derived(totalWaiting(items));
</script>

<svelte:head><title>Needs you - MDx</title></svelte:head>

<section class="needs-you" data-route-state="ready">
  <header class="head">
    <div class="mdx-page-head">
      <a class="mdx-crumb" href="/">Home</a>
      <h1>Needs you</h1>
      <p class="mdx-page-sub">
        {total === 0
          ? "Nothing waits on you right now."
          : `${total} ${total === 1 ? "thing waits" : "things wait"} on you — each row opens where you act on it.`}
      </p>
    </div>
  </header>

  {#if total === 0}
    <div class="empty">
      <p>The company keeps moving on its own — anything that needs a person lands here the moment a record asks for one.</p>
    </div>
  {:else}
    {#each groups as group (group.key)}
      <div class="group">
        <div class="group-head">
          <h2>{group.title}</h2>
          <span class="group-count">{group.items.length}</span>
        </div>
        <p class="group-line">{group.line}</p>
        <ul class="rows">
          {#each group.items as item (item.reason + item.ref)}
            <li>
              <a class="row" href={item.route}>
                <span class="row-detail">{item.detail || item.ref}</span>
                <span class="row-ref">{item.ref}</span>
                <span class="row-go" aria-hidden="true">→</span>
              </a>
            </li>
          {/each}
        </ul>
      </div>
    {/each}
  {/if}

  <footer class="quiet-line">
    Everything here is waiting on a person — open one to act on it where it lives.
    <details class="quiet-more">
      <summary>more</summary>
      <span>Boundary: {data.boundary}. Safe next move: {data.safeNext}.</span>
    </details>
  </footer>
</section>

<style>
  .needs-you { display: grid; gap: 20px; max-width: 760px; }
  .head { display: flex; align-items: flex-end; justify-content: space-between; gap: 12px; }

  .group {
    display: grid; gap: 6px;
    padding: 16px;
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
  }
  .group-head { display: flex; align-items: baseline; gap: 8px; }
  h2 { margin: 0; font-family: var(--mdx-font-display); font-size: 16px; }
  .group-count {
    font-size: 12px; padding: 1px 8px; border-radius: 999px;
    background: color-mix(in srgb, var(--mdx-accent-primary) 12%, transparent);
  }
  .group-line { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); }
  .rows { list-style: none; margin: 4px 0 0; padding: 0; display: grid; gap: 4px; }
  .row {
    display: flex; align-items: center; gap: 10px; min-height: 36px;
    padding: 6px 12px; border-radius: var(--mdx-radius-md); text-decoration: none; color: inherit;
    background: var(--mdx-surface-raised);
    border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 10%, transparent);
  }
  .row:hover { border-color: var(--mdx-accent-primary); }
  .row-detail { flex: 1; min-width: 0; font-size: 13.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .row-ref { flex: none; font-size: var(--mdx-text-xs); color: var(--mdx-text-muted); font-family: var(--mdx-font-mono, monospace); }
  .row-go { flex: none; color: var(--mdx-text-muted); }

  .empty {
    padding: 28px 20px;
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    border: 1px solid var(--mdx-border-subtle);
  }
  .empty p { margin: 0; color: var(--mdx-text-muted); font-size: 14px; }

  .quiet-line { font-size: 12.5px; color: var(--mdx-text-muted); }
  .quiet-more { display: inline; }
  .quiet-more summary { display: inline; cursor: pointer; }
</style>
