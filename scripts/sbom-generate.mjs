#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const repoRoot = path.resolve(new URL("..", import.meta.url).pathname);

function parseArgs(argv) {
  const args = {
    dryRun: false,
    runId: new Date().toISOString().replaceAll(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z")
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--dry-run") args.dryRun = true;
    else if (arg === "--run-id") args.runId = argv[++index];
    else throw new Error(`unknown argument: ${arg}`);
  }
  return args;
}

function rel(file) {
  return path.relative(repoRoot, file).replaceAll(path.sep, "/");
}

function writeJson(file, value) {
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

function writeText(file, value) {
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, value);
}

function utcNow() {
  return new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
}

function syftCommand() {
  return process.env.SYFT_BIN || "syft";
}

function syftAvailable(command) {
  return spawnSync(command, ["--version"], {
    cwd: repoRoot,
    stdio: "ignore"
  }).status === 0;
}

function generate(command, format, outFile) {
  const result = spawnSync(command, [
    "dir:.",
    "--exclude",
    "./target",
    "--exclude",
    "./node_modules",
    "--exclude",
    "./.svelte-kit",
    "--exclude",
    "./.mdx-local",
    "--exclude",
    "./audit/runs",
    "-o",
    format
  ], {
    cwd: repoRoot,
    encoding: "utf8",
    maxBuffer: 1024 * 1024 * 100
  });
  if (result.status !== 0) {
    throw new Error(`syft ${format} failed: ${result.stderr || result.stdout}`);
  }
  writeText(outFile, result.stdout);
}

function readCycloneDxSummary(file) {
  if (!existsSync(file)) return { component_count: 0 };
  const bom = JSON.parse(readFileSync(file, "utf8"));
  return {
    bom_format: bom.bomFormat,
    spec_version: bom.specVersion,
    component_count: Array.isArray(bom.components) ? bom.components.length : 0
  };
}

function markdown(summary) {
  return [
    "# SBOM Summary",
    "",
    `- Run ID: \`${summary.run_id}\``,
    `- Status: \`${summary.status}\``,
    `- Tool: \`${summary.tool}\``,
    `- CycloneDX components: \`${summary.cyclonedx.component_count}\``,
    `- CycloneDX file: \`${summary.outputs.cyclonedx_json}\``,
    `- SPDX file: \`${summary.outputs.spdx_json}\``,
    "",
    "SBOMs are generated from the repository filesystem with build outputs, local runtime state, and audit artifacts excluded."
  ].join("\n") + "\n";
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const runDir = path.join(repoRoot, "audit", "runs", args.runId);
  const cyclonedxFile = path.join(runDir, "sbom.cyclonedx.json");
  const spdxFile = path.join(runDir, "sbom.spdx.json");
  const command = syftCommand();

  const manifest = {
    name: "mdx-sbom",
    run_id: args.runId,
    generated_at: utcNow(),
    dry_run: args.dryRun,
    tool: "syft",
    source: "dir:.",
    outputs: {
      cyclonedx_json: rel(cyclonedxFile),
      spdx_json: rel(spdxFile)
    }
  };
  writeJson(path.join(runDir, "sbom-manifest.json"), manifest);

  if (args.dryRun) {
    const summary = { ...manifest, status: "planned", cyclonedx: { component_count: 0 } };
    writeJson(path.join(runDir, "sbom-summary.json"), summary);
    writeText(path.join(runDir, "sbom-summary.md"), markdown(summary));
    console.log(`sbom_generate: planned run=${args.runId}`);
    return;
  }

  if (!syftAvailable(command)) {
    throw new Error(`syft unavailable: ${command}`);
  }

  generate(command, "cyclonedx-json", cyclonedxFile);
  generate(command, "spdx-json", spdxFile);
  const summary = {
    ...manifest,
    status: "generated",
    cyclonedx: readCycloneDxSummary(cyclonedxFile)
  };
  writeJson(path.join(runDir, "sbom-summary.json"), summary);
  writeText(path.join(runDir, "sbom-summary.md"), markdown(summary));
  console.log(`sbom_generate: generated run=${args.runId} components=${summary.cyclonedx.component_count}`);
}

try {
  main();
} catch (error) {
  console.error(`sbom_generate: ERROR ${error.message}`);
  process.exit(1);
}
