import {
  diskImagePathForMacRelease,
  dmgReadyMacReleaseManifest,
  latestMacReleaseManifest,
  signedMacReleaseURL
} from "../../../../lib/server/macosRelease.server.js";

const PRIVATE_HEADERS = {
  "cache-control": "private, no-store",
  "x-content-type-options": "nosniff"
};

export async function GET({ locals, fetch }) {
  if (!locals.session?.authenticated || !locals.supabase) {
    return new Response("authenticated beta session required\n", {
      status: 401,
      headers: PRIVATE_HEADERS
    });
  }
  try {
    const manifest = dmgReadyMacReleaseManifest(
      await latestMacReleaseManifest({ locals, fetch })
    );
    if (!manifest) {
      return new Response("notarized Mac installer unavailable\n", {
        status: 404,
        headers: PRIVATE_HEADERS
      });
    }
    const signedURL = await signedMacReleaseURL(
      locals,
      diskImagePathForMacRelease(manifest),
      120
    );
    if (!signedURL) {
      return new Response("notarized Mac installer unavailable\n", {
        status: 503,
        headers: PRIVATE_HEADERS
      });
    }
    const diskImage = await fetch(signedURL, {
      headers: { accept: "application/x-apple-diskimage" },
      signal: AbortSignal.timeout(120_000)
    });
    if (!diskImage.ok || !diskImage.body) {
      return new Response("notarized Mac installer unavailable\n", {
        status: 503,
        headers: PRIVATE_HEADERS
      });
    }
    return new Response(diskImage.body, {
      headers: {
        ...PRIVATE_HEADERS,
        "content-type": "application/x-apple-diskimage",
        "content-length": String(manifest.dmg_size_bytes),
        "content-disposition": 'attachment; filename="MDx.dmg"'
      }
    });
  } catch {
    return new Response("private installer channel unavailable\n", {
      status: 503,
      headers: PRIVATE_HEADERS
    });
  }
}
