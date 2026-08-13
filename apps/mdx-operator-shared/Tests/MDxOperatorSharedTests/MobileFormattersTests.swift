import XCTest
@testable import MDxOperatorShared

final class MobileFormattersTests: XCTestCase {
  func testWorkingRevisionsReadAsLocalBranch() {
    XCTAssertEqual(MobileFormatters.shortRevision(""), "Local branch")
    XCTAssertEqual(MobileFormatters.shortRevision("pending"), "Local branch")
    XCTAssertEqual(MobileFormatters.shortRevision("working-copy"), "Local branch")
  }

  func testRecordedRevisionKeepsACompactIdentifier() {
    XCTAssertEqual(
      MobileFormatters.shortRevision("0123456789abcdef"),
      "01234567"
    )
  }
}
