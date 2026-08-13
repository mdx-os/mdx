import { redirect } from "@sveltejs/kit";
import { resolveOAuthProvider, safeReturnPath } from "../../../lib/session.server.js";

export async function GET({ locals, url }) {
  if (!locals.supabase) {
    return new Response(JSON.stringify({ error: "supabase_auth_not_configured" }), {
      status: 503,
      headers: { "content-type": "application/json", "cache-control": "private, no-store" }
    });
  }
  const provider = resolveOAuthProvider(url.searchParams.get("provider"));
  const callback = new URL("/auth/callback", url.origin);
  callback.searchParams.set("next", safeReturnPath(url.searchParams.get("next")));
  const { data, error } = await locals.supabase.auth.signInWithOAuth({
    provider,
    options: { redirectTo: callback.toString() }
  });
  if (error || !data?.url) {
    return new Response(JSON.stringify({ error: "supabase_oauth_start_failed" }), {
      status: 502,
      headers: { "content-type": "application/json", "cache-control": "private, no-store" }
    });
  }
  redirect(303, data.url);
}
