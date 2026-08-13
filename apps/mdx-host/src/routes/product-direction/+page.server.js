// Product is the bets pipeline: five stages derived from records, one
// altitude below the strategy board. Everything loads through the kernel
// proxy, fail-soft - an unreachable projection renders an inviting empty
// pipeline, never an invented one. Route stays /product-direction; the
// kernel keeps /product for its own endpoints.

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
  const [bets, conditions, decisions, resolutions, work, stream] = await Promise.all([
    readJson(fetch, "/product/bet-drafts/projection.json"),
    readJson(fetch, "/product/bet-conditions/projection.json"),
    readJson(fetch, "/product/ratification-decisions/projection.json"),
    readJson(fetch, "/product/bet-resolutions/projection.json"),
    readJson(fetch, "/product/work-items/projection.json"),
    readJson(fetch, "/product/triage/projection.json")
  ]);

  return {
    pipeline: { bets, conditions, decisions, resolutions, work },
    work,
    openSignal: typeof stream?.open_count === "number" ? stream.open_count : 0,
    boundary:
      "bets move only by records - anatomy, judged claims, a person's backing, a resolution; backing opens no authority by itself",
    safeNext: "follow the next-move line at the top, or shape a new bet from one sentence"
  };
}
