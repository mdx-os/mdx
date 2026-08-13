// Needs you: the notifications surface. One list of everything waiting on a
// person - the kernel's record-backed reasons plus the ready-to-back bets from
// the same pipeline fold the Product page uses. Reached from Home's quiet
// notifications pill rather than a top-level nav item. Fail-soft: unreachable
// projections render an empty, honest inbox.
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
  const [needsYou, bets, conditions, decisions, resolutions] = await Promise.all([
    readJson(fetch, "/product/needs-you/projection.json"),
    readJson(fetch, "/product/bet-drafts/projection.json"),
    readJson(fetch, "/product/bet-conditions/projection.json"),
    readJson(fetch, "/product/ratification-decisions/projection.json"),
    readJson(fetch, "/product/bet-resolutions/projection.json")
  ]);

  return {
    needsYou,
    pipeline: { bets, conditions, decisions, resolutions },
    boundary:
      "every row here is a record waiting on a person; acting happens behind its own door, never from this list",
    safeNext: "start at the top group - decisions first, then signatures, then reviews and reading"
  };
}
