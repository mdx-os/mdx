<script>
  // Companion answers, rendered. Escape-then-transform makes the HTML
  // safe by construction; block splitting makes streaming cheap - a
  // finished block's string never changes, so Svelte's equality check
  // skips it and only the block still receiving tokens re-parses.
  import { splitBlocks, repairBlock, renderBlock } from "../markdown.js";

  let { text = "", streaming = false } = $props();

  const blocks = $derived(splitBlocks(text));
</script>

<div class="md">
  {#each blocks as block, index (index)}
    {@html renderBlock(streaming && index === blocks.length - 1 ? repairBlock(block) : block)}
  {/each}
</div>

<style>
  .md {
    display: grid;
    gap: 8px;
    line-height: 1.55;
  }

  .md :global(p),
  .md :global(ul),
  .md :global(blockquote),
  .md :global(h3),
  .md :global(h4),
  .md :global(h5) {
    margin: 0;
  }

  .md :global(ul) {
    padding-left: 20px;
    display: grid;
    gap: 3px;
  }

  .md :global(pre) {
    margin: 0;
    padding: 10px 12px;
    border-radius: var(--mdx-radius-md, 8px);
    background: var(--mdx-surface-sunken);
    border: 1px solid var(--mdx-border-subtle);
    overflow-x: auto;
  }

  .md :global(code) {
    font-family: var(--mdx-font-mono);
    font-size: 0.88em;
  }

  .md :global(p code),
  .md :global(li code) {
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--mdx-surface-sunken);
  }

  .md :global(blockquote) {
    padding-left: 12px;
    border-left: 3px solid var(--mdx-border-default);
    color: var(--mdx-text-secondary);
  }

  .md :global(h3) {
    font-size: 1.05em;
  }

  .md :global(h4),
  .md :global(h5) {
    font-size: 1em;
  }

  .md :global(a) {
    color: var(--mdx-accent-primary);
  }
</style>
