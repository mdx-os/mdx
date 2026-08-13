import Foundation
import MDxOperatorShared

func require(_ condition: @autoclosure () -> Bool, _ message: String) {
  guard condition() else {
    FileHandle.standardError.write(Data("shared_contract_check: FAIL \(message)\n".utf8))
    exit(1)
  }
}

let requiredFixtures: Set<String> = [
  "loading", "empty", "offline", "active", "needs_you", "review_ready", "error",
]
require(Set(ForgeAnywhereFixture.allCases.map(\.rawValue)) == requiredFixtures, "fixture states")
require(ForgeAnywhereFixture.offline.snapshot.connection == .offline, "offline state")
require(ForgeAnywhereFixture.needsYou.snapshot.attention.count == 1, "needs-you state")
require(ForgeAnywhereFixture.reviewReady.snapshot.reviews.count == 1, "review-ready state")

for fixture in ForgeAnywhereFixture.allCases {
  for session in fixture.snapshot.sessions {
    require(!session.executionAuthorityOpen, "fixture execution authority")
    require(!session.deploymentAuthorityOpen, "fixture deployment authority")
  }
}

guard let session = ForgeAnywhereFixture.active.snapshot.sessions.first else {
  require(false, "active session fixture")
  exit(1)
}
let encoder = JSONEncoder()
encoder.dateEncodingStrategy = .iso8601
let data = try encoder.encode(session)
let json = String(decoding: data, as: UTF8.self)
require(json.contains("\"session_id\""), "session_id wire key")
require(json.contains("\"active_target\""), "active_target wire key")
require(
  json.contains("\"execution_authority_open\":false"), "closed execution authority wire value")
require(
  json.contains("\"deployment_authority_open\":false"), "closed deployment authority wire value")

let decoder = JSONDecoder()
decoder.dateDecodingStrategy = .iso8601
let decoded = try decoder.decode(ForgeWorkSession.self, from: data)
require(decoded == session, "session round trip")

let command = MobileSessionCommand(
  commandID: "mobile_command_contract_check",
  idempotencyKey: "mobile_idempotency_contract_check",
  sessionID: session.sessionID,
  expectedSessionVersion: session.sessionVersion,
  tenantID: session.tenantID,
  actorUserID: session.ownerUserID,
  actorDeviceID: "iphone_contract_check",
  target: session.activeTarget,
  kind: "follow_up",
  payload: MobileCommandPayload(instruction: "Run the focused proof next")
)
let commandData = try encoder.encode(command)
let commandJSON = String(decoding: commandData, as: UTF8.self)
require(commandJSON.contains("\"expected_session_version\""), "command freshness wire key")
require(commandJSON.contains("\"idempotency_key\""), "command idempotency wire key")
require(
  commandJSON.contains("\"grants_execution_authority\":false"),
  "command cannot grant execution authority")

let challengeJSON = Data(
  """
  {
    "challenge_id":"challenge_1",
    "tenant_id":"tenant_mdx",
    "user_id":"founder",
    "device_id":"iphone_founder",
    "host_id":"mac_founder",
    "host_public_key_thumbprint":"host_thumbprint_123456",
    "challenge_token":"signed-token",
    "signature":"signature",
    "server_public_key":"server-key",
    "issued_at_epoch":4102444740,
    "expires_at_epoch":4102444800,
    "single_use":true
  }
  """.utf8)
let challenge = try decoder.decode(MobilePairingChallenge.self, from: challengeJSON)
let pairingPayload = try MobilePairingPayload.encode(challenge)
let decodedChallenge = try MobilePairingPayload.decode(pairingPayload)
require(decodedChallenge == challenge, "pairing payload round trip")
require(challenge.singleUse, "pairing challenge is single use")

let cloudSetupJSON = Data(
  """
  {
    "status":"OK",
    "tenant_id":"tenant_mdx",
    "installations":[{
      "installation_id":42,
      "account_login":"mdx-labs",
      "account_type":"Organization",
      "status":"active",
      "receipt_id":"receipt_installation_42",
      "token_recorded":false
    }],
    "repositories":[{
      "repository_id":77,
      "full_name":"mdx-labs/native",
      "default_branch":"trunk",
      "repo_id":"github_77",
      "source_revision":"0123456789abcdef0123456789abcdef01234567",
      "status":"connected",
      "receipt_id":"receipt_repo_77",
      "token_recorded":false
    }],
    "environments":[],
    "ready_environment_count":0,
    "github_app_configured":true,
    "hosted_sandbox_status":"NOT_YET_VERIFIED",
    "secret_values_included":false,
    "production_write_allowed":false
  }
  """.utf8)
let cloudSetup = try decoder.decode(MobileCloudSetupProjection.self, from: cloudSetupJSON)
require(cloudSetup.installations.first?.accountLogin == "mdx-labs", "cloud installation decode")
require(cloudSetup.repositories.first?.repoID == "github_77", "cloud repository decode")
require(cloudSetup.repositories.first?.defaultBranch == "trunk", "cloud default branch decode")
require(!cloudSetup.secretValuesIncluded, "cloud setup excludes secret values")
require(!cloudSetup.productionWriteAllowed, "cloud setup does not grant write authority")

let connector = try HostConnectorConfiguration(
  relayURL: URL(string: "wss://relay.mdx.example/v1/hosts")!,
  hostID: "mac_founder",
  credentialRef: "keychain:host-relay",
  credentialExpiresAt: Date().addingTimeInterval(60)
)
require(!connector.acceptsInboundConnections, "host connector is outbound only")
do {
  _ = try HostConnectorConfiguration(
    relayURL: URL(string: "ws://relay.mdx.example/v1/hosts")!,
    hostID: "mac_founder",
    credentialRef: "keychain:host-relay",
    credentialExpiresAt: Date().addingTimeInterval(60)
  )
  require(false, "insecure relay was accepted")
} catch HostConnectorConfigurationError.secureRelayRequired {
  // Expected.
}

print(
  "shared_contract_check: OK fixtures=7 authority=closed pairing=single-use host=outbound-only cloud=github-app"
)
