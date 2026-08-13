<script>
  let { data, form } = $props();
  let inviteId = $derived(form?.values?.invite ?? data.invite ?? "");
  const arrivedInvited = $derived(Boolean(data.invite));
  const done = $derived(form?.success ? form.receiptId : "");
  const signInHref = $derived(`/auth/sign-in?next=${encodeURIComponent(`/redeem?invite=${inviteId.trim()}`)}`);
</script>

<svelte:head><title>Redeem your invite - MDx</title></svelte:head>

<main class="redeem">
  <a class="brand" href="/landing" aria-label="MDx home">MD<span>x</span></a>

  {#if done}
    <section class="card done" aria-live="polite">
      <h1>Invite accepted.</h1>
      <p>Your admitted account and beta enrollment now point at the same workspace.</p>
      <a class="go" href="/welcome/beta">Start the short path in &rarr;</a>
      <p class="quiet">Enrollment receipt <code>{done}</code> - kept on the record, like everything in MDx.</p>
    </section>
  {:else}
    {#if arrivedInvited}
      <p class="invited" aria-label="You arrived with an invite">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M22 11.08V12a10 10 0 1 1-5.93-9.14" />
          <polyline points="22 4 12 14.01 9 11.01" />
        </svg>
        You're invited
      </p>
    {/if}
    <h1>Redeem your invite</h1>
    <p class="sub">
      {data.authenticated
        ? "You are signed in. Confirm the invite id to join the beta workspace."
        : "Sign in with the Google or Apple account that received the invite. This binds the invitation to the right person before anything opens."}
    </p>

    <form class="card" method="POST">
      <label>
        Invite id
        <input type="text" name="invite_id" bind:value={inviteId} required placeholder="invite_..." autocomplete="off" spellcheck="false" />
      </label>
      {#if form?.error}<p class="flash" role="alert">{form.error}</p>{/if}
      {#if data.authenticated}
        <button type="submit" class="go" disabled={!inviteId.trim()}>Redeem invite</button>
      {:else}
        <a class="go" class:disabled={!inviteId.trim()} aria-disabled={!inviteId.trim()} href={inviteId.trim() ? signInHref : undefined}>Sign in to redeem</a>
        <p class="quiet">If your workspace is still being activated after sign-in, keep this invite email. We will tell you when to open the same link again.</p>
      {/if}
      <p class="quiet">No invite yet? <a href="/waitlist">Join the waitlist</a>.</p>
    </form>
  {/if}
</main>

<style>
  .redeem { max-width: 500px; margin: 0 auto; padding: 48px 24px 80px; }
  .brand { display: inline-block; margin-bottom: 34px; font-family: var(--mdx-font-display); font-size: 22px; font-weight: 900; letter-spacing: -0.03em; color: var(--mdx-text-primary); text-decoration: none; }
  .brand span { color: var(--mdx-accent-primary); }
  .invited { display: inline-flex; align-items: center; gap: 7px; margin: 0 0 14px; padding: 6px 12px; border-radius: 999px; border: 1px solid color-mix(in srgb, var(--mdx-accent-success, #18875d) 35%, transparent); color: var(--mdx-accent-success, #18875d); font-size: 12.5px; font-weight: 700; }
  .invited svg { width: 14px; height: 14px; }
  h1 { font-size: 26px; margin: 0 0 8px; letter-spacing: -0.01em; }
  .sub { margin: 0 0 22px; font-size: 14.5px; color: var(--mdx-text-secondary); line-height: 1.6; }
  .card { display: grid; gap: 14px; border: 1px solid var(--mdx-border-subtle); border-radius: 16px; background: var(--mdx-surface-base); padding: 24px; }
  label { display: grid; gap: 6px; font-size: 13px; font-weight: 600; }
  input { font: inherit; font-weight: 400; font-family: var(--mdx-font-mono, monospace); font-size: 13px; padding: 11px 12px; border-radius: 9px; border: 1px solid var(--mdx-border-subtle); background: var(--mdx-surface-raised); color: var(--mdx-text-primary); }
  .go { display: inline-block; text-align: center; text-decoration: none; font: inherit; font-weight: 700; padding: 12px 18px; border: none; border-radius: 999px; background: var(--mdx-accent-primary); color: var(--mdx-on-accent); cursor: pointer; }
  .go:disabled, .go.disabled { opacity: 0.55; cursor: default; pointer-events: none; }
  .flash { margin: 0; font-size: 13px; color: var(--mdx-accent-error); }
  .done h1 { margin: 0 0 6px; }
  .done p { margin: 0 0 10px; font-size: 14px; color: var(--mdx-text-secondary); line-height: 1.6; }
  .quiet { margin: 10px 0 0; font-size: 12.5px; color: var(--mdx-text-tertiary); }
  .quiet a { color: var(--mdx-text-secondary); }
  code { font-family: var(--mdx-font-mono, monospace); font-size: 11.5px; }
</style>
