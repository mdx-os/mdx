<script>
  import { buildMessageCatchUp } from "./messageCatchUp.js";

  let { channel, messages = [], pending = [], onClose = () => {} } = $props();

  const summary = $derived(buildMessageCatchUp(messages));
  const recent = $derived(summary.recent);
  const participants = $derived(summary.participants);
  const highlights = $derived(summary.highlights);
</script>

<section class="catch-up mdx-elevated" aria-label="Catch up on this channel">
  <header>
    <div>
      <p>Catch-up</p>
      <h2>{recent.length === 0 ? "Nothing new in this channel" : `${recent.length} recent updates from ${participants.length} ${participants.length === 1 ? "participant" : "participants"}`}</h2>
    </div>
    <button type="button" onclick={onClose} aria-label="Close catch-up">Close</button>
  </header>

  {#if pending.length > 0}
    <div class="needs-you">
      <strong>{pending.length} {pending.length === 1 ? "item needs" : "items need"} you</strong>
      <span>Decisions and direct asks stay in your inbox until you answer.</span>
    </div>
  {/if}

  {#if highlights.length > 0}
    <ol>
      {#each highlights as item (item.id)}
        <li><strong>{item.actor}</strong><span>{item.body}</span></li>
      {/each}
    </ol>
  {:else}
    <p class="empty">When the conversation moves, the most recent recorded updates will appear here.</p>
  {/if}

  <footer>Composed on this device from recorded messages in #{channel}. No model call or action ran.</footer>
</section>

<style>
  .catch-up { margin: 12px 0; padding: 16px; border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 24%, var(--mdx-border-subtle)); border-radius: var(--mdx-radius-lg); background: color-mix(in srgb, var(--mdx-accent-primary) 5%, var(--mdx-surface-base)); }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
  header p { margin: 0 0 3px; color: var(--mdx-accent-primary); font-size: var(--mdx-text-xs); font-weight: 700; text-transform: uppercase; letter-spacing: .08em; }
  h2 { margin: 0; font-size: var(--mdx-text-md); line-height: 1.35; }
  header button { border: 0; background: transparent; color: var(--mdx-text-secondary); cursor: pointer; }
  .needs-you { display: flex; flex-wrap: wrap; gap: 5px 12px; margin-top: 12px; padding: 10px 12px; border-radius: var(--mdx-radius-md); background: var(--mdx-surface-muted); font-size: var(--mdx-text-sm); }
  .needs-you span { color: var(--mdx-text-secondary); }
  ol { display: grid; gap: 8px; margin: 14px 0 0; padding: 0; list-style: none; }
  li { display: grid; grid-template-columns: minmax(80px, 120px) 1fr; gap: 10px; font-size: var(--mdx-text-sm); }
  li strong { overflow: hidden; color: var(--mdx-text-primary); text-overflow: ellipsis; white-space: nowrap; }
  li span { color: var(--mdx-text-secondary); line-height: 1.45; }
  .empty { margin: 14px 0 0; color: var(--mdx-text-secondary); font-size: var(--mdx-text-sm); }
  footer { margin-top: 14px; color: var(--mdx-text-muted); font-size: var(--mdx-text-xs); }
  @media (max-width: 680px) { .catch-up { margin-inline: 12px; } li { grid-template-columns: 1fr; gap: 2px; } }
</style>
