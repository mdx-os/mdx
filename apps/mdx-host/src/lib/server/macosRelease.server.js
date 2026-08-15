export const MAC_RELEASE_BUCKET = "mdx-canary-releases";
export const MAC_RELEASE_MANIFEST_PATH = "latest/manifest.json";

export function validMacReleaseManifest(value) {
  if (!value || typeof value !== "object") return null;
  const version = String(value.version ?? "").trim();
  const build = String(value.build ?? "").trim();
  const sha256 = String(value.sha256 ?? "").trim().toLowerCase();
  const sizeBytes = Number(value.size_bytes ?? 0);
  if (
    !/^\d+\.\d+\.\d+$/.test(version) ||
    !/^\d+$/.test(build) ||
    !/^[a-f0-9]{64}$/.test(sha256) ||
    !Number.isSafeInteger(sizeBytes) ||
    sizeBytes < 1
  ) {
    return null;
  }
  const manifest = {
    version,
    build,
    sha256,
    size_bytes: sizeBytes,
    notarized_at: String(value.notarized_at ?? "").trim()
  };
  const sparkleSignature = String(value.sparkle_ed_signature ?? "").trim();
  if (/^[A-Za-z0-9+/]{86}==$/.test(sparkleSignature)) {
    manifest.sparkle_ed_signature = sparkleSignature;
  }
  const dmgSha256 = String(value.dmg_sha256 ?? "").trim().toLowerCase();
  const dmgSizeBytes = Number(value.dmg_size_bytes ?? 0);
  const dmgNotarizedAt = String(value.dmg_notarized_at ?? "").trim();
  const hasDmgMetadata = ["dmg_sha256", "dmg_size_bytes", "dmg_notarized_at"]
    .some((field) => Object.prototype.hasOwnProperty.call(value, field));
  if (hasDmgMetadata) {
    if (
      !/^[a-f0-9]{64}$/.test(dmgSha256) ||
      !Number.isSafeInteger(dmgSizeBytes) ||
      dmgSizeBytes < 1 ||
      !/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(dmgNotarizedAt)
    ) {
      return null;
    }
    manifest.dmg_sha256 = dmgSha256;
    manifest.dmg_size_bytes = dmgSizeBytes;
    manifest.dmg_notarized_at = dmgNotarizedAt;
  }
  return manifest;
}

export function sparkleReadyMacReleaseManifest(value) {
  const manifest = validMacReleaseManifest(value);
  return manifest?.sparkle_ed_signature && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/.test(manifest.notarized_at)
    ? manifest
    : null;
}

export function dmgReadyMacReleaseManifest(value) {
  const manifest = validMacReleaseManifest(value);
  return manifest?.dmg_sha256 && manifest.dmg_size_bytes && manifest.dmg_notarized_at
    ? manifest
    : null;
}

export function archivePathForMacRelease(manifest) {
  return `releases/macos/canary/${manifest.version}/${manifest.build}/MDx.zip`;
}

export function diskImagePathForMacRelease(manifest) {
  return `releases/macos/canary/${manifest.version}/${manifest.build}/MDx.dmg`;
}

export async function signedMacReleaseURL(locals, path, expiresIn = 60) {
  const { data, error } = await locals.supabase.storage
    .from(MAC_RELEASE_BUCKET)
    .createSignedUrl(path, expiresIn);
  if (error || !data?.signedUrl) return "";
  const url = new URL(data.signedUrl);
  return url.protocol === "https:" ? url.href : "";
}

export async function latestMacReleaseManifest({ locals, fetch: fetchImpl }) {
  const manifestUrl = await signedMacReleaseURL(locals, MAC_RELEASE_MANIFEST_PATH);
  if (!manifestUrl) return null;
  const response = await fetchImpl(manifestUrl, {
    headers: { accept: "application/json" },
    signal: AbortSignal.timeout(5000)
  });
  return response.ok ? validMacReleaseManifest(await response.json()) : null;
}

function escapeXML(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}

export function macAppcastXML(manifest, origin) {
  const ready = sparkleReadyMacReleaseManifest(manifest);
  const item = ready
    ? `
    <item>
      <title>MDx ${escapeXML(ready.version)}</title>
      <pubDate>${escapeXML(new Date(ready.notarized_at).toUTCString())}</pubDate>
      <sparkle:minimumSystemVersion>14.0</sparkle:minimumSystemVersion>
      <enclosure
        url="${escapeXML(new URL("/download/macos/update.zip", origin).href)}"
        sparkle:version="${escapeXML(ready.build)}"
        sparkle:shortVersionString="${escapeXML(ready.version)}"
        sparkle:edSignature="${escapeXML(ready.sparkle_ed_signature)}"
        length="${ready.size_bytes}"
        type="application/octet-stream" />
    </item>`
    : "";
  return `<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>MDx private macOS canary</title>
    <link>${escapeXML(new URL("/download/macos/appcast.xml", origin).href)}</link>${item}
  </channel>
</rss>\n`;
}
