import { adminModels } from "../../../lib/adminModels.js";

const packetRoutes = [
  "/v2/provider-turn-on-matrix.json",
  "/v2/provider-activation-request.json",
  "/v2/provider-turn-on-decision.json",
  "/v2/provider-turn-on-drill.json",
  "/v2/provider-connected-local.json",
  "/twin/model-gateway-runtime.json",
  "/twin/model-gateway-provider-observations/projection.json",
  "/models/fabric.json",
  "/models/readiness.json",
  "/models/access-policy.json",
  "/models/connections.json",
  "/models/deployments.json",
  "/models/policies.json"
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
    models: adminModels(packetByRoute),
    boundary: "Model selection never grants permission to use tools, write, spend, or ship.",
    safeNext: "Connect a model, then let each app ask for a workload instead of naming that model directly."
  };
}

export const actions = {
  connect: async ({ request, fetch }) => {
    const form = await request.formData();
    const providerId = String(form.get("provider_id") ?? "").trim();
    const modelId = String(form.get("model_id") ?? "").trim();
    const apiKey = String(form.get("api_key") ?? "").trim();
    const endpointBaseUrl = String(form.get("endpoint_base_url") ?? "").trim();
    if (!providerId) {
      return { ok: false, line: "Choose a provider." };
    }
    const mode = String(form.get("connection_mode") ?? "byok");
    const provider = adminModels({}).fabric.providers.find((row) => row.provider_id === providerId);
    const providerCredentialEnv = provider?.credential_env ?? `${providerId.toUpperCase()}_API_KEY`;
    const credentialRefInput = String(form.get("credential_ref") ?? "").trim();
    const credentialRef = mode === "managed"
      ? `secret:managed/${providerCredentialEnv}`
      : mode === "enterprise"
        ? credentialRefInput || `secret:environment/${providerCredentialEnv}`
        : undefined;
    if (mode === "byok" && !apiKey) {
      return { ok: false, line: "Enter the provider key you want MDx to verify." };
    }
    try {
      const response = await fetch("/api/kernel/models/connect.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          provider_id: providerId,
          model_id: modelId || undefined,
          api_key: apiKey,
          credential_ref: credentialRef,
          persist_key: form.get("persist_key") === "on",
          endpoint_base_url: endpointBaseUrl || undefined,
          tools: form.get("tools") === "on",
          structured_output: form.get("structured_output") === "on",
          vision: form.get("vision") === "on",
          paid_service_data_terms: form.get("paid_service_data_terms") === "on",
          privacy_controls_attested: form.get("privacy_controls_attested") === "on",
          context_tokens: Number(form.get("context_tokens") || 0)
        })
      });
      const packet = await response.json().catch(() => ({}));
      if (response.ok && packet.status === "MODEL_CONNECTED") {
        const credentialNote = packet.credential_source === "local_keychain"
          ? "The key is saved in your local Keychain."
          : ["env", "mounted", "managed", "broker"].includes(packet.credential_source)
            ? `The ${packet.credential_source} credential reference is active and its value stays outside MDx records.`
          : packet.credential_source === "session"
            ? "The key is available for this session only."
            : "No provider key was required.";
        return { ok: true, line: `${providerId} ${packet.model_id || modelId} is connected and available to eligible workloads. ${credentialNote}` };
      }
      return { ok: false, line: packet.reason || "The model did not pass its live connection check." };
    } catch (error) {
      return { ok: false, line: "MDx could not complete the connection check." };
    }
  },
  test: async ({ request, fetch }) => {
    const form = await request.formData();
    const deploymentId = String(form.get("deployment_id") ?? "").trim();
    const maxCost = Math.max(1, Math.min(100000, Number(form.get("max_cost_microusd") || 1000)));
    try {
      const response = await fetch("/api/kernel/models/test.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          allow_live: true,
          max_cost_microusd: maxCost,
          // OpenAI Responses requires at least 16 output tokens. The request
          // remains inside the 1,000 micro-USD probe ceiling at the
          // conservative long-context rate recorded by the kernel.
          max_output_tokens: 16,
          workload_id: "mdx/pages/compose",
          pinned_deployment_id: deploymentId || undefined
        })
      });
      const packet = await response.json().catch(() => ({}));
      if (response.ok && packet.status === "MODEL_INFERENCE_COMPLETED") {
        return {
          ok: true,
          line: `${packet.model_id} answered in ${packet.latency_ms} ms for ${packet.cost_microusd} micro-USD. Decision and outcome receipts were recorded.`
        };
      }
      return { ok: false, line: packet.reason || packet.failure_classes?.join(", ") || "The budget-capped live test did not complete." };
    } catch (error) {
      return { ok: false, line: "The local kernel could not run the budget-capped model test." };
    }
  },
  disconnect: async ({ request, fetch }) => {
    const form = await request.formData();
    const providerId = String(form.get("provider_id") ?? "").trim();
    const action = String(form.get("connection_action") ?? "disconnect") === "revoke" ? "revoke" : "disconnect";
    if (!providerId) return { ok: false, line: "Choose a model connection." };
    try {
      const response = await fetch("/api/kernel/models/connections.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ provider_id: providerId, action })
      });
      const packet = await response.json().catch(() => ({}));
      if (response.ok && ["MODEL_CONNECTION_DISCONNECTED", "MODEL_CONNECTION_REVOKED"].includes(packet.status)) {
        return {
          ok: true,
          line: action === "revoke"
            ? `${providerId} was revoked immediately. Any MDx-owned local credential was forgotten; externally managed secrets remain under operator control.`
            : `${providerId} was disconnected immediately. Its credential was left in its configured secret store.`
        };
      }
      return { ok: false, line: packet.reason || `MDx could not ${action} that model connection.` };
    } catch (error) {
      return { ok: false, line: `The local kernel could not ${action} that model connection.` };
    }
  },
  assign: async ({ request, fetch }) => {
    const form = await request.formData();
    const workloadId = String(form.get("workload_id") ?? "").trim();
    const deploymentId = String(form.get("deployment_id") ?? "").trim();
    const workload = modelContractFor(workloadId);
    if (!workload) {
      return { ok: false, line: "Choose a declared kind of work." };
    }
    const policyId = `model-center-${workloadId.replaceAll("/", "-")}`;
    try {
      const response = await fetch("/api/kernel/models/policies.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          policy_id: policyId,
          policy_version: `v${Date.now()}`,
          lifecycle: "static",
          workload_ids: [workloadId],
          allowed_app_ids: [workload.surface],
          preferred_deployment_ids: deploymentId ? [deploymentId] : []
        })
      });
      const packet = await response.json().catch(() => ({}));
      if (response.ok && packet.status === "MODEL_ROUTE_POLICY_CONFIGURED") {
        return {
          ok: true,
          line: deploymentId
            ? `${workload.stage.replaceAll("_", " ")} now prefers that model and keeps eligible fallbacks.`
            : `${workload.stage.replaceAll("_", " ")} is back on automatic routing.`
        };
      }
      return { ok: false, line: packet.reason || "MDx could not save that work assignment." };
    } catch (error) {
      return { ok: false, line: "The local kernel could not save that work assignment." };
    }
  }
};

function modelContractFor(workloadId) {
  const row = adminModels({}).fabric.workloads.find((workload) => workload.workload_id === workloadId);
  return row ?? null;
}
