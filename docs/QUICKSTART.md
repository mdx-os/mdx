# Quickstart

Run MDx locally with one command:

```sh
make setup
```

The setup script checks prerequisites, installs the JavaScript workspace from the lockfile, builds the Rust kernel, starts the local web app, and opens the first-run flow. It does not install system toolchains for you.

## Prerequisites

- Rust with `cargo`: <https://rustup.rs>
- Node.js 22 or newer
- pnpm 9.15.9, normally enabled through Corepack

Docker and Postgres are not required for the default local path. Local workspace data is snapshot-backed under `.mdx-local/`, which is ignored by Git.

## Run the checks

```sh
make local-smoke
```

Native client checks are separate:

```sh
make macos-check
make ios-build-check
```

## Production-shaped local infrastructure

`docker-compose.yml` is provided for people working on the Postgres-backed deployment path. It is optional and intentionally separate from `make setup`. See `deploy/README.md` before using it.

## Model providers

The local shell can run without a live model provider. To connect one, create `.mdx-local/provider.env` yourself and restart the stack. This file is ignored by Git and its values should never be pasted into issues, commits, or transcripts.

The current local provider path is documented by the setup output. Provider, identity, cloud, and production-write behavior should be treated as unproven until you have exercised it with your own credentials and environment.
