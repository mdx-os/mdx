// The waiting room between "signed in" and "activated". A person lands
// here after OAuth when their account carries no MDx claims yet - either
// they redeemed an invite and activation is in flight, or they found the
// sign-in link before being invited. Both get the truth and a next step
// instead of a redirect loop.
import { fail, redirect } from "@sveltejs/kit";
import { publicFunnelEnabled } from "../../../lib/experienceProfile.server.js";
import { hasMdxAdmissionClaims, safeReturnPath } from "../../../lib/session.server.js";

function pendingNext(value) {
  const next = safeReturnPath(value);
  return next === "/" ? "/welcome/beta" : next;
}

export async function load({ locals, url }) {
  if (!publicFunnelEnabled()) {
    redirect(307, "/welcome");
  }
  // Already activated? Straight into the product path.
  if (locals.session?.authenticated) {
    redirect(307, "/welcome");
  }
  let identity = null;
  if (locals.supabase) {
    const { data, error } = await locals.supabase.auth.getUser();
    if (!error && data?.user) {
      identity = {
        email: data.user.email ?? "",
        name:
          String(
            data.user.user_metadata?.full_name ?? data.user.user_metadata?.name ?? ""
          ).trim() || null
      };
    }
  }
  return { identity, next: pendingNext(url.searchParams.get("next")) };
}

export const actions = {
  default: async ({ locals, request }) => {
    const form = await request.formData();
    const next = pendingNext(form.get("next"));
    if (!locals.supabase) {
      return fail(503, { error: "Sign-in is temporarily unavailable. Try again in a moment.", next });
    }

    const { data: userData, error: userError } = await locals.supabase.auth.getUser();
    if (userError || !userData?.user) {
      return fail(401, { error: "Your sign-in expired. Sign in again from the invite link.", next });
    }

    const { error: refreshError } = await locals.supabase.auth.refreshSession();
    if (refreshError) {
      return fail(502, { error: "We could not check your workspace yet. Try again in a moment.", next });
    }
    const { data: claimsData, error: claimsError } = await locals.supabase.auth.getClaims();
    if (!claimsError && hasMdxAdmissionClaims(claimsData?.claims)) {
      redirect(303, next);
    }
    return { pending: true, next };
  }
};
