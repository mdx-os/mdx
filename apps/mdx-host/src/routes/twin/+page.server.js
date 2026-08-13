// The front door: opening MDx is opening a conversation with your
// company. Contract truth stays server-side (companion roster, session,
// catalog slice), and the briefing's substance is runtime truth read
// through the kernel proxy, fail-soft - the greeting never claims a
// number the kernel did not report.
import { firstRouteForSurface, platformProof } from "../../lib/platform.js";
import { twinFirstViewport } from "../../lib/twinFirstViewport.js";
import { companions, defaultCompanionId } from "../../lib/twinCompanions.js";
import { greetingFor } from "../../lib/homeToday.js";
import generatedRoutes from "../../../../../generated/routes/mdx-local-routes.json";
import capabilityRegistry from "../../../../../generated/twin/twin-capability-registry.json";

// What each skill actually does when approved, from the registry's own
// declaration - the proposal card states route and record kind as facts.
const skillFacts = Object.fromEntries(
  capabilityRegistry.capabilities
    .filter((capability) => capability.kind === "skill")
    .map((capability) => [
      capability.id.replace(/^skill_/, ""),
      {
        route: capability.executes_via_route,
        receiptKind: capability.receipt_kind,
        humanLine: capability.human_description
      }
    ])
);

const twinCatalogSlice = {
  routes: generatedRoutes.routes.filter((route) => route.local_path.startsWith("/twin/"))
};

// The B5 Pages-draft write activates only once its route is in the generated
// catalog (i.e. landed on main). Until then the proposal stays a preview, so
// the save button is never live-but-broken against a kernel that lacks it.
const pagesDraftRouteReady = generatedRoutes.routes.some(
  (route) => route.local_path === "/twin/artifacts/pages-drafts.json"
);

// Office output activates only once both the generation and byte-download
// routes are in the generated catalog, same discipline as the Pages draft.
const officeGenerateRouteReady = generatedRoutes.routes.some(
  (route) => route.local_path === "/twin/office/generated-artifacts.json"
);
const officeDownloadRouteReady = generatedRoutes.routes.some(
  (route) => route.local_path === "/twin/office/downloads.json"
);
const officeOutputRouteReady = officeGenerateRouteReady && officeDownloadRouteReady;

export async function load({ locals }) {
  // First paint should not wait on the ten-way projection fan-out. The
  // greeting, composer, and shell are the critical surface; the "what's
  // moving" panels are enrichment, so the page loads instantly and the client
  // fetches those panels from /twin/moving after mount (a plain same-origin
  // fetch - streaming deferred promises are blocked by this app's strict
  // hash-based CSP, so a client fetch is the CSP-safe way to defer).
  return {
    proof: platformProof,
    route: firstRouteForSurface("twin"),
    viewport: twinFirstViewport,
    companions,
    defaultCompanionId,
    session: locals.session,
    catalogSlice: twinCatalogSlice,
    pagesDraftRouteReady,
    officeGenerateRouteReady: officeOutputRouteReady,
    skillFacts,
    greeting: greetingFor(new Date().getHours())
  };
}
