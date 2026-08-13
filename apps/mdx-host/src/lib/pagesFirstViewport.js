// Pages first-viewport facts, built server-side from generated contracts.
// The live library, bodies, and search are runtime truth loaded through the
// kernel proxy; these facts are what the contracts guarantee before any
// runtime answer arrives.
import worldModel from "../../../../generated/world-model/pages-projection-fixtures.json";
import authBoundary from "../../../../generated/auth/mdx-auth-session-boundary.json";

const corpus = Array.isArray(worldModel.developer_onboarding_corpus)
  ? worldModel.developer_onboarding_corpus
  : [];

export const pagesFirstViewport = {
  source: "generated/world-model/pages-projection-fixtures.json",
  currentSignal: `The company memory is a read-only projection over recorded sources; ${corpus.length} onboarding documents prove the read path before the live library loads.`,
  judgment:
    "Read what the company actually holds before writing anything new; every body is served from its recorded source, never a copy.",
  safeNext: "Open a document and follow its receipts, or search the whole corpus word for word.",
  boundary: [
    "public visibility blocked",
    "vector and embedding search blocked",
    "rich editing blocked",
    "production publish blocked",
    authBoundary.actor_admission.production_write_allowed ? "production writes allowed" : "production writes blocked"
  ],
  evidence: [worldModel.name ?? "pages projection fixtures", worldModel.contract ?? "world model contract"]
};
