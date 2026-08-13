<script>
  // Message's operator room: the switches the write path enforces, the
  // size limit, the rails as declared facts, honest not-built connector
  // slots, and the change trail - who flipped what, from what, to what.
  import { onMount } from "svelte";
  import { createMdxClient } from "@mdx/client";
  import { loadProfile, personaIdFrom } from "../../../lib/youProfile.js";

  let { data } = $props();

  const controlsPath = "/api/kernel/messages/controls.json";
  const changesPath = "/api/kernel/messages/control-changes/projection.json";

  const switches = [
    {
      key: "agent_posting_allowed",
      label: "Agents may post",
      line: "agent, pipeline, and worker actors can write to rooms - refused at the kernel when off"
    },
    {
      key: "reactions_enabled",
      label: "Reactions",
      line: "the quick 👍 on any message - refused at the kernel when off"
    },
    {
      key: "slash_commands_enabled",
      label: "Slash commands",
      line: "/decision, /handoff, /search, /mute in the composer"
    }
  ];

  let keys = $state(null);
  let saving = $state(false);
  let savedFlash = $state(false);
  let unreachable = $state("");
  let changes = $state([]);
  let maxBodyDraft = $state("");
  let flashTimer = null;
  // The Slack/Teams bridge status, derived from the kernel: inbound items
  // through the connector rail, outbound posts queued (recorded, gated).
  let bridge = $state(null);

  async function loadBridge() {
    try {
      const response = await fetch("/api/kernel/messages/bridge/projection.json");
      bridge = await response.json().catch(() => null);
    } catch (error) {
      bridge = null;
    }
  }

  async function load() {
    try {
      const response = await fetch(controlsPath);
      const packet = await response.json().catch(() => ({}));
      keys = packet.keys ?? null;
      if (keys) {
        unreachable = "";
        maxBodyDraft = String(keys.max_message_body_chars ?? 4000);
        await loadTrail();
        await loadBridge();
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

  async function loadTrail() {
    try {
      const response = await fetch(changesPath);
      const packet = await response.json().catch(() => ({}));
      changes = Array.isArray(packet.changes) ? packet.changes : [];
    } catch (error) {
      changes = [];
    }
  }

  async function save(patch) {
    if (!keys || saving) return;
    saving = true;
    try {
      const client = createMdxClient({ baseUrl: "/api/kernel", session: data.session });
      const packet = await client.write("/messages/controls.json", {
        ...patch,
        actor_id: personaIdFrom(loadProfile()) || "local_user"
      });
      keys = packet.keys ?? keys;
      maxBodyDraft = String(keys.max_message_body_chars ?? maxBodyDraft);
      savedFlash = true;
      clearTimeout(flashTimer);
      flashTimer = setTimeout(() => (savedFlash = false), 1600);
      await loadTrail();
    } catch (error) {
      unreachable = "down";
    }
    saving = false;
  }

  const labelByKey = {
    agent_posting_allowed: "Agents may post",
    reactions_enabled: "Reactions",
    slash_commands_enabled: "Slash commands",
    max_message_body_chars: "Size limit"
  };

  function changeLine(record) {
    return (record.changed ?? [])
      .map((move) =>
        typeof move.next === "boolean"
          ? `${labelByKey[move.key] ?? move.key} ${move.next ? "on" : "off"}`
          : `${labelByKey[move.key] ?? move.key} ${move.previous} → ${move.next}`
      )
      .join(", ");
  }

  function changeWhen(record) {
    const seconds = record.recorded_at_epoch_seconds;
    if (!seconds) return "";
    return new Date(seconds * 1000).toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit"
    });
  }

  function actorLabel(record) {
    return record.actor_id === "local_user" ? "this machine" : record.actor_id;
  }

  onMount(() => {
    load();
    return () => clearTimeout(flashTimer);
  });
</script>

<section class="message-admin" data-route-state="ready">
  <header class="mdx-page-head">
    <h1>Message</h1>
    <p class="mdx-page-sub">
      The room's rules, enforced where the writes happen. Changes apply to the very next message.
    </p>
    {#if savedFlash}<span class="saved">Saved</span>{/if}
  </header>

  {#if unreachable === "older"}
    <p class="quiet">
      The local server is running an older build that does not have these controls yet. Restart it
      and this page fills in - until then everything runs in its default-on state.
    </p>
  {:else if unreachable === "down"}
    <p class="quiet">
      The local server is not answering, so the current controls cannot be read. Start it and this
      page fills in.
    </p>
  {:else if !keys}
    <p class="quiet">Reading the current controls...</p>
  {:else}
    <div class="group">
      <h2>Who may write, and how much</h2>
      <div class="switches">
        {#each switches as item (item.key)}
          <button
            type="button"
            class="switch"
            class:on={keys[item.key]}
            onclick={() => save({ [item.key]: !keys[item.key] })}
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
      <div class="limit-row">
        <label for="max-body">Message size limit</label>
        <input
          id="max-body"
          type="number"
          min="100"
          max="20000"
          bind:value={maxBodyDraft}
          disabled={saving}
        />
        <button
          type="button"
          class="mdx-btn small"
          onclick={() => save({ max_message_body_chars: Number(maxBodyDraft) })}
          disabled={saving || Number(maxBodyDraft) === keys.max_message_body_chars}
        >
          Set
        </button>
        <span class="limit-note">characters; longer messages are refused with a plain reason</span>
      </div>
    </div>

    <div class="group">
      <h2>The rails, as declared</h2>
      <p class="fact-line"><strong>Offline</strong> - {data.rails.offlineQueue.line}</p>
      <p class="fact-line">
        <strong>Reconnect</strong> - {data.rails.replay.line}
      </p>
      <p class="fact-line">
        <strong>Agent rooms</strong> - {data.rails.agentNoise.line}
      </p>
      {#each data.relayFacts as fact (fact.label)}
        <p class="fact-line quiet-fact"><strong>{fact.label}</strong> - {fact.value}</p>
      {/each}
    </div>

    {#if bridge?.systems}
      <div class="group">
        <h2>Connected workspaces</h2>
        <p class="group-cue">
          Slack and Teams. Inbound messages arrive through the connector rail; outbound posts are
          recorded and queued. Nothing is delivered yet - real delivery waits on the provider gate.
        </p>
        {#each bridge.systems as system (system.system)}
          <p class="fact-line">
            <strong>{system.system === "slack" ? "Slack" : "Teams"}</strong>
            - {system.connected ? "connected" : "not connected yet"}
            · {system.inbound_count} in · {system.outbound_queued_count} queued out
          </p>
        {/each}
        <p class="quiet-line">
          Live delivery is off: {bridge.live_transport_note}
        </p>
      </div>
    {/if}

    <div class="group">
      <h2>What's coming</h2>
      <p class="group-cue">
        Declared, not built - each lights up here when its contract lands. Never a toggle that
        pretends.
      </p>
      {#each data.connectors as connector (connector.label)}
        <p class="fact-line"><strong>{connector.label}</strong> - {connector.line}</p>
      {/each}
    </div>

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
          .mdx-local/message-controls.json, read fresh on every write. Changes land write-once in
          .mdx-local/message-control-changes/ and are served back whole at
          /messages/control-changes/projection.json.
        </span>
      </details>
    </p>
  {/if}
</section>

<style>
  .message-admin {
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
    .switch-dot {
      transition: none;
    }
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

  .limit-row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .limit-row label {
    font-size: 13.5px;
    font-weight: 600;
  }

  .limit-row input {
    width: 110px;
    padding: 8px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: var(--mdx-text-sm);
  }

  .limit-note {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
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

  .quiet-fact {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
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
