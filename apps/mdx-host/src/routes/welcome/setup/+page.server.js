// Set up MDx: the activation journey. Reads the receipt-backed activation
// projection (the keystone Codex contract) - the 5 steps, their proof status,
// the model lanes (incl. local-model detection + recommendations), and the
// starter-workspace honesty. Fail-soft: an unreachable kernel yields a
// not-started shell, never a fake.
async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, { signal: AbortSignal.timeout(2000) });
    return response.ok ? await response.json() : null;
  } catch (error) {
    return null;
  }
}

export async function load({ fetch, locals }) {
  const [activation, flywheel, repos, fm] = await Promise.all([
    readJson(fetch, "/activation/projection.json"),
    readJson(fetch, "/forge/flywheel-proof.json"),
    readJson(fetch, "/forge/repos/projection.json"),
    readJson(fetch, "/activation/first-mission/projection.json")
  ]);
  return {
    activation: activation ?? { status: "UNREACHABLE", progress: { steps: [], complete_steps: 0, total_steps: 5 } },
    fm: fm ?? { status: "UNREACHABLE", starters: [], capabilities: {}, mission: null, fallback_available: true },
    flywheel: {
      outcomes: Number(flywheel?.outcome_signal_count ?? 0),
      lessons: Number(flywheel?.candidate_lesson_count ?? 0),
      installs: Number(flywheel?.installed_capability_count ?? 0),
      memories: Number(flywheel?.active_memory_count ?? 0),
      cited: flywheel?.compounding_proof?.lesson_cited_by_later_lap === true
    },
    repoCount: Array.isArray(repos?.repos) ? repos.repos.length : 0,
    repos: Array.isArray(repos?.repos) ? repos.repos : [],
    session: locals.session
  };
}
