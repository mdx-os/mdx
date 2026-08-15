<script>
  import { enhance } from "$app/forms";
  import BetaFunnelShell from "../../lib/marketing/BetaFunnelShell.svelte";

  let { form } = $props();
  let sending = $state(false);
</script>

<svelte:head><title>Join the beta waitlist - MDx</title></svelte:head>

<BetaFunnelShell
  title={form?.success ? `You're on the list${form.firstName ? `, ${form.firstName}` : ""}.` : "Get into the beta"}
  intro={form?.success
    ? "Invites go out in small waves so every beta member gets real attention."
    : "Tell us who you are and what you would build. We will email your invite when your spot opens."}
>
  {#if form?.success}
    <section class="card beta-panel done" aria-live="polite">
      <p>
        When your spot opens, your invite lands in your inbox from a human who
        can help you get started.
      </p>
      {#if form.receiptId}
        <p class="quiet beta-quiet">
          Your request reference is <code>{form.receiptId}</code>. Keep it if you need help before your account is active.
        </p>
      {/if}
      <p class="quiet beta-quiet"><a href="/redeem">Already have an invite? Redeem it &rarr;</a></p>
    </section>
  {:else}
    <form
      class="card beta-panel"
      method="POST"
      use:enhance={() => {
        sending = true;
        return async ({ update }) => {
          sending = false;
          await update();
        };
      }}
    >
      <label>
        Name
        <input class="beta-field" type="text" name="name" required placeholder="Alex Rivera" autocomplete="name" value={form?.values?.name ?? ""} maxlength="120" />
      </label>
      <label>
        Work email
        <input class="beta-field" type="email" name="email" required placeholder="you@company.com" autocomplete="email" value={form?.values?.email ?? ""} />
      </label>
      <label>
        What's closest to your role?
        <select class="beta-field" name="role" value={form?.values?.role ?? "engineer"}>
          <option value="engineer">Engineer</option>
          <option value="eng_lead">Engineering lead</option>
          <option value="founder">Founder</option>
          <option value="other">Something else</option>
        </select>
      </label>
      <label>
        What would you build first?
        <input class="beta-field" type="text" name="goal" placeholder="e.g. clear our flaky-test backlog" maxlength="280" value={form?.values?.goal ?? ""} />
      </label>
      <label class="consent">
        <input type="checkbox" name="consent" required />
        <span>Email me my invite and beta updates. Nothing else, no sharing.</span>
      </label>
      {#if form?.error}<p class="flash beta-error" role="alert">{form.error}</p>{/if}
      <button type="submit" class="go beta-primary" disabled={sending}>
        {sending ? "Saving your spot..." : "Request an invite"}
      </button>
      <p class="privacy">
        We keep your name and email so a human can send your invite - that's it.
        The product's own governed record keeps only a hash of your address.
      </p>
      <p class="quiet beta-quiet"><a href="/redeem">Already have an invite? Redeem it &rarr;</a></p>
    </form>
  {/if}
</BetaFunnelShell>

<style>
  .card {
    display: grid;
    gap: 1.05rem;
    padding: clamp(1.35rem, 4vw, 1.75rem);
  }

  label {
    display: grid;
    gap: 0.45rem;
    color: var(--beta-text);
    font-size: 0.82rem;
    font-weight: 700;
  }

  input[type="text"],
  input[type="email"],
  select {
    font: inherit;
    font-weight: 450;
    color-scheme: dark;
  }

  .consent {
    grid-template-columns: auto 1fr;
    align-items: start;
    gap: 0.65rem;
    color: var(--beta-muted);
    font-size: 0.8rem;
    font-weight: 450;
    line-height: 1.5;
  }

  .consent input {
    margin-top: 0.18rem;
    accent-color: var(--beta-accent-strong);
  }

  .go {
    min-height: 3rem;
    padding: 0.75rem 1rem;
    border-radius: 999px;
    cursor: pointer;
    font: inherit;
    font-weight: 800;
  }

  .go:disabled {
    cursor: default;
    opacity: 0.55;
  }

  .privacy {
    margin: 0;
    color: var(--beta-dim);
    font-size: 0.76rem;
    line-height: 1.55;
  }

  .done > p:not(.quiet) {
    margin: 0;
    color: var(--beta-muted);
    font-size: 0.92rem;
    line-height: 1.65;
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
