// Changelog surface: reads the changelog projection (recorded entries
// with title, summary, kind) through the kernel proxy. Fail-soft: an
// unreachable kernel or missing projection renders an honest empty
// state. Entries are newest-first from the projection contract.

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
  const changelog = await readJson(fetch, "/changelog/projection.json");

  return {
    changelog
  };
}
