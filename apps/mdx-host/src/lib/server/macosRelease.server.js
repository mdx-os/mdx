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
  return {
    version,
    build,
    sha256,
    size_bytes: sizeBytes,
    notarized_at: String(value.notarized_at ?? "").trim()
  };
}

export function archivePathForMacRelease(manifest) {
  return `releases/macos/canary/${manifest.version}/${manifest.build}/MDx.zip`;
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
