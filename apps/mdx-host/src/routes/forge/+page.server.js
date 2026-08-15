// Forge is the governed builder workspace: the page reads the live run rails
// (runs, repos, recipes, missions, fleet, control plane) plus the runtime
// truth routes. Reads fail soft to empty so a kernel-down Forge still opens
// with an honest invitation. The session rides through for client-side
// admission.
import { platformProof } from "../../lib/platform.js";
import { runtimeProjections } from "../../lib/forgeSession.js";
import { forgeKernelReachable } from "../../lib/forgeReachability.js";
import { verifiedHostSessionRequired } from "../../lib/session.server.js";

async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      signal: AbortSignal.timeout(1500)
    });
    if (!response.ok) {
      return null;
    }
    return await response.json();
  } catch (error) {
    return null;
  }
}

async function writeJson(fetchImpl, path, body) {
  const response = await fetchImpl(`/api/kernel${path}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body)
  });
  return response.json().catch(() => ({}));
}

export const actions = {
  prepareCloudSandbox: async ({ request, fetch, locals }) => {
    const form = await request.formData();
    const repoId = String(form.get("repo_id") ?? "");
    if (!/^[A-Za-z0-9._-]{1,128}$/.test(repoId)) {
      return { cloudSandboxOk: false, cloudSandboxLine: "Choose a connected repo first." };
    }
    const base = { repo_id: repoId, actor_id: locals.session?.user_id ?? "local_user" };
    const prepared = await writeJson(fetch, "/mobile/cloud/environments.json", { ...base, action: "prepare" });
    if (!String(prepared?.status ?? "").startsWith("PREPARED")) {
      return { cloudSandboxOk: false, cloudSandboxLine: prepared?.detail ?? "Cloud preparation did not finish." };
    }
    const verified = await writeJson(fetch, "/mobile/cloud/environments.json", { ...base, action: "verify" });
    return verified?.status === "VERIFIED"
      ? { cloudSandboxOk: true, cloudSandboxLine: "Cloud sandbox verified. Forge will use it for this repo." }
      : { cloudSandboxOk: false, cloudSandboxLine: verified?.detail ?? "Cloud verification did not finish." };
  }
};

export async function load({ fetch, locals, url, parent }) {
  const parentData = await parent();
  const [
    forgeRuntime,
    operatorPacket,
    runtimeMetrics,
    queueProjection,
    envelopeProjection
  ] = await Promise.all([
    readJson(fetch, runtimeProjections.forge),
    // Runtime truth, read-only: the operator decision packet, the runtime
    // metrics, the durable queue state, and the standing authorization
    // registry. Each fails soft to null and the page keeps its honest copy.
    readJson(fetch, runtimeProjections.operatorPacket),
    readJson(fetch, runtimeProjections.metrics),
    readJson(fetch, runtimeProjections.queue),
    readJson(fetch, runtimeProjections.envelopes)
  ]);

  // The front door is the working build loop: the runs that actually execute,
  // the repos a run can target, and the recipe quick-starts. Read here so the
  // home leads with "hand Forge something to build", not the governance rail.
  const [runs, repos, recipes, builderLoops, pagesPublished, missionDashboard, missionProjection, evalScoreboard, fleetCapacity, controlPlane, modelProviders, modelReadiness, cloudSetup] =
    await Promise.all([
      readJson(fetch, "/forge/runs/projection.json"),
      readJson(fetch, "/forge/repos/projection.json"),
      readJson(fetch, "/forge/recipes.json"),
      readJson(fetch, "/forge/builder-loops.json"),
      readJson(fetch, "/pages/publications/projection.json"),
      // Long-horizon missions: when one is shaped, Overview shows an at-a-glance
      // summary so the operator sees the meaty work first. Fail soft to null;
      // the full cockpit lives on /forge/missions.
      readJson(fetch, "/forge/long-horizon-mission-dashboard.json"),
      readJson(fetch, "/forge/long-horizon-missions/projection.json"),
      // The eval lane, so the classify proposal can ground confidence in real
      // per-class coverage rather than a generic link. Fail soft to null.
      readJson(fetch, "/forge/fleet-eval-scoreboard.json"),
      // The scale lane: bounded worker capacity and queue posture. This makes
      // the front-door width dial honest about N-wide work.
      readJson(fetch, "/forge/fleet-capacity.json"),
      // The control-plane fold: one read for the full decision inbox, the
      // pipeline pulse, and live capacity. Read-only, grants no authority.
      readJson(fetch, "/forge/control-plane/projection.json"),
      // Provider readiness, so the first-run setup strip knows whether a model
      // is connected. Read-only.
      readJson(fetch, "/forge/model-providers.json"),
      // Canonical cross-surface model truth. Forge consumes Model Fabric rather
      // than maintaining a second definition of connected.
      readJson(fetch, "/models/readiness.json"),
      // Hosted Forge readiness. A verified environment is the one signal that
      // lets the composer route checks through the isolated cloud command plane.
      readJson(fetch, "/mobile/cloud/setup.json")
    ]);

  // The machine-league stance: which builder Forge runs work with, and the
  // fallback it can stage as a held trial. Read-only, fail-soft; the composer
  // never waits on it.
  const machineLeague = await readJson(fetch, "/forge/machine-league/recommendations.json");

  // The flywheel: what a finished build leaves behind so the next one is
  // better. The proof summarizes how real the local loop is (isolated
  // worktree, never the live repo) and what it has learned; the outcome
  // signals are the per-run record; the candidate lessons are what those
  // outcomes propose (citation-only, never auto-applied); the installed
  // capabilities are what the team can lean on. Each fails soft to null.
  const [flywheelProof, outcomeSignals, candidateLessons, installedCapabilities] =
    await Promise.all([
      readJson(fetch, "/forge/flywheel-proof.json"),
      readJson(fetch, "/forge/outcome-signals/projection.json"),
      readJson(fetch, "/learning/forge-outcome-candidates/projection.json"),
      readJson(fetch, "/marketplace/installed-capabilities/projection.json")
    ]);

  // The standards Forge builds to: the engineering guidelines you publish in
  // Pages (titled "Standard: ..."). Forge cites them so an engineer can see
  // it is following your rules, not improvising - the credibility hook. The
  // source is Pages now; a Marketplace standards capability plugs in later.
  const publications = Array.isArray(pagesPublished?.publications)
    ? pagesPublished.publications
    : Array.isArray(pagesPublished?.documents)
      ? pagesPublished.documents
      : [];
  const standards = publications
    .filter((doc) => /^standard\b/i.test(String(doc?.title ?? "")) || String(doc?.document_id ?? "").includes("standard"))
    .map((doc) => ({
      documentId: String(doc?.document_id ?? ""),
      title: String(doc?.title ?? "").replace(/^standard:\s*/i, "")
    }));

  // Reachability belongs to the tiny version probe in the root layout. A large
  // projection can be slow or temporarily unavailable while the kernel itself
  // remains healthy, so its absence must never turn into a false offline state.
  const kernelReachable = forgeKernelReachable(parentData);
  const githubOutcome = url.searchParams.get("github");
  const githubNotice =
    githubOutcome === "connected"
      ? { ok: true, line: "GitHub is connected. Choose the repository for your first build." }
      : githubOutcome === "partial"
        ? { ok: false, line: "Some GitHub repositories connected, but at least one needs another try." }
        : githubOutcome === "no-repositories"
          ? { ok: false, line: "GitHub is connected, but no repositories were selected. Open Connect GitHub and choose at least one." }
          : githubOutcome === "narrow-selection"
            ? { ok: false, line: "Choose 20 or fewer repositories for this beta workspace, then connect GitHub again." }
            : githubOutcome === "setup-required"
              ? { ok: false, line: "GitHub connection is still being prepared for this beta. You can use a public repository or try again later." }
            : githubOutcome === "connection-failed"
              ? { ok: false, line: "GitHub did not finish connecting. Try Connect GitHub again." }
              : null;

  // The runs projection embeds every run's full event trail (~1MB measured).
  // The front door needs list-level truth only; the run viewer fetches its
  // own trail. Strip events and cap the list BEFORE this object is
  // serialized twice (SSR HTML + __data.json).
  const slimRuns = runs
    ? {
        ...runs,
        runs: (Array.isArray(runs.runs) ? runs.runs : []).slice(0, 60).map((run) => {
          const { events, ...rest } = run ?? {};
          return { ...rest, event_count: run?.event_count ?? (Array.isArray(events) ? events.length : 0) };
        })
      }
    : runs;

  return {
    proof: platformProof,
    session: locals.session,
    playgroundAvailable: !verifiedHostSessionRequired(),
    githubNotice,
    kernelReachable,
    runs: slimRuns,
    repos,
    recipes,
    builderLoops,
    machineLeague,
    standards,
    missionDashboard,
    missionProjection,
    evalScoreboard,
    fleetCapacity,
    controlPlane,
    modelProviders,
    modelReadiness,
    cloudSetup,
    forgeRuntime,
    // The flywheel rails: read-only, fail-soft. The page renders them as the
    // "this loop is real and compounds" story and keeps the proof one tap
    // away. Nothing here opens execution, write, or deployment authority.
    flywheelProof,
    outcomeSignals,
    candidateLessons,
    installedCapabilities,
    // The runtime read routes, passed through whole. The page prefers them
    // when present and falls back to its local derivation when they are null
    // or report an honest empty state. Nothing here is fabricated.
    operatorPacket,
    runtimeMetrics,
    queueProjection,
    envelopeProjection
  };
}
