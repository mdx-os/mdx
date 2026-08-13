#!/usr/bin/env node
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const repoRoot = path.resolve(new URL("..", import.meta.url).pathname);

function parseArgs(argv) {
  const args = {
    auditRoot: "audit/runs",
    baselineRunId: "local-scanner-baseline",
    check: false,
    out: "generated/security/mdx-security-trust.json",
    print: false,
    runId: null
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--audit-root") args.auditRoot = argv[++index];
    else if (arg === "--baseline-run-id") args.baselineRunId = argv[++index];
    else if (arg === "--check") args.check = true;
    else if (arg === "--out") args.out = argv[++index];
    else if (arg === "--print") args.print = true;
    else if (arg === "--run-id") args.runId = argv[++index];
    else throw new Error(`unknown argument: ${arg}`);
  }
  return args;
}

function resolveRepo(relPath) {
  return path.resolve(repoRoot, relPath);
}

function readJson(relPath) {
  return JSON.parse(readFileSync(resolveRepo(relPath), "utf8"));
}

function writeJson(relPath, value) {
  const target = resolveRepo(relPath);
  mkdirSync(path.dirname(target), { recursive: true });
  writeFileSync(target, `${JSON.stringify(value, null, 2)}\n`);
}

function sha256(relPath) {
  return createHash("sha256").update(readFileSync(resolveRepo(relPath))).digest("hex");
}

function sortedObject(input) {
  return Object.fromEntries(Object.entries(input ?? {}).sort(([a], [b]) => a.localeCompare(b)));
}

function zeroSeverityCounts(counts) {
  return {
    critical: Number(counts?.critical ?? 0),
    high: Number(counts?.high ?? 0),
    medium: Number(counts?.medium ?? 0),
    low: Number(counts?.low ?? 0),
    info: Number(counts?.info ?? 0),
    warning: Number(counts?.warning ?? 0),
    unknown: Number(counts?.unknown ?? 0)
  };
}

function latestRunId(auditRoot) {
  const root = resolveRepo(auditRoot);
  if (!existsSync(root)) return null;
  const candidates = readdirSync(root, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .filter((runId) => existsSync(path.join(root, runId, "summary.json")))
    .sort();
  return candidates.at(-1) ?? null;
}

function runRel(auditRoot, runId, file) {
  return path.posix.join(auditRoot.replaceAll(path.sep, "/"), runId, file);
}

function loadRun(auditRoot, runId) {
  const summaryPath = runRel(auditRoot, runId, "summary.json");
  const verdictPath = runRel(auditRoot, runId, "policy-verdict.json");
  const healthPath = runRel(auditRoot, runId, "scanner-health.json");
  const manifestPath = runRel(auditRoot, runId, "manifest.json");
  for (const relPath of [summaryPath, verdictPath, healthPath, manifestPath]) {
    if (!existsSync(resolveRepo(relPath))) {
      throw new Error(`security audit run ${runId} is missing ${relPath}`);
    }
  }
  return {
    runId,
    summary: readJson(summaryPath),
    verdict: readJson(verdictPath),
    health: readJson(healthPath),
    manifest: readJson(manifestPath),
    artifactDigests: {
      manifest_json: sha256(manifestPath),
      scanner_health_json: sha256(healthPath),
      policy_verdict_json: sha256(verdictPath),
      summary_json: sha256(summaryPath)
    }
  };
}

function scannerList(summary) {
  return Object.entries(summary.scanners ?? {})
    .map(([id, scanner]) => ({
      id,
      category: scanner.category ?? "unknown",
      required: scanner.required === true,
      applies: scanner.applies !== false,
      status: scanner.status ?? "unknown"
    }))
    .sort((left, right) => left.id.localeCompare(right.id));
}

function scannerStatuses(run) {
  return scannerList(run.summary).map((scanner) => scanner.status);
}

function hasPlannedScanner(run) {
  return scannerStatuses(run).some((status) => status === "planned");
}

function executedScannerCount(run) {
  return scannerStatuses(run).filter((status) => status !== "planned" && status !== "skipped").length;
}

function policyStatusCounts(verdict) {
  const counts = {};
  for (const finding of verdict.findings ?? []) {
    const status = finding.policy_status ?? "unknown";
    counts[status] = (counts[status] ?? 0) + 1;
  }
  return sortedObject(counts);
}

function sourceIntegrity(run) {
  return {
    completed: Boolean(run.manifest.completed_at),
    dry_run: run.manifest.dry_run === true,
    raw_outputs_committed: run.manifest.raw_outputs_committed === true,
    artifact_digests: run.artifactDigests,
    signature: {
      required: false,
      status: "not_configured",
      note: "The current scanner loop publishes source artifact digests. Add a signature file before changing this to signed."
    }
  };
}

function buildCurrent(run) {
  const statusCounts = policyStatusCounts(run.verdict);
  const findingCount = Number(run.summary.finding_count ?? run.verdict.finding_count ?? 0);
  const accepted = Number(statusCounts.accepted ?? 0);
  return {
    run_id: run.summary.run_id ?? run.runId,
    generated_at: run.summary.generated_at ?? run.verdict.generated_at ?? run.manifest.completed_at ?? null,
    scanners_executed: true,
    status: run.summary.status ?? run.verdict.status ?? "unknown",
    posture_score: Number(run.summary.posture_score ?? 0),
    findings: {
      open: Math.max(0, findingCount - accepted),
      accepted,
      total: findingCount,
      policy_blocked: Number(run.summary.policy_blocked_finding_count ?? run.verdict.policy_blocked_finding_count ?? 0),
      by_severity: zeroSeverityCounts(run.summary.severity_counts ?? run.verdict.severity_counts),
      by_category: sortedObject(run.summary.category_counts ?? run.verdict.category_counts ?? {}),
      by_policy_status: statusCounts
    },
    missing_required_scanners: Array.isArray(run.summary.missing_required_scanners)
      ? [...run.summary.missing_required_scanners].sort()
      : [],
    scanners: scannerList(run.summary),
    source_integrity: sourceIntegrity(run)
  };
}

function buildBaseline(run) {
  const findingCount = Number(run.summary.finding_count ?? run.verdict.finding_count ?? 0);
  return {
    run_id: run.summary.run_id ?? run.runId,
    generated_at: run.summary.generated_at ?? run.verdict.generated_at ?? run.manifest.completed_at ?? null,
    status: run.summary.status ?? run.verdict.status ?? "unknown",
    posture_score: Number(run.summary.posture_score ?? 0),
    findings_open: findingCount,
    findings_total: findingCount,
    by_severity: zeroSeverityCounts(run.summary.severity_counts ?? run.verdict.severity_counts),
    by_category: sortedObject(run.summary.category_counts ?? run.verdict.category_counts ?? {}),
    source_integrity: sourceIntegrity(run)
  };
}

function buildTrust(currentRun, baselineRun) {
  const current = buildCurrent(currentRun);
  const baseline = baselineRun.summary ? buildBaseline(baselineRun) : baselineRun;
  return {
    name: "mdx-security-trust",
    generated_at: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
    current,
    baseline,
    trend: {
      score_delta: current.posture_score - baseline.posture_score,
      findings_closed: Math.max(0, baseline.findings_open - current.findings.total),
      findings_delta: current.findings.total - baseline.findings_open
    },
    publish_rules: {
      dry_run_can_publish_current: false,
      planned_scanner_status_can_publish_current: false,
      public_payload: "aggregate_counts_scanner_statuses_and_source_digests_only"
    }
  };
}

function baselineFromPublishedTrust(outPath, baselineRunId) {
  if (!existsSync(resolveRepo(outPath))) {
    throw new Error(`baseline run ${baselineRunId} is unavailable and ${outPath} does not exist`);
  }
  const existing = readJson(outPath);
  assertSanitized(existing);
  if (existing.baseline?.run_id !== baselineRunId) {
    throw new Error(
      `baseline run ${baselineRunId} is unavailable and ${outPath} pins baseline ${existing.baseline?.run_id ?? "unknown"}`,
    );
  }
  return existing.baseline;
}

function sanitizeString(value) {
  return JSON.stringify(value);
}

function assertSanitized(trust) {
  const encoded = sanitizeString(trust);
  const forbiddenKeys = [
    "raw_output",
    "location",
    "fingerprint",
    "title",
    "line",
    "path",
    "command",
    "accepted_reason",
    "package"
  ];
  for (const key of forbiddenKeys) {
    if (encoded.includes(`"${key}"`)) {
      throw new Error(`public trust artifact contains forbidden detail key: ${key}`);
    }
  }
  const secretPatterns = [
    /sk-ant-api03-[A-Za-z0-9_-]{12,}/,
    /\bxai-[A-Za-z0-9]{24,}/,
    /-----BEGIN [A-Z ]*PRIVATE KEY-----/,
    /\bgh[pousr]_[A-Za-z0-9_]{20,}/
  ];
  for (const pattern of secretPatterns) {
    if (pattern.test(encoded)) {
      throw new Error(`public trust artifact contains secret-shaped content: ${pattern}`);
    }
  }
  if (trust.current?.scanners_executed !== true) {
    throw new Error("public trust artifact must publish only an executed current run");
  }
  if (trust.current?.source_integrity?.dry_run === true) {
    throw new Error("public trust artifact current run must not be a dry run");
  }
  if (trust.current?.source_integrity?.raw_outputs_committed === true) {
    throw new Error("public trust artifact current run must not claim raw outputs are committed");
  }
}

function publish(args) {
  const runId = args.runId ?? latestRunId(args.auditRoot);
  if (!runId) throw new Error(`no security audit run found under ${args.auditRoot}`);
  const currentRun = loadRun(args.auditRoot, runId);
  if (currentRun.manifest.dry_run === true || hasPlannedScanner(currentRun)) {
    console.log(`security_trust_publish: skipped run=${runId} reason=planned_or_dry_run`);
    return { published: false, runId };
  }
  if (!currentRun.manifest.completed_at) {
    throw new Error(`security audit run ${runId} is not completed`);
  }
  if (executedScannerCount(currentRun) === 0) {
    throw new Error(`security audit run ${runId} has no executed scanners`);
  }
  let baselineRun;
  let baselineSource = "audit-run";
  try {
    baselineRun = loadRun(args.auditRoot, args.baselineRunId);
  } catch (error) {
    baselineRun = baselineFromPublishedTrust(args.out, args.baselineRunId);
    baselineSource = "published-trust-baseline";
  }
  const trust = buildTrust(currentRun, baselineRun);
  assertSanitized(trust);
  if (args.check) {
    if (!existsSync(resolveRepo(args.out))) {
      throw new Error(`${args.out} does not exist`);
    }
    const existing = readJson(args.out);
    assertSanitized(existing);
    if (JSON.stringify({ ...existing, generated_at: trust.generated_at }) !== JSON.stringify(trust)) {
      throw new Error(`${args.out} is stale; rerun make security-trust-publish SECURITY_TRUST_ARGS="--run-id ${runId}"`);
    }
  } else {
    writeJson(args.out, trust);
  }
  if (args.print) {
    process.stdout.write(`${JSON.stringify(trust, null, 2)}\n`);
  } else {
    console.log(`security_trust_publish: published run=${runId} baseline=${args.baselineRunId} baseline_source=${baselineSource} out=${args.out}`);
  }
  return { published: true, runId, trust };
}

try {
  publish(parseArgs(process.argv.slice(2)));
} catch (error) {
  console.error(`security_trust_publish: ERROR ${error.message}`);
  process.exit(1);
}
