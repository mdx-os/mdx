import { redirect } from "@sveltejs/kit";

const accountPath = "/you";

function accountResult(status) {
  return `${accountPath}?identity=${status}`;
}

export async function GET({ locals, url }) {
  // /auth routes stay reachable so OAuth callbacks can complete, which means
  // this endpoint must enforce its own authenticated boundary. Identity
  // linking is never an admission path: it can only add Apple to an already
  // verified MDx session.
  if (!locals.session?.authenticated || !locals.supabase) {
    redirect(303, `/auth/sign-in?next=${encodeURIComponent(accountPath)}`);
  }

  const callback = new URL("/auth/callback", url.origin);
  callback.searchParams.set("flow", "link");
  callback.searchParams.set("next", accountResult("apple-linked"));

  try {
    const { data, error } = await locals.supabase.auth.linkIdentity({
      provider: "apple",
      options: { redirectTo: callback.toString() }
    });
    if (error || !data?.url) {
      redirect(303, accountResult("apple-link-failed"));
    }
    redirect(303, data.url);
  } catch (error) {
    if (error?.status === 303) throw error;
    redirect(303, accountResult("apple-link-failed"));
  }
}
