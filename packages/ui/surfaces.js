// Canonical registry of the local console surfaces and their dev URLs.
// Apps still own their nav arrays, but cross-surface deep links (for example
// inspecting a receipt) build their URLs here so the port map lives in one
// place instead of being copied into every app.

export const consoleSurfaces = [
  { id: "observatory", label: "Observatory", url: "http://127.0.0.1:5175", meta: "evidence" },
  { id: "concierge", label: "Concierge", url: "http://127.0.0.1:5174", meta: "answers" },
  { id: "twin", label: "Twin", url: "http://127.0.0.1:5176", meta: "companions" },
  { id: "forge", label: "Forge", url: "http://127.0.0.1:5177", meta: "build path" },
  { id: "pages", label: "Pages", url: "http://127.0.0.1:5178", meta: "knowledge" },
  { id: "message", label: "Message", url: "http://127.0.0.1:5179", meta: "threads" }
];

const byId = Object.fromEntries(consoleSurfaces.map((surface) => [surface.id, surface]));

// Build a URL to another surface, optionally with a query and a hash anchor.
// Navigating between surfaces is a full browser navigation, not a fetch, so
// this stays inside the read-only contract: no cross-origin requests.
export function surfaceUrl(id, { query = {}, hash = "" } = {}) {
  const surface = byId[id];
  if (!surface) {
    return "#";
  }
  const params = new URLSearchParams(query);
  const queryString = params.toString();
  return `${surface.url}${queryString ? `?${queryString}` : ""}${hash ? `#${hash}` : ""}`;
}

// Observatory is the evidence surface. A receipt id anywhere in the console
// should be inspectable there, so receipts deep link to Observatory with the
// id carried as a query param and the receipt trail as the landing anchor.
export function observatoryReceiptHref(receiptId) {
  return surfaceUrl("observatory", { query: { receipt: receiptId }, hash: "receipts" });
}

// Routes are receipt-backed JSON served by the local API. A route reference can
// open its evidence at the API base. Pattern routes (with a path parameter) are
// not directly fetchable, so callers should leave those inert.
export function isConcreteRoute(path) {
  return typeof path === "string" && path.length > 0 && !path.includes("{");
}

export function apiRouteHref(path, apiBase = "") {
  return `${apiBase}${path}`;
}
