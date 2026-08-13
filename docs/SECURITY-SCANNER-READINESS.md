# Security Scanner Readiness

MDx uses an internal fail-closed security posture, but external security teams also expect familiar scanner evidence. This document defines the free scanner loop that turns those tools into reviewable MDx artifacts.

## What Runs

The scanner loop is policy-driven from `security/scanner-policy.json` and writes artifacts to `audit/runs/<run-id>/`.

Current free scanners:

- `repo-secret-history`: existing MDx secret-history gate.
- `gitleaks`: secret scanning over the working tree.
- `cargo-audit`: Rust dependency advisories from `Cargo.lock`.
- `cargo-deny`: Rust advisory, license, source, and duplicate-dependency policy.
- `osv-scanner`: OSV vulnerability coverage across lockfiles.
- `rust-unsafe-inventory`: MDx-owned inventory of unsafe Rust and unchecked unsafe calls.
- `pnpm-audit`: Node dependency advisories from `pnpm-lock.yaml`.
- `semgrep`: SAST with OWASP Top 10 and secrets rules.
- `trivy-fs`: filesystem, dependency, IaC, container, and secret scanning.
- `sbom-generate`: Syft SBOM generation in CycloneDX JSON and SPDX JSON.
- `CodeQL`: GitHub code scanning for JavaScript and TypeScript surfaces.
- `Dependabot`: weekly Cargo, npm, GitHub Actions, and Docker update PRs.

This is intentionally the same shape as a serious enterprise intake: SCA, SAST, secrets, IaC/container, code scanning, SBOM inventory, and dependency update hygiene.

## Commands

Plan the scanner lane without requiring local scanner installs:

```sh
make security-scanner-readiness-check
```

Run the local scanner audit:

```sh
make security-audit
```

Completed non-dry-run audits also refresh
`generated/security/mdx-security-trust.json`, the public aggregate summary used by
the `/security` trust page.

Run the fail-closed nightly posture locally:

```sh
make security-audit-nightly
```

Print the latest sanitized report:

```sh
make security-audit-report
```

Republish the public trust-page summary from an existing real executed run:

```sh
make security-trust-publish SECURITY_TRUST_ARGS="--run-id local-scanner-final-pass"
```

Prove the public trust artifact stays sanitized and dry-runs cannot refresh it:

```sh
make security-trust-publish-check
```

Plan SBOM generation without requiring Syft:

```sh
make sbom-readiness-check
```

Generate local SBOM artifacts:

```sh
make sbom-generate SBOM_ARGS="--run-id local-sbom"
```

Recommended local installs for full local coverage:

```sh
cargo install cargo-audit --locked
brew install gitleaks trivy
python3 -m pip install semgrep
brew install syft
```

Without every scanner installed, local runs may return `degraded_coverage`. Nightly CI installs the required free scanners and fails closed when coverage is missing.

## Rust-Specific Review Posture

The Rust lane is intentionally layered. The first layer is deterministic and cheap enough to run in the normal security loop:

- `cargo-audit` for RustSec advisories.
- `cargo-deny` for advisories, duplicate dependencies, source policy, and license policy.
- `osv-scanner` for a second vulnerability database view across lockfiles.
- `rust-unsafe-inventory` for unsafe hotspots and any `unwrap_unchecked` use.
- `clippy --workspace --all-targets -- -D warnings` through the normal verify lane.

The next maturity layer is valuable, but should not be treated as a checkbox install:

- `cargo-vet`: use when MDx is ready to maintain dependency audit attestations, trusted imports, and explicit exemptions. This is the right answer for supply-chain provenance, but it needs an owner and a review rhythm.
- `cargo-auditable`: use for production release binaries and containers so dependency metadata is embedded in compiled Rust artifacts and external scanners can see what is inside a stripped image.
- `cargo-machete`: run periodically to remove unused direct dependencies and keep the attack surface small. Treat as dependency hygiene, not a high-severity vulnerability gate.
- `cargo-fuzz`: add targeted fuzz harnesses for security-sensitive parsers and protocols, starting with bearer parsing, relay headers, tenant routing, and JSON request payloads.

Do not add noisy Rust scanners as permanent blockers without a clear owner, finding format, and triage rule. Security reviewers care more about repeatable evidence and fixed high-value findings than about a long list of flaky tools.

## CI

`.github/workflows/security-audit.yml` runs nightly, on manual dispatch, and on PRs that touch security, dependency, deploy, or scanner files. It installs the free scanners, runs `make security-audit-nightly`, uploads sanitized artifacts, and fails the workflow when the policy verdict is blocked.

The same workflow also generates SBOMs with Syft and uploads:

- `sbom.cyclonedx.json`
- `sbom.spdx.json`
- `sbom-manifest.json`
- `sbom-summary.json`
- `sbom-summary.md`

`.github/workflows/codeql.yml` runs GitHub CodeQL for JavaScript and TypeScript and uploads SARIF as a workflow artifact. If repository code scanning is enabled later, the workflow can switch from artifact-only SARIF to GitHub code-scanning upload. Rust remains covered by `cargo-audit`, clippy, Semgrep patterns where applicable, and the MDx runtime gates.

`.github/dependabot.yml` keeps Cargo, npm, GitHub Actions, and Docker references moving through normal PR review.

## Artifact Rules

Commit sanitized evidence only:

- `manifest.json`
- `scanner-health.json`
- `findings.normalized.json`
- `policy-verdict.json`
- `summary.json`
- `summary.md`

Do not commit raw scanner outputs. They may include matched secret context or file snippets. Raw outputs stay under ignored `audit/runs/<run-id>/raw/`.

The committed public trust artifact is `generated/security/mdx-security-trust.json`.
It is derived only from a completed, non-dry-run scanner pass with no `planned`
scanner statuses. It contains aggregate counts, scanner ids and statuses, posture
score, baseline trend, and SHA-256 digests of the source sanitized artifacts. It
must not contain finding paths, line numbers, raw-output paths, titles, snippets,
or secret-shaped values. If a run is only a readiness dry-run, the publisher
leaves the current public artifact untouched.

## Policy

The first policy is deliberately conservative:

- secrets block
- high and critical findings block
- missing required scanner coverage degrades a local run
- missing required scanner coverage blocks nightly
- malformed scanner normalization blocks

Accepted findings are intentionally narrow. They must match scanner, category, id, and path in `security/scanner-policy.json`, include a reason, and remain visible in `findings.normalized.json` and `summary.md`. Use this only for false positives, local proof scaffolds, or an explicitly expiring infrastructure constraint with a named mitigation check. Do not use acceptance to hide a fixable vulnerability.

Current accepted findings:

- Semgrep loopback HTTP in `scripts/ctx-provider-turn-on-check.mjs`: local spawned proof server only.
- Trivy ConfigMap sensitivity in `deploy/kubernetes/base/configmap.yaml`: `MDX_SECRETS_BACKEND` is a backend selector, while secret values live in `mdx-runtime-secrets` or an external secret store.
- Trivy unrestricted HTTPS egress on the isolated AgentCore dependency proxy: package registries and source hosts use dynamic addresses that a security group cannot express as FQDNs. The runtime can reach only the proxy, Squid enforces the deployed domain allowlist and private-address denial, and the acceptance expires on 2026-10-31 for network-layer reassessment.

Do not add broad accepted findings. If a finding represents a real vulnerability or a production obligation, fix it or leave it as a warning until the owning deployment target supplies the missing detail.

## Enterprise Tool Slots

Snyk and Checkmarx are not required for the free baseline because they usually need organization credentials. They can be added as additional policy scanners later without changing the artifact shape:

- run the vendor CLI in CI with organization secrets
- write raw output under `audit/runs/<run-id>/raw/`
- normalize into `findings.normalized.json`
- block through `policy-verdict.json`

The rule is the same for every scanner: raw detail stays private, sanitized findings become reviewable evidence.

## Agent Security Research

Vercel `deepsec` is useful, but it is not the same kind of always-on deterministic gate as Gitleaks, cargo-audit, pnpm-audit, Semgrep, Trivy, CodeQL, or SBOM generation.

Use it as an on-demand agent security research harness when the goal is to find subtle application vulnerabilities across auth, tenant boundaries, request handling, data flows, and custom MDx conventions.

Recommended MDx posture:

- Do not make DeepSec a required PR gate yet.
- Run an initial bounded local pilot before any full-repo run.
- Keep `.deepsec/` project context tracked only after the pilot produces useful signal.
- Keep generated `data/`, findings, and reports out of git unless sanitized for review.
- Revalidate high and critical findings before filing work.
- If adding PR-mode later, use the two-job shape from DeepSec docs: the analysis job has read-only repo permissions and secrets, while the comment job has write permission but never executes PR code.

Starter pilot:

```sh
npx deepsec init
cd .deepsec
pnpm install
pnpm deepsec scan
pnpm deepsec process --limit 50 --concurrency 1
pnpm deepsec revalidate --min-severity HIGH
pnpm deepsec export --format md-dir --out ./findings
```

Why this is not required CI yet:

- Full processing is AI-driven and can cost real money on large repos.
- It sends relevant source snippets to the configured model provider.
- It needs repo-specific `INFO.md` and possibly custom matchers to avoid generic findings.
- Its own docs call out false positives and recommend revalidation for high-severity output.
