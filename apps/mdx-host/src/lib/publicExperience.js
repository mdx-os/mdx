export const PUBLIC_EXPERIENCE_PATHS = new Set([
  "/landing",
  "/auth/sign-in",
  "/waitlist",
  "/redeem",
  "/welcome/pending",
  "/forge-product",
  "/open-source",
  "/downloads",
  "/forge/connect/github",
  "/forge/connect/github/webhook",
  "/security"
]);

export function isPublicExperiencePath(pathname) {
  const normalized = String(pathname ?? "").replace(/\/__data\.json$/u, "");
  return PUBLIC_EXPERIENCE_PATHS.has(normalized);
}
