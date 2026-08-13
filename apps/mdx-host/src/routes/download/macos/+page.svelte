<script>
  let { data } = $props();
</script>

<svelte:head>
  <title>Get the MDx apps</title>
  <meta name="robots" content="noindex,nofollow" />
</svelte:head>

<main class="download-shell">
  <p class="eyebrow">MDx private beta</p>
  <h1>Get MDx</h1>
  {#if data.available}
    <p class="lede">{data.appHandoff.continuity}</p>
    <section class="app-card" aria-labelledby="mac-title">
      <h2 id="mac-title">MDx for Mac</h2>
      <p>Signed by MDx, notarized by Apple, and ready to install.</p>
      <a class="download" href={data.downloadUrl}>Download MDx.zip</a>
      <ol>
        {#each data.appHandoff.macSteps as step (step)}<li>{step}</li>{/each}
      </ol>
      <p class="note">Version {data.manifest.version} ({data.manifest.build}). The download link expires shortly; refresh this page if it does.</p>
    </section>
    <section class="app-card" aria-labelledby="iphone-title">
      <h2 id="iphone-title">MDx Anywhere</h2>
      <p>{data.appHandoff.iphone}</p>
      <a class="secondary" href="https://apps.apple.com/app/testflight/id899247664" target="_blank" rel="noopener">Get TestFlight</a>
      <p class="note">{data.appHandoff.iphoneNote}</p>
    </section>
    <a class="secondary" href="/welcome/beta">Continue to your first-session guide</a>
  {:else}
    <p class="lede">No notarized canary is published yet. Your access is ready, and this page will light up after the founder release passes Apple notarization.</p>
    <a class="secondary" href="/">Back to MDx</a>
  {/if}
</main>

<style>
  .download-shell { max-width: 48rem; margin: 0 auto; padding: 5rem 1.5rem; }
  .eyebrow { margin: 0 0 .75rem; color: var(--mdx-text-tertiary); text-transform: uppercase; font-size: var(--mdx-text-sm); }
  h1 { margin: 0; font-size: clamp(2.7rem, 7vw, 5rem); color: var(--mdx-text-primary); }
  .lede { margin: 1.25rem 0 2rem; color: var(--mdx-text-secondary); font-size: 1.15rem; line-height: 1.65; }
  .app-card { margin: 1rem 0; padding: 1.4rem; border: 1px solid var(--mdx-border); border-radius: 1rem; background: var(--mdx-surface-raised); }
  h2 { margin: 0; }
  ol { line-height: 1.65; }
  .download, .secondary { display: inline-flex; padding: .85rem 1.2rem; border-radius: .75rem; text-decoration: none; font-weight: 700; }
  .download { background: var(--mdx-accent, #6d5dfc); color: white; }
  .secondary { border: 1px solid var(--mdx-border); color: var(--mdx-text-primary); }
  .note { color: var(--mdx-text-tertiary); font-size: .9rem; }
</style>
