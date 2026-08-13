<script>
  // The global command surface: Cmd/Ctrl+K anywhere (Forge keeps its richer
  // page-local palette; this one yields there). Same a11y contract as the
  // Forge original: labeled dialog, focus trap-and-restore, type-to-filter,
  // arrows, Enter, Escape. Navigation and DISPATCH - a typed sentence
  // becomes a build or a Twin ask; admission gates apply at the destination.
  import { goto } from "$app/navigation";
  import { page } from "$app/state";

  let open = $state(false);
  let query = $state("");
  let index = $state(0);
  let inputEl = $state(null);
  let returnFocus = null;

  const surfaces = [
    { label: "Twin", hint: "your thinking partner", href: "/twin", key: "1" },
    { label: "Message", hint: "coordination and approvals", href: "/message", key: "2" },
    { label: "Forge", hint: "hand work to agents", href: "/forge", key: "3" },
    { label: "Pages", hint: "what the company knows, written down", href: "/pages", key: "4" },
    { label: "Memory", hint: "what MDx has learned from your work", href: "/memory", key: "" },
    { label: "Marketplace", hint: "capabilities and skills", href: "/marketplace", key: "5" },
    { label: "Runs", hint: "watch work happen", href: "/forge/runs", key: "6" },
    { label: "Review", hint: "decide on finished work", href: "/forge/review", key: "" },
    { label: "Changelog", hint: "what shipped", href: "/changelog", key: "" },
    { label: "Help", hint: "concepts and shortcuts", href: "/help", key: "" }
  ];

  const commands = $derived.by(() => {
    const q = query.trim();
    const lower = q.toLowerCase();
    const list = [];
    // A real sentence dispatches (the Raycast move: search that acts).
    if (q.length >= 12 && q.includes(" ")) {
      list.push({
        label: `Start a build: "${q.length > 60 ? q.slice(0, 57) + "..." : q}"`,
        hint: "hand this ask to Forge",
        run: () => goto(`/forge?intent=${encodeURIComponent(q)}`)
      });
      list.push({
        label: `Ask Twin: "${q.length > 60 ? q.slice(0, 57) + "..." : q}"`,
        hint: "think it through first",
        run: () => goto(`/twin?ask=${encodeURIComponent(q)}`)
      });
    }
    for (const surface of surfaces) {
      if (!lower || surface.label.toLowerCase().includes(lower) || surface.hint.includes(lower)) {
        list.push({ label: surface.label, hint: surface.hint, run: () => goto(surface.href), key: surface.key });
      }
    }
    return list;
  });

  export function toggle() {
    if (open) {
      close();
    } else {
      returnFocus = document.activeElement;
      open = true;
      query = "";
      index = 0;
      queueMicrotask(() => inputEl?.focus());
    }
  }

  function close() {
    open = false;
    returnFocus?.focus?.();
  }

  function run(command) {
    close();
    command.run();
  }

  function onKeydown(event) {
    if (!open) return;
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      index = Math.min(index + 1, commands.length - 1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      index = Math.max(index - 1, 0);
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (commands[index]) run(commands[index]);
    }
  }

  function globalKeys(event) {
    const meta = event.metaKey || event.ctrlKey;
    if (!meta) return;
    // Forge's own palette owns Cmd+K on its pages.
    if (event.key.toLowerCase() === "k" && !page.url.pathname.startsWith("/forge")) {
      event.preventDefault();
      toggle();
      return;
    }
    // Cmd+1..6: surface switching from anywhere (matches the native app).
    const jump = surfaces.find((surface) => surface.key && surface.key === event.key);
    if (jump && !event.shiftKey && !event.altKey) {
      const target = event.target;
      const typing = target?.tagName === "INPUT" || target?.tagName === "TEXTAREA" || target?.isContentEditable;
      if (!typing) {
        event.preventDefault();
        goto(jump.href);
      }
    }
  }
</script>

<svelte:window onkeydown={globalKeys} />

{#if open}
  <div class="cp-backdrop" role="presentation" onclick={close}>
    <div
      class="cp"
      role="dialog"
      aria-modal="true"
      aria-label="Command palette"
      tabindex="-1"
      onclick={(event) => event.stopPropagation()}
      onkeydown={onKeydown}
    >
      <input
        bind:this={inputEl}
        bind:value={query}
        oninput={() => (index = 0)}
        class="cp-input"
        type="text"
        placeholder="Go somewhere, or type what to build..."
        aria-label="Command"
      />
      <ul class="cp-list" role="listbox" aria-label="Commands">
        {#each commands as command, i (command.label)}
          <li>
            <button
              type="button"
              class="cp-item"
              class:active={i === index}
              role="option"
              aria-selected={i === index}
              onclick={() => run(command)}
              onmouseenter={() => (index = i)}
            >
              <span class="cp-label">{command.label}</span>
              <span class="cp-hint">{command.hint}</span>
              {#if command.key}<kbd class="cp-key">⌘{command.key}</kbd>{/if}
            </button>
          </li>
        {/each}
        {#if commands.length === 0}
          <li class="cp-empty">Nothing matches - try fewer words.</li>
        {/if}
      </ul>
    </div>
  </div>
{/if}

<style>
  .cp-backdrop { position: fixed; inset: 0; z-index: 90; background: color-mix(in srgb, black 34%, transparent); display: flex; align-items: flex-start; justify-content: center; padding-top: 14vh; }
  .cp { width: min(600px, calc(100vw - 32px)); border: 1px solid var(--mdx-border-subtle); border-radius: 12px; background: var(--mdx-surface-base); box-shadow: 0 18px 50px color-mix(in srgb, black 30%, transparent); overflow: hidden; }
  .cp-input { width: 100%; border: none; outline: none; background: transparent; padding: 15px 18px; font: inherit; font-size: 15px; color: var(--mdx-text-primary); border-bottom: 1px solid var(--mdx-border-subtle); }
  .cp-list { list-style: none; margin: 0; padding: 6px; max-height: 46vh; overflow: auto; }
  .cp-item { display: flex; align-items: baseline; gap: 10px; width: 100%; padding: 9px 12px; border: none; border-radius: 8px; background: none; font: inherit; text-align: left; cursor: pointer; }
  .cp-item.active { background: color-mix(in srgb, var(--mdx-accent-primary) 12%, transparent); }
  .cp-label { color: var(--mdx-text-primary); font-size: 13.5px; font-weight: 550; }
  .cp-hint { flex: 1; color: var(--mdx-text-muted); font-size: 12px; }
  .cp-key { font-family: var(--mdx-font-mono, monospace); font-size: 11px; color: var(--mdx-text-muted); border: 1px solid var(--mdx-border-subtle); border-radius: 4px; padding: 1px 5px; }
  .cp-empty { padding: 14px; font-size: 13px; color: var(--mdx-text-muted); }
</style>
