<script>
  export let overview;
  export let eyebrow = "Harness runway";

  $: title = overview?.title ?? "Harness runway";
  $: summary = overview?.summary ?? "";
  $: lanes = overview?.lanes ?? [];
  $: metrics = (overview?.metrics ?? []).slice(0, 3);
  $: callouts = (overview?.callouts ?? []).slice(0, 2);
  $: next = overview?.next;
  $: evidence = overview?.evidence ?? [];
</script>

<section class="harness-runway-strip" aria-label="Harness runway">
  <div class="runway-intro">
    <p>{eyebrow}</p>
    <h2>{title}</h2>
    {#if summary}
      <span>{summary}</span>
    {/if}
    {#if metrics.length > 0}
      <div class="runway-metrics" aria-label="Harness runway proof summary">
        {#each metrics as metric}
          <span data-state={metric.state}>
            <strong>{metric.value}</strong>
            <small>{metric.label}</small>
            <em>{metric.detail}</em>
          </span>
        {/each}
      </div>
    {/if}
  </div>

  <ol class="runway-lanes" aria-label="Harness build path">
    {#each lanes as lane}
      <li data-state={lane.state}>
        <strong>{lane.label}</strong>
        <span>{lane.body}</span>
        <small>{lane.proof}</small>
      </li>
    {/each}
  </ol>

  {#if callouts.length > 0}
    <div class="runway-callouts" aria-label="Harness proof cues">
      {#each callouts as callout}
        <article data-state={callout.state}>
          <p>{callout.label}</p>
          <strong>{callout.title}</strong>
          <span>{callout.body}</span>
          <small>{callout.proof}</small>
        </article>
      {/each}
    </div>
  {/if}

  {#if next}
    <div class="runway-next">
      <p>{next.label}</p>
      <h2>{next.title}</h2>
      <span>{next.body}</span>
      {#if evidence.length > 0}
        <small>{evidence.join("; ")}</small>
      {/if}
    </div>
  {/if}
</section>

<style>
  .harness-runway-strip {
    display: grid;
    grid-template-areas:
      "intro lanes"
      "callouts lanes"
      "next lanes";
    grid-template-columns: minmax(280px, 0.72fr) minmax(0, 1.48fr);
    gap: 12px;
    margin-top: 20px;
    border: 1px solid var(--mdx-border-soft);
    border-radius: var(--mdx-radius-panel);
    padding: 14px;
    background:
      linear-gradient(145deg, rgba(var(--mdx-surface-raised-rgb), 0.82), rgba(var(--mdx-surface-rgb), 0.74)),
      var(--mdx-bg);
  }

  .runway-intro,
  .runway-callouts,
  .runway-next {
    display: grid;
    align-content: start;
    gap: 10px;
    min-height: 100%;
    padding: 14px;
  }

  .runway-intro {
    grid-area: intro;
  }

  .runway-metrics {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
    margin-top: 4px;
  }

  .runway-metrics span {
    display: grid;
    gap: 4px;
    min-width: 0;
    border: 1px solid var(--mdx-border-chip);
    border-radius: var(--mdx-radius-panel);
    padding: 10px;
    background: rgba(var(--mdx-bg-rgb), 0.24);
  }

  .runway-metrics span[data-state="local"] {
    border-color: color-mix(in srgb, var(--mdx-success-green) 28%, var(--mdx-border-chip));
  }

  .runway-metrics span[data-state="held-back"] {
    border-color: color-mix(in srgb, var(--mdx-warning-amber) 34%, var(--mdx-border-chip));
  }

  .runway-metrics strong {
    overflow-wrap: anywhere;
    color: var(--mdx-text-primary);
    font-size: 15px;
    line-height: 1.1;
  }

  .runway-metrics small,
  .runway-metrics em {
    color: var(--mdx-text-secondary);
    font-size: 11px;
    font-style: normal;
    line-height: 1.2;
  }

  .runway-metrics small {
    color: var(--mdx-text-caption);
    font-weight: 800;
    text-transform: uppercase;
  }

  .runway-callouts {
    grid-area: callouts;
    grid-template-columns: 1fr;
    gap: 8px;
    padding: 0;
  }

  .runway-next {
    grid-area: next;
    border-top: 1px solid var(--mdx-border-soft);
  }

  p,
  h2,
  span,
  small {
    margin: 0;
  }

  p {
    color: var(--mdx-text-caption);
    font-size: 12px;
    font-weight: 800;
    letter-spacing: 0;
    text-transform: uppercase;
  }

  h2 {
    color: var(--mdx-text-primary);
    font-family: var(--mdx-font-display);
    font-size: 28px;
    line-height: 1.12;
  }

  span,
  small {
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.45;
  }

  .runway-lanes {
    grid-area: lanes;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .runway-callouts article {
    display: grid;
    gap: 7px;
    border: 1px solid var(--mdx-border-chip);
    border-radius: var(--mdx-radius-panel);
    padding: 12px;
    background: rgba(var(--mdx-bg-rgb), 0.28);
  }

  .runway-callouts article[data-state="local"] {
    border-color: color-mix(in srgb, var(--mdx-success-green) 30%, var(--mdx-border-chip));
  }

  .runway-callouts article[data-state="held-back"] {
    border-color: color-mix(in srgb, var(--mdx-warning-amber) 36%, var(--mdx-border-chip));
  }

  .runway-callouts article[data-state="next"] {
    border-color: color-mix(in srgb, var(--mdx-focus-blue) 34%, var(--mdx-border-chip));
  }

  .runway-callouts strong {
    color: var(--mdx-text-primary);
    font-size: 13px;
    line-height: 1.25;
  }

  .runway-lanes li {
    position: relative;
    display: grid;
    align-content: start;
    gap: 8px;
    min-height: 180px;
    border: 1px solid var(--mdx-border-chip);
    border-radius: var(--mdx-radius-panel);
    padding: 14px;
    background: rgba(var(--mdx-bg-rgb), 0.32);
  }

  .runway-lanes li::before {
    width: 28px;
    height: 3px;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-success-green);
    content: "";
  }

  .runway-lanes li[data-state="held-back"]::before {
    background: var(--mdx-warning-amber);
  }

  .runway-lanes strong {
    color: var(--mdx-text-primary);
    font-size: 13px;
    line-height: 1.2;
  }

  .runway-lanes small {
    font-size: 12px;
  }

  @media (max-width: 1080px) {
    .harness-runway-strip {
      grid-template-areas:
        "intro"
        "callouts"
        "lanes"
        "next";
      grid-template-columns: 1fr;
    }

    .runway-next {
      border-top: 1px solid var(--mdx-border-soft);
    }
  }

  @media (max-width: 720px) {
    .runway-metrics {
      grid-template-columns: 1fr;
    }

    .runway-lanes {
      grid-template-columns: 1fr;
    }

    h2 {
      font-size: 24px;
    }
  }
</style>
