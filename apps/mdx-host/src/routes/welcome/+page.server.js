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
  // Two projections describe setup state and they can disagree (the router
  // says complete while activation lags mid-mission). A user either source
  // considers done must NEVER be thrown back into activation - honor the OR
  // of the two, the same truth the front door routes on.
  const [activation, router] = await Promise.all([
    readJson(fetch, "/activation/projection.json"),
    readJson(fetch, "/install/setup-router.json")
  ]);

  if (activation?.status === "READY" || (router?.setup_complete === true && router?.track_chosen === true)) {
    throw redirect(307, "/twin");
  }

  const completeSteps = Number(activation?.progress?.complete_steps ?? 0);
  if (completeSteps > 0) {
    throw redirect(307, "/welcome/setup");
  }

  throw redirect(307, "/welcome/meet");
}
