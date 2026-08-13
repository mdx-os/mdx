import Foundation

final class ParityFixtureURLProtocol: URLProtocol {
  static let baseURL = URL(string: "http://127.0.0.1:18891")!
  static let hostedBaseURL = URL(string: "https://cohort-zero.invalid")!

  static func makeSession() -> URLSession {
    let configuration = URLSessionConfiguration.ephemeral
    configuration.protocolClasses = [ParityFixtureURLProtocol.self]
    return URLSession(configuration: configuration)
  }

  override class func canInit(with request: URLRequest) -> Bool {
    request.url?.host == baseURL.host || request.url?.host == hostedBaseURL.host
  }

  override class func canonicalRequest(for request: URLRequest) -> URLRequest {
    request
  }

  override func startLoading() {
    guard let url = request.url else {
      client?.urlProtocol(self, didFailWithError: URLError(.badURL))
      return
    }

    do {
      let body = request.httpBody.flatMap { String(data: $0, encoding: .utf8) } ?? ""
      let data = try Self.responseData(for: url.path, body: body)
      let response = HTTPURLResponse(
        url: url,
        statusCode: 200,
        httpVersion: "HTTP/1.1",
        headerFields: ["Content-Type": "application/json"]
      )!
      client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
      client?.urlProtocol(self, didLoad: data)
      client?.urlProtocolDidFinishLoading(self)
    } catch FixtureError.notFound {
      let response = HTTPURLResponse(
        url: url,
        statusCode: 404,
        httpVersion: "HTTP/1.1",
        headerFields: ["Content-Type": "application/json"]
      )!
      client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
      client?.urlProtocol(self, didLoad: Data("{}".utf8))
      client?.urlProtocolDidFinishLoading(self)
    } catch {
      client?.urlProtocol(self, didFailWithError: error)
    }
  }

  override func stopLoading() {}

  private enum FixtureError: Error {
    case notFound
  }

  private static func responseData(for path: String, body: String) throws -> Data {
    switch path {
    case "/local/auth-session.json":
      return try jsonData([
        "name": "mdx-auth-session-boundary",
        "source": "verified_trusted_session",
        "status": "VERIFIED_TRUSTED_SESSION",
        "verified_session": [
          "tenant_id": "tenant_beta",
          "user_id": "00000000-0000-4000-8000-000000000001",
          "role": "operator",
          "actor_kind": "human",
          "subject_actor_id": "00000000-0000-4000-8000-000000000001",
          "authority_scope": "governed_write_commands",
        ],
        "roles": ["owner", "operator", "auditor", "viewer"],
        "actor_admission": [
          "status": "VERIFIED-TRUSTED-SESSION",
          "scope": "governed_write_commands",
        ],
      ])
    case "/harness/session.json":
      return try jsonData([
        "operator_session": [
          "safe_controls": ["inspect"],
          "blocked_controls": ["deploy"],
        ],
        "authority_boundaries": [
          "status": "FAIL_CLOSED",
          "blocked_authorities": ["production_write"],
        ],
      ])
    case "/forge/runtime-projection.json":
      return try jsonData([
        "forge_runtime": [
          "current_signal": "Deterministic macOS parity fixture is reachable.",
          "safe_next_move": "Inspect the fixture-backed operator snapshot.",
          "boundary": "Fixture reads only. No execution or production writes.",
          "queue_pressure_status": "ready",
          "authority": "none",
        ]
      ])
    case "/forge/runs/projection.json":
      return try fixtureData("forge-runs.json")
    case "/forge/long-horizon-missions/projection.json":
      return try fixtureData("missions.json")
    case "/pages.json":
      return try jsonData([
        "documents": [
          [
            "document_id": "page_fixture_1",
            "title": "Fixture page",
            "body_ref": "fixture/page.md",
            "author_actor_id": "fixture_operator",
            "visibility": "private",
            "revision_id": "revision_fixture_1",
            "source_receipt_ids": ["receipt_fixture_1"],
            "charter_evidence_ids": [],
          ]
        ]
      ])
    case "/pages/lifecycle/projection.json":
      return try jsonData([
        "documents": [
          [
            "document_id": "page_fixture_1",
            "state": "draft",
            "trust": [
              "owner": "fixture_operator",
              "reviewer": "",
              "trusted_for_ai": false,
              "charter_backed": false,
              "evidence_count": 1,
            ],
            "freshness": ["stale": false, "review_interval_days": 30],
            "events": [],
          ]
        ]
      ])
    case "/messages/activity/projection.json":
      return try fixtureData("activity.json")
    case "/messages/thread-messages/projection.json":
      return try jsonData([
        "messages": [
          [
            "thread_id": "thread_fixture",
            "channel_id": "local-ops",
            "message_id": "message_fixture_1",
            "message_receipt_id": "message_receipt_fixture_1",
            "message_receipt_kind": "message.human.posted",
            "actor_id": "fixture_operator",
            "actor_type": "human",
            "actor_display_name": "Fixture operator",
            "body": "Fixture channel message",
            "sequence": 1,
            "created_at": "2026-01-01T00:00:00Z",
          ]
        ]
      ])
    case "/marketplace/capabilities.json":
      return try fixtureData("capabilities.json")
    case "/marketplace/packs.json":
      return try repositoryData("generated/marketplace/packs.json")
    case "/learning/memory-activations/projection.json":
      return try jsonData([
        "active_memories": [[
          "active_memory_receipt_id": "activation_receipt_fixture_1",
          "active_memory_id": "active_memory_fixture_1",
          "memory_promotion_receipt_id": "promotion_receipt_fixture_1",
          "judgment_decision_receipt_id": "judgment_receipt_fixture_1",
          "target_type": "work_type",
          "lesson_summary": "Keep the first viewport focused on the next human decision.",
          "activation_basis": "A person approved this lesson after reviewing the evidence.",
          "rollback_plan": "Retire the lesson when the product contract changes.",
          "review_owner": "fixture_operator",
          "active_memory_state": "ACTIVE",
        ]]
      ])
    case "/learning/adaptation-grants/projection.json":
      return try jsonData([
        "grants": [[
          "activation_receipt_id": "activation_receipt_fixture_1",
          "active_memory_id": "active_memory_fixture_1",
          "open": true,
          "grant_state": "OPEN",
        ]]
      ])
    case "/memory/consolidation-ratifications/projection.json":
      return try jsonData([
        "pending": [[
          "memory_id": "memory_fixture_pending",
          "memory_scope": "team_memory",
          "content": "The team prefers concise release notes with direct proof links.",
          "proposed_by": "fixture_agent",
          "source_receipt_id": "source_receipt_fixture_1",
          "gate_receipt_id": "gate_receipt_fixture_1",
          "consolidation_state": "PENDING_RATIFICATION",
        ]],
        "ratifications": [[
          "memory_id": "memory_fixture_kept",
          "ratification_receipt_id": "ratification_receipt_fixture_1",
          "ratification_decision": "APPROVE",
          "consolidation_state": "RATIFIED",
          "proposed_by": "fixture_agent",
          "ratified_by": "fixture_operator",
          "note": "Keep product claims grounded in receipts.",
        ]]
      ])
    case "/forge/model-scorecard.json":
      return try jsonData([
        "by_work_type": [[
          "model": "fixture-model",
          "work_type": "product_ux",
          "runs": 8,
          "done": 7,
          "done_rate": 0.875,
          "avg_turns": 4.5,
        ]]
      ])
    case "/learning/memory-supersedes.json":
      return try jsonData([
        "status": "RECORDED",
        "memory_superseded": true,
        "memory_supersede_receipt_id": "supersede_receipt_fixture_1",
        "reason": "",
      ])
    default:
      throw FixtureError.notFound
    }
  }

  private static func fixtureData(_ name: String) throws -> Data {
    try repositoryData("apps/mdx-operator-macos/Tests/Fixtures/\(name)")
  }

  private static func repositoryData(_ path: String) throws -> Data {
    try Data(contentsOf: repositoryRoot.appendingPathComponent(path))
  }

  private static func jsonData(_ object: Any) throws -> Data {
    try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
  }

  private static var repositoryRoot: URL {
    URL(fileURLWithPath: #filePath)
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
  }
}
