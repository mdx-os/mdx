// The guided "Try MDx" first experience, reached from the activation router.
// It only makes sense once setup is complete, so if it is not we send the
// operator back to /welcome to finish. If they have already done the guided
// start we still let them through (it can be redone) - the front door, not this
// page, decides the one-time routing.
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
