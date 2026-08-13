// Fleets: one spec built by parallel agents. Reads the plan projection
// (the streams and their seams) and the run projection (live lanes,
// integration, review) - the page joins them into one card per fleet.
// Fail-soft: an unreachable kernel renders an empty, honest invitation.

async function readJson(fetchImpl, path, timeoutMs = 1500) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      signal: AbortSignal.timeout(timeoutMs)
    });
    return response.ok ? await response.json() : null;
  } catch (error) {
    return null;
  }
}

export async function load({ fetch, url, depends }) {
  // Scoped live-poll key: invalidate("forge:fleets") re-runs only this loader,
  // not the shell layout, so a live fleet does not re-download the shell's
  // attention projection on every tick.
  depends("forge:fleets");
  const [plans, runs, repos, capacity, controlPlane] = await Promise.all([
    readJson(fetch, "/forge/fleet-plans/projection.json"),
    // The live-progress projections carry per-lane event trails that grow as
    // the build runs; give them more headroom so a poll tick returns fresh
    // progress instead of aborting to null (which froze the view at the first
    // post-start snapshot until a manual reload).
    readJson(fetch, "/forge/fleet-runs/projection.json", 4000),
    readJson(fetch, "/forge/repos/projection.json"),
    readJson(fetch, "/forge/fleet-capacity.json"),
    // The control-plane fold carries the per-fleet and per-lane result rollups.
    readJson(fetch, "/forge/control-plane/projection.json", 4000)
  ]);

  return {
    plans,
    runs,
    repos,
    capacity,
    controlPlane,
    focusFleetId: url.searchParams.get("fleet_id") ?? "",
    preferredRepoId: url.searchParams.get("repo_id") ?? "",
    preferredCloudEnvironmentId: url.searchParams.get("cloud_environment_id") ?? "",
    boundary:
      "every stream works in its own isolated copy, confined to its own files; the fleet produces one reviewed branch, never a deploy",
    safeNext:
      "describe what to build, look the proposed split in the eye, and the build starts only after your yes"
  };
}
