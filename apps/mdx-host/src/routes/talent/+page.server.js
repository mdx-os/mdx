// Talent: the roster is generated contract truth (complete without the
// kernel); worker presence is runtime truth read through the proxy,
// fail-soft to null so the surface invites instead of claiming.
import { talentRoster } from "../../lib/talentRoster.js";

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
  const floor = await readJson(fetch, "/talent/floor/projection.json");

  return {
    floor,
    roster: talentRoster(),
    boundary:
      "companions advise and never act; worker presence is read from receipts, and nothing on this page starts a worker",
    safeNext: "ask a companion something in Twin, or follow a build in Forge"
  };
}
