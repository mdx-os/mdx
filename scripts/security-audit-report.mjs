#!/usr/bin/env node
import { existsSync, readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const repoRoot = path.resolve(new URL("..", import.meta.url).pathname);
const runsRoot = path.join(repoRoot, "audit", "runs");

function parseArgs(argv) {
  const args = { runId: null, printSummary: false };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--run-id") args.runId = argv[++index];
    else if (arg === "--print-summary") args.printSummary = true;
    else throw new Error(`unknown argument: ${arg}`);
  }
  return args;
}

function latestRunId() {
  if (!existsSync(runsRoot)) return null;
  const runs = readdirSync(runsRoot, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort();
  return runs.at(-1) ?? null;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const runId = args.runId ?? latestRunId();
  if (!runId) throw new Error("no security audit run found");
  const runDir = path.join(runsRoot, runId);
  const summaryPath = path.join(runDir, "summary.md");
  const verdictPath = path.join(runDir, "policy-verdict.json");
  if (!existsSync(summaryPath) || !existsSync(verdictPath)) {
    throw new Error(`security audit artifacts are incomplete for ${runId}`);
  }
  const verdict = JSON.parse(readFileSync(verdictPath, "utf8"));
  if (args.printSummary) {
    process.stdout.write(readFileSync(summaryPath, "utf8"));
  } else {
    console.log(`security_audit_report: ${verdict.status} run=${runId} summary=${path.relative(repoRoot, summaryPath)}`);
  }
  process.exitCode = verdict.status === "policy_blocked" ? 1 : 0;
}

try {
  main();
} catch (error) {
  console.error(`security_audit_report: ERROR ${error.message}`);
  process.exit(1);
}
