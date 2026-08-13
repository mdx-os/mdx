import { adminForge } from "../../../lib/adminForge.js";

const packetRoutes = [
  "/forge/machine-league/recommendations.json",
  "/forge/machine-league/preflight.json",
  "/forge/fleet-eval-scoreboard.json",
  "/forge/machine-league/closed-runner-clearances/projection.json"
];

async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      signal: AbortSignal.timeout(path.includes("preflight") ? 8000 : 1500)
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
    forge: adminForge(packetByRoute),
    safeNext:
      "clear only machines this install is allowed to use, smoke the local runtime, then return to Forge Machines for operator trials"
  };
}

function clean(value) {
  return String(value ?? "").replace(/_/g, " ");
}

export const actions = {
  default: async ({ request, fetch }) => {
    const form = await request.formData();
    const intent = String(form.get("intent") ?? "");
    const runnerId = String(form.get("runner_id") ?? "");

    try {
      if (intent === "preflight") {
        const response = await fetch("/api/kernel/forge/machine-league/preflight.json", {
          signal: AbortSignal.timeout(20000)
        });
        return response.ok
          ? { ok: true, line: "Preflight refreshed. The table now reflects the local binaries and env gates Forge can see." }
          : { ok: false, line: "Preflight did not return cleanly." };
      }

      if (intent === "clear") {
        const mode = String(form.get("clearance_mode") ?? "local_companion");
        const response = await fetch("/api/kernel/forge/machine-league/closed-runner-clearances.json", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            runner_id: runnerId,
            clearance_mode: mode,
            operator_attestation: true
          })
        });
        const packet = await response.json();
        if (packet.status === "CLOSED_RUNNER_CLEARANCE_RECORDED") {
          return {
            ok: true,
            line:
              mode === "fleet_eval"
                ? `${packet.runner_display_name} is cleared for fleet eval. Execution, output use, and production writes are still gated.`
                : `${packet.runner_display_name} is cleared as a local companion. It stays out of the scorecard.`
          };
        }
        return { ok: false, line: packet.reason ? clean(packet.reason) : "Forge refused the clearance." };
      }

      if (intent === "smoke") {
        const response = await fetch("/api/kernel/forge/machine-league/runtime-smokes.json", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ runner_id: runnerId })
        });
        const packet = await response.json();
        if (packet.machine_runtime_smoke_passed === true) {
          return {
            ok: true,
            line: `${packet.runner_display_name} passed runtime smoke. Forge recorded version-only compatibility evidence.`
          };
        }
        return {
          ok: false,
          line:
            packet.status === "REFUSED"
              ? clean(packet.reason)
              : `${packet.runner_display_name ?? "That machine"} is still held. The local binary is missing or not compatible enough yet.`
        };
      }
    } catch (error) {
      if (intent === "preflight") return { ok: false, line: "Preflight could not reach the local kernel." };
      if (intent === "smoke") return { ok: false, line: "Runtime smoke did not land." };
      return { ok: false, line: "The clearance did not land." };
    }

    return { ok: false, line: "That admin action is not available." };
  }
};
