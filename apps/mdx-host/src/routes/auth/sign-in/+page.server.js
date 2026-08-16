import { safeReturnPath } from "../../../lib/session.server.js";

export const _signInGuidance = {
  title: "Sign in to MDx",
  intro: "Use the Google or Apple account that received your invite. Your workspace follows you across web, Mac, and iPhone.",
  identity: "Already started with Google? Connect Apple later from You. If you use Hide My Email, its relay address must be invited too."
};

const signInGuidanceByDestination = {
  "/download/macos": {
    ..._signInGuidance,
    title: "Get MDx for Mac",
    intro: "Sign in with the invited Google or Apple account. You will return here to download the signed, notarized Mac app."
  },
  "/welcome/beta": {
    ..._signInGuidance,
    title: "Sign in to open MDx",
    intro: "Use the Google or Apple account that received your invite. Your workspace and first-session guide will be ready when you return."
  }
};

export function load({ url }) {
  const next = safeReturnPath(url.searchParams.get("next"));
  return {
    next,
    guidance: signInGuidanceByDestination[next] ?? _signInGuidance,
    notice: url.searchParams.get("auth") === "exchange-failed"
      ? "We couldn't finish that sign-in. Try the same provider again. If you used Apple with Hide My Email, make sure its relay address received the invite."
      : ""
  };
}
