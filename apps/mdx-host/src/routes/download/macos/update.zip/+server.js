import {
  archivePathForMacRelease,
  latestMacReleaseManifest,
  signedMacReleaseURL,
  sparkleReadyMacReleaseManifest
} from "../../../../lib/server/macosRelease.server.js";

const PRIVATE_HEADERS = { "cache-control": "private, no-store" };

export async function GET({ locals, fetch }) {
  if (!locals.session?.authenticated || !locals.supabase) {
    return new Response("authenticated beta session required\n", {
      status: 401,
      headers: PRIVATE_HEADERS
    });
  }
  try {
    const manifest = sparkleReadyMacReleaseManifest(
      await latestMacReleaseManifest({ locals, fetch })
    );
    if (!manifest) {
      return new Response("signed Mac update unavailable\n", { status: 404, headers: PRIVATE_HEADERS });
    }
    const signedURL = await signedMacReleaseURL(
      locals,
      archivePathForMacRelease(manifest),
      120
    );
    if (!signedURL) {
      return new Response("signed Mac update unavailable\n", { status: 503, headers: PRIVATE_HEADERS });
    }
    // Stream browser and Sparkle downloads through MDx so the beta session or
    // Authorization header never follows a redirect to the storage provider.
    // The archive still comes from a fresh manifest-pinned storage URL and is
    // verified by EdDSA before Sparkle installs it.
    const archive = await fetch(signedURL, {
      headers: { accept: "application/zip" },
      signal: AbortSignal.timeout(120_000)
    });
    if (!archive.ok || !archive.body) {
      return new Response("signed Mac update unavailable\n", { status: 503, headers: PRIVATE_HEADERS });
    }
    return new Response(archive.body, {
      headers: {
        ...PRIVATE_HEADERS,
        "content-type": "application/zip",
        "content-length": String(manifest.size_bytes),
        "content-disposition": 'attachment; filename="MDx.zip"'
      }
    });
  } catch {
    return new Response("private update channel unavailable\n", { status: 503, headers: PRIVATE_HEADERS });
  }
}
