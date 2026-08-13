import generatedRoutes from "../../../../generated/routes/mdx-local-routes.json";
import authBoundary from "../../../../generated/auth/mdx-auth-session-boundary.json";
import { frontendPerformanceBudgets, frontendTelemetryRelease } from "../lib/telemetry.js";
import { adminAccess } from "../lib/session.server.js";
import { isPublicExperiencePath } from "../lib/publicExperience.js";

export async function load({ locals, fetch, url }) {
  // Attention is one kernel projection. The rail badges read the same
  // surface counts as the lists people land on when they click. A read that
  // fails does NOT mean the kernel is down (it may just be slow, or that one
  // projection is empty); reachability is decided by a dedicated liveness
  // probe below, never by whether a product read happened to answer in 800ms.
  async function readCount(path, extract, timeoutMs = 800) {
    try {
      const response = await fetch(`/api/kernel${path}`, { signal: AbortSignal.timeout(timeoutMs) });
      if (!response.ok) return null;
      return extract(await response.json());
    } catch (error) {
      return null;
    }
  }
  // Dedicated liveness read: version.json is the tiny contract fingerprint the
  // kernel always serves. This alone decides reachable, separate from the
  // product reads, so a slow projection never renders the app as "offline".
  async function probeReachable() {
    try {
      const response = await fetch("/api/kernel/version.json", { signal: AbortSignal.timeout(2500) });
      return { reachable: response.ok, error: response.ok ? "" : `kernel returned ${response.status}` };
    } catch (error) {
      const timedOut = error?.name === "TimeoutError" || error?.name === "AbortError";
      return { reachable: false, error: timedOut ? "liveness probe timed out" : (error?.message ?? "kernel unreachable") };
    }
  }

  // The two reads gated first paint sequentially; run them together so the
  // shell's first byte does not wait on the sum of both round-trips.
  const publicExperience = isPublicExperiencePath(url.pathname);
  const [attention, kernelVersion, liveness] = publicExperience ? [null, null, { reachable: false, error: "" }] : await Promise.all([
    readCount("/product/needs-you/projection.json", (p) => {
      const total = Number(p?.total_count ?? 0);
      const surfaceCounts = p?.surface_counts ?? {};
      const forge = Number(surfaceCounts?.forge ?? 0) || 0;
      const message = Number(surfaceCounts?.message ?? 0) || 0;
      return {
        total,
        forge,
        message,
        product: Math.max(0, total - forge - message)
      };
    }),
    readCount("/version.json", (p) => ({
      version: String(p?.kernel_version ?? ""),
      fingerprint: String(p?.contract_fingerprint ?? ""),
      minAppVersion: String(p?.min_app_version ?? "")
    })),
    probeReachable()
  ]);
  const needsYouCount = attention?.product ?? null;
  const forgeNeedsYouCount = attention?.forge ?? null;
  const messageWaitingCount = attention?.message ?? null;

  return {
    publicExperience,
    needsYouCount,
    forgeNeedsYouCount,
    messageWaitingCount,
    kernelVersion,
    reachable: liveness.reachable,
    reachableError: liveness.error,
    telemetry: {
      release: frontendTelemetryRelease,
      budgets: frontendPerformanceBudgets
    },
    // The host session seat: local-demo resolves to the deterministic owner
    // fixture; production/local-secure fail closed unless a verified identity
    // source populates locals.session.
    session: locals.session,
    sessionBoundary: {
      status: locals.session.status,
      authenticated: locals.session.authenticated === true,
      identitySource: locals.session.identity_source,
      admin: adminAccess(locals.session),
      productionAuthReady: locals.session.production_auth_ready,
      localRoute: authBoundary.local_route,
      actorAdmissionRequired: authBoundary.required_controls.includes("actor_admission_required_for_mutation")
    },
    routeSummary: {
      routeCount: generatedRoutes.routes.length,
      governedWriteCount: generatedRoutes.routes.filter((route) => route.method === "POST" && route.receipt_backed).length
    }
  };
}
