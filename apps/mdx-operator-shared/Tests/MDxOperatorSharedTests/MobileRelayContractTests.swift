import Foundation
import Testing

@testable import MDxOperatorShared

@Test func canonicalRelayEventRoundTripsWithOnlyPinnedSchemaKeys() throws {
  let data = Data(
    #"{"schema_version":1,"event_id":"forge_run_one:7","session_id":"forge_run_one","session_version":7,"sequence":7,"tenant_id":"tenant_one","target":{"kind":"paired_host","target_id":"host_one","tenant_id":"tenant_one","display_name":"Paired Mac","capability_revision":1},"kind":"review_ready","receipt_id":"receipt_7","safe_summary":"The focused diff is ready","evidence_refs":["receipt_7"],"occurred_at":"2026-07-13T12:00:00Z","redaction_status":"not_required","contains_secret_values":false,"grants_authority":false}"#
      .utf8
  )
  let event = try JSONDecoder().decode(MobileRelayEnvelope.self, from: data)
  #expect(event.sessionVersion == 7)
  #expect(event.target.targetID == "host_one")
  #expect(event.kind == "review_ready")
  #expect(event.state == "review_ready")
  #expect(event.stage == "done")
  #expect(event.occurredAt != .distantPast)

  let encoded = try JSONEncoder().encode(event)
  let object = try #require(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
  #expect(
    Set(object.keys) == [
      "schema_version", "event_id", "session_id", "session_version", "sequence", "tenant_id",
      "target", "kind", "receipt_id", "safe_summary", "evidence_refs", "occurred_at",
      "redaction_status", "contains_secret_values", "grants_authority",
    ]
  )
}

@Test func legacyRelayOnlyShapeIsRejected() {
  let legacy = Data(
    #"{"schema_version":1,"event_id":"forge_run_one:7","tenant_id":"tenant_one","session_id":"forge_run_one","sequence":7,"kind":"diff_ready","stage":"done","state":"review_ready","summary":"Ready","occurred_at_epoch_ms":1,"execution_target_kind":"paired_host","execution_target_id":"host_one","contains_secret_values":false,"production_write_allowed":false}"#
      .utf8
  )
  #expect(throws: DecodingError.self) {
    try JSONDecoder().decode(MobileRelayEnvelope.self, from: legacy)
  }
}
