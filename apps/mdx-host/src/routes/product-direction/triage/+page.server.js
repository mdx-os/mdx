// Incoming: the one stream where signal waits for a human call. Loads
// the triage projection (posted entries and feedback captures, already
// interleaved by the kernel) plus the bet list, so accepting can file
// work under the bet it serves. Fail-soft: an unreachable projection
// renders an inviting empty stream, never an invented one.

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
  const [stream, bets] = await Promise.all([
    readJson(fetch, "/product/triage/projection.json"),
    readJson(fetch, "/product/bet-drafts/projection.json")
  ]);

  return {
    stream,
    bets,
    boundary:
      "four calls and nothing else - accept, duplicate, decline, snooze; accepting mints the work or the bet on the record",
    safeNext: "take the top entry and make the call - one key each"
  };
}
