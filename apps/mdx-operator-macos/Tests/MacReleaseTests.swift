import XCTest
@testable import MDxWorkbench

final class MacReleaseTests: XCTestCase {
  func testSemanticVersionTakesPriorityOverBuildNumber() {
    let release = MacReleaseManifest(
      version: "0.10.0",
      build: "1",
      sha256: String(repeating: "a", count: 64),
      sizeBytes: 42,
      notarizedAt: "2026-07-20T21:00:00Z"
    )
    XCTAssertTrue(release.isNewer(thanVersion: "0.9.9", build: "999"))
    XCTAssertFalse(release.isNewer(thanVersion: "0.11.0", build: "1"))
  }

  func testBuildNumberBreaksAnEqualVersionTie() {
    let release = MacReleaseManifest(
      version: "0.9.2",
      build: "2",
      sha256: String(repeating: "b", count: 64),
      sizeBytes: 42,
      notarizedAt: "2026-07-20T21:00:00Z"
    )
    XCTAssertTrue(release.isNewer(thanVersion: "0.9.2", build: "1"))
    XCTAssertFalse(release.isNewer(thanVersion: "0.9.2", build: "2"))
  }

  func testCanaryDistributionExplainsThePrivateUpdateRail() {
    let profile = MacDistributionProfile(channel: "canary")

    XCTAssertTrue(profile.isCanary)
    XCTAssertTrue(profile.updateDetail.contains("private canary release"))
    XCTAssertTrue(profile.packagingSubtitle.contains("signed, notarized"))
  }

  func testLocalDistributionDoesNotClaimNotarization() {
    let profile = MacDistributionProfile(channel: nil)

    XCTAssertFalse(profile.isCanary)
    XCTAssertTrue(profile.updateDetail.contains("repository"))
    XCTAssertTrue(profile.packagingSubtitle.contains("when distribution credentials are ready"))
  }
}
