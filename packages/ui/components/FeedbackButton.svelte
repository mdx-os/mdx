<script>
  // A quiet, low-friction beta feedback affordance. It posts ONLY safe context
  // to the governed feedback route (/api/kernel/feedback/captures.json), which
  // runs the PR #895 safe-context contract before recording anything. It never
  // sends private page or message bodies, prompts, or model outputs: only the
  // surface, the route path, a bounded category, a short note the person types,
  // and a few non-secret hints (deployment mode, surface state, blocked reason).
  // The server scans every value and refuses anything unsafe; this surface just
  // shows the server's calm yes-or-try-again.
  import { createMdxClient } from "@mdx/client";

  let {
    surface = "",
    session = null,
    actorId = "local_user",
    deploymentMode = "",
    surfaceState = "",
    routeStatus = "",
    blockedReason = "",
    appVersion = "",
    featureArea = ""
  } = $props();

  // UI label -> contract category. The contract owns the vocabulary.
  const CATEGORIES = [
    { id: "bug", label: "Something broke" },
    { id: "blocked", label: "I got blocked" },
    { id: "confusing", label: "This was confusing" },
    { id: "idea", label: "I have an idea" }
  ];

  let open = $state(false);

  // Any surface can summon the report affordance prefilled - error flashes
  // dispatch this at the moment of failure, the highest-value capture point.
  $effect(() => {
    function onReport(event) {
      open = true;
      const prefill = String(event?.detail?.note ?? "");
      if (prefill) 
        note = prefill;
    }
    window.addEventListener("mdx:report", onReport);
    return () => window.removeEventListener("mdx:report", onReport);
  });
  let category = $state("bug");
  let note = $state("");
  let sending = $state(false);
  let result = $state(null); // { ok: boolean, message: string }

  function toggle() {
    open = !open;
    if (open) {
      result = null;
    }
  }

  function chooseCategory(id) {
    category = id;
  }

  async function send() {
    if (sending) return;
    sending = true;
    result = null;
    // Build the safe payload. Only allowlisted, non-secret hints ride along; the
    // route path is a path, never a full URL or query string.
    const path =
      typeof window !== "undefined" && window.location ? window.location.pathname : "/";
    const body = {
      surface,
      route: path,
      category,
      note,
      occurred_at: new Date().toISOString(),
      actor_id: session?.user_id ?? actorId
    };
    const hints = {
      deployment_mode: deploymentMode,
      surface_state: surfaceState,
      route_status: routeStatus,
      blocked_reason: blockedReason,
      app_version: appVersion,
      feature_area: featureArea
    };
    for (const [key, value] of Object.entries(hints)) {
      if (value) body[key] = value;
    }
    try {
      // The typed client is the one write rail: actor admission and receipt
      // intent ride every governed write, the same as Twin, Pages, and Forge.
      const client = createMdxClient({
        baseUrl: "/api/kernel",
        session: session ?? { tenant_id: "local_tenant", user_id: actorId, role: "operator" }
      });
      const packet = await client.write("/feedback/captures.json", body);
      if (packet.status === "BETA_FEEDBACK_RECORDED") {
        result = { ok: true, message: "Thanks - your feedback is in. We read every one." };
        note = "";
      } else if (packet.status === "BETA_FEEDBACK_REFUSED") {
        result = { ok: false, message: packet.message ?? "That feedback could not be sent. Please try again." };
      } else {
        result = { ok: false, message: "That did not send. Please try again." };
      }
    } catch (error) {
      result = { ok: false, message: "Nothing was sent - that did not land. Try again." };
    }
    sending = false;
  }
</script>

<div class="feedback-root">
  <button type="button" class="feedback-trigger" onclick={toggle} aria-expanded={open}>
    {open ? "Close" : "Report an issue"}
  </button>

  {#if open}
    <div class="feedback-panel" role="dialog" aria-label="Share beta feedback">
      <p class="feedback-eyebrow">Beta feedback</p>
      <h3 class="feedback-title">Tell us what happened</h3>
      <div class="feedback-cats">
        {#each CATEGORIES as entry}
          <button
            type="button"
            class="cat"
            class:active={category === entry.id}
            onclick={() => chooseCategory(entry.id)}
          >
            {entry.label}
          </button>
        {/each}
      </div>
      <textarea
        class="feedback-note"
        bind:value={note}
        rows="4"
        placeholder="In your own words. Please leave out anything private - no secrets, passwords, or pasted message or page text."
      ></textarea>
      <div class="feedback-actions">
        <span class="feedback-hint">Sends the page you are on and your note. Nothing private.</span>
        <button type="button" class="feedback-send" onclick={send} disabled={sending}>
          {sending ? "Sending..." : "Send"}
        </button>
      </div>
      {#if result}
        <p class="feedback-result" class:ok={result.ok}>{result.message}</p>
      {/if}
    </div>
  {/if}
</div>

<style>
  .feedback-root {
    position: relative;
    display: inline-block;
  }
  .feedback-trigger {
    appearance: none;
    border: 1px solid var(--mdx-border-soft, rgba(127, 127, 127, 0.25));
    background: transparent;
    color: var(--mdx-text-secondary, #6b7280);
    font-size: 0.78rem;
    padding: 0.3rem 0.7rem;
    border-radius: var(--mdx-radius-control, 8px);
    cursor: pointer;
    transition: color 0.15s ease, border-color 0.15s ease;
  }
  .feedback-trigger:hover {
    color: var(--mdx-text-primary, #111827);
    border-color: var(--mdx-border-default, rgba(127, 127, 127, 0.4));
  }
  .feedback-panel {
    position: absolute;
    right: 0;
    bottom: calc(100% + 0.5rem);
    width: 20rem;
    max-width: 84vw;
    background: var(--mdx-surface-overlay, #ffffff);
    border: 1px solid var(--mdx-border-soft, rgba(127, 127, 127, 0.25));
    border-radius: var(--mdx-radius-panel, 14px);
    box-shadow: var(--mdx-shadow-card, 0 12px 40px rgba(0, 0, 0, 0.18));
    padding: 1rem;
    z-index: 40;
    text-align: left;
  }
  .feedback-eyebrow {
    margin: 0;
    font-size: 0.66rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--mdx-text-muted, #9ca3af);
  }
  .feedback-title {
    margin: 0.15rem 0 0.7rem;
    font-size: 0.98rem;
    color: var(--mdx-text-primary, #111827);
  }
  .feedback-cats {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    margin-bottom: 0.6rem;
  }
  .cat {
    appearance: none;
    border: 1px solid var(--mdx-border-soft, rgba(127, 127, 127, 0.25));
    background: transparent;
    color: var(--mdx-text-secondary, #6b7280);
    font-size: 0.74rem;
    padding: 0.28rem 0.55rem;
    border-radius: 999px;
    cursor: pointer;
  }
  .cat.active {
    color: var(--mdx-text-primary, #ffffff);
    background: var(--mdx-brand-surface, #2563eb);
    border-color: transparent;
  }
  .feedback-note {
    width: 100%;
    box-sizing: border-box;
    resize: vertical;
    border: 1px solid var(--mdx-border-soft, rgba(127, 127, 127, 0.25));
    border-radius: var(--mdx-radius-control, 8px);
    background: var(--mdx-surface-sunken, rgba(127, 127, 127, 0.06));
    color: var(--mdx-text-primary, #111827);
    font: inherit;
    font-size: 0.84rem;
    padding: 0.5rem 0.6rem;
  }
  .feedback-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.6rem;
    margin-top: 0.6rem;
  }
  .feedback-hint {
    font-size: 0.68rem;
    color: var(--mdx-text-muted, #9ca3af);
  }
  .feedback-send {
    appearance: none;
    border: none;
    background: var(--mdx-brand-surface, #2563eb);
    color: #ffffff;
    font-size: 0.8rem;
    padding: 0.4rem 0.9rem;
    border-radius: var(--mdx-radius-control, 8px);
    cursor: pointer;
  }
  .feedback-send:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .feedback-result {
    margin: 0.6rem 0 0;
    font-size: 0.78rem;
    color: var(--mdx-danger-red, #b91c1c);
  }
  .feedback-result.ok {
    color: var(--mdx-accent-success, #15803d);
  }
</style>
