<script>
  import Skeleton from "./Skeleton.svelte";

  export let state = "Using local sample data";
  export let why = "This surface is rendering deterministic local fallback data.";
  export let appears = [];
  export let primaryCommand = "make local-proof";
  export let secondaryCommand = "";
  export let source = "local sample data";
  export let tone = "empty";

  $: visibleAppears = appears.slice(0, 3);
</script>

<section class="mdx-empty-state" data-tone={tone} aria-label={state}>
  <p>Preview state</p>
  <h3>{state}</h3>
  <span>{why}</span>

  {#if visibleAppears.length > 0}
    <div class="appears">
      <strong>When proof arrives, you will see</strong>
      <Skeleton variant="text" count={2} label="Proof preview loading shape" />
      <ul>
        {#each visibleAppears as item}
          <li>{item}</li>
        {/each}
      </ul>
    </div>
  {/if}

  {#if primaryCommand || secondaryCommand}
    <div class="commands" aria-label="Refresh commands">
      {#if primaryCommand}
        <code>{primaryCommand}</code>
      {/if}
      {#if secondaryCommand}
        <code>{secondaryCommand}</code>
      {/if}
    </div>
  {/if}

  <small>{source}</small>
</section>

<style>
  .mdx-empty-state {
    --mdx-empty-accent: var(--mdx-evidence-cyan);
    display: grid;
    gap: 12px;
    margin-top: 12px;
    padding: 12px 0 0 14px;
    border-left: 2px solid var(--mdx-empty-accent);
    color: var(--mdx-text-secondary);
    transition: border-color var(--mdx-motion-base) var(--mdx-ease);
  }

  .mdx-empty-state[data-tone="warning"] {
    --mdx-empty-accent: var(--mdx-warning-amber);
  }

  .mdx-empty-state[data-tone="unavailable"] {
    --mdx-empty-accent: var(--mdx-danger-red);
  }

  p,
  h3,
  span,
  ul {
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
    font-size: 20px;
    line-height: 1.15;
  }

  span,
  li,
  small {
    font-size: 13px;
    line-height: 1.45;
  }

  .appears {
    display: grid;
    gap: 8px;
  }

  strong {
    color: var(--mdx-text-primary);
    font-size: 13px;
  }

  ul {
    display: grid;
    gap: 6px;
    padding: 0;
    list-style: none;
  }

  li {
    position: relative;
    padding-left: 16px;
  }

  li::before {
    position: absolute;
    left: 0;
    color: var(--mdx-empty-accent);
    content: "·";
    font-weight: 900;
  }

  .commands {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  code {
    border: 1px solid var(--mdx-border-chip);
    border-radius: var(--mdx-radius-control);
    padding: 7px 9px;
    color: var(--mdx-text-primary);
    background: rgba(var(--mdx-surface-rgb), 0.82);
    font-family: var(--mdx-font-mono);
    font-size: 12px;
  }

  small {
    color: var(--mdx-text-muted);
  }
</style>
