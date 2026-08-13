import { readdirSync, readFileSync, statSync, existsSync } from "node:fs";
import { extname, resolve } from "node:path";

const appRoot = resolve(import.meta.dirname, "..");
const repoRoot = resolve(appRoot, "../..");
const read = (path) => readFileSync(resolve(appRoot, path), "utf8");
const readRoot = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const perfBudget = JSON.parse(read("scripts/perf-budget.json"));

const required = [
  "package.json",
  "svelte.config.js",
  "vite.config.js",
  "src/hooks.server.js",
  "src/lib/telemetry.js",
  "src/lib/twinFirstViewport.js",
  "src/lib/homeToday.js",
  "src/routes/home/+page.server.js",
  "src/routes/+layout.svelte",
  "src/routes/+layout.server.js",
  "src/routes/+page.svelte",
  "src/routes/+page.server.js",
  "src/routes/twin/+page.server.js",
  "src/routes/forge/+page.svelte",
  "src/routes/forge/connect/github/+server.js",
  "src/routes/forge/connect/github/webhook/+server.js",
  "src/routes/learn/+page.svelte",
  "src/routes/learn/+page.server.js",
  "src/routes/marketplace/+page.svelte",
  "src/routes/pages/+page.svelte",
  "src/routes/message/+page.svelte",
  "src/routes/api/kernel/[...path]/+server.js",
  "src/routes/api/telemetry/+server.js",
  "src/routes/admin/setup/+page.svelte",
  "src/routes/admin/setup/+page.server.js",
  "src/routes/setup/+page.server.js",
  "src/lib/setupTracks.js",
  "src/routes/you/+page.svelte",
  "src/routes/you/+page.server.js",
  "src/lib/youProfile.js",
  "src/routes/admin/developer/+page.svelte",
  "src/routes/admin/developer/+page.server.js",
  "src/routes/developer/+page.server.js",
  "src/lib/developerMap.js",
  "src/routes/talent/+page.svelte",
  "src/routes/talent/+page.server.js",
  "src/lib/talentRoster.js",
  "src/routes/admin/platform/+page.svelte",
  "src/routes/admin/platform/+page.server.js",
  "src/routes/platform/+page.server.js",
  "src/routes/admin/start/+page.svelte",
  "src/routes/admin/+layout.svelte",
  "src/routes/proof/+page.svelte",
  "src/routes/quality/+page.svelte",
  "src/routes/strategy/+page.svelte",
  "src/routes/product-direction/+page.svelte",
  "src/routes/admin/aegis/+page.svelte",
  "src/routes/admin/charter/+page.svelte",
  "src/lib/observatoryProof.js",
  "src/lib/evalSuites.js",
  "src/lib/aegisPosture.js",
  "src/lib/charterRules.js",
  "src/lib/strategyDirection.js",
  "src/lib/platformRails.js",
  "src/routes/legacy-hash/[app]/+page.server.js"
];

for (const file of required) {
  if (!existsSync(resolve(appRoot, file))) {
    throw new Error(`missing host spike file: ${file}`);
  }
}

const pkg = JSON.parse(read("package.json"));
if (pkg.name !== "@mdx/mdx-host") {
  throw new Error("host spike package must be named @mdx/mdx-host");
}

const hooks = read("src/hooks.server.js");
for (const marker of ["Content-Security-Policy", "X-Frame-Options", "Permissions-Policy", "event.locals.session"]) {
  if (!hooks.includes(marker)) {
    throw new Error(`hooks.server.js must keep marker: ${marker}`);
  }
}

const layout = read("src/routes/+layout.svelte");
for (const marker of [
  // The shell surfaces system status; it moved from the StatusPill tag to a
  // quiet online heartbeat dot in the rail foot.
  "status-dot",
  "data-sveltekit-preload-data",
  "actor admission required",
  "MDx frontend host",
  "routeTimingEvent",
  "errorEvent"
]) {
  if (!layout.includes(marker) && !read("src/app.html").includes(marker)) {
    throw new Error(`host layout must keep marker: ${marker}`);
  }
}

const telemetry = read("src/lib/telemetry.js");
for (const marker of ["web_vital", "relay_lag", "clientJsCssKb", "interactionP75Ms"]) {
  if (!telemetry.includes(marker)) {
    throw new Error(`host telemetry must keep marker: ${marker}`);
  }
}

const kernelProxy = read("src/routes/api/kernel/[...path]/+server.js");
const kernelIdentity = read("src/lib/session.server.js");
for (const marker of ["route not declared in the generated catalog", "kernel unavailable", "X-MDX-Actor-Admission"]) {
  if (!kernelProxy.includes(marker) && !kernelIdentity.includes(marker)) {
    throw new Error(`host kernel proxy must keep marker: ${marker}`);
  }
}

const twinProjection = read("src/lib/twinFirstViewport.js");
for (const marker of ["twin-session-persona-contract", "provider calls blocked", "memory writes blocked", "production writes blocked"]) {
  if (!twinProjection.includes(marker)) {
    throw new Error(`Twin viewport projection must keep marker: ${marker}`);
  }
}

for (const route of [".", "forge", "learn", "marketplace", "pages", "message", "admin/setup", "you", "admin/developer", "talent", "admin/platform", "admin/start", "proof", "quality", "strategy", "product-direction", "admin/aegis", "admin/charter"]) {
  const source = read(`src/routes/${route}/+page.svelte`);
  for (const marker of ["data-route-state=\"ready\"", "Boundary", "Safe next"]) {
    if (!source.includes(marker)) {
      throw new Error(`${route} route must keep platform marker: ${marker}`);
    }
  }
}

const routes = JSON.parse(readRoot("generated/routes/mdx-local-routes.json"));
const auth = JSON.parse(readRoot("generated/auth/mdx-auth-session-boundary.json"));
if (routes.routes.length < 80) {
  throw new Error("host spike must read the generated local route catalog");
}
if (!auth.required_controls.includes("actor_admission_required_for_mutation")) {
  throw new Error("host spike must prove actor admission requirement from generated auth boundary");
}

const clientRoot = resolve(appRoot, ".svelte-kit/output/client");
if (process.env.REQUIRE_BUILD_BUDGET === "1" && !existsSync(clientRoot)) {
  // The budget silently skipping is how 361KB crept to 1693KB unwatched.
  throw new Error("perf budget check requires build output (.svelte-kit/output/client missing) - run after a build");
}
if (existsSync(clientRoot)) {
  const assetBytes = collectAssets(clientRoot, new Set([".js", ".css"])).reduce((total, asset) => total + asset.bytes, 0);
  const assetKb = Math.round(assetBytes / 1024);
  if (assetKb > perfBudget.max_client_js_css_kb) {
    throw new Error(`host client JS/CSS budget exceeded: ${assetKb}KB > ${perfBudget.max_client_js_css_kb}KB`);
  }
}

console.log(
  `mdx_host_smoke: OK routes=${routes.routes.length} governed_writes=${routes.routes.filter((route) => route.method === "POST" && route.receipt_backed).length}`
);

function collectAssets(dir, extensions) {
  return readdirSync(dir).flatMap((entry) => {
    const path = resolve(dir, entry);
    const stats = statSync(path);
    if (stats.isDirectory()) {
      return collectAssets(path, extensions);
    }
    if (extensions.has(extname(path))) {
      return [{ path, bytes: stats.size }];
    }
    return [];
  });
}
