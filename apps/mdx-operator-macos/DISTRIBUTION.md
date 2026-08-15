# Distributing the MDx macOS app

The macOS client supports three distribution rungs. Each rung is explicit, and
the packaging scripts do not claim a higher level of trust than they can prove.

## Local development

Run the native checks with:

```sh
make macos-check
```

`script/package_release.sh` builds and versions `dist/MDx.app`. It prefers a
stable local code-signing identity and falls back to ad-hoc signing when one is
not available. Ad-hoc signing is suitable for local development, but macOS may
treat every rebuild as a different app for permissions and notifications.

## Direct distribution

The manually dispatched workflow in
`.github/workflows/macos-canary-release.yml` builds the app, signs it with a
Developer ID Application certificate, submits it to Apple's notary service,
staples the accepted ticket, and verifies the result with Gatekeeper.

It publishes two artifacts from the same accepted app bundle:

- `MDx.dmg` for the first install
- `MDx.zip` for authenticated Sparkle updates

The workflow also writes a manifest containing the version, build, size,
SHA-256, and Sparkle signature. Release credentials belong in the protected
GitHub environment and must never be committed to the repository.

## Update delivery

Canary builds embed Sparkle. After an invited user signs in, the app requests
the authenticated appcast at `/download/macos/appcast.xml`. The appcast points
to the same-origin `/download/macos/update.zip` route, and Sparkle verifies the
EdDSA signature before replacing and reopening the app.

The website's authenticated DMG download remains the recovery path. The host
uses short-lived storage URLs and does not expose storage credentials or
permanent release URLs to clients.

## Release verification

Before treating a build as distributable, verify all of the following:

1. Apple accepted both the app archive and the DMG.
2. `codesign`, `stapler`, and `spctl` accept the resulting artifacts.
3. The published manifest matches the DMG and ZIP checksums.
4. A clean Mac can install the DMG, launch MDx, and authenticate.
5. An installed older canary can discover, install, and relaunch into the new
   version through Sparkle.

A successful local Swift build does not prove signing, notarization, upload, or
update delivery.
