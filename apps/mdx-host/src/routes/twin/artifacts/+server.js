// The artifact workbench now lives inside the Twin composer. Keep old links
// working without exposing the earlier staging surface.
import { redirect } from "@sveltejs/kit";

export function GET() {
  throw redirect(307, "/twin");
}
