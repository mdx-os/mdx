<script>
  // The quiet proof affordance: a small verified glyph that opens full
  // provenance on intent. Surfaces stay calm and human; the receipts stay
  // one tap away, always real, never invented - and copyable from here,
  // so no surface needs a copy-id button in its primary row.
  // items: [{ label, value }].
  let { items = [], label = "Details" } = $props();

  let open = $state(false);
  let copied = $state("");
  let copiedTimer = null;

  function toggle() {
    open = !open;
  }

  async function copyValue(value) {
    try {
      await navigator.clipboard.writeText(value);
      copied = value;
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = ""), 1400);
    } catch (error) {
      // clipboard permission denied - the value is still visible to select
    }
  }

  function onKeydown(event) {
    if (event.key === "Escape") {
      open = false;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<span class="provenance" data-ui-provenance>
  <button
    type="button"
    class="mark"
    class:open
    aria-expanded={open}
    aria-label={label}
    title={label}
    onclick={toggle}
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" width="11" height="11" aria-hidden="true">
      <polyline points="20 6 9 17 4 12" />
    </svg>
  </button>
  {#if open}
    <span class="card" role="dialog" aria-label="Provenance">
      <span class="card-title">{label}</span>
      {#each items as item}
        {#if item.value}
          <span class="row">
            <span class="row-label">{item.label}</span>
            <span class="row-line">
              {#if /receipt|policy_|_decision_/.test(String(item.value)) && /^[a-z0-9_\-]+$/i.test(String(item.value))}
                <a class="row-value row-link" href={`/evidence/${encodeURIComponent(item.value)}`}><code>{item.value}</code></a>
              {:else}
                <code class="row-value">{item.value}</code>
              {/if}
              <button type="button" class="row-copy" onclick={() => copyValue(item.value)} aria-label="Copy {item.label}">
                {copied === item.value ? "copied" : "copy"}
              </button>
            </span>
          </span>
        {/if}
      {/each}
    </span>
  {/if}
</span>

<style>
  .provenance {
    position: relative;
    display: inline-flex;
  }

  .mark {
    display: inline-grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: none;
    background: color-mix(in srgb, var(--mdx-accent-success) 14%, transparent);
    color: var(--mdx-accent-success);
    cursor: pointer;
    opacity: 0.75;
    transition: opacity 160ms ease-out, transform 160ms ease-out;
  }

  .mark:hover,
  .mark.open,
  .mark:focus-visible {
    opacity: 1;
    transform: scale(1.08);
  }

  .card {
    position: absolute;
    bottom: calc(100% + 8px);
    left: 0;
    z-index: 30;
    display: grid;
    gap: 7px;
    min-width: 260px;
    max-width: 360px;
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-sunken);
    animation: provenance-in 180ms ease-out;
  }

  @keyframes provenance-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .card {
      animation: none;
    }
    .mark {
      transition: none;
    }
  }

  .card-title {
    color: var(--mdx-text-primary);
    font-size: 12px;
    font-weight: 600;
  }

  .row {
    display: grid;
    gap: 2px;
  }

  .row-label {
    color: var(--mdx-text-tertiary);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.07em;
  }

  .row-value {
    color: var(--mdx-text-secondary);
    font-family: var(--mdx-font-mono);
    font-size: 11px;
    overflow-wrap: anywhere;
  }

  .row-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }

  .row-copy {
    border: none;
    background: none;
    color: var(--mdx-text-muted);
    font-size: 10px;
    cursor: pointer;
    padding: 0;
    text-decoration: underline;
    text-underline-offset: 2px;
    flex: none;
  }

  .row-copy:hover {
    color: var(--mdx-text-secondary);
  }
  .row-link { text-decoration: none; }
  .row-link:hover code { text-decoration: underline; }
</style>
