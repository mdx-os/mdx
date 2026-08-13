import { safeReturnPath } from "../../../lib/session.server.js";

export const _signInGuidance = {
  intro: "First time here? Choose the provider for the address that received your invite. You will use this workspace on web, Mac, and iPhone.",
  identity: "Once you are inside, connect Apple from You without creating another workspace. If you start with Apple and choose Hide My Email, its private relay address must match your beta invite."
};

export function load({ url }) {
  return { next: safeReturnPath(url.searchParams.get("next")), guidance: _signInGuidance };
}
