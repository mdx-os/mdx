// Missions is the cockpit for long-horizon, meaty work: a big goal becomes a
// governed mission packet with milestones, validation cadence, budget, fleet
// width, and a model policy. This loader reads the mission dashboard and the
// supporting eval/readiness routes and joins them into one operator view.
//
// Every label this page shows is derived from what the routes actually return
// (the receipts that exist), not hardcoded. Reads fail soft to null so a
// kernel that has not compiled these routes yet still opens with an honest
// "Forge can shape a mission, but cannot run live workers yet" state.

async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      signal: AbortSignal.timeout(1500)
    });
    return response.ok ? await response.json() : null;
  } catch (error) {
    return null;
  }
}

export async function load({ fetch, locals }) {
  const [dashboard, projection, scoreboard, providerPreflight, liveRunApprovals] =
    await Promise.all([
      readJson(fetch, "/forge/long-horizon-mission-dashboard.json"),
      readJson(fetch, "/forge/long-horizon-missions/projection.json"),
      readJson(fetch, "/forge/fleet-eval-scoreboard.json"),
      readJson(fetch, "/forge/fleet-eval-provider-preflight.json"),
      readJson(fetch, "/forge/fleet-eval-live-run-approvals/projection.json")
    ]);

  return {
    session: locals.session,
    // The mission cockpit reads, passed through whole. The page prefers the
    // dashboard for counts and the projection for per-mission detail, and
    // keeps its honest empty state when both are null.
    dashboard,
    projection,
    scoreboard,
    providerPreflight,
    liveRunApprovals
  };
}
