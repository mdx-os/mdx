# Security Posture

MDx security is enforced through boring repo rails: declarations, generated artifacts, fail-closed local checks, and receipts. This file is the source for `generated/security/mdx-security-posture.json`.

## Baseline Controls

- Policy and receipt path: every consequential loop or tool action requires a PDP decision and a Ledger receipt.
- Tenant isolation: tenant-owned Postgres tables must carry tenant markers, RLS policies, and policy drop guards.
- Secrets and live credentials: live substrates stay `PENDING-LIVE-RUN` until observed evidence exists; `LIVE-LOCAL` is allowed only for CI-observed local infrastructure. Deterministic local stubs are allowed; fake live status is not.
- Durable storage: receipt writes route through `StorageProvider::append_receipt`; Postgres SQL projection stays owned by `mdx-core`.
- Generated contracts: agents must change source declarations and regenerate artifacts instead of hand-editing generated outputs.
- Generated security artifacts: files under `generated/security/` must stay flat, regular-file-only, JSON-only, and free of credential-looking payloads, provider tokens, or private key material.
- Generated security canary: `scripts/security-generated-artifacts-canary.mjs` must prove the security gate rejects credential-looking JSON, non-JSON files, nested artifacts, and symlinked artifacts before generated security scanning rules are changed.
- Companions: Twin companions are read-only and receipt-grounded until governed action rails are green. They cannot run loops, spawn workers, mint credentials, spend, deploy, call providers, or write memory without policy, authority, threat, provider or worker proof, and receipts.
- Workers: ephemeral workers require scope, expiry, budget, tool allowlist, parent, sponsor chain, spawn receipt, and retirement receipt.
- Waivers: exceptions must be owned, scoped, expiring, risk-linked, and unable to disable no-fake-green, policy, receipt, or tenant isolation gates.

## Required Gates

Run `make security-posture-check` for security posture changes and `make security-check` for runtime security invariants. Run `node scripts/security-generated-artifacts-canary.mjs` when changing generated security artifact handling or scanner rules. Run `make local-full-check` when the change touches receipt persistence, ledger export, or local Postgres evidence.

## Hard Stops

- No direct action path that skips policy.
- No receipt mint outside the storage boundary.
- No companion or worker standing authority.
- No live substrate status beyond pending without observed provider evidence.
- No generated security artifact with credential-looking payloads or private key material.
- No nested, symlinked, or non-JSON generated security artifact that can bypass scanning.
- No generated security scanner change without a failing negative canary for the bypass shape being changed.
- No security waiver without expiry and mitigation check.
