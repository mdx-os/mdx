import routes from "../../../../generated/routes/mdx-local-routes.json";
import fleetHarness from "../../../../generated/architecture/forge-fleet-eval-harness.json";

const forgeRoutes = [
  "/forge/machine-league/recommendations.json",
  "/forge/machine-league/preflight.json",
  "/forge/machine-league/closed-runner-clearances.json",
  "/forge/machine-league/closed-runner-clearances/projection.json",
  "/forge/machine-league/runtime-smokes.json",
  "/forge/fleet-eval-scoreboard.json"
];

function asArray(value) {
  return Array.isArray(value) ? value : [];
}

function routeByPath(path) {
  const route = (routes.routes ?? []).find((candidate) => candidate.local_path === path);
  return {
    method: route?.method ?? "GET",
    path,
    operation: route?.operation_id ?? "declared",
    receiptBacked: route?.receipt_backed === true
  };
}

function byRunner(rows, key = "runner_id") {
  return Object.fromEntries(asArray(rows).map((row) => [String(row?.[key] ?? ""), row]));
}

function titleize(value) {
  return String(value ?? "")
    .replace(/_/g, " ")
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

function humanModel(value) {
  const model = String(value ?? "");
  const map = {
    "grok-build-cli-selected-model": "Grok Build with Grok 4.5",
    "claude-code-selected-model": "Claude Agent SDK selected model",
    "codex-cli-selected-model": "Codex CLI selected model",
    "gemini-cli-selected-model": "Gemini CLI selected model",
    "opencode-selected-model": "OpenCode selected model",
    "cline-selected-model": "Cline selected model",
    "goose-selected-model": "Goose selected model",
    "mdx-native-builder": "Forge Native Builder",
    "mdx_native_local_model_profile": "Forge Native Builder"
  };
  return map[model] ?? titleize(model || "model selected by adapter");
}

function protocolLabel(adapterKind, tier) {
  const adapter = String(adapterKind ?? "");
  if (adapter === "grok_build_cli") return "ACP";
  if (adapter === "claude_code") return "Agents SDK";
  if (adapter === "mdx_native") return "Native";
  if (String(tier ?? "").includes("protocol")) return "Protocol adapter";
  if (String(tier ?? "").includes("sdk")) return "SDK adapter";
  return "CLI";
}

function modeLabel(mode) {
  if (mode === "fleet_eval") return "Fleet eval";
  if (mode === "local_companion") return "Local companion";
  return "Not cleared";
}

function readinessLine(machine) {
  if (machine.native) return "Built in. It is always available as the house builder.";
  if (machine.liveExecutionReady) {
    return "Installed and env-enabled here. Still quarantined: output must pass Forge gates before it counts.";
  }
  if (machine.optionEnabled && machine.binaryPresent) {
    return "Available as an option. Execution stays off until the local env gate is deliberately set.";
  }
  if (machine.optionEnabled) {
    return "Admin credentials are present enough to show this as an option, but the local binary is not ready.";
  }
  if (machine.requiresClearance && machine.clearanceMode) {
    return "Cleared by an operator. It still needs runtime readiness before Forge can cast it.";
  }
  if (machine.requiresClearance) {
    return "Supported, but held until an admin attests that this signed-in tool may be used here.";
  }
  if (!machine.binaryPresent) {
    return "Supported, but the local runner is not installed or not visible on this machine.";
  }
  return "Supported. Run a smoke check before relying on this runtime.";
}

function actionLine(machine) {
  if (machine.native) return "No turn-on control is needed for the native builder.";
  if (machine.requiresClearance && !machine.clearanceMode) {
    return "Start by clearing the machine as a local companion or fleet-eval participant.";
  }
  if (machine.smokePassed) return "Runtime smoke has passed for the recorded fingerprint.";
  if (machine.binaryPresent) return "Run runtime smoke to record version-only compatibility evidence.";
  return "Install or expose the runner locally, then run preflight again.";
}

function machineRows(recommendation, preflight, scoreboard, clearances) {
  const runners = asArray(recommendation?.runner_profiles);
  const checksByRunner = byRunner(preflight?.checks);
  const scoresByRunner = byRunner(scoreboard?.runner_scores);
  const clearanceByRunner = byRunner(clearances?.clearances);

  return runners.map((runner) => {
    const id = String(runner.runner_id ?? "");
    const check = checksByRunner[id] ?? {};
    const score = scoresByRunner[id] ?? {};
    const clearance = clearanceByRunner[id] ?? {};
    const adapterKind = String(runner.adapter_kind ?? check.adapter_kind ?? score.adapter_kind ?? "");
    const native = String(runner.runner_kind ?? "") === "mdx_native" || adapterKind === "mdx_native";
    const requiresClearance = adapterKind === "grok_build_cli" || adapterKind === "claude_code";
    const runnerStatus = String(runner.execution_status ?? "");
    const status = String(check.status ?? runnerStatus ?? "waiting_for_preflight");
    const clearanceMode = String(clearance.clearance_mode ?? "");
    const binaryPresent = check.binary_present === true;
    const openRunnerOption =
      !native &&
      !requiresClearance &&
      (runnerStatus.startsWith("PROFILE_ADMITTED") || runnerStatus.startsWith("PROFILE_DECLARED"));
    const optionEnabled =
      native || check.machine_option_enabled === true || status.includes("option_ready") || openRunnerOption;
    const liveExecutionReady = status === "live_execution_ready";
    const smokePassed = score.runtime_smoke_passed === true || String(score.runtime_smoke_status ?? "") === "runtime_smoke_passed";
    const machine = {
      id,
      name: String(runner.display_name ?? check.display_name ?? score.display_name ?? id),
      native,
      adapterKind,
      protocol: protocolLabel(adapterKind, clearance.adapter_protocol_tier),
      model: humanModel(score.model_profile_id ?? runner.model_profile_id ?? score.model),
      supported: true,
      requiresClearance,
      clearable: requiresClearance,
      clearanceMode,
      clearanceLabel: native ? "Built in" : modeLabel(clearanceMode),
      scorecardEligible: clearance.scorecard_eligible === true,
      status,
      statusLabel: titleize(status),
      binaryName: native ? "built in" : String(check.binary_name ?? ""),
      binaryPresent: native || binaryPresent,
      versionObserved: String(check.version_observed ?? score.runtime_version_observed ?? ""),
      optionEnabled,
      optionEnablement: titleize(check.machine_option_enablement ?? ""),
      liveExecutionReady,
      liveEnvRequired: String(check.live_execution_env_required ?? ""),
      smokeStatus: native ? "runtime_smoke_not_required" : String(score.runtime_smoke_status ?? "runtime_smoke_not_run"),
      smokePassed,
      tasksAttempted: Number(score.tasks_attempted ?? 0),
      accepted: Number(score.accepted ?? 0),
      receipt: String(clearance.clearance_receipt_id ?? score.runtime_last_observed_receipt_id ?? "")
    };
    return {
      ...machine,
      readinessLine: readinessLine(machine),
      actionLine: actionLine(machine)
    };
  });
}

export function adminForge(packets = {}) {
  const recommendation = packets["/forge/machine-league/recommendations.json"];
  const preflight = packets["/forge/machine-league/preflight.json"];
  const scoreboard = packets["/forge/fleet-eval-scoreboard.json"];
  const clearances = packets["/forge/machine-league/closed-runner-clearances/projection.json"];
  const machines = machineRows(recommendation, preflight, scoreboard, clearances);
  const externalMachines = machines.filter((machine) => !machine.native);

  return {
    summary: {
      machines: machines.length,
      external: externalMachines.length,
      optionEnabled: externalMachines.filter((machine) => machine.optionEnabled).length,
      liveReady: externalMachines.filter((machine) => machine.liveExecutionReady).length,
      cleared: externalMachines.filter((machine) => machine.clearanceMode).length,
      smokePassed: externalMachines.filter((machine) => machine.smokePassed).length
    },
    machines,
    boundary: {
      line:
        "Clearing a machine makes it governable here. It does not grant production writes, consume external output, export secrets, or bypass Forge checks.",
      policy: fleetHarness.closed_runner_clearance_policy,
      runtimePolicy:
        "Runtime smoke records binary/version compatibility only. Adapter execution and task execution remain separately gated."
    },
    routes: forgeRoutes.map(routeByPath),
    sources: [
      { label: "Machine League", value: "generated/architecture/forge-fleet-eval-harness.json" },
      { label: "Routes", value: "generated/routes/mdx-local-routes.json" },
      { label: "Scoreboard", value: "generated/response-schemas/forge-fleet-eval-scoreboard-response.schema.json" }
    ],
    status: recommendation ? "ready" : "waiting_for_kernel"
  };
}
