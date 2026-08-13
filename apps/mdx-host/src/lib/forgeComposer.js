const DEFAULT_DIRECT_CANDIDATE_WIDTH = 4;
const DEFAULT_FLEET_LADDER_CEILING = 48;

export function resolveExecutionTarget({ mode = "auto", cloudEnvironment = null } = {}) {
  const selectedMode = ["auto", "local", "cloud"].includes(mode) ? mode : "auto";
  const cloudReady = cloudEnvironment?.ready_for_cloud_builds === true && Boolean(cloudEnvironment?.environment_id);
  const blocked = selectedMode === "cloud" && !cloudReady;
  const useCloud = cloudReady && selectedMode !== "local";
  return {
    mode: selectedMode,
    backend: useCloud ? "hosted_sandbox" : "local",
    cloudEnvironmentId: useCloud ? String(cloudEnvironment.environment_id) : "",
    blocked,
    label: selectedMode === "auto" ? `Auto - ${useCloud ? "cloud" : "current host"}` : selectedMode === "cloud" ? "Cloud" : "Current host",
    line: blocked
      ? "Cloud needs a verified environment for this repo before Forge can start."
      : useCloud
        ? "Forge will run in the repo's verified isolated cloud environment."
        : "Forge will run on the MDx host that accepted this session."
  };
}

export function buildExecutionWidthOptions({
  workerCapacity = 0,
  queueDepthLimit = 0,
  directCandidateWidth = DEFAULT_DIRECT_CANDIDATE_WIDTH,
  fleetLadderCeiling = DEFAULT_FLEET_LADDER_CEILING
} = {}) {
  const workers = Math.max(0, Number(workerCapacity) || 0);
  const queueLimit = Math.max(0, Number(queueDepthLimit) || 0);
  const fallback = 16;
  const admittedWidth = workers > 0
    ? workers + queueLimit
    : fallback;
  const ceiling = Math.max(
    1,
    Math.min(Math.max(directCandidateWidth, Number(fleetLadderCeiling) || 0), admittedWidth)
  );
  const values = [1, directCandidateWidth, 8, 16, 24, 48, ceiling]
    .filter((value) => value >= 1 && value <= ceiling)
    .filter((value, index, rows) => rows.indexOf(value) === index)
    .sort((left, right) => left - right);

  return values.map((value) => ({
    value,
    label: `${value}`,
    line:
      value === 1
        ? "solo run"
        : value <= directCandidateWidth
          ? "parallel candidates"
          : "governed fleet"
  }));
}

export function buildProofPreflightView({
  isExternalRepo,
  proofPreflight,
  selectedCloudEnvironment,
  selectedChecks = ""
} = {}) {
  if (!isExternalRepo) return null;

  if (selectedCloudEnvironment?.ready_for_cloud_builds === true) {
    const firstCheck = String(selectedChecks)
      .split("\n")
      .map((line) => line.trim())
      .find(Boolean);
    return {
      tone: "ready",
      label: "Cloud proof ready",
      command: firstCheck || "Verified cloud environment",
      line: "The selected AgentCore environment owns toolchain readiness and runs checks outside the Render host.",
      missing: []
    };
  }

  if (!proofPreflight) return null;
  const status = String(proofPreflight.status ?? "");
  const command = String(proofPreflight.selected_command ?? "").trim();
  const missing = Array.isArray(proofPreflight.missing_readiness)
    ? proofPreflight.missing_readiness.map(String).filter(Boolean)
    : [];
  const ready = proofPreflight.ready === true || status === "READY";
  const setup = status === "SETUP_REQUIRED";
  const line = String(proofPreflight.safe_next_move ?? "").trim();
  return {
    tone: ready ? "ready" : setup ? "setup" : "check",
    label: ready ? "Proof ready" : setup ? "Setup needed" : "Check needed",
    command: command || "Choose a check",
    line: line || (ready ? "Start with this check and review proof coverage afterward." : "Choose a proof command before starting."),
    missing
  };
}
