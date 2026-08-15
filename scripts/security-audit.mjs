#!/usr/bin/env node
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const repoRoot = path.resolve(new URL("..", import.meta.url).pathname);
const defaultPolicyPath = path.join(repoRoot, "security", "scanner-policy.json");

function parseArgs(argv) {
  const args = {
    dryRun: false,
    nightly: false,
    noTrustPublish: false,
    printSummary: false,
    policyPath: defaultPolicyPath,
    runId: new Date().toISOString().replaceAll(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z")
  };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--dry-run") args.dryRun = true;
    else if (arg === "--nightly") args.nightly = true;
    else if (arg === "--no-trust-publish") args.noTrustPublish = true;
    else if (arg === "--print-summary") args.printSummary = true;
    else if (arg === "--policy") args.policyPath = path.resolve(argv[++index]);
    else if (arg === "--run-id") args.runId = argv[++index];
    else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  return args;
}

function readJson(file) {
  return JSON.parse(readFileSync(file, "utf8"));
}

function writeJson(file, value) {
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, `${JSON.stringify(value, null, 2)}\n`);
}

function writeText(file, value) {
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, value);
}

function rel(file) {
  return path.relative(repoRoot, file).replaceAll(path.sep, "/");
}

function utcNow() {
  return new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
}

function commandExists(command) {
  if (!command) return false;
  if (process.platform === "win32") {
    return spawnSync("where", [command], { stdio: "ignore" }).status === 0;
  }
  return spawnSync("sh", ["-c", `command -v "${command.replaceAll('"', '\\"')}" >/dev/null 2>&1`], { stdio: "ignore" }).status === 0;
}

function commandProbePasses(probe) {
  if (!Array.isArray(probe) || probe.length === 0) return false;
  if (!commandExists(probe[0])) return false;
  return spawnSync(probe[0], probe.slice(1), {
    cwd: repoRoot,
    stdio: "ignore"
  }).status === 0;
}

function filesExist(files) {
  return files.every((file) => existsSync(path.join(repoRoot, file)));
}

function scannerApplies(scanner) {
  const requiredFiles = scanner.applies_when?.files_exist ?? [];
  if (requiredFiles.length > 0 && !filesExist(requiredFiles)) {
    return [false, "required files not present"];
  }
  return [true, null];
}

function outputPath(rawDir, scannerName) {
  return path.join(rawDir, `${scannerName}.json`);
}

function commandForScanner(scannerName, scanner, rawDir) {
  const command = [...(scanner.command ?? [])];
  const out = outputPath(rawDir, scannerName);
  if (scannerName === "gitleaks" && !command.includes("--report-path")) {
    command.push("--report-path", out);
  }
  if (scannerName === "semgrep" && !command.includes("--output")) {
    command.push("--output", out);
  }
  if (scannerName === "trivy-fs" && !command.includes("--output")) {
    command.push("--output", out);
  }
  if (scannerName === "osv-scanner" && !command.includes("--output")) {
    command.push("--output", out);
  }
  return command;
}

function runScanner(runDir, rawDir, scannerName, scanner, dryRun) {
  const [applies, reason] = scannerApplies(scanner);
  const command = commandForScanner(scannerName, scanner, rawDir);
  const startedAt = utcNow();
  const result = {
    scanner: scannerName,
    category: scanner.category ?? "unknown",
    required: Boolean(scanner.required),
    block_on_unavailable: Boolean(scanner.block_on_unavailable),
    applies,
    command,
    status: "skipped",
    reason,
    exit_code: null,
    started_at: startedAt,
    ended_at: startedAt,
    duration_seconds: 0,
    raw_output: rel(outputPath(rawDir, scannerName))
  };

  if (!applies) return result;
  if (dryRun) {
    result.status = "planned";
    result.reason = "dry run";
    result.ended_at = utcNow();
    return result;
  }
  const availabilityProbe = scanner.availability_command ?? [command[0]];
  if (!commandProbePasses(availabilityProbe)) {
    result.status = "unavailable";
    result.reason = `scanner unavailable: ${availabilityProbe.join(" ")}`;
    result.ended_at = utcNow();
    return result;
  }

  const started = performance.now();
  const completed = spawnSync(command[0], command.slice(1), {
    cwd: repoRoot,
    encoding: "utf8",
    maxBuffer: 1024 * 1024 * 50,
    timeout: (scanner.timeout_seconds ?? 300) * 1000
  });
  const stdoutPath = path.join(rawDir, `${scannerName}.stdout.txt`);
  const stderrPath = path.join(rawDir, `${scannerName}.stderr.txt`);
  writeText(stdoutPath, completed.stdout ?? "");
  writeText(stderrPath, completed.stderr ?? "");
  const stdout = (completed.stdout ?? "").trimStart();
  if (!existsSync(outputPath(rawDir, scannerName)) && (stdout.startsWith("{") || stdout.startsWith("["))) {
    writeText(outputPath(rawDir, scannerName), completed.stdout);
  }

  result.exit_code = completed.status;
  result.ended_at = utcNow();
  result.duration_seconds = Number(((performance.now() - started) / 1000).toFixed(3));
  result.status =
    completed.error?.code === "ETIMEDOUT"
      ? scanner.block_on_unavailable
        ? "timed_out"
        : "unavailable"
      : completed.status === 0
        ? "passed"
        : "completed_nonzero";
  result.reason = completed.error?.code === "ETIMEDOUT"
    ? `scanner timed out after ${scanner.timeout_seconds ?? 300}s`
    : completed.status === 0
      ? null
      : "scanner exited nonzero";
  return result;
}

function scannerJsonState(rawDir, scannerName) {
  const file = outputPath(rawDir, scannerName);
  if (!existsSync(file)) return { exists: false, parsed: false, value: null };
  try {
    return { exists: true, parsed: true, value: readJson(file) };
  } catch {
    return { exists: true, parsed: false, value: null };
  }
}

function severityRank(severity) {
  return {
    critical: 5,
    high: 4,
    error: 4,
    medium: 3,
    moderate: 3,
    warning: 3,
    low: 2,
    info: 1,
    unknown: 1
  }[String(severity ?? "unknown").toLowerCase()] ?? 1;
}

function normalizeSeverity(value, fallback = "unknown") {
  const severity = String(value ?? fallback).toLowerCase();
  if (["critical", "high", "medium", "moderate", "low", "info", "warning", "error"].includes(severity)) {
    return severity === "moderate" ? "medium" : severity === "error" ? "high" : severity;
  }
  return fallback;
}

function severityFromScore(value) {
  const score = typeof value === "number" ? value : Number.parseFloat(String(value ?? ""));
  if (!Number.isFinite(score)) return null;
  if (score >= 9) return "critical";
  if (score >= 7) return "high";
  if (score >= 4) return "medium";
  if (score > 0) return "low";
  return "info";
}

function normalizeOsvSeverityValue(vulnerability) {
  const databaseSeverity = normalizeSeverity(vulnerability.database_specific?.severity, "");
  if (databaseSeverity) return databaseSeverity;
  const severity = Array.isArray(vulnerability.severity) ? vulnerability.severity[0] : null;
  const scored = severityFromScore(severity?.score);
  if (scored) return scored;
  if (String(severity?.score ?? "").startsWith("CVSS:")) return "high";
  return "high";
}

function fingerprint(parts) {
  return parts.map((part) => String(part ?? "")).join("|").toLowerCase();
}

function finding(scanner, category, severity, title, location = {}, extra = {}) {
  return {
    scanner,
    category,
    severity: normalizeSeverity(severity),
    title: String(title ?? "security finding"),
    location: {
      path: location.path ?? "",
      line: location.line ?? null
    },
    fingerprint: fingerprint([scanner, category, title, location.path, location.line, extra.id]),
    ...extra
  };
}

function acceptedFinding(policy, item) {
  const entries = policy.accepted_findings ?? [];
  return entries.find((entry) => {
    if (entry.expires_on) {
      const expiresAt = Date.parse(`${entry.expires_on}T23:59:59.999Z`);
      if (!Number.isFinite(expiresAt) || Date.now() > expiresAt) return false;
    }
    if (entry.scanner && entry.scanner !== item.scanner) return false;
    if (entry.category && entry.category !== item.category) return false;
    if (entry.id && entry.id !== item.id) return false;
    if (entry.path && entry.path !== item.location?.path) return false;
    return true;
  });
}

function normalizeGitleaks(raw) {
  if (!Array.isArray(raw)) return [];
  return raw.map((item) =>
    finding("gitleaks", "secrets", "high", item.Description ?? item.RuleID ?? "secret finding", {
      path: item.File,
      line: item.StartLine
    }, {
      id: item.RuleID,
      secret_redacted: true
    })
  );
}

function normalizeCargoAudit(raw) {
  const list = raw?.vulnerabilities?.list ?? raw?.vulnerabilities ?? [];
  if (!Array.isArray(list)) return [];
  return list.map((item) => {
    const advisory = item.advisory ?? item;
    const packageName = item.package?.name ?? advisory.package ?? "rust dependency";
    return finding("cargo-audit", "dependency", advisory.cvss >= 9 ? "critical" : "high", advisory.title ?? advisory.id ?? packageName, {
      path: "Cargo.lock",
      line: null
    }, {
      id: advisory.id,
      package: packageName
    });
  });
}

function normalizePnpmAudit(raw) {
  const advisories = raw?.advisories && typeof raw.advisories === "object" ? Object.values(raw.advisories) : [];
  const vulnerabilities = Array.isArray(raw?.vulnerabilities) ? raw.vulnerabilities : [];
  return [...advisories, ...vulnerabilities].map((item) =>
    finding("pnpm-audit", "dependency", item.severity ?? "unknown", item.title ?? item.name ?? "node dependency advisory", {
      path: "pnpm-lock.yaml",
      line: null
    }, {
      id: item.id ?? item.github_advisory_id,
      package: item.module_name ?? item.name
    })
  );
}

function normalizeSemgrep(raw) {
  const results = Array.isArray(raw?.results) ? raw.results : [];
  return results.map((item) =>
    finding("semgrep", "sast", item.extra?.severity ?? "warning", item.extra?.message ?? item.check_id, {
      path: item.path,
      line: item.start?.line
    }, {
      id: item.check_id
    })
  );
}

function normalizeTrivy(raw) {
  const findings = [];
  for (const result of raw?.Results ?? []) {
    for (const item of result.Vulnerabilities ?? []) {
      findings.push(finding("trivy-fs", "dependency", item.Severity, item.Title ?? item.VulnerabilityID, {
        path: result.Target,
        line: null
      }, {
        id: item.VulnerabilityID,
        package: item.PkgName
      }));
    }
    for (const item of result.Misconfigurations ?? []) {
      findings.push(finding("trivy-fs", "iac", item.Severity, item.Title ?? item.ID, {
        path: item.CauseMetadata?.Resource ?? result.Target,
        line: item.CauseMetadata?.StartLine ?? null
      }, {
        id: item.ID
      }));
    }
    for (const item of result.Secrets ?? []) {
      findings.push(finding("trivy-fs", "secrets", item.Severity ?? "high", item.Title ?? item.RuleID, {
        path: item.Target ?? result.Target,
        line: item.StartLine ?? null
      }, {
        id: item.RuleID,
        secret_redacted: true
      }));
    }
  }
  return findings;
}

function normalizeCargoDeny(raw) {
  const findings = [];
  const diagnostics = Array.isArray(raw?.diagnostics)
    ? raw.diagnostics
    : Array.isArray(raw)
      ? raw
      : [];
  for (const item of diagnostics) {
    const code = item.code?.code ?? item.code ?? item.id ?? "cargo-deny";
    const severity = normalizeSeverity(item.severity, "warning");
    const message = item.message ?? item.title ?? code;
    const span = Array.isArray(item.spans) ? item.spans[0] : null;
    findings.push(
      finding(
        "cargo-deny",
        "rust-supply-chain",
        severity,
        message,
        {
          path: span?.file_name ?? "Cargo.lock",
          line: span?.line_start ?? null
        },
        { id: code }
      )
    );
  }
  return findings;
}

function normalizeOsvScanner(raw) {
  const findings = [];
  for (const result of raw?.results ?? []) {
    const sourcePath = result.source?.path ?? result.path ?? "lockfile";
    const normalizedSourcePath = path.isAbsolute(sourcePath)
      ? rel(sourcePath)
      : sourcePath.replaceAll(path.sep, "/");
    for (const pkg of result.packages ?? []) {
      const packageName = pkg.package?.name ?? pkg.name ?? "dependency";
      for (const vulnerability of pkg.vulnerabilities ?? []) {
        findings.push(
            finding(
              "osv-scanner",
              "dependency",
              normalizeOsvSeverityValue(vulnerability),
              vulnerability.summary ?? vulnerability.id ?? packageName,
              { path: normalizedSourcePath, line: null },
              { id: vulnerability.id, package: packageName }
          )
        );
      }
    }
  }
  return findings;
}

function normalizeRustUnsafeInventory(raw) {
  const findings = Array.isArray(raw?.findings) ? raw.findings : [];
  return findings.map((item) =>
    finding(
      "rust-unsafe-inventory",
      "rust-unsafe",
      item.severity ?? "info",
      item.id === "unwrap-unchecked" ? "unchecked unwrap in Rust code" : "unsafe Rust usage inventory",
      { path: item.path, line: item.line },
      { id: item.id }
    )
  );
}

function normalizeFindings(rawDir, scannerHealth) {
  const normalizers = {
    "gitleaks": normalizeGitleaks,
    "cargo-audit": normalizeCargoAudit,
    "cargo-deny": normalizeCargoDeny,
    "osv-scanner": normalizeOsvScanner,
    "rust-unsafe-inventory": normalizeRustUnsafeInventory,
    "pnpm-audit": normalizePnpmAudit,
    "semgrep": normalizeSemgrep,
    "trivy-fs": normalizeTrivy
  };
  const findings = [];
  const normalizerErrors = [];
  const reportedScanners = [];
  for (const [scannerName, normalize] of Object.entries(normalizers)) {
    const scanner = scannerHealth?.scanners?.[scannerName];
    const rawState = scannerJsonState(rawDir, scannerName);
    const raw = rawState.value;
    if (!raw) {
      if (
        scanner?.applies &&
        (rawState.exists || scanner.status === "completed_nonzero" || scanner.status === "timed_out")
      ) {
        normalizerErrors.push({
          scanner: scannerName,
          error: `missing or invalid JSON output for ${scanner.status} scanner`
        });
      }
      continue;
    }
    try {
      findings.push(...normalize(raw));
      reportedScanners.push(scannerName);
    } catch (error) {
      normalizerErrors.push({ scanner: scannerName, error: error.message });
    }
  }
  return { findings, normalizer_errors: normalizerErrors, reported_scanners: reportedScanners };
}

function statusForFinding(policy, item) {
  const accepted = acceptedFinding(policy, item);
  if (accepted) return "accepted";
  if (item.category === "secrets") return "policy_blocked";
  if ((policy.block_severities ?? []).includes(item.severity)) return "policy_blocked";
  if (severityRank(item.severity) >= severityRank("medium")) return "warning";
  return "informational";
}

function buildVerdict(policy, scannerHealth, findingsPayload, nightly) {
  const findings = findingsPayload.findings.map((item) => ({
    ...item,
    policy_status: statusForFinding(policy, item),
    accepted_reason: acceptedFinding(policy, item)?.reason
  }));
  const missingRequired = Object.values(scannerHealth.scanners).filter(
    (scanner) => scanner.applies && scanner.required && scanner.status === "unavailable"
  );
  const policyBlockedFindings = findings.filter((item) => item.policy_status === "policy_blocked");
  const normalizerErrors = findingsPayload.normalizer_errors ?? [];
  const reportedScanners = new Set(findingsPayload.reported_scanners ?? []);
  const nonzeroRequired = Object.values(scannerHealth.scanners).filter(
    (scanner) =>
      scanner.applies &&
      scanner.required &&
      (scanner.status === "completed_nonzero" || scanner.status === "timed_out")
  );
  const failedRequired = nonzeroRequired.filter((scanner) => !reportedScanners.has(scanner.scanner));
  const missingBlocks = nightly && policy.nightly_blocks_on_missing_required ? missingRequired : missingRequired.filter((scanner) => scanner.block_on_unavailable);
  const baseBlocked = policyBlockedFindings.length > 0 || missingBlocks.length > 0 || failedRequired.length > 0 || normalizerErrors.length > 0;
  let status = baseBlocked
    ? "policy_blocked"
    : missingRequired.length > 0
      ? "degraded_coverage"
      : "passed";
  const severityCounts = {};
  const categoryCounts = {};
  for (const item of findings) {
    severityCounts[item.severity] = (severityCounts[item.severity] ?? 0) + 1;
    categoryCounts[item.category] = (categoryCounts[item.category] ?? 0) + 1;
  }
  const verdict = {
    status,
    nightly,
    generated_at: utcNow(),
    policy: policy.name,
    scanner_count: Object.keys(scannerHealth.scanners).length,
    missing_required_scanners: missingRequired.map((scanner) => scanner.scanner),
    blocking_missing_scanners: missingBlocks.map((scanner) => scanner.scanner),
    failed_required_scanners: failedRequired.map((scanner) => scanner.scanner),
    nonzero_required_scanners: nonzeroRequired.map((scanner) => scanner.scanner),
    nonzero_reported_scanners: nonzeroRequired
      .filter((scanner) => reportedScanners.has(scanner.scanner))
      .map((scanner) => scanner.scanner),
    finding_count: findings.length,
    policy_blocked_finding_count: policyBlockedFindings.length,
    severity_counts: severityCounts,
    category_counts: categoryCounts,
    normalizer_errors: normalizerErrors,
    findings
  };
  const score = postureScore(verdict);
  if (score < (policy.minimum_posture_score ?? 0)) {
    verdict.status = "policy_blocked";
    verdict.posture_score_blocked = true;
    verdict.minimum_posture_score = policy.minimum_posture_score;
  }
  return verdict;
}

function postureScore(verdict) {
  const missingPenalty = 8 * verdict.missing_required_scanners.length;
  const failedPenalty = 12 * (verdict.failed_required_scanners?.length ?? 0);
  const blockerPenalty = 15 * verdict.policy_blocked_finding_count;
  const normalizerPenalty = 10 * verdict.normalizer_errors.length;
  return Math.max(0, 100 - missingPenalty - failedPenalty - blockerPenalty - normalizerPenalty);
}

function markdownSummary(summary) {
  const lines = [
    `# Security Audit Summary`,
    ``,
    `- Run ID: \`${summary.run_id}\``,
    `- Status: \`${summary.status}\``,
    `- Posture score: \`${summary.posture_score}/100\``,
    `- Findings: \`${summary.finding_count}\``,
    `- Policy-blocking findings: \`${summary.policy_blocked_finding_count}\``,
    `- Missing required scanners: \`${summary.missing_required_scanners.join(", ") || "none"}\``,
    ``,
    `## Scanner Health`,
    ``,
    `| Scanner | Category | Status | Required | Reason |`,
    `| --- | --- | --- | --- | --- |`
  ];
  for (const scanner of Object.values(summary.scanners)) {
    lines.push(`| ${scanner.scanner} | ${scanner.category} | ${scanner.status} | ${scanner.required ? "yes" : "no"} | ${(scanner.reason ?? "").replaceAll("|", "\\|")} |`);
  }
  lines.push(``, `## Top Findings`, ``);
  const top = summary.top_findings.slice(0, 20);
  if (top.length === 0) {
    lines.push(`No normalized findings.`);
  } else {
    lines.push(`| Severity | Status | Scanner | Category | Location | Title |`, `| --- | --- | --- | --- | --- | --- |`);
    for (const item of top) {
      const location = item.location?.path ? `${item.location.path}${item.location.line ? `:${item.location.line}` : ""}` : "";
      lines.push(`| ${item.severity} | ${item.policy_status} | ${item.scanner} | ${item.category} | ${location.replaceAll("|", "\\|")} | ${item.title.replaceAll("|", "\\|")} |`);
    }
  }
  lines.push(``, `Raw scanner outputs are intentionally not committed. Use this sanitized summary for PR review.`);
  return `${lines.join("\n")}\n`;
}

function main() {
  const args = parseArgs(process.argv.slice(2));
  const policy = readJson(args.policyPath);
  const artifactRoot = path.join(repoRoot, policy.artifact_root ?? "audit/runs");
  const runDir = path.join(artifactRoot, args.runId);
  const rawDir = path.join(runDir, "raw");
  mkdirSync(rawDir, { recursive: true });

  const manifest = {
    name: "mdx-security-audit",
    run_id: args.runId,
    started_at: utcNow(),
    nightly: args.nightly,
    dry_run: args.dryRun,
    policy: rel(args.policyPath),
    artifact_root: rel(runDir),
    raw_outputs_committed: false
  };
  writeJson(path.join(runDir, "manifest.json"), manifest);

  const scannerResults = {};
  for (const [scannerName, scanner] of Object.entries(policy.scanners ?? {})) {
    scannerResults[scannerName] = runScanner(runDir, rawDir, scannerName, scanner, args.dryRun);
  }
  const scannerHealth = {
    generated_at: utcNow(),
    scanners: scannerResults
  };
  writeJson(path.join(runDir, "scanner-health.json"), scannerHealth);

  const findingsPayload = normalizeFindings(rawDir, scannerHealth);
  const verdict = buildVerdict(policy, scannerHealth, findingsPayload, args.nightly);
  const findingsNormalized = {
    generated_at: utcNow(),
    findings: verdict.findings,
    normalizer_errors: findingsPayload.normalizer_errors
  };
  writeJson(path.join(runDir, "findings.normalized.json"), findingsNormalized);
  writeJson(path.join(runDir, "policy-verdict.json"), verdict);

  const summary = {
    run_id: args.runId,
    generated_at: utcNow(),
    status: verdict.status,
    posture_score: postureScore(verdict),
    finding_count: verdict.finding_count,
    policy_blocked_finding_count: verdict.policy_blocked_finding_count,
    severity_counts: verdict.severity_counts,
    category_counts: verdict.category_counts,
    missing_required_scanners: verdict.missing_required_scanners,
    scanners: scannerResults,
    top_findings: [...verdict.findings]
      .sort((left, right) => severityRank(right.severity) - severityRank(left.severity))
      .slice(0, 50)
  };
  writeJson(path.join(runDir, "summary.json"), summary);
  writeText(path.join(runDir, "summary.md"), markdownSummary(summary));

  const completedManifest = { ...manifest, completed_at: utcNow(), status: verdict.status };
  writeJson(path.join(runDir, "manifest.json"), completedManifest);

  if (!args.dryRun && !args.noTrustPublish) {
    const publish = spawnSync(process.execPath, [
      "scripts/security-trust-publish.mjs",
      "--run-id",
      args.runId
    ], {
      cwd: repoRoot,
      encoding: "utf8"
    });
    if (publish.stdout) process.stdout.write(publish.stdout);
    if (publish.stderr) process.stderr.write(publish.stderr);
    if (publish.status !== 0) {
      process.exitCode = publish.status ?? 1;
      return;
    }
  }

  if (args.printSummary) {
    process.stdout.write(readFileSync(path.join(runDir, "summary.md"), "utf8"));
  } else {
    console.log(`security_audit: ${verdict.status} run=${args.runId} findings=${verdict.finding_count} artifact=${rel(runDir)}`);
  }
  process.exitCode = verdict.status === "policy_blocked" ? 1 : 0;
}

try {
  main();
} catch (error) {
  console.error(`security_audit: ERROR ${error.message}`);
  process.exit(1);
}
