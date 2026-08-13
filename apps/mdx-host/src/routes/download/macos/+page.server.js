import {
  archivePathForMacRelease,
  latestMacReleaseManifest,
  signedMacReleaseURL
} from "../../../lib/server/macosRelease.server.js";

export const _appHandoff = {
  continuity: "Use the same invited Google or Apple account everywhere. Your workspace, Pages, and Forge history will follow you between web, Mac, and iPhone.",
  macSteps: [
    "Open the ZIP after it downloads.",
    "Move MDx into your Applications folder.",
    "Open MDx from Applications and sign in with the account connected to your invite."
  ],
  iphone: "Your iPhone beta arrives through TestFlight after your beta seat is added. Open the invitation email on your iPhone, accept it, and sign in with the same account you used here.",
  iphoneNote: "No invitation yet? Your web and Mac access still work. Reply to your beta invitation so we can check the TestFlight seat."
};

function unavailable() {
  return { available: false, manifest: null, downloadUrl: "", appHandoff: _appHandoff };
}

export async function load({ locals, fetch, setHeaders }) {
  setHeaders({ "cache-control": "private, no-store" });
  if (!locals.session?.authenticated || !locals.supabase) {
    return unavailable();
  }
  try {
    const manifest = await latestMacReleaseManifest({ locals, fetch });
    if (!manifest) return unavailable();
    const downloadUrl = await signedMacReleaseURL(locals, archivePathForMacRelease(manifest), 120);
    return { available: Boolean(downloadUrl), manifest: downloadUrl ? manifest : null, downloadUrl, appHandoff: _appHandoff };
  } catch {
    return unavailable();
  }
}
