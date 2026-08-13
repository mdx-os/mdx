// Controls: read the rules and operating limits that bound Forge. This is a
// read surface over existing projections, not an authority panel.

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
  const [controlPlane, capacity, providerRouting, providerPreflight, repos] = await Promise.all([
    readJson(fetch, "/forge/control-plane/projection.json"),
    readJson(fetch, "/forge/fleet-capacity.json"),
    readJson(fetch, "/forge/model-providers.json"),
    readJson(fetch, "/forge/fleet-eval-provider-preflight.json"),
    readJson(fetch, "/forge/repos/projection.json")
  ]);

  return {
    controlPlane,
    capacity,
    providerRouting,
    providerPreflight,
    repos,
    boundary:
      "controls explain the limits Forge is operating under; changing authority still requires governed routes and receipts"
  };
}
