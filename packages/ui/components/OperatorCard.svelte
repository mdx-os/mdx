<script>
  import StatusPill from "./StatusPill.svelte";

  export let eyebrow = "";
  export let title = "";
  export let body = "";
  export let meta = "";
  export let status = "";
  export let statusLabel = "";
  export let tone = "neutral";
</script>

<article class="operator-card" data-ui-operator-card data-tone={tone}>
  {#if status || statusLabel}
    <StatusPill status={status || "ready"} label={statusLabel} />
  {/if}
  {#if eyebrow}
    <p>{eyebrow}</p>
  {/if}
  <h3>{title}</h3>
  {#if body}
    <span>{body}</span>
  {/if}
  {#if meta}
    <small>{meta}</small>
  {/if}
</article>

<style>
  .operator-card {
    display: grid;
    align-content: start;
    gap: 9px;
    min-height: 154px;
    border: 1px solid var(--mdx-border-soft);
    border-radius: var(--mdx-radius-panel);
    padding: 16px;
    background: rgba(var(--mdx-surface-rgb), 0.78);
    box-shadow: var(--mdx-shadow-card);
  }

  .operator-card[data-tone="success"] {
    border-color: color-mix(in srgb, var(--mdx-success-green) 34%, var(--mdx-border-soft));
  }

  .operator-card[data-tone="warning"] {
    border-color: color-mix(in srgb, var(--mdx-warning-amber) 34%, var(--mdx-border-soft));
  }

  .operator-card[data-tone="danger"] {
    border-color: color-mix(in srgb, var(--mdx-danger-red) 34%, var(--mdx-border-soft));
  }

  .operator-card[data-tone="info"] {
    border-color: color-mix(in srgb, var(--mdx-evidence-cyan) 30%, var(--mdx-border-soft));
  }

  p,
  h3,
  span,
  small {
    margin: 0;
  }

  p {
    color: var(--mdx-text-caption);
    font-size: 12px;
    font-weight: 800;
    letter-spacing: 0;
    text-transform: uppercase;
  }

  h3 {
    color: var(--mdx-text-primary);
    font-family: var(--mdx-font-display);
    font-size: 24px;
    line-height: 1.12;
  }

  span,
  small {
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.5;
  }
</style>
