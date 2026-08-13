// The model scorecard: which coder is performing, read straight from the
// chain. Fail-soft: an unreachable kernel renders an honest empty state.

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
  // The scorecard is the running tally from real runs. The eval lane is the
  // senior-engineer benchmark: how ready Forge is for serious classes of
  // work, measured on the same corpus across approved provider profiles.
  // Both fail soft to null and render honest "not measured yet" states.
  const [scorecard, scoreboard, providerPreflight, providerRouting, modelReadiness] = await Promise.all([
    readJson(fetch, "/forge/model-scorecard.json"),
    readJson(fetch, "/forge/fleet-eval-scoreboard.json"),
    readJson(fetch, "/forge/fleet-eval-provider-preflight.json"),
    readJson(fetch, "/forge/model-providers.json"),
    readJson(fetch, "/models/readiness.json")
  ]);

  return {
    scorecard,
    scoreboard,
    providerPreflight,
    providerRouting,
    modelReadiness,
    boundary:
      "every number is derived from run receipts - nothing here is self-reported, and same-task comparisons are the fair reads"
  };
}
