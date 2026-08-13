# Architecture

MDx is a workspace for governed work with AI. The implementation is split into a small set of product and runtime boundaries so authority, evidence, and user experience do not collapse into one service.

## Product surfaces

- `apps/mdx-host` is the web product, including Twin, Forge, Pages, and Message.
- `apps/mdx-operator-macos` is the macOS client.
- `apps/mdx-operator-ios` is the iOS client.
- `apps/mdx-operator-shared` contains Swift code shared by the native clients.

## Runtime

- `crates/mdx-core` owns the kernel contracts, receipts, policy-facing state, and route catalog.
- `crates/mdx-server` serves the local HTTP runtime and product integrations.
- `crates/mdx-message-relay` and `crates/mdx-mobile-relay` provide bounded relay paths.
- `crates/mdx-ctx-engine` and `crates/mdx-dxr-engine` own context and document-experience runtime work.
- `crates/mdx-observability` contains shared telemetry primitives.

## Contracts and generated artifacts

`contracts/` and `generated/` are committed so the system's declared shapes can be inspected and reviewed with the code that consumes them. Generated artifacts are not a substitute for runtime evidence. A declared route, schema, or deployment shape does not prove that an external provider has been exercised.

## Data and authority

The default local path uses a snapshot-backed kernel. `migrations/` and `docker-compose.yml` describe the optional Postgres-backed path. Consequential actions are designed to pass through explicit authority and leave receipts; credentials stay outside the repository.

## Dependency direction

Product clients depend on declared runtime interfaces. Runtime adapters depend on the kernel contracts. External providers and storage implementations sit behind those boundaries so a missing provider, credential, approval, or storage precondition can fail closed without changing product semantics.

Start with `docs/QUICKSTART.md` to run the system and `docs/UI-PRODUCT-SURFACE-CONTRACT.md` before changing visible product behavior.
