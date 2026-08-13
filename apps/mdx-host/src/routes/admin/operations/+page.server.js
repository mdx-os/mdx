import { adminOperations } from "../../../lib/adminOperations.js";

const packetRoutes = [
  "/status.json",
  "/runtime/watchtower.json",
  "/runtime/metrics.json",
  "/runtime/http-metrics.json",
  "/runtime/load-proof-summary.json",
  "/runtime/queue-projection.json",
  "/runtime/operator-packet.json",
  "/local/evidence-index.json",
  "/local/activation-summary.json",
  "/local/activation-report.json",
  "/receipts.json",
  "/api/telemetry"
];

async function readJson(fetchImpl, path) {
  try {
    const target = path.startsWith("/api/") ? path : `/api/kernel${path}`;
    const response = await fetchImpl(target, {
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
    operations: adminOperations(packetByRoute),
    boundary:
      "operations is read-only; it summarizes status, evidence, and blockers without opening runtime or provider authority",
    safeNext:
      "use Operations to decide what proof to refresh before moving a live substrate or the unified Gate"
  };
}
