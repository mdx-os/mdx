// Strategy is a board of cards derived from records: proposals are the
// cards, conditions and their verdicts are readiness, ratification
// decisions and resolutions move cards across columns. Everything loads
// through the kernel proxy, fail-soft: a missing projection renders an
// inviting empty board, never an invented one. The legacy fixture
// question stands in as one ready-to-decide card only while no human has
// authored a card yet, so a fresh install still opens with a real call
// waiting.
import { directionFrom } from "../../lib/strategyDirection.js";

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
  const [packet, decisions, proposals, conditions, resolutions, ourDirection] =
    await Promise.all([
      readJson(fetch, "/strategy.json"),
      readJson(fetch, "/strategy/ratification-decisions/projection.json"),
      readJson(fetch, "/strategy/direction-proposals/projection.json"),
      readJson(fetch, "/strategy/conditions/projection.json"),
      readJson(fetch, "/strategy/proposal-resolutions/projection.json"),
      readJson(fetch, "/strategy/our-direction/projection.json")
    ]);

  return {
    board: { proposals, conditions, decisions, resolutions },
    ourDirection,
    fixture: directionFrom(packet, decisions),
    boundary:
      "cards move only by records - conditions, verdicts with evidence, decisions, and resolutions; deciding opens no authority by itself",
    safeNext: "open a card to see what would have to be true, or put a new direction on the table"
  };
}
