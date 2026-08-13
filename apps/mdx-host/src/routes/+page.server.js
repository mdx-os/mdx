// The front door. Where a visitor lands depends on how far setup has gotten:
//   - not set up (no owner, or no model connected) -> /welcome to finish it.
//   - set up but no path chosen yet -> /welcome/path, the one-time chooser.
//   - set up and a path already chosen -> /twin, the home surface.
// "Set up" means the ceremony is done: this install has been claimed by an owner
// AND a model is connected (an old provider receipt alone must not skip the
// claim, so both are required). The chooser is shown exactly once because its
// trigger is receipt-derived: once a setup.track_chosen receipt exists, the
// front door routes to Twin and never shows it again. If the kernel is
// unreachable we send to welcome. Fail-soft, never a fake "ready".
import { redirect } from "@sveltejs/kit";
import { publicRootRedirect } from "../lib/publicRoot.server.js";

async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      signal: AbortSignal.timeout(1200)
    });
    return response.ok ? await response.json() : null;
  } catch (error) {
    return null;
  }
}

export async function load({ fetch }) {
  const hostedBetaRedirect = publicRootRedirect();
  if (hostedBetaRedirect) throw redirect(307, hostedBetaRedirect);
  // Model-connected is read from the receipt-backed runtime route (derived from
  // the governed observation receipt), not the generated proof file. The router
  // packet carries owner + model + whether a track has been chosen.
  const router = await readJson(fetch, "/install/setup-router.json");
  const setUp = router?.setup_complete === true;
  if (!setUp) {
    throw redirect(307, "/welcome");
  }
  const trackChosen = router?.track_chosen === true;
  throw redirect(307, trackChosen ? "/twin" : "/welcome/path");
}
