<script>
  // Changelog: what has shipped, in human words. Reads the projection
  // of recorded entries (title, summary, kind) through the kernel proxy.
  // Fail-soft: unreachable kernel or empty list shows an honest empty
  // invitation. Entries arrive newest-first from the contract.

  let { data } = $props();
  const entries = $derived(
    Array.isArray(data.changelog?.entries) ? data.changelog.entries : []
  );
</script>

<svelte:head><title>Changelog - MDx</title></svelte:head>

<section class="page">
  <header class="head">
    <div class="mdx-page-head">
      <h1>Changelog</h1>
      <p class="mdx-page-sub">
        What has shipped, in the order it became real. Each entry says what changed and
        why it matters to the work.
      </p>
    </div>
  </header>

  {#if entries.length === 0}
    <div class="empty">
      <p>Nothing has shipped yet. Entries appear here when work ships.</p>
    </div>
  {:else}
    <ul class="entry-list">
      {#each entries as entry (entry.title)}
        <li class="entry">
          <div class="entry-head">
            <strong class="title">{entry.title}</strong>
            {#if entry.kind}
              <span class="chip" data-kind={entry.kind}>{entry.kind}</span>
            {/if}
          </div>
          {#if entry.summary}
            <p class="summary">{entry.summary}</p>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .page { display: grid; gap: 18px; max-width: 720px; margin: 0 auto; padding: 28px 20px 60px; }
  .head { display: grid; gap: 6px; }

  .empty { padding: 28px; border-radius: var(--mdx-radius-lg, 12px); background: var(--mdx-surface-raised); }
  .empty p { margin: 0; font-size: 14px; color: var(--mdx-text-muted); max-width: 560px; }

  .entry-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 14px; }
  .entry { padding: 14px 16px; border-radius: var(--mdx-radius-lg, 12px); background: var(--mdx-surface-raised); }
  .entry-head { display: flex; align-items: baseline; gap: 10px; }
  .title { font-size: 15px; font-weight: 650; line-height: 1.3; }
  .chip {
    font-size: var(--mdx-text-xs); font-weight: 650; letter-spacing: 0.02em; text-transform: uppercase;
    padding: 1px 8px; border-radius: 999px; border: 1px solid color-mix(in srgb, var(--mdx-text-muted) 30%, transparent);
    color: var(--mdx-text-muted); white-space: nowrap;
  }
  .chip[data-kind="feature"] { border-color: color-mix(in srgb, var(--mdx-accent-primary) 40%, transparent); color: var(--mdx-accent-primary); }
  .chip[data-kind="fix"] { border-color: color-mix(in srgb, var(--mdx-success, #4a8a5c) 40%, transparent); color: var(--mdx-success, #4a8a5c); }
  .chip[data-kind="improvement"] { border-color: color-mix(in srgb, var(--mdx-warning, #b88a3a) 40%, transparent); color: var(--mdx-warning, #b88a3a); }

  .summary { margin: 6px 0 0; font-size: 13.5px; color: var(--mdx-text-secondary); line-height: 1.45; }
</style>
