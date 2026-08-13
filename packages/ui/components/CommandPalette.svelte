<script>
  import { tick, onMount, onDestroy } from "svelte";

  // Read-only navigator. Reuses the same nav array each app already passes to
  // OperatorChrome and splits it into cross-surface destinations (absolute URLs)
  // and on-page sections (hash anchors). It only navigates; it never fetches,
  // writes, or holds authority. The trigger lives in OperatorChrome and opens
  // this via the mdx:open-command-palette window event.
  export let nav = [];
  export let activeLabel = "";
  export let label = "Search and jump";

  function handleOpenEvent() {
    openPalette();
  }

  onMount(() => {
    window.addEventListener("mdx:open-command-palette", handleOpenEvent);
  });

  onDestroy(() => {
    if (typeof window !== "undefined") {
      window.removeEventListener("mdx:open-command-palette", handleOpenEvent);
    }
  });

  let open = false;
  let query = "";
  let activeIndex = 0;
  let inputEl;

  $: surfaces = nav.filter((item) => /^https?:/.test(item.href));
  $: sections = nav.filter((item) => item.href.startsWith("#"));
  $: needle = query.trim().toLowerCase();
  $: match = (item) => needle.length === 0 || `${item.label} ${item.meta ?? ""}`.toLowerCase().includes(needle);
  $: matchedSurfaces = surfaces.filter(match);
  $: matchedSections = sections.filter(match);
  $: flat = [...matchedSurfaces, ...matchedSections];
  $: if (activeIndex > flat.length - 1) {
    activeIndex = Math.max(0, flat.length - 1);
  }

  async function openPalette() {
    open = true;
    query = "";
    activeIndex = 0;
    await tick();
    inputEl?.focus();
  }

  function closePalette() {
    open = false;
  }

  function indexOfItem(item) {
    return flat.indexOf(item);
  }

  function navigateTo(item) {
    if (!item) {
      return;
    }
    closePalette();
    window.location.href = item.href;
  }

  function onWindowKey(event) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      if (open) {
        closePalette();
      } else {
        openPalette();
      }
    }
  }

  function onPaletteKey(event) {
    if (event.key === "Escape") {
      event.preventDefault();
      closePalette();
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      activeIndex = flat.length === 0 ? 0 : (activeIndex + 1) % flat.length;
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      activeIndex = flat.length === 0 ? 0 : (activeIndex - 1 + flat.length) % flat.length;
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      navigateTo(flat[activeIndex]);
    }
  }
</script>

<svelte:window onkeydown={onWindowKey} />

{#if open}
  <div class="palette-overlay">
    <button type="button" class="palette-backdrop" aria-label="Close search and jump menu" onclick={closePalette}></button>
    <div
      class="palette"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      aria-label={label}
    >
      <div class="palette-search">
        <span class="search-glyph" aria-hidden="true">/</span>
        <input
          bind:this={inputEl}
          bind:value={query}
          onkeydown={onPaletteKey}
          type="text"
          class="palette-input"
          placeholder="Search surfaces and sections"
          aria-label="Search surfaces and sections"
          autocomplete="off"
          spellcheck="false"
        />
        <kbd>esc</kbd>
      </div>

      <div class="palette-results" role="listbox" aria-label="Navigation targets">
        {#if matchedSurfaces.length > 0}
          <p class="result-group">Go to surface</p>
          {#each matchedSurfaces as item (item.label + item.href)}
            <a
              href={item.href}
              class="result"
              role="option"
              aria-selected={indexOfItem(item) === activeIndex}
              data-active={indexOfItem(item) === activeIndex}
              onmouseenter={() => (activeIndex = indexOfItem(item))}
              onclick={closePalette}
            >
              <span class="result-label">{item.label}</span>
              {#if item.label === activeLabel}
                <span class="result-here">Here</span>
              {/if}
              <span class="result-meta">{item.meta}</span>
            </a>
          {/each}
        {/if}

        {#if matchedSections.length > 0}
          <p class="result-group">Jump to section</p>
          {#each matchedSections as item (item.label + item.href)}
            <a
              href={item.href}
              class="result"
              role="option"
              aria-selected={indexOfItem(item) === activeIndex}
              data-active={indexOfItem(item) === activeIndex}
              onmouseenter={() => (activeIndex = indexOfItem(item))}
              onclick={closePalette}
            >
              <span class="result-label">{item.label}</span>
              <span class="result-meta">{item.meta}</span>
            </a>
          {/each}
        {/if}

        {#if flat.length === 0}
          <p class="result-empty">Nothing matches that yet. Try a surface or section name.</p>
        {/if}
      </div>

      <p class="palette-foot">Read-only navigator. It moves you between surfaces and sections; it never acts.</p>
    </div>
  </div>
{/if}

<style>
  kbd {
    border: 1px solid var(--mdx-border-soft);
    border-radius: var(--mdx-radius-control);
    padding: 1px 6px;
    color: var(--mdx-text-muted);
    font-family: var(--mdx-font-mono);
    font-size: 11px;
  }

  .palette-overlay {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 12vh 16px 16px;
    background: rgba(var(--mdx-bg-rgb), 0.62);
    backdrop-filter: blur(6px);
  }

  .palette-backdrop {
    position: absolute;
    inset: 0;
    border: none;
    padding: 0;
    background: transparent;
    cursor: default;
  }

  .palette {
    position: relative;
    z-index: 1;
    display: grid;
    gap: 12px;
    width: min(100%, 560px);
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-panel);
    padding: 14px;
    background: rgba(var(--mdx-surface-raised-rgb), 0.98);
    box-shadow: var(--mdx-shadow-card);
  }

  .palette-search {
    display: flex;
    align-items: center;
    gap: 10px;
    border: 1px solid rgba(var(--mdx-focus-blue-rgb), 0.4);
    border-radius: var(--mdx-radius-control);
    padding: 0 12px;
    background: rgba(var(--mdx-surface-rgb), 0.7);
  }

  .search-glyph {
    color: var(--mdx-text-muted);
    font-family: var(--mdx-font-mono);
    font-size: 14px;
  }

  .palette-input {
    flex: 1;
    min-height: 42px;
    border: none;
    background: transparent;
    color: var(--mdx-text-primary);
    font-family: var(--mdx-font-ui);
    font-size: 15px;
  }

  .palette-input:focus {
    outline: none;
  }

  .palette-results {
    display: grid;
    gap: 2px;
    max-height: 46vh;
    overflow-y: auto;
  }

  .result-group {
    margin: 8px 4px 2px;
    color: var(--mdx-text-caption);
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .result {
    display: flex;
    align-items: center;
    gap: 10px;
    border: 1px solid transparent;
    border-radius: var(--mdx-radius-control);
    padding: 9px 11px;
    color: var(--mdx-text-secondary);
    text-decoration: none;
  }

  .result[data-active="true"] {
    border-color: rgba(var(--mdx-focus-blue-rgb), 0.5);
    background: rgba(var(--mdx-focus-blue-rgb), 0.16);
    color: var(--mdx-text-primary);
  }

  .result-label {
    font-size: 14px;
    font-weight: 700;
  }

  .result-here {
    border: 1px solid var(--mdx-border-soft);
    border-radius: var(--mdx-radius-pill);
    padding: 1px 8px;
    color: var(--mdx-text-muted);
    font-size: 10px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .result-meta {
    margin-left: auto;
    color: var(--mdx-text-muted);
    font-size: 12px;
  }

  .result-empty {
    margin: 10px 4px;
    color: var(--mdx-text-muted);
    font-size: 13px;
  }

  .palette-foot {
    margin: 0;
    color: var(--mdx-text-caption);
    font-size: 12px;
    line-height: 1.4;
  }
</style>
