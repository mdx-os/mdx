import { adminEvidence } from "../../../lib/adminEvidence.js";

const packetRoutes = [
  "/receipts.json",
  "/audit/evidence-checkpoints/projection.json",
  "/local/evidence-index.json"
];

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
  const packets = await Promise.all(packetRoutes.map((route) => readJson(fetch, route)));
  const packetByRoute = Object.fromEntries(packetRoutes.map((route, index) => [route, packets[index]]));

  return {
    evidence: adminEvidence(packetByRoute),
    boundary:
      "evidence is read-only; it browses receipts, checkpoints, and chain contracts without signing or anchoring anything",
    safeNext:
      "use Evidence to inspect the receipt chain before opening a Gate approval or refreshing an audit checkpoint"
  };
}
