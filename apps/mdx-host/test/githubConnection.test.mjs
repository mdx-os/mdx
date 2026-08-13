import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { GET as githubCallback } from "../src/routes/forge/connect/github/+server.js";
import { POST as githubWebhook } from "../src/routes/forge/connect/github/webhook/+server.js";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");

test("hosted Forge connects private repositories through the GitHub App", () => {
  const repoConnect = read("../src/lib/RepoConnect.svelte");

  assert.match(repoConnect, /export let hosted|hosted = false/);
  assert.match(repoConnect, /forge\/connect\/github\?action=start/);
  assert.match(repoConnect, /Choose repositories in GitHub/);
  assert.match(repoConnect, /No personal access token/);
  assert.match(repoConnect, /\{#if !hosted\}[\s\S]*point Forge at a folder on this machine/);
  assert.match(repoConnect, /hosted[\s\S]*Public repositories connect directly/);
});

test("the GitHub callback preserves the signed state handoff and never asks for a token", () => {
  const callback = read("../src/routes/forge/connect/github/+server.js");

  assert.match(callback, /searchParams\.get\("installation_id"\)/);
  assert.match(callback, /searchParams\.get\("state"\)/);
  assert.match(callback, /state\.length < 32/);
  assert.match(callback, /mobile\/cloud\/github-installations\.json/);
  assert.match(callback, /mdx:\/\/github-installed\?installation_id=/);
  assert.match(callback, /mobile\/cloud\/github-installation-sessions\.json/);
  assert.match(callback, /mobile\/cloud\/repositories\.json/);
  assert.match(callback, /mobile\/cloud\/repository-connections\.json/);
  assert.match(callback, /repositories\.length > 20/);
  assert.match(callback, /The installation stays bound to the MDx account that started it/);
  assert.match(callback, /never a personal access token/);
  assert.doesNotMatch(callback, /personal access token[^\n]*input/i);
});

test("the web, welcome, and Forge entry points all select hosted repository connection", () => {
  for (const path of [
    "../src/routes/forge/+page.svelte",
    "../src/routes/forge/runs/+page.svelte",
    "../src/routes/welcome/setup/+page.svelte"
  ]) {
    assert.match(read(path), /<RepoConnect[\s\S]*hosted=\{data\.session\?\.authenticated === true\}/, path);
  }
  const forgeServer = read("../src/routes/forge/+page.server.js");
  assert.match(forgeServer, /githubOutcome === "connected"/);
  assert.match(forgeServer, /GitHub is connected\. Choose the repository for your first build\./);
});

test("a signed-out GitHub return preserves the native handoff without touching the kernel", async () => {
  const state = "abcdefghijklmnopqrstuvwxyz0123456789";
  const response = await githubCallback({
    locals: { session: { authenticated: false } },
    url: new URL(`https://mdx.test/forge/connect/github?installation_id=42&state=${state}`),
    fetch: async () => {
      throw new Error("signed-out handoff must not call the private kernel");
    }
  });
  assert.equal(response.status, 200);
  assert.match(response.headers.get("cache-control"), /no-store/);
  const html = await response.text();
  assert.match(html, /mdx:\/\/github-installed\?installation_id=42&amp;state=/);
  assert.match(html, /Finish on the web/);
  assert.match(html, /never a personal access token/);
});

test("an unconfigured GitHub App returns a human beta hold instead of a dead end", async () => {
  await assert.rejects(
    githubCallback({
      locals: { session: { authenticated: true } },
      url: new URL("https://mdx.test/forge/connect/github?action=start"),
      fetch: async () => Response.json({ status: "REFUSED", error: "github_app_not_configured" })
    }),
    (error) => error?.status === 303 && error?.location === "/forge?github=setup-required"
  );
});

test("a verified GitHub return connects the selected repositories and opens Forge", async () => {
  const calls = [];
  const fetch = async (path, init = {}) => {
    calls.push([path, init.method ?? "GET"]);
    if (path.endsWith("/github-installations.json")) return Response.json({ status: "CONNECTED" });
    if (path.endsWith("/setup.json")) {
      return Response.json({
        status: "OK",
        installations: [{ installation_id: 42, status: "active" }],
        repositories: []
      });
    }
    if (path.includes("/repositories.json?")) {
      return Response.json({
        status: "OK",
        repositories: [{ repository_id: 7, full_name: "cohort/example", default_branch: "main" }]
      });
    }
    if (path.endsWith("/repository-connections.json")) return Response.json({ status: "CONNECTED" });
    throw new Error(`unexpected fetch: ${path}`);
  };
  await assert.rejects(
    githubCallback({
      locals: { session: { authenticated: true } },
      url: new URL(
        "https://mdx.test/forge/connect/github?installation_id=42&state=abcdefghijklmnopqrstuvwxyz0123456789"
      ),
      fetch
    }),
    (error) => error?.status === 303 && error?.location === "/forge?github=connected"
  );
  assert.deepEqual(calls, [
    ["/api/kernel/mobile/cloud/github-installations.json", "POST"],
    ["/api/kernel/mobile/cloud/setup.json", "GET"],
    ["/api/kernel/mobile/cloud/repositories.json?installation_id=42", "GET"],
    ["/api/kernel/mobile/cloud/repository-connections.json", "POST"]
  ]);
});

test("the public GitHub webhook preserves the signed bytes and only forwards installation events", async () => {
  const payload = JSON.stringify({ action: "deleted", installation: { id: 42 } });
  let forwarded;
  const response = await githubWebhook({
    request: new Request("https://mdx.test/forge/connect/github/webhook", {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-hub-signature-256": `sha256=${"a".repeat(64)}`,
        "x-github-delivery": "delivery_42",
        "x-github-event": "installation"
      },
      body: payload
    }),
    fetch: async (path, init) => {
      forwarded = { path, init, body: new TextDecoder().decode(init.body) };
      return Response.json({ status: "ACCEPTED" });
    }
  });
  assert.equal(response.status, 200);
  assert.match(forwarded.path, /\/mobile\/cloud\/github-webhooks\.json$/);
  assert.equal(forwarded.init.headers["x-github-event"], "installation");
  assert.equal(forwarded.body, payload);
});

test("the GitHub webhook rejects unsupported events before the kernel", async () => {
  const response = await githubWebhook({
    request: new Request("https://mdx.test/forge/connect/github/webhook", {
      method: "POST",
      headers: {
        "x-hub-signature-256": `sha256=${"a".repeat(64)}`,
        "x-github-delivery": "delivery_42",
        "x-github-event": "push"
      },
      body: "{}"
    }),
    fetch: async () => {
      throw new Error("invalid events must not reach the kernel");
    }
  });
  assert.equal(response.status, 401);
});
