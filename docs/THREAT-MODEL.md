# Threat Model

This is the lean security threat model for the current local kernel and agent-first repo. The generated model lives at `generated/security/mdx-threat-model.json`.

## Threats

- Policy bypass before consequential action.
- Receipt forgery or receipt write outside the storage boundary.
- Tenant data crossing RLS or typed tenant boundaries.
- Live substrate fake green before observed evidence.
- Prompt or companion action drift into ungoverned tool execution.
- Worker scope creep, direct spawn, or missing retirement.
- Legacy app topology recreated as parallel truth stores.
- Secret leakage through local status, generated artifacts, nested generated artifacts, or logs.
- Cross-tenant model credential resolution through provider-wide fallback or an imprecise secret reference.
- Mounted model secret traversal, symlink substitution, writable-file replacement, or oversized secret ingestion.
- Model secret broker downgrade, credential-bearing URL, query leakage, or unbounded response ingestion.
- Stale model readiness after disconnect, revoke, credential loss, rotation, or server restart.
- Live model proof that runs without explicit operator intent or an absolute pre-call cost ceiling.
- Credential-looking payloads, private key material, symlinks, or nested non-JSON payloads committed under generated security artifacts.
- Generated security scanner drift that preserves happy-path artifacts while no longer rejecting credential, non-JSON, nested, or symlink canaries.

## Rules

- Every threat maps to a mitigation check.
- High impact threats map to a generated evidence artifact.
- Generated security scanner changes run the negative canary before handoff.
- New live substrate, app runtime, or worker behavior updates this model.

Run `make threat-model-check` after changing security-sensitive surfaces.
