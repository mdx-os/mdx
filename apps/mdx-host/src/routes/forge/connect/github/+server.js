import { redirect } from "@sveltejs/kit";

function callbackPath(url) {
  return `${url.pathname}${url.search}`;
}

function htmlEscape(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function handoffPage(url, line) {
  const installationId = Number(url.searchParams.get("installation_id") ?? 0);
  const state = String(url.searchParams.get("state") ?? "");
  const appHref =
    installationId && state.length >= 32
      ? `mdx://github-installed?installation_id=${encodeURIComponent(installationId)}&state=${encodeURIComponent(state)}`
      : "mdx://github-installed";
  const signInHref = `/auth/sign-in?next=${encodeURIComponent(callbackPath(url))}`;
  const body = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Connect GitHub - MDx</title></head>
<body><main><p>Forge setup</p><h1>Connect GitHub</h1><p>${htmlEscape(line)}</p><p><a href="${htmlEscape(appHref)}">Open MDx Anywhere</a> <a href="${htmlEscape(signInHref)}">Finish on the web</a></p><p>The installation stays bound to the MDx account that started it. MDx stores receipts, never a personal access token.</p></main></body></html>`;
  return new Response(body, {
    status: 200,
    headers: {
      "content-type": "text/html; charset=utf-8",
      "cache-control": "private, no-store",
      "content-security-policy": "default-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"
    }
  });
}

async function readJson(response) {
  return response.json().catch(() => ({}));
}

async function setupPacket(fetch) {
  const response = await fetch("/api/kernel/mobile/cloud/setup.json", {
    headers: { Accept: "application/json" }
  });
  const packet = await readJson(response);
  return response.ok && packet?.status === "OK" ? packet : null;
}

async function activeInstallation(fetch, installationId) {
  const setup = await setupPacket(fetch);
  const installation = (Array.isArray(setup?.installations) ? setup.installations : []).find(
    (candidate) =>
      candidate?.status === "active" &&
      Number(candidate?.installation_id ?? 0) === installationId
  );
  return installation ? { setup, installation } : null;
}

async function connectSelectedRepositories(fetch, installationId) {
  const active = await activeInstallation(fetch, installationId);
  if (!active) return "connection-failed";
  const response = await fetch(
    `/api/kernel/mobile/cloud/repositories.json?installation_id=${encodeURIComponent(installationId)}`,
    { headers: { Accept: "application/json" } }
  );
  const packet = await readJson(response);
  if (!response.ok || packet?.status !== "OK") return "connection-failed";
  const repositories = Array.isArray(packet.repositories) ? packet.repositories : [];
  if (!repositories.length) return "no-repositories";
  if (repositories.length > 20) return "narrow-selection";
  const connected = new Set(
    (Array.isArray(active.setup?.repositories) ? active.setup.repositories : [])
      .map((repository) => Number(repository?.repository_id ?? 0))
      .filter(Boolean)
  );
  let failures = 0;
  for (const repository of repositories) {
    const repositoryId = Number(repository?.repository_id ?? 0);
    if (!repositoryId || connected.has(repositoryId)) continue;
    const connectResponse = await fetch("/api/kernel/mobile/cloud/repository-connections.json", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ installation_id: installationId, repository_id: repositoryId })
    });
    const connectedPacket = await readJson(connectResponse);
    if (!connectResponse.ok || connectedPacket?.status !== "CONNECTED") failures += 1;
  }
  return failures ? "partial" : "connected";
}

export async function GET({ locals, url, fetch }) {
  const signedIn = locals.session?.authenticated === true;
  if (url.searchParams.get("action") === "start") {
    if (!signedIn) {
      redirect(303, `/auth/sign-in?next=${encodeURIComponent(callbackPath(url))}`);
    }
    const response = await fetch("/api/kernel/mobile/cloud/github-installation-sessions.json", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: "{}"
    });
    const packet = await readJson(response);
    if (response.ok && packet?.status === "INSTALLATION_REQUIRED" && packet?.install_url) {
      redirect(303, packet.install_url);
    }
    if (["github_app_not_configured", "github_installation_state_store_unavailable"].includes(packet?.error)) {
      redirect(303, "/forge?github=setup-required");
    }
    redirect(303, "/forge?github=connection-failed");
  }

  const installationId = Number(url.searchParams.get("installation_id") ?? 0);
  const state = String(url.searchParams.get("state") ?? "");
  if (!installationId || state.length < 32) {
    if (signedIn) redirect(303, "/forge?github=connection-failed");
    return handoffPage(url, "This GitHub return is incomplete. Start the connection again from Forge or MDx Anywhere.");
  }
  if (!signedIn) {
    return handoffPage(url, "Return to MDx Anywhere, or sign in here to finish connecting GitHub on the web.");
  }

  const response = await fetch("/api/kernel/mobile/cloud/github-installations.json", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ state, installation_id: installationId })
  });
  const packet = await readJson(response);
  if (!response.ok || (packet?.status !== "CONNECTED" && !(await activeInstallation(fetch, installationId)))) {
    redirect(303, "/forge?github=connection-failed");
  }
  const outcome = await connectSelectedRepositories(fetch, installationId);
  redirect(303, `/forge?github=${outcome}`);
}
