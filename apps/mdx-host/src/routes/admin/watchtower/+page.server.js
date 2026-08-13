// Watchtower reads the kernel's own operator pulse through the same-origin proxy.
// The kernel route (/runtime/watchtower.json) already aggregates the ledger into
// safe counts and reasons; this page only renders them. No raw receipts, payloads,
// notes, or bodies cross this boundary - the kernel never emits them.
async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      headers: { accept: "application/json" }
    });
    if (!response.ok) return null;
    return await response.json();
  } catch (error) {
    return null;
  }
}

export async function load({ fetch }) {
  const pulse = await readJson(fetch, "/runtime/watchtower.json");
  const httpMetrics = await readJson(fetch, "/runtime/http-metrics.json");
  return {
    pulse,
    httpMetrics,
    boundary:
      "this room only reads and summarizes; it shows counts, kinds, and reasons, never a feedback note, a message or page body, a prompt, an output, a secret, or a raw receipt payload",
    safeNext: "start the local server to see the pulse fill in, or open Platform and Security for the deeper view"
  };
}
