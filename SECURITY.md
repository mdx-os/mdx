# Security Policy

MDx is not production-deployment-ready yet. The public security posture is built around local proof, fail-closed gates, and explicit blocked authority.

## Supported Versions

Before a public release, only the current default branch is supported for security review. Tagged release support will be defined when MDx publishes its first public release candidate.

## Reporting A Vulnerability

Do not open public issues for suspected vulnerabilities that include exploit details, secrets, private data, or reproduction artifacts.

For the public repository, use GitHub private vulnerability reporting or GitHub
Security Advisories. If you cannot access that path, contact the maintainer
privately and do not include exploit details in a public issue.

## What We Run

The repository includes a free baseline security loop:

- secret pattern checks
- full-history secret scan target
- cargo-audit
- cargo-deny
- OSV Scanner
- pnpm audit
- Semgrep
- Trivy filesystem, IaC, and secret scan
- Syft SBOM generation
- CodeQL for JavaScript and TypeScript
- MDx-specific threat, fake-green, frozen-kernel, and security posture checks

See `docs/SECURITY-SCANNER-READINESS.md` for the current scanner posture and artifact rules.

## Artifact Rules

Raw scanner outputs are not committed. They can include matched context. Sanitized findings, summaries, posture verdicts, and SBOM summaries may be committed or uploaded as CI artifacts.

Secret values must never be committed, logged, or copied into generated evidence.

## Production Authority

Local setup, public source release, cloud deployment, external auth, email invites, production writes, and live worker execution are separate gates. A passing local security scan does not grant production deployment authority.
