<script>
  // Twin's capabilities, configurable: which skills the model may
  // propose, and which guards stand before and after it. v1's settings
  // console reborn at the right size. Every switch takes effect on the
  // next question - the kernel reads the config hot, and an absent file
  // means everything on (fail-safe, never fail-open). Every change
  // leaves a record: who flipped what, from what, to what - read back
  // from the server's own trail, never assumed.
  import { onMount } from "svelte";
  import { createMdxClient } from "@mdx/client";
  import { loadProfile, personaIdFrom } from "../../../lib/youProfile.js";

  // The switch list is the generated registry's declaration, handed down
  // by the server load - one source of truth for what Twin is made of.
  let { data } = $props();

  const capabilitiesPath = "/api/kernel/twin/capabilities.json";
  const changesPath = "/api/kernel/twin/capability-changes/projection.json";

  let keys = $state(null);
  let saving = $state(false);
  let savedFlash = $state(false);
  // The change trail, newest first, straight from the projection.
  let changes = $state([]);
  // The prompt packs the RUNNING server composes from - served by the
  // binary itself, so this list is what is actually active, not what the
  // repo happens to contain.
  let packs = $state(null);
  // Live-answer standing, straight from the observation projection: who
  // stands accepted to answer, or the honest deterministic state.
  let standing = $state(null);
  // The since-boot stream telemetry ring - load, latency, outcomes.
  let telemetry = $state(null);
  // Offers on the record, from the proposal ledger projection.
  let proposals = $state(null);
  // The guard console: try any text against the SAME deterministic
  // pipeline a real question takes - nothing sent, nothing saved.
  let guardText = $state("");
  let guardVerdict = $state(null);
  let guardChecking = $state(false);
  // "down" when nothing answers; "older" when the kernel answers other
  // routes but not this one - which means a restart picks up the build
  // that has it. Telling those apart saves the operator a guess.
  let unreachable = $state("");
  let flashTimer = null;

  async function load() {
    try {
      const response = await fetch(capabilitiesPath);
      const packet = await response.json().catch(() => ({}));
      keys = packet.keys ?? null;
      if (keys) {
        unreachable = "";
        await Promise.all([loadTrail(), loadPacks(), loadStanding(), loadProposals(), loadTelemetry()]);
        return;
      }
    } catch (error) {
      // fall through to the probe
    }
    try {
      const probe = await fetch("/api/kernel/local/auth-session.json");
      unreachable = probe.ok ? "older" : "down";
    } catch (error) {
      unreachable = "down";
    }
  }

  // Re-read the trail from the projection after every load and write -
  // the record on disk is the truth this page shows, not local memory.
  async function loadTrail() {
    try {
      const response = await fetch(changesPath);
      const packet = await response.json().catch(() => ({}));
      changes = Array.isArray(packet.changes) ? packet.changes : [];
    } catch (error) {
      changes = [];
    }
  }

  async function loadPacks() {
    try {
      const response = await fetch("/api/kernel/twin/prompt-packs.json");
      const packet = await response.json().catch(() => null);
      packs = packet && Array.isArray(packet.packs) ? packet : null;
    } catch (error) {
      packs = null;
    }
  }

  async function loadStanding() {
    try {
      const response = await fetch("/api/kernel/twin/model-gateway-provider-observations/projection.json");
      standing = (await response.json().catch(() => null)) ?? null;
    } catch (error) {
      standing = null;
    }
  }

  async function loadTelemetry() {
    try {
      const response = await fetch("/api/kernel/twin/stream-telemetry.json");
      const packet = await response.json().catch(() => null);
      telemetry = packet && typeof packet.stream_count === "number" ? packet : null;
    } catch (error) {
      telemetry = null;
    }
  }

  async function loadProposals() {
    try {
      const response = await fetch("/api/kernel/twin/skill-proposals/projection.json");
      const packet = await response.json().catch(() => null);
      proposals = packet && Array.isArray(packet.proposals) ? packet : null;
    } catch (error) {
      proposals = null;
    }
  }

  async function checkGuards() {
    if (!guardText.trim() || guardChecking) return;
    guardChecking = true;
    try {
      const response = await fetch("/api/kernel/twin/guard-preflight.json", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ text: guardText })
      });
      guardVerdict = await response.json();
    } catch (error) {
      guardVerdict = null;
    }
    guardChecking = false;
  }

  const acceptedProviders = $derived(
    (standing?.observations ?? [])
      .filter((entry) => entry.provider_connected_local_allowed === true)
      .map((entry) => `${entry.provider_id ?? "provider"} (${entry.model_id ?? "model"})`)
  );

  function proposalWhen(eventList) {
    const seconds = eventList?.[0]?.recorded_at_epoch_seconds;
    if (!seconds) return "";
    return new Date(seconds * 1000).toLocaleString(undefined, {
      month: "short", day: "numeric", hour: "numeric", minute: "2-digit"
    });
  }

  async function toggle(key) {
    if (!keys || saving) return;
    saving = true;
    try {
      const client = createMdxClient({ baseUrl: "/api/kernel", session: data.session });
      const packet = await client.write("/twin/capabilities.json", {
        [key]: !keys[key],
        // The change record names who flipped the switch: the You
        // profile's derived id when one exists, the local seat otherwise.
        actor_id: personaIdFrom(loadProfile()) || "local_user"
      });
      keys = packet.keys ?? keys;
      savedFlash = true;
      clearTimeout(flashTimer);
      flashTimer = setTimeout(() => (savedFlash = false), 1600);
      await loadTrail();
    } catch (error) {
      unreachable = "down";
    }
    saving = false;
  }

  const labelByKey = $derived(
    data.groups
      .flatMap((group) => group.items)
      .reduce((map, item) => ({ ...map, [item.key]: item.label }), {})
  );

  function changeLine(record) {
    const moves = (record.changed ?? [])
      .map((move) => `${labelByKey[move.key] ?? move.key} ${move.next ? "on" : "off"}`)
      .join(", ");
    return moves || "no keys recorded";
  }

  function changeWhen(record) {
    const seconds = record.recorded_at_epoch_seconds;
    if (!seconds) return "";
    return new Date(seconds * 1000).toLocaleString(undefined, {
      month: "short", day: "numeric", hour: "numeric", minute: "2-digit"
    });
  }

  // The derived You-profile id is shown verbatim - it is already the
  // transparent, never-hidden value the profile card displays.
  function actorLabel(record) {
    return record.actor_id === "local_user" ? "this machine" : record.actor_id;
  }

  onMount(() => {
    load();
    return () => clearTimeout(flashTimer);
  });
</script>

<section class="twin-admin" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Twin</h1>
    <p class="mdx-page-sub">
      What Twin may offer to do, and the guards that stand before and after the model. Changes
      apply to the very next question.
    </p>
    {#if savedFlash}<span class="saved">Saved</span>{/if}
  </header>

  {#if standing}
    <p class="standing" data-live={acceptedProviders.length > 0}>
      {#if acceptedProviders.length > 0}
        {acceptedProviders.join(" and ")} {acceptedProviders.length === 1 ? "stands" : "stand"}
        accepted to answer live - turned on by hand, standing recorded. Live answers also need
        this machine's keys in the server's shell.
      {:else}
        Answers are deterministic right now - no provider stands accepted. The turn-on ceremony
        opens live answers when you choose.
      {/if}
      {#if telemetry && telemetry.stream_count > 0}
        <span class="standing-load">
          Since boot: {telemetry.stream_count} live
          {telemetry.stream_count === 1 ? "stream" : "streams"}, first word typically
          {telemetry.first_token_ms?.p50 ?? 0}ms (p95 {telemetry.first_token_ms?.p95 ?? 0}ms),
          {telemetry.limits?.max_concurrent_streams ?? 0} seats.
        </span>
      {/if}
    </p>
  {/if}

  {#if unreachable === "older"}
    <p class="quiet">
      The local server is running an older build that does not have these switches yet. Restart
      it (same command, fresh binary) and this page fills in - until then every guard and skill
      runs in its default-on state.
    </p>
  {:else if unreachable === "down"}
    <p class="quiet">
      The local server is not answering, so the current switches cannot be read. Start it and
      this page fills in - until then every guard and skill runs in its default-on state.
    </p>
  {:else if !keys}
    <p class="quiet">Reading the current switches...</p>
  {:else}
    {#each data.groups as group}
      <div class="group">
        <h2>{group.title}</h2>
        <p class="group-cue">{group.cue}</p>
        <div class="switches">
          {#each group.items as item}
            <button
              type="button"
              class="switch"
              class:on={keys[item.key]}
              onclick={() => toggle(item.key)}
              disabled={saving}
              role="switch"
              aria-checked={keys[item.key]}
            >
              <span class="switch-pill" aria-hidden="true"><span class="switch-dot"></span></span>
              <span class="switch-body">
                <strong>{item.label}</strong>
                <span>{item.line}</span>
              </span>
            </button>
          {/each}
        </div>
      </div>
    {/each}
    {#if packs}
      <div class="group">
        <h2>What Twin is told</h2>
        <p class="group-cue">{packs.human_line}</p>
        <div class="packs">
          {#each packs.packs as pack (pack.id)}
            <details class="pack">
              <summary>
                <strong>{pack.id.replace("companion:twin_", "").replace(/_/g, " ")}</strong>
                <span class="pack-version">v{pack.version}</span>
              </summary>
              <p class="pack-text">{pack.text}</p>
            </details>
          {/each}
        </div>
        <p class="group-cue">{packs.private_layers_note}</p>
      </div>
    {/if}
    <div class="group">
      <h2>Who answers</h2>
      <p class="group-cue">
        Five specialists, each a declared prompt layer - plus the routing that picks one
        without a model call. Companions you create on the You page live on this machine and
        join them at ask time.
      </p>
      <div class="fact-grid">
        {#each data.companions as companion (companion.id)}
          <div class="fact">
            <strong>{companion.name}</strong>
            <span>{companion.stance}</span>
          </div>
        {/each}
      </div>
      {#each data.routing as policy (policy.id)}
        <p class="fact-line"><strong>{policy.label}</strong> - {policy.line}</p>
      {/each}
      {#each data.memory as policy (policy.id)}
        <p class="fact-line"><strong>{policy.label}</strong> - {policy.line}</p>
      {/each}
    </div>

    {#if proposals && proposals.proposal_count > 0}
      <div class="group">
        <h2>Offers on the record</h2>
        <p class="group-cue">
          Every offer Twin makes leaves a record, and so does your answer - approved, declined,
          or saved with its receipt. The text itself is recorded only when you approve.
        </p>
        <div class="proposal-rows">
          {#each proposals.proposals.slice(0, 6) as proposal (proposal.proposal_id)}
            <p class="fact-line">
              <strong>{proposal.skill.replace(/_/g, " ")}</strong>
              - {proposal.state}{proposalWhen(proposal.events) ? `, ${proposalWhen(proposal.events)}` : ""}
              {#if proposal.events.find((event) => event.executed_receipt_id)}
                <span class="trail-id">{proposal.events.find((event) => event.executed_receipt_id).executed_receipt_id}</span>
              {/if}
            </p>
          {/each}
        </div>
      </div>
    {/if}

    <div class="group">
      <h2>Try the guards</h2>
      <p class="group-cue">
        Run any text through the same deterministic checks a real question takes. Nothing is
        sent to a model and nothing is saved - this is the rail, not the train.
      </p>
      <div class="guard-console">
        <textarea
          rows="2"
          placeholder="Paste anything - an email address, an instruction-rewrite attempt..."
          bind:value={guardText}
        ></textarea>
        <button type="button" class="mdx-btn" onclick={checkGuards} disabled={guardChecking || !guardText.trim()}>
          {guardChecking ? "Checking..." : "Check"}
        </button>
      </div>
      {#if guardVerdict}
        <div class="guard-verdict" data-held={guardVerdict.input_would_hold}>
          {#if guardVerdict.input_would_hold}
            <p>Held before the model: {guardVerdict.input_hold_reason}</p>
          {:else if guardVerdict.output_would_redact}
            <p>
              Would pass to the model; in an answer, personal info would be redacted before
              saving: <code>{guardVerdict.output_after_redaction}</code>
            </p>
          {:else}
            <p>
              Passes clean.{guardVerdict.voice_drift_count > 0
                ? ` Voice check would note ${guardVerdict.voice_drift_count} chatbot-speak ${guardVerdict.voice_drift_count === 1 ? "phrase" : "phrases"}.`
                : ""}
            </p>
          {/if}
        </div>
      {/if}
    </div>

    {#if data.connectors.length > 0}
      <div class="group">
        <h2>What's coming</h2>
        <p class="group-cue">
          Declared, not built - each lights up here when its contract lands, with its own
          records and controls. Never a toggle that pretends.
        </p>
        {#each data.connectors as connector (connector.id)}
          <p class="fact-line"><strong>{connector.label}</strong> - {connector.line}</p>
        {/each}
      </div>
    {/if}

    {#if changes.length > 0}
      <div class="quiet-line">
        Last change: {changeLine(changes[0])}, {changeWhen(changes[0])} by {actorLabel(changes[0])}.
        <details class="quiet-more">
          <summary>history</summary>
          <span class="trail">
            {#each changes.slice(0, 8) as record (record.record_id)}
              <span class="trail-row">
                {changeLine(record)} - {changeWhen(record)} by {actorLabel(record)}
                <span class="trail-id">{record.record_id}</span>
              </span>
            {/each}
          </span>
        </details>
      </div>
    {/if}
    <p class="quiet-line">
      Lives in a plain file on this machine, and every change leaves a record.
      <details class="quiet-more">
        <summary>more</summary>
        <span>
          .mdx-local/twin-capabilities.json, read fresh on every question. Delete the file and
          everything returns to on - absence fails safe, never open. Changes land write-once in
          .mdx-local/twin-capability-changes/ and are served back whole at
          /twin/capability-changes/projection.json.
        </span>
      </details>
    </p>
  {/if}
</section>

<style>
  .twin-admin {
    display: grid;
    gap: 22px;
    max-width: 760px;
    margin: 0 auto;
    width: 100%;
    padding: 26px 0 60px;
  }

  .saved {
    color: var(--mdx-accent-success);
    font-size: var(--mdx-text-xs);
  }

  .group {
    display: grid;
    gap: 10px;
  }

  .group h2 {
    margin: 0;
    font-size: 15px;
  }

  .group-cue {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
    line-height: 1.5;
  }

  .switches {
    display: grid;
    gap: 8px;
  }

  .switch {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    text-align: left;
    cursor: pointer;
  }

  .switch:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .switch-pill {
    flex: none;
    width: 34px;
    height: 20px;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-border-default);
    position: relative;
    transition: background 160ms ease-out;
  }

  .switch.on .switch-pill {
    background: var(--mdx-accent-primary);
  }

  .switch-dot {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--mdx-surface-base);
    transition: transform 160ms ease-out;
  }

  .switch.on .switch-dot {
    transform: translateX(14px);
  }

  @media (prefers-reduced-motion: reduce) {
    .switch-pill,
    .switch-dot { transition: none; }
  }

  .switch-body {
    display: grid;
    gap: 2px;
  }

  .switch-body strong {
    font-size: 13.5px;
  }

  .switch-body span {
    color: var(--mdx-text-tertiary);
    font-size: 12px;
  }

  .quiet,
  .quiet-line {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
    line-height: 1.55;
  }

  .quiet-more {
    display: inline;
  }

  .quiet-more summary {
    display: inline;
    cursor: pointer;
    color: var(--mdx-text-muted);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .quiet-more[open] summary {
    display: none;
  }

  .standing {
    margin: 0;
    padding: 10px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
    line-height: 1.5;
  }

  .standing[data-live="true"] {
    color: var(--mdx-text-primary);
  }

  .standing-load {
    display: block;
    margin-top: 4px;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .fact-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 8px;
  }

  .fact {
    display: grid;
    gap: 2px;
    padding: 10px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }

  .fact strong {
    font-size: 13.5px;
  }

  .fact span {
    color: var(--mdx-text-tertiary);
    font-size: 12px;
  }

  .fact-line {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
    line-height: 1.5;
  }

  .fact-line strong {
    color: var(--mdx-text-primary);
    font-weight: 600;
  }

  .proposal-rows {
    display: grid;
    gap: 4px;
  }

  .guard-console {
    display: flex;
    gap: 8px;
    align-items: flex-start;
  }

  .guard-console textarea {
    flex: 1;
    resize: vertical;
    padding: 10px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: var(--mdx-text-sm);
  }

  .guard-verdict {
    margin: 0;
    padding: 10px 14px;
    border-radius: var(--mdx-radius-md);
    border: 1px solid var(--mdx-border-subtle);
    background: var(--mdx-surface-raised);
  }

  .guard-verdict[data-held="true"] {
    border-color: var(--mdx-accent-warning, var(--mdx-border-default));
  }

  .guard-verdict p {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
    line-height: 1.5;
  }

  .guard-verdict code {
    font-size: 12px;
  }

  .packs {
    display: grid;
    gap: 6px;
  }

  .pack {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    padding: 10px 14px;
  }

  .pack summary {
    cursor: pointer;
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 13.5px;
  }

  .pack-version {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .pack-text {
    margin: 8px 0 2px;
    color: var(--mdx-text-tertiary);
    font-size: var(--mdx-text-sm);
    line-height: 1.55;
    white-space: pre-wrap;
  }

  .trail {
    display: grid;
    gap: 4px;
    padding-top: 4px;
  }

  .trail-row {
    display: block;
  }

  .trail-id {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    font-family: var(--mdx-font-mono, monospace);
    margin-left: 6px;
  }
</style>
