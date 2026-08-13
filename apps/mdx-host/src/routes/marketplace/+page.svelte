<script>
  import { onMount } from "svelte";
  import { createMdxClient } from "@mdx/client";

  let { data } = $props();

  const TABS = [
    ["for-you", "For you"],
    ["packs", "Packs"],
    ["installed", "Installed"],
    ["review", "Review"]
  ];
  const STATUS = {
    not_installed: "Available",
    ready: "Ready",
    held: "Needs review",
    setup_required: "Setup verification needed",
    update_available: "Update available",
    disabled: "Disabled"
  };

  let activeTab = $state("for-you");
  let query = $state("");
  let selectedPackId = $state("");
  let installScope = $state("me");
  let applicationTargets = $state([]);
  let originSurface = $state("marketplace");
  let originObjectId = $state("");
  let returnTo = $state("");
  let acting = $state(false);
  let notice = $state(null);
  let trial = $state(null);
  let livePacks = $state(null);
  let liveInstalled = $state(null);
  let liveImpact = $state(null);
  let liveCapabilities = $state(null);

  const packs = $derived(livePacks ?? data.packs ?? []);
  const installedPacks = $derived(liveInstalled ?? data.installedPacks ?? []);
  const impact = $derived(liveImpact ?? data.impact ?? { packs: [] });
  const capabilities = $derived(liveCapabilities ?? data.capabilities ?? []);
  const selectedPack = $derived(packs.find((pack) => pack.id === selectedPackId) ?? null);
  const filteredPacks = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return packs;
    return packs.filter((pack) =>
      `${pack.name} ${pack.job} ${pack.outcome} ${(pack.supported_apps ?? []).join(" ")} ${(pack.items ?? []).join(" ")}`
        .toLowerCase()
        .includes(needle)
    );
  });
  const reviewCapabilities = $derived(
    capabilities.filter((capability) => capability.effective_status === "needs_review")
  );
  const proposals = $derived(data.packProposals ?? []);
  const reviewCount = $derived(
    reviewCapabilities.length + proposals.length + (data.skillDrafts ?? []).length
  );

  function safeReturn(value) {
    return typeof value === "string" && value.startsWith("/") && !value.startsWith("//") ? value : "";
  }

  function packImpact(packId) {
    return (impact.packs ?? []).find((row) => row.pack_id === packId) ?? {};
  }

  function openPack(pack) {
    selectedPackId = pack.id;
    applicationTargets = [...(pack.state?.application_targets ?? pack.application_targets ?? [])];
    installScope = pack.state?.scope || pack.install_scopes?.[0] || "me";
    notice = null;
    trial = null;
  }

  function setTab(tab) {
    activeTab = tab;
    selectedPackId = "";
    notice = null;
    history.replaceState({}, "", `${location.pathname}?tab=${tab}`);
  }

  async function refresh() {
    try {
      const [catalog, installed, freshImpact, freshCapabilities] = await Promise.all([
        fetch("/api/kernel/marketplace/packs.json").then((response) => response.json()),
        fetch("/api/kernel/marketplace/installed-packs/projection.json").then((response) => response.json()),
        fetch("/api/kernel/marketplace/impact/projection.json").then((response) => response.json()),
        fetch("/api/kernel/marketplace/capabilities.json").then((response) => response.json())
      ]);
      if (Array.isArray(catalog.packs)) livePacks = catalog.packs;
      if (Array.isArray(installed.packs)) liveInstalled = installed.packs;
      if (Array.isArray(freshImpact.packs)) liveImpact = freshImpact;
      if (Array.isArray(freshCapabilities.capabilities)) liveCapabilities = freshCapabilities.capabilities;
    } catch (error) {
      // Preserve the last honest read when the local kernel is unavailable.
    }
  }

  async function write(path, body) {
    if (acting) return null;
    acting = true;
    notice = null;
    try {
      const client = createMdxClient({ baseUrl: "/api/kernel", session: data.session });
      const packet = await client.write(`/marketplace/${path}`, {
        ...body,
        actor_id: data.session?.user_id ?? "local_user"
      });
      notice = packet.status === "REFUSED"
        ? { ok: false, text: packet.reason }
        : { ok: true, text: packet.line ?? "Recorded.", receipt: packet.record_id ?? "" };
      if (packet.status !== "REFUSED") await refresh();
      return packet;
    } catch (error) {
      notice = { ok: false, text: "Nothing changed. The local kernel did not accept that action." };
      return null;
    } finally {
      acting = false;
    }
  }

  async function applyPack(pack) {
    const packet = await write("installs.json", {
      pack_id: pack.id,
      scope: installScope,
      application_targets: applicationTargets,
      return_to: returnTo,
      origin_surface: originSurface,
      origin_object_id: originObjectId
    });
    if (packet?.status === "OK") {
      selectedPackId = pack.id;
      activeTab = "installed";
    }
  }

  async function tryPack(pack) {
    const packet = await write("pack-trials.json", {
      pack_id: pack.id,
      origin_surface: originSurface,
      origin_object_id: originObjectId,
      task_class: originSurface
    });
    if (packet?.status === "OK") trial = packet;
  }

  function lifecycle(pack, action, extra = {}) {
    return write("pack-actions.json", {
      pack_id: pack.id,
      action,
      scope: installScope,
      application_targets: applicationTargets,
      return_to: returnTo,
      origin_surface: originSurface,
      origin_object_id: originObjectId,
      ...extra
    });
  }

  function decideCapability(capabilityId, decision, extra = {}) {
    return write("approvals.json", { capability_id: capabilityId, decision, ...extra });
  }

  async function ratifySkill(draft, decision) {
    const packet = await write("skill-ratifications.json", {
      skill_id: draft.skill_id,
      draft_receipt_id: draft.draft_receipt_id,
      decision,
      note: ""
    });
    if (packet?.status === "OK") location.reload();
  }

  onMount(() => {
    const params = new URLSearchParams(location.search);
    const tab = params.get("tab");
    if (TABS.some(([id]) => id === tab)) activeTab = tab;
    query = params.get("q") ?? "";
    originSurface = params.get("origin_surface") ?? "marketplace";
    originObjectId = params.get("origin_object_id") ?? "";
    returnTo = safeReturn(params.get("return_to") ?? "");
    const wanted = params.get("pack");
    const pack = packs.find((candidate) => candidate.id === wanted);
    if (pack) openPack(pack);
    refresh();
  });
</script>

<svelte:head><title>Marketplace - MDx</title></svelte:head>

<section class="marketplace" data-route-state="ready" data-marketplace-flagship="true">
  <header class="hero">
    <div>
      <p class="eyebrow">Capability control plane</p>
      <h1>Marketplace</h1>
      <p class="lede">Give every MDx app the right skills, hooks, connectors, and policy - as one governed pack.</p>
    </div>
    <div class="hero-proof" aria-label="Marketplace state">
      <strong>{installedPacks.length}</strong><span>installed packs</span>
      <strong>{reviewCount}</strong><span>decisions waiting</span>
    </div>
  </header>

  {#if returnTo}
    <div class="return-context">
      <span>You came from <strong>{originSurface}</strong>. Apply a pack, then return to the work.</span>
      <a href={returnTo}>Back without changes</a>
    </div>
  {/if}

  <nav class="tabs" aria-label="Marketplace sections">
    {#each TABS as tab (tab[0])}
      <button class:active={activeTab === tab[0]} aria-current={activeTab === tab[0] ? "page" : undefined} onclick={() => setTab(tab[0])}>
        {tab[1]}
        {#if tab[0] === "installed"}<span>{installedPacks.length}</span>{/if}
        {#if tab[0] === "review" && reviewCount}<span>{reviewCount}</span>{/if}
      </button>
    {/each}
  </nav>

  <input class="search" type="search" bind:value={query} placeholder="What do you want MDx to do better?" aria-label="Search packs" />

  {#if selectedPack}
    <article class="detail" aria-label="Pack detail">
      <header class="detail-head">
        <div>
          <div class="badges">
            <span>{selectedPack.source_lane ?? "mdx"}</span>
            <span>{selectedPack.publisher?.verified ? "Verified" : "Team source"}</span>
            <span>{selectedPack.version}</span>
            <span data-state={selectedPack.state?.lifecycle_state}>{STATUS[selectedPack.state?.lifecycle_state ?? "not_installed"]}</span>
          </div>
          <h2>{selectedPack.name}</h2>
          <p>{selectedPack.outcome}</p>
        </div>
        <button class="quiet-button" onclick={() => (selectedPackId = "")}>Close</button>
      </header>

      <div class="before-after">
        <div><span>Before</span><strong>The app plans without this job-specific toolbelt.</strong></div>
        <div><span>After</span><strong>{selectedPack.improves_forge}.</strong></div>
      </div>

      <div class="detail-columns">
        <section>
          <h3>What lands</h3>
          <ul>
            {#each selectedPack.items ?? [] as item (item)}
              <li><strong>{selectedPack.item_details?.find((detail) => detail.id === item)?.name ?? item.replaceAll("_", " ")}</strong>{selectedPack.required_items?.includes(item) ? "Required" : "Optional"}</li>
            {/each}
          </ul>
        </section>
        <section>
          <h3>What it may do</h3>
          <ul>{#each selectedPack.requested_grants ?? [] as grant (grant)}<li>{grant}</li>{/each}</ul>
          <h3>Still blocked</h3>
          <ul class="blocked">{#each selectedPack.blocked_grants ?? [] as grant (grant)}<li>{grant}</li>{/each}</ul>
        </section>
      </div>

      {#if (selectedPack.dependencies ?? []).length || (selectedPack.conflicts ?? []).length}
        <div class="dependency-line">
          {#if selectedPack.dependencies?.length}<span><strong>Needs:</strong> {selectedPack.dependencies.join(", ")}</span>{/if}
          {#if selectedPack.conflicts?.length}<span><strong>Conflicts:</strong> {selectedPack.conflicts.join(", ")}</span>{/if}
        </div>
      {/if}

      <div class="controls">
        <div>
          <span class="control-label">Scope</span>
          {#each selectedPack.install_scopes ?? ["me", "team", "repo"] as scope (scope)}
            <button class:active={installScope === scope} onclick={() => (installScope = scope)}>{scope}</button>
          {/each}
        </div>
        <div>
          <span class="control-label">Use in</span>
          {#each selectedPack.application_targets ?? [] as target (target)}
            <button class:active={applicationTargets.includes(target)} onclick={() => applicationTargets = applicationTargets.includes(target) ? applicationTargets.filter((item) => item !== target) : [...applicationTargets, target]}>{target}</button>
          {/each}
        </div>
      </div>

      <div class="primary-actions">
        {#if !selectedPack.state?.installed}
          <button class="primary" disabled={acting || applicationTargets.length === 0} onclick={() => applyPack(selectedPack)}>Apply pack</button>
          <button disabled={acting} onclick={() => tryPack(selectedPack)}>Try safely</button>
        {:else}
          {#if selectedPack.state?.lifecycle_state === "disabled"}
            <button class="primary" disabled={acting} onclick={() => lifecycle(selectedPack, "enable")}>Enable</button>
          {:else if selectedPack.state?.ready}
            <a class="primary link" href={returnTo || selectedPack.first_use?.href || "/forge"}>{returnTo ? `Return to ${originSurface}` : selectedPack.first_use?.label ?? "Use it now"}</a>
            <button disabled={acting} onclick={() => lifecycle(selectedPack, "disable")}>Disable</button>
          {:else}
            {#if selectedPack.state?.lifecycle_state === "held"}
              <button class="primary" disabled={acting} onclick={() => applyPack(selectedPack)}>Re-apply reviewed pieces</button>
            {/if}
            <button disabled={acting} onclick={() => lifecycle(selectedPack, "disable")}>Disable</button>
          {/if}
          {#if selectedPack.state?.lifecycle_state === "setup_required"}
            <span class="boundary-note">This pack cannot become ready from a confirmation click. Its required setup has no verified completion rail yet.</span>
          {/if}
          {#if selectedPack.update}
            <button disabled={acting} onclick={() => lifecycle(selectedPack, "update", { version: selectedPack.update.version, change_summary: selectedPack.update.change, permission_diff: selectedPack.update.permission_diff })}>Review update</button>
          {/if}
          {#if selectedPack.state?.rollback_version}
            <button disabled={acting} onclick={() => lifecycle(selectedPack, "rollback", { version: selectedPack.state.rollback_version, change_summary: "Restore the prior pack version.", permission_diff: "Restore prior grants." })}>Roll back</button>
          {/if}
          <button disabled={acting} onclick={() => lifecycle(selectedPack, "rescope")}>Save scope</button>
          <button class="danger" disabled={acting} onclick={() => lifecycle(selectedPack, "remove")}>Remove</button>
        {/if}
      </div>

      {#if selectedPack.state?.installed}
        {@const row = packImpact(selectedPack.id)}
        <div class="impact-line"><strong>{row.use_count ?? 0}</strong> uses <strong>{row.accepted_count ?? 0}</strong> accepted <strong>{row.rejected_count ?? 0}</strong> rejected <span>Content is never stored in this ledger.</span></div>
      {/if}

      {#if notice}<p class:bad={!notice.ok} class="notice" role="status">{notice.text}{notice.receipt ? ` Receipt ${notice.receipt}.` : ""}</p>{/if}
      {#if trial}
        <div class="trial">
          <strong>Safe trial complete</strong>
          <p>{trial.trial.example}</p>
          <p>Nothing external changed. You can now preview it in {(trial.after.preview_available_in ?? []).join(", ")}.</p>
        </div>
      {/if}

      <details>
        <summary>Setup, history, and authority</summary>
        <p>Setup: {(selectedPack.setup_steps ?? []).join(" ") || "No setup needed."}</p>
        <p>Installed, configured, connected, authorized, enabled, applicable, and executable are tracked separately. Marketplace never grants execution, secrets, inherited agent permissions, or production writes.</p>
        {#if selectedPack.state?.install_receipt_id}<p>Install receipt: <code>{selectedPack.state.install_receipt_id}</code></p>{/if}
      </details>
    </article>
  {/if}

  {#if !selectedPack && activeTab === "for-you"}
    <section class="shelf">
      <div class="section-head"><div><p class="eyebrow">Recommended now</p><h2>Start with the job</h2></div><p>Every recommendation names why it is here. Ranking is advisory.</p></div>
      <div class="pack-grid">
        {#each (data.recommendations?.length ? data.recommendations : packs.map((pack) => ({ pack, reason: "A curated job pack with a bounded safe trial." }))).slice(0, 4) as recommendation (recommendation.pack.id)}
          <button class="pack-card feature" onclick={() => openPack(recommendation.pack)}>
            <span class="card-top"><span>{recommendation.pack.source_lane}</span><span>{STATUS[recommendation.pack.state?.lifecycle_state ?? "not_installed"]}</span></span>
            <strong>{recommendation.pack.name}</strong><p>{recommendation.pack.outcome}</p><small>{recommendation.reason}</small>
            <span class="app-list">{(recommendation.pack.supported_apps ?? []).join(" + ")}</span>
          </button>
        {/each}
      </div>
    </section>
  {:else if !selectedPack && activeTab === "packs"}
    <section class="shelf">
      <div class="section-head"><div><p class="eyebrow">Curated catalog</p><h2>{filteredPacks.length} job packs</h2></div><p>MDx, company, team, and personal sources stay visibly distinct.</p></div>
      <div class="pack-grid">
        {#each filteredPacks as pack (pack.id)}
          <button class="pack-card" onclick={() => openPack(pack)}>
            <span class="card-top"><span>{pack.source_lane}</span><span>{STATUS[pack.state?.lifecycle_state ?? "not_installed"]}</span></span>
            <strong>{pack.name}</strong><p>{pack.job}</p><small>{pack.items.length} capabilities - v{pack.version}</small>
            <span class="app-list">{(pack.supported_apps ?? []).join(" + ")}</span>
          </button>
        {/each}
      </div>
    </section>
  {:else if !selectedPack && activeTab === "installed"}
    <section class="shelf">
      <div class="section-head"><div><p class="eyebrow">Your capability shelf</p><h2>Installed packs</h2></div><p>Readiness, scope, updates, and impact in one place.</p></div>
      {#if installedPacks.length}
        <div class="installed-list">
          {#each installedPacks as pack (pack.id)}
            {@const row = packImpact(pack.id)}
            <button onclick={() => openPack(pack)}>
              <span><strong>{pack.name}</strong><small>{pack.state.scope} - {(pack.state.application_targets ?? []).join(", ")}</small></span>
              <span><strong>{STATUS[pack.state.lifecycle_state]}</strong><small>{row.use_count ?? 0} uses</small></span>
            </button>
          {/each}
        </div>
      {:else}
        <div class="empty"><h3>Your shelf is ready.</h3><p>Apply a pack and it will show its setup, scope, app targets, lifecycle, and impact here.</p><button class="primary" onclick={() => setTab("for-you")}>Find a first pack</button></div>
      {/if}
    </section>
  {:else if !selectedPack && activeTab === "review"}
    <section class="shelf">
      <div class="section-head"><div><p class="eyebrow">Human judgment</p><h2>Review</h2></div><p>Capabilities and team-pack proposals do not widen authority on their own.</p></div>
      {#each reviewCapabilities as capability (capability.id)}
        <article class="review-row">
          <div><strong>{capability.name}</strong><p>{capability.review_reason}</p><small>{capability.access_line}</small></div>
          <div><button onclick={() => decideCapability(capability.id, "approve", { scope: "team" })}>Approve for team</button><button onclick={() => decideCapability(capability.id, "approve", { scope: "me", read_only: true })}>Approve read-only</button><button class="danger" onclick={() => decideCapability(capability.id, "reject")}>Reject</button></div>
        </article>
      {/each}
      {#each proposals as proposal (proposal.record_id)}
        <article class="review-row"><div><strong>{proposal.pack_id}</strong><p>Proposed from proven work in {proposal.source_record_id || "a Forge run"}.</p><small>Publication decisions are not connected in this beta. This stays a draft and cannot become usable here.</small></div><span class="pill">Publication rail not connected</span></article>
      {/each}
      {#if (data.distilledSkills ?? []).length || (data.skillDrafts ?? []).length}
        <div class="section-head compact"><div><p class="eyebrow">Procedural memory</p><h2>Skills your work proved</h2></div><p>Distilled from a run your team proved, then signed off by a different person.</p></div>
        {#each data.distilledSkills ?? [] as skill (skill.skill_id)}
          <article class="review-row"><div><strong>{skill.skill_name}</strong><p>{skill.summary}</p><small>Ready - from {skill.source_run_id}, signed off by {skill.ratified_by}.</small></div><span class="pill">Ready</span></article>
        {/each}
        {#each data.skillDrafts ?? [] as draft (draft.draft_receipt_id)}
          <article class="review-row"><div><strong>{draft.skill_name}</strong><p>{draft.summary}</p><small>Waiting for a second person to sign off.</small></div><div><button onclick={() => ratifySkill(draft, "ratify")}>Sign off and make ready</button><button onclick={() => ratifySkill(draft, "decline")}>Set aside</button></div></article>
        {/each}
      {/if}
      {#if !reviewCapabilities.length && !proposals.length && !(data.skillDrafts ?? []).length}<div class="empty"><h3>Nothing is waiting.</h3><p>New grants, community sources, and team-pack proposals will land here before they become usable.</p></div>{/if}
      {#if notice}<p class:bad={!notice.ok} class="notice" role="status">{notice.text}</p>{/if}
    </section>
  {/if}

  <footer><strong>Boundary:</strong> Marketplace records capability state and human decisions. Execution authority remains with the app that owns the work. <strong>Safe next:</strong> try a pack against its bounded fixture, or apply it at the narrowest useful scope.</footer>
</section>

<style>
  .marketplace{display:grid;gap:22px;max-width:1120px;margin:0 auto;padding:4vh 0 64px;color:var(--mdx-text-primary)}
  .hero,.detail-head,.section-head{display:flex;justify-content:space-between;gap:24px;align-items:flex-start}.hero h1{font:650 clamp(32px,5vw,52px)/1 var(--mdx-font-display);margin:4px 0 10px}.eyebrow{margin:0;text-transform:uppercase;letter-spacing:.11em;font-size:11px;color:var(--mdx-accent-primary);font-weight:700}.lede{max-width:650px;margin:0;color:var(--mdx-text-secondary);font-size:16px;line-height:1.55}.hero-proof{display:grid;grid-template-columns:auto auto;gap:2px 10px;padding:14px 16px;border:1px solid var(--mdx-border-subtle);border-radius:var(--mdx-radius-lg);background:var(--mdx-surface-raised);white-space:nowrap}.hero-proof strong{font-size:18px;text-align:right}.hero-proof span{font-size:12px;color:var(--mdx-text-muted);align-self:center}
  .return-context{display:flex;justify-content:space-between;gap:16px;padding:10px 14px;border:1px solid color-mix(in srgb,var(--mdx-accent-primary) 40%,transparent);border-radius:var(--mdx-radius-md);font-size:13px;background:color-mix(in srgb,var(--mdx-accent-primary) 6%,transparent)}.return-context a{color:var(--mdx-accent-primary)}
  .tabs{display:flex;gap:6px;border-bottom:1px solid var(--mdx-border-subtle)}.tabs button{border:0;border-bottom:2px solid transparent;background:none;color:var(--mdx-text-secondary);padding:10px 14px;font:600 13px inherit;cursor:pointer}.tabs button.active{color:var(--mdx-text-primary);border-color:var(--mdx-accent-primary)}.tabs span{margin-left:6px;padding:1px 6px;border-radius:99px;background:var(--mdx-surface-raised);font-size:10px}.search{width:100%;box-sizing:border-box;padding:13px 16px;border:1px solid var(--mdx-border-default);border-radius:var(--mdx-radius-lg);background:var(--mdx-surface-raised);color:inherit;font:inherit}
  .shelf{display:grid;gap:14px}.section-head h2{margin:4px 0 0;font:650 22px var(--mdx-font-display)}.section-head>p{max-width:420px;margin:2px 0;color:var(--mdx-text-muted);font-size:12px;line-height:1.5}.pack-grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(250px,1fr));gap:12px}.pack-card{display:grid;gap:8px;text-align:left;padding:16px;border:1px solid var(--mdx-border-subtle);border-radius:var(--mdx-radius-lg);background:var(--mdx-surface-raised);box-shadow:var(--mdx-edge-highlight),var(--mdx-shadow-card);color:inherit;font:inherit;cursor:pointer}.pack-card:hover{border-color:var(--mdx-accent-primary)}.pack-card.feature:first-child{grid-column:span 2;background:linear-gradient(135deg,color-mix(in srgb,var(--mdx-accent-primary) 10%,var(--mdx-surface-raised)),var(--mdx-surface-raised))}.pack-card strong{font:650 16px var(--mdx-font-display)}.pack-card p{margin:0;color:var(--mdx-text-secondary);font-size:13px;line-height:1.5}.pack-card small{color:var(--mdx-text-muted);line-height:1.4}.card-top{display:flex;justify-content:space-between;color:var(--mdx-text-muted);font-size:10px;text-transform:uppercase;letter-spacing:.07em}.app-list{font-size:11px;color:var(--mdx-accent-primary)}
  .detail{display:grid;gap:18px;padding:22px;border:1px solid var(--mdx-border-default);border-radius:var(--mdx-radius-xl);background:var(--mdx-surface-raised);box-shadow:var(--mdx-edge-highlight),var(--mdx-shadow-card)}.detail h2{margin:8px 0 6px;font:650 25px var(--mdx-font-display)}.detail-head p{margin:0;color:var(--mdx-text-secondary);max-width:720px;line-height:1.5}.badges{display:flex;gap:6px;flex-wrap:wrap}.badges span,.pill{padding:3px 8px;border:1px solid var(--mdx-border-subtle);border-radius:99px;font-size:10px;color:var(--mdx-text-secondary)}.before-after{display:grid;grid-template-columns:1fr 1fr;gap:10px}.before-after div{display:grid;gap:6px;padding:13px;border:1px solid var(--mdx-border-subtle);border-radius:var(--mdx-radius-md)}.before-after span,.control-label{font-size:10px;text-transform:uppercase;letter-spacing:.08em;color:var(--mdx-text-muted)}.before-after strong{font-size:12px;line-height:1.5}.detail-columns{display:grid;grid-template-columns:1fr 1fr;gap:20px}.detail h3{margin:0 0 8px;font-size:12px;text-transform:uppercase;letter-spacing:.08em;color:var(--mdx-text-muted)}.detail ul{display:grid;gap:5px;padding:0;margin:0;list-style:none}.detail li{display:flex;justify-content:space-between;gap:12px;font-size:12px;color:var(--mdx-text-secondary)}.detail li strong{font-weight:550;color:var(--mdx-text-primary)}.detail .blocked li{color:var(--mdx-text-muted)}.dependency-line,.impact-line{display:flex;gap:16px;flex-wrap:wrap;padding:10px 12px;border:1px solid var(--mdx-border-subtle);border-radius:var(--mdx-radius-md);font-size:11px;color:var(--mdx-text-secondary)}.impact-line span{color:var(--mdx-text-muted)}
  .controls{display:flex;gap:18px;flex-wrap:wrap}.controls>div{display:flex;align-items:center;gap:6px;flex-wrap:wrap}.controls button,.primary-actions button,.review-row button,.quiet-button{border:1px solid var(--mdx-border-subtle);border-radius:99px;padding:6px 11px;background:none;color:var(--mdx-text-secondary);font:600 11px inherit;cursor:pointer}.controls button.active{border-color:var(--mdx-accent-primary);color:var(--mdx-accent-primary);background:color-mix(in srgb,var(--mdx-accent-primary) 7%,transparent)}.primary-actions{display:flex;gap:8px;align-items:center;flex-wrap:wrap}.primary-actions .primary,.primary,.primary.link{display:inline-block;border:0;border-radius:99px;background:var(--mdx-accent-primary);color:white;padding:9px 16px;font:650 12px inherit;text-decoration:none;cursor:pointer}.boundary-note{max-width:560px;color:var(--mdx-accent-warning);font-size:11px;line-height:1.45}.danger{color:var(--mdx-accent-danger,#d4183d)!important}.quiet-button{flex:none}.notice{margin:0;color:var(--mdx-accent-success);font-size:12px}.notice.bad{color:var(--mdx-accent-warning)}.trial{display:grid;gap:4px;padding:12px 14px;border:1px dashed var(--mdx-accent-primary);border-radius:var(--mdx-radius-md)}.trial p{margin:0;color:var(--mdx-text-secondary);font-size:12px}.detail details{font-size:11px;color:var(--mdx-text-muted)}.detail summary{cursor:pointer}.detail code{overflow-wrap:anywhere}
  .installed-list{display:grid;gap:8px}.installed-list button{display:flex;justify-content:space-between;text-align:left;padding:13px 15px;border:1px solid var(--mdx-border-subtle);border-radius:var(--mdx-radius-md);background:var(--mdx-surface-raised);color:inherit;cursor:pointer}.installed-list span{display:grid;gap:4px}.installed-list span:last-child{text-align:right}.installed-list small{color:var(--mdx-text-muted)}.review-row{display:flex;justify-content:space-between;gap:18px;padding:14px;border:1px solid var(--mdx-border-subtle);border-radius:var(--mdx-radius-md);background:var(--mdx-surface-raised)}.review-row p{margin:5px 0;color:var(--mdx-text-secondary);font-size:12px}.review-row small{color:var(--mdx-text-muted)}.review-row>div:last-child{display:flex;gap:6px;align-items:center;flex-wrap:wrap}.section-head.compact{margin-top:14px}.empty{display:grid;justify-items:start;gap:7px;padding:26px;border:1px dashed var(--mdx-border-default);border-radius:var(--mdx-radius-lg)}.empty h3,.empty p{margin:0}.empty p{color:var(--mdx-text-muted);font-size:13px}footer{padding-top:14px;border-top:1px solid var(--mdx-border-subtle);color:var(--mdx-text-muted);font-size:11px}
  @media(max-width:700px){.hero,.detail-head,.section-head,.review-row{display:grid}.hero-proof{justify-self:start}.pack-card.feature:first-child{grid-column:auto}.before-after,.detail-columns{grid-template-columns:1fr}.tabs{overflow:auto}.tabs button{white-space:nowrap}.section-head>p{margin:0}.installed-list button{display:grid;gap:10px}.installed-list span:last-child{text-align:left}}
</style>
