#!/usr/bin/env node
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const repoRoot = path.resolve(new URL("..", import.meta.url).pathname);
const roots = ["crates"];
const ignored = new Set(["target", "node_modules", ".git", ".mdx-local", "audit"]);

function walk(dir, files = []) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (ignored.has(entry.name)) continue;
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walk(full, files);
    } else if (entry.isFile() && entry.name.endsWith(".rs")) {
      files.push(full);
    }
  }
  return files;
}

function rel(file) {
  return path.relative(repoRoot, file).replaceAll(path.sep, "/");
}

const patterns = [
  { id: "unsafe-block", severity: "info", regex: /\bunsafe\s*\{/g },
  { id: "unsafe-fn", severity: "info", regex: /\bunsafe\s+fn\b/g },
  { id: "unsafe-trait", severity: "info", regex: /\bunsafe\s+trait\b/g },
  { id: "unsafe-impl", severity: "info", regex: /\bunsafe\s+impl\b/g },
  { id: "unwrap-unchecked", severity: "high", regex: /\bunwrap_unchecked\s*\(/g }
];

const findings = [];
for (const root of roots) {
  const absoluteRoot = path.join(repoRoot, root);
  for (const file of walk(absoluteRoot)) {
    const lines = readFileSync(file, "utf8").split(/\r?\n/);
    lines.forEach((line, index) => {
      for (const pattern of patterns) {
        pattern.regex.lastIndex = 0;
        if (pattern.regex.test(line)) {
          findings.push({
            id: pattern.id,
            severity: pattern.severity,
            path: rel(file),
            line: index + 1,
            snippet: line.trim().slice(0, 160)
          });
        }
      }
    });
  }
}

process.stdout.write(
  `${JSON.stringify(
    {
      name: "mdx-rust-unsafe-inventory",
      scanned_roots: roots,
      finding_count: findings.length,
      findings
    },
    null,
    2
  )}\n`
);
