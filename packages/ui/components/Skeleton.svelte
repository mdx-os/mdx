<script>
  export let variant = "text";
  export let count = 3;
  export let columns = 4;
  export let width = "100%";
  export let height = "";
  export let label = "Loading local proof";

  $: items = Array.from({ length: count });
  $: columnItems = Array.from({ length: columns });
</script>

{#if variant === "text"}
  <div class="mdx-skeleton text" style:max-width={width} aria-label={label} role="status">
    {#each items as _, index}
      <span class="shimmer line" style:width={index === items.length - 1 ? "75%" : "100%"}></span>
    {/each}
  </div>
{:else if variant === "card"}
  <div class="mdx-skeleton stack" aria-label={label} role="status">
    {#each items as _}
      <span class="shimmer card" style:width={width} style:height={height || "80px"}></span>
    {/each}
  </div>
{:else if variant === "table"}
  <div class="mdx-skeleton table" aria-label={label} role="status">
    <div class="row header">
      {#each columnItems as _}
        <span class="shimmer cell"></span>
      {/each}
    </div>
    {#each items as _}
      <div class="row">
        {#each columnItems as _}
          <span class="shimmer cell"></span>
        {/each}
      </div>
    {/each}
  </div>
{:else}
  <div class="mdx-skeleton" aria-label={label} role="status">
    <span class="shimmer chart" style:width={width} style:height={height || "180px"}></span>
  </div>
{/if}

<style>
  .mdx-skeleton {
    display: grid;
    gap: 10px;
    width: 100%;
  }

  .stack {
    gap: 12px;
  }

  .shimmer {
    display: block;
    border-radius: var(--mdx-radius-control);
    background:
      linear-gradient(
        90deg,
        rgba(var(--mdx-surface-raised-rgb), 0.72) 25%,
        rgba(var(--mdx-evidence-cyan-rgb), 0.12) 50%,
        rgba(var(--mdx-surface-raised-rgb), 0.72) 75%
      );
    background-size: 200% 100%;
    animation: mdx-skeleton-shimmer 1.5s var(--mdx-ease) infinite;
  }

  .line {
    height: 12px;
  }

  .card {
    min-height: 80px;
    border: 1px solid var(--mdx-border-soft);
  }

  .table {
    gap: 0;
  }

  .row {
    display: flex;
    gap: 8px;
    padding: 8px 0;
    border-bottom: 1px solid var(--mdx-border-soft);
  }

  .cell {
    flex: 1 1 0;
    height: 12px;
  }

  .header .cell {
    height: 14px;
    opacity: 0.82;
  }

  .chart {
    min-height: 160px;
    border: 1px solid var(--mdx-border-soft);
  }

  @keyframes mdx-skeleton-shimmer {
    0% {
      background-position: -200% 0;
    }

    100% {
      background-position: 200% 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .shimmer {
      animation: none;
      background: rgba(var(--mdx-surface-raised-rgb), 0.72);
    }
  }
</style>
