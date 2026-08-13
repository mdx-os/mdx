<script>
  // Which model is good at what - the running answer, derived entirely
  // from the work itself. Every agent run leaves receipts; this page
  // folds them per model: who finishes, who runs out of budget, who
  // quits, what it costs. Same-task comparisons (the A/B slots) are the
  // fair reads; mixed workloads are directional.
  import ForgeView from "../../../lib/ForgeView.svelte";

  let { data } = $props();
  let providerRoutingState = $state(null);
  let roleSaving = $state("");
  let profileSaving = $state("");
  let providerResult = $state(null);
  // Basic is the default and matches Welcome's one-key promise. Power setup is
  // opt-in: presets first, the resulting cast read-only underneath.
  let powerOpen = $state(false);
  let activePreset = $state("premier");

  $effect(() => {
    if (!providerRoutingState) providerRoutingState = data.providerRouting;
  });

  // The fair-attempt view, by kind of work, grouped per model - the same
  // evidence the planner casts on, so you see which model is good at WHAT.
  const segmentsByModel = $derived(
    (Array.isArray(data.scorecard?.by_work_type) ? data.scorecard.by_work_type : []).reduce(
      (acc, row) => {
        const model = String(row?.model ?? "");
        (acc[model] ??= []).push({
          workType: String(row?.work_type ?? ""),
          runs: Number(row?.runs ?? 0),
          done: Number(row?.done ?? 0),
          doneRate: Number(row?.done_rate ?? 0),
          avgTurns: Number(row?.avg_turns ?? 0),
          tokens: Number(row?.input_tokens ?? 0) + Number(row?.output_tokens ?? 0)
        });
        return acc;
      },
      {}
    )
  );

  const WORK_TYPE_LABEL = {
    focused: "Focused (1-2 files)",
    multi_file: "Multi-file (3-5)",
    cross_cutting: "Cross-cutting",
    no_edit: "No edits"
  };
  function workTypeLabel(t) {
    return WORK_TYPE_LABEL[t] ?? t;
  }
  function confidence(runs) {
    return runs === 1 ? "1 run - noisy" : runs <= 4 ? `${runs} runs - small sample` : `${runs} runs`;
  }

  const models = $derived(
    (Array.isArray(data.scorecard?.models) ? data.scorecard.models : []).map((row) => ({
      model: String(row?.model ?? ""),
      runs: Number(row?.runs ?? 0),
      finished: Number(row?.finished_runs ?? 0),
      done: Number(row?.done ?? 0),
      cannotProceed: Number(row?.cannot_proceed ?? 0),
      budgetExhausted: Number(row?.budget_exhausted ?? 0),
      stopped: Number(row?.stopped ?? 0),
      errored: Number(row?.errored ?? 0),
      doneRate: Number(row?.done_rate ?? 0),
      avgTurns: Number(row?.avg_turns ?? 0),
      inputTokens: Number(row?.input_tokens ?? 0),
      outputTokens: Number(row?.output_tokens ?? 0),
      segments: (segmentsByModel[String(row?.model ?? "")] ?? []).sort(
        (a, b) =>
          (a.workType === "no_edit" ? 1 : 0) - (b.workType === "no_edit" ? 1 : 0) || b.runs - a.runs
      )
    })).sort((a, b) => b.doneRate - a.doneRate || b.runs - a.runs)
  );

  function tokens(n) {
    return n >= 1_000_000 ? `${(n / 1_000_000).toFixed(1)}M` : n >= 1000 ? `${Math.round(n / 1000)}k` : String(n);
  }

  // Same task, different coders - the A/B/C/D builder-slot lever, the fairest
  // read of who is better. Each comparison is one work item built by 2+ models.
  const RUN_OUTCOME = {
    RUN_FINISHED_DONE: { label: "finished clean", tone: "ok" },
    RUN_FINISHED_NO_CHANGE: { label: "no change produced", tone: "warn" },
    RUN_FINISHED_CANNOT_PROCEED: { label: "could not proceed", tone: "warn" },
    RUN_BUDGET_EXHAUSTED: { label: "ran out of turns", tone: "warn" },
    RUN_STOPPED: { label: "stopped", tone: "" },
    RUN_ERROR: { label: "errored", tone: "warn" }
  };
  function outcome(status) {
    return RUN_OUTCOME[status] ?? { label: "working", tone: "" };
  }
  function taskName(workItem) {
    return String(workItem ?? "").replace(/^work_item_/, "").replace(/_/g, " ");
  }
  const comparisons = $derived(
    (Array.isArray(data.scorecard?.ab_comparisons) ? data.scorecard.ab_comparisons : []).map((c) => ({
      task: taskName(c?.work_item),
      runs: (Array.isArray(c?.runs) ? c.runs : []).map((r) => ({
        model: String(r?.model ?? ""),
        status: String(r?.status ?? ""),
        turns: Number(r?.turns ?? 0),
        tokens: Number(r?.tokens ?? 0)
      }))
    }))
  );

  // The eval lane: the senior-engineer benchmark. How ready Forge is for
  // serious classes of work, scored on one corpus across approved provider
  // profiles. Honest framing only - "covered by the eval lane" and "local
  // proof", never "scores well" or "proven live" until live runner receipts
  // exist. Fails soft to null and the section renders an honest "not measured
  // yet" note.
  const CLASS_LABEL = {
    bug_fix: "Bug fix",
    feature: "Feature",
    refactor: "Refactor with tests",
    security: "Security fix",
    ci_repair: "CI repair",
    docs_code: "Docs plus code",
    docs_plus_code: "Docs plus code",
    multi_file: "Multi-file change",
    product_ux: "Product workflow",
    constrained_multi_file: "Constrained multi-file",
    architecture: "Architecture boundary",
    api_compatibility: "API compatibility",
    api_compat: "API compatibility",
    migration: "Data migration safety",
    performance: "Performance regression",
    concurrency: "Concurrency race",
    observability: "Observability failure",
    frontend: "Frontend workflow",
    frontend_workflow: "Frontend workflow",
    long_horizon: "Long-horizon multi-stage"
  };
  const DIMENSION_LABEL = {
    correctness: "Correctness",
    regression_test_quality: "Regression tests",
    patch_quality: "Patch quality",
    architecture_fit: "Architecture fit",
    security_and_policy: "Security and policy",
    maintainability: "Maintainability",
    observability: "Observability",
    performance: "Performance",
    migration_compatibility: "Migration and compatibility",
    migration_and_compatibility: "Migration and compatibility",
    cost_latency_budget: "Cost and latency",
    handoff_quality: "Handoff quality"
  };
  // Connect results in plain language with the next step, never a raw enum.
  function providerResultLine(result) {
    const who = humanize(result.providerFamily);
    const map = {
      SESSION_KEY_STORED: `${who} is connected. Forge can use this key now.`,
      CONNECT_FAILED: `Couldn't connect ${who}. Check the key and try again.`,
      PROVIDER_NOT_DETECTED: "Couldn't tell which provider that key is for. Pick one and try again.",
      ROLE_SLOT_STORED: `${who} is set for this role.`,
      ENGINEERING_PROFILE_STORED: `${who} is now the active engineering stack.`,
      PROFILE_ROUTE_FAILED: `Couldn't apply ${who}. Try again.`
    };
    return map[result.status] ?? (result.ok ? `${who} is connected.` : `Couldn't connect ${who}. Check the key and try again.`);
  }
  function humanize(value) {
    return String(value ?? "").replace(/_/g, " ").replace(/^\w/, (c) => c.toUpperCase());
  }
  const classLabel = (c) => CLASS_LABEL[c] ?? humanize(c);
  const dimensionLabel = (d) => DIMENSION_LABEL[d] ?? humanize(d);
  const PACK_LABEL = {
    "ios-xcode": "iOS Xcode",
    "swift-spm": "Swift package",
    "java-maven": "Java Maven",
    "gradle-jvm": "Gradle JVM",
    "android-gradle": "Android Gradle",
    dotnet: ".NET",
    node: "Node",
    "rust-cargo": "Rust Cargo",
    python: "Python",
    go: "Go"
  };
  const PROVIDER_LABEL = {
    gemini: "Gemini",
    anthropic: "Anthropic",
    xai: "xAI",
    aws_bedrock: "AWS Bedrock"
  };

  const evalLane = $derived.by(() => {
    const sb = data.scoreboard;
    if (!sb) return null;
    const tasks = Array.isArray(sb.benchmark_tasks) ? sb.benchmark_tasks : [];
    const classes = [...new Set(tasks.map((t) => String(t?.class ?? "")).filter(Boolean))];
    const dims = (Array.isArray(sb.scoring_dimensions) ? sb.scoring_dimensions : []).map((d) => ({
      id: String(d?.dimension_id ?? ""),
      failClosed: d?.fail_closed_if_missing === true
    }));
    const matrix = Array.isArray(sb.model_matrix) ? sb.model_matrix : [];
    const load = sb.parallel_fleet_load_proof ?? null;
    let loadLine = "";
    if (load) {
      const agents = Number(load.target_parallel_agents ?? 0);
      const jobs = Number(load.total_jobs ?? 0);
      const clauses = [`held ${agents} parallel workers across ${jobs} jobs`];
      if (load.invariants_held === true) clauses.push("never over-admitted, with per-tenant fairness preserved");
      if (load.shed_observed === true) clauses.push("overload shed safely");
      loadLine = `Fleet load proof: ${clauses.join(" - ")}.`;
    }
    return {
      taskCount: Number(sb.benchmark_task_count ?? tasks.length),
      classes,
      dims,
      providerCount: matrix.length,
      liveMeasured: sb.authority_boundary?.codex_cli_live_execution_allowed === true,
      loadLine
    };
  });

  const languagePacks = $derived.by(() => {
    const cards = Array.isArray(data.scoreboard?.language_pack_scorecards)
      ? data.scoreboard.language_pack_scorecards
      : [];
    return cards
      .map((card) => ({
        id: String(card?.language_pack_id ?? ""),
        repoFamily: String(card?.repo_family ?? ""),
        taskCount: Number(card?.task_count ?? 0),
        small: Number(card?.small_task_count ?? 0),
        medium: Number(card?.medium_task_count ?? 0),
        large: Number(card?.large_task_count ?? 0),
        visibleChecks: Number(card?.visible_check_count ?? 0),
        hiddenChecks: Number(card?.hidden_check_slot_count ?? 0),
        artifactExpectations: Number(card?.artifact_noise_expectation_count ?? 0),
        principalGates: Number(card?.principal_review_gate_count ?? 0),
        principalVerdictRequired: card?.principal_verdict_required === true,
        readyForLiveEval: card?.ready_for_live_eval === true
      }))
      .filter((card) => card.id)
      .sort((a, b) => Number(b.readyForLiveEval) - Number(a.readyForLiveEval) || a.id.localeCompare(b.id));
  });

  const providerPreflight = $derived.by(() => {
    const preflight = data.providerPreflight;
    if (!preflight) return null;
    const requirements = Array.isArray(preflight.requirements) ? preflight.requirements : [];
    return {
      status: String(preflight.status ?? "UNKNOWN"),
      providerCount: Number(preflight.provider_count ?? requirements.length),
      readyProviderCount: Number(preflight.ready_provider_count ?? 0),
      allCredentialsPresent: preflight.all_credentials_present === true,
      liveEvalRunsAllowed: preflight.live_eval_runs_allowed === true,
      requirements: requirements.map((r) => ({
        providerFamily: String(r?.provider_family ?? ""),
        profileId: String(r?.profile_id ?? ""),
        requiredEnvKeys: Array.isArray(r?.required_env_keys) ? r.required_env_keys.map(String) : [],
        credentialsPresent: r?.credentials_present === true,
        secretValueExposed: r?.secret_value_exposed === true
      }))
    };
  });

  const providerRouting = $derived.by(() => {
    const routing = providerRoutingState;
    if (!routing) return null;
    const providers = Array.isArray(routing.providers) ? routing.providers : [];
    const roles = Array.isArray(routing.roles) ? routing.roles : [];
    const slots = Array.isArray(routing.slots) ? routing.slots : [];
    const engineeringStack = routing.engineering_stack ?? {};
    return {
      connectedProviderCount: Number(routing.ready_provider_count ?? 0),
      providerCount: Number(routing.provider_count ?? providers.length),
      routingPolicyBasis: String(routing.routing_policy_basis ?? "seeded_role_preferences"),
      evalInformedRoutingReady: routing.eval_informed_routing_ready === true,
      engineeringStack: {
        recommendedProfileId: String(engineeringStack.recommended_profile_id ?? "premier"),
        activeProfileId: String(engineeringStack.active_profile_id ?? "custom"),
        defaultHarness: String(engineeringStack.default_harness ?? "mdx_native"),
        harnessSelectionIndependent: engineeringStack.harness_selection_independent === true,
        verificationAuthority: String(engineeringStack.verification_authority ?? "mdx_deterministic_gates"),
        premierHarnesses: Array.isArray(engineeringStack.premier_harnesses)
          ? engineeringStack.premier_harnesses.map(String)
          : [],
        profiles: (Array.isArray(engineeringStack.profiles) ? engineeringStack.profiles : []).map((profile) => ({
          id: String(profile?.profile_id ?? ""),
          label: String(profile?.label ?? ""),
          line: String(profile?.summary ?? ""),
          ready: profile?.ready === true,
          readySlotCount: Number(profile?.ready_slot_count ?? 0),
          requiredSlotCount: Number(profile?.required_slot_count ?? 0),
          assignments: Array.isArray(profile?.assignments) ? profile.assignments : []
        }))
      },
      providers: providers.map((provider) => ({
        providerFamily: String(provider?.provider_family ?? ""),
        label: String(provider?.label ?? provider?.provider_family ?? ""),
        envKey: String(provider?.env_key ?? ""),
        credentialAvailable: provider?.credential_available === true,
        sessionKeyAvailable: provider?.session_key_available === true,
        modelHint: String(provider?.model_hint ?? ""),
        nativeAdapterStatus: String(provider?.native_adapter_status ?? "")
      })),
      roles: roles.map((role) => ({
        role: String(role?.role ?? ""),
        slot: String(role?.slot ?? ""),
        providerFamily: String(role?.provider_family ?? ""),
        modelId: String(role?.model_id ?? ""),
        source: String(role?.source ?? role?.status ?? ""),
        openaiCompatible: role?.openai_compatible === true,
        credentialAvailable: role?.credential_available === true,
        privacyClass: String(role?.privacy_class ?? ""),
        assignedSlot: String(role?.assigned_slot ?? "")
      })),
      slots: slots.map((slot) => ({
        slot: String(slot?.slot ?? ""),
        providerFamily: String(slot?.provider_family ?? ""),
        providerKeyReady: slot?.provider_key_ready === true,
        fullEnvEndpointReady: slot?.full_env_endpoint_ready === true,
        privacyClass: String(slot?.privacy_class ?? "")
      }))
    };
  });

  // Friendly model names so a card reads "Grok (fast)" not "xai · GROK · slot_env".
  // Six contract roles fold into five human cards: planner and conductor share a
  // model and a job, so they share one "Plan & orchestrate" card.
  const PROVIDER_FRIENDLY = {
    anthropic: "Claude",
    openai: "GPT",
    xai: "Grok",
    gemini: "Gemini",
    bedrock: "Bedrock",
    ollama: "Local model"
  };
  const ROLE_CARDS = [
    { key: "plan", label: "Plan & orchestrate", roles: ["planner", "conductor"], tone: "strong" },
    { key: "build", label: "Build", roles: ["executor"], tone: "fast" },
    { key: "integrate", label: "Integrate", roles: ["integrator"], tone: "" },
    { key: "review", label: "Review", roles: ["reviewer"], tone: "strict" },
    { key: "evaluate", label: "Evaluate", roles: ["evaluator"], tone: "" }
  ];

  const roleCards = $derived.by(() => {
    const byRole = {};
    for (const r of providerRouting?.roles ?? []) byRole[r.role] = r;
    return ROLE_CARDS.map((card) => {
      const resolved = card.roles.map((r) => byRole[r]).find((r) => r?.credentialAvailable);
      const ready = Boolean(resolved);
      return {
        key: card.key,
        label: card.label,
        roles: card.roles,
        tone: card.tone,
        ready,
        provider: ready ? (PROVIDER_FRIENDLY[resolved.providerFamily] ?? resolved.providerFamily) : "",
        modelId: ready ? resolved.modelId : "",
        slot: ready ? resolved.slot : "",
        source: ready ? resolved.source : "",
        sovereign: ready && resolved.privacyClass === "sovereign"
      };
    });
  });

  // The senior advisor (ADR 0490) is a judgment role, not an execution role: a
  // top model consulted rarely on hard calls, never a builder. It sits next to
  // the role slots but stays out of the execution runnable count and presets.
  // Unassigned, it falls back to the strongest configured model; an explicit
  // assignment (assignedSlot) reads differently from that auto fallback.
  const advisorCard = $derived.by(() => {
    const resolved = (providerRouting?.roles ?? []).find((r) => r.role === "advisor");
    const ready = Boolean(resolved?.credentialAvailable);
    const assignedSlot = String(resolved?.assignedSlot ?? "");
    const assigned = assignedSlot !== "" && assignedSlot !== "AUTO";
    return {
      key: "advise",
      label: "Senior advisor",
      roles: ["advisor"],
      ready,
      assigned,
      slot: assignedSlot,
      provider: ready ? (PROVIDER_FRIENDLY[resolved.providerFamily] ?? resolved.providerFamily) : "",
      modelId: ready ? resolved.modelId : "",
      sovereign: ready && resolved.privacyClass === "sovereign"
    };
  });

  const providerRoutingSummary = $derived.by(() => {
    const buildReady = roleCards.find((card) => card.key === "build")?.ready === true;
    const runnableRoleCount = roleCards.filter((card) => card.ready).length;
    const connectedProviderCount = providerRouting?.connectedProviderCount ?? 0;
    return {
      buildReady,
      runnableRoleCount,
      connectedProviderCount,
      countLine: `${connectedProviderCount} ${connectedProviderCount === 1 ? "provider" : "providers"} connected`,
      routingBasis: String(providerRouting?.routingPolicyBasis ?? "seeded_role_preferences"),
      evalInformedRoutingReady: providerRouting?.evalInformedRoutingReady === true
    };
  });

  const modelAccessReady = $derived(data.modelReadiness?.ready === true);
  const canonicalConnectionCount = $derived(Number(data.modelReadiness?.ready_connection_count ?? 0));
  const forgeBuilderReadiness = $derived(
    (data.modelReadiness?.consumer_readiness ?? []).find(
      (workload) => workload?.workload_id === "mdx/forge/builder"
    ) ?? null
  );
  const forgeBuilderReady = $derived(forgeBuilderReadiness?.ready === true);

  const availableRoleSlots = $derived.by(() =>
    (providerRouting?.slots ?? [])
      .filter((slot) => slot.providerKeyReady || slot.fullEnvEndpointReady)
      .map((slot) => ({
        slot: slot.slot,
        label: `${slot.slot} · ${PROVIDER_FRIENDLY[slot.providerFamily] ?? slot.providerFamily}`
      }))
  );

  const PRESETS = $derived(providerRouting?.engineeringStack?.profiles ?? []);
  const activeProfile = $derived(
    PRESETS.find((profile) => profile.id === activePreset) ?? PRESETS[0] ?? null
  );

  // The contract flags Anthropic/Gemini/Bedrock as not directly executable yet
  // (they need an OpenAI-compatible base). Say that plainly instead of leaking
  // the raw enum, and never imply a key can build when it can't.
  function plural(n, word) {
    return `${n} ${word}${Number(n) === 1 ? "" : "s"}`;
  }

  // Readiness and scorecard fold into compact summaries so setup leads and the
  // proof detail stays one disclosure away instead of a wall on first load.
  const readinessSummary = $derived.by(() => ({
    packsReady: languagePacks.filter((pack) => pack.readyForLiveEval).length,
    packTotal: languagePacks.length,
    providerReady: providerPreflight?.readyProviderCount ?? 0,
    providerTotal: providerPreflight?.providerCount ?? 0
  }));

  const scorecardSummary = $derived.by(() => ({
    modelCount: models.length,
    totalRuns: models.reduce((sum, model) => sum + model.runs, 0),
    top: models[0] ?? null
  }));

  async function saveRoleSlot(role, slot) {
    roleSaving = role.key;
    providerResult = null;
    try {
      for (const roleName of role.roles) {
        const response = await fetch("/api/kernel/forge/model-providers.json", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ role: roleName, slot })
        });
        if (!response.ok) throw new Error("role route failed");
        const result = await response.json();
        if (result?.status !== "ROLE_SLOT_STORED") throw new Error(String(result?.status ?? "UNKNOWN"));
      }
      providerResult = {
        providerFamily: role.label,
        ok: true,
        status: slot === "AUTO" ? "AUTO_ROUTING_RESTORED" : "ROLE_ROUTE_STORED"
      };
      const refreshed = await fetch("/api/kernel/forge/model-providers.json");
      providerRoutingState = refreshed.ok ? await refreshed.json() : providerRoutingState;
    } catch (error) {
      providerResult = { providerFamily: role.label, ok: false, status: "ROLE_ROUTE_FAILED" };
    } finally {
      roleSaving = "";
    }
  }

  async function applyProfile(profile) {
    profileSaving = profile.id;
    providerResult = null;
    try {
      const response = await fetch("/api/kernel/forge/model-providers.json", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ profile: profile.id })
      });
      if (!response.ok) throw new Error("profile route failed");
      const result = await response.json();
      if (result?.status !== "ENGINEERING_PROFILE_STORED") {
        throw new Error(String(result?.status ?? "UNKNOWN"));
      }
      const refreshed = await fetch("/api/kernel/forge/model-providers.json");
      providerRoutingState = refreshed.ok ? await refreshed.json() : providerRoutingState;
      providerResult = {
        providerFamily: profile.label,
        ok: true,
        status: "ENGINEERING_PROFILE_STORED"
      };
    } catch (error) {
      providerResult = { providerFamily: profile.label, ok: false, status: "PROFILE_ROUTE_FAILED" };
    } finally {
      profileSaving = "";
    }
  }

</script>

<svelte:head><title>Models - MDx</title></svelte:head>

<ForgeView
  title="Models"
  subtitle="Which model is good at what, answered by the work itself. Every run a model drives leaves its outcome on the record; this is the running tally - finish rate, where each one struggles, and what it costs."
>
  <a class="league-link" href="/forge/models/league">Machines Forge can run, and what it has learned from them &rarr;</a>
  {#if data.modelReadiness || providerRouting}
    <section class="provider-routing" aria-label="Models that run Forge">
      <div class="pr-head">
        <div>
          <h2 class="pr-title">Models that run Forge</h2>
          <p class="pr-frame">
            {#if !modelAccessReady}
              Connect one model in Model Center. The same verified connection then serves Forge, Twin, web, macOS, and paired iPhone work.
            {:else if forgeBuilderReady}
              Model access is ready. Forge can cast planning, building, and review roles without keeping a separate provider key or model list.
            {:else}
              A model is connected, but Forge is holding builds until its required capabilities are verified.
            {/if}
          </p>
        </div>
        <span class="pr-count">{canonicalConnectionCount} connected · {forgeBuilderReady ? "Forge builds ready" : "Forge builds need setup"}</span>
      </div>

      {#if !forgeBuilderReady && !powerOpen}
        <div class="first-connect">
          <p class="fc-hint">{forgeBuilderReadiness?.next_action ?? data.modelReadiness?.recommended_next_action ?? "Connect a model once in Model Center."}</p>
          <a class="fc-connect" href="/admin/models">Open Model Center &rarr;</a>
          <button type="button" class="fc-more" onclick={() => (powerOpen = true)}>
            Preview fleet roles and routing &rarr;
          </button>
        </div>
      {:else if providerRouting}
        <button type="button" class="power-toggle" aria-expanded={powerOpen} onclick={() => (powerOpen = !powerOpen)}>
          {powerOpen ? "Hide fleet setup" : "Set up a fleet"}
          <span class="pt-hint">{powerOpen ? "" : "presets, role casting, multiple providers"}</span>
        </button>
      {:else}
        <p class="fc-hint">Model access is ready. Fleet role controls will appear when Forge's role-routing projection is available.</p>
      {/if}

      {#if powerOpen}
        <div class="power-body">
          <div class="stack-primer">
            <strong>Choose the team, then tune any seat.</strong>
            <span>Harness and model are independent. MDx Native remains the recommended default harness so every run improves MDx, while Codex CLI and Grok Build are first-class alternatives for senior engineers.</span>
          </div>
          <div class="preset-row" role="group" aria-label="Engineering stack profiles">
            {#each PRESETS as preset (preset.id)}
              <button
                type="button"
                class="preset"
                data-active={activePreset === preset.id}
                data-met={providerRouting?.engineeringStack?.activeProfileId === preset.id}
                onclick={() => (activePreset = preset.id)}
              >
                <span class="preset-label">{preset.label}{#if providerRouting?.engineeringStack?.activeProfileId === preset.id}{" · active"}{/if}</span>
                <span class="preset-line">{preset.line}</span>
                {#if preset.requiredSlotCount > 0}
                  <span class="preset-readiness">{preset.readySlotCount}/{preset.requiredSlotCount} model seats connected</span>
                {/if}
              </button>
            {/each}
          </div>
          {#if activeProfile}
            <div class="profile-action">
              <p class="preset-gap">
                {#if activeProfile.ready || activeProfile.requiredSlotCount === 0}
                  This profile can route with the connected models. Individual role changes below turn it into a custom stack.
                {:else}
                  Connect the missing model seats in Model Center. You can apply the profile now; unavailable roles remain fail-closed until their provider is ready.
                {/if}
              </p>
              <button
                type="button"
                class="profile-apply"
                disabled={profileSaving !== "" || providerRouting?.engineeringStack?.activeProfileId === activeProfile.id}
                onclick={() => applyProfile(activeProfile)}
              >
                {profileSaving === activeProfile.id ? "Applying..." : providerRouting?.engineeringStack?.activeProfileId === activeProfile.id ? "Active profile" : `Use ${activeProfile.label}`}
              </button>
            </div>
          {/if}
          <p class="preset-note">
            {#if providerRoutingSummary.evalInformedRoutingReady}
              Forge can recommend role changes from measured eval performance. Your explicit choices remain in force until you change them.
            {:else}
              Premier is the default recommendation: GPT-5.6 planning, Grok-4.5 building, deterministic MDx verification, and Opus 4.8 review. These are role defaults, not vendor lock-in; every seat stays configurable.
            {/if}
          </p>

          <div class="role-grid">
            {#each roleCards as role (role.key)}
              <div class="role-card" data-ready={role.ready}>
                <span>{role.label}</span>
                <strong>{role.ready ? `${role.provider}${role.tone ? ` (${role.tone})` : ""}` : "not cast yet"}</strong>
                <small>{role.ready ? (role.sovereign ? `${role.modelId || "ready"} · on-prem` : role.modelId || "ready") : "connect a model below"}</small>
                {#if availableRoleSlots.length}
                  <select
                    aria-label="Route {role.label}"
                    disabled={roleSaving === role.key}
                    value={role.slot || "AUTO"}
                    onchange={(event) => saveRoleSlot(role, event.currentTarget.value)}
                  >
                    <option value="AUTO">Auto route</option>
                    {#each availableRoleSlots as slot (slot.slot)}
                      <option value={slot.slot}>{slot.label}</option>
                    {/each}
                  </select>
                {/if}
              </div>
            {/each}
          </div>

          <div class="advisor-row">
            <div class="advisor-copy">
              <strong>Ask a senior advisor on hard calls</strong>
              <span>A top judgment model, consulted rarely - plan review before a wide run, or when a build is stuck. It advises; it never builds or merges.</span>
            </div>
            <div class="advisor-pick">
              <small class="advisor-current" data-ready={advisorCard.ready}>
                {#if advisorCard.assigned && advisorCard.ready}
                  {advisorCard.provider}{advisorCard.modelId ? ` · ${advisorCard.modelId}` : ""}{#if advisorCard.sovereign}{" · on-prem"}{/if}
                {:else if advisorCard.ready}
                  Falls back to your strongest model{advisorCard.provider ? ` (${advisorCard.provider})` : ""} until you name one
                {:else}
                  Connect a model below to enable advisor consults
                {/if}
              </small>
              {#if availableRoleSlots.length}
                <select
                  aria-label="Assign senior advisor"
                  disabled={roleSaving === advisorCard.key}
                  value={advisorCard.slot || "AUTO"}
                  onchange={(event) => saveRoleSlot(advisorCard, event.currentTarget.value)}
                >
                  <option value="AUTO">Strongest available (auto)</option>
                  {#each availableRoleSlots as slot (slot.slot)}
                    <option value={slot.slot}>{slot.label}</option>
                  {/each}
                </select>
              {/if}
            </div>
          </div>

          <div class="quick-provider-connect">
            <div>
              <strong>Connection and fleet access live in Model Center</strong>
              <span>Rotate, revoke, test, or add a provider there. Forge consumes the canonical tenant connection and keeps role routing here.</span>
            </div>
            <a class="fc-connect" href="/admin/models">Manage model access &rarr;</a>
          </div>

          {#if providerResult}
            <p class="provider-result" data-ok={providerResult.ok}>{providerResultLine(providerResult)}</p>
          {/if}
        </div>
      {/if}
    </section>
  {/if}

  {#if evalLane || languagePacks.length}
    <details class="readiness" aria-label="How ready Forge is for serious work">
      <summary class="readiness-summary">
        <div class="rs-copy">
          <h2 class="rs-title">How ready is Forge for serious work</h2>
          <p class="rs-frame">
            {#if evalLane?.liveMeasured}
              Measured live across {evalLane.taskCount} senior-engineer tasks on {evalLane.dims.length} dimensions.
            {:else if evalLane}
              {evalLane.taskCount} senior-engineer tasks scored on {evalLane.dims.length} dimensions across {evalLane.providerCount} provider profiles. Local proof runs today - live results arrive once live execution is on.
            {:else}
              Local proof corpus for serious engineering work.
            {/if}
          </p>
        </div>
        <div class="rs-stats">
          {#if readinessSummary.packTotal}
            <span class="rs-pill">{readinessSummary.packsReady}/{readinessSummary.packTotal} language packs ready</span>
          {/if}
          {#if readinessSummary.providerTotal}
            <span class="rs-pill">{readinessSummary.providerReady}/{readinessSummary.providerTotal} providers ready for live eval</span>
          {/if}
        </div>
      </summary>

      <div class="readiness-body">
        {#if evalLane?.classes.length}
          <div class="el-block">
            <span class="el-label">Work classes covered</span>
            <div class="el-chips">
              {#each evalLane.classes as c (c)}<span class="chip">{classLabel(c)}</span>{/each}
            </div>
          </div>
        {/if}
        {#if evalLane?.dims.length}
          <div class="el-block">
            <span class="el-label">What strong engineering means here</span>
            <div class="el-chips">
              {#each evalLane.dims as d (d.id)}<span class="chip" class:must={d.failClosed}>{dimensionLabel(d.id)}{#if d.failClosed}{" · must pass"}{/if}</span>{/each}
            </div>
          </div>
        {/if}
        {#if evalLane?.loadLine}
          <p class="el-load">{evalLane.loadLine}</p>
        {/if}

        {#if languagePacks.length}
          <div class="el-block">
            <span class="el-label">Language packs proven locally</span>
            <p class="rs-note">Each pack carries small, medium, and large tasks with visible and hidden checks, artifact checks, and review gates - and a principal verdict is required before any live result can count.</p>
            <div class="pack-grid">
              {#each languagePacks as pack (pack.id)}
                <div class="pack-row" data-ready={pack.readyForLiveEval}>
                  <span class="pack-dot" aria-hidden="true"></span>
                  <strong>{PACK_LABEL[pack.id] ?? pack.repoFamily ?? pack.id}</strong>
                  <span class="pack-status">{pack.readyForLiveEval ? "ready" : "incomplete"}</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}

        {#if providerPreflight}
          <div class="el-block">
            <span class="el-label">Provider keys for live eval</span>
            <p class="rs-note">Presence-only checks - secret values are not exposed or recorded, and live eval execution still requires approval.</p>
            <div class="provider-grid">
              {#each providerPreflight.requirements as provider}
                <div class="provider-row" data-ready={provider.credentialsPresent}>
                  <span>{PROVIDER_LABEL[provider.providerFamily] ?? humanize(provider.providerFamily)}</span>
                  <strong>{provider.credentialsPresent ? "ready" : "needs a key"}</strong>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </div>
    </details>
  {/if}

  <div class="scorecard-head">
    <h2 class="sc-title">How each model is doing</h2>
    {#if scorecardSummary.modelCount}
      <p class="sc-summary">{plural(scorecardSummary.modelCount, "model")} tracked across {plural(scorecardSummary.totalRuns, "run")}{#if scorecardSummary.top} · <span class="sc-top-model">{scorecardSummary.top.model}</span> leads at {Math.round(scorecardSummary.top.doneRate * 100)}% finished clean{/if}.</p>
    {/if}
  </div>

  {#if models.length === 0}
    <div class="forge-empty">
      <h2>No model history yet</h2>
      <p>As agents work - runs, fleets, revisions - each model's outcomes land here automatically, so you can see who's good at what.</p>
      <a class="mdx-btn primary" href="/forge">Describe work &rarr;</a>
    </div>
  {:else}
    <ul class="model-list">
      {#each models as m (m.model)}
        <li class="model">
          <div class="model-head">
            <span class="model-name">{m.model}</span>
            <span class="model-runs">{m.runs} {m.runs === 1 ? "run" : "runs"}</span>
          </div>
          <div class="rate-row">
            <div class="rate-bar" role="img" aria-label="Finish rate {Math.round(m.doneRate * 100)} percent">
              <div class="rate-fill" style="width: {Math.max(2, m.doneRate * 100)}%"></div>
            </div>
            <span class="rate-label">{Math.round(m.doneRate * 100)}% finished clean</span>
          </div>
          <div class="facts">
            <span class="fact"><strong>{m.done}</strong> done</span>
            {#if m.budgetExhausted}<span class="fact warn"><strong>{m.budgetExhausted}</strong> ran out of turns</span>{/if}
            {#if m.cannotProceed}<span class="fact warn"><strong>{m.cannotProceed}</strong> said it could not</span>{/if}
            {#if m.stopped}<span class="fact"><strong>{m.stopped}</strong> stopped by you</span>{/if}
            {#if m.errored}<span class="fact warn"><strong>{m.errored}</strong> errored</span>{/if}
            <span class="fact quiet">{m.avgTurns.toFixed(1)} turns on average</span>
            <span class="fact quiet">{tokens(m.inputTokens)} in / {tokens(m.outputTokens)} out</span>
          </div>
          {#if m.segments.length}
            <div class="by-work">
              <span class="by-work-label">By kind of work</span>
              {#each m.segments as seg (seg.workType)}
                <div class="seg">
                  <span class="seg-type">{workTypeLabel(seg.workType)}</span>
                  <div class="seg-bar" role="img" aria-label="{Math.round(seg.doneRate * 100)} percent done on {workTypeLabel(seg.workType)}">
                    <div class="seg-fill" style="width: {Math.max(2, seg.doneRate * 100)}%"></div>
                  </div>
                  <span class="seg-rate">{Math.round(seg.doneRate * 100)}% done</span>
                  <span class="seg-meta">{confidence(seg.runs)} &middot; {seg.avgTurns.toFixed(0)} turns &middot; {tokens(seg.tokens)} tok{#if seg.runs > 1} &middot; {tokens(Math.round(seg.tokens / seg.runs))}/run{/if}</span>
                </div>
              {/each}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
    {#if comparisons.length}
      <details class="ab">
        <summary class="ab-summary">
          <span class="ab-title">Same task, different coders</span>
          <span class="ab-sub">The fairest read - one piece of work handed to more than one model. {comparisons.length} {comparisons.length === 1 ? "comparison" : "comparisons"}.</span>
        </summary>
        {#each comparisons as c (c.task)}
          <div class="ab-card">
            <p class="ab-task">{c.task}</p>
            <div class="ab-runs">
              {#each c.runs as r, i (`${r.model}-${r.status}-${r.turns}-${r.tokens}-${i}`)}
                <div class="ab-run" data-tone={outcome(r.status).tone}>
                  <span class="ab-model">{r.model}</span>
                  <span class="ab-outcome">{outcome(r.status).label}</span>
                  <span class="ab-facts">{r.turns} turns &middot; {tokens(r.tokens)} tok</span>
                </div>
              {/each}
            </div>
          </div>
        {/each}
      </details>
    {/if}

    <p class="read-note">
      Numbers grow with every run.
    </p>
  {/if}

  <p class="boundary">{data.boundary}.</p>
</ForgeView>

<style>
  .provider-routing { display: grid; gap: 12px; padding: 18px 20px; border-radius: var(--mdx-radius-lg, 12px); background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle); box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card); }
  .pr-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
  .pr-title { margin: 0; font-family: var(--mdx-font-display); font-size: 15px; font-weight: 650; }
  .pr-frame { margin: 4px 0 0; max-width: 720px; color: var(--mdx-text-muted); font-size: 13px; line-height: 1.5; }
  .pr-count { border: 1px solid var(--mdx-border-subtle); border-radius: 999px; padding: 3px 10px; color: var(--mdx-text-secondary); font-size: 12px; white-space: nowrap; }
  .power-toggle { display: flex; align-items: baseline; gap: 10px; width: fit-content; border: 1px solid var(--mdx-border-subtle); border-radius: 999px; background: var(--mdx-surface-base); color: var(--mdx-text-primary); padding: 6px 14px; font: inherit; font-size: 12.5px; font-weight: 650; cursor: pointer; }
  .power-toggle .pt-hint { color: var(--mdx-text-muted); font-weight: 400; }
  /* First-time connect: the obvious primary path when nothing is connected. */
  .first-connect { display: grid; gap: 8px; }
  .fc-connect { display:inline-flex; align-items:center; justify-content:center; width:fit-content; height:40px; border:1px solid var(--mdx-accent-primary); border-radius:var(--mdx-radius-md); background:var(--mdx-accent-primary); color:var(--mdx-on-accent, #fff); padding:0 18px; font:inherit; font-size:13px; font-weight:650; cursor:pointer; text-decoration:none; white-space:nowrap; }
  .fc-connect:disabled { cursor: default; opacity: 0.45; }
  .fc-hint { color: var(--mdx-text-muted); font-size: 12px; line-height: 1.45; }
  .fc-more { justify-self: start; border: none; background: none; padding: 0; color: var(--mdx-accent-primary); font: inherit; font-size: 12.5px; cursor: pointer; }
  .fc-more:hover { text-decoration: underline; }
  .power-body { display: grid; gap: 14px; }
  .stack-primer { padding: 12px 14px; border-left: 2px solid var(--mdx-accent-primary); background: var(--mdx-surface-subtle); font-size: 12px; }
  .stack-primer span { display: block; margin-top: 4px; color: var(--mdx-text-secondary); }
  .preset-row { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 8px; }
  .preset { display: grid; gap: 4px; text-align: left; padding: 11px 12px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-md); background: var(--mdx-surface-base); color: var(--mdx-text-primary); font: inherit; cursor: pointer; }
  .preset[data-active="true"] { border-color: var(--mdx-accent-primary); box-shadow: 0 0 0 1px var(--mdx-accent-primary) inset; }
  .preset-label { font-size: 12.5px; font-weight: 650; }
  .preset[data-met="true"] .preset-label { color: var(--mdx-accent-success); }
  .preset-line { color: var(--mdx-text-muted); font-size: 11.5px; line-height: 1.4; }
  .profile-action { display: flex; align-items: center; justify-content: space-between; gap: 14px; }
  .profile-apply { border: 0; border-radius: var(--mdx-radius-md); background: var(--mdx-accent-primary); color: var(--mdx-text-on-accent); padding: 9px 13px; font: inherit; font-size: 12px; }
  .profile-apply:disabled { opacity: 0.55; }
  .preset-gap { margin: 0; color: var(--mdx-text-secondary); font-size: 12.5px; }
  .preset-note { margin: 0; color: var(--mdx-text-muted); font-size: 12px; }
  .role-grid { display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 8px; }
  .role-card { display: grid; gap: 3px; min-width: 0; padding: 10px 11px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-text-muted) 7%, transparent); border: 1px solid transparent; }
  .role-card[data-ready="true"] { border-color: color-mix(in srgb, var(--mdx-accent-success) 28%, transparent); }
  .role-card span { color: var(--mdx-text-muted); font-size: 11px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.03em; }
  .role-card strong { overflow: hidden; text-overflow: ellipsis; color: var(--mdx-text-primary); font-family: var(--mdx-font-mono, monospace); font-size: 12.5px; white-space: nowrap; }
  .role-card small { overflow: hidden; text-overflow: ellipsis; color: var(--mdx-text-tertiary); font-size: 11px; white-space: nowrap; }
  .role-card select { min-width: 0; height: 30px; margin-top: 4px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-sm, 6px); background: var(--mdx-surface-base); color: var(--mdx-text-primary); padding: 0 8px; font: inherit; font-size: 11.5px; }
  .advisor-row { display: grid; grid-template-columns: minmax(0, 1fr) minmax(280px, 0.8fr); gap: 12px; align-items: center; padding: 12px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-text-muted) 5%, transparent); }
  .advisor-copy strong { display: block; color: var(--mdx-text-primary); font-size: 13px; }
  .advisor-copy span { display: block; margin-top: 3px; color: var(--mdx-text-muted); font-size: 12.5px; line-height: 1.45; }
  .advisor-pick { display: grid; gap: 5px; min-width: 0; }
  .advisor-current { overflow: hidden; text-overflow: ellipsis; color: var(--mdx-text-tertiary); font-size: 11.5px; white-space: nowrap; }
  .advisor-current[data-ready="true"] { color: var(--mdx-text-secondary, var(--mdx-text-muted)); }
  .advisor-pick select { min-width: 0; height: 30px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-sm, 6px); background: var(--mdx-surface-base); color: var(--mdx-text-primary); padding: 0 8px; font: inherit; font-size: 11.5px; }
  .quick-provider-connect { display: grid; grid-template-columns: minmax(0, 1fr) minmax(280px, 0.8fr); gap: 12px; align-items: end; padding: 12px; border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 24%, var(--mdx-border-subtle)); border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-accent-primary) 7%, transparent); }
  .quick-provider-connect strong { display: block; color: var(--mdx-text-primary); font-size: 13px; }
  .quick-provider-connect span { display: block; margin-top: 3px; color: var(--mdx-text-muted); font-size: 12.5px; line-height: 1.45; }
  .provider-result { margin: 0; color: var(--mdx-tone-warn-text, var(--mdx-text-muted)); font-size: 12.5px; }
  .provider-result[data-ok="true"] { color: var(--mdx-accent-success); }
  @media (max-width: 980px) { .role-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } .preset-row { grid-template-columns: repeat(2, minmax(0, 1fr)); } .quick-provider-connect, .advisor-row { grid-template-columns: 1fr; } }
  @media (max-width: 620px) { .pr-head { display: grid; justify-content: stretch; } .pr-count { justify-self: start; } .role-grid, .preset-row { grid-template-columns: 1fr; } }

  .forge-empty { display: grid; justify-items: center; text-align: center; gap: 12px; max-width: 460px; margin: 32px auto; }
  .forge-empty h2 { margin: 0; font-size: 18px; font-weight: 650; }
  .forge-empty p { margin: 0; color: var(--mdx-text-muted); font-size: 13.5px; line-height: 1.5; }

  .model-list { list-style: none; margin: 0; padding: 0; display: grid; gap: 10px; }
  .model { display: grid; gap: 10px; padding: 16px 18px; border-radius: var(--mdx-radius-lg, 12px); background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle); box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card); }
  .model-head { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }
  .model-name { font-family: var(--mdx-font-mono, monospace); font-size: 14.5px; font-weight: 650; }
  .model-runs { font-size: 12.5px; color: var(--mdx-text-muted); }
  .rate-row { display: flex; align-items: center; gap: 12px; }
  .rate-bar { flex: 1; height: 8px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-text-muted) 14%, transparent); overflow: hidden; }
  .rate-fill { height: 100%; border-radius: 999px; background: var(--mdx-accent-success); }
  .rate-label { font-size: 12.5px; white-space: nowrap; color: var(--mdx-text-muted); }
  .facts { display: flex; flex-wrap: wrap; gap: 8px 16px; font-size: 12.5px; }
  .fact strong { font-weight: 650; }
  .fact.warn { color: var(--mdx-tone-warn-text); }
  .fact.quiet { color: var(--mdx-text-muted); }
  .read-note { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); }

  /* Per kind of work - which model is good at what, the planner's evidence. */
  .by-work { display: grid; gap: 6px; padding-top: 4px; border-top: 1px solid color-mix(in srgb, var(--mdx-text-muted) 12%, transparent); }
  .by-work-label { font-size: 11.5px; font-weight: 650; color: var(--mdx-text-muted); text-transform: uppercase; letter-spacing: 0.03em; }
  .seg { display: grid; grid-template-columns: 130px 1fr auto auto; align-items: center; gap: 10px; font-size: 12px; }
  .seg-type { color: var(--mdx-text-muted); }
  .seg-bar { height: 6px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-text-muted) 12%, transparent); overflow: hidden; }
  .seg-fill { height: 100%; border-radius: 999px; background: var(--mdx-accent-primary); }
  .seg-rate { white-space: nowrap; }
  .seg-meta { white-space: nowrap; color: var(--mdx-text-muted); }
  @media (max-width: 560px) { .seg { grid-template-columns: 1fr auto; } .seg-bar { display: none; } }

  /* The scorecard header - the page's namesake evidence, given clear structure
     now that setup and readiness sit above it. */
  .scorecard-head { display: grid; gap: 3px; margin-top: 4px; }
  .sc-title { margin: 0; font-family: var(--mdx-font-display); font-size: 15px; font-weight: 650; }
  .sc-summary { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); }
  .sc-top-model { font-family: var(--mdx-font-mono, monospace); color: var(--mdx-text-secondary); }

  /* Same task, different coders - the A/B read, one disclosure away. */
  .ab { border-radius: var(--mdx-radius-lg, 12px); background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle); padding: 14px 16px; }
  .ab[open] { display: grid; gap: 10px; }
  .ab-summary { cursor: pointer; display: grid; gap: 2px; list-style: none; }
  .ab-summary::-webkit-details-marker { display: none; }
  .ab-title { font-family: var(--mdx-font-display); font-size: 15px; font-weight: 650; }
  .ab-sub { font-size: 12.5px; color: var(--mdx-text-muted); }
  .ab-card { display: grid; gap: 8px; padding: 14px 16px; border-radius: var(--mdx-radius-lg, 12px); background: var(--mdx-surface-raised); }
  .ab-task { margin: 0; font-size: 13px; font-weight: 600; }
  .ab-runs { display: grid; gap: 6px; }
  .ab-run { display: grid; grid-template-columns: 1fr auto auto; align-items: center; gap: 12px; font-size: 12.5px; padding: 7px 10px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-text-muted) 6%, transparent); }
  .ab-model { font-family: var(--mdx-font-mono, monospace); font-weight: 650; }
  .ab-outcome { color: var(--mdx-accent-success); white-space: nowrap; }
  .ab-run[data-tone="warn"] .ab-outcome { color: var(--mdx-tone-warn-text, var(--mdx-accent-warning)); }
  .ab-facts { color: var(--mdx-text-muted); white-space: nowrap; }
  @media (max-width: 560px) { .ab-run { grid-template-columns: 1fr auto; } .ab-facts { display: none; } }

  /* Readiness: eval lane + language packs + provider preflight, merged into one
     disclosure so setup leads and the proof detail is one click away. */
  .readiness { border-radius: var(--mdx-radius-lg, 12px); background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle); box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card); padding: 18px 20px; }
  .readiness-summary { cursor: pointer; display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; list-style: none; }
  .readiness-summary::-webkit-details-marker { display: none; }
  .rs-copy { display: grid; gap: 4px; }
  .rs-title { margin: 0; font-family: var(--mdx-font-display); font-size: 15px; font-weight: 650; }
  .rs-frame { margin: 0; font-size: 13.5px; color: var(--mdx-text-muted); max-width: 640px; line-height: 1.5; }
  .rs-stats { display: grid; gap: 6px; justify-items: end; }
  .rs-pill { border: 1px solid var(--mdx-border-subtle); border-radius: 999px; padding: 3px 10px; color: var(--mdx-text-secondary); font-size: 12px; white-space: nowrap; }
  .readiness-body { display: grid; gap: 14px; margin-top: 14px; padding-top: 14px; border-top: 1px solid var(--mdx-border-subtle); }
  .rs-note { margin: 0; color: var(--mdx-text-muted); font-size: 12px; line-height: 1.45; }
  .el-block { display: grid; gap: 6px; }
  .el-label { font-size: 11px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.03em; color: var(--mdx-text-muted); }
  .el-chips { display: flex; flex-wrap: wrap; gap: 6px; }
  .chip { font-size: 12px; padding: 3px 10px; border-radius: 999px; background: color-mix(in srgb, var(--mdx-text-muted) 9%, transparent); }
  .chip.must { background: color-mix(in srgb, var(--mdx-accent-success) 14%, transparent); color: var(--mdx-accent-success); font-weight: 600; }
  .el-load { margin: 0; font-size: 12.5px; color: var(--mdx-text-muted); max-width: 640px; line-height: 1.5; }
  .pack-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 6px; }
  .pack-row { display: flex; align-items: center; gap: 8px; min-width: 0; padding: 8px 11px; border: 1px solid var(--mdx-border-subtle); border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-surface-base) 76%, transparent); }
  .pack-row[data-ready="true"] { border-color: color-mix(in srgb, var(--mdx-accent-success) 28%, var(--mdx-border-subtle)); }
  .pack-dot { flex: 0 0 auto; width: 7px; height: 7px; border-radius: 50%; background: var(--mdx-text-muted); }
  .pack-row[data-ready="true"] .pack-dot { background: var(--mdx-accent-success); }
  .pack-row strong { flex: 1; min-width: 0; color: var(--mdx-text-primary); font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pack-status { flex: 0 0 auto; color: var(--mdx-text-tertiary); font-size: 11px; white-space: nowrap; }
  .pack-row[data-ready="true"] .pack-status { color: var(--mdx-accent-success); }
  .provider-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px; }
  .provider-row { display: grid; gap: 3px; min-width: 0; padding: 9px 10px; border-radius: var(--mdx-radius-md); background: color-mix(in srgb, var(--mdx-text-muted) 7%, transparent); }
  .provider-row span { color: var(--mdx-text-primary); font-size: 12.5px; font-weight: 600; }
  .provider-row strong { color: var(--mdx-text-muted); font-size: 11.5px; font-weight: 600; }
  .provider-row[data-ready="true"] strong { color: var(--mdx-accent-success); }
  @media (max-width: 820px) { .pack-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); } .provider-grid { grid-template-columns: 1fr; } }
  @media (max-width: 620px) { .readiness-summary { flex-direction: column; } .rs-stats { justify-items: start; } }

  .boundary { margin: 4px 0 0; font-size: 12px; color: var(--mdx-text-muted); }
  .league-link { display: inline-block; margin: 0 0 18px; font-size: 13px; color: var(--mdx-accent-primary); text-decoration: none; }
  .league-link:hover { text-decoration: underline; }
</style>
