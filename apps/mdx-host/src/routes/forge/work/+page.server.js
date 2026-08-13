// Compatibility doorway for the retired Work dashboard. Runs, fleets, and
// missions now share the Trails mental model, with Runs as its canonical entry.
import { redirect } from "@sveltejs/kit";

export function load() {
  throw redirect(307, "/forge/runs");
}
