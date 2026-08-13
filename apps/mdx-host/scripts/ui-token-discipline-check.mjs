#!/usr/bin/env node
// UI token discipline ratchet. A well-designed token system exists
// (generated/ui/mdx-theme.css); this keeps surfaces from drifting back to raw
// values. It counts raw hex colors and px font-sizes inside <style> blocks
// under src/routes and fails when the total rises above a recorded baseline,
// so NEW violations are blocked while the existing long tail is paid down over
// time rather than in one risky sweep. Run with --update to record the current
// counts as the new baseline after an intentional reduction.
import { readFileSync, writeFileSync, readdirSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const routesDir = join(here, "..", "src", "routes");
const baselinePath = join(here, "ui-token-discipline-baseline.json");

// Raw 3/4/6/8-digit hex colors, and px font sizes. We only look inside <style>
// blocks so a hex in a data URI or a comment in script does not count.
const HEX = /#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/g;
const PX_FONT = /font-size:\s*-?\d*\.?\d+px/g;

function walk(dir) {
  const out = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) out.push(...walk(full));
    else if (full.endsWith(".svelte")) out.push(full);
  }
  return out;
}

function styleBlocks(source) {
  const blocks = [];
  const re = /<style[^>]*>([\s\S]*?)<\/style>/g;
  let m;
  while ((m = re.exec(source))) blocks.push(m[1]);
  return blocks.join("\n");
}

function countViolations() {
  let hex = 0;
  let pxFont = 0;
  const perFile = {};
  for (const file of walk(routesDir)) {
    const css = styleBlocks(readFileSync(file, "utf8"));
    const h = (css.match(HEX) ?? []).length;
    const p = (css.match(PX_FONT) ?? []).length;
    if (h || p) perFile[file.slice(file.indexOf("src/"))] = { hex: h, pxFont: p };
    hex += h;
    pxFont += p;
  }
  return { hex, pxFont, perFile };
}

const current = countViolations();
const update = process.argv.includes("--update");

if (update) {
  writeFileSync(
    baselinePath,
    JSON.stringify({ hex: current.hex, pxFont: current.pxFont }, null, 2) + "\n"
  );
  console.log(`ui-token-discipline: baseline recorded (hex=${current.hex}, pxFont=${current.pxFont}).`);
  process.exit(0);
}

let baseline;
try {
  baseline = JSON.parse(readFileSync(baselinePath, "utf8"));
} catch {
  console.error("ui-token-discipline: no baseline found. Run with --update to record one.");
  process.exit(1);
}

const hexOver = current.hex - baseline.hex;
const pxOver = current.pxFont - baseline.pxFont;
if (hexOver > 0 || pxOver > 0) {
  console.error("ui-token-discipline: NEW raw-value violations introduced.");
  if (hexOver > 0) console.error(`  raw hex colors: ${current.hex} (baseline ${baseline.hex}, +${hexOver})`);
  if (pxOver > 0) console.error(`  px font-sizes: ${current.pxFont} (baseline ${baseline.pxFont}, +${pxOver})`);
  console.error("  Use the generated tokens (--mdx-* colors, --mdx-text-* sizes) instead of raw values.");
  console.error("  If a reduction is intended, run: node apps/mdx-host/scripts/ui-token-discipline-check.mjs --update");
  process.exit(1);
}

console.log(
  `ui-token-discipline: OK (hex=${current.hex} <= ${baseline.hex}, pxFont=${current.pxFont} <= ${baseline.pxFont}).`
);
