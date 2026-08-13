import { error } from "@sveltejs/kit";
import { adminAccess } from "../../lib/session.server.js";

export function load({ locals }) {
  const access = adminAccess(locals.session);
  if (!access.allowed) {
    error(403, {
      message: "Console requires an operator role",
      status: access.status,
      role: access.role,
      production_auth_ready: access.production_auth_ready
    });
  }
  return {
    consoleAccess: access,
    session: locals.session
  };
}
