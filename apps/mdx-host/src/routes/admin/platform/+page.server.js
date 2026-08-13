// Platform reads runtime truth through the kernel proxy server-side,
// fail-soft: kernel down means the page invites instead of claiming.
// This is the surface that lights up when cloud activation opens rails -
// the statuses drive the rendering, so turn-ons appear with zero UI work.
import { candidatesFrom, platformPulse, substratesFrom } from "../../../lib/platformRails.js";

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
  const [status, decision] = await Promise.all([
    readJson(fetch, "/status.json"),
    readJson(fetch, "/v2/provider-turn-on-decision.json")
  ]);

  const substrates = substratesFrom(status);
  return {
    substrates,
    candidates: candidatesFrom(decision),
    pulse: platformPulse(substrates),
    boundary:
      "this page reads and never switches anything on; every turn-on happens at the terminal with its own recorded approval and counts only when its live proof is observed",
    safeNext: "run make local-smoke to refresh local proof, or follow Setup when you are ready to point a rail at the cloud"
  };
}
