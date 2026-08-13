// The activation router: the one-time "What brings you here?" chooser, shown
// after the install is claimed and a model is connected, before a path is
// chosen. It reads the lean router projection (owner, model, runtime, tracks) -
// never composing a full readiness report. If setup is not actually complete we
// send the operator back to /welcome to finish it; we never show the chooser on
// a half-set-up install.
import { redirect } from "@sveltejs/kit";

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
  const router = await readJson(fetch, "/install/setup-router.json");
  // Kernel unreachable or setup not finished: the chooser has no business
  // showing yet. Back to welcome, which explains how to finish setup.
  if (!router || router.setup_complete !== true) {
    throw redirect(307, "/welcome");
  }
  return {
    ownerName: router.owner_name ?? "",
    modelName: router.model_id ?? "",
    trackChosen: router.track_chosen === true,
    chosenTrack: router.chosen_track ?? "",
    tracks: router.tracks ?? []
  };
}
