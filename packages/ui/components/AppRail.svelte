<script>
  // Narrow v1-style app rail, shared by Twin and Forge (and future surfaces).
  // The host app owns placement (grid column, band or sticky); this owns the rail.
  export let groups = [];
  export let accent = "var(--mdx-evidence-cyan)";
  export let footTitle = "";
  export let sticky = false;
  export let brandLabel = "MDx";
  export let brandHref = "#home";
</script>

<aside class="app-rail" class:sticky aria-label="MDx app rail" style={`--rail-accent: ${accent}`}>
  <a class="rail-brand" href={brandHref} aria-label="MDx home">{brandLabel}</a>
  {#each groups as group, groupIndex}
    {#if groupIndex > 0}
      <span class="rail-divider" aria-hidden="true"></span>
    {/if}
    <div class="rail-group" role="group" aria-label={group.group}>
      {#each group.items as item}
        <a
          href={item.href}
          class="rail-link"
          class:active={item.active}
          aria-current={item.active ? "page" : undefined}
          aria-label={item.label}
        >
          <svg class="rail-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{@html item.icon}</svg>
          <span class="rail-tip">{item.label}</span>
        </a>
      {/each}
    </div>
  {/each}
  {#if footTitle}
    <span class="rail-foot-dot" title={footTitle} aria-label="Rail status"></span>
  {/if}
</aside>

<style>
  .app-rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 14px 0;
    border-right: 1px solid var(--mdx-border-soft);
    background: rgba(var(--mdx-surface-rgb), 0.46);
  }

  .app-rail.sticky {
    position: sticky;
    top: 16px;
    align-self: start;
  }

  .rail-brand {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 38px;
    height: 38px;
    border: 1px solid rgba(var(--mdx-evidence-cyan-rgb), 0.45);
    border-radius: var(--mdx-radius-control);
    color: var(--mdx-evidence-cyan);
    font-size: 13px;
    font-weight: 900;
    text-decoration: none;
  }

  .rail-group {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }

  .rail-link {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border-radius: var(--mdx-radius-control);
    color: var(--mdx-text-muted);
    text-decoration: none;
    transition:
      color var(--mdx-motion-base) var(--mdx-ease),
      background var(--mdx-motion-base) var(--mdx-ease);
  }

  .rail-link:hover {
    background: rgba(var(--mdx-surface-raised-rgb), 0.7);
    color: var(--mdx-text-secondary);
  }

  .rail-link.active {
    background: rgba(var(--mdx-surface-raised-rgb), 0.95);
    color: var(--rail-accent);
  }

  .rail-link.active::before {
    content: "";
    position: absolute;
    left: -10px;
    top: 9px;
    bottom: 9px;
    width: 3px;
    border-radius: var(--mdx-radius-pill);
    background: var(--rail-accent);
  }

  .rail-icon {
    width: 20px;
    height: 20px;
  }

  .rail-tip {
    position: absolute;
    left: calc(100% + 12px);
    top: 50%;
    transform: translateY(-50%) translateX(-4px);
    z-index: 6;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-control);
    padding: 4px 9px;
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font-size: 12px;
    font-weight: 700;
    white-space: nowrap;
    opacity: 0;
    pointer-events: none;
    transition:
      opacity var(--mdx-motion-fast) var(--mdx-ease),
      transform var(--mdx-motion-fast) var(--mdx-ease);
  }

  .rail-link:hover .rail-tip,
  .rail-link:focus-visible .rail-tip {
    opacity: 1;
    transform: translateY(-50%) translateX(0);
  }

  .rail-divider {
    width: 22px;
    height: 1px;
    background: var(--mdx-border-soft);
  }

  .rail-foot-dot {
    width: 8px;
    height: 8px;
    margin-top: auto;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-warning-amber);
    box-shadow: 0 0 0 4px rgba(var(--mdx-surface-raised-rgb), 0.4);
  }

  @media (max-width: 720px) {
    .app-rail {
      flex-direction: row;
      flex-wrap: wrap;
      justify-content: center;
      gap: 8px;
      padding: 12px;
      border-right: 0;
      border-bottom: 1px solid var(--mdx-border-soft);
    }

    .app-rail.sticky {
      position: static;
    }

    .rail-group {
      flex-direction: row;
    }

    .rail-divider {
      width: 1px;
      height: 22px;
    }

    .rail-foot-dot {
      margin-top: 0;
    }

    .rail-tip {
      display: none;
    }
  }
</style>
