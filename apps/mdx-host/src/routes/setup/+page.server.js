// Moved to the admin plane (S6 two-plane split); deep links keep working.
import { redirect } from "@sveltejs/kit";

export function load() {
  throw redirect(307, "/admin/setup");
}
