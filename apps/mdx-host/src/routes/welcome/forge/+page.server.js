// The guided Build path, reached from the one-time activation router. It gives
// engineers a short mental model for Forge before dropping them into the
// steady-state builder workspace.
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
  if (!router || router.setup_complete !== true) {
    throw redirect(307, "/welcome");
  }
  return {
    ownerName: router.owner_name ?? "",
    modelName: router.model_id ?? ""
  };
}
