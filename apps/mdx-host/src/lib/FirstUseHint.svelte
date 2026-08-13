<script>
  // A progressive first-use moment: a slim, one-time, dismissible line that
  // orients a newcomer to a surface and points at the one thing to try - then
  // never shows again (a localStorage flag per surface, machine-only). It
  // complements the surface's empty state; it is NOT a permanent explainer and
  // NOT a wizard. Renders nothing on the server and nothing once dismissed, so
  // there is no flash for a returning operator.
  import { onMount } from "svelte";

  let { surface, title, body } = $props();

  let show = $state(false);
  let key = $derived(`mdx-firstuse-${surface}`);

  onMount(() => {
    try {
      show = !localStorage.getItem(key);
    } catch (error) {
      show = false;
    }
  });

  function dismiss() {
    try {
      localStorage.setItem(key, "1");
    } catch (error) {
      // best effort: a hint that cannot persist its dismissal just closes
    }
    show = false;
  }
</script>

{#if show}
  <aside class="firstuse" aria-label="First time here">
    <div class="firstuse-body">
      <strong>{title}</strong>
      <span>{body}</span>
    </div>
    <button type="button" class="firstuse-got" onclick={dismiss}>Got it</button>
  </aside>
{/if}

<style>
  .firstuse {
    display: flex;
    align-items: center;
    gap: 1rem;
    justify-content: space-between;
    width: 100%;
    max-width: 100%;
    min-width: 0;
    box-sizing: border-box;
    padding: 0.7rem 1rem;
    margin-bottom: 1rem;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }
  .firstuse-body {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 0.4rem;
    font-size: var(--mdx-text-sm);
    line-height: 1.45;
  }
  .firstuse-body strong {
    color: var(--mdx-text-primary);
  }
  .firstuse-body span {
    color: var(--mdx-text-secondary);
  }
  .firstuse-got {
    flex: none;
    padding: 0.35rem 0.8rem;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-pill);
    background: transparent;
    color: var(--mdx-text-secondary);
    font: inherit;
    font-size: var(--mdx-text-sm);
    cursor: pointer;
  }
  .firstuse-got:hover {
    color: var(--mdx-text-primary);
    border-color: var(--mdx-accent-primary);
  }
</style>
