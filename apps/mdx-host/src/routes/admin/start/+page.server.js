// The admin overview reads three kernel packets server-side through the
// proxy and turns them into one guided journey. Each fetch degrades to null
// on its own when the kernel is not answering, so the page arrives complete
// and honest: a down kernel renders step one as an invitation with zero
// claims. This load never adds readiness the packets did not report.
import { buildJourney, journeyPulse } from "../../../lib/adminJourney.js";

async function readPacket(fetch, route) {
  try {
    const response = await fetch(`/api/kernel${route}`, {
      signal: AbortSignal.timeout(1500)
    });
    if (response.ok) {
      return await response.json();
    }
  } catch (error) {
    return null;
  }
  return null;
}

export async function load({ fetch }) {
  const [readiness, onboarding, decision] = await Promise.all([
    readPacket(fetch, "/v2/fresh-install-readiness.json"),
    readPacket(fetch, "/v2/alpha-onboarding.json"),
    readPacket(fetch, "/v2/provider-turn-on-decision.json")
  ]);

  const journey = buildJourney({ readiness, onboarding, decision });

  return {
    journey,
    pulse: journeyPulse(journey),
    sources: journey.kernelUp
      ? [
          { label: "Step 1", route: "/v2/fresh-install-readiness.json" },
          { label: "Step 2", route: "/v2/alpha-onboarding.json" },
          { label: "Steps 3-6", route: "/v2/provider-turn-on-decision.json" }
        ]
      : [],
    boundary:
      "nothing in the admin plane acts without a recorded approval, and the product plane never shows setup internals",
    safeNext:
      "do the one step you're on - everything after it stays dimmed until the kernel says it's open"
  };
}
