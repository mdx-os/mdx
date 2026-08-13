// P0 of the polish program: photograph the WHOLE house - every member
// surface and every console room - against the live stack, one frame per
// route, desktop viewport, both color schemes. The output is the
// baseline the 12-persona panel audits. One-off program tooling, not a
// gate; the gate camera (screenshot-proof.mjs) keeps its own contract.
import { chromium } from "@playwright/test";
import { mkdirSync, writeFileSync, readFileSync } from "node:fs";

const base = process.env.MDX_UI_BASE ?? "http://127.0.0.1:5200";
const out = process.env.MDX_P0_OUT ?? "../../.mdx-local/polish-p0/frames";
const routes = JSON.parse(readFileSync("/tmp/p0-routes.json", "utf8"));
const all = [...routes.member.map((p) => ({ p, side: "member" })), ...routes.admin.map((p) => ({ p, side: "console" }))];

mkdirSync(out, { recursive: true });
const browser = await chromium.launch();
const report = [];
for (const scheme of ["light", "dark"]) {
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    colorScheme: scheme
  });
  const page = await context.newPage();
  for (const { p, side } of all) {
    const id = (p === "/" ? "home-root" : p.slice(1).replaceAll("/", "_"));
    try {
      const response = await page.goto(`${base}${p}`, { waitUntil: "networkidle", timeout: 20000 });
      await page.waitForTimeout(400);
      const file = `${out}/${side}_${id}_${scheme}.png`;
      await page.screenshot({ path: file, fullPage: true });
      const title = await page.title();
      const h1 = await page.locator("h1").first().textContent({ timeout: 1500 }).catch(() => "");
      report.push({ route: p, side, scheme, status: response?.status() ?? 0, title, h1: (h1 ?? "").trim(), file });
      console.log(`${side} ${p} [${scheme}] -> ${response?.status()} h1=${(h1 ?? "").trim().slice(0, 40)}`);
    } catch (error) {
      report.push({ route: p, side, scheme, status: -1, error: String(error).slice(0, 120) });
      console.log(`${side} ${p} [${scheme}] -> FAILED ${String(error).slice(0, 80)}`);
    }
  }
  await context.close();
}
await browser.close();
writeFileSync(`${out}/../walk-report.json`, JSON.stringify(report, null, 2));
const failed = report.filter((r) => r.status !== 200);
console.log(`\nframes: ${report.length}, non-200: ${failed.length}`);
