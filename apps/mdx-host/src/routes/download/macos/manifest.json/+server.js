import { json } from "@sveltejs/kit";
import { latestMacReleaseManifest } from "../../../../lib/server/macosRelease.server.js";

const PRIVATE_HEADERS = { "cache-control": "private, no-store" };

export async function GET({ locals, fetch }) {
  if (!locals.session?.authenticated || !locals.supabase) {
    return json({ error: "authenticated beta session required" }, { status: 401, headers: PRIVATE_HEADERS });
  }
  try {
    const manifest = await latestMacReleaseManifest({ locals, fetch });
    if (!manifest) {
      return json({ available: false, manifest: null }, { headers: PRIVATE_HEADERS });
    }
    return json({ available: true, manifest }, { headers: PRIVATE_HEADERS });
  } catch (error) {
    return json({ available: false, manifest: null }, { status: 503, headers: PRIVATE_HEADERS });
  }
}
