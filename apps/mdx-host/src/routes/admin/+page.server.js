import { redirect } from "@sveltejs/kit";

// The console opens on health, not setup: at 2am the first screen
// answers "is anything broken right now". Getting started lives one
// click away on the console rail.
export function load() {
  redirect(307, "/admin/watchtower");
}
