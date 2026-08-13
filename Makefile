.PHONY: setup dogfood-stack setup-path-check web-build web-smoke rust-check macos-check ios-build-check local-smoke package-posture-check secret-history-check security-audit security-audit-nightly sbom-generate

SECURITY_AUDIT_ARGS ?=
SBOM_ARGS ?=

setup:
	sh ./scripts/setup.sh

dogfood-stack:
	sh ./scripts/dogfood-stack.sh

setup-path-check:
	node ./scripts/setup-path-check.mjs

web-build:
	pnpm --dir apps/mdx-host build

web-smoke:
	pnpm --dir apps/mdx-host smoke

rust-check:
	cargo check --workspace --all-targets

macos-check:
	swift test --package-path apps/mdx-operator-macos

ios-build-check:
	xcodebuild -project apps/mdx-operator-ios/MDxAnywhere.xcodeproj -scheme MDxAnywhere -destination "generic/platform=iOS Simulator" CODE_SIGNING_ALLOWED=NO build

local-smoke: setup-path-check web-build web-smoke rust-check

package-posture-check:
	sh ./scripts/package-posture-check.sh

secret-history-check:
	sh ./scripts/secret-history-check.sh

security-audit:
	node ./scripts/security-audit.mjs --no-trust-publish $(SECURITY_AUDIT_ARGS)

security-audit-nightly:
	node ./scripts/security-audit.mjs --nightly --no-trust-publish $(SECURITY_AUDIT_ARGS)

sbom-generate:
	node ./scripts/sbom-generate.mjs $(SBOM_ARGS)
