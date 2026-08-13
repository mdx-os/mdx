<script>
  import { onMount } from "svelte";
  import { createMdxClient } from "@mdx/client";

  let { app, objectId = "", task = "", compact = false, session = null } = $props();
  let recommendations = $state([]);
  let installed = $state([]);
  let pendingReview = $state([]);
  let loading = $state(true);
  let deciding = $state(false);
  let decisionNote = $state("");

  const relevantInstalled = $derived(
    installed.filter((pack) => (pack.state?.application_targets ?? []).includes(app))
  );
  const suggested = $derived(
    recommendations
      .map((entry) => entry.pack)
      .filter((pack) => (pack.supported_apps ?? []).includes(app) && !pack.state?.installed)
      .slice(0, compact ? 1 : 2)
  );

  function marketplaceHref(packId = "") {
    const originPath = typeof location === "undefined" ? `/${app}` : location.pathname + location.search;
    const params = new URLSearchParams({
      origin_surface: app,
      return_to: originPath
    });
    if (objectId) params.set("origin_object_id", objectId);
    if (task) params.set("q", task);
    if (packId) params.set("pack", packId);
    return `/marketplace?${params}`;
  }

  async function decide(capability, decision, readOnly = false) {
    if (deciding || !session) return;
    deciding = true;
    decisionNote = "";
    try {
      const client = createMdxClient({ baseUrl: "/api/kernel", session });
      const packet = await client.write("/marketplace/approvals.json", {
        actor_id: session.user_id ?? "local_user",
        capability_id: capability.id,
        decision,
        scope: decision === "approve" && readOnly ? "me" : "team",
        read_only: readOnly
      });
      if (packet.status === "REFUSED") decisionNote = packet.reason;
      else {
        decisionNote = decision === "approve" ? "Approved and recorded." : "Rejected and recorded.";
        pendingReview = pendingReview.filter((item) => item.id !== capability.id);
      }
    } catch (error) {
      decisionNote = "Nothing changed. Open Marketplace to review it there.";
    } finally {
      deciding = false;
    }
  }

  onMount(async () => {
    try {
      const [forYou, shelf, review] = await Promise.all([
        fetch("/api/kernel/marketplace/for-you.json").then((response) => response.json()),
        fetch("/api/kernel/marketplace/installed-packs/projection.json").then((response) => response.json()),
        fetch("/api/kernel/marketplace/review-queue.json").then((response) => response.json())
      ]);
      recommendations = Array.isArray(forYou.recommendations) ? forYou.recommendations : [];
      installed = Array.isArray(shelf.packs) ? shelf.packs : [];
      pendingReview = Array.isArray(review.pending) ? review.pending : [];
    } catch (error) {
      // The app remains fully usable without Marketplace enrichment.
    } finally {
      loading = false;
    }
  });
</script>

<aside class:compact class="pack-prompt" data-marketplace-entry={app} aria-label={`${app} capability packs`}>
  <div class="prompt-copy">
    <span class="prompt-k">Toolbelt</span>
    {#if relevantInstalled.length}
      <strong>{relevantInstalled.length === 1 ? relevantInstalled[0].name : `${relevantInstalled.length} packs ready`}</strong>
      <small>{relevantInstalled.map((pack) => pack.name).join(" + ")}</small>
    {:else if suggested.length}
      <strong>{suggested[0].name} fits this work</strong>
      <small>{suggested[0].outcome}</small>
    {:else if loading}
      <strong>Checking your packs...</strong>
    {:else}
      <strong>Add a pack for this job</strong>
      <small>Skills, hooks, connectors, and policy arrive together.</small>
    {/if}
  </div>
  <div class="prompt-actions">
    {#if relevantInstalled.length}
      {#each relevantInstalled.slice(0, compact ? 1 : 2) as pack (pack.id)}<span class="ready">{pack.state?.ready ? "Ready" : "Setup"} - {pack.state?.scope}</span>{/each}
    {:else if suggested.length}
      <a class="apply" href={marketplaceHref(suggested[0].id)}>See and apply</a>
    {/if}
    <a href={marketplaceHref()}>Browse packs</a>
  </div>
  {#if app === "message" && session && pendingReview.length}
    {@const request = pendingReview[0]}
    <div class="review-card">
      <span><strong>{request.name}</strong><small>{request.ask}</small></span>
      <button disabled={deciding} onclick={() => decide(request, "approve")}>Approve for team</button>
      <button disabled={deciding} onclick={() => decide(request, "approve", true)}>Read-only</button>
      <button disabled={deciding} onclick={() => decide(request, "reject")}>Reject</button>
    </div>
  {/if}
  {#if decisionNote}<p class="decision-note" role="status">{decisionNote}</p>{/if}
</aside>

<style>
  .pack-prompt{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:11px 13px;border:1px solid var(--mdx-border-subtle);border-radius:var(--mdx-radius-md);background:color-mix(in srgb,var(--mdx-accent-primary) 5%,var(--mdx-surface-raised));color:var(--mdx-text-primary);flex-wrap:wrap}.prompt-copy{display:grid;gap:2px;min-width:0}.prompt-k{font-size:9px;text-transform:uppercase;letter-spacing:.1em;color:var(--mdx-accent-primary);font-weight:700}.prompt-copy strong{font-size:12px}.prompt-copy small{font-size:10.5px;color:var(--mdx-text-muted);white-space:nowrap;overflow:hidden;text-overflow:ellipsis;max-width:560px}.prompt-actions{display:flex;align-items:center;gap:8px;flex:none}.prompt-actions a,.ready{font-size:10px;color:var(--mdx-text-secondary);text-decoration:none;white-space:nowrap}.prompt-actions .apply{padding:5px 9px;border-radius:99px;background:var(--mdx-accent-primary);color:white;font-weight:650}.ready{padding:3px 7px;border:1px solid var(--mdx-border-subtle);border-radius:99px}.review-card{display:flex;align-items:center;gap:7px;flex:1 0 100%;padding-top:8px;border-top:1px solid var(--mdx-border-subtle)}.review-card span{display:grid;gap:2px;flex:1}.review-card strong{font-size:11px}.review-card small,.decision-note{font-size:9.5px;color:var(--mdx-text-muted)}.review-card button{border:1px solid var(--mdx-border-subtle);border-radius:99px;background:none;color:var(--mdx-text-secondary);padding:4px 7px;font:600 9px inherit;cursor:pointer}.decision-note{flex-basis:100%;margin:0}.compact{padding:8px 10px}.compact .prompt-copy small{max-width:260px}@media(max-width:680px){.pack-prompt{display:grid}.prompt-actions,.review-card{flex-wrap:wrap}.prompt-copy small{white-space:normal}.compact .prompt-copy small{max-width:none}.compact .review-card,.compact .decision-note{display:none}}
</style>
