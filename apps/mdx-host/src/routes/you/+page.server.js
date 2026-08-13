// The You surface: profile data itself is client-side (this machine
// only - no kernel profile contract exists, and the page never pretends
// one does). The server load passes only the companion roster (generated
// contract truth stays out of client bundles) and the viewport facts.
import { companions, defaultCompanionId } from "../../lib/twinCompanions.js";

export async function load({ locals, url }) {
  let appleConnected = false;
  if (locals.supabase && locals.session?.authenticated) {
    const { data, error } = await locals.supabase.auth.getUser();
    if (!error && data?.user) {
      appleConnected = (data.user.identities ?? []).some(
        (identity) => identity.provider === "apple"
      );
    }
  }
  return {
    companions: companions.map((companion) => ({
      id: companion.id,
      name: companion.name,
      stance: companion.stance
    })),
    defaultCompanionId,
    identity: {
      appleConnected,
      linkFailed: url.searchParams.get("identity") === "apple-link-failed"
    },
    boundary:
      "everything on this page stays in this browser on this machine; nothing is sent anywhere until you ask Twin a question, and then only the persona id and answer framing travel - never your samples",
    safeNext: "set a name and a style, then ask Twin something and open the answer's proof to see your persona riding along"
  };
}
