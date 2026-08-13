<script>
  export let session = { tenant_id: "local_tenant", user_id: "local_user", role: "owner" };
  const routes = [
    ["Observatory", "Receipt-backed operations"],
    ["Concierge", "Grounded answers"],
    ["Status", "Live substrate turn-on"],
    ["Readiness", "App sequencing"],
    ["Threats", "Security evidence"]
  ];
</script>

<main class="mdx-shell">
  <header>
    <p>MDx v2</p>
    <strong>{session.tenant_id} / {session.role}</strong>
  </header>
  <section aria-label="Local read-only surfaces">
    {#each routes as route}
      <article>
        <h2>{route[0]}</h2>
        <p>{route[1]}</p>
      </article>
    {/each}
  </section>
</main>

<style>
  .mdx-shell { font-family: var(--mdx-font-ui); color: var(--mdx-text-primary); background: var(--mdx-bg); min-height: 100vh; padding: 24px; }
  header { display: flex; justify-content: space-between; gap: 16px; align-items: center; border-bottom: 1px solid var(--mdx-border); padding-bottom: 16px; }
  header p { margin: 0; font-size: 14px; text-transform: uppercase; letter-spacing: 0; color: var(--mdx-text-secondary); }
  header strong { font-size: 14px; color: var(--mdx-text-primary); }
  section { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; margin-top: 24px; }
  article { border: 1px solid var(--mdx-border); border-radius: var(--mdx-radius-panel); padding: 14px; background: var(--mdx-surface); }
  h2 { margin: 0 0 6px; font-size: 16px; }
  article p { margin: 0; font-size: 13px; line-height: 1.4; color: var(--mdx-text-secondary); }
</style>
