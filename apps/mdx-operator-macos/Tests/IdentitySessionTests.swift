import XCTest
@testable import MDxWorkbench

final class IdentitySessionTests: XCTestCase {
  func testVerifiedHostedIdentityWinsOverLocalFallback() async {
    let client = MDxRouteClient(session: ParityFixtureURLProtocol.makeSession())
    let result = await client.loadIdentity(baseURL: ParityFixtureURLProtocol.baseURL)

    XCTAssertEqual(result.session?.tenantID, "tenant_beta")
    XCTAssertEqual(result.session?.userID, "00000000-0000-4000-8000-000000000001")
    XCTAssertEqual(result.session?.role, "operator")
    XCTAssertEqual(result.session?.admissionStatus, "VERIFIED-TRUSTED-SESSION")
    XCTAssertEqual(result.clearance?.status, "FAIL_CLOSED")
  }

  func testHostedIdentityDoesNotProbeLocalHarnessClearance() async {
    let client = MDxRouteClient(session: ParityFixtureURLProtocol.makeSession())
    let result = await client.loadIdentity(baseURL: ParityFixtureURLProtocol.hostedBaseURL)

    XCTAssertEqual(result.session?.tenantID, "tenant_beta")
    XCTAssertNil(result.clearance)
  }

  func testOpaqueHostedUserIDNeverBecomesVisibleAccountCopy() {
    let session = IdentitySession(
      userID: "00000000-0000-4000-8000-000000000001",
      tenantID: "tenant_beta",
      role: "operator",
      expiresAt: "",
      roles: ["owner", "operator", "auditor", "viewer"],
      admissionScope: "governed_write_commands",
      admissionStatus: "VERIFIED-TRUSTED-SESSION"
    )

    XCTAssertEqual(session.displayName, "You")
    XCTAssertEqual(session.roleLabel, "Operator")
  }

  func testHumanReadableLocalFixtureStillHasAUsefulName() {
    let session = IdentitySession(
      userID: "local_user",
      tenantID: "local_tenant",
      role: "owner",
      expiresAt: "",
      roles: ["owner"],
      admissionScope: "",
      admissionStatus: ""
    )

    XCTAssertEqual(session.displayName, "Local User")
  }
}
