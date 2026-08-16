<script>
  import BetaFunnelShell from "../../../lib/marketing/BetaFunnelShell.svelte";

  let { data } = $props();
  const next = $derived(encodeURIComponent(data.next));
</script>

<svelte:head><title>{data.guidance.title} - MDx</title></svelte:head>

<BetaFunnelShell title={data.guidance.title} intro={data.guidance.intro}>
  <section class="signin-card beta-panel">
    {#if data.notice}<p class="notice" role="alert">{data.notice}</p>{/if}
    <div class="choices">
      <a class="choice google" href={`/auth/login?provider=google&next=${next}`}>Continue with Google</a>
      <a class="choice apple" href={`/auth/login?provider=apple&next=${next}`}>
        <span aria-hidden="true">&#63743;</span> Continue with Apple
      </a>
    </div>
    <p class="privacy">{data.guidance.identity}</p>
  </section>
</BetaFunnelShell>

<style>
  .signin-card {
    padding: clamp(1.35rem, 4vw, 1.75rem);
  }

  .choices {
    display: grid;
    gap: 0.7rem;
  }

  .notice {
    margin: 0 0 1rem;
    padding: 0.8rem 0.9rem;
    border: 1px solid color-mix(in srgb, var(--beta-accent) 35%, var(--beta-line));
    border-radius: 0.75rem;
    background: color-mix(in srgb, var(--beta-accent) 9%, transparent);
    color: var(--beta-muted);
    font-size: 0.8rem;
    line-height: 1.5;
  }

  .choice {
    display: flex;
    min-height: 3.1rem;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 0 1rem;
    border-radius: 0.8rem;
    font-weight: 780;
    text-decoration: none;
  }

  .google {
    background: #ffffff;
    color: #15171c;
  }

  .apple {
    border: 1px solid var(--beta-line-strong);
    background: #000000;
    color: #ffffff;
  }

  .apple span {
    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 1.2rem;
  }

  .choice:hover {
    transform: translateY(-1px);
  }

  .choice:focus-visible {
    outline: 3px solid var(--beta-accent);
    outline-offset: 3px;
  }

  .privacy {
    margin: 1.15rem 0 0;
    color: var(--beta-dim);
    font-size: 0.76rem;
    line-height: 1.55;
  }
</style>
