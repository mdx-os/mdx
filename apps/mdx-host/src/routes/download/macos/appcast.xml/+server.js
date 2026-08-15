import {
  latestMacReleaseManifest,
  macAppcastXML
} from "../../../../lib/server/macosRelease.server.js";

const PRIVATE_HEADERS = {
  "cache-control": "private, no-store",
  "content-type": "application/rss+xml; charset=utf-8"
};

export async function GET({ locals, fetch, url }) {
  if (!locals.session?.authenticated || !locals.supabase) {
    return new Response("authenticated beta session required\n", { status: 401, headers: PRIVATE_HEADERS });
  }
  try {
    const manifest = await latestMacReleaseManifest({ locals, fetch });
    return new Response(macAppcastXML(manifest, url.origin), { headers: PRIVATE_HEADERS });
  } catch {
    return new Response("private update channel unavailable\n", { status: 503, headers: PRIVATE_HEADERS });
  }
}
