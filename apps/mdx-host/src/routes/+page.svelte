<script>
  // Home: the launcher. One clean question - "what do you want to build
  // next?" - and one input, the way Claude/Codex and old-MDx Twin open. The
  // composer is the hero and hands work to Twin (which can route to Forge).
  // Notifications are a quiet first-class pill, not a wall; the deeper lists
  // (needs-you, recent work) live one tap away, off the front door.
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { loadProfile } from "../lib/youProfile.js";

  let { data } = $props();

  const needsTotal = $derived(Number(data.needsYou?.total ?? 0));

  // One calm, professional greeting - the name from the profile when set, and a
  // graceful no-name line when it is not. Rendered whatever the profile says;
  // never an invented name. Time-of-day only. Resolved client-side so the first
  // server render stays stable.
  let greetLine = $state("Welcome back.");
  onMount(() => {
    const me = loadProfile()?.name?.trim();
    const hour = new Date().getHours();
    const tod = hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening";
    greetLine = me ? `${tod}, ${me}.` : `${tod}.`;
  });

  let draft = $state("");

  function startSomething() {
    const text = draft.trim();
    goto(text ? `/twin?ask=${encodeURIComponent(text)}` : "/twin");
  }
  // A chip is a one-tap ask: it runs straight into Twin rather than just
  // filling the box and waiting.
  function askStarter(text) {
    goto(`/twin?ask=${encodeURIComponent(text)}`);
  }
</script>

<svelte:head><title>Home - MDx</title></svelte:head>

<section class="home" data-route-state="ready">
  {#if needsTotal > 0}
    <a class="home-bell" href="/needs-you" aria-label="{needsTotal} items need you">
      <span class="home-bell-dot" aria-hidden="true"></span>
      {needsTotal} need you
    </a>
  {/if}

  <div class="home-hero">
    <p class="home-greet">{greetLine}</p>
    <h1 class="home-tagline">What do you wanna <span class="home-hl">build</span> next?</h1>
    <p class="home-sub">It's me, Twin - you know, your helper to just get stuff done. I've got specialist agents ready to help across strategy, coaching, architecture, problem solving, compliance, and coding.</p>

    <div class="home-chips">
      <button type="button" class="home-chip" onclick={() => askStarter("Help me think something through")}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9 18h6"/><path d="M10 22h4"/><path d="M12 2a7 7 0 0 0-4 12.7c.6.5 1 1.2 1 2h6c0-.8.4-1.5 1-2A7 7 0 0 0 12 2Z"/></svg>
        <span>Help me think something through</span>
      </button>
      <button type="button" class="home-chip" onclick={() => askStarter("What needs me right now?")}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M18 8a6 6 0 0 0-12 0c0 7-3 9-3 9h18s-3-2-3-9"/><path d="M13.7 21a2 2 0 0 1-3.4 0"/></svg>
        <span>What needs me right now?</span>
      </button>
      <button type="button" class="home-chip" onclick={() => askStarter("Get a stuck build moving")}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/></svg>
        <span>Get a stuck build moving</span>
      </button>
      <button type="button" class="home-chip" onclick={() => askStarter("Catch me up on what's changed")}>
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 1 0 9-9 9 9 0 0 0-6.4 2.6L3 8"/><path d="M3 4v4h4"/></svg>
        <span>Catch me up on what's changed</span>
      </button>
    </div>

    <div class="home-ask">
      <input
        bind:value={draft}
        type="text"
        placeholder="Ask me anything..."
        onkeydown={(e) => e.key === "Enter" && startSomething()}
      />
      <button type="button" class="mdx-btn primary" onclick={startSomething}>Ask Twin &rarr;</button>
    </div>
  </div>

  <p class="home-boundary">Boundary: {data.boundary} Safe next move: {data.safeNext}</p>
</section>

<style>
  .home {
    position: relative;
    display: grid;
    align-content: start;
    min-height: 100%;
    padding: 4vh 0 48px;
  }

  .home-bell {
    position: absolute;
    top: 0;
    right: 0;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 6px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-full);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight);
    color: var(--mdx-text-secondary);
    font-size: 13px;
    text-decoration: none;
  }
  .home-bell:hover {
    border-color: var(--mdx-border-strong);
    color: var(--mdx-text-primary);
  }
  .home-bell-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--mdx-accent-primary);
  }

  .home-hero {
    display: grid;
    gap: 18px;
    justify-items: center;
    text-align: center;
    max-width: 600px;
    margin: 0 auto;
    padding-top: 16vh;
  }

  .home-greet {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 15px;
  }

  .home-tagline {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: clamp(28px, 3.4vw, 38px);
    font-weight: 650;
    line-height: 1.15;
    letter-spacing: -0.01em;
  }

  .home-sub {
    margin: -8px 0 0;
    max-width: 520px;
    color: var(--mdx-text-muted);
    font-size: 15px;
    line-height: 1.55;
  }

  .home-ask {
    display: flex;
    gap: 8px;
    width: 100%;
    margin-top: 6px;
  }
  .home-ask input {
    flex: 1;
    min-width: 0;
    padding: 18px 20px;
    font-size: 16px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    color: inherit;
    font-family: inherit;
    box-shadow: var(--mdx-edge-highlight);
  }
  .home-ask input:focus {
    outline: none;
    border-color: var(--mdx-accent-primary);
  }
  .home-ask .mdx-btn {
    flex: none;
    padding: 0 22px;
    font-size: 14.5px;
  }

  .home-hl {
    color: var(--mdx-accent-primary);
  }

  /* v1 chip cards: a 2x2 grid of larger, icon-led starters - calmer and more
     inviting than pills. */
  .home-chips {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    width: 100%;
    margin-top: 4px;
  }
  .home-chip {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 14px 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight);
    color: var(--mdx-text-secondary);
    font: inherit;
    font-size: 13.5px;
    text-align: left;
    cursor: pointer;
    transition: border-color var(--mdx-motion-fast) var(--mdx-ease), color var(--mdx-motion-fast) var(--mdx-ease);
  }
  .home-chip svg {
    width: 18px;
    height: 18px;
    flex: none;
    color: var(--mdx-text-muted);
  }
  .home-chip:hover {
    border-color: var(--mdx-border-strong);
    color: var(--mdx-text-primary);
  }
  .home-chip:hover svg {
    color: var(--mdx-accent-primary);
  }
  @media (max-width: 560px) {
    .home-chips {
      grid-template-columns: 1fr;
    }
  }

  /* The surface's boundary + safe-next still ship for screen readers and the
     surface contract, just no visible "more" toggle cluttering the page. */
  .home-boundary {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    border: 0;
  }
</style>
