# macOS distribution

The macOS client is built from `apps/mdx-operator-macos` as a Swift package. Local contributors can validate it with:

```sh
make macos-check
```

The release workflow in `.github/workflows/macos-canary-release.yml` packages, signs, notarizes, and uploads a canary build when it is manually dispatched. It requires Apple signing and notarization credentials plus the release storage configuration declared by the workflow.

Release credentials are never stored in the repository. The workflow imports them into an ephemeral keychain and removes temporary signing material at the end of the job.

Before treating a build as distributable, verify the workflow output, notarization evidence, artifact checksum, update manifest, and a clean install on another Mac. A successful local Swift build does not prove signing, notarization, upload, or update delivery.
