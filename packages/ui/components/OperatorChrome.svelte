<script>
  export let brand = "Forge";
  export let nav = [];
  export let activeLabel = brand;
  export let themeModes = [];
  export let themeMode = "system";
  export let homeHref = "http://127.0.0.1:5175";
  export let label = "MDx local console navigation";

  function openCommandPalette() {
    window.dispatchEvent(new CustomEvent("mdx:open-command-palette"));
  }
</script>

<nav class="operator-chrome" data-ui-operator-chrome aria-label={label}>
  <a class="brand" href={homeHref} aria-label="MDx local console home">
    <span>MDx</span>
    <strong>{brand}</strong>
  </a>
  <div class="chrome-right">
    <button
      type="button"
      class="chrome-search"
      aria-label="Open search and jump menu, shortcut Command or Control K"
      aria-haspopup="dialog"
      onclick={openCommandPalette}
    >
      <span>Search</span>
      <kbd>⌘K</kbd>
    </button>
    <div class="nav-links">
      {#each nav as item}
        <a href={item.href} title={item.meta} aria-current={item.label === activeLabel ? "page" : undefined}>{item.label}</a>
      {/each}
    </div>
  </div>
</nav>

<div class="theme-switcher" data-ui-theme-switcher aria-label="Theme mode">
  <span>Theme mode</span>
  {#each themeModes as mode}
    <a href={mode.href} aria-current={themeMode === mode.id ? "true" : undefined}>{mode.label}</a>
  {/each}
</div>

<style>
  .operator-chrome {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin: -24px -24px 0;
    padding: 14px 24px;
    border-bottom: 1px solid var(--mdx-border-soft);
    background: rgba(var(--mdx-bg-rgb), 0.86);
    backdrop-filter: blur(18px);
  }

  .brand {
    display: inline-flex;
    align-items: baseline;
    gap: 8px;
    min-width: 170px;
    color: var(--mdx-text-primary);
    text-decoration: none;
  }

  .brand span {
    display: inline-grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border: 1px solid rgba(var(--mdx-evidence-cyan-rgb), 0.34);
    border-radius: var(--mdx-radius-control);
    background: var(--mdx-brand-surface);
    color: var(--mdx-evidence-cyan);
    font-size: 12px;
    font-weight: 800;
  }

  .brand strong {
    font-size: 14px;
  }

  .chrome-right {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
  }

  .chrome-search {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 34px;
    border: 1px solid var(--mdx-border-soft);
    border-radius: var(--mdx-radius-pill);
    padding: 0 12px;
    background: rgba(var(--mdx-surface-raised-rgb), 0.82);
    color: var(--mdx-nav-text);
    font-family: var(--mdx-font-ui);
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
  }

  .chrome-search:hover {
    color: var(--mdx-text-primary);
    border-color: rgba(var(--mdx-focus-blue-rgb), 0.5);
  }

  .chrome-search kbd {
    border: 1px solid var(--mdx-border-soft);
    border-radius: var(--mdx-radius-control);
    padding: 1px 6px;
    color: var(--mdx-text-muted);
    font-family: var(--mdx-font-mono);
    font-size: 11px;
  }

  .nav-links {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 6px;
  }

  .nav-links a,
  .theme-switcher a,
  .theme-switcher span {
    display: inline-flex;
    align-items: center;
    min-height: 34px;
    border: 1px solid var(--mdx-border-soft);
    border-radius: var(--mdx-radius-control);
    padding: 0 10px;
    color: var(--mdx-nav-text);
    text-decoration: none;
    font-size: 13px;
    font-weight: 700;
    background: rgba(var(--mdx-surface-raised-rgb), 0.82);
  }

  .nav-links a[aria-current="page"],
  .theme-switcher a[aria-current="true"] {
    border-color: rgba(var(--mdx-focus-blue-rgb), 0.64);
    background: var(--mdx-focus-blue);
    color: var(--mdx-primary-on-blue);
  }

  .theme-switcher {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 6px;
    margin-top: 12px;
  }

  .theme-switcher span {
    min-height: 28px;
    border-color: transparent;
    background: transparent;
    color: var(--mdx-text-caption);
    font-size: 12px;
    text-transform: uppercase;
  }

  .theme-switcher a {
    min-height: 28px;
    color: var(--mdx-text-caption);
    font-size: 12px;
  }

  @media (max-width: 720px) {
    .operator-chrome {
      position: static;
      align-items: flex-start;
      margin: -18px -18px 0;
      padding: 12px 18px;
    }

    .brand {
      min-width: 0;
    }

    .nav-links {
      justify-content: flex-start;
    }

    .theme-switcher {
      justify-content: flex-start;
      flex-wrap: wrap;
    }
  }
</style>
