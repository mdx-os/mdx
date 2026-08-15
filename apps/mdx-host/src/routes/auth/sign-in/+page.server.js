import { safeReturnPath } from "../../../lib/session.server.js";

export const _signInGuidance = {
  intro: "Use the Google or Apple account that received your invite. Your workspace follows you across web, Mac, and iPhone.",
  identity: "Already started with Google? Connect Apple later from You. If you use Hide My Email, its relay address must be invited too."
};

export function load({ url }) {
  return {
    next: safeReturnPath(url.searchParams.get("next")),
    guidance: _signInGuidance,
    notice: url.searchParams.get("auth") === "exchange-failed"
      ? "We couldn't finish that sign-in. Try the same provider again. If you used Apple with Hide My Email, make sure its relay address received the invite."
      : ""
  };
}
