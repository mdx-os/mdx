<script>
  import { page } from "$app/state";

  // The onboarding shell: a calm, full-bleed world for the activation journey.
  // The product rail steps aside (root +layout, onWelcome) so setup feels like an
  // arrival, not a settings page. Here we give just a quiet wordmark and an exit.
  let { children } = $props();
  const onPending = $derived(page.url.pathname === "/welcome/pending");
</script>

<div class="welcome-shell">
  <header class="welcome-bar">
    <a class="welcome-brand" href="/" aria-label="MDx home">MD<span>x</span></a>
    <!--
      The always-available door into the product. It is a plain server-rendered
      anchor with data-sveltekit-reload so the click does a full native
      navigation - reliable before hydration and immune to the client-router
      race that used to swallow it. Styled as a visible button, not a muted
      link, so someone who wants to look around now can leave setup at once.
    -->
    {#if !onPending}
      <a class="welcome-exit" href="/twin" data-sveltekit-reload>Skip for now, look around &rarr;</a>
    {/if}
  </header>
  <div class="welcome-stage">
    {@render children()}
  </div>
</div>

<style>
  .welcome-shell {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .welcome-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 22px 32px;
  }
  .welcome-brand {
    font-family: var(--mdx-font-display);
    font-weight: 800;
    font-size: 20px;
    letter-spacing: -0.02em;
    color: var(--mdx-text-primary);
    text-decoration: none;
  }
  .welcome-brand span { color: var(--mdx-accent-primary); }
  .welcome-exit {
    font-size: 13px;
    font-weight: 600;
    color: var(--mdx-text-primary);
    text-decoration: none;
    padding: 9px 16px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-raised);
    transition: border-color 0.12s ease, background 0.12s ease;
  }
  .welcome-exit:hover {
    border-color: var(--mdx-accent-primary);
    background: color-mix(in srgb, var(--mdx-accent-primary) 8%, var(--mdx-surface-raised));
  }
  .welcome-stage {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
</style>
