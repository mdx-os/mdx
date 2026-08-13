const FAST_TIMEOUT_MS = 15_000;
const REASONING_TIMEOUT_MS = 120_000;
const HOSTED_ENVIRONMENT_TIMEOUT_MS = 30 * 60 * 1000;
const REASONING_ROUTES = [
  "/forge/fleet-plans",
  "/forge/work-classifications",
  "/forge/long-horizon-mission",
  "/forge/fleet-runs"
];

export function timeoutForPath(path) {
  // Hosted verification runs repository-defined setup and proof commands in
  // the isolated runtime before it can record a VERIFIED receipt. Keep this
  // exception exact so an ordinary kernel write cannot inherit the long
  // budget. Render supports long-running web responses, while AgentCore still
  // enforces the per-command timeout and Forge keeps execution authority shut.
  if (path.split("?")[0] === "/mobile/cloud/environments.json") {
    return HOSTED_ENVIRONMENT_TIMEOUT_MS;
  }
  return REASONING_ROUTES.some((route) => path.includes(route))
    ? REASONING_TIMEOUT_MS
    : FAST_TIMEOUT_MS;
}
