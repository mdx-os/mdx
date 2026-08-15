<script>
  import BetaFunnelShell from "../../../lib/marketing/BetaFunnelShell.svelte";

  let { data } = $props();
</script>

<svelte:head>
  <title>Get the MDx apps</title>
  <meta name="robots" content="noindex,nofollow" />
</svelte:head>

<BetaFunnelShell
  eyebrow="MDx private beta"
  title="Get MDx"
  intro={data.available
    ? data.appHandoff.continuity
    : "Your access is ready. The Mac download will appear here after the founder release passes Apple notarization."}
  active="downloads"
  width="wide"
>
  {#if data.available}
    <div class="apps-grid">
      <section class="app-card beta-panel mac" aria-labelledby="mac-title">
        <h2 id="mac-title">MDx for Mac</h2>
        <p>Signed by MDx, notarized by Apple, and ready to install.</p>
        <a class="download beta-primary" href={data.downloadUrl}>Download {data.downloadFilename}</a>
        <ol>
          {#each data.macSteps as step (step)}<li>{step}</li>{/each}
        </ol>
        <p class="note">Version {data.manifest.version} ({data.manifest.build}).</p>
      </section>
      <section class="app-card beta-panel iphone" aria-labelledby="iphone-title">
        <h2 id="iphone-title">MDx Anywhere</h2>
        <p>{data.appHandoff.iphone}</p>
        <a class="secondary beta-secondary" href="https://apps.apple.com/app/testflight/id899247664" target="_blank" rel="noopener">Get TestFlight</a>
        <p class="note">{data.appHandoff.iphoneNote}</p>
      </section>
    </div>
    <a class="secondary beta-secondary guide" href="/welcome/beta">Continue to your first-session guide &rarr;</a>
  {:else}
    <a class="secondary beta-secondary" href="/">Back to MDx</a>
  {/if}
</BetaFunnelShell>

<style>
  .apps-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.15fr) minmax(0, 0.85fr);
    gap: 1rem;
  }

  .app-card {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 1rem;
    padding: clamp(1.3rem, 3vw, 1.7rem);
  }

  h2 {
    margin: 0;
    color: var(--beta-text);
    font-family: var(--mdx-font-display);
    font-size: 1.35rem;
    letter-spacing: -0.03em;
  }

  .app-card > p:not(.note) {
    margin: 0;
    color: var(--beta-muted);
    font-size: 0.9rem;
    line-height: 1.6;
  }

  ol {
    display: grid;
    gap: 0.55rem;
    margin: 0;
    padding-left: 1.2rem;
    color: var(--beta-muted);
    font-size: 0.82rem;
    line-height: 1.5;
  }

  .download,
  .secondary {
    display: inline-flex;
    min-height: 2.85rem;
    align-items: center;
    justify-content: center;
    align-self: flex-start;
    padding: 0.75rem 1rem;
    border-radius: 999px;
    font-size: 0.82rem;
    font-weight: 800;
    text-decoration: none;
  }

  .note {
    margin: auto 0 0;
    color: var(--beta-dim);
    font-size: 0.75rem;
    line-height: 1.55;
  }

  .guide {
    margin-top: 1rem;
  }

  @media (max-width: 760px) {
    .apps-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
