// Welcome to MDx: the friendly first hello + the honest "how it compounds"
// story. It reads the live flywheel proof so the compounding readout shows real
// receipt-backed numbers (zeros on a brand-new workspace, which is the truth).
// Fail-soft: an unreachable kernel just yields zeros, never a fake.
async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, { signal: AbortSignal.timeout(1500) });
    return response.ok ? await response.json() : null;
  } catch (error) {
    return null;
  }
}

export async function load({ fetch }) {
  const flywheel = await readJson(fetch, "/forge/flywheel-proof.json");
  return {
    flywheel: {
      outcomes: Number(flywheel?.outcome_signal_count ?? 0),
      lessons: Number(flywheel?.candidate_lesson_count ?? 0),
      installs: Number(flywheel?.installed_capability_count ?? 0),
      memories: Number(flywheel?.active_memory_count ?? 0),
      // True once a later Forge lap has cited a lesson an earlier lap left.
      cited: flywheel?.compounding_proof?.lesson_cited_by_later_lap === true
    }
  };
}
