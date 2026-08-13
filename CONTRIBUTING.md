# Contributing to MDx

MDx is early. Focused fixes, careful product improvements, tests, and clearer documentation are welcome.

## Before you start

For a small bug or documentation fix, open a pull request directly. For a new subsystem, public API, major dependency, or product-direction change, start with an issue so the shape can be discussed before a large patch exists.

Read `docs/ARCHITECTURE.md` before changing runtime boundaries and `docs/UI-PRODUCT-SURFACE-CONTRACT.md` before changing a product surface.

## Local checks

Install and start MDx with:

```sh
make setup
```

Before opening a pull request, run the checks that match your change:

```sh
make local-smoke
make macos-check       # macOS changes
make ios-build-check   # iOS changes
git diff --check
```

Security-sensitive changes should also run the scanner workflow described in `docs/SECURITY-SCANNER-READINESS.md`.

## Pull requests

Keep pull requests narrow enough to review. Explain why the change exists, what changed, what you validated, and what risk remains. Never include credentials, customer data, personal machine paths, or evidence that implies an external system was tested when it was not.

Generated contracts are committed because they are inspectable product artifacts. When a source and generated artifact change together, keep them in the same pull request and call out the relationship.

Commit titles should use `[area] Clear outcome`. Meaningful commits should include `Changed:` and `Validation:` sections in the body.

By contributing, you agree that your contribution is licensed under Apache 2.0.
