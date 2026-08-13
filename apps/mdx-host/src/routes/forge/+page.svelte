<script>
  // Forge: the governed builder workspace. The first read answers what the
  // work is, where it stands, and what needs your judgment - in human
  // words. Every claim on this page is a projection over kernel receipts
  // (the receipts are the join keys), every action is a real kernel write
  // that stops at its declared boundary, and the full proof sits one tap
  // away in Details. Execution stays honestly held until the engine
  // connects - this page never fakes a running build.
  import { onMount } from "svelte";
  import { refusalLine } from "../../lib/refusals.js";
  import RepoConnect from "../../lib/RepoConnect.svelte";
  import { fade, fly } from "svelte/transition";
  import { goto, invalidateAll } from "$app/navigation";
  import FirstUseHint from "../../lib/FirstUseHint.svelte";
  import LeagueRecommendation from "../../lib/LeagueRecommendation.svelte";
  import MarketplacePackPrompt from "../../lib/MarketplacePackPrompt.svelte";
  import { createMdxClient } from "@mdx/client";
  import { buildExecutionWidthOptions, buildProofPreflightView, resolveExecutionTarget } from "../../lib/forgeComposer.js";
  import { normalizeRuns, normalizeRepos, normalizeRecipes, summaryLine, runHeadline, hasHumanTitle, statusLabel, isRunning } from "../../lib/forgeRuns.js";
  import { forgeNextAction, isComposerPrimary } from "../../lib/forgeNextAction.js";

  let { data, form } = $props();

  // The governed client: writes carry the session's actor admission, the
  // same way every other wired surface fires its verbs.
  const client = $derived(createMdxClient({ baseUrl: "/api/kernel", routeCatalog: null, session: data.session }));
  const actor = $derived(data.session?.user_id ?? "local_user");

  // The front door: hand Forge something to build. This starts a real run
  // (the working loop) - the agent reads the repo, makes the change, runs the
  // checks you name, and lands a branch you review. Recent builds sit just
  // below; the deeper governance view is tucked into "How Forge works".
  const recentRuns = $derived(normalizeRuns(data.runs));
  const buildRepos = $derived(normalizeRepos(data.repos));

  // First run stays a clean door. We deliberately do NOT auto-select a
  // connected repo: a brand-new engineer (or one whose only repos are
  // leftover test artifacts) should land on a connect-and-go front door, not
  // a half-configured power-user form. The full composer discloses the moment
  // they engage - see composerEngaged below.

  // The composer adapts to the selected repo. MDx itself ("") gets its own
  // example and check; an external repo gets its detected language pack when
  // Forge can infer one, then falls back to generic guidance.
  const isExternalRepo = $derived(heroRepo !== "");
  const selectedBuildRepo = $derived(buildRepos.find((repo) => repo.repoId === heroRepo) ?? null);
  const selectedRepoChecks = $derived(
    Array.isArray(selectedBuildRepo?.profile?.suggested_checks)
      ? selectedBuildRepo.profile.suggested_checks.map(String)
      : []
  );
  const selectedRepoProofPlan = $derived(selectedBuildRepo?.profile?.proof_plan ?? null);
  const cloudSetup = $derived(data.cloudSetup ?? {});
  let executionTargetMode = $state("auto");
  const selectedCloudEnvironment = $derived(
    (Array.isArray(cloudSetup?.environments) ? cloudSetup.environments : []).find(
      (environment) =>
        String(environment?.repo_id ?? "") === heroRepo &&
        environment?.ready_for_cloud_builds === true
    ) ?? null
  );
  const executionTarget = $derived(resolveExecutionTarget({ mode: executionTargetMode, cloudEnvironment: selectedCloudEnvironment }));

  let repoIntelBusy = $state(false);
  let repoIntel = $state(null);
  let repoTaskScout = $state(null);
  let repoIntelFlash = $state(null);
  let repoIntelSeq = 0;
  let packSourceRun = $state(null);
  let packDraftName = $state("");
  let packDraftJob = $state("");
  let packDraftItems = $state("");
  let packProposalFlash = $state(null);
  $effect(() => {
    if (!isExternalRepo || !heroRepo) {
      repoIntel = null;
      repoTaskScout = null;
      repoIntelFlash = null;
      repoIntelBusy = false;
      return;
    }
    loadRepoIntelligence(heroRepo);
  });

  async function postKernelJson(path, body) {
    const response = await fetch(`/api/kernel${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(2000)
    });
    if (!response.ok) return null;
    return response.json();
  }

  function startPackProposal(run) {
    packSourceRun = run;
    packDraftName = `${runHeadline(run)} Pack`;
    packDraftJob = summaryLine(run);
    packDraftItems = run.lessonCandidate || "Reuse the proven checks and review shape from this run.";
    packProposalFlash = null;
  }

  async function proposeTeamPack() {
    if (!packSourceRun || !packDraftName.trim()) return;
    try {
      const packId = `team.${packDraftName.trim().toLowerCase().replace(/[^a-z0-9]+/g, ".").replace(/^\.|\.$/g, "")}`;
      const packet = await client.write("/marketplace/pack-actions.json", {
        actor_id: actor,
        action: "propose",
        pack_id: packId,
        source_lane: "team",
        source_record_id: packSourceRun.runId,
        task_class: "proven_forge_run",
        note: `${packDraftName.trim()} | ${packDraftJob.trim()} | ${packDraftItems.trim()}`
      });
      packProposalFlash = packet.status === "REFUSED"
        ? { ok: false, line: packet.reason }
        : { ok: true, line: "Draft team pack sent to Marketplace review. It cannot publish itself." };
      if (packet.status !== "REFUSED") packSourceRun = null;
    } catch (error) {
      packProposalFlash = { ok: false, line: "Nothing was recorded. Try again." };
    }
  }

  async function loadRepoIntelligence(repoId) {
    const seq = ++repoIntelSeq;
    repoIntelBusy = true;
    repoIntelFlash = null;
    try {
      const [onboarding, scout] = await Promise.all([
        postKernelJson("/forge/repo-onboarding-packet.json", { repo_id: repoId }),
        postKernelJson("/forge/repo-task-scout.json", { repo_id: repoId })
      ]);
      if (seq !== repoIntelSeq) return;
      repoIntel = onboarding?.status === "OK" ? onboarding : null;
      repoTaskScout = scout && scout.status !== "REFUSED" ? scout : null;
      if (!repoIntel && !repoTaskScout) {
        repoIntelFlash = "Forge could not read that repo's intelligence packet yet.";
      }
    } catch (error) {
      if (seq !== repoIntelSeq) return;
      repoIntel = null;
      repoTaskScout = null;
      repoIntelFlash = "Forge could not read that repo's intelligence packet yet.";
    }
    if (seq === repoIntelSeq) repoIntelBusy = false;
  }

  const repoIntelTasks = $derived.by(() => {
    const scoutTasks = Array.isArray(repoTaskScout?.candidates) ? repoTaskScout.candidates : [];
    const onboardingTasks = Array.isArray(repoIntel?.first_run_tasks) ? repoIntel.first_run_tasks : [];
    return scoutTasks.length > 0 ? scoutTasks : onboardingTasks;
  });
  // Honest framing: real scout findings point at specific code; the onboarding
  // fallback is just safe ways to start. Never claim "found" for a template.
  const tasksAreFindings = $derived((repoTaskScout?.candidates?.length ?? 0) > 0);
  const repoIntelStatusLine = $derived.by(() => {
    if (!isExternalRepo) return "";
    if (repoIntelBusy) return "Reading repo intelligence...";
    if (repoIntel?.safe_next_move) return String(repoIntel.safe_next_move);
    if (repoTaskScout?.safe_next_move) return String(repoTaskScout.safe_next_move);
    return repoIntelFlash || "";
  });
  // Human readiness, not raw enums. The panel should read like "Forge read
  // your repo and here's what it found", not a diagnostic status code.
  const repoReady = $derived(
    repoIntel?.onboarding_status === "READY_FOR_FIRST_MEDIUM_RUN" ||
      repoTaskScout?.status === "TASKS_FOUND"
  );
  // The badge must not say a green "Ready" while the panel itself is asking
  // for a proof command or a missing toolchain. It follows the proof state
  // first, then falls back to the task-scout readiness.
  const repoReadyTone = $derived.by(() => {
    if (repoIntelBusy) return "reading";
    if (proofPreflightView?.tone === "setup") return "toolchain";
    if (isExternalRepo && heroChecks.trim().length === 0 && !proofReady) return "check";
    if (repoReady) return "ready";
    return "getting";
  });
  const repoReadyLabel = $derived.by(() => {
    switch (repoReadyTone) {
      case "reading":
        return "Reading...";
      case "toolchain":
        return "Toolchain needed";
      case "check":
        return "Needs a check";
      case "ready":
        return "Ready";
      default: {
        const s = repoIntel?.onboarding_status || repoTaskScout?.status;
        return s ? String(s).toLowerCase().replace(/_/g, " ") : "Getting ready";
      }
    }
  });

  const proofPreflight = $derived(repoIntel?.proof_command_preflight ?? null);
  const proofPreflightView = $derived(buildProofPreflightView({
    isExternalRepo,
    proofPreflight,
    selectedCloudEnvironment,
    selectedChecks: heroChecks
  }));
  const externalRunNeedsCheck = $derived(isExternalRepo && heroChecks.trim().length === 0);
  // One calm hint replaces the old triple nag. If the repo's proof plan is
  // ready with a real command there is nothing to prompt; otherwise a single
  // friendly line asks for the check that proves the work. When the
  // per-language setup phase lands the plan reports ready and this falls
  // silent on its own.
  const proofReady = $derived(
    proofPreflightView?.tone === "ready" ||
      String(selectedRepoProofPlan?.status ?? "") === "ready"
  );
  const checkHint = $derived.by(() => {
    if (!isExternalRepo || proofReady) return "";
    if (heroChecks.trim().length === 0) {
      return "Name the check that proves this work, like npm test or pytest. Forge runs it in an isolated copy, never your real files.";
    }
    return "";
  });
  const repoSourceHostLine = $derived.by(() => {
    const summary = repoIntel?.source_host_summary;
    if (!summary) return "";
    const host = String(summary.source_host ?? "generic");
    const source = host === "github" ? "GitHub" : host === "bitbucket" ? "Bitbucket" : "source host";
    if (summary.origin_url_present === true) {
      return `${source} inferred from origin. PR delivery still waits for explicit operator action.`;
    }
    return "No recognizable GitHub or Bitbucket origin yet. PR delivery can be selected later.";
  });
  function applyRepoIntelTask(task) {
    const prompt = String(task?.prompt_template ?? "");
    if (prompt) heroIntent = prompt;
    const checks = Array.isArray(task?.suggested_checks) ? task.suggested_checks.map(String).filter(Boolean) : [];
    if (checks.length > 0) {
      heroChecks = checks.join("\n");
      checksTouched = true;
    }
    const tier = String(task?.complexity_tier ?? "");
    const taskClass = String(task?.task_class ?? "");
    executionWidth = tier === "extreme"
      ? Math.min(16, Math.max(8, scaleCapacity.n || 8))
      : tier === "medium" || taskClass === "refactor"
        ? Math.max(1, Math.min(4, scaleCapacity.n || 4))
        : 1;
    widthTouched = false;
  }
  // The four first-class verbs: guided entry points that scaffold the ask so
  // an engineer starts from a shape, not a blank box.
  const ACTION_VERBS = [
    { id: "refactor", label: "Refactor", scaffold: "Refactor [name the file or module]: improve its structure and readability without changing behavior. Keep all existing tests green." },
    { id: "bug", label: "Fix a bug", scaffold: "Fix a bug: [describe the wrong behavior and where it shows up]. Add a regression test that fails before the fix and passes after." },
    { id: "security", label: "Find & fix security", scaffold: "Find and fix a security issue: review [input handling / auth / dependencies] for vulnerabilities, then patch the highest-risk one and add a test that proves it." },
    { id: "build", label: "Build something", scaffold: "Build [name the feature]: [what it should do and how it is proven]. Add tests covering the new behavior." }
  ];
  function applyActionVerb(verb) {
    heroIntent = verb.scaffold;
  }

  const intentPlaceholder = $derived(
    isExternalRepo
      ? "e.g. Add a retry with backoff to the API client, and a test that covers it."
      : "e.g. In apps/mdx-host/src/lib/messageSession.js, add a helper that retries a failed send..."
  );
  const checksPlaceholder = $derived(
    isExternalRepo
      ? "e.g. npm test, pytest"
      : "e.g. cargo build -p mdx-core"
  );

  // The MDx-self default check, made ask-aware so a JS ask does not get a Rust
  // build (which would prove nothing about the change). Mirrors the server's
  // stack_aware check derivation: a named node test file wins, else a JS app
  // area runs that app's node tests, else the repo's cargo build. Rust asks and
  // an empty box still get cargo build.
  function defaultSelfCheck(intent) {
    const text = String(intent || "");
    const testFile = text.match(/\bapps\/[\w./-]+\.(?:test|spec)\.(?:mjs|js)\b/);
    if (testFile) return `node ${testFile[0]}`;
    const appArea = text.match(/\bapps\/([\w-]+)\//);
    if (appArea) return `node --test apps/${appArea[1]}/test`;
    return "cargo build -p mdx-core";
  }
  // Keep the proof command honest per repo: MDx itself auto-fills a check from
  // the ask (the dogfood convenience); an external repo uses the detected
  // language pack when one exists and otherwise stays empty. Stops adapting the
  // moment the operator edits the field.
  let checksTouched = $state(false);
  $effect(() => {
    if (checksTouched) return;
    heroChecks = isExternalRepo ? selectedRepoChecks.join("\n") : defaultSelfCheck(heroIntent);
  });
  const buildRecipes = $derived(normalizeRecipes(data.recipes));
  const builderLoops = $derived(Array.isArray(data.builderLoops?.builder_loops) ? data.builderLoops.builder_loops : []);

  function builderLoopStateLine(loop) {
    const state = loop?.state ?? {};
    if (state.external_result_ingestion_status) return "External trial quarantined";
    if (state.fleet_run_status === "FLEET_STARTED") return `Fleet attached: ${state.attached_fleet_run_id}`;
    if (state.fleet_plan_status) return `Fleet plan: ${state.fleet_plan_status.toLowerCase()}`;
    // Checker gate state is receipt-derived; the maker run cannot self-accept.
    if (state.checker_status) return state.checker_status.replace("CHECKER_", "Checker ").replace(/_/g, " ").toLowerCase();
    if (state.run_admission_status === "RUN_STARTED") return `Run attached: ${state.attached_run_id}`;
    if (state.run_admission_status === "RUN_ADMISSION_REFUSED") return "Run admission refused";
    return Number(state.tick_count ?? 0) > 0 ? "Ran before" : "Ready to run";
  }

  async function tickBuilderLoop(loop) {
    if (!loop?.id) return;
    await fetch("/api/kernel/forge/builder-loops/tick.json", {
      method: "POST",
      body: JSON.stringify({
        loop_id: loop.id,
        trigger: "manual"
      })
    }).catch(() => null);
    await invalidateAll();
  }

  // The flywheel: what a finished build leaves behind so the next one is
  // better. Read-only over kernel receipts. The proof route states exactly
  // how real the local loop is (an isolated worktree copy, never your live
  // repo) and what it has learned. Honest zeros until builds run; nothing
  // here implies a build runs, ships, or deploys on its own.
  const flywheel = $derived.by(() => {
    const proof = data.flywheelProof;
    const local = proof?.local_real ?? {};
    return {
      ready: proof?.status === "LOCAL_REAL_FLYWHEEL_PROOF_READY",
      outcomes: Number(proof?.outcome_signal_count ?? data.outcomeSignals?.outcome_signal_count ?? 0),
      lessons: Number(proof?.candidate_lesson_count ?? data.candidateLessons?.candidate_count ?? 0),
      activeMemory: Number(proof?.active_memory_count ?? 0),
      capabilities: Number(proof?.installed_capability_count ?? data.installedCapabilities?.installed_count ?? 0),
      worktreeIsolated: local.worktree_isolated !== false,
      liveRepoMutated: local.live_repo_mutated === true,
      modelConfigured: local.model_configured === true,
      hostChecks: local.host_checks_enabled === true,
      sandboxTests: local.sandbox_tests_enabled === true,
      patchApply: local.patch_apply_enabled === true,
      betaGaps: Array.isArray(proof?.named_beta_gaps) ? proof.named_beta_gaps : []
    };
  });
  // The lessons a finished build proposes - evidence-backed, citation-only.
  // A human promotes one before it can ever be cited by a later plan; none
  // of them change behavior on their own.
  const candidateLessons = $derived(
    Array.isArray(data.candidateLessons?.candidates) ? data.candidateLessons.candidates : []
  );
  // Builds that hit the same wall propose the same lesson. Collapse identical
  // suggestions into one row with a count so the panel reads as a signal, not
  // a repeated boilerplate line.
  const dedupedLessons = $derived.by(() => {
    const seen = new Map();
    for (const lesson of candidateLessons) {
      // Strip the machine "Candidate lesson:" prefix so the line reads human.
      const key = String(lesson?.lesson_summary || lesson?.title || "")
        .trim()
        .replace(/^candidate lesson:\s*/i, "");
      if (!key) continue;
      const existing = seen.get(key);
      if (existing) existing.count += 1;
      else seen.set(key, { id: lesson.candidate_id || key, text: key, count: 1 });
    }
    return [...seen.values()];
  });
  // The work the loop can really do right now, framed around what it leaves
  // behind for the next build. The isolation promise lives once, on the
  // composer; here we talk about learning, not safety.
  const localRealLine = $derived.by(() => {
    if (!flywheel.modelConfigured) return "Connect a model in Welcome (or start the server with your provider env) to let a build read, plan, and propose a change.";
    if (flywheel.patchApply && flywheel.sandboxTests) return "Each build patches and tests in its own copy, then leaves the outcome and a candidate lesson for the next plan to build on.";
    if (flywheel.sandboxTests || flywheel.hostChecks) return "Each build plans and runs your checks, then leaves the outcome for the next plan to build on.";
    return "Each build reads the repo and plans the change, leaving an outcome behind. Turn on local checks to let it patch and test for real.";
  });

  // A long-horizon mission, shown at a glance when one is shaped. The full
  // cockpit is /forge/missions; this is the "meaty work is happening" hero.
  // Every word is receipt-derived: the state line says "shaped, not live"
  // until the adapter turn-on receipt exists, never sooner.
  const activeMission = $derived.by(() => {
    const list = Array.isArray(data.missionProjection?.missions)
      ? data.missionProjection.missions
      : Array.isArray(data.missionDashboard?.missions)
        ? data.missionDashboard.missions
        : [];
    if (list.length === 0) return null;
    const m = list[0];
    const milestones = Array.isArray(m?.milestones) ? m.milestones : [];
    const passed = milestones.filter((x) =>
      ["PASSED", "DONE"].includes(String(x?.status ?? ""))
    ).length;
    const executionOn = data.missionDashboard?.adapter_execution_allowed === true;
    return {
      count: Number(data.missionDashboard?.active_mission_count ?? list.length),
      goal: String(m?.goal ?? ""),
      total: milestones.length,
      passed,
      line: executionOn
        ? "Running. Checks run after each milestone."
        : "Shaped and governed. Open it to launch a milestone through a run or fleet.",
      tone: executionOn ? "active" : "shaping"
    };
  });
  // Named beta gaps, in human words. The proof carries the exact machine
  // strings; this is the honest plain-language version one tap away.
  function betaGapLine(gap) {
    const key = String(gap);
    if (key.includes("durable_queue")) return "If MDx restarts mid build, in-flight work is lost until the cloud queue lands.";
    if (key.includes("gvisor") || key.includes("firecracker") || key.includes("community")) return "Capabilities shared by the community can't run yet - that needs stronger isolation first.";
    return key.replace(/_/g, " ");
  }
  let heroIntent = $state("");
  let heroChecks = $state("");
  const checkCount = $derived(heroChecks.split("\n").map((line) => line.trim()).filter(Boolean).length);
  let heroRepo = $state("");
  let heroBusy = $state(false);
  // Run control surface: how hard the coder thinks, and whether to plan first.
  let heroEffort = $state(""); // "" = harness default
  let heroPlanFirst = $state(false);
  let heroOutcomePreference = $state("balanced");
  let heroHarness = $state("auto");
  let heroPlannerMode = $state("auto");
  let heroReviewMode = $state("auto");
  let heroMaxCostDollars = $state("");
  let heroMaxRuntimeMinutes = $state("");
  let heroMaxTurns = $state("");
  const harnessLabels = { auto: "Auto", mdx_native: "MDx Native", codex_cli: "Codex CLI", grok_build: "Grok Build" };
  const harnessLines = {
    auto: "Forge chooses and explains why.",
    mdx_native: "Use the house harness and improve its learning loop.",
    codex_cli: "Use Codex CLI inside the same proof boundary.",
    grok_build: "Use open-source Grok Build with isolated state and MDx gates."
  };
  const harnessLabel = $derived(harnessLabels[heroHarness] ?? "Auto");
  const plannerLabel = $derived({ auto: "Auto", skip: "Skip", bounded: "Bounded", required: "Required" }[heroPlannerMode] ?? "Auto");
  const reviewLabel = $derived({ auto: "Auto", deterministic_only: "Checks", conditional_independent: "When useful", required_independent: "Required" }[heroReviewMode] ?? "Auto");
  const effortChoices = [
    { value: "", label: "Balanced" },
    { value: "low", label: "Light" },
    { value: "high", label: "Deep" },
    { value: "xhigh", label: "Max" }
  ];
  let effortLabel = $derived(effortChoices.find((c) => c.value === heroEffort)?.label ?? "Balanced");
  // Which machine runs it: pick a specific coder slot, or let the harness cast.
  // Each option carries whether it is sovereign (data never leaves it).
  let heroCast = $state(""); // "" = harness casts automatically
  let castOptions = $derived.by(() => {
    const providers = Array.isArray(data.modelProviders?.providers) ? data.modelProviders.providers : [];
    const slots = Array.isArray(data.modelProviders?.slots) ? data.modelProviders.slots : [];
    return slots
      .filter((slot) => (slot?.full_env_endpoint_ready === true || slot?.provider_key_ready === true) && String(slot?.slot ?? "").trim())
      .map((slot) => {
        const family = String(slot?.provider_family ?? "");
        const provider = providers.find((row) => String(row?.provider_family ?? "") === family);
        return {
          slot: String(slot.slot),
          family,
          label: String(provider?.model_hint || provider?.label || slot.slot),
          sovereign: String(slot?.privacy_class ?? "") === "sovereign"
        };
      })
      .filter((option, index, all) => all.findIndex((other) => other.slot === option.slot) === index);
  });
  function harnessAllowsModel(harness, family) {
    if (harness === "codex_cli") return family === "openai";
    if (harness === "grok_build") return family === "xai";
    return true;
  }
  let compatibleCastOptions = $derived(castOptions.filter((option) => harnessAllowsModel(heroHarness, option.family)));
  function chooseHarness(value) {
    heroHarness = value;
    if (value === "codex_cli" || value === "grok_build") heroPlanFirst = false;
    const selected = castOptions.find((option) => option.slot === heroCast);
    if (selected && !harnessAllowsModel(value, selected.family)) heroCast = "";
  }
  let castLabel = $derived(heroCast ? (castOptions.find((o) => o.slot === heroCast)?.label ?? heroCast) : "Auto-cast");
  let heroFlash = $state(null);
  let executionWidth = $state(1);
  let widthTouched = $state(false);
  const strategyHasOverrides = $derived(
    heroHarness !== "auto" ||
      heroPlannerMode !== "auto" ||
      heroReviewMode !== "auto" ||
      heroOutcomePreference !== "balanced" ||
      widthTouched ||
      String(heroMaxCostDollars).trim() !== "" ||
      String(heroMaxRuntimeMinutes).trim() !== "" ||
      String(heroMaxTurns).trim() !== ""
  );
  let handoffNote = $state("");
  let playgroundBusy = $state(false);
  let playgroundNote = $state("");
  const PLAYGROUND_REPO_ID = "mdx-playground";
  const PLAYGROUND_CHECK = "npm test";

  function broadDemoAsk(intent = heroIntent) {
    const text = String(intent ?? "").toLowerCase();
    if (!text.trim()) return false;
    if (/(\bapps\/|\bcrates\/|\bpackages\/|\bdocs\/|\.svelte\b|\.rs\b|\.js\b|\.ts\b|\.md\b)/.test(text)) {
      return false;
    }
    return /\b(game|web ?page|website|toy app|demo app|hello world|landing page|prototype|playground|simple app)\b/.test(text);
  }

  const broadDemoNeedsRepo = $derived(heroRepo === "" && broadDemoAsk(heroIntent) && data.playgroundAvailable !== true);
  const shouldUsePlayground = $derived(heroRepo === "" && broadDemoAsk(heroIntent) && data.playgroundAvailable === true);
  const playgroundRepo = $derived(buildRepos.find((repo) => repo.repoId === PLAYGROUND_REPO_ID) ?? null);

  // The composer has two states: a door (connect-and-go, first run) and the
  // full form. It opens to the form the moment the engineer engages - by
  // picking a connected repo, connecting a new one, choosing MDx, or starting
  // to describe the work. repoTouched is sticky so the form never collapses
  // mid-thought.
  let repoTouched = $state(false);
  const composerEngaged = $derived(repoTouched || heroIntent.trim().length > 0);
  function chooseRepo(repoId) {
    heroRepo = repoId;
    repoTouched = true;
  }
  function chooseMdx() {
    heroRepo = "";
    repoTouched = true;
  }
  function repoLanguageLabel(repo) {
    const lang = String(repo?.profile?.primary_language ?? "").trim();
    if (!lang || lang === "generic" || lang === "unknown") return "";
    return lang
      .split("-")
      .map((part) =>
        part === "typescript"
          ? "TypeScript"
          : part === "javascript"
            ? "JavaScript"
            : part.charAt(0).toUpperCase() + part.slice(1)
      )
      .join(" / ");
  }

  // Connect a repo right here at the front door, so pointing Forge at your
  // own repo (the recommended way to try it - no MDx scope rules) is one
  // place: connect, it auto-selects, build. Mirrors the Runs connect flow.
  let repoConnectOpen = $state(false);
  let connecting = $state(false);
  let connectFlash = $state(null);
  // A success confirmation that survives the door-to-form swap (the connect
  // panel unmounts on success, so its own flash would never be seen).
  let connectedNote = $state(null);
  let connectedNoteTimer = null;
  function flashConnected(line) {
    connectedNote = line;
    if (connectedNoteTimer) clearTimeout(connectedNoteTimer);
    connectedNoteTimer = setTimeout(() => (connectedNote = null), 4500);
  }
  // Repos the operator removed from view this session. Durable removal needs a
  // kernel detach route (not built yet); this declutters the door in the
  // meantime and is the affordance that route will hang off.
  const HIDDEN_REPOS_KEY = "forge.hiddenRepos";
  let hiddenRepoIds = $state([]);
  const visibleRepos = $derived(buildRepos.filter((repo) => !hiddenRepoIds.includes(repo.repoId)));
  function disconnectRepo(repoId) {
    hiddenRepoIds = [...hiddenRepoIds, repoId];
    if (heroRepo === repoId) heroRepo = "";
    try {
      localStorage.setItem(HIDDEN_REPOS_KEY, JSON.stringify(hiddenRepoIds));
    } catch (error) {
      // archive is best-effort; the ledger truth is unchanged either way
    }
  }
  // Split a git failure into a human line and the raw stderr we tuck away.


  // Native folder picker - the server opens the OS dialog and returns the
  // absolute path (a browser cannot read one). Fills the path field; the
  // existing governed connect still does the write.

  // Post-connect behavior for the shared RepoConnect: auto-select the fresh
  // repo, open the full composer, un-hide if it was archived, keep the setup
  // strip on the repo step, and confirm from the form side (the door
  // unmounts on success).
  async function handleRepoConnected(connectedId, connectedLabel) {
    connectFlash = null;
    await invalidateAll();
    heroRepo = connectedId;
    repoTouched = true;
    repoConnectOpen = false;
    if (hiddenRepoIds.includes(connectedId)) {
      hiddenRepoIds = hiddenRepoIds.filter((id) => id !== connectedId);
      try {
        localStorage.setItem(HIDDEN_REPOS_KEY, JSON.stringify(hiddenRepoIds));
      } catch (error) {
        // best-effort archive; the ledger truth is unchanged
      }
    }
    if (showSetup) pickedStep = 2;
    flashConnected(`Connected ${connectedLabel}`);
  }

  async function ensurePlaygroundRepo() {
    playgroundBusy = true;
    playgroundNote = "";
    try {
      const response = await fetch("/api/kernel/forge/playground-repo.json", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ actor_id: actor })
      });
      const packet = await response.json().catch(() => ({}));
      if (response.ok && packet?.status === "CONNECTED") {
        await invalidateAll();
        const repoId = String(packet.repo_id || PLAYGROUND_REPO_ID);
        const check = String(packet.suggested_check || PLAYGROUND_CHECK);
        heroRepo = repoId;
        repoTouched = true;
        heroChecks = check;
        checksTouched = true;
        playgroundNote = "Running this in the local playground so Forge can build freely without touching MDx itself.";
        return { repoId, check };
      }
      throw new Error(packet?.reason || "playground did not connect");
    } catch (error) {
      heroFlash = { ok: false, line: "Playground did not start. Connect a repo or choose MDx self with a scoped path." };
      return null;
    } finally {
      playgroundBusy = false;
    }
  }

  const scaleCapacity = $derived.by(() => {
    const raw = data.fleetCapacity ?? {};
    const capacity = Math.max(0, Number(raw.worker_capacity ?? 0) || 0);
    const active = Math.max(0, Number(raw.active_now ?? 0) || 0);
    const queueDepth = Math.max(0, Number(raw.queue_depth ?? 0) || 0);
    const queueLimit = Math.max(0, Number(raw.queue_depth_limit ?? 0) || 0);
    const n = capacity > 0 ? Math.max(4, capacity) : 16;
    return { capacity, active, queueDepth, queueLimit, n };
  });
  const DIRECT_CANDIDATE_WIDTH = 4;
  const executionWidthOptions = $derived(buildExecutionWidthOptions({
    workerCapacity: scaleCapacity.capacity,
    queueDepthLimit: scaleCapacity.queueLimit,
    directCandidateWidth: DIRECT_CANDIDATE_WIDTH
  }));

  function boundedExecutionWidth(value) {
    const width = Math.max(1, Math.round(Number(value) || 1));
    return Math.min(512, width);
  }

  function chooseExecutionWidth(value) {
    executionWidth = boundedExecutionWidth(value);
    widthTouched = true;
  }

  function recommendedExecutionWidth(c) {
    const strategyWidth = Number(c?.strategy?.width ?? 0);
    if (Number.isFinite(strategyWidth) && strategyWidth > 0) {
      return boundedExecutionWidth(strategyWidth);
    }
    const contractWidth = Number(c?.recommended_width ?? 0);
    if (Number.isFinite(contractWidth) && contractWidth > 0) {
      return boundedExecutionWidth(contractWidth);
    }
    const taskClass = String(c?.task_class ?? "");
    const tier = String(c?.complexity_tier ?? "");
    if (c?.recommended_shape === "mission" || taskClass === "long_horizon") {
      if (tier === "extreme") return Math.min(32, Math.max(8, scaleCapacity.n || 8));
      if (tier === "xl" || tier === "large") return Math.min(16, Math.max(4, scaleCapacity.n || 4));
      return 4;
    }
    if (["architecture", "migration", "concurrency", "multi_file", "product_ux"].includes(taskClass)) {
      return tier === "small" ? 1 : 4;
    }
    return 1;
  }

  function usesDirectCandidateRun(width = executionWidth) {
    return width > 1 && width <= DIRECT_CANDIDATE_WIDTH;
  }

  function usesGovernedFleetPlan(width = executionWidth) {
    return width > DIRECT_CANDIDATE_WIDTH;
  }

  // The recommendation card speaks the shape in human terms. "Parallel lanes"
  // for small fan-out (2-4, bounded run path); "fleet" only at 5+ where Forge
  // genuinely drafts a fleet plan. Reads executionWidth so the card updates live
  // when the operator dials width under Adjust.
  function recommendationLabel(c) {
    const contractWidth = Number(c?.recommended_width ?? 0);
    if (c?.recommendation_label && executionWidth === boundedExecutionWidth(contractWidth || executionWidth)) {
      if (String(c.recommendation_label) === "Parallel lanes" && executionWidth > 1) {
        return `${executionWidth} parallel lanes`;
      }
      if (String(c.recommendation_label) === "A fleet" && executionWidth > DIRECT_CANDIDATE_WIDTH) {
        return `A ${executionWidth}-lane fleet`;
      }
      return c.recommendation_label;
    }
    if (c?.recommended_shape === "mission") return "A mission";
    const w = executionWidth;
    if (w <= 1) return "Single-run starting point";
    if (usesGovernedFleetPlan(w)) return `A ${w}-lane fleet`;
    return `${w} parallel lanes`;
  }

  function recommendationWhy(c) {
    const contractWidth = Number(c?.recommended_width ?? 0);
    if (c?.recommendation_reason && executionWidth === boundedExecutionWidth(contractWidth || executionWidth)) {
      return c.recommendation_reason;
    }
    if (c?.recommended_shape === "mission") {
      return "Long-horizon work - many steps with a check after each milestone, a budget, and a fleet of workers. Worth shaping the scope and checkpoints before it runs.";
    }
    const w = executionWidth;
    if (w <= 1) return "One focused change in one repo. Auto-read may add a comparison lane; choosing Solo keeps one run.";
    if (usesGovernedFleetPlan(w)) {
      return `Enough separable surfaces to run ${w} lanes in parallel - each in its own isolated copy with disjoint scope - then merge into one reviewed branch.`;
    }
    return `A few separable pieces - ${w} lanes run in parallel in isolated copies, and Review ranks the finished branches before you choose.`;
  }

  // Gate-aware CTA: a run starts immediately; a fleet or mission has a
  // ratification gate before any spend, so the copy must not imply it started.
  function recommendationCta(c) {
    if (c?.recommended_shape === "mission") return "Looks right - set it up";
    if (usesGovernedFleetPlan(executionWidth)) return "Looks right - prepare the plan";
    return "Looks right - start";
  }

  function executionScaleLine(width = executionWidth) {
    if (width <= 1) return "One isolated run. Fastest path for tight changes.";
    if (usesDirectCandidateRun(width)) {
      return `${width} candidate workers run in isolated copies; Review ranks the finished branches before you choose.`;
    }
    return `${width} agents work in parallel, each in its own copy on separate files so they never collide, and Forge waits for your approval before it runs.`;
  }

  function startLabel(width = executionWidth) {
    if (usesGovernedFleetPlan(width)) return "Plan fleet build";
    if (usesDirectCandidateRun(width)) return "Run candidates";
    return "Start the build";
  }

  function busyLabel(width = executionWidth) {
    if (usesGovernedFleetPlan(width)) return "Planning...";
    if (usesDirectCandidateRun(width)) return "Starting candidates...";
    return "Starting...";
  }

  function useRecipe(recipe) {
    heroIntent = recipe.intent;
    if (!isExternalRepo) {
      heroChecks = recipe.commands.join("\n");
      checksTouched = true;
    }
    executionWidth = 1;
  }

  function adaptiveStrategyFields() {
    const costDollars = Number(heroMaxCostDollars);
    const runtimeMinutes = Number(heroMaxRuntimeMinutes);
    const turns = Number(heroMaxTurns);
    return {
      strategy_mode: strategyHasOverrides ? "custom" : "auto",
      outcome_preference: heroOutcomePreference,
      harness_id: heroHarness,
      planner_mode: heroPlannerMode,
      review_mode: heroReviewMode,
      width_locked: widthTouched,
      max_cost_cents: costDollars > 0 ? Math.round(costDollars * 100) : undefined,
      max_runtime_ms: runtimeMinutes > 0 ? Math.round(runtimeMinutes * 60_000) : undefined,
      max_turns: turns > 0 ? Math.round(turns) : undefined
    };
  }

  async function classifyForLaunch(runRepo, runChecks) {
    const response = await fetch("/api/kernel/forge/work-classifications.json", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        ask: heroIntent.trim(),
        repo: runRepo || "MDx",
        declared_checks: runChecks.join("\n"),
        actor_id: actor,
        builder_slot: heroCast,
        fleet_width: executionWidth,
        ...adaptiveStrategyFields()
      })
    });
    const packet = await response.json().catch(() => ({}));
    if (!response.ok || packet?.status !== "FORGE_WORK_CLASSIFICATION_RECORDED") {
      return null;
    }
    return packet;
  }

  async function startHeroBuild(options = {}) {
    if (heroBusy || !heroIntent.trim()) return;
    if (executionTarget.blocked) {
      heroFlash = { ok: false, line: executionTarget.line };
      return;
    }
    const adaptBeforeLaunch = options.adapt !== false;
    heroBusy = true;
    heroFlash = null;
    try {
      let runRepo = heroRepo;
      let runChecks = heroChecks
        .split("\n")
        .map((line) => line.trim())
        .filter(Boolean);
      if (shouldUsePlayground) {
        const playground = await ensurePlaygroundRepo();
        if (!playground) return;
        runRepo = playground.repoId;
        runChecks = [playground.check];
      }
      if (adaptBeforeLaunch && !widthTouched) {
        const launchClassification = await classifyForLaunch(runRepo, runChecks);
        if (launchClassification) {
          // Route on the classification (width, mission divert, checks) but do
          // not paint the "How I'd approach this" proposal card here - the
          // operator pressed Start, not Plan it. Rendering a proposal on a
          // direct start (and leaving it stranded next to a refusal) is what
          // made the primary verb feel like it never actually starts. The
          // proposal is reserved for the Plan-it path (classifyAsk).
          executionWidth = recommendedExecutionWidth(launchClassification);
          if (launchClassification.strategy_execution_readiness?.ready === false) {
            heroFlash = {
              ok: false,
              line: launchClassification.strategy_execution_readiness.reason ?? "That strategy needs setup before it can run."
            };
            return;
          }
          if (launchClassification.suggested_checks && !checksTouched) {
            heroChecks = launchClassification.suggested_checks;
            checksTouched = true;
            runChecks = launchClassification.suggested_checks
              .split("\n")
              .map((line) => line.trim())
              .filter(Boolean);
          }
          if (launchClassification.recommended_shape === "mission") {
            openMissionForm();
            return;
          }
        }
      }
      const target = runRepo === heroRepo
        ? executionTarget
        : resolveExecutionTarget({ mode: "local" });
      if (usesGovernedFleetPlan()) {
        try {
          const checks = runChecks.join("; ");
          const spec = `${heroIntent.trim()}\n\nOperator requested a ${executionWidth}-worker execution plan. Use up to ${executionWidth} independent streams, but split only where the seams are real and write scopes stay disjoint.${checks ? ` Checks to prove it: ${checks}.` : ""}`;
          const response = await fetch("/api/kernel/forge/fleet-plans.json", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
              goal: heroIntent.trim(),
              spec,
              repo_id: runRepo,
              actor_id: actor,
              requested_width: executionWidth,
              checks: runChecks,
              return_pending: true,
              execution_backend: target.backend,
              cloud_environment_id: target.cloudEnvironmentId
            })
          });
          const packet = await response.json();
          if (packet.status === "DRAFTED" || packet.status === "PLANNING") {
            const params = new URLSearchParams();
            if (packet.fleet_id) params.set("fleet_id", packet.fleet_id);
            if (runRepo) params.set("repo_id", runRepo);
            if (target.cloudEnvironmentId) {
              params.set("cloud_environment_id", target.cloudEnvironmentId);
            }
            const query = params.toString();
            await goto(query ? `/forge/fleets?${query}` : "/forge/fleets");
          } else {
            heroFlash = { ok: false, line: refusalLine(packet, { fallback: "That fleet plan did not land. Try a smaller width or tighter scope." }) };
          }
        } catch (error) {
          heroFlash = { ok: false, line: "Nothing planned - that did not land. Try again." };
        }
        return;
      }
      const allowed_commands = runChecks;
      try {
        const response = await fetch("/api/kernel/forge/runs.json", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            intent: heroIntent.trim(),
            repo_id: runRepo,
            allowed_commands,
            actor_id: actor,
            fleet_width: executionWidth,
            reasoning_effort: heroEffort,
            plan_only: heroPlanFirst,
            builder_slot: heroCast,
            execution_backend: target.backend,
            cloud_environment_id: target.cloudEnvironmentId || undefined,
            ...adaptiveStrategyFields()
          })
        });
        const packet = await response.json();
        if (packet.status === "RUN_STARTED") {
          // Hand off straight to the run you just started, opened and scrolled to,
          // so you watch your run instead of hunting the full list.
          const newRunId = packet.run_id ?? packet.attached_run_id ?? "";
          await goto(newRunId ? `/forge/runs?run=${encodeURIComponent(newRunId)}` : "/forge/runs");
        } else {
          heroFlash = { ok: false, line: refusalLine(packet, { fallback: "That did not start. Try again." }) };
        }
      } catch (error) {
        heroFlash = { ok: false, line: "Nothing started - that did not land. Try again." };
      }
    } finally {
      // The primary verb must never brick: whatever happened above, the
      // button comes back.
      heroBusy = false;
    }
  }

  // Classify-then-confirm: the operator describes the ask, Forge proposes run
  // vs mission and the eval task class, recorded as a receipt. It grants no
  // execution authority - the operator confirms the next move. Classification
  // is an explicit step (not on every keystroke) because it writes a receipt.
  let classifyBusy = $state(false);
  let classification = $state(null);
  let classifyFlash = $state(null);
  let externalAuthorizationBusy = $state(false);
  let externalAuthorizationFlash = $state(null);

  const TASK_CLASS_LABEL = {
    bug_fix: "bug fix",
    feature: "feature",
    refactor: "refactor",
    security: "security fix",
    ci_repair: "CI repair",
    docs_code: "docs and code change",
    multi_file: "multi-file change",
    architecture: "architecture change",
    api_compat: "API compatibility change",
    migration: "data migration",
    performance: "performance fix",
    concurrency: "concurrency fix",
    observability: "observability change",
    product_ux: "product workflow",
    long_horizon: "long-horizon mission"
  };
  const TIER_LABEL = { small: "small", medium: "medium", large: "large", xl: "extra-large", extreme: "extreme" };
  const taskClassLabel = (c) => TASK_CLASS_LABEL[c] ?? String(c ?? "").replace(/_/g, " ");
  const tierLabel = (t) => TIER_LABEL[t] ?? String(t ?? "");

  // Eval-lane coverage, joined to the classifier's task_class so the proposal
  // can show real per-class readiness, not a generic link. The classifier and
  // the scoreboard share one class vocabulary, so the join is direct. Honest:
  // local proof today, never "scores well" until live runner receipts exist.
  const evalCoverage = $derived.by(() => {
    const sb = data.evalScoreboard;
    if (!sb) return null;
    const tasks = Array.isArray(sb.benchmark_tasks) ? sb.benchmark_tasks : [];
    const byClass = {};
    for (const t of tasks) {
      const c = String(t?.class ?? "");
      if (c) byClass[c] = (byClass[c] ?? 0) + 1;
    }
    const liveResultCount = Number(
      sb.live_result_count ?? sb.result_count ?? sb.accepted_for_scoreboard_count ?? 0
    ) || 0;
    return {
      byClass,
      dims: Array.isArray(sb.scoring_dimensions) ? sb.scoring_dimensions.length : 0,
      providers: Array.isArray(sb.model_matrix) ? sb.model_matrix.length : 0,
      measuredLive: liveResultCount > 0 && sb.authority_boundary?.codex_cli_live_execution_allowed === true
    };
  });

  async function classifyAsk() {
    if (classifyBusy || !heroIntent.trim()) return;
    classifyBusy = true;
    classifyFlash = null;
    externalAuthorizationFlash = null;
    classification = null;
    try {
      let classifyRepo = heroRepo;
      if (shouldUsePlayground) {
        const playground = await ensurePlaygroundRepo();
        if (!playground) return;
        classifyRepo = playground.repoId;
      }
      const packet = await client.write(
        "/forge/work-classifications.json",
        {
          ask: heroIntent.trim(),
          repo: classifyRepo || "MDx",
          declared_checks: heroChecks.trim(),
          actor_id: actor,
          builder_slot: heroCast,
          fleet_width: executionWidth,
          ...adaptiveStrategyFields()
        },
        { receiptIntent: "forge_work_classification" }
      );
      if (packet.status === "FORGE_WORK_CLASSIFICATION_RECORDED") {
        classification = packet;
        executionWidth = recommendedExecutionWidth(packet);
      } else {
        classifyFlash = { line: packet.reason ?? "Could not read that ask. Try describing it again." };
      }
    } catch (error) {
      classifyFlash = { line: "That did not land. Try again." };
    } finally {
      classifyBusy = false;
    }
  }

  async function authorizeExternalStrategy(c) {
    if (externalAuthorizationBusy) return;
    const harness = String(c?.strategy?.harness_id ?? "");
    if (!['codex_cli', 'grok_build'].includes(harness)) return;
    const grok = harness === 'grok_build';
    const width = Math.max(1, Number(c?.strategy?.width ?? 1));
    const perWorkerCents = Math.max(1, Number(c?.strategy?.max_cost_cents ?? 500));
    externalAuthorizationBusy = true;
    externalAuthorizationFlash = null;
    try {
      const clearance = await client.write(
        "/forge/machine-league/closed-runner-clearances.json",
        {
          actor_id: actor,
          runner_id: grok ? "grok_build_cli_external_worker" : "codex_cli_external_worker",
          operator_attestation: true,
          clearance_mode: "fleet_eval",
          adapter_protocol_tier: grok ? "protocol_adapter" : "headless_cli_adapter"
        },
        { receiptIntent: "forge_external_harness_clearance" }
      );
      if (clearance?.status !== "CLOSED_RUNNER_CLEARANCE_RECORDED") {
        externalAuthorizationFlash = { ok: false, line: clearance?.reason ?? "The execution clearance was not recorded." };
        return;
      }
      const approval = await client.write(
        "/forge/fleet-eval-live-run-approvals.json",
        {
          actor_id: actor,
          approval_id: `forge_${harness}_${Date.now()}`,
          provider_allowlist: grok ? "xai" : "openai",
          max_spend_cents: perWorkerCents * width,
          max_tasks: width,
          max_parallel_agents: width,
          artifact_retention_policy: "quarantine_7_days",
          redaction_policy: "mdx_context_redaction_required",
          stop_conditions: "budget_exhausted,quality_gate_failure,manual_stop"
        },
        { receiptIntent: "forge_external_harness_live_run_approval" }
      );
      if (approval?.status !== "FLEET_EVAL_LIVE_RUN_APPROVAL_RECORDED") {
        externalAuthorizationFlash = { ok: false, line: approval?.blocked_reason ?? "The bounded live-run approval was not recorded." };
        return;
      }
      externalAuthorizationFlash = { ok: true, line: "Clearance and a one-run budget were recorded. Checking the server gate now." };
      const refreshed = await classifyForLaunch(heroRepo, heroChecks.split("\n").map((line) => line.trim()).filter(Boolean));
      if (refreshed) classification = refreshed;
    } catch (error) {
      externalAuthorizationFlash = { ok: false, line: "That authorization did not land. Check the signed-in operator session and try again." };
    } finally {
      externalAuthorizationBusy = false;
    }
  }

  function startFromClassification() {
    // Confirm execution: carry the suggested checks into the front-door
    // composer, then use the selected width to start one run or draft a fleet.
    if (classification?.suggested_checks) {
      heroChecks = classification.suggested_checks;
      checksTouched = true;
    }
    classification = null;
    startHeroBuild({ adapt: false });
  }

  function classificationWriteBoundary(c) {
    if (isExternalRepo) return "Confirmed from this repository before execution";
    return String(c?.allowed_write_scope ?? "").trim();
  }

  function dismissClassification() {
    classification = null;
  }

  // Slice 6: admit a mission. The classifier proposal prefills a mission
  // packet form; the operator reviews and confirms before anything is
  // recorded. Admission writes a packet and checkpoint plan and returns a
  // receipt - it does not start live workers or grant execution authority.
  let missionForm = $state(null);
  let missionBusy = $state(false);
  let missionResult = $state(null);
  let missionFlash = $state(null);

  function openMissionForm() {
    const c = classification ?? {};
    missionForm = {
      goal: String(c.ask ?? heroIntent ?? "").trim(),
      doneWhen: "",
      validationCommands: String(c.suggested_checks ?? heroChecks ?? "").trim(),
      allowedWriteScope: String(c.allowed_write_scope ?? "").trim(),
      blockedPaths: String(c.blocked_paths ?? "").trim(),
      nonGoals: "",
      fleetWidth: boundedExecutionWidth(executionWidth || recommendedExecutionWidth(c)),
      maxCostDollars: 10,
      maxRuntimeHours: 4,
      checkpointCadence: 30
    };
    missionFlash = null;
  }

  function cancelMissionForm() {
    missionForm = null;
  }

  async function admitMission() {
    if (missionBusy || !missionForm) return;
    const f = missionForm;
    if (!f.goal.trim() || !f.validationCommands.trim()) {
      missionFlash = { line: "A goal and at least one check are needed before Forge can shape the mission." };
      return;
    }
    if (isExternalRepo && !f.allowedWriteScope.trim()) {
      missionFlash = { line: "Name the exact files or folders this mission may change before Forge can shape it." };
      return;
    }
    missionBusy = true;
    missionFlash = null;
    try {
      const response = await fetch("/api/kernel/forge/long-horizon-missions.json", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          goal: f.goal.trim(),
          done_when: f.doneWhen.trim() || "the checks pass and receipts stay coherent",
          validation_commands: f.validationCommands.trim(),
          allowed_write_scope: f.allowedWriteScope.trim() || "repository scope to be confirmed",
          blocked_paths:
            f.blockedPaths.trim() || "provider secrets,production state,live adapter execution",
          non_goals:
            f.nonGoals.trim() || "no live provider calls,no adapter execution,no production writes",
          fleet_width: Math.max(1, Number(f.fleetWidth) || 1),
          max_cost_cents: Math.max(1, Math.round((Number(f.maxCostDollars) || 0) * 100)),
          max_runtime_ms: Math.max(60000, Math.round((Number(f.maxRuntimeHours) || 0) * 3_600_000)),
          checkpoint_cadence_minutes: Math.max(1, Number(f.checkpointCadence) || 1),
          actor_id: actor
        })
      });
      const packet = await response.json();
      if (packet.status === "FORGE_LONG_HORIZON_MISSION_ADMITTED") {
        missionResult = packet;
        missionForm = null;
        classification = null;
      } else {
        missionFlash = { line: packet.reason ?? "That did not land. Check the details and try again." };
      }
    } catch (error) {
      missionFlash = { line: "That did not land. Try again." };
    }
    missionBusy = false;
  }

  // The control-plane fold: the full decision inbox across fleets and runs, the
  // pipeline pulse, and live worker capacity. Each row routes to its surface.
  const controlPlane = $derived.by(() => {
    const cp = data.controlPlane;
    if (!cp) return null;
    return {
      needsYou: (Array.isArray(cp.needs_you) ? cp.needs_you : []).map((n) => ({
        id: String(n?.id ?? ""),
        kind: String(n?.kind ?? ""),
        label: String(n?.label ?? ""),
        line: String(n?.line ?? ""),
        href: String(n?.href ?? "/forge"),
        priority: Number(n?.priority ?? 999),
        actionState: String(n?.primary_action?.state ?? "review_required"),
        blockerCodes: Array.isArray(n?.primary_action?.blocker_codes) ? n.primary_action.blocker_codes.map(String) : [],
        evidenceReceiptIds: Array.isArray(n?.primary_action?.evidence_receipt_ids) ? n.primary_action.evidence_receipt_ids.map(String) : []
      })),
      pipeline: cp.pipeline_counts && typeof cp.pipeline_counts === "object" ? cp.pipeline_counts : null,
      capacity: cp.capacity_pulse && typeof cp.capacity_pulse === "object" ? cp.capacity_pulse : null
    };
  });
  const needsYou = $derived(controlPlane?.needsYou ?? []);
  // One source of truth for the decision count, matching Work and Controls
  // (the control-plane needs-you count) instead of double-counting the ship-door.
  const decisionsWaiting = $derived(Number(controlPlane?.needs_you_count ?? needsYou.length));
  // Where a grouped decision row routes, so the destination is visible up front.
  function destLabel(href) {
    const h = String(href ?? "");
    if (h.includes("/review")) return "Review";
    if (h.includes("/fleets")) return "Fleets";
    if (h.includes("/missions")) return "Missions";
    if (h.includes("/work")) return "Work";
    return "";
  }

  // Group the decision inbox by kind so a flood of one type (e.g. nine changes
  // ready to review) reads as one actionable row, not nine near-identical ones.
  const NEEDS_YOU_KINDS = {
    fleet_repair_escalation: { rank: 0, label: (n) => `${n} ${n === 1 ? "fleet" : "fleets"} to unblock` },
    ratify_plan: { rank: 1, label: (n) => `${n} ${n === 1 ? "split" : "splits"} to approve` },
    wide_plan_review: { rank: 2, label: (n) => `${n} ${n === 1 ? "plan" : "plans"} to review before they scale` },
    ship_call: { rank: 3, label: (n) => `${n} ${n === 1 ? "change" : "changes"} ready to review` }
  };
  const needsYouGroups = $derived.by(() => {
    const byKind = new Map();
    for (const item of needsYou) {
      const g = byKind.get(item.kind) ?? { kind: item.kind, count: 0, href: item.href, sample: item, priority: item.priority };
      g.count += 1;
      g.priority = Math.min(g.priority, item.priority);
      byKind.set(item.kind, g);
    }
    return [...byKind.values()]
      .map((g) => ({
        kind: g.kind,
        count: g.count,
        href: g.href,
        priority: g.priority,
        actionState: g.sample.actionState,
        blockerCodes: g.sample.blockerCodes,
        label: NEEDS_YOU_KINDS[g.kind] ? NEEDS_YOU_KINDS[g.kind].label(g.count) : `${g.count} ${g.sample.label || "decisions"}`,
        line: g.count === 1 ? g.sample.line : ""
      }))
      .sort((a, b) => a.priority - b.priority || (NEEDS_YOU_KINDS[a.kind]?.rank ?? 9) - (NEEDS_YOU_KINDS[b.kind]?.rank ?? 9));
  });
  const nextAction = $derived(forgeNextAction({ decisions: needsYouGroups, runs: recentRuns }));
  const composerPrimary = $derived(isComposerPrimary(nextAction));

  // The pipeline pulse, as a curated row of stages. Gate stages (a human is the
  // next step) are marked so the operator's job reads at a glance.
  const pipelineStages = $derived.by(() => {
    const p = controlPlane?.pipeline;
    if (!p) return [];
    const stage = (key, label, gate) => ({ label, count: Number(p[key] ?? 0), gate });
    return [
      stage("plan_review_needed", "Plan review", true),
      stage("approval_needed", "Approval", true),
      stage("building", "Building", false),
      stage("repairing", "Repairing", false),
      stage("needs_attention", "Attention", true),
      stage("review_needed", "Review", true),
      stage("ship_needed", "Ship call", true)
    ];
  });
  const capacityPulse = $derived.by(() => {
    const c = controlPlane?.capacity;
    if (!c || !c.worker_capacity) return null;
    return {
      active: Number(c.active_now ?? 0),
      capacity: Number(c.worker_capacity ?? 0),
      queued: Number(c.queue_depth ?? 0)
    };
  });

  // Fields the standing-authorization fit panel reads. They describe the task
  // the operator would hand in; with no structured intake wired to them today
  // they hold their honest defaults and the panel says so.
  let scope = $state("");
  let riskHint = $state("low");
  let preferredEnvelope = $state("");
  let humanReview = $state(false);

  // The command surface. Cmd/Ctrl+K opens it; it is a labeled dialog that traps
  // focus, filters as you type, moves the highlight with the arrows, runs on
  // Enter, and restores focus on Escape. Every command is navigation, a
  // selection, a filter, or a clipboard copy - none introduces a write.
  let paletteOpen = $state(false);
  let paletteQuery = $state("");
  let paletteIndex = $state(0);
  let paletteReturnFocus = null;
  let paletteInputEl = $state(null);
  let paletteListEl = $state(null);

  // Starting work lives on Overview. Work is the read surface after Forge has
  // something underway.
  function focusComposer() {
    document.querySelector(".fs-intent")?.scrollIntoView({ behavior: "smooth", block: "center" });
    queueMicrotask(() => document.querySelector(".fs-intent")?.focus?.());
  }
  // The command set. Each command is a label, a short hint, and a run action.
  // Nothing here writes to the kernel; the governed writes stay on their own
  // buttons. Commands that need a build select the most relevant one first.
  const COMMANDS = [
    { id: "new", label: "New build", hint: "Focus the composer to hand Forge a change.", run: focusComposer },
    { id: "trails", label: "Open Trails", hint: "Every run and its proof.", run: () => goto("/forge/runs") },
    { id: "review", label: "Open Review", hint: "Changes waiting on your call.", run: () => goto("/forge/review") }
  ];
  const filteredCommands = $derived.by(() => {
    const q = paletteQuery.trim().toLowerCase();
    if (!q) return COMMANDS;
    return COMMANDS.filter(
      (command) => command.label.toLowerCase().includes(q) || command.hint.toLowerCase().includes(q)
    );
  });

  function openPalette() {
    paletteReturnFocus = document.activeElement;
    paletteQuery = "";
    paletteIndex = 0;
    paletteOpen = true;
    queueMicrotask(() => paletteInputEl?.focus());
  }
  function closePalette() {
    paletteOpen = false;
    const target = paletteReturnFocus;
    paletteReturnFocus = null;
    queueMicrotask(() => target?.focus?.());
  }
  function runCommand(command) {
    if (!command) return;
    closePalette();
    queueMicrotask(() => command.run());
  }

  function onPaletteKeydown(event) {
    const commands = filteredCommands;
    if (event.key === "Escape") {
      event.preventDefault();
      closePalette();
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      paletteIndex = commands.length ? (paletteIndex + 1) % commands.length : 0;
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      paletteIndex = commands.length ? (paletteIndex - 1 + commands.length) % commands.length : 0;
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      runCommand(commands[paletteIndex]);
      return;
    }
    if (event.key === "Tab") {
      // Trap focus: the dialog holds a single input and a list, so we keep the
      // input focused rather than letting focus leave the dialog.
      event.preventDefault();
      paletteInputEl?.focus();
    }
  }

  // Keep the highlight valid as the filter narrows.
  $effect(() => {
    const count = filteredCommands.length;
    if (paletteIndex >= count) paletteIndex = count > 0 ? count - 1 : 0;
  });

  function onGlobalKeydown(event) {
    if ((event.metaKey || event.ctrlKey) && (event.key === "k" || event.key === "K")) {
      event.preventDefault();
      if (paletteOpen) closePalette();
      else openPalette();
    }
  }

  // The empty state earns its calm: with no runs, Forge leads with the one
  // thing to do (describe work) and lets the depth unfold as work appears.
  const hasRuns = $derived(recentRuns.length > 0);
  const hasActiveEnvelope = $derived(data.envelopeProjection?.active_authorization === true);

  // First-run golden path, done in place: connect a model, connect a repo,
  // describe the work. The strip is the progress map; the active step's actual
  // input appears inline below it - no jumping to another page, no dead scroll.
  // It retires once a build exists.
  const forgeBuilderReadiness = $derived(
    (data.modelReadiness?.consumer_readiness ?? []).find(
      (workload) => workload?.workload_id === "mdx/forge/builder"
    ) ?? null
  );
  const modelsReady = $derived(forgeBuilderReadiness?.ready === true);
  // Count what's actually shown (visibleRepos), not the raw ledger - otherwise
  // a repo hidden from the list still counts and the strip and door disagree.
  const connectedRepoCount = $derived(visibleRepos.length);
  const showSetup = $derived(data.kernelReachable && !hasRuns);
  // Just-in-time helper: the first time a build is actually in flight.
  const anyBuildRunning = $derived(recentRuns.some((run) => isRunning(run)));
  // currentStep = the furthest step still to do; activeStep = the panel shown,
  // which follows currentStep until you click back to a completed step.
  const currentStep = $derived(!modelsReady ? 1 : connectedRepoCount === 0 ? 2 : 3);
  let pickedStep = $state(0);
  const activeStep = $derived(pickedStep || currentStep);
  function pickStep(n) {
    if (n <= currentStep) pickedStep = n;
  }
  // Inside the setup wizard, step 2 is the repo view (connect + connected
  // repos) and step 3 is the composer - so the two steps look different. For
  // returning users the door/composer still toggles on whether work is engaged.
  // The repo-first "door" is only the setup wizard's step 2. Outside setup the
  // composer always leads with the ask box - the repo is quiet context, not a
  // gate you have to clear before you can type.
  const showStartDoor = $derived(showSetup && activeStep === 2);
  // Which models are connected, by name - so step 1 shows what landed and you
  // can add more (a fleet wants several: a fast builder, a strong reviewer).
  const connectedModels = $derived(
    (Array.isArray(data.modelReadiness?.connections) ? data.modelReadiness.connections : [])
      .filter((connection) => connection?.credential_available === true)
      .map((connection) => ({
        family: String(connection?.provider_id ?? ""),
        label: String(connection?.provider_id ?? "model"),
        source: String(connection?.credential_source ?? "configured")
      }))
  );

  // The autonomy envelope: a standing operating policy, not a per-task call.
  // An envelope is what lets some work complete on the human's prior
  // authorization instead of stopping for review every time. The active list is
  // read from the standing-authorization registry route; in the running app it
  // reports none recorded, so the list is honestly empty. We describe what an
  // envelope defines, and never fabricate one as if it were live.
  const ENVELOPE_FIELDS = [
    { name: "Owner", meaning: "The person whose name stands behind the authorization. They answer for what completes under it." },
    { name: "Scope", meaning: "The repos, services, and paths the envelope reaches. Work outside this set always reviews with you." },
    { name: "What it can finish on its own", meaning: "The kinds of change it can complete without you - a dependency bump, a docs edit, a test fix. Anything else comes back to you." },
    { name: "How far it can go", meaning: "The most it will take on by itself. Anything riskier than this stops and waits for you." },
    { name: "Required tests and evals", meaning: "The checks that must come back clear before anything completes under it. No clear result, no completion." },
    { name: "Rollback requirement", meaning: "A way back must exist before a change lands. If the work cannot be undone, the envelope does not cover it." },
    { name: "Expiry", meaning: "When the standing authorization lapses. After it expires, work reviews with you again until an owner renews it." },
    { name: "Escalation triggers", meaning: "The conditions that hand the call back to a human even inside scope - a failed check, a risk jump, a touched boundary." }
  ];

  // The named conditions that hand the call back to a human, even inside a
  // covering envelope. These are the standing escalation triggers an envelope
  // carries plus the four state conditions the policy always enforces. With no
  // authorization active they read as the model; when one is live, the run
  // itself is what fires them.
  const ESCALATION_TRIGGERS = [
    { name: "Out of scope", when: "The change touches a repo, service, or path the envelope does not reach." },
    { name: "Over the risk ceiling", when: "The task is rated higher than the envelope's risk ceiling." },
    { name: "A check did not pass", when: "A required test or eval came back failed, or could not be run." },
    { name: "No rollback", when: "There is no proven way to undo the change before it lands." },
    { name: "Expired", when: "The standing authorization has lapsed and no owner has renewed it." },
    { name: "Unknown work class", when: "The kind of change is not one the envelope preapproves." }
  ];

  // The live list, read from the standing-authorization registry route. When
  // the route is unavailable we keep an empty list and the honest empty copy -
  // we never seed it with example rows dressed up as active. Each row is read
  // defensively against the envelope's own field names so a real recorded
  // envelope renders its facts and nothing is invented where a field is absent.
  const activeEnvelopes = $derived(
    Array.isArray(data.envelopeProjection?.envelopes) ? data.envelopeProjection.envelopes : []
  );
  const hasActiveAuthorization = $derived(data.envelopeProjection?.active_authorization === true);

  function listOrNone(value) {
    if (Array.isArray(value)) return value.filter(Boolean).join(", ");
    return typeof value === "string" ? value : "";
  }
  // The facts of a real recorded envelope, read from its own receipt fields.
  // Where the registry has not stated a field, the value is left blank and the
  // markup shows "not stated" rather than a fabricated default.
  function envelopeFacts(env) {
    const owner = env.owner_id ?? env.owner ?? "";
    const scopeText = listOrNone(env.scopes ?? env.scope ?? env.allowed_path_scopes);
    const classes = listOrNone(env.work_classes ?? env.preapproved_work_classes);
    const ceiling = env.max_risk ?? env.risk_ceiling ?? env.max_risk_class ?? "";
    const expiry = env.expires_at ?? env.expiry ?? "";
    const evals = env.eval_required === true || env.required_evals ? "required to pass" : env.eval_required === false ? "not required" : "";
    const rollback = env.rollback_required === true ? "required" : env.rollback_required === false ? "not required" : "";
    const triggers = listOrNone(env.triggers ?? env.escalation_triggers);
    return {
      id: env.envelope_id ?? env.id ?? "",
      owner,
      scopeText,
      classes,
      ceiling,
      expiry,
      evals,
      rollback,
      triggers,
      active: env.active !== false
    };
  }

  // Whether the current intake sits inside or outside an envelope, and a plain
  // at-a-glance verdict. With no active authorization the honest answer is
  // always "not covered - it reviews with you," but each line is built from the
  // real entered risk, scope, and preferred-envelope fields so it reflects this
  // task. Each requirement carries a small state so the engineer can see which
  // ones their intake already satisfies and which are still open.
  const envelopeFitForTask = $derived.by(() => {
    const hasScope = scope.trim().length > 0;
    const named = preferredEnvelope.trim();
    const noneRecorded = !hasActiveAuthorization;
    const recordedNote = noneRecorded ? "none is recorded yet" : "matched against the ones recorded";
    const requirements = [
      {
        state: noneRecorded ? "open" : named ? "set" : "open",
        text: named
          ? `An active envelope with id "${named}" would have to exist - ${recordedNote}.`
          : noneRecorded
            ? "An active envelope would have to exist - none is recorded yet, so name one or leave it to review."
            : "An active envelope would have to cover this task - name one or leave it to review."
      },
      {
        state: hasScope ? "set" : "open",
        text: hasScope
          ? `Its scope would have to include the paths you named (${scope.trim()}).`
          : "Its scope would have to include this task's paths - you have not named any yet, so cover would be ambiguous."
      },
      {
        state: "set",
        text: `Its risk ceiling would have to be at least "${riskHint}" to carry this task on its own.`
      },
      { state: "open", text: "The change's work class would have to be one the envelope preapproves." },
      { state: "open", text: "Its required tests and evals would have to pass, and a rollback path would have to exist." },
      { state: "open", text: "It would have to be unexpired, with no escalation trigger firing on this task." }
    ];
    // The at-a-glance state: covered only when an authorization is live and the
    // human did not ask to review. Today that is never true, so this reads
    // "reviews with you" honestly while still naming why.
    let coverage;
    if (humanReview) {
      coverage = { tone: "review", label: "Reviews with you", line: "You asked to review this one yourself, so it stops with you whatever an envelope might say." };
    } else if (noneRecorded) {
      coverage = { tone: "review", label: "Comes back to you", line: "Forge isn't set to finish this on its own yet, so it stops with you. Below is exactly what it would need to." };
    } else {
      coverage = { tone: "check", label: "Checked against your envelopes", line: "An authorization is active - this task is matched against it on the conditions below." };
    }
    return { coverage, requirements };
  });

  // The work graph: the one mental model for how work moves through Forge -
  // Intent, Plan, Run, Change, Evidence, Decision. A static orienting map;
  // the live per-run truth lives on Work and Review.
  const WORK_GRAPH = [
    { key: "intent", label: "Intent", whose: "you", model: "You hand Forge a change to make." },
    { key: "plan", label: "Plan", whose: "forge", model: "Forge shapes a plan, then proves it without running anything." },
    { key: "run", label: "Run", whose: "forge", model: "It works through the plan one step at a time - nothing acts on its own." },
    { key: "change", label: "Change", whose: "forge", model: "The proposed edit is previewed, short of touching anything live." },
    { key: "evidence", label: "Evidence", whose: "forge", model: "Receipts, checks, and the verdict gather into a packet you can read." },
    { key: "decision", label: "Decision", whose: "you", model: "A person reads the evidence and puts their name on what happens next." }
  ];

  onMount(() => {
    // Repos the operator has archived from the door (a durable view
    // preference; the connect receipts stay on the ledger untouched).
    try {
      const stored = JSON.parse(localStorage.getItem(HIDDEN_REPOS_KEY) ?? "[]");
      if (Array.isArray(stored)) hiddenRepoIds = stored.map(String);
    } catch (error) {
      // no stored archive yet
    }
    try {
      const params = new URLSearchParams(location.search);
      // A handed-in ask belongs at the single front door.
      const handed = params.get("intent");
      if (handed) {
        heroIntent = handed;
        repoTouched = true;
        if (modelsReady) pickedStep = 3;
        if (params.get("source") === "first-mission") {
          handoffNote = "Your setup ask is ready here. Press Plan it or Start a run now. Broad demo builds use the local playground so Forge can build freely without touching MDx itself.";
        }
        // composerEngaged derives from heroIntent, so the full composer
        // discloses on its own; the old repoStep assignment was a fossil
        // that threw and silently killed this hand-off.
      }
    } catch (error) {
      // hand-off is best effort
    }
  });
</script>

<svelte:head><title>Forge - MDx</title></svelte:head>

<svelte:window onkeydown={onGlobalKeydown} />

<section class="forge" data-route-state="ready">
  <header class="forge-head">
    <div class="mdx-page-head">
      <h1>Forge</h1>
      <p class="mdx-page-sub">Hand a change to agents and review the branch they bring back.</p>
    </div>
    <div class="head-aside">
      <nav class="mdx-segmented" aria-label="Forge views">
        <a class="active" href="/forge" aria-current="page">Build</a>
        <a href="/forge/runs">Trails</a>
        <a href="/forge/review">Review</a>
        <a href="/forge/models">Models</a>
        <a href="/forge/controls">Controls</a>
      </nav>
      <button type="button" class="cmd-open" onclick={openPalette} aria-haspopup="dialog" title="Quick actions">
        <kbd class="cmd-kbd" aria-hidden="true">⌘K</kbd>
      </button>
    </div>
  </header>

  {#if data.githubNotice}
    <p class:bad={!data.githubNotice.ok} class="flash" role="status">{data.githubNotice.line}</p>
  {/if}

  <MarketplacePackPrompt app="forge" objectId={heroRepo || "new-build"} task={heroIntent} compact />

  <section class="next-action" data-kind={nextAction.kind} aria-label="Your next move">
    <div class="na-mark" aria-hidden="true">
      {#if nextAction.kind === "working"}<span class="na-pulse"></span>{:else}<span>&rarr;</span>{/if}
    </div>
    <div class="na-copy">
      <span class="na-eyebrow">{nextAction.eyebrow}</span>
      <strong>{nextAction.label}</strong>
      <p>{nextAction.line}</p>
      <span class="na-boundary">Nothing ships without your call.</span>
      {#if nextAction.additionalWaiting > 0}
        <a class="na-queue" href={nextAction.queueHref}>
          +{nextAction.additionalWaiting} more waiting
        </a>
      {/if}
    </div>
    <a class:primary={nextAction.primary} class="mdx-btn na-cta" href={nextAction.href}>{nextAction.cta}</a>
  </section>

  {#snippet forgeDashboard()}
  {#if activeMission}
    <a class="active-mission" href="/forge/missions" data-tone={activeMission.tone}>
      <div class="am-top">
        <span class="am-eyebrow">{activeMission.count === 1 ? "Active mission" : `${activeMission.count} active missions`}</span>
        {#if activeMission.total > 0}
          <span class="am-progress">{activeMission.passed} of {activeMission.total === 1 ? "1 check" : `${activeMission.total} checks`} passed</span>
        {/if}
      </div>
      <p class="am-goal">{activeMission.goal}</p>
      <p class="am-line">{activeMission.line}</p>
      <span class="am-open">Open mission &rarr;</span>
    </a>
  {/if}

  {#if !showSetup && controlPlane && pipelineStages.length}
    <section class="pipeline-band" aria-label="What is running">
      <div class="pb-stages">
        {#each pipelineStages as stage (stage.label)}
          <div class="pb-stage" data-gate={stage.gate} data-active={stage.count > 0}>
            <span class="pb-count">{stage.count}</span>
            <span class="pb-label">{stage.label}</span>
          </div>
        {/each}
      </div>
      {#if capacityPulse}
        <p class="pb-capacity">
          <span class="pb-dot" class:live={capacityPulse.active > 0} aria-hidden="true"></span>
          {capacityPulse.active} of {capacityPulse.capacity} workers active{#if capacityPulse.queued} &middot; {capacityPulse.queued} queued{/if}. Forge reserves, queues, then refuses overflow - it never overruns capacity.
        </p>
      {/if}
    </section>
  {/if}

  {#if !showSetup && builderLoops.length}
    <section class="pipeline-band" aria-label="Build automations">
      <div class="dec-head">
        <span class="dec-eyebrow">Build automations</span>
        <span class="dec-count">{builderLoops.length} available</span>
      </div>
      <div class="pb-stages">
        {#each builderLoops as loop (loop.id)}
          <article class="pb-stage" data-active={Number(loop.state?.tick_count ?? 0) > 0} data-gate="true">
            <strong class="pb-count">{loop.title}</strong>
            <span class="pb-label">{builderLoopStateLine(loop)}</span>
            {#if loop.state?.review_packet_route}
              <a class="qa-link" href="/forge/review">Review packet</a>
            {/if}
            <button
              type="button"
              class="mdx-btn small"
              aria-label="Run {loop.title}"
              onclick={() => tickBuilderLoop(loop)}
            >
              Run now
            </button>
          </article>
        {/each}
      </div>
    </section>
  {/if}
  {/snippet}

  {#if !showSetup && recentRuns.length === 0}
    <FirstUseHint
      surface="forge"
      title="First time in Forge?"
      body="Forge hands real work to coding agents and brings back a branch for you to review - it's not a chatbot. Describe a change, approve the plan it proposes, and review the branch it brings back. Nothing ships without you."
    />
  {/if}

  {#if anyBuildRunning}
    <FirstUseHint
      surface="forge-build-running"
      title="Forge is building"
      body="Follow its translated trail under Trails. Nothing ships until you say so - the change comes back to Review when it's ready for your call."
    />
  {/if}

  {#if !data.kernelReachable}
    <section class="setup-strip kernel-down" aria-label="MDx is not reachable">
      <p class="ss-lead">MDx isn't reachable on this Mac</p>
      <p class="kd-line">
        The local server isn't answering, so nothing here is missing - it just can't be read yet.
        The bar at the top has a Retry, or start it from the MDx repo and this page connects on its own:
      </p>
      <p class="kd-cmd"><code>sh scripts/dogfood-stack.sh</code></p>
    </section>
  {/if}

  {#if showSetup}
    <section class="setup-strip" aria-label="Get set up">
      <p class="ss-lead">Three steps to your first build</p>
      <ol class="ss-steps">
        <li>
          <button type="button" class="ss-step" data-done={modelsReady} data-current={activeStep === 1} disabled={1 > currentStep} onclick={() => pickStep(1)}>
            <span class="ss-mark" aria-hidden="true">{modelsReady ? "✓" : "1"}</span>
            <span class="ss-body">
              <strong>Connect a model</strong>
              <span class="ss-state">{connectedModels.length === 0 ? (currentStep === 1 ? "Start here" : "") : connectedModels.length === 1 ? `${connectedModels[0].label} connected` : `${connectedModels.length} models connected`}</span>
            </span>
          </button>
        </li>
        <li>
          <button type="button" class="ss-step" data-done={connectedRepoCount > 0} data-current={activeStep === 2} disabled={2 > currentStep} onclick={() => pickStep(2)}>
            <span class="ss-mark" aria-hidden="true">{connectedRepoCount > 0 ? "✓" : "2"}</span>
            <span class="ss-body">
              <strong>Connect a repo</strong>
              <span class="ss-state">{connectedRepoCount > 0 ? `${connectedRepoCount} connected` : currentStep === 2 ? "Do this next" : ""}</span>
            </span>
          </button>
        </li>
        <li>
          <button type="button" class="ss-step" data-current={activeStep === 3} disabled={3 > currentStep} onclick={() => pickStep(3)}>
            <span class="ss-mark" aria-hidden="true">3</span>
            <span class="ss-body">
              <strong>Describe the work</strong>
              <span class="ss-state">{currentStep === 3 ? "You're ready" : ""}</span>
            </span>
          </button>
        </li>
      </ol>
    </section>
  {/if}

  {#if showSetup && activeStep === 1}
    <section class="step-card" aria-label="Connect a model">
      <h2 class="sc-title">Connect a model</h2>
      {#if connectedModels.length}
        <div class="mc-connected">
          <span class="mc-conn-label">Connected</span>
          {#each connectedModels as model (model.family)}
            <span class="mc-chip">
              {model.label}
              <small>{model.source}</small>
            </span>
          {/each}
        </div>
      {/if}
      <p class="sc-frame">
        {#if modelsReady}
          Model Fabric is ready. The same connection now serves Forge, Twin, web, macOS, and paired iPhone workflows.
        {:else if connectedModels.length}
          Your model connection is healthy, but Forge still needs one capability check before it can build safely.
        {:else}
          Model Center connects managed, provider, enterprise, or local models once and verifies them before Forge can spend or run.
        {/if}
      </p>
      <p class="mc-hint">
        {forgeBuilderReadiness?.next_action ?? data.modelReadiness?.recommended_next_action ?? "Connect a model in Model Center."}
      </p>
      <div class="mc-actions">
        <a class="mdx-btn primary" href="/admin/models">Open Model Center &rarr;</a>
        {#if modelsReady}
          <button type="button" class="mdx-btn primary mc-next" onclick={() => pickStep(2)}>Connect a repo &rarr;</button>
        {/if}
      </div>
    </section>
  {/if}

  {#if !showSetup || activeStep >= 2}
  <section id="start-build" class="forge-start" aria-label="Start a build">
    {#snippet connectForm(variant)}
      <div class="fs-connect" data-variant={variant}>
        <RepoConnect actor={actor} hosted={data.session?.authenticated === true} onConnected={handleRepoConnected} />
      </div>
    {/snippet}

    <h2 class="fs-title">{showStartDoor ? "Point Forge at your code" : "What should I build or fix?"}</h2>

    {#if showStartDoor}
      <div class="fs-door" in:fade={{ duration: 140 }}>
        <p class="fs-door-sub">Forge works in an isolated copy of your repo, proves each change with the checks you name, and hands you a branch to review. Your real files are never touched.</p>
        {#if visibleRepos.length > 0}
          <div class="fs-door-pick">
            <p class="fs-door-pick-head">{showSetup ? (visibleRepos.length === 1 ? "Connected" : "Connected repos") : "Pick up where you left off"}</p>
            <div class="fs-repo-chips">
              {#each visibleRepos as repo (repo.repoId)}
                {@const lang = repoLanguageLabel(repo)}
                <div class="fs-repo-chip">
                  <button type="button" class="chip-pick" onclick={() => chooseRepo(repo.repoId)}>
                    <strong>{repo.label || repo.repoId}</strong>
                    {#if lang}<span>{lang}</span>{/if}
                  </button>
                  <button type="button" class="chip-x" aria-label={`Remove ${repo.label || repo.repoId}`} title="Remove from this list" onclick={() => disconnectRepo(repo.repoId)}>&times;</button>
                </div>
              {/each}
            </div>
          </div>
        {/if}
        <div class="fs-door-connect">
          <p class="fs-door-eyebrow">{visibleRepos.length > 0 ? "Or connect another repo" : "Connect your repo"}</p>
          {@render connectForm("door")}
        </div>
        <button type="button" class="fs-door-mdx" onclick={chooseMdx}>No repo yet? Try a build on this app's own code &rarr;</button>
        {#if showSetup && connectedRepoCount > 0}
          <button type="button" class="mdx-btn primary fs-door-next" onclick={() => pickStep(3)}>Describe the work &rarr;</button>
        {/if}
      </div>
    {:else}
      {#if connectedNote}
        <p class="fs-connected" role="status" in:fly={{ y: -4, duration: 160 }}>
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>
          {connectedNote}
        </p>
      {/if}
      {#if handoffNote}
        <p class="fs-handoff" role="status">{handoffNote}</p>
      {/if}
      <textarea
        class="fs-intent" aria-label="Describe the change you want built"
        rows="3"
        bind:value={heroIntent}
        placeholder={intentPlaceholder}
      ></textarea>
      {#if heroIntent.trim()}
        {@const liveRecommendation = classification ?? {}}
        <div
          class="fs-run-plan"
          data-lane={usesGovernedFleetPlan() ? "fleet" : usesDirectCandidateRun() ? "candidates" : "solo"}
          aria-live="polite"
        >
          <div class="frp-copy">
            <span class="frp-eyebrow">How Forge will run it</span>
            <strong>{recommendationLabel(liveRecommendation)}</strong>
            <p>{recommendationWhy(liveRecommendation)}</p>
          </div>
          <div class="frp-facts" aria-label="Run shape">
            <span><b>{checkCount}</b> {checkCount === 1 ? "check" : "checks"}</span>
            <span><b>{harnessLabel}</b> harness</span>
            <span><b>{castLabel}</b> model</span>
            <span><b>{plannerLabel}</b> plan</span>
            <span><b>{reviewLabel}</b> review</span>
            <span><b>{effortLabel}</b> effort</span>
            {#if !widthTouched && !classification}
              <span class="frp-auto">Auto-read before launch</span>
            {/if}
          </div>
        </div>
      {/if}
      {#if shouldUsePlayground || playgroundNote}
        <div class="fs-playground" role="status">
          <span class="fs-playground-dot" aria-hidden="true"></span>
          <div>
            <strong>{playgroundRepo ? "Playground ready" : "Free-build playground"}</strong>
            <p>{playgroundNote || "This looks like a broad demo build. Forge will run it in a tiny local playground repo instead of the strict MDx codebase."}</p>
          </div>
          {#if shouldUsePlayground}
            <button type="button" disabled={playgroundBusy} onclick={ensurePlaygroundRepo}>
              {playgroundBusy ? "Preparing..." : playgroundRepo ? "Use playground" : "Create playground"}
            </button>
          {/if}
        </div>
      {/if}
      {#if broadDemoNeedsRepo}
        <div class="fs-playground" role="status">
          <span class="fs-playground-dot" aria-hidden="true"></span>
          <div>
            <strong>Connect the repo you want to build</strong>
            <p>Hosted Forge needs a connected repo before it can create this project in an isolated cloud workspace.</p>
          </div>
        </div>
      {/if}
      <div class="fs-verbs" aria-label="Start from a kind of work">
        <span class="fs-verbs-label">Recipes - proven ways to start:</span>
        {#each ACTION_VERBS as v (v.id)}
          <button type="button" class="fs-verb" onclick={() => applyActionVerb(v)}>{v.label}</button>
        {/each}
        {#each buildRecipes.slice(0, 3) as recipe (recipe.id)}
          <button type="button" class="fs-verb" onclick={() => useRecipe(recipe)}>{recipe.title || recipe.intent}</button>
        {/each}
      </div>
      {#if isExternalRepo}
      <div class="fs-repo-intel" aria-live="polite" in:fly={{ y: 4, duration: 160 }}>
        {#if String(cloudSetup?.hosted_sandbox_status ?? "") !== "CONFIGURATION_REQUIRED"}
          {#if selectedCloudEnvironment}
            <p class="fri-note">Cloud sandbox ready - checks run in its isolated workspace.</p>
          {:else}
            <form method="POST" action="?/prepareCloudSandbox">
              <input type="hidden" name="repo_id" value={heroRepo} />
              <button type="submit" class="mdx-btn primary">Prepare cloud sandbox</button>
            </form>
          {/if}
          {#if form?.cloudSandboxLine}
            <p class="fri-note" role="status">{form.cloudSandboxLine}</p>
          {/if}
        {/if}
        <div class="fri-head">
          <p class="fri-eyebrow">
            <svg viewBox="0 0 24 24" width="13" height="13" fill="currentColor" aria-hidden="true"><path d="M12 2l1.9 5.6L19.5 9l-4.6 3.3 1.6 5.7L12 14.8 7.5 18l1.6-5.7L4.5 9l5.6-1.4z"/></svg>
            {#if repoIntelBusy}
              Reading {selectedBuildRepo?.label || "the repo"}...
            {:else if repoIntelTasks.length > 0}
              {tasksAreFindings ? `Forge spotted these in ${selectedBuildRepo?.label || "your code"}` : `Good ways to start on ${selectedBuildRepo?.label || "this repo"}`}
            {:else}
              Forge read {selectedBuildRepo?.label || "this repo"}
            {/if}
          </p>
          <span class="fri-status" data-tone={repoReadyTone}>{repoReadyLabel}</span>
        </div>
        {#if proofPreflightView && proofPreflightView.missing.length > 0}
          <div class="fri-proof" data-tone={proofPreflightView.tone}>
            <div>
              <span>{proofPreflightView.label}</span>
              <strong>{proofPreflightView.command}</strong>
            </div>
            <p>{proofPreflightView.line}</p>
            <p class="fri-proof-missing">Still needs: {proofPreflightView.missing.join(", ")}</p>
          </div>
        {/if}
        {#if repoIntelTasks.length > 0}
          <div class="fri-tasks" aria-label="Suggested moves">
            {#each repoIntelTasks.slice(0, 3) as task (task.task_id)}
              <button type="button" class="fri-task" onclick={() => applyRepoIntelTask(task)}>
                <span class="fri-task-top">
                  <span class="fri-task-kind">{taskClassLabel(task.kind || task.task_class) || "move"}</span>
                  {#if task.path}<span class="fri-task-loc">{task.path}{task.line ? `:${task.line}` : ""}</span>{/if}
                </span>
                <strong>{task.title}</strong>
                <span class="fri-task-use">Use this &rarr;</span>
              </button>
            {/each}
          </div>
          <p class="fri-note">Pick one to fill the box - nothing runs until you say go.{#if repoSourceHostLine}<span class="fri-foot"> {repoSourceHostLine}</span>{/if}</p>
        {:else if repoIntelBusy}
          <div class="fri-tasks" aria-hidden="true">
            <div class="fri-task fri-skeleton-card"></div>
            <div class="fri-task fri-skeleton-card"></div>
          </div>
        {:else}
          <p class="fri-note">No standout move yet - just describe what you want above.</p>
        {/if}
      </div>
    {/if}
      <div class="fs-bar">
        <div class="fs-bar-ctx">
          <div class="fs-ctl fs-ctl-repo">
            <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z"/></svg>
            <select bind:value={heroRepo} aria-label="Repo for this build">
              {#each visibleRepos as repo (repo.repoId)}<option value={repo.repoId}>{repo.label || repo.repoId}</option>{/each}
              <option value="">MDx (this app)</option>
            </select>
          </div>
          <details class="fs-ctl-pop">
            <summary class="fs-ctl">Checks{#if checkCount > 0}<span class="fs-ctl-badge">{checkCount}</span>{/if}</summary>
            <div class="fs-pop">
              <label class="fs-label" for="fs-checks">Checks that prove it</label>
              <textarea id="fs-checks" class="fs-pop-checks" rows="3" bind:value={heroChecks} placeholder={checksPlaceholder} oninput={() => (checksTouched = true)}></textarea>
              {#if checkHint}<p class="fs-check-hint">{checkHint}</p>{/if}
            </div>
          </details>
          <details class="fs-ctl-pop">
            <summary class="fs-ctl">{executionWidth === 1 ? "Solo" : `${executionWidth} wide`}</summary>
            <div class="fs-pop">
              <span class="fs-label">How wide to run</span>
              <div class="fs-scale-options" role="group" aria-label="Execution width">
                {#each executionWidthOptions as option (option.value)}
                  <button
                    type="button"
                    class:active={executionWidth === option.value}
                    onclick={() => chooseExecutionWidth(option.value)}
                    aria-pressed={executionWidth === option.value}
                    title={option.line}
                  >
                    {option.label}
                  </button>
                {/each}
              </div>
              <p class="fs-pop-line">{executionScaleLine()}</p>
            </div>
          </details>
          <details class="fs-ctl-pop">
            <summary class="fs-ctl">{executionTarget.label}</summary>
            <div class="fs-pop">
              <span class="fs-label">Where it runs</span>
              <div class="fs-scale-options" role="group" aria-label="Execution location">
                <button type="button" class:active={executionTargetMode === "auto"} onclick={() => (executionTargetMode = "auto")} aria-pressed={executionTargetMode === "auto"}>Auto</button>
                <button type="button" class:active={executionTargetMode === "local"} onclick={() => (executionTargetMode = "local")} aria-pressed={executionTargetMode === "local"}>Current host</button>
                <button type="button" class:active={executionTargetMode === "cloud"} onclick={() => (executionTargetMode = "cloud")} aria-pressed={executionTargetMode === "cloud"}>Cloud</button>
              </div>
              <p class="fs-pop-line">{executionTarget.line}</p>
            </div>
          </details>
          {#if compatibleCastOptions.length}
            <details class="fs-ctl-pop">
              <summary class="fs-ctl">{castLabel}</summary>
              <div class="fs-pop">
                <span class="fs-label">Which machine runs it</span>
                <div class="fs-scale-options" role="group" aria-label="Which machine runs it">
                  <button type="button" class:active={heroCast === ""} onclick={() => (heroCast = "")} aria-pressed={heroCast === ""}>Auto-cast</button>
                  {#each compatibleCastOptions as option (option.slot)}
                    <button type="button" class:active={heroCast === option.slot} onclick={() => (heroCast = option.slot)} aria-pressed={heroCast === option.slot}>
                      {option.label}{#if option.sovereign} - sovereign{/if}
                    </button>
                  {/each}
                </div>
                <p class="fs-pop-line">Model and harness are separate choices. Auto-cast uses the harness default; named choices show only pairings the selected harness can run. MDx Native supports every connected provider.</p>
              </div>
            </details>
          {/if}
          <details class="fs-ctl-pop">
            <summary class="fs-ctl">{effortLabel}</summary>
            <div class="fs-pop">
              <span class="fs-label">How hard to think</span>
              <div class="fs-scale-options" role="group" aria-label="How hard to think">
                {#each effortChoices as choice (choice.value)}
                  <button
                    type="button"
                    class:active={heroEffort === choice.value}
                    onclick={() => (heroEffort = choice.value)}
                    aria-pressed={heroEffort === choice.value}
                  >
                    {choice.label}
                  </button>
                {/each}
              </div>
              <p class="fs-pop-line">Deeper thinking finds more on hard changes; it costs more time.</p>
            </div>
          </details>
          <details class="fs-ctl-pop fs-strategy-pop">
            <summary class="fs-ctl">{strategyHasOverrides ? "Custom strategy" : "Auto strategy"}</summary>
            <div class="fs-pop fs-strategy-panel">
              <div class="fs-strategy-head">
                <div>
                  <span class="fs-label">Run strategy</span>
                  <p>Forge adapts the harness, planning, review, width, and budget to the task. Lock only the choices you care about.</p>
                </div>
                <span class:active={!strategyHasOverrides}>{strategyHasOverrides ? "Adjusted" : "Auto"}</span>
              </div>

              <div class="fs-strategy-grid">
                <label><span>Optimize for</span><select bind:value={heroOutcomePreference}><option value="balanced">Balanced</option><option value="faster">Faster</option><option value="confidence">More confidence</option></select></label>
                <label><span>Harness</span><select value={heroHarness} onchange={(event) => chooseHarness(event.currentTarget.value)}><option value="auto">Auto</option><option value="mdx_native">MDx Native</option><option value="codex_cli">Codex CLI</option><option value="grok_build">Grok Build</option></select></label>
                <label><span>Planning</span><select bind:value={heroPlannerMode}><option value="auto">Auto</option><option value="skip">Skip</option><option value="bounded">Bounded</option><option value="required">Required</option></select></label>
                <label><span>Independent review</span><select bind:value={heroReviewMode}><option value="auto">Auto</option><option value="deterministic_only">Checks only</option><option value="conditional_independent">When useful</option><option value="required_independent">Required</option></select></label>
              </div>
              <p class="fs-pop-line">{harnessLines[heroHarness]}</p>

              <div class="fs-budget-grid">
                <label><span>Cost cap ($)</span><input type="number" min="0" step="1" bind:value={heroMaxCostDollars} placeholder="Auto" /></label>
                <label><span>Time cap (min)</span><input type="number" min="0" step="5" bind:value={heroMaxRuntimeMinutes} placeholder="Auto" /></label>
                <label><span>Turn cap</span><input type="number" min="0" step="1" bind:value={heroMaxTurns} placeholder="Auto" /></label>
              </div>
              <p class="fs-pop-line">High-risk work keeps required planning and review floors even when a lower setting is requested. Every resolved choice is recorded before execution.</p>
            </div>
          </details>
          <button
            type="button"
            class="fs-ctl"
            class:active={heroPlanFirst}
            aria-pressed={heroPlanFirst}
            disabled={heroHarness === "codex_cli" || heroHarness === "grok_build"}
            onclick={() => (heroPlanFirst = !heroPlanFirst)}
            title={heroHarness === "codex_cli" || heroHarness === "grok_build" ? "Manual plan-first mode uses MDx Native. Adaptive planning still guides this external harness." : "Propose a plan and the machines to run it, and wait for your go before writing any code."}
          >
            {heroPlanFirst ? "Plan first: on" : "Plan first"}
          </button>
          <button type="button" class="fs-ctl fs-ctl-link" onclick={() => (repoConnectOpen = !repoConnectOpen)}>{repoConnectOpen ? "Close" : "+ Connect repo"}</button>
        </div>
        <div class="fs-bar-go">
          <button type="button" class="fs-direct" disabled={classifyBusy || !heroIntent.trim()} onclick={classifyAsk}>
            {classifyBusy ? "Reading the ask..." : "Plan it first"}
          </button>
          <button type="button" class="mdx-btn fs-go" class:primary={composerPrimary} disabled={heroBusy || !heroIntent.trim() || externalRunNeedsCheck || executionTarget.blocked} onclick={startHeroBuild}>
            {heroBusy ? busyLabel() : startLabel()}
          </button>
        </div>
      </div>
      {#if externalRunNeedsCheck && heroIntent.trim()}
        <p class="fs-runhint">{checkHint || "Name a check (under Checks) before starting - or use Plan it."}</p>
      {/if}
      {#if executionTarget.blocked && heroIntent.trim()}
        <p class="fs-runhint">{executionTarget.line}</p>
      {/if}
      {#if composerEngaged}
        <div style="margin-top:10px"><LeagueRecommendation rec={data.machineLeague} /></div>
      {/if}
      {#if repoConnectOpen}
        {@render connectForm("inline")}
      {/if}
      {#if heroFlash}<p class="flash bad">{heroFlash.line}</p>{/if}
    {#if classifyFlash}<p class="flash bad">{classifyFlash.line}</p>{/if}
    {#if missionResult}
      {@const r = missionResult}
      <div class="classify" data-shape="done">
        <p class="cl-head">Mission shaped.</p>
        <p class="cl-shape">Forge recorded a {r.milestone_count === 1 ? "one-checkpoint" : `${r.milestone_count}-checkpoint`}
        plan for {r.fleet_width} {r.fleet_width === 1 ? "worker" : "workers"} and a budget. Nothing runs until you open it and launch a milestone through a run or fleet - each one is governed and comes back for your review.</p>
        <dl class="cl-detail">
          <dt>Mission receipt</dt><dd>{r.mission_receipt_id}</dd>
          <dt>Policy decision</dt><dd>{r.policy_decision_id}</dd>
        </dl>
        <div class="cl-actions">
          <a class="mdx-btn primary" href="/forge/missions">Open mission &rarr;</a>
          <button type="button" class="cl-dismiss" onclick={() => (missionResult = null)}>Dismiss</button>
        </div>
      </div>
    {:else if missionForm}
      <div class="classify" data-shape="mission">
        <p class="cl-head">Shape the mission</p>
        <p class="cl-shape">Review what Forge proposed, edit anything, then confirm. Each check becomes a milestone.</p>
        <div class="mf-grid">
          <label class="mf-field mf-wide"><span>Goal</span><textarea rows="2" bind:value={missionForm.goal}></textarea></label>
          <label class="mf-field mf-wide"><span>Done when</span><input bind:value={missionForm.doneWhen} placeholder="e.g. the checks pass and no duplicates survive a replay" /></label>
          <label class="mf-field mf-wide"><span>Checks after each milestone (comma-separated)</span><input bind:value={missionForm.validationCommands} /></label>
          <label class="mf-field mf-wide"><span>Where it can write</span><input bind:value={missionForm.allowedWriteScope} /></label>
          <label class="mf-field mf-wide"><span>Off limits</span><input bind:value={missionForm.blockedPaths} placeholder="paths Forge must not touch" /></label>
          <label class="mf-field mf-wide"><span>Out of scope (optional)</span><input bind:value={missionForm.nonGoals} /></label>
          <div class="mf-field">
            <span>Execution width</span>
            <input type="number" min="1" max="512" bind:value={missionForm.fleetWidth} />
            <div class="mf-presets" role="group" aria-label="Mission execution width">
              {#each executionWidthOptions as option (option.value)}
                <button type="button" class:active={Number(missionForm.fleetWidth) === option.value} onclick={() => (missionForm.fleetWidth = option.value)}>{option.label}</button>
              {/each}
            </div>
          </div>
          <label class="mf-field"><span>Budget cap ($)</span><input type="number" min="1" bind:value={missionForm.maxCostDollars} /></label>
          <label class="mf-field"><span>Max time (hours)</span><input type="number" min="1" bind:value={missionForm.maxRuntimeHours} /></label>
          <label class="mf-field"><span>Check every (min)</span><input type="number" min="1" bind:value={missionForm.checkpointCadence} /></label>
        </div>
        {#if missionFlash}<p class="flash bad">{missionFlash.line}</p>{/if}
        <div class="cl-actions">
          <button type="button" class="mdx-btn primary" disabled={missionBusy} onclick={admitMission}>{missionBusy ? "Shaping..." : "Create this mission"}</button>
          <button type="button" class="cl-dismiss" onclick={cancelMissionForm}>Cancel</button>
        </div>
        <p class="cl-note">Admission records a mission packet and checkpoint plan. It does not start live workers,
        call providers, or grant execution authority.</p>
      </div>
    {:else if classification}
      {@const c = classification}
      {@const writeBoundary = classificationWriteBoundary(c)}
      <div class="reco" data-shape={c.recommended_shape} in:fly={{ y: 6, duration: 160 }}>
        <div class="reco-head">
          <span class="reco-eyebrow">How I'd approach this</span>
          {#if c.confidence_pct}<span class="reco-conf">{c.confidence_pct}% confident</span>{/if}
        </div>
        <p class="reco-label">{recommendationLabel(c)}</p>
        <p class="reco-why">{recommendationWhy(c)}</p>
        {#if c.rationale}<p class="reco-rationale">Reads as a {tierLabel(c.complexity_tier)} {taskClassLabel(c.task_class)} &middot; {c.rationale}</p>{/if}

        <div class="reco-facts">
          {#if c.strategy?.harness_id}<div class="reco-fact"><span>Harness</span><strong>{c.strategy.harness_id === "mdx_native" ? "MDx Native" : c.strategy.harness_id === "codex_cli" ? "Codex CLI" : c.strategy.harness_id === "grok_build" ? "Grok Build" : c.strategy.harness_id}</strong></div>{/if}
          {#if c.strategy?.planner_mode}<div class="reco-fact"><span>Planning</span><strong>{String(c.strategy.planner_mode).replace(/_/g, " ")}</strong></div>{/if}
          {#if c.strategy?.review_mode}<div class="reco-fact"><span>Review</span><strong>{String(c.strategy.review_mode).replace(/_/g, " ")}</strong></div>{/if}
          {#if c.strategy?.max_cost_cents}<div class="reco-fact"><span>Budget</span><strong>${(Number(c.strategy.max_cost_cents) / 100).toFixed(2)} and {Math.round(Number(c.strategy.max_runtime_ms) / 60_000)} min</strong></div>{/if}
          {#if c.suggested_checks}<div class="reco-fact"><span>Checks</span><strong>{c.suggested_checks}</strong></div>{/if}
          {#if writeBoundary}<div class="reco-fact"><span>Write boundary</span><strong>{writeBoundary}</strong></div>{/if}
          {#if c.blocked_paths}<div class="reco-fact"><span>Off limits</span><strong>{c.blocked_paths}</strong></div>{/if}
        </div>
        {#if c.strategy?.rationale}<p class="reco-rationale">{c.strategy.rationale}</p>{/if}

        {#if c.strategy_execution_readiness && c.strategy?.harness_id !== "mdx_native"}
          <div class="reco-note">
            <div>
              <strong>{c.strategy_execution_readiness.ready ? "Ready to run" : "One-time execution setup"}</strong>
              <p>{c.strategy_execution_readiness.reason}</p>
              {#if c.strategy_execution_readiness.cost_enforcement?.includes("usage_metering_unavailable")}
                <small>The approval, runtime, isolation, write scope, task, and parallel limits are enforced. Grok Build also enforces its turn cap. Codex CLI does not yet expose live turn metering, and neither adapter exposes a live dollar cutoff.</small>
              {/if}
            </div>
            {#if !c.strategy_execution_readiness.ready}
              <button type="button" class="mdx-btn" disabled={externalAuthorizationBusy} onclick={() => authorizeExternalStrategy(c)}>
                {externalAuthorizationBusy ? "Authorizing..." : "Authorize this bounded run"}
              </button>
            {/if}
          </div>
          {#if externalAuthorizationFlash}<p class:bad={!externalAuthorizationFlash.ok} class="flash">{externalAuthorizationFlash.line}</p>{/if}
        {/if}

        <div class="reco-actions">
          <button type="button" class="mdx-btn primary reco-go" disabled={heroBusy || c.strategy_execution_readiness?.ready === false} onclick={c.recommended_shape === "mission" ? openMissionForm : startFromClassification}>
            {heroBusy ? busyLabel() : c.strategy_execution_readiness?.ready === false ? "Setup required" : recommendationCta(c)}
          </button>
          <button type="button" class="reco-dismiss" onclick={dismissClassification}>Dismiss</button>
        </div>

        <details class="reco-adjust">
          <summary>Adjust</summary>
          <div class="reco-adjust-body">
            <span class="fs-label">How wide</span>
            <div class="fs-scale-options" role="group" aria-label="How wide to run">
              {#each executionWidthOptions as option (option.value)}
                <button
                  type="button"
                  class:active={executionWidth === option.value}
                  onclick={() => chooseExecutionWidth(option.value)}
                  aria-pressed={executionWidth === option.value}
                  title={option.line}
                >
                  {option.label}
                </button>
              {/each}
            </div>
            <p class="reco-width-line">{executionScaleLine()}</p>
            {#if c.recommended_shape === "mission"}
              <button type="button" class="reco-alt" onclick={startFromClassification}>Start as a run instead &rarr;</button>
            {:else}
              <button type="button" class="reco-alt" onclick={openMissionForm}>Make it a mission instead &rarr;</button>
            {/if}
          </div>
        </details>
        <p class="reco-note">A proposal - nothing runs and no authority is granted until you accept.</p>
      </div>
    {/if}
    {#if data.standards?.length > 0 && !isExternalRepo}
      <div class="fs-standards">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 2 4 5v6c0 5 3.4 8.6 8 11 4.6-2.4 8-6 8-11V5l-8-3Z"/><path d="m9 12 2 2 4-4"/></svg>
        <div class="fs-standards-body">
          <p class="fs-standards-head">Every build here is grounded in your standards</p>
          <p class="fs-standards-sub">Forge reads these from Memory and holds the work to them - your guidelines become the bar, automatically.</p>
          <div class="fs-standards-chips">
            {#each data.standards as standard (standard.documentId)}
              <a class="fs-standard" href="/pages" title="Open in Memory">{standard.title}</a>
            {/each}
          </div>
        </div>
      </div>
    {/if}
    {/if}
    {#if recentRuns.length > 0}
      <div class="fs-recent">
        <p class="fs-recent-head">Recent builds</p>
        <ul class="fs-recent-list">
          {#each recentRuns.slice(0, 5) as run (run.runId)}
            <li>
              <a class="fs-recent-row" href={`/forge/runs?run=${encodeURIComponent(run.runId)}`}>
                <span class="fs-recent-dot" data-status={run.status} class:spin={isRunning(run.status)} aria-hidden="true"></span>
                <span class="fs-recent-main">
                  <span class="fs-recent-top">
                    <span class="fs-recent-status">{statusLabel(run.status)}</span>
                    {#if run.outcomeDisposition}<span class="fs-recent-outcome" title={run.lessonCandidate || "Recorded for the next build"}>{run.outcomeDisposition}</span>{/if}
                    {#if run.branch}<span class="fs-recent-branch" title={run.branch}>{run.branch}</span>{/if}
                  </span>
                  {#if hasHumanTitle(run)}<span class="fs-recent-title">{runHeadline(run)}</span>{/if}
                  <span class="fs-recent-summary">{summaryLine(run)}</span>
                </span>
              </a>
              {#if !isRunning(run.status)}<button type="button" class="fs-pack-draft" onclick={() => startPackProposal(run)}>Make a team pack</button>{/if}
            </li>
          {/each}
        </ul>
        <a class="fs-allruns" href="/forge/runs">All trails &rarr;</a>
        {#if packSourceRun}
          <form class="pack-draft" onsubmit={(event) => { event.preventDefault(); proposeTeamPack(); }}>
            <div><span>Proven run to team pack</span><strong>{packSourceRun.runId}</strong></div>
            <label>Name<input bind:value={packDraftName} /></label>
            <label>Job<input bind:value={packDraftJob} /></label>
            <label>Reusable shape<textarea rows="2" bind:value={packDraftItems}></textarea></label>
            <p>Marketplace will hold this as an editable team-source draft until a person reviews publication.</p>
            <div><button type="button" onclick={() => (packSourceRun = null)}>Cancel</button><button class="mdx-btn primary" type="submit">Propose pack</button></div>
          </form>
        {/if}
        {#if packProposalFlash}<p class:bad={!packProposalFlash.ok} class="pack-proposal-flash">{packProposalFlash.line}</p>{/if}
      </div>
    {/if}
  </section>
  {/if}

  {@render forgeDashboard()}

  {#if data.kernelReachable && flywheel.ready && hasRuns}
    <details class="forge-doctrine flywheel-disclosure">
      <summary>What finished builds teach Forge</summary>
    <section class="flywheel" aria-label="What builds leave behind">
      <div class="fw-head">
        <h2 class="fw-title">Every build makes the next one smarter</h2>
        <p class="fw-sub">{localRealLine}</p>
      </div>

      {#if flywheel.outcomes + flywheel.lessons + flywheel.capabilities > 0}
        <ul class="fw-counts">
          <li>
            <span class="fw-n">{flywheel.outcomes}</span>
            <span class="fw-l">{flywheel.outcomes === 1 ? "build recorded" : "builds recorded"}</span>
          </li>
          <li>
            <span class="fw-n">{flywheel.lessons}</span>
            <span class="fw-l">{flywheel.lessons === 1 ? "lesson proposed" : "lessons proposed"}</span>
          </li>
          <li>
            <span class="fw-n">{flywheel.capabilities}</span>
            <span class="fw-l">{flywheel.capabilities === 1 ? "capability ready" : "capabilities ready"}</span>
          </li>
        </ul>
      {:else}
        <p class="fw-empty">Your first finished build starts this: it records what happened, proposes a lesson, and the next plan can build on it.</p>
      {/if}

      <div class="fw-chips" role="status">
        {#if flywheel.activeMemory > 0}
          <span class="fw-chip">{flywheel.activeMemory} {flywheel.activeMemory === 1 ? "lesson" : "lessons"} now guiding new plans</span>
        {/if}
        <span class="fw-chip held">Nothing ships or deploys without you</span>
      </div>

      {#if dedupedLessons.length > 0}
        <div class="fw-lessons">
          <p class="fw-lessons-head">What the last builds suggest</p>
          <ul>
            {#each dedupedLessons.slice(0, 3) as lesson (lesson.id)}
              <li class="fw-lesson">
                <span class="fw-lesson-text">{lesson.text}</span>
                {#if lesson.count > 1}<span class="fw-lesson-count">{lesson.count} builds</span>{/if}
                <a class="fw-lesson-link" href="/learn">Decide in Learn &rarr;</a>
              </li>
            {/each}
          </ul>
          <p class="fw-lessons-note">A suggestion only. It guides a plan once you promote it - never on its own.</p>
        </div>
      {/if}

      <details class="fw-proof">
        <summary>How real is this, exactly</summary>
        <ul class="fw-proof-list">
          <li><span>The build loop</span><span>An isolated copy of your repo. Your live files are never written.</span></li>
          <li><span>Model connected</span><span>{flywheel.modelConfigured ? "Yes" : "Not yet"}</span></li>
          <li><span>Runs your checks</span><span>{flywheel.sandboxTests || flywheel.hostChecks ? "Yes, in the sandbox copy" : "Held until you turn it on"}</span></li>
          <li><span>Applies the change</span><span>{flywheel.patchApply ? "Yes, in the sandbox copy" : "Held until you turn it on"}</span></li>
          <li><span>Lessons change behavior on their own</span><span>No - a human promotes each one first</span></li>
          <li><span>Ships or deploys on its own</span><span>No - that stays your decision</span></li>
        </ul>
        {#if flywheel.betaGaps.length > 0}
          <div class="fw-gaps">
            <p class="fw-gaps-head">Limits we're honest about</p>
            <ul>
              {#each flywheel.betaGaps as gap (gap)}<li>{betaGapLine(gap)}</li>{/each}
            </ul>
          </div>
        {/if}
      </details>
    </section>
    </details>
  {/if}

  <details class="forge-doctrine">
    <summary>Authority and operating details</summary>
  <div class="forge-orient">
    <nav class="work-graph-slim" aria-label="How work moves through Forge">
      <ol class="wgs-track">
        {#each WORK_GRAPH as stage (stage.key)}
          <li class="wgs-step" class:wgs-mine={stage.whose === "you"}>
            <span class="wgs-dot" aria-hidden="true"></span>
            <span class="wgs-label">{stage.label}</span>
          </li>
        {/each}
      </ol>
    </nav>
    <p class="status-line" role="status">
      {#if !data.kernelReachable}
        Resting - connect MDx to wake it
      {:else}
        {[
          decisionsWaiting === 0
            ? "No decisions waiting"
            : `${decisionsWaiting} ${decisionsWaiting === 1 ? "decision" : "decisions"} waiting`,
          "Nothing runs without your sign-off"
        ].join(" · ")}
      {/if}
    </p>
  </div>

  {#if !showSetup}
  <section class="envelopes" aria-label="Let Forge finish small things on its own">
    <details class="section-fold" open={hasActiveEnvelope}>
      <summary class="section-fold-head">
        <span class="builds-head">Let Forge finish small things on its own</span>
        <span class="section-fold-state"
          >{hasActiveEnvelope
            ? "on"
            : "off - every change comes back to you"}</span
        >
      </summary>
      <div class="env-head">
        <p class="env-cue">
          Give the go-ahead once for a kind of small, low-stakes change, and Forge can finish
          that kind on its own; everything else still comes back to you to review.
        </p>
      </div>

    <div class="env-fit" aria-label="Is this task covered">
      <p class="env-sub-head">Is your current task covered?</p>
      <p class="env-coverage env-coverage-{envelopeFitForTask.coverage.tone}">
        <span class="env-coverage-dot" aria-hidden="true"></span>
        <span class="env-coverage-label">{envelopeFitForTask.coverage.label}</span>
      </p>
      <p class="env-fit-verdict">{envelopeFitForTask.coverage.line}</p>
      <p class="env-fit-lead">What Forge would need before it could finish this kind of task on its own:</p>
      <ul class="env-fit-list">
        {#each envelopeFitForTask.requirements as need, index (index)}
          <li class="env-fit-row env-fit-{need.state}">
            <span class="env-fit-mark" aria-hidden="true">{need.state === "set" ? "ok" : "?"}</span>
            <span class="env-fit-text">{need.text}</span>
          </li>
        {/each}
      </ul>
    </div>

    <div class="env-active" aria-label="Active standing authorizations">
      <p class="env-sub-head">Active now</p>
      {#if activeEnvelopes.length === 0}
        <p class="env-empty">
          {data.envelopeProjection?.human_summary ||
            "No standing authorization is active - every task reviews with you until an owner records one. Nothing is completing on prior say-so today."}
        </p>
      {:else}
        <ul class="env-list">
          {#each activeEnvelopes as env (env.envelope_id ?? env.id)}
            {@const facts = envelopeFacts(env)}
            <li class="env-row">
              <div class="env-row-top">
                <span class="env-row-owner">{facts.owner || "Owner not stated"}</span>
                {#if facts.active}
                  <span class="env-row-state">active</span>
                {:else}
                  <span class="env-row-state env-row-state-off">inactive</span>
                {/if}
              </div>
              <dl class="env-facts">
                <div><dt>Scope</dt><dd>{facts.scopeText || "not stated"}</dd></div>
                <div><dt>What it can finish</dt><dd>{facts.classes || "not stated"}</dd></div>
                <div><dt>How far it can go</dt><dd>{facts.ceiling || "not stated"}</dd></div>
                <div><dt>Tests and evals</dt><dd>{facts.evals || "not stated"}</dd></div>
                <div><dt>Rollback</dt><dd>{facts.rollback || "not stated"}</dd></div>
                <div><dt>Expiry</dt><dd>{facts.expiry || "not stated"}</dd></div>
              </dl>
              {#if facts.triggers}
                <p class="env-row-triggers">Escalates on: {facts.triggers}</p>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <div class="env-model" aria-label="What a standing authorization covers">
      <p class="env-model-head">What an authorization covers</p>
      <div class="env-grid">
        {#each ENVELOPE_FIELDS as field (field.name)}
          <div class="env-field">
            <span class="env-field-name">{field.name}</span>
            <span class="env-field-meaning">{field.meaning}</span>
          </div>
        {/each}
      </div>
      <p class="env-model-note">
        {hasActiveAuthorization
          ? "These are the fields each active authorization above is read from."
          : "No authorization is live, so read this as the fields an owner would set when they record one."}
      </p>
    </div>

    <div class="env-escalate" aria-label="What would cause escalation">
      <p class="env-sub-head">What would hand the call back to you</p>
      <p class="env-escalate-cue">
        Even when Forge is set to finish this kind of change on its own, any of these stops the work and brings it to you. This
        is the guardrail that makes a standing authorization safe to grant.
      </p>
      <ul class="env-escalate-list">
        {#each ESCALATION_TRIGGERS as trigger (trigger.name)}
          <li class="env-escalate-row">
            <span class="env-escalate-name">{trigger.name}</span>
            <span class="env-escalate-when">{trigger.when}</span>
          </li>
        {/each}
      </ul>
    </div>

    <div class="env-govern" aria-label="Recording and pulling an authorization">
      <p class="env-sub-head">Recording or pulling one</p>
      <p class="env-govern-line">
        Recording or pulling a standing authorization is the owner putting their name on a policy,
        or taking it back. It is kept as proof. The write that records it lands with the
        autonomy-policy rail; it is not wired here yet, so these are descriptions, not buttons.
      </p>
      <ul class="env-govern-acts">
        <li class="env-act-row">
          <span class="env-act-label">Record an authorization</span>
          <span class="env-act-line">An owner sets the scope, work classes, risk ceiling, checks, rollback, and expiry. It lands as a recorded decision when the autonomy-policy rail connects.</span>
        </li>
        <li class="env-act-row">
          <span class="env-act-label">Pull one</span>
          <span class="env-act-line">An owner takes the authorization back. Work under it goes back to reviewing with you, and the change is kept as proof the same way.</span>
        </li>
      </ul>
      <p class="env-boundary"><strong>Boundary</strong> No standing authorization can grant a ship or a production write. Completing, shipping, and deploying stay a human decision, every time, whatever the policy says.</p>
    </div>
    </details>
  </section>
  {/if}

  </details>

  <footer class="quiet-line">
    Nothing builds without your say.
    <details class="quiet-more">
      <summary>more</summary>
      <span>
        Boundary: no worker spawn, no shell, no patches, no live provider calls, no CI claims, no
        deployment - each opens one at a time, with its own evidence, through Talent and the engine
        rails. Safe next: ask for a build, or clear what's waiting on you. Every step on this page
        is a recorded kernel write that stops exactly where its authority ends.
      </span>
    </details>
  </footer>

  {#if paletteOpen}
    <div
      class="palette-scrim"
      onclick={closePalette}
      onkeydown={onPaletteKeydown}
      role="presentation"
    >
      <div
        class="palette"
        role="dialog"
        aria-label="Quick actions"
        aria-modal="true"
        tabindex="-1"
        onclick={(event) => event.stopPropagation()}
        onkeydown={onPaletteKeydown}
      >
        <input
          class="palette-input"
          type="text"
          bind:this={paletteInputEl}
          bind:value={paletteQuery}
          placeholder="Type a command - new build, review, evidence, copy reference"
          aria-label="Filter commands"
          autocomplete="off"
        />
        <ul class="palette-list" bind:this={paletteListEl} role="listbox" aria-label="Commands">
          {#each filteredCommands as command, index (command.id)}
            <li>
              <button
                type="button"
                class="palette-item"
                class:highlight={index === paletteIndex}
                role="option"
                aria-selected={index === paletteIndex}
                onmousemove={() => (paletteIndex = index)}
                onclick={() => runCommand(command)}
              >
                <span class="palette-item-label">{command.label}</span>
                <span class="palette-item-hint">{command.hint}</span>
              </button>
            </li>
          {/each}
          {#if filteredCommands.length === 0}
            <li class="palette-empty">No command matches that. Try a different word, or Esc to close.</li>
          {/if}
        </ul>
        <p class="palette-foot">
          Quick actions only move you around. Governed decisions stay on their own buttons.
        </p>
      </div>
    </div>
  {/if}
</section>

<style>
  .forge {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    justify-items: stretch;
    gap: 24px;
    align-content: start;
    max-width: 1020px;
    margin: 0 auto;
    width: 100%;
    min-width: 0;
    padding: 4vh 0 60px;
    /* overflow-wrap is inherited, so one declaration here keeps every id,
       scope, reason, and receipt in every card, chip, list, and fact from
       pushing past its column. min-width:0 lets grid/flex children shrink. */
    overflow-wrap: anywhere;
    overflow-x: clip;
    box-sizing: border-box;
    contain: inline-size;
  }
  .forge > * {
    min-width: 0;
    max-width: 100%;
  }
  .forge *,
  .forge *::before,
  .forge *::after {
    box-sizing: border-box;
  }

  .forge-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    min-width: 0;
    max-width: 100%;
  }

  .next-action {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 14px;
    align-items: center;
    padding: 16px 18px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg);
    background: color-mix(in srgb, var(--mdx-surface-raised) 88%, var(--mdx-accent-primary) 12%);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .next-action[data-kind="start"] {
    background: var(--mdx-surface-raised);
    border-color: var(--mdx-border-subtle);
  }

  .na-mark {
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--mdx-accent-primary);
    color: var(--mdx-text-on-accent);
    font-weight: 700;
  }

  .na-pulse {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: currentColor;
    box-shadow: 0 0 0 6px color-mix(in srgb, currentColor 18%, transparent);
  }

  .na-copy {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .na-eyebrow {
    color: var(--mdx-accent-primary);
    font-size: var(--mdx-text-xs);
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .na-copy strong {
    color: var(--mdx-text-primary);
    font-size: var(--mdx-text-lg);
    line-height: 1.25;
  }

  .na-copy p {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-sm);
    line-height: 1.45;
  }

  .na-queue {
    width: fit-content;
    margin-top: 3px;
    color: var(--mdx-text-secondary);
    font-size: var(--mdx-text-xs);
    font-weight: 650;
    text-decoration: none;
  }

  .na-boundary {
    margin-top: 2px;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .na-queue:hover {
    color: var(--mdx-accent-primary);
    text-decoration: underline;
  }

  .na-cta {
    white-space: nowrap;
  }

  @media (max-width: 680px) {
    .next-action {
      grid-template-columns: auto minmax(0, 1fr);
    }

    .na-cta {
      grid-column: 2;
      justify-self: start;
    }
  }

  .signal {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .signal-chip {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    padding: 4px 12px;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-tertiary);
    background: transparent;
  }

  .signal-chip.attention {
    border-color: var(--mdx-accent-primary);
    color: var(--mdx-accent-primary);
    font-weight: 600;
  }

  .signal-chip.held,
  .signal-chip.down {
    color: var(--mdx-text-muted);
  }

  .first-read {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 12px;
    margin-bottom: 20px;
  }

  .fr-card {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    padding: 14px 16px;
    display: grid;
    gap: 6px;
    align-content: start;
    min-width: 0;
    /* A predictable floor so the first read does not jump a line as the live
       signal lands - the band holds its height while the words settle in. */
    min-height: 96px;
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .fr-card.fr-wide {
    grid-column: 1 / -1;
  }

  .fr-card.fr-attention {
    border-color: var(--mdx-accent-primary);
  }

  .fr-label {
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--mdx-text-muted);
    margin: 0;
  }

  .fr-lead {
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-primary);
    line-height: 1.45;
    margin: 0;
  }

  .fr-evidence {
    margin-top: 4px;
  }

  /* Empty state: the work graph as a quiet inline strip - borderless, so it
     orients without competing with the composer. A single recessed status line
     sits beneath it, replacing the detached top-right chips. */
  .forge-orient {
    display: grid;
    gap: 6px;
    margin-bottom: 22px;
  }

  .work-graph-slim {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 10px;
  }

  .wgs-track {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px 18px;
  }

  .wgs-step {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }

  /* A small stage dot gives the path quiet rhythm without the mechanical feel
     of arrows. The accent dot marks the one stage that stays yours. */
  .wgs-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--mdx-border-default);
    flex: 0 0 auto;
  }

  .wgs-step.wgs-mine .wgs-dot {
    background: var(--mdx-accent-primary);
  }

  .wgs-label {
    font-size: var(--mdx-text-xs);
    letter-spacing: 0.02em;
    color: var(--mdx-text-secondary);
  }

  .wgs-step.wgs-mine .wgs-label {
    color: var(--mdx-text-primary);
    font-weight: 600;
  }

  .status-line {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    line-height: 1.5;
  }

  .builds-aside {
    margin: 6px 0 0;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
  }

  .act-note {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--mdx-accent-success);
    font-size: var(--mdx-text-xs);
  }

  .act-note.bad {
    color: var(--mdx-accent-warning);
  }

  .workspace {
    display: grid;
    gap: 16px;
    padding: 18px 20px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .ws-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .ws-title {
    min-width: 0;
  }

  .ws-intent {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 17px;
    font-weight: 650;
    line-height: 1.35;
    min-width: 0;
  }

  .ws-meta {
    margin: 6px 0 0;
    display: flex;
    gap: 8px;
    align-items: baseline;
  }

  .ws-source {
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .ws-close {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: none;
    color: var(--mdx-text-tertiary);
    padding: 6px 12px;
    cursor: pointer;
    flex: none;
  }

  .stage-chip {
    border-radius: var(--mdx-radius-pill);
    padding: 2px 10px;
    font-size: var(--mdx-text-xs);
    font-weight: 600;
    border: 1px solid var(--mdx-border-subtle);
    color: var(--mdx-text-secondary);
  }

  .stage-path {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 10px;
  }

  .path-step {
    display: grid;
    gap: 3px;
    justify-items: start;
    opacity: 0.55;
  }

  .path-step.done {
    opacity: 1;
  }

  .path-dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 2px solid var(--mdx-border-default);
    background: var(--mdx-surface-base);
  }

  .path-label {
    font-size: var(--mdx-text-xs);
    font-weight: 650;
    color: var(--mdx-text-primary);
  }

  .path-line {
    font-size: 11px;
    color: var(--mdx-text-muted);
    line-height: 1.4;
  }

  .run-strip {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }

  .run-state {
    font-size: 13px;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }

  .pattern-chip {
    border: 1px dashed var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    padding: 2px 10px;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
  }

  .run-controls {
    display: flex;
    gap: 6px;
    margin-left: auto;
  }

  .run-act {
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    padding: 5px 14px;
    font-size: var(--mdx-text-xs);
    font-weight: 600;
    cursor: pointer;
  }

  .run-act.stop {
    color: var(--mdx-accent-danger, #d4183d);
    border-color: color-mix(in srgb, var(--mdx-accent-danger, #d4183d) 40%, transparent);
  }

  .run-act:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .control-note {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--mdx-accent-success);
    font-size: 13px;
  }

  .control-note.bad {
    color: var(--mdx-accent-warning);
  }

  .replay-line {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 11px;
  }

  .run-budget {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
    line-height: 1.5;
  }

  .ws-drawer {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-tertiary);
  }

  .drawer-body {
    display: grid;
    gap: 4px;
    padding-top: 6px;
  }

  .drawer-line {
    margin: 0;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-secondary);
  }

  .quiet-drawer {
    color: var(--mdx-text-muted);
    line-height: 1.5;
  }

  .story {
    display: grid;
    gap: 10px;
    border-left: 2px solid var(--mdx-border-subtle);
    padding-left: 14px;
  }

  .story-step {
    display: grid;
    gap: 2px;
  }

  .story-head {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--mdx-text-primary);
  }

  .story-line {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.5;
  }

  .ws-key {
    margin-left: 6px;
    color: var(--mdx-accent-primary);
    font-weight: 600;
    text-decoration: none;
  }

  .ws-key:hover {
    text-decoration: underline;
  }

  .ws-next {
    margin: 0;
    color: var(--mdx-text-secondary);
    font-size: 13px;
  }

  .stack-strip {
    display: grid;
    gap: 6px;
    padding: 10px 12px;
    border: 1px dashed var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
  }

  .stack-line {
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
    line-height: 1.5;
  }

  .stack-chips {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .stack-chip {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    padding: 2px 11px;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-primary);
    text-decoration: none;
    background: var(--mdx-surface-raised);
    max-width: 100%;
  }

  .stack-chip:hover {
    border-color: var(--mdx-accent-primary);
  }

  .stack-chip.in {
    border-color: color-mix(in srgb, var(--mdx-accent-success) 50%, transparent);
    color: var(--mdx-accent-success);
  }

  .stack-chip.pack {
    color: var(--mdx-accent-primary);
  }

  .ws-stack {
    margin: 0;
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
    font-size: 13px;
  }

  .stack-note {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
  }

  .ws-context {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .review {
    display: grid;
    gap: var(--mdx-space-5, 20px);
    padding: var(--mdx-space-5, 20px) var(--mdx-space-5, 20px);
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-base, transparent);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .review-top {
    display: grid;
    gap: 6px;
  }

  .review-head {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 16px;
    font-weight: 650;
    color: var(--mdx-text-primary);
  }

  .review-sub {
    margin: 0;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
    line-height: 1.45;
  }

  /* The six parts share one calm rhythm: a quiet uppercase label, then the
     answer in plain words. Whitespace, not boxes, carries the hierarchy. */
  .review-part {
    display: grid;
    gap: 6px;
    padding: 0 0 0 12px;
    border-left: 2px solid var(--mdx-border-subtle);
  }

  .review-part-good {
    border-left-color: color-mix(in srgb, var(--mdx-accent-success) 50%, transparent);
  }

  .review-part-risk {
    border-left-color: color-mix(in srgb, var(--mdx-accent-warning) 50%, transparent);
  }

  .review-part-gap {
    border-left-color: var(--mdx-border-default);
  }

  .review-part-head {
    margin: 0;
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--mdx-text-tertiary);
    font-weight: 600;
  }

  .review-part-line {
    margin: 0;
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-primary);
    line-height: 1.55;
  }

  .review-part-note {
    margin: 0;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
    line-height: 1.45;
  }

  .review-part-empty {
    margin: 0;
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-muted);
    line-height: 1.5;
  }

  .review-part-list {
    margin: 0;
    padding-left: 18px;
    display: grid;
    gap: 5px;
  }

  .review-decide {
    display: grid;
    gap: 10px;
    padding: 14px 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
  }

  .rd-safe {
    margin: 0;
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-primary);
    line-height: 1.55;
    font-weight: 550;
  }

  .rd-live {
    display: grid;
    gap: 8px;
  }

  .rd-action {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  .rd-go {
    border: 1px solid var(--mdx-accent-primary);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-accent-primary);
    color: #fff;
    padding: 6px 16px;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    flex: none;
  }

  .rd-go:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .rd-line {
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
    line-height: 1.45;
    min-width: 0;
  }

  .rd-edge-note {
    margin: 0;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
    line-height: 1.5;
  }

  .rd-gate {
    margin: 0;
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-secondary);
    line-height: 1.5;
    padding: 8px 10px;
    border: 1px dashed var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    background: var(--mdx-surface-base);
  }

  .rd-ship {
    display: grid;
    gap: 8px;
  }

  .rd-reject {
    border-color: var(--mdx-border-default);
    background: transparent;
    color: var(--mdx-text-primary);
  }

  .rd-result {
    display: grid;
    gap: 6px;
    padding: 10px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    background: var(--mdx-surface-base);
  }

  .rd-result.bad {
    border-color: var(--mdx-accent-warn, var(--mdx-border-default));
  }

  .rd-result-line {
    margin: 0;
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-primary);
    line-height: 1.5;
  }

  .rd-result-next {
    margin: 0;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-secondary);
    line-height: 1.5;
  }

  .review-packet-line {
    margin: 0;
    font-size: var(--mdx-text-sm);
    color: var(--mdx-text-primary);
    line-height: 1.5;
  }

  .builds,
  .readiness {
    display: grid;
    gap: 10px;
  }

  .envelopes {
    display: grid;
    gap: 16px;
    padding: 18px 20px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .env-head {
    display: grid;
    gap: 4px;
  }

  .env-cue {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 13px;
    line-height: 1.55;
    max-width: 70ch;
  }

  .env-model {
    display: grid;
    gap: 10px;
    padding: 14px 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base, transparent);
  }

  .env-model-head,
  .env-sub-head {
    margin: 0;
    font-size: 13px;
    font-weight: 650;
    color: var(--mdx-text-primary);
  }

  .env-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 10px;
  }

  .env-field {
    display: grid;
    gap: 3px;
    align-content: start;
    padding: 10px 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    background: var(--mdx-surface-raised);
  }

  .env-field-name {
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--mdx-text-muted);
  }

  .env-field-meaning {
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
    line-height: 1.45;
  }

  .env-model-note {
    margin: 0;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
    line-height: 1.5;
  }

  .env-active,
  .env-fit,
  .env-escalate,
  .env-govern {
    display: grid;
    gap: 8px;
  }

  .env-empty {
    margin: 0;
    padding: 12px 14px;
    border: 1px dashed var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    color: var(--mdx-text-secondary);
    font-size: 13px;
    line-height: 1.5;
  }

  .env-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }

  .env-row {
    display: grid;
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    background: var(--mdx-surface-raised);
  }

  .env-row-top {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }

  .env-row-owner {
    font-size: 13px;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }

  .env-row-state {
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--mdx-accent-positive, var(--mdx-text-secondary));
  }

  .env-row-state-off {
    color: var(--mdx-text-muted);
  }

  .env-facts {
    margin: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 8px 16px;
  }

  .env-facts div {
    display: grid;
    gap: 2px;
  }

  .env-facts dt {
    font-size: var(--mdx-text-xs);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--mdx-text-muted);
  }

  .env-facts dd {
    margin: 0;
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
    line-height: 1.4;
  }

  .env-row-triggers {
    margin: 0;
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
    line-height: 1.45;
  }

  .env-coverage {
    margin: 0;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 5px 12px;
    border-radius: var(--mdx-radius-pill, 999px);
    border: 1px solid var(--mdx-border-default);
    width: fit-content;
  }

  .env-coverage-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--mdx-text-muted);
    flex: none;
  }

  .env-coverage-label {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }

  .env-coverage-review {
    border-color: color-mix(in srgb, var(--mdx-accent-warning) 32%, transparent);
    background: color-mix(in srgb, var(--mdx-accent-warning) 5%, transparent);
  }

  .env-coverage-review .env-coverage-dot {
    background: var(--mdx-accent-warning);
  }

  .env-coverage-check {
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 32%, transparent);
    background: color-mix(in srgb, var(--mdx-accent-primary) 5%, transparent);
  }

  .env-coverage-check .env-coverage-dot {
    background: var(--mdx-accent-primary);
  }

  .env-fit-verdict {
    margin: 0;
    font-size: 13px;
    color: var(--mdx-text-primary);
    line-height: 1.5;
  }

  .env-fit-lead {
    margin: 2px 0 0;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--mdx-text-secondary);
  }

  .env-fit-list,
  .env-escalate-list,
  .env-govern-acts {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
  }

  .env-fit-row {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
    line-height: 1.45;
  }

  .env-fit-mark {
    flex: none;
    width: 20px;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    text-align: center;
    padding: 1px 0;
    border-radius: var(--mdx-radius-sm);
    color: var(--mdx-text-muted);
    border: 1px solid var(--mdx-border-subtle);
  }

  .env-fit-set .env-fit-mark {
    color: var(--mdx-accent-positive, var(--mdx-accent-primary));
    border-color: color-mix(in srgb, var(--mdx-accent-positive, var(--mdx-accent-primary)) 40%, transparent);
  }

  .env-escalate-cue {
    margin: 0;
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
    line-height: 1.5;
  }

  .env-escalate-row {
    display: flex;
    align-items: baseline;
    gap: 12px;
    flex-wrap: wrap;
    padding: 8px 10px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
  }

  .env-escalate-name {
    font-size: 12.5px;
    font-weight: 650;
    color: var(--mdx-text-primary);
    flex: none;
    min-width: 130px;
  }

  .env-escalate-when {
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
    line-height: 1.45;
    flex: 1;
    min-width: 200px;
  }

  .env-govern-line {
    margin: 0;
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
    line-height: 1.5;
  }

  .env-act-row {
    display: flex;
    align-items: baseline;
    gap: 12px;
    flex-wrap: wrap;
    padding: 8px 10px;
    border: 1px dashed var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
  }

  .env-act-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--mdx-text-tertiary);
    flex: none;
  }

  .env-act-line {
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
    line-height: 1.45;
  }

  .env-boundary {
    margin: 0;
    padding: 10px 12px;
    border: 1px solid color-mix(in srgb, var(--mdx-accent-warning) 32%, transparent);
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-warning) 4%, transparent);
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
    line-height: 1.5;
  }

  .env-boundary strong {
    color: var(--mdx-text-primary);
    margin-right: 6px;
  }

  /* System readiness is background, not user work - so it sits as a quiet
     collapsed row like the other folds, no accent calling for attention. */
  .liveloop {
    display: grid;
    gap: 12px;
    padding: 18px 20px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .liveloop-head {
    display: grid;
    gap: 4px;
  }

  .liveloop-cue {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 13px;
    line-height: 1.5;
    max-width: 64ch;
  }

  .loop-posture {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .posture-chip {
    padding: 4px 11px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-secondary);
    background: var(--mdx-surface-base, transparent);
  }

  .thread {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 0;
  }

  .thread-turn {
    display: grid;
    grid-template-columns: 18px 1fr;
    gap: 12px;
  }

  .thread-rail {
    display: grid;
    justify-items: center;
    position: relative;
  }

  .thread-rail::before {
    content: "";
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--mdx-border-subtle);
  }

  .thread-dot {
    position: relative;
    z-index: 1;
    margin-top: 4px;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    border: 2px solid var(--mdx-border-default);
    background: var(--mdx-surface-raised);
  }

  .thread-body {
    display: grid;
    gap: 2px;
    padding-bottom: 14px;
    min-width: 0;
  }

  .thread-top {
    margin: 0;
    display: flex;
    align-items: baseline;
    gap: 9px;
    flex-wrap: wrap;
  }

  .thread-speaker {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    color: var(--mdx-text-tertiary);
  }

  .thread-head {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }

  .thread-line {
    margin: 0;
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
    line-height: 1.45;
  }


  .timeline-drawer {
    margin-top: 2px;
  }

  .builds-head {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 15px;
    font-weight: 650;
  }

  /* A collapsible section: the summary reads as a calm header with a state hint,
     and the depth opens only when the operator wants it. */
  .section-fold {
    display: block;
  }

  .section-fold > .section-fold-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
    flex-wrap: wrap;
    cursor: pointer;
    list-style: none;
    padding: 2px 0;
  }

  .section-fold > .section-fold-head::-webkit-details-marker {
    display: none;
  }

  .section-fold > .section-fold-head::before {
    content: "›";
    color: var(--mdx-text-muted);
    font-size: 14px;
    transition: transform 0.15s ease;
  }

  .section-fold[open] > .section-fold-head::before {
    transform: rotate(90deg);
  }

  .section-fold-state {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
  }

  .builds-empty,
  .thread-empty {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 13.5px;
    line-height: 1.5;
  }

  /* Quick-actions affordance in the header. */
  .head-aside {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: flex-end;
    min-width: 0;
    max-width: 100%;
  }

  .head-aside :global(.mdx-segmented) {
    max-width: 100%;
    overflow-x: auto;
  }

  .cmd-open {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-control);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-secondary);
    padding: 6px 9px;
    font: inherit;
    cursor: pointer;
  }

  .cmd-open:hover {
    border-color: var(--mdx-accent-primary);
    color: var(--mdx-text-primary);
  }

  .cmd-kbd {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    padding: 0 5px;
    font-size: 10px;
    color: var(--mdx-text-muted);
    font-family: var(--mdx-font-mono, monospace);
  }

  .cmd-note {
    margin: 0;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-accent-success);
    max-width: 220px;
    text-align: right;
    line-height: 1.4;
  }

  /* Saved views: a calm row of filters over the dense list. */
  .views {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .view-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    background: none;
    color: var(--mdx-text-tertiary);
    padding: 4px 12px;
    font: inherit;
    font-size: var(--mdx-text-xs);
    cursor: pointer;
  }

  .view-chip:hover {
    border-color: var(--mdx-border-default);
    color: var(--mdx-text-secondary);
  }

  .view-chip.active {
    border-color: var(--mdx-accent-primary);
    color: var(--mdx-text-primary);
    background: color-mix(in srgb, var(--mdx-accent-primary) 8%, transparent);
  }

  .view-count {
    font-size: 10px;
    color: var(--mdx-text-muted);
    font-variant-numeric: tabular-nums;
  }

  /* The dense build list: one scannable row per build. */
  .build-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }

  .build-row {
    display: grid;
    grid-template-columns: minmax(150px, 0.9fr) minmax(0, 1.6fr) minmax(0, 1.1fr) minmax(120px, 0.8fr);
    align-items: center;
    gap: 12px;
    width: 100%;
    text-align: left;
    padding: 10px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    color: inherit;
    font: inherit;
    cursor: pointer;
    transition: border-color 140ms ease-out;
  }

  .build-row:hover,
  .build-row.open {
    border-color: var(--mdx-accent-primary);
  }

  .build-row.row-waiting {
    border-left: 2px solid var(--mdx-accent-primary);
  }

  @media (prefers-reduced-motion: reduce) {
    .build-row { transition: none; }
  }

  .row-status {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: var(--mdx-text-xs);
    font-weight: 600;
    color: var(--mdx-text-secondary);
    min-width: 0;
  }

  .row-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--mdx-border-default);
    flex: 0 0 auto;
  }

  .row-intent {
    font-size: 13.5px;
    color: var(--mdx-text-primary);
    font-weight: 550;
    line-height: 1.4;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .row-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    min-width: 0;
  }

  .row-tag {
    font-size: 11px;
    color: var(--mdx-text-muted);
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    padding: 1px 8px;
    white-space: nowrap;
  }

  .row-tag.stage {
    color: var(--mdx-text-secondary);
  }

  .row-next {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-tertiary);
    text-align: right;
    line-height: 1.35;
  }

  .row-next.next-yours {
    color: var(--mdx-accent-primary);
    font-weight: 600;
  }

  @media (max-width: 720px) {
    .build-row {
      grid-template-columns: 1fr;
      gap: 6px;
      align-items: start;
    }

    .row-intent {
      white-space: normal;
    }

    .row-next {
      text-align: left;
    }
  }

  /* The command palette: a labeled dialog over a scrim. */
  .palette-scrim {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 12vh 16px 16px;
    background: color-mix(in srgb, var(--mdx-surface-base) 70%, transparent);
    backdrop-filter: blur(2px);
  }

  .palette {
    width: 100%;
    max-width: 520px;
    display: grid;
    gap: 8px;
    padding: 14px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-shadow-lg, 0 16px 48px rgba(0, 0, 0, 0.35));
  }

  .palette-input {
    width: 100%;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    padding: 10px 12px;
    font: inherit;
    font-size: 14px;
  }

  .palette-input:focus {
    outline: 2px solid var(--mdx-accent-primary);
    outline-offset: 1px;
  }

  .palette-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 2px;
    max-height: 46vh;
    overflow-y: auto;
  }

  .palette-item {
    display: grid;
    gap: 2px;
    width: 100%;
    text-align: left;
    border: none;
    border-radius: var(--mdx-radius-md);
    background: none;
    color: inherit;
    font: inherit;
    padding: 8px 10px;
    cursor: pointer;
  }

  .palette-item.highlight {
    background: color-mix(in srgb, var(--mdx-accent-primary) 12%, transparent);
  }

  .palette-item-label {
    font-size: 13.5px;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }

  .palette-item-hint {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
    line-height: 1.35;
  }

  .palette-empty {
    font-size: var(--mdx-text-xs);
    color: var(--mdx-text-muted);
    padding: 8px 10px;
    line-height: 1.4;
  }

  .palette-note {
    margin: 0;
    font-size: var(--mdx-text-xs);
    color: var(--mdx-accent-success);
  }

  .palette-foot {
    margin: 0;
    font-size: 11px;
    color: var(--mdx-text-muted);
    line-height: 1.4;
    border-top: 1px solid var(--mdx-border-subtle);
    padding-top: 8px;
  }

  .lanes {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }

  .lane {
    display: grid;
    gap: 3px;
    padding: 14px 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }

  .lane-count {
    font-family: var(--mdx-font-display);
    font-size: 22px;
    font-weight: 650;
    color: var(--mdx-text-primary);
  }

  .lane-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--mdx-text-secondary);
  }

  .lane-line {
    font-size: 11.5px;
    color: var(--mdx-text-muted);
    line-height: 1.45;
  }

  .quiet-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    color: var(--mdx-text-muted);
    font-size: var(--mdx-text-xs);
  }

  .quiet-more {
    display: inline;
  }

  .quiet-more summary {
    display: inline;
    color: var(--mdx-text-muted);
    cursor: pointer;
    list-style: none;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .quiet-more summary::-webkit-details-marker {
    display: none;
  }

  .quiet-more[open] summary {
    display: none;
  }

  @media (max-width: 860px) {
    .forge {
      padding-top: 2vh;
    }

    .first-read {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .ops.has-judgment {
      grid-template-columns: 1fr;
    }

    .stage-path {
      grid-template-columns: 1fr;
    }

    .lanes {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 560px) {
    .first-read {
      grid-template-columns: 1fr;
    }

    .fr-card {
      min-height: 0;
    }
  }

  /* The governance/overview view recedes into a disclosure - present and
     provable, but not the front door. The build box leads. */
  .forge-doctrine {
    margin-top: 4px;
  }
  .forge-doctrine > summary {
    list-style: none;
    cursor: pointer;
    padding: 11px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    color: var(--mdx-text-muted);
    font-size: 13px;
    font-weight: 550;
  }
  .forge-doctrine > summary::-webkit-details-marker {
    display: none;
  }
  .forge-doctrine > summary::before {
    content: "›";
    display: inline-block;
    margin-right: 9px;
    color: var(--mdx-text-tertiary);
    transition: transform var(--mdx-motion-fast) var(--mdx-ease);
  }
  .forge-doctrine[open] > summary::before {
    transform: rotate(90deg);
  }
  .forge-doctrine > summary:hover {
    color: var(--mdx-text-primary);
    border-color: var(--mdx-border-default);
  }
  .forge-doctrine[open] > summary {
    margin-bottom: 16px;
  }

  /* The front door: hand Forge something to build. Leads the page so the
     first thing an engineer sees is how to make something happen. */

  /* Decisions waiting: the returning operator's first job, promoted out of
     the fold into an actionable card near the top. Shows only when a build
     is waiting on a human call. */
  .decisions {
    margin-bottom: 14px;
    border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 35%, var(--mdx-border-default));
    border-radius: var(--mdx-radius-lg, var(--mdx-radius-md));
    background: color-mix(in srgb, var(--mdx-surface-base) 86%, var(--mdx-accent-primary));
    padding: 14px 16px;
    display: grid;
    gap: 10px;
  }
  .dec-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }
  .dec-eyebrow {
    color: var(--mdx-accent-primary);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }
  .dec-count {
    color: var(--mdx-text-muted);
    font-size: 12px;
    font-weight: 600;
  }
  .dec-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px 14px;
  }
  .dec-row + .dec-row {
    border-top: 1px solid var(--mdx-border-subtle);
    padding-top: 10px;
  }
  .dec-body {
    min-width: 0;
    display: grid;
    gap: 2px;
  }
  .dec-intent {
    display: block;
    min-width: 0;
    color: var(--mdx-text-primary);
    font-size: 13.5px;
    font-weight: 600;
    text-decoration: none;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dec-intent:hover {
    color: var(--mdx-accent-primary);
  }
  .dec-line {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 12px;
  }
  .dec-note {
    grid-column: 1 / -1;
    margin: 0;
    font-size: 12px;
    color: var(--mdx-text-secondary);
  }
  .dec-note.bad {
    color: var(--mdx-danger-red, #d4503e);
  }
  .dec-all {
    justify-self: start;
    color: var(--mdx-accent-primary);
    font-size: 12.5px;
    text-decoration: none;
  }
  .dec-all:hover {
    text-decoration: underline;
  }
  .dec-go {
    color: var(--mdx-text-muted);
    font-size: 14px;
  }
  .dec-dest {
    flex: none;
    color: var(--mdx-text-tertiary);
    font-size: 11.5px;
  }

  /* What is running: the pipeline pulse plus live capacity. Gate stages with a
     count are marked so the operator sees where a human is the next step. */
  .pipeline-band {
    display: grid;
    gap: 10px;
    padding: 14px 16px;
    border-radius: var(--mdx-radius-lg, 12px);
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }
  .pb-stages {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .pb-stage {
    display: grid;
    justify-items: center;
    gap: 2px;
    min-width: 64px;
    flex: 1 1 64px;
    padding: 8px 10px;
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-text-muted) 6%, transparent);
  }
  .pb-stage[data-gate="true"][data-active="true"] {
    background: color-mix(in srgb, var(--mdx-accent-warning) 16%, transparent);
  }
  .pb-stage[data-gate="false"][data-active="true"] {
    background: color-mix(in srgb, var(--mdx-accent-primary) 12%, transparent);
  }
  .pb-count {
    font-family: var(--mdx-font-display);
    font-size: 17px;
    font-weight: 700;
    line-height: 1;
  }
  .pb-stage[data-active="false"] .pb-count {
    color: var(--mdx-text-muted);
  }
  .pb-label {
    font-size: 11px;
    color: var(--mdx-text-muted);
    text-align: center;
  }
  .pb-capacity {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
    font-size: 12.5px;
    color: var(--mdx-text-muted);
    line-height: 1.45;
  }
  .pb-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
    background: var(--mdx-text-muted);
  }
  .pb-dot.live {
    background: var(--mdx-accent-primary);
    animation: pb-pulse 1.4s ease-in-out infinite;
  }
  @keyframes pb-pulse {
    50% {
      opacity: 0.35;
    }
  }

  /* Active mission, at a glance - leads the first viewport when meaty work
     is shaped. Links to the full cockpit on /forge/missions. */
  .active-mission {
    display: grid;
    gap: 4px;
    margin: 4px 0 18px;
    padding: 16px 18px;
    border: 1px solid var(--mdx-border-default);
    border-left: 3px solid var(--mdx-accent-warning, var(--mdx-text-muted));
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    text-decoration: none;
    color: inherit;
  }
  .active-mission[data-tone="active"] { border-left-color: var(--mdx-accent-primary); }
  .active-mission:hover { border-color: var(--mdx-border-strong, var(--mdx-accent-primary)); }
  .am-top { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }
  .am-eyebrow { font-size: 11.5px; font-weight: 650; text-transform: uppercase; letter-spacing: 0.03em; color: var(--mdx-text-muted); }
  .am-progress { font-size: 12.5px; color: var(--mdx-text-muted); white-space: nowrap; }
  .am-goal { margin: 0; font-size: 15px; font-weight: 650; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .am-line { margin: 0; font-size: 13px; color: var(--mdx-text-muted); }
  .am-open { margin-top: 4px; font-size: 12.5px; font-weight: 600; color: var(--mdx-accent-primary); }

  .forge-start {
    margin: 4px 0 22px;
    padding: 18px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
    min-width: 0;
    max-width: 100%;
    overflow: clip;
  }
  .fs-title {
    margin: 0 0 12px;
    font-family: var(--mdx-font-display);
    font-size: clamp(18px, 2vw, 22px);
    font-weight: 650;
  }
  .fs-intent {
    width: 100%;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    padding: 11px 13px;
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 14px;
    line-height: 1.5;
    resize: vertical;
    outline: none;
    min-width: 0;
    max-width: 100%;
  }
  .fs-intent:focus {
    border-color: var(--mdx-accent-primary);
  }
  .fs-run-plan {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 12px;
    align-items: center;
    margin-top: 10px;
    padding: 12px 14px;
    border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 20%, var(--mdx-border-subtle));
    border-left: 3px solid var(--mdx-accent-primary);
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-primary) 6%, transparent);
    min-width: 0;
  }
  .fs-run-plan[data-lane="fleet"] {
    border-left-color: var(--mdx-accent-warning, var(--mdx-accent-primary));
    background: color-mix(in srgb, var(--mdx-accent-warning, var(--mdx-accent-primary)) 7%, transparent);
  }
  .frp-copy {
    display: grid;
    gap: 3px;
    min-width: 0;
  }
  .frp-eyebrow {
    font-size: 10.5px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--mdx-text-muted);
  }
  .frp-copy strong {
    font-size: 14px;
    color: var(--mdx-text-primary);
  }
  .frp-copy p {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--mdx-text-secondary);
  }
  .frp-facts {
    display: flex;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 6px;
    min-width: min(320px, 100%);
  }
  .frp-facts span {
    min-height: 24px;
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 9px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--mdx-text-muted) 10%, transparent);
    color: var(--mdx-text-secondary);
    font-size: 11.5px;
    white-space: nowrap;
  }
  .frp-facts b {
    color: var(--mdx-text-primary);
    font-weight: 650;
  }
  .frp-facts .frp-auto {
    color: var(--mdx-accent-primary);
    background: color-mix(in srgb, var(--mdx-accent-primary) 11%, transparent);
  }
  /* The Codex-style context bar: a clean ask box, then a compact row of quiet
     controls (repo, checks popover, width popover, connect) and the actions. */
  .fs-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-top: 12px;
    padding-top: 12px;
    border-top: 1px solid var(--mdx-border-subtle);
    min-width: 0;
    max-width: 100%;
  }
  .fs-bar-ctx {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    min-width: 0;
    flex: 1 1 420px;
    max-width: 100%;
  }
  .fs-ctl {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 30px;
    padding: 0 11px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill, 999px);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-secondary);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    white-space: nowrap;
    max-width: 100%;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .fs-ctl:hover {
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 45%, var(--mdx-border-subtle));
    color: var(--mdx-text-primary);
  }
  .fs-ctl-repo {
    padding-right: 6px;
    min-width: 0;
  }
  .fs-ctl-repo select {
    border: none;
    background: transparent;
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
    outline: none;
    max-width: min(220px, 42vw);
    min-width: 0;
  }
  .fs-ctl-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 999px;
    background: var(--mdx-accent-primary);
    color: #fff;
    font-size: 10px;
    font-weight: 600;
  }
  .fs-ctl-link {
    color: var(--mdx-accent-primary);
  }
  .fs-ctl-pop {
    position: relative;
    max-width: 100%;
    min-width: 0;
  }
  .fs-ctl-pop > summary {
    list-style: none;
  }
  .fs-ctl-pop > summary::-webkit-details-marker {
    display: none;
  }
  .fs-pop {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    z-index: 30;
    width: 340px;
    max-width: min(80vw, calc(100vw - 48px));
    display: grid;
    gap: 7px;
    padding: 12px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-shadow-card);
  }
  .fs-pop-checks {
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    padding: 8px 10px;
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 12.5px;
    resize: vertical;
    outline: none;
    width: 100%;
    box-sizing: border-box;
  }
  .fs-pop-line {
    margin: 0;
    font-size: 11.5px;
    color: var(--mdx-text-muted);
  }
  .fs-strategy-pop .fs-pop {
    left: auto;
    right: 0;
  }
  .fs-strategy-panel {
    width: 620px;
    gap: 10px;
  }
  .fs-strategy-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
  }
  .fs-strategy-head p {
    margin: 3px 0 0;
    max-width: 480px;
    color: var(--mdx-text-muted);
    font-size: 12px;
  }
  .fs-strategy-head > span {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: 999px;
    padding: 3px 8px;
    color: var(--mdx-text-muted);
    font-size: 11px;
  }
  .fs-strategy-head > span.active {
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 45%, var(--mdx-border-subtle));
    color: var(--mdx-accent-primary);
  }
  .fs-strategy-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .fs-strategy-grid label,
  .fs-budget-grid label {
    display: grid;
    gap: 4px;
    color: var(--mdx-text-muted);
    font-size: 11px;
  }
  .fs-budget-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .fs-budget-grid input,
  .fs-strategy-grid select {
    box-sizing: border-box;
    width: 100%;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-sm, 6px);
    padding: 7px 8px;
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    font: inherit;
  }
  .fs-bar-go {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    min-width: 0;
    flex: 0 1 auto;
    max-width: 100%;
  }
  .fs-runhint {
    margin: 8px 0 0;
    font-size: 12px;
    color: var(--mdx-text-muted);
  }
  .fs-verbs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-top: 10px;
    min-width: 0;
  }
  .fs-verbs-label {
    font-size: 12px;
    color: var(--mdx-text-muted);
  }
  .fs-verb {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: 999px;
    padding: 4px 11px;
    background: transparent;
    color: var(--mdx-text-muted);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .fs-verb:hover {
    color: var(--mdx-text-primary);
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 45%, var(--mdx-border-subtle));
  }
  .fs-verb:hover {
    border-color: var(--mdx-accent-primary);
    color: var(--mdx-accent-primary);
  }
  .fs-scale {
    margin-top: 12px;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 8px 14px;
    align-items: center;
    padding: 12px 14px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-primary) 5%, transparent);
  }
  .fs-scale-copy {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 5px 10px;
    min-width: 0;
  }
  .fs-scale-copy span:last-child,
  .fs-scale-proof {
    color: var(--mdx-text-muted);
    font-size: 12.5px;
  }
  .fs-scale-options,
  .mf-presets {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 3px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-sunken);
  }
  .fs-scale-options button,
  .mf-presets button {
    min-width: 34px;
    border: none;
    border-radius: var(--mdx-radius-sm, 6px);
    background: transparent;
    color: var(--mdx-text-muted);
    cursor: pointer;
    font: inherit;
    font-size: 12.5px;
    padding: 5px 9px;
  }
  .fs-scale-options button:hover,
  .mf-presets button:hover {
    color: var(--mdx-text-primary);
  }
  .fs-scale-options button.active,
  .mf-presets button.active {
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
  }
  .fs-scale-proof {
    grid-column: 1 / -1;
    margin: 0;
  }
  .fs-row {
    display: flex;
    align-items: flex-end;
    gap: 12px;
    margin-top: 12px;
    flex-wrap: wrap;
  }
  .fs-field {
    display: grid;
    gap: 5px;
    flex: 1;
    min-width: 180px;
  }
  .fs-field-repo {
    flex: 0 0 200px;
  }
  .fs-label {
    color: var(--mdx-text-muted);
    font-size: 12px;
  }
  .fs-checks {
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    padding: 9px 11px;
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 13px;
    outline: none;
  }
  .fs-checks:focus {
    border-color: var(--mdx-accent-primary);
  }
  /* A clear action bar, so Start the build is the obvious primary move rather
     than one cramped control in the input row. */
  .fs-actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 10px;
    margin-top: 14px;
  }
  .fs-go {
    flex: none;
    height: 40px;
    padding: 0 22px;
    font-weight: 650;
  }
  .fs-direct {
    border: none;
    background: transparent;
    color: var(--mdx-text-muted);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
    padding: 4px 6px;
  }
  .fs-direct:hover:not(:disabled) {
    color: var(--mdx-text-primary);
    text-decoration: underline;
  }
  .fs-direct:disabled {
    opacity: 0.5;
    cursor: default;
  }
  @media (max-width: 560px) {
    .fs-run-plan {
      grid-template-columns: 1fr;
    }
    .frp-facts {
      justify-content: flex-start;
      min-width: 0;
    }
    .fs-actions {
      flex-direction: column-reverse;
      align-items: stretch;
    }
  }

  /* The recommendation card: Forge's read of the ask, with one-click accept.
     A proposal, never an execution grant - the operator accepts or adjusts. */
  .reco {
    margin-top: 14px;
    padding: 16px 18px;
    border: 1px solid var(--mdx-border-default);
    border-left: 3px solid var(--mdx-accent-primary);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-base);
    display: grid;
    gap: 9px;
  }
  .reco[data-shape="mission"] {
    border-left-color: var(--mdx-accent-warning, var(--mdx-text-muted));
  }
  .reco-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
  }
  .reco-eyebrow {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--mdx-text-muted);
  }
  .reco-conf {
    font-size: 11.5px;
    color: var(--mdx-text-muted);
    white-space: nowrap;
  }
  .reco-label {
    margin: 0;
    font-size: 21px;
    font-weight: 680;
    color: var(--mdx-text-primary);
  }
  .reco-why {
    margin: 0;
    font-size: 14px;
    line-height: 1.5;
    color: var(--mdx-text-secondary);
    max-width: 640px;
  }
  .reco-rationale {
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--mdx-text-muted);
  }
  .reco-facts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 18px;
    margin: 2px 0;
  }
  .reco-fact {
    display: grid;
    gap: 1px;
    min-width: 0;
  }
  .reco-fact span {
    font-size: 10.5px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--mdx-text-muted);
  }
  .reco-fact strong {
    font-size: 12px;
    font-weight: 500;
    color: var(--mdx-text-secondary);
    word-break: break-word;
  }
  .reco-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    margin-top: 2px;
  }
  .reco-go {
    height: 40px;
    padding: 0 22px;
    font-weight: 650;
  }
  .reco-dismiss {
    border: none;
    background: transparent;
    color: var(--mdx-text-muted);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }
  .reco-dismiss:hover {
    color: var(--mdx-text-primary);
  }
  .reco-adjust > summary {
    cursor: pointer;
    font-size: 12.5px;
    color: var(--mdx-accent-primary);
    width: fit-content;
  }
  .reco-adjust-body {
    display: grid;
    gap: 8px;
    margin-top: 8px;
    padding: 12px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-primary) 5%, transparent);
  }
  .reco-width-line {
    margin: 0;
    font-size: 12px;
    color: var(--mdx-text-muted);
  }
  .reco-alt {
    justify-self: start;
    border: none;
    background: transparent;
    color: var(--mdx-text-secondary);
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
    padding: 0;
  }
  .reco-alt:hover {
    color: var(--mdx-text-primary);
  }
  .reco-note {
    margin: 0;
    font-size: 11px;
    color: var(--mdx-text-muted);
  }

  /* Classify-then-confirm proposal. Receipt-backed recommendation, never an
     execution grant - the operator confirms the next move. */
  .classify {
    margin-top: 14px;
    padding: 16px 18px;
    border: 1px solid var(--mdx-border-default);
    border-left: 3px solid var(--mdx-accent-primary);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-base);
    display: grid;
    gap: 8px;
  }
  .classify[data-shape="mission"] {
    border-left-color: var(--mdx-accent-warning, var(--mdx-text-muted));
  }
  .classify[data-shape="done"] {
    border-left-color: var(--mdx-accent-success);
  }
  .mf-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px 12px;
    margin: 2px 0;
  }
  .mf-field {
    display: grid;
    gap: 4px;
    min-width: 0;
  }
  .mf-field.mf-wide {
    grid-column: 1 / -1;
  }
  .mf-field span {
    font-size: 12px;
    color: var(--mdx-text-muted);
  }
  .mf-field input,
  .mf-field textarea {
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    padding: 8px 10px;
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 13px;
    outline: none;
    width: 100%;
    box-sizing: border-box;
  }
  .mf-field textarea {
    resize: vertical;
  }
  .mf-presets {
    justify-self: start;
  }
  @media (max-width: 640px) {
    .mf-grid { grid-template-columns: repeat(2, 1fr); }
    .fs-scale { grid-template-columns: 1fr; }
    .fs-scale-options { justify-self: start; }
    .fs-strategy-grid,
    .fs-budget-grid { grid-template-columns: 1fr; }
    .fs-strategy-pop .fs-pop { left: 0; right: auto; }
  }
  .cl-head {
    margin: 0;
    font-size: 15px;
    font-weight: 650;
  }
  .cl-shape {
    margin: 0;
    font-size: 13.5px;
    color: var(--mdx-text-muted);
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 6px 10px;
  }
  .cl-detail {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 4px 14px;
    margin: 2px 0;
    font-size: 12.5px;
  }
  .cl-detail dt {
    font-weight: 650;
    color: var(--mdx-text-muted);
  }
  .cl-detail dd {
    margin: 0;
    font-family: var(--mdx-font-mono, monospace);
    font-size: 11.5px;
    word-break: break-word;
  }
  .cl-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin-top: 4px;
  }
  .cl-dismiss {
    border: none;
    background: none;
    cursor: pointer;
    font: inherit;
    font-size: 12.5px;
    color: var(--mdx-text-muted);
  }
  .cl-note {
    margin: 0;
    font-size: 12px;
    color: var(--mdx-text-muted);
    max-width: 620px;
  }
  /* Grounded in your standards: the guidelines (from Pages) every build
     follows - the credibility hook, shown right where you start one. */
  .fs-standards {
    display: flex;
    align-items: flex-start;
    gap: 11px;
    margin-top: 14px;
    padding: 13px 15px;
    border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 22%, transparent);
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-primary) 5%, transparent);
  }
  .fs-standards > svg {
    width: 17px;
    height: 17px;
    flex: none;
    margin-top: 1px;
    color: var(--mdx-accent-primary);
  }
  .fs-standards-body { display: grid; gap: 4px; }
  .fs-standards-head { margin: 0; font-size: 13px; font-weight: 650; color: var(--mdx-text-primary); }
  .fs-standards-sub { margin: 0; font-size: 12px; color: var(--mdx-text-secondary); line-height: 1.45; }
  .fs-standards-chips { display: flex; flex-wrap: wrap; gap: 7px; margin-top: 4px; }
  .fs-standard {
    padding: 3px 10px;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    text-decoration: none;
    font-size: 12px;
  }
  .fs-standard:hover {
    color: var(--mdx-accent-primary);
  }

  .fs-recent {
    margin-top: 18px;
    border-top: 1px solid var(--mdx-border-subtle);
    padding-top: 14px;
  }
  /* The first-run door: connect-as-hero, plus a calm way back into the form. */
  .fs-door {
    margin-top: 4px;
    display: grid;
    gap: 16px;
  }
  .fs-door-next {
    justify-self: start;
  }
  .fs-door-sub {
    margin: 0;
    max-width: 620px;
    color: var(--mdx-text-secondary);
    font-size: 13.5px;
    line-height: 1.55;
  }
  .fs-door-pick {
    display: grid;
    gap: 8px;
  }
  .fs-door-pick-head,
  .fs-door-eyebrow {
    margin: 0;
    color: var(--mdx-text-tertiary);
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }
  .fs-repo-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .fs-repo-chip {
    display: inline-flex;
    align-items: stretch;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-base);
    overflow: hidden;
  }
  .fs-repo-chip:hover {
    border-color: var(--mdx-accent-primary);
  }
  .chip-pick {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    border: none;
    background: none;
    color: var(--mdx-text-primary);
    cursor: pointer;
    padding: 7px 6px 7px 13px;
    font: inherit;
    font-size: 13px;
  }
  .chip-pick strong {
    font-weight: 600;
  }
  .chip-pick span {
    color: var(--mdx-text-muted);
    font-size: 11.5px;
  }
  .chip-x {
    border: none;
    background: none;
    color: var(--mdx-text-tertiary);
    cursor: pointer;
    padding: 0 10px 0 4px;
    font-size: 15px;
    line-height: 1;
  }
  .chip-x:hover {
    color: var(--mdx-danger-red, #d4503e);
  }
  .fs-door-connect {
    display: grid;
    gap: 9px;
    padding: 16px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-lg, var(--mdx-radius-md));
    background: color-mix(in srgb, var(--mdx-surface-base) 80%, var(--mdx-accent-primary));
  }
  .fs-door-connect .fs-connect {
    margin-top: 0;
    padding: 0;
    border: none;
    background: none;
  }
  .fs-door-mdx {
    justify-self: start;
    border: none;
    background: none;
    padding: 0;
    cursor: pointer;
    font: inherit;
    font-size: 12.5px;
    color: var(--mdx-text-muted);
  }
  .fs-door-mdx:hover {
    color: var(--mdx-text-primary);
  }
  .fs-field-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }
  .fs-repo-connect {
    border: none;
    background: none;
    padding: 0;
    cursor: pointer;
    font: inherit;
    font-size: 11.5px;
    color: var(--mdx-accent-primary);
  }
  /* One calm check hint replaces the old triple nag. */
  .fs-check-hint {
    margin: 10px 0 0;
    max-width: 720px;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.45;
  }
  /* Execution width is power-user depth, tucked behind a disclosure. */
  .fs-scale-disclosure {
    margin-top: 12px;
  }
  .fs-scale-current {
    color: var(--mdx-text-muted);
    font-weight: 550;
  }
  .fs-repo-intel {
    margin-top: 12px;
    padding: 14px 16px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-text-muted) 5%, transparent);
    display: grid;
    gap: 8px;
  }
  .fri-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  .fri-eyebrow {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    font-weight: 650;
  }
  .fri-eyebrow svg {
    color: var(--mdx-accent-primary);
    flex: none;
  }
  .fri-status,
  .fri-task-kind {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    padding: 2px 8px;
    color: var(--mdx-text-muted);
    font-size: 11.5px;
    font-weight: 600;
  }
  .fri-status[data-tone="ready"] {
    color: var(--mdx-accent-success);
    border-color: color-mix(in srgb, var(--mdx-accent-success) 45%, transparent);
  }
  .fri-status[data-tone="check"],
  .fri-status[data-tone="toolchain"] {
    color: var(--mdx-accent-warning, var(--mdx-text-secondary));
    border-color: color-mix(in srgb, var(--mdx-accent-warning, var(--mdx-text-secondary)) 42%, transparent);
  }
  .fri-foot {
    color: var(--mdx-text-tertiary);
    font-size: 11px;
  }
  .fri-note {
    margin: 0;
    max-width: 760px;
    color: var(--mdx-text-muted);
    font-size: 12.5px;
    line-height: 1.45;
  }
  .fri-proof {
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-sm);
    background: color-mix(in srgb, var(--mdx-accent-warning, var(--mdx-text-muted)) 8%, transparent);
    display: grid;
    gap: 5px;
    padding: 9px 10px;
  }
  .fri-proof[data-tone="ready"] {
    border-color: color-mix(in srgb, var(--mdx-accent-success) 40%, var(--mdx-border-subtle));
  }
  .fri-proof[data-tone="setup"] {
    border-color: color-mix(in srgb, var(--mdx-accent-warning, var(--mdx-text-secondary)) 42%, var(--mdx-border-subtle));
  }
  .fri-proof > div {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 8px;
  }
  .fri-proof span {
    color: var(--mdx-text-tertiary);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }
  .fri-proof strong {
    color: var(--mdx-text-primary);
    font-size: 12.5px;
    font-weight: 650;
    overflow-wrap: anywhere;
  }
  .fri-proof p {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 12.3px;
    line-height: 1.4;
  }
  .fri-proof .fri-proof-missing {
    color: var(--mdx-accent-warning, var(--mdx-text-secondary));
  }
  .fri-tasks {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }
  .fri-task {
    min-width: 0;
    min-height: 82px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    cursor: pointer;
    display: grid;
    align-content: start;
    gap: 6px;
    padding: 10px;
    text-align: left;
    font: inherit;
  }
  .fri-task:hover {
    border-color: var(--mdx-accent-primary);
  }
  /* Loading shimmer while Forge reads the repo - premium over a plain line. */
  .fri-skeleton-card,
  .fri-skeleton-line {
    background: linear-gradient(
      90deg,
      color-mix(in srgb, var(--mdx-surface-base) 90%, var(--mdx-border-subtle)) 25%,
      color-mix(in srgb, var(--mdx-surface-base) 72%, var(--mdx-border-subtle)) 50%,
      color-mix(in srgb, var(--mdx-surface-base) 90%, var(--mdx-border-subtle)) 75%
    );
    background-size: 320px 100%;
    animation: fri-shimmer 1.15s ease-in-out infinite;
  }
  .fri-skeleton-card {
    cursor: default;
  }
  .fri-skeleton-line {
    height: 13px;
    width: 190px;
    border-radius: var(--mdx-radius-pill);
  }
  @keyframes fri-shimmer {
    0% { background-position: -160px 0; }
    100% { background-position: 160px 0; }
  }
  @media (prefers-reduced-motion: reduce) {
    .fri-skeleton-card,
    .fri-skeleton-line {
      animation: none;
    }
  }
  .fri-task strong {
    font-size: 13px;
    line-height: 1.3;
  }
  .fri-task-top {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;
  }
  .fri-task-loc {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mdx-font-mono, monospace);
    font-size: 11px;
    color: var(--mdx-text-tertiary);
  }
  .fri-task-use {
    color: var(--mdx-accent-primary);
    font-size: 11.5px;
    font-weight: 600;
    opacity: 0;
    transition: opacity 0.12s ease;
  }
  .fri-task:hover .fri-task-use {
    opacity: 1;
  }
  @media (max-width: 840px) {
    .fri-tasks {
      grid-template-columns: 1fr;
    }
    .fri-task {
      min-height: auto;
    }
  }
  .fs-connect {
    margin-top: 12px;
    padding: 14px 16px;
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    display: grid;
    gap: 10px;
  }
  /* Paste-and-go: one joined field + button, a provider glyph, and a quiet
     toggle to the folder path. The frontier connect bar, frontend-only. */
  .fs-connect-field {
    display: flex;
    align-items: stretch;
    max-width: 560px;
    border-radius: var(--mdx-radius-md);
  }
  .fs-connect-field:focus-within {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--mdx-accent-primary) 35%, transparent);
  }
  .fs-connect-glyph {
    display: inline-flex;
    align-items: center;
    padding: 0 8px 0 11px;
    border: 1px solid var(--mdx-border-default);
    border-right: none;
    border-radius: var(--mdx-radius-md) 0 0 var(--mdx-radius-md);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-tertiary);
  }
  .fs-connect-url {
    flex: 1 1 auto;
    min-width: 0;
    border: 1px solid var(--mdx-border-default);
    border-right: none;
    padding: 9px 12px 9px 4px;
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 13px;
    outline: none;
  }
  /* First-run setup strip: the three-step golden path with live checkmarks. */
  .setup-strip {
    display: grid;
    gap: 10px;
    padding: 16px 18px;
    border-radius: var(--mdx-radius-lg, 12px);
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }
  .ss-lead {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 14px;
    font-weight: 650;
  }
  .kernel-down { border-color: color-mix(in srgb, var(--mdx-accent-warning, #b45309) 35%, transparent); }
  .kd-line { margin: 6px 0 8px; font-size: 13px; color: var(--mdx-text-secondary); line-height: 1.5; }
  .kd-cmd code { font-family: var(--mdx-font-mono, monospace); font-size: 12.5px; padding: 4px 9px; border-radius: 6px; background: var(--mdx-surface-raised); border: 1px solid var(--mdx-border-subtle); }
  .ss-steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }
  .ss-steps li {
    min-width: 0;
    list-style: none;
  }
  .ss-step {
    width: 100%;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 12px 14px;
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-text-muted) 6%, transparent);
    border: 1px solid transparent;
    font: inherit;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }
  .ss-step:disabled {
    cursor: default;
    opacity: 0.5;
  }
  .ss-step:not(:disabled):hover {
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 30%, var(--mdx-border-subtle));
  }
  .ss-step[data-done="true"] {
    background: color-mix(in srgb, var(--mdx-accent-success) 9%, transparent);
    border-color: transparent;
  }
  .ss-mark {
    flex: 0 0 auto;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--mdx-surface-base);
    border: 1px solid var(--mdx-border-subtle);
    font-size: 12px;
    font-weight: 700;
  }
  .ss-step[data-done="true"] .ss-mark {
    background: var(--mdx-accent-success);
    border-color: var(--mdx-accent-success);
    color: var(--mdx-on-accent, #fff);
  }
  .ss-body {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .ss-body strong {
    font-size: 13px;
  }
  .ss-step[data-current="true"] {
    background: color-mix(in srgb, var(--mdx-accent-primary) 12%, transparent);
    border-color: color-mix(in srgb, var(--mdx-accent-primary) 32%, transparent);
  }
  .ss-step[data-current="true"]:not([data-done="true"]) .ss-mark {
    background: var(--mdx-accent-primary);
    border-color: var(--mdx-accent-primary);
    color: var(--mdx-on-accent, #fff);
  }
  .ss-state {
    font-size: 12px;
    color: var(--mdx-text-muted);
  }
  .ss-step[data-current="true"] .ss-state {
    color: var(--mdx-accent-primary);
    font-weight: 600;
  }
  .ss-step[data-done="true"] .ss-state {
    color: var(--mdx-accent-success);
  }
  @media (max-width: 640px) {
    .ss-steps {
      grid-template-columns: 1fr;
    }
  }

  /* The active step's actual input, inline - so the flow never leaves the page
     or scrolls to something already on screen. */
  .step-card {
    display: grid;
    gap: 8px;
    padding: 20px 22px;
    border-radius: var(--mdx-radius-lg, 12px);
    background: var(--mdx-surface-raised);
    border: 1px solid var(--mdx-border-subtle);
    box-shadow: var(--mdx-edge-highlight), var(--mdx-shadow-card);
  }
  .sc-title {
    margin: 0;
    font-family: var(--mdx-font-display);
    font-size: 18px;
    font-weight: 650;
  }
  .sc-frame {
    margin: 0;
    max-width: 640px;
    color: var(--mdx-text-muted);
    font-size: 13.5px;
    line-height: 1.5;
  }
  .mc-hint {
    margin: 0;
    color: var(--mdx-text-muted);
    font-size: 12px;
    line-height: 1.45;
  }
  .mc-connected {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 7px;
  }
  .mc-conn-label {
    font-size: 11px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--mdx-text-muted);
  }
  .mc-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 6px 3px 10px;
    border-radius: var(--mdx-radius-pill, 999px);
    background: color-mix(in srgb, var(--mdx-accent-success) 14%, transparent);
    color: var(--mdx-accent-success);
    font-size: 12px;
    font-weight: 600;
  }
  .mc-chip-x {
    border: none;
    background: none;
    padding: 0 2px;
    color: inherit;
    opacity: 0.65;
    font-size: 14px;
    line-height: 1;
    cursor: pointer;
  }
  .mc-chip-x:hover {
    opacity: 1;
  }
  .mc-chip-x:disabled {
    cursor: default;
    opacity: 0.35;
  }
  .mc-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    margin-top: 2px;
  }
  .mc-next {
    flex: 0 0 auto;
  }
  @media (max-width: 560px) {
    .mc-bar {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 820px) {
    .forge-head,
    .fs-bar,
    .fs-bar-ctx,
    .fs-bar-go {
      align-items: stretch;
    }
    .fs-bar-ctx,
    .fs-bar-go {
      width: 100%;
    }
    .fs-go,
    .fs-direct {
      flex: 1 1 180px;
      justify-content: center;
      text-align: center;
    }
    .fs-ctl-repo select {
      max-width: 58vw;
    }
    .fs-pop {
      left: auto;
      right: 0;
    }
  }

  .fs-connect-browse {
    flex: none;
    border: 1px solid var(--mdx-border-default);
    border-right: none;
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-secondary);
    padding: 0 13px;
    font: inherit;
    font-size: 12.5px;
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
  }
  .fs-connect-browse:hover { color: var(--mdx-text-primary); }
  .fs-connect-browse:disabled { cursor: default; opacity: 0.6; }
  .fs-connect-go {
    flex: none;
    border-radius: 0 var(--mdx-radius-md) var(--mdx-radius-md) 0;
  }
  .fs-connect-alt {
    justify-self: start;
    border: none;
    background: none;
    padding: 0;
    cursor: pointer;
    font: inherit;
    font-size: 12px;
    color: var(--mdx-accent-primary);
  }
  .fs-connect-alt:hover {
    text-decoration: underline;
  }
  .fs-connect-note {
    margin: 0;
    font-size: 11.5px;
    color: var(--mdx-text-muted);
  }
  /* Clone progress: name the repo so a slow clone feels handled. */
  .fs-connect-progress {
    margin: 0;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--mdx-text-secondary);
  }
  .fs-connect-spin {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 2px solid color-mix(in srgb, var(--mdx-accent-primary) 30%, transparent);
    border-top-color: var(--mdx-accent-primary);
    animation: fs-connect-spin 0.7s linear infinite;
  }
  @keyframes fs-connect-spin {
    to { transform: rotate(360deg); }
  }
  @media (prefers-reduced-motion: reduce) {
    .fs-connect-spin { animation-duration: 1.6s; }
  }
  /* A connect error reads as an error: icon, clean line, raw git tucked away. */
  .fs-connect-err {
    display: flex;
    gap: 9px;
    padding: 10px 12px;
    border: 1px solid color-mix(in srgb, var(--mdx-danger-red, #d4503e) 40%, var(--mdx-border-default));
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-surface-base) 88%, var(--mdx-danger-red, #d4503e));
  }
  .fs-connect-raw {
    margin-top: 5px;
  }
  /* Success confirmation after a connect, shown from the form side. */
  .fs-connected {
    margin: 0 0 10px;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 6px 12px;
    border-radius: var(--mdx-radius-pill);
    background: color-mix(in srgb, var(--mdx-surface-base) 84%, var(--mdx-accent-success));
    color: var(--mdx-accent-success);
    font-size: 12.5px;
    font-weight: 600;
  }
  .fs-connected > svg {
    width: 14px;
    height: 14px;
  }
  .fs-handoff {
    margin: 0 0 10px;
    padding: 9px 11px;
    border: 1px solid color-mix(in srgb, var(--mdx-accent-primary) 20%, transparent);
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-primary) 7%, transparent);
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.45;
  }
  .fs-playground {
    margin: 10px 0 0;
    padding: 10px 11px;
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 10px;
    align-items: center;
    border: 1px solid color-mix(in srgb, var(--mdx-accent-success, #2fbf7a) 24%, var(--mdx-border-default));
    border-radius: var(--mdx-radius-md);
    background: color-mix(in srgb, var(--mdx-accent-success, #2fbf7a) 7%, var(--mdx-surface-base));
    color: var(--mdx-text-secondary);
    font-size: 12.5px;
    line-height: 1.45;
  }
  .fs-playground strong {
    display: block;
    margin-bottom: 2px;
    color: var(--mdx-text-primary);
    font-size: 13px;
  }
  .fs-playground p {
    margin: 0;
  }
  .fs-playground-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--mdx-accent-success, #2fbf7a);
  }
  .fs-playground button {
    border: 1px solid var(--mdx-border-default);
    border-radius: var(--mdx-radius-sm);
    background: var(--mdx-surface-raised);
    color: var(--mdx-text-primary);
    font: inherit;
    font-size: 12px;
    font-weight: 650;
    padding: 7px 10px;
    cursor: pointer;
    white-space: nowrap;
  }
  .fs-playground button:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .fs-recent-head {
    margin: 0 0 8px;
    color: var(--mdx-text-tertiary);
    font-size: 11.5px;
    font-weight: 600;
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }
  .fs-recent-list {
    display: grid;
    gap: 2px;
    min-width: 0;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .fs-recent-row {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    min-width: 0;
    overflow: hidden;
    padding: 9px 10px;
    border-radius: var(--mdx-radius-md);
    text-decoration: none;
    color: var(--mdx-text-secondary);
    font-size: 13px;
  }
  .fs-recent-main { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .fs-recent-top { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .fs-recent-title { min-width: 0; font-weight: 550; color: var(--mdx-text-primary); line-height: 1.35; display: -webkit-box; -webkit-line-clamp: 1; -webkit-box-orient: vertical; overflow: hidden; }
  .fs-recent-row:hover {
    background: var(--mdx-surface-base);
    color: var(--mdx-text-primary);
  }
  .fs-recent-dot {
    width: 8px;
    height: 8px;
    flex: none;
    margin-top: 5px;
    border-radius: 50%;
    background: var(--mdx-text-tertiary);
  }
  .fs-recent-dot[data-status="done"],
  .fs-recent-dot[data-status="finished"] {
    background: var(--mdx-success-green, #2faa6a);
  }
  .fs-recent-dot[data-status="stopped"],
  .fs-recent-dot[data-status="error"] {
    background: var(--mdx-danger-red, #d4503e);
  }
  .fs-recent-dot.spin {
    background: var(--mdx-accent-primary);
  }
  .fs-recent-status {
    flex: none;
    font-weight: 550;
    color: var(--mdx-text-primary);
  }
  .fs-recent-summary {
    min-width: 0;
    color: var(--mdx-text-muted);
    line-height: 1.45;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .fs-recent-branch {
    flex: 0 1 45%;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--mdx-font-mono, monospace);
    font-size: 11px;
    color: var(--mdx-text-tertiary);
  }
  .fs-allruns {
    display: inline-block;
    margin-top: 10px;
    color: var(--mdx-accent-primary);
    text-decoration: none;
    font-size: 12.5px;
  }

  /* The flywheel band: what finished builds leave behind. Quiet, confident,
     evidence-backed. The proof and limits live one tap away. */
  .flywheel {
    margin: 18px 0 0;
    padding: 18px 20px;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-lg);
    background: var(--mdx-surface-raised);
    box-shadow: var(--mdx-shadow-card);
  }
  .fw-head {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .fw-title {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }
  .fw-sub {
    margin: 0;
    font-size: 13px;
    color: var(--mdx-text-secondary);
  }
  .fw-counts {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
    margin: 16px 0 0;
    padding: 0;
    list-style: none;
  }
  .fw-counts li {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 14px;
    border-radius: var(--mdx-radius-md);
    background: var(--mdx-surface-base);
  }
  .fw-n {
    font-size: 22px;
    font-weight: 650;
    color: var(--mdx-text-primary);
    line-height: 1.1;
  }
  .fw-l {
    font-size: 12px;
    color: var(--mdx-text-muted);
  }
  .fw-empty {
    margin: 14px 0 0;
    font-size: 13px;
    color: var(--mdx-text-muted);
  }
  .fw-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 14px 0 0;
  }
  .fw-chip {
    font-size: 11.5px;
    padding: 4px 10px;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-secondary);
  }
  .fw-chip.safe {
    color: var(--mdx-accent-success, var(--mdx-success-green));
  }
  .fw-chip.held {
    color: var(--mdx-text-tertiary);
  }
  .fw-lessons {
    margin: 16px 0 0;
    padding-top: 14px;
    border-top: 1px solid var(--mdx-border-subtle);
  }
  .fw-lessons-head {
    margin: 0 0 8px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--mdx-text-primary);
  }
  .fw-lessons ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .fw-lesson {
    display: flex;
    align-items: center;
    gap: 12px;
    justify-content: space-between;
    font-size: 13px;
    color: var(--mdx-text-secondary);
  }
  .fw-lesson-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fw-lesson-count {
    flex: none;
    margin-left: auto;
    border: 1px solid var(--mdx-border-subtle);
    border-radius: var(--mdx-radius-pill);
    padding: 1px 8px;
    color: var(--mdx-text-muted);
    font-size: 11px;
    font-weight: 600;
  }
  .fw-lesson-link {
    flex: none;
    font-size: 12px;
    color: var(--mdx-accent-primary);
    text-decoration: none;
  }
  .fw-lessons-note {
    margin: 8px 0 0;
    font-size: 11.5px;
    color: var(--mdx-text-tertiary);
  }
  .fw-proof {
    margin: 14px 0 0;
  }
  .fw-proof > summary {
    cursor: pointer;
    font-size: 12.5px;
    color: var(--mdx-text-tertiary);
  }
  .fw-proof-list {
    margin: 10px 0 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .fw-proof-list li {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    font-size: 12.5px;
  }
  .fw-proof-list li > span:first-child {
    color: var(--mdx-text-muted);
  }
  .fw-proof-list li > span:last-child {
    color: var(--mdx-text-secondary);
    text-align: right;
  }
  .fw-gaps {
    margin: 12px 0 0;
  }
  .fw-gaps-head {
    margin: 0 0 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--mdx-text-secondary);
  }
  .fw-gaps ul {
    margin: 0;
    padding-left: 16px;
    font-size: 12px;
    color: var(--mdx-text-tertiary);
  }
  .fs-recent-outcome {
    flex: none;
    font-size: 11px;
    padding: 1px 7px;
    border-radius: var(--mdx-radius-pill);
    background: var(--mdx-surface-base);
    color: var(--mdx-text-tertiary);
  }
  @media (max-width: 640px) {
    .fw-counts {
      grid-template-columns: 1fr;
    }
    .fw-proof-list li {
      flex-direction: column;
      gap: 2px;
    }
    .fw-proof-list li > span:last-child {
      text-align: left;
    }
    .fw-lesson {
      flex-wrap: wrap;
      gap: 4px 10px;
    }
  }
  .fs-recent-list li{position:relative}.fs-pack-draft{margin:0 0 8px 34px;border:0;background:none;color:var(--mdx-accent-primary);font:600 10px inherit;cursor:pointer}.pack-draft{display:grid;gap:9px;margin-top:10px;padding:13px;border:1px solid var(--mdx-border-subtle);border-radius:var(--mdx-radius-md);background:var(--mdx-surface-raised)}.pack-draft>div:first-child{display:flex;justify-content:space-between;gap:10px;font-size:10px;color:var(--mdx-text-muted)}.pack-draft label{display:grid;gap:3px;font-size:10px;color:var(--mdx-text-muted)}.pack-draft input,.pack-draft textarea{box-sizing:border-box;width:100%;padding:7px 9px;border:1px solid var(--mdx-border-default);border-radius:var(--mdx-radius-sm);background:var(--mdx-surface-base);color:var(--mdx-text-primary);font:inherit}.pack-draft p,.pack-proposal-flash{margin:0;font-size:10px;color:var(--mdx-text-muted)}.pack-draft>div:last-child{display:flex;justify-content:flex-end;gap:7px}.pack-draft>div:last-child button{border:1px solid var(--mdx-border-subtle);border-radius:99px;padding:6px 10px;background:none;color:var(--mdx-text-secondary);cursor:pointer}.pack-proposal-flash{color:var(--mdx-accent-success)}.pack-proposal-flash.bad{color:var(--mdx-accent-warning)}
</style>
