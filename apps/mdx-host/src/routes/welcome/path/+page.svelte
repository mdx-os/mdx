<script>
  // The activation router. One honest question - "What brings you here?" - with
  // three answers. The choice MDx cannot detect (who you are, what you came to
  // do) is the one thing we ask; it records a governed receipt and routes you to
  // the right surface. This is a one-time chooser, not a home: after you pick,
  // Twin is home. Each card shows the honest state of its door today.
  import { createMdxClient } from "@mdx/client";
  import { goto } from "$app/navigation";

  let { data } = $props();

  const client = $derived(createMdxClient({ baseUrl: "/api/kernel", session: data.session }));
  const actor = $derived(data.session?.user_id ?? "local_user");

  let busy = $state(false);
  let error = $state(null);

  const statusLabel = {
    ready: "Ready now",
    needs_action: "Needs a step",
    blocked: "Blocked",
    later: "Coming soon"
  };

  async function choose(track) {
    if (busy) return;
    busy = true;
    error = null;
    try {
      const packet = await client.write(
        "/install/track-choices.json",
        { track, actor_id: actor },
        { receiptIntent: "setup_track_choice" }
      );
      if (packet?.chosen && packet.destination) {
        // Recorded on the record; go where this track leads. Twin is home from
        // here on - the front door will not show this chooser again.
        await goto(packet.destination);
      } else {
        error = packet?.reason ?? "That did not record. Try again.";
        busy = false;
      }
    } catch (err) {
      error = "Nothing was recorded - try again.";
      busy = false;
    }
  }
</script>

<section class="router">
  <header>
    <p class="eyebrow">MDx</p>
    <h1>What brings you here?</h1>
    <p class="lede">
      {#if data.ownerName}Welcome, {data.ownerName}. {/if}MDx is running and {data.modelName ||
        "your model"} is connected. Pick where to start - you can change later, and Twin is always home.
    </p>
  </header>

  <ul class="tracks">
    {#each data.tracks as track}
      <li class="track" class:later={track.status === "later"}>
        <button
          type="button"
          onclick={() => choose(track.id)}
          disabled={busy}
          aria-label={`Choose ${track.label}`}
        >
          <div class="track-head">
            <strong>{track.label}</strong>
            <span class="badge" class:soft={track.status !== "ready"}>
              {statusLabel[track.status] ?? track.status}
            </span>
          </div>
          <p class="who">{track.who}</p>
          <p class="note">{track.note}</p>
          <span class="go">{busy ? "..." : "Start here →"}</span>
        </button>
      </li>
    {/each}
  </ul>

  {#if error}<p class="error">{error}</p>{/if}

</section>

<style>
  .router {
    max-width: 44rem;
    margin: 0 auto;
    padding: 4rem 1.5rem 5rem;
    display: flex;
    flex-direction: column;
    gap: 2rem;
  }
  header {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .eyebrow {
    margin: 0;
    font-size: var(--mdx-text-sm);
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--mdx-text-tertiary);
  }
  h1 {
    margin: 0;
    font-family: var(--mdx-font-display);
    color: var(--mdx-text-primary);
  }
  .lede {
    margin: 0;
    color: var(--mdx-text-secondary);
    max-width: 34rem;
  }
  .tracks {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
  }
  .track button {
    width: 100%;
    text-align: left;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding: 1.25rem 1.4rem;
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    cursor: pointer;
    font: inherit;
    transition: border-color 0.12s ease, transform 0.12s ease;
  }
  .track button:hover:not(:disabled) {
    border-color: var(--mdx-accent-primary);
    transform: translateY(-1px);
  }
  .track button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .track.later button {
    background: var(--mdx-surface-base);
  }
  .track-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }
  .track-head strong {
    color: var(--mdx-text-primary);
    font-size: var(--mdx-text-lg);
  }
  .badge {
    font-size: var(--mdx-text-xs);
    font-weight: 600;
    padding: 0.15rem 0.55rem;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-tone-ok-surface, var(--mdx-surface-sunken));
    color: var(--mdx-tone-ok-text);
    white-space: nowrap;
  }
  .badge.soft {
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-tertiary);
  }
  .who {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
  }
  .note {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
  }
  .go {
    margin-top: 0.35rem;
    color: var(--mdx-accent-primary);
    font-weight: 600;
    font-size: var(--mdx-text-sm);
  }
  .error {
    margin: 0;
    color: var(--mdx-tone-danger-text);
    font-size: var(--mdx-text-sm);
  }
</style>
