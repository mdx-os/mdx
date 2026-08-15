<script>
  import PublicNav from "./PublicNav.svelte";

  let {
    eyebrow = "Private beta",
    title,
    intro = "",
    active = "",
    width = "compact",
    children
  } = $props();
</script>

<div class="beta-page">
  <PublicNav {active} />

  <main class="beta-stage" class:wide={width === "wide"}>
    <header class="beta-heading">
      <p class="eyebrow">{eyebrow}</p>
      <h1>{title}</h1>
      {#if intro}<p class="intro">{intro}</p>{/if}
    </header>

    <div class="beta-content">
      {@render children?.()}
    </div>
  </main>
</div>

<style>
  .beta-page {
    --beta-bg: #08080a;
    --beta-panel: #101014;
    --beta-field: #0c0c10;
    --beta-line: #2a2a31;
    --beta-line-strong: #3a3a43;
    --beta-text: #f4f4f6;
    --beta-muted: #a0a0aa;
    --beta-dim: #71717b;
    --beta-accent: #6f8fff;
    --beta-accent-strong: #5378ff;
    --beta-error: #ff8278;
    --beta-on-accent: #ffffff;

    min-height: 100dvh;
    color: var(--beta-text);
    background:
      radial-gradient(circle at 50% 9%, rgba(83, 120, 255, 0.09), transparent 28rem),
      var(--beta-bg);
    font-family: var(--mdx-font-body);
  }

  .beta-stage {
    width: min(100%, 610px);
    margin: 0 auto;
    padding: clamp(4.5rem, 10vw, 7.5rem) 1.5rem clamp(5rem, 11vw, 8rem);
  }

  .beta-stage.wide {
    width: min(100%, 920px);
  }

  .beta-heading {
    max-width: 700px;
    margin-bottom: 2rem;
  }

  .eyebrow {
    margin: 0 0 1rem;
    color: var(--beta-accent);
    font-family: var(--mdx-font-mono);
    font-size: 0.7rem;
    font-weight: 800;
    letter-spacing: 0.16em;
    text-transform: uppercase;
  }

  h1 {
    margin: 0;
    color: var(--beta-text);
    font-family: var(--mdx-font-display);
    font-size: clamp(2.75rem, 7vw, 4.7rem);
    font-weight: 900;
    letter-spacing: -0.06em;
    line-height: 0.98;
  }

  .intro {
    max-width: 680px;
    margin: 1.25rem 0 0;
    color: var(--beta-muted);
    font-size: clamp(0.98rem, 2vw, 1.08rem);
    line-height: 1.68;
  }

  .beta-content {
    min-width: 0;
  }

  .beta-page :global(.beta-panel) {
    border: 1px solid var(--beta-line);
    border-radius: 1.25rem;
    background: color-mix(in srgb, var(--beta-panel) 96%, transparent);
    box-shadow: 0 24px 70px rgba(0, 0, 0, 0.22);
  }

  .beta-page :global(.beta-field) {
    min-height: 2.8rem;
    padding: 0.7rem 0.8rem;
    border: 1px solid var(--beta-line);
    border-radius: 0.7rem;
    outline: none;
    background: var(--beta-field);
    color: var(--beta-text);
  }

  .beta-page :global(.beta-field:focus) {
    border-color: var(--beta-accent);
    box-shadow: 0 0 0 3px rgba(111, 143, 255, 0.16);
  }

  .beta-page :global(.beta-field::placeholder) {
    color: var(--beta-dim);
  }

  .beta-page :global(.beta-primary) {
    border: 0;
    background: var(--beta-accent-strong);
    color: var(--beta-on-accent);
  }

  .beta-page :global(.beta-primary:hover) {
    background: var(--beta-accent);
  }

  .beta-page :global(.beta-secondary) {
    border: 1px solid var(--beta-line-strong);
    color: var(--beta-text);
  }

  .beta-page :global(.beta-secondary:hover) {
    border-color: var(--beta-accent);
  }

  .beta-page :global(.beta-quiet) {
    margin: 0;
    color: var(--beta-dim);
    font-size: 0.76rem;
    line-height: 1.55;
  }

  .beta-page :global(.beta-error) {
    margin: 0;
    color: var(--beta-error);
    font-size: 0.8rem;
  }

  @media (max-width: 560px) {
    .beta-stage {
      padding: 3.75rem 1rem 5rem;
    }

    .beta-heading {
      margin-bottom: 1.5rem;
    }
  }
</style>
