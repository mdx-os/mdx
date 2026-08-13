<script>
  // The guided "Try MDx" first experience. A short, honest walk: what MDx is,
  // who you are, what you want to do first, and the local-posture truth - then
  // your first real Twin conversation, seeded with your own goal. The few facts
  // are recorded on the record (role, goal, working mode); the chosen role also
  // seeds the "You" profile so Twin's context reflects it from the first reply.
  // No login, no account, no fake users.
  import { createMdxClient } from "@mdx/client";
  import { goto } from "$app/navigation";
  import { loadProfile, saveServerProfile } from "../../../lib/youProfile.js";

  let { data } = $props();

  const client = $derived(createMdxClient({ baseUrl: "/api/kernel", session: data.session }));
  const actor = $derived(data.session?.user_id ?? "local_user");

  const roles = [
    { id: "leadership", label: "Leadership", line: "You set direction." },
    { id: "product", label: "Product", line: "You decide what matters." },
    { id: "design", label: "Design", line: "You shape the experience." },
    { id: "engineering", label: "Engineering", line: "You build it." }
  ];
  const modes = [
    { id: "exploring", label: "Just exploring", line: "Show me what MDx can do." },
    { id: "deciding", label: "Evaluating it", line: "I'm deciding if this fits my team." },
    { id: "hands_on", label: "Hands on", line: "I want to put it to real work now." }
  ];

  let step = $state(0);
  let role = $state("");
  let goal = $state("");
  let mode = $state("");
  let privacyAck = $state(false);
  let busy = $state(false);
  let error = $state(null);

  const total = 4;
  const canContinue = $derived(
    (step === 0) ||
      (step === 1 && role !== "") ||
      (step === 2 && mode !== "") ||
      (step === 3 && privacyAck)
  );

  function next() {
    error = null;
    if (step < total - 1) step += 1;
  }
  function back() {
    error = null;
    if (step > 0) step -= 1;
  }

  // The first thing Twin hears: the operator's own goal, or a role-shaped opener
  // if they did not write one. Twin opens with this in the composer.
  function seededPrompt() {
    const written = goal.trim();
    if (written.length > 0) return written;
    const opener = {
      leadership: "Help me get a clear read on what my team should focus on next.",
      product: "Help me think through what matters most to build next.",
      design: "Help me pressure-test an experience idea I'm carrying.",
      engineering: "Help me think through a problem I'm about to build."
    };
    return opener[role] ?? "Help me think something through.";
  }

  async function finish() {
    if (busy || !privacyAck) return;
    busy = true;
    error = null;
    try {
      const packet = await client.write(
        "/install/first-run-profiles.json",
        {
          role,
          goal: goal.trim(),
          working_mode: mode,
          privacy_ack: true,
          actor_id: actor
        },
        { receiptIntent: "first_run_profile" }
      );
      if (!packet?.recorded) {
        error = packet?.reason ?? "That did not record. Try again.";
        busy = false;
        return;
      }
      // Seed the "You" profile so Twin's context reflects the chosen role from
      // the first reply. Local-only, best-effort; never blocks the payoff.
      try {
        const profile = loadProfile();
        profile.role = role;
        await saveServerProfile(profile);
      } catch (err) {
        // Profile persistence is best effort; the first-run receipt still recorded.
      }
      await goto(`/twin?ask=${encodeURIComponent(seededPrompt())}&send=1`);
    } catch (err) {
      error = "Nothing was recorded - try again.";
      busy = false;
    }
  }
</script>

<section class="try">
  <header>
    <p class="eyebrow">Try MDx</p>
    <div class="dots" aria-hidden="true">
      {#each Array(total) as _, i}
        <span class="dot" class:on={i <= step}></span>
      {/each}
    </div>
  </header>

  {#if step === 0}
    <div class="card">
      <h1>Welcome{data.ownerName ? `, ${data.ownerName}` : ""}.</h1>
      <p class="lede">
        MDx is a workspace where you and AI do real work together, and every step is on the
        record. {data.modelName || "Your model"} is connected and ready.
      </p>
      <ul class="facts">
        <li><strong>It runs here.</strong> Everything lives on this machine - nothing in the cloud.</li>
        <li><strong>It shows its work.</strong> Every action leaves a receipt you can read.</li>
        <li><strong>Start small.</strong> In a minute you'll be talking with Twin about your own work.</li>
      </ul>
    </div>
  {:else if step === 1}
    <div class="card">
      <h1>What's your work?</h1>
      <p class="lede">This shapes how Twin talks with you. You can change it later in You.</p>
      <div class="choices">
        {#each roles as r}
          <button type="button" class="choice" class:sel={role === r.id} onclick={() => (role = r.id)}>
            <strong>{r.label}</strong>
            <span>{r.line}</span>
          </button>
        {/each}
      </div>
    </div>
  {:else if step === 2}
    <div class="card">
      <h1>What do you want to do first?</h1>
      <p class="lede">A line about what's on your mind - Twin will open with it. Optional.</p>
      <textarea
        bind:value={goal}
        rows="3"
        maxlength="400"
        placeholder="e.g. Help me shape next quarter's plan"
        aria-label="What do you want to do first"
      ></textarea>
      <p class="sub">How do you want to start?</p>
      <div class="choices">
        {#each modes as m}
          <button type="button" class="choice" class:sel={mode === m.id} onclick={() => (mode = m.id)}>
            <strong>{m.label}</strong>
            <span>{m.line}</span>
          </button>
        {/each}
      </div>
    </div>
  {:else}
    <div class="card">
      <h1>One thing before you start.</h1>
      <ul class="facts">
        <li><strong>This is local.</strong> Your work and your data stay on this machine.</li>
        <li><strong>Some things need a key.</strong> Live answers use the model you connected; you can add more later.</li>
        <li><strong>Not production yet.</strong> This is a working local install, not a deployed system. Preparing for production is its own path.</li>
      </ul>
      <label class="ack">
        <input type="checkbox" bind:checked={privacyAck} />
        <span>I understand MDx runs locally and my data stays on this machine.</span>
      </label>
    </div>
  {/if}

  {#if error}<p class="error">{error}</p>{/if}

  <div class="nav">
    {#if step > 0}
      <button type="button" class="ghost" onclick={back} disabled={busy}>Back</button>
    {:else}
      <a class="ghost" href="/welcome/path">All paths</a>
    {/if}
    {#if step < total - 1}
      <button type="button" class="primary" onclick={next} disabled={!canContinue}>Continue</button>
    {:else}
      <button type="button" class="primary" onclick={finish} disabled={!canContinue || busy}>
        {busy ? "Starting..." : "Start with Twin →"}
      </button>
    {/if}
  </div>
</section>

<style>
  .try {
    max-width: 40rem;
    margin: 0 auto;
    padding: 3.5rem 1.5rem 5rem;
    display: flex;
    flex-direction: column;
    gap: 1.75rem;
  }
  header {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .eyebrow {
    margin: 0;
    font-size: var(--mdx-text-sm);
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--mdx-text-tertiary);
  }
  .dots {
    display: flex;
    gap: 0.4rem;
  }
  .dot {
    width: 2rem;
    height: 0.25rem;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-sunken);
  }
  .dot.on {
    background: var(--mdx-accent-primary);
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
  }
  h1 {
    margin: 0;
    font-family: var(--mdx-font-display);
    color: var(--mdx-text-primary);
  }
  .lede {
    margin: 0;
    color: var(--mdx-text-secondary);
  }
  .sub {
    margin: 0.5rem 0 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
  }
  .facts {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .facts li {
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
  }
  .facts strong {
    color: var(--mdx-text-primary);
  }
  .choices {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .choice {
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    padding: 0.85rem 1rem;
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    cursor: pointer;
    font: inherit;
  }
  .choice strong {
    color: var(--mdx-text-primary);
  }
  .choice span {
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
  }
  .choice.sel {
    border-color: var(--mdx-accent-primary);
    background: var(--mdx-surface-base);
  }
  textarea {
    width: 100%;
    padding: 0.6rem 0.75rem;
    background: var(--mdx-surface-base);
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    color: var(--mdx-text-primary);
    font: inherit;
    resize: vertical;
  }
  textarea:focus {
    outline: none;
    border-color: var(--mdx-accent-primary);
  }
  .ack {
    display: flex;
    align-items: flex-start;
    gap: 0.6rem;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    cursor: pointer;
  }
  .ack input {
    margin-top: 0.15rem;
  }
  .error {
    margin: 0;
    color: var(--mdx-tone-danger-text);
    font-size: var(--mdx-text-sm);
  }
  .nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }
  .primary {
    padding: 0.6rem 1.25rem;
    background: var(--mdx-accent-primary);
    color: var(--mdx-surface-base);
    border: none;
    border-radius: var(--mdx-radius-pill);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .ghost {
    padding: 0.6rem 0.5rem;
    background: none;
    border: none;
    color: var(--mdx-text-tertiary);
    font: inherit;
    cursor: pointer;
    text-decoration: none;
  }
  .ghost:hover {
    color: var(--mdx-text-secondary);
  }
</style>
