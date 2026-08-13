// Repo identity helpers, shared so the activation "Set up Forge" step derives
// the exact same repo_id/label as the Forge home (forge/+page.svelte keeps an
// identical local copy). Pure functions: a path or clone URL in, a stable id +
// human label out. If the Forge derivation ever changes, mirror it here.

export function slugRepoId(name) {
  const slug = String(name)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return slug || "repo";
}

// A http(s)/git@/ssh URL is a remote; anything else is a local folder path.
export function repoKindOf(root) {
  return /^(https?:\/\/|git@|ssh:\/\/)/i.test(String(root).trim()) ? "remote" : "local";
}

export function deriveRepoFields(root, kind) {
  const raw = String(root).trim();
  if (kind !== "remote") {
    const base = raw.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || "repo";
    return { id: slugRepoId(base), label: base };
  }
  // Normalize https://host/owner/repo.git and git@host:owner/repo.git alike.
  let path = raw
    .replace(/\.git$/i, "")
    .replace(/\/+$/, "")
    .replace(/^[a-z][a-z0-9+.-]*:\/\//i, "")
    .replace(/^[^@/]+@/, "")
    .replace(/:/g, "/");
  const parts = path.split("/").filter(Boolean);
  const name = parts.length ? parts[parts.length - 1] : "repo";
  const owner = parts.length >= 2 ? parts[parts.length - 2] : "";
  return { id: slugRepoId(name), label: owner ? `${owner}/${name}` : name };
}

// Split a git failure into the human line and the raw stderr we tuck behind
// a disclosure. One implementation for every connect surface.
export function parseConnectError(reason) {
  const text = String(reason ?? "").trim();
  const notFound = /not found|404|does not exist|repository .* not found/i.test(text);
  const marker = text.match(/\b(fatal:|error:|remote:)/i);
  let line = text || "That did not connect.";
  let raw = "";
  if (marker && marker.index > 0) {
    line = text.slice(0, marker.index).replace(/[\s:;-]+$/, "").trim();
    raw = text.slice(marker.index).trim();
  }
  if (notFound) line = "We couldn't find that repo. Check the URL, or it may be private.";
  return { line, raw };
}
