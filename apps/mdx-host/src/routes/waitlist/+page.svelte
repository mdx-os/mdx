<script>
  import { enhance } from "$app/forms";

  let { form } = $props();
  let sending = $state(false);
</script>

<svelte:head><title>Join the beta waitlist - MDx</title></svelte:head>

<main class="waitlist">
  <a class="brand" href="/landing" aria-label="MDx home">MD<span>x</span></a>

  {#if form?.success}
    <section class="card done" aria-live="polite">
      <h1>You're on the list{form.firstName ? `, ${form.firstName}` : ""}.</h1>
      <p>
        Invites go out in small waves so every beta member gets real attention.
        When your spot opens, your invite lands in your inbox - from a human,
        not a robot.
      </p>
      {#if form.receiptId}
        <p class="quiet">
          Your request reference is <code>{form.receiptId}</code>. Keep it if you need help before your account is active.
        </p>
      {/if}
      <p class="quiet"><a href="/redeem">Already have an invite? Redeem it &rarr;</a></p>
    </section>
  {:else}
    <h1>Get into the beta</h1>
    <p class="sub">
      MDx is opening in small waves. Tell us who you are and what you'd build,
      and we'll email your invite when your spot opens.
    </p>

    <form
      class="card"
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
        <input type="text" name="name" required placeholder="Alex Rivera" autocomplete="name" value={form?.values?.name ?? ""} maxlength="120" />
      </label>
      <label>
        Work email
        <input type="email" name="email" required placeholder="you@company.com" autocomplete="email" value={form?.values?.email ?? ""} />
      </label>
      <label>
        What's closest to your role?
        <select name="role" value={form?.values?.role ?? "engineer"}>
          <option value="engineer">Engineer</option>
          <option value="eng_lead">Engineering lead</option>
          <option value="founder">Founder</option>
          <option value="other">Something else</option>
        </select>
      </label>
      <label>
        What would you build first?
        <input type="text" name="goal" placeholder="e.g. clear our flaky-test backlog" maxlength="280" value={form?.values?.goal ?? ""} />
      </label>
      <label>
        How did you hear about MDx? <span class="optional">(optional)</span>
        <input type="text" name="referral" maxlength="160" value={form?.values?.referral ?? ""} />
      </label>
      <label class="consent">
        <input type="checkbox" name="consent" required />
        <span>Email me my invite and beta updates. Nothing else, no sharing.</span>
      </label>
      {#if form?.error}<p class="flash" role="alert">{form.error}</p>{/if}
      <button type="submit" class="go" disabled={sending}>
        {sending ? "Saving your spot..." : "Request an invite"}
      </button>
      <p class="privacy">
        We keep your name and email so a human can send your invite - that's it.
        The product's own governed record keeps only a hash of your address.
      </p>
      <p class="quiet"><a href="/redeem">Already have an invite? Redeem it &rarr;</a></p>
    </form>
  {/if}
</main>

<style>
  .waitlist { max-width: 560px; margin: 0 auto; padding: 48px 24px 80px; }
  .brand {
    display: inline-block;
    margin-bottom: 34px;
    font-family: var(--mdx-font-display);
    font-size: 22px;
    font-weight: 900;
    letter-spacing: -0.03em;
    color: var(--mdx-text-primary);
    text-decoration: none;
  }
  .brand span { color: var(--mdx-accent-primary); }
  h1 { font-size: 27px; margin: 0 0 8px; letter-spacing: -0.01em; }
  .sub { margin: 0 0 24px; font-size: 15px; color: var(--mdx-text-secondary); line-height: 1.6; }
  .card { display: grid; gap: 16px; border: 1px solid var(--mdx-border-subtle); border-radius: 16px; background: var(--mdx-surface-base); padding: 24px; }
  label { display: grid; gap: 6px; font-size: 13px; font-weight: 600; color: var(--mdx-text-primary); }
  input[type="text"], input[type="email"], select {
    font: inherit; font-weight: 400; padding: 10px 12px; border-radius: 9px;
    border: 1px solid var(--mdx-border-subtle); background: var(--mdx-surface-raised); color: var(--mdx-text-primary);
  }
  .optional { font-weight: 400; color: var(--mdx-text-muted); }
  .consent { grid-template-columns: auto 1fr; display: grid; align-items: start; gap: 9px; font-weight: 400; font-size: 13px; color: var(--mdx-text-secondary); }
  .consent input { margin-top: 2px; }
  .go { font: inherit; font-weight: 700; padding: 13px; border: none; border-radius: 999px; background: var(--mdx-accent-primary); color: var(--mdx-on-accent); cursor: pointer; }
  .go:disabled { opacity: 0.55; cursor: default; }
  .flash { margin: 0; font-size: 13px; color: var(--mdx-accent-error); }
  .privacy { margin: 0; font-size: 12px; color: var(--mdx-text-muted); line-height: 1.5; }
  .done h1 { margin: 0 0 8px; }
  .done p { margin: 0 0 10px; font-size: 14px; color: var(--mdx-text-secondary); line-height: 1.6; }
  .quiet { margin: 0; font-size: 12.5px; }
  .quiet a { color: var(--mdx-text-secondary); }
  code { font-family: var(--mdx-font-mono, monospace); font-size: 12px; }
</style>
