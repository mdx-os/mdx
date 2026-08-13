import { adminRegistry, adminRegistryHtml } from "../../../lib/adminRegistry.js";

export function load() {
  const registry = adminRegistry();
  return {
    registry,
    registryHtml: adminRegistryHtml(registry),
    boundary:
      "registry is read-only; it maps existing generated contracts and opens no admin write path",
    safeNext:
      "use this map to pick the next room, then add governed writes only behind receipts and session authority"
  };
}
