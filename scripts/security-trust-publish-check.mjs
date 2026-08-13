#!/usr/bin/env node
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import process from "node:process";

const repoRoot = path.resolve(new URL("..", import.meta.url).pathname);
const scratch = path.join(repoRoot, ".mdx-local", "security-trust-publish-check");
const tempOut = path.join(".mdx-local", "security-trust-publish-check", "mdx-security-trust.json");
const tempAuditRoot = path.join(".mdx-local", "security-trust-publish-check", "audit-runs");

function run(args) {
  const result = spawnSync("node", ["scripts/security-trust-publish.mjs", ...args], {
    cwd: repoRoot,
    encoding: "utf8"
  });
  if (result.status !== 0) {
    throw new Error(`${args.join(" ")} failed\n${result.stdout}${result.stderr}`);
  }
  return result;
}

function runAudit(args) {
  const result = spawnSync("node", ["scripts/security-audit.mjs", ...args], {
    cwd: repoRoot,
    encoding: "utf8"
  });
  if (result.status !== 0) {
    throw new Error(`security-audit ${args.join(" ")} failed\n${result.stdout}${result.stderr}`);
  }
  return result;
}

function hash(relPath) {
  return createHash("sha256").update(readFileSync(path.join(repoRoot, relPath))).digest("hex");
}

function readJson(relPath) {
  return JSON.parse(readFileSync(path.join(repoRoot, relPath), "utf8"));
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function assertNoPublicDetail(value) {
  const encoded = JSON.stringify(value);
  for (const key of ["raw_output", "location", "fingerprint", "title", "line", "path", "command"]) {
    assert(!encoded.includes(`"${key}"`), `trust artifact leaked ${key}`);
  }
  assert(!/sk-ant-api03-[A-Za-z0-9_-]{12,}/.test(encoded), "trust artifact leaked Anthropic-shaped key");
  assert(!/\bxai-[A-Za-z0-9]{24,}/.test(encoded), "trust artifact leaked xAI-shaped key");
  assert(!/-----BEGIN [A-Z ]*PRIVATE KEY-----/.test(encoded), "trust artifact leaked private-key marker");
}

try {
  rmSync(scratch, { recursive: true, force: true });
  mkdirSync(scratch, { recursive: true });

  run(["--run-id", "local-scanner-final-pass", "--out", tempOut]);
  const trust = readJson(tempOut);
  assert(trust.name === "mdx-security-trust", "unexpected trust artifact name");
  assert(trust.current?.scanners_executed === true, "executed run did not publish scanners_executed=true");
  assert(trust.current?.run_id === "local-scanner-final-pass", "unexpected current run id");
  assert(trust.baseline?.run_id === "local-scanner-baseline", "unexpected baseline run id");
  assert(trust.current?.posture_score === 100, "expected final pass posture score to be 100");
  assert(trust.current?.findings?.policy_blocked === 0, "expected no policy-blocking findings");
  assert((trust.current?.missing_required_scanners ?? []).length === 0, "expected no missing required scanners");
  assert(trust.trend?.findings_closed > 0, "expected baseline-to-current finding closure trend");
  assertNoPublicDetail(trust);

  const before = hash(tempOut);
  run(["--run-id", "scanner-readiness", "--out", tempOut]);
  const after = hash(tempOut);
  assert(before === after, "planned dry-run changed the published trust artifact");

  cpSync(
    path.join(repoRoot, "audit", "runs", "local-scanner-final-pass"),
    path.join(repoRoot, tempAuditRoot, "local-scanner-final-pass"),
    { recursive: true },
  );
  run(["--audit-root", tempAuditRoot, "--run-id", "local-scanner-final-pass", "--out", tempOut]);
  const ciLikeTrust = readJson(tempOut);
  assert(ciLikeTrust.baseline?.run_id === "local-scanner-baseline", "missing baseline fallback did not keep pinned baseline");
  assert(ciLikeTrust.current?.run_id === "local-scanner-final-pass", "missing baseline fallback changed current run");

  if (existsSync(path.join(repoRoot, "generated", "security", "mdx-security-trust.json"))) {
    run(["--run-id", "local-scanner-final-pass", "--check"]);
    const committedBefore = hash("generated/security/mdx-security-trust.json");
    runAudit(["--dry-run", "--run-id", "security-trust-dry-run-check"]);
    const committedAfter = hash("generated/security/mdx-security-trust.json");
    assert(committedBefore === committedAfter, "security-audit dry-run changed the committed trust artifact");
  }

  console.log("security_trust_publish_check: OK executed_publish dry_run_noop sanitized_public_artifact");
} catch (error) {
  console.error(`security_trust_publish_check: FAIL - ${error.message}`);
  process.exit(1);
}
