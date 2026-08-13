import { pagesFirstViewport } from "../../lib/pagesFirstViewport.js";
import generatedRoutes from "../../../../../generated/routes/mdx-local-routes.json";
import templateRegistry from "../../../../../generated/pages/pages-template-registry.json";

// The client receives only the Pages-scoped catalog slice; full generated
// catalogs never ship in the bundle. Pattern and query routes are enforced
// server-side by the kernel proxy.
const pagesCatalogSlice = {
  routes: generatedRoutes.routes.filter((route) => route.local_path.startsWith("/pages"))
};

async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, { signal: AbortSignal.timeout(1500) });
    return response.ok ? await response.json() : null;
  } catch (error) {
    return null;
  }
}

export async function load({ fetch, locals }) {
  // The memory pulse: real counts, fail-soft to null so the page invites
  // instead of reciting zeros or inventing health.
  const [citations, lifecycle, contextSources, contextArtifacts, contextFreshness, connectorHealth] = await Promise.all([
    readJson(fetch, "/pages/agent-citations.json"),
    readJson(fetch, "/pages/lifecycle/projection.json"),
    readJson(fetch, "/pages/context-sources/projection.json"),
    readJson(fetch, "/pages/context-artifacts/projection.json"),
    readJson(fetch, "/pages/context-freshness/projection.json"),
    readJson(fetch, "/connectors/health.json")
  ]);
  return {
    pulse: {
      documents: lifecycle?.document_count ?? null,
      citable: citations?.citable_count ?? null,
      heldBack: citations?.excluded_count ?? null,
      sourceReviews: contextSources?.review_queue_count ?? null
    },
    contextSources: contextSources ?? null,
    contextArtifacts: contextArtifacts ?? null,
    contextFreshness: contextFreshness ?? null,
    connectorHealth: connectorHealth ?? null,
    thread: {
      cameFrom: [
        { label: "Forge - what building taught us", href: "/forge" },
        { label: "Message - where it was discussed", href: "/message" }
      ],
      feeds: [{ label: "what companions may cite", href: "/" }],
      record: null
    },
    viewport: pagesFirstViewport,
    catalogSlice: pagesCatalogSlice,
    // The local session signs approve/reject as governed writes. Approval is
    // the verb this library learns in v2; the session is what makes the human
    // decision land with admission, not as an unsigned local post.
    session: locals.session,
    // Nine declared shapes for company knowledge: scaffold, must-have
    // sections (advisory), and a suggested review cadence that seeds
    // stewardship at publish. Generated, never hand-typed here.
    templates: templateRegistry.templates
  };
}
