import routes from "../../../../generated/routes/mdx-local-routes.json";
import modelGateway from "../../../../generated/architecture/harness-model-gateway-driver.json";
import providerGateway from "../../../../generated/architecture/harness-provider-gateway.json";
import modelContract from "../../../../generated/architecture/harness-model-contract.json";
import providerEvidence from "../../../../generated/live-substrates/provider-turn-on-evidence.json";
import modelFabric from "../../../../generated/architecture/model-fabric.json";

const modelRoutes = [
  "/v2/provider-turn-on-matrix.json",
  "/v2/provider-activation-request.json",
  "/v2/provider-turn-on-decision.json",
  "/v2/provider-turn-on-drill.json",
  "/v2/provider-connected-local.json",
  "/twin/model-gateway-runtime.json",
  "/twin/model-gateway-provider-observations/projection.json"
];

function routeByPath(path) {
  const route = (routes.routes ?? []).find((candidate) => candidate.local_path === path);
  return {
    method: route?.method ?? "GET",
    path,
    operation: route?.operation_id ?? "declared",
    receiptBacked: route?.receipt_backed === true
  };
}

function asArray(value) {
  return Array.isArray(value) ? value : [];
}

function packetStatus(packet, fallback = "waiting for kernel") {
  return packet?.status ?? fallback;
}

function modelProfiles(matrix) {
  const liveProfiles = asArray(matrix?.model_gateway_profiles);
  const generatedProfiles = asArray(modelGateway.vendor_slots).map((slot) => ({
    id: `${slot.driver_id}_model_gateway`,
    provider: slot.driver_id,
    live_adapter: slot.provider,
    status: slot.status,
    env_ready: false,
    required_receipt_kind:
      asArray(providerEvidence.receipts).find((receipt) => receipt.substrate === slot.driver_id)
        ?.receipt_kind ?? "provider.turn_on.receipt.required",
    stream_contract: slot.stream_contract,
    fallback_strategy: slot.fallback_strategy,
    failover_slo_ms: slot.failover_slo_ms,
    provider_call_allowed_now: false,
    live_provider_call_allowed_now: false,
    turn_on_allowed_now: false,
    credential_presence_only: true,
    credential_values_recorded: false,
    env_requirements: []
  }));
  return liveProfiles.length > 0 ? liveProfiles : generatedProfiles;
}

function activationRows(packet) {
  return asArray(packet?.activation_requests).map((request) => ({
    id: request.id,
    substrate: request.substrate,
    adapter: request.live_adapter,
    status: request.status,
    nextAction: request.next_action,
    receiptKind: request.required_receipt_kind,
    proof: request.proof_command,
    envNames: asArray(request.env_requirements).map((requirement) => ({
      name: requirement.name,
      present: requirement.present === true,
      purpose: requirement.purpose
    }))
  }));
}

function observationRows(packet) {
  return asArray(packet?.observations).map((observation) => ({
    provider: observation.provider_id ?? observation.provider ?? "provider",
    model: observation.model_id ?? "model",
    status: observation.status ?? "recorded",
    allowed: observation.provider_connected_local_allowed === true,
    receipt: observation.observation_receipt_id ?? observation.receipt_id ?? "receipt not loaded"
  }));
}

export function adminModels(packets = {}) {
  const matrix = packets["/v2/provider-turn-on-matrix.json"];
  const activation = packets["/v2/provider-activation-request.json"];
  const decision = packets["/v2/provider-turn-on-decision.json"];
  const drill = packets["/v2/provider-turn-on-drill.json"];
  const connected = packets["/v2/provider-connected-local.json"];
  const observations = packets["/twin/model-gateway-provider-observations/projection.json"];
  const liveConnections = asArray(packets["/models/connections.json"]?.connections);
  const readiness = packets["/models/readiness.json"] ?? {};
  const accessPolicy = packets["/models/access-policy.json"] ?? readiness.access_policy ?? {};
  const liveDeployments = asArray(packets["/models/deployments.json"]?.deployments);
  const livePolicies = asArray(packets["/models/policies.json"]?.policies);

  const profiles = modelProfiles(matrix);
  const acceptedObservations = observationRows(observations).filter((entry) => entry.allowed);
  const liveReadyCount =
    Number(matrix?.model_gateway_profiles_ready_for_live_call_count ?? 0) ||
    Number(connected?.ready_for_live_call_count ?? 0);

  return {
    fabric: {
      status: modelFabric.status,
      presets: asArray(modelFabric.presets).map((id) => ({
        id,
        label: id[0].toUpperCase() + id.slice(1),
        summary: {
          balanced: "Best overall fit for everyday work",
          quality: "Favor stronger outcomes over speed and price",
          fast: "Favor quick responses and lower cost",
          private: "Stay on an allowed local or sovereign deployment"
        }[id] ?? id
      })),
      providerCount: asArray(modelFabric.providers).length,
      providers: asArray(modelFabric.providers).map((provider) => ({
        ...provider,
        connectable: provider.wire_api !== "bedrock_converse"
      })),
      openModelProviderCount: asArray(modelFabric.providers).filter((row) => row.open_model_host).length,
      workloads: asArray(modelFabric.workloads),
      forgeStages: asArray(modelFabric.workloads).filter((row) => row.surface === "forge"),
      flagshipSurfaces: [...new Set(asArray(modelFabric.workloads).map((row) => row.surface))],
      authority: modelFabric.authority
    },
    summary: {
      providers: profiles.length,
      adapters: asArray(modelGateway.provider_adapter_registry).length,
      pendingLive: Number(matrix?.pending_live_count ?? asArray(modelGateway.vendor_slots).length),
      envReady: Number(matrix?.model_gateway_profiles_env_ready_count ?? 0),
      liveReady: liveReadyCount,
      acceptedObservations: acceptedObservations.length
    },
    connected: liveConnections,
    readiness: {
      ready: readiness.ready === true,
      status: readiness.status ?? "NEEDS_CONNECTION",
      provider: readiness.provider_id ?? "",
      model: readiness.model_id ?? "",
      readyConnections: Number(readiness.ready_connection_count ?? 0),
      readyDeployments: Number(readiness.ready_deployment_count ?? 0),
      readySurfaces: Number(readiness.ready_surface_count ?? 0),
      surfaceCount: Number(readiness.surface_count ?? 0),
      workloads: asArray(readiness.consumer_readiness).map((workload) => ({
        consumer: String(workload?.consumer ?? ""),
        workloadId: String(workload?.workload_id ?? ""),
        stage: String(workload?.stage ?? ""),
        ready: workload?.ready === true,
        blockReason: String(workload?.block_reason ?? ""),
        nextAction: String(workload?.next_action ?? "")
      })),
      nextAction: readiness.recommended_next_action ?? "Connect a model."
    },
    accessPolicy: {
      modes: asArray(accessPolicy.allowed_connection_modes),
      providers: asArray(accessPolicy.allowed_providers),
      maxLiveTestMicrousd: Number(accessPolicy.max_live_test_microusd ?? 100000),
      credentialBackends: asArray(accessPolicy.credential_backends)
    },
    deployments: liveDeployments,
    policies: livePolicies,
    defaultGateway: {
      provider: modelGateway.local_default.provider,
      model: modelGateway.local_default.model_id,
      status: modelGateway.local_default.status,
      routing: modelGateway.local_default.routing_strategy,
      stream: modelGateway.local_default.stream_contract,
      networkAllowed: modelGateway.local_default.network_allowed,
      providerSecretsRequired: modelGateway.local_default.provider_secrets_required,
      fallback: modelGateway.local_default.fallback_strategy
    },
    providers: profiles.map((profile) => ({
      id: profile.id,
      provider: profile.provider ?? profile.substrate,
      adapter: profile.live_adapter,
      status: profile.status,
      envReady: profile.env_ready === true,
      receiptKind: profile.required_receipt_kind,
      stream: profile.stream_contract,
      fallback: profile.fallback_strategy,
      failoverMs: profile.failover_slo_ms,
      providerCallAllowed: profile.provider_call_allowed_now === true,
      liveProviderCallAllowed: profile.live_provider_call_allowed_now === true,
      turnOnAllowed: profile.turn_on_allowed_now === true,
      credentialPresenceOnly: profile.credential_presence_only !== false,
      credentialValuesRecorded: profile.credential_values_recorded === true,
      envNames: asArray(profile.env_requirements).map((requirement) => ({
        name: requirement.name,
        present: requirement.present === true,
        purpose: requirement.purpose
      }))
    })),
    turnOn: {
      matrixStatus: packetStatus(matrix, modelGateway.status),
      activationStatus: packetStatus(activation),
      decisionStatus: packetStatus(decision),
      drillStatus: packetStatus(drill),
      connectedStatus: packetStatus(connected),
      recommendedCandidate: decision?.recommended_first_candidate_id ?? "none",
      recommendedAction: decision?.recommended_first_action ?? "load local kernel",
      selectedCandidate: drill?.selected_candidate_id ?? "none",
      selectedSubstrate: drill?.selected_substrate ?? "none",
      providerCallAllowed:
        decision?.provider_call_allowed_now === true || connected?.provider_call_allowed_now === true,
      liveProviderCallAllowed:
        decision?.live_provider_call_allowed_now === true ||
        connected?.live_provider_call_allowed_now === true,
      liveNetworkAllowed: connected?.live_network_allowed_now === true,
      activationRows: activationRows(activation)
    },
    observations: {
      status: packetStatus(observations),
      accepted: acceptedObservations,
      route: "/twin/model-gateway-provider-observations/projection.json",
      receiptKind: providerGateway.turn_on_bridge.accepted_receipt_kind
    },
    contracts: {
      runtimePrimitive: providerGateway.runtime_primitive,
      gatewayStatus: providerGateway.status,
      liveProviderCallsAllowed: providerGateway.live_provider_calls_allowed,
      providerAdapterRegistry: providerGateway.provider_adapter_registry,
      requiredProfileFields: providerGateway.required_profile_fields,
      localReceiptKinds: providerGateway.local_receipt_kinds,
      policyRules: providerGateway.policy_rules,
      forbiddenCapabilities: providerGateway.forbidden_capabilities,
      modelRules: modelContract.rules,
      modelZones: modelContract.zones,
      requiredReceiptPayload: modelGateway.required_receipt_payload
    },
    routes: modelRoutes.map(routeByPath),
    sources: [
      { label: "Routes", value: "generated/routes/mdx-local-routes.json" },
      { label: "Model gateway", value: "generated/architecture/harness-model-gateway-driver.json" },
      { label: "Provider gateway", value: "generated/architecture/harness-provider-gateway.json" },
      { label: "Model contract", value: "generated/architecture/harness-model-contract.json" },
      { label: "Turn-on evidence", value: "generated/live-substrates/provider-turn-on-evidence.json" }
    ]
  };
}
