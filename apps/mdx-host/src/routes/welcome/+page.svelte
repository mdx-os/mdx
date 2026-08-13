<script>
  // First-run welcome: the screen `make setup` opens to. It confirms MDx is
  // running, then walks the short path to making it yours - reading real state
  // (kernel reachable, owner claimed, model connected) rather than guessing.
  // Steps 2 and 3 are live here: claim this install (records who owns it, no
  // password, no pretend auth), and connect a model through Model Fabric. The
  // server discovers a model, performs one live probe, and keeps secret values
  // out of receipts and app state.
  import { createMdxClient } from "@mdx/client";
  import { invalidateAll } from "$app/navigation";
  import MarketplacePackPrompt from "../../lib/MarketplacePackPrompt.svelte";

  let { data } = $props();

  const client = $derived(createMdxClient({ baseUrl: "/api/kernel", session: data.session }));
  const actor = $derived(data.session?.user_id ?? "local_user");

  let ownerInput = $state("");
  let keyInput = $state("");
  let busy = $state(false);
  let flash = $state(null);
  // The provider's live model list, captured on connect so a model change can be
  // offered without re-pasting the key (the server reuses the in-memory key).
  // Seeded from the loader so the picker survives a refresh (the server holds the
  // list in memory for the process and returns it from canonical readiness.
  let liveAvailableModels = $state(null);
  const availableModels = $derived(liveAvailableModels ?? data.availableModels ?? []);
  function detectProvider(key) {
    const k = String(key ?? "").trim();
    if (k.startsWith("xai-")) return { id: "xai", label: "xAI" };
    if (k.startsWith("sk-ant-")) return { id: "anthropic", label: "Anthropic" };
    if (k.startsWith("sk-proj-") || k.startsWith("sk-")) return { id: "openai", label: "OpenAI" };
    if (k.startsWith("AIza")) return { id: "gemini", label: "Gemini" };
    return null;
  }
  const detectedProvider = $derived(detectProvider(keyInput));

  async function claim() {
    const name = ownerInput.trim();
    if (busy || name.length === 0) return;
    busy = true;
    flash = null;
    try {
      const packet = await client.write(
        "/install/owner-claims.json",
        { owner_name: name, actor_id: actor },
        { receiptIntent: "install_owner_claim" }
      );
      if (packet?.claimed) {
        flash = packet.already_claimed
          ? { ok: true, line: `This install is already owned by ${packet.owner_name}.` }
          : { ok: true, line: "Done. This install is yours, on the record." };
        ownerInput = "";
        await invalidateAll();
      } else {
        flash = { ok: false, line: packet?.reason ?? "That did not record. Try again." };
      }
    } catch (error) {
      flash = { ok: false, line: "Nothing was recorded - try again." };
    }
    busy = false;
  }

  async function connect() {
    const key = keyInput.trim();
    if (busy || key.length === 0) return;
    const provider = detectedProvider;
    if (!provider) {
      flash = { ok: false, line: "MDx could not identify that provider from the key. Open Model Center to choose it explicitly." };
      return;
    }
    busy = true;
    flash = null;
    try {
      const packet = await client.write(
        "/models/connect.json",
        { provider_id: provider.id, api_key: key, actor_id: actor, persist_key: true },
        { receiptIntent: "install_model_connect" }
      );
      if (packet?.status === "MODEL_CONNECTED") {
        liveAvailableModels = packet.available_models ?? [];
        flash = { ok: true, line: `Connected. ${packet.model_id} is live - MDx can do real work now.` };
        keyInput = "";
        await invalidateAll();
      } else {
        flash = { ok: false, line: packet?.reason ?? "That did not connect. Check the key and try again." };
      }
    } catch (error) {
      flash = { ok: false, line: "Nothing was recorded - try again." };
    }
    busy = false;
  }

  // Switch to another model the provider offers. No key is sent - the server
  // reuses the key already held in memory from the connect above.
  async function changeModel(model) {
    if (busy || !model || model === data.modelName) return;
    busy = true;
    flash = null;
    try {
      const packet = await client.write(
        "/models/connect.json",
        { provider_id: "xai", model_id: model, actor_id: actor },
        { receiptIntent: "install_model_connect" }
      );
      if (packet?.status === "MODEL_CONNECTED") {
        liveAvailableModels = packet.available_models ?? availableModels;
        flash = { ok: true, line: `Now using ${packet.model_id}.` };
        await invalidateAll();
      } else {
        flash = { ok: false, line: packet?.reason ?? "Could not switch model. Try again." };
      }
    } catch (error) {
      flash = { ok: false, line: "Nothing was recorded - try again." };
    }
    busy = false;
  }

  const steps = $derived([
    {
      title: "Running on this machine",
      body: "MDx is up locally - your data lives in a snapshot file on this computer, nothing in the cloud.",
      done: data.kernelUp
    },
    {
      title: "Claim this install",
      body: data.ownerClaimed
        ? `Owned by ${data.ownerName}, on the record.`
        : "Say who owns this MDx. No password, no account to create - just your name, recorded once.",
      done: data.ownerClaimed,
      claim: !data.ownerClaimed
    },
    {
      title: "Connect a model",
      body: data.modelConnected
        ? `${data.modelName || "A model"} is connected through Model Fabric - every MDx app now sees the same readiness.`
        : "Connect xAI, OpenAI, Anthropic, Google, or another supported provider once. MDx discovers a model and verifies it before any app can use it.",
      done: data.modelConnected,
      connect: data.ownerClaimed && !data.modelConnected,
      change: data.modelConnected
    }
  ]);

  // The payoff: kernel up, owner claimed, model connected - MDx is ready to use.
  const allSet = $derived(data.kernelUp && data.ownerClaimed && data.modelConnected);
</script>

<section class="welcome">
  <header>
    <p class="eyebrow">MDx</p>
    {#if allSet}
      <h1>You're all set.</h1>
      <p class="lede">Owned by {data.ownerName}, {data.modelName || "a model"} connected through Model Fabric. MDx is ready to work.</p>
    {:else}
      <h1>You're running MDx.</h1>
      {#if data.kernelUp}
        <p class="lede">It's live on this machine. Here's the short path to making it yours.</p>
      {:else}
        <p class="lede">The app is open, but it can't reach the MDx engine yet. Run <code>make setup</code> again, then refresh.</p>
      {/if}
    {/if}
  </header>

  <ol class="steps">
    {#each steps as step, i}
      <li class="step" class:done={step.done}>
        <span class="marker">{step.done ? "✓" : i + 1}</span>
        <div class="step-body">
          <strong>{step.title}</strong>
          <p>{step.body}</p>
          {#if step.claim}
            <form class="claim" onsubmit={(event) => { event.preventDefault(); claim(); }}>
              <input
                type="text"
                bind:value={ownerInput}
                placeholder="Your name"
                aria-label="Your name"
                maxlength="80"
              />
              <button type="submit" disabled={busy || ownerInput.trim().length === 0}>
                {busy ? "Claiming..." : "Claim it"}
              </button>
              {#if flash}<span class="flash" class:bad={!flash.ok}>{flash.line}</span>{/if}
            </form>
          {/if}
          {#if step.connect}
            <form class="claim" onsubmit={(event) => { event.preventDefault(); connect(); }}>
              <input
                type="password"
                bind:value={keyInput}
                placeholder="xAI API key"
                aria-label="xAI API key"
                autocomplete="off"
              />
              <button type="submit" disabled={busy || keyInput.trim().length === 0}>
                {busy ? "Connecting..." : "Connect"}
              </button>
              {#if flash}<span class="flash" class:bad={!flash.ok}>{flash.line}</span>{/if}
            </form>
          {/if}
          {#if step.change && availableModels.length > 1}
            <label class="change">
              <span>Model</span>
              <select
                value={data.modelName}
                disabled={busy}
                onchange={(event) => changeModel(event.currentTarget.value)}
              >
                {#each availableModels as model}
                  <option value={model}>{model}</option>
                {/each}
              </select>
              {#if flash}<span class="flash" class:bad={!flash.ok}>{flash.line}</span>{/if}
            </label>
          {/if}
        </div>
      </li>
    {/each}
  </ol>

  <MarketplacePackPrompt app="twin" objectId="first-run" task="starter pack" />

  <div class="foot">
    {#if allSet}
      <a class="enter" href="/welcome/path">Choose where to start</a>
      <span class="foot-note">Everything's connected. Pick a path, then Twin is home.</span>
    {:else}
      <a class="enter" href="/welcome/meet">Look around MDx</a>
      <span class="foot-note">A quick tour of the apps; setup picks up where you left off.</span>
    {/if}
  </div>
</section>

<style>
  .welcome {
    max-width: 40rem;
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
    max-width: 32rem;
  }
  .lede code {
    font-family: var(--mdx-font-mono);
    font-size: 0.9em;
    background: var(--mdx-surface-sunken);
    padding: 0.1rem 0.35rem;
    border-radius: var(--mdx-radius-sm);
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .step {
    display: flex;
    gap: 1rem;
    align-items: flex-start;
    padding: 1.1rem 1.25rem;
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
  }
  .step.done {
    border-color: var(--mdx-tone-ok-border, var(--mdx-border-default));
  }
  .marker {
    flex: 0 0 1.75rem;
    height: 1.75rem;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--mdx-surface-sunken);
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    font-weight: 600;
  }
  .step.done .marker {
    background: var(--mdx-tone-ok-text);
    color: var(--mdx-surface-base);
  }
  .step-body {
    flex: 1;
  }
  .step-body strong {
    color: var(--mdx-text-primary);
  }
  .step-body p {
    margin: 0.2rem 0 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
  }
  .claim {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
    margin-top: 0.75rem;
  }
  .claim input {
    flex: 1 1 12rem;
    padding: 0.5rem 0.7rem;
    background: var(--mdx-surface-base);
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    color: var(--mdx-text-primary);
    font: inherit;
  }
  .claim input:focus {
    outline: none;
    border-color: var(--mdx-accent-primary);
  }
  .claim button {
    padding: 0.5rem 1rem;
    background: var(--mdx-accent-primary);
    color: var(--mdx-surface-base);
    border: none;
    border-radius: var(--mdx-radius-pill);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .claim button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .change {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
    margin-top: 0.75rem;
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-secondary);
  }
  .change select {
    padding: 0.4rem 0.6rem;
    background: var(--mdx-surface-base);
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    color: var(--mdx-text-primary);
    font: inherit;
  }
  .change select:focus {
    outline: none;
    border-color: var(--mdx-accent-primary);
  }
  .flash {
    font-size: var(--mdx-text-sm);
    color: var(--mdx-tone-ok-text);
    flex-basis: 100%;
  }
  .flash.bad {
    color: var(--mdx-tone-danger-text);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 1rem;
    flex-wrap: wrap;
  }
  .enter {
    padding: 0.6rem 1.25rem;
    background: var(--mdx-accent-primary);
    color: var(--mdx-surface-base);
    border-radius: var(--mdx-radius-pill);
    font-weight: 600;
    text-decoration: none;
  }
  .foot-note {
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-tertiary);
  }
</style>
