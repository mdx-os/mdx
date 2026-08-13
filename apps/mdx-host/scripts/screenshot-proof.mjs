// Visual proof of the live product: boots the real kernel and the real
// host, walks the product routes in both viewports and both color schemes,
// refuses any banned visible label, and writes hashed screenshot evidence.
// This replaces the gen-1 screenshot rail as the primary visual proof
// (ADR 0478 named this follow-up): the camera now points at the product.
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { chromium } from "@playwright/test";

const root = resolve(import.meta.dirname, "../../..");
const hostDir = resolve(root, "apps/mdx-host");
const evidenceDir = resolve(root, ".mdx-local/ui-host-screenshots");
const kernelAddr = "127.0.0.1:18879";
const hostPort = 5582;

// requiredVisible is the cold-read floor: a fresh reader must be able to
// tell what each room is for from its rendered words alone, so the words
// that carry each surface's job are asserted on the live page - the same
// walk that refuses banned labels.
const routes = [
  { id: "home", path: "/" },
  {
    id: "twin",
    path: "/twin",
    requiredVisible: ["Twin", "Help me think something through"]
  },
  { id: "forge", path: "/forge", requiredVisible: ["Nothing ships"] },
  { id: "forge-runs", path: "/forge/runs", requiredVisible: ["Runs", "Every build Forge has run"] },
  { id: "forge-review", path: "/forge/review", requiredVisible: ["Review", "Changes waiting for your call"] },
  { id: "forge-models", path: "/forge/models", requiredVisible: ["Models", "Models that run Forge"] },
  { id: "forge-controls", path: "/forge/controls", requiredVisible: ["Controls"] },
  { id: "studio", path: "/studio", requiredVisible: ["Start the run"] },
  { id: "pages", path: "/pages", requiredVisible: ["What your company knows", "Add source", "Search your pages"] },
  {
    id: "memory",
    path: "/memory",
    requiredVisible: ["What MDx has learned", "What each surface recorded", "What the work proved about the models"]
  },
  {
    id: "message",
    path: "/message",
    requiredVisible: ["The team's channels", "Approve the Forge plan", "Action result", "Decline", "Respond"]
  },
  { id: "marketplace", path: "/marketplace" },
  { id: "strategy", path: "/strategy", requiredVisible: ["Where the company is heading"] },
  { id: "product", path: "/product-direction", requiredVisible: ["What we are building"] },
  { id: "talent", path: "/talent", requiredVisible: ["Everyone who works here"] },
  { id: "proof", path: "/proof" },
  {
    id: "console-models",
    path: "/admin/models",
    requiredVisible: ["Connect models once", "Your models", "Connect a model", "How work is routed"]
  },
  { id: "you", path: "/you" }
];
const viewports = [
  { id: "desktop", width: 1280, height: 900 },
  { id: "mobile", width: 390, height: 844 }
];
const schemes = ["light", "dark"];

// The doctrine's banned visible labels plus internal identity literals.
// Closed <details> content is not rendered text, so quiet disclosures pass.
const forbiddenVisibleTerms = [
  "Activation Report",
  "Local API Index",
  "Pending Live Substrates",
  "Runtime Status",
  "Source Receipts",
  "generated fallback",
  "local_tenant",
  "deterministic-local-stub"
];

const waitForHttp = async (url, attempts = 60) => {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch {
      // not up yet
    }
    await new Promise((resolveSleep) => setTimeout(resolveSleep, 1000));
  }
  throw new Error(`server at ${url} did not come up`);
};

rmSync(evidenceDir, { recursive: true, force: true });
mkdirSync(evidenceDir, { recursive: true });

const kernel = spawn("cargo", ["run", "-q", "-p", "mdx-server", "--", "serve", kernelAddr], {
  cwd: root,
  stdio: "ignore"
});
const host = spawn(
  "node",
  ["node_modules/vite/bin/vite.js", "dev", "--port", String(hostPort), "--strictPort"],
  {
    cwd: hostDir,
    stdio: "ignore",
    env: { ...process.env, MDX_LOCAL_API_URL: `http://${kernelAddr}` }
  }
);
const shutdown = () => {
  kernel.kill("SIGKILL");
  host.kill("SIGKILL");
};
process.on("exit", shutdown);

try {
  await waitForHttp(`http://${kernelAddr}/health`);
  await seedMessageActionProof();
  await waitForHttp(`http://127.0.0.1:${hostPort}/`);

  const browser = await chromium.launch();
  const shots = [];
  const violations = [];
  for (const scheme of schemes) {
    for (const viewport of viewports) {
      const context = await browser.newContext({
        viewport: { width: viewport.width, height: viewport.height },
        colorScheme: scheme
      });
      const page = await context.newPage();
      let currentRoute = "/";
      const runtimeErrors = [];
      page.on("pageerror", (error) => {
        runtimeErrors.push(`${currentRoute}: ${error.message}`);
      });
      page.on("console", (message) => {
        if (message.type() !== "error") return;
        const error = message.text();
        // Realtime is an optional local companion service, not part of this
        // product-shell proof. Keep page exceptions and every other console
        // error authoritative without turning its absent socket into a false
        // UI regression.
        if (error.includes("ws://127.0.0.1:9000/messages/realtime/ws")) return;
        runtimeErrors.push(`${currentRoute}: ${error}`);
      });
      for (const route of routes) {
        currentRoute = route.path;
        runtimeErrors.length = 0;
        if (route.id === "message") {
          await page.addInitScript(() => {
            localStorage.setItem("mdx-message-channel", "forge");
          });
        }
        await page.goto(`http://127.0.0.1:${hostPort}${route.path}`, {
          waitUntil: "networkidle",
          timeout: 30000
        });
        for (const error of [...new Set(runtimeErrors)]) {
          violations.push(`${route.path} [${viewport.id}/${scheme}] raised a browser error: ${error}`);
        }
        const visibleText = await page.evaluate(() => document.body.innerText);
        for (const term of forbiddenVisibleTerms) {
          if (visibleText.includes(term)) {
            violations.push(`${route.path} [${viewport.id}/${scheme}] shows banned label: ${term}`);
          }
        }
        const visibleLower = visibleText.toLowerCase();
        for (const promise of route.requiredVisible ?? []) {
          // Case-insensitive: CSS text-transform changes rendered case and a
          // cold reader does not care.
          if (!visibleLower.includes(promise.toLowerCase())) {
            violations.push(
              `${route.path} [${viewport.id}/${scheme}] fails the cold read: a fresh reader cannot see "${promise}"`
            );
          }
        }
        if (route.id === "forge") {
          const forgeOverflow = await page.evaluate(() => {
            const forge = document.querySelector(".forge");
            if (!forge) return [];
            const viewportRight = document.documentElement.clientWidth;
            const forgeRight = forge.getBoundingClientRect().right;
            return Array.from(document.querySelectorAll(".forge > *, .forge .forge-start, .forge .fs-intent, .forge .fs-bar-go"))
              .map((element) => {
                const rect = element.getBoundingClientRect();
                return {
                  selector: element.className || element.tagName.toLowerCase(),
                  right: Math.round(rect.right),
                  width: Math.round(rect.width)
                };
              })
              .filter((item) => item.width > 0 && item.right > Math.min(viewportRight, forgeRight) + 6)
              .slice(0, 5);
          });
          for (const offender of forgeOverflow) {
            violations.push(
              `/forge [${viewport.id}/${scheme}] clips ${offender.selector} at right=${offender.right} width=${offender.width}`
            );
          }
          const forgeInternalOverflow = await page.evaluate(() =>
            Array.from(document.querySelectorAll(".forge-start, .fs-recent, .fs-recent-list, .fs-recent-row"))
              .map((element) => ({
                selector: element.className || element.tagName.toLowerCase(),
                scrollWidth: Math.round(element.scrollWidth),
                clientWidth: Math.round(element.clientWidth)
              }))
              .filter((item) => item.clientWidth > 0 && item.scrollWidth > item.clientWidth + 2)
              .slice(0, 5)
          );
          for (const offender of forgeInternalOverflow) {
            violations.push(
              `/forge [${viewport.id}/${scheme}] internally overflows ${offender.selector} (${offender.scrollWidth}px > ${offender.clientWidth}px)`
            );
          }
        }
        if (route.id === "memory") {
          const memoryGroups = await page.evaluate(() =>
            Array.from(document.querySelectorAll(".surface-group")).map((group) => {
              const preview = group.querySelector(":scope > .record-list");
              const waiting = preview
                ? Array.from(preview.children).filter((record) => record.textContent?.includes("not used until you clear it")).length
                : 0;
              const history = group.querySelector(":scope > details .record-list");
              return {
                surface: group.querySelector("h3")?.textContent?.trim() || "unknown",
                previewCount: preview?.children.length ?? 0,
                historyCount: history?.children.length ?? 0,
                waiting
              };
            })
          );
          for (const group of memoryGroups) {
            if (group.previewCount > Math.max(4, group.waiting)) {
              violations.push(
                `/memory [${viewport.id}/${scheme}] exposes ${group.previewCount} ${group.surface} records before history disclosure`
              );
            }
            if (group.historyCount > 0 && group.previewCount === 0) {
              violations.push(`/memory [${viewport.id}/${scheme}] hides all current ${group.surface} records in history`);
            }
          }
        }
        if (viewport.id === "desktop") {
          const shellGeometry = await page.evaluate(() => {
            const banner = document.querySelector(".host-kernel-banner");
            const rail = document.querySelector(".host-rail");
            const stage = document.querySelector(".host-stage");
            return {
              bannerHeight: banner?.getBoundingClientRect().height ?? 0,
              bannerHasContent: Boolean(banner?.querySelector(".kbanner")),
              railTop: rail?.getBoundingClientRect().top ?? null,
              stageTop: stage?.getBoundingClientRect().top ?? null
            };
          });
          if (!shellGeometry.bannerHasContent && shellGeometry.bannerHeight > 1) {
            violations.push(
              `${route.path} [${viewport.id}/${scheme}] stretches the empty kernel row to ${Math.round(shellGeometry.bannerHeight)}px`
            );
          }
          if (!shellGeometry.bannerHasContent && shellGeometry.railTop !== null && shellGeometry.railTop > 1) {
            violations.push(
              `${route.path} [${viewport.id}/${scheme}] shifts the product rail down ${Math.round(shellGeometry.railTop)}px`
            );
          }
          if (shellGeometry.stageTop !== null && shellGeometry.railTop !== null && Math.abs(shellGeometry.stageTop - shellGeometry.railTop) > 1) {
            violations.push(
              `${route.path} [${viewport.id}/${scheme}] misaligns the stage and rail (${Math.round(shellGeometry.stageTop)}px vs ${Math.round(shellGeometry.railTop)}px)`
            );
          }
        }
        const file = resolve(evidenceDir, `${route.id}-${viewport.id}-${scheme}.png`);
        await page.screenshot({ path: file, fullPage: false });
        shots.push({
          route: route.path,
          viewport: viewport.id,
          scheme,
          file: `${route.id}-${viewport.id}-${scheme}.png`,
          sha256: createHash("sha256").update(readFileSync(file)).digest("hex")
        });
      }

      // Route-by-route screenshots alone missed the real regression. Exercise
      // the shared internal scroller and Forge tabs at the tall viewport where
      // the empty grid row previously absorbed hundreds of pixels.
      if (viewport.id === "desktop" && scheme === "dark") {
        await page.setViewportSize({ width: 1478, height: 1249 });
        currentRoute = "/forge";
        runtimeErrors.length = 0;
        await page.goto(`http://127.0.0.1:${hostPort}/forge`, { waitUntil: "networkidle", timeout: 30000 });
        const scrolledFrom = await page.evaluate(() => {
          const stage = document.querySelector(".host-stage");
          if (!stage) return 0;
          stage.scrollTop = stage.scrollHeight;
          return stage.scrollTop;
        });
        if (scrolledFrom <= 0) violations.push("/forge [large/dark] could not exercise the shell scroll reset");

        currentRoute = "/forge/runs";
        runtimeErrors.length = 0;
        await page.locator('a[href="/forge/runs"]').first().click();
        await page.waitForURL("**/forge/runs");
        await page.locator("h1", { hasText: "Runs" }).waitFor();
        const runsTop = await page.locator(".host-stage").evaluate((stage) => stage.scrollTop);
        if (runsTop !== 0) violations.push(`/forge/runs [large/dark] inherited ${runsTop}px of shell scroll`);

        currentRoute = "/forge/models";
        runtimeErrors.length = 0;
        await page.locator('a[href="/forge/models"]').first().click();
        await page.waitForURL("**/forge/models");
        const modelsLoaded = await page
          .locator("h1", { hasText: "Models" })
          .waitFor({ timeout: 10000 })
          .then(() => true)
          .catch(() => false);
        if (!modelsLoaded) {
          const visibleText = (await page.locator("body").innerText()).replace(/\s+/g, " ").slice(0, 240);
          violations.push(`/forge/models [large/dark] did not render after client navigation: ${visibleText || "blank page"}`);
        }
        await page.locator(".host-stage").evaluate((stage) => {
          stage.scrollTop = stage.scrollHeight;
        });
        currentRoute = "/forge/runs?run=scroll-reset-probe";
        await page.goto(`http://127.0.0.1:${hostPort}${currentRoute}`, { waitUntil: "networkidle", timeout: 30000 });
        await page.locator("h1", { hasText: "Runs" }).waitFor();
        const deepLinkTop = await page.locator(".host-stage").evaluate((stage) => stage.scrollTop);
        if (deepLinkTop !== 0) violations.push(`${currentRoute} [large/dark] inherited ${deepLinkTop}px of shell scroll`);
        for (const error of [...new Set(runtimeErrors)]) {
          violations.push(`/forge/models [large/dark] raised a browser error during navigation: ${error}`);
        }
      }
      await context.close();
    }
  }
  await browser.close();

  if (violations.length > 0) {
    for (const violation of violations) {
      console.error(`mdx_host_screenshot_proof: ${violation}`);
    }
    process.exit(1);
  }

  writeFileSync(
    resolve(evidenceDir, "evidence.json"),
    JSON.stringify(
      {
        name: "mdx-host-screenshot-proof",
        kernel: "live-local",
        routes: routes.map((route) => route.path),
        screenshot_count: shots.length,
        screenshots: shots
      },
      null,
      2
    )
  );
  console.log(
    `mdx_host_screenshot_proof: OK routes=${routes.length} screenshots=${shots.length} banned_labels=0 cold_read=held kernel=live-local`
  );
} finally {
  shutdown();
}

async function seedMessageActionProof() {
  const base = `http://${kernelAddr}`;
  const writeJson = async (path, body) => {
    const response = await fetch(`${base}${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body)
    });
    if (!response.ok) {
      throw new Error(`seed ${path} failed: ${response.status} ${await response.text()}`);
    }
    return response.json();
  };
  const request = await writeJson("/messages/action-requests.json", {
    action_request_id: "message_screenshot_action_gate",
    actor_id: "agent:forge",
    thread_id: "local",
    channel_id: "forge",
    title: "Approve the Forge plan",
    summary: "A governed action needs your call.",
    proposed_action: "Approve the Message vNext governed action-card proof.",
    action_payload: "{\"plan_hash\":\"plan-message-screenshot-action-gate\"}"
  });
  if (!request?.action_request_receipt_id) throw new Error("seed action request did not return a receipt");
  await writeJson("/messages/action-verdicts.json", {
    action_verdict_id: "message_screenshot_action_gate_approve",
    action_request_receipt_id: request.action_request_receipt_id,
    verdict: "approve",
    decision_note: "Approved from screenshot proof. Execution remains blocked."
  });
  await writeJson("/messages/action-requests.json", {
    action_request_id: "message_screenshot_pending_action_gate",
    actor_id: "agent:forge",
    thread_id: "local",
    channel_id: "forge",
    title: "Approve the Forge plan",
    summary: "Keep the pending Message action card visible for proof.",
    proposed_action: "Review the pending Message vNext action-card proof.",
    action_payload: "{\"plan_hash\":\"plan-message-screenshot-pending-action-gate\"}"
  });

  // Keep enough shared surface history to prove Memory remains bounded under
  // real retained records. The author and reviewer are deliberately distinct,
  // preserving the same human-ratification boundary as production flows.
  const memoryPrefix = "Screenshot scale memory";
  for (let index = 1; index <= 12; index += 1) {
    await writeJson("/messages/thread-messages.json", {
      actor_id: "human:screenshot_seed",
      thread_id: "screenshot-memory-scale",
      channel_id: "local-ops",
      message_id: `screenshot_memory_${index}`,
      body: `${memoryPrefix} ${index}`
    });
  }
  const projectionResponse = await fetch(`${base}/memory/consolidation-ratifications/projection.json`);
  if (!projectionResponse.ok) throw new Error(`seed memory projection failed: ${projectionResponse.status}`);
  const projection = await projectionResponse.json();
  const pending = (projection.pending ?? []).filter((record) => record.content?.startsWith(memoryPrefix));
  if (pending.length !== 12) throw new Error(`seed memory expected 12 pending records, found ${pending.length}`);
  for (const record of pending) {
    const ratification = await writeJson("/memory/consolidation-ratifications.json", {
      memory_id: record.memory_id,
      decision: "approve",
      note: "Retained for screenshot scale proof."
    });
    if (ratification.status !== "RECORDED") throw new Error(`seed memory ratification failed for ${record.memory_id}`);
  }
}
