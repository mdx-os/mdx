<script>
  export let status = "ready";
  export let label = "";
  export let ariaLabel = "";

  const LABELS = {
    "working-local": "Working locally",
    "waiting-live": "Waiting on live proof",
    ready: "Ready",
    partial: "Partly ready",
    missing: "Missing",
    "sample-data": "Using local sample data",
    blocked: "Blocked"
  };

  $: displayText = label || LABELS[status] || "Status unknown";
  $: resolvedAriaLabel = ariaLabel || displayText;
</script>

<span class="mdx-status-pill" data-status={status} aria-label={resolvedAriaLabel}>
  <span class="dot" aria-hidden="true"></span>
  <span class="label">{displayText}</span>
</span>

<style>
  .mdx-status-pill {
    display: inline-flex;
    align-items: center;
    gap: var(--mdx-space-sm);
    max-width: 100%;
    border: 1px solid var(--mdx-border-chip);
    border-radius: var(--mdx-radius-pill);
    padding: 7px 10px;
    color: var(--mdx-text-secondary);
    background: rgba(var(--mdx-surface-rgb), 0.72);
    font-family: var(--mdx-font-ui);
    font-size: 12px;
    font-weight: 800;
    line-height: 1;
    white-space: nowrap;
    transition:
      border-color var(--mdx-motion-base) var(--mdx-ease),
      background-color var(--mdx-motion-base) var(--mdx-ease),
      color var(--mdx-motion-base) var(--mdx-ease);
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-text-muted);
    flex: 0 0 auto;
  }

  .mdx-status-pill[data-status="working-local"],
  .mdx-status-pill[data-status="ready"] {
    border-color: color-mix(in srgb, var(--mdx-success-green) 32%, transparent);
    color: var(--mdx-success-green);
  }

  .mdx-status-pill[data-status="working-local"] .dot,
  .mdx-status-pill[data-status="ready"] .dot {
    background: var(--mdx-success-green);
    box-shadow: 0 0 18px color-mix(in srgb, var(--mdx-success-green) 36%, transparent);
  }

  .mdx-status-pill[data-status="waiting-live"],
  .mdx-status-pill[data-status="partial"],
  .mdx-status-pill[data-status="sample-data"] {
    border-color: color-mix(in srgb, var(--mdx-warning-amber) 34%, transparent);
    color: var(--mdx-warning-amber);
  }

  .mdx-status-pill[data-status="waiting-live"] .dot,
  .mdx-status-pill[data-status="partial"] .dot,
  .mdx-status-pill[data-status="sample-data"] .dot {
    background: var(--mdx-warning-amber);
    box-shadow: 0 0 18px color-mix(in srgb, var(--mdx-warning-amber) 30%, transparent);
  }

  .mdx-status-pill[data-status="missing"],
  .mdx-status-pill[data-status="blocked"] {
    border-color: color-mix(in srgb, var(--mdx-danger-red) 36%, transparent);
    color: var(--mdx-danger-red);
  }

  .mdx-status-pill[data-status="missing"] .dot,
  .mdx-status-pill[data-status="blocked"] .dot {
    background: var(--mdx-danger-red);
    box-shadow: 0 0 18px color-mix(in srgb, var(--mdx-danger-red) 32%, transparent);
  }
</style>
