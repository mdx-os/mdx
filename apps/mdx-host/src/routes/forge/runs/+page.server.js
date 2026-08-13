// Forge runs: the build agent at work. Reads the run projection (event
// trails in the DXR grammar) and the backed work items a run can serve.
// Fail-soft: an unreachable kernel renders an empty, honest invitation.

import { fail } from "@sveltejs/kit";

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

export async function load({ fetch, depends, locals }) {
  // Scoped live-poll key: the page re-reads on invalidate("forge:runs") which
  // re-runs ONLY this loader, not the shell layout waterfall (which would
  // otherwise re-download the needs-you projection and version every tick).
  // The open run's live detail rides SSE in RunViewer; this poll just keeps
  // the list's membership and statuses fresh.
  depends("forge:runs");
  const [runs, work, repos, recipes] = await Promise.all([
    readJson(fetch, "/forge/runs/projection.json"),
    readJson(fetch, "/product/work-items/projection.json"),
    readJson(fetch, "/forge/repos/projection.json"),
    readJson(fetch, "/forge/recipes.json")
  ]);

  // The projection embeds full event trails; the page renders the recent
  // tail and RunViewer streams the rest live. Cap what gets serialized (and
  // re-downloaded on every live-poll tick) instead of shipping full history.
  const slimRuns = runs
    ? {
        ...runs,
        runs: (Array.isArray(runs.runs) ? runs.runs : []).slice(0, 60).map((run) => ({
          ...run,
          status:
            run?.status === "running" &&
            run?.operator_status === "needs_you" &&
            (Array.isArray(run?.events) ? run.events : []).some((event) =>
              /startup found an interrupted run|explicit resume can recover/i.test(String(event?.detail ?? ""))
            )
              ? "interrupted"
              : run?.status,
          event_count: run?.event_count ?? (Array.isArray(run?.events) ? run.events.length : 0),
          events: Array.isArray(run?.events) ? run.events.slice(-120) : run?.events
        }))
      }
    : runs;

  return {
    runs: slimRuns,
    work,
    repos,
    recipes,
    session: locals.session,
    boundary:
      "a run works in an isolated copy of the repo and can only run the checks you allow; it produces a branch, never a deploy",
    safeNext: "describe the change, name the checks that prove it, and watch the agent work"
  };
}

export const actions = {
  resume: async ({ request, fetch, locals }) => {
    const form = await request.formData();
    const runId = String(form.get("run_id") ?? "");
    if (!runId.startsWith("forge_run_")) return fail(400, { resumeError: "That saved run is not valid.", runId });
    const projectionResponse = await fetch("/api/kernel/forge/runs/projection.json");
    const projection = await projectionResponse.json().catch(() => ({}));
    const run = (Array.isArray(projection?.runs) ? projection.runs : []).find((candidate) => candidate?.run_id === runId);
    if (!projectionResponse.ok || !run) return fail(404, { resumeError: "That saved run is no longer available.", runId });
    const backend = String(run.execution_backend_kind ?? "local");
    const cloudEnvironmentId = String(run.cloud_environment_id ?? "");
    if (backend === "hosted_sandbox" && !cloudEnvironmentId) {
      return fail(400, { resumeError: "The saved cloud environment is missing. Start a fresh run from Build.", runId });
    }
    const selectedChecks = (Array.isArray(run.selected_checks) ? run.selected_checks : []).map(String).filter(Boolean);
    const response = await fetch("/api/kernel/forge/runs.json", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        intent: String(run.operator_intent ?? run.intent ?? run.run_title ?? ""),
        run_title: String(run.run_title ?? ""),
        repo_id: String(run.repo_id ?? ""),
        allowed_commands: selectedChecks,
        actor_id: locals.session?.user_id ?? "local_user",
        fleet_width: 1,
        execution_backend: backend,
        cloud_environment_id: backend === "hosted_sandbox" ? cloudEnvironmentId : undefined,
        requested_run_id: runId,
        resume: true
      })
    });
    const packet = await response.json().catch(() => ({}));
    if (!response.ok || packet.status !== "RUN_STARTED") {
      return fail(response.ok ? 502 : response.status, { resumeError: packet.reason ?? "That run could not resume.", runId });
    }
    return { resumeOk: true, runId };
  }
};
