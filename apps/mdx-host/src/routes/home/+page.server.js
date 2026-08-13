// The conversation moved to the front door; deep links keep working.
import { redirect } from "@sveltejs/kit";

export function load() {
  throw redirect(307, "/");
}
