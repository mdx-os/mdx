<script>
  // Set up MDx: the activation journey. One in-place flow that walks the five
  // receipt-backed steps (Twin knows you -> Models can run -> Forge workspace ->
  // Starter workspace -> First proof), posting to Codex's /activation/* contracts
  // and re-reading the projection so progress is real and resumable. The closing
  // act lights the flywheel with the seeded-but-honest starter workspace.
  import { invalidateAll } from "$app/navigation";
  import { roles, styleDimensions, beliefLine } from "../../../lib/youProfile.js";
  import { deriveRepoFields, repoKindOf } from "../../../lib/forgeRepo.js";
  import MissionFlow from "../../../lib/MissionFlow.svelte";
  import { activationStepEvent, telemetry } from "../../../lib/telemetry.js";
  import RepoConnect from "../../../lib/RepoConnect.svelte";

  let { data } = $props();

  const ACTOR = "human:local_user";
  const steps = $derived(data.activation?.progress?.steps ?? []);
  const lanes = $derived(data.activation?.model_lanes ?? {});
  const firstOpen = $derived(steps.find((s) => !s.done)?.key ?? "first_proof");

  let picked = $state("");
  const active = $derived(picked || firstOpen);
  const allDone = $derived(steps.length > 0 && steps.every((s) => s.done));
  const activeIndex = $derived(Math.max(0, steps.findIndex((s) => s.key === active)));
  // The REAL first mission takes over beats 4-5 (starter_workspace + first_proof)
  // when a model + Forge are ready; otherwise the seeded warm-up is the fallback.
  const fmRunnable = $derived(data.fm?.capabilities?.runnable_model === true && data.fm?.capabilities?.forge_builder_ready === true);
  const onMissionBeat = $derived(fmRunnable && (active === "starter_workspace" || active === "first_proof"));

  let busy = $state(false);
  let flash = $state(null);

  async function post(path, body) {
    busy = true;
    flash = null;
    const stepAtWrite = active;
    try {
      const res = await fetch(`/api/kernel${path}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ...body, actor_id: ACTOR })
      });
      const packet = await res.json().catch(() => ({}));
      busy = false;
      if (!res.ok || packet?.status === "REFUSED") {
        flash = { ok: false, line: packet?.reason ?? "That step did not land. Try again." };
        return false;
      }
      picked = "";
      // Funnel truth for the beta program: each completed beat emits a
      // Source-B activation_step (content-free) so drop-off is measurable.
      telemetry.record(activationStepEvent({ step: stepAtWrite, route: "/welcome/setup" }));
      await invalidateAll();
      return true;
    } catch (error) {
      busy = false;
      flash = { ok: false, line: "Nothing recorded - that did not land." };
      return false;
    }
  }

  // Step 1 - profile
  // Twin is a team of specialists (mirrors the /twin product + marketing page) -
  // shown as a light persona strip so the card has a face, not just fields.
  const TWIN_AGENTS = [
    { id: "advisor", name: "Advisor" },
    { id: "coach", name: "Coach" },
    { id: "architect", name: "Architect" },
    { id: "solver", name: "Solver" },
    { id: "compliance", name: "Compliance" },
    { id: "coder", name: "Coder" }
  ];
  let name = $state("");
  let role = $state("");
  let style = $state({ register: "", grounding: "", guidance: "" });
  // Name + role is the moment; the style prefs are an optional fine-tune.
  const profileReady = $derived(!!(name.trim() && role));
  const belief = $derived(beliefLine({ name, role, style }));
  async function saveProfile() {
    if (!profileReady) return;
    await post("/activation/profile.json", { name: name.trim(), role, style, voice: "" });
  }

  // Step 2 - model setup (three lanes; connect as many frontier keys as you want)
  let keyDraft = $state("");
  let addedKeys = $state([]);
  let usePrivate = $state(false);
  let privateInitialized = $state(false);
  let rememberOnMachine = $state(false);
  $effect(() => {
    if (!privateInitialized && data.activation?.model_lanes) {
      usePrivate = data.activation.model_lanes.ollama_runtime_detected === true;
      privateInitialized = true;
    }
  });
  const detected = $derived(detectProvider(keyDraft));
  function detectProvider(k) {
    const v = String(k ?? "").trim();
    if (!v) return "";
    if (v.startsWith("xai-")) return "xai";
    if (v.startsWith("sk-ant-")) return "anthropic";
    if (v.startsWith("sk-")) return "openai";
    if (v.startsWith("AIza")) return "gemini";
    return "";
  }
  function addKey() {
    if (!detected || addedKeys.some((k) => k.provider === detected)) return;
    addedKeys = [...addedKeys, { provider: detected, key: keyDraft.trim() }];
    keyDraft = "";
  }
  function removeKey(provider) {
    addedKeys = addedKeys.filter((k) => k.provider !== provider);
  }
  async function recordModelSetup(body) {
    const stepAtWrite = active;
    const res = await fetch("/api/kernel/activation/model-setup.json", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ...body, actor_id: ACTOR })
    });
    const packet = await res.json().catch(() => ({}));
    if (!res.ok || packet?.status === "REFUSED") {
      throw new Error(packet?.reason || "That model setup did not land.");
    }
    picked = "";
    telemetry.record(activationStepEvent({ step: stepAtWrite, route: "/welcome/setup" }));
    await invalidateAll();
    return packet;
  }
  const privateModel = $derived(lanes.recommendations?.find((r) => r.lane === "private_twin")?.model_id ?? "qwen3");
  const codingModel = $derived(lanes.recommendations?.find((r) => r.lane === "forge_coding")?.model_id ?? "qwen2.5-coder");
  const modelReady = $derived(addedKeys.length > 0 || (usePrivate && lanes.ollama_runtime_detected));
  async function saveModels() {
    if (!modelReady || busy) return;
    const providers = addedKeys.map((k) => ({ provider: k.provider, api_key: k.key }));
    const body = { providers, remember_on_machine: rememberOnMachine };
    if (usePrivate && lanes.ollama_runtime_detected) {
      body.private_twin_model = privateModel;
      body.local_coding_model = codingModel;
    }
    busy = true;
    flash = null;
    try {
      await recordModelSetup(body);
      const connected = [...addedKeys.map((entry) => entry.provider), ...(usePrivate ? ["ollama"] : [])];
      flash = {
        ok: true,
        line: `${connected.join(", ")} connected once through Model Fabric. Twin, Forge, web, macOS, and paired iPhone workflows now share that verified access.`
      };
    } catch (error) {
      flash = { ok: false, line: error?.message ?? "That model setup did not land." };
    } finally {
      busy = false;
    }
  }

  // Step 3 - forge workspace
  let useMdxSelf = $state(false);
  let connectedRepos = $state([]);
  let repoFlash = $state("");
  // Connective tissue from beat 2: which models you have, and what that unlocks.
  const hasFrontier = $derived(addedKeys.length > 0);
  const hasLocal = $derived(usePrivate && lanes.ollama_runtime_detected);
  const forgeNudge = $derived.by(() => {
    if (hasFrontier && hasLocal) return { tone: "ok", text: "You've got both - private models for sensitive work, frontier for the heavy lifting. Forge can power anything." };
    if (hasFrontier) return { tone: "ok", text: "Your frontier models will power heavy builds." };
    if (hasLocal) return { tone: "ok", text: `Forge will build with your local ${codingModel} - great for most work. Add a frontier key (step 2) for heavier lifting.` };
    return { tone: "warn", text: "Forge needs a model to build - add one back in step 2 (a frontier key, or your local runtime)." };
  });
  // What Forge can run - unlocked by the models above. Shown, not chosen.
  const forgeCaps = $derived([
    { label: "One focused build", on: hasLocal || hasFrontier },
    { label: "A few in parallel", on: hasLocal || hasFrontier },
    { label: "A full fleet", on: hasFrontier }
  ]);
  const forgeReady = $derived(hasLocal || hasFrontier);
  const defaultWidth = $derived(hasFrontier ? 8 : (hasLocal ? 4 : 1));
  function removeRepo(id) {
    connectedRepos = connectedRepos.filter((r) => r.id !== id);
  }
  async function saveForge() {
    await post("/activation/forge-workspace.json", { repo_ids: connectedRepos.map((r) => r.id), default_fleet_width: defaultWidth, use_mdx_self: useMdxSelf });
  }

  // Step 4/5 - seed + prove
  function newWorkspaceId() {
    return `starter_${Date.now().toString(36)}`;
  }
  // "Bring it to life" runs the whole lap: seed the cross-surface workspace, then
  // record the first-proof receipts - so beat 5 is purely the payoff.
  async function seedWorkspace() {
    const wsid = newWorkspaceId();
    const seeded = await post("/activation/starter-workspace.json", { starter_workspace_id: wsid });
    if (seeded) {
      await post("/activation/first-proof.json", { starter_workspace_id: wsid });
    }
  }
  async function runFirstProof() {
    const wsid = data.activation?.starter_workspace?.starter_workspace_id || newWorkspaceId();
    await post("/activation/first-proof.json", { starter_workspace_id: wsid });
  }

  const fw = $derived(data.flywheel ?? { outcomes: 0, lessons: 0, installs: 0, memories: 0 });
  const fwTotal = $derived(fw.outcomes + fw.lessons + fw.installs + fw.memories);

  // The cross-surface loop in human terms - the flywheel made visible. Each node
  // is receipt-backed: what Twin did becomes context the next surface builds on.
  const CHAIN = [
    { key: "twin_personalized", surface: "Twin", href: "/twin", did: "learned how you work and handled your private notes - right on your machine." },
    { key: "pages_source_available", surface: "Pages", href: "/pages", did: "turned that into a standard - a Page your team and every future build can draw on." },
    { key: "forge_workspace_ready", surface: "Forge", href: "/forge", did: "ran a build grounded in that standard, and left a lesson the next build will cite." },
    { key: "message_action_card_works", surface: "Message", href: "/message", did: "landed the result as a decision you can approve or steer - with its receipt." }
  ];
  const proofFor = (key) => (data.activation?.proofs ?? []).find((p) => p.key === key);
</script>

<svelte:head><title>Set up MDx</title></svelte:head>

<section class="setup" data-route-state="ready">
  {#snippet agentIcon(id)}
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      {#if id === "advisor"}<circle cx="12" cy="12" r="9"/><polygon points="16 8 13 13 8 16 11 11"/>
      {:else if id === "coach"}<path d="M21 11.5a8 8 0 0 1-11.5 7.2L4 20l1.3-5.5A8 8 0 1 1 21 11.5z"/>
      {:else if id === "architect"}<path d="M3 21h18"/><path d="M5 21V7l7-4 7 4v14"/><path d="M9 21v-6h6v6"/>
      {:else if id === "solver"}<path d="M9 18h6"/><path d="M10 22h4"/><path d="M12 2a7 7 0 0 0-4 12.7c.6.5 1 1.2 1 2h6c0-.8.4-1.5 1-2A7 7 0 0 0 12 2z"/>
      {:else if id === "compliance"}<path d="M12 2 4 5v6c0 5 3.4 8.6 8 11 4.6-2.4 8-6 8-11V5l-8-3z"/><path d="m9 12 2 2 4-4"/>
      {:else if id === "coder"}<polyline points="16 18 22 12 16 6"/><polyline points="8 6 2 12 8 18"/>
      {/if}
    </svg>
  {/snippet}

  {#snippet chainView(preview = false)}
    <ol class="chain">
      {#each CHAIN as node, i (node.key)}
        {@const pf = preview ? null : proofFor(node.key)}
        <li class="chain-node" data-lit={pf?.satisfied ? "true" : "false"}>
          <span class="chain-dot">{pf?.satisfied ? "✓" : i + 1}</span>
          <div class="chain-body">
            <div class="chain-top">
              <strong>{node.surface}</strong>
              {#if pf?.satisfied}<a class="chain-go" href={node.href}>see it →</a>{/if}
            </div>
            <p>{node.did}</p>
          </div>
        </li>
      {/each}
    </ol>
  {/snippet}

  {#if onMissionBeat}
    <MissionFlow initial={data.fm} repos={data.repos} />
  {:else}
  <div class="setup-inner">
    <div class="setup-progress">
      <span class="eyebrow">Set up MDx</span>
      <span class="setup-count">{allDone ? "All set" : `Step ${activeIndex + 1} of ${steps.length}`}</span>
    </div>

    <div class="card">
    {#if steps.length === 0}
      <h2>MDx isn't reachable right now</h2>
      <p class="panel-sub">
        The local server isn't answering, so setup can't read where you left off - nothing is lost.
        Start it from the MDx repo and this page picks up exactly where you were:
      </p>
      <p class="panel-sub"><code>sh scripts/dogfood-stack.sh</code></p>
    {:else if active === "profile"}
      <div class="tt-agents" aria-label="Your Twin is a team of specialists">
        {#each TWIN_AGENTS as a (a.id)}
          <span class="tt-agent">{@render agentIcon(a.id)}{a.name}</span>
        {/each}
      </div>
      <h2>Meet your Twin</h2>
      <p class="panel-sub">A team of specialists that thinks with you, gets things done, and points work to the right place. Tell Twin a little about you, and it'll sound like it knows you from your first message.</p>
      <div class="field"><label class="flabel" for="su-name">What should Twin call you?</label><input id="su-name" class="finput" bind:value={name} placeholder="Your name" autocomplete="off" /></div>
      <div class="field"><span class="flabel">What's your world?</span><div class="chips">{#each roles as r (r.id)}<button type="button" class="chip" class:active={role === r.id} onclick={() => (role = r.id)}><strong>{r.label}</strong></button>{/each}</div></div>

      <div class="finetune">
        <span class="ft-head">Fine-tune how it talks <span>optional</span></span>
        {#each styleDimensions as dim (dim.id)}
          <div class="ft-row"><span class="ft-label">{dim.label}</span><div class="chips">{#each dim.options as opt (opt.id)}<button type="button" class="chip sm" class:active={style[dim.id] === opt.id} onclick={() => (style = { ...style, [dim.id]: opt.id })}>{opt.label}</button>{/each}</div></div>
        {/each}
      </div>

      {#if belief}
        <div class="twin-preview" aria-live="polite">
          <span class="tp-label">So Twin knows</span>
          <p>{belief}</p>
        </div>
      {/if}
      <button type="button" class="mdx-btn primary go" disabled={!profileReady || busy} onclick={saveProfile}>{busy ? "Saving..." : "This is me →"}</button>

    {:else if active === "model_setup"}
      <h2>Give it power</h2>
      <p class="panel-sub">A model is the engine - it's what lets Twin think and your agents build. Pick how it runs. You can use one, or both.</p>

      {#if lanes.ollama_runtime_detected}
        <p class="detected-note"><span class="dn-dot" aria-hidden="true"></span>We spotted a private model runtime on your machine - so you're already set to run locally. Add a frontier key below too, if you want more muscle.</p>
      {/if}

      <div class="lanes">
        {#if lanes.ollama_runtime_detected}
          <button type="button" class="powerlane" class:active={usePrivate} onclick={() => (usePrivate = !usePrivate)}>
            <span class="pl-icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></svg></span>
            <span class="pl-body">
              <span class="pl-top"><strong>Private, on this machine</strong><span class="pl-badge">detected</span></span>
              <span class="pl-line">Runs right here - {privateModel} powers Twin, {codingModel} powers your builds. Nothing leaves your machine, and you don't need an account.</span>
              {#if usePrivate}<span class="pl-note">First run downloads the models (a few GB, a minute). After that, instant and offline.</span>{/if}
            </span>
            <span class="pl-check">{#if usePrivate}<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m5 12 5 5L20 7"/></svg>{/if}</span>
          </button>
        {:else}
          <div class="powerlane powerlane-static">
            <span class="pl-icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></svg></span>
            <span class="pl-body">
              <span class="pl-top"><strong>Private, on this machine</strong></span>
              <span class="pl-line">Install Ollama and a local model to run privately with no account - we'll detect it here.</span>
              <span class="local-setup">
                <span>Fast local path</span>
                <code>brew install ollama</code>
                <code>ollama serve</code>
                <code>ollama pull {privateModel}</code>
              </span>
            </span>
          </div>
        {/if}

        <div class="powerlane powerlane-static" class:active={addedKeys.length > 0}>
          <div class="pl-icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M13 2 4 14h7l-1 8 9-12h-7l1-8z"/></svg></div>
          <div class="pl-body">
            <div class="pl-top"><strong>Frontier models</strong>{#if addedKeys.length}<span class="pl-badge">on</span>{:else}<span class="pl-sub">most power</span>{/if}</div>
            <div class="pl-line">Connect a provider key for the strongest models - heavy coding and deep reasoning. Add as many as you like.</div>
            {#if addedKeys.length}
              <div class="key-chips">
                {#each addedKeys as k (k.provider)}
                  <span class="key-chip">{k.provider}<button type="button" aria-label={`Remove ${k.provider}`} onclick={() => removeKey(k.provider)}>&times;</button></span>
                {/each}
              </div>
            {/if}
            <div class="key-row">
              <input id="su-key" class="finput" type="password" bind:value={keyDraft} placeholder="Paste a key - Anthropic, OpenAI, xAI, or Gemini" autocomplete="off" onkeydown={(e) => { if (e.key === "Enter") addKey(); }} />
              <button type="button" class="mdx-btn secondary key-add" disabled={!detected} onclick={addKey}>Add</button>
            </div>
            <p class="fhint">{detected ? `Detected ${detected} - click Add.` : keyDraft.trim() ? "That prefix isn't one we recognize (Anthropic, OpenAI, xAI, Gemini)." : "One key per provider. xAI connects Twin here; the other providers are stored for Forge until their Twin GUI turn-on lands."}</p>
            <label class="remember">
              <input type="checkbox" bind:checked={rememberOnMachine} />
              <span>
                <strong>Remember these keys on this machine</strong>
                <small>Stored in your OS keychain. Leave off to keep them just for this session.</small>
              </span>
            </label>
          </div>
        </div>
      </div>

      <button type="button" class="mdx-btn primary go" disabled={!modelReady || busy} onclick={saveModels}>{busy ? "Connecting..." : "Continue →"}</button>

    {:else if active === "forge_workspace"}
      <h2>Set up Forge</h2>
      <p class="panel-sub">Forge is where your agents write code. It works in a safe copy of your repo and always brings you a branch to review - nothing ships without your say-so.</p>

      <p class="detected-note"><span class="dn-dot {forgeNudge.tone}" aria-hidden="true"></span>{forgeNudge.text}</p>
      {#if !forgeReady}
        <button type="button" class="back-to-models" onclick={() => (picked = "model_setup")}>Back to models</button>
      {/if}

      <div class="lanes">
        <button type="button" class="powerlane" class:active={useMdxSelf} onclick={() => (useMdxSelf = !useMdxSelf)}>
          <span class="pl-icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91-.09z"/><path d="m12 15-3-3a22 22 0 0 1 2-3.95A12.88 12.88 0 0 1 22 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 0 1-4 2z"/><path d="M9 12H4s.55-3.03 2-4c1.62-1.08 5 0 5 0"/><path d="M12 15v5s3.03-.55 4-2c1.08-1.62 0-5 0-5"/></svg></span>
          <span class="pl-body">
            <span class="pl-top"><strong>Start on MDx itself</strong><span class="pl-sub">no setup</span></span>
            <span class="pl-line">The quickest way to see Forge work - it builds on MDx's own code, in a safe copy. No repo of your own needed.</span>
          </span>
          <span class="pl-check">{#if useMdxSelf}<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m5 12 5 5L20 7"/></svg>{/if}</span>
        </button>

        <div class="powerlane powerlane-static" class:active={connectedRepos.length > 0}>
          <div class="pl-icon"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="6" y1="3" x2="6" y2="15"/><circle cx="18" cy="6" r="3"/><circle cx="6" cy="18" r="3"/><path d="M18 9a9 9 0 0 1-9 9"/></svg></div>
          <div class="pl-body">
            <div class="pl-top"><strong>Connect your code</strong>{#if connectedRepos.length}<span class="pl-badge">on</span>{/if}</div>
            <div class="pl-line">Choose repositories through GitHub on hosted MDx, or use a URL or local folder on your Mac. Forge works in a safe copy and brings you a branch to review.</div>
            {#if connectedRepos.length}
              <div class="key-chips">
                {#each connectedRepos as r (r.id)}
                  <span class="key-chip">{r.label}<button type="button" aria-label={`Remove ${r.label}`} onclick={() => removeRepo(r.id)}>&times;</button></span>
                {/each}
              </div>
            {/if}
            <div class="key-row">
              <RepoConnect
                actor={ACTOR}
                compact
                hosted={data.session?.authenticated === true}
                onConnected={(repoId, label) => {
                  if (!connectedRepos.some((r) => r.id === repoId)) {
                    connectedRepos = [...connectedRepos, { id: repoId, label }];
                  }
                  repoFlash = "";
                }}
              />
            </div>
            <p class="fhint" class:bad={!!repoFlash}>{repoFlash || (data.repoCount > 0 ? `${data.repoCount} already connected. Add more, or skip - you can connect from Forge anytime.` : "Optional - you can skip this and connect from Forge anytime.")}</p>
          </div>
        </div>
      </div>

      <div class="finetune">
        <span class="ft-head">What Forge can take on</span>
        <div class="caps-row">
          {#each forgeCaps as c (c.label)}
            <div class="cap" data-on={c.on}>
              <span class="cap-mark">
                {#if c.on}
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m5 12 5 5L20 7"/></svg>
                {:else}
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="5" y="11" width="14" height="10" rx="2"/><path d="M8 11V7a4 4 0 0 1 8 0v4"/></svg>
                {/if}
              </span>
              <span class="cap-label">{c.label}</span>
              {#if !c.on}<span class="cap-note">needs a frontier key</span>{/if}
            </div>
          {/each}
        </div>
      </div>

      <button type="button" class="mdx-btn primary go" disabled={busy || !forgeReady} onclick={saveForge}>{busy ? "Saving..." : forgeReady ? "Continue →" : "Connect a model first"}</button>

    {:else if active === "starter_workspace"}
      <h2>Bring it to life</h2>
      <p class="panel-sub">Twin knows you, your models are wired, and Forge is ready. Let's run the whole loop once, end to end - so you can see how it all fits together. Think of it as a warm-up; your real work takes over from here.</p>
      {@render chainView(true)}
      <button type="button" class="mdx-btn primary go" disabled={busy} onclick={seedWorkspace}>{busy ? "Bringing it to life..." : "Bring it to life →"}</button>

    {:else}
      <h2>{allDone ? "It's alive." : "Almost there"}</h2>
      {#if !allDone}
        <p class="panel-sub">The loop's set up - one last step to put it on the record.</p>
        {@render chainView()}
        <button type="button" class="mdx-btn primary go" disabled={busy} onclick={runFirstProof}>{busy ? "Finishing..." : "Finish →"}</button>
      {:else}
        <p class="panel-sub">It ran end to end - and every step left a receipt. Here's the loop you just watched:</p>
        {@render chainView()}
        <div class="compound">
          <span class="compound-head">And here's the good part</span>
          <p>Each run leaves something the next one builds on - the lesson Forge wrote, the standard now in Memory. Nothing's lost, so the whole thing gets sharper the more you use it. That's the flywheel.</p>
          <p class="fw-mini">It already left a build, a lesson, and a capability behind - all real, all yours to build on.</p>
        </div>
        <p class="po-note">This first run was a warm-up - your real work writes over it from here.</p>
        <div class="po-cta">
          <a class="mdx-btn primary" href="/twin">Start working →</a>
          <a class="po-skip" href="/forge">or open Forge</a>
        </div>
      {/if}
    {/if}

    {#if flash}<p class="flash" class:bad={!flash.ok}>{flash.line}</p>{/if}
    </div>

    <ol class="dots" aria-label="Setup progress">
      {#each steps as s, i (s.key)}
        <li>
          <button
            type="button"
            class="dot"
            data-done={s.done}
            data-active={!allDone && active === s.key}
            disabled={!s.done && active !== s.key && i > 0 && !steps[i - 1].done}
            onclick={() => (picked = s.key)}
            aria-label={s.label}
            title={s.label}
          ></button>
        </li>
      {/each}
    </ol>
  </div>
  {/if}
</section>

<style>
  .setup { flex: 1; display: flex; flex-direction: column; align-items: center; padding: 24px; }
  .setup-inner { width: 100%; max-width: 540px; margin: auto 0; }
  .eyebrow { font-size: 11px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; color: var(--mdx-text-muted); }
  .setup-progress { display: flex; align-items: baseline; justify-content: space-between; margin-bottom: 14px; }
  .setup-count { font-size: 12px; color: var(--mdx-text-muted); font-variant-numeric: tabular-nums; }
  .card { padding: 32px 34px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-lg); background: var(--mdx-surface-raised); box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card); }
  .card h2 { margin: 0; font-family: var(--mdx-font-display); font-size: 23px; font-weight: 700; letter-spacing: -0.01em; }
  .panel-sub { margin: 8px 0 0; color: var(--mdx-text-secondary); font-size: 14px; line-height: 1.55; }
  .dots { list-style: none; margin: 22px 0 0; padding: 0; display: flex; justify-content: center; gap: 9px; }
  .dots li { display: flex; }
  .dot { width: 8px; height: 8px; border-radius: 50%; border: none; padding: 0; background: var(--mdx-border-default); cursor: pointer; transition: background 0.18s ease, width 0.18s ease, border-radius 0.18s ease; }
  .dot:disabled { cursor: default; }
  .dot[data-done="true"] { background: var(--mdx-accent-success); }
  .dot[data-active="true"] { width: 22px; border-radius: 4px; background: var(--mdx-accent-primary); }
  .field { margin-top: 20px; }
  .flabel { display: block; margin-bottom: 8px; font-size: 13px; font-weight: 600; }
  .finput { width: 100%; height: 42px; box-sizing: border-box; border: 1px solid var(--mdx-border-default); border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); color: var(--mdx-text-primary); padding: 0 13px; font: inherit; font-size: 14px; outline: none; }
  .finput:focus-visible { border-color: var(--mdx-accent-primary); }
  .fhint { margin: 6px 0 0; font-size: 12px; color: var(--mdx-text-muted); }
  .fhint.bad { color: var(--mdx-accent-error); }
  .caps-row { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; margin-top: 13px; }
  .cap { display: flex; flex-direction: column; align-items: flex-start; gap: 8px; padding: 13px 14px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); }
  .cap[data-on="true"] { border-color: color-mix(in srgb, var(--mdx-accent-success) 32%, var(--mdx-border-subtle)); }
  .cap-mark { display: inline-flex; width: 22px; height: 22px; align-items: center; justify-content: center; border-radius: 50%; }
  .cap[data-on="true"] .cap-mark { background: var(--mdx-accent-success); color: #fff; }
  .cap[data-on="false"] .cap-mark { color: var(--mdx-text-muted); border: 1px solid var(--mdx-border-subtle); }
  .cap-mark svg { width: 13px; height: 13px; }
  .cap-label { font-size: 13px; font-weight: 600; line-height: 1.25; }
  .cap[data-on="false"] .cap-label { color: var(--mdx-text-muted); }
  .cap-note { font-size: 11px; color: var(--mdx-text-muted); }
  @media (max-width: 480px) { .caps-row { grid-template-columns: 1fr; } }
  .key-chips { display: flex; flex-wrap: wrap; gap: 7px; margin-bottom: 9px; }
  .key-chip { display: inline-flex; align-items: center; gap: 6px; padding: 4px 6px 4px 12px; border-radius: var(--mdx-radius-pill, 999px); background: color-mix(in srgb, var(--mdx-accent-success) 14%, transparent); color: var(--mdx-accent-success); font-size: 12.5px; font-weight: 600; }
  .key-chip button { border: none; background: none; color: inherit; opacity: 0.7; font-size: 14px; line-height: 1; cursor: pointer; padding: 0 2px; }
  .key-chip button:hover { opacity: 1; }
  .key-row { display: flex; gap: 8px; }
  .key-row .finput { flex: 1; }
  .key-add { flex: none; height: 42px; }
  .tt-agents { display: flex; flex-wrap: wrap; gap: 7px; margin-bottom: 22px; }
  .tt-agent { display: inline-flex; align-items: center; gap: 7px; padding: 5px 13px 5px 10px; border-radius: var(--mdx-radius-pill, 999px); background: var(--mdx-surface-base); border: 1px solid var(--mdx-border-subtle); font-size: 12.5px; font-weight: 550; color: var(--mdx-text-secondary); }
  .tt-agent svg { width: 14px; height: 14px; flex: none; color: var(--mdx-accent-primary); }
  .chips { display: flex; flex-wrap: wrap; gap: 7px; }
  .chip { padding: 8px 14px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); color: var(--mdx-text-secondary); font: inherit; font-size: 13px; font-weight: 550; cursor: pointer; transition: border-color 0.14s ease, background 0.14s ease, color 0.14s ease, transform 0.08s ease; }
  .chip:hover { border-color: color-mix(in srgb, var(--mdx-accent-primary) 45%, var(--mdx-border-subtle)); color: var(--mdx-text-primary); }
  .chip:active { transform: translateY(1px); }
  .chip.active { border-color: var(--mdx-accent-primary); background: color-mix(in srgb, var(--mdx-accent-primary) 14%, transparent); color: var(--mdx-text-primary); }
  .chip.sm { padding: 6px 12px; font-size: 12.5px; }
  .finetune { margin-top: 26px; padding-top: 20px; border-top: 1px solid var(--mdx-border-subtle); }
  .ft-head { display: flex; align-items: baseline; gap: 8px; font-size: 11.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.07em; color: var(--mdx-text-muted); }
  .ft-head span { font-size: 11px; font-weight: 400; text-transform: none; letter-spacing: 0; }
  .ft-row { display: flex; align-items: center; gap: 14px; margin-top: 14px; flex-wrap: wrap; }
  .ft-label { flex: none; width: 132px; font-size: 12.5px; color: var(--mdx-text-secondary); }
  .detected-note { margin: 16px 0 0; display: flex; gap: 9px; align-items: baseline; font-size: 12.5px; color: var(--mdx-text-secondary); line-height: 1.5; }
  .dn-dot { flex: none; width: 6px; height: 6px; border-radius: 50%; background: var(--mdx-accent-success); transform: translateY(-1px); }
  .dn-dot.ok { background: var(--mdx-accent-success); }
  .dn-dot.warn { background: var(--mdx-accent-warning); }
  .back-to-models { margin: 10px 0 0 15px; border: none; background: none; color: var(--mdx-accent-primary); font: inherit; font-size: 12.5px; font-weight: 650; cursor: pointer; padding: 0; }
  .back-to-models:hover { text-decoration: underline; }
  .lanes { display: grid; gap: 12px; margin-top: 18px; }
  .powerlane { display: flex; gap: 14px; width: 100%; text-align: left; padding: 16px 18px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-lg); background: var(--mdx-surface-base); color: inherit; font: inherit; cursor: pointer; transition: border-color 0.14s ease, background 0.14s ease; }
  .powerlane:hover { border-color: color-mix(in srgb, var(--mdx-accent-primary) 35%, var(--mdx-border-subtle)); }
  .powerlane.active { border-color: var(--mdx-accent-primary); background: color-mix(in srgb, var(--mdx-accent-primary) 7%, var(--mdx-surface-base)); }
  .powerlane-static { cursor: default; }
  .powerlane-static:not(.active):hover { border-color: var(--mdx-border-subtle); }
  .pl-icon { flex: none; display: inline-flex; width: 38px; height: 38px; align-items: center; justify-content: center; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-primary) 12%, transparent); color: var(--mdx-accent-primary); }
  .pl-icon svg { width: 19px; height: 19px; }
  .pl-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px; }
  .pl-top { display: flex; align-items: center; gap: 9px; }
  .pl-top strong { font-size: 14.5px; font-weight: 650; }
  .pl-badge { font-size: 10.5px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.04em; color: var(--mdx-accent-success); }
  .pl-sub { font-size: 11.5px; color: var(--mdx-text-muted); }
  .pl-line { font-size: 13px; color: var(--mdx-text-secondary); line-height: 1.5; }
  .pl-note { font-size: 12px; color: var(--mdx-text-muted); }
  .local-setup { display: grid; gap: 6px; margin-top: 4px; }
  .local-setup span { font-size: 11px; font-weight: 700; color: var(--mdx-text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
  .local-setup code { display: block; width: fit-content; max-width: 100%; padding: 5px 8px; border-radius: var(--mdx-radius-sm, 6px); background: color-mix(in srgb, var(--mdx-text-primary) 7%, transparent); color: var(--mdx-text-primary); font-size: 12px; overflow-wrap: anywhere; }
  .pl-check { flex: none; align-self: center; width: 22px; height: 22px; display: inline-flex; align-items: center; justify-content: center; color: var(--mdx-accent-primary); }
  .pl-check svg { width: 19px; height: 19px; }
  .twin-preview { margin: 22px 0 0; padding: 13px 15px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-primary) 8%, transparent); border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 22%, transparent); }
  .tp-label { font-size: 11px; font-weight: 700; letter-spacing: 0.05em; text-transform: uppercase; color: var(--mdx-accent-primary); }
  .twin-preview p { margin: 4px 0 0; font-size: 13.5px; color: var(--mdx-text-primary); line-height: 1.5; }
  .remember { margin-top: 12px; display: flex; gap: 10px; align-items: flex-start; color: var(--mdx-text-secondary); font-size: 13px; cursor: pointer; }
  .remember input { margin-top: 3px; flex: none; }
  .remember span { display: grid; gap: 2px; }
  .remember strong { color: var(--mdx-text-primary); font-weight: 650; }
  .remember small { color: var(--mdx-text-muted); font-size: 12px; line-height: 1.35; }
  .go { margin-top: 22px; height: 42px; padding: 0 22px; font-weight: 650; }
  .compound { margin: 22px 0 0; padding: 18px 20px; border-radius: var(--mdx-radius-lg); background: color-mix(in srgb, var(--mdx-accent-primary) 6%, var(--mdx-surface-base)); border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 18%, transparent); }
  .compound-head { font-size: 11px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: var(--mdx-accent-primary); }
  .compound p { margin: 7px 0 0; font-size: 13.5px; color: var(--mdx-text-secondary); line-height: 1.55; }
  .fw-mini { margin: 13px 0 0; font-size: 12.5px; color: var(--mdx-text-muted); line-height: 1.5; }
  .chain { list-style: none; margin: 20px 0 0; padding: 0; }
  .chain-node { position: relative; display: flex; gap: 13px; padding: 0 0 18px; }
  .chain-node:last-child { padding-bottom: 0; }
  .chain-node:not(:last-child) .chain-dot::after { content: ""; position: absolute; top: 24px; left: 11px; width: 2px; height: calc(100% - 24px); background: var(--mdx-border-subtle); }
  .chain-node[data-lit="true"]:not(:last-child) .chain-dot::after { background: color-mix(in srgb, var(--mdx-accent-success) 45%, var(--mdx-border-subtle)); }
  .chain-dot { position: relative; flex: none; width: 24px; height: 24px; border-radius: 50%; display: inline-flex; align-items: center; justify-content: center; font-size: 12px; font-weight: 700; background: var(--mdx-surface-base); border: 1px solid var(--mdx-border-subtle); color: var(--mdx-text-muted); }
  .chain-node[data-lit="true"] .chain-dot { background: var(--mdx-accent-success); border-color: var(--mdx-accent-success); color: #fff; }
  .chain-body { flex: 1; min-width: 0; padding-top: 1px; }
  .chain-node[data-lit="false"] .chain-body { opacity: 0.6; }
  .chain-top { display: flex; align-items: baseline; gap: 10px; }
  .chain-top strong { font-size: 14px; font-weight: 650; }
  .chain-go { color: var(--mdx-accent-primary); text-decoration: none; font-size: 12px; font-weight: 600; }
  .chain-go:hover { text-decoration: underline; }
  .chain-body p { margin: 3px 0 0; font-size: 13px; color: var(--mdx-text-secondary); line-height: 1.5; }
  .po-note { margin: 16px 0 0; font-size: 12.5px; color: var(--mdx-text-muted); }
  .po-cta { margin: 24px 0 0; display: flex; align-items: center; gap: 18px; }
  .po-skip { color: var(--mdx-text-muted); font-size: 13px; text-decoration: none; }
  .po-skip:hover { color: var(--mdx-text-primary); }
  .flash { margin: 14px 0 0; font-size: 12.5px; color: var(--mdx-accent-primary); }
  .flash.bad { color: var(--mdx-accent-error); }
</style>
