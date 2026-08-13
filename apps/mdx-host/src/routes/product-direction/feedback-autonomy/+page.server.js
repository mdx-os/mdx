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

async function postJson(fetchImpl, path) {
  const response = await fetchImpl(`/api/kernel${path}`, {
    method: "POST",
    headers: { "content-type": "application/json", "x-mdx-receipt-intent": "feedback_autonomy" },
    body: "{}"
  });
  if (!response.ok) {
    throw new Error(`kernel write failed: ${response.status}`);
  }
  return response.json();
}

export async function load({ fetch }) {
  const projection = await readJson(fetch, "/feedback/autonomy-runs/projection.json");
  return {
    projection,
    boundary:
      "feedback autonomy shows receipt-backed loop state only; assessment and closeout pages hold the reasoning trail",
    safeNext: "run open loops, then inspect the lane, Pages evidence, Product work, and Forge handoff"
  };
}

export const actions = {
  run: async ({ fetch }) => {
    try {
      const packet = await postJson(fetch, "/feedback/autonomy-runs.json");
      return {
        ok: true,
        line: `${packet.processed_count ?? 0} processed, ${packet.skipped_count ?? 0} already closed.`
      };
    } catch (error) {
      return {
        ok: false,
        line: "The loop did not run. Check the local kernel."
      };
    }
  }
};
