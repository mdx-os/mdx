import generatedRoutes from "../../../../generated/routes/mdx-local-routes.json";
import authBoundary from "../../../../generated/auth/mdx-auth-session-boundary.json";
import { routeCatalogFromGenerated } from "@mdx/client";

export const routeCatalog = routeCatalogFromGenerated(generatedRoutes);

export const platformProof = {
  routeCount: generatedRoutes.routes.length,
  governedWriteCount: generatedRoutes.routes.filter((route) => route.method === "POST" && route.receipt_backed).length,
  authStatus: authBoundary.status,
  actorAdmissionRequired: authBoundary.required_controls.includes("actor_admission_required_for_mutation"),
  productionWriteAllowed: authBoundary.actor_admission.production_write_allowed
};

export function firstRouteForSurface(surface) {
  return generatedRoutes.routes.find((route) => route.surface === surface && route.method === "GET") ?? null;
}
