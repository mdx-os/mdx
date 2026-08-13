// Forge Review Room: the one place to read a finished run as a change you
// decide on. Loads the run projection so the room can offer a review queue
// (what is ready, what needs you); the packet and diff are fetched per run on
// the client. Fail-soft: an unreachable kernel renders an honest empty room.

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

export async function load({ fetch, depends }) {
  // Scoped dependency: the client live-polls by invalidating this key, which
  // re-runs only this loader (the runs projection), not the whole layout
  // waterfall. A run finishing while you sit here then appears on its own.
  depends("forge:runs");
  const runs = await readJson(fetch, "/forge/runs/projection.json");
  return {
    runs,
    boundary:
      "review reads the change the agent already committed to a branch; it opens no authority and changes nothing - the ship decision stays its own governed step",
    safeNext: "open a finished run, read the change in order, and decide the one next move"
  };
}
