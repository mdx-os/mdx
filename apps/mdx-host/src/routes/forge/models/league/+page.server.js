// The machines lens: which build machine Forge runs your work on, the others
// it can govern, and what it has learned from them. Read-only. The
// recommendation is Forge's current stance (it names the house builder and a
// fallback); the learning ledger fails closed and shows only what accepted
// evidence supports. An unreachable kernel renders an honest empty state.

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

export async function load({ fetch }) {
  const [recommendation, ledger, scoreboard, preflight, faceOffs, clearances, fleetRuns, arbitrations, improvementLoops] = await Promise.all([
    readJson(fetch, "/forge/machine-league/recommendations.json"),
    readJson(fetch, "/forge/machine-league/learning-ledger.json"),
    // The raw routing evidence. Operator-facing, kept behind a disclosure on
    // this page - it is not the front door. Native is always the baseline.
    readJson(fetch, "/forge/fleet-eval-scoreboard.json"),
    // Preflight tells us whether live execution is turned on for a runner
    // (binary present + MDX_MACHINE_LEAGUE_CODEX_EXEC=1). Presence only, no
    // secrets. When off, the lens offers a held trial; when on, a live run.
    readJson(fetch, "/forge/machine-league/preflight.json"),
    // The product packet that binds recommendation, cast, trial, native
    // comparison, and learning state into one operator-facing story.
    readJson(fetch, "/forge/machine-league/face-offs/projection.json"),
    // Which sign-in-gated machines (Claude Code, Grok Build) an operator has
    // cleared, and in which mode. Clearing never grants execution by itself -
    // it stays env-gated and quarantined.
    readJson(fetch, "/forge/machine-league/closed-runner-clearances/projection.json"),
    // Fleet runs: a mix of machines actually executing one task in parallel,
    // each lane quarantined. Arbitration recommends a winner from accepted
    // lanes only; a human still ratifies. Improvement loops take a distilled
    // edge into a Builder Loop and plan a re-measurement.
    readJson(fetch, "/forge/machine-league/fleet-runs/projection.json"),
    readJson(fetch, "/forge/machine-league/fleet-runs/arbitrations/projection.json"),
    readJson(fetch, "/forge/machine-league/native-improvement-loops/projection.json")
  ]);

  return {
    recommendation,
    ledger,
    scoreboard,
    preflight,
    faceOffs,
    clearances,
    fleetRuns,
    arbitrations,
    improvementLoops,
    boundary:
      "Forge owns admission, scope, checks, and the final say; an outside machine never counts until Forge's checks accept its work"
  };
}
