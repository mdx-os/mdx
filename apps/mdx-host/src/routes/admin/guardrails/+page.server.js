import { adminGuardrails } from "../../../lib/adminGuardrails.js";

const projectionRoutes = [
  "/auth/tenant-policy-preflights/projection.json",
  "/auth/session-controls/projection.json",
  "/twin/provider-call-refusals/projection.json",
  "/twin/production-memory-refusals/projection.json",
  "/twin/tool-execution-refusals/projection.json",
  "/messages/service-role-fanout-refusals/projection.json",
  "/pages/embedding-provider-refusals/projection.json"
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
  const packets = await Promise.all(projectionRoutes.map((route) => readJson(fetch, route)));
  const projections = Object.fromEntries(projectionRoutes.map((route, index) => [route, packets[index]]));

  return {
    guardrails: adminGuardrails(projections),
    boundary:
      "guardrails is read-only; it shows generated policy and live refusal projections without opening a mutation path",
    safeNext:
      "keep refusals visible here while the unified Gate is designed around session authority, policy, and receipts"
  };
}
