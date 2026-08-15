import {
  dmgReadyMacReleaseManifest,
  latestMacReleaseManifest
} from "../../../lib/server/macosRelease.server.js";

const AUTHENTICATED_MAC_DMG_PATH = "/download/macos/installer.dmg";
const AUTHENTICATED_MAC_ZIP_PATH = "/download/macos/update.zip";

export const _appHandoff = {
  continuity: "Use the same invited Google or Apple account everywhere. Your workspace, Pages, and Forge history will follow you between web, Mac, and iPhone.",
  macDmgSteps: [
    "Open MDx.dmg after it downloads.",
    "Drag MDx into the Applications folder shown in the installer.",
    "Eject MDx, then open the app from Applications and sign in with the account connected to your invite."
  ],
  macZipSteps: [
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
    const hasDmg = Boolean(dmgReadyMacReleaseManifest(manifest));
    return {
      available: true,
      manifest,
      downloadUrl: hasDmg ? AUTHENTICATED_MAC_DMG_PATH : AUTHENTICATED_MAC_ZIP_PATH,
      downloadFilename: hasDmg ? "MDx.dmg" : "MDx.zip",
      macSteps: hasDmg ? _appHandoff.macDmgSteps : _appHandoff.macZipSteps,
      appHandoff: _appHandoff
    };
  } catch {
    return unavailable();
  }
}
