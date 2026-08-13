import { readFileSync, existsSync } from "node:fs";
import { resolve } from "node:path";

// The setup-path contract is the single source of truth for how MDx is stood up
// locally. This check gives it teeth: the contract's claims must match the real
// Makefile, scripts, and QUICKSTART, so the snapshot and Docker stories cannot
// silently re-diverge and the contract can never describe a path that is not wired.
const root = resolve(import.meta.dirname, "..");
const read = (p) => readFileSync(resolve(root, p), "utf8");
const fail = (m) => {
  throw new Error(`setup_path_check: ${m}`);
};

const contract = JSON.parse(read("generated/setup/mdx-setup-path.json"));

// 1. One default local path, no Docker; the opt-in path is the Docker one.
if (contract.entry_command !== "make setup") fail("entry_command must be 'make setup'");
if (contract.canonical_path?.default !== true) fail("canonical_path must be the default");
if (contract.canonical_path?.needs_docker !== false)
  fail("canonical (default) path must NOT need Docker");
if (contract.opt_in_path?.needs_docker !== true)
  fail("opt_in path must be the Docker+Postgres one");
if (!/never install/i.test(contract.cold_start_rule || ""))
  fail("cold_start_rule must state prerequisites are detected, never installed");

// 2. The contract cannot claim a path that is not actually wired.
if (!existsSync(resolve(root, "scripts/setup.sh")))
  fail("contract names 'make setup' but scripts/setup.sh is missing");
const makefile = read("Makefile");
if (!/^setup:/m.test(makefile)) fail("Makefile has no 'setup:' target the contract promises");
if (!/\.PHONY:.*\bsetup\b/.test(makefile) && !/^setup:/m.test(makefile))
  fail("'setup' should be a real target");

// 3. The human docs point at the one canonical entry, not the old assemble-it-yourself path.
const quickstart = read("docs/QUICKSTART.md");
if (!/make setup/.test(quickstart))
  fail("QUICKSTART must lead with 'make setup' (the one canonical cold start)");

// 4. The setup script must DETECT, not install. Guard against actually executing a
//    toolchain installer (printed hints that mention rustup.rs/corepack are fine).
const setup = read("scripts/setup.sh");
if (/apt-get +install|brew +install|rustup-init|curl[^\n]*\|\s*sh\b/.test(setup))
  fail("scripts/setup.sh must DETECT prerequisites, never run an installer");
if (!/command -v/.test(setup)) fail("scripts/setup.sh must detect tools via 'command -v'");

console.log(
  `setup_path_check: OK default=${contract.canonical_path.id} opt_in=${contract.opt_in_path.id} detect_only=true`
);
