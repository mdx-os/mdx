<script>
  // One work-trail grammar at every scale. The surrounding route supplies the
  // receipt-derived active stage; this component only renders that projection.
  let { scale = "Run", active = "plan", title = "One trail", detail = "" } = $props();

  const stages = [
    { key: "plan", label: "Plan" },
    { key: "build", label: "Build" },
    { key: "prove", label: "Prove" },
    { key: "review", label: "Review" }
  ];
  const activeIndex = $derived(Math.max(0, stages.findIndex((stage) => stage.key === active)));
</script>

<section class="trail-map" aria-label={`${scale} trail`}>
  <div class="trail-copy">
    <span>{scale} trail</span>
    <strong>{title}</strong>
    {#if detail}<p>{detail}</p>{/if}
  </div>
  <ol>
    {#each stages as stage, index (stage.key)}
      <li data-state={index < activeIndex ? "done" : index === activeIndex ? "active" : "next"}>
        <span class="dot" aria-hidden="true"></span>
        <span>{stage.label}</span>
      </li>
    {/each}
  </ol>
</section>

<style>
  .trail-map {
    display: grid;
    grid-template-columns: minmax(180px, 0.8fr) minmax(320px, 1.2fr);
    align-items: center;
    gap: 22px;
    margin: 0 0 18px;
    padding: 14px 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
  }

  .trail-copy { display: grid; gap: 2px; min-width: 0; }
  .trail-copy > span {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .trail-copy strong { color: var(--mdx-text-primary); font-size: var(--mdx-text-sm); }
  .trail-copy p { margin: 0; color: var(--mdx-text-secondary); font-size: var(--mdx-text-xs); line-height: 1.4; }

  ol {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    position: relative;
    display: grid;
    justify-items: center;
    gap: 6px;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }
  li::before {
    content: "";
    position: absolute;
    top: 5px;
    left: -50%;
    width: 100%;
    height: 1px;
    background: var(--mdx-border-default);
  }
  li:first-child::before { display: none; }
  .dot {
    z-index: 1;
    width: 11px;
    height: 11px;
    border: 2px solid var(--mdx-border-default);
    border-radius: 50%;
    background: var(--mdx-surface-raised);
  }
  li[data-state="done"] { color: var(--mdx-text-secondary); }
  li[data-state="done"] .dot { border-color: var(--mdx-accent-success); background: var(--mdx-accent-success); }
  li[data-state="done"]::before,
  li[data-state="active"]::before { background: var(--mdx-accent-success); }
  li[data-state="active"] { color: var(--mdx-text-primary); font-weight: 700; }
  li[data-state="active"] .dot { border-color: var(--mdx-accent-primary); background: var(--mdx-accent-primary); box-shadow: 0 0 0 4px color-mix(in srgb, var(--mdx-accent-primary) 16%, transparent); }

  @media (max-width: 680px) {
    .trail-map { grid-template-columns: 1fr; }
  }
</style>
