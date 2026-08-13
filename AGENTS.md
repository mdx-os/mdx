# Working on MDx with coding agents

MDx welcomes changes authored with coding agents. The same standards apply whether a human, an agent, or both wrote the patch.

Before changing product behavior, read:

- `README.md`
- `CONTRIBUTING.md`
- `docs/ARCHITECTURE.md`
- `docs/UI-PRODUCT-SURFACE-CONTRACT.md` for interface work
- `SECURITY.md` for trust-boundary work

Keep each change scoped and reviewable. Preserve the distinction between generated contracts and the source that produces them. Never add credentials, private customer material, personal machine paths, or fabricated readiness evidence.

Before handing work off, run the checks that match the change. `make local-smoke` is the normal baseline. Native app changes should also run `make macos-check` or `make ios-build-check` as appropriate.

Use commit titles in the form `[area] Clear outcome`. Meaningful commits should explain why the change was needed, what changed, what was validated, and any remaining risk.
