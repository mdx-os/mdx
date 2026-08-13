import { adminKnowledge } from "../../../lib/adminKnowledge.js";

const packetRoutes = [
  "/pages.json",
  "/pages/runtime-readiness.json",
  "/pages/lifecycle/projection.json",
  "/pages/agent-citations.json",
  "/pages/search-preflights/projection.json",
  "/pages/embedding-provider-refusals/projection.json",
  "/pages/world-model/projection.json",
  "/pages/stewardship-changes/projection.json"
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
    knowledge: adminKnowledge(packetByRoute),
    boundary:
      "Knowledge is read-only here; drafts, approvals, search preflights, and publication changes stay behind Pages receipts and the Gate",
    safeNext:
      "use Knowledge to inspect citable pages, gaps, and learning hooks before asking Gate for a receipt-backed Pages change"
  };
}
