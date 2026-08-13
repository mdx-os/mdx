<script>
  export let id = "";
  export let label = "";
  export let kicker = "";
  export let title = "";
  export let description = "";
</script>

<section class="operator-section" {id} aria-label={label || title}>
  <div class="section-head">
    <p>{kicker}</p>
    <h2>{title}</h2>
    <span>{description}</span>
  </div>
  <slot />
</section>

<style>
  .operator-section {
    margin-top: 28px;
  }

  .section-head {
    display: grid;
    gap: 8px;
    max-width: 820px;
    margin-bottom: 14px;
  }

  p,
  h2,
  span {
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
    font-size: 24px;
    line-height: 1.12;
  }

  span {
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.5;
  }
</style>
