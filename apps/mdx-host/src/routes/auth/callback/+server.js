import { redirect } from "@sveltejs/kit";
import { hasMdxAdmissionClaims, safeReturnPath } from "../../../lib/session.server.js";
import { publicFunnelEnabled } from "../../../lib/experienceProfile.server.js";

export async function GET({ locals, url }) {
  const code = url.searchParams.get("code");
  const linkingIdentity = url.searchParams.get("flow") === "link";
  if (!locals.supabase || !code) {
    return new Response(JSON.stringify({ error: "supabase_oauth_callback_invalid" }), {
      status: 400,
      headers: { "content-type": "application/json", "cache-control": "private, no-store" }
    });
  }
  const { error } = await locals.supabase.auth.exchangeCodeForSession(code);
  if (error) {
    // A failed link must not strand an already admitted person on a JSON
    // error page. Their original session and authority remain untouched.
    if (linkingIdentity && locals.session?.authenticated) {
      redirect(303, "/you?identity=apple-link-failed");
    }
    const next = safeReturnPath(url.searchParams.get("next"));
    redirect(303, `/auth/sign-in?next=${encodeURIComponent(next)}&auth=exchange-failed`);
  }
  // The exchange succeeded, so the person is signed in with the identity
  // provider - but signing in is not the same as being admitted. Until an
  // operator activates their workspace their token carries no MDx claims,
  // and every product route would bounce them straight back to login. In
  // the public beta they get an honest waiting room instead of a loop.
  const { data: claimsData, error: claimsError } = await locals.supabase.auth.getClaims();
  const admitted = !claimsError && hasMdxAdmissionClaims(claimsData?.claims);
  if (!admitted && publicFunnelEnabled()) {
    const next = safeReturnPath(url.searchParams.get("next"));
    redirect(303, `/welcome/pending?next=${encodeURIComponent(next)}`);
  }
  redirect(303, safeReturnPath(url.searchParams.get("next")));
}
