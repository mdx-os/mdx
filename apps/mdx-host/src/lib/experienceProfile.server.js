// One deployment, three front doors. The same MDx build serves:
//
//   local        - a cloned repo on someone's machine: no marketing, no
//                  waitlist; "/" goes straight to setup and the product.
//   public-beta  - the invite-only hosted beta: public landing, waitlist,
//                  invite redemption, and a pending room for signed-in
//                  people whose access is not active yet.
//   enterprise   - a private company deployment (e.g. an internal cloud):
//                  no public marketing at all; every route requires the
//                  organization's verified sign-in.
//
// The profile is derived from configuration the deployments already set
// (MDX_PUBLIC_ROOT_MODE, MDX_DEPLOYMENT_MODE) so nothing existing breaks,
// and MDX_EXPERIENCE_PROFILE can name the profile explicitly when a
// deployment wants to be unambiguous.
import { publicRootEnabled } from "./publicRoot.server.js";
import { verifiedHostSessionRequired } from "./session.server.js";

const KNOWN_PROFILES = new Set(["local", "public-beta", "enterprise"]);

export function experienceProfile() {
  const explicit = String(process.env.MDX_EXPERIENCE_PROFILE ?? "")
    .trim()
    .toLowerCase();
  if (KNOWN_PROFILES.has(explicit)) return explicit;
  if (publicRootEnabled()) return "public-beta";
  if (verifiedHostSessionRequired()) return "enterprise";
  return "local";
}

// The public funnel (landing, waitlist, redeem, pending) exists only in the
// public beta. Local machines don't need marketing; enterprise deployments
// must not expose it.
export function publicFunnelEnabled() {
  return experienceProfile() === "public-beta";
}

// Hosted experiences (public beta and enterprise) run on someone else's
// infrastructure: first-run copy must not claim "this machine".
export function hostedExperience() {
  return experienceProfile() !== "local";
}
