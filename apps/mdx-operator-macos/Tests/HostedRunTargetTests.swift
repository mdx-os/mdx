import Foundation
import XCTest
@testable import MDxWorkbench

final class HostedRunTargetTests: XCTestCase {
  override func setUp() {
    super.setUp()
    HostedRunTargetURLProtocol.capture.reset()
  }

  func testHostedRunBindsVerifiedCloudEnvironment() async throws {
    let client = MDxRouteClient(
      session: HostedRunTargetURLProtocol.makeSession(),
      accessTokenProvider: { "fixture-access-token" }
    )

    let outcome = try await client.startRun(
      baseURL: HostedRunTargetURLProtocol.hostedBaseURL,
      intent: "Prove the classnames edge case.",
      repoID: "classnames",
      fleetWidth: 1,
      allowedCommands: ["npm test"],
      actorID: "human:fixture"
    )

    let body = try XCTUnwrap(HostedRunTargetURLProtocol.capture.lastBody(for: "/forge/runs.json"))
    XCTAssertEqual(body["execution_backend"] as? String, "hosted_sandbox")
    XCTAssertEqual(body["cloud_environment_id"] as? String, "cloud_env_classnames")
    XCTAssertEqual(body["repo_id"] as? String, "classnames")
    XCTAssertEqual(body["allowed_commands"] as? [String], ["npm test"])
    XCTAssertNil(body["actor_id"], "Hosted authority comes from the bearer session")
    XCTAssertEqual(outcome.receiptID, "forge_run_cloud_fixture")
    XCTAssertEqual(
      HostedRunTargetURLProtocol.capture.requestCount(for: "/mobile/handoff-targets.json"),
      1
    )
  }

  func testLocalRunKeepsLocalExecutionContract() async throws {
    let client = MDxRouteClient(session: HostedRunTargetURLProtocol.makeSession())

    _ = try await client.startRun(
      baseURL: HostedRunTargetURLProtocol.localBaseURL,
      intent: "Prove the local fixture.",
      repoID: "classnames",
      fleetWidth: 1,
      allowedCommands: ["npm test"],
      actorID: "human:local"
    )

    let body = try XCTUnwrap(HostedRunTargetURLProtocol.capture.lastBody(for: "/forge/runs.json"))
    XCTAssertNil(body["execution_backend"])
    XCTAssertNil(body["cloud_environment_id"])
    XCTAssertEqual(body["actor_id"] as? String, "human:local")
    XCTAssertEqual(
      HostedRunTargetURLProtocol.capture.requestCount(for: "/mobile/handoff-targets.json"),
      0
    )
  }
}

private final class HostedRunTargetCapture: @unchecked Sendable {
  private let lock = NSLock()
  private var requests: [(path: String, body: [String: Any]?)] = []

  func reset() {
    lock.lock()
    requests = []
    lock.unlock()
  }

  func record(_ request: URLRequest) {
    let body = Self.bodyData(from: request).flatMap {
      try? JSONSerialization.jsonObject(with: $0) as? [String: Any]
    }
    lock.lock()
    requests.append((request.url?.path ?? "", body))
    lock.unlock()
  }

  private static func bodyData(from request: URLRequest) -> Data? {
    if let body = request.httpBody { return body }
    guard let stream = request.httpBodyStream else { return nil }
    stream.open()
    defer { stream.close() }
    var data = Data()
    var buffer = [UInt8](repeating: 0, count: 4_096)
    while stream.hasBytesAvailable {
      let count = stream.read(&buffer, maxLength: buffer.count)
      if count <= 0 { break }
      data.append(buffer, count: count)
    }
    return data.isEmpty ? nil : data
  }

  func lastBody(for path: String) -> [String: Any]? {
    lock.lock()
    defer { lock.unlock() }
    return requests.last(where: { $0.path == path })?.body
  }

  func requestCount(for path: String) -> Int {
    lock.lock()
    defer { lock.unlock() }
    return requests.count(where: { $0.path == path })
  }
}

private final class HostedRunTargetURLProtocol: URLProtocol {
  static let hostedBaseURL = URL(string: "https://hosted-run-target.invalid")!
  static let localBaseURL = URL(string: "http://127.0.0.1:18892")!
  static let capture = HostedRunTargetCapture()

  static func makeSession() -> URLSession {
    let configuration = URLSessionConfiguration.ephemeral
    configuration.protocolClasses = [HostedRunTargetURLProtocol.self]
    return URLSession(configuration: configuration)
  }

  override class func canInit(with request: URLRequest) -> Bool {
    request.url?.host == hostedBaseURL.host || request.url?.host == localBaseURL.host
  }

  override class func canonicalRequest(for request: URLRequest) -> URLRequest {
    request
  }

  override func startLoading() {
    guard let url = request.url else {
      client?.urlProtocol(self, didFailWithError: URLError(.badURL))
      return
    }
    Self.capture.record(request)
    let object: [String: Any]
    switch url.path {
    case "/forge/runs/projection.json":
      object = ["runs": []]
    case "/mobile/handoff-targets.json":
      object = [
        "cloud_environments": [[
          "environment_id": "cloud_env_classnames",
          "repository_id": "classnames",
          "verified": true
        ]]
      ]
    case "/forge/runs.json":
      object = [
        "status": "ACCEPTED",
        "run_id": "forge_run_cloud_fixture",
        "run_started_receipt_id": "receipt_cloud_fixture"
      ]
    default:
      respond(status: 404, data: Data("{}".utf8), url: url)
      return
    }
    let data = (try? JSONSerialization.data(withJSONObject: object)) ?? Data("{}".utf8)
    respond(status: 200, data: data, url: url)
  }

  override func stopLoading() {}

  private func respond(status: Int, data: Data, url: URL) {
    let response = HTTPURLResponse(
      url: url,
      statusCode: status,
      httpVersion: "HTTP/1.1",
      headerFields: ["Content-Type": "application/json"]
    )!
    client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
    client?.urlProtocol(self, didLoad: data)
    client?.urlProtocolDidFinishLoading(self)
  }
}
