<script>
  import BetaFunnelShell from "../../lib/marketing/BetaFunnelShell.svelte";

  let { data, form } = $props();
  let inviteId = $derived(form?.values?.invite ?? data.invite ?? "");
  const arrivedInvited = $derived(Boolean(data.invite));
  const done = $derived(form?.success ? form.receiptId : "");
  const signInHref = $derived(`/auth/sign-in?next=${encodeURIComponent(`/redeem?invite=${inviteId.trim()}`)}`);
</script>

<svelte:head><title>Redeem your invite - MDx</title></svelte:head>

<BetaFunnelShell
  eyebrow={arrivedInvited ? "Your invitation is ready" : "Private beta"}
  title={done ? "Invite accepted." : "Redeem your invite"}
  intro={done
    ? "Your account and beta enrollment now point at the same MDx workspace."
    : data.authenticated
      ? "You are signed in. Confirm the invite id to join the beta workspace."
      : "Use the Google or Apple account that received the invite so MDx opens the right workspace."}
>
  {#if done}
    <section class="card beta-panel done" aria-live="polite">
      <a class="go beta-primary" href="/welcome/beta">Start the short path in &rarr;</a>
      <p class="quiet beta-quiet">Enrollment receipt <code>{done}</code> - kept on the record, like everything in MDx.</p>
    </section>
  {:else}
    <form class="card beta-panel" method="POST">
      <label>
        Invite id
        <input class="beta-field" type="text" name="invite_id" bind:value={inviteId} required placeholder="invite_..." autocomplete="off" spellcheck="false" />
      </label>
      {#if form?.error}<p class="flash beta-error" role="alert">{form.error}</p>{/if}
      {#if data.authenticated}
        <button type="submit" class="go beta-primary" disabled={!inviteId.trim()}>Redeem invite</button>
      {:else}
        <a class="go beta-primary" class:disabled={!inviteId.trim()} aria-disabled={!inviteId.trim()} href={inviteId.trim() ? signInHref : undefined}>Sign in to redeem</a>
        <p class="quiet beta-quiet">If your workspace is still being activated after sign-in, keep this invite email. We will tell you when to open the same link again.</p>
      {/if}
      <p class="quiet beta-quiet">No invite yet? <a href="/waitlist">Join the waitlist</a>.</p>
    </form>
  {/if}
</BetaFunnelShell>

<style>
  .card {
    display: grid;
    gap: 0.95rem;
    padding: clamp(1.35rem, 4vw, 1.75rem);
  }

  label {
    display: grid;
    gap: 0.45rem;
    color: var(--beta-text);
    font-size: 0.82rem;
    font-weight: 700;
  }

  input {
    font-family: var(--mdx-font-mono, monospace);
    font-size: 0.8rem;
    font-weight: 450;
  }

  .go {
    display: inline-flex;
    min-height: 3rem;
    align-items: center;
    justify-content: center;
    padding: 0.75rem 1rem;
    border-radius: 999px;
    cursor: pointer;
    font: inherit;
    font-weight: 800;
    text-align: center;
    text-decoration: none;
  }

  .go:disabled,
  .go.disabled {
    cursor: default;
    opacity: 0.55;
    pointer-events: none;
  }

  .quiet a {
    color: var(--beta-muted);
    text-underline-offset: 3px;
  }

  code {
    color: var(--beta-text);
    font-family: var(--mdx-font-mono, monospace);
    font-size: 0.72rem;
  }
</style>
