# MDx

**A workspace for doing real company work with AI, without giving up control.**

MDx brings AI specialists, company context, governed actions, and receipts into
one inspectable system. Run it locally, connect the model you choose, and keep
the final say.

![Build with agents. Keep the final say.](apps/mdx-host/static/og.png)

## Wait, is this another AI chat?

Not really.

Chat is one surface. MDx is an attempt to answer a larger question: what should
an AI-native workspace look like when authority, evidence, memory, and human
judgment are product features rather than afterthoughts?

- **Twin** is the home surface for grounded work with the model you connect.
- **Forge** turns scoped software work into plans, changes, and receipts.
- **Pages and Message** carry local knowledge, citations, and handoffs.
- **Web, macOS, and iOS source** share the same governed core.

## Try it locally

```sh
make setup
```

That command checks prerequisites, starts the canonical local path, and opens
the first-run flow. It does not install tools or require Docker or Postgres.

For prerequisites and alternatives, read the
[quickstart](docs/QUICKSTART.md).

> MDx is very early. The local web path works and is tested. Signed macOS and
> iOS distribution is still being prepared, and several external integrations
> remain unproven.

MDx marks unproven cloud, identity, invite, deploy, and production-write paths
as `PENDING-LIVE-RUN`. A scaffold is not the same thing as a proven capability.

## Why open source?

A system that mediates company work should be inspectable.

You should be able to see how authority works, understand what an agent did,
and change the system when its defaults do not fit you. If MDx goes in a
direction you dislike, fork it. Steal the useful parts. Build the version you
want.

MDx is licensed under [Apache 2.0](LICENSE).

## Start exploring

- [Documentation](docs/README.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Contributing](CONTRIBUTING.md)
- [Security](SECURITY.md)
- [Product direction](docs/UI-PRODUCT-NORTH-STAR.md)

MDx was incubated privately. This repository begins from a deliberately clean
public snapshot, and public development continues here from that release
forward.
