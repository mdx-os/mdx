import { redirect } from "@sveltejs/kit";
import { publicFunnelEnabled } from "../../../lib/experienceProfile.server.js";

export async function POST({ locals }) {
  if (locals.supabase) await locals.supabase.auth.signOut();
  // Signing out must land somewhere calm. In the public beta that is the
  // landing page; the sign-in chooser remains available for enterprise hosts.
  // redirect, which reads as "I can't leave".
  redirect(303, publicFunnelEnabled() ? "/landing" : "/auth/sign-in");
}
