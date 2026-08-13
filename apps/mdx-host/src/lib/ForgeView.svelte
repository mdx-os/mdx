<script>
  // The shared Forge subview shell: one header treatment, one subview tab
  // list, and one content container, so switching between Forge views never
  // shifts the frame. Build keeps its first-viewport header inline because
  // the product-surface gate checks its literal h1 and governed verb.
  //
  // The subview nav lives here once. Old run/mission/fleet detail routes stay
  // alive, but they now share one Trails lane in the visible IA.
  import { page } from "$app/stores";

  let { title, subtitle = "", context = "", backTo = null, actions, children } = $props();

  const TABS = [
    { href: "/forge", label: "Build" },
    { href: "/forge/runs", label: "Trails", aliases: ["/forge/work", "/forge/missions", "/forge/fleets"] },
    { href: "/forge/review", label: "Review" },
    { href: "/forge/models", label: "Models" },
    { href: "/forge/controls", label: "Controls" }
  ];

  const current = $derived(($page.url.pathname || "/forge").replace(/\/+$/, "") || "/forge");
  const isActive = (tab) =>
    tab.href === "/forge"
      ? current === "/forge"
      : current === tab.href || (tab.aliases ?? []).some((alias) => current === alias);
</script>

<section class="forge-view">
  <header class="forge-head">
    <div class="mdx-page-head">
      <div class="fh-title">
        {#if backTo}
          <a class="fh-back" href={backTo.href}>
            <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m15 18-6-6 6-6"/></svg>
            {backTo.label}
          </a>
        {/if}
        <h1>{title}</h1>
        {#if context}<span class="forge-context">{context}</span>{/if}
      </div>
      {#if subtitle}<p class="mdx-page-sub">{subtitle}</p>{/if}
    </div>
    <div class="forge-head-aside">
      <nav class="mdx-segmented" aria-label="Forge views">
        {#each TABS as tab (tab.href)}
          <a href={tab.href} class:active={isActive(tab)} aria-current={isActive(tab) ? "page" : undefined}>{tab.label}</a>
        {/each}
      </nav>
      {#if actions}{@render actions()}{/if}
    </div>
  </header>
  <div class="forge-body">
    {@render children?.()}
  </div>
</section>

<style>
  .forge-view {
    max-width: 1020px;
    margin: 0 auto;
    padding: 28px 20px 60px;
    width: 100%;
    min-width: 0;
    box-sizing: border-box;
    overflow-x: clip;
  }
  .forge-view *,
  .forge-view *::before,
  .forge-view *::after {
    box-sizing: border-box;
  }
  /* Mirrors the Overview header: title block left, segmented nav pinned
     top-right. The title column shrinks (flex 1, min-width 0) so a long
     subtitle wraps inside its column instead of pushing the nav below -
     the nav stays in the same place on every view. */
  .forge-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px 24px;
    margin-bottom: 18px;
    min-width: 0;
  }
  .forge-head .mdx-page-head {
    flex: 1 1 0;
    min-width: 0;
  }
  /* h1 stays the top line of the title column so the nav aligns with it on
     every view (matching Overview). The drilldown context rides inline beside
     the title as a quiet tag, never a line above that would lift the nav. */
  .fh-title {
    display: flex;
    align-items: baseline;
    gap: 10px;
    flex-wrap: wrap;
    min-width: 0;
  }
  .fh-back {
    align-self: center;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    margin-right: 2px;
    padding: 5px 12px 5px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill, 999px);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    font-weight: 600;
    text-decoration: none;
    white-space: nowrap;
    transition: border-color 0.12s ease, color 0.12s ease;
  }
  .fh-back:hover {
    color: var(--mdx-text-primary);
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 40%, var(--mdx-border-subtle));
  }
  .forge-context {
    align-self: center;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill, 999px);
    padding: 2px 9px;
  }
  .forge-head-aside {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    max-width: 100%;
    flex-wrap: wrap;
  }
  .forge-head-aside :global(.mdx-segmented) {
    max-width: 100%;
    overflow-x: auto;
  }
  .forge-body {
    display: grid;
    gap: 18px;
    min-width: 0;
    max-width: 100%;
  }
  @media (max-width: 720px) {
    .forge-head {
      flex-direction: column;
    }
    .forge-head-aside {
      flex: none;
      width: 100%;
    }
  }
</style>
