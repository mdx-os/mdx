<script>
  let { data, form } = $props();
  const firstName = $derived(data.identity?.name ? data.identity.name.split(" ")[0] : null);
</script>

<svelte:head><title>Almost in - MDx</title></svelte:head>

<main class="pending">
  <div class="mark" aria-hidden="true">MD<span>x</span></div>

  {#if data.identity}
    <h1>{firstName ? `You're almost in, ${firstName}.` : "You're almost in."}</h1>
    <p class="sub">
      You're signed in as <strong>{data.identity.email}</strong>, and your workspace
      isn't active yet. That's the last step, and it's ours, not yours.
    </p>
    <section class="card">
      <h2>What happens now</h2>
      <ol>
        <li><strong>We activate your workspace.</strong> A human turns on each beta account - it usually happens within a day.</li>
        <li><strong>We reply to your invite thread</strong> when the receipt-backed account is ready.</li>
        <li><strong>Check access here.</strong> This page securely refreshes your sign-in and continues the same invite when your workspace is ready.</li>
      </ol>
    </section>
    <form method="POST" class="check-access">
      <input type="hidden" name="next" value={form?.next ?? data.next} />
      <button type="submit" class="go">Check access</button>
      {#if form?.pending}<p class="quiet" role="status">Not active yet. You can leave this page open and check again after our reply.</p>{/if}
      {#if form?.error}<p class="quiet error" role="alert">{form.error}</p>{/if}
    </form>
    <p class="quiet">
      Been waiting longer than a day? Reply to your invite email and a human will
      look right away.
    </p>
    <form method="POST" action="/auth/logout">
      <button type="submit" class="signout">Sign out</button>
    </form>
  {:else}
    <h1>The beta is invite-only for now.</h1>
    <p class="sub">
      If you have an invite, redeem it and we'll get your workspace ready. If not,
      join the waitlist and a human will email you when your spot opens.
    </p>
    <div class="acts">
      <a class="go" href="/redeem">Redeem an invite</a>
      <a class="alt" href="/waitlist">Join the waitlist</a>
    </div>
  {/if}
</main>

<style>
  .pending {
    max-width: 520px;
    margin: 0 auto;
    padding: 10vh 24px 80px;
    text-align: center;
  }
  .mark {
    font-family: var(--mdx-font-display);
    font-size: 40px;
    font-weight: 900;
    letter-spacing: -0.04em;
    color: var(--mdx-text-primary);
    margin-bottom: 28px;
  }
  .mark span { color: var(--mdx-accent-primary); }
  h1 { font-size: 26px; margin: 0 0 10px; }
  .sub { margin: 0 0 24px; font-size: 15px; color: var(--mdx-text-secondary); line-height: 1.6; }
  .card {
    text-align: left;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: 14px;
    background: var(--mdx-surface-raised);
    padding: 20px 22px;
    margin-bottom: 18px;
  }
  .card h2 { margin: 0 0 10px; font-size: 14px; }
  .card ol { margin: 0; padding-left: 18px; display: grid; gap: 10px; font-size: 13.5px; color: var(--mdx-text-secondary); line-height: 1.55; }
  .card strong { color: var(--mdx-text-primary); }
  .quiet { font-size: 12.5px; color: var(--mdx-text-tertiary); margin: 0 0 26px; }
  .check-access { display: grid; justify-items: center; gap: 10px; margin-bottom: 14px; }
  .check-access .quiet { margin: 0; }
  .error { color: var(--mdx-accent-error); }
  .signout {
    font: inherit;
    font-size: 13px;
    background: none;
    border: 1px solid var(--mdx-border-default);
    color: var(--mdx-text-secondary);
    padding: 8px 18px;
    border-radius: 999px;
    cursor: pointer;
  }
  .acts { display: flex; gap: 10px; justify-content: center; flex-wrap: wrap; }
  .go, .alt {
    display: inline-flex;
    align-items: center;
    min-height: 44px;
    padding: 0 22px;
    border-radius: 999px;
    font-weight: 650;
    text-decoration: none;
  }
  .go { background: var(--mdx-accent-primary); color: var(--mdx-on-accent); }
  .alt { border: 1px solid var(--mdx-border-default); color: var(--mdx-text-primary); }
</style>
