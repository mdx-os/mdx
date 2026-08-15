import XCTest
@testable import MDxWorkbench

final class MacReleaseTests: XCTestCase {
  func testCanaryDistributionExplainsThePrivateUpdateRail() {
    let profile = MacDistributionProfile(channel: "canary")

    XCTAssertTrue(profile.isCanary)
    XCTAssertTrue(profile.updateDetail.contains("install"))
    XCTAssertTrue(profile.packagingSubtitle.contains("signed, notarized"))
  }

  func testLocalDistributionDoesNotClaimNotarization() {
    let profile = MacDistributionProfile(channel: nil)

    XCTAssertFalse(profile.isCanary)
    XCTAssertTrue(profile.updateDetail.contains("repository"))
    XCTAssertTrue(profile.packagingSubtitle.contains("when distribution credentials are ready"))
  }
}
